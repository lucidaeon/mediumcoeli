#![allow(clippy::cast_precision_loss, clippy::needless_range_loop)]

//! Equal house system (from Ascendant).
//!
//! Each cusp is exactly 30° from the previous. House 1 cusp = ASC degree
//! (not the sign boundary — unlike Whole Sign).

use super::HouseCusps;
use std::f64::consts::{FRAC_PI_6, TAU};

/// Equal-from-ASC house cusps (radians).
///
/// House `n` cusp = `(ac + (n-1) × 30°) mod 360°`.
#[must_use]
pub fn equal_as_rad(ac_rad: f64) -> HouseCusps {
    let ac = ac_rad.rem_euclid(TAU);
    let mut cusps = [0.0_f64; 12];
    for i in 0..12 {
        cusps[i] = (ac + i as f64 * FRAC_PI_6).rem_euclid(TAU);
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
        let hc = equal_as_rad(ac);
        for i in 1..=12_u8 {
            let c = hc.cusp(i).to_degrees();
            let expected = (45.0 + f64::from(i - 1) * 30.0).rem_euclid(360.0);
            assert_abs_diff_eq!(c, expected, epsilon = EPS);
        }
    }

    #[test]
    fn h7_is_opposite_h1() {
        let ac = 317.671_f64.to_radians(); // reference chart: AC=317.671°
        let hc = equal_as_rad(ac);
        let h1 = hc.cusp(1).to_degrees();
        let h7 = hc.cusp(7).to_degrees();
        let diff = (h7 - h1).rem_euclid(360.0);
        assert_abs_diff_eq!(diff, 180.0, epsilon = EPS);
    }

    #[test]
    fn h10_is_mc_equivalent() {
        // H10 cusp = ASC + 270° = the point opposite to H4 (ASC + 90°).
        let ac = 100_f64.to_radians();
        let hc = equal_as_rad(ac);
        assert_abs_diff_eq!(
            hc.cusp(10).to_degrees(),
            370.0_f64.rem_euclid(360.0),
            epsilon = EPS
        );
    }

    #[test]
    fn house_of_places_body_correctly() {
        let ac = 0_f64.to_radians(); // ASC at 0°
        let hc = equal_as_rad(ac);
        assert_eq!(hc.house_of(15_f64.to_radians()), 1); // 15° → H1
        assert_eq!(hc.house_of(35_f64.to_radians()), 2); // 35° → H2
        assert_eq!(hc.house_of(350_f64.to_radians()), 12); // 350° → H12
    }

    #[test]
    fn h1_is_asc() {
        // ASC: Aqu⌖17°40'17" = 317.671°. H1 cusp must equal ASC exactly.
        let ac_deg = 300.0 + 17.0 + 40.0 / 60.0 + 17.0 / 3600.0_f64;
        let hc = equal_as_rad(ac_deg.to_radians());
        assert_abs_diff_eq!(hc.cusp(1).to_degrees(), ac_deg, epsilon = EPS);
    }
}
