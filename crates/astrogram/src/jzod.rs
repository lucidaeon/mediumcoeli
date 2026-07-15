//! JZOD v0.0.0 writer.
//!
//! Converts a slice of [`crate::chart::Chart`]s into a JZOD-compliant JSON string.
//! JZOD is a write-only format in this version — reading is not implemented.
//!
//! Field mapping:
//! - `birth.datetime` — year/month/day/hour/minute/second + `utc_offset` (+HH:MM)
//! - `zodiac` — object `{ "name": "tropical" }` per OQ-4
//! - `gender` — "m"/"f"/"a" from EventType; absent for entity charts
//! - `placements.bodies` — empty; blackmoon carries no ephemeris data
//! - `ephemeris` — omitted entirely; blackmoon converts formats, it does not
//!   compute positions, so there is no ephemeris provenance to report
//! - `generator` — supplied by the caller (see [`crate::jzod::write_file`]); blackmoon
//!   passes its own name/version plus the `astrogram`/`jzod` components it's
//!   built against
//! - `uid` — deterministic from birth data (stable across repeated exports)

use crate::capability::{CapabilitySet, ChartField};
use crate::chart::{Chart, CoordinateSystem, EventType, Zodiac};

pub use crate::capability::CapabilitySet as Caps;

/// Fields recovered when reading JZOD (none — read is not implemented).
pub const READ_CAPS: CapabilitySet = CapabilitySet::new(&[]);

/// Fields the JZOD writer preserves.
pub const WRITE_CAPS: CapabilitySet = CapabilitySet::new(&[
    ChartField::SecondaryName,
    ChartField::Region,
    ChartField::SourceRating,
    ChartField::Zodiac,
    ChartField::CoordinateSystem,
    ChartField::EventType,
]);

/// Serialize `charts` as a JZOD v0.0.0 JSON document.
///
/// `generator` identifies the tool producing this document (name, version,
/// and the in-repo library components it's built against) and is stamped
/// onto every chart. astrogram has no generator identity of its own — it is
/// a library, not a producing tool — so callers (e.g. `blackmoon`) must
/// supply one.
///
/// # Panics
///
/// Never in practice — `serde_json` only fails on non-finite floats, which
/// `Chart` coordinate fields cannot hold (enforced at construction).
#[must_use]
pub fn write_file(charts: &[Chart], generator: &jzod::Generator) -> String {
    let jcharts: Vec<jzod::Chart> = charts.iter().map(|c| chart_to_jzod(c, generator)).collect();
    jzod::to_string_pretty(&jzod::JzodDocument::new(jcharts))
}

fn chart_to_jzod(c: &Chart, generator: &jzod::Generator) -> jzod::Chart {
    let uid = jzod::uid::derive_uid(&jzod::uid::UidSeed {
        name: &c.name,
        year: c.year,
        month: c.month,
        day: c.day,
        hour: c.hour,
        minute: c.minute,
        second: c.second,
        latitude: c.latitude.degrees(),
        longitude: c.longitude.degrees(),
        tz_offset_hours: c.tz_offset_hours,
        secondary_name: c.secondary_name.as_deref(),
    });

    let aliases: Vec<String> = c
        .secondary_name
        .iter()
        .filter(|s| !s.is_empty())
        .cloned()
        .collect();

    let location_name = match (&c.city, &c.region) {
        (Some(city), Some(region)) if !region.is_empty() => Some(format!("{city}, {region}")),
        (Some(city), _) => Some(city.clone()),
        _ => Some(c.name.clone()),
    };

    let rodden_rating = c.source_rating.as_ref().filter(|r| !r.is_empty()).cloned();

    jzod::Chart {
        uid,
        chart_type: jzod::ChartType::Radix,
        name: Some(jzod::Name {
            display: c.name.clone(),
            aliases,
        }),
        gender: gender_code(c.event_type).map(str::to_string),
        rodden_rating,
        birth: jzod::Birth {
            datetime: jzod::Datetime {
                year: i32::from(c.year),
                month: c.month,
                day: c.day,
                hour: c.hour,
                minute: c.minute,
                second: c.second,
                utc_offset: jzod::time::format_utc_offset(c.tz_offset_hours),
                iana_tz: c.tz_abbreviation.clone(),
                unknown: false,
                tod_method: None,
            },
            location: jzod::Location {
                name: location_name,
                latitude: Some(c.latitude.degrees()),
                longitude: Some(c.longitude.degrees()),
            },
        },
        zodiac: zodiac_to_jzod(c.zodiac),
        coordinate_system: match c.coordinate_system {
            CoordinateSystem::Geocentric => jzod::CoordinateSystem::Geocentric,
            CoordinateSystem::Topocentric => jzod::CoordinateSystem::Topocentric,
            CoordinateSystem::Heliocentric => jzod::CoordinateSystem::Heliocentric,
        },
        sect: None,
        interp_sect_twilight: None,
        // blackmoon converts formats; it never computes ephemeris positions,
        // so there is no provenance to report here.
        ephemeris: None,
        generator: generator.clone(),
        placements: jzod::Placements::default(),
        houses: jzod::Houses::new(),
        lunar_phase: None,
        tithi: None,
        nested: vec![],
    }
}

