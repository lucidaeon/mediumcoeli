//! Placement objects: bodies, angles, points, and lots.

use crate::coord::{Degrees8, Position};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Celestial body identifiers. The classical/modern set is the minimally
/// calculated radix; dwarf planets and asteroids are optional extensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[allow(missing_docs)]
pub enum BodyId {
    Sun,
    Moon,
    Mercury,
    Venus,
    Earth,
    Mars,
    Jupiter,
    Saturn,
    Uranus,
    Neptune,
    Pluto,
    EarthMoonBarycenter,
    Ceres,
    Quaoar,
    Sedna,
    Orcus,
    Haumea,
    Eris,
    Makemake,
    Gonggong,
    Chiron,
    Pallas,
    Juno,
    Vesta,
    Hygiea,
    Pholus,
    Nessus,
    Chariklo,
    Ixion,
    Varuna,
}

/// Chart angle identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[allow(missing_docs)]
pub enum AngleId {
    Ascendant,
    Descendant,
    Midheaven,
    ImumCoeli,
}

/// Mathematical point identifiers. Mean/true variants carry the suffix
/// (JZOD OQ-19, Option A).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[allow(missing_docs)]
pub enum PointId {
    Vertex,
    AntiVertex,
    NorthNodeMean,
    NorthNodeTrue,
    SouthNodeMean,
    SouthNodeTrue,
    BlackMoonLilithMean,
    BlackMoonLilithTrue,
    PriapusMean,
    PriapusTrue,
}

/// Arabic/Hermetic lot identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[allow(missing_docs)]
pub enum LotId {
    LotOfFortune,
    LotOfSpirit,
    LotOfEros,
    LotOfExaltation,
    LotOfNecessity,
    LotOfCourage,
    LotOfVictory,
    LotOfNemesis,
}

/// A celestial body placement: zodiacal position plus latitude, speed,
/// retrograde flag, optional distance, and per-house-system house numbers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Body {
    /// Body identifier.
    pub id: BodyId,
    /// Zodiacal position (flattened: `ecliptic_longitude`, `sign`, `degree`, …).
    #[serde(flatten)]
    pub position: Position,
    /// Ecliptic latitude in degrees (north positive).
    pub ecliptic_latitude: Degrees8,
    /// Daily motion in degrees (negative = retrograde).
    pub daily_speed: Degrees8,
    /// Retrograde flag (derived from the sign of `daily_speed`).
    pub retrograde: bool,
    /// Distance from the chart origin in AU, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub distance_au: Option<f64>,
    /// House number (1–12) keyed by house-system slug.
    #[serde(default)]
    pub house: BTreeMap<String, u8>,
}

/// A chart angle placement (no latitude, speed, or retrograde).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Angle {
    /// Angle identifier.
    pub id: AngleId,
    /// Zodiacal position (flattened).
    #[serde(flatten)]
    pub position: Position,
}

/// A mathematical point placement (has a retrograde flag, no latitude/speed).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Point {
    /// Point identifier.
    pub id: PointId,
    /// Zodiacal position (flattened).
    #[serde(flatten)]
    pub position: Position,
    /// Retrograde flag.
    pub retrograde: bool,
}

/// An Arabic/Hermetic lot placement (longitude only).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Lot {
    /// Lot identifier.
    pub id: LotId,
    /// Zodiacal position (flattened).
    #[serde(flatten)]
    pub position: Position,
}

/// All placements for a chart.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Placements {
    /// Celestial bodies. Always emitted (may be an empty array).
    #[serde(default)]
    pub bodies: Vec<Body>,
    /// Chart angles. Omitted when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub angles: Vec<Angle>,
    /// Mathematical points. Omitted when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub points: Vec<Point>,
    /// Lots. Omitted when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lots: Vec<Lot>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coord::Position;

    #[test]
    fn body_id_slugs() {
        assert_eq!(serde_json::to_string(&BodyId::Sun).unwrap(), "\"sun\"");
        assert_eq!(serde_json::to_string(&BodyId::Pluto).unwrap(), "\"pluto\"");
    }

    #[test]
    fn hygiea_serializes() {
        assert_eq!(
            serde_json::to_string(&BodyId::Hygiea).unwrap(),
            "\"hygiea\""
        );
    }

    #[test]
    fn point_id_keeps_variant_suffix() {
        assert_eq!(
            serde_json::to_string(&PointId::NorthNodeTrue).unwrap(),
            "\"north_node_true\""
        );
        assert_eq!(
            serde_json::to_string(&PointId::BlackMoonLilithMean).unwrap(),
            "\"black_moon_lilith_mean\""
        );
    }

    #[test]
    fn lot_id_slug() {
        assert_eq!(
            serde_json::to_string(&LotId::LotOfFortune).unwrap(),
            "\"lot_of_fortune\""
        );
    }

    #[test]
    fn body_flattens_position_fields() {
        let body = Body {
            id: BodyId::Sun,
            position: Position::from_longitude(251.206),
            ecliptic_latitude: crate::coord::Degrees8(-0.002),
            daily_speed: crate::coord::Degrees8(1.015),
            retrograde: false,
            distance_au: None,
            house: std::collections::BTreeMap::new(),
        };
        let v = serde_json::to_value(&body).unwrap();
        assert_eq!(v["id"], "sun");
        assert_eq!(v["sign"], "sagittarius"); // flattened, not nested under "position"
        assert_eq!(v["degree"], 11);
        assert!(v.get("position").is_none());
        assert!(v.get("distance_au").is_none()); // None is skipped
    }

    #[test]
    fn empty_optional_arrays_are_omitted() {
        let p = Placements {
            bodies: vec![],
            angles: vec![],
            points: vec![],
            lots: vec![],
        };
        let v = serde_json::to_value(&p).unwrap();
        assert_eq!(v["bodies"], serde_json::json!([])); // always present
        assert!(v.get("angles").is_none());
        assert!(v.get("points").is_none());
        assert!(v.get("lots").is_none());
    }
}
