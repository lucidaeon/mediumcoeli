//! Tests for the Zeus `.zdb` text-format parser.

use astrogram::chart::EventType;
use astrogram::zeus::parse_file;
use std::path::PathBuf;

fn specimen_path() -> Option<PathBuf> {
    std::env::var_os("ASTRO_SPECIMENS").map(|v| PathBuf::from(v).join("zdb/zeus.zdb"))
}

// Build a minimal valid record line (16 fields, trailing semicolon).
#[allow(clippy::too_many_arguments)]
fn record(
    name: &str,
    chart_type: &str,
    date: &str,
    time: &str,
    utc: &str,
    loc: &str,
    lat: &str,
    lon: &str,
    sex: &str,
    rodden: &str,
    rect: &str,
    notes: &str,
) -> String {
    format!(
        "{name};{chart_type};{date};{time};{utc};{loc};{lat};{lon};{sex};{rodden};{rect};{notes};;12345;1;\n"
    )
}

// --- basic parsing ---

#[test]
fn empty_input_gives_empty_vec() {
    let charts = parse_file("").unwrap();
    assert!(charts.is_empty());
}

#[test]
fn blank_lines_are_skipped() {
    let input = "\n\n\n";
    let charts = parse_file(input).unwrap();
    assert!(charts.is_empty());
}

#[test]
fn parses_single_minimal_record() {
    let input = record(
        "Ada Lovelace",
        "1",
        "10.12.1815",
        "07:30:00",
        "-00:00:00",
        "London",
        "N51.30.00",
        "W000.07.00",
        "F",
        "B",
        "",
        "Notes.",
    );
    let charts = parse_file(&input).unwrap();
    assert_eq!(charts.len(), 1);
    assert_eq!(charts[0].name, "Ada Lovelace");
}

// --- dates ---

#[test]
fn gregorian_date_parses() {
    let input = record(
        "Test",
        "1",
        "01.11.1984",
        "12:00:00",
        "+00:00:00",
        "",
        "N51.30.00",
        "W000.07.00",
        "M",
        "A",
        "",
        "",
    );
    let charts = parse_file(&input).unwrap();
    assert_eq!(charts[0].year, 1984);
    assert_eq!(charts[0].month, 11);
    assert_eq!(charts[0].day, 1);
}

#[test]
fn julian_calendar_suffix_strips_cleanly() {
    // Vettius Valens: 08.02.0120JC
    let input = record(
        "Valens",
        "1",
        "08.02.0120JC",
        "18:35:01",
        "+02:24:14",
        "Antioch",
        "N36.12.24",
        "E036.09.26",
        "M",
        "B",
        "",
        "",
    );
    let charts = parse_file(&input).unwrap();
    assert_eq!(charts[0].year, 120);
    assert_eq!(charts[0].month, 2);
    assert_eq!(charts[0].day, 8);
}

// --- time ---

#[test]
fn time_parses_to_hms() {
    let input = record(
        "Test",
        "1",
        "01.01.2000",
        "18:35:01",
        "+00:00:00",
        "",
        "N51.30.00",
        "W000.07.00",
        "-",
        "AA",
        "",
        "",
    );
    let charts = parse_file(&input).unwrap();
    assert_eq!(charts[0].hour, 18);
    assert_eq!(charts[0].minute, 35);
    assert_eq!(charts[0].second, 1);
}

// --- UTC offset ---

#[test]
fn positive_utc_offset_is_east_positive() {
    // +02:24:14 → 2 + 24/60 + 14/3600 = 2.4038...
    let input = record(
        "Test",
        "1",
        "01.01.2000",
        "12:00:00",
        "+02:24:14",
        "Antioch",
        "N36.12.24",
        "E036.09.26",
        "-",
        "B",
        "",
        "",
    );
    let charts = parse_file(&input).unwrap();
    let expected = 2.0 + 24.0 / 60.0 + 14.0 / 3600.0;
    assert!(
        (charts[0].tz_offset_hours - expected).abs() < 1e-9,
        "{}",
        charts[0].tz_offset_hours
    );
}

