//! Acceptance test: starcat vs **NASA HORIZONS** for each reference chart.
//!
//! For each chart we compute the apparent ecliptic-of-date longitude and
//! latitude for every body in the v1 set and compare against HORIZONS
//! cached fixtures.
//!
//! HORIZONS is the source of record. The refchart oracle (which carries
//! its own ΔT model) lives in `acceptance_refchart.rs` and is asserted
//! independently there.
//!
//! Fixture families:
//! - `horizons_{chart}_geo.json` — geocentric (lon/lat)
//! - `horizons_{chart}_topo.json` — topocentric (lon/lat, observer location embedded)
//! - `horizons_{chart}_helio.json` — heliocentric VECTORS (`x_au`/`y_au`/`z_au`,
//!   J2000/ICRF, geometric/no-LT). Tests apply our precession+nutation to
//!   rotate to ecliptic-of-date and diff against `heliocentric_ecliptic_position`.

// jd_ut/jd_tt, max_dlon/max_dlat, etc. are semantically distinct test-only
// bindings — collisions only show up to clippy as a name-length similarity.
#![allow(clippy::similar_names)]
// VectorsBodyValue is a literal HORIZONS payload: x_au/y_au/z_au is the wire
// format. Stripping the suffix would lose the unit cue.
#![allow(clippy::struct_field_names)]

use pericynthion::body::Body;
use pericynthion::coords::apparent::{
    apparent_ecliptic_position, apparent_ecliptic_position_topocentric,
    heliocentric_ecliptic_position,
};
use pericynthion::coords::topocentric::ObserverLocation;
use pericynthion::ephemeris::Ephemeris;
use pericynthion::jpl::{discover, header::parse as parse_header, reader::EphemerisFile};
use pericynthion::time::calendar::{Calendar, CivilDate, civil_to_jd};
use pericynthion::time::delta_t::jd_ut_to_jd_tt;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::PathBuf;

// =============================================================================
// HORIZONS fixture loader
// =============================================================================

#[derive(Debug, Deserialize)]
struct HorizonsFixture {
    iso_ut: String,
    bodies: BTreeMap<String, BodyValue>,
}

/// Observer metadata embedded in topocentric fixtures.
#[derive(Debug, Deserialize)]
struct FixtureObserver {
    lat_deg: f64,
    lon_e_deg: f64,
    elev_km: f64,
}

/// Topocentric fixture includes the observer location used for the HORIZONS fetch.
#[derive(Debug, Deserialize)]
struct HorizonsTopoFixture {
    iso_ut: String,
    bodies: BTreeMap<String, BodyValue>,
    observer: FixtureObserver,
}

/// Heliocentric VECTORS fixture: geometric J2000/ICRF position in AU.
#[derive(Debug, Deserialize)]
struct VectorsBodyValue {
    x_au: f64,
    y_au: f64,
    z_au: f64,
}

#[derive(Debug, Deserialize)]
struct HorizonsVectorsFixture {
    iso_ut: String,
    bodies: BTreeMap<String, VectorsBodyValue>,
}

#[derive(Debug, Deserialize)]
struct BodyValue {
    longitude_deg: f64,
    latitude_deg: f64,
}

/// Per-chart calendar context. HORIZONS interprets numeric dates by
/// Western-European convention (Julian before 1582-10-15, Gregorian
/// after), but for our purposes the calendar is locked per-chart by
/// what the original source records say.
fn calendar_for_chart(chart_id: &str) -> Calendar {
    match chart_id {
        // Year 120 CE: pre-Gregorian-reform, recorded as Julian.
        "vettius_valens" => Calendar::Julian,
        _ => Calendar::Gregorian,
    }
}

/// Longitude-aware angular distance in degrees, accounting for the
/// 0°/360° seam.
fn longitude_delta_deg(a: f64, b: f64) -> f64 {
    let raw = (a - b).abs().rem_euclid(360.0);
    raw.min(360.0 - raw)
}

fn arcseconds(deg: f64) -> f64 {
    deg * 3600.0
}

