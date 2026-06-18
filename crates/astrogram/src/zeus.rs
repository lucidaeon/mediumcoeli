//! Zeus `.zdb` chart-database text format.
//!
//! ## File structure
//!
//! Plain UTF-8 text. One chart record per line. Fields are semicolon-separated;
//! 16 fields per record (trailing `;` produces an empty 17th which is ignored):
//!
//! ```text
//! [0]  name           free text
//! [1]  chart_type     0-5 integer enum
//! [2]  date           DD.MM.YYYY or DD.MM.YYYYJc (Julian Calendar suffix)
//! [3]  time           HH:MM:SS (24-hour)
//! [4]  utc_offset     ±HH:MM:SS (East positive — already ISO 6709, no flip needed)
//! [5]  location       free text, may be empty
//! [6]  latitude       {N|S}{D+}.{MM}.{SS}
//! [7]  longitude      {E|W}{DDD}.{MM}.{SS}
//! [8]  sex            M / F / -
//! [9]  rodden_rating  AA / A / B / C / DD / X / XX
//! [10] rectified      '+' if rectified, otherwise empty (not mapped to Chart)
//! [11] notes          free text; || is paragraph separator (preserved as-is)
//! [12] unknown        always empty in observed records
//! [13] ref_id         numeric ADB/Wikipedia ID, not mapped to Chart
//! [14] flag           0 or 1, not mapped to Chart
//! [15] image          Windows file path or empty, not mapped to Chart
//! ```
//!
//! UTC offset is already East-positive (ISO 6709). No sign flip required.
//! Zeus does not store house system or zodiac; defaults (Placidus, Tropical,
//! Geocentric) are used when constructing [`Chart`].

use crate::capability::{CapabilitySet, ChartField};
use crate::chart::{Chart, CoordinateSystem, EventType, HouseSystem, Latitude, Longitude, Zodiac};
use crate::error::ParseError;

/// Fields recovered when reading a Zeus `.zdb` file.
///
/// Zeus stores source rating, notes, and event type directly.  House system,
/// zodiac, and coordinate system are not stored; the file always produces
/// Placidus / Tropical / Geocentric defaults and those values are dropped on
/// the return trip.
pub const READ_CAPS: CapabilitySet = CapabilitySet::new(&[
    ChartField::SourceRating,
    ChartField::Notes,
    ChartField::EventType,
]);

/// Fields persisted when writing a Zeus `.zdb` file. Identical to [`READ_CAPS`].
pub const WRITE_CAPS: CapabilitySet = READ_CAPS;

/// Parse a Zeus `.zdb` file into a vec of canonical charts.
///
/// Blank lines are skipped. The Julian Calendar suffix (`JC`) on dates is
/// stripped; the date is stored as given (the Chart type has no calendar flag).
/// The rectification flag (field 10) and fields 12-15 are not mapped to Chart.
///
/// # Errors
///
/// Returns [`ParseError::InvalidRecord`] if any non-empty line cannot be parsed.
pub fn parse_file(text: &str) -> Result<Vec<Chart>, ParseError> {
    let mut charts = Vec::new();
    for (idx, line) in text.lines().enumerate() {
        if line.is_empty() {
            continue;
        }
        charts.push(parse_record(line, idx + 1)?);
    }
    Ok(charts)
}

// --- internal ---

fn bad(line: usize, reason: impl Into<String>) -> ParseError {
    ParseError::InvalidRecord {
        line,
        reason: reason.into(),
    }
}

fn parse_record(line: &str, line_num: usize) -> Result<Chart, ParseError> {
    let fields: Vec<&str> = line.split(';').collect();
    if fields.len() < 16 {
        return Err(bad(
            line_num,
            format!("expected ≥16 fields, got {}", fields.len()),
        ));
    }

    let name = fields[0].to_string();
    let chart_type: u8 = fields[1]
        .parse()
        .map_err(|_| bad(line_num, format!("invalid chart_type {:?}", fields[1])))?;
    let (year, month, day) = parse_date(fields[2], line_num)?;
    let (hour, minute, second) = parse_time(fields[3], line_num)?;
    let tz_offset_hours = parse_utc_offset(fields[4], line_num)?;
    let city = non_empty(fields[5]);
    let latitude = parse_latitude(fields[6], line_num)?;
    let longitude = parse_longitude(fields[7], line_num)?;
    let sex = fields[8];
    let source_rating = non_empty(fields[9]);
    // fields[10] = rectification flag — not mapped
    let notes = non_empty(fields[11]);
    // fields[12-15] — not mapped

    Ok(Chart {
        name,
        secondary_name: None,
        city,
        region: None,
        longitude,
        latitude,
        year,
        month,
        day,
        hour,
        minute,
        second,
        tz_offset_hours,
        tz_abbreviation: None,
        is_lmt: false,
        event_type: map_event_type(chart_type, sex),
        source_rating,
        house_system: HouseSystem::Placidus,
        zodiac: Zodiac::Tropical,
        coordinate_system: CoordinateSystem::Geocentric,
        sub_charts: vec![],
        notes,
    })
}

