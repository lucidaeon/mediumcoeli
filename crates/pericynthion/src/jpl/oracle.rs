//! Hardcoded BLAKE3 oracle of the JPL `eph/` data mirror.
//!
//! This module is a *dataset oracle for posterity*: it records every file
//! we mirror under `ssd.jpl.nasa.gov/ftp/eph/{planets,satellites,small_bodies}/`,
//! its byte size, and its unkeyed BLAKE3 hash. It lets us verify a user's
//! local copy is bit-identical to the reference mirror, and detect silent
//! corruption or truncated downloads.

use crate::error::PericynthionError;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// The sentinel `provides` value meaning "all fixed stars" (used by `catalog.gz`).
pub const STAR_CLASS_ALL: &str = "@fixed-stars";

/// The committed oracle manifest: every mirrored file's identity plus the
/// entourage groupings. Hand-editable single source of truth — parsed once at
/// first use (see [`loaded`]); there is no codegen step.
const ORACLE_JSON: &str = include_str!("oracle.json");

/// Which upstream family a manifest directory belongs to. Determines URL
/// derivation and whether rows are integrity-pinned or presence-only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    /// A file in the JPL SSD eph mirror; URL = `https://{prefix}/{name}`; hash-pinned.
    JplMirror,
    /// A CDS `VizieR` catalog file; URL = `https://{prefix}/{name}`; hash-pinned.
    CdsCatalog,
    /// Per-body SPK from the JPL Horizons API; presence-only (synthesized, never stored here).
    HorizonsSpk,
}

/// One file's identity: integrity (`size` + `blake3_hex`) plus provenance
/// (`provides`/`coverage`), keyed by name within an [`OracleDir`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OracleFile {
    /// File name (no directory component).
    pub name: &'static str,
    /// Size in bytes.
    pub size: u64,
    /// Unkeyed BLAKE3 of the file's bytes, lowercase hex (64 chars).
    pub blake3_hex: &'static str,
    /// Catalogued body display names this file backs, or [`STAR_CLASS_ALL`].
    /// Empty for mirror files not tied to a catalogued body.
    pub provides: &'static [&'static str],
    /// Optional human coverage gloss, e.g. `"Yale BSC5P (Hoffleit & Warren 1991)"`.
    pub coverage: Option<&'static str>,
}

/// A directory of files sharing a common path prefix and [`SourceKind`].
#[derive(Debug, Clone, Copy)]
pub struct OracleDir {
    /// Path prefix relative to the mirror root / host-first URL path, e.g.
    /// `ssd.jpl.nasa.gov/ftp/eph/planets/Linux/de441`.
    pub prefix: &'static str,
    /// Which upstream family this directory belongs to.
    pub kind: SourceKind,
    /// Files directly in this directory.
    pub files: &'static [OracleFile],
}

/// A flattened row: full relative path + size + hash. Not `Copy` — owns a `String`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OracleEntry {
    /// Full path relative to the mirror root.
    pub path: String,
    /// Size in bytes.
    pub size: u64,
    /// Unkeyed BLAKE3, lowercase hex.
    pub blake3_hex: &'static str,
}

/// A named dataset grouping — a DE integration and its consistent perturber set
/// (the "entourage" that always travels with a given DE release). Drives
/// `starcat data fetch <slug>`: `planets` + `perturbers` are the default fetch;
/// `optional` are heavier extras (e.g. the full `sb441-n373`) the user can add.
///
/// Each field holds full `https://` URLs; resolve them to integrity-checked
/// [`OracleEntry`] values with [`entourage_entries`].
#[derive(Debug, Clone, Copy)]
pub struct Entourage {
    /// Slug used on the CLI and for [`entourage`] lookup, e.g. `"de441"`.
    pub slug: &'static str,
    /// Human label, e.g. `"DE441"`.
    pub label: &'static str,
    /// The DE integration itself: header + full-span binary.
    pub planets: &'static [&'static str],
    /// The asteroid-perturber SPK(s) that ship with this DE release.
    pub perturbers: &'static [&'static str],
    /// Heavier optional extras (fetched only when explicitly requested).
    pub optional: &'static [&'static str],
}

