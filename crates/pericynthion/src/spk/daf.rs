//! DAF/SPK container reader: file-record validation and segment-table walk.
//!
//! # DAF binary layout (little-endian, `LTL-IEEE`)
//!
//! All JPL modern distribution files use little-endian byte order.
//! Physical records are 1024 bytes (128 doubles). DP element addresses
//! are **1-based**: element `A` is at byte `(A − 1) × 8`. Summary record
//! number `R` starts at byte `(R − 1) × 1024`.
//!
//! ## File record (first 1024 bytes)
//!
//! | Offset | Field | Value |
//! |--------|-------|-------|
//! | 0..8 | `LOCIDW` | `b"DAF/SPK "` |
//! | 8..12 | `ND` (i32 LE) | 2 (two f64 per summary: `et_start`, `et_stop`) |
//! | 12..16 | `NI` (i32 LE) | 6 (six i32 per summary: target, center, frame, `seg_type`, `start_addr`, `end_addr`) |
//! | 76..80 | `FWARD` (i32 LE) | first summary record number |
//! | 88..96 | `LOCFMT` | `"LTL-IEEE"` (must contain `"LTL"`) |
//!
//! ## Summary record
//!
//! Each summary record (1024 bytes) starts with three f64 values:
//! `NEXT`, `PREV`, `NSUM` (cast to i32; `NEXT == 0` ends the chain).
//! Starting at byte 24, there are `NSUM` segment summaries, each 40 bytes:
//! - 2 × f64: `et_start`, `et_stop` (seconds past J2000 TDB)
//! - 24 bytes as 6 × i32 LE: `target, center, frame, seg_type, start_addr, end_addr`

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]

use crate::error::PericynthionError;
use memmap2::Mmap;
use std::fs::File;
use std::path::{Path, PathBuf};

/// Magic bytes expected at the start of every DAF/SPK file.
const DAF_MAGIC: &[u8; 8] = b"DAF/SPK ";

/// Physical record size in bytes (1024 bytes = 128 doubles).
const RECORD_BYTES: usize = 1024;

/// Expected number of double-precision components per SPK summary (`ND`).
const EXPECTED_ND: i32 = 2;

/// Expected number of integer components per SPK summary (`NI`).
const EXPECTED_NI: i32 = 6;

/// Size in bytes of one segment summary: `ND` doubles + `(NI+1)/2` doubles = 5 doubles = 40 bytes.
const SUMMARY_BYTES: usize = 40;

/// A single SPK segment descriptor, extracted from the DAF summary records.
///
/// Each segment covers one body over a contiguous time window and stores
/// Chebyshev polynomial coefficients in the DP element array.
#[derive(Debug, Clone, PartialEq)]
pub struct SpkSegment {
    /// NAIF integer code for the target body (e.g. `2000001` for Ceres).
    pub target: i32,
    /// NAIF integer code for the centre body (e.g. `10` for the Sun).
    pub center: i32,
    /// NAIF frame code (e.g. `1` = J2000/ICRF).
    pub frame: i32,
    /// SPK segment type (e.g. `2` = Type-2 Chebyshev position).
    pub seg_type: i32,
    /// Start element address in the DP array (1-based).
    pub start_addr: i32,
    /// End element address in the DP array (1-based).
    pub end_addr: i32,
    /// Start of the segment's coverage (seconds past J2000 TDB).
    pub et_start: f64,
    /// End of the segment's coverage (seconds past J2000 TDB).
    pub et_stop: f64,
}

/// Memory-mapped DAF/SPK file with a pre-built segment table.
///
/// The file is held open and mmapped for its lifetime; the OS page cache
/// handles the working set. Segment metadata is fully decoded at open time
/// into a `Vec<SpkSegment>`. Coefficient data is read on demand via
/// `Daf::dword`.
pub struct Daf {
    /// Path to the file on disk (kept for diagnostics).
    path: PathBuf,
    /// The mmap handle. Dropped with the `Daf`.
    /// Read on demand via `Daf::dword` by the Type-2 evaluator.
    mmap: Mmap,
    /// Pre-built segment table decoded from all summary records.
    segments: Vec<SpkSegment>,
}

impl std::fmt::Debug for Daf {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Daf")
            .field("path", &self.path)
            .field("segments", &self.segments.len())
            .finish_non_exhaustive()
    }
}

