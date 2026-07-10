//! Filesystem-aware resolution over the pure [`crate::provenance`] schema:
//! cache-presence checks and the production file list. This is the OS-join
//! layer; `provenance.rs` itself stays pure (no env, no disk).

use crate::jpl::oracle;
use crate::provenance::{Provider, RootKind};
use std::path::{Path, PathBuf};

/// Maximum directory depth the tree walkers descend before giving up. A loop
/// backstop, not a real limit: JPL mirror trees are only ~6 levels deep, so 64
/// is generous while still bounding a pathological (e.g. symlink-induced) walk.
const MAX_WALK_DEPTH: usize = 64;

/// Locate a JPL data file starting from wherever the user pointed.
///
/// If `start` sits inside (or above) a real `ssd.jpl.nasa.gov/` mirror, hoist to
/// that mirror root first (see [`crate::jpl::oracle::mirror_root_from`]);
/// otherwise search `start` as-is. Then walk down via [`find_under`]. This is
/// the single common JPL-data locator: it makes "point it anywhere" hold for
/// both a flat drop-folder and a deep point inside a full mirror (e.g. pointing
/// at `.../planets/Linux/de441` still finds `sb441-n16.bsp` over in the sibling
/// `.../small_bodies/...` branch).
#[must_use]
pub fn locate_jpl_file(start: &Path, name: &str) -> Option<PathBuf> {
    let root = oracle::mirror_root_from(start).unwrap_or_else(|| start.to_path_buf());
    find_under(&root, name)
}

/// Like [`locate_jpl_file`], but matches file names by a predicate — for the
/// `header.<digits>` case where the ephemeris number varies. Same mirror-root
/// hoist, then [`find_under_matching`].
#[must_use]
pub fn locate_jpl_file_matching(
    start: &Path,
    matches: impl FnMut(&str) -> bool,
) -> Option<PathBuf> {
    let root = oracle::mirror_root_from(start).unwrap_or_else(|| start.to_path_buf());
    find_under_matching(&root, matches)
}

/// Like [`locate_jpl_file`], but accepts a file only when `accept` (given the
/// full path) returns true — the same mirror-root hoist, then
/// [`find_under_accepting`]. Lets a caller require a **content** match, walking
/// past a same-named file from another layout whose bytes differ.
#[must_use]
pub fn locate_jpl_file_accepting(
    start: &Path,
    accept: impl FnMut(&Path) -> bool,
) -> Option<PathBuf> {
    let root = oracle::mirror_root_from(start).unwrap_or_else(|| start.to_path_buf());
    find_under_accepting(&root, accept)
}

/// Find the first file named `name` anywhere in the tree rooted at `root`.
///
/// Layout-agnostic: works whether the user mirrored the full
/// `ssd.jpl.nasa.gov/` tree or dropped the file in a single flat directory.
/// Bounded recursive walk; does NOT follow symlinks (no loops); returns the
/// first match in a deterministic (lexicographically sorted) traversal order.
///
/// If `root` itself is a file named `name`, that is a match. If `root` does not
/// exist or is not readable, returns `None` (never errors).
#[must_use]
pub fn find_under(root: &Path, name: &str) -> Option<PathBuf> {
    find_under_matching(root, |candidate| candidate == name)
}

/// Like [`find_under`], but matches file names by an arbitrary predicate.
///
/// Useful when the target name is not a fixed string — e.g. locating a
/// `header.<digits>` DE-series header where the ephemeris number varies.
/// Same traversal contract as [`find_under`]: deterministic lexicographic
/// order, no symlink following, bounded depth, first match wins.
#[must_use]
pub fn find_under_matching(root: &Path, mut matches: impl FnMut(&str) -> bool) -> Option<PathBuf> {
    find_under_accepting(root, |p| {
        p.file_name()
            .and_then(|n| n.to_str())
            .is_some_and(&mut matches)
    })
}

