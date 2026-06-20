//! Lunar nodes: Moon's orbital plane / ecliptic intersection.
//!
//! The **North Node (Nn)** is the ecliptic longitude where the Moon's
//! orbit crosses the ecliptic going south-to-north. The **South Node
//! (Sn)** is its exact opposite (Nn + 180°). The Moon's orbital plane is
//! inclined ~5.14° to the ecliptic and the node line regresses with an
//! ~18.6-year period (mean rate ≈ −19.34°/year).
//!
//! Two computation modes ship:
//!
//! 1. [`mean_nn_rad`] — closed-form polynomial (Meeus AA Ch. 47.4) smoothed
//!    over short-period perturbations. Cheap, monotonically retrograde,
//!    no ephemeris read needed.
//! 2. [`true_nn_rad`] — instantaneous (osculating) node from the Moon's
//!    state vector via angular momentum `h = r × v` in the ecliptic-of-
//!    date frame. Matches refchart's "Nod" entries (which show "SR"
//!    stationary-retrograde notation in some charts, confirming osculating
//!    mode).

use crate::body::Body;
use crate::coords::nutation::{nutate_mean_to_true, nutation};
use crate::coords::obliquity::mean_obliquity_rad;
use crate::coords::precession::precess_j2000_to_date;
use crate::coords::transform::equatorial_to_ecliptic;
use crate::ephemeris::Ephemeris;
use crate::error::PericynthionError;
use std::f64::consts::{PI, TAU};

/// Mean ecliptic longitude of the Moon's ascending node, radians \[0, TAU).
///
/// Meeus AA 2nd ed. Ch. 47.4 polynomial:
/// ```text
///   Ω = 125.04452 − 1934.136261·T + 0.0020708·T² + T³/450000   (degrees)
/// ```
/// with `T = (jd_tt − 2451545.0) / 36525` (Julian centuries from J2000.0).
#[must_use]
pub fn mean_nn_rad(jd_tt: f64) -> f64 {
    let t = (jd_tt - 2_451_545.0) / 36_525.0;
    let omega_deg = 125.044_52 - 1_934.136_261 * t + 0.002_070_8 * t * t + (t * t * t) / 450_000.0;
    omega_deg.to_radians().rem_euclid(TAU)
}

/// True (osculating) ecliptic longitude of the Moon's ascending node,
/// radians \[0, TAU).
///
/// Computed from the Moon's instantaneous orbital plane: `h = r × v`,
/// then `Ω = atan2(h_x, −h_y)` in the ecliptic-of-date frame. The
/// rotation chain J2000 equatorial → mean of date → true of date →
/// ecliptic of date is applied to both `r` and `v` independently
/// (rotations are linear and preserve the cross product).
///
/// # Errors
///
/// Propagates I/O / out-of-range errors from the underlying
/// [`Ephemeris::state`] call.
pub fn true_nn_rad(ephem: &Ephemeris, jd_tt: f64) -> Result<f64, PericynthionError> {
    let moon = ephem.state(Body::Moon, jd_tt)?;
    let r0 = moon.position_km;
    let v0 = moon.velocity_km_per_day;

    let eps_mean = mean_obliquity_rad(jd_tt);
    let nut = nutation(jd_tt);
    let eps_true = eps_mean + nut.delta_epsilon;

    // Rotate r and v J2000 equatorial → ecliptic of date.
    let r_mean = precess_j2000_to_date(&r0, jd_tt);
    let v_mean = precess_j2000_to_date(&v0, jd_tt);
    let r_true = nutate_mean_to_true(&r_mean, jd_tt, eps_mean);
    let v_true = nutate_mean_to_true(&v_mean, jd_tt, eps_mean);
    let r_ec = equatorial_to_ecliptic(&r_true, eps_true);
    let v_ec = equatorial_to_ecliptic(&v_true, eps_true);

    // h = r × v (orbital plane normal). The ascending-node direction
    // N̂ = ẑ × ĥ (with ẑ = ecliptic pole) has components (−h_y, h_x, 0).
    let hx = r_ec[1] * v_ec[2] - r_ec[2] * v_ec[1];
    let hy = r_ec[2] * v_ec[0] - r_ec[0] * v_ec[2];

    Ok(hx.atan2(-hy).rem_euclid(TAU))
}

/// South-node longitude: exactly opposite the north node.
#[must_use]
pub fn sn_rad(nn_rad: f64) -> f64 {
    (nn_rad + PI).rem_euclid(TAU)
}

/// Returns `true` when the true (osculating) North Node is retrograde at
/// `jd_tt`.
///
/// Uses the same ±0.5-day finite-difference as [`crate::coords::body_is_retrograde`].
/// The mean node is always retrograde by construction; this function
/// is only meaningful for the osculating mode.
///
/// # Errors
///
/// Propagates I/O / out-of-range errors from the underlying
/// [`true_nn_rad`] calls.
pub fn true_nn_is_retrograde(
    ephem: &Ephemeris,
    jd_tt: f64,
) -> Result<bool, crate::error::PericynthionError> {
    let before = true_nn_rad(ephem, jd_tt - 0.5)?;
    let after = true_nn_rad(ephem, jd_tt + 0.5)?;
    Ok(crate::coords::signed_daily_motion(before.to_degrees(), after.to_degrees()) < 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_abs_diff_eq;

    #[test]
    fn sn_is_nn_plus_180() {
        let nn = 257.481_f64.to_radians();
        let sn = sn_rad(nn);
        let diff = (sn - nn).rem_euclid(TAU);
        assert_abs_diff_eq!(diff.to_degrees(), 180.0, epsilon = 1e-12);
    }

    #[test]
    fn mean_nn_at_j2000_is_about_125() {
        // From the Meeus polynomial constant term.
        let nn = mean_nn_rad(2_451_545.0).to_degrees();
        assert_abs_diff_eq!(nn, 125.044_52, epsilon = 1e-4);
    }

    #[test]
    fn mean_nn_regresses_over_one_year() {
        // Mean node retrogrades ~19.34°/year. Over one Julian year (365.25
        // days) the longitude should decrease by ~19.34° (mod 360°).
        let nn_2000 = mean_nn_rad(2_451_545.0).to_degrees();
        let nn_2001 = mean_nn_rad(2_451_545.0 + 365.25).to_degrees();
        let delta = (nn_2000 - nn_2001).rem_euclid(360.0);
        assert!(
            (19.0..=20.0).contains(&delta),
            "expected ~19.34°/yr regression, got {delta:.3}°"
        );
    }
}
