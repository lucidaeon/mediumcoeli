//! Black Moon Lilith and Priapus: the Moon's orbital apsides axis.
//!
//! **Black Moon Lilith** is the ecliptic longitude of the Moon's apogee —
//! the far point of its elliptical orbit around Earth. **Priapus** is the
//! perigee, exactly opposite Lilith (Lilith + 180°).
//!
//! Two computation modes are shipped:
//!
//! 1. **Mean Lilith** ([`mean_lilith_rad`]) — closed-form polynomial.
//!    Smoothed mean longitude of the lunar apogee from Moon's mean
//!    elements (Meeus AA Ch. 47): Mean Apogee = (L − M′) + 180°, where
//!    L is the Moon's mean longitude and M′ its mean anomaly. Cheap,
//!    monotonically prograde, no ephemeris read.
//!
//! 2. **True / Osculating Lilith** ([`true_lilith_rad`]) — instantaneous
//!    direction of the apogee from Moon's state vector, via the
//!    eccentricity vector e = (v × h)/μ − r̂. Erratic / oscillates ~±15°
//!    around mean.
//!
//! Default mode in the CLI is **true** (matches Lilith's commonest
//! modern astrological reading).

use crate::body::Body;
use crate::coords::nutation::{nutate_mean_to_true, nutation};
use crate::coords::obliquity::mean_obliquity_rad;
use crate::coords::precession::precess_j2000_to_date;
use crate::coords::transform::equatorial_to_ecliptic;
use crate::ephemeris::Ephemeris;
use crate::error::PericynthionError;
use std::f64::consts::{PI, TAU};

/// Gravitational parameter μ for the Earth–Moon two-body problem,
/// `G·(M_Earth + M_Moon)` in km³ / day².
///
/// Derived from DE441-consistent values (`GM_Earth` = 398 600.4418 km³/s²,
/// `GM_Moon` = 4 902.800 066 km³/s²) → sum 403 503.241 87 km³/s², scaled
/// to per-day by (86 400 s/day)² = 7.464 96e9.
const MU_EM_KM3_PER_DAY2: f64 = 403_503.241_87 * 86_400.0 * 86_400.0;

/// Mean ecliptic longitude of Black Moon Lilith (mean lunar apogee),
/// radians \[0, TAU).
///
/// Meeus AA 2nd ed. Ch. 47 mean elements:
/// - `L  = 218.3164477 + 481267.88123421·T − 0.0015786·T² + T³/538841 − T⁴/65194000`
/// - `M' = 134.9633964 + 477198.8675055·T + 0.0087414·T² + T³/69699   − T⁴/14712000`
///
/// Mean apogee = (L − M') + 180°, where `T = (jd_tt − 2451545.0) / 36525`.
#[must_use]
pub fn mean_lilith_rad(jd_tt: f64) -> f64 {
    let t = (jd_tt - 2_451_545.0) / 36_525.0;
    let t2 = t * t;
    let t3 = t2 * t;
    let t4 = t3 * t;

    let l = 218.316_447_7 + 481_267.881_234_21 * t - 0.001_578_6 * t2 + t3 / 538_841.0
        - t4 / 65_194_000.0;
    let m_prime = 134.963_396_4 + 477_198.867_505_5 * t + 0.008_741_4 * t2 + t3 / 69_699.0
        - t4 / 14_712_000.0;

    let perigee = l - m_prime;
    let apogee = perigee + 180.0;
    apogee.to_radians().rem_euclid(TAU)
}

