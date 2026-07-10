//! Locate JPL DE-series ephemeris files from any node in the mirror hierarchy.
//!
//! [`locate`] and [`open_dataset`] accept **any** directory in the JPL mirror
//! tree — the dataset leaf, a platform sub-dir, or the repository root — and
//! walk up to 8 levels deep to find the best available dataset. Binary is
//! preferred over ASCII at equal DE numbers; the highest-numbered series wins.
//!
//! [`open_dataset_for_year`] is the **date-aware** entry point the CLI uses: it
//! honours the oracle's precision-ordered preference and opens the smallest,
//! most accurate DE whose window covers the chart year (e.g. DE440 over DE441
//! for a modern date), falling back to `open_dataset` when nothing preferred is
//! present. Because it opens each entourage's *exact* header + binary by name,
//! it also handles layouts the convention-based `locate` cannot (see below).
//!
//! The same `PATH` value works for `--jpl-data` / `$STARCAT_JPL_DATA`
//! regardless of whether the user points at the `de441/` leaf or the
//! top-level `ssd.jpl.nasa.gov` mirror root.
//!
//! # The naming convention
//!
//! JPL distributes ephemerides as paired files:
//!
//! - `header.NNN` — the ASCII header (small, human-readable).
//! - `linux_*.NNN` — little-endian binary (used on x86/ARM systems).
//! - `xnp_*.NNN` — big-endian binary (legacy SPARC, PowerPC, etc).
//! - `ascp*.NNN` / `ascm*.NNN` — ASCII coefficient chunks (positive/negative JD).
//!
//! `NNN` is the ephemeris number (`440`, `441`, `442` …). A typical mirror
//! layout contains multiple dataset directories:
//!
//! ```text
//! ssd.jpl.nasa.gov/ftp/eph/planets/
//!     Linux/de441/
//!         header.441
//!         linux_m13000p17000.441
//!     ascii/de441/
//!         header.441
//!         ascp00000.441   ascp01826.441  …
//!         ascm32000.441   …
//! ```
//!
//! [`locate`] classifies every directory it visits as binary, ASCII, or
//! neither, and returns the best match. [`open_dataset`] then parses the
//! header and opens the backing store — either a memory-mapped
//! [`EphemerisFile`] or a lazy [`AsciiEphemeris`] — returning both through
//! the common [`RecordSource`] trait.
//!
//! # Why this matters
//!
//! When NASA releases DE442 in some future year, users should drop
//! the new files alongside DE441 and have the library pick them up
//! automatically — no environment-variable rename, no CLI flag
//! redesign. The data directory is the unit of distribution.
//!
//! # JPL DE-series cheat sheet
//!
//! Two parallel tracks since 1997 — standard window and long-range:
//!
//! | DE   | Year | Coverage             | Notes |
//! |------|------|----------------------|-------|
//! | 200  | 1982 | 1599–2169            | FK5-aligned; Swiss Ephemeris SE1 origin |
//! | 405  | 1997 | 1599–2201            | Gold standard 1997–2013; SE1 standard window; astro.com historically |
//! | 406  | 1997 | −3000 to +3000       | Long-range companion to DE405; SE1 extended window; truncated coefficients (NCOEFF 728) |
//! | 421  | 2008 | 1899–2053            | Major LLR update; HORIZONS default for a period |
//! | 422  | 2009 | −3000 to +3000       | Long-range companion to DE421; full precision |
//! | 430  | 2013 | 1549–2650            | MESSENGER data; 343 asteroids; SE2 / Solar Fire 9.x standard window |
//! | 430t | 2013 | 1549–2650            | DE430 + TT-TDB Chebyshev polynomial; same positions |
//! | 431  | 2013 | −13200 to +17191     | Long-range companion to DE430; SE2 extended window; Solar Fire 9.x |
//! | 440  | 2020 | 1549–2650            | Juno + Cassini Grand Finale data; current JPL standard |
//! | 440t | 2020 | 1549–2650            | DE440 + TT-TDB polynomial |
//! | 441  | 2020 | −13200 to +17191     | Long-range companion to DE440; **pericynthion default** |
//!
//! The `t` suffix adds a TT-TDB geocenter time-scale polynomial; positions are
//! identical to the non-`t` build.
//!
//! **Naming quirk (DE430/431 and older):** older releases use `lnx*` (not
//! `linux_*`) for binaries and `header.NNN_572` (not `header.NNN`) for the
//! extended-constant header. The convention-based [`locate`]/[`open_dataset`]
//! only recognise `linux_*` + plain `header.NNN`, so they skip DE430/431. The
//! date-aware [`open_dataset_for_year`] does not have this limitation: it opens
//! the entourage's exact `header.NNN_572`/`lnx*` pair by name, so those series
//! work on the CLI compute path with no symlinks.

