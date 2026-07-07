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

    /// Download `entries` into `root`, skipping any already present + hash-valid.
    ///
    /// # Errors
    /// Returns [`FetchError::Http`] on network failures, [`FetchError::Io`] on
    /// filesystem errors, or [`FetchError::Verify`] if a post-download retry also
    /// fails the BLAKE3 integrity check.
    pub fn fetch_entries(
        entries: &[OracleEntry],
        root: &Path,
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
            download_entry(&client, entry, &target, i, count, &mut on_progress)?;
            summary.downloaded.push(target);
        }
        Ok(summary)
    }

    /// Download all of a dataset's entries into `root`.
    ///
    /// # Errors
    /// Delegates to [`fetch_entries`]; see its error documentation.
    pub fn fetch_dataset(
        dataset: &Dataset,
        root: &Path,
        on_progress: impl FnMut(FetchProgress),
    ) -> Result<FetchSummary, FetchError> {
        fetch_entries(&dataset.entries(), root, on_progress)
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

        let s1 = super::fetch_entries(&entries, root, |_| {}).expect("first fetch");
        assert_eq!(s1.downloaded.len(), 1);
        assert_eq!(s1.skipped.len(), 0);
        // Verified on land: the file matches the oracle.
        assert!(matches!(
            oracle::verify_entry(root, &entries[0]).status,
            oracle::VerifyStatus::Ok
        ));

        // Second run skips (idempotent).
        let s2 = super::fetch_entries(&entries, root, |_| {}).expect("second fetch");
        assert_eq!(s2.downloaded.len(), 0);
        assert_eq!(s2.skipped.len(), 1);
    }
}