/// One row of the date-aware DE-selection preference: an entourage slug and the
/// civil-year window over which its planetary binary is valid. [`de_preference`]
/// returns these **sorted best-precision-first**, so a date-aware selector walks
/// the list and takes the first entry that both [`covers`](DePreference::covers)
/// the requested year and is present on disk.
///
/// Precision order (encoded by position): newer DE release generations first;
/// within a generation the standard-window integration before its long-range
/// sibling (e.g. `de440` before `de441` — the long-range file approximates
/// lunar tidal dissipation for 30-millennia stability, so it is marginally less
/// accurate in-window). The difference is astrologically negligible, but the
/// ordering picks the smaller, higher-fidelity file when it covers the date.
#[derive(Debug, Clone, Copy)]
pub struct DePreference {
    /// Entourage slug, e.g. `"de440"`. Always a valid [`entourage`] slug.
    pub slug: &'static str,
    /// First civil year the binary covers (negative for BCE).
    pub from_year: i32,
    /// Last civil year the binary covers.
    pub to_year: i32,
}

impl DePreference {
    /// True when `year` (civil, negative for BCE) falls within this window.
    #[must_use]
    pub fn covers(&self, year: i32) -> bool {
        self.from_year <= year && year <= self.to_year
    }
}

// --- JSON manifest (oracle.json) deserialization -------------------------------
//
// The committed `oracle.json` is a flat list of file objects plus a map of
// entourage objects. It is parsed once and leaked to `'static` (the oracle
// lives for the whole process, exactly as the former compiled-in table did), so
// the public API keeps returning `&'static` references with zero churn.

#[derive(Deserialize)]
struct RawOracle {
    files: Vec<RawFile>,
    entourages: std::collections::BTreeMap<String, RawEntourage>,
    #[serde(default)]
    de_preference: Vec<RawDePref>,
}

#[derive(Deserialize)]
struct RawDePref {
    slug: String,
    from_year: i32,
    to_year: i32,
    // `released` / `window` are human documentation in the JSON; ignored here.
}

#[derive(Deserialize)]
struct RawFile {
    url: String,
    size: u64,
    blake3: String,
    #[serde(default)]
    provides: Vec<String>,
    #[serde(default)]
    coverage: Option<String>,
}

#[derive(Deserialize)]
struct RawEntourage {
    label: String,
    #[serde(default)]
    planets: Vec<String>,
    #[serde(default)]
    perturbers: Vec<String>,
    #[serde(default)]
    optional: Vec<String>,
}

/// Everything parsed from `oracle.json`, leaked to `'static`.
struct Loaded {
    dirs: &'static [OracleDir],
    entourages: &'static [Entourage],
    de_preference: &'static [DePreference],
}

/// Leak a `String` to a `&'static str` (the oracle lives for the whole process).
fn leak_str(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

/// Leak a `Vec<String>` to a `&'static [&'static str]`.
fn leak_strs(v: Vec<String>) -> &'static [&'static str] {
    Box::leak(
        v.into_iter()
            .map(leak_str)
            .collect::<Vec<_>>()
            .into_boxed_slice(),
    )
}

/// Parse `oracle.json` once and leak it. Panics only on a malformed committed
/// manifest — a build-time invariant, not a runtime input.
fn loaded() -> &'static Loaded {
    static LOADED: OnceLock<Loaded> = OnceLock::new();
    LOADED.get_or_init(|| {
        let raw: RawOracle =
            serde_json::from_str(ORACLE_JSON).expect("committed oracle.json is valid JSON");

        // Group files into directories by their URL's parent path, preserving
        // first-seen order so provenance output stays deterministic.
        let mut order: Vec<String> = Vec::new();
        let mut groups: HashMap<String, (SourceKind, Vec<OracleFile>)> = HashMap::new();
        for f in raw.files {
            let hostpath = f.url.strip_prefix("https://").unwrap_or(&f.url);
            let (prefix, name) = match hostpath.rsplit_once('/') {
                Some((p, n)) => (p.to_string(), n.to_string()),
                None => (String::new(), hostpath.to_string()),
            };
            let kind = if hostpath.starts_with("ssd.jpl.nasa.gov/") {
                SourceKind::JplMirror
            } else {
                SourceKind::CdsCatalog
            };
            let file = OracleFile {
                name: leak_str(name),
                size: f.size,
                blake3_hex: leak_str(f.blake3),
                provides: leak_strs(f.provides),
                coverage: f.coverage.map(leak_str),
            };
            if let Some((_, files)) = groups.get_mut(&prefix) {
                files.push(file);
            } else {
                order.push(prefix.clone());
                groups.insert(prefix, (kind, vec![file]));
            }
        }
        let dirs: Vec<OracleDir> = order
            .into_iter()
            .map(|prefix| {
                let (kind, files) = groups.remove(&prefix).expect("prefix inserted above");
                OracleDir {
                    prefix: leak_str(prefix),
                    kind,
                    files: Box::leak(files.into_boxed_slice()),
                }
            })
            .collect();
        let dirs: &'static [OracleDir] = Box::leak(dirs.into_boxed_slice());

        let entourages: Vec<Entourage> = raw
            .entourages
            .into_iter()
            .map(|(slug, e)| Entourage {
                slug: leak_str(slug),
                label: leak_str(e.label),
                planets: leak_strs(e.planets),
                perturbers: leak_strs(e.perturbers),
                optional: leak_strs(e.optional),
            })
            .collect();
        let entourages: &'static [Entourage] = Box::leak(entourages.into_boxed_slice());

        let de_preference: Vec<DePreference> = raw
            .de_preference
            .into_iter()
            .map(|p| DePreference {
                slug: leak_str(p.slug),
                from_year: p.from_year,
                to_year: p.to_year,
            })
            .collect();
        let de_preference: &'static [DePreference] = Box::leak(de_preference.into_boxed_slice());

        Loaded {
            dirs,
            entourages,
            de_preference,
        }
    })
}

