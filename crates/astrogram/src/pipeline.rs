//! Pure decision helpers for the convert pipeline: what fields drop, which
//! charts need fills, and structured readback verification. The binary keeps
//! orchestration and I/O; these functions hold the reusable logic a GUI shares.

use crate::capability::ChartField;
use crate::chart::{Chart, CoordinateSystem, HouseSystem, Zodiac};
use crate::format::Format;
use crate::provider::{DatetimeKey, GlobalRender, key};
use crate::transcript::FieldMapping;
use std::collections::BTreeSet;
use std::collections::HashMap;

/// Cross-batch field-loss summary for a sink: how many charts lose data and the
/// deduplicated, sorted set of field labels dropped.
pub struct DropSummary {
    /// Number of charts that lose at least one field.
    pub affected: usize,
    /// Sorted, deduplicated labels of every dropped field across the batch.
    pub fields: Vec<&'static str>,
}

/// Compute the [`DropSummary`] for writing `merged` to `sink`, consulting
/// `source_of` (keyed by [`key`]) so each chart is judged against its own
/// source's capabilities (defaulting to `sink` when unknown).
#[must_use]
pub fn drop_summary<S: ::std::hash::BuildHasher>(
    merged: &[Chart],
    source_of: &HashMap<DatetimeKey, Format, S>,
    sink: Format,
) -> DropSummary {
    use crate::capability::lost_fields;
    let mut affected = 0usize;
    let mut all: BTreeSet<&'static str> = BTreeSet::new();
    for chart in merged {
        let source = source_of.get(&key(chart)).copied().unwrap_or(sink);
        let lost = lost_fields(chart, source, sink);
        if !lost.is_empty() {
            affected += 1;
            for f in lost {
                all.insert(f.label());
            }
        }
    }
    DropSummary {
        affected,
        fields: all.into_iter().collect(),
    }
}

/// Record the source `format` of each chart in `batch` into `source_of`,
/// keeping the **first** format seen for a given [`key`] (matching
/// [`crate::consolidate::merge`]'s keep-first dedup). Charts already present
/// under their key are left untouched.
///
/// Front-ends build this provenance map as they read each input batch, so later
/// stages ([`drop_summary`], [`fill_targets`]) can judge every chart against the
/// capabilities of the format it actually came from.
pub fn record_sources<S: ::std::hash::BuildHasher>(
    source_of: &mut HashMap<DatetimeKey, Format, S>,
    batch: &[Chart],
    format: Format,
) {
    for chart in batch {
        source_of.entry(key(chart)).or_insert(format);
    }
}

/// The union of [`crate::capability::fill_fields`] across every distinct source
/// format present in `source_of`, for writing to `sink`.
///
/// This is the set of settings fields (house system / zodiac / locus) that at
/// least one source never carried but `sink` demands — so a mixed batch (e.g.
/// some ADB charts, some `SFcht`) yields exactly the fields that need filling.
/// Duplicates are removed (first occurrence wins).
#[must_use]
pub fn fill_fields_needed<S: ::std::hash::BuildHasher>(
    source_of: &HashMap<DatetimeKey, Format, S>,
    sink: Format,
) -> Vec<ChartField> {
    let sources: std::collections::HashSet<Format> = source_of.values().copied().collect();
    let mut needed: Vec<ChartField> = Vec::new();
    for src in sources {
        for f in crate::capability::fill_fields(src, sink) {
            if !needed.contains(&f) {
                needed.push(f);
            }
        }
    }
    needed
}

/// A resolved fill value for a single capability field.
#[derive(Clone, Copy)]
pub enum FillValue {
    /// House-system fill.
    House(HouseSystem),
    /// Zodiac fill.
    Zodiac(Zodiac),
    /// Coordinate-system (locus) fill.
    Coord(CoordinateSystem),
}

/// Everything a front-end needs to resolve a fill for one settings field:
/// display label, flag/parameter suffix, suggested default, and the parser
/// producing the typed [`FillValue`].
pub struct FillSpec {
    /// The field this spec resolves.
    pub field: crate::capability::ChartField,
    /// Human label — matches [`crate::capability::ChartField::label`].
    pub label: &'static str,
    /// CLI flag / GUI parameter suffix (e.g. `"house"` for `--fill-house`).
    pub flag_suffix: &'static str,
    /// Suggested default slug offered when the user gives no value.
    pub default_slug: &'static str,
    /// Canonical slugs this field accepts — mirrors the matching enum's
    /// `accepted_slugs()`, so prompts and non-interactive-stdin failures can
    /// enumerate valid choices.
    pub accepted: &'static [&'static str],
    /// Parse a slug into the typed fill value; `None` = unknown slug.
    pub parse: fn(&str) -> Option<FillValue>,
}