fn zodiac_to_jzod(z: Zodiac) -> jzod::Zodiac {
    match z {
        Zodiac::Tropical => jzod::Zodiac::Tropical,
        Zodiac::Draconic => jzod::Zodiac::Draconic { node: None },
        Zodiac::FaganAllen => sidereal_from_canon("fagan_allen"),
        Zodiac::Lahiri => sidereal_from_canon("lahiri"),
        Zodiac::DeLuce => sidereal_from_canon("de_luce"),
        Zodiac::Raman => sidereal_from_canon("raman"),
        Zodiac::UshaShashi => sidereal_from_canon("usha_shashi"),
        Zodiac::Krishnamurti => sidereal_from_canon("krishnamurti"),
        Zodiac::DjwhalKhul => sidereal_from_canon("djwhal_khul"),
        Zodiac::Svp => sidereal_from_canon("svp"),
        Zodiac::SriYukteswar => sidereal_from_canon("sri_yukteswar"),
        Zodiac::JnBhasin => sidereal_from_canon("jn_bhasin"),
        Zodiac::LarryEly => sidereal_from_canon("larry_ely"),
        Zodiac::TakraI => sidereal_from_canon("takra_i"),
        Zodiac::TakraII => sidereal_from_canon("takra_ii"),
        Zodiac::SundaraRajan => sidereal_from_canon("sundara_rajan"),
        Zodiac::ShillPond => sidereal_from_canon("shill_pond"),
        // Solar Fire records the raw id; preserve it textually so consumers get
        // a visible failure rather than silent tropical when they encounter an
        // unknown ayanamsha.
        Zodiac::Other(n) => jzod::Zodiac::Sidereal {
            ayanamsha: Some(format!("other_{n}")),
            frame: None,
        },
    }
}

/// Map a raw ayanamsha slug (or known alias) to a [`jzod::Zodiac::Sidereal`]
/// value whose canonical slug and `default_frame` come from the JZOD canonical
/// authority table ([`jzod::ayanamsha`]).
///
/// # Panics
///
/// Panics if `raw_slug` is neither a canonical slug nor a known alias.  This
/// indicates a mis-wiring between `astrogram::chart::Zodiac` variants and the
/// canon table, which the coupling test catches at test time.
fn sidereal_from_canon(raw_slug: &str) -> jzod::Zodiac {
    let info = jzod::ayanamsha::resolve(raw_slug)
        .unwrap_or_else(|| panic!("ayanamsha slug not in canonical table: {raw_slug}"));
    jzod::Zodiac::Sidereal {
        ayanamsha: Some(info.slug.to_string()),
        frame: info.default_frame,
    }
}

