//! Acceptance test: ASCII DE441 reader agrees with binary reader to < 1e-6 km.
//!
//! Both readers decode the same Chebyshev coefficients; agreement should be
//! close to f64 epsilon. The 1e-6 km bound is a safe guard against any
//! parsing regression in the ASCII reader.
//!
//! Skips cleanly when `$STARCAT_JPL_DATA` is unset or when either the binary
//! or ASCII de441 directory cannot be located from the given path.

use pericynthion::body::Body;
use pericynthion::ephemeris::Ephemeris;
use pericynthion::jpl::ascii::AsciiEphemeris;
use pericynthion::jpl::header::parse as parse_header;
use pericynthion::jpl::reader::EphemerisFile;
use std::path::{Path, PathBuf};

/// Given `$STARCAT_JPL_DATA` (which may point at the de441 dir itself or any
/// ancestor), return `(binary_de441_dir, ascii_de441_dir)` if both can be
/// found by walking upward from `start` looking for a `planets/` ancestor
/// that has `Linux/de441` and `ascii/de441` siblings.
///
/// Accepted forms:
///  - `.../planets/Linux/de441`  — typical mirror checkout
///  - `.../planets/Linux`        — one level up
///  - `.../planets`              — two levels up
///  - `.../eph/planets/...`      — deeper ancestor
///  - `.../ssd.jpl.nasa.gov/...` — mirror root
///
/// Strategy: walk upward from `start` until we find a dir that has both
/// `Linux/de441` and `ascii/de441` children. Stop after 10 hops (avoids
/// infinite ascent on a misconfigured path).
fn resolve_both_dirs(start: &Path) -> Option<(PathBuf, PathBuf)> {
    // Normalise away trailing slashes.
    let start = start.canonicalize().ok()?;

    let mut candidate = start.as_path();
    for _ in 0..10 {
        let bin_dir = candidate.join("Linux").join("de441");
        let asc_dir = candidate.join("ascii").join("de441");
        if bin_dir.is_dir() && asc_dir.is_dir() {
            return Some((bin_dir, asc_dir));
        }
        // Go up one level.
        candidate = candidate.parent()?;
    }
    None
}

