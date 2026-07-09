//! Home-dir ephemeris store: default data directory, named datasets, and a
//! resumable, self-verifying fetcher for the JPL production subset.

use crate::jpl::oracle::{self, OracleEntry};

/// The platform-native persistent **data** directory for starcat's ephemerides
/// (not a cache dir — never subject to OS cache eviction).
///
/// macOS `~/Library/Application Support/starcat/`, Linux `$XDG_DATA_HOME/starcat/`,
/// Windows `%APPDATA%\starcat\data\`. `None` if no platform base dir is available.
#[cfg(feature = "data-dir")]
#[must_use]
pub fn default_data_dir() -> Option<std::path::PathBuf> {
    directories::ProjectDirs::from("", "", "starcat").map(|d| d.data_dir().to_path_buf())
}

/// A named, fetchable bundle of oracle files. The registry
/// ([`datasets`]) is the single extension point for "name a dataset to fetch".
#[derive(Debug, Clone, Copy)]
pub struct Dataset {
    /// CLI slug, e.g. `"de441"`.
    pub slug: &'static str,
    /// One-line human description.
    pub description: &'static str,
}

impl Dataset {
    /// The oracle entries this dataset comprises (URLs + hashes come from the
    /// oracle).
    #[must_use]
    pub fn entries(&self) -> Vec<OracleEntry> {
        match self.slug {
            "de441" => oracle::production_entries(),
            _ => Vec::new(),
        }
    }
}

static DATASETS: &[Dataset] = &[Dataset {
    slug: "de441",
    description: "DE441 production subset (planets + core small bodies), ~2.8 GB",
}];

/// The registered datasets. v1: exactly `de441`.
#[must_use]
pub fn datasets() -> &'static [Dataset] {
    DATASETS
}

/// Look up a dataset by slug.
#[must_use]
pub fn dataset_from_slug(slug: &str) -> Option<&'static Dataset> {
    DATASETS.iter().find(|d| d.slug == slug)
}

#[cfg(feature = "data-fetch")]
mod fetch {
    use super::{Dataset, oracle};
    use crate::jpl::oracle::{OracleEntry, VerifyStatus};
    use std::io::{Read, Write};
    use std::path::{Path, PathBuf};

    /// Per-file download progress, reported as bytes arrive. UI-agnostic.
    #[derive(Debug, Clone)]
    pub struct FetchProgress {
        /// Zero-based index of the file currently being fetched.
        pub file_index: usize,
        /// Total number of files in the batch.
        pub file_count: usize,
        /// The file name component of the current file's path.
        pub file_name: String,
        /// Bytes received so far for this file (including any resumed prefix).
        pub bytes_done: u64,
        /// Expected total byte count from the oracle.
        pub bytes_total: u64,
    }

    /// What a fetch touched.
    #[derive(Debug, Default)]
    pub struct FetchSummary {
        /// Files that were freshly downloaded (and verified).
        pub downloaded: Vec<PathBuf>,
        /// Files cloned copy-on-write from the source mirror (~0 disk on
        /// APFS/btrfs/ReFS), verified after cloning.
        pub reflinked: Vec<PathBuf>,
        /// Files copied in full from the source mirror (the off-CoW fallback),
        /// verified after copying.
        pub copied: Vec<PathBuf>,
        /// Files that were already present and hash-valid (skipped).
        pub skipped: Vec<PathBuf>,
    }

    /// Fetch failures.
    #[derive(Debug, thiserror::Error)]
    pub enum FetchError {
        /// An HTTP-level error (connection, redirect, non-2xx after retry, etc.).
        #[error("HTTP error fetching {url}: {source}")]
        Http {
            /// The URL that was being fetched when the error occurred.
            url: String,
            /// The underlying reqwest error.
            source: reqwest::Error,
        },
        /// A filesystem or stream I/O error.
        #[error("I/O error at {path}: {source}")]
        Io {
            /// The file path associated with the I/O error.
            path: PathBuf,
            /// The underlying I/O error.
            source: std::io::Error,
        },
        /// The downloaded file's BLAKE3 did not match the oracle record.
        #[error("checksum mismatch for {path}: expected {expected}, got {actual}")]
        Verify {
            /// The destination path of the file that failed verification.
            path: PathBuf,
            /// Expected BLAKE3 hex digest from the oracle.
            expected: &'static str,
            /// Actual BLAKE3 hex digest computed from the downloaded file.
            actual: String,
        },
    }

