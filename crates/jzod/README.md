# JZOD

[![Crates.io](https://img.shields.io/crates/v/jzod.svg)](https://crates.io/crates/jzod)
[![Documentation](https://docs.rs/jzod/badge.svg)](https://docs.rs/jzod)
[![License](https://img.shields.io/crates/l/jzod.svg)](https://github.com/lucidaeon/mediumcoeli#license)

Typed model and serializer for the [JZOD](https://github.com/lucidaeon/mediumcoeli/blob/main/crates/jzod/JZOD.md) open astrology chart interchange format.

## What it does

`jzod` is the single source of truth for the JZOD wire format. It provides:

- **Typed structs and enums** for every JZOD construct: `JzodDocument`, `Chart`, `ChartType`, `Placements`, `Body`, `Angle`, `Point`, `Lot`, `Houses`, `Zodiac`, `Sect`, and more.
- **`to_string_pretty`** — serialize a `JzodDocument` to well-formed JSON with `lower_snake_case` keys.
- **`from_str`** — deserialize a JZOD JSON string back into the typed model.
- **`FORMAT_VERSION`** — the JZOD wire-format version the crate emits, distinct from the crate's own package version.
- **UID helpers** — `random_uid` and `derive_uid` for deterministic UUID v4 generation.
- **Coordinate types** — `Sign`, `Position`, `Degrees8` for zodiacal placement arithmetic.

## Why it exists

`astrogram` and `starcat` both produce JZOD output. Without a shared typed model, each would re-implement the same structs independently and diverge silently. `jzod` is a dependency-free leaf crate that lets every consumer build the typed model from its own domain types and serialize through one path.

## How it works

The crate is organized by domain:

| module | contents |
|---|---|
| `document` | `JzodDocument` — top-level `{ version, charts, relationships, views }` |
| `chart` | `Chart`, `ChartType`, `Birth`, `Datetime`, `Location`, `Ephemeris`, `Zodiac`, `Sect`, `CoordinateSystem`, `LunarPhase` |
| `placement` | `Placements`, `Body` / `BodyId`, `Angle` / `AngleId`, `Point` / `PointId`, `Lot` / `LotId` |
| `house` | `Houses`, `HouseSystemCusps`, `HouseCusp` |
| `coord` | `Position`, `Sign`, `Degrees8` — zodiacal position and coordinate arithmetic |
| `time` | `Datetime` helpers |
| `uid` | UUID v4 generation (`random_uid`, `derive_uid`) |

Serialization delegates to `serde_json`; the only dependencies are `serde`, `serde_json`, and `uuid`. The `schema/` directory contains the JSON Schema for external validators; `jsonschema` is a dev-dependency used in schema conformance tests.

See [`JZOD.md`](https://github.com/lucidaeon/mediumcoeli/blob/main/crates/jzod/JZOD.md) for the full format specification.
