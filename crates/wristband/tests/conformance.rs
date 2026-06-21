//! Conformance tests for the `wristband` safety spine (INV-6).
//!
//! These tests are the proof that the library upholds its documented invariants.
//! They are deliberately integration-level so that any refactor that breaks
//! an invariant will also break a test.
use wristband::{Browser, ReadOptions, WristbandError, read_cookies, scan_names};

// ---------------------------------------------------------------------------
// INV-1 / INV-6: empty allow-list is a hard error in ALL entry points
// ---------------------------------------------------------------------------

#[test]
fn empty_allow_list_is_a_hard_error() {
    // Both a specific browser and the "all stores" (None) path must reject empty.
    let err = read_cookies(Some(Browser::Firefox), &[], &ReadOptions::default()).unwrap_err();
    assert!(
        matches!(err, WristbandError::EmptyAllowList),
        "expected EmptyAllowList, got {err:?}"
    );
    let err_all = read_cookies(None, &[], &ReadOptions::default()).unwrap_err();
    assert!(
        matches!(err_all, WristbandError::EmptyAllowList),
        "expected EmptyAllowList for None path, got {err_all:?}"
    );
}

#[test]
fn scan_names_also_rejects_empty_allow_list() {
    let err = scan_names(Some(Browser::Chrome), &[], &ReadOptions::default()).unwrap_err();
    assert!(matches!(err, WristbandError::EmptyAllowList));
    let err_all = scan_names(None, &[], &ReadOptions::default()).unwrap_err();
    assert!(matches!(err_all, WristbandError::EmptyAllowList));
}

// ---------------------------------------------------------------------------
// INV-5 / INV-6d: no network-I/O crates used in wristband sources
// ---------------------------------------------------------------------------

fn collect_rs_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    for e in std::fs::read_dir(dir).expect("src dir readable").flatten() {
        let p = e.path();
        if p.is_dir() {
            collect_rs_files(&p, out);
        } else if p.extension().and_then(|x| x.to_str()) == Some("rs") {
            out.push(p);
        }
    }
}

#[test]
fn no_networking_crate_in_sources() {
    use std::path::Path;

    let src_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let forbidden = ["use reqwest", "use hyper", "use ureq", "use isahc"];

    let mut rs_files = Vec::new();
    collect_rs_files(&src_dir, &mut rs_files);

    for path in rs_files {
        let content = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
        for banned in &forbidden {
            assert!(
                !content.contains(banned),
                "found network-I/O import `{banned}` in {}",
                path.display()
            );
        }
    }
}
