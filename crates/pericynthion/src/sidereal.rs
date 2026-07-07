//! Sidereal-zodiac engine: ayanamsha-at-date math and a compiled-in catalog of
//! ayanamsha definitions.
//!
//! A *sidereal* ecliptic longitude is the tropical (equinox-of-date) longitude
//! minus the **ayanamsha** — the angular gap between the moving vernal equinox
//! and the fixed sidereal zero point:
//!
//! ```text
//! sidereal_lon = tropical_lon − ayanamsha(date)
//! ```
//!
//! Built-in definitions (Lahiri, Fagan-Bradley, Raman) live in the compiled-in
//! [`BUILTIN_AYANAMSHAS`] table, reached through [`AyanamshaRegistry`].
//!
//! # Precession models
//!
//! Two models are supported (see [`PrecessionModel`]):
//!
//! - **[`PrecessionModel::Iau2006`]** — epoch-anchor + IAU 2006 general precession
//!   in ecliptic longitude (pₐ). Used by Lahiri and Fagan-Bradley. The ayanamsha
//!   at a date is its published value at a defining epoch plus pₐ accrued since.
//!   Because the vernal equinox regresses (~50.29″/yr), the ayanamsha grows.
//! - **[`PrecessionModel::FixedAnnualRate`]** — formula-defined: no epoch anchor;
//!   the value is computed directly as `(julian_epoch_year − zero_year) × rate / 3600`.
//!   Used by Raman, whose published table is defined by a flat rate formula.
//!
//! # Frames: mean vs. true
//!
//! Each ayanamsha is available in two frames (see [`AyanamshaFrame`]):
//!
//! - **Mean** — precession only; no nutation. Conventional Western-sidereal usage.
//! - **True** — precession plus nutation in longitude (Δψ). Published by the
//!   Indian Astronomical Ephemeris (PAC/IMD) as the "True Ayanāṃśa".
//!
//! The frame is intrinsic to each ayanāṃśa — recorded in
//! [`Ayanamsha::default_frame`] — and can be overridden by the caller.
//! Built-in defaults: Lahiri → [`AyanamshaFrame::True`] (IAE convention);
//! Fagan-Bradley → [`AyanamshaFrame::Mean`] (Bradley SVP);
//! Raman → [`AyanamshaFrame::Mean`] (published values are means).
//!
//! For `Iau2006` rows: `value_at_epoch_deg` is stored verbatim in the row's
//! `default_frame`; `ayanamsha_deg` normalizes the anchor to mean at the epoch,
//! precesses to the target date, and optionally adds `Δψ(jd_tt)` for `True`.
//! For `FixedAnnualRate` rows: `epoch_jd_tt` and `value_at_epoch_deg` are unused;
//! the mean value is computed directly from the formula.
//!
//! # Adding an ayanamsha
//!
//! **Epoch-anchor model (`Iau2006`):** append an [`Ayanamsha`] entry to
//! [`BUILTIN_AYANAMSHAS`] carrying its published value at a defining epoch and a
//! primary-source citation. The division logic is shared; only the constants change.
//!
//! **Fixed-rate model (`FixedAnnualRate`):** set `epoch_jd_tt` and
//! `value_at_epoch_deg` to `0.0` (unused), and supply `zero_year` and
//! `rate_arcsec` instead. The formula is applied directly in `ayanamsha_deg`.

use crate::coords::obliquity::julian_centuries_t;

/// Calibration precession model an ayanamsha uses to accrue its value from the
/// defining epoch to a target date.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PrecessionModel {
    /// IAU 2006 general precession in ecliptic longitude (Capitaine et al.).
    Iau2006,
    /// Fixed annual rate: `ayanamsha_deg(jd) = (julian_epoch_year(jd) − zero_year) × rate_arcsec / 3600`.
    /// where `julian_epoch_year(jd) = 2000.0 + (jd − 2451545.0) / 365.25`.
    /// Produces the **mean** value directly; the true frame adds `Δψ` exactly as for
    /// `Iau2006`. No epoch anchor or published value is needed.
    FixedAnnualRate {
        /// Calendar year at which the ayanamsha is zero.
        zero_year: f64,
        /// Annual accrual rate (arcseconds per Julian year).
        rate_arcsec: f64,
    },
}