fn non_empty(s: &str) -> Option<String> {
    if s.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}

fn map_event_type(chart_type: u8, sex: &str) -> EventType {
    match chart_type {
        2 => EventType::Horary,
        3..=5 => EventType::Event,
        _ => match sex {
            "M" => EventType::Male,
            "F" => EventType::Female,
            _ => EventType::Unspecified,
        },
    }
}

fn parse_date(s: &str, line: usize) -> Result<(i16, u8, u8), ParseError> {
    let s = s.trim_end_matches("JC");
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() != 3 {
        return Err(bad(line, format!("invalid date {s:?}")));
    }
    let day: u8 = parts[0]
        .parse()
        .map_err(|_| bad(line, format!("invalid day {:?}", parts[0])))?;
    let month: u8 = parts[1]
        .parse()
        .map_err(|_| bad(line, format!("invalid month {:?}", parts[1])))?;
    let year: i16 = parts[2]
        .parse()
        .map_err(|_| bad(line, format!("invalid year {:?}", parts[2])))?;
    Ok((year, month, day))
}

fn parse_time(s: &str, line: usize) -> Result<(u8, u8, u8), ParseError> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 3 {
        return Err(bad(line, format!("invalid time {s:?}")));
    }
    let hour: u8 = parts[0]
        .parse()
        .map_err(|_| bad(line, format!("invalid hour {:?}", parts[0])))?;
    let min: u8 = parts[1]
        .parse()
        .map_err(|_| bad(line, format!("invalid minute {:?}", parts[1])))?;
    let sec: u8 = parts[2]
        .parse()
        .map_err(|_| bad(line, format!("invalid second {:?}", parts[2])))?;
    Ok((hour, min, sec))
}

fn parse_utc_offset(s: &str, line: usize) -> Result<f64, ParseError> {
    if s.len() < 2 {
        return Err(bad(line, format!("invalid utc_offset {s:?}")));
    }
    let sign = if s.starts_with('-') {
        -1.0_f64
    } else {
        1.0_f64
    };
    let rest = &s[1..];
    let parts: Vec<&str> = rest.split(':').collect();
    if parts.len() != 3 {
        return Err(bad(line, format!("invalid utc_offset {s:?}")));
    }
    let h: f64 = parts[0]
        .parse()
        .map_err(|_| bad(line, format!("invalid offset hours {:?}", parts[0])))?;
    let m: f64 = parts[1]
        .parse()
        .map_err(|_| bad(line, format!("invalid offset minutes {:?}", parts[1])))?;
    let sec: f64 = parts[2]
        .parse()
        .map_err(|_| bad(line, format!("invalid offset seconds {:?}", parts[2])))?;
    Ok(sign * (h + m / 60.0 + sec / 3600.0))
}

fn parse_latitude(s: &str, line: usize) -> Result<Latitude, ParseError> {
    parse_coord(s, line, &['N', 'S']).and_then(|(deg, hemi)| {
        let signed = if hemi == 'S' { -deg } else { deg };
        Latitude::new(signed).map_err(|_| bad(line, format!("latitude {signed} out of range")))
    })
}

fn parse_longitude(s: &str, line: usize) -> Result<Longitude, ParseError> {
    parse_coord(s, line, &['E', 'W']).and_then(|(deg, hemi)| {
        let signed = if hemi == 'W' { -deg } else { deg };
        Longitude::new(signed).map_err(|_| bad(line, format!("longitude {signed} out of range")))
    })
}

