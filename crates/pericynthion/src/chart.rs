//! Chart-domain geometry: angles, nodes, Lilith apogee, and Hermetic lots.
//!
//! This module holds the per-point types and computation functions that belong
//! to the library rather than to any specific CLI or GUI front-end.
//!
//! # Types
//!
//! - [`Angles`] — the four chart axes (Mc/Ic always; Ac/Ds/Vx/Ax when latitude
//!   is known). Geometry only — node and Lilith points are in their own structs.
//! - [`NodePoints`] — mean and true lunar node longitudes plus retrograde flag.
//! - [`LilithPoints`] — mean and true Black Moon Lilith longitudes plus
//!   retrograde flag.
//! - [`Lots`] — Hellenistic sect and the eight Hermetic lots.
//! - [`ComputedBody`] — a body's ecliptic position with daily speed and
//!   retrograde flag.
//! - [`CoordMode`] — which coordinate reference frame to use.
//! - [`ModeRequest`] — caller-facing coordinate-mode selector (no payload).
//! - [`ChartRequest`] — full input specification for [`compute`].
//! - [`ComputedChart`] — full output from [`compute`].
//!
//! # Functions
//!
//! - [`compute_angles`] — computes all chart axes from JD(TT) and location.
//! - [`compute_node_points`] — computes both mean and true lunar nodes.
//! - [`compute_lilith_points`] — computes both mean and true Black Moon Lilith.
//! - [`compute_lots`] — computes all eight Hermetic lots given longitudes.
//! - [`compute`] — full chart orchestration: bodies, angles, nodes, lots, houses.

use crate::body::Body;
use crate::coords::acds::{ac_rad, ds_rad};
use crate::coords::apparent::{
    EclipticPosition, apparent_ecliptic_position, apparent_ecliptic_position_topocentric,
    heliocentric_ecliptic_position,
};
use crate::coords::lilith::{
    mean_lilith_rad, priapus_rad, true_lilith_is_retrograde, true_lilith_rad,
};
use crate::coords::mcic::{ic_rad, mc_rad};
use crate::coords::nodes::{mean_nn_rad, sn_rad, true_nn_is_retrograde, true_nn_rad};
use crate::coords::nutation::nutation;
use crate::coords::obliquity::mean_obliquity_rad;
use crate::coords::phase::LunarPhase;
use crate::coords::sidereal_time::gast_rad;
use crate::coords::topocentric::ObserverLocation;
use crate::coords::vxax::{ax_rad, vx_rad};
use crate::coords::{body_is_retrograde, signed_daily_motion};
use crate::ephemeris::Ephemeris;
use crate::error::PericynthionError;
use crate::houses::{HouseCusps, HouseSystem};
use crate::lots::{
    Sect, courage_rad, eros_rad, exaltation_rad, fortune_rad, necessity_rad, nemesis_rad, sect,
    spirit_rad, victory_rad,
};
use crate::time::calendar::{Calendar, CivilDate};
use crate::time::delta_t::jd_ut_to_jd_tt;
use crate::time::zone::{Zone, civil_to_jd_ut};
use std::f64::consts::TAU;

// =============================================================================
// Types
// =============================================================================

/// Which coordinate reference frame to apply for body positions.
#[derive(Debug, Clone)]
pub enum CoordMode {
    /// Apparent position from Earth's centre.
    Geocentric,
    /// Parallax-corrected position for a surface observer.
    Topocentric(ObserverLocation),
    /// Sun-centred position (no aberration correction).
    Heliocentric,
}

/// Caller-facing coordinate-mode selector (no observer-location payload).
///
/// Use [`CoordMode`] when an [`ObserverLocation`] must accompany the variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModeRequest {
    /// Apparent position from Earth's centre.
    Geocentric,
    /// Parallax-corrected position for a surface observer.
    Topocentric,
    /// Sun-centred position (no aberration correction).
    Heliocentric,
}

