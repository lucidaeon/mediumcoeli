#![cfg(feature = "noref-houses")]
#![allow(clippy::similar_names)]

//! Meridian (Axial Rotation / Zariel) house system — gated behind
//! `noref-houses` until a refchart oracle is captured.
//!
//! Divides the celestial equator into twelve 30° arcs from the RAMC,
//! then projects each onto the ecliptic. H10 = MC, H1 = ASC (hybrid
//! convention — H1 is pinned to the rising point rather than the
//! equator projection of (RAMC + 270°)).

use super::HouseCusps;
use crate::coords::acds::ac_rad;
use crate::coords::mcic::{ic_rad, mc_rad};
use std::f64::consts::{FRAC_PI_2, PI, TAU};

/// Meridian house cusps from RAMC, obliquity, and observer latitude.
///
/// Returns `None` for `|lat| >= 90°` (where the ASC is undefined).
#[must_use]
pub fn meridian_rad(ramc_rad: f64, obliquity_rad: f64, lat_rad: f64) -> Option<HouseCusps> {
    if lat_rad.abs() >= FRAC_PI_2 {
        return None;
    }
    let eps_cos = obliquity_rad.cos();
    let mc = mc_rad(ramc_rad, obliquity_rad);
    let ac = ac_rad(ramc_rad, obliquity_rad, lat_rad)?;
    let ic = ic_rad(mc);
    let ds = (ac + PI).rem_euclid(TAU);

    let project = |ra_offset_rad: f64| -> f64 {
        let ra = (ramc_rad + ra_offset_rad).rem_euclid(TAU);
        f64::atan2(ra.sin(), ra.cos() * eps_cos).rem_euclid(TAU)
    };

    let deg30 = 30.0_f64.to_radians();
    let h11 = project(deg30);
    let h12 = project(2.0 * deg30);
    let h2 = project(4.0 * deg30);
    let h3 = project(5.0 * deg30);
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
        assert!(meridian_rad(0.0, J2000_EPS_RAD, FRAC_PI_2).is_none());
    }

    #[test]
    fn h1_h4_h7_h10_match_standard_angles() {
        let ramc = 200.0_f64.to_radians();
        let eps = J2000_EPS_RAD;
        let lat = 40.0_f64.to_radians();
        let hc = meridian_rad(ramc, eps, lat).unwrap();
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
        let hc = meridian_rad(ramc, eps, lat).unwrap();
        for (a, b) in [(1_u8, 7_u8), (2, 8), (3, 9), (4, 10), (5, 11), (6, 12)] {
            let diff = (hc.cusp(b) - hc.cusp(a)).rem_euclid(TAU);
            assert_abs_diff_eq!(diff.to_degrees(), 180.0, epsilon = 1e-9);
        }
    }
}
