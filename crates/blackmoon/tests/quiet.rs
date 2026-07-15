//! `--quiet` gating contract: env narration (progress, UA, "authenticated
//! via…") and the per-file "N charts" counts are dropped, but data-affecting
//! disclosures (dropped fields) and the summary result (in/dupes/out,
//! "wrote") always survive. Uses synthetic fixtures (`.jhd`/`.SFcht`), not
//! specimens or reference charts, so these tests never skip.

use std::process::Command;

const BLACKMOON: &str = env!("CARGO_BIN_EXE_blackmoon");

// A minimal valid .jhd chart (7-line head): 1990-03-21 06:00, 75E 15N, +5:30.
const JHD: &[u8] = b"3\r\n21\r\n1990\r\n6.0\r\n-5.30\r\n-75.0\r\n15.0\r\n";

/// `--quiet` on a file conversion that also drops data (SFcht -> JSON: JSON
/// cannot store house system): stderr keeps the drop disclosure and the
/// "wrote" result line. Neither is env narration, so `--quiet` must not
/// remove them. (A file-target conversion never calls the web
/// `resolve_provider` path, so there is no UA/"authenticated via" narration
/// to observe here either way — those are covered by the gating logic itself
/// in `providers::quiet_aware_sink_tests` and the `resolve_provider` code
/// path, which is network-dependent and not exercised by this offline test.)
#[test]
fn quiet_file_conversion_keeps_drops_and_wrote() {
    let dir = tempdir::TempDir::new("bm_quiet_file").unwrap();
    let jhd = dir.path().join("Person.jhd");
    std::fs::write(&jhd, JHD).unwrap();
    let sfcht = dir.path().join("Person.SFcht");

    // First produce a full-fidelity SFcht (carries house system) so the
    // second conversion (SFcht -> JSON) has something for JSON to drop.
    let prep = Command::new(BLACKMOON)
        .arg(&jhd)
        .args(["--to", "sfcht", "--output"])
        .arg(&sfcht)
        .args([
            "--fill-house",
            "whole-sign",
            "--fill-zodiac",
            "lahiri",
            "--fill-locus",
            "geocentric",
        ])
        .output()
        .expect("run blackmoon (prep)");
    assert!(
        prep.status.success(),
        "prep conversion failed:\n{}",
        String::from_utf8_lossy(&prep.stderr)
    );

    let out = dir.path().join("out.json");
    let output = Command::new(BLACKMOON)
        .arg(&sfcht)
        .args(["--to", "json", "--output"])
        .arg(&out)
        .arg("--quiet")
        .output()
        .expect("run blackmoon --quiet");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "blackmoon failed:\n{stderr}");
    assert!(
        stderr.contains("does not store") && stderr.contains("house system"),
        "expected the drop disclosure to survive --quiet, got:\n{stderr}"
    );
    assert!(
        stderr.contains("wrote") && stderr.contains("out.json"),
        "expected the 'wrote <path>' result to survive --quiet, got:\n{stderr}"
    );
    assert!(
        !stderr.contains("charts"),
        "expected per-file 'N charts' counts to be suppressed under --quiet, got:\n{stderr}"
    );
}

/// Same conversion without `--quiet`: the drop disclosure, "wrote" line, the
/// per-file "N charts" counts, and the in/dupes/out summary are all present.
/// Under `--quiet` the per-file counts are suppressed while the summary and
/// disclosures survive (see `quiet_file_conversion_keeps_drops_and_wrote`).
#[test]
fn normal_mode_file_conversion_unchanged() {
    let dir = tempdir::TempDir::new("bm_quiet_normal").unwrap();
    let jhd = dir.path().join("Person.jhd");
    std::fs::write(&jhd, JHD).unwrap();
    let sfcht = dir.path().join("Person.SFcht");

    let prep = Command::new(BLACKMOON)
        .arg(&jhd)
        .args(["--to", "sfcht", "--output"])
        .arg(&sfcht)
        .args([
            "--fill-house",
            "whole-sign",
            "--fill-zodiac",
            "lahiri",
            "--fill-locus",
            "geocentric",
        ])
        .output()
        .expect("run blackmoon (prep)");
    assert!(
        prep.status.success(),
        "prep conversion failed:\n{}",
        String::from_utf8_lossy(&prep.stderr)
    );

    let out = dir.path().join("out.json");
    let output = Command::new(BLACKMOON)
        .arg(&sfcht)
        .args(["--to", "json", "--output"])
        .arg(&out)
        .output()
        .expect("run blackmoon");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "blackmoon failed:\n{stderr}");
    assert!(
        stderr.contains("does not store") && stderr.contains("house system"),
        "expected the drop disclosure, got:\n{stderr}"
    );
    assert!(
        stderr.contains("wrote") && stderr.contains("out.json"),
        "expected the 'wrote <path>' result, got:\n{stderr}"
    );
    assert!(
        stderr.contains("charts"),
        "expected the per-file 'N charts' narration without --quiet, got:\n{stderr}"
    );
}
