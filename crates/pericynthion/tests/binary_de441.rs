//! Integration test: open the real DE441 binary file, verify its
//! structure against the companion ASCII header, and probe a known
//! coefficient record (J2000, JD 2,451,545.0).
//!
//! Skips gracefully when `$STARCAT_JPL_DATA` is unset or points at a non-existent directory.

use pericynthion::jpl::{discover, header::parse, reader::EphemerisFile};
use std::path::PathBuf;

fn locate_dir() -> Option<PathBuf> {
    let val = std::env::var_os("STARCAT_JPL_DATA")?;
    Some(PathBuf::from(val))
}

#[test]
fn opens_real_de441_and_reports_actual_coverage() {
    let Some(dir) = locate_dir() else {
        eprintln!("STARCAT_JPL_DATA not set — skipping integration test");
        return;
    };
    let paths = discover::discover(&dir)
        .unwrap_or_else(|e| panic!("autodiscovery failed for {}: {e}", dir.display()));

    let source = std::fs::read_to_string(&paths.header).expect("read header failed");
    let header = parse(&source).expect("parse header failed");

    let ephem = EphemerisFile::open(&paths.binary, &header).expect("open binary file failed");

    // === Endianness: linux_*.441 files are little-endian ===
    assert!(
        ephem.is_little_endian(),
        "linux_*.441 files must be little-endian on x86 distribution"
    );

    // === Coverage: 342,419 coefficient records (342,421 total − 2 header) ===
    assert_eq!(
        ephem.coefficient_records(),
        342_419,
        "DE441 linux_m13000p17000.441 has exactly 342,419 coefficient records"
    );

    // === Granule size matches ASCII header ===
    assert!(
        (ephem.granule_days() - 32.0).abs() < 1e-9,
        "DE-series uses 32-day granules; got {}",
        ephem.granule_days()
    );

    // === Actual file coverage: this particular DE441 distribution
    //     (`linux_m13000p17000.441`) is clipped to JD −3,027,215.5
    //     through JD +7,930,192.5, i.e. ~13,000 BCE to ~17,000 CE.
    //     The ASCII header advertises a wider DE441 *design* range
    //     (−3,100,015.5 to +8,000,016.5). The binary file is
    //     authoritative for what is actually computable; the ASCII
    //     header describes the format, not necessarily this file's
    //     payload.
    assert!(
        (ephem.start_jd() - (-3_027_215.5)).abs() < 1e-6,
        "binary file actually starts at JD −3,027,215.5; got {}",
        ephem.start_jd()
    );
    let expected_end = ephem.start_jd() + 342_419.0 * 32.0;
    assert!(
        (ephem.end_jd() - expected_end).abs() < 1e-6,
        "end_jd should equal start_jd + n_records × granule_days; \
         got {}, expected {}",
        ephem.end_jd(),
        expected_end
    );
    assert!(
        ephem.start_jd() > header.epoch.start_jd && ephem.end_jd() < header.epoch.end_jd,
        "binary coverage [{}, {}] should sit strictly inside ASCII design \
         range [{}, {}]",
        ephem.start_jd(),
        ephem.end_jd(),
        header.epoch.start_jd,
        header.epoch.end_jd
    );
}

#[test]
fn reads_j2000_coefficient_record() {
    let Some(dir) = locate_dir() else {
        eprintln!("STARCAT_JPL_DATA not set — skipping integration test");
        return;
    };
    let paths = discover::discover(&dir)
        .unwrap_or_else(|e| panic!("autodiscovery failed for {}: {e}", dir.display()));

    let source = std::fs::read_to_string(&paths.header).unwrap();
    let header = parse(&source).unwrap();
    let ephem = EphemerisFile::open(&paths.binary, &header).unwrap();

    // J2000.0 = JD 2,451,545.0 TT (2000-01-01 12:00 TT).
    let jd_j2000 = 2_451_545.0_f64;
    let record = ephem
        .record_for_jd(jd_j2000)
        .expect("J2000 is well within DE441 coverage");

    // The record's window must contain J2000.0.
    assert!(
        record.start_jd() <= jd_j2000 && record.end_jd() >= jd_j2000,
        "record [{}, {}] does not contain JD {}",
        record.start_jd(),
        record.end_jd(),
        jd_j2000
    );
    // And its span must be exactly one granule.
    let span = record.end_jd() - record.start_jd();
    assert!((span - 32.0).abs() < 1e-9, "record span {span} ≠ 32 days");
    // First Mercury coefficient (offset 3, 1-indexed → byte offset 2)
    // of the first sub-granule. We don't check a specific value here
    // (that's the Chebyshev-evaluation test's job) but we do check it's
    // a finite, non-tiny number that wasn't accidentally byte-swapped.
    let mercury_first = record.get(2);
    assert!(mercury_first.is_finite());
    assert!(
        mercury_first.abs() > 1.0,
        "Mercury position coefficient ≈ {mercury_first} suggests bad parse"
    );
}