/// Compute the lowercase-hex unkeyed BLAKE3 of a file's bytes.
///
/// # Errors
/// Returns [`PericynthionError::Io`] if the file cannot be read.
pub fn hash_file(path: &Path) -> Result<String, PericynthionError> {
    let mut hasher = blake3::Hasher::new();
    let mut file = std::fs::File::open(path).map_err(|source| PericynthionError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    std::io::copy(&mut file, &mut hasher).map_err(|source| PericynthionError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(hasher.finalize().to_hex().to_string())
}

/// Outcome of checking one oracle entry against a file on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerifyStatus {
    /// Size and hash both match.
    Ok,
    /// File present but wrong size (hash not computed — fast fail).
    SizeMismatch {
        /// Expected size from the oracle.
        expected: u64,
        /// Actual size on disk.
        actual: u64,
    },
    /// Size matches but BLAKE3 differs.
    HashMismatch {
        /// Expected hash from the oracle.
        expected: &'static str,
        /// Actual hash computed from the file on disk.
        actual: String,
    },
    /// File not found under the root.
    Missing,
}

/// Per-file verification result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifyReport {
    /// Path relative to the mirror root (same as [`OracleEntry::path`]).
    pub path: String,
    /// Verification outcome for this file.
    pub status: VerifyStatus,
}

/// Verify a single oracle entry against a file on disk.
///
/// `root` is the directory that directly contains `ssd.jpl.nasa.gov/`; the
/// full path checked is `root.join(&entry.path)`.  Size is checked first; if
/// it differs the file is not hashed (fast fail).
#[must_use]
pub fn verify_entry(root: &Path, entry: &OracleEntry) -> VerifyReport {
    let full = root.join(&entry.path);
    VerifyReport {
        path: entry.path.clone(),
        status: verify_file(&full, entry),
    }
}

/// Verify a specific file on disk against an oracle entry's size and BLAKE3.
///
/// Unlike [`verify_entry`], which joins `entry.path` under a mirror root, this
/// checks the file *at `path` exactly* — useful when the file was located by a
/// layout-agnostic walk (a flat drop-folder) rather than the canonical mirror
/// layout. Size is checked first (fast fail), then the hash.
#[must_use]
pub fn verify_file(path: &Path, entry: &OracleEntry) -> VerifyStatus {
    match std::fs::metadata(path) {
        Err(_) => VerifyStatus::Missing,
        Ok(m) if m.len() != entry.size => VerifyStatus::SizeMismatch {
            expected: entry.size,
            actual: m.len(),
        },
        Ok(_) => match hash_file(path) {
            Ok(actual) if actual == entry.blake3_hex => VerifyStatus::Ok,
            Ok(actual) => VerifyStatus::HashMismatch {
                expected: entry.blake3_hex,
                actual,
            },
            Err(_) => VerifyStatus::Missing,
        },
    }
}

/// Verify every oracle entry resolved under `root`, returning one
/// [`VerifyReport`] per file.
///
/// `root` is the directory that directly contains `ssd.jpl.nasa.gov/`.
/// Delegates to [`verify_entry`] for each entry from [`entries()`].
#[must_use]
pub fn verify_against_root(root: &Path) -> Vec<VerifyReport> {
    entries().iter().map(|e| verify_entry(root, e)).collect()
}

/// All manifest directories (every [`SourceKind`]). Provenance reads this.
#[must_use]
pub fn manifest_dirs() -> &'static [OracleDir] {
    loaded().dirs
}

/// Every entourage (DE integration + its perturber set), for `data fetch`.
#[must_use]
pub fn entourages() -> &'static [Entourage] {
    loaded().entourages
}

/// Look up a single entourage by slug (e.g. `"de441"`, `"de431"`).
#[must_use]
pub fn entourage(slug: &str) -> Option<&'static Entourage> {
    loaded().entourages.iter().find(|e| e.slug == slug)
}

