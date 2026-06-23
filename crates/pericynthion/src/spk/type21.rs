//! Type-21 (Extended Modified Difference Array) evaluation for SPK segments.
//!
//! Horizons-generated small-body SPKs use Type 21, whereas the bundled
//! `sb441` files use Type 2 ([`crate::spk::type2`]). The record layout and the
//! interpolation algorithm are documented in the implementation plan and in
//! NAIF's `spke21.f`.
//!
//! # Units
//!
//! The MDA algorithm yields km and km/s; velocity is scaled to km/day to match
//! [`StateVector`], exactly as the Type-2 evaluator does.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]

use crate::ephemeris::StateVector;
use crate::error::PericynthionError;
use crate::spk::daf::{Daf, SpkSegment};

/// Max difference-table dimension supported (NAIF `spk21.inc` `MAXTRM`).
const MAXTRM: usize = 25;

/// Evaluate a Type-21 SPK segment at `et_sec` (seconds past J2000 TDB/TT).
///
/// Reads the segment trailer (`MAXDIM`, `NRECS`), binary-searches the epoch
/// table for the record covering `et_sec`, then runs the Modified Difference
/// Array interpolation to produce position (km) and velocity (km/day).
///
/// # Errors
///
/// Returns [`PericynthionError::Io`] (`InvalidData`) if the segment is
/// truncated/corrupt (any address out of bounds), if `MAXDIM` is non-positive
/// or exceeds [`MAXTRM`], if `NRECS < 1`, or if a stepsize entry is zero.
#[allow(clippy::too_many_lines)]
pub(crate) fn eval_type21(
    daf: &Daf,
    seg: &SpkSegment,
    et_sec: f64,
) -> Result<StateVector, PericynthionError> {
    macro_rules! corrupt {
        ($msg:expr) => {
            PericynthionError::Io {
                path: daf.path().to_path_buf(),
                source: std::io::Error::new(std::io::ErrorKind::InvalidData, $msg),
            }
        };
    }
    macro_rules! dword {
        ($addr:expr, $what:expr) => {
            daf.try_dword($addr).ok_or_else(|| {
                corrupt!(format!(
                    "Type-21 SPK segment truncated/corrupt: {} out of bounds",
                    $what
                ))
            })?
        };
    }

    // Trailer: penultimate element = MAXDIM, last element = NRECS.
    let maxdim = dword!(seg.end_addr - 1, "MAXDIM") as i32;
    let nrecs = dword!(seg.end_addr, "NRECS") as i32;
    if maxdim < 1 || maxdim as usize > MAXTRM {
        return Err(corrupt!(format!(
            "Type-21 MAXDIM={maxdim} out of range (1..={MAXTRM})"
        )));
    }
    if nrecs < 1 {
        return Err(corrupt!(format!("Type-21 NRECS={nrecs} (must be ≥ 1)")));
    }
    let maxdim_u = maxdim as usize;
    let dlsize = 4 * maxdim + 11;

    // The segment must hold NRECS records (DLSIZE each) + NRECS epochs + the
    // 2-element trailer (MAXDIM, NRECS). Validate in i64 to avoid i32 overflow
    // in the address math below on a corrupt/oversized NRECS.
    let span = i64::from(seg.end_addr) - i64::from(seg.start_addr) + 1;
    let need = i64::from(nrecs) * i64::from(dlsize) + i64::from(nrecs) + 2;
    if span < need {
        return Err(corrupt!(format!(
            "Type-21 segment too small: NRECS={nrecs} needs {need} elements, have {span}"
        )));
    }

    // Epoch table: NRECS epochs starting at start_addr + NRECS*DLSIZE.
    let epoch_base = seg.start_addr + nrecs * dlsize;
    let epoch_at = |i: i32| daf.try_dword(epoch_base + i); // 0-based i in 0..NRECS

    // Find the first record whose epoch is strictly greater than et_sec.
    // Epochs ascend, so a binary search over [0, NRECS) gives the record index.
    let mut lo = 0_i32;
    let mut hi = nrecs - 1;
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        let e = epoch_at(mid).ok_or_else(|| corrupt!("Type-21 epoch table out of bounds"))?;
        if e > et_sec {
            hi = mid;
        } else {
            lo = mid + 1;
        }
    }
    let rec_idx = lo; // 0-based; clamps to last record when et_sec ≥ all epochs

    // Record rec_idx occupies DLSIZE elements starting here (1-based).
    let rec_start = seg.start_addr + rec_idx * dlsize;
    if rec_start + dlsize - 1 > seg.end_addr {
        return Err(corrupt!(format!(
            "Type-21 record {rec_idx} (size {dlsize}) exceeds segment end_addr={}",
            seg.end_addr
        )));
    }
    let rec = |off: i32| daf.try_dword(rec_start + off); // 0-based off into record

    // Unpack the record.
    let tl = rec(0).ok_or_else(|| corrupt!("Type-21 TL out of bounds"))?;
    let mut g = [0.0_f64; MAXTRM];
    for (j, slot) in g.iter_mut().take(maxdim_u).enumerate() {
        *slot = rec(1 + j as i32).ok_or_else(|| corrupt!("Type-21 G out of bounds"))?;
    }
    let mut refpos = [0.0_f64; 3];
    let mut refvel = [0.0_f64; 3];
    for axis in 0..3_i32 {
        refpos[axis as usize] =
            rec(maxdim + 1 + 2 * axis).ok_or_else(|| corrupt!("Type-21 REFPOS out of bounds"))?;
        refvel[axis as usize] =
            rec(maxdim + 2 + 2 * axis).ok_or_else(|| corrupt!("Type-21 REFVEL out of bounds"))?;
    }
    // DT[i][comp] = rec[MAXDIM+7 + comp*MAXDIM + i] (column-major).
    let mut dt = [[0.0_f64; 3]; MAXTRM];
    for comp in 0..3_i32 {
        for i in 0..maxdim {
            dt[i as usize][comp as usize] = rec(maxdim + 7 + comp * maxdim + i)
                .ok_or_else(|| corrupt!("Type-21 DT out of bounds"))?;
        }
    }
    let kqmax1 =
        rec(4 * maxdim + 7).ok_or_else(|| corrupt!("Type-21 KQMAX1 out of bounds"))? as i32;
    let mut kq = [0_i32; 3];
    for (c, slot) in kq.iter_mut().enumerate() {
        *slot = rec(4 * maxdim + 8 + c as i32)
            .ok_or_else(|| corrupt!("Type-21 KQ out of bounds"))? as i32;
    }
    if kqmax1 < 2 || kqmax1 as usize > MAXTRM {
        return Err(corrupt!(format!("Type-21 KQMAX1={kqmax1} out of range")));
    }

    // --- Modified Difference Array interpolation (NAIF spke21) ---
    let mut state = [0.0_f64; 6];
    let delta = et_sec - tl;
    let mut tp = delta;
    let mq2 = kqmax1 - 2;
    let mut ks = kqmax1 - 1;

    let mut fc = [0.0_f64; MAXTRM];
    fc[0] = 1.0;
    let mut wc = [0.0_f64; MAXTRM - 1];
    let mut w = [0.0_f64; MAXTRM + 2];

    for j in 1..=mq2 {
        let gj = g[(j - 1) as usize];
        if gj == 0.0 {
            return Err(corrupt!(format!("Type-21 stepsize G[{}] is zero", j - 1)));
        }
        fc[j as usize] = tp / gj;
        wc[(j - 1) as usize] = delta / gj;
        tp = delta + gj;
    }
    for j in 1..=kqmax1 {
        w[(j - 1) as usize] = 1.0 / f64::from(j);
    }

    let mut jx = 0_i32;
    let mut ks1 = ks - 1;
    while ks >= 2 {
        jx += 1;
        for j in 1..=jx {
            w[(j + ks - 1) as usize] = fc[j as usize] * w[(j + ks1 - 1) as usize]
                - wc[(j - 1) as usize] * w[(j + ks - 1) as usize];
        }
        ks = ks1;
        ks1 -= 1;
    }

    // Position (ks == 1 here).
    for i in 1..=3_i32 {
        let kqq = kq[(i - 1) as usize];
        let mut sum = 0.0;
        for j in (1..=kqq).rev() {
            sum += dt[(j - 1) as usize][(i - 1) as usize] * w[(j + ks - 1) as usize];
        }
        state[(i - 1) as usize] =
            refpos[(i - 1) as usize] + delta * (refvel[(i - 1) as usize] + delta * sum);
    }

    // Velocity W update, then velocity.
    for j in 1..=jx {
        w[(j + ks - 1) as usize] = fc[j as usize] * w[(j + ks1 - 1) as usize]
            - wc[(j - 1) as usize] * w[(j + ks - 1) as usize];
    }
    ks -= 1;
    for i in 1..=3_i32 {
        let kqq = kq[(i - 1) as usize];
        let mut sum = 0.0;
        for j in (1..=kqq).rev() {
            sum += dt[(j - 1) as usize][(i - 1) as usize] * w[(j + ks - 1) as usize];
        }
        state[(i + 2) as usize] = refvel[(i - 1) as usize] + delta * sum;
    }

    Ok(StateVector {
        position_km: [state[0], state[1], state[2]],
        // MDA velocity is km/s; StateVector wants km/day.
        velocity_km_per_day: [
            state[3] * 86_400.0,
            state[4] * 86_400.0,
            state[5] * 86_400.0,
        ],
    })
}

