//! Integration test: parse the real DE441 ASCII header.
//!
//! Resolves the JPL data directory from `$STARCAT_JPL_DATA`,
//! auto-discovers the header, and verifies the parsed values match the
//! documented DE441 layout.
//!
//! Skips gracefully when `$STARCAT_JPL_DATA` is unset or the directory contains no header.

use pericynthion::jpl::{discover, header::parse};
use std::path::PathBuf;

/// Resolve the JPL ASCII header path via `locate`.
fn locate_header() -> Option<PathBuf> {
    let val = std::env::var_os("STARCAT_JPL_DATA")?;
    let dir = PathBuf::from(val);
    let loc =
        discover::locate(&dir).unwrap_or_else(|e| panic!("STARCAT_JPL_DATA locate failed: {e}"));
    let paths = match loc {
        discover::DatasetLocation::Binary(p) => p,
        discover::DatasetLocation::Ascii { .. } => {
            panic!("expected binary DE dataset under {}", dir.display())
        }
    };
    Some(paths.header)
}

#[test]
fn parses_real_de441_header_layout_and_constants() {
    let Some(path) = locate_header() else {
        eprintln!("STARCAT_JPL_DATA not set — skipping integration test");
        return;
    };

    let source = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read {} failed: {e}", path.display()));
    let header = parse(&source).expect("real header should parse cleanly");

    // === Record size invariants ===
    assert_eq!(header.ksize, 2036, "DE441 KSIZE is fixed at 2036");
    assert_eq!(header.ncoeff, 1018, "DE441 NCOEFF is fixed at 1018");
    assert_eq!(
        header.ksize,
        2 * header.ncoeff,
        "KSIZE must always be 2·NCOEFF"
    );

    // === Title sanity ===
    assert!(
        header.title.iter().any(|line| line.contains("DE441")),
        "title block should mention DE441; got {:?}",
        header.title
    );

    // === Epoch span (DE441 covers ~13200 BCE to 17191 CE) ===
    assert!(
        header.epoch.start_jd < -3_000_000.0,
        "start JD should be in deep antiquity; got {}",
        header.epoch.start_jd
    );
    assert!(
        header.epoch.end_jd > 7_000_000.0,
        "end JD should be in deep future; got {}",
        header.epoch.end_jd
    );
    // Granule size is a header constant — exact equality is correct here.
    #[allow(clippy::float_cmp)]
    {
        assert_eq!(
            header.epoch.granule_days, 32.0,
            "DE-series granule size is always 32 days"
        );
    }

    // === Body layout: 15 columns, 13 active + 2 reserved zeros ===
    let layout = &header.layout;
    assert_eq!(layout.len(), 15, "DE441 layout has 15 columns");
    assert_eq!(
        layout.coeffs_per_axis[13], 0,
        "slot 14 (lunar mantle ω) is unused in DE441"
    );
    assert_eq!(
        layout.coeffs_per_axis[14], 0,
        "slot 15 (TT−TDB) is unused in DE441"
    );
    assert_eq!(layout.offsets[0], 3, "Mercury starts at word 3");
    assert_eq!(
        layout.coeffs_per_axis[0], 14,
        "Mercury uses 14 Chebyshev coefficients per axis"
    );
    assert_eq!(
        layout.subgranules[0], 4,
        "Mercury subdivides each 32-day record into 4 sub-granules"
    );
    // Total active payload should equal NCOEFF − 2 leading time-tag words.
    let total_active: u32 = (0..13)
        .map(|i| {
            let axes = if i == 11 { 2 } else { 3 }; // slot 12 = Earth nutations
            layout.record_words(i, axes).expect("slot in bounds")
        })
        .sum();
    assert_eq!(
        total_active,
        header.ncoeff - 2,
        "active body words must equal NCOEFF − 2 (two leading time-tag words)"
    );

    // === Key physical constants must be present and physically sane ===
    let au = header
        .constants
        .get("AU")
        .copied()
        .expect("AU constant must be present");
    assert!(
        (au - 149_597_870.7).abs() < 1.0,
        "AU should be ≈ 149,597,870.7 km; got {au}"
    );

    let emrat = header
        .constants
        .get("EMRAT")
        .copied()
        .expect("EMRAT constant must be present");
    assert!(
        (emrat - 81.3).abs() < 0.05,
        "Earth-Moon mass ratio should be ≈ 81.3; got {emrat}"
    );

    let clight = header
        .constants
        .get("CLIGHT")
        .copied()
        .expect("CLIGHT constant must be present");
    assert!(
        (clight - 299_792.458).abs() < 0.01,
        "speed of light should be ≈ 299,792.458 km/s; got {clight}"
    );

    // 645 named constants in DE441.
    assert_eq!(
        header.constants.len(),
        645,
        "DE441 ships exactly 645 named constants"
    );
}
