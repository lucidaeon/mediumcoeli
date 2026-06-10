//! Integration test: spawn the `starcat` binary as a subprocess, parse
//! its JSON output, and compare against refchart oracle data.
//!
//! Unlike `acceptance_refchart.rs` (which calls library functions in-
//! process), this test exercises the *whole CLI surface*: clap arg
//! parsing, civil-time + zone → `JD_UT`, `JD_UT` → `JD_TT` (ΔT), JPL discovery,
//! ephemeris + house pipeline, and JSON serialisation.
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
    /// Refchart cusps in 1-based natural order H1..H12, indexed 0..12.
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

    let cusps_obj = json["houses"][case.house_system]
        .as_object()
        .unwrap_or_else(|| {
            panic!(
                "{}: houses.{} must be a JSON object keyed 1-12; got {}",
                case.id, case.house_system, json["houses"]
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
    for (i, expected_deg) in case.cusps_deg.iter().enumerate() {
        let house_key = (i + 1).to_string();
        let got = cusps_obj[&house_key]
            .as_f64()
            .unwrap_or_else(|| panic!("{}: H{} cusp not a number", case.id, i + 1));
        // Wrap to shortest arc to handle the 0/360 seam.
        let raw = (got - expected_deg).abs().rem_euclid(360.0);
        let delta_arcmin = raw.min(360.0 - raw) * 60.0;
        let (c, r) = dlt(delta_arcmin, case.tol_arcmin);
        println!(
            "  H{:>2}  starcat={:>10.4}°  refchart={:>10.4}°  Δ: {c}{:>6.2}{r}′",
            i + 1,
            got,
            expected_deg,
            delta_arcmin
        );
        max_arcmin = max_arcmin.max(delta_arcmin);
        assert!(
            delta_arcmin < case.tol_arcmin,
            "{}/H{}: starcat {:.4}° vs refchart {:.4}° → Δ {:.2}′ exceeds {:.0}′",
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

    // Envelope sanity.
    assert!(json["jd_ut"].is_number(), "{}: missing jd_ut", case.id);
    assert!(json["jd_tt"].is_number(), "{}: missing jd_tt", case.id);
    assert!(
        json["placements"]["angles"]["ac_deg"].is_number(),
        "{}: missing placements.angles.ac_deg",
        case.id
    );
    assert!(
        json["placements"]["angles"]["mc_deg"].is_number(),
        "{}: missing placements.angles.mc_deg",
        case.id
    );
}

// =============================================================================
// Test 5 — Lightning Strike + Placidus
// =============================================================================
//
// 1955-11-12 22:04 PST, Universal City CA (refchart resolved coords
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
// 1602-05-11 02:00 LMT Diseworth (refchart resolved coords 52°N47' 001°W11'
// → 52.7833°, -1.1833°). `--lmt` + `--lon` derives the LMT offset (≈ −4m44s)
// from the longitude automatically, so civil-time + zone reproduces
// refchart's resolved UT 02:04:44 exactly.
//
// Tolerance 30′ matches `angle_tol_arcmin("william_lilly")` in
// `acceptance_refchart.rs` — refchart's ΔT model differs from SMH 2016 by
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
// 0120-02-08 18:35 LMT Antioch (refchart resolved coords 36°N14' east of
// Greenwich — see acceptance_refchart.rs::VETTIUS_VALENS for the W↔E
// transcription discussion). `--lmt --lon=+36.1167` derives LMT offset
// +2h24m28s, reproducing refchart's UT 16:10:32.
//
// Tolerance 120′ (2°) matches `angle_tol_arcmin("vettius_valens")` — the
// year-120 chart sits on the bleeding edge of the ΔT-model divergence:
// refchart +9340 s vs SMH 2016 +10570 s (1230 s gap).
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
// Placements structure: bodies, angles (ac/ds/mc/ic), points (vx/nn/lil…), lots
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
            "--nodes",
            "mean",
            "--lilith",
            "mean",
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

    // Top-level placements object must exist.
    assert!(json["placements"].is_object(), "missing placements");

    // bodies under placements.
    assert!(
        json["placements"]["bodies"].is_array(),
        "missing placements.bodies"
    );

    // angles contains only ac/ds/mc/ic.
    assert!(
        json["placements"]["angles"]["ac_deg"].is_number(),
        "missing placements.angles.ac_deg"
    );
    assert!(
        json["placements"]["angles"]["mc_deg"].is_number(),
        "missing placements.angles.mc_deg"
    );
    assert!(
        json["placements"]["angles"]["vx_deg"].is_null(),
        "vx_deg must not be in angles"
    );

    // points contains vx/ax/nodes/lilith.
    assert!(
        json["placements"]["points"]["vx_deg"].is_number(),
        "missing placements.points.vx_deg"
    );
    assert!(
        json["placements"]["points"]["nn_deg"].is_number(),
        "missing placements.points.nn_deg"
    );
    assert!(
        json["placements"]["points"]["lilith_deg"].is_number(),
        "missing placements.points.lilith_deg"
    );

    // lots under placements.
    assert!(
        json["placements"]["lots"]["fortune_deg"].is_number(),
        "missing placements.lots.fortune_deg"
    );

    // Old top-level keys must be gone.
    assert!(json["bodies"].is_null(), "bodies must move to placements");
    assert!(json["angles"].is_null(), "angles must move to placements");
    assert!(json["lots"].is_null(), "lots must move to placements");
}

// =============================================================================
// House cusps are a JSON object keyed "1"–"12", H1 = Ascendant cusp
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
            "--house",
            "placidus",
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

    // Must be an object, not an array.
    assert!(
        json["houses"]["placidus"].is_object(),
        "placidus cusps must be a JSON object keyed by house number"
    );

    // All 12 keys "1"–"12" must be present and numeric.
    for h in 1..=12_usize {
        let key = h.to_string();
        assert!(
            json["houses"]["placidus"][&key].is_number(),
            "missing or non-numeric H{h} in placidus"
        );
    }

    // H1 must be the Ascendant cusp: Lightning Strike Asc = Leo 05°19'30" = 125.325°.
    let h1 = json["houses"]["placidus"]["1"].as_f64().unwrap();
    let delta_arcmin = (h1 - dms(LEO, 5.0, 19.0, 30.0)).abs() * 60.0;
    assert!(
        delta_arcmin < 5.0,
        "H1 should be Asc (~125.325°), got {h1:.4}°, Δ = {delta_arcmin:.2}′"
    );

    // H7 must be the Descendant (opposite H1).
    let h7 = json["houses"]["placidus"]["7"].as_f64().unwrap();
    let delta7 = (h7 - dms(AQU, 5.0, 19.0, 30.0)).abs() * 60.0;
    assert!(
        delta7 < 5.0,
        "H7 should be Desc (~305.325°), got {h7:.4}°, Δ = {delta7:.2}′"
    );
}

// =============================================================================
// JSON output formatting: 8dp on all degree values including house cusps
// =============================================================================
//
// Whole Sign cusps are multiples of 30° but float arithmetic produces
// 29.999999999999996. format!("{:.8}") must round these to 30.00000000.
// Body longitude_deg and house cusps should both carry exactly 8dp in
// the raw JSON string.
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
            "--house",
            "whole-sign,placidus",
            "--json",
            "--jpl-data",
            &jpl,
        ])
        .output()
        .expect("failed to launch starcat binary");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf-8");
    // Strip whitespace so format (pretty/compact) doesn't matter.
    let compact: String = stdout.chars().filter(|c| !c.is_whitespace()).collect();

    // Body longitude_deg should have exactly 8 decimal places.
    let lon_marker = "\"longitude_deg\":";
    let pos = compact.find(lon_marker).expect("missing longitude_deg");
    let after = &compact[pos + lon_marker.len()..];
    let end = after.find([',', '}']).unwrap();
    let num = after[..end].trim();
    let dp = num.split('.').nth(1).map_or(0, str::len);
    assert_eq!(dp, 8, "longitude_deg should have 8dp, got: {num}");

    // Whole Sign H1 cusp (key "1") should have 8dp and no float noise.
    // Object format: "whole_sign":{"1":120.00000000,"2":150.00000000,...}
    let ws_h1_marker = "\"whole_sign\":{\"1\":";
    let wp = compact.find(ws_h1_marker).expect("missing whole_sign H1");
    let after_h1 = &compact[wp + ws_h1_marker.len()..];
    let h1_end = after_h1.find([',', '}']).unwrap();
    let h1_cusp = &after_h1[..h1_end];
    let cusp_dp = h1_cusp.split('.').nth(1).map_or(0, str::len);
    assert_eq!(
        cusp_dp, 8,
        "whole_sign H1 cusp should have 8dp, got: {h1_cusp}"
    );

    // H2 (key "2") must not show float noise (29.999...).
    let ws_h2_marker = "\"2\":";
    let wp2 = compact[wp..]
        .find(ws_h2_marker)
        .expect("missing whole_sign H2");
    let after_h2 = &compact[wp + wp2 + ws_h2_marker.len()..];
    let h2_end = after_h2.find([',', '}']).unwrap();
    let h2_cusp = &after_h2[..h2_end];
    assert!(
        !h2_cusp.contains("29."),
        "H2 whole_sign cusp must not show float noise, got: {h2_cusp}"
    );

    // Any cusp at 0° absolute (Aries boundary) must appear as "0.00000000"
    // not "0E-8" or "0" — Lightning Strike Leo-rising has H9 = Aries = 0°.
    let zero_marker = "\"9\":";
    let zp = compact[wp..]
        .find(zero_marker)
        .expect("missing whole_sign H9");
    let after_zero = &compact[wp + zp + zero_marker.len()..];
    let zero_end = after_zero.find([',', '}']).unwrap();
    let zero_cusp = &after_zero[..zero_end];
    assert_eq!(
        zero_cusp, "0.00000000",
        "0° cusp must be '0.00000000', not '{zero_cusp}'"
    );
}

