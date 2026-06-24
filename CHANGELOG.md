# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [main](https://github.com/lucidaeon/mediumcoeli/compare/01dd8042b5b5bb0e8df0f55adf030cb556071872...main), [astrogram/0.2.3](https://github.com/lucidaeon/mediumcoeli/releases/tag/astrogram/0.2.3), [blackmoon/0.2.2](https://github.com/lucidaeon/mediumcoeli/releases/tag/blackmoon/0.2.2), [jzod/0.4.0](https://github.com/lucidaeon/mediumcoeli/releases/tag/jzod/0.4.0), [pericynthion/0.6.0](https://github.com/lucidaeon/mediumcoeli/releases/tag/pericynthion/0.6.0), [starcat/0.5.0](https://github.com/lucidaeon/mediumcoeli/releases/tag/starcat/0.5.0), [wristband/0.0.2](https://github.com/lucidaeon/mediumcoeli/releases/tag/wristband/0.0.2), 2026.06.24

### Added — `pericynthion`

- **Fixed-star catalog + ICRS→ecliptic engine**: `FixedStar`, a curated `CATALOG`
  of 12 traditional fixed stars plus the Galactic Center (Sgr A*), and
  `ecliptic_position_from_icrs` / `compute_star` / `galactic_center` using the
  IAU 2006 precession + IAU 2000B nutation pipeline. A drift test pins century
  rates for the Galactic Center and the four Royal Stars.
- **Named-star resolution**: `ResolvedStar`, `StarCluster`, and `resolve_star`
  with a full alias table, backed by the **BSC5 (Bright Star Catalog)** —
  decompressed from a committed `catalog.gz` by a new `build.rs` (`flate2`
  build-dependency).
- **`ComputedChart::stars`**: new `ComputedStar` rows on the computed chart.
  `compute_with_spk` now takes a **caller-supplied `resolved_stars` slice**
  (replacing the auto-populated CATALOG block), giving callers full control over
  which fixed stars appear.
- **Data provenance** (`provenance.rs`): `providers_for_body` joins body, fixed
  star, and Horizons-synthesis sources into a single accounting.
- **Oracle manifest expansion** (`jpl::oracle`): a committed, sized BLAKE3
  manifest tagging each dataset directory with a `SourceKind` and per-file
  provides/coverage; `locate_n373_bsp` alongside `locate_default_bsp`.
- New catalog bodies **Asbolus** (centaur) and **Albion** (1992 QB₁, first
  confirmed classical KBO).

### Changed — `pericynthion`

- **`index_chunks` reads only the head and tail of each ASCII chunk**,
  eliminating full-file reads (~25 MB × 30 files) during `AsciiEphemeris`
  construction; only the first and last records are parsed for `start_jd`/`end_jd`.
- Centaurs (Chiron, Pholus, Nessus, Chariklo) and 9 outer bodies (Eris, Haumea,
  Makemake, Quaoar, Orcus, Ixion, Varuna, Sedna, Gonggong) promoted to
  **supported** via `sb441-n373.bsp` / Horizons SPK.

### Fixed — `pericynthion`

- NOTABLE **Agena** mis-mapped to HR 5440 (Eta Cen); corrected to HR 5267
  (Bet Cen / Hadar). Removed duplicate `STAR_ALIASES` entries (`alpherg`,
  `rasalhague`).

### Added — `starcat`

- **`catalogue` restructured** from a bare command into a flagged one:
  `--stars`, `--clusters`, `--all`, `--bodies`, `--points`, `--verbose`.
- **`compute --stars NAMES`** resolves comma-separated fixed-star names via
  `resolve_star` (unknown names warn and skip); a **Stars** section is rendered
  in `--text` output between the points and lots blocks.
- **`placements --verify [--dry-run]`** discovers unsupported catalog bodies and
  confirms which are computable, feeding `promote_placements.py`.
- **Data provenance output**: body/star sources, URLs, and cached status;
  runtime data production enumerates n373 + Horizons bodies; `data verify` emits
  b3sum-style `<blake3>  <path>` lines.

### Fixed — `starcat`

- Distinguish open-vs-state failure in placements verify; collapse double-slash
  and relativize verify paths under the cwd; suppress the dangling `## Fixed
  Stars` header when no BSC5 catalog is loaded; skip empty/whitespace star names.

### Added — `jzod`

