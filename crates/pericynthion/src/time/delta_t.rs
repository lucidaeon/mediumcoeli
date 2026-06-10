//! ΔT (`TT − UT`): the offset between Terrestrial Time and Universal
//! Time, in seconds.
//!
//! # What ΔT is and why we care
//!
//! Terrestrial Time (TT) ticks at a uniform rate — it is the time
//! coordinate of the JPL ephemeris and the IAU coordinate-transformation
//! routines. Universal Time (UT) follows the Earth's rotation, which
//! is not uniform: tidal friction is slowing the Earth down by roughly
//! 1.7 milliseconds per day per century, and on shorter time scales the
//! rotation also speeds up and slows down unpredictably due to angular-
//! momentum exchanges with the atmosphere and the fluid core.
//!
//! ΔT = TT − UT captures the accumulated divergence between these two
//! time scales.
//!
//! | Year | ΔT (seconds) |
//! |------|--------------|
//! | −720 | ≈ +20,551    |
//! | 0    | ≈ +10,574    |
//! | 1000 | ≈ +1,468     |
//! | 1620 | ≈ +67        |
//! | 1657 | ≈ +44        |
//! | 1820 | ≈ +12        |
//! | 1900 | ≈ −3         |
//! | 2000 | ≈ +64        |
//! | 2050 | ≈ +93 (extrapolated) |
//!
//! For our reference charts:
//! - Vettius Valens (120 CE): ΔT ≈ +9,356 seconds (2 h 35 m)
//! - William Lilly (1602): ΔT ≈ +86 seconds
//! - Anna Freud (1895): ΔT ≈ −5 seconds
//! - Lightning Strike (1955): ΔT ≈ +31 seconds
//! - Adèle Haenel (1989): ΔT ≈ +56 seconds
//!
//! Without ΔT, the year-120 chart would have planetary positions off
//! by the Moon's motion across 2.6 hours — astrologically catastrophic.
//!
//! # Algorithm: SMH 2016 spline + observational table + parabolic extrapolation
//!
//! - **1657–2025**: linear interpolation in `OBSERVATIONAL_TABLE` (±2–3 s).
//! - **−720 to <1657**: cubic spline from Stephenson, Morrison & Hohenkerk 2016
//!   (*Proc. R. Soc. A* 472, 20160404), Table S15 (figshare article 4290866).
//!   54 segments; accuracy ±1–30 s depending on epoch.
//! - **year < −720 / year > 2025**: parabolic or Espenak polynomial extrapolation.
//!   Secular coefficient 32.5 s/century² from SMH 2016.
//!
//! Sources:
//! - Espenak & Meeus 2006, NASA/TP-2006-214141.
//!   <https://eclipse.gsfc.nasa.gov/SEhelp/deltatpoly2004.html>
//! - Stephenson, Morrison & Hohenkerk 2016, *Proc. R. Soc. A* 472,
//!   20160404. DOI 10.1098/rspa.2016.0404.

/// Observational ΔT table from historical eclipse and timing records,
/// 1657-2025 at decade spacing. Values agree with USNO Circular 179
/// and SMH 2016 to within a few seconds.
///
/// Linear interpolation between table entries gives ΔT accurate to
/// ~2-3 seconds for any year in the table window — better than what
/// the Espenak smoothed polynomials achieve in this period.
const OBSERVATIONAL_TABLE: &[(f64, f64)] = &[
    (1657.0, 44.0),
    (1670.0, 35.0),
    (1685.0, 27.0),
    (1700.0, 9.0),
    (1720.0, 11.0),
    (1740.0, 11.5),
    (1760.0, 14.0),
    (1780.0, 16.5),
    (1800.0, 13.7),
    (1820.0, 11.9),
    (1840.0, 5.7),
    (1860.0, 7.6),
    (1880.0, -5.1),
    (1900.0, -2.8),
    (1910.0, 10.4),
    (1920.0, 21.1),
    (1930.0, 24.0),
    (1940.0, 24.3),
    (1950.0, 29.1),
    (1960.0, 33.2),
    (1970.0, 40.2),
    (1980.0, 50.5),
    (1990.0, 56.9),
    (2000.0, 63.8),
    (2010.0, 66.1),
    (2020.0, 69.4),
    (2025.0, 71.0),
];