/// The four chart axes expressed in tropical ecliptic degrees.
///
/// Mc/Ic are always present (they need only the observer's longitude and JD).
/// Ac/Ds/Vx/Ax are `Some` only when a geographic latitude was supplied, and
/// even then may be `None` at latitudes where they are geometrically undefined
/// (equator, poles, or circumpolar).
///
/// **Geometry only.** Lunar nodes and Black Moon Lilith live in [`NodePoints`]
/// and [`LilithPoints`] respectively; they need the ephemeris and are computed
/// separately.
#[derive(Debug, Clone)]
pub struct Angles {
    /// Medium Coeli (Midheaven) longitude, degrees \[0, 360).
    pub mc_deg: f64,
    /// Imum Coeli longitude, degrees \[0, 360).
    pub ic_deg: f64,
    /// Ascendant longitude, degrees \[0, 360). `None` when latitude is absent.
    pub ac_deg: Option<f64>,
    /// Descendant longitude, degrees \[0, 360). `None` when latitude is absent.
    pub ds_deg: Option<f64>,
    /// Vertex (western prime-vertical / ecliptic intersection), degrees \[0, 360).
    /// Requires latitude; degenerate at equator and poles.
    pub vx_deg: Option<f64>,
    /// Anti-Vertex = Vx + 180°. Same nullability as `vx_deg`.
    pub ax_deg: Option<f64>,
}

/// Both mean and true lunar node longitudes, with retrograde status for the
/// true (osculating) variant.
///
/// The mean node is always retrograde by construction; `true_retrograde`
/// reflects whether the osculating node was retrograde at the chart moment.
#[derive(Debug, Clone)]
pub struct NodePoints {
    /// Mean North Node longitude, degrees \[0, 360).
    pub mean_nn_deg: f64,
    /// Mean South Node longitude, degrees \[0, 360).
    pub mean_sn_deg: f64,
    /// True (osculating) North Node longitude, degrees \[0, 360).
    pub true_nn_deg: f64,
    /// True (osculating) South Node longitude, degrees \[0, 360).
    pub true_sn_deg: f64,
    /// `true` when the true North Node was retrograde at the chart moment.
    pub true_retrograde: bool,
}

/// Both mean and true Black Moon Lilith (lunar apogee) longitudes, with
/// retrograde status for the true (osculating) variant.
///
/// Mean Lilith is always prograde; `true_retrograde` reflects whether the
/// osculating apogee was retrograde at the chart moment.
#[derive(Debug, Clone)]
pub struct LilithPoints {
    /// Mean Lilith longitude, degrees \[0, 360).
    pub mean_lilith_deg: f64,
    /// Mean Priapus (perigee opposite mean Lilith) longitude, degrees \[0, 360).
    pub mean_priapus_deg: f64,
    /// True (osculating) Lilith longitude, degrees \[0, 360).
    pub true_lilith_deg: f64,
    /// True (osculating) Priapus longitude, degrees \[0, 360).
    pub true_priapus_deg: f64,
    /// `true` when the true Lilith was retrograde at the chart moment.
    pub true_retrograde: bool,
}

/// Hellenistic sect and the eight Hermetic lots.
///
/// `fortune_deg`, `spirit_deg`, and `exaltation_deg` are always present
/// (they need only Ac/Sun/Moon). The remaining five lots are `Some` only
/// when the corresponding planet longitude was supplied.
#[derive(Debug, Clone)]
pub struct Lots {
    /// Whether the chart is diurnal (Sun above horizon) or nocturnal.
    pub sect: Sect,
    /// Part of Fortune, degrees \[0, 360).
    pub fortune_deg: f64,
    /// Part of Spirit, degrees \[0, 360).
    pub spirit_deg: f64,
    /// Part of Exaltation, degrees \[0, 360).
    pub exaltation_deg: f64,
    /// Part of Eros. `Some` when Venus longitude was supplied.
    pub eros_deg: Option<f64>,
    /// Part of Necessity. `Some` when Mercury longitude was supplied.
    pub necessity_deg: Option<f64>,
    /// Part of Courage. `Some` when Mars longitude was supplied.
    pub courage_deg: Option<f64>,
    /// Part of Victory. `Some` when Jupiter longitude was supplied.
    pub victory_deg: Option<f64>,
    /// Part of Nemesis. `Some` when Saturn longitude was supplied.
    pub nemesis_deg: Option<f64>,
}

