//! High-level facade: the full coordinate-transformation pipeline as
//! a single call. Three shipped variants:
//!
//! - [`apparent_ecliptic_position`] — geocentric.
//! - [`apparent_ecliptic_position_topocentric`] — observer parallax.
//! - [`heliocentric_ecliptic_position`] — Sun-centred (no aberration).
//!
//! Each wires together the lower-level pieces — light-time, aberration
//! (geo/topo only), precession, nutation, obliquity, equatorial→
//! ecliptic — in the canonical order documented in [`crate::coords`]
//! and returns an [`EclipticPosition`] (lon, lat, distance). Callers
//! who want intermediate vectors reach into the lower modules directly.

use crate::body::Body;
use crate::coords::aberration::{apply_annual_aberration, km_per_day_to_km_per_s};
use crate::coords::light_time::iterate_light_time;
use crate::coords::nutation::nutate_mean_to_true;
use crate::coords::obliquity::mean_obliquity_rad;
use crate::coords::precession::precess_j2000_to_date;
use crate::coords::sidereal_time::gast_rad;
use crate::coords::topocentric::{ObserverLocation, apply_topocentric, observer_equatorial_km};
use crate::coords::transform::{equatorial_to_ecliptic, latitude_rad, longitude_rad, magnitude};
use crate::ephemeris::{Ephemeris, StateVector};
use crate::error::PericynthionError;
use crate::spk::SpkEphemeris;

/// Convert a TT Julian Date to ET seconds past J2000 (`et_sec`), the
/// time argument [`SpkEphemeris::state`] expects.
///
/// TT and TDB differ by at most ±1.7 ms (periodic), producing sub-meter
/// position errors for main-belt asteroids — negligible for astrology.
fn et_of(jd_tt: f64) -> f64 {
    (jd_tt - 2_451_545.0) * 86_400.0
}

/// Final astrologer-facing output: a body's apparent position in
/// tropical ecliptic-of-date coordinates.
///
/// Angles are in **degrees**; distance is in **astronomical units**.
/// Longitude is in `[0, 360)`, latitude in `[−90, +90]`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EclipticPosition {
    /// Tropical ecliptic longitude of date, degrees, `[0, 360)`.
    pub longitude_deg: f64,
    /// Ecliptic latitude of date, degrees, `[−90, +90]`.
    pub latitude_deg: f64,
    /// Geocentric distance, astronomical units.
    pub distance_au: f64,
}

const AU_KM: f64 = 149_597_870.7;

/// Body-agnostic geocentric apparent-place core.
///
/// Given Earth's [`StateVector`] at the observation epoch and a closure
/// returning the body's **barycentric** km-position at any (retarded) TT
/// Julian Date, runs the canonical pipeline and returns the apparent
/// ecliptic-of-date position. Every step after the body closure is
/// identical for planets, the Moon, and SPK asteroids.
///
/// Pipeline:
///
/// 1. Light-time iteration to find emission epoch.
/// 2. Annual aberration (Earth's velocity / c shift).
/// 3. Precession (J2000 → mean-of-date).
/// 4. Nutation (mean-of-date → true-of-date).
/// 5. Rotation to ecliptic of date using true obliquity (ε + Δε).
/// 6. Convert to longitude/latitude/distance.
fn apparent_from_earth_and_body(
    earth: &StateVector,
    jd_tt: f64,
    body_position_at: impl Fn(f64) -> [f64; 3],
) -> EclipticPosition {
    // === 1. Light-time iteration ===
    let (geo_astrometric_km, _tau) =
        iterate_light_time(jd_tt, &earth.position_km, body_position_at);

    // === 2. Annual aberration ===
    let v_earth_km_per_s = km_per_day_to_km_per_s(&earth.velocity_km_per_day);
    let geo_apparent_km = apply_annual_aberration(&geo_astrometric_km, &v_earth_km_per_s);

    // === 3. Precession (J2000 → mean-of-date) ===
    let geo_mean_of_date_km = precess_j2000_to_date(&geo_apparent_km, jd_tt);

    // === 4. Nutation (mean → true of date) ===
    let eps_mean = mean_obliquity_rad(jd_tt);
    let geo_true_of_date_km = nutate_mean_to_true(&geo_mean_of_date_km, jd_tt, eps_mean);

    // === 5. Rotation to ecliptic of date using TRUE obliquity ===
    let nut = crate::coords::nutation::nutation(jd_tt);
    let eps_true = eps_mean + nut.delta_epsilon;
    let geo_ecliptic_km = equatorial_to_ecliptic(&geo_true_of_date_km, eps_true);

    // === 6. Spherical conversion ===
    let lon_rad = longitude_rad(&geo_ecliptic_km);
    let lat_rad = latitude_rad(&geo_ecliptic_km);
    let r_km = magnitude(&geo_ecliptic_km);

    EclipticPosition {
        longitude_deg: lon_rad.to_degrees(),
        latitude_deg: lat_rad.to_degrees(),
        distance_au: r_km / AU_KM,
    }
}