/// Whether an ayanāṃśa is applied in the **mean** frame (precession only) or the
/// **true** frame (with nutation in longitude, Δψ). Also records which frame a
/// stored anchor value is published in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AyanamshaFrame {
    /// Precession only; no nutation. Conventional Western-sidereal usage.
    Mean,
    /// Precession plus nutation in longitude (Δψ). Conventional Jyotish usage.
    True,
}

/// A single ayanamsha definition.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ayanamsha {
    /// Lookup slug, lowercased (e.g. `"lahiri"`).
    pub slug: &'static str,
    /// Human-readable name (e.g. `"Lahiri (Chitrapaksha)"`).
    pub display_name: &'static str,
    /// Defining epoch as a Julian Date in Terrestrial Time.
    pub epoch_jd_tt: f64,
    /// Published ayanamsha value (degrees) at the defining epoch.
    pub value_at_epoch_deg: f64,
    /// Precession model used to accrue the value away from the epoch.
    pub precession_model: PrecessionModel,
    /// The frame the stored `value_at_epoch_deg` is published in, and the
    /// default display frame for this ayanāṃśa. (Both roles coincide for the
    /// built-ins; decouple into two fields only if a future definition needs it.)
    pub default_frame: AyanamshaFrame,
    /// Optional fixed-star anchor this ayanamsha is calibrated to.
    pub star_anchor: Option<&'static str>,
}

/// The conventional default ayanamsha slug (Jyotish standard).
pub const DEFAULT_AYANAMSHA_SLUG: &str = "lahiri";

/// Compiled-in built-in ayanamsha definitions.
///
/// Each value carries a primary-source citation. The division logic is shared,
/// so adding an ayanamsha is a data edit here plus a rebuild.
pub static BUILTIN_AYANAMSHAS: &[Ayanamsha] = &[
    // Lahiri (Chitrapaksha) — the Indian national standard, calibrated so the
    // star Spica (Chitra) sits near 180° sidereal. Anchor value is the published
    // "True Ayanāṃśa" for 2019-01-01 (JD 2458484.5), 24°07′06.0″, from the
    // Indian Astronomical Ephemeris 2019 (Positional Astronomy Centre, IMD);
    // the definition follows the Calendar Reform Committee (1955). The anchor is
    // the IAE True Ayanāṃśa, so `default_frame` is `True`; see module Frames.
    Ayanamsha {
        slug: "lahiri",
        display_name: "Lahiri (Chitrapaksha)",
        epoch_jd_tt: 2_458_484.5,
        value_at_epoch_deg: 24.118_333, // 24°07′06.0″
        precession_model: PrecessionModel::Iau2006,
        default_frame: AyanamshaFrame::True,
        star_anchor: Some("Spica (Chitra)"),
    },
    // Fagan-Bradley — the Western sidereal standard. Anchor is Bradley's Synetic
    // Vernal Point, SVP = 335°57′28.64″ at epoch 1950.0 (Besselian, JD
    // 2433282.42346), determined from Spica at 29° Virgo; published by Donald
    // Bradley ("Garth Allen") in American Astrology, May 1957, and reproduced in
    // Cyril Fagan & R. C. Firebrace, A Primer of Sidereal Astrology (AFA), p.13.
    // The ayanamsha is the complement of the SVP: 360° − 335°57′28.64″.
    Ayanamsha {
        slug: "fagan_bradley",
        display_name: "Fagan-Bradley",
        epoch_jd_tt: 2_433_282.423_46,
        value_at_epoch_deg: 24.042_044, // 360° − 335°57′28.64″
        precession_model: PrecessionModel::Iau2006,
        default_frame: AyanamshaFrame::Mean,
        star_anchor: Some("Spica (29° Virgo, SVP)"),
    },
    // Raman — B.V. Raman, Hindu Predictive Astrology, 20th ed. (1992), Appendix A
    // + Table IV. Formula: ayanamsha(year) = (year − 397) × 50⅓″, where 50⅓ = 151/3
    // arcsec exactly. Zero year 397 CE is stated as an algorithm step; Raman
    // explicitly declines to derive it. Rate: exactly 151/3 arcsec per calendar year,
    // flat (no nutation). All published values are means → default_frame: Mean.
    // Primary-source authority: $ASTRO_RESEARCH/ayanamsha_raman.md.
    // `epoch_jd_tt` and `value_at_epoch_deg` are unused for FixedAnnualRate rows.
    Ayanamsha {
        slug: "raman",
        display_name: "Raman",
        epoch_jd_tt: 0.0,        // unused for FixedAnnualRate
        value_at_epoch_deg: 0.0, // unused for FixedAnnualRate
        precession_model: PrecessionModel::FixedAnnualRate {
            zero_year: 397.0,
            rate_arcsec: 151.0 / 3.0,
        },
        default_frame: AyanamshaFrame::Mean,
        star_anchor: None,
    },
];

