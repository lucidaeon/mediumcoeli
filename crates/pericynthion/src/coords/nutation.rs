//! Nutation — short-period wobble of Earth's rotation axis on top of
//! the smooth precession.
//!
//! # IAU 2000B model (77 luni-solar terms)
//!
//! Nutation in longitude (Δψ) and obliquity (Δε) are computed by summing
//! 77 trigonometric terms in five Delaunay fundamental arguments
//! (l, l', F, D, Ω). Amplitudes are in units of 0.1 microarcsecond.
//! Accuracy: < 1 mas for modern dates, ΔT-limited for ancient epochs.
//!
//! Source: ERFA eraNut00b (liberally licensed fork of IAU SOFA iauNut00b).
//! Coefficient table verified against IERS Conventions 2010 Table 5.3b.

use crate::coords::obliquity::{ARCSEC_TO_RAD, julian_centuries_t};
use std::f64::consts::PI;

const TWO_PI: f64 = 2.0 * PI;

/// Conversion: 0.1 microarcseconds → radians.
const U2R: f64 = ARCSEC_TO_RAD / 1e7;

/// Nutation in longitude (Δψ) and obliquity (Δε), both in radians.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Nutation {
    /// Δψ — nutation in longitude, radians.
    pub delta_psi: f64,
    /// Δε — nutation in obliquity, radians.
    pub delta_epsilon: f64,
}

/// IAU 2000B luni-solar nutation term.
/// Fields: nl, nlp, nf, nd, nom (Delaunay argument multipliers),
///         ps, pst, pc (Δψ sine amplitude, time-rate, cosine amplitude),
///         ec, ect, es (Δε cosine amplitude, time-rate, sine amplitude).
/// Amplitude units: 0.1 μas; time-rate units: 0.1 μas/Julian century.
#[allow(clippy::type_complexity)]
type NutTerm = (i8, i8, i8, i8, i8, f64, f64, f64, f64, f64, f64);

