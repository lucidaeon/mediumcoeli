//! Top-level JZOD document.

use crate::chart::Chart;
use serde::{Deserialize, Serialize};

/// A complete JZOD file: a version string and a list of charts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JzodDocument {
    /// JZOD wire-format version.
    pub version: String,
    /// Top-level charts.
    pub charts: Vec<Chart>,
}

impl JzodDocument {
    /// Build a document at the current [`crate::FORMAT_VERSION`].
    #[must_use]
    pub fn new(charts: Vec<Chart>) -> JzodDocument {
        JzodDocument {
            version: crate::FORMAT_VERSION.to_string(),
            charts,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chart::{
        Birth, Chart, ChartType, CoordinateSystem, Datetime, Ephemeris, Location, Zodiac,
    };
    use crate::placement::Placements;

    fn sample_chart() -> Chart {
        Chart {
            uid: "a3f8c2d1-6b94-4e17-8f53-2c71d0b43e85".into(),
            chart_type: ChartType::Radix,
            name: None,
            gender: None,
            rodden_rating: None,
            birth: Birth {
                datetime: Datetime {
                    year: 1895,
                    month: 12,
                    day: 3,
                    hour: 15,
                    minute: 15,
                    second: 0,
                    utc_offset: "+01:00".into(),
                    iana_tz: Some("Europe/Vienna".into()),
                    unknown: false,
                    tod_method: None,
                },
                location: Location {
                    name: Some("Vienna, Austria".into()),
                    latitude: Some(48.208_333),
                    longitude: Some(16.371_667),
                },
            },
            zodiac: Zodiac::Tropical,
            coordinate_system: CoordinateSystem::Geocentric,
            sect: None,
            interp_sect_twilight: None,
            ephemeris: Ephemeris {
                source: "test".into(),
                calculated_at: "2026-06-08T20:45:18Z".into(),
                jd_ut: None,
                jd_tt: None,
            },
            placements: Placements::default(),
            houses: crate::house::Houses::new(),
            lunar_phase: None,
            tithi: None,
            nested: vec![],
        }
    }

    #[test]
    fn new_sets_format_version() {
        let doc = JzodDocument::new(vec![sample_chart()]);
        assert_eq!(doc.version, "0.0.0");
    }

    #[test]
    fn round_trips_through_json() {
        let doc = JzodDocument::new(vec![sample_chart()]);
        let json = crate::to_string_pretty(&doc);
        let back = crate::from_str(&json).expect("valid JZOD");
        assert_eq!(doc, back);
    }

    #[test]
    fn ignores_unknown_keys() {
        // Forward compatibility: extra keys must not break deserialization.
        let json = r#"{ "version": "9.9.9", "charts": [], "future_field": 42 }"#;
        let doc = crate::from_str(json).expect("unknown keys ignored");
        assert_eq!(doc.version, "9.9.9");
    }
}
