//! Parser for the JPL DE-series ASCII header file (`header.NNN`).
//!
//! # Format overview
//!
//! The header is a plain-ASCII file shipped alongside the binary
//! ephemeris (`linux_*.NNN`, `ascp*.NNN`, etc.). It describes:
//!
//! - The on-disk record size (`KSIZE`, in 4-byte single-precision words)
//!   and the same value expressed as 8-byte doubles (`NCOEFF` = `KSIZE`/2).
//! - The time span covered by the ephemeris and the size of each
//!   coefficient record's "granule" (almost always 32 days for DE-series).
//! - A symbol table of named physical constants (Earth-Moon mass ratio,
//!   speed of light, astronomical unit, planetary masses, etc.).
//! - The per-body layout within a single coefficient record: where each
//!   body's Chebyshev coefficients begin (1-indexed offset into the
//!   double-precision word array), how many coefficients per axis per
//!   sub-granule, and how many sub-granules per record.
//!
//! The file is organized as a sequence of `GROUP NNNN` blocks, each
//! terminated by a blank line:
//!
//! ```text
//! KSIZE= 2036    NCOEFF= 1018
//!
//! GROUP   1010
//!
//! JPL Planetary Ephemeris DE441/LE441
//! Start Epoch: JED= -3100015.5-13200-AUG-15 00:00:00
//! Final Epoch: JED=  8000016.5 17191-MAR-15 00:00:00
//!
//! GROUP   1030
//!
//!  -3100015.50  8000016.50         32.
//!
//! GROUP   1040
//!
//!    645
//!   DENUM   LENUM   TDATEF  TDATEB  JDEPOC  CENTER  CLIGHT  BETA    GAMMA   AU
//!   ... (constant names, 6 per line)
//!
//! GROUP   1041
//!
//!    645
//!   0.441000000000000000D+03  0.441000000000000000D+03  0.200818112044000000D+12
//!   ... (constant values, 3 per line)
//!
//! GROUP   1050
//!
//!      3   171   231   309   342   366   387   405   423   441   753   819   899  1019  1019
//!     14    10    13    11     8     7     6     6     6    13    11    10    10     0     0
//!      4     2     2     1     1     1     1     1     1     8     2     4     4     0     0
//!
//! GROUP   1070
//!
//! ```
//!
//! Numeric values use Fortran-style D-exponent notation (`0.441D+03`)
//! which we translate to Rust's `e`-exponent form before parsing.
//!
//! # The body-layout table (GROUP 1050)
//!
//! Each column corresponds to one body slot. DE441 ships 13 active slots
//! (plus 2 reserved slots filled with zeros, present so older readers
//! that assume a fixed 15-column layout don't crash). The active slots are:
//!
//! | Slot | Body                       | Frame                  | Axes |
//! |------|----------------------------|------------------------|------|
//! | 1    | Mercury                    | Solar-System barycentric | 3 |
//! | 2    | Venus                      | Solar-System barycentric | 3 |
//! | 3    | Earth-Moon barycenter      | Solar-System barycentric | 3 |
//! | 4    | Mars                       | Solar-System barycentric | 3 |
//! | 5    | Jupiter                    | Solar-System barycentric | 3 |
//! | 6    | Saturn                     | Solar-System barycentric | 3 |
//! | 7    | Uranus                     | Solar-System barycentric | 3 |
//! | 8    | Neptune                    | Solar-System barycentric | 3 |
//! | 9    | Pluto                      | Solar-System barycentric | 3 |
//! | 10   | Moon                       | Geocentric (Earth-relative) | 3 |
//! | 11   | Sun                        | Solar-System barycentric | 3 |
//! | 12   | Earth nutations (Δψ, Δε)   | —                      | 2 |
//! | 13   | Lunar mantle libration     | —                      | 3 |
//!
//! Note that **Earth has no slot**: its position is computed from EMB
//! and Moon via the Earth-Moon mass ratio (`EMRAT`).

use crate::error::HeaderError;
use std::collections::BTreeMap;

/// Parsed contents of a JPL DE-series ASCII header.
#[derive(Debug, Clone)]
pub struct Header {
    /// Record size in single-precision (4-byte) words. Equal to
    /// `2 * NCOEFF`. For DE441: 2036.
    pub ksize: u32,
    /// Record size in double-precision (8-byte) words. The binary file
    /// is a flat array of `NCOEFF`-double records. For DE441: 1018.
    pub ncoeff: u32,
    /// Free-form description lines from GROUP 1010 (typically 3 lines:
    /// title, start epoch, final epoch).
    pub title: Vec<String>,
    /// Span covered by the ephemeris, expressed as Julian Dates in
    /// Terrestrial Time, plus the granule size in days.
    pub epoch: EpochSpan,
    /// Named physical constants from GROUPs 1040/1041. Sorted for
    /// deterministic iteration.
    pub constants: BTreeMap<String, f64>,
    /// Per-body layout table from GROUP 1050.
    pub layout: BodyLayoutTable,
}

