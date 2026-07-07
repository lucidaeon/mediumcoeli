//! Astrodatabank XML reader (`export_format` 160715).
//!
//! ## Coordinate encoding
//!
//! Latitude:  `{DD}{n|s}{MM}` or `{DD}{n|s}{MMSS}`  — e.g. `45n42`, `52n0445`
//! Longitude: `{DDD}{e|w}{MM}` or `{DDD}{e|w}{MMSS}` — e.g. `0w20`, `122w1959`
//!
//! ## Timezone offset
//!
//! Derived from `jd_ut` (Julian Day UT) and the local time stored in
//! `sbtime.value`.  When `jd_ut` is absent, `stmerid` is parsed instead.
//! `stmerid` encodes either a longitude meridian (`m{deg}{E|W}{min}`) or a
//! standard timezone offset (`h{hours}{E|W}{min}`).
//!
//! ## Defaults
//!
//! ADB exports do not carry house system or zodiac; all charts default to
//! Placidus / Tropical / Geocentric.

use crate::capability::{CapabilitySet, ChartField};
use crate::chart::{Chart, CoordinateSystem, EventType, HouseSystem, Latitude, Longitude, Zodiac};
use crate::error::ParseError;
use roxmltree::{Document, Node};

/// Fields recovered when reading an Astrodatabank XML export.
///
/// `SourceRating` is excluded: the ADB format maps ratings through a numeric
/// `rrc` code, so a free-text rating like `"AA Himself to Astrolabe"` is
/// normalised to `"AA"` on the return trip and does not survive unmodified.
/// Zodiac and coordinate system are not stored; the file always produces
/// Tropical / Geocentric defaults and those values are dropped on the return trip.
pub const READ_CAPS: CapabilitySet =
    CapabilitySet::new(&[ChartField::Region, ChartField::Notes, ChartField::EventType]);

/// Fields persisted when writing an Astrodatabank XML export. Identical to [`READ_CAPS`].
pub const WRITE_CAPS: CapabilitySet = READ_CAPS;

/// Parse an ADB XML export into a vec of canonical charts.
///
/// Each `<adb_entry>` becomes one [`Chart`].  The primary birth data
/// (`public_data/bdata`) is used; alternative birth times (`bdata_alt`)
/// are silently ignored.
///
/// # Errors
///
/// Returns [`ParseError::Xml`] if the document is malformed XML.
/// Returns [`ParseError::AdbEntry`] if a required field is absent from an
/// otherwise well-formed entry.
pub fn parse_file(xml: &str) -> Result<Vec<Chart>, ParseError> {
    let doc = Document::parse(xml).map_err(|e| ParseError::Xml(e.to_string()))?;
    let root = doc.root_element();
    let mut charts = Vec::new();
    for node in root.children() {
        if node.has_tag_name("adb_entry") {
            let raw_id = node
                .attribute("adb_id")
                .ok_or_else(|| ParseError::Xml("adb_entry missing adb_id".to_string()))?;
            let adb_id: u32 = raw_id
                .parse()
                .map_err(|_| ParseError::Xml(format!("invalid adb_id {raw_id:?}")))?;
            charts.push(parse_entry(node, adb_id)?);
        }
    }
    Ok(charts)
}

// ── entry ─────────────────────────────────────────────────────────────────────

