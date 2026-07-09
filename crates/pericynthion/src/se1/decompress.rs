//! Nibble-packed Chebyshev record decompression for SE1 files.
//!
//! Ported from Swiss Ephemeris `sweph.c` function `get_new_segment`.
//! Each record holds 3 axis blocks (X, Y, Z). Each axis block begins with
//! a 2- or 4-byte header encoding nsize nibbles, followed by packed bytes.

use std::path::Path;

use crate::error::PericynthionError;

fn io_err(path: &Path, msg: impl Into<String>) -> PericynthionError {
    PericynthionError::Io {
        path: path.to_path_buf(),
        source: std::io::Error::new(std::io::ErrorKind::InvalidData, msg.into()),
    }
}

/// Decode one SE1 nibble-packed Chebyshev record.
///
/// `data` — raw record bytes (from `index[iseg]` to `index[iseg+1]`).
/// `ncoe` — coefficient count per axis (from binary header field `ncoe`).
/// `rmax` — normalization radius in AU (from binary header field `rmax`).
///
/// Returns a flat `Vec<f64>` of length `3 * ncoe`:
/// - `[0 .. ncoe]`       — X coefficients (AU)
/// - `[ncoe .. 2*ncoe]`  — Y coefficients (AU)
/// - `[2*ncoe .. 3*ncoe]`— Z coefficients (AU)
///
/// Unfilled coefficient slots are zero (high-frequency terms absent in
/// smooth, slow-moving bodies at the resolution of a 1000-day granule).
#[allow(clippy::too_many_lines)]
pub(super) fn decode_record(
    data: &[u8],
    ncoe: usize,
    rmax: f64,
    path: &Path,
) -> Result<Vec<f64>, PericynthionError> {
    let mut segp = vec![0.0_f64; 3 * ncoe];
    let mut pos = 0usize;

    for axis in 0..3usize {
        let axis_base = axis * ncoe;
        let mut coeff_idx = 0usize;

        // ── nsize header (2 bytes or 4 bytes) ──────────────────────────
        if pos + 2 > data.len() {
            return Err(io_err(
                path,
                format!("SE1 record axis {axis}: truncated at nsize header"),
            ));
        }
        let c0 = data[pos];
        let c1 = data[pos + 1];

        let (nsizes, nsize) = if c0 & 0x80 != 0 {
            // 4-byte header
            if pos + 4 > data.len() {
                return Err(io_err(
                    path,
                    format!("SE1 record axis {axis}: truncated at 4-byte nsize header"),
                ));
            }
            let c2 = data[pos + 2];
            let c3 = data[pos + 3];
            pos += 4;
            (
                6usize,
                [
                    usize::from(c1 >> 4),
                    usize::from(c1 & 0xf),
                    usize::from(c2 >> 4),
                    usize::from(c2 & 0xf),
                    usize::from(c3 >> 4),
                    usize::from(c3 & 0xf),
                ],
            )
        } else {
            pos += 2;
            (
                4usize,
                [
                    usize::from(c0 >> 4),
                    usize::from(c0 & 0xf),
                    usize::from(c1 >> 4),
                    usize::from(c1 & 0xf),
                    0,
                    0,
                ],
            )
        };

        // ── coefficient groups ──────────────────────────────────────────
        for (i, &count) in nsize[..nsizes].iter().enumerate() {
            if count == 0 {
                continue;
            }

            match i {
                // Groups 0–3: multi-byte coefficients (4, 3, 2, 1 bytes each)
                0..=3 => {
                    let bytes_per = 4 - i;
                    for _ in 0..count {
                        if pos + bytes_per > data.len() {
                            return Err(io_err(
                                path,
                                format!(
                                    "SE1 record axis {axis} group {i}: \
                                     truncated reading {bytes_per}-byte coeff at pos {pos}"
                                ),
                            ));
                        }
                        let mut v = 0u64;
                        for b in 0..bytes_per {
                            v |= u64::from(data[pos + b]) << (b * 8);
                        }
                        pos += bytes_per;
                        if coeff_idx < ncoe {
                            segp[axis_base + coeff_idx] = decode_int(v, rmax);
                        }
                        coeff_idx += 1;
                    }
                }

                // Group 4: half-nibble (2 per byte); same odd=neg/even=pos encoding as decode_int
                4 => {
                    let mut remaining = count;
                    while remaining > 0 {
                        if pos >= data.len() {
                            return Err(io_err(
                                path,
                                format!(
                                    "SE1 record axis {axis} group 4: \
                                     truncated at pos {pos}"
                                ),
                            ));
                        }
                        let byte = data[pos];
                        pos += 1;

                        // High nibble
                        if coeff_idx < ncoe {
                            segp[axis_base + coeff_idx] =
                                decode_nibble(usize::from(byte >> 4), rmax);
                        }
                        coeff_idx += 1;
                        remaining -= 1;

                        if remaining > 0 {
                            // Low nibble
                            if coeff_idx < ncoe {
                                segp[axis_base + coeff_idx] =
                                    decode_nibble(usize::from(byte & 0xf), rmax);
                            }
                            coeff_idx += 1;
                            remaining -= 1;
                        }
                    }
                }

                // Group 5: quarter-nibble (4 per byte); same odd=neg/even=pos encoding
                5 => {
                    let mut remaining = count;
                    while remaining > 0 {
                        if pos >= data.len() {
                            return Err(io_err(
                                path,
                                format!(
                                    "SE1 record axis {axis} group 5: \
                                     truncated at pos {pos}"
                                ),
                            ));
                        }
                        let byte = data[pos];
                        pos += 1;

                        let quads = [
                            usize::from((byte >> 6) & 3),
                            usize::from((byte >> 4) & 3),
                            usize::from((byte >> 2) & 3),
                            usize::from(byte & 3),
                        ];
                        for &q in &quads {
                            if remaining == 0 {
                                break;
                            }
                            if coeff_idx < ncoe {
                                segp[axis_base + coeff_idx] = decode_nibble(q, rmax);
                            }
                            coeff_idx += 1;
                            remaining -= 1;
                        }
                    }
                }

                _ => unreachable!(),
            }
        }
    }

    Ok(segp)
}

