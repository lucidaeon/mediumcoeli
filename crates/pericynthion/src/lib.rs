//! # pericynthion
//!
//! Astrological ephemeris computation library.
//!
//! `pericynthion` reads NASA JPL planetary ephemeris files (DE441 specifically,
//! though the on-disk format extends back to DE200) and produces the
//! astronomical quantity an astrologer cares about most: tropical
//! ecliptic-of-date apparent positions of bodies + supporting chart
//! infrastructure (angles, lots, house cusps).
//!
//! ## Layered architecture
//!
//! Each numerical primitive lives in its own module and is testable in
//! isolation:
//!
//! 1. [`chebyshev`] — the mathematical kernel. Evaluates a series
//!    ∑ cₖ·Tₖ(x) and its derivative on x ∈ \[−1, 1\] via the Clenshaw
//!    recurrence. No I/O, no astronomy, no opinions.
//! 2. [`jpl`] — parses the JPL ASCII header and reads coefficient
//!    records from the binary file. Outputs raw Chebyshev coefficient
//!    bands for a given body and time interval.
//! 3. [`body`] — enumerates the bodies the library can compute.
//!    Hides the DE441 internal ordering quirks (Earth-Moon barycenter
//!    triangulation, Moon-relative-to-Earth coefficients) from callers.
//! 4. [`time`] — Julian/Gregorian calendar ↔ Julian Day, ΔT (TT − UT)
//!    via SMH 2016 spline + observational table, LMT/fixed-offset zones.
//! 5. [`coords`] — light-time iteration, annual aberration, IAU 2006
//!    precession, IAU 2000B nutation, mean + true obliquity, sidereal
//!    time, the four axis modules ([`coords::acds`], [`coords::mcic`],
//!    [`coords::vxax`]), lunar nodes ([`coords::nodes`]), Black Moon
//!    Lilith + Priapus ([`coords::lilith`]), lunar phase
//!    ([`coords::phase`] — synodic arc, 8-fold name, 28-fold day),
//!    WGS84 topocentric parallax, and the geo/topo/helio
//!    apparent-position facades in [`coords::apparent`].
//! 6. [`houses`] — house cusps (Whole Sign, Equal-from-Ac, Placidus,
//!    Regiomontanus, Porphyry).
//! 7. [`lots`] — Hellenistic sect + the eight Hermetic lots (Fortune,
//!    Spirit, Exaltation, Necessity, Eros, Courage, Victory, Nemesis).
//! 8. [`geo`] — ISO 6709 DD/DMS/DDM geographic-coordinate parsing.
//! 9. [`spk`] — SPICE SPK reader for asteroid ephemerides (`sb441-n16.bsp` /
//!    `sb441-n373.bsp`); `Asteroid` enum (Ceres, Pallas, Juno, Vesta, Hygiea).
//!
//! ## Naming convention for chart points
//!
//! Every named chart point uses **2-letter `UPPERlower`** display labels
//! and lowercase 2-letter struct fields / function names / module paths.
//! Ascendant uses `ac` rather than `as` to dodge the Rust keyword:
//!
//! | Concept | Module | Library fn | Display |
//! |---|---|---|---|
//! | Ascendant / Descendant | [`coords::acds`] | `ac_rad` / `ds_rad` | `Ac` / `Ds` |
//! | Medium Coeli / Imum Coeli | [`coords::mcic`] | `mc_rad` / `ic_rad` | `Mc` / `Ic` |
//! | Vertex / Anti-Vertex | [`coords::vxax`] | `vx_rad` / `ax_rad` | `Vx` / `Ax` |
//! | North Node / South Node | [`coords::nodes`] | `mean_nn_rad` or `true_nn_rad`, `sn_rad` | `Nn` / `Sn` |
//! | Black Moon Lilith / Priapus | [`coords::lilith`] | `mean_lilith_rad` or `true_lilith_rad`, `priapus_rad` | `Lil` / `Pri` |
//!
//! Axis modules (acds, mcic, vxax) are named by concatenating the codes
//! for both endpoints of the axis.
//!
//! ## Computation modes (CLI aliases)
//!
//! Points with multiple computation modes accept these aliases:
//!
//! - `mean` ≡ `average`
//! - `true` ≡ `apparent` ≡ `osculating`
//! - `natural` ≡ `interpolated`
//!
//! Defaults to `true` for both [`coords::nodes`] and
//! [`coords::lilith`]. Natural/interpolated mode is reserved for
//! Black Moon Lilith and is currently not yet shipped.
//!
//! ## Shipped facades
//!
//! The single calls most callers want live in [`coords::apparent`]:
//! [`coords::apparent::apparent_ecliptic_position`] (geocentric),
//! [`coords::apparent::apparent_ecliptic_position_topocentric`]
//! (parallax-corrected), and
//! [`coords::apparent::heliocentric_ecliptic_position`].
//!
//! ## v1 non-goals (still deferred)
//!
//! - Sidereal zodiacs / ayanāṃśas (Lahiri, Fagan-Bradley, …)
//! - Dwarf planets and minor bodies beyond the five shipped asteroids (Chiron, Eris, …)
//! - Wider Hellenistic lot catalog beyond the Hermetic eight
//! - Natural / Interpolated Black Moon Lilith
//! - House systems beyond Whole Sign / Equal / Placidus / Regiomontanus
//!   / Porphyry (Koch, Campanus, Topocentric, Alcabitius, …)
//! - Full IANA tzdb (named historical zones)

#![cfg_attr(docsrs, feature(doc_cfg))]
#![warn(missing_docs)]
#![warn(rustdoc::missing_crate_level_docs)]
#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]

pub mod body;
pub mod chart;
pub mod chebyshev;
pub mod coords;
pub mod ephemeris;
pub mod error;
pub mod geo;
#[cfg(feature = "horizons")]
pub mod horizons;
pub mod houses;
pub mod jpl;
#[cfg(feature = "jzod")]
pub mod jzod;
pub mod lots;
pub mod manifest;
pub mod placements;
pub mod spk;
pub mod time;

pub use chart::{
    Angles, ChartRequest, ComputedBody, ComputedChart, CoordMode, LilithPoints, Lots, ModeRequest,
    NodePoints, compute,
};
pub use coords::topocentric::ObserverLocation;
#[cfg(feature = "jzod")]
pub use jzod::{ChartBirth, house_for, to_jzod_chart};
