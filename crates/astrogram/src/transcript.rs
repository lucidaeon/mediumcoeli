//! Write→readback transcript: diff a source `Chart` against the chart actually
//! stored by a sink, producing display-ready per-field `source → landed`
//! mappings. Pure and I/O-free so a GUI can render the same report.

use crate::capability::ChartField;
use crate::chart::Chart;

/// How a single field fared across a write→readback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldStatus {
    /// Source and landed values are equal (perfect transcription).
    Preserved,
    /// Both present but differ (normalized, rounded, re-labelled, truncated).
    Transformed,
    /// Source had a value; landed does not (lost).
    Dropped,
    /// Landed has a value the source lacked (filled by the sink).
    Filled,
    /// Neither side has a value; omitted from the transcript.
    Absent,
}

/// One field's `source → landed` mapping, pre-rendered for display.
#[derive(Debug, Clone)]
pub struct FieldMapping {
    /// Human-readable field label, e.g. `"house system"`.
    pub label: &'static str,
    /// Source value rendered for display (`""` when none).
    pub from: String,
    /// Landed value rendered for display (`""` when none).
    pub to: String,
    /// How the field fared.
    pub status: FieldStatus,
    /// Provenance note for the landed value (e.g. `"global setting"`,
    /// `"not supported"`); `None` for ordinary per-chart fields.
    pub note: Option<&'static str>,
}

/// Lowercase the `Debug` rendering of an always-valued enum for display.
fn enum_label<T: std::fmt::Debug>(v: &T) -> String {
    format!("{v:?}").to_lowercase()
}

/// Compose `"city, region"` (or just city, or `""`) for the location field.
fn location(c: &Chart) -> String {
    match (c.city.as_deref(), c.region.as_deref()) {
        (Some(city), Some(region)) if !region.is_empty() => format!("{city}, {region}"),
        (Some(city), _) => city.to_string(),
        (None, _) => String::new(),
    }
}

/// `YYYY-MM-DD HH:MM:SS` from the date/time components.
fn datetime(c: &Chart) -> String {
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        c.year, c.month, c.day, c.hour, c.minute, c.second
    )
}

/// UTC offset as `+HH:MM`, e.g. `"+01:00"`.
fn tz_offset(c: &Chart) -> String {
    crate::util::format_utc_offset(c.tz_offset_hours)
}

/// Timezone abbreviation or IANA name, with `" (LMT)"` appended when flagged.
fn tz_name(c: &Chart) -> String {
    let mut s = c
        .tz_abbreviation
        .as_deref()
        .filter(|a| !a.is_empty())
        .unwrap_or("")
        .to_string();
    if c.is_lmt {
        if s.is_empty() {
            s.push_str("LMT");
        } else {
            s.push_str(" (LMT)");
        }
    }
    s
}

/// Render an `Option<String>` to its value or `""`.
fn opt(o: Option<&String>) -> String {
    o.cloned().unwrap_or_default()
}

/// Classify two already-rendered string values.
fn classify(from: &str, to: &str) -> FieldStatus {
    match (from.is_empty(), to.is_empty()) {
        (true, true) => FieldStatus::Absent,
        (false, true) => FieldStatus::Dropped,
        (true, false) => FieldStatus::Filled,
        (false, false) => {
            if from == to {
                FieldStatus::Preserved
            } else {
                FieldStatus::Transformed
            }
        }
    }
}

