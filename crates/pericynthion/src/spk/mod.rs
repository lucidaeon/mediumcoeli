//! SPICE Kernel (SPK) reader for NASA asteroid ephemerides.
//!
//! This module reads **DAF/SPK** (Double precision Array Files, Spacecraft
//! and Planet Kernel) binary files such as `sb441-n16.bsp`, produced by
//! the JPL SPICE toolkit and distributed through the NAIF archive.
//!
//! Only little-endian (`LTL-IEEE`) files are supported, which is all
//! modern JPL distribution files. Big-endian files are detected and
//! rejected with a clear error.
//!
//! # Usage
//!
//! ```no_run
//! use pericynthion::spk::SpkEphemeris;
//!
//! let spk = SpkEphemeris::open("sb441-n16.bsp").unwrap();
//! // Ceres (NAIF 2000001) at J2000 (et_sec = 0.0)
//! let state = spk.state(2_000_001, 0.0).unwrap();
//! println!("Ceres X = {:.3} km", state.position_km[0]);
//! ```
//!
//! # References
//!
//! - NAIF SPK Required Reading: <https://naif.jpl.nasa.gov/pub/naif/toolkit_docs/FORTRAN/req/spk.html>
//! - NAIF DAF Required Reading: <https://naif.jpl.nasa.gov/pub/naif/toolkit_docs/FORTRAN/req/daf.html>

pub mod body;
pub mod daf;
mod type2;
mod type21;

pub use body::Asteroid;
pub use daf::{Daf, SpkSegment};

use crate::ephemeris::StateVector;
use crate::error::PericynthionError;
use std::path::{Path, PathBuf};

/// High-level SPK ephemeris reader for asteroid positions.
///
/// Wraps a memory-mapped [`Daf`] and provides body-level state queries.
/// Construct with [`SpkEphemeris::open`]; then call [`SpkEphemeris::state`]
/// to retrieve the position and velocity of any body stored in the file.
///
/// # Coverage
///
/// Each body may have multiple time-window segments. `state` selects the
/// segment whose `[et_start, et_stop]` brackets the requested `et_sec`.
/// If no segment covers the requested time, an error is returned naming
/// the body, the requested time, and the available coverage windows.
///
/// # Frame
///
/// All state vectors are returned in the reference frame stored by the
/// SPK file (J2000/ICRF, frame code 1) relative to the segment's center
/// body (typically the Sun, NAIF 10). Use [`SpkEphemeris::center_of`] to
/// confirm the center for a given body.
///
/// # Approximation note
///
/// SPK files use TDB (Barycentric Dynamical Time). Callers typically
/// supply TT seconds past J2000. TT and TDB differ by at most ±1.7 ms,
/// producing sub-meter position errors for main-belt asteroids — negligible
/// for astrological use.
#[derive(Debug)]
pub struct SpkEphemeris {
    daf: Daf,
}

impl SpkEphemeris {
    /// Open a DAF/SPK binary file and build the segment table.
    ///
    /// # Errors
    ///
    /// Returns [`PericynthionError::Io`] if:
    /// - The file cannot be opened or memory-mapped.
    /// - The file fails DAF/SPK validation (magic, endianness, layout).
    pub fn open(path: impl AsRef<Path>) -> Result<Self, PericynthionError> {
        let daf = Daf::open(path)?;
        Ok(Self { daf })
    }

