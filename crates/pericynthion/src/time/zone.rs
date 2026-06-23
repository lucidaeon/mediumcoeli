//! Time-zone resolution: civil time → UT offset.
//!
//! # v1 scope
//!
//! The four reference charts exercise three kinds of
//! zone handling:
//!
//! 1. **Local Mean Time (LMT)** — used in antiquity and through the
//!    mid-19th century. UT offset = longitude / 15° (hours east of
//!    Greenwich). Vettius Valens (120 CE Antioch) and William Lilly
//!    (1602 London) both record LMT.
//! 2. **Standard named zones** — Paris in 1989 (CET in February),
//!    Universal City CA in 1955 (PST in November).
//! 3. **Explicit fixed offset** — caller supplies the offset directly,
//!    bypassing zone-name lookup. Convenient for the CLI.
//!
//! v1 implements (1) and (3) natively; for (2) the caller computes the
//! correct offset out-of-band (e.g. via the zone-database project)
//! and supplies it as a fixed offset. A full IANA tzdb integration is
//! deferred — for four test charts it's overkill, and tzdb wraps
//! Olson-format binary files in ways that deserve their own crate
//! rather than a hand-rolled half-solution here.

use crate::time::calendar::{Calendar, CivilDate, civil_to_jd};

/// Time-zone specification.
///
/// All variants describe the offset between the caller's civil clock
/// and UT (the astronomical time scale we feed into the ephemeris).
/// East-of-Greenwich is positive. A `Lmt { longitude_east: 36.157 }`
/// (Antioch) corresponds to an offset of +2.41 hours from UT.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Zone {
    /// Local Mean Time at a given geographic longitude. The offset
    /// from UT is `longitude_east_degrees / 15` hours (east positive).
    Lmt {
        /// Geographic longitude, degrees east of Greenwich.
        longitude_east: f64,
    },
    /// Explicit fixed UT offset in seconds (east positive). Use this
    /// when the caller has already resolved a named zone (e.g.
    /// `Europe/Paris` in February 1989 → CET = +1h = +3600 s) via an
    /// external tool.
    FixedSeconds(i32),
}

impl Zone {
    /// UT offset in seconds for this zone (east positive).
    #[must_use]
    pub fn offset_seconds(self) -> i32 {
        match self {
            #[allow(clippy::cast_possible_truncation)]
            Self::Lmt { longitude_east } => (longitude_east / 15.0 * 3600.0).round() as i32,
            Self::FixedSeconds(s) => s,
        }
    }

    /// Construct a fixed-offset zone from hours and minutes (east
    /// positive). Convenience for CLI parsing.
    ///
    /// # Examples
    ///
    /// ```
    /// use pericynthion::time::zone::Zone;
    /// // UTC−05:00 (US Central Daylight Time)
    /// let z = Zone::fixed_hms(-5, 0, 0);
    /// assert_eq!(z.offset_seconds(), -5 * 3600);
    /// ```
    #[must_use]
    pub fn fixed_hms(hours: i32, minutes: i32, seconds: i32) -> Self {
        let sign = if hours < 0 || minutes < 0 || seconds < 0 {
            -1
        } else {
            1
        };
        let total = sign * (hours.abs() * 3600 + minutes.abs() * 60 + seconds.abs());
        Self::FixedSeconds(total)
    }
}