/// Look up a provenance note for a `ChartField`.
fn note_for(field_notes: &[(ChartField, &'static str)], field: ChartField) -> Option<&'static str> {
    field_notes
        .iter()
        .find(|(f, _)| *f == field)
        .map(|(_, n)| *n)
}

/// Render a sub-chart count, e.g. `"1 sub-chart"` / `"2 sub-charts"` / `""`.
fn sub_chart_label(n: usize) -> String {
    match n {
        0 => String::new(),
        1 => "1 sub-chart".to_string(),
        k => format!("{k} sub-charts"),
    }
}

/// Build a [`FieldMapping`] for a field with string-rendered values,
/// classifying automatically. Returns `None` when both sides are empty (Absent).
fn mapping(
    label: &'static str,
    from: String,
    to: String,
    note: Option<&'static str>,
) -> Option<FieldMapping> {
    let status = classify(&from, &to);
    if status == FieldStatus::Absent {
        None
    } else {
        Some(FieldMapping {
            label,
            from,
            to,
            status,
            note,
        })
    }
}

/// Diff `source` against `landed`, producing one [`FieldMapping`] per populated
/// field in fixed display order. Fields empty on both sides are omitted.
///
/// `field_notes` tags specific fields' landed values with a provenance note —
/// e.g. `(ChartField::HouseSystem, "global setting")` when the sink renders the
/// house system from an account-wide setting rather than storing it per chart.
/// The caller must populate the corresponding `landed` fields from those
/// settings before calling.
#[must_use]
pub fn diff(
    source: &Chart,
    landed: &Chart,
    field_notes: &[(ChartField, &'static str)],
) -> Vec<FieldMapping> {
    // Coordinates: always present; epsilon decides preserved vs transformed.
    let from_c = format!(
        "{:.4}, {:.4}",
        source.latitude.degrees(),
        source.longitude.degrees()
    );
    let to_c = format!(
        "{:.4}, {:.4}",
        landed.latitude.degrees(),
        landed.longitude.degrees()
    );
    let coord_status = if (source.latitude.degrees() - landed.latitude.degrees()).abs() < 1e-4
        && (source.longitude.degrees() - landed.longitude.degrees()).abs() < 1e-4
    {
        FieldStatus::Preserved
    } else {
        FieldStatus::Transformed
    };

    [
        mapping("name", source.name.clone(), landed.name.clone(), None),
        mapping(
            "secondary name",
            opt(source.secondary_name.as_ref()),
            opt(landed.secondary_name.as_ref()),
            None,
        ),
        mapping("location", location(source), location(landed), None),
        Some(FieldMapping {
            label: "coordinates",
            from: from_c,
            to: to_c,
            status: coord_status,
            note: None,
        }),
        mapping("datetime", datetime(source), datetime(landed), None),
        mapping("tz offset", tz_offset(source), tz_offset(landed), None),
        mapping("timezone", tz_name(source), tz_name(landed), None),
        mapping(
            "event type",
            enum_label(&source.event_type),
            enum_label(&landed.event_type),
            None,
        ),
        mapping(
            "source rating",
            opt(source.source_rating.as_ref()),
            opt(landed.source_rating.as_ref()),
            None,
        ),
        mapping(
            "house system",
            enum_label(&source.house_system),
            enum_label(&landed.house_system),
            note_for(field_notes, ChartField::HouseSystem),
        ),
        mapping(
            "zodiac",
            enum_label(&source.zodiac),
            enum_label(&landed.zodiac),
            note_for(field_notes, ChartField::Zodiac),
        ),
        mapping(
            "coordinate system",
            enum_label(&source.coordinate_system),
            enum_label(&landed.coordinate_system),
            note_for(field_notes, ChartField::CoordinateSystem),
        ),
        mapping(
            "sub-charts",
            sub_chart_label(source.sub_charts.len()),
            sub_chart_label(landed.sub_charts.len()),
            None,
        ),
        mapping(
            "notes",
            opt(source.notes.as_ref()),
            opt(landed.notes.as_ref()),
            None,
        ),
    ]
    .into_iter()
    .flatten()
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::ChartField;
    use crate::chart::{
        Chart, CoordinateSystem, EventType, HouseSystem, Latitude, Longitude, SubChart, Zodiac,
    };

    fn base() -> Chart {
        Chart {
            name: "Anna Freud".into(),
            secondary_name: Some("Freud, Anna".into()),
            city: Some("Vienna".into()),
            region: Some("AT".into()),
            longitude: Longitude::new(16.3717).unwrap(),
            latitude: Latitude::new(48.2083).unwrap(),
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
            source_rating: Some("AA".into()),
            house_system: HouseSystem::Alcabitius,
            zodiac: Zodiac::Lahiri,
            coordinate_system: CoordinateSystem::Heliocentric,
            sub_charts: vec![],
            notes: Some("a note".into()),
        }
    }

    fn find<'a>(m: &'a [FieldMapping], label: &str) -> &'a FieldMapping {
        m.iter().find(|f| f.label == label).expect("label present")
    }

    #[test]
    fn identical_charts_are_all_preserved() {
        let c = base();
        let m = diff(&c, &c, &[]);
        assert!(m.iter().all(|f| f.status == FieldStatus::Preserved));
        assert_eq!(find(&m, "name").from, "Anna Freud");
        assert_eq!(find(&m, "name").to, "Anna Freud");
    }

    #[test]
    fn missing_optional_on_landed_is_dropped() {
        let src = base();
        let mut landed = base();
        landed.secondary_name = None;
        landed.notes = None;
        let m = diff(&src, &landed, &[]);
        assert_eq!(find(&m, "secondary name").status, FieldStatus::Dropped);
        assert_eq!(find(&m, "notes").status, FieldStatus::Dropped);
    }

    #[test]
    fn value_only_on_landed_is_filled() {
        let mut src = base();
        src.notes = None;
        let landed = base();
        let m = diff(&src, &landed, &[]);
        assert_eq!(find(&m, "notes").status, FieldStatus::Filled);
    }

    #[test]
    fn differing_value_is_transformed() {
        let src = base();
        let mut landed = base();
        landed.name = "Anna F.".into();
        let m = diff(&src, &landed, &[]);
        assert_eq!(find(&m, "name").status, FieldStatus::Transformed);
        assert_eq!(find(&m, "name").to, "Anna F.");
    }

    #[test]
    fn coordinates_within_epsilon_preserved_else_transformed() {
        let src = base();
        let mut near = base();
        near.latitude = Latitude::new(48.20835).unwrap(); // Δ < 1e-4
        assert_eq!(
            find(&diff(&src, &near, &[]), "coordinates").status,
            FieldStatus::Preserved
        );
        let mut far = base();
        far.latitude = Latitude::new(48.21).unwrap(); // Δ > 1e-4
        assert_eq!(
            find(&diff(&src, &far, &[]), "coordinates").status,
            FieldStatus::Transformed
        );
    }

    #[test]
    fn both_none_optional_is_absent_and_omitted() {
        let mut src = base();
        let mut landed = base();
        src.secondary_name = None;
        landed.secondary_name = None;
        let m = diff(&src, &landed, &[]);
        assert!(m.iter().all(|f| f.label != "secondary name"));
    }

    #[test]
    fn location_composes_city_and_region() {
        let src = base();
        let m = diff(&src, &src, &[]);
        assert_eq!(find(&m, "location").from, "Vienna, AT");
    }

    #[test]
    fn fixed_field_order() {
        let m = diff(&base(), &base(), &[]);
        let labels: Vec<&str> = m.iter().map(|f| f.label).collect();
        let name_i = labels.iter().position(|l| *l == "name").unwrap();
        let loc_i = labels.iter().position(|l| *l == "location").unwrap();
        let house_i = labels.iter().position(|l| *l == "house system").unwrap();
        assert!(name_i < loc_i && loc_i < house_i);
    }

    #[test]
    fn global_field_gets_note_and_status() {
        let src = base(); // house_system Alcabitius
        let mut landed = base();
        landed.house_system = HouseSystem::Placidus; // account global
        let notes = [(ChartField::HouseSystem, "global setting")];
        let m = diff(&src, &landed, &notes);
        let hs = find(&m, "house system");
        assert_eq!(hs.status, FieldStatus::Transformed);
        assert_eq!(hs.note, Some("global setting"));
        assert_eq!(hs.to, "placidus");
    }

    #[test]
    fn sub_charts_render_count() {
        let mut src = base();
        src.sub_charts = vec![SubChart {
            name: "Event".into(),
            city: None,
            region: None,
            longitude: Longitude::new(0.0).unwrap(),
            latitude: Latitude::new(0.0).unwrap(),
            year: 1900,
            month: 1,
            day: 1,
            hour: 0,
            minute: 0,
            second: 0,
            tz_offset_hours: 0.0,
            tz_abbreviation: None,
            is_lmt: false,
            notes: None,
        }];
        let mut landed = src.clone();
        landed.sub_charts.clear();
        let m = diff(&src, &landed, &[]);
        assert_eq!(find(&m, "sub-charts").from, "1 sub-chart");
        assert_eq!(find(&m, "sub-charts").status, FieldStatus::Dropped);
    }
}
