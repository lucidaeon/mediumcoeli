#![cfg(feature = "noref-houses")]
#![allow(clippy::similar_names)]

//! Topocentric (Polich-Page 1961) house system — gated behind
//! `noref-houses` until a refchart oracle is captured.
//!
//! Closed-form Placidus approximation using a modified-latitude function
//! tan(θ) = f · tan(φ), where f is the Placidus semi-arc fraction
//! (1/3 or 2/3). Numerically stable at high latitudes where Placidus
//! bisection breaks down.

use super::HouseCusps;
use crate::coords::acds::ac_rad;
use crate::coords::mcic::{ic_rad, mc_rad};
use std::f64::consts::{FRAC_PI_2, PI, TAU};

/// Topocentric (Polich-Page) house cusps from RAMC, obliquity, and lat.
///
/// Returns `None` for `|lat| >= 90°` (where the ASC is undefined).
#[must_use]
pub fn topocentric_rad(ramc_rad: f64, obliquity_rad: f64, lat_rad: f64) -> Option<HouseCusps> {
    if lat_rad.abs() >= FRAC_PI_2 {
        return None;
    }
    let eps_cos = obliquity_rad.cos();
    let eps_sin = obliquity_rad.sin();
    let lat_tan = lat_rad.tan();

    let mc = mc_rad(ramc_rad, obliquity_rad);
    let ac = ac_rad(ramc_rad, obliquity_rad, lat_rad)?;
    let ic = ic_rad(mc);
    let ds = (ac + PI).rem_euclid(TAU);

    let cusp_for = |h_rad: f64, fraction: f64| -> f64 {
        let theta_tan = fraction * lat_tan;
        let ra = (ramc_rad + h_rad).rem_euclid(TAU);
        let y = ra.sin();
        let x = ra.cos() * eps_cos - theta_tan * eps_sin * h_rad.sin();
        y.atan2(x).rem_euclid(TAU)
    };

    let deg30 = 30.0_f64.to_radians();
    let h11 = cusp_for(deg30, 1.0 / 3.0);
    let h12 = cusp_for(2.0 * deg30, 2.0 / 3.0);
    let h2 = cusp_for(4.0 * deg30, 2.0 / 3.0);
    let h3 = cusp_for(5.0 * deg30, 1.0 / 3.0);
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
        assert!(topocentric_rad(0.0, J2000_EPS_RAD, FRAC_PI_2).is_none());
    }

    #[test]
    fn h1_h4_h7_h10_match_standard_angles() {
        let ramc = 200.0_f64.to_radians();
        let eps = J2000_EPS_RAD;
        let lat = 40.0_f64.to_radians();
        let hc = topocentric_rad(ramc, eps, lat).unwrap();
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
        let hc = topocentric_rad(ramc, eps, lat).unwrap();
        for (a, b) in [(1_u8, 7_u8), (2, 8), (3, 9), (4, 10), (5, 11), (6, 12)] {
            let diff = (hc.cusp(b) - hc.cusp(a)).rem_euclid(TAU);
            assert_abs_diff_eq!(diff.to_degrees(), 180.0, epsilon = 1e-9);
        }
    }

    #[test]
    fn finite_at_low_latitude() {
        // At low latitudes the modified-pole correction shrinks to zero;
        // Topocentric should remain finite and well-defined.
        let ramc = 100.0_f64.to_radians();
        let eps = J2000_EPS_RAD;
        let lat = 5.0_f64.to_radians();
        let hc = topocentric_rad(ramc, eps, lat).unwrap();
        for h in 1u8..=12 {
            assert!(hc.cusp(h).is_finite(), "H{h} should be finite at low lat");
        }
    }
}
