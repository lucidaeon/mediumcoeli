//! Chromium Linux cookie decryption — desktop-environment detection, keyring
//! dispatch, PBKDF2-1 key derivation, and AES-128-CBC (16-space IV, PKCS7).
//!
//! # Cross-platform structure
//!
//! The module is deliberately **not** gated on `#[cfg(target_os = "linux")]` as
//! a whole, so that the pure-logic and pure-crypto portions compile and run on
//! macOS (the development host):
//!
//! - **Cross-platform** (compiled + tested on macOS):
//!   - [`Keyring`] enum
//!   - [`detect_keyring`] — DE-detection table with injected env
//!   - [`derive_key_linux`] — PBKDF2-HMAC-SHA1, 1 iteration
//!   - [`decrypt`] — AES-128-CBC + v10/v11 dispatch + empty-password retry
//!
//! - **Linux-only** (`#[cfg(target_os = "linux")]`):
//!   - `key_for` — shells out to `kwallet-query` / `secret-tool` to fetch
//!     the keyring password; tests are `#[ignore]`d
//!   - The `read_cookies` Linux arm wiring in `lib.rs`
//!
//! # Key derivation
//!
//! `PBKDF2-HMAC-SHA1(password, b"saltysalt", 1)` → 16-byte key.
//! Iteration count is **1** on Linux (contrast: 1003 on macOS).
//!
//! # Cipher
//!
//! AES-128-CBC, IV = 16 × `0x20` (ASCII space), PKCS7 padding — same as macOS.
//!
//! # Prefix / key mapping
//!
//! | Prefix | Password used |
//! |--------|--------------|
//! | `v10`  | `b"peanuts"` (hardcoded) |
//! | `v11`  | keyring password |
//!
//! On decrypt failure, retry with the empty-password-derived key (Chromium
//! bugfix parity). Persistent failure → `None` (graceful).
//!
//! # Subprocess tools used for keyring access (Linux only)
//!
//! - **`KWallet`** variants: `kwallet-query` (D-Bus via the CLI wrapper)
//! - **GNOME Keyring**: `secret-tool search` (part of `libsecret-tools`)

use aes::Aes128;
use cbc::cipher::{BlockDecryptMut, KeyIvInit, block_padding::Pkcs7};
use pbkdf2::pbkdf2_hmac;
use sha1::Sha1;

use crate::chromium::framing::{Key, Scheme, classify, strip_hash};
use crate::cookie::RawRow;

/// AES-128-CBC IV for Chromium Linux: 16 ASCII space bytes (`0x20`).
// Used by decrypt_cbc and tests on all platforms.
const IV: [u8; 16] = [0x20u8; 16];

/// PBKDF2 salt used by all Chromium browsers on Linux.
// Used by derive_key_linux on all platforms.
const SALT: &[u8] = b"saltysalt";

/// PBKDF2 iteration count for Chromium Linux key derivation.
///
/// **Linux uses 1 iteration** (contrast: macOS uses 1003). This is the
/// authoritative source-of-truth; the KAT in tests locks it.
// Used by derive_key_linux on all platforms.
const ITERATIONS: u32 = 1;

// ---------------------------------------------------------------------------
// Keyring enum — cross-platform
// ---------------------------------------------------------------------------

/// The desktop-keyring backend that Chromium will use to store the Safe
/// Storage password on Linux.
///
/// Determined by [`detect_keyring`] from environment variables.
// The Linux arm in lib.rs uses this; on macOS it is exercised only by tests.
#[allow(dead_code)]
// `GnomeKeyring` intentionally ends with the enum name — it is the established
// term for this keyring subsystem (libsecret / gnome-keyring daemon).
#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Keyring {
    /// KDE Wallet 4 (`kwallet-query`, network wallet).
    KWallet,
    /// KDE Wallet 5 (`kwallet-query`, network wallet).
    KWallet5,
    /// KDE Wallet 6 (`kwallet-query`, network wallet).
    KWallet6,
    /// GNOME Keyring / libsecret (`secret-tool search`).
    GnomeKeyring,
    /// No keyring access — use the hardcoded `peanuts` password and treat all
    /// values as v10 (no real v11 encryption is present).
    BasicText,
}

// ---------------------------------------------------------------------------
// Desktop-environment detection — cross-platform, injected env
// ---------------------------------------------------------------------------

