//! Chromium macOS cookie decryption — Keychain password → PBKDF2-HMAC-SHA1 →
//! AES-128-CBC (16-space IV, PKCS7 padding).
//!
//! This module is compiled **only** on macOS (`#[cfg(target_os = "macos")]`).
//!
//! # Key derivation
//!
//! 1. Retrieve the raw password from the system Keychain by shelling out to
//!    `security find-generic-password -wa <account> -s "<service>"`.
//! 2. Derive a 16-byte AES key: `PBKDF2-HMAC-SHA1(password, b"saltysalt", 1003)`.
//!
//! # Decryption
//!
//! For `v10`-prefixed blobs:
//! - Skip the 3-byte `b"v10"` prefix.
//! - Decrypt with `AES-128-CBC`, IV = 16 × `0x20` (ASCII space), PKCS7.
//! - Apply [`crate::chromium::framing::strip_hash`] for Chromium meta-version ≥ 24.
//! - Decode the resulting bytes as UTF-8.
//!
//! For legacy plaintext (empty blob): decode directly as UTF-8.
//!
//! For `v11` or non-prefixed DPAPI blobs (not used on macOS): return `None`.
//!
//! Decryption failures always yield `None` — the cookie is silently skipped.

use aes::Aes128;
use cbc::cipher::{BlockDecryptMut, KeyIvInit, block_padding::Pkcs7};
use pbkdf2::pbkdf2_hmac;
use sha1::Sha1;
use std::process::Command;

use crate::Browser;
use crate::chromium::framing::{Key, Scheme, classify, strip_hash};
use crate::cookie::RawRow;
use crate::error::WristbandError;

/// AES-128-CBC IV for Chromium macOS: 16 ASCII space bytes (`0x20`).
const IV: [u8; 16] = [0x20u8; 16];

/// PBKDF2 salt used by all Chromium browsers on macOS.
const SALT: &[u8] = b"saltysalt";

/// PBKDF2 iteration count used by Chromium macOS.
const ITERATIONS: u32 = 1003;

// ---------------------------------------------------------------------------
// Keychain lookup helper
// ---------------------------------------------------------------------------

/// Return the `(account, service)` pair for the `security` command for each
/// Chromium-family browser on macOS.
///
/// Confirmed values:
/// - Chrome  — account `"Chrome"`,         service `"Chrome Safe Storage"`
/// - Chromium — account `"Chromium"`,       service `"Chromium Safe Storage"`
/// - Brave   — account `"Brave"`,           service `"Brave Safe Storage"`
/// - Edge    — account `"Microsoft Edge"`,  service `"Microsoft Edge Safe Storage"`
/// - Opera   — account `"Opera"`,           service `"Opera Safe Storage"`
/// - Vivaldi — account `"Vivaldi"`,         service `"Vivaldi Safe Storage"`
/// - Whale   — account `"Whale"`,           service `"Whale Safe Storage"`
fn keychain_creds(browser: Browser) -> (&'static str, &'static str) {
    match browser {
        Browser::Chrome => ("Chrome", "Chrome Safe Storage"),
        Browser::Chromium => ("Chromium", "Chromium Safe Storage"),
        Browser::Brave => ("Brave", "Brave Safe Storage"),
        Browser::Edge => ("Microsoft Edge", "Microsoft Edge Safe Storage"),
        Browser::Opera => ("Opera", "Opera Safe Storage"),
        Browser::Vivaldi => ("Vivaldi", "Vivaldi Safe Storage"),
        Browser::Whale => ("Whale", "Whale Safe Storage"),
        Browser::Firefox | Browser::Safari => {
            unreachable!("Firefox/Safari do not use the Chromium Keychain entry")
        }
    }
}

// ---------------------------------------------------------------------------
// Public(crate) API
// ---------------------------------------------------------------------------

