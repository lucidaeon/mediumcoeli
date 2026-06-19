//! Coordinate notation primitives: signs, fixed-precision degrees, and the
//! absolute-longitude → zodiacal (sign + d/m/s) decomposition.

use serde::{Deserialize, Serialize};

/// The twelve zodiac signs, serialized as lower-snake-case slugs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sign {
    /// Aries (0°–30°).
    Aries,
    /// Taurus (30°–60°).
    Taurus,
    /// Gemini (60°–90°).
    Gemini,
    /// Cancer (90°–120°).
    Cancer,
    /// Leo (120°–150°).
    Leo,
    /// Virgo (150°–180°).
    Virgo,
    /// Libra (180°–210°).
    Libra,
    /// Scorpio (210°–240°).
    Scorpio,
    /// Sagittarius (240°–270°).
    Sagittarius,
    /// Capricorn (270°–300°).
    Capricorn,
    /// Aquarius (300°–330°).
    Aquarius,
    /// Pisces (330°–360°).
    Pisces,
}

impl Sign {
    /// All twelve signs in zodiacal order (Aries first).
    pub const ALL: [Sign; 12] = [
        Sign::Aries,
        Sign::Taurus,
        Sign::Gemini,
        Sign::Cancer,
        Sign::Leo,
        Sign::Virgo,
        Sign::Libra,
        Sign::Scorpio,
        Sign::Sagittarius,
        Sign::Capricorn,
        Sign::Aquarius,
        Sign::Pisces,
    ];

    /// Sign at the given zodiacal index (0 = Aries). Wraps modulo 12.
    #[must_use]
    pub fn from_index(i: usize) -> Sign {
        Sign::ALL[i % 12]
    }
}

/// A degree value serialized as a JSON number with exactly eight decimal
/// places (e.g. `58.26166755`). The workspace enables `serde_json`'s
/// `arbitrary_precision` feature, so the fixed-precision string is emitted
/// verbatim. Deserializes from any JSON number.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Degrees8(pub f64);

impl Serialize for Degrees8 {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let n: serde_json::Number = format!("{:.8}", self.0)
            .parse()
            .map_err(serde::ser::Error::custom)?;
        n.serialize(s)
    }
}

impl<'de> Deserialize<'de> for Degrees8 {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        // Deserialize via `serde_json::Number` so that the `arbitrary_precision`
        // map-token representation (produced by `#[serde(flatten)]` collect paths)
        // is handled correctly in addition to regular streaming JSON.
        let n = serde_json::Number::deserialize(d)?;
        n.as_f64()
            .ok_or_else(|| serde::de::Error::custom("degree value is not a finite f64"))
            .map(Degrees8)
    }
}

/// Zodiacal position: an absolute longitude paired with its sign and the
/// degree/minute/second within that sign.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Position {
    /// Absolute ecliptic longitude, 0–360° from 0° Aries.
    pub ecliptic_longitude: Degrees8,
    /// Sign containing the position.
    pub sign: Sign,
    /// Degree within the sign, 0–29.
    pub degree: u8,
    /// Arcminute within the degree, 0–59.
    pub minute: u8,
    /// Arcsecond within the arcminute, 0–59.
    pub second: u8,
}

impl Position {
    /// Decompose an absolute ecliptic longitude into sign + degree/minute/second.
    ///
    /// Rounds to 4 decimal places before splitting so values like
    /// `29.9999999…°` do not silently land in the next sign.
    #[must_use]
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    pub fn from_longitude(lon_deg: f64) -> Position {
        let norm = (lon_deg.rem_euclid(360.0) * 1e4).round() / 1e4;
        let norm = norm.rem_euclid(360.0);
        let idx = (norm / 30.0).floor() as usize;
        let in_sign = norm - idx as f64 * 30.0;
        let d = in_sign.floor() as u8;
        let mf = (in_sign - f64::from(d)) * 60.0;
        let m = mf.floor() as u8;
        let s = ((mf - f64::from(m)) * 60.0).floor() as u8;
        Position {
            ecliptic_longitude: Degrees8(lon_deg),
            sign: Sign::from_index(idx),
            degree: d,
            minute: m,
            second: s,
        }
    }

    /// Decompose for a whole-sign cusp: keeps the absolute longitude and the
    /// sign but forces degree/minute/second to 0, honouring the JZOD whole-sign
    /// invariant (cusps are always exactly 0° of a sign — no float noise).
    #[must_use]
    pub fn whole_sign_from_longitude(lon_deg: f64) -> Position {
        let mut p = Position::from_longitude(lon_deg);
        p.degree = 0;
        p.minute = 0;
        p.second = 0;
        p
    }
}

#[cfg(test)]
#[allow(clippy::unreadable_literal)]
mod tests {
    use super::*;

    #[test]
    fn sign_serializes_as_snake_case_slug() {
        let json = serde_json::to_string(&Sign::Sagittarius).unwrap();
        assert_eq!(json, "\"sagittarius\"");
    }

    #[test]
    fn from_index_wraps_and_orders() {
        assert_eq!(Sign::from_index(0), Sign::Aries);
        assert_eq!(Sign::from_index(11), Sign::Pisces);
        assert_eq!(Sign::from_index(12), Sign::Aries); // wraps
    }

    #[test]
    fn degrees8_serializes_with_eight_decimals() {
        let json = serde_json::to_string(&Degrees8(58.26166755)).unwrap();
        assert_eq!(json, "58.26166755");
    }

    #[test]
    fn degrees8_pads_trailing_zeros() {
        let json = serde_json::to_string(&Degrees8(30.0)).unwrap();
        assert_eq!(json, "30.00000000");
    }

    #[test]
    fn position_decomposes_known_longitude() {
        // 251.206° = 11°12'21" Sagittarius (sag starts at 240°).
        let p = Position::from_longitude(251.206);
        assert_eq!(p.sign, Sign::Sagittarius);
        assert_eq!(p.degree, 11);
        assert_eq!(p.minute, 12);
        assert_eq!(p.second, 21);
    }

    #[test]
    fn position_snaps_boundary_noise_up_to_next_sign() {
        // A value that is really 30.0° but carries tiny negative float noise must
        // snap up to Taurus 0°, not render as 29°59'59" Aries.
        let p = Position::from_longitude(29.99999999);
        assert_eq!(p.sign, Sign::Taurus);
        assert_eq!(p.degree, 0);
        assert_eq!(p.minute, 0);
        assert_eq!(p.second, 0);
    }

    #[test]
    fn whole_sign_position_zeroes_dms() {
        let p = Position::whole_sign_from_longitude(48.5);
        assert_eq!(p.sign, Sign::Taurus);
        assert_eq!(p.degree, 0);
        assert_eq!(p.minute, 0);
        assert_eq!(p.second, 0);
    }
}
