//! Cross-validate the Type-21 reader against the trusted Type-2 reader using
//! Ceres, which is present in both `sb441-n16.bsp` (Type 2, NAIF 2000001) and a
//! Horizons-generated `.bsp` (Type 21, NAIF 20000001).
//!
//! The two files are independent DE441-family solutions (sb441 bundle vs Horizons
//! reconstruction). They are expected to agree within ~100 km across the span;
//! a Type-21 decoding bug would be orders of magnitude larger (thousands to
//! millions of km).
//!
//! Skips cleanly unless both files are present:
//! - `sb441-n16.bsp` under `$STARCAT_JPL_DATA`
//! - `20000001.bsp` under `$STARCAT_HORIZONS_DATA`
//!   (fetch once with `starcat horizons dp`).

use pericynthion::spk::SpkEphemeris;
use std::path::PathBuf;

fn sb441_n16() -> Option<PathBuf> {
    let start = PathBuf::from(std::env::var_os("STARCAT_JPL_DATA")?)
        .canonicalize()
        .ok()?;
    let mut dir = start.as_path();
    for _ in 0..10 {
        let bsp = dir.join("ftp/eph/small_bodies/asteroids_de441/sb441-n16.bsp");
        if bsp.is_file() {
            return Some(bsp);
        }
        dir = dir.parent()?;
    }
    None
}

fn horizons_ceres() -> Option<PathBuf> {
    let dir = PathBuf::from(std::env::var_os("STARCAT_HORIZONS_DATA")?);
    let bsp = dir.join("20000001.bsp");
    bsp.is_file().then_some(bsp)
}

#[test]
fn ceres_type21_matches_type2() {
    let (Some(n16), Some(hz)) = (sb441_n16(), horizons_ceres()) else {
        eprintln!(
            "skip: need sb441-n16.bsp ($STARCAT_JPL_DATA) and 20000001.bsp ($STARCAT_HORIZONS_DATA)"
        );
        return;
    };
    let t2 = SpkEphemeris::open(&n16).unwrap(); // Type 2, id 2000001
    let t21 = SpkEphemeris::open(&hz).unwrap(); // Type 21, id 20000001

    // Sample ETs within the Horizons fetch window (1781..2038). Seconds past
    // J2000; stay clear of the very edges.
    let ets = [
        -5.0e9_f64, // ~1842
        -1.0e9,     // ~1968
        0.0,        // J2000
        1.0e9,      // ~2031
    ];
    let mut worst = 0.0_f64;
    for et in ets {
        let a = t2.state(2_000_001, et).unwrap();
        let b = t21.state(20_000_001, et).unwrap();
        for axis in 0..3 {
            let d = (a.position_km[axis] - b.position_km[axis]).abs();
            worst = worst.max(d);
        }
    }
    eprintln!("Ceres Type-2 vs Type-21 max position diff: {worst:.3} km");
    // Both are DE441-consistent solutions; expect close agreement. A real
    // Type-21 decoding bug would produce km-to-AU-scale errors; 100 km gives
    // comfortable margin over the observed ~68 km orbit-solution divergence.
    assert!(
        worst < 100.0,
        "Type-21 disagrees with Type-2 by {worst:.3} km"
    );
}