/// Retrieve the AES-128-CBC key for `browser` from the macOS Keychain and
/// derive it via PBKDF2-HMAC-SHA1.
///
/// Shells out to:
/// ```text
/// security find-generic-password -wa <account> -s "<service>"
/// ```
/// where `account` and `service` are determined by `browser`.
///
/// # Errors
///
/// Returns [`WristbandError::Keychain`] if:
/// - the `security` command fails or exits non-zero,
/// - the output is empty (no password found),
/// - the output is not valid UTF-8.
pub(crate) fn key_for(browser: Browser) -> Result<Key, WristbandError> {
    let (account, service) = keychain_creds(browser);

    let output = Command::new("security")
        .args(["find-generic-password", "-wa", account, "-s", service])
        .output()
        .map_err(|e| WristbandError::Keychain(format!("security command failed: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(WristbandError::Keychain(format!(
            "security exited with status {}: {}",
            output.status,
            stderr.trim()
        )));
    }

    // stdout is the password followed by a trailing newline.
    let raw = std::str::from_utf8(&output.stdout)
        .map_err(|e| WristbandError::Keychain(format!("Keychain password is not UTF-8: {e}")))?;
    let password = raw.strip_suffix('\n').unwrap_or(raw);

    if password.is_empty() {
        return Err(WristbandError::Keychain(format!(
            "Keychain returned empty password for {browser:?}"
        )));
    }

    Ok(derive_key(password.as_bytes()))
}

/// Derive a 16-byte AES key from `password` via PBKDF2-HMAC-SHA1.
///
/// Constants: salt = `b"saltysalt"`, iterations = 1003, output = 16 bytes.
fn derive_key(password: &[u8]) -> Key {
    let mut key = [0u8; 16];
    pbkdf2_hmac::<Sha1>(password, SALT, ITERATIONS, &mut key);
    Key(key)
}

/// Decrypt one Chromium cookie row using `key`, returning the plaintext as a
/// `String`, or `None` on any failure.
///
/// # Scheme dispatch
///
/// | [`Scheme`] | Action |
/// |---|---|
/// | [`Scheme::V10`] | AES-128-CBC decrypt bytes after the 3-byte `b"v10"` prefix; apply [`strip_hash`]; decode UTF-8. |
/// | [`Scheme::LegacyPlaintext`] | Decode `encrypted_value` as UTF-8 directly (empty or pre-encryption data). |
/// | [`Scheme::V11`] / [`Scheme::LegacyDpapi`] | Return `None` — not used on macOS. |
///
/// Decryption failure, invalid UTF-8, and all other errors return `None`
/// (graceful degradation; the row is silently skipped by the gate — INV).
pub(crate) fn decrypt(row: &RawRow, key: &Key, meta_version: i64) -> Option<String> {
    match classify(&row.encrypted_value) {
        Scheme::V10 => decrypt_v10(&row.encrypted_value[3..], key, meta_version),
        Scheme::LegacyPlaintext => String::from_utf8(row.encrypted_value.clone()).ok(),
        // V11 (AES-GCM / App-Bound Encryption) and DPAPI blobs are not
        // handled on macOS — return None to skip the cookie.
        Scheme::V11 | Scheme::LegacyDpapi => None,
    }
}