/// ANSI-color a Δ value: green within tol, red over. No-op on non-TTY.
fn dlt(d: f64, tol: f64) -> (&'static str, &'static str) {
    use std::io::IsTerminal;
    if !std::io::stdout().is_terminal() {
        return ("", "");
    }
    if d < tol {
        ("\x1b[32m", "\x1b[0m")
    } else {
        ("\x1b[31m", "\x1b[0m")
    }
}

fn locate_jpl_paths() -> Option<(PathBuf, PathBuf)> {
    let dir = std::env::var("STARCAT_JPL_DATA").ok().map(PathBuf::from)?;
    let loc =
        discover::locate(&dir).unwrap_or_else(|e| panic!("STARCAT_JPL_DATA locate failed: {e}"));
    let paths = match loc {
        discover::DatasetLocation::Binary(p) => p,
        discover::DatasetLocation::Ascii { .. } => {
            panic!("expected binary DE dataset under {}", dir.display())
        }
    };
    Some((paths.header, paths.binary))
}

fn body_from_name(name: &str) -> Option<Body> {
    Some(match name {
        "Sun" => Body::Sun,
        "Moon" => Body::Moon,
        "Mercury" => Body::Mercury,
        "Venus" => Body::Venus,
        "Mars" => Body::Mars,
        "Jupiter" => Body::Jupiter,
        "Saturn" => Body::Saturn,
        "Uranus" => Body::Uranus,
        "Neptune" => Body::Neptune,
        "Pluto" => Body::Pluto,
        _ => return None,
    })
}

