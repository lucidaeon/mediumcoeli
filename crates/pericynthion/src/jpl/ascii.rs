//! Reader for JPL DE-series **ASCII** ephemeris chunks (`ascp*.NNN` / `ascm*.NNN`).
//!
//! The ASCII distribution carries the same Chebyshev coefficient records as the
//! binary file, as text: each record is a header line `<recnum>  <ncoeff>` then
//! `ncoeff` doubles (3 per line) in Fortran `D`-exponent notation, the first two
//! being the granule start/end JD (TT). We parse on demand, one chunk at a time,
//! and serve records through the same interface the binary reader uses.

use std::cell::RefCell;
use std::path::{Path, PathBuf};

use crate::error::PericynthionError;
use crate::jpl::header::Header;
use crate::jpl::reader::{OwnedRecord, RecordSource};

/// Convert a Fortran `D`-exponent token (`0.44D+03`) to `f64`.
fn parse_d_double(tok: &str) -> Option<f64> {
    let swapped: String = tok
        .chars()
        .map(|c| if c == 'D' || c == 'd' { 'e' } else { c })
        .collect();
    swapped.trim().parse().ok()
}

/// One parsed ASCII coefficient record.
pub(crate) struct AsciiRecord {
    /// 1-based record index as written in the chunk. Retained from the
    /// parsed header line for round-trip/diagnostic checks (asserted in
    /// tests); record selection uses JD arithmetic, not this index.
    #[allow(dead_code)]
    pub index: u32,
    /// `ncoeff` doubles; `[0]`/`[1]` are start/end JD.
    pub coeffs: Vec<f64>,
}

/// Parse the next record block from `lines`. Returns `None` at clean EOF.
fn parse_record<'a>(
    lines: &mut impl Iterator<Item = &'a str>,
    ncoeff: usize,
) -> Option<AsciiRecord> {
    // Skip blank lines before header.
    let header = loop {
        let l = lines.next()?;
        if !l.trim().is_empty() {
            break l;
        }
    };
    let mut hp = header.split_whitespace();
    let index: u32 = hp.next()?.parse().ok()?;
    let _declared: usize = hp.next()?.parse().ok()?;
    let mut coeffs = Vec::with_capacity(ncoeff);
    while coeffs.len() < ncoeff {
        let l = lines.next()?;
        for tok in l.split_whitespace() {
            if let Some(v) = parse_d_double(tok) {
                coeffs.push(v);
            }
        }
    }
    coeffs.truncate(ncoeff);
    Some(AsciiRecord { index, coeffs })
}

/// One ASCII chunk file and the JD span it covers.
pub(crate) struct AsciiChunk {
    /// Absolute path to the chunk file.
    pub path: PathBuf,
    /// Julian Date of the first granule's start (coeffs[0] of the first record).
    pub start_jd: f64,
    /// Julian Date of the last granule's end (coeffs[1] of the last record).
    pub end_jd: f64,
    /// Granule size in days as read from the first record (`coeffs[1] -
    /// coeffs[0]`). Retained so `index_chunks` can debug-assert grid
    /// alignment without a [`Header`].
    #[cfg_attr(not(debug_assertions), allow(dead_code))]
    pub first_granule_days: f64,
}

/// Scan `text` (the tail of a chunk file) for the last complete record and
/// return its `coeffs[1]` (end JD). Returns `None` if no record header is found.
fn last_record_end_jd(text: &str, ncoeff: usize) -> Option<f64> {
    let lines: Vec<&str> = text.lines().collect();
    let ncoeff_str = ncoeff.to_string();
    let mut end_jd: Option<f64> = None;
    for i in 0..lines.len() {
        let mut toks = lines[i].split_whitespace();
        let is_header = match (toks.next(), toks.next(), toks.next()) {
            (Some(a), Some(b), None) => a.parse::<u32>().is_ok() && b == ncoeff_str.as_str(),
            _ => false,
        };
        if is_header {
            let mut it = lines[i..].iter().copied();
            if let Some(r) = parse_record(&mut it, ncoeff) {
                end_jd = Some(r.coeffs[1]);
            }
        }
    }
    end_jd
}

