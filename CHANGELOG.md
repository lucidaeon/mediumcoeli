# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [main](https://github.com/lucidaeon/mediumcoeli/compare/fc0eb42cecd5eac393bc66f1b938205d11443002...main), [astrogram/0.6.0](https://github.com/lucidaeon/mediumcoeli/releases/tag/astrogram/0.6.0), [blackmoon/0.6.0](https://github.com/lucidaeon/mediumcoeli/releases/tag/blackmoon/0.6.0), [jzod/0.7.0](https://github.com/lucidaeon/mediumcoeli/releases/tag/jzod/0.7.0), [pericynthion/0.14.0](https://github.com/lucidaeon/mediumcoeli/releases/tag/pericynthion/0.14.0), [starcat/0.13.0](https://github.com/lucidaeon/mediumcoeli/releases/tag/starcat/0.13.0), 2026.07.15

This cycle hardens both CLIs for scripting — a granular exit-code taxonomy,
`--quiet` narration gating, strict machine-output stdout purity, and richer
per-value shell completion — and threads data-source provenance from the
ephemeris reader all the way into JZOD output. It adds `starcat data fetch
bsc5`, and it relocates the remaining domain logic out of the CLIs into
`astrogram` and `pericynthion`, so those libraries now expose everything a
second consumer (a GUI, a test harness) needs without re-implementing it.

### Added — pericynthion

- **Observed data-source provenance.** `compute_with_spk` records which
  ephemeris file actually backed each body; `provenance::observed_sources`
  folds those into per-source rows (cached state + mirror URLs), and
  `Ephemeris` / `SpkEphemeris` expose their `source_path`.
- **JZOD generator + sources.** JZOD output now carries a `generator` string
  and an `ephemeris.sources` map derived from that observed provenance.
- **`bsc5` is a first-class fetchable dataset.** `datasets()` enumerates the
  BSC5 fixed-star catalogue alongside the DE entourages (new `DatasetKind`),
  and `default_horizons_dir()` resolves the `<data-dir>/horizons/` convention —
  so completion, `--list`, and slug validation all derive from one registry.
- **Placement detail and selection helpers.** `chart::Placement` /
  `sorted_placements_detailed` carry the retrograde flag (with
  `NodePoints::MEAN_RETROGRADE` / `LilithPoints::MEAN_RETROGRADE` as data),
  plus `chart::north_node_deg`, `stars::expand_notable`,
  `time::calendar::in_transition_era`, and `horizons::fetch_candidates`.
- **`CARGO_PKG_VERSION` public const** for generator provenance.

### Added — astrogram

- **`auth` module.** Web-provider credential-chain assembly (cookie → token →
  login), validation, and `WebProvider::authenticate` are library-owned;
  `AstrotheorosCredential::parse_token_triple` parses the Clerk token triple.
- **Convert and pipeline surface.** `convert::write_preserving` (preserves an
  existing SFcht file's blocks on overwrite) and `convert::without_output_file`;
  `pipeline::{record_sources, fill_fields_needed, FillSpec, accepted_slugs}`.
- **`ChartError::MissingGenerator`** replaces a panic when a JZOD generator is
  not supplied.

### Added — jzod

- **Generator provenance and an `ephemeris.sources` map** in the interchange
  model (new types + schema fields).

### Added — starcat

- **`data fetch bsc5`** downloads the BSC5 fixed-star catalogue (both mirrors)
  into the platform data root, and appears in `--list`, the bare-fetch
  guidance, and shell completion.
- **`--verbose` data-source provenance dump**, **`--ayanamsha` completion
  candidates** wired from the sidereal registry, and **per-value completion
  help** for house / zodiac / coordinate candidates.

### Added — blackmoon

- **Per-value shell-completion help** for `--from` / `--to` / `--capabilities`
  / `--grant-cookie-access` / `--generate-completion`, and the `json` and `raw`
  write-only sinks are now documented.

### Changed

- **Both CLIs are script-friendly.** Errors map through a granular exit-code
  taxonomy via `classify` (input errors exit 3, integrity failures exit 5, …);
  all human and diagnostic output goes to stderr, keeping stdout pure for
  `--to json` / JZOD / table payloads; a non-interactive machine-output run
  fails rather than prompting, and machine-mode failures list the valid values.
- **`--quiet` gates narration** on both CLIs while always keeping data-affecting
  disclosures and the result lines; for blackmoon it also suppresses the
  per-file "N charts" counts, leaving only the summary and `wrote` line.
- **starcat**: the `horizons` exit code reflects the fetch failure's root
  cause; converted charts drop the ephemeris block; and body categories follow
  the latest IAU designations (Pluto a dwarf planet; Quaoar/Orcus Kuiper-belt;
  Sedna/Gonggong trans-Neptunian).
- **blackmoon**: fill prompts are fully informed (they list accepted values);
  credential sources fall through cookie → token → login with a disclosure of
  which one authenticated; the README opens with the supported platforms.

### Fixed

- **starcat**: the named-BSC5P count reported by `catalogue` is 3,143 (it had
  double-counted the 14 non-stellar catalogue objects).
- **blackmoon**: a bad `--fill-*` value classifies as an input error (exit 3)
  and, for a stdout sink on a TTY, prompts instead of failing.
- **pericynthion**: de-duplicated the `locate()` walk in the ephemeris fallback.
- **docs**: reconciled README/rustdoc with binary behavior (`--page` samples,
  `data prod` Horizons-dir resolution, big-endian reader support, the
  always-on house-system list) and fixed broken intra-doc links.

## [fc0eb42](https://github.com/lucidaeon/mediumcoeli/compare/9627a5f3b29b4c3f6e151c4393a431412ee83f93...fc0eb42cecd5eac393bc66f1b938205d11443002), [pericynthion/0.13.0](https://github.com/lucidaeon/mediumcoeli/releases/tag/pericynthion/0.13.0), [starcat/0.12.0](https://github.com/lucidaeon/mediumcoeli/releases/tag/starcat/0.12.0), 2026.07.10

This cycle promotes the Koch (Birthplace) house system from the unverified
`noref-houses` gate to always-on, now that it carries a reference-chart oracle
and a corrected formula, and it lands a production-ready cross-platform release
pipeline: Homebrew bottles on macOS and a Scoop manifest on Windows, published
and merged automatically from a release tag.

### Added — pericynthion

- **Koch (Birthplace) house system is now always-on.** Previously gated behind
  the `noref-houses` "unverified" feature, Koch now ships in the default build:
  the enum variant, label, slug, and compute arm are ungated, and Koch joins
  `DEFAULT_SET` (seven always-on systems become eight). Its cusps are computed by
  semi-arc trisection of the MC's diurnal semi-arc (Makransky, *Primary
  Directions* (1992), p. 69) and reference-verified to within 0.01° against a
  Solar Fire Koch chart (Alan Turing) via a new acceptance test.

