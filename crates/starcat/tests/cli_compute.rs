//! Integration test: spawn the `starcat` binary as a subprocess, parse
//! its JZOD JSON output, and compare against reference chart oracle data.
//!
//! Unlike `acceptance_refchart.rs` (which calls library functions in-
//! process), this test exercises the *whole CLI surface*: clap arg
//! parsing, civil-time + zone → `JD_UT`, `JD_UT` → `JD_TT` (ΔT), JPL discovery,
//! ephemeris + house pipeline, and JZOD JSON serialisation.
//!
//! Requires `STARCAT_JPL_DATA` set to a directory containing a DE441
//! header/binary pair, same as the library acceptance tests.

use std::env;
use std::process::Command;

/// Cargo sets this for integration tests in a crate that builds a binary.
const STARCAT_BIN: &str = env!("CARGO_BIN_EXE_starcat");

fn jpl_data_dir() -> Option<String> {
    env::var("STARCAT_JPL_DATA").ok()
}

/// Unsigned DMS → DD.
fn dms(base: f64, d: f64, m: f64, s: f64) -> f64 {
    base + d + m / 60.0 + s / 3600.0
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

/// Find a placement object by `id` in a JZOD array (angles, points, or lots).
fn find_by_id<'a>(arr: &'a serde_json::Value, id: &str) -> Option<&'a serde_json::Value> {
    arr.as_array()?.iter().find(|v| v["id"] == id)
}

// Zodiac base degrees (Ari at 0°, …, Pis at 330°).
const ARI: f64 = 0.0;
const TAU: f64 = 30.0;
const GEM: f64 = 60.0;
const CAN: f64 = 90.0;
const LEO: f64 = 120.0;
const VIR: f64 = 150.0;
const LIB: f64 = 180.0;
const SCO: f64 = 210.0;
const SAG: f64 = 240.0;
const CAP: f64 = 270.0;
const AQU: f64 = 300.0;
const PIS: f64 = 330.0;

/// One CLI-driven chart test case.
struct Case {
    id: &'static str,
    args: &'static [&'static str],
    /// JSON key under `houses` — e.g. "placidus", "regiomontanus", "porphyry".
    house_system: &'static str,
    /// Reference cusps in 1-based natural order H1..H12, indexed 0..12.
    cusps_deg: [f64; 12],
    /// Per-chart tolerance in arcmin (matches `angle_tol_arcmin` in
    /// `acceptance_refchart.rs`).
    tol_arcmin: f64,
}

