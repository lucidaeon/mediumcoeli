#![allow(clippy::cast_possible_truncation)]

//! House system calculations.
//!
//! Each system returns a [`HouseCusps`] — the twelve cusp longitudes in
//! radians \[0, TAU), indexed 0-based (index 0 = house 1 cusp = ASC,
//! index 9 = house 10 cusp = MC, etc.).

pub mod equal;
pub mod koch;
pub mod placidus;
pub mod porphyry;
pub mod regiomontanus;
pub mod whole_sign;

pub use equal::equal_as_rad;
pub use koch::koch_rad;
pub use placidus::placidus_rad;
pub use porphyry::porphyry_rad;
pub use regiomontanus::regiomontanus_rad;
pub use whole_sign::whole_sign_rad;

// =============================================================================
// `noref-houses` feature gate
// =============================================================================
//
// Every module declared under this gate is compiled only when the
// `noref-houses` Cargo feature is enabled. Default builds see nothing
// here. As each system gets refchart-oracle coverage, its `#[cfg]`
// attribute is removed in a focused promotion commit.

#[cfg(feature = "noref-houses")]
pub mod campanus;
#[cfg(feature = "noref-houses")]
pub use campanus::campanus_rad;

pub mod alcabitius;
pub use alcabitius::alcabitius_rad;

pub mod morinus;
pub use morinus::morinus_rad;

#[cfg(feature = "noref-houses")]
pub mod meridian;
#[cfg(feature = "noref-houses")]
pub use meridian::meridian_rad;

#[cfg(feature = "noref-houses")]
pub mod equal_mc;
#[cfg(feature = "noref-houses")]
pub use equal_mc::equal_mc_rad;

#[cfg(feature = "noref-houses")]
pub mod horizontal;
#[cfg(feature = "noref-houses")]
pub use horizontal::horizontal_rad;

#[cfg(feature = "noref-houses")]
pub mod topocentric;
#[cfg(feature = "noref-houses")]
pub use topocentric::topocentric_rad;

#[cfg(feature = "noref-houses")]
pub mod krusinski;
#[cfg(feature = "noref-houses")]
pub use krusinski::krusinski_rad;

#[cfg(feature = "noref-houses")]
pub mod sripati;
#[cfg(feature = "noref-houses")]
pub use sripati::sripati_rad;

#[cfg(feature = "noref-houses")]
pub mod vehlow;
#[cfg(feature = "noref-houses")]
pub use vehlow::vehlow_rad;

#[cfg(feature = "noref-houses")]
pub mod carter;
#[cfg(feature = "noref-houses")]
pub use carter::carter_rad;

#[cfg(feature = "noref-houses")]
pub mod pullen_sd;
#[cfg(feature = "noref-houses")]
pub use pullen_sd::pullen_sd_rad;

#[cfg(feature = "noref-houses")]
pub mod pullen_sr;
#[cfg(feature = "noref-houses")]
pub use pullen_sr::pullen_sr_rad;

use std::f64::consts::TAU;

/// Registry of supported house systems.
///
/// Each variant knows its own human-readable [`label`](HouseSystem::label),
/// URL-safe [`slug`](HouseSystem::slug), and how to [`compute`](HouseSystem::compute)
/// its cusps from the standard four inputs.
///
/// Systems gated behind `noref-houses` are available only when that Cargo
/// feature is enabled — they lack a reference-chart oracle and are not
/// included in the default test suite.
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HouseSystem {
    WholeSign,
    EqualFromAsc,
    Placidus,
    Regiomontanus,
    Porphyry,
    Alcabitius,
    Morinus,
    Koch,
    #[cfg(feature = "noref-houses")]
    Campanus,
    #[cfg(feature = "noref-houses")]
    Meridian,
    #[cfg(feature = "noref-houses")]
    EqualFromMc,
    #[cfg(feature = "noref-houses")]
    Horizontal,
    #[cfg(feature = "noref-houses")]
    Topocentric,
    #[cfg(feature = "noref-houses")]
    Krusinski,
    #[cfg(feature = "noref-houses")]
    Sripati,
    #[cfg(feature = "noref-houses")]
    Vehlow,
    #[cfg(feature = "noref-houses")]
    Carter,
    #[cfg(feature = "noref-houses")]
    PullenSd,
    #[cfg(feature = "noref-houses")]
    PullenSr,
}

