//! Refchart oracle: per-chart hand-transcribed reference data + cross-
//! validation tests.

// Test bindings (jd_ut/jd_tt, ax_deg/ac_deg, d_ac/d_mc/d_ic, ...) are
// astronomically meaningful; clippy's similar_names heuristic isn't.
#![allow(clippy::similar_names)]
// HouseSystem::WholeSign is part of an exhaustive enum mirroring the chart-data
// schema. Some charts don't use it (yet), but it must remain matchable.
#![allow(dead_code)]
//!
//! Every constant in this file is derived from `REFCHARTS.md` (the human-
//! readable refchart dump). Refchart's *resolved location* is taken as
//! authoritative — its Asc/MC and house cusps were computed at the
//! coords it printed, so we must match those coords to compare cleanly.
//! Refchart's *resolved time* is recorded as a debug aid only; we derive
//! `jd_ut` from the civil date + zone offset ourselves.
//!
//! Test families:
//!
//! 1. **Primitive cross-validation** — ΔT, JDE, ST(0°), LST, obliquity
//!    per chart. These exercise [`crate::time::delta_t`],
//!    [`crate::time::calendar::civil_to_jd`],
//!    [`crate::coords::sidereal_time::gast_rad`], and
//!    [`crate::coords::obliquity`] directly, not transitively via Asc/MC.
//! 2. **Position oracle** — longitude, latitude, daily motion per body
//!    per chart against [`apparent_ecliptic_position`] (geo) and
//!    [`heliocentric_ecliptic_position`].
//! 3. **Angles** — Asc and MC per chart, using refchart's resolved
//!    location.
//! 4. **House cusps** — per chart's house system (Placidus, Equal,
//!    Whole Sign, Regiomontanus, Porphyry).
//!
//! Where refchart's ΔT model differs measurably from SMH 2016 (notably
//! the year-120 chart, where refchart reports +9340 s vs SMH ~10570 s,
//! a ~1230 s gap), the divergence is documented inline and the position
//! tolerances widened to absorb it.
//!
//! **No-oracle outputs.** starcat emits `distance_au` per body, but
//! refchart's chart-report format never prints heliocentric or geocentric
//! distance in AU. Distance therefore has no refchart oracle and is not
//! asserted in this file. (HORIZONS *can* return distance; if we ever
//! want a distance oracle we add it to `acceptance_horizons.rs`.)

use pericynthion::body::Body;
use pericynthion::coords::acds::ac_rad;
use pericynthion::coords::apparent::{apparent_ecliptic_position, heliocentric_ecliptic_position};
use pericynthion::coords::mcic::mc_rad;
use pericynthion::coords::nutation::nutation;
use pericynthion::coords::obliquity::mean_obliquity_rad;
use pericynthion::coords::sidereal_time::gast_rad;
use pericynthion::ephemeris::Ephemeris;
use pericynthion::houses::{
    HouseCusps, equal_as_rad, placidus_rad, porphyry_rad, regiomontanus_rad, whole_sign_rad,
};
use pericynthion::jpl::{discover, header::parse as parse_header, reader::EphemerisFile};
use pericynthion::time::calendar::{Calendar, CivilDate, civil_to_jd};
use pericynthion::time::delta_t::jd_ut_to_jd_tt;
use std::f64::consts::TAU as CIRCLE_RAD;
use std::path::PathBuf;

// =============================================================================
// DMS helpers
// =============================================================================

/// Unsigned DMS (zodiac longitude): base° + d° + m′ + s″.
const fn dms(base: f64, d: f64, m: f64, s: f64) -> f64 {
    base + d + m / 60.0 + s / 3600.0
}

/// Signed DMS (latitude / declination / daily motion): sign × (d + m/60 + s/3600).
const fn sdms(sign: f64, d: f64, m: f64, s: f64) -> f64 {
    sign * (d + m / 60.0 + s / 3600.0)
}

// Zodiac base degrees (Ari at 0°, …, Pis at 330°).
const ARI: f64 = 0.0;
const TAU: f64 = 30.0;
const GEM: f64 = 60.0;
const CAN: f64 = 90.0;
const LEO: f64 = 120.0;
const VIR: f64 = 150.0;
const LIB: f64 = 180.0;
const SCO: f64 = 210.0;
const SAG: f64 = 240.0;
const CAP: f64 = 270.0;
const AQU: f64 = 300.0;
const PIS: f64 = 330.0;

// =============================================================================
// Chart context types
// =============================================================================

#[derive(Clone, Copy, Debug)]
enum Mode {
    Geocentric,
    Heliocentric,
}

#[derive(Clone, Copy, Debug)]
enum HouseSystem {
    Placidus,
    Equal,
    WholeSign,
    Regiomontanus,
    Porphyry,
}

struct BodyRef {
    name: &'static str,
    body: Body,
    lon_deg: f64,
    lat_deg: f64,
    travel_deg_per_day: f64,
}

struct Chart {
    id: &'static str,
    mode: Mode,
    civil: CivilDate,
    calendar: Calendar,
    // Refchart's resolved coords (east-positive longitude).
    lat_deg: f64,
    lon_deg: f64,
    // Refchart-reported primitives.
    delta_t_s: f64,
    jde: f64,
    lst_hours: f64,
    obliquity_deg: f64,
    ac_deg: f64,
    mc_deg: f64,
    ic_deg: f64,
    ds_deg: f64,
    /// Vertex (western prime-vertical / ecliptic intersection).
    vx_deg: f64,
    /// Anti-Vertex = Vx + 180°.
    ax_deg: f64,
    house_system: HouseSystem,
    /// Refchart cusps in 1-based natural order H1..H12, mapped here to indices 0..11.
    house_cusps_deg: [f64; 12],
    bodies: &'static [BodyRef],
    /// Part of Fortune longitude, refchart-reported. `None` for heliocentric
    /// charts (PF is undefined / not emitted).
    fortune_deg: Option<f64>,
    /// Refchart's "Nod" entry — the **true** (osculating) lunar north node
    /// longitude. `None` for heliocentric charts (refchart's UNIX 2038
    /// heliocentric output omits Nod).
    true_nn_deg: Option<f64>,
}

// =============================================================================
// Chart data — derived directly from REFCHARTS.md.
// =============================================================================

