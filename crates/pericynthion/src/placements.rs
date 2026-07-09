//! The placements catalog: the single source of truth for every chart point
//! and body starcat knows about — supported or not.
//!
//! [`CATALOG`] is an ordered, hand-curated table. Everything else in the
//! product reads from it: `starcat compute --omniscient ls` lists the
//! supported entries, and `docs/placements.md` is generated from it via
//! [`markdown`]. Editing the catalog here updates all consumers at once.

/// Broad classification for a [`Placement`]. Used to group the generated doc.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    /// The two lights: Sun and Moon.
    Luminary,
    /// Major planets (Mercury … Neptune; Earth in heliocentric charts).
    Planet,
    /// Dwarf planets (Pluto, Ceres, Eris, …).
    DwarfPlanet,
    /// Main-belt asteroids carried in the small-body ephemerides.
    Asteroid,
    /// Centaurs — minor bodies between Jupiter and Neptune (Chiron, …).
    Centaur,
    /// Kuiper-belt objects.
    Kbo,
    /// Trans-Neptunian objects beyond the classical belt.
    Tno,
    /// Computed (not observed) points: angles, nodes, apogees, lots.
    MathematicalPoint,
}

impl Category {
    /// Render order for the generated doc — also the only categories shown.
    pub const ORDER: &'static [Category] = &[
        Category::Luminary,
        Category::Planet,
        Category::DwarfPlanet,
        Category::Asteroid,
        Category::Centaur,
        Category::Kbo,
        Category::Tno,
        Category::MathematicalPoint,
    ];

    /// Human-readable section heading for this category.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Category::Luminary => "Luminaries",
            Category::Planet => "Planets",
            Category::DwarfPlanet => "Dwarf planets",
            Category::Asteroid => "Asteroids",
            Category::Centaur => "Centaurs",
            Category::Kbo => "Kuiper-belt objects",
            Category::Tno => "Trans-Neptunian objects",
            Category::MathematicalPoint => "Mathematical points",
        }
    }
}

/// One catalog entry: a named point or body and whether starcat computes it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    /// Display name (e.g. `"Sun"`, `"Ascendant"`, `"Lot of Fortune"`).
    pub name: &'static str,
    /// Broad classification.
    pub category: Category,
    /// Whether starcat can compute this placement. For bodies whose `note`
    /// references a data source (`sb441-n373.bsp`, `Horizons SPK`), `true`
    /// presumes that file has been fetched or the network fetch has been
    /// performed; see the `note` field for specifics.
    pub supported: bool,
    /// Minor Planet Center number, for minor bodies that have one (asteroids,
    /// centaurs, KBOs, TNOs, and the minor-planet dwarf planets). `None` for
    /// the Sun, Moon, major planets, the DE441 body Pluto, and mathematical
    /// points.
    ///
    /// Two distinct NAIF id schemes derive from this number, and they differ:
    /// JPL's bundled `sb441-n16/n373.bsp` use `2_000_000 + mpc`, while
    /// SPK files generated on demand by the Horizons API use
    /// `20_000_000 + mpc` (see [`Placement::horizons_naif_id`]). Storing the
    /// MPC number rather than a single NAIF id keeps both derivable.
    pub mpc_number: Option<u32>,
    /// Short note: how it is computed (supported) or why not yet (otherwise).
    pub note: &'static str,
}

impl Placement {
    /// The Horizons `COMMAND` designator for this body's small-body record:
    /// the MPC number with a trailing `;` (which forces a small-body lookup by
    /// number). `None` for bodies with no MPC number.
    #[must_use]
    pub fn horizons_command(&self) -> Option<String> {
        self.mpc_number.map(|n| format!("{n};"))
    }

    /// The NAIF integer id that a Horizons-generated SPK stamps on this body's
    /// segment: `20_000_000 + mpc`. This is the id our SPK reader must query in
    /// a Horizons `.bsp`, and it differs from the `2_000_000 + mpc` id used by
    /// the bundled `sb441` files. `None` for bodies with no MPC number.
    #[must_use]
    #[allow(clippy::cast_possible_wrap)] // MPC numbers are far below i32::MAX
    pub fn horizons_naif_id(&self) -> Option<i32> {
        self.mpc_number.map(|n| 20_000_000 + n as i32)
    }