/// The date-aware DE-selection preference, **sorted best-precision-first**. A
/// selector walks this and takes the first [`DePreference`] that both
/// [`covers`](DePreference::covers) the requested year and is present on disk.
#[must_use]
pub fn de_preference() -> &'static [DePreference] {
    loaded().de_preference
}

/// Resolve an entourage slug to its integrity-checked [`OracleEntry`] list.
///
/// Returns the planets + perturbers; when `include_optional` is set, the heavier
/// optional extras are appended too. Any URL not present in the file oracle is
/// skipped (the manifest is internally consistent, so this is belt-and-braces).
/// Returns `None` when `slug` names no entourage.
#[must_use]
pub fn entourage_entries(slug: &str, include_optional: bool) -> Option<Vec<OracleEntry>> {
    let ent = entourage(slug)?;
    let all = entries();
    let resolve = |url: &str| -> Option<OracleEntry> {
        let path = url.strip_prefix("https://").unwrap_or(url);
        all.iter().find(|e| e.path == path).cloned()
    };
    let mut out = Vec::new();
    let optional = if include_optional {
        ent.optional
    } else {
        &[][..]
    };
    for url in ent
        .planets
        .iter()
        .chain(ent.perturbers.iter())
        .chain(optional.iter())
    {
        if let Some(e) = resolve(url) {
            out.push(e);
        }
    }
    Some(out)
}

/// The deduplicated union of every entourage's files (planets + perturbers +
/// optional), as integrity entries — the complete set of files starcat can
/// *use*, across all DE series. Drives `data migrate`, which cherry-picks these
/// out of whatever the user points at. First-seen order across entourages.
#[must_use]
pub fn all_entourage_entries() -> Vec<OracleEntry> {
    let all = entries();
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for ent in entourages() {
        for url in ent
            .planets
            .iter()
            .chain(ent.perturbers.iter())
            .chain(ent.optional.iter())
        {
            let path = url.strip_prefix("https://").unwrap_or(url);
            if seen.insert(path)
                && let Some(e) = all.iter().find(|e| e.path == path)
            {
                out.push(e.clone());
            }
        }
    }
    out
}

/// Number of JPL-mirror files in the oracle (the integrity surface).
#[must_use]
pub fn file_count() -> usize {
    manifest_dirs()
        .iter()
        .filter(|d| matches!(d.kind, SourceKind::JplMirror))
        .map(|d| d.files.len())
        .sum()
}

/// Flatten the JPL-mirror rows into full-path integrity entries.
///
/// Each [`OracleEntry`] has a `path` formed by joining the [`OracleDir::prefix`]
/// with the [`OracleFile::name`] via `/`.
#[must_use]
pub fn entries() -> Vec<OracleEntry> {
    manifest_dirs()
        .iter()
        .filter(|d| matches!(d.kind, SourceKind::JplMirror))
        .flat_map(|d| {
            d.files.iter().map(move |f| OracleEntry {
                path: format!("{}/{}", d.prefix, f.name),
                size: f.size,
                blake3_hex: f.blake3_hex,
            })
        })
        .collect()
}

/// The oracle subset for starcat's currently-supported placements: the DE441
/// binary dataset (header + ephemeris) plus the headline small-body SPK.
///
/// This is the ~3 GB `starcat data verify` checks and `starcat data prod`
/// lists. It is drawn from the oracle table itself — so the BLAKE3 hashes are
/// already known and selection needs no disk access — unlike
/// [`crate::manifest::production_data_files`], which discovers whatever DE
/// dataset (binary or ASCII) actually exists under a given path.
#[must_use]
pub fn production_entries() -> Vec<OracleEntry> {
    entries()
        .into_iter()
        .filter(|e| is_production_path(&e.path))
        .collect()
}

/// True when an oracle path belongs to the supported-placements subset:
/// the DE441 binary layout (`Linux/de441/header.441` + `linux_*.441`) or the
/// headline small-body SPK (`sb441-n16.bsp`).
fn is_production_path(path: &str) -> bool {
    let de441_binary = path.contains("/planets/Linux/de441/")
        && (path.ends_with("/header.441") || path.contains("/linux_"));
    let small_body = path.ends_with("/sb441-n16.bsp");
    de441_binary || small_body
}