fn parse_fill_house(s: &str) -> Option<FillValue> {
    HouseSystem::from_str_slug(s).map(FillValue::House)
}
fn parse_fill_zodiac(s: &str) -> Option<FillValue> {
    Zodiac::from_str_slug(s).map(FillValue::Zodiac)
}
fn parse_fill_coord(s: &str) -> Option<FillValue> {
    CoordinateSystem::from_str_slug(s).map(FillValue::Coord)
}

/// The fill spec table — one entry per [`crate::capability::NON_OMITTABLE`] field.
/// Every `NON_OMITTABLE` member must have a corresponding entry here; the
/// `fill_specs_cover_every_non_omittable_field` pin test enforces this.
pub const FILL_SPECS: &[FillSpec] = &[
    FillSpec {
        field: crate::capability::ChartField::HouseSystem,
        label: "house system",
        flag_suffix: "house",
        default_slug: "placidus",
        accepted: HouseSystem::accepted_slugs(),
        parse: parse_fill_house,
    },
    FillSpec {
        field: crate::capability::ChartField::Zodiac,
        label: "zodiac",
        flag_suffix: "zodiac",
        default_slug: "tropical",
        accepted: Zodiac::accepted_slugs(),
        parse: parse_fill_zodiac,
    },
    FillSpec {
        field: crate::capability::ChartField::CoordinateSystem,
        label: "coordinate system",
        flag_suffix: "locus",
        default_slug: "geocentric",
        accepted: CoordinateSystem::accepted_slugs(),
        parse: parse_fill_coord,
    },
];

/// Spec for one field; `None` when the field is not fillable.
#[must_use]
pub fn fill_spec(field: crate::capability::ChartField) -> Option<&'static FillSpec> {
    FILL_SPECS.iter().find(|s| s.field == field)
}

/// Indices of charts in `merged` whose source (per `source_of`, default `sink`)
/// did NOT preserve `field` — i.e. the charts that need a fill for it.
#[must_use]
pub fn fill_targets<S: ::std::hash::BuildHasher>(
    merged: &[Chart],
    field: ChartField,
    source_of: &HashMap<DatetimeKey, Format, S>,
    sink: Format,
) -> Vec<usize> {
    merged
        .iter()
        .enumerate()
        .filter(|(_, c)| {
            let src = source_of.get(&key(c)).copied().unwrap_or(sink);
            !src.read_caps().preserves(field)
        })
        .map(|(i, _)| i)
        .collect()
}

/// Apply a resolved `value` to the charts at `indices` (in place).
pub fn apply_fill_value(merged: &mut [Chart], value: FillValue, indices: &[usize]) {
    for &i in indices {
        match value {
            FillValue::House(v) => merged[i].house_system = v,
            FillValue::Zodiac(v) => merged[i].zodiac = v,
            FillValue::Coord(v) => merged[i].coordinate_system = v,
        }
    }
}

/// The readback outcome for one written chart.
pub enum LandedOutcome {
    /// No landed chart paired to this source on readback.
    NotFound,
    /// Paired and diffed against the (optionally global-folded) landed chart.
    Diffed(Vec<FieldMapping>),
}

/// One row of a readback verification: the source name, its write-status string
/// (`Some` for newly-written charts, `None` for pre-existing), and the outcome.
pub struct VerifyRow {
    /// Source chart name (header label).
    pub name: String,
    /// Write status string, if this chart was newly written.
    pub write_status: Option<String>,
    /// Pairing/diff outcome.
    pub outcome: LandedOutcome,
}

