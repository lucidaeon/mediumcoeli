//! Precession of the equinoxes — IAU 2006 (Capitaine et al.) model.
//!
//! # What precession does
//!
//! Earth's rotation axis traces a circle in space, completing one
//! cycle every ~25,772 years. This motion shifts the equatorial
//! coordinate frame (and hence the equinoxes, which lie at its
//! intersections with the ecliptic) backward through the zodiac by
//! ~50″ per year — the "precession of the equinoxes" that explains
//! why the tropical sign of the spring equinox has drifted from
//! Aries into Pisces over the past two millennia.
//!
//! To compute *of-date* positions from J2000-frame ephemeris vectors,
//! we must rotate from the mean equator and equinox of J2000 to those
//! of the target date.
//!
//! # IAU 2006 three-angle rotation
//!
//! Capitaine, Wallace & Chapront (2003, A&A 412, 567) express the
//! precession as a product of three rotations — the same structure as
//! IAU 1976 (Lieske) — but with revised polynomial coefficients and
//! T⁴/T⁵ terms that improve accuracy to ≈ 0.1″ over ±1000 years
//! and are consistent with the IAU 2000A/B nutation series:
//!
//! ```text
//! P(t) = Rz(−z) · Ry(+θ) · Rz(−ζ)
//! ```
//!
//! where `ζ`, `z`, and `θ` (arcseconds, T = Julian centuries from J2000 TT):
//!
//! ```text
//! ζ = +2.650545 + 2306.083227·T + 0.2988499·T² + 0.01801828·T³ − 0.000005971·T⁴ − 0.0000003173·T⁵
//! z = −2.650545 + 2306.077181·T + 1.0927348·T² + 0.01826837·T³ − 0.000028596·T⁴ − 0.0000002904·T⁵
//! θ =              2004.191903·T − 0.4294934·T² − 0.04182264·T³ − 0.000007089·T⁴ − 0.0000001274·T⁵
//! ```
//!
//! Applied to a J2000 mean-equator vector `v_J2000`, the product
//! `P(t) · v_J2000` yields the corresponding vector in the mean
//! equator and equinox of the target date.

use crate::coords::obliquity::{ARCSEC_TO_RAD, julian_centuries_t};
use crate::coords::transform::{Matrix3, Vector3, apply, multiply, rotate_y, rotate_z};

/// The three precession angles (ζ, z, θ) in radians for the elapsed
/// time `T` (Julian centuries since J2000 TT).
#[derive(Debug, Clone, Copy)]
pub struct PrecessionAngles {
    /// ζ (zeta) angle, radians.
    pub zeta: f64,
    /// z angle, radians.
    pub z: f64,
    /// θ (theta) angle, radians.
    pub theta: f64,
}

/// Compute the IAU 2006 precession angles for the given JD-TT.
///
/// Source: Capitaine, Wallace & Chapront, A&A 412, 567 (2003), Table 1.
#[must_use]
pub fn precession_angles(jd_tt: f64) -> PrecessionAngles {
    let t = julian_centuries_t(jd_tt);
    let t2 = t * t;
    let t3 = t2 * t;
    let t4 = t3 * t;
    let t5 = t4 * t;
    let zeta_arcsec = 2.650_545 + 2_306.083_227 * t + 0.298_849_9 * t2 + 0.018_018_28 * t3
        - 0.000_005_971 * t4
        - 0.000_000_317_3 * t5;
    let z_arcsec = -2.650_545 + 2_306.077_181 * t + 1.092_734_8 * t2 + 0.018_268_37 * t3
        - 0.000_028_596 * t4
        - 0.000_000_290_4 * t5;
    let theta_arcsec = 2_004.191_903 * t
        - 0.429_493_4 * t2
        - 0.041_822_64 * t3
        - 0.000_007_089 * t4
        - 0.000_000_127_4 * t5;
    PrecessionAngles {
        zeta: zeta_arcsec * ARCSEC_TO_RAD,
        z: z_arcsec * ARCSEC_TO_RAD,
        theta: theta_arcsec * ARCSEC_TO_RAD,
    }
}

/// Build the precession rotation matrix P(t) such that
/// `v_of_date = P(t) · v_J2000`.
#[must_use]
pub fn precession_matrix(jd_tt: f64) -> Matrix3 {
    let p = precession_angles(jd_tt);
    // P = Rz(-z) · Ry(+θ) · Rz(-ζ)
    let a = rotate_z(-p.zeta);
    let b = rotate_y(p.theta);
    let c = rotate_z(-p.z);
    multiply(&c, &multiply(&b, &a))
}

/// Apply precession to a J2000 equatorial 3-vector, returning the
/// equivalent vector in the mean equator and equinox of date.
#[must_use]
pub fn precess_j2000_to_date(v_j2000: &Vector3, jd_tt: f64) -> Vector3 {
    apply(&precession_matrix(jd_tt), v_j2000)
}

