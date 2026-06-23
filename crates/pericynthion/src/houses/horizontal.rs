#![cfg(feature = "noref-houses")]
#![allow(clippy::similar_names, clippy::many_single_char_names)]

//! Horizontal (Azimuthal) house system — gated behind `noref-houses`
//! until a refchart oracle is captured.
//!
//! Divides the horizon into 12 equal 30° azimuth sectors starting from
//! the east point (so H1 = ASC). Each cusp is the ecliptic image of an
//! azimuth division.

use super::HouseCusps;
use crate::coords::acds::ac_rad;
use std::f64::consts::{FRAC_PI_2, PI, TAU};

/// Horizontal house cusps from RAMC, obliquity, and observer latitude.
///
/// Returns `None` for `|lat| >= 90°` and at the equator (`lat_rad == 0.0`),
/// where the horizon coincides with the prime vertical.
#[must_use]
pub fn horizontal_rad(ramc_rad: f64, obliquity_rad: f64, lat_rad: f64) -> Option<HouseCusps> {
    if lat_rad.abs() >= FRAC_PI_2 {
        return None;
    }
    if lat_rad == 0.0 {
        // At the equator the horizon coincides with the prime vertical;
        // azimuth-derived cusps are not uniquely separable from Campanus.
        return None;
    }
    let eps_cos = obliquity_rad.cos();
    let eps_sin = obliquity_rad.sin();
    let lat_cos = lat_rad.cos();
    let lat_sin = lat_rad.sin();

    let ac = ac_rad(ramc_rad, obliquity_rad, lat_rad)?;
    let ds = (ac + PI).rem_euclid(TAU);

    // For each azimuth (measured east from north), project the horizon
    // point onto the ecliptic.
    let cusp_for_az = |az_rad: f64| -> f64 {
        let sin_a = az_rad.sin();
        let cos_a = az_rad.cos();
        let sin_delta = (cos_a * lat_cos).clamp(-1.0, 1.0);
        let delta = sin_delta.asin();
        let h = f64::atan2(-sin_a, -cos_a * lat_sin);
        let ra = (ramc_rad + h).rem_euclid(TAU);
        let y = ra.sin() * eps_cos + delta.tan() * eps_sin;
        let x = ra.cos();
        y.atan2(x).rem_euclid(TAU)
    };

    let deg30 = 30.0_f64.to_radians();
    // H1 at azimuth 90° (east = ASC); subsequent cusps step by +30°.
    let h1 = ac;
    let h2 = cusp_for_az(FRAC_PI_2 + deg30);
    let h3 = cusp_for_az(FRAC_PI_2 + 2.0 * deg30);
    let h4 = cusp_for_az(FRAC_PI_2 + 3.0 * deg30); // azimuth 180° = north → nadir
    let h5 = cusp_for_az(FRAC_PI_2 + 4.0 * deg30);
    let h6 = cusp_for_az(FRAC_PI_2 + 5.0 * deg30);
    let h7 = ds;
    let h8 = (h2 + PI).rem_euclid(TAU);
    let h9 = (h3 + PI).rem_euclid(TAU);
    let h10 = (h4 + PI).rem_euclid(TAU); // azimuth 0° = zenith
    let h11 = (h5 + PI).rem_euclid(TAU);
    let h12 = (h6 + PI).rem_euclid(TAU);

    Some(HouseCusps([
        h1, h2, h3, h4, h5, h6, h7, h8, h9, h10, h11, h12,
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_abs_diff_eq;

    const J2000_EPS_RAD: f64 = 0.409_092_62;

    #[test]
    fn polar_returns_none() {
        assert!(horizontal_rad(0.0, J2000_EPS_RAD, FRAC_PI_2).is_none());
    }

    #[test]
    fn equator_returns_none() {
        assert!(horizontal_rad(0.0, J2000_EPS_RAD, 0.0).is_none());
    }

    #[test]
    fn h1_is_asc_and_h7_is_dsc() {
        let ramc = 200.0_f64.to_radians();
        let eps = J2000_EPS_RAD;
        let lat = 40.0_f64.to_radians();
        let hc = horizontal_rad(ramc, eps, lat).unwrap();
        let ac = ac_rad(ramc, eps, lat).unwrap();
        assert_abs_diff_eq!(hc.cusp(1), ac, epsilon = 1e-10);
        assert_abs_diff_eq!(hc.cusp(7), (ac + PI).rem_euclid(TAU), epsilon = 1e-10);
    }

    #[test]
    fn opposite_cusps_are_180_apart() {
        let ramc = 200.0_f64.to_radians();
        let eps = J2000_EPS_RAD;
        let lat = 40.0_f64.to_radians();
        let hc = horizontal_rad(ramc, eps, lat).unwrap();
        for (a, b) in [(1_u8, 7_u8), (2, 8), (3, 9), (4, 10), (5, 11), (6, 12)] {
            let diff = (hc.cusp(b) - hc.cusp(a)).rem_euclid(TAU);
            assert_abs_diff_eq!(diff.to_degrees(), 180.0, epsilon = 1e-9);
        }
    }
}
