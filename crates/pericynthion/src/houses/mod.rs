#![allow(clippy::cast_possible_truncation)]

//! House system calculations.
//!
//! Each system returns a [`HouseCusps`] — the twelve cusp longitudes in
//! radians \[0, TAU), indexed 0-based (index 0 = house 1 cusp = ASC,
//! index 9 = house 10 cusp = MC, etc.).

pub mod equal;
pub mod placidus;
pub mod porphyry;
pub mod regiomontanus;
pub mod whole_sign;

pub use equal::equal_as_rad;
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
// attribute is removed in a focused promotion commit — see
// docs/discovery/HOUSE_PROMOTION.md.

#[cfg(feature = "noref-houses")]
pub mod koch;
#[cfg(feature = "noref-houses")]
pub use koch::koch_rad;

#[cfg(feature = "noref-houses")]
pub mod campanus;
#[cfg(feature = "noref-houses")]
pub use campanus::campanus_rad;

#[cfg(feature = "noref-houses")]
pub mod alcabitius;
#[cfg(feature = "noref-houses")]
pub use alcabitius::alcabitius_rad;

#[cfg(feature = "noref-houses")]
pub mod morinus;
#[cfg(feature = "noref-houses")]
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

use std::f64::consts::TAU;

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
