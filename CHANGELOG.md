# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

---

## [0.1.1](https://github.com/lucidaeon/mediumcoeli/compare/0.1.0...0.1.1) — 2026-06-18

### Fixed — `pericynthion`

- **Morinus house cusps were computed with the obliquity factor on the
  wrong `atan2` term** and H10 was incorrectly pinned to the MC. Against
  the First Contact refchart oracle the cusps were off by up to 4.6°.
  Corrected the equator→ecliptic projection to `atan2(sin α · cos ε,
  cos α)` and removed the MC pin; all twelve cusps now match the oracle to
  arcseconds. (Bug was latent — Morinus shipped only behind `noref-houses`.)
- **Acceptance-test ΔT / JDE tolerance widened for future-dated charts.**
  For `unix_overflow_2038` and `first_contact`, both tolerances are now 60 s.
  ΔT beyond the last IERS observation is not known; the Espenak/Meeus
  parabola (pericynthion) and the reference tool's IERS linear extrapolation
  diverge by up to ~40 s at 2038–2063 — the new band covers that disagreement
  without changing any computation.

### Added — `pericynthion`

- **Vehlow Equal house system** (`noref-houses`). Equal 30° houses seeded from
  the Ascendant, each cusp shifted 15° earlier — centres each zodiac sign on a
  house rather than starting it there. H1 ≠ Asc; H10 ≠ MC.
- **Carter Poli-Equatorial house system** (`noref-houses`). Divides the
  celestial equator into twelve 30° arcs from the RA of the Ascendant, projects
  each back to the ecliptic via `λ = atan2(sin α, cos α · cos ε)`. H1 = Asc
  exactly; H10 ≠ MC. Latitude-independent.
- **Pullen Sinusoidal Delta house system** (`noref-houses`, astro.com code `L`).
  Generalises Porphyry with a linear delta offset `Δ = (q − 90°) / 4` on each
  30° cusp step, where `q` is the minimum MC–Asc arc. Collapses to Porphyry
  at `q = 90°`; collapses intermediates to the quadrant midpoint when either
  quadrant is narrower than 30°. Latitude-independent.
- **Pullen Sinusoidal Ratio house system** (`noref-houses`, astro.com code `Q`).
  Scales intermediate cusps by a ratio `r` solved analytically from the minimum
  quadrant arc via a closed-form depressed cubic. The r⁴ factor produces
  pronounced central-house swelling for extreme quadrant charts. Latitude-independent.

### Added — `blackmoon`

- **`--clear`** — delete every chart on a web target (`--target luna /
  astrocom / astrotheoros`) after an interactive confirmation prompt, with a
  zero-padded `[n/N] name  deleted` progress line per chart.

### Changed — `pericynthion` / `starcat`

- **Morinus house system promoted out of `noref-houses`.** Refchart oracle
  captured (`docs/ref_first_contact_morinus.md`) and acceptance test added.
  Morinus now compiles in default builds, is emitted in `starcat` JZOD output,
  and is part of `HouseArg::ALL` (computed when `--house` is omitted). Seven
  house systems are now always-on; twelve remain gated.

### Changed — `blackmoon`

- **Web-write output consolidated into one block per chart.** The previous
  three sections — pre-write field-drop list, write-progress lines, and the
  post-write readback transcript — are now a single per-chart block: a
  `[n/N] name  created uuid=…` header immediately followed by that chart's
  field-by-field transcript. The redundant per-chart drop list is gone (the
  transcript already shows `→ (dropped)`); the global "sink does not store …"
  notice is condensed to one line. Progress counters are zero-padded to the
  width of the total.
