//! Tests for chart consolidation / deduplication.

use astrogram::chart::{
    Chart, CoordinateSystem, EventType, HouseSystem, Latitude, Longitude, Zodiac,
};
use astrogram::consolidate::{
    find_candidate, group_candidates, is_candidate, merge, merge_reporting,
};

#[allow(clippy::too_many_arguments)]
fn chart(
    name: &str,
    lat: f64,
    lon: f64,
    year: i16,
    month: u8,
    day: u8,
    hour: u8,
    minute: u8,
    second: u8,
) -> Chart {
    Chart {
        name: name.to_string(),
        secondary_name: None,
        city: None,
        region: None,
        longitude: Longitude::new(lon).unwrap(),
        latitude: Latitude::new(lat).unwrap(),
        year,
        month,
        day,
        hour,
        minute,
        second,
        tz_offset_hours: 0.0,
        tz_abbreviation: None,
        is_lmt: false,
        event_type: EventType::Male,
        source_rating: None,
        house_system: HouseSystem::Placidus,
        zodiac: Zodiac::Tropical,
        coordinate_system: CoordinateSystem::Geocentric,
        sub_charts: vec![],
        notes: None,
    }
}

// --- no-op cases ---

#[test]
fn empty_inputs_give_empty_output() {
    assert!(merge(&[]).is_empty());
}

#[test]
fn single_empty_batch_gives_empty_output() {
    assert!(merge(&[vec![]]).is_empty());
}

#[test]
fn single_chart_passes_through() {
    let c = chart("Amber", 51.5, -0.117, 1815, 12, 10, 7, 30, 0);
    let out = merge(&[vec![c.clone()]]);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].name, "Amber");
}

// --- deduplication: exact match ---

#[test]
fn identical_charts_in_same_batch_deduplicated() {
    let c = chart("Amber", 51.5, -0.117, 1815, 12, 10, 7, 30, 0);
    let out = merge(&[vec![c.clone(), c.clone()]]);
    assert_eq!(out.len(), 1);
}

#[test]
fn identical_charts_across_batches_deduplicated() {
    let c = chart("Amber", 51.5, -0.117, 1815, 12, 10, 7, 30, 0);
    let out = merge(&[vec![c.clone()], vec![c.clone()]]);
    assert_eq!(out.len(), 1);
}

// --- first seen wins ---

#[test]
fn first_seen_record_survives() {
    let mut a = chart("Amber", 51.5, -0.117, 1815, 12, 10, 7, 30, 0);
    a.source_rating = Some("AA".to_string());
    let mut b = chart("Amber", 51.5, -0.117, 1815, 12, 10, 7, 30, 0);
    b.source_rating = Some("B".to_string());
    let out = merge(&[vec![a], vec![b]]);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].source_rating.as_deref(), Some("AA"));
}

// --- name sensitivity ---

#[test]
fn different_name_is_not_a_duplicate() {
    let a = chart("Amber Celeste", 51.5, -0.117, 1815, 12, 10, 7, 30, 0);
    let b = chart("Amber Cerise", 51.5, -0.117, 1815, 12, 10, 7, 30, 0);
    let out = merge(&[vec![a, b]]);
    assert_eq!(out.len(), 2);
}

#[test]
fn name_match_is_case_sensitive() {
    let a = chart("amber", 51.5, -0.117, 1815, 12, 10, 7, 30, 0);
    let b = chart("Amber", 51.5, -0.117, 1815, 12, 10, 7, 30, 0);
    let out = merge(&[vec![a, b]]);
    assert_eq!(out.len(), 2);
}

// --- date sensitivity ---

#[test]
fn different_year_is_not_a_duplicate() {
    let a = chart("Test", 51.5, -0.117, 1984, 11, 1, 12, 0, 0);
    let b = chart("Test", 51.5, -0.117, 1985, 11, 1, 12, 0, 0);
    let out = merge(&[vec![a, b]]);
    assert_eq!(out.len(), 2);
}

#[test]
fn different_month_is_not_a_duplicate() {
    let a = chart("Test", 51.5, -0.117, 1984, 11, 1, 12, 0, 0);
    let b = chart("Test", 51.5, -0.117, 1984, 12, 1, 12, 0, 0);
    let out = merge(&[vec![a, b]]);
    assert_eq!(out.len(), 2);
}

