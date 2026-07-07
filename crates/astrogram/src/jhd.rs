//! Reader for Jagannatha Hora `.jhd` chart files (line-oriented text, one chart
//! per file). Format documented in `$ASTRO_RESEARCH/file_jhd.md`.
//!
//! `.jhd` stores longitude and timezone East-negative (opposite of the ISO 6709
//! East-positive `Chart`); both are negated on read. The chart name is not in
//! the payload — it is the file's base name — so `parse_file` leaves `name`
//! empty and the read boundary fills it from the file stem.

use crate::capability::{CapabilitySet, ChartField};
use crate::chart::{Chart, CoordinateSystem, EventType, HouseSystem, Latitude, Longitude, Zodiac};
use thiserror::Error;

/// The only *varying* field `.jhd` recovers is the country → `region`.
pub const READ_CAPS: CapabilitySet = CapabilitySet::new(&[ChartField::Region]);
/// `.jhd` is read-only.
pub const WRITE_CAPS: CapabilitySet = CapabilitySet::new(&[]);

/// Failure parsing a `.jhd` file.
#[derive(Debug, Error)]
pub enum JhdError {
    /// Fewer than the seven required head lines.
    #[error("jhd: expected at least 7 lines, found {0}")]
    TooFewLines(usize),
    /// A numeric field did not parse.
    #[error("jhd: line {line}: expected a number, found {value:?}")]
    NotANumber {
        /// 1-based line number.
        line: usize,
        /// The offending text.
        value: String,
    },
    /// A coordinate was out of range.
    #[error("jhd: {0}")]
    Coordinate(String),
}

/// Parse one `.jhd` chart. `name` is left empty (recovered from the filename).
///
/// # Errors
/// [`JhdError`] on too-few lines, a non-numeric numeric field, or an
/// out-of-range coordinate.
pub fn parse_file(text: &str) -> Result<Chart, JhdError> {
    // Trim trailing \r from each line; keep them addressable by 1-based index.
    let lines: Vec<&str> = text
        .lines()
        .map(|l| l.trim_end_matches('\r').trim())
        .collect();
    if lines.len() < 7 {
        return Err(JhdError::TooFewLines(lines.len()));
    }
    let num = |idx: usize| -> Result<f64, JhdError> {
        lines[idx - 1]
            .parse::<f64>()
            .map_err(|_| JhdError::NotANumber {
                line: idx,
                value: lines[idx - 1].to_string(),
            })
    };

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let month = num(1)? as u8;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let day = num(2)? as u8;
    #[allow(clippy::cast_possible_truncation)]
    let year = num(3)? as i16;

    // Line 4: local time, decimal hours → h/m/s.
    let t = num(4)?;
    let hour = t.floor();
    let rem_min = (t - hour) * 60.0;
    let minute = rem_min.floor();
    let second = ((rem_min - minute) * 60.0).round();
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let (hour, minute, second) = (hour as u8, minute as u8, (second as u8).min(59));

    // Line 5: timezone in H.MM (East-negative). Decode to decimal hours, negate to ISO.
    let v5 = num(5)?;
    let mag = v5.abs();
    let dec_east_neg = v5.signum() * (mag.floor() + (mag.fract() * 100.0) / 60.0);
    let tz_offset_hours = -dec_east_neg;

    // Line 6: longitude (East-negative) → ISO East-positive. Line 7: latitude (North-positive).
    let longitude = Longitude::new(-num(6)?).map_err(|e| JhdError::Coordinate(e.to_string()))?;
    let latitude = Latitude::new(num(7)?).map_err(|e| JhdError::Coordinate(e.to_string()))?;

    // Variant: the input variant carries city (line 13) + country (line 14) as
    // strings. The position variant has a *number* at line 13 (a longitude) and
    // no place strings. Discriminate on line 13 being non-numeric.
    let (city, region) = match lines.get(12) {
        Some(l13) if !l13.is_empty() && l13.parse::<f64>().is_err() => {
            let clean = |s: &str| (s != "Unknown").then(|| s.to_string());
            (clean(l13), lines.get(13).and_then(|c| clean(c))) // city, country→region
        }
        _ => (None, None),
    };
    // The position variant's stored longitudes (lines 8–17) are ignored — `Chart`
    // has no body-position fields and those values are third-party-computed.

    Ok(Chart {
        name: String::new(),
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
        is_lmt: false,
        event_type: EventType::Unspecified,
        source_rating: None,
        // Not in the payload → fallback values, excluded from READ_CAPS so the
        // fill boundary treats them as un-sourced.
        house_system: HouseSystem::WholeSign,
        zodiac: Zodiac::Lahiri,
        coordinate_system: CoordinateSystem::Geocentric,
        sub_charts: vec![],
        notes: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // Synthetic chart: 1990-03-21, 06:00 local, 75°E 15°N, zone UTC+5:30.
    // JHD encodes East-negative for BOTH longitude and timezone.
    const MINIMAL: &str = "3\r\n21\r\n1990\r\n6.000000\r\n-5.300000\r\n-75.000000\r\n15.000000\r\n";

    #[test]
    fn minimal_variant_parses_with_sign_negation() {
        let c = parse_file(MINIMAL).unwrap();
        assert_eq!((c.year, c.month, c.day), (1990, 3, 21));
        assert_eq!((c.hour, c.minute, c.second), (6, 0, 0));
        // East-positive after negation:
        assert!((c.longitude.degrees() - 75.0).abs() < 1e-9);
        assert!((c.latitude.degrees() - 15.0).abs() < 1e-9);
        assert!((c.tz_offset_hours - 5.5).abs() < 1e-9); // -5.30 H.MM (East-neg) → +5.5 ISO
        assert!(c.name.is_empty()); // name comes from the filename, not the payload
        assert_eq!(c.city, None);
        assert_eq!(c.region, None);
    }

    const INPUT: &str = "3\r\n21\r\n1990\r\n6.000000\r\n-5.300000\r\n-75.000000\r\n15.000000\r\n\
0.000000\r\n-5.500000\r\n-5.500000\r\n0\r\n0\r\nSampletown\r\nSampleland\r\n";

    #[test]
    fn input_variant_recovers_city_and_region() {
        let c = parse_file(INPUT).unwrap();
        assert_eq!(c.city.as_deref(), Some("Sampletown"));
        assert_eq!(c.region.as_deref(), Some("Sampleland"));
        assert!((c.tz_offset_hours - 5.5).abs() < 1e-9);
    }

    #[test]
    fn position_variant_ignores_stored_longitudes_and_has_no_place() {
        // Lines 8–17 numeric (stored longitudes), line 18 a bit-string.
        let pos = "8\r\n8\r\n1912\r\n19.380000\r\n-5.300000\r\n-77.350000\r\n12.590000\r\n\
0.975635\r\n112.980246\r\n53.638040\r\n141.357796\r\n133.951795\r\n222.962485\r\n122.239615\r\n\
40.154017\r\n352.791606\r\n309.112932\r\n000100000\r\n";
        let c = parse_file(pos).unwrap();
        assert_eq!(c.city, None); // line 13 is numeric here → not a place string
        assert_eq!(c.region, None);
        assert!((c.tz_offset_hours - 5.5).abs() < 1e-9);
    }

    #[test]
    fn too_few_lines_errors() {
        assert!(matches!(
            parse_file("1\r\n2\r\n3\r\n"),
            Err(JhdError::TooFewLines(3))
        ));
    }
}
