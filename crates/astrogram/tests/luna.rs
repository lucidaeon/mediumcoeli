//! Tests for the LUNA® extractor — pure parsing only, no network.

use astrogram::chart::{
    Chart, CoordinateSystem, EventType, HouseSystem, Latitude, Longitude, Zodiac,
};
use astrogram::luna::{
    FormTokens, LunaChart, candidate_status, chart_type_str, create_payload, delete_payload,
    edit_payload, extract_phenom_id, luna_chart_to_chart, luna_house_system,
    luna_type_to_event_type, luna_zodiac, map_rodden_rating, parse_cast_json, parse_form_tokens,
    parse_listing_page, parse_sidebar, source_id_for_rating,
};

// --- listing-page parser ---

const LISTING_HTML: &str = r#"
<table><tbody>
<tr data-chart-url="/radix-charts/view?uniwheel=2caebfdc-2e36-437f-835e-88d58aafdf87">
  <td><input type="checkbox"></td>
  <td data-sort="9"><span class="badge badge-primary">Natal</span></td>
  <td><a class="font-lg" href="/radix-charts/view?...">Amber Celeste</a></td>
  <td data-sort="1815-12-10T13:00:00+00:00">1815 December 10</td>
  <td>London, UK</td>
</tr>
<tr data-chart-url="/radix-charts/view?uniwheel=aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee">
  <td><input type="checkbox"></td>
  <td data-sort="2"><span class="badge badge-warning">Horary</span></td>
  <td><a class="font-lg" href="/...">A Question</a></td>
  <td data-sort="2026-03-15T18:22:08+03:00">2026 March 15</td>
  <td>Moscow, Russia</td>
</tr>
</tbody></table>
"#;

#[test]
fn listing_extracts_chart_ids() {
    let rows = parse_listing_page(LISTING_HTML);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].chart_id, "2caebfdc-2e36-437f-835e-88d58aafdf87");
    assert_eq!(rows[1].chart_id, "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee");
}

#[test]
fn listing_extracts_names() {
    let rows = parse_listing_page(LISTING_HTML);
    assert_eq!(rows[0].name, "Amber Celeste");
    assert_eq!(rows[1].name, "A Question");
}

#[test]
fn listing_extracts_chart_types() {
    let rows = parse_listing_page(LISTING_HTML);
    assert_eq!(rows[0].chart_type, "natal");
    assert_eq!(rows[1].chart_type, "horary");
}

#[test]
fn listing_extracts_datetimes() {
    let rows = parse_listing_page(LISTING_HTML);
    assert_eq!((rows[0].year, rows[0].month, rows[0].day), (1815, 12, 10));
    assert_eq!((rows[0].hour, rows[0].minute, rows[0].second), (13, 0, 0));
    assert_eq!((rows[1].year, rows[1].month, rows[1].day), (2026, 3, 15));
    assert_eq!((rows[1].hour, rows[1].minute, rows[1].second), (18, 22, 8));
}

#[test]
fn listing_empty_html_gives_empty_vec() {
    assert!(parse_listing_page("<html></html>").is_empty());
}

// --- cast.json parser ---

const CAST_JSON: &str = r#"{
  "svg": "<svg/>",
  "uniwheel": {
    "datepicker": "1815-12-10",
    "eventTime": "13:00:00",
    "latitude": 51.5072178,
    "longitude": -0.1275862,
    "offset": "UTC-00:00:32",
    "timezone": "Europe/London",
    "zodiac": "Tropical",
    "location": "London, UK"
  },
  "alert": ""
}"#;

#[test]
fn cast_json_extracts_date() {
    let meta = parse_cast_json(CAST_JSON).unwrap();
    assert_eq!(meta.date, "1815-12-10");
}

#[test]
fn cast_json_extracts_time() {
    let meta = parse_cast_json(CAST_JSON).unwrap();
    assert_eq!(meta.time, "13:00:00");
}

#[test]
fn cast_json_extracts_coordinates() {
    let meta = parse_cast_json(CAST_JSON).unwrap();
    assert!((meta.lat - 51.507_217_8).abs() < 1e-6);
    assert!((meta.lon - -0.127_586_2).abs() < 1e-6);
}

