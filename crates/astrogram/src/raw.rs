//! Raw key: value text writer.
//!
//! Write-only format for inspecting charts — one `key: value` line per field,
//! blank line between charts. Intended for piping into `grep`, `awk`, or
//! human reading. Not designed for round-tripping.

use crate::capability::{CapabilitySet, ChartField};
use crate::chart::{Chart, CoordinateSystem, EventType, HouseSystem, Zodiac};

/// Fields the raw writer emits (all non-trivial per-chart fields).
pub const WRITE_CAPS: CapabilitySet = CapabilitySet::new(&[
    ChartField::SecondaryName,
    ChartField::Region,
    ChartField::SourceRating,
    ChartField::HouseSystem,
    ChartField::Zodiac,
    ChartField::CoordinateSystem,
    ChartField::SubCharts,
    ChartField::Notes,
    ChartField::EventType,
]);

/// Fields recovered when reading raw (none — read is not implemented).
pub const READ_CAPS: CapabilitySet = CapabilitySet::new(&[]);

/// Serialize `charts` as a raw key: value text document.
///
/// Each chart is a block of `key: value` lines separated by blank lines.
#[must_use]
pub fn write_file(charts: &[Chart]) -> String {
    charts
        .iter()
        .map(chart_to_raw)
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn chart_to_raw(c: &Chart) -> String {
    let mut lines: Vec<String> = Vec::new();

    push(&mut lines, "name", &c.name);
    push_opt(&mut lines, "secondary_name", c.secondary_name.as_deref());
    push_opt(&mut lines, "city", c.city.as_deref());
    push_opt(&mut lines, "region", c.region.as_deref());
    lines.push(format!("longitude: {}", c.longitude.degrees()));
    lines.push(format!("latitude: {}", c.latitude.degrees()));
    lines.push(format!(
        "birth: {}-{:02}-{:02} {:02}:{:02}:{:02}",
        c.year, c.month, c.day, c.hour, c.minute, c.second
    ));
    lines.push(format!(
        "tz_offset: {}",
        crate::util::format_utc_offset(c.tz_offset_hours)
    ));
    push_opt(&mut lines, "tz_abbreviation", c.tz_abbreviation.as_deref());
    lines.push(format!("is_lmt: {}", c.is_lmt));
    lines.push(format!("event_type: {}", event_type_name(c.event_type)));
    push_opt(&mut lines, "source_rating", c.source_rating.as_deref());
    lines.push(format!(
        "house_system: {}",
        house_system_name(c.house_system)
    ));
    lines.push(format!("zodiac: {}", zodiac_name(c.zodiac)));
    lines.push(format!(
        "coordinate_system: {}",
        coord_sys_name(c.coordinate_system)
    ));
    if !c.sub_charts.is_empty() {
        lines.push(format!("sub_charts: {}", c.sub_charts.len()));
        for (i, s) in c.sub_charts.iter().enumerate() {
            lines.push(format!("  sub_charts[{i}].name: {}", s.name));
        }
    }
    push_opt(&mut lines, "notes", c.notes.as_deref());

    lines.join("\n")
}

fn push(out: &mut Vec<String>, key: &str, val: &str) {
    out.push(format!("{key}: {val}"));
}

fn push_opt(out: &mut Vec<String>, key: &str, val: Option<&str>) {
    if let Some(v) = val {
        out.push(format!("{key}: {v}"));
    }
}

fn event_type_name(e: EventType) -> &'static str {
    match e {
        EventType::Unspecified => "unspecified",
        EventType::Male => "male",
        EventType::Female => "female",
        EventType::Event => "event",
        EventType::Horary => "horary",
    }
}

fn house_system_name(h: HouseSystem) -> String {
    match h {
        HouseSystem::Campanus => "campanus".into(),
        HouseSystem::Koch => "koch".into(),
        HouseSystem::Meridian => "meridian".into(),
        HouseSystem::Morinus => "morinus".into(),
        HouseSystem::Placidus => "placidus".into(),
        HouseSystem::Porphyry => "porphyry".into(),
        HouseSystem::Regiomontanus => "regiomontanus".into(),
        HouseSystem::Topocentric => "topocentric".into(),
        HouseSystem::Equal => "equal".into(),
        HouseSystem::ZeroAries => "zero_aries".into(),
        HouseSystem::SolarSign => "solar_sign".into(),
        HouseSystem::WholeSign => "whole_sign".into(),
        HouseSystem::HinduBhava => "hindu_bhava".into(),
        HouseSystem::Alcabitius => "alcabitius".into(),
        HouseSystem::Other(n) => format!("other({n})"),
    }
}

fn zodiac_name(z: Zodiac) -> &'static str {
    match z {
        Zodiac::Tropical => "tropical",
        Zodiac::FaganAllen => "fagan_allen",
        Zodiac::Lahiri => "lahiri",
        Zodiac::DeLuce => "de_luce",
        Zodiac::Raman => "raman",
        Zodiac::UshaShashi => "usha_shashi",
        Zodiac::Krishnamurti => "krishnamurti",
        Zodiac::DjwhalKhul => "djwhal_khul",
        Zodiac::Draconic => "draconic",
        Zodiac::Svp => "svp",
        Zodiac::SriYukteswar => "sri_yukteswar",
        Zodiac::JnBhasin => "jn_bhasin",
        Zodiac::LarryEly => "larry_ely",
        Zodiac::TakraI => "takra_i",
        Zodiac::TakraII => "takra_ii",
        Zodiac::SundaraRajan => "sundara_rajan",
        Zodiac::ShillPond => "shill_pond",
        Zodiac::Other(_) => "other",
    }
}

fn coord_sys_name(c: CoordinateSystem) -> &'static str {
    match c {
        CoordinateSystem::Geocentric => "geocentric",
        CoordinateSystem::Heliocentric => "heliocentric",
    }
}
