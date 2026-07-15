//! Exit-code taxonomy contract: each classified failure exits with its
//! `ExitClass` code (see `crates/blackmoon/src/exit.rs`), not a blanket 1.
//! Uses synthetic fixtures only — not specimens or reference charts — so
//! these tests never skip.

use std::process::{Command, Stdio};

const BLACKMOON: &str = env!("CARGO_BIN_EXE_blackmoon");

// A minimal valid .jhd chart (7-line head): 1990-03-21 06:00, 75E 15N, +5:30.
const JHD: &[u8] = b"3\r\n21\r\n1990\r\n6.0\r\n-5.30\r\n-75.0\r\n15.0\r\n";

/// A well-formed Zeus `.zdb` record with `notes` populated — JZOD's
/// WRITE_CAPS does not preserve `ChartField::Notes`, so writing this to
/// `--to json` under `--strict` triggers the lossy-refusal path.
const ZEUS_WITH_NOTES: &[u8] =
    b"Jane Doe;0;21.03.1990;06:00:00;+00:00:00;City;N15.00.00;E075.00.00;-;;;Some notes;;;;\n";

/// A malformed Zeus `.zdb` record: fewer than the 16 required
/// semicolon-separated fields, so `zeus::parse_file` returns
/// `ParseError::InvalidRecord`, which `read_bytes` wraps as
/// `ChartError::Parse`.
const ZEUS_MALFORMED: &[u8] = b"Jane Doe;not enough fields\n";

#[test]
fn malformed_chart_input_exits_chart_parse_code_6() {
    let dir = tempdir::TempDir::new("bm_exit_chartparse").unwrap();
    let input = dir.path().join("bad.zdb");
    std::fs::write(&input, ZEUS_MALFORMED).unwrap();
    let out = dir.path().join("out.SFcht");

    let output = Command::new(BLACKMOON)
        .arg(&input)
        .args(["--to", "sfcht", "--output"])
        .arg(&out)
        .output()
        .expect("run blackmoon");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(6),
        "expected exit code 6 (ChartParse), got {:?}; stderr:\n{stderr}",
        output.status.code()
    );
}

#[test]
fn strict_lossy_write_refusal_exits_lossy_refused_code_9() {
    let dir = tempdir::TempDir::new("bm_exit_lossy").unwrap();
    let input = dir.path().join("notes.zdb");
    std::fs::write(&input, ZEUS_WITH_NOTES).unwrap();
    let out = dir.path().join("out.json");

    let output = Command::new(BLACKMOON)
        .arg(&input)
        .args(["--to", "json", "--output"])
        .arg(&out)
        .arg("--strict")
        .output()
        .expect("run blackmoon");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(9),
        "expected exit code 9 (LossyRefused), got {:?}; stderr:\n{stderr}",
        output.status.code()
    );
    assert!(
        stderr.contains("--strict"),
        "expected the strict-refusal message on stderr, got:\n{stderr}"
    );
}

#[test]
fn machine_mode_missing_fill_exits_need_input_code_10() {
    let dir = tempdir::TempDir::new("bm_exit_needinput").unwrap();
    let input = dir.path().join("Person.jhd");
    std::fs::write(&input, JHD).unwrap();

    let output = Command::new(BLACKMOON)
        .arg(&input)
        .args(["--to", "json", "--output", "-"])
        .stdin(Stdio::null())
        .output()
        .expect("run blackmoon");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(10),
        "expected exit code 10 (NeedInput), got {:?}; stderr:\n{stderr}",
        output.status.code()
    );
}

/// A supplied but unrecognised `--fill-*` value (when the fill is actually
/// needed) exits Input (code 3) — a bad *value*, distinct from a structural
/// clap usage error (code 2) — and the message lists the accepted values.
#[test]
fn invalid_fill_value_exits_input_code_3() {
    let dir = tempdir::TempDir::new("bm_exit_input").unwrap();
    let input = dir.path().join("Person.jhd");
    std::fs::write(&input, JHD).unwrap();
    let out = dir.path().join("out.SFcht");

    let output = Command::new(BLACKMOON)
        .arg(&input)
        .args(["--to", "sfcht", "--output"])
        .arg(&out)
        // A bogus house value; zodiac/locus are valid so the run reaches the
        // house fill and fails on the bad value rather than a missing one.
        .args([
            "--fill-house",
            "xyzzy",
            "--fill-zodiac",
            "tropical",
            "--fill-locus",
            "geocentric",
        ])
        .stdin(Stdio::null())
        .output()
        .expect("run blackmoon");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(3),
        "expected exit code 3 (Input), got {:?}; stderr:\n{stderr}",
        output.status.code()
    );
    assert!(
        stderr.contains("xyzzy") && stderr.contains("--fill-house"),
        "expected the bad value + flag named on stderr, got:\n{stderr}"
    );
    assert!(
        stderr.contains("placidus"),
        "expected the accepted values listed on stderr, got:\n{stderr}"
    );
}

#[test]
fn no_input_files_exits_not_found_code_4() {
    let dir = tempdir::TempDir::new("bm_exit_notfound").unwrap();
    let out = dir.path().join("out.SFcht");

    // `--from`/`--target` absent and no positional inputs: the "at least one
    // input file is required" NoInputError path.
    let output = Command::new(BLACKMOON)
        .args(["--to", "sfcht", "--output"])
        .arg(&out)
        .output()
        .expect("run blackmoon");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(4),
        "expected exit code 4 (NotFound), got {:?}; stderr:\n{stderr}",
        output.status.code()
    );
}

#[test]
fn empty_directory_source_exits_not_found_code_4() {
    let dir = tempdir::TempDir::new("bm_exit_notfound_dir").unwrap();
    std::fs::write(dir.path().join("notes.txt"), b"no charts here").unwrap();
    let out = dir.path().join("out.SFcht");

    let output = Command::new(BLACKMOON)
        .arg(dir.path())
        .args(["--to", "sfcht", "--output"])
        .arg(&out)
        .output()
        .expect("run blackmoon");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(4),
        "expected exit code 4 (NotFound), got {:?}; stderr:\n{stderr}",
        output.status.code()
    );
}

#[test]
fn missing_output_target_exits_usage_code_2() {
    let dir = tempdir::TempDir::new("bm_exit_usage").unwrap();
    let input = dir.path().join("Person.jhd");
    std::fs::write(&input, JHD).unwrap();

    // A file input with no --output/--to/--target the writer can infer from:
    // Format::from_path succeeds off the .jhd extension for reading, but
    // there is no way to infer a *write* target, so out_target resolution
    // falls all the way to the "--output is required" UsageError.
    let output = Command::new(BLACKMOON)
        .arg(&input)
        .output()
        .expect("run blackmoon");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(2),
        "expected exit code 2 (Usage), got {:?}; stderr:\n{stderr}",
        output.status.code()
    );
}

#[test]
fn successful_conversion_exits_zero() {
    let dir = tempdir::TempDir::new("bm_exit_success").unwrap();
    let input = dir.path().join("Person.jhd");
    std::fs::write(&input, JHD).unwrap();
    let out = dir.path().join("out.SFcht");

    let output = Command::new(BLACKMOON)
        .arg(&input)
        .args(["--to", "sfcht", "--output"])
        .arg(&out)
        .args([
            "--fill-house",
            "whole-sign",
            "--fill-zodiac",
            "lahiri",
            "--fill-locus",
            "geocentric",
        ])
        .output()
        .expect("run blackmoon");

    assert_eq!(output.status.code(), Some(0));
}
