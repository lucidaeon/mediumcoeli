//! Tests for the Zeus `.zdb` text-format writer.

use astrogram::chart::{
    Chart, CoordinateSystem, EventType, HouseSystem, Latitude, Longitude, Zodiac,
};
use astrogram::zeus::{parse_file, write_file};

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
    source_rating: Option<&str>,
    notes: Option<&str>,
) -> Chart {
    Chart {
        name: name.to_string(),
        secondary_name: None,
        city: city.map(str::to_string),
        region: None,
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

// --- basic output ---

#[test]
fn empty_slice_gives_empty_string() {
    assert!(write_file(&[]).is_empty());
}

#[test]
fn single_chart_produces_one_line() {
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
    );
    assert_eq!(write_file(&[c]).lines().count(), 1);
}

#[test]
fn output_has_at_least_16_semicolon_delimited_fields() {
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
    );
    let out = write_file(&[c]);
    let line = out.lines().next().unwrap();
    assert!(
        line.split(';').count() >= 16,
        "fields: {}",
        line.split(';').count()
    );
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
        Some("London"),
        Some("B"),
        None,
    );
    let parsed = parse_file(&write_file(&[c])).unwrap();
    assert_eq!(parsed[0].name, "Ada Lovelace");
}

// --- date round-trip ---

#[test]
fn round_trip_date() {
    let c = chart(
        "Test",
        51.5,
        -0.117,
        1984,
        11,
        1,
        12,
        0,
        0,
        0.0,
        EventType::Male,
        None,
        Some("AA"),
        None,
    );
    let parsed = parse_file(&write_file(&[c])).unwrap();
    assert_eq!(
        (parsed[0].year, parsed[0].month, parsed[0].day),
        (1984, 11, 1)
    );
}

#[test]
fn round_trip_ancient_year() {
    // 4-digit year, zero-padded
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
        None,
        Some("B"),
        None,
    );
    let parsed = parse_file(&write_file(&[c])).unwrap();
    assert_eq!(parsed[0].year, 120);
}

// --- time round-trip ---

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
    );
    let parsed = parse_file(&write_file(&[c])).unwrap();
    assert_eq!(
        (parsed[0].hour, parsed[0].minute, parsed[0].second),
        (18, 35, 1)
    );
}

// --- DMS coordinate formatting ---

#[test]
fn north_east_coordinates_format_correctly() {
    // Antioch: N36.12.24, E036.09.26
    let lat = 36.0 + 12.0 / 60.0 + 24.0 / 3600.0;
    let lon = 36.0 + 9.0 / 60.0 + 26.0 / 3600.0;
    let c = chart(
        "Valens",
        lat,
        lon,
        120,
        2,
        8,
        18,
        35,
        1,
        2.0 + 24.0 / 60.0 + 14.0 / 3600.0,
        EventType::Male,
        Some("Antioch"),
        Some("B"),
        None,
    );
    let out = write_file(&[c]);
    assert!(out.contains("N36.12.24"), "lat not found in: {out}");
    assert!(out.contains("E036.09.26"), "lon not found in: {out}");
}

#[test]
fn south_west_coordinates_format_correctly() {
    // Santiago: S33.52.00, W070.40.00
    let lat = -(33.0 + 52.0 / 60.0);
    let lon = -(70.0 + 40.0 / 60.0);
    let c = chart(
        "Test",
        lat,
        lon,
        2000,
        1,
        1,
        12,
        0,
        0,
        -4.0,
        EventType::Unspecified,
        Some("Santiago"),
        Some("AA"),
        None,
    );
    let out = write_file(&[c]);
    assert!(out.contains("S33.52.00"), "lat not found in: {out}");
    assert!(out.contains("W070.40.00"), "lon not found in: {out}");
}

#[test]
fn coordinates_round_trip() {
    let lat = 36.0 + 12.0 / 60.0 + 24.0 / 3600.0;
    let lon = 36.0 + 9.0 / 60.0 + 26.0 / 3600.0;
    let c = chart(
        "Valens",
        lat,
        lon,
        120,
        2,
        8,
        18,
        35,
        1,
        2.0 + 24.0 / 60.0 + 14.0 / 3600.0,
        EventType::Male,
        None,
        Some("B"),
        None,
    );
    let parsed = parse_file(&write_file(&[c])).unwrap();
    assert!(
        (parsed[0].latitude.degrees() - lat).abs() < 1e-9,
        "{}",
        parsed[0].latitude.degrees()
    );
    assert!(
        (parsed[0].longitude.degrees() - lon).abs() < 1e-9,
        "{}",
        parsed[0].longitude.degrees()
    );
}

