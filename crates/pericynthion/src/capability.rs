//! On-disk capability assessment: given where the user's JPL data and Horizons
//! SPKs live, report which catalog bodies are computable right now. Grouped by
//! [`crate::placements::DataSource`]; the single readout the CLI renders after a
//! fetch and the basis for the static "what gets you what" table.

use crate::placements::{self, CATALOG, Category, DataSource, Placement};
use std::fmt::Write as _;
use std::path::Path;

/// One catalog body and whether its backing data is present on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BodyStatus {
    /// Catalog display name.
    pub name: &'static str,
    /// Where its data comes from.
    pub source: DataSource,
    /// Whether the backing file(s) are present right now.
    pub present: bool,
}

/// Per-body presence across the whole catalog.
#[derive(Debug, Clone, Default)]
pub struct CapabilityReport {
    /// One entry per catalog body, in catalog order.
    pub bodies: Vec<BodyStatus>,
}

impl CapabilityReport {
    /// Bodies whose data is present.
    pub fn present(&self) -> impl Iterator<Item = &BodyStatus> {
        self.bodies.iter().filter(|b| b.present)
    }
    /// Bodies whose data is not present.
    pub fn absent(&self) -> impl Iterator<Item = &BodyStatus> {
        self.bodies.iter().filter(|b| !b.present)
    }
}

/// Is the DE441 planetary binary reachable from `jpl_start`? Detected by finding
/// a `header.<digits>` anywhere under the (hoisted) pointed-at root.
fn de441_present(jpl_start: Option<&Path>) -> bool {
    jpl_start.is_some_and(|s| {
        crate::locate_jpl_file_matching(s, |name| {
            name.strip_prefix("header.")
                .is_some_and(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit()))
        })
        .is_some()
    })
}

/// Assess what is computable given where JPL data and Horizons SPKs live.
#[must_use]
pub fn assess(jpl_start: Option<&Path>, horizons_dir: Option<&Path>) -> CapabilityReport {
    let de441 = de441_present(jpl_start);
    let n16 = jpl_start.and_then(crate::spk::locate_default_bsp).is_some();
    let n373 = jpl_start.and_then(crate::spk::locate_n373_bsp).is_some();

    let horizons_has = |p: &Placement| -> bool {
        match (horizons_dir, p.horizons_naif_id()) {
            (Some(dir), Some(naif)) => dir.join(format!("{naif}.bsp")).is_file(),
            _ => false,
        }
    };

    let bodies = placements::CATALOG
        .iter()
        .map(|p| {
            let source = p.data_source();
            let present = match source {
                DataSource::De441 | DataSource::Computed => de441,
                DataSource::Sb441N16 => n16,
                DataSource::Sb441N373 => n373,
                DataSource::Horizons => horizons_has(p),
            };
            BodyStatus {
                name: p.name,
                source,
                present,
            }
        })
        .collect();
    CapabilityReport { bodies }
}

const OK: &str = "[have]";
const NO: &str = "[need]";

/// The four file-backed sources in display order (Computed is folded into DE441
/// since it needs no separate file).
const SOURCE_ORDER: &[DataSource] = &[
    DataSource::De441,
    DataSource::Sb441N16,
    DataSource::Sb441N373,
    DataSource::Horizons,
];

/// Human label + the command/file that supplies a source.
#[must_use]
pub fn source_label(s: DataSource) -> &'static str {
    match s {
        DataSource::De441 => "Planets & luminaries (DE441 binary)",
        DataSource::Sb441N16 => "Main-belt (sb441-n16.bsp)",
        DataSource::Sb441N373 => "Dwarf planets / TNOs (sb441-n373.bsp)",
        DataSource::Horizons => "Centaurs + Albion (Horizons on-demand)",
        DataSource::Computed => "Computed points (no extra file)",
    }
}

/// Bodies for a source, in catalog order. For DE441 also fold in the Computed
/// points (they come free with the planetary binary).
fn bodies_for(s: DataSource) -> Vec<&'static str> {
    CATALOG
        .iter()
        .filter(|p| {
            let ds = p.data_source();
            ds == s || (s == DataSource::De441 && ds == DataSource::Computed)
        })
        .map(|p| p.name)
        .collect()
}

/// How to obtain a non-Horizons source (used in the static table). Horizons is
/// per-category and handled by [`horizons_commands`], not this function.
fn how_to_get(s: DataSource) -> &'static str {
    match s {
        DataSource::De441 | DataSource::Sb441N16 | DataSource::Sb441N373 => {
            "starcat data fetch de441"
        }
        DataSource::Horizons => "starcat horizons",
        DataSource::Computed => "(computed)",
    }
}

/// The `starcat horizons <noun>` subcommand for a body's category. `None` for
/// categories that carry no Horizons-fetchable bodies.
fn horizons_noun(cat: Category) -> Option<&'static str> {
    match cat {
        Category::DwarfPlanet => Some("dp"),
        Category::Asteroid => Some("ast"),
        Category::Centaur => Some("cent"),
        Category::Kbo => Some("kbo"),
        Category::Tno => Some("tno"),
        _ => None,
    }
}

