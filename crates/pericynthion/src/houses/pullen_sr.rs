#![cfg(feature = "noref-houses")]
#![allow(clippy::cast_precision_loss, clippy::needless_range_loop)]

//! Pullen Sinusoidal Ratio (SR) house system — gated behind `noref-houses`
//! until a refchart oracle is captured.
//!
//! H1=Asc and H10=MC are fixed.  The smaller of the two upper-hemisphere
//! quadrant arcs is divided into house sizes `r·x`, `x`, `r·x`; the larger
//! quadrant gets `r³·x`, `r⁴·x`, `r³·x`.  The ratio `r` and base size `x`
//! are solved analytically from the smaller arc alone.  Collapses to equal
//! 30° houses when both quadrants are 90°.
//!
//! Source: Walter D. Pullen, Astrolog v7.80
//! `HousePullenSinusoidalRatio` — <https://github.com/CruiserOne/Astrolog/blob/v7.80/calc.cpp>

use super::HouseCusps;
use std::f64::consts::{FRAC_PI_2, PI, TAU};

/// Pullen Sinusoidal Ratio house cusps.
///
/// Inputs: Ascendant and MC longitudes in radians.  Latitude-independent.
/// H1 = Asc, H10 = MC exactly.  Lower houses are exact opposites of H7–H12.
#[must_use]
pub fn pullen_sr_rad(ac_rad: f64, mc_rad: f64) -> HouseCusps {
    // Forward arc (zodiacally) from MC to Asc.
    let q_mc_asc = (ac_rad - mc_rad).rem_euclid(TAU);

    // Identify the smaller upper-hemisphere quadrant.
    // MC→Asc arc + Dsc→MC arc = π.  Whichever is ≤ π/2 is the small one.
    let (q_small, mc_asc_is_small) = if q_mc_asc <= FRAC_PI_2 {
        (q_mc_asc, true)
    } else {
        (PI - q_mc_asc, false)
    };

    let h7 = (ac_rad + PI).rem_euclid(TAU);
    let (ratio, x) = solve_ratio(q_small);
    let r3 = ratio.powi(3);

    let (h8, h9, h11, h12) = if mc_asc_is_small {
        // Small = MC→Asc: H10→H11→H12→H1 sizes xr, x, xr
        // Large = H7→MC:  H7→H8→H9→H10 sizes xr³, xr⁴, xr³
        let h11 = (mc_rad + x * ratio).rem_euclid(TAU);
        let h12 = (ac_rad - x * ratio).rem_euclid(TAU);
        let h9 = (mc_rad - x * r3).rem_euclid(TAU);
        let h8 = (h7 + x * r3).rem_euclid(TAU);
        (h8, h9, h11, h12)
    } else {
        // Small = H7→MC: H7→H8→H9→H10 sizes xr, x, xr
        // Large = MC→Asc: H10→H11→H12→H1 sizes xr³, xr⁴, xr³
        let h8 = (h7 + x * ratio).rem_euclid(TAU);
        let h9 = (mc_rad - x * ratio).rem_euclid(TAU);
        let h11 = (mc_rad + x * r3).rem_euclid(TAU);
        let h12 = (ac_rad - x * r3).rem_euclid(TAU);
        (h8, h9, h11, h12)
    };

    let opp = |v: f64| (v + PI).rem_euclid(TAU);
    HouseCusps([
        ac_rad,      // H1  = Asc
        opp(h8),     // H2
        opp(h9),     // H3
        opp(mc_rad), // H4  = IC
        opp(h11),    // H5
        opp(h12),    // H6
        h7,          // H7  = Dsc
        h8,          // H8
        h9,          // H9
        mc_rad,      // H10 = MC
        h11,         // H11
        h12,         // H12
    ])
}

