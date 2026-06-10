#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::similar_names,
    clippy::too_many_arguments,
    clippy::manual_midpoint,
    clippy::doc_markdown,
    clippy::missing_panics_doc
)]

//! Placidus house system.
//!
//! Trisects the diurnal and nocturnal semi-arcs. Each intermediate cusp is
//! found by bisection within its bounding arc (MC→ASC for houses 11/12,
//! ASC→IC for houses 2/3). Bisection is more robust than simple iteration
//! for high-latitude charts where the iteration can jump to the wrong root.
//!
//! Returns `None` for `|lat| ≥ 90°` and when any cusp computation fails
//! (circumpolar ecliptic degrees at extreme latitudes, roughly above 66°).

use super::HouseCusps;
use crate::coords::acds::ac_rad;
use crate::coords::mcic::{ic_rad, mc_rad};
use std::f64::consts::{FRAC_PI_2, PI, TAU};

/// Evaluate the Placidus cusp function for bisection.
///
/// Let `lst_rise(λ) = RA(λ) − DSA(λ)` be the local sidereal time at which
/// the ecliptic point λ crosses the eastern horizon (where
/// `DSA = π/2 + AD`, `AD = arcsin(tan φ · tan δ)`).
///
/// Upper hemisphere (houses 11, 12):  f = (RAMC − lst_rise) mod 2π − k·DSA
/// Lower hemisphere (houses  2,  3):  f = (lst_rise − RAMC) mod 2π − k·NSA
///
/// Zero of f puts the body at fraction k of its DSA above the horizon
/// (upper) or k of its NSA before rising (lower).
fn cusp_fn(
    lon: f64,
    ramc: f64,
    eps_cos: f64,
    eps_sin: f64,
    lat_tan: f64,
    k: f64,
    upper: bool,
) -> Option<f64> {
    let sin_lon = lon.sin();
    let cos_lon = lon.cos();
    let sin_delta = (eps_sin * sin_lon).clamp(-1.0, 1.0);
    let delta = sin_delta.asin();
    let ad_sin = lat_tan * delta.tan();
    if ad_sin.abs() >= 1.0 {
        return None; // circumpolar: this ecliptic degree never rises or sets
    }
    let ad = ad_sin.asin();
    let dsa = FRAC_PI_2 + ad;
    let nsa = FRAC_PI_2 - ad;
    let ra = f64::atan2(eps_cos * sin_lon, cos_lon).rem_euclid(TAU);
    let lst_rise = ra - dsa;
    // Signed normalisation to (−π, π]: ASC sits exactly at the boundary where
    // RAMC − lst_rise == 0; rem_euclid(TAU) alone would flip tiny negatives
    // to ~2π and break sign-tracking in bisection.
    let signed = |x: f64| (x + PI).rem_euclid(TAU) - PI;
    Some(if upper {
        signed(ramc - lst_rise) - k * dsa
    } else {
        signed(lst_rise - ramc) - k * nsa
    })
}

/// Bisect for a Placidus cusp within the arc [lo, hi] (counterclockwise).
///
/// The function `cusp_fn` is assumed to change sign monotonically across
/// the arc; bisection finds the zero to sub-arcsecond precision.
fn bisect_cusp(
    ramc: f64,
    eps_cos: f64,
    eps_sin: f64,
    lat_tan: f64,
    lo: f64,
    hi: f64,
    k: f64,
    upper: bool,
) -> Option<f64> {
    let span = (hi - lo).rem_euclid(TAU); // arc length, counterclockwise
    let f_lo = cusp_fn(lo, ramc, eps_cos, eps_sin, lat_tan, k, upper)?;
    let f_hi = cusp_fn(hi, ramc, eps_cos, eps_sin, lat_tan, k, upper)?;

    // Work in offset-from-lo space to avoid wrap-around arithmetic.
    let mut a = 0.0_f64;
    let mut b = span;
    let mut fa = f_lo;

    // If the bracket doesn't straddle zero the chart has degenerate cusps.
    if f_lo * f_hi > 0.0 {
        return None;
    }

    for _ in 0..60 {
        let mid = (a + b) / 2.0;
        let mid_lon = (lo + mid).rem_euclid(TAU);
        let f_mid = cusp_fn(mid_lon, ramc, eps_cos, eps_sin, lat_tan, k, upper)?;
        if f_mid.abs() < 1e-11 || (b - a) < 1e-12 {
            return Some(mid_lon);
        }
        if fa * f_mid <= 0.0 {
            b = mid;
        } else {
            a = mid;
            fa = f_mid;
        }
    }
    Some((lo + (a + b) / 2.0).rem_euclid(TAU))
}