#[test]
fn cast_json_extracts_offset_and_location() {
    let meta = parse_cast_json(CAST_JSON).unwrap();
    assert_eq!(meta.offset_str, "UTC-00:00:32");
    assert_eq!(meta.location, "London, UK");
    assert_eq!(meta.zodiac, "Tropical");
}

#[test]
fn cast_json_errors_on_missing_uniwheel() {
    assert!(parse_cast_json("{}").is_err());
}

// --- sidebar parser ---

const SIDEBAR_HTML: &str = r##"
<html><body>
<div class="sidebar-info">
  <p>House System</p><p>Whole Sign</p>
  <p>Zodiac</p><p>Tropical</p>
  <p>Locus</p><p>Geocentric</p>
  <p>Timezone</p><p>LMT</p>
</div>
<a href="#rodden-rating">(B) - Bio/autobiography</a>
</body></html>
"##;

const SIDEBAR_PLACIDUS_HTML: &str = r##"
<html><body>
<div class="sidebar-info">
  <p>House System</p><p>Placidus</p>
  <p>Zodiac</p><p>Lahiri</p>
  <p>Locus</p><p>Geocentric</p>
  <p>Timezone</p><p>EST</p>
</div>
<a href="#rodden-rating">(AA) - BC/BR in hand</a>
</body></html>
"##;

#[test]
fn sidebar_extracts_house_system() {
    let meta = parse_sidebar(SIDEBAR_HTML);
    assert_eq!(meta.house_system, "Whole Sign");
}

#[test]
fn sidebar_extracts_zodiac() {
    let meta = parse_sidebar(SIDEBAR_HTML);
    assert_eq!(meta.zodiac, "Tropical");
}

#[test]
fn sidebar_detects_lmt() {
    let meta = parse_sidebar(SIDEBAR_HTML);
    assert!(meta.is_lmt);
    assert_eq!(meta.tz_abbrev, "LMT");
}

#[test]
fn sidebar_non_lmt_timezone() {
    let meta = parse_sidebar(SIDEBAR_PLACIDUS_HTML);
    assert!(!meta.is_lmt);
    assert_eq!(meta.tz_abbrev, "EST");
}

#[test]
fn sidebar_extracts_rodden_code_and_desc() {
    let meta = parse_sidebar(SIDEBAR_HTML);
    assert_eq!(meta.rodden_code, "B");
    assert_eq!(meta.rodden_desc, "Bio/autobiography");
}

#[test]
fn sidebar_extracts_aa_rodden() {
    let meta = parse_sidebar(SIDEBAR_PLACIDUS_HTML);
    assert_eq!(meta.rodden_code, "AA");
}

const SIDEBAR_WITH_PHENOM_HTML: &str = r##"
<html><body>
<div class="sidebar-info">
  <p>House System</p><p>Placidus</p>
  <p>Zodiac</p><p>Tropical</p>
  <p>Timezone</p><p>EST</p>
</div>
<a href="#rodden-rating">(A) - From memory</a>
<a href="/phenomena/edit/deadbeef-1234-5678-abcd-000000000002" class="float-right">Edit</a>
</body></html>
"##;

#[test]
fn sidebar_extracts_phenom_id() {
    let meta = parse_sidebar(SIDEBAR_WITH_PHENOM_HTML);
    assert_eq!(
        meta.phenom_id,
        Some("deadbeef-1234-5678-abcd-000000000002".to_string())
    );
}

#[test]
fn sidebar_phenom_id_none_when_no_edit_link() {
    let meta = parse_sidebar(SIDEBAR_HTML);
    assert!(meta.phenom_id.is_none());
}

// --- offset parsing (via luna_chart_to_chart) ---

#[test]
fn offset_zero_parses() {
    let c = make_luna_chart("UTC+00:00:00");
    let chart = luna_chart_to_chart(&c).unwrap();
    assert!((chart.tz_offset_hours - 0.0).abs() < 1e-9);
}

#[test]
fn negative_small_offset_parses() {
    // UTC-00:00:32 → -(32/3600)
    let c = make_luna_chart("UTC-00:00:32");
    let chart = luna_chart_to_chart(&c).unwrap();
    let expected = -(32.0 / 3600.0);
    assert!(
        (chart.tz_offset_hours - expected).abs() < 1e-9,
        "{}",
        chart.tz_offset_hours
    );
}

