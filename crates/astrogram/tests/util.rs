//! Tests for `astrogram::util` — timestamp helpers.

use astrogram::util::{expand_now, utc_timestamp_from_secs};

#[test]
fn unix_epoch_formats() {
    assert_eq!(utc_timestamp_from_secs(0), "19700101T000000Z");
}

#[test]
fn y2k_formats() {
    assert_eq!(utc_timestamp_from_secs(946_684_800), "20000101T000000Z");
}

#[test]
fn known_datetime_formats() {
    // 2026-06-06 19:30:45 UTC
    assert_eq!(utc_timestamp_from_secs(1_780_774_245), "20260606T193045Z");
}

#[test]
fn leap_day_formats() {
    // 2000-02-29 12:00:00 UTC
    assert_eq!(utc_timestamp_from_secs(951_825_600), "20000229T120000Z");
}

// ── expand_now ────────────────────────────────────────────────────────────────

#[test]
fn expand_now_sfcht() {
    let p = expand_now(std::path::Path::new("now.SFcht"), 946_684_800);
    assert_eq!(
        p,
        std::path::PathBuf::from("blackmoon.20000101T000000Z.SFcht")
    );
}

#[test]
fn expand_now_zdb() {
    let p = expand_now(std::path::Path::new("now.zdb"), 946_684_800);
    assert_eq!(
        p,
        std::path::PathBuf::from("blackmoon.20000101T000000Z.zdb")
    );
}

#[test]
fn expand_now_xml() {
    let p = expand_now(std::path::Path::new("now.xml"), 946_684_800);
    assert_eq!(
        p,
        std::path::PathBuf::from("blackmoon.20000101T000000Z.xml")
    );
}

#[test]
fn expand_now_with_path_prefix() {
    let p = expand_now(std::path::Path::new("/tmp/now.SFcht"), 946_684_800);
    assert_eq!(
        p,
        std::path::PathBuf::from("/tmp/blackmoon.20000101T000000Z.SFcht")
    );
}

#[test]
fn expand_now_non_now_unchanged() {
    let p = expand_now(std::path::Path::new("myfile.SFcht"), 946_684_800);
    assert_eq!(p, std::path::PathBuf::from("myfile.SFcht"));
}

#[test]
fn expand_now_no_extension_unchanged() {
    let p = expand_now(std::path::Path::new("now"), 946_684_800);
    assert_eq!(p, std::path::PathBuf::from("now"));
}
