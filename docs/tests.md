# Test rationale & accuracy budget

This document maps the calculation regime — the ΔT model, the JPL coverage
window, and the residual physical-model error — onto the acceptance
tolerances we apply at each epoch. Each row in the chronology corresponds
to a boundary in *the math*, not to any individual reference chart.

Two things govern the per-body Moon-vs-planet tolerance split:

- **Moon** depends almost entirely on ΔT (`TT − UT`). A 1-second error in
  ΔT moves the Moon by ~13.5″ along the ecliptic. So the Moon's
  tolerance tracks the ΔT model's accuracy band, multiplied by 13.5″/s.
- **Planets** depend on precession (IAU 2006), nutation (IAU 2000B
  77-term), light-time iteration, and aberration. Their longitude
  residuals are sub-arcsecond at any epoch the JPL file covers, so the
  per-planet tolerance is dominated by transcription/refchart precision,
  not by ΔT.

See [`time/delta_t`](../crates/pericynthion/src/time/delta_t.rs) and
[`coords/apparent`](../crates/pericynthion/src/coords/apparent.rs) for the
implementations.

## Chronology of calculation regimes

Times are astronomical (year 0 exists; BCE is negative). Each row holds
*until* the next row's start year.

Units throughout the table: years are astronomical (BCE negative); the
ΔT band is in **seconds of time**, and the Moon/planet tolerance columns
are in **arcseconds of ecliptic longitude**. The 13.5″/s factor converts
the first into the second: a ±30 s ΔT band implies roughly ±400″ of Moon
longitude uncertainty (and ~30 s × planet daily motion / 86400, i.e.
sub-arcsecond on planets).

| From (year) | To (year) | ΔT model | ΔT band (seconds of time) | Moon tol (arcsec of longitude) | Planet tol (arcsec of longitude) | What changes at the boundary |
|---|---|---|---|---|---|---|
| n/a | −13200 BCE | (out of coverage) | n/a | — | — | Below the JPL file start (JD −3,100,015.5). [`record_for_jd`](../crates/pericynthion/src/jpl/reader.rs) returns `PericynthionError::Io`. |
| −13200 BCE | −720 BCE | SMH 2016 long parabolic extrapolation, secular term 32.5 s/cy² | ~10³–10⁴ s, grows quadratically | not gated — model uncertainty exceeds astrological resolution | dominated by ΔT | At −720 the SMH 2016 spline takes over from pure extrapolation. |
| −720 BCE | +400 CE | SMH 2016 cubic spline, long segments | ±30–100 s | ~90 (≈ 1.3× the model floor) | 15 | New spline knot at +400 shortens the segment length, tightens the band. |
| +400 CE | +1000 CE | SMH 2016 cubic spline | ±20–50 s | ~60 | 10 | Spline segments shorten further at +1000. |
| +1000 CE | +1500 CE | SMH 2016 cubic spline | ±10–30 s | ~30 | 5 | Pre-Gregorian-reform regime ends; segment length drops to ~50 years. |
| +1500 CE | +1657 CE | SMH 2016 cubic spline, short segments | ±5–15 s | 15 | 5 | At +1657 the spline cedes to the dense USNO/IERS observational table. |
| +1657 CE | +2025 CE | USNO/IERS observational table, linear interpolation between decade entries | ±2–3 s | 5 | 2 | Tightest possible — directly measured ΔT. End of the observational table at 2025. |
| +2025 CE | +2050 CE | Espenak/Meeus 2006 quadratic, anchored at 2000 | ±5–10 s, growing | ~15 | 5 | At +2050 a linear correction term comes on. |
| +2050 CE | +2150 CE | Espenak/Meeus 2006 quadratic + linear correction | ±20–60 s | ~60 | 15 | At +2150 the long-term parabolic takes over. |
| +2150 CE | +17191 CE | SMH 2016 long-term parabolic, secular term 32.5 s/cy² | grows quadratically with (year − 1820)² | not gated — same reason as deep antiquity | dominated by ΔT | Above the JPL file end (JD +8,000,016.5). |
| +17191 CE | n/a | (out of coverage) | n/a | — | — | `record_for_jd` returns `Io`. |

### Heliocentric VECTORS oracle

The heliocentric tests are framed differently: they don't depend on ΔT at
all (the input is `JD_TT` directly), and the oracle is HORIZONS's
**geometric J2000/ICRF position** rather than the apparent ecliptic
output. The residual is dominated by two effects:

