use crate::capability::{CapabilitySet, ChartField};
use crate::chart::{Chart, CoordinateSystem, EventType, HouseSystem, Latitude, Longitude, Zodiac};
use thiserror::Error;

/// Fields recovered when reading an AAF file.
///
/// AAF carries a region sub-locality (the 7th comma-separated field of the
/// A-row).  Event type and source rating are not stored.
pub const READ_CAPS: CapabilitySet = CapabilitySet::new(&[ChartField::Region]);

/// AAF is a read-only format; nothing is written.
pub const WRITE_CAPS: CapabilitySet = CapabilitySet::new(&[]);

/// Errors that can arise while parsing AAF data.
#[derive(Debug, Error)]
pub enum AafError {
    /// A `#A93:`/`#B93:` pair failed to parse; `pair` is the 0-based index.
    #[error("AAF pair {pair}: {reason}")]
    BadPair {
        /// 0-based pair index.
        pair: usize,
        /// Human-readable explanation.
        reason: String,
    },

    /// A `#A93:` line was not followed by a `#B93:` line.
    #[error("AAF #A93: line without following #B93: near: {context}")]
    MissingB {
        /// Excerpt of the offending line for diagnostics.
        context: String,
    },
}

fn bad(pair: usize, reason: impl Into<String>) -> AafError {
    AafError::BadPair {
        pair,
        reason: reason.into(),
    }
}

/// Parse an AAF text blob (e.g. extracted from an astro.com `<pre>` block)
/// into a vector of charts.  Handles both the Astrolog standard dialect and
/// the astro.com dialect, which uses lowercase hemisphere
/// letters (`n`/`s`/`e`/`w`) and may differ in timezone format.
///
/// # Errors
/// - [`AafError::MissingB`] if a `#A93:` line has no following `#B93:` line.
/// - [`AafError::BadPair`] if a `#A93:`/`#B93:` pair fails to parse.
pub fn parse_file(text: &str) -> Result<Vec<Chart>, AafError> {
    let mut charts = Vec::new();
    let mut lines = text.lines().peekable();
    let mut pair_idx = 0usize;

    while let Some(line) = lines.next() {
        let line = line.trim();
        let Some(a_data) = line.strip_prefix("#A93:") else {
            continue;
        };

        let b_data = loop {
            match lines.next() {
                None => break None,
                Some(l) => {
                    let l = l.trim();
                    if let Some(rest) = l.strip_prefix("#B93:") {
                        break Some(rest.to_string());
                    }
                }
            }
        };

        let b_data = b_data.ok_or_else(|| AafError::MissingB {
            context: a_data.chars().take(60).collect(),
        })?;

        charts.push(parse_pair(pair_idx, a_data, &b_data)?);
        pair_idx += 1;
    }

    Ok(charts)
}

