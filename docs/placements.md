# Placements

Points and bodies starcat can compute, and the wider catalog it does not
yet cover. Categories follow the latest IAU designations. Generated from
`pericynthion::placements::CATALOG` — do not edit by hand; run
`just placements` to regenerate.

**Supported column:** `yes` means starcat can compute the placement given
the right data. Bodies whose Notes column references a data source
(`sb441-n373.bsp`, `Horizons SPK`) require that file to be present or
the network fetch to have been performed before the computation succeeds.

## Luminaries

| Name | Supported | Notes |
|------|-----------|-------|
| Sun | yes | DE441 (Earth replaces it in heliocentric) |
| Moon | yes | DE441 |

## Planets

| Name | Supported | Notes |
|------|-----------|-------|
| Mercury | yes | DE441 |
| Venus | yes | DE441 |
| Mars | yes | DE441 |
| Jupiter | yes | DE441 |
| Saturn | yes | DE441 |
| Uranus | yes | DE441 |
| Neptune | yes | DE441 |

## Dwarf planets

| Name | Supported | Notes |
|------|-----------|-------|
| Pluto | yes | DE441 |
| Ceres | yes | small-body SPK (sb441-n16.bsp) |
| Eris | yes | small-body SPK (sb441-n373.bsp) |
| Haumea | yes | small-body SPK (sb441-n373.bsp) |
| Makemake | yes | small-body SPK (sb441-n373.bsp) |

## Asteroids

| Name | Supported | Notes |
|------|-----------|-------|
| Pallas | yes | small-body SPK (sb441-n16.bsp) |
| Juno | yes | small-body SPK (sb441-n16.bsp) |
| Vesta | yes | small-body SPK (sb441-n16.bsp) |
| Hygiea | yes | small-body SPK (sb441-n16.bsp) |

## Centaurs

| Name | Supported | Notes |
|------|-----------|-------|
| Chiron | yes | Horizons SPK; fetch with `starcat horizons` |
| Pholus | yes | Horizons SPK; fetch with `starcat horizons` |
| Nessus | yes | Horizons SPK; fetch with `starcat horizons` |
| Chariklo | yes | Horizons SPK; fetch with `starcat horizons` |
| Asbolus | yes | Horizons SPK; fetch with `starcat horizons` |

## Kuiper-belt objects

| Name | Supported | Notes |
|------|-----------|-------|
| Quaoar | yes | small-body SPK (sb441-n373.bsp) |
| Orcus | yes | small-body SPK (sb441-n373.bsp) |
| Ixion | yes | small-body SPK (sb441-n373.bsp) |
| Varuna | yes | small-body SPK (sb441-n373.bsp) |
| Albion | yes | Horizons SPK; fetch with `starcat horizons` |

## Trans-Neptunian objects

| Name | Supported | Notes |
|------|-----------|-------|
| Sedna | yes | small-body SPK (sb441-n373.bsp) |
| Gonggong | yes | small-body SPK (sb441-n373.bsp) |

## Mathematical points

| Name | Supported | Notes |
|------|-----------|-------|
| Ascendant | yes | Ac; needs lat + lon |
| Descendant | yes | Ds; needs lat + lon |
| Medium Coeli | yes | Mc; needs lon |
| Imum Coeli | yes | Ic; needs lon |
| Vertex | yes | Vx; needs lat + lon |
| Anti-Vertex | yes | Ax; needs lat + lon |
| North Node | yes | Nn; mean or true |
| South Node | yes | Sn; mean or true |
| Black Moon Lilith | yes | Lil; mean or true |
| Priapus | yes | Pri; mean or true |
| Lot of Fortune | yes | needs Ac + Sun + Moon |
| Lot of Spirit | yes | needs Ac + Sun + Moon |
| Lot of Exaltation | yes | needs Ac + Sun + Moon |
| Lot of Necessity | yes | + Mercury |
| Lot of Eros | yes | + Venus |
| Lot of Courage | yes | + Mars |
| Lot of Victory | yes | + Jupiter |
| Lot of Nemesis | yes | + Saturn |

## Derived Views

Derived views re-project or augment the placement longitudes already emitted. They are not independently computed bodies — they require the base tropical placement set to be present.

| View | Flag | Description |
|------|------|-------------|
| Draconic zodiac | `--draconic` | Re-projects every placement longitude by `(λ − node_lon) mod 360°`, where `node_lon` is the selected North Node (mean or true). Chart-level `zodiac` becomes `{ "name": "draconic" }` in JZOD output. |
| Antiscion | `--antiscia` | Appends solstice-axis reflection `(180° − λ) mod 360°` for every body and angle. Bodies equidistant from the Cancer/Capricorn axis share an antiscion. |
| Contra-antiscion | `--antiscia` | Appends equinox-axis reflection `(360° − λ) mod 360°` for every body and angle. Bodies equidistant from the Aries/Libra axis share a contra-antiscion. |

## Fixed Stars — Yale BSC5P

9096 stars to V≤6.5 (3143 Bayer/Flamsteed named, 5953 HR-number only). J2000 ICRS positions; tropical longitude computed via IAU 2006 precession at chart epoch. Source: *The Bright Star Catalogue, 5th Revised Ed.* (Hoffleit & Warren 1991, NASA/NSSDC/ADC — public domain).

**Notable fixed stars** (3109 others not listed):

Listed by Harvard Revised (HR) catalogue number, which runs in order of right ascension.

- Alpheratz (21Alp And, HR 15, V2.06)
- Mirach (43Bet And, HR 337, V2.06)
- Hamal (13Alp Ari, HR 617, V2.00)
- Menkar (92Alp Cet, HR 911, V2.53)
- Algol (26Bet Per, HR 936, V2.12)
- Alcyone (25Eta Tau, HR 1165, V2.87)
- Aldebaran (87Alp Tau, HR 1457, V0.85)
- Capella (13Alp Aur, HR 1708, V0.08)
- Rigel (19Bet Ori, HR 1713, V0.12)
- Bellatrix (24Gam Ori, HR 1790, V1.64)
- Betelgeuse (58Alp Ori, HR 2061, V0.50)
- Sirius (9Alp CMa, HR 2491, V-1.46)
- Castor (66Alp Gem, HR 2891, V1.98)
- Procyon (10Alp CMi, HR 2943, V0.38)
- Pollux (78Bet Gem, HR 2990, V1.14)
- Gamma Velorum (Gam2Vel, HR 3207, V1.78)
- Regulus (32Alp Leo, HR 3982, V1.35)
- Denebola (94Bet Leo, HR 4534, V2.14)
- Vindemiatrix (47Eps Vir, HR 4932, V2.83)
- Spica (67Alp Vir, HR 5056, V0.98)
- Agena (Bet Cen, HR 5267, V0.61)
- Arcturus (16Alp Boo, HR 5340, V-0.04)
- Zuben Elgenubi (9Alp2Lib, HR 5531, V2.75)
- Alphecca (5Alp CrB, HR 5793, V2.23)
- Unukalhai (24Alp Ser, HR 5854, V2.65)
- Antares (21Alp Sco, HR 6134, V0.96)
- Ras Alhague (55Alp Oph, HR 6556, V2.08)
- Vega (3Alp Lyr, HR 7001, V0.03)
- Altair (53Alp Aql, HR 7557, V0.77)
- Deneb (50Alp Cyg, HR 7924, V1.25)
- Sadalsuud (22Bet Aqr, HR 8232, V2.91)
- Fomalhaut (24Alp PsA, HR 8728, V1.16)
- Scheat (53Bet Peg, HR 8775, V2.42)
- Markab (54Alp Peg, HR 8781, V2.49)
