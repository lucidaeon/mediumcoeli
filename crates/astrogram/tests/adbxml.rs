//! Tests for the ADB XML parser and writer (`export_format` 160715).

use astrogram::adbxml::{parse_file, write_file};
use astrogram::chart::{
    Chart, CoordinateSystem, EventType, HouseSystem, Latitude, Longitude, Zodiac,
};
use std::path::PathBuf;

fn specimen_path() -> Option<PathBuf> {
    let val = std::env::var_os("ASTRO_SPECIMENS")?;
    let dir = PathBuf::from(val).join("adb");
    let found = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .filter_map(std::result::Result::ok)
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("xml")));
    Some(found.unwrap_or_else(|| dir.join("specimen.xml")))
}

// ── helpers ──────────────────────────────────────────────────────────────────

fn within(a: f64, b: f64, eps: f64) -> bool {
    (a - b).abs() <= eps
}

// Build the minimal wrapping element around an <adb_entry> snippet.
fn wrap(entry_xml: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<astrodatabank_export export_format="160715">
{entry_xml}
</astrodatabank_export>"#
    )
}

// Minimal valid entry for a male, AA-rated, standard-time chart.
#[allow(clippy::too_many_arguments)]
fn minimal_entry(
    adb_id: u32,
    name: &str,
    csex: &str,
    rrc: u32,
    iyear: i32,
    imonth: u32,
    iday: u32,
    time_val: &str,
    jd_ut: &str,
    ctimetype: &str,
    sznabbr: &str,
    slati: &str,
    slong: &str,
    city: &str,
    country: &str,
) -> String {
    format!(
        r#"  <adb_entry adb_id="{adb_id}">
    <public_data>
      <name>{name}</name>
      <gender csex="{csex}">{}</gender>
      <roddenrating rrc="{rrc}">{}</roddenrating>
      <datatype sdatatype="Public Figure" dtc="1" />
      <bdata>
        <sbdate ccalendar="g" iyear="{iyear}" imonth="{imonth}" iday="{iday}">{iyear}/{imonth:02}/{iday:02}</sbdate>
        <sbtime ctimetype="{ctimetype}" jd_ut="{jd_ut}" sznabbr="{sznabbr}">{time_val}</sbtime>
        <place slati="{slati}" slong="{slong}">{city}</place>
        <country>{country}</country>
      </bdata>
    </public_data>
  </adb_entry>"#,
        csex.to_uppercase(),
        rrc_to_str(rrc),
    )
}

fn rrc_to_str(rrc: u32) -> &'static str {
    match rrc {
        1 => "AA",
        2 => "A",
        3 => "B",
        4 => "C",
        5 => "DD",
        6 => "X",
        7 => "XX",
        _ => "?",
    }
}

// ── parse_file contract ───────────────────────────────────────────────────────

#[test]
fn empty_document_gives_empty_vec() {
    let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<astrodatabank_export export_format="160715">
</astrodatabank_export>"#;
    let charts = parse_file(xml).unwrap();
    assert!(charts.is_empty());
}

#[test]
fn bad_xml_returns_error() {
    let result = parse_file("<unclosed");
    assert!(result.is_err());
}

// ── date / time fields ────────────────────────────────────────────────────────

#[test]
fn parses_date_fields() {
    // jd_ut for 1970-01-01 12:00 UTC = 2440588.0  → offset = 0.0
    let xml = wrap(&minimal_entry(
        1,
        "Epoch Zero",
        "m",
        2,
        1970,
        1,
        1,
        "12:00",
        "2440588.0",
        "s",
        "GMT",
        "00n00",
        "0e00",
        "Null Island",
        "Ocean",
    ));
    let charts = parse_file(&xml).unwrap();
    let c = &charts[0];
    assert_eq!(c.year, 1970);
    assert_eq!(c.month, 1);
    assert_eq!(c.day, 1);
}

#[test]
fn parses_time_fields() {
    let xml = wrap(&minimal_entry(
        2,
        "Time Test",
        "f",
        3,
        2000,
        6,
        15,
        "14:30",
        "2451711.104167",
        "s",
        "UTC",
        "51n30",
        "0e00",
        "London",
        "England",
    ));
    let charts = parse_file(&xml).unwrap();
    let c = &charts[0];
    assert_eq!(c.hour, 14);
    assert_eq!(c.minute, 30);
    assert_eq!(c.second, 0);
}