/// Convert a civil date in a given zone to a Julian Date in UT.
///
/// # Examples
///
/// ```
/// use pericynthion::time::calendar::{Calendar, CivilDate};
/// use pericynthion::time::zone::{civil_to_jd_ut, Zone};
///
/// // 2026-01-01 00:00 CST (US Central, UTC−6) = 2026-01-01 06:00 UT.
/// // JD for 2026-01-01 06:00 UT (Gregorian) is 2,461,041.75.
/// let jd = civil_to_jd_ut(
///     CivilDate {
///         year: 2026, month: 1, day: 1,
///         hour: 0, minute: 0, second: 0.0,
///     },
///     Calendar::Gregorian,
///     Zone::fixed_hms(-6, 0, 0),
/// );
/// assert!((jd - 2_461_041.75).abs() < 1e-6);
/// ```
#[must_use]
pub fn civil_to_jd_ut(date: CivilDate, calendar: Calendar, zone: Zone) -> f64 {
    let jd_local = civil_to_jd(date, calendar);
    // Local clock = UT + offset, so UT = local − offset.
    jd_local - f64::from(zone.offset_seconds()) / 86_400.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_abs_diff_eq;

    fn d(year: i32, month: u8, day: u8, h: u8, mi: u8, s: f64) -> CivilDate {
        CivilDate {
            year,
            month,
            day,
            hour: h,
            minute: mi,
            second: s,
        }
    }

    #[test]
    fn lmt_offset_for_greenwich_is_zero() {
        let z = Zone::Lmt {
            longitude_east: 0.0,
        };
        assert_eq!(z.offset_seconds(), 0);
    }

    #[test]
    fn lmt_offset_for_antioch_is_about_2_hours_24_min_east() {
        // Antioch (modern Antakya): 36° 9' 25" E ≈ 36.157°.
        // LMT offset = 36.157 / 15 = 2.4105 h = 8678 s.
        let z = Zone::Lmt {
            longitude_east: 36.157,
        };
        let s = z.offset_seconds();
        assert_abs_diff_eq!(f64::from(s), 8678.0, epsilon = 5.0);
    }

    #[test]
    fn lmt_offset_for_universal_city_is_about_minus_8_hours() {
        // Universal City CA: ~118.35° W = −118.35° E.
        let z = Zone::Lmt {
            longitude_east: -118.35,
        };
        let s = z.offset_seconds();
        // -118.35 / 15 = -7.890 h = -28404 s.
        assert_abs_diff_eq!(f64::from(s), -28404.0, epsilon = 5.0);
    }

    #[test]
    fn fixed_hms_negative_offset_packs_correctly() {
        let z = Zone::fixed_hms(-5, 30, 0);
        assert_eq!(z.offset_seconds(), -(5 * 3600 + 30 * 60));
    }

    #[test]
    fn fixed_hms_positive_offset_packs_correctly() {
        let z = Zone::fixed_hms(9, 30, 0);
        assert_eq!(z.offset_seconds(), 9 * 3600 + 30 * 60);
    }

    #[test]
    fn civil_to_jd_ut_subtracts_zone_offset() {
        // 12:00 CST = 18:00 UT, six hours later.
        let civil = d(2000, 1, 1, 12, 0, 0.0);
        let cst = Zone::fixed_hms(-6, 0, 0);
        let jd_ut = civil_to_jd_ut(civil, Calendar::Gregorian, cst);
        let jd_at_18_ut =
            crate::time::calendar::civil_to_jd(d(2000, 1, 1, 18, 0, 0.0), Calendar::Gregorian);
        assert_abs_diff_eq!(jd_ut, jd_at_18_ut, epsilon = 1e-9);
    }

    // === Per-test-chart LMT/zone resolution ===

    #[test]
    fn vettius_valens_jd_ut_via_lmt() {
        // 120-02-08 18:35 LMT Antioch. LMT offset +2h24m → UT 16:11.
        let civil = d(120, 2, 8, 18, 35, 0.0);
        let zone = Zone::Lmt {
            longitude_east: 36.157,
        };
        let jd_ut = civil_to_jd_ut(civil, Calendar::Julian, zone);
        // Sanity: JD at noon UT Feb 8 120 = 1_764_925.5 + 0.5 = 1_764_926.0
        // We expect a hair past 16:11 UT, ≈ JD 1_764_926.17.
        assert!(
            (jd_ut - 1_764_926.17).abs() < 0.01,
            "Vettius Valens JD UT ≈ 1,764,926.17; got {jd_ut}"
        );
    }

    #[test]
    fn william_lilly_jd_ut_via_lmt() {
        // 1602-05-11 02:00 LMT London (−1.328° E). LMT offset −5.3 min.
        let civil = d(1602, 5, 11, 2, 0, 0.0);
        let zone = Zone::Lmt {
            longitude_east: -1.328,
        };
        let jd_ut = civil_to_jd_ut(civil, Calendar::Julian, zone);
        // Local 02:00 + 5.3 min ≈ UT 02:05.3. JD ≈ 2_306_318.5870.
        assert!(
            (jd_ut - 2_306_318.587).abs() < 0.001,
            "William Lilly JD UT ≈ 2,306,318.587; got {jd_ut}"
        );
    }

    #[test]
    fn anna_freud_jd_ut_via_fixed_cet() {
        // Vienna December 3, 1895 — Central European Time = UT+1.
        let civil = d(1895, 12, 3, 15, 15, 0.0);
        let zone = Zone::fixed_hms(1, 0, 0);
        let jd_ut = civil_to_jd_ut(civil, Calendar::Gregorian, zone);
        // Local 15:15 CET = UT 14:15. JD ≈ 2_413_531.094.
        assert!(
            (jd_ut - 2_413_531.094).abs() < 0.001,
            "Anna Freud JD UT ≈ 2,413,531.094; got {jd_ut}"
        );
    }

    #[test]
    fn lightning_strike_jd_ut_via_fixed_pst() {
        // Universal City CA, November 12, 1955 — Pacific Standard Time = UT−8
        // (US DST ended late October; November is standard time).
        let civil = d(1955, 11, 12, 22, 4, 0.0);
        let zone = Zone::fixed_hms(-8, 0, 0);
        let jd_ut = civil_to_jd_ut(civil, Calendar::Gregorian, zone);
        // Local 22:04 PST = UT 1955-11-13 06:04. JD ≈ 2_435_424.7528.
        assert!(
            (jd_ut - 2_435_424.752_8).abs() < 0.001,
            "Lightning Strike JD UT ≈ 2,435,424.753; got {jd_ut}"
        );
    }
}