use crate::error::PericynthionError;
use crate::jpl::ascii::AsciiEphemeris;
use crate::jpl::header::{self, Header};
use crate::jpl::reader::{EphemerisFile, RecordSource};
use std::path::{Path, PathBuf};

/// The result of a recursive ephemeris search: either a ready-to-use binary
/// dataset or an ASCII dataset that the caller can load with a parser.
#[derive(Debug, Clone)]
pub enum DatasetLocation {
    /// A binary (little-endian or big-endian) DE-series installation.
    Binary(JplDataPaths),
    /// An ASCII DE-series installation.
    Ascii {
        /// Path to the `header.NNN` file.
        header: PathBuf,
        /// Directory that contains the `asc[pm]*.NNN` coefficient files.
        dir: PathBuf,
        /// Ephemeris number (e.g. `441`).
        denum: u32,
    },
}

/// Attempt to recognise `dir` as an ASCII DE-series dataset.
///
/// Returns `Some((header_path, denum))` if `dir` contains both a `header.NNN`
/// and at least one `ascp*.NNN` or `ascm*.NNN` file sharing the same `NNN`.
fn discover_ascii(dir: &Path) -> Option<(PathBuf, u32)> {
    let entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .collect();

    // Find header.NNN candidates, sorted highest denum first.
    let mut candidates: Vec<(u32, PathBuf)> = entries
        .iter()
        .filter_map(|p| {
            let stem = p.file_stem().and_then(|s| s.to_str())?;
            let ext = p.extension().and_then(|s| s.to_str())?;
            if stem != "header" {
                return None;
            }
            let denum: u32 = ext.parse().ok()?;
            Some((denum, p.clone()))
        })
        .collect();
    candidates.sort_by_key(|(denum, _)| std::cmp::Reverse(*denum));

    // For each header, check for at least one ascp*.NNN or ascm*.NNN.
    for (denum, header_path) in candidates {
        let suffix = format!(".{denum}");
        let has_asc = entries.iter().any(|p| {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            name.ends_with(&suffix) && (name.starts_with("ascp") || name.starts_with("ascm"))
        });
        if has_asc {
            return Some((header_path, denum));
        }
    }
    None
}

/// A candidate found during the recursive walk, before ranking.
#[derive(Debug)]
enum Candidate {
    Binary(JplDataPaths),
    Ascii {
        header: PathBuf,
        dir: PathBuf,
        denum: u32,
    },
}

impl Candidate {
    fn denum(&self) -> u32 {
        match self {
            Candidate::Binary(p) => p.denum,
            Candidate::Ascii { denum, .. } => *denum,
        }
    }

    /// Higher is better: binary beats ascii at the same denum.
    fn rank(&self) -> (u32, u8) {
        (self.denum(), u8::from(matches!(self, Candidate::Binary(_))))
    }
}

