//! Tests for the `SFcht` binary writer.
//!
//! Strategy: `write_file` → `parse_file` and verify the round-trip. We do NOT
//! try to match Solar Fire's exact byte layout for fields we invent (version,
//! `record_count`) — we only verify what `parse_file` can read back correctly.

use astrogram::chart::{
    Chart, CoordinateSystem, EventType, HouseSystem, Latitude, Longitude, SubChart, Zodiac,
};
use astrogram::sfcht::{parse_file, write_file};

#[allow(clippy::too_many_arguments)]
fn chart(
    name: &str,
    lat: f64,
    lon: f64,
    year: i16,
    month: u8,
    day: u8,
    hour: u8,
    minute: u8,
    second: u8,
    tz: f64,
    event_type: EventType,
    city: Option<&str>,
    region: Option<&str>,
    source_rating: Option<&str>,
    notes: Option<&str>,
) -> Chart {
    Chart {
        name: name.to_string(),
        secondary_name: None,
        city: city.map(str::to_string),
        region: region.map(str::to_string),
        longitude: Longitude::new(lon).unwrap(),
        latitude: Latitude::new(lat).unwrap(),
        year,
        month,
        day,
        hour,
        minute,
        second,
        tz_offset_hours: tz,
        tz_abbreviation: None,
        is_lmt: false,
        event_type,
        source_rating: source_rating.map(str::to_string),
        house_system: HouseSystem::Placidus,
        zodiac: Zodiac::Tropical,
        coordinate_system: CoordinateSystem::Geocentric,
        sub_charts: vec![],
        notes: notes.map(str::to_string),
    }
}

// --- basic structure ---

#[test]
fn empty_slice_produces_header_only() {
    let bytes = write_file(&[]).unwrap();
    let (hdr, charts) = parse_file(&bytes).unwrap();
    assert_eq!(hdr.record_count, 0);
    assert!(charts.is_empty());
}

#[test]
fn single_chart_record_count_is_1() {
    let c = chart(
        "Test",
        51.5,
        -0.117,
        2000,
        1,
        1,
        12,
        0,
        0,
        0.0,
        EventType::Male,
        None,
        None,
        None,
        None,
    );
    let bytes = write_file(&[c]).unwrap();
    let (hdr, charts) = parse_file(&bytes).unwrap();
    assert_eq!(hdr.record_count, 1);
    assert_eq!(charts.len(), 1);
}

// --- name round-trip ---

#[test]
fn round_trip_name() {
    let c = chart(
        "Ada Lovelace",
        51.5,
        -0.117,
        1815,
        12,
        10,
        7,
        30,
        0,
        0.0,
        EventType::Female,
        None,
        None,
        Some("B"),
        None,
    );
    let (_, charts) = parse_file(&write_file(&[c]).unwrap()).unwrap();
    assert_eq!(charts[0].name, "Ada Lovelace");
}

// --- date/time ---

#[test]
fn round_trip_date() {
    let c = chart(
        "Test",
        51.5,
        -0.117,
        1955,
        11,
        13,
        6,
        4,
        0,
        0.0,
        EventType::Male,
        None,
        None,
        None,
        None,
    );
    let (_, charts) = parse_file(&write_file(&[c]).unwrap()).unwrap();
    assert_eq!(
        (charts[0].year, charts[0].month, charts[0].day),
        (1955, 11, 13)
    );
}

#[test]
fn round_trip_ancient_year() {
    let c = chart(
        "Valens",
        36.207,
        36.157,
        120,
        2,
        8,
        18,
        35,
        1,
        2.404,
        EventType::Male,
        Some("Antioch"),
        None,
        Some("B"),
        None,
    );
    let (_, charts) = parse_file(&write_file(&[c]).unwrap()).unwrap();
    assert_eq!(charts[0].year, 120);
}

#[test]
fn round_trip_time() {
    let c = chart(
        "Test",
        51.5,
        -0.117,
        2000,
        1,
        1,
        18,
        35,
        1,
        0.0,
        EventType::Male,
        None,
        None,
        None,
        None,
    );
    let (_, charts) = parse_file(&write_file(&[c]).unwrap()).unwrap();
    assert_eq!(
        (charts[0].hour, charts[0].minute, charts[0].second),
        (18, 35, 1)
    );
}

// --- coordinates (sign convention: ISO 6709 in, ISO 6709 out) ---

#[test]
fn positive_longitude_round_trips() {
    // ISO 6709 +East stored as +West on disk; parse_file flips back
    let lon = 36.0 + 9.0 / 60.0 + 26.0 / 3600.0;
    let c = chart(
        "Valens",
        36.207,
        lon,
        120,
        2,
        8,
        18,
        35,
        1,
        2.404,
        EventType::Male,
        None,
        None,
        None,
        None,
    );
    let (_, charts) = parse_file(&write_file(&[c]).unwrap()).unwrap();
    assert!(
        (charts[0].longitude.degrees() - lon).abs() < 1e-4,
        "{}",
        charts[0].longitude.degrees()
    );
}

