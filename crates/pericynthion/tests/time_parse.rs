use pericynthion::time::zone::Zone;
use pericynthion::time::{parse_date, parse_time, parse_tz, unix_to_utc};

// --- parse_date ---

#[test]
fn parse_date_standard() {
    assert_eq!(parse_date("2000-03-20").unwrap(), (2000, 3, 20));
}

#[test]
fn parse_date_bce() {
    assert_eq!(parse_date("-0044-03-15").unwrap(), (-44, 3, 15));
}

#[test]
fn parse_date_invalid_format() {
    assert!(parse_date("20000320").is_err());
}

#[test]
fn parse_date_month_out_of_range() {
    assert!(parse_date("2000-13-01").is_err());
}

#[test]
fn parse_date_day_out_of_range() {
    assert!(parse_date("2000-01-32").is_err());
}

// --- parse_time ---

#[test]
fn parse_time_hhmm() {
    assert_eq!(parse_time("14:30").unwrap(), (14, 30, 0.0));
}

#[test]
fn parse_time_hhmmss() {
    assert_eq!(parse_time("14:30:45").unwrap(), (14, 30, 45.0));
}

#[test]
fn parse_time_fractional_seconds() {
    let (h, m, s) = parse_time("14:30:45.5").unwrap();
    assert_eq!((h, m), (14, 30));
    assert!((s - 45.5).abs() < 1e-9);
}

#[test]
fn parse_time_hour_out_of_range() {
    assert!(parse_time("24:00").is_err());
}

#[test]
fn parse_time_minute_out_of_range() {
    assert!(parse_time("12:60").is_err());
}

#[test]
fn parse_time_missing_minutes() {
    assert!(parse_time("12").is_err());
}

// --- parse_tz ---

#[test]
fn parse_tz_plus_five_thirty() {
    // +05:30 = 5*3600 + 30*60 = 19800 seconds
    assert_eq!(parse_tz("+05:30").unwrap(), Zone::FixedSeconds(19800));
}

#[test]
fn parse_tz_minus_five() {
    assert_eq!(parse_tz("-05:00").unwrap(), Zone::FixedSeconds(-18000));
}

#[test]
fn parse_tz_with_seconds() {
    // -00:09:21 = -(9*60 + 21) = -561 seconds
    assert_eq!(parse_tz("-00:09:21").unwrap(), Zone::FixedSeconds(-561));
}

#[test]
fn parse_tz_zero() {
    assert_eq!(parse_tz("+00:00").unwrap(), Zone::FixedSeconds(0));
}

#[test]
fn parse_tz_no_sign_defaults_positive() {
    assert_eq!(parse_tz("01:00").unwrap(), Zone::FixedSeconds(3600));
}

#[test]
fn parse_tz_invalid() {
    assert!(parse_tz("bad").is_err());
}

// --- unix_to_utc ---

#[test]
fn unix_epoch_is_1970_01_01() {
    assert_eq!(unix_to_utc(0), (1970, 1, 1, 0, 0, 0));
}

#[test]
fn unix_known_datetime() {
    // 2000-01-01T00:00:00Z = 946684800
    assert_eq!(unix_to_utc(946_684_800), (2000, 1, 1, 0, 0, 0));
}

#[test]
fn unix_with_time_component() {
    // 2000-01-01T13:30:45Z = 946684800 + 13*3600 + 30*60 + 45 = 946733445
    assert_eq!(unix_to_utc(946_733_445), (2000, 1, 1, 13, 30, 45));
}