fn run_case(case: &Case) {
    let Some(jpl) = jpl_data_dir() else {
        eprintln!("STARCAT_JPL_DATA not set — skipping integration test");
        return;
    };
    let mut argv: Vec<String> = case.args.iter().map(|s| (*s).to_string()).collect();
    // --json (JZOD mode) always computes all house systems, so --house is
    // overridden internally — pass it anyway to keep argv well-formed.
    argv.extend([
        "--house".into(),
        case.house_system.into(),
        "--json".into(),
        "--jpl-data".into(),
        jpl,
    ]);

    let output = Command::new(STARCAT_BIN)
        .args(&argv)
        .output()
        .expect("failed to launch starcat binary");

    assert!(
        output.status.success(),
        "{}: starcat exited with {:?}\nargs: {:?}\nstderr:\n{}",
        case.id,
        output.status,
        argv,
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("starcat stdout must be UTF-8");
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("starcat --json output must parse as JSON");

    // JZOD: chart data lives under charts[0].
    let chart = &json["charts"][0];

    let cusps_obj = chart["houses"][case.house_system]
        .as_object()
        .unwrap_or_else(|| {
            panic!(
                "{}: houses.{} must be a JSON object keyed 1-12; got {}",
                case.id, case.house_system, chart["houses"]
            )
        });
    assert_eq!(
        cusps_obj.len(),
        12,
        "{}: must emit exactly 12 cusps, got {}",
        case.id,
        cusps_obj.len()
    );

    println!("=== CLI {}  {} ===", case.house_system, case.id);
    let mut max_arcmin = 0.0_f64;
    // cusps_deg is in H1..H12 order; access by house number key.
    // Each cusp is now an object {longitude, sign, degree, minute, second}.
    for (i, expected_deg) in case.cusps_deg.iter().enumerate() {
        let house_key = (i + 1).to_string();
        let got = cusps_obj[&house_key]["longitude"]
            .as_f64()
            .unwrap_or_else(|| panic!("{}: H{} cusp longitude not a number", case.id, i + 1));
        // Wrap to shortest arc to handle the 0/360 seam.
        let raw = (got - expected_deg).abs().rem_euclid(360.0);
        let delta_arcmin = raw.min(360.0 - raw) * 60.0;
        let (c, r) = dlt(delta_arcmin, case.tol_arcmin);
        println!(
            "  H{:>2}  starcat={:>10.4}°  reference={:>10.4}°  Δ: {c}{:>6.2}{r}′",
            i + 1,
            got,
            expected_deg,
            delta_arcmin
        );
        max_arcmin = max_arcmin.max(delta_arcmin);
        assert!(
            delta_arcmin < case.tol_arcmin,
            "{}/H{}: starcat {:.4}° vs reference {:.4}° → Δ {:.2}′ exceeds {:.0}′",
            case.id,
            i + 1,
            got,
            expected_deg,
            delta_arcmin,
            case.tol_arcmin
        );
    }
    println!(
        "  → max Δ = {:.2}′ (tol {:.0}′)",
        max_arcmin, case.tol_arcmin
    );

    // Envelope sanity: JD values are in ephemeris block, angles are an array.
    assert!(
        chart["ephemeris"]["jd_ut"].is_number(),
        "{}: missing ephemeris.jd_ut",
        case.id
    );
    assert!(
        chart["ephemeris"]["jd_tt"].is_number(),
        "{}: missing ephemeris.jd_tt",
        case.id
    );
    let angles = &chart["placements"]["angles"];
    assert!(
        find_by_id(angles, "ascendant")
            .and_then(|v| v["ecliptic_longitude"].as_f64())
            .is_some(),
        "{}: missing ascendant in placements.angles",
        case.id
    );
    assert!(
        find_by_id(angles, "midheaven")
            .and_then(|v| v["ecliptic_longitude"].as_f64())
            .is_some(),
        "{}: missing midheaven in placements.angles",
        case.id
    );
}

// =============================================================================
// Test 5 — Lightning Strike + Placidus
// =============================================================================
//
// 1955-11-12 22:04 PST, Universal City CA (reference resolved coords
// 34°N08'20" 118°W21'09" → 34.138889°, -118.3525°). PST = UTC−8.
#[test]
fn lightning_strike_placidus_via_cli() {
    let case = Case {
        id: "lightning_strike",
        args: &[
            "compute",
            "--date",
            "1955-11-12",
            "--time",
            "22:04:00",
            "--calendar",
            "gregorian",
            "--tz=-08:00",
            "--lat",
            "34.138889",
            "--lon=-118.3525",
        ],
        house_system: "placidus",
        cusps_deg: [
            dms(LEO, 5.0, 19.0, 30.0),  // H1  Leo⌖05°19'30"
            dms(LEO, 27.0, 41.0, 52.0), // H2  Leo⌖27°41'52"
            dms(VIR, 24.0, 16.0, 24.0), // H3  Vir⌖24°16'24"
            dms(LIB, 26.0, 7.0, 43.0),  // H4  Lib⌖26°07'43"
            dms(SAG, 1.0, 13.0, 48.0),  // H5  Sag⌖01°13'48"
            dms(CAP, 5.0, 6.0, 57.0),   // H6  Cap⌖05°06'57"
            dms(AQU, 5.0, 19.0, 30.0),  // H7  Aqu⌖05°19'30"
            dms(AQU, 27.0, 41.0, 52.0), // H8  Aqu⌖27°41'52"
            dms(PIS, 24.0, 16.0, 24.0), // H9  Pis⌖24°16'24"
            dms(ARI, 26.0, 7.0, 43.0),  // H10 Ari⌖26°07'43"
            dms(GEM, 1.0, 13.0, 48.0),  // H11 Gem⌖01°13'48"
            dms(CAN, 5.0, 6.0, 57.0),   // H12 Can⌖05°06'57"
        ],
        tol_arcmin: 5.0,
    };
    let _ = (TAU, SCO); // tolerate unused zodiac consts per chart
    run_case(&case);
}

// =============================================================================
// Test 1 — William Lilly + Regiomontanus
// =============================================================================
//
// 1602-05-11 02:00 LMT Diseworth (reference resolved coords 52°N47' 001°W11'
// → 52.7833°, -1.1833°). `--lmt` + `--lon` derives the LMT offset (≈ −4m44s)
// from the longitude automatically, so civil-time + zone reproduces
// reference's resolved UT 02:04:44 exactly.
//
// Tolerance 30′ matches `angle_tol_arcmin("william_lilly")` in
// `acceptance_refchart.rs` — reference's ΔT model differs from SMH 2016 by
// ~1 s at this epoch, but the Regiomontanus formula is lat/RAMC-sensitive
// enough that the equator-trisection arc residual dominates.
#[test]
fn william_lilly_regiomontanus_via_cli() {
    let case = Case {
        id: "william_lilly",
        args: &[
            "compute",
            "--date",
            "1602-05-11",
            "--time",
            "02:00:00",
            "--calendar",
            "gregorian",
            "--lat",
            "52.7833",
            "--lmt",
            "--lon=-1.1833",
        ],
        house_system: "regiomontanus",
        cusps_deg: [
            dms(PIS, 2.0, 6.0, 37.0),   // H1  Pis⌖02°06'37"
            dms(TAU, 7.0, 31.0, 40.0),  // H2  Tau⌖07°31'40"
            dms(GEM, 5.0, 20.0, 8.0),   // H3  Gem⌖05°20'08"
            dms(GEM, 19.0, 30.0, 14.0), // H4  Gem⌖19°30'14"
            dms(CAN, 1.0, 48.0, 2.0),   // H5  Can⌖01°48'02"
            dms(CAN, 19.0, 25.0, 5.0),  // H6  Can⌖19°25'05"
            dms(VIR, 2.0, 6.0, 37.0),   // H7  Vir⌖02°06'37"
            dms(SCO, 7.0, 31.0, 40.0),  // H8  Sco⌖07°31'40"
            dms(SAG, 5.0, 20.0, 8.0),   // H9  Sag⌖05°20'08"
            dms(SAG, 19.0, 30.0, 14.0), // H10 Sag⌖19°30'14"
            dms(CAP, 1.0, 48.0, 2.0),   // H11 Cap⌖01°48'02"
            dms(CAP, 19.0, 25.0, 5.0),  // H12 Cap⌖19°25'05"
        ],
        tol_arcmin: 30.0,
    };
    let _ = (ARI, LEO, AQU); // tolerate unused zodiac consts per chart
    run_case(&case);
}

// =============================================================================
// Test 0 — Vettius Valens + Porphyry
// =============================================================================
//
// 0120-02-08 18:35 LMT Antioch (reference resolved coords 36°N14' east of
// Greenwich — see acceptance_refchart.rs::VETTIUS_VALENS for the W↔E
// transcription discussion). `--lmt --lon=+36.1167` derives LMT offset
// +2h24m28s, reproducing reference's UT 16:10:32.
//
// Tolerance 120′ (2°) matches `angle_tol_arcmin("vettius_valens")` — the
// year-120 chart sits on the bleeding edge of the ΔT-model divergence:
// reference +9340 s vs SMH 2016 +10570 s (1230 s gap).
#[test]
fn vettius_valens_porphyry_via_cli() {
    let case = Case {
        id: "vettius_valens",
        args: &[
            "compute",
            "--date",
            "0120-02-08",
            "--time",
            "18:35:00",
            "--calendar",
            "julian",
            "--lat",
            "36.2333",
            "--lmt",
            "--lon=36.1167",
        ],
        house_system: "porphyry",
        cusps_deg: [
            dms(VIR, 1.0, 29.0, 3.0),  // H1  Vir⌖01°29'03"
            dms(LIB, 0.0, 12.0, 5.0),  // H2  Lib⌖00°12'05"
            dms(LIB, 28.0, 55.0, 6.0), // H3  Lib⌖28°55'06"
            dms(SCO, 27.0, 38.0, 8.0), // H4  Sco⌖27°38'08"
            dms(SAG, 28.0, 55.0, 6.0), // H5  Sag⌖28°55'06"
            dms(AQU, 0.0, 12.0, 5.0),  // H6  Aqu⌖00°12'05"
            dms(PIS, 1.0, 29.0, 3.0),  // H7  Pis⌖01°29'03"
            dms(ARI, 0.0, 12.0, 5.0),  // H8  Ari⌖00°12'05"
            dms(ARI, 28.0, 55.0, 6.0), // H9  Ari⌖28°55'06"
            dms(TAU, 27.0, 38.0, 8.0), // H10 Tau⌖27°38'08"
            dms(GEM, 28.0, 55.0, 6.0), // H11 Gem⌖28°55'06"
            dms(LEO, 0.0, 12.0, 5.0),  // H12 Leo⌖00°12'05"
        ],
        tol_arcmin: 120.0,
    };
    let _ = (CAN, CAP); // tolerate unused zodiac consts per chart
    run_case(&case);
}

// =============================================================================
// Placements structure: bodies, angles, points (vx/nodes/bml), lots
// =============================================================================
#[test]
fn json_placements_structure() {
    let Some(jpl) = jpl_data_dir() else {
        eprintln!("STARCAT_JPL_DATA not set — skipping integration test");
        return;
    };

    let output = Command::new(STARCAT_BIN)
        .args([
            "compute",
            "--date",
            "1955-11-12",
            "--time",
            "22:04:00",
            "--calendar",
            "gregorian",
            "--tz=-08:00",
            "--lat",
            "34.138889",
            "--lon=-118.3525",
            "--json",
            "--jpl-data",
            &jpl,
        ])
        .output()
        .expect("failed to launch starcat binary");

    assert!(output.status.success());
    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output.stdout).expect("utf-8"))
            .expect("must parse as JSON");

    // Top-level JZOD structure.
    assert!(json["version"].is_string(), "missing version");
    assert!(json["charts"].is_array(), "missing charts array");
    let chart = &json["charts"][0];
    // zodiac must be an object {name: "tropical"}, not a bare string (drift fix).
    assert_eq!(
        chart["zodiac"]["name"], "tropical",
        "zodiac must be an object with name=\"tropical\""
    );
    assert!(chart["placements"].is_object(), "missing placements");

    // Bodies array under placements.
    assert!(
        chart["placements"]["bodies"].is_array(),
        "missing placements.bodies"
    );

    // Angles is an array; ASC and MC must be present.
    let angles = &chart["placements"]["angles"];
    assert!(
        find_by_id(angles, "ascendant")
            .and_then(|v| v["ecliptic_longitude"].as_f64())
            .is_some(),
        "missing ascendant in placements.angles"
    );
    assert!(
        find_by_id(angles, "midheaven")
            .and_then(|v| v["ecliptic_longitude"].as_f64())
            .is_some(),
        "missing midheaven in placements.angles"
    );
    // Vertex must not be in angles (it's in points).
    assert!(
        find_by_id(angles, "vertex").is_none(),
        "vertex must not be in angles"
    );

    // Points array: vertex, both node variants, both BML variants.
    let points = &chart["placements"]["points"];
    assert!(
        find_by_id(points, "vertex")
            .and_then(|v| v["ecliptic_longitude"].as_f64())
            .is_some(),
        "missing vertex in placements.points"
    );
    assert!(
        find_by_id(points, "north_node_mean")
            .and_then(|v| v["ecliptic_longitude"].as_f64())
            .is_some(),
        "missing north_node_mean in placements.points"
    );
    assert!(
        find_by_id(points, "north_node_true")
            .and_then(|v| v["ecliptic_longitude"].as_f64())
            .is_some(),
        "missing north_node_true in placements.points"
    );
    assert!(
        find_by_id(points, "black_moon_lilith_mean")
            .and_then(|v| v["ecliptic_longitude"].as_f64())
            .is_some(),
        "missing black_moon_lilith_mean in placements.points"
    );
    assert!(
        find_by_id(points, "black_moon_lilith_true")
            .and_then(|v| v["ecliptic_longitude"].as_f64())
            .is_some(),
        "missing black_moon_lilith_true in placements.points"
    );

    // Lots array: lot_of_fortune must be present.
    let lots = &chart["placements"]["lots"];
    assert!(
        find_by_id(lots, "lot_of_fortune")
            .and_then(|v| v["ecliptic_longitude"].as_f64())
            .is_some(),
        "missing lot_of_fortune in placements.lots"
    );

    // Old top-level keys must not exist at the root.
    assert!(json["bodies"].is_null(), "bodies must live under charts[0]");
    assert!(json["angles"].is_null(), "angles must live under charts[0]");
    assert!(json["lots"].is_null(), "lots must live under charts[0]");
    assert!(
        json["placements"].is_null(),
        "placements must live under charts[0]"
    );
}