| Source | Magnitude (arcsec of longitude) | Affects |
|---|---|---|
| Chebyshev-evaluator differences between our reader and HORIZONS's | ~5–10 on inner planets, < 1 on outer | Mercury, Venus, Mars |
| `Body::Earth` derivation (EMB − Moon/EMRAT) vs HORIZONS body 399 (Earth geocenter) | < 7 typically | Earth, Moon only |

So heliocentric tolerances (all arcseconds of longitude) are: **30** on
Earth/Moon (oracle-mismatch allowance), **10** on inner planets (at the
interpolation floor), **5** on outer planets.

## Why this matters

A test passing at 0.1″ tells us no more than a test passing at 2″. The
tolerance exists to **catch regressions**, not to advertise precision.
The "tol" columns above are sized as follows:

- **Modern era (post-1657)**: 3–10× headroom over the observed residual,
  so a small change in nutation coefficients or precession constants
  won't cause false failures, but a real bug (forgotten light-time,
  double-applied aberration, dropped ΔT) shows up immediately.
- **Pre-1657 (spline era)**: 1.5–3× headroom over the ΔT model band. The
  Moon tolerance is the ΔT band × 13.5″/s, with a small fudge for the
  short refchart-transcription quantization (refchart prints arcseconds).
- **At-the-model-floor windows** (−720 to +400 Moon, future Mercury
  interpolation): 1× headroom — i.e. the tolerance *is* the model floor.
  Tightening these would cause spurious failures with no diagnostic value.

## Test layout

| Crate | File | Layer it covers |
|---|---|---|
| `pericynthion` | [`acceptance_horizons.rs`](../crates/pericynthion/tests/acceptance_horizons.rs) | Apparent geo/topo/helio positions against NASA JPL HORIZONS. **The source of record for accuracy claims.** Fixture families: `_geo.json`, `_topo.json`, `_helio.json`. |
| `pericynthion` | [`acceptance_refchart.rs`](../crates/pericynthion/tests/acceptance_refchart.rs) | Asc/MC/cusps + body positions against an independent reference oracle. Carries its own ΔT model — divergences are documented inline and absorbed by the chronology bands above. |
| `pericynthion` | [`binary_de441.rs`](../crates/pericynthion/tests/binary_de441.rs), [`header_de441.rs`](../crates/pericynthion/tests/header_de441.rs) | DE441 binary + ASCII header invariants (32-day granule, 15 columns, etc.). |
| `pericynthion` | [`ephemeris_de441.rs`](../crates/pericynthion/tests/ephemeris_de441.rs) | Chebyshev coefficient → position/velocity round-trip per body. |
| `pericynthion` | [`helio_topo_de441.rs`](../crates/pericynthion/tests/helio_topo_de441.rs) | Frame-plumbing sanity (Sun at heliocentric origin, Earth at ~1 AU). |
| `pericynthion` | [`retrograde.rs`](../crates/pericynthion/tests/retrograde.rs) | Velocity-based retrograde-sign computation. |
| `pericynthion` | [`time_parse.rs`](../crates/pericynthion/tests/time_parse.rs) | Civil-date parser, calendar choice, UT/zone conversion, Unix-epoch round-trip. |
| `astrogram` | various | Format parsing and round-tripping for chart-database files (SFcht, Zeus, ADB XML, Luna). Not ephemeris tests. |
| `starcat` | [`cli_compute.rs`](../crates/starcat/tests/cli_compute.rs) | End-to-end CLI: spawn the binary, decode JSON output, compare against the refchart oracle. Exercises the *whole* surface — clap → civil-time + zone → `JD_UT` → ΔT → `JD_TT` → JPL discovery → ephemeris + house pipeline → JSON. |
| `blackmoon` | [`exit_codes.rs`](../crates/blackmoon/tests/exit_codes.rs), [`streams.rs`](../crates/blackmoon/tests/streams.rs), [`dir_source.rs`](../crates/blackmoon/tests/dir_source.rs), [`quiet.rs`](../crates/blackmoon/tests/quiet.rs) | CLI integration: exit codes, stdin/stdout streaming, directory sources, quiet mode — plus inline unit tests. |

## Running

```
cargo test                          # all unit + acceptance
cargo test --all-features           # incl. noref-houses-gated systems
cargo test -- --nocapture           # show the per-chart Δ table
STARCAT_JPL_DATA=/path/to/de441/    # required for ephemeris tests
ASTRO_SPECIMENS=/path/to/sfcht/     # required for sfcht-records golden test
```

Tests that need `STARCAT_JPL_DATA` or `$ASTRO_SPECIMENS` print a one-line
skip message and pass cleanly when the env var is unset.
