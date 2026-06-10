//! Geographic coordinate parsing: decimal degrees (DD), degrees decimal
//! minutes (DDM), and degrees-minutes-seconds (DMS), with input
//! sanitisation for common encoding variants and pasted-from-UI debris.
//!
//! All three formats accepted by [`parse_geo_coord`]:
//!
//! | Format | Example |
//! |--------|---------|
//! | DD  | `52.8166118,-1.3281652` |
//! | DDM | `41° 53.296566' N, 87° 37.13841' W` |
//! | DMS | `34° 8' 20" N, 118° 21' 9" W` |
//!
//! When the `°`/`'`/`"` markers are omitted, whitespace works as a
//! separator: `34 8 20 N` (DMS), `34 8.333 N` (DDM), `34.14` (DD).
//!
//! See ISO 6709 for the canonical definition of these formats.

use thiserror::Error;

/// Decimal-degree geographic coordinate pair.
///
/// * `lat` — latitude in degrees north (negative = south). Range: −90..+90.
/// * `lon` — longitude in degrees east (negative = west). Range: −180..+180.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GeoCoord {
    /// Latitude, degrees north. Negative = south.
    pub lat: f64,
    /// Longitude, degrees east. Negative = west.
    pub lon: f64,
}

/// Errors returned by geographic coordinate parsing.
#[derive(Debug, Error, PartialEq)]
pub enum GeoError {
    /// The string could not be interpreted as a coordinate.
    #[error("cannot parse coordinate: {0}")]
    Parse(String),
    /// Latitude is outside −90..+90.
    #[error("latitude {0} out of range (−90 to +90)")]
    LatitudeRange(f64),
    /// Longitude is outside −180..+180.
    #[error("longitude {0} out of range (−180 to +180)")]
    LongitudeRange(f64),
}

/// Parse a geographic coordinate pair from a free-form string.
///
/// Accepts all three common formats (DD, DDM, DMS). Latitude must appear
/// first. Hemisphere indicators (N/S/E/W) must trail their component.
/// A plain numeric leading sign (−/+) is accepted in lieu of N/S/E/W.
///
/// Input is sanitised before parsing: degree-symbol lookalikes (˚),
/// smart quotes (`'`, `"`), backtick-as-arcminute, and two consecutive
/// apostrophes-as-arcsecond are all normalised to their canonical forms.
///
/// # Examples
///
/// ```
/// use pericynthion::geo::parse_geo_coord;
/// // DD
/// let c = parse_geo_coord("52.8166118,-1.3281652").unwrap();
/// assert!((c.lat - 52.8166118).abs() < 1e-6);
/// assert!((c.lon - (-1.3281652)).abs() < 1e-6);
///
/// // DMS — no-comma hemisphere-separated form
/// let c = parse_geo_coord("36° 12' 23\" N 36° 9' 25\" E").unwrap();
/// assert!((c.lat - (36.0 + 12.0/60.0 + 23.0/3600.0)).abs() < 1e-4);
/// ```
///
/// # Errors
///
/// Returns `GeoError` if the input cannot be parsed or values are out of range.
pub fn parse_geo_coord(s: &str) -> Result<GeoCoord, GeoError> {
    let normed = normalize(s);
    let (lat_s, lon_s) = split_lat_lon(&normed)?;
    let lat = parse_component(&lat_s)?;
    let lon = parse_component(&lon_s)?;
    if !(-90.0..=90.0).contains(&lat) {
        return Err(GeoError::LatitudeRange(lat));
    }
    if !(-180.0..=180.0).contains(&lon) {
        return Err(GeoError::LongitudeRange(lon));
    }
    Ok(GeoCoord { lat, lon })
}

/// Parse a single latitude value from a free-form string.
///
/// Accepts the same format variants as [`parse_geo_coord`] but for a
/// single component. Useful for CLI `--lat`-style arguments.
///
/// ```
/// use pericynthion::geo::parse_lat;
/// assert!((parse_lat("34.14").unwrap() - 34.14).abs() < 1e-9);
/// assert!((parse_lat("51° 30' 26\" N").unwrap() - (51.0 + 30.0/60.0 + 26.0/3600.0)).abs() < 1e-4);
/// ```
///
/// # Errors
///
/// Returns `GeoError` if the input cannot be parsed or value is out of range.
pub fn parse_lat(s: &str) -> Result<f64, GeoError> {
    let normed = normalize(s);
    let lat = parse_component(normed.trim())?;
    if !(-90.0..=90.0).contains(&lat) {
        return Err(GeoError::LatitudeRange(lat));
    }
    Ok(lat)
}

