//! Solar Fire `.SFcht` binary chart-collection format.
//!
//! Sign conventions inside the file are **opposite** to ISO 6709 — Solar
//! Fire stores `+West` longitude and `+West` timezone offset. Negation to
//! canonical [`crate::chart::Chart`] conventions happens at the parse boundary.
//!
//! ## Fixed record layout (offsets within each chart record)
//!
//! ```text
//! +0    u16       marker (0x0101)
//! +2    char[50]  name (cp1252, space/NUL padded)
//! +52   char[20]  city
//! +72   char[20]  region
//! +92   f32 LE    longitude  (SF: +West; negated to ISO 6709 East+)
//! +96   f32 LE    latitude   (North positive, no flip)
//! +100  i16 LE    year       (signed for BCE)
//! +102  u8        month
//! +103  u8        day
//! +104  u8        hour
//! +105  u8        minute
//! +106  u8        second
//! +107  f32 LE    tz_offset  (SF: +West; negated to ISO 6709 East+)
//! +111  char[5]   tz_abbrev
//! +116  u8        is_lmt     (1 = LMT, 0 = named TZ)
//! +117  u8        event_type
//! +118  char[32]  source_rating
//! +151  u8        house_system
//! +152  u8        zodiac
//! +157  u8        coordinate_system
//! +158  u16 LE    record_index (1-based)
//! +162  char[50]  secondary_name
//! +292  u32 LE    sub_chart_count
//! ```
//! Followed by `sub_chart_count` sub-chart blocks, then a u32 notes length
//! and the notes bytes (cp1252).
//!
//! ## Sub-chart layout (115 bytes fixed, then notes)
//!
//! ```text
//! +0    char[50]  name
//! +50   char[20]  city
//! +70   char[20]  region
//! +90   f32 LE    longitude
//! +94   f32 LE    latitude
//! +98   i16 LE    year
//! +100  u8        month
//! +101  u8        day
//! +102  u8        hour
//! +103  u8        minute
//! +104  u8        second
//! +105  f32 LE    tz_offset
//! +109  char[5]   tz_abbrev
//! +114  u8        is_lmt
//! ```
//! Then u32 `notes_length` + notes bytes.

use crate::chart::{
    Chart, CoordinateSystem, EventType, HouseSystem, Latitude, Longitude, SubChart, Zodiac,
};
use crate::error::ParseError;
use std::borrow::Cow;

/// Size in bytes of the fixed `.SFcht` file header.
pub const HEADER_SIZE: usize = 86;
const MAIN_RECORD_SIZE: usize = 296;
const SUB_RECORD_SIZE: usize = 115;

/// Header of a `.SFcht` chart-collection file.
///
/// The fixed 86-byte block at the start of every file. All multi-byte
/// integers are little-endian. The trailing `u16` at offset +84 is always
/// `0` in observed files; it is consumed but not returned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    /// File format version (observed value: `3`).
    pub version: u16,
    /// File description text set by the user in Solar Fire. The on-disk field
    /// is 80 cp1252 bytes space-padded; trailing spaces are trimmed.
    pub description: String,
    /// Number of chart records that follow the header.
    pub record_count: u16,
}

/// Parse the 86-byte `.SFcht` file header.
///
/// # Errors
///
/// Returns [`ParseError::Truncated`] when `bytes.len() < HEADER_SIZE`.
pub fn parse_header(bytes: &[u8]) -> Result<Header, ParseError> {
    if bytes.len() < HEADER_SIZE {
        return Err(ParseError::Truncated {
            needed: HEADER_SIZE,
            got: bytes.len(),
        });
    }
    let version = u16::from_le_bytes([bytes[0], bytes[1]]);
    let description = decode_cp1252(&bytes[2..82]);
    let record_count = u16::from_le_bytes([bytes[82], bytes[83]]);
    Ok(Header {
        version,
        description,
        record_count,
    })
}