fn parse_pair(pair: usize, a: &str, b: &str) -> Result<Chart, AafError> {
    // Two A-row dialects both use 7 comma-separated fields, but differ in position[0]:
    //
    // Standard Astrolog:  *,Last,First,DD.MM.YYYY,HH:MM,City,Country
    // astro.com (split):  Last,First,Gender,DD.MM.YYYY,HH:MM,City,Country
    //
    // Detect by whether field[0] is the literal "*".
    // In the astro.com variant, gender (m/f/e) occupies field[2] and date is at field[3]
    // regardless of variant — so the date offset stays constant.
    let a_fields: Vec<&str> = a.splitn(7, ',').collect();
    if a_fields.len() < 6 {
        return Err(bad(
            pair,
            format!("A row: expected ≥6 fields, got {}", a_fields.len()),
        ));
    }

    let (last_name, first_name) = if a_fields[0].trim() == "*" {
        // Standard / astro.com single-name format: *, Last, First-or-Gender, date, ...
        let last = a_fields[1].trim().replace(';', ",");
        let first = if a_fields.len() > 2 {
            let raw = a_fields[2].trim();
            // astro.com stores gender code in the first-name slot when there is no given name.
            if matches!(raw, "m" | "f" | "e" | "M" | "F" | "E") {
                String::new()
            } else {
                raw.replace(';', ",")
            }
        } else {
            String::new()
        };
        (last, first)
    } else {
        // astro.com two-name format: Last, First, Gender, date, ...
        let last = a_fields[0].trim().replace(';', ",");
        let first = a_fields[1].trim().replace(';', ",");
        // field[2] is the gender code — discard, used only for ssx elsewhere.
        (last, first)
    };

    let name = match (first_name.is_empty(), last_name.is_empty()) {
        (true, _) => last_name.clone(),
        (_, true) => first_name.clone(),
        (false, false) => format!("{last_name}, {first_name}"),
    };

    let (day, month, year) = parse_date(pair, a_fields[3].trim())?;
    let (hour, minute, second) = parse_time(pair, a_fields[4].trim())?;

    let city = opt_str(if a_fields.len() > 5 {
        a_fields[5].trim().replace(';', ",")
    } else {
        String::new()
    });
    let region = opt_str(if a_fields.len() > 6 {
        a_fields[6].trim().replace(';', ",")
    } else {
        String::new()
    });

    // B row: JulianDay,Lat,Lon,Zone,DST
    let b_fields: Vec<&str> = b.splitn(5, ',').collect();
    if b_fields.len() < 5 {
        return Err(bad(
            pair,
            format!("B row: expected 5 fields, got {}", b_fields.len()),
        ));
    }
    // b_fields[0] = Julian Day — ignored; calendar date from A row is authoritative.
    let latitude = parse_lat(pair, b_fields[1].trim())?;
    let longitude = parse_lon(pair, b_fields[2].trim())?;
    let (tz_offset_hours, is_lmt) =
        parse_zone(pair, b_fields[3].trim(), b_fields[4].trim(), longitude)?;

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
        tz_abbreviation: None,
        is_lmt,
        event_type: EventType::Unspecified,
        source_rating: None,
        // AAF does not store house system or zodiac — use sensible defaults.
        house_system: HouseSystem::Placidus,
        zodiac: Zodiac::Tropical,
        coordinate_system: CoordinateSystem::Geocentric,
        sub_charts: Vec::new(),
        notes: None,
    })
}

fn opt_str(s: String) -> Option<String> {
    if s.is_empty() { None } else { Some(s) }
}

fn parse_date(pair: usize, s: &str) -> Result<(u8, u8, i16), AafError> {
    let p: Vec<&str> = s.splitn(3, '.').collect();
    if p.len() != 3 {
        return Err(bad(pair, format!("date: expected DD.MM.YYYY, got '{s}'")));
    }
    let day = p[0]
        .parse::<u8>()
        .map_err(|_| bad(pair, format!("day in '{s}'")))?;
    let month = p[1]
        .parse::<u8>()
        .map_err(|_| bad(pair, format!("month in '{s}'")))?;
    // astro.com appends calendar-system suffixes to ancient dates, e.g. "120g" (120 CE).
    let year_str = p[2].trim_end_matches(|c: char| c.is_alphabetic());
    let year = year_str
        .parse::<i16>()
        .map_err(|_| bad(pair, format!("year in '{s}'")))?;
    Ok((day, month, year))
}

fn parse_time(pair: usize, s: &str) -> Result<(u8, u8, u8), AafError> {
    let p: Vec<&str> = s.splitn(3, ':').collect();
    if p.len() < 2 {
        return Err(bad(pair, format!("time: expected HH:MM, got '{s}'")));
    }
    let hour = p[0]
        .parse::<u8>()
        .map_err(|_| bad(pair, format!("hour in '{s}'")))?;
    let minute = p[1]
        .parse::<u8>()
        .map_err(|_| bad(pair, format!("minute in '{s}'")))?;
    let second = if p.len() > 2 {
        p[2].parse().unwrap_or(0)
    } else {
        0
    };
    Ok((hour, minute, second))
}

fn parse_lat(pair: usize, s: &str) -> Result<Latitude, AafError> {
    let (mag, is_north) = parse_coord(pair, s, "NS")?;
    let val = mag * if is_north { 1.0 } else { -1.0 };
    Latitude::new(val).map_err(|_| bad(pair, format!("latitude out of range ({val}) from '{s}'")))
}

fn parse_lon(pair: usize, s: &str) -> Result<Longitude, AafError> {
    let (mag, is_east) = parse_coord(pair, s, "EW")?;
    let val = mag * if is_east { 1.0 } else { -1.0 };
    Longitude::new(val).map_err(|_| bad(pair, format!("longitude out of range ({val}) from '{s}'")))
}