/// Walk `start` and its ancestors looking for a directory that directly
/// contains an `ssd.jpl.nasa.gov/` child directory.
///
/// Returns `Some(d)` for the first such ancestor (including `start` itself),
/// or `None` if no ancestor matches.
///
/// # Return semantics
///
/// The returned path is the *mirror root* — the directory you pass to
/// [`verify_against_root`].  Oracle paths begin with
/// `ssd.jpl.nasa.gov/ftp/…`, so the mirror root is the directory that
/// **directly contains** `ssd.jpl.nasa.gov/`.
///
/// # Examples
///
/// Given a mirror laid out as `.../nasa/ssd.jpl.nasa.gov/ftp/eph/…`:
///
/// - `mirror_root_from(".../nasa/ssd.jpl.nasa.gov/ftp/eph/planets")` → `Some(".../nasa")`
/// - `mirror_root_from(".../nasa")` → `Some(".../nasa")`
/// - `mirror_root_from("/tmp/unrelated")` → `None`
#[must_use]
pub fn mirror_root_from(start: &Path) -> Option<PathBuf> {
    let mut current = start;
    loop {
        if current.join("ssd.jpl.nasa.gov").is_dir() {
            return Some(current.to_path_buf());
        }
        current = current.parent()?;
    }
}

/// Normalize a user-supplied start path to the mirror *root* — the directory
/// that contains (or will contain) `ssd.jpl.nasa.gov/` — for WRITE/fetch use,
/// where the mirror may not exist yet. Unlike [`mirror_root_from`], never
/// returns `None`: if no existing mirror is found by walking up, and the path
/// descends through an `ssd.jpl.nasa.gov` component, the parent of that
/// component is used (so `root.join(entry.path)` never doubles the segment);
/// otherwise the path is returned as the root to create the mirror under.
/// Lexical only — the second/third cases never touch the filesystem.
#[must_use]
pub fn mirror_root_for_write(start: &Path) -> PathBuf {
    // 1. An existing mirror wins.
    if let Some(root) = mirror_root_from(start) {
        return root;
    }
    // 2. Descends through an `ssd.jpl.nasa.gov` component: take everything
    //    before it, so `root.join(entry.path)` does not double the segment.
    let comps: Vec<std::path::Component> = start.components().collect();
    if let Some(idx) = comps
        .iter()
        .position(|c| c.as_os_str() == "ssd.jpl.nasa.gov")
    {
        return comps[..idx].iter().collect();
    }
    // 3. No mirror, no ssd component: create the mirror under `start` as-is.
    display_path_buf(start)
}

/// Render a filesystem path for display, collapsing repeated separators and
/// normalizing `.` components. Lexical only — never touches the filesystem
/// (safe for paths that don't exist yet; unlike `canonicalize`).
#[must_use]
pub fn display_path(p: &Path) -> String {
    display_path_buf(p).display().to_string()
}