/// Recursively walk `dir` up to `depth` levels deep and collect every
/// directory that looks like a DE-series dataset (binary or ASCII).
fn collect_candidates(dir: &Path, depth: u8, candidates: &mut Vec<Candidate>) {
    // Try to classify the current directory first.
    if let Ok(paths) = discover(dir) {
        candidates.push(Candidate::Binary(paths));
        // Don't descend further into a dataset dir — its children aren't datasets.
        return;
    }
    if let Some((header, denum)) = discover_ascii(dir) {
        candidates.push(Candidate::Ascii {
            header,
            dir: dir.to_path_buf(),
            denum,
        });
        return;
    }

    if depth == 0 {
        return;
    }

    // Recurse into subdirectories.
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in read.filter_map(Result::ok) {
        let child = entry.path();
        if child.is_dir() {
            collect_candidates(&child, depth - 1, candidates);
        }
    }
}

/// Locate the best DE-series ephemeris dataset reachable from `start`.
///
/// Walks the directory hierarchy up to 8 levels deep, classifying each
/// directory as a binary or ASCII DE dataset. The dataset with the highest
/// ephemeris number wins; at equal DE number, binary is preferred over ASCII.
///
/// Accepts any node in the JPL mirror tree — the dataset directory itself,
/// `.../planets/Linux/`, `.../planets/`, `.../eph/`, `.../ftp/`, the
/// `ssd.jpl.nasa.gov` root, or a parent containing it.
///
/// # Errors
///
/// Returns a `PericynthionError::Io` if no DE dataset (binary or ASCII) is
/// found anywhere under `start`.
pub fn locate(start: &Path) -> Result<DatasetLocation, PericynthionError> {
    // Canonicalise trailing slashes / `.` components without requiring the
    // path to exist on a real filesystem (TempDir paths are already canonical).
    let start = if start.is_dir() {
        start.to_path_buf()
    } else {
        return Err(PericynthionError::Io {
            path: start.to_path_buf(),
            source: std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("{} is not a directory", crate::display_path(start)),
            ),
        });
    };

    let mut candidates: Vec<Candidate> = Vec::new();
    collect_candidates(&start, 8, &mut candidates);

    // Fallback for layouts deeper than the recursive cap (or otherwise missed):
    // locate any `header.<digits>` file with the layout-agnostic tree walker and
    // classify its parent directory. This resolves a flat drop-folder no matter
    // how deep it sits under `start`.
    if candidates.is_empty()
        && let Some(header) = crate::locate_jpl_file_matching(&start, is_de_header_name)
        && let Some(parent) = header.parent()
    {
        if let Ok(paths) = discover(parent) {
            candidates.push(Candidate::Binary(paths));
        } else if let Some((hdr, denum)) = discover_ascii(parent) {
            candidates.push(Candidate::Ascii {
                header: hdr,
                dir: parent.to_path_buf(),
                denum,
            });
        }
    }

    candidates
        .into_iter()
        .max_by_key(Candidate::rank)
        .map(|c| match c {
            Candidate::Binary(p) => DatasetLocation::Binary(p),
            Candidate::Ascii { header, dir, denum } => {
                DatasetLocation::Ascii { header, dir, denum }
            }
        })
        .ok_or_else(|| PericynthionError::Io {
            path: start.clone(),
            source: std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!(
                    "no DE dataset (binary or ASCII) found under {}",
                    crate::display_path(&start)
                ),
            ),
        })
}

/// A header + binary file pair from a JPL DE-series installation.
#[derive(Debug, Clone)]
pub struct JplDataPaths {
    /// Path to the ASCII header file (`header.NNN`).
    pub header: PathBuf,
    /// Path to the binary ephemeris file (`linux_*.NNN` preferred).
    pub binary: PathBuf,
    /// The ephemeris number (e.g. 441). Useful for diagnostic output.
    pub denum: u32,
}

