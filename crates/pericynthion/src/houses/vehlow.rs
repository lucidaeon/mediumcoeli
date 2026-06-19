#![cfg(feature = "noref-houses")]
#![allow(clippy::cast_precision_loss, clippy::needless_range_loop)]

//! Vehlow Equal house system — gated behind `noref-houses` until a refchart
//! oracle is captured.
//!
//! Equal houses anchored so the Ascendant falls at the midpoint of H1.
//! H1 cusp = λAsc − 15°; subsequent cusps every 30°.

use super::HouseCusps;
use std::f64::consts::{FRAC_PI_6, TAU};

const FRAC_PI_12: f64 = std::f64::consts::PI / 12.0; // 15°

/// Vehlow Equal house cusps from the Ascendant longitude (radians).
///
/// H1 cusp = `(ac − 15°) mod 360°`; each subsequent cusp is 30° further.
#[must_use]
pub fn vehlow_rad(ac_rad: f64) -> HouseCusps {
    let h1 = (ac_rad - FRAC_PI_12).rem_euclid(TAU);
    let mut cusps = [0.0_f64; 12];
    for k in 0..12 {
        cusps[k] = (h1 + k as f64 * FRAC_PI_6).rem_euclid(TAU);
    }
    HouseCusps(cusps)
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_abs_diff_eq;

    const EPS: f64 = 1e-10;

    #[test]
    fn cusps_are_30_degrees_apart() {
        let ac = 45_f64.to_radians();
        let hc = vehlow_rad(ac);
        for k in 0..12_usize {
            let next = hc.0[(k + 1) % 12];
            let curr = hc.0[k];
            let gap = (next - curr).rem_euclid(TAU).to_degrees();
            assert_abs_diff_eq!(gap, 30.0, epsilon = EPS);
        }
    }

    #[test]
    fn asc_is_midpoint_of_h1() {
        let ac_deg = 45.0_f64;
        let hc = vehlow_rad(ac_deg.to_radians());
        let h1 = hc.cusp(1).to_degrees();
        let h2 = hc.cusp(2).to_degrees();
        // Asc should lie exactly 15° inside H1.
        let mid = (h1 + 15.0).rem_euclid(360.0);
        assert_abs_diff_eq!(mid, ac_deg, epsilon = EPS);
        let _ = h2; // h2 = h1 + 30°; structural invariant checked above
    }

    #[test]
    fn h1_is_15_degrees_before_asc() {
        let ac_deg = 200.0_f64;
        let hc = vehlow_rad(ac_deg.to_radians());
        let h1 = hc.cusp(1).to_degrees();
        assert_abs_diff_eq!(h1, 185.0, epsilon = EPS);
    }

    #[test]
    fn h7_is_opposite_h1() {
        let ac = 317.671_f64.to_radians();
        let hc = vehlow_rad(ac);
        let diff = (hc.cusp(7) - hc.cusp(1)).rem_euclid(TAU).to_degrees();
        assert_abs_diff_eq!(diff, 180.0, epsilon = EPS);
    }
}
