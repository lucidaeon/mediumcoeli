#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::missing_panics_doc,
    clippy::doc_markdown
)]

//! Memory-mapped reader for JPL DE-series **binary** ephemeris files.
//!
//! # File layout (DE-series binary, little- or big-endian)
//!
//! The binary file is a flat sequence of fixed-size records. Every
//! record is `NCOEFF` × 8 bytes (1018 doubles = 8144 bytes for DE441).
//! The records fall into two categories:
//!
//! - **Records 0 and 1** are the *binary header*: they encode the same
//!   title, constants, and layout that the companion ASCII `header.NNN`
//!   file also describes. We do not parse the binary header — the ASCII
//!   header is authoritative and we use it instead.
//! - **Records 2 onward** are *coefficient records*. Each one covers a
//!   single 32-day granule of time and contains, for every body, the
//!   Chebyshev coefficients for that granule. The first two doubles of
//!   each coefficient record are the granule's start and end Julian
//!   Date in Terrestrial Time.
//!
//! # Endianness
//!
//! DE-series files exist in two byte orders: little-endian (filename
//! prefix `linux_`) and big-endian (prefix `xnp_` for "executable, no
//! prefix" or similar legacy). There is no magic number in the file
//! itself. The reader determines byte order at open time by reading
//! the first coefficient record's leading time tag in both byte orders
//! and choosing the one that yields a value inside the file's plausible
//! time range. If neither works, the file is rejected.
//!
//! # Time-window scope
//!
//! The ASCII header advertises a *design* time range (e.g. DE441
//! advertises JD −3,100,015.5 to +8,000,016.5, ~13,200 BCE to
//! +17,191 CE). The actual binary file may cover a subset of that
//! range. The reader reports the file's *actual* coverage by inspecting
//! the time tag of the first and last coefficient records — never
//! trusting the ASCII header for this.

use crate::error::PericynthionError;
use crate::jpl::header::Header;
use memmap2::Mmap;
use std::fs::File;
use std::path::{Path, PathBuf};

/// Memory-mapped DE-series binary ephemeris file.
///
/// The file is held mmapped for its lifetime; the OS page cache handles
/// the working set, which keeps memory pressure proportional to the
/// span of dates actually queried, not the 2.6 GB file size.
pub struct EphemerisFile {
    /// Path to the file on disk (kept for diagnostics).
    path: PathBuf,
    /// The mmap handle. Dropped with the [`EphemerisFile`].
    mmap: Mmap,
    /// `NCOEFF` from the ASCII header — record size in 8-byte doubles.
    ncoeff: usize,
    /// `true` if the file's native byte order is little-endian.
    little_endian: bool,
    /// Number of coefficient records actually present in the file
    /// (computed as `file_size / record_bytes − 2 header records`).
    coefficient_records: usize,
    /// JD at which the first coefficient record begins (read from the
    /// file, *not* from the ASCII header).
    file_start_jd: f64,
    /// JD at which the last coefficient record ends.
    file_end_jd: f64,
    /// Granule size in days, read from the first coefficient record's
    /// time tag span. Used to validate against the ASCII header.
    granule_days: f64,
}