/// Like [`find_under_matching`], but the predicate sees the **full path**, so it
/// can accept a file by *content* as well as name.
///
/// This is what lets a caller skip a same-named file from another layout whose
/// bytes differ (e.g. a DE-series `header.NNN` under `ascii/` that is not the
/// `Linux/` one it wants) and keep walking until a genuine match is found —
/// rather than stopping at the first name match and rejecting it. Same traversal
/// contract as [`find_under`]: deterministic lexicographic order, no symlink
/// following, bounded depth, first accepted file wins.
#[must_use]
pub fn find_under_accepting(root: &Path, mut accept: impl FnMut(&Path) -> bool) -> Option<PathBuf> {
    // `root` itself being an accepted file counts as a match.
    if root.is_file() && accept(root) {
        return Some(root.to_path_buf());
    }
    find_under_rec(root, &mut accept, 0)
}

/// Depth-first search body. Uses `symlink_metadata` so symlinked directories are
/// not descended into (no cycles). Entries within each directory are sorted by
/// file name for deterministic first-match ordering.
fn find_under_rec(
    dir: &Path,
    accept: &mut impl FnMut(&Path) -> bool,
    depth: usize,
) -> Option<PathBuf> {
    if depth >= MAX_WALK_DEPTH {
        return None;
    }
    let Ok(read) = std::fs::read_dir(dir) else {
        return None;
    };
    // Collect + sort for a deterministic traversal order.
    let mut entries: Vec<PathBuf> = read.filter_map(|e| e.ok().map(|e| e.path())).collect();
    entries.sort();

    // Files first (a match at this level short-circuits before descending).
    for entry in &entries {
        let Ok(meta) = std::fs::symlink_metadata(entry) else {
            continue;
        };
        if meta.file_type().is_file() && accept(entry) {
            return Some(entry.clone());
        }
    }
    // Then descend into real (non-symlink) subdirectories, in sorted order.
    for entry in &entries {
        let Ok(meta) = std::fs::symlink_metadata(entry) else {
            continue;
        };
        if meta.file_type().is_dir()
            && let Some(hit) = find_under_rec(entry, accept, depth + 1)
        {
            return Some(hit);
        }
    }
    None
}

/// True when a provider's file exists locally. JPL files resolve under the
/// mirror root; Horizons files under the Horizons dir; CDS (`catalog.gz`)
/// resolves at its `rel_path` as given. A `None` root means "not configured"
/// → not cached.
#[must_use]
pub fn provider_cached(p: &Provider, jpl_root: Option<&Path>, horizons_dir: Option<&Path>) -> bool {
    match p.root_kind {
        RootKind::JplMirror => jpl_root.is_some_and(|r| r.join(&p.rel_path).is_file()),
        RootKind::HorizonsDir => horizons_dir.is_some_and(|d| d.join(&p.rel_path).is_file()),
        RootKind::CdsBuild => Path::new(&p.rel_path).is_file(),
    }
}

