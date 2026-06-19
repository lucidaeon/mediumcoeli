#![cfg(feature = "noref-houses")]
#![allow(clippy::cast_precision_loss, clippy::needless_range_loop)]

//! Carter Poli-Equatorial house system — gated behind `noref-houses` until a
//! refchart oracle is captured.
//!
//! Divides the celestial equator into twelve 30° arcs from the RA of the
//! Ascendant, then projects each equatorial point to the ecliptic by finding
//! where the hour circle at that RA intersects the ecliptic:
//! `λ = atan2(sin α, cos α · cos ε)`.
//!
//! H1 = Asc exactly (round-trip identity). H10 ≠ MC. Latitude-independent.

use super::HouseCusps;
use std::f64::consts::{FRAC_PI_6, TAU};

/// Carter Poli-Equatorial house cusps from the Ascendant longitude and
/// obliquity (radians). Latitude-independent.
///
/// The RA of the Ascendant seeds the equatorial divisions; each 30° arc is
/// then projected back to the ecliptic via the hour-circle intersection
/// formula `λ = atan2(sin α, cos α · cos ε)`.
#[must_use]
pub fn carter_rad(ac_rad: f64, obliquity_rad: f64) -> HouseCusps {
    let eps_cos = obliquity_rad.cos();
    // RA of the Ascendant: ecliptic → equatorial for a point on the ecliptic
    // (β = 0). α = atan2(sin λ · cos ε, cos λ).
    let ra_asc = f64::atan2(ac_rad.sin() * eps_cos, ac_rad.cos()).rem_euclid(TAU);
    let mut cusps = [0.0_f64; 12];
    for k in 0..12 {
        let ra = (ra_asc + k as f64 * FRAC_PI_6).rem_euclid(TAU);
        // Project equator point back to ecliptic via hour-circle intersection.
        cusps[k] = f64::atan2(ra.sin(), ra.cos() * eps_cos).rem_euclid(TAU);
    }
    HouseCusps(cusps)
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_abs_diff_eq;

    const J2000_EPS_RAD: f64 = 0.409_092_62;
    const EPS: f64 = 1e-9;

    #[test]
    fn h1_equals_asc() {
        // The round-trip RA(Asc) → projection must recover the Ascendant exactly.
        let ac = 45_f64.to_radians();
        let hc = carter_rad(ac, J2000_EPS_RAD);
        assert_abs_diff_eq!(hc.cusp(1), ac, epsilon = EPS);
    }

    #[test]
    fn h7_is_opposite_h1() {
        let ac = 200_f64.to_radians();
        let hc = carter_rad(ac, J2000_EPS_RAD);
        let diff = (hc.cusp(7) - hc.cusp(1)).rem_euclid(TAU).to_degrees();
        assert_abs_diff_eq!(diff, 180.0, epsilon = EPS);
    }

    #[test]
    fn opposite_cusps_are_180_apart() {
        let ac = 100_f64.to_radians();
        let hc = carter_rad(ac, J2000_EPS_RAD);
        for (a, b) in [(1_u8, 7_u8), (2, 8), (3, 9), (4, 10), (5, 11), (6, 12)] {
            let diff = (hc.cusp(b) - hc.cusp(a)).rem_euclid(TAU).to_degrees();
            assert_abs_diff_eq!(diff, 180.0, epsilon = EPS);
        }
    }

    #[test]
    fn cusps_advance_in_zodiacal_order() {
        let ac = 45_f64.to_radians();
        let hc = carter_rad(ac, J2000_EPS_RAD);
        // Consecutive cusps must advance eastward (allow for wrap at 360°).
        for k in 0..12_usize {
            let gap = (hc.0[(k + 1) % 12] - hc.0[k]).rem_euclid(TAU).to_degrees();
            assert!(gap > 0.0 && gap < 90.0, "gap {gap} out of range at k={k}");
        }
    }

    #[test]
    fn latitude_independent() {
        // Carter is purely equatorial — obliquity and Asc fully determine cusps.
        // Results must not differ across representative latitudes when AC is equal.
        let ac = 150_f64.to_radians();
        let hc1 = carter_rad(ac, J2000_EPS_RAD);
        let hc2 = carter_rad(ac, J2000_EPS_RAD);
        for k in 0..12 {
            assert_abs_diff_eq!(hc1.0[k], hc2.0[k], epsilon = EPS);
        }
    }
}
