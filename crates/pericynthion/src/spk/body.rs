//! Astrologer-name ↔ NAIF id mapping for the headline asteroids.
//!
//! JPL distributes asteroid ephemerides (e.g. `sb441-n16.bsp`,
//! `sb441-n373.bsp`) keyed by **NAIF integer id**, where the id of a
//! numbered minor planet is
//!
//! ```text
//! NAIF id = 2_000_000 + MPC number
//! ```
//!
//! So Ceres (MPC 1) is `2000001`, Pallas (2) is `2000002`, Juno (3) is
//! `2000003`, Vesta (4) is `2000004`, and Hygiea (10) is `2000010`.
//! This module names the headline bodies an astrologer reaches for and
//! resolves them to the NAIF ids that [`crate::spk::SpkEphemeris::state`]
//! expects.

/// A headline asteroid carried in the JPL small-body ephemerides.
///
/// Each variant maps to a NAIF integer id via [`Asteroid::naif_id`]
/// (`2_000_000 + MPC number`). The set covers the classical "big four"
/// (Ceres, Pallas, Juno, Vesta) plus Hygiea, the largest remaining
/// main-belt body — the bodies most commonly requested in astrological
/// work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Asteroid {
    /// 1 Ceres — NAIF `2000001`.
    Ceres,
    /// 2 Pallas — NAIF `2000002`.
    Pallas,
    /// 3 Juno — NAIF `2000003`.
    Juno,
    /// 4 Vesta — NAIF `2000004`.
    Vesta,
    /// 10 Hygiea — NAIF `2000010`.
    Hygiea,
}

impl Asteroid {
    /// Every [`Asteroid`] variant, in MPC-number order.
    pub const ALL: &'static [Asteroid] = &[
        Asteroid::Ceres,
        Asteroid::Pallas,
        Asteroid::Juno,
        Asteroid::Vesta,
        Asteroid::Hygiea,
    ];

    /// The NAIF integer id for this asteroid (`2_000_000 + MPC number`),
    /// suitable for [`crate::spk::SpkEphemeris::state`].
    #[must_use]
    pub const fn naif_id(self) -> i32 {
        match self {
            Asteroid::Ceres => 2_000_001,
            Asteroid::Pallas => 2_000_002,
            Asteroid::Juno => 2_000_003,
            Asteroid::Vesta => 2_000_004,
            Asteroid::Hygiea => 2_000_010,
        }
    }

    /// The display name of this asteroid (capitalised, no MPC number).
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Asteroid::Ceres => "Ceres",
            Asteroid::Pallas => "Pallas",
            Asteroid::Juno => "Juno",
            Asteroid::Vesta => "Vesta",
            Asteroid::Hygiea => "Hygiea",
        }
    }

    /// Resolve a NAIF integer id to an [`Asteroid`], the inverse of
    /// [`Asteroid::naif_id`].
    ///
    /// Returns `None` for any id outside the headline set.
    #[must_use]
    pub const fn from_naif(naif_id: i32) -> Option<Asteroid> {
        match naif_id {
            2_000_001 => Some(Asteroid::Ceres),
            2_000_002 => Some(Asteroid::Pallas),
            2_000_003 => Some(Asteroid::Juno),
            2_000_004 => Some(Asteroid::Vesta),
            2_000_010 => Some(Asteroid::Hygiea),
            _ => None,
        }
    }

    /// Resolve an astrologer-facing slug to an [`Asteroid`], case-insensitively.
    ///
    /// Returns `None` for any name outside the headline set. Matching is
    /// case-insensitive and allocation-free (no heap allocation per call).
    #[must_use]
    pub fn from_slug(slug: &str) -> Option<Asteroid> {
        if slug.eq_ignore_ascii_case("ceres") {
            Some(Asteroid::Ceres)
        } else if slug.eq_ignore_ascii_case("pallas") {
            Some(Asteroid::Pallas)
        } else if slug.eq_ignore_ascii_case("juno") {
            Some(Asteroid::Juno)
        } else if slug.eq_ignore_ascii_case("vesta") {
            Some(Asteroid::Vesta)
        } else if slug.eq_ignore_ascii_case("hygiea") {
            Some(Asteroid::Hygiea)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Asteroid;

    #[test]
    fn naif_ids_follow_mpc_offset() {
        assert_eq!(Asteroid::Ceres.naif_id(), 2_000_001);
        assert_eq!(Asteroid::Pallas.naif_id(), 2_000_002);
        assert_eq!(Asteroid::Juno.naif_id(), 2_000_003);
        assert_eq!(Asteroid::Vesta.naif_id(), 2_000_004);
        assert_eq!(Asteroid::Hygiea.naif_id(), 2_000_010);
    }

    #[test]
    fn from_naif_inverts_naif_id() {
        for a in Asteroid::ALL {
            assert_eq!(Asteroid::from_naif(a.naif_id()), Some(*a));
        }
        assert_eq!(Asteroid::from_naif(2_000_999), None);
        assert_eq!(Asteroid::from_naif(399), None);
    }

    #[test]
    fn from_slug_is_case_insensitive() {
        assert_eq!(Asteroid::from_slug("vesta"), Some(Asteroid::Vesta));
        assert_eq!(Asteroid::from_slug("CERES"), Some(Asteroid::Ceres));
        assert_eq!(Asteroid::from_slug("Hygiea"), Some(Asteroid::Hygiea));
        assert_eq!(Asteroid::from_slug("pluto"), None);
    }

    #[test]
    fn vesta_round_trips_slug_and_naif() {
        let v = Asteroid::from_slug("vesta").unwrap();
        assert_eq!(v.naif_id(), 2_000_004);
        assert_eq!(v.name(), "Vesta");
    }

    #[test]
    fn all_covers_every_variant_in_mpc_order() {
        let ids: Vec<i32> = Asteroid::ALL.iter().map(|a| a.naif_id()).collect();
        assert_eq!(
            ids,
            vec![2_000_001, 2_000_002, 2_000_003, 2_000_004, 2_000_010]
        );
    }
}
