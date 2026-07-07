//! Error type hierarchy for pericynthion.
//!
//! Each layer (header parsing, binary reading, time conversion, coordinate
//! transformation) emits its own variant so callers can match on the
//! specific failure mode without parsing strings. All variants carry
//! enough context — file paths, byte offsets, JD values, group numbers —
//! to be diagnosable without re-running with extra logging.

use std::path::PathBuf;
use thiserror::Error;

/// Top-level error for pericynthion operations.
///
/// Specific subsystems may also expose their own focused error types
/// (e.g. [`HeaderError`]) which convert into this top-level variant.
#[derive(Debug, Error)]
pub enum PericynthionError {
    /// Failure parsing or interpreting a JPL DE-series ASCII header file.
    #[error("JPL header error: {0}")]
    Header(#[from] HeaderError),

    /// I/O failure (file not found, permission denied, read truncated).
    #[error("I/O error reading {path:?}: {source}")]
    Io {
        /// Path of the file we tried to access.
        path: PathBuf,
        /// Underlying OS error.
        source: std::io::Error,
    },

    /// An ayanamsha slug passed to `to_jzod_chart` was not found in the
    /// built-in registry.
    #[error("unknown ayanamsha slug '{slug}'; known: {known}")]
    UnknownAyanamshaSlug {
        /// The slug that was not recognized.
        slug: String,
        /// Comma-separated list of known slugs from the registry.
        known: String,
    },

    /// A draconic chart was requested but no lunar-node longitude was available
    /// to rotate by (node ephemeris missing).
    #[error(
        "draconic zodiac requested but no lunar-node longitude is available (node ephemeris missing)"
    )]
    DraconicNodeUnavailable,
}

/// Failures that can occur while parsing a JPL DE-series ASCII header.
///
/// All variants carry the byte offset (line, 1-indexed) where the
/// problem was detected and the group context if known.
#[derive(Debug, Error)]
pub enum HeaderError {
    /// The first header line is expected to declare `KSIZE=` and
    /// `NCOEFF=`. This variant is emitted if either keyword is missing
    /// or the value cannot be parsed as a positive integer.
    #[error("invalid KSIZE/NCOEFF declaration at line {line}: {detail}")]
    InvalidSizeDeclaration {
        /// Line number (1-indexed) where the bad declaration was found.
        line: usize,
        /// Human-readable diagnostic.
        detail: String,
    },

    /// A `GROUP NNNN` marker was expected but not found.
    #[error("expected `GROUP {expected}` marker, found {found:?} at line {line}")]
    MissingGroup {
        /// Group ID we expected to see next.
        expected: u32,
        /// Actual content of the line (truncated for diagnostics).
        found: String,
        /// 1-indexed line number.
        line: usize,
    },

    /// A numeric field could not be parsed.
    ///
    /// JPL uses Fortran-style scientific notation: `0.441000000000000000D+03`.
    /// The parser translates `D` → `e` before delegating to the standard
    /// floating-point parser; this variant fires if the result still
    /// can't be interpreted.
    #[error("invalid numeric value at line {line} in GROUP {group}: {raw:?}")]
    InvalidNumber {
        /// Group context where the bad number appeared.
        group: u32,
        /// 1-indexed line number.
        line: usize,
        /// Raw token that failed to parse.
        raw: String,
    },

    /// The constant-name count in GROUP 1040 does not match the
    /// constant-value count in GROUP 1041.
    #[error("constant-name/value count mismatch: {names} names vs {values} values")]
    ConstantCountMismatch {
        /// Number of constants declared in GROUP 1040.
        names: usize,
        /// Number of constants declared in GROUP 1041.
        values: usize,
    },

    /// GROUP 1050 was expected to contain exactly three rows (offsets,
    /// coefficients-per-axis, subgranules) of the same length. This
    /// variant fires when the row count or row widths don't match.
    #[error("invalid body-layout block in GROUP 1050: {detail}")]
    InvalidLayout {
        /// Human-readable diagnostic describing the mismatch.
        detail: String,
    },

    /// End-of-input reached unexpectedly inside a group.
    #[error("unexpected end of header inside GROUP {group}")]
    UnexpectedEnd {
        /// Group context at the point of truncation.
        group: u32,
    },
}
