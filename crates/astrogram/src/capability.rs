//! Per-field capability vocabulary for lossy-conversion detection.
//!
//! [`ChartField`] enumerates the canonical [`crate::chart::Chart`] fields that
//! some format does not persist. Universal-core fields (name, date, time,
//! latitude, longitude, city) are carried by every writable format and are not
//! members. `tz_abbreviation` is excluded: an IANA tz id vs an abbreviation is a
//! representation change, not data loss.

/// A canonical chart field whose support varies across formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChartField {
    /// Secondary/alternate name field (`SFcht` 50-char slot).
    SecondaryName,
    /// Region / state / country sub-locality.
    Region,
    /// Rodden rating (source reliability).
    SourceRating,
    /// House system (per-chart; only `SFcht` persists this).
    HouseSystem,
    /// Zodiac (tropical/sidereal/etc.; only `SFcht` persists this).
    Zodiac,
    /// Geocentric/heliocentric locus (only `SFcht` persists this).
    CoordinateSystem,
    /// Attached sub-charts / associated events (only `SFcht`).
    SubCharts,
    /// Free-text notes.
    Notes,
    /// Chart/subject type (natal/event/horary/…).
    EventType,
}

impl ChartField {
    /// Human-readable label used in disclosures.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            ChartField::SecondaryName => "secondary name",
            ChartField::Region => "region",
            ChartField::SourceRating => "source rating",
            ChartField::HouseSystem => "house system",
            ChartField::Zodiac => "zodiac",
            ChartField::CoordinateSystem => "coordinate system",
            ChartField::SubCharts => "sub-charts",
            ChartField::Notes => "notes",
            ChartField::EventType => "event type",
        }
    }
}

/// The set of [`ChartField`]s a format's data carries.
#[derive(Debug, Clone, Copy)]
pub struct CapabilitySet(&'static [ChartField]);

impl CapabilitySet {
    /// Construct from a static field list.
    #[must_use]
    pub const fn new(fields: &'static [ChartField]) -> Self {
        Self(fields)
    }

    /// Whether this set preserves `field`.
    #[must_use]
    pub fn preserves(self, field: ChartField) -> bool {
        self.0.contains(&field)
    }

    /// The preserved fields (for UI "stores: …" display).
    #[must_use]
    pub fn fields(self) -> &'static [ChartField] {
        self.0
    }
}

use crate::chart::{Chart, EventType};
use crate::format::Format;

/// Fields a writer cannot leave blank — the always-valued `Chart` enums.
pub const NON_OMITTABLE: &[ChartField] = &[
    ChartField::HouseSystem,
    ChartField::Zodiac,
    ChartField::CoordinateSystem,
];

/// Loss-prone fields this chart holds a value for. The three always-valued
/// enums are always counted here; provenance (was the value real?) is applied
/// in [`lost_fields`] via the source's read capabilities.
#[must_use]
pub fn populated_lossy_fields(chart: &Chart) -> Vec<ChartField> {
    let mut out = Vec::new();
    let has = |o: &Option<String>| o.as_deref().is_some_and(|s| !s.is_empty());
    if has(&chart.secondary_name) {
        out.push(ChartField::SecondaryName);
    }
    if has(&chart.region) {
        out.push(ChartField::Region);
    }
    if has(&chart.source_rating) {
        out.push(ChartField::SourceRating);
    }
    out.push(ChartField::HouseSystem);
    out.push(ChartField::Zodiac);
    out.push(ChartField::CoordinateSystem);
    if !chart.sub_charts.is_empty() {
        out.push(ChartField::SubCharts);
    }
    if has(&chart.notes) {
        out.push(ChartField::Notes);
    }
    if chart.event_type != EventType::Unspecified {
        out.push(ChartField::EventType);
    }
    out
}

/// Fields this chart loses going `source` → `sink`:
/// the source genuinely carried it, the sink cannot persist it, and the chart
/// actually populates it.
#[must_use]
pub fn lost_fields(chart: &Chart, source: Format, sink: Format) -> Vec<ChartField> {
    let src = source.read_caps();
    let dst = sink.write_caps();
    populated_lossy_fields(chart)
        .into_iter()
        .filter(|&f| src.preserves(f) && !dst.preserves(f))
        .collect()
}

