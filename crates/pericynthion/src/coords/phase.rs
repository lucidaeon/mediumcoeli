//! Lunar phase: synodic arc, 8-fold phase name, 28-fold lunation day.
//!
//! The **synodic arc** is the ecliptic elongation of the Moon ahead of the
//! Sun: `(moon_longitude − sun_longitude).rem_euclid(360°)`.  It drives
//! everything else:
//!
//! - The **8-fold phase name** divides the full synodic cycle into eight
//!   45° octants starting at New Moon (0°).
//! - The **lunation day** is the 1-indexed position within the 28-fold
//!   lunar month: `⌊arc / (360 / 28)⌋ + 1`, range 1–28.
//!
//! This module contains no ephemeris I/O — callers supply the two
//! geocentric ecliptic longitudes already computed by
//! [`apparent_ecliptic_position`](crate::coords::apparent::apparent_ecliptic_position).

/// The eight traditional phases of the synodic cycle.
///
/// Each phase covers a 45° arc of the Moon's elongation ahead of the Sun.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LunarPhaseName {
    /// 0–45°: the Moon is near conjunction with the Sun.
    NewMoon,
    /// 45–90°: the waxing crescent visible in the western evening sky.
    Crescent,
    /// 90–135°: the waxing half-disc at eastern quadrature.
    FirstQuarter,
    /// 135–180°: the waxing gibbous approaching opposition.
    Gibbous,
    /// 180–225°: the Moon is near opposition to the Sun.
    FullMoon,
    /// 225–270°: the waning gibbous disseminating phase.
    Disseminating,
    /// 270–315°: the waning half-disc at western quadrature.
    LastQuarter,
    /// 315–360°: the waning balsamic crescent approaching conjunction.
    Balsamic,
}

impl LunarPhaseName {
    /// Lower-case human label for the phase (`"new moon"`, `"first quarter"`,
    /// …). Shared by every front-end so the CLI and a GUI print identical names.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            LunarPhaseName::NewMoon => "new moon",
            LunarPhaseName::Crescent => "crescent",
            LunarPhaseName::FirstQuarter => "first quarter",
            LunarPhaseName::Gibbous => "gibbous",
            LunarPhaseName::FullMoon => "full moon",
            LunarPhaseName::Disseminating => "disseminating",
            LunarPhaseName::LastQuarter => "last quarter",
            LunarPhaseName::Balsamic => "balsamic",
        }
    }
}

/// Computed lunar phase: synodic arc, phase name, and lunation day.
#[derive(Debug, Clone, PartialEq)]
pub struct LunarPhase {
    /// Moon elongation ahead of the Sun, in degrees, range \[0, 360).
    pub synodic_arc_deg: f64,
    /// 8-fold phase name derived from the arc.
    pub phase: LunarPhaseName,
    /// 1-indexed position within the 28-fold lunar month, range 1–28.
    pub lunation_day: u8,
}