// =============================================================================
// House cusps: JSON object keyed "1"–"12", each value an object with longitude
// =============================================================================
#[test]
fn json_house_cusps_keyed_by_house_number() {
    let Some(jpl) = jpl_data_dir() else {
        eprintln!("STARCAT_JPL_DATA not set — skipping integration test");
        return;
    };

    let output = Command::new(STARCAT_BIN)
        .args([
            "compute",
            "--date",
            "1955-11-12",
            "--time",
            "22:04:00",
            "--calendar",
            "gregorian",
            "--tz=-08:00",
            "--lat",
            "34.138889",
            "--lon=-118.3525",
            "--json",
            "--jpl-data",
            &jpl,
        ])
        .output()
        .expect("failed to launch starcat binary");

    assert!(output.status.success());
    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output.stdout).expect("utf-8"))
            .expect("must parse as JSON");

    let chart = &json["charts"][0];

    // Placidus must be present (JZOD mode always emits all systems).
    assert!(
        chart["houses"]["placidus"].is_object(),
        "placidus cusps must be a JSON object keyed by house number"
    );

    // All 12 keys "1"–"12" must be present and each must be an object with a
    // numeric `longitude` field.
    for h in 1..=12_usize {
        let key = h.to_string();
        assert!(
            chart["houses"]["placidus"][&key]["longitude"].is_number(),
            "missing or non-numeric H{h}.longitude in placidus"
        );
    }

    // H1 must be the Ascendant cusp: Lightning Strike Asc = Leo 05°19'30" = 125.325°.
    let h1 = chart["houses"]["placidus"]["1"]["longitude"]
        .as_f64()
        .unwrap();
    let delta_arcmin = (h1 - dms(LEO, 5.0, 19.0, 30.0)).abs() * 60.0;
    assert!(
        delta_arcmin < 5.0,
        "H1 should be Asc (~125.325°), got {h1:.4}°, Δ = {delta_arcmin:.2}′"
    );

    // H7 must be the Descendant (opposite H1).
    let h7 = chart["houses"]["placidus"]["7"]["longitude"]
        .as_f64()
        .unwrap();
    let delta7 = (h7 - dms(AQU, 5.0, 19.0, 30.0)).abs() * 60.0;
    assert!(
        delta7 < 5.0,
        "H7 should be Desc (~305.325°), got {h7:.4}°, Δ = {delta7:.2}′"
    );
}