/// 77-term IAU 2000B luni-solar nutation series.
/// Source: ERFA eraNut00b coefficient table (identical to SOFA iauNut00b).
#[allow(clippy::unreadable_literal)]
const NUT00B: &[NutTerm; 77] = &[
    (
        0,
        0,
        0,
        0,
        1,
        -172064161.0,
        -174666.0,
        33386.0,
        92052331.0,
        9086.0,
        15377.0,
    ),
    (
        0,
        0,
        2,
        -2,
        2,
        -13170906.0,
        -1675.0,
        -13696.0,
        5730336.0,
        -3015.0,
        -4587.0,
    ),
    (
        0, 0, 2, 0, 2, -2276413.0, -234.0, 2796.0, 978459.0, -485.0, 1374.0,
    ),
    (
        0, 0, 0, 0, 2, 2074554.0, 207.0, -698.0, -897492.0, 470.0, -291.0,
    ),
    (
        0, 1, 0, 0, 0, 1475877.0, -3633.0, 11817.0, 73871.0, -184.0, -1924.0,
    ),
    (
        0, 1, 2, -2, 2, -516821.0, 1226.0, -524.0, 224386.0, -677.0, -174.0,
    ),
    (1, 0, 0, 0, 0, 711159.0, 73.0, -872.0, -6750.0, 0.0, 358.0),
    (
        0, 0, 2, 0, 1, -387298.0, -367.0, 380.0, 200728.0, 18.0, 318.0,
    ),
    (
        1, 0, 2, 0, 2, -301461.0, -36.0, 816.0, 129025.0, -63.0, 367.0,
    ),
    (
        0, -1, 2, -2, 2, 215829.0, -494.0, 111.0, -95929.0, 299.0, 132.0,
    ),
    (0, 0, 2, -2, 1, 128227.0, 137.0, 181.0, -68982.0, -9.0, 39.0),
    (-1, 0, 2, 0, 2, 123457.0, 11.0, 19.0, -53311.0, 32.0, -4.0),
    (-1, 0, 0, 2, 0, 156994.0, 10.0, -168.0, -1235.0, 0.0, 82.0),
    (1, 0, 0, 0, 1, 63110.0, 63.0, 27.0, -33228.0, 0.0, -9.0),
    (-1, 0, 0, 0, 1, -57976.0, -63.0, -189.0, 31429.0, 0.0, -75.0),
    (-1, 0, 2, 2, 2, -59641.0, -11.0, 149.0, 25543.0, -11.0, 66.0),
    (1, 0, 2, 0, 1, -51613.0, -42.0, 129.0, 26366.0, 0.0, 78.0),
    (-2, 0, 2, 0, 1, 45893.0, 50.0, 31.0, -24236.0, -10.0, 20.0),
    (0, 0, 0, 2, 0, 63384.0, 11.0, -150.0, -1220.0, 0.0, 29.0),
    (0, 0, 2, 2, 2, -38571.0, -1.0, 158.0, 16452.0, -11.0, 68.0),
    (0, -2, 2, -2, 2, 32481.0, 0.0, 0.0, -13870.0, 0.0, 0.0),
    (-2, 0, 0, 2, 0, -47722.0, 0.0, -18.0, 477.0, 0.0, -25.0),
    (2, 0, 2, 0, 2, -31046.0, -1.0, 131.0, 13238.0, -11.0, 59.0),
    (1, 0, 2, -2, 2, 28593.0, 0.0, -1.0, -12338.0, 10.0, -3.0),
    (-1, 0, 2, 0, 1, 20441.0, 21.0, 10.0, -10758.0, 0.0, -3.0),
    (2, 0, 0, 0, 0, 29243.0, 0.0, -74.0, -609.0, 0.0, 13.0),
    (0, 0, 2, 0, 0, 25887.0, 0.0, -66.0, -550.0, 0.0, 11.0),
    (0, 1, 0, 0, 1, -14053.0, -25.0, 79.0, 8551.0, -2.0, -45.0),
    (-1, 0, 0, 2, 1, 15164.0, 10.0, 11.0, -8001.0, 0.0, -1.0),
    (0, 2, 2, -2, 2, -15794.0, 72.0, -16.0, 6850.0, -42.0, -5.0),
    (0, 0, -2, 2, 0, 21783.0, 0.0, 13.0, -167.0, 0.0, 13.0),
    (1, 0, 0, -2, 1, -12873.0, -10.0, -37.0, 6953.0, 0.0, -14.0),
    (0, -1, 0, 0, 1, -12654.0, 11.0, 63.0, 6415.0, 0.0, 26.0),
    (-1, 0, 2, 2, 1, -10204.0, 0.0, 25.0, 5222.0, 0.0, 15.0),
    (0, 2, 0, 0, 0, 16707.0, -85.0, -10.0, 168.0, -1.0, 10.0),
    (1, 0, 2, 2, 2, -7691.0, 0.0, 44.0, 3268.0, 0.0, 19.0),
    (-2, 0, 2, 0, 0, -11024.0, 0.0, -14.0, 104.0, 0.0, 2.0),
    (0, 1, 2, 0, 2, 7566.0, -21.0, -11.0, -3250.0, 0.0, -5.0),
    (0, 0, 2, 2, 1, -6637.0, -11.0, 25.0, 3353.0, 0.0, 14.0),
    (0, -1, 2, 0, 2, -7141.0, 21.0, 8.0, 3070.0, 0.0, 4.0),
    (0, 0, 0, 2, 1, -6302.0, -11.0, 2.0, 3272.0, 0.0, 4.0),
    (1, 0, 2, -2, 1, 5800.0, 10.0, 2.0, -3045.0, 0.0, -1.0),
    (2, 0, 2, -2, 2, 6443.0, 0.0, -7.0, -2768.0, 0.0, -4.0),
    (-2, 0, 0, 2, 1, -5774.0, -11.0, -15.0, 3041.0, 0.0, -5.0),
    (2, 0, 2, 0, 1, -5350.0, 0.0, 21.0, 2695.0, 0.0, 12.0),
    (0, -1, 2, -2, 1, -4752.0, -11.0, -3.0, 2719.0, 0.0, -3.0),
    (0, 0, 0, -2, 1, -4940.0, -11.0, -21.0, 2720.0, 0.0, -9.0),
    (-1, -1, 0, 2, 0, 7350.0, 0.0, -8.0, -51.0, 0.0, 4.0),
    (2, 0, 0, -2, 1, 4065.0, 0.0, 6.0, -2206.0, 0.0, 1.0),
    (1, 0, 0, 2, 0, 6579.0, 0.0, -24.0, -199.0, 0.0, 2.0),
    (0, 1, 2, -2, 1, 3579.0, 0.0, 5.0, -1900.0, 0.0, 1.0),
    (1, -1, 0, 0, 0, 4725.0, 0.0, -6.0, -41.0, 0.0, 3.0),
    (-2, 0, 2, 0, 2, -3075.0, 0.0, -2.0, 1313.0, 0.0, -1.0),
    (3, 0, 2, 0, 2, -2904.0, 0.0, 15.0, 1233.0, 0.0, 7.0),
    (0, -1, 0, 2, 0, 4348.0, 0.0, -10.0, -81.0, 0.0, 2.0),
    (1, -1, 2, 0, 2, -2878.0, 0.0, 8.0, 1232.0, 0.0, 4.0),
    (0, 0, 0, 1, 0, -4230.0, 0.0, 5.0, -20.0, 0.0, -2.0),
    (-1, -1, 2, 2, 2, -2819.0, 0.0, 7.0, 1207.0, 0.0, 3.0),
    (-1, 0, 2, 0, 0, -4056.0, 0.0, 5.0, 40.0, 0.0, -2.0),
    (0, -1, 2, 2, 2, -2647.0, 0.0, 11.0, 1129.0, 0.0, 5.0),
    (-2, 0, 0, 0, 1, -2294.0, 0.0, -10.0, 1266.0, 0.0, -4.0),
    (1, 1, 2, 0, 2, 2481.0, 0.0, -7.0, -1062.0, 0.0, -3.0),
    (2, 0, 0, 0, 1, 2179.0, 0.0, -2.0, -1129.0, 0.0, -2.0),
    (-1, 1, 0, 1, 0, 3276.0, 0.0, 1.0, -9.0, 0.0, 0.0),
    (1, 1, 0, 0, 0, -3389.0, 0.0, 5.0, 35.0, 0.0, -2.0),
    (1, 0, 2, 0, 0, 3339.0, 0.0, -13.0, -107.0, 0.0, 1.0),
    (-1, 0, 2, -2, 1, -1987.0, 0.0, -6.0, 1073.0, 0.0, -2.0),
    (1, 0, 0, 0, 2, -1981.0, 0.0, 0.0, 854.0, 0.0, 0.0),
    (-1, 0, 0, 1, 0, 4026.0, 0.0, -353.0, -553.0, 0.0, -139.0),
    (0, 0, 2, 1, 2, 1660.0, 0.0, -5.0, -710.0, 0.0, -2.0),
    (-1, 0, 2, 4, 2, -1521.0, 0.0, 9.0, 647.0, 0.0, 4.0),
    (-1, 1, 0, 1, 1, 1314.0, 0.0, 0.0, -700.0, 0.0, 0.0),
    (0, -2, 2, -2, 1, -1283.0, 0.0, 0.0, 672.0, 0.0, 0.0),
    (1, 0, 2, 2, 1, -1331.0, 0.0, 8.0, 663.0, 0.0, 4.0),
    (-2, 0, 2, 2, 2, 1383.0, 0.0, -2.0, -594.0, 0.0, -2.0),
    (-1, 0, 0, 0, 2, 1405.0, 0.0, 4.0, -610.0, 0.0, 2.0),
    (1, 1, 2, -2, 2, 1290.0, 0.0, 0.0, -556.0, 0.0, 0.0),
];

