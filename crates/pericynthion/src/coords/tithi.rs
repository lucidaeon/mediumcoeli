//! Tithi: the 30-fold Vedic lunar day.
//!
//! A **tithi** is one of the 30 equal divisions of the synodic month, each
//! spanning 12° of the Moon's elongation ahead of the Sun.  The 30 tithis
//! are divided into two fortnights of 15:
//!
//! - **Shukla paksha** (waxing), tithis 1–15: from New Moon (0°) to
//!   Full Moon (180°).  Tithi 15 is Purnima (Full Moon).
//! - **Krishna paksha** (waning), tithis 16–30: from Full Moon (180°)
//!   back to New Moon (360°).  Tithi 30 is Amavasya (New Moon).
//!
//! Because a tithi is defined by a **relative angle** (Moon − Sun modulo
//! 360°) it is completely zodiac-independent: the same tithi number falls
//! at the same elongation whether you use tropical or sidereal longitudes.
//!
//! This module contains no ephemeris I/O.  Callers supply the two
//! geocentric ecliptic longitudes, typically produced by
//! [`apparent_ecliptic_position`](crate::coords::apparent::apparent_ecliptic_position).

/// One of the 30 tithis (Vedic lunar days): the 12°-wide divisions of the
/// Moon's elongation ahead of the Sun. Zodiac-independent (a relative angle),
/// so identical in tropical or sidereal frames.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tithi {
    /// 1-indexed tithi number, range 1..=30.
    pub index: u8,
    /// Traditional name (e.g. "Pratipada", "Purnima", "Amavasya").
    pub name: &'static str,
    /// Progress through the current tithi, range [0.0, 1.0).
    pub fraction: f64,
}

/// Repeating fortnight names for tithis 1–14 (indices 0–13).
///
/// Applied to both Shukla and Krishna paksha by `tithi_name`; tithis 15
/// and 30 are special-cased as "Purnima" and "Amavasya" respectively.
const TITHI_NAMES: [&str; 14] = [
    "Pratipada",
    "Dwitiya",
    "Tritiya",
    "Chaturthi",
    "Panchami",
    "Shashthi",
    "Saptami",
    "Ashtami",
    "Navami",
    "Dashami",
    "Ekadashi",
    "Dwadashi",
    "Trayodashi",
    "Chaturdashi",
];

/// Return the traditional name for a 1-indexed tithi (1–30).
///
/// - Tithi 15 → "Purnima" (Full Moon).
/// - Tithi 30 → "Amavasya" (New Moon / dark moon).
/// - All others → the repeating fortnight name from `TITHI_NAMES`.
fn tithi_name(index: u8) -> &'static str {
    match index {
        15 => "Purnima",
        30 => "Amavasya",
        _ => TITHI_NAMES[((index - 1) % 15) as usize],
    }
}

