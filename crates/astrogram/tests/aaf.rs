use astrogram::aaf::parse_file;

const SAMPLE: &str = "\
#: Astrolog 7.80
#A93:*,Lilly,William,11.05.1602,02:00,Diseworth,GB
#B93:2306308.587,52N47,1W11,*,L
#: AAF end
";

const MULTI: &str = "\
#: two charts
#A93:*,Valens,Vettius,08.02.0120,18:35,Antioch,TR
#B93:1764928.174,36N14,36E07,*,L
#A93:*,Haenel,Adele,11.02.1989,16:20,Paris,FR
#B93:2447569.139,48N52,2E07,1E,0
";

// ── parse count ──────────────────────────────────────────────────────────────

#[test]
fn single_chart_parsed() {
    let charts = parse_file(SAMPLE).unwrap();
    assert_eq!(charts.len(), 1);
}

#[test]
fn multiple_charts_all_parsed() {
    let charts = parse_file(MULTI).unwrap();
    assert_eq!(charts.len(), 2);
}

#[test]
fn comment_lines_ignored() {
    let text =
        "#: header\n#A93:*,A,B,01.01.2000,12:00,City,Country\n#B93:0.0,0N0,0E0,0,0\n#: trailer\n";
    let charts = parse_file(text).unwrap();
    assert_eq!(charts.len(), 1);
}

#[test]
fn empty_file_returns_empty() {
    let charts = parse_file("").unwrap();
    assert!(charts.is_empty());
}

// ── name handling ─────────────────────────────────────────────────────────────

#[test]
fn name_last_first_combined() {
    let charts = parse_file(SAMPLE).unwrap();
    assert_eq!(charts[0].name, "Lilly, William");
}

#[test]
fn name_empty_first_uses_last_only() {
    let text = "#A93:*,Valens,,08.02.0120,18:35,Antioch,TR\n#B93:0.0,36N14,36E07,*,L\n";
    let charts = parse_file(text).unwrap();
    assert_eq!(charts[0].name, "Valens");
}

#[test]
fn semicolons_in_name_restored_to_commas() {
    // AAF escapes embedded commas as semicolons
    let text = "#A93:*,O'Brien; Jr.,Patrick,01.06.1980,08:00,Dublin,IE\n#B93:0.0,53N21,6W15,0,0\n";
    let charts = parse_file(text).unwrap();
    assert_eq!(charts[0].name, "O'Brien, Jr., Patrick");
}

#[test]
fn city_semicolon_restored() {
    // city field may contain semicolons encoding commas
    let text = "#A93:*,A,B,01.01.2000,00:00,Los Angeles; CA,US\n#B93:0.0,34N3,118W15,8W,0\n";
    let charts = parse_file(text).unwrap();
    assert_eq!(charts[0].city.as_deref(), Some("Los Angeles, CA"));
}

// ── date / time ───────────────────────────────────────────────────────────────

#[test]
fn date_fields_parsed() {
    let charts = parse_file(SAMPLE).unwrap();
    let c = &charts[0];
    assert_eq!(c.day, 11);
    assert_eq!(c.month, 5);
    assert_eq!(c.year, 1602);
}

#[test]
fn time_fields_parsed() {
    let charts = parse_file(SAMPLE).unwrap();
    let c = &charts[0];
    assert_eq!(c.hour, 2);
    assert_eq!(c.minute, 0);
    assert_eq!(c.second, 0);
}

#[test]
fn time_with_seconds_parsed() {
    let text = "#A93:*,X,Y,01.01.2000,12:34:56,City,Country\n#B93:0.0,0N0,0E0,0,0\n";
    let charts = parse_file(text).unwrap();
    assert_eq!(charts[0].second, 56);
}

// ── coordinates ───────────────────────────────────────────────────────────────

#[test]
fn latitude_north() {
    let charts = parse_file(SAMPLE).unwrap();
    let lat = charts[0].latitude.degrees();
    assert!((lat - (52.0 + 47.0 / 60.0)).abs() < 1e-6, "got {lat}");
}

#[test]
fn latitude_south() {
    // Jakarta -6.21°, 106.85° (docs/ref_synthetics.md) → DMS 6°12'36"S, 106°51'E
    let text = "#A93:*,X,Y,01.01.2000,00:00,Jakarta,ID\n#B93:0.0,6S12:36,106E51,7E,0\n";
    let charts = parse_file(text).unwrap();
    let lat = charts[0].latitude.degrees();
    assert!(
        (lat - -(6.0 + 12.0 / 60.0 + 36.0 / 3600.0)).abs() < 1e-6,
        "got {lat}"
    );
}