/// Parse `DDDhMM` or `DDDhMM:SS` where `h` is one of the two chars in `pos_neg`
/// (first = positive hemisphere, second = negative).  Case-insensitive.
///
/// Returns `(magnitude_decimal_degrees, is_positive_hemisphere)`.
fn parse_coord(pair: usize, s: &str, pos_neg: &str) -> Result<(f64, bool), AafError> {
    let pos_char = pos_neg.chars().next().unwrap().to_ascii_uppercase();
    let neg_char = pos_neg.chars().nth(1).unwrap().to_ascii_uppercase();

    let (sep_byte, is_pos) = s
        .char_indices()
        .find_map(|(i, c)| {
            let u = c.to_ascii_uppercase();
            if u == pos_char {
                Some((i, true))
            } else if u == neg_char {
                Some((i, false))
            } else {
                None
            }
        })
        .ok_or_else(|| {
            bad(
                pair,
                format!("coord '{s}': no hemisphere letter ({pos_neg})"),
            )
        })?;

    let deg: f64 = s[..sep_byte]
        .parse()
        .map_err(|_| bad(pair, format!("coord '{s}': bad degrees")))?;
    let rest = &s[sep_byte + 1..]; // all hemisphere chars are ASCII → 1 byte

    let frac = if rest.is_empty() {
        0.0
    } else if let Some(colon) = rest.find(':') {
        let min: f64 = rest[..colon]
            .parse()
            .map_err(|_| bad(pair, format!("coord '{s}': bad minutes")))?;
        let sec: f64 = rest[colon + 1..].parse().unwrap_or(0.0);
        min / 60.0 + sec / 3600.0
    } else {
        let min: f64 = rest
            .parse()
            .map_err(|_| bad(pair, format!("coord '{s}': bad minutes")))?;
        min / 60.0
    };

    Ok((deg + frac, is_pos))
}

/// Parse the zone and DST fields into `(tz_offset_hours, is_lmt)`.
///
/// Zone: `8W`, `5E30`, `*` (LMT sentinel)
/// DST:  `D` (+1 h), `L` (LMT), `0` (none), numeric
fn parse_zone(
    pair: usize,
    zone: &str,
    dst: &str,
    longitude: Longitude,
) -> Result<(f64, bool), AafError> {
    let dst_upper = dst.trim().to_ascii_uppercase();

    if dst_upper == "L" || zone == "*" {
        return Ok((longitude.degrees() / 15.0, true));
    }

    let offset = parse_zone_offset(pair, zone)?;
    let dst_hours = match dst_upper.as_str() {
        "D" => 1.0,
        "0" | "" => 0.0,
        _ => dst.trim().parse::<f64>().unwrap_or(0.0),
    };

    Ok((offset + dst_hours, false))
}

fn parse_zone_offset(pair: usize, zone: &str) -> Result<f64, AafError> {
    let zu = zone.to_ascii_uppercase();
    if zu.is_empty() || zu == "0" {
        return Ok(0.0);
    }
    // astro.com dialect: "7hw00" (hours + 'h' separator + hemisphere + mins)
    // standard Astrolog:  "7W00"  (hours + hemisphere + mins)
    // Strip trailing 'h'/'H' from the degrees portion before parsing.
    if let Some(pos) = zu.find('E') {
        let hrs: f64 = zone[..pos]
            .trim_end_matches(['h', 'H'])
            .parse()
            .map_err(|_| bad(pair, format!("zone '{zone}': bad hours")))?;
        let mins: f64 = if zone[pos + 1..].is_empty() {
            0.0
        } else {
            zone[pos + 1..]
                .parse()
                .map_err(|_| bad(pair, format!("zone '{zone}': bad minutes")))?
        };
        return Ok(hrs + mins / 60.0);
    }
    if let Some(pos) = zu.find('W') {
        let hrs: f64 = zone[..pos]
            .trim_end_matches(['h', 'H'])
            .parse()
            .map_err(|_| bad(pair, format!("zone '{zone}': bad hours")))?;
        let mins: f64 = if zone[pos + 1..].is_empty() {
            0.0
        } else {
            zone[pos + 1..]
                .parse()
                .map_err(|_| bad(pair, format!("zone '{zone}': bad minutes")))?
        };
        return Ok(-(hrs + mins / 60.0));
    }
    zone.parse::<f64>()
        .map_err(|_| bad(pair, format!("zone '{zone}': unrecognised format")))
}
