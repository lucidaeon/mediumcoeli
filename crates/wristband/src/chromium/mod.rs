//! Chromium-family cookie reader — raw row extraction, schema metadata, and
//! the unified platform-dispatching entry point.
//!
//! This module handles the database layer for all Chromium-based browsers
//! (Chrome, Edge, Brave, Opera, Vivaldi, Whale, Chromium).
//!
//! # Entry point
//!
//! [`read_chromium`] is the single function that `lib.rs` calls for every
//! Chromium-family browser on every platform. It:
//!
//! 1. Calls [`crate::discover::locate_store`] to find the `Cookies` `SQLite`
//!    file and browser root.
//! 2. Selects the platform-specific key-derivation path at compile time
//!    (macOS → `decrypt_macos::key_for`; Linux → `decrypt_linux::key_for`;
//!    Windows → `decrypt_windows::master_key`).
//! 3. Reads the `meta.version` row via [`meta_version`].
//! 4. Runs [`crate::gate::gate`] over [`chromium_rows`], passing the
//!    platform-specific decrypt closure.
//!
//! # Columns read
//!
//! From the `cookies` table:
//! `host_key`, `name`, `encrypted_value`, `path`, `expires_utc`, `is_secure`.
//!
//! # Epoch conversion
//!
//! Chromium stores `expires_utc` as **microseconds since 1601-01-01 00:00:00
//! UTC** (the Windows FILETIME epoch). Converting to Unix seconds:
//!
//! ```text
//! unix_seconds = expires_utc / 1_000_000 - 11_644_473_600
//! ```
//!
//! The constant `11_644_473_600` is the number of seconds between
//! 1601-01-01 and 1970-01-01 (Unix epoch). A value of `0` in the database
//! means the cookie is a session cookie (no expiry); these are mapped to
//! `None`.
//!
//! # Filtering
//!
//! Host filtering is **not** performed here. It is the gate's responsibility
//! (INV-2). The `allow` parameter is accepted for API symmetry and may be
//! used in future optimisations, but no SQL `WHERE host_key` clause is added.

pub(crate) mod decrypt_linux;
#[cfg(target_os = "macos")]
pub(crate) mod decrypt_macos;
pub(crate) mod decrypt_windows;
pub(crate) mod framing;

use std::path::Path;

use rusqlite::params;

use crate::Browser;
use crate::cookie::{Cookie, RawRow};
use crate::domain::Domain;
use crate::error::WristbandError;
use crate::sqlite_copy::{copy_db, open_ro};

/// Number of seconds between the Windows FILETIME epoch (1601-01-01) and the
/// Unix epoch (1970-01-01).
const FILETIME_TO_UNIX_OFFSET_SECS: i64 = 11_644_473_600;

// ---------------------------------------------------------------------------
// Unified Chromium reader — single entry point for all Chromium browsers
// ---------------------------------------------------------------------------

/// Read cookies for `domains` from a Chromium-family `browser`.
///
/// This is the single dispatcher that `lib.rs` calls for all seven Chromium
/// browser variants on all platforms. It:
///
/// 1. Calls [`crate::discover::locate_store`] to find the `Cookies` `SQLite`
///    file and browser root directory.
/// 2. Selects the compile-time platform decrypt path:
///    - **macOS** — `decrypt_macos::key_for(browser)` → AES-128-CBC.
///    - **Linux** — `decrypt_linux::detect_keyring` + `key_for(browser, keyring)` → AES-128-CBC.
///    - **Windows** — `decrypt_windows::master_key(root)` → AES-256-GCM.
///    - **Other** — returns `Unsupported`.
/// 3. Reads [`meta_version`] from the `meta` table.
/// 4. Runs [`crate::gate::gate`] over [`chromium_rows`] with the platform
///    decrypt closure (INV-2: filter before decrypt).
///
/// # Errors
///
/// - [`WristbandError::NoStore`] — no `Cookies` file found for `browser`.
/// - [`WristbandError::Keychain`] — key retrieval failed.
/// - [`WristbandError::Sqlite`] — database query failed.
/// - [`WristbandError::Unsupported`] — no Chromium decrypt path for this OS.
pub(crate) fn read_chromium(
    browser: Browser,
    domains: &[Domain],
    profile: Option<&str>,
) -> Result<Vec<Cookie>, WristbandError> {
    use crate::discover::StorePath;

    let store = crate::discover::locate_store(browser, profile)?;
    let StorePath::ChromiumSqlite(db, root) = store else {
        return Err(WristbandError::Unsupported(
            "unexpected StorePath variant for Chromium browser".to_owned(),
        ));
    };

    read_chromium_from_paths(browser, domains, &db, &root)
}

