//! Shared fixtures for the per-format capability round-trip tests.
//!
//! Each writable format asserts that the fields surviving a write→read
//! round-trip exactly match its declared `WRITE_CAPS`. The fixture and the
//! survivor-detection logic are format-independent, so they live here rather
//! than being copied into every format module's test block.

use crate::capability::ChartField;
use crate::chart::{
    Chart, CoordinateSystem, EventType, HouseSystem, Latitude, Longitude, SubChart, Zodiac,
};

/// A chart populating every lossy field, using allow-listed reference data
/// (Anna Freud, Vienna — `skills/astrologer/fixtures/ref_anna_freud_alcabitius.md`).
///
/// Non-default `HouseSystem`, `Zodiac`, and `CoordinateSystem` values are
/// deliberate: a format that hard-codes the defaults on read would otherwise
/// appear to preserve these fields by coincidence.
pub(crate) fn fully_populated() -> Chart {
    Chart {
        name: "Anna Freud".into(),
        secondary_name: Some("Freud, Anna".into()),
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
        tz_abbreviation: Some("CET".into()),
        is_lmt: false,
        event_type: EventType::Female,
        source_rating: Some("AA Himself to Astrolabe".into()),
        house_system: HouseSystem::Alcabitius,
        zodiac: Zodiac::Lahiri,
        coordinate_system: CoordinateSystem::Heliocentric,
        sub_charts: vec![SubChart {
            name: "Event".into(),
            city: None,
            region: None,
            longitude: Longitude::new(16.371_667).unwrap(),
            latitude: Latitude::new(48.208_333).unwrap(),
            year: 1900,
            month: 1,
            day: 1,
            hour: 0,
            minute: 0,
            second: 0,
            tz_offset_hours: 1.0,
            tz_abbreviation: None,
            is_lmt: false,
            notes: Some("sub note".into()),
        }],
        notes: Some("a note".into()),
    }
}

/// Which lossy fields survived a write→read round-trip.
pub(crate) fn survivors(original: &Chart, restored: &Chart) -> Vec<ChartField> {
    let mut out = Vec::new();
    if restored.secondary_name == original.secondary_name && original.secondary_name.is_some() {
        out.push(ChartField::SecondaryName);
    }
    if restored.region == original.region && original.region.is_some() {
        out.push(ChartField::Region);
    }
    if restored.source_rating == original.source_rating && original.source_rating.is_some() {
        out.push(ChartField::SourceRating);
    }
    if restored.house_system == original.house_system {
        out.push(ChartField::HouseSystem);
    }
    if restored.zodiac == original.zodiac {
        out.push(ChartField::Zodiac);
    }
    if restored.coordinate_system == original.coordinate_system {
        out.push(ChartField::CoordinateSystem);
    }
    if !restored.sub_charts.is_empty() {
        out.push(ChartField::SubCharts);
    }
    if restored.notes == original.notes && original.notes.is_some() {
        out.push(ChartField::Notes);
    }
    if restored.event_type == original.event_type && original.event_type != EventType::Unspecified {
        out.push(ChartField::EventType);
    }
    out
}
