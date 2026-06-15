# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.0.1](https://github.com/lucidaeon/mediumcoeli/compare/0.0.0...0.0.1) — 2026-06-15

### Added — `pericynthion`

- **Alcabitius house system promoted to stable.** Refchart oracle captured
  and acceptance test added to `acceptance_refchart.rs`. Removed
  `#[cfg(feature = "noref-houses")]` gate — Alcabitius now compiles and
  ships unconditionally.

### Added — `starcat`

- **JZOD output is now the default.** Running `starcat compute` with no output
  flag emits JZOD-format JSON. `--text` and `--page` are the explicit opt-ins
  and are mutually exclusive with each other and with `--jzod`.
- **`--jzod` flag** (with `--json` as a visible alias) makes the default
  explicit; it is a no-op when neither `--text` nor `--page` is given.
- **`--page` flag** added as an explicit opt-in (previously the implicit
  default when the `page` feature was enabled).
- **Alcabitius** added to `HouseArg::ALL` and emitted in JZOD output alongside
  the five previously promoted systems.
- **UUID dependency** added; chart `uid` fields are now populated with v4 UUIDs.

### Changed — `starcat`

- JZOD mode computes all six promoted house systems unconditionally; `--house`
  filtering applies only to `--text` and `--page` modes.
- `--dm` coordinate format default now applies only when `--page` is explicitly
  passed; `--text` and JZOD both default to `--dd`.

### Added — `astrogram`

- **`write_file_with_description`** — new public function on the `sfcht` module.
  Writes a `.SFcht` file stamping `"Blackmoon <version>"` in the 80-byte file
  description header field; preserves any existing description that was not
  written by Blackmoon.

### Changed — `blackmoon`

- SFcht write path now reads the existing file's description header before
  writing, passing it to `write_file_with_description` so hand-curated
  descriptions are preserved across overwrite operations.

### Changed — build / justfile

- **`just docker build`** now loads a single-arch image into the local daemon
  (`--load`) without pushing. BuildKit layer cache is populated for subsequent
  multi-arch push.
- **`just docker push`** added as the explicit gate for multi-arch
  (`--platform`) buildx push to all configured registries.
- Removed `just docker local`, `just docker tag`, and the old auto-push
  behaviour from `just docker build`.

---

## [0.0.0](https://github.com/lucidaeon/mediumcoeli/compare/0a080f3534e15c52e6e3493815eb85875f08e179...0.0.0)

Initial public release. Four crates ship as one workspace:

- **`pericynthion`** — astrological ephemeris library (pure-Rust JPL DE441 reader).
- **`astrogram`** — chart data-format conversion library.
- **`starcat`** — command-line ephemeris built on `pericynthion`.
- **`blackmoon`** — command-line format converter built on `astrogram`.

### Added — `pericynthion` (ephemeris library)

- **JPL DE441 reader.** Pure-Rust parser for the ASCII header and binary
  coefficient file, with discovery (`linux_*.NNN` / `xnp_*.NNN`),
  little- and big-endian decoders, granule + sub-granule navigation, and
  bounds-checked record lookup. Coverage matches the file: JD
  −3,100,015.5 to +8,000,016.5 (≈ −13200 BCE to +17191 CE).
- **Chebyshev evaluator.** Position and velocity from the per-body
  coefficient series.
- **Bodies.** Sun, Moon, Mercury through Pluto, Earth (derived from EMB
  and Moon via EMRAT), and the Earth-Moon barycentre.
- **Coordinate pipeline.** Light-time iteration, annual aberration, IAU
  2006 precession (three-angle model with T⁴/T⁵ terms and the ±2.650545″
  frame-bias constant), IAU 2000B 77-term nutation (sub-mas in the
  modern era), mean and true obliquity, equatorial-to-ecliptic rotation.
- **Apparent positions.** Three shipped variants:
  - `apparent_ecliptic_position` (geocentric),
  - `apparent_ecliptic_position_topocentric` (observer parallax via
    WGS84 + GAST),
  - `heliocentric_ecliptic_position` (Sun-centred, no aberration).
- **ΔT model.** USNO/IERS observational table 1657–2025 with linear
  interpolation; SMH 2016 cubic spline −720 to <1657; parabolic /
  Espenak-Meeus extrapolation outside that range.
- **Chart points.** Ascendant/Descendant, MC/IC, Vertex/Anti-Vertex,
  true and mean lunar nodes, true and mean Black Moon Lilith/Priapus.
