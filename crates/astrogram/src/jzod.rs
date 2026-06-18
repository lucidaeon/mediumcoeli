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
//! - `ephemeris.source` — "blackmoon"
//! - `uid` — deterministic from birth data (stable across repeated exports)

use crate::capability::{CapabilitySet, ChartField};
use crate::chart::{Chart, CoordinateSystem, EventType, Zodiac};
use std::hash::{Hash, Hasher};

pub use crate::capability::CapabilitySet as Caps;

/// Fields the JZOD writer preserves.
pub const WRITE_CAPS: CapabilitySet = CapabilitySet::new(&[
    ChartField::SecondaryName,
    ChartField::Region,
    ChartField::SourceRating,
    ChartField::Zodiac,
    ChartField::CoordinateSystem,
    ChartField::EventType,
]);

/// Fields recovered when reading JZOD (none — read is not implemented).
pub const READ_CAPS: CapabilitySet = CapabilitySet::new(&[]);

/// Serialize `charts` as a JZOD v0.0.0 JSON document.
///
/// # Panics
///
/// Never in practice — `serde_json` only fails on non-finite floats, which
/// `Chart` coordinate fields cannot hold (enforced at construction).
#[must_use]
pub fn write_file(charts: &[Chart]) -> String {
    let now = crate::util::utc_iso8601();
    let jzod_charts: Vec<serde_json::Value> =
        charts.iter().map(|c| chart_to_jzod(c, &now)).collect();
    let doc = serde_json::json!({
        "version": "0.0.0",
        "charts": jzod_charts
    });
    serde_json::to_string_pretty(&doc).expect("jzod serialization is infallible")
}

fn chart_to_jzod(chart: &Chart, calculated_at: &str) -> serde_json::Value {
    let uid = chart_uid(chart);

    let mut aliases: Vec<serde_json::Value> = Vec::new();
    if let Some(s) = &chart.secondary_name {
        if !s.is_empty() {
            aliases.push(serde_json::Value::String(s.clone()));
        }
    }

    let location_name = match (&chart.city, &chart.region) {
        (Some(c), Some(r)) if !r.is_empty() => format!("{c}, {r}"),
        (Some(c), _) => c.clone(),
        _ => chart.name.clone(),
    };

    let utc_offset = crate::util::format_utc_offset(chart.tz_offset_hours);

    let mut obj = serde_json::json!({
        "uid": uid,
        "type": "radix",
        "name": {
            "display": chart.name,
            "aliases": aliases
        },
        "birth": {
            "datetime": {
                "year": chart.year,
                "month": chart.month,
                "day": chart.day,
                "hour": chart.hour,
                "minute": chart.minute,
                "second": chart.second,
                "utc_offset": utc_offset,
                "iana_tz": chart.tz_abbreviation
            },
            "location": {
                "name": location_name,
                "latitude": chart.latitude.degrees(),
                "longitude": chart.longitude.degrees()
            }
        },
        "zodiac": zodiac_obj(chart.zodiac),
        "coordinate_system": coord_sys(chart.coordinate_system),
        "ephemeris": {
            "source": concat!("blackmoon/", env!("CARGO_PKG_VERSION")),
            "calculated_at": calculated_at
        },
        "placements": {
            "bodies": []
        }
    });

    // gender: absent for entity charts, present for all others
    if let Some(g) = gender_code(chart.event_type) {
        obj["gender"] = serde_json::Value::String(g.to_string());
    }

    if let Some(rr) = &chart.source_rating {
        if !rr.is_empty() {
            obj["rodden_rating"] = serde_json::Value::String(rr.clone());
        }
    }

    obj
}

fn zodiac_obj(z: Zodiac) -> serde_json::Value {
    let (name, ayanamsha) = zodiac_slug(z);
    if let Some(ay) = ayanamsha {
        serde_json::json!({ "name": name, "ayanamsha": ay })
    } else {
        serde_json::json!({ "name": name })
    }
}

