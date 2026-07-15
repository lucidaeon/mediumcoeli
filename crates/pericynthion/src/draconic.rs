//! Draconic longitude — uniform shift redefining 0° Aries as the Moon's North Node.
//!
//! The **draconic zodiac** subtracts the tropical longitude of the Moon's
//! North Node from every tropical longitude, wrapping the result into
//! \[0°, 360°). The North Node itself maps to 0° draconic Aries; all
//! other points shift uniformly by the same amount.
//!
//! # Chart projection
//!
//! [`project_chart`] converts a full [`crate::chart::ComputedChart`] to a
//! [`DraconicChart`] by applying [`draconic_longitude`] to every emitted
//! longitude. Latitude, speed, and retrograde flags are untouched — a uniform
//! longitude rotation does not change them.

use crate::chart::{Angles, ComputedChart, LilithPoints, Lots, NodePoints};

/// A lightweight draconic view of a [`ComputedChart`].
///
/// Every longitude collection is a flat `Vec` of `(label, draconic_lon_deg)`
/// pairs so renderers can consume all point types uniformly. The node longitude
/// used for the shift is preserved in [`node_lon_deg`](DraconicChart::node_lon_deg).
///
/// Latitude, daily speed, and retrograde flags are not carried — they are
/// invariant under a uniform longitude rotation.
#[derive(Debug, Clone)]
pub struct DraconicChart {
    /// Tropical longitude of the Moon's North Node used for the projection, degrees.
    pub node_lon_deg: f64,
    /// Draconic longitudes of the classical bodies: `(Body, lon_deg)`.
    pub bodies: Vec<(crate::body::Body, f64)>,
    /// Draconic longitudes of SPK asteroids: `(name, lon_deg)`.
    pub asteroids: Vec<(&'static str, f64)>,
    /// Draconic longitudes of the chart angles (Mc/Ic always; Ac/Ds/Vx/Ax when present).
    /// Labels: `"Mc"`, `"Ic"`, `"Ac"`, `"Ds"`, `"Vx"`, `"Ax"`.
    pub angles: Vec<(&'static str, f64)>,
    /// Draconic longitudes of lunar nodes: `(label, lon_deg)`.
    /// Labels: `"MeanNn"`, `"MeanSn"`, `"TrueNn"`, `"TrueSn"`.
    pub nodes: Vec<(&'static str, f64)>,
    /// Draconic longitudes of Black Moon Lilith points: `(label, lon_deg)`.
    /// Labels: `"MeanLilith"`, `"MeanPriapus"`, `"TrueLilith"`, `"TruePriapus"`.
    pub lilith: Vec<(&'static str, f64)>,
    /// Draconic longitudes of the Hermetic lots: `(label, lon_deg)`.
    /// Labels: `"Fortune"`, `"Spirit"`, `"Exaltation"`, `"Eros"`,
    /// `"Necessity"`, `"Courage"`, `"Victory"`, `"Nemesis"`.
    pub lots: Vec<(&'static str, f64)>,
    /// Draconic longitudes of fixed stars: `(name, lon_deg)`.
    pub stars: Vec<(&'static str, f64)>,
}

/// Projects a [`ComputedChart`] into the draconic zodiac.
///
/// Every emitted tropical longitude is shifted by subtracting `node_lon_deg`
/// and wrapping into \[0°, 360°) via [`draconic_longitude`]. Latitude, daily
/// speed, and retrograde flags are not included in the result — they are
/// invariant under a uniform longitude rotation.
///
/// `None` optional fields (`angles`, `nodes`, `lilith`, `lots`) produce empty
/// `Vec`s in the returned chart.
///
/// # Arguments
/// * `computed` — the tropical chart to project.
/// * `node_lon_deg` — tropical longitude of the Moon's North Node in degrees.
// The function body is naturally long: it systematically maps every optional
// sub-structure of ComputedChart through draconic_longitude. Splitting it into
// helpers would add indirection without clarity.
#[allow(clippy::too_many_lines)]
#[must_use]
pub fn project_chart(computed: &ComputedChart, node_lon_deg: f64) -> DraconicChart {
    let shift = |lon: f64| draconic_longitude(lon, node_lon_deg);

    // Bodies
    let bodies: Vec<(crate::body::Body, f64)> = computed
        .bodies
        .iter()
        .map(|cb| (cb.body, shift(cb.position.longitude_deg)))
        .collect();

    // Asteroids
    let asteroids: Vec<(&'static str, f64)> = computed
        .asteroids
        .iter()
        .map(|ca| (ca.name, shift(ca.position.longitude_deg)))
        .collect();

    // Angles (Mc/Ic always present when angles is Some; Ac/Ds/Vx/Ax optional)
    let angles: Vec<(&'static str, f64)> = match &computed.angles {
        None => vec![],
        Some(Angles {
            mc_deg,
            ic_deg,
            ac_deg,
            ds_deg,
            vx_deg,
            ax_deg,
        }) => {
            let mut v = vec![("Mc", shift(*mc_deg)), ("Ic", shift(*ic_deg))];
            if let Some(ac) = ac_deg {
                v.push(("Ac", shift(*ac)));
            }
            if let Some(ds) = ds_deg {
                v.push(("Ds", shift(*ds)));
            }
            if let Some(vx) = vx_deg {
                v.push(("Vx", shift(*vx)));
            }
            if let Some(ax) = ax_deg {
                v.push(("Ax", shift(*ax)));
            }
            v
        }
    };

    // Nodes
    let nodes: Vec<(&'static str, f64)> = match &computed.nodes {
        None => vec![],
        Some(NodePoints {
            mean_nn_deg,
            mean_sn_deg,
            true_nn_deg,
            true_sn_deg,
            ..
        }) => vec![
            ("MeanNn", shift(*mean_nn_deg)),
            ("MeanSn", shift(*mean_sn_deg)),
            ("TrueNn", shift(*true_nn_deg)),
            ("TrueSn", shift(*true_sn_deg)),
        ],
    };

    // Lilith
    let lilith: Vec<(&'static str, f64)> = match &computed.lilith {
        None => vec![],
        Some(LilithPoints {
            mean_lilith_deg,
            mean_priapus_deg,
            true_lilith_deg,
            true_priapus_deg,
            ..
        }) => vec![
            ("MeanLilith", shift(*mean_lilith_deg)),
            ("MeanPriapus", shift(*mean_priapus_deg)),
            ("TrueLilith", shift(*true_lilith_deg)),
            ("TruePriapus", shift(*true_priapus_deg)),
        ],
    };

    // Lots
    let lots: Vec<(&'static str, f64)> = match &computed.lots {
        None => vec![],
        Some(Lots {
            fortune_deg,
            spirit_deg,
            exaltation_deg,
            eros_deg,
            necessity_deg,
            courage_deg,
            victory_deg,
            nemesis_deg,
            ..
        }) => {
            let mut v = vec![
                ("Fortune", shift(*fortune_deg)),
                ("Spirit", shift(*spirit_deg)),
                ("Exaltation", shift(*exaltation_deg)),
            ];
            if let Some(e) = eros_deg {
                v.push(("Eros", shift(*e)));
            }
            if let Some(n) = necessity_deg {
                v.push(("Necessity", shift(*n)));
            }
            if let Some(c) = courage_deg {
                v.push(("Courage", shift(*c)));
            }
            if let Some(vi) = victory_deg {
                v.push(("Victory", shift(*vi)));
            }
            if let Some(ne) = nemesis_deg {
                v.push(("Nemesis", shift(*ne)));
            }
            v
        }
    };

    // Stars
    let stars: Vec<(&'static str, f64)> = computed
        .stars
        .iter()
        .map(|cs| (cs.name, shift(cs.position.longitude_deg)))
        .collect();

    DraconicChart {
        node_lon_deg,
        bodies,
        asteroids,
        angles,
        nodes,
        lilith,
        lots,
        stars,
    }
}

/// Returns the draconic longitude of a tropical ecliptic position.
///
/// Subtracts the North Node's tropical longitude from `tropical_lon_deg` and
/// wraps the result into \[0°, 360°). The North Node itself maps to exactly 0°;
/// all chart points shift by the same uniform amount.
///
/// # Arguments
/// * `tropical_lon_deg` — tropical ecliptic longitude in degrees \[0, 360).
/// * `node_lon_deg` — tropical longitude of the Moon's North Node in degrees
///   \[0, 360).
///
/// # Returns
/// Draconic longitude in degrees \[0, 360).
#[must_use]
pub fn draconic_longitude(tropical_lon_deg: f64, node_lon_deg: f64) -> f64 {
    (tropical_lon_deg - node_lon_deg).rem_euclid(360.0)
}

#[cfg(test)]
mod tests {
    use approx::assert_abs_diff_eq;

    use super::{draconic_longitude, project_chart};
    use crate::body::Body;
    use crate::chart::{ComputedBody, ComputedChart, CoordMode};
    use crate::coords::apparent::EclipticPosition;

    /// Build a minimal hand-crafted [`ComputedChart`] with two bodies at known
    /// longitudes and all optional fields absent. No ephemeris required.
    fn minimal_chart(sun_lon: f64, moon_lon: f64) -> ComputedChart {
        let make_body = |body: Body, lon: f64| ComputedBody {
            body,
            position: EclipticPosition {
                longitude_deg: lon,
                latitude_deg: 0.0,
                distance_au: 1.0,
            },
            daily_speed_deg: 1.0,
            retrograde: false,
        };
        ComputedChart {
            jd_ut: 2_451_545.0,
            jd_tt: 2_451_545.0,
            mode: CoordMode::Geocentric,
            utc_offset: "+00:00".to_string(),
            bodies: vec![
                make_body(Body::Sun, sun_lon),
                make_body(Body::Moon, moon_lon),
            ],
            asteroids: vec![],
            angles: None,
            nodes: None,
            lilith: None,
            lots: None,
            houses: vec![],
            lunar_phase: None,
            tithi: None,
            sect: None,
            interp_sect_twilight: None,
            stars: vec![],
            provenance: vec![],
        }
    }

    #[test]
    fn project_chart_shifts_all_bodies() {
        let node = 47.0_f64;
        let sun_lon = 120.0_f64;
        let moon_lon = 300.0_f64;
        let chart = minimal_chart(sun_lon, moon_lon);
        let drac = project_chart(&chart, node);

        assert_eq!(drac.bodies.len(), 2);
        let (sun_body, sun_drac) = drac.bodies[0];
        let (moon_body, moon_drac) = drac.bodies[1];
        assert_eq!(sun_body, Body::Sun);
        assert_eq!(moon_body, Body::Moon);
        assert_abs_diff_eq!(sun_drac, draconic_longitude(sun_lon, node), epsilon = 1e-12);
        assert_abs_diff_eq!(
            moon_drac,
            draconic_longitude(moon_lon, node),
            epsilon = 1e-12
        );
    }

    #[test]
    fn project_chart_records_node() {
        let chart = minimal_chart(0.0, 90.0);
        assert_abs_diff_eq!(
            project_chart(&chart, 47.0).node_lon_deg,
            47.0,
            epsilon = 1e-12
        );
    }

    #[test]
    fn project_chart_node_zero_is_passthrough() {
        use crate::chart::Angles;
        let mc = 60.0_f64;
        let ic = 240.0_f64;
        let ac = 150.0_f64;
        let ds = 330.0_f64;
        let mut chart = minimal_chart(10.0, 200.0);
        chart.angles = Some(Angles {
            mc_deg: mc,
            ic_deg: ic,
            ac_deg: Some(ac),
            ds_deg: Some(ds),
            vx_deg: None,
            ax_deg: None,
        });
        let drac = project_chart(&chart, 0.0);
        let angle_map: std::collections::HashMap<_, _> = drac.angles.iter().copied().collect();
        assert_abs_diff_eq!(angle_map["Mc"], mc, epsilon = 1e-12);
        assert_abs_diff_eq!(angle_map["Ic"], ic, epsilon = 1e-12);
        assert_abs_diff_eq!(angle_map["Ac"], ac, epsilon = 1e-12);
        assert_abs_diff_eq!(angle_map["Ds"], ds, epsilon = 1e-12);
        assert!(!angle_map.contains_key("Vx"));
        assert!(!angle_map.contains_key("Ax"));
    }

    #[test]
    fn node_at_zero_leaves_longitude_unchanged() {
        for lambda in [0.0_f64, 45.0, 123.456, 359.9] {
            assert_abs_diff_eq!(draconic_longitude(lambda, 0.0), lambda, epsilon = 1e-12);
        }
    }

    #[test]
    fn shifts_whole_set_by_node() {
        let node = 30.0_f64;
        let cases: &[(f64, f64)] = &[(0.0, 330.0), (30.0, 0.0), (90.0, 60.0), (15.0, 345.0)];
        for &(input, expected) in cases {
            assert_abs_diff_eq!(draconic_longitude(input, node), expected, epsilon = 1e-12);
        }
    }

    #[test]
    fn node_maps_to_zero() {
        for node in [12.3_f64, 200.0, 359.5] {
            assert_abs_diff_eq!(draconic_longitude(node, node), 0.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn wraps_into_range() {
        assert_abs_diff_eq!(draconic_longitude(10.0, 50.0), 320.0, epsilon = 1e-12);
    }
}
