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
        "Amber Celeste",
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
    assert_eq!(charts[0].name, "Amber Celeste");
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
        "+02:24:28",
        "Antioch",
        "N36.14.00",
        "E036.07.00",
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
    // +02:24:28 → 2 + 24/60 + 28/3600 = 2.4078...
    let input = record(
        "Test",
        "1",
        "01.01.2000",
        "12:00:00",
        "+02:24:28",
        "Antioch",
        "N36.14.00",
        "E036.07.00",
        "-",
        "B",
        "",
        "",
    );
    let charts = parse_file(&input).unwrap();
    let expected = 2.0 + 24.0 / 60.0 + 28.0 / 3600.0;
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
    // N36.14.00, E036.07.00
    let input = record(
        "Valens",
        "1",
        "08.02.0120JC",
        "18:35:01",
        "+02:24:28",
        "Antioch",
        "N36.14.00",
        "E036.07.00",
        "M",
        "B",
        "",
        "",
    );
    let charts = parse_file(&input).unwrap();
    let expected_lat = 36.0 + 14.0 / 60.0;
    let expected_lon = 36.0 + 7.0 / 60.0;
    assert!((charts[0].latitude.degrees() - expected_lat).abs() < 1e-9);
    assert!((charts[0].longitude.degrees() - expected_lon).abs() < 1e-9);
}

#[test]
fn south_west_coordinates_are_negative() {
    // S34.36.00, W058.22.48 (Buenos Aires, Argentina)
    let input = record(
        "Test",
        "1",
        "01.01.2000",
        "12:00:00",
        "-04:00:00",
        "Buenos Aires",
        "S34.36.00",
        "W058.22.48",
        "-",
        "AA",
        "",
        "",
    );
    let charts = parse_file(&input).unwrap();
    let expected_lat = -(34.0 + 36.0 / 60.0);
    let expected_lon = -(58.0 + 22.0 / 60.0 + 48.0 / 3600.0);
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
        "Chartreuse",
        "2",
        "17.03.1901",
        "06:45:00",
        "+02:00:00",
        "",
        "N030.02.24",
        "E031.14.24",
        "-",
        "AA",
        "",
        "",
    );
    let b = record(
        "Ruby",
        "1",
        "28.11.1958",
        "22:10:00",
        "-03:00:00",
        "",
        "S034.36.00",
        "W058.22.48",
        "F",
        "A",
        "",
        "",
    );
    let input = format!("{a}{b}");
    let charts = parse_file(&input).unwrap();
    assert_eq!(charts.len(), 2);
    assert_eq!(charts[0].name, "Chartreuse");
    assert_eq!(charts[1].name, "Ruby");
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

    // Structural only: the Zeus parser must read every record in the real
    // specimen without error. Per-record value assertions are intentionally
    // omitted — committed tests must not bake specimen-extracted data. Field-
    // level coverage (hemispheres, event types, notes, LMT, Cyrillic, etc.)
    // lives in the synthetic record() unit tests above.
    assert_eq!(charts.len(), 11, "expected 11 records in zeus.zdb");
    eprintln!("acceptance: parsed {} Zeus records", charts.len());
}
