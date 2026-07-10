# JPL ephemeris mirror: file tree and what starcat needs

## Purpose

This document describes the **static JPL ephemeris mirror file tree** as it is
laid out under `ssd.jpl.nasa.gov/ftp/eph/`, catalogues the file families and
formats it contains, and answers two questions precisely:

> Of the many files in this tree, which can starcat actually use, and how are
> they related?

That answer informs `starcat data fetch` and `starcat data migrate`, which bring
the **usable** files into starcat's default data directory. Because the tree is
large (hundreds of GB), they must select files **by format and role**, not by
name alone.

This doc deliberately does **not** re-derive two adjacent topics:

- **DE version genealogy** — the 440/441 and 430/431 standard/long-range
  sibling pairs, the `t` / `_572` / `linux_` / `lnx` naming conventions, and how
  DeltaT is handled outside these files (the "which DE is which and why" story).
- **The Horizons on-demand SPK fetch path** (Type-21 MDA files, the
  `20000000 + MPC` NAIF id scheme, per-body `.bsp` storage) — a *different* data
  source from this static mirror: Horizons is fetched live per body; this mirror
  is a bulk static archive. The asteroid SPKs discussed below (`sb441-*`) are
  the mirror's Type-2 Chebyshev bundles, not Horizons Type-21 output.

Everything here concerns the mirror **file tree, formats, and file selection**.

---

## Mirror layout

The mirror root is `ssd.jpl.nasa.gov/`. Ephemeris data lives under
`ssd.jpl.nasa.gov/ftp/eph/`, with four top-level subtrees:

- **`planets/`** (~64 GB) - the JPL DE-series planetary and lunar ephemerides:
  positions of the Sun, Moon, and planetary-system barycenters. This is where
  the file starcat's core reader consumes lives (the DE441 little-endian binary).
  It holds the *same* ephemeris data packaged in several formats (ASCII source,
  platform binaries, SPICE SPK, reader source code), plus documentation.

- **`small_bodies/`** (~19 GB) - asteroid and minor-body ephemerides. The
  natal-relevant part is `asteroids_de441/` (and the older `asteroids_de430/`):
  SPICE SPK bundles giving the trajectories of the small bodies used as
  perturbers in the DE integrations. The remaining subdirectories are individual
  mission targets (Apophis, comet 67P, the DART impact target, OSIRIS-REx's
  Bennu, comet Siding Spring) - not natal bodies.