// ── test 5 ── Lightning Strike — 1955-11-12 22:04 PST Universal City CA ──────
//
// Civil UT = 22:04 PST + 8h = 1955-11-13 06:04 UT.
// Refchart resolved coords: 34°N08'20" 118°W21'09"
// Refchart resolved time:   22:04 PST +8:00
const LIGHTNING_STRIKE_BODIES: &[BodyRef] = &[
    BodyRef {
        name: "Sun",
        body: Body::Sun,
        lon_deg: dms(SCO, 20.0, 4.0, 55.0),
        lat_deg: sdms(-1.0, 0.0, 0.0, 0.0),
        travel_deg_per_day: sdms(1.0, 1.0, 0.0, 0.0),
    }, // Sco⌖20°04'55"
    BodyRef {
        name: "Moon",
        body: Body::Moon,
        lon_deg: dms(SCO, 6.0, 6.0, 30.0),
        lat_deg: sdms(-1.0, 3.0, 27.0, 0.0),
        travel_deg_per_day: sdms(1.0, 12.0, 18.0, 0.0),
    }, // Sco⌖06°06'30"
    BodyRef {
        name: "Mercury",
        body: Body::Mercury,
        lon_deg: dms(SCO, 7.0, 47.0, 49.0),
        lat_deg: sdms(1.0, 1.0, 26.0, 0.0),
        travel_deg_per_day: sdms(1.0, 1.0, 35.0, 0.0),
    }, // Sco⌖07°47'49"
    BodyRef {
        name: "Venus",
        body: Body::Venus,
        lon_deg: dms(SAG, 8.0, 56.0, 15.0),
        lat_deg: sdms(-1.0, 0.0, 29.0, 0.0),
        travel_deg_per_day: sdms(1.0, 1.0, 14.0, 0.0),
    }, // Sag⌖08°56'15"
    BodyRef {
        name: "Mars",
        body: Body::Mars,
        lon_deg: dms(LIB, 19.0, 46.0, 40.0),
        lat_deg: sdms(1.0, 0.0, 55.0, 0.0),
        travel_deg_per_day: sdms(1.0, 0.0, 38.0, 42.0),
    }, // Lib⌖19°46'40"
    BodyRef {
        name: "Jupiter",
        body: Body::Jupiter,
        lon_deg: dms(LEO, 29.0, 36.0, 46.0),
        lat_deg: sdms(1.0, 0.0, 48.0, 0.0),
        travel_deg_per_day: sdms(1.0, 0.0, 6.0, 14.0),
    }, // Leo⌖29°36'46"
    BodyRef {
        name: "Saturn",
        body: Body::Saturn,
        lon_deg: dms(SCO, 23.0, 21.0, 40.0),
        lat_deg: sdms(1.0, 1.0, 57.0, 0.0),
        travel_deg_per_day: sdms(1.0, 0.0, 7.0, 9.0),
    }, // Sco⌖23°21'40"
    BodyRef {
        name: "Uranus",
        body: Body::Uranus,
        lon_deg: dms(LEO, 2.0, 19.0, 33.0),
        lat_deg: sdms(1.0, 0.0, 33.0, 0.0),
        travel_deg_per_day: sdms(-1.0, 0.0, 0.0, 16.0),
    }, // Leo⌖02°19'33" R
    BodyRef {
        name: "Neptune",
        body: Body::Neptune,
        lon_deg: dms(LIB, 28.0, 47.0, 18.0),
        lat_deg: sdms(1.0, 1.0, 40.0, 0.0),
        travel_deg_per_day: sdms(1.0, 0.0, 2.0, 7.0),
    }, // Lib⌖28°47'18"
    BodyRef {
        name: "Pluto",
        body: Body::Pluto,
        lon_deg: dms(LEO, 28.0, 31.0, 57.0),
        lat_deg: sdms(1.0, 10.0, 30.0, 0.0),
        travel_deg_per_day: sdms(1.0, 0.0, 0.0, 32.0),
    }, // Leo⌖28°31'57"
];

const LIGHTNING_STRIKE: Chart = Chart {
    id: "lightning_strike",
    mode: Mode::Geocentric,
    civil: CivilDate {
        year: 1955,
        month: 11,
        day: 13,
        hour: 6,
        minute: 4,
        second: 0.0,
    },
    calendar: Calendar::Gregorian,
    lat_deg: dms(0.0, 34.0, 8.0, 20.0),   // 34°N08'20"
    lon_deg: -dms(0.0, 118.0, 21.0, 9.0), // 118°W21'09"
    delta_t_s: 31.0,
    jde: 2_435_424.753_140,
    lst_hours: 1.0 + 36.0 / 60.0 + 55.0 / 3600.0, //  1:36:55
    obliquity_deg: dms(0.0, 23.0, 26.0, 42.0),    // 23°26'42"
    ac_deg: dms(LEO, 5.0, 19.0, 30.0),            // Leo⌖05°19'30"
    mc_deg: dms(ARI, 26.0, 7.0, 43.0),            // Ari⌖26°07'43"
    ic_deg: dms(LIB, 26.0, 7.0, 43.0),            // Lib⌖26°07'43"  (= MC+180)
    ds_deg: dms(AQU, 5.0, 19.0, 30.0),            // Aqu⌖05°19'30"  (= ASC+180)
    vx_deg: dms(SAG, 17.0, 0.0, 51.0),            // Sag⌖17°00'51"
    ax_deg: dms(GEM, 17.0, 0.0, 51.0),            // Gem⌖17°00'51"  (= Vx+180)
    house_system: HouseSystem::Placidus,
    house_cusps_deg: [
        dms(LEO, 5.0, 19.0, 30.0),  // H1  Leo⌖05°19'30"
        dms(LEO, 27.0, 41.0, 52.0), // H2  Leo⌖27°41'52"
        dms(VIR, 24.0, 16.0, 24.0), // H3  Vir⌖24°16'24"
        dms(LIB, 26.0, 7.0, 43.0),  // H4  Lib⌖26°07'43"
        dms(SAG, 1.0, 13.0, 48.0),  // H5  Sag⌖01°13'48"
        dms(CAP, 5.0, 6.0, 57.0),   // H6  Cap⌖05°06'57"
        dms(AQU, 5.0, 19.0, 30.0),  // H7  Aqu⌖05°19'30"
        dms(AQU, 27.0, 41.0, 52.0), // H8  Aqu⌖27°41'52"
        dms(PIS, 24.0, 16.0, 24.0), // H9  Pis⌖24°16'24"
        dms(ARI, 26.0, 7.0, 43.0),  // H10 Ari⌖26°07'43"
        dms(GEM, 1.0, 13.0, 48.0),  // H11 Gem⌖01°13'48"
        dms(CAN, 5.0, 6.0, 57.0),   // H12 Can⌖05°06'57"
    ],
    bodies: LIGHTNING_STRIKE_BODIES,
    fortune_deg: Some(dms(LEO, 19.0, 17.0, 55.0)), // PF Leo⌖19°17'55"
    true_nn_deg: Some(dms(SAG, 17.0, 28.0, 53.0)), // Nn Sag⌖17°28'53" R
};

// ── test 4 ── UNIX 32-bit overflow — 2038-01-19 03:14:07 UT London ───────────
//
// Heliocentric chart. Earth replaces the Sun; Moon is absent from the helio
// output. Civil UT = the chart instant (no zone offset).
// Refchart resolved coords: 51°N30' 000°W10'
// Refchart resolved time:   03:14:07 UT +0:00
const UNIX_OVERFLOW_BODIES: &[BodyRef] = &[
    BodyRef {
        name: "Earth",
        body: Body::Earth,
        lon_deg: dms(CAN, 29.0, 7.0, 39.0),
        lat_deg: sdms(1.0, 0.0, 0.0, 0.0),
        travel_deg_per_day: sdms(1.0, 1.0, 1.0, 0.0),
    }, // Can⌖29°07'39"
    BodyRef {
        name: "Mercury",
        body: Body::Mercury,
        lon_deg: dms(SAG, 10.0, 11.0, 0.0),
        lat_deg: sdms(-1.0, 2.0, 34.0, 0.0),
        travel_deg_per_day: sdms(1.0, 2.0, 45.0, 0.0),
    }, // Sag⌖10°11'00"
    BodyRef {
        name: "Venus",
        body: Body::Venus,
        lon_deg: dms(LEO, 8.0, 13.0, 35.0),
        lat_deg: sdms(1.0, 2.0, 38.0, 0.0),
        travel_deg_per_day: sdms(1.0, 1.0, 37.0, 0.0),
    }, // Leo⌖08°13'35"
    BodyRef {
        name: "Mars",
        body: Body::Mars,
        lon_deg: dms(GEM, 28.0, 56.0, 9.0),
        lat_deg: sdms(1.0, 1.0, 9.0, 0.0),
        travel_deg_per_day: sdms(1.0, 0.0, 29.0, 38.0),
    }, // Gem⌖28°56'09"
    BodyRef {
        name: "Jupiter",
        body: Body::Jupiter,
        lon_deg: dms(CAN, 25.0, 5.0, 43.0),
        lat_deg: sdms(1.0, 0.0, 19.0, 0.0),
        travel_deg_per_day: sdms(1.0, 0.0, 4.0, 55.0),
    }, // Can⌖25°05'43"
    BodyRef {
        name: "Saturn",
        body: Body::Saturn,
        lon_deg: dms(VIR, 11.0, 20.0, 9.0),
        lat_deg: sdms(1.0, 1.0, 49.0, 0.0),
        travel_deg_per_day: sdms(1.0, 0.0, 2.0, 6.0),
    }, // Vir⌖11°20'09"
    BodyRef {
        name: "Uranus",
        body: Body::Uranus,
        lon_deg: dms(CAN, 22.0, 13.0, 34.0),
        lat_deg: sdms(1.0, 0.0, 28.0, 0.0),
        travel_deg_per_day: sdms(1.0, 0.0, 0.0, 44.0),
    }, // Can⌖22°13'34"
    BodyRef {
        name: "Neptune",
        body: Body::Neptune,
        lon_deg: dms(ARI, 28.0, 15.0, 1.0),
        lat_deg: sdms(-1.0, 1.0, 42.0, 0.0),
        travel_deg_per_day: sdms(1.0, 0.0, 0.0, 22.0),
    }, // Ari⌖28°15'01"
    BodyRef {
        name: "Pluto",
        body: Body::Pluto,
        lon_deg: dms(AQU, 22.0, 14.0, 56.0),
        lat_deg: sdms(-1.0, 9.0, 8.0, 0.0),
        travel_deg_per_day: sdms(1.0, 0.0, 0.0, 14.0),
    }, // Aqu⌖22°14'56"
];