    /// Source URL for an entry: `https://{path}`.
    #[must_use]
    pub fn entry_url(entry: &OracleEntry) -> String {
        format!("https://{}", entry.path)
    }

    /// The partial-download sidecar: the full filename plus `.part`.
    #[must_use]
    pub fn part_path(target: &Path) -> PathBuf {
        let mut s = target.as_os_str().to_owned();
        s.push(".part");
        PathBuf::from(s)
    }

    fn io(path: &Path) -> impl Fn(std::io::Error) -> FetchError + '_ {
        move |source| FetchError::Io {
            path: path.to_path_buf(),
            source,
        }
    }

    /// Fetch `entries` into `root`, preferring local data over the network:
    ///
    /// 1. Already present + BLAKE3-valid in `root` -> skip (no network, no copy).
    /// 2. Not in `root`, but a base-name match is present + BLAKE3-valid
    ///    anywhere under `source` (a distinct existing mirror, in any layout —
    ///    full `ssd.jpl.nasa.gov/` tree or a flat drop-folder; located by
    ///    walking, see [`crate::find_under`]) -> copy-on-write clone into `root`
    ///    at the entry's canonical path (via [`reflink_copy::reflink_or_copy`]),
    ///    then re-verify. On a true reflink the file is recorded in
    ///    [`FetchSummary::reflinked`]; on the full-copy fallback, in
    ///    [`FetchSummary::copied`]. If the clone fails verification it is removed
    ///    and the entry falls through to a download.
    /// 3. Valid nowhere locally -> download.
    ///
    /// The `source` mirror is only ever read from (cloned/copied) — never moved
    /// or deleted.
    ///
    /// # Errors
    /// Returns [`FetchError::Http`] on network failures, [`FetchError::Io`] on
    /// filesystem errors, or [`FetchError::Verify`] if a post-download retry also
    /// fails the BLAKE3 integrity check.
    pub fn fetch_entries(
        entries: &[OracleEntry],
        root: &Path,
        source: Option<&Path>,
        mut on_progress: impl FnMut(FetchProgress),
    ) -> Result<FetchSummary, FetchError> {
        let client = reqwest::blocking::Client::new();
        let count = entries.len();
        let mut summary = FetchSummary::default();
        for (i, entry) in entries.iter().enumerate() {
            let target = root.join(&entry.path);
            if matches!(oracle::verify_entry(root, entry).status, VerifyStatus::Ok) {
                summary.skipped.push(target);
                continue;
            }
            if let Some(src) = source
                && src != root
                && let Some(src_file) = locate_source_file(src, entry)
                && let Some(reflinked) = clone_entry(&src_file, root, entry, &target)?
            {
                if reflinked {
                    summary.reflinked.push(target);
                } else {
                    summary.copied.push(target);
                }
                continue;
            }
            download_entry(&client, entry, &target, i, count, &mut on_progress)?;
            summary.downloaded.push(target);
        }
        Ok(summary)
    }

    /// Locate the source file for `entry` under an existing mirror `src`.
    ///
    /// Routes through the common [`crate::locate_jpl_file`] locator: hoists `src`
    /// to the `ssd.jpl.nasa.gov/` mirror root when it points inside a real
    /// mirror, then walks down for a file whose base name matches the entry's —
    /// so a deep-point, differently-laid-out, or flat source mirror still yields
    /// a copy-on-write candidate. The located file's size + BLAKE3 are verified
    /// against the oracle record; returns the path only when it verifies `Ok`,
    /// otherwise `None` (the entry then falls through to a download).
    fn locate_source_file(src: &Path, entry: &OracleEntry) -> Option<PathBuf> {
        let base = entry.path.rsplit('/').next().unwrap_or(&entry.path);
        let candidate = crate::locate_jpl_file(src, base)?;
        matches!(oracle::verify_file(&candidate, entry), VerifyStatus::Ok).then_some(candidate)
    }

    /// Copy-on-write clone a single located source file (`src_path`) into `root`
    /// at the entry's canonical destination.
    ///
    /// Returns `Ok(Some(true))` when a true reflink succeeded, `Ok(Some(false))`
    /// when it fell back to a full copy, and `Ok(None)` when the clone landed but
    /// failed BLAKE3 verification (the stale destination is removed so the caller
    /// falls through to a network download).
    fn clone_entry(
        src_path: &Path,
        root: &Path,
        entry: &OracleEntry,
        target: &Path,
    ) -> Result<Option<bool>, FetchError> {
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(io(parent))?;
        }
        // reflink_or_copy requires the destination absent; clear any stale files.
        let _ = std::fs::remove_file(target);
        let _ = std::fs::remove_file(part_path(target));

        let reflinked = match reflink_copy::reflink_or_copy(src_path, target) {
            Ok(None) => true,
            Ok(Some(_)) => false,
            Err(source) => {
                return Err(FetchError::Io {
                    path: target.to_path_buf(),
                    source,
                });
            }
        };

        if matches!(oracle::verify_entry(root, entry).status, VerifyStatus::Ok) {
            Ok(Some(reflinked))
        } else {
            let _ = std::fs::remove_file(target);
            Ok(None)
        }
    }

    /// Fetch all of a dataset's entries into `root`, optionally cloning from an
    /// existing `source` mirror before hitting the network.
    ///
    /// # Errors
    /// Delegates to [`fetch_entries`]; see its error documentation.
    pub fn fetch_dataset(
        dataset: &Dataset,
        root: &Path,
        source: Option<&Path>,
        on_progress: impl FnMut(FetchProgress),
    ) -> Result<FetchSummary, FetchError> {
        fetch_entries(&dataset.entries(), root, source, on_progress)
    }

    fn download_entry(
        client: &reqwest::blocking::Client,
        entry: &OracleEntry,
        target: &Path,
        file_index: usize,
        file_count: usize,
        on_progress: &mut impl FnMut(FetchProgress),
    ) -> Result<(), FetchError> {
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(io(parent))?;
        }
        // Resume-capable first attempt; on a checksum failure retry once fresh.
        match stream_to_part(
            client,
            entry,
            target,
            file_index,
            file_count,
            on_progress,
            true,
        ) {
            Err(FetchError::Verify { .. }) => {
                let _ = std::fs::remove_file(part_path(target));
                stream_to_part(
                    client,
                    entry,
                    target,
                    file_index,
                    file_count,
                    on_progress,
                    false,
                )
            }
            other => other,
        }
    }

    fn stream_to_part(
        client: &reqwest::blocking::Client,
        entry: &OracleEntry,
        target: &Path,
        file_index: usize,
        file_count: usize,
        on_progress: &mut impl FnMut(FetchProgress),
        allow_resume: bool,
    ) -> Result<(), FetchError> {
        use reqwest::header::RANGE;
        let part = part_path(target);
        let url = entry_url(entry);
        let file_name = entry
            .path
            .rsplit('/')
            .next()
            .unwrap_or(&entry.path)
            .to_string();

        let existing = if allow_resume {
            std::fs::metadata(&part).map_or(0, |m| m.len())
        } else {
            0
        };

        let mut req = client.get(&url);
        if existing > 0 {
            req = req.header(RANGE, format!("bytes={existing}-"));
        }
        let mut resp = req
            .send()
            .and_then(reqwest::blocking::Response::error_for_status)
            .map_err(|source| FetchError::Http {
                url: url.clone(),
                source,
            })?;

        let resumed = existing > 0 && resp.status() == reqwest::StatusCode::PARTIAL_CONTENT;
        let mut done = if resumed { existing } else { 0 };
        let mut file = if resumed {
            std::fs::OpenOptions::new().append(true).open(&part)
        } else {
            std::fs::File::create(&part)
        }
        .map_err(io(&part))?;

        let mut buf = vec![0u8; 65536].into_boxed_slice();
        loop {
            let n = resp.read(&mut buf).map_err(io(&part))?;
            if n == 0 {
                break;
            }
            file.write_all(&buf[..n]).map_err(io(&part))?;
            done += n as u64;
            on_progress(FetchProgress {
                file_index,
                file_count,
                file_name: file_name.clone(),
                bytes_done: done,
                bytes_total: entry.size,
            });
        }
        file.sync_all().map_err(io(&part))?;
        drop(file);

        let actual = oracle::hash_file(&part).map_err(|_| FetchError::Io {
            path: part.clone(),
            source: std::io::Error::other("hashing the downloaded file failed"),
        })?;
        if actual != entry.blake3_hex {
            return Err(FetchError::Verify {
                path: target.to_path_buf(),
                expected: entry.blake3_hex,
                actual,
            });
        }
        std::fs::rename(&part, target).map_err(io(target))?;
        Ok(())
    }
}

