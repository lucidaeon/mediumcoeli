//! Timestamp and calendar utilities.

use std::path::{Path, PathBuf};

/// Format a Unix timestamp (seconds since 1970-01-01 00:00:00 UTC) as
/// ISO 8601 basic compact UTC: `YYYYMMDDThhmmssZ`.
///
/// Used to generate default output filenames (e.g. `20260606T193045Z.SFcht`).
#[must_use]
#[allow(clippy::cast_possible_wrap)]
pub fn utc_timestamp_from_secs(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let hh = rem / 3_600;
    let mm = (rem % 3_600) / 60;
    let ss = rem % 60;
    let (y, m, d) = days_to_ymd(days);
    format!("{y:04}{m:02}{d:02}T{hh:02}{mm:02}{ss:02}Z")
}

/// Format a UTC offset (decimal hours) as `+HH:MM` or `-HH:MM`.
///
/// Used wherever an offset needs display or serialisation (transcript,
/// JZOD, raw dump).
#[must_use]
pub fn format_utc_offset(hours: f64) -> String {
    jzod::time::format_utc_offset(hours)
}

/// Current wall-clock time as `YYYY-MM-DDTHH:MM:SSZ` (ISO 8601 extended).
///
/// Used in JZOD `ephemeris.calculated_at` fields.
#[must_use]
pub fn utc_iso8601() -> String {
    jzod::time::calculated_at_now()
}

/// Current wall-clock time as `YYYYMMDDThhmmssZ`.
#[must_use]
pub fn utc_timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    utc_timestamp_from_secs(secs)
}

/// Expand a `now.{ext}` path to `blackmoon.{YYYYMMDDThhmmssZ}.{ext}` using the given
/// Unix timestamp.  Any other filename is returned unchanged.  The directory
/// component, if present, is preserved.
///
/// This lets callers pass `--output now.SFcht` instead of having to compute
/// a timestamp manually.
#[must_use]
pub fn expand_now(path: &Path, secs: u64) -> PathBuf {
    let stem = match path.file_stem().and_then(|s| s.to_str()) {
        Some("now") => utc_timestamp_from_secs(secs),
        _ => return path.to_path_buf(),
    };
    let ext = match path.extension().and_then(|s| s.to_str()) {
        Some(e) => format!("blackmoon.{stem}.{e}"),
        None => return path.to_path_buf(),
    };
    match path.parent() {
        Some(p) if p != Path::new("") => p.join(ext),
        _ => PathBuf::from(ext),
    }
}

// Hinnant civil_from_days: days since 1970-01-01 → (year, month, day).
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]
fn days_to_ymd(z: i64) -> (i32, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = i64::from(yoe) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utc_offset_positive_half_hour() {
        assert_eq!(format_utc_offset(5.5), "+05:30");
    }

    #[test]
    fn utc_offset_negative() {
        assert_eq!(format_utc_offset(-8.0), "-08:00");
    }

    #[test]
    fn utc_offset_zero() {
        assert_eq!(format_utc_offset(0.0), "+00:00");
    }

    #[test]
    fn utc_offset_whole_hour() {
        assert_eq!(format_utc_offset(1.0), "+01:00");
    }
}
