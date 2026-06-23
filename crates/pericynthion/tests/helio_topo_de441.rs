//! Physics-sanity integration tests for heliocentric and topocentric pipelines.
//!
//! All tests require `STARCAT_JPL_DATA` pointing at a DE441 directory.
//! The tests assert physically expected magnitudes, not exact values, so
//! they are robust across DE releases and don't need separate HORIZONS fixtures.

use pericynthion::body::Body;
use pericynthion::coords::apparent::{
    apparent_ecliptic_position, apparent_ecliptic_position_topocentric,
    heliocentric_ecliptic_position,
};
use pericynthion::coords::topocentric::ObserverLocation;
use pericynthion::ephemeris::Ephemeris;
use pericynthion::jpl::{discover, header::parse as parse_header, reader::EphemerisFile};
use std::path::PathBuf;

// J2000.0 — a convenient reference epoch present in all DE releases.
const J2000: f64 = 2_451_545.0;

fn locate_jpl() -> Option<(PathBuf, PathBuf)> {
    let dir = std::env::var("STARCAT_JPL_DATA").ok().map(PathBuf::from)?;
    let loc =
        discover::locate(&dir).unwrap_or_else(|e| panic!("STARCAT_JPL_DATA locate failed: {e}"));
    let paths = match loc {
        discover::DatasetLocation::Binary(p) => p,
        discover::DatasetLocation::Ascii { .. } => {
            panic!("expected binary DE dataset under {}", dir.display())
        }
    };
    Some((paths.header, paths.binary))
}

fn make_ephem(
    header_path: &PathBuf,
    binary_path: &PathBuf,
) -> (EphemerisFile, pericynthion::jpl::header::Header) {
    let src = std::fs::read_to_string(header_path).expect("read header");
    let header = parse_header(&src).expect("parse header");
    let file = EphemerisFile::open(binary_path, &header).expect("open ephemeris");
    (file, header)
}

// =============================================================================
// Heliocentric
// =============================================================================

#[test]
fn heliocentric_sun_is_at_origin() {
    let Some((hp, bp)) = locate_jpl() else {
        eprintln!("STARCAT_JPL_DATA not set — skipping integration test");
        return;
    };
    let (file, header) = make_ephem(&hp, &bp);
    let ephem = Ephemeris::new(&file, &header).unwrap();
    let pos = heliocentric_ecliptic_position(&ephem, Body::Sun, J2000).unwrap();
    // Sun is the heliocentric origin by definition — exact zero is correct.
    #[allow(clippy::float_cmp)]
    {
        assert_eq!(
            pos.distance_au, 0.0,
            "Sun must be at the heliocentric origin"
        );
    }
}

#[test]
fn heliocentric_earth_is_about_1_au() {
    let Some((hp, bp)) = locate_jpl() else {
        eprintln!("STARCAT_JPL_DATA not set — skipping integration test");
        return;
    };
    let (file, header) = make_ephem(&hp, &bp);
    let ephem = Ephemeris::new(&file, &header).unwrap();
    let pos = heliocentric_ecliptic_position(&ephem, Body::Earth, J2000).unwrap();
    // Earth's heliocentric distance stays within 0.983–1.017 AU.
    assert!(
        (0.983..=1.017).contains(&pos.distance_au),
        "Earth heliocentric distance = {} AU (expected ~1 AU)",
        pos.distance_au
    );
}

#[test]
fn heliocentric_jupiter_is_several_au() {
    let Some((hp, bp)) = locate_jpl() else {
        eprintln!("STARCAT_JPL_DATA not set — skipping integration test");
        return;
    };
    let (file, header) = make_ephem(&hp, &bp);
    let ephem = Ephemeris::new(&file, &header).unwrap();
    let pos = heliocentric_ecliptic_position(&ephem, Body::Jupiter, J2000).unwrap();
    // Jupiter's heliocentric distance: 4.95–5.46 AU.
    assert!(
        (4.9..=5.5).contains(&pos.distance_au),
        "Jupiter heliocentric distance = {} AU",
        pos.distance_au
    );
}

