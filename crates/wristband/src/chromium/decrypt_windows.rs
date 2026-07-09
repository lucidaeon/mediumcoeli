//! Chromium Windows cookie decryption — `Local State` master key via DPAPI
//! (PowerShell `ProtectedData.Unprotect`) + AES-256-GCM per cookie value.
//!
//! # Cross-platform structure
//!
//! This module is deliberately **not** gated on `#[cfg(target_os = "windows")]`
//! as a whole, so that the pure-crypto portion compiles and runs on macOS (the
//! development host):
//!
//! - **Cross-platform** (compiled + tested on macOS):
//!   - [`decrypt_v10_gcm`] — strips the `v10` prefix, splits nonce / ciphertext /
//!     tag, AES-256-GCM-decrypts, applies [`crate::chromium::framing::strip_hash`],
//!     and UTF-8-decodes. All failures return `None`.
//!
//! - **Windows-only** (`#[cfg(target_os = "windows")]`):
//!   - `master_key` — reads `<browser_root>/Local State` JSON, base64-decodes
//!     `os_crypt.encrypted_key`, strips the 5-byte `DPAPI` prefix, and
//!     decrypts via a PowerShell subprocess calling
//!     `System.Security.Cryptography.ProtectedData.Unprotect` (`CurrentUser` scope).
//!   - `decrypt_legacy_dpapi` — DPAPI-decrypts a raw `encrypted_value` blob
//!     (pre-v10 per-cookie DPAPI path) via the same PowerShell mechanism.
//!   - `decrypt` — dispatches: v10/v11 → [`decrypt_gcm_body`] (after stripping
//!     the 3-byte prefix); legacy DPAPI → `decrypt_legacy_dpapi`.
//!   - `chromium::read_chromium` / `read_chromium_from_paths` (in
//!     `chromium/mod.rs`) is the Windows entry point that drives this module.
//!
//! # Master-key layout (`Local State` JSON)
//!
//! ```text
//! os_crypt.encrypted_key  →  base64-encoded blob
//!                            [0..5]   = b"DPAPI"  (ASCII prefix, not encrypted)
//!                            [5..]    = DPAPI-encrypted 32-byte AES key
//! ```
//! After stripping the prefix and decrypting: 32 raw bytes → `Key256`.
//!
//! # v10 cookie-value layout
//!
//! After the 3-byte `v10` prefix:
//! ```text
//! [0..12]        = GCM nonce (12 bytes)
//! [12..len-16]   = ciphertext
//! [len-16..len]  = GCM authentication tag (16 bytes)
//! ```
//! The minimum meaningful length (excluding the `v10` prefix) is therefore
//! 12 + 0 + 16 = 28 bytes.  Values shorter than this return `None`.
//!
//! # PowerShell DPAPI invocation (Windows-gated)
//!
//! The encrypted blob is passed **as base64 via an environment variable**
//! (`WB_IN`) to avoid argument-escaping and length-limit issues. The PowerShell
//! one-liner round-trips through `ProtectedData.Unprotect` in `CurrentUser` scope
//! and prints the result as base64 on stdout:
//!
//! ```powershell
//! $b=[Convert]::FromBase64String($env:WB_IN);
//! $o=[System.Security.Cryptography.ProtectedData]::Unprotect(
//!     $b, $null,
//!     [System.Security.Cryptography.DataProtectionScope]::CurrentUser);
//! [Convert]::ToBase64String($o)
//! ```

use aes_gcm::{
    Aes256Gcm, Key, Nonce,
    aead::{Aead, KeyInit},
};

use crate::chromium::framing::{Key256, strip_hash};

// ---------------------------------------------------------------------------
// Cross-platform: AES-256-GCM decryption (tested on macOS)
// ---------------------------------------------------------------------------