#[test]
fn positive_offset_parses() {
    // UTC+05:30:00 → 5.5
    let c = make_luna_chart("UTC+05:30:00");
    let chart = luna_chart_to_chart(&c).unwrap();
    assert!(
        (chart.tz_offset_hours - 5.5).abs() < 1e-9,
        "{}",
        chart.tz_offset_hours
    );
}

#[test]
fn negative_offset_parses() {
    // UTC-07:00:00 → -7.0
    let c = make_luna_chart("UTC-07:00:00");
    let chart = luna_chart_to_chart(&c).unwrap();
    assert!((chart.tz_offset_hours - -7.0).abs() < 1e-9);
}

// --- event type mapping ---

#[test]
fn natal_maps_to_unspecified() {
    assert_eq!(luna_type_to_event_type("natal"), EventType::Unspecified);
}

#[test]
fn event_maps_to_event() {
    assert_eq!(luna_type_to_event_type("event"), EventType::Event);
}

#[test]
fn horary_maps_to_horary() {
    assert_eq!(luna_type_to_event_type("horary"), EventType::Horary);
}

#[test]
fn unknown_type_maps_to_unspecified() {
    assert_eq!(luna_type_to_event_type("unknown"), EventType::Unspecified);
}

// --- house system mapping ---

#[test]
fn house_system_whole_sign() {
    use astrogram::chart::HouseSystem;
    assert_eq!(luna_house_system("Whole Sign"), HouseSystem::WholeSign);
}

#[test]
fn house_system_placidus() {
    use astrogram::chart::HouseSystem;
    assert_eq!(luna_house_system("Placidus"), HouseSystem::Placidus);
}

#[test]
fn house_system_koch() {
    use astrogram::chart::HouseSystem;
    assert_eq!(luna_house_system("Koch"), HouseSystem::Koch);
}

// --- zodiac mapping ---

#[test]
fn zodiac_tropical() {
    use astrogram::chart::Zodiac;
    assert_eq!(luna_zodiac("Tropical"), Zodiac::Tropical);
}

#[test]
fn zodiac_lahiri() {
    use astrogram::chart::Zodiac;
    assert_eq!(luna_zodiac("Lahiri"), Zodiac::Lahiri);
}

// --- rodden rating mapping ---

#[test]
fn rodden_code_b_formats_correctly() {
    assert_eq!(
        map_rodden_rating("B", "Bio/autobiography"),
        Some("B Bio/autobiography".to_string())
    );
}

#[test]
fn rodden_code_aa_no_desc() {
    assert_eq!(map_rodden_rating("AA", ""), Some("AA".to_string()));
}

#[test]
fn rodden_empty_gives_none() {
    assert_eq!(map_rodden_rating("", ""), None);
}

#[test]
fn rodden_truncates_to_32_chars() {
    let long_desc = "a".repeat(50);
    let rating = map_rodden_rating("A", &long_desc).unwrap();
    assert!(rating.len() <= 32);
}

// --- full luna_chart_to_chart conversion ---

#[test]
fn full_conversion_amber_celeste() {
    let c = LunaChart {
        chart_id: "2caebfdc-2e36-437f-835e-88d58aafdf87".to_string(),
        name: "Amber Celeste".to_string(),
        chart_type: "natal".to_string(),
        date: "1815-12-10".to_string(),
        time: "13:00:00".to_string(),
        lat: 51.507_217_8,
        lon: -0.127_586_2,
        offset_str: "UTC-00:00:32".to_string(),
        location: "London, UK".to_string(),
        zodiac: "Tropical".to_string(),
        house_system: "Whole Sign".to_string(),
        tz_abbrev: "LMT".to_string(),
        is_lmt: true,
        rodden_code: "B".to_string(),
        rodden_desc: "Bio/autobiography".to_string(),
        notes: String::new(),
    };
    let chart = luna_chart_to_chart(&c).unwrap();
    assert_eq!(chart.name, "Amber Celeste");
    assert_eq!((chart.year, chart.month, chart.day), (1815, 12, 10));
    assert_eq!((chart.hour, chart.minute, chart.second), (13, 0, 0));
    assert!((chart.latitude.degrees() - 51.507_217_8).abs() < 1e-6);
    assert!((chart.longitude.degrees() - -0.127_586_2).abs() < 1e-6);
    let expected_tz = -(32.0 / 3600.0);
    assert!(
        (chart.tz_offset_hours - expected_tz).abs() < 1e-9,
        "{}",
        chart.tz_offset_hours
    );
    assert_eq!(chart.event_type, EventType::Unspecified);
    assert_eq!(chart.source_rating.as_deref(), Some("B Bio/autobiography"));
    assert!(chart.is_lmt);
    assert_eq!(chart.city.as_deref(), Some("London"));
}