- **`satellites/`** (~118 GB) - planetary-moon system ephemerides (Jupiter's,
  Saturn's, Uranus's, Neptune's moons, plus Mars moons). These give moon
  positions relative to their host planet. Not natal chart bodies; starcat does
  not use them.

- **`spacecraft/`** (~1.6 GB) - probe/mission trajectories (Cassini, Voyager 1/2,
  Mariner, Viking, Phobos-2, etc.). Not natal chart bodies; starcat does not
  use them.

Only `planets/` and the `asteroids_de441/` portion of `small_bodies/` are
relevant to starcat.

---

## `planets/` formats

The planetary ephemeris exists once as a numerical integration; JPL then ships
it in several **packagings of the same coefficient data**. The ASCII coefficient
files are the *source of record*; the platform binaries are endian-specific
packings of those same coefficients; the SPK files are a portable SPICE re-pack;
the rest is reader code and documentation.

Endianness matters because the binaries are raw little-endian or big-endian
IEEE doubles. starcat reads **only** little-endian (`Linux/`) binaries.

| Subdir | Format / purpose | Endianness | starcat uses |
|--------|------------------|-----------|--------------|
| `Linux/` | Little-endian DE binaries: paired `header.NNN` + `linux_*.NNN` coefficient file. Directly memory-mapped by starcat's DE reader. | little-endian | **YES** (DE441 only) |
| `SunOS/` | Same DE data, big-endian packing (`xnp_*.NNN`), for legacy SPARC/PowerPC. | big-endian | no (wrong endianness; reader rejects) |
| `ascii/` | The source coefficients as text: `header.NNN` + `ascp*.NNN` (forward JD) / `ascm*.NNN` (negative JD) blocks. Users convert these to a platform binary. This is the *origin* the `Linux/`+`SunOS/` binaries are built from. | text | no (redundant source; also huge) |
| `bsp/` | SPICE SPK (`.bsp`) re-pack of the DE data - portable across platforms, read via the SPICE toolkit. A different reader path than starcat's native DE reader. | portable | no |
| `bpc/` | Binary planetary **constants** kernel (`.bpc`) plus a text frame kernel (`.tf`): the Moon's orientation (lunar libration Euler angles, PA-to-ME frame). Orientation, not body positions. | portable | no |
| `nio/` | NAVIO (`.ftp`/`.nio`) files - a JPL-internal interplanetary-navigation binary format usable only with JPL's own navigation software. | internal | no |
| `fortran/` | Reader/converter **source code** (`asc2eph.f`, `testeph.f`, `userguide.txt`) - the reference Fortran to turn ASCII into a binary and self-test it. Code, not data. | n/a | no |
| `ioms/` | Interoffice-memo **PDFs** documenting individual DE releases. Documentation. | n/a | no |
| `stations/` | Ground-station coordinates (`dsn.itrf93`, the Deep Space Network in ITRF93). Not ephemeris body data. | n/a | no |
| `test-data/` | Test-point printouts (`testpo.NNN`) for validating a reader against known positions, plus a few small sample sets. Validation aid, not chart data. | text | no (starcat's DE441 testpo ships inside `Linux/de441/`, below) |

### DE versions present per format

Not every DE series is present in every format, so file selection keys on
**format and role**, not on the DE number in the filename:

- **`Linux/`** (little-endian, what starcat reads): de102, de200, de202, de403,
  de405, de406, de410, de413, de414, de418, de421, de422, de423, de430, de430t,
  de431, de440, de440t, **de441**.
- **`SunOS/`** (big-endian): only through **de423** - there is no de440 or de441
  in this format at all.
- **`ascii/`**: the widest set (29 DE variants, de102 through de441 including
  many `t` and interim releases). `ascii/de441/` alone is ~8.6 GB of text - the
  binary it produces (`Linux/de441/`) is only 2.6 GB.
- **`nio/`**: many DEs including a `de441.ftp`, in the JPL-internal NAVIO format,
  usable only with JPL navigation software.

The same DE number therefore appears in formats that are the source (`ascii/`),
wrong-endian (`SunOS/`), or a different reader path (`bsp/`, `nio/`). Selection
targets the little-endian `Linux/` binary specifically - see *What starcat
needs* below.

### The one directory starcat's DE reader needs

`planets/Linux/de441/` contains exactly:

- `header.441` - the ASCII header (~22 KB) describing record layout, constants,
  and the 15 Chebyshev item triplets (Mercury ... Sun, nutations, librations,
  TT-TDB). starcat's discover logic looks for a plain `header.NNN`.
- `linux_m13000p17000.441` - the little-endian binary coefficient file
  (~2.6 GB), spanning the full DE441 window (-13200 to +17191). This is the file
  starcat memory-maps and evaluates for planet/Sun/Moon positions.
- `testpo.441` - JPL's test-point printout (~23 MB) for validating a reader's
  output against known JPL positions.

starcat's file discovery (in the DE reader) recognizes the `header.NNN` +
`linux_*.NNN` pair specifically. It does **not** recognize the older `lnx*` /
`header.NNN_572` naming used by DE430/431 and earlier without symlinks - another
reason `migrate` should target the DE441 (`linux_` / plain-header) layout rather
than pattern-matching across DE generations.

---

## `small_bodies/`

### `asteroids_de441/` - the asteroid perturber SPKs

These are SPICE **DAF/SPK** (`.bsp`) files: Type-2 Chebyshev trajectories of the
small bodies JPL integrated as perturbers when building DE440/DE441. They share
the DE441 planetary background. starcat reads these with its SPK reader
(little-endian DAF, Type-2 and Type-21 segments supported).

`asteroids_de441/` holds three bundles that differ in **which bodies** they carry
and **over what time span**:

| File | Size | Bodies | Time span | Notes |
|------|------|--------|-----------|-------|
| `sb441-n16.bsp` | ~646 MB | 16 most-massive main-belt perturbers | -8001 to 9000 (full DE441 window) | Headline file. Provides the natal main-belt asteroids starcat exposes: Ceres, Pallas, Juno, Vesta, Hygiea (plus 11 more perturbers). |
| `sb441-n373.bsp` | ~15.2 GB | all 373 DE440/DE441 perturbers | -8001 to 9000 (full DE441 window) | Superset of n16. This is the file that carries the trans-Neptunian bodies (Eris, Haumea, Makemake, Quaoar, Orcus, Ixion, Varuna, Sedna, Gonggong) and dozens more. |
| `sb441-n373s.bsp` | ~982 MB | the same 373 perturbers | 1549 to 2650 (DE440 standard window) | An **SPKMERGE-derived subset of n373** (its comment header records `SOURCE_SPK_KERNEL = sb441-n373.bsp`, `BEGIN_TIME 1549 DEC 30 / END_TIME 2650 JAN 24`). Same bodies as n373 but truncated to the short standard window, hence ~15x smaller. The trailing `s` = short/standard-window. |

The directory also contains `SB441_IOM392R-21-005_perturbers.pdf` - the JPL
memo listing the perturbers. Documentation, not data.

**n16 vs n373 vs n373s, in one line each:**

- **n16** - few bodies (16), full time span. The 5 main-belt asteroids starcat
  natively names live here.
- **n373** - many bodies (373, incl. the KBOs/TNOs), full time span, very large
  (15 GB).
- **n373s** - same 373 bodies as n373 but only the 1549-2650 window; a
  space-saving stand-in for n373 when only modern-era dates are needed.

**Does starcat compute the n373 bodies today?** Yes, out of the box. The SPK
reader is format-agnostic: it scans the DAF segment directory and dispatches per
segment (Type-2 / Type-21) by NAIF id, with no dependence on the file's body
count. It reads any little-endian `sb441-n*.bsp`. And `spk::open_all_sources`
auto-opens the dwarf bundle at compute time (`sb441-n373s.bsp`, else the full
`sb441-n373.bsp`), so a plain `data fetch de441` install computes Eris, Haumea,
Makemake, Quaoar, Orcus, Ixion, Varuna, Sedna, and Gonggong without `--spk`. Two
nuances remain:

1. pericynthion's high-level `Asteroid` enum only enumerates the five main-belt
   bodies found in n16 (Ceres, Pallas, Juno, Vesta, Hygiea). The trans-Neptunian
   bodies in n373 are computed **by NAIF id**, not surfaced through that
   named-asteroid convenience type.
2. The hash-pinned **production set** (the oracle's supported-placements verify
   subset) lists only `sb441-n16.bsp` among the asteroid SPKs; n373/n373s have
   full oracle entries but are not in that verify predicate. So the dwarf bundle
   is *computed* by default but is not (yet) a *verified* production dependency.
   Separately, when the SPK source is the **platform data dir** (curated) every
   `.bsp` in it is opened; only a **bulk external mirror** falls back to the
   named-bundle allow-list, so its `satellites/`/`spacecraft/` SPKs stay out.

### `asteroids_de430/`

`ast343de430.bsp` (~1.2 GB) and `ast343de430.ftp` - the DE430-generation
predecessor of the sb441 bundles (343 asteroids). Superseded by the sb441 files;
not needed if DE441 is the target.

### Mission subdirectories (not natal-relevant)

`apophis/`, `67p/`, `dart/` (Didymos/Dimorphos), `orex/` (OSIRIS-REx / Bennu),
`siding_spring/` - individual mission-target small bodies. Not natal chart
bodies; skip.

---

## `satellites/` and `spacecraft/`

`satellites/` holds planetary-moon ephemerides (e.g. `jup365.bsp`, `sat450.bsp`,
Uranus/Neptune/Mars moon sets) as SPK files, giving each moon's position relative
to its host planet. `spacecraft/` holds probe trajectories (Cassini, Voyager,
Mariner, Viking, etc.). Neither contains natal chart bodies; starcat uses nothing
from either subtree. They are also the two largest subtrees (satellites ~118 GB),
so excluding them is the biggest single win for `migrate`.

---

## What starcat needs / what `data migrate` should grab

starcat consumes exactly two things from this mirror:

1. **The little-endian DE441 binary triple** in `planets/Linux/de441/`:
   - `planets/Linux/de441/header.441`
   - `planets/Linux/de441/linux_*.441` (currently `linux_m13000p17000.441`)
   - `planets/Linux/de441/testpo.441` (test-point printout; useful for
     post-migrate verification even though it is not read at compute time)

2. **The asteroid perturber SPK(s)** in `small_bodies/asteroids_de441/`:
   - `small_bodies/asteroids_de441/sb441-n16.bsp` - **required**; the production
     main-belt asteroid source.
   - `small_bodies/asteroids_de441/sb441-n373.bsp` **or** `sb441-n373s.bsp` - 
     **optional**, needed only if trans-Neptunian bodies (Eris, Haumea, etc.) are
     wanted from this static source rather than fetched via Horizons. Prefer
     `n373s` (~982 MB, 1549-2650) over `n373` (~15 GB, full span) unless the
     deep historical window is required. These are not in the production set
     today, so migrating them is a forward-looking option, not a current need.

Separately, the **Horizons per-body SPK directory** (documented in the Horizons
reference) is starcat's other data source, but it is fetched on demand and stored
under its own data root - it is not part of this static mirror and is out of
scope for a mirror `migrate`.

### Skip everything else, and why

- **`planets/ascii/`** - the *source* the `Linux/` binary is built from;
  redundant once you have the binary, and larger (ascii/de441 ~8.6 GB vs the
  2.6 GB binary).
- **`planets/SunOS/`** - big-endian; starcat's reader rejects big-endian, and
  it does not even contain de441.
- **`planets/bsp/`, `bpc/`, `nio/`, `fortran/`, `ioms/`, `stations/`,
  `test-data/`** - SPK/orientation re-packs, JPL-internal NAVIO, reader source
  code, PDFs, ground-station coordinates, and validation printouts: none are the
  format starcat's DE reader consumes.
- **All DE versions other than 441** across every format - starcat's default is
  DE441; older DEs are historical.
- **`small_bodies/asteroids_de430/` and the mission subdirs** - superseded
  asteroid set, and non-natal mission targets.
- **`satellites/` and `spacecraft/` entirely** - planetary moons and probes are
  not natal chart bodies (and are the bulk of the tree's size).

### Selection is format-aware

`data migrate` selects files by **format and role**, keyed on what starcat's
readers consume, not on the DE number in the filename. For a chosen series that
means two things:

- the little-endian DE binary directory under `planets/Linux/deNNN/` - the
  `header.NNN` + `linux_*.NNN` pair (for DE441: `header.441` +
  `linux_m13000p17000.441`); and
- the matching asteroid SPK(s): the `sb441-*` bundles for de440/de441, or
  `ast343de430.bsp` for de430/de431 (see the entourage registry below).

The same DE number also exists as `ascii/` (the text source the binary is built
from), `SunOS/` (big-endian), `bsp/` (a SPICE re-pack read by a different code
path), and `nio/` (JPL-internal NAVIO); the perturber SPKs use the separate
`sb441-*` naming. Because role and format do not follow from the DE number
alone, selection targets the specific format each reader needs rather than
matching on the name.

---

## Entourages: every DE series as a selectable batch

DE441 is starcat's default, but it is not the only usable series. This section
treats each DE series as an **atomic, choosable batch** - its "entourage": the
little-endian binary that carries the planets, plus whatever asteroid companion
(if any) the mirror pairs with that generation. `data migrate` (and a future
`--series` selector) can offer any of these as a unit.

Two scopes bound the registry:

- **Only little-endian (`planets/Linux/`) binaries are candidates.** The `SunOS/`
  tree is big-endian (starcat's reader rejects it) and stops at de423 anyway;
  `ascii/` is redundant text source. So the entourage registry is the 19 series
  present under `planets/Linux/`.
- **The window / short-vs-long-sibling role of each series is not re-derived
  here** - it comes from the DE version genealogy. The registry reflects it:
  e.g. de440 (1550-2650, modern standard) pairs with its deep-time sibling de441
  (-13200..+17191); de430 (1550-2650, prior generation) pairs with de431
  (deep-time). The two members of a short/long pair share the same asteroid
  companion.

### Asteroid companion, by generation

The perturber SPK naming changed across generations and is **not 1:1** with the
planetary binary:

- **de440 / de441** -> `small_bodies/asteroids_de441/` : `sb441-n16.bsp`
  (~646 MB, 16 main-belt perturbers, full window), `sb441-n373.bsp` (~15.2 GB,
  all 373 perturbers incl. TNOs, full window), `sb441-n373s.bsp` (~982 MB, same
  373 bodies but 1550-2650 subset). Both de440 and de441 draw on this same set.
- **de430 / de431** -> `small_bodies/asteroids_de430/ast343de430.bsp`
  (~1.15 GB). de430 and de431 (the prior short/long pair) **share** this one
  file. Verified directly from its DAF segment table: **343 asteroid segments**,
  target ids `2000001`..`2001467` - i.e. the **same `2000000 + MPC` scheme** as
  the sb441 files (Ceres = 2000001, Pallas = 2000002, ...), all **SPK Type-2
  Chebyshev**, centered on the Sun (NAIF 10) in the J2000/ICRF frame, spanning
  **~1550 to ~2650** (the DE430 standard window, not deep-time). Its SPK comment
  area is sparse (internal name `ast343de430_corrected.bsp`, "Created by Monte
  100 on 2014/12/01"); the body/scheme/window facts above come from parsing the
  segment descriptors, not the comment.
- **Series older than de430** (de403, de405, de406, de410, de413, de414, de418,
  de421, de422, de423, de200, de202, de102) -> **no asteroid companion in the
  mirror**. Their entourage is planets-only.

### The `t` variants (de430t, de440t)

The `t` suffix is **not a separate ephemeris** - it is the same planetary
integration with an **extra Chebyshev series fit to TT-TDB** (the geocenter
relativistic time-scale difference), per the de430t `README.txt` and the ASCII
format's item list (TT-TDB is item 15, stored in seconds). Positions are
identical to the non-`t` build; the extra polynomial only lets a caller compute
TT-TDB without an external formula. starcat does not currently consume the
TT-TDB series, so the `t` variants are treated as a **footnote**, not a distinct
entourage. (They are also currently unreadable as-is - see the wiring column:
their header is named `header.NNNt`, which fails the digits-only header rule.)

### starcat wiring: what lights up today

Two independent code paths gate whether a series' entourage actually works:

- **Planetary binary** (`crates/pericynthion/src/jpl/` discover): requires a
  `header.NNN` whose extension is **all digits** AND a binary whose name starts
  with **`linux_`** (LE) or `xnp_` (BE). This means:
  - de440, de441 - plain `header.NNN` + `linux_*.NNN` -> **readable**.
  - de430, de430t, de440t - have a `linux_*` binary, but their header is
    `header.NNN_572` (de430) or `header.NNNt` (de430t/de440t), which fails the
    digits-only rule -> **not discovered as-is** (needs a `header.NNN` symlink).
  - de431 - header `header.431_572` fails digits, and its binary is `lnxm...`
    (not `linux_`) -> **not discovered as-is** (needs header + binary symlinks).
  - de102..de423 (all pre-de430) - header.NNN passes, but the binary is `lnx*`
    (not `linux_`/`xnp_`), so no binary matches -> **not discovered as-is**
    (needs a `linux_*` symlink to the `lnx*` file).
- **Asteroid bundle** (`crates/pericynthion/src/spk/` `locate_default_bsp` /
  `locate_n373_bsp`): hardcoded to the exact filenames `sb441-n16.bsp` and
  `sb441-n373.bsp`. The SPK *reader* itself is generic (any LE DAF, Type-2 /
  Type-21), so `ast343de430.bsp` and `sb441-n373s.bsp` are **readable but not
  located by name** - nothing in the locators matches those filenames.

So there are three asteroid states: **wired** (located + read: `sb441-n16`,
`sb441-n373`), **readable-but-not-wired** (`sb441-n373s`, `ast343de430` - the
reader would parse them, but no locator finds them), and **absent** (pre-de430
series have no companion at all).

### Entourage registry

Sizes are the on-disk byte counts rounded. "Window/role" follows the DE version
genealogy. All binaries are little-endian (`planets/Linux/deNNN/`). Every series
also ships a `testpo.NNN` verification vector (sizes range ~46 KB for de413 up to
~24 MB for de431/de441); it is **optional** - not read at compute time - so it is
omitted from the essential-files column and noted separately.

| Series | Window / role | Essential binary (header + linux_*) | Size | Asteroid companion | Companion size | Bodies added by companion | starcat wiring |
|--------|---------------|-------------------------------------|------|--------------------|----------------|---------------------------|----------------|
| de441 | -13200..+17191, deep-time (long sibling of de440); **pericynthion default** | `header.441` + `linux_m13000p17000.441` | ~2.6 GB | `sb441-n16` (+opt `n373`/`n373s`) | ~646 MB (+15.2 GB / 982 MB) | Ceres, Pallas, Juno, Vesta, Hygiea (n16); +TNOs Eris, Haumea, Makemake, Quaoar, Orcus, Ixion, Varuna, Sedna, Gonggong (n373) | **planets: wired; asteroids: n16/n373 wired, n373s readable-not-wired** |
| de440 | 1550-2650, modern standard (short sibling) | `header.440` + `linux_p1550p2650.440` | ~102 MB | same sb441 set as de441 | ~646 MB (+...) | same as de441 | **planets: wired; asteroids: n16/n373 wired, n373s readable-not-wired** |
| de440t | 1550-2650, de440 + TT-TDB series (footnote) | `header.440t` + `linux_p1550p2650.440t` | ~113 MB | same sb441 set | ~646 MB (+...) | same as de441 | planets: readable-but-not-wired (header `.440t` fails digit rule); asteroids as de440 |
| de431 | -13200..+17191, deep-time (long sibling of de430); SE2 / Solar Fire 9.x | `header.431_572` + `lnxm13000p17000.431` | ~2.6 GB | `ast343de430.bsp` (shared w/ de430) | ~1.15 GB | 343 main-belt/near perturbers (2000000+MPC scheme) - Ceres..~MPC 1467; no TNOs | planets: readable-but-not-wired (header `_572` + `lnx*` fail rules); asteroids: readable-but-not-wired |
| de430 | 1550-2650, prior-gen standard (short sibling); SE2 / Solar Fire 9.x | `header.430_572` + `linux_p1550p2650.430` | ~102 MB | `ast343de430.bsp` (shared w/ de431) | ~1.15 GB | 343 perturbers (as de431) | planets: readable-but-not-wired (header `_572` fails digit rule); asteroids: readable-but-not-wired |
| de430t | 1550-2650, de430 + TT-TDB series (footnote) | `header.430t` + `linux_p1550p2650.430t` | ~99 MB | `ast343de430.bsp` | ~1.15 GB | as de430 | planets: readable-but-not-wired (header `.430t` fails); asteroids: readable-but-not-wired |
| de423 | 1799-2200, MESSENGER Mercury refinement | `header.423` + `lnxp1800p2200.423` | ~37 MB | none | - | - | planets: readable-but-not-wired (binary `lnx*` not `linux_`); asteroids: absent |
| de422 | -3000..+3000, deep-time (long sibling of de421); full precision | `header.422` + `lnxm3000p3000.422` | ~558 MB | none | - | - | planets: readable-but-not-wired (`lnx*`); asteroids: absent |
| de421 | 1899-2053, LLR update; HORIZONS default for a period | `header.421` + `lnxp1900p2053.421` | ~14 MB | none | - | - | planets: readable-but-not-wired (`lnx*`); asteroids: absent |
| de418 | 1899-2051, MESSENGER pre-flyby | `header.418` + `lnxp1900p2050.418` | ~14 MB | none | - | - | planets: readable-but-not-wired (`lnx*`); asteroids: absent |
| de414 | 1600-2200, MGS/Odyssey inner-planet fit | `header.414` + `lnxp1600p2200.414` | ~56 MB | none | - | - | planets: readable-but-not-wired (`lnx*`); asteroids: absent |
| de413 | 1900-2050, Pluto/Charon occultation planning | `header.413` + `lnxp1900p2050.413` | ~14 MB | none | - | - | planets: readable-but-not-wired (`lnx*`); asteroids: absent |
| de410 | 1960-2020, Mars rover navigation | `header.410` + `lnxp1960p2020.410` | ~5.6 MB | none | - | - | planets: readable-but-not-wired (`lnx*`); asteroids: absent |
| de406 | -3000..+3000, deep-time (long sibling of de405); truncated coeffs | `header.406` + `lnxm3000p3000.406` | ~199 MB | none | - | - | planets: readable-but-not-wired (`lnx*`); asteroids: absent |
| de405 | 1600-2200, gold standard 1997-2013; SE1 standard window | `header.405` + `lnxp1600p2200.405` | ~56 MB | none | - | - | planets: readable-but-not-wired (`lnx*`; also ships year-block splits); asteroids: absent |
| de403 | 1600-2200, first 300-asteroid integration; ICRS | `header.403` + `lnxp1600p2200.403` | ~56 MB | none | - | - | planets: readable-but-not-wired (`lnx*`); asteroids: absent |
| de202 | 1900-2050, narrow-window variant | `header.202` + `lnxp1900p2050.202` | ~11 MB | none | - | - | planets: readable-but-not-wired (`lnx*`); asteroids: absent |
| de200 | 1600-2170, FK5-aligned; SE1 origin | `header.200` + `lnxm1600p2170.200` | ~43 MB | none | - | - | planets: readable-but-not-wired (`lnx*`); asteroids: absent |
| de102 | -1410..+3002, Voyager-era; FK4 frame | `header.102` + `lnxm1410p3002.102` | ~155 MB | none | - | - | planets: readable-but-not-wired (`lnx*`); asteroids: absent |

Notes:

- **Only de441 is fully usable out of the box today** (planets wired + at least
  one asteroid bundle wired). de440 shares that status for the modern window.
- **Everything else needs locator/discover wiring to light up**, in two forms:
  (a) the planetary binary needs the discover rules broadened (or a `header.NNN`
  + `linux_*` symlink) to accept the `lnx*` / `header.NNN_572` / `header.NNNt`
  naming of pre-de440 and `t` series; (b) the asteroid companion for the de430
  generation (`ast343de430.bsp`) and the `sb441-n373s.bsp` subset need locators
  that match those filenames. The SPK *reader* already handles all of them - only
  the by-name locators are the gap.
- **de405 and de403 split their binary into overlapping year blocks**
  (`lnxp1600p2200.NNN` full-span plus `lnx1600.NNN`, `lnx1750.NNN`, ...). The
  discover code's "largest file wins" tiebreak would pick the full-span file
  once the `lnx*` prefix is accepted; the year-block files are redundant.
- **de430/de431 asteroid GMs vs positions:** the older `header.NNN_572` lists 572
  constants (343 asteroid masses); the `_229` reduced header (not present in this
  Linux tree) omits those masses. Body positions are unaffected either way.

---

## Complete inventory

This section accounts for **every directory and file-class** under
`ssd.jpl.nasa.gov/ftp/eph/`, so a reader can confirm nothing was skipped
unexamined. Useful items carry their detail; everything else has a one-line
exclusion reason. The entourage registry above remains the actionable core; this
is the exhaustiveness backstop - the "don't let another `sb441-n373s.bsp` slip
past" check.

File-suffix shorthand used below (see earlier sections for the full
explanations): `.bsp` = SPICE SPK binary (starcat-readable if LE + Type-2/21);
`.nio` / `.ftp` = JPL-internal NAVIO (unusable outside JPL nav software); `.xsp`
= big-endian SPK variant (reader rejects); `.tsp` = tracking/text SPK product;
`.tpc` / `.tf` = SPICE text constants / frame kernels (orientation, not
positions); `.bpc` = binary PCK (orientation); `.log` / `.tex` / `.txt` /
`.pdf` = logs, docs, memos.

### `planets/` - fully itemized

| Item | What it is | starcat verdict |
|------|-----------|-----------------|
| `Linux/deNNN/` | LE DE binaries (header + `linux_*`/`lnx*`, + `testpo`). | **The source of the entourage registry.** de441 wired; others need wiring (see above). |
| `ascii/` | DE coefficients as text (`ascp*`/`ascm*`), the source the binaries build from. | Excluded: redundant text source, larger than the binary. |
| `SunOS/` | Big-endian (`xnp_*`) DE binaries; only through de423. | Excluded: big-endian (reader rejects), and no de440/de441. |
| `bsp/deNNN.bsp` | SPK re-pack of the planetary ephemeris. Verified: `de441.bsp` is a Type-2 DAF/SPK carrying planet barycenters 1-10 vs SSB (center 0) plus 199/299/399/301 body-centers - the same bodies as the Linux binary, **SPK-reader-consumable** in principle. | Excluded as redundant: starcat's planet path uses the native DE reader, and the SPK locators only match `sb441-*`, so this `.bsp` is not located. It is a *usable alternative* format, not a missing capability. Also carries `_plus_MarsPC`, `_s` (short), `TTmTDB.*.bsp`, and many non-441 DEs - all redundant. |
| `bpc/` | `moon_pa_de430_1550-2650.bpc` (binary PCK: lunar libration Euler angles) + `moon_190627.tf` (PA-to-ME frame) + README. Lunar **orientation**, not body position. | Excluded: orientation kernels; starcat computes positions, not lunar body-fixed frames. |
| `nio/` | NAVIO (`.ftp`/`.nio`) DE files + `partials/` (7 `ppar_*` partial-derivative files for nav fits). | Excluded: JPL-internal NAVIO, unusable outside JPL nav software. |
| `fortran/` | Reader/converter source (`asc2eph.f`, `testeph.f`, `userguide.txt`). | Excluded: reference source code, not data. |
| `ioms/` | ~18 interoffice-memo PDFs documenting individual DE releases. | Excluded: documentation. |
| `stations/dsn.itrf93/` | Deep Space Network antenna coordinates in ITRF93 (`*_cartesian_*.txt`, `*_geodetic_*`, an `OLD/` archive, README). | Excluded: ground-station locations, not ephemeris body data. |
| `test-data/` | `testpo.NNN` test-point printouts + a small `430/` sample set. | Excluded: reader-validation vectors (starcat's DE441 `testpo` already sits in `Linux/de441/`). |
| `CDROM.notes`, `README.txt`, `other_readers.txt`, `ascii_format.txt` | Top-level docs / historical CD notes. | Excluded: documentation. |

### `small_bodies/` - fully itemized

| Item | What it is | starcat verdict |
|------|-----------|-----------------|
| `asteroids_de441/` | `sb441-n16.bsp`, `sb441-n373.bsp`, `sb441-n373s.bsp` (+ perturber IOM PDF). | **Core asteroid entourage** for de440/de441 (analyzed above). n16/n373 wired; n373s readable-not-wired. |
| `asteroids_de430/` | `ast343de430.bsp` (343 asteroids, 2000000+MPC scheme, Type-2, ~1550-2650) + `.ftp`. | de430/de431 companion; readable-but-not-wired (locator matches only `sb441-*`). Superseded by sb441 when targeting DE441. |
| `apophis/` | 99942 Apophis mission products: `a99942.set3_*`, `sb-99942-*.tsp`, `sbp-99942-*.nio.ftp`, IOM PDFs. | Excluded from mirror-migrate: single-object mission SPK. Apophis is a body some astrologers use, but it is reachable on-demand via `starcat horizons` (MPC 99942) - no need to bulk-migrate the mission set. |
| `dart/` | Didymos 65803 + Dimorphos: `sb-65803-205.bsp` (verified target 20065803, heliocentric Type-2 - identical to what Horizons yields), `.xsp` (BE), `.nio`, `dimorphos_*.bsp`/`.tf`/`.tpc`, docs, `s523.tgz`. | Excluded: single-object mission SPK; Didymos reachable via `starcat horizons` (MPC 65803). Dimorphos is a moonlet, not a chart body. |
| `orex/` | OSIRIS-REx / 101955 Bennu: `sb-101955-*.bsp`/`.tsp`/`.nio.ftp`, `cov-*` covariances, `planet/`, docs. | Excluded: single-object mission SPK; Bennu reachable via `starcat horizons` (MPC 101955). |
| `67p/` | Comet 67P/Churyumov-Gerasimenko: `sb-67p-k151-6.bsp` (target 1000012, comet-id scheme). | Excluded: single comet mission SPK; reachable via Horizons if wanted. Comets are rarely natal bodies. |
| `siding_spring/` | Comet C/2013 A1: several `c2013a1_s*_merged_DE431.bsp`. | Excluded: single comet mission SPK; Mars-encounter product. |

### `satellites/` - fully itemized (the ~118 GB tree)

This is the largest subtree and the main "hidden gem" hunting ground. Its files
are **planetary-moon and minor-body-satellite systems** - each gives moon orbits
relative to a planet or small-body barycenter. Itemized by family:

| Family | Primary / contents | Natal relevance |
|--------|--------------------|-----------------|
| `bsp/jup*` (~24 files) | Jupiter moon systems (Io, Europa, Ganymede, Callisto + minor moons), targets 5xx vs Jupiter barycenter 5. | None: Jovian moons are not chart bodies. |
| `bsp/sat*` (~27) | Saturn moon systems (Titan, Rhea, ... + `daphnis`, `pan`), 6xx vs barycenter 6. | None. |
| `bsp/ura*` (~22) | Uranus moon systems, 7xx vs barycenter 7 (incl. `.30kyr`, `.xl` long-span variants). | None. |
| `bsp/nep*` (~14) | Neptune moon systems (Triton etc.), 8xx vs barycenter 8; plus `Triton.nep097.30kyr.bsp`. | None (Triton is a moon). |
| `bsp/mar*` (~4) | Mars moons Phobos/Deimos, 4xx vs barycenter 4. | None. |
| `bsp/plu*`, `se_pluto*` (~8) | **Pluto system**: verified `plu060.bsp` carries target **999 (Pluto body-center)** and 901-905 (Charon, Nix, Hydra, Kerberos, Styx) relative to **Pluto-system barycenter 9**, window ~1800-2200. | Excluded: body-center (999) vs system barycenter (9) is a sub-arcsecond distinction (~0.075-0.10" at Pluto's distance), astrologically negligible, and the barycenter is the convention (Swiss Ephemeris et al.). See *Deliberately excluded* below. |
| `bsp/tnosat_v*` (~12) | TNO satellite systems. Verified each file also carries a **heliocentric Type-21 segment of the primary** (e.g. `20136108` Haumea, `20136199` Eris, `20050000` Quaoar, `20090482` Orcus vs Sun 10), plus the moon(s) vs the TNO barycenter. Also 120347 Salacia, 469705, 617 Patroclus, two 53M-scheme bodies. | Excluded: the primaries (Eris, Haumea, Quaoar, Orcus) already come from `sb441-n373.bsp` + `starcat horizons`; this family's unique content is TNO-moon orbits, not chart bodies - no new coverage. |
| `orientation/` | one IOM PDF (satellite orientation memo). | Excluded: documentation. |
| `nio/LINUX_PC/` | NAVIO satellite files (`.nio`/`.ftp`/`.txt`). | Excluded: JPL-internal NAVIO. |
| `rckin/` | `rckin.*` rotational-constants text/log kernels per moon system. | Excluded: orientation/rotation constants, not positions. |

### `spacecraft/` - brief-excluded

Probe/mission trajectories (Cassini, Voyager 1/2, Mariner, Viking, Phobos-2,
etc.), ~1.6 GB, in `.bsp`/`.xsp`/`.nio` form. Excluded: spacecraft trajectories,
not natal bodies.

### Deliberately excluded

Two satellite-tree items touch chart bodies. Both were examined and are excluded
from the mirror-migrate; the reasoning is recorded here so the decision is
traceable.

1. **Pluto body-center vs barycenter (`plu060.bsp`, ~117 MB)** - The DE441 Linux
   binary and `bsp/de441.bsp` provide the **Pluto-system barycenter** (NAIF 9),
   not Pluto's own body-center (999). `plu060.bsp` carries 999 (and Charon/Nix/
   Hydra/Kerberos/Styx) but only **relative to barycenter 9**, so it cannot stand
   alone - it would have to be composed with the DE441 barycenter position. The
   body-center-vs-barycenter offset is **sub-arcsecond** (~0.075-0.10" at Pluto's
   distance), astrologically negligible, and the barycenter is the convention
   (Swiss Ephemeris et al.). **Excluded**, with the composition left open as a
   future opt-in max-precision mode.

2. **TNO dwarf-planet ephemerides in `tnosat_v*`** - These carry heliocentric
   Type-21 segments for Haumea, Eris, Quaoar, Orcus (all chart-relevant dwarf
   planets), but starcat already obtains those four from `sb441-n373.bsp`
   (Type-2, full -8001..9000 window) and can fetch them via `starcat horizons`.
   The family's unique content is the TNO-moon orbits (Weywot, Vanth, Hi'iaka,
   Dysnomia), which are not chart bodies. **Excluded** - no new body coverage.

Everything else in `satellites/` and `spacecraft/` is planetary moons and probes
with no chart-body content.