/// Shared AES-256-GCM decryption body — operates on the raw bytes **after**
/// the 3-byte version prefix has been stripped.
///
/// # Layout (`body_after_prefix`)
///
/// ```text
/// [0..12]        GCM nonce (12 bytes)
/// [12..len-16]   ciphertext
/// [len-16..len]  GCM authentication tag (16 bytes)
/// ```
///
/// Minimum length: 12 + 16 = 28 bytes (empty plaintext). Shorter input
/// returns `None` without panicking.
///
/// After successful decryption, [`strip_hash`] is applied for
/// `meta_version >= 24` (Chromium prepends a 32-byte SHA-256 domain hash).
/// The resulting bytes are decoded as UTF-8; invalid UTF-8 returns `None`.
///
/// This function is called by both [`decrypt_v10_gcm`] (which strips the
/// `v10` prefix before calling) and by the `v11` arm of
/// `decrypt` (which strips the `v11` prefix). Neither arm allocates an
/// intermediate Vec to re-prefix.
// Cross-platform for testability; `decrypt` (Windows-gated) is the only
// non-test caller, so this function is unreachable on non-Windows non-test builds.
#[allow(dead_code)]
pub(crate) fn decrypt_gcm_body(
    key: &Key256,
    body_after_prefix: &[u8],
    meta_version: i64,
) -> Option<String> {
    // Minimum: 12-byte nonce + 16-byte tag = 28 bytes. Ciphertext may be empty.
    if body_after_prefix.len() < 28 {
        return None;
    }

    let nonce_bytes = &body_after_prefix[..12];
    let tag_bytes = &body_after_prefix[body_after_prefix.len() - 16..];
    let ciphertext = &body_after_prefix[12..body_after_prefix.len() - 16];

    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key.0));
    let nonce = Nonce::from_slice(nonce_bytes);

    // Reconstruct the combined ciphertext+tag that aes-gcm expects.
    // aes_gcm::Aead::decrypt accepts ciphertext||tag concatenated.
    let mut combined = ciphertext.to_vec();
    combined.extend_from_slice(tag_bytes);

    let plaintext = cipher.decrypt(nonce, combined.as_slice()).ok()?;
    let stripped = strip_hash(plaintext, meta_version);
    String::from_utf8(stripped).ok()
}

/// Decrypt a Chromium Windows v10-prefixed cookie value using AES-256-GCM.
///
/// Strips the 3-byte `v10` prefix and delegates to [`decrypt_gcm_body`].
///
/// # Returns
///
/// `Some(plaintext)` on success, `None` on any failure (missing prefix, too
/// short, GCM tag mismatch, invalid UTF-8).
// Cross-platform for testability; callers from lib.rs are only on Windows.
#[allow(dead_code)]
pub(crate) fn decrypt_v10_gcm(
    key: &Key256,
    encrypted_value: &[u8],
    meta_version: i64,
) -> Option<String> {
    let body = encrypted_value.strip_prefix(b"v10")?;
    decrypt_gcm_body(key, body, meta_version)
}

// ---------------------------------------------------------------------------
// Windows-only: PowerShell DPAPI helpers + high-level dispatch
// ---------------------------------------------------------------------------

/// Invoke Windows DPAPI (`ProtectedData.Unprotect`, CurrentUser scope) via a
/// PowerShell subprocess, returning the decrypted bytes.
///
/// The encrypted blob is passed via the `WB_IN` environment variable (base64)
/// to avoid PowerShell argument-escaping and `cmd.exe` length limits.
///
/// # Errors
///
/// Returns `None` on any failure (PowerShell not found, non-zero exit, bad
/// base64 output, or empty result).
#[cfg(target_os = "windows")]
fn dpapi_unprotect(encrypted: &[u8]) -> Option<Vec<u8>> {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    use std::process::Command;

    let b64_in = STANDARD.encode(encrypted);

    let ps_script = concat!(
        "$b=[Convert]::FromBase64String($env:WB_IN);",
        "$o=[System.Security.Cryptography.ProtectedData]::Unprotect(",
        "    $b,$null,",
        "    [System.Security.Cryptography.DataProtectionScope]::CurrentUser);",
        "[Convert]::ToBase64String($o)"
    );

    let output = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", ps_script])
        .env("WB_IN", &b64_in)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let b64_out = std::str::from_utf8(&output.stdout).ok()?.trim();
    if b64_out.is_empty() {
        return None;
    }

    STANDARD.decode(b64_out).ok()
}