#[test]
fn different_day_is_not_a_duplicate() {
    let a = chart("Test", 51.5, -0.117, 1984, 11, 1, 12, 0, 0);
    let b = chart("Test", 51.5, -0.117, 1984, 11, 2, 12, 0, 0);
    let out = merge(&[vec![a, b]]);
    assert_eq!(out.len(), 2);
}

// --- time tolerance: ±2 hours (7200 seconds) ---

#[test]
fn same_time_is_duplicate() {
    let a = chart("Test", 51.5, -0.117, 2000, 1, 1, 12, 0, 0);
    let b = chart("Test", 51.5, -0.117, 2000, 1, 1, 12, 0, 0);
    let out = merge(&[vec![a, b]]);
    assert_eq!(out.len(), 1);
}

#[test]
fn time_within_2_hours_is_duplicate() {
    // 12:00 and 13:59:59 → 7199 seconds apart → duplicate
    let a = chart("Test", 51.5, -0.117, 2000, 1, 1, 12, 0, 0);
    let b = chart("Test", 51.5, -0.117, 2000, 1, 1, 13, 59, 59);
    let out = merge(&[vec![a, b]]);
    assert_eq!(out.len(), 1);
}

#[test]
fn time_exactly_2_hours_is_duplicate() {
    // 12:00:00 and 14:00:00 → exactly 7200 seconds → duplicate
    let a = chart("Test", 51.5, -0.117, 2000, 1, 1, 12, 0, 0);
    let b = chart("Test", 51.5, -0.117, 2000, 1, 1, 14, 0, 0);
    let out = merge(&[vec![a, b]]);
    assert_eq!(out.len(), 1);
}

#[test]
fn time_beyond_2_hours_is_not_a_duplicate() {
    // 12:00:00 and 14:00:01 → 7201 seconds → distinct
    let a = chart("Test", 51.5, -0.117, 2000, 1, 1, 12, 0, 0);
    let b = chart("Test", 51.5, -0.117, 2000, 1, 1, 14, 0, 1);
    let out = merge(&[vec![a, b]]);
    assert_eq!(out.len(), 2);
}

// --- coordinate tolerance: 0.1° ---

#[test]
fn coords_within_0_1_deg_is_duplicate() {
    // 0.09° difference on both axes — clearly within 0.1° tolerance
    let a = chart("Test", 51.500, -0.117, 2000, 1, 1, 12, 0, 0);
    let b = chart("Test", 51.590, -0.027, 2000, 1, 1, 12, 0, 0);
    let out = merge(&[vec![a, b]]);
    assert_eq!(out.len(), 1);
}

#[test]
fn coords_beyond_0_1_deg_is_not_a_duplicate() {
    // lat off by 0.101°
    let a = chart("Test", 51.500, -0.117, 2000, 1, 1, 12, 0, 0);
    let b = chart("Test", 51.601, -0.117, 2000, 1, 1, 12, 0, 0);
    let out = merge(&[vec![a, b]]);
    assert_eq!(out.len(), 2);
}

// --- distinct charts are preserved ---

#[test]
fn distinct_charts_all_kept() {
    let a = chart("Alice", 51.5, -0.117, 2000, 1, 1, 12, 0, 0);
    let b = chart("Bob", 40.7, -74.0, 1985, 2, 2, 8, 0, 0);
    let c = chart("Carol", 48.9, 2.35, 1970, 7, 4, 0, 0, 0);
    let out = merge(&[vec![a, b, c]]);
    assert_eq!(out.len(), 3);
}

#[test]
fn merge_preserves_order_first_seen() {
    let a = chart("Alice", 51.5, -0.117, 2000, 1, 1, 12, 0, 0);
    let b = chart("Bob", 40.7, -74.0, 1985, 2, 2, 8, 0, 0);
    // Bob appears in batch 0, Alice in batch 1 (duplicate Alice dropped)
    let out = merge(&[vec![b.clone(), a.clone()], vec![a.clone()]]);
    assert_eq!(out.len(), 2);
    assert_eq!(out[0].name, "Bob");
    assert_eq!(out[1].name, "Alice");
}

// --- merge_reporting: skipped name tracking ---

#[test]
fn reporting_no_dupes_empty_skipped() {
    let a = chart("Alice", 51.5, -0.117, 2000, 1, 1, 12, 0, 0);
    let b = chart("Bob", 40.7, -74.0, 1985, 2, 2, 8, 0, 0);
    let (out, skipped) = merge_reporting(&[vec![a, b]]);
    assert_eq!(out.len(), 2);
    assert!(skipped.is_empty());
}

