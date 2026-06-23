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
//! [`Aaf`](crate::format::Format::Aaf) on write) return
//! [`UnsupportedDirection`](crate::error::ChartError::UnsupportedDirection).

use crate::chart::Chart;
use crate::error::ChartError;
use crate::format::Format;

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
}