    /// The NAIF integer id for this body in JPL's bundled `sb441` files:
    /// `2_000_000 + mpc`. `None` for bodies with no MPC number. Contrast
    /// [`Placement::horizons_naif_id`] (the `20_000_000 + mpc` Horizons scheme).
    #[must_use]
    #[allow(clippy::cast_possible_wrap)] // MPC numbers are far below i32::MAX
    pub fn sb441_naif_id(&self) -> Option<i32> {
        self.mpc_number.map(|n| 2_000_000 + n as i32)
    }

    /// Classify this placement's data source from its `note`. The catalog notes
    /// are the canonical strings, so this is a total, deterministic mapping.
    #[must_use]
    pub fn data_source(&self) -> DataSource {
        let n = self.note;
        if n.contains("sb441-n16") {
            DataSource::Sb441N16
        } else if n.contains("sb441-n373") {
            DataSource::Sb441N373
        } else if n.contains("Horizons") {
            DataSource::Horizons
        } else if n.starts_with("DE441") {
            DataSource::De441
        } else {
            DataSource::Computed
        }
    }
}

/// Where a placement's ephemeris data comes from -- the single axis the
/// "what gets you what" table and the post-fetch capability readout group on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataSource {
    /// The DE441 planetary binary (Sun, Moon, Mercury..Neptune, Pluto).
    De441,
    /// JPL's bundled `sb441-n16.bsp` main-belt ephemeris.
    Sb441N16,
    /// JPL's bundled `sb441-n373.bsp` extended small-body ephemeris.
    Sb441N373,
    /// A Horizons-generated on-demand SPK (`starcat horizons`).
    Horizons,
    /// Computed from other bodies plus chart geometry; needs no extra file.
    Computed,
}

const fn p(
    name: &'static str,
    category: Category,
    supported: bool,
    note: &'static str,
) -> Placement {
    Placement {
        name,
        category,
        supported,
        mpc_number: None,
        note,
    }
}

/// Like [`p`], for a minor body that carries a Minor Planet Center number.
const fn pm(
    name: &'static str,
    category: Category,
    supported: bool,
    mpc: u32,
    note: &'static str,
) -> Placement {
    Placement {
        name,
        category,
        supported,
        mpc_number: Some(mpc),
        note,
    }
}