// =============================================================================
// JZOD output formatting: 8dp on body ecliptic_longitude; cusp sign labels
// =============================================================================
#[test]
fn json_degree_formatting() {
    let Some(jpl) = jpl_data_dir() else {
        eprintln!("STARCAT_JPL_DATA not set — skipping integration test");
        return;
    };

    let output = Command::new(STARCAT_BIN)
        .args([
            "compute",
            "--date",
            "1955-11-12",
            "--time",
            "22:04:00",
            "--calendar",
            "gregorian",
            "--tz=-08:00",
            "--lat",
            "34.138889",
            "--lon=-118.3525",
            "--json",
            "--jpl-data",
            &jpl,
        ])
        .output()
        .expect("failed to launch starcat binary");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf-8");
    let compact: String = stdout.chars().filter(|c| !c.is_whitespace()).collect();

    // Body ecliptic_longitude should have exactly 8 decimal places.
    let lon_marker = "\"ecliptic_longitude\":";
    let pos = compact
        .find(lon_marker)
        .expect("missing ecliptic_longitude");
    let after = &compact[pos + lon_marker.len()..];
    let end = after.find([',', '}']).unwrap();
    let num = after[..end].trim();
    let dp = num.split('.').nth(1).map_or(0, str::len);
    assert_eq!(dp, 8, "ecliptic_longitude should have 8dp, got: {num}");

    // Whole-sign H1 cusp must be an object with a `longitude` field (Leo rising,
    // H1 = Leo = 120°). The sign label must be "leo".
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("must parse as JSON");
    let chart = &json["charts"][0];
    let ws_h1 = &chart["houses"]["whole_sign"]["1"];
    assert!(
        ws_h1["longitude"].is_number(),
        "whole_sign H1 must have a numeric longitude field"
    );
    assert_eq!(
        ws_h1["sign"], "leo",
        "Lightning Strike H1 whole_sign must be Leo"
    );
    // H1 longitude for Leo rising = 120.0°.
    let h1_lon = ws_h1["longitude"].as_f64().unwrap();
    assert!(
        (h1_lon - 120.0).abs() < 0.01,
        "whole_sign H1 should be ~120°, got {h1_lon}"
    );

    // Whole-sign H2 must not show float noise (29.999... for 30° boundary).
    let ws_h2_lon = chart["houses"]["whole_sign"]["2"]["longitude"]
        .as_f64()
        .unwrap();
    assert!(
        ws_h2_lon > 29.9999,
        "H2 whole_sign cusp must not show float noise, got: {ws_h2_lon}"
    );
}

