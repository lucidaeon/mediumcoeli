//! Mean obliquity of the ecliptic — the tilt of Earth's equator
//! relative to the ecliptic plane, in radians.
//!
//! # Background
//!
//! The ecliptic is the plane of Earth's orbit around the Sun; the
//! equator is the plane perpendicular to Earth's rotation axis. These
//! two planes are tilted relative to each other by the obliquity ε,
//! which at J2000.0 equals 23°26′21.448″ (≈ 0.40909263 rad).
//!
//! Obliquity is slowly decreasing — by about 47 arcseconds per
//! century. The **mean** obliquity is the smoothed long-term trend;
//! the **true** obliquity adds nutation in obliquity (Δε, on the order
//! of ±9″), computed in the [`super::nutation`] module.
//!
//! # IAU 2006 formula
//!
//! The IAU 2006 standard fits the mean obliquity to a degree-5
//! polynomial in `T`, Julian centuries since J2000.0 TT:
//!
//! ```text
//! ε₀(T) = 23° 26′ 21.406″
//!       − 46.836769″       · T
//!       −  0.0001831″      · T²
//!       +  0.00200340″     · T³
//!       −  0.000000576″    · T⁴
//!       −  0.0000000434″   · T⁵
//! ```
//!
//! At J2000 (T = 0) this evaluates to 23°26′21.406″, the modern IAU
//! convention. Note this differs slightly from the older J2000.0 value
//! of 23°26′21.448″ used by the IAU 1976 obliquity formula; the 0.042″
//! difference reflects refined VLBI measurements between 1976 and 2006.

use std::f64::consts::PI;

/// Arcsecond → radian conversion factor.
pub const ARCSEC_TO_RAD: f64 = PI / (180.0 * 3600.0);

/// Julian centuries since J2000.0 TT for a given JD-TT.
#[must_use]
pub fn julian_centuries_t(jd_tt: f64) -> f64 {
    (jd_tt - 2_451_545.0) / 36_525.0
}

/// Mean obliquity of the ecliptic at the given JD-TT, in radians.
///
/// Uses the IAU 2006 polynomial fit.
///
/// # Examples
///
/// ```
/// use pericynthion::coords::obliquity::mean_obliquity_rad;
/// // At J2000, obliquity ≈ 23°26′21.4″ ≈ 0.4090928 rad
/// let eps = mean_obliquity_rad(2_451_545.0);
/// assert!((eps - 0.4090928).abs() < 1e-6);
/// ```
#[must_use]
pub fn mean_obliquity_rad(jd_tt: f64) -> f64 {
    let t = julian_centuries_t(jd_tt);
    // Constant term: 23°26′21.406″ in arcseconds.
    let arcseconds_at_j2000 = 23.0 * 3600.0 + 26.0 * 60.0 + 21.406;
    let arcseconds = arcseconds_at_j2000
        + t * (-46.836_769
            + t * (-0.000_183_1
                + t * (0.002_003_40 + t * (-0.000_000_576 + t * -0.000_000_043_4))));
    arcseconds * ARCSEC_TO_RAD
}

/// Mean obliquity in degrees (convenience for human-readable output).
#[must_use]
pub fn mean_obliquity_deg(jd_tt: f64) -> f64 {
    mean_obliquity_rad(jd_tt).to_degrees()
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_abs_diff_eq;

    #[test]
    fn julian_centuries_at_j2000_is_zero() {
        assert!((julian_centuries_t(2_451_545.0) - 0.0).abs() < 1e-15);
    }

    #[test]
    fn julian_centuries_one_century_later() {
        assert_abs_diff_eq!(
            julian_centuries_t(2_451_545.0 + 36_525.0),
            1.0,
            epsilon = 1e-15
        );
    }

    #[test]
    fn obliquity_at_j2000_matches_iau_2006_constant() {
        // 23°26′21.406″ in radians.
        let expected = (23.0 * 3600.0 + 26.0 * 60.0 + 21.406) * ARCSEC_TO_RAD;
        let got = mean_obliquity_rad(2_451_545.0);
        assert_abs_diff_eq!(got, expected, epsilon = 1e-12);
    }

    #[test]
    fn obliquity_decreases_with_time() {
        // The trend is ≈ −47 arcseconds per century.
        let now = mean_obliquity_rad(2_451_545.0);
        let in_a_century = mean_obliquity_rad(2_451_545.0 + 36_525.0);
        let delta = in_a_century - now;
        let delta_arcsec = delta / ARCSEC_TO_RAD;
        assert!(
            (-50.0..=-45.0).contains(&delta_arcsec),
            "Obliquity decrease over 1 century should be ~−47″; got {delta_arcsec}″"
        );
    }

    #[test]
    fn obliquity_at_year_2000_is_23_44_degrees() {
        let deg = mean_obliquity_deg(2_451_545.0);
        assert_abs_diff_eq!(deg, 23.4393, epsilon = 1e-3);
    }

    #[test]
    fn obliquity_at_year_120_was_larger() {
        // Year 120 CE — about 1880 years before J2000, so T ≈ −18.8.
        // Each century adds ~47″, so expect ~880″ ≈ 0.25° larger.
        let jd_120 = 1_764_926.0;
        let eps_120 = mean_obliquity_deg(jd_120);
        let eps_now = mean_obliquity_deg(2_451_545.0);
        let diff_deg = eps_120 - eps_now;
        // Approximate expectation: ≈ +0.245° (≈ 880″ in degrees).
        assert!(
            (0.2..0.3).contains(&diff_deg),
            "Obliquity at year 120 should be ≈ 0.25° larger than at J2000; got Δ={diff_deg}°"
        );
    }
}