/// The canonical, ordered catalog of placements.
///
/// Order within a category is intentional and feeds the generated doc and the
/// `--omniscient ls` listing verbatim. Keep it stable.
pub const CATALOG: &[Placement] = &[
    // — Luminaries —
    p(
        "Sun",
        Category::Luminary,
        true,
        "DE441 (Earth replaces it in heliocentric)",
    ),
    p("Moon", Category::Luminary, true, "DE441"),
    // — Planets —
    p("Mercury", Category::Planet, true, "DE441"),
    p("Venus", Category::Planet, true, "DE441"),
    p("Mars", Category::Planet, true, "DE441"),
    p("Jupiter", Category::Planet, true, "DE441"),
    p("Saturn", Category::Planet, true, "DE441"),
    p("Uranus", Category::Planet, true, "DE441"),
    p("Neptune", Category::Planet, true, "DE441"),
    // — Dwarf planets —
    p("Pluto", Category::DwarfPlanet, true, "DE441"),
    pm(
        "Ceres",
        Category::DwarfPlanet,
        true,
        1,
        "small-body SPK (sb441-n16.bsp)",
    ),
    pm(
        "Eris",
        Category::DwarfPlanet,
        true,
        136_199,
        "small-body SPK (sb441-n373.bsp)",
    ),
    pm(
        "Haumea",
        Category::DwarfPlanet,
        true,
        136_108,
        "small-body SPK (sb441-n373.bsp)",
    ),
    pm(
        "Makemake",
        Category::DwarfPlanet,
        true,
        136_472,
        "small-body SPK (sb441-n373.bsp)",
    ),
    // — Asteroids —
    pm(
        "Pallas",
        Category::Asteroid,
        true,
        2,
        "small-body SPK (sb441-n16.bsp)",
    ),
    pm(
        "Juno",
        Category::Asteroid,
        true,
        3,
        "small-body SPK (sb441-n16.bsp)",
    ),
    pm(
        "Vesta",
        Category::Asteroid,
        true,
        4,
        "small-body SPK (sb441-n16.bsp)",
    ),
    pm(
        "Hygiea",
        Category::Asteroid,
        true,
        10,
        "small-body SPK (sb441-n16.bsp)",
    ),
    // — Centaurs —
    pm(
        "Chiron",
        Category::Centaur,
        true,
        2060,
        "Horizons SPK; fetch with `starcat horizons`",
    ),
    pm(
        "Pholus",
        Category::Centaur,
        true,
        5145,
        "Horizons SPK; fetch with `starcat horizons`",
    ),
    pm(
        "Nessus",
        Category::Centaur,
        true,
        7066,
        "Horizons SPK; fetch with `starcat horizons`",
    ),
    pm(
        "Chariklo",
        Category::Centaur,
        true,
        10_199,
        "Horizons SPK; fetch with `starcat horizons`",
    ),
    pm(
        "Asbolus",
        Category::Centaur,
        true,
        8_405,
        "Horizons SPK; fetch with `starcat horizons`",
    ),
    // — Kuiper-belt objects —
    pm(
        "Quaoar",
        Category::Kbo,
        true,
        50_000,
        "small-body SPK (sb441-n373.bsp)",
    ),
    pm(
        "Orcus",
        Category::Kbo,
        true,
        90_482,
        "small-body SPK (sb441-n373.bsp)",
    ),
    pm(
        "Ixion",
        Category::Kbo,
        true,
        28_978,
        "small-body SPK (sb441-n373.bsp)",
    ),
    pm(
        "Varuna",
        Category::Kbo,
        true,
        20_000,
        "small-body SPK (sb441-n373.bsp)",
    ),
    pm(
        "Albion",
        Category::Kbo,
        true,
        15_760,
        "Horizons SPK; fetch with `starcat horizons`",
    ),
    // — Trans-Neptunian objects —
    pm(
        "Sedna",
        Category::Tno,
        true,
        90_377,
        "small-body SPK (sb441-n373.bsp)",
    ),
    pm(
        "Gonggong",
        Category::Tno,
        true,
        225_088,
        "small-body SPK (sb441-n373.bsp)",
    ),
    // — Mathematical points (angles) —
    p(
        "Ascendant",
        Category::MathematicalPoint,
        true,
        "Ac; needs lat + lon",
    ),
    p(
        "Descendant",
        Category::MathematicalPoint,
        true,
        "Ds; needs lat + lon",
    ),
    p(
        "Medium Coeli",
        Category::MathematicalPoint,
        true,
        "Mc; needs lon",
    ),
    p(
        "Imum Coeli",
        Category::MathematicalPoint,
        true,
        "Ic; needs lon",
    ),
    p(
        "Vertex",
        Category::MathematicalPoint,
        true,
        "Vx; needs lat + lon",
    ),
    p(
        "Anti-Vertex",
        Category::MathematicalPoint,
        true,
        "Ax; needs lat + lon",
    ),
    // — Mathematical points (nodes / apogees) —
    p(
        "North Node",
        Category::MathematicalPoint,
        true,
        "Nn; mean or true",
    ),
    p(
        "South Node",
        Category::MathematicalPoint,
        true,
        "Sn; mean or true",
    ),
    p(
        "Black Moon Lilith",
        Category::MathematicalPoint,
        true,
        "Lil; mean or true",
    ),
    p(
        "Priapus",
        Category::MathematicalPoint,
        true,
        "Pri; mean or true",
    ),
    // — Mathematical points (Hermetic lots) —
    p(
        "Lot of Fortune",
        Category::MathematicalPoint,
        true,
        "needs Ac + Sun + Moon",
    ),
    p(
        "Lot of Spirit",
        Category::MathematicalPoint,
        true,
        "needs Ac + Sun + Moon",
    ),
    p(
        "Lot of Exaltation",
        Category::MathematicalPoint,
        true,
        "needs Ac + Sun + Moon",
    ),
    p(
        "Lot of Necessity",
        Category::MathematicalPoint,
        true,
        "+ Mercury",
    ),
    p("Lot of Eros", Category::MathematicalPoint, true, "+ Venus"),
    p(
        "Lot of Courage",
        Category::MathematicalPoint,
        true,
        "+ Mars",
    ),
    p(
        "Lot of Victory",
        Category::MathematicalPoint,
        true,
        "+ Jupiter",
    ),
    p(
        "Lot of Nemesis",
        Category::MathematicalPoint,
        true,
        "+ Saturn",
    ),
];