#[test]
fn negative_longitude_round_trips() {
    let lon = -(118.0 + 21.0 / 60.0 + 9.0 / 3600.0);
    let c = chart(
        "Test",
        34.14,
        lon,
        1955,
        11,
        12,
        22,
        4,
        0,
        -8.0,
        EventType::Male,
        None,
        None,
        None,
        None,
    );
    let (_, charts) = parse_file(&write_file(&[c]).unwrap()).unwrap();
    assert!(
        (charts[0].longitude.degrees() - lon).abs() < 1e-4,
        "{}",
        charts[0].longitude.degrees()
    );
}

#[test]
fn positive_latitude_round_trips() {
    let lat = 36.0 + 12.0 / 60.0 + 24.0 / 3600.0;
    let c = chart(
        "Valens",
        lat,
        36.157,
        120,
        2,
        8,
        18,
        35,
        1,
        2.404,
        EventType::Male,
        None,
        None,
        None,
        None,
    );
    let (_, charts) = parse_file(&write_file(&[c]).unwrap()).unwrap();
    assert!(
        (charts[0].latitude.degrees() - lat).abs() < 1e-4,
        "{}",
        charts[0].latitude.degrees()
    );
}

#[test]
fn negative_latitude_round_trips() {
    let lat = -(33.0 + 52.0 / 60.0);
    let c = chart(
        "Test",
        lat,
        -70.667,
        2000,
        1,
        1,
        12,
        0,
        0,
        -4.0,
        EventType::Unspecified,
        None,
        None,
        None,
        None,
    );
    let (_, charts) = parse_file(&write_file(&[c]).unwrap()).unwrap();
    assert!(
        (charts[0].latitude.degrees() - lat).abs() < 1e-4,
        "{}",
        charts[0].latitude.degrees()
    );
}

// --- timezone ---

#[test]
fn positive_tz_round_trips() {
    let tz = 2.0 + 24.0 / 60.0 + 14.0 / 3600.0;
    let c = chart(
        "Valens",
        36.207,
        36.157,
        120,
        2,
        8,
        18,
        35,
        1,
        tz,
        EventType::Male,
        None,
        None,
        None,
        None,
    );
    let (_, charts) = parse_file(&write_file(&[c]).unwrap()).unwrap();
    assert!(
        (charts[0].tz_offset_hours - tz).abs() < 1e-4,
        "{}",
        charts[0].tz_offset_hours
    );
}

#[test]
fn negative_tz_round_trips() {
    let c = chart(
        "Test",
        34.14,
        -118.35,
        1955,
        11,
        12,
        22,
        4,
        0,
        -8.0,
        EventType::Male,
        None,
        None,
        None,
        None,
    );
    let (_, charts) = parse_file(&write_file(&[c]).unwrap()).unwrap();
    assert!(
        (charts[0].tz_offset_hours - -8.0).abs() < 1e-4,
        "{}",
        charts[0].tz_offset_hours
    );
}

// --- event type ---

#[test]
fn all_event_types_round_trip() {
    for et in [
        EventType::Male,
        EventType::Female,
        EventType::Horary,
        EventType::Event,
        EventType::Unspecified,
    ] {
        let c = chart(
            "Test", 51.5, -0.117, 2000, 1, 1, 12, 0, 0, 0.0, et, None, None, None, None,
        );
        let (_, charts) = parse_file(&write_file(&[c]).unwrap()).unwrap();
        assert_eq!(
            charts[0].event_type, et,
            "event_type {et:?} did not round-trip"
        );
    }
}

// --- optional fields ---

#[test]
fn city_round_trips() {
    let c = chart(
        "Valens",
        36.207,
        36.157,
        120,
        2,
        8,
        18,
        35,
        1,
        2.404,
        EventType::Male,
        Some("Antioch, Turkey"),
        None,
        Some("B"),
        None,
    );
    let (_, charts) = parse_file(&write_file(&[c]).unwrap()).unwrap();
    assert_eq!(charts[0].city.as_deref(), Some("Antioch, Turkey"));
}

#[test]
fn none_city_round_trips() {
    let c = chart(
        "Test",
        51.5,
        -0.117,
        2000,
        1,
        1,
        12,
        0,
        0,
        0.0,
        EventType::Male,
        None,
        None,
        None,
        None,
    );
    let (_, charts) = parse_file(&write_file(&[c]).unwrap()).unwrap();
    assert!(charts[0].city.is_none());
}

