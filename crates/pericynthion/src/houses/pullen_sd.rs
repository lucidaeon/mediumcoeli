#![cfg(feature = "noref-houses")]
#![allow(clippy::cast_precision_loss, clippy::needless_range_loop)]

//! Pullen Sinusoidal Delta (SD) house system — gated behind `noref-houses`
//! until a refchart oracle is captured.
//!
//! A generalisation of Porphyry: H1=Asc and H10=MC are fixed; the three
//! intermediate cusps in each upper-hemisphere quadrant are spaced by adding
//! a linear delta offset `Δ = (q − 90°) / 4` to each 30° step, where `q`
//! is the minimum arc between MC and Asc.  One side gains what the other
//! loses.  Collapses to Porphyry when `q = 90°`.
//!
//! Source: Walter D. Pullen, Astrolog v7.80
//! `HousePullenSinusoidalDelta` — <https://github.com/CruiserOne/Astrolog/blob/v7.80/calc.cpp>

use super::HouseCusps;
use std::f64::consts::{FRAC_PI_2, FRAC_PI_6, PI, TAU};

/// Pullen Sinusoidal Delta house cusps.
///
/// Inputs: Ascendant and MC longitudes in radians.  Latitude-independent.
/// H1 = Asc, H10 = MC exactly.  Lower houses are exact opposites of H7–H12.
#[must_use]
pub fn pullen_sd_rad(ac_rad: f64, mc_rad: f64) -> HouseCusps {
    // Minimum arc between MC and Asc (always ≤ π).
    let q = min_arc(mc_rad, ac_rad);
    let delta = (q - FRAC_PI_2) / 4.0;
    let deg30 = FRAC_PI_6;

    let h7 = (ac_rad + PI).rem_euclid(TAU);

    // Upper-right quadrant: MC → H11 → H12 → Asc (going zodiacally forward).
    let (h11, h12) = if q >= deg30 {
        let h11 = (mc_rad + deg30 + delta).rem_euclid(TAU);
        let h12 = (h11 + deg30 + 2.0 * delta).rem_euclid(TAU);
        (h11, h12)
    } else {
        // Quadrant narrower than 30°: collapse both intermediates to midpoint.
        let mid = arc_midpoint(mc_rad, ac_rad);
        (mid, mid)
    };

    // Upper-left quadrant: H7 → H8 → H9 → MC (going zodiacally forward).
    let (h9, h8) = if q <= 5.0 * deg30 {
        let h9 = (mc_rad - deg30 + delta).rem_euclid(TAU);
        let h8 = (h9 - deg30 + 2.0 * delta).rem_euclid(TAU);
        (h9, h8)
    } else {
        // Opposite quadrant narrower than 30°: collapse both intermediates.
        let mid = arc_midpoint(mc_rad, h7);
        (mid, mid)
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

// Minimum arc between two longitudes (always ≤ π, always ≥ 0).
fn min_arc(a: f64, b: f64) -> f64 {
    let d = (a - b).abs().rem_euclid(TAU);
    f64::min(d, TAU - d)
}

// Midpoint along the shorter arc between a and b (both in [0, TAU)).
fn arc_midpoint(a: f64, b: f64) -> f64 {
    let mid = f64::midpoint(a, b);
    if (b - a).abs() > PI {
        (mid + PI).rem_euclid(TAU)
    } else {
        mid.rem_euclid(TAU)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_abs_diff_eq;

    const J2000_MC: f64 = 1.0; // arbitrary MC for structural tests
    const EPS: f64 = 1e-9;

    fn equal_quadrant_chart() -> HouseCusps {
        // AC = MC + 90° → q = 90°, Δ = 0, result should be Porphyry-equal spacing.
        let mc = J2000_MC;
        let ac = (mc + FRAC_PI_2).rem_euclid(TAU);
        pullen_sd_rad(ac, mc)
    }

    #[test]
    fn h1_is_asc_and_h10_is_mc() {
        let mc = 1.5_f64;
        let ac = (mc + FRAC_PI_2).rem_euclid(TAU);
        let hc = pullen_sd_rad(ac, mc);
        assert_abs_diff_eq!(hc.cusp(1), ac, epsilon = EPS);
        assert_abs_diff_eq!(hc.cusp(10), mc, epsilon = EPS);
    }

    #[test]
    fn equal_quadrants_give_30_degree_houses() {
        let hc = equal_quadrant_chart();
        for k in 0..12_usize {
            let gap = (hc.0[(k + 1) % 12] - hc.0[k]).rem_euclid(TAU).to_degrees();
            assert_abs_diff_eq!(gap, 30.0, epsilon = 1e-6);
        }
    }

    #[test]
    fn opposite_cusps_are_180_apart() {
        let mc = 2.1_f64;
        let ac = (mc + 1.2).rem_euclid(TAU); // non-equal quadrants
        let hc = pullen_sd_rad(ac, mc);
        for (a, b) in [(1_u8, 7_u8), (2, 8), (3, 9), (4, 10), (5, 11), (6, 12)] {
            let diff = (hc.cusp(b) - hc.cusp(a)).rem_euclid(TAU).to_degrees();
            assert_abs_diff_eq!(diff, 180.0, epsilon = EPS);
        }
    }

    #[test]
    fn cusps_advance_in_zodiacal_order() {
        let mc = 0.5_f64;
        let ac = (mc + 1.4).rem_euclid(TAU);
        let hc = pullen_sd_rad(ac, mc);
        for k in 0..12_usize {
            let gap = (hc.0[(k + 1) % 12] - hc.0[k]).rem_euclid(TAU).to_degrees();
            assert!(
                gap > 0.0 && gap < 180.0,
                "gap {gap:.3}° out of range at k={k}"
            );
        }
    }

    #[test]
    fn narrow_quadrant_collapses_intermediates() {
        // q < 30°: H11 and H12 should equal their midpoint collapse.
        let mc = 0.0_f64;
        let ac = (mc + 10_f64.to_radians()).rem_euclid(TAU); // q = 10°
        let hc = pullen_sd_rad(ac, mc);
        assert_abs_diff_eq!(hc.cusp(11), hc.cusp(12), epsilon = EPS);
    }
}
