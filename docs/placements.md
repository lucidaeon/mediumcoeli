# Placements

Points and bodies starcat can compute, and the wider catalog it does not
yet cover. Categories follow the latest IAU designations. Generated from
`pericynthion::placements::CATALOG` — do not edit by hand; run
`just placements` to regenerate.

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
| Eris | no | no ephemeris shipped |
| Haumea | no | no ephemeris shipped |
| Makemake | no | no ephemeris shipped |

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
| Chiron | no | no ephemeris shipped |
| Pholus | no | no ephemeris shipped |
| Nessus | no | no ephemeris shipped |
| Chariklo | no | no ephemeris shipped |

## Kuiper-belt objects

| Name | Supported | Notes |
|------|-----------|-------|
| Quaoar | no | no ephemeris shipped |
| Orcus | no | no ephemeris shipped |
| Ixion | no | no ephemeris shipped |
| Varuna | no | no ephemeris shipped |

## Trans-Neptunian objects

| Name | Supported | Notes |
|------|-----------|-------|
| Sedna | no | no ephemeris shipped |
| Gonggong | no | no ephemeris shipped |

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