/// Inner helper: given a resolved `db` path and `browser_root`, select the
/// platform key, read rows, and run the gate.
///
/// Split out so that integration tests can inject a synthetic db without
/// touching the real Keychain / keyring.
#[cfg(target_os = "macos")]
pub(crate) fn read_chromium_from_paths(
    browser: Browser,
    domains: &[Domain],
    db: &Path,
    _root: &Path,
) -> Result<Vec<Cookie>, WristbandError> {
    use crate::gate::gate;
    let key = decrypt_macos::key_for(browser)?;
    let meta = meta_version(db)?;
    Ok(gate(chromium_rows(db, domains)?, domains, |row| {
        decrypt_macos::decrypt(row, &key, meta)
    }))
}

/// Inner helper — Linux platform arm.
#[cfg(target_os = "linux")]
pub(crate) fn read_chromium_from_paths(
    browser: Browser,
    domains: &[Domain],
    db: &Path,
    _root: &Path,
) -> Result<Vec<Cookie>, WristbandError> {
    use crate::gate::gate;
    let keyring = decrypt_linux::detect_keyring(&|var| std::env::var(var).ok());
    let key = decrypt_linux::key_for(browser, keyring)?;
    let meta = meta_version(db)?;
    Ok(gate(chromium_rows(db, domains)?, domains, |row| {
        decrypt_linux::decrypt(row, &key, meta)
    }))
}

/// Inner helper — Windows platform arm.
#[cfg(target_os = "windows")]
pub(crate) fn read_chromium_from_paths(
    browser: Browser,
    domains: &[Domain],
    db: &Path,
    root: &Path,
) -> Result<Vec<Cookie>, WristbandError> {
    use crate::gate::gate;
    let _ = browser;
    let key = decrypt_windows::master_key(root)?;
    let meta = meta_version(db)?;
    Ok(gate(chromium_rows(db, domains)?, domains, |row| {
        decrypt_windows::decrypt(row, &key, meta)
    }))
}

/// Inner helper — unsupported platform arm.
#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
pub(crate) fn read_chromium_from_paths(
    _browser: Browser,
    _domains: &[Domain],
    _db: &Path,
    _root: &Path,
) -> Result<Vec<Cookie>, WristbandError> {
    Err(WristbandError::Unsupported(
        "Chromium decryption not implemented for this OS".to_owned(),
    ))
}