/// Body-agnostic topocentric apparent-place core.
///
/// Identical to `apparent_from_earth_and_body` through the nutation
/// step, then the observer's geocentric position vector is subtracted in
/// the true equatorial frame of date before rotating to ecliptic
/// coordinates.
fn apparent_from_earth_and_body_topocentric(
    earth: &StateVector,
    jd_tt: f64,
    observer: &ObserverLocation,
    body_position_at: impl Fn(f64) -> [f64; 3],
) -> EclipticPosition {
    let (geo_astrometric_km, _tau) =
        iterate_light_time(jd_tt, &earth.position_km, body_position_at);

    let v_earth_km_per_s = km_per_day_to_km_per_s(&earth.velocity_km_per_day);
    let geo_apparent_km = apply_annual_aberration(&geo_astrometric_km, &v_earth_km_per_s);
    let geo_mean_of_date_km = precess_j2000_to_date(&geo_apparent_km, jd_tt);
    let eps_mean = mean_obliquity_rad(jd_tt);
    let geo_true_of_date_km = nutate_mean_to_true(&geo_mean_of_date_km, jd_tt, eps_mean);

    // === Topocentric correction in true equatorial of date ===
    let gast = gast_rad(jd_tt);
    let obs_km = observer_equatorial_km(observer, gast);
    let topo_true_km = apply_topocentric(&geo_true_of_date_km, &obs_km);

    let nut = crate::coords::nutation::nutation(jd_tt);
    let eps_true = eps_mean + nut.delta_epsilon;
    let topo_ecliptic_km = equatorial_to_ecliptic(&topo_true_km, eps_true);

    let lon_rad = longitude_rad(&topo_ecliptic_km);
    let lat_rad = latitude_rad(&topo_ecliptic_km);
    let r_km = magnitude(&topo_ecliptic_km);

    EclipticPosition {
        longitude_deg: lon_rad.to_degrees(),
        latitude_deg: lat_rad.to_degrees(),
        distance_au: r_km / AU_KM,
    }
}

/// Build the barycentric-position closure for a DE441 planet or the Moon.
///
/// Returns the body's *barycentric* km-position at `jd` so that
/// `iterate_light_time` can subtract `earth_bary(jd_tt)` and yield the
/// correct astrometric direction:
///   `geo_astro = body_bary(emission) − earth_bary(observation)`
///
/// For planets this is just `body_bary(jd)` directly.
///
/// For the Moon, DE441 stores `moon_geo(jd) = moon_bary(jd) − earth_bary(jd)`,
/// so we must add `earth_bary(jd)` — Earth at the *emission* epoch, not at
/// the observation epoch. Using `earth_bary(jd_tt)` here would cancel in the
/// loop and leave `moon_geo(emission)`: geo anchored to Earth at emission
/// rather than Earth at observation, causing annual aberration to be applied
/// twice (~20" error for the Moon).
fn planet_body_position_at<'a>(
    ephem: &'a Ephemeris<'a>,
    body: Body,
) -> impl Fn(f64) -> [f64; 3] + 'a {
    move |jd: f64| {
        if body == Body::Moon {
            let earth_at_jd = ephem
                .state(Body::Earth, jd)
                .map_or([0.0; 3], |s| s.position_km);
            let moon_geo = ephem
                .state(Body::Moon, jd)
                .map_or([0.0; 3], |s| s.position_km);
            [
                earth_at_jd[0] + moon_geo[0],
                earth_at_jd[1] + moon_geo[1],
                earth_at_jd[2] + moon_geo[2],
            ]
        } else {
            ephem.state(body, jd).map_or([0.0; 3], |s| s.position_km)
        }
    }
}

/// Compute the apparent ecliptic-of-date position of a body, observed
/// from Earth's barycentric position at the given TT Julian Date.
///
/// Thin wrapper over `apparent_from_earth_and_body` supplying the
/// DE441 planet/Moon body-position closure.
///
/// # Errors
///
/// Propagates I/O errors from the ephemeris if `JD_TT` is out of range.
pub fn apparent_ecliptic_position(
    ephem: &Ephemeris,
    body: Body,
    jd_tt: f64,
) -> Result<EclipticPosition, PericynthionError> {
    // Earth's state at the observer's epoch.
    let earth = ephem.state(Body::Earth, jd_tt)?;
    let body_position_at = planet_body_position_at(ephem, body);
    Ok(apparent_from_earth_and_body(
        &earth,
        jd_tt,
        body_position_at,
    ))
}

