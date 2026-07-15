#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_lossless,
    clippy::many_single_char_names
)]

//! Julian / Gregorian calendar date ↔ Julian Date.
//!
//! # Why both calendars?
//!
//! The Gregorian reform of 1582 was not adopted everywhere
//! simultaneously. Catholic countries switched on 1582-10-15 (skipping
//! ten days); England and its colonies waited until 1752-09-14
//! (skipping eleven days); Russia held out until 1918; Greece, 1923.
//! For astrological work to be correct, *the caller* must know which
//! calendar the recorded date is in — this module refuses to guess.
//!
//! Vettius Valens' birth date (`0120-02-08`) is a Julian date; William
//! Lilly's (`1602-05-11`) is also Julian (England adopted Gregorian
//! 150 years later); post-1582 dates such as 1955-11-12 (Universal City CA)
//! and 1989-02-11 (Paris) are Gregorian. The caller passes the [`Calendar`]
//! explicitly.
//!
//! # Year zero
//!
//! Astronomers use **proleptic** year numbering with a year 0: the
//! sequence … −2, −1, 0, 1, 2 … corresponds to historians' … 3 BCE,
//! 2 BCE, 1 BCE, 1 CE, 2 CE …. We adopt the astronomical convention.
//! A caller with a BCE-labelled date must convert to the astronomical
//! year first (1 BCE → 0, 2 BCE → −1, etc.).
//!
//! # Algorithm
//!
//! Standard Meeus *Astronomical Algorithms* Ch. 7. The Julian Date at
//! 00:00 UT of a calendar date `(year, month, day)`:
//!
//! ```text
//! if month ≤ 2:
//!     y = year − 1
//!     m = month + 12
//! else:
//!     y = year
//!     m = month
//!
//! if Gregorian:
//!     A = ⌊y/100⌋
//!     B = 2 − A + ⌊A/4⌋
//! else:                       // Julian
//!     B = 0
//!
//! JD = ⌊365.25·(y+4716)⌋ + ⌊30.6001·(m+1)⌋ + day + B − 1524.5
//! ```
//!
//! Add the time-of-day fraction (`hour/24 + minute/1440 + second/86400`)
//! to obtain JD at the desired instant. The result is **JD-UT1** when
//! the caller's time is in UT1; conversion to TT applies ΔT via
//! [`crate::time::delta_t::jd_ut_to_jd_tt`].

/// Which calendar the input date is expressed in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Calendar {
    /// Proleptic Julian calendar (used before 1582-10-15 in Catholic
    /// Europe and before various later dates elsewhere).
    Julian,
    /// Proleptic Gregorian calendar (used from 1582-10-15 onward in
    /// Catholic Europe, later elsewhere).
    Gregorian,
    /// Automatic: Julian before 1582-10-15, Gregorian on or after.
    ///
    /// Uses the standard Catholic cutover date only. Does **not** account
    /// for jurisdiction-specific adoption (England 1752, Russia 1918,
    /// Greece 1923). When in doubt about the source calendar, pass
    /// [`Julian`](Calendar::Julian) or [`Gregorian`](Calendar::Gregorian)
    /// explicitly.
    Auto,
}

/// Resolve [`Calendar::Auto`] to [`Calendar::Julian`] or
/// [`Calendar::Gregorian`] for a given date using the 1582-10-15
/// Catholic cutover.
#[must_use]
pub fn auto_calendar(year: i32, month: u8, day: u8) -> Calendar {
    if year < 1582 || (year == 1582 && month < 10) || (year == 1582 && month == 10 && day < 15) {
        Calendar::Julian
    } else {
        Calendar::Gregorian
    }
}

/// Is this date inside the Julian/Gregorian transition era — on or after the
/// 1582-10-15 Catholic cutover through 1927?
///
/// In this window the recorded calendar depends on jurisdiction (Britain/US
/// switched 1752, Russia 1918, Greece 1923, Turkey 1926), so a date alone
/// cannot determine which calendar it was recorded in. Dates before the 1582
/// cutover (proleptic Julian) and after 1927 (universally Gregorian) are
/// unambiguous and return `false`.
///
/// Callers decide the policy (e.g. requiring an explicit calendar in this era);
/// this predicate only reports membership.
#[must_use]
pub fn in_transition_era(year: i16, month: u8, day: u8) -> bool {
    let on_or_after_cutover =
        year > 1582 || (year == 1582 && (month > 10 || (month == 10 && day >= 15)));
    let through_1927 = year <= 1927;
    on_or_after_cutover && through_1927
}