/// Compute the lunar phase from geocentric tropical longitudes.
///
/// Both arguments are ecliptic longitude in degrees.  The synodic arc
/// (`moon − sun` modulo 360°) drives the phase name and lunation day.
///
/// Only meaningful for geocentric / topocentric positions — heliocentric
/// charts have no Earth-relative Moon illumination.  Callers are expected
/// to gate on coordinate mode before calling.
#[must_use]
pub fn lunar_phase(moon_lon_deg: f64, sun_lon_deg: f64) -> LunarPhase {
    let arc = (moon_lon_deg - sun_lon_deg).rem_euclid(360.0);

    // 8-fold: each octant is 45°. floor(arc/45) gives 0..7.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let octant = (arc / 45.0).floor() as u8;
    let phase = match octant {
        0 => LunarPhaseName::NewMoon,
        1 => LunarPhaseName::Crescent,
        2 => LunarPhaseName::FirstQuarter,
        3 => LunarPhaseName::Gibbous,
        4 => LunarPhaseName::FullMoon,
        5 => LunarPhaseName::Disseminating,
        6 => LunarPhaseName::LastQuarter,
        _ => LunarPhaseName::Balsamic,
    };

    // 28-fold: each "day" is 360/28 ≈ 12.857°. Clamp to 1–28.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let lunation_day = ((arc / (360.0 / 28.0)).floor() as u8)
        .saturating_add(1)
        .min(28);

    LunarPhase {
        synodic_arc_deg: arc,
        phase,
        lunation_day,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase_labels_cover_all_eight() {
        assert_eq!(LunarPhaseName::NewMoon.label(), "new moon");
        assert_eq!(LunarPhaseName::FirstQuarter.label(), "first quarter");
        assert_eq!(LunarPhaseName::FullMoon.label(), "full moon");
        assert_eq!(LunarPhaseName::Disseminating.label(), "disseminating");
        assert_eq!(LunarPhaseName::Balsamic.label(), "balsamic");
    }

    // Use arc directly: lunar_phase(arc, 0.0) gives synodic_arc == arc.

    // ── boundary cases ────────────────────────────────────────────────────────

    #[test]
    fn new_moon_at_zero() {
        let p = lunar_phase(0.0, 0.0);
        assert!(p.synodic_arc_deg.abs() < 1e-9);
        assert_eq!(p.phase, LunarPhaseName::NewMoon);
        assert_eq!(p.lunation_day, 1);
    }

    #[test]
    fn crescent_starts_at_45() {
        let p = lunar_phase(45.0, 0.0);
        assert_eq!(p.phase, LunarPhaseName::Crescent);
        assert_eq!(p.lunation_day, 4); // 45/(360/28)=3.5 → floor=3 → +1=4
    }

    #[test]
    fn full_moon_starts_at_180() {
        let p = lunar_phase(180.0, 0.0);
        assert_eq!(p.phase, LunarPhaseName::FullMoon);
        assert_eq!(p.lunation_day, 15); // 180/(360/28)=14.0 → floor=14 → +1=15
    }

    #[test]
    fn balsamic_starts_at_315() {
        let p = lunar_phase(315.0, 0.0);
        assert_eq!(p.phase, LunarPhaseName::Balsamic);
        assert_eq!(p.lunation_day, 25); // 315/(360/28)=24.5 → floor=24 → +1=25
    }

    #[test]
    fn wraps_correctly_for_moon_behind_sun() {
        // moon=10°, sun=350° → arc = (10−350).rem_euclid(360) = 20° → NewMoon
        let p = lunar_phase(10.0, 350.0);
        assert!((p.synodic_arc_deg - 20.0).abs() < 1e-9);
        assert_eq!(p.phase, LunarPhaseName::NewMoon);
    }

    // ── refchart oracles ──────────────────────────────────────────────────────
    // Each test passes (arc, 0.0) so synodic_arc == arc. Arc ±0.1° tolerance
    // matches the arcminute precision of the reference chart printouts.

    #[test]
    fn refchart_adele_haenel_crescent() {
        // Angle: +72°47' = 72.783°, Crescent (2nd of 8), day 6
        let p = lunar_phase(72.783, 0.0);
        assert!((p.synodic_arc_deg - 72.783).abs() < 0.1);
        assert_eq!(p.phase, LunarPhaseName::Crescent);
        assert_eq!(p.lunation_day, 6);
    }

    #[test]
    fn refchart_anna_freud_full_moon() {
        // Angle: +196°22' = 196.367°, Full Moon (5th of 8), day 16
        let p = lunar_phase(196.367, 0.0);
        assert!((p.synodic_arc_deg - 196.367).abs() < 0.1);
        assert_eq!(p.phase, LunarPhaseName::FullMoon);
        assert_eq!(p.lunation_day, 16);
    }

    #[test]
    fn refchart_lightning_strike_balsamic() {
        // Angle: +346°01' = 346.017°, Balsamic (8th of 8), day 27
        let p = lunar_phase(346.017, 0.0);
        assert!((p.synodic_arc_deg - 346.017).abs() < 0.1);
        assert_eq!(p.phase, LunarPhaseName::Balsamic);
        assert_eq!(p.lunation_day, 27);
    }

    #[test]
    fn refchart_william_lilly_disseminating() {
        // Angle: +234°48' = 234.800°, Disseminating (6th of 8), day 19
        let p = lunar_phase(234.800, 0.0);
        assert!((p.synodic_arc_deg - 234.800).abs() < 0.1);
        assert_eq!(p.phase, LunarPhaseName::Disseminating);
        assert_eq!(p.lunation_day, 19);
    }

    #[test]
    fn refchart_vettius_valens_disseminating() {
        // Angle: +253°10' = 253.167°, Disseminating (6th of 8), day 20
        let p = lunar_phase(253.167, 0.0);
        assert!((p.synodic_arc_deg - 253.167).abs() < 0.1);
        assert_eq!(p.phase, LunarPhaseName::Disseminating);
        assert_eq!(p.lunation_day, 20);
    }

    #[test]
    fn refchart_first_contact_crescent() {
        // Angle: +81°47' = 81.783°, Crescent (2nd of 8), day 7
        let p = lunar_phase(81.783, 0.0);
        assert!((p.synodic_arc_deg - 81.783).abs() < 0.1);
        assert_eq!(p.phase, LunarPhaseName::Crescent);
        assert_eq!(p.lunation_day, 7);
    }

    #[test]
    fn refchart_roberta_bondar_balsamic() {
        // Angle: +357°38' = 357.633°, Balsamic (8th of 8), day 28
        let p = lunar_phase(357.633, 0.0);
        assert!((p.synodic_arc_deg - 357.633).abs() < 0.1);
        assert_eq!(p.phase, LunarPhaseName::Balsamic);
        assert_eq!(p.lunation_day, 28);
    }
}