/// Read the Chromium master AES-256-GCM key from `<browser_root>/Local State`.
///
/// # Local State key path
///
/// `os_crypt.encrypted_key` contains a base64-encoded blob whose first 5 bytes
/// are the ASCII string `DPAPI` (a prefix marker, not part of the ciphertext).
/// The remaining bytes are DPAPI-encrypted and hold the raw 32-byte AES key.
///
/// # Errors
///
/// Returns [`crate::error::WristbandError::Keychain`] if:
/// - `Local State` cannot be read or parsed as JSON,
/// - `os_crypt.encrypted_key` is absent or not a valid base64 string,
/// - the blob is shorter than 5 bytes (missing `DPAPI` prefix),
/// - `ProtectedData.Unprotect` fails,
/// - the decrypted payload is not exactly 32 bytes.
#[cfg(target_os = "windows")]
pub(crate) fn master_key(
    browser_root: &std::path::Path,
) -> Result<Key256, crate::error::WristbandError> {
    use base64::{Engine as _, engine::general_purpose::STANDARD};

    let local_state_path = browser_root.join("Local State");
    let json_text = std::fs::read_to_string(&local_state_path).map_err(|e| {
        crate::error::WristbandError::Keychain(format!(
            "cannot read Local State at {}: {e}",
            crate::path::display_path(&local_state_path)
        ))
    })?;

    let json: serde_json::Value = serde_json::from_str(&json_text).map_err(|e| {
        crate::error::WristbandError::Keychain(format!("Local State JSON parse error: {e}"))
    })?;

    let b64 = json
        .get("os_crypt")
        .and_then(|v| v.get("encrypted_key"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            crate::error::WristbandError::Keychain(
                "os_crypt.encrypted_key not found in Local State".to_owned(),
            )
        })?;

    let blob = STANDARD.decode(b64).map_err(|e| {
        crate::error::WristbandError::Keychain(format!(
            "os_crypt.encrypted_key is not valid base64: {e}"
        ))
    })?;

    // Strip the 5-byte "DPAPI" ASCII prefix.
    let dpapi_blob = blob.strip_prefix(b"DPAPI").ok_or_else(|| {
        crate::error::WristbandError::Keychain(
            "encrypted_key blob does not begin with 'DPAPI' prefix".to_owned(),
        )
    })?;

    // Decrypt via PowerShell ProtectedData.Unprotect.
    let raw_key = dpapi_unprotect(dpapi_blob).ok_or_else(|| {
        crate::error::WristbandError::Keychain(
            "DPAPI decryption of master key failed (PowerShell ProtectedData.Unprotect)".to_owned(),
        )
    })?;

    if raw_key.len() != 32 {
        return Err(crate::error::WristbandError::Keychain(format!(
            "expected 32-byte AES key after DPAPI decryption, got {} bytes",
            raw_key.len()
        )));
    }

    let mut arr = [0u8; 32];
    arr.copy_from_slice(&raw_key);
    Ok(Key256(arr))
}

/// Decrypt a legacy per-cookie DPAPI blob (pre-v10 Chromium Windows path).
///
/// Calls `ProtectedData.Unprotect` (CurrentUser scope) on the raw
/// `encrypted_value` bytes and UTF-8-decodes the result.
///
/// All failures return `None` (graceful degradation).
#[cfg(target_os = "windows")]
fn decrypt_legacy_dpapi(row: &RawRow) -> Option<String> {
    let plaintext_bytes = dpapi_unprotect(&row.encrypted_value)?;
    String::from_utf8(plaintext_bytes).ok()
}

