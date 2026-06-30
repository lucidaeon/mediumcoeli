//! Blackmoon's CLI binding to `astrogram::provider`: re-exports the moved types
//! and supplies the printing [`ProgressSink`] used by every command.

use astrogram::provider::ProgressSink;
pub use astrogram::provider::{DatetimeKey, WebProvider, key};
use std::io::{IsTerminal, Write as _};

/// Prints provider progress to stdout/stderr exactly as the pre-extraction
/// inline code did. Transient write progress is tty-gated.
pub struct CliSink;

impl ProgressSink for CliSink {
    fn phase(&self, msg: &str) {
        println!("{msg}");
    }
    fn count(&self, msg: &str) {
        println!("{msg}");
    }
    fn item_start(&self, i: usize, total: usize, name: &str) {
        print!("[{i:>3}/{total}] {name:<40}  ");
        std::io::stdout().flush().ok();
    }
    fn item_result(&self, status: &str) {
        println!("{status}");
    }
    fn write_progress(&self, i: usize, total: usize, name: &str) {
        if std::io::stdout().is_terminal() {
            let w = total.to_string().len();
            print!("\r\x1b[Kwriting [{i:0>w$}/{total}] {name}");
            std::io::stdout().flush().ok();
        }
    }
    fn write_error(&self, msg: &str) {
        if std::io::stdout().is_terminal() {
            print!("\r\x1b[K");
        }
        println!("{msg}");
    }
    fn write_done(&self) {
        if std::io::stdout().is_terminal() {
            print!("\r\x1b[K");
            std::io::stdout().flush().ok();
        }
    }
    fn note(&self, msg: &str) {
        // The caller (astrogram::provider) already embeds the two leading spaces
        // in the note string (e.g. "  3 already in LUNA — skipped").
        // Do NOT add extra spaces here — that would produce four leading spaces.
        println!("{msg}");
    }
}