/// Parse a complete `.SFcht` file into a header and a vec of canonical charts.
///
/// Sign conventions are flipped at this boundary: Solar Fire's `+West`
/// longitude and `+West` `tz_offset` are negated to ISO 6709 `+East`.
///
/// # Errors
///
/// Returns [`ParseError`] on truncation, a bad record marker, or an
/// out-of-range coordinate.
pub fn parse_file(bytes: &[u8]) -> Result<(Header, Vec<Chart>), ParseError> {
    let header = parse_header(bytes)?;
    let mut charts = Vec::with_capacity(header.record_count as usize);
    let mut pos = HEADER_SIZE;
    for _ in 0..header.record_count {
        let (chart, consumed) = parse_chart_at(bytes, pos)?;
        charts.push(chart);
        pos += consumed;
    }
    Ok((header, charts))
}

// --- internal helpers ---

fn need(bytes: &[u8], pos: usize, n: usize) -> Result<(), ParseError> {
    if bytes.len() < pos + n {
        Err(ParseError::Truncated {
            needed: pos + n,
            got: bytes.len(),
        })
    } else {
        Ok(())
    }
}

fn decode_cp1252(raw: &[u8]) -> String {
    let stripped: Vec<u8> = raw.iter().copied().filter(|&b| b != 0).collect();
    let (decoded, _, _) = encoding_rs::WINDOWS_1252.decode(&stripped);
    decoded.trim_end_matches(' ').to_string()
}

fn opt_string(s: String) -> Option<String> {
    if s.is_empty() { None } else { Some(s) }
}

fn read_f32(bytes: &[u8], pos: usize) -> f32 {
    f32::from_le_bytes(bytes[pos..pos + 4].try_into().unwrap())
}

fn read_i16(bytes: &[u8], pos: usize) -> i16 {
    i16::from_le_bytes([bytes[pos], bytes[pos + 1]])
}

fn read_u16(bytes: &[u8], pos: usize) -> u16 {
    u16::from_le_bytes([bytes[pos], bytes[pos + 1]])
}

fn read_u32(bytes: &[u8], pos: usize) -> u32 {
    u32::from_le_bytes(bytes[pos..pos + 4].try_into().unwrap())
}

fn parse_notes(bytes: &[u8], pos: usize) -> Result<(Option<String>, usize), ParseError> {
    need(bytes, pos, 4)?;
    let len = read_u32(bytes, pos) as usize;
    need(bytes, pos + 4, len)?;
    let notes = if len > 0 {
        let raw: Vec<u8> = bytes[pos + 4..pos + 4 + len]
            .iter()
            .copied()
            .filter(|&b| b != 0)
            .collect();
        let (decoded, _, _) = encoding_rs::WINDOWS_1252.decode(&raw);
        opt_string(decoded.into_owned())
    } else {
        None
    };
    Ok((notes, 4 + len))
}

fn parse_sub_chart_at(bytes: &[u8], pos: usize) -> Result<(SubChart, usize), ParseError> {
    need(bytes, pos, SUB_RECORD_SIZE + 4)?;

    let lon_sf = read_f32(bytes, pos + 90);
    let lat_raw = read_f32(bytes, pos + 94);
    let tz_sf = read_f32(bytes, pos + 105);

    let longitude = Longitude::new(f64::from(-lon_sf))
        .map_err(|_| ParseError::CoordinateOutOfRange { offset: pos })?;
    let latitude = Latitude::new(f64::from(lat_raw))
        .map_err(|_| ParseError::CoordinateOutOfRange { offset: pos })?;

    let (notes, notes_consumed) = parse_notes(bytes, pos + SUB_RECORD_SIZE)?;

    let sc = SubChart {
        name: decode_cp1252(&bytes[pos..pos + 50]),
        city: opt_string(decode_cp1252(&bytes[pos + 50..pos + 70])),
        region: opt_string(decode_cp1252(&bytes[pos + 70..pos + 90])),
        longitude,
        latitude,
        year: read_i16(bytes, pos + 98),
        month: bytes[pos + 100],
        day: bytes[pos + 101],
        hour: bytes[pos + 102],
        minute: bytes[pos + 103],
        second: bytes[pos + 104],
        tz_offset_hours: f64::from(-tz_sf),
        tz_abbreviation: opt_string(decode_cp1252(&bytes[pos + 109..pos + 114])),
        is_lmt: bytes[pos + 114] == 1,
        notes,
    };

    Ok((sc, SUB_RECORD_SIZE + notes_consumed))
}