#[test]
fn negative_utc_offset_is_west_negative() {
    // -07:00:00 → -7.0
    let input = record(
        "Test",
        "1",
        "01.01.2000",
        "12:00:00",
        "-07:00:00",
        "",
        "N39.43.46",
        "W104.49.55",
        "M",
        "A",
        "",
        "",
    );
    let charts = parse_file(&input).unwrap();
    assert!((charts[0].tz_offset_hours - -7.0).abs() < 1e-9);
}

#[test]
fn near_zero_utc_offset() {
    // -00:05:19 → -(0 + 5/60 + 19/3600) = -0.08861...
    let input = record(
        "Lilly",
        "1",
        "11.05.1602",
        "02:00:01",
        "-00:05:19",
        "",
        "N52.48.54",
        "W001.20.01",
        "M",
        "B",
        "",
        "",
    );
    let charts = parse_file(&input).unwrap();
    let expected = -(5.0 / 60.0 + 19.0 / 3600.0);
    assert!(
        (charts[0].tz_offset_hours - expected).abs() < 1e-9,
        "{}",
        charts[0].tz_offset_hours
    );
}

// --- coordinates ---

#[test]
fn north_east_coordinates_are_positive() {
    // N36.12.24, E036.09.26
    let input = record(
        "Valens",
        "1",
        "08.02.0120JC",
        "18:35:01",
        "+02:24:14",
        "Antioch",
        "N36.12.24",
        "E036.09.26",
        "M",
        "B",
        "",
        "",
    );
    let charts = parse_file(&input).unwrap();
    let expected_lat = 36.0 + 12.0 / 60.0 + 24.0 / 3600.0;
    let expected_lon = 36.0 + 9.0 / 60.0 + 26.0 / 3600.0;
    assert!((charts[0].latitude.degrees() - expected_lat).abs() < 1e-9);
    assert!((charts[0].longitude.degrees() - expected_lon).abs() < 1e-9);
}

#[test]
fn south_west_coordinates_are_negative() {
    // S33.52.00, W070.40.00 (Santiago, Chile)
    let input = record(
        "Test",
        "1",
        "01.01.2000",
        "12:00:00",
        "-04:00:00",
        "Santiago",
        "S33.52.00",
        "W070.40.00",
        "-",
        "AA",
        "",
        "",
    );
    let charts = parse_file(&input).unwrap();
    let expected_lat = -(33.0 + 52.0 / 60.0);
    let expected_lon = -(70.0 + 40.0 / 60.0);
    assert!((charts[0].latitude.degrees() - expected_lat).abs() < 1e-9);
    assert!((charts[0].longitude.degrees() - expected_lon).abs() < 1e-9);
}

// --- event type ---

#[test]
fn sex_m_natal_gives_male() {
    let input = record(
        "Test",
        "1",
        "01.01.2000",
        "12:00:00",
        "+00:00:00",
        "",
        "N51.30.00",
        "W000.07.00",
        "M",
        "A",
        "",
        "",
    );
    let charts = parse_file(&input).unwrap();
    assert_eq!(charts[0].event_type, EventType::Male);
}

#[test]
fn sex_f_natal_gives_female() {
    let input = record(
        "Test",
        "1",
        "01.01.2000",
        "12:00:00",
        "+00:00:00",
        "",
        "N51.30.00",
        "W000.07.00",
        "F",
        "A",
        "",
        "",
    );
    let charts = parse_file(&input).unwrap();
    assert_eq!(charts[0].event_type, EventType::Female);
}

