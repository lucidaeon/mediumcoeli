//! The bodies the library can compute, and the mapping from those
//! astrologer-facing identities to JPL's on-disk slot ordering.
//!
//! # Two enumerations, one mapping
//!
//! The DE-series ephemeris files lay bodies out in a fixed JPL order
//! that is convenient for fitting orbits (heavy planets next to each
//! other, the Moon's geocentric coefficients grouped with the Sun's
//! barycentric coefficients). That order is *not* the order an
//! astrologer would use. We expose two enums:
//!
//! - [`JplSlot`]: the on-disk slot ordering. Each variant corresponds
//!   to one column of GROUP 1050 in the ASCII header. This is purely
//!   plumbing.
//! - [`Body`]: the astrologer-facing identity. Variants are in the
//!   conventional order (Sun, Moon, Mercury, Venus, Mars, ...). This
//!   is what callers actually use.
//!
//! [`Body::Earth`] has no JPL slot of its own — the DE files compute
//! the Earth-Moon barycenter and the Moon's geocentric offset, and
//! Earth is derived as `EMB − Moon/(1+EMRAT)`. This is handled in
//! [`crate::ephemeris`], not here.

/// The 13 numerical "slots" in a DE-series coefficient record, in the
/// order JPL writes them on disk (column order of GROUP 1050).
///
/// Slot 12 ([`JplSlot::EarthNutations`]) has 2 axes (Δψ, Δε). All
/// other slots use 3 axes. Slot 13 ([`JplSlot::LunarLibration`])
/// contains the Moon's physical libration angles (φ, θ, ψ).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum JplSlot {
    /// Barycentric Mercury — column 1.
    Mercury = 0,
    /// Barycentric Venus — column 2.
    Venus = 1,
    /// Barycentric Earth-Moon barycenter — column 3.
    EarthMoonBarycenter = 2,
    /// Barycentric Mars — column 4.
    Mars = 3,
    /// Barycentric Jupiter — column 5.
    Jupiter = 4,
    /// Barycentric Saturn — column 6.
    Saturn = 5,
    /// Barycentric Uranus — column 7.
    Uranus = 6,
    /// Barycentric Neptune — column 8.
    Neptune = 7,
    /// Barycentric Pluto — column 9.
    Pluto = 8,
    /// Geocentric Moon (Earth-relative, not barycentric) — column 10.
    Moon = 9,
    /// Barycentric Sun — column 11.
    Sun = 10,
    /// Earth nutations (Δψ, Δε) — column 12. Two axes.
    EarthNutations = 11,
    /// Lunar physical libration (φ, θ, ψ) — column 13.
    LunarLibration = 12,
}

impl JplSlot {
    /// Zero-indexed column number into GROUP 1050.
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }

    /// Number of independent axes this slot carries. Three for all
    /// positional bodies and lunar libration; two for Earth nutations.
    #[must_use]
    pub const fn axes(self) -> u32 {
        match self {
            Self::EarthNutations => 2,
            _ => 3,
        }
    }
}

/// Astrologer-facing body identifier.
///
/// Includes all ten classical bodies (Sun, Moon, Mercury, Venus, Mars,
/// Jupiter, Saturn, Uranus, Neptune, Pluto), plus the two derived
/// frames a library caller may want explicitly:
///
/// - [`Body::Earth`] — derived from EMB and Moon; not a JPL slot.
/// - [`Body::EarthMoonBarycenter`] — exposed for callers that want
///   the raw barycenter without the Earth derivation step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Body {
    /// The Sun, barycentric.
    Sun,
    /// The Moon, default frame is geocentric (Earth-relative).
    Moon,
    /// Mercury, barycentric.
    Mercury,
    /// Venus, barycentric.
    Venus,
    /// Earth, derived from EMB and Moon; barycentric.
    Earth,
    /// Mars, barycentric.
    Mars,
    /// Jupiter, barycentric.
    Jupiter,
    /// Saturn, barycentric.
    Saturn,
    /// Uranus, barycentric.
    Uranus,
    /// Neptune, barycentric.
    Neptune,
    /// Pluto, barycentric.
    Pluto,
    /// Earth-Moon barycenter, barycentric.
    EarthMoonBarycenter,
}