fn parse_entry(node: Node, adb_id: u32) -> Result<Chart, ParseError> {
    let pub_data = child(node, "public_data").ok_or_else(|| bad(adb_id, "missing public_data"))?;

    let name = child_text(pub_data, "name")
        .ok_or_else(|| bad(adb_id, "missing name"))?
        .to_string();

    let csex = child(pub_data, "gender")
        .and_then(|n| n.attribute("csex"))
        .unwrap_or("");
    let event_type = event_type_from_csex(csex);

    let rrc: u32 = child(pub_data, "roddenrating")
        .and_then(|n| n.attribute("rrc"))
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let source_rating = rodden_rating(rrc);

    let bdata = child(pub_data, "bdata").ok_or_else(|| bad(adb_id, "missing bdata"))?;

    // Date
    let sbdate = child(bdata, "sbdate").ok_or_else(|| bad(adb_id, "missing sbdate"))?;
    let year: i16 = sbdate
        .attribute("iyear")
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| bad(adb_id, "missing iyear"))?;
    let month: u8 = sbdate
        .attribute("imonth")
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| bad(adb_id, "missing imonth"))?;
    let day: u8 = sbdate
        .attribute("iday")
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| bad(adb_id, "missing iday"))?;

    // Time + timezone
    let (hour, minute, second, tz_offset_hours, tz_abbreviation, is_lmt) =
        if let Some(t) = child(bdata, "sbtime") {
            // ADB stores "unknown, 12:00 used" when birth time is unknown.
            let time_unknown = t.attribute("time_unknown").is_some_and(|s| s == "yes");
            let (hh, mm, ss) = if time_unknown {
                (12u8, 0u8, 0u8)
            } else {
                let time_str = t.text().unwrap_or("00:00").trim();
                parse_time_hhmm(time_str, adb_id)?
            };
            let jd_ut: Option<f64> = t.attribute("jd_ut").and_then(|s| s.parse().ok());
            let stmerid = t.attribute("stmerid").unwrap_or("");
            let ctimetype = t.attribute("ctimetype").unwrap_or("");
            let tz_off = tz_offset(jd_ut, stmerid, hh, mm);
            let tz_abbr = t
                .attribute("sznabbr")
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string);
            (hh, mm, ss, tz_off, tz_abbr, ctimetype == "l")
        } else {
            (0u8, 0u8, 0u8, 0.0f64, None, false)
        };

    // Coordinates
    let place = child(bdata, "place").ok_or_else(|| bad(adb_id, "missing place"))?;
    let lat_str = place
        .attribute("slati")
        .ok_or_else(|| bad(adb_id, "missing slati"))?;
    let lon_str = place
        .attribute("slong")
        .ok_or_else(|| bad(adb_id, "missing slong"))?;
    let latitude = parse_lat(lat_str, adb_id)?;
    let longitude = parse_lon(lon_str, adb_id)?;
    let city = place.text().filter(|s| !s.is_empty()).map(str::to_string);
    let region = child(bdata, "country")
        .and_then(|n| n.text())
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    // Notes from sourcenotes
    let notes = child(node, "text_data")
        .and_then(|td| child(td, "sourcenotes"))
        .and_then(|sn| sn.text())
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    Ok(Chart {
        name,
        secondary_name: None,
        city,
        region,
        longitude,
        latitude,
        year,
        month,
        day,
        hour,
        minute,
        second,
        tz_offset_hours,
        tz_abbreviation,
        is_lmt,
        event_type,
        source_rating,
        house_system: HouseSystem::Placidus,
        zodiac: Zodiac::Tropical,
        coordinate_system: CoordinateSystem::Geocentric,
        sub_charts: vec![],
        notes,
    })
}

// ── coordinate parsing ────────────────────────────────────────────────────────

fn parse_lat(s: &str, adb_id: u32) -> Result<Latitude, ParseError> {
    let (deg, hem) = parse_coord(s, &['n', 's'], adb_id)?;
    let signed = if hem == 's' { -deg } else { deg };
    Latitude::new(signed).map_err(|_| bad(adb_id, format!("latitude {signed} out of range")))
}

fn parse_lon(s: &str, adb_id: u32) -> Result<Longitude, ParseError> {
    let (deg, hem) = parse_coord(s, &['e', 'w'], adb_id)?;
    let signed = if hem == 'w' { -deg } else { deg };
    // ADB occasionally encodes Aleutian longitudes as >180°W (e.g. 182w3040).
    // Wrap into [-180, 180] by adding 360° when negative out-of-range.
    let normalized = if signed < -180.0 {
        signed + 360.0
    } else if signed > 180.0 {
        signed - 360.0
    } else {
        signed
    };
    Longitude::new(normalized)
        .map_err(|_| bad(adb_id, format!("longitude {normalized} out of range")))
}

fn parse_coord(s: &str, hems: &[char], adb_id: u32) -> Result<(f64, char), ParseError> {
    let hem_pos = s
        .find(|c: char| hems.contains(&c))
        .ok_or_else(|| bad(adb_id, format!("no hemisphere marker in coord {s:?}")))?;
    let deg: f64 = s[..hem_pos]
        .parse()
        .map_err(|_| bad(adb_id, format!("invalid degrees in coord {s:?}")))?;
    let hem = s.as_bytes()[hem_pos] as char;
    let raw = &s[hem_pos + 1..];
    let frac = parse_minsec_digits(raw).ok_or_else(|| {
        bad(
            adb_id,
            format!("invalid minsec digits {raw:?} in coord {s:?}"),
        )
    })?;
    Ok((deg + frac, hem))
}