// --- UTC offset formatting ---

#[test]
fn positive_utc_offset_formats_correctly() {
    // +02:24:14
    let tz = 2.0 + 24.0 / 60.0 + 14.0 / 3600.0;
    let c = chart(
        "Test",
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
        Some("B"),
        None,
    );
    let out = write_file(&[c]);
    assert!(out.contains("+02:24:14"), "offset not found in: {out}");
}

#[test]
fn negative_utc_offset_formats_correctly() {
    // -07:00:00
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
        Some("A"),
        None,
    );
    let out = write_file(&[c]);
    assert!(out.contains("-08:00:00"), "offset not found in: {out}");
}

#[test]
fn near_zero_negative_offset_formats_correctly() {
    // -00:05:19
    let tz = -(5.0 / 60.0 + 19.0 / 3600.0);
    let c = chart(
        "Lilly",
        52.815,
        -1.333,
        1602,
        5,
        11,
        2,
        0,
        1,
        tz,
        EventType::Male,
        None,
        Some("B"),
        None,
    );
    let out = write_file(&[c]);
    assert!(out.contains("-00:05:19"), "offset not found in: {out}");
}

#[test]
fn utc_offset_round_trips() {
    let tz = -(5.0 / 60.0 + 19.0 / 3600.0);
    let c = chart(
        "Lilly",
        52.815,
        -1.333,
        1602,
        5,
        11,
        2,
        0,
        1,
        tz,
        EventType::Male,
        None,
        Some("B"),
        None,
    );
    let parsed = parse_file(&write_file(&[c])).unwrap();
    assert!(
        (parsed[0].tz_offset_hours - tz).abs() < 1e-9,
        "{}",
        parsed[0].tz_offset_hours
    );
}

// --- EventType round-trips ---

#[test]
fn male_round_trips() {
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
        Some("A"),
        None,
    );
    let parsed = parse_file(&write_file(&[c])).unwrap();
    assert_eq!(parsed[0].event_type, EventType::Male);
}

#[test]
fn female_round_trips() {
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
        EventType::Female,
        None,
        Some("A"),
        None,
    );
    let parsed = parse_file(&write_file(&[c])).unwrap();
    assert_eq!(parsed[0].event_type, EventType::Female);
}

#[test]
fn horary_round_trips() {
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
        EventType::Horary,
        None,
        Some("A"),
        None,
    );
    let parsed = parse_file(&write_file(&[c])).unwrap();
    assert_eq!(parsed[0].event_type, EventType::Horary);
}

#[test]
fn event_round_trips() {
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
        EventType::Event,
        None,
        Some("A"),
        None,
    );
    let parsed = parse_file(&write_file(&[c])).unwrap();
    assert_eq!(parsed[0].event_type, EventType::Event);
}

#[test]
fn unspecified_round_trips() {
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
        EventType::Unspecified,
        None,
        Some("A"),
        None,
    );
    let parsed = parse_file(&write_file(&[c])).unwrap();
    assert_eq!(parsed[0].event_type, EventType::Unspecified);
}

// --- optional fields ---

#[test]
fn cyrillic_city_preserved() {
    let c = chart(
        "Test",
        55.752,
        37.616,
        2026,
        3,
        15,
        18,
        21,
        2,
        3.0,
        EventType::Unspecified,
        Some("Москва, Россия"),
        Some("AA"),
        None,
    );
    let parsed = parse_file(&write_file(&[c])).unwrap();
    assert_eq!(parsed[0].city.as_deref(), Some("Москва, Россия"));
}

#[test]
fn notes_with_double_pipe_preserved() {
    let notes = "One.||Two.||Three.";
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
        EventType::Female,
        None,
        Some("A"),
        Some(notes),
    );
    let parsed = parse_file(&write_file(&[c])).unwrap();
    assert_eq!(parsed[0].notes.as_deref(), Some(notes));
}

#[test]
fn source_rating_preserved() {
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
        Some("AA"),
        None,
    );
    let parsed = parse_file(&write_file(&[c])).unwrap();
    assert_eq!(parsed[0].source_rating.as_deref(), Some("AA"));
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
    );
    let parsed = parse_file(&write_file(&[c])).unwrap();
    assert!(parsed[0].city.is_none());
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
    );
    let parsed = parse_file(&write_file(&[c])).unwrap();
    assert!(parsed[0].notes.is_none());
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
        Some("A"),
        None,
    );
    let parsed = parse_file(&write_file(&[a, b])).unwrap();
    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed[0].name, "Alice");
    assert_eq!(parsed[1].name, "Bob");
}