/// Compute the apparent ecliptic-of-date position of a body with topocentric
/// parallax applied, as seen by an observer at a specific location on Earth.
///
/// Identical to [`apparent_ecliptic_position`] through the nutation step,
/// then the observer's geocentric position vector is subtracted in the true
/// equatorial frame of date before rotating to ecliptic coordinates.
///
/// # Errors
///
/// Propagates I/O errors from the ephemeris if `JD_TT` is out of range.
pub fn apparent_ecliptic_position_topocentric(
    ephem: &Ephemeris,
    body: Body,
    jd_tt: f64,
    observer: &ObserverLocation,
) -> Result<EclipticPosition, PericynthionError> {
    let earth = ephem.state(Body::Earth, jd_tt)?;
    let body_position_at = planet_body_position_at(ephem, body);
    Ok(apparent_from_earth_and_body_topocentric(
        &earth,
        jd_tt,
        observer,
        body_position_at,
    ))
}

/// Build the barycentric-position closure for an SPK asteroid.
///
/// The SPK file stores the asteroid heliocentric (center = Sun, NAIF 10)
/// in ICRF; DE441 gives the Sun's barycentric position. Their sum is the
/// asteroid's barycentric ICRF position — consistent with how planets are
/// returned barycentric, so the same light-time loop applies.
///
/// Frame note: this is the one place two independently-realized frames are
/// added — the SPK's ICRF (frame code 1) and DE441's own reference frame.
/// The `DE4xx` series is aligned to ICRF at the ~milliarcsecond level, far
/// below astrological resolution, so the sum is treated as a single ICRF
/// frame. (An absolute HORIZONS asteroid fixture would bound this directly;
/// see the `spk_apparent` test TODO.)
///
/// Errors from either ephemeris fall back to `[0.0; 3]` (matching the
/// planet closure's `map_or`); the public entry points probe coverage up
/// front so a bad NAIF id surfaces a real error before this runs.
fn asteroid_body_position_at<'a>(
    ephem: &'a Ephemeris<'a>,
    spk: &'a SpkEphemeris,
    naif_id: i32,
) -> impl Fn(f64) -> [f64; 3] + 'a {
    move |jd: f64| {
        let sun = ephem
            .state(Body::Sun, jd)
            .map_or([0.0; 3], |s| s.position_km);
        let helio = spk
            .state(naif_id, et_of(jd))
            .map_or([0.0; 3], |s| s.position_km);
        [sun[0] + helio[0], sun[1] + helio[1], sun[2] + helio[2]]
    }
}

/// Compute the geocentric apparent ecliptic-of-date position of an SPK
/// asteroid, reusing the same pipeline as the planets.
///
/// The asteroid's barycentric ICRF position is `sun_bary(jd) +
/// helio(jd)`, where `helio` comes from `spk.state(naif_id, et_of(jd))`
/// (heliocentric, center = Sun) and `sun_bary` from `ephem.state(Body::Sun)`.
/// That barycentric closure is fed to the body-agnostic core, so light-time,
/// aberration, precession, nutation, and ecliptic rotation are identical to
/// the planet path.
///
/// # Errors
///
/// Returns [`PericynthionError`] if Earth's state cannot be read, or if
/// `naif_id` is not covered by `spk` at `jd_tt` (probed up front so a bad
/// id or out-of-coverage epoch surfaces a real error rather than silently
/// producing garbage).
pub fn apparent_ecliptic_position_spk(
    ephem: &Ephemeris,
    spk: &SpkEphemeris,
    naif_id: i32,
    jd_tt: f64,
) -> Result<EclipticPosition, PericynthionError> {
    let earth = ephem.state(Body::Earth, jd_tt)?;
    // Probe coverage up front: a bad NAIF id / out-of-range epoch must be a
    // hard error, not a [0;0;0] fallback hidden inside the light-time loop.
    spk.state(naif_id, et_of(jd_tt))?;
    let body_position_at = asteroid_body_position_at(ephem, spk, naif_id);
    Ok(apparent_from_earth_and_body(
        &earth,
        jd_tt,
        body_position_at,
    ))
}

/// Compute the topocentric apparent ecliptic-of-date position of an SPK
/// asteroid (observer parallax applied), reusing the planet pipeline.
///
/// Identical to [`apparent_ecliptic_position_spk`] but with the observer's
/// geocentric vector subtracted in the true equatorial frame of date.
///
/// # Errors
///
/// Returns [`PericynthionError`] if Earth's state cannot be read, or if
/// `naif_id` is not covered by `spk` at `jd_tt` (probed up front).
pub fn apparent_ecliptic_position_spk_topocentric(
    ephem: &Ephemeris,
    spk: &SpkEphemeris,
    naif_id: i32,
    jd_tt: f64,
    observer: &ObserverLocation,
) -> Result<EclipticPosition, PericynthionError> {
    let earth = ephem.state(Body::Earth, jd_tt)?;
    spk.state(naif_id, et_of(jd_tt))?;
    let body_position_at = asteroid_body_position_at(ephem, spk, naif_id);
    Ok(apparent_from_earth_and_body_topocentric(
        &earth,
        jd_tt,
        observer,
        body_position_at,
    ))
}

