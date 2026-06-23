//! Feature-gated JZOD export for [`ComputedChart`].
//!
//! This module is compiled only when the `jzod` Cargo feature is enabled. It
//! provides:
//!
//! - [`ChartBirth`] — per-chart metadata (date, time, location) that the
//!   numeric layer does not own and therefore cannot include in
//!   [`ComputedChart`].
//! - [`impl From<crate::body::Body> for jzod::BodyId`] — zero-copy
//!   conversion from the ephemeris body enum to the JZOD wire identifier.
//! - [`house_for`] — return the 1-based house number for a given ecliptic
//!   longitude given a set of house cusps.
//! - [`to_jzod_chart`] — assemble a [`jzod::Chart`] from a [`ComputedChart`]
//!   and a [`ChartBirth`]. The caller supplies a pre-generated `uid` string
//!   so that `pericynthion` does not depend on the `uuid` crate.
//!
//! # Usage
//!
//! ```toml
//! # Cargo.toml
//! [dependencies]
//! pericynthion = { version = "*", features = ["jzod"] }
//! ```
//!
//! ```rust,ignore
//! let chart = pericynthion::jzod::to_jzod_chart(&computed, birth, uid);
//! println!("{}", jzod::to_string_pretty(&jzod::JzodDocument::new(vec![chart])));
//! ```

use crate::body::Body;
use crate::chart::ComputedChart;
use crate::coords::phase::LunarPhaseName as P;
use crate::houses::{HouseCusps, HouseSystem};
use crate::lots::Sect;
use std::collections::BTreeMap;

/// Per-chart birth metadata that lives outside [`ComputedChart`].
///
/// `ComputedChart` holds the numeric results of an ephemeris computation;
/// it does not carry the original civil date/time fields or the geographic
/// coordinates verbatim. Callers supply these through `ChartBirth` so that
/// the JZOD `birth` block can be populated faithfully.
///
/// `utc_offset` is taken directly from [`ComputedChart::utc_offset`].
/// JD values come from [`ComputedChart::jd_ut`] / [`ComputedChart::jd_tt`].
#[derive(Debug, Clone)]
pub struct ChartBirth {
    /// Birth year (signed; negative for BCE).
    pub year: i32,
    /// Birth month, 1–12.
    pub month: u8,
    /// Birth day, 1–31.
    pub day: u8,
    /// Birth hour, 0–23.
    pub hour: u8,
    /// Birth minute, 0–59.
    pub minute: u8,
    /// Birth second, 0–59.
    pub second: u8,
    /// Geographic latitude in decimal degrees (ISO 6709: North positive).
    /// `None` when the birth location is unknown.
    pub lat: Option<f64>,
    /// Geographic longitude in decimal degrees (ISO 6709: East positive).
    /// `None` when the birth location is unknown.
    pub lon: Option<f64>,
}

impl From<Body> for jzod::BodyId {
    fn from(body: Body) -> jzod::BodyId {
        match body {
            Body::Sun => jzod::BodyId::Sun,
            Body::Moon => jzod::BodyId::Moon,
            Body::Mercury => jzod::BodyId::Mercury,
            Body::Venus => jzod::BodyId::Venus,
            Body::Earth => jzod::BodyId::Earth,
            Body::Mars => jzod::BodyId::Mars,
            Body::Jupiter => jzod::BodyId::Jupiter,
            Body::Saturn => jzod::BodyId::Saturn,
            Body::Uranus => jzod::BodyId::Uranus,
            Body::Neptune => jzod::BodyId::Neptune,
            Body::Pluto => jzod::BodyId::Pluto,
            Body::EarthMoonBarycenter => jzod::BodyId::EarthMoonBarycenter,
        }
    }
}