/// Build the production file list at runtime: the JPL subset (DE441 + n16) plus
/// `sb441-n373.bsp`, all joined under `jpl_root`, plus each unbundled minor
/// body's Horizons `<naif>.bsp` joined under `horizons_dir`. Returns absolute
/// or relative `PathBuf`s exactly as joined (display formatting is the caller's
/// concern).
#[must_use]
pub fn production_file_paths(jpl_root: &Path, horizons_dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for e in oracle::production_entries() {
        out.push(jpl_root.join(&e.path));
    }
    for e in oracle::entries() {
        if e.path.ends_with("sb441-n373.bsp") {
            out.push(jpl_root.join(&e.path));
        }
    }
    for (_name, naif) in crate::production_horizons_targets() {
        out.push(horizons_dir.join(format!("{naif}.bsp")));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jpl::oracle::SourceKind;

    #[test]
    fn find_under_finds_file_directly_under_root() {
        let tmp = tempdir::TempDir::new("find-flat").unwrap();
        let f = tmp.path().join("sb441-n16.bsp");
        std::fs::write(&f, b"x").unwrap();
        assert_eq!(find_under(tmp.path(), "sb441-n16.bsp"), Some(f));
    }

    #[test]
    fn find_under_finds_file_deep_in_archivist_layout() {
        let tmp = tempdir::TempDir::new("find-deep").unwrap();
        let deep = tmp
            .path()
            .join("ssd.jpl.nasa.gov/ftp/eph/small_bodies/asteroids_de441");
        std::fs::create_dir_all(&deep).unwrap();
        let f = deep.join("sb441-n16.bsp");
        std::fs::write(&f, b"x").unwrap();
        assert_eq!(find_under(tmp.path(), "sb441-n16.bsp"), Some(f));
    }

    #[test]
    fn find_under_returns_none_when_absent() {
        let tmp = tempdir::TempDir::new("find-absent").unwrap();
        assert_eq!(find_under(tmp.path(), "sb441-n16.bsp"), None);
        // A non-existent root is None, not an error.
        assert_eq!(find_under(&tmp.path().join("nope"), "x"), None);
    }

    #[test]
    fn find_under_matches_by_predicate() {
        let tmp = tempdir::TempDir::new("find-pred").unwrap();
        std::fs::write(tmp.path().join("header.441"), b"x").unwrap();
        let hit = find_under_matching(tmp.path(), |n| {
            n.starts_with("header.") && n["header.".len()..].chars().all(|c| c.is_ascii_digit())
        });
        assert_eq!(hit, Some(tmp.path().join("header.441")));
    }

    #[test]
    fn find_under_treats_root_file_as_match() {
        let tmp = tempdir::TempDir::new("find-rootfile").unwrap();
        let f = tmp.path().join("header.441");
        std::fs::write(&f, b"x").unwrap();
        assert_eq!(find_under(&f, "header.441"), Some(f));
    }

    #[test]
    #[cfg(unix)]
    fn find_under_does_not_hang_on_symlink_cycle() {
        use std::os::unix::fs::symlink;
        let tmp = tempdir::TempDir::new("find-cycle").unwrap();
        let root = tmp.path();
        let sub = root.join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        // A real sibling file to find.
        let target = sub.join("sb441-n16.bsp");
        std::fs::write(&target, b"x").unwrap();
        // A directory symlink pointing back at an ancestor (a cycle).
        if symlink(root, sub.join("loop")).is_err() {
            eprintln!("skip: symlink creation unavailable on this platform");
            return;
        }
        // Must terminate and still find the real file.
        assert_eq!(find_under(root, "sb441-n16.bsp"), Some(target));
    }

    #[test]
    fn provider_cached_checks_correct_root() {
        let tmp = tempdir::TempDir::new("datafiles").unwrap();
        let rel = "ssd.jpl.nasa.gov/eph/x.bsp";
        let p = Provider {
            kind: SourceKind::JplMirror,
            root_kind: RootKind::JplMirror,
            rel_path: rel.to_string(),
            source_url: String::new(),
            coverage: None,
        };
        // No file yet → not cached.
        assert!(!provider_cached(&p, Some(tmp.path()), None));
        // None root → not cached.
        assert!(!provider_cached(&p, None, None));
        // Create the file → cached.
        let full = tmp.path().join(rel);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(&full, b"x").unwrap();
        assert!(provider_cached(&p, Some(tmp.path()), None));
    }

    #[test]
    fn production_file_paths_include_n373_and_horizons_bodies() {
        let jpl = Path::new("/jpl");
        let hz = Path::new("/hz");
        let paths = production_file_paths(jpl, hz);
        assert!(
            paths.iter().any(|p| p.ends_with("sb441-n373.bsp")),
            "expected the n373 bundle"
        );
        // Each unbundled Horizons body contributes a <naif>.bsp under the hz dir.
        for (_name, naif) in crate::production_horizons_targets() {
            let want = hz.join(format!("{naif}.bsp"));
            assert!(paths.contains(&want), "missing {want:?}");
        }
    }
}
