//! Chart-level types: the `Chart` object and its scalar/enum members.

use crate::house::Houses;
use crate::placement::Placements;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// What a chart represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[allow(missing_docs)]
pub enum ChartType {
    #[default]
    Radix,
    Event,
    Horary,
    Election,
    Decumbiture,
    Mundane,
    Transit,
    SolarReturn,
    LunarReturn,
    SecondaryProgression,
    TertiaryProgression,
    SolarArc,
    Relocated,
    Composite,
    Chart,
}

/// Display and alias names.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Name {
    /// Current/colloquial name.
    pub display: String,
    /// Historical, legal, birth, married, or stage names.
    #[serde(default)]
    pub aliases: Vec<String>,
}

/// Birth/event date and time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Datetime {
    /// Year (signed; negative = BCE).
    pub year: i32,
    /// Month, 1–12.
    pub month: u8,
    /// Day, 1–31.
    pub day: u8,
    /// Hour, 0–23.
    pub hour: u8,
    /// Minute, 0–59.
    pub minute: u8,
    /// Second, 0–59.
    pub second: u8,
    /// Authoritative UTC offset actually used for calculation (`+HH:MM`).
    pub utc_offset: String,
    /// Informational IANA tz id (display/debug only; not a calculation input).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iana_tz: Option<String>,
    /// True when the time of day is unknown.
    pub unknown: bool,
    /// Method used to populate an unknown time (`sunrise`, `noon`, …).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tod_method: Option<String>,
}

/// Birth/event location.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Location {
    /// Human-readable place name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Latitude, decimal degrees (ISO 6709: North positive).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latitude: Option<f64>,
    /// Longitude, decimal degrees (ISO 6709: East positive).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub longitude: Option<f64>,
}

/// `birth` block: datetime + location.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Birth {
    /// Date and time.
    pub datetime: Datetime,
    /// Location.
    pub location: Location,
}

/// Ephemeris provenance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ephemeris {
    /// Ephemeris source identifier (e.g. `"DE441"`).
    pub source: String,
    /// ISO 8601 UTC timestamp of calculation.
    pub calculated_at: String,
    /// Julian Date (UT), when relevant.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jd_ut: Option<f64>,
    /// Julian Date (TT), when relevant.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jd_tt: Option<f64>,
}

/// Zodiac, as an object with a `name` discriminator (JZOD OQ-4).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "name", rename_all = "snake_case")]
pub enum Zodiac {
    /// Anchored to the vernal equinox.
    Tropical,
    /// Anchored to the North Node.
    Draconic,
    /// Anchored to the fixed stars; carries an optional ayanamsha slug.
    Sidereal {
        /// Ayanamsha identifier (e.g. `"lahiri"`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ayanamsha: Option<String>,
    },
}

/// Coordinate reference frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[allow(missing_docs)]
pub enum CoordinateSystem {
    Geocentric,
    Topocentric,
    Heliocentric,
}

/// Chart sect. Geocentric by definition (Sun relative to the local horizon);
/// heliocentric charts omit the `sect` field entirely rather than using a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[allow(missing_docs)]
pub enum Sect {
    Diurnal,
    Nocturnal,
    /// Birth time is unknown (`datetime.unknown` is `true`), so sect cannot be
    /// trusted from the placeholder time-of-day.
    Unknown,
}

/// The eight traditional phases of the synodic cycle (45° octants).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[allow(missing_docs)]
pub enum LunarPhaseName {
    NewMoon,
    Crescent,
    FirstQuarter,
    Gibbous,
    FullMoon,
    Disseminating,
    LastQuarter,
    Balsamic,
}

/// Computed lunar phase: synodic arc, 8-fold phase name, and 28-fold lunation
/// day.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LunarPhase {
    /// Moon–Sun elongation, degrees, range \[0, 360).
    pub synodic_arc_deg: f64,
    /// 8-fold phase name.
    pub phase: LunarPhaseName,
    /// 1-indexed position within the 28-fold lunar month, range 1–28.
    pub lunation_day: u8,
}