fn parse_coord(s: &str, line: usize, hemis: &[char]) -> Result<(f64, char), ParseError> {
    let mut chars = s.chars();
    let hemi = chars
        .next()
        .ok_or_else(|| bad(line, format!("empty coordinate {s:?}")))?;
    if !hemis.contains(&hemi) {
        return Err(bad(
            line,
            format!("expected hemisphere in {hemis:?}, got {hemi:?}"),
        ));
    }
    let rest = &s[1..];
    let parts: Vec<&str> = rest.split('.').collect();
    if parts.len() != 3 {
        return Err(bad(line, format!("invalid coordinate {s:?}")));
    }
    let deg: f64 = parts[0]
        .parse()
        .map_err(|_| bad(line, format!("invalid coord degrees {:?}", parts[0])))?;
    let min: f64 = parts[1]
        .parse()
        .map_err(|_| bad(line, format!("invalid coord minutes {:?}", parts[1])))?;
    let sec: f64 = parts[2]
        .parse()
        .map_err(|_| bad(line, format!("invalid coord seconds {:?}", parts[2])))?;
    Ok((deg + min / 60.0 + sec / 3600.0, hemi))
}

// --- writer ---

/// Serialize a slice of canonical charts to Zeus `.zdb` text format.
///
/// Each chart becomes one semicolon-delimited line. Fields not representable
/// in Zeus (`secondary_name`, region, `tz_abbreviation`, `is_lmt`, `house_system`,
/// zodiac, `coordinate_system`, `sub_charts`) are silently dropped. The Julian
/// Calendar flag is not written; all dates are emitted as `DD.MM.YYYY`.
pub fn write_file(charts: &[Chart]) -> String {
    charts.iter().map(write_record).collect()
}

fn write_record(chart: &Chart) -> String {
    let (chart_type, sex) = unmap_event_type(chart.event_type);
    let date = format!("{:02}.{:02}.{:04}", chart.day, chart.month, chart.year);
    let time = format!("{:02}:{:02}:{:02}", chart.hour, chart.minute, chart.second);
    let utc = fmt_utc_offset(chart.tz_offset_hours);
    let city = chart.city.as_deref().unwrap_or("");
    let lat = fmt_latitude(chart.latitude);
    let lon = fmt_longitude(chart.longitude);
    let rating = chart.source_rating.as_deref().unwrap_or("");
    let notes = chart.notes.as_deref().unwrap_or("");
    // fields: name;chart_type;date;time;utc;city;lat;lon;sex;rating;rect;notes;;;;
    format!(
        "{};{};{};{};{};{};{};{};{};{};{};{};;;;\n",
        chart.name, chart_type, date, time, utc, city, lat, lon, sex, rating, "", notes
    )
}

fn unmap_event_type(et: EventType) -> (&'static str, &'static str) {
    match et {
        EventType::Male => ("1", "M"),
        EventType::Female => ("1", "F"),
        EventType::Horary => ("2", "-"),
        EventType::Event => ("3", "-"),
        EventType::Unspecified => ("0", "-"),
    }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn to_dms(degrees_abs: f64) -> (u32, u32, u32) {
    let total_sec = (degrees_abs * 3600.0).round() as u32;
    (total_sec / 3600, (total_sec % 3600) / 60, total_sec % 60)
}

fn fmt_latitude(lat: Latitude) -> String {
    let deg = lat.degrees();
    let hemi = if deg >= 0.0 { 'N' } else { 'S' };
    let (d, m, s) = to_dms(deg.abs());
    format!("{hemi}{d:02}.{m:02}.{s:02}")
}

fn fmt_longitude(lon: Longitude) -> String {
    let deg = lon.degrees();
    let hemi = if deg >= 0.0 { 'E' } else { 'W' };
    let (d, m, s) = to_dms(deg.abs());
    format!("{hemi}{d:03}.{m:02}.{s:02}")
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn fmt_utc_offset(hours: f64) -> String {
    let sign = if hours < 0.0 { '-' } else { '+' };
    let total_sec = (hours.abs() * 3600.0).round() as u32;
    let h = total_sec / 3600;
    let m = (total_sec % 3600) / 60;
    let s = total_sec % 60;
    format!("{sign}{h:02}:{m:02}:{s:02}")
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
        assert_eq!(got, declared, "zeus WRITE_CAPS disagrees with round-trip");
    }
}