impl HouseSystem {
    /// The eight always-on house systems in canonical presentation order.
    pub const DEFAULT_SET: &'static [Self] = &[
        Self::WholeSign,
        Self::EqualFromAsc,
        Self::Placidus,
        Self::Regiomontanus,
        Self::Porphyry,
        Self::Alcabitius,
        Self::Morinus,
        Self::Koch,
    ];

    /// Human-readable display name for the house system.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::WholeSign => "Whole Sign",
            Self::EqualFromAsc => "Equal (from ASC)",
            Self::Placidus => "Placidus",
            Self::Regiomontanus => "Regiomontanus",
            Self::Porphyry => "Porphyry",
            Self::Alcabitius => "Alcabitius",
            Self::Morinus => "Morinus",
            Self::Koch => "Koch",
            #[cfg(feature = "noref-houses")]
            Self::Campanus => "Campanus",
            #[cfg(feature = "noref-houses")]
            Self::Meridian => "Meridian",
            #[cfg(feature = "noref-houses")]
            Self::EqualFromMc => "Equal (from MC)",
            #[cfg(feature = "noref-houses")]
            Self::Horizontal => "Horizontal",
            #[cfg(feature = "noref-houses")]
            Self::Topocentric => "Topocentric (Polich-Page)",
            #[cfg(feature = "noref-houses")]
            Self::Krusinski => "Krusinski-Pisa-Goeldi",
            #[cfg(feature = "noref-houses")]
            Self::Sripati => "Sripati",
            #[cfg(feature = "noref-houses")]
            Self::Vehlow => "Vehlow Equal",
            #[cfg(feature = "noref-houses")]
            Self::Carter => "Carter Poli-Equatorial",
            #[cfg(feature = "noref-houses")]
            Self::PullenSd => "Pullen (Sinusoidal Delta)",
            #[cfg(feature = "noref-houses")]
            Self::PullenSr => "Pullen (Sinusoidal Ratio)",
        }
    }

    /// URL-safe identifier for the house system.
    #[must_use]
    pub fn slug(self) -> &'static str {
        match self {
            Self::WholeSign => "whole_sign",
            Self::EqualFromAsc => "equal_asc",
            Self::Placidus => "placidus",
            Self::Regiomontanus => "regiomontanus",
            Self::Porphyry => "porphyry",
            Self::Alcabitius => "alcabitius",
            Self::Morinus => "morinus",
            Self::Koch => "koch",
            #[cfg(feature = "noref-houses")]
            Self::Campanus => "campanus",
            #[cfg(feature = "noref-houses")]
            Self::Meridian => "meridian",
            #[cfg(feature = "noref-houses")]
            Self::EqualFromMc => "equal_mc",
            #[cfg(feature = "noref-houses")]
            Self::Horizontal => "horizontal",
            #[cfg(feature = "noref-houses")]
            Self::Topocentric => "topocentric",
            #[cfg(feature = "noref-houses")]
            Self::Krusinski => "krusinski",
            #[cfg(feature = "noref-houses")]
            Self::Sripati => "sripati",
            #[cfg(feature = "noref-houses")]
            Self::Vehlow => "vehlow",
            #[cfg(feature = "noref-houses")]
            Self::Carter => "carter",
            #[cfg(feature = "noref-houses")]
            Self::PullenSd => "pullen_sd",
            #[cfg(feature = "noref-houses")]
            Self::PullenSr => "pullen_sr",
        }
    }

    /// Compute the twelve house cusps for this system.
    ///
    /// Returns `None` for systems that are undefined at circumpolar latitudes
    /// (Placidus, Regiomontanus, Koch, etc.) when the geometry degenerates.
    /// Infallible systems (Whole Sign, Equal, Porphyry, etc.) always return
    /// `Some`.
    ///
    /// # Parameters
    /// - `ramc_rad` — Right Ascension of the Midheaven (Local Apparent Sidereal Time) in radians.
    /// - `obliquity_rad` — true obliquity of the ecliptic in radians.
    /// - `ac_rad` — Ascendant longitude in radians.
    /// - `lat_rad` — geographic latitude of the observer in radians.
    #[must_use]
    pub fn compute(
        self,
        ramc_rad: f64,
        obliquity_rad: f64,
        ac_rad: f64,
        lat_rad: f64,
    ) -> Option<HouseCusps> {
        match self {
            Self::WholeSign => Some(whole_sign_rad(ac_rad)),
            Self::EqualFromAsc => Some(equal_as_rad(ac_rad)),
            Self::Placidus => placidus_rad(ramc_rad, obliquity_rad, lat_rad),
            Self::Regiomontanus => regiomontanus_rad(ramc_rad, obliquity_rad, lat_rad),
            Self::Porphyry => Some(porphyry_rad(
                ac_rad,
                crate::coords::mcic::mc_rad(ramc_rad, obliquity_rad),
            )),
            Self::Alcabitius => alcabitius_rad(ramc_rad, obliquity_rad, lat_rad),
            Self::Morinus => morinus_rad(ramc_rad, obliquity_rad, lat_rad),
            Self::Koch => koch_rad(ramc_rad, obliquity_rad, lat_rad),
            #[cfg(feature = "noref-houses")]
            Self::Campanus => campanus_rad(ramc_rad, obliquity_rad, lat_rad),
            #[cfg(feature = "noref-houses")]
            Self::Meridian => meridian_rad(ramc_rad, obliquity_rad, lat_rad),
            #[cfg(feature = "noref-houses")]
            Self::EqualFromMc => Some(equal_mc_rad(crate::coords::mcic::mc_rad(
                ramc_rad,
                obliquity_rad,
            ))),
            #[cfg(feature = "noref-houses")]
            Self::Horizontal => horizontal_rad(ramc_rad, obliquity_rad, lat_rad),
            #[cfg(feature = "noref-houses")]
            Self::Topocentric => topocentric_rad(ramc_rad, obliquity_rad, lat_rad),
            #[cfg(feature = "noref-houses")]
            Self::Krusinski => krusinski_rad(ramc_rad, obliquity_rad, lat_rad),
            #[cfg(feature = "noref-houses")]
            Self::Sripati => Some(sripati_rad(
                ac_rad,
                crate::coords::mcic::mc_rad(ramc_rad, obliquity_rad),
            )),
            #[cfg(feature = "noref-houses")]
            Self::Vehlow => Some(vehlow_rad(ac_rad)),
            #[cfg(feature = "noref-houses")]
            Self::Carter => Some(carter_rad(ac_rad, obliquity_rad)),
            #[cfg(feature = "noref-houses")]
            Self::PullenSd => Some(pullen_sd_rad(
                ac_rad,
                crate::coords::mcic::mc_rad(ramc_rad, obliquity_rad),
            )),
            #[cfg(feature = "noref-houses")]
            Self::PullenSr => Some(pullen_sr_rad(
                ac_rad,
                crate::coords::mcic::mc_rad(ramc_rad, obliquity_rad),
            )),
        }
    }
}