/// A single body's apparent position together with its daily motion and
/// retrograde status.
#[derive(Debug, Clone)]
pub struct ComputedBody {
    /// The body this record describes.
    pub body: Body,
    /// Tropical ecliptic-of-date position.
    pub position: EclipticPosition,
    /// Daily change in ecliptic longitude, degrees per day (signed).
    pub daily_speed_deg: f64,
    /// `true` when the body was retrograde at the chart moment.
    pub retrograde: bool,
}

/// Full input specification for a chart computation.
///
/// Pass a built `ChartRequest` to [`compute`] together with an open
/// [`Ephemeris`] to obtain a [`ComputedChart`].
#[derive(Debug, Clone)]
pub struct ChartRequest {
    /// Civil birth date and time (clock time in the given zone).
    pub civil: CivilDate,
    /// Calendar convention for the civil date.
    pub calendar: Calendar,
    /// Time-zone offset that converts the civil clock to UT.
    pub zone: Zone,
    /// Coordinate reference frame.
    pub mode: ModeRequest,
    /// Observer's geographic latitude, degrees north-positive (ISO 6709).
    /// Required for Topocentric mode and for Ac/Ds/Vx/Ax angles.
    pub lat_deg: Option<f64>,
    /// Observer's geographic longitude, degrees east-positive (ISO 6709).
    /// Required for Mc/Ic angles and for Topocentric mode.
    pub lon_deg: Option<f64>,
    /// Bodies to compute. `None` means all classical bodies (or Earth
    /// replaces Sun in Heliocentric mode).
    pub bodies: Option<Vec<Body>>,
    /// House systems to compute. Empty slice = none. Caller-specified
    /// order is preserved in the output.
    pub houses: Vec<HouseSystem>,
}

/// Complete result of a single chart computation.
///
/// Produced by [`compute`].
#[derive(Debug, Clone)]
pub struct ComputedChart {
    /// Julian Day in Universal Time.
    pub jd_ut: f64,
    /// Julian Day in Terrestrial Time (TT = TDT).
    pub jd_tt: f64,
    /// The coordinate reference frame that was used.
    pub mode: CoordMode,
    /// UTC offset string derived from the input zone, e.g. `"+05:30"`.
    pub utc_offset: String,
    /// Computed positions for each body in the request, in request order.
    pub bodies: Vec<ComputedBody>,
    /// Chart axes. `None` when no longitude was supplied.
    pub angles: Option<Angles>,
    /// Lunar node longitudes. `None` in Heliocentric mode or when no
    /// longitude was supplied (nodes require angles).
    pub nodes: Option<NodePoints>,
    /// Black Moon Lilith longitudes. `None` under the same conditions as
    /// [`nodes`](ComputedChart::nodes).
    pub lilith: Option<LilithPoints>,
    /// Hermetic lots. `None` in Heliocentric mode or when Sun + Moon
    /// positions are unavailable.
    pub lots: Option<Lots>,
    /// House cusps for each requested system.
    /// Each entry is `(system, Some(cusps))` when the geometry is defined,
    /// or `(system, None)` when it degenerates (circumpolar, equator, etc.).
    pub houses: Vec<(HouseSystem, Option<HouseCusps>)>,
    /// Lunar phase. `None` in Heliocentric mode or when Sun/Moon are absent.
    pub lunar_phase: Option<LunarPhase>,
    /// Hellenistic sect (day / night). `None` when Ac or Sun is unavailable.
    pub sect: Option<Sect>,
}

// =============================================================================
// Computation functions
// =============================================================================

/// Format a [`Zone`] as a UTC offset string `±HH:MM`.
///
/// For [`Zone::Lmt`] the offset is derived from `longitude_east / 15`, rounded
/// at the **minute** level (matching starcat's original implementation for
/// byte-for-byte output parity). Rounding at the second level and then
/// truncating to minutes would diverge for sub-minute longitudes.
///
/// For [`Zone::FixedSeconds`] the stored offset is formatted directly.
fn utc_offset_string(zone: Zone) -> String {
    match zone {
        // LMT: derive ±HH:MM from longitude, rounding at the MINUTE level to match
        // starcat's original (parity-critical). Do NOT round at the second level.
        Zone::Lmt { longitude_east } => {
            let h = longitude_east / 15.0;
            let sign = if h >= 0.0 { '+' } else { '-' };
            let abs_h = h.abs();
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let hh = abs_h.floor() as u32;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let mm = ((abs_h - f64::from(hh)) * 60.0).round() as u32;
            format!("{sign}{hh:02}:{mm:02}")
        }
        Zone::FixedSeconds(total_seconds) => {
            let sign = if total_seconds >= 0 { '+' } else { '-' };
            let abs_s = total_seconds.unsigned_abs();
            let hh = abs_s / 3600;
            let mm = (abs_s % 3600) / 60;
            format!("{sign}{hh:02}:{mm:02}")
        }
    }
}