### Added — starcat

- **`--house koch` in the default build.** Koch is now selectable without the
  `noref-houses` feature and is reported among the always-on house systems; the
  module docs now read eight always-on, eleven remaining behind `noref-houses`.

### Changed — ci / release

- **Cross-platform release pipeline is production-ready.** The release workflow is
  reorganized into `macos-release` (builds and uploads a Homebrew bottle, then
  opens and squash-merges the tap formula PR gated on an install check) and
  `windows-release` (Scoop publish-path harness), both driven from a release tag
  and consolidated on a single `PACKAGE_PUBLISH` token.
- **`just publish` skips crates already on crates.io**, so a partial-bump release
  no longer aborts on unchanged sibling crates.

## [9627a5f](https://github.com/lucidaeon/mediumcoeli/compare/8cc6ea68d6332aa407985bc976350dd24c39ac08...9627a5f3b29b4c3f6e151c4393a431412ee83f93), [blackmoon/0.5.0](https://github.com/lucidaeon/mediumcoeli/releases/tag/blackmoon/0.5.0), [starcat/0.11.0](https://github.com/lucidaeon/mediumcoeli/releases/tag/starcat/0.11.0), 2026.07.10

This cycle makes both CLIs drivable from the environment: every `starcat compute`
input and every `blackmoon` file-conversion flag now also reads a matching
`STARCAT_*` / `BLACKMOON_*` variable (a command-line flag still overrides its
variable). It also turns starcat's previously mutually-exclusive output and
coord-format flags into a bail-free priority resolution, adds format-slug
tab-completion to blackmoon, and ships `starcat` / `blackmoon` container images to
GHCR and Docker Hub on release tags.

### Added — starcat

- **Environment-variable configuration.** Every `compute` input flag now also
  reads a flat `STARCAT_<FLAG>` variable (`--date` ↔ `STARCAT_DATE`, `--house` ↔
  `STARCAT_HOUSE`, and so on), so a chart can be driven entirely from the
  environment. A flag passed on the command line always overrides its variable;
  boolean flags take `true`/`false`. `--jpl-data` keeps its existing
  `$STARCAT_JPL_DATA` resolution.

### Added — blackmoon

- **Environment-variable configuration** for the file-conversion flags
  (`BLACKMOON_INPUTS`, `_OUTPUT`, `_TO`, `_TARGET`, `_NORMALIZE`, `_STRICT`,
  `_FILL_HOUSE` / `_FILL_ZODIAC` / `_FILL_LOCUS`, `_VERBOSE`); `BLACKMOON_INPUTS`
  is comma-split for multiple inputs. Web-only, destructive, and consent flags are
  intentionally excluded; the credential variables are unchanged.
- **Format-slug tab-completion.** `--from` / `--to` / `--target` expose every
  format-registry slug as a clap possible value, so shells tab-complete them and
  `--help` lists them; unknown values surface clap's did-you-mean suggestions.
  `FORMATS` stays the single source of truth.

### Changed — starcat

- **Output and coord-format flags no longer conflict.** `--jzod` / `--text` /
  `--page` and `--dd` / `--dms` / `--ddm` / `--dm` / `--d` used to error when
  combined; they now resolve to a single winner — a command-line flag beats an
  environment variable, and within a source tier priority is `jzod > text > page`
  and `dd > dms > ddm > dm > d`.

### Added — ci / docker

- Build and push `starcat` and `blackmoon` container images to GHCR and Docker Hub
  on release tags; images carry OCI source / description / license labels for
  registry linkage.

### Changed — docs

- README shell-completion section leads with clap's dynamic registration for
  `starcat` (`source <(COMPLETE=bash starcat)`), documenting `generate-completion`
  as the static fallback; `blackmoon` ships a static completion script.