// Parse 2-digit (MM) or 4-digit (MMSS) trailing field into fractional degrees/hours.
// Returns `None` when the digits cannot be parsed as numbers.
fn parse_minsec_digits(s: &str) -> Option<f64> {
    match s.len() {
        0 => Some(0.0),
        4 => {
            let min: f64 = s[..2].parse().ok()?;
            let sec: f64 = s[2..4].parse().ok()?;
            Some(min / 60.0 + sec / 3600.0)
        }
        // 2-digit (minutes only) and any unexpected length: minutes
        _ => Some(s.parse::<f64>().ok()? / 60.0),
    }
}

// ── timezone ──────────────────────────────────────────────────────────────────

fn tz_offset(jd_ut: Option<f64>, stmerid: &str, hh: u8, mm: u8) -> f64 {
    if let Some(jd) = jd_ut {
        // JD epoch is noon; add 0.5 to shift to midnight-based fraction.
        let ut_frac = ((jd + 0.5).fract() + 1.0) % 1.0;
        let ut_hours = ut_frac * 24.0;
        let local_hours = f64::from(hh) + f64::from(mm) / 60.0;
        let diff = local_hours - ut_hours;
        // Normalize to (-12, 12].
        if diff > 12.0 {
            diff - 24.0
        } else if diff <= -12.0 {
            diff + 24.0
        } else {
            diff
        }
    } else {
        parse_stmerid(stmerid)
    }
}

// Parse an ADB meridian code into an hour offset (East positive).
//
// Format:
//   m{deg}{e|w}{min[sec]}  — longitude meridian; offset = (deg + frac) / 15
//   h{hours}{e|w}{min}     — standard timezone; offset = hours + frac
//
// Examples: "m0w20" → -0.0222h, "h7w" → -7h, "h0e45" → +0.75h, "m37e35" → +2.472h
fn parse_stmerid(s: &str) -> f64 {
    let s = s.trim();
    if s.is_empty() {
        return 0.0;
    }
    let (scale, rest) = if let Some(r) = s.strip_prefix('m') {
        (1.0_f64 / 15.0, r)
    } else if let Some(r) = s.strip_prefix('h') {
        (1.0_f64, r)
    } else {
        return 0.0;
    };
    let Some(dir_pos) = rest.find(['e', 'w']) else {
        return 0.0;
    };
    // `.unwrap_or(0.0)` is deliberate: `stmerid` is a timezone-offset hint,
    // not a coordinate. A malformed meridian code defaults to UTC (0.0) rather
    // than hard-erroring, consistent with the `jd_ut`-absent fallback path.
    // This is exempt from the hard-error policy that applies to coordinates.
    let major: f64 = rest[..dir_pos].parse().unwrap_or(0.0);
    let dir = rest.as_bytes()[dir_pos] as char;
    let frac = parse_minsec_digits(&rest[dir_pos + 1..]).unwrap_or(0.0);
    let value = (major + frac) * scale;
    if dir == 'w' { -value } else { value }
}

// ── time ──────────────────────────────────────────────────────────────────────

// Parse "HH:MM" or "HH:MM:SS" — seconds are optional.
fn parse_time_hhmm(s: &str, adb_id: u32) -> Result<(u8, u8, u8), ParseError> {
    let mut parts = s.splitn(3, ':');
    let hh: u8 = parts
        .next()
        .and_then(|p| p.trim().parse().ok())
        .ok_or_else(|| bad(adb_id, format!("invalid time {s:?}")))?;
    let mm: u8 = parts
        .next()
        .and_then(|p| p.trim().parse().ok())
        .ok_or_else(|| bad(adb_id, format!("missing minutes in time {s:?}")))?;
    let ss: u8 = parts
        .next()
        .and_then(|p| p.trim().parse().ok())
        .unwrap_or(0);
    Ok((hh, mm, ss))
}

// ── mappings ──────────────────────────────────────────────────────────────────

fn event_type_from_csex(csex: &str) -> EventType {
    match csex {
        "m" => EventType::Male,
        "f" => EventType::Female,
        _ => EventType::Unspecified,
    }
}

fn rodden_rating(rrc: u32) -> Option<String> {
    let s = match rrc {
        1 => "AA",
        2 => "A",
        3 => "B",
        4 => "C",
        5 => "DD",
        6 => "X",
        7 => "XX",
        _ => return None,
    };
    Some(s.to_string())
}