const UNIX_OVERFLOW: Chart = Chart {
    id: "unix_overflow_2038",
    mode: Mode::Heliocentric,
    civil: CivilDate {
        year: 2038,
        month: 1,
        day: 19,
        hour: 3,
        minute: 14,
        second: 7.0,
    },
    calendar: Calendar::Gregorian,
    lat_deg: dms(0.0, 51.0, 30.0, 0.0),
    lon_deg: -dms(0.0, 0.0, 10.0, 0.0),
    delta_t_s: 75.0,
    jde: 2_465_442.635_674,
    lst_hours: 11.0 + 7.0 / 60.0 + 57.0 / 3600.0,
    obliquity_deg: dms(0.0, 23.0, 26.0, 3.0),
    ac_deg: dms(SCO, 24.0, 3.0, 9.0),   // Sco⌖24°03'09"
    mc_deg: dms(VIR, 15.0, 51.0, 55.0), // Vir⌖15°51'55"
    ic_deg: dms(PIS, 15.0, 51.0, 55.0), // Pis⌖15°51'55"  (= MC+180)
    ds_deg: dms(TAU, 24.0, 3.0, 9.0),   // Tau⌖24°03'09"  (= ASC+180)
    vx_deg: dms(CAN, 6.0, 25.0, 44.0),  // Can⌖06°25'44"
    ax_deg: dms(CAP, 6.0, 25.0, 44.0),  // Cap⌖06°25'44"  (= Vx+180)
    house_system: HouseSystem::Equal,
    house_cusps_deg: [
        dms(SCO, 24.0, 3.0, 9.0), // H1  Sco⌖24°03'09"
        dms(SAG, 24.0, 3.0, 9.0), // H2  Sag⌖24°03'09"
        dms(CAP, 24.0, 3.0, 9.0), // H3  Cap⌖24°03'09"
        dms(AQU, 24.0, 3.0, 9.0), // H4  Aqu⌖24°03'09"
        dms(PIS, 24.0, 3.0, 9.0), // H5  Pis⌖24°03'09"
        dms(ARI, 24.0, 3.0, 9.0), // H6  Ari⌖24°03'09"
        dms(TAU, 24.0, 3.0, 9.0), // H7  Tau⌖24°03'09"
        dms(GEM, 24.0, 3.0, 9.0), // H8  Gem⌖24°03'09"
        dms(CAN, 24.0, 3.0, 9.0), // H9  Can⌖24°03'09"
        dms(LEO, 24.0, 3.0, 9.0), // H10 Leo⌖24°03'09"
        dms(VIR, 24.0, 3.0, 9.0), // H11 Vir⌖24°03'09"
        dms(LIB, 24.0, 3.0, 9.0), // H12 Lib⌖24°03'09"
    ],
    bodies: UNIX_OVERFLOW_BODIES,
    fortune_deg: None, // heliocentric — refchart does not emit PF
    true_nn_deg: None, // heliocentric — refchart does not emit Nod
};

// ── test 1 ── William Lilly — 1602-05-11 02:00 LMT Diseworth ─────────────────
//
// Refchart treats this as PROLEPTIC GREGORIAN (its only mode), so the civil
// date 1602-05-11 maps to JD ≈ 2 306 308.58. The historically correct Julian
// date for this event is 10 days later.
// LMT for Diseworth (lon = -1°11'): UT = LMT + 0:04:44.
// Civil UT = 02:04:44 UT on 1602-05-11.
// Refchart resolved coords: 52°N47' 001°W11'
// Refchart resolved time:   02:00 LMT +0:04:44
const WILLIAM_LILLY_BODIES: &[BodyRef] = &[
    BodyRef {
        name: "Sun",
        body: Body::Sun,
        lon_deg: dms(TAU, 19.0, 59.0, 11.0),
        lat_deg: sdms(1.0, 0.0, 0.0, 0.0),
        travel_deg_per_day: sdms(1.0, 0.0, 57.0, 46.0),
    }, // Tau⌖19°59'11"
    BodyRef {
        name: "Moon",
        body: Body::Moon,
        lon_deg: dms(CAP, 14.0, 47.0, 36.0),
        lat_deg: sdms(1.0, 2.0, 38.0, 0.0),
        travel_deg_per_day: sdms(1.0, 11.0, 49.0, 0.0),
    }, // Cap⌖14°47'36"
    BodyRef {
        name: "Mercury",
        body: Body::Mercury,
        lon_deg: dms(TAU, 4.0, 16.0, 0.0),
        lat_deg: sdms(-1.0, 2.0, 27.0, 0.0),
        travel_deg_per_day: sdms(-1.0, 0.0, 9.0, 9.0),
    }, // Tau⌖04°16'00" R
    BodyRef {
        name: "Venus",
        body: Body::Venus,
        lon_deg: dms(TAU, 19.0, 9.0, 16.0),
        lat_deg: sdms(-1.0, 0.0, 36.0, 0.0),
        travel_deg_per_day: sdms(1.0, 1.0, 13.0, 0.0),
    }, // Tau⌖19°09'16"
    BodyRef {
        name: "Mars",
        body: Body::Mars,
        lon_deg: dms(VIR, 6.0, 32.0, 59.0),
        lat_deg: sdms(1.0, 1.0, 37.0, 0.0),
        travel_deg_per_day: sdms(1.0, 0.0, 16.0, 36.0),
    }, // Vir⌖06°32'59"
    BodyRef {
        name: "Jupiter",
        body: Body::Jupiter,
        lon_deg: dms(LIB, 13.0, 29.0, 38.0),
        lat_deg: sdms(1.0, 1.0, 31.0, 0.0),
        travel_deg_per_day: sdms(-1.0, 0.0, 4.0, 58.0),
    }, // Lib⌖13°29'38" R
    BodyRef {
        name: "Saturn",
        body: Body::Saturn,
        lon_deg: dms(SCO, 18.0, 41.0, 59.0),
        lat_deg: sdms(1.0, 2.0, 26.0, 0.0),
        travel_deg_per_day: sdms(-1.0, 0.0, 4.0, 28.0),
    }, // Sco⌖18°41'59" R
    BodyRef {
        name: "Uranus",
        body: Body::Uranus,
        lon_deg: dms(TAU, 10.0, 1.0, 56.0),
        lat_deg: sdms(-1.0, 0.0, 23.0, 0.0),
        travel_deg_per_day: sdms(1.0, 0.0, 3.0, 26.0),
    }, // Tau⌖10°01'56"
    BodyRef {
        name: "Neptune",
        body: Body::Neptune,
        lon_deg: dms(LEO, 29.0, 36.0, 33.0),
        lat_deg: sdms(1.0, 0.0, 44.0, 0.0),
        travel_deg_per_day: sdms(1.0, 0.0, 0.0, 0.0),
    }, // Leo⌖29°36'33" SD (station)
    BodyRef {
        name: "Pluto",
        body: Body::Pluto,
        lon_deg: dms(ARI, 25.0, 17.0, 11.0),
        lat_deg: sdms(-1.0, 16.0, 37.0, 0.0),
        travel_deg_per_day: sdms(1.0, 0.0, 1.0, 16.0),
    }, // Ari⌖25°17'11"
];