fn gender_code(e: EventType) -> Option<&'static str> {
    match e {
        EventType::Male => Some("m"),
        EventType::Female => Some("f"),
        EventType::Unspecified => Some("a"),
        EventType::Event | EventType::Horary => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn anna_freud() -> Chart {
        use crate::chart::{HouseSystem, Latitude, Longitude};
        Chart {
            name: "Anna Freud".into(),
            secondary_name: None,
            city: Some("Vienna".into()),
            region: Some("Austria".into()),
            longitude: Longitude::new(16.371_667).unwrap(),
            latitude: Latitude::new(48.208_333).unwrap(),
            year: 1895,
            month: 12,
            day: 3,
            hour: 15,
            minute: 15,
            second: 0,
            tz_offset_hours: 1.0,
            tz_abbreviation: Some("LMT".into()),
            is_lmt: false,
            event_type: EventType::Female,
            source_rating: Some("AA".into()),
            house_system: HouseSystem::Placidus,
            zodiac: Zodiac::Tropical,
            coordinate_system: CoordinateSystem::Geocentric,
            sub_charts: vec![],
            notes: None,
        }
    }

    fn test_generator() -> jzod::Generator {
        jzod::Generator {
            name: "blackmoon".into(),
            version: "0.0.0".into(),
            components: vec![
                jzod::Component {
                    name: "astrogram".into(),
                    version: crate::ASTROGRAM_VERSION.into(),
                },
                jzod::Component {
                    name: "jzod".into(),
                    version: jzod::JZOD_VERSION.into(),
                },
            ],
        }
    }

    #[test]
    fn uid_is_stable_across_exports() {
        let c = anna_freud();
        let a: serde_json::Value =
            serde_json::from_str(&write_file(std::slice::from_ref(&c), &test_generator())).unwrap();
        let b: serde_json::Value =
            serde_json::from_str(&write_file(std::slice::from_ref(&c), &test_generator())).unwrap();
        assert_eq!(a["charts"][0]["uid"], b["charts"][0]["uid"]);
    }

    #[test]
    fn uid_differs_on_name_change() {
        let c = anna_freud();
        let mut c2 = c.clone();
        c2.name = "Sigmund Freud".into();
        let a: serde_json::Value =
            serde_json::from_str(&write_file(&[c], &test_generator())).unwrap();
        let b: serde_json::Value =
            serde_json::from_str(&write_file(&[c2], &test_generator())).unwrap();
        assert_ne!(a["charts"][0]["uid"], b["charts"][0]["uid"]);
    }

    #[test]
    fn write_file_parses_as_valid_json() {
        let chart = anna_freud();
        let out = write_file(&[chart], &test_generator());
        let v: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
        assert_eq!(v["version"], "0.0.0");
        let charts = v["charts"].as_array().unwrap();
        assert_eq!(charts.len(), 1);
        let c = &charts[0];
        assert_eq!(c["name"]["display"], "Anna Freud");
        assert_eq!(c["gender"], "f");
        assert_eq!(c["rodden_rating"], "AA");
        assert_eq!(c["zodiac"]["name"], "tropical");
        assert_eq!(c["birth"]["datetime"]["utc_offset"], "+01:00");
        assert_eq!(c["birth"]["location"]["name"], "Vienna, Austria");
        assert_eq!(c["placements"]["bodies"], serde_json::json!([]));
    }

    #[test]
    fn generator_is_emitted_and_ephemeris_is_omitted() {
        let chart = anna_freud();
        let out = write_file(&[chart], &test_generator());
        let v: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
        let c = &v["charts"][0];
        assert_eq!(c["generator"]["name"], "blackmoon");
        let components: Vec<String> = c["generator"]["components"]
            .as_array()
            .unwrap()
            .iter()
            .map(|comp| comp["name"].as_str().unwrap().to_string())
            .collect();
        assert!(components.contains(&"astrogram".to_string()));
        assert!(components.contains(&"jzod".to_string()));
        assert!(
            c.get("ephemeris").is_none(),
            "blackmoon carries no ephemeris data and must omit the field entirely"
        );
    }

    #[test]
    fn gender_absent_for_event_chart() {
        let mut c = anna_freud();
        c.event_type = EventType::Event;
        let out = write_file(&[c], &test_generator());
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert!(v["charts"][0].get("gender").is_none());
    }

    #[test]
    fn coordinate_system_topocentric() {
        let mut c = anna_freud();
        c.coordinate_system = CoordinateSystem::Topocentric;
        let out = write_file(&[c], &test_generator());
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["charts"][0]["coordinate_system"], "topocentric");
    }

    #[test]
    fn topocentric_round_trips_to_jzod() {
        let mut c = anna_freud();
        c.coordinate_system = CoordinateSystem::Topocentric;
        let out = write_file(&[c], &test_generator());
        let v: serde_json::Value = serde_json::from_str(&out).expect("valid JZOD JSON");
        assert_eq!(v["version"], "0.0.0");
        assert_eq!(v["charts"][0]["coordinate_system"], "topocentric");
    }

    #[test]
    fn sidereal_zodiac_emits_ayanamsha() {
        let mut c = anna_freud();
        c.zodiac = Zodiac::Lahiri;
        let out = write_file(&[c], &test_generator());
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        let z = &v["charts"][0]["zodiac"];
        assert_eq!(z["name"], "sidereal");
        assert_eq!(z["ayanamsha"], "lahiri");
    }

    #[test]
    fn lahiri_emits_canonical_slug_and_true_frame() {
        let mut c = anna_freud();
        c.zodiac = Zodiac::Lahiri;
        let out = write_file(&[c], &test_generator());
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        let z = &v["charts"][0]["zodiac"];
        assert_eq!(z["name"], "sidereal");
        assert_eq!(z["ayanamsha"], "lahiri");
        assert_eq!(z["frame"], "true");
    }

    #[test]
    fn fagan_allen_emits_canonical_slug_and_mean_frame() {
        let mut c = anna_freud();
        c.zodiac = Zodiac::FaganAllen;
        let out = write_file(&[c], &test_generator());
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        let z = &v["charts"][0]["zodiac"];
        assert_eq!(z["name"], "sidereal");
        assert_eq!(z["ayanamsha"], "fagan_bradley");
        assert_eq!(z["frame"], "mean");
    }

    #[test]
    fn raman_emits_canonical_slug_and_mean_frame() {
        let mut c = anna_freud();
        c.zodiac = Zodiac::Raman;
        let out = write_file(&[c], &test_generator());
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        let z = &v["charts"][0]["zodiac"];
        assert_eq!(z["name"], "sidereal");
        assert_eq!(z["ayanamsha"], "raman");
        assert_eq!(z["frame"], "mean");
    }

    #[test]
    fn de_luce_emits_canonical_slug_and_no_frame() {
        let mut c = anna_freud();
        c.zodiac = Zodiac::DeLuce;
        let out = write_file(&[c], &test_generator());
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        let z = &v["charts"][0]["zodiac"];
        assert_eq!(z["name"], "sidereal");
        assert_eq!(z["ayanamsha"], "de_luce");
        assert!(
            z.get("frame").is_none() || z["frame"].is_null(),
            "frame must be absent for de_luce"
        );
    }

    #[test]
    fn other_zodiac_preserves_raw_id_and_no_frame() {
        let mut c = anna_freud();
        c.zodiac = Zodiac::Other(53);
        let out = write_file(&[c], &test_generator());
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        let z = &v["charts"][0]["zodiac"];
        assert_eq!(z["name"], "sidereal");
        assert_eq!(z["ayanamsha"], "other_53");
        assert!(
            z.get("frame").is_none() || z["frame"].is_null(),
            "frame must be absent for Other"
        );
    }

    /// Coupling test: every named sidereal `chart::Zodiac` variant emits a slug
    /// that resolves in the JZOD canonical ayanamsha table.  A new astrogram
    /// variant or a canon edit that breaks alignment fails here before it can
    /// reach production.
    #[test]
    fn all_named_sidereal_variants_emit_resolvable_slug() {
        use Zodiac::{
            DeLuce, DjwhalKhul, FaganAllen, JnBhasin, Krishnamurti, Lahiri, LarryEly, Raman,
            ShillPond, SriYukteswar, SundaraRajan, Svp, TakraI, TakraII, UshaShashi,
        };
        let named_sidereal = [
            FaganAllen,
            Lahiri,
            DeLuce,
            Raman,
            UshaShashi,
            Krishnamurti,
            DjwhalKhul,
            Svp,
            SriYukteswar,
            JnBhasin,
            LarryEly,
            TakraI,
            TakraII,
            SundaraRajan,
            ShillPond,
        ];
        for variant in named_sidereal {
            let jzod_zodiac = zodiac_to_jzod(variant);
            let slug = match &jzod_zodiac {
                jzod::Zodiac::Sidereal {
                    ayanamsha: Some(s), ..
                } => s.clone(),
                other => panic!("expected Sidereal with ayanamsha for {variant:?}, got {other:?}"),
            };
            assert!(
                jzod::ayanamsha::resolve(&slug).is_some(),
                "emitted slug '{slug}' for {variant:?} does not resolve in the canonical table"
            );
        }
    }
}
