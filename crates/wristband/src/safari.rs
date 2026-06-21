//! Safari `Cookies.binarycookies` parser.
//!
//! Safari stores cookies in a proprietary binary format, **not** in `SQLite`.
//! This module reads that file — via a bounds-checked, panic-safe parser over
//! the raw bytes — and routes every row through [`crate::gate::gate`] so that
//! host filtering is always applied before any value is exposed (INV-2).
//!
//! # Format overview
//!
//! ```text
//! File header:
//!   [0..4]   magic            b"cook"
//!   [4..8]   page_count       big-endian u32
//!   [8..]    page_sizes[]     N × big-endian u32
//!   pages follow inline
//!
//! Page header (at page_start):
//!   [+0..+4]  signature        \x00\x00\x01\x00
//!   [+4..+8]  cookie_count     little-endian u32
//!   [+8..]    offsets[]        M × little-endian u32 (each relative to page_start)
//!   cookie records follow
//!
//! Cookie record (at page_start + offset[i]):
//!   Byte offset within record (all LE unless noted):
//!   [+0..+4]   record_size     little-endian u32
//!   [+4..+8]   unknown         (skip)
//!   [+8..+12]  flags           little-endian u32  (bit 0 = Secure)
//!   [+12..+16] unknown         (skip)
//!   [+16..+20] url_offset      little-endian u32  (from record start → NUL-terminated domain)
//!   [+20..+24] name_offset     little-endian u32
//!   [+24..+28] path_offset     little-endian u32
//!   [+28..+32] value_offset    little-endian u32
//!   [+32..+40] expiry          little-endian f64  seconds since 2001-01-01
//!   [+40..+48] creation        little-endian f64  seconds since 2001-01-01
//!   [+48..]    string data     NUL-terminated strings at the offsets above
//! ```
//!
//! Mac absolute time → Unix: `unix = mac_time + 978_307_200`.
//!
//! # Panic safety
//!
//! Every read is bounds-checked against the actual buffer.  A truncated or
//! malformed file returns [`WristbandError::Parse`]; it **never** panics or
//! indexes out of bounds.

use std::path::Path;

use crate::cookie::{Cookie, RawRow};
use crate::domain::Domain;
use crate::error::WristbandError;
use crate::gate::gate;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// File magic bytes.
const MAGIC: &[u8; 4] = b"cook";
/// Page header signature.
const PAGE_SIG: &[u8; 4] = &[0x00, 0x00, 0x01, 0x00];
/// Mac absolute-time epoch offset to Unix (seconds from 1970-01-01 to 2001-01-01).
const MAC_EPOCH_DELTA: i64 = 978_307_200;

/// Minimum record header size in bytes (all fixed fields through the
/// end of the creation double, exclusive of string data).
const RECORD_HEADER_MIN: usize = 48;

/// Sanity cap on the page count from the file header — far above any real
/// Safari cookie store; guards against a crafted count triggering a huge
/// allocation. A real file has at most a handful of pages.
const MAX_PAGE_COUNT: usize = 1_000_000;
/// Sanity cap on per-page cookie count, same rationale.
const MAX_COOKIE_COUNT: usize = 1_000_000;

// ---------------------------------------------------------------------------
// Byte-slice helpers (all bounds-checked)
// ---------------------------------------------------------------------------

/// Read a big-endian `u32` from `buf[offset..offset+4]`.
///
/// Returns `None` if the slice is too short.
#[inline]
fn read_be_u32(buf: &[u8], offset: usize) -> Option<u32> {
    let end = offset.checked_add(4)?;
    let bytes: [u8; 4] = buf.get(offset..end)?.try_into().ok()?;
    Some(u32::from_be_bytes(bytes))
}

/// Read a little-endian `u32` from `buf[offset..offset+4]`.
///
/// Returns `None` if the slice is too short.
#[inline]
fn read_le_u32(buf: &[u8], offset: usize) -> Option<u32> {
    let end = offset.checked_add(4)?;
    let bytes: [u8; 4] = buf.get(offset..end)?.try_into().ok()?;
    Some(u32::from_le_bytes(bytes))
}