- **Hermetic lots.** Eight classical lots (Fortune, Spirit, Eros, etc.).
- **House systems.** Five always-on: Whole Sign, Equal-from-Ascendant,
  Placidus, Regiomontanus, Porphyry. Ten more behind the
  `noref-houses` cargo feature: Koch, Campanus, Alcabitius, Morinus,
  Meridian, Equal-from-MC, Horizontal, Topocentric (Polich-Page),
  Krusiński-Pisa-Goeldi, Sripati.
- **Time + civil-date support.** Julian and Gregorian calendars, civil
  date to JD, named time-zone offsets, LMT-from-longitude convenience,
  Unix-epoch round-trip.
- **Geographic-coordinate parser.** DMS, DDM, decimal-degree variants
  with hemispheres.

### Added — `starcat` (CLI ephemeris)

- `starcat compute` subcommand with the full chart pipeline behind a
  single invocation: civil date + time + zone → `JD_UT` → ΔT → `JD_TT`
  → JPL discovery → ephemeris → coords → houses → output.
- Three coordinate modes: geocentric (default), topocentric (`--lat` +
  `--lon`), heliocentric (`--helio`).
- Output formats: textual chart card (default) or JSON (`--json`).
- Body filter (`--bodies`), house filter (`--house`), node mode
  (`--nodes mean|true`), Lilith mode (`--lilith mean|true`).
- Display precision selectors: `--dms`, `--ddm`, `--dm`, `--d`.
- LMT and fixed-offset time-zone handling (`--lmt`, `--tz`).
- Shell-completion generator (`starcat generate-completion`) for bash,
  zsh, fish, and PowerShell.

### Added — `astrogram` (chart-format library)

- **SFcht** (Solar Fire chart files, cp1252) reader and writer with
  Kaitai-derived layout.
- **ADB XML** (Astrodatabank `export_format` 160715) reader and writer.
- **Zeus** chart database (`.zdb`, UTF-8 semicolon-delimited) reader
  and writer.
- **AAF** (Astrolog) reader.
- **LUNA®** web-listing scraper and adapter to the canonical `Chart`
  type, including session + `cast_json` parsing.
- **astro.com** web session + chart-list scraper.
- **Consolidation engine.** Deduplication keyed on
  `(name, year, month, day, hour, minute, second)` across sources.
- **Durable decision log** (JSONL) for consolidation keystrokes.
- **Normalization.** cp1252-safe transliteration of chart text fields.

### Added — `blackmoon` (CLI format converter)

- Any-target-in, any-target-out conversion with automatic detection by
  file extension and explicit `--from` / `--to` overrides.
- Multi-input merge with dedup against the existing destination so
  re-runs don't add duplicates.
- LUNA® and astro.com fetch (`--from luna`, `--from astro`).
- In-place normalize mode (`--normalize`) over any number of input
  files.
- Interactive consolidation UI for cross-source dedup (`--consolidate`
  with `--target luna`).
- Shell completion generator (`blackmoon --generate-completion`).

### Added — Testing

- **HORIZONS oracle.** Geocentric, topocentric, and heliocentric
  acceptance tests against NASA JPL HORIZONS for every reference chart,
  with `_geo.json` / `_topo.json` / `_helio.json` fixture families per
  chart. The source of record for accuracy claims.
- **Refchart oracle.** Independent reference-chart comparison covering
  ΔT, JDE, ST(0°), LST, obliquity, Asc/MC, house cusps, body positions,
  Part of Fortune, and lunar nodes.
- **DE441 invariants.** Header layout, binary granule = 32 days,
  Chebyshev round-trip per body, heliocentric frame plumbing.
- **Format round-trips.** SFcht, Zeus, ADB XML; per-record golden tests
  against Python-oracle JSON fixtures.
- **CLI end-to-end.** `starcat compute` spawned as a subprocess, JSON
  output diffed against the refchart oracle — exercises the whole
  surface including clap, time/zone resolution, JPL discovery, and
  serialisation.
- **Tolerance documentation.** `docs/tests.md` records the
  calculation-regime chronology and the rationale behind each tolerance
  band.

### Added — Tooling

- Cargo workspace with `resolver = "3"` on the 2024 edition (Rust
  1.85+).
- `cargo clippy --all-features --all-targets -- -W clippy::pedantic -D warnings`
  clean baseline.
- `scripts/horizons_fetch.py` — pulls HORIZONS fixtures for every
  reference chart in geocentric, topocentric, and heliocentric modes.
- `just` recipes for release builds and DE441 download.
- Docker image and shell-completion installation paths.
