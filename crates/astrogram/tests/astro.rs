use astrogram::astro::{
    create_payload, edit_payload, extract_aaf, offset_to_szon, parse_listing, parse_nhor_from_url,
};
use astrogram::chart::{
    Chart, CoordinateSystem, EventType, HouseSystem, Latitude, Longitude, Zodiac,
};

fn make_chart(name: &str) -> Chart {
    Chart {
        name: name.to_string(),
        secondary_name: None,
        city: Some("Diseworth".to_string()),
        region: Some("GB".to_string()),
        longitude: Longitude::new(-1.0 - 11.0 / 60.0).unwrap(),
        latitude: Latitude::new(52.0 + 47.0 / 60.0).unwrap(),
        year: 1602,
        month: 5,
        day: 11,
        hour: 2,
        minute: 0,
        second: 0,
        tz_offset_hours: -(4.0 / 60.0 + 44.0 / 3600.0),
        tz_abbreviation: None,
        is_lmt: true,
        event_type: EventType::Male,
        source_rating: None,
        house_system: HouseSystem::Placidus,
        zodiac: Zodiac::Tropical,
        coordinate_system: CoordinateSystem::Geocentric,
        sub_charts: vec![],
        notes: None,
    }
}

// ── parse_listing ─────────────────────────────────────────────────────────────

const LISTING_HTML: &str = r#"
<html><body>
<table>
  <tr><td><a href="/cgi/ade.cgi?&amp;nhor=1&amp;ract=xx" class="txt6p" title="edit birth data for Lilly, William">Edit</a></td></tr>
  <tr><td><a href="/cgi/ade.cgi?&amp;nhor=3&amp;ract=xx" class="txt6p" title="edit birth data for Valens">Edit</a></td></tr>
  <tr><td><a href="/cgi/ade.cgi?&amp;nhor=6&amp;ract=xx" class="txt6p" title="edit birth data for Freud, Anna">Edit</a></td></tr>
</table>
</body></html>
"#;

#[test]
fn listing_extracts_nhor_ids_and_names() {
    let charts = parse_listing(LISTING_HTML);
    assert_eq!(charts.len(), 3);
    assert_eq!(charts[0].nhor_id, 1);
    assert_eq!(charts[1].nhor_id, 3);
    assert_eq!(charts[2].nhor_id, 6);
}

#[test]
fn listing_name_from_title_attribute() {
    let charts = parse_listing(LISTING_HTML);
    assert_eq!(charts[0].name, "Lilly, William");
}

#[test]
fn listing_plain_name_unchanged() {
    let charts = parse_listing(LISTING_HTML);
    assert_eq!(charts[1].name, "Valens");
}

#[test]
fn listing_empty_html_returns_empty() {
    let charts = parse_listing("<html><body></body></html>");
    assert!(charts.is_empty());
}

// ── extract_aaf ───────────────────────────────────────────────────────────────

const AAF_HTML: &str = r"
<html><body>
<h2>AAF Export</h2>
<pre>
#: Astrolog 7.80
#A93:*,Lilly,William,11.05.1602,02:00,Diseworth,GB
#B93:2306308.587,52N47,1W11,*,L
</pre>
</body></html>
";

#[test]
fn aaf_extracted_from_pre_block() {
    let text = extract_aaf(AAF_HTML).unwrap();
    assert!(text.contains("#A93:"));
    assert!(text.contains("#B93:"));
}

#[test]
fn html_entities_decoded_in_pre() {
    let html = "<html><body><pre>#: test &amp; data\n#A93:*,A,B,01.01.2000,12:00,C,D\n#B93:0.0,0N0,0E0,0,0\n</pre></body></html>";
    let text = extract_aaf(html).unwrap();
    assert!(text.contains("test & data"));
}

#[test]
fn no_pre_block_returns_none() {
    let text = extract_aaf("<html><body>no pre here</body></html>");
    assert!(text.is_none());
}

// ── parse_nhor_from_url ───────────────────────────────────────────────────────

