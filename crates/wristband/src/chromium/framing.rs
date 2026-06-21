//! Chromium cookie value framing — version-prefix classification and
//! meta-version hash stripping.
//!
//! Chromium encrypts cookie values and prepends a 3-byte version prefix:
//!
//! - `b"v10"` — AES-CBC (Linux/macOS PBKDF2-derived key, 16-byte IV follows).
//! - `b"v11"` — AES-GCM (Windows DPAPI-protected key; same prefix on macOS
//!   App-Bound Encryption builds).
//!
//! Values that lack a recognized prefix are legacy:
//!
//! - **Empty blob** → the cookie was stored with a plaintext (empty) value.
//! - **Non-empty, no prefix** → a Windows legacy DPAPI blob (pass directly to
//!   `CryptUnprotectData`). Distinguishing which is which for non-prefixed
//!   values is platform-dependent; [`classify`] only inspects the prefix. The
//!   platform decrypt layer decides what to do with a [`Scheme::LegacyDpapi`]
//!   blob.
//!
//! Starting at Chromium **meta-version 24**, the decrypted plaintext has a
//! 32-byte SHA-256 domain hash prepended. [`strip_hash`] removes it when
//! appropriate.

// ---------------------------------------------------------------------------
// Version-prefix scheme
// ---------------------------------------------------------------------------

/// The encryption scheme implied by a raw `encrypted_value` blob.
///
/// Determined by [`classify`] from the first three bytes of the blob.
// Used by platform decrypt tasks (8–10); allow dead_code until they land.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Scheme {
    /// `b"v10"` prefix — AES-CBC, 16-byte key (Linux/macOS PBKDF2).
    V10,
    /// `b"v11"` prefix — AES-GCM, 32-byte key (Windows DPAPI-wrapped or
    /// macOS App-Bound Encryption).
    V11,
    /// Empty blob — the cookie value is empty; no decryption needed.
    LegacyPlaintext,
    /// Non-empty blob without a recognized prefix — treat as a Windows DPAPI
    /// blob for `CryptUnprotectData`. The platform decrypt layer is
    /// responsible for determining the actual format.
    LegacyDpapi,
}

/// Classify a raw `encrypted_value` blob by its version prefix.
///
/// - First 3 bytes `b"v10"` → [`Scheme::V10`].
/// - First 3 bytes `b"v11"` → [`Scheme::V11`].
/// - Empty slice → [`Scheme::LegacyPlaintext`].
/// - Any other non-empty slice → [`Scheme::LegacyDpapi`].
///
/// Note: distinguishing [`Scheme::LegacyPlaintext`] from [`Scheme::LegacyDpapi`]
/// for non-prefixed values is platform-dependent. This function classifies by
/// prefix alone; the platform decrypt layer resolves ambiguity.
// Used by platform decrypt tasks (8–10); allow dead_code until they land.
#[allow(dead_code)]
#[must_use]
pub(crate) fn classify(value: &[u8]) -> Scheme {
    if value.starts_with(b"v10") {
        Scheme::V10
    } else if value.starts_with(b"v11") {
        Scheme::V11
    } else if value.is_empty() {
        Scheme::LegacyPlaintext
    } else {
        Scheme::LegacyDpapi
    }
}

// ---------------------------------------------------------------------------
// Meta-version hash strip
// ---------------------------------------------------------------------------

/// Strip the 32-byte SHA-256 domain hash that Chromium prepends to decrypted
/// cookie plaintext when the Chromium **meta-version** (from the `meta` table)
/// is ≥ 24.
///
/// When `meta_version < 24`, the plaintext is returned as-is.
///
/// # Edge case
///
/// If `meta_version >= 24` but `plaintext` is shorter than 32 bytes (a
/// malformed or truncated decryption result), this function returns an empty
/// `Vec` rather than panicking.
// Used by platform decrypt tasks (8–10); allow dead_code until they land.
#[allow(dead_code)]
pub(crate) fn strip_hash(plaintext: Vec<u8>, meta_version: i64) -> Vec<u8> {
    if meta_version >= 24 {
        if plaintext.len() < 32 {
            // Guard: truncated or malformed — return empty rather than panic.
            return Vec::new();
        }
        plaintext[32..].to_vec()
    } else {
        plaintext
    }
}

