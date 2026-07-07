//! # jzod
//!
//! Typed model and serializer for the [JZOD] v0.0.0 astrology chart
//! interchange format. JZOD stores charts as JSON with `lower_snake_case`
//! keys; this crate is the single source of truth for the wire format so
//! that consuming libraries and CLIs do not each re-implement it.
//!
//! This crate is a dependency-free *leaf*: it knows nothing about ephemeris
//! computation or any specific source format. Consumers build the typed
//! model from their own domain types and serialize through
//! [`to_string_pretty`].
//!
//! [JZOD]: https://github.com/lucidaeon/mediumcoeli/blob/main/crates/jzod/JZOD.md

#![warn(missing_docs)]
#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]

pub mod ayanamsha;
pub mod chart;
pub mod coord;
pub mod document;
pub mod house;
pub mod placement;
pub mod time;
pub mod uid;

/// The JZOD wire-format version this crate emits. Distinct from the crate's
/// own package version.
pub const FORMAT_VERSION: &str = "0.0.0";

pub use chart::{
    Birth, Chart, ChartType, CoordinateSystem, Datetime, DraconicNode, Ephemeris, Location,
    LunarPhase, LunarPhaseName, Name, Sect, SiderealFrame, Tithi, Zodiac,
};
pub use coord::{Degrees8, Position, Sign};
pub use document::JzodDocument;
pub use house::{HouseCusp, HouseSystemCusps, Houses};
pub use placement::{Angle, AngleId, Body, BodyId, Lot, LotId, Placements, Point, PointId};
pub use uid::{UidSeed, derive_uid, random_uid};

/// Serialize a JZOD document as pretty-printed JSON.
///
/// # Panics
///
/// Never in practice — `serde_json` only fails serializing non-finite floats,
/// which the typed model constrains away at construction.
#[must_use]
pub fn to_string_pretty(doc: &JzodDocument) -> String {
    serde_json::to_string_pretty(doc).expect("JZOD serialization is infallible")
}

/// Parse a JZOD document from a JSON string.
///
/// # Errors
///
/// Returns the underlying [`serde_json::Error`] if `s` is not valid JSON or
/// does not match the JZOD shape.
pub fn from_str(s: &str) -> Result<JzodDocument, serde_json::Error> {
    serde_json::from_str(s)
}

#[cfg(test)]
mod version_tests {
    use super::*;

    #[test]
    fn format_version_is_pinned() {
        assert_eq!(FORMAT_VERSION, "0.0.0");
    }
}