fn parse_iso_ut(iso: &str) -> CivilDate {
    // Format: YYYY-MM-DD HH:MM:SS
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

/// Per-body tolerances for one chart, in arcseconds. Sized to absorb model
/// drift at the chart's epoch with ~1.5–10× headroom over the worst observed
/// residual after the Moon-aberration fix.
fn tolerances(chart_id: &str, body: Body) -> (f64, f64) {
    match (chart_id, body) {
        // Modern (1895/1955/1989): Moon ≤ 1.4″, planets ≤ 0.5″ observed.
        ("lightning_strike" | "anna_freud" | "adele_haenel", Body::Moon) => (5.0, 5.0),
        ("lightning_strike" | "anna_freud" | "adele_haenel", _) => (2.0, 2.0),
        // William Lilly 1602: SMH 2016 ΔT (86 s). Observed Moon 9.2″, planets ≤ 2.7″.
        ("william_lilly", Body::Moon) => (15.0, 15.0),
        ("william_lilly", _) => (5.0, 5.0),
        // Vettius Valens 120 CE: SMH 2016 ΔT (~9356 s). Observed Moon 68.9″ —
        // we're at the model-accuracy floor here; do not tighten further.
        ("vettius_valens", Body::Moon) => (90.0, 10.0),
        ("vettius_valens", _) => (15.0, 15.0),
        _ => (60.0, 60.0),
    }
}

fn run_chart(chart_id: &str) {
    let Some((header_path, binary_path)) = locate_jpl_paths() else {
        eprintln!("STARCAT_JPL_DATA not set — skipping integration test");
        return;
    };
    let fixture_path: PathBuf = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(format!("horizons_{chart_id}_geo.json"));
    assert!(
        fixture_path.exists(),
        "missing fixture: {}",
        fixture_path.display()
    );
    let fixture_text = std::fs::read_to_string(&fixture_path).unwrap();
    let fixture: HorizonsFixture = serde_json::from_str(&fixture_text).unwrap();

    let calendar = calendar_for_chart(chart_id);
    let civil = parse_iso_ut(&fixture.iso_ut);
    let jd_ut = civil_to_jd(civil, calendar);
    let jd_tt = jd_ut_to_jd_tt(jd_ut);

    let header_src = std::fs::read_to_string(&header_path).unwrap();
    let header = parse_header(&header_src).unwrap();
    let file = EphemerisFile::open(&binary_path, &header).unwrap();
    let ephem = Ephemeris::new(&file, &header).unwrap();

    let mut max_delta_lon = 0.0_f64;
    let mut max_delta_lat = 0.0_f64;
    let mut worst = String::new();
    let mut bodies_checked = 0;

    println!("=== {chart_id}  JD_UT={jd_ut:.6}  JD_TT={jd_tt:.6} ===");
    for (name, horizons) in &fixture.bodies {
        let Some(body) = body_from_name(name) else {
            continue;
        };
        let pos = apparent_ecliptic_position(&ephem, body, jd_tt).unwrap();
        let dlon = arcseconds(longitude_delta_deg(
            pos.longitude_deg,
            horizons.longitude_deg,
        ));
        let dlat = arcseconds((pos.latitude_deg - horizons.latitude_deg).abs());
        let (tol_lon, tol_lat) = tolerances(chart_id, body);

        let (c, r) = dlt(dlon, tol_lon);
        println!(
            "  {name:<8} starcat lon={:>10.4}°  HORIZONS={:>10.4}°  Δlon: {c}{:>7.2}{r}″  (tol {:.0}″)",
            pos.longitude_deg, horizons.longitude_deg, dlon, tol_lon
        );

        if dlon > max_delta_lon {
            max_delta_lon = dlon;
            worst.clone_from(name);
        }
        max_delta_lat = max_delta_lat.max(dlat);

        assert!(
            dlon < tol_lon,
            "{chart_id}/{name}: longitude Δ vs HORIZONS {dlon:.2}″ exceeds tolerance {tol_lon}″"
        );
        assert!(
            dlat < tol_lat,
            "{chart_id}/{name}: latitude Δ vs HORIZONS {dlat:.2}″ exceeds tolerance {tol_lat}″"
        );
        bodies_checked += 1;
    }
    println!(
        "  → {bodies_checked} bodies checked  max Δlon: {max_delta_lon:.2}″ ({worst})  max Δlat: {max_delta_lat:.2}″"
    );
}

#[test]
fn vettius_valens_120_ce() {
    run_chart("vettius_valens");
}

#[test]
fn william_lilly_1602() {
    run_chart("william_lilly");
}

#[test]
fn horizons_lightning_strike() {
    run_chart("lightning_strike");
}

#[test]
fn horizons_anna_freud() {
    run_chart("anna_freud");
}

#[test]
fn horizons_adele_haenel() {
    run_chart("adele_haenel");
}

// =============================================================================
// Topocentric acceptance tests
// =============================================================================
//
// Compares apparent_ecliptic_position_topocentric against HORIZONS quantity 31
// fetched with CENTER='coord@399' at the chart's birth-place coordinates.
// Observer location is embedded in each `horizons_*_topo.json` fixture.
// Tolerance bands are the same as geocentric — the comparison is topo vs topo,
// so model accuracy governs, not parallax magnitude.

fn run_chart_topocentric(chart_id: &str) {
    let Some((header_path, binary_path)) = locate_jpl_paths() else {
        eprintln!("STARCAT_JPL_DATA not set — skipping integration test");
        return;
    };
    let fixture_path: PathBuf = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(format!("horizons_{chart_id}_topo.json"));
    assert!(
        fixture_path.exists(),
        "missing fixture: {}  (run: scripts/horizons_fetch.py {chart_id} --mode topocentric)",
        fixture_path.display()
    );
    let fixture_text = std::fs::read_to_string(&fixture_path).unwrap();
    let fixture: HorizonsTopoFixture = serde_json::from_str(&fixture_text).unwrap();

    let obs = ObserverLocation {
        lat_deg: fixture.observer.lat_deg,
        lon_deg: fixture.observer.lon_e_deg,
        elev_m: fixture.observer.elev_km * 1_000.0,
    };

    let calendar = calendar_for_chart(chart_id);
    let civil = parse_iso_ut(&fixture.iso_ut);
    let jd_ut = civil_to_jd(civil, calendar);
    let jd_tt = jd_ut_to_jd_tt(jd_ut);

    let header_src = std::fs::read_to_string(&header_path).unwrap();
    let header = parse_header(&header_src).unwrap();
    let file = EphemerisFile::open(&binary_path, &header).unwrap();
    let ephem = Ephemeris::new(&file, &header).unwrap();

    let mut max_dlon = 0.0_f64;
    let mut max_dlat = 0.0_f64;
    let mut worst = String::new();
    let mut bodies_checked = 0;

    println!(
        "=== topocentric {chart_id}  JD_TT={jd_tt:.6}  lat={:.4} lon={:.4} ===",
        obs.lat_deg, obs.lon_deg
    );
    for (name, horizons) in &fixture.bodies {
        let Some(body) = body_from_name(name) else {
            continue;
        };
        let pos = apparent_ecliptic_position_topocentric(&ephem, body, jd_tt, &obs).unwrap();
        let dlon = arcseconds(longitude_delta_deg(
            pos.longitude_deg,
            horizons.longitude_deg,
        ));
        let dlat = arcseconds((pos.latitude_deg - horizons.latitude_deg).abs());
        let (tol_lon, tol_lat) = tolerances(chart_id, body);

        let (c, r) = dlt(dlon, tol_lon);
        println!(
            "  {name:<8} topo lon={:>10.4}°  HORIZONS={:>10.4}°  Δlon: {c}{:>7.2}{r}″  (tol {:.0}″)",
            pos.longitude_deg, horizons.longitude_deg, dlon, tol_lon
        );

        if dlon > max_dlon {
            max_dlon = dlon;
            worst.clone_from(name);
        }
        max_dlat = max_dlat.max(dlat);

        assert!(
            dlon < tol_lon,
            "{chart_id}/topo/{name}: Δlon vs HORIZONS {dlon:.2}″ exceeds {tol_lon}″"
        );
        assert!(
            dlat < tol_lat,
            "{chart_id}/topo/{name}: Δlat vs HORIZONS {dlat:.2}″ exceeds {tol_lat}″"
        );
        bodies_checked += 1;
    }
    println!(
        "  → {bodies_checked} bodies checked  max Δlon: {max_dlon:.2}″ ({worst})  max Δlat: {max_dlat:.2}″"
    );
}

#[test]
fn vettius_valens_120_ce_topocentric() {
    run_chart_topocentric("vettius_valens");
}

#[test]
fn william_lilly_1602_topocentric() {
    run_chart_topocentric("william_lilly");
}

#[test]
fn horizons_lightning_strike_topocentric() {
    run_chart_topocentric("lightning_strike");
}

#[test]
fn horizons_anna_freud_topocentric() {
    run_chart_topocentric("anna_freud");
}

#[test]
fn horizons_adele_haenel_topocentric() {
    run_chart_topocentric("adele_haenel");
}

// =============================================================================
// Heliocentric VECTORS oracle — UNIX 32-bit overflow (2038-01-19 03:14:07 UTC)
// =============================================================================
//
// HORIZONS VECTORS endpoint (EPHEM_TYPE='VECTORS', CENTER='500@10', VEC_CORR='NONE',
// REF_PLANE='FRAME') gives geometric heliocentric J2000/ICRF Cartesian vectors in AU.
// We apply our own precession+nutation pipeline to rotate them to ecliptic-of-date,
// then diff against heliocentric_ecliptic_position. The two pipelines share the same
// rotation, so residual error comes from DE441 interpolation differences between our
// Chebyshev reader and HORIZONS's reader.
//
// Tolerances:
//   Earth/Moon: 30″ — HORIZONS body 399 (Earth geocenter) vs our Body::Earth derivation.
//   Mercury/Venus: 10″ — inner planets move fast; interpolation differences accumulate.
//   Outer planets: 5″.

fn run_chart_heliocentric_vectors(chart_id: &str) {
    use pericynthion::coords::nutation::{nutate_mean_to_true, nutation};
    use pericynthion::coords::obliquity::mean_obliquity_rad;
    use pericynthion::coords::precession::precess_j2000_to_date;
    use pericynthion::coords::transform::{equatorial_to_ecliptic, latitude_rad, longitude_rad};

    let Some((header_path, binary_path)) = locate_jpl_paths() else {
        eprintln!("STARCAT_JPL_DATA not set — skipping integration test");
        return;
    };
    let fixture_path: PathBuf = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(format!("horizons_{chart_id}_helio.json"));
    assert!(
        fixture_path.exists(),
        "missing fixture: {}  (run: scripts/horizons_fetch.py {chart_id} --mode heliocentric)",
        fixture_path.display()
    );
    let fixture_text = std::fs::read_to_string(&fixture_path).unwrap();
    let fixture: HorizonsVectorsFixture = serde_json::from_str(&fixture_text).unwrap();

    let civil = parse_iso_ut(&fixture.iso_ut);
    let jd_ut = civil_to_jd(civil, Calendar::Gregorian);
    let jd_tt = jd_ut_to_jd_tt(jd_ut);

    let header_src = std::fs::read_to_string(&header_path).unwrap();
    let header = parse_header(&header_src).unwrap();
    let file = EphemerisFile::open(&binary_path, &header).unwrap();
    let ephem = Ephemeris::new(&file, &header).unwrap();

    let eps_mean = mean_obliquity_rad(jd_tt);
    let nut = nutation(jd_tt);
    let eps_true = eps_mean + nut.delta_epsilon;

    let mut max_dlon = 0.0_f64;
    let mut worst = String::new();
    let mut bodies_checked = 0;

    println!("=== heliocentric VECTORS {chart_id}  JD_TT={jd_tt:.6} ===");
    for (name, vec_val) in &fixture.bodies {
        let Some(body) = body_from_name(name) else {
            continue;
        };
        if body == Body::Sun {
            continue;
        }

        let j2000 = [vec_val.x_au, vec_val.y_au, vec_val.z_au];
        let mean_of_date = precess_j2000_to_date(&j2000, jd_tt);
        let true_of_date = nutate_mean_to_true(&mean_of_date, jd_tt, eps_mean);
        let ecliptic = equatorial_to_ecliptic(&true_of_date, eps_true);
        let vec_lon_deg = longitude_rad(&ecliptic).to_degrees();
        let vec_lat_deg = latitude_rad(&ecliptic).to_degrees();

        let pos = heliocentric_ecliptic_position(&ephem, body, jd_tt).unwrap();
        let dlon = arcseconds(longitude_delta_deg(pos.longitude_deg, vec_lon_deg));
        let dlat = arcseconds((pos.latitude_deg - vec_lat_deg).abs());

        let (tol_lon, tol_lat) = match body {
            Body::Earth | Body::Moon => (30.0, 30.0),
            Body::Mercury | Body::Venus => (10.0, 10.0),
            _ => (5.0, 5.0),
        };

        let (clon, rlon) = dlt(dlon, tol_lon);
        let (clat, rlat) = dlt(dlat, tol_lat);
        println!(
            "  {name:<8} starcat={:>10.4}°  vec-rot={:>10.4}°  Δlon: {clon}{:>6.3}{rlon}″  Δlat: {clat}{:>6.3}{rlat}″  (tol {:.0}″)",
            pos.longitude_deg, vec_lon_deg, dlon, dlat, tol_lon
        );

        if dlon > max_dlon {
            max_dlon = dlon;
            worst.clone_from(name);
        }
        assert!(
            dlon < tol_lon,
            "{chart_id}/helio/{name}: Δlon {dlon:.3}″ exceeds {tol_lon}″"
        );
        assert!(
            dlat < tol_lat,
            "{chart_id}/helio/{name}: Δlat {dlat:.3}″ exceeds {tol_lat}″"
        );
        bodies_checked += 1;
    }
    println!("  → {bodies_checked} bodies checked  max Δlon: {max_dlon:.3}″ ({worst})");
}

#[test]
fn unix_overflow_2038_heliocentric_vectors() {
    run_chart_heliocentric_vectors("unix_overflow_2038");
}
