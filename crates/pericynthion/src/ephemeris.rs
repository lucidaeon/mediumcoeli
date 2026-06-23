#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

//! High-level body-position computation atop the JPL DE-series file.
//!
//! This module glues together three lower-level pieces:
//!
//! 1. [`crate::jpl::header::Header`] — gives us the per-body layout
//!    (which words of a record belong to which body) and the named
//!    physical constants (EMRAT, AU, etc.) we need to derive Earth.
//! 2. [`crate::jpl::reader::RecordSource`] — the trait implemented by both
//!    [`crate::jpl::reader::EphemerisFile`] (binary, memory-mapped) and
//!    [`crate::jpl::ascii::AsciiEphemeris`] (ASCII chunks); supplies
//!    coefficient bytes for the record covering a target JD.
//! 3. [`crate::chebyshev`] — evaluates the polynomial series.
//!
//! The output is a [`StateVector`] — position and velocity in the
//! barycentric frame (or geocentric for the Moon, which is how JPL
//! stores it). Coordinate transformations into ecliptic-of-date and
//! the rest of the apparent-place pipeline happen in [`crate::coords`].
//!
//! # Interpolation arithmetic, in pictures
//!
//! Each coefficient record covers `granule_days` (32 for DE441). A
//! body with `subgranules = 4` further divides that 32-day record into
//! four 8-day sub-windows. For a target JD inside the record:
//!
//! ```text
//!         |--------record (32 d)--------|
//!         | sub 0 | sub 1 | sub 2 | sub 3 |
//!                            ^
//!                            jd
//! ```
//!
//! The body's `coeffs_per_axis` Chebyshev coefficients in sub 2 fit a
//! polynomial on the 8-day window. We normalize the target JD into the
//! window's local coordinate τ ∈ \[−1, +1\] and Clenshaw-evaluate the
//! series for X, Y, and Z. Velocity is the τ-derivative scaled by
//! `dτ/dt = 2 · subgranules / granule_days` (the chain rule from
//! "fraction of sub-granule" up to "days since J2000").

use crate::body::{Body, JplSlot};
use crate::chebyshev;
use crate::error::PericynthionError;
use crate::jpl::header::Header;
use crate::jpl::reader::RecordSource;

/// A body's position and velocity at a given Julian Date, expressed in
/// whichever rectangular frame the DE file stores it in.
///
/// For all bodies except the Moon, this is **barycentric** (relative
/// to the Solar System barycenter). For the Moon, this is
/// **geocentric** (relative to Earth's center). Units are kilometers
/// for position and kilometers-per-day for velocity.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StateVector {
    /// Position (km), \[X, Y, Z\] in the ICRS-aligned barycentric or
    /// geocentric frame as appropriate to the source body.
    pub position_km: [f64; 3],
    /// Velocity (km/day), \[Vx, Vy, Vz\] in the same frame.
    pub velocity_km_per_day: [f64; 3],
}

/// A high-level ephemeris computer that ties together a coefficient
/// source and its companion header.
///
/// Generic over any [`RecordSource`], so the same computer — and the
/// entire coordinate / houses / lots pipeline above it — runs unchanged
/// whether the source is the memory-mapped binary reader
/// ([`EphemerisFile`](crate::jpl::reader::EphemerisFile)) or the text
/// [`AsciiEphemeris`](crate::jpl::ascii::AsciiEphemeris).
///
/// Construct one at startup and call [`Ephemeris::state`] for each
/// body/JD pair you want.
///
/// The source is held as `&dyn RecordSource` so the many downstream
/// helpers that take `&Ephemeris` need no type parameter; dynamic
/// dispatch happens once per record fetch and is dwarfed by the
/// Chebyshev evaluation that follows.
pub struct Ephemeris<'a> {
    file: &'a dyn RecordSource,
    header: &'a Header,
    emrat: f64,
}

impl<'a> Ephemeris<'a> {
    /// Bundle a source with its header. Extracts EMRAT once so each
    /// [`Ephemeris::state`] call doesn't repeat the map lookup.
    ///
    /// # Errors
    ///
    /// Returns a header error if the source's companion header does not
    /// contain the `EMRAT` constant. All real DE-series headers do.
    pub fn new(file: &'a dyn RecordSource, header: &'a Header) -> Result<Self, PericynthionError> {
        let emrat = header.constants.get("EMRAT").copied().ok_or_else(|| {
            crate::error::HeaderError::InvalidLayout {
                detail: "header missing required constant EMRAT".into(),
            }
        })?;
        Ok(Self {
            file,
            header,
            emrat,
        })
    }

    /// The Earth-Moon mass ratio (E/M) used by this header.
    #[must_use]
    pub fn emrat(&self) -> f64 {
        self.emrat
    }