/// Decrypt one Chromium Windows cookie row.
///
/// # Scheme dispatch
///
/// | Scheme | Action |
/// |--------|--------|
/// | `v10`-prefixed | [`decrypt_gcm_body`] (after stripping the 3-byte prefix) |
/// | `v11`-prefixed | [`decrypt_gcm_body`] (same AES-256-GCM, same key — no intermediate alloc) |
/// | Legacy plaintext (empty blob) | `Some("")` |
/// | Legacy DPAPI (non-empty, no prefix) | [`decrypt_legacy_dpapi`] |
///
/// All failures return `None`.
#[cfg(target_os = "windows")]
pub(crate) fn decrypt(row: &RawRow, key: &Key256, meta_version: i64) -> Option<String> {
    use crate::chromium::framing::{Scheme, classify};
    match classify(&row.encrypted_value) {
        Scheme::V10 => {
            let body = row.encrypted_value.strip_prefix(b"v10")?;
            decrypt_gcm_body(key, body, meta_version)
        }
        // v11 on Windows uses the same AES-256-GCM scheme as v10; call the
        // shared body function directly without re-allocating a v10-prefixed Vec.
        Scheme::V11 => {
            let body = row.encrypted_value.strip_prefix(b"v11")?;
            decrypt_gcm_body(key, body, meta_version)
        }
        Scheme::LegacyPlaintext => String::from_utf8(row.encrypted_value.clone()).ok(),
        Scheme::LegacyDpapi => decrypt_legacy_dpapi(row),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cookie::RawRow;
    use aes_gcm::{
        Aes256Gcm, Key, Nonce,
        aead::{Aead, KeyInit},
    };

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    /// Build a minimal `RawRow` with the given `encrypted_value`.
    // Only used from Windows-gated tests; on macOS the test module compiles but
    // these helpers go unused.
    #[allow(dead_code)]
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

    /// AES-256-GCM encrypt `plaintext` with `key` and `nonce`, returning the
    /// combined ciphertext+tag blob (without the `v10` prefix).
    fn gcm_encrypt(key_bytes: &[u8; 32], nonce_bytes: &[u8; 12], plaintext: &[u8]) -> Vec<u8> {
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key_bytes));
        let nonce = Nonce::from_slice(nonce_bytes);
        // aes-gcm returns ciphertext || tag (tag is appended)
        cipher.encrypt(nonce, plaintext).expect("gcm_encrypt")
    }

    /// Build a Chromium-style v10 blob: `b"v10"` + nonce + ciphertext + tag.
    fn make_v10_blob(key_bytes: &[u8; 32], nonce_bytes: &[u8; 12], plaintext: &[u8]) -> Vec<u8> {
        let ct_plus_tag = gcm_encrypt(key_bytes, nonce_bytes, plaintext);
        // Chromium layout: nonce (12) || ciphertext || tag (16)
        // aes-gcm returns: ciphertext || tag — split them out
        let tag_start = ct_plus_tag.len() - 16;
        let ciphertext = &ct_plus_tag[..tag_start];
        let tag = &ct_plus_tag[tag_start..];

        let mut blob = b"v10".to_vec();
        blob.extend_from_slice(nonce_bytes); // nonce: 12 bytes
        blob.extend_from_slice(ciphertext); // ciphertext
        blob.extend_from_slice(tag); // tag: 16 bytes
        blob
    }

    // -----------------------------------------------------------------------
    // decrypt_v10_gcm — round-trip, no hash (meta_version < 24)
    // -----------------------------------------------------------------------

    /// Basic round-trip: encrypt a known plaintext, wrap in the v10 layout,
    /// call `decrypt_v10_gcm`, assert the original is recovered.
    ///
    /// This test runs cross-platform (including macOS) because `decrypt_v10_gcm`
    /// is not gated on Windows.
    #[test]
    fn decrypt_v10_gcm_round_trip_no_hash() {
        let key = Key256([
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d,
            0x0e, 0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b,
            0x1c, 0x1d, 0x1e, 0x1f,
        ]);
        let nonce: [u8; 12] = [0xAA; 12];
        let plaintext = b"hello from Windows Chromium";

        let blob = make_v10_blob(&key.0, &nonce, plaintext);
        let result = decrypt_v10_gcm(&key, &blob, 23 /* < 24, no hash strip */);
        assert_eq!(
            result.as_deref(),
            Some("hello from Windows Chromium"),
            "AES-256-GCM round-trip must recover the original plaintext"
        );
    }

    // -----------------------------------------------------------------------
    // decrypt_v10_gcm — hash strip (meta_version >= 24)
    // -----------------------------------------------------------------------

    /// Round-trip with the 32-byte domain hash prepended to the plaintext
    /// (Chromium meta-version >= 24). After decryption, `strip_hash` must
    /// remove the leading 32 bytes.
    #[test]
    fn decrypt_v10_gcm_round_trip_strips_hash_at_version_24() {
        let key = Key256([0xBBu8; 32]);
        let nonce: [u8; 12] = [0x11; 12];
        let cookie_value = b"session=win_token_42";

        // Prepend a fake 32-byte domain hash (as Chromium >= v24 does).
        let mut plaintext_with_hash = vec![0xCCu8; 32];
        plaintext_with_hash.extend_from_slice(cookie_value);

        let blob = make_v10_blob(&key.0, &nonce, &plaintext_with_hash);
        let result = decrypt_v10_gcm(&key, &blob, 24 /* >= 24, strip hash */);
        assert_eq!(
            result.as_deref(),
            Some("session=win_token_42"),
            "decrypt must strip the 32-byte hash prefix for meta_version >= 24"
        );
    }

    // -----------------------------------------------------------------------
    // Panic-safety: too-short blobs → None
    // -----------------------------------------------------------------------

    /// A blob shorter than `b"v10"` (3) + 12 (nonce) + 16 (tag) = 31 bytes
    /// must return `None` without panicking.
    #[test]
    fn decrypt_v10_gcm_too_short_returns_none() {
        let key = Key256([0u8; 32]);
        // Just the prefix, no body.
        let blob = b"v10".to_vec();
        assert_eq!(
            decrypt_v10_gcm(&key, &blob, 0),
            None,
            "v10 with empty body must return None"
        );
    }

    #[test]
    fn decrypt_v10_gcm_body_shorter_than_28_bytes_returns_none() {
        let key = Key256([0u8; 32]);
        // 27 bytes after "v10" — one short of the minimum.
        let mut blob = b"v10".to_vec();
        blob.extend_from_slice(&[0u8; 27]);
        assert_eq!(
            decrypt_v10_gcm(&key, &blob, 0),
            None,
            "27-byte body (< 28) must return None"
        );
    }

    #[test]
    fn decrypt_v10_gcm_exactly_28_byte_body_with_correct_key_returns_some() {
        // 28 bytes after "v10" = 12 nonce + 0 ciphertext + 16 tag.
        // Encrypt empty plaintext so the tag is correct.
        let key = Key256([0x42u8; 32]);
        let nonce: [u8; 12] = [0x07; 12];
        let blob = make_v10_blob(&key.0, &nonce, b""); // empty plaintext
        let result = decrypt_v10_gcm(&key, &blob, 0);
        // Empty plaintext decrypts to "" which is valid UTF-8.
        assert_eq!(
            result.as_deref(),
            Some(""),
            "empty plaintext must round-trip"
        );
    }

    // -----------------------------------------------------------------------
    // Panic-safety: flipped tag byte → GCM authentication failure → None
    // -----------------------------------------------------------------------

    #[test]
    fn decrypt_v10_gcm_flipped_tag_returns_none() {
        let key = Key256([0x55u8; 32]);
        let nonce: [u8; 12] = [0x22; 12];
        let mut blob = make_v10_blob(&key.0, &nonce, b"cookie value");

        // Flip the last byte (part of the GCM auth tag).
        let last = blob.last_mut().unwrap();
        *last ^= 0xFF;

        assert_eq!(
            decrypt_v10_gcm(&key, &blob, 0),
            None,
            "flipped tag byte must cause GCM authentication failure → None"
        );
    }

    #[test]
    fn decrypt_v10_gcm_wrong_key_returns_none() {
        let encrypt_key = Key256([0xAAu8; 32]);
        let wrong_key = Key256([0xBBu8; 32]);
        let nonce: [u8; 12] = [0x33; 12];
        let blob = make_v10_blob(&encrypt_key.0, &nonce, b"secret cookie");
        assert_eq!(
            decrypt_v10_gcm(&wrong_key, &blob, 0),
            None,
            "wrong key must cause GCM authentication failure → None"
        );
    }

    // -----------------------------------------------------------------------
    // No "v10" prefix → None (guard against caller confusion)
    // -----------------------------------------------------------------------

    #[test]
    fn decrypt_v10_gcm_missing_prefix_returns_none() {
        let key = Key256([0u8; 32]);
        // Valid-length blob but no "v10" prefix.
        let blob = vec![0u8; 31];
        assert_eq!(
            decrypt_v10_gcm(&key, &blob, 0),
            None,
            "blob without v10 prefix must return None"
        );
    }

    // -----------------------------------------------------------------------
    // Windows-gated: master_key / DPAPI round-trip test (requires Windows)
    // -----------------------------------------------------------------------

    /// Live DPAPI round-trip via PowerShell.
    ///
    /// Marked `#[ignore]` because it requires a Windows host and PowerShell.
    /// The test synthesises a fake Local State file with a DPAPI-encrypted key
    /// and verifies that `master_key` decrypts it correctly.
    ///
    /// On Windows CI, remove the `#[ignore]` and ensure `WB_IN` is not already
    /// set in the environment.
    #[test]
    #[ignore = "requires Windows host with PowerShell and DPAPI access"]
    #[cfg(target_os = "windows")]
    fn master_key_round_trips_via_dpapi() {
        use base64::{Engine as _, engine::general_purpose::STANDARD};
        use std::process::Command;
        use tempfile::TempDir;

        // Encrypt a known 32-byte payload via ProtectedData.Protect.
        let known_key_bytes = [0x42u8; 32];
        let b64_plain = STANDARD.encode(known_key_bytes);

        let protect_script = concat!(
            "$b=[Convert]::FromBase64String($env:WB_IN);",
            "$o=[System.Security.Cryptography.ProtectedData]::Protect(",
            "    $b,$null,",
            "    [System.Security.Cryptography.DataProtectionScope]::CurrentUser);",
            "[Convert]::ToBase64String($o)"
        );

        let out = Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", protect_script])
            .env("WB_IN", &b64_plain)
            .output()
            .expect("powershell protect");

        assert!(out.status.success(), "ProtectedData.Protect failed");
        let dpapi_b64 = std::str::from_utf8(&out.stdout).unwrap().trim().to_owned();

        // Build a fake "DPAPI" + <dpapi_blob> and base64-encode it for Local State.
        let dpapi_blob = STANDARD.decode(&dpapi_b64).unwrap();
        let mut prefixed = b"DPAPI".to_vec();
        prefixed.extend_from_slice(&dpapi_blob);
        let encrypted_key_b64 = STANDARD.encode(&prefixed);

        // Write a minimal Local State JSON.
        let dir = TempDir::new().unwrap();
        let local_state = serde_json::json!({
            "os_crypt": {
                "encrypted_key": encrypted_key_b64
            }
        });
        std::fs::write(
            dir.path().join("Local State"),
            serde_json::to_string(&local_state).unwrap(),
        )
        .unwrap();

        let key = master_key(dir.path()).expect("master_key");
        assert_eq!(
            key.0, known_key_bytes,
            "master_key must round-trip through DPAPI"
        );
    }
}
