//! Porphyry house system.
//!
//! Trisects the diurnal and nocturnal *ecliptic* arcs between angles —
//! a pure-longitude scheme with no spherical-trig dependence on
//! latitude. House 1 (ASC) and 10 (MC) coincide with the standard
//! formulas. The arc MC → ASC is trisected for cusps 11 and 12; the
//! arc ASC → IC is trisected for cusps 2 and 3.
//!
//! Inputs are ASC and MC in radians; no `lat`/`obliquity` needed at this
//! layer (they were already absorbed by [`crate::coords::acds`]
//! and [`crate::coords::mcic`]).

use super::HouseCusps;
use std::f64::consts::{PI, TAU};

/// Porphyry house cusps from precomputed ASC and MC longitudes (radians).
#[must_use]
pub fn porphyry_rad(ac_rad: f64, mc_rad: f64) -> HouseCusps {
    let ac = ac_rad.rem_euclid(TAU);
    let mc = mc_rad.rem_euclid(TAU);
    let ic = (mc + PI).rem_euclid(TAU);
    let ds = (ac + PI).rem_euclid(TAU);

    // Upper-east arc MC → ASC, trisected.
    let upper = (ac - mc).rem_euclid(TAU);
    let h11 = (mc + upper / 3.0).rem_euclid(TAU);
    let h12 = (mc + 2.0 * upper / 3.0).rem_euclid(TAU);

    // Lower-east arc ASC → IC, trisected.
    let lower = (ic - ac).rem_euclid(TAU);
    let h2 = (ac + lower / 3.0).rem_euclid(TAU);
    let h3 = (ac + 2.0 * lower / 3.0).rem_euclid(TAU);

    let h5 = (h11 + PI).rem_euclid(TAU);
    let h6 = (h12 + PI).rem_euclid(TAU);
    let h8 = (h2 + PI).rem_euclid(TAU);
    let h9 = (h3 + PI).rem_euclid(TAU);

    HouseCusps([ac, h2, h3, ic, h5, h6, ds, h8, h9, mc, h11, h12])
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_abs_diff_eq;

    #[test]
    fn h1_is_asc_h10_is_mc() {
        let ac = 151.484_f64.to_radians();
        let mc = 57.636_f64.to_radians();
        let hc = porphyry_rad(ac, mc);
        assert_abs_diff_eq!(hc.cusp(1), ac, epsilon = 1e-12);
        assert_abs_diff_eq!(hc.cusp(10), mc, epsilon = 1e-12);
    }

    #[test]
    fn opposite_cusps_are_180_apart() {
        let ac = 130.0_f64.to_radians();
        let mc = 40.0_f64.to_radians();
        let hc = porphyry_rad(ac, mc);
        for (a, b) in [(1_u8, 7_u8), (2, 8), (3, 9), (4, 10), (5, 11), (6, 12)] {
            let diff = (hc.cusp(b) - hc.cusp(a)).rem_euclid(TAU);
            assert_abs_diff_eq!(diff.to_degrees(), 180.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn arc_trisection_is_exact() {
        // ASC at 100°, MC at 10° → upper arc = 90°, lower arc = 90°.
        let ac = 100.0_f64.to_radians();
        let mc = 10.0_f64.to_radians();
        let hc = porphyry_rad(ac, mc);
        assert_abs_diff_eq!(hc.cusp(11).to_degrees(), 40.0, epsilon = 1e-9);
        assert_abs_diff_eq!(hc.cusp(12).to_degrees(), 70.0, epsilon = 1e-9);
        assert_abs_diff_eq!(hc.cusp(2).to_degrees(), 130.0, epsilon = 1e-9);
        assert_abs_diff_eq!(hc.cusp(3).to_degrees(), 160.0, epsilon = 1e-9);
    }

    #[test]
    fn valens_porphyry_h11_matches_refchart() {
        // Asc Vir⌖01°29'03" = 151.484167°, MC Tau⌖27°38'08" = 57.635556°.
        // Upper arc = 93.848611°, /3 = 31.282870°.
        // H11 = MC + 31.282870° = 88.918426° → Gem⌖28°55'06".
        let ac = 151.484_167_f64.to_radians();
        let mc = 57.635_556_f64.to_radians();
        let hc = porphyry_rad(ac, mc);
        let expected_h11 = 60.0 + 28.0 + 55.0 / 60.0 + 6.0 / 3600.0_f64; // Gem⌖28°55'06"
        assert_abs_diff_eq!(hc.cusp(11).to_degrees(), expected_h11, epsilon = 1.0 / 60.0);
    }
}