// Heliocentric and geocentric longitudes of a distant body must differ.
// Jupiter at J2000: geocentric shifts by ~1/5 AU relative to heliocentric
// (Earth is ~1 AU from Sun; Jupiter is ~5 AU), so longitude difference can
// be several degrees.
#[test]
fn heliocentric_differs_from_geocentric_for_jupiter() {
    let Some((hp, bp)) = locate_jpl() else {
        eprintln!("STARCAT_JPL_DATA not set — skipping integration test");
        return;
    };
    let (file, header) = make_ephem(&hp, &bp);
    let ephem = Ephemeris::new(&file, &header).unwrap();
    let geo = apparent_ecliptic_position(&ephem, Body::Jupiter, J2000).unwrap();
    let helio = heliocentric_ecliptic_position(&ephem, Body::Jupiter, J2000).unwrap();
    let diff = (geo.longitude_deg - helio.longitude_deg)
        .abs()
        .rem_euclid(360.0)
        .min(
            360.0
                - (geo.longitude_deg - helio.longitude_deg)
                    .abs()
                    .rem_euclid(360.0),
        );
    assert!(
        diff > 0.1,
        "geocentric and heliocentric Jupiter must differ; diff = {diff}°"
    );
}

// =============================================================================
// Topocentric
// =============================================================================

// Universal City CA — ref_lightning_strike: 34°08'20"N 118°21'09"W.
fn lightning_strike_observer() -> ObserverLocation {
    ObserverLocation {
        lat_deg: 34.1389,
        lon_deg: -118.3525,
        elev_m: 165.0,
    }
}

// The Moon's topocentric parallax is up to ~57' (~1°). Any real observer
// position should produce a measurable difference from the geocentric Moon.
#[test]
fn topocentric_moon_shift_is_bounded_and_nonzero() {
    let Some((hp, bp)) = locate_jpl() else {
        eprintln!("STARCAT_JPL_DATA not set — skipping integration test");
        return;
    };
    let (file, header) = make_ephem(&hp, &bp);
    let ephem = Ephemeris::new(&file, &header).unwrap();
    let observer = lightning_strike_observer();

    let geo = apparent_ecliptic_position(&ephem, Body::Moon, J2000).unwrap();
    let topo =
        apparent_ecliptic_position_topocentric(&ephem, Body::Moon, J2000, &observer).unwrap();

    let diff_arcsec = (geo.longitude_deg - topo.longitude_deg).abs() * 3600.0;

    // Shift must be between 0" and 3600" (60' = 1°).
    assert!(
        diff_arcsec > 0.0 && diff_arcsec < 3_600.0,
        "Moon topocentric lon shift = {diff_arcsec}\" (expected 0\"–3600\")"
    );
}

// The Sun's equatorial horizontal parallax is ~8.79" at mean distance.
// Topocentric shift should be < 10" for any reasonable observer.
#[test]
fn topocentric_sun_shift_is_small() {
    let Some((hp, bp)) = locate_jpl() else {
        eprintln!("STARCAT_JPL_DATA not set — skipping integration test");
        return;
    };
    let (file, header) = make_ephem(&hp, &bp);
    let ephem = Ephemeris::new(&file, &header).unwrap();
    let observer = lightning_strike_observer();

    let geo = apparent_ecliptic_position(&ephem, Body::Sun, J2000).unwrap();
    let topo = apparent_ecliptic_position_topocentric(&ephem, Body::Sun, J2000, &observer).unwrap();

    let diff_arcsec = (geo.longitude_deg - topo.longitude_deg).abs() * 3600.0;
    assert!(
        diff_arcsec < 10.0,
        "Sun topocentric lon shift = {diff_arcsec}\" (expected < 10\")"
    );
}

// Jupiter at ~5 AU has an equatorial horizontal parallax of ~1.76". Any
// observer on Earth's surface can produce a shift up to that magnitude.
// Contrast this with the Moon's ~57' — the outer-planet effect is real
// but small enough to be irrelevant for most astrological purposes.
#[test]
fn topocentric_jupiter_shift_is_bounded_by_parallax() {
    let Some((hp, bp)) = locate_jpl() else {
        eprintln!("STARCAT_JPL_DATA not set — skipping integration test");
        return;
    };
    let (file, header) = make_ephem(&hp, &bp);
    let ephem = Ephemeris::new(&file, &header).unwrap();
    let observer = lightning_strike_observer();

    let geo = apparent_ecliptic_position(&ephem, Body::Jupiter, J2000).unwrap();
    let topo =
        apparent_ecliptic_position_topocentric(&ephem, Body::Jupiter, J2000, &observer).unwrap();

    let diff_arcsec = (geo.longitude_deg - topo.longitude_deg).abs() * 3600.0;
    // Maximum parallax for Jupiter ≈ 1.76"; must be < 3" for any observer/epoch.
    assert!(
        diff_arcsec < 3.0,
        "Jupiter topocentric shift = {diff_arcsec}\" (expected < 3\")"
    );
    // Must also be non-zero — the observer is not at Earth's centre.
    assert!(diff_arcsec > 0.0, "topocentric and geocentric must differ");
}