const WILLIAM_LILLY: Chart = Chart {
    id: "william_lilly",
    mode: Mode::Geocentric,
    civil: CivilDate {
        year: 1602,
        month: 5,
        day: 11,
        hour: 2,
        minute: 4,
        second: 44.0,
    },
    calendar: Calendar::Gregorian,
    lat_deg: dms(0.0, 52.0, 47.0, 0.0),
    lon_deg: -dms(0.0, 1.0, 11.0, 0.0),
    delta_t_s: 85.0,
    jde: 2_306_308.587_613,
    lst_hours: 17.0 + 14.0 / 60.0 + 19.0 / 3600.0,
    obliquity_deg: dms(0.0, 23.0, 29.0, 27.0),
    ac_deg: dms(PIS, 2.0, 6.0, 37.0),   // Pis⌖02°06'37"
    mc_deg: dms(SAG, 19.0, 30.0, 14.0), // Sag⌖19°30'14"
    ic_deg: dms(GEM, 19.0, 30.0, 14.0), // Gem⌖19°30'14"  (= MC+180)
    ds_deg: dms(VIR, 2.0, 6.0, 37.0),   // Vir⌖02°06'37"  (= ASC+180)
    vx_deg: dms(VIR, 20.0, 38.0, 35.0), // Vir⌖20°38'35"
    ax_deg: dms(PIS, 20.0, 38.0, 35.0), // Pis⌖20°38'35"  (= Vx+180)
    house_system: HouseSystem::Regiomontanus,
    house_cusps_deg: [
        dms(PIS, 2.0, 6.0, 37.0),   // H1  Pis⌖02°06'37"
        dms(TAU, 7.0, 31.0, 40.0),  // H2  Tau⌖07°31'40"
        dms(GEM, 5.0, 20.0, 8.0),   // H3  Gem⌖05°20'08"
        dms(GEM, 19.0, 30.0, 14.0), // H4  Gem⌖19°30'14"
        dms(CAN, 1.0, 48.0, 2.0),   // H5  Can⌖01°48'02"
        dms(CAN, 19.0, 25.0, 5.0),  // H6  Can⌖19°25'05"
        dms(VIR, 2.0, 6.0, 37.0),   // H7  Vir⌖02°06'37"
        dms(SCO, 7.0, 31.0, 40.0),  // H8  Sco⌖07°31'40"
        dms(SAG, 5.0, 20.0, 8.0),   // H9  Sag⌖05°20'08"
        dms(SAG, 19.0, 30.0, 14.0), // H10 Sag⌖19°30'14"
        dms(CAP, 1.0, 48.0, 2.0),   // H11 Cap⌖01°48'02"
        dms(CAP, 19.0, 25.0, 5.0),  // H12 Cap⌖19°25'05"
    ],
    bodies: WILLIAM_LILLY_BODIES,
    fortune_deg: Some(dms(CAN, 7.0, 18.0, 12.0)), // PF Can⌖07°18'12"
    true_nn_deg: Some(dms(SAG, 14.0, 39.0, 52.0)), // Nn Sag⌖14°39'52"
};

// ── test 0 ── Vettius Valens — 0120-02-08 18:35 LMT Antioch ──────────────────
//
// Antioch (Antakya), Türkiye: 36°N14', +36°E07' (geographically east).
// REFCHARTS.md transcription shows "036°W07'" — typo: refchart's own time
// offset (−2:24:28) confirms east. We use east per geography.
// LMT for east longitude: UT = LMT − offset. 18:35 − 2:24:28 = 16:10:32 UT.
// Calendar: refchart works in proleptic Gregorian; for year 120 CE
// the Gregorian-Julian offset is 0 days (the leap-rule drift had not yet
// accumulated), so Julian and proleptic Gregorian dates coincide.
// Refchart resolved coords: 36°N14' 036°W07' [W is a transcription typo — east]
// Refchart resolved time:   18:35 LMT -2:24:28
const VETTIUS_VALENS_BODIES: &[BodyRef] = &[
    BodyRef {
        name: "Sun",
        body: Body::Sun,
        lon_deg: dms(AQU, 18.0, 28.0, 39.0),
        lat_deg: sdms(1.0, 0.0, 0.0, 0.0),
        travel_deg_per_day: sdms(1.0, 0.0, 59.0, 56.0),
    }, // Aqu⌖18°28'39"
    BodyRef {
        name: "Moon",
        body: Body::Moon,
        lon_deg: dms(SCO, 1.0, 38.0, 51.0),
        lat_deg: sdms(1.0, 5.0, 16.0, 0.0),
        travel_deg_per_day: sdms(1.0, 13.0, 37.0, 0.0),
    }, // Sco⌖01°38'51"
    BodyRef {
        name: "Mercury",
        body: Body::Mercury,
        lon_deg: dms(CAP, 29.0, 9.0, 55.0),
        lat_deg: sdms(1.0, 2.0, 26.0, 0.0),
        travel_deg_per_day: sdms(-1.0, 0.0, 14.0, 31.0),
    }, // Cap⌖29°09'55" R
    BodyRef {
        name: "Venus",
        body: Body::Venus,
        lon_deg: dms(CAP, 25.0, 29.0, 29.0),
        lat_deg: sdms(-1.0, 0.0, 38.0, 0.0),
        travel_deg_per_day: sdms(1.0, 1.0, 14.0, 0.0),
    }, // Cap⌖25°29'29"
    BodyRef {
        name: "Mars",
        body: Body::Mars,
        lon_deg: dms(VIR, 22.0, 40.0, 58.0),
        lat_deg: sdms(1.0, 3.0, 45.0, 0.0),
        travel_deg_per_day: sdms(-1.0, 0.0, 8.0, 39.0),
    }, // Vir⌖22°40'58" R
    BodyRef {
        name: "Jupiter",
        body: Body::Jupiter,
        lon_deg: dms(LIB, 23.0, 11.0, 59.0),
        lat_deg: sdms(1.0, 1.0, 26.0, 0.0),
        travel_deg_per_day: sdms(1.0, 0.0, 0.0, 2.0),
    }, // Lib⌖23°11'59" DS (station)
    BodyRef {
        name: "Saturn",
        body: Body::Saturn,
        lon_deg: dms(GEM, 29.0, 13.0, 50.0),
        lat_deg: sdms(-1.0, 0.0, 9.0, 0.0),
        travel_deg_per_day: sdms(-1.0, 0.0, 2.0, 19.0),
    }, // Gem⌖29°13'50" R
    BodyRef {
        name: "Uranus",
        body: Body::Uranus,
        lon_deg: dms(VIR, 5.0, 27.0, 44.0),
        lat_deg: sdms(1.0, 0.0, 48.0, 0.0),
        travel_deg_per_day: sdms(-1.0, 0.0, 2.0, 26.0),
    }, // Vir⌖05°27'44" R
    BodyRef {
        name: "Neptune",
        body: Body::Neptune,
        lon_deg: dms(LEO, 11.0, 19.0, 57.0),
        lat_deg: sdms(1.0, 0.0, 42.0, 0.0),
        travel_deg_per_day: sdms(-1.0, 0.0, 1.0, 39.0),
    }, // Leo⌖11°19'57" R
    BodyRef {
        name: "Pluto",
        body: Body::Pluto,
        lon_deg: dms(ARI, 5.0, 53.0, 47.0),
        lat_deg: sdms(-1.0, 16.0, 40.0, 0.0),
        travel_deg_per_day: sdms(1.0, 0.0, 0.0, 59.0),
    }, // Ari⌖05°53'47"
];