// ── timezone offset from jd_ut ────────────────────────────────────────────────

#[test]
fn tz_offset_zero_at_gmt() {
    // 1970-01-01 12:00 UTC exactly → local 12:00 → offset 0
    let xml = wrap(&minimal_entry(
        3,
        "GMT Test",
        "m",
        1,
        1970,
        1,
        1,
        "12:00",
        "2440588.0",
        "s",
        "GMT",
        "51n30",
        "0e00",
        "Greenwich",
        "England",
    ));
    let charts = parse_file(&xml).unwrap();
    assert!(within(charts[0].tz_offset_hours, 0.0, 1e-3));
}

#[test]
fn tz_offset_plus_one_east() {
    // Local noon, UTC 11:00 → offset = +1h
    // JD for 1970-01-01 11:00 UTC: (11-12)/24 + 2440588 = 2440587.958333
    let xml = wrap(&minimal_entry(
        4,
        "CET Test",
        "m",
        1,
        1970,
        1,
        1,
        "12:00",
        "2440587.958333",
        "s",
        "CET",
        "48n52",
        "2e20",
        "Paris",
        "France",
    ));
    let charts = parse_file(&xml).unwrap();
    assert!(within(charts[0].tz_offset_hours, 1.0, 1e-3));
}

#[test]
fn tz_offset_minus_five_west() {
    // Local noon, UTC 17:00 → offset = -5h
    // JD for 1970-01-01 17:00 UTC: (17-12)/24 + 2440588 = 2440588.208333
    let xml = wrap(&minimal_entry(
        5,
        "EST Test",
        "m",
        1,
        1970,
        1,
        1,
        "12:00",
        "2440588.208333",
        "s",
        "EST",
        "40n43",
        "74w00",
        "New York",
        "USA",
    ));
    let charts = parse_file(&xml).unwrap();
    assert!(within(charts[0].tz_offset_hours, -5.0, 1e-3));
}

// ── coordinate parsing ────────────────────────────────────────────────────────

#[test]
fn parses_north_lat_degrees_and_minutes() {
    let xml = wrap(&minimal_entry(
        10,
        "Lat Test",
        "m",
        1,
        2000,
        1,
        1,
        "12:00",
        "2451545.0",
        "s",
        "UTC",
        "45n42",
        "0e00",
        "City",
        "Country",
    ));
    let charts = parse_file(&xml).unwrap();
    // 45°42' N = 45 + 42/60 = 45.7°
    assert!(within(charts[0].latitude.degrees(), 45.7, 1e-4));
}

#[test]
fn parses_north_lat_with_seconds() {
    // "52n0445" = 52°04'45" N = 52 + 4/60 + 45/3600
    let xml = wrap(&minimal_entry(
        11,
        "Lat Sec Test",
        "m",
        1,
        2000,
        1,
        1,
        "12:00",
        "2451545.0",
        "s",
        "UTC",
        "52n0445",
        "0e00",
        "City",
        "Country",
    ));
    let charts = parse_file(&xml).unwrap();
    let expected = 52.0 + 4.0 / 60.0 + 45.0 / 3600.0;
    assert!(within(charts[0].latitude.degrees(), expected, 1e-4));
}

#[test]
fn parses_south_lat() {
    // Jakarta -6.21°, 106.85° (skills/astrologer/fixtures/ref_synthetics.md) → DMS 6°12'36"S, 106°51'E.
    // ADB "6s1236" = 6°12'36" S = -(6 + 12/60 + 36/3600).
    let xml = wrap(&minimal_entry(
        12,
        "Coral",
        "m",
        1,
        2000,
        1,
        1,
        "12:00",
        "2451545.0",
        "s",
        "UTC",
        "6s1236",
        "106e51",
        "Jakarta",
        "Indonesia",
    ));
    let charts = parse_file(&xml).unwrap();
    let expected = -(6.0 + 12.0 / 60.0 + 36.0 / 3600.0);
    assert!(within(charts[0].latitude.degrees(), expected, 1e-4));
}

#[test]
fn parses_east_lon_degrees_and_minutes() {
    let xml = wrap(&minimal_entry(
        13,
        "Lon Test",
        "m",
        1,
        2000,
        1,
        1,
        "12:00",
        "2451545.0",
        "s",
        "UTC",
        "48n52",
        "2e20",
        "Paris",
        "France",
    ));
    let charts = parse_file(&xml).unwrap();
    // 2°20' E = 2 + 20/60 = 2.3333°
    assert!(within(charts[0].longitude.degrees(), 2.3333, 1e-3));
}

