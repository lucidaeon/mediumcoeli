#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::needless_range_loop
)]

//! Whole Sign house system.
//!
//! The sign containing the ASC is house 1. Each subsequent sign is the
//! next house. All cusps fall at exact sign boundaries (multiples of 30°).

use super::HouseCusps;

/// Whole Sign house cusps from the Ascendant longitude (radians).
///
/// Returns cusps at sign boundaries: cusp of house 1 = start of ASC's sign,
/// cusp of house 2 = next sign, and so on.
#[must_use]
pub fn whole_sign_rad(ac_rad: f64) -> HouseCusps {
    let ac_deg = ac_rad.to_degrees().rem_euclid(360.0);
    let sign_idx = (ac_deg / 30.0).floor() as usize; // 0 = Aries … 11 = Pisces
    let sign_base_deg = sign_idx as f64 * 30.0;
    let mut cusps = [0.0_f64; 12];
    for i in 0..12 {
        cusps[i] = ((sign_base_deg + i as f64 * 30.0).rem_euclid(360.0)).to_radians();
    }
    HouseCusps(cusps)
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_abs_diff_eq;

    const EPS: f64 = 1e-10;

    #[test]
    fn aries_rising_gives_aries_as_h1() {
        // ASC anywhere in Aries → H1 starts at 0°
        let hc = whole_sign_rad(15_f64.to_radians());
        assert_abs_diff_eq!(hc.cusp(1).to_degrees(), 0.0, epsilon = EPS);
        assert_abs_diff_eq!(hc.cusp(2).to_degrees(), 30.0, epsilon = EPS);
        assert_abs_diff_eq!(hc.cusp(12).to_degrees(), 330.0, epsilon = EPS);
    }

    #[test]
    fn scorpio_rising_gives_scorpio_as_h1() {
        // ASC at 215° (Scorpio, 210°–240°) → H1 = 210°
        // Sc(H1)→Sg(H2)→Cp(H3)→Aq(H4)→Pi(H5)→Ar(H6)→Ta(H7)→Ge(H8)→Cn(H9)→Le(H10)→Vi(H11)→Li(H12)
        let hc = whole_sign_rad(215_f64.to_radians());
        assert_abs_diff_eq!(hc.cusp(1).to_degrees(), 210.0, epsilon = EPS); // Scorpio
        assert_abs_diff_eq!(hc.cusp(4).to_degrees(), 300.0, epsilon = EPS); // Aquarius
        assert_abs_diff_eq!(hc.cusp(7).to_degrees(), 30.0, epsilon = EPS); // Taurus
        assert_abs_diff_eq!(hc.cusp(10).to_degrees(), 120.0, epsilon = EPS); // Leo
    }

    #[test]
    fn pisces_rising_wraps_correctly() {
        // ASC at 340° (Pisces) → H1 = 330°, H2 = 0° Aries
        let hc = whole_sign_rad(340_f64.to_radians());
        assert_abs_diff_eq!(hc.cusp(1).to_degrees(), 330.0, epsilon = EPS);
        assert_abs_diff_eq!(hc.cusp(2).to_degrees(), 0.0, epsilon = EPS);
        assert_abs_diff_eq!(hc.cusp(12).to_degrees(), 300.0, epsilon = EPS);
    }

    #[test]
    fn house_of_places_body_correctly() {
        // Scorpio rising: H1=210°, H2=240°(Sg), H3=270°(Cp), H4=300°(Aq)...
        let hc = whole_sign_rad(215_f64.to_radians());
        assert_eq!(hc.house_of(220_f64.to_radians()), 1); // mid-Scorpio → H1
        assert_eq!(hc.house_of(250_f64.to_radians()), 2); // mid-Sagittarius → H2
        assert_eq!(hc.house_of(280_f64.to_radians()), 3); // Capricorn → H3
        assert_eq!(hc.house_of(305_f64.to_radians()), 4); // mid-Aquarius → H4
        assert_eq!(hc.house_of(30_f64.to_radians()), 7); // Taurus → H7
    }

    // ── Reference charts ─────────────────────────────────────────────────────

    #[test]
    fn vettius_valens_whole_sign_h1() {
        // ASC: Vir⌖01°29'03" — Virgo rising, so H1 = 150°.
        let ac = (150.0 + 1.0 + 29.0 / 60.0 + 3.0 / 3600.0_f64).to_radians();
        let hc = whole_sign_rad(ac);
        assert_abs_diff_eq!(hc.cusp(1).to_degrees(), 150.0, epsilon = EPS);
        assert_abs_diff_eq!(hc.cusp(4).to_degrees(), 240.0, epsilon = EPS); // Sagittarius
        assert_abs_diff_eq!(hc.cusp(7).to_degrees(), 330.0, epsilon = EPS); // Pisces
        assert_abs_diff_eq!(hc.cusp(10).to_degrees(), 60.0, epsilon = EPS); // Gemini
    }

    #[test]
    fn whole_sign_pf_house() {
        // ASC: Aquarius (300°). PF: Gem⌖02°21'41" = 62.36° → Gemini.
        // In whole sign from Aquarius: Gemini is the 5th sign from Aquarius.
        // Aqr(1), Pis(2), Ari(3), Tau(4), Gem(5)
        let ac = (300.0 + 17.0 + 40.0 / 60.0 + 17.0 / 3600.0_f64).to_radians();
        let pf = (60.0 + 2.0 + 21.0 / 60.0 + 41.0 / 3600.0_f64).to_radians();
        let hc = whole_sign_rad(ac);
        assert_eq!(hc.house_of(pf), 5);
    }
}