/// Time span and granule size declared by a JPL header (GROUP 1030).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EpochSpan {
    /// First instant covered by the ephemeris (TT Julian Date).
    pub start_jd: f64,
    /// Last instant covered (TT Julian Date).
    pub end_jd: f64,
    /// Granule size in days. One coefficient record spans this many days.
    /// DE-series uses 32 days for all versions to date.
    pub granule_days: f64,
}

/// Per-body layout from GROUP 1050.
///
/// The three vectors have identical length (typically 15 for DE441-era
/// files: 13 active slots + 2 reserved-zero slots).
#[derive(Debug, Clone, PartialEq)]
pub struct BodyLayoutTable {
    /// 1-indexed offset into a coefficient record where each body's
    /// coefficients begin. Convert to 0-indexed for Rust slicing.
    pub offsets: Vec<u32>,
    /// Number of Chebyshev coefficients per coordinate axis per
    /// sub-granule. Zero means the slot is unused.
    pub coeffs_per_axis: Vec<u32>,
    /// Number of sub-granules per record. The body is fitted to this
    /// many shorter polynomials within each 32-day granule; faster
    /// bodies (Mercury, Moon) use more sub-granules. Zero means unused.
    pub subgranules: Vec<u32>,
}

impl BodyLayoutTable {
    /// How many words a body occupies in one full record:
    /// `coeffs_per_axis × axes × subgranules`. The number of axes is
    /// supplied by the caller (3 for positional bodies, 2 for Earth
    /// nutations, 3 for lunar libration) since the header itself does
    /// not declare it.
    ///
    /// Returns `None` if the slot index is out of range, or `Some(0)`
    /// for unused slots.
    #[must_use]
    pub fn record_words(&self, slot: usize, axes: u32) -> Option<u32> {
        let coeffs = *self.coeffs_per_axis.get(slot)?;
        let subgr = *self.subgranules.get(slot)?;
        Some(coeffs * axes * subgr)
    }

    /// Number of body slots described (active + reserved).
    #[must_use]
    pub fn len(&self) -> usize {
        self.offsets.len()
    }

    /// True if no slots were parsed (parser error indicator only —
    /// real headers always have ≥13 slots).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.offsets.is_empty()
    }
}

/// Parse a JPL DE-series ASCII header file into a [`Header`] value.
///
/// # Errors
///
/// Returns a [`HeaderError`] describing the first malformed token,
/// with line number and group context for diagnostics. The parser
/// stops at the first failure rather than attempting recovery — header
/// files are short and machine-generated, so partial-parse semantics
/// would only hide real upstream problems.
pub fn parse(source: &str) -> Result<Header, HeaderError> {
    let mut lines = LineCursor::new(source);

    let (ksize, ncoeff) = parse_size_declaration(&mut lines)?;
    expect_group(&mut lines, 1010)?;
    let title = read_until_group(&mut lines);

    expect_group(&mut lines, 1030)?;
    let epoch = parse_epoch_span(&mut lines)?;

    expect_group(&mut lines, 1040)?;
    let names = parse_constant_names(&mut lines)?;

    expect_group(&mut lines, 1041)?;
    let values = parse_constant_values(&mut lines)?;

    if names.len() != values.len() {
        return Err(HeaderError::ConstantCountMismatch {
            names: names.len(),
            values: values.len(),
        });
    }
    let constants: BTreeMap<String, f64> = names.into_iter().zip(values).collect();

    expect_group(&mut lines, 1050)?;
    let layout = parse_body_layout(&mut lines)?;

    // GROUP 1070 is the terminator. Absence is not a hard error —
    // some derivatives strip it.
    let _ = expect_group(&mut lines, 1070);

    Ok(Header {
        ksize,
        ncoeff,
        title,
        epoch,
        constants,
        layout,
    })
}

// =============================================================================
// Cursor helpers
// =============================================================================

/// Internal iterator over input lines with a 1-indexed line counter.
/// Tracks position so error messages can pinpoint the offending line.
struct LineCursor<'a> {
    iter: std::str::Lines<'a>,
    line_no: usize,
    peeked: Option<&'a str>,
}