- **`BodyId` extended** with `Asbolus` and `Albion` (snake_case serialisation);
  every `placements` catalog body maps to a `BodyId`.

### Changed — repo

- **`justfile`**: `placements` recipe now runs the full auto-promote pipeline
  (verify → promote → rebuild → regenerate docs); adds `placements-dry-run` and
  `oracle-regen` (generate-then-`cargo fmt -p pericynthion` for byte-identical
  `oracle_data.rs`). New scripts: `promote_placements.py`,
  `extract_oracle_manifest.py`, and an expanded `gen_oracle.py`.
- **`astrogram`** / **`blackmoon`** READMEs: `astro` → `astrocom` format-slug
  rename and refreshed web-target / pipeline docs.
- **`blackmoon`** / **`wristband`** crate descriptions reworded.

## [01dd804](https://github.com/lucidaeon/mediumcoeli/compare/b3670c460b2cdc7f9efb283fde3af4650892a90f...01dd8042b5b5bb0e8df0f55adf030cb556071872), [astrogram/0.2.2](https://github.com/lucidaeon/mediumcoeli/releases/tag/astrogram/0.2.2), [jzod/0.3.0](https://github.com/lucidaeon/mediumcoeli/releases/tag/jzod/0.3.0), [pericynthion/0.5.0](https://github.com/lucidaeon/mediumcoeli/releases/tag/pericynthion/0.5.0), [starcat/0.4.0](https://github.com/lucidaeon/mediumcoeli/releases/tag/starcat/0.4.0), 2026.06.23

### Added — `pericynthion`

- **SPICE SPK asteroid ephemeris** (`spk`): `Daf` DAF/SPK container reader,
  `SpkEphemeris::{open,state,center_of}` evaluating **Type-2 Chebyshev** and
  **Type-21 MDA (Modified Difference Array)** segments — Type-21 is the format
  JPL Horizons generates on demand. Type-21 evaluator implements the FC/WC/W MDA
  recurrence with MAXTRM guard (≤ 25) and km/s → km/day velocity conversion.
  Both types dispatched transparently via `SpkEphemeris::state`. Cross-validated:
  Ceres Type-21 vs Type-2 agree to sub-kilometre.
- **Asteroid apparent positions** (`coords::apparent`):
  `apparent_ecliptic_position_spk` and `apparent_ecliptic_position_spk_topocentric`
  compute tropical ecliptic-of-date apparent positions for SPK bodies, reusing the
  full planet pipeline (Sun-barycentre from DE441, heliocentric vector from SPK,
  light-time, aberration, IAU 2006 precession, IAU 2000B nutation). Absolute
  HORIZONS validation at J2000: 5 main-belt asteroids, max error 0.084″ (Juno lat).
- **Multi-SPK routing**: `compute_with_spk(ephem, spks: &[&SpkEphemeris], request)`
  routes each body to the first SPK that covers it; `spk::open_dir(path)` opens every
  `.bsp` in a directory. `ComputedAsteroid` gains `daily_speed_deg` and `retrograde`
  (sampled at ±0.5 day via two SPK evaluations).
- **JPL Horizons SPK fetcher** (`horizons` feature, optional): `fetch_spk` /
  `fetch_all` pull on-demand SPKs from the public Horizons API, writing
  `<naif_id>.bsp` files using the `20_000_000 + MPC` NAIF scheme. Handles
  Horizons' line-wrapped base64 and single-quotes datetime parameters (bare spaces
  cause Horizons to silently truncate to date-only). Throttled (1 s between requests
  — polite to the shared research resource).
- **`placements` catalog** — single source of truth for every supported body:
  planets, dwarf planets, centaurs, KBOs, TNOs, main-belt asteroids. Each
  `Placement` carries MPC number, `sb441_naif_id()`, `horizons_naif_id()`,
  `horizons_command()`, slug, and category. Public API: `find_by_slug`,
  `name_for_naif` (both id schemes), `supported_list`, deterministic `to_markdown()`.
- **JPL `eph/` dataset oracle** (`jpl::oracle`): hardcoded BLAKE3 + size manifest of
  all 1 374 files across `planets/`, `satellites/`, and `small_bodies/` directories.
  `verify_entry` size-fast-fails then hashes; `production_entries` exposes the required
  subset; `mirror_root_from(start)` walks ancestors to find the `ssd.jpl.nasa.gov/` root.