/// Compute a complete natal chart.
///
/// This is the pure-computation counterpart to the `starcat compute` CLI
/// command. It expects an already-open [`Ephemeris`] and an already-parsed
/// [`ChartRequest`]; it does not open files, parse strings, or produce I/O.
///
/// # Errors
///
/// Returns [`PericynthionError`] when the ephemeris cannot provide a body
/// position (I/O error, body out of range for the given JD, etc.).
// `jd_ut` / `jd_tt` are established astronomical abbreviations — suppressing
// similar_names is intentional and domain-appropriate here.
// The orchestration necessarily touches every pipeline step in sequence.
#[allow(clippy::similar_names, clippy::too_many_lines)]
pub fn compute(
    ephem: &Ephemeris<'_>,
    request: &ChartRequest,
) -> Result<ComputedChart, PericynthionError> {
    // ── 1. Time scales ───────────────────────────────────────────────────────
    let jd_ut = civil_to_jd_ut(request.civil, request.calendar, request.zone);
    let jd_tt = jd_ut_to_jd_tt(jd_ut);

    // ── 2. UTC offset string ─────────────────────────────────────────────────
    let utc_offset = utc_offset_string(request.zone);

    // ── 3. Coordinate mode ───────────────────────────────────────────────────
    let mode = match request.mode {
        ModeRequest::Heliocentric => CoordMode::Heliocentric,
        ModeRequest::Topocentric => match (request.lat_deg, request.lon_deg) {
            (Some(lat_deg), Some(lon_deg)) => CoordMode::Topocentric(ObserverLocation {
                lat_deg,
                lon_deg,
                elev_m: 0.0,
            }),
            _ => CoordMode::Geocentric,
        },
        ModeRequest::Geocentric => CoordMode::Geocentric,
    };
    let is_helio = matches!(mode, CoordMode::Heliocentric);

    // ── 4. Body list ─────────────────────────────────────────────────────────
    let body_list: Vec<Body> = match &request.bodies {
        Some(list) => list.clone(),
        None => {
            if is_helio {
                Body::ALL_HELIOCENTRIC.to_vec()
            } else {
                Body::ALL.to_vec()
            }
        }
    };

    // ── 5. Per-body positions + daily speed + retrograde ─────────────────────
    let mut bodies: Vec<ComputedBody> = Vec::with_capacity(body_list.len());
    for body in &body_list {
        let body = *body;
        let pos = match &mode {
            CoordMode::Geocentric => apparent_ecliptic_position(ephem, body, jd_tt)?,
            CoordMode::Topocentric(obs) => {
                apparent_ecliptic_position_topocentric(ephem, body, jd_tt, obs)?
            }
            CoordMode::Heliocentric => heliocentric_ecliptic_position(ephem, body, jd_tt)?,
        };
        let lon_at = |jd: f64| -> f64 {
            match &mode {
                CoordMode::Heliocentric => heliocentric_ecliptic_position(ephem, body, jd)
                    .map_or(pos.longitude_deg, |p| p.longitude_deg),
                _ => apparent_ecliptic_position(ephem, body, jd)
                    .map_or(pos.longitude_deg, |p| p.longitude_deg),
            }
        };
        let daily_speed_deg = signed_daily_motion(lon_at(jd_tt - 0.5), lon_at(jd_tt + 0.5));
        let retrograde = body_is_retrograde(ephem, body, jd_tt, is_helio);
        bodies.push(ComputedBody {
            body,
            position: pos,
            daily_speed_deg,
            retrograde,
        });
    }

    // ── 6. Angles ────────────────────────────────────────────────────────────
    let angles = request
        .lon_deg
        .map(|lon| compute_angles(jd_tt, lon, request.lat_deg));

    // ── 7. Nodes + Lilith (geo/topo only) ────────────────────────────────────
    // Both are functions of the Moon's orbital geometry at the instant: they
    // need no observer latitude and no Ascendant. Computed for any geocentric
    // or topocentric chart; omitted only in heliocentric mode (the node and
    // apogee are defined relative to Earth's orbital plane).
    let (nodes, lilith) = if is_helio {
        (None, None)
    } else {
        let n = compute_node_points(jd_tt, ephem)?;
        let l = compute_lilith_points(jd_tt, ephem)?;
        (Some(n), Some(l))
    };

    // ── 8. Lots (geo/topo only; need Ac + Sun + Moon) ───────────────────────
    let find_lon = |b: Body| {
        bodies
            .iter()
            .find(|cb| cb.body == b)
            .map(|cb| cb.position.longitude_deg)
    };
    let lots = if is_helio {
        None
    } else {
        angles.as_ref().and_then(|a| a.ac_deg).and_then(|ac_deg| {
            let sun = find_lon(Body::Sun)?;
            let moon = find_lon(Body::Moon)?;
            let mercury = find_lon(Body::Mercury);
            let venus = find_lon(Body::Venus);
            let mars = find_lon(Body::Mars);
            let jupiter = find_lon(Body::Jupiter);
            let saturn = find_lon(Body::Saturn);
            Some(compute_lots(
                ac_deg, sun, moon, mercury, venus, mars, jupiter, saturn,
            ))
        })
    };

    // ── 9. Lunar phase (geo/topo; needs Sun + Moon) ──────────────────────────
    let lunar_phase: Option<LunarPhase> = if is_helio {
        None
    } else {
        find_lon(Body::Sun)
            .zip(find_lon(Body::Moon))
            .map(|(sun, moon)| crate::coords::phase::lunar_phase(moon, sun))
    };

    // ── 10. House cusps ──────────────────────────────────────────────────────
    let house_cusps: Vec<(HouseSystem, Option<HouseCusps>)> =
        if is_helio || request.houses.is_empty() {
            request.houses.iter().map(|&h| (h, None)).collect()
        } else {
            match (request.lon_deg, request.lat_deg) {
                (Some(lon), Some(lat)) => {
                    let ramc = (gast_rad(jd_tt) + lon.to_radians()).rem_euclid(TAU);
                    let nut = nutation(jd_tt);
                    let obliquity = mean_obliquity_rad(jd_tt) + nut.delta_epsilon;
                    let lat_rad = lat.to_radians();
                    if let Some(ac) = ac_rad(ramc, obliquity, lat_rad) {
                        request
                            .houses
                            .iter()
                            .map(|&sys| (sys, sys.compute(ramc, obliquity, ac, lat_rad)))
                            .collect()
                    } else {
                        request.houses.iter().map(|&h| (h, None)).collect()
                    }
                }
                _ => request.houses.iter().map(|&h| (h, None)).collect(),
            }
        };

    // ── 11. Sect (needs Ac + Sun) ─────────────────────────────────────────────
    let sect_val: Option<Sect> = angles
        .as_ref()
        .and_then(|a| a.ac_deg)
        .zip(find_lon(Body::Sun))
        .map(|(ac_deg, sun_deg)| sect(sun_deg.to_radians(), ac_deg.to_radians()));

    Ok(ComputedChart {
        jd_ut,
        jd_tt,
        mode,
        utc_offset,
        bodies,
        angles,
        nodes,
        lilith,
        lots,
        houses: house_cusps,
        lunar_phase,
        sect: sect_val,
    })
}

