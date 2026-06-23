//! Absolute HORIZONS validation of SPK asteroid apparent positions.
//!
//! Loads the cached HORIZONS geocentric fixture for the J2000 epoch and
//! compares each asteroid's apparent ecliptic-of-date longitude and latitude
//! against `apparent_ecliptic_position_spk` — the same pipeline used for
//! real chart computation.
//!
//! **UT → TT conversion** follows `acceptance_horizons.rs` exactly:
//! `civil_to_jd` (Gregorian calendar) → `jd_ut_to_jd_tt` (pericynthion ΔT).
//!
//! Skip conditions (all skip-clean, no panic):
//! - `$STARCAT_JPL_DATA` unset
//! - fixture absent
//! - `sb441-n16.bsp` not reachable from the mirror root
//! - DE441 binary not present
//!
//! Hygiea (NAIF 2000010) is in `sb441-n373.bsp`, *not* `sb441-n16.bsp`.
//! If `spk.state(naif, et)` errors for a body, the body is skipped with an
//! `eprintln!` — it does not fail the test.
//!
//! Ceres, Pallas, Juno, and Vesta are asserted (all four are in n16).

// jd_ut/jd_tt, max_dlon/max_dlat naming mirrors acceptance_horizons.rs.
#![allow(clippy::similar_names)]

use pericynthion::coords::apparent::apparent_ecliptic_position_spk;
use pericynthion::ephemeris::Ephemeris;
use pericynthion::jpl::{discover, header::parse as parse_header, reader::EphemerisFile};
use pericynthion::spk::{Asteroid, SpkEphemeris};
use pericynthion::time::calendar::{Calendar, CivilDate, civil_to_jd};
use pericynthion::time::delta_t::jd_ut_to_jd_tt;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

// =============================================================================
// Fixture type (reuses the same JSON schema as acceptance_horizons.rs)
// =============================================================================

#[derive(Debug, Deserialize)]
struct HorizonsFixture {
    iso_ut: String,
    bodies: BTreeMap<String, BodyValue>,
}

#[derive(Debug, Deserialize)]
struct BodyValue {
    longitude_deg: f64,
    latitude_deg: f64,
}

// =============================================================================
// Helpers — identical to acceptance_horizons.rs
// =============================================================================

fn longitude_delta_deg(a: f64, b: f64) -> f64 {
    let raw = (a - b).abs().rem_euclid(360.0);
    raw.min(360.0 - raw)
}

fn arcseconds(deg: f64) -> f64 {
    deg * 3600.0
}

fn parse_iso_ut(iso: &str) -> CivilDate {
    let parts: Vec<&str> = iso.split_whitespace().collect();
    let date_parts: Vec<&str> = parts[0].split('-').collect();
    let time_parts: Vec<&str> = parts[1].split(':').collect();
    CivilDate {
        year: date_parts[0].parse().unwrap(),
        month: date_parts[1].parse().unwrap(),
        day: date_parts[2].parse().unwrap(),
        hour: time_parts[0].parse().unwrap(),
        minute: time_parts[1].parse().unwrap(),
        second: time_parts[2].parse().unwrap_or(0.0),
    }
}

/// Walk up from `$STARCAT_JPL_DATA` to the mirror root, then resolve the
/// named small-body BSP under `ftp/eph/small_bodies/asteroids_de441/`.
/// Mirrors the same logic in `spk_apparent.rs`.
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

fn locate_jpl_paths() -> Option<(PathBuf, PathBuf)> {
    let dir = std::env::var("STARCAT_JPL_DATA").ok().map(PathBuf::from)?;
    let loc = discover::locate(&dir).ok()?;
    let paths = match loc {
        discover::DatasetLocation::Binary(p) => p,
        discover::DatasetLocation::Ascii { .. } => return None,
    };
    Some((paths.header, paths.binary))
}

// =============================================================================
// Test
// =============================================================================