/// A civil date with sub-second time-of-day.
///
/// `year` follows the astronomical convention: a year 0 exists, and
/// negative values represent BCE (year −44 = 45 BCE). `hour`, `minute`,
/// `second` are non-negative; `second` is `f64` to carry sub-second
/// precision when needed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CivilDate {
    /// Astronomical year (year 0 exists; BCE years are negative).
    pub year: i32,
    /// Month, 1..=12.
    pub month: u8,
    /// Day of month, 1..=31 (validity not checked here).
    pub day: u8,
    /// Hour, 0..=23.
    pub hour: u8,
    /// Minute, 0..=59.
    pub minute: u8,
    /// Second, 0.0..60.0.
    pub second: f64,
}

/// Convert a [`CivilDate`] in the specified [`Calendar`] to a Julian
/// Date.
///
/// The returned JD is in the *same time scale* as the input time: if
/// the caller's `hour:minute:second` is in UT1, the result is JD-UT1;
/// if in TT, the result is JD-TT.
///
/// # Examples
///
/// ```
/// use pericynthion::time::calendar::{civil_to_jd, Calendar, CivilDate};
///
/// // J2000.0 = JD 2,451,545.0 = 2000-01-01 12:00 UT (Gregorian)
/// let jd = civil_to_jd(
///     CivilDate { year: 2000, month: 1, day: 1, hour: 12, minute: 0, second: 0.0 },
///     Calendar::Gregorian,
/// );
/// assert!((jd - 2_451_545.0).abs() < 1e-9);
/// ```
#[must_use]
pub fn civil_to_jd(date: CivilDate, calendar: Calendar) -> f64 {
    let calendar = match calendar {
        Calendar::Auto => auto_calendar(date.year, date.month, date.day),
        other => other,
    };
    let (y, m) = if date.month <= 2 {
        (date.year - 1, date.month as i32 + 12)
    } else {
        (date.year, date.month as i32)
    };
    let b = match calendar {
        Calendar::Gregorian => {
            let a = y.div_euclid(100);
            2 - a + a.div_euclid(4)
        }
        Calendar::Julian => 0,
        Calendar::Auto => unreachable!("Auto resolved above"),
    };
    let jd_at_midnight = (365.25 * f64::from(y + 4716)).floor()
        + (30.6001 * f64::from(m + 1)).floor()
        + f64::from(date.day)
        + f64::from(b)
        - 1524.5;
    let frac =
        f64::from(date.hour) / 24.0 + f64::from(date.minute) / 1440.0 + date.second / 86_400.0;
    jd_at_midnight + frac
}

