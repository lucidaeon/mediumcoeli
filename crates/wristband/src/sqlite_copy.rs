//! Copy-before-read `SQLite` utility (INV-5).
//!
//! Browser cookie databases are live files. Opening them directly would
//! contend with the running browser and risk corrupting the WAL. This module
//! implements the offline, read-only discipline:
//!
//! 1. Copy the main DB — and any `-wal` / `-shm` sidecars that exist — into a
//!    freshly-created [`tempfile::TempDir`].
//! 2. Open the *copy* read-only.
//! 3. Drop [`DbCopy`] when done: the tempdir is removed automatically.
//!
//! The original path is **never written to or locked**.

use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags};
use tempfile::TempDir;

use crate::error::WristbandError;

/// An in-scope, read-only copy of a browser `SQLite` database.
///
/// Holds the [`TempDir`] alive so that the copy remains on disk for the
/// duration of the read. Dropping this value removes the entire tempdir
/// (INV-5).
// Future browser backends will construct DbCopy via copy_db(); allow dead_code
// until they land.
#[allow(dead_code)]
pub(crate) struct DbCopy {
    /// Keeps the tempdir alive; removed on drop.
    _dir: TempDir,
    /// Absolute path to the copied main database file inside the tempdir.
    pub path: PathBuf,
}

/// Copy `original` (and any sibling `-wal` / `-shm` files) into a fresh
/// [`TempDir`] and return a [`DbCopy`] whose `path` points to the copy.
// Used by future browser backends; allow dead_code until they land.
#[allow(dead_code)]
///
/// The original file is **never written to or locked**. Copying before opening
/// avoids contention with a running browser and eliminates any risk of WAL
/// corruption propagating back to the original (INV-5).
///
/// # Errors
///
/// Returns [`WristbandError::Io`] if the temp directory cannot be created or
/// any of the file copies fail.
pub(crate) fn copy_db(original: &Path) -> Result<DbCopy, WristbandError> {
    let dir = TempDir::new().map_err(|e| WristbandError::Io(e.to_string()))?;

    let file_name = original.file_name().ok_or_else(|| {
        WristbandError::Io(format!("no filename in path: {}", original.display()))
    })?;

    // Copy main database file, preserving its name so SQLite can re-associate
    // the sibling WAL/SHM files (SQLite finds them by appending "-wal"/"-shm"
    // to the database path).
    let dest = dir.path().join(file_name);
    std::fs::copy(original, &dest)
        .map_err(|e| WristbandError::Io(format!("copy {}: {e}", original.display())))?;

    // Copy sidecars if they exist alongside the original.
    for suffix in &["-wal", "-shm"] {
        let mut sidecar = original.as_os_str().to_owned();
        sidecar.push(suffix);
        let sidecar_src = PathBuf::from(sidecar);
        if sidecar_src.exists() {
            let mut sidecar_dest = file_name.to_owned();
            sidecar_dest.push(suffix);
            std::fs::copy(&sidecar_src, dir.path().join(&sidecar_dest))
                .map_err(|e| WristbandError::Io(format!("copy {}: {e}", sidecar_src.display())))?;
        }
    }

    Ok(DbCopy {
        _dir: dir,
        path: dest,
    })
}

/// Open the `SQLite` database at `path` in read-only mode.
///
/// Uses [`OpenFlags::SQLITE_OPEN_READ_ONLY`] so the file (or its WAL) can
/// never be modified through this connection.
// Used by future browser backends; allow dead_code until they land.
#[allow(dead_code)]
///
/// # Errors
///
/// Returns [`WristbandError::Sqlite`] if rusqlite cannot open the database.
pub(crate) fn open_ro(path: &Path) -> Result<Connection, WristbandError> {
    Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| WristbandError::Sqlite(e.to_string()))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;

    /// Create a minimal `SQLite` database at `path` with a single table and row.
    fn make_test_db(path: &Path) {
        let conn = Connection::open(path).expect("open source db");
        conn.execute_batch("CREATE TABLE t (val TEXT NOT NULL); INSERT INTO t VALUES ('hello');")
            .expect("create table");
        // Connection drop flushes the DB.
    }

    #[test]
    fn copy_db_copies_main_and_wal_and_shm_sidecars() {
        // Arrange: a real SQLite file plus sibling "-wal" and "-shm" files.
        let src_dir = TempDir::new().expect("source tempdir");
        let src_path = src_dir.path().join("Cookies");
        make_test_db(&src_path);

        // Create fake WAL/SHM sidecars (content need not be a real WAL/SHM).
        let wal_path = src_dir.path().join("Cookies-wal");
        std::fs::write(&wal_path, b"fake-wal-content").expect("write wal");
        let shm_path = src_dir.path().join("Cookies-shm");
        std::fs::write(&shm_path, b"fake-shm-content").expect("write shm");

        // Act: copy the DB.
        let db_copy = copy_db(&src_path).expect("copy_db should succeed");

        // Assert: the tempdir contains the main file and BOTH sidecars.
        assert!(db_copy.path.exists(), "copied main DB must exist");
        let copied_wal = db_copy.path.parent().unwrap().join("Cookies-wal");
        assert!(copied_wal.exists(), "copied -wal sidecar must exist");
        let copied_shm = db_copy.path.parent().unwrap().join("Cookies-shm");
        assert!(copied_shm.exists(), "copied -shm sidecar must exist");

        // The copied main DB must be queryable read-only.
        let conn = open_ro(&db_copy.path).expect("open_ro should succeed");
        let val: String = conn
            .query_row("SELECT val FROM t", params![], |row| row.get(0))
            .expect("query should succeed");
        assert_eq!(val, "hello");

        // Remember the tempdir path before dropping.
        let tmp_path = db_copy.path.parent().unwrap().to_owned();
        assert!(
            tmp_path.exists(),
            "tempdir must exist while DbCopy is alive"
        );

        // Drop DbCopy → tempdir must be removed (INV-5).
        drop(db_copy);
        assert!(
            !tmp_path.exists(),
            "tempdir must be removed when DbCopy is dropped"
        );
    }

    #[test]
    fn copy_db_works_without_wal_sidecar() {
        let src_dir = TempDir::new().expect("source tempdir");
        let src_path = src_dir.path().join("cookies.sqlite");
        make_test_db(&src_path);
        // No WAL file created.

        let db_copy = copy_db(&src_path).expect("copy_db should succeed without wal");
        assert!(db_copy.path.exists());

        let copied_wal = db_copy.path.parent().unwrap().join("cookies.sqlite-wal");
        assert!(
            !copied_wal.exists(),
            "no -wal expected when source has none"
        );
    }

    #[test]
    fn open_ro_rejects_writes() {
        let src_dir = TempDir::new().expect("source tempdir");
        let src_path = src_dir.path().join("test.sqlite");
        make_test_db(&src_path);

        let db_copy = copy_db(&src_path).expect("copy_db");
        let conn = open_ro(&db_copy.path).expect("open_ro");

        let result = conn.execute("INSERT INTO t VALUES ('evil')", params![]);
        assert!(result.is_err(), "write through an RO connection must fail");
    }
}