/// Lexically tidy a path (collapse repeated separators, drop `.` components)
/// as a [`PathBuf`], for storing a clean value rather than only displaying it.
#[must_use]
fn display_path_buf(p: &Path) -> PathBuf {
    p.components().collect::<PathBuf>()
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    /// BLAKE3 of `"hello world\n"` — shared with A1/A3 tests.
    pub(super) const TEST_HELLO_HASH: &str =
        "dc5a4edb8240b018124052c330270696f96771a63b45250a5c17d3000e823355";

    #[test]
    fn mirror_root_for_write_uses_existing_mirror() {
        let tmp = tempdir::TempDir::new("oracle-write-existing").unwrap();
        // Build <tmp>/nasa/ssd.jpl.nasa.gov and point start at the ssd dir.
        let nasa = tmp.path().join("nasa");
        let ssd = nasa.join("ssd.jpl.nasa.gov");
        std::fs::create_dir_all(&ssd).unwrap();
        // start pointing *into* the mirror resolves up to the mirror root.
        assert_eq!(super::mirror_root_for_write(&ssd), nasa);
    }

    #[test]
    fn mirror_root_for_write_strips_ssd_component_when_absent() {
        // Nonexistent path descending through an ssd.jpl.nasa.gov component:
        // the parent of that component is the mirror root.
        assert_eq!(
            super::mirror_root_for_write(Path::new("/x/ssd.jpl.nasa.gov")),
            PathBuf::from("/x")
        );
        // Deep, still nonexistent — same answer.
        assert_eq!(
            super::mirror_root_for_write(Path::new("/x/ssd.jpl.nasa.gov/ftp/eph")),
            PathBuf::from("/x")
        );
    }

    #[test]
    fn mirror_root_for_write_returns_path_as_is_without_ssd_component() {
        assert_eq!(
            super::mirror_root_for_write(Path::new("/x/data")),
            PathBuf::from("/x/data")
        );
    }

    #[test]
    fn display_path_collapses_repeated_separators() {
        assert_eq!(super::display_path(Path::new("/a//b/")), "/a/b");
        assert_eq!(super::display_path(Path::new("a//b")), "a/b");
        // Root-only stays root.
        assert_eq!(super::display_path(Path::new("/")), "/");
        // Relative dot component is normalized away.
        assert_eq!(super::display_path(Path::new("a/./b")), "a/b");
    }

    #[test]
    fn verify_entry_reports_ok_missing_and_mismatch() {
        use std::io::Write;
        let tmp = tempdir::TempDir::new("oracle-verify").unwrap();
        let root = tmp.path();
        let entry = super::OracleEntry {
            path: "ssd.jpl.nasa.gov/ftp/eph/x/hello.txt".into(),
            size: 12,
            // b3sum of "hello world\n" — same value as Task A1 `want`.
            blake3_hex: TEST_HELLO_HASH,
        };
        // Missing:
        assert!(matches!(
            super::verify_entry(root, &entry).status,
            super::VerifyStatus::Missing
        ));
        // Create correct file → Ok:
        let full = root.join(&entry.path);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::File::create(&full)
            .unwrap()
            .write_all(b"hello world\n")
            .unwrap();
        assert!(matches!(
            super::verify_entry(root, &entry).status,
            super::VerifyStatus::Ok
        ));
        // Corrupt size → SizeMismatch:
        std::fs::File::create(&full)
            .unwrap()
            .write_all(b"short")
            .unwrap();
        assert!(matches!(
            super::verify_entry(root, &entry).status,
            super::VerifyStatus::SizeMismatch {
                expected: 12,
                actual: 5
            }
        ));
    }

    #[test]
    fn verify_entry_reports_hash_mismatch_on_same_size_wrong_bytes() {
        use std::io::Write;
        let tmp = tempdir::TempDir::new("oracle-hashmismatch").unwrap();
        let root = tmp.path();
        // Entry claims 12 bytes hashing to TEST_HELLO_HASH ("hello world\n").
        let entry = super::OracleEntry {
            path: "ssd.jpl.nasa.gov/ftp/eph/x/hello.txt".into(),
            size: 12,
            blake3_hex: TEST_HELLO_HASH,
        };
        let full = root.join(&entry.path);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        // Same byte length (12) but different content → size check passes,
        // hash check fails: exercises the HashMismatch arm.
        std::fs::File::create(&full)
            .unwrap()
            .write_all(b"HELLO world\n")
            .unwrap();
        match super::verify_entry(root, &entry).status {
            super::VerifyStatus::HashMismatch { expected, actual } => {
                assert_eq!(expected, TEST_HELLO_HASH);
                assert_ne!(actual, TEST_HELLO_HASH);
                assert_eq!(actual.len(), 64);
                assert!(actual.bytes().all(|b| b.is_ascii_hexdigit()));
            }
            other => panic!("expected HashMismatch, got {other:?}"),
        }
    }

    #[test]
    fn hash_file_matches_known_blake3() {
        use std::io::Write;
        let tmp = tempdir::TempDir::new("oracle-hash").unwrap();
        let p = tmp.path().join("hello.txt");
        let mut f = std::fs::File::create(&p).unwrap();
        f.write_all(b"hello world\n").unwrap();
        // b3sum of "hello world\n":
        let want = "dc5a4edb8240b018124052c330270696f96771a63b45250a5c17d3000e823355";
        let got = super::hash_file(&p).unwrap();
        assert_eq!(got.len(), 64);
        assert_eq!(got, want);
    }

    #[test]
    fn mirror_root_from_finds_ancestor_containing_ssd_dir() {
        let tmp = tempdir::TempDir::new("oracle-mirror-root").unwrap();
        let root = tmp.path();
        // Build root/ssd.jpl.nasa.gov/ftp/eph/planets/deep/
        let deep = root.join("ssd.jpl.nasa.gov/ftp/eph/planets/deep");
        std::fs::create_dir_all(&deep).unwrap();

        // Walking up from a deeply nested dir should find root.
        assert_eq!(super::mirror_root_from(&deep), Some(root.to_path_buf()));

        // Walking from ssd.jpl.nasa.gov itself should also find root (its parent has the child).
        let ssd_dir = root.join("ssd.jpl.nasa.gov");
        assert_eq!(super::mirror_root_from(&ssd_dir), Some(root.to_path_buf()));

        // Walking from root itself should return root.
        assert_eq!(super::mirror_root_from(root), Some(root.to_path_buf()));

        // An unrelated directory has no ssd.jpl.nasa.gov ancestor.
        let unrelated = tempdir::TempDir::new("oracle-unrelated").unwrap();
        assert_eq!(super::mirror_root_from(unrelated.path()), None);
    }

    #[test]
    fn production_entries_are_the_de441_binary_plus_small_body_spk() {
        let prod = super::production_entries();
        assert!(!prod.is_empty(), "production subset must not be empty");
        // The supported subset is small — a handful of files, not the mirror.
        assert!(prod.len() <= 8, "unexpectedly large: {}", prod.len());
        let ends = |s: &str| prod.iter().any(|e| e.path.ends_with(s));
        assert!(ends("ftp/eph/planets/Linux/de441/header.441"));
        assert!(ends("ftp/eph/planets/Linux/de441/linux_m13000p17000.441"));
        assert!(ends("ftp/eph/small_bodies/asteroids_de441/sb441-n16.bsp"));
        // No ASCII-layout or unrelated DE sets leak in.
        assert!(prod.iter().all(|e| !e.path.contains("/ascii/")));
        assert!(prod.iter().all(|e| !e.path.contains("/de102/")));
    }

    #[test]
    fn oracle_covers_all_mirrored_files() {
        // The mirror we hashed contained exactly 1374 files under the three trees.
        assert_eq!(super::file_count(), 1374);
        // Every hash is 64 lowercase hex chars; every path starts at the mirror root.
        for e in super::entries() {
            assert_eq!(e.blake3_hex.len(), 64, "bad hash for {}", e.path);
            assert!(e.blake3_hex.bytes().all(|b| b.is_ascii_hexdigit()));
            assert!(
                e.path.starts_with("ssd.jpl.nasa.gov/ftp/eph/"),
                "{}",
                e.path
            );
        }
        // DE441 binary is present and known-size.
        let bin = super::entries()
            .into_iter()
            .find(|e| e.path.ends_with("Linux/de441/linux_m13000p17000.441"))
            .expect("DE441 binary in oracle");
        assert_eq!(bin.size, 2_788_676_624);
    }

    #[test]
    fn manifest_includes_catalog_gz_with_star_coverage() {
        let cds: Vec<&super::OracleFile> = super::manifest_dirs()
            .iter()
            .filter(|d| matches!(d.kind, super::SourceKind::CdsCatalog))
            .flat_map(|d| d.files.iter())
            .collect();
        let cat = cds
            .iter()
            .find(|f| f.name == "catalog.gz")
            .expect("catalog.gz present in manifest");
        assert_eq!(cat.provides, &[super::STAR_CLASS_ALL]);
        assert_eq!(cat.blake3_hex.len(), 64);
        // Integrity surface is unchanged: catalog.gz is NOT in entries().
        assert!(
            super::entries()
                .iter()
                .all(|e| !e.path.ends_with("catalog.gz"))
        );
    }

    #[test]
    fn harvard_ybsc5_is_a_byte_identical_alternate_of_cds_catalog() {
        let cds: Vec<&super::OracleFile> = super::manifest_dirs()
            .iter()
            .filter(|d| matches!(d.kind, super::SourceKind::CdsCatalog))
            .flat_map(|d| d.files.iter())
            .collect();
        let cat = cds
            .iter()
            .find(|f| f.name == "catalog.gz")
            .expect("CDS catalog.gz present");
        let ybsc5 = cds
            .iter()
            .find(|f| f.name == "ybsc5.gz")
            .expect("Harvard ybsc5.gz present");
        // Same BSC5 bytes mirrored from two hosts: identical hash + size.
        assert_eq!(cat.blake3_hex, ybsc5.blake3_hex);
        assert_eq!(cat.size, ybsc5.size);
        assert_eq!(ybsc5.provides, &[super::STAR_CLASS_ALL]);
    }

    #[test]
    fn sb441_bundles_declare_their_bodies() {
        let by_name = |n: &str| {
            super::manifest_dirs()
                .iter()
                .flat_map(|d| d.files.iter())
                .find(|f| f.name == n)
                .unwrap_or_else(|| panic!("{n} in manifest"))
        };
        assert!(by_name("sb441-n16.bsp").provides.contains(&"Ceres"));
        assert!(by_name("sb441-n373.bsp").provides.contains(&"Eris"));
        assert!(by_name("sb441-n373.bsp").provides.contains(&"Sedna"));
        // Albion is Horizons-only — must NOT be claimed by n373.
        assert!(!by_name("sb441-n373.bsp").provides.contains(&"Albion"));
    }

    #[test]
    fn de441_entourage_resolves_to_integrity_entries() {
        let ent = super::entourage("de441").expect("de441 entourage present");
        assert_eq!(ent.label, "DE441");
        // The DE integration itself: header + full-span binary.
        assert_eq!(ent.planets.len(), 2);
        assert!(!ent.perturbers.is_empty());

        // Default fetch (no optional) resolves to real, hash-pinned entries.
        let entries = super::entourage_entries("de441", false).expect("resolves");
        assert!(!entries.is_empty());
        assert!(entries.iter().all(|e| e.blake3_hex.len() == 64));
        assert!(
            entries
                .iter()
                .any(|e| e.path.ends_with("Linux/de441/linux_m13000p17000.441"))
        );
        // Optional adds the heavy full sb441-n373 bundle.
        let with_opt = super::entourage_entries("de441", true).expect("resolves");
        assert!(with_opt.len() > entries.len());
        assert!(with_opt.iter().any(|e| e.path.ends_with("sb441-n373.bsp")));
    }

    #[test]
    fn all_entourage_entries_span_series_and_dedup_shared_files() {
        let all = super::all_entourage_entries();
        assert!(!all.is_empty());
        // Dedup: sb441-n16 is shared by de440 and de441 but appears once.
        let n16 = all
            .iter()
            .filter(|e| e.path.ends_with("sb441-n16.bsp"))
            .count();
        assert_eq!(n16, 1, "shared perturber must be de-duplicated");
        // Spans multiple series: both a DE441 and a DE431 binary are present.
        assert!(
            all.iter()
                .any(|e| e.path.ends_with("linux_m13000p17000.441"))
        );
        assert!(all.iter().any(|e| e.path.ends_with("lnxm13000p17000.431")));
        // Every hash is well-formed and every path is under the mirror.
        assert!(all.iter().all(|e| e.blake3_hex.len() == 64));
        assert!(all.iter().all(|e| e.path.starts_with("ssd.jpl.nasa.gov/")));
    }

    #[test]
    fn de_preference_slugs_are_all_valid_entourages() {
        let prefs = super::de_preference();
        assert!(!prefs.is_empty(), "preference list must be populated");
        for p in prefs {
            assert!(
                super::entourage(p.slug).is_some(),
                "de_preference references unknown entourage: {}",
                p.slug
            );
            assert!(p.from_year <= p.to_year, "bad window for {}", p.slug);
        }
    }

    #[test]
    fn de_preference_ranks_de440_above_de441_and_both_cover_today() {
        let prefs = super::de_preference();
        let pos = |slug: &str| prefs.iter().position(|p| p.slug == slug);
        let (i440, i441) = (pos("de440").unwrap(), pos("de441").unwrap());
        assert!(
            i440 < i441,
            "de440 must outrank de441 (more precise in-window)"
        );
        // For a modern year, walking the list top-down reaches de440 first.
        let year = 2026;
        let first_covering = prefs.iter().find(|p| p.covers(year)).unwrap();
        assert_eq!(
            first_covering.slug, "de440",
            "the most-preferred covering DE for {year} is de440"
        );
        // de441's deep-time window also covers it (the fallback when de440 absent).
        assert!(prefs[i441].covers(year));
    }

    #[test]
    fn de_preference_covers_boundaries_and_bce() {
        let p = super::de_preference()
            .iter()
            .find(|p| p.slug == "de441")
            .unwrap();
        assert!(p.covers(p.from_year) && p.covers(p.to_year)); // inclusive
        assert!(p.covers(-5000)); // deep BCE inside DE441
        assert!(!p.covers(p.to_year + 1)); // just past the end
    }

    #[test]
    fn entourage_files_have_globally_unique_basenames() {
        // No two usable files share a base name, so flattening the entourage set
        // never collides and the migrator's per-file location is unambiguous.
        // (Cross-layout twins like `ascii/header.NNN` live outside this set and
        // are handled by content-verified location in `migrate_scan`.)
        let mut seen = std::collections::HashSet::new();
        for e in super::all_entourage_entries() {
            let base = e.path.rsplit('/').next().unwrap().to_string();
            assert!(
                seen.insert(base.clone()),
                "duplicate entourage basename: {base}"
            );
        }
    }

    #[test]
    fn unknown_entourage_slug_is_none() {
        assert!(super::entourage("de999").is_none());
        assert!(super::entourage_entries("de999", false).is_none());
    }

    #[test]
    fn every_entourage_url_resolves_in_the_file_oracle() {
        // Integrity closure: no entourage may reference a URL absent from the
        // file oracle (guards against a typo'd path in oracle.json).
        let paths: std::collections::HashSet<String> =
            super::entries().into_iter().map(|e| e.path).collect();
        for ent in super::entourages() {
            for url in ent
                .planets
                .iter()
                .chain(ent.perturbers.iter())
                .chain(ent.optional.iter())
            {
                let path = url.strip_prefix("https://").unwrap_or(url);
                assert!(
                    paths.contains(path),
                    "entourage {} references unknown oracle path: {url}",
                    ent.slug
                );
            }
        }
    }
}
