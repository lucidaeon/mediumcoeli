//! Format-agnostic bytes dispatch for reading and writing chart data.
//!
//! [`read_bytes`](crate::convert::read_bytes) and
//! [`write_bytes`](crate::convert::write_bytes) are the single call sites a GUI
//! (or any non-CLI consumer) needs to convert raw bytes to/from
//! [`Chart`](crate::chart::Chart) values.  The path and stdin/stdout I/O —
//! which depend on `std::fs` and terminal handling — remain in the `blackmoon`
//! CLI; this module owns only the format↔bytes dispatch tables.
//!
//! Web formats ([`Luna`](crate::format::Format::Luna),
//! [`Astrocom`](crate::format::Format::Astrocom),
//! [`Astrotheoros`](crate::format::Format::Astrotheoros)) and wrong-direction
//! formats ([`Json`](crate::format::Format::Json) /
//! [`Raw`](crate::format::Format::Raw) on read;
//! [`Aaf`](crate::format::Format::Aaf) on write;
//! [`Jhd`](crate::format::Format::Jhd) on write) return
//! [`UnsupportedDirection`](crate::error::ChartError::UnsupportedDirection).

use crate::chart::Chart;
use crate::error::ChartError;
use crate::format::Format;
use std::path::{Path, PathBuf};

/// Parse `bytes` using `format`'s reader and return the decoded charts.
///
/// # Errors
///
/// Returns [`ChartError::UnsupportedDirection`] when `format` is write-only or
/// a web format.  Returns [`ChartError::Utf8`] when the bytes are not valid
/// UTF-8 and the parser requires text.  Returns [`ChartError::Parse`] for
/// format-level parse errors.
pub fn read_bytes(format: Format, bytes: &[u8]) -> Result<Vec<Chart>, ChartError> {
    match format {
        Format::Sfcht => {
            let (_, charts) =
                crate::sfcht::parse_file(bytes).map_err(|e| ChartError::Parse(e.to_string()))?;
            Ok(charts)
        }
        Format::Zeus => {
            let text = std::str::from_utf8(bytes)?;
            crate::zeus::parse_file(text).map_err(|e| ChartError::Parse(e.to_string()))
        }
        Format::Adb => {
            let text = std::str::from_utf8(bytes)?;
            crate::adbxml::parse_file(text).map_err(|e| ChartError::Parse(e.to_string()))
        }
        Format::Aaf => {
            let text = std::str::from_utf8(bytes)?;
            crate::aaf::parse_file(text).map_err(|e| ChartError::Parse(e.to_string()))
        }
        Format::Jhd => {
            let text = std::str::from_utf8(bytes)?;
            crate::jhd::parse_file(text)
                .map(|c| vec![c])
                .map_err(|e| ChartError::Parse(e.to_string()))
        }
        Format::Luna => Err(ChartError::UnsupportedDirection(
            "use the Luna web provider rather than passing raw bytes",
        )),
        Format::Astrocom => Err(ChartError::UnsupportedDirection(
            "use the Astrocom web provider rather than passing raw bytes",
        )),
        Format::Astrotheoros => Err(ChartError::UnsupportedDirection(
            "use the Astrotheoros web provider rather than passing raw bytes",
        )),
        Format::Json => Err(ChartError::UnsupportedDirection(
            "JZOD (json) is a write-only format; reading is not supported",
        )),
        Format::Raw => Err(ChartError::UnsupportedDirection(
            "raw is a write-only format; reading is not supported",
        )),
    }
}

/// Read a chart file into `Chart`s, naming any chart whose embedded `name` is
/// empty from the file stem. This is the format-agnostic filename→name rule
/// (only formats without an in-payload name, e.g. JHD, are affected).
///
/// # Errors
///
/// Returns [`ChartError::Io`] when the file cannot be read.  Returns
/// [`ChartError::Parse`] for format-level parse errors (see [`read_bytes`]).
pub fn read_path(format: Format, path: &Path) -> Result<Vec<Chart>, ChartError> {
    let bytes = std::fs::read(path).map_err(|e| ChartError::Io {
        path: path.to_path_buf(),
        source: e,
    })?;
    let mut charts = read_bytes(format, &bytes)?;
    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
        for c in &mut charts {
            if c.name.is_empty() {
                c.name = stem.to_string();
            }
        }
    }
    Ok(charts)
}

