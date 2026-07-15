//! Exit-code taxonomy contract: each classified failure exits with its
//! `ExitClass` code (see `crates/starcat/src/exit.rs`), not a blanket 1.
//! Uses synthetic fixtures only — never specimens or reference charts — so
//! these tests never skip.
//!
//! `STARCAT_JPL_DATA`/`STARCAT_HORIZONS_DATA` are always explicitly cleared
//! (`.env_remove`) and `HOME`/`XDG_DATA_HOME` redirected to an empty tempdir
//! so the platform default data dir (which may hold real ephemeris data on
//! the machine running these tests) never leaks in and makes a "no data"
//! assertion flaky.

use std::process::Command;

mod common;

/// A `Command` for `starcat` with no ambient data-location environment:
/// `STARCAT_JPL_DATA`/`STARCAT_HORIZONS_DATA` cleared (compute-input
/// `STARCAT_*` vars are already cleared by [`common::starcat_command`]), and
/// `HOME`/`XDG_DATA_HOME` redirected to `empty_home` so
/// `pericynthion::default_data_dir` resolves under it instead of the real
/// platform data dir.
fn bare_cmd(empty_home: &std::path::Path) -> Command {
    let mut cmd = common::starcat_command();
    cmd.env_remove("STARCAT_JPL_DATA")
        .env_remove("STARCAT_HORIZONS_DATA")
        .env("HOME", empty_home)
        .env("XDG_DATA_HOME", empty_home.join(".local/share"));
    cmd
}

/// `starcat compute` with no `--jpl-data`/`$STARCAT_JPL_DATA` and no data in
/// the (redirected, empty) platform data dir: `resolve_jpl_dir` returns the
/// typed `NotFoundError`, which `classify` maps to code 4.
#[test]
fn compute_with_no_ephemeris_data_exits_not_found_code_4() {
    let home = tempdir::TempDir::new("sc_exit_notfound").unwrap();

    let output = bare_cmd(home.path())
        .args([
            "compute",
            "--date",
            "1990-03-21",
            "--time",
            "06:00:00",
            "--tz=+00:00",
        ])
        .output()
        .expect("run starcat");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(4),
        "expected exit code 4 (NotFound), got {:?}; stderr:\n{stderr}",
        output.status.code()
    );
    assert!(
        stderr.contains("no ephemeris data found"),
        "expected the no-ephemeris-data message on stderr, got:\n{stderr}"
    );
}

/// A bad `--ayanamsha` slug: `resolve_zodiac` returns
/// `PericynthionError::UnknownAyanamshaSlug`, which `classify` maps to code 3
/// (Input). `resolve_zodiac` runs after chart computation succeeds, not before
/// the SPK ephemeris is opened, so a JPL dir with real ephemeris bytes must be
/// supplied — a synthetic empty-but-present dir isn't enough. The
/// `--ayanamsha` flag is only meaningful under `--zodiac sidereal`, validated
/// during `cmd_compute` after computation succeeds — so this test requires
/// `STARCAT_JPL_DATA` and skips cleanly without it, same convention as
/// `tests/cli_compute.rs`.
#[test]
fn bad_ayanamsha_slug_exits_input_code_3() {
    let Ok(jpl) = std::env::var("STARCAT_JPL_DATA") else {
        eprintln!("STARCAT_JPL_DATA not set — skipping integration test");
        return;
    };

    let output = common::starcat_command()
        .args([
            "compute",
            "--date",
            "1990-03-21",
            "--time",
            "06:00:00",
            "--tz=+00:00",
            "--zodiac",
            "sidereal",
            "--ayanamsha",
            "bogus-slug",
            "--jpl-data",
            &jpl,
        ])
        .output()
        .expect("run starcat");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(3),
        "expected exit code 3 (Input), got {:?}; stderr:\n{stderr}",
        output.status.code()
    );
    assert!(
        stderr.contains("bogus-slug"),
        "expected the error to name the bad slug, got:\n{stderr}"
    );
}

/// `data verify` against a mirror root whose `header.441` is present but has
/// the wrong size (a corrupt/truncated download): `verify_required_subset`
/// returns the typed `IntegrityError`, which `classify` maps to code 5.
/// `header.441` is one of only three `production_entries()` members, chosen
/// because it's small enough to fabricate a wrong-content fixture for
/// in-memory (22802 bytes declared; this test writes a short stub, which
/// fails on size before any hashing).
#[test]
fn data_verify_corrupt_file_exits_integrity_code_5() {
    let dir = tempdir::TempDir::new("sc_exit_integrity").unwrap();
    let header_path = dir
        .path()
        .join("ssd.jpl.nasa.gov/ftp/eph/planets/Linux/de441/header.441");
    std::fs::create_dir_all(header_path.parent().unwrap()).unwrap();
    // Declared size is 22802 bytes; this stub is deliberately short, so
    // `verify_entry` reports `SizeMismatch` without ever hashing.
    std::fs::write(&header_path, b"not the real header").unwrap();

    let output = common::starcat_command()
        .args(["data", "verify", "--root"])
        .arg(dir.path())
        .output()
        .expect("run starcat");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(5),
        "expected exit code 5 (Integrity), got {:?}; stderr:\n{stderr}",
        output.status.code()
    );
}

/// A bare `starcat data fetch` (no dataset, no `bsc5`, no `--list`/`--what`):
/// `guide_bare_fetch` prints guidance to stderr then returns the typed
/// `UsageError`, which `classify` maps to code 2. Makes zero network calls.
#[test]
fn bare_data_fetch_exits_usage_code_2() {
    let output = common::starcat_command()
        .args(["data", "fetch"])
        .output()
        .expect("run starcat");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(2),
        "expected exit code 2 (Usage), got {:?}; stderr:\n{stderr}",
        output.status.code()
    );
    assert!(
        stdout.is_empty(),
        "expected empty stdout for bare data fetch guidance, got:\n{stdout}"
    );
    assert!(
        stderr.contains("specify a dataset"),
        "expected the guidance hint on stderr, got:\n{stderr}"
    );
}

/// `starcat catalogue --all` (a real, always-available success path) exits
/// zero — the taxonomy's `Ok` arm.
#[test]
fn successful_catalogue_exits_zero() {
    let output = common::starcat_command()
        .args(["catalogue", "--all"])
        .output()
        .expect("run starcat");

    assert_eq!(output.status.code(), Some(0));
}
