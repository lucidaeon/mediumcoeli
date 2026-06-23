//! On-demand SPK fetching from the JPL Horizons API.
//!
//! Horizons generates a binary SPK (`.bsp`) for a single small body per
//! request. We use it to acquire ephemerides for bodies not in the bundled
//! `sb441` files — centaurs, KBOs, TNOs, and the outer dwarf planets — in the
//! same DAF/SPK format [`crate::spk`] already reads.
//!
//! # The id scheme gotcha
//!
//! A Horizons-generated SPK stamps its segment target NAIF id as
//! `20_000_000 + MPC#` (Chiron → `20002060`), **not** the `2_000_000 + MPC#`
//! used by the bundled `sb441` files. Query and name Horizons files by the
//! `20`-prefixed id (see [`crate::placements::Placement::horizons_naif_id`]).
//!
//! # Defaults & courtesy
//!
//! [`DEFAULT_START`]..[`DEFAULT_STOP`] spans Uranus's discovery to the 2038
//! 32-bit `time_t` overflow. [`fetch_all`] fetches sequentially with a
//! [`THROTTLE`] delay between requests — be a good netizen; never parallel-flood
//! JPL.
//!
//! This module is compiled only under the `horizons` feature.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// JPL Horizons API endpoint.
pub const API_URL: &str = "https://ssd.jpl.nasa.gov/api/horizons.api";

/// Default SPK span start — Uranus's discovery, 1781-03-13 00:00:00 UTC.
pub const DEFAULT_START: &str = "1781-03-13 00:00:00";

/// Default SPK span stop — the signed 32-bit `time_t` overflow,
/// 2038-01-19 03:14:07 UTC.
pub const DEFAULT_STOP: &str = "2038-01-19 03:14:07";

/// Delay between sequential Horizons requests, to stay a polite API citizen.
pub const THROTTLE: Duration = Duration::from_millis(500);

/// The default `(start, stop)` span when the caller supplies none.
#[must_use]
pub fn default_span() -> (&'static str, &'static str) {
    (DEFAULT_START, DEFAULT_STOP)
}

