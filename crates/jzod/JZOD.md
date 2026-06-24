# JZOD

An open format for storage, transmission, and processing of astrology chart data.

- File suffix: `.jzod` or `.json`
- Encoding: UTF-8 (implementations must support full Unicode / UTF-8 mb4)
- Key naming: `lower_snake_case`
- Versioning: [Semantic Versioning](https://semver.org/), starting at `0.0.0`
- Schema: JSON Schema (this directory)
- Unknown keys are ignored — forward compatibility by design

> **Status: `0.0.0` — unstable.** Fields, types, and structure may change without notice until `1.0.0`.

---

## Table of Contents

1. [Design Goals](#design-goals)
2. [File Structure](#file-structure)
3. [Chart Object](#chart-object)
   - [Chart Types](#chart-types)
   - [Nesting Model](#nesting-model)
   - [Core Fields](#core-fields)
4. [Birth Data](#birth-data)
5. [Ephemeris Metadata](#ephemeris-metadata)
6. [Chart-Level Fields](#chart-level-fields)
7. [Placements](#placements)
   - [Coordinate Notation](#coordinate-notation)
   - [Bodies](#bodies)
   - [Angles](#angles)
   - [Points](#points)
   - [Lots](#lots)
8. [Houses](#houses)
9. [Lunar Phase](#lunar-phase)
10. [Relationships](#relationships)
    - [Relationship Codes](#relationship-codes)
11. [Views](#views)
12. [Enumerated Values](#enumerated-values)
13. [Minimally Calculated Radix](#minimally-calculated-radix)
14. [Compliance](#compliance)
15. [Decision Chronology](#decision-chronology)
16. [Open Questions](#open-questions)
    - [OQ-1 Relationship placement](#oq-1--relationship-placement-top-level-array-vs-per-chart)
    - [OQ-2 Relationship codes](#oq-2--relationship-codes-technical-detail)
    - [OQ-3 sub_charts naming](#oq-3--sub_charts-array-naming) ✓
    - [OQ-4 Zodiac beyond tropical](#oq-4--zodiac-field-extending-beyond-tropical) ✓
    - [OQ-5 Rodden decoupling](#oq-5--data-confidence-decoupling-rodden-rating)
    - [OQ-6 Vedic fields](#oq-6--vedic--jyotish-fields)
    - [OQ-7 Literal nesting vs uid-reference](#oq-7--should-derivative-charts-be-literally-nested-or-uid-referenced)
    - [OQ-8 JZOD acronym](#oq-8--jzod-acronym)
    - [OQ-9 Aspects](#oq-9--aspects)
    - [OQ-10 Notes and biography](#oq-10--notes-and-biography-fields)
    - [OQ-11 Settings object](#oq-11--settings-object-for-software-specific-data)
    - [OQ-12 Research query goal](#oq-12--research-query-design-goal)
    - [OQ-13 Declination](#oq-13--declination-field-on-bodies)
    - [OQ-14 Topocentric coordinate system](#oq-14--topocentric-coordinate-system)
    - [OQ-15 Black Moon Lilith mean vs. true](#oq-15--black-moon-lilith-mean-vs-trueosculating)
    - [OQ-16 Lunar nodes mean vs. true](#oq-16--lunar-nodes-mean-vs-trueosculating)
    - [OQ-17 Julian dates and distance_au](#oq-17--julian-dates-and-heliocentric-distance)
    - [OQ-18 equal_asc vs equal house slug](#oq-18--house-system-slug-equal_asc-vs-equal)
    - [OQ-19 Variant placement schema (mean vs true)](#oq-19--variant-placement-schema-mean-vs-trueosculating)
    - [OQ-20 Calendar system on datetime](#oq-20--calendar-system-field-on-datetime)
    - [OQ-21 LMT longitude annotation](#oq-21--lmt-longitude-annotation-on-datetime)
    - [OQ-22 CBOR binary encoding](#oq-22--cbor-binary-encoding-rfc-8949)
16. [Example](#example)
17. [jq Query Examples](#jq-query-examples)
18. [Versioning](#versioning)

---

## Design Goals

JZOD is designed to be queryable without special tools or proprietary infrastructure. Three commitments serve this goal:

**Ordinary JSON.** A JZOD file is valid JSON. Any tool, language, or platform that reads JSON can read JZOD — `jq`, Python's `json` module, JavaScript `JSON.parse`, R's `jsonlite`, database JSON parsers, and so on. No custom parser required.

**Pre-computed placements.** Positions, house assignments, lots, and (future) aspects are stored in the file rather than computed on demand. A query asking "which charts have Jupiter in the 8th house by Placidus?" can be answered by traversing data — no ephemeris engine required at query time.

**Portable across query targets.** The same JZOD data is queryable from any environment that handles JSON:

| query target | usage |
|---|---|
| Flat files | `jq` queries on `.jzod.json` files |
| Document stores | MongoDB, CouchDB, Firestore — one chart or one file per document |
| Relational JSONB | PostgreSQL `jsonb` columns via `->` and `@>` operators |
| Object stores | S3 + Athena, GCS + BigQuery — JZOD files as queryable objects |
| NoSQL | DynamoDB, Cassandra with JSON support |
| REST and GraphQL APIs | JZOD objects map to API responses without transformation |

**Qualify discriminators on first arrival.** If a bare string field has named, structurally distinct peers — even ones not yet supported — qualify it into an object on introduction and enumerate all known siblings, even if undefined. An unqualified string that later acquires peers with different shapes is a breaking change. This principle applies only to bare string discriminators; object fields are self-protecting because new sibling keys are always additive.

**Compute all siblings.** When computing any member of a family of related values, compute the whole family. One house system → all house systems. One node variant → mean and true/osculating both. One BML variant → mean and true both. This maximises the utility of each computation pass and means a JZOD file is a complete reference artifact rather than a partial snapshot. The scope boundary for this principle is the minimally calculated radix — see §[Minimally Calculated Radix](#minimally-calculated-radix). Bodies beyond that boundary (dwarf planets, centaurs, trans-Neptunians, fixed stars) are explicitly out of scope for the reference format.

See §[jq Query Examples](#jq-query-examples) for working single-chart queries and [OQ-12](#oq-12--research-query-design-goal) for the cross-chart research query goal.

---

## File Structure

```json
{
  "version": "0.0.0",
  "charts": [],
  "relationships": [],
  "views": []
}
```

| field | type | required | description |
|---|---|---|---|
| `version` | string | yes | Semver string. Parsers MUST NOT reject unknown versions — ignore gracefully. |
| `charts` | array | yes | Top-level chart objects: standalone radixes, standalone independent charts, and composite charts. Derivative charts live nested inside their parent, not here. |
| `relationships` | array | no | Graph edges between chart `uid`s. See §Relationships. |
| `views` | array | no | Display-only wrappers (biwheels, triwheels). No computed data. See §Views. |

---

## Chart Object

Every chart object carries a `type` field. `type` determines what the chart represents semantically. Whether a chart is top-level or nested is determined by its structural category (see §Nesting Model).

```json
{
  "uid": "a3f8c2d1-6b94-4e17-8f53-2c71d0b43e85",
  "type": "radix",
  "name": { "display": "Anna Freud", "aliases": [] },
  "gender": "f",
  "rodden_rating": "AA",
  "birth": { ... },
  "zodiac": { "name": "tropical" },
  "coordinate_system": "geocentric",
  "sect": "diurnal",
  "ephemeris": { ... },
  "placements": { ... },
  "houses": { ... },
  "lunar_phase": { "synodic_arc_deg": 142.7, "phase": "gibbous", "lunation_day": 12 },
  "nested": []
}
```

### Chart Types

| `type` | structural category | notes |
|---|---|---|
| `radix` | independent | Birth/natal/root chart. Covers individuals and entities (nations, companies, projects). Entity radix has no gender. |
| `event` | independent | Standalone historical event, or nested as biographical entry on a radix. |
| `horary` | independent | Chart for the moment a question is posed. Standalone, or optionally nested under the querant's radix. |
| `election` | independent | Chart for choosing an auspicious moment. First-class; ideally nested. |
| `decumbiture` | independent | Chart for the onset of illness. First-class; ideally nested. |
| `mundane` | independent | Ingress, lunation, eclipse, or national chart. Standalone, or optionally nested under an entity radix. |
| `transit` | independent | Ephemeris snapshot for a given moment. Standalone (e.g. a saved eclipse chart), or nested against a radix. |
| `solar_return` | derivative | Cast for the moment the transiting Sun returns to its natal degree. Always nested inside its parent radix. |
| `lunar_return` | derivative | Cast for the moment the transiting Moon returns to its natal degree. Always nested inside its parent radix. |
| `secondary_progression` | derivative | One day of ephemeris after birth = one year of life. Always nested. |
| `tertiary_progression` | derivative | One day of ephemeris after birth = one month of life. Always nested. |
| `solar_arc` | derivative | All points advance equally by the arc the progressed Sun has moved. Always nested. |
| `relocated` | derivative | Same datetime as parent, different location. Always nested. |
| `composite` | two-parent | Midpoint blend of two radixes. Top-level, references two parents via `parent_uids`. |
| `chart` | independent | Generic fallback for conversion tools that cannot determine chart type. |

### Nesting Model

> **Note:** Whether derivative charts should be literally nested or uid-referenced is under active discussion — see [OQ-7](#oq-7--should-derivative-charts-be-literally-nested-or-uid-referenced). The current spec reflects literal nesting.

JZOD makes a structural distinction between **independent** and **derivative** charts.

**Derivative charts** require a parent radix as computational input. They are stored as literal JSON objects inside the parent's `nested` array. They do not appear in the top-level `charts` array. Their parent relationship is implicit — they are physically inside it.

**Independent charts** do not require another chart as input. A radix, event, horary, transit, etc. can stand alone. However, independent charts are often meaningfully associated with a radix (e.g. a horary question posed by the native, a notable event in their biography). When that association exists, the independent chart SHOULD be stored inside the radix's `nested` array. When no parent context applies, they appear at the top level of `charts`.

**Composite charts** require exactly two parent radixes. They appear at the top level of `charts` and reference their parents via `parent_uids: [uid_a, uid_b]`.

Summary:

| chart is... | lives in... |
|---|---|
| derivative | `nested` array inside its parent radix |
| independent with a natural parent | `nested` array inside that parent radix (preferred) |
| independent with no parent | top-level `charts` array |
| composite | top-level `charts` array, with `parent_uids` |

A `nested` array on a radix can hold any mix of derivative and independent charts:

```json
{
  "uid": "radix-uid",
  "type": "radix",
  ...
  "nested": [
    { "uid": "sr-uid", "type": "solar_return", ... },
    { "uid": "event-uid", "type": "event", "name": { "display": "First Home Purchase" }, ... },
    { "uid": "prog-uid", "type": "secondary_progression", ... }
  ]
}
```

Composite charts reference their parent radixes by `uid` rather than nesting, since they have two parents:

```json
{
  "uid": "composite-uid",
  "type": "composite",
  "parent_uids": ["radix-uid-a", "radix-uid-b"],
  ...
}
```

### Core Fields

#### `uid`

UUID v4. Unique within a JZOD file. For data originating in other software, use the software's native ID if it is a UUID v4, or generate a new UUID v4 and record the original ID in `notes` or a software-specific `settings` block.

#### `type`

See chart types table above.

#### `name`

```json
"name": {
  "display": "Lana Del Rey",
  "aliases": ["Elizabeth Woolridge Grant", "Lizzy Grant"]
}
```

`display` is the current or colloquial name. `aliases` captures historical, legal, birth, married, and stage names. For events and entities, `display` is the current common name; `aliases` can include superseded names (e.g. an event later reclassified by historians).

#### `gender`

| value | meaning |
|---|---|
| `"m"` | male |
| `"f"` | female |
| `"x"` | nonbinary |
| `"a"` | any / unspecified |
| _(absent)_ | entity chart (nation, company, project) — gender does not apply |
| any string | user-supplied self-description |

#### `rodden_rating`

Follows the Astro-Databank system. Encodes both data source and confidence level.

| rating | meaning |
|---|---|
| `AA` | Birth certificate or record in hand |
| `A` | From memory or news report |
| `B` | Biography or autobiography |
| `C` | Caution — no source given |
| `DD` | Dirty data — conflicting sources |
| `X` | No birth time |
| `XX` | No date or time |

---

## Birth Data

```json
"birth": {
  "datetime": {
    "year": 1895,
    "month": 12,
    "day": 3,
    "hour": 15,
    "minute": 15,
    "second": 0,
    "utc_offset": "+01:00",
    "iana_tz": "Europe/Vienna",
    "unknown": false
  },
  "location": {
    "name": "Vienna, Austria",
    "latitude": 48.208333,
    "longitude": 16.371667
  }
}
```

### `datetime`

| field | type | notes |
|---|---|---|
| `year` | integer | Signed. Negative = BCE. |
| `month` | integer | 1–12 |
| `day` | integer | 1–31 |
| `hour` | integer | 0–23 |
| `minute` | integer | 0–59 |
| `second` | integer | 0–59 |
| `utc_offset` | string | **Authoritative.** The offset actually used for ephemeris calculation. Format: `+HH:MM` or `-HH:MM`. |
| `iana_tz` | string | Informational only. IANA tz database identifier (e.g. `"Europe/Vienna"`). Useful for display and debugging. Do not use for calculation. |
| `unknown` | boolean | If `true`, the time of day is not known. When `true`, `tod_method` must be present. |
| `tod_method` | string | Present only when `unknown` is `true`. Pre-defined values: `"sunrise"`, `"chandra_lagna"` (moonrise), `"noon"`. Default when importing a timeless chart: `"sunrise"`. |

**`utc_offset` vs. `iana_tz`:** When these two fields disagree (e.g. around DST transitions or historical timezone changes), `utc_offset` governs. `iana_tz` is a human-readable hint, not a calculation input.

### `location`

| field | type | notes |
|---|---|---|
| `name` | string | Human-readable place name. |
| `latitude` | number | Decimal degrees. ISO 6709: North positive. |
| `longitude` | number | Decimal degrees. ISO 6709: East positive. |

---

## Ephemeris Metadata

```json
"ephemeris": {
  "source": "DE441",
  "calculated_at": "2026-06-08T20:45:18Z"
}
```

| field | notes |
|---|---|
| `source` | Ephemeris source identifier. Known values: `"DE441"`. |
| `calculated_at` | ISO 8601 UTC timestamp of calculation. |

---

## Chart-Level Fields

| field | values | notes |
|---|---|---|
| `zodiac` | object | `name` field is required. `"tropical"` — anchored to the vernal equinox. `"sidereal"` — adds `ayanamsha` field. `"draconic"` — anchored to the North Node. Additional names to be specified. |
| `coordinate_system` | `"geocentric"`, `"topocentric"`, `"heliocentric"` | `"topocentric"` = parallax-corrected for observer's surface location; affects Moon by up to ~1°. |
| `sect` | `"diurnal"`, `"nocturnal"`, `"unknown"` | Sun above the horizon = diurnal. Use `"unknown"` when the birth time is unknown (`datetime.unknown` is `true`), since sect cannot be trusted from a placeholder time-of-day. **Omit the field entirely for heliocentric charts** — sect is geocentric by definition (Sun relative to the local horizon) and has no meaning without an Ascendant. |

---

## Placements

Placements are the computed positions of celestial bodies and mathematical points.

### Coordinate Notation

Two notations coexist. Both are present for convenience — they encode the same value and are trivially interconvertible.

| notation | example | description |
|---|---|---|
| Absolute longitude | `251.206` | Degrees from 0° Aries, range 0–360. |
| Zodiacal | sign `"sagittarius"`, degree `11`, minute `12`, second `21` | Position within the sign, 0–29°. |

Conversion: `absolute_longitude = sign_index × 30 + degree + minute/60 + second/3600`
where sign index: aries=0, taurus=1, … pisces=11.

### Bodies

Celestial objects with physical ephemeris data.

```json
{
  "id": "sun",
  "ecliptic_longitude": 251.206,
  "sign": "sagittarius",
  "degree": 11,
  "minute": 12,
  "second": 21,
  "ecliptic_latitude": -0.002,
  "daily_speed": 1.015,
  "retrograde": false,
  "house": { "whole_sign": 8, "placidus": 7 }
}
```

| field | notes |
|---|---|
| `id` | Body identifier. See §Enumerated Values. |
| `ecliptic_longitude` | Absolute longitude, 0–360°. |
| `sign` | Sign name slug. |
| `degree` | 0–29, degree within sign. |
| `minute` | 0–59 |
| `second` | 0–59 |
| `ecliptic_latitude` | Angular distance north (positive) or south (negative) of the ecliptic plane. Bodies only — angles and lots have no latitude. |
| `daily_speed` | Degrees per day. Negative = retrograde. |
| `retrograde` | Boolean. Derived from the sign of `daily_speed`. |
| `house` | Object mapping house system slug → house number (1–12). |

### Angles

Mathematical axis points computed from time and location. No latitude, no speed, no retrograde.

```json
{
  "id": "ascendant",
  "ecliptic_longitude": 58.26166755,
  "sign": "taurus",
  "degree": 28,
  "minute": 15,
  "second": 42
}
```

### Points

Mathematical points that have a `retrograde` flag but no latitude and no daily speed.

```json
{
  "id": "north_node",
  "ecliptic_longitude": 338.02621501,
  "sign": "pisces",
  "degree": 8,
  "minute": 1,
  "second": 34,
  "retrograde": true
}
```

### Lots

Arabic/Hermetic lots. Longitude only — no latitude, no speed, no retrograde.

```json
{
  "id": "lot_of_fortune",
  "ecliptic_longitude": 254.99609674,
  "sign": "sagittarius",
  "degree": 14,
  "minute": 59,
  "second": 45
}
```

---

## Houses

House cusps, keyed by house system slug then by house number. House number keys are strings (`"1"`–`"12"`) due to JSON object key constraints.

```json
"houses": {
  "whole_sign": {
    "1": { "longitude": 30.0, "sign": "taurus", "degree": 0, "minute": 0, "second": 0 }
  },
  "placidus": {
    "1": { "longitude": 58.26166755, "sign": "taurus", "degree": 28, "minute": 15, "second": 42 }
  }
}
```

**Whole Sign constraint:** Whole sign house cusps are always exactly 0° of a sign (`"degree": 0, "minute": 0, "second": 0`). This is a hard invariant of the system, not a rounding artifact. Implementations MUST NOT write floating-point noise into whole sign cusps.

---

## Lunar Phase

The Moon's position in the synodic cycle relative to the Sun, computed when both luminaries are present.

```json
"lunar_phase": {
  "synodic_arc_deg": 142.7,
  "phase": "gibbous",
  "lunation_day": 12
}
```

| field | type | required | description |
|---|---|---|---|
| `synodic_arc_deg` | number | yes | Moon−Sun ecliptic elongation in degrees, range `[0, 360)`. `0°` = exact conjunction (new moon), `180°` = opposition (full moon). |
| `phase` | string | yes | The traditional 8-fold phase name (45° octants). One of: `new_moon`, `crescent`, `first_quarter`, `gibbous`, `full_moon`, `disseminating`, `last_quarter`, `balsamic`. |
| `lunation_day` | integer | yes | 1-indexed position within the 28-fold lunar month, range `1–28`. |

The whole field is `null` when a lunar phase is undefined for the chart — heliocentric charts, or any chart where the Sun or Moon is absent from `placements`:

```json
"lunar_phase": null
```

---

## Relationships

The `relationships` array is top-level in the JZOD file. It owns the graph edges between charts — neither chart duplicates the relationship data.

```json
"relationships": [
  {
    "uid": "rel-uuid-here",
    "chart_uid": "chart-abc",
    "foreign_uid": "chart-def",
    "code": "spouse",
    "bond": "crucial",
    "display_name": "Mileva Marić",
    "notes": null
  }
]
```

| field | type | notes |
|---|---|---|
| `uid` | string | UUID v4 for this relationship record. |
| `chart_uid` | string | The chart this relationship is declared from. Must resolve to a chart in this file. |
| `foreign_uid` | string | The related chart's `uid`. May point to a chart not present in this file (orphaned link). Parsers MUST NOT fail on unresolvable `foreign_uid`. |
| `code` | string | Relationship type slug. See §Relationship Codes. |
| `bond` | string | `"crucial"` or `"loose"`. Crucial = computationally or biographically significant (e.g. composite inputs, spouse). Loose = contextual or background connection. |
| `display_name` | string \| null | Optional safety copy of the foreign chart's display name at time of linking. Useful when the foreign chart is not present in the file. |
| `notes` | string \| null | Free text. |

**Compliance note:** A JZOD-compliant implementation SHOULD maintain bidirectional consistency — when creating or deleting a relationship from chart A to chart B, it SHOULD update chart B's counterpart relationship entry if chart B is present in the same dataset. Orphaned relationships (where the foreign chart is absent) MUST be preserved and MUST NOT cause parse errors.

### Relationship Codes

Organized by domain using the house system as a taxonomy. The codes are human-readable slugs — not integers. For every gendered relationship, a neuter form is provided and preferred where gender is unknown or not applicable.

Use `"other"` with a `notes` field for relationships not covered here.

**Self / Identity**

| code | notes |
|---|---|
| `self` | The chart represents the native themselves. Used to link a composite back to its source inputs. |
| `alternate_chart` | Another chart for the same entity (e.g. rectified vs. original birth time). |

**Belongings / Resources**

| code | notes |
|---|---|
| `owned_entity` | A business, property, or asset the native owns or founded. |
| `owner` | The person or entity that owns or controls the subject of this chart. |

**Siblings / Immediate Circle**

| code | notes |
|---|---|
| `sibling` | neuter |
| `brother` | |
| `sister` | |
| `half_sibling` | neuter |
| `step_sibling` | neuter |
| `neighbor` | |
| `colleague` | Immediate working peer. |

**Parents / Family / Home**

| code | notes |
|---|---|
| `parent` | neuter |
| `mother` | |
| `father` | |
| `grandparent` | neuter |
| `grandmother` | |
| `grandfather` | |
| `step_parent` | neuter |
| `foster_parent` | neuter |
| `adoptive_parent` | neuter |
| `ancestor` | Earlier in family lineage; generation unspecified. |

**Children / Creativity**

| code | notes |
|---|---|
| `child` | neuter |
| `son` | |
| `daughter` | |
| `step_child` | neuter |
| `foster_child` | neuter |
| `adoptive_child` | neuter |

**Subordinates**

| code | notes |
|---|---|
| `employee` | |
| `assistant` | |
| `apprentice` | |
| `pet` | |

**Committed Partnerships**

| code | notes |
|---|---|
| `spouse` | neuter |
| `husband` | |
| `wife` | |
| `partner` | Long-term committed partner, not legally married. |
| `cofounder` | Business co-founder; lifelong professional bond. |
| `business_partner` | Formal equal partnership. |
| `composite_input` | This chart is one of two inputs used to calculate a composite. |
| `composite_counterpart` | The other input chart in the composite pair. |
| `composite_output` | The composite chart produced from this chart and its counterpart. |

**Joint Ventures / Shared Resources**

| code | notes |
|---|---|
| `joint_venture` | Shared undertaking, not a formal partnership. |
| `heir` | Intended inheritor. |
| `benefactor` | Person or entity providing significant resources. |

**Education / Philosophy**

| code | notes |
|---|---|
| `teacher` | neuter |
| `student` | neuter |
| `mentor` | |
| `mentee` | |
| `spiritual_guide` | neuter |

**Career / Public Life**

| code | notes |
|---|---|
| `employer` | |
| `manager` | |
| `patron` | Person or institution sponsoring the native's work. |
| `organization` | Chart of an organization the native is significantly associated with. |

**Friends / Groups / Memberships**

| code | notes |
|---|---|
| `friend` | |
| `associate` | Looser social or professional connection. |
| `group_member` | Shared membership in a group or movement. |

**Extended Family**

| code | notes |
|---|---|
| `parent_sibling` | Aunt or uncle — neuter. |
| `aunt` | |
| `uncle` | |
| `sibling_child` | Niece or nephew — neuter. |
| `niece` | |
| `nephew` | |
| `cousin` | neuter |

**Fallback**

| code | notes |
|---|---|
| `other` | Use with `notes` to describe the relationship when no canonical code applies. |

---

## Views

The `views` array holds display-only wrappers for biwheels, triwheels, and quadwheels. A view contains no computed chart data — it only references existing charts by `uid`. This prevents data drift: if a referenced chart is updated, the view automatically reflects it.

```json
"views": [
  {
    "uid": "view-uuid-here",
    "type": "biwheel",
    "label": "Native + Eclipse",
    "subject_uids": ["chart-uid-1", "chart-uid-2"]
  }
]
```

| field | notes |
|---|---|
| `uid` | UUID v4. |
| `type` | `"biwheel"`, `"triwheel"`, `"quadwheel"`. |
| `label` | Human-readable label for this display configuration. |
| `subject_uids` | Ordered list of chart `uid`s. Order is significant: first entry is innermost wheel. |

---

## Enumerated Values

### Body IDs

**Luminaries:** `sun`, `moon`

**Classical Planets:** `mercury`, `venus`, `mars`, `jupiter`, `saturn`

**Modern Planets:** `uranus`, `neptune`, `pluto`

**Dwarf Planets:** `ceres`, `quaoar`, `sedna`, `orcus`, `haumea`, `eris`, `makemake`, `gonggong`

**Major Asteroids:** `chiron`, `pallas`, `juno`, `vesta`, `hygiea`

**Centaurs:** `pholus`, `nessus`, `chariklo`, `asbolus`

**Kuiper-belt Objects:** `ixion`, `varuna`, `albion`

### Angle IDs
`ascendant`, `descendant`, `midheaven`, `imum_coeli`

### Point IDs
`vertex`, `anti_vertex`, `north_node_mean`, `north_node_true`, `south_node_mean`, `south_node_true`, `black_moon_lilith_mean`, `black_moon_lilith_true`, `priapus_mean`, `priapus_true`

### Lot IDs
`lot_of_fortune`, `lot_of_spirit`, `lot_of_eros`, `lot_of_exaltation`, `lot_of_necessity`, `lot_of_courage`, `lot_of_victory`, `lot_of_nemesis`

### House System Slugs
`whole_sign`, `placidus`, `equal_asc`, `equal_mc`, `regiomontanus`, `porphyry`, `campanus`, `koch`, `morinus`, `meridian`, `topocentric`, `krusinski`, `alcabitius`, `sripati`, `horizontal`

### Sign Slugs
`aries`, `taurus`, `gemini`, `cancer`, `leo`, `virgo`, `libra`, `scorpio`, `sagittarius`, `capricorn`, `aquarius`, `pisces`

---

## Minimally Calculated Radix

A radix is considered **minimally calculated** (reference format) when it contains zodiacal positions for all of the following:

- The luminaries: `sun`, `moon`
- The classical planets: `mercury`, `venus`, `mars`, `jupiter`, `saturn`
- The modern planets: `uranus`, `neptune`, `pluto`
- The Ascendant/Descendant axis
- The Midheaven/Imum Coeli axis
- The lunar nodal axis — both mean (`north_node_mean`, `south_node_mean`) and true (`north_node_true`, `south_node_true`)
- The Vertex/Antivertex axis
- Black Moon Lilith / Priapus — both mean (`black_moon_lilith_mean`, `priapus_mean`) and true (`black_moon_lilith_true`, `priapus_true`)
- The 8 Hermetic lots: `lot_of_fortune`, `lot_of_spirit`, `lot_of_eros`, `lot_of_exaltation`, `lot_of_necessity`, `lot_of_courage`, `lot_of_victory`, `lot_of_nemesis`
- House cusps for all enumerated house systems (see §[Enumerated Values](#enumerated-values) — House System Slugs)
- Lunar phase (the `lunar_phase` object; `null` only for heliocentric charts or when a luminary is absent — see §[Lunar Phase](#lunar-phase))

**Scope boundary — what is not in the reference format:**

The following are explicitly out of scope for the minimally calculated radix. They may appear in JZOD files but are not required:

- Dwarf planets: `ceres`, `quaoar`, `sedna`, `orcus`, `haumea`, `eris`, `makemake`, `gonggong`
- Major asteroids: `chiron`, `pallas`, `juno`, `vesta`, `hygiea`
- Centaurs: `pholus`, `nessus`, `chariklo`
- Kuiper-belt objects: `ixion`, `varuna`
- Other trans-Neptunian objects, centaurs, and Kuiper-belt objects beyond those enumerated above
- Fixed stars

---

## Compliance

A JZOD-compliant implementation:

1. **MUST** support reading and writing UTF-8 / Unicode (full mb4) in all string fields and in the user interface.
2. **MUST** treat unknown top-level keys and unknown keys within any object as ignorable — do not fail on forward-compatible additions.
3. **MUST NOT** fail when `version` is an unrecognized value.
4. **MUST NOT** fail when `relationships[].foreign_uid` points to a chart not present in the file.
5. **MUST** preserve `uid` values when round-tripping a JZOD file.
6. **SHOULD** maintain bidirectional relationship consistency: when creating or deleting a relationship from chart A pointing to chart B, update chart B's counterpart if chart B is in the same dataset.
7. **SHOULD** use canonical relationship codes from §Relationship Codes where applicable.

---

## Decision Chronology

*Resolved design decisions in the order they were made. Open questions live in §[Open Questions](#open-questions) until resolved, then move here.*

| Date | OQ | Decision | Rationale |
|---|---|---|---|
| 2026-06-19 | — | `sect` is three-state: `"diurnal"` / `"nocturnal"` / `"unknown"` (unknown when `datetime.unknown`); the field is omitted entirely for heliocentric charts | Sect is geocentric (Sun vs. local horizon) and needs an Ascendant, so it is meaningless heliocentrically — absence is honest. A placeholder time-of-day must not masquerade as a real day/night determination, so `"unknown"` is distinct from a computed value. |
| 2026-06-14 | OQ-4 | `zodiac` is an object `{ "name": "tropical" }`; sidereal adds `ayanamsha`; draconic stubbed | Bare string discriminator with structurally distinct peers is a breaking change on extension; object is self-protecting |
| 2026-06-14 | OQ-3 | Nested chart array is named `nested` | Short; works as a query noun (`chart.nested[]`); no redundancy with element type |
| 2026-06-13 | OQ-19 | Suffixed IDs for mean/true variants (Option A): `north_node_mean`, `north_node_true`, etc. | jq queries stay simple; uniform placement shape is preserved |
| 2026-06-13 | OQ-18 | House system slug is `equal_asc`; `equal_mc` added for the MC-rooted variant | `equal` alone is ambiguous once a second variant exists; precision prevents silent misidentification |
| 2026-06-13 | OQ-16 | Compute both mean and true/osculating lunar nodes | Same rationale as OQ-15; mode must be declared |
| 2026-06-13 | OQ-15 | Compute both mean and true/osculating BML | Silent divergence (~15°) is a research trust risk; declare both, don't choose one |
| 2026-06-13 | OQ-14 | `"topocentric"` added to `coordinate_system` enum | Real third mode used in practice; meaningful for Moon (up to ~1°) |
| 2026-06-13 | OQ-1 | Relationships live in a top-level array, not per-chart | Avoids mirroring; one edge store, no drift between A→B and B→A copies |

---

## Open Questions

*This section documents design decisions that are still being worked out. It is intended for collaborators including data scientists and domain experts reviewing this spec. Resolved decisions are recorded in §[Decision Chronology](#decision-chronology) above.*

---

### OQ-1 — Relationship placement: top-level array vs. per-chart

**Status: decided — top-level array (reflected in spec above). Capturing rationale here for review.**

The `relationships` array lives at the top level of the JZOD file rather than on each chart object.

**Why top-level:**
- Avoids mirroring: if chart A declares a relationship to chart B, and B also declares the same relationship back, you have two sources of truth that can drift independently.
- Graph traversal is straightforward — one place to look, no per-chart hunting.
- Aligns with relational/graph data modeling conventions: nodes (charts) and edges (relationships) are separate collections.

**Why per-chart (the alternative):**
- ADB (Astrodatabank), the only existing format that stores inter-chart relationships, puts them per-chart.
- A chart extracted from a file as a standalone document carries its relationship context with it — no need for a separate relationships export.
- Simpler for software that works with one chart at a time.

**Compromise considered:** Top-level is canonical; a chart exported as a standalone document MAY carry a `relationships` summary marked as denormalized. Not yet in spec.

**Question for review:** Is the top-level decision correct given that most astrology software operates one chart at a time rather than on whole-file graph queries?

---

### OQ-2 — Relationship codes: technical detail

**Status: draft vocabulary in §Relationship Codes above. Seeking review on completeness and taxonomy.**

The relationship code vocabulary was designed with the following principles:

1. **Human-readable slugs, not integers.** Early discussion considered using house numbers (1–12) as relationship codes directly, since the house system's topical domains (3rd house = siblings, 7th = partnerships, etc.) map naturally onto relationship types. This was rejected because bare integers require a lookup table and are opaque in raw JSON. The house domains are used as an organizational taxonomy for the vocabulary, but the codes themselves are slug strings.

2. **Neuter forms for all gendered relationships.** Where a relationship type has gendered variants (father/mother, husband/wife, son/daughter), a neuter form is provided and is the preferred choice when gender is unknown or not relevant to the link being described.

3. **Separation of semantic and structural codes.** Human relationship types (`spouse`, `sibling`, `employer`) and computational/structural codes (`composite_input`, `composite_counterpart`, `composite_output`) live in the same `code` field but serve different purposes. A data scientist reviewing this should consider whether these should be in separate fields or separate namespaces to avoid conflation.

4. **`bond` field:** A separate `bond: "crucial" | "loose"` field captures relationship strength orthogonally to the code. This maps to the "crucially related vs. loosely related" distinction: a spouse and a composite input are both `bond: "crucial"`; a distant colleague is `bond: "loose"`. Feedback welcome on whether a two-value enum is sufficient or whether a richer scale is needed.

**Question for review:** Is the relationship vocabulary complete enough for the research astrology use case (querying a database of charts for relationship patterns)? Are there common relationship types missing?

---

### OQ-3 — `sub_charts` array naming

**Status: resolved — `nested`.**

The field is named `nested`. It is short, works as a query noun (`chart.nested[]`), and reads naturally for both implementors and astrologers. Rejected alternatives: `sub_charts` (verbose, SFcht-specific), `nested_charts` (redundant with array element type), `children` (too graph-technical), `ncharts` (no prior art).

---

### OQ-7 — Should derivative charts be literally nested or uid-referenced?

**Status: open. The spec currently says literally nested — this decision is being reconsidered.**

The current spec places derivative charts (solar returns, progressions, solar arcs, etc.) as literal JSON objects inside the parent radix's `nested` array. This is one approach. The alternative is uid-referencing: derivative charts live in the top-level `charts` array like any other chart and carry a `parent_uid` field pointing to their parent.

**Literal nesting (current spec):**
- Parent relationship is structurally unambiguous — a solar return can only belong to the radix it lives inside.
- Mirrors how practitioners think: "Anna Freud's solar returns" are conceptually her nested charts.
- Matches Solar Fire's sub-chart model.
- Makes it harder to query all solar returns across a file without traversing nested structures.
- A derivative chart cannot be referenced by `uid` from a relationship or view without also being accessible at the top level.

**Uid-referencing (alternative):**
- All charts are peers in a flat `charts` array — simpler, uniform traversal.
- A derivative with `parent_uid` can still be identified as belonging to its parent.
- Easier to query: `charts.filter(c => c.type === "solar_return")` works without recursion.
- Parent relationship is a field value, not a structural guarantee — more flexible but less strict.

**Question:** Does literal nesting match how astrology databases are actually queried and maintained? This is an architectural decision with real consequences for how query tools, importers, and research pipelines traverse the file structure. Input from implementors, database designers, and practitioners who work with large chart collections is welcome.

---

### OQ-8 — JZOD acronym

**Status: open. Candidates in the file header above.**

The acronym candidates being considered:

| expansion | notes |
|---|---|
| JSON Zodiacal Open Data | "Open" signals the format's open nature. "Data" is generic. |
| JSON Zodiacal Open Document | "Document" fits the file-per-collection model. |
| JSON Zodiacal Open Definition | "Definition" implies schema/spec, which is accurate but possibly narrow. |
| JSON Zodiacal Open Database | "Database" implies a collection of records, which fits the multi-chart use case. |
| JSON Zodiacal Object Data | "Object" echoes JSON's object structure. |
| JSON Zodiacal Object Document | Same. |

**Zodiacal vs. Zodiac:** All candidates above use "Zodiacal" (the adjective form). "Zodiac" (noun as modifier) is also grammatically valid in English compound forms and flows more naturally when the acronym is spoken aloud — "jay-zod" reads as short for "JSON Zodiac..." without the clinicalness of "-al". "Zodiacal" is more formally correct in written prose. Dropping the suffix is a deliberate style choice, not an error. Variants with "Zodiac" (e.g. "JSON Zodiac Open Data") should be considered alongside the "-al" forms.

**Question:** Which expansion best communicates what JZOD is to someone encountering the name for the first time — an astrologer or a developer? And does "Zodiacal" or "Zodiac" feel more natural in your context?

---

### OQ-9 — Aspects

**Status: not yet in spec.**

The domain knowledge documents aspects (angular relationships between planets) extensively — major aspects (conjunction, opposition, trine, square, sextile), minor aspects, orbs, applying vs. separating. Of the existing formats, only LUNA persists computed aspects to disk. All others recompute on open.

JZOD's design goal includes enabling pre-computed research queries (`jq`-style queries across a chart database). Aspects are central to interpretive astrology and to many research questions ("how many AA-rated charts have Mars square Saturn?"). Storing pre-computed aspects would make these queries fast without re-calculation.

Fields LUNA stores per aspect: from-body id, to-body id, aspect type (as angle: 0, 60, 90, 120, 180, etc.), actual arc, hemicycle (waxing/waning), applying/separating, orb.

**Questions:**
1. Should aspects be a field on the chart object alongside `placements`?
2. If stored, should orb interpretation be stored (e.g. "applying trine") or just the raw arc (leaving interpretation to the consumer)?
3. Should aspects include body-to-angle and body-to-lot aspects, or bodies only?

---

### OQ-10 — Notes and biography fields

**Status: not yet in spec.**

All existing formats support free-text notes per chart. Solar Fire supports notes on both the main chart and each sub-chart. ADB stores structured biography text, source notes, and attribution (collector, editor, creation date, last edit date). LUNA stores titled notes with rich text, timestamps, and private/public visibility flags.

JZOD should capture at minimum:
- A free-text `notes` field on any chart object
- Optionally: `source_notes` (citation/provenance), `biography` (biographical text)
- Optionally: provenance metadata — collector name, creation date, last edit date

**Question:** Is a single `notes` string sufficient, or does JZOD need structured note types (source citation vs. biographical text vs. user annotation)?

---

### OQ-11 — Settings object for software-specific data

**Status: not yet in spec. From seed prompt.**

The seed prompt specifies: *"Astrology software authors are encouraged to create an object under the `settings` top-level object using the name of their software in lower snake case Reverse Domain Name Notation or Uniform Type Identifier (e.g. `org_videolan_vlc`), and storing any internal or user-facing settings there."*

This would allow a JZOD file to carry software preferences (display options, calculation defaults, orb tables, etc.) without polluting the chart data namespace. The mechanism is a top-level `settings` object with software-namespaced keys:

```json
{
  "version": "0.0.0",
  "charts": [...],
  "settings": {
    "com_esoterictech_solarfire": { ... },
    "io_mediumcoeli_starcat": { ... }
  }
}
```

**Question:** Is this the right namespace mechanism, and should it be documented as a SHOULD or a MAY?

---

### OQ-12 — Research query design goal

**Status: not yet formally documented. From seed prompt.**

The seed prompt establishes a concrete research use case: a database of pre-computed charts that can be queried with tools like `jq`. Example (from seed prompt, lightly corrected):

```sh
jq -r '.charts[] | select(.rodden_rating == "AA") | ... | select(jupiter_house_placidus == 8)' research.jzod
```

This means: *"For all charts where I have high confidence in birth time accuracy, give me the names of everyone who has Jupiter in their 8th house using the Placidus house system."*

This design goal has implications for the schema:
- House placements on bodies (the `house` object on each body) must be present and queryable without navigating deeply nested structures.
- Pre-computed aspects support similar queries without re-calculation.
- Flat structure (uid-referencing rather than literal nesting, OQ-7) makes `jq` traversal simpler.

**This is not an open question — it is a stated design goal.** Capturing it here so schema decisions can be evaluated against it.

---

### OQ-13 — Declination field on bodies

**Status: not yet in spec.**

The domain knowledge documents declination (angular distance north or south of the celestial equator) as a distinct field used for parallel aspects and out-of-bounds calculations. A body with declination beyond ±23°27′ is "out of bounds." The current example file and spec do not include declination on body placements.

Declination is present in the LUNA dataset (stored per body). It is distinct from `ecliptic_latitude` (distance from the ecliptic plane).

**Question:** Should `declination` and an `out_of_bounds` boolean be added to the body placement object?

---

### OQ-14 — Topocentric coordinate system

**Status: resolved — `"topocentric"` added to `coordinate_system` enumeration.**

The JZOD spec currently enumerates `"geocentric"` and `"heliocentric"` as the only valid values for `coordinate_system`. Topocentric positions are corrected for the observer's location on Earth's surface (parallax) and differ meaningfully from geocentric for the Moon (up to ~1°), negligibly for outer planets. It is a real third mode in common use that the enumeration must cover.

Note: implementations that expose a `coordinate` field (rather than `coordinate_system`) should align to the JZOD field name.

**Question:** Should `"topocentric"` be added to the `coordinate_system` enumeration now?

---

### OQ-15 — Black Moon Lilith: mean vs. true/osculating

**Status: compute policy decided — output both. Schema representation open — see [OQ-19](#oq-19--variant-placement-schema-mean-vs-trueosculating).**

Black Moon Lilith (BML) has two common calculation modes:
- **Mean BML** — averaged over time, smoother, more stable
- **True/Osculating BML** — instantaneous, can diverge from mean by 15° or more

The example file and starcat (`lilith_mode: "true"`) diverge by ~15° on BML. This is a silent data quality issue: two JZOD files claiming the same birth data will give different BML positions if one used mean and the other used true. Without a declared mode, consumers cannot know which to trust or how to compare.

> **Research trust risk.** Two JZOD files for the same birth data, both technically spec-compliant, can silently disagree on BML by 15°. A research database mixing mean and true BML files will produce quietly wrong results — wrong sign, wrong house, wrong aspects — with no detectable error. This is the highest-priority open question to resolve before any research use of JZOD.

Priapus (the anti-BML point) carries the same divergence symmetrically.

**Questions:**
1. Should JZOD mandate a default BML mode (recommend: mean, as it is more stable and widely supported)?
2. Should the calculation mode be stored per-placement (on the `black_moon_lilith` and `priapus` point objects) or in the `ephemeris` metadata block?

---

### OQ-16 — Lunar nodes: mean vs. true/osculating

**Status: compute policy decided — output both. Schema representation open — see [OQ-19](#oq-19--variant-placement-schema-mean-vs-trueosculating).**

The lunar nodal axis (North Node / South Node) also has mean and true/osculating variants. The ~0.24° divergence seen between independent implementations of the same birth data (both nominally using true nodes) suggests either a calculation difference or an ephemeris version gap. Either way, the mode must be declared in the file so consumers can detect and flag mismatches.

JZOD needs a node mode field — either per-placement on the point objects or globally in `ephemeris` metadata. Implementations should conform to whichever form the spec settles on.

**Question:** Should node mode be declared per-placement or globally in `ephemeris`, and should JZOD mandate a default (mean or true)?

---

### OQ-17 — Julian dates and heliocentric distance

**Status: not yet in spec.**

Three fields with genuine domain value are absent from the JZOD spec:

| field | proposed key | notes |
|---|---|---|
| Julian Date (UT) | `jd_ut` | The exact Julian Day number used for ephemeris lookup — precise, unambiguous provenance |
| Julian Date (TT) | `jd_tt` | Terrestrial Time variant — used in some ephemeris calculations |
| Distance | `distance_au` (per body) | Astronomical units from Earth — useful for parallax, phase, and research |

`jd_ut` / `jd_tt` in the `ephemeris` block would give consumers a fully reproducible calculation anchor that doesn't depend on parsing the birth datetime + utc_offset correctly. `distance_au` on body placements is present in LUNA and supports out-of-bounds and parallax calculations. Implementations should emit these fields once they are in spec.

**Questions:**
1. Should `jd_ut` (and optionally `jd_tt`) be added to the `ephemeris` metadata block?
2. Should `distance_au` be added to body placement objects?

---

### OQ-18 — House system slug: `equal_asc` vs. `equal`

**Status: resolved — slug is `equal_asc`. `equal_mc` added for the equal-from-Midheaven variant.**

The JZOD spec lists `equal` in its house system slug enumeration, but "equal from Ascendant" is the precise name — there is also an "equal from Midheaven" variant. Using a vague `equal` slug causes silent misidentification once `equal_mc` exists alongside it.

**Question:** Should the slug be `equal_asc` (precise, unambiguous) or `equal` (shorter, with a standing convention that `equal` always means from-Ascendant unless stated otherwise)?

---

### OQ-19 — Variant placement schema: mean vs. true/osculating

**Status: resolved — Option A (suffixed IDs).**

The design principle "compute all siblings" means both mean and true/osculating variants of the lunar nodes and Black Moon Lilith must be stored in a JZOD file. The current spec represents each point as a single object in the `points` array with a unique `id`. Storing two variants of the same point requires a schema decision.

Three options:

**Option A — suffixed IDs**
Two separate entries with distinct ids:
```json
{ "id": "north_node_mean", "ecliptic_longitude": 338.026, ... },
{ "id": "north_node_true", "ecliptic_longitude": 337.782, ... }
```
- jq queries are simple: `select(.id == "north_node_mean")`
- Doubles the number of point IDs in the enumeration
- Works cleanly with the existing array structure

**Option B — variant field**
Two entries sharing the same base `id`, distinguished by a `variant` field:
```json
{ "id": "north_node", "variant": "mean", "ecliptic_longitude": 338.026, ... },
{ "id": "north_node", "variant": "true", "ecliptic_longitude": 337.782, ... }
```
- `id` stays clean; enumeration doesn't double
- jq queries require two predicates: `select(.id == "north_node" and .variant == "mean")`
- Requires a new `variant` field on point objects

**Option C — nested object**
Single entry with sub-objects per variant:
```json
{ "id": "north_node", "mean": { "ecliptic_longitude": 338.026, ... }, "true": { "ecliptic_longitude": 337.782, ... } }
```
- Compact, no array duplication
- jq path becomes: `.placements.points[] | select(.id == "north_node") | .mean.ecliptic_longitude`
- Breaks the uniform placement object shape — a point no longer has `ecliptic_longitude` at the top level

Applies to: `north_node`, `south_node`, `black_moon_lilith`, `priapus`.

**Question:** Which schema best serves the jq queryability goal while keeping the enumerated ID space clean?

---

### OQ-4 — Zodiac field: extending beyond tropical

**Status: resolved — `zodiac` is an object with a `name` discriminator.**

`zodiac` is now `{ "name": "tropical" }`. The `name` field is the discriminator. Known names and their additional fields:

| `name` | additional fields | notes |
|---|---|---|
| `"tropical"` | — | Anchored to the vernal equinox. Default. |
| `"sidereal"` | `ayanamsha` | Offset from tropical. Ayanamsha registry TBD — see seed prompt for TOML registry approach. |
| `"draconic"` | — | Anchored to the North Node. |

Additional sidereal variants (Lahiri, Fagan-Bradley, Krishnamurti, Vettius Valens, etc.) are expressed via the `ayanamsha` field on a `"sidereal"` zodiac object, not as distinct `name` values. The ayanamsha value space is left open pending a registry definition.

---

### OQ-5 — Data confidence: decoupling Rodden rating

**Status: open. From seed prompt.**

The Rodden Rating system conflates data *source* (where the data came from) with data *confidence* (how trustworthy it is). The seed prompt notes interest in exploring whether a new rating system that decouples these two axes would be more useful for research purposes.

LUNA (lunaastrology.com) already extends the Rodden system with 24 codes that partially address this: separate codes for timed official source, timed documented, timed historic, timed unknown-source, dirty timed, and untimed variants.

**Question for data scientist review:** Is the existing Rodden system sufficient for research queries, or is a two-axis (source × confidence) rating system worth designing into JZOD now?

---

### OQ-6 — Vedic / Jyotish fields

**Status: deferred. From seed prompt.**

The seed prompt notes interest in predefined keys for Vedic astrology (tithi, nakshatra, Vikrama Samvat, etc.), referencing the PyJhora project for clues. These are not in scope for `0.0.0` but should be designed in a way that doesn't conflict with a future Vedic extension.

---

### OQ-20 — Calendar system field on `datetime`

**Status: not yet in spec.**

The `datetime` block records `year`, `month`, `day` but does not declare which calendar system those values are expressed in. The implicit assumption is Gregorian, but this assumption is wrong for a large class of historically significant charts.

**The trap:** A consumer who receives `{ "year": 120, "month": 2, "day": 8 }` with no calendar field cannot determine the Julian Day number without guessing the calendar. If they assume Gregorian and the date is Julian, the resulting JD is off by several days — wrong positions for everything.

Dates requiring explicit calendar declaration:
- All dates before the Gregorian reform (1582-10-15) are Julian. No chart from antiquity, classical Rome, medieval Europe, or the Renaissance can be correctly computed without knowing this.
- Post-reform holdouts: Russia (Julian until 1918-02-14), Greece (until 1923-03-01), Mount Athos (still Julian). A chart for a 19th-century Russian native recorded in the Julian calendar needs the declaration even though the year is modern.

**Proposed:** `"calendar": "gregorian" | "julian"` on the `datetime` object. Optional field, default `"gregorian"`.

**Open design question:** Should `calendar` be required (not optional) when `year` is before 1582, to eliminate the ambiguity entirely? The default-Gregorian shortcut silently corrupts every pre-reform chart whose producer omits the field.

---

### OQ-21 — LMT longitude annotation on `datetime`

**Status: not yet in spec.**

`utc_offset` is the authoritative calculation input and carries the offset actually used. For LMT (Local Mean Time) charts — common for all historical dates before standardized civil time zones, roughly pre-1850 depending on region — the offset is not a fixed civil zone but a value derived from the birth longitude: `offset = longitude_degrees / 15`.

Currently JZOD can encode the derived value (e.g. `"+02:24"` for Antioch at 36.1°E) but loses the information that the offset was LMT-derived. A consumer cannot distinguish a fixed civil offset from a longitude-derived one, cannot verify the derivation, and cannot flag the chart as LMT for research queries.

**Proposed:** `"lmt_longitude": number | null` on `datetime`, following the same pattern as `iana_tz` — informational, not a calculation input. `utc_offset` remains authoritative; `lmt_longitude` records the geographic longitude used to derive it, East positive (ISO 6709).

**Open design questions:**
1. Should `lmt_longitude` be informational (parallel to `iana_tz`) or should it be a calculation input that supersedes `utc_offset`? The informational model is simpler but requires the producer to pre-derive the offset correctly; the calculation-input model lets consumers re-derive and cross-check.
2. Should `lmt_longitude` and `iana_tz` be mutually exclusive (one or the other signals the offset provenance), or can both be null (fixed civil offset with no provenance annotation)?

---

### OQ-22 — CBOR binary encoding (RFC 8949)

**Status: not yet in spec. Under investigation.**

JZOD is currently defined as UTF-8 JSON. A natural extension question is whether a binary encoding should be standardised alongside it — specifically **CBOR (Concise Binary Object Representation)**, standardised as RFC 8949 (obsoletes RFC 7049).

CBOR is a binary data format with a design goal of extreme simplicity of implementation and small encoding size, built on a JSON-compatible data model (maps, arrays, strings, numbers, booleans, null, bytes). Its key properties:

- **Same data model as JSON.** A JZOD CBOR file is a direct binary encoding of the same object tree — no schema changes, no new concepts. A CBOR-aware reader can decode it to the same in-memory structure a JSON reader produces.
- **Compact.** CBOR omits JSON's character overhead (quotes, braces, colons). A JZOD file with many numeric fields (longitudes, speeds, house cusps) compresses well. Typical savings vs. minified JSON: 20–40%; more with binary-tagged extensions.
- **Streaming.** CBOR supports indefinite-length arrays and maps, making it friendlier for large chart collections written incrementally.
- **Self-describing with tags.** RFC 8949 §3.4 defines optional semantic tags (e.g. tag 1 = epoch-based datetime, tag 2/3 = big integers). These could annotate JD values, distances, or longitudes without changing the schema.
- **Broadly supported.** `ciborium` (Rust, pure), `cbor2` (Python), `jackson-dataformat-cbor` (JVM), browser-native via `cbor-x`/`cbor2` (JS). No special parser needed beyond the library call.

**Concrete JZOD considerations:**

| concern | notes |
|---------|-------|
| File suffix | `.jzod` for JSON, `.jzodc` or `.jzod.cbor` for CBOR — both carry the same schema |
| `version` field | CBOR preserves string key order; parsers should not assume field order either way |
| `uid` as bytes | RFC 8949 tag 37 = UUID bytes (16 bytes vs 36 UTF-8 chars per UID, ~8% of a chart's identifiers) |
| `ecliptic_longitude` precision | IEEE 754 f64 in CBOR is lossless — identical precision to JSON numbers |
| Streaming collections | CBOR indefinite-length arrays let a multi-chart collection be appended without rewriting the size prefix |
| Tooling gap | `jq` does not read CBOR natively; research workflows that depend on `jq` would require a decode step |

**The core tension:** JZOD's primary design goal is queryability with ordinary tools (`jq`, `jsonb`, Python `json`). CBOR trades away that zero-setup tooling advantage for compactness and streaming. The two encodings are not mutually exclusive — a spec could define CBOR as an optional transport encoding (like MessagePack alongside JSON in many APIs) while keeping JSON as the canonical query-time format.

**Questions:**
1. Is there a concrete use case where JZOD file size is a bottleneck? (Typical minimally calculated radix is well under 50 KB as minified JSON.)
2. Should CBOR be defined as a first-class optional encoding, or only as a transport hint (decompress to JSON before querying)?
3. If CBOR is adopted, should JZOD mandate the `application/cbor` MIME type and a magic-number or tag prefix to distinguish `.jzod` CBOR files from JSON files without reading them?

---

## Example

See [`anna_freud_radix.json`](https://github.com/lucidaeon/mediumcoeli/blob/main/crates/jzod/anna_freud_radix.json) for a complete minimally calculated radix.

---

## jq Query Examples

These queries work against any JZOD file. Single-chart examples use `file.json` as a placeholder — substitute your file path. Research examples assume a multi-chart collection file.

**get sun longitude**
```sh
jq '.charts[0].placements.bodies[] | select(.id == "sun") | .ecliptic_longitude' file.json
```

**get sun in sign notation**
```sh
jq '.charts[0].placements.bodies[] | select(.id == "sun") | "\(.degree)° \(.sign)"' file.json
```

**get jupiter's placidus house number**
```sh
jq '.charts[0].placements.bodies[] | select(.id == "jupiter") | .house.placidus' file.json
```

**get ascendant longitude**
```sh
jq '.charts[0].placements.angles[] | select(.id == "ascendant") | .ecliptic_longitude' file.json
```

**list retrograde bodies**
```sh
jq '[.charts[0].placements.bodies[] | select(.retrograde) | .id]' file.json
```

**get lot of fortune longitude**
```sh
jq '.charts[0].placements.lots[] | select(.id == "lot_of_fortune") | .ecliptic_longitude' file.json
```

**get placidus 8th house cusp**
```sh
jq '.charts[0].houses.placidus."8"' file.json
```

**all bodies in scorpio**
```sh
jq '[.charts[0].placements.bodies[] | select(.sign == "scorpio") | .id]' file.json
```

**names of all AA-rated charts** (multi-chart collection)
```sh
jq '[.charts[] | select(.rodden_rating == "AA") | .name.display]' research.json
```

**AA-rated charts with jupiter in placidus 8th** (multi-chart collection)
```sh
jq '[.charts[] | select(.rodden_rating == "AA") | select(any(.placements.bodies[]; .id == "jupiter" and .house.placidus == 8)) | .name.display]' research.json
```

---

## Versioning

JZOD uses [Semantic Versioning](https://semver.org/).

- `0.x.x` — unstable. Any field may change without notice.
- `1.0.0` — first stable release. Breaking changes require a major version bump.
- Parsers MUST NOT reject files with an unrecognized version number.
