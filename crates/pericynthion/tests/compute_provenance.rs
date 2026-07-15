//! Integration test: [`pericynthion::chart::compute_with_spk`] records observed
//! data-source provenance for both the DE binary (planets) and an SPK asteroid.
//!
//! Resolves data via the crate's standard discovery — `$STARCAT_JPL_DATA`
//! (walked with [`discover::locate`] for the DE binary, and the mirror-relative
//! `small_bodies/asteroids_de441/sb441-n16.bsp` path for the asteroid SPK, the
//! same pattern used by `tests/spk_apparent.rs`). Skips cleanly, printing why,
//! when the data is genuinely absent — never hard-codes a specimens path.

use pericynthion::body::Body;
use pericynthion::chart::{ChartRequest, ModeRequest, compute_with_spk};
use pericynthion::ephemeris::Ephemeris;
use pericynthion::jpl::{discover, header::parse, reader::EphemerisFile};
use pericynthion::spk::SpkEphemeris;
use pericynthion::time::calendar::{Calendar, CivilDate};
use pericynthion::time::zone::Zone;
use std::path::{Path, PathBuf};

/// Open the DE441 binary ephemeris from `$STARCAT_JPL_DATA` via the crate's
/// standard discovery, returning the file, its header, and the resolved
/// binary path. `None` when `$STARCAT_JPL_DATA` is unset or does not resolve
/// to a binary dataset.
fn open_de441() -> Option<(EphemerisFile, pericynthion::jpl::header::Header, PathBuf)> {
    let val = std::env::var_os("STARCAT_JPL_DATA")?;
    let dir = PathBuf::from(val);
    let loc = discover::locate(&dir).ok()?;
    let paths = match loc {
        discover::DatasetLocation::Binary(p) => p,
        discover::DatasetLocation::Ascii { .. } => return None,
    };
    let source = std::fs::read_to_string(&paths.header).ok()?;
    let header = parse(&source).ok()?;
    let file = EphemerisFile::open(&paths.binary, &header).ok()?;
    Some((file, header, paths.binary))
}

/// Walk up from `$STARCAT_JPL_DATA` to the mirror root, then resolve
/// `ftp/eph/small_bodies/asteroids_de441/sb441-n16.bsp` — the same resolution
/// `tests/spk_apparent.rs` uses.
fn resolve_sb441_n16() -> Option<PathBuf> {
    let val = std::env::var_os("STARCAT_JPL_DATA")?;
    let start = PathBuf::from(&val).canonicalize().ok()?;
    let mut candidate: &Path = start.as_path();
    for _ in 0..10 {
        let bsp = candidate
            .join("ftp")
            .join("eph")
            .join("small_bodies")
            .join("asteroids_de441")
            .join("sb441-n16.bsp");
        if bsp.is_file() {
            return Some(bsp);
        }
        candidate = candidate.parent()?;
    }
    None
}

#[test]
fn compute_records_observed_provenance() {
    let (Some((file, header, de_path)), Some(bsp_path)) = (open_de441(), resolve_sb441_n16())
    else {
        eprintln!(
            "skip: need $STARCAT_JPL_DATA (DE441 binary + small_bodies/asteroids_de441/sb441-n16.bsp)"
        );
        return;
    };

    let ephem = Ephemeris::new(&file, &header)
        .expect("build Ephemeris")
        .with_source_path(de_path.clone());
    let spk = SpkEphemeris::open(&bsp_path).expect("open sb441-n16.bsp");

    let req = ChartRequest {
        civil: CivilDate {
            year: 2000,
            month: 1,
            day: 1,
            hour: 12,
            minute: 0,
            second: 0.0,
        },
        calendar: Calendar::Gregorian,
        zone: Zone::FixedSeconds(0),
        mode: ModeRequest::Geocentric,
        lat_deg: None,
        lon_deg: None,
        bodies: None,
        houses: Vec::new(),
        // Ceres (dwarf planet; sb441 id scheme) + Pallas (Category::Asteroid).
        asteroids: vec![2_000_001, 2_000_002],
    };

    let computed = compute_with_spk(&ephem, &[&spk], &req, &[]).expect("compute_with_spk");

    // Sanity: the chart actually pulled planets + both asteroids.
    assert!(computed.bodies.iter().any(|b| b.body == Body::Sun));
    assert_eq!(computed.asteroids.len(), 2, "expected Ceres + Pallas");

    assert!(
        computed
            .provenance
            .iter()
            .any(|u| u.key == "planets" && u.path == de_path),
        "expected a planets entry naming the DE binary path; got {:?}",
        computed.provenance
    );
    assert!(
        computed
            .provenance
            .iter()
            .any(|u| u.path.to_string_lossy().contains("sb441")),
        "expected an entry naming the SPK that served Ceres; got {:?}",
        computed.provenance
    );
    // Ceres is Category::DwarfPlanet, which has no dedicated provenance
    // category key — it is keyed by its own display name instead.
    assert!(
        computed.provenance.iter().any(|u| u.key == "Ceres"),
        "expected the Ceres entry keyed by its own body name; got {:?}",
        computed.provenance
    );
    // Pallas is Category::Asteroid — its provenance is keyed by category.
    assert!(
        computed.provenance.iter().any(|u| u.key == "asteroids"),
        "expected the Pallas entry keyed by category 'asteroids'; got {:?}",
        computed.provenance
    );

    eprintln!("provenance: {:?}", computed.provenance);
}