// Regression: chart with a whole-sign H12 at exactly 0° Aries must serialize
// the cusp longitude as a numeric 0 (not break parsing). Uses Taurus-rising
// which puts H12 = Aries = 0°.
#[test]
fn json_zero_cusp_no_scientific_notation() {
    let Some(jpl) = jpl_data_dir() else {
        eprintln!("STARCAT_JPL_DATA not set — skipping integration test");
        return;
    };

    let output = Command::new(STARCAT_BIN)
        .args([
            "compute",
            "--date",
            "1895-12-03",
            "--time",
            "15:15:00",
            "--calendar",
            "gregorian",
            "--tz=+01:00",
            "--lat",
            "48.208333",
            "--lon=16.371667",
            "--json",
            "--jpl-data",
            &jpl,
        ])
        .output()
        .expect("failed to launch starcat binary");

    assert!(output.status.success());
    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output.stdout).expect("utf-8"))
            .expect("must parse as JSON");

    let chart = &json["charts"][0];

    // H12 for Taurus-rising = Aries = 0°.
    let h12_lon = chart["houses"]["whole_sign"]["12"]["longitude"]
        .as_f64()
        .unwrap_or_else(|| panic!("H12 whole_sign longitude must be a number"));
    assert!(
        h12_lon.abs() < 0.001,
        "H12 Aries cusp must be ~0°, got {h12_lon}"
    );
    // Sign label must be "aries".
    assert_eq!(
        chart["houses"]["whole_sign"]["12"]["sign"], "aries",
        "H12 must be labeled aries"
    );
    // degree/minute/second must all be 0 (whole-sign invariant).
    assert_eq!(chart["houses"]["whole_sign"]["12"]["degree"], 0);
    assert_eq!(chart["houses"]["whole_sign"]["12"]["minute"], 0);
    assert_eq!(chart["houses"]["whole_sign"]["12"]["second"], 0);
}

