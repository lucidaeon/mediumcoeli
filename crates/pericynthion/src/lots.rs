//! Hellenistic / Arabic lots (parts): derived chart points computed
//! from the Ascendant and pairs of planets, with sect-aware day/night
//! formulas.
//!
//! Shipped: sect determination plus the eight Hermetic lots — Fortune,
//! Spirit, Exaltation, Necessity, Eros, Courage, Victory, Nemesis. The
//! seven sect-symmetric lots all delegate to [`hermetic_lot_rad`];
//! Exaltation is the outlier with separate day/night formulas.
//!
//! All inputs and outputs are radians in \[0, TAU).

use std::f64::consts::{PI, TAU};

/// Whether the Sun is above or below the horizon at chart time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sect {
    /// Sun above the horizon (houses 7–12).
    Day,
    /// Sun below the horizon (houses 1–6).
    Night,
}

/// Determine sect from Sun and Ascendant longitudes (radians).
///
/// Day when the Sun is in the upper hemisphere (houses 7–12),
/// i.e. between DSC and ASC going counterclockwise through the MC.
#[must_use]
pub fn sect(sun_lon_rad: f64, ac_rad: f64) -> Sect {
    if (sun_lon_rad - ac_rad + PI).rem_euclid(TAU) < PI {
        Sect::Day
    } else {
        Sect::Night
    }
}

/// Base lot formula: `(ac + a − b) mod 2π`.
#[must_use]
pub fn lot_rad(ac: f64, a: f64, b: f64) -> f64 {
    (ac + a - b).rem_euclid(TAU)
}

/// Generic Hermetic lot formula: personal point + significator − trigger,
/// with optional sect reversal at night.
///
/// Day (or `reverse=false` at night): ASC + significator − trigger.
/// Night with `reverse=true`: ASC + trigger − significator (swap).
///
/// All seven of {Fortune, Spirit, Eros, Necessity, Courage, Victory,
/// Nemesis} follow this pattern with `reverse=true`. Exaltation does
/// not — its day and night formulas use different significators
/// (18° Aries / 2° Taurus) and triggers (Sun / Moon).
#[must_use]
pub fn hermetic_lot_rad(
    ac: f64,
    significator: f64,
    trigger: f64,
    sect: Sect,
    reverse: bool,
) -> f64 {
    match (sect, reverse) {
        (Sect::Day, _) | (Sect::Night, false) => lot_rad(ac, significator, trigger),
        (Sect::Night, true) => lot_rad(ac, trigger, significator),
    }
}

/// Part of Fortune.
///
/// Day: ASC + Moon − Sun.  Night: ASC + Sun − Moon.
#[must_use]
pub fn fortune_rad(ac: f64, sun: f64, moon: f64, sect: Sect) -> f64 {
    hermetic_lot_rad(ac, moon, sun, sect, true)
}

/// Part of Spirit.
///
/// Day: ASC + Sun − Moon.  Night: ASC + Moon − Sun.
/// Always the sect-inverse of Fortune.
#[must_use]
pub fn spirit_rad(ac: f64, sun: f64, moon: f64, sect: Sect) -> f64 {
    hermetic_lot_rad(ac, sun, moon, sect, true)
}

/// Lot of Eros (Erotos): the Hellenistic lot of desire.
///
/// Day: ASC + Venus − Spirit.  Night: ASC + Spirit − Venus.
/// Built on top of Spirit, so Spirit's sect choice flows through.
#[must_use]
pub fn eros_rad(ac: f64, sun: f64, moon: f64, venus: f64, sect: Sect) -> f64 {
    let spirit = spirit_rad(ac, sun, moon, sect);
    hermetic_lot_rad(ac, venus, spirit, sect, true)
}

/// Lot of Necessity (Anankē).
///
/// Day: ASC + Fortune − Mercury.  Night: ASC + Mercury − Fortune.
#[must_use]
pub fn necessity_rad(ac: f64, sun: f64, moon: f64, mercury: f64, sect: Sect) -> f64 {
    let fortune = fortune_rad(ac, sun, moon, sect);
    hermetic_lot_rad(ac, fortune, mercury, sect, true)
}

/// Lot of Courage (Tolma).
///
/// Day: ASC + Fortune − Mars.  Night: ASC + Mars − Fortune.
#[must_use]
pub fn courage_rad(ac: f64, sun: f64, moon: f64, mars: f64, sect: Sect) -> f64 {
    let fortune = fortune_rad(ac, sun, moon, sect);
    hermetic_lot_rad(ac, fortune, mars, sect, true)
}

/// Lot of Victory (Nikē).
///
/// Day: ASC + Jupiter − Spirit.  Night: ASC + Spirit − Jupiter.
#[must_use]
pub fn victory_rad(ac: f64, sun: f64, moon: f64, jupiter: f64, sect: Sect) -> f64 {
    let spirit = spirit_rad(ac, sun, moon, sect);
    hermetic_lot_rad(ac, jupiter, spirit, sect, true)
}

/// Lot of Nemesis.
///
/// Day: ASC + Fortune − Saturn.  Night: ASC + Saturn − Fortune.
#[must_use]
pub fn nemesis_rad(ac: f64, sun: f64, moon: f64, saturn: f64, sect: Sect) -> f64 {
    let fortune = fortune_rad(ac, sun, moon, sect);
    hermetic_lot_rad(ac, fortune, saturn, sect, true)
}