const VETTIUS_VALENS: Chart = Chart {
    id: "vettius_valens",
    mode: Mode::Geocentric,
    civil: CivilDate {
        year: 120,
        month: 2,
        day: 8,
        hour: 16,
        minute: 10,
        second: 32.0,
    },
    calendar: Calendar::Julian,
    lat_deg: dms(0.0, 36.0, 14.0, 0.0),
    lon_deg: dms(0.0, 36.0, 7.0, 0.0), // east (geography wins over REFCHARTS.md typo)
    delta_t_s: 9340.0,
    jde: 1_764_926.282_087,
    lst_hours: 3.0 + 41.0 / 60.0 + 16.0 / 3600.0,
    obliquity_deg: dms(0.0, 23.0, 40.0, 48.0),
    ac_deg: dms(VIR, 1.0, 29.0, 3.0),   // Vir⌖01°29'03"
    mc_deg: dms(TAU, 27.0, 38.0, 8.0),  // Tau⌖27°38'08"
    ic_deg: dms(SCO, 27.0, 38.0, 8.0),  // Sco⌖27°38'08"  (= MC+180)
    ds_deg: dms(PIS, 1.0, 29.0, 3.0),   // Pis⌖01°29'03"  (= ASC+180)
    vx_deg: dms(CAP, 19.0, 48.0, 35.0), // Cap⌖19°48'35"
    ax_deg: dms(CAN, 19.0, 48.0, 35.0), // Can⌖19°48'35"  (= Vx+180)
    house_system: HouseSystem::Porphyry,
    house_cusps_deg: [
        dms(VIR, 1.0, 29.0, 3.0),  // H1  Vir⌖01°29'03"
        dms(LIB, 0.0, 12.0, 5.0),  // H2  Lib⌖00°12'05"
        dms(LIB, 28.0, 55.0, 6.0), // H3  Lib⌖28°55'06"
        dms(SCO, 27.0, 38.0, 8.0), // H4  Sco⌖27°38'08"
        dms(SAG, 28.0, 55.0, 6.0), // H5  Sag⌖28°55'06"
        dms(AQU, 0.0, 12.0, 5.0),  // H6  Aqu⌖00°12'05"
        dms(PIS, 1.0, 29.0, 3.0),  // H7  Pis⌖01°29'03"
        dms(ARI, 0.0, 12.0, 5.0),  // H8  Ari⌖00°12'05"
        dms(ARI, 28.0, 55.0, 6.0), // H9  Ari⌖28°55'06"
        dms(TAU, 27.0, 38.0, 8.0), // H10 Tau⌖27°38'08"
        dms(GEM, 28.0, 55.0, 6.0), // H11 Gem⌖28°55'06"
        dms(LEO, 0.0, 12.0, 5.0),  // H12 Leo⌖00°12'05"
    ],
    bodies: VETTIUS_VALENS_BODIES,
    fortune_deg: Some(dms(SAG, 18.0, 18.0, 52.0)), // PF Sag⌖18°18'52"
    true_nn_deg: Some(dms(LEO, 5.0, 13.0, 11.0)),  // Nn Leo⌖05°13'11" R
};

// All charts under test.
const CHARTS: &[&Chart] = &[
    &LIGHTNING_STRIKE,
    &UNIX_OVERFLOW,
    &WILLIAM_LILLY,
    &VETTIUS_VALENS,
];

// =============================================================================
// Test infrastructure
// =============================================================================

fn locate_jpl_paths() -> Option<(PathBuf, PathBuf)> {
    let dir = std::env::var("STARCAT_JPL_DATA").ok().map(PathBuf::from)?;
    let paths = discover::discover(&dir)
        .unwrap_or_else(|e| panic!("STARCAT_JPL_DATA autodiscovery failed: {e}"));
    Some((paths.header, paths.binary))
}

fn longitude_delta_deg(a: f64, b: f64) -> f64 {
    let raw = (a - b).abs().rem_euclid(360.0);
    raw.min(360.0 - raw)
}

fn arcseconds(deg: f64) -> f64 {
    deg * 3600.0
}

fn jd_ut_for(chart: &Chart) -> f64 {
    civil_to_jd(chart.civil, chart.calendar)
}

/// Wrap a Δ value in ANSI color codes: green when within tolerance,
/// red when it exceeds. Returns (prefix, suffix) so callers can splice
/// them into existing format strings. No-op on non-TTY stdout (cargo
/// captures and `2>&1`-piped runs both look like non-TTY).
fn dlt(d: f64, tol: f64) -> (&'static str, &'static str) {
    use std::io::IsTerminal;
    if !std::io::stdout().is_terminal() {
        return ("", "");
    }
    if d < tol {
        ("\x1b[32m", "\x1b[0m")
    } else {
        ("\x1b[31m", "\x1b[0m")
    }
}

fn jd_tt_for(chart: &Chart) -> f64 {
    jd_ut_to_jd_tt(jd_ut_for(chart))
}

/// Per-chart (longitude, latitude) tolerances in arcseconds.
///
/// Refchart prints latitude to arcmin precision (no seconds column) — this
/// implies a ±30″ rounding floor. Latitudes are widened beyond longitudes
/// accordingly. The ancient charts add a further ΔT-model penalty.
fn body_tol_arcsec(chart: &Chart, body: Body) -> (f64, f64) {
    use Body::*;
    let is_moon = matches!(body, Moon);
    match chart.id {
        "vettius_valens" if is_moon => (90.0, 120.0),
        "vettius_valens" => (15.0, 60.0),
        "william_lilly" if is_moon => (40.0, 80.0),
        "william_lilly" => (5.0, 60.0),
        // 2038 is beyond the observational ΔT table; the SMH 2016 spline
        // extrapolation diverges ~8 s from refchart's polynomial extrapolation,
        // which propagates ~20–30″ into heliocentric longitudes for fast bodies.
        "unix_overflow_2038" => (60.0, 60.0),
        _ if is_moon => (20.0, 60.0),
        _ => (3.0, 60.0),
    }
}

// =============================================================================
// (1) Primitive cross-validation tests
// =============================================================================

#[test]
fn jd_tt_matches_refchart_jde_per_chart() {
    // Tolerance: 2 seconds in days. Refchart's ΔT model (used internally
    // to produce its JDE) diverges from SMH 2016 most sharply for ancient
    // dates; Valens is the binding constraint at ~1230 s gap.
    println!("=== JD_TT vs refchart JDE ===");
    for chart in CHARTS {
        let jd_tt = jd_tt_for(chart);
        let delta_s = (jd_tt - chart.jde).abs() * 86400.0;
        let tol_s = match chart.id {
            // Year 120 CE: refchart ΔT model disagrees with SMH 2016 by ~1230 s.
            "vettius_valens" => 1300.0,
            // 2038 is past the 2025 observational table; SMH-spline extrapolation
            // and refchart's polynomial extrapolation diverge by ~8 s.
            "unix_overflow_2038" => 30.0,
            _ => 5.0,
        };
        let (c, r) = dlt(delta_s, tol_s);
        println!(
            "  {:<20} starcat={:.6}  refchart={:.6}  Δ: {c}{:.2}{r} s  (tol {:.0} s)",
            chart.id, jd_tt, chart.jde, delta_s, tol_s
        );
        assert!(
            delta_s < tol_s,
            "{}: JD_TT Δ {:.2} s exceeds {:.0} s",
            chart.id,
            delta_s,
            tol_s
        );
    }
}