#[test]
fn reporting_returns_skipped_name() {
    let c = chart("Amber", 51.5, -0.117, 1815, 12, 10, 7, 30, 0);
    let (out, skipped) = merge_reporting(&[vec![c.clone(), c.clone()]]);
    assert_eq!(out.len(), 1);
    assert_eq!(skipped, vec!["Amber"]);
}

#[test]
fn reporting_multiple_skipped_names_in_order() {
    let a = chart("Amber", 51.5, -0.117, 1815, 12, 10, 7, 30, 0);
    let b = chart("Bob", 40.7, -74.0, 1985, 2, 2, 8, 0, 0);
    let (out, skipped) = merge_reporting(&[
        vec![a.clone(), b.clone()],
        vec![a.clone(), b.clone()], // both duplicates
    ]);
    assert_eq!(out.len(), 2);
    assert_eq!(skipped, vec!["Amber", "Bob"]);
}

#[test]
fn reporting_result_matches_merge() {
    let a = chart("Alice", 51.5, -0.117, 2000, 1, 1, 12, 0, 0);
    let b = chart("Bob", 40.7, -74.0, 1985, 2, 2, 8, 0, 0);
    let batches = vec![vec![a.clone(), b.clone()], vec![a.clone()]];
    let merged = merge(&batches);
    let (reported, _) = merge_reporting(&batches);
    assert_eq!(merged.len(), reported.len());
    for (m, r) in merged.iter().zip(reported.iter()) {
        assert_eq!(m.name, r.name);
    }
}

// --- is_candidate: spacetime-only predicate ---
//
// is_candidate intentionally ignores name; it is the cue for "these two
// records need a human decision" and powers read-side flagging as well as
// the future interactive prompt.

#[test]
fn is_candidate_different_name_same_spacetime_returns_true() {
    // The bug we shipped fixed: terse-vs-verbose name pairs with identical
    // datetime + lat/long must be flagged, not invisibly different records.
    let a = chart("Terse", 40.0, -75.0, 1990, 5, 1, 12, 0, 0);
    let b = chart(
        "Terse plus a long descriptor",
        40.0,
        -75.0,
        1990,
        5,
        1,
        12,
        0,
        0,
    );
    assert!(is_candidate(&a, &b));
}

#[test]
fn is_candidate_same_name_different_spacetime_returns_false() {
    let a = chart("Same", 40.0, -75.0, 1990, 5, 1, 12, 0, 0);
    let b = chart("Same", 50.0, 10.0, 1990, 5, 1, 12, 0, 0);
    assert!(!is_candidate(&a, &b));
}

#[test]
fn is_candidate_spacetime_at_tolerance_boundaries_returns_true() {
    // 0.09° on each coord (within 0.1°), exactly 2h apart on the clock.
    let a = chart("A", 51.500, -0.117, 2000, 1, 1, 12, 0, 0);
    let b = chart("B", 51.590, -0.027, 2000, 1, 1, 14, 0, 0);
    assert!(is_candidate(&a, &b));
}

#[test]
fn is_candidate_just_outside_lat_tolerance_returns_false() {
    // lat differs by 0.101°
    let a = chart("A", 51.500, -0.117, 2000, 1, 1, 12, 0, 0);
    let b = chart("B", 51.601, -0.117, 2000, 1, 1, 12, 0, 0);
    assert!(!is_candidate(&a, &b));
}

#[test]
fn is_candidate_just_outside_time_tolerance_returns_false() {
    let a = chart("A", 51.500, -0.117, 2000, 1, 1, 12, 0, 0);
    let b = chart("B", 51.500, -0.117, 2000, 1, 1, 14, 0, 1);
    assert!(!is_candidate(&a, &b));
}

#[test]
fn is_candidate_different_day_returns_false() {
    let a = chart("A", 40.0, -75.0, 1990, 5, 1, 12, 0, 0);
    let b = chart("B", 40.0, -75.0, 1990, 5, 2, 12, 0, 0);
    assert!(!is_candidate(&a, &b));
}

#[test]
fn is_candidate_is_symmetric() {
    let a = chart("A", 40.0, -75.0, 1990, 5, 1, 12, 0, 0);
    let b = chart("B", 40.05, -75.05, 1990, 5, 1, 13, 0, 0);
    assert_eq!(is_candidate(&a, &b), is_candidate(&b, &a));
}

// --- find_candidate: lookup over an already-collected slice ---

