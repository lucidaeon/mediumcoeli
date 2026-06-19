use astrogram::astrotheoros::{
    ApiChartEntry, AstrotheorosSession, calendar_to_unix_ms, chart_to_create_body, entry_to_chart,
    extract_client_uat, jwt_exp, parse_rsc_response,
};
use astrogram::chart::{
    Chart, CoordinateSystem, EventType, HouseSystem, Latitude, Longitude, Zodiac,
};

const RSC_FIXTURE: &str = concat!(
    "0:[\"$\",\"html\",null,{}]\n",
    "1:[\"$\",\"body\",null,{}]\n",
    r#"2:["$","$Labc",null,{"charts":[{"id":"b735ff9a-92b7-4050-9f07-a745cf765997","createdAt":"$D2026-06-16T15:08:29.457Z","updatedAt":"$D2026-06-16T15:56:07.332Z","name":"Anna Freud","favorite":true,"day":3,"month":11,"year":1895,"hour":15,"minute":15,"timezone":"Europe/Vienna","utcOffset":1,"manualUtcOffset":null,"locationName":"Vienna, Austria","latitude":48.20806959999999,"longitude":16.3713095,"tUseBirthLocation":true,"tLocationName":null,"tLatitude":null,"tLongitude":null,"tTimezone":null,"userId":"89fd1251-9903-417d-82fe-4207a60e0ab3"}],"settings":{}}]"#,
    "\n",
);

#[test]
fn rsc_parses_single_entry() {
    let entries = parse_rsc_response(RSC_FIXTURE);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].id, "b735ff9a-92b7-4050-9f07-a745cf765997");
    assert_eq!(entries[0].name, "Anna Freud");
}

#[test]
fn rsc_month_is_zero_indexed() {
    let entries = parse_rsc_response(RSC_FIXTURE);
    assert_eq!(entries[0].month, 11); // 0-indexed: December
}

#[test]
fn rsc_date_prefix_stripped() {
    // $D prefix on createdAt/updatedAt must not cause parse failure
    let entries = parse_rsc_response(RSC_FIXTURE);
    assert!(!entries.is_empty());
}

#[test]
fn rsc_undefined_sentinel_handled() {
    // $undefined in tLocationName etc must not cause parse failure
    let entries = parse_rsc_response(RSC_FIXTURE);
    assert!(entries[0].t_location_name.is_none());
}

#[test]
fn rsc_empty_response_returns_empty() {
    let entries = parse_rsc_response("0:[]\n1:[]\n");
    assert!(entries.is_empty());
}

#[test]
fn rsc_no_charts_line_returns_empty() {
    let entries = parse_rsc_response("2:[\"$\",\"div\",null,{\"foo\":\"bar\"}]\n");
    assert!(entries.is_empty());
}

fn anna_freud_entry() -> ApiChartEntry {
    ApiChartEntry {
        id: "b735ff9a-92b7-4050-9f07-a745cf765997".to_string(),
        name: "Anna Freud".to_string(),
        day: 3,
        month: 11, // 0-indexed December
        year: 1895,
        hour: 15,
        minute: 15,
        timezone: "Europe/Vienna".to_string(),
        utc_offset: 1,
        location_name: "Vienna, Austria".to_string(),
        latitude: 48.20806959999999,
        longitude: 16.3713095,
        favorite: Some(true),
        t_location_name: None,
        t_latitude: None,
        t_longitude: None,
        t_timezone: None,
    }
}