/// True when `name` is a DE-series ASCII header file name: `header.` followed
/// by one or more ASCII digits (e.g. `header.441`). Used by the walker fallback
/// in [`locate`] to find a dataset directory in any layout.
fn is_de_header_name(name: &str) -> bool {
    name.strip_prefix("header.")
        .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

/// Discover the highest-numbered ephemeris in `dir`, returning the
/// header/binary pair.
///
/// # Errors
///
/// Returns an I/O error if `dir` cannot be read, or if no matching
/// `header.NNN` + binary pair is found. The error message lists what
/// the directory does contain to aid diagnosis.
pub fn discover(dir: &Path) -> Result<JplDataPaths, PericynthionError> {
    if !dir.is_dir() {
        return Err(PericynthionError::Io {
            path: dir.to_path_buf(),
            source: std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("{} is not a directory", crate::display_path(dir)),
            ),
        });
    }
    let entries: Vec<_> = std::fs::read_dir(dir)
        .map_err(|source| PericynthionError::Io {
            path: dir.to_path_buf(),
            source,
        })?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .collect();

    // Find all header.NNN files and extract their NNN.
    let mut candidates: Vec<(u32, PathBuf)> = entries
        .iter()
        .filter_map(|p| {
            let stem = p.file_stem().and_then(|s| s.to_str())?;
            let ext = p.extension().and_then(|s| s.to_str())?;
            if stem != "header" {
                return None;
            }
            let denum: u32 = ext.parse().ok()?;
            Some((denum, p.clone()))
        })
        .collect();
    candidates.sort_by_key(|(denum, _)| std::cmp::Reverse(*denum));

    if candidates.is_empty() {
        let listing: Vec<String> = entries
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()).map(String::from))
            .collect();
        return Err(PericynthionError::Io {
            path: dir.to_path_buf(),
            source: std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!(
                    "no `header.NNN` file in {}. Contains: {}",
                    crate::display_path(dir),
                    listing.join(", ")
                ),
            ),
        });
    }

    // For each header.NNN, look for a matching binary.
    for (denum, header_path) in &candidates {
        if let Some(binary) = find_binary_for_denum(&entries, *denum) {
            return Ok(JplDataPaths {
                header: header_path.clone(),
                binary,
                denum: *denum,
            });
        }
    }

    // Headers exist but no binary matched.
    let header_list: Vec<String> = candidates
        .iter()
        .filter_map(|(_, p)| p.file_name().and_then(|n| n.to_str()).map(String::from))
        .collect();
    Err(PericynthionError::Io {
        path: dir.to_path_buf(),
        source: std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!(
                "found header(s) {:?} in {} but no matching `linux_*.NNN` or `xnp_*.NNN` binary",
                header_list,
                crate::display_path(dir)
            ),
        ),
    })
}

/// Look for the binary file matching a given ephemeris number. Prefers
/// `linux_*.NNN` (little-endian); falls back to `xnp_*.NNN` (big-endian).
/// If multiple matches, the largest file wins (more coverage).
fn find_binary_for_denum(entries: &[PathBuf], denum: u32) -> Option<PathBuf> {
    let suffix = format!(".{denum}");
    let mut matches: Vec<(u64, PathBuf, bool)> = entries
        .iter()
        .filter_map(|p| {
            let name = p.file_name()?.to_str()?;
            if !name.ends_with(&suffix) {
                return None;
            }
            let is_linux = name.starts_with("linux_");
            let is_bigendian = name.starts_with("xnp_");
            if !is_linux && !is_bigendian {
                return None;
            }
            let size = p.metadata().ok()?.len();
            Some((size, p.clone(), is_linux))
        })
        .collect();
    // Sort: prefer little-endian, then largest size.
    matches.sort_by(|a, b| {
        b.2.cmp(&a.2) // is_le descending (true first)
            .then(b.0.cmp(&a.0)) // size descending
    });
    matches.into_iter().next().map(|(_, p, _)| p)
}