    /// Compute the [`StateVector`] (position + velocity) for a body at the
    /// given epoch.
    ///
    /// Scans the segment table for a segment whose `target` matches `naif_id`
    /// and whose `[et_start, et_stop]` brackets `et_sec`, then evaluates the
    /// segment. Type-2 (Chebyshev position) and Type-21 (Modified Difference
    /// Array) segments are supported. Any other segment type returns an error.
    ///
    /// # Errors
    ///
    /// Returns [`PericynthionError::Io`] (with a descriptive message) if:
    /// - No segment covers `naif_id` at `et_sec`. The error names the NAIF id,
    ///   the requested ET, and summarises the available coverage windows.
    /// - The bracketing segment is neither Type-2 nor Type-21.
    pub fn state(&self, naif_id: i32, et_sec: f64) -> Result<StateVector, PericynthionError> {
        // Linear scan: find the first segment for this body that brackets et_sec.
        let seg = self
            .daf
            .segments()
            .iter()
            .find(|s| s.target == naif_id && s.et_start <= et_sec && et_sec <= s.et_stop);

        if let Some(s) = seg {
            match s.seg_type {
                2 => type2::eval_type2(&self.daf, s, et_sec),
                21 => type21::eval_type21(&self.daf, s, et_sec),
                other => Err(PericynthionError::Io {
                    path: self.daf.path().to_path_buf(),
                    source: std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!(
                            "SPK segment for target {} is type {other}; only \
                             Type-2 (Chebyshev) and Type-21 (MDA) are supported",
                            s.target
                        ),
                    ),
                }),
            }
        } else {
            // Build a coverage summary for a helpful error message.
            let windows: Vec<String> = self
                .daf
                .segments()
                .iter()
                .filter(|s| s.target == naif_id)
                .map(|s| format!("[{:.3e}, {:.3e}]", s.et_start, s.et_stop))
                .collect();
            let coverage = if windows.is_empty() {
                "no segments for this body".to_owned()
            } else {
                windows.join(", ")
            };
            Err(PericynthionError::Io {
                path: self.daf.path().to_path_buf(),
                source: std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!(
                        "NAIF id {naif_id} not covered at et_sec={et_sec:.3e}; \
                         available: {coverage}"
                    ),
                ),
            })
        }
    }

    /// Return the center body NAIF id for the first segment of `naif_id`,
    /// or `None` if the body is not present in this file.
    ///
    /// For asteroid SPK files (e.g. `sb441-n16.bsp`) the center is
    /// typically `10` (the Sun), making positions heliocentric ICRF.
    #[must_use]
    pub fn center_of(&self, naif_id: i32) -> Option<i32> {
        self.daf
            .segments()
            .iter()
            .find(|s| s.target == naif_id)
            .map(|s| s.center)
    }
}

/// Open every `*.bsp` file directly inside `dir`, skipping any that are not
/// valid DAF/SPK files. A missing or unreadable directory yields an empty Vec.
///
/// Used to load the per-body SPKs fetched by `starcat horizons` from a single
/// directory (e.g. `$STARCAT_HORIZONS_DATA`). Order is unspecified.
#[must_use]
pub fn open_dir(dir: &Path) -> Vec<SpkEphemeris> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("bsp")
            && let Ok(spk) = SpkEphemeris::open(&path)
        {
            out.push(spk);
        }
    }
    out
}

/// Locate the default asteroid SPK file (`sb441-n16.bsp`) from `start`.
///
/// Routes through the common [`crate::locate_jpl_file`] locator: hoists to the
/// `ssd.jpl.nasa.gov/` mirror root when `start` is inside a real mirror, then
/// walks down. Returns the first `sb441-n16.bsp` found, or `None`.
/// Layout-agnostic: resolves whether the user mirrored the full tree (even when
/// pointing at a deep sibling branch such as `.../planets/Linux/de441`) or
/// dropped the BSP into a single flat directory.
///
/// # Examples
///
/// ```no_run
/// use pericynthion::spk::locate_default_bsp;
/// use std::path::Path;
///
/// if let Some(bsp) = locate_default_bsp(Path::new("/data/nasa")) {
///     println!("found BSP at {}", bsp.display());
/// }
/// ```
#[must_use]
pub fn locate_default_bsp(start: &Path) -> Option<PathBuf> {
    crate::locate_jpl_file(start, "sb441-n16.bsp")
}

/// Open every SPK a chart computation should consult, in priority order:
/// an explicit file (error is fatal), the auto-located default sb441 bundle
/// under `jpl_start` (best-effort — a missing/invalid bundle is skipped), and
/// every valid `*.bsp` directly inside `horizons_dir` (best-effort).
///
/// `None` for any input skips that source. This is the single SPK-opening
/// entry point a GUI or the CLI shares.
///
/// # Errors
/// [`PericynthionError`] if `explicit` is `Some` and cannot be opened.
pub fn open_all_sources(
    jpl_start: Option<&Path>,
    horizons_dir: Option<&Path>,
    explicit: Option<&Path>,
) -> Result<Vec<SpkEphemeris>, crate::error::PericynthionError> {
    let mut spk_files: Vec<SpkEphemeris> = Vec::new();
    if let Some(p) = explicit {
        spk_files.push(SpkEphemeris::open(p)?);
    }
    if let Some(start) = jpl_start
        && let Some(bsp) = locate_default_bsp(start)
        && let Ok(s) = SpkEphemeris::open(&bsp)
    {
        spk_files.push(s);
    }
    if let Some(hz) = horizons_dir {
        spk_files.extend(open_dir(hz));
    }
    Ok(spk_files)
}

