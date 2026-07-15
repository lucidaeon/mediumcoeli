//! Stream-routing contract: human/diagnostic output goes to stderr; stdout
//! carries only the chart data payload when the sink is stdout (`-o -` /
//! `--to json`). Uses a synthetic minimal `.jhd` fixture — not a specimen or
//! reference chart, just fixed bytes — so these tests never skip.

use std::process::{Command, Stdio};

const BLACKMOON: &str = env!("CARGO_BIN_EXE_blackmoon");

// A minimal valid .jhd chart (7-line head): 1990-03-21 06:00, 75E 15N, +5:30.
const JHD: &[u8] = b"3\r\n21\r\n1990\r\n6.0\r\n-5.30\r\n-75.0\r\n15.0\r\n";

/// Converting to a FILE target: process stdout is empty; the summary/"wrote"
/// lines land on stderr instead.
#[test]
fn file_target_conversion_leaves_stdout_empty() {
    let dir = tempdir::TempDir::new("bm_streams_file").unwrap();
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

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "blackmoon failed:\n{stderr}");
    assert!(
        stdout.is_empty(),
        "expected empty stdout for a file target, got:\n{stdout}"
    );
    assert!(
        stderr.contains("wrote") && stderr.contains("out.SFcht"),
        "expected the 'wrote <path>' summary on stderr, got:\n{stderr}"
    );
    assert!(
        stderr.contains("in:") && stderr.contains("out:"),
        "expected the in/dupes/out summary on stderr, got:\n{stderr}"
    );
}

/// Machine mode (`--to json` to stdout) needing a fill, with no `--fill-*`
/// flag and stdin not a tty: the typed NeedInputError short-circuits instead
/// of prompting. No prompt text, stdout stays empty.
#[test]
fn machine_mode_missing_fill_returns_error_with_empty_stdout_and_no_prompt() {
    let dir = tempdir::TempDir::new("bm_streams_machine_missing_fill").unwrap();
    let input = dir.path().join("Person.jhd");
    std::fs::write(&input, JHD).unwrap();

    let output = Command::new(BLACKMOON)
        .arg(&input)
        .args(["--to", "json", "--output", "-"])
        .stdin(Stdio::null())
        .output()
        .expect("run blackmoon");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "expected failure for missing fill in machine mode, stdout:\n{stdout}"
    );
    assert!(
        stdout.is_empty(),
        "expected empty stdout on machine-mode failure, got:\n{stdout}"
    );
    assert!(
        !stderr.contains("Value for"),
        "expected no interactive prompt text on stderr, got:\n{stderr}"
    );
    assert!(
        stderr.contains("--fill-house") || stderr.contains("--fill-zodiac"),
        "expected the error to name a --fill-* flag, got:\n{stderr}"
    );
    // Distinguishes the typed NeedInputError (non-interactive stdin) from a
    // plain bail: only the former lists the accepted values and explains why
    // there was no prompt.
    assert!(
        stderr.contains("accepted:") && stderr.contains("stdin is not a TTY"),
        "expected the typed NeedInputError message (accepted values + non-tty framing), got:\n{stderr}"
    );
}

/// Machine mode success: `--to json` to stdout with all needed fills supplied
/// writes exactly one JSON document to stdout, nothing else.
#[test]
fn machine_mode_success_stdout_is_clean_json() {
    let dir = tempdir::TempDir::new("bm_streams_machine_ok").unwrap();
    let input = dir.path().join("Person.jhd");
    std::fs::write(&input, JHD).unwrap();

    let output = Command::new(BLACKMOON)
        .arg(&input)
        .args(["--to", "json", "--output", "-"])
        .args([
            "--fill-house",
            "whole-sign",
            "--fill-zodiac",
            "lahiri",
            "--fill-locus",
            "geocentric",
        ])
        .stdin(Stdio::null())
        .output()
        .expect("run blackmoon");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "blackmoon failed:\n{stderr}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout is not clean JSON: {e}\nstdout:\n{stdout}"));
    assert!(parsed.is_object(), "expected a JSON object document");
}
