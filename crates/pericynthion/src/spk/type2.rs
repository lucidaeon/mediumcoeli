//! Type-2 Chebyshev position/velocity evaluation for SPK segments.
//!
//! # SPK Type-2 record layout
//!
//! A Type-2 segment stores `N` records, each occupying `RSIZE` DP elements.
//! The last four elements of the segment (`end_addr-3 … end_addr`) are the
//! trailer:
//!
//! | offset from segment end | field | meaning |
//! |---|---|---|
//! | −3 | `INIT` | ET (seconds past J2000) of the first record's interval start |
//! | −2 | `INTLEN` | seconds per record interval |
//! | −1 | `RSIZE` | doubles per record |
//! | 0 | `N` | record count |
//!
//! Each record is `[MID, RADIUS, Cx[0..nc], Cy[0..nc], Cz[0..nc]]` where
//! `nc = (RSIZE − 2) / 3`.
//!
//! # Evaluation
//!
//! For target ET `et`:
//!
//! ```text
//! idx  = clamp(floor((et - INIT) / INTLEN), 0, N - 1)
//! rec  = start_addr + idx * RSIZE          (1-based element address)
//! MID  = dword(rec)
//! RAD  = dword(rec + 1)
//! tau  = (et - MID) / RAD                  ∈ [−1, 1]
//! pos[axis] = evaluate(coeffs_axis, tau)   (km)
//! vel[axis] = evaluate_derivative(coeffs_axis, tau) / RAD * 86 400   (km/day)
//! ```
//!
//! The `/ RAD` converts d(pos)/dτ → d(pos)/d(sec), and `× 86 400` converts
//! km/s → km/day.
//!
//! # Approximation note
//!
//! SPK files use TDB (Barycentric Dynamical Time) as their time argument.
//! Callers typically supply TT seconds past J2000. TT and TDB differ by at
//! most ±1.7 ms (a periodic relativistic correction); for main-belt asteroids
//! at ~20 km/s this translates to a sub-meter position error, which is
//! negligible for astrological use.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]

use crate::chebyshev;
use crate::ephemeris::StateVector;
use crate::error::PericynthionError;
use crate::spk::daf::{Daf, SpkSegment};

