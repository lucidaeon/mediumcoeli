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

    // ── Bodies array ─────────────────────────────────────────────────────────
    let bodies: Vec<jzod::placement::Body> = computed
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
}