- **DE441 ASCII ephemeris reader** (`jpl::ascii`): `AsciiEphemeris` parses
  `ascp*.NNN` / `ascm*.NNN` chunks and serves records through the `RecordSource`
  trait — same interface as the binary reader. JD-indexed for fast chunk selection;
  bit-identical to the binary reader at 1e-6 km.
- **Any-node JPL discovery** (`jpl::discover`): `locate` and `open_dataset` walk up
  to 8 levels deep — `start` may be the `de441/` dir, `ascii/`, `planets/`, `eph/`,
  `ftp/`, or the mirror root. Binary preferred over ASCII at the same DE number.

### Added — `jzod`

- **`BodyId` extended** with `Hygiea`, `Pholus`, `Nessus`, `Chariklo`, `Ixion`, and
  `Varuna`. All serialise as snake_case (e.g. `"chariklo"`). Every body in the
  `placements` catalog now maps to a JZOD `BodyId`; enforced by a dedicated test.

### Added — `starcat`

- **`starcat compute --asteroids SLUG,...`** — comma-separated asteroid slugs
  (`ceres`, `pallas`, `juno`, `vesta`, `hygiea`, `chiron`, `pholus`, `nessus`,
  `chariklo`, etc.) appended after the planet block in all output modes (text, page,
  JZOD). Retrograde marker (℞) shown in `--page` output. `--spk PATH` provides an
  explicit BSP; when omitted, `sb441-n16.bsp` is auto-discovered under the JPL
  mirror root and `$STARCAT_HORIZONS_DATA` is opened for Horizons-fetched bodies.
- **`--omniscient`** — compute every body covered by available data files.
- **`starcat horizons <dp|ast|cent|kbo|tno>`** — fetches all bodies in the named
  category (dwarf planets, asteroids, centaurs, KBOs, TNOs) from JPL Horizons,
  writing `<naif_id>.bsp` to `--out` / `$STARCAT_HORIZONS_DATA`. Idempotent (skips
  bodies already on disk), sequential with 1 s throttle, exits non-zero on any failure.
- **`starcat catalogue`** — top-level command listing every supported body (slug,
  category, NAIF ids, MPC) from the placements catalog.
- **`starcat data verify`** — verifies the required production subset against the
  built-in oracle. **`starcat data verify all`** — verifies integrity of all present
  files (absent files skipped; present-but-corrupt files fail). **`starcat data prod`**
  — lists the oracle-covered production file set. (Subcommand `verify-data` renamed
  to `data` with structured sub-modes.)
- **Placements doc generator** — `just placements` regenerates `docs/placements.md`
  deterministically; a golden test guards against drift.
- **`--jpl-data` accepts any mirror node** (binary or ASCII, any hierarchy level).

### Fixed — `pericynthion`

- SPK/DAF reader rejects corrupt summary records (no panic on bad input).
- SPK Type-2 rejects truncated segments (no panic); `from_slug` is alloc-free.
- SPK Type-21: NRECS overflow computed as `i64`; segments with `MAXTRM > 25` rejected.
- Horizons `START_TIME` / `STOP_TIME` single-quoted on the wire — bare spaces caused
  Horizons' batch parser to split the datetime and silently drop the time component.
- Retrograde flag suppressed for asteroids in heliocentric mode.
- Corrected BLAKE3 hashes for 3 satellite/NIO files read during a live mirror sync.

### Fixed — `starcat`

- Lunar phase rendered in all output boxes.
- `--omniscient prod` no longer requires chart arguments.
- `--spk` help text accurately describes the Type-2/Type-21 BSP format.
- `--help` and `long_about` house listing updated to all seven always-on systems
  (Alcabitius, Morinus); asteroid output mode note corrected to "all output modes".

### Added — `starcat`

- `starcat data provenance`: read-only report of every catalogued body and the
  fixed-star catalogue — backing data file(s), source URL, and cached status;
  `--json` supported. Prints both fixed-star facts (compiled-in + CDS V/50 source).

### Changed — `starcat`, `pericynthion`

- `starcat data prod` now enumerates its file set at runtime, including
  `sb441-n373.bsp` and unbundled minor bodies' Horizons SPKs (KBOs/TNOs/centaurs).
- The BLAKE3 oracle is now a data manifest: directories carry a `SourceKind` and
  files carry `provides`/`coverage`; `catalog.gz` (fixed stars) is tracked.
  Generated from `scripts/oracle_manifest.tsv` (mirror-independent regeneration).