/// Detect which keyring backend Chromium would use on this Linux session.
///
/// `env` is an injected environment lookup (`&str` → `Option<String>`), making
/// the function pure and unit-testable without mutating the process environment.
///
/// # Detection order (mirrors Chromium exactly)
///
/// 1. `XDG_CURRENT_DESKTOP` — may be a colon-separated list; each token is
///    checked in order.
/// 2. `DESKTOP_SESSION`.
/// 3. Presence of `GNOME_DESKTOP_SESSION_ID` → `GnomeKeyring`.
/// 4. Presence of `KDE_FULL_SESSION` → KDE version branch (see below).
///
/// # KDE version mapping
///
/// `KDE_SESSION_VERSION` selects the wallet variant:
/// - `"6"` → [`Keyring::KWallet6`]
/// - `"5"` → [`Keyring::KWallet5`]
/// - `"4"` or unset → [`Keyring::KWallet`] (KDE4)
/// - `"3"` or other unrecognised value → [`Keyring::BasicText`]
///   (KDE3 is so old it has no usable kwallet integration)
///
/// # Pitfall — unknown DE defaults to `BasicText`
///
/// Anything unrecognised **falls through to [`Keyring::BasicText`]**, not
/// `GnomeKeyring`. Defaulting to GNOME for unknown DEs would silently drop
/// all v11 cookies on headless hosts, KDE3, `LXQt`, and other environments.
// Called from lib.rs Linux arm and from tests on all platforms.
#[allow(dead_code)]
pub(crate) fn detect_keyring(env: &dyn Fn(&str) -> Option<String>) -> Keyring {
    // Step 1 — XDG_CURRENT_DESKTOP (colon-separated list of tokens)
    if let Some(val) = env("XDG_CURRENT_DESKTOP") {
        for token in val.split(':') {
            if let Some(k) = keyring_for_de_name(token, env) {
                return k;
            }
        }
    }

    // Step 2 — DESKTOP_SESSION (single value)
    if let Some(val) = env("DESKTOP_SESSION")
        && let Some(k) = keyring_for_de_name(&val, env)
    {
        return k;
    }

    // Step 3 — GNOME_DESKTOP_SESSION_ID
    if env("GNOME_DESKTOP_SESSION_ID").is_some() {
        return Keyring::GnomeKeyring;
    }

    // Step 4 — KDE_FULL_SESSION
    if env("KDE_FULL_SESSION").is_some() {
        return kde_keyring(env);
    }

    // Fallthrough — unrecognised / headless → BasicText (pitfall guard)
    Keyring::BasicText
}

/// Map a single DE name token to a [`Keyring`] variant, or `None` if
/// unrecognised (caller continues scanning).
///
/// KDE detection further inspects `KDE_SESSION_VERSION` via `env`.
fn keyring_for_de_name(de: &str, env: &dyn Fn(&str) -> Option<String>) -> Option<Keyring> {
    // Normalise to uppercase for case-insensitive matching.
    let upper = de.to_uppercase();
    let upper = upper.as_str();

    match upper {
        // GNOME-family desktops
        "GNOME" | "X-CINNAMON" | "UNITY" | "PANTHEON" | "XFCE" | "UKUI" | "DEEPIN" => {
            Some(Keyring::GnomeKeyring)
        }

        // KDE — version dispatch
        "KDE" => Some(kde_keyring(env)),

        // LXQt → BasicText (no supported keyring)
        "LXQT" => Some(Keyring::BasicText),

        // Unrecognised — caller keeps scanning other tokens / env vars
        _ => None,
    }
}

/// Determine which KDE wallet variant to use based on `KDE_SESSION_VERSION`.
fn kde_keyring(env: &dyn Fn(&str) -> Option<String>) -> Keyring {
    match env("KDE_SESSION_VERSION").as_deref() {
        Some("6") => Keyring::KWallet6,
        Some("5") => Keyring::KWallet5,
        // KDE4 is signalled by "4" or an absent version var
        Some("4") | None => Keyring::KWallet,
        // KDE3 or other ancient/unrecognised → BasicText (no usable wallet)
        _ => Keyring::BasicText,
    }
}

// ---------------------------------------------------------------------------
// PBKDF2 key derivation — cross-platform
// ---------------------------------------------------------------------------

/// Derive a 16-byte AES key from `password` via PBKDF2-HMAC-SHA1.
///
/// **Linux constants:** salt = `b"saltysalt"`, iterations = **1**, output = 16 bytes.
///
/// This differs from the macOS path (1003 iterations). The iteration count is
/// locked by the known-answer test in the test module.
// Called from decrypt, key_for (Linux), and tests on all platforms.
#[allow(dead_code)]
pub(crate) fn derive_key_linux(password: &[u8]) -> Key {
    let mut key = [0u8; 16];
    pbkdf2_hmac::<Sha1>(password, SALT, ITERATIONS, &mut key);
    Key(key)
}

// ---------------------------------------------------------------------------
// AES-128-CBC decryption helper — cross-platform
// ---------------------------------------------------------------------------