/// Errors from fetching or decoding a Horizons SPK.
#[derive(Debug, thiserror::Error)]
pub enum HorizonsError {
    /// The HTTP request itself failed (network, DNS, non-2xx status).
    #[error("Horizons HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),
    /// The response body was not valid JSON.
    #[error("Horizons response was not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
    /// The response carried no `spk` field — Horizons' own message is included.
    #[error("Horizons returned no SPK: {0}")]
    NoSpk(String),
    /// The `spk` field was not valid base64.
    #[error("Horizons SPK was not valid base64: {0}")]
    Base64(#[from] base64::DecodeError),
    /// Writing the decoded `.bsp` to disk failed.
    #[error("writing SPK to {path}: {source}")]
    Io {
        /// The path we tried to write.
        path: PathBuf,
        /// The underlying I/O error.
        source: std::io::Error,
    },
}

/// One body to fetch: the Horizons `COMMAND` designator plus the NAIF id used
/// to name the output file (`<naif_id>.bsp`).
#[derive(Debug, Clone)]
pub struct FetchTarget {
    /// Display label for progress reporting (e.g. `"Chiron"`).
    pub label: String,
    /// Horizons `COMMAND` value, e.g. `"2060;"`.
    pub command: String,
    /// Segment NAIF id (`20_000_000 + mpc`); names the file and is what the
    /// SPK reader queries.
    pub naif_id: i32,
}

/// The query parameters for an SPK request. Pure — no network — so it can be
/// asserted in tests.
fn spk_query(command: &str, start: &str, stop: &str) -> [(&'static str, String); 6] {
    [
        ("format", "json".to_string()),
        ("EPHEM_TYPE", "SPK".to_string()),
        ("OBJ_DATA", "NO".to_string()),
        ("COMMAND", command.to_string()),
        // Single-quote the time values: Horizons' batch parser otherwise splits
        // a datetime on its space ("Too many constants"). Quoting is stripped by
        // Horizons and works for both `YYYY-MM-DD` and `YYYY-MM-DD HH:MM:SS`.
        ("START_TIME", format!("'{start}'")),
        ("STOP_TIME", format!("'{stop}'")),
    ]
}

/// Extract the decoded SPK bytes from a Horizons JSON response body.
///
/// On success the `spk` field (base64) is decoded to the raw `.bsp` bytes. If
/// there is no `spk` field the request failed; Horizons' own `error`/`result`
/// text is surfaced in [`HorizonsError::NoSpk`].
///
/// # Errors
/// [`HorizonsError::Json`] if the body is not JSON, [`HorizonsError::Base64`]
/// if the `spk` field is malformed, or [`HorizonsError::NoSpk`] otherwise.
pub fn parse_spk_response(body: &[u8]) -> Result<Vec<u8>, HorizonsError> {
    let v: serde_json::Value = serde_json::from_slice(body)?;
    if let Some(spk) = v.get("spk").and_then(serde_json::Value::as_str) {
        // Horizons line-wraps the base64 payload; strip ASCII whitespace
        // (newlines, spaces) before decoding — the strict engine rejects it.
        let compact: String = spk.split_ascii_whitespace().collect();
        return Ok(BASE64.decode(compact)?);
    }
    let msg = v
        .get("error")
        .and_then(serde_json::Value::as_str)
        .or_else(|| v.get("result").and_then(serde_json::Value::as_str))
        .unwrap_or("response contained no `spk` field")
        .trim()
        .to_string();
    Err(HorizonsError::NoSpk(msg))
}

/// Fetch one body's SPK from Horizons and return the raw `.bsp` bytes.
///
/// # Errors
/// Any [`HorizonsError`] except [`HorizonsError::Io`] (this does not write).
pub fn fetch_spk(command: &str, start: &str, stop: &str) -> Result<Vec<u8>, HorizonsError> {
    let client = reqwest::blocking::Client::new();
    let resp = client
        .get(API_URL)
        .query(&spk_query(command, start, stop))
        .send()?
        .error_for_status()?;
    let body = resp.bytes()?;
    parse_spk_response(&body)
}

/// Fetch several bodies sequentially into `dir`, writing `<naif_id>.bsp` each.
///
/// Requests are spaced by [`THROTTLE`] to be courteous to JPL. `progress` is
/// invoked once per target with the outcome — `Ok((path, bytes_written))` or a
/// reference to the error — so a CLI or GUI can report as it goes. A single
/// body's failure does not abort the batch; the function returns the number of
/// failures.
///
/// # Errors
/// [`HorizonsError::Io`] only if `dir` cannot be created. Per-body fetch/write
/// errors are delivered to `progress` and counted, not returned.
pub fn fetch_all(
    targets: &[FetchTarget],
    dir: &Path,
    start: &str,
    stop: &str,
    mut progress: impl FnMut(&FetchTarget, Result<(&Path, usize), &HorizonsError>),
) -> Result<usize, HorizonsError> {
    std::fs::create_dir_all(dir).map_err(|source| HorizonsError::Io {
        path: dir.to_path_buf(),
        source,
    })?;
    let mut failures = 0;
    for (i, target) in targets.iter().enumerate() {
        if i > 0 {
            std::thread::sleep(THROTTLE);
        }
        match fetch_one(target, dir, start, stop) {
            Ok((path, n)) => progress(target, Ok((&path, n))),
            Err(e) => {
                failures += 1;
                progress(target, Err(&e));
            }
        }
    }
    Ok(failures)
}

/// Fetch one target and write `<naif_id>.bsp` into `dir`.
fn fetch_one(
    target: &FetchTarget,
    dir: &Path,
    start: &str,
    stop: &str,
) -> Result<(PathBuf, usize), HorizonsError> {
    let bytes = fetch_spk(&target.command, start, stop)?;
    let path = dir.join(format!("{}.bsp", target.naif_id));
    std::fs::write(&path, &bytes).map_err(|source| HorizonsError::Io {
        path: path.clone(),
        source,
    })?;
    Ok((path, bytes.len()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spk_query_carries_the_required_params() {
        let q = spk_query("2060;", "1781-03-13 00:00:00", "2038-01-19 03:14:07");
        let get = |k: &str| q.iter().find(|(n, _)| *n == k).map(|(_, v)| v.as_str());
        assert_eq!(get("format"), Some("json"));
        assert_eq!(get("EPHEM_TYPE"), Some("SPK"));
        assert_eq!(get("OBJ_DATA"), Some("NO"));
        assert_eq!(get("COMMAND"), Some("2060;"));
        // Time values are single-quoted on the wire (Horizons batch-parser quirk).
        assert_eq!(get("START_TIME"), Some("'1781-03-13 00:00:00'"));
        assert_eq!(get("STOP_TIME"), Some("'2038-01-19 03:14:07'"));
    }

    #[test]
    fn parse_spk_response_decodes_the_spk_field() {
        let raw = b"DAF/SPK fake-bytes";
        let b64 = BASE64.encode(raw);
        let body = format!(
            r#"{{"signature":{{"version":"1.2","source":"NASA/JPL Horizons API"}},"spk":"{b64}","spk_file_id":20002060}}"#
        );
        let out = parse_spk_response(body.as_bytes()).unwrap();
        assert_eq!(out, raw);
    }

    #[test]
    fn parse_spk_response_decodes_line_wrapped_base64() {
        // Horizons returns the base64 wrapped across lines; the decoder must
        // tolerate embedded newlines/whitespace (regression: strict decode).
        let raw = b"DAF/SPK wrapped-payload-bytes-long-enough-to-wrap";
        let b64 = BASE64.encode(raw);
        let wrapped: String = b64
            .as_bytes()
            .chunks(16)
            .map(|c| std::str::from_utf8(c).unwrap())
            .collect::<Vec<_>>()
            .join("\n");
        // Build via serde_json so the embedded newlines are escaped exactly as
        // Horizons sends them (`\n` in the JSON string, not raw bytes).
        let body = serde_json::json!({ "spk": wrapped, "spk_file_id": 1 }).to_string();
        let out = parse_spk_response(body.as_bytes()).unwrap();
        assert_eq!(out, raw);
    }

    #[test]
    fn parse_spk_response_surfaces_horizons_message_when_no_spk() {
        let body = br#"{"signature":{"version":"1.2"},"result":"No matches found.\n"}"#;
        match parse_spk_response(body) {
            Err(HorizonsError::NoSpk(msg)) => assert_eq!(msg, "No matches found."),
            other => panic!("expected NoSpk, got {other:?}"),
        }
    }

    #[test]
    fn parse_spk_response_rejects_non_json() {
        assert!(matches!(
            parse_spk_response(b"<html>504 gateway</html>"),
            Err(HorizonsError::Json(_))
        ));
    }

    #[test]
    fn default_span_is_uranus_to_y2038() {
        assert_eq!(default_span(), (DEFAULT_START, DEFAULT_STOP));
        assert_eq!(DEFAULT_START, "1781-03-13 00:00:00");
        assert_eq!(DEFAULT_STOP, "2038-01-19 03:14:07");
    }

    /// Live end-to-end fetch against the real JPL Horizons API. Ignored by
    /// default (hits the network); run with `--ignored` to exercise it.
    #[test]
    #[ignore = "hits live JPL Horizons API"]
    fn live_fetch_chiron_one_year() {
        let bytes = fetch_spk("2060;", "2000-01-01", "2001-01-01").unwrap();
        assert!(
            bytes.starts_with(b"DAF/SPK "),
            "not an SPK: {:?}",
            &bytes[..8]
        );
    }
}
