//! Home-dir ephemeris store: default data directory, named datasets, and a
//! resumable, self-verifying fetcher for the JPL production subset.

use crate::jpl::oracle::{self, OracleEntry};
use std::sync::OnceLock;

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
    /// The oracle entries this dataset comprises — the entourage's default set
    /// (the DE integration plus its perturber SPKs). URLs + hashes come from the
    /// oracle. The heavier `optional` extras are excluded here; a caller who
    /// wants them uses [`oracle::entourage_entries`] with `include_optional`.
    #[must_use]
    pub fn entries(&self) -> Vec<OracleEntry> {
        oracle::entourage_entries(self.slug, false).unwrap_or_default()
    }
}

/// Rough human byte size (binary GB/MB) for a dataset description.
#[allow(clippy::cast_precision_loss)] // display-only; sub-byte precision irrelevant
fn human_bytes(n: u64) -> String {
    const GB: f64 = 1_073_741_824.0;
    const MB: f64 = 1_048_576.0;
    let f = n as f64;
    if f >= GB {
        format!("{:.1} GB", f / GB)
    } else {
        format!("{:.0} MB", f / MB)
    }
}

/// Every fetchable dataset — one per oracle entourage (a DE integration and its
/// perturber set). Derived from [`oracle::entourages`], so the committed JSON
/// manifest is the single source of truth. `de441` is the default; `de431` and
/// the older DE integrations are selectable by slug.
#[must_use]
pub fn datasets() -> &'static [Dataset] {
    static DATASETS: OnceLock<Vec<Dataset>> = OnceLock::new();
    DATASETS
        .get_or_init(|| {
            oracle::entourages()
                .iter()
                .map(|e| {
                    let bytes: u64 = oracle::entourage_entries(e.slug, false)
                        .map_or(0, |v| v.iter().map(|x| x.size).sum());
                    let description: &'static str = Box::leak(
                        format!(
                            "{} integration + perturbers (~{})",
                            e.label,
                            human_bytes(bytes)
                        )
                        .into_boxed_str(),
                    );
                    Dataset {
                        slug: e.slug,
                        description,
                    }
                })
                .collect()
        })
        .as_slice()
}