#[cfg(feature = "data-fetch")]
pub use fetch::{
    FetchError, FetchProgress, FetchSummary, entry_url, fetch_dataset, fetch_entries, part_path,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(feature = "data-dir")]
    fn default_data_dir_is_named_starcat() {
        let dir = default_data_dir().expect("a platform data dir");
        assert_eq!(dir.file_name().unwrap(), "starcat");
    }

    #[test]
    fn registry_has_de441_and_rejects_unknown() {
        assert!(dataset_from_slug("de441").is_some());
        assert!(dataset_from_slug("nope").is_none());
        assert_eq!(datasets().len(), 1);
    }

    #[test]
    fn de441_entries_are_the_production_subset() {
        let ds = dataset_from_slug("de441").unwrap();
        assert_eq!(ds.entries(), oracle::production_entries());
        assert!(!ds.entries().is_empty());
    }

    #[test]
    #[cfg(feature = "data-fetch")]
    fn entry_url_is_https_over_path() {
        let e = OracleEntry {
            path: "ssd.jpl.nasa.gov/ftp/eph/planets/Linux/de441/header.441".into(),
            size: 22802,
            blake3_hex: "00",
        };
        assert_eq!(
            super::entry_url(&e),
            "https://ssd.jpl.nasa.gov/ftp/eph/planets/Linux/de441/header.441"
        );
    }

    #[test]
    #[cfg(feature = "data-fetch")]
    fn part_path_appends_dot_part() {
        let p = std::path::Path::new("/data/x/header.441");
        assert_eq!(
            super::part_path(p),
            std::path::PathBuf::from("/data/x/header.441.part")
        );
    }

    /// BLAKE3 of `b"cow bytes\n"` — used by the CoW-source tests below.
    #[cfg(feature = "data-fetch")]
    const COW_BYTES: &[u8] = b"cow bytes\n";

    /// Build a leaked `&'static str` of the BLAKE3 of `bytes`, so a synthetic
    /// [`OracleEntry`] can carry a real hash without a compile-time constant.
    #[cfg(feature = "data-fetch")]
    fn leak_hash(bytes: &[u8]) -> &'static str {
        let mut hasher = blake3::Hasher::new();
        hasher.update(bytes);
        Box::leak(hasher.finalize().to_hex().to_string().into_boxed_str())
    }

    /// Write `bytes` at `root.join(rel_path)`, creating parent dirs.
    #[cfg(feature = "data-fetch")]
    fn place(root: &std::path::Path, rel_path: &str, bytes: &[u8]) {
        use std::io::Write;
        let full = root.join(rel_path);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::File::create(&full)
            .unwrap()
            .write_all(bytes)
            .unwrap();
    }

    #[test]
    #[cfg(feature = "data-fetch")]
    fn fetch_entries_skips_when_already_valid_in_root() {
        let rel = "ssd.jpl.nasa.gov/ftp/eph/x/cow.bin";
        let entry = OracleEntry {
            path: rel.into(),
            size: COW_BYTES.len() as u64,
            blake3_hex: leak_hash(COW_BYTES),
        };
        let dst = tempdir::TempDir::new("fetch-skip-dst").unwrap();
        let src = tempdir::TempDir::new("fetch-skip-src").unwrap();
        // Already present + valid in the destination root.
        place(dst.path(), rel, COW_BYTES);

        let summary = super::fetch_entries(
            std::slice::from_ref(&entry),
            dst.path(),
            Some(src.path()),
            |_| {},
        )
        .expect("skip fetch");

        assert_eq!(summary.skipped, vec![dst.path().join(rel)]);
        assert!(summary.downloaded.is_empty());
        assert!(summary.reflinked.is_empty());
        assert!(summary.copied.is_empty());
        // The source was never touched (nothing was ever written to it).
        assert!(!src.path().join(rel).exists());
    }

    #[test]
    #[cfg(feature = "data-fetch")]
    fn fetch_entries_clones_from_source_without_network() {
        let rel = "ssd.jpl.nasa.gov/ftp/eph/x/cow.bin";
        let entry = OracleEntry {
            path: rel.into(),
            size: COW_BYTES.len() as u64,
            blake3_hex: leak_hash(COW_BYTES),
        };
        let dst = tempdir::TempDir::new("fetch-cow-dst").unwrap();
        let src = tempdir::TempDir::new("fetch-cow-src").unwrap();
        // Destination empty; valid file present at the source mirror.
        place(src.path(), rel, COW_BYTES);

        let summary = super::fetch_entries(
            std::slice::from_ref(&entry),
            dst.path(),
            Some(src.path()),
            |_| {},
        )
        .expect("cow fetch");

        // Landed in the destination and verifies Ok.
        let landed = dst.path().join(rel);
        assert!(landed.exists());
        assert_eq!(std::fs::read(&landed).unwrap(), COW_BYTES);
        assert!(matches!(
            oracle::verify_entry(dst.path(), &entry).status,
            oracle::VerifyStatus::Ok
        ));
        // It came from the source path (reflink OR copy — filesystem-dependent).
        let in_reflinked = summary.reflinked.contains(&landed);
        let in_copied = summary.copied.contains(&landed);
        assert!(
            in_reflinked ^ in_copied,
            "landed file must be reported in exactly one of reflinked/copied"
        );
        assert!(summary.downloaded.is_empty());
        assert!(summary.skipped.is_empty());
        // The source file is untouched (clone only, never moved/deleted).
        assert!(src.path().join(rel).exists());
        assert_eq!(std::fs::read(src.path().join(rel)).unwrap(), COW_BYTES);
    }

    #[test]
    #[cfg(feature = "data-fetch")]
    fn fetch_entries_clones_from_flat_source_layout() {
        // Source mirror is laid out FLAT: the file sits directly in the source
        // dir, not under the canonical ssd.jpl.nasa.gov/... path. The walker
        // must still find it by base name and CoW-clone it to the canonical
        // destination path in `root`.
        let rel = "ssd.jpl.nasa.gov/ftp/eph/x/cow.bin";
        let base = "cow.bin";
        let entry = OracleEntry {
            path: rel.into(),
            size: COW_BYTES.len() as u64,
            blake3_hex: leak_hash(COW_BYTES),
        };
        let dst = tempdir::TempDir::new("fetch-flatcow-dst").unwrap();
        let src = tempdir::TempDir::new("fetch-flatcow-src").unwrap();
        // Flat: base name directly in the source root (no mirror subtree).
        place(src.path(), base, COW_BYTES);

        let summary = super::fetch_entries(
            std::slice::from_ref(&entry),
            dst.path(),
            Some(src.path()),
            |_| {},
        )
        .expect("flat cow fetch");

        // Landed at the CANONICAL destination path and verifies Ok.
        let landed = dst.path().join(rel);
        assert!(landed.exists());
        assert_eq!(std::fs::read(&landed).unwrap(), COW_BYTES);
        assert!(matches!(
            oracle::verify_entry(dst.path(), &entry).status,
            oracle::VerifyStatus::Ok
        ));
        let in_reflinked = summary.reflinked.contains(&landed);
        let in_copied = summary.copied.contains(&landed);
        assert!(
            in_reflinked ^ in_copied,
            "landed file must be reported in exactly one of reflinked/copied"
        );
        assert!(summary.downloaded.is_empty());
        assert!(summary.skipped.is_empty());
        // The flat source file is untouched.
        assert!(src.path().join(base).exists());
    }

    #[test]
    #[cfg(feature = "data-fetch")]
    fn fetch_entries_clones_from_deep_point_in_full_source_mirror() {
        // The `source` points DEEP inside a full mirror (a sibling branch to
        // where the file lives). The mirror-root hoist in locate_source_file
        // must walk up, then find the file in its own branch and CoW-clone it.
        let rel = "ssd.jpl.nasa.gov/ftp/eph/small_bodies/asteroids_de441/cow.bin";
        let entry = OracleEntry {
            path: rel.into(),
            size: COW_BYTES.len() as u64,
            blake3_hex: leak_hash(COW_BYTES),
        };
        let dst = tempdir::TempDir::new("fetch-deepcow-dst").unwrap();
        let src = tempdir::TempDir::new("fetch-deepcow-src").unwrap();
        // Full mirror at the source; file in the small_bodies branch.
        place(src.path(), rel, COW_BYTES);
        // Point `source` at a DEEP sibling branch (planets/Linux/de441).
        let deep = src
            .path()
            .join("ssd.jpl.nasa.gov/ftp/eph/planets/Linux/de441");
        std::fs::create_dir_all(&deep).unwrap();

        let summary = super::fetch_entries(
            std::slice::from_ref(&entry),
            dst.path(),
            Some(deep.as_path()),
            |_| {},
        )
        .expect("deep-point cow fetch");

        let landed = dst.path().join(rel);
        assert!(landed.exists());
        assert_eq!(std::fs::read(&landed).unwrap(), COW_BYTES);
        assert!(matches!(
            oracle::verify_entry(dst.path(), &entry).status,
            oracle::VerifyStatus::Ok
        ));
        let in_reflinked = summary.reflinked.contains(&landed);
        let in_copied = summary.copied.contains(&landed);
        assert!(
            in_reflinked ^ in_copied,
            "landed file must be reported in exactly one of reflinked/copied"
        );
        assert!(summary.downloaded.is_empty());
        assert!(summary.skipped.is_empty());
    }

    #[test]
    #[cfg(feature = "data-fetch")]
    fn reflink_or_copy_yields_byte_identical_content() {
        let tmp = tempdir::TempDir::new("reflink-sanity").unwrap();
        let from = tmp.path().join("from.bin");
        let to = tmp.path().join("to.bin");
        std::fs::write(&from, COW_BYTES).unwrap();
        // Ok(None) => true reflink; Ok(Some(_)) => full-copy fallback.
        let outcome = reflink_copy::reflink_or_copy(&from, &to).expect("reflink_or_copy");
        assert!(matches!(outcome, None | Some(_)));
        assert_eq!(std::fs::read(&to).unwrap(), COW_BYTES);
    }

    #[test]
    #[cfg(feature = "data-fetch")]
    fn live_fetch_small_body_verifies_and_is_idempotent() {
        if std::env::var("STARCAT_FETCH_LIVE").is_err() {
            eprintln!("skip: set STARCAT_FETCH_LIVE=1 to run the live fetch test");
            return;
        }
        let entries: Vec<_> = oracle::production_entries()
            .into_iter()
            .filter(|e| e.path.ends_with("sb441-n16.bsp"))
            .collect();
        assert_eq!(entries.len(), 1, "expected exactly one sb441-n16.bsp entry");

        let tmp = tempdir::TempDir::new("fetch").unwrap();
        let root = tmp.path();

        let s1 = super::fetch_entries(&entries, root, None, |_| {}).expect("first fetch");
        assert_eq!(s1.downloaded.len(), 1);
        assert_eq!(s1.skipped.len(), 0);
        // Verified on land: the file matches the oracle.
        assert!(matches!(
            oracle::verify_entry(root, &entries[0]).status,
            oracle::VerifyStatus::Ok
        ));

        // Second run skips (idempotent).
        let s2 = super::fetch_entries(&entries, root, None, |_| {}).expect("second fetch");
        assert_eq!(s2.downloaded.len(), 0);
        assert_eq!(s2.skipped.len(), 1);
    }
}