// ── tree helpers ──────────────────────────────────────────────────────────────

fn child<'a, 'b>(parent: Node<'a, 'b>, tag: &str) -> Option<Node<'a, 'b>> {
    parent.children().find(|n| n.has_tag_name(tag))
}

fn child_text<'a>(parent: Node<'a, '_>, tag: &str) -> Option<&'a str> {
    child(parent, tag).and_then(|n| n.text())
}

fn bad(adb_id: u32, reason: impl Into<String>) -> ParseError {
    ParseError::AdbEntry {
        adb_id,
        reason: reason.into(),
    }
}

// ── writer ────────────────────────────────────────────────────────────────────

/// Serialize a slice of canonical charts to ADB XML (`export_format` 160715).
///
/// Each chart receives a local-origin `adb_id` starting at `100_000_001`
/// (values above `100_000_000` are reserved for local entries per ADB spec).
/// All dates are written as Gregorian (`ccalendar="g"`).  Fields with no ADB
/// equivalent (`house_system`, `zodiac`, `sub_charts`) are silently dropped.
/// Planetary positions are omitted — they are computed server-side by astro.com
/// and are not stored in [`Chart`].
#[must_use]
pub fn write_file(charts: &[Chart]) -> String {
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\n");
    out.push_str("<astrodatabank_export export_format=\"160715\">\n");
    for (i, chart) in charts.iter().enumerate() {
        #[allow(clippy::cast_possible_truncation)]
        let adb_id = 100_000_001u32 + i as u32;
        out.push_str(&write_entry(adb_id, chart));
    }
    out.push_str("</astrodatabank_export>\n");
    out
}

fn write_entry(adb_id: u32, chart: &Chart) -> String {
    let name = xml_escape(&chart.name);
    let csex = csex_from_event_type(chart.event_type);
    let gvalue = match csex {
        "m" => "M",
        "f" => "F",
        _ => "N/A",
    };
    let rrc = rrc_from_source_rating(chart.source_rating.as_deref());
    let rating = rodden_rating(rrc).unwrap_or_else(|| "X".to_string());

    let jd = compute_jd_ut(chart);
    let ctimetype = if chart.is_lmt { "l" } else { "s" };
    let sznabbr = xml_escape(chart.tz_abbreviation.as_deref().unwrap_or(""));
    let time_str = format!("{:02}:{:02}:{:02}", chart.hour, chart.minute, chart.second);

    let slati = coord_to_adb(chart.latitude.degrees(), 'n', 's');
    let slong = coord_to_adb(chart.longitude.degrees(), 'e', 'w');
    let city = xml_escape(chart.city.as_deref().unwrap_or(""));
    let country = xml_escape(chart.region.as_deref().unwrap_or(""));

    let date_val = format!("{:04}/{:02}/{:02}", chart.year, chart.month, chart.day);

    let mut s = format!(
        "  <adb_entry adb_id=\"{adb_id}\">\n\
         \x20   <public_data>\n\
         \x20     <name>{name}</name>\n\
         \x20     <gender csex=\"{csex}\">{gvalue}</gender>\n\
         \x20     <roddenrating rrc=\"{rrc}\">{rating}</roddenrating>\n\
         \x20     <datatype sdatatype=\"Public Figure\" dtc=\"1\" />\n\
         \x20     <bdata>\n\
         \x20       <sbdate ccalendar=\"g\" iyear=\"{y}\" imonth=\"{mo}\" iday=\"{d}\">{date_val}</sbdate>\n\
         \x20       <sbtime ctimetype=\"{ctimetype}\" jd_ut=\"{jd:.6}\" sznabbr=\"{sznabbr}\">{time_str}</sbtime>\n\
         \x20       <place slati=\"{slati}\" slong=\"{slong}\">{city}</place>\n\
         \x20       <country>{country}</country>\n\
         \x20     </bdata>\n\
         \x20   </public_data>\n",
        y = chart.year,
        mo = chart.month,
        d = chart.day,
    );

    if let Some(notes) = &chart.notes {
        use std::fmt::Write as _;
        let _ = write!(
            s,
            "    <text_data>\n      <sourcenotes>{}</sourcenotes>\n    </text_data>\n",
            xml_escape(notes)
        );
    }

    s.push_str("  </adb_entry>\n\n");
    s
}