/// Compute the chart axes (Mc/Ic/Ac/Ds/Vx/Ax) from a Julian Day (TT) and
/// an observer's geographic longitude.
///
/// `lon_east_deg` is the observer's longitude in degrees, **east-positive**
/// (ISO 6709 convention). `lat_deg` is the observer's geographic latitude in
/// degrees; pass `None` for a chart with no known location (Mc/Ic only).
///
/// Returns an [`Angles`] where Ac/Ds/Vx/Ax are `None` when `lat_deg` is
/// `None` or when the geometry is undefined at the given latitude.
#[must_use]
pub fn compute_angles(jd_tt: f64, lon_east_deg: f64, lat_deg: Option<f64>) -> Angles {
    let ramc = (gast_rad(jd_tt) + lon_east_deg.to_radians()).rem_euclid(TAU);
    let nut = nutation(jd_tt);
    let obliquity = mean_obliquity_rad(jd_tt) + nut.delta_epsilon;
    let mc = mc_rad(ramc, obliquity);
    let ic = ic_rad(mc);
    let ac = lat_deg.and_then(|lat| ac_rad(ramc, obliquity, lat.to_radians()));
    let ds = ac.map(ds_rad);
    let vx = lat_deg.and_then(|lat| vx_rad(ramc, obliquity, lat.to_radians()));
    let ax = vx.map(ax_rad);
    Angles {
        mc_deg: mc.to_degrees(),
        ic_deg: ic.to_degrees(),
        ac_deg: ac.map(f64::to_degrees),
        ds_deg: ds.map(f64::to_degrees),
        vx_deg: vx.map(f64::to_degrees),
        ax_deg: ax.map(f64::to_degrees),
    }
}

