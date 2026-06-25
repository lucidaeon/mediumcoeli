//! Coordinate-transformation pipeline: JPL barycentric → tropical
//! ecliptic-of-date apparent geocentric.
//!
//! # The pipeline, end-to-end
//!
//! Given a body's barycentric position from the JPL ephemeris at TT
//! Julian Date, what an astrologer wants is the **tropical ecliptic
//! longitude of date, apparent geocentric**. Producing it requires
//! seven distinct transformations stacked in a specific order:
//!
//! ```text
//! body_bary(TT)                              ← input from JPL
//!   │   (light-time iteration, ≈ 3 steps)
//!   ▼
//! body_bary(TT − τ)  where τ = |geo| / c
//!   │
//!   │ subtract earth_bary(TT)               ← geocentric vector
//!   ▼
//! geo_pos_astrometric                       ← still ICRS / mean J2000
//!   │   (annual aberration: V_earth / c)
//!   ▼
//! geo_pos_apparent                          ← still in J2000 frame
//!   │   (precession: J2000 → mean-of-date)
//!   ▼
//! geo_pos_mean_of_date
//!   │   (nutation: mean-of-date → true-of-date)
//!   ▼
//! geo_pos_true_of_date                      ← equatorial frame of date
//!   │   (rotate by true obliquity ε)
//!   ▼
//! geo_pos_ecliptic_of_date
//!   │   (atan2(y, x))
//!   ▼
//! ecliptic_longitude_of_date                ← final astrologer-facing value
//! ```
//!
//! Each step has its own submodule:
//!
//! - [`obliquity`] — IAU 2006 mean obliquity polynomial.
//! - [`precession`] — IAU 2006 (Capitaine et al. 2003) three-angle model
//!   with T⁴/T⁵ terms and the ±2.650545″ frame-bias constant.
//! - [`nutation`] — IAU 2000B 77-term luni-solar series (ERFA-derived,
//!   sub-mas for modern dates).
//! - [`aberration`] — annual aberration from Earth's velocity.
//! - [`light_time`] — iterative correction for finite speed of light.
//! - [`transform`] — generic 3-vector rotation primitives and the
//!   equatorial→ecliptic conversion.
//! - [`apparent`] — the high-level facade that composes the above into
//!   `apparent_ecliptic_position(body, jd_tt)` (and topo/helio variants).
//! - [`acds`], [`mcic`] — Asc/Dsc/MC/IC angles from sidereal time +
//!   true obliquity + geographic latitude.
//! - [`sidereal_time`] — GMST/GAST for the ascendant and topocentric
//!   parallax computations.
//! - [`topocentric`] — WGS84 observer-position subtraction for parallax.
//!
//! # Accuracy budget
//!
//! Target: **sub-arcsecond** for modern (post-1900) planet positions vs
//! NASA HORIZONS; Moon delta ~5–15″ at modern epochs (residual under
//! investigation, not nutation). Precession + nutation accuracy is
//! sub-mas in this era; aberration <0.1″; light-time <0.001″ once the
//! iteration converges.
//!
//! For ancient epochs (Vettius Valens, year 120 CE), the dominant
//! error source is the *choice of ΔT model*, not its internal precision.
//! starcat uses the SMH 2016 spline (~10570 s at year 120); refchart's
//! desktop ephemeris uses a different model (~9340 s at the same epoch).
//! The 1230 s gap propagates to ~74″ in Moon longitude — larger than
//! the precession (~1″) or nutation (sub-mas) residuals combined. See
//! `tests/acceptance_refchart.rs` for the per-chart tolerance accounting.

pub mod aberration;
pub mod acds;
pub mod apparent;
pub mod light_time;
pub mod lilith;
pub mod mcic;
pub mod nodes;
pub mod nutation;
pub mod obliquity;
pub mod phase;
pub mod precession;
pub mod sidereal_time;
pub mod tithi;
pub mod topocentric;
pub mod transform;
pub mod vxax;

use self::apparent::apparent_ecliptic_position;
use crate::body::Body;
use crate::ephemeris::Ephemeris;

/// Signed angular motion (degrees) between two longitudes, wrapping the 0/360
/// seam so that retrograde motion near the seam still reads as negative.
#[must_use]
pub fn signed_daily_motion(lon_before: f64, lon_after: f64) -> f64 {
    let raw = lon_after - lon_before;
    if raw > 180.0 {
        raw - 360.0
    } else if raw < -180.0 {
        raw + 360.0
    } else {
        raw
    }
}

/// Returns `true` when `body` is retrograde at `jd_tt` in geocentric or
/// topocentric mode.  Sun, Moon, and Earth are never marked retrograde.
/// Heliocentric positions are never retrograde.
#[must_use]
pub fn body_is_retrograde(ephem: &Ephemeris, body: Body, jd_tt: f64, heliocentric: bool) -> bool {
    if matches!(body, Body::Sun | Body::Moon | Body::Earth) {
        return false;
    }
    if heliocentric {
        return false;
    }
    let lon_at =
        |jd: f64| apparent_ecliptic_position(ephem, body, jd).map_or(0.0, |p| p.longitude_deg);
    signed_daily_motion(lon_at(jd_tt - 0.5), lon_at(jd_tt + 0.5)) < 0.0
}