---

## [b3670c4](https://github.com/lucidaeon/mediumcoeli/compare/0809052f5004901b7e5d9d97b11cb09fc2aab10c...b3670c460b2cdc7f9efb283fde3af4650892a90f), [astrogram/0.2.1](https://github.com/lucidaeon/mediumcoeli/releases/tag/astrogram/0.2.1), [blackmoon/0.2.1](https://github.com/lucidaeon/mediumcoeli/releases/tag/blackmoon/0.2.1), [jzod/0.2.0](https://github.com/lucidaeon/mediumcoeli/releases/tag/jzod/0.2.0), [pericynthion/0.4.0](https://github.com/lucidaeon/mediumcoeli/releases/tag/pericynthion/0.4.0), [starcat/0.3.1](https://github.com/lucidaeon/mediumcoeli/releases/tag/starcat/0.3.1), [wristband/0.0.1](https://github.com/lucidaeon/mediumcoeli/releases/tag/wristband/0.0.1), 2026.06.20

### Added — `jzod`

- **`Sign::abbrev()`** — three-letter display abbreviation (`Ari`, `Tau`, … `Pis`). Presentation helper; does not affect the `snake_case` wire format.
- **`Sign::split_longitude(lon_deg)`** — converts an absolute ecliptic longitude to `(Sign, f64)` with a cusp-rounding invariant: float noise at sign boundaries (e.g. `29.9999…°`) snaps up to the next sign instead of rendering as `29°…` of the previous one.
- `Position::from_longitude` refactored to delegate to `split_longitude`, eliminating duplicate cusp logic.

### Added — `pericynthion`

- **`LunarPhaseName::label()`** — human-readable label for each of the eight phase names (`"new moon"`, `"first quarter"`, …), shared by every front-end so the CLI, a GUI, and a WASM consumer all print identical strings without reimplementing the mapping.

### Changed — `starcat`

- **Delegates coordinate helpers to `jzod` and `pericynthion` libraries.** Local `zodiac_sign`, `split_sign`, and `phase_name_str` functions replaced with `jzod::coord::Sign::abbrev()` / `split_longitude()` and `pericynthion::LunarPhaseName::label()`. No arithmetic changes; output is identical.

### Fixed — `wristband`

- **Safari module gated to macOS and test builds** (`#[cfg(any(test, target_os = "macos"))]`), silencing 14 `dead_code` warnings on Linux.

### Documented

- **`jzod` format spec extracted to `JZOD.md`.** The full JZOD interchange format specification lives at `crates/jzod/JZOD.md`; `crates/jzod/README.md` is now a standard crate overview (What / Why / How) consistent with the rest of the workspace. Cross-links in `starcat`, `astrogram`, and `jzod/src/lib.rs` updated accordingly.
- **README housekeeping across all crates** — project names capitalized as proper nouns in headers and prose; `wristband` cross-linked from `blackmoon --grant-cookie-access`; `jzod` cross-linked from `starcat` JSON output description.

---

## [0809052](https://github.com/lucidaeon/mediumcoeli/compare/bc317a221d7e71cadae83816615ff5703c24a2dd...0809052f5004901b7e5d9d97b11cb09fc2aab10c), [astrogram/0.2.0](https://github.com/lucidaeon/mediumcoeli/releases/tag/astrogram/0.2.0), [blackmoon/0.2.0](https://github.com/lucidaeon/mediumcoeli/releases/tag/blackmoon/0.2.0), [jzod/0.1.1](https://github.com/lucidaeon/mediumcoeli/releases/tag/jzod/0.1.1), [pericynthion/0.3.0](https://github.com/lucidaeon/mediumcoeli/releases/tag/pericynthion/0.3.0), [starcat/0.3.0](https://github.com/lucidaeon/mediumcoeli/releases/tag/starcat/0.3.0), [wristband/0.0.0](https://github.com/lucidaeon/mediumcoeli/releases/tag/wristband/0.0.0), 2026.06.20

### Added — `wristband` (new crate)