/// Read all cookie rows from a Chromium `Cookies` `SQLite` database.
///
/// Copies the database via [`copy_db`] before opening (INV-5), then reads
/// all rows from the `cookies` table.
///
/// The `allow` parameter is accepted for API symmetry with the gate but host
/// filtering is intentionally **not** applied here — filtering happens inside
/// [`crate::gate::gate`] (INV-2).
///
/// # Returns
///
/// A `Vec<RawRow>` where each row has:
/// - `encrypted_value` set to the raw blob from the database.
/// - `plaintext_value` set to `None` (Chromium values are always encrypted).
/// - `expires_unix` set to `None` for session cookies (`expires_utc == 0`),
///   or the converted Unix timestamp otherwise.
///
/// # Errors
///
/// Returns [`WristbandError::Io`] if the database copy fails, or
/// [`WristbandError::Sqlite`] if any query fails.
pub(crate) fn chromium_rows(db: &Path, _allow: &[Domain]) -> Result<Vec<RawRow>, WristbandError> {
    let db_copy = copy_db(db)?;
    let conn = open_ro(&db_copy.path)?;

    let mut stmt = conn
        .prepare(
            "SELECT host_key, name, encrypted_value, path, expires_utc, is_secure \
             FROM cookies",
        )
        .map_err(|e| WristbandError::Sqlite(e.to_string()))?;

    let rows: Vec<RawRow> = stmt
        .query_map(params![], |row| {
            let host: String = row.get(0)?;
            let name: String = row.get(1)?;
            let encrypted_value: Vec<u8> = row.get(2)?;
            let path: String = row.get(3)?;
            let expires_utc: i64 = row.get(4)?;
            let is_secure: i64 = row.get(5)?;
            Ok((host, name, encrypted_value, path, expires_utc, is_secure))
        })
        .map_err(|e| WristbandError::Sqlite(e.to_string()))?
        .filter_map(std::result::Result::ok)
        .map(
            |(host, name, encrypted_value, path, expires_utc, is_secure)| {
                // Convert Chromium's Windows-epoch microseconds to Unix seconds.
                // expires_utc == 0 means session cookie (no expiry).
                let expires_unix = if expires_utc == 0 {
                    None
                } else {
                    Some(expires_utc / 1_000_000 - FILETIME_TO_UNIX_OFFSET_SECS)
                };
                RawRow {
                    host,
                    name,
                    path,
                    secure: is_secure != 0,
                    expires_unix,
                    encrypted_value,
                    plaintext_value: None,
                }
            },
        )
        .collect();

    Ok(rows)
}