/// Twelve house cusp longitudes in radians \[0, TAU), 0-based.
///
/// Layout: `[H1=ASC, H2, H3, H4=IC, H5, H6, H7=DSC, H8, H9, H10=MC, H11, H12]`
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HouseCusps(pub [f64; 12]);

impl HouseCusps {
    /// Cusp longitude for house `n` (1-based), in radians.
    #[must_use]
    pub fn cusp(&self, n: u8) -> f64 {
        self.0[(n - 1) as usize]
    }

    /// Which house (1-based) contains the given longitude (radians)?
    #[must_use]
    pub fn house_of(&self, lon: f64) -> u8 {
        let lon = lon.rem_euclid(TAU);
        for h in 0..12_usize {
            let start = self.0[h];
            let end = self.0[(h + 1) % 12];
            let span = (end - start).rem_euclid(TAU);
            let dist = (lon - start).rem_euclid(TAU);
            if dist < span {
                return (h + 1) as u8;
            }
        }
        1 // fallback — should not occur for valid inputs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn house_system_default_set_is_eight_in_order() {
        use HouseSystem::*;
        assert_eq!(
            HouseSystem::DEFAULT_SET,
            &[
                WholeSign,
                EqualFromAsc,
                Placidus,
                Regiomontanus,
                Porphyry,
                Alcabitius,
                Morinus,
                Koch
            ]
        );
    }

    #[test]
    fn house_system_slug_and_label_roundtrip() {
        assert_eq!(HouseSystem::WholeSign.slug(), "whole_sign");
        assert_eq!(HouseSystem::WholeSign.label(), "Whole Sign");
        assert_eq!(HouseSystem::EqualFromAsc.slug(), "equal_asc");
        assert_eq!(HouseSystem::Alcabitius.slug(), "alcabitius");
    }

    #[test]
    fn house_system_compute_whole_sign_starts_at_sign_boundary() {
        // Leo Ascendant frame (≈125.33°); whole-sign H1 = start of Leo = 120°.
        let ramc = 0.5_f64; // any frame; whole-sign only uses ac_rad
        let ac = 125.33_f64.to_radians();
        let cusps = HouseSystem::WholeSign
            .compute(ramc, 0.4, ac, 0.6)
            .expect("whole sign always defined");
        assert!((cusps.cusp(1).to_degrees().rem_euclid(360.0) - 120.0).abs() < 1e-6);
    }

    #[test]
    fn compute_filter_two_systems_preserves_caller_order() {
        // Caller controls order; the enum does not reorder.
        let order = [HouseSystem::Placidus, HouseSystem::WholeSign];
        let slugs: Vec<_> = order.iter().map(|h| h.slug()).collect();
        assert_eq!(slugs, vec!["placidus", "whole_sign"]);
    }
}