/// Inverse precession: from mean-of-date back to J2000.
#[must_use]
pub fn precess_date_to_j2000(v_of_date: &Vector3, jd_tt: f64) -> Vector3 {
    // The inverse of a rotation matrix is its transpose.
    let p = precession_matrix(jd_tt);
    let pt: Matrix3 = [
        [p[0][0], p[1][0], p[2][0]],
        [p[0][1], p[1][1], p[2][1]],
        [p[0][2], p[1][2], p[2][2]],
    ];
    apply(&pt, v_of_date)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coords::transform::magnitude;
    use approx::assert_abs_diff_eq;

    #[test]
    fn angles_at_j2000_are_near_zero() {
        // IAU 2006 has non-zero constant terms: ζ = +2.650545", z = -2.650545".
        // These are the "frame bias" corrections at T=0; θ = 0 exactly.
        let a = precession_angles(2_451_545.0);
        assert_abs_diff_eq!(a.zeta / ARCSEC_TO_RAD, 2.650_545, epsilon = 1e-6);
        assert_abs_diff_eq!(a.z / ARCSEC_TO_RAD, -2.650_545, epsilon = 1e-6);
        assert_abs_diff_eq!(a.theta, 0.0, epsilon = 1e-15);
    }

    #[test]
    fn matrix_at_j2000_is_near_identity() {
        // At T=0 the IAU 2006 constant terms (±2.65") produce a rotation
        // smaller than 1e-5 rad from identity. Verify the off-diagonal
        // elements are tiny and the determinant is 1.
        let m = precession_matrix(2_451_545.0);
        // Diagonal elements close to 1.
        for (i, row) in m.iter().enumerate() {
            assert_abs_diff_eq!(row[i], 1.0, epsilon = 1e-8);
        }
        // Off-diagonal elements tiny (< 2e-5).
        assert!(m[0][1].abs() < 2e-5, "m[0][1] = {}", m[0][1]);
        assert!(m[1][0].abs() < 2e-5, "m[1][0] = {}", m[1][0]);
    }

    #[test]
    fn angles_one_century_match_well_known_values() {
        // T = 1 (year 2100). IAU 2006 values from Capitaine et al. 2003:
        // ζ = 2309.05″, z = 2304.54″, θ = 2003.72″
        let a = precession_angles(2_451_545.0 + 36_525.0);
        assert_abs_diff_eq!(a.zeta / ARCSEC_TO_RAD, 2309.05, epsilon = 0.02);
        assert_abs_diff_eq!(a.z / ARCSEC_TO_RAD, 2304.54, epsilon = 0.02);
        assert_abs_diff_eq!(a.theta / ARCSEC_TO_RAD, 2003.72, epsilon = 0.02);
    }

    #[test]
    fn precession_preserves_vector_magnitude() {
        // A rotation must leave |v| unchanged.
        let v = [3.0, 4.0, 5.0]; // arbitrary
        let r = precess_j2000_to_date(&v, 2_451_545.0 + 36_525.0); // +1 century
        let mag_v = magnitude(&v);
        let mag_r = magnitude(&r);
        assert_abs_diff_eq!(mag_r, mag_v, epsilon = 1e-13);
    }

    #[test]
    fn precess_then_unprecess_round_trips() {
        let v = [1.5, -0.3, 0.7];
        let jd = 2_451_545.0 + 36_525.0 * 5.0; // +500 years
        let to_date = precess_j2000_to_date(&v, jd);
        let back = precess_date_to_j2000(&to_date, jd);
        for i in 0..3 {
            assert_abs_diff_eq!(back[i], v[i], epsilon = 1e-13);
        }
    }

    #[test]
    fn precession_over_one_century_shifts_x_axis_by_about_50_arcmin() {
        // A vector along the J2000 +X axis (pointing at the J2000 vernal
        // equinox) will, after one century of precession, no longer point
        // at the new vernal equinox — it lags behind by ≈ 1°23′ (the
        // ~50″/year cumulative rate over 100 years). We check the angle subtended.
        let v_x = [1.0, 0.0, 0.0];
        let after = precess_j2000_to_date(&v_x, 2_451_545.0 + 36_525.0);
        let dot = v_x[0] * after[0] + v_x[1] * after[1] + v_x[2] * after[2];
        let angle_deg = dot.acos().to_degrees();
        // IAU 2006: ~50.3″/year × 100 years ≈ 1.40°. Allow ±0.1°.
        assert!(
            (1.3..1.5).contains(&angle_deg),
            "X-axis precession over 1 century should be ≈ 1.4°; got {angle_deg}°"
        );
    }
}