#[test]
fn parses_west_lon_with_seconds() {
    // "122w1959" = 122°19'59" W = -(122 + 19/60 + 59/3600)
    let xml = wrap(&minimal_entry(
        14,
        "LA",
        "m",
        1,
        2000,
        1,
        1,
        "12:00",
        "2451545.0",
        "s",
        "UTC",
        "34n03",
        "122w1959",
        "Los Angeles",
        "USA",
    ));
    let charts = parse_file(&xml).unwrap();
    let expected = -(122.0 + 19.0 / 60.0 + 59.0 / 3600.0);
    assert!(within(charts[0].longitude.degrees(), expected, 1e-3));
}

// ── gender / event_type ───────────────────────────────────────────────────────

#[test]
fn csex_m_gives_male() {
    let xml = wrap(&minimal_entry(
        20,
        "Male",
        "m",
        1,
        2000,
        1,
        1,
        "12:00",
        "2451545.0",
        "s",
        "UTC",
        "51n30",
        "0e00",
        "X",
        "Y",
    ));
    assert_eq!(parse_file(&xml).unwrap()[0].event_type, EventType::Male);
}

#[test]
fn csex_f_gives_female() {
    let xml = wrap(&minimal_entry(
        21,
        "Female",
        "f",
        1,
        2000,
        1,
        1,
        "12:00",
        "2451545.0",
        "s",
        "UTC",
        "51n30",
        "0e00",
        "X",
        "Y",
    ));
    assert_eq!(parse_file(&xml).unwrap()[0].event_type, EventType::Female);
}

// ── rodden rating ─────────────────────────────────────────────────────────────

#[test]
fn rrc_maps_to_rating_strings() {
    for (rrc, expected) in [
        (1, "AA"),
        (2, "A"),
        (3, "B"),
        (4, "C"),
        (5, "DD"),
        (6, "X"),
        (7, "XX"),
    ] {
        let xml = wrap(&minimal_entry(
            rrc,
            "Rating Test",
            "m",
            rrc,
            2000,
            1,
            1,
            "12:00",
            "2451545.0",
            "s",
            "UTC",
            "51n30",
            "0e00",
            "X",
            "Y",
        ));
        let charts = parse_file(&xml).unwrap();
        assert_eq!(
            charts[0].source_rating.as_deref(),
            Some(expected),
            "rrc={rrc}"
        );
    }
}

// ── LMT / ctimetype ──────────────────────────────────────────────────────────

#[test]
fn ctimetype_l_sets_is_lmt() {
    // Synthetic record (Lima -12.05°, -77.04° from skills/astrologer/fixtures/ref_synthetics.md →
    // DMS 12°03'S, 77°02'24"W). ctimetype "l" must set is_lmt.
    let xml = wrap(&minimal_entry(
        30,
        "Sienna",
        "m",
        1,
        2000,
        1,
        1,
        "12:00",
        "2451545.0",
        "l",
        "LMT",
        "12s03",
        "77w0224",
        "Lima",
        "Peru",
    ));
    let charts = parse_file(&xml).unwrap();
    assert!(charts[0].is_lmt);
}

#[test]
fn ctimetype_s_clears_is_lmt() {
    let xml = wrap(&minimal_entry(
        31,
        "Standard",
        "m",
        1,
        2000,
        1,
        1,
        "12:00",
        "2451545.0",
        "s",
        "UTC",
        "51n30",
        "0e00",
        "London",
        "England",
    ));
    assert!(!parse_file(&xml).unwrap()[0].is_lmt);
}

// ── place / country ───────────────────────────────────────────────────────────

#[test]
fn city_and_region_populated() {
    let xml = wrap(&minimal_entry(
        40,
        "Geo Test",
        "m",
        1,
        2000,
        1,
        1,
        "12:00",
        "2451545.0",
        "s",
        "UTC",
        "48n52",
        "2e20",
        "Paris",
        "France",
    ));
    let charts = parse_file(&xml).unwrap();
    assert_eq!(charts[0].city.as_deref(), Some("Paris"));
    assert_eq!(charts[0].region.as_deref(), Some("France"));
}

// ── antimeridian wraparound ───────────────────────────────────────────────────

