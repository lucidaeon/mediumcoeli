//! Round-trip tests: verify fields survive cross-format conversion.
//!
//! Field coverage matrix (what each format preserves):
//!
//! | Field                | `SFcht` | Zeus |
//! |----------------------|---------|------|
//! | name                 |  ✓      |  ✓   |
//! | city                 |  ✓      |  ✓   |
//! | region               |  ✓      |  ✗   |
//! | longitude            |  ✓¹     |  ✓²  |
//! | latitude             |  ✓¹     |  ✓²  |
//! | year/month/day       |  ✓      |  ✓   |
//! | hour/min/sec         |  ✓      |  ✓   |
//! | `tz_offset_hours`    |  ✓¹     |  ✓²  |
//! | `event_type`         |  ✓      |  ✓   |
//! | `source_rating`      |  ✓      |  ✓   |
//! | notes                |  ✓      |  ✓   |
//! | `house_system`       |  ✓      |  ✗   |
//! | zodiac               |  ✓      |  ✗   |
//! | `coordinate_system`  |  ✓      |  ✗   |
//! | `sub_charts`         |  ✓      |  ✗   |
//!
//! ¹ f32 precision loss: tolerance 1e-4
//! ² integer-second precision loss: tolerance 1e-9 for DMS-exact values

use astrogram::chart::{
    Chart, CoordinateSystem, EventType, HouseSystem, Latitude, Longitude, Zodiac,
};
use astrogram::sfcht;
use astrogram::zeus;

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

// Helper: verify the fields Zeus can preserve survived
fn assert_zeus_fields_match(original: &Chart, after: &Chart, label: &str) {
    assert_eq!(after.name, original.name, "{label}: name");
    assert_eq!(
        (after.year, after.month, after.day),
        (original.year, original.month, original.day),
        "{label}: date"
    );
    assert_eq!(
        (after.hour, after.minute, after.second),
        (original.hour, original.minute, original.second),
        "{label}: time"
    );
    assert_eq!(after.event_type, original.event_type, "{label}: event_type");
    assert_eq!(after.city, original.city, "{label}: city");
    assert_eq!(
        after.source_rating, original.source_rating,
        "{label}: source_rating"
    );
    assert_eq!(after.notes, original.notes, "{label}: notes");
    // Coordinates: Zeus uses integer DMS → 1 arcsecond precision = 1/3600 ≈ 2.8e-4 degrees
    assert!(
        (after.latitude.degrees() - original.latitude.degrees()).abs() < 1e-3,
        "{label}: latitude {} vs {}",
        after.latitude.degrees(),
        original.latitude.degrees()
    );
    assert!(
        (after.longitude.degrees() - original.longitude.degrees()).abs() < 1e-3,
        "{label}: longitude {} vs {}",
        after.longitude.degrees(),
        original.longitude.degrees()
    );
    // UTC offset: integer-second precision
    assert!(
        (after.tz_offset_hours - original.tz_offset_hours).abs() < 1e-3,
        "{label}: tz_offset {} vs {}",
        after.tz_offset_hours,
        original.tz_offset_hours
    );
}

// Helper: verify the fields SFcht can preserve survived
fn assert_sfcht_fields_match(original: &Chart, after: &Chart, label: &str) {
    assert_eq!(after.name, original.name, "{label}: name");
    assert_eq!(
        (after.year, after.month, after.day),
        (original.year, original.month, original.day),
        "{label}: date"
    );
    assert_eq!(
        (after.hour, after.minute, after.second),
        (original.hour, original.minute, original.second),
        "{label}: time"
    );
    assert_eq!(after.event_type, original.event_type, "{label}: event_type");
    assert_eq!(after.city, original.city, "{label}: city");
    assert_eq!(
        after.source_rating, original.source_rating,
        "{label}: source_rating"
    );
    assert_eq!(after.notes, original.notes, "{label}: notes");
    assert_eq!(
        after.house_system, original.house_system,
        "{label}: house_system"
    );
    assert_eq!(after.zodiac, original.zodiac, "{label}: zodiac");
    assert_eq!(
        after.coordinate_system, original.coordinate_system,
        "{label}: coordinate_system"
    );
    // f32 precision: ~7 decimal digits, tolerance 1e-4
    assert!(
        (after.latitude.degrees() - original.latitude.degrees()).abs() < 1e-4,
        "{label}: latitude {} vs {}",
        after.latitude.degrees(),
        original.latitude.degrees()
    );
    assert!(
        (after.longitude.degrees() - original.longitude.degrees()).abs() < 1e-4,
        "{label}: longitude {} vs {}",
        after.longitude.degrees(),
        original.longitude.degrees()
    );
    assert!(
        (after.tz_offset_hours - original.tz_offset_hours).abs() < 1e-4,
        "{label}: tz_offset {} vs {}",
        after.tz_offset_hours,
        original.tz_offset_hours
    );
}

// --- Zeus → SFcht → Zeus ---

#[test]
fn zeus_to_sfcht_to_zeus_preserves_zeus_fields() {
    // Start with a Chart (as Zeus would produce it), convert to SFcht bytes,
    // parse back, then write to Zeus text and parse again.
    let original = chart(
        "Vettius Valens",
        36.0 + 14.0 / 60.0,
        36.0 + 7.0 / 60.0,
        120,
        2,
        8,
        18,
        35,
        1,
        2.0 + 24.0 / 60.0 + 28.0 / 3600.0,
        EventType::Male,
        Some("Antioch"),
        Some("B"),
        Some("A note."),
    );

    // Chart → SFcht bytes → Chart
    let sfcht_bytes = sfcht::write_file(std::slice::from_ref(&original)).unwrap();
    let (_, via_sfcht) = sfcht::parse_file(&sfcht_bytes).unwrap();
    let mid = &via_sfcht[0];

    // Chart → Zeus text → Chart
    let zeus_text = zeus::write_file(std::slice::from_ref(mid));
    let via_zeus = zeus::parse_file(&zeus_text).unwrap();
    let final_chart = &via_zeus[0];

    assert_zeus_fields_match(&original, final_chart, "zeus→sfcht→zeus");
}