#[test]
fn source_rating_round_trips() {
    let c = chart(
        "Test",
        51.5,
        -0.117,
        2000,
        1,
        1,
        12,
        0,
        0,
        0.0,
        EventType::Male,
        None,
        None,
        Some("AA"),
        None,
    );
    let (_, charts) = parse_file(&write_file(&[c]).unwrap()).unwrap();
    assert_eq!(charts[0].source_rating.as_deref(), Some("AA"));
}

#[test]
fn notes_round_trip() {
    let c = chart(
        "Test",
        51.5,
        -0.117,
        2000,
        1,
        1,
        12,
        0,
        0,
        0.0,
        EventType::Male,
        None,
        None,
        None,
        Some("A note."),
    );
    let (_, charts) = parse_file(&write_file(&[c]).unwrap()).unwrap();
    assert_eq!(charts[0].notes.as_deref(), Some("A note."));
}

#[test]
fn none_notes_round_trips() {
    let c = chart(
        "Test",
        51.5,
        -0.117,
        2000,
        1,
        1,
        12,
        0,
        0,
        0.0,
        EventType::Male,
        None,
        None,
        None,
        None,
    );
    let (_, charts) = parse_file(&write_file(&[c]).unwrap()).unwrap();
    assert!(charts[0].notes.is_none());
}

#[test]
fn house_system_round_trips() {
    for hs in [
        HouseSystem::Placidus,
        HouseSystem::WholeSign,
        HouseSystem::Koch,
    ] {
        let mut c = chart(
            "Test",
            51.5,
            -0.117,
            2000,
            1,
            1,
            12,
            0,
            0,
            0.0,
            EventType::Male,
            None,
            None,
            None,
            None,
        );
        c.house_system = hs;
        let (_, charts) = parse_file(&write_file(&[c]).unwrap()).unwrap();
        assert_eq!(
            charts[0].house_system, hs,
            "house_system {hs:?} did not round-trip"
        );
    }
}

#[test]
fn zodiac_round_trips() {
    for z in [Zodiac::Tropical, Zodiac::Lahiri, Zodiac::FaganAllen] {
        let mut c = chart(
            "Test",
            51.5,
            -0.117,
            2000,
            1,
            1,
            12,
            0,
            0,
            0.0,
            EventType::Male,
            None,
            None,
            None,
            None,
        );
        c.zodiac = z;
        let (_, charts) = parse_file(&write_file(&[c]).unwrap()).unwrap();
        assert_eq!(charts[0].zodiac, z, "zodiac {z:?} did not round-trip");
    }
}

// --- sub-charts ---

#[test]
fn chart_with_sub_chart_round_trips() {
    let mut c = chart(
        "Test",
        51.5,
        -0.117,
        2000,
        1,
        1,
        12,
        0,
        0,
        0.0,
        EventType::Male,
        None,
        None,
        None,
        None,
    );
    c.sub_charts.push(SubChart {
        name: "Progressed".to_string(),
        city: Some("London".to_string()),
        region: None,
        longitude: Longitude::new(-0.117).unwrap(),
        latitude: Latitude::new(51.5).unwrap(),
        year: 2010,
        month: 6,
        day: 15,
        hour: 8,
        minute: 30,
        second: 0,
        tz_offset_hours: 1.0,
        tz_abbreviation: None,
        is_lmt: false,
        notes: None,
    });
    let (_, charts) = parse_file(&write_file(&[c]).unwrap()).unwrap();
    assert_eq!(charts[0].sub_charts.len(), 1);
    assert_eq!(charts[0].sub_charts[0].name, "Progressed");
    assert_eq!(
        (
            charts[0].sub_charts[0].year,
            charts[0].sub_charts[0].month,
            charts[0].sub_charts[0].day
        ),
        (2010, 6, 15)
    );
}

// --- multi-record ---

#[test]
fn multiple_charts_round_trip() {
    let a = chart(
        "Alice",
        51.5,
        -0.117,
        2000,
        1,
        1,
        12,
        0,
        0,
        0.0,
        EventType::Female,
        None,
        None,
        Some("AA"),
        None,
    );
    let b = chart(
        "Bob",
        40.714,
        -74.006,
        1985,
        2,
        2,
        8,
        30,
        0,
        -5.0,
        EventType::Male,
        None,
        None,
        Some("A"),
        None,
    );
    let (hdr, charts) = parse_file(&write_file(&[a, b]).unwrap()).unwrap();
    assert_eq!(hdr.record_count, 2);
    assert_eq!(charts.len(), 2);
    assert_eq!(charts[0].name, "Alice");
    assert_eq!(charts[1].name, "Bob");
}
