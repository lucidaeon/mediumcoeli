//! Annual aberration — the apparent angular shift of a body caused by
//! Earth's orbital velocity relative to the speed of light.
//!
//! # The effect
//!
//! Light from a distant body takes time to reach Earth. During that
//! transit, Earth's orbital motion (~30 km/s) carries the observer
//! sideways relative to the inbound ray. The body appears displaced
//! toward the direction Earth is moving — up to ≈ 20.5″ (the
//! "constant of aberration") for bodies at the ecliptic poles, and
//! correspondingly less for bodies near Earth's velocity vector.
//!
//! This is **annual** aberration (from orbital motion), applied to
//! geocentric and topocentric positions alike. Diurnal aberration (the
//! additional ≈ 0.3″ shift from Earth's rotation observed at the
//! surface) is currently not added to topocentric output — its effect
//! is well below our 1″ planet-accuracy floor and below the Moon
//! residual on modern charts.
//!
//! # First-order approximation
//!
//! For our sub-arcsecond accuracy floor, first-order aberration is
//! sufficient.
//! Given the body's apparent direction unit vector `p̂` and Earth's
//! barycentric velocity `V` (km/s), the apparent direction is:
//!
//! ```text
//! p̂' ≈ p̂ + (V/c) − (p̂ · V/c) · p̂
//! ```
//!
//! followed by renormalization. The relativistic correction is O((V/c)²)
//! ≈ 10⁻⁸ — well below 1″.

use crate::coords::transform::Vector3;

/// Speed of light in km/s (CODATA 2018 value).
pub const SPEED_OF_LIGHT_KM_PER_S: f64 = 299_792.458;

/// Convert km/day → km/s (for using ephemeris velocities with the
/// km/s speed of light).
#[must_use]
pub fn km_per_day_to_km_per_s(v: &Vector3) -> Vector3 {
    let f = 1.0 / 86_400.0;
    [v[0] * f, v[1] * f, v[2] * f]
}

/// Apply first-order annual aberration to a geocentric position vector
/// `p_geo_km` (in km), given Earth's barycentric velocity in km/s.
///
/// Returns a new position vector with the aberration shift applied.
/// The magnitude of the vector is preserved to first order; the
/// direction shifts by ≈ |V|/c · sin(angle to V).
#[must_use]
pub fn apply_annual_aberration(p_geo_km: &Vector3, v_earth_km_per_s: &Vector3) -> Vector3 {
    let r = (p_geo_km[0].powi(2) + p_geo_km[1].powi(2) + p_geo_km[2].powi(2)).sqrt();
    if r == 0.0 {
        return *p_geo_km;
    }
    // Unit direction.
    let p_hat = [p_geo_km[0] / r, p_geo_km[1] / r, p_geo_km[2] / r];
    // V/c.
    let v_over_c = [
        v_earth_km_per_s[0] / SPEED_OF_LIGHT_KM_PER_S,
        v_earth_km_per_s[1] / SPEED_OF_LIGHT_KM_PER_S,
        v_earth_km_per_s[2] / SPEED_OF_LIGHT_KM_PER_S,
    ];
    // p̂ · (V/c).
    let p_dot_vc = p_hat[0] * v_over_c[0] + p_hat[1] * v_over_c[1] + p_hat[2] * v_over_c[2];
    // Shifted unit direction.
    let shifted = [
        p_hat[0] + v_over_c[0] - p_dot_vc * p_hat[0],
        p_hat[1] + v_over_c[1] - p_dot_vc * p_hat[1],
        p_hat[2] + v_over_c[2] - p_dot_vc * p_hat[2],
    ];
    // Renormalize to preserve magnitude.
    let new_mag = (shifted[0].powi(2) + shifted[1].powi(2) + shifted[2].powi(2)).sqrt();
    [
        shifted[0] / new_mag * r,
        shifted[1] / new_mag * r,
        shifted[2] / new_mag * r,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coords::transform::magnitude;
    use approx::assert_abs_diff_eq;

    #[test]
    fn aberration_with_zero_velocity_is_a_no_op() {
        let p = [149_597_870.7, 0.0, 0.0];
        let v = [0.0, 0.0, 0.0];
        let p_app = apply_annual_aberration(&p, &v);
        for i in 0..3 {
            assert_abs_diff_eq!(p_app[i], p[i], epsilon = 1e-9);
        }
    }

    #[test]
    fn aberration_preserves_magnitude() {
        let p = [149_597_870.7, 1e6, 1e5];
        let v = [29.78, 0.0, 0.0]; // Earth at perihelion-ish
        let p_app = apply_annual_aberration(&p, &v);
        assert_abs_diff_eq!(magnitude(&p_app), magnitude(&p), epsilon = 1e-3);
    }

    #[test]
    fn aberration_shifts_perpendicular_body_by_constant_of_aberration() {
        // A body at the ecliptic pole as seen by an observer moving
        // perpendicular to it (Earth at maximum tangential velocity)
        // shifts by the full aberration constant ~20.5″.
        // Set up: body at +Z, Earth moving in +X at 29.78 km/s.
        let body_distance = 1e9; // arbitrary
        let p = [0.0, 0.0, body_distance];
        let v = [29.78, 0.0, 0.0];
        let p_app = apply_annual_aberration(&p, &v);
        // The shift should be in the +X direction; magnitude ≈ R · (V/c).
        let shift_arcsec = (p_app[0] / body_distance).atan().to_degrees() * 3600.0;
        // V/c · 1 rad = 29.78/299792 = 9.93e-5 rad = 20.49″.
        assert_abs_diff_eq!(shift_arcsec, 20.49, epsilon = 0.1);
    }

    #[test]
    fn velocity_unit_conversion_round_trip() {
        let v_per_day = [2_592_000.0, 0.0, 0.0]; // 30 km/s × 86400 s/day
        let v_per_s = km_per_day_to_km_per_s(&v_per_day);
        assert_abs_diff_eq!(v_per_s[0], 30.0, epsilon = 1e-9);
    }
}
