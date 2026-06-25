# Medium Coeli

Astrology software written in Rust.

- **[Blackmoon](crates/blackmoon/README.md)** — chart data-format conversion. Reads and writes:

  - Files
    - Solar Fire `.SFcht` files
    - Astrodatabank XML
    - Astrolog AAF
    - Zeus

  - Web platforms
    - astro.com
    - astrotheoros.com
    - lunaastrology.com

> Blackmoon is pre-release. It should only be used by those who have successfully restored databases from backups. It is recommended to backup your Astrology databases on at least one flash drive and one cloud drive. See your Astrology software user guide for information.

- **[Starcat](crates/starcat/README.md)** — ephemeris computation and presentation. Reads NASA JPL DE441 binary files and produces ecliptic-of-date apparent positions for Astrological placements. Supports multiple geographic coordinate systems, astronomical coordinate systems, and astrological house systems.

> Starcat is also pre-release. It is fast and accurate, with support for small bodies, asteroids, and fixed stars.

## Quick Start
 - [Homebrew](https://brew.sh) recommended
 - Clone this repo

```
brew bundle
just release
export PATH="$PATH:$PWD/target/release"
```

## Blackmoon example

```
blackmoon ~/Library/Mobile\ Documents/com~apple~CloudDocs/charts/*.{SFcht,xml,zdb} --output now.sfcht
...
wrote blackmoon.20260608T224605Z.sfcht
open -a "Astro Gold.app" blackmoon.*.sfcht
```

## Starcat example
```any
just fetch de441
export STARCAT_JPL_DATA="$PWD/de441"
starcat compute --date 1895-12-03 --time 15:15:00 --calendar gregorian --tz=+01:00 --lat 48.208333 --lon=16.371667 --house=placidus

╭─────────────────────────────────────────────────╮
│ 1895.12.03 15:15 UTC+01:00     48°N12' 016°E22' │
│ Gregorian                           Topocentric │
│ Tropical                               Placidus │
╰─────────────────────────────────────────────────╯
╭─────────────────────────────────────────────────╮
│ JD UT 2413531.0938                      Diurnal │
├─────────────────────────────────────────────────┤
│ Placement │  Longitude │ Placement │  Longitude │
├───────────┼────────────┼───────────┼────────────┤
│ H1        │ 28°15' Tau │ Uranus    │ 21°30' Sco │
│ Ac        │ 28°15' Tau │ Mars      │ 23°58' Sco │
│ Pluto   ℞ │ 11°46' Gem │ H7        │ 28°15' Sco │
│ Neptune ℞ │ 16°48' Gem │ Ds        │ 28°15' Sco │
│ H2        │ 22°06' Gem │ Mercury   │  1°47' Sag │
│ Moon      │ 27°56' Gem │ Sun       │ 11°12' Sag │
│ H3        │ 10°47' Can │ Fortune   │ 14°59' Sag │
│ H4        │  0°05' Leo │ H8        │ 22°06' Sag │
│ Ic        │  0°05' Leo │ H9        │ 10°47' Cap │
│ Jupiter ℞ │  9°02' Leo │ H10       │  0°05' Aqu │
│ H5        │ 24°54' Leo │ Mc        │  0°05' Aqu │
│ Sn      ℞ │  7°46' Vir │ H11       │ 24°54' Aqu │
│ H6        │  3°40' Lib │ Nn      ℞ │  7°46' Pis │
│ Venus     │ 24°29' Lib │ H12       │  3°40' Ari │
│ Vx        │ 25°16' Lib │ Ax        │ 25°16' Ari │
│ Spirit    │ 11°31' Sco │ Eros      │ 11°13' Tau │
│ Saturn    │ 13°37' Sco │           │            │
╰───────────┴────────────┴───────────┴────────────╯
```

## Docker users

```
docker run --rm --pull always -v "${STARCAT_JPL_DATA}":/jpl:ro lucidaeon/starcat compute --date 1895-12-03 --time 15:15:00 --calendar gregorian --tz=+01:00 --lat 48.208333 --lon=16.371667 --house=placidus
```
```
docker run --rm --pull always -v "$(pwd)":/workspace:rw lucidaeon/blackmoon /workspace/adb_export_sample.xml  --output /workspace/now.sfcht
```

## Shell completion

```
eval "$(starcat generate-completion)"
eval "$(blackmoon --generate-completion)"
```


## Common tasks

```bash
just build     # cargo build
just release   # cargo build --release
just test      # cargo test --release -- --nocapture
just lint      # cargo clippy --workspace --all-targets -- -D warnings
just fmt       # cargo fmt --all
```

## NASA JPL Ephemerides

Starcat usage and tests require `STARCAT_JPL_DATA` set to a directory containing the DE441 ASCII header and binary pair. Use `just fetch de441` to mirror the JPL release. Tests skip cleanly when the env var is unset.

## License

CC0-1.0. See [LICENSE](LICENSE).

## Disclaimer

All product names, logos, and brands are property of their respective owners. All company, product and service names used in this software and documentation are for identification purposes only. Use of these names, logos, and brands does not imply endorsement.