#[test]
fn ascii_agrees_with_binary_de441() {
    // --- locate env var -------------------------------------------------
    let Some(val) = std::env::var_os("STARCAT_JPL_DATA") else {
        eprintln!("STARCAT_JPL_DATA not set — skipping ascii_agrees_with_binary_de441");
        return;
    };
    let start = PathBuf::from(&val);

    // --- resolve binary + ascii de441 dirs ------------------------------
    let Some((bin_dir, asc_dir)) = resolve_both_dirs(&start) else {
        eprintln!(
            "Could not locate both Linux/de441 and ascii/de441 dirs relative to \
             STARCAT_JPL_DATA={} — skipping ascii_agrees_with_binary_de441",
            start.display()
        );
        return;
    };
    eprintln!("binary dir : {}", bin_dir.display());
    eprintln!("ascii  dir : {}", asc_dir.display());

    // --- parse header (from the binary dir; identical in both) ----------
    let header_path = {
        let mut p = bin_dir.clone();
        p.push("header.441");
        p
    };
    let header_text = std::fs::read_to_string(&header_path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", header_path.display()));
    let header = parse_header(&header_text)
        .unwrap_or_else(|e| panic!("header parse error for {}: {e}", header_path.display()));

    // --- open binary reader ---------------------------------------------
    // discover::discover picks the right linux_*.441 binary for us.
    let paths = pericynthion::jpl::discover::discover(&bin_dir)
        .unwrap_or_else(|e| panic!("discover binary: {e}"));
    let bin_file = EphemerisFile::open(&paths.binary, &header)
        .unwrap_or_else(|e| panic!("EphemerisFile::open: {e}"));
    let bin_ephem = Ephemeris::new(&bin_file, &header)
        .unwrap_or_else(|e| panic!("Ephemeris::new (binary): {e}"));

    // --- open ASCII reader ----------------------------------------------
    let asc_src = AsciiEphemeris::open(&asc_dir, &header)
        .unwrap_or_else(|e| panic!("AsciiEphemeris::open: {e}"));
    let asc_ephem =
        Ephemeris::new(&asc_src, &header).unwrap_or_else(|e| panic!("Ephemeris::new (ascii): {e}"));

    // --- probe a handful of JDs and bodies ------------------------------
    // Choose JDs well within the modern portion of DE441 that both the
    // full binary and the ASCII chunks are guaranteed to cover.
    let jds: &[f64] = &[
        2_451_545.0, // J2000 (2000-Jan-1.5 TT)
        2_440_000.5, // 1968-May-23 — well into the binary range
        2_460_000.5, // 2023-Feb-25 — recent epoch
    ];
    let bodies: &[Body] = &[Body::Sun, Body::Moon, Body::Mars, Body::Pluto];

    let pos_tol: f64 = 1e-6; // km — same coefficients, should be ~epsilon
    let vel_tol: f64 = 1e-9; // km/day

    let mut max_pos_diff: f64 = 0.0;
    let mut max_vel_diff: f64 = 0.0;

    for &jd in jds {
        for &body in bodies {
            let bin_s = bin_ephem
                .state(body, jd)
                .unwrap_or_else(|e| panic!("binary state({body:?}, {jd}): {e}"));
            let asc_s = asc_ephem
                .state(body, jd)
                .unwrap_or_else(|e| panic!("ascii state({body:?}, {jd}): {e}"));

            for axis in 0..3 {
                let dp = (bin_s.position_km[axis] - asc_s.position_km[axis]).abs();
                let dv = (bin_s.velocity_km_per_day[axis] - asc_s.velocity_km_per_day[axis]).abs();
                max_pos_diff = max_pos_diff.max(dp);
                max_vel_diff = max_vel_diff.max(dv);
                assert!(
                    dp < pos_tol,
                    "{body:?} JD={jd} axis={axis}: position differs by {dp} km (tol {pos_tol})"
                );
                assert!(
                    dv < vel_tol,
                    "{body:?} JD={jd} axis={axis}: velocity differs by {dv} km/day (tol {vel_tol})"
                );
            }
        }
    }

    eprintln!("max position diff = {max_pos_diff:.3e} km  (tol {pos_tol:.0e})");
    eprintln!("max velocity diff = {max_vel_diff:.3e} km/day  (tol {vel_tol:.0e})");
}

/// Read the first coefficient record's `start_jd` (`coeffs[0]`) from an ASCII
/// chunk file. The first record's header line is `<recnum>  <ncoeff>`; the
/// next token-stream begins with the granule start JD in Fortran `D`-exponent
/// notation. Returns `None` if the file can't be read or parsed.
fn first_record_start_jd(chunk_path: &Path) -> Option<f64> {
    let text = std::fs::read_to_string(chunk_path).ok()?;
    let mut lines = text.lines();
    // First non-blank line is the record header `<recnum>  <ncoeff>`.
    loop {
        let l = lines.next()?;
        if !l.trim().is_empty() {
            break;
        }
    }
    // The next non-blank token is the granule start JD.
    for l in lines {
        if let Some(tok) = l.split_whitespace().next() {
            let swapped: String = tok
                .chars()
                .map(|c| if c == 'D' || c == 'd' { 'e' } else { c })
                .collect();
            return swapped.parse().ok();
        }
    }
    None
}

/// Seam test: prove ASCII↔binary parity at a **chunk boundary**.
///
/// The mid-chunk cases above never exercise the JD where one ASCII chunk file
/// (`ascpNNNNN.441`) hands off to the next. DE441's chunks overlap by one
/// granule — the last record of `ascp00000.441` and the first record of
/// `ascp01000.441` both carry the granule starting at JD ≈ 2 086 288.5 — and
/// every chunk's first-record start lies on the same 32-day global grid as the
/// binary file. `AsciiEphemeris::record_for_jd` floors against the *selected
/// chunk's own* `start_jd`, while the binary reader floors against the *file's*
/// `start_jd`; this test confirms both pick the identical granule for JDs that
/// straddle the seam (`B-1.0`, `B-0.001`, `B`, `B+0.001`, `B+1.0`).
///
/// `B` is derived empirically from the dataset (the first-record start JD of
/// `ascp01000.441`), not hardcoded, so it tracks the mirror.
///
/// Skips cleanly when `$STARCAT_JPL_DATA` is unset or the de441 dirs / the
/// boundary chunk cannot be located.
#[test]
fn ascii_agrees_with_binary_at_chunk_seam() {
    let Some(val) = std::env::var_os("STARCAT_JPL_DATA") else {
        eprintln!("STARCAT_JPL_DATA not set — skipping ascii_agrees_with_binary_at_chunk_seam");
        return;
    };
    let start = PathBuf::from(&val);

    let Some((bin_dir, asc_dir)) = resolve_both_dirs(&start) else {
        eprintln!(
            "Could not locate both Linux/de441 and ascii/de441 dirs relative to \
             STARCAT_JPL_DATA={} — skipping ascii_agrees_with_binary_at_chunk_seam",
            start.display()
        );
        return;
    };

    // --- derive the boundary JD `B` from a real chunk seam ----------------
    // `ascp01000.441` is the first positive chunk after `ascp00000.441`; its
    // first-record start JD is a genuine seam between two ASCII chunk files,
    // comfortably inside DE441 coverage.
    let boundary_chunk = asc_dir.join("ascp01000.441");
    let Some(boundary) = first_record_start_jd(&boundary_chunk) else {
        eprintln!(
            "Could not read boundary chunk {} — skipping ascii_agrees_with_binary_at_chunk_seam",
            boundary_chunk.display()
        );
        return;
    };
    eprintln!("chunk-seam boundary JD B = {boundary}");

    // --- parse header + open both readers (same as the mid-chunk test) ----
    let header_path = {
        let mut p = bin_dir.clone();
        p.push("header.441");
        p
    };
    let header_text = std::fs::read_to_string(&header_path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", header_path.display()));
    let header = parse_header(&header_text)
        .unwrap_or_else(|e| panic!("header parse error for {}: {e}", header_path.display()));

    let paths = pericynthion::jpl::discover::discover(&bin_dir)
        .unwrap_or_else(|e| panic!("discover binary: {e}"));
    let bin_file = EphemerisFile::open(&paths.binary, &header)
        .unwrap_or_else(|e| panic!("EphemerisFile::open: {e}"));
    let bin_ephem = Ephemeris::new(&bin_file, &header)
        .unwrap_or_else(|e| panic!("Ephemeris::new (binary): {e}"));

    let asc_src = AsciiEphemeris::open(&asc_dir, &header)
        .unwrap_or_else(|e| panic!("AsciiEphemeris::open: {e}"));
    let asc_ephem =
        Ephemeris::new(&asc_src, &header).unwrap_or_else(|e| panic!("Ephemeris::new (ascii): {e}"));

    // JDs straddling the seam: one granule below, just below, exactly at,
    // just above, and one granule above the boundary.
    let jds: &[f64] = &[
        boundary - 1.0,
        boundary - 0.001,
        boundary,
        boundary + 0.001,
        boundary + 1.0,
    ];
    let bodies: &[Body] = &[Body::Sun, Body::Mars, Body::Pluto];

    let pos_tol: f64 = 1e-6; // km
    let vel_tol: f64 = 1e-9; // km/day

    let mut max_pos_diff: f64 = 0.0;
    let mut max_vel_diff: f64 = 0.0;

    for &jd in jds {
        for &body in bodies {
            let bin_s = bin_ephem
                .state(body, jd)
                .unwrap_or_else(|e| panic!("binary state({body:?}, {jd}): {e}"));
            let asc_s = asc_ephem
                .state(body, jd)
                .unwrap_or_else(|e| panic!("ascii state({body:?}, {jd}): {e}"));

            for axis in 0..3 {
                let dp = (bin_s.position_km[axis] - asc_s.position_km[axis]).abs();
                let dv = (bin_s.velocity_km_per_day[axis] - asc_s.velocity_km_per_day[axis]).abs();
                max_pos_diff = max_pos_diff.max(dp);
                max_vel_diff = max_vel_diff.max(dv);
                assert!(
                    dp < pos_tol,
                    "{body:?} JD={jd} (B={boundary}) axis={axis}: position differs by {dp} km \
                     (tol {pos_tol}) — chunk-seam record selection diverged"
                );
                assert!(
                    dv < vel_tol,
                    "{body:?} JD={jd} (B={boundary}) axis={axis}: velocity differs by {dv} km/day \
                     (tol {vel_tol}) — chunk-seam record selection diverged"
                );
            }
        }
    }

    eprintln!("seam max position diff = {max_pos_diff:.3e} km  (tol {pos_tol:.0e})");
    eprintln!("seam max velocity diff = {max_vel_diff:.3e} km/day  (tol {vel_tol:.0e})");
}