// ── writer helpers ────────────────────────────────────────────────────────────

// Days since Unix epoch (1970-01-01 = 0) for a proleptic Gregorian date.
// Hinnant civil_from_days inverse. All intermediate values are non-negative
// after subtracting the era offset, so the u64 casts are safe.
#[allow(clippy::cast_sign_loss, clippy::cast_possible_wrap)]
fn days_from_civil(y: i32, m: u32, d: u32) -> i64 {
    let y = if m <= 2 {
        i64::from(y) - 1
    } else {
        i64::from(y)
    };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u64; // 0..=399 — safe
    let m_adj = if m <= 2 { m + 9 } else { m - 3 };
    let doy = (153 * u64::from(m_adj) + 2) / 5 + u64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe as i64 - 719_468 // doe ≤ 146096 — safe
}

#[allow(clippy::cast_precision_loss)]
fn compute_jd_ut(chart: &Chart) -> f64 {
    let days = days_from_civil(
        i32::from(chart.year),
        u32::from(chart.month),
        u32::from(chart.day),
    );
    let local_h =
        f64::from(chart.hour) + f64::from(chart.minute) / 60.0 + f64::from(chart.second) / 3600.0;
    let ut_h = local_h - chart.tz_offset_hours;
    // JD 2440588.0 = 1970-01-01 12:00 UT; days fits f64 for any historical date
    2_440_588.0 + days as f64 + (ut_h - 12.0) / 24.0
}

// Decimal degrees → ADB coordinate string  e.g. 45.7 → "45n42", -0.333 → "0w20"
fn coord_to_adb(degrees: f64, pos: char, neg: char) -> String {
    let hemi = if degrees >= 0.0 { pos } else { neg };
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let total_sec = (degrees.abs() * 3600.0).round() as u32;
    let deg = total_sec / 3600;
    let min = (total_sec % 3600) / 60;
    let sec = total_sec % 60;
    if sec == 0 {
        format!("{deg}{hemi}{min:02}")
    } else {
        format!("{deg}{hemi}{min:02}{sec:02}")
    }
}

fn csex_from_event_type(et: EventType) -> &'static str {
    match et {
        EventType::Male => "m",
        EventType::Female => "f",
        _ => "e",
    }
}

fn rrc_from_source_rating(rating: Option<&str>) -> u32 {
    let s = match rating {
        Some(s) => s.trim(),
        None => return 6,
    };
    // Exact match first, then prefix (handles "AA BC in hand" style strings)
    match s {
        "AA" => 1,
        "A" => 2,
        "B" => 3,
        "C" => 4,
        "DD" => 5,
        "X" => 6,
        "XX" => 7,
        _ if s.starts_with("AA") => 1,
        _ if s.starts_with("XX") => 7,
        _ if s.starts_with("DD") => 5,
        _ if s.starts_with('A') => 2,
        _ if s.starts_with('B') => 3,
        _ if s.starts_with('C') => 4,
        _ if s.starts_with('X') => 6,
        _ => 6,
    }
}

fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

// ── unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::float_cmp, clippy::unreadable_literal)]
mod tests {
    use super::*;

    // --- parse_minsec_digits ---

    #[test]
    fn minsec_empty_is_zero() {
        assert_eq!(parse_minsec_digits(""), Some(0.0));
    }

    #[test]
    fn minsec_two_digits_is_minutes() {
        // "42" → 42/60 = 0.7
        let v = parse_minsec_digits("42").unwrap();
        assert!((v - 42.0 / 60.0).abs() < 1e-9);
    }

    #[test]
    fn minsec_four_digits_is_min_sec() {
        // "0445" → 04' 45" = 4/60 + 45/3600
        let v = parse_minsec_digits("0445").unwrap();
        let expected = 4.0 / 60.0 + 45.0 / 3600.0;
        assert!((v - expected).abs() < 1e-9);
    }

    #[test]
    fn minsec_garbage_is_none() {
        assert_eq!(parse_minsec_digits("XX"), None);
        assert_eq!(parse_minsec_digits("XXYY"), None);
    }

    // --- parse_stmerid ---

    #[test]
    fn stmerid_empty_is_zero() {
        assert_eq!(parse_stmerid(""), 0.0);
    }