// ---------------------------------------------------------------------------
// Key newtypes
// ---------------------------------------------------------------------------

/// A 16-byte AES-CBC key derived on Linux/macOS via PBKDF2.
///
/// Used with [`Scheme::V10`] blobs.
// Consumed by Tasks 8–10 platform decrypt; allow dead_code until those land.
#[allow(dead_code)]
pub(crate) struct Key(pub [u8; 16]);

/// A 32-byte AES-GCM key (Windows DPAPI-wrapped or macOS App-Bound
/// Encryption).
///
/// Used with [`Scheme::V11`] blobs.
// Consumed by Tasks 8–10 platform decrypt; allow dead_code until those land.
#[allow(dead_code)]
pub(crate) struct Key256(pub [u8; 32]);

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // classify
    // -----------------------------------------------------------------------

    #[test]
    fn classify_v10_prefix() {
        // Exactly the prefix
        assert_eq!(classify(b"v10"), Scheme::V10);
        // Prefix followed by payload
        assert_eq!(classify(b"v10\x00\x01\x02"), Scheme::V10);
    }

    #[test]
    fn classify_v11_prefix() {
        assert_eq!(classify(b"v11"), Scheme::V11);
        assert_eq!(classify(b"v11\xFF\xFE\xFD"), Scheme::V11);
    }

    #[test]
    fn classify_empty_is_legacy_plaintext() {
        assert_eq!(classify(b""), Scheme::LegacyPlaintext);
    }

    #[test]
    fn classify_non_prefixed_blob_is_legacy_dpapi() {
        // Starts with something other than v10/v11
        assert_eq!(classify(b"\x01\x02\x03"), Scheme::LegacyDpapi);
        assert_eq!(classify(b"dpapi-stuff"), Scheme::LegacyDpapi);
        // Single byte
        assert_eq!(classify(b"\x00"), Scheme::LegacyDpapi);
    }

    #[test]
    fn classify_v10_not_confused_with_v11() {
        assert_ne!(classify(b"v10abc"), classify(b"v11abc"));
    }

    // -----------------------------------------------------------------------
    // strip_hash
    // -----------------------------------------------------------------------

    #[test]
    fn strip_hash_removes_32_bytes_when_version_ge_24() {
        let hash = vec![0xABu8; 32];
        let payload = b"actual-cookie-value";
        let mut input = hash.clone();
        input.extend_from_slice(payload);

        let result = strip_hash(input, 24);
        assert_eq!(result, payload.as_slice());
    }

    #[test]
    fn strip_hash_removes_32_bytes_at_version_gt_24() {
        let mut input = vec![0u8; 32];
        input.extend_from_slice(b"value");
        let result = strip_hash(input, 25);
        assert_eq!(result, b"value");
    }

    #[test]
    fn strip_hash_passthrough_when_version_lt_24() {
        let input = vec![0u8; 32];
        let result = strip_hash(input.clone(), 23);
        assert_eq!(result, input);
    }

    #[test]
    fn strip_hash_passthrough_at_version_zero() {
        let input = b"hello".to_vec();
        let result = strip_hash(input.clone(), 0);
        assert_eq!(result, input);
    }

    #[test]
    fn strip_hash_short_input_guard_returns_empty() {
        // version >= 24 but only 10 bytes — must not panic, must return empty
        let input = vec![0u8; 10];
        let result = strip_hash(input, 24);
        assert!(
            result.is_empty(),
            "expected empty vec for short input at version 24"
        );
    }

    #[test]
    fn strip_hash_exactly_32_bytes_at_version_24_returns_empty_not_panic() {
        // Exactly 32 bytes → plaintext is empty after stripping the hash
        let input = vec![0xFFu8; 32];
        let result = strip_hash(input, 24);
        assert!(result.is_empty());
    }

    #[test]
    fn strip_hash_version_23_does_not_strip() {
        // Boundary: 23 is the last version that does NOT strip
        let mut input = vec![0xABu8; 32];
        input.extend_from_slice(b"val");
        let expected = input.clone();
        let result = strip_hash(input, 23);
        assert_eq!(result, expected);
    }
}