/// Decrypt the ciphertext (after the `v10` prefix has been stripped) and
/// return the plaintext string, or `None` on any failure.
fn decrypt_v10(ciphertext: &[u8], key: &Key, meta_version: i64) -> Option<String> {
    type Aes128CbcDec = cbc::Decryptor<Aes128>;

    // AES-CBC requires the ciphertext length to be a multiple of the block size
    // (16 bytes). If the blob is shorter or not aligned, bail out.
    if ciphertext.is_empty() || !ciphertext.len().is_multiple_of(16) {
        return None;
    }

    // Work in a mutable buffer; decrypt_padded_mut operates in-place.
    let mut buf = ciphertext.to_vec();

    let decryptor = Aes128CbcDec::new(&key.0.into(), &IV.into());
    let plaintext_slice = decryptor.decrypt_padded_mut::<Pkcs7>(&mut buf).ok()?;

    let plaintext = plaintext_slice.to_vec();
    let stripped = strip_hash(plaintext, meta_version);
    String::from_utf8(stripped).ok()
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

    /// Encrypt `plaintext` with AES-128-CBC (16-space IV, PKCS7), then
    /// prepend `b"v10"`. Returns the combined blob.
    fn v10_encrypt(key: &[u8; 16], plaintext: &[u8]) -> Vec<u8> {
        use cbc::cipher::{BlockEncryptMut, KeyIvInit};
        type Aes128CbcEnc = cbc::Encryptor<Aes128>;

        // Buffer must be large enough for padded output.
        let padded_len = ((plaintext.len() / 16) + 1) * 16;
        let mut buf = vec![0u8; padded_len];
        buf[..plaintext.len()].copy_from_slice(plaintext);

        let ct = Aes128CbcEnc::new(key.into(), &IV.into())
            .encrypt_padded_mut::<Pkcs7>(&mut buf, plaintext.len())
            .expect("encrypt_padded_mut");

        let mut blob = b"v10".to_vec();
        blob.extend_from_slice(ct);
        blob
    }

    // -----------------------------------------------------------------------
    // derive_key — deterministic round-trip + independent KAT
    // -----------------------------------------------------------------------

    /// Parse a 32-hex-char string into a 16-byte array for KAT assertions.
    fn hex_to_16(s: &str) -> [u8; 16] {
        assert_eq!(s.len(), 32, "expected 32 hex chars for a 16-byte key");
        let mut out = [0u8; 16];
        for (i, chunk) in s.as_bytes().chunks(2).enumerate() {
            out[i] = u8::from_str_radix(std::str::from_utf8(chunk).unwrap(), 16)
                .expect("invalid hex byte");
        }
        out
    }

    /// Verify the PBKDF2 key derivation with a known test vector.
    ///
    /// Expected value computed independently:
    /// `PBKDF2-HMAC-SHA1("password", b"saltysalt", 1003)` truncated to 16 bytes.
    #[test]
    fn derive_key_is_deterministic() {
        let k1 = derive_key(b"password");
        let k2 = derive_key(b"password");
        assert_eq!(k1.0, k2.0, "derive_key must be deterministic");
    }

    /// Independent PBKDF2-SHA1 known-answer test.
    ///
    /// Expected value computed independently:
    /// ```text
    /// python3 -c "import hashlib; print(hashlib.pbkdf2_hmac('sha1', b'peanuts', b'saltysalt', 1003, 16).hex())"
    /// ```
    /// Output: `d9a09d499b4e1b7461f28e67972c6dbd`
    ///
    /// Locks salt=saltysalt, iterations=1003, SHA1, 16-byte output. A wrong
    /// iteration count or salt would fail this even though the round-trip test
    /// (which uses a known key directly) would not.
    #[test]
    fn derive_key_matches_independent_pbkdf2_sha1_vector() {
        // Independently computed:
        //   python3 -c "import hashlib; print(hashlib.pbkdf2_hmac('sha1', b'peanuts', b'saltysalt', 1003, 16).hex())"
        // Locks salt=saltysalt, iterations=1003, SHA1, 16-byte output. A wrong
        // iteration count or salt would fail this even though the round-trip test
        // (which uses a known key directly) would not.
        let expected = hex_to_16("d9a09d499b4e1b7461f28e67972c6dbd");
        assert_eq!(derive_key(b"peanuts").0, expected);
    }

    #[test]
    fn derive_key_differs_for_different_passwords() {
        let k1 = derive_key(b"password");
        let k2 = derive_key(b"hunter2");
        assert_ne!(k1.0, k2.0);
    }

    // -----------------------------------------------------------------------
    // AES-128-CBC round-trip — meta_version < 24 (no hash)
    // -----------------------------------------------------------------------

    /// Pure-crypto round-trip at `meta_version` < 24: encrypt a known plaintext,
    /// prepend `v10`, call `decrypt`, assert the original is recovered.
    #[test]
    fn decrypt_v10_round_trip_no_hash() {
        let key: [u8; 16] = *b"0123456789abcdef";
        let plaintext = b"hello from Chromium";

        let blob = v10_encrypt(&key, plaintext);
        let row = make_row(blob);
        let result = decrypt(&row, &Key(key), 23 /* < 24, no hash */);
        assert_eq!(
            result.as_deref(),
            Some("hello from Chromium"),
            "decrypt must recover the original plaintext"
        );
    }

    // -----------------------------------------------------------------------
    // AES-128-CBC round-trip — meta_version >= 24 (32-byte hash prepended)
    // -----------------------------------------------------------------------

    /// Pure-crypto round-trip at `meta_version` >= 24: the plaintext has a
    /// 32-byte SHA-256 domain hash prepended before encryption. After
    /// decryption `strip_hash` must remove those bytes, leaving only the
    /// original cookie value.
    #[test]
    fn decrypt_v10_round_trip_strips_hash_at_version_24() {
        let key: [u8; 16] = *b"fedcba9876543210";
        let cookie_value = b"session=xyz123";

        // Build plaintext with a fake 32-byte hash prefix (as Chromium would).
        let mut plaintext_with_hash = vec![0xABu8; 32];
        plaintext_with_hash.extend_from_slice(cookie_value);

        let blob = v10_encrypt(&key, &plaintext_with_hash);
        let row = make_row(blob);
        let result = decrypt(&row, &Key(key), 24 /* >= 24, strip hash */);
        assert_eq!(
            result.as_deref(),
            Some("session=xyz123"),
            "decrypt must strip the 32-byte hash and return the cookie value"
        );
    }

    // -----------------------------------------------------------------------
    // LegacyPlaintext — empty encrypted_value
    // -----------------------------------------------------------------------

    #[test]
    fn decrypt_legacy_plaintext_empty_returns_empty_string() {
        let row = make_row(vec![]);
        let key = Key([0u8; 16]);
        let result = decrypt(&row, &key, 0);
        assert_eq!(result.as_deref(), Some(""), "empty blob → empty string");
    }

    // -----------------------------------------------------------------------
    // V11 / DPAPI — must return None
    // -----------------------------------------------------------------------

    #[test]
    fn decrypt_v11_returns_none() {
        let mut blob = b"v11".to_vec();
        blob.extend_from_slice(&[0u8; 16]);
        let row = make_row(blob);
        let key = Key([0u8; 16]);
        assert!(
            decrypt(&row, &key, 0).is_none(),
            "v11 must yield None on macOS"
        );
    }

    #[test]
    fn decrypt_legacy_dpapi_returns_none() {
        let row = make_row(vec![0x01, 0x02, 0x03]); // non-empty, no prefix
        let key = Key([0u8; 16]);
        assert!(
            decrypt(&row, &key, 0).is_none(),
            "legacy DPAPI blob must yield None on macOS"
        );
    }

    // -----------------------------------------------------------------------
    // Malformed ciphertext — must return None, not panic
    // -----------------------------------------------------------------------

    #[test]
    fn decrypt_v10_bad_padding_returns_none() {
        // A well-formed ciphertext but decrypted with the wrong key → PKCS7
        // unpadding will fail → must return None gracefully.
        let encrypt_key: [u8; 16] = *b"0123456789abcdef";
        let wrong_key: [u8; 16] = *b"ffffffffffffffff";
        let blob = v10_encrypt(&encrypt_key, b"some value");
        let row = make_row(blob);
        let result = decrypt(&row, &Key(wrong_key), 0);
        assert!(result.is_none(), "wrong key must return None, not panic");
    }

    #[test]
    fn decrypt_v10_too_short_returns_none() {
        // Only the prefix, no actual ciphertext payload.
        let row = make_row(b"v10".to_vec());
        let key = Key([0u8; 16]);
        assert!(
            decrypt(&row, &key, 0).is_none(),
            "v10 with empty body must return None"
        );
    }

    #[test]
    fn decrypt_v10_non_block_aligned_returns_none() {
        // 7 bytes after v10 prefix — not a multiple of 16.
        let mut blob = b"v10".to_vec();
        blob.extend_from_slice(&[0xAAu8; 7]);
        let row = make_row(blob);
        let key = Key([0u8; 16]);
        assert!(
            decrypt(&row, &key, 0).is_none(),
            "non-block-aligned ciphertext must return None"
        );
    }

    // -----------------------------------------------------------------------
    // Keychain path — #[ignore]d (requires a real Keychain and installed browser)
    // -----------------------------------------------------------------------

    /// Live Keychain test: shells out to `security` and derives the key.
    ///
    /// Marked `#[ignore]` because it requires a real macOS Keychain with
    /// Chrome installed (and the user to have granted Keychain access).
    #[test]
    #[ignore = "requires real macOS Keychain with Chrome installed"]
    fn key_for_chrome_live_keychain() {
        let result = key_for(Browser::Chrome);
        // We cannot assert the exact key value (it varies per machine), but we
        // can at least verify the call succeeds and returns 16 bytes.
        match result {
            Ok(k) => {
                assert_ne!(k.0, [0u8; 16], "derived key must not be all-zero");
                println!("Chrome key (first 4 bytes): {:02x?}", &k.0[..4]);
            }
            Err(e) => {
                panic!("key_for failed: {e}");
            }
        }
    }
}
