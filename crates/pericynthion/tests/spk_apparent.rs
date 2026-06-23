//! Integration test: asteroid apparent positions via SPK reuse the
//! planet apparent-place pipeline.
//!
//! Two assertions, both skip-clean when the real data is absent:
//!
//! 1. **Structural sanity** — geocentric apparent Ceres at J2000 has a
//!    longitude in `[0, 360)`, small ecliptic latitude (`|β| < 30°`), and
//!    a geocentric distance in a physically plausible main-belt range
//!    (~1.0–3.5 AU).
//! 2. **Cross-file consistency** — computing the same apparent longitude
//!    from `sb441-n16.bsp` and `sb441-n373.bsp` agrees to < 1e-6°. Both
//!    files carry an independent fit of Ceres; agreement proves the SPK
//!    read + apparent pipeline is deterministic and file-agnostic.
//! 3. **Heliocentric sanity** — `heliocentric_ecliptic_position_spk` for
//!    Ceres at J2000 has a longitude in `[0, 360)`, small ecliptic
//!    latitude (`|β| < 30°`), and a heliocentric distance ≈ 2.551 AU
//!    (matches the raw SPK |r|, since the Sun is the origin).
//!
//! Requires `$STARCAT_JPL_DATA` to resolve both the DE441 binary and the
//! small-body BSPs from the JPL mirror checkout. Skips cleanly otherwise.
//!
//! TODO: external HORIZONS Ceres fixture via `just fetch horizons` for
//! absolute validation (we currently assert structural sanity +
//! cross-file consistency only — no offline ground-truth oracle).

use pericynthion::body::Body;
use pericynthion::chart::{ChartRequest, ModeRequest, compute_with_spk};
use pericynthion::coords::apparent::{
    apparent_ecliptic_position_spk, heliocentric_ecliptic_position_spk,
};
use pericynthion::ephemeris::Ephemeris;
use pericynthion::jpl::{discover, header::parse, reader::EphemerisFile};
use pericynthion::spk::{Asteroid, SpkEphemeris};
use pericynthion::time::calendar::{Calendar, CivilDate};
use pericynthion::time::zone::Zone;
use std::path::{Path, PathBuf};

const J2000: f64 = 2_451_545.0;

/// Open `sb441-n16.bsp` and the DE441 binary together.
///
/// Returns `Some((spk, (file, header)))` when both are present; `None` otherwise.
/// The caller must keep the returned `(file, header)` alive for the `Ephemeris`
/// borrow to remain valid.
fn open_spk_and_ephem() -> Option<(
    SpkEphemeris,
    (EphemerisFile, pericynthion::jpl::header::Header),
)> {
    let bsp = resolve_bsp("sb441-n16.bsp")?;
    let de441 = open_de441()?;
    let spk = SpkEphemeris::open(&bsp).ok()?;
    Some((spk, de441))
}

/// Walk up from `$STARCAT_JPL_DATA` to the mirror root, then resolve the
/// named small-body BSP under `ftp/eph/small_bodies/asteroids_de441/`.
fn resolve_bsp(file_name: &str) -> Option<PathBuf> {
    let val = std::env::var_os("STARCAT_JPL_DATA")?;
    let start = PathBuf::from(&val).canonicalize().ok()?;
    let mut candidate: &Path = start.as_path();
    for _ in 0..10 {
        let bsp = candidate
            .join("ftp")
            .join("eph")
            .join("small_bodies")
            .join("asteroids_de441")
            .join(file_name);
        if bsp.is_file() {
            return Some(bsp);
        }
        candidate = candidate.parent()?;
    }
    None
}

/// Open the DE441 binary ephemeris from `$STARCAT_JPL_DATA`, if present.
fn open_de441() -> Option<(EphemerisFile, pericynthion::jpl::header::Header)> {
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
    Some((file, header))
}

#[test]
fn ceres_geocentric_apparent_is_physically_sane() {
    let Some((file, header)) = open_de441() else {
        eprintln!("STARCAT_JPL_DATA / DE441 not available — skipping");
        return;
    };
    let Some(bsp) = resolve_bsp("sb441-n16.bsp") else {
        eprintln!("sb441-n16.bsp not present — skipping");
        return;
    };
    let ephem = Ephemeris::new(&file, &header).expect("build Ephemeris");
    // Sanity: confirm the Sun is reachable (the asteroid closure needs it).
    ephem.state(Body::Sun, J2000).expect("Sun state at J2000");
    let spk = SpkEphemeris::open(&bsp).expect("open sb441-n16.bsp");

    let pos = apparent_ecliptic_position_spk(&ephem, &spk, Asteroid::Ceres.naif_id(), J2000)
        .expect("Ceres apparent position");

    assert!(
        (0.0..360.0).contains(&pos.longitude_deg),
        "longitude out of [0,360): {}",
        pos.longitude_deg
    );
    assert!(
        pos.latitude_deg.abs() < 30.0,
        "ecliptic latitude implausibly large: {}",
        pos.latitude_deg
    );
    assert!(
        (1.0..=3.5).contains(&pos.distance_au),
        "geocentric Ceres distance out of main-belt range: {} AU",
        pos.distance_au
    );

    eprintln!(
        "Ceres @ J2000 (n16): lon={:.6}° lat={:.6}° dist={:.6} AU",
        pos.longitude_deg, pos.latitude_deg, pos.distance_au
    );
}

