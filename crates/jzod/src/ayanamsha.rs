//! Canonical ayanamsha name authority for JZOD.
//!
//! This module owns the master table of sidereal ayanamsha slugs, their
//! accepted aliases, and their canonical default [`SiderealFrame`] where a
//! primary authority establishes one.
//!
//! The slug set mirrors every named sidereal zodiac that
//! `astrogram::chart::Zodiac` can express (Solar Fire's zodiac list).
//! Canonical slugs are lowercase `snake_case`. Aliases resolve to their
//! canonical entry via [`resolve`].

use crate::chart::SiderealFrame;

/// Canonical information for a single ayanamsha.
pub struct AyanamshaInfo {
    /// Canonical slug (lowercase `snake_case`).
    pub slug: &'static str,
    /// Accepted aliases that resolve to this entry.
    pub aliases: &'static [&'static str],
    /// Canonical default frame, where a primary authority establishes one.
    ///
    /// `None` means no researched authority on file; frame must be stated or
    /// left unrecorded. A consumer that needs a concrete frame should consult
    /// this first, then refuse if `None`.
    pub default_frame: Option<SiderealFrame>,
}

/// Master table of JZOD canonical ayanamsha entries.
///
/// Every entry in [`crate::chart::Zodiac::Sidereal`]'s expected ayanamsha
/// slug space appears here exactly once (by canonical slug). Aliases resolve
/// to their canonical entry via [`resolve`].
///
/// Default frames:
/// - `lahiri` — [`SiderealFrame::True`] (Indian Astronomical Ephemeris
///   publishes true Chitrapaksha; `JHora` defaults to true).
/// - `fagan_bradley` — [`SiderealFrame::Mean`] (Bradley's published anchor
///   uses precession only).
/// - `raman` — [`SiderealFrame::Mean`] (B.V. Raman's published table values
///   are means; formula defines a flat rate with no nutation term).
/// - All others — `None` (no primary authority on file).
pub const AYANAMSHAS: &[AyanamshaInfo] = &[
    AyanamshaInfo {
        slug: "fagan_bradley",
        aliases: &["fagan_allen"],
        default_frame: Some(SiderealFrame::Mean),
    },
    AyanamshaInfo {
        slug: "lahiri",
        aliases: &[],
        default_frame: Some(SiderealFrame::True),
    },
    AyanamshaInfo {
        slug: "de_luce",
        aliases: &[],
        default_frame: None,
    },
    AyanamshaInfo {
        slug: "raman",
        aliases: &[],
        default_frame: Some(SiderealFrame::Mean),
    },
    AyanamshaInfo {
        slug: "usha_shashi",
        aliases: &[],
        default_frame: None,
    },
    AyanamshaInfo {
        slug: "krishnamurti",
        aliases: &[],
        default_frame: None,
    },
    AyanamshaInfo {
        slug: "djwhal_khul",
        aliases: &[],
        default_frame: None,
    },
    AyanamshaInfo {
        slug: "svp",
        aliases: &[],
        default_frame: None,
    },
    AyanamshaInfo {
        slug: "sri_yukteswar",
        aliases: &[],
        default_frame: None,
    },
    AyanamshaInfo {
        slug: "jn_bhasin",
        aliases: &[],
        default_frame: None,
    },
    AyanamshaInfo {
        slug: "larry_ely",
        aliases: &[],
        default_frame: None,
    },
    AyanamshaInfo {
        slug: "takra_i",
        aliases: &[],
        default_frame: None,
    },
    AyanamshaInfo {
        slug: "takra_ii",
        aliases: &[],
        default_frame: None,
    },
    AyanamshaInfo {
        slug: "sundara_rajan",
        aliases: &[],
        default_frame: None,
    },
    AyanamshaInfo {
        slug: "shill_pond",
        aliases: &[],
        default_frame: None,
    },
];