// Whole-sign cusps are always exact multiples of 30°. Verify the sign labels
// and that the longitude field parses as a numeric value without noise.
// Uses an Aries-rising date+location so H1=Ari(0°) exercises the zero-cusp path.
#[test]
fn whole_sign_cusps_are_multiples_of_30() {
    let Some(jpl) = jpl_data_dir() else {
        eprintln!("STARCAT_JPL_DATA not set — skipping integration test");
        return;
    };

    let output = Command::new(STARCAT_BIN)
        .args([
            "compute",
            "--date",
            "1955-11-12",
            "--time",
            "15:15:00",
            "--calendar",
            "gregorian",
            "--tz=+01:00",
            "--lat",
            "48.208333",
            "--lon=16.371667",
            "--json",
            "--jpl-data",
            &jpl,
        ])
        .output()
        .expect("failed to launch starcat binary");

    assert!(output.status.success());
    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output.stdout).expect("utf-8"))
            .expect("must parse as JSON");

    let chart = &json["charts"][0];

    // Verify Aries rising so H1 = 0° exercises the zero-cusp path.
    let angles = &chart["placements"]["angles"];
    let ac = find_by_id(angles, "ascendant")
        .and_then(|v| v["ecliptic_longitude"].as_f64())
        .expect("ascendant must be present");
    assert!(
        (0.0..30.0).contains(&ac),
        "expected Aries rising, got ac = {ac:.4}"
    );

    let cusps = chart["houses"]["whole_sign"]
        .as_object()
        .expect("whole_sign must be a JSON object");
    assert_eq!(cusps.len(), 12, "must have 12 cusps");

    // Each cusp must have a longitude within 0.001° of its expected multiple of 30°.
    // H1 = Tau = 30°, H2 = Gem = 60°, …, H12 = Ari = 0°.
    // H1 starts at sign index 1 (Taurus = 30°); each subsequent adds 30°.
    let asc_sign_start = (ac / 30.0).floor() as usize; // 1 for Taurus
    for h in 1..=12_usize {
        let key = h.to_string();
        let lon = cusps[&key]["longitude"]
            .as_f64()
            .unwrap_or_else(|| panic!("H{h} longitude must be a number"));
        #[allow(clippy::cast_precision_loss)]
        let expected = (((asc_sign_start + h - 1) % 12) as f64) * 30.0;
        let delta = (lon - expected).abs().rem_euclid(360.0);
        let delta = delta.min(360.0 - delta);
        assert!(
            delta < 0.001,
            "whole_sign H{h}: expected ~{expected}°, got {lon}° (Δ = {delta}°)"
        );
        // degree/minute/second must all be 0 (whole-sign invariant).
        assert_eq!(cusps[&key]["degree"], 0, "H{h} degree must be 0");
        assert_eq!(cusps[&key]["minute"], 0, "H{h} minute must be 0");
        assert_eq!(cusps[&key]["second"], 0, "H{h} second must be 0");
    }
}