/// Locate `sb441-n373.bsp` from `start`.
///
/// Routes through the common [`crate::locate_jpl_file`] locator (mirror-root
/// hoist, then walk down) and returns the first `sb441-n373.bsp` found, `None`
/// otherwise. Layout-agnostic: resolves whether the file sits in the full mirror
/// tree (including a deep-point start) or a flat directory.
#[must_use]
pub fn locate_n373_bsp(start: &Path) -> Option<PathBuf> {
    crate::locate_jpl_file(start, "sb441-n373.bsp")
}

#[cfg(test)]
mod open_all_tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn open_all_sources_empty_when_nothing_supplied() {
        let v = open_all_sources(None, None, None).expect("empty is ok");
        assert!(v.is_empty());
    }

    #[test]
    fn open_all_sources_empty_horizons_dir_yields_none() {
        let tmp = tempdir::TempDir::new("spk_empty").unwrap();
        let v = open_all_sources(None, Some(tmp.path()), None).expect("empty dir ok");
        assert!(v.is_empty());
    }

    #[test]
    fn open_all_sources_propagates_explicit_open_error() {
        let bogus = Path::new("/no/such/file.bsp");
        assert!(open_all_sources(None, None, Some(bogus)).is_err());
    }
}

#[cfg(test)]
mod tests {
    use super::locate_default_bsp;

    #[test]
    fn open_dir_opens_bsp_and_skips_junk() {
        use std::io::Write;
        let tmp = tempdir::TempDir::new("opendir").unwrap();
        // A minimal valid DAF/SPK file (file record + empty summary record).
        let mut file_rec = [0u8; 1024];
        file_rec[0..8].copy_from_slice(b"DAF/SPK ");
        file_rec[8..12].copy_from_slice(&2i32.to_le_bytes());
        file_rec[12..16].copy_from_slice(&6i32.to_le_bytes());
        file_rec[76..80].copy_from_slice(&2i32.to_le_bytes()); // FWARD=2
        file_rec[88..96].copy_from_slice(b"LTL-IEEE");
        let mut sum_rec = [0u8; 1024];
        sum_rec[16..24].copy_from_slice(&0.0f64.to_le_bytes()); // NSUM=0
        let mut good = Vec::new();
        good.extend_from_slice(&file_rec);
        good.extend_from_slice(&sum_rec);
        std::fs::File::create(tmp.path().join("a.bsp"))
            .unwrap()
            .write_all(&good)
            .unwrap();
        // Junk files that must be ignored.
        std::fs::write(tmp.path().join("notes.txt"), b"hello").unwrap();
        std::fs::write(tmp.path().join("broken.bsp"), b"NOPE").unwrap();

        let opened = super::open_dir(tmp.path());
        assert_eq!(opened.len(), 1, "only the one valid .bsp opens");

        // Missing dir → empty, no error.
        assert!(super::open_dir(&tmp.path().join("nope")).is_empty());
    }