/// Accumulated IAU 2006 general precession in ecliptic longitude pₐ
/// (arcseconds) from J2000.0 to the instant `t` Julian centuries (TT) past
/// J2000.0.
///
/// P03 series from Capitaine, Wallace & Chapront (2003), "Expressions for IAU
/// 2000 precession quantities", A&A 412, 567–586 (eq. 39, the IAU 2006
/// general precession in longitude):
/// `pₐ = 5028.796195″ T + 1.1054348″ T² + 0.00007964″ T³`
/// `   − 0.000023857″ T⁴ − 0.0000000383″ T⁵`.
fn general_precession_lon_arcsec(t: f64) -> f64 {
    // Horner form, ascending powers of T (the constant term is zero — pₐ is
    // measured *from* J2000, so it vanishes at the epoch).
    t * (5_028.796_195
        + t * (1.105_434_8 + t * (0.000_079_64 + t * (-0.000_023_857 + t * -0.000_000_038_3))))
}

/// General precession in ecliptic longitude (degrees) accrued between two
/// JD-TT instants under the IAU 2006 model. Positive = equinox regression.
#[must_use]
fn precession_lon_accrued_iau2006_deg(from_jd_tt: f64, to_jd_tt: f64) -> f64 {
    let accrued_arcsec = general_precession_lon_arcsec(julian_centuries_t(to_jd_tt))
        - general_precession_lon_arcsec(julian_centuries_t(from_jd_tt));
    accrued_arcsec / 3600.0
}

/// Nutation in longitude Δψ (degrees) at the given JD-TT (IAU 2000B).
fn delta_psi_deg(jd_tt: f64) -> f64 {
    crate::coords::nutation::nutation(jd_tt)
        .delta_psi
        .to_degrees()
}

/// Value of the ayanāṃśa (degrees) at the given JD-TT, in the requested frame.
///
/// For `Iau2006` rows: the stored `value_at_epoch_deg` is in the row's
/// `default_frame`; it is normalized to a mean value at the epoch, precessed to
/// `jd_tt`, and — for the `True` frame — the nutation term `Δψ(jd_tt)` is added.
///
/// For `FixedAnnualRate` rows: the mean value is computed directly as
/// `(julian_epoch_year(jd_tt) − zero_year) × rate_arcsec / 3600`;
/// `epoch_jd_tt` and `value_at_epoch_deg` are ignored.
#[must_use]
pub fn ayanamsha_deg(ayanamsha: &Ayanamsha, jd_tt: f64, frame: AyanamshaFrame) -> f64 {
    let mean = match ayanamsha.precession_model {
        PrecessionModel::FixedAnnualRate {
            zero_year,
            rate_arcsec,
        } => {
            // Formula: ayanamsha_deg(jd) = (julian_epoch_year(jd) − zero_year) × rate_arcsec / 3600
            // julian_epoch_year(jd) = 2000.0 + (jd − J2000) / 365.25
            let julian_year = 2000.0 + (jd_tt - 2_451_545.0) / 365.25;
            (julian_year - zero_year) * rate_arcsec / 3_600.0
        }
        PrecessionModel::Iau2006 => {
            // Normalize a natively-true anchor to mean at the epoch (mean anchors are
            // already mean). Sign pinned by `lahiri_true_matches_iae_2019_across_year`.
            let mean_at_epoch = match ayanamsha.default_frame {
                AyanamshaFrame::Mean => ayanamsha.value_at_epoch_deg,
                AyanamshaFrame::True => {
                    ayanamsha.value_at_epoch_deg - delta_psi_deg(ayanamsha.epoch_jd_tt)
                }
            };
            mean_at_epoch + precession_lon_accrued_iau2006_deg(ayanamsha.epoch_jd_tt, jd_tt)
        }
    };
    match frame {
        AyanamshaFrame::Mean => mean,
        AyanamshaFrame::True => mean + delta_psi_deg(jd_tt),
    }
}

