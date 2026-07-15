# Pericynthion

[![Crates.io](https://img.shields.io/crates/v/pericynthion.svg)](https://crates.io/crates/pericynthion)
[![Documentation](https://docs.rs/pericynthion/badge.svg)](https://docs.rs/pericynthion)
[![License](https://img.shields.io/crates/l/pericynthion.svg)](https://github.com/lucidaeon/mediumcoeli#license)

The ephemeris-reading and chart-generation engine behind [Starcat](https://github.com/lucidaeon/mediumcoeli/blob/main/crates/starcat/README.md). A pure-Rust library that turns a JPL DE-series planetary file into apparent ecliptic positions, chart angles, lunar nodes, Black Moon Lilith, Hermetic lots, and house cusps.

## What it does

A layered numerical pipeline. Each module is independently testable and can be composed à la carte:

1. **`chebyshev`** — Clenshaw recurrence for ∑ cₖ·Tₖ(x) and its derivative. Pure math, no astronomy.
2. **`jpl`** — parses the JPL ASCII header, memory-maps the binary, and yields Chebyshev coefficient bands for any body and time interval. Auto-discovers the highest-numbered release (DE441, DE442, …) and detects each file's byte order (little- or big-endian), byte-swapping on read.
3. **`body`** — `Body` enum hiding DE441's internal-ordering quirks (Earth-Moon barycenter triangulation, Moon-relative-to-Earth coefficients).
4. **`time`** — Julian/Gregorian ↔ JD, the SMH 2016 cubic-spline ΔT model −720..1657 + observational table 1657..2025 + parabolic extrapolation outside, plus LMT/fixed-offset zone conversion.
5. **`coords`** — light-time iteration, annual aberration, IAU 2006 precession, IAU 2000B nutation (77 luni-solar terms), mean + true obliquity, sidereal time, the three axis modules (`acds`, `mcic`, `vxax`), lunar nodes (mean + true), Black Moon Lilith + Priapus (mean + true), WGS84 topocentric parallax, and the geo/topo/helio facades in `coords::apparent`.
6. **`houses`** — the always-on default set (Whole Sign, Equal-from-Asc, Placidus, Regiomontanus, Porphyry, Alcabitius, Morinus, Koch), plus a further eleven systems behind the `noref-houses` feature (Campanus, Meridian, Equal-from-MC, Horizontal, Topocentric, Krusinski, Sripati, Vehlow, Carter, Pullen SD, Pullen SR).
7. **`lots`** — Hellenistic sect + the eight Hermetic lots.
8. **`geo`** — ISO 6709 DD/DMS/DDM coordinate parsing.

Higher-level modules compose that pipeline into chart-domain APIs: **`chart`** (`compute` / `compute_with_spk` → `ComputedChart`, plus `sorted_placements` for renderers), **`placements`** (the body catalog: `CATALOG`, `find_by_slug`, `resolve_body_id`, `omniscient_body_ids`), **`spk`** (asteroid SPK ephemeris + `open_all_sources`), **`stars`** / **`bsc5_catalogue`** (baked fixed-star catalog, `galactic_center`), **`antiscia`** / **`draconic`** (pure reflection / projection), and **`datafiles`** / **`provenance`** (filesystem-aware data resolution over a pure provenance schema).

The single call most callers want is `chart::compute` (or `chart::compute_with_spk` to include asteroids); `coords::apparent::apparent_ecliptic_position` is the lower-level building block underneath it.

## Why it exists

`pericynthion` is a dependency-light, IAU-2006/2000B-compliant library designed so every numerical stage is small enough to audit and unit-test against NASA JPL HORIZONS oracles. It is rust native and uses current astrometric models out of the box.

## How it works

**Data flows strictly downward** through the layers above — `chebyshev` knows nothing about `body`; `body` knows nothing about `coords`. This keeps each layer's tests focused on one physical phenomenon.

**Oracles drive correctness.** Integration tests in `tests/acceptance_horizons.rs` compare results against NASA JPL HORIZONS fixtures at 0.5″ for planets and 5″ for the Moon (tightened from 20″ after the IAU 2000B upgrade). Curated reference charts cross-check the angles, lots, and house cusps for anchor charts spanning from antiquity through the modern era.

**Naming convention.** Every named chart point uses two-letter `UPPERlower` display labels and lowercase struct fields; Ascendant uses `ac` to dodge the Rust keyword `as`. Axis modules concatenate both endpoint codes (`acds`, `mcic`, `vxax`). For computation modes: `mean ≡ average`, `true ≡ apparent ≡ osculating`, `natural ≡ interpolated`.

### Data manifest & provenance

`src/jpl/oracle.json` (hand-edited, validated by `just oracle-check`, baked in
via `include_str!`) is the monolithic manifest of known data files. Each
directory carries a `SourceKind` (`JplMirror`, `CdsCatalog`) and each file an
optional `provides` list (the catalogued bodies it backs, or `@fixed-stars`).
The `provenance` module joins this with `placements::CATALOG` — and synthesizes
per-body Horizons SPK providers — to answer "what data backs this body, where
from, is it cached?" The integrity (`data verify`) path reads only `JplMirror`
rows, so its behavior is unchanged.