/// Locate the best DE-series dataset reachable from `start` and open it,
/// returning a parsed [`Header`] and a boxed [`RecordSource`] ready for
/// Chebyshev evaluation.
///
/// This is the single-call convenience wrapper for the common case: call
/// [`locate`] to find the dataset, parse its header, and open the backing
/// store (memory-mapped binary or lazy ASCII reader). The caller does not
/// need to branch on binary vs ASCII — both are returned through the same
/// `Box<dyn RecordSource>` interface.
///
/// Accepts any node in the JPL mirror tree: the dataset directory itself,
/// `.../planets/Linux/`, `.../planets/`, `.../ftp/`, the
/// `ssd.jpl.nasa.gov` root, or a parent containing it.
///
/// # Errors
///
/// Returns a [`PericynthionError`] if:
/// - no DE dataset (binary or ASCII) is found under `start`,
/// - the `header.NNN` file cannot be read or fails to parse, or
/// - the backing store (binary mmap or ASCII chunk index) cannot be opened.
pub fn open_dataset(start: &Path) -> Result<(Header, Box<dyn RecordSource>), PericynthionError> {
    match locate(start)? {
        DatasetLocation::Binary(paths) => open_binary_pair(&paths.header, &paths.binary),
        DatasetLocation::Ascii {
            header: hdr_path,
            dir,
            ..
        } => {
            let source = std::fs::read_to_string(&hdr_path).map_err(|e| PericynthionError::Io {
                path: hdr_path.clone(),
                source: e,
            })?;
            // Propagate the structured HeaderError via `?`/From<HeaderError>
            // rather than flattening it into an opaque Io(InvalidData).
            let header = header::parse(&source)?;
            let ascii = AsciiEphemeris::open(&dir, &header)?;
            Ok((header, Box::new(ascii)))
        }
    }
}

/// Open a specific located `header.NNN` + binary pair into a ready dataset,
/// bypassing filename-convention discovery.
///
/// The date-aware selector knows *exactly* which header pairs with which binary
/// (from the oracle entourage), so it opens them directly — which also works for
/// the `header.NNN_572` (DE430/431) headers that [`locate`] cannot classify by
/// name.
///
/// # Errors
/// [`PericynthionError::Io`] if the header cannot be read, or the header/binary
/// fail to parse/open.
pub fn open_binary_pair(
    header_path: &Path,
    binary_path: &Path,
) -> Result<(Header, Box<dyn RecordSource>), PericynthionError> {
    let source = std::fs::read_to_string(header_path).map_err(|e| PericynthionError::Io {
        path: header_path.to_path_buf(),
        source: e,
    })?;
    let header = header::parse(&source)?;
    let file = EphemerisFile::open(binary_path, &header)?;
    Ok((header, Box::new(file)))
}

/// The (header basename, binary basename) of an entourage's planetary dataset —
/// the header is the `planets` file whose name starts `header.`, the binary is
/// the other. `None` if the entourage lacks one of them.
fn entourage_de_files(ent: &crate::jpl::oracle::Entourage) -> Option<(&'static str, &'static str)> {
    let base = |url: &'static str| url.rsplit('/').next().unwrap_or(url);
    let mut header = None;
    let mut binary = None;
    for &url in ent.planets {
        let b = base(url);
        if b.starts_with("header.") {
            header = Some(b);
        } else {
            binary = Some(b);
        }
    }
    Some((header?, binary?))
}

/// Locate + open the best DE dataset for `year`, honoring the oracle's
/// precision-ordered [`de_preference`](crate::jpl::oracle::de_preference).
///
/// Walks the preference list (best-precision first) and opens the first entry
/// whose window [`covers`](crate::jpl::oracle::DePreference::covers) `year` AND
/// whose header + binary are both present under `start`. So with both DE440 and
/// DE441 on disk, a modern date resolves to the smaller, marginally more
/// accurate DE440; a deep-time date falls through to DE441.
///
/// Falls back to [`open_dataset`] (highest-denum, date-blind) when no
/// preferred+present dataset covers `year`, preserving behavior for custom or
/// unrecognized data layouts.
///
/// # Errors
/// [`PericynthionError`] if the chosen (or fallback) dataset cannot be opened.
pub fn open_dataset_for_year(
    start: &Path,
    year: i32,
) -> Result<(Header, Box<dyn RecordSource>), PericynthionError> {
    if let Some((header, binary)) = select_de_pair_for_year(start, year) {
        return open_binary_pair(&header, &binary);
    }
    // Nothing preferred is present for this year — fall back to discovery.
    open_dataset(start)
}