/// Read a little-endian `f64` from `buf[offset..offset+8]`.
///
/// Returns `None` if the slice is too short.
#[inline]
fn read_le_f64(buf: &[u8], offset: usize) -> Option<f64> {
    let end = offset.checked_add(8)?;
    let bytes: [u8; 8] = buf.get(offset..end)?.try_into().ok()?;
    Some(f64::from_le_bytes(bytes))
}

/// Find the first NUL byte in `buf` at or after `start`, and return the
/// UTF-8 string up to (but not including) that NUL.
///
/// Returns `None` if there is no NUL byte within `buf[start..]` or if the
/// bytes are not valid UTF-8.
#[inline]
fn read_cstr(buf: &[u8], start: usize) -> Option<String> {
    let slice = buf.get(start..)?;
    let nul_pos = slice.iter().position(|&b| b == 0)?;
    let s = std::str::from_utf8(slice.get(..nul_pos)?).ok()?;
    Some(s.to_owned())
}

// ---------------------------------------------------------------------------
// Core parser
// ---------------------------------------------------------------------------

/// Parse a `Cookies.binarycookies` byte buffer into raw rows.
///
/// This function is **pure** (no I/O) and is not `#[cfg(target_os = "macos")]`
/// so that its unit tests compile on every host.
///
/// # Errors
///
/// Returns [`WristbandError::Parse`] when the file magic, page count, or any
/// other structural element is invalid or truncated.  Individual malformed
/// cookie records are skipped rather than aborting the whole parse.
pub(crate) fn parse_binarycookies(bytes: &[u8]) -> Result<Vec<RawRow>, WristbandError> {
    // -----------------------------------------------------------------------
    // File header
    // -----------------------------------------------------------------------

    // Magic: first 4 bytes must be b"cook".
    if bytes.get(..4) != Some(MAGIC.as_ref()) {
        return Err(WristbandError::Parse(
            "binarycookies: bad magic (expected b\"cook\")".to_owned(),
        ));
    }

    // Page count (BE u32 at offset 4).
    let page_count = read_be_u32(bytes, 4)
        .ok_or_else(|| WristbandError::Parse("binarycookies: truncated page count".to_owned()))?
        as usize;

    // Guard: a huge page count from a crafted file must not trigger a giant
    // allocation.  Check before computing sizes_end so the reviewer's minimal
    // test (magic + count only) triggers this error, not the truncation error.
    if page_count > MAX_PAGE_COUNT {
        return Err(WristbandError::Parse(
            "binarycookies: page count exceeds sanity limit".to_owned(),
        ));
    }

    // Page sizes array: N × BE u32 starting at offset 8.
    // Each size must be readable without overflow.
    let sizes_end = 8_usize
        .checked_add(page_count.checked_mul(4).ok_or_else(|| {
            WristbandError::Parse("binarycookies: page count too large".to_owned())
        })?)
        .ok_or_else(|| WristbandError::Parse("binarycookies: page sizes overflow".to_owned()))?;

    if bytes.len() < sizes_end {
        return Err(WristbandError::Parse(
            "binarycookies: file truncated in page-size table".to_owned(),
        ));
    }

    let mut page_sizes: Vec<usize> = Vec::with_capacity(page_count);
    for i in 0..page_count {
        // Each page-size entry is at offset 8 + i*4.
        let sz = read_be_u32(bytes, 8 + i * 4)
            .ok_or_else(|| WristbandError::Parse("binarycookies: truncated page size".to_owned()))?
            as usize;
        page_sizes.push(sz);
    }

    // -----------------------------------------------------------------------
    // Pages
    // -----------------------------------------------------------------------

    let mut rows: Vec<RawRow> = Vec::new();
    let mut page_start = sizes_end;

    for page_size in &page_sizes {
        // Safety: both page_start and page_size could be attacker-controlled.
        let page_end = page_start
            .checked_add(*page_size)
            .ok_or_else(|| WristbandError::Parse("binarycookies: page size overflow".to_owned()))?;
        let page = bytes.get(page_start..page_end).ok_or_else(|| {
            WristbandError::Parse("binarycookies: file truncated before end of page".to_owned())
        })?;

        parse_page(page, &mut rows);

        page_start = page_end;
    }

    Ok(rows)
}

