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

    /// The on-disk file this SPK was opened from.
    #[must_use]
    pub fn source_path(&self) -> &Path {
        self.daf.path()
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
///
/// 1. `explicit` — an explicit `--spk` file (a failure here is fatal).
/// 2. `jpl_start`, interpreted by `jpl_curated`:
///    - **curated** (the platform data dir, where every file is intentional):
///      *every* `.bsp` under it is opened ([`collect_bsp_paths`]) — so the
///      default main-belt + dwarf bundles fetched by `data fetch de441`, and any
///      SPK the user hand-dropped, all work without being named here.
///    - **not curated** (a bulk external mirror): only the named bundles are
///      opened — `sb441-n16.bsp` then `sb441-n373s.bsp`/`sb441-n373.bsp` — never
///      the mirror's `satellites/`/`spacecraft/` `.bsp`s.
/// 3. `horizons_dir` — every `.bsp` in the (curated) Horizons output dir.
///
/// Opened files are de-duplicated by canonical path, so a curated tree that
/// contains the Horizons dir does not open the same file twice. All SPK opens
/// are mmap-backed (a missing-page-only read), so opening even the large
/// `sb441-n373.bsp` is cheap. `None` for `jpl_start`/`horizons_dir`/`explicit`
/// skips that source. This is the single SPK-opening entry point a GUI or the
/// CLI shares.
///
/// # Errors
/// [`PericynthionError`] if `explicit` is `Some` and cannot be opened.
pub fn open_all_sources(
    jpl_start: Option<&Path>,
    jpl_curated: bool,
    horizons_dir: Option<&Path>,
    explicit: Option<&Path>,
) -> Result<Vec<SpkEphemeris>, crate::error::PericynthionError> {
    use std::collections::HashSet;
    let mut seen: HashSet<PathBuf> = HashSet::new();
    let mut out: Vec<SpkEphemeris> = Vec::new();
    // Dedup by canonical path so a curated tree that *contains* the Horizons dir
    // does not open the same `.bsp` twice.
    let key = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());

    // Explicit --spk: highest priority, and a failure here is fatal.
    if let Some(p) = explicit
        && seen.insert(key(p))
    {
        out.push(SpkEphemeris::open(p)?);
    }

    // The JPL location.
    if let Some(start) = jpl_start {
        if jpl_curated {
            // A curated location (the platform data dir): every `.bsp` in it was
            // put there intentionally by `data fetch`/`data migrate` or by hand —
            // open them all, so hand-added SPKs work without being named here.
            for path in collect_bsp_paths(start) {
                if seen.insert(key(&path))
                    && let Ok(s) = SpkEphemeris::open(&path)
                {
                    out.push(s);
                }
            }
        } else {
            // A bulk external mirror: open only the named bundles we want, never
            // the moons/spacecraft. Main belt (`sb441-n16`) before the dwarfs
            // (`sb441-n373s`/`n373`) so the belt is never shadowed.
            let locators: [fn(&Path) -> Option<PathBuf>; 2] =
                [locate_default_bsp, locate_dwarf_bsp];
            for locate in locators {
                if let Some(bsp) = locate(start)
                    && seen.insert(key(&bsp))
                    && let Ok(s) = SpkEphemeris::open(&bsp)
                {
                    out.push(s);
                }
            }
        }
    }

    // The Horizons dir is itself curated — open every `.bsp` in it.
    if let Some(hz) = horizons_dir {
        for path in collect_bsp_paths(hz) {
            if seen.insert(key(&path))
                && let Ok(s) = SpkEphemeris::open(&path)
            {
                out.push(s);
            }
        }
    }
    Ok(out)
}