impl<'a> LineCursor<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            iter: source.lines(),
            line_no: 0,
            peeked: None,
        }
    }

    /// Advance to and return the next line, incrementing the line counter.
    fn next(&mut self) -> Option<&'a str> {
        if let Some(line) = self.peeked.take() {
            self.line_no += 1;
            return Some(line);
        }
        self.iter.next().inspect(|_| self.line_no += 1)
    }

    /// Peek the next line without advancing.
    fn peek(&mut self) -> Option<&'a str> {
        if self.peeked.is_none() {
            self.peeked = self.iter.next();
        }
        self.peeked
    }

    /// Skip consecutive blank lines.
    fn skip_blank(&mut self) {
        while matches!(self.peek(), Some(line) if line.trim().is_empty()) {
            self.next();
        }
    }

    fn line(&self) -> usize {
        self.line_no
    }
}

// =============================================================================
// Section parsers
// =============================================================================

/// Parse the first non-blank line: `KSIZE= NNNN    NCOEFF= NNNN`.
fn parse_size_declaration(lines: &mut LineCursor) -> Result<(u32, u32), HeaderError> {
    lines.skip_blank();
    let line = lines.next().ok_or(HeaderError::InvalidSizeDeclaration {
        line: 0,
        detail: "header is empty".into(),
    })?;
    let line_no = lines.line();

    let ksize =
        extract_keyed_uint(line, "KSIZE").ok_or_else(|| HeaderError::InvalidSizeDeclaration {
            line: line_no,
            detail: format!("no KSIZE= in {line:?}"),
        })?;
    let ncoeff =
        extract_keyed_uint(line, "NCOEFF").ok_or_else(|| HeaderError::InvalidSizeDeclaration {
            line: line_no,
            detail: format!("no NCOEFF= in {line:?}"),
        })?;
    Ok((ksize, ncoeff))
}

/// Extract a `KEY= NNNN` pair from a line (case-sensitive).
fn extract_keyed_uint(line: &str, key: &str) -> Option<u32> {
    let idx = line.find(key)?;
    let rest = &line[idx + key.len()..];
    let after_eq = rest.strip_prefix('=')?;
    let token = after_eq.split_whitespace().next()?;
    token.parse().ok()
}

/// Consume blank lines, then require a `GROUP NNNN` marker.
fn expect_group(lines: &mut LineCursor, expected: u32) -> Result<(), HeaderError> {
    lines.skip_blank();
    let line = lines.next().ok_or(HeaderError::MissingGroup {
        expected,
        found: "<eof>".into(),
        line: lines.line(),
    })?;
    let line_no = lines.line();
    let trimmed = line.trim();
    let rest = trimmed
        .strip_prefix("GROUP")
        .ok_or_else(|| HeaderError::MissingGroup {
            expected,
            found: trimmed.to_string(),
            line: line_no,
        })?;
    let id: u32 = rest.trim().parse().map_err(|_| HeaderError::MissingGroup {
        expected,
        found: trimmed.to_string(),
        line: line_no,
    })?;
    if id != expected {
        return Err(HeaderError::MissingGroup {
            expected,
            found: format!("GROUP {id}"),
            line: line_no,
        });
    }
    Ok(())
}

/// Read non-blank lines until the next `GROUP` marker (which is not
/// consumed). Used for the free-form title block.
fn read_until_group(lines: &mut LineCursor) -> Vec<String> {
    let mut out = Vec::new();
    loop {
        lines.skip_blank();
        match lines.peek() {
            Some(line) if line.trim_start().starts_with("GROUP") => break,
            Some(_) => {
                let line = lines.next().unwrap();
                let trimmed = line.trim().to_string();
                if !trimmed.is_empty() {
                    out.push(trimmed);
                }
            }
            None => break,
        }
    }
    out
}

/// Parse GROUP 1030's single data line: `start_jd end_jd granule_days`.
fn parse_epoch_span(lines: &mut LineCursor) -> Result<EpochSpan, HeaderError> {
    lines.skip_blank();
    let line = lines
        .next()
        .ok_or(HeaderError::UnexpectedEnd { group: 1030 })?;
    let line_no = lines.line();
    let mut tokens = line.split_whitespace();
    let start_jd = parse_jpl_float(tokens.next().unwrap_or(""), 1030, line_no)?;
    let end_jd = parse_jpl_float(tokens.next().unwrap_or(""), 1030, line_no)?;
    let granule_days = parse_jpl_float(tokens.next().unwrap_or(""), 1030, line_no)?;
    Ok(EpochSpan {
        start_jd,
        end_jd,
        granule_days,
    })
}