// Regression: Anna Freud (Taurus rising) H12 = Aries = 0° must not serialize
// as "0E-8". This was broken when arbitrary_precision was used with a sorted
// array — the zero-value serde_json::Number normalized to scientific notation.
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
            "--house",
            "whole-sign",
            "--json",
            "--jpl-data",
            &jpl,
        ])
        .output()
        .expect("failed to launch starcat binary");

    assert!(output.status.success());
    let compact: String = String::from_utf8(output.stdout)
        .expect("utf-8")
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();

    // H12 for Taurus-rising = Aries = 0°.
    let marker = "\"12\":";
    let pos = compact.find(marker).expect("missing whole_sign H12");
    let after = &compact[pos + marker.len()..];
    let end = after.find([',', '}']).unwrap();
    let cusp = &after[..end];
    assert_eq!(
        cusp, "0.00000000",
        "H12 Aries cusp must be '0.00000000', got '{cusp}'"
    );
}

// Whole-sign cusps are always exact multiples of 30°. Every cusp must serialize
// as "N.00000000" (8dp, no float noise, no scientific notation for the 0° case).
// Uses a synthetic Aries-rising date+location (not PII) so H1 = 0° exercises
// the "0.00000000 not 0E-8" zero path, and H2–H12 exercise 30–330.
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
            "--house",
            "whole-sign",
            "--json",
            "--jpl-data",
            &jpl,
        ])
        .output()
        .expect("failed to launch starcat binary");

    assert!(output.status.success());
    let compact: String = String::from_utf8(output.stdout)
        .expect("utf-8")
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();

    let json: serde_json::Value = serde_json::from_str(&compact).expect("must parse as JSON");

    // Verify Aries rising so H1 = 0° exercises the 0E-8 regression path.
    let ac = json["placements"]["angles"]["ac_deg"].as_f64().unwrap();
    assert!(
        (0.0..30.0).contains(&ac),
        "expected Aries rising, got ac_deg = {ac:.4}"
    );

    let cusps = json["houses"]["whole_sign"]
        .as_object()
        .expect("whole_sign must be a JSON object");
    assert_eq!(cusps.len(), 12, "must have 12 cusps");

    // H1 = Aries = 0°; each subsequent house adds 30°.
    for h in 1..=12_usize {
        let key = h.to_string();
        // h ∈ 1..=12, well within f64's exact-integer range.
        #[allow(clippy::cast_precision_loss)]
        let expected_deg = ((h - 1) as f64) * 30.0;
        let expected_str = format!("{expected_deg:.8}");

        // Raw serialized value — must have exactly 8dp, no scientific notation.
        let raw_marker = format!("\"{key}\":");
        let pos = compact[compact.find("\"whole_sign\"").unwrap()..]
            .find(&raw_marker)
            .unwrap_or_else(|| panic!("missing whole_sign H{h}"));
        let base = compact.find("\"whole_sign\"").unwrap();
        let after = &compact[base + pos + raw_marker.len()..];
        let end = after.find([',', '}']).unwrap();
        let raw = &after[..end];
        assert_eq!(
            raw, expected_str,
            "whole_sign H{h}: expected \"{expected_str}\", got \"{raw}\""
        );
    }
}