- **astrotheoros writes now verify inline, with no extra HTTP.** Because the
  `POST /api/chart` response echoes the full landed entry, each chart is
  diffed and its block printed the instant it lands — no post-write readback.
  Account-wide globals (house system, zodiac) are fetched once up front. Luna
  and astro.com (whose create responses don't echo the full entry) keep the
  transient-progress-then-readback path.

### Changed — `astrogram` (**breaking**)

- **`AstrotheorosSession::create_one` now returns the full `ApiChartEntry`**
  (the create response echoes the complete landed chart) instead of just the
  UUID `String`; callers wanting only the id use `.id`.
- **`AstrotheorosSession::write_charts` signature changed** from separate
  `on_start` / `on_result` closures to a single per-record callback
  `on_record(orig_index, new_index, total_new, source, status, landed_entry)`,
  exposing the landed entry so callers can verify a write without a readback.

---

## [0.1.0](https://github.com/lucidaeon/mediumcoeli/compare/0.0.1...0.1.0) — 2026-06-18

### Added — `astrogram`

- **`capability` module** — `ChartField` enum enumerates the canonical `Chart`
  fields whose support varies across formats (universal-core fields excluded).
  `CapabilitySet` wraps a `&'static [ChartField]`. `lost_fields` and
  `fill_fields` compute what data a source→sink conversion drops or needs
  supplied before writing. Each format module now carries `READ_CAPS` /
  `WRITE_CAPS` constants, making field-loss detection a pure data lookup.
- **`format` module** — `Format` enum is the canonical format identity shared
  by the CLI and library consumers. `FORMATS` is the static registry: each
  `FormatSpec` carries `slug`, `Kind` (`File` | `Web`), `Auth` (`None` |
  `Token` | `LoginOrToken`), file extensions, read/write direction, and
  `CapabilitySet` pointers. `from_slug` resolves a format by slug string.
- **`transcript` module** — post-write readback diffing. `diff(source, landed,
  field_notes)` produces a `Vec<FieldMapping>` in fixed display order; each
  `FieldMapping` carries a `FieldStatus` (`Preserved` / `Transformed` /
  `Dropped` / `Filled`) and pre-rendered source/landed strings. Used by
  `blackmoon` to report per-field fidelity after every web write.
- **`astrotheoros` module — astrotheoros.com as an authenticated web target.**
  New public `astrotheoros` module providing full read/write/delete against
  astrotheoros.com:
  - **RSC wire-format parser** (`parse_rsc_response`) for the Next.js React
    Server Components payload returned by the chart-listing endpoint.
  - **Clerk session auth** — `AstrotheorosSession::login` (Clerk identify +
    auth, two-step) and `from_jwt`, with JWT expiry detection (`jwt_exp`) and
    refresh.
  - **`fetch_charts`** — pulls every chart via the RSC endpoint, returning
    `Chart`s alongside their astrotheoros UUIDs.
  - **`create_one` / `write_charts`** — atlas timezone lookup plus chart
    creation; **`delete_one` / `delete_charts`** — removal by UUID.
  - Data-conversion helpers (`entry_to_chart`, `chart_to_create_body`,
    `calendar_to_unix_ms`, `extract_client_uat`) and a dedicated
    `AstrotheorosError` type.
- `lib.rs` doc updated: extractors now list `astrotheoros.com` alongside
  `lunaastrology.com` and `astro.com`.
- **`jzod` module** — write-only JZOD v0.0.0 serializer. Converts a
  `&[Chart]` to a JZOD-compliant JSON document; UIDs are deterministic
  from birth data (stable across repeated exports).
- **`raw` module** — write-only key:value text format for inspection.
  Emits one `key: value` line per chart field, blank line between charts;
  designed for piping into `grep` / `awk` or human reading.

### Added — `blackmoon`

- **astrotheoros.com as a read/write/delete target.** New `--from
  astrotheoros` / `--to astrotheoros` target, `--astrotheoros-user` /
  `--astrotheoros-pass` (auto-login) and `--astrotheoros-token`
  (`jwt:session_id:client_uat`) credentials. Existing-chart dedup uses
  astrotheoros UUID lookup keyed on spacetime, matching the LUNA/astro flows.
- **`--strict`** — abort the conversion (non-zero exit) when the output
  sink cannot store one or more fields present in the source, instead of
  warning and proceeding.
- **`--no-verify`** — skip the post-write read-back transcript on web
  targets. Read-back and diff are on by default.
- **`--fill-house` / `--fill-zodiac` / `--fill-locus`** — supply a value
  for per-chart fields the source never carried (e.g. when converting ADB
  XML → SFcht). Without a flag, blackmoon prompts interactively on a TTY
  or errors in non-interactive mode.

### Changed — `astrogram` (**breaking**)

- **`astro` module renamed to `astrocom`.** Any code importing
  `astrogram::astro` must be updated to `astrogram::astrocom`; the public API
  is otherwise unchanged.

### Changed — environment variables and CLI flags (**breaking**)

- **Credential env vars and flags renamed.** Update any scripts or shell profiles:

  | Old env var | New env var |
  |---|---|
  | `LUNA_ASTROLOGY_APP` | `LUNA_TOKEN` |
  | `ASTRO_COM_CID` | `ASTROCOM_TOKEN` |
  | `ASTRO_COM_USER` | `ASTROCOM_USER` |
  | `ASTRO_COM_PASS` | `ASTROCOM_PASS` |
  | (new) | `ASTROTHEOROS_TOKEN` / `ASTROTHEOROS_USER` / `ASTROTHEOROS_PASS` |

  | Old flag | New flag |
  |---|---|
  | `--luna-session` | `--luna-token` |
  | `--astro-session` | `--astrocom-token` |
  | `--astro-user` | `--astrocom-user` |
  | `--astro-pass` | `--astrocom-pass` |

- **`--astro-delete` / `--luna-delete` removed.** Use `--consolidate`
  (interactive spacetime-keyed dedup and delete) instead.

### Fixed — `astrogram`

- **astro.com chart creation now resolves the city through the autocomplete
  API.** Mirrors the browser's JS flow: queries `place_query` to obtain the
  `scit` label and `spli` atlas identifier, submits `js=true` / `sown=n` /
  `sctr`, and re-submits the server's confirmation form (carrying `extset`
  and the embedded `sprev`) when the first POST returns the disambiguation
  page. Previously the atlas was resolved only via the `spli` dropdown
  fallback, which missed the browser's confirmation step.
- **astro.com timezone format** — `offset_to_szon` omits the minutes segment
  for whole-hour offsets (`h8w`, not `h8w00`; `h0e`, not `h0e00`), matching
  what astro.com submits.
- **LUNA delete used the wrong form tokens and HTTP method.** `delete_phenom`
  now reads the CSRF/`_Token` envelope from the *delete* form
  (`action=/phenomena/delete/<uuid>`) rather than the edit form, and
  `delete_payload` sends `_method=POST` instead of `_method=DELETE` — the
  delete route is reached by POSTing directly. Deletes failed silently before.

### Fixed — `blackmoon`

- astrotheoros.com UUID lookup, doc comments, and write-confirmation prompt
  aligned with the LUNA/astro targets.

### Changed — internal

- Removed dead `RscParseFailed` error variant; clippy pedantic cleanup
  (`similar_names`, cast-truncation, `doc_markdown` backticks); `cargo fmt`.
- Web integration tests gated behind `#[ignore]` so the default test run stays
  offline; synthetic city pool expanded to 41 cities >10M population;
  round-trip test names normalized across `astrogram` / `pericynthion`.

---

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
