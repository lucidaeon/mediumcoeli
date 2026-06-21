//! Firefox plaintext cookie reader.
//!
//! Firefox stores cookie values in plaintext in `moz_cookies` inside
//! `cookies.sqlite`. This module reads that database — via the
//! copy-before-read discipline (INV-5) — and routes every row through
//! [`crate::gate::gate`] so that host filtering is always applied before
//! any value is exposed (INV-2).
//!
//! Container scoping uses the `originAttributes` column together with
//! an optional `containers.json` metadata file. See [`Container`] and
//! [`read_firefox`] for the full filtering logic.

use std::path::Path;

use rusqlite::params;

use crate::Container;
use crate::cookie::{Cookie, RawRow};
use crate::domain::Domain;
use crate::error::WristbandError;
use crate::gate::gate;
use crate::sqlite_copy::{copy_db, open_ro};

// ---------------------------------------------------------------------------
// containers.json helpers
// ---------------------------------------------------------------------------

/// Resolve a container name to its numeric `userContextId` from Firefox's
/// `containers.json` file inside the profile directory.
///
/// Returns `None` if the file does not exist, cannot be parsed, or does not
/// contain an identity with the given name.
// Called only from container_matches; both are dead until Task 6 wires read_firefox in.
#[allow(dead_code)]
fn resolve_container_id(profile_dir: &Path, name: &str) -> Option<u32> {
    let path = profile_dir.join("containers.json");
    let text = std::fs::read_to_string(&path).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    let identities = v.get("identities")?.as_array()?;
    for identity in identities {
        if identity.get("name")?.as_str()? == name {
            return identity
                .get("userContextId")?
                .as_u64()
                .and_then(|n| u32::try_from(n).ok());
        }
    }
    None
}

/// Return `true` when `origin_attributes` contains an exact `userContextId=<id>`
/// token.
///
/// Firefox encodes `originAttributes` as `^key=value&key=value…`.  A plain
/// substring search on `"userContextId=2"` would also match
/// `"userContextId=20"` or `"userContextId=200"`.  This function splits on
/// the `^` and `&` delimiters and compares the resulting tokens by value, so
/// each numeric id is matched precisely.
// Called from container_matches; dead until Task 6 wires read_firefox in.
#[allow(dead_code)]
fn origin_attrs_has_context(origin_attributes: &str, id: u32) -> bool {
    let want = format!("userContextId={id}");
    origin_attributes.split(['^', '&']).any(|tok| tok == want)
}

/// Return `true` if `origin_attributes` matches the given container filter.
///
/// - [`Container::None`] — keep rows where `originAttributes` is empty.
/// - [`Container::Named(n)`] — resolve `n` → id via `containers.json`, then
///   keep rows where `originAttributes` has an exact `userContextId=<id>`
///   token (delimited by `^` or `&`).
/// - [`Container::Id(id)`] — same as `Named` but with the id already known.
/// - [`Container::All`] — keep everything.
// Called from read_firefox; dead until Task 6 wires read_firefox in.
#[allow(dead_code)]
fn container_matches(origin_attributes: &str, container: &Container, profile_dir: &Path) -> bool {
    match container {
        Container::All => true,
        Container::None => origin_attributes.is_empty(),
        Container::Id(id) => origin_attrs_has_context(origin_attributes, *id),
        Container::Named(name) => match resolve_container_id(profile_dir, name) {
            Some(id) => origin_attrs_has_context(origin_attributes, id),
            // Unknown name → no match.
            None => false,
        },
    }
}

// ---------------------------------------------------------------------------
// Schema version → expiry unit
// ---------------------------------------------------------------------------

/// Return `true` when `expiry` is stored in **milliseconds** (schema ≥ 16).
///
/// Firefox 142 changed `moz_cookies.expiry` from seconds to milliseconds at
/// schema version 16. `PRAGMA user_version` returns the moz schema version
/// for `cookies.sqlite`.
// Called from read_firefox; dead until Task 6 wires read_firefox in.
#[allow(dead_code)]
fn expiry_is_millis(conn: &rusqlite::Connection) -> bool {
    conn.query_row("PRAGMA user_version", params![], |row| row.get::<_, i64>(0))
        .is_ok_and(|v| v >= 16)
}

// ---------------------------------------------------------------------------
// Public(crate) entry point
// ---------------------------------------------------------------------------