#[test]
fn sex_dash_gives_unspecified() {
    let input = record(
        "Event",
        "0",
        "15.03.2026",
        "18:21:02",
        "+03:00:00",
        "Moscow",
        "N55.45.08",
        "E037.36.56",
        "-",
        "AA",
        "",
        "",
    );
    let charts = parse_file(&input).unwrap();
    assert_eq!(charts[0].event_type, EventType::Unspecified);
}

#[test]
fn chart_type_2_gives_horary() {
    let input = record(
        "Horary",
        "2",
        "15.03.2026",
        "18:22:08",
        "+03:00:00",
        "Moscow",
        "N55.45.08",
        "E037.36.56",
        "-",
        "AA",
        "",
        "",
    );
    let charts = parse_file(&input).unwrap();
    assert_eq!(charts[0].event_type, EventType::Horary);
}

#[test]
fn chart_types_3_4_5_give_event() {
    for ct in ["3", "4", "5"] {
        let input = record(
            "Test",
            ct,
            "15.03.2026",
            "18:00:00",
            "+03:00:00",
            "Moscow",
            "N55.45.08",
            "E037.36.56",
            "-",
            "AA",
            "",
            "",
        );
        let charts = parse_file(&input).unwrap();
        assert_eq!(charts[0].event_type, EventType::Event, "chart_type={ct}");
    }
}

// --- optional fields ---

#[test]
fn empty_location_is_none() {
    // William Lilly has an empty location field
    let input = record(
        "Lilly",
        "1",
        "11.05.1602",
        "02:00:01",
        "-00:05:19",
        "",
        "N52.48.54",
        "W001.20.01",
        "M",
        "B",
        "",
        "Notes.",
    );
    let charts = parse_file(&input).unwrap();
    assert!(charts[0].city.is_none());
}

#[test]
fn non_empty_location_is_some() {
    let input = record(
        "Test",
        "1",
        "01.01.2000",
        "12:00:00",
        "+00:00:00",
        "London, England",
        "N51.30.00",
        "W000.07.00",
        "M",
        "A",
        "",
        "",
    );
    let charts = parse_file(&input).unwrap();
    assert_eq!(charts[0].city.as_deref(), Some("London, England"));
}

#[test]
fn empty_notes_is_none() {
    let input = record(
        "Test",
        "1",
        "01.01.2000",
        "12:00:00",
        "+00:00:00",
        "",
        "N51.30.00",
        "W000.07.00",
        "M",
        "A",
        "",
        "",
    );
    let charts = parse_file(&input).unwrap();
    assert!(charts[0].notes.is_none());
}

#[test]
fn notes_with_double_pipe_preserved() {
    // Notes with || paragraph separators should pass through as-is
    let raw_notes = "Paragraph one.||Paragraph two.||Paragraph three.";
    let input = record(
        "Test",
        "1",
        "01.01.2000",
        "12:00:00",
        "+00:00:00",
        "",
        "N51.30.00",
        "W000.07.00",
        "F",
        "A",
        "",
        raw_notes,
    );
    let charts = parse_file(&input).unwrap();
    assert_eq!(charts[0].notes.as_deref(), Some(raw_notes));
}

#[test]
fn rodden_rating_stored_in_source_rating() {
    let input = record(
        "Test",
        "1",
        "01.01.2000",
        "12:00:00",
        "+00:00:00",
        "",
        "N51.30.00",
        "W000.07.00",
        "M",
        "AA",
        "",
        "",
    );
    let charts = parse_file(&input).unwrap();
    assert_eq!(charts[0].source_rating.as_deref(), Some("AA"));
}

// --- multi-record and Cyrillic ---

#[test]
fn parses_multiple_records() {
    let a = record(
        "Alice",
        "2",
        "01.01.2000",
        "12:00:00",
        "+00:00:00",
        "",
        "N51.30.00",
        "W000.07.00",
        "-",
        "AA",
        "",
        "",
    );
    let b = record(
        "Bob",
        "1",
        "02.02.1985",
        "08:30:00",
        "-05:00:00",
        "",
        "N40.42.46",
        "W074.00.21",
        "M",
        "A",
        "",
        "",
    );
    let input = format!("{a}{b}");
    let charts = parse_file(&input).unwrap();
    assert_eq!(charts.len(), 2);
    assert_eq!(charts[0].name, "Alice");
    assert_eq!(charts[1].name, "Bob");
}

