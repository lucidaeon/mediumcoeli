//! Acceptance: `parse_header` against every committed `SFcht` specimen.
//!
//! Resolves `$ASTRO_SPECIMENS`, scopes to its `sfcht/` subdir (the specimen
//! corpus convention — see `AGENTS.md`), and walks that recursively for any
//! file whose extension is `SFcht` (case-insensitive), then asserts the format
//! invariants documented in `sfcht.ksy`:
//!
//! - `version == 3` (observed across the entire reference corpus)
//! - `record_count > 0` (no `SFcht` file ships with zero records)
//!
//! Skips cleanly when `$ASTRO_SPECIMENS` is unset — see the "Test corpus"
//! section of `AGENTS.md`.

use astrogram::sfcht::parse_header;
use std::fs;
use std::path::{Path, PathBuf};

fn collect_sfcht_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for entry in rd.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_sfcht_files(&path, out);
        } else if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("sfcht"))
        {
            out.push(path);
        }
    }
}

#[test]
fn parses_every_real_sfcht_header() {
    let Some(root) = std::env::var_os("ASTRO_SPECIMENS").map(PathBuf::from) else {
        eprintln!("ASTRO_SPECIMENS not set — skipping integration test");
        return;
    };
    if !root.exists() {
        eprintln!("specimens dir absent ({}); skipping", root.display());
        return;
    }

    // Scope the walk to the `sfcht/` specimen subdir. The `$ASTRO_SPECIMENS`
    // root also holds multi-million-file mirrors (star catalogs, SPK data);
    // walking it whole costs ~60s of `stat()` syscalls to find 4 files. Fall
    // back to the root if the subdir is absent (other corpus layouts).
    let sfcht_dir = root.join("sfcht");
    let walk_root = if sfcht_dir.is_dir() {
        &sfcht_dir
    } else {
        &root
    };
    let mut paths = Vec::new();
    collect_sfcht_files(walk_root, &mut paths);
    paths.sort();

    let mut count = 0usize;
    for path in &paths {
        let bytes = fs::read(path).expect("read specimen");
        let header = parse_header(&bytes)
            .unwrap_or_else(|e| panic!("{}: parse_header failed: {e}", path.display()));
        assert_eq!(
            header.version,
            3,
            "unexpected version in {}",
            path.display()
        );
        assert!(
            header.record_count > 0,
            "zero-record file: {}",
            path.display()
        );
        eprintln!(
            "  {:<32} v{} records={:<4} desc={:?}",
            path.file_name().unwrap().to_string_lossy(),
            header.version,
            header.record_count,
            header.description,
        );
        count += 1;
    }
    assert!(count > 0, "no .SFcht specimens found in {}", root.display());
    eprintln!("acceptance: parsed {count} real SFcht headers");
}