/// Parse one page of the `binarycookies` file, appending valid rows to `out`.
///
/// Malformed cookie records within a valid page are skipped silently.
fn parse_page(page: &[u8], out: &mut Vec<RawRow>) {
    // Page signature: first 4 bytes must be `\x00\x00\x01\x00`.
    if page.get(..4) != Some(PAGE_SIG.as_ref()) {
        // Malformed page: skip entirely.
        return;
    }

    // Cookie count (LE u32 at offset 4 within page).
    let Some(cookie_count) = read_le_u32(page, 4).map(|n| n as usize) else {
        return;
    };

    // Guard: a crafted cookie count must not trigger a giant allocation.
    // Check immediately after reading the field, before any arithmetic on it.
    if cookie_count > MAX_COOKIE_COUNT {
        return;
    }

    // Per-cookie offsets: M × LE u32 starting at page offset 8.
    let Some(offsets_end) = (8_usize).checked_add(cookie_count.saturating_mul(4)) else {
        return;
    };
    if page.len() < offsets_end {
        return;
    }

    let mut offsets: Vec<usize> = Vec::with_capacity(cookie_count);
    for i in 0..cookie_count {
        let offset_pos = 8 + i * 4;
        let Some(off) = read_le_u32(page, offset_pos).map(|n| n as usize) else {
            return;
        };
        offsets.push(off);
    }

    // Parse each cookie record.
    for rec_off in offsets {
        if let Some(row) = parse_record(page, rec_off) {
            out.push(row);
        }
    }
}

/// Parse a single cookie record from `page` at byte offset `rec_off`.
///
/// Returns `None` for any structural problem (truncation, bad offsets, bad
/// UTF-8) so the caller can skip the record.
fn parse_record(page: &[u8], rec_off: usize) -> Option<RawRow> {
    // Record slice: starts at rec_off; minimum `RECORD_HEADER_MIN` bytes needed.
    let rec = page.get(rec_off..)?; // unbounded — we validate offsets below

    // Verify we have at least the fixed header.
    if rec.len() < RECORD_HEADER_MIN {
        return None;
    }

    // Record size (LE u32 at rec[0..4]).
    let record_size = read_le_u32(rec, 0)? as usize;
    if record_size < RECORD_HEADER_MIN {
        return None;
    }
    // Ensure the record doesn't claim to extend beyond the page.
    let rec_end = rec_off.checked_add(record_size)?;
    let rec_full = page.get(rec_off..rec_end)?;

    // Flags (LE u32 at rec[8..12]).
    let flags = read_le_u32(rec_full, 8)?;
    let secure = (flags & 1) != 0;

    // String offsets (all LE u32, all relative to record start).
    // Byte positions within the record header:
    //   [+16..+20] `url_offset`
    //   [+20..+24] `name_offset`
    //   [+24..+28] `path_offset`
    //   [+28..+32] `value_offset`
    let url_off = read_le_u32(rec_full, 16)? as usize;
    let name_off = read_le_u32(rec_full, 20)? as usize;
    let path_off = read_le_u32(rec_full, 24)? as usize;
    let val_off = read_le_u32(rec_full, 28)? as usize;

    // Expiry double (LE f64 at rec[32..40]).
    let mac_expiry = read_le_f64(rec_full, 32)?;
    // `mac_expiry` = 0.0 may mean session cookie; still convert.
    // Truncation is intentional: sub-second precision is not needed for expiry.
    #[allow(clippy::cast_possible_truncation)]
    let expires_unix: i64 = (mac_expiry as i64).checked_add(MAC_EPOCH_DELTA)?;

    // Read NUL-terminated strings from within `rec_full`.
    let host = read_cstr(rec_full, url_off)?;
    let name = read_cstr(rec_full, name_off)?;
    let path = read_cstr(rec_full, path_off)?;
    let value = read_cstr(rec_full, val_off)?;

    Some(RawRow {
        host,
        name,
        path,
        secure,
        expires_unix: Some(expires_unix),
        encrypted_value: vec![],
        plaintext_value: Some(value),
    })
}