/// Placidus house cusps.
///
/// Returns `None` at polar latitudes or when any cusp is undefined.
#[must_use]
pub fn placidus_rad(ramc: f64, obliquity_rad: f64, lat_rad: f64) -> Option<HouseCusps> {
    if lat_rad.abs() >= FRAC_PI_2 {
        return None;
    }
    let eps_cos = obliquity_rad.cos();
    let eps_sin = obliquity_rad.sin();
    let lat_tan = lat_rad.tan();

    let mc = mc_rad(ramc, obliquity_rad);
    let ac = ac_rad(ramc, obliquity_rad, lat_rad)?;
    let ic = ic_rad(mc);
    let ds = (ac + PI).rem_euclid(TAU);

    // Upper eastern arc: MC → ASC (counterclockwise)
    let h11 = bisect_cusp(ramc, eps_cos, eps_sin, lat_tan, mc, ac, 2.0 / 3.0, true)?;
    let h12 = bisect_cusp(ramc, eps_cos, eps_sin, lat_tan, mc, ac, 1.0 / 3.0, true)?;

    // Lower eastern arc: ASC → IC (counterclockwise)
    let h2 = bisect_cusp(ramc, eps_cos, eps_sin, lat_tan, ac, ic, 1.0 / 3.0, false)?;
    let h3 = bisect_cusp(ramc, eps_cos, eps_sin, lat_tan, ac, ic, 2.0 / 3.0, false)?;

    // Opposite cusps
    let h5 = (h11 + PI).rem_euclid(TAU);
    let h6 = (h12 + PI).rem_euclid(TAU);
    let h8 = (h2 + PI).rem_euclid(TAU);
    let h9 = (h3 + PI).rem_euclid(TAU);

    Some(HouseCusps([
        ac, h2, h3, ic, h5, h6, ds, h8, h9, mc, h11, h12,
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_abs_diff_eq;

    const EPS_DEG: f64 = 1.0 / 6.0; // 10 arcminutes

    // ── Structural invariants ────────────────────────────────────────────────

    #[test]
    fn opposite_cusps_are_180_apart() {
        let ramc = 243.0_f64.to_radians();
        let eps = 0.409_092_62_f64;
        let lat = 34.14_f64.to_radians();
        let hc = placidus_rad(ramc, eps, lat).unwrap();
        for (a, b) in [(1, 7), (2, 8), (3, 9), (4, 10), (5, 11), (6, 12)] {
            let diff = (hc.cusp(b) - hc.cusp(a)).rem_euclid(TAU);
            assert_abs_diff_eq!(diff.to_degrees(), 180.0, epsilon = 1e-6);
        }
    }

    #[test]
    fn polar_returns_none() {
        let eps = 0.409_092_62_f64;
        assert!(placidus_rad(0.0, eps, FRAC_PI_2).is_none());
        assert!(placidus_rad(0.0, eps, -FRAC_PI_2).is_none());
    }

    #[test]
    fn h1_is_asc_h4_is_ic_h7_is_dsc_h10_is_mc() {
        let ramc = 243.0_f64.to_radians();
        let eps = 0.409_092_62_f64;
        let lat = 34.14_f64.to_radians();
        let hc = placidus_rad(ramc, eps, lat).unwrap();
        let ac = ac_rad(ramc, eps, lat).unwrap();
        let mc = mc_rad(ramc, eps);
        assert_abs_diff_eq!(hc.cusp(1), ac, epsilon = 1e-10);
        assert_abs_diff_eq!(hc.cusp(10), mc, epsilon = 1e-10);
        assert_abs_diff_eq!(hc.cusp(4), ic_rad(mc), epsilon = 1e-10);
        assert_abs_diff_eq!(hc.cusp(7), (ac + PI).rem_euclid(TAU), epsilon = 1e-10);
    }

    // ── Equatorial degenerate case ───────────────────────────────────────────
    // At lat = 0°: AD = 0 everywhere, DSA = NSA = π/2.
    // Upper:  RAMC − lst_rise = k·DSA → RA = RAMC + (1−k)·DSA + 0·DSA … work out:
    //   lst_rise = RA − DSA, so RA = lst_rise + DSA = (RAMC − k·DSA) + DSA = RAMC + (1−k)·DSA.
    //   H11 (k = 2/3): RA = RAMC + 30°.
    //   H12 (k = 1/3): RA = RAMC + 60°.
    // Lower:  lst_rise − RAMC = k·NSA → RA = RAMC + k·NSA + DSA.
    //   H2  (k = 1/3): RA = RAMC + 30° + 90° = RAMC + 120°.
    //   H3  (k = 2/3): RA = RAMC + 60° + 90° = RAMC + 150°.
    // λ from RA: atan2(sin(RA), cos(RA)·cos(ε)).

    fn equatorial_expected(ramc: f64, ra_offset_deg: f64, eps_cos: f64) -> f64 {
        let ra = (ramc + ra_offset_deg.to_radians()).rem_euclid(TAU);
        f64::atan2(ra.sin(), ra.cos() * eps_cos).rem_euclid(TAU)
    }

    #[test]
    fn equatorial_intermediate_cusps() {
        let ramc = 120.0_f64.to_radians();
        let eps = 0.409_092_62_f64;
        let eps_cos = eps.cos();
        let hc = placidus_rad(ramc, eps, 0.0).unwrap();

        let offsets: [(u8, f64); 4] = [(11, 30.0), (12, 60.0), (2, 120.0), (3, 150.0)];
        for (house, off) in offsets {
            let expected = equatorial_expected(ramc, off, eps_cos)
                .to_degrees()
                .rem_euclid(360.0);
            let got = hc.cusp(house).to_degrees().rem_euclid(360.0);
            assert_abs_diff_eq!(got, expected, epsilon = 1e-5);
        }
    }

    // ── Reference chart — UNIX 2038 ─────────────────────────────────────────
    // 2038-01-19 03:14:07 UT, London 51°30'N 000°10'W.
    // ASC and MC verified against refchart output.

    fn unix_2038_placidus() -> HouseCusps {
        use crate::coords::nutation::nutation;
        use crate::coords::obliquity::mean_obliquity_rad;
        use crate::coords::sidereal_time::gast_rad;
        use crate::time::delta_t::jd_ut_to_jd_tt;

        // Unix 32-bit overflow: 2^31 − 1 seconds after 1970-01-01 00:00 UT.
        let jd_ut = 2_440_587.5 + 2_147_483_647.0_f64 / 86400.0;
        let jd_tt = jd_ut_to_jd_tt(jd_ut);
        let lon_east = -(10.0 / 60.0_f64); // 000°10'W
        let lat = (51.0 + 30.0 / 60.0_f64).to_radians();
        let ramc = (gast_rad(jd_tt) + lon_east.to_radians()).rem_euclid(TAU);
        let nut = nutation(jd_tt);
        let obliquity = mean_obliquity_rad(jd_tt) + nut.delta_epsilon;
        placidus_rad(ramc, obliquity, lat).expect("Placidus should succeed for London lat")
    }

    #[test]
    fn unix_2038_asc_matches_refchart() {
        // Sco⌖24°03'09" = 234.053° (placidus_rad H1 equals ac_rad)
        let hc = unix_2038_placidus();
        let expected = 210.0 + 24.0 + 3.0 / 60.0 + 9.0 / 3600.0_f64;
        assert_abs_diff_eq!(hc.cusp(1).to_degrees(), expected, epsilon = EPS_DEG);
    }

    #[test]
    fn unix_2038_mc_matches_refchart() {
        // Vir⌖15°51'55" = 165.865° (placidus_rad H10 equals mc_rad)
        let hc = unix_2038_placidus();
        let expected = 150.0 + 15.0 + 51.0 / 60.0 + 55.0 / 3600.0_f64;
        assert_abs_diff_eq!(hc.cusp(10).to_degrees(), expected, epsilon = EPS_DEG);
    }

    #[test]
    fn unix_2038_cusps_are_ordered_and_partition_circle() {
        let hc = unix_2038_placidus();
        let cusps = hc.0;
        let mut total_span = 0.0_f64;
        for i in 0..12 {
            let start = cusps[i];
            let end = cusps[(i + 1) % 12];
            let span = (end - start).rem_euclid(TAU).to_degrees();
            assert!(
                span > 0.0 && span < 180.0,
                "H{} span {:.3}° out of range (start={:.3}°, end={:.3}°)",
                i + 1,
                span,
                start.to_degrees(),
                end.to_degrees()
            );
            total_span += span;
        }
        assert_abs_diff_eq!(total_span, 360.0, epsilon = 1e-6);
    }

    #[test]
    fn unix_2038_angles_in_correct_houses() {
        let hc = unix_2038_placidus();
        let ac = hc.cusp(1);
        let mc = hc.cusp(10);
        let ic = hc.cusp(4);
        let tiny = 0.001_f64.to_radians();
        assert_eq!(hc.house_of((ac + tiny).rem_euclid(TAU)), 1);
        assert_eq!(hc.house_of((mc + tiny).rem_euclid(TAU)), 10);
        assert_eq!(hc.house_of((ic + tiny).rem_euclid(TAU)), 4);
    }
}