/// Read Firefox cookies from `profile_dir/cookies.sqlite`.
///
/// Steps:
/// 1. Copy `cookies.sqlite` (and any `-wal`/`-shm` sidecars) into a fresh
///    tempdir using [`copy_db`] (INV-5).
/// 2. Open the copy read-only with [`open_ro`].
/// 3. Read `PRAGMA user_version` to determine the expiry unit.
/// 4. Query `moz_cookies` and build [`RawRow`] values, applying container
///    filtering via `originAttributes`.
/// 5. Pass the rows to [`gate`] with a no-op decrypt closure — Firefox values
///    are plaintext, so the gate uses `plaintext_value` directly (INV-2).
///
/// # Errors
///
/// Returns [`WristbandError::Io`] if `cookies.sqlite` cannot be located or
/// copied, and [`WristbandError::Sqlite`] for any query failure.
// Task 6 (discovery) will wire this into read_cookies; allow dead_code until then.
#[allow(dead_code)]
pub(crate) fn read_firefox(
    profile_dir: &Path,
    allow: &[Domain],
    container: &Container,
) -> Result<Vec<Cookie>, WristbandError> {
    let db_path = profile_dir.join("cookies.sqlite");
    let db_copy = copy_db(&db_path)?;
    let conn = open_ro(&db_copy.path)?;

    let millis = expiry_is_millis(&conn);

    let mut stmt = conn
        .prepare(
            "SELECT host, name, value, path, expiry, isSecure, originAttributes \
             FROM moz_cookies",
        )
        .map_err(|e| WristbandError::Sqlite(e.to_string()))?;

    let rows: Vec<RawRow> = stmt
        .query_map(params![], |row| {
            let host: String = row.get(0)?;
            let name: String = row.get(1)?;
            let value: String = row.get(2)?;
            let path: String = row.get(3)?;
            let expiry_raw: i64 = row.get(4)?;
            let is_secure: i64 = row.get(5)?;
            let origin_attrs: String = row.get(6)?;
            Ok((host, name, value, path, expiry_raw, is_secure, origin_attrs))
        })
        .map_err(|e| WristbandError::Sqlite(e.to_string()))?
        .filter_map(std::result::Result::ok)
        .filter(|(_, _, _, _, _, _, origin_attrs)| {
            container_matches(origin_attrs, container, profile_dir)
        })
        .map(|(host, name, value, path, expiry_raw, is_secure, _)| {
            let expires_unix = if millis {
                Some(expiry_raw / 1000)
            } else {
                Some(expiry_raw)
            };
            RawRow {
                host,
                name,
                path,
                secure: is_secure != 0,
                expires_unix,
                encrypted_value: vec![],
                plaintext_value: Some(value),
            }
        })
        .collect();

    // INV-2: gate applies host filtering before any value is examined.
    // Firefox is plaintext — the decrypt closure is never called.
    Ok(gate(rows, allow, |_| None))
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
    // Fixture helpers
    // -----------------------------------------------------------------------

    /// Create a `cookies.sqlite` file inside `dir` that matches the real
    /// Firefox schema (columns used by `read_firefox`).
    ///
    /// `schema_version` is written via `PRAGMA user_version = N` so that we can
    /// test the seconds-vs-milliseconds expiry conversion.
    fn make_firefox_db(dir: &TempDir, schema_version: u32) -> std::path::PathBuf {
        let path = dir.path().join("cookies.sqlite");
        let conn = Connection::open(&path).expect("open test db");
        conn.execute_batch(&format!(
            "PRAGMA user_version = {schema_version};
             CREATE TABLE moz_cookies (
                 host             TEXT NOT NULL,
                 name             TEXT NOT NULL,
                 value            TEXT NOT NULL,
                 path             TEXT NOT NULL,
                 expiry           INTEGER NOT NULL,
                 isSecure         INTEGER NOT NULL,
                 originAttributes TEXT NOT NULL DEFAULT ''
             );"
        ))
        .expect("create schema");
        path
    }

    /// Insert a row into `moz_cookies`.
    #[allow(clippy::too_many_arguments)]
    fn insert_cookie(
        conn: &Connection,
        host: &str,
        name: &str,
        value: &str,
        path: &str,
        expiry: i64,
        is_secure: i64,
        origin_attrs: &str,
    ) {
        conn.execute(
            "INSERT INTO moz_cookies
             (host, name, value, path, expiry, isSecure, originAttributes)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![host, name, value, path, expiry, is_secure, origin_attrs],
        )
        .expect("insert cookie");
    }

    /// Write a minimal `containers.json` into `dir`.
    fn write_containers_json(dir: &TempDir, entries: &[(&str, u32)]) {
        let identities: Vec<serde_json::Value> = entries
            .iter()
            .map(|(name, id)| {
                serde_json::json!({
                    "userContextId": id,
                    "name": name,
                    "icon": "fingerprint",
                    "color": "blue",
                    "public": true
                })
            })
            .collect();
        let doc = serde_json::json!({
            "version": 5,
            "lastUserContextId": entries.last().map_or(0, |e| e.1),
            "identities": identities
        });
        std::fs::write(dir.path().join("containers.json"), doc.to_string())
            .expect("write containers.json");
    }

    // -----------------------------------------------------------------------
    // Test 1: domain allow-list (gate wiring)
    // -----------------------------------------------------------------------

    #[test]
    fn allow_list_keeps_allowed_domain_excludes_others() {
        let dir = TempDir::new().unwrap();
        let db_path = make_firefox_db(&dir, 10); // schema < 16 → expiry in seconds

        {
            let conn = Connection::open(&db_path).unwrap();
            insert_cookie(
                &conn,
                "astro.com",
                "session",
                "abc123",
                "/",
                9_999_999,
                1,
                "",
            );
            insert_cookie(&conn, "evil.net", "steal", "secret", "/", 9_999_999, 0, "");
            insert_cookie(&conn, "www.astro.com", "sub", "xyz", "/", 9_999_999, 0, "");
        }

        let allow = [Domain::explicit("astro.com").unwrap()];
        let cookies = read_firefox(dir.path(), &allow, &Container::None).expect("read_firefox");

        // gate keeps astro.com and www.astro.com, drops evil.net
        assert_eq!(cookies.len(), 2, "expected 2 cookies (astro.com + sub)");
        assert!(
            cookies.iter().all(|c| c.host.ends_with("astro.com")),
            "all cookies must be on astro.com"
        );
        assert!(
            cookies.iter().all(|c| c.host != "evil.net"),
            "evil.net must be excluded"
        );

        // Value must be plaintext, not garbled
        let sess = cookies.iter().find(|c| c.name == "session").unwrap();
        assert_eq!(sess.value, "abc123");
    }

    // -----------------------------------------------------------------------
    // Test 2: expiry unit — schema < 16 → seconds, ≥ 16 → ms ÷ 1000
    // -----------------------------------------------------------------------

    #[test]
    fn expiry_seconds_when_schema_lt_16() {
        let dir = TempDir::new().unwrap();
        let db_path = make_firefox_db(&dir, 15);
        {
            let conn = Connection::open(&db_path).unwrap();
            insert_cookie(&conn, "astro.com", "tok", "v", "/", 1_750_000_000, 0, "");
        }
        let allow = [Domain::explicit("astro.com").unwrap()];
        let cookies = read_firefox(dir.path(), &allow, &Container::All).unwrap();
        assert_eq!(cookies[0].expires_unix, Some(1_750_000_000));
    }

    #[test]
    fn expiry_millis_divided_when_schema_ge_16() {
        let dir = TempDir::new().unwrap();
        let db_path = make_firefox_db(&dir, 16);
        {
            let conn = Connection::open(&db_path).unwrap();
            // Store 1_750_000_000_000 ms → should come out as 1_750_000_000 s
            insert_cookie(
                &conn,
                "astro.com",
                "tok",
                "v",
                "/",
                1_750_000_000_000,
                0,
                "",
            );
        }
        let allow = [Domain::explicit("astro.com").unwrap()];
        let cookies = read_firefox(dir.path(), &allow, &Container::All).unwrap();
        assert_eq!(cookies[0].expires_unix, Some(1_750_000_000));
    }

    // -----------------------------------------------------------------------
    // Test 3: Container::None keeps only empty originAttributes
    // -----------------------------------------------------------------------

    #[test]
    fn container_none_keeps_only_default_cookies() {
        let dir = TempDir::new().unwrap();
        let db_path = make_firefox_db(&dir, 10);
        write_containers_json(&dir, &[("Work", 1), ("Personal", 2)]);
        {
            let conn = Connection::open(&db_path).unwrap();
            insert_cookie(&conn, "astro.com", "default", "d", "/", 0, 0, "");
            insert_cookie(
                &conn,
                "astro.com",
                "work",
                "w",
                "/",
                0,
                0,
                "^userContextId=1",
            );
            insert_cookie(
                &conn,
                "astro.com",
                "personal",
                "p",
                "/",
                0,
                0,
                "^userContextId=2",
            );
        }
        let allow = [Domain::explicit("astro.com").unwrap()];
        let cookies = read_firefox(dir.path(), &allow, &Container::None).unwrap();
        assert_eq!(cookies.len(), 1);
        assert_eq!(cookies[0].name, "default");
    }

    // -----------------------------------------------------------------------
    // Test 4: Container::Id filters to that id
    // -----------------------------------------------------------------------

    #[test]
    fn container_id_keeps_only_matching_id() {
        let dir = TempDir::new().unwrap();
        let db_path = make_firefox_db(&dir, 10);
        {
            let conn = Connection::open(&db_path).unwrap();
            insert_cookie(&conn, "astro.com", "default", "d", "/", 0, 0, "");
            insert_cookie(
                &conn,
                "astro.com",
                "c1",
                "v1",
                "/",
                0,
                0,
                "^userContextId=1",
            );
            insert_cookie(
                &conn,
                "astro.com",
                "c2",
                "v2",
                "/",
                0,
                0,
                "^userContextId=2",
            );
        }
        let allow = [Domain::explicit("astro.com").unwrap()];
        let cookies = read_firefox(dir.path(), &allow, &Container::Id(2)).unwrap();
        assert_eq!(cookies.len(), 1);
        assert_eq!(cookies[0].name, "c2");
    }

    // -----------------------------------------------------------------------
    // Test 5: Container::Named resolves via containers.json
    // -----------------------------------------------------------------------

    #[test]
    fn container_named_resolves_to_correct_id() {
        let dir = TempDir::new().unwrap();
        let db_path = make_firefox_db(&dir, 10);
        write_containers_json(&dir, &[("Work", 1), ("Personal", 2)]);
        {
            let conn = Connection::open(&db_path).unwrap();
            insert_cookie(&conn, "astro.com", "default", "d", "/", 0, 0, "");
            insert_cookie(
                &conn,
                "astro.com",
                "work",
                "w",
                "/",
                0,
                0,
                "^userContextId=1",
            );
            insert_cookie(
                &conn,
                "astro.com",
                "personal",
                "p",
                "/",
                0,
                0,
                "^userContextId=2",
            );
        }
        let allow = [Domain::explicit("astro.com").unwrap()];

        // Ask for the "Personal" container → should get only the id=2 row
        let cookies =
            read_firefox(dir.path(), &allow, &Container::Named("Personal".to_owned())).unwrap();
        assert_eq!(cookies.len(), 1);
        assert_eq!(cookies[0].name, "personal");
    }

    // -----------------------------------------------------------------------
    // Test 6: Container::Named with unknown name → 0 results
    // -----------------------------------------------------------------------

    #[test]
    fn container_named_unknown_returns_empty() {
        let dir = TempDir::new().unwrap();
        let db_path = make_firefox_db(&dir, 10);
        write_containers_json(&dir, &[("Work", 1)]);
        {
            let conn = Connection::open(&db_path).unwrap();
            insert_cookie(
                &conn,
                "astro.com",
                "work",
                "w",
                "/",
                0,
                0,
                "^userContextId=1",
            );
        }
        let allow = [Domain::explicit("astro.com").unwrap()];
        let cookies =
            read_firefox(dir.path(), &allow, &Container::Named("Nope".to_owned())).unwrap();
        assert!(cookies.is_empty());
    }

    // -----------------------------------------------------------------------
    // Test 7: Container::All returns every row regardless of originAttributes
    // -----------------------------------------------------------------------

    #[test]
    fn container_all_returns_every_cookie() {
        let dir = TempDir::new().unwrap();
        let db_path = make_firefox_db(&dir, 10);
        {
            let conn = Connection::open(&db_path).unwrap();
            insert_cookie(&conn, "astro.com", "default", "d", "/", 0, 0, "");
            insert_cookie(
                &conn,
                "astro.com",
                "c1",
                "v1",
                "/",
                0,
                0,
                "^userContextId=1",
            );
            insert_cookie(
                &conn,
                "astro.com",
                "c2",
                "v2",
                "/",
                0,
                0,
                "&userContextId=2",
            );
        }
        let allow = [Domain::explicit("astro.com").unwrap()];
        let cookies = read_firefox(dir.path(), &allow, &Container::All).unwrap();
        assert_eq!(cookies.len(), 3);
    }

    // -----------------------------------------------------------------------
    // Test 8: domain filter + container interact correctly
    // -----------------------------------------------------------------------

    #[test]
    fn domain_and_container_filters_are_independent() {
        let dir = TempDir::new().unwrap();
        let db_path = make_firefox_db(&dir, 10);
        {
            let conn = Connection::open(&db_path).unwrap();
            // astro.com, no container → kept
            insert_cookie(&conn, "astro.com", "a", "1", "/", 0, 0, "");
            // evil.net, no container → dropped by gate
            insert_cookie(&conn, "evil.net", "b", "2", "/", 0, 0, "");
            // astro.com, container=1 → dropped by container filter
            insert_cookie(&conn, "astro.com", "c", "3", "/", 0, 0, "^userContextId=1");
            // evil.net, container=1 → dropped by both
            insert_cookie(&conn, "evil.net", "d", "4", "/", 0, 0, "^userContextId=1");
        }
        let allow = [Domain::explicit("astro.com").unwrap()];
        let cookies = read_firefox(dir.path(), &allow, &Container::None).unwrap();
        assert_eq!(cookies.len(), 1);
        assert_eq!(cookies[0].name, "a");
        assert_eq!(cookies[0].value, "1");
    }

    // -----------------------------------------------------------------------
    // Test 9: Container::Id(2) must NOT match userContextId=20 (boundary bug)
    // -----------------------------------------------------------------------

    #[test]
    fn container_id_2_does_not_match_id_20() {
        let dir = TempDir::new().unwrap();
        let db_path = make_firefox_db(&dir, 10);
        {
            let conn = Connection::open(&db_path).unwrap();
            // Two rows for the same host: one in container 2, one in container 20.
            // An unanchored substring match would return both when id=2 is requested.
            insert_cookie(
                &conn,
                "astro.com",
                "container_2_cookie",
                "val2",
                "/",
                0,
                0,
                "^userContextId=2",
            );
            insert_cookie(
                &conn,
                "astro.com",
                "container_20_cookie",
                "val20",
                "/",
                0,
                0,
                "^userContextId=20",
            );
        }
        let allow = [Domain::explicit("astro.com").unwrap()];
        let cookies = read_firefox(dir.path(), &allow, &Container::Id(2)).unwrap();

        // Must return exactly one cookie and it must be the id=2 one.
        assert_eq!(
            cookies.len(),
            1,
            "Container::Id(2) must not match userContextId=20; got: {:?}",
            cookies.iter().map(|c| &c.name).collect::<Vec<_>>()
        );
        assert_eq!(cookies[0].name, "container_2_cookie");
        assert_eq!(cookies[0].value, "val2");
    }

    // -----------------------------------------------------------------------
    // Unit tests for the origin_attrs_has_context helper directly
    // -----------------------------------------------------------------------

    #[test]
    fn origin_attrs_has_context_exact_match_only() {
        // id=2 must NOT match "20" or "200"
        assert!(origin_attrs_has_context("^userContextId=2", 2));
        assert!(!origin_attrs_has_context("^userContextId=20", 2));
        assert!(!origin_attrs_has_context("^userContextId=200", 2));
        // id=20 should match "20" but not "2"
        assert!(origin_attrs_has_context("^userContextId=20", 20));
        assert!(!origin_attrs_has_context("^userContextId=2", 20));
        // compound attribute string
        assert!(origin_attrs_has_context(
            "^userContextId=2&firstPartyDomain=foo.com",
            2
        ));
        assert!(!origin_attrs_has_context(
            "^userContextId=20&firstPartyDomain=foo.com",
            2
        ));
        // empty string → no match
        assert!(!origin_attrs_has_context("", 2));
    }
}
