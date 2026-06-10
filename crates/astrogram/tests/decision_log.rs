//! Tests for the `decision_log`: durable JSONL of consolidation keystrokes.

use astrogram::decision_log::{Choice, DecisionLog, DecisionRecord};
use std::io::Write as _;

#[test]
fn append_then_read_round_trips_one_record() {
    let tmp = tempfile_path("dlog_round_trip");
    let mut log = DecisionLog::open(&tmp).unwrap();
    let rec = DecisionRecord {
        group_id: "grp-001".into(),
        phenom_id: "uuid-1".into(),
        choice: Choice::Keep,
        chart_name: "A".into(),
    };
    log.append(&rec).unwrap();
    drop(log);

    let read = DecisionLog::read_all(&tmp).unwrap();
    assert_eq!(read.len(), 1);
    assert_eq!(read[0].phenom_id, "uuid-1");
    assert!(matches!(read[0].choice, Choice::Keep));
    cleanup(&tmp);
}

#[test]
fn append_survives_reopen() {
    let tmp = tempfile_path("dlog_reopen");
    {
        let mut log = DecisionLog::open(&tmp).unwrap();
        log.append(&DecisionRecord {
            group_id: "g".into(),
            phenom_id: "u1".into(),
            choice: Choice::Keep,
            chart_name: "A".into(),
        })
        .unwrap();
    }
    {
        let mut log = DecisionLog::open(&tmp).unwrap();
        log.append(&DecisionRecord {
            group_id: "g".into(),
            phenom_id: "u2".into(),
            choice: Choice::Drop,
            chart_name: "B".into(),
        })
        .unwrap();
    }
    let read = DecisionLog::read_all(&tmp).unwrap();
    assert_eq!(read.len(), 2);
    assert_eq!(read[0].phenom_id, "u1");
    assert_eq!(read[1].phenom_id, "u2");
    cleanup(&tmp);
}

#[test]
fn read_all_skips_corrupt_trailing_partial_line() {
    let tmp = tempfile_path("dlog_corrupt");
    let good = DecisionRecord {
        group_id: "g".into(),
        phenom_id: "u1".into(),
        choice: Choice::Keep,
        chart_name: "A".into(),
    };
    {
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&tmp)
            .unwrap();
        let line = serde_json::to_string(&good).unwrap();
        writeln!(f, "{line}").unwrap();
        write!(f, r#"{{"group_id":"g","phenom_id":"u2","choice":"drop","#).unwrap();
    }
    let read = DecisionLog::read_all(&tmp).unwrap();
    assert_eq!(read.len(), 1);
    assert_eq!(read[0].phenom_id, "u1");
    cleanup(&tmp);
}

#[test]
fn choice_serializes_as_lowercase() {
    assert_eq!(serde_json::to_string(&Choice::Keep).unwrap(), "\"keep\"");
    assert_eq!(serde_json::to_string(&Choice::Drop).unwrap(), "\"drop\"");
    assert_eq!(serde_json::to_string(&Choice::Skip).unwrap(), "\"skip\"");
}

fn tempfile_path(tag: &str) -> std::path::PathBuf {
    let pid = std::process::id();
    let ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("astrogram-{tag}-{pid}-{ns}.jsonl"))
}

fn cleanup(p: &std::path::Path) {
    let _ = std::fs::remove_file(p);
}