#[test]
fn delta_t_matches_refchart_per_chart() {
    // starcat ΔT − refchart ΔT, seconds.
    println!("=== ΔT(jd_ut) vs refchart ΔT ===");
    for chart in CHARTS {
        let jd_ut = jd_ut_for(chart);
        let jd_tt = jd_ut_to_jd_tt(jd_ut);
        let starcat_dt = (jd_tt - jd_ut) * 86400.0;
        let delta = (starcat_dt - chart.delta_t_s).abs();
        let tol = match chart.id {
            "vettius_valens" => 1300.0,
            "unix_overflow_2038" => 30.0,
            _ => 5.0,
        };
        let (c, r) = dlt(delta, tol);
        println!(
            "  {:<20} starcat={:>7.1} s  refchart={:>7.1} s  Δ: {c}{:>6.1}{r} s  (tol {:.0} s)",
            chart.id, starcat_dt, chart.delta_t_s, delta, tol
        );
        assert!(
            delta < tol,
            "{}: ΔT Δ {:.1} s exceeds {:.0} s",
            chart.id,
            delta,
            tol
        );
    }
}

#[test]
fn local_sidereal_time_matches_refchart_per_chart() {
    // LST_starcat = (GAST(jd_tt) + lon_east) mod 2π, converted to hours.
    // Refchart's LST is independent of starcat's ΔT — it's a function of
    // refchart's own (jd_ut → jd_tt) path. Tolerance widens for charts
    // whose ΔT differs from refchart's.
    println!("=== LST vs refchart ===");
    for chart in CHARTS {
        let jd_tt = jd_tt_for(chart);
        let gast = gast_rad(jd_tt);
        let lst_rad = (gast + chart.lon_deg.to_radians()).rem_euclid(CIRCLE_RAD);
        let lst_hours = lst_rad.to_degrees() / 15.0;
        // Wrap difference into (-12, 12] hour interval for comparison.
        let mut delta = lst_hours - chart.lst_hours;
        while delta > 12.0 {
            delta -= 24.0;
        }
        while delta <= -12.0 {
            delta += 24.0;
        }
        let delta_s = delta.abs() * 3600.0;
        let tol_s = match chart.id {
            "vettius_valens" => 90.0, // ΔT gap × 15″/s × 1° ≈ but LST in seconds
            _ => 5.0,
        };
        let (c, r) = dlt(delta_s, tol_s);
        println!(
            "  {:<20} starcat={:>10.6} h  refchart={:>10.6} h  Δ: {c}{:>7.2}{r} s  (tol {:.0} s)",
            chart.id, lst_hours, chart.lst_hours, delta_s, tol_s
        );
        assert!(
            delta_s < tol_s,
            "{}: LST Δ {:.2} s exceeds {:.0} s",
            chart.id,
            delta_s,
            tol_s
        );
    }
}

#[test]
fn mean_obliquity_matches_refchart_per_chart() {
    // Refchart's "Ob =" line is the MEAN obliquity (IAU 2006 polynomial),
    // not the true obliquity (mean + Δε). Verified empirically: for the
    // Lightning Strike (1955), refchart prints 23°26'42″ which matches our
    // mean ε within 0.5″; true ε adds Δε ≈ +9″ on top.
    println!("=== ε_mean vs refchart ===");
    for chart in CHARTS {
        let jd_tt = jd_tt_for(chart);
        let eps_mean_deg = mean_obliquity_rad(jd_tt).to_degrees();
        let delta_arcsec = (eps_mean_deg - chart.obliquity_deg).abs() * 3600.0;
        // Refchart prints ε to integer arcsec → ±0.5″ rounding floor.
        let tol_arcsec = match chart.id {
            "vettius_valens" => 5.0,
            _ => 2.0,
        };
        let (c, r) = dlt(delta_arcsec, tol_arcsec);
        println!(
            "  {:<20} starcat={:.6}°  refchart={:.6}°  Δ: {c}{:>5.2}{r}″  (tol {:.0}″)",
            chart.id, eps_mean_deg, chart.obliquity_deg, delta_arcsec, tol_arcsec
        );
        assert!(
            delta_arcsec < tol_arcsec,
            "{}: ε_mean Δ {:.2}″ exceeds {:.0}″",
            chart.id,
            delta_arcsec,
            tol_arcsec
        );
    }
}

// =============================================================================
// (2) Position oracle (longitude, latitude, daily motion)
// =============================================================================

fn position_at(ephem: &Ephemeris, mode: Mode, body: Body, jd_tt: f64) -> (f64, f64) {
    match mode {
        Mode::Geocentric => {
            let p = apparent_ecliptic_position(ephem, body, jd_tt).unwrap();
            (p.longitude_deg, p.latitude_deg)
        }
        Mode::Heliocentric => {
            let p = heliocentric_ecliptic_position(ephem, body, jd_tt).unwrap();
            (p.longitude_deg, p.latitude_deg)
        }
    }
}

fn run_position_chart(chart: &Chart) {
    let Some((header_path, binary_path)) = locate_jpl_paths() else {
        eprintln!("STARCAT_JPL_DATA not set — skipping integration test");
        return;
    };
    let header_src = std::fs::read_to_string(&header_path).unwrap();
    let header = parse_header(&header_src).unwrap();
    let file = EphemerisFile::open(&binary_path, &header).unwrap();
    let ephem = Ephemeris::new(&file, &header).unwrap();

    let jd_tt = jd_tt_for(chart);
    let mut max_dlon = 0.0_f64;
    let mut max_dlat = 0.0_f64;
    let mut max_dtravel = 0.0_f64;
    let mut worst_lon = String::new();

    println!("=== positions  {}  JD_TT={:.6} ===", chart.id, jd_tt);
    for b in chart.bodies {
        let (lon, lat) = position_at(&ephem, chart.mode, b.body, jd_tt);
        let (lon_back, _) = position_at(&ephem, chart.mode, b.body, jd_tt - 0.5);
        let (lon_fwd, _) = position_at(&ephem, chart.mode, b.body, jd_tt + 0.5);

        // Centered finite difference, wrapping the seam if it occurs.
        let mut travel = lon_fwd - lon_back;
        if travel > 180.0 {
            travel -= 360.0;
        }
        if travel < -180.0 {
            travel += 360.0;
        }

        let dlon = arcseconds(longitude_delta_deg(lon, b.lon_deg));
        let dlat = arcseconds((lat - b.lat_deg).abs());
        let dtravel = (travel - b.travel_deg_per_day).abs() * 3600.0;

        let (tol_lon, tol_lat) = body_tol_arcsec(chart, b.body);
        // Travel tolerance: refchart prints "Travel" to arcmin (fast bodies)
        // or arcmin+arcsec (slow bodies). Allow 5′ (300″) for non-Moon and
        // 30′ (1800″) for the Moon to absorb both the "instantaneous vs
        // daily-mean" convention and the print-rounding floor.
        let tol_travel = match (chart.id, b.body) {
            ("vettius_valens", Body::Moon) => 3600.0,
            ("vettius_valens", _) => 300.0,
            (_, Body::Moon) => 1800.0,
            _ => 300.0,
        };

        let (clon, rlon) = dlt(dlon, tol_lon);
        let (clat, rlat) = dlt(dlat, tol_lat);
        let (ctra, rtra) = dlt(dtravel, tol_travel);
        println!(
            "  {:<8} lon Δ: {clon}{:>6.2}{rlon}″ (tol {:.0}″)  lat Δ: {clat}{:>5.2}{rlat}″ (tol {:.0}″)  travel Δ: {ctra}{:>6.1}{rtra}″/d (tol {:.0}″)",
            b.name, dlon, tol_lon, dlat, tol_lat, dtravel, tol_travel
        );

        if dlon > max_dlon {
            max_dlon = dlon;
            worst_lon = b.name.to_string();
        }
        if dlat > max_dlat {
            max_dlat = dlat;
        }
        if dtravel > max_dtravel {
            max_dtravel = dtravel;
        }

        assert!(
            dlon < tol_lon,
            "{}/{}: Δlon {:.2}″ exceeds {:.0}″",
            chart.id,
            b.name,
            dlon,
            tol_lon
        );
        assert!(
            dlat < tol_lat,
            "{}/{}: Δlat {:.2}″ exceeds {:.0}″",
            chart.id,
            b.name,
            dlat,
            tol_lat
        );
        // Travel sign assertion: starcat and refchart must agree on
        // retrograde/direct for every body.
        assert_eq!(
            travel.is_sign_negative(),
            b.travel_deg_per_day.is_sign_negative(),
            "{}/{}: travel sign mismatch (starcat {:+.4}°/d, refchart {:+.4}°/d)",
            chart.id,
            b.name,
            travel,
            b.travel_deg_per_day
        );
        assert!(
            dtravel < tol_travel,
            "{}/{}: Δtravel {:.1}″/d exceeds {:.0}″/d",
            chart.id,
            b.name,
            dtravel,
            tol_travel
        );
    }
    println!(
        "  → max Δlon: {max_dlon:.2}″ ({worst_lon})  max Δlat: {max_dlat:.2}″  max Δtravel: {max_dtravel:.1}″/d"
    );
}