/// Resolve a slug or alias (exact, lowercase) to its canonical entry.
///
/// Returns `None` if neither the canonical slug nor any alias matches.
#[must_use]
pub fn resolve(slug: &str) -> Option<&'static AyanamshaInfo> {
    AYANAMSHAS
        .iter()
        .find(|a| a.slug == slug || a.aliases.contains(&slug))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chart::SiderealFrame;

    #[test]
    fn canonical_slug_resolves() {
        let info = resolve("lahiri").expect("lahiri must resolve");
        assert_eq!(info.slug, "lahiri");
    }

    #[test]
    fn fagan_allen_alias_resolves_to_fagan_bradley() {
        let info = resolve("fagan_allen").expect("fagan_allen must resolve as alias");
        assert_eq!(info.slug, "fagan_bradley");
    }

    #[test]
    fn unknown_slug_returns_none() {
        assert!(resolve("nonsense_ayanamsha").is_none());
    }

    #[test]
    fn no_duplicate_slugs_or_aliases() {
        let mut seen = std::collections::HashSet::new();
        for entry in AYANAMSHAS {
            assert!(seen.insert(entry.slug), "duplicate slug: {}", entry.slug);
            for alias in entry.aliases {
                assert!(seen.insert(*alias), "duplicate alias: {alias}");
            }
        }
    }

    #[test]
    fn lahiri_default_frame_is_true() {
        let info = resolve("lahiri").unwrap();
        assert_eq!(info.default_frame, Some(SiderealFrame::True));
    }

    #[test]
    fn fagan_bradley_default_frame_is_mean() {
        let info = resolve("fagan_bradley").unwrap();
        assert_eq!(info.default_frame, Some(SiderealFrame::Mean));
    }

    #[test]
    fn all_other_entries_have_no_default_frame() {
        for entry in AYANAMSHAS {
            if matches!(entry.slug, "lahiri" | "fagan_bradley" | "raman") {
                continue;
            }
            assert!(
                entry.default_frame.is_none(),
                "unexpected default_frame on '{}': {:?}",
                entry.slug,
                entry.default_frame
            );
        }
    }

    #[test]
    fn raman_default_frame_is_mean() {
        let info = resolve("raman").unwrap();
        assert_eq!(info.default_frame, Some(SiderealFrame::Mean));
    }

    #[test]
    fn slug_table_matches_ayanamshas() {
        let doc = include_str!("../JZOD.md");
        let lines: Vec<&str> = doc.lines().collect();

        // Locate the section heading.
        let section_start = lines
            .iter()
            .position(|l| l.contains("### Canonical Ayanamsha Slugs"))
            .expect("'### Canonical Ayanamsha Slugs' section not found in JZOD.md");

        // Skip forward to first `|` line (that is the header row).
        let after_heading = &lines[section_start + 1..];
        let header_offset = after_heading
            .iter()
            .position(|l| l.starts_with('|'))
            .expect("no table header row found after section heading");

        // Skip the header row and the separator row, then collect consecutive data rows.
        // Each row is split on `|` with whitespace trimmed per cell.
        let data_rows: Vec<Vec<&str>> = after_heading
            .iter()
            .skip(header_offset + 2)
            .take_while(|l| l.starts_with('|'))
            .map(|l| l.split('|').map(str::trim).collect())
            .collect();

        // Assertion 4: row count must equal AYANAMSHAS length (no orphan doc rows).
        assert_eq!(
            data_rows.len(),
            AYANAMSHAS.len(),
            "doc table has {} rows but AYANAMSHAS has {} entries — one is stale",
            data_rows.len(),
            AYANAMSHAS.len()
        );

        // Assertions 1–3: every AYANAMSHAS entry must appear in exactly one row
        // with matching frame and aliases.
        for entry in AYANAMSHAS {
            // Assertion 1: slug present in table.
            let row = data_rows
                .iter()
                .find(|cols| {
                    cols.get(1)
                        .is_some_and(|c| c.trim_matches('`') == entry.slug)
                })
                .unwrap_or_else(|| {
                    panic!(
                        "slug '{}' not found in JZOD.md Canonical Ayanamsha Slugs table",
                        entry.slug
                    )
                });

            // Assertion 2: default_frame cell matches.
            // Cells may be backtick-wrapped (`mean`, `true`) or bare (—).
            let frame_cell = row.get(3).copied().unwrap_or("").trim_matches('`');
            let expected_frame = match entry.default_frame {
                Some(SiderealFrame::Mean) => "mean",
                Some(SiderealFrame::True) => "true",
                None => "\u{2014}", // em dash —
            };
            assert_eq!(
                frame_cell, expected_frame,
                "slug '{}': doc frame cell is '{}' but expected '{}'",
                entry.slug, frame_cell, expected_frame
            );

            // Assertion 3: every alias from AYANAMSHAS appears in the alias cell.
            let alias_cell = row.get(2).copied().unwrap_or("").trim();
            let doc_aliases: Vec<&str> = if alias_cell == "\u{2014}" {
                // em dash — means no aliases
                vec![]
            } else {
                alias_cell
                    .split(',')
                    .map(|a| a.trim().trim_matches('`'))
                    .collect()
            };
            for alias in entry.aliases {
                assert!(
                    doc_aliases.contains(alias),
                    "slug '{}': alias '{}' missing from doc table alias cell",
                    entry.slug,
                    alias
                );
            }
        }
    }
}