/// Lot of Exaltation. Hermetic outlier — separate day/night formulas
/// rather than a simple sect swap.
///
/// Day: ASC + 18° Aries − Sun     (significator is the Sun's exaltation degree)
/// Night: ASC + 2° Taurus − Moon  (significator is the Moon's exaltation degree)
#[must_use]
pub fn exaltation_rad(ac: f64, sun: f64, moon: f64, sect: Sect) -> f64 {
    match sect {
        Sect::Day => lot_rad(ac, 18.0_f64.to_radians(), sun),
        Sect::Night => lot_rad(ac, 32.0_f64.to_radians(), moon),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_abs_diff_eq;

    // ── Sect determination ───────────────────────────────────────────────────

    // At equator with RAMC=90°: MC=Cancer(90°), ASC=Libra(180°), DSC=Aries(0°), IC=Capricorn(270°).
    // Upper hemisphere (houses 7-12) = DSC→ASC through MC = 0°→90°→180°.

    #[test]
    fn sun_at_mc_is_day() {
        // ASC=180°(Libra), MC=90°(Cancer). Sun at MC = above horizon.
        let ac = 180_f64.to_radians();
        let sun = 90_f64.to_radians();
        assert_eq!(sect(sun, ac), Sect::Day);
    }

    #[test]
    fn sun_at_ic_is_night() {
        // IC = 270°(Capricorn); Sun at IC = below horizon.
        let ac = 180_f64.to_radians();
        let sun = 270_f64.to_radians();
        assert_eq!(sect(sun, ac), Sect::Night);
    }

    #[test]
    fn sun_in_upper_hemisphere_is_day() {
        // Sun between DSC(0°) and ASC(180°) via MC(90°) = Day.
        let ac = 180_f64.to_radians();
        let sun = 45_f64.to_radians(); // between DSC and MC
        assert_eq!(sect(sun, ac), Sect::Day);
    }

    #[test]
    fn sun_in_lower_hemisphere_is_night() {
        // Sun between ASC(180°) and DSC(0°) via IC(270°) = Night.
        let ac = 180_f64.to_radians();
        let sun = 225_f64.to_radians(); // between ASC and IC
        assert_eq!(sect(sun, ac), Sect::Night);
    }

    // ── Eros: ASC + Venus − Spirit (day) / ASC + Spirit − Venus (night) ─────

    #[test]
    fn eros_day_matches_literal_formula() {
        // Day chart: Eros = ASC + Venus − Spirit, where Spirit = ASC + Sun − Moon.
        let ac = 90_f64.to_radians();
        let sun = 30_f64.to_radians();
        let moon = 60_f64.to_radians();
        let venus = 45_f64.to_radians();
        let s = sect(sun, ac);
        assert_eq!(s, Sect::Day);
        let spirit = spirit_rad(ac, sun, moon, s);
        let expected = (ac + venus - spirit).rem_euclid(TAU);
        let got = eros_rad(ac, sun, moon, venus, s);
        assert_abs_diff_eq!(got, expected, epsilon = 1e-12);
    }

    #[test]
    fn eros_night_matches_literal_formula() {
        // Night chart: Eros = ASC + Spirit − Venus, where Spirit = ASC + Moon − Sun.
        // ASC=315°(Aq), DSC=135°(Le), IC=45°(Ta). Sun at 10° (Ar) sits in the
        // lower hemisphere arc ASC→IC→DSC, i.e. below the horizon.
        let ac = 315_f64.to_radians();
        let sun = 10_f64.to_radians();
        let moon = 250_f64.to_radians();
        let venus = 220_f64.to_radians();
        let s = sect(sun, ac);
        assert_eq!(s, Sect::Night);
        let spirit = spirit_rad(ac, sun, moon, s);
        let expected = (ac + spirit - venus).rem_euclid(TAU);
        let got = eros_rad(ac, sun, moon, venus, s);
        assert_abs_diff_eq!(got, expected, epsilon = 1e-12);
    }

    #[test]
    fn eros_day_reduces_to_venus_minus_sun_plus_moon() {
        // Algebraic identity: Day Eros = Venus − Sun + Moon (ASC cancels).
        for (ac_d, sun_d, moon_d, venus_d) in [
            (90.0_f64, 30.0_f64, 60.0_f64, 45.0_f64),
            (180.0, 45.0, 120.0, 75.0),
        ] {
            let ac = ac_d.to_radians();
            let sun = sun_d.to_radians();
            let moon = moon_d.to_radians();
            let venus = venus_d.to_radians();
            let s = sect(sun, ac);
            assert_eq!(s, Sect::Day);
            let got = eros_rad(ac, sun, moon, venus, s);
            let expected = (venus - sun + moon).rem_euclid(TAU);
            assert_abs_diff_eq!(got, expected, epsilon = 1e-12);
        }
    }

    // ── Fortune + Spirit are always sect-inverses ────────────────────────────

    #[test]
    fn fortune_and_spirit_sum_to_2_asc() {
        // F + S = 2·ASC (the bodies cancel), modulo 2π.
        for (sun_d, moon_d, ac_d) in [(30.0_f64, 60.0_f64, 90.0_f64), (250.0, 10.0, 315.0)] {
            let (sun, moon, ac) = (sun_d.to_radians(), moon_d.to_radians(), ac_d.to_radians());
            let s = sect(sun, ac);
            let f = fortune_rad(ac, sun, moon, s);
            let sp = spirit_rad(ac, sun, moon, s);
            let sum = (f + sp).rem_euclid(TAU);
            let expected = (2.0 * ac).rem_euclid(TAU);
            assert_abs_diff_eq!(sum, expected, epsilon = 1e-10);
        }
    }

    // Reference-chart Fortune tests against refchart's resolved coords live
    // in `tests/acceptance_refchart.rs` so the constants stay in a single
    // place.
}