impl EphemerisFile {
    /// Open and validate a DE-series binary file against its companion
    /// ASCII header.
    ///
    /// # Errors
    ///
    /// Returns [`PericynthionError::Io`] if the file cannot be opened or
    /// mmapped. Returns a header-derived error if the file's size is
    /// not a multiple of the record size, or if the leading time tags
    /// of the first coefficient record can't be interpreted as
    /// plausible Julian Dates in either byte order.
    pub fn open(path: impl AsRef<Path>, header: &Header) -> Result<Self, PericynthionError> {
        let path = path.as_ref().to_path_buf();
        let file = File::open(&path).map_err(|source| PericynthionError::Io {
            path: path.clone(),
            source,
        })?;
        // SAFETY: the file is opened read-only and must remain untouched
        // by other processes for the lifetime of this mmap. JPL DE files
        // are immutable distribution payloads, so this assumption holds.
        let mmap = unsafe { Mmap::map(&file) }.map_err(|source| PericynthionError::Io {
            path: path.clone(),
            source,
        })?;

        let ncoeff = header.ncoeff as usize;
        let record_bytes = ncoeff * 8;
        let total_records = mmap.len() / record_bytes;
        if mmap.len() % record_bytes != 0 {
            return Err(PericynthionError::Io {
                path: path.clone(),
                source: std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!(
                        "file size {} is not a multiple of record size {record_bytes}",
                        mmap.len()
                    ),
                ),
            });
        }
        if total_records < 3 {
            return Err(PericynthionError::Io {
                path: path.clone(),
                source: std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "file contains fewer than 3 records (need 2 header + ≥1 coefficient)",
                ),
            });
        }
        let coefficient_records = total_records - 2;

        // Probe record 2 (first coefficient record) at byte offset 2*record_bytes.
        // First two doubles are the granule's start_jd and end_jd.
        let first_coeff_offset = 2 * record_bytes;
        let bytes_le_or_be = &mmap[first_coeff_offset..first_coeff_offset + 16];
        let (little_endian, start_jd, end_jd) = detect_endianness(
            bytes_le_or_be,
            header.epoch.start_jd,
            header.epoch.end_jd,
            header.epoch.granule_days,
        )
        .map_err(|detail| PericynthionError::Io {
            path: path.clone(),
            source: std::io::Error::new(std::io::ErrorKind::InvalidData, detail),
        })?;
        let granule_days = end_jd - start_jd;

        // Last coefficient record's end JD: read second double of last record.
        let last_offset = (total_records - 1) * record_bytes;
        let last_end_jd = read_double_at(&mmap, last_offset + 8, little_endian);

        Ok(Self {
            path,
            mmap,
            ncoeff,
            little_endian,
            coefficient_records,
            file_start_jd: start_jd,
            file_end_jd: last_end_jd,
            granule_days,
        })
    }

    /// Filesystem path of the open file.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// `true` if the file's native byte order is little-endian.
    #[must_use]
    pub fn is_little_endian(&self) -> bool {
        self.little_endian
    }

    /// Number of coefficient records actually present.
    #[must_use]
    pub fn coefficient_records(&self) -> usize {
        self.coefficient_records
    }

    /// First JD covered by the file (start of the first coefficient
    /// record). May differ from the ASCII header's design range.
    #[must_use]
    pub fn start_jd(&self) -> f64 {
        self.file_start_jd
    }

    /// Last JD covered by the file (end of the last coefficient record).
    #[must_use]
    pub fn end_jd(&self) -> f64 {
        self.file_end_jd
    }

    /// Granule size in days (read from the first coefficient record's
    /// time tag).
    #[must_use]
    pub fn granule_days(&self) -> f64 {
        self.granule_days
    }

    /// Read the coefficient record containing the given JD as a slice
    /// of `NCOEFF` doubles. The caller is responsible for byte-swapping
    /// via the `at` accessor: this method gives a typed view that handles
    /// endianness conversion on read.
    ///
    /// # Errors
    ///
    /// Returns [`PericynthionError::Io`] if the JD lies outside the file's
    /// actual coverage.
    pub fn record_for_jd(&self, jd_tt: f64) -> Result<CoefficientRecord<'_>, PericynthionError> {
        if jd_tt < self.file_start_jd || jd_tt > self.file_end_jd {
            return Err(PericynthionError::Io {
                path: self.path.clone(),
                source: std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!(
                        "JD {jd_tt} outside file coverage [{}, {}]",
                        self.file_start_jd, self.file_end_jd
                    ),
                ),
            });
        }
        let granule_index = ((jd_tt - self.file_start_jd) / self.granule_days).floor() as usize;
        let record_index = granule_index + 2; // skip 2 binary-header records
        let offset = record_index * self.ncoeff * 8;
        let end = offset + self.ncoeff * 8;
        // Final record edge case: a JD exactly at file_end_jd would
        // pick granule_index = coefficient_records, one past the end.
        // Clamp by stepping back into the last record.
        let (offset, end) = if granule_index >= self.coefficient_records {
            let off = (self.coefficient_records + 1) * self.ncoeff * 8;
            (off, off + self.ncoeff * 8)
        } else {
            (offset, end)
        };
        Ok(CoefficientRecord {
            bytes: &self.mmap[offset..end],
            little_endian: self.little_endian,
            ncoeff: self.ncoeff,
        })
    }
}

