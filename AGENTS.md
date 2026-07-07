 # mediumcoeli — agent instructions

Project-specific rules for AI agents working in this repository.

---

Use test driven design.

Use best practices and simple architecture.

Always consult the local source code for information about Rust dependencies, which is guaranteed to
be up-to-date for the correct version.

Run `cargo path NAME` from inside a given crate directory to find the source directory for a dependency.

Blackmoon and Starcat are thin CLI wrappers to the libraries they leverage. Do not put anything in
the CLI tools that would have to be repeated by a GUI tool using the same libraries. 

Committed code must only point at committed files.

# Definitions

Accurate: Documentation is a promise to the world. It needs to reflect the reality of the code base.
If this is not the case, flag it and prepare options to bring the two into convergence.

Documentation: includes usage information and shell completion generation (clap-derived) domain-specific markdown files, in documentation generator sources, inline code comments, and/or the changelog as appropriate.

Documented: Something is considered documented when the what, why and and how have been succinctly and accurately captured in documentation. $ASTRO_RESEARCH/*.md are evaluated to see if anything has become stale.

Done: is defined as written, tested, and documented. 

In Pericynthion's catalogue: Pericynthion knows about a celestial body or object. This holds regardless of whether the codebase has constants or metadata wired up for it, and regardless of whether the required ephemeris data is present to compute its position in a chart. We know where it is, or know **how** to know where it is.

Normalize (record fields): Read records, replace any non-cp1252 chars with a cp1252 equivalent, and
write the record back in situ.

# Skills

Always use the /astrologer skill or integrated [Astrologer skill](./skills/astrologer/SKILL.md). 

Offer to use superpowers:writing-plans for changes beyond the scope of a single file.

Offer to use /changelog-generator to capture meaningful changes.

# Resources

## References
Reference charts can be found at `$ASTRO_RESEARCH/ref_*.md`
Reference details may be converted in to tests or examples as needed.

## Specimens
Real-world specimens are resolved from `$ASTRO_SPECIMENS` (subdirs `sfcht/`, `zdb/`, `adb/`)
These specimens are assumed to contain PII and as such the particulars contained within may not be brought in under version control.

# Test corpus

Test suites may use references or specimens only if they adhere their respective usage conditions. 
Always validate provenance of natal chart data when writing or updating test.
Acceptance tests may skip cleanly if $ASTRO_RESEARCH or $ASTRO_SPECIMENS are unset.

## Starcat (`crates/pericynthion` + `crates/starcat`)

Test against reference charts.

## Blackmoon (`crates/astrogram` + `crates/blackmoon`)

When focusing on a web endpoint, ensure all CRUD operations are well modelled and documented.

## Roadmap

Deferred work and larger work items are captured by (superpowers) /writing-plans 
- Every deferred item **must** carry a complexity estimate.
- Use a simple three-point scale: **S** (hours), **M** (days), **L** (weeks+).
- Place the estimate at the end of the section heading line, e.g.
  `### sqlite write — M`
- When adding or updating a roadmap item, assign an estimate; do not leave it blank.
- As items from the roadmap are completed, ensure they are documented. Check them off in the plan file as they are accomplished.

# Commands

```bash
just build          # cargo build --workspace
just release        # cargo build --release --workspace
just test           # cargo test --release --workspace -- --nocapture
just test astrogram # test a single crate
just lint           # cargo clippy --workspace --all-targets -- -D warnings
just lint-narrow    # lib + bin only (no tests/examples)
just fmt            # cargo fmt --all
just fmt-check      # verify formatting without writing
just fetch de441    # mirror JPL DE441 ephemeris (required for pericynthion/starcat tests)
just fetch adbxml   # download ADB sample XML (for astrogram parser tests)
just fetch horizons # refresh all HORIZONS reference fixtures
```

`scripts/horizons_fetch.py` calls the public NASA JPL HORIZONS API directly and
may be invoked as needed for reference positions (planets and minor bodies, by
HORIZONS designator — asteroids use a trailing `;`, e.g. Ceres `1;`, Vesta `4;`).
It caches results as `crates/pericynthion/tests/fixtures/horizons_*.json`. Use it
to generate/refresh acceptance fixtures rather than hand-entering ephemeris values.

Run a single test by name:
```bash
cargo test --release -p astrogram rsc_parses_single_entry -- --nocapture
```

## Environment variables

| Variable | Required by |
|---|---|
| `STARCAT_JPL_DATA` | `pericynthion` + `starcat` tests — path to DE441 binary dir; tests skip cleanly if unset |
| `ASTRO_SPECIMENS` | blackmoon/astrogram specimen tests — path to `sfcht/`, `zdb/`, `adb/` subdirs; tests skip cleanly if unset |
| `ASTRO_RESEARCH` | reference chart docs (`ref_*.md`); acceptance tests skip cleanly if unset |

Credentials for web targets are read from env vars with a `BLACKMOON_` prefix (`BLACKMOON_ASTROTHEOROS_USER`, `BLACKMOON_ASTROTHEOROS_PASS`, `BLACKMOON_ASTROTHEOROS_TOKEN`, `BLACKMOON_ASTROCOM_TOKEN`, `BLACKMOON_ASTROCOM_USER`, `BLACKMOON_ASTROCOM_PASS`, `BLACKMOON_LUNA_TOKEN`).

## Workspace layout

| Crate | Role |
|---|---|
| `pericynthion` | Ephemeris library — JPL DE441 binary reads, coordinate transforms, house systems |
| `starcat` | CLI — ephemeris computation and table presentation |
| `astrogram` | Chart format conversion library |
| `blackmoon` | CLI — reads any format, merges, writes any format; wraps `astrogram` |
| `jzod` | JZOD v0.0.0 typed model — single source of truth for the chart interchange format; `astrogram` and `starcat` build and serialize through it; also carries the canonical ayanamsha slug, alias, and default-frame table (`jzod::ayanamsha`) |
| `wristband` | Consent-gated, domain-scoped reader for the user's own browser session cookies; structural no-harvester posture — see `crates/wristband/SECURITY.md` |

# astrogram architecture

## Canonical chart type

`chart::Chart` is the single in-memory pivot. Every reader produces one; every writer consumes one. **Sign conventions are resolved at the format boundary, never inside `Chart`.**

- `Longitude` / `Latitude` use ISO 6709 (East positive, North positive).
- `SFcht` files store `+West` longitude and `+West` tz offset — both are negated on read and negated again on write.
- `year` is `i16` (negative for BCE).
- `month` is 1-indexed throughout `Chart`. The astrotheoros.com API uses 0-indexed months (JS `Date.getMonth()`); conversion happens in `entry_to_chart` (+1 on read) and `chart_to_create_body` (−1 on write).

## JZOD writer

`astrogram/src/jzod.rs` implements the JZOD output format for `astrogram`. It maps each `astrogram::chart::Chart` to a `jzod::Chart` and delegates all serialization to the `jzod` crate (`jzod::to_string_pretty`). The `jzod` crate is the single source of truth for the JZOD typed model; `astrogram` and `starcat` both build their JZOD output by constructing `jzod::Chart` values and calling into it.

## Format registry

`format::Format` is an enum; `format::FORMATS` is the static registry. Each `FormatSpec` carries:
- `slug` — drives CLI flag names, env var names, and `from_slug` parsing
- `kind` (`File` | `Web`) and `auth` (`None` | `Token` | `LoginOrToken`)
- `read_caps` / `write_caps` — `CapabilitySet` documenting which `ChartField`s survive

Adding a new format means: add a `Format` variant, add a `FormatSpec` row to `FORMATS`, and implement `READ_CAPS`/`WRITE_CAPS` constants beside the parser. Tests in `format.rs` enforce the registry invariants.

## Capability / loss tracking

`capability::ChartField` enumerates fields that vary across formats. `CapabilitySet` wraps a `&'static [ChartField]`. The `lost_fields` and `fill_fields` functions compute what data a specific source→sink conversion loses or needs filled. Only `SFcht` persists `HouseSystem`, `Zodiac`, `CoordinateSystem`, and `SubCharts`; web formats store none of these per-chart.

## Transcript / readback

After writing to a web sink, blackmoon calls `transcript::diff(source, landed, field_notes)` to produce per-field `FieldMapping` (Preserved / Transformed / Dropped / Filled). For astrotheoros, house system and zodiac are account-wide globals, so `fetch_global_settings()` returns a `GlobalRender` that is folded into the landed chart before diffing, with `field_notes` tagging those fields as `"global setting"`.

How `landed` is obtained depends on the provider (`WebProvider::verifies_inline()`):

- **Inline (astrotheoros):** the `POST /api/chart` response echoes the full landed entry (shape-identical to a readback entry), so `create_one` returns the `ApiChartEntry` and each chart is diffed and printed the instant it lands — no separate readback. `write_charts` drives this via a per-record `on_landed(new_index, total_new, source, landed, status)` callback; account globals are fetched once up front.
- **Post-readback (luna / astrocom):** their create responses do not echo the full entry, so blackmoon shows transient write progress, then reads all charts back via `fetch_all_with_ids()` and diffs (see `verify_and_report`).

## Consolidation

`consolidate::merge` deduplicates across multiple input batches (first-seen-wins). The `is_candidate` rule for interactive dedup: date equal, time within ±2 h, lat/lon each within 0.1°. `group_candidates` clusters all candidates transitively via union-find. The interactive `--consolidate` flow is in `blackmoon/src/consolidate_ui.rs`; decisions are persisted to a JSONL decision log so interrupted runs can resume.

## astrotheoros.com CRUD (astrotheoros module)

Authentication uses Clerk's two-step flow:
1. `POST {CLERK_URL}/v1/client/sign_ins` — email identifies, returns `sign_in_id`.
2. `POST {CLERK_URL}/v1/client/sign_ins/{id}/attempt_first_factor` — password verifies, returns JWT + `session_id` + `__client_uat` cookie.

The JWT expires every 60 seconds. `AstrotheorosSession` auto-refreshes it (via `POST {CLERK_URL}/v1/client/sessions/{id}/tokens`) when fewer than 20 seconds remain. `from_jwt` constructs a session from existing credentials without a network call (useful for tests).

**Read:** `GET {BASE_URL}/app` with `rsc: 1` header returns Next.js RSC wire format — newline-delimited `<hex>:<json>` lines. `parse_rsc_response` finds the line containing `"charts":[`, strips the `$D` date prefix, maps `"$undefined"` to `null`, then deserializes `Vec<ApiChartEntry>`. `parse_rsc_settings` extracts the `settings` object from the same payload to get account-wide house system and zodiac.

**Create:** Atlas lookup first — `GET {BASE_URL}/api/atlas?time=<unix_ms>&latitude=…&longitude=…` returns the historical IANA timezone and UTC offset for the birth location. Then `POST {BASE_URL}/api/chart` with the `{"data": {...}}` body built by `chart_to_create_body`. The response's `entry` object is the full landed chart (same shape as a readback entry); `create_one` deserializes it to an `ApiChartEntry` and returns it (use `.id` for just the UUID). This is what enables inline verification — see *Transcript / readback*.

**Delete:** `DELETE {BASE_URL}/api/chart` with `{"data": {"id": uuid}}`.

**Month convention:** `ApiChartEntry.month` is 0-indexed; `Chart.month` is 1-indexed. Both conversion points (`entry_to_chart`, `chart_to_create_body`) have tests asserting the offset.

## blackmoon CLI (blackmoon/src/main.rs)

The `WebProvider` enum in `providers.rs` adapts Luna / Astrocom / Astrotheoros behind a common interface (`read_existing`, `read_input`, `write_charts`, `fetch_all_with_ids`, `delete_one`, `fetch_global_settings`).

`cmd_convert` pipeline:
1. Read existing output target (read-before-write dedup).
2. Read input sources, tagging each chart's `Format` in `source_of`.
3. `consolidate::merge_reporting` across all batches.
4. Optional `--normalize` pass.
5. `report_drops` — disclose fields the sink cannot store; `--strict` aborts.
6. `apply_fills` — resolve house/zodiac/locus for charts whose source never carried them (ADB→SFcht case). Fill values come from flags (`--fill-house`, etc.) or interactive TTY prompt.
7. Write. For web sinks, optionally `verify_and_report` (read back + transcript).

Readback pairing uses `temporal_key` (year/month/day/hour/minute/second) not name, so renamed charts still pair correctly. When multiple charts share a birth datetime, pairing falls back to input order with a warning.

## SFcht binary format

Solar Fire stores strings as cp1252, longitude and tz offset as `+West` f32 LE, year as i16 LE. The record layout is documented in a comment at the top of `sfcht.rs`. Sub-chart blocks follow the main record; notes are a length-prefixed cp1252 blob at the end.

## Test implementation

- Inline `#[test]` modules live inside the source file for unit-level tests; integration tests live in `tests/` directories.
- The `test_support` module (astrogram) and fixture JSON files (pericynthion) provide shared helpers and static inputs.

