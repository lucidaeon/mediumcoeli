//! Integration test: compute body positions at J2000 using DE441
//! and validate against well-known physical ranges.
//!
//! Tolerances here are deliberately loose (millions of km, days of
//! velocity) — this test verifies the *interpolation arithmetic*
//! produces something physically plausible. Fine-grained accuracy is
//! the job of the cached HORIZONS comparison tests.

use pericynthion::body::Body;
use pericynthion::ephemeris::Ephemeris;
use pericynthion::jpl::{discover, header::parse, reader::EphemerisFile};
use std::path::PathBuf;

const AU_KM: f64 = 149_597_870.7;

fn locate_dir() -> Option<PathBuf> {
    let val = std::env::var_os("STARCAT_JPL_DATA")?;
    Some(PathBuf::from(val))
}

/// Setup helper: open the real JPL file once per test.
fn setup() -> Option<(EphemerisFile, pericynthion::jpl::header::Header)> {
    let dir = locate_dir()?;
    let paths = discover::discover(&dir)
        .unwrap_or_else(|e| panic!("autodiscovery failed for {}: {e}", dir.display()));
    let source = std::fs::read_to_string(&paths.header).expect("read header");
    let header = parse(&source).expect("parse header");
    let file = EphemerisFile::open(&paths.binary, &header).expect("open ephemeris file");
    Some((file, header))
}

#[test]
fn sun_barycentric_position_at_j2000_is_near_ssb() {
    let Some((file, header)) = setup() else {
        eprintln!("STARCAT_JPL_DATA not set — skipping integration test");
        return;
    };
    let ephem = Ephemeris::new(&file, &header).unwrap();
    let s = ephem.state(Body::Sun, 2_451_545.0).unwrap();
    let r = magnitude(&s.position_km);
    // The Sun's distance from the SSB is dominated by Jupiter's pull;
    // it stays within ≈ 1.5 million km of the barycenter. Use 2M km
    // as a generous upper bound.
    assert!(
        r < 2_000_000.0,
        "Sun at J2000 should be within ≈ 1.5M km of SSB; got {r} km"
    );
}

#[test]
fn earth_barycentric_distance_at_j2000_is_about_one_au() {
    let Some((file, header)) = setup() else {
        eprintln!("STARCAT_JPL_DATA not set — skipping integration test");
        return;
    };
    let ephem = Ephemeris::new(&file, &header).unwrap();
    let e = ephem.state(Body::Earth, 2_451_545.0).unwrap();
    let r = magnitude(&e.position_km);
    // Earth perihelion is ≈ 0.983 AU (early January); aphelion ≈ 1.017
    // AU (early July). J2000 is January 1.5 so we are near perihelion.
    let r_au = r / AU_KM;
    assert!(
        (0.98..=1.02).contains(&r_au),
        "Earth-SSB at J2000 should be ≈ 0.98 AU (near perihelion); got {r_au} AU"
    );
}

#[test]
fn earth_sun_distance_at_j2000_is_about_one_au() {
    let Some((file, header)) = setup() else {
        eprintln!("STARCAT_JPL_DATA not set — skipping integration test");
        return;
    };
    let ephem = Ephemeris::new(&file, &header).unwrap();
    let earth = ephem.state(Body::Earth, 2_451_545.0).unwrap();
    let sun = ephem.state(Body::Sun, 2_451_545.0).unwrap();
    let mut delta = [0.0_f64; 3];
    for (i, item) in delta.iter_mut().enumerate() {
        *item = earth.position_km[i] - sun.position_km[i];
    }
    let r_au = magnitude(&delta) / AU_KM;
    // Near perihelion: 0.9833 AU. Allow generous range.
    assert!(
        (0.97..=1.00).contains(&r_au),
        "Earth-Sun distance at J2000 should be ≈ 0.983 AU; got {r_au} AU"
    );
}

#[test]
fn moon_geocentric_distance_at_j2000_is_in_lunar_orbit_range() {
    let Some((file, header)) = setup() else {
        eprintln!("STARCAT_JPL_DATA not set — skipping integration test");
        return;
    };
    let ephem = Ephemeris::new(&file, &header).unwrap();
    let m = ephem.state(Body::Moon, 2_451_545.0).unwrap();
    let r = magnitude(&m.position_km);
    // Lunar orbit: perigee ≈ 356,500 km, apogee ≈ 406,700 km.
    assert!(
        (350_000.0..=410_000.0).contains(&r),
        "Moon-Earth distance at J2000 should be in lunar orbit range; got {r} km"
    );
}

#[test]
fn velocity_matches_position_derivative_via_central_difference() {
    // Independent sanity check: the velocity reported by Chebyshev
    // derivative should equal a central-difference estimate using two
    // position evaluations near the target JD. If derivative scaling is
    // off (e.g. wrong dτ/dt factor), this fails loudly.
    let Some((file, header)) = setup() else {
        eprintln!("STARCAT_JPL_DATA not set — skipping integration test");
        return;
    };
    let ephem = Ephemeris::new(&file, &header).unwrap();
    let jd = 2_451_545.0;
    let h = 0.01_f64; // days

    for &body in &[Body::Mercury, Body::Earth, Body::Jupiter, Body::Moon] {
        let s_plus = ephem.state(body, jd + h).unwrap();
        let s_minus = ephem.state(body, jd - h).unwrap();
        let s_mid = ephem.state(body, jd).unwrap();
        for axis in 0..3 {
            let analytic = s_mid.velocity_km_per_day[axis];
            let numeric = (s_plus.position_km[axis] - s_minus.position_km[axis]) / (2.0 * h);
            let rel_err = ((analytic - numeric) / analytic.abs().max(1e-6)).abs();
            // Tolerance: central difference is O(h²); for h=0.01 d the
            // truncation noise should be well below 1e-4 relative.
            assert!(
                rel_err < 1e-4,
                "{body:?} axis {axis}: analytic={analytic}, numeric={numeric}, \
                 relative error {rel_err}"
            );
        }
    }
}

#[test]
fn earth_plus_moon_geo_equals_emb_within_emrat_consistency() {
    // The derivation Earth = EMB - Moon/(1+EMRAT) must be consistent
    // with Moon (geocentric) + Earth = (m_M·Moon_bary + m_E·Earth)/(m_E+m_M)·(1+EMRAT)/EMRAT
    // Simpler: EMB == (m_E·Earth + m_M·Moon_bary) / (m_E+m_M)
    //              == Earth + Moon_geo · (m_M)/(m_E+m_M)
    //              == Earth + Moon_geo / (1+EMRAT)
    // So: Earth + Moon_geo/(1+EMRAT) must equal EMB to machine precision.
    let Some((file, header)) = setup() else {
        eprintln!("STARCAT_JPL_DATA not set — skipping integration test");
        return;
    };
    let ephem = Ephemeris::new(&file, &header).unwrap();
    let emrat = ephem.emrat();
    let f = 1.0 / (1.0 + emrat);
    let jd = 2_451_545.0;
    let earth = ephem.state(Body::Earth, jd).unwrap();
    let moon = ephem.state(Body::Moon, jd).unwrap();
    let emb = ephem.state(Body::EarthMoonBarycenter, jd).unwrap();
    for axis in 0..3 {
        let reconstructed = earth.position_km[axis] + moon.position_km[axis] * f;
        let err = (reconstructed - emb.position_km[axis]).abs();
        assert!(
            err < 1e-9,
            "axis {axis}: Earth + Moon·f = {reconstructed}, EMB = {}; err {err}",
            emb.position_km[axis]
        );
    }
}

fn magnitude(v: &[f64; 3]) -> f64 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}