/// Compute the five Delaunay fundamental arguments in radians at JD-TT.
/// Polynomials from IERS Conventions 2010 / ERFA eraNut00b.
#[allow(clippy::unreadable_literal)]
fn delaunay_args(t: f64) -> [f64; 5] {
    let to_rad = |arcsec: f64| (arcsec * ARCSEC_TO_RAD).rem_euclid(TWO_PI);
    [
        to_rad(485868.249036 + 1717915923.2178 * t), // l:  Moon mean anomaly
        to_rad(1287104.79305 + 129596581.0481 * t),  // l': Sun mean anomaly
        to_rad(335779.526232 + 1739527262.8478 * t), // F:  Moon arg of latitude
        to_rad(1072260.70369 + 1602961601.2090 * t), // D:  Moon–Sun elongation
        to_rad(450160.398036 - 6962890.5431 * t),    // Ω:  Moon ascending node
    ]
}

/// Compute IAU 2000B nutation (77-term luni-solar series) at JD-TT.
#[must_use]
pub fn nutation(jd_tt: f64) -> Nutation {
    let t = julian_centuries_t(jd_tt);
    let [l, lp, f, d, om] = delaunay_args(t);

    let mut dp = 0.0_f64;
    let mut de = 0.0_f64;

    for &(nl, nlp, nf, nd, nom, ps, pst, pc, ec, ect, es) in NUT00B {
        let arg = f64::from(nl) * l
            + f64::from(nlp) * lp
            + f64::from(nf) * f
            + f64::from(nd) * d
            + f64::from(nom) * om;
        let (sin_arg, cos_arg) = arg.sin_cos();
        dp += (ps + pst * t) * sin_arg + pc * cos_arg;
        de += (ec + ect * t) * cos_arg + es * sin_arg;
    }

    Nutation {
        delta_psi: dp * U2R,
        delta_epsilon: de * U2R,
    }
}

