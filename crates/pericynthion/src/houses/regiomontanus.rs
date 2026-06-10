//! Regiomontanus house system.
//!
//! Divides the celestial equator into twelve 30° arcs from the RAMC,
//! then projects each division onto the ecliptic via the great circle
//! through the horizon's north and south points. House 1 (ASC) and
//! house 10 (MC) coincide with the standard formulas; the intermediate
//! cusps differ from Placidus/Porphyry by depending on the lat/RAMC
//! geometry rather than ecliptic-arc trisection.
//!
//! Returns `None` for `|lat| ≥ 90°`.
//!
//! Cusp formula, with `H` the equatorial-arc offset from RAMC:
//! ```text
//!   RA  = (RAMC + H) mod 2π
//!   λ   = atan2( sin(RA),
//!                cos(RA)·cos(ε) − tan(φ)·sin(ε)·sin(H) )
//! ```
//! Reducing to the standard Asc/MC formulas at `H = π/2` and `H = 0`.

use super::HouseCusps;
use crate::coords::acds::ac_rad;
use crate::coords::mcic::{ic_rad, mc_rad};
use std::f64::consts::{FRAC_PI_2, PI, TAU};

/// Regiomontanus house cusps.
///
/// `ramc_rad`: Right Ascension of the Midheaven (Local Apparent Sidereal Time).
/// `obliquity_rad`: true obliquity of the ecliptic.
/// `lat_rad`: geographic latitude (north positive).
///
/// Returns `None` at the geographic poles where `tan(lat)` diverges.
#[must_use]
pub fn regiomontanus_rad(ramc_rad: f64, obliquity_rad: f64, lat_rad: f64) -> Option<HouseCusps> {
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

    let cusp_for_h = |h_rad: f64| -> f64 {
        let ra = (ramc_rad + h_rad).rem_euclid(TAU);
        let y = ra.sin();
        let x = ra.cos() * eps_cos - lat_tan * eps_sin * h_rad.sin();
        y.atan2(x).rem_euclid(TAU)
    };

    let deg30 = 30.0_f64.to_radians();
    let h11 = cusp_for_h(deg30);
    let h12 = cusp_for_h(2.0 * deg30);
    let h2 = cusp_for_h(4.0 * deg30);
    let h3 = cusp_for_h(5.0 * deg30);
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
        assert!(regiomontanus_rad(0.0, J2000_EPS_RAD, FRAC_PI_2).is_none());
        assert!(regiomontanus_rad(0.0, J2000_EPS_RAD, -FRAC_PI_2).is_none());
    }

    #[test]
    fn equator_intermediate_cusps_collapse_to_atan2() {
        // At φ = 0, tan(φ) = 0 so the formula reduces to:
        //   λ = atan2(sin(RA), cos(RA)·cos(ε))
        // For each H ∈ {30°, 60°, 120°, 150°}.
        let ramc = 100.0_f64.to_radians();
        let eps = J2000_EPS_RAD;
        let hc = regiomontanus_rad(ramc, eps, 0.0).unwrap();

        let expected = |h_deg: f64| {
            let ra = (ramc + h_deg.to_radians()).rem_euclid(TAU);
            f64::atan2(ra.sin(), ra.cos() * eps.cos()).rem_euclid(TAU)
        };
        for (h, h_deg) in [(11_u8, 30.0_f64), (12, 60.0), (2, 120.0), (3, 150.0)] {
            assert_abs_diff_eq!(
                hc.cusp(h).to_degrees(),
                expected(h_deg).to_degrees(),
                epsilon = 1e-9
            );
        }
    }

    #[test]
    fn h1_h4_h7_h10_match_standard_angles() {
        let ramc = 200.0_f64.to_radians();
        let eps = J2000_EPS_RAD;
        let lat = 40.0_f64.to_radians();
        let hc = regiomontanus_rad(ramc, eps, lat).unwrap();
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
        let hc = regiomontanus_rad(ramc, eps, lat).unwrap();
        for (a, b) in [(1_u8, 7_u8), (2, 8), (3, 9), (4, 10), (5, 11), (6, 12)] {
            let diff = (hc.cusp(b) - hc.cusp(a)).rem_euclid(TAU);
            assert_abs_diff_eq!(diff.to_degrees(), 180.0, epsilon = 1e-9);
        }
    }
}