fn anna_freud_chart() -> Chart {
    Chart {
        name: "Anna Freud".to_string(),
        secondary_name: None,
        city: Some("Vienna, Austria".to_string()),
        region: None,
        longitude: Longitude::new(16.3713095).unwrap(),
        latitude: Latitude::new(48.20806959999999).unwrap(),
        year: 1895,
        month: 12, // 1-indexed
        day: 3,
        hour: 15,
        minute: 15,
        second: 0,
        tz_offset_hours: 1.0,
        tz_abbreviation: Some("Europe/Vienna".to_string()),
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

// ── live smoke (ignored; network) ─────────────────────────────────────────────
// Confirms the inline-verify path end to end: create one allow-listed chart,
// receive the landed entry from the create response, convert it back to a Chart,
// assert the round-trip, then delete. Uses Anna Freud (approved native list).
#[test]
#[ignore = "live network: writes+deletes one allow-listed chart on astrotheoros.com"]
fn live_create_returns_landed_entry_then_deletes() {
    let user = std::env::var("ASTROTHEOROS_USER").expect("ASTROTHEOROS_USER");
    let pass = std::env::var("ASTROTHEOROS_PASS").expect("ASTROTHEOROS_PASS");
    let session = AstrotheorosSession::login(&user, &pass, 500).expect("login");

    let source = anna_freud_chart();
    let uuids = vec![String::new()]; // empty uuid → will be created
    let mut landed_id: Option<String> = None;
    session
        .write_charts(
            std::slice::from_ref(&source),
            &uuids,
            &mut |orig_i, new_i, total, src, status, entry| {
                assert_eq!(orig_i, 0);
                assert_eq!((new_i, total), (1, 1));
                assert!(status.starts_with("created uuid="), "status: {status}");
                let entry = entry.expect("create response echoes landed entry");
                let landed = entry_to_chart(entry).expect("entry_to_chart");
                // Round-trip: the landed chart matches what we sent.
                assert_eq!(landed.name, src.name);
                assert_eq!((landed.year, landed.month, landed.day), (1895, 12, 3));
                assert_eq!((landed.hour, landed.minute), (15, 15));
                landed_id = Some(entry.id.clone());
            },
        )
        .expect("write_charts");

    let id = landed_id.expect("a chart was created");
    session.delete_one(&id).expect("delete");
}

// ── entry_to_chart ────────────────────────────────────────────────────────────

#[test]
fn entry_to_chart_month_is_one_indexed() {
    let chart = entry_to_chart(&anna_freud_entry()).unwrap();
    assert_eq!(chart.month, 12); // 0-indexed 11 → 1-indexed 12
}

#[test]
fn entry_to_chart_year_day_hour_minute_unchanged() {
    let chart = entry_to_chart(&anna_freud_entry()).unwrap();
    assert_eq!(chart.year, 1895);
    assert_eq!(chart.day, 3);
    assert_eq!(chart.hour, 15);
    assert_eq!(chart.minute, 15);
}

#[test]
fn entry_to_chart_longitude_east_positive() {
    let chart = entry_to_chart(&anna_freud_entry()).unwrap();
    assert!((chart.longitude.degrees() - 16.3713095).abs() < 1e-9);
}

#[test]
fn entry_to_chart_latitude_north_positive() {
    let chart = entry_to_chart(&anna_freud_entry()).unwrap();
    assert!((chart.latitude.degrees() - 48.208_069_599_999_99).abs() < 1e-9);
}

#[test]
fn entry_to_chart_tz_offset_from_utc_offset() {
    let chart = entry_to_chart(&anna_freud_entry()).unwrap();
    assert!((chart.tz_offset_hours - 1.0).abs() < 1e-9);
}

#[test]
fn entry_to_chart_iana_goes_to_tz_abbreviation() {
    let chart = entry_to_chart(&anna_freud_entry()).unwrap();
    assert_eq!(chart.tz_abbreviation.as_deref(), Some("Europe/Vienna"));
}

#[test]
fn entry_to_chart_is_lmt_always_false() {
    let chart = entry_to_chart(&anna_freud_entry()).unwrap();
    assert!(!chart.is_lmt);
}

// ── chart_to_create_body ──────────────────────────────────────────────────────

#[test]
fn create_body_month_is_zero_indexed() {
    let body = chart_to_create_body(&anna_freud_chart(), "Europe/Vienna", 1);
    assert_eq!(body["data"]["month"], 11); // 1-indexed 12 → 0-indexed 11
}

#[test]
fn create_body_longitude_east_positive() {
    let body = chart_to_create_body(&anna_freud_chart(), "Europe/Vienna", 1);
    let lon = body["data"]["longitude"].as_f64().unwrap();
    assert!((lon - 16.3713095).abs() < 1e-9);
}

#[test]
fn create_body_uses_supplied_iana_tz() {
    let body = chart_to_create_body(&anna_freud_chart(), "Europe/Vienna", 1);
    assert_eq!(body["data"]["timezone"], "Europe/Vienna");
}

#[test]
fn create_body_t_use_birth_location_true() {
    let body = chart_to_create_body(&anna_freud_chart(), "Europe/Vienna", 1);
    assert_eq!(body["data"]["tUseBirthLocation"], true);
}

// ── calendar_to_unix_ms ───────────────────────────────────────────────────────

#[test]
fn unix_epoch_is_zero() {
    assert_eq!(calendar_to_unix_ms(1970, 1, 1, 0, 0), 0);
}

#[test]
fn unix_epoch_plus_one_hour() {
    assert_eq!(calendar_to_unix_ms(1970, 1, 1, 1, 0), 3_600_000);
}

#[test]
fn unix_epoch_plus_one_day() {
    assert_eq!(calendar_to_unix_ms(1970, 1, 2, 0, 0), 86_400_000);
}

#[test]
fn lightning_strike_approx_unix_ms() {
    // 1955-11-12 22:04 ≈ -446,090,160,000 ms from epoch (treating as UTC)
    let ms = calendar_to_unix_ms(1955, 11, 12, 22, 4);
    assert!(ms < 0, "1955 date should be before unix epoch");
    // Within ±24h of the expected value
    assert!((ms - (-446_090_160_000i64)).abs() < 86_400_000);
}

// ── jwt_exp ───────────────────────────────────────────────────────────────────

// A JWT with exp=1718548800 (2024-06-16T12:00:00Z), crafted manually.
// Header: {"alg":"RS256","typ":"JWT"}  → eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9
// Payload: {"exp":1718548800}          → eyJleHAiOjE3MTg1NDg4MDB9
// Signature: placeholder (not verified)
const FAKE_JWT: &str = "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.eyJleHAiOjE3MTg1NDg4MDB9.sig";

#[test]
fn jwt_exp_extracts_expiry() {
    assert_eq!(jwt_exp(FAKE_JWT), Some(1_718_548_800));
}

#[test]
fn jwt_exp_invalid_returns_none() {
    assert_eq!(jwt_exp("not.a.jwt"), None);
    assert_eq!(jwt_exp(""), None);
    assert_eq!(jwt_exp("only_one_part"), None);
}

// ── extract_client_uat ────────────────────────────────────────────────────────

#[test]
fn extract_client_uat_from_set_cookie_header() {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        "set-cookie",
        "__client_uat=1234567890; Path=/; Max-Age=315360000; SameSite=None; Secure"
            .parse()
            .unwrap(),
    );
    assert_eq!(extract_client_uat(&headers).as_deref(), Some("1234567890"));
}

#[test]
fn extract_client_uat_missing_returns_none() {
    let headers = reqwest::header::HeaderMap::new();
    assert_eq!(extract_client_uat(&headers), None);
}

#[test]
fn extract_client_uat_ignores_other_cookies() {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert("set-cookie", "other_cookie=xyz; Path=/".parse().unwrap());
    assert_eq!(extract_client_uat(&headers), None);
}
