//! Integration test for [`pericynthion::compute`] — full chart orchestration.
//!
//! Uses the Lightning Strike reference chart (1955-11-12 22:04 PST, Universal
//! City CA) which has a Leo Ascendant. The test skips cleanly when
//! `$STARCAT_JPL_DATA` is unset.

use pericynthion::body::Body;
use pericynthion::houses::HouseSystem;
use pericynthion::jpl::{discover, header::parse as parse_header, reader::EphemerisFile};
use pericynthion::time::calendar::{Calendar, CivilDate};
use pericynthion::time::zone::Zone;
use pericynthion::{ChartRequest, ModeRequest, compute};
use std::path::PathBuf;

fn locate_jpl_paths() -> Option<(PathBuf, PathBuf)> {
    let dir = std::env::var("STARCAT_JPL_DATA").ok().map(PathBuf::from)?;
    let paths = discover::discover(&dir)
        .unwrap_or_else(|e| panic!("STARCAT_JPL_DATA autodiscovery failed: {e}"));
    Some((paths.header, paths.binary))
}

#[test]
fn compute_lightning_strike_leo_asc_frame() {
    let Some((header_path, binary_path)) = locate_jpl_paths() else {
        eprintln!("STARCAT_JPL_DATA not set — skipping compute integration test");
        return;
    };

    let header_src = std::fs::read_to_string(&header_path).expect("read DE441 header");
    let header = parse_header(&header_src).expect("parse DE441 header");
    let file = EphemerisFile::open(&binary_path, &header).expect("open DE441 binary");
    let ephem =
        pericynthion::ephemeris::Ephemeris::new(&file, &header).expect("build ephemeris facade");

    // Lightning Strike: 1955-11-12 22:04 PST (= UTC−8) → UT 1955-11-13 06:04.
    // Universal City CA: 34°N08'20", 118°W21'09".
    let request = ChartRequest {
        civil: CivilDate {
            year: 1955,
            month: 11,
            day: 12,
            hour: 22,
            minute: 4,
            second: 0.0,
        },
        calendar: Calendar::Gregorian,
        zone: Zone::fixed_hms(-8, 0, 0),
        mode: ModeRequest::Geocentric,
        lat_deg: Some(34.0 + 8.0 / 60.0 + 20.0 / 3600.0),
        lon_deg: Some(-(118.0 + 21.0 / 60.0 + 9.0 / 3600.0)),
        bodies: None, // all 10 classical bodies
        houses: vec![HouseSystem::Placidus, HouseSystem::WholeSign],
    };

    let chart = compute(&ephem, &request).expect("compute chart");

    // 10 classical bodies in geocentric mode.
    assert_eq!(
        chart.bodies.len(),
        10,
        "geocentric default body count should be 10"
    );

    // Angles present (lon was supplied).
    assert!(chart.angles.is_some(), "angles should be Some with lon+lat");

    // Nodes present in geocentric mode.
    assert!(
        chart.nodes.is_some(),
        "nodes should be Some in geocentric mode"
    );

    // House output count matches request.
    assert_eq!(
        chart.houses.len(),
        request.houses.len(),
        "houses output count should match request"
    );

    // UTC offset for PST (-8h) should be "-08:00".
    assert_eq!(chart.utc_offset, "-08:00", "PST UTC offset string");

    // Ascendant is Leo (120–150°).
    let ac_deg = chart
        .angles
        .as_ref()
        .unwrap()
        .ac_deg
        .expect("Ac present with lat+lon");
    assert!(
        (120.0..150.0).contains(&ac_deg),
        "Leo Ascendant expected [120°,150°), got {ac_deg:.3}°"
    );

    // Sun retrograde = false (Sun is never retrograde).
    let sun = chart
        .bodies
        .iter()
        .find(|b| b.body == Body::Sun)
        .expect("Sun present");
    assert!(!sun.retrograde, "Sun is never retrograde");

    // Lunar phase should be Some (Sun + Moon present in geocentric mode).
    assert!(chart.lunar_phase.is_some(), "lunar_phase should be Some");

    // Sect should be Some (Ac + Sun present).
    assert!(chart.sect.is_some(), "sect should be Some");
}

#[test]
fn nodes_and_lilith_present_without_latitude() {
    // Nodes and Black Moon Lilith are functions of the Moon's orbital geometry
    // at the instant — they need no observer latitude and no Ascendant. A
    // geocentric chart with a longitude but NO latitude (so there is no Ac)
    // must still carry both node and Lilith points.
    let Some((header_path, binary_path)) = locate_jpl_paths() else {
        eprintln!("STARCAT_JPL_DATA not set — skipping no-latitude nodes test");
        return;
    };

    let header_src = std::fs::read_to_string(&header_path).expect("read DE441 header");
    let header = parse_header(&header_src).expect("parse DE441 header");
    let file = EphemerisFile::open(&binary_path, &header).expect("open DE441 binary");
    let ephem =
        pericynthion::ephemeris::Ephemeris::new(&file, &header).expect("build ephemeris facade");

    let request = ChartRequest {
        civil: CivilDate {
            year: 1955,
            month: 11,
            day: 12,
            hour: 22,
            minute: 4,
            second: 0.0,
        },
        calendar: Calendar::Gregorian,
        zone: Zone::fixed_hms(-8, 0, 0),
        mode: ModeRequest::Geocentric,
        lat_deg: None, // no latitude → no Ascendant
        lon_deg: Some(-118.3525),
        bodies: None,
        houses: vec![],
    };

    let chart = compute(&ephem, &request).expect("compute chart");

    // No latitude → angles exist (MC/IC from longitude) but no Ascendant.
    assert!(
        chart.angles.is_some(),
        "angles present from longitude (MC/IC)"
    );
    assert!(
        chart.angles.as_ref().unwrap().ac_deg.is_none(),
        "no Ascendant without latitude"
    );

    // Nodes and Lilith must STILL be present — they do not depend on latitude.
    assert!(
        chart.nodes.is_some(),
        "nodes present without latitude (orbital geometry only)"
    );
    assert!(
        chart.lilith.is_some(),
        "Lilith present without latitude (orbital geometry only)"
    );
}