    #[test]
    fn state_errors_on_non_type2_segment() {
        // Build a minimal valid DAF/SPK file whose one segment is Type-3
        // (not supported). `SpkEphemeris::state` must return Err, not panic.
        use crate::spk::daf::unpack_summary_ints;
        let tmp = tempdir::TempDir::new("spk_type3").unwrap();
        let p = tmp.path().join("type3.bsp");

        // File record: magic + ND=2 + NI=6 + FWARD=2 + LOCFMT="LTL-IEEE"
        let mut file_rec = [0u8; 1024];
        file_rec[0..8].copy_from_slice(b"DAF/SPK ");
        file_rec[8..12].copy_from_slice(&2i32.to_le_bytes()); // ND
        file_rec[12..16].copy_from_slice(&6i32.to_le_bytes()); // NI
        file_rec[76..80].copy_from_slice(&2i32.to_le_bytes()); // FWARD = record 2
        file_rec[88..96].copy_from_slice(b"LTL-IEEE");

        // Summary record: NEXT=0, PREV=0, NSUM=1; one Type-3 segment summary.
        let mut sum_rec = [0u8; 1024];
        sum_rec[0..8].copy_from_slice(&0.0f64.to_le_bytes()); // NEXT = 0
        sum_rec[8..16].copy_from_slice(&0.0f64.to_le_bytes()); // PREV = 0
        sum_rec[16..24].copy_from_slice(&1.0f64.to_le_bytes()); // NSUM = 1
        // Summary at byte 24: et_start, et_stop (2×f64), then 6×i32
        sum_rec[24..32].copy_from_slice(&(-1.0f64).to_le_bytes()); // et_start
        sum_rec[32..40].copy_from_slice(&1.0f64.to_le_bytes()); // et_stop
        // 6 i32: target=2000001, center=10, frame=1, seg_type=3, start_addr=257, end_addr=512
        let ints: [i32; 6] = [2_000_001, 10, 1, 3, 257, 512];
        let mut int_bytes = [0u8; 24];
        for (i, v) in ints.iter().enumerate() {
            int_bytes[i * 4..i * 4 + 4].copy_from_slice(&v.to_le_bytes());
        }
        // Verify round-trip of unpack_summary_ints (guard the packing).
        assert_eq!(unpack_summary_ints(&int_bytes), ints);
        sum_rec[40..64].copy_from_slice(&int_bytes);

        let mut data = Vec::with_capacity(3 * 1024);
        data.extend_from_slice(&file_rec);
        data.extend_from_slice(&sum_rec);
        // Data record (record 3) — content doesn't matter for type guard test.
        data.extend_from_slice(&[0u8; 1024]);
        std::fs::write(&p, &data).unwrap();

        let spk = super::SpkEphemeris::open(&p).unwrap();
        let err = spk.state(2_000_001, 0.0).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("type 3") || msg.to_lowercase().contains("type-2"),
            "expected type-3 error, got: {err}"
        );
        // Error must name the file path (Fix 3).
        assert!(
            msg.contains("type3.bsp"),
            "error should contain file name, got: {err}"
        );
    }

    #[test]
    fn state_errors_on_unsupported_segment_type() {
        // A segment claiming an unsupported type (e.g. 5) must error clearly,
        // not silently mis-evaluate. Build a minimal DAF with one type-5 segment.
        use std::io::Write;
        let tmp = tempdir::TempDir::new("spk_type5").unwrap();
        let p = tmp.path().join("t5.bsp");
        let mut file_rec = [0u8; 1024];
        file_rec[0..8].copy_from_slice(b"DAF/SPK ");
        file_rec[8..12].copy_from_slice(&2i32.to_le_bytes());
        file_rec[12..16].copy_from_slice(&6i32.to_le_bytes());
        file_rec[76..80].copy_from_slice(&2i32.to_le_bytes());
        file_rec[88..96].copy_from_slice(b"LTL-IEEE");
        let mut sum_rec = [0u8; 1024];
        sum_rec[16..24].copy_from_slice(&1.0f64.to_le_bytes()); // NSUM=1
        sum_rec[24..32].copy_from_slice(&(-1.0e9_f64).to_le_bytes());
        sum_rec[32..40].copy_from_slice(&1.0e9_f64.to_le_bytes());
        let ints: [i32; 6] = [5, 10, 1, 5, 257, 300]; // seg_type = 5
        let mut ib = [0u8; 24];
        for (i, v) in ints.iter().enumerate() {
            ib[i * 4..i * 4 + 4].copy_from_slice(&v.to_le_bytes());
        }
        sum_rec[40..64].copy_from_slice(&ib);
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&file_rec);
        bytes.extend_from_slice(&sum_rec);
        bytes.extend_from_slice(&[0u8; 1024]); // data record (zeros)
        std::fs::File::create(&p)
            .unwrap()
            .write_all(&bytes)
            .unwrap();

        let spk = super::SpkEphemeris::open(&p).unwrap();
        let err = spk.state(5, 0.0).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("type 5"),
            "expected an unsupported-type error for type 5, got: {err}"
        );
    }

    #[test]
    fn locate_default_bsp_finds_file_under_mirror() {
        let tmp = tempdir::TempDir::new("spk-locate-test").unwrap();
        let root = tmp.path();
        // Create the nested BSP path under a synthetic mirror root.
        let bsp_path =
            root.join("ssd.jpl.nasa.gov/ftp/eph/small_bodies/asteroids_de441/sb441-n16.bsp");
        std::fs::create_dir_all(bsp_path.parent().unwrap()).unwrap();
        std::fs::write(&bsp_path, b"").unwrap();

        // start = mirror root itself (canonical full-mirror layout resolves)
        assert_eq!(locate_default_bsp(root), Some(bsp_path.clone()));

        // start = a sibling subdirectory within the mirror (NOT an ancestor of
        // the BSP). The mirror-root hoist restores this: point anywhere inside a
        // real mirror and the BSP over in the small_bodies branch still resolves.
        let sub = root.join("ssd.jpl.nasa.gov/ftp/eph/planets/Linux/de441");
        std::fs::create_dir_all(&sub).unwrap();
        assert_eq!(locate_default_bsp(&sub), Some(bsp_path));
    }

    #[test]
    fn locate_default_bsp_deep_point_in_full_mirror_finds_sibling_branch() {
        // Full mirror with BOTH branches populated. Point start at the DEEP
        // planets/Linux/de441 dir and assert the BSP over in the sibling
        // small_bodies branch still resolves (mirror-root hoist).
        let tmp = tempdir::TempDir::new("spk-deep-point").unwrap();
        let root = tmp.path();
        let de_dir = root.join("ssd.jpl.nasa.gov/ftp/eph/planets/Linux/de441");
        std::fs::create_dir_all(&de_dir).unwrap();
        std::fs::write(de_dir.join("header.441"), b"").unwrap();
        std::fs::write(de_dir.join("linux_m13000p17000.441"), b"").unwrap();
        let bsp_path =
            root.join("ssd.jpl.nasa.gov/ftp/eph/small_bodies/asteroids_de441/sb441-n16.bsp");
        std::fs::create_dir_all(bsp_path.parent().unwrap()).unwrap();
        std::fs::write(&bsp_path, b"").unwrap();

        // Pointed at the deep DE441 dir, the sibling-branch BSP resolves.
        assert_eq!(locate_default_bsp(&de_dir), Some(bsp_path));
    }

    #[test]
    fn locate_default_bsp_finds_file_in_flat_dir() {
        // A normal user who dropped sb441-n16.bsp into one flat folder.
        let tmp = tempdir::TempDir::new("spk-locate-flat").unwrap();
        let bsp = tmp.path().join("sb441-n16.bsp");
        std::fs::write(&bsp, b"").unwrap();
        assert_eq!(locate_default_bsp(tmp.path()), Some(bsp));
    }

    #[test]
    fn locate_default_bsp_returns_none_for_unrelated_dir() {
        let tmp = tempdir::TempDir::new("spk-locate-none-test").unwrap();
        // No sb441-n16.bsp anywhere under the dir.
        assert_eq!(locate_default_bsp(tmp.path()), None);
    }

    #[test]
    fn locate_n373_bsp_finds_bsp_under_mirror_root() {
        let tmp = tempdir::TempDir::new("n373loc").unwrap();
        let bsp_dir = tmp
            .path()
            .join("ssd.jpl.nasa.gov")
            .join("ftp")
            .join("eph")
            .join("small_bodies")
            .join("asteroids_de441");
        std::fs::create_dir_all(&bsp_dir).unwrap();
        // A minimal valid DAF header so open() would succeed (we only need the file to exist).
        std::fs::write(bsp_dir.join("sb441-n373.bsp"), b"DAF/SPK placeholder").unwrap();
        // locate from a deep child of the mirror root (not an ancestor of the
        // file): the mirror-root hoist walks up to ssd.jpl.nasa.gov's parent,
        // then descends to the canonical file.
        let deep = tmp.path().join("some").join("subdir");
        std::fs::create_dir_all(&deep).unwrap();
        let found = super::locate_n373_bsp(&deep);
        assert_eq!(found, Some(bsp_dir.join("sb441-n373.bsp")));
        // absent → None
        let other = tempdir::TempDir::new("n373loc_absent").unwrap();
        assert!(super::locate_n373_bsp(other.path()).is_none());
    }

    #[test]
    fn locate_n373_bsp_finds_file_in_flat_dir() {
        // Flat layout: the n373 bundle dropped directly into one folder.
        let tmp = tempdir::TempDir::new("n373flat").unwrap();
        let bsp = tmp.path().join("sb441-n373.bsp");
        std::fs::write(&bsp, b"DAF/SPK placeholder").unwrap();
        assert_eq!(super::locate_n373_bsp(tmp.path()), Some(bsp));
    }
}