## [8cc6ea6](https://github.com/lucidaeon/mediumcoeli/compare/35429fa3bead501b7b82ccabced12c402f75edc8...8cc6ea68d6332aa407985bc976350dd24c39ac08), [pericynthion/0.12.0](https://github.com/lucidaeon/mediumcoeli/releases/tag/pericynthion/0.12.0), [starcat/0.10.0](https://github.com/lucidaeon/mediumcoeli/releases/tag/starcat/0.10.0), [wristband/0.1.4](https://github.com/lucidaeon/mediumcoeli/releases/tag/wristband/0.1.4), 2026.07.09

This cycle replaces the code-generated JPL **oracle** with a hand-edited
`oracle.json` parsed at load, adds **entourages** (`starcat data fetch <series>`,
`de441` default, slug autocomplete) and a new **`data migrate`** command that
cherry-picks usable JPL and Horizons files out of an existing data location into
the platform data directory by copy or move, and makes DE selection
**date-aware** — preferring the smallest, most precise integration whose window
covers the chart year.

### Added — starcat

- **`data migrate`.** Brings usable ephemeris files out of an existing JPL data
  location (`--from-jpl` / `$STARCAT_JPL_DATA`) and Horizons SPKs
  (`--from-horizons` / `$STARCAT_HORIZONS_DATA`) into the platform data
  directory. Copy or move (`--copy` / `--move`, else a `c`/`m`/`q` TTY prompt),
  with a copy-on-write probe so the copy path uses no extra disk on CoW
  filesystems. Each JPL file is BLAKE3-verified against the oracle and each
  Horizons `.bsp` is validated by opening it as an SPK; truncated files are
  reported and skipped, never migrated.
- **Entourages:** `data fetch <series>` (`de441` default) with live slug
  autocomplete drawn from the oracle registry; the `de441` default rolls in the
  dwarf-perturber SPK bundle.

### Added — pericynthion

- Public data-migration engine: `MigrateItem` / `MigrateMode` / `MigratePlan` /
  `MigrateSummary` with `migrate_scan` / `migrate_apply`, the Horizons variants
  (`HorizonsMigrate*`, `horizons_migrate_scan` / `horizons_migrate_apply`), and
  a `probe_cow` copy-on-write capability probe.
- Content-verified data locators — `find_under_accepting` /
  `locate_jpl_file_accepting` — that skip a wrong-content basename twin.
- **Date-aware DE selection:** prefer the smallest, most precise DE whose window
  covers the chart year (`de_preference` in `oracle.json`), opening the exact
  entourage header + binary (handles `header.NNN_572`). The dwarf SPK bundle is
  auto-loaded at compute time; the curated platform dir loads every `.bsp`, an
  external mirror only its named bundles.

### Changed

- **pericynthion** oracle is now a hand-edited `oracle.json` parsed at load
  rather than a code-generated Rust table; `serde` / `serde_json` become
  non-optional core dependencies (the `serde` feature is retained as a no-op),
  and `horizons` no longer pulls `serde_json` separately.
- **pericynthion** provenance ranks the DE441 family (the DE441 integration and
  its `asteroids_de441` perturbers) first, so it is reported as the primary
  source with older integrations following as selectable alternates.
- **ci** GitHub Actions moved off deprecated Node 20 (`actions/checkout` v4→v7,
  `softprops/action-gh-release` v2→v3, both on Node 24); lint/build/test now run
  on every branch push.

### Removed

- **pericynthion** retired the Python oracle codegen and its CI drift job:
  `scripts/gen_oracle.py`, `scripts/oracle_manifest.tsv`, and the generated
  `oracle_data.rs` are gone, superseded by the committed `oracle.json`.

### Fixed

- **wristband** Windows `RawRow` import is gated to Windows so it is not an
  unused import (a `-D warnings` failure) on macOS/Linux; DPAPI doc comments
  backtick `CurrentUser` (clippy `doc_markdown`).

## [35429fa](https://github.com/lucidaeon/mediumcoeli/compare/a53f13a2ef1de7f8d2066fc0acfc75a9b448a4e9...35429fa3bead501b7b82ccabced12c402f75edc8), [blackmoon/0.4.3](https://github.com/lucidaeon/mediumcoeli/releases/tag/blackmoon/0.4.3), [pericynthion/0.11.0](https://github.com/lucidaeon/mediumcoeli/releases/tag/pericynthion/0.11.0), [starcat/0.9.0](https://github.com/lucidaeon/mediumcoeli/releases/tag/starcat/0.9.0), [wristband/0.1.3](https://github.com/lucidaeon/mediumcoeli/releases/tag/wristband/0.1.3), 2026.07.09

This cycle hardens the JPL **fetch-and-locate** layer and the historical-calendar
contract. `starcat data fetch` now lands in the platform data directory and
**reuses an existing mirror via copy-on-write** (reflink) instead of
re-downloading; every data lookup tolerates a flat drop-folder or a full site
mirror; each fetch prints a **capabilities readout** of what the on-disk data can
actually compute. `--calendar` becomes optional with an `auto` default — but a
hard error in the 1582-1927 Julian/Gregorian transition window, where the
recorded calendar is jurisdiction-dependent, so a chart is never silently cast in
the wrong calendar.

### Added — starcat

- **Copy-on-write `data fetch`.** Clones DE441 + `sb441` bundles from an existing
  mirror (`--jpl-data` / `$STARCAT_JPL_DATA`) via reflink (APFS `clonefile`,
  btrfs/XFS `FICLONE`, Windows ReFS), falling back to a plain copy off-CoW, and
  reaching the network only when a file is valid nowhere locally. The destination
  is always the platform data directory.