/// True (osculating) ecliptic longitude of Black Moon Lilith, radians
/// \[0, TAU).
///
/// Built from the Laplace-Runge-Lenz eccentricity vector
/// `e = (v × h)/μ − r̂` in the ecliptic-of-date frame, where
/// `h = r × v` is the Moon's specific angular momentum and `μ` is the
/// Earth–Moon system gravitational parameter. The perigee direction is
/// `+e`; Lilith (apogee) is the diametrically opposite direction,
/// `atan2(−e_y, −e_x)`.
///
/// # Errors
///
/// Propagates I/O / out-of-range errors from the underlying
/// [`Ephemeris::state`] call.
pub fn true_lilith_rad(ephem: &Ephemeris, jd_tt: f64) -> Result<f64, PericynthionError> {
    let moon = ephem.state(Body::Moon, jd_tt)?;
    let r0 = moon.position_km;
    let v0 = moon.velocity_km_per_day;

    let eps_mean = mean_obliquity_rad(jd_tt);
    let nut = nutation(jd_tt);
    let eps_true = eps_mean + nut.delta_epsilon;

    let r_mean = precess_j2000_to_date(&r0, jd_tt);
    let v_mean = precess_j2000_to_date(&v0, jd_tt);
    let r_true = nutate_mean_to_true(&r_mean, jd_tt, eps_mean);
    let v_true = nutate_mean_to_true(&v_mean, jd_tt, eps_mean);
    let r = equatorial_to_ecliptic(&r_true, eps_true);
    let v = equatorial_to_ecliptic(&v_true, eps_true);

    let h = cross(&r, &v);
    let v_cross_h = cross(&v, &h);
    let r_norm = (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt();

    // Eccentricity vector points from focus (Earth) toward perigee.
    let mu = MU_EM_KM3_PER_DAY2;
    let e_x = v_cross_h[0] / mu - r[0] / r_norm;
    let e_y = v_cross_h[1] / mu - r[1] / r_norm;

    // Apogee = perigee + 180° → direction is −e in the ecliptic plane.
    Ok((-e_y).atan2(-e_x).rem_euclid(TAU))
}

/// Priapus (perigee) longitude: exactly opposite Lilith (apogee).
#[must_use]
pub fn priapus_rad(lilith_rad: f64) -> f64 {
    (lilith_rad + PI).rem_euclid(TAU)
}

/// Returns `true` when the true (osculating) Black Moon Lilith is retrograde
/// at `jd_tt`.
///
/// Uses the same ±0.5-day finite-difference as
/// [`crate::coords::body_is_retrograde`].  True Lilith oscillates erratically
/// around the mean apogee and can station/retrograde briefly; mean Lilith
/// always progrades and is never retrograde.
///
/// # Errors
///
/// Propagates I/O / out-of-range errors from the underlying
/// [`true_lilith_rad`] calls.
pub fn true_lilith_is_retrograde(ephem: &Ephemeris, jd_tt: f64) -> Result<bool, PericynthionError> {
    let before = true_lilith_rad(ephem, jd_tt - 0.5)?;
    let after = true_lilith_rad(ephem, jd_tt + 0.5)?;
    Ok(crate::coords::signed_daily_motion(before.to_degrees(), after.to_degrees()) < 0.0)
}

fn cross(a: &[f64; 3], b: &[f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_abs_diff_eq;

    #[test]
    fn priapus_is_lilith_plus_180() {
        let lilith = 100.0_f64.to_radians();
        let priapus = priapus_rad(lilith);
        let diff = (priapus - lilith).rem_euclid(TAU);
        assert_abs_diff_eq!(diff.to_degrees(), 180.0, epsilon = 1e-12);
    }

    #[test]
    fn mean_lilith_at_j2000_is_about_263() {
        // L − M' at T=0 → (218.316 − 134.963) + 180 = 263.353°.
        let lilith = mean_lilith_rad(2_451_545.0).to_degrees();
        assert_abs_diff_eq!(lilith, 263.353, epsilon = 0.01);
    }

    #[test]
    fn mean_lilith_progrades_over_one_year() {
        // Mean apogee progresses ~40.69°/year (one cycle every ~8.85 years).
        let l0 = mean_lilith_rad(2_451_545.0).to_degrees();
        let l1 = mean_lilith_rad(2_451_545.0 + 365.25).to_degrees();
        let delta = (l1 - l0).rem_euclid(360.0);
        assert!(
            (40.0..=42.0).contains(&delta),
            "expected ~40.69°/yr progression, got {delta:.3}°"
        );
    }
}
