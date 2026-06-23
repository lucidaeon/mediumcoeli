#![cfg(feature = "noref-houses")]
#![allow(clippy::similar_names, clippy::many_single_char_names)]

//! Campanus house system — gated behind `noref-houses` until a refchart
//! oracle is captured.
//!
//! Divides the prime vertical (the great circle through east, zenith,
//! west, nadir) into twelve equal 30° arcs from the east point, then
//! projects each division onto the ecliptic.

use super::HouseCusps;
use crate::coords::acds::ac_rad;
use crate::coords::mcic::{ic_rad, mc_rad};
use std::f64::consts::{FRAC_PI_2, PI, TAU};

/// Campanus house cusps in radians \[0, TAU), keyed by RAMC, obliquity,
/// and geographic latitude.
///
/// Returns `None` when `|lat_rad| ≥ π/2` or when `lat_rad == 0.0`
/// (the prime vertical coincides with the horizon at the equator).
#[must_use]
pub fn campanus_rad(ramc_rad: f64, obliquity_rad: f64, lat_rad: f64) -> Option<HouseCusps> {
    if lat_rad.abs() >= FRAC_PI_2 {
        return None;
    }
    if lat_rad == 0.0 {
        // At the equator the prime vertical coincides with the horizon;
        // Campanus is degenerate (matches the Vertex degeneracy in vxax.rs).
        return None;
    }
    let eps_cos = obliquity_rad.cos();
    let eps_sin = obliquity_rad.sin();
    let lat_sin = lat_rad.sin();
    let lat_cos = lat_rad.cos();

    let mc = mc_rad(ramc_rad, obliquity_rad);
    let ac = ac_rad(ramc_rad, obliquity_rad, lat_rad)?;
    let ic = ic_rad(mc);
    let ds = (ac + PI).rem_euclid(TAU);

    // Prime-vertical projection: A measured counterclockwise from the
    // east point. The four intermediate cusps sit at A = 30°, 60°, 120°,
    // 150°.
    let cusp_for_a = |a_rad: f64| -> f64 {
        let sin_a = a_rad.sin();
        let cos_a = a_rad.cos();
        let sin_h = ramc_rad.sin();
        let cos_h = ramc_rad.cos();
        let y = sin_h * cos_a + cos_h * sin_a * lat_sin;
        let x = eps_cos * (cos_h * cos_a - sin_h * sin_a * lat_sin) - eps_sin * sin_a * lat_cos;
        y.atan2(x).rem_euclid(TAU)
    };

    let deg30 = 30.0_f64.to_radians();
    let h11 = cusp_for_a(deg30);
    let h12 = cusp_for_a(2.0 * deg30);
    let h2 = cusp_for_a(4.0 * deg30);
    let h3 = cusp_for_a(5.0 * deg30);
    let h5 = (h11 + PI).rem_euclid(TAU);
    let h6 = (h12 + PI).rem_euclid(TAU);
    let h8 = (h2 + PI).rem_euclid(TAU);
    let h9 = (h3 + PI).rem_euclid(TAU);

    Some(HouseCusps([
        ac, h2, h3, ic, h5, h6, ds, h8, h9, mc, h11, h12,
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_abs_diff_eq;

    const J2000_EPS_RAD: f64 = 0.409_092_62;

    #[test]
    fn polar_returns_none() {
        assert!(campanus_rad(0.0, J2000_EPS_RAD, FRAC_PI_2).is_none());
        assert!(campanus_rad(0.0, J2000_EPS_RAD, -FRAC_PI_2).is_none());
    }

    #[test]
    fn equator_returns_none() {
        assert!(campanus_rad(0.0, J2000_EPS_RAD, 0.0).is_none());
    }

    #[test]
    fn h1_h4_h7_h10_match_standard_angles() {
        let ramc = 200.0_f64.to_radians();
        let eps = J2000_EPS_RAD;
        let lat = 40.0_f64.to_radians();
        let hc = campanus_rad(ramc, eps, lat).unwrap();
        let ac = ac_rad(ramc, eps, lat).unwrap();
        let mc = mc_rad(ramc, eps);
        assert_abs_diff_eq!(hc.cusp(1), ac, epsilon = 1e-10);
        assert_abs_diff_eq!(hc.cusp(10), mc, epsilon = 1e-10);
        assert_abs_diff_eq!(hc.cusp(4), ic_rad(mc), epsilon = 1e-10);
        assert_abs_diff_eq!(hc.cusp(7), (ac + PI).rem_euclid(TAU), epsilon = 1e-10);
    }

    #[test]
    fn opposite_cusps_are_180_apart() {
        let ramc = 200.0_f64.to_radians();
        let eps = J2000_EPS_RAD;
        let lat = 40.0_f64.to_radians();
        let hc = campanus_rad(ramc, eps, lat).unwrap();
        for (a, b) in [(1_u8, 7_u8), (2, 8), (3, 9), (4, 10), (5, 11), (6, 12)] {
            let diff = (hc.cusp(b) - hc.cusp(a)).rem_euclid(TAU);
            assert_abs_diff_eq!(diff.to_degrees(), 180.0, epsilon = 1e-9);
        }
    }

    #[test]
    fn cusps_partition_the_circle() {
        let ramc = 200.0_f64.to_radians();
        let eps = J2000_EPS_RAD;
        let lat = 40.0_f64.to_radians();
        let hc = campanus_rad(ramc, eps, lat).unwrap();
        let mut total = 0.0_f64;
        for i in 0..12 {
            let span = (hc.0[(i + 1) % 12] - hc.0[i]).rem_euclid(TAU).to_degrees();
            assert!(span > 0.0 && span < 180.0, "H{} span {:.3}°", i + 1, span);
            total += span;
        }
        assert_abs_diff_eq!(total, 360.0, epsilon = 1e-6);
    }
}