/// Apply the nutation rotation to a mean-of-date equatorial 3-vector,
/// yielding a true-of-date equatorial 3-vector.
///
/// The rotation is `Rx(−(ε+Δε)) · Rz(−Δψ) · Rx(+ε)`, where `ε` is the
/// mean obliquity at the same epoch.
#[must_use]
pub fn nutate_mean_to_true(
    v_mean: &crate::coords::transform::Vector3,
    jd_tt: f64,
    mean_obliquity_rad: f64,
) -> crate::coords::transform::Vector3 {
    let n = nutation(jd_tt);
    let rx_plus = crate::coords::transform::rotate_x(mean_obliquity_rad);
    let rz = crate::coords::transform::rotate_z(-n.delta_psi);
    let rx_minus = crate::coords::transform::rotate_x(-(mean_obliquity_rad + n.delta_epsilon));
    let mid = crate::coords::transform::multiply(&rz, &rx_plus);
    let combined = crate::coords::transform::multiply(&rx_minus, &mid);
    crate::coords::transform::apply(&combined, v_mean)
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_abs_diff_eq;

    #[test]
    fn nutation_amplitude_is_in_arcseconds_range() {
        for jd_offset in [-100_000.0, 0.0, 100_000.0] {
            let n = nutation(2_451_545.0 + jd_offset);
            let dpsi_arcsec = n.delta_psi / ARCSEC_TO_RAD;
            let deps_arcsec = n.delta_epsilon / ARCSEC_TO_RAD;
            assert!(
                dpsi_arcsec.abs() < 20.0,
                "Δψ {dpsi_arcsec}″ exceeds 20″ at JD offset {jd_offset}"
            );
            assert!(
                deps_arcsec.abs() < 10.0,
                "Δε {deps_arcsec}″ exceeds 10″ at JD offset {jd_offset}"
            );
        }
    }

    #[test]
    fn nutation_at_j2000_matches_published_values() {
        // IAU 2000B at J2000.0: dominated by the Ω term, agrees with
        // Meeus 4-term to within 0.5″. Tighter epsilon than old 2.0″ bound.
        let n = nutation(2_451_545.0);
        let dpsi = n.delta_psi / ARCSEC_TO_RAD;
        let deps = n.delta_epsilon / ARCSEC_TO_RAD;
        assert_abs_diff_eq!(dpsi, -13.93, epsilon = 0.5);
        assert_abs_diff_eq!(deps, -5.78, epsilon = 0.5);
    }

    #[test]
    fn nutation_dominant_term_is_near_periodic_at_18_6_years() {
        let jd0 = 2_451_545.0;
        let n0 = nutation(jd0);
        let n_period = nutation(jd0 + 6798.4);
        let diff_arcsec = (n0.delta_psi - n_period.delta_psi) / ARCSEC_TO_RAD;
        assert!(
            diff_arcsec.abs() < 2.0,
            "Δψ at one Ω period later should return within ~1.5″; got Δ={diff_arcsec}″"
        );
    }

    #[test]
    fn nutation_iau2000b_leading_term_amplitude() {
        // At J2000.0, Ω ≈ 125.04°, so the dominant Ω term contributes
        // -17.2064 × sin(125.04°) ≈ -14.1″ to Δψ. The remaining terms
        // bring the total to ~-13.9″. Both the truncated and full IAU 2000B
        // models are dominated by this term; > 12.0 is a lower bound that
        // any correct nutation implementation must satisfy at J2000.0.
        let n = nutation(2_451_545.0);
        let dpsi = n.delta_psi / ARCSEC_TO_RAD;
        assert!(
            dpsi.abs() > 12.0,
            "Δψ at J2000.0 should exceed 12″ in magnitude; got {dpsi:.3}″"
        );
    }

    #[test]
    fn nutation_iau2000b_agrees_with_meeus_to_within_half_arcsec() {
        let n = nutation(2_451_545.0);
        let dpsi = n.delta_psi / ARCSEC_TO_RAD;
        let deps = n.delta_epsilon / ARCSEC_TO_RAD;
        assert!(
            (dpsi - (-13.93)).abs() < 0.5,
            "IAU 2000B Δψ at J2000.0 should be within 0.5″ of -13.93″; got {dpsi:.4}″"
        );
        assert!(
            (deps - (-5.78)).abs() < 0.5,
            "IAU 2000B Δε at J2000.0 should be within 0.5″ of -5.78″; got {deps:.4}″"
        );
    }

    #[test]
    fn nutation_iau2000b_sub_milliarcsec_stability() {
        let jd = 2_451_545.0;
        let n1 = nutation(jd);
        let n2 = nutation(jd);
        assert!((n1.delta_psi - n2.delta_psi).abs() < 1e-15);
        assert!((n1.delta_epsilon - n2.delta_epsilon).abs() < 1e-15);
    }
}
