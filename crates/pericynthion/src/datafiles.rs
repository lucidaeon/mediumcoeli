//! Filesystem-aware resolution over the pure [`crate::provenance`] schema:
//! cache-presence checks and the production file list. This is the OS-join
//! layer; `provenance.rs` itself stays pure (no env, no disk).

use crate::jpl::oracle;
use crate::provenance::{Provider, RootKind};
use std::path::{Path, PathBuf};

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