/// Look up a dataset by slug.
#[must_use]
pub fn dataset_from_slug(slug: &str) -> Option<&'static Dataset> {
    datasets().iter().find(|d| d.slug == slug)
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

    // --- data migrate: cherry-pick usable files from a source into the data dir

    /// How `data migrate` relocates each usable file it finds in the source.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum MigrateMode {
        /// Copy into the data dir — a copy-on-write clone where the filesystem
        /// supports it (zero extra disk), otherwise a full byte copy. The source
        /// is left in place.
        Copy,
        /// Move into the data dir — rename where possible, else copy then delete.
        /// The source file is removed once the destination verifies.
        Move,
    }

    /// One usable file located in the source, ready to migrate to `entry`'s
    /// canonical path under the data dir.
    #[derive(Debug, Clone)]
    pub struct MigrateItem {
        /// The oracle entry (canonical path + size + hash) this file satisfies.
        pub entry: OracleEntry,
        /// Where the matching file was found in the source tree.
        pub source_path: PathBuf,
    }

    /// The result of scanning a source for usable files — computed WITHOUT
    /// modifying anything, so the CLI can report and prompt before acting.
    #[derive(Debug, Default)]
    pub struct MigratePlan {
        /// Verified usable files to migrate (base-name match + BLAKE3 `Ok`).
        pub migrate: Vec<MigrateItem>,
        /// Entries already present + valid in the data dir (nothing to do).
        pub skipped: Vec<PathBuf>,
        /// Files whose base name matched a usable entry but which FAILED the
        /// size/hash check — e.g. a truncated download. Reported, never moved.
        pub corrupt: Vec<PathBuf>,
    }

    impl MigratePlan {
        /// Total bytes across the files queued to migrate.
        #[must_use]
        pub fn total_bytes(&self) -> u64 {
            self.migrate.iter().map(|i| i.entry.size).sum()
        }
    }

    /// What one migrated file became.
    #[derive(Debug, Default)]
    pub struct MigrateSummary {
        /// Copy mode: copy-on-write clones (no additional disk used).
        pub reflinked: Vec<PathBuf>,
        /// Copy mode: full byte copies (off-CoW fallback).
        pub copied: Vec<PathBuf>,
        /// Move mode: relocated files (source removed).
        pub moved: Vec<PathBuf>,
        /// Files that landed but failed post-migration verification (removed).
        pub failed: Vec<PathBuf>,
    }

    /// Scan `source` for every usable file in `entries`, WITHOUT modifying
    /// anything. For each entry: already valid under `root` -> skipped; a source
    /// file whose base name matches AND whose bytes verify -> queued to migrate;
    /// a same-named file that fails size/hash (with none that verifies) ->
    /// flagged corrupt; no name match at all -> ignored (a given source rarely
    /// holds every series).
    ///
    /// Location is **content-verified**, not merely name-based: several usable
    /// files share a base name with a byte-different file in another layout (a
    /// DE-series `header.NNN` exists identically under `Linux/`, `ascii/`, and
    /// `SunOS/` — but for DE406 and DE421 the `ascii/` copy differs by a byte).
    /// The scan therefore walks past a wrong-content twin and keeps looking, so
    /// the correct file is found regardless of traversal order, and a genuinely
    /// truncated download is still reported rather than silently accepted.
    #[must_use]
    pub fn migrate_scan(entries: &[OracleEntry], source: &Path, root: &Path) -> MigratePlan {
        let mut plan = MigratePlan::default();
        for entry in entries {
            if matches!(oracle::verify_entry(root, entry).status, VerifyStatus::Ok) {
                plan.skipped.push(root.join(&entry.path));
                continue;
            }
            let base = entry.path.rsplit('/').next().unwrap_or(&entry.path);
            // Prefer a source file that matches this entry by name AND content.
            let verified = crate::locate_jpl_file_accepting(source, |p| {
                p.file_name().and_then(|n| n.to_str()) == Some(base)
                    && matches!(oracle::verify_file(p, entry), VerifyStatus::Ok)
            });
            if let Some(found) = verified {
                plan.migrate.push(MigrateItem {
                    entry: entry.clone(),
                    source_path: found,
                });
            } else if let Some(any) = crate::locate_jpl_file(source, base) {
                // A same-named file exists but nothing verified — e.g. a
                // truncated download. Report it; never migrate it.
                plan.corrupt.push(any);
            }
        }
        plan
    }

    /// Best-effort probe of whether copy-on-write cloning works from the source
    /// filesystem into `root`: reflinks `sample` (an existing source file) to a
    /// throwaway in `root`. `true` only on a genuine reflink. Never writes to the
    /// source, and always removes its probe file.
    #[must_use]
    pub fn probe_cow(sample: &Path, root: &Path) -> bool {
        let _ = std::fs::create_dir_all(root);
        let probe = root.join(".starcat-cow-probe");
        let _ = std::fs::remove_file(&probe);
        let is_reflink = matches!(reflink_copy::reflink_or_copy(sample, &probe), Ok(None));
        let _ = std::fs::remove_file(&probe);
        is_reflink
    }

    /// Apply a [`MigratePlan`] with the chosen [`MigrateMode`], returning what
    /// each file became. `on_item(index, total, item)` fires as each file is
    /// about to be processed (for progress reporting).
    ///
    /// # Errors
    /// Returns [`FetchError::Io`] on a filesystem error during copy/move/clone.
    pub fn migrate_apply(
        plan: &MigratePlan,
        root: &Path,
        mode: MigrateMode,
        mut on_item: impl FnMut(usize, usize, &MigrateItem),
    ) -> Result<MigrateSummary, FetchError> {
        let total = plan.migrate.len();
        let mut summary = MigrateSummary::default();
        for (i, item) in plan.migrate.iter().enumerate() {
            on_item(i, total, item);
            let target = root.join(&item.entry.path);
            match mode {
                MigrateMode::Copy => {
                    match clone_entry(&item.source_path, root, &item.entry, &target)? {
                        Some(true) => summary.reflinked.push(target),
                        Some(false) => summary.copied.push(target),
                        None => summary.failed.push(target),
                    }
                }
                MigrateMode::Move => {
                    if move_entry(&item.source_path, root, &item.entry, &target)? {
                        summary.moved.push(target);
                    } else {
                        summary.failed.push(target);
                    }
                }
            }
        }
        Ok(summary)
    }

    /// Move a source file to `target`, verifying the landed file. Rename first
    /// (instant within a filesystem); on a cross-filesystem rename error, fall
    /// back to copy + remove-source. Returns whether the landed file verified.
    fn move_entry(
        src: &Path,
        root: &Path,
        entry: &OracleEntry,
        target: &Path,
    ) -> Result<bool, FetchError> {
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(io(parent))?;
        }
        let _ = std::fs::remove_file(target);
        let _ = std::fs::remove_file(part_path(target));
        if std::fs::rename(src, target).is_err() {
            // Cross-filesystem: rename is not permitted, so copy then delete.
            std::fs::copy(src, target).map_err(io(target))?;
            std::fs::remove_file(src).map_err(io(src))?;
        }
        if matches!(oracle::verify_entry(root, entry).status, VerifyStatus::Ok) {
            Ok(true)
        } else {
            let _ = std::fs::remove_file(target);
            Ok(false)
        }
    }

    // --- data migrate: Horizons per-body SPKs (no oracle hash; validated by
    // opening as an SPK) from a source Horizons dir into the platform one.

    /// What placing one file became.
    enum Placed {
        Reflinked,
        Copied,
        Moved,
    }

    /// Relocate one file into `dest` by [`MigrateMode`], with no verification
    /// (the caller validates). Copy = copy-on-write clone (else full copy); move
    /// = rename (instant within a filesystem), else copy + delete-source.
    fn place_file(src: &Path, dest: &Path, mode: MigrateMode) -> Result<Placed, FetchError> {
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(io(parent))?;
        }
        let _ = std::fs::remove_file(dest);
        match mode {
            MigrateMode::Copy => match reflink_copy::reflink_or_copy(src, dest) {
                Ok(None) => Ok(Placed::Reflinked),
                Ok(Some(_)) => Ok(Placed::Copied),
                Err(source) => Err(FetchError::Io {
                    path: dest.to_path_buf(),
                    source,
                }),
            },
            MigrateMode::Move => {
                if std::fs::rename(src, dest).is_err() {
                    std::fs::copy(src, dest).map_err(io(dest))?;
                    std::fs::remove_file(src).map_err(io(src))?;
                }
                Ok(Placed::Moved)
            }
        }
    }

    /// One Horizons SPK located in the source, to migrate to the Horizons dir
    /// under its own base name (`<naif>.bsp`).
    #[derive(Debug, Clone)]
    pub struct HorizonsMigrateItem {
        /// Where the `.bsp` was found in the source.
        pub source_path: PathBuf,
        /// Its size in bytes.
        pub size: u64,
    }

    /// The result of scanning a source Horizons dir — computed WITHOUT modifying
    /// anything, so the CLI can report and prompt before acting.
    #[derive(Debug, Default)]
    pub struct HorizonsMigratePlan {
        /// Valid `.bsp` SPKs to migrate.
        pub migrate: Vec<HorizonsMigrateItem>,
        /// Already present + valid in the destination Horizons dir.
        pub skipped: Vec<PathBuf>,
        /// Present in the source but not a valid SPK (e.g. a truncated download).
        pub invalid: Vec<PathBuf>,
    }

    impl HorizonsMigratePlan {
        /// Total bytes queued to migrate.
        #[must_use]
        pub fn total_bytes(&self) -> u64 {
            self.migrate.iter().map(|i| i.size).sum()
        }
    }

    /// What the Horizons migration did.
    #[derive(Debug, Default)]
    pub struct HorizonsMigrateSummary {
        /// Copy mode: copy-on-write clones (no additional disk).
        pub reflinked: Vec<PathBuf>,
        /// Copy mode: full byte copies.
        pub copied: Vec<PathBuf>,
        /// Move mode: relocated files.
        pub moved: Vec<PathBuf>,
        /// Landed but failed to open as an SPK afterward (removed).
        pub failed: Vec<PathBuf>,
    }

    /// Scan a source Horizons directory for per-body `.bsp` SPKs to bring into
    /// `dest` (the platform Horizons dir), WITHOUT modifying anything.
    ///
    /// The source is a curated Horizons dir — every `.bsp` in it is a prior
    /// `starcat horizons` pull — so all are candidates. Each is validated by
    /// opening it as an SPK (guarding a truncated download); one already present
    /// and valid in `dest` is skipped; one that fails to open is flagged invalid.
    #[must_use]
    pub fn horizons_migrate_scan(source: &Path, dest: &Path) -> HorizonsMigratePlan {
        let mut plan = HorizonsMigratePlan::default();
        for src_path in crate::spk::collect_bsp_paths(source) {
            let Some(name) = src_path.file_name() else {
                continue;
            };
            let dest_path = dest.join(name);
            if dest_path.is_file() && crate::spk::SpkEphemeris::open(&dest_path).is_ok() {
                plan.skipped.push(dest_path);
                continue;
            }
            if crate::spk::SpkEphemeris::open(&src_path).is_ok() {
                let size = std::fs::metadata(&src_path).map_or(0, |m| m.len());
                plan.migrate.push(HorizonsMigrateItem {
                    source_path: src_path,
                    size,
                });
            } else {
                plan.invalid.push(src_path);
            }
        }
        plan
    }

    /// Apply a [`HorizonsMigratePlan`] into `dest` by copy or move, re-validating
    /// each landed file opens as an SPK. `on_item(index, total, item)` fires as
    /// each file is processed.
    ///
    /// # Errors
    /// [`FetchError::Io`] on a filesystem error during copy/move.
    pub fn horizons_migrate_apply(
        plan: &HorizonsMigratePlan,
        dest: &Path,
        mode: MigrateMode,
        mut on_item: impl FnMut(usize, usize, &HorizonsMigrateItem),
    ) -> Result<HorizonsMigrateSummary, FetchError> {
        let total = plan.migrate.len();
        let mut summary = HorizonsMigrateSummary::default();
        for (i, item) in plan.migrate.iter().enumerate() {
            on_item(i, total, item);
            let name = item.source_path.file_name().unwrap_or_default();
            let target = dest.join(name);
            let placed = place_file(&item.source_path, &target, mode)?;
            if crate::spk::SpkEphemeris::open(&target).is_ok() {
                match placed {
                    Placed::Reflinked => summary.reflinked.push(target),
                    Placed::Copied => summary.copied.push(target),
                    Placed::Moved => summary.moved.push(target),
                }
            } else {
                let _ = std::fs::remove_file(&target);
                summary.failed.push(target);
            }
        }
        Ok(summary)
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
    FetchError, FetchProgress, FetchSummary, HorizonsMigrateItem, HorizonsMigratePlan,
    HorizonsMigrateSummary, MigrateItem, MigrateMode, MigratePlan, MigrateSummary, entry_url,
    fetch_dataset, fetch_entries, horizons_migrate_apply, horizons_migrate_scan, migrate_apply,
    migrate_scan, part_path, probe_cow,
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
    fn registry_has_de441_de431_and_rejects_unknown() {
        // The registry is derived from the oracle entourages (JSON manifest).
        assert!(dataset_from_slug("de441").is_some());
        assert!(dataset_from_slug("de431").is_some());
        assert!(dataset_from_slug("nope").is_none());
        assert_eq!(datasets().len(), oracle::entourages().len());
    }

    #[test]
    fn de441_dataset_is_the_entourage_default_superset_of_production() {
        let ds = dataset_from_slug("de441").unwrap();
        let entries = ds.entries();
        assert!(!entries.is_empty());
        // The DE integration binary and the headline small-body SPK are present.
        assert!(
            entries
                .iter()
                .any(|e| e.path.ends_with("linux_m13000p17000.441"))
        );
        assert!(entries.iter().any(|e| e.path.ends_with("sb441-n16.bsp")));
        // The entourage rolls in the dwarf-planet perturber so users don't end
        // up without the dwarves — it is a strict superset of the narrower
        // compute/verify `production_entries` subset.
        assert!(entries.iter().any(|e| e.path.ends_with("sb441-n373s.bsp")));
        for p in oracle::production_entries() {
            assert!(
                entries.iter().any(|e| e.path == p.path),
                "production entry {} missing from de441 entourage",
                p.path
            );
        }
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

    /// Synthetic oracle entry hashing to `bytes`, at `rel`.
    #[cfg(feature = "data-fetch")]
    fn synth_entry(rel: &str, bytes: &[u8]) -> OracleEntry {
        OracleEntry {
            path: rel.into(),
            size: bytes.len() as u64,
            blake3_hex: leak_hash(bytes),
        }
    }

    #[test]
    #[cfg(feature = "data-fetch")]
    fn migrate_scan_classifies_skip_migrate_corrupt_and_ignores_absent() {
        let src = tempdir::TempDir::new("mig-scan-src").unwrap();
        let dst = tempdir::TempDir::new("mig-scan-dst").unwrap();

        let skip = synth_entry("ssd.jpl.nasa.gov/ftp/eph/a/skip.bin", COW_BYTES);
        let good = synth_entry("ssd.jpl.nasa.gov/ftp/eph/b/good.bin", COW_BYTES);
        let bad = synth_entry("ssd.jpl.nasa.gov/ftp/eph/c/bad.bin", COW_BYTES);
        let absent = synth_entry("ssd.jpl.nasa.gov/ftp/eph/d/gone.bin", COW_BYTES);

        // `skip` already valid in the data dir; `good` valid in source; `bad`
        // present in source but truncated (wrong bytes); `absent` nowhere.
        place(dst.path(), &skip.path, COW_BYTES);
        place(src.path(), &good.path, COW_BYTES);
        place(src.path(), &bad.path, b"TRUNCATED");

        let entries = [skip.clone(), good.clone(), bad, absent];
        let plan = super::migrate_scan(&entries, src.path(), dst.path());

        assert_eq!(plan.skipped, vec![dst.path().join(&skip.path)]);
        assert_eq!(plan.migrate.len(), 1);
        assert_eq!(plan.migrate[0].entry.path, good.path);
        assert_eq!(
            plan.corrupt.len(),
            1,
            "truncated file flagged, not migrated"
        );
        assert_eq!(plan.total_bytes(), COW_BYTES.len() as u64);
    }

    #[test]
    #[cfg(feature = "data-fetch")]
    fn migrate_apply_copy_lands_valid_and_leaves_source() {
        let src = tempdir::TempDir::new("mig-copy-src").unwrap();
        let dst = tempdir::TempDir::new("mig-copy-dst").unwrap();
        let e = synth_entry("ssd.jpl.nasa.gov/ftp/eph/x/de441.bin", COW_BYTES);
        place(src.path(), &e.path, COW_BYTES);

        let plan = super::migrate_scan(std::slice::from_ref(&e), src.path(), dst.path());
        assert_eq!(plan.migrate.len(), 1);
        let summary =
            super::migrate_apply(&plan, dst.path(), super::MigrateMode::Copy, |_, _, _| {})
                .expect("copy migrate");

        // Landed at the canonical path and verifies; a reflink or a full copy.
        assert!(matches!(
            oracle::verify_entry(dst.path(), &e).status,
            oracle::VerifyStatus::Ok
        ));
        assert_eq!(summary.reflinked.len() + summary.copied.len(), 1);
        assert!(summary.moved.is_empty() && summary.failed.is_empty());
        // Copy leaves the source in place.
        assert!(src.path().join(&e.path).is_file());
    }

    #[test]
    #[cfg(feature = "data-fetch")]
    fn migrate_apply_move_relocates_and_removes_source() {
        let src = tempdir::TempDir::new("mig-move-src").unwrap();
        let dst = tempdir::TempDir::new("mig-move-dst").unwrap();
        let e = synth_entry("ssd.jpl.nasa.gov/ftp/eph/y/de431.bin", COW_BYTES);
        let src_file = src.path().join(&e.path);
        place(src.path(), &e.path, COW_BYTES);

        let plan = super::migrate_scan(std::slice::from_ref(&e), src.path(), dst.path());
        let summary =
            super::migrate_apply(&plan, dst.path(), super::MigrateMode::Move, |_, _, _| {})
                .expect("move migrate");

        assert!(matches!(
            oracle::verify_entry(dst.path(), &e).status,
            oracle::VerifyStatus::Ok
        ));
        assert_eq!(summary.moved.len(), 1);
        // Move removes the located source file.
        assert!(!src_file.exists(), "source should be gone after a move");
    }

    #[test]
    #[cfg(feature = "data-fetch")]
    fn migrate_scan_walks_past_wrong_content_twin_to_the_correct_file() {
        // Two files share the base name `header.406` (as the real Linux/ascii
        // twins do) but differ in content. The wrong one sorts first ("a_" <
        // "z_"), so a name-only locate would hit it, fail verify, and give up.
        // Content-verified location must skip it and find the correct file.
        let src = tempdir::TempDir::new("mig-twin-src").unwrap();
        let dst = tempdir::TempDir::new("mig-twin-dst").unwrap();
        let e = synth_entry(
            "ssd.jpl.nasa.gov/ftp/eph/planets/Linux/de406/header.406",
            COW_BYTES,
        );
        place(
            src.path(),
            "a_wrong/header.406",
            b"a different header entirely",
        );
        place(src.path(), "z_right/header.406", COW_BYTES);

        let plan = super::migrate_scan(std::slice::from_ref(&e), src.path(), dst.path());
        assert_eq!(plan.migrate.len(), 1, "must find the content-matching twin");
        assert!(
            plan.migrate[0].source_path.ends_with("z_right/header.406"),
            "expected the correct file, got {:?}",
            plan.migrate[0].source_path
        );
        assert!(plan.corrupt.is_empty(), "a valid file was present");
    }

    #[test]
    #[cfg(feature = "data-fetch")]
    fn migrate_scan_flags_corrupt_only_when_no_twin_verifies() {
        // Only a wrong-content same-named file exists: nothing verifies, so the
        // entry is reported corrupt (a truncated download), never migrated.
        let src = tempdir::TempDir::new("mig-onlybad-src").unwrap();
        let dst = tempdir::TempDir::new("mig-onlybad-dst").unwrap();
        let e = synth_entry(
            "ssd.jpl.nasa.gov/ftp/eph/planets/Linux/de406/header.406",
            COW_BYTES,
        );
        place(src.path(), "somewhere/header.406", b"truncated");

        let plan = super::migrate_scan(std::slice::from_ref(&e), src.path(), dst.path());
        assert!(plan.migrate.is_empty());
        assert_eq!(plan.corrupt.len(), 1);
    }

    #[test]
    #[cfg(feature = "data-fetch")]
    fn probe_cow_leaves_no_residue() {
        let src = tempdir::TempDir::new("probe-src").unwrap();
        let dst = tempdir::TempDir::new("probe-dst").unwrap();
        let sample = src.path().join("sample.bin");
        std::fs::write(&sample, COW_BYTES).unwrap();
        // Result is filesystem-dependent (true only on CoW); we only assert the
        // probe cleans up after itself regardless of outcome.
        let _ = super::probe_cow(&sample, dst.path());
        assert!(!dst.path().join(".starcat-cow-probe").exists());
    }

    /// Write a minimal valid DAF/SPK (file record + empty summary record) at
    /// `dir/name`. Opens as an SPK; carries no segments.
    #[cfg(feature = "data-fetch")]
    fn write_spk(dir: &std::path::Path, name: &str) {
        use std::io::Write;
        let mut file_rec = [0u8; 1024];
        file_rec[0..8].copy_from_slice(b"DAF/SPK ");
        file_rec[8..12].copy_from_slice(&2i32.to_le_bytes());
        file_rec[12..16].copy_from_slice(&6i32.to_le_bytes());
        file_rec[76..80].copy_from_slice(&2i32.to_le_bytes());
        file_rec[88..96].copy_from_slice(b"LTL-IEEE");
        let sum_rec = [0u8; 1024];
        std::fs::create_dir_all(dir).unwrap();
        let mut f = std::fs::File::create(dir.join(name)).unwrap();
        f.write_all(&file_rec).unwrap();
        f.write_all(&sum_rec).unwrap();
    }

    #[test]
    #[cfg(feature = "data-fetch")]
    fn horizons_migrate_scan_classifies_migrate_skip_and_invalid() {
        let src = tempdir::TempDir::new("hz-scan-src").unwrap();
        let dst = tempdir::TempDir::new("hz-scan-dst").unwrap();
        // Source: a new valid SPK, one already present in dest, one truncated.
        write_spk(src.path(), "2060.bsp"); // Chiron — new
        write_spk(src.path(), "2000001.bsp"); // already in dest
        std::fs::write(src.path().join("bad.bsp"), b"NOPE").unwrap(); // truncated
        write_spk(dst.path(), "2000001.bsp"); // present + valid in dest

        let plan = super::horizons_migrate_scan(src.path(), dst.path());
        assert_eq!(plan.migrate.len(), 1);
        assert!(plan.migrate[0].source_path.ends_with("2060.bsp"));
        assert_eq!(plan.skipped.len(), 1, "2000001 already valid in dest");
        assert_eq!(plan.invalid.len(), 1, "truncated bad.bsp flagged");
    }

    #[test]
    #[cfg(feature = "data-fetch")]
    fn horizons_migrate_apply_copy_lands_valid_and_leaves_source() {
        let src = tempdir::TempDir::new("hz-copy-src").unwrap();
        let dst = tempdir::TempDir::new("hz-copy-dst").unwrap();
        write_spk(src.path(), "2060.bsp");
        let plan = super::horizons_migrate_scan(src.path(), dst.path());
        let summary = super::horizons_migrate_apply(
            &plan,
            dst.path(),
            super::MigrateMode::Copy,
            |_, _, _| {},
        )
        .expect("copy");
        assert!(dst.path().join("2060.bsp").is_file());
        assert!(crate::spk::SpkEphemeris::open(dst.path().join("2060.bsp")).is_ok());
        assert_eq!(summary.reflinked.len() + summary.copied.len(), 1);
        assert!(summary.failed.is_empty());
        assert!(src.path().join("2060.bsp").is_file(), "copy leaves source");
    }

    #[test]
    #[cfg(feature = "data-fetch")]
    fn horizons_migrate_apply_move_relocates_and_removes_source() {
        let src = tempdir::TempDir::new("hz-move-src").unwrap();
        let dst = tempdir::TempDir::new("hz-move-dst").unwrap();
        write_spk(src.path(), "2060.bsp");
        let plan = super::horizons_migrate_scan(src.path(), dst.path());
        let summary = super::horizons_migrate_apply(
            &plan,
            dst.path(),
            super::MigrateMode::Move,
            |_, _, _| {},
        )
        .expect("move");
        assert!(crate::spk::SpkEphemeris::open(dst.path().join("2060.bsp")).is_ok());
        assert_eq!(summary.moved.len(), 1);
        assert!(!src.path().join("2060.bsp").exists(), "move removes source");
    }
}
