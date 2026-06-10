#![cfg(feature = "noref-houses")]
#![allow(clippy::similar_names, clippy::many_single_char_names)]

//! Krusinski-Pisa-Goeldi house system — gated behind `noref-houses`
//! until a refchart oracle is captured. See
//! `docs/discovery/HOUSE_PROMOTION.md`.
//!
//! Every cusp's great circle passes through the ASC–DSC axis, tilted at
//! 30°, 60°, 90°, 120°, 150° from horizontal. Cusps are the ecliptic
//! intersections of these tilted great circles.

use super::HouseCusps;
use crate::coords::acds::ac_rad;
use std::f64::consts::{FRAC_PI_2, PI, TAU};

/// Krusinski-Pisa-Goeldi house cusps from RAMC, obliquity, and latitude.
///
/// Returns `None` for `|lat| >= 90°` and at the equator (`lat_rad == 0.0`),
/// where the ASC–DSC axis lies in the horizon plane and the great-circle
/// family degenerates.
#[must_use]
pub fn krusinski_rad(ramc_rad: f64, obliquity_rad: f64, lat_rad: f64) -> Option<HouseCusps> {
    if lat_rad.abs() >= FRAC_PI_2 {
        return None;
    }
    if lat_rad == 0.0 {
        return None;
    }
    let ac = ac_rad(ramc_rad, obliquity_rad, lat_rad)?;
    let ds = (ac + PI).rem_euclid(TAU);
    let lat_tan = lat_rad.tan();
    let sin_a = ac.sin();
    let cos_a = ac.cos();

    let cusp_for_tilt = |tilt_rad: f64| -> f64 {
        let sin_t = tilt_rad.sin();
        let cos_t = tilt_rad.cos();
        let y = sin_a * cos_t - cos_a * sin_t * lat_tan;
        let x = cos_a * cos_t + sin_a * sin_t * lat_tan;
        y.atan2(x).rem_euclid(TAU)
    };

    let deg30 = 30.0_f64.to_radians();
    let h12 = cusp_for_tilt(deg30);
    let h11 = cusp_for_tilt(2.0 * deg30);
    let h10 = cusp_for_tilt(3.0 * deg30); // tilted 90° = meridian intersection
    let h9 = cusp_for_tilt(4.0 * deg30);
    let h8 = cusp_for_tilt(5.0 * deg30);

    let h2 = (h8 + PI).rem_euclid(TAU);
    let h3 = (h9 + PI).rem_euclid(TAU);
    let h4 = (h10 + PI).rem_euclid(TAU);
    let h5 = (h11 + PI).rem_euclid(TAU);
    let h6 = (h12 + PI).rem_euclid(TAU);

    Some(HouseCusps([
        ac, h2, h3, h4, h5, h6, ds, h8, h9, h10, h11, h12,
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_abs_diff_eq;

    const J2000_EPS_RAD: f64 = 0.409_092_62;

    #[test]
    fn polar_returns_none() {
        assert!(krusinski_rad(0.0, J2000_EPS_RAD, FRAC_PI_2).is_none());
    }

    #[test]
    fn equator_returns_none() {
        assert!(krusinski_rad(0.0, J2000_EPS_RAD, 0.0).is_none());
    }

    #[test]
    fn h1_is_asc_and_h7_is_dsc() {
        let ramc = 200.0_f64.to_radians();
        let eps = J2000_EPS_RAD;
        let lat = 40.0_f64.to_radians();
        let hc = krusinski_rad(ramc, eps, lat).unwrap();
        let ac = ac_rad(ramc, eps, lat).unwrap();
        assert_abs_diff_eq!(hc.cusp(1), ac, epsilon = 1e-10);
        assert_abs_diff_eq!(hc.cusp(7), (ac + PI).rem_euclid(TAU), epsilon = 1e-10);
    }

    #[test]
    fn opposite_cusps_are_180_apart() {
        let ramc = 200.0_f64.to_radians();
        let eps = J2000_EPS_RAD;
        let lat = 40.0_f64.to_radians();
        let hc = krusinski_rad(ramc, eps, lat).unwrap();
        for (a, b) in [(1_u8, 7_u8), (2, 8), (3, 9), (4, 10), (5, 11), (6, 12)] {
            let diff = (hc.cusp(b) - hc.cusp(a)).rem_euclid(TAU);
            assert_abs_diff_eq!(diff.to_degrees(), 180.0, epsilon = 1e-9);
        }
    }
}
