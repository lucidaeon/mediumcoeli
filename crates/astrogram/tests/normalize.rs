use astrogram::normalize::normalize_cp1252_str;

// --- basic passthrough ---

#[test]
fn ascii_unchanged() {
    assert_eq!(normalize_cp1252_str("Amber Celeste"), "Amber Celeste");
}

#[test]
fn empty_unchanged() {
    assert_eq!(normalize_cp1252_str(""), "");
}

// --- cp1252-representable chars are kept ---

#[test]
fn latin1_accents_kept() {
    // è U+00E8 = cp1252 0xE8; É U+00C9 = cp1252 0xC9
    assert_eq!(normalize_cp1252_str("Adèle Haenel"), "Adèle Haenel");
    assert_eq!(
        normalize_cp1252_str("École Polytechnique"),
        "École Polytechnique"
    );
}

#[test]
fn cp1252_extras_kept() {
    // € U+20AC = cp1252 0x80
    assert_eq!(normalize_cp1252_str("€100"), "€100");
}

#[test]
fn magic_quotes_kept() {
    // All four smart quotes map to cp1252 0x91–0x94 — must be preserved, not stripped.
    assert_eq!(
        normalize_cp1252_str("\u{2018}hello\u{2019}"),
        "\u{2018}hello\u{2019}"
    ); // '' 0x91/0x92
    assert_eq!(
        normalize_cp1252_str("\u{201C}hello\u{201D}"),
        "\u{201C}hello\u{201D}"
    ); // "" 0x93/0x94
    assert_eq!(normalize_cp1252_str("it\u{2019}s"), "it\u{2019}s"); // common contraction
}

// --- out-of-cp1252 chars stripped ---

#[test]
fn emoji_at_end_stripped() {
    // ♒ U+2652, 🌙 U+1F319 — not in cp1252
    assert_eq!(normalize_cp1252_str("Amanda ♒️"), "Amanda");
}

#[test]
fn emoji_in_middle_space_collapsed() {
    assert_eq!(normalize_cp1252_str("Emmy 🐋 Komarczyk"), "Emmy Komarczyk");
}

#[test]
fn only_emoji_becomes_empty() {
    assert_eq!(normalize_cp1252_str("🌙♒️"), "");
}

#[test]
fn astrological_run_stripped() {
    // ♒ ️🌙 ❓ ⬆ ️ — none are cp1252
    assert_eq!(
        normalize_cp1252_str("Amanda Musgrove ♒️🌙❓⬆️"),
        "Amanda Musgrove"
    );
}

#[test]
fn eclipse_with_symbols() {
    assert_eq!(
        normalize_cp1252_str("Eclipse 2024.03.13 ℧☾ 24°♍️"),
        "Eclipse 2024.03.13 24°"
    );
}

// --- whitespace normalisation ---

#[test]
fn leading_whitespace_trimmed() {
    assert_eq!(normalize_cp1252_str("  Amber"), "Amber");
}

#[test]
fn trailing_whitespace_trimmed() {
    assert_eq!(normalize_cp1252_str("Amber  "), "Amber");
}

#[test]
fn double_space_collapsed() {
    assert_eq!(normalize_cp1252_str("Amber  Celeste"), "Amber Celeste");
}

#[test]
fn tabs_and_newlines_collapsed() {
    assert_eq!(normalize_cp1252_str("Amber\t\nCeleste"), "Amber Celeste");
}
