//! Golden test: the committed `docs/placements.md` must match what
//! `pericynthion::placements::markdown()` generates. If this fails, the doc is
//! stale — run `just placements` to regenerate it.

#[test]
fn placements_doc_is_current() {
    // Precondition: catalog.gz must be fetched (`just fetch bsc5`) and pericynthion
    // rebuilt before this test passes; stars::markdown_stats() returns a stub otherwise.
    let want = format!(
        "{}{}",
        pericynthion::placements::markdown(),
        pericynthion::stars::markdown_stats()
    );
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/placements.md");
    let have =
        std::fs::read_to_string(path).expect("docs/placements.md missing — run `just placements`");
    assert_eq!(
        have, want,
        "docs/placements.md is stale — run `just placements` and commit the result"
    );
}
