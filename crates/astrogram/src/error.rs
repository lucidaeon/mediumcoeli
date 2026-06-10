//! Error types shared across `astrogram` parsers.

use thiserror::Error;

/// Errors that can arise while parsing format-specific input.
#[allow(missing_docs)]
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ParseError {
    #[error("input truncated: needed {needed} bytes, got {got}")]
    Truncated { needed: usize, got: usize },

    #[error("bad record marker 0x{got:04X} at offset 0x{offset:04X}")]
    BadMarker { offset: usize, got: u16 },

    #[error("coordinate out of range in record at offset 0x{offset:04X}")]
    CoordinateOutOfRange { offset: usize },

    #[error("invalid record on line {line}: {reason}")]
    InvalidRecord { line: usize, reason: String },

    #[error("ADB XML: {0}")]
    Xml(String),

    #[error("ADB XML entry {adb_id}: {reason}")]
    AdbEntry { adb_id: u32, reason: String },
}

/// Errors that can arise when constructing canonical chart values.
#[allow(missing_docs)]
#[derive(Debug, Clone, PartialEq, Error)]
pub enum ChartError {
    #[error("longitude {0} out of range -180..=180")]
    LongitudeOutOfRange(f64),
    #[error("latitude {0} out of range -90..=90")]
    LatitudeOutOfRange(f64),
}