/// Map an SPK asteroid NAIF id to a [`jzod::BodyId`], when the JZOD model has a
/// variant for it. Handles both the sb441 (`2_000_000 + mpc`) and Horizons
/// (`20_000_000 + mpc`) id schemes. Every minor body in the placements catalog
/// has a corresponding JZOD variant, so `None` is returned only for ids that are
/// not catalog minor bodies (or a future body added without a `jzod::BodyId`).
#[must_use]
fn asteroid_naif_to_jzod_body_id(naif_id: i32) -> Option<jzod::BodyId> {
    let mpc = if (20_000_001..=20_999_999).contains(&naif_id) {
        naif_id - 20_000_000
    } else if (2_000_001..=2_999_999).contains(&naif_id) {
        naif_id - 2_000_000
    } else {
        return None;
    };
    Some(match mpc {
        1 => jzod::BodyId::Ceres,
        2 => jzod::BodyId::Pallas,
        3 => jzod::BodyId::Juno,
        4 => jzod::BodyId::Vesta,
        10 => jzod::BodyId::Hygiea,
        2_060 => jzod::BodyId::Chiron,
        50_000 => jzod::BodyId::Quaoar,
        90_377 => jzod::BodyId::Sedna,
        90_482 => jzod::BodyId::Orcus,
        136_108 => jzod::BodyId::Haumea,
        136_199 => jzod::BodyId::Eris,
        136_472 => jzod::BodyId::Makemake,
        225_088 => jzod::BodyId::Gonggong,
        5_145 => jzod::BodyId::Pholus,
        7_066 => jzod::BodyId::Nessus,
        10_199 => jzod::BodyId::Chariklo,
        28_978 => jzod::BodyId::Ixion,
        20_000 => jzod::BodyId::Varuna,
        _ => return None,
    })
}

/// Return the 1-based house number for `lon_deg` given the provided cusps.
///
/// Iterates through houses 1–12, testing whether the longitude falls within
/// each house's arc. The wrap-around case (house spanning 0°/360°) is
/// handled correctly. Falls back to house 1 when no house matches (should
/// not occur with valid cusps).
#[must_use]
pub fn house_for(lon_deg: f64, cusps: &HouseCusps) -> u8 {
    let lon = lon_deg.rem_euclid(360.0);
    for h in 1u8..=12 {
        let next = if h == 12 { 1 } else { h + 1 };
        let start = cusps.cusp(h).to_degrees().rem_euclid(360.0);
        let end = cusps.cusp(next).to_degrees().rem_euclid(360.0);
        let contains = if end > start {
            lon >= start && lon < end
        } else {
            // House straddles the 0°/360° boundary.
            lon >= start || lon < end
        };
        if contains {
            return h;
        }
    }
    1
}