/// A single JZOD chart.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Chart {
    /// Unique id within the file.
    pub uid: String,
    /// Chart type (serialized as `type`).
    #[serde(rename = "type")]
    pub chart_type: ChartType,
    /// Display + alias names. Absent for charts without a name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<Name>,
    /// Gender marker (`m`/`f`/`x`/`a`/free string). Absent for entity charts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gender: Option<String>,
    /// Astro-Databank/Rodden rating.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rodden_rating: Option<String>,
    /// Birth/event data.
    pub birth: Birth,
    /// Zodiac.
    pub zodiac: Zodiac,
    /// Coordinate system.
    pub coordinate_system: CoordinateSystem,
    /// Sect, when computed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sect: Option<Sect>,
    /// Ephemeris provenance.
    pub ephemeris: Ephemeris,
    /// Computed placements.
    pub placements: Placements,
    /// House cusps by system. Omitted when empty.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub houses: Houses,
    /// Lunar phase. `null` for heliocentric charts or when Sun/Moon are absent.
    pub lunar_phase: Option<LunarPhase>,
    /// Nested derivative/associated charts. Omitted when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nested: Vec<Chart>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sect_serializes_three_states_snake_case() {
        assert_eq!(
            serde_json::to_string(&Sect::Diurnal).unwrap(),
            "\"diurnal\""
        );
        assert_eq!(
            serde_json::to_string(&Sect::Nocturnal).unwrap(),
            "\"nocturnal\""
        );
        assert_eq!(
            serde_json::to_string(&Sect::Unknown).unwrap(),
            "\"unknown\""
        );
    }

    #[test]
    fn chart_type_renames_to_type_key_and_snake_case() {
        assert_eq!(
            serde_json::to_string(&ChartType::Radix).unwrap(),
            "\"radix\""
        );
        assert_eq!(
            serde_json::to_string(&ChartType::SolarReturn).unwrap(),
            "\"solar_return\""
        );
    }

    #[test]
    fn tropical_zodiac_is_object_with_name() {
        let v = serde_json::to_value(Zodiac::Tropical).unwrap();
        assert_eq!(v, serde_json::json!({ "name": "tropical" }));
    }

    #[test]
    fn sidereal_zodiac_adds_ayanamsha() {
        let v = serde_json::to_value(Zodiac::Sidereal {
            ayanamsha: Some("lahiri".into()),
        })
        .unwrap();
        assert_eq!(
            v,
            serde_json::json!({ "name": "sidereal", "ayanamsha": "lahiri" })
        );
    }

    #[test]
    fn sidereal_without_ayanamsha_omits_it() {
        let v = serde_json::to_value(Zodiac::Sidereal { ayanamsha: None }).unwrap();
        assert_eq!(v, serde_json::json!({ "name": "sidereal" }));
    }

    #[test]
    fn datetime_skips_optional_iana_and_tod() {
        let dt = Datetime {
            year: 1895,
            month: 12,
            day: 3,
            hour: 15,
            minute: 15,
            second: 0,
            utc_offset: "+01:00".into(),
            iana_tz: None,
            unknown: false,
            tod_method: None,
        };
        let v = serde_json::to_value(&dt).unwrap();
        assert_eq!(v["utc_offset"], "+01:00");
        assert_eq!(v["unknown"], false); // always emitted
        assert!(v.get("iana_tz").is_none());
        assert!(v.get("tod_method").is_none());
    }

    #[test]
    fn lunar_phase_serializes_as_object() {
        let lp = LunarPhase {
            synodic_arc_deg: 72.783,
            phase: LunarPhaseName::Crescent,
            lunation_day: 6,
        };
        let v = serde_json::to_value(&lp).unwrap();
        assert_eq!(v["phase"], "crescent");
        assert_eq!(v["lunation_day"], 6);
        assert!((v["synodic_arc_deg"].as_f64().unwrap() - 72.783).abs() < 1e-9);
    }

    #[test]
    fn chart_renders_type_key_and_null_lunar_phase() {
        let chart = Chart {
            uid: "u".into(),
            chart_type: ChartType::Radix,
            name: None,
            gender: None,
            rodden_rating: None,
            birth: Birth {
                datetime: Datetime {
                    year: 2000,
                    month: 1,
                    day: 1,
                    hour: 0,
                    minute: 0,
                    second: 0,
                    utc_offset: "+00:00".into(),
                    iana_tz: None,
                    unknown: false,
                    tod_method: None,
                },
                location: Location {
                    name: None,
                    latitude: None,
                    longitude: None,
                },
            },
            zodiac: Zodiac::Tropical,
            coordinate_system: CoordinateSystem::Geocentric,
            sect: None,
            ephemeris: Ephemeris {
                source: "test".into(),
                calculated_at: "1970-01-01T00:00:00Z".into(),
                jd_ut: None,
                jd_tt: None,
            },
            placements: crate::placement::Placements::default(),
            houses: crate::house::Houses::new(),
            lunar_phase: None,
            nested: vec![],
        };
        let v = serde_json::to_value(&chart).unwrap();
        assert_eq!(v["type"], "radix");
        assert!(v.get("chart_type").is_none());
        assert!(v["lunar_phase"].is_null()); // present and null
        assert!(v.get("houses").is_none()); // empty houses skipped
        assert!(v.get("sect").is_none());
        assert!(v.get("nested").is_none());
    }
}