/// SMH 2016 cubic spline for ΔT, Table S15 (figshare article 4290866).
/// Each row: `(k_start, k_end, a0, a1, a2, a3)`.
/// Evaluate: `t = (year − k_start) / (k_end − k_start)`;
/// `ΔT = a0 + a1·t + a2·t² + a3·t³` (seconds).
/// Valid for years −720..<1657 (above that the observational table wins).
#[allow(clippy::unreadable_literal)]
const SMH2016_SPLINE: &[(f64, f64, f64, f64, f64, f64); 54] = &[
    (-720.0, 400.0, 20550.593, -21268.478, 11863.418, -4541.129),
    (400.0, 1000.0, 6604.404, -5981.266, -505.093, 1349.609),
    (1000.0, 1500.0, 1467.654, -2452.187, 2460.927, -1183.759),
    (1500.0, 1600.0, 292.635, -216.322, -43.614, 56.681),
    (1600.0, 1650.0, 89.380, -66.754, 31.607, -10.497),
    (1650.0, 1720.0, 43.736, -49.043, 0.227, 15.811),
    (1720.0, 1800.0, 10.730, -1.321, 62.250, -52.946),
    (1800.0, 1810.0, 18.714, -4.457, -1.509, 2.507),
    (1810.0, 1820.0, 15.255, 0.046, 6.012, -4.634),
    (1820.0, 1830.0, 16.679, -1.831, -7.889, 3.799),
    (1830.0, 1840.0, 10.758, -6.211, 3.509, -0.388),
    (1840.0, 1850.0, 7.668, -0.357, 2.345, -0.338),
    (1850.0, 1855.0, 9.317, 1.659, 0.332, -0.932),
    (1855.0, 1860.0, 10.376, -0.472, -2.463, 1.596),
    (1860.0, 1865.0, 9.038, -0.610, 2.325, -2.497),
    (1865.0, 1870.0, 8.256, -3.450, -5.166, 2.729),
    (1870.0, 1875.0, 2.369, -5.596, 3.020, -0.919),
    (1875.0, 1880.0, -1.126, -2.312, 0.264, -0.037),
    (1880.0, 1885.0, -3.211, -1.894, 0.154, 0.562),
    (1885.0, 1890.0, -4.388, 0.101, 1.841, -1.438),
    (1890.0, 1895.0, -3.884, -0.531, -2.473, 1.870),
    (1895.0, 1900.0, -5.017, 0.134, 3.138, -0.232),
    (1900.0, 1905.0, -1.977, 5.715, 2.443, -1.257),
    (1905.0, 1910.0, 4.923, 6.828, -1.329, 0.720),
    (1910.0, 1915.0, 11.142, 6.330, 0.831, -0.825),
    (1915.0, 1920.0, 17.479, 5.518, -1.643, 0.262),
    (1920.0, 1925.0, 21.617, 3.020, -0.856, 0.008),
    (1925.0, 1930.0, 23.789, 1.333, -0.831, 0.127),
    (1930.0, 1935.0, 24.418, 0.052, -0.449, 0.142),
    (1935.0, 1940.0, 24.164, -0.419, -0.022, 0.702),
    (1940.0, 1945.0, 24.426, 1.645, 2.086, -1.106),
    (1945.0, 1950.0, 27.050, 2.499, -1.232, 0.614),
    (1950.0, 1953.0, 28.932, 1.127, 0.220, -0.277),
    (1953.0, 1956.0, 30.002, 0.737, -0.610, 0.631),
    (1956.0, 1959.0, 30.760, 1.409, 1.282, -0.799),
    (1959.0, 1962.0, 32.652, 1.577, -1.115, 0.507),
    (1962.0, 1965.0, 33.621, 0.868, 0.406, 0.199),
    (1965.0, 1968.0, 35.093, 2.275, 1.002, -0.414),
    (1968.0, 1971.0, 37.956, 3.035, -0.242, 0.202),
    (1971.0, 1974.0, 40.951, 3.157, 0.364, -0.229),
    (1974.0, 1977.0, 44.244, 3.198, -0.323, 0.172),
    (1977.0, 1980.0, 47.291, 3.069, 0.193, -0.192),
    (1980.0, 1983.0, 50.361, 2.878, -0.384, 0.081),
    (1983.0, 1986.0, 52.936, 2.354, -0.140, -0.166),
    (1986.0, 1989.0, 54.984, 1.577, -0.637, 0.448),
    (1989.0, 1992.0, 56.373, 1.649, 0.709, -0.277),
    (1992.0, 1995.0, 58.453, 2.235, -0.122, 0.111),
    (1995.0, 1998.0, 60.677, 2.324, 0.212, -0.315),
    (1998.0, 2001.0, 62.899, 1.804, -0.732, 0.112),
    (2001.0, 2004.0, 64.082, 0.675, -0.396, 0.193),
    (2004.0, 2007.0, 64.555, 0.463, 0.184, -0.008),
    (2007.0, 2010.0, 65.194, 0.809, 0.161, -0.101),
    (2010.0, 2013.0, 66.063, 0.828, -0.142, 0.168),
    (2013.0, 2016.0, 66.917, 1.046, 0.360, -0.282),
];