/// The (header path, binary path) of the best DE covering `year` that is present
/// under `start`, per the oracle's precision-ordered
/// [`de_preference`](crate::jpl::oracle::de_preference); `None` if no preferred
/// dataset covering `year` is on disk. Pure locate — opens nothing.
#[must_use]
pub fn select_de_pair_for_year(start: &Path, year: i32) -> Option<(PathBuf, PathBuf)> {
    for pref in crate::jpl::oracle::de_preference() {
        if !pref.covers(year) {
            continue;
        }
        let Some(ent) = crate::jpl::oracle::entourage(pref.slug) else {
            continue;
        };
        let Some((header_name, binary_name)) = entourage_de_files(ent) else {
            continue;
        };
        if let Some(header) = crate::locate_jpl_file(start, header_name)
            && let Some(binary) = crate::locate_jpl_file(start, binary_name)
        {
            return Some((header, binary));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;

    fn make_dir() -> tempdir::TempDir {
        tempdir::TempDir::new("starcat-discover-test").unwrap()
    }

    fn write_file(dir: &Path, name: &str, size: usize) -> PathBuf {
        let path = dir.join(name);
        let mut f = fs::File::create(&path).unwrap();
        f.write_all(&vec![0u8; size]).unwrap();
        path
    }

    #[test]
    fn discover_single_de441_pair() {
        let tmp = make_dir();
        write_file(tmp.path(), "header.441", 22_802);
        let bin = write_file(tmp.path(), "linux_m13000p17000.441", 1024);
        let result = discover(tmp.path()).unwrap();
        assert_eq!(result.denum, 441);
        assert_eq!(result.binary, bin);
    }

    #[test]
    fn discover_prefers_higher_denum_when_multiple_present() {
        let tmp = make_dir();
        write_file(tmp.path(), "header.440", 22_000);
        write_file(tmp.path(), "linux_de440.440", 1024);
        write_file(tmp.path(), "header.441", 22_802);
        let bin441 = write_file(tmp.path(), "linux_m13000p17000.441", 2048);
        let result = discover(tmp.path()).unwrap();
        assert_eq!(result.denum, 441);
        assert_eq!(result.binary, bin441);
    }

    #[test]
    fn discover_prefers_little_endian_when_both_present() {
        let tmp = make_dir();
        write_file(tmp.path(), "header.441", 22_802);
        write_file(tmp.path(), "xnp_de441.441", 4096); // big-endian, larger
        let bin_le = write_file(tmp.path(), "linux_de441.441", 1024);
        let result = discover(tmp.path()).unwrap();
        assert_eq!(result.binary, bin_le);
    }

    #[test]
    fn discover_picks_largest_le_when_multiple_le_files() {
        let tmp = make_dir();
        write_file(tmp.path(), "header.441", 22_802);
        write_file(tmp.path(), "linux_short.441", 1024);
        let big = write_file(tmp.path(), "linux_full.441", 16_384);
        let result = discover(tmp.path()).unwrap();
        assert_eq!(result.binary, big);
    }

    #[test]
    fn discover_errors_when_no_header() {
        let tmp = make_dir();
        write_file(tmp.path(), "linux_de441.441", 1024);
        let err = discover(tmp.path()).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("no `header.NNN`"));
    }

    #[test]
    fn discover_errors_when_header_without_binary() {
        let tmp = make_dir();
        write_file(tmp.path(), "header.441", 22_802);
        let err = discover(tmp.path()).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("no matching"));
    }

    #[test]
    fn discover_errors_for_nonexistent_directory() {
        let err = discover(Path::new("/nonexistent-starcat-test-path-xxxx")).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("not a directory"));
    }

    #[test]
    fn locate_finds_binary_from_mirror_root() {
        let tmp = make_dir();
        let de = tmp
            .path()
            .join("ssd.jpl.nasa.gov/ftp/eph/planets/Linux/de441");
        std::fs::create_dir_all(&de).unwrap();
        write_file(&de, "header.441", 22_802);
        write_file(&de, "linux_m13000p17000.441", 8144 * 3);
        let loc = super::locate(tmp.path()).unwrap();
        assert!(matches!(loc, super::DatasetLocation::Binary(_)));
    }

    #[test]
    fn locate_finds_binary_in_flat_folder() {
        // A normal user dropped header.441 + linux_*.441 into one flat folder
        // (no ssd.jpl.nasa.gov mirror tree). locate must still resolve it.
        let tmp = make_dir();
        write_file(tmp.path(), "header.441", 22_802);
        let bin = write_file(tmp.path(), "linux_m13000p17000.441", 8144 * 3);
        let loc = super::locate(tmp.path()).unwrap();
        match loc {
            super::DatasetLocation::Binary(p) => {
                assert_eq!(p.denum, 441);
                assert_eq!(p.binary, bin);
            }
            l @ super::DatasetLocation::Ascii { .. } => panic!("expected Binary, got {l:?}"),
        }
    }

    #[test]
    fn locate_finds_binary_in_flat_folder_nested_below_start() {
        // The flat folder is nested a few levels below the pointed-at root.
        let tmp = make_dir();
        let de = tmp.path().join("a/b/de441_flat");
        std::fs::create_dir_all(&de).unwrap();
        write_file(&de, "header.441", 22_802);
        let bin = write_file(&de, "linux_m13000p17000.441", 8144 * 3);
        let loc = super::locate(tmp.path()).unwrap();
        match loc {
            super::DatasetLocation::Binary(p) => assert_eq!(p.binary, bin),
            l @ super::DatasetLocation::Ascii { .. } => panic!("expected Binary, got {l:?}"),
        }
    }

    #[test]
    fn locate_finds_binary_flat_folder_deeper_than_recursion_cap() {
        // Nest the dataset dir 12 levels deep — past the depth-8 recursive cap —
        // so only the walker fallback in `locate` can find it.
        let tmp = make_dir();
        let mut de = tmp.path().to_path_buf();
        for i in 0..12 {
            de = de.join(format!("lvl{i}"));
        }
        std::fs::create_dir_all(&de).unwrap();
        write_file(&de, "header.441", 22_802);
        let bin = write_file(&de, "linux_m13000p17000.441", 8144 * 3);
        let loc = super::locate(tmp.path()).unwrap();
        match loc {
            super::DatasetLocation::Binary(p) => assert_eq!(p.binary, bin),
            l @ super::DatasetLocation::Ascii { .. } => panic!("expected Binary, got {l:?}"),
        }
    }

    #[test]
    fn locate_falls_back_to_ascii_when_no_binary() {
        let tmp = make_dir();
        let de = tmp
            .path()
            .join("ssd.jpl.nasa.gov/ftp/eph/planets/ascii/de441");
        std::fs::create_dir_all(&de).unwrap();
        write_file(&de, "header.441", 22_802);
        write_file(&de, "ascp00000.441", 64);
        let loc = super::locate(tmp.path()).unwrap();
        assert!(matches!(
            loc,
            super::DatasetLocation::Ascii { denum: 441, .. }
        ));
    }

    #[test]
    fn locate_prefers_binary_over_ascii_same_denum() {
        let tmp = make_dir();
        let root = tmp.path().join("eph/planets");
        let bin = root.join("Linux/de441");
        let asc = root.join("ascii/de441");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::create_dir_all(&asc).unwrap();
        write_file(&bin, "header.441", 22_802);
        write_file(&bin, "linux_x.441", 8144 * 3);
        write_file(&asc, "header.441", 22_802);
        write_file(&asc, "ascp00000.441", 64);
        assert!(matches!(
            super::locate(tmp.path()).unwrap(),
            super::DatasetLocation::Binary(_)
        ));
    }

    /// `open_dataset` resolves and opens the real DE441 dataset.
    ///
    /// A fully synthetic binary test is not practical: `EphemerisFile::open`
    /// validates the file by probing the first coefficient record's time-tag
    /// bytes for a plausible Julian Date and a 32-day granule span — a zero-
    /// filled stub will always fail that check. We therefore test against the
    /// real file under `STARCAT_JPL_DATA` (any hierarchy node) and skip
    /// cleanly when unset, consistent with all other integration tests in this
    /// crate. The assertion that `granule_days ≈ 32` and `start_jd` is finite
    /// exercises the trait dispatch from `Box<dyn RecordSource>`.
    #[test]
    fn open_dataset_returns_usable_record_source() {
        let Some(val) = std::env::var_os("STARCAT_JPL_DATA") else {
            eprintln!("STARCAT_JPL_DATA not set — skipping open_dataset integration test");
            return;
        };
        let start = Path::new(&val);
        let (_header, source) = super::open_dataset(start).expect("open_dataset should succeed");
        assert!(
            source.start_jd().is_finite(),
            "start_jd must be a finite Julian Date; got {}",
            source.start_jd()
        );
        assert!(
            (source.granule_days() - 32.0).abs() < 1e-9,
            "DE-series granule size must be ≈ 32 days; got {}",
            source.granule_days()
        );
    }

    /// Lay down presence-only `header.NNN` + binary files for a DE series under a
    /// synthetic mirror. Content is not parsed by `select_de_pair_for_year`.
    fn place_de(root: &Path, subdir: &str, header: &str, binary: &str) {
        let dir = root
            .join("ssd.jpl.nasa.gov/ftp/eph/planets/Linux")
            .join(subdir);
        fs::create_dir_all(&dir).unwrap();
        write_file(&dir, header, 8);
        write_file(&dir, binary, 8);
    }

    #[test]
    fn select_de_pair_prefers_de440_over_de441_for_a_modern_year() {
        let tmp = make_dir();
        let root = tmp.path();
        place_de(root, "de440", "header.440", "linux_p1550p2650.440");
        place_de(root, "de441", "header.441", "linux_m13000p17000.441");

        // Modern date: DE440 (smaller, marginally more precise) wins over DE441.
        let (h, b) = super::select_de_pair_for_year(root, 2026).expect("a pair");
        assert!(h.ends_with("de440/header.440"), "{h:?}");
        assert!(b.ends_with("de440/linux_p1550p2650.440"), "{b:?}");

        // Deep-time date: DE440's window excludes it, so it falls to DE441.
        let (h, b) = super::select_de_pair_for_year(root, -5000).expect("a pair");
        assert!(h.ends_with("de441/header.441"), "{h:?}");
        assert!(b.ends_with("de441/linux_m13000p17000.441"), "{b:?}");
    }

    #[test]
    fn select_de_pair_falls_to_de441_when_de440_absent() {
        let tmp = make_dir();
        let root = tmp.path();
        place_de(root, "de441", "header.441", "linux_m13000p17000.441");
        // A modern date still resolves — DE441 is the preferred one present.
        let (h, _) = super::select_de_pair_for_year(root, 2026).expect("de441 covers 2026");
        assert!(h.ends_with("de441/header.441"), "{h:?}");
    }

    #[test]
    fn select_de_pair_is_none_when_nothing_present() {
        let tmp = make_dir();
        assert!(super::select_de_pair_for_year(tmp.path(), 2026).is_none());
        // A half-present dataset (header without its binary) is not selectable.
        let root = tmp.path();
        let dir = root.join("ssd.jpl.nasa.gov/ftp/eph/planets/Linux/de440");
        fs::create_dir_all(&dir).unwrap();
        write_file(&dir, "header.440", 8);
        assert!(super::select_de_pair_for_year(root, 2026).is_none());
    }
}