impl Body {
    /// All bodies a v1 caller might compute, in astrologer order.
    /// Handy for "compute everything" UI flows.
    pub const ALL: &'static [Self] = &[
        Self::Sun,
        Self::Moon,
        Self::Mercury,
        Self::Venus,
        Self::Mars,
        Self::Jupiter,
        Self::Saturn,
        Self::Uranus,
        Self::Neptune,
        Self::Pluto,
    ];

    /// Default body list for heliocentric charts: Earth replaces the Sun
    /// (the Sun is the origin in heliocentric coordinates).
    pub const ALL_HELIOCENTRIC: &'static [Self] = &[
        Self::Earth,
        Self::Moon,
        Self::Mercury,
        Self::Venus,
        Self::Mars,
        Self::Jupiter,
        Self::Saturn,
        Self::Uranus,
        Self::Neptune,
        Self::Pluto,
    ];

    /// Map to the JPL on-disk slot. Returns `None` for [`Body::Earth`]
    /// (which has no direct slot — it is derived).
    #[must_use]
    pub const fn jpl_slot(self) -> Option<JplSlot> {
        Some(match self {
            Self::Sun => JplSlot::Sun,
            Self::Moon => JplSlot::Moon,
            Self::Mercury => JplSlot::Mercury,
            Self::Venus => JplSlot::Venus,
            Self::Mars => JplSlot::Mars,
            Self::Jupiter => JplSlot::Jupiter,
            Self::Saturn => JplSlot::Saturn,
            Self::Uranus => JplSlot::Uranus,
            Self::Neptune => JplSlot::Neptune,
            Self::Pluto => JplSlot::Pluto,
            Self::EarthMoonBarycenter => JplSlot::EarthMoonBarycenter,
            Self::Earth => return None,
        })
    }

    /// Human-readable name. Used by the CLI's text output and in
    /// diagnostic messages.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Sun => "Sun",
            Self::Moon => "Moon",
            Self::Mercury => "Mercury",
            Self::Venus => "Venus",
            Self::Earth => "Earth",
            Self::Mars => "Mars",
            Self::Jupiter => "Jupiter",
            Self::Saturn => "Saturn",
            Self::Uranus => "Uranus",
            Self::Neptune => "Neptune",
            Self::Pluto => "Pluto",
            Self::EarthMoonBarycenter => "EMB",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jpl_slot_indices_are_zero_through_twelve() {
        assert_eq!(JplSlot::Mercury.index(), 0);
        assert_eq!(JplSlot::Sun.index(), 10);
        assert_eq!(JplSlot::EarthNutations.index(), 11);
        assert_eq!(JplSlot::LunarLibration.index(), 12);
    }

    #[test]
    fn nutations_have_two_axes_others_have_three() {
        assert_eq!(JplSlot::EarthNutations.axes(), 2);
        for slot in [
            JplSlot::Mercury,
            JplSlot::Venus,
            JplSlot::EarthMoonBarycenter,
            JplSlot::Mars,
            JplSlot::Jupiter,
            JplSlot::Saturn,
            JplSlot::Uranus,
            JplSlot::Neptune,
            JplSlot::Pluto,
            JplSlot::Moon,
            JplSlot::Sun,
            JplSlot::LunarLibration,
        ] {
            assert_eq!(slot.axes(), 3, "{slot:?} should be 3-axis");
        }
    }

    #[test]
    fn earth_has_no_jpl_slot() {
        assert!(Body::Earth.jpl_slot().is_none());
    }

    #[test]
    fn classical_bodies_map_to_correct_slots() {
        assert_eq!(Body::Sun.jpl_slot(), Some(JplSlot::Sun));
        assert_eq!(Body::Moon.jpl_slot(), Some(JplSlot::Moon));
        assert_eq!(Body::Mercury.jpl_slot(), Some(JplSlot::Mercury));
        assert_eq!(Body::Pluto.jpl_slot(), Some(JplSlot::Pluto));
        assert_eq!(
            Body::EarthMoonBarycenter.jpl_slot(),
            Some(JplSlot::EarthMoonBarycenter)
        );
    }

    #[test]
    fn body_all_excludes_earth_and_emb() {
        assert!(!Body::ALL.contains(&Body::Earth));
        assert!(!Body::ALL.contains(&Body::EarthMoonBarycenter));
        assert_eq!(Body::ALL.len(), 10);
    }
}
