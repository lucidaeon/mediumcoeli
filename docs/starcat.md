# Starcat

[![Crates.io](https://img.shields.io/crates/v/starcat.svg)](https://crates.io/crates/starcat)
[![Documentation](https://docs.rs/starcat/badge.svg)](https://docs.rs/starcat)
[![License](https://img.shields.io/crates/l/starcat.svg)](https://github.com/lucidaeon/mediumcoeli#license)


A fast, modern ephemeris reader that turns a civil date + location into a complete chart-of-the-moment in one shot.

## What it does

Given a date, time, calendar, and zone (or LMT longitude), `starcat compute` emits:

- Tropical or Sidereal ecliptic-of-date apparent positions — longitude, latitude, distance, daily speed, retrograde flag and more.
- Geocentric, topocentric , or heliocentric frames.
- The chart axes — `Ac`/`Ds`, `Mc`/`Ic`, `Vx`/`Ax`, `Nn`/`Sn`, `Lil`/`Pri` — with selectable mean-vs-true modes for nodes and Lilith.
- Five house systems in one call: Whole Sign, Equal-from-Asc, Placidus, Regiomontanus, Porphyry.
- Hellenistic sect and the eight Hermetic lots (Fortune, Spirit, Exaltation, Necessity, Eros, Courage, Victory, Nemesis).
- Output as human-readable text, JSON (`--json`), or an opt-in TUI page (`--page`, feature-gated).

Coordinates render in `--dd`, `--dms`, `--ddm`, `--dm`, or `--d`. The JSON shape includes a `placements` wrapper with `calculated_at`, daily-motion floats at 8dp, and whole-sign cusps as exact multiples of 30°.

## Why it exists

`starcat` the reference implementation of [`pericynthion`](pericynthion.md). Suitable for quick chart lookups, pipelines, batch backtesting, or wiring into other tools.

## How it works

`starcat` is a thin CLI shell over [`pericynthion`](pericynthion.md). The binary does three jobs:

1. **Parse and validate input.** Date/time parsing, calendar selection (Julian, Gregorian, or auto-detect at 1582-10-15), longitude/latitude parsing in DD/DMS/DDM, JPL data resolution (`--jpl-data` → `--jpl-file`+`--jpl-header` → `$STARCAT_JPL_DATA`), and disambiguation of the LMT-vs-UTC-offset workflows.
2. **Drive the library.** Convert civil time → JD UT → JD TT, open the ephemeris, and call `pericynthion::coords::apparent::apparent_ecliptic_position` (or its topocentric/heliocentric siblings) once per requested body, plus the angle, node, Lilith, lots, and house-cusp routines.
3. **Serialize.** Format coordinates in the requested style and emit text, JSON, or the page renderer.

The default build has a minimal dependency tree (`clap`, `anyhow`, `serde`); the `page` feature pulls in `tabled` for the TUI table. Integration tests live in `tests/cli_compute.rs` and exercise the full pipeline against reference charts when `STARCAT_JPL_DATA` is set, skipping cleanly when it isn't.
