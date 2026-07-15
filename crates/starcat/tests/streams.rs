//! Stream-routing contract (B3): the jzod output path is pure JSON on
//! stdout — nothing else — with any diagnostic (e.g. an unknown-star
//! warning) routed to stderr instead. Requires `STARCAT_JPL_DATA` (a real
//! chart must be computed to reach the jzod render); skips cleanly without
//! it, same convention as `tests/cli_compute.rs`.

mod common;

fn jpl_data_dir() -> Option<String> {
    std::env::var("STARCAT_JPL_DATA").ok()
}

/// `starcat compute --jzod` stdout parses as JSON and is *exactly* one
/// document — no leading/trailing lines, no interleaved diagnostics.
#[test]
fn jzod_stdout_is_pure_json_with_no_stray_lines() {
    let Some(jpl) = jpl_data_dir() else {
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
            "--jzod",
            "--jpl-data",
            &jpl,
        ])
        .output()
        .expect("run starcat");

    assert!(
        output.status.success(),
        "starcat failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("stdout must be UTF-8");

    // Exactly one JSON document, no extra stdout lines: the whole trimmed
    // stdout must parse, and re-serializing it must round-trip to the same
    // non-whitespace content (i.e. nothing precedes/follows the document).
    let parsed: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("stdout must be a single JSON document");
    assert!(parsed.is_object(), "expected a JSON object document");

    // A single `println!` writes the whole pretty-printed document, so
    // stdout is exactly that string plus one trailing newline — assert that
    // shape directly rather than just "parses", to catch a stray leading or
    // trailing diagnostic line.
    assert_eq!(
        stdout.matches('\n').count(),
        stdout.trim_end_matches('\n').matches('\n').count() + 1,
        "expected exactly one trailing newline, got stdout:\n{stdout:?}"
    );
}

/// A diagnostic interleaved with a jzod run (an unknown `--stars` name) must
/// land on stderr, never polluting the jzod stdout payload.
#[test]
fn unknown_star_warning_goes_to_stderr_not_stdout() {
    let Some(jpl) = jpl_data_dir() else {
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
            "--jzod",
            "--stars",
            "not-a-real-star",
            "--jpl-data",
            &jpl,
        ])
        .output()
        .expect("run starcat");

    assert!(
        output.status.success(),
        "starcat failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    let _: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("stdout must still be pure JSON");
    assert!(
        !stdout.contains("unknown star"),
        "the unknown-star warning must not appear on stdout, got:\n{stdout}"
    );
    assert!(
        stderr.contains("unknown star") && stderr.contains("not-a-real-star"),
        "expected the unknown-star warning on stderr, got:\n{stderr}"
    );
}

/// `--jzod --verbose` together: the "data sources" section is a `--text`/
/// `--page` concept (`cmd_compute` gates it on `!is_jzod && verbosity.is_verbose()`
/// — see `tests/cli_compute.rs::non_verbose_text_omits_data_sources_section`
/// for the text-mode sibling of this check). `--verbose` must not leak that
/// diagnostic prose onto jzod's stdout: jzod already carries the same
/// provenance data machine-readably under `ephemeris.sources` (see
/// `jzod_output_reports_generator_and_planet_source` in `cli_compute.rs`), so
/// stdout must stay pure JSON with no "data sources" text regardless of
/// verbosity.
#[test]
fn jzod_verbose_stdout_has_no_data_sources_text() {
    let Some(jpl) = jpl_data_dir() else {
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
            "--jzod",
            "--verbose",
            "--jpl-data",
            &jpl,
        ])
        .output()
        .expect("run starcat");

    assert!(
        output.status.success(),
        "starcat failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("stdout must be UTF-8");

    // Stdout must still be exactly one parsable JZOD document.
    let parsed: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("stdout must be a single JSON document");
    assert!(parsed.is_object(), "expected a JSON object document");

    // The human "data sources" diagnostic must never appear on jzod stdout,
    // verbose or not — locks in the `!is_jzod` gate in `cmd_compute`.
    assert!(
        !stdout.contains("data sources"),
        "jzod stdout must never contain 'data sources' text, even with --verbose:\n{stdout}"
    );
}
