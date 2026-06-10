//! Auto-discover JPL DE-series ephemeris files within a data directory.
//!
//! # The naming convention
//!
//! JPL distributes ephemerides as paired files:
//!
//! - `header.NNN` — the ASCII header (small, human-readable).
//! - `linux_*.NNN` — little-endian binary (used on x86/ARM systems).
//! - `xnp_*.NNN` — big-endian binary (legacy SPARC, PowerPC, etc).
//!
//! `NNN` is the ephemeris number (`440`, `441`, `442` …). A typical
//! installation directory holds:
//!
//! ```text
//! ~/.../planets/Linux/de441/
//!     header.441
//!     linux_m13000p17000.441
//! ```
//!
//! This module finds the right pair given just the directory path. If
//! multiple ephemeris versions are present, the highest-numbered one
//! wins — newer DE-series releases supersede older ones in published
//! accuracy.
//!
//! # Why this matters
//!
//! When NASA releases DE442 in some future year, users should drop
//! the new files alongside DE441 and have the library pick them up
//! automatically — no environment-variable rename, no CLI flag
//! redesign. The data directory is the unit of distribution.

use crate::error::PericynthionError;
use std::path::{Path, PathBuf};

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
                format!("{} is not a directory", dir.display()),
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
                    dir.display(),
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
                dir.display()
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
}