/// The distinct Horizons fetch commands, in catalog order: one `(noun, bodies)`
/// pair per category among the Horizons-source bodies. This is what keeps a
/// KBO like Albion (`kbo`) from being folded under the centaurs' `cent` hint.
fn horizons_commands() -> Vec<(&'static str, Vec<&'static str>)> {
    let mut groups: Vec<(&'static str, Vec<&'static str>)> = Vec::new();
    for p in CATALOG {
        if p.data_source() != DataSource::Horizons {
            continue;
        }
        let Some(noun) = horizons_noun(p.category) else {
            continue;
        };
        if let Some(entry) = groups.iter_mut().find(|(n, _)| *n == noun) {
            entry.1.push(p.name);
        } else {
            groups.push((noun, vec![p.name]));
        }
    }
    groups
}

/// Presence-agnostic reference: what data unlocks which bodies.
#[must_use]
pub fn what_gets_you_what() -> String {
    let mut out = String::from("What each dataset gets you:\n");
    for &s in SOURCE_ORDER {
        if s == DataSource::Horizons {
            let _ = writeln!(out, "  {}", source_label(s));
            for (noun, bodies) in horizons_commands() {
                let _ = write!(
                    out,
                    "    via `starcat horizons {noun}`\n    {}\n",
                    bodies.join(", ")
                );
            }
        } else {
            let _ = write!(
                out,
                "  {}\n    via `{}`\n    {}\n",
                source_label(s),
                how_to_get(s),
                bodies_for(s).join(", ")
            );
        }
    }
    out
}

/// Post-fetch readout: mark each source group have/need against `report`, and
/// hint the per-category horizons fetch commands when a group is absent.
#[must_use]
pub fn render_capabilities(report: &CapabilityReport) -> String {
    let mut out = String::from("Capabilities given files on disk:\n");
    for &s in SOURCE_ORDER {
        let group: Vec<&BodyStatus> = report.bodies.iter().filter(|b| b.source == s).collect();
        // A group counts as "have" only if all its members are present.
        let have = !group.is_empty() && group.iter().all(|b| b.present);
        let marker = if have { OK } else { NO };
        let _ = write!(
            out,
            "  {marker} {}\n    {}\n",
            source_label(s),
            bodies_for(s).join(", ")
        );
        if !have && s == DataSource::Horizons {
            for (noun, bodies) in horizons_commands() {
                let _ = writeln!(
                    out,
                    "    -> run `starcat horizons {noun}` for: {}",
                    bodies.join(", ")
                );
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::placements::DataSource;
    use std::fs;

    // A flat drop-folder with only the two sb441 bundles present.
    #[test]
    fn assess_reports_present_bundles_and_absent_horizons() {
        let tmp = tempdir::TempDir::new("cap").unwrap();
        let root = tmp.path();
        // DE441 binary + header present (minimal, content irrelevant to presence).
        fs::write(root.join("header.441"), b"h").unwrap();
        fs::write(root.join("linux_p1550p2650.441"), b"b").unwrap();
        fs::write(root.join("sb441-n16.bsp"), b"x").unwrap();
        fs::write(root.join("sb441-n373.bsp"), b"y").unwrap();
        // No horizons dir supplied.
        let report = assess(Some(root), None);

        let by = |name: &str| report.bodies.iter().find(|b| b.name == name).unwrap();
        assert!(by("Sun").present, "DE441 body present");
        assert_eq!(by("Sun").source, DataSource::De441);
        assert!(by("Ceres").present, "sb441-n16 present");
        assert!(by("Eris").present, "sb441-n373 present");
        assert!(by("Ascendant").present, "computed follows DE441");
        assert!(!by("Chiron").present, "no horizons dir -> centaur absent");
        assert_eq!(by("Chiron").source, DataSource::Horizons);
    }

    #[test]
    fn assess_reports_present_horizons_body_when_its_bsp_exists() {
        let tmp = tempdir::TempDir::new("caphz").unwrap();
        let hz = tmp.path();
        // Chiron's Horizons NAIF id is 20_000_000 + 2060.
        let chiron = crate::placements::find_by_slug("Chiron").unwrap();
        let naif = chiron.horizons_naif_id().unwrap();
        fs::write(hz.join(format!("{naif}.bsp")), b"z").unwrap();
        let report = assess(None, Some(hz));
        let by = |name: &str| report.bodies.iter().find(|b| b.name == name).unwrap();
        assert!(by("Chiron").present, "chiron bsp on disk -> present");
        assert!(!by("Pholus").present, "pholus bsp absent -> absent");
        assert!(!by("Sun").present, "no jpl dir -> DE441 absent");
    }
}

#[cfg(test)]
mod render_tests {
    use super::*;

    #[test]
    fn what_gets_you_what_lists_each_source_and_a_body() {
        let t = what_gets_you_what();
        assert!(t.contains("sb441-n16"));
        assert!(t.contains("Ceres"));
        assert!(t.contains("sb441-n373"));
        assert!(t.contains("Eris"));
        assert!(t.contains("DE441"));
        assert!(t.contains("Sun"));
        assert!(t.contains("starcat horizons cent"));
        assert!(t.contains("Chiron"));
        // Albion is a KBO, not a centaur -- it needs its own `kbo` command.
        assert!(t.contains("starcat horizons kbo"));
        assert!(t.contains("Albion"));
    }

    #[test]
    fn render_capabilities_marks_have_and_need_and_hints_horizons() {
        // Empty disk: nothing present.
        let report = assess(None, None);
        let out = render_capabilities(&report);
        assert!(out.contains("[need]"), "absent groups marked need");
        assert!(
            out.contains("starcat horizons cent"),
            "absent centaurs get the cent hint"
        );
        assert!(
            out.contains("starcat horizons kbo"),
            "absent Albion (a KBO) gets its own kbo hint"
        );
    }
}