/// Encode `charts` using `format`'s writer and return the raw bytes.
///
/// `sfcht_description` is forwarded to [`crate::sfcht::write_file_with_description`]
/// as the file-header description field; pass `None` to use the default
/// `"Blackmoon <version>"` string.
///
/// # Errors
///
/// Returns [`ChartError::UnsupportedDirection`] when `format` is read-only or
/// a web format.  Returns [`ChartError::Parse`] for format-level write errors
/// (currently only possible for `SFcht`).
pub fn write_bytes(
    format: Format,
    charts: &[Chart],
    sfcht_description: Option<&str>,
) -> Result<Vec<u8>, ChartError> {
    match format {
        Format::Sfcht => {
            let bytes = crate::sfcht::write_file_with_description(charts, sfcht_description)
                .map_err(|e| ChartError::Parse(e.to_string()))?;
            Ok(bytes)
        }
        Format::Zeus => {
            let text = crate::zeus::write_file(charts);
            Ok(text.into_bytes())
        }
        Format::Adb => {
            let text = crate::adbxml::write_file(charts);
            Ok(text.into_bytes())
        }
        Format::Json => {
            let text = crate::jzod::write_file(charts);
            Ok(text.into_bytes())
        }
        Format::Raw => {
            let text = crate::raw::write_file(charts);
            Ok(text.into_bytes())
        }
        Format::Aaf => Err(ChartError::UnsupportedDirection(
            "AAF is a read-only format; choose a writable output format",
        )),
        Format::Jhd => Err(ChartError::UnsupportedDirection(
            "JHD is a read-only format; choose a writable output format",
        )),
        Format::Luna => Err(ChartError::UnsupportedDirection(
            "use the Luna web provider rather than raw bytes",
        )),
        Format::Astrocom => Err(ChartError::UnsupportedDirection(
            "use the Astrocom web provider rather than raw bytes",
        )),
        Format::Astrotheoros => Err(ChartError::UnsupportedDirection(
            "use the Astrotheoros web provider rather than raw bytes",
        )),
    }
}

/// The chart files discovered under a directory, plus a count of files whose
/// extension no format recognises (skipped as non-chart).
pub struct DirScan {
    /// Recognised chart files, sorted by path.
    pub files: Vec<PathBuf>,
    /// Files skipped because their extension is not a known chart format.
    pub skipped: usize,
}

/// Recursively collect chart files under `dir`. A file is included when
/// [`Format::from_path`] recognises its extension; when `only` is `Some(f)`,
/// only that format's files are kept (other recognised formats are excluded but
/// not counted as skipped junk). Directory symlinks are not followed, but file
/// symlinks are followed. The returned `files` are sorted by path.
///
/// # Errors
///
/// Returns [`ChartError::Io`] on an I/O failure while reading the tree.
pub fn chart_files_under(dir: &Path, only: Option<Format>) -> Result<DirScan, ChartError> {
    let mut files = Vec::new();
    let mut skipped = 0usize;
    collect_chart_files(dir, only, &mut files, &mut skipped)?;
    files.sort();
    Ok(DirScan { files, skipped })
}