#[test]
fn longitude_west() {
    let charts = parse_file(SAMPLE).unwrap();
    let lon = charts[0].longitude.degrees();
    assert!((lon - -(1.0 + 11.0 / 60.0)).abs() < 1e-6, "got {lon}");
}

#[test]
fn longitude_east() {
    // Valens: 36E07
    let charts = parse_file(MULTI).unwrap();
    let lon = charts[0].longitude.degrees();
    assert!((lon - (36.0 + 7.0 / 60.0)).abs() < 1e-6, "got {lon}");
}

#[test]
fn coord_lowercase_hemisphere_accepted() {
    // astro.com AAF dialect uses lowercase n/s/e/w
    let text = "#A93:*,X,Y,01.01.2000,00:00,City,Country\n#B93:0.0,47n36,122w19,8W,0\n";
    let charts = parse_file(text).unwrap();
    assert!((charts[0].latitude.degrees() - (47.0 + 36.0 / 60.0)).abs() < 1e-6);
    assert!((charts[0].longitude.degrees() - -(122.0 + 19.0 / 60.0)).abs() < 1e-6);
}

#[test]
fn coord_with_seconds() {
    // 47N36:23 → 47 + 36/60 + 23/3600
    let text = "#A93:*,X,Y,01.01.2000,00:00,City,Country\n#B93:0.0,47N36:23,122W19:45,8W,0\n";
    let charts = parse_file(text).unwrap();
    let expected_lat = 47.0 + 36.0 / 60.0 + 23.0 / 3600.0;
    let expected_lon = -(122.0 + 19.0 / 60.0 + 45.0 / 3600.0);
    assert!((charts[0].latitude.degrees() - expected_lat).abs() < 1e-6);
    assert!((charts[0].longitude.degrees() - expected_lon).abs() < 1e-6);
}

// ── timezone ──────────────────────────────────────────────────────────────────

#[test]
fn zone_west_no_dst() {
    let text = "#A93:*,Strike,Lightning,12.11.1955,22:04,Universal City,US\n#B93:2435424.753,34N08,118W21,8W,0\n";
    let charts = parse_file(text).unwrap();
    assert!((charts[0].tz_offset_hours - (-8.0)).abs() < 1e-6);
    assert!(!charts[0].is_lmt);
}

#[test]
fn zone_lmt_ancient() {
    // Valens: *,L → LMT derived from longitude
    let charts = parse_file(MULTI).unwrap();
    assert!(charts[0].is_lmt);
}

#[test]
fn zone_east_with_minutes() {
    // Haenel: 1E → UTC+1
    let charts = parse_file(MULTI).unwrap();
    assert!((charts[1].tz_offset_hours - 1.0).abs() < 1e-6);
}

#[test]
fn zone_fractional_east() {
    // 5E30 → UTC+5.5
    let text = "#A93:*,X,Y,01.01.2000,00:00,City,Country\n#B93:0.0,22N0,78E0,5E30,0\n";
    let charts = parse_file(text).unwrap();
    assert!((charts[0].tz_offset_hours - 5.5).abs() < 1e-6);
}

#[test]
fn zone_dst_flag_adds_one_hour() {
    // 8W,D → -8 + 1 = -7
    let text = "#A93:*,X,Y,01.06.2000,00:00,City,Country\n#B93:0.0,47N0,122W0,8W,D\n";
    let charts = parse_file(text).unwrap();
    assert!((charts[0].tz_offset_hours - (-7.0)).abs() < 1e-6);
    assert!(!charts[0].is_lmt);
}

#[test]
fn zone_lmt_from_dst_l() {
    // DST=L → LMT derived from longitude
    let text = "#A93:*,X,Y,01.01.2000,00:00,City,Country\n#B93:0.0,47N36,120W0,*,L\n";
    let charts = parse_file(text).unwrap();
    // lon = -120°, LMT = -120/15 = -8.0
    assert!((charts[0].tz_offset_hours - (-8.0)).abs() < 1e-6);
    assert!(charts[0].is_lmt);
}

#[test]
fn zone_star_is_lmt() {
    // zone=* → LMT regardless of dst flag
    let text = "#A93:*,X,Y,01.01.2000,00:00,City,Country\n#B93:0.0,47N36,90W0,*,0\n";
    let charts = parse_file(text).unwrap();
    // lon = -90°, LMT = -90/15 = -6.0
    assert!((charts[0].tz_offset_hours - (-6.0)).abs() < 1e-6);
    assert!(charts[0].is_lmt);
}