- **Post-fetch capabilities readout** and `data fetch --what`: which datasets and
  files unlock which bodies (DE441 planets, `sb441-n16` main belt, `sb441-n373`
  dwarfs/TNOs, Horizons centaurs + Albion), driven live off the placements catalog.
- `--stars notable` / `--stars all` (and a bare `--stars`) expand to the 33
  notable common-name stars; combinable with explicit names, de-duplicated.
- `horizons` output directory now defaults to `.../starcat/horizons/`.

### Added — pericynthion

- `capability` module — on-disk body-availability assessment from the placements
  catalog.
- `datafiles` module — layout-agnostic data-file location (`find_under` /
  `locate_jpl_file`): hoist to the `ssd.jpl.nasa.gov/` mirror root when one exists
  above the pointed path, otherwise search the pointed directory directly.
- `DataSource` classification on the placements catalog; copy-on-write
  clone-from-mirror in the fetcher (`FetchSummary` reflinked/copied tallies).

### Changed

- **starcat** `--calendar` is now optional, defaulting to `auto` (proleptic
  Julian before 1582-10-15, Gregorian after), but **required** for dates in the
  1582-1927 transition era, where the recorded calendar depends on jurisdiction.
  Input validation now runs before ephemeris resolution (fast-fail).
- **starcat / pericynthion** JPL data resolution is layout-agnostic — a flat
  folder of loose files or a full 200-GB site mirror both resolve; no assumption
  of the `ssd.jpl.nasa.gov/ftp/eph/.../` tree.
- **pericynthion** adds the `reflink-copy` dependency for copy-on-write cloning.

### Fixed

- **starcat** `data fetch` no longer re-downloads — nor double-nests the
  `ssd.jpl.nasa.gov/` path segment — when `$STARCAT_JPL_DATA` points into an
  existing mirror; existing BLAKE3-valid files are found and skipped.
- **starcat / pericynthion / blackmoon / wristband** all filesystem-path output
  collapses repeated separators (no stray `//`).

## [a53f13a](https://github.com/lucidaeon/mediumcoeli/compare/2e993084950c1de635d8ea0b2831b05ba8db2dcb...a53f13a2ef1de7f8d2066fc0acfc75a9b448a4e9), [blackmoon/0.4.2](https://github.com/lucidaeon/mediumcoeli/releases/tag/blackmoon/0.4.2), 2026.07.08

This cycle opens the **Windows distribution channel**: a tag-triggered workflow
builds the Windows executable, publishes it to the tag's GitHub Release, and
opens a scoop-bucket manifest bump PR — with clippy matrixed over Linux and
Windows so `cfg(windows)` code is linted before it ships. `blackmoon` picks up
the missing `--version` flag as a patch release.

### Added — CI

- **`windows-release.yml`** — on `starcat/*` / `blackmoon/*` tag pushes: builds
  the crate's `.exe` (`--release --locked`, `x86_64-pc-windows-msvc`), zips and
  sha256-hashes it, uploads it to the tag's GitHub Release, and opens a
  manifest-bump PR (version, url, hash) in the scoop bucket, worded for a
  GitHub-signed squash-merge.
