//! Vertex (Vx) and Anti-Vertex (Ax): the ecliptic intersection of the
//! *prime vertical* (the great circle through the east point, zenith,
//! west point, and nadir).
//!
//! The Vertex is the **western** intersection of the prime vertical with
//! the ecliptic; the Anti-Vertex is its exact opposite (Vx + 180°). At
//! the equator the prime vertical coincides with the horizon, so Vx
//! degenerates with the Descendant; at the geographic poles the formula
//! diverges (`tan(lat)` → ∞), and we return `None`.
//!
//! Formula (Meeus AA Ch. 14, adapted): with `H = RAMC`,
//! ```text
//!   Vx = atan2( −cos(H),  sin(H)·cos(ε) − cot(φ)·sin(ε) )
//! ```
//! Equivalent to substituting co-latitude (90° − φ) for latitude in the
//! Ascendant formula and rotating by 180°.

use std::f64::consts::{FRAC_PI_2, PI, TAU};

/// Ecliptic longitude of the Vertex, radians in \[0, TAU).
///
/// Inputs:
/// - `ramc_rad` — Right Ascension of the Midheaven (= Local Apparent
///   Sidereal Time) in radians.
/// - `obliquity_rad` — true obliquity of the ecliptic in radians.
/// - `lat_rad` — geographic latitude in radians (north positive).
///
/// Returns `None` at the geographic poles or at the equator
/// (`lat_rad == 0`), where the prime vertical degenerates.
#[must_use]
pub fn vx_rad(ramc_rad: f64, obliquity_rad: f64, lat_rad: f64) -> Option<f64> {
    if lat_rad.abs() >= FRAC_PI_2 {
        return None;
    }
    if lat_rad == 0.0 {
        // At the equator the prime vertical coincides with the horizon;
        // cot(φ) → ∞ and the Vertex is undefined.
        return None;
    }
    let cot_lat = 1.0 / lat_rad.tan();
    let y = -ramc_rad.cos();
    let x = ramc_rad.sin() * obliquity_rad.cos() - cot_lat * obliquity_rad.sin();
    Some(y.atan2(x).rem_euclid(TAU))
}

/// Ecliptic longitude of the Anti-Vertex, radians in \[0, TAU).
///
/// Always exactly opposite the Vertex: `(vx + π) mod 2π`.
#[must_use]
pub fn ax_rad(vx_rad: f64) -> f64 {
    (vx_rad + PI).rem_euclid(TAU)
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_abs_diff_eq;

    const J2000_EPS_RAD: f64 = 0.409_092_62;

    #[test]
    fn equator_returns_none() {
        assert!(vx_rad(1.0, J2000_EPS_RAD, 0.0).is_none());
    }

    #[test]
    fn poles_return_none() {
        assert!(vx_rad(0.0, J2000_EPS_RAD, FRAC_PI_2).is_none());
        assert!(vx_rad(0.0, J2000_EPS_RAD, -FRAC_PI_2).is_none());
    }

    #[test]
    fn antivertex_is_vertex_plus_180() {
        let lat = 40.0_f64.to_radians();
        let ramc = 100.0_f64.to_radians();
        let vx = vx_rad(ramc, J2000_EPS_RAD, lat).unwrap();
        let ax = ax_rad(vx);
        let diff = (ax - vx).rem_euclid(TAU);
        assert_abs_diff_eq!(diff, PI, epsilon = 1e-12);
    }
}