/// Decrypt `ciphertext` (after the `v10`/`v11` prefix has been stripped) with
/// `key`, apply [`strip_hash`] for `meta_version` ≥ 24, and decode as UTF-8.
///
/// Returns `None` on any failure (wrong padding, invalid UTF-8, empty input,
/// non-block-aligned length).
fn decrypt_cbc(ciphertext: &[u8], key: &Key, meta_version: i64) -> Option<String> {
    type Aes128CbcDec = cbc::Decryptor<Aes128>;

    if ciphertext.is_empty() || !ciphertext.len().is_multiple_of(16) {
        return None;
    }

    let mut buf = ciphertext.to_vec();
    let decryptor = Aes128CbcDec::new(&key.0.into(), &IV.into());
    let plaintext_slice = decryptor.decrypt_padded_mut::<Pkcs7>(&mut buf).ok()?;

    let plaintext = plaintext_slice.to_vec();
    let stripped = strip_hash(plaintext, meta_version);
    String::from_utf8(stripped).ok()
}

// ---------------------------------------------------------------------------
// Public(crate) decrypt — cross-platform
// ---------------------------------------------------------------------------

/// Decrypt one Chromium cookie row on Linux.
///
/// # Scheme dispatch
///
/// | [`Scheme`]          | Key used                        |
/// |---------------------|---------------------------------|
/// | [`Scheme::V10`]     | `derive_key_linux(b"peanuts")`  |
/// | [`Scheme::V11`]     | `key` (keyring-derived)         |
/// | [`Scheme::LegacyPlaintext`] | decode `encrypted_value` as UTF-8 |
/// | [`Scheme::LegacyDpapi`]    | `None` (Windows-only blob)        |
///
/// # Retry semantics (Chromium bugfix parity)
///
/// On decrypt failure for v10/v11, the function retries with
/// `derive_key_linux(b"")` (empty-password key). Persistent failure → `None`.
///
/// # `BasicText`
///
/// When `keyring` is [`Keyring::BasicText`], the caller passes `key` derived
/// from `b"peanuts"` and all values are treated as v10.
// Called from lib.rs Linux arm and from tests on all platforms.
#[allow(dead_code)]
pub(crate) fn decrypt(row: &RawRow, key: &Key, meta_version: i64) -> Option<String> {
    match classify(&row.encrypted_value) {
        Scheme::V10 => {
            let v10_key = derive_key_linux(b"peanuts");
            let ct = &row.encrypted_value[3..];
            decrypt_cbc(ct, &v10_key, meta_version).or_else(|| {
                // Retry with empty-password key (Chromium bugfix parity)
                let empty_key = derive_key_linux(b"");
                decrypt_cbc(ct, &empty_key, meta_version)
            })
        }
        Scheme::V11 => {
            let ct = &row.encrypted_value[3..];
            decrypt_cbc(ct, key, meta_version).or_else(|| {
                // Retry with empty-password key
                let empty_key = derive_key_linux(b"");
                decrypt_cbc(ct, &empty_key, meta_version)
            })
        }
        Scheme::LegacyPlaintext => String::from_utf8(row.encrypted_value.clone()).ok(),
        Scheme::LegacyDpapi => None,
    }
}

// ---------------------------------------------------------------------------
// Linux-only: keyring password retrieval via subprocess
// ---------------------------------------------------------------------------

/// Linux-only helper: shell app identifier used in GNOME Keyring lookups.
///
/// `chrome` covers Chrome + Brave + Chromium; `msedge` covers Edge.
#[cfg(target_os = "linux")]
fn gnome_app_attr(browser: crate::Browser) -> &'static str {
    use crate::Browser;
    match browser {
        Browser::Edge => "msedge",
        _ => "chrome",
    }
}

/// Linux-only helper: the "account" string used in GNOME / `KWallet` lookups.
///
/// Maps each browser variant to the account name used in its keyring entry.
#[cfg(target_os = "linux")]
fn browser_account(browser: crate::Browser) -> &'static str {
    use crate::Browser;
    match browser {
        Browser::Chrome => "Chrome",
        Browser::Chromium => "Chromium",
        Browser::Brave => "Brave",
        Browser::Edge => "Microsoft Edge",
        Browser::Opera => "Opera",
        Browser::Vivaldi => "Vivaldi",
        Browser::Whale => "Whale",
        Browser::Firefox | Browser::Safari => {
            unreachable!("Firefox/Safari do not use Chromium keyring")
        }
    }
}

