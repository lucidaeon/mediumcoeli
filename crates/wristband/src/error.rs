//! Error types for the `wristband` crate.

use thiserror::Error;

/// Errors that can be returned by `wristband` operations.
#[derive(Debug, Error)]
pub enum WristbandError {
    /// The caller supplied an empty allow-list; at least one `Domain` is required.
    #[error("allow-list must not be empty")]
    EmptyAllowList,

    /// The supplied string is not a valid domain (empty, contains wildcards, scheme,
    /// path, whitespace, or is a single label with no dot).
    #[error("invalid domain: {0}")]
    InvalidDomain(String),

    /// The supplied string is a public suffix / eTLD and would span an entire zone.
    /// Only registrable domains (eTLD+1) or deeper subdomains are accepted.
    #[error("public suffix rejected (would span zone): {0}")]
    PublicSuffix(String),

    /// The requested browser or feature is not yet supported by a backend.
    ///
    /// This variant is returned by stub implementations while platform-specific
    /// backends are being built out. It will not appear once all backends are
    /// complete.
    #[error("not yet supported: {0}")]
    Unsupported(String),

    /// A filesystem I/O error (e.g. creating the temp directory or copying a
    /// file during the copy-before-read step).
    #[error("I/O error: {0}")]
    Io(String),

    /// A `SQLite` error returned by rusqlite (stringified to keep rusqlite out of
    /// the public error surface).
    #[error("SQLite error: {0}")]
    Sqlite(String),

    /// No cookie store could be located for the named browser on this OS.
    ///
    /// The string is the browser name (e.g. `"Firefox"`, `"Chrome"`).
    #[error("no cookie store found for {0}")]
    NoStore(String),

    /// The macOS Keychain lookup for the browser's Safe Storage password
    /// failed (e.g. the browser is not installed, permission denied, or the
    /// `security` tool is unavailable).
    #[error("Keychain error: {0}")]
    Keychain(String),

    /// Cookie value decryption failed (e.g. bad padding, wrong key).
    ///
    /// This variant is intentionally not returned by the crate-internal
    /// macOS Chromium `decrypt` path (which returns `None` for graceful
    /// degradation), but is available for
    /// callers that need to distinguish a total decryption failure from
    /// `None`-from-`gate`.
    #[error("decryption error: {0}")]
    Decrypt(String),

    /// A binary-format parse error (e.g. bad magic, truncated buffer, invalid
    /// offsets in a `binarycookies` file).
    #[error("parse error: {0}")]
    Parse(String),
}
