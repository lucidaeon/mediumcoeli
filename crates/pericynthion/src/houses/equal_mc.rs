#![cfg(feature = "noref-houses")]
#![allow(clippy::needless_range_loop)]

//! Equal-from-MC house system — gated behind `noref-houses` until a
//! refchart oracle is captured. See `docs/discovery/HOUSE_PROMOTION.md`.
//!
//! H10 cusp = MC; every other cusp is exactly 30° from MC counter-
//! clockwise around the ecliptic. **H1 ≠ ASC** — H1 = MC + 90°.

use super::HouseCusps;
use std::f64::consts::{FRAC_PI_6, TAU};

/// Equal-from-MC house cusps from a single MC longitude (radians).
///
/// House `n` cusp = `(mc + (n − 10) × 30°) mod 360°`, so H10 = MC and
/// H1 = MC + 90°.
#[must_use]
pub fn equal_mc_rad(mc_rad: f64) -> HouseCusps {
    let mc = mc_rad.rem_euclid(TAU);
    let mut cusps = [0.0_f64; 12];
    // H10 sits at MC. H11 = MC + 30°, H12 = MC + 60°, H1 = MC + 90°, ….
    for i in 0..12_u8 {
        let k_minus_10 = f64::from(i) - 9.0; // i=0 → H1 → MC+90°; i=9 → H10 → MC
        cusps[i as usize] = (mc + k_minus_10 * FRAC_PI_6).rem_euclid(TAU);
    }
    HouseCusps(cusps)
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_abs_diff_eq;

    const EPS: f64 = 1e-10;

    #[test]
    fn h10_is_mc() {
        let mc = 57.636_f64.to_radians();
        let hc = equal_mc_rad(mc);
        assert_abs_diff_eq!(hc.cusp(10), mc, epsilon = EPS);
    }

    #[test]
    fn h1_is_mc_plus_90() {
        let mc = 57.636_f64.to_radians();
        let hc = equal_mc_rad(mc);
        let expected = (mc + 90.0_f64.to_radians()).rem_euclid(TAU);
        assert_abs_diff_eq!(hc.cusp(1), expected, epsilon = EPS);
    }

    #[test]
    fn cusps_are_30_deg_apart() {
        let mc = 45.0_f64.to_radians();
        let hc = equal_mc_rad(mc);
        for i in 0..12_u8 {
            let curr = hc.0[i as usize].to_degrees();
            let next = hc.0[(i as usize + 1) % 12].to_degrees();
            let diff = (next - curr).rem_euclid(360.0);
            assert_abs_diff_eq!(diff, 30.0, epsilon = 1e-7);
        }
    }

    #[test]
    fn opposite_cusps_are_180_apart() {
        let mc = 100.0_f64.to_radians();
        let hc = equal_mc_rad(mc);
        for (a, b) in [(1_u8, 7_u8), (2, 8), (3, 9), (4, 10), (5, 11), (6, 12)] {
            let diff = (hc.cusp(b) - hc.cusp(a)).rem_euclid(TAU);
            assert_abs_diff_eq!(diff.to_degrees(), 180.0, epsilon = EPS);
        }
    }
}
