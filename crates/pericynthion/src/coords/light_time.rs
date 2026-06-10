//! Light-time correction: iterate to find the past instant from which
//! light arriving at Earth at JD-TT actually departed the body.
//!
//! # Why it matters
//!
//! When we observe a planet, we don't see where it is *now* — we see
//! where it was when the light we're observing first left. For Mars
//! at opposition, that's ~4 minutes of motion; for Jupiter, ~30
//! minutes; for Pluto, ~5 hours. Without correction, our geocentric
//! direction vector points at where the planet has since moved to,
//! introducing arc-minute-scale errors on the outer planets.
//!
//! # Algorithm
//!
//! Given:
//! - `JD_TT` — the observation epoch.
//! - `body_position(jd)` — function returning the body's barycentric
//!   position at any JD.
//! - `earth_position_at_obs` — Earth's barycentric position at `JD_TT`
//!   (the observer's position is fixed; only the source moves).
//!
//! Iterate:
//!
//! ```text
//! t = JD_TT
//! for _ in 0..3 {
//!     pos = body_position(t) − earth_position_at_obs
//!     τ_days = |pos| / c   // light-time in days
//!     t = JD_TT − τ_days
//! }
//! ```
//!
//! Convergence: 3 iterations are sufficient for inner planets and
//! reduce residual to << 0.1″ for Pluto.

use crate::coords::aberration::SPEED_OF_LIGHT_KM_PER_S;
use crate::coords::transform::Vector3;

/// Light-time in days for a body at distance `r_km` from the observer.
#[must_use]
pub fn light_time_days(r_km: f64) -> f64 {
    r_km / SPEED_OF_LIGHT_KM_PER_S / 86_400.0
}

/// Iterate to find the body's geocentric position at the corrected
/// past instant `t_emission = JD_TT − τ`.
///
/// `body_position_at_tt` is a callback that returns the body's
/// barycentric km-position at any TT JD. `earth_position_at_obs` is
/// fixed (the observer is at Earth's position at `JD_TT`, not at the
/// emission time — only the source moves during light-time).
///
/// Returns the geocentric position vector (km) at the corrected
/// instant, and the converged light-time in days.
#[must_use]
pub fn iterate_light_time<F>(
    jd_tt: f64,
    earth_position_at_obs: &Vector3,
    body_position_at_tt: F,
) -> (Vector3, f64)
where
    F: Fn(f64) -> Vector3,
{
    let mut t = jd_tt;
    let mut tau = 0.0_f64;
    let mut geo = [0.0_f64; 3];
    for _ in 0..3 {
        let body_pos = body_position_at_tt(t);
        for axis in 0..3 {
            geo[axis] = body_pos[axis] - earth_position_at_obs[axis];
        }
        let r = (geo[0].powi(2) + geo[1].powi(2) + geo[2].powi(2)).sqrt();
        tau = light_time_days(r);
        t = jd_tt - tau;
    }
    (geo, tau)
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_abs_diff_eq;

    #[test]
    fn light_time_for_one_au_is_about_8_minutes() {
        let r_au_km = 149_597_870.7;
        let tau_days = light_time_days(r_au_km);
        let tau_seconds = tau_days * 86_400.0;
        assert_abs_diff_eq!(tau_seconds, 499.0, epsilon = 1.0);
    }

    #[test]
    fn iteration_converges_for_stationary_body() {
        // If the body never moves, light-time iteration should still
        // give the correct distance and a valid τ.
        let earth_pos = [0.0, 0.0, 0.0];
        let body_pos = [149_597_870.7, 0.0, 0.0];
        let (geo, tau) = iterate_light_time(2_451_545.0, &earth_pos, |_jd| body_pos);
        for i in 0..3 {
            assert_abs_diff_eq!(geo[i], body_pos[i], epsilon = 1e-9);
        }
        assert_abs_diff_eq!(tau, 499.0 / 86_400.0, epsilon = 1e-6);
    }

    #[test]
    fn iteration_steps_back_for_moving_body() {
        // Synthetic body moving in +X at 1000 km/day (toy value).
        // At t = JD_TT it's at 149,597,870.7 km. After τ light-time
        // correction, we should be looking at where it WAS, ~τ * 1000
        // km earlier.
        let earth_pos = [0.0, 0.0, 0.0];
        let velocity_km_per_day = 1000.0;
        let body_at_t = |jd: f64| {
            let dt = jd - 2_451_545.0;
            [149_597_870.7 + velocity_km_per_day * dt, 0.0, 0.0]
        };
        let (geo, tau) = iterate_light_time(2_451_545.0, &earth_pos, body_at_t);
        // The geocentric position we report should be at t = JD_TT − τ:
        // body was at 149_597_870.7 − τ * 1000 then.
        let expected_x = 149_597_870.7 - tau * velocity_km_per_day;
        assert_abs_diff_eq!(geo[0], expected_x, epsilon = 1e-3);
    }
}
