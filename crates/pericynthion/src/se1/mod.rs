//! Swiss Ephemeris SE1 asteroid/TNO ephemeris reader.
//!
//! Reads per-body `.se1` binary files from a Swiss Ephemeris data
//! installation. Each file covers a single body over a fixed time span
//! using nibble-packed Chebyshev polynomial coefficients.
//!
//! ## Coordinate frame
//!
//! Positions are barycentric J2000 equatorial (ICRS) in km when
//! `iflg & SEI_FLG_HELIO == 0`. Chiron has `iflg = 0x08`
//! (`SEI_FLG_EMBHEL`), which is barycentric — the same frame as DE441
//! output, so no frame conversion is needed in
//! [`crate::ephemeris::Ephemeris`].
//!
//! ## File path convention
//!
//! ```text
//! $SE_DATA/astNN/seNNNNN.se1   (full-range file, preferred)
//! $SE_DATA/astNN/sNNNNNs.se1   (short-range file, fallback)
//! ```
//!
//! Example: Chiron (2060) → `$SE_DATA/ast2/se02060.se1`
//!
//! [`se1_path`] resolves these paths from an asteroid number.
//!
//! ## Environment variable
//!
//! `STARCAT_SE_DATA` (or `SE_DATA`) — root of the Swiss Ephemeris data
//! directory (contains `ast0/`, `ast1/`, … subdirectories).
//! Integration tests skip cleanly when unset.

#![allow(clippy::cast_precision_loss, clippy::cast_sign_loss)]

mod decompress;
mod header;

use std::path::{Path, PathBuf};

use memmap2::Mmap;

use crate::chebyshev;
use crate::ephemeris::StateVector;
use crate::error::PericynthionError;
use header::Se1Header;

/// `SEI_FLG_HELIO`: heliocentric when set; barycentric when clear.
const SEI_FLG_HELIO: u8 = 0x01;

fn io_err(path: &Path, msg: impl Into<String>) -> PericynthionError {
    PericynthionError::Io {
        path: path.to_path_buf(),
        source: std::io::Error::new(std::io::ErrorKind::InvalidData, msg.into()),
    }
}

/// Memory-mapped Swiss Ephemeris SE1 binary file for a single body.
///
/// Open via [`Se1File::open`]. Query state vectors via [`Se1File::state_at`].
///
/// Positions and velocities are returned in **km** (converted from the
/// AU-basis Chebyshev coefficients using the header's `aunit` field).
pub struct Se1File {
    path: PathBuf,
    mmap: Mmap,
    /// JD TT of the first granule start.
    tfstart: f64,
    /// Granule length in days.
    dseg: f64,
    /// Chebyshev coefficients per axis per record.
    ncoe: usize,
    /// Normalization radius (AU) for coefficient decoding.
    rmax: f64,
    /// Body flag bits.
    iflg: u8,
    /// Kilometres per AU (from the binary header's `aunit` field).
    aunit_km: f64,
    /// Number of granule records.
    nrec: usize,
    /// Raw file offsets for each record; `index[iseg+1]` bounds record `iseg`.
    index: Vec<usize>,
    /// Body name as stored in the file (e.g. "Chiron").
    body_name: String,
    /// IAU asteroid number (SE body number − 10000).
    asteroid_number: u32,
}

impl Se1File {
    /// Open and memory-map a Swiss Ephemeris SE1 binary file.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be opened, memory-mapped, or
    /// does not parse as a valid SE1 binary (bad endian test, truncated
    /// header, index out of bounds, etc.).
    pub fn open(path: impl AsRef<Path>) -> Result<Self, PericynthionError> {
        let path = path.as_ref().to_path_buf();
        let file = std::fs::File::open(&path).map_err(|source| PericynthionError::Io {
            path: path.clone(),
            source,
        })?;
        // SAFETY: file is opened read-only; SE1 files are immutable ephemeris data.
        let mmap = unsafe { Mmap::map(&file) }.map_err(|source| PericynthionError::Io {
            path: path.clone(),
            source,
        })?;

        let hdr = Se1Header::parse(&mmap, &path)?;

        // nrec = round((tfend - tfstart) / dseg)
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let nrec = ((hdr.tfend - hdr.tfstart) / hdr.dseg + 0.5) as usize;

        // Index table: nrec + 2 entries of 3 bytes each, starting at lndx0.
        // Entries 0..nrec are record start offsets; entry nrec (and nrec+1)
        // give the end offset of the last record.
        let n_entries = nrec + 2;
        let idx_end = hdr.lndx0 + n_entries * 3;
        if idx_end > mmap.len() {
            return Err(io_err(
                &path,
                format!(
                    "SE1 index table [{}, {}) extends past file end ({}B)",
                    hdr.lndx0,
                    idx_end,
                    mmap.len()
                ),
            ));
        }

        let mut index = Vec::with_capacity(n_entries);
        let mut p = hdr.lndx0;
        for _ in 0..n_entries {
            let b0 = mmap[p] as usize;
            let b1 = mmap[p + 1] as usize;
            let b2 = mmap[p + 2] as usize;
            index.push(b0 | (b1 << 8) | (b2 << 16));
            p += 3;
        }

        Ok(Self {
            path,
            mmap,
            tfstart: hdr.tfstart,
            dseg: hdr.dseg,
            ncoe: hdr.ncoe,
            rmax: hdr.rmax,
            iflg: hdr.iflg,
            aunit_km: hdr.aunit / 1000.0, // file stores m/AU; convert to km/AU
            nrec,
            index,
            body_name: hdr.body_name,
            asteroid_number: hdr.asteroid_number,
        })
    }