/// Iterator over every catalog entry starcat can currently compute.
pub fn supported() -> impl Iterator<Item = &'static Placement> {
    CATALOG.iter().filter(|p| p.supported)
}

/// The catalog entry whose display name matches `slug` case-insensitively.
#[must_use]
pub fn find_by_slug(slug: &str) -> Option<&'static Placement> {
    CATALOG.iter().find(|p| p.name.eq_ignore_ascii_case(slug))
}

/// Why a body slug could not be resolved to an available NAIF id.
///
/// `Display` text is tool-neutral so any frontend can show it; the `starcat`
/// CLI re-formats `NotMinorBody`/`NotCovered` to its own wording.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BodyResolveError {
    /// The slug is not in [`CATALOG`].
    #[error("unknown body {0:?}")]
    Unknown(String),
    /// The body has no MPC number, so it has no SPK minor-body id.
    #[error("{0} is not an SPK minor body")]
    NotMinorBody(&'static str),
    /// Neither the sb441 nor the Horizons id is covered by the open SPKs.
    #[error("{0} is not available locally")]
    NotCovered(&'static str),
}

/// Resolve a body slug to the NAIF id actually covered, preferring the sb441
/// id (`2_000_000 + mpc`) over the Horizons id (`20_000_000 + mpc`).
/// `covered(id)` reports whether any open SPK covers `id`.
///
/// # Errors
/// [`BodyResolveError::Unknown`] if the slug is not in the catalog,
/// [`BodyResolveError::NotMinorBody`] if it has no MPC number, or
/// [`BodyResolveError::NotCovered`] if no open SPK covers either id.
pub fn resolve_body_id(slug: &str, covered: impl Fn(i32) -> bool) -> Result<i32, BodyResolveError> {
    let placement =
        find_by_slug(slug).ok_or_else(|| BodyResolveError::Unknown(slug.to_string()))?;
    let (Some(sb441), Some(horizons)) = (placement.sb441_naif_id(), placement.horizons_naif_id())
    else {
        return Err(BodyResolveError::NotMinorBody(placement.name));
    };
    if covered(sb441) {
        Ok(sb441)
    } else if covered(horizons) {
        Ok(horizons)
    } else {
        Err(BodyResolveError::NotCovered(placement.name))
    }
}

/// Every catalog minor body whose sb441 or Horizons id is covered by the open
/// SPKs (sb441 preferred). The set `--omniscient` computes.
#[must_use]
pub fn omniscient_body_ids(covered: impl Fn(i32) -> bool) -> Vec<i32> {
    let mut ids = Vec::new();
    for p in CATALOG {
        let (Some(sb441), Some(horizons)) = (p.sb441_naif_id(), p.horizons_naif_id()) else {
            continue;
        };
        if covered(sb441) {
            ids.push(sb441);
        } else if covered(horizons) {
            ids.push(horizons);
        }
    }
    ids
}

/// Display name for an SPK NAIF id, resolving both the sb441 (`2_000_000 + mpc`)
/// and Horizons (`20_000_000 + mpc`) id schemes back to a catalog name.
#[must_use]
pub fn name_for_naif(naif_id: i32) -> Option<&'static str> {
    let mpc = mpc_from_naif(naif_id)?;
    CATALOG
        .iter()
        .find(|p| p.mpc_number == Some(mpc))
        .map(|p| p.name)
}

/// Extract the MPC number from an SPK NAIF id under either id scheme.
fn mpc_from_naif(naif_id: i32) -> Option<u32> {
    let raw = if (20_000_001..=20_999_999).contains(&naif_id) {
        naif_id - 20_000_000
    } else if (2_000_001..=2_999_999).contains(&naif_id) {
        naif_id - 2_000_000
    } else {
        return None;
    };
    u32::try_from(raw).ok()
}

/// Newline-separated names of every supported placement, in catalog order,
/// with a single trailing newline. Backs `starcat compute --omniscient ls`.
#[must_use]
pub fn supported_list() -> String {
    let mut out = String::new();
    for entry in supported() {
        out.push_str(entry.name);
        out.push('\n');
    }
    out
}