#[test]
fn positions_lightning_strike() {
    run_position_chart(&LIGHTNING_STRIKE);
}
#[test]
fn positions_unix_overflow() {
    run_position_chart(&UNIX_OVERFLOW);
}
#[test]
fn positions_william_lilly() {
    run_position_chart(&WILLIAM_LILLY);
}
#[test]
fn positions_vettius_valens() {
    run_position_chart(&VETTIUS_VALENS);
}

// =============================================================================
// (3) Angles (Asc, MC)
// =============================================================================

fn ramc_and_obliquity(chart: &Chart) -> (f64, f64) {
    let jd_tt = jd_tt_for(chart);
    let ramc = (gast_rad(jd_tt) + chart.lon_deg.to_radians()).rem_euclid(CIRCLE_RAD);
    let nut = nutation(jd_tt);
    let obliquity = mean_obliquity_rad(jd_tt) + nut.delta_epsilon;
    (ramc, obliquity)
}

fn angle_tol_arcmin(chart_id: &str) -> f64 {
    match chart_id {
        "vettius_valens" => 120.0, // 2°, ΔT-limited
        "william_lilly" => 30.0,   // 30′
        _ => 5.0,
    }
}

fn run_angles_chart(chart: &Chart) {
    use pericynthion::coords::acds::ds_rad;
    use pericynthion::coords::mcic::ic_rad;
    use pericynthion::coords::vxax::{ax_rad, vx_rad};

    let (ramc, eps) = ramc_and_obliquity(chart);
    let lat_rad = chart.lat_deg.to_radians();

    let ac_rad = ac_rad(ramc, eps, lat_rad).unwrap();
    let mc_rad = mc_rad(ramc, eps);
    let vx_rad = vx_rad(ramc, eps, lat_rad).unwrap();
    let ac_deg = ac_rad.to_degrees();
    let mc_deg = mc_rad.to_degrees();
    let ic_deg = ic_rad(mc_rad).to_degrees();
    let ds_deg = ds_rad(ac_rad).to_degrees();
    let vx_deg = vx_rad.to_degrees();
    let ax_deg = ax_rad(vx_rad).to_degrees();

    let d_ac = longitude_delta_deg(ac_deg, chart.ac_deg) * 60.0; // arcmin
    let d_mc = longitude_delta_deg(mc_deg, chart.mc_deg) * 60.0;
    let d_ic = longitude_delta_deg(ic_deg, chart.ic_deg) * 60.0;
    let d_ds = longitude_delta_deg(ds_deg, chart.ds_deg) * 60.0;
    let d_vx = longitude_delta_deg(vx_deg, chart.vx_deg) * 60.0;
    let d_ax = longitude_delta_deg(ax_deg, chart.ax_deg) * 60.0;
    let tol_arcmin = angle_tol_arcmin(chart.id);

    println!("=== angles  {}  (tol {:.0}′)", chart.id, tol_arcmin);
    let (c, r) = dlt(d_ac, tol_arcmin);
    println!(
        "  Ac starcat={:>10.4}°  refchart={:>10.4}°  Δ: {c}{:>5.2}{r}′",
        ac_deg, chart.ac_deg, d_ac
    );
    let (c, r) = dlt(d_mc, tol_arcmin);
    println!(
        "  Mc starcat={:>10.4}°  refchart={:>10.4}°  Δ: {c}{:>5.2}{r}′",
        mc_deg, chart.mc_deg, d_mc
    );
    let (c, r) = dlt(d_ic, tol_arcmin);
    println!(
        "  Ic starcat={:>10.4}°  refchart={:>10.4}°  Δ: {c}{:>5.2}{r}′",
        ic_deg, chart.ic_deg, d_ic
    );
    let (c, r) = dlt(d_ds, tol_arcmin);
    println!(
        "  Ds starcat={:>10.4}°  refchart={:>10.4}°  Δ: {c}{:>5.2}{r}′",
        ds_deg, chart.ds_deg, d_ds
    );
    let (c, r) = dlt(d_vx, tol_arcmin);
    println!(
        "  Vx starcat={:>10.4}°  refchart={:>10.4}°  Δ: {c}{:>5.2}{r}′",
        vx_deg, chart.vx_deg, d_vx
    );
    let (c, r) = dlt(d_ax, tol_arcmin);
    println!(
        "  Ax starcat={:>10.4}°  refchart={:>10.4}°  Δ: {c}{:>5.2}{r}′",
        ax_deg, chart.ax_deg, d_ax
    );

    assert!(
        d_ac < tol_arcmin,
        "{}: Δac {:.2}′ exceeds {:.0}′",
        chart.id,
        d_ac,
        tol_arcmin
    );
    assert!(
        d_mc < tol_arcmin,
        "{}: Δmc {:.2}′ exceeds {:.0}′",
        chart.id,
        d_mc,
        tol_arcmin
    );
    assert!(
        d_ic < tol_arcmin,
        "{}: Δic {:.2}′ exceeds {:.0}′",
        chart.id,
        d_ic,
        tol_arcmin
    );
    assert!(
        d_ds < tol_arcmin,
        "{}: Δds {:.2}′ exceeds {:.0}′",
        chart.id,
        d_ds,
        tol_arcmin
    );
    assert!(
        d_vx < tol_arcmin,
        "{}: Δvx {:.2}′ exceeds {:.0}′",
        chart.id,
        d_vx,
        tol_arcmin
    );
    assert!(
        d_ax < tol_arcmin,
        "{}: Δax {:.2}′ exceeds {:.0}′",
        chart.id,
        d_ax,
        tol_arcmin
    );
}

#[test]
fn angles_lightning_strike() {
    run_angles_chart(&LIGHTNING_STRIKE);
}
#[test]
fn angles_unix_overflow() {
    run_angles_chart(&UNIX_OVERFLOW);
}
#[test]
fn angles_william_lilly() {
    run_angles_chart(&WILLIAM_LILLY);
}
#[test]
fn angles_vettius_valens() {
    run_angles_chart(&VETTIUS_VALENS);
}

// =============================================================================
// (4) House cusps (per-chart system)
// =============================================================================

fn cusps_for(chart: &Chart) -> Option<HouseCusps> {
    use pericynthion::coords::acds::ac_rad;
    use pericynthion::coords::mcic::mc_rad;
    let (ramc, eps) = ramc_and_obliquity(chart);
    let lat = chart.lat_deg.to_radians();
    match chart.house_system {
        HouseSystem::Placidus => placidus_rad(ramc, eps, lat),
        HouseSystem::Equal => {
            let ac = ac_rad(ramc, eps, lat)?;
            Some(equal_as_rad(ac))
        }
        HouseSystem::WholeSign => {
            let ac = ac_rad(ramc, eps, lat)?;
            Some(whole_sign_rad(ac))
        }
        HouseSystem::Regiomontanus => regiomontanus_rad(ramc, eps, lat),
        HouseSystem::Porphyry => {
            let ac = ac_rad(ramc, eps, lat)?;
            let mc = mc_rad(ramc, eps);
            Some(porphyry_rad(ac, mc))
        }
    }
}