/// Sidereal ecliptic longitude (degrees, normalized to `[0, 360)`):
/// `tropical_lon_deg − ayanamsha_deg(ayanamsha, jd_tt, frame)`.
///
/// Pass [`Ayanamsha::default_frame`] to match the publisher's intent, or select
/// [`AyanamshaFrame::Mean`] / [`AyanamshaFrame::True`] explicitly for a specific
/// mean/true-ayanamsha comparison.
#[must_use]
pub fn sidereal_longitude(
    tropical_lon_deg: f64,
    jd_tt: f64,
    ayanamsha: &Ayanamsha,
    frame: AyanamshaFrame,
) -> f64 {
    (tropical_lon_deg - ayanamsha_deg(ayanamsha, jd_tt, frame)).rem_euclid(360.0)
}

/// Rotate every placement longitude of a [`crate::chart::ComputedChart`] into
/// the sidereal frame defined by `ayanamsha`, returning a new chart.
///
/// Bodies, asteroids, angles, nodes, Lilith points, lots, and fixed stars are
/// shifted by [`sidereal_longitude`] at the chart's `jd_tt`. House cusps are
/// deliberately left in tropical longitudes: house *assignment* is invariant
/// under the constant ayanamsha shift, and this matches the draconic renderer
/// and the JZOD emitter, which also display tropical cusps. Shift-invariant
/// quantities (lunar phase, tithi, sect) are copied unchanged.
///
/// `frame` selects mean (precession only) or true (with nutation); pass
/// [`Ayanamsha::default_frame`] to match the publisher's intent.
#[must_use]
pub fn project_chart(
    computed: &crate::chart::ComputedChart,
    ayanamsha: &Ayanamsha,
    frame: AyanamshaFrame,
) -> crate::chart::ComputedChart {
    let jd_tt = computed.jd_tt;
    let shift = |lon: f64| sidereal_longitude(lon, jd_tt, ayanamsha, frame);
    let mut out = computed.clone();
    for b in &mut out.bodies {
        b.position.longitude_deg = shift(b.position.longitude_deg);
    }
    for a in &mut out.asteroids {
        a.position.longitude_deg = shift(a.position.longitude_deg);
    }
    if let Some(ang) = &mut out.angles {
        ang.mc_deg = shift(ang.mc_deg);
        ang.ic_deg = shift(ang.ic_deg);
        ang.ac_deg = ang.ac_deg.map(&shift);
        ang.ds_deg = ang.ds_deg.map(&shift);
        ang.vx_deg = ang.vx_deg.map(&shift);
        ang.ax_deg = ang.ax_deg.map(&shift);
    }
    if let Some(n) = &mut out.nodes {
        n.mean_nn_deg = shift(n.mean_nn_deg);
        n.mean_sn_deg = shift(n.mean_sn_deg);
        n.true_nn_deg = shift(n.true_nn_deg);
        n.true_sn_deg = shift(n.true_sn_deg);
    }
    if let Some(l) = &mut out.lilith {
        l.mean_lilith_deg = shift(l.mean_lilith_deg);
        l.mean_priapus_deg = shift(l.mean_priapus_deg);
        l.true_lilith_deg = shift(l.true_lilith_deg);
        l.true_priapus_deg = shift(l.true_priapus_deg);
    }
    if let Some(lots) = &mut out.lots {
        lots.fortune_deg = shift(lots.fortune_deg);
        lots.spirit_deg = shift(lots.spirit_deg);
        lots.exaltation_deg = shift(lots.exaltation_deg);
        lots.eros_deg = lots.eros_deg.map(&shift);
        lots.necessity_deg = lots.necessity_deg.map(&shift);
        lots.courage_deg = lots.courage_deg.map(&shift);
        lots.victory_deg = lots.victory_deg.map(&shift);
        lots.nemesis_deg = lots.nemesis_deg.map(&shift);
    }
    for s in &mut out.stars {
        s.position.longitude_deg = shift(s.position.longitude_deg);
    }
    out
}