// --- helpers ---

fn make_luna_chart(offset_str: &str) -> LunaChart {
    LunaChart {
        chart_id: "test-id".to_string(),
        name: "Test".to_string(),
        chart_type: "natal".to_string(),
        date: "2000-01-01".to_string(),
        time: "12:00:00".to_string(),
        lat: 51.5,
        lon: -0.117,
        offset_str: offset_str.to_string(),
        location: "London, UK".to_string(),
        zodiac: "Tropical".to_string(),
        house_system: "Placidus".to_string(),
        tz_abbrev: "GMT".to_string(),
        is_lmt: false,
        rodden_code: "A".to_string(),
        rodden_desc: String::new(),
        notes: String::new(),
    }
}

// ── write helpers ─────────────────────────────────────────────────────────────

const ADD_FORM_HTML: &str = r#"<!DOCTYPE html>
<html><body>
<form action="/phenomena/add" method="post">
  <input type="hidden" name="_csrfToken" value="abc123csrf">
  <input type="hidden" name="_Token[fields]" value="fieldsvalue%3A">
  <input type="hidden" name="_Token[unlocked]" value="unlockedvalue">
  <input type="text" name="name">
</form>
</body></html>"#;

const EDIT_FORM_HTML: &str = r#"<!DOCTYPE html>
<html><body>
<form action="/phenomena/edit/deadbeef-1234-5678-abcd-000000000000" method="post">
  <input type="hidden" name="_csrfToken" value="editcsrf999">
  <input type="hidden" name="_Token[fields]" value="editfields">
  <input type="hidden" name="_Token[unlocked]" value="editunlocked">
</form>
</body></html>"#;

// --- parse_form_tokens ---

#[test]
fn parse_add_form_tokens() {
    let tokens = parse_form_tokens(ADD_FORM_HTML, "/phenomena/add").unwrap();
    assert_eq!(tokens.csrf, "abc123csrf");
    assert_eq!(tokens.fields, "fieldsvalue%3A");
    assert_eq!(tokens.unlocked, "unlockedvalue");
}

#[test]
fn parse_edit_form_tokens() {
    let tokens = parse_form_tokens(
        EDIT_FORM_HTML,
        "/phenomena/edit/deadbeef-1234-5678-abcd-000000000000",
    )
    .unwrap();
    assert_eq!(tokens.csrf, "editcsrf999");
    assert_eq!(tokens.fields, "editfields");
    assert_eq!(tokens.unlocked, "editunlocked");
}

#[test]
fn parse_form_tokens_returns_none_when_form_absent() {
    assert!(parse_form_tokens("<html><body>no form</body></html>", "/phenomena/add").is_none());
}

// --- chart_type_str ---

#[test]
fn chart_type_str_male_is_natal() {
    assert_eq!(chart_type_str(astrogram::chart::EventType::Male), "natal");
}

#[test]
fn chart_type_str_female_is_natal() {
    assert_eq!(chart_type_str(astrogram::chart::EventType::Female), "natal");
}

#[test]
fn chart_type_str_event_is_event() {
    assert_eq!(chart_type_str(astrogram::chart::EventType::Event), "event");
}

#[test]
fn chart_type_str_horary_is_horary() {
    assert_eq!(
        chart_type_str(astrogram::chart::EventType::Horary),
        "horary"
    );
}

// --- source_id_for_rating ---

#[test]
fn source_id_maps_standard_codes() {
    assert_eq!(source_id_for_rating(Some("AA")), 1);
    assert_eq!(source_id_for_rating(Some("A")), 3);
    assert_eq!(source_id_for_rating(Some("B")), 5);
    assert_eq!(source_id_for_rating(Some("C")), 6);
    assert_eq!(source_id_for_rating(Some("DD")), 9);
    assert_eq!(source_id_for_rating(Some("X")), 10);
    assert_eq!(source_id_for_rating(Some("XX")), 12);
}

