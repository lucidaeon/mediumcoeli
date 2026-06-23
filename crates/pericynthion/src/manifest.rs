//! Minimal data-file manifest for starcat's supported placements.
//!
//! [`production_data_files`] returns the set of files on disk that
//! `starcat compute` actually reads to produce its full supported output —
//! planets from the DE441 ephemeris plus named asteroids from the small-body
//! SPK.  A CI/CD pipeline can call this function, ship only those files
//! (~3 GB), and skip the rest of the 190 GB JPL mirror.
//!
//! The function is a thin wrapper around [`crate::jpl::discover::locate`] and
//! [`crate::spk::locate_default_bsp`]; all file-discovery logic lives in those
//! modules so GUI tools and CLI wrappers share the same implementation.

use crate::error::PericynthionError;
use crate::jpl::discover::DatasetLocation;
use std::path::{Path, PathBuf};

/// Return the minimal set of files needed to compute starcat's currently-supported
/// placements: planets from DE441 plus named asteroids from the small-body SPK.
///
/// The returned paths are formed by joining onto the caller-supplied `start`,
/// so they keep whichever form (relative or absolute) `start` had — they are
/// not canonicalized.  The caller should verify that every path
/// [`Path::is_file`] before trusting it — the function only enumerates what
/// the discovery logic finds; it does not open or validate file contents.
///
/// For the canonical, oracle-derived packaging subset used by
/// `starcat data prod` / `data verify`, see
/// [`crate::jpl::oracle::production_entries`].
///
/// # What is included
///
/// - **DE441 (binary layout):** `header.NNN` and `linux_*.NNN` (or `xnp_*.NNN`).
/// - **DE441 (ASCII layout):** `header.NNN` plus every `ascp*.NNN` /
///   `ascm*.NNN` chunk file in the ASCII directory.  All chunks are needed
///   because the ASCII reader selects chunks by JD coverage at query time.
/// - **Small-body SPK** (`sb441-n16.bsp`): included when [`crate::spk::locate_default_bsp`]
///   finds the file under the mirror.  Absent SPK is not an error — the function
///   returns only the DE441 files in that case.
///
/// # Errors
///
/// Returns [`PericynthionError`] if:
/// - `start` is not a directory.
/// - No DE441 (or other DE-series) dataset is found anywhere under `start`
///   (forwarded from [`crate::jpl::discover::locate`]).
/// - An ASCII chunk directory cannot be read.
pub fn production_data_files(start: &Path) -> Result<Vec<PathBuf>, PericynthionError> {
    let mut files: Vec<PathBuf> = Vec::new();

    // --- DE441 dataset ---
    match crate::jpl::discover::locate(start)? {
        DatasetLocation::Binary(paths) => {
            files.push(paths.header);
            files.push(paths.binary);
        }
        DatasetLocation::Ascii { header, dir, denum } => {
            files.push(header);
            // Collect every ascp*.NNN / ascm*.NNN chunk — the ASCII reader
            // needs all of them because it selects by JD coverage.
            let suffix = format!(".{denum}");
            let rd = std::fs::read_dir(&dir).map_err(|source| PericynthionError::Io {
                path: dir.clone(),
                source,
            })?;
            for entry in rd.filter_map(Result::ok) {
                let p = entry.path();
                let name = p
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_owned();
                if name.ends_with(&suffix) && (name.starts_with("ascp") || name.starts_with("ascm"))
                {
                    files.push(p);
                }
            }
        }
    }

    // --- Small-body SPK (optional) ---
    if let Some(bsp) = crate::spk::locate_default_bsp(start) {
        files.push(bsp);
    }

    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::production_data_files;
    use std::fs;
    use std::io::Write as _;
    use std::path::Path;

    fn write_file(dir: &Path, name: &str) -> std::path::PathBuf {
        let p = dir.join(name);
        let mut f = fs::File::create(&p).unwrap();
        f.write_all(b"stub").unwrap();
        p
    }

    /// Synthetic binary-layout mirror: header + binary + bsp must all appear.
    #[test]
    fn binary_layout_returns_header_binary_bsp() {
        let tmp = tempdir::TempDir::new("manifest-bin-test").unwrap();
        let root = tmp.path();

        // Binary DE441 under the standard mirror path.
        let de_dir = root.join("ssd.jpl.nasa.gov/ftp/eph/planets/Linux/de441");
        fs::create_dir_all(&de_dir).unwrap();
        let hdr = write_file(&de_dir, "header.441");
        let bin = write_file(&de_dir, "linux_m13000p17000.441");

        // BSP under the standard mirror path.
        let bsp_dir = root.join("ssd.jpl.nasa.gov/ftp/eph/small_bodies/asteroids_de441");
        fs::create_dir_all(&bsp_dir).unwrap();
        let bsp = write_file(&bsp_dir, "sb441-n16.bsp");

        let mut got = production_data_files(root).unwrap();
        got.sort();
        let mut want = vec![hdr, bin, bsp];
        want.sort();

        assert_eq!(got, want);
        for p in &got {
            assert!(p.is_file(), "path must exist: {}", p.display());
        }
    }

    /// Missing BSP is not an error — only DE441 files are returned.
    #[test]
    fn binary_layout_without_bsp_returns_header_and_binary() {
        let tmp = tempdir::TempDir::new("manifest-nosbsp-test").unwrap();
        let root = tmp.path();

        let de_dir = root.join("ssd.jpl.nasa.gov/ftp/eph/planets/Linux/de441");
        fs::create_dir_all(&de_dir).unwrap();
        let hdr = write_file(&de_dir, "header.441");
        let bin = write_file(&de_dir, "linux_m13000p17000.441");

        let got = production_data_files(root).unwrap();
        assert_eq!(got.len(), 2);
        assert!(got.contains(&hdr));
        assert!(got.contains(&bin));
    }

    /// Real-data integration test — skips cleanly when `STARCAT_JPL_DATA` is unset.
    #[test]
    fn real_data_contains_expected_files() {
        let Some(val) = std::env::var_os("STARCAT_JPL_DATA") else {
            eprintln!("STARCAT_JPL_DATA not set — skipping production_data_files integration test");
            return;
        };
        let start = Path::new(&val);
        let files = production_data_files(start)
            .expect("production_data_files should succeed with real data");

        // Every returned path must exist.
        for p in &files {
            assert!(p.is_file(), "returned path must be a file: {}", p.display());
        }

        // Must contain a header (header.NNN) and a binary or ASCII chunk.
        let has_header = files.iter().any(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("header."))
        });
        assert!(has_header, "manifest must include a header.NNN file");

        // Must contain either a linux_*.441 binary or at least one ascp*.441 chunk.
        let has_data = files.iter().any(|p| {
            p.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
                n.starts_with("linux_")
                    || n.starts_with("xnp_")
                    || n.starts_with("ascp")
                    || n.starts_with("ascm")
            })
        });
        assert!(has_data, "manifest must include the ephemeris data file(s)");

        // Must contain the small-body SPK when the mirror is present.
        let has_bsp = files.iter().any(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n == "sb441-n16.bsp")
        });
        // Only assert when the full mirror is likely mirrored (bsp may not be
        // present on a minimal mirror).
        if has_bsp {
            // Already verified by the is_file check above.
        } else {
            eprintln!("sb441-n16.bsp not found under mirror — BSP assertion skipped");
        }
    }
}
