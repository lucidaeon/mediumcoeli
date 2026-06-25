//! Acceptance: verify the hardcoded BLAKE3 oracle against the real local mirror.
//!
//! Both tests are `#[ignore]`d — hashing real ephemeris data takes seconds to
//! minutes, so the standard suite (`just test`) never runs them; the committed
//! oracle (with documented regeneration) is the source of truth. Invoke on
//! demand:
//! `cargo test --release -p pericynthion --test oracle_mirror -- --ignored`.
//! Both skip cleanly unless `$ASTRO_SPECIMENS/nasa` holds the
//! `ssd.jpl.nasa.gov/` mirror the oracle was generated from.
//!
//! - [`oracle_matches_production_subset`] — the narrow net: only the hash-pinned
//!   files backing the bodies starcat supports (DE441 binary + `sb441-n16` +
//!   `sb441-n373`), i.e. what `starcat data prod` ships minus the presence-only
//!   Horizons SPKs. ~18 GB, seconds.
//! - [`oracle_matches_local_mirror`] — the full net: every mirrored `eph/` file,
//!   each asserted against the oracle's recorded size + hash. ~190 GB, minutes.
use pericynthion::jpl::oracle;
use std::path::PathBuf;

/// Resolve the mirror root (the directory that directly contains
/// `ssd.jpl.nasa.gov/`) from `$ASTRO_SPECIMENS`, or `None` if absent.
fn mirror_root() -> Option<PathBuf> {
    let base = std::env::var_os("ASTRO_SPECIMENS")?;
    oracle::mirror_root_from(&PathBuf::from(base).join("nasa"))
}

/// The narrow net: verify only the hash-pinned files backing the bodies starcat
/// supports — the DE441 binary plus the `sb441-n16`/`sb441-n373` small-body
/// bundles. This is what `starcat data prod` ships, minus the presence-only
/// Horizons SPKs (which carry no oracle hash). Hashes ~18 GB, so it runs in
/// seconds rather than minutes — but it still re-hashes committed oracle data,
/// so it is gated behind `#[ignore]` like the full-mirror check.
#[test]
#[ignore = "slow: hashes the ~18 GB supported-placements subset; run explicitly: cargo test --release -p pericynthion --test oracle_mirror -- --ignored"]
fn oracle_matches_production_subset() {
    let Some(root) = mirror_root() else {
        eprintln!("skip: need $ASTRO_SPECIMENS/nasa/ssd.jpl.nasa.gov mirror");
        return;
    };

    // `production_entries()` is the DE441 binary + `sb441-n16`; `data prod` also
    // ships the larger `sb441-n373` bundle (Eris, Sedna, …), pulled from the
    // manifest by name exactly as starcat's `prod_paths` does.
    let mut entries = oracle::production_entries();
    entries.extend(
        oracle::entries()
            .into_iter()
            .filter(|e| e.path.ends_with("sb441-n373.bsp")),
    );
    assert!(!entries.is_empty(), "production subset must not be empty");

    let bad: Vec<_> = entries
        .iter()
        .map(|e| oracle::verify_entry(&root, e))
        .filter(|r| !matches!(r.status, oracle::VerifyStatus::Ok))
        .collect();
    assert!(
        bad.is_empty(),
        "{} of {} production-subset files failed oracle verification: {:#?}",
        bad.len(),
        entries.len(),
        bad
    );
}

#[test]
#[ignore = "slow: hashes the full ~190 GB JPL eph mirror; run explicitly: cargo test --release -p pericynthion --test oracle_mirror -- --ignored"]
fn oracle_matches_local_mirror() {
    let Some(root) = mirror_root() else {
        eprintln!("skip: need $ASTRO_SPECIMENS/nasa/ssd.jpl.nasa.gov mirror");
        return;
    };

    let reports = oracle::verify_against_root(&root);
    assert_eq!(
        reports.len(),
        oracle::file_count(),
        "verify covered {} files but the oracle holds {}",
        reports.len(),
        oracle::file_count()
    );

    let bad: Vec<_> = reports
        .iter()
        .filter(|r| !matches!(r.status, oracle::VerifyStatus::Ok))
        .collect();
    assert!(
        bad.is_empty(),
        "{} of {} files failed oracle verification; first {}: {:#?}",
        bad.len(),
        reports.len(),
        bad.len().min(10),
        &bad[..bad.len().min(10)]
    );
}
