# Pericynthion

[![Crates.io](https://img.shields.io/crates/v/pericynthion.svg)](https://crates.io/crates/pericynthion)
[![Documentation](https://docs.rs/pericynthion/badge.svg)](https://docs.rs/pericynthion)
[![License](https://img.shields.io/crates/l/pericynthion.svg)](https://github.com/lucidaeon/mediumcoeli#license)

The ephemeris-reading and chart-generation engine behind [`Starcat`](starcat.md). A pure-Rust library that turns a JPL DE-series planetary file into apparent ecliptic positions, chart angles, lunar nodes, Black Moon Lilith, Hermetic lots, and house cusps.

## What it does

A layered numerical pipeline. Each module is independently testable and can be composed à la carte:

1. **`chebyshev`** — Clenshaw recurrence for ∑ cₖ·Tₖ(x) and its derivative. Pure math, no astronomy.
2. **`jpl`** — parses the JPL ASCII header, memory-maps the binary, and yields Chebyshev coefficient bands for any body and time interval. Auto-discovers the highest-numbered release (DE441, DE442, …) and picks the correct endianness for the host.
3. **`body`** — `Body` enum hiding DE441's internal-ordering quirks (Earth-Moon barycenter triangulation, Moon-relative-to-Earth coefficients).
4. **`time`** — Julian/Gregorian ↔ JD, the SMH 2016 cubic-spline ΔT model −720..1657 + observational table 1657..2025 + parabolic extrapolation outside, plus LMT/fixed-offset zone conversion.
5. **`coords`** — light-time iteration, annual aberration, IAU 2006 precession, IAU 2000B nutation (77 luni-solar terms), mean + true obliquity, sidereal time, the three axis modules (`acds`, `mcic`, `vxax`), lunar nodes (mean + true), Black Moon Lilith + Priapus (mean + true), WGS84 topocentric parallax, and the geo/topo/helio facades in `coords::apparent`.
6. **`houses`** — Whole Sign, Equal-from-Asc, Placidus, Regiomontanus, Porphyry.
7. **`lots`** — Hellenistic sect + the eight Hermetic lots.
8. **`geo`** — ISO 6709 DD/DMS/DDM coordinate parsing.

The single call most callers want: `coords::apparent::apparent_ecliptic_position`.

## Why it exists

`pericynthion` is a dependency-light, IAU-2006/2000B-compliant library designed so every numerical stage is small enough to audit and unit-test against NASA JPL HORIZONS oracles. It is rust native and uses current astrometric models out of the box.

## How it works

**Data flows strictly downward** through the layers above — `chebyshev` knows nothing about `body`; `body` knows nothing about `coords`. This keeps each layer's tests focused on one physical phenomenon.

**Oracles drive correctness.** Integration tests in `tests/acceptance_horizons.rs` compare results against NASA JPL HORIZONS fixtures at 0.5″ for planets and 5″ for the Moon (tightened from 20″ after the IAU 2000B upgrade). Hand-transcribed reference charts in `REFCHARTS.md` cross-check angles, lots, and house cusps for four anchor charts spanning 120 CE through the modern era.

**Naming convention.** Every named chart point uses two-letter `UPPERlower` display labels and lowercase struct fields; Ascendant uses `ac` to dodge the Rust keyword `as`. Axis modules concatenate both endpoint codes (`acds`, `mcic`, `vxax`). For computation modes: `mean ≡ average`, `true ≡ apparent ≡ osculating`, `natural ≡ interpolated`.

**v1 non-goals** (deferred to `backlog.md`): sidereal zodiacs, asteroids and dwarf planets beyond Pluto, the wider Hellenistic lot catalog, natural/interpolated Lilith, additional house systems (Koch, Campanus, Topocentric, Alcabitius), and a full IANA tzdb. The shipped surface is the v1 commitment.