#[test]
fn lon_exceeding_180w_wraps_to_east() {
    // "182w3040" = 182°30'40" W → normalize: -182.511° + 360° = 177.489°E
    let xml = wrap(
        r#"  <adb_entry adb_id="79883">
    <public_data>
      <name>Kiska</name>
      <gender csex="e">N/A</gender>
      <roddenrating rrc="1">AA</roddenrating>
      <datatype sdatatype="Mundane" dtc="5" />
      <bdata>
        <sbdate ccalendar="g" iyear="2014" imonth="6" iday="23">2014/06/23</sbdate>
        <sbtime ctimetype="s" jd_ut="2456832.370243" sznabbr="UTC">20:53:09</sbtime>
        <place slati="51n5731" slong="182w3040">Kiska</place>
        <country>Alaska</country>
      </bdata>
    </public_data>
  </adb_entry>"#,
    );
    let charts = parse_file(&xml).unwrap();
    let expected = -(182.0 + 30.0 / 60.0 + 40.0 / 3600.0) + 360.0;
    assert!(within(charts[0].longitude.degrees(), expected, 1e-3));
}

// ── defaults ─────────────────────────────────────────────────────────────────

#[test]
fn defaults_to_placidus_tropical_geocentric() {
    let xml = wrap(&minimal_entry(
        50,
        "Defaults",
        "m",
        1,
        2000,
        1,
        1,
        "12:00",
        "2451545.0",
        "s",
        "UTC",
        "51n30",
        "0e00",
        "X",
        "Y",
    ));
    let c = &parse_file(&xml).unwrap()[0];
    assert_eq!(c.house_system, HouseSystem::Placidus);
    assert_eq!(c.zodiac, Zodiac::Tropical);
}

// ── HH:MM:SS times ───────────────────────────────────────────────────────────

#[test]
fn parses_time_with_seconds() {
    let xml = wrap(
        r#"  <adb_entry adb_id="1798">
    <public_data>
      <name>Seconds Test</name>
      <gender csex="m">M</gender>
      <roddenrating rrc="1">AA</roddenrating>
      <datatype sdatatype="Public Figure" dtc="1" />
      <bdata>
        <sbdate ccalendar="g" iyear="2000" imonth="6" iday="15">2000/06/15</sbdate>
        <sbtime ctimetype="s" jd_ut="2451711.0" sznabbr="UTC">05:09:25</sbtime>
        <place slati="51n30" slong="0e00">London</place>
        <country>England</country>
      </bdata>
    </public_data>
  </adb_entry>"#,
    );
    let charts = parse_file(&xml).unwrap();
    let c = &charts[0];
    assert_eq!(c.hour, 5);
    assert_eq!(c.minute, 9);
    assert_eq!(c.second, 25);
}

// ── time_unknown entries ──────────────────────────────────────────────────────

#[test]
fn time_unknown_yes_uses_noon_placeholder() {
    let xml = wrap(
        r#"  <adb_entry adb_id="15">
    <public_data>
      <name>Valens, Vettius</name>
      <gender csex="m">M</gender>
      <roddenrating rrc="6">X</roddenrating>
      <datatype sdatatype="Public Figure" dtc="1" />
      <bdata>
        <sbdate ccalendar="j" iyear="120" imonth="2" iday="8">0120/02/08</sbdate>
        <sbtime sbtime_ampm="" ctimetype="l" stimetype="local mean time" stmerid="m36e07" ctzauto="a" jd_ut="1764928.174" sznabbr="LMT" time_unknown="yes">unknown, 12:00 used</sbtime>
        <place slati="36n14" slong="36e07">Antioch</place>
        <country sctr="TUR">Türkiye</country>
      </bdata>
    </public_data>
  </adb_entry>"#,
    );
    let charts = parse_file(&xml).unwrap();
    assert_eq!(charts[0].hour, 12);
    assert_eq!(charts[0].minute, 0);
}

// ── multiple entries ──────────────────────────────────────────────────────────

#[test]
fn parses_multiple_entries() {
    let entry1 = minimal_entry(
        60,
        "Chartreuse",
        "f",
        1,
        1901,
        3,
        17,
        "06:45",
        "2415460.78125",
        "s",
        "EET",
        "30n02",
        "31e14",
        "Cairo",
        "Egypt",
    );
    let entry2 = minimal_entry(
        61,
        "Ruby",
        "f",
        2,
        1958,
        11,
        28,
        "22:10",
        "2436537.423611",
        "s",
        "ART",
        "34s36",
        "58w23",
        "Buenos Aires",
        "Argentina",
    );
    let xml = wrap(&format!("{entry1}\n{entry2}"));
    let charts = parse_file(&xml).unwrap();
    assert_eq!(charts.len(), 2);
    assert_eq!(charts[0].name, "Chartreuse");
    assert_eq!(charts[1].name, "Ruby");
}