    #[test]
    fn stmerid_lmt_west() {
        // "m0w20" → 0°20' W → -(0+20/60)/15 = -0.02222h
        let v = parse_stmerid("m0w20");
        let expected = -(20.0 / 60.0) / 15.0;
        assert!((v - expected).abs() < 1e-6);
    }

    #[test]
    fn stmerid_standard_east_hours_and_minutes() {
        // "h0e45" → +0.75h
        let v = parse_stmerid("h0e45");
        assert!((v - 0.75).abs() < 1e-6);
    }

    #[test]
    fn stmerid_standard_west_hours_only() {
        // "h7w" → -7h
        let v = parse_stmerid("h7w");
        assert!((v - (-7.0)).abs() < 1e-6);
    }

    #[test]
    fn stmerid_gmt() {
        // "h0e" → 0h
        assert_eq!(parse_stmerid("h0e"), 0.0);
    }

    #[test]
    fn stmerid_lmt_with_seconds() {
        // "m16e2223" → 16°22'23" E → (16 + 22/60 + 23/3600) / 15
        let v = parse_stmerid("m16e2223");
        let expected = (16.0 + 22.0 / 60.0 + 23.0 / 3600.0) / 15.0;
        assert!((v - expected).abs() < 1e-6);
    }

    // --- tz_offset ---

    #[test]
    fn tz_offset_zero_when_local_equals_ut() {
        // JD 2440588.0 = 1970-01-01 12:00 UT; local 12:00 → offset 0
        let off = tz_offset(Some(2440588.0), "", 12, 0);
        assert!(off.abs() < 1e-6);
    }

    #[test]
    fn tz_offset_positive_one_hour() {
        // local 12:00, UT 11:00 → +1h
        // JD for UT 11:00 on 1970-01-01: 2440588 - 1/24 = 2440587.958333
        let off = tz_offset(Some(2440587.958_333), "", 12, 0);
        assert!((off - 1.0).abs() < 1e-3);
    }

    #[test]
    fn tz_offset_falls_back_to_stmerid_when_no_jd_ut() {
        // No jd_ut, stmerid = "h5e" → +5h
        let off = tz_offset(None, "h5e", 12, 0);
        assert!((off - 5.0).abs() < 1e-6);
    }

    #[test]
    fn tz_offset_normalizes_past_midnight() {
        // Local 01:00, UT 23:00 the day before → diff = 1 - 23 = -22 → normalize → +2h
        // JD for UT 23:00 on day D: JD_noon_D + 11/24
        let jd = 2440588.0 + 11.0 / 24.0; // noon + 11h = 23h UT
        let off = tz_offset(Some(jd), "", 1, 0);
        assert!((off - 2.0).abs() < 1e-3);
    }

    // --- parse_lat / parse_lon ---

    #[test]
    fn lat_north_degrees_minutes() {
        let lat = parse_lat("45n42", 0).unwrap();
        assert!((lat.degrees() - (45.0 + 42.0 / 60.0)).abs() < 1e-9);
    }

    #[test]
    fn lat_south_degrees_minutes() {
        let lat = parse_lat("33s52", 0).unwrap();
        assert!((lat.degrees() - (-(33.0 + 52.0 / 60.0))).abs() < 1e-9);
    }

    #[test]
    fn lat_with_seconds() {
        let lat = parse_lat("52n0445", 0).unwrap();
        let expected = 52.0 + 4.0 / 60.0 + 45.0 / 3600.0;
        assert!((lat.degrees() - expected).abs() < 1e-9);
    }

    #[test]
    fn lon_east_degrees_minutes() {
        let lon = parse_lon("2e20", 0).unwrap();
        assert!((lon.degrees() - (2.0 + 20.0 / 60.0)).abs() < 1e-9);
    }

    #[test]
    fn lon_west_degrees_minutes() {
        let lon = parse_lon("0w20", 0).unwrap();
        assert!((lon.degrees() - (-(20.0 / 60.0))).abs() < 1e-9);
    }

    #[test]
    fn lon_west_with_seconds() {
        let lon = parse_lon("122w1959", 0).unwrap();
        let expected = -(122.0 + 19.0 / 60.0 + 59.0 / 3600.0);
        assert!((lon.degrees() - expected).abs() < 1e-9);
    }

    #[test]
    fn lon_zero_east() {
        let lon = parse_lon("0e00", 0).unwrap();
        assert_eq!(lon.degrees(), 0.0);
    }

    // --- coord_to_adb ---