/// Pair each written chart to its landed counterpart, fold in `global` render
/// settings, and diff — returning structured rows the caller formats. Pure: no
/// I/O. (The caller fetches `landed_all` and `global` and does the printing.)
#[must_use]
pub fn verify_rows(
    written: &[Chart],
    landed_all: &[Chart],
    write_results: &[Option<String>],
    global: Option<&GlobalRender>,
) -> Vec<VerifyRow> {
    let pairing = crate::transcript::pair_landed(written, landed_all);
    let mut rows = Vec::with_capacity(written.len());
    for ((src, maybe_idx), status) in written.iter().zip(pairing).zip(write_results.iter()) {
        let outcome = match maybe_idx {
            None => LandedOutcome::NotFound,
            Some(i) => {
                let mut landed = landed_all[i].clone();
                let notes: &[(ChartField, &'static str)] = if let Some(g) = global {
                    g.apply_to(&mut landed);
                    &g.field_notes
                } else {
                    &[]
                };
                LandedOutcome::Diffed(crate::transcript::diff(src, &landed, notes))
            }
        };
        rows.push(VerifyRow {
            name: src.name.clone(),
            write_status: status.clone(),
            outcome,
        });
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pin: every `NON_OMITTABLE` field has a [`FillSpec`], every `FILL_SPECS` entry's
    /// field is in `NON_OMITTABLE`, each spec's label matches [`ChartField::label()`],
    /// and each `parse(default_slug)` returns the expected [`FillValue`] variant.
    /// Adding a fourth `NON_OMITTABLE` field without a `FILL_SPECS` entry fails here.
    #[test]
    fn fill_specs_cover_every_non_omittable_field() {
        use crate::capability::{ChartField, NON_OMITTABLE};

        // Every NON_OMITTABLE field must have a fill spec.
        for &field in NON_OMITTABLE {
            let spec = fill_spec(field).unwrap_or_else(|| {
                panic!("{field:?} is NON_OMITTABLE but has no FillSpec in FILL_SPECS")
            });
            assert_eq!(
                spec.label,
                field.label(),
                "FillSpec label for {field:?} must match ChartField::label()",
            );
            let parsed = (spec.parse)(spec.default_slug).unwrap_or_else(|| {
                panic!(
                    "FillSpec::parse({field:?}) returned None for default_slug {:?}",
                    spec.default_slug
                )
            });
            // Each variant must match its field.
            match (field, parsed) {
                (ChartField::HouseSystem, FillValue::House(_))
                | (ChartField::Zodiac, FillValue::Zodiac(_))
                | (ChartField::CoordinateSystem, FillValue::Coord(_)) => {}
                (f, v) => panic!(
                    "parse({f:?}.default_slug) returned wrong FillValue variant: {:?}",
                    match v {
                        FillValue::House(_) => "House",
                        FillValue::Zodiac(_) => "Zodiac",
                        FillValue::Coord(_) => "Coord",
                    }
                ),
            }
        }

        // Every FILL_SPECS entry's field must be in NON_OMITTABLE.
        for spec in FILL_SPECS {
            assert!(
                NON_OMITTABLE.contains(&spec.field),
                "FILL_SPECS has an entry for {:?} which is not in NON_OMITTABLE",
                spec.field
            );
        }

        // Counts must agree — no extras, no gaps.
        assert_eq!(
            FILL_SPECS.len(),
            NON_OMITTABLE.len(),
            "FILL_SPECS and NON_OMITTABLE must have the same number of entries"
        );
    }

    /// Each `FILL_SPECS` row's `accepted` list must be non-empty and equal to
    /// the matching enum's `accepted_slugs()`, so prompts built from
    /// `FILL_SPECS` enumerate exactly the values `parse` will accept.
    #[test]
    fn fill_specs_accepted_matches_enum_accepted_slugs() {
        for spec in FILL_SPECS {
            assert!(
                !spec.accepted.is_empty(),
                "FILL_SPECS entry for {:?} has an empty accepted list",
                spec.field
            );
        }

        let house = fill_spec(crate::capability::ChartField::HouseSystem).unwrap();
        assert_eq!(house.accepted, HouseSystem::accepted_slugs());

        let zodiac = fill_spec(crate::capability::ChartField::Zodiac).unwrap();
        assert_eq!(zodiac.accepted, Zodiac::accepted_slugs());

        let coord = fill_spec(crate::capability::ChartField::CoordinateSystem).unwrap();
        assert_eq!(coord.accepted, CoordinateSystem::accepted_slugs());
    }

    #[test]
    fn record_sources_keeps_first_seen_format() {
        let c = crate::test_support::fully_populated();
        let mut source_of: HashMap<DatetimeKey, Format> = HashMap::new();
        record_sources(&mut source_of, std::slice::from_ref(&c), Format::Sfcht);
        // A later batch re-observing the same chart under a different format
        // must NOT overwrite the first-seen source.
        record_sources(&mut source_of, std::slice::from_ref(&c), Format::Luna);
        assert_eq!(source_of.get(&key(&c)), Some(&Format::Sfcht));
    }

    #[test]
    fn fill_fields_needed_unions_across_sources() {
        // Luna source lacks house/zodiac/coord that an SFcht sink preserves;
        // an SFcht source lacks none. The union is exactly Luna's fill set.
        let mut luna_chart = crate::test_support::fully_populated();
        luna_chart.name = "FromLuna".into();
        let mut sfcht_chart = crate::test_support::fully_populated();
        sfcht_chart.name = "FromSfcht".into();
        let mut source_of: HashMap<DatetimeKey, Format> = HashMap::new();
        source_of.insert(key(&luna_chart), Format::Luna);
        source_of.insert(key(&sfcht_chart), Format::Sfcht);

        let needed = fill_fields_needed(&source_of, Format::Sfcht);
        let expected = crate::capability::fill_fields(Format::Luna, Format::Sfcht);
        // Same membership regardless of set-iteration order.
        for f in &needed {
            assert!(expected.contains(f));
        }
        assert_eq!(needed.len(), expected.len());
        assert!(!needed.is_empty(), "Luna→SFcht must need fills");
    }

    #[test]
    fn fill_fields_needed_empty_when_no_loss() {
        let c = crate::test_support::fully_populated();
        let mut source_of: HashMap<DatetimeKey, Format> = HashMap::new();
        source_of.insert(key(&c), Format::Sfcht);
        // SFcht → SFcht needs no fills.
        assert!(fill_fields_needed(&source_of, Format::Sfcht).is_empty());
    }

    #[test]
    fn fill_targets_selects_charts_whose_source_lacked_field() {
        // Two charts: one from Sfcht (carries house system), one from Adb-like
        // source that does not. We model "lacking" via source_of mapping.
        let mut a = crate::test_support::fully_populated();
        a.name = "HasHouse".into();
        let mut b = crate::test_support::fully_populated();
        b.name = "NoHouse".into();
        let mut source_of: HashMap<DatetimeKey, Format> = HashMap::new();
        source_of.insert(key(&a), Format::Sfcht); // preserves HouseSystem
        source_of.insert(key(&b), Format::Luna); // does not
        let targets = fill_targets(&[a, b], ChartField::HouseSystem, &source_of, Format::Sfcht);
        assert_eq!(targets, vec![1]); // only the Luna-sourced chart
    }

    #[test]
    fn apply_fill_value_sets_only_targeted_indices() {
        let mut a = crate::test_support::fully_populated();
        let mut b = crate::test_support::fully_populated();
        a.house_system = HouseSystem::Placidus;
        b.house_system = HouseSystem::Placidus;
        let mut v = vec![a, b];
        apply_fill_value(&mut v, FillValue::House(HouseSystem::WholeSign), &[1]);
        assert_eq!(v[0].house_system, HouseSystem::Placidus);
        assert_eq!(v[1].house_system, HouseSystem::WholeSign);
    }

    #[test]
    fn drop_summary_counts_charts_losing_data() {
        // Sfcht source preserves everything; Luna sink drops several fields.
        let c = crate::test_support::fully_populated();
        let mut source_of: HashMap<DatetimeKey, Format> = HashMap::new();
        source_of.insert(key(&c), Format::Sfcht);
        let s = drop_summary(&[c], &source_of, Format::Luna);
        assert_eq!(s.affected, 1);
        assert!(!s.fields.is_empty());
    }

    #[test]
    fn drop_summary_empty_when_no_loss() {
        let c = crate::test_support::fully_populated();
        let mut source_of: HashMap<DatetimeKey, Format> = HashMap::new();
        source_of.insert(key(&c), Format::Sfcht);
        // Sfcht → Sfcht loses nothing.
        let s = drop_summary(&[c], &source_of, Format::Sfcht);
        assert_eq!(s.affected, 0);
        assert!(s.fields.is_empty());
    }

    #[test]
    fn verify_rows_empty_inputs_yield_empty() {
        let rows = verify_rows(&[], &[], &[], None);
        assert!(rows.is_empty());
    }

    #[test]
    fn verify_rows_marks_unpaired_as_not_found() {
        let src = crate::test_support::fully_populated();
        let rows = verify_rows(&[src], &[], &[None], None);
        assert_eq!(rows.len(), 1);
        assert!(matches!(rows[0].outcome, LandedOutcome::NotFound));
        assert_eq!(rows[0].write_status, None);
    }

    #[test]
    fn verify_rows_diffed_when_paired() {
        // Two charts sharing the same temporal key (year/month/day/hour/minute/second)
        // so `pair_landed` pairs them. The landed chart is a clone of the written
        // chart, so the diff reports all fields as preserved.
        let written = crate::test_support::fully_populated();
        let landed = written.clone();
        let rows = verify_rows(&[written], &[landed], &[Some("created".into())], None);
        assert_eq!(rows.len(), 1);
        assert!(matches!(rows[0].outcome, LandedOutcome::Diffed(_)));
        assert_eq!(rows[0].write_status, Some("created".into()));
    }
}