/// Recursively collect every `*.bsp` path under `dir`, in deterministic
/// (sorted) order. Bounded depth, does not follow symlinks. Best-effort: an
/// unreadable directory yields nothing rather than an error. For **curated**
/// locations only — never point this at a bulk government mirror whose
/// `satellites/`/`spacecraft/` trees hold tens of thousands of `.bsp`s.
#[must_use]
pub fn collect_bsp_paths(dir: &Path) -> Vec<PathBuf> {
    fn rec(dir: &Path, out: &mut Vec<PathBuf>, depth: usize) {
        if depth >= 64 {
            return;
        }
        let Ok(read) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in read.flatten() {
            let path = entry.path();
            let Ok(meta) = std::fs::symlink_metadata(&path) else {
                continue;
            };
            if meta.file_type().is_file() {
                if path.extension().and_then(|e| e.to_str()) == Some("bsp") {
                    out.push(path);
                }
            } else if meta.file_type().is_dir() {
                rec(&path, out, depth + 1);
            }
        }
    }
    let mut out = Vec::new();
    rec(dir, &mut out, 0);
    out.sort();
    out
}

/// Locate the dwarf-planet SPK bundle from `start`, **preferring** the compact
/// DE440-window subset `sb441-n373s.bsp` over the full deep-time
/// `sb441-n373.bsp`. Layout-agnostic (mirror-root hoist, then walk), like the
/// other locators; returns `None` if neither is present.
#[must_use]
pub fn locate_dwarf_bsp(start: &Path) -> Option<PathBuf> {
    crate::locate_jpl_file(start, "sb441-n373s.bsp")
        .or_else(|| crate::locate_jpl_file(start, "sb441-n373.bsp"))
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

    /// Write a minimal valid DAF/SPK (file record + empty summary record) at
    /// `path`, creating parents. Zero segments — opens fine, covers nothing.
    fn write_minimal_spk(path: &Path) {
        use std::io::Write;
        let mut file_rec = [0u8; 1024];
        file_rec[0..8].copy_from_slice(b"DAF/SPK ");
        file_rec[8..12].copy_from_slice(&2i32.to_le_bytes());
        file_rec[12..16].copy_from_slice(&6i32.to_le_bytes());
        file_rec[76..80].copy_from_slice(&2i32.to_le_bytes()); // FWARD=2
        file_rec[88..96].copy_from_slice(b"LTL-IEEE");
        let sum_rec = [0u8; 1024]; // NSUM=0
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut f = std::fs::File::create(path).unwrap();
        f.write_all(&file_rec).unwrap();
        f.write_all(&sum_rec).unwrap();
    }

    #[test]
    fn open_all_sources_empty_when_nothing_supplied() {
        let v = open_all_sources(None, false, None, None).expect("empty is ok");
        assert!(v.is_empty());
    }

    #[test]
    fn open_all_sources_empty_horizons_dir_yields_none() {
        let tmp = tempdir::TempDir::new("spk_empty").unwrap();
        let v = open_all_sources(None, false, Some(tmp.path()), None).expect("empty dir ok");
        assert!(v.is_empty());
    }

    #[test]
    fn open_all_sources_propagates_explicit_open_error() {
        let bogus = Path::new("/no/such/file.bsp");
        assert!(open_all_sources(None, false, None, Some(bogus)).is_err());
    }

    #[test]
    fn curated_source_opens_every_bsp_in_the_tree() {
        // A curated platform data dir: two `.bsp`s in different nested subdirs
        // (as the fetched main-belt + dwarf bundles land) are BOTH opened,
        // without being named — that is the whole point of "curated = load all".
        let tmp = tempdir::TempDir::new("spk_curated").unwrap();
        let root = tmp.path();
        write_minimal_spk(
            &root.join("ssd.jpl.nasa.gov/ftp/eph/small_bodies/asteroids_de441/sb441-n16.bsp"),
        );
        write_minimal_spk(
            &root.join("ssd.jpl.nasa.gov/ftp/eph/small_bodies/asteroids_de441/sb441-n373s.bsp"),
        );
        std::fs::write(root.join("stray.txt"), b"ignored").unwrap();

        let v = open_all_sources(Some(root), true, None, None).expect("curated open");
        assert_eq!(v.len(), 2, "both nested bundles open under a curated dir");
    }

    #[test]
    fn mirror_source_opens_only_named_bundles_not_arbitrary_bsps() {
        // A non-curated (bulk mirror) source: an arbitrary `.bsp` that is NOT a
        // named bundle must be ignored (this is the moons/spacecraft guard).
        let tmp = tempdir::TempDir::new("spk_mirror").unwrap();
        let root = tmp.path();
        write_minimal_spk(&root.join("ssd.jpl.nasa.gov/ftp/eph/satellites/jup365.bsp"));
        write_minimal_spk(
            &root.join("ssd.jpl.nasa.gov/ftp/eph/small_bodies/asteroids_de441/sb441-n16.bsp"),
        );

        let v = open_all_sources(Some(root), false, None, None).expect("mirror open");
        assert_eq!(
            v.len(),
            1,
            "only the named sb441-n16 bundle, not the moon SPK"
        );
    }

    #[test]
    fn curated_and_horizons_overlap_is_deduped() {
        // If the curated tree *contains* the Horizons dir, the same `.bsp` must
        // not be opened twice.
        let tmp = tempdir::TempDir::new("spk_dedup").unwrap();
        let root = tmp.path();
        let hz = root.join("horizons");
        write_minimal_spk(&hz.join("2060.bsp"));

        let v = open_all_sources(Some(root), true, Some(&hz), None).expect("dedup open");
        assert_eq!(v.len(), 1, "the shared Horizons .bsp opens exactly once");
    }

    #[test]
    fn locate_dwarf_bsp_prefers_n373s_over_full_n373() {
        let tmp = tempdir::TempDir::new("dwarf").unwrap();
        let dir = tmp
            .path()
            .join("ssd.jpl.nasa.gov/ftp/eph/small_bodies/asteroids_de441");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("sb441-n373.bsp"), b"x").unwrap();
        std::fs::write(dir.join("sb441-n373s.bsp"), b"x").unwrap();
        assert!(
            super::locate_dwarf_bsp(tmp.path())
                .unwrap()
                .ends_with("sb441-n373s.bsp")
        );
        // Falls back to the full bundle when the compact subset is absent.
        std::fs::remove_file(dir.join("sb441-n373s.bsp")).unwrap();
        assert!(
            super::locate_dwarf_bsp(tmp.path())
                .unwrap()
                .ends_with("sb441-n373.bsp")
        );
    }

    #[test]
    fn collect_bsp_paths_is_recursive_sorted_and_bsp_only() {
        let tmp = tempdir::TempDir::new("collect").unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join("a/b")).unwrap();
        std::fs::write(root.join("a/z.bsp"), b"x").unwrap();
        std::fs::write(root.join("a/b/c.bsp"), b"x").unwrap();
        std::fs::write(root.join("a/notes.txt"), b"x").unwrap();
        let got = super::collect_bsp_paths(root);
        assert_eq!(got.len(), 2, "both nested .bsp, no .txt");
        assert!(
            got[0].ends_with("a/b/c.bsp") && got[1].ends_with("a/z.bsp"),
            "sorted"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::locate_default_bsp;

    /// Write a minimal valid DAF/SPK (file record + empty summary record) at
    /// `path`, creating parents. Zero segments — opens fine, covers nothing.
    /// Mirrors `open_all_tests::write_minimal_spk`.
    fn write_minimal_spk(path: &std::path::Path) {
        use std::io::Write;
        let mut file_rec = [0u8; 1024];
        file_rec[0..8].copy_from_slice(b"DAF/SPK ");
        file_rec[8..12].copy_from_slice(&2i32.to_le_bytes());
        file_rec[12..16].copy_from_slice(&6i32.to_le_bytes());
        file_rec[76..80].copy_from_slice(&2i32.to_le_bytes()); // FWARD=2
        file_rec[88..96].copy_from_slice(b"LTL-IEEE");
        let sum_rec = [0u8; 1024]; // NSUM=0
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut f = std::fs::File::create(path).unwrap();
        f.write_all(&file_rec).unwrap();
        f.write_all(&sum_rec).unwrap();
    }

    #[test]
    fn source_path_returns_the_opened_file() {
        let tmp = tempdir::TempDir::new("spk_source_path").unwrap();
        let path = tmp.path().join("synthetic.bsp");
        write_minimal_spk(&path);
        let spk = super::SpkEphemeris::open(&path).expect("open synthetic SPK");
        assert_eq!(spk.source_path(), path);
        assert!(spk.source_path().ends_with("synthetic.bsp"));
    }

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
