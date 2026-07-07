use std::process::Command;

const BLACKMOON: &str = env!("CARGO_BIN_EXE_blackmoon");

// A minimal valid .jhd chart (7-line head): 1990-03-21 06:00, 75E 15N, +5:30.
const JHD: &[u8] = b"3\r\n21\r\n1990\r\n6.0\r\n-5.30\r\n-75.0\r\n15.0\r\n";

#[test]
fn directory_source_reads_all_jhd_files() {
    let dir = tempdir::TempDir::new("bm_dir").unwrap();
    let root = dir.path();
    std::fs::write(root.join("Person One.jhd"), JHD).unwrap();
    std::fs::write(root.join("Person Two.jhd"), JHD).unwrap();
    let sub = root.join("more");
    std::fs::create_dir(&sub).unwrap();
    std::fs::write(sub.join("Person Three.jhd"), JHD).unwrap();
    std::fs::write(root.join("readme.txt"), b"ignore me").unwrap(); // junk

    let out = root.join("collection.SFcht");
    let output = Command::new(BLACKMOON)
        .arg(root)
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

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "blackmoon failed:\n{stderr}");
    assert!(out.exists(), "output not written");
    // One summary line naming the count of files read and junk skipped. Matches
    // the exact wording emitted by the pre-pass in Step 3.
    assert!(
        stderr.contains("chart files under") && stderr.contains("skipped 1 non-chart file"),
        "expected a directory summary, got:\n{stderr}"
    );
}

#[test]
fn empty_directory_source_errors_clearly() {
    let dir = tempdir::TempDir::new("bm_empty").unwrap();
    std::fs::write(dir.path().join("notes.txt"), b"no charts here").unwrap();
    let out = dir.path().join("out.SFcht");
    let output = Command::new(BLACKMOON)
        .arg(dir.path())
        .args(["--to", "sfcht", "--output"])
        .arg(&out)
        .output()
        .expect("run blackmoon");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("no chart files found"));
}