fn collect_chart_files(
    dir: &Path,
    only: Option<Format>,
    files: &mut Vec<PathBuf>,
    skipped: &mut usize,
) -> Result<(), ChartError> {
    let entries = std::fs::read_dir(dir).map_err(|e| ChartError::Io {
        path: dir.to_path_buf(),
        source: e,
    })?;
    for entry in entries {
        let entry = entry.map_err(|e| ChartError::Io {
            path: dir.to_path_buf(),
            source: e,
        })?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(|e| ChartError::Io {
            path: path.clone(),
            source: e,
        })?;
        if file_type.is_dir() {
            // A symlinked directory reports is_symlink(), not is_dir(), so this
            // descends real subdirectories only.
            collect_chart_files(&path, only, files, skipped)?;
        } else {
            // Regular file, or a symlink. metadata() follows the link: a symlink to
            // a chart file is read, a symlink to a directory is NOT recursed (cycle
            // safety), and a broken symlink is skipped.
            let is_file = if file_type.is_symlink() {
                std::fs::metadata(&path).is_ok_and(|m| m.is_file())
            } else {
                file_type.is_file()
            };
            if is_file {
                match Format::from_path(&path) {
                    Some(f) if only.is_none_or(|o| o == f) => files.push(path),
                    Some(_) => {} // recognised chart, excluded by `only` — not junk
                    None => *skipped += 1,
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_charts() -> Vec<Chart> {
        // Anna Freud — reference data from skills/astrologer/fixtures/ref_anna_freud_alcabitius.md.
        // Uses test_support::fully_populated() which is cfg(test)-only inside
        // the test_support module; we call it directly here since we are also
        // cfg(test).
        vec![crate::test_support::fully_populated()]
    }

    #[test]
    fn write_then_read_zeus_roundtrips_count() {
        let charts = sample_charts();
        let bytes = write_bytes(Format::Zeus, &charts, None).unwrap();
        let back = read_bytes(Format::Zeus, &bytes).unwrap();
        assert_eq!(back.len(), charts.len());
    }

    #[test]
    fn read_write_only_format_errors() {
        assert!(read_bytes(Format::Json, b"{}").is_err());
    }

    #[test]
    fn jhd_reads_through_convert_with_empty_name() {
        let bytes = b"3\r\n21\r\n1990\r\n6.0\r\n-5.30\r\n-75.0\r\n15.0\r\n";
        let charts = read_bytes(Format::Jhd, bytes).unwrap();
        assert_eq!(charts.len(), 1);
        assert!(charts[0].name.is_empty());
        assert!((charts[0].tz_offset_hours - 5.5).abs() < 1e-9);
    }

    #[test]
    fn read_path_names_jhd_chart_from_file_stem() {
        let dir = tempdir::TempDir::new("jhd").unwrap();
        let p = dir.path().join("Mahatma Gandhi.jhd");
        std::fs::write(
            &p,
            b"10\r\n2\r\n1869\r\n7.2\r\n-4.39\r\n-69.49\r\n21.37\r\n",
        )
        .unwrap();
        let charts = read_path(Format::Jhd, &p).unwrap();
        assert_eq!(charts[0].name, "Mahatma Gandhi");
    }

    #[test]
    fn chart_files_under_recurses_filters_and_sorts() {
        let root = tempdir::TempDir::new("scan").unwrap();
        let r = root.path();
        // top-level chart files + junk
        std::fs::write(r.join("b.jhd"), b"").unwrap();
        std::fs::write(r.join("a.jhd"), b"").unwrap();
        std::fs::write(r.join("notes.txt"), b"").unwrap(); // junk
        std::fs::write(r.join(".DS_Store"), b"").unwrap(); // junk (no ext)
        // nested subdir with a chart file of a different recognized format
        let sub = r.join("sub");
        std::fs::create_dir(&sub).unwrap();
        std::fs::write(sub.join("c.aaf"), b"").unwrap();

        // No filter: all recognized formats, recursive, sorted, junk counted.
        let scan = chart_files_under(r, None).unwrap();
        let names: Vec<String> = scan
            .files
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["a.jhd", "b.jhd", "c.aaf"]); // sorted by full path; a/b top-level < sub/c
        assert_eq!(scan.skipped, 2); // notes.txt + .DS_Store

        // Filter to jhd only: the nested .aaf is excluded and NOT counted as skipped junk.
        let only_jhd = chart_files_under(r, Some(Format::Jhd)).unwrap();
        let jhd_names: Vec<String> = only_jhd
            .files
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(jhd_names, vec!["a.jhd", "b.jhd"]);
        assert_eq!(only_jhd.skipped, 2); // still only the true junk
    }

    #[test]
    fn chart_files_under_skips_directory_symlinks() {
        let root = tempdir::TempDir::new("scan_sym").unwrap();
        let r = root.path();
        let real = r.join("real");
        std::fs::create_dir(&real).unwrap();
        std::fs::write(real.join("x.jhd"), b"").unwrap();
        // A symlink pointing back at the root would cause an infinite loop if followed.
        #[cfg(unix)]
        std::os::unix::fs::symlink(r, r.join("loop")).unwrap();
        let scan = chart_files_under(r, None).unwrap();
        // Only the real file; the symlinked dir is not descended.
        assert_eq!(scan.files.len(), 1);
        assert!(scan.files[0].ends_with("real/x.jhd"));
    }

    #[test]
    #[cfg(unix)]
    fn chart_files_under_follows_file_symlinks() {
        let target_dir = tempdir::TempDir::new("scan_target").unwrap();
        let target = target_dir.path().join("real.jhd");
        std::fs::write(&target, b"").unwrap();

        let root = tempdir::TempDir::new("scan_filesym").unwrap();
        std::os::unix::fs::symlink(&target, root.path().join("linked.jhd")).unwrap();

        let scan = chart_files_under(root.path(), None).unwrap();
        assert_eq!(scan.files.len(), 1);
        assert!(scan.files[0].ends_with("linked.jhd"));
    }

    #[test]
    fn chart_files_under_empty_when_no_charts() {
        let root = tempdir::TempDir::new("scan_empty").unwrap();
        std::fs::write(root.path().join("readme.txt"), b"").unwrap();
        let scan = chart_files_under(root.path(), None).unwrap();
        assert!(scan.files.is_empty());
        assert_eq!(scan.skipped, 1);
    }

    /// Item 1: `read_path` on a missing file must return `ChartError::Io` with
    /// the path in its display and no "parse" in the message.
    #[test]
    fn read_path_nonexistent_yields_io_error() {
        let p = Path::new("/nonexistent/no_such_dir/chart.jhd");
        let err = read_path(Format::Jhd, p).unwrap_err();
        assert!(
            matches!(err, ChartError::Io { .. }),
            "expected ChartError::Io, got {err:?}"
        );
        let display = err.to_string();
        assert!(
            !display.contains("parse"),
            "display must not contain 'parse': {display}"
        );
        assert!(
            display.contains("no_such_dir"),
            "display must contain the path component: {display}"
        );
    }

    /// Item 2: `ChartError::Parse` display must carry only the inner message,
    /// with no "parse error:" wrapper prefix.
    #[test]
    fn chart_error_parse_display_has_no_prefix() {
        let err = ChartError::Parse("jhd: bad field".to_string());
        let display = err.to_string();
        assert_eq!(display, "jhd: bad field");
        assert!(
            !display.starts_with("parse error:"),
            "display must not start with 'parse error:': {display}"
        );
    }
}