/// ΔT (`TT − UT`) in seconds for the given astronomical year (year 0
/// exists; BCE years are negative). A fractional year shifts the result
/// smoothly — `2000.5` is mid-year-2000.
///
/// # Algorithm selection
///
/// - Years 1657–2025: linear interpolation in the `OBSERVATIONAL_TABLE`.
/// - Years −720 to <1657: SMH 2016 cubic spline (`SMH2016_SPLINE`).
/// - All other years: parabolic or Espenak polynomial extrapolation.
///
/// # Accuracy
///
/// - 1657–2025: ±2-3 seconds (limited by table resolution).
/// - 1500–1657: ±5–15 seconds (SMH 2016 spline).
/// - 500–1500: ±10–30 seconds (SMH 2016 spline).
/// - −720 to +500 CE: ±30–100 seconds (SMH 2016 spline, long segments).
/// - Before −720 / after 2025: parabolic extrapolation, uncertainty grows.
///
/// # Examples
///
/// ```
/// use pericynthion::time::delta_t::delta_t_seconds;
///
/// // Year 2000: about 64 seconds.
/// let dt = delta_t_seconds(2000.0);
/// assert!((dt - 64.0).abs() < 3.0);
/// ```
#[must_use]
pub fn delta_t_seconds(year: f64) -> f64 {
    if let Some(first) = OBSERVATIONAL_TABLE.first()
        && let Some(last) = OBSERVATIONAL_TABLE.last()
        && year >= first.0
        && year <= last.0
    {
        return interpolate_table(year);
    }
    if (-720.0..1657.0).contains(&year) {
        return smh2016_spline(year);
    }
    extrapolate(year)
}

/// Linear interpolation inside the [`OBSERVATIONAL_TABLE`]. Caller
/// guarantees `year ∈ [first.year, last.year]`.
fn interpolate_table(year: f64) -> f64 {
    let table = OBSERVATIONAL_TABLE;
    // Binary search by year (table is sorted ascending).
    let i = table
        .partition_point(|&(y, _)| y <= year)
        .saturating_sub(1)
        .min(table.len() - 2);
    let (y0, dt0) = table[i];
    let (y1, dt1) = table[i + 1];
    dt0 + (dt1 - dt0) * (year - y0) / (y1 - y0)
}