    /// Compute the [`StateVector`] for a body at the given Terrestrial
    /// Time Julian Date.
    ///
    /// # Errors
    ///
    /// Propagates I/O errors from the underlying [`RecordSource`] if
    /// the JD is outside the source's coverage.
    ///
    /// # Frame
    ///
    /// - Sun and planets: barycentric (relative to SSB).
    /// - Moon: geocentric (relative to Earth's center). Add the result
    ///   to Earth's barycentric position to obtain barycentric Moon.
    /// - Earth: barycentric, derived as `EMB − Moon / (1 + EMRAT)`.
    /// - `EarthMoonBarycenter`: as JPL stores it (barycentric).
    ///
    /// # Panics
    ///
    /// Panics if `body` is not Earth and has no JPL slot assignment.
    pub fn state(&self, body: Body, jd_tt: f64) -> Result<StateVector, PericynthionError> {
        if body == Body::Earth {
            return self.derive_earth(jd_tt);
        }
        let slot = body.jpl_slot().expect("non-Earth bodies have a slot");
        self.state_slot(slot, jd_tt)
    }

    /// Compute Earth's barycentric state from EMB and the Moon's
    /// geocentric state. The relationship is:
    ///
    /// `Earth_bary = EMB_bary − Moon_geo / (1 + EMRAT)`
    ///
    /// because the EMB is `(m_E · Earth + m_M · Moon) / (m_E + m_M)`
    /// and `m_M / (m_E + m_M) = 1 / (1 + EMRAT)`.
    fn derive_earth(&self, jd_tt: f64) -> Result<StateVector, PericynthionError> {
        let emb = self.state_slot(JplSlot::EarthMoonBarycenter, jd_tt)?;
        let moon_geo = self.state_slot(JplSlot::Moon, jd_tt)?;
        let f = 1.0 / (1.0 + self.emrat);
        let mut position_km = [0.0_f64; 3];
        let mut velocity_km_per_day = [0.0_f64; 3];
        for axis in 0..3 {
            position_km[axis] = emb.position_km[axis] - moon_geo.position_km[axis] * f;
            velocity_km_per_day[axis] =
                emb.velocity_km_per_day[axis] - moon_geo.velocity_km_per_day[axis] * f;
        }
        Ok(StateVector {
            position_km,
            velocity_km_per_day,
        })
    }

    /// Compute the state vector for one JPL slot directly. Handles the
    /// sub-granule selection, the τ → x normalization, and the
    /// Chebyshev / derivative evaluation per axis.
    fn state_slot(&self, slot: JplSlot, jd_tt: f64) -> Result<StateVector, PericynthionError> {
        let record = self.file.record_for_jd(jd_tt)?;
        let layout = &self.header.layout;
        let slot_idx = slot.index();
        let offset_1based = layout.offsets[slot_idx] as usize;
        let coeffs_per_axis = layout.coeffs_per_axis[slot_idx] as usize;
        let subgranules = layout.subgranules[slot_idx] as usize;
        let axes = slot.axes() as usize;

        // Granule fraction (0.0 inclusive at record start, 1.0 exclusive
        // at record end).
        let granule_days = self.file.granule_days();
        let frac = ((jd_tt - record.start_jd()) / granule_days).clamp(0.0, 1.0);

        // Which sub-granule does this JD fall in? clamp the upper edge
        // (frac == 1.0) into the last sub-granule.
        let sub_f = frac * subgranules as f64;
        let mut sub_index = sub_f.floor() as usize;
        if sub_index >= subgranules {
            sub_index = subgranules - 1;
        }

        // Position within sub-granule, normalized to x ∈ [−1, +1].
        // sub_local = sub_f − sub_index ∈ [0, 1]
        // x = 2 · sub_local − 1
        let sub_local = sub_f - sub_index as f64;
        let x = 2.0 * sub_local - 1.0;

        // Byte indices: the slot's coefficients start at word offset
        // `offset_1based − 1` (convert 1-indexed JPL offset to 0-indexed).
        // Within that, sub_index advances by `axes · coeffs_per_axis`.
        // Within a sub-granule, axis k advances by `coeffs_per_axis`.
        let slot_base = offset_1based - 1;
        let sub_base = slot_base + sub_index * axes * coeffs_per_axis;

        let mut position_km = [0.0_f64; 3];
        let mut velocity_km_per_day = [0.0_f64; 3];
        // d τ / d t = 2 · subgranules / granule_days
        // (factor of 2 because we mapped sub_local ∈ [0,1] → x ∈ [-1,+1])
        let dtau_dt = 2.0 * subgranules as f64 / granule_days;

        for axis in 0..axes {
            let axis_base = sub_base + axis * coeffs_per_axis;
            let coeffs = record.slice(axis_base, coeffs_per_axis);
            position_km[axis] = chebyshev::evaluate(&coeffs, x);
            // dPos/dt = (dPos/dx) · (dx/dt) where dx/dt = dτ/dt.
            velocity_km_per_day[axis] = chebyshev::evaluate_derivative(&coeffs, x) * dtau_dt;
        }
        Ok(StateVector {
            position_km,
            velocity_km_per_day,
        })
    }
}

#[cfg(test)]
mod tests {
    // Pure unit tests live in `body.rs` and `chebyshev.rs`; the body-
    // position interpolation requires an actual ephemeris file and is
    // covered by the `ephemeris_de441.rs` integration test.
}