impl Daf {
    /// Open and validate a DAF/SPK binary file, walking all summary records
    /// to build the complete segment table.
    ///
    /// # Errors
    ///
    /// Returns [`PericynthionError::Io`] if:
    /// - The file cannot be opened or memory-mapped.
    /// - The file's first 8 bytes are not `b"DAF/SPK "` (magic mismatch).
    /// - `ND` ≠ 2 or `NI` ≠ 6 (unsupported SPK layout).
    /// - `LOCFMT` does not contain `"LTL"` (big-endian or unknown format).
    /// - The summary-record chain extends beyond the file boundaries.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, PericynthionError> {
        let path = path.as_ref().to_path_buf();
        let file = File::open(&path).map_err(|source| PericynthionError::Io {
            path: path.clone(),
            source,
        })?;
        // SAFETY: the file is opened read-only and must remain untouched
        // by other processes for the lifetime of this mmap. JPL SPK files
        // are immutable distribution payloads, so this assumption holds.
        let mmap = unsafe { Mmap::map(&file) }.map_err(|source| PericynthionError::Io {
            path: path.clone(),
            source,
        })?;

        // Validate minimum file size for the file record.
        if mmap.len() < RECORD_BYTES {
            return Err(PericynthionError::Io {
                path: path.clone(),
                source: std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!(
                        "file too small to be a DAF/SPK: {} bytes (need ≥ {})",
                        mmap.len(),
                        RECORD_BYTES
                    ),
                ),
            });
        }

        // Validate DAF magic: bytes 0..8 must be b"DAF/SPK ".
        if &mmap[0..8] != DAF_MAGIC {
            return Err(PericynthionError::Io {
                path: path.clone(),
                source: std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!(
                        "not a DAF/SPK file: expected magic {:?}, found {:?}",
                        DAF_MAGIC,
                        &mmap[0..8.min(mmap.len())]
                    ),
                ),
            });
        }

        // Read ND (i32 @ byte 8) and NI (i32 @ byte 12).
        let nd = read_i32_le(&mmap, 8);
        let ni = read_i32_le(&mmap, 12);
        if nd != EXPECTED_ND || ni != EXPECTED_NI {
            return Err(PericynthionError::Io {
                path: path.clone(),
                source: std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("unsupported DAF/SPK layout: ND={nd} NI={ni} (expected ND=2 NI=6)"),
                ),
            });
        }

        // Read FWARD (i32 @ byte 76): first summary record number (1-based).
        let fward = read_i32_le(&mmap, 76);

        // Read LOCFMT (bytes 88..96): must contain "LTL" for little-endian.
        let locfmt = &mmap[88..96];
        let locfmt_display = String::from_utf8_lossy(locfmt);
        if !locfmt.windows(3).any(|w| w == b"LTL") {
            return Err(PericynthionError::Io {
                path: path.clone(),
                source: std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!(
                        "unsupported DAF byte order: LOCFMT={locfmt_display:?} (only LTL-IEEE little-endian is supported)"
                    ),
                ),
            });
        }

        // Walk the summary-record linked list starting at FWARD.
        let segments = walk_summary_records(&mmap, &path, fward)?;

        Ok(Self {
            path,
            mmap,
            segments,
        })
    }

    /// Filesystem path of the open file.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The complete segment table decoded from all summary records.
    ///
    /// Segments are listed in the order they appear in the file's summary
    /// records. For bodies with multiple time-window segments (common for
    /// long-span files), all segments for that body appear in this slice.
    #[must_use]
    pub fn segments(&self) -> &[SpkSegment] {
        &self.segments
    }

    /// Try to read the double-precision value at 1-based DP element address `addr`.
    ///
    /// Returns `None` if `addr_1based < 1` or if the corresponding byte range
    /// `(addr_1based - 1) * 8 .. (addr_1based - 1) * 8 + 8` exceeds the mapped
    /// file length. Returns `Some(value)` for any address within bounds.
    #[must_use]
    pub(crate) fn try_dword(&self, addr_1based: i32) -> Option<f64> {
        if addr_1based < 1 {
            return None;
        }
        let byte_off = (addr_1based as usize - 1) * 8;
        if byte_off + 8 > self.mmap.len() {
            return None;
        }
        Some(read_f64_le(&self.mmap, byte_off))
    }
}

// =============================================================================
// Summary-record walker
// =============================================================================

