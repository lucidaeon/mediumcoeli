use std::path::Path;

use crate::error::PericynthionError;

fn io_err(path: &Path, msg: &str) -> PericynthionError {
    PericynthionError::Io {
        path: path.to_path_buf(),
        source: std::io::Error::new(std::io::ErrorKind::InvalidData, msg),
    }
}

/// `SE_AST_OFFSET`: SE body numbers for asteroids = 10000 + IAU number.
const SE_AST_OFFSET: u16 = 10_000;

/// Parsed binary header from an SE1 file.
///
/// The binary header starts immediately after the 4-line text header and
/// is exactly 196 bytes, little-endian.
pub(super) struct Se1Header {
    /// JD TT of the first granule start (authoritative).
    pub tfstart: f64,
    /// JD TT one granule past the last covered instant (authoritative).
    pub tfend: f64,
    /// Body flag bits (see `SEI_FLG_*` constants in `mod.rs`).
    pub iflg: u8,
    /// Chebyshev coefficient count per axis per segment.
    pub ncoe: usize,
    /// Normalization radius in AU; used in coefficient decoding.
    pub rmax: f64,
    /// Astronomical unit in km (used to convert AU output to km).
    pub aunit: f64,
    /// Raw file offset of the index table start.
    pub lndx0: usize,
    /// Granule length in days.
    pub dseg: f64,
    /// Body name as stored in the file (null-padded, up to 30 chars).
    pub body_name: String,
    /// IAU asteroid number (= `ipl_0` − `SE_AST_OFFSET`).
    pub asteroid_number: u32,
}

impl Se1Header {
    /// Parse the text header and binary header from the raw file bytes.
    pub fn parse(data: &[u8], path: &Path) -> Result<Self, PericynthionError> {
        // Locate end of text header: 4 newline-terminated ASCII lines.
        let mut nl = 0usize;
        let mut text_len = 0usize;
        for (i, &b) in data.iter().enumerate() {
            if b == b'\n' {
                nl += 1;
                if nl == 4 {
                    text_len = i + 1;
                    break;
                }
            }
        }
        if nl < 4 {
            return Err(io_err(path, "SE1 file has fewer than 4 text header lines"));
        }

        let bin = data.get(text_len..text_len + 196).ok_or_else(|| {
            io_err(
                path,
                "SE1 binary header truncated (file < text_header + 196 bytes)",
            )
        })?;

        // [0:4] endian_test = b"cba\0"
        if &bin[0..4] != b"cba\0" {
            return Err(io_err(
                path,
                "SE1 endian_test mismatch — big-endian files are not supported",
            ));
        }

        // [12:20] tfstart, [20:28] tfend
        let tfstart = f64::from_le_bytes(bin[12..20].try_into().unwrap());
        let tfend = f64::from_le_bytes(bin[20..28].try_into().unwrap());

        // [30:32] ipl[0] = SE body number
        let ipl_0 = u16::from_le_bytes(bin[30..32].try_into().unwrap());

        // [32:62] body name, null/space padded ASCII
        let name_raw = &bin[32..62];
        let name_end = name_raw.iter().position(|&b| b == 0).unwrap_or(30);
        let body_name = std::str::from_utf8(&name_raw[..name_end])
            .unwrap_or("")
            .trim()
            .to_owned();

        // [74:82] aunit in km
        let aunit = f64::from_le_bytes(bin[74..82].try_into().unwrap());

        // [106:110] lndx0 raw file offset
        let lndx0 = u32::from_le_bytes(bin[106..110].try_into().unwrap()) as usize;

        // [110] iflg, [111] ncoe
        let iflg = bin[110];
        let ncoe = bin[111] as usize;

        // [112:116] rmax_int (rmax * 1000)
        let rmax_int = u32::from_le_bytes(bin[112..116].try_into().unwrap());
        let rmax = f64::from(rmax_int) / 1000.0;

        // [132:140] dseg
        let dseg = f64::from_le_bytes(bin[132..140].try_into().unwrap());

        let asteroid_number = if ipl_0 >= SE_AST_OFFSET {
            u32::from(ipl_0 - SE_AST_OFFSET)
        } else {
            return Err(io_err(
                path,
                "SE1 ipl_0 is below SE_AST_OFFSET=10000 — not an asteroid file",
            ));
        };

        Ok(Self {
            tfstart,
            tfend,
            iflg,
            ncoe,
            rmax,
            aunit,
            lndx0,
            dseg,
            body_name,
            asteroid_number,
        })
    }
}