    #[test]
    fn coord_north_degrees_minutes() {
        // 45°42' N = 45.7°
        assert_eq!(coord_to_adb(45.7, 'n', 's'), "45n42");
    }

    #[test]
    fn coord_south() {
        // 33°52' S = -33.8667°
        assert_eq!(coord_to_adb(-(33.0 + 52.0 / 60.0), 'n', 's'), "33s52");
    }

    #[test]
    fn coord_west_zero_degrees() {
        // 0°20' W = -0.3333°
        assert_eq!(coord_to_adb(-(20.0 / 60.0), 'e', 'w'), "0w20");
    }

    #[test]
    fn coord_east_zero() {
        assert_eq!(coord_to_adb(0.0, 'e', 'w'), "0e00");
    }

    #[test]
    fn coord_with_seconds() {
        // 52°04'45" N = 52 + 4/60 + 45/3600
        let deg = 52.0 + 4.0 / 60.0 + 45.0 / 3600.0;
        assert_eq!(coord_to_adb(deg, 'n', 's'), "52n0445");
    }

    // --- compute_jd_ut ---

    #[test]
    fn jd_ut_unix_epoch_noon() {
        // 1970-01-01 12:00:00 UTC → JD 2440588.0
        use crate::chart::{
            Chart, CoordinateSystem, EventType, HouseSystem, Latitude, Longitude, Zodiac,
        };
        let c = Chart {
            name: String::new(),
            secondary_name: None,
            city: None,
            region: None,
            longitude: Longitude::new(0.0).unwrap(),
            latitude: Latitude::new(0.0).unwrap(),
            year: 1970,
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
        };
        assert!((compute_jd_ut(&c) - 2_440_588.0).abs() < 1e-6);
    }

    #[test]
    fn jd_ut_j2000() {
        // 2000-01-01 12:00:00 UTC → JD 2451545.0
        use crate::chart::{
            Chart, CoordinateSystem, EventType, HouseSystem, Latitude, Longitude, Zodiac,
        };
        let c = Chart {
            name: String::new(),
            secondary_name: None,
            city: None,
            region: None,
            longitude: Longitude::new(0.0).unwrap(),
            latitude: Latitude::new(0.0).unwrap(),
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
        };
        assert!((compute_jd_ut(&c) - 2_451_545.0).abs() < 1e-6);
    }

    // --- rrc_from_source_rating ---

    #[test]
    fn rrc_maps_standard_codes() {
        assert_eq!(rrc_from_source_rating(Some("AA")), 1);
        assert_eq!(rrc_from_source_rating(Some("A")), 2);
        assert_eq!(rrc_from_source_rating(Some("B")), 3);
        assert_eq!(rrc_from_source_rating(Some("C")), 4);
        assert_eq!(rrc_from_source_rating(Some("DD")), 5);
        assert_eq!(rrc_from_source_rating(Some("X")), 6);
        assert_eq!(rrc_from_source_rating(Some("XX")), 7);
    }

    #[test]
    fn rrc_prefix_matches_combined_strings() {
        assert_eq!(rrc_from_source_rating(Some("AA BC in hand")), 1);
        assert_eq!(rrc_from_source_rating(Some("B Bio/autobiography")), 3);
    }

    #[test]
    fn rrc_unknown_defaults_to_x() {
        assert_eq!(rrc_from_source_rating(None), 6);
        assert_eq!(rrc_from_source_rating(Some("?")), 6);
    }

    // --- parse errors: garbage minsec and bad adb_id ---

    #[test]
    fn parse_file_garbage_minsec_errors_with_adb_id_and_raw_text() {
        // Latitude "45nXX" has unparseable minute digits "XX".
        // parse_file must return AdbEntry { adb_id: 42, reason: mentions "XX" }.
        let xml = concat!(
            r#"<?xml version="1.0" encoding="utf-8"?>"#,
            "\n<astrodatabank_export export_format=\"160715\">\n",
            "  <adb_entry adb_id=\"42\">\n",
            "    <public_data>\n",
            "      <name>Garbage Lat</name>\n",
            "      <bdata>\n",
            "        <sbdate iyear=\"2000\" imonth=\"1\" iday=\"1\"/>\n",
            "        <sbtime jd_ut=\"2451544.5\">12:00</sbtime>\n",
            "        <place slati=\"45nXX\" slong=\"2e20\"/>\n",
            "      </bdata>\n",
            "    </public_data>\n",
            "  </adb_entry>\n",
            "</astrodatabank_export>\n",
        );
        let err = parse_file(xml).expect_err("expected error for garbage minsec");
        match err {
            ParseError::AdbEntry { adb_id, ref reason } => {
                assert_eq!(adb_id, 42, "adb_id must be 42");
                assert!(
                    reason.contains("XX"),
                    "reason must name the garbage text; got: {reason}"
                );
            }
            other => panic!("expected ParseError::AdbEntry, got {other:?}"),
        }
    }