/// Walk the summary-record linked list and collect all `SpkSegment` entries.
///
/// Starting at record `fward` (1-based), reads `NEXT/PREV/NSUM` and all
/// summaries from each 1024-byte record, following `NEXT` until it is 0.
fn walk_summary_records(
    mmap: &[u8],
    path: &Path,
    fward: i32,
) -> Result<Vec<SpkSegment>, PericynthionError> {
    let mut segments = Vec::new();
    let mut rec = fward;

    loop {
        // Summary record number `rec` (1-based) starts at byte `(rec-1)*1024`.
        let rec_byte = (rec as usize - 1) * RECORD_BYTES;
        if rec_byte + RECORD_BYTES > mmap.len() {
            return Err(PericynthionError::Io {
                path: path.to_path_buf(),
                source: std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!(
                        "summary record {rec} at byte {rec_byte} exceeds file length {}",
                        mmap.len()
                    ),
                ),
            });
        }
        let record = &mmap[rec_byte..rec_byte + RECORD_BYTES];

        // First three doubles: NEXT, PREV, NSUM (read as f64, truncated to i32).
        let next = read_f64_le(record, 0) as i32;
        // PREV is at offset 8 but not used for walking.
        let nsum = read_f64_le(record, 16) as i32;

        // Guard: NSUM must be non-negative and all its summaries must fit
        // within the 1024-byte record (byte 24 + NSUM*40 ≤ 1024).
        // Use i64 literals (not usize→i64 casts) to keep clippy happy.
        // SUMMARY_BYTES == 40, RECORD_BYTES == 1024 — stated numerically here
        // so the compiler can verify the constants match the guard at test time.
        debug_assert_eq!(SUMMARY_BYTES, 40);
        debug_assert_eq!(RECORD_BYTES, 1024);
        if nsum < 0 || 24_i64 + i64::from(nsum) * 40_i64 > 1024_i64 {
            return Err(PericynthionError::Io {
                path: path.to_path_buf(),
                source: std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("corrupt summary record (implausible NSUM {nsum}) in record {rec}"),
                ),
            });
        }

        // Parse `nsum` summaries starting at byte 24 within the record.
        for s in 0..nsum as usize {
            let off = 24 + s * SUMMARY_BYTES;
            // Guard: every field of this summary must lie within the record.
            if off + SUMMARY_BYTES > record.len() {
                return Err(PericynthionError::Io {
                    path: path.to_path_buf(),
                    source: std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!(
                            "corrupt summary record (NSUM overruns record) at summary {s} in record {rec}"
                        ),
                    ),
                });
            }
            let et_start = read_f64_le(record, off);
            let et_stop = read_f64_le(record, off + 8);
            // The next 24 bytes hold 6 i32 values packed as little-endian.
            let int_bytes: &[u8; 24] = record[off + 16..off + 40]
                .try_into()
                .expect("slice is exactly 24 bytes");
            let ints = unpack_summary_ints(int_bytes);
            segments.push(SpkSegment {
                target: ints[0],
                center: ints[1],
                frame: ints[2],
                seg_type: ints[3],
                start_addr: ints[4],
                end_addr: ints[5],
                et_start,
                et_stop,
            });
        }

        // Follow the chain. NEXT == 0 means this was the last summary record.
        if next == 0 {
            break;
        }
        rec = next;
    }

    Ok(segments)
}

// =============================================================================
// Byte-level helpers
// =============================================================================

/// Read a little-endian `i32` at byte offset `off` in `bytes`.
///
/// Panics if `off + 4 > bytes.len()`.
fn read_i32_le(bytes: &[u8], off: usize) -> i32 {
    let arr: [u8; 4] = bytes[off..off + 4]
        .try_into()
        .expect("4-byte slice required for i32");
    i32::from_le_bytes(arr)
}

/// Read a little-endian `f64` at byte offset `off` in `bytes`.
///
/// Panics if `off + 8 > bytes.len()`.
fn read_f64_le(bytes: &[u8], off: usize) -> f64 {
    let arr: [u8; 8] = bytes[off..off + 8]
        .try_into()
        .expect("8-byte slice required for f64");
    f64::from_le_bytes(arr)
}

/// Unpack 6 little-endian `i32` values from a 24-byte slice.
///
/// In a DAF/SPK summary, the NI=6 integer fields (target, center, frame,
/// `seg_type`, `start_addr`, `end_addr`) are packed as 3 doubles (24 bytes), each
/// double holding two consecutive i32 values in little-endian byte order.
/// Reading the raw bytes as 6 × i32 LE extracts them correctly.
pub(crate) fn unpack_summary_ints(bytes: &[u8; 24]) -> [i32; 6] {
    let mut out = [0i32; 6];
    for (i, v) in out.iter_mut().enumerate() {
        *v = read_i32_le(bytes, i * 4);
    }
    out
}