/// Discover and JD-index `asc[pm]*.NNN` chunks in `dir`.
///
/// Only the first and last records of each file are read: the head gives
/// `start_jd`, the tail gives `end_jd`. The returned list is sorted by
/// `start_jd` ascending.
///
/// # Chunk-grid invariant
///
/// DE-series ASCII chunks lie on a single global 32-day granule grid: every
/// chunk's first-record `start_jd` is an exact integer number of granules from
/// every other chunk's, and adjacent chunks tile the timeline (the official
/// distribution overlaps neighbours by exactly one granule — the last record
/// of `ascpNNNNN.441` repeats the first record of the next chunk). Because of
/// this, flooring a JD against the *selected chunk's own* `start_jd`
/// (`AsciiEphemeris::record_for_jd`) picks the same granule the binary
/// reader picks by flooring against the *file's* `start_jd`, including at chunk
/// seams — proven by the `ascii_agrees_with_binary_at_chunk_seam` acceptance
/// test. A `debug_assert!` below checks the grid alignment of adjacent chunks
/// so any future dataset that violates it trips in debug builds rather than
/// silently mis-selecting a record near a seam.
///
/// # Errors
///
/// Returns [`PericynthionError::Io`] if `dir` cannot be listed or any matched
/// file cannot be read.
pub(crate) fn index_chunks(
    dir: &Path,
    ncoeff: usize,
) -> Result<Vec<AsciiChunk>, PericynthionError> {
    use std::io::{Read, Seek, SeekFrom};

    let rd = std::fs::read_dir(dir).map_err(|source| PericynthionError::Io {
        path: dir.to_path_buf(),
        source,
    })?;
    let mut chunks = Vec::new();
    for ent in rd.filter_map(Result::ok) {
        let p = ent.path();
        let Some(name) = p.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if !(name.starts_with("ascp") || name.starts_with("ascm")) {
            continue;
        }

        // Budget: Fortran writes 3 coefficients per 80-char line.
        // lines_per_rec = 1 header + ceil(ncoeff/3) data lines.
        // Double that and floor at 4 KiB for the head read.
        let lines_per_rec = 1 + ncoeff.div_ceil(3);
        let rec_budget = (lines_per_rec * 2 * 80).max(4096);

        let mut file = std::fs::File::open(&p).map_err(|source| PericynthionError::Io {
            path: p.clone(),
            source,
        })?;
        let file_len = file
            .metadata()
            .map_err(|source| PericynthionError::Io {
                path: p.clone(),
                source,
            })?
            .len();

        // --- first record: read from the head of the file ------------------
        // Cast via u64 avoids overflow on usize-is-32-bit targets; file_len
        // is already ≤ usize::MAX bytes for any file we can mmap.
        #[allow(clippy::cast_possible_truncation)]
        let head_len = (rec_budget as u64).min(file_len) as usize;
        let mut head_buf = vec![0u8; head_len];
        file.read_exact(&mut head_buf)
            .map_err(|source| PericynthionError::Io {
                path: p.clone(),
                source,
            })?;
        let head_text = String::from_utf8_lossy(&head_buf);
        let Some(first) = parse_record(&mut head_text.lines(), ncoeff) else {
            continue;
        };
        let start_jd = first.coeffs[0];
        let first_granule_days = first.coeffs[1] - first.coeffs[0];

        // --- last record: read from the tail of the file -------------------
        // 3× rec_budget so the window is wide enough to contain at least one
        // full record even if the seek lands mid-record.  We cannot assume the
        // tail starts on a record boundary, so we scan every line looking for
        // record headers (exactly two whitespace tokens: recnum + ncoeff) and
        // try to parse from each match; the last successful parse wins.
        let tail_len = ((rec_budget * 3) as u64).min(file_len);
        let tail_start = file_len.saturating_sub(tail_len);
        file.seek(SeekFrom::Start(tail_start))
            .map_err(|source| PericynthionError::Io {
                path: p.clone(),
                source,
            })?;
        let mut tail_buf = Vec::new();
        file.read_to_end(&mut tail_buf)
            .map_err(|source| PericynthionError::Io {
                path: p.clone(),
                source,
            })?;
        let tail_text = String::from_utf8_lossy(&tail_buf);
        let end_jd = last_record_end_jd(&tail_text, ncoeff).unwrap_or(first.coeffs[1]);

        chunks.push(AsciiChunk {
            path: p,
            start_jd,
            end_jd,
            first_granule_days,
        });
    }
    chunks.sort_by(|a, b| {
        a.start_jd
            .partial_cmp(&b.start_jd)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Invariant check (debug builds only): every chunk's first-record start
    // sits on the same granule grid as the first chunk's, i.e. the gap between
    // adjacent chunk starts is an exact integer number of granules. This is the
    // precondition that lets `record_for_jd` floor against a chunk's own
    // `start_jd` and still agree with the binary reader at chunk seams. The
    // granule size is taken from each chunk's own first record span.
    #[cfg(debug_assertions)]
    if let Some(first) = chunks.first() {
        let grid_origin = first.start_jd;
        for c in &chunks {
            let granule = c.first_granule_days;
            if granule > 0.0 {
                let granules = (c.start_jd - grid_origin) / granule;
                debug_assert!(
                    (granules - granules.round()).abs() < 1e-6,
                    "ASCII chunk {} start_jd {} is not granule-aligned to grid origin {} \
                     (granule {} days, {granules} granules) — record_for_jd may mis-select \
                     near a chunk seam",
                    c.path.display(),
                    c.start_jd,
                    grid_origin,
                    granule,
                );
            }
        }
    }

    Ok(chunks)
}

/// ASCII-backed coefficient source: the text-distribution counterpart of
/// [`EphemerisFile`](crate::jpl::reader::EphemerisFile).
///
/// Chunks (`ascp*.NNN` / `ascm*.NNN`) are JD-indexed once at construction
/// via `index_chunks`; their records are parsed lazily, one chunk at a
/// time, and the most-recently-touched chunk's records are cached so a
/// run of queries within the same ~20-year span re-parses nothing.
///
/// Implements [`RecordSource`], so [`crate::ephemeris::Ephemeris`] runs
/// over it identically to the binary reader.
pub struct AsciiEphemeris {
    /// `NCOEFF` — doubles per record, taken from the companion header.
    header_ncoeff: usize,
    /// Granule size in days (32 for DE-series).
    granule_days: f64,
    /// JD-indexed chunks, sorted ascending by `start_jd`.
    chunks: Vec<AsciiChunk>,
    /// Cache of the most-recently-parsed chunk: `(chunk_index, records)`.
    cache: RefCell<Option<(usize, Vec<AsciiRecord>)>>,
}

impl AsciiEphemeris {
    /// Build an ASCII source for `dir`, taking `NCOEFF` and the granule
    /// size from `header`.
    ///
    /// # Errors
    ///
    /// Returns [`PericynthionError::Io`] if `dir` cannot be listed or any
    /// chunk file cannot be read while indexing.
    pub fn open(dir: impl AsRef<Path>, header: &Header) -> Result<Self, PericynthionError> {
        Self::from_params(dir, header.ncoeff as usize, header.epoch.granule_days)
    }

    /// Build an ASCII source directly from `ncoeff` and `granule_days`,
    /// bypassing a [`Header`].
    ///
    /// Exposed as `pub(crate)` so unit tests can construct a source from
    /// synthetic chunks without first fabricating a full header; the
    /// public entry point is [`AsciiEphemeris::open`].
    ///
    /// # Errors
    ///
    /// Returns [`PericynthionError::Io`] if `dir` cannot be listed or any
    /// chunk file cannot be read while indexing.
    pub(crate) fn from_params(
        dir: impl AsRef<Path>,
        ncoeff: usize,
        granule_days: f64,
    ) -> Result<Self, PericynthionError> {
        let chunks = index_chunks(dir.as_ref(), ncoeff)?;
        Ok(Self {
            header_ncoeff: ncoeff,
            granule_days,
            chunks,
            cache: RefCell::new(None),
        })
    }

    /// Locate the chunk whose JD span contains `jd_tt`.
    ///
    /// Binary-search on `start_jd`: the chunk is the last one whose
    /// `start_jd <= jd_tt`, provided `jd_tt <= end_jd`.
    fn chunk_index_for_jd(&self, jd_tt: f64) -> Option<usize> {
        // partition_point gives the count of chunks with start_jd <= jd_tt.
        let pp = self.chunks.partition_point(|c| c.start_jd <= jd_tt);
        if pp == 0 {
            return None;
        }
        let idx = pp - 1;
        if jd_tt <= self.chunks[idx].end_jd {
            Some(idx)
        } else {
            None
        }
    }
}

impl RecordSource for AsciiEphemeris {
    fn granule_days(&self) -> f64 {
        self.granule_days
    }

    fn start_jd(&self) -> f64 {
        self.chunks.first().map_or(f64::NAN, |c| c.start_jd)
    }

    fn end_jd(&self) -> f64 {
        self.chunks.last().map_or(f64::NAN, |c| c.end_jd)
    }

    fn record_for_jd(&self, jd_tt: f64) -> Result<OwnedRecord, PericynthionError> {
        let chunk_idx = self
            .chunk_index_for_jd(jd_tt)
            .ok_or_else(|| PericynthionError::Io {
                path: PathBuf::new(),
                source: std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!(
                        "JD {jd_tt} outside ASCII coverage [{}, {}]",
                        self.start_jd(),
                        self.end_jd()
                    ),
                ),
            })?;
        let chunk = &self.chunks[chunk_idx];

        // Parse + cache the chunk's records if not already cached.
        {
            let cached = self.cache.borrow();
            let hit = matches!(&*cached, Some((idx, _)) if *idx == chunk_idx);
            if !hit {
                drop(cached);
                let text = std::fs::read_to_string(&chunk.path).map_err(|source| {
                    PericynthionError::Io {
                        path: chunk.path.clone(),
                        source,
                    }
                })?;
                let mut it = text.lines();
                let mut records = Vec::new();
                while let Some(r) = parse_record(&mut it, self.header_ncoeff) {
                    records.push(r);
                }
                *self.cache.borrow_mut() = Some((chunk_idx, records));
            }
        }

        // Within the chunk, pick the granule by floor-division on the
        // chunk's own start_jd. This agrees with the binary reader (which
        // floors against the *file's* start_jd) at every JD, including chunk
        // seams, because all chunks share one global granule grid — see the
        // chunk-grid invariant on `index_chunks` and the
        // `ascii_agrees_with_binary_at_chunk_seam` acceptance test. Clamp the
        // final-record edge so a JD exactly at end_jd resolves to the last
        // granule, mirroring the binary reader's clamp.
        let cached = self.cache.borrow();
        let (_, records) = cached.as_ref().expect("cache populated above");
        if records.is_empty() {
            return Err(PericynthionError::Io {
                path: chunk.path.clone(),
                source: std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "ASCII chunk parsed to zero records",
                ),
            });
        }
        // floor() of a non-negative in-range fraction; clamped below.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let mut granule_index = ((jd_tt - chunk.start_jd) / self.granule_days).floor() as usize;
        if granule_index >= records.len() {
            granule_index = records.len() - 1;
        }
        Ok(OwnedRecord::new(records[granule_index].coeffs.clone()))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn index_two_synthetic_chunks_sorted() {
        use std::io::Write;
        let tmp = tempdir::TempDir::new("ascii-index").unwrap();
        // ncoeff=2 so each record is just [start,end].
        let write = |name: &str, recs: &[(f64, f64)]| {
            let mut f = std::fs::File::create(tmp.path().join(name)).unwrap();
            for (i, (s, e)) in recs.iter().enumerate() {
                writeln!(f, "  {}     2", i + 1).unwrap();
                writeln!(f, "    {s:.10}D+00    {e:.10}D+00").unwrap();
            }
        };
        // Starts are an integer number of 32-day granules apart (100.0 and
        // 100.0 + 3*32 = 196.0) so the grid-alignment debug_assert in
        // index_chunks is satisfied — mirroring the real DE441 distribution.
        write("ascp01000.441", &[(196.0, 228.0), (228.0, 260.0)]);
        write("ascm01000.441", &[(100.0, 132.0), (132.0, 164.0)]);
        let chunks = super::index_chunks(tmp.path(), 2).unwrap();
        assert_eq!(chunks.len(), 2);
        assert!(chunks[0].start_jd < chunks[1].start_jd); // ascm before ascp
        assert!((chunks[0].start_jd - 100.0).abs() < 1e-9);
        assert!((chunks[1].end_jd - 260.0).abs() < 1e-9);
    }

    #[test]
    fn parse_d_notation() {
        assert!(
            (super::parse_d_double("0.1721040500000000D+07").unwrap() - 1_721_040.5).abs() < 1e-6
        );
        assert!(
            (super::parse_d_double("-0.3115808711534555D+08").unwrap() + 31_158_087.115_345_55)
                .abs()
                < 1e-1
        );
        assert!(
            (super::parse_d_double("0.2938133100000835D-04").unwrap() - 2.938_133_100_000_835e-5)
                .abs()
                < 1e-18
        );
        assert!(super::parse_d_double("notanumber").is_none());
    }

    #[test]
    fn ascii_ephemeris_serves_record_for_jd() {
        use crate::jpl::reader::RecordSource;
        use std::io::Write;

        // Synthetic single-chunk ASCII dir, ncoeff=8, two 32-day granules.
        // Granule 0: [1000.0, 1032.0]; granule 1: [1032.0, 1064.0].
        // Coefficient index 2 carries a recognizable marker per granule.
        let tmp = tempdir::TempDir::new("ascii-eph").unwrap();
        let ncoeff = 8usize;
        let granule_days = 32.0_f64;
        let granules = [(1000.0_f64, 1032.0_f64, 11.0_f64), (1032.0, 1064.0, 22.0)];
        {
            let mut f = std::fs::File::create(tmp.path().join("ascp02000.441")).unwrap();
            for (i, (s, e, marker)) in granules.iter().enumerate() {
                writeln!(f, "  {}     {ncoeff}", i + 1).unwrap();
                // coeffs[0]=start, coeffs[1]=end, coeffs[2]=marker, rest=0.
                let coeffs = [*s, *e, *marker, 0.0, 0.0, 0.0, 0.0, 0.0];
                for c in &coeffs {
                    writeln!(f, "    {c:.10}D+00").unwrap();
                }
            }
        }

        let eph = super::AsciiEphemeris::from_params(tmp.path(), ncoeff, granule_days).unwrap();

        // Sanity on the source-level span.
        assert!((eph.start_jd() - 1000.0).abs() < 1e-9);
        assert!((eph.end_jd() - 1064.0).abs() < 1e-9);
        assert!((eph.granule_days() - 32.0).abs() < 1e-9);

        // Midpoint of granule 1 -> JD 1048.0.
        let query = 1048.0_f64;
        let rec = eph.record_for_jd(query).unwrap();
        assert!(rec.start_jd() <= query, "start_jd must bracket query below");
        assert!(rec.end_jd() >= query, "end_jd must bracket query above");
        assert!((rec.start_jd() - 1032.0).abs() < 1e-9);
        assert!((rec.end_jd() - 1064.0).abs() < 1e-9);
        // Marker placed in granule 1.
        assert!((rec.get(2) - 22.0).abs() < 1e-9);
        assert_eq!(rec.len(), ncoeff);
        let band = rec.slice(2, 1);
        assert_eq!(band.len(), 1);
        assert!((band[0] - 22.0).abs() < 1e-9);

        // Cross-check granule 0.
        let rec0 = eph.record_for_jd(1016.0).unwrap();
        assert!((rec0.get(2) - 11.0).abs() < 1e-9);
    }

    #[test]
    fn parse_one_record_of_ncoeff_4() {
        let block = "     1     4\n    0.1721040500000000D+07    0.1721072500000000D+07   -0.3000000000000000D+01\n    0.5000000000000000D+00\n";
        let mut it = block.lines();
        let rec = super::parse_record(&mut it, 4).unwrap();
        assert_eq!(rec.index, 1);
        assert_eq!(rec.coeffs.len(), 4);
        assert!((rec.coeffs[0] - 1_721_040.5).abs() < 1e-6);
        assert!((rec.coeffs[3] - 0.5).abs() < 1e-12);
    }
}