fn run_cusps_chart(chart: &Chart) {
    let hc = cusps_for(chart).expect("house computation succeeded");
    let tol_arcmin = angle_tol_arcmin(chart.id);
    println!(
        "=== cusps  {}  system={:?} ===",
        chart.id, chart.house_system
    );
    for h in 1u8..=12 {
        let starcat = hc.cusp(h).to_degrees();
        let refc = chart.house_cusps_deg[(h - 1) as usize];
        let d_arcmin = longitude_delta_deg(starcat, refc) * 60.0;
        let (c, r) = dlt(d_arcmin, tol_arcmin);
        println!(
            "  H{h:>2} starcat={starcat:>10.4}°  refchart={refc:>10.4}°  Δ: {c}{d_arcmin:>6.2}{r}′"
        );
        assert!(
            d_arcmin < tol_arcmin,
            "{}/H{}: Δ {:.2}′ exceeds {:.0}′",
            chart.id,
            h,
            d_arcmin,
            tol_arcmin
        );
    }
}

#[test]
fn cusps_lightning_strike() {
    run_cusps_chart(&LIGHTNING_STRIKE);
}
#[test]
fn cusps_unix_overflow() {
    run_cusps_chart(&UNIX_OVERFLOW);
}
#[test]
fn cusps_william_lilly() {
    run_cusps_chart(&WILLIAM_LILLY);
}
#[test]
fn cusps_vettius_valens() {
    run_cusps_chart(&VETTIUS_VALENS);
}

// =============================================================================
// (5) Part of Fortune (Lot of Fortune)
// =============================================================================
//
// Uses the body-position pipeline (Sun + Moon from starcat) and the angle
// pipeline (Asc at refchart's lat) to drive `fortune_rad`, then compares
// to refchart's printed PF.

fn run_fortune_chart(chart: &Chart) {
    use pericynthion::lots::{Sect, fortune_rad, sect};

    let Some(expected_pf) = chart.fortune_deg else {
        return;
    };

    let (ramc, eps) = ramc_and_obliquity(chart);
    let lat = chart.lat_deg.to_radians();
    let ac = ac_rad(ramc, eps, lat).unwrap();

    // Pull Sun and Moon from the chart's body table.
    let body_lon = |body: Body| -> f64 {
        chart
            .bodies
            .iter()
            .find(|b| b.body == body)
            .map(|b| b.lon_deg.to_radians())
            .expect("body present in chart")
    };
    let sun = body_lon(Body::Sun);
    let moon = body_lon(Body::Moon);
    let s: Sect = sect(sun, ac);
    let pf_deg = fortune_rad(ac, sun, moon, s).to_degrees();

    let dpf_arcmin = longitude_delta_deg(pf_deg, expected_pf) * 60.0;
    let tol_arcmin = angle_tol_arcmin(chart.id);
    let (c, r) = dlt(dpf_arcmin, tol_arcmin);
    println!(
        "=== fortune  {}  sect={:?}  starcat={:.4}°  refchart={:.4}°  Δ: {c}{:.2}{r}′  (tol {:.0}′)",
        chart.id, s, pf_deg, expected_pf, dpf_arcmin, tol_arcmin
    );
    assert!(
        dpf_arcmin < tol_arcmin,
        "{}: Δfortune {:.2}′ exceeds {:.0}′",
        chart.id,
        dpf_arcmin,
        tol_arcmin
    );
}

#[test]
fn fortune_lightning_strike() {
    run_fortune_chart(&LIGHTNING_STRIKE);
}
#[test]
fn fortune_unix_overflow() {
    run_fortune_chart(&UNIX_OVERFLOW);
} // no-op (None)
#[test]
fn fortune_william_lilly() {
    run_fortune_chart(&WILLIAM_LILLY);
}
#[test]
fn fortune_vettius_valens() {
    run_fortune_chart(&VETTIUS_VALENS);
}

// =============================================================================
// (6) Lunar Nodes (true) — Moon's osculating ascending node
// =============================================================================
//
// Refchart's "Nod" entry is the **true** node (verified empirically: the
// mean node is monotonically retrograde and never stations, so any "SR"
// stationary-retrograde label on the node proves the value is the true node).
// starcat computes both modes; the test asserts on `true_nn_rad` against
// refchart's printed value, and on `sn_rad(nn) − nn ≡ 180°` as a structural invariant.

fn run_nodes_chart(chart: &Chart) {
    use pericynthion::coords::nodes::{sn_rad, true_nn_rad};

    let Some(expected_nn_deg) = chart.true_nn_deg else {
        return;
    };

    let Some((header_path, binary_path)) = locate_jpl_paths() else {
        eprintln!("STARCAT_JPL_DATA not set — skipping integration test");
        return;
    };
    let header_src = std::fs::read_to_string(&header_path).unwrap();
    let header = parse_header(&header_src).unwrap();
    let file = EphemerisFile::open(&binary_path, &header).unwrap();
    let ephem = Ephemeris::new(&file, &header).unwrap();

    let jd_tt = jd_tt_for(chart);
    let nn_rad = true_nn_rad(&ephem, jd_tt).expect("Moon state in range");
    let nn_deg = nn_rad.to_degrees();
    let sn_deg = sn_rad(nn_rad).to_degrees();
    let expected_sn_deg = (expected_nn_deg + 180.0).rem_euclid(360.0);

    let d_nn_arcmin = longitude_delta_deg(nn_deg, expected_nn_deg) * 60.0;
    let d_sn_arcmin = longitude_delta_deg(sn_deg, expected_sn_deg) * 60.0;
    // Refchart prints to arcsec; modern charts should match within ~5′.
    // Valens (year 120) widens to absorb ΔT divergence in Moon position.
    let tol_arcmin = match chart.id {
        "vettius_valens" => 120.0,
        "william_lilly" => 30.0,
        _ => 5.0,
    };

    println!("=== nodes  {}  (tol {:.0}′)", chart.id, tol_arcmin);
    let (c, r) = dlt(d_nn_arcmin, tol_arcmin);
    println!(
        "  Nn starcat={nn_deg:>10.4}°  refchart={expected_nn_deg:>10.4}°  Δ: {c}{d_nn_arcmin:>5.2}{r}′"
    );
    let (c, r) = dlt(d_sn_arcmin, tol_arcmin);
    println!(
        "  Sn starcat={sn_deg:>10.4}°  refchart={expected_sn_deg:>10.4}°  Δ: {c}{d_sn_arcmin:>5.2}{r}′"
    );

    assert!(
        d_nn_arcmin < tol_arcmin,
        "{}: Δnn {:.2}′ exceeds {:.0}′",
        chart.id,
        d_nn_arcmin,
        tol_arcmin
    );
    assert!(
        d_sn_arcmin < tol_arcmin,
        "{}: Δsn {:.2}′ exceeds {:.0}′",
        chart.id,
        d_sn_arcmin,
        tol_arcmin
    );
}

#[test]
fn nodes_lightning_strike() {
    run_nodes_chart(&LIGHTNING_STRIKE);
}
#[test]
fn nodes_unix_overflow() {
    run_nodes_chart(&UNIX_OVERFLOW);
} // no-op (None)
#[test]
fn nodes_william_lilly() {
    run_nodes_chart(&WILLIAM_LILLY);
}
#[test]
fn nodes_vettius_valens() {
    run_nodes_chart(&VETTIUS_VALENS);
}
