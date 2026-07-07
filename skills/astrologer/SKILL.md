---
name: astrologer
description: Technical consultant for astrology software. Use when working on astrology-related code, software, geographic/astronomical coordinates, or when the user mentions astrology. Provides guidance on practices, lexicon, and PII protection.
---

# Technical Astrologer

This skill provides domain context for building astrology software. It covers the traditions, vocabulary, data structures, and technical concepts needed to reason carefully about astrological data, specifications, and tooling.

## Traditions and Zodiacs

Astrology is not a monolith. Software that serves astrologers should be aware that multiple distinct traditions exist, each with its own history, techniques, and worldview. The major traditions include [Babylonian](https://web.archive.org/web/20260124124908/https://en.wikipedia.org/wiki/Babylonian_astrologyy), [Chinese](https://web.archive.org/web/20260301123241/https://en.wikipedia.org/wiki/Chinese_astrology), [Hellenistic](https://web.archive.org/web/20260312131624/https://en.wikipedia.org/wiki/Hellenistic_astrology), [Islamic](https://web.archive.org/web/20260211204823/https://en.wikipedia.org/wiki/Astrology_in_the_medieval_Islamic_world), [Jewish](https://web.archive.org/web/20251217174719/https://en.wikipedia.org/wiki/Jewish_astrology), [Tibetan](https://web.archive.org/web/20260312112512/https://en.wikipedia.org/wiki/Tibetan_astrology), [Vedic](https://web.archive.org/web/20260311151225/https://en.wikipedia.org/wiki/Hindu_astrology), and [Western](https://web.archive.org/web/20260117014810/https://en.wikipedia.org/wiki/Western_astrology). A data format or application that presumes only Western tropical astrology will be too narrow.

A foundational axis of difference is the zodiac. The [Tropical zodiac](https://web.archive.org/web/20260311162703/https://en.wikipedia.org/wiki/Sidereal_and_tropical_astrology) is anchored to the seasons — 0° Aries is always the vernal equinox. The Sidereal zodiac is anchored to the fixed stars and drifts slowly relative to the tropical due to precession. Western and Hellenistic practice typically use tropical; Vedic (Jyotish) uses sidereal. The offset between them, called the ayanamsha, is itself contested among Vedic practitioners. Any chart position stored without specifying which zodiac it uses is ambiguous.

## Language

Precise language matters in astrology software because the domain has its own vocabulary that diverges from colloquial use in ways that affect how data is modeled and displayed.

In natal astrology, the person for whom a chart is cast is called the **native**. The word "user" means something to software developers; "native" means something to astrologers — they are not the same person and should not be conflated in naming or schema design. Horary, event, and mundane charts represent spacetime coordinates rather than individuals; "subject" or "event" is more appropriate terminology for those.

Gender handling requires care. New software and specifications should include "nonbinary" and "any" as a gender options, with "x" and "a" as the short forms respectively. They should also support arbitrary strings supplied by the user, since gender is self-described.

## Test fixtures, sample data, PII and more

Birth data — date, time, and place of birth — is personally identifying information. When writing any files to disk for testing, examples, usage, etc, only the reference charts found in the [fixtures/](fixtures/) directory may be used. If test fixures are missing attributes you need such as character sets / encoding, certain edge case dates times locations or names, stop and flag the issue. Do not invent or use real birth data. Do **not** use any other real or synthesized birth/event data without explicit permission.

## Time and Location

Every chart requires a precise spacetime coordinate. Getting time right is non-trivial. [UTC](https://web.archive.org/web/20260310222158/https://en.wikipedia.org/wiki/Coordinated_Universal_Time) is the stable reference point, but birth times are recorded in local time, which means working through [time zones](https://web.archive.org/web/20260312170644/https://en.wikipedia.org/wiki/Time_zone), [UTC offsets](https://web.archive.org/web/20260310144926/https://en.wikipedia.org/wiki/List_of_UTC_offsets), and the [tz database](https://en.wikipedia.org/wiki/Tz_database) — which captures historical idiosyncrasies like wartime clock changes and regional transitions. A birth recorded in London in 1940 requires different offset handling than one in 1980. [ISO 8601](https://web.archive.org/web/20260523174424/https://en.wikipedia.org/wiki/ISO_8601) is a standard format for storing and transmitting date and time data, but astrologers often work with legacy formats. A robust system should be able to parse multiple common formats and convert them to a consistent internal representation.

Location is expressed as geographic coordinates. The three common formats are [decimal degrees](https://web.archive.org/web/20260212141905/https://en.wikipedia.org/wiki/Decimal_degrees) (DD, e.g. +40.446 −79.982), [sexagesimal](https://web.archive.org/web/20260524072559/https://en.wikipedia.org/wiki/Sexagesimal) degrees/minutes/seconds (DMS, e.g. 40° 26′ 46″ N 79° 58′ 56″ W), and degrees and decimal minutes (DDM, e.g. 40° 26.767′ N 79° 58.933′ W). All three appear in the wild — in databases, user input, and legacy data. A robust system can parse and convert between them. The full [geographic coordinate system](https://web.archive.org/web/20260312172000/https://en.wikipedia.org/wiki/Geographic_coordinate_system) and [astronomical coordinate systems](https://web.archive.org/web/20260308030932/https://en.wikipedia.org/wiki/Astronomical_coordinate_systems) provide the broader context. [ISO 6709](https://web.archive.org/web/20260524072559/https://en.wikipedia.org/wiki/ISO_6709) is an international standard for representation of latitude, longitude and altitude for geographic point locations. It states that when CRS identification is missing, the data must be interpreted by the following conventions: Latitude comes before longitude, North latitude is positive, East longitude is positive, Fraction of degrees [decimal degrees](https://web.archive.org/web/20260524072559/https://en.wikipedia.org/wiki/Decimal_degrees) is preferred in digital data exchange, while [sexagesimal](https://web.archive.org/web/20260524072559/https://en.wikipedia.org/wiki/Sexagesimal) notation is tolerated for compatibility

## Astronomical Background

The luminaries — Sun and Moon — have been observed and tracked by humans for at least 200,000 years; their astrological significance is as old as recorded human thought. Mercury, Venus, Mars, Jupiter, and Saturn are all visible to the naked eye and were documented in antiquity across multiple cultures independently; their astrological symbolism developed over millennia of observation.

Telescope-era discoveries brought new bodies into the astrological catalog at specific historical moments: Uranus in 1781, Ceres in 1801, Neptune in 1846, Pluto in 1930, Quaoar in 2002, Sedna in 2003, Orcus and Haumea in 2004, Eris and Makemake in 2005, and Gonggong in 2007. The year of discovery matters because it marks when a body became available to astrologers and when interpretive traditions around it began forming.

## Ephemeris Data: Sources and File Formats

A body's position comes from an **ephemeris** — a dataset of precomputed positions, or of coefficients from which positions are reconstructed. Sourcing this data is a real engineering task, and the landscape has traps worth knowing.

The authoritative source is JPL's **Development Ephemeris (DE) series** (NASA Jet Propulsion Laboratory, Solar System Dynamics group). Each release carries a number: **DE440** is the modern high-precision fit; **DE441** is its long-span companion, covering roughly **−13200 … +17191** (≈13,200 BCE to 17,191 CE). A higher number is a newer fit, not automatically "better" — the covered span and the release both matter.

JPL exposes DE data through **two distinct channels**, and conflating them causes design mistakes:

1. **Bulk static files — the SSD FTP tree** (served over HTTPS at `https://ssd.jpl.nasa.gov/ftp/eph/…`). Whole-ephemeris files that already exist on the server; you download them verbatim. This is how you get the full planetary/lunar ephemeris.
2. **The Horizons API** (`https://ssd.jpl.nasa.gov/api/horizons.api`). An on-demand service: you *query* for one body over one time span and it **generates** the answer (an SPK file or a table). This is how you get individual small bodies — asteroids, comets, TNOs — that are not in the bulk planetary set.

**The same ephemeris ships in several file formats — identical numbers, different container.** The suffix and filename encode meaning:

- **`.NNN` ASCII** (`ascp*.441`, `ascm*.441`, plus a `header.441`) — human-readable Chebyshev coefficient blocks. The numeric suffix is the DE version (`.441` = DE441). The `ascp` / `ascm` split separates future (positive years) from past (negative years) epochs.
- **`.NNN` platform binary** (`linux_*.441`, with Mac/PC-endian variants, plus `header.441`) — the endian-packed binary of the same coefficients. The **filename encodes the span**: `linux_m13000p17000.441` = −13000 … +17000; `linux_p1550p2650.441` = 1550 … 2650. `m` / `p` are minus / plus years.
- **`.bsp` (SPICE SPK)** — the NAIF SPICE toolkit's kernel format. `de441.bsp` is the full-span SPK; `de441s.bsp` is the "short" 1550–2650 SPK. Small-body ephemerides are distributed as SPK as well (e.g. `sb441-n16.bsp`, `sb441-n373.bsp` — bundled asteroid-perturber sets — or per-body kernels keyed by NAIF ID). A `.bsp` and a `.441` can hold the **identical** ephemeris in different wrappers; which one an engine reads is an implementation choice — the planetary DE-binary reader and the SPICE-SPK reader are separate code paths.

**Span versus size is a correctness axis, not merely a disk-space one.** Long-span files are large (the DE441 full binary is ~2.8 GB); short-span files are small (a 1550–2650 SPK is ~27 MB) but **hard-error outside their window**. Defaulting to a truncated ephemeris silently breaks charts for ancient nativities, historical mundane work, or deep-time ingresses. For general-purpose astrology software the long span is the safer default; trimming coverage to save bytes is a decision to make deliberately, never by accident.

**Other ephemeris families exist.** The Swiss Ephemeris distributes compressed `.se1` files derived from JPL DE data — a different lineage with its own licensing terms; treat its published documentation as citable and its internals as off-limits. The naked-eye planets and the luminaries are always covered by the DE set; obscure asteroids and freshly discovered TNOs typically require a Horizons fetch.

## Astrology Concepts

The [Rodden Rating System](https://web.archive.org/web/20251126091315/https://www.astro.com/astro-databank/Help:RR), extended by the [Astro-Databank system](https://web.archive.org/web/20251011012415/http://www.astro.com/astro-databank/Help:DataSource), is the standard way to communicate confidence in birth data. Ratings range from AA (birth certificate or equivalent) down through A, B, C, and DD (dirty data). The important nuance is that these systems conflate data source with data confidence — a rating describes where the data came from as much as how trustworthy it is.

[House systems](https://web.archive.org/web/20260312041824/https://en.wikipedia.org/wiki/House_(astrology)#Systems_of_house_division) divide the chart into twelve sectors. Different systems produce different cusp positions from the same birth data, and practitioners disagree vigorously about which is most valid. This means any data format that stores house cusps must record which system was used. It should be noted when constructing a query for multiple placements in the same house, the whole sign house system is the only house system where those placements are guaranteed to be in the same zodiac. In other systems, the same house may span multiple zodiacs, so a query for "stellium in the fifth house" would need to know the house bounds. All house systems require the degree of the Ascendant as an input, and many require the degree of the Midheaven as well.

[Astrological symbols](https://web.archive.org/web/20260312142908/https://en.wikipedia.org/wiki/Astrological_symbols) and [astronomical symbols](https://web.archive.org/web/20260311002659/https://en.wikipedia.org/wiki/Astronomical_symbols) are Unicode glyphs used throughout the domain. They are not decorative — astrologers use them as shorthand in charts, tables, and notation.

This list of symbols supplements or overrides the symbols in the linked articles.

```
⊕  Earth (preferred)
♁  Earth (alternate)
⊗  Lot of Fortune
⏀  Lot of Spirit
♡  Lot of Eros
◬  Cardinal
⊡  Fixed
🜳  Mutable
```

Sect — whether a chart is diurnal or nocturnal — is determined by the position of the Sun relative to the horizon. If the Sun is above the Ascendant/Descendant axis, the chart is diurnal (a day chart). If below, it is nocturnal (a night chart). The boundary has an **interperative** nuance: a chart  **may** **behave**  diurnally if the Sun is within 6° of the Ascendant or within 3° of the Descendant, even if technically below the horizon. A chart that falls within this grace band — Sun below the horizon but inside the 6° Ascendant / 3° Descendant tolerance — is called a **twilight chart**. Sect governs which planets are considered more powerful in a given chart in traditional and Hellenistic practice. This twilight nuance is NEVER considered when calculating any of the hermetic lots. For hermatic lots, sect is a binary condtion, trinary evaluation is not even possible.

Key terms: the **radix** is the root chart — the foundational moment for a native or event, requiring an exact date, time, and location to the best available precision. When time or location is unknown, much can still be inferred, and rectification services exist to estimate a confident time of birth from the native's life events. A **polation**, subchart, event chart, or derived chart is any chart that is a child of the radix — solar returns, secondary progressions, and similar. **Celestial objects** are the luminaries, planets, dwarf planets, asteroids, KBOs, TNOs, and fixed stars — physical bodies with ephemeris data. **Mathematical points** are computed positions that represent no physical object.

## Zodiacal Positions

A zodiacal position is the primary output of any chart calculation. Every celestial object and mathematical point in a chart has a position described by ecliptic longitude measured along the ecliptic from the vernal equinox. Two notations are in common use and must not be confused:

**Absolute longitude** expresses a position as a single angle from 0° to 360°, indexed from 0° Aries. 0° Aries is the zero-point of the circle; each subsequent sign adds 30°. For example, 18° Taurus is 48° in absolute longitude (30° for all of Aries plus 18° into Taurus). When a calculation produces a value above 360°, one full circle has been traversed and 360° is subtracted — 389° and 29° Aries are the same position.

**Zodiacal longitude** expresses a position within its sign as 0–29°, plus minutes and seconds. It is the notation astrologers use in charts and speech: "Sun at 19° Scorpio 25′." The sign name carries the context that absolute longitude encodes numerically.

Software that stores or transmits positions must specify which notation it uses. The two are trivially interconvertible but silently mixing them produces wrong charts.

From absolute longitude we derive the sign (each of the 12 signs occupies 30° of the ecliptic), the degree within that sign (0–29°), and the minute and second within that degree.

**Zodiacal order** is a concept that sorts calculations by their position in the zodiac, rather than by their order in the sky. For example, if the Sun is in Pisces and Mercury is in Aries, the zodiacal order is Mercury first, then the Sun. This is important for certain interpretive techniques and for consistent display. It also applies to sorting the zodiacs themselves, where the signs are always in the same order when ascending by absolute longitude.

Beyond longitude, physical bodies (not mathematical points) also have ecliptic latitude — their angular distance north or south of the ecliptic plane. A body exactly on the ecliptic has 0° latitude; most planets wander slightly. Mathematical points like the Ascendant and Midheaven lie by definition on the ecliptic and have no meaningful latitude.

Declination is the body's angular distance north or south of the celestial equator. It is used in parallel aspects and out-of-bounds calculations (any body beyond ±23°27′ is "out of bounds").

Daily speed is how far a body moves along the ecliptic in one day, in degrees. When a planet's speed is negative, it is retrograde — appearing to move backward against the backdrop of stars due to the geometry of Earth's orbit relative to the planet's. The retrograde flag is a boolean derived from the sign of the speed value. Stations (near-zero speed) deserve special handling.

A complete position record for a physical body therefore includes: ecliptic longitude (decimal degrees or DMS), sign, degree/minute/second within sign, ecliptic latitude, declination, daily speed, and retrograde flag. Mathematical points omit latitude.

## Placements and Bodies

A **placement** is any positionable thing in a chart — celestial objects and mathematical points alike. The term covers the full set of computed positions a chart contains, without distinguishing between physical and mathematical origin.

A **body** is a physical object whose position requires ephemeris data: a luminary, planet, dwarf planet, asteroid, centaur, KBO, TNO, or fixed star. Bodies have latitude, declination, daily speed, and a retrograde flag. Mathematical points do not — they are not bodies.

## Celestial Objects

Astrology software must handle a wide and growing catalog of objects. They fall into meaningful categories that inform both calculation and interpretation.

**The Luminaries** — the Sun (☉) and Moon (☽) — are the most fundamental objects in every tradition. They move fastest (Sun ~1°/day, Moon ~13°/day) and anchor the chart's sect determination.

**The Classical Planets** are the five naked-eye planets known to antiquity: Mercury (☿), Venus (♀), Mars (♂), Jupiter (♃), and Saturn (♄). They form the core of traditional and Hellenistic astrology. Every house system and dignities framework is built around these seven bodies (luminaries + five planets).

**The Modern Planets** were discovered with telescopes: Uranus (♅, 1781), Neptune (♆, 1846), and Pluto (♇, 1930). Western modern astrology incorporates all three. Pluto was reclassified as a dwarf planet in 2006 but retains its place in most astrological practice.

**The Dwarf Planets** are a growing class. Ceres (⚳, 1801) is the largest object in the asteroid belt and is used in many modern charts. The trans-Neptunian dwarf planets — Quaoar (2002), Sedna (2003), Orcus (2004), Haumea (2004), Eris (2005), Makemake (2005), and Gonggong (2007) — are increasingly included in contemporary practice. Their slow movement means generational rather than personal significance.

**The Major Asteroids** — Chiron (⚷), Pallas (⚴), Juno (⚵), and Vesta (⚶) — are commonly included in modern natal work. Chiron (technically a centaur, not an asteroid) orbits between Saturn and Uranus and carries particular interpretive weight.

## Mathematical Points

Mathematical points are computed from the chart's time and location, not from ephemeris data. They have ecliptic longitude but no latitude.

**The Angles** are the four cardinal points of the chart. The Ascendant (Asc) is the degree of the ecliptic rising on the eastern horizon; it defines the cusp of the first house in most systems. The Descendant (Desc) is directly opposite, the western horizon point. The Midheaven or Medium Coeli (MC) is the degree of the ecliptic at the meridian; the Imum Coeli (IC) is its opposite. Together the Asc/Desc and MC/IC form two axes. In a data model, storing each axis as a pair (or as a single longitude from which the opposite is derived at 180°) is a reasonable choice.

**The Vertex and Antivertex** form a third axis, computed from the intersection of the prime vertical with the ecliptic in the western hemisphere. Less universally used but present in many software implementations.

**The Lunar Nodes** are the points where the Moon's orbit crosses the ecliptic. The North Node (☊, also called the Dragon's Head) and South Node (☋, Dragon's Tail) are always exactly opposite each other. Both True Node (the actual geometric intersection, which oscillates) and Mean Node (a smoothed average) are in common use. A data format should be able to store which calculation method was used.

**The Arabic Lots** (also called Parts) are computed from the longitudes of two planets and the Ascendant using a tradition-specific formula. The Lot of Fortune (⊕) and Lot of Spirit are foundational in Hellenistic practice; the formula differs between day and night charts. The Lot of Eros is also among the minimum set. Many other lots exist (Necessity, Courage, Victory, etc.). A lot has only a longitude — no latitude, no speed, no retrograde.

## House Cusps

A house system divides the chart into twelve sectors. Each sector boundary is a cusp, defined by an ecliptic longitude. The first cusp (the cusp of the first house) is always the Ascendant. The seventh cusp is the Descendant. The tenth cusp is the MC. The fourth cusp is the IC. The other eight cusps vary by system.

**Whole Sign Houses** is the oldest known system. The sign containing the Ascendant becomes the entire first house; each subsequent sign is the next house. Cusps in whole sign are always 0° of a sign — the system is purely sign-based, not degree-based. This means the MC may fall in any house, not necessarily the tenth. Storing whole sign cusps means storing the Ascendant sign (from which all others follow) rather than twelve individual degree positions, though software may still emit all twelve for uniformity. Whole Sign House cusps must not contain "floating point noise" 

**Placidus** is the most common modern system in Western practice. It divides the semi-arcs of the sky (the time a point spends above or below the horizon) into thirds. It fails to produce cusps for locations above the Arctic Circle where some degrees of the ecliptic never rise or set. Cusps are specific ecliptic degrees.

**Equal House** places cusps every 30° from the Ascendant. Simple and unambiguous; the MC floats freely and is tracked separately. Common in older British practice and in some Vedic-influenced approaches.

Other systems in common use include Koch, Regiomontanus, Campanus, Morinus, and Porphyry. A robust data format should store which house system was used, and ideally store multiple house systems simultaneously since practitioners frequently compare them.

## Aspects

An aspect is an angular relationship between two points in a chart. When two planets are separated by a significant angle, they are said to be in aspect. The significance of the angle and the tolerance allowed (the orb) vary by tradition.

**Major aspects** are universally recognized: conjunction (0°), opposition (180°), trine (120°), square (90°), and sextile (60°). These form the backbone of all astrological interpretation.

**Minor aspects** include the semi-sextile (30°), quincunx or inconjunct (150°), semi-square (45°), sesquiquadrate (135°), quintile (72°), and biquintile (144°). Their use varies widely; many traditional practitioners ignore them entirely.

**Orb** is the allowed deviation from the exact angle. A conjunction with 8° orb means two planets within 8° of each other are considered conjunct. Orbs are contested: traditional practice often assigns larger orbs to the Sun and Moon and smaller to the outer planets; Hellenistic practice uses whole-sign aspects (any two planets in aspect signs regardless of degree) rather than degree-based orbs. Modern practice varies by practitioner. A data format storing aspects should record the exact angular separation alongside the aspect type, leaving orb interpretation to the application layer.

Aspects can be **applying** (the two planets are moving toward exact aspect) or **separating** (moving away). This is determined by comparing their speeds and positions. Applying aspects are generally considered stronger in traditional practice.

## Chart Types Beyond the Radix

The radix is the foundation, but practitioners regularly work with derived and time-based charts.

**Solar Return** is cast for the moment the transiting Sun returns to its natal degree each year. It uses the native's current location or birth location depending on tradition. It produces a full chart with its own Ascendant, houses, and planetary positions for that moment.

**Lunar Return** is the monthly equivalent — cast for when the transiting Moon returns to its natal degree.

**Secondary Progressions** advance the chart symbolically: one day of ephemeris movement after birth corresponds to one year of life. A progressed chart has its own set of planetary positions derived from the ephemeris, not from a real sky moment.

**Solar Arc Directions** move every point in the chart forward by the same arc — equal to the amount the progressed Sun has moved from the natal Sun. Every planet and angle advances together.

**Transits** are the current sky positions overlaid on the natal chart. A transit chart is simply an ephemeris snapshot for a given date/time/location, interpreted in relation to the radix.

**Synastry** compares two charts — typically two people's radices — by overlaying their planetary positions. **Composite** blends two charts into one by taking the midpoints of corresponding planets.

All of these share the same positional data structure as the radix. The key distinctions for a data format are: what reference chart they derive from (linking back to a radix uuid), what technique was used to calculate them, and for location-sensitive techniques, what location was used.