/// Parse GROUP 1040: count line, then count-many constant names.
fn parse_constant_names(lines: &mut LineCursor) -> Result<Vec<String>, HeaderError> {
    lines.skip_blank();
    let line = lines
        .next()
        .ok_or(HeaderError::UnexpectedEnd { group: 1040 })?;
    let line_no = lines.line();
    let count: usize = line
        .trim()
        .parse()
        .map_err(|_| HeaderError::InvalidNumber {
            group: 1040,
            line: line_no,
            raw: line.trim().to_string(),
        })?;
    let mut names = Vec::with_capacity(count);
    while names.len() < count {
        let line = lines
            .next()
            .ok_or(HeaderError::UnexpectedEnd { group: 1040 })?;
        for tok in line.split_whitespace() {
            names.push(tok.to_string());
            if names.len() == count {
                break;
            }
        }
    }
    Ok(names)
}

/// Parse GROUP 1041: count line, then count-many Fortran-D-notation
/// floating-point values.
fn parse_constant_values(lines: &mut LineCursor) -> Result<Vec<f64>, HeaderError> {
    lines.skip_blank();
    let line = lines
        .next()
        .ok_or(HeaderError::UnexpectedEnd { group: 1041 })?;
    let line_no = lines.line();
    let count: usize = line
        .trim()
        .parse()
        .map_err(|_| HeaderError::InvalidNumber {
            group: 1041,
            line: line_no,
            raw: line.trim().to_string(),
        })?;
    let mut values = Vec::with_capacity(count);
    while values.len() < count {
        let line = lines
            .next()
            .ok_or(HeaderError::UnexpectedEnd { group: 1041 })?;
        let line_no = lines.line();
        for tok in line.split_whitespace() {
            values.push(parse_jpl_float(tok, 1041, line_no)?);
            if values.len() == count {
                break;
            }
        }
    }
    Ok(values)
}

/// Parse GROUP 1050: exactly three whitespace-separated integer rows
/// of equal length.
fn parse_body_layout(lines: &mut LineCursor) -> Result<BodyLayoutTable, HeaderError> {
    let row1 = read_int_row(lines, 1050)?;
    let row2 = read_int_row(lines, 1050)?;
    let row3 = read_int_row(lines, 1050)?;
    if row1.len() != row2.len() || row2.len() != row3.len() {
        return Err(HeaderError::InvalidLayout {
            detail: format!(
                "row length mismatch: offsets={}, coeffs={}, subgranules={}",
                row1.len(),
                row2.len(),
                row3.len()
            ),
        });
    }
    if row1.is_empty() {
        return Err(HeaderError::InvalidLayout {
            detail: "empty layout block".into(),
        });
    }
    Ok(BodyLayoutTable {
        offsets: row1,
        coeffs_per_axis: row2,
        subgranules: row3,
    })
}

fn read_int_row(lines: &mut LineCursor, group: u32) -> Result<Vec<u32>, HeaderError> {
    lines.skip_blank();
    let line = lines.next().ok_or(HeaderError::UnexpectedEnd { group })?;
    let line_no = lines.line();
    line.split_whitespace()
        .map(|tok| {
            tok.parse::<u32>().map_err(|_| HeaderError::InvalidNumber {
                group,
                line: line_no,
                raw: tok.to_string(),
            })
        })
        .collect()
}

/// Parse a Fortran D-notation float: `0.441000000000000000D+03` → 441.0.
///
/// Accepts `D`, `d`, `E`, or `e` as the exponent marker. Empty input
/// yields the named `InvalidNumber` error.
fn parse_jpl_float(token: &str, group: u32, line: usize) -> Result<f64, HeaderError> {
    if token.is_empty() {
        return Err(HeaderError::InvalidNumber {
            group,
            line,
            raw: String::new(),
        });
    }
    let translated: String = token
        .chars()
        .map(|c| match c {
            'D' | 'd' => 'e',
            other => other,
        })
        .collect();
    translated.parse().map_err(|_| HeaderError::InvalidNumber {
        group,
        line,
        raw: token.to_string(),
    })
}

// =============================================================================
// Unit tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_abs_diff_eq;

    /// Minimal but syntactically complete header for unit-testing the
    /// parser without touching the 22 KB real file. Constants count is
    /// 2 so we can verify alignment without volume.
    const MINI_HEADER: &str = "\
KSIZE= 2036    NCOEFF= 1018