    #[test]
    fn parse_file_missing_adb_id_attribute_errors() {
        // An <adb_entry> with no adb_id attribute must produce ParseError::Xml
        // containing "missing adb_id" — the missing-attribute path in parse_file.
        let xml = concat!(
            r#"<?xml version="1.0" encoding="utf-8"?>"#,
            "\n<astrodatabank_export export_format=\"160715\">\n",
            "  <adb_entry>\n",
            "    <public_data>\n",
            "      <name>No Id</name>\n",
            "      <bdata>\n",
            "        <sbdate iyear=\"2000\" imonth=\"1\" iday=\"1\"/>\n",
            "        <sbtime jd_ut=\"2451544.5\">12:00</sbtime>\n",
            "        <place slati=\"45n42\" slong=\"2e20\"/>\n",
            "      </bdata>\n",
            "    </public_data>\n",
            "  </adb_entry>\n",
            "</astrodatabank_export>\n",
        );
        let err = parse_file(xml).expect_err("expected error for missing adb_id attribute");
        match err {
            ParseError::Xml(ref msg) => {
                assert!(
                    msg.contains("missing adb_id"),
                    "error must mention 'missing adb_id'; got: {msg}"
                );
            }
            other => panic!("expected ParseError::Xml, got {other:?}"),
        }
    }

    #[test]
    fn parse_file_bad_adb_id_attribute_errors() {
        // adb_id="notanumber" cannot be parsed as u32; must not silently become 0.
        let xml = concat!(
            r#"<?xml version="1.0" encoding="utf-8"?>"#,
            "\n<astrodatabank_export export_format=\"160715\">\n",
            "  <adb_entry adb_id=\"notanumber\">\n",
            "    <public_data>\n",
            "      <name>Bad Id</name>\n",
            "      <bdata>\n",
            "        <sbdate iyear=\"2000\" imonth=\"1\" iday=\"1\"/>\n",
            "        <sbtime jd_ut=\"2451544.5\">12:00</sbtime>\n",
            "        <place slati=\"45n42\" slong=\"2e20\"/>\n",
            "      </bdata>\n",
            "    </public_data>\n",
            "  </adb_entry>\n",
            "</astrodatabank_export>\n",
        );
        let err = parse_file(xml).expect_err("expected error for bad adb_id");
        match err {
            ParseError::Xml(ref msg) => {
                assert!(
                    msg.contains("notanumber"),
                    "error must mention the raw id text; got: {msg}"
                );
            }
            other => panic!("expected ParseError::Xml, got {other:?}"),
        }
    }

    // --- xml_escape ---

    #[test]
    fn xml_escape_ampersand() {
        assert_eq!(xml_escape("a & b"), "a &amp; b");
    }

    #[test]
    fn xml_escape_lt_gt() {
        assert_eq!(xml_escape("<tag>"), "&lt;tag&gt;");
    }

    #[test]
    fn xml_escape_quote() {
        assert_eq!(xml_escape("say \"hi\""), "say &quot;hi&quot;");
    }

    #[test]
    fn xml_escape_plain_text_unchanged() {
        assert_eq!(xml_escape("hello world"), "hello world");
    }
}

#[cfg(test)]
mod cap_roundtrip {
    use super::*;
    use crate::capability::ChartField;
    use crate::test_support::{fully_populated, survivors};

    #[test]
    fn write_caps_match_roundtrip() {
        let original = fully_populated();
        let text = write_file(std::slice::from_ref(&original));
        let restored = parse_file(&text).expect("parse");
        let restored = &restored[0];
        let mut got = survivors(&original, restored);
        got.sort_by_key(|f| format!("{f:?}"));
        let mut declared: Vec<ChartField> = WRITE_CAPS.fields().to_vec();
        declared.sort_by_key(|f| format!("{f:?}"));
        assert_eq!(got, declared, "adbxml WRITE_CAPS disagrees with round-trip");
    }
}