#[test]
fn spk_asteroid_positions_match_horizons_j2000() {
    // ── skip-clean guards ────────────────────────────────────────────────────

    let Some((header_path, binary_path)) = locate_jpl_paths() else {
        eprintln!("STARCAT_JPL_DATA not set — skipping spk_horizons test");
        return;
    };

    let fixture_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/horizons_asteroids_j2000_geo.json");
    if !fixture_path.exists() {
        eprintln!(
            "fixture absent: {} — skipping (run: scripts/horizons_fetch.py asteroids_j2000 --mode geocentric)",
            fixture_path.display()
        );
        return;
    }

    let Some(bsp_path) = resolve_bsp("sb441-n16.bsp") else {
        eprintln!("sb441-n16.bsp not found — skipping spk_horizons test");
        return;
    };

    // ── open ephemerides ─────────────────────────────────────────────────────

    let header_src = std::fs::read_to_string(&header_path).unwrap();
    let header = parse_header(&header_src).unwrap();
    let file = EphemerisFile::open(&binary_path, &header).unwrap();
    let ephem = Ephemeris::new(&file, &header).unwrap();

    let spk = SpkEphemeris::open(&bsp_path).expect("open sb441-n16.bsp");

    // ── UT → TT (identical to acceptance_horizons.rs) ────────────────────────

    let fixture_text = std::fs::read_to_string(&fixture_path).unwrap();
    let fixture: HorizonsFixture = serde_json::from_str(&fixture_text).unwrap();

    let civil = parse_iso_ut(&fixture.iso_ut);
    let jd_ut = civil_to_jd(civil, Calendar::Gregorian);
    let jd_tt = jd_ut_to_jd_tt(jd_ut);

    // ── per-body comparison ──────────────────────────────────────────────────

    // Tolerance: 1 arcsec. Observed residuals at J2000 are ≤ 0.084″ (max Δlat
    // for Juno). This is ~10× the actual worst-case delta — tight enough to
    // catch regressions but with comfortable headroom over HORIZONS quantisation.
    const TOL_ARCSEC: f64 = 1.0;

    let mut max_dlon = 0.0_f64;
    let mut max_dlat = 0.0_f64;
    let mut worst_body = String::new();
    let mut bodies_asserted = 0usize;

    println!("=== spk_horizons  JD_UT={jd_ut:.6}  JD_TT={jd_tt:.6} ===",);

    for (name, horizons) in &fixture.bodies {
        let Some(asteroid) = Asteroid::from_slug(name) else {
            eprintln!("  {name}: no matching Asteroid variant — skipping");
            continue;
        };
        let naif = asteroid.naif_id();

        // Probe coverage: Hygiea is in n373, not n16 — skip gracefully.
        let et = (jd_tt - 2_451_545.0) * 86_400.0;
        if spk.state(naif, et).is_err() {
            eprintln!(
                "  {name} (NAIF {naif}): not covered by sb441-n16.bsp at this epoch — skipping"
            );
            continue;
        }

        let pos = apparent_ecliptic_position_spk(&ephem, &spk, naif, jd_tt)
            .unwrap_or_else(|e| panic!("{name}: apparent_ecliptic_position_spk failed: {e}"));

        let dlon_arcsec = arcseconds(longitude_delta_deg(
            pos.longitude_deg,
            horizons.longitude_deg,
        ));
        let dlat_arcsec = arcseconds((pos.latitude_deg - horizons.latitude_deg).abs());

        println!(
            "  {name:<8}  lon={:>10.4}°  HORIZONS={:>10.4}°  Δlon={:>7.3}″  \
             lat={:>+9.4}°  HORIZONS={:>+9.4}°  Δlat={:>7.3}″  (tol {TOL_ARCSEC:.0}″)",
            pos.longitude_deg,
            horizons.longitude_deg,
            dlon_arcsec,
            pos.latitude_deg,
            horizons.latitude_deg,
            dlat_arcsec,
        );

        if dlon_arcsec > max_dlon {
            max_dlon = dlon_arcsec;
            worst_body.clone_from(name);
        }
        max_dlat = max_dlat.max(dlat_arcsec);

        assert!(
            dlon_arcsec < TOL_ARCSEC,
            "{name}: longitude Δ vs HORIZONS {dlon_arcsec:.2}″ exceeds tolerance {TOL_ARCSEC}″"
        );
        assert!(
            dlat_arcsec < TOL_ARCSEC,
            "{name}: latitude Δ vs HORIZONS {dlat_arcsec:.2}″ exceeds tolerance {TOL_ARCSEC}″"
        );
        bodies_asserted += 1;
    }

    println!(
        "  → {bodies_asserted} bodies asserted  max Δlon: {max_dlon:.3}″ ({worst_body})  max Δlat: {max_dlat:.3}″"
    );

    assert!(
        bodies_asserted >= 4,
        "expected at least 4 asteroid assertions (Ceres/Pallas/Juno/Vesta), got {bodies_asserted}"
    );
}