/// Apply the precession → nutation → ecliptic-rotation → spherical pipeline
/// to an ICRF J2000 position vector and return the ecliptic-of-date position.
///
/// This is the shared tail used by both `heliocentric_ecliptic_position` (planet
/// path, after Sun subtraction) and `heliocentric_ecliptic_position_spk` (asteroid
/// path, directly from the SPK heliocentric vector). Keeping it in one place
/// ensures both call sites get identical math and that a change in one is
/// automatically reflected in the other.
fn icrf_vector_to_ecliptic_of_date(v: &[f64; 3], jd_tt: f64) -> EclipticPosition {
    // Precession and nutation still apply — we want ecliptic of date, not J2000.
    let mean_of_date = precess_j2000_to_date(v, jd_tt);
    let eps_mean = mean_obliquity_rad(jd_tt);
    let true_of_date = nutate_mean_to_true(&mean_of_date, jd_tt, eps_mean);

    let nut = crate::coords::nutation::nutation(jd_tt);
    let eps_true = eps_mean + nut.delta_epsilon;
    let ecliptic_km = equatorial_to_ecliptic(&true_of_date, eps_true);

    let lon_rad = longitude_rad(&ecliptic_km);
    let lat_rad = latitude_rad(&ecliptic_km);
    let r_km = magnitude(&ecliptic_km);

    EclipticPosition {
        longitude_deg: lon_rad.to_degrees(),
        latitude_deg: lat_rad.to_degrees(),
        distance_au: r_km / AU_KM,
    }
}

/// Compute the heliocentric ecliptic-of-date position of a body.
///
/// The origin is the Sun's centre (not the Solar System Barycenter). All
/// bodies are valid targets, including Earth. The Sun itself returns a
/// zero-distance result — it is the heliocentric origin.
///
/// No annual aberration is applied (that correction is specific to a
/// geocentric, Earth-moving observer).
///
/// # Errors
///
/// Propagates I/O errors from the ephemeris if `JD_TT` is out of range.
pub fn heliocentric_ecliptic_position(
    ephem: &Ephemeris,
    body: Body,
    jd_tt: f64,
) -> Result<EclipticPosition, PericynthionError> {
    let sun_km = ephem.state(Body::Sun, jd_tt)?.position_km;

    let body_bary_km = match body {
        Body::Sun => sun_km,
        Body::Earth => ephem.state(Body::Earth, jd_tt)?.position_km,
        Body::Moon => {
            let earth = ephem.state(Body::Earth, jd_tt)?;
            let moon_geo = ephem.state(Body::Moon, jd_tt)?;
            [
                earth.position_km[0] + moon_geo.position_km[0],
                earth.position_km[1] + moon_geo.position_km[1],
                earth.position_km[2] + moon_geo.position_km[2],
            ]
        }
        _ => ephem.state(body, jd_tt)?.position_km,
    };

    let helio_km = [
        body_bary_km[0] - sun_km[0],
        body_bary_km[1] - sun_km[1],
        body_bary_km[2] - sun_km[2],
    ];

    Ok(icrf_vector_to_ecliptic_of_date(&helio_km, jd_tt))
}

/// Heliocentric ecliptic-of-date position of an SPK asteroid.
///
/// The SPK stores the body Sun-centred in ICRF, so this is the same
/// precession→nutation→ecliptic transform the planet heliocentric path uses,
/// applied directly to the SPK vector (no Sun subtraction, no aberration).
///
/// # Errors
///
/// Propagates SPK coverage/IO errors from [`SpkEphemeris::state`].
pub fn heliocentric_ecliptic_position_spk(
    spk: &SpkEphemeris,
    naif_id: i32,
    jd_tt: f64,
) -> Result<EclipticPosition, PericynthionError> {
    let helio_icrf = spk.state(naif_id, et_of(jd_tt))?.position_km;
    Ok(icrf_vector_to_ecliptic_of_date(&helio_icrf, jd_tt))
}

#[cfg(test)]
mod tests {
    // High-level integration tests for `apparent_ecliptic_position`
    // (plus topocentric/heliocentric variants) live in
    // `tests/acceptance_horizons.rs` — HORIZONS- and refchart-anchored.
    //
    // The lower-level submodules' unit tests validate each pipeline
    // step in isolation against textbook formulas.
}