- **Consent-gated, domain-scoped reader for the user's own browser session cookies.** It exists so `astrogram`/`blackmoon` can authenticate to web targets with a cookie the user already holds, without ever becoming a general cookie harvester. The consent and scoping invariants are enshrined in `crates/wristband/SECURITY.md` and proven by a no-network conformance suite.
- **`Domain` is registrable-only** (validated via the public-suffix list, `psl`): no zone/TLD globbing, malformed hostnames rejected eagerly in `Domain::explicit`. Cookies are filtered to the requested registrable domain *before* any decryption — the gate is the only path to plaintext (invariants INV-2/3/6).
- **Firefox** end-to-end: plaintext `cookies.sqlite` reader with `userContextId` container scoping (anchored match — container `2` never matches `20`).
- **Chromium family** (Chrome, Edge, Brave, Opera, …) across all three desktop OSes:
  - macOS — Keychain key + PBKDF2-SHA1 (1003 iterations) + AES-128-CBC, with an independent KAT for the key derivation.
  - Linux — desktop-environment detection, keyring dispatch, PBKDF2 (1 iteration) + AES-128-CBC; unknown environments fall back to `BASICTEXT`.
  - Windows — DPAPI via PowerShell + AES-256-GCM (v10), with **no `unsafe`**.
  - Shared v10/v11 framing, meta-version hash strip, and per-row read.
- **Safari** — `binarycookies` parser plus Safari 17+ per-profile `WebsiteDataStore` discovery; file-derived page/cookie counts are capped to refuse pathological allocations.
- **All-installed-stores aggregation** (the `None` selector) reads every discovered profile and picks across them; copy-before-read SQLite access copies `-wal`/`-shm` sidecars (INV-5) so a live browser lock never blocks a read.

### Added — `astrogram`

- **`web_auth::try_chain` credential fall-through combinator** with HTTP 401/403 detection: browser-cookie, token, and login sources are attempted in order, advancing only when a source is rejected as stale. Each web session (luna, astrocom, astrotheoros) gains an `authenticate()` entry point and an authenticated `probe()`, and classifies auth failures through its own error enum. Replaces the previous cookie-*or*-login behaviour.
- **`cookie-import` feature** (optional `dep:wristband`): provider→domain mapping and cookie→session glue, where `import_credential` yields chainable credential material. A GUI built on `astrogram` without `blackmoon` can import the user's own session cookies.
- **`convert` module** — `read_bytes` / `write_bytes` format dispatch, the single byte↔`Chart` call sites a non-CLI consumer needs.
- **`from_str_slug` parsers** for `HouseSystem` / `Zodiac` / `CoordinateSystem`; `temporal_key` / `has_tied_datetimes` / `pair_landed` moved into the `transcript` module.

### Added — `pericynthion`

- **`HouseSystem` registry enum** (`label` / `slug` / `compute`).
- **`chart` module** — Angles, Lots, Node, and Lilith point computation, orchestrated by `ChartRequest` + `compute()` into a full `ComputedChart`. Nodes and Lilith are now computed for every geocentric/topocentric chart (the Ascendant gate was dropped).
- **`jzod` feature** — `ComputedChart` → `jzod::Chart` mapping, optional so the numeric core compiles without the serialization dependency.

### Added — `blackmoon`

- **`--grant-cookie-access[=browser]`** — consent-by-grant cookie import with an upfront disclosure of which store won (invariant INV-4); **`--verbose`** additionally shows each store's `__session` expiry.
- **Unified credential fall-through chain per web target**, wired to the new `astrogram` `authenticate()` entry points.

### Changed

- **`blackmoon` is now a thin wrapper** over `astrogram`'s `convert` / `transcript` / `chart` APIs, and **`starcat` is now a thin wrapper** over `pericynthion::chart` + `pericynthion::jzod` — no astronomical arithmetic or format logic remains in either binary. `starcat` also drops a redundant `--page` guard and a dead `lilith_mode` parameter.
- **LMT `utc_offset` rounds at the minute level** in `pericynthion` (matches prior `starcat` behaviour).

### Fixed — `astrotheoros`

- Capture the `__client` cookie at login so JWT refresh authenticates; cookie-imported sessions seed `__client` and force a refresh; probe via `fetch_settings` rather than a forced token refresh. Cookie-imported astro.com sessions use `--astrocom-user`/`--pass` for delete (read is cookie-gated, delete is password-gated).

### Documented

- astro.com's read/write auth asymmetry: a `cid` cookie authenticates reads, but delete re-submits the account password (`AstrocomSession::from_cid` / `delete_charts` doc comments; astrogram & blackmoon READMEs).

### Changed — workspace & packaging