// ── acceptance: real specimen ─────────────────────────────────────────────────
//
// Per-entry value assertions (which hardcoded specific ADB-export entries) are
// intentionally omitted: committed tests must not bake specimen-extracted
// data. Field-level coverage lives in the synthetic
// minimal_entry unit tests above; the structural specimen checks below
// (round-trip + parse-all) exercise the real corpus without hardcoding any of
// its values.

// ── writer ────────────────────────────────────────────────────────────────────

fn chart_for_write(name: &str) -> Chart {
    Chart {
        name: name.to_string(),
        secondary_name: None,
        city: Some("London".to_string()),
        region: Some("England".to_string()),
        longitude: Longitude::new(-0.117).unwrap(),
        latitude: Latitude::new(51.5).unwrap(),
        year: 2000,
        month: 6,
        day: 15,
        hour: 14,
        minute: 30,
        second: 0,
        tz_offset_hours: 1.0,
        tz_abbreviation: Some("BST".to_string()),
        is_lmt: false,
        event_type: EventType::Male,
        source_rating: Some("AA".to_string()),
        house_system: HouseSystem::Placidus,
        zodiac: Zodiac::Tropical,
        coordinate_system: CoordinateSystem::Geocentric,
        sub_charts: vec![],
        notes: Some("Test notes.".to_string()),
    }
}

#[test]
fn write_empty_slice_produces_parseable_xml() {
    let xml = write_file(&[]);
    let charts = parse_file(&xml).unwrap();
    assert!(charts.is_empty());
}

#[test]
fn write_produces_valid_xml_declaration() {
    let xml = write_file(&[]);
    assert!(xml.starts_with("<?xml version=\"1.0\" encoding=\"utf-8\"?>"));
}

#[test]
fn write_round_trips_name() {
    let original = chart_for_write("Haenel, Adele");
    let xml = write_file(&[original]);
    let charts = parse_file(&xml).unwrap();
    assert_eq!(charts[0].name, "Haenel, Adele");
}

#[test]
fn write_round_trips_date() {
    let original = chart_for_write("Date Test");
    let xml = write_file(&[original]);
    let charts = parse_file(&xml).unwrap();
    assert_eq!(charts[0].year, 2000);
    assert_eq!(charts[0].month, 6);
    assert_eq!(charts[0].day, 15);
}

#[test]
fn write_round_trips_time_and_seconds() {
    let mut c = chart_for_write("Time Test");
    c.hour = 9;
    c.minute = 5;
    c.second = 37;
    let xml = write_file(&[c]);
    let charts = parse_file(&xml).unwrap();
    assert_eq!(charts[0].hour, 9);
    assert_eq!(charts[0].minute, 5);
    assert_eq!(charts[0].second, 37);
}

#[test]
fn write_round_trips_tz_offset() {
    let original = chart_for_write("TZ Test");
    let xml = write_file(&[original]);
    let charts = parse_file(&xml).unwrap();
    assert!(within(charts[0].tz_offset_hours, 1.0, 1e-4));
}

#[test]
fn write_round_trips_coordinates() {
    let original = chart_for_write("Coord Test");
    let xml = write_file(&[original]);
    let charts = parse_file(&xml).unwrap();
    // 51.5° N, precision to nearest second (~0.0003°)
    assert!(within(charts[0].latitude.degrees(), 51.5, 5e-4));
    // -0.117° W
    assert!(within(charts[0].longitude.degrees(), -0.117, 5e-4));
}

#[test]
fn write_round_trips_female_event_type() {
    let mut c = chart_for_write("Female Test");
    c.event_type = EventType::Female;
    let xml = write_file(&[c]);
    let charts = parse_file(&xml).unwrap();
    assert_eq!(charts[0].event_type, EventType::Female);
}