#[cfg(test)]
mod tests {
    use crate::spk::SpkEphemeris;
    use std::io::Write;

    // MAXDIM, records, epochs → a synthetic Type-21 .bsp with one segment whose
    // records all have DT=0 (so motion is exactly REFPOS + dt*REFVEL).
    // Each record: [TL, G(MAXDIM), refpos_x,refvel_x,refpos_y,refvel_y,
    //               refpos_z,refvel_z, DT(3*MAXDIM)=0, KQMAX1, KQ0,KQ1,KQ2].
    fn build_linear_type21(
        path: &std::path::Path,
        recs: &[(f64, [f64; 3], [f64; 3])],
        epochs: &[f64],
    ) {
        const MAXDIM: usize = 3;
        let dlsize = 4 * MAXDIM + 11;
        let kqmax1 = 3.0; // KS=2, MQ2=1 — exercises one while-iteration
        // DP element array (1-based addressing): segment will start at element 1
        // of the data area (record 3 onward).
        let mut data: Vec<f64> = Vec::new();
        for (tl, pos, vel) in recs {
            let mut rec = vec![0.0; dlsize];
            rec[0] = *tl;
            for g in rec.iter_mut().take(MAXDIM + 1).skip(1) {
                *g = 1.0; // nonzero stepsizes
            }
            rec[MAXDIM + 1] = pos[0];
            rec[MAXDIM + 2] = vel[0];
            rec[MAXDIM + 3] = pos[1];
            rec[MAXDIM + 4] = vel[1];
            rec[MAXDIM + 5] = pos[2];
            rec[MAXDIM + 6] = vel[2];
            // DT (3*MAXDIM) left zero.
            rec[4 * MAXDIM + 7] = kqmax1;
            rec[4 * MAXDIM + 8] = 1.0; // KQ0
            rec[4 * MAXDIM + 9] = 1.0; // KQ1
            rec[4 * MAXDIM + 10] = 1.0; // KQ2
            data.extend_from_slice(&rec);
        }
        data.extend_from_slice(epochs); // epoch table
        data.push(MAXDIM as f64); // penultimate = MAXDIM
        data.push(recs.len() as f64); // last = NRECS

        // start_addr is the 1-based DP element of the first data double. Data
        // begins at record 3 (byte 2048) → element (2048/8)+1 = 257.
        let start_addr: i32 = 257;
        let end_addr: i32 = start_addr + data.len() as i32 - 1;

        let mut file_rec = [0u8; 1024];
        file_rec[0..8].copy_from_slice(b"DAF/SPK ");
        file_rec[8..12].copy_from_slice(&2i32.to_le_bytes()); // ND
        file_rec[12..16].copy_from_slice(&6i32.to_le_bytes()); // NI
        file_rec[76..80].copy_from_slice(&2i32.to_le_bytes()); // FWARD=2
        file_rec[88..96].copy_from_slice(b"LTL-IEEE");

        let mut sum_rec = [0u8; 1024];
        sum_rec[0..8].copy_from_slice(&0.0f64.to_le_bytes()); // NEXT
        sum_rec[8..16].copy_from_slice(&0.0f64.to_le_bytes()); // PREV
        sum_rec[16..24].copy_from_slice(&1.0f64.to_le_bytes()); // NSUM
        sum_rec[24..32].copy_from_slice(&recs[0].0.to_le_bytes()); // et_start = TL of first record
        sum_rec[32..40].copy_from_slice(&epochs[epochs.len() - 1].to_le_bytes()); // et_stop
        let ints: [i32; 6] = [9999, 10, 1, 21, start_addr, end_addr];
        let mut ib = [0u8; 24];
        for (i, v) in ints.iter().enumerate() {
            ib[i * 4..i * 4 + 4].copy_from_slice(&v.to_le_bytes());
        }
        sum_rec[40..64].copy_from_slice(&ib);

        let mut bytes = Vec::new();
        bytes.extend_from_slice(&file_rec); // record 1
        bytes.extend_from_slice(&sum_rec); // record 2
        // record 3+: the data area, padded to a 1024-byte boundary.
        let mut data_bytes = Vec::new();
        for d in &data {
            data_bytes.extend_from_slice(&d.to_le_bytes());
        }
        bytes.extend_from_slice(&data_bytes);
        while bytes.len() % 1024 != 0 {
            bytes.push(0);
        }
        let mut f = std::fs::File::create(path).unwrap();
        f.write_all(&bytes).unwrap();
    }