- `include` publish-allowlists added to every crate manifest (tests and dev artifacts excluded); crate metadata tidied (categories, descriptions, keywords); the workspace `license-file` was dropped in favour of the SPDX `license` only.
- New `just doc` gate builds workspace docs with `RUSTDOCFLAGS="-D warnings"` and runs in the `just publish` preflight; all 13 rustdoc intra-doc-link warnings resolved to zero. Live cookie/astrotheoros tests skip cleanly when credentials/sessions are absent.

---

## [bc317a2](https://github.com/lucidaeon/mediumcoeli/compare/2f1243d7a2b8d19365dd1ff6c59a11a80f070456...bc317a221d7e71cadae83816615ff5703c24a2dd), [astrogram/0.1.3](https://github.com/lucidaeon/mediumcoeli/releases/tag/astrogram/0.1.3), [blackmoon/0.1.3](https://github.com/lucidaeon/mediumcoeli/releases/tag/blackmoon/0.1.3), [jzod/0.1.0](https://github.com/lucidaeon/mediumcoeli/releases/tag/jzod/0.1.0), [pericynthion/0.2.0](https://github.com/lucidaeon/mediumcoeli/releases/tag/pericynthion/0.2.0), [starcat/0.2.0](https://github.com/lucidaeon/mediumcoeli/releases/tag/starcat/0.2.0), 2026.06.19

### Added — `pericynthion`

- **`coords::phase` — new lunar phase module.** `lunar_phase(moon_lon_deg, sun_lon_deg) -> LunarPhase` computes the synodic arc, an 8-fold `LunarPhaseName` (NewMoon → Balsamic in 45° octants), and the 28-fold lunation day — all as pure arithmetic with no ephemeris dependency. Every GUI, web service, or WASM consumer that links `pericynthion` can now retrieve the lunar phase without reimplementing the calculation.
- **`coords::nodes::true_nn_is_retrograde(ephem, jd_tt)`** — returns whether the true North Node is retrograde at the given Julian Day by comparing positions 12 h before/after. Previously this logic existed only inline inside `starcat`.
- **`coords::lilith::true_lilith_is_retrograde(ephem, jd_tt)`** — same pattern for true Black Moon Lilith. Both functions live in the library so every consumer benefits without drift.

### Added — `jzod`

- **`LunarPhaseName` enum** (8 snake_case variants: `new_moon` … `balsamic`) and **`LunarPhase` struct** (`synodic_arc_deg: f64`, `phase: LunarPhaseName`, `lunation_day: u8`) added to `jzod::chart` and re-exported from the crate root.
- **`Chart.lunar_phase` promoted from `Option<serde_json::Value>` to `Option<LunarPhase>`.** The field is now fully typed; the wire format is unchanged for existing `null` values.

### Changed — `starcat`

- **Lunar phase appears in all three output modes.** JZOD output carries `"lunar_phase": {"synodic_arc_deg": …, "phase": "…", "lunation_day": …}` (or `null` for heliocentric / missing bodies). Text mode appends a `Lunar Phase: crescent  72.78°  day 6 of 28` line after lots. Page mode adds a lunar phase row to the right-side banner.
- **Inline retrograde math removed.** The two duplicated blocks computing true-node and true-Lilith retrograde status (one in `print_jzod`, one in `print_page`) are replaced with calls to the new library functions. No inline astronomical arithmetic remains in `starcat/src/main.rs`.

### Added — `pericynthion` (tests)

- **Standalone phase acceptance tests** in `pericynthion/tests/acceptance_refchart.rs` covering five of the seven reference charts (Adèle Haenel, Anna Freud, Lightning Strike, William Lilly, Vettius Valens). Tests assert synodic arc (±0.1°), phase name, and lunation day from the reference-chart oracle. No ephemeris required — the body positions are read from the reference fixtures.

---

## [2f1243d](https://github.com/lucidaeon/mediumcoeli/compare/db1f399811ee4731aea08b50e224dbb3b6d6836e...2f1243d7a2b8d19365dd1ff6c59a11a80f070456), [astrogram/0.1.2](https://github.com/lucidaeon/mediumcoeli/releases/tag/astrogram/0.1.2), [blackmoon/0.1.2](https://github.com/lucidaeon/mediumcoeli/releases/tag/blackmoon/0.1.2), [jzod/0.0.0](https://github.com/lucidaeon/mediumcoeli/releases/tag/jzod/0.0.0), [pericynthion/0.1.2](https://github.com/lucidaeon/mediumcoeli/releases/tag/pericynthion/0.1.2), [starcat/0.1.2](https://github.com/lucidaeon/mediumcoeli/releases/tag/starcat/0.1.2), 2026.06.19

