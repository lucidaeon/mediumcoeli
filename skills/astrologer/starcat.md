# Ground truth: compute with starcat, don't recall

## What this is for

This skill exists to make sound **software-development** decisions for astrology
tooling — modeling formats, catching sign-convention and calendar traps, writing
*accurate* tests and examples, validating conversions. It is **not** for
interpreting charts, and neither is `starcat`: nothing in this toolchain produces
interpretations. Both deal only in cold facts — where the bodies are, what the
geometry is. The meaning is out of scope.

When a design, test, or validation question needs a real ephemeris fact — a
body's position on a date, house cusps, a daily-motion speed, a retrograde flag,
whether a round-trip conversion preserved a value — **compute it with `starcat`
rather than recalling it from memory.** Recalled positions drift by degrees; an
LLM is not an ephemeris. `starcat` is DE441-backed and correct by construction,
and it is the free, fast, local, deterministic source of truth (no paid API,
no network at compute time). Reach for it instead of guessing.

## Readiness handshake (do this before trusting any computation)

`starcat` needs the DE441 ephemeris present on disk. Confirm it first:

1. **Is the data present?** — `starcat data provenance` reports, per body/file,
   whether it is cached locally (no network, never errors). `starcat data fetch
   --what` shows which datasets unlock which bodies.
2. **If it is absent**, point the user at `starcat data fetch de441` (one-time,
   ~2.8 GB). Do **not** fabricate positions while it is missing — say the data
   is not wired and stop.
3. **Hello-world smoke test** — a generic, non-PII moment that proves the
   pipeline computes end to end:

   ```
   starcat compute --date 2000-01-01 --time 12:00 --lat 0 --lon 0 --tz +00:00 --json
   ```

   Success is JZOD on stdout: `{"version": "...", "charts": [ ... ]}`. An error
   like "no ephemeris data" means the data is not wired — see step 2. (That
   date-and-coordinate names no person, so it is not birth data and is safe to
   run freely; see PII below.)

If `starcat` itself is not on `PATH`, it is the CLI from the `mediumcoeli`
workspace (`crates/starcat`) and may need building/installing there first.

## Common invocations

- **Positions / a chart at a moment:**
  `starcat compute --date YYYY-MM-DD --time HH:MM --lat <lat> --lon <lon> --tz <±HH:MM> [--calendar julian|gregorian] --json`.
  `--tz` (or `--lmt` with `--lon`) is required. `--calendar` is required only for
  dates in the 1582-1927 Julian/Gregorian transition era; before 1582-10-15 and
  after 1927 it defaults to `auto`. An explicit `--calendar` is always honored.
- **What can be computed** (bodies, points, stars, clusters):
  `starcat catalogue --all` (or `--bodies` / `--points` / `--stars`). The full
  catalog as Markdown: `starcat placements`.
- **Minor bodies not carried in DE441** — the centaurs (Chiron, Pholus, Nessus,
  Chariklo, Asbolus) and Albion — are on-demand Horizons SPKs:
  `starcat horizons cent` and `starcat horizons kbo`. The post-fetch
  capabilities readout and `data fetch --what` show what is present versus what
  still needs fetching.

## Boundaries and rules

- **Facts only.** `starcat` gives the sky's state; this skill supplies the
  *software* judgment (schema, correctness, tests). Neither interprets meaning.
- **Never hardcode a data path.** `starcat` resolves the ephemeris via
  `$STARCAT_JPL_DATA` / `--jpl-data` / the platform default data directory.
  Committed files — this skill included — reference those *generically*, never a
  concrete mirror path.
- **Retrograde stations are a scan, not one call:** sweep dates for the
  daily-motion speed crossing zero (stationary retrograde / stationary direct).
  There is no first-class "find stations" command yet.
- **PII:** when writing any chart data to disk, use only the reference charts in
  [fixtures/](fixtures/). An arbitrary date-plus-coordinate with no person
  attached (like the hello-world above) is not birth data and is fine.