#[test]
fn nhor_from_semicolon_query() {
    let url = "https://www.astro.com/cgi/awd.cgi?;nhor=5";
    assert_eq!(parse_nhor_from_url(url), Some(5));
}

#[test]
fn nhor_from_amp_query() {
    let url = "https://www.astro.com/cgi/awd.cgi?lang=e&nhor=12";
    assert_eq!(parse_nhor_from_url(url), Some(12));
}

#[test]
fn nhor_missing_returns_none() {
    let url = "https://www.astro.com/cgi/awd.cgi?lang=e";
    assert_eq!(parse_nhor_from_url(url), None);
}

// ── offset_to_szon ────────────────────────────────────────────────────────────

#[test]
fn negative_offset_is_west() {
    assert_eq!(offset_to_szon(-8.0, false), "h8w00");
}

#[test]
fn positive_offset_is_east() {
    assert_eq!(offset_to_szon(1.0, false), "h1e00");
}

#[test]
fn zero_offset_is_east() {
    assert_eq!(offset_to_szon(0.0, false), "h0e00");
}

#[test]
fn fractional_offset_encodes_minutes() {
    // 5.5 → UTC+5:30
    assert_eq!(offset_to_szon(5.5, false), "h5e30");
}

#[test]
fn lmt_returns_lmt_string() {
    assert_eq!(offset_to_szon(-8.0, true), "lmt");
}

// ── create_payload ────────────────────────────────────────────────────────────

fn find_field<'a>(payload: &'a [(String, String)], key: &str) -> Option<&'a str> {
    payload
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}

#[test]
fn create_payload_has_subcon_continue() {
    let p = create_payload(&make_chart("Lilly, William"), "test_cid", "tok");
    assert_eq!(find_field(&p, "subcon"), Some("continue"));
}

#[test]
fn create_payload_has_btyp() {
    let p = create_payload(&make_chart("X"), "c", "tok");
    assert_eq!(find_field(&p, "btyp"), Some("w2at"));
}

#[test]
fn create_payload_name_with_comma_splits() {
    let p = create_payload(&make_chart("Lilly, William"), "c", "tok");
    assert_eq!(find_field(&p, "snam"), Some("Lilly"));
    assert_eq!(find_field(&p, "sfnm"), Some("William"));
}

#[test]
fn create_payload_name_without_comma_goes_to_sfnm() {
    // astro.com requires sfnm non-empty; no-comma names go in first-name slot
    let p = create_payload(&make_chart("Madonna"), "c", "tok");
    assert_eq!(find_field(&p, "sfnm"), Some("Madonna"));
    assert_eq!(find_field(&p, "snam"), Some(""));
}

#[test]
fn create_payload_tz_lmt() {
    let p = create_payload(&make_chart("X"), "c", "tok");
    assert_eq!(find_field(&p, "szon"), Some("lmt"));
}

#[test]
fn create_payload_male_sex() {
    let p = create_payload(&make_chart("X"), "c", "tok");
    assert_eq!(find_field(&p, "ssx"), Some("m"));
}

#[test]
fn create_payload_date_fields() {
    let p = create_payload(&make_chart("X"), "c", "tok");
    assert_eq!(find_field(&p, "sday"), Some("11"));
    assert_eq!(find_field(&p, "imon"), Some("5"));
    assert_eq!(find_field(&p, "syar"), Some("1602"));
}

#[test]
fn create_payload_includes_cid() {
    let p = create_payload(&make_chart("X"), "my_session_abc", "tok");
    assert_eq!(find_field(&p, "cid"), Some("my_session_abc"));
}

// ── edit_payload ──────────────────────────────────────────────────────────────

#[test]
fn edit_payload_includes_nhor() {
    let p = edit_payload(&make_chart("X"), 42, "c");
    assert_eq!(find_field(&p, "nhor"), Some("42"));
}

#[test]
fn edit_payload_has_subcon_continue() {
    let p = edit_payload(&make_chart("X"), 1, "c");
    assert_eq!(find_field(&p, "subcon"), Some("continue"));
}