### Added — `jzod`

- **New leaf crate — the single source of truth for the JZOD `0.0.0` open
  chart-interchange format.** Typed serde model (`JzodDocument`, `Chart`,
  `Birth`/`Datetime`/`Location`, `Placements` with `Body`/`Angle`/`Point`/`Lot`,
  `Houses`, `Zodiac`, `Ephemeris`) with `to_string_pretty` / `from_str`
  round-tripping and forward-compatible unknown-key tolerance.
- **Coordinate, UID, and time primitives.** `Sign`, `Degrees8` (fixed
  8-decimal serialization), `Position` (absolute-longitude → sign/degree/
  minute/second); deterministic `derive_uid` and random `random_uid`;
  `format_utc_offset` and `calculated_at`.
- **Published JSON Schema** (`schema/jzod-0.0.0.schema.json`, draft-2020-12)
  with an integration test that validates emitted output and the worked
  example against it. The spec (`README.md`) and a complete worked example
  (`anna_freud_radix.json`) ship inside the crate.

### Added — `jzod`, `starcat`

- **`sect` is now three-state** — `diurnal` / `nocturnal` / `unknown`
  (`unknown` when the birth time is unknown). The field is omitted entirely
  for heliocentric charts, where sect is undefined.

### Changed — `astrogram`, `starcat`

- **JZOD output is built through the shared `jzod` crate** instead of two
  divergent hand-rolled `serde_json` implementations — the format now evolves
  in one place. `astrogram`'s `util` time helpers delegate to `jzod::time`.

### Fixed — `starcat`

- **`zodiac` was emitted as the bare string `"tropical"`**; it is now the
  spec-correct object `{"name":"tropical"}`.

### Fixed — `jzod`

- **`Degrees8` deserialization** routes through `serde_json::Number` so that
  flattened placements round-trip under the `arbitrary_precision` feature
  (a `Body` with a populated `house` map previously failed to deserialize).
- **Deterministic UID hashes the year as `i16`**, preserving bit-for-bit
  identity with the prior `astrogram` `chart_uid`.
- **Sign-boundary rounding snaps sub-arcsecond noise up** to the next sign
  rather than rendering it as `29°59'59"` of the previous sign.

### Fixed — build

- **Docker images (`starcat`, `blackmoon`)** add the new `jzod` workspace
  member to their dependency-cache stage; `just docker build-no-cache`
  previously failed to load the workspace.

### Changed — docs & packaging

- **Each crate now owns its published doc as `crates/<name>/README.md`**
  (cargo auto-detects it; explicit `readme` fields removed). The
  `crates/*/docs/` symlink dirs are deleted and cross-crate links point at
  the new locations, keeping them live on crates.io.

### Changed — versions

- `astrogram`, `blackmoon`, `pericynthion`, `starcat` bumped to `0.1.2`;
  `jzod` introduced at `0.0.0`.

---

## [db1f399](https://github.com/lucidaeon/mediumcoeli/compare/7f116df4f0d1e77493dc034c28383193a6374714...db1f399811ee4731aea08b50e224dbb3b6d6836e), [0.1.1](https://github.com/lucidaeon/mediumcoeli/releases/tag/0.1.1), 2026.06.18

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

## [7f116df](https://github.com/lucidaeon/mediumcoeli/compare/19ba32d2ffb396492d481410ee41017a6949740d...7f116df4f0d1e77493dc034c28383193a6374714), [0.1.0](https://github.com/lucidaeon/mediumcoeli/releases/tag/0.1.0), 2026.06.18

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

## [19ba32d](https://github.com/lucidaeon/mediumcoeli/compare/584712ba3ce6414493f1f0ea4f997533025ef442...19ba32d2ffb396492d481410ee41017a6949740d), [0.0.1](https://github.com/lucidaeon/mediumcoeli/releases/tag/0.0.1), 2026.06.15

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

## [584712b](https://github.com/lucidaeon/mediumcoeli/compare/0a080f3534e15c52e6e3493815eb85875f08e179...584712ba3ce6414493f1f0ea4f997533025ef442), [0.0.0](https://github.com/lucidaeon/mediumcoeli/releases/tag/0.0.0), 2026.06.09

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