GROUP   1010

JPL Planetary Ephemeris DE441/LE441 (test fixture)
Start Epoch: JED= -3100015.5
Final Epoch: JED=  8000016.5

GROUP   1030

 -3100015.50  8000016.50         32.

GROUP   1040

     2
  AU      EMRAT

GROUP   1041

     2
  0.149597870699999988D+09  0.813005682214972154D+02

GROUP   1050

     3   171   231
    14    10    13
     4     2     2

GROUP   1070
";

    #[test]
    fn parses_size_declaration() {
        let header = parse(MINI_HEADER).expect("mini header should parse");
        assert_eq!(header.ksize, 2036);
        assert_eq!(header.ncoeff, 1018);
    }

    #[test]
    fn parses_title_block() {
        let header = parse(MINI_HEADER).expect("mini header should parse");
        assert_eq!(header.title.len(), 3);
        assert!(header.title[0].contains("DE441"));
        assert!(header.title[1].starts_with("Start Epoch"));
        assert!(header.title[2].starts_with("Final Epoch"));
    }

    #[test]
    fn parses_epoch_span() {
        let header = parse(MINI_HEADER).expect("mini header should parse");
        assert_abs_diff_eq!(header.epoch.start_jd, -3_100_015.5, epsilon = 1e-9);
        assert_abs_diff_eq!(header.epoch.end_jd, 8_000_016.5, epsilon = 1e-9);
        assert_abs_diff_eq!(header.epoch.granule_days, 32.0, epsilon = 1e-12);
    }

    #[test]
    fn parses_constants() {
        let header = parse(MINI_HEADER).expect("mini header should parse");
        assert_eq!(header.constants.len(), 2);
        assert_abs_diff_eq!(header.constants["AU"], 149_597_870.7, epsilon = 1e-3);
        assert_abs_diff_eq!(
            header.constants["EMRAT"],
            81.300_568_221_497_22,
            epsilon = 1e-9
        );
    }

    #[test]
    fn parses_body_layout_rows_align() {
        let header = parse(MINI_HEADER).expect("mini header should parse");
        let layout = &header.layout;
        assert_eq!(layout.offsets, vec![3, 171, 231]);
        assert_eq!(layout.coeffs_per_axis, vec![14, 10, 13]);
        assert_eq!(layout.subgranules, vec![4, 2, 2]);
        assert_eq!(layout.len(), 3);
        // Mercury: 14 coeffs × 3 axes × 4 subgranules = 168 words.
        assert_eq!(layout.record_words(0, 3), Some(168));
    }

    #[test]
    fn fortran_d_notation_round_trip() {
        assert_abs_diff_eq!(
            parse_jpl_float("0.441000000000000000D+03", 0, 0).unwrap(),
            441.0,
            epsilon = 1e-12
        );
        assert_abs_diff_eq!(
            parse_jpl_float("-0.899999999999999952D-13", 0, 0).unwrap(),
            -0.9e-13,
            epsilon = 1e-26
        );
        // Lowercase and standard-`e` accepted too.
        assert_abs_diff_eq!(
            parse_jpl_float("1.5d2", 0, 0).unwrap(),
            150.0,
            epsilon = 1e-12
        );
        assert_abs_diff_eq!(
            parse_jpl_float("1.5e2", 0, 0).unwrap(),
            150.0,
            epsilon = 1e-12
        );
    }

    #[test]
    fn missing_ksize_is_reported_with_line_number() {
        let bad = "\nNCOEFF= 1018\n\nGROUP   1010\n";
        let err = parse(bad).unwrap_err();
        match err {
            HeaderError::InvalidSizeDeclaration { line, .. } => {
                assert!(line >= 1, "line {line} should be ≥1");
            }
            other => panic!("wrong error variant: {other:?}"),
        }
    }

    #[test]
    fn unexpected_group_id_is_reported() {
        let bad = "KSIZE= 1 NCOEFF= 1\n\nGROUP   9999\n";
        let err = parse(bad).unwrap_err();
        assert!(matches!(
            err,
            HeaderError::MissingGroup { expected: 1010, .. }
        ));
    }

    #[test]
    fn body_layout_record_words_returns_none_for_out_of_range() {
        let layout = BodyLayoutTable {
            offsets: vec![3, 171],
            coeffs_per_axis: vec![14, 10],
            subgranules: vec![4, 2],
        };
        assert_eq!(layout.record_words(0, 3), Some(168));
        assert_eq!(layout.record_words(2, 3), None);
    }
}