// --- acceptance: full specimen ---

#[test]
fn acceptance_parses_full_zeus_specimen() {
    let Some(path) = specimen_path() else {
        eprintln!("ASTRO_SPECIMENS not set — skipping integration test");
        return;
    };
    if !path.exists() {
        eprintln!("specimen absent ({}); skipping", path.display());
        return;
    }

    let text = std::fs::read_to_string(&path).expect("read specimen");
    let charts = parse_file(&text).unwrap();

    assert_eq!(charts.len(), 11, "expected 11 records in zeus.zdb");

    // Vettius Valens (record 0)
    let valens = &charts[0];
    assert_eq!(valens.name, "Vettius Valens");
    assert_eq!(valens.year, 120);
    assert_eq!(valens.month, 2);
    assert_eq!(valens.day, 8);
    assert_eq!(valens.hour, 18);
    assert_eq!(valens.minute, 35);
    assert_eq!(valens.second, 1);
    assert!((valens.latitude.degrees() - (36.0 + 12.0 / 60.0 + 24.0 / 3600.0)).abs() < 1e-9);
    assert!((valens.longitude.degrees() - (36.0 + 9.0 / 60.0 + 26.0 / 3600.0)).abs() < 1e-9);
    assert!((valens.tz_offset_hours - (2.0 + 24.0 / 60.0 + 14.0 / 3600.0)).abs() < 1e-9);
    assert_eq!(valens.event_type, EventType::Male);
    assert_eq!(valens.source_rating.as_deref(), Some("B"));
    assert_eq!(valens.city.as_deref(), Some("Antioch, Turkey"));

    // Record 1
    let r1 = &charts[1];
    assert_eq!(r1.year, 1984);
    assert!((r1.latitude.degrees() - (39.0 + 43.0 / 60.0 + 46.0 / 3600.0)).abs() < 1e-9);
    assert!((r1.longitude.degrees() - -(104.0 + 49.0 / 60.0 + 55.0 / 3600.0)).abs() < 1e-9);
    assert!((r1.tz_offset_hours - -7.0).abs() < 1e-9);
    assert_eq!(r1.event_type, EventType::Male);

    // Record 2: Female with double-pipe notes
    let r2 = &charts[2];
    assert_eq!(r2.event_type, EventType::Female);
    assert!(
        r2.notes.as_ref().is_some_and(|n| n.contains("||")),
        "record 2 notes should preserve || separators"
    );

    // William Lilly (record 3)
    let lilly = &charts[3];
    assert!(lilly.city.is_none(), "Lilly has empty location field");
    let expected_tz = -(5.0 / 60.0 + 19.0 / 3600.0);
    assert!((lilly.tz_offset_hours - expected_tz).abs() < 1e-9);

    // Cyrillic location (records 4-10)
    for chart in &charts[4..11] {
        assert_eq!(
            chart.city.as_deref(),
            Some("Москва, Россия"),
            "record should have Cyrillic city"
        );
    }

    // Chart type variety
    assert_eq!(charts[4].event_type, EventType::Unspecified); // Event, sex=-
    assert_eq!(charts[5].event_type, EventType::Unspecified); // Event w Rect
    assert_eq!(charts[6].event_type, EventType::Horary); // chart_type=2
    assert_eq!(charts[7].event_type, EventType::Event); // chart_type=3
    assert_eq!(charts[8].event_type, EventType::Event); // chart_type=4
    assert_eq!(charts[9].event_type, EventType::Event); // chart_type=5

    eprintln!("acceptance: 10 Zeus records verified");
}