/// Assemble a [`jzod::Chart`] from a [`ComputedChart`] and birth metadata.
///
/// # Parameters
///
/// - `computed` — the full numeric result from [`crate::chart::compute`].
/// - `birth` — civil date/time and location fields that accompany the chart.
/// - `uid` — a pre-generated unique identifier (e.g. `uuid::Uuid::new_v4().to_string()`).
///   Passed in by the caller so that `pericynthion` does not depend on `uuid`.
///
/// # Mapping notes
///
/// - **Zodiac** is always `Tropical` (the only zodiac this library computes).
/// - **Bodies**: speed and retrograde are read from `computed.bodies[i]`; no
///   recomputation is performed.
/// - **Nodes / Lilith**: both mean and true variants are read from
///   `computed.nodes` / `computed.lilith` respectively (no ephemeris call).
/// - **Vertex / Anti-Vertex**: read from `computed.angles`.
/// - **Houses**: `WholeSign` cusps use [`jzod::HouseCusp::whole_sign_from_longitude`];
///   all other systems use [`jzod::HouseCusp::from_longitude`].
/// - **`calculated_at`**: current wall-clock time via
///   [`jzod::time::calculated_at_now`].
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn to_jzod_chart(computed: &ComputedChart, birth: &ChartBirth, uid: String) -> jzod::Chart {
    // ── Coordinate system ────────────────────────────────────────────────────
    let coord_system = match &computed.mode {
        crate::chart::CoordMode::Geocentric => jzod::CoordinateSystem::Geocentric,
        crate::chart::CoordMode::Topocentric(_) => jzod::CoordinateSystem::Topocentric,
        crate::chart::CoordMode::Heliocentric => jzod::CoordinateSystem::Heliocentric,
    };

    // ── Sect ─────────────────────────────────────────────────────────────────
    let jzod_sect = computed.sect.map(|s| match s {
        Sect::Day => jzod::Sect::Diurnal,
        Sect::Night => jzod::Sect::Nocturnal,
    });

    // ── House assignment helper ──────────────────────────────────────────────
    let body_houses = |lon_deg: f64| -> BTreeMap<String, u8> {
        let mut map = BTreeMap::new();
        for (sys, cusps) in &computed.houses {
            if let Some(c) = cusps {
                map.insert(sys.slug().to_string(), house_for(lon_deg, c));
            }
        }
        map
    };

    // ── Bodies array (planets, then SPK asteroids) ──────────────────────────
    // Asteroids ride in the same body list as planets. Speed and retrograde
    // are read from ComputedAsteroid (computed at ±0.5 day). Asteroids the
    // JZOD body enum cannot name are omitted from JZOD output but still
    // appear in the text/page renderers.
    let mut bodies: Vec<jzod::placement::Body> = computed
        .bodies
        .iter()
        .map(|cb| jzod::placement::Body {
            id: jzod::BodyId::from(cb.body),
            position: jzod::coord::Position::from_longitude(cb.position.longitude_deg),
            ecliptic_latitude: jzod::coord::Degrees8(cb.position.latitude_deg),
            daily_speed: jzod::coord::Degrees8(cb.daily_speed_deg),
            retrograde: cb.retrograde,
            distance_au: Some(cb.position.distance_au),
            house: body_houses(cb.position.longitude_deg),
        })
        .collect();
    for ca in &computed.asteroids {
        let Some(id) = asteroid_naif_to_jzod_body_id(ca.naif_id) else {
            continue;
        };
        bodies.push(jzod::placement::Body {
            id,
            position: jzod::coord::Position::from_longitude(ca.position.longitude_deg),
            ecliptic_latitude: jzod::coord::Degrees8(ca.position.latitude_deg),
            daily_speed: jzod::coord::Degrees8(ca.daily_speed_deg),
            retrograde: ca.retrograde,
            distance_au: Some(ca.position.distance_au),
            house: body_houses(ca.position.longitude_deg),
        });
    }

    // ── Angles array (Ac, Ds, Mc, Ic — in that order when present) ──────────
    let mut angles_vec: Vec<jzod::Angle> = Vec::new();
    if let Some(a) = &computed.angles {
        if let Some(ac) = a.ac_deg {
            angles_vec.push(jzod::Angle {
                id: jzod::AngleId::Ascendant,
                position: jzod::coord::Position::from_longitude(ac),
            });
        }
        if let Some(ds) = a.ds_deg {
            angles_vec.push(jzod::Angle {
                id: jzod::AngleId::Descendant,
                position: jzod::coord::Position::from_longitude(ds),
            });
        }
        angles_vec.push(jzod::Angle {
            id: jzod::AngleId::Midheaven,
            position: jzod::coord::Position::from_longitude(a.mc_deg),
        });
        angles_vec.push(jzod::Angle {
            id: jzod::AngleId::ImumCoeli,
            position: jzod::coord::Position::from_longitude(a.ic_deg),
        });
    }

    // ── Points array: Vertex/Anti-Vertex, then Nodes, then Lilith ───────────
    // Suffixed PointIds resolve JZOD OQ-19 (mean/true both present).
    let mut points_vec: Vec<jzod::Point> = Vec::new();

    // Vertex axis — from angles.
    if let Some(a) = &computed.angles {
        if let Some(vx) = a.vx_deg {
            points_vec.push(jzod::Point {
                id: jzod::PointId::Vertex,
                position: jzod::coord::Position::from_longitude(vx),
                retrograde: false,
            });
        }
        if let Some(ax) = a.ax_deg {
            points_vec.push(jzod::Point {
                id: jzod::PointId::AntiVertex,
                position: jzod::coord::Position::from_longitude(ax),
                retrograde: false,
            });
        }
    }

    // Nodes — both mean and true, read from computed.nodes.
    if let Some(n) = &computed.nodes {
        points_vec.push(jzod::Point {
            id: jzod::PointId::NorthNodeMean,
            position: jzod::coord::Position::from_longitude(n.mean_nn_deg),
            retrograde: true, // mean node is always retrograde by construction
        });
        points_vec.push(jzod::Point {
            id: jzod::PointId::SouthNodeMean,
            position: jzod::coord::Position::from_longitude(n.mean_sn_deg),
            retrograde: true,
        });
        points_vec.push(jzod::Point {
            id: jzod::PointId::NorthNodeTrue,
            position: jzod::coord::Position::from_longitude(n.true_nn_deg),
            retrograde: n.true_retrograde,
        });
        points_vec.push(jzod::Point {
            id: jzod::PointId::SouthNodeTrue,
            position: jzod::coord::Position::from_longitude(n.true_sn_deg),
            retrograde: n.true_retrograde,
        });
    }

    // Black Moon Lilith — both mean and true, read from computed.lilith.
    if let Some(l) = &computed.lilith {
        points_vec.push(jzod::Point {
            id: jzod::PointId::BlackMoonLilithMean,
            position: jzod::coord::Position::from_longitude(l.mean_lilith_deg),
            retrograde: false, // mean Lilith is always prograde
        });
        points_vec.push(jzod::Point {
            id: jzod::PointId::PriapusMean,
            position: jzod::coord::Position::from_longitude(l.mean_priapus_deg),
            retrograde: false,
        });
        points_vec.push(jzod::Point {
            id: jzod::PointId::BlackMoonLilithTrue,
            position: jzod::coord::Position::from_longitude(l.true_lilith_deg),
            retrograde: l.true_retrograde,
        });
        points_vec.push(jzod::Point {
            id: jzod::PointId::PriapusTrue,
            position: jzod::coord::Position::from_longitude(l.true_priapus_deg),
            retrograde: l.true_retrograde,
        });
    }

    // ── Lots array ───────────────────────────────────────────────────────────
    let mut lots_vec: Vec<jzod::Lot> = Vec::new();
    if let Some(l) = &computed.lots {
        lots_vec.push(jzod::Lot {
            id: jzod::LotId::LotOfFortune,
            position: jzod::coord::Position::from_longitude(l.fortune_deg),
        });
        lots_vec.push(jzod::Lot {
            id: jzod::LotId::LotOfSpirit,
            position: jzod::coord::Position::from_longitude(l.spirit_deg),
        });
        lots_vec.push(jzod::Lot {
            id: jzod::LotId::LotOfExaltation,
            position: jzod::coord::Position::from_longitude(l.exaltation_deg),
        });
        if let Some(d) = l.necessity_deg {
            lots_vec.push(jzod::Lot {
                id: jzod::LotId::LotOfNecessity,
                position: jzod::coord::Position::from_longitude(d),
            });
        }
        if let Some(d) = l.eros_deg {
            lots_vec.push(jzod::Lot {
                id: jzod::LotId::LotOfEros,
                position: jzod::coord::Position::from_longitude(d),
            });
        }
        if let Some(d) = l.courage_deg {
            lots_vec.push(jzod::Lot {
                id: jzod::LotId::LotOfCourage,
                position: jzod::coord::Position::from_longitude(d),
            });
        }
        if let Some(d) = l.victory_deg {
            lots_vec.push(jzod::Lot {
                id: jzod::LotId::LotOfVictory,
                position: jzod::coord::Position::from_longitude(d),
            });
        }
        if let Some(d) = l.nemesis_deg {
            lots_vec.push(jzod::Lot {
                id: jzod::LotId::LotOfNemesis,
                position: jzod::coord::Position::from_longitude(d),
            });
        }
    }

    // ── Houses, keyed by system slug ─────────────────────────────────────────
    let mut jzod_houses: jzod::Houses = jzod::Houses::new();
    for (sys, cusps) in &computed.houses {
        if let Some(c) = cusps {
            let mut system_cusps: jzod::HouseSystemCusps = jzod::HouseSystemCusps::new();
            for h in 1u8..=12 {
                let lon_deg = c.cusp(h).to_degrees().rem_euclid(360.0);
                let cusp = if *sys == HouseSystem::WholeSign {
                    jzod::HouseCusp::whole_sign_from_longitude(lon_deg)
                } else {
                    jzod::HouseCusp::from_longitude(lon_deg)
                };
                system_cusps.insert(h, cusp);
            }
            jzod_houses.insert(sys.slug().to_string(), system_cusps);
        }
    }

    // ── Lunar phase ──────────────────────────────────────────────────────────
    let lunar_phase = computed.lunar_phase.as_ref().map(|lp| jzod::LunarPhase {
        synodic_arc_deg: lp.synodic_arc_deg,
        phase: match lp.phase {
            P::NewMoon => jzod::LunarPhaseName::NewMoon,
            P::Crescent => jzod::LunarPhaseName::Crescent,
            P::FirstQuarter => jzod::LunarPhaseName::FirstQuarter,
            P::Gibbous => jzod::LunarPhaseName::Gibbous,
            P::FullMoon => jzod::LunarPhaseName::FullMoon,
            P::Disseminating => jzod::LunarPhaseName::Disseminating,
            P::LastQuarter => jzod::LunarPhaseName::LastQuarter,
            P::Balsamic => jzod::LunarPhaseName::Balsamic,
        },
        lunation_day: lp.lunation_day,
    });

    // ── Assemble the chart ───────────────────────────────────────────────────
    jzod::Chart {
        uid,
        chart_type: jzod::ChartType::Radix,
        name: None,
        gender: None,
        rodden_rating: None,
        birth: jzod::Birth {
            datetime: jzod::Datetime {
                year: birth.year,
                month: birth.month,
                day: birth.day,
                hour: birth.hour,
                minute: birth.minute,
                second: birth.second,
                utc_offset: computed.utc_offset.clone(),
                iana_tz: None,
                unknown: false,
                tod_method: None,
            },
            location: jzod::Location {
                name: None,
                latitude: birth.lat,
                longitude: birth.lon,
            },
        },
        zodiac: jzod::Zodiac::Tropical,
        coordinate_system: coord_system,
        sect: jzod_sect,
        ephemeris: jzod::Ephemeris {
            source: "DE441".to_string(),
            calculated_at: jzod::time::calculated_at_now(),
            jd_ut: Some(computed.jd_ut),
            jd_tt: Some(computed.jd_tt),
        },
        placements: jzod::Placements {
            bodies,
            angles: angles_vec,
            points: points_vec,
            lots: lots_vec,
        },
        houses: jzod_houses,
        lunar_phase,
        nested: vec![],
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(all(test, feature = "jzod"))]
mod jzod_tests {
    #[test]
    fn asteroid_naif_maps_both_schemes_and_jzod_known_bodies() {
        use super::asteroid_naif_to_jzod_body_id as m;
        // big-5, sb441 scheme
        assert_eq!(m(2_000_001), Some(jzod::BodyId::Ceres));
        assert_eq!(m(2_000_010), Some(jzod::BodyId::Hygiea));
        // sb441 scheme, fetched bodies jzod knows
        assert_eq!(m(2_002_060), Some(jzod::BodyId::Chiron));
        assert_eq!(m(2_050_000), Some(jzod::BodyId::Quaoar));
        // Horizons scheme, big-5
        assert_eq!(m(20_000_001), Some(jzod::BodyId::Ceres));
        // Horizons scheme, fetched bodies jzod knows
        assert_eq!(m(20_002_060), Some(jzod::BodyId::Chiron));
        assert_eq!(m(20_136_199), Some(jzod::BodyId::Eris));
        assert_eq!(m(20_225_088), Some(jzod::BodyId::Gonggong));
        // centaurs + KBOs (now mapped, both schemes)
        assert_eq!(m(20_005_145), Some(jzod::BodyId::Pholus));
        assert_eq!(m(20_010_199), Some(jzod::BodyId::Chariklo));
        assert_eq!(m(2_028_978), Some(jzod::BodyId::Ixion));
        assert_eq!(m(20_020_000), Some(jzod::BodyId::Varuna));
        // not a minor body
        assert_eq!(m(399), None);
    }

    /// Every minor body in the placements catalog must map to a JZOD `BodyId`,
    /// so no computed body is silently dropped from JZOD (the default output).
    #[test]
    fn every_catalog_minor_body_maps_to_jzod() {
        for p in crate::placements::CATALOG {
            if let Some(id) = p.sb441_naif_id() {
                assert!(
                    super::asteroid_naif_to_jzod_body_id(id).is_some(),
                    "catalog body {} (mpc {:?}) has no jzod::BodyId mapping",
                    p.name,
                    p.mpc_number
                );
            }
        }
    }

    #[test]
    fn body_maps_to_jzod_id() {
        assert_eq!(
            jzod::BodyId::from(crate::body::Body::Sun),
            jzod::BodyId::Sun
        );
        assert_eq!(
            jzod::BodyId::from(crate::body::Body::Pluto),
            jzod::BodyId::Pluto
        );
    }

    #[test]
    fn house_for_places_longitude_in_first_house_at_cusp() {
        // A longitude exactly on cusp 1 lands in house 1.
        use crate::houses::{HouseCusps, HouseSystem};
        let ac = 125.33_f64.to_radians();
        let cusps: HouseCusps = HouseSystem::WholeSign.compute(0.5, 0.4, ac, 0.6).unwrap();
        let h1_deg = cusps.cusp(1).to_degrees().rem_euclid(360.0);
        assert_eq!(super::house_for(h1_deg + 0.01, &cusps), 1);
    }

    /// Verify all 5 main-belt asteroids (Ceres, Pallas, Juno, Vesta, Hygiea)
    /// appear in `jzod::Chart::placements.bodies` with nonzero `daily_speed`.
    ///
    /// NAIFs requested: `2_000_001` (Ceres), `2_000_002` (Pallas), `2_000_003` (Juno),
    /// `2_000_004` (Vesta), `2_000_010` (Hygiea).  All five are carried by
    /// `sb441-n373.bsp`; Hygiea is absent from `sb441-n16.bsp`.
    ///
    /// Skips cleanly when `$STARCAT_JPL_DATA` or the small-body BSP is absent.
    #[test]
    #[allow(clippy::too_many_lines)]
    fn hygiea_in_jzod_bodies_with_nonzero_speed() {
        use crate::chart::{ChartRequest, ModeRequest, compute_with_spk};
        use crate::jpl::{discover, header::parse, reader::EphemerisFile};
        use crate::spk::SpkEphemeris;
        use crate::time::calendar::{Calendar, CivilDate};
        use crate::time::zone::Zone;
        use std::path::{Path, PathBuf};

        // Skip when JPL data is absent.
        let Ok(data_var) = std::env::var("STARCAT_JPL_DATA") else {
            eprintln!(
                "STARCAT_JPL_DATA not set — skipping hygiea_in_jzod_bodies_with_nonzero_speed"
            );
            return;
        };
        let data_dir = PathBuf::from(&data_var);

        // Locate DE441.
        let Ok(loc) = discover::locate(&data_dir) else {
            eprintln!("DE441 locate failed — skipping");
            return;
        };
        let discover::DatasetLocation::Binary(paths) = loc else {
            eprintln!("DE441 binary not found — skipping");
            return;
        };
        let Ok(source) = std::fs::read_to_string(&paths.header) else {
            eprintln!("DE441 header unreadable — skipping");
            return;
        };
        let Ok(header) = parse(&source) else {
            eprintln!("DE441 header parse failed — skipping");
            return;
        };
        let Ok(file) = EphemerisFile::open(&paths.binary, &header) else {
            eprintln!("DE441 binary open failed — skipping");
            return;
        };
        let ephem = crate::ephemeris::Ephemeris::new(&file, &header).expect("build Ephemeris");

        // Locate sb441-n373.bsp (Hygiea is not in n16).
        let mut bsp_path: Option<PathBuf> = None;
        let mut candidate: &Path = data_dir.as_path();
        for _ in 0..10 {
            let p = candidate
                .join("ftp")
                .join("eph")
                .join("small_bodies")
                .join("asteroids_de441")
                .join("sb441-n373.bsp");
            if p.is_file() {
                bsp_path = Some(p);
                break;
            }
            if let Some(parent) = candidate.parent() {
                candidate = parent;
            } else {
                break;
            }
        }
        let Some(bsp_path) = bsp_path else {
            eprintln!("sb441-n373.bsp not present — skipping");
            return;
        };
        let Ok(spk) = SpkEphemeris::open(&bsp_path) else {
            eprintln!("sb441-n373.bsp open failed — skipping");
            return;
        };

        let req = ChartRequest {
            civil: CivilDate {
                year: 2023,
                month: 2,
                day: 25,
                hour: 12,
                minute: 0,
                second: 0.0,
            },
            calendar: Calendar::Gregorian,
            zone: Zone::FixedSeconds(0),
            mode: ModeRequest::Geocentric,
            lat_deg: None,
            lon_deg: None,
            bodies: None,
            houses: Vec::new(),
            asteroids: vec![2_000_001, 2_000_002, 2_000_003, 2_000_004, 2_000_010],
        };
        let computed = compute_with_spk(&ephem, &[&spk], &req).expect("compute_with_spk");

        let birth = super::ChartBirth {
            year: 2023,
            month: 2,
            day: 25,
            hour: 12,
            minute: 0,
            second: 0,
            lat: None,
            lon: None,
        };
        let chart = super::to_jzod_chart(&computed, &birth, "test-uid".to_string());

        // All 5 asteroids must appear in the JZOD placements with nonzero daily_speed.
        let expected: &[(jzod::BodyId, &str)] = &[
            (jzod::BodyId::Ceres, "Ceres"),
            (jzod::BodyId::Pallas, "Pallas"),
            (jzod::BodyId::Juno, "Juno"),
            (jzod::BodyId::Vesta, "Vesta"),
            (jzod::BodyId::Hygiea, "Hygiea"),
        ];
        for (body_id, name) in expected {
            let body = chart
                .placements
                .bodies
                .iter()
                .find(|b| b.id == *body_id)
                .unwrap_or_else(|| panic!("{name} must be present in jzod bodies"));
            assert!(
                body.daily_speed.0.abs() > 1e-6,
                "{name} daily_speed must be nonzero in JZOD output, got {}",
                body.daily_speed.0
            );
            eprintln!(
                "{name} JZOD: id={:?} daily_speed={:.8} retrograde={}",
                body.id, body.daily_speed.0, body.retrograde
            );
        }
    }
}