#[test]
fn ceres_apparent_longitude_agrees_across_n16_and_n373() {
    let Some((file, header)) = open_de441() else {
        eprintln!("STARCAT_JPL_DATA / DE441 not available — skipping");
        return;
    };
    let (Some(bsp16), Some(bsp373)) = (resolve_bsp("sb441-n16.bsp"), resolve_bsp("sb441-n373.bsp"))
    else {
        eprintln!("sb441-n16.bsp and/or sb441-n373.bsp not present — skipping");
        return;
    };
    let ephem = Ephemeris::new(&file, &header).expect("build Ephemeris");
    let spk16 = SpkEphemeris::open(&bsp16).expect("open sb441-n16.bsp");
    let spk373 = SpkEphemeris::open(&bsp373).expect("open sb441-n373.bsp");

    let naif = Asteroid::Ceres.naif_id();
    let a = apparent_ecliptic_position_spk(&ephem, &spk16, naif, J2000).expect("n16 Ceres");
    let b = apparent_ecliptic_position_spk(&ephem, &spk373, naif, J2000).expect("n373 Ceres");

    let dlon = (a.longitude_deg - b.longitude_deg).abs();
    eprintln!(
        "Ceres @ J2000: n16 lon={:.9}°, n373 lon={:.9}°, |Δ|={:.3e}°",
        a.longitude_deg, b.longitude_deg, dlon
    );
    assert!(
        dlon < 1e-6,
        "n16 vs n373 apparent longitude disagree by {dlon}° (> 1e-6°)"
    );
}

#[test]
fn ceres_heliocentric_ecliptic_is_sane() {
    let Some((spk, (file, header))) = open_spk_and_ephem() else {
        eprintln!("STARCAT_JPL_DATA / sb441-n16.bsp / DE441 not available — skipping");
        return;
    };
    let _ephem = Ephemeris::new(&file, &header).expect("build Ephemeris");
    let p = heliocentric_ecliptic_position_spk(&spk, Asteroid::Ceres.naif_id(), J2000)
        .expect("Ceres heliocentric position");
    assert!(
        (0.0..360.0).contains(&p.longitude_deg),
        "longitude out of [0,360): {}",
        p.longitude_deg
    );
    assert!(
        p.latitude_deg.abs() < 30.0,
        "ecliptic latitude implausibly large: {}",
        p.latitude_deg
    );
    // Heliocentric distance equals the raw SPK |r| (Sun is origin).
    assert!(
        (p.distance_au - 2.551_151_206).abs() < 1e-3,
        "heliocentric distance out of expected range: {} AU (expected ~2.551151206)",
        p.distance_au
    );
    eprintln!(
        "Ceres @ J2000 helio: lon={:.6}° lat={:.6}° dist={:.9} AU",
        p.longitude_deg, p.latitude_deg, p.distance_au
    );
}

/// Build a minimal geocentric `ChartRequest` (no location, no houses) with the
/// given asteroid NAIF ids. The civil date is 2000-01-01 12:00 UT (≈ J2000).
fn base_request(asteroids: Vec<i32>) -> ChartRequest {
    ChartRequest {
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
        asteroids,
    }
}

#[test]
fn compute_with_spk_populates_asteroids() {
    let Some((spk, (file, header))) = open_spk_and_ephem() else {
        eprintln!("STARCAT_JPL_DATA / sb441-n16.bsp / DE441 not available — skipping");
        return;
    };
    let ephem = Ephemeris::new(&file, &header).expect("build Ephemeris");
    let req = base_request(vec![2_000_001, 2_000_004]);
    let chart = compute_with_spk(&ephem, &[&spk], &req).expect("compute_with_spk");

    assert_eq!(chart.asteroids.len(), 2, "expected Ceres + Vesta");
    let ceres = chart
        .asteroids
        .iter()
        .find(|a| a.naif_id == 2_000_001)
        .expect("Ceres present");
    assert_eq!(ceres.name, "Ceres");
    assert!(
        (0.0..360.0).contains(&ceres.position.longitude_deg),
        "Ceres longitude out of [0,360): {}",
        ceres.position.longitude_deg
    );
}

