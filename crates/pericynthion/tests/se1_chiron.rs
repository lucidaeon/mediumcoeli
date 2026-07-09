//! Integration test: SE1 ephemeris reader for Chiron via ast2/se02060.se1.
//!
//! Validates the reader pipeline: Se1File::open → state_at.
//!
//! Reference value (Chiron at J2000.0, JD 2451545.0, barycentric J2000
//! equatorial): X = −5.821023 AU, Y = −17.968142 AU, Z = −5.988003 AU;
//! |r| = 19.814 AU.
//!
//! Test skips cleanly when STARCAT_SE_DATA (or ASTRO_SPECIMENS) is unset.

use std::path::PathBuf;

use pericynthion::se1::{Se1File, se1_path};

const AU_KM: f64 = 149_597_870.7;
const J2000: f64 = 2_451_545.0;

fn se_root() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("STARCAT_SE_DATA") {
        let p = PathBuf::from(d);
        if p.exists() {
            return Some(p);
        }
    }
    // Fall back to $ASTRO_SPECIMENS/ast
    if let Some(base) = std::env::var_os("ASTRO_SPECIMENS") {
        let p = PathBuf::from(base).join("ast");
        if p.exists() {
            return Some(p);
        }
    }
    None
}

fn magnitude(v: &[f64; 3]) -> f64 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

// ─── Se1File reader tests ────────────────────────────────────────────────────

#[test]
fn se1_path_resolves_chiron() {
    let Some(root) = se_root() else {
        eprintln!("SE data not found — skipping");
        return;
    };
    let p = se1_path(&root, 2060).expect("Chiron se1 file should exist");
    assert!(p.exists(), "resolved path should exist: {}", p.display());
    assert!(
        p.to_string_lossy().contains("se02060"),
        "path should contain se02060: {}",
        p.display()
    );
}

#[test]
fn se1_chiron_opens_and_parses_header() {
    let Some(root) = se_root() else {
        eprintln!("SE data not found — skipping");
        return;
    };
    let Some(p) = se1_path(&root, 2060) else {
        eprintln!("Chiron se1 not found — skipping");
        return;
    };
    let se1 = Se1File::open(&p).expect("open Chiron se1");
    assert_eq!(se1.asteroid_number(), 2060);
    // Specimen covers roughly 675 CE – 3001 CE (850 × 1000-day records, DE441 basis).
    assert!(se1.start_jd() < J2000, "coverage should start before J2000");
    assert!(
        se1.end_jd() > J2000 + 100_000.0,
        "coverage should extend well past J2000"
    );
    assert!(
        se1.is_barycentric(),
        "Chiron iflg=0x08 should be barycentric"
    );
}

#[test]
fn chiron_at_j2000_barycentric_distance_plausible() {
    let Some(root) = se_root() else {
        eprintln!("SE data not found — skipping");
        return;
    };
    let Some(p) = se1_path(&root, 2060) else {
        eprintln!("Chiron se1 not found — skipping");
        return;
    };
    let se1 = Se1File::open(&p).expect("open Chiron se1");
    let sv = se1.state_at(J2000).expect("Chiron at J2000");
    let r_km = magnitude(&sv.position_km);
    let r_au = r_km / AU_KM;
    // Reference: |r| = 19.814 AU. Tolerance ±0.5 AU.
    assert!(
        r_au > 19.0 && r_au < 21.0,
        "Chiron barycentric distance {r_au:.3} AU, expected ~19.8 AU"
    );
}

#[test]
fn chiron_at_j2000_xyz_matches_reference() {
    let Some(root) = se_root() else {
        eprintln!("SE data not found — skipping");
        return;
    };
    let Some(p) = se1_path(&root, 2060) else {
        eprintln!("Chiron se1 not found — skipping");
        return;
    };
    let se1 = Se1File::open(&p).expect("open Chiron se1");
    let sv = se1.state_at(J2000).expect("Chiron at J2000");

    // Reference: X=-5.821023, Y=-17.968142, Z=-5.988003 AU.
    let x_au = sv.position_km[0] / AU_KM;
    let y_au = sv.position_km[1] / AU_KM;
    let z_au = sv.position_km[2] / AU_KM;

    let tol = 0.05; // ±0.05 AU tolerance (~7.5 million km)
    assert!(
        (x_au - (-5.821_023)).abs() < tol,
        "X: got {x_au:.6} AU, expected −5.821023 AU"
    );
    assert!(
        (y_au - (-17.968_142)).abs() < tol,
        "Y: got {y_au:.6} AU, expected −17.968142 AU"
    );
    assert!(
        (z_au - (-5.988_003)).abs() < tol,
        "Z: got {z_au:.6} AU, expected −5.988003 AU"
    );
}

#[test]
fn chiron_out_of_range_returns_error() {
    let Some(root) = se_root() else {
        eprintln!("SE data not found — skipping");
        return;
    };
    let Some(p) = se1_path(&root, 2060) else {
        eprintln!("Chiron se1 not found — skipping");
        return;
    };
    let se1 = Se1File::open(&p).expect("open Chiron se1");
    // JD 0 is well before the file's coverage
    let result = se1.state_at(0.0);
    assert!(result.is_err(), "out-of-range JD should return an error");
    let msg = format!("{}", result.unwrap_err());
    assert!(
        msg.contains("outside coverage") || msg.contains("coverage"),
        "error should mention coverage: {msg}"
    );
}

#[test]
fn se1_path_resolves_each_centaur() {
    let Some(root) = se_root() else {
        eprintln!("SE data not found — skipping");
        return;
    };
    for (ast_num, name) in [
        (2060, "Chiron"),
        (5145, "Pholus"),
        (7066, "Nessus"),
        (10199, "Chariklo"),
        (15760, "Albion"),
    ] {
        match se1_path(&root, ast_num) {
            Some(p) => assert!(p.exists(), "{name} path should exist: {}", p.display()),
            None => eprintln!("{name} (asteroid {ast_num}) se1 file not found — skipping"),
        }
    }
}