/// Non-omittable fields the `sink` demands that the `source` never carried — the
/// writer would otherwise invent a value. The caller must supply these.
#[must_use]
pub fn fill_fields(source: Format, sink: Format) -> Vec<ChartField> {
    let src = source.read_caps();
    let dst = sink.write_caps();
    NON_OMITTABLE
        .iter()
        .copied()
        .filter(|&f| dst.preserves(f) && !src.preserves(f))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_reflects_membership() {
        let caps = CapabilitySet::new(&[ChartField::Notes, ChartField::Region]);
        assert!(caps.preserves(ChartField::Notes));
        assert!(caps.preserves(ChartField::Region));
        assert!(!caps.preserves(ChartField::Zodiac));
    }

    #[test]
    fn empty_set_preserves_nothing() {
        let caps = CapabilitySet::new(&[]);
        assert!(!caps.preserves(ChartField::Notes));
        assert_eq!(caps.fields().len(), 0);
    }

    #[test]
    fn label_is_lowercase_human_readable() {
        assert_eq!(ChartField::CoordinateSystem.label(), "coordinate system");
        assert_eq!(ChartField::SourceRating.label(), "source rating");
    }

    use crate::chart::{
        Chart, CoordinateSystem, EventType, HouseSystem, Latitude, Longitude, Zodiac,
    };
    use crate::format::Format;

    fn bare_chart() -> Chart {
        Chart {
            name: "x".into(),
            secondary_name: None,
            city: Some("c".into()),
            region: None,
            longitude: Longitude::new(0.0).unwrap(),
            latitude: Latitude::new(0.0).unwrap(),
            year: 2000,
            month: 1,
            day: 1,
            hour: 0,
            minute: 0,
            second: 0,
            tz_offset_hours: 0.0,
            tz_abbreviation: None,
            is_lmt: false,
            event_type: EventType::Unspecified,
            source_rating: None,
            house_system: HouseSystem::Placidus,
            zodiac: Zodiac::Tropical,
            coordinate_system: CoordinateSystem::Geocentric,
            sub_charts: vec![],
            notes: None,
        }
    }

    #[test]
    fn populated_counts_only_real_optional_data() {
        let mut c = bare_chart();
        assert!(populated_lossy_fields(&c).contains(&ChartField::HouseSystem)); // always-valued
        assert!(!populated_lossy_fields(&c).contains(&ChartField::Notes));
        c.notes = Some("hi".into());
        c.region = Some(String::new()); // empty → not counted
        assert!(populated_lossy_fields(&c).contains(&ChartField::Notes));
        assert!(!populated_lossy_fields(&c).contains(&ChartField::Region));
    }

    #[test]
    fn sfcht_helio_with_notes_to_astrocom_loses_coord_and_notes() {
        let mut c = bare_chart();
        c.coordinate_system = CoordinateSystem::Heliocentric;
        c.notes = Some("n".into());
        let lost = lost_fields(&c, Format::Sfcht, Format::Astrocom);
        assert!(lost.contains(&ChartField::CoordinateSystem));
        assert!(lost.contains(&ChartField::Notes));
    }

    #[test]
    fn adb_source_never_flags_settings_as_lost() {
        let c = bare_chart(); // house/zodiac/coord always-valued but ADB never carried them
        let lost = lost_fields(&c, Format::Adb, Format::Astrocom);
        assert!(!lost.contains(&ChartField::HouseSystem));
        assert!(!lost.contains(&ChartField::Zodiac));
        assert!(!lost.contains(&ChartField::CoordinateSystem));
    }

    #[test]
    fn astrocom_roundtrip_loses_region() {
        let mut c = bare_chart();
        c.region = Some("Austria".into());
        let lost = lost_fields(&c, Format::Astrocom, Format::Astrocom);
        assert_eq!(lost, vec![ChartField::Region]);
    }

    #[test]
    fn fill_fields_adb_to_sfcht_is_the_three_settings() {
        let fills = fill_fields(Format::Adb, Format::Sfcht);
        assert!(fills.contains(&ChartField::HouseSystem));
        assert!(fills.contains(&ChartField::Zodiac));
        assert!(fills.contains(&ChartField::CoordinateSystem));
        assert_eq!(fills.len(), 3);
    }

    #[test]
    fn fill_fields_sfcht_to_sfcht_is_empty() {
        assert!(fill_fields(Format::Sfcht, Format::Sfcht).is_empty());
    }
}