// --- writer ---

/// Write a vec of canonical charts to Solar Fire `.SFcht` binary format.
///
/// Sign conventions are flipped at this boundary: ISO 6709 `+East` longitude
/// and `+East` `tz_offset` are negated to Solar Fire's `+West` convention.
/// String fields that cannot be encoded in Windows-1252 have unmappable
/// characters replaced with `?`.
///
/// # Errors
///
/// Currently infallible; returns `Ok` always. The `Result` wrapper exists
/// for forwards-compatibility.
pub fn write_file(charts: &[Chart]) -> Result<Vec<u8>, ParseError> {
    write_file_with_description(charts, None)
}

/// Like [`write_file`] but honours an existing file description.
///
/// Pass the `description` field from the file's [`Header`] when overwriting an
/// existing file. Empty strings and values that start with `"Blackmoon "` are
/// treated as owned by Blackmoon and updated to the current version. Any other
/// value is preserved unchanged.
///
/// # Errors
///
/// Currently infallible; returns `Ok` always. The `Result` wrapper exists
/// for forwards-compatibility.
#[allow(clippy::cast_possible_truncation)]
pub fn write_file_with_description(
    charts: &[Chart],
    existing_description: Option<&str>,
) -> Result<Vec<u8>, ParseError> {
    let mut buf = vec![0u8; HEADER_SIZE];
    buf[0..2].copy_from_slice(&3u16.to_le_bytes()); // version = 3
    let blackmoon_desc = format!("Blackmoon {}", env!("CARGO_PKG_VERSION"));
    let desc_str = match existing_description {
        None | Some("") => blackmoon_desc.as_str(),
        Some(s) if s.starts_with("Blackmoon ") => blackmoon_desc.as_str(),
        Some(s) => s,
    };
    buf[2..82].copy_from_slice(&encode_cp1252_field(desc_str, 80));
    buf[82..84].copy_from_slice(&(charts.len() as u16).to_le_bytes());
    // +84: u16 trailing zero — already zeroed

    for (idx, chart) in charts.iter().enumerate() {
        encode_chart_into(&mut buf, chart, (idx + 1) as u16);
    }
    Ok(buf)
}

fn encode_cp1252_field(s: &str, field_len: usize) -> Vec<u8> {
    let (encoded, _, _) = encoding_rs::WINDOWS_1252.encode(s);
    let mut out = vec![0u8; field_len];
    let copy_len = encoded.len().min(field_len);
    out[..copy_len].copy_from_slice(&encoded[..copy_len]);
    out
}

#[allow(clippy::cast_possible_truncation)]
fn encode_notes_block(notes: Option<&str>) -> Vec<u8> {
    match notes {
        None => vec![0, 0, 0, 0],
        Some(text) => {
            let (encoded, _, _) = encoding_rs::WINDOWS_1252.encode(text);
            let len = encoded.len() as u32;
            let mut out = len.to_le_bytes().to_vec();
            out.extend_from_slice(match &encoded {
                Cow::Borrowed(b) => b,
                Cow::Owned(b) => b.as_slice(),
            });
            out
        }
    }
}

fn event_type_to_u8(et: EventType) -> u8 {
    match et {
        EventType::Male => 1,
        EventType::Female => 2,
        EventType::Event => 3,
        EventType::Horary => 4,
        EventType::Unspecified => 0,
    }
}

fn house_system_to_u8(hs: HouseSystem) -> u8 {
    match hs {
        HouseSystem::Campanus => 1,
        HouseSystem::Koch => 2,
        HouseSystem::Meridian => 3,
        HouseSystem::Morinus => 4,
        HouseSystem::Placidus => 5,
        HouseSystem::Porphyry => 6,
        HouseSystem::Regiomontanus => 7,
        HouseSystem::Topocentric => 8,
        HouseSystem::Equal => 9,
        HouseSystem::ZeroAries => 10,
        HouseSystem::SolarSign => 11,
        HouseSystem::WholeSign => 26,
        HouseSystem::HinduBhava => 27,
        HouseSystem::Alcabitius => 28,
        HouseSystem::Other(n) => n,
    }
}

