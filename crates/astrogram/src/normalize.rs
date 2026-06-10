//! Character-set normalization for cp1252-target string fields.
//!
//! `SFcht` stores all text as Windows-1252 (cp1252). Characters outside that
//! encoding are silently replaced by `encoding_rs` with HTML numeric character
//! references (`&#xNNNN;`), which Solar Fire then renders literally.
//!
//! [`normalize_cp1252_str`] strips non-cp1252 characters and collapses the
//! resulting whitespace, giving clean field values before the `SFcht` writer
//! ever sees them.

use crate::chart::Chart;

/// Strip characters not representable in cp1252, then collapse whitespace.
///
/// Keeps any char that encodes to exactly one byte in Windows-1252 without
/// error (covers ASCII, Latin-1 Supplement, and the cp1252-specific extras
/// like `€`, smart quotes, and `…`). Everything else — emojis, astrological
/// Unicode symbols, variation selectors — is removed.
///
/// Whitespace is normalised with `split_whitespace` / `join(" ")`, which
/// trims leading/trailing space and collapses all internal runs.
#[must_use]
pub fn normalize_cp1252_str(s: &str) -> String {
    let filtered: String = s.chars().filter(|&ch| is_cp1252(ch)).collect();
    filtered.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Apply [`normalize_cp1252_str`] to every text field of `chart` in place.
///
/// Normalises: `name`, `secondary_name`, `city`, `region`, `source_rating`,
/// `notes`, and all `sub_charts` recursively.
pub fn normalize_chart(chart: &mut Chart) {
    chart.name = normalize_cp1252_str(&chart.name);
    if let Some(v) = &chart.secondary_name {
        chart.secondary_name = Some(normalize_cp1252_str(v));
    }
    if let Some(v) = &chart.city {
        chart.city = Some(normalize_cp1252_str(v));
    }
    if let Some(v) = &chart.region {
        chart.region = Some(normalize_cp1252_str(v));
    }
    if let Some(v) = &chart.source_rating {
        chart.source_rating = Some(normalize_cp1252_str(v));
    }
    if let Some(v) = &chart.notes {
        chart.notes = Some(normalize_cp1252_str(v));
    }
    for sub in &mut chart.sub_charts {
        sub.name = normalize_cp1252_str(&sub.name);
        if let Some(v) = &sub.city {
            sub.city = Some(normalize_cp1252_str(v));
        }
        if let Some(v) = &sub.region {
            sub.region = Some(normalize_cp1252_str(v));
        }
        if let Some(v) = &sub.notes {
            sub.notes = Some(normalize_cp1252_str(v));
        }
    }
}

/// Returns `true` if `ch` encodes to exactly one byte in Windows-1252 without
/// substitution. cp1252 is a single-byte encoding, so any valid mapping
/// produces exactly one output byte; unmappable chars produce numeric character
/// references (`&#xNNNN;`) and set the error flag.
fn is_cp1252(ch: char) -> bool {
    let mut buf = [0u8; 4];
    let s = ch.encode_utf8(&mut buf);
    let (encoded, _, had_errors) = encoding_rs::WINDOWS_1252.encode(s);
    !had_errors && encoded.len() == 1
}