/// Retrieve the raw keyring password for `browser` and `keyring`, then derive
/// and return the AES key.
///
/// # Subprocess tools used
///
/// - **`KWallet`** (`KWallet`, `KWallet5`, `KWallet6`): shells to
///   `kwallet-query --read-password "<Browser> Safe Storage" --folder "<Browser> Keys" kdewallet`
///   (D-Bus access via the CLI wrapper). An exit code indicating "failed to
///   read" is treated as an empty password.
/// - **`GnomeKeyring`**: shells to
///   `secret-tool search --unlock application <app>` first, then falls back
///   to `secret-tool lookup service "<Browser> Safe Storage" account "<Browser>"`.
/// - **`BasicText`**: no I/O — derives from `b"peanuts"` immediately.
///
/// All decryption falls through to `derive_key_linux(b"")` on any retrieval
/// failure, mirroring Chromium's own fallback behaviour.
///
/// # Errors
///
/// Returns [`crate::error::WristbandError::Keychain`] if all retrieval
/// attempts fail and the fallback empty-password path is also unavailable
/// (only on programming error; in practice the empty key is always returned).
#[cfg(target_os = "linux")]
// `Result` is kept for cross-platform parity: the macOS/Windows `key_for` are
// fallible (keychain / DPAPI access), even though the Linux path currently
// always succeeds via the empty-key fallback.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn key_for(
    browser: crate::Browser,
    keyring: Keyring,
) -> Result<Key, crate::error::WristbandError> {
    use std::process::Command;

    let account = browser_account(browser);

    match keyring {
        // BasicText: no keyring I/O — use the peanuts hardcoded key.
        Keyring::BasicText => Ok(derive_key_linux(b"peanuts")),

        // KWallet variants — shell to kwallet-query
        Keyring::KWallet | Keyring::KWallet5 | Keyring::KWallet6 => {
            let folder = format!("{account} Keys");
            let entry = format!("{account} Safe Storage");

            let output = Command::new("kwallet-query")
                .args(["--read-password", &entry, "--folder", &folder, "kdewallet"])
                .output();

            let password = match output {
                Ok(out) if out.status.success() => {
                    let raw = String::from_utf8_lossy(&out.stdout);
                    let trimmed = raw.trim();
                    // kwallet-query prints "failed to read" on missing entry
                    if trimmed.starts_with("failed to read") || trimmed.is_empty() {
                        b"".to_vec()
                    } else {
                        trimmed.as_bytes().to_vec()
                    }
                }
                // Any error (tool absent, non-zero exit) → empty password
                _ => b"".to_vec(),
            };

            Ok(derive_key_linux(&password))
        }

        // GNOME Keyring — shell to secret-tool
        Keyring::GnomeKeyring => {
            let app_attr = gnome_app_attr(browser);
            let service = format!("{account} Safe Storage");

            // Attempt 1: search by `application` attribute
            let by_app = Command::new("secret-tool")
                .args(["search", "--unlock", "application", app_attr])
                .output();

            let password = if let Ok(out) = by_app {
                // `secret-tool search` exits 0 even when zero entries match,
                // printing nothing — so the `!stdout.is_empty()` guard (not just
                // the exit status) is what distinguishes a hit from no-results.
                // The Attempt-2 `lookup` fallback handles the no-results case.
                if out.status.success() && !out.stdout.is_empty() {
                    // secret-tool prints "secret = <value>" lines;
                    // extract the value after "secret = "
                    let text = String::from_utf8_lossy(&out.stdout);
                    parse_secret_tool_output(&text)
                } else {
                    None
                }
            } else {
                None
            };

            // Attempt 2: lookup by service + account
            let password = password.or_else(|| {
                let out = Command::new("secret-tool")
                    .args(["lookup", "service", &service, "account", account])
                    .output()
                    .ok()?;
                if out.status.success() && !out.stdout.is_empty() {
                    let text = String::from_utf8_lossy(&out.stdout);
                    let trimmed = text.trim().to_string();
                    if trimmed.is_empty() {
                        None
                    } else {
                        Some(trimmed)
                    }
                } else {
                    None
                }
            });

            let pw_bytes = password.map_or_else(|| b"".to_vec(), String::into_bytes);

            Ok(derive_key_linux(&pw_bytes))
        }
    }
}