/// Compute both mean and true lunar node longitudes at the given Julian Day
/// (TT).
///
/// Both variants are always computed. The mean node uses the closed-form
/// Meeus polynomial (no ephemeris read). The true node derives from the
/// Moon's osculating orbital plane via the ephemeris; `true_retrograde`
/// indicates whether it was retrograde at `jd_tt`.
///
/// # Errors
///
/// Propagates any I/O or out-of-range error from the underlying ephemeris
/// reads for the true node and its retrograde check.
pub fn compute_node_points(
    jd_tt: f64,
    ephem: &Ephemeris<'_>,
) -> Result<NodePoints, PericynthionError> {
    let m = mean_nn_rad(jd_tt);
    let t = true_nn_rad(ephem, jd_tt)?;
    let t_retro = true_nn_is_retrograde(ephem, jd_tt)?;
    Ok(NodePoints {
        mean_nn_deg: m.to_degrees(),
        mean_sn_deg: sn_rad(m).to_degrees(),
        true_nn_deg: t.to_degrees(),
        true_sn_deg: sn_rad(t).to_degrees(),
        true_retrograde: t_retro,
    })
}

/// Compute both mean and true Black Moon Lilith (lunar apogee) longitudes at
/// the given Julian Day (TT).
///
/// Both variants are always computed. The mean Lilith uses the closed-form
/// Meeus polynomial (no ephemeris read). The true Lilith derives from the
/// Moon's osculating eccentricity vector via the ephemeris; `true_retrograde`
/// indicates whether it was retrograde at `jd_tt`.
///
/// # Errors
///
/// Propagates any I/O or out-of-range error from the underlying ephemeris
/// reads for the true Lilith and its retrograde check.
pub fn compute_lilith_points(
    jd_tt: f64,
    ephem: &Ephemeris<'_>,
) -> Result<LilithPoints, PericynthionError> {
    let m = mean_lilith_rad(jd_tt);
    let t = true_lilith_rad(ephem, jd_tt)?;
    let t_retro = true_lilith_is_retrograde(ephem, jd_tt)?;
    Ok(LilithPoints {
        mean_lilith_deg: m.to_degrees(),
        mean_priapus_deg: priapus_rad(m).to_degrees(),
        true_lilith_deg: t.to_degrees(),
        true_priapus_deg: priapus_rad(t).to_degrees(),
        true_retrograde: t_retro,
    })
}