    #[test]
    fn type21_linear_motion_is_exact() {
        let tmp = tempdir::TempDir::new("t21").unwrap();
        let p = tmp.path().join("linear.bsp");
        // Two records covering [0,100) and [100,200).
        let recs = [
            (0.0, [10.0, 20.0, 30.0], [1.0, 2.0, 3.0]),
            (100.0, [110.0, 220.0, 330.0], [1.0, 2.0, 3.0]),
        ];
        let epochs = [100.0, 200.0];
        build_linear_type21(&p, &recs, &epochs);
        let spk = SpkEphemeris::open(&p).unwrap();
        // At ET=50 (record 0, TL=0): pos = refpos + 50*refvel.
        let s = spk.state(9999, 50.0).unwrap();
        assert!((s.position_km[0] - (10.0 + 50.0 * 1.0)).abs() < 1e-9);
        assert!((s.position_km[1] - (20.0 + 50.0 * 2.0)).abs() < 1e-9);
        assert!((s.position_km[2] - (30.0 + 50.0 * 3.0)).abs() < 1e-9);
        // velocity = refvel, in km/day = km/s * 86400.
        assert!((s.velocity_km_per_day[0] - 1.0 * 86_400.0).abs() < 1e-6);
        // At ET=150 (record 1, TL=100): pos = refpos + 50*refvel.
        let s2 = spk.state(9999, 150.0).unwrap();
        assert!((s2.position_km[0] - (110.0 + 50.0 * 1.0)).abs() < 1e-9);
    }

