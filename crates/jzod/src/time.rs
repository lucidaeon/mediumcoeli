//! Timestamp and UTC-offset formatting for JZOD `utc_offset` and
//! `ephemeris.calculated_at` fields.

/// Format a UTC offset given in decimal hours as `+HH:MM` or `-HH:MM`.
#[must_use]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
pub fn format_utc_offset(hours: f64) -> String {
    let sign = if hours < 0.0 { '-' } else { '+' };
    let abs = hours.abs();
    let h = abs.floor() as u32;
    let m = ((abs - f64::from(h)) * 60.0).round() as u32;
    format!("{sign}{h:02}:{m:02}")
}

/// Format a Unix timestamp (seconds since 1970-01-01 UTC) as ISO 8601 extended
/// UTC: `YYYY-MM-DDTHH:MM:SSZ`.
#[must_use]
#[allow(clippy::cast_possible_wrap)]
pub fn calculated_at_from_secs(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let hh = rem / 3_600;
    let mm = (rem % 3_600) / 60;
    let ss = rem % 60;
    let (y, m, d) = days_to_ymd(days);
    format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}Z")
}

/// Current wall-clock time as `YYYY-MM-DDTHH:MM:SSZ`.
#[must_use]
pub fn calculated_at_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    calculated_at_from_secs(secs)
}

// Hinnant civil_from_days: days since 1970-01-01 -> (year, month, day).
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
    fn offset_positive_half_hour() {
        assert_eq!(format_utc_offset(5.5), "+05:30");
    }

    #[test]
    fn offset_negative_whole_hour() {
        assert_eq!(format_utc_offset(-8.0), "-08:00");
    }

    #[test]
    fn offset_zero_is_positive() {
        assert_eq!(format_utc_offset(0.0), "+00:00");
    }

    #[test]
    fn calculated_at_formats_known_epoch() {
        // 2026-06-08T20:45:18Z -> 1780951518 seconds since the Unix epoch.
        assert_eq!(
            calculated_at_from_secs(1_780_951_518),
            "2026-06-08T20:45:18Z"
        );
    }

    #[test]
    fn calculated_at_epoch_zero() {
        assert_eq!(calculated_at_from_secs(0), "1970-01-01T00:00:00Z");
    }
}
