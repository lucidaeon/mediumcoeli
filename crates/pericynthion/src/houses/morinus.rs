#![cfg(feature = "noref-houses")]

//! Morinus (Morin de Villefranche) house system — gated behind
//! `noref-houses` until a refchart oracle is captured. See
//! `docs/discovery/HOUSE_PROMOTION.md`.
//!
//! Divides the celestial equator into twelve 30° arcs starting from the
//! RAMC, then projects each division onto the ecliptic. No latitude
//! dependence in the intermediate cusps — Morinus is the only quadrant
//! system that remains well-defined at all geographic latitudes
//! including the poles. Note that **H1 in Morinus is not the ASC**; it
//! is the ecliptic image of (RAMC + 270°) on the equator. H10 = MC.

use super::HouseCusps;
use crate::coords::mcic::mc_rad;
use std::f64::consts::{FRAC_PI_2, TAU};

/// Morinus house cusps from RAMC and obliquity. Latitude is accepted for
/// signature symmetry with the other quadrant systems but used only to
/// guard against the polar singularity.
///
/// Returns `None` for `|lat| >= 90°`.
#[must_use]
pub fn morinus_rad(ramc_rad: f64, obliquity_rad: f64, lat_rad: f64) -> Option<HouseCusps> {
    if lat_rad.abs() >= FRAC_PI_2 {
        return None;
    }
    let eps_cos = obliquity_rad.cos();
    let project = |ra_offset_rad: f64| -> f64 {
        let ra = (ramc_rad + ra_offset_rad).rem_euclid(TAU);
        f64::atan2(ra.sin(), ra.cos() * eps_cos).rem_euclid(TAU)
    };

    let deg30 = 30.0_f64.to_radians();
    let mc = mc_rad(ramc_rad, obliquity_rad);
    let mut cusps = [0.0_f64; 12];
    for k in 1_u8..=12 {
        // H10 sits at RAMC; H11 at RAMC + 30°; …; H9 at RAMC + 330°.
        // Offsets cycle modulo 12 with H10 as the reference.
        let offset = (f64::from(k) - 10.0) * deg30;
        cusps[(k - 1) as usize] = project(offset);
    }
    // Pin H10 to mc_rad's value exactly — the projection formula above
    // is algebraically identical, but this defends against floating-point
    // drift in downstream tests.
    cusps[9] = mc;

    Some(HouseCusps(cusps))
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_abs_diff_eq;
    use std::f64::consts::PI;

    const J2000_EPS_RAD: f64 = 0.409_092_62;

    #[test]
    fn polar_returns_none() {
        assert!(morinus_rad(0.0, J2000_EPS_RAD, FRAC_PI_2).is_none());
    }

    #[test]
    fn h10_is_mc() {
        let ramc = 200.0_f64.to_radians();
        let eps = J2000_EPS_RAD;
        let lat = 40.0_f64.to_radians();
        let hc = morinus_rad(ramc, eps, lat).unwrap();
        assert_abs_diff_eq!(hc.cusp(10), mc_rad(ramc, eps), epsilon = 1e-10);
    }

    #[test]
    fn opposite_cusps_are_180_apart() {
        let ramc = 200.0_f64.to_radians();
        let eps = J2000_EPS_RAD;
        let lat = 40.0_f64.to_radians();
        let hc = morinus_rad(ramc, eps, lat).unwrap();
        for (a, b) in [(1_u8, 7_u8), (2, 8), (3, 9), (4, 10), (5, 11), (6, 12)] {
            let diff = (hc.cusp(b) - hc.cusp(a)).rem_euclid(TAU);
            assert_abs_diff_eq!(diff.to_degrees(), 180.0, epsilon = 1e-9);
        }
    }

    #[test]
    fn cusps_are_30_deg_in_ra_space() {
        // Verifying the equator-division property: when we project the
        // 12 cusps back into RA (via atan2 inverse), consecutive cusps
        // are exactly 30° apart.
        let ramc = 200.0_f64.to_radians();
        let eps = J2000_EPS_RAD;
        let lat = 40.0_f64.to_radians();
        let hc = morinus_rad(ramc, eps, lat).unwrap();
        for i in 0_u8..12 {
            let lon = hc.0[usize::from(i)];
            let ra_back = f64::atan2(lon.sin() * eps.cos(), lon.cos()).rem_euclid(TAU);
            let expected_ra = (ramc + (f64::from(i) - 9.0) * 30.0_f64.to_radians()).rem_euclid(TAU);
            let diff = (ra_back - expected_ra).rem_euclid(TAU);
            let signed = if diff > PI { diff - TAU } else { diff };
            assert_abs_diff_eq!(signed.to_degrees(), 0.0, epsilon = 1e-6);
        }
    }
}