#[test]
fn write_round_trips_source_rating() {
    for rating in ["AA", "A", "B", "C", "DD", "X", "XX"] {
        let mut c = chart_for_write("Rating Test");
        c.source_rating = Some(rating.to_string());
        let xml = write_file(&[c]);
        let charts = parse_file(&xml).unwrap();
        assert_eq!(
            charts[0].source_rating.as_deref(),
            Some(rating),
            "rating {rating}"
        );
    }
}

#[test]
fn write_round_trips_is_lmt() {
    let mut c = chart_for_write("LMT Test");
    c.is_lmt = true;
    c.tz_abbreviation = Some("LMT".to_string());
    let xml = write_file(&[c]);
    let charts = parse_file(&xml).unwrap();
    assert!(charts[0].is_lmt);
}

#[test]
fn write_round_trips_city_and_region() {
    let original = chart_for_write("Place Test");
    let xml = write_file(&[original]);
    let charts = parse_file(&xml).unwrap();
    assert_eq!(charts[0].city.as_deref(), Some("London"));
    assert_eq!(charts[0].region.as_deref(), Some("England"));
}

#[test]
fn write_round_trips_notes() {
    let original = chart_for_write("Notes Test");
    let xml = write_file(&[original]);
    let charts = parse_file(&xml).unwrap();
    assert_eq!(charts[0].notes.as_deref(), Some("Test notes."));
}

#[test]
fn write_escapes_xml_special_chars_in_name() {
    let mut c = chart_for_write("A & B <Test> \"quoted\"");
    c.notes = None;
    let xml = write_file(&[c]);
    // Must be valid XML (parse succeeds)
    let charts = parse_file(&xml).unwrap();
    assert_eq!(charts[0].name, "A & B <Test> \"quoted\"");
}

#[test]
fn write_multiple_charts_preserves_order() {
    let a = chart_for_write("Alpha");
    let b = chart_for_write("Beta");
    let c = chart_for_write("Gamma");
    let xml = write_file(&[a, b, c]);
    let charts = parse_file(&xml).unwrap();
    assert_eq!(charts.len(), 3);
    assert_eq!(charts[0].name, "Alpha");
    assert_eq!(charts[1].name, "Beta");
    assert_eq!(charts[2].name, "Gamma");
}

#[test]
fn write_assigns_local_adb_ids_above_threshold() {
    // Local-origin IDs must be > 100_000_000 per ADB spec
    let xml = write_file(&[chart_for_write("ID Test")]);
    assert!(xml.contains("adb_id=\"1000000"));
}

// ── acceptance: writer ────────────────────────────────────────────────────────

#[test]
fn acceptance_write_all_then_parse_back() {
    let Some(path) = specimen_path() else {
        eprintln!("ASTRO_SPECIMENS not set — skipping integration test");
        return;
    };
    if !path.exists() {
        eprintln!("ADB specimen absent ({}); skipping", path.display());
        return;
    }

    let xml = std::fs::read_to_string(&path).expect("read specimen");
    let original = parse_file(&xml).expect("parse specimen");

    let written = write_file(&original);
    let roundtripped = parse_file(&written).expect("parse written XML");

    assert_eq!(roundtripped.len(), original.len());
    // Spot-check entry 0 round-trips against itself (compares to parsed input,
    // not to any hardcoded specimen value).
    assert_eq!(roundtripped[0].name, original[0].name);
    assert_eq!(roundtripped[0].year, original[0].year);
    assert_eq!(roundtripped[0].month, original[0].month);
    assert_eq!(roundtripped[0].day, original[0].day);
    assert!(within(
        roundtripped[0].latitude.degrees(),
        original[0].latitude.degrees(),
        5e-4
    ));
    assert!(within(
        roundtripped[0].longitude.degrees(),
        original[0].longitude.degrees(),
        5e-4
    ));
    assert!(within(
        roundtripped[0].tz_offset_hours,
        original[0].tz_offset_hours,
        1e-4
    ));
}

#[test]
fn acceptance_parses_all_entries_without_error() {
    let Some(path) = specimen_path() else {
        eprintln!("ASTRO_SPECIMENS not set — skipping integration test");
        return;
    };
    if !path.exists() {
        eprintln!("ADB specimen absent ({}); skipping", path.display());
        return;
    }

    let xml = std::fs::read_to_string(&path).expect("read specimen");
    let charts = parse_file(&xml).expect("parse specimen");
    // The full ADB export has tens of thousands of entries.
    assert!(
        charts.len() > 10_000,
        "expected >10k entries, got {}",
        charts.len()
    );
}
