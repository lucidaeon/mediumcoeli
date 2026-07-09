# Starcat

[![Crates.io](https://img.shields.io/crates/v/starcat.svg)](https://crates.io/crates/starcat)
[![License](https://img.shields.io/crates/l/starcat.svg)](https://github.com/lucidaeon/mediumcoeli#license)


A fast, modern ephemeris reader that turns a civil date + location into a complete chart-of-the-moment in one shot.

## What it does

Given a date, time, calendar, and zone (or LMT longitude), `starcat compute` emits:

- Tropical or Sidereal ecliptic-of-date apparent positions — longitude, latitude, distance, daily speed, retrograde flag and more.
- Geocentric, topocentric , or heliocentric frames.
- The chart axes — `Ac`/`Ds`, `Mc`/`Ic`, `Vx`/`Ax`, `Nn`/`Sn`, `Lil`/`Pri` — with selectable mean-vs-true modes for nodes and Lilith.
- Seven house systems in one call: Whole Sign, Equal-from-Asc, Placidus, Regiomontanus, Porphyry, Alcabitius, Morinus.
- Hellenistic sect and the eight Hermetic lots (Fortune, Spirit, Exaltation, Necessity, Eros, Courage, Victory, Nemesis).
- Fixed stars via `--stars` (comma-separated common names, Robson/Brady names, HR numbers, or BSC5P designations). The sentinel `notable` (alias `all`, case-insensitive) expands to the 33 common-name stars; it combines with explicit names and de-duplicates, so `--stars notable,Regulus` yields the 33 with no duplicate Regulus. A bare `--stars` (no value) is equivalent to `--stars notable`.
- Output as human-readable text, JSON (`--json`), or an opt-in TUI page (`--page`, feature-gated).

Coordinates render in `--dd`, `--dms`, `--ddm`, `--dm`, or `--d`. The [`jzod`](https://github.com/lucidaeon/mediumcoeli/blob/main/crates/jzod/JZOD.md) JSON shape includes a `placements` wrapper with `calculated_at`, daily-motion floats at 8dp, and whole-sign cusps as exact multiples of 30°.

## Why it exists

`starcat` the reference implementation of [`pericynthion`](https://github.com/lucidaeon/mediumcoeli/blob/main/crates/pericynthion/README.md). Suitable for quick chart lookups, pipelines, batch backtesting, or wiring into other tools.

## How it works

`starcat` is a thin CLI shell over [`pericynthion`](https://github.com/lucidaeon/mediumcoeli/blob/main/crates/pericynthion/README.md). The binary does three jobs:

1. **Parse and validate input.** Date/time parsing, calendar selection, longitude/latitude parsing in DD/DMS/DDM, JPL data resolution (`--jpl-data` → `--jpl-file`+`--jpl-header` → `$STARCAT_JPL_DATA`), and disambiguation of the LMT-vs-UTC-offset workflows. `--date` and `--time` are required. `--calendar` is **optional for dates before 1582-10-15 or after 1927** (defaults to `auto`: proleptic Julian before the cutover, Gregorian on or after) and **required for 1582-1927 dates**, where the recorded calendar is jurisdiction-dependent (Catholic Europe switched 1582, Britain/US 1752, Russia 1918, Greece 1923, Turkey 1926) — omitting it there is a hard error, so ambiguous charts fail loudly instead of silently guessing. Pass `--calendar julian|gregorian` in that window.
2. **Drive the library.** Convert civil time → JD UT → JD TT, open the ephemeris, and call `pericynthion::coords::apparent::apparent_ecliptic_position` (or its topocentric/heliocentric siblings) once per requested body, plus the angle, node, Lilith, lots, and house-cusp routines.
3. **Serialize.** Format coordinates in the requested style and emit text, JSON, or the page renderer.

The default build has a minimal dependency tree (`clap`, `anyhow`, `serde`); the `page` feature pulls in `tabled` for the TUI table. Integration tests live in `tests/cli_compute.rs` and exercise the full pipeline against reference charts when `STARCAT_JPL_DATA` is set, skipping cleanly when it isn't.

## Data commands

### `starcat data fetch de441` (basic)

`starcat data fetch de441` pulls the **static JPL DE441 mirror** from fixed HTTP
paths. This is two things from the *same survey, version 441* — planets and
asteroids:

- the **DE441 planetary ephemeris** (Sun, Moon, Mercury..Pluto barycenters), and
- the **DE441 small-body bundles** — `sb441-n16.bsp` and `sb441-n373.bsp`.

They ship as pre-built files at fixed URLs, so a fetch is a plain mirrored
download (resumable and self-verifying by BLAKE3). The command prints the
destination, the file count and total size up front, leaves a scrollback line
as each file starts, and runs a verify pass at the end.

Fetched files always land in the **default data directory** (see *Where the data
lives* below) — the destination is fixed and is no longer overridable. A fetch
never re-downloads or duplicates data it can obtain locally. Per DE441 file:

1. **Already present + BLAKE3-valid** in the default data dir — skipped (no
   network, no copy).
2. Not present locally but present + BLAKE3-valid at an **existing mirror you
   name** (see `--jpl-data` below) — **copy-on-write cloned** into the data dir
   (APFS `clonefile` / btrfs+XFS `FICLONE` / Windows ReFS block-clone, via the
   [`reflink-copy`](https://crates.io/crates/reflink-copy) crate), which costs
   near-zero disk, then re-verified. Off copy-on-write filesystems this falls
   back to a plain full **copy**. No network either way.
3. Valid **nowhere locally** — downloaded from the fixed HTTP paths.

The summary at the end distinguishes downloaded / reflinked / copied / skipped
counts, with a persistent line per reflinked or copied file. Your named mirror
is only ever read from — it is cloned or copied, never moved or deleted.

`--jpl-data PATH` (falling back to `$STARCAT_JPL_DATA`) names **your existing
opinionated JPL mirror** to reuse as that copy-on-write source. It is no longer
a fetch *destination*: if it points at (or into) a real mirror it lets the fetch
clone what you already have instead of re-downloading it; if it names nothing
usable, the fetch simply proceeds over the network.

This is distinct from HORIZONS, which *generates* SPKs on demand rather than
serving a fixed file.

#### What each dataset gets you

Each dataset unlocks a distinct group of bodies. Run `starcat data fetch --what`
for the live version of this list (it is generated from the catalog, so it never
drifts):

- **DE441 planetary binary** — the Sun, the Moon, Mercury through Neptune, and
  Pluto, plus the **computed points** (Ascendant, Midheaven, the lunar nodes,
  the lots, etc.) that come free with the planetary binary.
- **`sb441-n16.bsp`** (bundled with the DE441 fetch) — Ceres, Pallas, Juno,
  Vesta, and Hygiea.
- **`sb441-n373.bsp`** (bundled with the DE441 fetch) — the extended small-body
  set: Eris, Haumea, Makemake, Quaoar, Orcus, Ixion, Varuna, Sedna, and
  Gonggong.
- **HORIZONS on-demand** (not part of the DE441 fetch) — the centaurs Chiron,
  Pholus, Nessus, Chariklo, and Asbolus via `starcat horizons cent`, and the KBO
  Albion via `starcat horizons kbo`.

After a `data fetch`, starcat prints a **capabilities readout** showing which of
these groups are actually on disk right now (marked `[have]` / `[need]`), with a
hint for the `starcat horizons` commands that would supply any absent group.

### On-demand SPKs: `starcat horizons <class>`

`starcat horizons <class>` fetches HORIZONS-generated `<naif>.bsp` SPKs for the
bodies in a class (`dp`, `ast`, `cent`, `kbo`, `tno`) — centaurs, extra TNOs,
KBOs, and other minor bodies not carried in the DE441 small-body bundles. Each
body's SPK is computed by NASA JPL HORIZONS at request time for the requested
span (default: Uranus's discovery 1781-03-13 to the 2038 32-bit `time_t`
overflow). Runs are sequential and throttled, and skip any `<naif>.bsp` already
on disk, so re-runs are idempotent and courteous to JPL.

### Where the data lives

With no flag or env var, both fetch commands resolve a default under the
platform-native persistent data directory:

| OS | Default data directory |
|---|---|
| macOS | `~/Library/Application Support/starcat/` |
| Linux | `$XDG_DATA_HOME/starcat/` (default `~/.local/share/starcat/`) |
| Windows | `%APPDATA%\starcat\data\` |

Inside that directory, DE441 files land under the `ssd.jpl.nasa.gov/…` mirror
subtree, and HORIZONS SPKs land in `…/starcat/horizons/` (a sibling of the
mirror subtree).

Resolution order per command:

- **`data fetch`** — destination is **always** the platform data dir (not
  overridable). `--jpl-data` → `$STARCAT_JPL_DATA` names an *existing mirror* to
  copy-on-write clone from instead of re-downloading; if neither is set or no
  mirror is found there, the fetch is network-only.
- **`horizons`** — `--out` → `$STARCAT_HORIZONS_DATA` → `…/starcat/horizons/`.

### `starcat data provenance`

Read-only report of every catalogued body and the fixed-star catalogue: the
data file(s) that back it, the public source URL, and whether each is cached
locally. Resolves `$STARCAT_JPL_DATA` and `$STARCAT_HORIZONS_DATA` (or `--root`
/ `--horizons`); missing roots simply report "absent". `--json` emits the same
data structured. No network, never exits non-zero.

For fixed stars it prints two facts: that `BSC5_CATALOG` is compiled into the
binary at build time, and that its source is `catalog.gz` from CDS VizieR V/50.

### `starcat data prod` (runtime)

Now enumerates the full production set at runtime: the DE441 binary,
`sb441-n16.bsp`, `sb441-n373.bsp`, and each unbundled minor body's Horizons
`<naif>.bsp` (resolved via `--horizons` → `$STARCAT_HORIZONS_DATA` →
`…/starcat/horizons/`) — so KBOs, TNOs, and centaurs are included. One path per
line, never canonicalized.
