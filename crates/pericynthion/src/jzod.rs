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
//! let generator = jzod::Generator {
//!     name: "starcat".to_string(),
//!     version: starcat::STARCAT_VERSION.to_string(),
//!     components: vec![],
//! };
//! let chart = pericynthion::jzod::to_jzod_chart(
//!     &computed, &birth, uid, jzod::Zodiac::Tropical, None, false, generator,
//! )?;
//! println!("{}", jzod::to_string_pretty(&jzod::JzodDocument::new(vec![chart])));
//! ```

use crate::body::Body;
use crate::chart::ComputedChart;
use crate::coords::phase::LunarPhaseName as P;
use crate::error::PericynthionError;
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
        8_405 => jzod::BodyId::Asbolus,
        28_978 => jzod::BodyId::Ixion,
        20_000 => jzod::BodyId::Varuna,
        15_760 => jzod::BodyId::Albion,
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

/// Validate an ayanamsha slug and resolve its effective frame.
///
/// `ayanamsha` defaults to [`crate::sidereal::DEFAULT_AYANAMSHA_SLUG`] when
/// `None`. `frame` overrides the ayanamsha's intrinsic
/// [`crate::sidereal::Ayanamsha::default_frame`] when `Some`. This is the single
/// "validate slug / default frame" step shared by [`resolve_zodiac`] and
/// [`to_jzod_chart`].
///
/// # Errors
///
/// Returns [`PericynthionError::UnknownAyanamshaSlug`] when the slug is not in
/// [`crate::sidereal::AyanamshaRegistry::with_builtins`].
fn resolve_ayanamsha(
    ayanamsha: Option<&str>,
    frame: Option<jzod::SiderealFrame>,
) -> Result<(crate::sidereal::Ayanamsha, crate::sidereal::AyanamshaFrame), PericynthionError> {
    let slug = ayanamsha.unwrap_or(crate::sidereal::DEFAULT_AYANAMSHA_SLUG);
    let registry = crate::sidereal::AyanamshaRegistry::with_builtins();
    if let Some(a) = registry.get(slug).copied() {
        let resolved_frame = match frame {
            Some(f) => crate::sidereal::AyanamshaFrame::from(f),
            // frame not specified: use the ayanamsha's intrinsic default_frame
            None => a.default_frame,
        };
        Ok((a, resolved_frame))
    } else {
        let known = registry.slugs().join(", ");
        Err(PericynthionError::UnknownAyanamshaSlug {
            slug: slug.to_string(),
            known,
        })
    }
}

/// A caller-facing zodiac selection, before slug validation and frame
/// defaulting. Mirrors the CLI's zodiac flags without pulling clap arg enums
/// into the library.
///
/// Sidereal takes precedence over draconic (see [`resolve_zodiac`]).
#[derive(Debug, Clone, Default)]
pub struct ZodiacRequest {
    /// A sidereal zodiac was requested (e.g. `--zodiac sidereal`).
    pub sidereal: bool,
    /// A draconic zodiac was requested (e.g. `--zodiac draconic` or the
    /// `--draconic` convenience flag). Ignored when `sidereal` is set.
    pub draconic: bool,
    /// Ayanamsha slug for a sidereal request; `None` selects the built-in
    /// default ([`crate::sidereal::DEFAULT_AYANAMSHA_SLUG`]).
    pub ayanamsha: Option<String>,
    /// Frame override for a sidereal request; `None` uses the ayanamsha's
    /// intrinsic default frame.
    pub frame: Option<jzod::SiderealFrame>,
    /// Node metadata recorded on a draconic zodiac's `node` field. Pure
    /// metadata — the actual draconic projection is steered separately by the
    /// `draconic_node` longitude passed to [`to_jzod_chart`].
    pub draconic_node: Option<jzod::DraconicNode>,
}

/// Resolve a [`ZodiacRequest`] into the concrete [`jzod::Zodiac`] to emit and,
/// for a sidereal request, the resolved ayanamsha companion needed to rotate
/// longitudes (e.g. via [`crate::sidereal::project_chart`]).
///
/// Precedence: a sidereal request wins over draconic, which wins over tropical.
/// For sidereal the slug is validated and the frame defaulted once (via the
/// shared internal resolver), so this rule lives in exactly one place. The
/// returned companion is `Some` only for sidereal; `None` for tropical/draconic.
///
/// # Errors
///
/// Returns [`PericynthionError::UnknownAyanamshaSlug`] when a sidereal request
/// names an ayanamsha slug not in the built-in registry.
#[allow(clippy::type_complexity)]
pub fn resolve_zodiac(
    request: &ZodiacRequest,
) -> Result<
    (
        jzod::Zodiac,
        Option<(crate::sidereal::Ayanamsha, crate::sidereal::AyanamshaFrame)>,
    ),
    PericynthionError,
> {
    if request.sidereal {
        let slug = request
            .ayanamsha
            .as_deref()
            .unwrap_or(crate::sidereal::DEFAULT_AYANAMSHA_SLUG);
        let (ay, frame) = resolve_ayanamsha(Some(slug), request.frame)?;
        let zodiac = jzod::Zodiac::Sidereal {
            ayanamsha: Some(slug.to_string()),
            frame: Some(jzod::SiderealFrame::from(frame)),
        };
        return Ok((zodiac, Some((ay, frame))));
    }
    if request.draconic {
        return Ok((
            jzod::Zodiac::Draconic {
                node: request.draconic_node,
            },
            None,
        ));
    }
    Ok((jzod::Zodiac::Tropical, None))
}

