//! Time-system primitives: calendar conversion, ΔT, time-zone handling.
//!
//! Astrology lives in the borderlands between several time scales:
//!
//! - **Civil time** — what a clock on a wall reads, in some named or
//!   ad-hoc time zone.
//! - **UT1 / UT** — astronomical universal time, tied to Earth's
//!   rotation. We treat civil-time-minus-zone-offset as UT1 to working
//!   accuracy.
//! - **TT** — Terrestrial Time, the smooth dynamical time used by the
//!   JPL ephemeris and the IAU coordinate-transformation routines.
//!   `TT = UT + ΔT`, where ΔT ≈ 70 seconds today (SMH 2016 spline /
//!   observational table) and reaches ~10,574 seconds at year 0.
//!
//! Each submodule owns one conversion step.

pub mod calendar;
pub mod delta_t;
pub mod zone;

use zone::Zone;

// ── civil input parsing ───────────────────────────────────────────────────────

/// Errors that arise when parsing user-supplied date, time, or timezone strings.
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    /// Date string did not match expected format.
    #[error("expected YYYY-MM-DD, got {0:?}")]
    DateFormat(String),
    /// Month value out of range 1–12.
    #[error("month {0} out of range (1–12)")]
    Month(u8),
    /// Day value out of range 1–31.
    #[error("day {0} out of range (1–31)")]
    Day(u8),
    /// Time string did not match expected format.
    #[error("expected HH:MM or HH:MM:SS, got {0:?}")]
    TimeFormat(String),
    /// Hour value out of range 0–23.
    #[error("hour {0} out of range (0–23)")]
    Hour(u8),
    /// Minute value out of range 0–59.
    #[error("minute {0} out of range (0–59)")]
    Minute(u8),
    /// Timezone string did not match expected format.
    #[error("expected ±HH:MM[:SS], got {0:?}")]
    TzFormat(String),
    /// Integer parse failure.
    #[error("invalid integer: {0}")]
    Int(#[from] std::num::ParseIntError),
    /// Float parse failure.
    #[error("invalid float: {0}")]
    Float(#[from] std::num::ParseFloatError),
}

/// Parse `"YYYY-MM-DD"` (with optional leading `"-"` for BCE) into `(year, month, day)`.
///
/// # Examples
/// ```
/// use pericynthion::time::parse_date;
/// assert_eq!(parse_date("2000-03-20").unwrap(), (2000, 3, 20));
/// assert_eq!(parse_date("-0044-03-15").unwrap(), (-44, 3, 15));
/// ```
///
/// # Errors
///
/// Returns [`ParseError`] if the string is not in `YYYY-MM-DD` format,
/// or if month/day values are out of range.
pub fn parse_date(s: &str) -> Result<(i32, u8, u8), ParseError> {
    let (sign, rest) = if let Some(stripped) = s.strip_prefix('-') {
        (-1_i32, stripped)
    } else {
        (1, s)
    };
    let parts: Vec<&str> = rest.split('-').collect();
    if parts.len() != 3 {
        return Err(ParseError::DateFormat(s.to_string()));
    }
    let year: i32 = sign * parts[0].parse::<i32>()?;
    let month: u8 = parts[1].parse()?;
    let day: u8 = parts[2].parse()?;
    if !(1..=12).contains(&month) {
        return Err(ParseError::Month(month));
    }
    if !(1..=31).contains(&day) {
        return Err(ParseError::Day(day));
    }
    Ok((year, month, day))
}

/// Parse `"HH:MM"` or `"HH:MM:SS[.frac]"` into `(hour, minute, second)`.
/// Second is `f64` to support fractional seconds.
///
/// # Errors
///
/// Returns [`ParseError`] if the string is not in `HH:MM` or `HH:MM:SS` format,
/// or if hour/minute values are out of range.
pub fn parse_time(s: &str) -> Result<(u8, u8, f64), ParseError> {
    let parts: Vec<&str> = s.split(':').collect();
    if !(2..=3).contains(&parts.len()) {
        return Err(ParseError::TimeFormat(s.to_string()));
    }
    let hour: u8 = parts[0].parse()?;
    let minute: u8 = parts[1].parse()?;
    let second: f64 = if parts.len() == 3 {
        parts[2].parse()?
    } else {
        0.0
    };
    if hour >= 24 {
        return Err(ParseError::Hour(hour));
    }
    if minute >= 60 {
        return Err(ParseError::Minute(minute));
    }
    Ok((hour, minute, second))
}

/// Parse `"±HH:MM[:SS]"` (or unsigned, defaults to positive) into a [`Zone`].
///
/// # Errors
///
/// Returns [`ParseError`] if the string is not in `±HH:MM` or `±HH:MM:SS` format.
pub fn parse_tz(s: &str) -> Result<Zone, ParseError> {
    let (sign, rest) = if let Some(r) = s.strip_prefix('-') {
        (-1_i32, r)
    } else if let Some(r) = s.strip_prefix('+') {
        (1, r)
    } else {
        (1, s)
    };
    let parts: Vec<&str> = rest.split(':').collect();
    if parts.is_empty() || parts.len() > 3 {
        return Err(ParseError::TzFormat(s.to_string()));
    }
    let h: i32 = parts[0].parse()?;
    let m: i32 = if parts.len() >= 2 {
        parts[1].parse()?
    } else {
        0
    };
    let sec: i32 = if parts.len() == 3 {
        parts[2].parse()?
    } else {
        0
    };
    Ok(Zone::FixedSeconds(sign * (h * 3600 + m * 60 + sec)))
}

/// Convert a Unix timestamp (seconds since 1970-01-01T00:00:00Z) to a UTC
/// civil date-time tuple `(year, month, day, hour, minute, second)`.
///
/// Uses the proleptic Gregorian algorithm from Howard Hinnant's date library.
#[must_use]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::many_single_char_names
)]
pub fn unix_to_utc(secs: u64) -> (i32, u8, u8, u8, u8, u8) {
    let days = secs / 86_400;
    let rem = secs % 86_400;
    let (h, mi, s) = (
        (rem / 3_600) as u8,
        ((rem % 3_600) / 60) as u8,
        (rem % 60) as u8,
    );
    let z = days as i64 + 719_468;
    let era = (if z >= 0 { z } else { z - 146_096 }) / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u8;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u8;
    let y = if m <= 2 { y + 1 } else { y } as i32;
    (y, m, d, h, mi, s)
}
