//! Shared helper for starcat's subprocess-driven integration tests.
//!
//! Each integration test binary (`cli_compute.rs`, `exit_codes.rs`,
//! `streams.rs`) spawns the built `starcat` binary via `Command::new`, which
//! by default inherits the *whole* parent process environment — including
//! any `STARCAT_*` compute vars a developer exports as personal shell
//! defaults (e.g. `STARCAT_TZ`). Tests pass their scenario explicitly as
//! argv, so an inherited env var never changes a resolved value (a typed CLI
//! flag wins), but clap's `conflicts_with` checks fire on *presence*
//! regardless of origin — e.g. `--lmt` in argv still conflicts with an
//! ambient `STARCAT_TZ`, even though no `--tz` flag was ever typed. `not(test)`
//! Cargo/CI runs (a clean env) never exercise this path, which is why the gap
//! was latent.
//!
//! [`starcat_command`] returns a `Command` with every clap-bound `STARCAT_*`
//! compute-input var pre-cleared, so these tests behave identically in any
//! shell. `STARCAT_JPL_DATA` / `STARCAT_HORIZONS_DATA` are deliberately left
//! untouched — they are hand-rolled (not clap `env` attrs), and clearing them
//! would make data-dependent tests skip instead of run.

use std::process::Command;

const STARCAT_BIN: &str = env!("CARGO_BIN_EXE_starcat");

/// Every `STARCAT_*` var bound to a `compute` clap arg via `env = "..."` in
/// `crates/starcat/src/main.rs`'s `ComputeArgs`.
const COMPUTE_ENV_VARS: &[&str] = &[
    "STARCAT_DATE",
    "STARCAT_TIME",
    "STARCAT_CALENDAR",
    "STARCAT_TZ",
    "STARCAT_LMT",
    "STARCAT_LAT",
    "STARCAT_LON",
    "STARCAT_HELIO",
    "STARCAT_BODIES",
    "STARCAT_HOUSE",
    "STARCAT_NODES",
    "STARCAT_LILITH",
    "STARCAT_JZOD",
    "STARCAT_TEXT",
    "STARCAT_PAGE",
    "STARCAT_DD",
    "STARCAT_DMS",
    "STARCAT_DDM",
    "STARCAT_DM",
    "STARCAT_D",
    "STARCAT_ASTEROIDS",
    "STARCAT_SPK",
    "STARCAT_OMNISCIENT",
    "STARCAT_STARS",
    "STARCAT_ANTISCIA",
    "STARCAT_DRACONIC",
    "STARCAT_ZODIAC",
    "STARCAT_AYANAMSHA",
    "STARCAT_AYANAMSHA_FRAME",
    "STARCAT_VERBOSE",
    "STARCAT_QUIET",
];

/// A `Command` for the built `starcat` binary with ambient `STARCAT_*`
/// compute-input vars cleared, so the test's explicit argv is the only thing
/// driving the parse — regardless of what the developer's shell exports.
/// `STARCAT_JPL_DATA` / `STARCAT_HORIZONS_DATA` are preserved.
#[allow(dead_code)] // not every test binary that includes this module uses every helper
pub fn starcat_command() -> Command {
    let mut cmd = Command::new(STARCAT_BIN);
    for var in COMPUTE_ENV_VARS {
        cmd.env_remove(var);
    }
    cmd
}
