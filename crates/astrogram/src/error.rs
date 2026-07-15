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
#[derive(Debug, Error)]
pub enum ChartError {
    #[error("longitude {0} out of range -180..=180")]
    LongitudeOutOfRange(f64),
    #[error("latitude {0} out of range -90..=90")]
    LatitudeOutOfRange(f64),
    /// A format was used in a direction it does not support (e.g. reading a
    /// write-only format, writing a read-only format, or passing a web format
    /// to a file-bytes function).
    #[error("{0}")]
    UnsupportedDirection(&'static str),
    /// Bytes could not be decoded as UTF-8 text required by the parser.
    #[error("invalid UTF-8: {0}")]
    Utf8(#[from] std::str::Utf8Error),
    /// A parse-level error reported by a format parser.
    #[error("{0}")]
    Parse(String),
    /// Filesystem access failed (reading a chart file or scanning a directory).
    #[error("I/O error on {path:?}: {source}")]
    Io {
        /// The path whose access failed.
        path: std::path::PathBuf,
        /// The underlying OS error.
        #[source]
        source: std::io::Error,
    },
    /// A format requiring a `jzod::Generator` identity (currently only
    /// [`Format::Json`](crate::format::Format::Json)) was written without one.
    #[error("JSON output requires a generator")]
    MissingGenerator,
}