    #[test]
    fn type21_oversized_nrecs_returns_err_not_panic() {
        // Build a tiny but structurally-valid Type-21 BSP whose trailer NRECS
        // field is wildly larger than the actual file. The overflow guard added
        // after `dlsize` is computed must return Err before any i32 address
        // arithmetic wraps or panics.
        use std::io::Write;
        const MAXDIM: usize = 3;
        let dlsize = 4 * MAXDIM + 11; // == 23
        let kqmax1 = 3.0_f64;

        // Build one legitimate record so the file parses past the header.
        let mut rec = vec![0.0_f64; dlsize];
        rec[0] = 0.0; // TL
        for g in rec.iter_mut().take(MAXDIM + 1).skip(1) {
            *g = 1.0; // non-zero stepsizes
        }
        rec[4 * MAXDIM + 7] = kqmax1;
        rec[4 * MAXDIM + 8] = 1.0;
        rec[4 * MAXDIM + 9] = 1.0;
        rec[4 * MAXDIM + 10] = 1.0;

        let epoch = 100.0_f64;
        let mut data: Vec<f64> = Vec::new();
        data.extend_from_slice(&rec); // one record
        data.push(epoch); // one epoch entry
        data.push(MAXDIM as f64); // penultimate = MAXDIM
        data.push(100_000_000.0); // NRECS — huge lie; file is tiny

        let start_addr: i32 = 257;
        let end_addr: i32 = start_addr + data.len() as i32 - 1;

        let mut file_rec = [0u8; 1024];
        file_rec[0..8].copy_from_slice(b"DAF/SPK ");
        file_rec[8..12].copy_from_slice(&2i32.to_le_bytes());
        file_rec[12..16].copy_from_slice(&6i32.to_le_bytes());
        file_rec[76..80].copy_from_slice(&2i32.to_le_bytes());
        file_rec[88..96].copy_from_slice(b"LTL-IEEE");

        let mut sum_rec = [0u8; 1024];
        sum_rec[16..24].copy_from_slice(&1.0f64.to_le_bytes());
        sum_rec[24..32].copy_from_slice(&0.0f64.to_le_bytes()); // et_start
        sum_rec[32..40].copy_from_slice(&200.0f64.to_le_bytes()); // et_stop
        let ints: [i32; 6] = [9999, 10, 1, 21, start_addr, end_addr];
        let mut ib = [0u8; 24];
        for (i, v) in ints.iter().enumerate() {
            ib[i * 4..i * 4 + 4].copy_from_slice(&v.to_le_bytes());
        }
        sum_rec[40..64].copy_from_slice(&ib);

        let mut bytes = Vec::new();
        bytes.extend_from_slice(&file_rec);
        bytes.extend_from_slice(&sum_rec);
        let mut data_bytes = Vec::new();
        for d in &data {
            data_bytes.extend_from_slice(&d.to_le_bytes());
        }
        bytes.extend_from_slice(&data_bytes);
        while bytes.len() % 1024 != 0 {
            bytes.push(0);
        }

        let tmp = tempdir::TempDir::new("t21overflow").unwrap();
        let p = tmp.path().join("overflow.bsp");
        std::fs::File::create(&p)
            .unwrap()
            .write_all(&bytes)
            .unwrap();

        let spk = SpkEphemeris::open(&p).unwrap();
        let result = spk.state(9999, 50.0);
        assert!(result.is_err(), "expected Err for oversized NRECS, got Ok");
        let msg = format!("{}", result.unwrap_err());
        assert!(
            msg.contains("too small") || msg.contains("NRECS"),
            "expected overflow-guard error, got: {msg}"
        );
    }

    #[test]
    fn type21_selects_record_by_epoch() {
        let tmp = tempdir::TempDir::new("t21sel").unwrap();
        let p = tmp.path().join("sel.bsp");
        // Distinct refpos per record so we can tell which one was used at TL.
        let recs = [
            (0.0, [1.0, 0.0, 0.0], [0.0, 0.0, 0.0]),
            (100.0, [2.0, 0.0, 0.0], [0.0, 0.0, 0.0]),
            (200.0, [3.0, 0.0, 0.0], [0.0, 0.0, 0.0]),
        ];
        let epochs = [100.0, 200.0, 300.0];
        build_linear_type21(&p, &recs, &epochs);
        let spk = SpkEphemeris::open(&p).unwrap();
        // DT=0, refvel=0 → position is exactly the record's refpos.
        assert!((spk.state(9999, 50.0).unwrap().position_km[0] - 1.0).abs() < 1e-9);
        assert!((spk.state(9999, 150.0).unwrap().position_km[0] - 2.0).abs() < 1e-9);
        assert!((spk.state(9999, 250.0).unwrap().position_km[0] - 3.0).abs() < 1e-9);
    }
}