/// Borrowed view of one coefficient record's `NCOEFF` doubles.
///
/// The bytes live in the mmap; this struct just adds endianness-aware
/// accessors. Cheap to construct, cheap to drop.
pub struct CoefficientRecord<'a> {
    bytes: &'a [u8],
    little_endian: bool,
    ncoeff: usize,
}

impl CoefficientRecord<'_> {
    /// Number of doubles in this record (== `NCOEFF`).
    #[must_use]
    pub fn len(&self) -> usize {
        self.ncoeff
    }

    /// Always non-empty for a real DE record; this method exists to
    /// satisfy clippy and the `len`/`is_empty` API convention.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ncoeff == 0
    }

    /// Read the double at index `i` (0-indexed within the record).
    /// Panics if `i ≥ ncoeff`.
    #[must_use]
    pub fn get(&self, i: usize) -> f64 {
        assert!(
            i < self.ncoeff,
            "index {i} out of bounds (ncoeff={})",
            self.ncoeff
        );
        read_double_at(self.bytes, i * 8, self.little_endian)
    }

    /// Start JD of this granule (the first double of the record).
    #[must_use]
    pub fn start_jd(&self) -> f64 {
        self.get(0)
    }

    /// End JD of this granule (the second double of the record).
    #[must_use]
    pub fn end_jd(&self) -> f64 {
        self.get(1)
    }

    /// Copy a contiguous range of doubles into an owned `Vec<f64>`,
    /// applying byte-swap if necessary. Used to materialize one body's
    /// coefficient band for one sub-granule.
    #[must_use]
    pub fn slice(&self, start: usize, count: usize) -> Vec<f64> {
        (0..count).map(|k| self.get(start + k)).collect()
    }
}

// =============================================================================
// Byte-level helpers (decoupled from mmap for unit-testability)
// =============================================================================

/// Read a `f64` at byte offset `offset` in `bytes`, interpreting it as
/// little- or big-endian. Panics if `offset + 8 > bytes.len()`.
fn read_double_at(bytes: &[u8], offset: usize, little_endian: bool) -> f64 {
    let arr: [u8; 8] = bytes[offset..offset + 8]
        .try_into()
        .expect("8-byte slice required");
    if little_endian {
        f64::from_le_bytes(arr)
    } else {
        f64::from_be_bytes(arr)
    }
}

/// Try to interpret the first 16 bytes of a coefficient record as two
/// doubles (granule start_jd, granule end_jd) in little- and big-endian
/// order. Returns whichever order yields a start_jd inside `[hdr_start,
/// hdr_end]` *and* an end_jd exactly `granule_days` after start_jd. If
/// both succeed or both fail, returns an error string.
///
/// The granule-equality check is what makes this reliable: a 32-day
/// granule (or whatever the header declares) is so unlikely to appear
/// by coincidence in byte-swapped IEEE 754 that double-success is
/// astronomically improbable on a real ephemeris file.
fn detect_endianness(
    bytes: &[u8],
    hdr_start: f64,
    hdr_end: f64,
    granule_days: f64,
) -> Result<(bool, f64, f64), String> {
    assert!(bytes.len() >= 16);
    let try_order = |little: bool| -> Option<(f64, f64)> {
        let start = read_double_at(bytes, 0, little);
        let end = read_double_at(bytes, 8, little);
        if start.is_finite()
            && end.is_finite()
            && start >= hdr_start
            && start <= hdr_end
            && (end - start - granule_days).abs() < 1e-6
        {
            Some((start, end))
        } else {
            None
        }
    };
    match (try_order(true), try_order(false)) {
        (Some((s, e)), None) => Ok((true, s, e)),
        (None, Some((s, e))) => Ok((false, s, e)),
        (Some(_), Some(_)) => {
            Err("ambiguous endianness: both byte orders yield plausible JDs".into())
        }
        (None, None) => Err(
            "neither byte order yields a plausible time tag (first record \
             may not be a coefficient record, or file is corrupt)"
                .into(),
        ),
    }
}

