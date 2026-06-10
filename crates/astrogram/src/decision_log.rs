//! Durable per-keystroke log of consolidation decisions.
//!
//! Each line of the log is a JSON `DecisionRecord`.  Writes are flushed and
//! fsync'd before [`DecisionLog::append`] returns, so a crash, signal, or
//! network partition between two keystrokes never loses a prior decision.
//!
//! On re-run, [`DecisionLog::read_all`] replays the log so the user is not
//! re-asked about groups they already answered.  Any line that fails to
//! deserialize is silently skipped — this covers the "crash mid-write"
//! case for ASCII-only records (which is everything we write today: UUIDs,
//! group ids, and chart names that already round-trip through JSON's
//! `\u` escapes).  An invalid UTF-8 sequence torn at EOF would propagate
//! as an `Io` error from the underlying line reader, which is acceptable
//! in practice given the current field shapes.

use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

/// What the user decided about one chart inside one candidate group.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Choice {
    /// Preserve this record; do not delete from LUNA.
    Keep,
    /// Remove this record from LUNA in the apply phase.
    Drop,
    /// Don't decide right now; the group becomes pending again on re-run.
    Skip,
}

/// One persisted decision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecisionRecord {
    /// Opaque identifier for the candidate group (typically the first
    /// `phenom_id` in the group, stable across reruns).
    pub group_id: String,
    /// The LUNA phenomenon UUID this decision applies to.
    pub phenom_id: String,
    /// What the user chose.
    pub choice: Choice,
    /// Display name at decision time (informational; never used as a key).
    pub chart_name: String,
}

/// I/O errors from the decision log.
#[derive(Debug, thiserror::Error)]
pub enum DecisionLogError {
    /// File I/O failed.
    #[error("decision log I/O: {0}")]
    Io(#[from] std::io::Error),
    /// JSON serialization of a record failed.
    #[error("decision log JSON: {0}")]
    Json(#[from] serde_json::Error),
}

/// Append-only handle to a decision log file.
pub struct DecisionLog {
    file: File,
    path: PathBuf,
}

impl DecisionLog {
    /// Open (or create) the log file for appending.  Parent directories are
    /// created automatically.
    ///
    /// # Errors
    /// - [`DecisionLogError::Io`] if the directory cannot be created or the
    ///   file cannot be opened for appending.
    pub fn open(path: &Path) -> Result<Self, DecisionLogError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        Ok(Self {
            file,
            path: path.to_path_buf(),
        })
    }

    /// Append one record, fsync, return.
    ///
    /// # Errors
    /// - [`DecisionLogError::Json`] if the record fails to serialize.
    /// - [`DecisionLogError::Io`] if writing or flushing fails.
    pub fn append(&mut self, rec: &DecisionRecord) -> Result<(), DecisionLogError> {
        let line = serde_json::to_string(rec)?;
        writeln!(self.file, "{line}")?;
        self.file.sync_data()?;
        Ok(())
    }

    /// Path of the underlying log file.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Read every well-formed record from `path`, in file order.
    ///
    /// Returns an empty vec when the file does not exist.  Any line that
    /// fails JSON deserialization is silently skipped — including a
    /// partially-written trailing line from a crash mid-keystroke and any
    /// schema-broken middle record from a hand-edit.  Skipped records are
    /// re-prompted on the next run rather than producing an error.
    ///
    /// # Errors
    /// - [`DecisionLogError::Io`] if the file exists but cannot be read, or
    ///   if the trailing partial line contains an invalid UTF-8 sequence
    ///   (the line reader propagates this as `Io::InvalidData`).
    pub fn read_all(path: &Path) -> Result<Vec<DecisionRecord>, DecisionLogError> {
        if !path.exists() {
            return Ok(Vec::new());
        }
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let mut out = Vec::new();
        for line in reader.lines() {
            let line = line?;
            if line.is_empty() {
                continue;
            }
            if let Ok(rec) = serde_json::from_str::<DecisionRecord>(&line) {
                out.push(rec);
            }
        }
        Ok(out)
    }
}