/// Convert a Julian Date back to a [`CivilDate`] in the given calendar.
///
/// Used for diagnostic output and round-trip testing. The result is in
/// the same time scale as the input JD.
///
/// # Panics
///
/// Panics if the computed year cannot be represented as an `i32`.
#[must_use]
pub fn jd_to_civil(jd: f64, calendar: Calendar) -> CivilDate {
    // Meeus Ch. 7, JD → calendar.
    // For Auto: JD 2299160.5 = 1582-10-15 Gregorian 00:00 (first Gregorian day).
    let calendar = match calendar {
        Calendar::Auto => {
            if jd < 2_299_160.5 {
                Calendar::Julian
            } else {
                Calendar::Gregorian
            }
        }
        other => other,
    };
    let jd_plus_half = jd + 0.5;
    let z = jd_plus_half.floor() as i64;
    let f = jd_plus_half - z as f64;
    let a = match calendar {
        Calendar::Gregorian => {
            let alpha = ((z as f64 - 1_867_216.25) / 36_524.25).floor() as i64;
            z + 1 + alpha - alpha.div_euclid(4)
        }
        Calendar::Julian => z,
        Calendar::Auto => unreachable!("Auto resolved above"),
    };
    let b = a + 1524;
    let c = ((b as f64 - 122.1) / 365.25).floor() as i64;
    let d = (365.25 * c as f64).floor() as i64;
    let e = ((b as f64 - d as f64) / 30.6001).floor() as i64;

    let day_with_frac = b as f64 - d as f64 - (30.6001 * e as f64).floor() + f;
    let day = day_with_frac.floor() as u8;
    let day_fraction = day_with_frac - day as f64;

    let month = if e < 14 {
        (e - 1) as u8
    } else {
        (e - 13) as u8
    };
    let year_i64 = if month > 2 { c - 4716 } else { c - 4715 };
    let year = i32::try_from(year_i64).expect("year out of i32 range");

    let total_seconds = day_fraction * 86_400.0;
    let hour = (total_seconds / 3600.0).floor() as u8;
    let minute = ((total_seconds - f64::from(hour) * 3600.0) / 60.0).floor() as u8;
    let second = total_seconds - f64::from(hour) * 3600.0 - f64::from(minute) * 60.0;

    CivilDate {
        year,
        month,
        day,
        hour,
        minute,
        second,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_abs_diff_eq;

    #[test]
    fn in_transition_era_bounds() {
        // Before the 1582-10-15 cutover: unambiguous proleptic Julian.
        assert!(!in_transition_era(1582, 10, 14));
        // The cutover day and after, through 1927: ambiguous.
        assert!(in_transition_era(1582, 10, 15));
        assert!(in_transition_era(1752, 9, 2));
        assert!(in_transition_era(1927, 12, 31));
        // After 1927: universally Gregorian, unambiguous.
        assert!(!in_transition_era(1928, 1, 1));
        // Well before 1582.
        assert!(!in_transition_era(1500, 6, 1));
    }

    fn date(year: i32, month: u8, day: u8, h: u8, mi: u8, s: f64) -> CivilDate {
        CivilDate {
            year,
            month,
            day,
            hour: h,
            minute: mi,
            second: s,
        }
    }

    // === Anchor JDs from authoritative sources ===

    #[test]
    fn j2000_is_jd_2451545_in_gregorian() {
        // J2000.0 by definition: 2000-01-01 12:00:00 TT, in Gregorian.
        let jd = civil_to_jd(date(2000, 1, 1, 12, 0, 0.0), Calendar::Gregorian);
        assert_abs_diff_eq!(jd, 2_451_545.0, epsilon = 1e-9);
    }

    #[test]
    fn unix_epoch_is_jd_2440587_5() {
        // 1970-01-01 00:00:00 UTC = JD 2,440,587.5 (Gregorian).
        let jd = civil_to_jd(date(1970, 1, 1, 0, 0, 0.0), Calendar::Gregorian);
        assert_abs_diff_eq!(jd, 2_440_587.5, epsilon = 1e-9);
    }

    #[test]
    fn jd_zero_is_julian_minus_4712_jan_1_noon() {
        // JD 0 = 1 January −4712 (i.e. 4713 BCE) 12:00, on the Julian
        // calendar (proleptic). Astronomical year −4712.
        let jd = civil_to_jd(date(-4712, 1, 1, 12, 0, 0.0), Calendar::Julian);
        assert_abs_diff_eq!(jd, 0.0, epsilon = 1e-9);
    }

    // === Calendar reform sanity ===

    #[test]
    fn gregorian_reform_skips_ten_days() {
        // 1582-10-04 Julian (last Julian day in Catholic Europe) was
        // immediately followed by 1582-10-15 Gregorian. The two should
        // share the same JD when each is expressed in its own calendar.
        let jd_julian = civil_to_jd(date(1582, 10, 4, 0, 0, 0.0), Calendar::Julian);
        let jd_gregorian = civil_to_jd(date(1582, 10, 15, 0, 0, 0.0), Calendar::Gregorian);
        assert_abs_diff_eq!(jd_julian + 1.0, jd_gregorian, epsilon = 1e-9);
    }

    // === Round-trip ===

    #[test]
    fn civil_to_jd_round_trip_gregorian() {
        for year in [-4000, -1, 0, 1, 1066, 1582, 2026] {
            for &(month, day) in &[(1u8, 1u8), (3, 15), (7, 4), (10, 31), (12, 25)] {
                let d = date(year, month, day, 13, 27, 45.5);
                let jd = civil_to_jd(d, Calendar::Gregorian);
                let back = jd_to_civil(jd, Calendar::Gregorian);
                assert_eq!(back.year, d.year);
                assert_eq!(back.month, d.month);
                assert_eq!(back.day, d.day);
                assert_eq!(back.hour, d.hour);
                assert_eq!(back.minute, d.minute);
                // 1 ms tolerance — comfortably above f64 LSB noise at the
                // largest JDs in the suite (~3.8e-10 days = 33 µs per LSB).
                assert!(
                    (back.second - d.second).abs() < 1e-3,
                    "second mismatch for {d:?}: got {}",
                    back.second
                );
            }
        }
    }

    #[test]
    fn civil_to_jd_round_trip_julian() {
        for year in [-4712, -1, 120, 800, 1500, 1700] {
            for &(month, day) in &[(1u8, 1u8), (3, 15), (6, 21), (9, 23), (12, 31)] {
                let d = date(year, month, day, 6, 13, 22.0);
                let jd = civil_to_jd(d, Calendar::Julian);
                let back = jd_to_civil(jd, Calendar::Julian);
                assert_eq!(back.year, d.year);
                assert_eq!(back.month, d.month);
                assert_eq!(back.day, d.day);
                assert_eq!(back.hour, d.hour);
                assert_eq!(back.minute, d.minute);
                // 1 ms tolerance — comfortably above f64 LSB noise at the
                // largest JDs in the suite (~3.8e-10 days = 33 µs per LSB).
                assert!(
                    (back.second - d.second).abs() < 1e-3,
                    "second mismatch for {d:?}: got {}",
                    back.second
                );
            }
        }
    }

    // === Calendar::Auto ===

    #[test]
    fn auto_before_reform_is_julian() {
        assert_eq!(auto_calendar(120, 2, 8), Calendar::Julian);
        assert_eq!(auto_calendar(1582, 10, 14), Calendar::Julian);
    }

    #[test]
    fn auto_on_reform_day_is_gregorian() {
        assert_eq!(auto_calendar(1582, 10, 15), Calendar::Gregorian);
        assert_eq!(auto_calendar(1984, 11, 1), Calendar::Gregorian);
    }

    #[test]
    fn auto_civil_to_jd_matches_explicit() {
        let d = CivilDate {
            year: 120,
            month: 2,
            day: 8,
            hour: 18,
            minute: 35,
            second: 0.0,
        };
        assert_abs_diff_eq!(
            civil_to_jd(d, Calendar::Auto),
            civil_to_jd(d, Calendar::Julian),
            epsilon = 1e-9
        );
        let d2 = CivilDate {
            year: 1984,
            month: 11,
            day: 1,
            hour: 13,
            minute: 28,
            second: 0.0,
        };
        assert_abs_diff_eq!(
            civil_to_jd(d2, Calendar::Auto),
            civil_to_jd(d2, Calendar::Gregorian),
            epsilon = 1e-9
        );
    }

    // === Test-chart dates from the reference chart set ===
    //
    // Reference values are computed via the same Meeus formula (cross-
    // checked against multiple independent JD calculators including
    // USNO's). These verify the *parsing/wiring* path, not the formula
    // itself — that's anchored by the J2000, Unix-epoch, and JD-0 tests
    // further up.

    #[test]
    fn vettius_valens_birth_date_jd() {
        // 120-02-08 18:35 LMT Antioch, Julian calendar.
        let jd = civil_to_jd(date(120, 2, 8, 18, 35, 0.0), Calendar::Julian);
        assert!(
            (jd - 1_764_926.274_3).abs() < 1e-3,
            "Vettius Valens JD (Julian, local clock as UT) ≈ 1,764,926.274; got {jd}"
        );
    }

    #[test]
    fn william_lilly_birth_date_jd() {
        // 1602-05-11 02:00 LMT, Julian calendar (England pre-1752).
        let jd = civil_to_jd(date(1602, 5, 11, 2, 0, 0.0), Calendar::Julian);
        assert!(
            (jd - 2_306_318.583_3).abs() < 1e-3,
            "William Lilly JD (Julian, local clock as UT) ≈ 2,306,318.583; got {jd}"
        );
    }

    #[test]
    fn anna_freud_local_as_ut_jd() {
        // 1895-12-03 15:15 CET (local clock treated as UT), Gregorian.
        let jd = civil_to_jd(date(1895, 12, 3, 15, 15, 0.0), Calendar::Gregorian);
        assert!(
            (jd - 2_413_531.135).abs() < 1e-3,
            "Anna Freud local-as-UT JD ≈ 2,413,531.135; got {jd}"
        );
    }

    #[test]
    fn lightning_strike_local_as_ut_jd() {
        // 1955-11-12 22:04 PST (local clock treated as UT), Gregorian.
        let jd = civil_to_jd(date(1955, 11, 12, 22, 4, 0.0), Calendar::Gregorian);
        assert!(
            (jd - 2_435_424.419).abs() < 1e-3,
            "Lightning Strike local-as-UT JD ≈ 2,435,424.419; got {jd}"
        );
    }
}