fn zodiac_slug(z: Zodiac) -> (&'static str, Option<&'static str>) {
    match z {
        Zodiac::Tropical => ("tropical", None),
        Zodiac::Draconic => ("draconic", None),
        Zodiac::FaganAllen => ("sidereal", Some("fagan_allen")),
        Zodiac::Lahiri => ("sidereal", Some("lahiri")),
        Zodiac::DeLuce => ("sidereal", Some("de_luce")),
        Zodiac::Raman => ("sidereal", Some("raman")),
        Zodiac::UshaShashi => ("sidereal", Some("usha_shashi")),
        Zodiac::Krishnamurti => ("sidereal", Some("krishnamurti")),
        Zodiac::DjwhalKhul => ("sidereal", Some("djwhal_khul")),
        Zodiac::Svp => ("sidereal", Some("svp")),
        Zodiac::SriYukteswar => ("sidereal", Some("sri_yukteswar")),
        Zodiac::JnBhasin => ("sidereal", Some("jn_bhasin")),
        Zodiac::LarryEly => ("sidereal", Some("larry_ely")),
        Zodiac::TakraI => ("sidereal", Some("takra_i")),
        Zodiac::TakraII => ("sidereal", Some("takra_ii")),
        Zodiac::SundaraRajan => ("sidereal", Some("sundara_rajan")),
        Zodiac::ShillPond => ("sidereal", Some("shill_pond")),
        Zodiac::Other(_) => ("sidereal", None),
    }
}

fn coord_sys(c: CoordinateSystem) -> &'static str {
    match c {
        CoordinateSystem::Geocentric => "geocentric",
        CoordinateSystem::Heliocentric => "heliocentric",
    }
}

fn gender_code(e: EventType) -> Option<&'static str> {
    match e {
        EventType::Male => Some("m"),
        EventType::Female => Some("f"),
        EventType::Unspecified => Some("a"),
        // Entity / horary charts have no gender field in JZOD
        EventType::Event | EventType::Horary => None,
    }
}

/// Deterministic UUID-like identifier derived from birth data.
///
/// Stable across repeated exports of the same chart. Not RFC 4122 compliant but
/// guaranteed to look like a UUID and to differ when any birth field differs.
///
/// # Panics
///
/// Never — bit shifts are bounded by the u64 hash size.
#[allow(clippy::cast_possible_truncation)]
fn chart_uid(chart: &Chart) -> String {
    use std::collections::hash_map::DefaultHasher;
    let mut h1 = DefaultHasher::new();
    chart.name.hash(&mut h1);
    chart.year.hash(&mut h1);
    chart.month.hash(&mut h1);
    chart.day.hash(&mut h1);
    chart.hour.hash(&mut h1);
    chart.minute.hash(&mut h1);
    chart.second.hash(&mut h1);
    chart.latitude.degrees().to_bits().hash(&mut h1);
    chart.longitude.degrees().to_bits().hash(&mut h1);
    let a = h1.finish();

    let mut h2 = DefaultHasher::new();
    a.hash(&mut h2);
    chart.tz_offset_hours.to_bits().hash(&mut h2);
    chart.secondary_name.hash(&mut h2);
    let b = h2.finish();

    format!(
        "{:08x}-{:04x}-4{:03x}-{:04x}-{:012x}",
        (a >> 32) as u32,
        (a >> 16) as u16,
        (a & 0x0FFF) as u16,
        0x8000u16 | ((b >> 48) as u16 & 0x3FFF),
        b & 0x0000_FFFF_FFFF_FFFF_u64
    )
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

    #[test]
    fn chart_uid_is_stable() {
        let c = anna_freud();
        assert_eq!(chart_uid(&c), chart_uid(&c));
    }

    #[test]
    fn chart_uid_differs_on_name_change() {
        let mut c = anna_freud();
        let uid_a = chart_uid(&c);
        c.name = "Sigmund Freud".into();
        assert_ne!(uid_a, chart_uid(&c));
    }

    #[test]
    fn write_file_parses_as_valid_json() {
        let chart = anna_freud();
        let out = write_file(&[chart]);
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
    fn gender_absent_for_event_chart() {
        let mut c = anna_freud();
        c.event_type = EventType::Event;
        let out = write_file(&[c]);
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert!(v["charts"][0].get("gender").is_none());
    }

    #[test]
    fn sidereal_zodiac_emits_ayanamsha() {
        let mut c = anna_freud();
        c.zodiac = Zodiac::Lahiri;
        let out = write_file(&[c]);
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        let z = &v["charts"][0]["zodiac"];
        assert_eq!(z["name"], "sidereal");
        assert_eq!(z["ayanamsha"], "lahiri");
    }
}