/// Parse a single longitude value from a free-form string.
///
/// Accepts the same format variants as [`parse_geo_coord`] but for a
/// single component. Useful for CLI `--lon`-style arguments where
/// only the east-of-Greenwich longitude is required.
///
/// ```
/// use pericynthion::geo::parse_lon;
/// assert!((parse_lon("-1.3281652").unwrap() - (-1.3281652)).abs() < 1e-9);
/// assert!((parse_lon("36° 9' 25\" E").unwrap() - (36.0 + 9.0/60.0 + 25.0/3600.0)).abs() < 1e-4);
/// ```
///
/// # Errors
///
/// Returns `GeoError` if the input cannot be parsed or value is out of range.
pub fn parse_lon(s: &str) -> Result<f64, GeoError> {
    let normed = normalize(s);
    let lon = parse_component(normed.trim())?;
    if !(-180.0..=180.0).contains(&lon) {
        return Err(GeoError::LongitudeRange(lon));
    }
    Ok(lon)
}

// =============================================================================
// Internal helpers
// =============================================================================

/// Normalise encoding variants to standard ASCII/Unicode.
///
/// * Two consecutive apostrophes (`''`) → `"`
/// * Arcminute variants (U+2019, U+02BC, backtick) → `'`
/// * Arcsecond variants (U+201D, U+02BA) → `"`
/// * Ring-above U+02DA → `°`
fn normalize(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            // Two consecutive apostrophes → arcsecond marker
            '\'' if chars.peek() == Some(&'\'') => {
                chars.next();
                out.push('"');
            }
            // Arcminute variants → standard apostrophe
            '\'' | '\u{2019}' | '\u{02BC}' | '`' => out.push('\''),
            // Arcsecond variants → standard double-quote
            '"' | '\u{201D}' | '\u{02BA}' => out.push('"'),
            // Degree-mark lookalikes: U+02DA RING ABOVE, U+00BA MASCULINE ORDINAL INDICATOR
            '\u{02DA}' | '\u{00BA}' => out.push('°'),
            c => out.push(c),
        }
    }
    out
}

/// Split a normalised coordinate string into (latitude, longitude) halves.
///
/// Strategy 1: split on the first comma.
/// Strategy 2: split after the first trailing N/S hemisphere indicator
/// that is followed (after optional whitespace) by a digit or sign.
fn split_lat_lon(s: &str) -> Result<(String, String), GeoError> {
    if let Some(pos) = s.find(',') {
        return Ok((s[..pos].to_string(), s[pos + 1..].to_string()));
    }

    for (byte_pos, c) in s.char_indices() {
        if c != 'N' && c != 'S' {
            continue;
        }
        let before = s[..byte_pos].trim_end();
        let prev = before.chars().last().unwrap_or('\0');
        if !prev.is_ascii_digit() && !matches!(prev, '\'' | '"' | '°') {
            continue;
        }
        let after = s[byte_pos + c.len_utf8()..].trim_start();
        let first = after.chars().next().unwrap_or('\0');
        if first.is_ascii_digit() || matches!(first, '+' | '-') {
            return Ok((s[..byte_pos + c.len_utf8()].to_string(), after.to_string()));
        }
    }

    Err(GeoError::Parse(format!("cannot split into lat/lon: {s:?}")))
}

/// Parse one coordinate component to decimal degrees.
///
/// Handles leading sign (−/+) and trailing hemisphere indicator (N/S/E/W).
fn parse_component(s: &str) -> Result<f64, GeoError> {
    let s = s.trim();

    let (s, leading_neg) = if let Some(rest) = s.strip_prefix('-') {
        (rest.trim_start(), true)
    } else if let Some(rest) = s.strip_prefix('+') {
        (rest.trim_start(), false)
    } else {
        (s, false)
    };

    let last = s.chars().last().unwrap_or('\0');
    let (s, hemi) = if matches!(last, 'N' | 'S' | 'E' | 'W') {
        (s[..s.len() - last.len_utf8()].trim_end(), Some(last))
    } else {
        (s, None)
    };

    let magnitude = parse_numeric_part(s)?;

    let sign = match hemi {
        Some('S' | 'W') => -1.0,
        _ => {
            if leading_neg {
                -1.0
            } else {
                1.0
            }
        }
    };

    Ok(magnitude * sign)
}

