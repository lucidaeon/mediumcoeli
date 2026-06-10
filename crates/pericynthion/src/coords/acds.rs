//! Ascendant + Descendant: the ecliptic / horizon axis.
//!
//! The Ascendant ([`ac_rad`]) is the eastern intersection of the horizon
//! with the ecliptic; the Descendant ([`ds_rad`]) is its exact opposite
//! (Ac + 180°). The Ascendant formula is Meeus *Astronomical Algorithms*
//! Ch. 14 spherical trig in (RAMC, true obliquity, geographic latitude).

use std::f64::consts::{FRAC_PI_2, PI, TAU};

/// Ecliptic longitude of the Ascendant, radians in \[0, TAU).
///
/// Inputs:
/// - `ramc_rad` — Right Ascension of the Midheaven (= Local Apparent Sidereal
///   Time) in radians.
/// - `obliquity_rad` — true obliquity of the ecliptic in radians.
/// - `lat_rad` — geographic latitude in radians (north positive).
///
/// Returns `None` when `|lat_rad| ≥ π/2` (at or beyond the geographic poles,
/// where `tan(lat)` diverges).
///
/// For the polar zone (`|lat| > π/2 − ε ≈ 1.162 rad ≈ 66.6°`) the formula
/// degrades gracefully: as latitude → ±90° the result converges toward
/// 0° Libra (180°), which is the only ecliptic degree on the horizon at the
/// poles.
#[must_use]
pub fn ac_rad(ramc_rad: f64, obliquity_rad: f64, lat_rad: f64) -> Option<f64> {
    if lat_rad.abs() >= FRAC_PI_2 {
        return None;
    }
    let y = ramc_rad.cos();
    let x = -(ramc_rad.sin() * obliquity_rad.cos() + lat_rad.tan() * obliquity_rad.sin());
    Some(y.atan2(x).rem_euclid(TAU))
}

/// Ecliptic longitude of the Descendant, radians in \[0, TAU).
///
/// Always exactly opposite the Ascendant: `(ac + π) mod 2π`. Parallel to
/// [`crate::coords::mcic::ic_rad`] and [`crate::coords::vxax::ax_rad`].
#[must_use]
pub fn ds_rad(ac_rad: f64) -> f64 {
    (ac_rad + PI).rem_euclid(TAU)
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_abs_diff_eq;

    // Mean obliquity at J2000.0 (23°26′21.406″ → 0.40909262… rad), used for
    // the exact equatorial tests where the specific ε value cancels out.
    const J2000_EPS_RAD: f64 = 0.409_092_62;

    // ── Exact equatorial cases ──────────────────────────────────────────────
    // At lat = 0, tan(lat) = 0, so the formula reduces to:
    //   atan2(cos(RAMC), -sin(RAMC)*cos(ε)).
    // At the cardinal RAMC values cos(ε) is nonzero and the denominator is
    // either ±cos(ε) or 0, giving exact quadrant results independent of ε.

    #[test]
    fn equator_ramc_0_asc_is_cancer() {
        // RAMC = 0°: y = 1, x = 0  →  atan2(1, 0) = π/2 = 90° = 0°Cancer.
        let ac = ac_rad(0.0, J2000_EPS_RAD, 0.0).unwrap();
        assert_abs_diff_eq!(ac.to_degrees(), 90.0, epsilon = 1e-10);
    }

    #[test]
    fn equator_ramc_90_asc_is_libra() {
        // RAMC = 90°: y = 0, x = -cos(ε) < 0  →  atan2(0, neg) = π = 180° = 0°Libra.
        let ac = ac_rad(FRAC_PI_2, J2000_EPS_RAD, 0.0).unwrap();
        assert_abs_diff_eq!(ac.to_degrees(), 180.0, epsilon = 1e-10);
    }

    #[test]
    fn equator_ramc_180_asc_is_capricorn() {
        // RAMC = 180°: y = -1, x = 0  →  atan2(-1, 0) = -π/2 → 270° = 0°Capricorn.
        let ac = ac_rad(std::f64::consts::PI, J2000_EPS_RAD, 0.0).unwrap();
        assert_abs_diff_eq!(ac.to_degrees(), 270.0, epsilon = 1e-10);
    }

    #[test]
    fn equator_ramc_270_asc_is_aries() {
        // RAMC = 270°: y ≈ 0, x = cos(ε) > 0  →  atan2(0, pos) = 0° = 0°Aries.
        let ac = ac_rad(3.0 * FRAC_PI_2, J2000_EPS_RAD, 0.0).unwrap();
        // f64 cos(3π/2) is ~−1.8e−16 (not exactly 0), so the result wraps
        // fractionally below TAU. Both 0° and 360° representations are ≈ 0°.
        assert_abs_diff_eq!(ac.to_degrees().rem_euclid(360.0), 0.0, epsilon = 1e-8);
    }

    // ── Polar edge cases ────────────────────────────────────────────────────

    #[test]
    fn north_pole_returns_none() {
        assert!(ac_rad(0.0, J2000_EPS_RAD, FRAC_PI_2).is_none());
    }

    #[test]
    fn south_pole_returns_none() {
        assert!(ac_rad(0.0, J2000_EPS_RAD, -FRAC_PI_2).is_none());
    }

    #[test]
    fn beyond_poles_returns_none() {
        assert!(ac_rad(0.0, J2000_EPS_RAD, FRAC_PI_2 + 0.01).is_none());
        assert!(ac_rad(0.0, J2000_EPS_RAD, -(FRAC_PI_2 + 0.01)).is_none());
    }

    // Reference-chart Ac/Ds tests against refchart's resolved coords live
    // in `tests/acceptance_refchart.rs` so the constants stay in a single place.

    #[test]
    fn ds_is_ac_plus_180() {
        let ac = 125.0_f64.to_radians();
        let ds = ds_rad(ac);
        let diff = (ds - ac).rem_euclid(TAU);
        assert_abs_diff_eq!(diff.to_degrees(), 180.0, epsilon = 1e-12);
    }
}
