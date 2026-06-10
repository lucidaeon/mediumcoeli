//! Topocentric parallax correction.
//!
//! Converts a geocentric body position in the true equatorial frame of
//! date to a topocentric position by subtracting the observer's geocentric
//! position vector (derived from geodetic latitude, longitude, and GAST).
//!
//! The effect is largest for the Moon (up to ~57' shift), measurable for
//! the Sun (~8.79" at the horizon), and negligible for all other bodies.

use crate::coords::transform::Vector3;

// WGS84 reference ellipsoid constants.
const A_KM: f64 = 6_378.137; // equatorial radius, km
const F: f64 = 1.0 / 298.257_223_563; // flattening

/// Observer's geodetic location on Earth's surface.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ObserverLocation {
    /// Geodetic latitude, degrees. Positive = north, negative = south.
    pub lat_deg: f64,
    /// Geographic longitude, degrees. Positive = east, negative = west.
    pub lon_deg: f64,
    /// Elevation above the WGS84 ellipsoid, metres. Sea level = 0.
    pub elev_m: f64,
}

/// Observer's position in the true equatorial frame of date (km).
///
/// `gast_rad`: Greenwich Apparent Sidereal Time in radians, used to
/// orient the observer's Earth-fixed position into the inertial frame.
#[must_use]
pub fn observer_equatorial_km(obs: &ObserverLocation, gast_rad: f64) -> Vector3 {
    let phi = obs.lat_deg.to_radians();
    let h_km = obs.elev_m / 1_000.0;

    // Auxiliary geocentric latitude u (Meeus, Ch. 11, eq. 11.1):
    // tan(u) = (1 − f) · tan(φ)
    let u = ((1.0 - F) * phi.tan()).atan();

    // ρ · sin φ' and ρ · cos φ', dimensionless (Earth equatorial radii).
    let rho_sin = (1.0 - F) * u.sin() + h_km / A_KM * phi.sin();
    let rho_cos = u.cos() + h_km / A_KM * phi.cos();

    // Local Apparent Sidereal Time = GAST + observer longitude (east positive).
    let last = gast_rad + obs.lon_deg.to_radians();

    [
        A_KM * rho_cos * last.cos(),
        A_KM * rho_cos * last.sin(),
        A_KM * rho_sin,
    ]
}

/// Apply topocentric parallax to a geocentric body vector.
///
/// Both vectors must be in the **true equatorial frame of date**, in km.
/// Returns the topocentric position vector (body as seen from the observer,
/// not from Earth's centre).
#[must_use]
pub fn apply_topocentric(geo_km: &Vector3, obs_km: &Vector3) -> Vector3 {
    [
        geo_km[0] - obs_km[0],
        geo_km[1] - obs_km[1],
        geo_km[2] - obs_km[2],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_abs_diff_eq;

    // At lat=0, lon=0, elev=0, GAST=0: observer sits at the equatorial
    // intersection of the Greenwich meridian. Position = [A_KM, 0, 0].
    #[test]
    fn equator_at_greenwich_gast_zero() {
        let obs = ObserverLocation {
            lat_deg: 0.0,
            lon_deg: 0.0,
            elev_m: 0.0,
        };
        let p = observer_equatorial_km(&obs, 0.0);
        assert_abs_diff_eq!(p[0], A_KM, epsilon = 0.001);
        assert_abs_diff_eq!(p[1], 0.0, epsilon = 0.001);
        assert_abs_diff_eq!(p[2], 0.0, epsilon = 0.001);
    }

    // At the north pole, the observer is on Earth's rotation axis regardless
    // of longitude or GAST. X and Y ≈ 0; Z ≈ A_KM · (1 − F) (polar radius).
    #[test]
    fn north_pole() {
        let obs = ObserverLocation {
            lat_deg: 90.0,
            lon_deg: 0.0,
            elev_m: 0.0,
        };
        let p = observer_equatorial_km(&obs, 0.0);
        assert_abs_diff_eq!(p[0], 0.0, epsilon = 0.01);
        assert_abs_diff_eq!(p[1], 0.0, epsilon = 0.01);
        let expected_z = (1.0 - F) * A_KM; // ≈ 6356.752 km
        assert_abs_diff_eq!(p[2], expected_z, epsilon = 0.1);
    }

    // Elevation adds directly to the radial distance from Earth's centre.
    // At the equator, GAST=0, 1000 m elevation adds ~1 km to x.
    #[test]
    fn elevation_adds_to_radius() {
        let sea_level = ObserverLocation {
            lat_deg: 0.0,
            lon_deg: 0.0,
            elev_m: 0.0,
        };
        let elevated = ObserverLocation {
            lat_deg: 0.0,
            lon_deg: 0.0,
            elev_m: 1_000.0,
        };
        let p0 = observer_equatorial_km(&sea_level, 0.0);
        let p1 = observer_equatorial_km(&elevated, 0.0);
        // 1000 m = 1 km added to the equatorial x component.
        assert_abs_diff_eq!(p1[0] - p0[0], 1.0, epsilon = 0.001);
        assert_abs_diff_eq!(p1[1], p0[1], epsilon = 1e-9);
        assert_abs_diff_eq!(p1[2], p0[2], epsilon = 0.001);
    }

    // Subtracting a zero observer vector leaves the body vector unchanged.
    #[test]
    fn topocentric_with_zero_observer_is_geocentric() {
        let geo: Vector3 = [384_400.0, 50_000.0, -10_000.0];
        let topo = apply_topocentric(&geo, &[0.0, 0.0, 0.0]);
        for i in 0..3 {
            assert_abs_diff_eq!(topo[i], geo[i], epsilon = 1e-10);
        }
    }
}