// =============================================================================
// Unit tests
// =============================================================================

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    /// Walk up from `$STARCAT_JPL_DATA` to find the mirror root, then resolve
    /// `ftp/eph/small_bodies/asteroids_de441/sb441-n16.bsp`.
    ///
    /// Returns `Some(path)` only if the file exists on disk.
    fn test_bsp_n16() -> Option<PathBuf> {
        let val = std::env::var_os("STARCAT_JPL_DATA")?;
        let start = PathBuf::from(&val).canonicalize().ok()?;

        // Walk upward until we find a directory that has the small-bodies
        // subdirectory structure next to the planets directory.
        let mut candidate = start.as_path();
        for _ in 0..10 {
            let bsp = candidate
                .join("ftp")
                .join("eph")
                .join("small_bodies")
                .join("asteroids_de441")
                .join("sb441-n16.bsp");
            if bsp.is_file() {
                return Some(bsp);
            }
            candidate = candidate.parent()?;
        }
        None
    }

    #[test]
    fn unpacks_six_i32_from_three_doubles() {
        // 6 i32 packed little-endian into 24 bytes (3 doubles).
        let ints: [i32; 6] = [2_000_001, 10, 1, 2, 7_645_161, 8_945_164];
        let mut bytes = [0u8; 24];
        for (i, v) in ints.iter().enumerate() {
            bytes[i * 4..i * 4 + 4].copy_from_slice(&v.to_le_bytes());
        }
        assert_eq!(super::unpack_summary_ints(&bytes), ints);
    }

    #[test]
    fn rejects_non_daf_magic() {
        // open() on a tiny non-DAF temp file errors clearly.
        let tmp = tempdir::TempDir::new("spk").unwrap();
        let p = tmp.path().join("x.bsp");
        std::fs::write(&p, b"NOTADAF!").unwrap();
        let err = super::Daf::open(&p).unwrap_err();
        assert!(format!("{err}").to_lowercase().contains("daf"));
    }

    #[test]
    fn rejects_corrupt_nsum_in_summary_record() {
        // Build a minimal but valid DAF/SPK file record (1024 bytes) followed
        // by a summary record whose NSUM is so large it would overrun the record.
        let tmp = tempdir::TempDir::new("spk_corrupt").unwrap();
        let p = tmp.path().join("corrupt.bsp");

        // File record: magic + ND=2 + NI=6 + FWARD=2 + LOCFMT="LTL-IEEE"
        let mut file_rec = [0u8; super::RECORD_BYTES];
        file_rec[0..8].copy_from_slice(b"DAF/SPK ");
        file_rec[8..12].copy_from_slice(&2i32.to_le_bytes()); // ND
        file_rec[12..16].copy_from_slice(&6i32.to_le_bytes()); // NI
        file_rec[76..80].copy_from_slice(&2i32.to_le_bytes()); // FWARD = record 2
        file_rec[88..96].copy_from_slice(b"LTL-IEEE");

        // Summary record: NEXT=0, PREV=0, NSUM=999 (way too big — only ~25 fit)
        let mut sum_rec = [0u8; super::RECORD_BYTES];
        sum_rec[0..8].copy_from_slice(&0.0f64.to_le_bytes()); // NEXT = 0
        sum_rec[8..16].copy_from_slice(&0.0f64.to_le_bytes()); // PREV = 0
        sum_rec[16..24].copy_from_slice(&999.0f64.to_le_bytes()); // NSUM = 999

        let mut data = Vec::with_capacity(2 * super::RECORD_BYTES);
        data.extend_from_slice(&file_rec);
        data.extend_from_slice(&sum_rec);
        std::fs::write(&p, &data).unwrap();

        let err = super::Daf::open(&p).unwrap_err();
        let msg = format!("{err}").to_lowercase();
        assert!(
            msg.contains("nsum") || msg.contains("corrupt"),
            "expected corrupt/nsum in error, got: {err}"
        );
    }

    #[test]
    fn finds_ceres_segment_in_sb441_n16() {
        let Some(bsp) = test_bsp_n16() else {
            eprintln!("skip: sb441-n16.bsp not present");
            return;
        };
        let daf = super::Daf::open(&bsp).unwrap();
        let ceres: Vec<_> = daf
            .segments()
            .iter()
            .filter(|s| s.target == 2_000_001)
            .collect();
        assert_eq!(ceres.len(), 4, "Ceres has 4 time-window segments");
        assert!(
            ceres
                .iter()
                .all(|s| s.center == 10 && s.seg_type == 2 && s.frame == 1)
        );
    }
}