#[test]
fn source_id_handles_combined_strings() {
    assert_eq!(source_id_for_rating(Some("AA BC in hand")), 1);
    assert_eq!(source_id_for_rating(Some("B Bio/autobiography")), 5);
}

#[test]
fn source_id_unknown_defaults_to_99() {
    assert_eq!(source_id_for_rating(None), 99);
    assert_eq!(source_id_for_rating(Some("?")), 99);
}

// --- create_payload ---

#[test]
fn create_payload_contains_required_fields() {
    use astrogram::chart::{
        Chart, CoordinateSystem, EventType, HouseSystem, Latitude, Longitude, Zodiac,
    };
    let chart = Chart {
        name: "Amber Celeste".to_string(),
        secondary_name: None,
        city: Some("London".to_string()),
        region: Some("UK".to_string()),
        longitude: Longitude::new(-0.1276).unwrap(),
        latitude: Latitude::new(51.5074).unwrap(),
        year: 1815,
        month: 12,
        day: 10,
        hour: 13,
        minute: 0,
        second: 0,
        tz_offset_hours: 0.0,
        tz_abbreviation: None,
        is_lmt: false,
        event_type: EventType::Female,
        source_rating: Some("B".to_string()),
        house_system: HouseSystem::Placidus,
        zodiac: Zodiac::Tropical,
        coordinate_system: CoordinateSystem::Geocentric,
        sub_charts: vec![],
        notes: None,
    };
    let tokens = FormTokens {
        csrf: "tok1".to_string(),
        fields: "tok2".to_string(),
        unlocked: "tok3".to_string(),
    };
    let payload = create_payload(&chart, &tokens);
    let map: std::collections::HashMap<_, _> = payload.iter().cloned().collect();

    assert_eq!(map["name"], "Amber Celeste");
    assert_eq!(map["type"], "natal");
    assert_eq!(map["primary_radix_chart[event_date]"], "1815-12-10");
    assert_eq!(map["primary_radix_chart[event_time]"], "13:00:00");
    assert_eq!(map["primary_radix_chart[location]"], "London, UK");
    assert_eq!(map["primary_radix_chart[chart_source_id]"], "5");
    assert_eq!(map["_csrfToken"], "tok1");
    assert_eq!(map["_Token[fields]"], "tok2");
    assert_eq!(map["_Token[unlocked]"], "tok3");
    assert_eq!(map["tags"], "");

    // Coordinates: ISO 6709 (East positive)
    let lat: f64 = map["primary_radix_chart[latitude]"].parse().unwrap();
    let lon: f64 = map["primary_radix_chart[longitude]"].parse().unwrap();
    assert!((lat - 51.5074).abs() < 1e-4);
    assert!((lon - (-0.1276)).abs() < 1e-4);
}

// --- edit_payload ---

fn make_test_chart() -> astrogram::chart::Chart {
    use astrogram::chart::{
        Chart, CoordinateSystem, EventType, HouseSystem, Latitude, Longitude, Zodiac,
    };
    Chart {
        name: "Test Chart".to_string(),
        secondary_name: None,
        city: Some("London".to_string()),
        region: None,
        longitude: Longitude::new(-0.1276).unwrap(),
        latitude: Latitude::new(51.5074).unwrap(),
        year: 2000,
        month: 1,
        day: 1,
        hour: 12,
        minute: 0,
        second: 0,
        tz_offset_hours: 0.0,
        tz_abbreviation: None,
        is_lmt: false,
        event_type: EventType::Unspecified,
        source_rating: None,
        house_system: HouseSystem::Placidus,
        zodiac: Zodiac::Tropical,
        coordinate_system: CoordinateSystem::Geocentric,
        sub_charts: vec![],
        notes: None,
    }
}

#[test]
fn edit_payload_prepends_method_put() {
    let tokens = FormTokens {
        csrf: "c".into(),
        fields: "f".into(),
        unlocked: "u".into(),
    };
    let payload = edit_payload(&make_test_chart(), &tokens);
    assert_eq!(payload[0].0, "_method");
    assert_eq!(payload[0].1, "PUT");
}

