//! Blackmoon's CLI binding to `astrogram::provider`: re-exports the moved types
//! and supplies the printing [`ProgressSink`] used by every command.

use astrogram::provider::ProgressSink;
pub use astrogram::provider::{DatetimeKey, WebProvider};
use std::io::{IsTerminal, Write as _};

/// Prints provider progress to stdout/stderr exactly as the pre-extraction
/// inline code did. Transient write progress is tty-gated.
pub struct CliSink;

impl ProgressSink for CliSink {
    fn phase(&self, msg: &str) {
        eprintln!("{msg}");
    }
    fn count(&self, msg: &str) {
        eprintln!("{msg}");
    }
    fn item_start(&self, i: usize, total: usize, name: &str) {
        eprint!("[{i:>3}/{total}] {name:<40}  ");
        std::io::stderr().flush().ok();
    }
    fn item_result(&self, status: &str) {
        eprintln!("{status}");
    }
    fn write_progress(&self, i: usize, total: usize, name: &str) {
        if std::io::stderr().is_terminal() {
            let w = total.to_string().len();
            eprint!("\r\x1b[Kwriting [{i:0>w$}/{total}] {name}");
            std::io::stderr().flush().ok();
        }
    }
    fn write_error(&self, msg: &str) {
        if std::io::stderr().is_terminal() {
            eprint!("\r\x1b[K");
        }
        eprintln!("{msg}");
    }
    fn write_done(&self) {
        if std::io::stderr().is_terminal() {
            eprint!("\r\x1b[K");
            std::io::stderr().flush().ok();
        }
    }
    fn note(&self, msg: &str) {
        // The caller (astrogram::provider) already embeds the two leading spaces
        // in the note string (e.g. "  3 already in LUNA — skipped").
        // Do NOT add extra spaces here — that would produce four leading spaces.
        eprintln!("{msg}");
    }
}

/// Wraps a [`ProgressSink`] and, when `quiet` is set, no-ops the pure-progress
/// methods (`phase`/`count`/`item_start`/`item_result`/`write_progress`/
/// `write_done`) — the "reading…"/"[i/N] name" narration `--quiet` drops.
///
/// `write_error` and `note` still delegate unconditionally: both report a
/// *data* outcome (a per-record write failure, or a dedup/skip count) rather
/// than routine progress chatter, so they survive `--quiet` per the settled
/// quiet/verbose model.
pub struct QuietAwareSink<S> {
    inner: S,
    quiet: bool,
}

impl<S: ProgressSink> QuietAwareSink<S> {
    /// Wrap `inner`; when `quiet` is true, progress-only methods become no-ops.
    pub fn new(inner: S, quiet: bool) -> Self {
        Self { inner, quiet }
    }
}

impl<S: ProgressSink> ProgressSink for QuietAwareSink<S> {
    fn phase(&self, msg: &str) {
        if !self.quiet {
            self.inner.phase(msg);
        }
    }
    fn count(&self, msg: &str) {
        if !self.quiet {
            self.inner.count(msg);
        }
    }
    fn item_start(&self, i: usize, total: usize, name: &str) {
        if !self.quiet {
            self.inner.item_start(i, total, name);
        }
    }
    fn item_result(&self, status: &str) {
        if !self.quiet {
            self.inner.item_result(status);
        }
    }
    fn write_progress(&self, i: usize, total: usize, name: &str) {
        if !self.quiet {
            self.inner.write_progress(i, total, name);
        }
    }
    fn write_error(&self, msg: &str) {
        // Always delegates — a per-record write failure is an error
        // disclosure, not progress narration; --quiet must not hide it.
        self.inner.write_error(msg);
    }
    fn write_done(&self) {
        if !self.quiet {
            self.inner.write_done();
        }
    }
    fn note(&self, msg: &str) {
        // Always delegates — a dedup/skip count is a data-affecting
        // disclosure, not progress narration; --quiet must not hide it.
        self.inner.note(msg);
    }
}

#[cfg(test)]
mod quiet_aware_sink_tests {
    use super::*;
    use std::cell::RefCell;

    /// Records every call it receives, so tests can assert which methods a
    /// [`QuietAwareSink`] let through without needing to capture real stderr.
    #[derive(Default)]
    struct SpySink {
        calls: RefCell<Vec<String>>,
    }

    impl ProgressSink for SpySink {
        fn phase(&self, msg: &str) {
            self.calls.borrow_mut().push(format!("phase:{msg}"));
        }
        fn count(&self, msg: &str) {
            self.calls.borrow_mut().push(format!("count:{msg}"));
        }
        fn item_start(&self, i: usize, total: usize, name: &str) {
            self.calls
                .borrow_mut()
                .push(format!("item_start:{i}/{total}:{name}"));
        }
        fn item_result(&self, status: &str) {
            self.calls
                .borrow_mut()
                .push(format!("item_result:{status}"));
        }
        fn write_progress(&self, i: usize, total: usize, name: &str) {
            self.calls
                .borrow_mut()
                .push(format!("write_progress:{i}/{total}:{name}"));
        }
        fn write_error(&self, msg: &str) {
            self.calls.borrow_mut().push(format!("write_error:{msg}"));
        }
        fn write_done(&self) {
            self.calls.borrow_mut().push("write_done".to_string());
        }
        fn note(&self, msg: &str) {
            self.calls.borrow_mut().push(format!("note:{msg}"));
        }
    }

    #[test]
    fn quiet_sink_drops_progress_narration() {
        let spy = SpySink::default();
        let sink = QuietAwareSink::new(spy, true);
        sink.phase("reading…");
        sink.count("Found 3 charts");
        sink.item_start(1, 3, "Anna");
        sink.item_result("ok");
        sink.write_progress(1, 3, "Anna");
        sink.write_done();
        assert!(
            sink.inner.calls.borrow().is_empty(),
            "quiet sink must drop all progress narration, got: {:?}",
            sink.inner.calls.borrow()
        );
    }

    #[test]
    fn quiet_sink_still_delegates_write_error_and_note() {
        let spy = SpySink::default();
        let sink = QuietAwareSink::new(spy, true);
        sink.write_error("boom");
        sink.note("  3 already in LUNA — skipped");
        let calls = sink.inner.calls.borrow();
        assert_eq!(
            *calls,
            vec![
                "write_error:boom".to_string(),
                "note:  3 already in LUNA \u{2014} skipped".to_string(),
            ]
        );
    }

    #[test]
    fn non_quiet_sink_delegates_everything() {
        let spy = SpySink::default();
        let sink = QuietAwareSink::new(spy, false);
        sink.phase("reading…");
        sink.count("Found 3 charts");
        sink.item_start(1, 3, "Anna");
        sink.item_result("ok");
        sink.write_progress(1, 3, "Anna");
        sink.write_done();
        sink.write_error("boom");
        sink.note("skipped");
        assert_eq!(sink.inner.calls.borrow().len(), 8);
    }
}