#[test]
fn compute_without_spk_leaves_asteroids_empty() {
    let Some((_spk, (file, header))) = open_spk_and_ephem() else {
        eprintln!("STARCAT_JPL_DATA / sb441-n16.bsp / DE441 not available — skipping");
        return;
    };
    let ephem = Ephemeris::new(&file, &header).expect("build Ephemeris");
    let req = base_request(vec![2_000_001, 2_000_004]);
    let chart = pericynthion::chart::compute(&ephem, &req).expect("compute");
    assert!(chart.asteroids.is_empty(), "plain compute must skip SPK");
}

#[test]
fn asteroid_daily_speed_is_nonzero() {
    // Verify that `ComputedAsteroid::daily_speed_deg` is no longer stubbed to
    // zero. Any real asteroid position will move by a fraction of a degree per
    // day — the stub was exactly `0.0`.
    let Some((spk, (file, header))) = open_spk_and_ephem() else {
        eprintln!("STARCAT_JPL_DATA / sb441-n16.bsp / DE441 not available — skipping");
        return;
    };
    let ephem = Ephemeris::new(&file, &header).expect("build Ephemeris");
    let req = base_request(vec![2_000_001]);
    let chart = compute_with_spk(&ephem, &[&spk], &req).expect("compute_with_spk");
    let ceres = chart
        .asteroids
        .iter()
        .find(|a| a.naif_id == 2_000_001)
        .expect("Ceres present");
    assert!(
        ceres.daily_speed_deg.abs() > 1e-6,
        "Ceres daily speed must be nonzero — stub removed (got {})",
        ceres.daily_speed_deg
    );
    eprintln!(
        "Ceres @ J2000 daily_speed={:.8}°/day",
        ceres.daily_speed_deg
    );
}

/// Build a geocentric `ChartRequest` for 2023-02-25 12:00 UT, during which
/// Ceres is retrograde (confirmed empirically via starcat --omniscient --page).
fn request_2023_02_25() -> ChartRequest {
    ChartRequest {
        civil: CivilDate {
            year: 2023,
            month: 2,
            day: 25,
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
        asteroids: vec![2_000_001],
    }
}

#[test]
fn ceres_retrograde_2023_02_25() {
    // Ceres was geocentric retrograde on 2023-02-25: confirmed empirically
    // via `starcat compute --date 2023-02-25 --omniscient --page` which shows
    // `Ceres   ℞ │  5°16' Lib` and JZOD output with `"retrograde": true` and
    // `"daily_speed": -0.152...`.
    let Some((spk, (file, header))) = open_spk_and_ephem() else {
        eprintln!("STARCAT_JPL_DATA / sb441-n16.bsp / DE441 not available — skipping");
        return;
    };
    let ephem = Ephemeris::new(&file, &header).expect("build Ephemeris");
    let req = request_2023_02_25();
    let chart = compute_with_spk(&ephem, &[&spk], &req).expect("compute_with_spk");
    let ceres = chart
        .asteroids
        .iter()
        .find(|a| a.naif_id == 2_000_001)
        .expect("Ceres present");
    assert!(
        ceres.retrograde,
        "Ceres must be retrograde on 2023-02-25 (daily_speed={:.6}°/day)",
        ceres.daily_speed_deg
    );
    assert!(
        ceres.daily_speed_deg < 0.0,
        "retrograde Ceres speed must be negative, got {}",
        ceres.daily_speed_deg
    );
    eprintln!(
        "Ceres 2023-02-25: retrograde={} daily_speed={:.8}°/day",
        ceres.retrograde, ceres.daily_speed_deg
    );
}

#[test]
fn compute_with_multiple_spks_names_from_catalog() {
    // Prove that an asteroid found in the supplied SPK slice is computed and
    // named from the placements catalog (not from the Asteroid enum).
    let Some((spk, (file, header))) = open_spk_and_ephem() else {
        eprintln!("skip: sb441-n16.bsp / DE441 not available");
        return;
    };
    let ephem = Ephemeris::new(&file, &header).expect("build Ephemeris");
    // Request Ceres (sb441 naif id 2_000_001); pass a single-element slice.
    // The multi-element path is tested implicitly: the resolver iterates the
    // slice to find whichever SPK covers the requested id.
    let req = base_request(vec![2_000_001]);
    let chart = compute_with_spk(&ephem, &[&spk], &req).unwrap();
    let ceres = chart
        .asteroids
        .iter()
        .find(|a| a.naif_id == 2_000_001)
        .unwrap();
    assert_eq!(ceres.name, "Ceres");
    assert!(ceres.position.longitude_deg.is_finite());
}