#[test]
fn edit_payload_remaining_fields_match_create() {
    let tokens = FormTokens {
        csrf: "c".into(),
        fields: "f".into(),
        unlocked: "u".into(),
    };
    let chart = make_test_chart();
    let edit = edit_payload(&chart, &tokens);
    let create = create_payload(&chart, &tokens);
    assert_eq!(edit.len(), create.len() + 1);
    assert_eq!(&edit[1..], &create[..]);
}

// --- listing-only dedup contract ---

/// Confirms that `luna_chart_to_chart` with empty sidebar fields still populates
/// all fields required by consolidate dedup: name, year, month, day, hour,
/// minute, second, lat, lon.  Documents that the listing+`cast_json` data alone
/// is sufficient to build a dedup-correct Chart when sidebar is not fetched.
#[test]
fn luna_chart_without_sidebar_has_correct_dedup_fields() {
    let c = LunaChart {
        chart_id: "2caebfdc-2e36-437f-835e-88d58aafdf87".to_string(),
        name: "Amber Celeste".to_string(),
        chart_type: "natal".to_string(),
        date: "1815-12-10".to_string(),
        time: "13:00:00".to_string(),
        lat: 51.507_217_8,
        lon: -0.127_586_2,
        offset_str: "UTC-00:00:32".to_string(),
        location: "London, UK".to_string(),
        zodiac: "Tropical".to_string(),
        // sidebar fields intentionally empty (skip_sidebar path)
        house_system: String::new(),
        tz_abbrev: String::new(),
        is_lmt: false,
        rodden_code: String::new(),
        rodden_desc: String::new(),
        notes: String::new(),
    };
    let chart = luna_chart_to_chart(&c).unwrap();
    assert_eq!(chart.name, "Amber Celeste");
    assert_eq!((chart.year, chart.month, chart.day), (1815, 12, 10));
    assert_eq!((chart.hour, chart.minute, chart.second), (13, 0, 0));
    assert!((chart.latitude.degrees() - 51.507_217_8).abs() < 1e-6);
    assert!((chart.longitude.degrees() - -0.127_586_2).abs() < 1e-6);
    assert!(chart.source_rating.is_none());
}

// --- at_resume_point / needs_fetch_for_normalize ---

use astrogram::luna::{at_resume_point, needs_fetch_for_normalize};

#[test]
fn resume_matches_case_insensitive_prefix() {
    assert!(at_resume_point("Jonathan Smith", "jo"));
    assert!(!at_resume_point("Kate", "jo"));
}

#[test]
fn resume_empty_prefix_matches_everything() {
    assert!(at_resume_point("Anything", ""));
}

#[test]
fn clean_ascii_no_fetch_needed() {
    assert!(!needs_fetch_for_normalize("Smith, John"));
}

#[test]
fn truncated_name_needs_fetch() {
    assert!(needs_fetch_for_normalize("Amber Celeste Very Long Nam…"));
}

#[test]
fn non_cp1252_char_needs_fetch() {
    assert!(needs_fetch_for_normalize("Amber ★ Celeste"));
}

// --- LunaSession compile check ---

#[test]
fn luna_session_new_type_check() {
    let result = astrogram::luna::LunaSession::new("dummy-cookie", 0, "test/1.0");
    assert!(
        result.is_ok(),
        "LunaSession::new should not fail with a simple cookie string"
    );
}

// --- extract_phenom_id ---

#[test]
fn extract_phenom_id_from_redirect_url() {
    let url = "https://www.lunaastrology.com/phenomena/edit/deadbeef-1234-5678-abcd-000000000001";
    assert_eq!(
        extract_phenom_id(url),
        Some("deadbeef-1234-5678-abcd-000000000001".to_string())
    );
}

#[test]
fn extract_phenom_id_from_html_body() {
    let html = r#"<a href="/phenomena/edit/feedface-0000-1111-2222-333333333333">Edit</a>"#;
    assert_eq!(
        extract_phenom_id(html),
        Some("feedface-0000-1111-2222-333333333333".to_string())
    );
}

#[test]
fn extract_phenom_id_returns_none_when_absent() {
    assert!(extract_phenom_id("no uuid here").is_none());
}

// --- delete_payload: CakePHP DELETE method tunnel ---

fn fake_tokens() -> FormTokens {
    FormTokens {
        csrf: "CSRF-XYZ".into(),
        fields: "FIELDS-XYZ".into(),
        unlocked: "UNLOCKED-XYZ".into(),
    }
}

