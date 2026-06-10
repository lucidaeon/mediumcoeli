#![allow(clippy::similar_names)]

//! Greenwich Sidereal Time (mean and apparent).
//!
//! Required for expressing the observer's Earth-fixed position in the
//! true equatorial frame of date — the prerequisite for topocentric
//! parallax correction.

use std::f64::consts::TAU;

/// Greenwich Mean Sidereal Time (radians) at the given UT1 Julian Date.
///
/// IAU 1982 formula (Meeus, *Astronomical Algorithms*, eq. 12.2):
/// θ₀ = 280.46061837° + 360.98564736629° × (JD − J2000) + 0.000387933° T² − T³/38710000°
/// where T = (JD − 2451545) / 36525.
#[must_use]
pub fn gmst_rad(jd_ut1: f64) -> f64 {
    let d = jd_ut1 - 2_451_545.0;
    let t = d / 36_525.0;
    let deg =
        280.460_618_37 + 360.985_647_366_29 * d + 0.000_387_933 * t * t - t * t * t / 38_710_000.0;
    deg.to_radians().rem_euclid(TAU)
}

/// Greenwich Apparent Sidereal Time (radians) at the given TT Julian Date.
///
/// GAST = GMST + equation of the equinoxes = GMST + Δψ · `cos(ε_true)`.
/// Converts `JD_TT` to `JD_UT1` internally using ΔT.
#[must_use]
pub fn gast_rad(jd_tt: f64) -> f64 {
    use crate::coords::nutation::nutation;
    use crate::coords::obliquity::mean_obliquity_rad;
    use crate::time::delta_t::jd_tt_to_jd_ut;

    let jd_ut1 = jd_tt_to_jd_ut(jd_tt);
    let gmst = gmst_rad(jd_ut1);
    let nut = nutation(jd_tt);
    let eps_true = mean_obliquity_rad(jd_tt) + nut.delta_epsilon;
    (gmst + nut.delta_psi * eps_true.cos()).rem_euclid(TAU)
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_abs_diff_eq;

    // Meeus, Astronomical Algorithms, Example 12.a (p. 88):
    // At JD 2451545.0 (J2000.0 = 2000-01-01 12:00 UT), GMST = 280.46061837°.
    #[test]
    fn gmst_at_j2000() {
        let gmst_deg = gmst_rad(2_451_545.0).to_degrees();
        assert_abs_diff_eq!(gmst_deg, 280.460_618_37, epsilon = 0.001);
    }

    // GMST advances by 360.985647° per mean solar day. Both values are wrapped
    // to [0°, 360°), so the numeric difference is 0.985647° (the fractional
    // part above a full rotation), not 360.985647°.
    #[test]
    fn gmst_advances_one_sidereal_day_per_solar_day() {
        let g0 = gmst_rad(2_451_545.0);
        let g1 = gmst_rad(2_451_546.0);
        let advance_deg = (g1 - g0).rem_euclid(TAU).to_degrees();
        assert_abs_diff_eq!(advance_deg, 0.985_647, epsilon = 0.001);
    }

    // GAST and GMST must agree to within the equation of the equinoxes (~16").
    // gast_rad(JD_TT) converts to JD_UT1 internally; compare against the GMST
    // at that same UT1 Julian Date, not the original TT date.
    #[test]
    fn gast_near_gmst_at_j2000() {
        use crate::time::delta_t::jd_tt_to_jd_ut;
        let jd_tt = 2_451_545.0;
        let jd_ut1 = jd_tt_to_jd_ut(jd_tt);
        let gast = gast_rad(jd_tt).to_degrees();
        let gmst = gmst_rad(jd_ut1).to_degrees();
        // Equation of the equinoxes is < 20 arcseconds.
        assert!(
            (gast - gmst).abs() < 20.0 / 3600.0,
            "gast={gast} gmst={gmst}"
        );
    }
}