fn zodiac_to_u8(z: Zodiac) -> u8 {
    match z {
        Zodiac::Tropical => 1,
        Zodiac::FaganAllen => 2,
        Zodiac::Lahiri => 3,
        Zodiac::DeLuce => 4,
        Zodiac::Raman => 5,
        Zodiac::UshaShashi => 6,
        Zodiac::Krishnamurti => 7,
        Zodiac::DjwhalKhul => 8,
        Zodiac::Draconic => 9,
        Zodiac::Svp => 10,
        Zodiac::SriYukteswar => 11,
        Zodiac::JnBhasin => 12,
        Zodiac::LarryEly => 13,
        Zodiac::TakraI => 14,
        Zodiac::TakraII => 15,
        Zodiac::SundaraRajan => 16,
        Zodiac::ShillPond => 17,
        Zodiac::Other(n) => n,
    }
}

fn coordinate_system_to_u8(cs: CoordinateSystem) -> u8 {
    match cs {
        CoordinateSystem::Geocentric => 1,
        CoordinateSystem::Heliocentric => 2,
    }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn encode_chart_into(buf: &mut Vec<u8>, chart: &Chart, record_idx: u16) {
    let base = buf.len();
    buf.resize(base + MAIN_RECORD_SIZE, 0);
    let rec = &mut buf[base..base + MAIN_RECORD_SIZE];

    rec[0..2].copy_from_slice(&0x0101u16.to_le_bytes());
    rec[2..52].copy_from_slice(&encode_cp1252_field(&chart.name, 50));
    rec[52..72].copy_from_slice(&encode_cp1252_field(
        chart.city.as_deref().unwrap_or(""),
        20,
    ));
    rec[72..92].copy_from_slice(&encode_cp1252_field(
        chart.region.as_deref().unwrap_or(""),
        20,
    ));
    rec[92..96].copy_from_slice(&((-chart.longitude.degrees()) as f32).to_le_bytes());
    rec[96..100].copy_from_slice(&(chart.latitude.degrees() as f32).to_le_bytes());
    rec[100..102].copy_from_slice(&chart.year.to_le_bytes());
    rec[102] = chart.month;
    rec[103] = chart.day;
    rec[104] = chart.hour;
    rec[105] = chart.minute;
    rec[106] = chart.second;
    rec[107..111].copy_from_slice(&((-chart.tz_offset_hours) as f32).to_le_bytes());
    rec[111..116].copy_from_slice(&encode_cp1252_field(
        chart.tz_abbreviation.as_deref().unwrap_or(""),
        5,
    ));
    rec[116] = u8::from(chart.is_lmt);
    rec[117] = event_type_to_u8(chart.event_type);
    rec[118..150].copy_from_slice(&encode_cp1252_field(
        chart.source_rating.as_deref().unwrap_or(""),
        32,
    ));
    // rec[150]: unknown — zero
    rec[151] = house_system_to_u8(chart.house_system);
    rec[152] = zodiac_to_u8(chart.zodiac);
    // rec[153..157]: unknown — zero
    rec[157] = coordinate_system_to_u8(chart.coordinate_system);
    rec[158..160].copy_from_slice(&record_idx.to_le_bytes());
    // rec[160..162]: unknown — zero
    rec[162..212].copy_from_slice(&encode_cp1252_field(
        chart.secondary_name.as_deref().unwrap_or(""),
        50,
    ));
    // rec[212..292]: unknown — zero
    rec[292..296].copy_from_slice(&(chart.sub_charts.len() as u32).to_le_bytes());

    for sub in &chart.sub_charts {
        encode_sub_chart_into(buf, sub);
    }
    buf.extend_from_slice(&encode_notes_block(chart.notes.as_deref()));
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn encode_sub_chart_into(buf: &mut Vec<u8>, sub: &SubChart) {
    let base = buf.len();
    buf.resize(base + SUB_RECORD_SIZE, 0);
    let rec = &mut buf[base..base + SUB_RECORD_SIZE];

    rec[0..50].copy_from_slice(&encode_cp1252_field(&sub.name, 50));
    rec[50..70].copy_from_slice(&encode_cp1252_field(sub.city.as_deref().unwrap_or(""), 20));
    rec[70..90].copy_from_slice(&encode_cp1252_field(
        sub.region.as_deref().unwrap_or(""),
        20,
    ));
    rec[90..94].copy_from_slice(&((-sub.longitude.degrees()) as f32).to_le_bytes());
    rec[94..98].copy_from_slice(&(sub.latitude.degrees() as f32).to_le_bytes());
    rec[98..100].copy_from_slice(&sub.year.to_le_bytes());
    rec[100] = sub.month;
    rec[101] = sub.day;
    rec[102] = sub.hour;
    rec[103] = sub.minute;
    rec[104] = sub.second;
    rec[105..109].copy_from_slice(&((-sub.tz_offset_hours) as f32).to_le_bytes());
    rec[109..114].copy_from_slice(&encode_cp1252_field(
        sub.tz_abbreviation.as_deref().unwrap_or(""),
        5,
    ));
    rec[114] = u8::from(sub.is_lmt);

    buf.extend_from_slice(&encode_notes_block(sub.notes.as_deref()));
}

fn parse_chart_at(bytes: &[u8], pos: usize) -> Result<(Chart, usize), ParseError> {
    need(bytes, pos, MAIN_RECORD_SIZE)?;

    let marker = read_u16(bytes, pos);
    if marker != 0x0101 {
        return Err(ParseError::BadMarker {
            offset: pos,
            got: marker,
        });
    }

    let lon_sf = read_f32(bytes, pos + 92);
    let lat_raw = read_f32(bytes, pos + 96);
    let tz_sf = read_f32(bytes, pos + 107);

    let longitude = Longitude::new(f64::from(-lon_sf))
        .map_err(|_| ParseError::CoordinateOutOfRange { offset: pos })?;
    let latitude = Latitude::new(f64::from(lat_raw))
        .map_err(|_| ParseError::CoordinateOutOfRange { offset: pos })?;

    let sub_chart_count = read_u32(bytes, pos + 292) as usize;

    let mut q = pos + MAIN_RECORD_SIZE;
    let mut sub_charts = Vec::with_capacity(sub_chart_count);
    for _ in 0..sub_chart_count {
        let (sc, consumed) = parse_sub_chart_at(bytes, q)?;
        sub_charts.push(sc);
        q += consumed;
    }

    let (notes, notes_consumed) = parse_notes(bytes, q)?;
    q += notes_consumed;

    let chart = Chart {
        name: decode_cp1252(&bytes[pos + 2..pos + 52]),
        secondary_name: opt_string(decode_cp1252(&bytes[pos + 162..pos + 212])),
        city: opt_string(decode_cp1252(&bytes[pos + 52..pos + 72])),
        region: opt_string(decode_cp1252(&bytes[pos + 72..pos + 92])),
        longitude,
        latitude,
        year: read_i16(bytes, pos + 100),
        month: bytes[pos + 102],
        day: bytes[pos + 103],
        hour: bytes[pos + 104],
        minute: bytes[pos + 105],
        second: bytes[pos + 106],
        tz_offset_hours: f64::from(-tz_sf),
        tz_abbreviation: opt_string(decode_cp1252(&bytes[pos + 111..pos + 116])),
        is_lmt: bytes[pos + 116] == 1,
        event_type: EventType::from(bytes[pos + 117]),
        source_rating: opt_string(decode_cp1252(&bytes[pos + 118..pos + 150])),
        house_system: HouseSystem::from(bytes[pos + 151]),
        zodiac: Zodiac::from(bytes[pos + 152]),
        coordinate_system: CoordinateSystem::from(bytes[pos + 157]),
        sub_charts,
        notes,
    };

    Ok((chart, q - pos))
}