/// Extract the numeric magnitude from a coordinate component, handling
/// DD (no `°`), DDM (`°` then `'`), and DMS (`°`, `'`, `"`).
///
/// When no `°` marker is present, accepts whitespace-separated forms too:
/// a single token is DD, two tokens are DDM (`deg dec_min`), three tokens
/// are DMS (`deg min sec`).
fn parse_numeric_part(s: &str) -> Result<f64, GeoError> {
    let s = s.trim();

    let Some(deg_end) = s.find('°') else {
        return parse_marker_free(s);
    };

    let deg: f64 = s[..deg_end]
        .trim()
        .parse()
        .map_err(|_| GeoError::Parse(format!("bad degree value in {s:?}")))?;
    let rest = s[deg_end + '°'.len_utf8()..].trim();

    if rest.is_empty() {
        return Ok(deg);
    }

    let min_end = rest.find('\'').ok_or_else(|| {
        GeoError::Parse(format!(
            "expected arcminute separator after degrees in {s:?}"
        ))
    })?;
    let min: f64 = rest[..min_end]
        .trim()
        .parse()
        .map_err(|_| GeoError::Parse(format!("bad arcminute value in {s:?}")))?;
    let after_min = rest[min_end + 1..].trim();

    if after_min.is_empty() {
        return Ok(deg + min / 60.0); // DDM
    }

    let sec_end = after_min
        .find('"')
        .ok_or_else(|| GeoError::Parse(format!("expected arcsecond separator in {s:?}")))?;
    let sec: f64 = after_min[..sec_end]
        .trim()
        .parse()
        .map_err(|_| GeoError::Parse(format!("bad arcsecond value in {s:?}")))?;

    Ok(deg + min / 60.0 + sec / 3600.0) // DMS
}