/// Decode a multi-byte unsigned value `v` into a Chebyshev coefficient (AU).
///
/// Sign encoding: odd v → negative, even v → positive.
///   - `v` even:   `coeff =  (v / 2) / 1e9 * rmax / 2`
///   - `v` odd:    `coeff = -((v + 1) / 2) / 1e9 * rmax / 2`
#[allow(clippy::cast_precision_loss)]
#[inline]
fn decode_int(v: u64, rmax: f64) -> f64 {
    let scale = rmax * 0.5e-9; // rmax / 2 / 1e9
    if v & 1 != 0 {
        -(v.div_ceil(2) as f64) * scale
    } else {
        ((v / 2) as f64) * scale
    }
}

/// Decode a nibble or sub-byte value `v` into a Chebyshev coefficient (AU).
///
/// Uses the same odd=negative / even=positive encoding as [`decode_int`]:
///   - `v` even: `coeff =  (v / 2) / 1e9 * rmax / 2`
///   - `v` odd:  `coeff = -((v + 1) / 2) / 1e9 * rmax / 2`
#[inline]
fn decode_nibble(v: usize, rmax: f64) -> f64 {
    decode_int(v as u64, rmax)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_int_zero_is_zero() {
        assert!(decode_int(0, 136.0).abs() < 1e-30);
    }

    #[test]
    fn decode_int_sign_alternation() {
        let rmax = 136.0_f64;
        let scale = rmax * 0.5e-9;
        // v=2 (even) → +1 * scale
        assert!((decode_int(2, rmax) - scale).abs() < 1e-30);
        // v=1 (odd) → -1 * scale
        assert!((decode_int(1, rmax) + scale).abs() < 1e-30);
        // v=4 (even) → +2 * scale
        assert!((decode_int(4, rmax) - 2.0 * scale).abs() < 1e-30);
        // v=3 (odd) → -2 * scale
        assert!((decode_int(3, rmax) + 2.0 * scale).abs() < 1e-30);
    }

    #[test]
    fn decode_nibble_half_nibble() {
        let rmax = 136.0_f64;
        let scale = rmax * 0.5e-9;
        // Same odd=neg/even=pos encoding as decode_int
        assert!(decode_nibble(0, rmax).abs() < 1e-30); // v=0  even  → 0
        assert!((decode_nibble(2, rmax) - scale).abs() < 1e-20); // v=2 even → +1
        assert!((decode_nibble(1, rmax) + scale).abs() < 1e-20); // v=1 odd → -1
        assert!((decode_nibble(10, rmax) - 5.0 * scale).abs() < 1e-20); // v=10 even → +5
        assert!((decode_nibble(11, rmax) + 6.0 * scale).abs() < 1e-20); // v=11 odd → -6
        assert!((decode_nibble(15, rmax) + 8.0 * scale).abs() < 1e-20); // v=15 odd → -8
    }
}