// --- SFcht → Zeus → SFcht ---

#[test]
fn sfcht_to_zeus_to_sfcht_preserves_zeus_fields() {
    // Start from a Chart (as SFcht would produce it), write to Zeus text,
    // parse back, write to SFcht bytes, parse back again.
    let original = chart(
        "Haenel, Adele",
        48.0 + 52.0 / 60.0,
        2.0 + 20.0 / 60.0,
        1989,
        2,
        11,
        16,
        20,
        0,
        1.0,
        EventType::Female,
        Some("Paris, France"),
        Some("AA"),
        None,
    );

    // Chart → Zeus text → Chart
    let zeus_text = zeus::write_file(std::slice::from_ref(&original));
    let via_zeus = zeus::parse_file(&zeus_text).unwrap();
    let mid = &via_zeus[0];

    // Chart → SFcht bytes → Chart
    let sfcht_bytes = sfcht::write_file(std::slice::from_ref(mid)).unwrap();
    let (_, via_sfcht) = sfcht::parse_file(&sfcht_bytes).unwrap();
    let final_chart = &via_sfcht[0];

    // After Zeus transit, only Zeus-preserved fields can be compared
    assert_zeus_fields_match(&original, final_chart, "sfcht→zeus→sfcht");
}

// --- multi-chart round-trip ---

#[test]
fn multi_chart_sfcht_round_trip() {
    let charts = vec![
        chart(
            "Chartreuse",
            30.04,
            31.24,
            1901,
            3,
            17,
            6,
            45,
            0,
            2.0,
            EventType::Female,
            Some("Cairo"),
            Some("AA"),
            None,
        ),
        chart(
            "Ruby",
            -34.60,
            -58.38,
            1958,
            11,
            28,
            22,
            10,
            0,
            -3.0,
            EventType::Female,
            None,
            Some("A"),
            Some("Note."),
        ),
        chart(
            "Coral",
            37.57,
            126.98,
            1983,
            5,
            9,
            14,
            30,
            0,
            9.0,
            EventType::Female,
            Some("Seoul"),
            Some("B"),
            None,
        ),
    ];

    let bytes = sfcht::write_file(&charts).unwrap();
    let (hdr, recovered) = sfcht::parse_file(&bytes).unwrap();
    assert_eq!(hdr.record_count, 3);
    assert_eq!(recovered.len(), 3);
    for (i, (orig, rec)) in charts.iter().zip(recovered.iter()).enumerate() {
        assert_sfcht_fields_match(orig, rec, &format!("chart[{i}]"));
    }
}

#[test]
fn multi_chart_zeus_round_trip() {
    let charts = vec![
        chart(
            "Valens",
            36.0 + 14.0 / 60.0,
            36.0 + 7.0 / 60.0,
            120,
            2,
            8,
            18,
            35,
            1,
            2.0 + 24.0 / 60.0 + 28.0 / 3600.0,
            EventType::Male,
            Some("Antioch"),
            Some("B"),
            None,
        ),
        chart(
            "Lilly",
            52.0 + 48.0 / 60.0 + 54.0 / 3600.0,
            -(1.0 + 20.0 / 60.0 + 1.0 / 3600.0),
            1602,
            5,
            11,
            2,
            0,
            1,
            -(5.0 / 60.0 + 19.0 / 3600.0),
            EventType::Male,
            None,
            Some("B"),
            None,
        ),
    ];

    let text = zeus::write_file(&charts);
    let recovered = zeus::parse_file(&text).unwrap();
    assert_eq!(recovered.len(), 2);
    for (i, (orig, rec)) in charts.iter().zip(recovered.iter()).enumerate() {
        assert_zeus_fields_match(orig, rec, &format!("chart[{i}]"));
    }
}

// --- event type preservation through both formats ---

#[test]
fn all_event_types_survive_sfcht_zeus_sfcht() {
    for et in [
        EventType::Male,
        EventType::Female,
        EventType::Horary,
        EventType::Event,
        EventType::Unspecified,
    ] {
        let original = chart(
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
            et,
            None,
            Some("A"),
            None,
        );

        let sfcht_bytes = sfcht::write_file(std::slice::from_ref(&original)).unwrap();
        let (_, via_sfcht) = sfcht::parse_file(&sfcht_bytes).unwrap();
        let zeus_text = zeus::write_file(std::slice::from_ref(&via_sfcht[0]));
        let via_zeus = zeus::parse_file(&zeus_text).unwrap();

        assert_eq!(
            via_zeus[0].event_type, et,
            "event_type {et:?} did not survive sfcht→zeus→parse"
        );
    }
}

// --- coordinate precision boundaries ---

#[test]
fn dms_exact_coordinates_survive_zeus_round_trip_losslessly() {
    // These coords are exactly representable in DMS integer arcseconds
    let lat = 36.0 + 14.0 / 60.0;
    let lon = 36.0 + 7.0 / 60.0;
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
        2.0 + 24.0 / 60.0 + 28.0 / 3600.0,
        EventType::Male,
        None,
        Some("B"),
        None,
    );

    let recovered = zeus::parse_file(&zeus::write_file(&[c])).unwrap();
    assert!((recovered[0].latitude.degrees() - lat).abs() < 1e-9);
    assert!((recovered[0].longitude.degrees() - lon).abs() < 1e-9);
}
