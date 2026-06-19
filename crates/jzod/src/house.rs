//! House cusps, keyed by system slug then by house number.

use crate::coord::{Degrees8, Position, Sign};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A single house cusp.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct HouseCusp {
    /// Absolute ecliptic longitude of the cusp, 0–360°.
    pub longitude: Degrees8,
    /// Sign the cusp falls in.
    pub sign: Sign,
    /// Degree within the sign, 0–29.
    pub degree: u8,
    /// Arcminute, 0–59.
    pub minute: u8,
    /// Arcsecond, 0–59.
    pub second: u8,
}

impl HouseCusp {
    /// Build a cusp from an absolute longitude.
    #[must_use]
    pub fn from_longitude(lon_deg: f64) -> HouseCusp {
        Self::from_position(Position::from_longitude(lon_deg))
    }

    /// Build a whole-sign cusp (degree/minute/second forced to 0).
    #[must_use]
    pub fn whole_sign_from_longitude(lon_deg: f64) -> HouseCusp {
        Self::from_position(Position::whole_sign_from_longitude(lon_deg))
    }

    fn from_position(p: Position) -> HouseCusp {
        HouseCusp {
            longitude: p.ecliptic_longitude,
            sign: p.sign,
            degree: p.degree,
            minute: p.minute,
            second: p.second,
        }
    }
}

/// Cusps for one house system, keyed by house number (1–12). Serializes with
/// numeric-ordered string keys (`"1"`…`"12"`).
pub type HouseSystemCusps = BTreeMap<u8, HouseCusp>;

/// House cusps for all computed systems, keyed by system slug.
pub type Houses = BTreeMap<String, HouseSystemCusps>;

#[cfg(test)]
#[allow(clippy::unreadable_literal)]
mod tests {
    use super::*;

    #[test]
    fn cusp_decomposes_longitude() {
        let c = HouseCusp::from_longitude(58.26166755);
        assert_eq!(c.sign, crate::coord::Sign::Taurus);
        assert_eq!(c.degree, 28);
        assert_eq!(c.minute, 15);
        assert_eq!(c.second, 42);
    }

    #[test]
    fn whole_sign_cusp_zeroes_dms() {
        let c = HouseCusp::whole_sign_from_longitude(48.9);
        assert_eq!(c.sign, crate::coord::Sign::Taurus);
        assert_eq!(c.degree, 0);
        assert_eq!(c.minute, 0);
        assert_eq!(c.second, 0);
    }

    #[test]
    fn house_numbers_serialize_in_numeric_string_order() {
        let mut cusps: HouseSystemCusps = HouseSystemCusps::new();
        for h in 1u8..=12 {
            cusps.insert(h, HouseCusp::from_longitude(f64::from(h) * 30.0));
        }
        // The real output path serializes the typed map directly to a string;
        // BTreeMap<u8> emits keys in numeric order ("1".."12"), not lexical.
        let json = serde_json::to_string(&cusps).unwrap();
        let offset_two = json.find("\"2\":").expect("key 2 present");
        let offset_ten = json.find("\"10\":").expect("key 10 present");
        let offset_twelve = json.find("\"12\":").expect("key 12 present");
        // Numeric order: "2" precedes "10" and "12". (Lexical order would put
        // "10"/"12" before "2".)
        assert!(
            offset_two < offset_ten,
            "key 2 must precede key 10 (numeric, not lexical)"
        );
        assert!(offset_ten < offset_twelve, "key 10 must precede key 12");
    }
}
