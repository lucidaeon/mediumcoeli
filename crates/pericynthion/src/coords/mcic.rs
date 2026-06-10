//! Midheaven (MC) and Imum Coeli (IC): the meridian / ecliptic axis.
//!
//! The MC is the ecliptic degree culminating on the upper meridian.
//! The IC is its exact opposite (MC + 180°). Neither has polar edge
//! cases — the meridian always intersects the ecliptic at two points
//! regardless of latitude.

use std::f64::consts::{PI, TAU};

/// Ecliptic longitude of the Midheaven (Medium Coeli), radians in \[0, TAU).
///
/// Inputs:
/// - `ramc_rad` — Right Ascension of the Midheaven (= Local Apparent
///   Sidereal Time) in radians.
/// - `obliquity_rad` — true obliquity of the ecliptic in radians.
///
/// Always returns a value — no polar edge case exists for the MC.
#[must_use]
pub fn mc_rad(ramc_rad: f64, obliquity_rad: f64) -> f64 {
    let y = ramc_rad.sin();
    let x = ramc_rad.cos() * obliquity_rad.cos();
    y.atan2(x).rem_euclid(TAU)
}

/// Ecliptic longitude of the Imum Coeli (IC), radians in \[0, TAU).
///
/// Always exactly opposite the MC: `(mc + π) mod 2π`.
#[must_use]
pub fn ic_rad(mc: f64) -> f64 {
    (mc + PI).rem_euclid(TAU)
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_abs_diff_eq;

    const J2000_EPS_RAD: f64 = 0.409_092_62;

    // ── Exact equatorial cases ──────────────────────────────────────────────
    // At the cardinal RAMC values the meridian hits the ecliptic exactly at
    // 0°Aries/Cancer/Libra/Capricorn, independent of obliquity.

    #[test]
    fn ramc_0_mc_is_aries() {
        // RAMC=0°: y=0, x=cos(ε)>0 → atan2(0, pos) = 0° = 0°Aries.
        let mc = mc_rad(0.0, J2000_EPS_RAD).to_degrees();
        assert_abs_diff_eq!(mc, 0.0, epsilon = 1e-10);
    }

    #[test]
    fn ramc_90_mc_is_cancer() {
        // RAMC=90°: y=1, x=0 → atan2(1, 0) = 90° = 0°Cancer.
        let mc = mc_rad(std::f64::consts::FRAC_PI_2, J2000_EPS_RAD).to_degrees();
        assert_abs_diff_eq!(mc, 90.0, epsilon = 1e-10);
    }

    #[test]
    fn ramc_180_mc_is_libra() {
        // RAMC=180°: y=0, x=-cos(ε)<0 → atan2(0, neg) = 180° = 0°Libra.
        let mc = mc_rad(PI, J2000_EPS_RAD).to_degrees();
        assert_abs_diff_eq!(mc, 180.0, epsilon = 1e-10);
    }

    #[test]
    fn ramc_270_mc_is_capricorn() {
        // RAMC=270°: y=-1, x=0 → atan2(-1, 0) = -90° → 270° = 0°Capricorn.
        let mc = mc_rad(3.0 * std::f64::consts::FRAC_PI_2, J2000_EPS_RAD).to_degrees();
        assert_abs_diff_eq!(mc, 270.0, epsilon = 1e-10);
    }

    // ── IC is always exactly opposite ────────────────────────────────────────

    #[test]
    fn ic_is_mc_plus_180() {
        for ramc_deg in [0.0_f64, 45.0, 90.0, 135.0, 180.0, 225.0, 270.0, 315.0] {
            let ramc = ramc_deg.to_radians();
            let mc = mc_rad(ramc, J2000_EPS_RAD);
            let ic = ic_rad(mc);
            let diff = (ic - mc).rem_euclid(TAU);
            assert_abs_diff_eq!(diff, PI, epsilon = 1e-12);
        }
    }

    // Reference-chart MC tests against refchart's resolved coords live in
    // `tests/acceptance_refchart.rs` so the constants stay in a single place.
}