/// Render the catalog as deterministic Markdown for `docs/placements.md`.
///
/// Output is byte-stable across runs: fixed section order ([`Category::ORDER`]),
/// catalog order within each section, LF line endings, a single trailing
/// newline, and no timestamps or other volatile content. Callers may write the
/// result unconditionally — identical input yields identical bytes, so no
/// spurious diff results.
#[must_use]
pub fn markdown() -> String {
    let mut out = String::new();
    out.push_str("# Placements\n\n");
    out.push_str(
        "Points and bodies starcat can compute, and the wider catalog it does not\n\
         yet cover. Categories follow the latest IAU designations. Generated from\n\
         `pericynthion::placements::CATALOG` — do not edit by hand; run\n\
         `just placements` to regenerate.\n\n\
         **Supported column:** `yes` means starcat can compute the placement given\n\
         the right data. Bodies whose Notes column references a data source\n\
         (`sb441-n373.bsp`, `Horizons SPK`) require that file to be present or\n\
         the network fetch to have been performed before the computation succeeds.\n\n",
    );
    for category in Category::ORDER {
        if !CATALOG.iter().any(|p| p.category == *category) {
            continue;
        }
        out.push_str("## ");
        out.push_str(category.label());
        out.push_str("\n\n| Name | Supported | Notes |\n|------|-----------|-------|\n");
        for entry in CATALOG.iter().filter(|p| p.category == *category) {
            out.push_str("| ");
            out.push_str(entry.name);
            out.push_str(" | ");
            out.push_str(if entry.supported { "yes" } else { "no" });
            out.push_str(" | ");
            out.push_str(entry.note);
            out.push_str(" |\n");
        }
        out.push('\n');
    }
    // Collapse the trailing blank line to a single newline.
    while out.ends_with("\n\n") {
        out.pop();
    }
    // Append the derived-views section. These are not catalog entries — they
    // are view modifiers that transform the set of placement longitudes already
    // emitted. The Fixed Stars section is appended separately by `starcat placements`
    // via `pericynthion::stars::markdown_stats()`.
    out.push_str(
        "\n## Derived Views\n\n\
         Derived views re-project or augment the placement longitudes already emitted. \
         They are not independently computed bodies — they require the base tropical placement \
         set to be present.\n\n\
         | View | Flag | Description |\n\
         |------|------|-------------|\n\
         | Draconic zodiac | `--draconic` | Re-projects every placement longitude by `(λ − node_lon) mod 360°`, where `node_lon` is the selected North Node (mean or true). Chart-level `zodiac` becomes `{ \"name\": \"draconic\" }` in JZOD output. |\n\
         | Antiscion | `--antiscia` | Appends solstice-axis reflection `(180° − λ) mod 360°` for every body and angle. Bodies equidistant from the Cancer/Capricorn axis share an antiscion. |\n\
         | Contra-antiscion | `--antiscia` | Appends equinox-axis reflection `(360° − λ) mod 360°` for every body and angle. Bodies equidistant from the Aries/Libra axis share a contra-antiscion. |\n",
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_source_classifies_each_body_group() {
        use super::{DataSource, find_by_slug};
        let src = |name: &str| find_by_slug(name).unwrap().data_source();
        assert_eq!(src("Sun"), DataSource::De441);
        assert_eq!(src("Moon"), DataSource::De441);
        assert_eq!(src("Mercury"), DataSource::De441);
        assert_eq!(src("Pluto"), DataSource::De441);
        assert_eq!(src("Ceres"), DataSource::Sb441N16);
        assert_eq!(src("Vesta"), DataSource::Sb441N16);
        assert_eq!(src("Eris"), DataSource::Sb441N373);
        assert_eq!(src("Quaoar"), DataSource::Sb441N373);
        assert_eq!(src("Chiron"), DataSource::Horizons);
        assert_eq!(src("Albion"), DataSource::Horizons);
        assert_eq!(src("Ascendant"), DataSource::Computed);
        assert_eq!(src("Lot of Fortune"), DataSource::Computed);
    }

    #[test]
    fn every_mpc_body_maps_to_a_file_source() {
        use super::{CATALOG, DataSource};
        for p in CATALOG {
            if p.mpc_number.is_some() {
                assert_ne!(
                    p.data_source(),
                    DataSource::Computed,
                    "{} has an MPC number but classified as Computed -- note {:?} unrecognized",
                    p.name,
                    p.note
                );
                assert_ne!(p.data_source(), DataSource::De441, "{}", p.name);
            }
        }
    }

    #[test]
    fn catalog_is_well_formed() {
        // Non-empty, names unique, every category in ORDER is renderable.
        assert!(!CATALOG.is_empty());
        let mut names: Vec<&str> = CATALOG.iter().map(|p| p.name).collect();
        let count = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), count, "placement names must be unique");
        for c in Category::ORDER {
            let _ = c.label(); // every ordered category has a label
        }
    }

    #[test]
    fn known_supported_and_unsupported_membership() {
        let find = |n: &str| CATALOG.iter().find(|p| p.name == n).copied();
        // Supported headliners.
        assert!(find("Sun").unwrap().supported);
        assert!(find("Pluto").unwrap().supported);
        assert!(find("Ceres").unwrap().supported);
        assert!(find("Ascendant").unwrap().supported);
        assert!(find("Lot of Fortune").unwrap().supported);
        // n373 KBO/TNO perturbers now supported.
        assert!(find("Eris").unwrap().supported);
        assert!(find("Gonggong").unwrap().supported);
        // Centaurs now supported via Horizons SPK.
        assert!(find("Chiron").unwrap().supported);
        assert_eq!(find("Chiron").unwrap().category, Category::Centaur);
        assert!(find("Pholus").unwrap().supported);
        assert!(find("Nessus").unwrap().supported);
        assert!(find("Chariklo").unwrap().supported);
    }

    #[test]
    fn supported_iter_matches_flag() {
        let n = supported().count();
        let m = CATALOG.iter().filter(|p| p.supported).count();
        assert_eq!(n, m);
        assert!(supported().all(|p| p.supported));
    }

    #[test]
    fn supported_bodies_track_the_code() {
        // The catalog's supported flag is not editorial guesswork for bodies:
        // every body/asteroid the code can compute MUST appear as supported.
        // If the code gains a body and the catalog is not updated, this fails.
        use crate::body::Body;
        use crate::spk::Asteroid;
        let supported_named = |n: &str| CATALOG.iter().any(|p| p.name == n && p.supported);
        for b in Body::ALL {
            assert!(
                supported_named(b.name()),
                "catalog missing supported body {}",
                b.name()
            );
        }
        for a in Asteroid::ALL {
            assert!(
                supported_named(a.name()),
                "catalog missing supported asteroid {}",
                a.name()
            );
        }
    }

    #[test]
    fn minor_bodies_carry_mpc_numbers() {
        let find = |n: &str| CATALOG.iter().find(|p| p.name == n).copied().unwrap();
        assert_eq!(find("Ceres").mpc_number, Some(1));
        assert_eq!(find("Hygiea").mpc_number, Some(10));
        assert_eq!(find("Chiron").mpc_number, Some(2060));
        assert_eq!(find("Eris").mpc_number, Some(136_199));
        assert_eq!(find("Gonggong").mpc_number, Some(225_088));
        // Bodies that are not numbered minor planets have no MPC number.
        assert_eq!(find("Sun").mpc_number, None);
        assert_eq!(find("Pluto").mpc_number, None); // DE441 body, not the SPK minor planet
        assert_eq!(find("Ascendant").mpc_number, None);
    }

    #[test]
    fn horizons_id_uses_the_20m_scheme_not_sb441() {
        let find = |n: &str| CATALOG.iter().find(|p| p.name == n).copied().unwrap();
        // Horizons stamps 20_000_000 + mpc (NOT the 2_000_000 + mpc of sb441).
        assert_eq!(find("Chiron").horizons_naif_id(), Some(20_002_060));
        assert_eq!(find("Eris").horizons_naif_id(), Some(20_136_199));
        assert_eq!(find("Ceres").horizons_naif_id(), Some(20_000_001));
        assert_eq!(find("Chiron").horizons_command().as_deref(), Some("2060;"));
        assert_eq!(find("Eris").horizons_command().as_deref(), Some("136199;"));
        // No MPC number → no Horizons id/command.
        assert_eq!(find("Sun").horizons_naif_id(), None);
        assert_eq!(find("Sun").horizons_command(), None);
        // sb441 scheme is 2_000_000 + mpc (the bundled-file id).
        assert_eq!(find("Ceres").sb441_naif_id(), Some(2_000_001));
        assert_eq!(find("Hygiea").sb441_naif_id(), Some(2_000_010));
        assert_eq!(find("Chiron").sb441_naif_id(), Some(2_002_060));
        assert_eq!(find("Sun").sb441_naif_id(), None);
    }

    #[test]
    fn markdown_is_deterministic() {
        assert_eq!(markdown(), markdown());
    }

    #[test]
    fn markdown_shape_and_content() {
        let md = markdown();
        assert!(md.starts_with("# Placements\n"));
        // Exactly one trailing newline.
        assert!(md.ends_with('\n'));
        assert!(!md.ends_with("\n\n"));
        // LF only — no carriage returns, no tabs.
        assert!(!md.contains('\r'));
        assert!(!md.contains('\t'));
        // Taxonomy provenance is stated.
        assert!(md.contains("latest IAU designations"));
        // Section headings present in ORDER.
        assert!(md.contains("## Luminaries\n"));
        assert!(md.contains("## Centaurs\n"));
        // A supported row and an unsupported row.
        assert!(md.contains("| Sun | yes |"));
        assert!(md.contains("| Chiron | yes |"));
        // Luminaries section precedes Centaurs section.
        assert!(md.find("## Luminaries").unwrap() < md.find("## Centaurs").unwrap());
    }

    #[test]
    fn supported_list_lists_names_one_per_line() {
        let list = supported_list();
        assert!(list.contains("Sun\n"));
        assert!(list.contains("Ascendant\n"));
        assert!(list.contains("Pluto\n"));
        assert!(list.ends_with('\n'));
        // n373 bodies now in the list.
        assert!(list.contains("Eris\n"));
        assert!(list.contains("Gonggong\n"));
        // Centaurs with Horizons SPK now in the list.
        assert!(list.contains("Chiron\n"));
        assert!(list.contains("Pholus\n"));
        assert!(list.contains("Nessus\n"));
        assert!(list.contains("Chariklo\n"));
        // Line count equals supported count.
        assert_eq!(list.lines().count(), supported().count());
    }

    #[test]
    fn find_by_slug_is_case_insensitive() {
        assert_eq!(find_by_slug("chiron").map(|p| p.name), Some("Chiron"));
        assert_eq!(find_by_slug("CERES").map(|p| p.name), Some("Ceres"));
        assert_eq!(find_by_slug("nonsuch"), None);
    }

    #[test]
    fn name_for_naif_resolves_both_id_schemes() {
        // sb441 scheme (2_000_000 + mpc)
        assert_eq!(name_for_naif(2_000_001), Some("Ceres"));
        assert_eq!(name_for_naif(2_000_010), Some("Hygiea"));
        // Horizons scheme (20_000_000 + mpc)
        assert_eq!(name_for_naif(20_002_060), Some("Chiron"));
        assert_eq!(name_for_naif(20_136_199), Some("Eris"));
        // Not a minor-body id
        assert_eq!(name_for_naif(399), None);
        assert_eq!(name_for_naif(0), None);
        // Base offsets alone (no MPC component) map to nothing.
        assert_eq!(name_for_naif(2_000_000), None);
        assert_eq!(name_for_naif(20_000_000), None);
    }

    #[test]
    fn mpc_numbers_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for p in CATALOG {
            if let Some(mpc) = p.mpc_number {
                assert!(
                    seen.insert(mpc),
                    "duplicate MPC number {mpc} in CATALOG ({})",
                    p.name
                );
            }
        }
    }
}