#[cfg(feature = "jzod")]
impl From<jzod::SiderealFrame> for AyanamshaFrame {
    /// Convert a JZOD [`jzod::SiderealFrame`] to a pericynthion [`AyanamshaFrame`].
    ///
    /// Used at format boundaries so callers write `.into()` rather than
    /// repeating an ad-hoc two-arm match. Exhaustive match: adding a variant to
    /// either enum causes a compile error here.
    fn from(f: jzod::SiderealFrame) -> Self {
        match f {
            jzod::SiderealFrame::Mean => Self::Mean,
            jzod::SiderealFrame::True => Self::True,
        }
    }
}

#[cfg(feature = "jzod")]
impl From<AyanamshaFrame> for jzod::SiderealFrame {
    /// Convert a pericynthion [`AyanamshaFrame`] to a JZOD [`jzod::SiderealFrame`].
    ///
    /// Exhaustive match: adding a variant to either enum causes a compile error here.
    fn from(f: AyanamshaFrame) -> Self {
        match f {
            AyanamshaFrame::Mean => Self::Mean,
            AyanamshaFrame::True => Self::True,
        }
    }
}

/// Catalog accessor over the compiled-in built-in ayanamshas.
#[derive(Debug, Clone, Copy)]
pub struct AyanamshaRegistry {
    entries: &'static [Ayanamsha],
}

impl AyanamshaRegistry {
    /// Catalog of the built-in ayanamshas ([`BUILTIN_AYANAMSHAS`]).
    #[must_use]
    pub fn with_builtins() -> Self {
        Self {
            entries: BUILTIN_AYANAMSHAS,
        }
    }