#[test]
fn delete_payload_starts_with_method_post() {
    // LUNA's delete route is reached by POSTing to /phenomena/delete/<uuid>;
    // _method=POST tells CakePHP this is a normal POST (not a spoofed DELETE).
    let payload = delete_payload(&fake_tokens());
    assert_eq!(payload[0], ("_method".to_string(), "POST".to_string()));
}

#[test]
fn delete_payload_carries_all_three_tokens() {
    let payload = delete_payload(&fake_tokens());
    let map: std::collections::HashMap<_, _> = payload.iter().cloned().collect();
    assert_eq!(map.get("_csrfToken").map(String::as_str), Some("CSRF-XYZ"));
    assert_eq!(
        map.get("_Token[fields]").map(String::as_str),
        Some("FIELDS-XYZ")
    );
    assert_eq!(
        map.get("_Token[unlocked]").map(String::as_str),
        Some("UNLOCKED-XYZ")
    );
}

#[test]
fn delete_payload_contains_no_chart_fields() {
    let payload = delete_payload(&fake_tokens());
    let keys: std::collections::HashSet<_> = payload.iter().map(|(k, _)| k.as_str()).collect();
    let allowed: std::collections::HashSet<&str> = [
        "_method",
        "_csrfToken",
        "_Token[fields]",
        "_Token[unlocked]",
    ]
    .iter()
    .copied()
    .collect();
    let extras: Vec<_> = keys.difference(&allowed).collect();
    assert!(extras.is_empty(), "unexpected payload keys: {extras:?}");
}

// --- candidate_status: inline duplicate flag for the fetch loop ---

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
) -> Chart {
    Chart {
        name: name.to_string(),
        secondary_name: None,
        city: None,
        region: None,
        longitude: Longitude::new(lon).unwrap(),
        latitude: Latitude::new(lat).unwrap(),
        year,
        month,
        day,
        hour,
        minute,
        second,
        tz_offset_hours: 0.0,
        tz_abbreviation: None,
        is_lmt: false,
        event_type: EventType::Unspecified,
        source_rating: None,
        house_system: HouseSystem::Placidus,
        zodiac: Zodiac::Tropical,
        coordinate_system: CoordinateSystem::Geocentric,
        sub_charts: vec![],
        notes: None,
    }
}

#[test]
fn candidate_status_returns_ok_when_no_match() {
    let c = chart("Solo", 51.5, -0.117, 1815, 12, 10, 7, 30, 0);
    assert_eq!(candidate_status(&c, &[], &[]), "ok");
}

#[test]
fn candidate_status_returns_ok_when_existing_charts_dont_match() {
    let existing = chart("Earlier", 40.0, -75.0, 1990, 5, 1, 12, 0, 0);
    let c = chart("Different", 51.5, -0.117, 1815, 12, 10, 7, 30, 0);
    assert_eq!(candidate_status(&c, &[existing], &[1]), "ok");
}

#[test]
fn candidate_status_flags_match_with_listing_index() {
    // Match on spacetime even though the names differ — the whole point of
    // the candidate flag.
    let earlier = chart("Terse", 40.0, -75.0, 1990, 5, 1, 12, 0, 0);
    let later = chart(
        "Terse plus a long descriptor",
        40.0,
        -75.0,
        1990,
        5,
        1,
        12,
        0,
        0,
    );
    assert_eq!(
        candidate_status(&later, &[earlier], &[5]),
        "ok  \u{26a0} candidate of #5"
    );
}

#[test]
fn candidate_status_reports_listing_index_not_vec_index() {
    // The fetch loop skips some rows; listing_indices preserves the
    // human-visible position so "#N" matches what was printed earlier.
    let a = chart("A", 10.0, 10.0, 1990, 5, 1, 12, 0, 0);
    let b = chart("B", 40.0, -75.0, 1990, 5, 1, 12, 0, 0); // the match
    let c = chart("Later", 40.0, -75.0, 1990, 5, 1, 12, 0, 0);
    // Listing positions 3 and 7 were kept; b sits at vec index 1.
    let status = candidate_status(&c, &[a, b], &[3, 7]);
    assert_eq!(status, "ok  \u{26a0} candidate of #7");
}