/// Evaluate a Type-2 SPK segment at `et_sec` (seconds past J2000 TDB/TT).
///
/// Reads the segment trailer to locate the correct Chebyshev record, then
/// evaluates position and velocity via Clenshaw recurrence.
///
/// # Errors
///
/// Returns [`PericynthionError::Io`] (`InvalidData`) if:
/// - Any trailer element (`INIT`, `INTLEN`, `RSIZE`, `N`) falls outside the
///   mapped file (truncated or corrupt segment).
/// - `RSIZE < 2` or `nc = (RSIZE − 2) / 3 == 0` (degenerate segment).
/// - `N < 1` (no records in the segment).
/// - The selected record's extent exceeds `end_addr` (addresses are inconsistent).
/// - Any coefficient element falls outside the mapped file.
pub(crate) fn eval_type2(
    daf: &Daf,
    seg: &SpkSegment,
    et_sec: f64,
) -> Result<StateVector, PericynthionError> {
    /// Build a truncation/corrupt error referencing the daf path.
    macro_rules! truncated {
        ($msg:expr) => {
            PericynthionError::Io {
                path: daf.path().to_path_buf(),
                source: std::io::Error::new(std::io::ErrorKind::InvalidData, $msg),
            }
        };
    }

    // Read trailer: last four elements of the segment.
    let init = daf
        .try_dword(seg.end_addr - 3)
        .ok_or_else(|| truncated!("SPK segment for the requested epoch extends past end of file (truncated/corrupt): INIT element out of bounds"))?;
    let intlen = daf
        .try_dword(seg.end_addr - 2)
        .ok_or_else(|| truncated!("SPK segment for the requested epoch extends past end of file (truncated/corrupt): INTLEN element out of bounds"))?;
    let rsize = daf
        .try_dword(seg.end_addr - 1)
        .ok_or_else(|| truncated!("SPK segment for the requested epoch extends past end of file (truncated/corrupt): RSIZE element out of bounds"))? as i32;
    let n = daf
        .try_dword(seg.end_addr)
        .ok_or_else(|| truncated!("SPK segment for the requested epoch extends past end of file (truncated/corrupt): N element out of bounds"))? as i32;

    // Validate structural invariants before indexing.
    if rsize < 2 {
        return Err(truncated!(format!(
            "SPK segment for the requested epoch is corrupt: RSIZE={rsize} (must be ≥ 2)"
        )));
    }

    // Number of Chebyshev coefficients per axis (stays i32 throughout to
    // avoid usize→i32 cast lints when computing 1-based addresses).
    let nc = (rsize - 2) / 3;
    if nc < 1 {
        return Err(truncated!(format!(
            "SPK segment for the requested epoch is corrupt: RSIZE={rsize} yields nc={nc} (must be ≥ 1)"
        )));
    }

    if n < 1 {
        return Err(truncated!(format!(
            "SPK segment for the requested epoch is corrupt: N={n} (must be ≥ 1)"
        )));
    }

    // Which record covers `et_sec`?
    let raw_idx = ((et_sec - init) / intlen).floor() as i32;
    let idx = raw_idx.clamp(0, n - 1);

    // 1-based element address of the record start.
    let rec = seg.start_addr + idx * rsize;

    // Validate that the record's last element does not exceed end_addr.
    // The record spans [rec, rec + rsize - 1] (1-based, inclusive).
    if rec + rsize - 1 > seg.end_addr {
        return Err(truncated!(format!(
            "SPK segment for the requested epoch is corrupt: record at {rec} \
             (size {rsize}) exceeds segment end_addr={}",
            seg.end_addr
        )));
    }

    // MID and RADIUS of the Chebyshev sub-interval (both in seconds).
    let mid = daf
        .try_dword(rec)
        .ok_or_else(|| truncated!("SPK segment for the requested epoch extends past end of file (truncated/corrupt): MID element out of bounds"))?;
    let rad = daf
        .try_dword(rec + 1)
        .ok_or_else(|| truncated!("SPK segment for the requested epoch extends past end of file (truncated/corrupt): RADIUS element out of bounds"))?;

    // Normalized time argument τ ∈ [−1, 1].
    let tau = (et_sec - mid) / rad;

    let mut position_km = [0.0_f64; 3];
    let mut velocity_km_per_day = [0.0_f64; 3];

    for axis in 0..3_i32 {
        // Coefficients for this axis start at rec + 2 + axis * nc (1-based).
        let coeff_base = rec + 2 + axis * nc;
        let coeffs: Result<Vec<f64>, PericynthionError> = (0..nc)
            .map(|k| {
                daf.try_dword(coeff_base + k).ok_or_else(|| {
                    truncated!(format!(
                        "SPK segment for the requested epoch extends past end of file \
                         (truncated/corrupt): coefficient at addr={} out of bounds",
                        coeff_base + k
                    ))
                })
            })
            .collect();
        let coeffs = coeffs?;

        let ai = axis as usize;
        position_km[ai] = chebyshev::evaluate(&coeffs, tau);

        // d(pos)/dτ → d(pos)/dt: divide by RAD (seconds) → km/s → km/day.
        velocity_km_per_day[ai] = chebyshev::evaluate_derivative(&coeffs, tau) / rad * 86_400.0;
    }

    Ok(StateVector {
        position_km,
        velocity_km_per_day,
    })
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    /// Resolve `sb441-n16.bsp` by walking up from `$STARCAT_JPL_DATA`.
    fn test_bsp_n16() -> Option<PathBuf> {
        let val = std::env::var_os("STARCAT_JPL_DATA")?;
        let start = PathBuf::from(&val).canonicalize().ok()?;
        let mut candidate = start.as_path();
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

    /// Resolve `sb441-n373.bsp` by walking up from `$STARCAT_JPL_DATA`.
    fn test_bsp_n373() -> Option<PathBuf> {
        let val = std::env::var_os("STARCAT_JPL_DATA")?;
        let start = PathBuf::from(&val).canonicalize().ok()?;
        let mut candidate = start.as_path();
        for _ in 0..10 {
            let bsp = candidate
                .join("ftp")
                .join("eph")
                .join("small_bodies")
                .join("asteroids_de441")
                .join("sb441-n373.bsp");
            if bsp.is_file() {
                return Some(bsp);
            }
            candidate = candidate.parent()?;
        }
        None
    }

    /// Ceres (NAIF 2000001) at J2000 (et=0.0), heliocentric ICRF.
    ///
    /// Known-answer oracle from the SPK format reference (verified against
    /// both sb441-n16 and sb441-n373, bit-identical).
    #[test]
    fn ceres_heliocentric_at_j2000_matches_known_answer() {
        let Some(bsp) = test_bsp_n16() else {
            eprintln!("skip: sb441-n16.bsp not present (set STARCAT_JPL_DATA)");
            return;
        };
        let spk = super::super::SpkEphemeris::open(&bsp).unwrap();
        let s = spk.state(2_000_001, 0.0).unwrap();
        let want = [
            -355_942_359.526_872_f64,
            81_631_232.297_440,
            110_885_750.079_794,
        ];
        for (i, &w) in want.iter().enumerate() {
            assert!(
                (s.position_km[i] - w).abs() < 1e-3,
                "axis {i}: got {} want {} (diff {})",
                s.position_km[i],
                w,
                s.position_km[i] - w
            );
        }
        let r_km: f64 = s.position_km.iter().map(|c| c * c).sum::<f64>().sqrt();
        let au = r_km / 149_597_870.7;
        assert!(
            (au - 2.551_151_206).abs() < 1e-6,
            "|r| = {au:.9} AU (want 2.551151206)"
        );
        eprintln!(
            "Ceres J2000: X={:.6} Y={:.6} Z={:.6} |r|={:.9} AU",
            s.position_km[0], s.position_km[1], s.position_km[2], au
        );
    }

    /// Velocity is finite and in the right ballpark for a main-belt asteroid
    /// (~17 km/s ≈ 1.47e6 km/day; order of magnitude check only).
    #[test]
    fn ceres_velocity_is_finite_and_sane() {
        let Some(bsp) = test_bsp_n16() else {
            eprintln!("skip: sb441-n16.bsp not present (set STARCAT_JPL_DATA)");
            return;
        };
        let spk = super::super::SpkEphemeris::open(&bsp).unwrap();
        let s = spk.state(2_000_001, 0.0).unwrap();
        let speed = s
            .velocity_km_per_day
            .iter()
            .map(|v| v * v)
            .sum::<f64>()
            .sqrt();
        eprintln!(
            "Ceres speed at J2000: {speed:.2} km/day ({:.2} km/s)",
            speed / 86_400.0
        );
        assert!(speed.is_finite(), "velocity must be finite");
        assert!(speed > 0.0, "velocity must be non-zero");
        // Main-belt asteroid: 10 km/s – 30 km/s  →  8.6e5 – 2.6e6 km/day
        assert!(
            speed > 5e5 && speed < 5e6,
            "speed {speed:.0} km/day outside sanity band [5e5, 5e6]"
        );
    }

    /// n16 and n373 must agree on Ceres at a handful of ETs.
    ///
    /// Skips cleanly if either file is absent.
    #[test]
    fn ceres_n16_equals_n373_at_sample_times() {
        let (Some(bsp16), Some(bsp373)) = (test_bsp_n16(), test_bsp_n373()) else {
            eprintln!("skip: sb441-n16.bsp or sb441-n373.bsp not present");
            return;
        };
        let spk16 = super::super::SpkEphemeris::open(&bsp16).unwrap();
        let spk373 = super::super::SpkEphemeris::open(&bsp373).unwrap();

        // Sample ETs spanning different Chebyshev records (seconds past J2000).
        let ets = [
            0.0_f64,         // J2000
            1_000_000.0,     // ~11.6 days
            -5_000_000.0,    // ~57.9 days before J2000
            3_600_000_000.0, // ~114 years after J2000
        ];
        for et in ets {
            let s16 = spk16.state(2_000_001, et).unwrap();
            let s373 = spk373.state(2_000_001, et).unwrap();
            for axis in 0..3 {
                let diff = (s16.position_km[axis] - s373.position_km[axis]).abs();
                assert!(
                    diff < 1e-6,
                    "n16 vs n373 mismatch at et={et}: axis {axis} diff={diff:.3e} km"
                );
            }
        }
        eprintln!(
            "n16==n373 consistency check passed for {} sample ETs",
            ets.len()
        );
    }

    /// A valid DAF/SPK file whose segment `start_addr`/`end_addr` point beyond
    /// the file length must produce `Err`, not a panic, from `eval_type2` (via
    /// `SpkEphemeris::state`).
    ///
    /// This exercises the TDD-first path: the test was written before
    /// `try_dword` replaced the panicking `dword` in `eval_type2`.
    #[test]
    fn eval_type2_returns_err_for_truncated_segment() {
        use crate::spk::daf::unpack_summary_ints;

        let tmp = tempdir::TempDir::new("spk_trunc").unwrap();
        let p = tmp.path().join("truncated.bsp");

        // ── File record (record 1, bytes 0..1024) ────────────────────────────
        let mut file_rec = [0u8; 1024];
        file_rec[0..8].copy_from_slice(b"DAF/SPK ");
        file_rec[8..12].copy_from_slice(&2i32.to_le_bytes()); // ND=2
        file_rec[12..16].copy_from_slice(&6i32.to_le_bytes()); // NI=6
        file_rec[76..80].copy_from_slice(&2i32.to_le_bytes()); // FWARD=2
        file_rec[88..96].copy_from_slice(b"LTL-IEEE");

        // ── Summary record (record 2, bytes 1024..2048) ──────────────────────
        // One segment whose start_addr and end_addr are well past EOF.
        let mut sum_rec = [0u8; 1024];
        sum_rec[0..8].copy_from_slice(&0.0f64.to_le_bytes()); // NEXT=0
        sum_rec[8..16].copy_from_slice(&0.0f64.to_le_bytes()); // PREV=0
        sum_rec[16..24].copy_from_slice(&1.0f64.to_le_bytes()); // NSUM=1
        // Summary at offset 24: et_start, et_stop, then 6×i32.
        sum_rec[24..32].copy_from_slice(&(-1.0e10_f64).to_le_bytes()); // et_start
        sum_rec[32..40].copy_from_slice(&1.0e10_f64.to_le_bytes()); // et_stop
        // start_addr=9999, end_addr=99999 — both far past EOF.
        let ints: [i32; 6] = [2_000_001, 10, 1, 2, 9999, 99_999];
        let mut int_bytes = [0u8; 24];
        for (i, v) in ints.iter().enumerate() {
            int_bytes[i * 4..i * 4 + 4].copy_from_slice(&v.to_le_bytes());
        }
        assert_eq!(unpack_summary_ints(&int_bytes), ints);
        sum_rec[40..64].copy_from_slice(&int_bytes);

        // Write only 2 records (file + summary); NO data record — file is
        // intentionally truncated relative to the declared segment addresses.
        let mut data = Vec::with_capacity(2 * 1024);
        data.extend_from_slice(&file_rec);
        data.extend_from_slice(&sum_rec);
        std::fs::write(&p, &data).unwrap();

        // `Daf::open` must succeed (file record + summary are intact).
        let spk = super::super::SpkEphemeris::open(&p).unwrap();

        // `state` must return Err (not panic).
        let result = spk.state(2_000_001, 0.0);
        assert!(
            result.is_err(),
            "expected Err for truncated segment, got Ok"
        );
        let msg = format!("{}", result.unwrap_err());
        assert!(
            msg.to_lowercase().contains("truncated")
                || msg.to_lowercase().contains("corrupt")
                || msg.to_lowercase().contains("out of bounds"),
            "error message should mention truncation/corrupt/bounds, got: {msg}"
        );
        eprintln!("truncated-segment test: Err as expected — {msg}");
    }
}