#[test]
fn find_candidate_returns_first_matching_index() {
    let a = chart("A", 10.0, 10.0, 1990, 5, 1, 12, 0, 0);
    let b = chart("B", 20.0, 20.0, 1990, 5, 1, 12, 0, 0);
    let c = chart("C", 30.0, 30.0, 1990, 5, 1, 12, 0, 0);
    let candidate = chart("X", 20.0, 20.0, 1990, 5, 1, 12, 0, 0); // matches b
    assert_eq!(find_candidate(&candidate, &[a, b, c]), Some(1));
}

#[test]
fn find_candidate_returns_none_when_no_match() {
    let a = chart("A", 10.0, 10.0, 1990, 5, 1, 12, 0, 0);
    let candidate = chart("X", 60.0, 60.0, 1990, 5, 1, 12, 0, 0);
    assert_eq!(find_candidate(&candidate, &[a]), None);
}

#[test]
fn find_candidate_returns_none_for_empty_slice() {
    let candidate = chart("X", 10.0, 10.0, 1990, 5, 1, 12, 0, 0);
    assert_eq!(find_candidate(&candidate, &[]), None);
}

// --- group_candidates: cluster spacetime matches by transitive closure ---

#[test]
fn group_candidates_empty_input_returns_empty() {
    let groups = group_candidates(&[]);
    assert!(groups.is_empty());
}

#[test]
fn group_candidates_no_matches_one_group_per_chart() {
    // Three distinct people, no spacetime overlap.
    let a = chart("A", 51.5, -0.117, 1815, 12, 10, 7, 30, 0);
    let b = chart("B", 40.7, -74.0, 1985, 2, 2, 8, 0, 0);
    let c = chart("C", 48.9, 2.35, 1970, 7, 4, 0, 0, 0);
    let groups = group_candidates(&[a, b, c]);
    assert_eq!(groups, vec![vec![0], vec![1], vec![2]]);
}

#[test]
fn group_candidates_pair_clusters_together() {
    let a = chart("Terse", 40.0, -75.0, 1990, 5, 1, 12, 0, 0);
    let b = chart("Verbose suffix", 40.0, -75.0, 1990, 5, 1, 12, 0, 0);
    let c = chart("Unrelated", 51.5, -0.117, 1985, 1, 1, 0, 0, 0);
    let groups = group_candidates(&[a, b, c]);
    assert_eq!(groups, vec![vec![0, 1], vec![2]]);
}

#[test]
fn group_candidates_transitive_chain_is_one_group() {
    // A↔B by lat, B↔C by lat — A and C are 0.18° apart (outside tolerance)
    // but transitively share a cluster because both pair with B.
    let a = chart("A", 40.00, -75.0, 1990, 5, 1, 12, 0, 0);
    let b = chart("B", 40.09, -75.0, 1990, 5, 1, 12, 0, 0);
    let c = chart("C", 40.18, -75.0, 1990, 5, 1, 12, 0, 0);
    let groups = group_candidates(&[a, b, c]);
    assert_eq!(groups, vec![vec![0, 1, 2]]);
}

#[test]
fn group_candidates_preserves_input_order_within_a_group() {
    let a = chart("A", 40.0, -75.0, 1990, 5, 1, 12, 0, 0);
    let b = chart("B", 40.0, -75.0, 1990, 5, 1, 12, 0, 0);
    let c = chart("C", 40.0, -75.0, 1990, 5, 1, 12, 0, 0);
    let groups = group_candidates(&[a, b, c]);
    assert_eq!(groups, vec![vec![0, 1, 2]]);
}

// --- multiple batches, complex scenario ---

#[test]
fn three_batches_with_overlaps() {
    let amber = chart("Amber", 51.5, -0.117, 1815, 12, 10, 7, 30, 0);
    let bob = chart("Bob", 40.7, -74.0, 1985, 2, 2, 8, 0, 0);
    let carol = chart("Carol", 48.9, 2.35, 1970, 7, 4, 0, 0, 0);
    // Slightly rectified Amber (within 2h, same coords)
    let amber2 = chart("Amber", 51.5, -0.117, 1815, 12, 10, 8, 15, 0);

    let out = merge(&[
        vec![amber.clone(), bob.clone()],
        vec![amber2, carol.clone()],      // amber2 is a dupe of amber
        vec![bob.clone(), carol.clone()], // both dupes
    ]);
    assert_eq!(out.len(), 3);
    assert_eq!(out[0].name, "Amber");
    assert_eq!(out[1].name, "Bob");
    assert_eq!(out[2].name, "Carol");
}