// ---------------------------------------------------------------------------
// File reader
// ---------------------------------------------------------------------------

/// Read Safari cookies from a `Cookies.binarycookies` file at `path`.
///
/// Reads the file into memory, calls [`parse_binarycookies`], then routes
/// every row through [`gate`].  Host filtering (INV-2) is performed by the
/// gate — this function does **not** filter by domain before the gate.
///
/// # Errors
///
/// - [`WristbandError::Io`] — the file cannot be read.
/// - [`WristbandError::Parse`] — the file is not a valid `binarycookies` file.
pub(crate) fn read_safari(path: &Path, allow: &[Domain]) -> Result<Vec<Cookie>, WristbandError> {
    let bytes = std::fs::read(path).map_err(|e| WristbandError::Io(e.to_string()))?;
    let rows = parse_binarycookies(&bytes)?;
    Ok(gate(rows, allow, |_| None))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::io::Write as _;

    use tempfile::NamedTempFile;

    use super::*;

    // -----------------------------------------------------------------------
    // Byte-buffer builder helpers
    // -----------------------------------------------------------------------

    /// Build a 4-byte big-endian u32.
    fn be32(n: u32) -> [u8; 4] {
        n.to_be_bytes()
    }

    /// Build a 4-byte little-endian u32.
    fn le32(n: u32) -> [u8; 4] {
        n.to_le_bytes()
    }

    /// Build an 8-byte little-endian f64.
    fn le_f64(f: f64) -> [u8; 8] {
        f.to_le_bytes()
    }

    /// Encode a NUL-terminated C string.
    fn cstr(s: &str) -> Vec<u8> {
        let mut v = s.as_bytes().to_vec();
        v.push(0);
        v
    }

    /// Build a single cookie record (all LE, strings after the fixed header).
    ///
    /// Layout used:
    ///   `[0..4]`   `record_size` (filled in by this function)
    ///   `[4..8]`   unknown (zeros)
    ///   `[8..12]`  flags (bit 0 = secure)
    ///   `[12..16]` unknown (zeros)
    ///   `[16..20]` `url_offset`  (from record start)
    ///   `[20..24]` `name_offset`
    ///   `[24..28]` `path_offset`
    ///   `[28..32]` `value_offset`
    ///   `[32..40]` expiry f64 mac absolute time
    ///   `[40..48]` creation f64 (unused, set to 0)
    ///   `[48..]`   string data
    fn build_record(
        domain: &str,
        name: &str,
        path: &str,
        value: &str,
        secure: bool,
        mac_expiry: f64,
    ) -> Vec<u8> {
        // String data, each NUL-terminated.
        // Offsets are from the start of the record; strings start at byte 48.
        let url_bytes = cstr(domain);
        let name_bytes = cstr(name);
        let path_bytes = cstr(path);
        let value_bytes = cstr(value);

        // These casts are safe in tests: test strings are tiny and always < u32::MAX.
        #[allow(clippy::cast_possible_truncation)]
        let url_off: u32 = 48;
        #[allow(clippy::cast_possible_truncation)]
        let name_off: u32 = url_off + url_bytes.len() as u32;
        #[allow(clippy::cast_possible_truncation)]
        let path_off: u32 = name_off + name_bytes.len() as u32;
        #[allow(clippy::cast_possible_truncation)]
        let value_off: u32 = path_off + path_bytes.len() as u32;

        let mut record: Vec<u8> = Vec::new();
        record.extend_from_slice(&[0u8; 4]); // record_size placeholder
        record.extend_from_slice(&[0u8; 4]); // unknown
        record.extend_from_slice(&le32(u32::from(secure))); // flags
        record.extend_from_slice(&[0u8; 4]); // unknown
        record.extend_from_slice(&le32(url_off));
        record.extend_from_slice(&le32(name_off));
        record.extend_from_slice(&le32(path_off));
        record.extend_from_slice(&le32(value_off));
        record.extend_from_slice(&le_f64(mac_expiry));
        record.extend_from_slice(&le_f64(0.0)); // creation (ignored)
        record.extend_from_slice(&url_bytes);
        record.extend_from_slice(&name_bytes);
        record.extend_from_slice(&path_bytes);
        record.extend_from_slice(&value_bytes);

        // Write the actual record size.
        #[allow(clippy::cast_possible_truncation)]
        let size = record.len() as u32;
        record[0..4].copy_from_slice(&le32(size));

        record
    }

    /// Build a complete `binarycookies` file with one page containing `records`.
    fn build_file(records: &[Vec<u8>]) -> Vec<u8> {
        // Build the page.
        #[allow(clippy::cast_possible_truncation)]
        let cookie_count = records.len() as u32;

        // Cookie offsets within the page: each starts after the page header.
        //   page header = 4 (sig) + 4 (count) + count*4 (offsets) = 8 + count*4 bytes
        let page_header_size = 8 + records.len() * 4;
        let mut offsets: Vec<u32> = Vec::new();
        let mut pos: usize = page_header_size;
        for rec in records {
            #[allow(clippy::cast_possible_truncation)]
            offsets.push(pos as u32);
            pos += rec.len();
        }

        let mut page: Vec<u8> = Vec::new();
        page.extend_from_slice(PAGE_SIG);
        page.extend_from_slice(&le32(cookie_count));
        for off in &offsets {
            page.extend_from_slice(&le32(*off));
        }
        for rec in records {
            page.extend_from_slice(rec);
        }

        #[allow(clippy::cast_possible_truncation)]
        let page_size = page.len() as u32;

        // File header.
        let mut file: Vec<u8> = Vec::new();
        file.extend_from_slice(MAGIC);
        file.extend_from_slice(&be32(1)); // 1 page
        file.extend_from_slice(&be32(page_size)); // page 0 size
        file.extend_from_slice(&page);
        file
    }

    // -----------------------------------------------------------------------
    // Test 1: valid two-cookie file — gate filters to allowed domain only
    // -----------------------------------------------------------------------

    /// A mac-epoch expiry of `640_000_000.0` seconds.
    /// Unix = `640_000_000` + `978_307_200` = `1_618_307_200`.
    const MAC_EXPIRY: f64 = 640_000_000.0;
    const UNIX_EXPIRY: i64 = 640_000_000 + 978_307_200;

    #[test]
    fn valid_file_gate_filters_to_allowed_domain() {
        let rec_astro = build_record(
            "astro.com",
            "session",
            "/app",
            "tok_astro",
            true,
            MAC_EXPIRY,
        );
        let rec_evil = build_record("evil.net", "steal", "/", "tok_evil", false, MAC_EXPIRY);
        let bytes = build_file(&[rec_astro, rec_evil]);

        // Write to a tempfile so read_safari can open it.
        let mut tmp = NamedTempFile::new().unwrap();
        tmp.write_all(&bytes).unwrap();

        let allow = [Domain::explicit("astro.com").unwrap()];
        let cookies = read_safari(tmp.path(), &allow).expect("read_safari");

        assert_eq!(cookies.len(), 1, "gate must keep only astro.com");
        let c = &cookies[0];
        assert_eq!(c.host, "astro.com");
        assert_eq!(c.name, "session");
        assert_eq!(c.path, "/app");
        assert_eq!(c.value, "tok_astro");
        assert!(c.secure, "secure flag must be set");
        assert_eq!(
            c.expires_unix,
            Some(UNIX_EXPIRY),
            "expiry must be unix (mac+978307200)"
        );
    }

    #[test]
    fn gate_keeps_evil_when_allowed() {
        let rec_evil = build_record("evil.net", "x", "/", "ev", false, MAC_EXPIRY);
        let bytes = build_file(&[rec_evil]);
        let mut tmp = NamedTempFile::new().unwrap();
        tmp.write_all(&bytes).unwrap();

        let allow = [Domain::explicit("evil.net").unwrap()];
        let cookies = read_safari(tmp.path(), &allow).expect("read_safari");
        assert_eq!(cookies.len(), 1);
        assert_eq!(cookies[0].host, "evil.net");
    }

    #[test]
    fn two_cookies_same_domain_different_values() {
        let rec1 = build_record("astro.com", "alpha", "/", "val_alpha", false, MAC_EXPIRY);
        let rec2 = build_record(
            "astro.com",
            "beta",
            "/sub",
            "val_beta",
            true,
            MAC_EXPIRY + 1.0,
        );
        let bytes = build_file(&[rec1, rec2]);
        let mut tmp = NamedTempFile::new().unwrap();
        tmp.write_all(&bytes).unwrap();

        let allow = [Domain::explicit("astro.com").unwrap()];
        let cookies = read_safari(tmp.path(), &allow).expect("read_safari");
        assert_eq!(cookies.len(), 2);
        let names: Vec<&str> = cookies.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"alpha"));
        assert!(names.contains(&"beta"));
    }

    // -----------------------------------------------------------------------
    // Test 2: parse_binarycookies — pure unit tests (no I/O)
    // -----------------------------------------------------------------------

    #[test]
    fn parse_returns_correct_row_count() {
        let rec1 = build_record("a.com", "n1", "/", "v1", false, 0.0);
        let rec2 = build_record("b.com", "n2", "/", "v2", false, 0.0);
        let rec3 = build_record("c.com", "n3", "/", "v3", true, 0.0);
        let bytes = build_file(&[rec1, rec2, rec3]);
        let rows = parse_binarycookies(&bytes).unwrap();
        assert_eq!(rows.len(), 3);
    }

    #[test]
    fn parse_secure_flag_bit() {
        let rec_secure = build_record("a.com", "s", "/", "v", true, 0.0);
        let rec_plain = build_record("b.com", "p", "/", "v", false, 0.0);
        let bytes = build_file(&[rec_secure, rec_plain]);
        let rows = parse_binarycookies(&bytes).unwrap();
        let secure_row = rows.iter().find(|r| r.host == "a.com").unwrap();
        let plain_row = rows.iter().find(|r| r.host == "b.com").unwrap();
        assert!(secure_row.secure);
        assert!(!plain_row.secure);
    }

    #[test]
    fn parse_expiry_conversion() {
        // mac_expiry = 0.0 → unix = 978_307_200
        let rec = build_record("a.com", "n", "/", "v", false, 0.0);
        let bytes = build_file(&[rec]);
        let rows = parse_binarycookies(&bytes).unwrap();
        assert_eq!(rows[0].expires_unix, Some(978_307_200));
    }

    #[test]
    fn parse_plaintext_value_is_some() {
        let rec = build_record("a.com", "tok", "/", "myval", false, 0.0);
        let bytes = build_file(&[rec]);
        let rows = parse_binarycookies(&bytes).unwrap();
        assert_eq!(rows[0].plaintext_value.as_deref(), Some("myval"));
        assert!(rows[0].encrypted_value.is_empty());
    }

    // -----------------------------------------------------------------------
    // Test 3: malformed-input panic-safety table
    // -----------------------------------------------------------------------
    //
    // NONE of these must panic.  Each must return Err(_) or Ok(empty vec).

    #[test]
    fn malformed_empty_file() {
        let result = parse_binarycookies(b"");
        assert!(result.is_err(), "empty file must be Err");
    }

    #[test]
    fn malformed_bad_magic() {
        let result = parse_binarycookies(b"NOPE");
        assert!(result.is_err(), "bad magic must be Err");
    }

    #[test]
    fn malformed_just_magic_no_count() {
        // Only 4 bytes: magic but no page count.
        let result = parse_binarycookies(b"cook");
        assert!(result.is_err(), "truncated after magic must be Err");
    }

    #[test]
    fn malformed_huge_page_count_no_pages() {
        // Magic + page_count=0xFFFFFFFF, no actual page size entries.
        let mut buf = Vec::new();
        buf.extend_from_slice(b"cook");
        buf.extend_from_slice(&be32(0xFFFF_FFFF));
        let result = parse_binarycookies(&buf);
        assert!(result.is_err(), "huge page count with no data must be Err");
    }

    #[test]
    fn malformed_page_count_but_truncated_sizes() {
        // Claim 3 pages but only provide 1 size entry.
        let mut buf = Vec::new();
        buf.extend_from_slice(b"cook");
        buf.extend_from_slice(&be32(3));
        buf.extend_from_slice(&be32(100)); // only 1 of the 3 promised size entries
        let result = parse_binarycookies(&buf);
        assert!(result.is_err(), "truncated size table must be Err");
    }

    #[test]
    fn malformed_page_size_beyond_eof() {
        // One page of size 999_999 but the file is only 20 bytes total.
        let mut buf = Vec::new();
        buf.extend_from_slice(b"cook");
        buf.extend_from_slice(&be32(1));
        buf.extend_from_slice(&be32(999_999));
        buf.extend_from_slice(&[0u8; 8]); // stub page data (far too short)
        let result = parse_binarycookies(&buf);
        assert!(result.is_err(), "page extends past EOF must be Err");
    }

    #[test]
    fn malformed_cookie_offset_past_page() {
        // Build a page with cookie_count=1, but the offset points past page end.
        let mut page: Vec<u8> = Vec::new();
        page.extend_from_slice(PAGE_SIG);
        page.extend_from_slice(&le32(1)); // cookie_count = 1
        page.extend_from_slice(&le32(99_999)); // offset WAY past page end

        #[allow(clippy::cast_possible_truncation)]
        let page_size = page.len() as u32;
        let mut file: Vec<u8> = Vec::new();
        file.extend_from_slice(b"cook");
        file.extend_from_slice(&be32(1));
        file.extend_from_slice(&be32(page_size));
        file.extend_from_slice(&page);

        // Must not panic; the bad record is skipped.
        let result = parse_binarycookies(&file);
        assert!(
            result.is_ok(),
            "bad cookie offset should skip record, not panic"
        );
        assert!(result.unwrap().is_empty(), "no valid records expected");
    }

    #[test]
    fn malformed_string_no_nul_terminator() {
        // Build a record where the string data has no NUL byte.
        let mut record = build_record("a.com", "n", "/", "v", false, 0.0);
        // Overwrite all string data bytes to remove NUL terminators.
        for b in &mut record[48..] {
            *b = b'x'; // overwrite NULs
        }
        // Re-write the record size (unchanged, but reset for clarity).
        #[allow(clippy::cast_possible_truncation)]
        let sz = record.len() as u32;
        record[0..4].copy_from_slice(&le32(sz));

        let bytes = build_file(&[record]);

        // Must not panic; the record is skipped due to missing NUL.
        let result = parse_binarycookies(&bytes);
        assert!(result.is_ok(), "missing NUL in string must not panic");
        assert!(
            result.unwrap().is_empty(),
            "record with no NUL must be skipped"
        );
    }

    #[test]
    fn malformed_record_size_too_small() {
        // Build a valid record then overwrite its size field with something < RECORD_HEADER_MIN.
        let mut record = build_record("a.com", "n", "/", "v", false, 0.0);
        record[0..4].copy_from_slice(&le32(4)); // claim record is only 4 bytes
        let bytes = build_file(&[record]);
        let result = parse_binarycookies(&bytes);
        assert!(result.is_ok());
        assert!(
            result.unwrap().is_empty(),
            "record with tiny size must be skipped"
        );
    }

    #[test]
    fn malformed_record_size_exceeds_page() {
        // Build a valid record then overwrite its size to claim it extends beyond the page.
        let mut record = build_record("a.com", "n", "/", "v", false, 0.0);
        record[0..4].copy_from_slice(&le32(0x00FF_FFFF)); // enormous size
        let bytes = build_file(&[record]);
        let result = parse_binarycookies(&bytes);
        assert!(result.is_ok());
        assert!(
            result.unwrap().is_empty(),
            "oversized record must be skipped"
        );
    }

    #[test]
    fn malformed_page_bad_signature() {
        // Replace the page signature with garbage.
        let rec = build_record("a.com", "n", "/", "v", false, 0.0);
        let mut bytes = build_file(&[rec]);
        // Find the page body (after file header = 8 + 1*4 = 12 bytes) and corrupt sig.
        let page_start = 12;
        bytes[page_start] = 0xFF;
        // The page is skipped, no rows returned, no panic.
        let result = parse_binarycookies(&bytes);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn malformed_zero_pages() {
        // Claim 0 pages — should produce Ok(empty).
        let mut buf = Vec::new();
        buf.extend_from_slice(b"cook");
        buf.extend_from_slice(&be32(0));
        let result = parse_binarycookies(&buf);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    // -----------------------------------------------------------------------
    // Sanity-cap tests: file-derived counts must not trigger huge allocations
    // -----------------------------------------------------------------------

    #[test]
    fn rejects_absurd_page_count_without_huge_alloc() {
        // magic "cook" + BE page_count = 0x00FFFFFF, then nothing else.
        // The sanity cap fires before the truncation check (cap is checked
        // immediately after reading page_count) so no huge allocation is
        // attempted and no panic occurs.
        let mut bytes = b"cook".to_vec();
        bytes.extend_from_slice(&0x00FF_FFFFu32.to_be_bytes());
        let result = parse_binarycookies(&bytes);
        // must be a parse error, not a panic / OOM
        assert!(
            matches!(result, Err(WristbandError::Parse(_))),
            "expected Err(Parse(_)), got Ok"
        );
    }

    #[test]
    fn rejects_absurd_cookie_count_without_huge_alloc() {
        // Build a page with cookie_count just above MAX_COOKIE_COUNT and no
        // offset table — the cap fires immediately after reading the count
        // (before any allocation or arithmetic on it), so the page is skipped
        // and no huge Vec is allocated.
        #[allow(clippy::cast_possible_truncation)]
        let cookie_count: u32 = (MAX_COOKIE_COUNT as u32) + 1;

        let mut page: Vec<u8> = Vec::new();
        page.extend_from_slice(PAGE_SIG);
        page.extend_from_slice(&cookie_count.to_le_bytes());
        // No offset table supplied — cap fires before the truncation check.

        #[allow(clippy::cast_possible_truncation)]
        let page_size = page.len() as u32;
        let mut file: Vec<u8> = Vec::new();
        file.extend_from_slice(b"cook");
        file.extend_from_slice(&1u32.to_be_bytes()); // 1 page
        file.extend_from_slice(&page_size.to_be_bytes());
        file.extend_from_slice(&page);

        // parse_page silently skips the page; the file itself is structurally
        // valid so parse_binarycookies returns Ok(empty) — no panic/OOM.
        let result = parse_binarycookies(&file).expect("should not be Err at file level");
        assert!(
            result.is_empty(),
            "page with absurd cookie_count must yield no rows"
        );
    }
}