// =============================================================================
// JZOD body fields: daily_speed + retrograde
// =============================================================================
//
// Lightning Strike 1955-11-12: Uranus is retrograde on this date; Sun never
// is. Verifies that --json emits daily_speed and retrograde on every body.
#[test]
fn json_bodies_have_speed_and_retrograde() {
    let Some(jpl) = jpl_data_dir() else {
        eprintln!("STARCAT_JPL_DATA not set — skipping integration test");
        return;
    };

    let output = Command::new(STARCAT_BIN)
        .args([
            "compute",
            "--date",
            "1955-11-12",
            "--time",
            "22:04:00",
            "--calendar",
            "gregorian",
            "--tz=-08:00",
            "--lat",
            "34.138889",
            "--lon=-118.3525",
            "--json",
            "--jpl-data",
            &jpl,
        ])
        .output()
        .expect("failed to launch starcat binary");

    assert!(
        output.status.success(),
        "starcat failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output.stdout).expect("stdout must be UTF-8"))
            .expect("output must parse as JSON");

    let chart = &json["charts"][0];
    let bodies = chart["placements"]["bodies"]
        .as_array()
        .expect("placements.bodies must be an array");
    assert_eq!(bodies.len(), 10, "must emit exactly 10 bodies");

    // Every body must carry daily_speed and retrograde.
    for body in bodies {
        assert!(
            body["daily_speed"].is_number(),
            "missing daily_speed on {:?}",
            body["id"]
        );
        assert!(
            body["retrograde"].is_boolean(),
            "missing retrograde on {:?}",
            body["id"]
        );
    }

    // Sun (index 0): never retrograde, speed ~1°/day.
    assert_eq!(bodies[0]["id"], "sun");
    assert_eq!(bodies[0]["retrograde"], false, "Sun must not be retrograde");
    let sun_speed = bodies[0]["daily_speed"].as_f64().unwrap();
    assert!(
        (0.9..=1.1).contains(&sun_speed),
        "Sun daily speed should be ~1°/day, got {sun_speed:.4}"
    );

    // Uranus (index 7): retrograde on 1955-11-12 per reference chart set test 5.
    assert_eq!(bodies[7]["id"], "uranus");
    assert_eq!(
        bodies[7]["retrograde"], true,
        "Uranus should be retrograde on 1955-11-12"
    );
    assert!(
        bodies[7]["daily_speed"].as_f64().unwrap() < 0.0,
        "Uranus retrograde speed must be negative"
    );
}

// =============================================================================
// Heliocentric: daily_speed must use heliocentric positions
// =============================================================================
//
// In heliocentric mode Earth replaces Sun at index 0 and moves ~1°/day.
// No body should be retrograde (heliocentric view is always prograde).
// reference chart set test 4 uses the UNIX 2038 timestamp as a heliocentric chart.
#[test]
fn heliocentric_speed_uses_helio_positions() {
    let Some(jpl) = jpl_data_dir() else {
        eprintln!("STARCAT_JPL_DATA not set — skipping integration test");
        return;
    };

    let output = Command::new(STARCAT_BIN)
        .args([
            "compute",
            "--date",
            "2038-01-19",
            "--time",
            "03:14:07",
            "--calendar",
            "gregorian",
            "--tz=+00:00",
            "--helio",
            "--json",
            "--jpl-data",
            &jpl,
        ])
        .output()
        .expect("failed to launch starcat binary");

    assert!(
        output.status.success(),
        "starcat failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output.stdout).expect("stdout must be UTF-8"))
            .expect("output must parse as JSON");

    let chart = &json["charts"][0];
    let bodies = chart["placements"]["bodies"]
        .as_array()
        .expect("placements.bodies must be an array");

    // No body is ever retrograde from the heliocentric frame.
    for body in bodies {
        assert_eq!(
            body["retrograde"], false,
            "heliocentric body {:?} must not be retrograde",
            body["id"]
        );
    }

    // Earth (index 0) replaces Sun in heliocentric mode; moves ~1°/day.
    assert_eq!(bodies[0]["id"], "earth");
    let earth_speed = bodies[0]["daily_speed"].as_f64().unwrap();
    assert!(
        (0.95..=1.02).contains(&earth_speed),
        "Earth heliocentric speed should be ~1°/day, got {earth_speed:.5}"
    );
}