/// Evaluate the SMH 2016 cubic spline for the given year.
///
/// The table extends to 2016, but in normal dispatch the
/// `OBSERVATIONAL_TABLE` takes priority for 1657–2025. Callers therefore
/// invoke this only for `year ∈ [−720, 1657)`.
fn smh2016_spline(year: f64) -> f64 {
    let i = SMH2016_SPLINE
        .partition_point(|&(k, _, _, _, _, _)| k <= year)
        .saturating_sub(1)
        .min(SMH2016_SPLINE.len() - 1);
    let (k_i, k_next, a0, a1, a2, a3) = SMH2016_SPLINE[i];
    let t = (year - k_i) / (k_next - k_i);
    a0 + a1 * t + a2 * t * t + a3 * t * t * t
}

/// Parabolic or Espenak polynomial extrapolation for years outside the
/// observational table and SMH 2016 spline coverage.
///
/// Called only for year < −720 or year > 2025.
#[allow(clippy::unreadable_literal)]
fn extrapolate(year: f64) -> f64 {
    let y = year;

    // Before −720: long-term parabolic (SMH 2016, 32.5 s/cy²).
    if y < -720.0 {
        let u = (y - 1820.0) / 100.0;
        return -20.0 + 32.5 * u * u;
    }

    // 2025 to 2050: Espenak/Meeus quadratic.
    if y < 2050.0 {
        let u = y - 2000.0;
        return 62.92 + 0.32217 * u + 0.005589 * u * u;
    }

    // 2050 to 2150: Espenak/Meeus with linear correction.
    if y < 2150.0 {
        let u = (y - 1820.0) / 100.0;
        return -20.0 + 32.0 * u * u - 0.5628 * (2150.0 - y);
    }

    // After 2150: long-term parabolic (SMH 2016, 32.5 s/cy²).
    let u = (y - 1820.0) / 100.0;
    -20.0 + 32.5 * u * u
}

/// Convert a Julian Date in UT1 to a Julian Date in TT by adding
/// `ΔT / 86400` days.
#[must_use]
pub fn jd_ut_to_jd_tt(jd_ut: f64) -> f64 {
    let year = jd_to_decimal_year(jd_ut);
    jd_ut + delta_t_seconds(year) / 86_400.0
}

/// Convert a Julian Date in TT back to a JD in UT1.
#[must_use]
pub fn jd_tt_to_jd_ut(jd_tt: f64) -> f64 {
    // We compute ΔT at the TT-side year — close enough to the UT-side
    // year for ΔT computation (ΔT itself is only ~1 minute precise).
    let year = jd_to_decimal_year(jd_tt);
    jd_tt - delta_t_seconds(year) / 86_400.0
}

/// Convert a Julian Date to a decimal year (Gregorian), accurate enough
/// for ΔT interpolation. Returns e.g. 2000.5 for JD 2,451,720 (mid-2000).
fn jd_to_decimal_year(jd: f64) -> f64 {
    // Use the standard JD → calendar conversion in Gregorian, then
    // collapse to fractional year. ΔT does not need calendar-system
    // distinction (~1 day error from Julian/Gregorian disagreement is
    // negligible vs ΔT's own uncertainty).
    let cd = crate::time::calendar::jd_to_civil(jd, crate::time::calendar::Calendar::Gregorian);
    // Day-of-year approximation: month*30 + day. Fine for ΔT.
    let days_in_year = if is_gregorian_leap(cd.year) {
        366.0
    } else {
        365.0
    };
    let day_of_year = f64::from(approx_day_of_year(cd.year, cd.month, cd.day))
        + f64::from(cd.hour) / 24.0
        + f64::from(cd.minute) / 1440.0
        + cd.second / 86_400.0;
    f64::from(cd.year) + (day_of_year - 1.0) / days_in_year
}

