#![allow(clippy::similar_names)]

//! Koch (Birthplace) house system. Reference-verified against a Solar Fire
//! chart (`skills/astrologer/fixtures/ref_alan_turing_koch.md`); see the
//! `matches_koch_refchart_alan_turing` test below.

use super::HouseCusps;
use crate::coords::acds::ac_rad;
use crate::coords::mcic::{ic_rad, mc_rad};
use std::f64::consts::{FRAC_PI_2, PI, TAU};

/// Koch (Birthplace) house cusps in radians \[0, TAU), keyed by RAMC,
/// obliquity, and geographic latitude.
///
/// Returns `None` when `|lat_rad| ≥ π/2` or when the MC point is
/// circumpolar at the given latitude (`tan(φ)·tan(δ_mc) ≥ 1`).
#[must_use]
pub fn koch_rad(ramc_rad: f64, obliquity_rad: f64, lat_rad: f64) -> Option<HouseCusps> {
    if lat_rad.abs() >= FRAC_PI_2 {
        return None;
    }
    let lat_tan = lat_rad.tan();

    let mc = mc_rad(ramc_rad, obliquity_rad);
    let ac = ac_rad(ramc_rad, obliquity_rad, lat_rad)?;
    let ic = ic_rad(mc);
    let ds = (ac + PI).rem_euclid(TAU);

    // Declination of the MC point and its ascensional difference.
    let delta_mc = (obliquity_rad.sin() * mc.sin()).clamp(-1.0, 1.0).asin();
    let ad_arg = lat_tan * delta_mc.tan();
    if ad_arg.abs() >= 1.0 {
        return None; // MC point is circumpolar — Koch undefined at this lat.
    }
    let ad_mc = ad_arg.asin();

    // Trisect the MC's diurnal semi-arc (DSA_MC = π/2 + AD_MC): cusp N is the
    // ecliptic degree sharing the oblique ascension of the point at
    // RA = RAMC + M·DSA_MC/3, declination D_MC (M = 1, 2, 4, 5 for cusps 11, 12,
    // 2, 3). In this crate's Ascendant convention the ecliptic point whose
    // oblique ascension is θ is `ac_rad(θ − π/2)`. Source: Makransky, *Primary
    // Directions: A Primer of Calculation* (1992), p. 69.
    let dsa_mc = FRAC_PI_2 + ad_mc;
    let cusp = |m: f64| {
        ac_rad(
            ramc_rad + m * dsa_mc / 3.0 - ad_mc - FRAC_PI_2,
            obliquity_rad,
            lat_rad,
        )
    };
    let h11 = cusp(1.0)?;
    let h12 = cusp(2.0)?;
    let h2 = cusp(4.0)?;
    let h3 = cusp(5.0)?;
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
    use crate::coords::acds::ac_rad;
    use crate::coords::mcic::{ic_rad, mc_rad};
    use approx::assert_abs_diff_eq;

    const J2000_EPS_RAD: f64 = 0.409_092_62;

    #[test]
    fn polar_returns_none() {
        assert!(koch_rad(0.0, J2000_EPS_RAD, FRAC_PI_2).is_none());
        assert!(koch_rad(0.0, J2000_EPS_RAD, -FRAC_PI_2).is_none());
    }

    #[test]
    fn h1_h4_h7_h10_match_standard_angles() {
        let ramc = 200.0_f64.to_radians();
        let eps = J2000_EPS_RAD;
        let lat = 40.0_f64.to_radians();
        let hc = koch_rad(ramc, eps, lat).unwrap();
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
        let hc = koch_rad(ramc, eps, lat).unwrap();
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
        let hc = koch_rad(ramc, eps, lat).unwrap();
        let mut total = 0.0_f64;
        for i in 0..12 {
            let span = (hc.0[(i + 1) % 12] - hc.0[i]).rem_euclid(TAU).to_degrees();
            assert!(span > 0.0 && span < 180.0, "H{} span {:.3}°", i + 1, span);
            total += span;
        }
        assert_abs_diff_eq!(total, 360.0, epsilon = 1e-6);
    }

    #[test]
    fn matches_koch_refchart_alan_turing() {
        // Oracle: skills/astrologer/fixtures/ref_alan_turing_koch.md
        // (Solar Fire, Koch system). LST 20:17:52, ε 23°27′02″, lat 51°N30′.
        // Formula: Makransky, Primary Directions (1992), p. 69.
        let ramc = ((20.0 + 17.0 / 60.0 + 52.0 / 3600.0) * 15.0_f64).to_radians();
        let eps = (23.0 + 27.0 / 60.0 + 2.0 / 3600.0_f64).to_radians();
        let lat = 51.5_f64.to_radians();
        let hc = koch_rad(ramc, eps, lat).unwrap();
        // Fixture cusp longitudes H1..H12 (decimal degrees).
        let want = [
            65.6583, 88.7156, 106.5194, 122.2017, 161.8108, 210.5311, 245.6583, 268.7156, 286.5194,
            302.2017, 341.8108, 30.5311,
        ];
        for (i, &w) in want.iter().enumerate() {
            let got = hc.0[i].to_degrees();
            let diff = (got - w + 180.0).rem_euclid(360.0) - 180.0;
            assert!(
                diff.abs() < 0.01,
                "cusp {} = {got:.4}°, want {w:.4}° (Δ {diff:.4}°)",
                i + 1
            );
        }
    }
}