// =============================================================================
// JSON body fields: daily_speed_deg + retrograde
// =============================================================================
//
// Lightning Strike 1955-11-12: Uranus is retrograde on this date; Sun never
// is. Verifies that --json emits daily_speed_deg and retrograde on every body.
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

    let bodies = json["placements"]["bodies"]
        .as_array()
        .expect("placements.bodies must be an array");
    assert_eq!(bodies.len(), 10, "must emit exactly 10 bodies");

    // Every body must carry daily_speed_deg and retrograde.
    for body in bodies {
        assert!(
            body["daily_speed_deg"].is_number(),
            "missing daily_speed_deg on {:?}",
            body["name"]
        );
        assert!(
            body["retrograde"].is_boolean(),
            "missing retrograde on {:?}",
            body["name"]
        );
    }

    // Sun (index 0): never retrograde, speed ~1°/day.
    assert_eq!(bodies[0]["name"], "Sun");
    assert_eq!(bodies[0]["retrograde"], false, "Sun must not be retrograde");
    let sun_speed = bodies[0]["daily_speed_deg"].as_f64().unwrap();
    assert!(
        (0.9..=1.1).contains(&sun_speed),
        "Sun daily speed should be ~1°/day, got {sun_speed:.4}"
    );

    // Uranus (index 7): retrograde on 1955-11-12 per REFCHARTS test 5.
    assert_eq!(bodies[7]["name"], "Uranus");
    assert_eq!(
        bodies[7]["retrograde"], true,
        "Uranus should be retrograde on 1955-11-12"
    );
    assert!(
        bodies[7]["daily_speed_deg"].as_f64().unwrap() < 0.0,
        "Uranus retrograde speed must be negative"
    );
}

// =============================================================================
// Heliocentric: daily_speed_deg must use heliocentric positions
// =============================================================================
//
// In heliocentric mode Earth replaces Sun at index 0 and moves ~1°/day.
// No body should be retrograde (heliocentric view is always prograde).
// REFCHARTS test 4 uses the UNIX 2038 timestamp as a heliocentric chart.
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

    let bodies = json["placements"]["bodies"]
        .as_array()
        .expect("placements.bodies must be an array");

    // No body is ever retrograde from the heliocentric frame.
    for body in bodies {
        assert_eq!(
            body["retrograde"], false,
            "heliocentric body {:?} must not be retrograde",
            body["name"]
        );
    }

    // Earth (index 0) replaces Sun in heliocentric mode; moves ~1°/day.
    assert_eq!(bodies[0]["name"], "Earth");
    let earth_speed = bodies[0]["daily_speed_deg"].as_f64().unwrap();
    assert!(
        (0.95..=1.02).contains(&earth_speed),
        "Earth heliocentric speed should be ~1°/day, got {earth_speed:.5}"
    );
}