/// Read the Chromium schema meta-version from the `meta` table.
///
/// This is used by [`framing::strip_hash`] to decide whether to strip the
/// 32-byte SHA-256 domain hash prepended to decrypted cookie values in
/// Chromium meta-version ≥ 24.
///
/// # Errors
///
/// Returns [`WristbandError::Io`] if the database copy fails, or
/// [`WristbandError::Sqlite`] if the query fails or the row is missing.
pub(crate) fn meta_version(db: &Path) -> Result<i64, WristbandError> {
    let db_copy = copy_db(db)?;
    let conn = open_ro(&db_copy.path)?;

    // The `meta.value` column is declared `LONGVARCHAR` (text) in the real
    // Chromium schema; the version is stored as a numeric string such as "24".
    // Using `CAST(... AS INTEGER)` lets SQLite coerce both text and integer
    // storage transparently, avoiding a type-mismatch error from rusqlite.
    conn.query_row(
        "SELECT CAST(value AS INTEGER) FROM meta WHERE key = 'version'",
        params![],
        |row| row.get::<_, i64>(0),
    )
    .map_err(|e| WristbandError::Sqlite(e.to_string()))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use tempfile::TempDir;

    // -----------------------------------------------------------------------
    // Fixture helpers — build a synthetic Chromium-shaped cookies database
    // -----------------------------------------------------------------------

    /// Schema constants matching the real Chromium `Cookies` database.
    ///
    /// Column widths match those written by Chromium; the tests only use the
    /// columns listed in the SELECT.
    const CREATE_COOKIES: &str = "
        CREATE TABLE cookies (
            creation_utc    INTEGER NOT NULL,
            host_key        TEXT NOT NULL,
            top_frame_site_key TEXT NOT NULL DEFAULT '',
            name            TEXT NOT NULL,
            value           TEXT NOT NULL DEFAULT '',
            encrypted_value BLOB NOT NULL DEFAULT x'',
            path            TEXT NOT NULL,
            expires_utc     INTEGER NOT NULL,
            is_secure       INTEGER NOT NULL,
            is_httponly     INTEGER NOT NULL DEFAULT 0,
            last_access_utc INTEGER NOT NULL DEFAULT 0,
            has_expires     INTEGER NOT NULL DEFAULT 1,
            is_persistent   INTEGER NOT NULL DEFAULT 1,
            priority        INTEGER NOT NULL DEFAULT 1,
            samesite        INTEGER NOT NULL DEFAULT -1,
            source_scheme   INTEGER NOT NULL DEFAULT 0,
            source_port     INTEGER NOT NULL DEFAULT -1,
            last_update_utc INTEGER NOT NULL DEFAULT 0,
            source_type     INTEGER NOT NULL DEFAULT 0
        );
    ";

    const CREATE_META: &str = "
        CREATE TABLE meta (
            key   LONGVARCHAR NOT NULL UNIQUE PRIMARY KEY,
            value LONGVARCHAR
        );
    ";

    /// Build a synthetic Chromium-shaped `Cookies` database at `path`.
    fn make_chromium_db(dir: &TempDir, meta_ver: i64) -> std::path::PathBuf {
        let path = dir.path().join("Cookies");
        let conn = Connection::open(&path).expect("open test db");
        // Real Chromium stores `meta.value` as text (LONGVARCHAR), so quote the
        // version to give the fixture text affinity and exercise the CAST in
        // `meta_version` under realistic conditions.
        conn.execute_batch(&format!(
            "{CREATE_COOKIES}\n{CREATE_META}\n\
             INSERT INTO meta (key, value) VALUES ('version', '{meta_ver}');"
        ))
        .expect("create schema");
        path
    }

    /// Insert one row into the `cookies` table.
    #[allow(clippy::too_many_arguments)]
    fn insert_cookie(
        conn: &Connection,
        host_key: &str,
        name: &str,
        encrypted_value: &[u8],
        path: &str,
        expires_utc: i64,
        is_secure: i64,
    ) {
        conn.execute(
            "INSERT INTO cookies
             (creation_utc, host_key, name, encrypted_value, path, expires_utc,
              is_secure, is_httponly)
             VALUES (0, ?1, ?2, ?3, ?4, ?5, ?6, 0)",
            params![
                host_key,
                name,
                encrypted_value,
                path,
                expires_utc,
                is_secure
            ],
        )
        .expect("insert cookie row");
    }

    // -----------------------------------------------------------------------
    // meta_version
    // -----------------------------------------------------------------------

    #[test]
    fn meta_version_returns_stored_value() {
        let dir = TempDir::new().unwrap();
        let db_path = make_chromium_db(&dir, 24);
        let ver = meta_version(&db_path).expect("meta_version");
        assert_eq!(ver, 24);
    }

    #[test]
    fn meta_version_returns_older_value() {
        let dir = TempDir::new().unwrap();
        let db_path = make_chromium_db(&dir, 18);
        let ver = meta_version(&db_path).expect("meta_version");
        assert_eq!(ver, 18);
    }

    // -----------------------------------------------------------------------
    // chromium_rows — basic row reading
    // -----------------------------------------------------------------------

    #[test]
    fn chromium_rows_reads_all_rows_regardless_of_host() {
        let dir = TempDir::new().unwrap();
        let db_path = make_chromium_db(&dir, 20);

        {
            let conn = Connection::open(&db_path).unwrap();
            insert_cookie(&conn, "astro.com", "session", b"v10\x01\x02", "/", 0, 0);
            insert_cookie(&conn, "evil.net", "steal", b"v10\x03\x04", "/", 0, 0);
        }

        // No allow-list filtering here — all rows are returned.
        let allow = [crate::domain::Domain::explicit("astro.com").unwrap()];
        let rows = chromium_rows(&db_path, &allow).expect("chromium_rows");

        assert_eq!(
            rows.len(),
            2,
            "both rows must be returned (filtering is the gate's job)"
        );
    }

    #[test]
    fn chromium_rows_encrypted_value_round_trips() {
        let dir = TempDir::new().unwrap();
        let db_path = make_chromium_db(&dir, 20);
        let blob = b"v10\x00\x11\x22\x33";

        {
            let conn = Connection::open(&db_path).unwrap();
            insert_cookie(&conn, "astro.com", "tok", blob, "/app", 0, 1);
        }

        let allow: &[crate::domain::Domain] = &[];
        let rows = chromium_rows(&db_path, allow).expect("chromium_rows");

        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert_eq!(row.host, "astro.com");
        assert_eq!(row.name, "tok");
        assert_eq!(row.path, "/app");
        assert!(row.secure);
        assert_eq!(row.encrypted_value, blob.as_slice());
        assert!(
            row.plaintext_value.is_none(),
            "plaintext_value must always be None for Chromium"
        );
    }

    // -----------------------------------------------------------------------
    // expires_utc → unix seconds conversion
    // -----------------------------------------------------------------------

    /// `expires_utc == 0` → session cookie → `expires_unix == None`.
    #[test]
    fn zero_expires_utc_maps_to_none() {
        let dir = TempDir::new().unwrap();
        let db_path = make_chromium_db(&dir, 20);
        {
            let conn = Connection::open(&db_path).unwrap();
            insert_cookie(&conn, "astro.com", "sess", b"v10\x01", "/", 0, 0);
        }
        let rows = chromium_rows(&db_path, &[]).expect("chromium_rows");
        assert_eq!(rows[0].expires_unix, None);
    }

    // A known `expires_utc` value should convert correctly to Unix seconds.
    //
    // Verification: 2026-01-01 00:00:00 UTC as Windows FILETIME microseconds.
    //
    // Unix timestamp for 2026-01-01: 1_767_225_600
    // Windows FILETIME microseconds: (1_767_225_600 + 11_644_473_600) * 1_000_000
    //                               = 13_411_699_200 * 1_000_000
    //                               = 13_411_699_200_000_000
    #[test]
    fn known_expires_utc_converts_to_correct_unix_timestamp() {
        let dir = TempDir::new().unwrap();
        let db_path = make_chromium_db(&dir, 20);

        // 2026-01-01 00:00:00 UTC
        let expires_utc: i64 = 13_411_699_200_000_000;
        let expected_unix: i64 = 1_767_225_600;

        {
            let conn = Connection::open(&db_path).unwrap();
            insert_cookie(&conn, "astro.com", "exp", b"v10\x01", "/", expires_utc, 0);
        }

        let rows = chromium_rows(&db_path, &[]).expect("chromium_rows");
        assert_eq!(rows[0].expires_unix, Some(expected_unix));
    }

    #[test]
    fn non_zero_expires_utc_maps_to_some() {
        let dir = TempDir::new().unwrap();
        let db_path = make_chromium_db(&dir, 20);
        // A small nonzero value — just check it comes back as Some
        let expires_utc: i64 = 13_000_000_000_000_000;
        {
            let conn = Connection::open(&db_path).unwrap();
            insert_cookie(&conn, "astro.com", "x", b"v10\x01", "/", expires_utc, 0);
        }
        let rows = chromium_rows(&db_path, &[]).expect("chromium_rows");
        assert!(rows[0].expires_unix.is_some());
    }

    // -----------------------------------------------------------------------
    // is_secure flag
    // -----------------------------------------------------------------------

    #[test]
    fn is_secure_flag_round_trips() {
        let dir = TempDir::new().unwrap();
        let db_path = make_chromium_db(&dir, 20);
        {
            let conn = Connection::open(&db_path).unwrap();
            insert_cookie(&conn, "astro.com", "secure_yes", b"v10\x01", "/", 0, 1);
            insert_cookie(&conn, "astro.com", "secure_no", b"v10\x02", "/", 0, 0);
        }
        let mut rows = chromium_rows(&db_path, &[]).expect("chromium_rows");
        rows.sort_by(|a, b| a.name.cmp(&b.name));
        let r_no = rows.iter().find(|r| r.name == "secure_no").unwrap();
        let r_yes = rows.iter().find(|r| r.name == "secure_yes").unwrap();
        assert!(r_yes.secure);
        assert!(!r_no.secure);
    }

    // -----------------------------------------------------------------------
    // Empty database returns empty vec
    // -----------------------------------------------------------------------

    #[test]
    fn empty_database_returns_empty_vec() {
        let dir = TempDir::new().unwrap();
        let db_path = make_chromium_db(&dir, 20);
        let rows = chromium_rows(&db_path, &[]).expect("chromium_rows");
        assert!(rows.is_empty());
    }

    // -----------------------------------------------------------------------
    // Integration: chromium_rows → gate → platform decrypt (macOS)
    //
    // Tests the end-to-end path WITHOUT calling key_for() (no Keychain I/O).
    // We derive a known key, encrypt values with it, write them to a synthetic
    // SQLite database, then call chromium_rows + gate + decrypt_macos::decrypt
    // directly.  This proves the integration minus the Keychain lookup.
    // -----------------------------------------------------------------------

    #[cfg(target_os = "macos")]
    mod integration_macos {
        use super::*;
        use crate::domain::Domain;
        use crate::gate::gate;
        use std::cell::Cell;

        /// Encrypt `plaintext` with AES-128-CBC (16-space IV, PKCS7) and
        /// prepend `b"v10"`.  Uses the macOS path constants.
        fn v10_encrypt_macos(key: &[u8; 16], plaintext: &[u8]) -> Vec<u8> {
            use aes::Aes128;
            use cbc::cipher::{BlockEncryptMut, KeyIvInit, block_padding::Pkcs7};
            type Aes128CbcEnc = cbc::Encryptor<Aes128>;

            const IV: [u8; 16] = [0x20u8; 16];
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

        /// Build a synthetic Chromium db with specific rows pre-encrypted.
        ///
        /// Returns: (`db_path`, `TempDir` keeping the dir alive).
        fn make_integration_db(
            key: &[u8; 16],
            meta_ver: i64,
            rows: &[(&str, &str, &[u8])], // (host, name, plaintext)
        ) -> (std::path::PathBuf, TempDir) {
            let dir = TempDir::new().unwrap();
            let db_path = make_chromium_db(&dir, meta_ver);
            let conn = Connection::open(&db_path).unwrap();
            for (host, name, plaintext) in rows {
                let enc = v10_encrypt_macos(key, plaintext);
                insert_cookie(&conn, host, name, &enc, "/", 0, 1);
            }
            (db_path, dir)
        }

        /// Derive the macOS PBKDF2 key from a known password (same as
        /// `decrypt_macos::derive_key` but accessible here).
        fn derive_key(password: &[u8]) -> [u8; 16] {
            use pbkdf2::pbkdf2_hmac;
            use sha1::Sha1;
            let mut key = [0u8; 16];
            pbkdf2_hmac::<Sha1>(password, b"saltysalt", 1003, &mut key);
            key
        }

        /// End-to-end: `chromium_rows` → gate → `decrypt_macos::decrypt`.
        ///
        /// Three cookies:
        ///   - allowed host "astro.com" → must be decrypted and returned.
        ///   - allowed subdomain "sub.astro.com" → must be decrypted and returned.
        ///   - non-allowed host "evil.net" → gate must skip WITHOUT calling decrypt.
        #[test]
        fn end_to_end_gate_and_decrypt_macos_known_key() {
            let key_bytes = derive_key(b"test_integration_password");
            let key = crate::chromium::framing::Key(key_bytes);

            let (db, _dir) = make_integration_db(
                &key_bytes,
                20,
                &[
                    ("astro.com", "session", b"session_value_42"),
                    (".sub.astro.com", "token", b"token_xyz"),
                    ("evil.net", "steal", b"secret_data"),
                ],
            );

            let allow = [Domain::explicit("astro.com").unwrap()];
            let rows = chromium_rows(&db, &allow).expect("chromium_rows");
            assert_eq!(rows.len(), 3, "all 3 rows read (gate filters, not rows)");

            let decrypt_calls = Cell::new(0u32);
            let evil_decrypted = Cell::new(false);

            let cookies = gate(rows, &allow, |row| {
                decrypt_calls.set(decrypt_calls.get() + 1);
                if row.host.contains("evil") {
                    evil_decrypted.set(true);
                }
                decrypt_macos::decrypt(row, &key, 20)
            });

            // Gate must call decrypt exactly twice (for the two allowed hosts).
            assert_eq!(
                decrypt_calls.get(),
                2,
                "decrypt must be called exactly for the 2 allowed-host rows"
            );
            // INV-2: evil.net must NEVER be decrypted.
            assert!(
                !evil_decrypted.get(),
                "decrypt must never be called for evil.net (INV-2)"
            );
            // Output must have exactly 2 cookies.
            assert_eq!(cookies.len(), 2, "two cookies expected in output");

            // Values must round-trip correctly.
            let sess = cookies
                .iter()
                .find(|c| c.name == "session")
                .expect("session cookie");
            assert_eq!(sess.host, "astro.com");
            assert_eq!(sess.value, "session_value_42");

            let tok = cookies
                .iter()
                .find(|c| c.name == "token")
                .expect("token cookie");
            assert_eq!(tok.host, "sub.astro.com"); // leading dot stripped
            assert_eq!(tok.value, "token_xyz");

            // INV-6: all output hosts must be within the allow-list.
            for c in &cookies {
                assert!(
                    crate::host_matches(&c.host, &allow),
                    "output host {} not in allow-list",
                    c.host
                );
            }
        }

        /// Same fixture at `meta_version` = 24: the 32-byte hash prefix must be
        /// stripped from each decrypted value.
        #[test]
        fn end_to_end_gate_and_decrypt_macos_strips_hash_at_v24() {
            let key_bytes = derive_key(b"another_test_password");
            let key = crate::chromium::framing::Key(key_bytes);

            // Build plaintexts WITH the 32-byte hash prefix (as Chromium v24+ does).
            let hash_prefix = vec![0xABu8; 32];
            let mut with_hash_a = hash_prefix.clone();
            with_hash_a.extend_from_slice(b"real_value_a");
            let mut with_hash_b = hash_prefix.clone();
            with_hash_b.extend_from_slice(b"real_value_b");

            let dir = TempDir::new().unwrap();
            let db_path = make_chromium_db(&dir, 24);
            {
                let conn = Connection::open(&db_path).unwrap();
                let enc_a = v10_encrypt_macos(&key_bytes, &with_hash_a);
                let enc_b = v10_encrypt_macos(&key_bytes, &with_hash_b);
                insert_cookie(&conn, "astro.com", "cookie_a", &enc_a, "/", 0, 1);
                insert_cookie(&conn, "other.net", "cookie_b", &enc_b, "/", 0, 1);
            }

            let allow = [Domain::explicit("astro.com").unwrap()];
            let rows = chromium_rows(&db_path, &allow).expect("chromium_rows");
            let cookies = gate(rows, &allow, |row| decrypt_macos::decrypt(row, &key, 24));

            assert_eq!(cookies.len(), 1);
            assert_eq!(cookies[0].name, "cookie_a");
            assert_eq!(
                cookies[0].value, "real_value_a",
                "32-byte hash prefix must be stripped at meta_version 24"
            );
        }
    }

    // -----------------------------------------------------------------------
    // Integration: read_chromium end-to-end with real Keychain
    // (ignored — requires a real browser installation)
    // -----------------------------------------------------------------------

    /// Full `read_chromium` path through the Keychain.
    ///
    /// Marked `#[ignore]` because it requires Chrome installed with the real
    /// macOS Keychain entry and a `~/Library/Application Support/Google/Chrome`
    /// profile. Remove the ignore on a machine with Chrome installed to run.
    #[test]
    #[ignore = "requires a real Chrome installation with Keychain access"]
    #[cfg(target_os = "macos")]
    fn read_chromium_live_chrome_keychain() {
        let allow = [crate::domain::Domain::explicit("google.com").unwrap()];
        let result = read_chromium(Browser::Chrome, &allow, None);
        // We can't assert specific cookies, but the call must not error with
        // Unsupported or Keychain (i.e. Chrome is installed and key is readable).
        match result {
            Ok(cookies) => {
                println!(
                    "read_chromium Chrome: {} cookies for google.com",
                    cookies.len()
                );
            }
            Err(e) => panic!("read_chromium live failed: {e}"),
        }
    }
}
