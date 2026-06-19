//! Morinus (Morin de Villefranche) house system — gated behind
//! `noref-houses` until a refchart oracle is captured. See
//! `docs/discovery/HOUSE_PROMOTION.md`.
//!
//! Divides the celestial equator into twelve 30° arcs starting from the
//! RAMC, then projects each division onto the ecliptic. No latitude
//! dependence in the intermediate cusps — Morinus is the only quadrant
//! system that remains well-defined at all geographic latitudes
//! including the poles. Note that **H1 in Morinus is not the ASC** and
//! **H10 is not the MC**: each cusp is the ecliptic image of an equator
//! point (declination 0) at RAMC + k·30°, via λ = atan2(sin α · cos ε,
//! cos α). H10 (the RAMC image) is close to — but not equal to — the
//! Midheaven.

use super::HouseCusps;
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
    // Project the equator division point at (RAMC + offset), declination 0,
    // onto the ecliptic: λ = atan2(sin α · cos ε, cos α). This is the
    // equator→ecliptic transform for a point ON the equator — NOT the MC
    // transform, so H10 (offset 0) differs from the Midheaven.
    let project = |ra_offset_rad: f64| -> f64 {
        let ra = (ramc_rad + ra_offset_rad).rem_euclid(TAU);
        f64::atan2(ra.sin() * eps_cos, ra.cos()).rem_euclid(TAU)
    };

    let deg30 = 30.0_f64.to_radians();
    let mut cusps = [0.0_f64; 12];
    for k in 1_u8..=12 {
        // H10 sits at RAMC; H11 at RAMC + 30°; …; H9 at RAMC + 330°.
        // Offsets cycle modulo 12 with H10 as the reference.
        let offset = (f64::from(k) - 10.0) * deg30;
        cusps[(k - 1) as usize] = project(offset);
    }

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
    fn h10_differs_from_mc() {
        use crate::coords::mcic::mc_rad;
        let ramc = 200.0_f64.to_radians();
        let eps = J2000_EPS_RAD;
        let lat = 40.0_f64.to_radians();
        let hc = morinus_rad(ramc, eps, lat).unwrap();
        // Morinus H10 is the ecliptic image of the equator point at RAMC
        // (declination 0) — NOT the MC. For nonzero obliquity they differ.
        assert!(
            (hc.cusp(10) - mc_rad(ramc, eps)).abs() > 1e-4,
            "Morinus H10 must differ from the MC"
        );
    }

    #[test]
    fn matches_first_contact_oracle() {
        // docs/ref_first_contact_morinus.md — driven by the reference's own
        // RAMC (LST 23:38:54) and obliquity (23°25'51"). Latitude is irrelevant
        // to Morinus but supplied for signature symmetry. No ephemeris/ΔT path,
        // so this is a tight (1′) check of the projection math itself.
        let ramc = ((23.0 + 38.0 / 60.0 + 54.0 / 3600.0) * 15.0_f64).to_radians();
        let eps = (23.0 + 25.0 / 60.0 + 51.0 / 3600.0_f64).to_radians();
        let lat = (45.0 + 40.0 / 60.0 + 47.0 / 3600.0_f64).to_radians();
        let oracle_deg = [
            60.0 + 24.0 + 15.0 / 60.0 + 20.0 / 3600.0, // H1  Gem 24°15'20"
            90.0 + 26.0 + 39.0 / 60.0 + 7.0 / 3600.0,  // H2  Can 26°39'07"
            120.0 + 27.0 + 0.0 / 60.0 + 57.0 / 3600.0, // H3  Leo 27°00'57"
            150.0 + 25.0 + 9.0 / 60.0 + 33.0 / 3600.0, // H4  Vir 25°09'33"
            180.0 + 22.0 + 54.0 / 60.0 + 21.0 / 3600.0, // H5  Lib 22°54'21"
            210.0 + 22.0 + 22.0 / 60.0 + 15.0 / 3600.0, // H6  Sco 22°22'15"
            240.0 + 24.0 + 15.0 / 60.0 + 20.0 / 3600.0, // H7  Sag 24°15'20"
            270.0 + 26.0 + 39.0 / 60.0 + 7.0 / 3600.0, // H8  Cap 26°39'07"
            300.0 + 27.0 + 0.0 / 60.0 + 57.0 / 3600.0, // H9  Aqu 27°00'57"
            330.0 + 25.0 + 9.0 / 60.0 + 33.0 / 3600.0, // H10 Pis 25°09'33"
            0.0 + 22.0 + 54.0 / 60.0 + 21.0 / 3600.0,  // H11 Ari 22°54'21"
            30.0 + 22.0 + 22.0 / 60.0 + 15.0 / 3600.0, // H12 Tau 22°22'15"
        ];
        let hc = morinus_rad(ramc, eps, lat).unwrap();
        for h in 1u8..=12 {
            let got = hc.cusp(h).to_degrees();
            let want = oracle_deg[(h - 1) as usize];
            let d_arcmin = (((got - want + 180.0).rem_euclid(360.0)) - 180.0).abs() * 60.0;
            assert!(
                d_arcmin < 1.0,
                "H{h}: got {got:.4}°, want {want:.4}° (Δ {d_arcmin:.2}′)"
            );
        }
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
            // Inverse of λ = atan2(sin α · cos ε, cos α): α = atan2(sin λ, cos λ · cos ε).
            let ra_back = f64::atan2(lon.sin(), lon.cos() * eps.cos()).rem_euclid(TAU);
            let expected_ra = (ramc + (f64::from(i) - 9.0) * 30.0_f64.to_radians()).rem_euclid(TAU);
            let diff = (ra_back - expected_ra).rem_euclid(TAU);
            let signed = if diff > PI { diff - TAU } else { diff };
            assert_abs_diff_eq!(signed.to_degrees(), 0.0, epsilon = 1e-6);
        }
    }
}