/// Assemble a [`jzod::Chart`] from a [`ComputedChart`] and birth metadata.
///
/// # Parameters
///
/// - `computed` — the full numeric result from [`crate::chart::compute`].
/// - `birth` — civil date/time and location fields that accompany the chart.
/// - `uid` — a pre-generated unique identifier (e.g. `uuid::Uuid::new_v4().to_string()`).
///   Passed in by the caller so that `pericynthion` does not depend on `uuid`.
/// - `zodiac` — the zodiac frame to emit, and the authoritative value of
///   `chart.zodiac`. It also selects how each placement longitude is projected:
///   - [`jzod::Zodiac::Tropical`] — longitudes emitted unchanged.
///   - [`jzod::Zodiac::Draconic`] — longitudes projected via
///     [`crate::draconic::draconic_longitude`] using `draconic_node` (see below).
///   - [`jzod::Zodiac::Sidereal`] — longitudes rotated via
///     [`crate::sidereal::sidereal_longitude`] using the ayanamsha named in the
///     variant (defaulting to [`crate::sidereal::DEFAULT_AYANAMSHA_SLUG`]),
///     resolved against [`crate::sidereal::AyanamshaRegistry::with_builtins`].
///     An unrecognized slug returns
///     [`crate::error::PericynthionError::UnknownAyanamshaSlug`].
/// - `draconic_node` — the North-Node longitude consumed only when `zodiac` is
///   [`jzod::Zodiac::Draconic`]. `None` in Draconic mode is a hard error
///   ([`crate::error::PericynthionError::DraconicNodeUnavailable`]); ignored
///   for every other zodiac.
/// - `emit_antiscia` — when `true`, the `antiscion` and `contra_antiscion`
///   optional fields on [`jzod::placement::Body`] and [`jzod::placement::Angle`]
///   are populated from [`crate::antiscia::antiscion`] /
///   [`crate::antiscia::contra_antiscion`] applied to the *emitted* longitude
///   (after any draconic or sidereal projection). When `false`, both fields are `None`.
/// - `generator` — identifies the producing tool (name/version/components);
///   set verbatim on the returned `jzod::Chart::generator`.
///
/// # Mapping notes
///
/// - **Zodiac**: emitted verbatim from the `zodiac` argument.
/// - **Bodies**: speed and retrograde are read from `computed.bodies[i]`; no
///   recomputation is performed.
/// - **Nodes / Lilith**: both mean and true variants are read from
///   `computed.nodes` / `computed.lilith` respectively (no ephemeris call).
/// - **Vertex / Anti-Vertex**: read from `computed.angles`.
/// - **Houses**: `WholeSign` cusps use [`jzod::HouseCusp::whole_sign_from_longitude`];
///   all other systems use [`jzod::HouseCusp::from_longitude`].
/// - **`calculated_at`**: current wall-clock time via
///   [`jzod::time::calculated_at_now`].
///
/// # Errors
///
/// Returns [`PericynthionError::UnknownAyanamshaSlug`] when `zodiac` is
/// [`jzod::Zodiac::Sidereal`] and the ayanamsha slug it names is not present
/// in [`crate::sidereal::AyanamshaRegistry::with_builtins`].
///
/// Returns [`PericynthionError::DraconicNodeUnavailable`] when `zodiac` is
/// [`jzod::Zodiac::Draconic`] and `draconic_node` is `None` — emitting
/// tropical longitudes stamped draconic would silently mislabel the chart.
#[allow(clippy::too_many_lines)]
pub fn to_jzod_chart(
    computed: &ComputedChart,
    birth: &ChartBirth,
    uid: String,
    zodiac: jzod::Zodiac,
    draconic_node: Option<f64>,
    emit_antiscia: bool,
    generator: jzod::Generator,
) -> Result<jzod::Chart, PericynthionError> {
    // ── Coordinate system ────────────────────────────────────────────────────
    let coord_system = match &computed.mode {
        crate::chart::CoordMode::Geocentric => jzod::CoordinateSystem::Geocentric,
        crate::chart::CoordMode::Topocentric(_) => jzod::CoordinateSystem::Topocentric,
        crate::chart::CoordMode::Heliocentric => jzod::CoordinateSystem::Heliocentric,
    };

    // ── Sidereal ayanamsha resolution ────────────────────────────────────────
    // Resolved once up front when a sidereal zodiac is requested. An unknown
    // slug is a hard error — the chart would be mislabeled if projection
    // silently fell back to tropical.
    // The frame (Mean/True) is captured alongside the ayanamsha so that the
    // projection closure can pass it to `sidereal_longitude`.
    let sidereal: Option<(crate::sidereal::Ayanamsha, crate::sidereal::AyanamshaFrame)> =
        match &zodiac {
            jzod::Zodiac::Sidereal { ayanamsha, frame } => {
                Some(resolve_ayanamsha(ayanamsha.as_deref(), *frame)?)
            }
            _ => None,
        };

    // ── Draconic node guard ──────────────────────────────────────────────────
    // A draconic chart without a node longitude would emit tropical longitudes
    // stamped draconic — the same silent-mislabel class as the unknown-ayanamsha
    // case above. Fail fast before any chart is built.
    if matches!(zodiac, jzod::Zodiac::Draconic { .. }) && draconic_node.is_none() {
        return Err(PericynthionError::DraconicNodeUnavailable);
    }

    // ── Longitude projection ─────────────────────────────────────────────────
    // Every emitted longitude is shifted through the projection selected by the
    // chosen zodiac before conversion to a zodiacal Position. The decision is
    // driven by the resolved companions (so the closure does not borrow
    // `zodiac`, leaving it free to move into the emitted chart): a resolved
    // sidereal ayanamsha takes precedence, then a draconic node, else identity.
    let project_lon = |tropical_lon: f64| -> f64 {
        if let Some((a, frame)) = &sidereal {
            crate::sidereal::sidereal_longitude(tropical_lon, computed.jd_tt, a, *frame)
        } else if let Some(node) = draconic_node {
            crate::draconic::draconic_longitude(tropical_lon, node)
        } else {
            tropical_lon
        }
    };

    // ── Antiscia helpers ─────────────────────────────────────────────────────
    // Build antiscion/contra_antiscion Positions from an *emitted* longitude.
    let make_antiscia =
        |emitted_lon: f64| -> (Option<jzod::coord::Position>, Option<jzod::coord::Position>) {
            if emit_antiscia {
                (
                    Some(jzod::coord::Position::from_longitude(
                        crate::antiscia::antiscion(emitted_lon),
                    )),
                    Some(jzod::coord::Position::from_longitude(
                        crate::antiscia::contra_antiscion(emitted_lon),
                    )),
                )
            } else {
                (None, None)
            }
        };

    // ── Sect ─────────────────────────────────────────────────────────────────
    let jzod_sect = computed.sect.map(|s| match s {
        Sect::Day => jzod::Sect::Diurnal,
        Sect::Night => jzod::Sect::Nocturnal,
    });

    // ── Civil-boundary twilight flag ──────────────────────────────────────────
    // Propagated verbatim from ComputedChart; None when sect was not computed.
    let interp_sect_twilight = computed.interp_sect_twilight;

    // ── House assignment helper ──────────────────────────────────────────────
    // House cusps are always computed in tropical coordinates; house number
    // assignment uses the tropical longitude regardless of draconic projection.
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
        .map(|cb| {
            let emitted_lon = project_lon(cb.position.longitude_deg);
            let (antiscion, contra_antiscion) = make_antiscia(emitted_lon);
            jzod::placement::Body {
                id: jzod::BodyId::from(cb.body),
                position: jzod::coord::Position::from_longitude(emitted_lon),
                ecliptic_latitude: jzod::coord::Degrees8(cb.position.latitude_deg),
                daily_speed: jzod::coord::Degrees8(cb.daily_speed_deg),
                retrograde: cb.retrograde,
                distance_au: Some(cb.position.distance_au),
                house: body_houses(cb.position.longitude_deg),
                antiscion,
                contra_antiscion,
            }
        })
        .collect();
    for ca in &computed.asteroids {
        let Some(id) = asteroid_naif_to_jzod_body_id(ca.naif_id) else {
            continue;
        };
        let emitted_lon = project_lon(ca.position.longitude_deg);
        let (antiscion, contra_antiscion) = make_antiscia(emitted_lon);
        bodies.push(jzod::placement::Body {
            id,
            position: jzod::coord::Position::from_longitude(emitted_lon),
            ecliptic_latitude: jzod::coord::Degrees8(ca.position.latitude_deg),
            daily_speed: jzod::coord::Degrees8(ca.daily_speed_deg),
            retrograde: ca.retrograde,
            distance_au: Some(ca.position.distance_au),
            house: body_houses(ca.position.longitude_deg),
            antiscion,
            contra_antiscion,
        });
    }

    // ── Angles array (Ac, Ds, Mc, Ic — in that order when present) ──────────
    let mut angles_vec: Vec<jzod::Angle> = Vec::new();
    if let Some(a) = &computed.angles {
        if let Some(ac) = a.ac_deg {
            let emitted_lon = project_lon(ac);
            let (antiscion, contra_antiscion) = make_antiscia(emitted_lon);
            angles_vec.push(jzod::Angle {
                id: jzod::AngleId::Ascendant,
                position: jzod::coord::Position::from_longitude(emitted_lon),
                antiscion,
                contra_antiscion,
            });
        }
        if let Some(ds) = a.ds_deg {
            let emitted_lon = project_lon(ds);
            let (antiscion, contra_antiscion) = make_antiscia(emitted_lon);
            angles_vec.push(jzod::Angle {
                id: jzod::AngleId::Descendant,
                position: jzod::coord::Position::from_longitude(emitted_lon),
                antiscion,
                contra_antiscion,
            });
        }
        {
            let emitted_lon = project_lon(a.mc_deg);
            let (antiscion, contra_antiscion) = make_antiscia(emitted_lon);
            angles_vec.push(jzod::Angle {
                id: jzod::AngleId::Midheaven,
                position: jzod::coord::Position::from_longitude(emitted_lon),
                antiscion,
                contra_antiscion,
            });
        }
        {
            let emitted_lon = project_lon(a.ic_deg);
            let (antiscion, contra_antiscion) = make_antiscia(emitted_lon);
            angles_vec.push(jzod::Angle {
                id: jzod::AngleId::ImumCoeli,
                position: jzod::coord::Position::from_longitude(emitted_lon),
                antiscion,
                contra_antiscion,
            });
        }
    }

    // ── Points array: Vertex/Anti-Vertex, then Nodes, then Lilith ───────────
    // Suffixed PointIds resolve JZOD OQ-19 (mean/true both present).
    let mut points_vec: Vec<jzod::Point> = Vec::new();

    // Vertex axis — from angles.
    if let Some(a) = &computed.angles {
        if let Some(vx) = a.vx_deg {
            points_vec.push(jzod::Point {
                id: jzod::PointId::Vertex,
                position: jzod::coord::Position::from_longitude(project_lon(vx)),
                retrograde: false,
            });
        }
        if let Some(ax) = a.ax_deg {
            points_vec.push(jzod::Point {
                id: jzod::PointId::AntiVertex,
                position: jzod::coord::Position::from_longitude(project_lon(ax)),
                retrograde: false,
            });
        }
    }

    // Nodes — both mean and true, read from computed.nodes.
    if let Some(n) = &computed.nodes {
        points_vec.push(jzod::Point {
            id: jzod::PointId::NorthNodeMean,
            position: jzod::coord::Position::from_longitude(project_lon(n.mean_nn_deg)),
            // Mean node is always retrograde by construction — see the astronomical
            // fact carried as data on the node type.
            retrograde: crate::chart::NodePoints::MEAN_RETROGRADE,
        });
        points_vec.push(jzod::Point {
            id: jzod::PointId::SouthNodeMean,
            position: jzod::coord::Position::from_longitude(project_lon(n.mean_sn_deg)),
            retrograde: crate::chart::NodePoints::MEAN_RETROGRADE,
        });
        points_vec.push(jzod::Point {
            id: jzod::PointId::NorthNodeTrue,
            position: jzod::coord::Position::from_longitude(project_lon(n.true_nn_deg)),
            retrograde: n.true_retrograde,
        });
        points_vec.push(jzod::Point {
            id: jzod::PointId::SouthNodeTrue,
            position: jzod::coord::Position::from_longitude(project_lon(n.true_sn_deg)),
            retrograde: n.true_retrograde,
        });
    }

    // Black Moon Lilith — both mean and true, read from computed.lilith.
    if let Some(l) = &computed.lilith {
        points_vec.push(jzod::Point {
            id: jzod::PointId::BlackMoonLilithMean,
            position: jzod::coord::Position::from_longitude(project_lon(l.mean_lilith_deg)),
            // Mean Lilith is always prograde — see the astronomical fact carried
            // as data on the Lilith type.
            retrograde: crate::chart::LilithPoints::MEAN_RETROGRADE,
        });
        points_vec.push(jzod::Point {
            id: jzod::PointId::PriapusMean,
            position: jzod::coord::Position::from_longitude(project_lon(l.mean_priapus_deg)),
            retrograde: crate::chart::LilithPoints::MEAN_RETROGRADE,
        });
        points_vec.push(jzod::Point {
            id: jzod::PointId::BlackMoonLilithTrue,
            position: jzod::coord::Position::from_longitude(project_lon(l.true_lilith_deg)),
            retrograde: l.true_retrograde,
        });
        points_vec.push(jzod::Point {
            id: jzod::PointId::PriapusTrue,
            position: jzod::coord::Position::from_longitude(project_lon(l.true_priapus_deg)),
            retrograde: l.true_retrograde,
        });
    }

    // ── Lots array ───────────────────────────────────────────────────────────
    let mut lots_vec: Vec<jzod::Lot> = Vec::new();
    if let Some(l) = &computed.lots {
        lots_vec.push(jzod::Lot {
            id: jzod::LotId::LotOfFortune,
            position: jzod::coord::Position::from_longitude(project_lon(l.fortune_deg)),
        });
        lots_vec.push(jzod::Lot {
            id: jzod::LotId::LotOfSpirit,
            position: jzod::coord::Position::from_longitude(project_lon(l.spirit_deg)),
        });
        lots_vec.push(jzod::Lot {
            id: jzod::LotId::LotOfExaltation,
            position: jzod::coord::Position::from_longitude(project_lon(l.exaltation_deg)),
        });
        if let Some(d) = l.necessity_deg {
            lots_vec.push(jzod::Lot {
                id: jzod::LotId::LotOfNecessity,
                position: jzod::coord::Position::from_longitude(project_lon(d)),
            });
        }
        if let Some(d) = l.eros_deg {
            lots_vec.push(jzod::Lot {
                id: jzod::LotId::LotOfEros,
                position: jzod::coord::Position::from_longitude(project_lon(d)),
            });
        }
        if let Some(d) = l.courage_deg {
            lots_vec.push(jzod::Lot {
                id: jzod::LotId::LotOfCourage,
                position: jzod::coord::Position::from_longitude(project_lon(d)),
            });
        }
        if let Some(d) = l.victory_deg {
            lots_vec.push(jzod::Lot {
                id: jzod::LotId::LotOfVictory,
                position: jzod::coord::Position::from_longitude(project_lon(d)),
            });
        }
        if let Some(d) = l.nemesis_deg {
            lots_vec.push(jzod::Lot {
                id: jzod::LotId::LotOfNemesis,
                position: jzod::coord::Position::from_longitude(project_lon(d)),
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

    // ── Tithi ────────────────────────────────────────────────────────────────
    let tithi = computed.tithi.as_ref().map(|t| jzod::Tithi {
        index: t.index,
        name: t.name.to_string(),
        fraction: t.fraction,
    });

    // ── Ephemeris sources, folded from observed provenance ──────────────────
    // The first-seen-wins-per-key fold lives in `provenance::observed_sources`,
    // shared with the CLI's human "data sources" renderer.
    let mut sources = BTreeMap::new();
    for row in crate::provenance::observed_sources(&computed.provenance) {
        sources.insert(
            row.key,
            jzod::DataSource {
                urls: row.urls,
                cached: row.cached,
            },
        );
    }

    // ── Assemble the chart ───────────────────────────────────────────────────
    Ok(jzod::Chart {
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
        zodiac,
        coordinate_system: coord_system,
        sect: jzod_sect,
        interp_sect_twilight,
        generator,
        ephemeris: Some(jzod::Ephemeris {
            sources,
            calculated_at: jzod::time::calculated_at_now(),
            jd_ut: Some(computed.jd_ut),
            jd_tt: Some(computed.jd_tt),
        }),
        placements: jzod::Placements {
            bodies,
            angles: angles_vec,
            points: points_vec,
            lots: lots_vec,
        },
        houses: jzod_houses,
        lunar_phase,
        tithi,
        nested: vec![],
    })
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(all(test, feature = "jzod"))]
mod jzod_tests {
    /// A minimal `Generator` for tests that don't specifically exercise
    /// generator content.
    fn test_generator() -> jzod::Generator {
        jzod::Generator {
            name: "starcat".to_string(),
            version: "0.0.0-test".to_string(),
            components: vec![],
        }
    }

    /// `ComputedChart::tithi` is propagated verbatim (index + name + fraction)
    /// to `jzod::Chart::tithi` by `to_jzod_chart`.
    #[test]
    fn tithi_maps_to_jzod_chart() {
        use crate::chart::{ComputedChart, CoordMode};
        use crate::coords::tithi::Tithi;

        let mut computed = ComputedChart {
            jd_ut: 2_451_545.0,
            jd_tt: 2_451_545.0,
            mode: CoordMode::Geocentric,
            utc_offset: "+00:00".to_string(),
            bodies: vec![],
            asteroids: vec![],
            angles: None,
            nodes: None,
            lilith: None,
            lots: None,
            houses: vec![],
            lunar_phase: None,
            tithi: None,
            sect: None,
            interp_sect_twilight: None,
            stars: vec![],
            provenance: vec![],
        };

        // Set tithi directly — moon=12°, sun=0° → index 2 (Dwitiya).
        computed.tithi = Some(Tithi {
            index: 2,
            name: "Dwitiya",
            fraction: 0.4,
        });

        let birth = super::ChartBirth {
            year: 2000,
            month: 1,
            day: 1,
            hour: 0,
            minute: 0,
            second: 0,
            lat: None,
            lon: None,
        };
        let chart = super::to_jzod_chart(
            &computed,
            &birth,
            "test-uid".to_string(),
            jzod::Zodiac::Tropical,
            None,
            false,
            test_generator(),
        )
        .unwrap();
        let t = chart
            .tithi
            .expect("tithi must be Some when computed.tithi is set");
        assert_eq!(t.index, 2);
        assert_eq!(t.name, "Dwitiya");
        assert!((t.fraction - 0.4).abs() < 1e-9);
    }

    /// When `draconic_node` is `Some(node_lon)`, `to_jzod_chart` must:
    ///   - set `chart.zodiac == jzod::Zodiac::Draconic`
    ///   - project every body longitude through `draconic_longitude(lon, node)`
    #[test]
    fn to_jzod_chart_emits_draconic_zodiac() {
        use crate::body::Body;
        use crate::chart::{ComputedBody, ComputedChart, CoordMode};
        use crate::coords::apparent::EclipticPosition;

        let sun_lon = 120.0_f64;
        let node_lon = 47.0_f64;
        let expected_drac = crate::draconic::draconic_longitude(sun_lon, node_lon);

        let computed = ComputedChart {
            jd_ut: 2_451_545.0,
            jd_tt: 2_451_545.0,
            mode: CoordMode::Geocentric,
            utc_offset: "+00:00".to_string(),
            bodies: vec![ComputedBody {
                body: Body::Sun,
                position: EclipticPosition {
                    longitude_deg: sun_lon,
                    latitude_deg: 0.0,
                    distance_au: 1.0,
                },
                daily_speed_deg: 1.0,
                retrograde: false,
            }],
            asteroids: vec![],
            angles: None,
            nodes: None,
            lilith: None,
            lots: None,
            houses: vec![],
            lunar_phase: None,
            tithi: None,
            sect: None,
            interp_sect_twilight: None,
            stars: vec![],
            provenance: vec![],
        };
        let birth = super::ChartBirth {
            year: 2000,
            month: 1,
            day: 1,
            hour: 0,
            minute: 0,
            second: 0,
            lat: None,
            lon: None,
        };
        let chart = super::to_jzod_chart(
            &computed,
            &birth,
            "test-uid".to_string(),
            jzod::Zodiac::Draconic { node: None },
            Some(node_lon),
            false,
            test_generator(),
        )
        .unwrap();

        assert_eq!(
            chart.zodiac,
            jzod::Zodiac::Draconic { node: None },
            "zodiac must be Draconic when node is supplied"
        );
        let sun_body = chart
            .placements
            .bodies
            .iter()
            .find(|b| b.id == jzod::BodyId::Sun)
            .expect("Sun must be in placements");
        assert!(
            (sun_body.position.ecliptic_longitude.0 - expected_drac).abs() < 1e-9,
            "Sun draconic longitude must equal draconic_longitude({sun_lon}, {node_lon}) = {expected_drac}, got {}",
            sun_body.position.ecliptic_longitude.0
        );
    }

    /// Build a single-body (Sun at `sun_lon`) geocentric `ComputedChart` at
    /// J2000 for the zodiac-projection tests below.
    fn sun_only_computed(sun_lon: f64) -> crate::chart::ComputedChart {
        use crate::body::Body;
        use crate::chart::{ComputedBody, ComputedChart, CoordMode};
        use crate::coords::apparent::EclipticPosition;
        ComputedChart {
            jd_ut: 2_451_545.0,
            jd_tt: 2_451_545.0,
            mode: CoordMode::Geocentric,
            utc_offset: "+00:00".to_string(),
            bodies: vec![ComputedBody {
                body: Body::Sun,
                position: EclipticPosition {
                    longitude_deg: sun_lon,
                    latitude_deg: 0.0,
                    distance_au: 1.0,
                },
                daily_speed_deg: 1.0,
                retrograde: false,
            }],
            asteroids: vec![],
            angles: None,
            nodes: None,
            lilith: None,
            lots: None,
            houses: vec![],
            lunar_phase: None,
            tithi: None,
            sect: None,
            interp_sect_twilight: None,
            stars: vec![],
            provenance: vec![],
        }
    }

    fn j2000_birth() -> super::ChartBirth {
        super::ChartBirth {
            year: 2000,
            month: 1,
            day: 1,
            hour: 0,
            minute: 0,
            second: 0,
            lat: None,
            lon: None,
        }
    }

    /// A tropical request emits `Zodiac::Tropical` and leaves body longitudes
    /// exactly as computed.
    #[test]
    fn tropical_zodiac_emitted_unchanged() {
        let sun_lon = 120.0_f64;
        let computed = sun_only_computed(sun_lon);
        let chart = super::to_jzod_chart(
            &computed,
            &j2000_birth(),
            "test-uid".to_string(),
            jzod::Zodiac::Tropical,
            None,
            false,
            test_generator(),
        )
        .unwrap();
        assert_eq!(chart.zodiac, jzod::Zodiac::Tropical);
        let sun = chart
            .placements
            .bodies
            .iter()
            .find(|b| b.id == jzod::BodyId::Sun)
            .expect("Sun must be in placements");
        assert!((sun.position.ecliptic_longitude.0 - sun_lon).abs() < 1e-9);
    }

    /// A sidereal request carries the ayanamsha slug on `chart.zodiac` and
    /// rotates every body longitude through `sidereal_longitude`.
    #[test]
    fn sidereal_zodiac_rotates_bodies() {
        let sun_lon = 120.0_f64;
        let computed = sun_only_computed(sun_lon);
        let registry = crate::sidereal::AyanamshaRegistry::with_builtins();
        let lahiri = registry.get("lahiri").expect("lahiri built-in");
        let expected = crate::sidereal::sidereal_longitude(
            sun_lon,
            computed.jd_tt,
            lahiri,
            crate::sidereal::AyanamshaFrame::Mean,
        );

        let chart = super::to_jzod_chart(
            &computed,
            &j2000_birth(),
            "test-uid".to_string(),
            jzod::Zodiac::Sidereal {
                ayanamsha: Some("lahiri".to_string()),
                frame: Some(jzod::SiderealFrame::Mean),
            },
            None,
            false,
            test_generator(),
        )
        .unwrap();
        assert_eq!(
            chart.zodiac,
            jzod::Zodiac::Sidereal {
                ayanamsha: Some("lahiri".to_string()),
                frame: Some(jzod::SiderealFrame::Mean),
            }
        );
        let sun = chart
            .placements
            .bodies
            .iter()
            .find(|b| b.id == jzod::BodyId::Sun)
            .expect("Sun must be in placements");
        assert!(
            (sun.position.ecliptic_longitude.0 - expected).abs() < 1e-9,
            "Sun sidereal longitude must equal sidereal_longitude({sun_lon}, jd, lahiri) = {expected}, got {}",
            sun.position.ecliptic_longitude.0
        );
    }

    /// A sidereal request with `frame: True` rotates bodies through
    /// `sidereal_longitude(..., AyanamshaFrame::True)` and emits the frame
    /// verbatim on `chart.zodiac`.
    #[test]
    fn sidereal_true_frame_rotates_and_is_emitted() {
        let sun_lon = 120.0_f64;
        let computed = sun_only_computed(sun_lon);
        let registry = crate::sidereal::AyanamshaRegistry::with_builtins();
        let lahiri = registry.get("lahiri").unwrap();
        let expected = crate::sidereal::sidereal_longitude(
            sun_lon,
            computed.jd_tt,
            lahiri,
            crate::sidereal::AyanamshaFrame::True,
        );
        let chart = super::to_jzod_chart(
            &computed,
            &j2000_birth(),
            "test-uid".to_string(),
            jzod::Zodiac::Sidereal {
                ayanamsha: Some("lahiri".to_string()),
                frame: Some(jzod::SiderealFrame::True),
            },
            None,
            false,
            test_generator(),
        )
        .unwrap();
        assert_eq!(
            chart.zodiac,
            jzod::Zodiac::Sidereal {
                ayanamsha: Some("lahiri".to_string()),
                frame: Some(jzod::SiderealFrame::True),
            }
        );
        let sun = chart
            .placements
            .bodies
            .iter()
            .find(|b| b.id == jzod::BodyId::Sun)
            .unwrap();
        assert!((sun.position.ecliptic_longitude.0 - expected).abs() < 1e-9);
    }

    /// When `emit_antiscia` is `true`, each body must carry `antiscion` and
    /// `contra_antiscion` positions matching the antiscia functions.
    #[test]
    fn to_jzod_chart_emits_antiscia_when_requested() {
        use crate::body::Body;
        use crate::chart::{ComputedBody, ComputedChart, CoordMode};
        use crate::coords::apparent::EclipticPosition;

        let sun_lon = 30.0_f64;
        let expected_ant = crate::antiscia::antiscion(sun_lon);
        let expected_con = crate::antiscia::contra_antiscion(sun_lon);

        let computed = ComputedChart {
            jd_ut: 2_451_545.0,
            jd_tt: 2_451_545.0,
            mode: CoordMode::Geocentric,
            utc_offset: "+00:00".to_string(),
            bodies: vec![ComputedBody {
                body: Body::Sun,
                position: EclipticPosition {
                    longitude_deg: sun_lon,
                    latitude_deg: 0.0,
                    distance_au: 1.0,
                },
                daily_speed_deg: 1.0,
                retrograde: false,
            }],
            asteroids: vec![],
            angles: None,
            nodes: None,
            lilith: None,
            lots: None,
            houses: vec![],
            lunar_phase: None,
            tithi: None,
            sect: None,
            interp_sect_twilight: None,
            stars: vec![],
            provenance: vec![],
        };
        let birth = super::ChartBirth {
            year: 2000,
            month: 1,
            day: 1,
            hour: 0,
            minute: 0,
            second: 0,
            lat: None,
            lon: None,
        };
        let chart = super::to_jzod_chart(
            &computed,
            &birth,
            "test-uid".to_string(),
            jzod::Zodiac::Tropical,
            None,
            true,
            test_generator(),
        )
        .unwrap();

        let sun = chart
            .placements
            .bodies
            .iter()
            .find(|b| b.id == jzod::BodyId::Sun)
            .expect("Sun must be in placements");
        let ant = sun
            .antiscion
            .expect("antiscion must be Some when emit_antiscia=true");
        let con = sun
            .contra_antiscion
            .expect("contra_antiscion must be Some when emit_antiscia=true");
        assert!(
            (ant.ecliptic_longitude.0 - expected_ant).abs() < 1e-9,
            "antiscion of {sun_lon}° must be {expected_ant}°, got {}",
            ant.ecliptic_longitude.0
        );
        assert!(
            (con.ecliptic_longitude.0 - expected_con).abs() < 1e-9,
            "contra_antiscion of {sun_lon}° must be {expected_con}°, got {}",
            con.ecliptic_longitude.0
        );
    }

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

    /// When `draconic_node` is `Some(node_lon)`, every Point and Lot longitude
    /// must be projected through `draconic_longitude`, not left as tropical.
    #[test]
    fn to_jzod_chart_draconic_projects_points_and_lots() {
        use crate::chart::{ComputedChart, CoordMode, Lots, NodePoints};
        use crate::lots::Sect;

        let node_lon = 47.0_f64;
        // Tropical values that must be projected.
        let tropical_nn = 80.0_f64;
        let tropical_fortune = 200.0_f64;

        let expected_nn = crate::draconic::draconic_longitude(tropical_nn, node_lon);
        let expected_fortune = crate::draconic::draconic_longitude(tropical_fortune, node_lon);

        let computed = ComputedChart {
            jd_ut: 2_451_545.0,
            jd_tt: 2_451_545.0,
            mode: CoordMode::Geocentric,
            utc_offset: "+00:00".to_string(),
            bodies: vec![],
            asteroids: vec![],
            angles: None,
            nodes: Some(NodePoints {
                mean_nn_deg: tropical_nn,
                mean_sn_deg: (tropical_nn + 180.0).rem_euclid(360.0),
                true_nn_deg: tropical_nn,
                true_sn_deg: (tropical_nn + 180.0).rem_euclid(360.0),
                true_retrograde: true,
            }),
            lilith: None,
            lots: Some(Lots {
                sect: Sect::Day,
                fortune_deg: tropical_fortune,
                spirit_deg: 10.0,
                exaltation_deg: 20.0,
                eros_deg: None,
                necessity_deg: None,
                courage_deg: None,
                victory_deg: None,
                nemesis_deg: None,
            }),
            houses: vec![],
            lunar_phase: None,
            tithi: None,
            sect: None,
            interp_sect_twilight: None,
            stars: vec![],
            provenance: vec![],
        };
        let birth = super::ChartBirth {
            year: 2000,
            month: 1,
            day: 1,
            hour: 0,
            minute: 0,
            second: 0,
            lat: None,
            lon: None,
        };
        let chart = super::to_jzod_chart(
            &computed,
            &birth,
            "test-uid".to_string(),
            jzod::Zodiac::Draconic { node: None },
            Some(node_lon),
            false,
            test_generator(),
        )
        .unwrap();

        // NorthNodeMean must be projected, not tropical.
        let nn = chart
            .placements
            .points
            .iter()
            .find(|p| p.id == jzod::PointId::NorthNodeMean)
            .expect("NorthNodeMean must be in points");
        assert!(
            (nn.position.ecliptic_longitude.0 - expected_nn).abs() < 1e-9,
            "NorthNodeMean draconic longitude must equal {expected_nn}, got {} (tropical was {tropical_nn})",
            nn.position.ecliptic_longitude.0
        );

        // LotOfFortune must be projected, not tropical.
        let fortune = chart
            .placements
            .lots
            .iter()
            .find(|l| l.id == jzod::LotId::LotOfFortune)
            .expect("LotOfFortune must be in lots");
        assert!(
            (fortune.position.ecliptic_longitude.0 - expected_fortune).abs() < 1e-9,
            "LotOfFortune draconic longitude must equal {expected_fortune}, got {} (tropical was {tropical_fortune})",
            fortune.position.ecliptic_longitude.0
        );
    }

    /// `ComputedChart::interp_sect_twilight` is propagated verbatim (Some(true) / Some(false) / None)
    /// to `jzod::Chart::interp_sect_twilight` by `to_jzod_chart`. `sect` mapping is unchanged.
    ///
    /// A twilight chart has `sect: Night` + `interp_sect_twilight: Some(true)` — the Sun is
    /// below the horizon (nocturnal) but near the Asc/Desc grace band.
    #[test]
    fn jzod_chart_carries_interp_sect_twilight() {
        use crate::chart::{ComputedChart, CoordMode};
        use crate::lots::Sect;

        let mut computed = ComputedChart {
            jd_ut: 2_451_545.0,
            jd_tt: 2_451_545.0,
            mode: CoordMode::Geocentric,
            utc_offset: "+00:00".to_string(),
            bodies: vec![],
            asteroids: vec![],
            angles: None,
            nodes: None,
            lilith: None,
            lots: None,
            houses: vec![],
            lunar_phase: None,
            tithi: None,
            sect: Some(Sect::Night),
            interp_sect_twilight: Some(true),
            stars: vec![],
            provenance: vec![],
        };

        let birth = super::ChartBirth {
            year: 2000,
            month: 1,
            day: 1,
            hour: 0,
            minute: 0,
            second: 0,
            lat: None,
            lon: None,
        };

        // Twilight chart: nocturnal sect + twilight flag both pass through.
        let chart = super::to_jzod_chart(
            &computed,
            &birth,
            "test-uid".to_string(),
            jzod::Zodiac::Tropical,
            None,
            false,
            test_generator(),
        )
        .unwrap();
        assert_eq!(chart.sect, Some(jzod::Sect::Nocturnal));
        assert_eq!(chart.interp_sect_twilight, Some(true));

        // Also verify Some(false) and None pass through correctly.
        computed.interp_sect_twilight = Some(false);
        let chart2 = super::to_jzod_chart(
            &computed,
            &birth,
            "test-uid".to_string(),
            jzod::Zodiac::Tropical,
            None,
            false,
            test_generator(),
        )
        .unwrap();
        assert_eq!(chart2.interp_sect_twilight, Some(false));

        computed.interp_sect_twilight = None;
        let chart3 = super::to_jzod_chart(
            &computed,
            &birth,
            "test-uid".to_string(),
            jzod::Zodiac::Tropical,
            None,
            false,
            test_generator(),
        )
        .unwrap();
        assert_eq!(chart3.interp_sect_twilight, None);
    }

    /// `to_jzod_chart` sets `generator` verbatim on the returned chart, and
    /// folds `computed.provenance` through `provenance::urls_for_observed`
    /// into `ephemeris.sources`, keyed by `SourceUse::key`.
    #[test]
    fn to_jzod_chart_emits_generator_and_observed_sources() {
        use crate::chart::{ComputedChart, CoordMode, SourceUse};
        use std::path::PathBuf;

        let computed = ComputedChart {
            jd_ut: 2_451_545.0,
            jd_tt: 2_451_545.0,
            mode: CoordMode::Geocentric,
            utc_offset: "+00:00".to_string(),
            bodies: vec![],
            asteroids: vec![],
            angles: None,
            nodes: None,
            lilith: None,
            lots: None,
            houses: vec![],
            lunar_phase: None,
            tithi: None,
            sect: None,
            interp_sect_twilight: None,
            stars: vec![],
            provenance: vec![
                SourceUse {
                    key: "asteroids".to_string(),
                    path: PathBuf::from("/data/nasa/small_bodies/asteroids_de441/sb441-n16.bsp"),
                },
                SourceUse {
                    key: "fixed_stars".to_string(),
                    path: PathBuf::from("catalog.gz"),
                },
            ],
        };
        let birth = super::ChartBirth {
            year: 2000,
            month: 1,
            day: 1,
            hour: 0,
            minute: 0,
            second: 0,
            lat: None,
            lon: None,
        };
        let generator = jzod::Generator {
            name: "starcat".to_string(),
            version: "9.9.9".to_string(),
            components: vec![],
        };
        let chart = super::to_jzod_chart(
            &computed,
            &birth,
            "test-uid".to_string(),
            jzod::Zodiac::Tropical,
            None,
            false,
            generator,
        )
        .unwrap();

        assert_eq!(chart.generator.name, "starcat");
        assert_eq!(chart.generator.version, "9.9.9");

        let ephemeris = chart.ephemeris.expect("ephemeris must be Some");
        assert_eq!(ephemeris.jd_ut, Some(2_451_545.0));
        assert_eq!(ephemeris.jd_tt, Some(2_451_545.0));

        let asteroids = ephemeris
            .sources
            .get("asteroids")
            .expect("asteroids key must be present");
        assert_eq!(
            asteroids.urls,
            vec![
                "https://ssd.jpl.nasa.gov/ftp/eph/small_bodies/asteroids_de441/sb441-n16.bsp"
                    .to_string()
            ]
        );
        assert_eq!(asteroids.cached.as_deref(), Some("sb441-n16.bsp"));

        let fixed_stars = ephemeris
            .sources
            .get("fixed_stars")
            .expect("fixed_stars key must be present");
        assert_eq!(fixed_stars.urls.len(), 2);
        assert!(fixed_stars.cached.is_none());
    }

    /// An unknown ayanamsha slug must be a hard error, not a silent
    /// fallback to tropical.
    #[test]
    fn unknown_ayanamsha_slug_errors() {
        let computed = sun_only_computed(120.0);
        let result = super::to_jzod_chart(
            &computed,
            &j2000_birth(),
            "test-uid".to_string(),
            jzod::Zodiac::Sidereal {
                ayanamsha: Some("nope".to_string()),
                frame: Some(jzod::SiderealFrame::Mean),
            },
            None,
            false,
            test_generator(),
        );
        let err = result.expect_err("expected error for unknown slug");
        let msg = err.to_string();
        assert!(
            msg.contains("nope"),
            "error message should name the offending slug; got: {msg}"
        );
    }

    /// `Zodiac::Draconic { node: None }` + `draconic_node: None` must be a hard
    /// error — the chart would carry tropical longitudes stamped draconic otherwise.
    /// The error message must mention both "draconic" and "node".
    #[test]
    fn draconic_without_node_longitude_is_hard_error() {
        let computed = sun_only_computed(120.0);
        let result = super::to_jzod_chart(
            &computed,
            &j2000_birth(),
            "test-uid".to_string(),
            jzod::Zodiac::Draconic { node: None },
            None, // no node longitude available
            false,
            test_generator(),
        );
        let err = result.expect_err("expected DraconicNodeUnavailable error");
        let msg = err.to_string();
        assert!(
            msg.contains("draconic"),
            "error message must contain 'draconic'; got: {msg}"
        );
        assert!(
            msg.contains("node"),
            "error message must contain 'node'; got: {msg}"
        );
    }

    /// `Zodiac::Draconic { node: Some(DraconicNode::Mean) }` + `draconic_node: None`
    /// must also be a hard error — the `node` field on the zodiac variant records
    /// which node type the caller *intended* but does not substitute for the missing
    /// longitude.
    #[test]
    fn draconic_with_node_metadata_but_no_longitude_is_hard_error() {
        let computed = sun_only_computed(120.0);
        let result = super::to_jzod_chart(
            &computed,
            &j2000_birth(),
            "test-uid".to_string(),
            jzod::Zodiac::Draconic {
                node: Some(jzod::DraconicNode::Mean),
            },
            None, // still no node longitude
            false,
            test_generator(),
        );
        let err = result.expect_err("expected DraconicNodeUnavailable error");
        let msg = err.to_string();
        assert!(
            msg.contains("draconic"),
            "error message must contain 'draconic'; got: {msg}"
        );
        assert!(
            msg.contains("node"),
            "error message must contain 'node'; got: {msg}"
        );
    }

    /// Tropical chart with `draconic_node: None` must remain `Ok` — the guard
    /// must not fire for non-draconic zodiacs.
    #[test]
    fn tropical_with_no_draconic_node_is_ok() {
        let computed = sun_only_computed(120.0);
        let result = super::to_jzod_chart(
            &computed,
            &j2000_birth(),
            "test-uid".to_string(),
            jzod::Zodiac::Tropical,
            None,
            false,
            test_generator(),
        );
        assert!(
            result.is_ok(),
            "tropical zodiac with draconic_node:None must not error"
        );
    }

    /// Sidereal chart with `draconic_node: None` must remain `Ok` — the guard
    /// must not fire for sidereal zodiacs.
    #[test]
    fn sidereal_with_no_draconic_node_is_ok() {
        let computed = sun_only_computed(120.0);
        let result = super::to_jzod_chart(
            &computed,
            &j2000_birth(),
            "test-uid".to_string(),
            jzod::Zodiac::Sidereal {
                ayanamsha: Some("lahiri".to_string()),
                frame: None,
            },
            None,
            false,
            test_generator(),
        );
        assert!(
            result.is_ok(),
            "sidereal zodiac with draconic_node:None must not error"
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
        let computed = compute_with_spk(&ephem, &[&spk], &req, &[]).expect("compute_with_spk");

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
        let chart = super::to_jzod_chart(
            &computed,
            &birth,
            "test-uid".to_string(),
            jzod::Zodiac::Tropical,
            None,
            false,
            test_generator(),
        )
        .unwrap();

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

    /// `frame: None` for lahiri resolves to `True` (its intrinsic default),
    /// so the projected body longitudes must match `frame: Some(True)`.
    #[test]
    fn sidereal_none_frame_resolves_through_ayanamsha_default_lahiri() {
        let sun_lon = 120.0_f64;
        let computed = sun_only_computed(sun_lon);
        let chart_none = super::to_jzod_chart(
            &computed,
            &j2000_birth(),
            "uid-none".to_string(),
            jzod::Zodiac::Sidereal {
                ayanamsha: Some("lahiri".to_string()),
                frame: None,
            },
            None,
            false,
            test_generator(),
        )
        .unwrap();
        let chart_true = super::to_jzod_chart(
            &computed,
            &j2000_birth(),
            "uid-true".to_string(),
            jzod::Zodiac::Sidereal {
                ayanamsha: Some("lahiri".to_string()),
                frame: Some(jzod::SiderealFrame::True),
            },
            None,
            false,
            test_generator(),
        )
        .unwrap();
        let sun_none = chart_none
            .placements
            .bodies
            .iter()
            .find(|b| b.id == jzod::BodyId::Sun)
            .unwrap();
        let sun_true = chart_true
            .placements
            .bodies
            .iter()
            .find(|b| b.id == jzod::BodyId::Sun)
            .unwrap();
        assert!(
            (sun_none.position.ecliptic_longitude.0 - sun_true.position.ecliptic_longitude.0).abs()
                < 1e-12,
            "lahiri frame:None must project identically to frame:Some(True), got None={}, True={}",
            sun_none.position.ecliptic_longitude.0,
            sun_true.position.ecliptic_longitude.0
        );
    }

    /// `frame: None` for `fagan_bradley` resolves to `Mean` (its intrinsic default).
    #[test]
    fn sidereal_none_frame_resolves_through_ayanamsha_default_fagan_bradley() {
        let sun_lon = 120.0_f64;
        let computed = sun_only_computed(sun_lon);
        let chart_none = super::to_jzod_chart(
            &computed,
            &j2000_birth(),
            "uid-none".to_string(),
            jzod::Zodiac::Sidereal {
                ayanamsha: Some("fagan_bradley".to_string()),
                frame: None,
            },
            None,
            false,
            test_generator(),
        )
        .unwrap();
        let chart_mean = super::to_jzod_chart(
            &computed,
            &j2000_birth(),
            "uid-mean".to_string(),
            jzod::Zodiac::Sidereal {
                ayanamsha: Some("fagan_bradley".to_string()),
                frame: Some(jzod::SiderealFrame::Mean),
            },
            None,
            false,
            test_generator(),
        )
        .unwrap();
        let sun_none = chart_none
            .placements
            .bodies
            .iter()
            .find(|b| b.id == jzod::BodyId::Sun)
            .unwrap();
        let sun_mean = chart_mean
            .placements
            .bodies
            .iter()
            .find(|b| b.id == jzod::BodyId::Sun)
            .unwrap();
        assert!(
            (sun_none.position.ecliptic_longitude.0 - sun_mean.position.ecliptic_longitude.0).abs()
                < 1e-12,
            "fagan_bradley frame:None must project identically to frame:Some(Mean), got None={}, Mean={}",
            sun_none.position.ecliptic_longitude.0,
            sun_mean.position.ecliptic_longitude.0
        );
    }
}