// Solve for (ratio r, house base x) given the smaller quadrant arc q (radians).
// From Astrolog: rx + x + rx = q (small), xr³ + xr⁴ + xr³ = π − q (large).
// The polynomial constants are in degrees; q is converted before use.
fn solve_ratio(q_small_rad: f64) -> (f64, f64) {
    if q_small_rad <= 0.0 {
        return (0.0, 0.0);
    }
    let q = q_small_rad.to_degrees();
    let rlo = 2.0 * (q * q - 270.0 * q + 16_200.0).cbrt() / q.powf(2.0 / 3.0);
    let rhi = (rlo + 1.0).sqrt();
    let disc = (-6.0 * (q - 120.0) / (q * rhi) - rlo + 2.0).max(0.0);
    let ratio = 0.5 * rhi + 0.5 * disc.sqrt() - 0.5;
    let x = q_small_rad / (2.0 * ratio + 1.0);
    (ratio, x)
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_abs_diff_eq;

    const EPS: f64 = 1e-7;

    #[test]
    fn h1_is_asc_and_h10_is_mc() {
        let mc = 1.5_f64;
        let ac = (mc + FRAC_PI_2).rem_euclid(TAU);
        let hc = pullen_sr_rad(ac, mc);
        assert_abs_diff_eq!(hc.cusp(1), ac, epsilon = EPS);
        assert_abs_diff_eq!(hc.cusp(10), mc, epsilon = EPS);
    }

    #[test]
    fn equal_quadrants_give_30_degree_houses() {
        // q_mc_asc = 90° → r = 1, x = 30°, all houses equal.
        let mc = 1.0_f64;
        let ac = (mc + FRAC_PI_2).rem_euclid(TAU);
        let hc = pullen_sr_rad(ac, mc);
        for k in 0..12_usize {
            let gap = (hc.0[(k + 1) % 12] - hc.0[k]).rem_euclid(TAU).to_degrees();
            assert_abs_diff_eq!(gap, 30.0, epsilon = 1e-5);
        }
    }

    #[test]
    fn opposite_cusps_are_180_apart() {
        let mc = 2.1_f64;
        let ac = (mc + 1.2).rem_euclid(TAU);
        let hc = pullen_sr_rad(ac, mc);
        for (a, b) in [(1_u8, 7_u8), (2, 8), (3, 9), (4, 10), (5, 11), (6, 12)] {
            let diff = (hc.cusp(b) - hc.cusp(a)).rem_euclid(TAU).to_degrees();
            assert_abs_diff_eq!(diff, 180.0, epsilon = EPS);
        }
    }

    #[test]
    fn cusps_advance_in_zodiacal_order() {
        let mc = 0.5_f64;
        let ac = (mc + 1.4).rem_euclid(TAU);
        let hc = pullen_sr_rad(ac, mc);
        for k in 0..12_usize {
            let gap = (hc.0[(k + 1) % 12] - hc.0[k]).rem_euclid(TAU).to_degrees();
            assert!(
                gap > 0.0 && gap < 180.0,
                "gap {gap:.3}° out of range at k={k}"
            );
        }
    }

    #[test]
    fn small_quadrant_sums_correctly() {
        // H10→H11→H12→H1 arc should equal q_mc_asc when it is the small quadrant.
        let mc = 0.3_f64;
        let ac = (mc + 0.8).rem_euclid(TAU); // q_mc_asc ≈ 45.8° < 90°
        let hc = pullen_sr_rad(ac, mc);
        let arc_h10_h1 = (hc.cusp(1) - hc.cusp(10)).rem_euclid(TAU);
        let q_mc_asc = (ac - mc).rem_euclid(TAU);
        assert_abs_diff_eq!(arc_h10_h1, q_mc_asc, epsilon = EPS);
    }

    #[test]
    fn ratio_is_one_at_equal_quadrants() {
        let (r, x) = solve_ratio(FRAC_PI_2);
        assert_abs_diff_eq!(r, 1.0, epsilon = 1e-5);
        assert_abs_diff_eq!(x.to_degrees(), 30.0, epsilon = 1e-4);
    }
}
