//! Cookie types: the public [`Cookie`] and the crate-internal `RawRow`.
//!
//! [`Cookie`] is the only type that carries a decrypted cookie value. It can
//! only be constructed by the crate-internal `gate()`, which enforces that the
//! host has already been matched against the caller's allow-list.
//!
//! `RawRow` is the raw storage representation produced by a backend reader
//! before the gate. It is never exposed outside the crate.

/// A single browser cookie that has passed the allow-list gate.
///
/// Instances are only produced by the crate-internal `gate()`; there is no
/// public constructor. This means every `Cookie` in existence has had its host
/// verified against the caller-supplied allow-list before any decryption
/// was attempted (INV-2).
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cookie {
    /// The host the cookie was set for (normalised, leading dot stripped).
    pub host: String,
    /// The cookie name.
    pub name: String,
    /// The decrypted cookie value.
    pub value: String,
    /// The cookie path (defaults to `"/"` when unset in the store).
    pub path: String,
    /// Whether the cookie has the `Secure` flag.
    pub secure: bool,
    /// Expiry as a Unix timestamp, or `None` for session cookies.
    pub expires_unix: Option<i64>,
}

impl Cookie {
    /// Construct a [`Cookie`] for use in tests.
    ///
    /// This constructor is available only under `#[cfg(any(test, feature =
    /// "test-support"))]` — never in production code.  It intentionally
    /// bypasses the allow-list gate so downstream crates can build synthetic
    /// `Cookie` values without needing a real browser store.
    #[cfg(any(test, feature = "test-support"))]
    #[must_use]
    pub fn for_test(host: &str, name: &str, value: &str) -> Self {
        Self {
            host: host.to_owned(),
            name: name.to_owned(),
            value: value.to_owned(),
            path: "/".to_owned(),
            secure: false,
            expires_unix: None,
        }
    }
}

/// A raw storage row from a browser's cookie database, before the allow-list
/// gate and before decryption.
///
/// This type is never exposed outside the crate. Backends fill it and pass it
/// to [`crate::gate::gate`].
// Future backends will construct RawRow; allow dead_code until they land.
#[allow(dead_code)]
pub(crate) struct RawRow {
    /// Host field from the cookie database (may have a leading `.`).
    pub host: String,
    /// Cookie name.
    pub name: String,
    /// Cookie path.
    pub path: String,
    /// Whether the `Secure` flag is set.
    pub secure: bool,
    /// Expiry as a Unix timestamp, or `None` for session cookies.
    pub expires_unix: Option<i64>,
    /// Encrypted value bytes (non-empty for Chromium-family stores).
    pub encrypted_value: Vec<u8>,
    /// Plaintext value (populated for Firefox/Safari stores that do not
    /// encrypt individual cookie values). When `Some`, the gate uses this
    /// directly without calling the decrypt closure.
    pub plaintext_value: Option<String>,
}