/// Parse a coordinate component with no `°`/`'`/`"` markers.
///
/// One numeric token → DD. Two tokens → DDM (deg, decimal-minutes).
/// Three tokens → DMS (deg, minutes, seconds).
fn parse_marker_free(s: &str) -> Result<f64, GeoError> {
    let tokens: Vec<&str> = s.split_whitespace().collect();
    match tokens.as_slice() {
        [dd] => dd
            .parse::<f64>()
            .map_err(|_| GeoError::Parse(format!("cannot parse as decimal degrees: {s:?}"))),
        [d, m] => {
            let deg: f64 = d
                .parse()
                .map_err(|_| GeoError::Parse(format!("bad degree value in {s:?}")))?;
            let min: f64 = m
                .parse()
                .map_err(|_| GeoError::Parse(format!("bad arcminute value in {s:?}")))?;
            Ok(deg + min / 60.0)
        }
        [d, m, sec] => {
            let deg: f64 = d
                .parse()
                .map_err(|_| GeoError::Parse(format!("bad degree value in {s:?}")))?;
            let min: f64 = m
                .parse()
                .map_err(|_| GeoError::Parse(format!("bad arcminute value in {s:?}")))?;
            let sec: f64 = sec
                .parse()
                .map_err(|_| GeoError::Parse(format!("bad arcsecond value in {s:?}")))?;
            Ok(deg + min / 60.0 + sec / 3600.0)
        }
        _ => Err(GeoError::Parse(format!(
            "expected 1, 2, or 3 numeric tokens in {s:?}"
        ))),
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_abs_diff_eq;

    // --- Reference chart locations ---

    #[test]
    fn dd_william_lilly_diseworth() {
        // test 1 location (DD): 52.8166118,-1.3281652
        let c = parse_geo_coord("52.8166118,-1.3281652").unwrap();
        assert_abs_diff_eq!(c.lat, 52.816_611_8, epsilon = 1e-7);
        assert_abs_diff_eq!(c.lon, -1.328_165_2, epsilon = 1e-7);
    }

    #[test]
    fn ddm_anna_freud_vienna() {
        // ref_anna_freud: 48°12'30"N 16°22'18"E in DDM
        // note: DDM converted from 48.208333, 16.371667
        let c = parse_geo_coord("48° 12.500' N, 16° 22.300' E").unwrap();
        assert_abs_diff_eq!(c.lat, 48.208_333, epsilon = 1e-4);
        assert_abs_diff_eq!(c.lon, 16.371_667, epsilon = 1e-4);
    }

    #[test]
    fn dms_lightning_strike_universal_city() {
        // ref_lightning_strike: 34°08'20" N, 118°21'09" W
        // note: DMS converted to 34.138889, -118.352500
        let c = parse_geo_coord("34° 8' 20\" N, 118° 21' 9\" W").unwrap();
        assert_abs_diff_eq!(c.lat, 34.138_888_9, epsilon = 1e-6);
        assert_abs_diff_eq!(c.lon, -118.352_5, epsilon = 1e-4);
    }

    #[test]
    fn dms_vettius_valens_antioch_space_separated() {
        // test 0 location (DMS): 36° 12' 23" N 36° 9' 25" E
        let c = parse_geo_coord("36° 12' 23\" N 36° 9' 25\" E").unwrap();
        assert_abs_diff_eq!(c.lat, 36.0 + 12.0 / 60.0 + 23.0 / 3600.0, epsilon = 1e-6);
        assert_abs_diff_eq!(c.lon, 36.0 + 9.0 / 60.0 + 25.0 / 3600.0, epsilon = 1e-6);
    }

    // --- Normalisation / input sanitisation ---

    #[test]
    fn backtick_as_arcminute() {
        let c = parse_geo_coord("34° 8` 20\" N, 118° 21` 9\" W").unwrap();
        assert_abs_diff_eq!(c.lat, 34.0 + 8.0 / 60.0 + 20.0 / 3600.0, epsilon = 1e-6);
        assert_abs_diff_eq!(c.lon, -(118.0 + 21.0 / 60.0 + 9.0 / 3600.0), epsilon = 1e-6);
    }

    #[test]
    fn double_apostrophe_as_arcsecond() {
        let c = parse_geo_coord("34° 8' 20'' N, 118° 21' 9'' W").unwrap();
        assert_abs_diff_eq!(c.lat, 34.0 + 8.0 / 60.0 + 20.0 / 3600.0, epsilon = 1e-6);
        assert_abs_diff_eq!(c.lon, -(118.0 + 21.0 / 60.0 + 9.0 / 3600.0), epsilon = 1e-6);
    }

    #[test]
    fn smart_quotes_normalised() {
        // U+2019 right single quotation mark, U+201D right double quotation mark
        let s = "36\u{00B0} 12\u{2019} 23\u{201D} N, 36\u{00B0} 9\u{2019} 25\u{201D} E";
        let c = parse_geo_coord(s).unwrap();
        assert_abs_diff_eq!(c.lat, 36.0 + 12.0 / 60.0 + 23.0 / 3600.0, epsilon = 1e-6);
        assert_abs_diff_eq!(c.lon, 36.0 + 9.0 / 60.0 + 25.0 / 3600.0, epsilon = 1e-6);
    }

    #[test]
    fn signed_dd_with_plus() {
        let c = parse_geo_coord("+40.446,-79.982").unwrap();
        assert_abs_diff_eq!(c.lat, 40.446, epsilon = 1e-6);
        assert_abs_diff_eq!(c.lon, -79.982, epsilon = 1e-6);
    }

    #[test]
    fn truncated_decimal_places() {
        let c = parse_geo_coord("34.14,-118.35").unwrap();
        assert_abs_diff_eq!(c.lat, 34.14, epsilon = 1e-9);
        assert_abs_diff_eq!(c.lon, -118.35, epsilon = 1e-9);
    }

    #[test]
    fn degrees_only_no_arcmin() {
        let c = parse_geo_coord("36°N,36°E").unwrap();
        assert_abs_diff_eq!(c.lat, 36.0, epsilon = 1e-9);
        assert_abs_diff_eq!(c.lon, 36.0, epsilon = 1e-9);
    }

    #[test]
    fn dms_signed_no_hemisphere() {
        // Leading minus in DMS notation
        let c = parse_geo_coord("39° 44' 28\" N, -104° 50' 29\"").unwrap();
        assert_abs_diff_eq!(c.lat, 39.0 + 44.0 / 60.0 + 28.0 / 3600.0, epsilon = 1e-6);
        assert_abs_diff_eq!(
            c.lon,
            -(104.0 + 50.0 / 60.0 + 29.0 / 3600.0),
            epsilon = 1e-6
        );
    }

    // --- Range validation ---

    #[test]
    fn lat_out_of_range() {
        assert_eq!(
            parse_geo_coord("91.0,-79.982").unwrap_err(),
            GeoError::LatitudeRange(91.0)
        );
    }

    #[test]
    fn lon_out_of_range() {
        assert_eq!(
            parse_geo_coord("40.0,181.0").unwrap_err(),
            GeoError::LongitudeRange(181.0)
        );
    }

    // --- parse_lon ---

    #[test]
    fn parse_lon_decimal_west() {
        assert_abs_diff_eq!(
            parse_lon("-1.3281652").unwrap(),
            -1.328_165_2,
            epsilon = 1e-9
        );
    }

    #[test]
    fn parse_lon_dms_east() {
        assert_abs_diff_eq!(
            parse_lon("36° 9' 25\" E").unwrap(),
            36.0 + 9.0 / 60.0 + 25.0 / 3600.0,
            epsilon = 1e-6
        );
    }

    #[test]
    fn parse_lon_dms_west() {
        assert_abs_diff_eq!(
            parse_lon("104° 50' 29.49828\" W").unwrap(),
            -(104.0 + 50.0 / 60.0 + 29.49828 / 3600.0),
            epsilon = 1e-6
        );
    }

    #[test]
    fn parse_lon_ddm_east() {
        assert_abs_diff_eq!(
            parse_lon("36° 9.4167' E").unwrap(),
            36.0 + 9.4167 / 60.0,
            epsilon = 1e-4
        );
    }

    // --- Space-separated DMS/DDM (no degree/minute/second markers) ---

    #[test]
    fn parse_lat_space_dms_north() {
        assert_abs_diff_eq!(
            parse_lat("39 44 28 N").unwrap(),
            39.0 + 44.0 / 60.0 + 28.0 / 3600.0,
            epsilon = 1e-6
        );
    }

    #[test]
    fn parse_lon_space_dms_west() {
        assert_abs_diff_eq!(
            parse_lon("104 50 29 W").unwrap(),
            -(104.0 + 50.0 / 60.0 + 29.0 / 3600.0),
            epsilon = 1e-6
        );
    }

    #[test]
    fn parse_lat_space_ddm_north() {
        assert_abs_diff_eq!(
            parse_lat("39 44.477 N").unwrap(),
            39.0 + 44.477 / 60.0,
            epsilon = 1e-6
        );
    }

    #[test]
    fn parse_lon_space_dms_signed_no_hemi() {
        assert_abs_diff_eq!(
            parse_lon("-104 50 29").unwrap(),
            -(104.0 + 50.0 / 60.0 + 29.0 / 3600.0),
            epsilon = 1e-6
        );
    }

    #[test]
    fn parse_geo_coord_space_dms_pair() {
        // Vettius Valens — Antioch — no degree markers, space-separated DMS
        let c = parse_geo_coord("36 12 23 N 36 9 25 E").unwrap();
        assert_abs_diff_eq!(c.lat, 36.0 + 12.0 / 60.0 + 23.0 / 3600.0, epsilon = 1e-6);
        assert_abs_diff_eq!(c.lon, 36.0 + 9.0 / 60.0 + 25.0 / 3600.0, epsilon = 1e-6);
    }

    #[test]
    fn parse_lat_space_dms_fractional_seconds() {
        // fractional seconds
        assert_abs_diff_eq!(
            parse_lat("34 8 20.12 N").unwrap(),
            34.0 + 8.0 / 60.0 + 20.12 / 3600.0,
            epsilon = 1e-6
        );
    }

    #[test]
    fn parse_lat_dd_still_works() {
        // Regression: a single DD value must keep parsing as DD.
        assert_abs_diff_eq!(parse_lat("34.14").unwrap(), 34.14, epsilon = 1e-9);
    }
}
