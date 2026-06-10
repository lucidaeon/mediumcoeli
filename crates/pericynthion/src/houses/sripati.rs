#![cfg(feature = "noref-houses")]
#![allow(clippy::needless_range_loop)]

//! Sripati (Sri Pati Paddhati) house system — gated behind
//! `noref-houses` until a refchart oracle is captured. See
//! `docs/discovery/HOUSE_PROMOTION.md`.
//!
//! Porphyry variant: bhāva sandhi (house boundaries) sit at Porphyry's
//! cusps; bhāva madhya (house centers) sit at Porphyry's cusps. Encoded
//! here as a 12-cusp object whose entries are the midpoints between
//! consecutive Porphyry cusps. The ASC therefore lies at the center of
//! H1 rather than at H1's boundary.

use super::HouseCusps;
use super::porphyry::porphyry_rad;
use std::f64::consts::TAU;

/// Sripati house cusps from ASC and MC longitudes (radians).
///
/// Each cusp is the midpoint of the corresponding Porphyry boundary
/// arc, so the ASC sits at the *center* of H1 (not at H1's boundary).
#[must_use]
pub fn sripati_rad(ac_rad: f64, mc_rad: f64) -> HouseCusps {
    let porph = porphyry_rad(ac_rad, mc_rad);
    let mut cusps = [0.0_f64; 12];
    for i in 0..12_u8 {
        let curr = porph.0[i as usize];
        let next = porph.0[((i + 1) % 12) as usize];
        let span = (next - curr).rem_euclid(TAU);
        cusps[i as usize] = (curr + span / 2.0).rem_euclid(TAU);
    }
    HouseCusps(cusps)
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_abs_diff_eq;
    use std::f64::consts::PI;

    #[test]
    fn opposite_cusps_are_180_apart() {
        let ac = 100.0_f64.to_radians();
        let mc = 10.0_f64.to_radians();
        let hc = sripati_rad(ac, mc);
        for (a, b) in [(1_u8, 7_u8), (2, 8), (3, 9), (4, 10), (5, 11), (6, 12)] {
            let diff = (hc.cusp(b) - hc.cusp(a)).rem_euclid(TAU);
            assert_abs_diff_eq!(diff.to_degrees(), 180.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn asc_lies_inside_h1_not_at_boundary() {
        // With ASC=100°, MC=10°, Porphyry trisects: upper arc=90°,
        // Porphyry H11=40°, H12=70°. Sripati H12 should be midpoint(70, 100)=85°.
        // Sripati H1 = midpoint(100, midpoint of ASC-IC arc). Verify ASC=100
        // is between Sripati H12 and Sripati H1.
        let ac = 100.0_f64.to_radians();
        let mc = 10.0_f64.to_radians();
        let hc = sripati_rad(ac, mc);
        let h12 = hc.cusp(12);
        let h1 = hc.cusp(1);
        let span = (h1 - h12).rem_euclid(TAU);
        let asc_offset = (ac - h12).rem_euclid(TAU);
        assert!(
            asc_offset < span,
            "ASC at {:.3}° should sit between Sripati H12 {:.3}° and H1 {:.3}°",
            ac.to_degrees(),
            h12.to_degrees(),
            h1.to_degrees()
        );
    }

    #[test]
    fn cusps_partition_the_circle() {
        let ac = 130.0_f64.to_radians();
        let mc = 40.0_f64.to_radians();
        let hc = sripati_rad(ac, mc);
        let mut total = 0.0_f64;
        for i in 0..12 {
            let span = (hc.0[(i + 1) % 12] - hc.0[i]).rem_euclid(TAU).to_degrees();
            assert!(span > 0.0 && span < 180.0, "H{} span {:.3}°", i + 1, span);
            total += span;
        }
        assert_abs_diff_eq!(total, 360.0, epsilon = 1e-9);
    }

    #[test]
    fn no_lat_or_obliquity_dependence() {
        // Sripati derives from ASC + MC only, like its Porphyry parent.
        // Calling twice with the same (ASC, MC) pair must produce
        // identical cusps.
        let ac = 151.484_f64.to_radians();
        let mc = 57.636_f64.to_radians();
        let hc1 = sripati_rad(ac, mc);
        let hc2 = sripati_rad(ac, mc);
        for i in 0..12 {
            assert_abs_diff_eq!(hc1.0[i], hc2.0[i], epsilon = 1e-15);
        }
        let _ = PI;
    }
}