/// Parse the `secret = <value>` line from `secret-tool search` stdout.
#[cfg(target_os = "linux")]
fn parse_secret_tool_output(text: &str) -> Option<String> {
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(val) = trimmed.strip_prefix("secret = ") {
            let v = val.trim().to_string();
            if !v.is_empty() {
                return Some(v);
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cookie::RawRow;

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    /// Build a minimal `RawRow` with the given `encrypted_value`.
    fn make_row(encrypted_value: Vec<u8>) -> RawRow {
        RawRow {
            host: "example.com".to_owned(),
            name: "test".to_owned(),
            path: "/".to_owned(),
            secure: false,
            expires_unix: None,
            encrypted_value,
            plaintext_value: None,
        }
    }

    /// Parse 32-hex-char string into a 16-byte array for KAT assertions.
    fn hex_to_16(s: &str) -> [u8; 16] {
        assert_eq!(s.len(), 32, "expected 32 hex chars for a 16-byte key");
        let mut out = [0u8; 16];
        for (i, chunk) in s.as_bytes().chunks(2).enumerate() {
            out[i] = u8::from_str_radix(std::str::from_utf8(chunk).unwrap(), 16)
                .expect("invalid hex byte");
        }
        out
    }

    /// Encrypt `plaintext` with AES-128-CBC (16-space IV, PKCS7), prepend `prefix`.
    fn aes_encrypt(key: &[u8; 16], plaintext: &[u8], prefix: &[u8]) -> Vec<u8> {
        use cbc::cipher::{BlockEncryptMut, KeyIvInit};
        type Aes128CbcEnc = cbc::Encryptor<Aes128>;

        let padded_len = ((plaintext.len() / 16) + 1) * 16;
        let mut buf = vec![0u8; padded_len];
        buf[..plaintext.len()].copy_from_slice(plaintext);

        let ct = Aes128CbcEnc::new(key.into(), &IV.into())
            .encrypt_padded_mut::<Pkcs7>(&mut buf, plaintext.len())
            .expect("encrypt_padded_mut");

        let mut blob = prefix.to_vec();
        blob.extend_from_slice(ct);
        blob
    }

    // -----------------------------------------------------------------------
    // detect_keyring — table-driven over injected env
    // -----------------------------------------------------------------------

    /// Build a lookup function from a slice of `(key, value)` pairs.
    fn env_map(pairs: &[(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
        let map: std::collections::HashMap<&str, &str> = pairs.iter().copied().collect();
        move |key: &str| map.get(key).map(std::string::ToString::to_string)
    }

    /// Convenience: env with only `XDG_CURRENT_DESKTOP`.
    fn xdg(de: &'static str) -> impl Fn(&str) -> Option<String> {
        let de: &'static str = de;
        move |key: &str| {
            if key == "XDG_CURRENT_DESKTOP" {
                Some(de.to_string())
            } else {
                None
            }
        }
    }

    #[test]
    fn detect_kde5_via_xdg() {
        let env = env_map(&[("XDG_CURRENT_DESKTOP", "KDE"), ("KDE_SESSION_VERSION", "5")]);
        assert_eq!(detect_keyring(&env), Keyring::KWallet5);
    }

    #[test]
    fn detect_kde6_via_xdg() {
        let env = env_map(&[("XDG_CURRENT_DESKTOP", "KDE"), ("KDE_SESSION_VERSION", "6")]);
        assert_eq!(detect_keyring(&env), Keyring::KWallet6);
    }

    #[test]
    fn detect_kde4_via_xdg_version_4() {
        let env = env_map(&[("XDG_CURRENT_DESKTOP", "KDE"), ("KDE_SESSION_VERSION", "4")]);
        assert_eq!(detect_keyring(&env), Keyring::KWallet);
    }

    #[test]
    fn detect_kde4_via_xdg_no_version() {
        // KDE_SESSION_VERSION absent → KDE4 (KWallet)
        let env = env_map(&[("XDG_CURRENT_DESKTOP", "KDE")]);
        assert_eq!(detect_keyring(&env), Keyring::KWallet);
    }

    #[test]
    fn detect_kde3_via_xdg_returns_basictext() {
        // KDE3 → BasicText (pitfall: no usable kwallet)
        let env = env_map(&[("XDG_CURRENT_DESKTOP", "KDE"), ("KDE_SESSION_VERSION", "3")]);
        assert_eq!(detect_keyring(&env), Keyring::BasicText);
    }

    #[test]
    fn detect_gnome_via_xdg() {
        let env = xdg("GNOME");
        assert_eq!(detect_keyring(&env), Keyring::GnomeKeyring);
    }

    #[test]
    fn detect_cinnamon_via_xdg() {
        // X-Cinnamon maps to GnomeKeyring
        let env = xdg("X-Cinnamon");
        assert_eq!(detect_keyring(&env), Keyring::GnomeKeyring);
    }

    #[test]
    fn detect_unity_via_xdg() {
        let env = xdg("Unity");
        assert_eq!(detect_keyring(&env), Keyring::GnomeKeyring);
    }

    #[test]
    fn detect_pantheon_via_xdg() {
        let env = xdg("Pantheon");
        assert_eq!(detect_keyring(&env), Keyring::GnomeKeyring);
    }

    #[test]
    fn detect_xfce_via_xdg() {
        let env = xdg("XFCE");
        assert_eq!(detect_keyring(&env), Keyring::GnomeKeyring);
    }

    #[test]
    fn detect_ukui_via_xdg() {
        let env = xdg("UKUI");
        assert_eq!(detect_keyring(&env), Keyring::GnomeKeyring);
    }

    #[test]
    fn detect_deepin_via_xdg() {
        let env = xdg("Deepin");
        assert_eq!(detect_keyring(&env), Keyring::GnomeKeyring);
    }

    #[test]
    fn detect_lxqt_via_xdg_returns_basictext() {
        // LXQt → BasicText (pitfall case)
        let env = xdg("LXQt");
        assert_eq!(detect_keyring(&env), Keyring::BasicText);
    }

    #[test]
    fn detect_unknown_de_via_xdg_returns_basictext() {
        // Unrecognised token → pitfall: must NOT default to GnomeKeyring
        let env = xdg("SomeObscureDE");
        assert_eq!(detect_keyring(&env), Keyring::BasicText);
    }

    #[test]
    fn detect_empty_xdg_returns_basictext() {
        // Empty string in XDG_CURRENT_DESKTOP → BasicText
        let env = xdg("");
        assert_eq!(detect_keyring(&env), Keyring::BasicText);
    }

    #[test]
    fn detect_no_env_returns_basictext() {
        // Completely empty environment
        let env = env_map(&[]);
        assert_eq!(detect_keyring(&env), Keyring::BasicText);
    }

    #[test]
    fn detect_gnome_via_gnome_desktop_session_id() {
        // No XDG_CURRENT_DESKTOP, but GNOME_DESKTOP_SESSION_ID is set
        let env = env_map(&[("GNOME_DESKTOP_SESSION_ID", "this-is-deprecated")]);
        assert_eq!(detect_keyring(&env), Keyring::GnomeKeyring);
    }

    #[test]
    fn detect_kde_via_kde_full_session() {
        // No XDG/DESKTOP_SESSION, but KDE_FULL_SESSION is set; version = 5
        let env = env_map(&[("KDE_FULL_SESSION", "true"), ("KDE_SESSION_VERSION", "5")]);
        assert_eq!(detect_keyring(&env), Keyring::KWallet5);
    }

    #[test]
    fn detect_kde_via_kde_full_session_no_version() {
        let env = env_map(&[("KDE_FULL_SESSION", "true")]);
        assert_eq!(detect_keyring(&env), Keyring::KWallet);
    }

    #[test]
    fn detect_colon_list_ubuntu_gnome() {
        // XDG_CURRENT_DESKTOP may be "ubuntu:GNOME" — second token is GNOME
        let env = env_map(&[("XDG_CURRENT_DESKTOP", "ubuntu:GNOME")]);
        assert_eq!(detect_keyring(&env), Keyring::GnomeKeyring);
    }

    #[test]
    fn detect_colon_list_first_token_wins() {
        // First token is a recognised DE — it wins regardless of later tokens
        let env = env_map(&[("XDG_CURRENT_DESKTOP", "X-Cinnamon:GNOME")]);
        assert_eq!(detect_keyring(&env), Keyring::GnomeKeyring);
    }

    #[test]
    fn detect_desktop_session_gnome_fallback() {
        // XDG_CURRENT_DESKTOP absent; DESKTOP_SESSION = gnome
        let env = env_map(&[("DESKTOP_SESSION", "gnome")]);
        // "gnome" lowercased; keyring_for_de_name uses to_uppercase → "GNOME"
        assert_eq!(detect_keyring(&env), Keyring::GnomeKeyring);
    }

    #[test]
    fn detect_desktop_session_kde_fallback() {
        let env = env_map(&[("DESKTOP_SESSION", "kde"), ("KDE_SESSION_VERSION", "6")]);
        assert_eq!(detect_keyring(&env), Keyring::KWallet6);
    }

    // Pitfall coverage: explicitly assert that UNKNOWN → BasicText, not GnomeKeyring
    #[test]
    fn pitfall_unknown_de_is_basictext_not_gnome() {
        let env = xdg("i3");
        let k = detect_keyring(&env);
        assert_ne!(
            k,
            Keyring::GnomeKeyring,
            "unknown DE must NOT map to GnomeKeyring"
        );
        assert_eq!(k, Keyring::BasicText, "unknown DE must map to BasicText");
    }

    #[test]
    fn pitfall_kde3_is_basictext_not_kwallet() {
        let env = env_map(&[("XDG_CURRENT_DESKTOP", "KDE"), ("KDE_SESSION_VERSION", "3")]);
        let k = detect_keyring(&env);
        assert_ne!(k, Keyring::KWallet, "KDE3 must NOT map to KWallet");
        assert_eq!(k, Keyring::BasicText, "KDE3 must map to BasicText");
    }

    #[test]
    fn pitfall_lxqt_is_basictext_not_gnome() {
        let env = xdg("LXQt");
        let k = detect_keyring(&env);
        assert_ne!(
            k,
            Keyring::GnomeKeyring,
            "LXQt must NOT map to GnomeKeyring"
        );
        assert_eq!(k, Keyring::BasicText, "LXQt must map to BasicText");
    }

    // -----------------------------------------------------------------------
    // derive_key_linux — known-answer test (KAT)
    // -----------------------------------------------------------------------

    /// Independent known-answer test for PBKDF2-HMAC-SHA1, 1 iteration.
    ///
    /// Expected value computed independently via:
    /// ```text
    /// python3 -c "import hashlib; print(hashlib.pbkdf2_hmac('sha1', b'peanuts', b'saltysalt', 1, 16).hex())"
    /// ```
    /// Output: `fd621fe5a2b402539dfa147ca9272778`
    ///
    /// This test locks: salt=b"saltysalt", iterations=1 (NOT 1003), SHA1,
    /// password=b"peanuts", 16-byte output.
    /// A wrong iteration count or salt would fail this test.
    #[test]
    fn derive_key_linux_kat_peanuts_1_iteration() {
        // Independently verified via Python:
        //   python3 -c "import hashlib; print(hashlib.pbkdf2_hmac('sha1', b'peanuts', b'saltysalt', 1, 16).hex())"
        // Output: fd621fe5a2b402539dfa147ca9272778
        let expected = hex_to_16("fd621fe5a2b402539dfa147ca9272778");
        assert_eq!(
            derive_key_linux(b"peanuts").0,
            expected,
            "PBKDF2-SHA1(peanuts, saltysalt, 1) must equal the Python-computed KAT"
        );
    }

    #[test]
    fn derive_key_linux_is_deterministic() {
        let k1 = derive_key_linux(b"password");
        let k2 = derive_key_linux(b"password");
        assert_eq!(k1.0, k2.0);
    }

    #[test]
    fn derive_key_linux_differs_from_macos_iterations() {
        // Sanity-check: 1-iteration key ≠ 1003-iteration key (proves we use 1)
        use pbkdf2::pbkdf2_hmac;
        let mut macos_key = [0u8; 16];
        pbkdf2_hmac::<Sha1>(b"peanuts", b"saltysalt", 1003, &mut macos_key);
        let linux_key = derive_key_linux(b"peanuts");
        assert_ne!(
            linux_key.0, macos_key,
            "Linux key (1 iteration) must differ from macOS key (1003 iterations)"
        );
    }

    // -----------------------------------------------------------------------
    // AES-CBC round-trip — v10 prefix, meta_version < 24
    // -----------------------------------------------------------------------

    /// Encrypt with the peanuts-derived key, prepend v10, decrypt, assert recovery.
    #[test]
    fn decrypt_v10_round_trip_peanuts_key_no_hash() {
        let key = derive_key_linux(b"peanuts");
        let plaintext = b"hello from Linux Chromium";
        let blob = aes_encrypt(&key.0, plaintext, b"v10");
        let row = make_row(blob);
        // Pass the peanuts key as the "keyring key" — for v10 it uses the internal key
        let result = decrypt(&row, &key, 23 /* < 24, no hash strip */);
        assert_eq!(
            result.as_deref(),
            Some("hello from Linux Chromium"),
            "v10 round-trip must recover the plaintext"
        );
    }

    /// v10 round-trip with `meta_version` ≥ 24 — 32-byte hash stripped.
    #[test]
    fn decrypt_v10_round_trip_strips_hash_at_version_24() {
        let key = derive_key_linux(b"peanuts");
        let cookie_value = b"session=linux42";
        // Prepend a fake 32-byte hash (as Chromium would)
        let mut plaintext_with_hash = vec![0xCDu8; 32];
        plaintext_with_hash.extend_from_slice(cookie_value);

        let blob = aes_encrypt(&key.0, &plaintext_with_hash, b"v10");
        let row = make_row(blob);
        let result = decrypt(&row, &key, 24);
        assert_eq!(
            result.as_deref(),
            Some("session=linux42"),
            "hash-stripping must remove the 32-byte prefix"
        );
    }

    // -----------------------------------------------------------------------
    // AES-CBC round-trip — v11 prefix, uses keyring key
    // -----------------------------------------------------------------------

    #[test]
    fn decrypt_v11_round_trip_with_keyring_key() {
        // For v11 the keyring-derived key is used
        let keyring_key: [u8; 16] = *b"secretkeyringkey";
        let plaintext = b"v11-protected-cookie";
        let blob = aes_encrypt(&keyring_key, plaintext, b"v11");
        let row = make_row(blob);
        let result = decrypt(&row, &Key(keyring_key), 0);
        assert_eq!(
            result.as_deref(),
            Some("v11-protected-cookie"),
            "v11 round-trip must use the keyring key"
        );
    }

    // -----------------------------------------------------------------------
    // Empty-password retry (Chromium bugfix parity)
    // -----------------------------------------------------------------------

    /// v10 with wrong peanuts decryption (encrypted with empty-password key)
    /// — must recover via the retry path.
    #[test]
    fn decrypt_v10_retries_with_empty_password_key() {
        // Encrypt with the EMPTY-password key (not peanuts)
        let empty_key = derive_key_linux(b"");
        let plaintext = b"empty-password-cookie";
        let blob = aes_encrypt(&empty_key.0, plaintext, b"v10");
        let row = make_row(blob);
        // Pass any key as the nominal key — v10 first tries peanuts, then empty
        let result = decrypt(&row, &empty_key, 0);
        assert_eq!(
            result.as_deref(),
            Some("empty-password-cookie"),
            "must recover via empty-password retry"
        );
    }

    // -----------------------------------------------------------------------
    // LegacyPlaintext
    // -----------------------------------------------------------------------

    #[test]
    fn decrypt_legacy_plaintext_empty_returns_empty_string() {
        let row = make_row(vec![]);
        let key = Key([0u8; 16]);
        let result = decrypt(&row, &key, 0);
        assert_eq!(result.as_deref(), Some(""), "empty blob → empty string");
    }

    // -----------------------------------------------------------------------
    // LegacyDpapi — must return None
    // -----------------------------------------------------------------------

    #[test]
    fn decrypt_legacy_dpapi_returns_none() {
        let row = make_row(vec![0x01, 0x02, 0x03]);
        let key = Key([0u8; 16]);
        assert!(
            decrypt(&row, &key, 0).is_none(),
            "DPAPI blob must return None on Linux"
        );
    }

    // -----------------------------------------------------------------------
    // Malformed ciphertext — graceful None
    // -----------------------------------------------------------------------

    #[test]
    fn decrypt_v10_bad_key_returns_none() {
        // Encrypt with a key that is neither peanuts-derived nor empty-derived.
        // The decrypt dispatch for v10 tries peanuts first, then empty — both fail.
        let other_key: [u8; 16] = *b"ffffffffffffffff";
        let blob = aes_encrypt(&other_key, b"other", b"v10");
        let row = make_row(blob);
        // Key passed here is irrelevant for v10; peanuts and empty both fail.
        let result = decrypt(&row, &Key([0u8; 16]), 0);
        assert!(result.is_none(), "wrong-key v10 must return None");
    }

    #[test]
    fn decrypt_v10_too_short_returns_none() {
        let row = make_row(b"v10".to_vec());
        let key = Key([0u8; 16]);
        assert!(decrypt(&row, &key, 0).is_none());
    }

    #[test]
    fn decrypt_v10_non_block_aligned_returns_none() {
        let mut blob = b"v10".to_vec();
        blob.extend_from_slice(&[0xAAu8; 7]); // 7 bytes — not a multiple of 16
        let row = make_row(blob);
        let key = Key([0u8; 16]);
        assert!(decrypt(&row, &key, 0).is_none());
    }

    // -----------------------------------------------------------------------
    // Linux-gated keyring tests — #[ignore]d
    // -----------------------------------------------------------------------

    /// Live `KWallet` test — requires `kwallet-query` and a running KDE session.
    #[test]
    #[ignore = "requires Linux with a running KDE session and kwallet-query installed"]
    #[cfg(target_os = "linux")]
    fn key_for_chrome_kwallet5_live() {
        use crate::Browser;
        let k = key_for(Browser::Chrome, Keyring::KWallet5);
        match k {
            Ok(key) => println!("KWallet5 Chrome key (4 bytes): {:02x?}", &key.0[..4]),
            Err(e) => panic!("key_for failed: {e}"),
        }
    }

    /// Live GNOME Keyring test — requires `secret-tool` and a running GNOME session.
    #[test]
    #[ignore = "requires Linux with a running GNOME session and secret-tool installed"]
    #[cfg(target_os = "linux")]
    fn key_for_chrome_gnome_keyring_live() {
        use crate::Browser;
        let k = key_for(Browser::Chrome, Keyring::GnomeKeyring);
        match k {
            Ok(key) => println!("GnomeKeyring Chrome key (4 bytes): {:02x?}", &key.0[..4]),
            Err(e) => panic!("key_for failed: {e}"),
        }
    }
}