/// Compute the tithi from geocentric tropical longitudes (degrees).
///
/// Both arguments are ecliptic longitude in degrees.  The synodic arc
/// (`moon − sun` modulo 360°) is divided into 30 equal 12° steps to
/// produce the tithi index and intra-tithi fraction.
///
/// Returns a [`Tithi`] with `index` in 1..=30, `name` from the traditional
/// Sanskrit names, and `fraction` in \[0.0, 1.0).
#[must_use]
pub fn tithi(moon_lon_deg: f64, sun_lon_deg: f64) -> Tithi {
    let arc = (moon_lon_deg - sun_lon_deg).rem_euclid(360.0);

    // 30-fold: each tithi spans 12°.  Clamp to 1–30 to guard against
    // float edge cases where arc ≈ 360° would otherwise yield index 31.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let index = ((arc / 12.0).floor() as u8).saturating_add(1).min(30);

    let fraction = (arc % 12.0) / 12.0;
    let name = tithi_name(index);

    Tithi {
        index,
        name,
        fraction,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_moon_at_zero() {
        let t = tithi(0.0, 0.0);
        assert_eq!(t.index, 1);
        assert_eq!(t.name, "Pratipada");
        assert!(t.fraction.abs() < 1e-9, "fraction was {}", t.fraction);
    }

    #[test]
    fn mid_first_tithi_at_six() {
        let t = tithi(6.0, 0.0);
        assert_eq!(t.index, 1);
        assert_eq!(t.name, "Pratipada");
        assert!(
            (t.fraction - 0.5).abs() < 1e-9,
            "fraction was {}",
            t.fraction
        );
    }

    #[test]
    fn second_tithi_at_twelve() {
        let t = tithi(12.0, 0.0);
        assert_eq!(t.index, 2);
        assert_eq!(t.name, "Dwitiya");
        assert!(t.fraction.abs() < 1e-9, "fraction was {}", t.fraction);
    }

    #[test]
    fn purnima_at_one_eighty() {
        // 180° is the *start* of tithi 16 (Krishna Pratipada), not Purnima.
        // Purnima (tithi 15) occupies 168°–180°.
        let t = tithi(180.0, 0.0);
        assert_eq!(t.index, 16);
        assert_eq!(t.name, "Pratipada");
    }

    #[test]
    fn purnima_is_tithi_fifteen() {
        let t = tithi(174.0, 0.0);
        assert_eq!(t.index, 15);
        assert_eq!(t.name, "Purnima");
    }

    #[test]
    fn amavasya_starts_at_three_forty_eight() {
        // Brief listed 354°, but tithi 30 (Amavasya) spans 348°–360°; its
        // start is 29 × 12 = 348°.  fraction = 0 at the boundary exactly.
        let t = tithi(348.0, 0.0);
        assert_eq!(t.index, 30);
        assert_eq!(t.name, "Amavasya");
        assert!(t.fraction.abs() < 1e-9, "fraction was {}", t.fraction);
    }

    #[test]
    fn amavasya_mid_at_three_fifty_four() {
        // 354° is the midpoint of Amavasya (tithi 30 spans 348°–360°).
        let t = tithi(354.0, 0.0);
        assert_eq!(t.index, 30);
        assert_eq!(t.name, "Amavasya");
        assert!(
            (t.fraction - 0.5).abs() < 1e-9,
            "fraction was {}",
            t.fraction
        );
    }

    #[test]
    fn wraps_for_moon_behind_sun() {
        // moon=10°, sun=350° → arc = (10−350).rem_euclid(360) = 20° → tithi 2 "Dwitiya"
        let t = tithi(10.0, 350.0);
        assert_eq!(t.index, 2);
        assert_eq!(t.name, "Dwitiya");
    }

    #[test]
    fn index_never_exceeds_thirty() {
        // arc ≈ 359.999° → floor(359.999/12) = 29 → +1 = 30, not 31
        let t = tithi(359.999, 0.0);
        assert_eq!(t.index, 30);
    }

    /// Sanity-check the 12°-band arithmetic against the published tithi
    /// convention (drikpanchang.com tithi definition: 12° elongation per
    /// tithi; Shukla Panchami = tithis spanning 48°–60°).
    ///
    /// This is a pure-arithmetic oracle — NOT derived from specimen data.
    /// It exercises a concrete elongation that falls squarely inside the
    /// Shukla Panchami band:
    ///   arc = 50°  →  floor(50 / 12) + 1 = 4 + 1 = 5  →  "Panchami"
    /// and confirms that the implementation matches the convention.
    #[test]
    fn panchang_sanity_oracle() {
        // arc = moon − sun = 50° − 0° = 50°, which lies in the range 48°–60°
        // (Shukla Panchami, tithi 5).
        let t = tithi(50.0, 0.0);
        assert_eq!(t.index, 5, "expected tithi 5 (Shukla Panchami) for arc=50°");
        assert_eq!(t.name, "Panchami");
    }
}