#[cfg(test)]
mod resolve_tests {
    use super::*;

    #[test]
    fn resolve_prefers_sb441_then_horizons() {
        assert_eq!(
            resolve_body_id("Ceres", |id| id == 2_000_001),
            Ok(2_000_001)
        );
        assert_eq!(
            resolve_body_id("Ceres", |id| id == 20_000_001),
            Ok(20_000_001)
        );
    }

    #[test]
    fn resolve_reports_unknown_not_minor_and_uncovered() {
        assert_eq!(
            resolve_body_id("Nonsuch", |_| true),
            Err(BodyResolveError::Unknown("Nonsuch".to_string()))
        );
        assert_eq!(
            resolve_body_id("Sun", |_| true),
            Err(BodyResolveError::NotMinorBody("Sun"))
        );
        assert_eq!(
            resolve_body_id("Ceres", |_| false),
            Err(BodyResolveError::NotCovered("Ceres"))
        );
    }

    #[test]
    fn omniscient_includes_covered_minor_bodies() {
        let all = omniscient_body_ids(|_| true);
        assert!(all.contains(&2_000_001), "expected Ceres sb441 id");
        // Mathematical points / luminaries have no MPC number → never included.
        assert!(all.iter().all(|&id| id >= 2_000_000));
    }
}