/// Compute the eight Hermetic lots from the chart's key longitudes (degrees).
///
/// `ac_deg`, `sun_deg`, and `moon_deg` are always required (they drive Fortune,
/// Spirit, and Exaltation). The five remaining lots are `Some` only when the
/// corresponding planet longitude is supplied.
///
/// All outputs are ecliptic longitudes in degrees, `[0, 360)`.
#[allow(clippy::too_many_arguments)]
#[must_use]
pub fn compute_lots(
    ac_deg: f64,
    sun_deg: f64,
    moon_deg: f64,
    mercury_deg: Option<f64>,
    venus_deg: Option<f64>,
    mars_deg: Option<f64>,
    jupiter_deg: Option<f64>,
    saturn_deg: Option<f64>,
) -> Lots {
    let ac = ac_deg.to_radians();
    let sun = sun_deg.to_radians();
    let moon = moon_deg.to_radians();
    let s = sect(sun, ac);
    let deg = |r: f64| r.to_degrees().rem_euclid(360.0);
    let f = deg(fortune_rad(ac, sun, moon, s));
    let sp = deg(spirit_rad(ac, sun, moon, s));
    let ex = deg(exaltation_rad(ac, sun, moon, s));
    let er = venus_deg.map(|v| deg(eros_rad(ac, sun, moon, v.to_radians(), s)));
    let nec = mercury_deg.map(|m| deg(necessity_rad(ac, sun, moon, m.to_radians(), s)));
    let cou = mars_deg.map(|m| deg(courage_rad(ac, sun, moon, m.to_radians(), s)));
    let vic = jupiter_deg.map(|j| deg(victory_rad(ac, sun, moon, j.to_radians(), s)));
    let nem = saturn_deg.map(|sa| deg(nemesis_rad(ac, sun, moon, sa.to_radians(), s)));
    Lots {
        sect: s,
        fortune_deg: f,
        spirit_deg: sp,
        exaltation_deg: ex,
        eros_deg: er,
        necessity_deg: nec,
        courage_deg: cou,
        victory_deg: vic,
        nemesis_deg: nem,
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lots::Sect;

    #[test]
    fn compute_angles_leo_asc_mc() {
        // 1955-11-13 06:04 UT, Universal City CA. Refchart resolved coords:
        // 34°N08'20" = 34.1389° lat, 118°W21'09" = -118.3525° lon.
        // Ar⌖26°07'43" = 26.129° MC, Le⌖05°19'30" = 125.325° Asc.
        use crate::time::delta_t::jd_ut_to_jd_tt;
        let jd_tt = jd_ut_to_jd_tt(2_435_424.752_8);
        let ang = compute_angles(jd_tt, -118.352_500, Some(34.138_889));
        let mc_expected = 0.0 + 26.0 + 7.0 / 60.0 + 43.0 / 3600.0;
        let as_expected = 120.0 + 5.0 + 19.0 / 60.0 + 30.0 / 3600.0;
        assert!(
            (ang.mc_deg - mc_expected).abs() < 5.0 / 60.0,
            "Mc {:.4} expected {:.4}",
            ang.mc_deg,
            mc_expected
        );
        assert!(
            (ang.ac_deg.unwrap() - as_expected).abs() < 5.0 / 60.0,
            "As {:.4} expected {:.4}",
            ang.ac_deg.unwrap(),
            as_expected
        );
    }

    #[test]
    fn compute_angles_no_lat_omits_asc() {
        use crate::time::delta_t::jd_ut_to_jd_tt;
        let jd_tt = jd_ut_to_jd_tt(2_435_424.752_8);
        let ang = compute_angles(jd_tt, -118.352_500, None);
        assert!(ang.ac_deg.is_none());
        let diff = (ang.ic_deg - ang.mc_deg).rem_euclid(360.0);
        assert!((diff - 180.0).abs() < 1e-6, "IC-MC diff {diff:.6}");
    }

    #[test]
    fn compute_angles_dsc_is_asc_plus_180() {
        use crate::time::delta_t::jd_ut_to_jd_tt;
        let jd_tt = jd_ut_to_jd_tt(2_435_424.752_8);
        let ang = compute_angles(jd_tt, -118.352_500, Some(34.138_889));
        let ac = ang.ac_deg.expect("As present with lat");
        let ds = ang.ds_deg.expect("Ds present with lat");
        let diff = (ds - ac).rem_euclid(360.0);
        assert!((diff - 180.0).abs() < 1e-9, "Ds-As diff {diff:.9}");
    }

    #[test]
    fn compute_angles_no_lat_omits_dsc() {
        use crate::time::delta_t::jd_ut_to_jd_tt;
        let jd_tt = jd_ut_to_jd_tt(2_435_424.752_8);
        let ang = compute_angles(jd_tt, -118.352_500, None);
        assert!(ang.ds_deg.is_none());
    }

    #[test]
    fn compute_lots_leo_asc_day_chart() {
        // Adèle Haenel: Sun=322.889° (Aqr), Moon=35.683° (Tau), ASC=124.919° (Leo).
        // Sun above horizon → day chart. refchart PF: Lib⌖17°42'46" = 197.713°.
        // Spirit (Day) = ASC + Sun − Moon. No planets → no Eros/Necessity/Courage/Victory/Nemesis.
        let lots = compute_lots(124.919, 322.889, 35.683, None, None, None, None, None);
        assert_eq!(lots.sect, Sect::Day);
        let expected_pf = 180.0 + 17.0 + 42.0 / 60.0 + 46.0 / 3600.0_f64;
        assert!(
            (lots.fortune_deg - expected_pf).abs() < 5.0 / 60.0,
            "Fortune {:.4} expected {:.4}",
            lots.fortune_deg,
            expected_pf
        );
        let expected_spirit = (124.919 + 322.889 - 35.683_f64).rem_euclid(360.0);
        assert!(
            (lots.spirit_deg - expected_spirit).abs() < 1e-3,
            "Spirit {:.4} expected {:.4}",
            lots.spirit_deg,
            expected_spirit
        );
        // Exaltation always emits — day = ASC + 18° − Sun.
        let expected_exalt = (124.919 + 18.0 - 322.889_f64).rem_euclid(360.0);
        assert!(
            (lots.exaltation_deg - expected_exalt).abs() < 1e-3,
            "Exaltation {:.4} expected {:.4}",
            lots.exaltation_deg,
            expected_exalt
        );
        assert!(lots.eros_deg.is_none(), "Eros absent without Venus");
        assert!(
            lots.necessity_deg.is_none(),
            "Necessity absent without Mercury"
        );
        assert!(lots.courage_deg.is_none(), "Courage absent without Mars");
        assert!(lots.victory_deg.is_none(), "Victory absent without Jupiter");
        assert!(lots.nemesis_deg.is_none(), "Nemesis absent without Saturn");
    }

    #[test]
    fn utc_offset_lmt_rounds_at_minute_level() {
        // 15.4°E → 1.0267 h → +01:02 (minute-level rounding, matching starcat).
        // Second-level rounding would give (15.4/15*3600).round()=3696 s →
        // (3696 % 3600) / 60 = 1 → wrongly yield "+01:01".
        let zone = Zone::Lmt {
            longitude_east: 15.4,
        };
        assert_eq!(utc_offset_string(zone), "+01:02");
    }

    #[test]
    fn compute_lots_lilly_night_chart() {
        // Lilly: Sun=49.987°, Moon=284.760°, ASC=332.110° → night chart.
        let lots = compute_lots(332.110, 49.987, 284.760, None, None, None, None, None);
        assert_eq!(lots.sect, Sect::Night);
        // Night PF: ASC + Sun − Moon = 97.337°
        let expected_pf = (332.110 + 49.987 - 284.760_f64).rem_euclid(360.0);
        assert!(
            (lots.fortune_deg - expected_pf).abs() < 1e-3,
            "PF {:.4} expected {:.4}",
            lots.fortune_deg,
            expected_pf
        );
        // Exaltation night = ASC + 32° − Moon.
        let expected_exalt = (332.110 + 32.0 - 284.760_f64).rem_euclid(360.0);
        assert!(
            (lots.exaltation_deg - expected_exalt).abs() < 1e-3,
            "Exaltation {:.4} expected {:.4}",
            lots.exaltation_deg,
            expected_exalt
        );
    }

    #[test]
    fn compute_lots_emits_eros_when_venus_present() {
        // Leo ASC day chart: Sun=219.601°, Moon=324.291°, ASC=317.671°,
        // Venus=255.325° (DE441 apparent geo from acceptance test output).
        // Day Eros = ASC + Venus − Spirit, Spirit_day = ASC + Sun − Moon = 212.981°.
        // Eros_day = 317.671 + 255.325 − 212.981 = 360.015° → 0.015°.
        let lots = compute_lots(
            317.671,
            219.601,
            324.291,
            None,
            Some(255.325),
            None,
            None,
            None,
        );
        let eros = lots.eros_deg.expect("eros present with venus");
        let expected = (317.671 + 255.325 - 212.981_f64).rem_euclid(360.0);
        assert!(
            (eros - expected).abs() < 1e-3,
            "Eros {eros:.4} expected {expected:.4}"
        );
    }
}