    /// Filesystem path of the open file.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Body name as stored in the file header (e.g. `"Chiron"`).
    #[must_use]
    pub fn body_name(&self) -> &str {
        &self.body_name
    }

    /// IAU asteroid number encoded in the file (e.g. `2060` for Chiron).
    #[must_use]
    pub fn asteroid_number(&self) -> u32 {
        self.asteroid_number
    }

    /// Earliest Julian Date (TT) covered by this file.
    #[must_use]
    pub fn start_jd(&self) -> f64 {
        self.tfstart
    }

    /// Latest Julian Date (TT) covered by this file.
    #[must_use]
    pub fn end_jd(&self) -> f64 {
        self.tfstart + self.nrec as f64 * self.dseg
    }

    /// Returns `true` when positions from this file are **barycentric**
    /// (the `SEI_FLG_HELIO` bit is not set).
    ///
    /// Chiron (`iflg = 0x08` = `SEI_FLG_EMBHEL`) is barycentric J2000
    /// equatorial — the same frame as DE441. No Sun-offset is needed in
    /// [`crate::ephemeris::Ephemeris::state`].
    #[must_use]
    pub fn is_barycentric(&self) -> bool {
        self.iflg & SEI_FLG_HELIO == 0
    }

    /// Compute the position and velocity of this body at `jd_tt`
    /// (Terrestrial Time Julian Date).
    ///
    /// Returns a [`StateVector`] with:
    /// - `position_km`          — ICRF km (barycentric for Chiron)
    /// - `velocity_km_per_day`  — ICRF km/day
    ///
    /// # Errors
    ///
    /// Returns an error if `jd_tt` is outside this file's coverage, or if
    /// the record cannot be decompressed.
    pub fn state_at(&self, jd_tt: f64) -> Result<StateVector, PericynthionError> {
        let jd_end = self.end_jd();
        if jd_tt < self.tfstart || jd_tt > jd_end {
            return Err(io_err(
                &self.path,
                format!(
                    "SE1 {}: JD {jd_tt:.4} outside coverage [{:.4}, {jd_end:.4}]",
                    self.body_name, self.tfstart
                ),
            ));
        }

        // Which granule?
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let iseg = ((jd_tt - self.tfstart) / self.dseg)
            .floor()
            .min((self.nrec - 1) as f64) as usize;

        let rec_start = self.index[iseg];
        let rec_end = self.index[iseg + 1];
        if rec_end > self.mmap.len() || rec_start >= rec_end {
            return Err(io_err(
                &self.path,
                format!(
                    "SE1 {}: record {iseg} index [{rec_start}, {rec_end}) is out of file bounds",
                    self.body_name
                ),
            ));
        }
        let data = &self.mmap[rec_start..rec_end];

        let segp = decompress::decode_record(data, self.ncoe, self.rmax, &self.path)?;

        // tau ∈ [-1, 1] within the granule
        let tseg0 = self.tfstart + iseg as f64 * self.dseg;
        let tau = 2.0 * (jd_tt - tseg0) / self.dseg - 1.0;
        // dτ/dt = 2/dseg (days⁻¹)
        let dtau_dt = 2.0 / self.dseg;

        let mut position_km = [0.0_f64; 3];
        let mut velocity_km_per_day = [0.0_f64; 3];
        for axis in 0..3 {
            let coeffs = &segp[axis * self.ncoe..(axis + 1) * self.ncoe];
            position_km[axis] = chebyshev::evaluate(coeffs, tau) * self.aunit_km;
            velocity_km_per_day[axis] =
                chebyshev::evaluate_derivative(coeffs, tau) * dtau_dt * self.aunit_km;
        }

        Ok(StateVector {
            position_km,
            velocity_km_per_day,
        })
    }
}

/// Resolve the SE1 file path for `asteroid_number` in a Swiss Ephemeris
/// data directory.
///
/// Tries the full-range file first (`seNNNNN.se1`), then the short-range
/// variant (`sNNNNNs.se1`). Returns `None` if neither exists.
///
/// ```text
/// $se_root/astNN/seNNNNN.se1
/// $se_root/astNN/sNNNNNs.se1
/// ```
#[must_use]
pub fn se1_path(se_root: &Path, asteroid_number: u32) -> Option<PathBuf> {
    let dir_num = asteroid_number / 1000;
    let dir = se_root.join(format!("ast{dir_num}"));

    let full = dir.join(format!("se{asteroid_number:05}.se1"));
    if full.exists() {
        return Some(full);
    }

    let short = dir.join(format!("s{asteroid_number:05}s.se1"));
    if short.exists() {
        return Some(short);
    }

    None
}
