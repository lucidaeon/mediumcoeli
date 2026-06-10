//! Chart consolidation: merge multiple collections, surfacing duplicate
//! *candidates* for user decision.
//!
//! ## Deduplication
//!
//! Spacetime is the primary signal — nothing is ever silently dropped on the
//! basis of a name alone.  Two records are flagged as duplicate **candidates**
//! when their spacetime coordinates fall within tolerance:
//!
//! - `year`, `month`, `day` — equal
//! - hour-of-day — within ±2 hours of each other
//! - `latitude` and `longitude` — each within 0.1° of each other (when both
//!   sides have coordinates; missing-coord records fall back to date+time only
//!   and are always surfaced for review)
//!
//! Candidates are presented to the user for an approved consolidation
//! decision; the user chooses which record to keep (or to keep both).  Names
//! are a *cue* used to group and label candidates — exact equality (after
//! Unicode NFC + whitespace + zero-width + smart-quote normalization),
//! prefix match (handles LUNA listing's `…` truncation), and full containment
//! all qualify as labelling cues.  An opt-in `--fuzzy` threshold may add
//! Levenshtein-style cues on top.
//!
//! Read-side dedup applies the same candidate rule as it ingests, so the
//! merge step is not the last line of defence.
//!
//! ## Current implementation (transitional)
//!
//! The function below still performs auto-drop with first-seen-wins on a
//! strict all-AND match (exact name equality + the spacetime tolerances
//! above).  The interactive candidate flow, the relaxed name cues, and the
//! sidecar decision log described above are pending implementation.

use crate::chart::Chart;

/// Merge multiple chart collections, removing duplicates.
///
/// Input order is preserved. Earlier batches take priority over later ones;
/// within a batch, lower index wins.
#[must_use]
pub fn merge(inputs: &[Vec<Chart>]) -> Vec<Chart> {
    merge_reporting(inputs).0
}

/// Like [`merge`], but also returns the names of every chart that was dropped
/// as a duplicate, in the order they were encountered.
#[must_use]
pub fn merge_reporting(inputs: &[Vec<Chart>]) -> (Vec<Chart>, Vec<String>) {
    let mut result: Vec<Chart> = Vec::new();
    let mut skipped: Vec<String> = Vec::new();
    for batch in inputs {
        for chart in batch {
            if is_duplicate(chart, &result) {
                skipped.push(chart.name.clone());
            } else {
                result.push(chart.clone());
            }
        }
    }
    (result, skipped)
}

fn is_duplicate(candidate: &Chart, existing: &[Chart]) -> bool {
    existing.iter().any(|c| charts_match(candidate, c))
}

/// Returns `true` when `a` and `b` are duplicate **candidates** by the
/// spacetime rule documented at the module level — date equal, time within
/// ±2h, latitude and longitude each within 0.1°.
///
/// Name is intentionally not consulted: candidates are surfaced to the user
/// for an approved consolidation decision, so name differences are
/// informational, not a gate.
#[must_use]
pub fn is_candidate(a: &Chart, b: &Chart) -> bool {
    a.year == b.year
        && a.month == b.month
        && a.day == b.day
        && time_diff_seconds(a, b) <= 7200
        && (a.latitude.degrees() - b.latitude.degrees()).abs() <= 0.1
        && (a.longitude.degrees() - b.longitude.degrees()).abs() <= 0.1
}

/// Position of the first chart in `existing` that is a duplicate candidate
/// of `chart`, or `None` if there is no match.
///
/// Used by streaming ingest paths (e.g. the LUNA fetch loop) to flag
/// candidates as they arrive without altering the input.
#[must_use]
pub fn find_candidate(chart: &Chart, existing: &[Chart]) -> Option<usize> {
    existing.iter().position(|c| is_candidate(chart, c))
}

/// Path-compressed union-find root lookup.
fn uf_find(parent: &mut [usize], mut i: usize) -> usize {
    while parent[i] != i {
        parent[i] = parent[parent[i]]; // path compression
        i = parent[i];
    }
    i
}

/// Cluster `charts` into transitive groups of duplicate candidates.
///
/// Two charts share a group iff there is a sequence connecting them where
/// every consecutive pair satisfies [`is_candidate`].  Returns a list of
/// groups, each group a vector of indices into `charts`.  Within each group
/// indices are in ascending order; groups themselves are ordered by the
/// smallest index they contain.  Every chart appears in exactly one group.
///
/// Used by the interactive consolidation flow to present one decision unit
/// per cluster.
#[must_use]
pub fn group_candidates(charts: &[Chart]) -> Vec<Vec<usize>> {
    let n = charts.len();
    if n == 0 {
        return Vec::new();
    }
    // Union-find by smallest-index representative.
    let mut parent: Vec<usize> = (0..n).collect();
    for i in 0..n {
        for j in (i + 1)..n {
            if is_candidate(&charts[i], &charts[j]) {
                let ri = uf_find(&mut parent, i);
                let rj = uf_find(&mut parent, j);
                if ri != rj {
                    let (lo, hi) = if ri < rj { (ri, rj) } else { (rj, ri) };
                    parent[hi] = lo;
                }
            }
        }
    }
    // Bucket indices by representative, preserving ascending order.
    let mut buckets: std::collections::BTreeMap<usize, Vec<usize>> =
        std::collections::BTreeMap::new();
    for i in 0..n {
        let r = uf_find(&mut parent, i);
        buckets.entry(r).or_default().push(i);
    }
    buckets.into_values().collect()
}

fn charts_match(a: &Chart, b: &Chart) -> bool {
    a.name == b.name
        && a.year == b.year
        && a.month == b.month
        && a.day == b.day
        && time_diff_seconds(a, b) <= 7200
        && (a.latitude.degrees() - b.latitude.degrees()).abs() <= 0.1
        && (a.longitude.degrees() - b.longitude.degrees()).abs() <= 0.1
}

fn time_diff_seconds(a: &Chart, b: &Chart) -> u32 {
    let a_sec = u32::from(a.hour) * 3600 + u32::from(a.minute) * 60 + u32::from(a.second);
    let b_sec = u32::from(b.hour) * 3600 + u32::from(b.minute) * 60 + u32::from(b.second);
    a_sec.abs_diff(b_sec)
}