// =============================================================================
// Unit tests (synthetic bytes only; integration test handles real file)
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_le_double_round_trip() {
        let v = std::f64::consts::PI;
        let bytes = v.to_le_bytes();
        let got = read_double_at(&bytes, 0, true);
        assert!((got - v).abs() < 1e-15);
    }

    #[test]
    fn read_be_double_round_trip() {
        let v: f64 = -1234.5678e90;
        let bytes = v.to_be_bytes();
        let got = read_double_at(&bytes, 0, false);
        assert!((got - v).abs() < 1e-15);
    }

    #[test]
    fn endianness_detected_as_little_when_le_bytes_yield_valid_jd() {
        let start: f64 = 2_451_545.0;
        let end: f64 = 2_451_577.0;
        let mut bytes = [0u8; 16];
        bytes[..8].copy_from_slice(&start.to_le_bytes());
        bytes[8..].copy_from_slice(&end.to_le_bytes());
        let (le, s, e) = detect_endianness(&bytes, 0.0, 3_000_000.0, 32.0).unwrap();
        assert!(le);
        assert!((s - start).abs() < 1e-15);
        assert!((e - end).abs() < 1e-15);
    }

    #[test]
    fn endianness_detected_as_big_when_be_bytes_yield_valid_jd() {
        let start: f64 = 2_451_545.0;
        let end: f64 = 2_451_577.0;
        let mut bytes = [0u8; 16];
        bytes[..8].copy_from_slice(&start.to_be_bytes());
        bytes[8..].copy_from_slice(&end.to_be_bytes());
        let (le, s, e) = detect_endianness(&bytes, 0.0, 3_000_000.0, 32.0).unwrap();
        assert!(!le);
        assert!((s - start).abs() < 1e-15);
        assert!((e - end).abs() < 1e-15);
    }

    #[test]
    fn endianness_detection_rejects_garbage() {
        let bytes = [0xFFu8; 16]; // NaN in both orders
        let err = detect_endianness(&bytes, 0.0, 3_000_000.0, 32.0).unwrap_err();
        assert!(err.contains("neither byte order"));
    }

    #[test]
    fn endianness_detection_rejects_out_of_range() {
        // Plausibly-shaped numbers, but well outside the JD window.
        let start: f64 = 9_999_999.0;
        let end: f64 = 10_000_031.0;
        let mut bytes = [0u8; 16];
        bytes[..8].copy_from_slice(&start.to_le_bytes());
        bytes[8..].copy_from_slice(&end.to_le_bytes());
        let result = detect_endianness(&bytes, 0.0, 1_000_000.0, 32.0);
        assert!(result.is_err());
    }

    #[test]
    fn endianness_detection_rejects_implausible_granule_size() {
        // start_jd inside range but end_jd is 1000 days later — not a
        // 32-day granule, so this must be wrong order.
        let start: f64 = 2_451_545.0;
        let end: f64 = start + 1000.0;
        let mut bytes = [0u8; 16];
        bytes[..8].copy_from_slice(&start.to_le_bytes());
        bytes[8..].copy_from_slice(&end.to_le_bytes());
        let result = detect_endianness(&bytes, 0.0, 3_000_000.0, 32.0);
        assert!(result.is_err());
    }
}
