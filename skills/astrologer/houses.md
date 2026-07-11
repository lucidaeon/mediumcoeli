# Reference: House Systems

> **Provenance.** Tables seeded from the comparison table under *Space-based house
> systems* in Wikipedia, [*House (astrology)*](https://en.wikipedia.org/wiki/House_(astrology))
> (retrieved 2026-06-18), then extended to the full catalogue.
>
> **Conventions.** ε = obliquity of the ecliptic. α = right ascension. λ = ecliptic
> longitude. φ = geographic latitude. RAMC = right ascension of the Midheaven
> (= Local Apparent Sidereal Time as an angle). Asc = Ascendant, MC = Midheaven,
> IC = Imum Coeli, EP = East Point (equatorial Ascendant). "Projection along great
> circles through *X*" means each division point is carried to the ecliptic along
> the great circle that passes through *X* and the division point.

---

## Table 1 — Space-division (great-circle) house systems

Divide a **fundamental great circle** into equal arcs of **arc/angle**, then carry
each division to the ecliptic. The division is geometric (space), not temporal.

| System | Fundamental great circle | Division | Projection onto ecliptic | 1st cusp | 10th cusp | Citations | Notes |
|---|---|---|---|---|---|---|---|
| Whole Sign | Ecliptic | 12 × 30° of longitude, by sign | — (lies on ecliptic) | 0° of the rising sign | 0° of the 10th sign | Hand, *Whole Sign Houses* (ARHAT, 2000); [Skyscript: House Division](https://www.skyscript.co.uk/glossary/house-division/); Holden, "Ancient House Division," *J. Research AFA* 1.1 (1982) | Holden's "Sign House" — the oldest system; still standard in India |
| Equal (from Asc) | Ecliptic | 12 × 30° of longitude from Asc | — | λAsc | λAsc + 270° (≠ MC) | Hone, *The Modern Text-Book of Astrology*, ISBN 0-85243-357-3; North, *Horoscopes and History*, ISBN 978-0-85481-068-0; Holden, "Ancient House Division," *J. Research AFA* 1.1 (1982) | North's "Single Longitude" method (group 6); Firmicus, *Mathesis* II.19 (c. AD 335), with Ptolemy's tacit approval |
| Equal from MC (M-House) | Ecliptic | 12 × 30° of longitude from MC | — | λMC + 90° (≠ Asc) | λMC | [Swiss Ephemeris Programmer's Guide §13](https://www.astro.com/swisseph/swephprg.htm) | — |
| Vehlow Equal | Ecliptic | 12 × 30° of longitude, Asc centred in H1 | — | λAsc − 15° | λAsc + 255° | Hone, *The Modern Text-Book of Astrology*, ISBN 0-85243-357-3 | — |
| Porphyry | Ecliptic (quadrants between angles) | Trisect each Asc–IC–Dsc–MC quadrant in longitude | — | λAsc | λMC | North, *Horoscopes and History*, ISBN 978-0-85481-068-0; Holden, "Ancient House Division," *J. Research AFA* 1.1 (1982) | North's "Dual Longitude" method (group 2); al-Bīrūnī's "method of the ancients"; Valens attributes the trisection to Orion (Holden) |
| Sripati | Ecliptic (Porphyry, shifted ½ house) | Porphyry cusps become house *centres*; boundaries are their midpoints | — | midpoint(λAsc, Porphyry H12) | midpoint(λMC, Porphyry H9) | Raman, *A Manual of Hindu Astrology* (classical bhāva system), ISBN 978-81-208-1043-1 | — |
| Pullen Sinusoidal (SR/SP) | Ecliptic (quadrants between angles) | Porphyry, with the middle house of each quadrant sinusoidally compressed/expanded | — | λAsc | λMC | Pullen, [Sinusoidal House Systems](http://www.astrolog.org/astrolog/astsine.htm) (astrolog.org) | — |
| Carter Poli-Equatorial | Celestial equator | 12 × 30° of RA from RA(Asc) | Great circles through the **celestial poles** (hour circles) | Asc | ≠ MC | Carter, *Essays on the Foundations of Astrology*, ISBN 0-7229-5132-9; North, *Horoscopes and History*, ISBN 978-0-85481-068-0 | Geometry = North's "Equatorial, moving boundaries" (group 5) |
| Meridian (Axial Rotation / Zariel) | Celestial equator | 12 × 30° of RA from RAMC | Great circles through the **celestial poles** (hour circles) | East Point (EP) | MC | [Swiss Ephemeris Programmer's Guide §13](https://www.astro.com/swisseph/swephprg.htm) | Absent from North's typology (modern; uniform RA from RAMC, not from Asc) |
| Morinus | Celestial equator | 12 × 30° of RA from RAMC | Great circles through the **ecliptic poles** (longitude circles) | ≠ Asc | ≠ MC | [Swiss Ephemeris Programmer's Guide §13](https://www.astro.com/swisseph/swephprg.htm) | Absent from North's typology (Morin, 17th c.; equator divided, ecliptic-pole projection) |
| Regiomontanus | Celestial equator | 12 × 30° of RA from RAMC | Great circles through the **N/S horizon points** | Asc | MC | North, *Horoscopes and History*, ISBN 978-0-85481-068-0 | North's "Equatorial, fixed boundaries" (group 4) |
| Campanus | Prime vertical | 12 × 30° from the East Point | Great circles through the **N/S horizon points** | Asc | MC | North, *Horoscopes and History*, ISBN 978-0-85481-068-0 | North's "Prime Vertical, fixed boundaries" (group 3) |
| Horizon (Horizontal) | Horizon | 12 × 30° of azimuth from the East Point | Azimuth (vertical) circles ∩ ecliptic | Asc (EP) | the prime-vertical/ecliptic point | [Swiss Ephemeris §13](https://www.astro.com/swisseph/swephprg.htm) (system `H`) | — |
| Krusinski–Pisa–Goeldi | Great circle through Asc and zenith | 12 × 30° from the Asc | Great circles through the poles of the Asc–zenith circle | Asc | MC | [Swiss Ephemeris §13](https://www.astro.com/swisseph/swephprg.htm) (system `U`) | — |

> **Morinus vs Meridian.** Both divide the equator into 30° RA arcs from the RAMC;
> they differ *only* in projection. Meridian carries each point along an **hour
> circle** (constant RA), so λ = atan2(sin α, cos α · cos ε) and the H10 image is
> the MC. Morinus carries it along a **longitude circle** (constant λ) — a direct
> equator→ecliptic transform of the δ = 0 point — so λ = atan2(sin α · cos ε,
> cos α) and H10 is *not* the MC.

> **Cardinal-angle invariant.** The four angles (Asc / IC / Dsc / MC = cusps
> 1 / 4 / 7 / 10) are *identical* across the Standard (Alcabitius), Campanus, and
> Regiomontanus systems; these methods differ only in the intermediate cusps.
> (North, *Horoscopes and History*, §1.2, ISBN 978-0-85481-068-0.) — a useful
> cross-check oracle for any quadrant implementation.

---

## Table 2 — Time-division house systems

Divide a **temporal quantity** (a diurnal/semi-diurnal arc measured in time, or an
equal sweep of RA standing in for sidereal time) and read off the ecliptic degree
reached. All four reduce toward Porphyry near the equator and degrade or fail at
high latitude.

| System | Quantity divided | Division method | 1st cusp | 10th cusp | Polar behaviour | Citations | Notes |
|---|---|---|---|---|---|---|---|
| Placidus | Diurnal & nocturnal **semi-arcs** of each ecliptic degree | Trisect each semi-arc in time | Asc | MC | Undefined above the polar circle (degrees that never rise/set) | [Skyscript: House Division](https://www.skyscript.co.uk/glossary/house-division/) | Semi-arc method; outside North's typology (he treats it as the later displacer) |
| Koch (Birthplace / GOH) | **MC's** diurnal **semi-arc** | Equal thirds; intermediate cusps share oblique ascension under the birthplace pole | Asc | MC | Undefined when MC degree is circumpolar | Makransky, *Primary Directions* (1992) p. 69 | — |
| Alcabitius | Asc → MC and Asc → IC **diurnal arcs** | Trisect each in time; cusps lie on the corresponding hour circles | Asc | MC | Undefined when the Asc is circumpolar | North, *Horoscopes and History*, ISBN 978-0-85481-068-0; Gansten, [doi:10.1163/25899201-12340029](https://doi.org/10.1163/25899201-12340029); Holden, "Ancient House Division," *J. Research AFA* 1.1 (1982) | North's "Standard method" (group 1); the name is a late ascription — Rhetorius gave a worked example c. AD 500, and Al-Qabisi (d. c. 967) merely explained it (Holden) |
| Topocentric (Polich–Page) | — (closed-form **approximation** of Placidus) | Cusp "pole" from tan θ = (k/3)·tan φ, k = 1,2 | Asc | MC | Numerically stable at high latitude (unlike Placidus) | Polich & Page (1964) | — |

---

## Derivations (alphabetical)

Each entry states, tersely, how the cusps are obtained.

- **Alcabitius.** The diurnal arc from the Ascendant's degree to the MC is trisected
  in time, as is the arc from the Ascendant to the IC; the intermediate cusps (11,
  12 and 2, 3) are the ecliptic degrees lying on the hour circles that cut those
  time-divisions. H1 = Asc, H10 = MC. Undefined when the Ascendant is circumpolar.

- **Campanus.** The prime vertical (the great circle through the East/West horizon
  points and the zenith) is divided into twelve equal 30° arcs from the East Point;
  each division is projected onto the ecliptic along the great circle through the
  north and south points of the horizon. H1 = Asc, H10 = MC.

- **Carter Poli-Equatorial.** The celestial equator is divided into twelve equal 30°
  arcs of right ascension beginning at the RA of the Ascendant; each is projected
  onto the ecliptic along an hour circle (great circle through the celestial poles).
  H1 = Asc; the 10th cusp is generally not the MC.

- **Equal (from Asc).** Cusps every 30° of ecliptic longitude from the Ascendant:
  H1 = λAsc, H_n = λAsc + 30°(n−1). The MC floats and is not the 10th cusp.

- **Equal from MC (M-House).** Cusps every 30° of ecliptic longitude with the 10th
  fixed at the MC: H10 = λMC, H1 = λMC + 90°. The Ascendant floats.

- **Horizon (Horizontal).** The horizon is divided into twelve equal 30° arcs of
  azimuth from the East Point; each division's vertical (azimuth) circle is
  intersected with the ecliptic to give the cusp. H1 = Asc (East Point).

- **Koch (Birthplace / GOH).** The **MC's** diurnal semi-arc at the birth
  latitude is split into equal thirds; cusp N is the ecliptic degree whose
  oblique ascension equals that of the point at RA = RAMC + M·(DSA_MC)/3 carrying
  the MC's declination (M = 1, 2, 4, 5 for cusps 11, 12, 2, 3). H1 = Asc,
  H10 = MC. Fails when the MC degree is circumpolar. Formula: Makransky, *Primary
  Directions: A Primer of Calculation* (1992), p. 69.

- **Krusinski–Pisa–Goeldi.** The great circle passing through the Ascendant and the
  zenith is divided into twelve equal 30° arcs from the Ascendant; each is projected
  onto the ecliptic. H1 = Asc, H10 = MC.

- **Meridian (Axial Rotation / Zariel).** The celestial equator is divided into
  twelve equal 30° arcs of right ascension from the RAMC; each equator point is
  carried to the ecliptic along an hour circle (constant RA), giving
  λ = atan2(sin α, cos α · cos ε). H1 = East Point, H10 = MC. Latitude-independent.

- **Morinus.** The celestial equator is divided into twelve equal 30° arcs of right
  ascension from the RAMC; each equator point (declination 0) is converted directly
  to ecliptic longitude, λ = atan2(sin α · cos ε, cos α) — i.e. projected along a
  longitude circle through the ecliptic poles. H1 ≠ Asc and H10 ≠ MC.
  Latitude-independent, so it is the only quadrant-style system that stays
  well-defined at every latitude, including the poles.

- **Placidus.** For each ecliptic degree, its diurnal and nocturnal semi-arcs (the
  time spent above/below the horizon) are trisected in time; cusps 11 and 12 are the
  degrees that have traversed 1/3 and 2/3 of their semi-diurnal arc since rising,
  and 2 and 3 the analogous nocturnal fractions. H1 = Asc, H10 = MC. Undefined for
  ecliptic degrees that never rise or set (above the polar circle).

- **Porphyry.** Each ecliptic quadrant bounded by the angles (Asc–IC, IC–Dsc,
  Dsc–MC, MC–Asc) is divided into three equal arcs of ecliptic longitude.
  H1 = Asc, H10 = MC.

- **Pullen Sinusoidal (SR / SP).** As Porphyry, but instead of three equal arcs per
  quadrant, the middle house of each quadrant is sinusoidally compressed or expanded
  according to whether the quadrant spans less or more than 90°, smoothing the
  house-size discontinuities Porphyry produces. H1 = Asc, H10 = MC.

- **Regiomontanus.** The celestial equator is divided into twelve equal 30° arcs of
  right ascension from the RAMC; each is projected onto the ecliptic along the great
  circle through the north and south points of the horizon. H1 = Asc, H10 = MC.

- **Sripati.** Porphyry shifted by half a house: the Porphyry cusp longitudes are
  taken as house *centres* (bhāva madhya) and the house *boundaries* (bhāva sandhi)
  are the midpoints between consecutive Porphyry cusps, so the Ascendant sits at the
  centre of the first house rather than on its cusp.

- **Topocentric (Polich–Page).** An empirical closed-form approximation of Placidus:
  each intermediate cusp is found using a modified latitude θ with
  tan θ = (k/3)·tan φ (k = 1, 2), then standard ascension under that θ. Agrees with
  Placidus to within ~1° and stays numerically stable at high latitude. H1 = Asc,
  H10 = MC.

- **Vehlow Equal.** Equal houses offset so the Ascendant falls at the *middle* of
  the first house: H1 = λAsc − 15°, H_n = λAsc − 15° + 30°(n−1).

- **Whole Sign.** Each house is one whole zodiac sign. H1 begins at 0° of the sign
  containing the Ascendant; each following sign is the next house. All cusps are 0°
  of a sign, so the MC may fall in any house, not necessarily the 10th.