- The clippy job now runs a **matrix over `ubuntu-latest` + `windows-latest`**,
  so platform-gated code (e.g. `wristband`'s per-OS cookie stores) is linted on
  a toolchain that actually compiles it.

### Changed — build / justfile

- `just ci` is now a composed macOS preflight: it first drops our own crates'
  build/clippy artifacts (dependencies kept), then chains the qa recipes —
  `fmt-check`, `lint`, `doc`, `test` — under `-D warnings`; OS-specific lints
  are left to the pipeline's matrix.
- `just doc` now passes `--document-private-items`, making the local gate a
  superset of CI's doc job.
- Removed the `publish-order` recipe (superseded by cargo's coordinated
  `publish --workspace`); `just fetch` help now lists `bsc5`.

### Fixed — `blackmoon`

- Wired the `--version` flag (mirroring `starcat`), with a test pinning it —
  the flag should have existed all along, hence patch **0.4.2**.

## [2a9542a](https://github.com/lucidaeon/mediumcoeli/compare/c27784981d38727fbfcd3bd9eef55f3a565f2f83...2a9542af2f308e7410af739bfb8e7c80111c905e), [astrogram/0.5.1](https://github.com/lucidaeon/mediumcoeli/releases/tag/astrogram/0.5.1), [blackmoon/0.4.1](https://github.com/lucidaeon/mediumcoeli/releases/tag/blackmoon/0.4.1), [pericynthion/0.10.0](https://github.com/lucidaeon/mediumcoeli/releases/tag/pericynthion/0.10.0), [starcat/0.8.0](https://github.com/lucidaeon/mediumcoeli/releases/tag/starcat/0.8.0), [wristband/0.1.2](https://github.com/lucidaeon/mediumcoeli/releases/tag/wristband/0.1.2), 2026.07.07

This cycle makes starcat **self-sufficient for a first-time user**: a new
`data fetch` command downloads the DE441 production ephemeris — resumably and
BLAKE3-verified — into a platform-native data directory, so computing a chart no
longer requires a hand-mirrored `$STARCAT_JPL_DATA`. It also lands **dynamic
`--stars` completion**, the **Gamma Velorum** fixed star, GitHub Actions CI, and
a self-cleaning Docker builder teardown.

### Added — `pericynthion`

- **`datafetch` module** (behind the new `data-dir` / `data-fetch` features):
  `default_data_dir` resolving the platform-native persistent **data** directory
  (via `directories`); a **named-dataset registry** (`Dataset`, `datasets`,
  `dataset_from_slug`) whose sole `de441` entry maps to the DE441 production
  subset; and `fetch_dataset` — a **streaming, resumable** downloader
  (`Range` / `.part`, `206`-append / `200`-restart) that verifies each file's
  BLAKE3 **before** promoting it, skips already-valid files, and retries once
  from scratch on a post-resume checksum mismatch.
- **Gamma Velorum** (γ² Vel, HR 3207) added to the notable fixed stars —
  resolvable by name or the `Regor` alias, and now `--stars` tab-completable.

### Changed — `pericynthion`

- The JPL data-resolution chain now falls back to `default_data_dir()` when
  neither `--jpl-data` nor `$STARCAT_JPL_DATA` is set, with a missing-data
  message pointing at `starcat data fetch`.
- The `NOTABLE` fixed-star list is now **strictly HR-sorted** (Agena ahead of
  Arcturus), enforced by a sort-invariant test.

### Fixed — `pericynthion`

- Corrected the documented Windows data-directory path to
  `%APPDATA%\starcat\data\` (matching `directories` v6).

### Added — `starcat`

- **`data fetch`** subcommand: downloads a dataset (default `de441`, ~2.8 GB)
  into the platform data directory with an `indicatif` progress bar, then prints
  the canonical `data verify` report; `--list` enumerates datasets.
- **Dynamic `--stars` shell completion** (clap_complete `unstable-dynamic`):
  suggests the notable fixed stars **without restricting** input (arbitrary
  BSC5P designations still parse). The static `generate-completion` script is
  retained as a fallback.

### Added — CI

- **Quality-gate workflow** (fmt, clippy `-D warnings`, doc, workspace tests,
  codegen-drift) and a **release-trigger workflow** that opens a Homebrew
  formula-bump PR on a `starcat/*` or `blackmoon/*` tag.

### Changed — build / justfile

- `just docker prune-builders` now performs a **full teardown** — removing every
  `multiarch-*` buildx builder and sweeping orphaned BuildKit containers on the
  local and current-remote daemons — and runs automatically at the tail of
  `just docker release`.

### Added — docs

- New **"Ephemeris Data: Sources and File Formats"** section in the astrologer
  domain reference: the JPL DE series, bulk SSD-FTP vs the Horizons API,
  `.441` / `.bsp` container formats, and span-vs-coverage as a correctness axis.

### Changed — workspace

- Raised the workspace MSRV to **Rust 1.96** (the CI toolchain is pinned to
  match), and cleared the lint debt the new CI's fresh `-D warnings` pass
  surfaced — `collapsible_if` → let-chains, `manual_is_multiple_of`, and
  `broken-intra-doc-links` — across `astrogram`, `blackmoon`, `pericynthion`,
  `starcat`, and `wristband`.
- Added a **`just ci`** recipe mirroring the GitHub Actions gates (`-D warnings`
  fmt / clippy / doc / test), so a clean local run predicts a green CI.

## [c277849](https://github.com/lucidaeon/mediumcoeli/compare/88e39c860240bb696c7dd2e23aeb89b51ba3df52...c27784981d38727fbfcd3bd9eef55f3a565f2f83), [astrogram/0.5.0](https://github.com/lucidaeon/mediumcoeli/releases/tag/astrogram/0.5.0), [blackmoon/0.4.0](https://github.com/lucidaeon/mediumcoeli/releases/tag/blackmoon/0.4.0), [jzod/0.6.0](https://github.com/lucidaeon/mediumcoeli/releases/tag/jzod/0.6.0), [pericynthion/0.9.0](https://github.com/lucidaeon/mediumcoeli/releases/tag/pericynthion/0.9.0), [starcat/0.7.0](https://github.com/lucidaeon/mediumcoeli/releases/tag/starcat/0.7.0), [wristband/0.1.1](https://github.com/lucidaeon/mediumcoeli/releases/tag/wristband/0.1.1), 2026.07.07

This cycle delivers the **sidereal-zodiac engine** end to end — ayanamsha-at-date
math from the numeric core through the CLI flags and the JZOD output — alongside
a **JHD (Jagannatha Hora) reader** and **directory input** in the conversion
suite, and a hard-error sweep that replaces silent fallbacks with named errors.

### Added — `pericynthion`

- **`sidereal` module** (behind the new `sidereal` feature): `sidereal_longitude`
  (tropical − ayanamsha, normalized to [0°, 360°)); frame-aware `ayanamsha_deg` —
  the **mean** frame accrues IAU 2006 general precession in longitude from a
  published epoch value, the **true** frame adds nutation in longitude;
  `AyanamshaFrame` with a per-ayanamsha intrinsic `default_frame`; and
  `project_chart`, rotating a whole `ComputedChart` into the sidereal frame.
  House cusps stay tropical — assignment is invariant under the constant shift.
- **Compiled-in ayanamsha catalog** (`AyanamshaRegistry` over the static
  `BUILTIN_AYANAMSHAS` table), primary-sourced: **Lahiri** (Indian Astronomical
  Ephemeris 2019 True Ayanamsa, definition per Calendar Reform Committee 1955;
  intrinsic frame true), **Fagan-Bradley** (Bradley's Synetic Vernal Point,
  335°57′28.64″ at epoch 1950.0; intrinsic frame mean), and **Raman**
  (fixed-annual-rate model, validated against Table IV of *Hindu Predictive
  Astrology*; intrinsic frame mean). `PrecessionModel` dispatches between the
  IAU 2006 epoch-anchor path and `FixedAnnualRate`.

### Changed — `pericynthion`

- `jzod::to_jzod_chart` gains an authoritative **`zodiac` parameter** that both
  stamps `chart.zodiac` and selects the longitude projection (tropical /
  draconic / sidereal), resolving an unrecorded sidereal frame through the
  ayanamsha's intrinsic default. The `jzod` feature now implies `sidereal`.
- Two silent fallbacks are now hard errors: an **unknown ayanamsha slug**
  (`UnknownAyanamshaSlug`, listing the known slugs) and a **draconic chart
  without a node longitude** (`DraconicNodeUnavailable`) — both previously
  emitted tropical longitudes under a wrong zodiac stamp.

### Added — `jzod`

- **`ayanamsha` module** — the canonical ayanamsha authority table (15 slugs)
  with alias resolution (`fagan_allen` → `fagan_bradley`) and canonical default
  frames; pinned against the JZOD.md slug table by test and consumed by both
  pericynthion and astrogram, with authority-coherence tests failing on drift.
- **`DraconicNode`** (Mean / True), re-exported at the crate root;
  `Zodiac::Draconic` becomes a struct variant carrying `node:
  Option<DraconicNode>` (absent = unrecorded).

### Changed — `jzod`

- `Zodiac::Sidereal.frame` is now **`Option<SiderealFrame>`** — absent means
  unrecorded rather than a fabricated default. Schema gains the `node` property
  and the optional frame.

### Added — `astrogram`

- **JHD (Jagannatha Hora) reader**: `Format::Jhd` joins the format registry
  (slug `jhd`, file kind, read-only) with a chart-file parser and
  specimen-gated structural tests.
- **`convert::chart_files_under`** — recursive chart-file discovery under a
  directory (follows file symlinks; directory symlinks are not recursed, for
  cycle safety) — and **`convert::read_path`**, which names an
  embedded-name-less chart (e.g. JHD) from its file stem.
- **Library-owned fill policy**: `pipeline::FillSpec` / `FILL_SPECS` /
  `fill_spec` (label, flag suffix, default slug, parser per non-omittable
  field) plus **`GlobalRender::apply_to`**; pin tests guard both against field
  drift.
- **`ChartError::Io`** — filesystem failures carry the path and `io::Error`
  source instead of masquerading as parse errors.

### Changed — `astrogram`

- The JZOD writer **consults the canonical jzod ayanamsha table**:
  `FaganAllen` emits the canonical `fagan_bradley` slug, Lahiri emits
  `frame: true`, other named sidereal variants emit no frame rather than a
  fabricated mean, and a Solar Fire `Other(n)` id is preserved textually as
  `other_<n>`.
- Parse-error hardening: ADB XML minute/second and `adb_id` failures propagate
  as errors naming the offending raw text (was a silent zero fallback);
  astrocom's `AafParse` carries the structured `AafError` via `source()`; the
  redundant "parse error: " display prefix is gone.

### Added — `starcat`

- **`compute --zodiac {tropical|sidereal|draconic}`**, **`--ayanamsha <slug>`**
  (default `lahiri`), and **`--ayanamsha-frame {mean|true}`** (default: the
  ayanamsha's intrinsic frame). Sidereal rotates all placements (bodies,
  angles, nodes, Lilith, lots, stars) across the JZOD, `--text`, and `--page`
  outputs; the banner discloses the resolution, e.g. `sidereal (lahiri, true)`.
  `--zodiac draconic` is equivalent to `--draconic`.

### Added — `blackmoon`

- **Directory input**: a directory named as an input is scanned recursively for
  chart files of any registered file format, with a summary line of files read
  and skipped; the resolved `--output` file is excluded from the scan so a
  prior output collection is not re-ingested on a later run.

### Changed — `blackmoon`

- `apply_fills` drives off the library `FillSpec` table; a non-omittable field
  without a spec is now a hard error instead of a silent skip. The read-only
  JHD format gets an explicit write-bail arm.

### Fixed — `wristband`

- Crate-manifest description typo ("Ins and out" → "In and out").

## [88e39c8](https://github.com/lucidaeon/mediumcoeli/compare/3e1b1e8a6538875769995b4b83565afe9db324b1...88e39c860240bb696c7dd2e23aeb89b51ba3df52), [astrogram/0.4.0](https://github.com/lucidaeon/mediumcoeli/releases/tag/astrogram/0.4.0), [blackmoon/0.3.0](https://github.com/lucidaeon/mediumcoeli/releases/tag/blackmoon/0.3.0), [jzod/0.5.1](https://github.com/lucidaeon/mediumcoeli/releases/tag/jzod/0.5.1), [pericynthion/0.8.0](https://github.com/lucidaeon/mediumcoeli/releases/tag/pericynthion/0.8.0), [starcat/0.6.1](https://github.com/lucidaeon/mediumcoeli/releases/tag/starcat/0.6.1), [wristband/0.1.0](https://github.com/lucidaeon/mediumcoeli/releases/tag/wristband/0.1.0), 2026.06.29

This cycle centres on two efforts: a **consent-gated User-Agent system** — a web
request can present the cookie-source browser's real UA, but only on explicit
opt-in — and a **CLI→library extraction** that lifts the convert pipeline, the
web-provider layer, and the placement helpers out of the CLIs into the libraries
so a future GUI shares them rather than reimplementing them.

### Added — `wristband`

- **`user_agent` module** — divines a browser's own User-Agent with no network
  and no cookie access, reading on-disk version metadata (Chromium `Last
  Version`, Firefox `compatibility.ini`, Safari app-bundle `Info.plist`) and
  interpolating a per-browser template, then reducing the result to what the
  browser actually sends. Separates the **Chromium engine** version
  (`Chrome/<major>.0.0.0`) from a derivative's **product** version:
  Chrome/Chromium/Brave/Edge derive the engine from disk, while Vivaldi/Opera/
  Whale fall back to a maintained pin; Firefox reports `<major>.<minor>` with its
  own dotted macOS token. Reads version metadata only — no cookie store is
  opened, decrypted, or copied.

### Added — `astrogram`

- **`user_agent` module** — the UA-selection policy in one place: `UaChoice`, a
  frontend-neutral `UaIntent` + `choose(grant, intent, cookie_ua)` (granting
  cookie access never implies impersonation; browser mimicry is opt-in),
  `resolve`, `ua_kind_label`, the fixed `STATIC` spoof, and a **required** typed
  `AppProduct` for the self-reported UA.
- **`provider` module** — `WebProvider` unifying Luna / Astrocom / Astrotheoros
  behind one interface (`read_existing`, `read_input`, `write_charts`,
  `fetch_all_with_ids`, `delete_one`, `fetch_global_settings`), plus
  `key`/`DatetimeKey`, `GlobalRender`, a `ProgressSink` output trait, and
  `ProviderError`.
- **`pipeline` module** — the convert engine as pure, structured-return
  functions: `drop_summary`, `fill_targets` + `apply_fill_value`, and
  `verify_rows`/`VerifyRow`.
- **`format::capability_matrix()` + `CapabilityRow`** (registry-derived support
  matrix) and a canonical **`ChartField::ALL`** guarded by a compile-time
  exhaustiveness test.
- **`decision_log::default_path`** (XDG) and **`sfcht::write_file_preserving`**.
- Per-session injectable User-Agent on Luna / Astrocom / Astrotheoros; the
  cookie-source browser UA is divined into `CredentialOutcome.cookie_ua`.

### Changed — `astrogram`

- Web sessions take a **required** User-Agent — the `Option`/default and the
  stale per-module spoof constants are gone.
- Live-test credentials are namespaced **`ASTROGRAM_*`**, decoupled from
  blackmoon's runtime `BLACKMOON_*`.

### Fixed — `astrogram`

- `cookie_ua` is divined from the **winning** cookie store's profile, not the
  allow-list filter.
- The SFcht acceptance walk is scoped to the `sfcht/` subdir (≈62 s → <1 ms).

### Added — `pericynthion`

- **`ComputedChart::sorted_placements` + `NodeVariant`** — the chart-render
  primitive: one zodiacally-sorted list of cusps/bodies/angles/nodes/lots.
- **`placements::resolve_body_id` + `omniscient_body_ids`** — sb441-preferred
  NAIF id resolution plus the covered-body set.
- **`spk::open_all_sources`** — union of explicit SPK + auto sb441 + Horizons dir.
- **`datafiles` module** — `provider_cached` + `production_file_paths`
  (disk-presence join over `Provider`).

### Added — `blackmoon`

- **`--capabilities[=text|json]`** — the format-support matrix (read/write
  direction, auth shape, per-field write loss), rendered from
  `astrogram::format::capability_matrix`.
- **`--ua`** (requires `--grant-cookie-access`): `--ua browser` mimics the
  cookie-source browser, bare `--ua` is a fixed static spoof, `--ua <string>` is
  verbatim. Every web target prints a `user-agent (<kind>): <string>` disclosure
  line before authenticating.

### Changed — `blackmoon`

- **Browser impersonation is opt-in.** A granted run with no `--ua` sends the
  self-reported UA (`Mozilla/5.0 Blackmoon/<v> Astrogram/<v>`) — granting
  cookie *read* access no longer implies UA *impersonation*.
- Consumes `astrogram::provider` behind a thin `CliSink`; `report_drops`,
  `apply_fills`, `verify_and_report`, `write_file_target`, and the decision-log
  path now delegate to the libraries — the CLI is a thin wrapper.
- Credential env vars renamed to the **`BLACKMOON_`** prefix;
  `--grant-cookie-access` is repeatable (last-wins).

### Changed — `starcat`

- Delegates to the new pericynthion primitives (`sorted_placements`,
  `resolve_body_id`/`omniscient_body_ids`, `open_all_sources`, `datafiles`) —
  behaviour-preserving.
- Corrected the `--draconic` / `--antiscia` help: both apply to the **default
  JZOD** output (not only `--text`); the only no-op mode is `--page`.

### Changed — `jzod`

- `JZOD.md` spec-doc tidy.

## [3e1b1e8](https://github.com/lucidaeon/mediumcoeli/compare/d28d3efee3375bc13bf43a270ee0f93c26518012...3e1b1e8a6538875769995b4b83565afe9db324b1), [astrogram/0.3.0](https://github.com/lucidaeon/mediumcoeli/releases/tag/astrogram/0.3.0), [jzod/0.5.0](https://github.com/lucidaeon/mediumcoeli/releases/tag/jzod/0.5.0), [pericynthion/0.7.0](https://github.com/lucidaeon/mediumcoeli/releases/tag/pericynthion/0.7.0), [starcat/0.6.0](https://github.com/lucidaeon/mediumcoeli/releases/tag/starcat/0.6.0), 2026.06.25

### Added — `pericynthion`

- **Tithi** (`coords::tithi`): a `Tithi` type and a pure `tithi()` function
  computing the 30-fold Vedic lunar day from the Moon–Sun elongation (12° steps,
  no ephemeris dependency). `ComputedChart` now carries `tithi`, populated for
  geocentric/topocentric charts and gated on Sun + Moon being present.
- **Antiscia** (`antiscia`): pure `antiscion()` and `contra_antiscion()`
  reflection involutions on ecliptic longitude (solstice axis `(180° − λ)`,
  equinox axis `(360° − λ)`).
- **Draconic projection** (`draconic`): a `draconic_longitude` uniform-shift
  function plus `DraconicChart` / `project_chart`, re-projecting every emitted
  longitude (bodies, asteroids, angles, nodes, lilith, lots, stars) about the
  lunar node.
- **Twilight grace band**: `is_twilight_chart()` flags a Night chart with the
  Sun within 6° of the ASC or 3° of the DSC; `ComputedChart::interp_sect_twilight`
  surfaces it **without** ever promoting binary `sect` to Day.
- `catalogue_provenance()` exposes the embedded BSC5 CDS ReadMe, and
  `placements::markdown()` gains a **Derived Views** section documenting
  `--draconic` and `--antiscia`.

### Changed — `pericynthion`

- The **Yale Bright Star Catalogue (BSC5)** is now baked verbatim into
  `bsc5_catalogue.rs` and parsed once via `LazyLock`. The committed `catalog.gz`,
  the `build.rs` script, and the `flate2` build-dependency are **removed** — a
  fresh `cargo test` is green with no decompression at build or run time.
- The data oracle records Harvard `ybsc5.gz` as a byte-identical alternate BSC5
  mirror (same BLAKE3 + 573 921 bytes) for provenance.

### Added — `starcat`

- **`compute --antiscia`**: appends an Antiscion / Contra-antiscion sub-table to
  `--text` output and emits `antiscion` / `contra_antiscion` in `--jzod`.
- **`compute --draconic`**: re-projects all longitudes into the draconic zodiac
  (0° = Moon's node; mean/true follows `--nodes`), printing `Zodiac : draconic`;
  applies to both `--text` and `--jzod`. Both flags default OFF and are no-ops in
  `--page`.
- A **Tithi** line in `--text` output, immediately after the Lunar Phase line.
- `catalogue --points` documents the derived Antiscion / Contra-antiscion rows.

### Added — `jzod`

- **`Tithi`** struct (`index` / `name` / `fraction`), re-exported from the crate
  root, plus an optional `Chart::tithi` field (skip-if-none; absent for
  heliocentric charts).
- Optional **`interp_sect_twilight: Option<bool>`** on `Chart`, supplementing
  `sect`: a twilight chart is `sect: nocturnal` + `interp_sect_twilight: true`.
- Optional **`antiscion` / `contra_antiscion`** `Position` fields on
  `placement::Body` and `placement::Angle`, emitted only when antiscia output is
  requested (fixed stars and lots have none by design).
- JZOD.md candidate entries documenting the tithi, antiscia, draconic, and
  twilight model additions as proposed (not yet ratified) spec.

### Added — `astrogram`

- **`Topocentric`** variant on `chart::CoordinateSystem`, threaded through every
  exhaustive match into the JZOD writer (`jzod::CoordinateSystem::Topocentric`);
  `from_str_slug` accepts `topocentric` / `topo` and `From<u8>` decodes `3`.

### Changed — `astrogram`

- SFcht has no topocentric encoding, so a topocentric chart falls back to the
  geocentric byte on write (lossy); capability membership is unaffected, since
  `ChartField::CoordinateSystem` tracks only whether a frame is stored.

## [d28d3ef](https://github.com/lucidaeon/mediumcoeli/compare/01dd8042b5b5bb0e8df0f55adf030cb556071872...d28d3efee3375bc13bf43a270ee0f93c26518012), [astrogram/0.2.3](https://github.com/lucidaeon/mediumcoeli/releases/tag/astrogram/0.2.3), [blackmoon/0.2.2](https://github.com/lucidaeon/mediumcoeli/releases/tag/blackmoon/0.2.2), [jzod/0.4.0](https://github.com/lucidaeon/mediumcoeli/releases/tag/jzod/0.4.0), [pericynthion/0.6.0](https://github.com/lucidaeon/mediumcoeli/releases/tag/pericynthion/0.6.0), [starcat/0.5.0](https://github.com/lucidaeon/mediumcoeli/releases/tag/starcat/0.5.0), [wristband/0.0.2](https://github.com/lucidaeon/mediumcoeli/releases/tag/wristband/0.0.2), 2026.06.24

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