    /// Look up an ayanamsha by slug (case-insensitive). `None` if unknown.
    #[must_use]
    pub fn get(&self, slug: &str) -> Option<&'static Ayanamsha> {
        self.entries
            .iter()
            .find(|a| a.slug.eq_ignore_ascii_case(slug))
    }

    /// All slugs, in catalog order, for CLI listing / error suggestions.
    #[must_use]
    pub fn slugs(&self) -> Vec<&'static str> {
        self.entries.iter().map(|a| a.slug).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Ayanamsha, AyanamshaFrame, AyanamshaRegistry, PrecessionModel, ayanamsha_deg,
        project_chart, sidereal_longitude,
    };
    use approx::assert_abs_diff_eq;

    /// Single-body chart with Sun at 120° tropical longitude, used by sidereal
    /// projection tests. `jd_tt = 2_451_545.0` (J2000.0).
    fn sun_only_chart() -> crate::chart::ComputedChart {
        use crate::body::Body;
        use crate::chart::{ComputedBody, ComputedChart, CoordMode};
        use crate::coords::apparent::EclipticPosition;
        use crate::houses::{HouseCusps, HouseSystem};
        let cusp_rad = 120.0_f64.to_radians();
        ComputedChart {
            jd_ut: 2_451_545.0,
            jd_tt: 2_451_545.0,
            mode: CoordMode::Geocentric,
            utc_offset: "+00:00".to_string(),
            bodies: vec![ComputedBody {
                body: Body::Sun,
                position: EclipticPosition {
                    longitude_deg: 120.0,
                    latitude_deg: 0.0,
                    distance_au: 1.0,
                },
                daily_speed_deg: 1.0,
                retrograde: false,
            }],
            asteroids: vec![],
            angles: None,
            nodes: None,
            lilith: None,
            lots: None,
            houses: vec![(HouseSystem::WholeSign, Some(HouseCusps([cusp_rad; 12])))],
            lunar_phase: None,
            tithi: None,
            sect: None,
            interp_sect_twilight: None,
            stars: vec![],
        }
    }

    /// A bare test ayanamsha defined at J2000 (JD 2451545.0 TT).
    fn test_ayanamsha(value_at_epoch_deg: f64) -> Ayanamsha {
        Ayanamsha {
            slug: "test",
            display_name: "Test",
            epoch_jd_tt: 2_451_545.0,
            value_at_epoch_deg,
            precession_model: PrecessionModel::Iau2006,
            default_frame: AyanamshaFrame::Mean,
            star_anchor: None,
        }
    }

    #[test]
    fn precession_accrual_is_zero_at_epoch() {
        let a = test_ayanamsha(24.0);
        assert_abs_diff_eq!(
            ayanamsha_deg(&a, 2_451_545.0, AyanamshaFrame::Mean),
            24.0,
            epsilon = 1e-9
        );
    }

    #[test]
    fn precession_accrues_about_50_arcsec_per_year() {
        // IAU 2006 general precession in longitude pₐ ≈ 5028.796″/century at
        // J2000 (Capitaine et al. 2003), i.e. ≈ 50.288″ over one Julian year.
        let a = test_ayanamsha(24.0);
        let one_year_later = 2_451_545.0 + 365.25;
        let accrued_arcsec = (ayanamsha_deg(&a, one_year_later, AyanamshaFrame::Mean)
            - a.value_at_epoch_deg)
            * 3600.0;
        assert_abs_diff_eq!(accrued_arcsec, 50.288, epsilon = 0.5);
    }

    #[test]
    fn sidereal_subtracts_and_wraps() {
        // tropical 5° − ayanamsha 24° (at epoch) = −19° ≡ 341°.
        let a = test_ayanamsha(24.0);
        assert_abs_diff_eq!(
            sidereal_longitude(5.0, 2_451_545.0, &a, AyanamshaFrame::Mean),
            341.0,
            epsilon = 1e-9
        );
    }

    #[test]
    fn builtins_have_lahiri_and_fagan() {
        let r = AyanamshaRegistry::with_builtins();
        assert!(r.get("lahiri").is_some());
        // Lookup is case-insensitive.
        assert!(r.get("FAGAN_BRADLEY").is_some());
        assert!(r.get("raman").is_some());
        assert!(r.get("nope").is_none());
    }

    // JD 2458484.5 = 2019-01-01; JD 2433282.42346 = epoch 1950.0 (Besselian).
    const JD_2019: f64 = 2_458_484.5;
    const JD_B1950: f64 = 2_433_282.423_46;

    #[test]
    fn fagan_anchor_reconstructs_bradley_svp() {
        // Bradley's Synetic Vernal Point: SVP = 335°57′28.64″ = 335.957956° at
        // epoch 1950.0 (American Astrology, May 1957; Fagan & Firebrace, Primer,
        // p.13). The ayanamsha is its complement; 360° − ayanamsha must return
        // the published SVP. Fagan-Bradley is Mean-frame; test in Mean.
        let fagan = AyanamshaRegistry::with_builtins()
            .get("fagan_bradley")
            .unwrap();
        assert_abs_diff_eq!(
            ayanamsha_deg(fagan, JD_B1950, AyanamshaFrame::Mean),
            24.042_044,
            epsilon = 0.001
        );
        let svp = 360.0 - ayanamsha_deg(fagan, JD_B1950, AyanamshaFrame::Mean);
        assert_abs_diff_eq!(svp, 335.957_956, epsilon = 0.001);
    }

    #[test]
    fn project_chart_rotates_bodies_but_not_house_cusps() {
        use crate::houses::HouseCusps;

        let sun_lon = 120.0_f64;
        let cusp_rad = 120.0_f64.to_radians();
        let computed = sun_only_chart();

        let lahiri = AyanamshaRegistry::with_builtins().get("lahiri").unwrap();
        let expected_body =
            sidereal_longitude(sun_lon, computed.jd_tt, lahiri, AyanamshaFrame::Mean);
        let out = project_chart(&computed, lahiri, AyanamshaFrame::Mean);

        // Body longitude is rotated into the sidereal frame.
        assert_abs_diff_eq!(
            out.bodies[0].position.longitude_deg,
            expected_body,
            epsilon = 1e-9
        );
        // House cusps are left in tropical longitudes (unchanged).
        let HouseCusps(cusps) = out.houses[0].1.as_ref().unwrap();
        assert_abs_diff_eq!(cusps[0], cusp_rad, epsilon = 1e-12);
    }

    #[test]
    fn project_chart_true_differs_from_mean_by_nutation() {
        let computed = sun_only_chart();
        let lahiri = AyanamshaRegistry::with_builtins().get("lahiri").unwrap();
        let mean = project_chart(&computed, lahiri, AyanamshaFrame::Mean);
        let truef = project_chart(&computed, lahiri, AyanamshaFrame::True);
        let dpsi = crate::coords::nutation::nutation(computed.jd_tt)
            .delta_psi
            .to_degrees();
        let dm = mean.bodies[0].position.longitude_deg;
        let dt = truef.bodies[0].position.longitude_deg;
        assert_abs_diff_eq!((dt - dm).abs(), dpsi.abs(), epsilon = 1e-9);
    }

    #[test]
    fn builtins_carry_expected_default_frames() {
        let r = AyanamshaRegistry::with_builtins();
        assert_eq!(r.get("lahiri").unwrap().default_frame, AyanamshaFrame::True);
        assert_eq!(
            r.get("fagan_bradley").unwrap().default_frame,
            AyanamshaFrame::Mean
        );
        assert_eq!(r.get("raman").unwrap().default_frame, AyanamshaFrame::Mean);
    }

    #[test]
    fn true_and_mean_differ_by_nutation() {
        let a = test_ayanamsha(24.0); // default_frame = Mean
        let jd = 2_451_545.0 + 3_650.0; // ~2010, |Δψ| clearly non-zero
        let mean = ayanamsha_deg(&a, jd, AyanamshaFrame::Mean);
        let truef = ayanamsha_deg(&a, jd, AyanamshaFrame::True);
        let dpsi_deg = crate::coords::nutation::nutation(jd).delta_psi.to_degrees();
        assert!(
            (mean - truef).abs() > 1e-4,
            "frames must differ by nutation"
        );
        assert_abs_diff_eq!((truef - mean).abs(), dpsi_deg.abs(), epsilon = 1e-12);
    }

    #[test]
    fn lahiri_true_matches_iae_2019_across_year() {
        let lahiri = AyanamshaRegistry::with_builtins().get("lahiri").unwrap();
        // IAE 2019 True Ayanāṃśa (PAC/IMD): 2019-01-01 = 24°07′06.0″ (anchor).
        assert_abs_diff_eq!(
            ayanamsha_deg(lahiri, JD_2019, AyanamshaFrame::True),
            24.118_333, // 24°07′06.0″
            epsilon = 0.001
        );
        // 2019-07-03 (JD 2458667.5) pins the Δψ sign against the intra-year wobble.
        // Read from IAE 2019 True Ayanāṃśa table: 24°07′30.3″.
        let jd_jul3 = 2_458_667.5;
        assert_abs_diff_eq!(
            ayanamsha_deg(lahiri, jd_jul3, AyanamshaFrame::True),
            24.125_083, // 24°07′30.3″
            epsilon = 0.000_2  // ~0.72″: engine-vs-IAE residual is ~0.004″ (green); a Δψ sign
                        // flip produces ~1.8″ error (rejected). Was 0.000_8 (~2.88″),
                        // which was too loose to catch the sign error.
        );
    }

    #[test]
    fn lahiri_fagan_offset_is_about_0_88_deg() {
        // The two independently-sourced anchors (IAE 2019 Lahiri; Bradley SVP
        // Fagan-Bradley), carried to a common epoch by the same engine, must
        // reproduce the well-documented ~0.88° Lahiri↔Fagan-Bradley offset.
        let r = AyanamshaRegistry::with_builtins();
        let lahiri = r.get("lahiri").unwrap();
        let fagan = r.get("fagan_bradley").unwrap();
        let offset = ayanamsha_deg(fagan, 2_451_545.0, AyanamshaFrame::Mean)
            - ayanamsha_deg(lahiri, 2_451_545.0, AyanamshaFrame::Mean);
        assert_abs_diff_eq!(offset, 0.88, epsilon = 0.01);
    }

    /// Degrees-minutes-seconds to decimal degrees.
    fn dms(d: u32, m: u32, s: f64) -> f64 {
        f64::from(d) + f64::from(m) / 60.0 + s / 3600.0
    }

    #[test]
    fn raman_zero_at_year_397() {
        // The defining property of the Raman formula: ayanamsha(397 CE) = 0.
        // Test epoch: Julian epoch year 397.0 → JD = 2451545.0 + (397.0 − 2000.0) × 365.25
        let raman = AyanamshaRegistry::with_builtins().get("raman").unwrap();
        let jd_397 = 2_451_545.0 + (397.0 - 2000.0) * 365.25;
        assert_abs_diff_eq!(
            ayanamsha_deg(raman, jd_397, AyanamshaFrame::Mean),
            0.0,
            epsilon = 1e-9
        );
    }

    #[test]
    fn raman_table_iv_spot_checks() {
        // Spot values from Table IV: B.V. Raman, Hindu Predictive Astrology, 20th
        // ed. (1992), Appendix A + Table IV; verified against formula in
        // $ASTRO_RESEARCH/ayanamsha_raman.md.
        //
        // JD basis: Julian epoch year Y → JD = 2451545.0 + (Y − 2000.0) × 365.25.
        // Raman works at integer-year precision; the Julian-vs-calendar-year
        // discrepancy across 1880–2000 is ≤1.7″, well within the 2″ tolerance.
        let raman = AyanamshaRegistry::with_builtins().get("raman").unwrap();
        let jd = |y: f64| 2_451_545.0 + (y - 2000.0) * 365.25;

        let cases: &[(f64, f64)] = &[
            (1880.0, dms(20, 44, 4.0)),  // 20°44′04″
            (1900.0, dms(21, 0, 50.0)),  // 21°00′50″
            (1912.0, dms(21, 10, 54.0)), // 21°10′54″
            (1950.0, dms(21, 42, 47.0)), // 21°42′47″
            (2000.0, dms(22, 24, 44.0)), // 22°24′44″
        ];
        for &(year, expected) in cases {
            assert_abs_diff_eq!(
                ayanamsha_deg(raman, jd(year), AyanamshaFrame::Mean),
                expected,
                epsilon = 0.000_556, // 2″
            );
        }
    }

    #[test]
    fn raman_true_differs_from_mean_by_delta_psi() {
        // True frame = mean + Δψ, exactly as for other ayanamshas.
        let raman = AyanamshaRegistry::with_builtins().get("raman").unwrap();
        let jd = 2_451_545.0; // J2000.0
        let mean = ayanamsha_deg(raman, jd, AyanamshaFrame::Mean);
        let truef = ayanamsha_deg(raman, jd, AyanamshaFrame::True);
        let dpsi = crate::coords::nutation::nutation(jd).delta_psi.to_degrees();
        assert_abs_diff_eq!((truef - mean).abs(), dpsi.abs(), epsilon = 1e-12);
    }
}

#[cfg(all(test, feature = "jzod"))]
mod jzod_tests {
    use super::BUILTIN_AYANAMSHAS;

    #[test]
    fn builtin_ayanamshas_agree_with_jzod_canon() {
        // Every pericynthion BUILTIN_AYANAMSHAS row must resolve in the JZOD
        // canonical table (jzod::ayanamsha), and the two default_frame fields
        // must agree. A future drift between the numeric registry and the name
        // canon causes a compile error in the From impls and a test failure here.
        for row in BUILTIN_AYANAMSHAS {
            let info = jzod::ayanamsha::resolve(row.slug).unwrap_or_else(|| {
                panic!(
                    "pericynthion builtin '{}' not found in jzod::ayanamsha",
                    row.slug
                )
            });
            let jzod_frame = jzod::SiderealFrame::from(row.default_frame);
            assert_eq!(
                info.default_frame,
                Some(jzod_frame),
                "default_frame mismatch for '{}': pericynthion={:?}, jzod={:?}",
                row.slug,
                row.default_frame,
                info.default_frame,
            );
        }
    }
}