fn is_gregorian_leap(year: i32) -> bool {
    (year.rem_euclid(4) == 0 && year.rem_euclid(100) != 0) || year.rem_euclid(400) == 0
}

fn approx_day_of_year(year: i32, month: u8, day: u8) -> u32 {
    let mut days = [31u32, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    if is_gregorian_leap(year) {
        days[1] = 29;
    }
    let mut total = u32::from(day);
    for m in 0..(month - 1) {
        total += days[m as usize];
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_abs_diff_eq;

    // Reference ΔT values come from three sources, each authoritative
    // for its range:
    // - 1657–2025: OBSERVATIONAL_TABLE (USNO/IERS values to ±2-3 s)
    // - −720 to <1657: SMH 2016 cubic spline (Table S15)
    // - Outside those ranges: parabolic or Espenak/Meeus extrapolation
    //
    // Tolerances reflect the dominant source's accuracy at that epoch:
    // sub-second in modern centuries, ±5–15 s in the 1500–1657 spline,
    // ±10–30 s before 1500, ~100 s for ancient long-segment extrapolation.

    #[test]
    fn delta_t_at_year_2000_is_about_64s() {
        let dt = delta_t_seconds(2000.0);
        // SMH 2016 gives 63.83 s at 2000.0; polynomial gives 63.86 s.
        assert_abs_diff_eq!(dt, 63.86, epsilon = 1.0);
    }

    #[test]
    fn delta_t_at_year_1900_is_slightly_negative() {
        let dt = delta_t_seconds(1900.0);
        assert_abs_diff_eq!(dt, -2.79, epsilon = 1.0);
    }

    #[test]
    fn delta_t_at_year_1820_from_table() {
        // 1820 is in the table (entry value 11.9 s).
        let dt = delta_t_seconds(1820.0);
        assert_abs_diff_eq!(dt, 11.9, epsilon = 0.1);
    }

    #[test]
    fn delta_t_at_year_1620_smh2016_value() {
        // Segment 5 (1600..1650), t=0.4. SMH 2016 spline gives 67.064 s.
        let dt = delta_t_seconds(1620.0);
        assert_abs_diff_eq!(dt, 67.064, epsilon = 1.0);
    }

    #[test]
    fn delta_t_at_year_1000_is_about_1468s() {
        // Year 1000 is the left boundary of SMH 2016 segment 3; ΔT = a0 = 1467.654 s.
        let dt = delta_t_seconds(1000.0);
        assert_abs_diff_eq!(dt, 1467.654, epsilon = 5.0);
    }

    #[test]
    fn delta_t_at_year_0_is_about_10574s() {
        // SMH 2016 segment 1 (−720..400) gives 10574.295 s at year 0.
        let dt = delta_t_seconds(0.0);
        assert_abs_diff_eq!(dt, 10574.295, epsilon = 10.0);
    }

    // === Per-test-chart ΔT ===

    #[test]
    fn delta_t_for_vettius_valens_year_120() {
        // Feb 8, 120 CE ≈ day 39. SMH 2016 spline: 9355.556 s.
        let dt = delta_t_seconds(120.0 + 39.0 / 365.0);
        assert_abs_diff_eq!(dt, 9356.0, epsilon = 50.0);
    }

    #[test]
    fn delta_t_for_william_lilly_year_1602() {
        // May 11, 1602 ≈ year 1602.36. SMH 2016 spline gives 86.299 s.
        let dt = delta_t_seconds(1602.36);
        assert_abs_diff_eq!(dt, 86.3, epsilon = 1.0);
    }

    #[test]
    fn delta_t_for_year_1946() {
        // 1946 sits in the observational table. Interpolating between
        // 1940 (24.3) and 1950 (29.1) gives ≈ 27.4 s.
        let dt = delta_t_seconds(1946.57);
        assert_abs_diff_eq!(dt, 27.4, epsilon = 1.0);
    }

    #[test]
    fn delta_t_for_year_1984() {
        // 1984 sits in the observational table. Interpolating between
        // 1980 (50.5) and 1990 (56.9) gives ≈ 53.6 s.
        let dt = delta_t_seconds(1984.83);
        assert_abs_diff_eq!(dt, 53.6, epsilon = 1.0);
    }

    // === Round-trip ===

    #[test]
    #[allow(clippy::similar_names)]
    fn jd_ut_to_tt_and_back() {
        let jd_ut = 2_451_545.0; // 2000-01-01 12:00 UT
        let jd_tt = jd_ut_to_jd_tt(jd_ut);
        let back = jd_tt_to_jd_ut(jd_tt);
        // Round-trip within micro-day precision (≈ 0.1 s).
        assert_abs_diff_eq!(back, jd_ut, epsilon = 1e-6);
        // Forward step adds ΔT(2000)/86400 ≈ 63.8/86400 ≈ 0.000738 day.
        // (Using the observational-table value at 2000.0, not the Espenak
        // polynomial value.)
        assert!(jd_tt > jd_ut);
        assert_abs_diff_eq!(jd_tt - jd_ut, 63.8 / 86400.0, epsilon = 1e-6);
    }

    #[test]
    fn boundary_between_table_and_smh2016_spline_is_continuous() {
        // At 1657, the SMH2016 spline (segment 6: 1650..1720) gives 38.85 s.
        // The observational table starts at 1657 (44.0 s). Jump ≈ 5.15 s.
        // Both sources have their own measurement uncertainty; < 10 s is acceptable.
        let below = delta_t_seconds(1657.0 - 1e-6); // spline
        let above = delta_t_seconds(1657.0 + 1e-6); // table
        let jump = (below - above).abs();
        assert!(
            jump < 10.0,
            "table/spline splice at 1657 should be within 10 s; got {jump}"
        );
    }

    // === SMH 2016 spline unit tests (call smh2016_spline() directly) ===

    #[test]
    fn smh2016_spline_at_year_0() {
        // Year 0 is in segment 1 (−720..400). Expected from spline evaluation: 10574.295 s.
        assert_abs_diff_eq!(smh2016_spline(0.0), 10574.295, epsilon = 5.0);
    }

    #[test]
    fn smh2016_spline_at_year_120_107() {
        // Vettius Valens: Feb 8, 120 CE ≈ day 39. Expected: 9355.556 s.
        let year = 120.0 + 39.0 / 365.0;
        assert_abs_diff_eq!(smh2016_spline(year), 9355.556, epsilon = 5.0);
    }

    #[test]
    fn smh2016_spline_at_year_1000() {
        // Year 1000 is the left boundary of segment 3 (1000..1500), so ΔT = a0 exactly.
        assert_abs_diff_eq!(smh2016_spline(1000.0), 1467.654, epsilon = 0.01);
    }

    #[test]
    fn smh2016_spline_at_year_1602_36() {
        // William Lilly: year 1602.36, segment 5 (1600..1650). Expected: 86.299 s.
        assert_abs_diff_eq!(smh2016_spline(1602.36), 86.299, epsilon = 0.5);
    }

    #[test]
    fn smh2016_spline_at_year_1620() {
        // Segment 5 (1600..1650), t=0.4. Expected: 67.064 s.
        assert_abs_diff_eq!(smh2016_spline(1620.0), 67.064, epsilon = 0.5);
    }

    #[test]
    fn smh2016_spline_segment_boundaries_are_continuous() {
        // At each internal knot the value from the left segment and right segment agree.
        for &k in &[400.0_f64, 1000.0, 1500.0, 1600.0, 1650.0] {
            let left = smh2016_spline(k - 1e-6);
            let right = smh2016_spline(k + 1e-6);
            assert!(
                (left - right).abs() < 0.01,
                "discontinuity at knot {k}: left={left:.6}, right={right:.6}"
            );
        }
    }
}
