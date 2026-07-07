//! Yale Bright Star Catalogue (BSC5) — static lookup table and fixed-star engine.
//!
//! [`BSC5_CATALOG`] is parsed once (via [`std::sync::LazyLock`]) from the
//! catalogue text inlined in the private `bsc5_catalogue` module (Yale BSC5P, Hoffleit &
//! Warren 1991). The data is baked into the source — there is no `catalog.gz`
//! in the tree and no decompression at build or run time.
//!
//! J2000 ICRS coordinates. `pm_ra` is the projected proper motion
//! cos(Dec)·dRA/dt (arcsec/yr), matching the BSC5 convention.
//!
//! The fixed-star engine ([`ecliptic_position_from_icrs`], [`compute_star`],
//! [`galactic_center`]) converts any J2000 ICRS direction to the tropical
//! ecliptic position of date via IAU 2006 precession + IAU 2000B nutation.
//! [`CATALOG`] holds 12 traditional fixed stars (Yale BSC5P J2000 coordinates)
//! plus the Galactic Center (Sgr A*, Reid & Brunthaler 2004).

/// One entry from the Yale Bright Star Catalogue, 5th Revised Edition.
#[derive(Debug, Clone, Copy)]
pub struct BscEntry {
    /// Harvard Revised number (1–9110).
    pub hr: u16,
    /// Bayer or Flamsteed designation, e.g. `"21Alp CMa"`, trimmed.
    pub name: &'static str,
    /// J2000 ICRS right ascension in decimal degrees (0–360).
    pub ra_deg: f64,
    /// J2000 ICRS declination in decimal degrees (−90–+90).
    pub dec_deg: f64,
    /// Johnson V magnitude, if present.
    pub vmag: Option<f32>,
    /// Projected proper motion in RA (cos Dec · dRA/dt), arcsec/yr.
    pub pm_ra: Option<f32>,
    /// Proper motion in declination, arcsec/yr.
    pub pm_dec: Option<f32>,
}

impl BscEntry {
    /// Look up an entry by HR number. O(n) scan; catalog is small enough.
    #[must_use]
    pub fn by_hr(hr: u16) -> Option<&'static BscEntry> {
        BSC5_CATALOG.iter().find(|e| e.hr == hr)
    }

    /// Look up by Bayer/Flamsteed name (case-sensitive, trimmed).
    #[must_use]
    pub fn by_name(name: &str) -> Option<&'static BscEntry> {
        BSC5_CATALOG.iter().find(|e| e.name == name)
    }

    /// All entries brighter than or equal to `max_vmag`.
    pub fn brighter_than(max_vmag: f32) -> impl Iterator<Item = &'static BscEntry> {
        BSC5_CATALOG
            .iter()
            .filter(move |e| e.vmag.is_some_and(|v| v <= max_vmag))
    }
}

/// Astrologically notable fixed stars: (common name, HR number), ordered by HR.
///
/// Source: traditional fixed star literature (Robson, Brady), royal stars,
/// first-magnitude stars, and bodies commonly used in modern natal work.
pub const NOTABLE: &[(&str, u16)] = &[
    ("Alpheratz", 15),
    ("Mirach", 337),
    ("Hamal", 617),
    ("Menkar", 911),
    ("Algol", 936),
    ("Alcyone", 1165),   // brightest Pleiad
    ("Aldebaran", 1457), // royal star
    ("Capella", 1708),
    ("Rigel", 1713),
    ("Bellatrix", 1790),
    ("Betelgeuse", 2061),
    ("Sirius", 2491),
    ("Castor", 2891),
    ("Procyon", 2943),
    ("Pollux", 2990),
    ("Gamma Velorum", 3207),
    ("Regulus", 3982), // royal star
    ("Denebola", 4534),
    ("Vindemiatrix", 4932),
    ("Spica", 5056),
    ("Agena", 5267), // Hadar / β Cen
    ("Arcturus", 5340),
    ("Zuben Elgenubi", 5531),
    ("Alphecca", 5793),
    ("Unukalhai", 5854), // α Ser
    ("Antares", 6134),   // royal star
    ("Ras Alhague", 6556),
    ("Vega", 7001),
    ("Altair", 7557),
    ("Deneb", 7924),
    ("Sadalsuud", 8232),
    ("Fomalhaut", 8728), // royal star
    ("Scheat", 8775),
    ("Markab", 8781),
];

/// The embedded CDS `ReadMe` for the Bright Star Catalogue (V/50): the
/// authoritative byte-by-byte record format and provenance for the data behind
/// [`BSC5_CATALOG`]. Returned verbatim from the private `bsc5_catalogue` module.
#[must_use]
pub fn catalogue_provenance() -> &'static str {
    crate::bsc5_catalogue::BSC5_README
}

/// Markdown section summarising BSC5 catalog contents for `docs/placements.md`.
#[must_use]
pub fn markdown_stats() -> String {
    let total = BSC5_CATALOG.len();
    let named = BSC5_CATALOG.iter().filter(|e| !e.name.is_empty()).count();

    if total == 0 {
        return "\n## Fixed Stars — Yale BSC5P\n\n\
            *Catalog not loaded. Run `just fetch bsc5` then rebuild.*\n"
            .to_string();
    }

    // Build the notable-stars inline list from the live catalog.
    let notable_list: Vec<String> = NOTABLE
        .iter()
        .filter_map(|&(common, hr)| {
            BscEntry::by_hr(hr).map(|e| {
                let vmag = e
                    .vmag
                    .map_or_else(|| "—".to_string(), |v| format!("{v:.2}"));
                format!("{common} ({}, HR {hr}, V{vmag})", e.name)
            })
        })
        .collect();

    let others = named.saturating_sub(NOTABLE.len());
    let bullets: String = notable_list
        .iter()
        .map(|s| ["- ", s, "\n"].concat())
        .collect();

    format!(
        "\n## Fixed Stars — Yale BSC5P\n\n\
        {total} stars to V≤6.5 ({named} Bayer/Flamsteed named, {} HR-number only). \
        J2000 ICRS positions; tropical longitude computed via IAU 2006 precession at chart epoch. \
        Source: *The Bright Star Catalogue, 5th Revised Ed.* (Hoffleit & Warren 1991, \
        NASA/NSSDC/ADC — public domain).\n\n\
        **Notable fixed stars** ({others} others not listed):\n\n\
        Listed by Harvard Revised (HR) catalogue number, which runs in order of right ascension.\n\n\
        {bullets}",
        total - named,
    )
}

/// An astrologically used open cluster (not a single star; no BSC5P entry).
/// Direction only — `distance_au` is 0.0 in computed positions.
pub struct StarCluster {
    /// Common astrological name, e.g. `"Aculeus"`.
    pub name: &'static str,
    /// J2000 ICRS right ascension, degrees.
    pub ra_deg: f64,
    /// J2000 ICRS declination, degrees.
    pub dec_deg: f64,
    /// Catalogue designation, e.g. `"M6 Sco (NGC 6405)"`.
    pub object: &'static str,
}

/// Open clusters used as astrological fixed points (Robson / Brady).
/// These have no single-star BSC5P entry.
pub static CLUSTERS: [StarCluster; 3] = [
    StarCluster {
        name: "Aculeus",
        ra_deg: 265.083,
        dec_deg: -32.250,
        object: "M6 Sco (NGC 6405)",
    },
    StarCluster {
        name: "Acumen",
        ra_deg: 268.463,
        dec_deg: -34.800,
        object: "M7 Sco (NGC 6475)",
    },
    StarCluster {
        name: "Capulus",
        ra_deg: 34.750,
        dec_deg: 57.133,
        object: "H Per (NGC 869)",
    },
];

/// Result of resolving a star name or HR number.
pub enum ResolvedStar {
    /// A named entry in the Yale BSC5P catalog.
    Bsc5(&'static BscEntry),
    /// A named fixed direction without a BSC5P entry (Galactic Center only).
    Named(&'static FixedStar),
    /// An open cluster treated as a fixed direction.
    Cluster(&'static StarCluster),
}

impl ResolvedStar {
    /// J2000 ICRS right ascension, degrees.
    #[must_use]
    pub fn ra_deg(&self) -> f64 {
        match self {
            Self::Bsc5(e) => e.ra_deg,
            Self::Named(s) => s.ra_deg,
            Self::Cluster(c) => c.ra_deg,
        }
    }

    /// J2000 ICRS declination, degrees.
    #[must_use]
    pub fn dec_deg(&self) -> f64 {
        match self {
            Self::Bsc5(e) => e.dec_deg,
            Self::Named(s) => s.dec_deg,
            Self::Cluster(c) => c.dec_deg,
        }
    }

    /// Common astrological name. For `Bsc5` entries, returns the common name
    /// from `NOTABLE` if known, otherwise the raw BSC5P designation.
    #[must_use]
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Bsc5(e) => NOTABLE
                .iter()
                .find(|(_, hr)| *hr == e.hr)
                .map_or(e.name, |(name, _)| *name),
            Self::Named(s) => s.name,
            Self::Cluster(c) => c.name,
        }
    }

    /// Tropical ecliptic position of date via IAU 2006 precession + nutation.
    #[must_use]
    pub fn position(&self, jd_tt: f64) -> crate::coords::apparent::EclipticPosition {
        ecliptic_position_from_icrs(self.ra_deg(), self.dec_deg(), jd_tt)
    }
}

/// Iterate only the BSC5P entries that have a non-empty Bayer/Flamsteed name.
pub fn named_bsc5_entries() -> impl Iterator<Item = &'static BscEntry> {
    BSC5_CATALOG.iter().filter(|e| !e.name.is_empty())
}

/// Normalise a user-supplied star name for alias lookup:
/// strip whitespace and hyphens, fold to lowercase.
#[must_use]
pub fn normalize_star_name(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .collect::<String>()
        .to_lowercase()
}

// ── Galactic Center aliases ──────────────────────────────────────────────────
// Resolve to Named(&CATALOG[12]).
const GC_ALIASES: &[&str] = &["galacticcenter", "gc", "sgra", "sgrastar", "sgra*"];

// ── Cluster aliases ──────────────────────────────────────────────────────────
// Maps normalised alias → index into CLUSTERS.
const CLUSTER_ALIASES: &[(&str, usize)] = &[
    ("aculeus", 0),
    ("m6sco", 0),
    ("ngc6405", 0),
    ("acumen", 1),
    ("m7sco", 1),
    ("ngc6475", 1),
    ("capulus", 2),
    ("hper", 2),
    ("ngc869", 2),
    ("hpersei", 2),
];

// ── Star aliases: normalised name → HR number ────────────────────────────────
// Covers: NOTABLE common names, all Robson 87, all Brady 64, synonym pairs,
// and Bayer Greek-letter shorthands. Sorted by HR for readability.
const STAR_ALIASES: &[(&str, u16)] = &[
    // HR 15 — Alpheratz / α And
    ("alpheratz", 15),
    ("alphaand", 15),
    // HR 39 — Algenib / γ Peg (Robson)
    ("algenib", 39),
    ("gammapeg", 39),
    // HR 99 — Ankaa / α Phe (Brady)
    ("ankaa", 99),
    ("alphaphe", 99),
    // HR 168 — Schedar / α Cas (Brady)
    ("schedar", 168),
    ("alphacas", 168),
    // HR 188 — Difda / β Cet (Robson)
    ("difda", 188),
    ("betcet", 188),
    // HR 337 — Mirach / β And
    ("mirach", 337),
    ("betand", 337),
    // HR 424 — Polaris / α UMi (Robson / Brady)
    ("polaris", 424),
    ("alphaumi", 424),
    ("northstar", 424),
    // HR 437 — Al Pherg / η Psc (Robson)
    ("alpherg", 437),
    ("etapsc", 437),
    // HR 472 — Achernar / α Eri (Robson / Brady)
    ("achernar", 472),
    ("alphaeri", 472),
    // HR 539 — Baten Kaitos / ζ Cet (Robson)
    ("batenkaitos", 539),
    ("zetacet", 539),
    // HR 553 — Sharatan / β Ari (Robson)
    ("sharatan", 553),
    ("betari", 553),
    // HR 596 — Al Rescha / α Psc (Brady)
    ("alrescha", 596),
    ("alphapsc", 596),
    // HR 603 — Almach / γ And (Robson)
    ("almach", 603),
    ("gammaand", 603),
    // HR 617 — Hamal / α Ari (Robson / NOTABLE)
    ("hamal", 617),
    ("alphaari", 617),
    // HR 911 — Menkar / α Cet (NOTABLE)
    ("menkar", 911),
    ("alphacet", 911),
    // HR 936 — Algol / β Per
    ("algol", 936),
    ("betper", 936),
    ("demonstar", 936),
    // HR 1017 — Mirfak / α Per (Brady)
    ("mirfak", 1017),
    ("alphaper", 1017),
    // HR 1165 — Alcyone / η Tau
    ("alcyone", 1165),
    ("etatau", 1165),
    // HR 1346 — Prima Hyadum / γ Tau (Robson)
    ("primahyadum", 1346),
    ("gammatau", 1346),
    // HR 1457 — Aldebaran / α Tau
    ("aldebaran", 1457),
    ("alphatau", 1457),
    // HR 1708 — Capella / α Aur
    ("capella", 1708),
    ("alphaaur", 1708),
    // HR 1713 — Rigel / β Ori
    ("rigel", 1713),
    ("betori", 1713),
    // HR 1790 — Bellatrix / γ Ori (Robson / Brady)
    ("bellatrix", 1790),
    ("gammaori", 1790),
    // HR 1791 — El Nath / β Tau (Robson / Brady)
    ("elnath", 1791),
    ("betatau", 1791),
    // HR 1852 — Mintaka / δ Ori; also Robson's "Cingula Orionis" (belt primary)
    ("mintaka", 1852),
    ("deltaori", 1852),
    ("cingulaorionis", 1852),
    // HR 1903 — Alnilam / ε Ori (Robson / Brady)
    ("alnilam", 1903),
    ("epsilonori", 1903),
    // HR 1910 — Al Hecka / ζ Tau (Robson)
    ("alhecka", 1910),
    ("zetatau", 1910),
    // HR 1956 — Phact / α Col (Brady)
    ("phact", 1956),
    ("alphacol", 1956),
    // HR 2061 — Betelgeuse; Robson spells "Betelgeuze"
    ("betelgeuse", 2061),
    ("betelgeuze", 2061),
    ("alphaori", 2061),
    // HR 2088 — Menkalinan / β Aur (Robson)
    ("menkalinan", 2088),
    ("betaaur", 2088),
    // HR 2216 — Propus / η Gem (Robson)
    ("propus", 2216),
    ("etagemi", 2216),
    // HR 2286 — Dirah / μ Gem (Robson)
    ("dirah", 2286),
    ("mugem", 2286),
    // HR 2326 — Canopus / α Car (Robson / Brady)
    ("canopus", 2326),
    ("alphacar", 2326),
    // HR 2421 — Alhena / γ Gem (Brady)
    ("alhena", 2421),
    ("gamgem", 2421),
    // HR 2491 — Sirius / α CMa
    ("sirius", 2491),
    ("alphacma", 2491),
    ("dogstar", 2491),
    // HR 2891 — Castor / α Gem (Robson / Brady)
    ("castor", 2891),
    ("alphagem", 2891),
    // HR 2943 — Procyon / α CMi (Robson / Brady)
    ("procyon", 2943),
    ("alphacmi", 2943),
    // HR 2990 — Pollux / β Gem (Robson / Brady)
    ("pollux", 2990),
    ("betgem", 2990),
    // HR 3165 — Pelagus / ζ Pup (Robson)
    ("pelagus", 3165),
    ("zetapup", 3165),
    // HR 3207 — Gamma Velorum / γ² Vel (NOTABLE; alias Regor)
    ("gammavelorum", 3207),
    ("regor", 3207),
    // HR 3572 — Acubens / α Cnc (Robson / Brady)
    ("acubens", 3572),
    ("alphacnc", 3572),
    // HR 3734 — Markeb / κ Vel (Robson)
    ("markeb", 3734),
    ("kappavel", 3734),
    // HR 3748 — Alphard / α Hya (Robson / Brady)
    ("alphard", 3748),
    ("alphahya", 3748),
    // HR 3873 — Algenubi / ε Leo (Robson)
    ("algenubi", 3873),
    ("epsilonleo", 3873),
    // HR 3975 — Al Jabhah / η Leo (Robson)
    ("aljabhah", 3975),
    ("etaleo", 3975),
    // HR 3982 — Regulus / α Leo
    ("regulus", 3982),
    ("alphaleo", 3982),
    // HR 4031 — Adhafera / ζ Leo (Robson)
    ("adhafera", 4031),
    ("zetaleo", 4031),
    // HR 4210 — Foramen / η Car (Robson; LBV, variable magnitude)
    ("foramen", 4210),
    ("etacar", 4210),
    // HR 4287 — Alkes / α Crt (Brady)
    ("alkes", 4287),
    ("alphacrt", 4287),
    // HR 4301 — Dubhe / α UMa (Brady)
    ("dubhe", 4301),
    ("alphauma", 4301),
    // HR 4357 — Zosma / δ Leo (Robson / Brady)
    ("zosma", 4357),
    ("deltaleo", 4357),
    // HR 4382 — Labrum / δ Crt (Robson)
    ("labrum", 4382),
    ("deltacrt", 4382),
    // HR 4534 — Denebola / β Leo (NOTABLE)
    ("denebola", 4534),
    ("betleo", 4534),
    // HR 4540 — Zavijava / β Vir (Robson)
    ("zavijava", 4540),
    ("betavir", 4540),
    // HR 4689 — Zaniah / η Vir (Robson)
    ("zaniah", 4689),
    ("etavir", 4689),
    // HR 4730 — Acrux / α Cru (Robson / Brady; primary component Alp1Cru)
    ("acrux", 4730),
    ("alphacru", 4730),
    // HR 4932 — Vindemiatrix / ε Vir (NOTABLE)
    ("vindemiatrix", 4932),
    ("epsilonvir", 4932),
    // HR 4968 — Diadem / α Com (Brady)
    ("diadem", 4968),
    ("alphacom", 4968),
    // HR 5056 — Spica / α Vir
    ("spica", 5056),
    ("alphavir", 5056),
    // HR 5267 — Agena / β Cen (Robson / Brady; also Hadar)
    ("agena", 5267),
    ("betcen", 5267),
    ("hadar", 5267),
    // HR 5291 — Thuban / α Dra (Brady)
    ("thuban", 5291),
    ("alphadra", 5291),
    // HR 5340 — Arcturus / α Boo
    ("arcturus", 5340),
    ("alphaboo", 5340),
    // HR 5359 — Khambalia / λ Vir (Robson)
    ("khambalia", 5359),
    ("lambdavir", 5359),
    // HR 5435 — Seginus / γ Boo (Robson)
    ("seginus", 5435),
    ("gammaboo", 5435),
    // HR 5459 — Bungula (Robson) = Toliman (Brady) = α Cen
    ("bungula", 5459),
    ("toliman", 5459),
    ("alphacen", 5459),
    // HR 5531 — Zuben Elgenubi / α Lib (Brady)
    ("zubenelgenubi", 5531),
    ("alphalib", 5531),
    // HR 5681 — Princeps / δ Boo (Robson)
    ("princeps", 5681),
    ("deltaboo", 5681),
    // HR 5685 — Zuben Eschamali / β Lib (Brady)
    ("zubeneschamali", 5685),
    ("betalib", 5685),
    // HR 5793 — Alphecca / α CrB (NOTABLE)
    ("alphecca", 5793),
    ("alphacorb", 5793),
    // HR 5854 — Unukalhai / α Ser (NOTABLE)
    ("unukalhai", 5854),
    ("alphaserp", 5854),
    // HR 5984 — Graffias / β Sco (Robson)
    ("graffias", 5984),
    ("betsco", 5984),
    // HR 6056 — Yed Prior / δ Oph (Robson)
    ("yedprior", 6056),
    ("deltaoph", 6056),
    // HR 6134 — Antares / α Sco
    ("antares", 6134),
    ("alphasco", 6134),
    // HR 6175 — Han / ζ Oph (Robson)
    ("han", 6175),
    ("zetaoph", 6175),
    // HR 6378 — Sabik / η Oph (Robson)
    ("sabik", 6378),
    ("etaoph", 6378),
    // HR 6406 — Ras Algethi / α Her (Brady)
    ("rasalgethi", 6406),
    ("alphaher", 6406),
    // HR 6508 — Lesath / υ Sco (Robson)
    ("lesath", 6508),
    ("upsilonsco", 6508),
    // HR 6556 — Rasalhague (Robson) / Ras Alhague (Brady) / α Oph; normalise to same
    ("rasalhague", 6556),
    ("alphaoph", 6556),
    // HR 6698 — Sinistra / ν Oph (Robson)
    ("sinistra", 6698),
    ("nuoph", 6698),
    // HR 6812 — Polis / μ Sgr (Robson)
    ("polis", 6812),
    ("musgr", 6812),
    // HR 7001 — Vega / Wega / α Lyr (NOTABLE; Robson spells Wega)
    ("vega", 7001),
    ("wega", 7001),
    ("alphalyr", 7001),
    // HR 7194 — Ascella / ζ Sgr (Robson)
    ("ascella", 7194),
    ("zetasgr", 7194),
    // HR 7217 — Manubrium / o Sgr (Robson)
    ("manubrium", 7217),
    ("omisgr", 7217),
    // HR 7348 — Rukbat / α Sgr (Brady)
    ("rukbat", 7348),
    ("alphasgr", 7348),
    // HR 7417 — Albireo / β Cyg (Robson)
    ("albireo", 7417),
    ("betcyg", 7417),
    // HR 7557 — Altair / α Aql (Robson / NOTABLE)
    ("altair", 7557),
    ("alphaaql", 7557),
    // HR 7747 — Giedi / α Cap (Robson)
    ("giedi", 7747),
    ("alphacap", 7747),
    // HR 7776 — Dabih / β Cap (Robson)
    ("dabih", 7776),
    ("betcap", 7776),
    // HR 7814 — Oculus / π Cap (Robson)
    ("oculus", 7814),
    ("picap", 7814),
    // HR 7906 — Sualocin / α Del (Brady)
    ("sualocin", 7906),
    ("alphadel", 7906),
    // HR 7924 — Deneb (Robson) / Deneb Adige (Brady) / α Cyg
    ("deneb", 7924),
    ("denebadige", 7924),
    ("alphacyg", 7924),
    // HR 8060 — Armus / η Cap (Robson)
    ("armus", 8060),
    ("etacap", 8060),
    // HR 8162 — Alderamin / α Cep (Brady)
    ("alderamin", 8162),
    ("alphacep", 8162),
    // HR 8232 — Sadalsuud / β Aqr (NOTABLE)
    ("sadalsuud", 8232),
    ("betaqr", 8232),
    // HR 8260 — Castra / ε Cap (Robson)
    ("castra", 8260),
    ("epsiloncap", 8260),
    // HR 8278 — Nashira / γ Cap (Robson)
    ("nashira", 8278),
    ("gammacap", 8278),
    // HR 8322 — Deneb Algedi / δ Cap (Robson / Brady)
    ("denebalgedi", 8322),
    ("deltacap", 8322),
    // HR 8414 — Sadalmelik (Robson) / Sadalmelek (Brady) / α Aqr
    ("sadalmelik", 8414),
    ("sadalmelek", 8414),
    ("alphaaqr", 8414),
    // HR 8709 — Skat / δ Aqr (Robson)
    ("skat", 8709),
    ("deltaaqr", 8709),
    // HR 8728 — Fomalhaut / α PsA
    ("fomalhaut", 8728),
    ("alphapsa", 8728),
    // HR 8775 — Scheat / β Peg (NOTABLE)
    ("scheat", 8775),
    ("betpeg", 8775),
    // HR 8781 — Markab / α Peg (NOTABLE)
    ("markab", 8781),
    ("alphapeg", 8781),
];

/// Resolve a star by common name, Robson/Brady alias, BSC5P designation, or HR number.
///
/// Input is normalised (stripped of whitespace, lowercased) before lookup.
/// Resolution order:
/// 1. Galactic Center aliases → `Named(&CATALOG[12])`
/// 2. Cluster aliases → `Cluster(&CLUSTERS[i])`
/// 3. `STAR_ALIASES` table → `BscEntry::by_hr` → `Bsc5`
/// 4. `BSC5_CATALOG` name-field scan (after normalising the BSC5 name) → `Bsc5`
/// 5. Parse as decimal HR number (optionally prefixed `"HR"`) → `Bsc5`
#[must_use]
pub fn resolve_star(input: &str) -> Option<ResolvedStar> {
    let norm = normalize_star_name(input);
    if norm.is_empty() {
        return None;
    }

    // 1. Galactic Center
    if GC_ALIASES.contains(&norm.as_str()) {
        return Some(ResolvedStar::Named(&CATALOG[12]));
    }

    // 2. Open clusters
    if let Some(&(_, idx)) = CLUSTER_ALIASES.iter().find(|(k, _)| *k == norm) {
        return Some(ResolvedStar::Cluster(&CLUSTERS[idx]));
    }

    // 3. Alias table → HR → BSC5 entry
    if let Some(&(_, hr)) = STAR_ALIASES.iter().find(|(k, _)| *k == norm)
        && let Some(entry) = BscEntry::by_hr(hr)
    {
        return Some(ResolvedStar::Bsc5(entry));
    }

    // 4. BSC5_CATALOG name-field scan (normalise BSC5 name for comparison)
    if let Some(entry) = BSC5_CATALOG
        .iter()
        .find(|e| !e.name.is_empty() && normalize_star_name(e.name) == norm)
    {
        return Some(ResolvedStar::Bsc5(entry));
    }

    // 5. HR number parse: strip optional "hr" prefix then parse
    let hr_str = norm.strip_prefix("hr").unwrap_or(&norm);
    if let Ok(hr) = hr_str.parse::<u16>()
        && let Some(entry) = BscEntry::by_hr(hr)
    {
        return Some(ResolvedStar::Bsc5(entry));
    }

    None
}

use crate::coords::{
    apparent::EclipticPosition,
    nutation::{nutate_mean_to_true, nutation},
    obliquity::mean_obliquity_rad,
    precession::precess_j2000_to_date,
    transform::{Vector3, equatorial_to_ecliptic, latitude_rad, longitude_rad},
};

/// A fixed star (or fixed direction) defined by its J2000 ICRS equatorial
/// coordinates. Proper motion is not tracked.
pub struct FixedStar {
    /// Common name used in output (e.g. `"Regulus"`, `"Galactic Center"`).
    pub name: &'static str,
    /// J2000 ICRS right ascension in degrees, range [0, 360).
    pub ra_deg: f64,
    /// J2000 ICRS declination in degrees, range [−90, +90].
    pub dec_deg: f64,
}

/// Curated catalog of 12 traditional fixed stars plus the Galactic Center.
///
/// Coordinates: Yale BSC5P (Hoffleit & Warren 1991) J2000 ICRS for the 12
/// stars (see HR numbers in inline comments); Sgr A* from Reid & Brunthaler
/// 2004. Distinct from [`BSC5_CATALOG`] (all 9,096 entries); this slice is
/// the astrologically curated subset.
pub static CATALOG: [FixedStar; 13] = [
    FixedStar {
        name: "Algol",
        ra_deg: 47.042,
        dec_deg: 40.956,
    }, // β Per HR 936
    FixedStar {
        name: "Alcyone",
        ra_deg: 56.871,
        dec_deg: 24.105,
    }, // η Tau HR 1165
    FixedStar {
        name: "Aldebaran",
        ra_deg: 68.980,
        dec_deg: 16.509,
    }, // α Tau HR 1457
    FixedStar {
        name: "Rigel",
        ra_deg: 78.634,
        dec_deg: -8.202,
    }, // β Ori HR 1713
    FixedStar {
        name: "Capella",
        ra_deg: 79.172,
        dec_deg: 45.998,
    }, // α Aur HR 1708
    FixedStar {
        name: "Sirius",
        ra_deg: 101.287,
        dec_deg: -16.716,
    }, // α CMa HR 2491
    FixedStar {
        name: "Pollux",
        ra_deg: 116.329,
        dec_deg: 28.026,
    }, // β Gem HR 2990
    FixedStar {
        name: "Regulus",
        ra_deg: 152.093,
        dec_deg: 11.967,
    }, // α Leo HR 3982
    FixedStar {
        name: "Spica",
        ra_deg: 201.299,
        dec_deg: -11.161,
    }, // α Vir HR 5056
    FixedStar {
        name: "Arcturus",
        ra_deg: 213.915,
        dec_deg: 19.182,
    }, // α Boo HR 5340
    FixedStar {
        name: "Antares",
        ra_deg: 247.352,
        dec_deg: -26.432,
    }, // α Sco HR 6134
    FixedStar {
        name: "Fomalhaut",
        ra_deg: 344.413,
        dec_deg: -29.622,
    }, // α PsA HR 8728
    FixedStar {
        name: "Galactic Center",
        ra_deg: 266.417,
        dec_deg: -29.008,
    }, // Sgr A* Reid+2004
];

/// Convert a fixed J2000 ICRS direction to the tropical ecliptic position of
/// date using precession and nutation.
///
/// Pipeline: ICRS unit vector → precess (J2000→mean of date) → nutate (mean→
/// true equatorial of date) → rotate to ecliptic of date.
///
/// `distance_au` is set to 0.0 (direction-only; no physical distance in AU).
#[must_use]
pub fn ecliptic_position_from_icrs(ra_deg: f64, dec_deg: f64, jd_tt: f64) -> EclipticPosition {
    let v = icrs_unit_vector(ra_deg, dec_deg);
    let v_mean = precess_j2000_to_date(&v, jd_tt);
    let eps_mean = mean_obliquity_rad(jd_tt);
    let v_true = nutate_mean_to_true(&v_mean, jd_tt, eps_mean);
    let eps_true = eps_mean + nutation(jd_tt).delta_epsilon;
    let v_ecl = equatorial_to_ecliptic(&v_true, eps_true);
    EclipticPosition {
        longitude_deg: longitude_rad(&v_ecl).to_degrees(),
        latitude_deg: latitude_rad(&v_ecl).to_degrees(),
        distance_au: 0.0,
    }
}

/// Compute the tropical ecliptic position of a catalog star at `jd_tt`.
#[must_use]
pub fn compute_star(star: &FixedStar, jd_tt: f64) -> EclipticPosition {
    ecliptic_position_from_icrs(star.ra_deg, star.dec_deg, jd_tt)
}

/// Tropical ecliptic position of the Galactic Center (Sgr A*) at `jd_tt`.
///
/// Convenience wrapper around the last entry in [`CATALOG`].
/// Longitude drifts ~1.4°/century due to precession of the vernal equinox.
#[must_use]
pub fn galactic_center(jd_tt: f64) -> EclipticPosition {
    compute_star(&CATALOG[12], jd_tt)
}

fn icrs_unit_vector(ra_deg: f64, dec_deg: f64) -> Vector3 {
    let ra = ra_deg.to_radians();
    let dec = dec_deg.to_radians();
    let cos_dec = dec.cos();
    [cos_dec * ra.cos(), cos_dec * ra.sin(), dec.sin()]
}

/// The Yale Bright Star Catalogue, parsed once from the embedded raw text in
/// the private `bsc5_catalogue` module (`BSC5_RAW`). Indexed lookups live on [`BscEntry`].
pub static BSC5_CATALOG: std::sync::LazyLock<Vec<BscEntry>> = std::sync::LazyLock::new(parse_bsc5);

/// Slice a fixed-width column `[a, b)` from a record line, clamping the end to
/// the line length. Records are pure ASCII, so byte and char offsets coincide.
fn col(line: &'static str, a: usize, b: usize) -> &'static str {
    line.get(a..b.min(line.len())).unwrap_or("")
}

/// Parse the inlined BSC5 catalogue into typed entries. Byte offsets follow the
/// CDS V/50 record layout documented in [`crate::bsc5_catalogue::BSC5_README`].
fn parse_bsc5() -> Vec<BscEntry> {
    crate::bsc5_catalogue::BSC5_RAW
        .lines()
        .filter_map(parse_bsc5_line)
        .collect()
}

/// Parse one BSC5 record line, or `None` for headers/blank lines and records
/// without J2000 coordinates (the handful of non-stellar objects).
fn parse_bsc5_line(line: &'static str) -> Option<BscEntry> {
    if line.len() < 90 {
        return None;
    }
    let hr: u16 = col(line, 0, 4).trim().parse().ok()?;
    let name = col(line, 4, 14).trim();
    // J2000 RA (bytes 75–82) and Dec (sign at 83, bytes 84–89).
    let ra_h: f64 = col(line, 75, 77).trim().parse().ok()?;
    let ra_m: f64 = col(line, 77, 79).trim().parse().ok()?;
    let ra_s: f64 = col(line, 79, 83).trim().parse().ok()?;
    let dec_sign = line.as_bytes()[83];
    let dec_d: f64 = col(line, 84, 86).trim().parse().ok()?;
    let dec_m: f64 = col(line, 86, 88).trim().parse().ok()?;
    let dec_s: f64 = col(line, 88, 90).trim().parse().ok()?;
    // V magnitude (bytes 102–106) and proper motion (RA 148–153, Dec 154–159).
    let vmag = col(line, 102, 107).trim().parse().ok();
    let pm_ra = col(line, 148, 154).trim().parse().ok();
    let pm_dec = col(line, 154, 160).trim().parse().ok();

    let ra_deg = (ra_h + ra_m / 60.0 + ra_s / 3600.0) * 15.0;
    let dec_abs = dec_d + dec_m / 60.0 + dec_s / 3600.0;
    let dec_deg = if dec_sign == b'-' { -dec_abs } else { dec_abs };

    Some(BscEntry {
        hr,
        name,
        ra_deg,
        dec_deg,
        vmag,
        pm_ra,
        pm_dec,
    })
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod fixed_star_tests {
    use super::*;

    #[test]
    fn regulus_j2000_is_late_leo() {
        // Regulus α Leo, HR 3982 — tropical longitude near 29°48' Leo (~149.8°) at J2000.
        let pos = compute_star(&CATALOG[7], 2_451_545.0);
        assert!(
            (148.0..152.0).contains(&pos.longitude_deg),
            "Regulus longitude {:.3}° not in [148, 152]",
            pos.longitude_deg
        );
    }

    #[test]
    fn galactic_center_j2000_is_late_sagittarius() {
        let pos = galactic_center(2_451_545.0);
        // Sgr A* ≈ 26°51' Sagittarius = ~266.85° ecliptic longitude at J2000.
        assert!(
            (266.0..268.0).contains(&pos.longitude_deg),
            "GC longitude {:.3}° not in [266, 268]",
            pos.longitude_deg
        );
        assert!(
            (-7.0..-4.0).contains(&pos.latitude_deg),
            "GC latitude {:.3}° not in [−7, −4]",
            pos.latitude_deg
        );
        assert_eq!(pos.distance_au, 0.0);
    }

    #[test]
    fn all_catalog_stars_have_unique_names() {
        let names: std::collections::HashSet<_> = CATALOG.iter().map(|s| s.name).collect();
        assert_eq!(names.len(), CATALOG.len(), "duplicate star name in catalog");
    }

    #[test]
    fn all_catalog_longitudes_are_in_range() {
        for star in &CATALOG {
            let pos = compute_star(star, 2_451_545.0);
            assert!(
                (0.0..360.0).contains(&pos.longitude_deg),
                "{} longitude {:.3}° out of [0, 360)",
                star.name,
                pos.longitude_deg
            );
            assert!(
                (-90.0..=90.0).contains(&pos.latitude_deg),
                "{} latitude {:.3}° out of [−90, 90]",
                star.name,
                pos.latitude_deg
            );
            assert_eq!(pos.distance_au, 0.0);
        }
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod resolve_tests {
    use super::*;

    #[test]
    fn embedded_catalogue_parses_in_full_with_provenance() {
        // The inlined BSC5 raw text must parse to the full ~9110-record catalogue
        // (a handful of non-stellar rows lack coordinates and are dropped).
        let n = BSC5_CATALOG.len();
        assert!(
            (9000..=9110).contains(&n),
            "expected the full BSC5, parsed {n} entries"
        );
        // Provenance ships alongside the data.
        assert!(catalogue_provenance().contains("Bright Star Catalogue"));
        // Spot-check a landmark record survived parsing.
        assert_eq!(
            BscEntry::by_hr(2491).map(|e| e.name.is_empty()),
            Some(false)
        ); // Sirius
    }

    #[test]
    fn resolve_by_common_name_case_insensitive() {
        let r = resolve_star("Sirius").expect("Sirius not found");
        assert!(matches!(r, ResolvedStar::Bsc5(e) if e.hr == 2491));
        let r2 = resolve_star("sirius").expect("sirius not found");
        assert!(matches!(r2, ResolvedStar::Bsc5(e) if e.hr == 2491));
    }

    #[test]
    fn resolve_by_hr_string() {
        let r = resolve_star("2491").expect("2491 not found");
        assert!(matches!(r, ResolvedStar::Bsc5(e) if e.hr == 2491));
        let r2 = resolve_star("HR2491").expect("HR2491 not found");
        assert!(matches!(r2, ResolvedStar::Bsc5(e) if e.hr == 2491));
    }

    #[test]
    fn resolve_multiword_concatenated() {
        // "Zuben Elgenubi" → "zubenelgenubi"
        let r = resolve_star("ZubenElgenubi").expect("ZubenElgenubi not found");
        assert!(matches!(r, ResolvedStar::Bsc5(e) if e.hr == 5531));
        let r2 = resolve_star("Zuben Elgenubi").expect("Zuben Elgenubi not found");
        assert!(matches!(r2, ResolvedStar::Bsc5(e) if e.hr == 5531));
    }

    #[test]
    fn resolve_variant_spellings() {
        // Wega / Vega
        let r1 = resolve_star("Wega").expect("Wega not found");
        let r2 = resolve_star("Vega").expect("Vega not found");
        assert!(matches!(r1, ResolvedStar::Bsc5(e) if e.hr == 7001));
        assert!(matches!(r2, ResolvedStar::Bsc5(e) if e.hr == 7001));
        // Bungula / Toliman
        let b = resolve_star("Bungula").expect("Bungula not found");
        let t = resolve_star("Toliman").expect("Toliman not found");
        assert!(matches!(b, ResolvedStar::Bsc5(e) if e.hr == 5459));
        assert!(matches!(t, ResolvedStar::Bsc5(e) if e.hr == 5459));
        // Sadalmelik / Sadalmelek
        let m1 = resolve_star("Sadalmelik").expect("Sadalmelik not found");
        let m2 = resolve_star("Sadalmelek").expect("Sadalmelek not found");
        assert!(matches!(m1, ResolvedStar::Bsc5(e) if e.hr == 8414));
        assert!(matches!(m2, ResolvedStar::Bsc5(e) if e.hr == 8414));
        // Rasalhague / Ras Alhague
        let ra1 = resolve_star("Rasalhague").expect("Rasalhague not found");
        let ra2 = resolve_star("Ras Alhague").expect("Ras Alhague not found");
        assert!(matches!(ra1, ResolvedStar::Bsc5(e) if e.hr == 6556));
        assert!(matches!(ra2, ResolvedStar::Bsc5(e) if e.hr == 6556));
    }

    #[test]
    fn resolve_galactic_center() {
        for alias in &[
            "galacticcenter",
            "GalacticCenter",
            "gc",
            "GC",
            "sgra",
            "SgrA",
        ] {
            let r = resolve_star(alias).unwrap_or_else(|| panic!("{alias} not found"));
            assert!(
                matches!(r, ResolvedStar::Named(fs) if fs.name == "Galactic Center"),
                "{alias} did not resolve to GC"
            );
        }
    }

    #[test]
    fn resolve_clusters() {
        let a = resolve_star("Aculeus").expect("Aculeus not found");
        assert!(matches!(a, ResolvedStar::Cluster(c) if c.name == "Aculeus"));
        let ac = resolve_star("acumen").expect("acumen not found");
        assert!(matches!(ac, ResolvedStar::Cluster(c) if c.name == "Acumen"));
        let cap = resolve_star("Capulus").expect("Capulus not found");
        assert!(matches!(cap, ResolvedStar::Cluster(c) if c.name == "Capulus"));
    }

    #[test]
    fn resolve_unknown_returns_none() {
        assert!(resolve_star("xyzzy").is_none());
        assert!(resolve_star("HR99999").is_none());
        assert!(resolve_star("").is_none());
    }

    #[test]
    fn resolved_star_position_returns_valid_ecliptic() {
        let r = resolve_star("Sirius").unwrap();
        let pos = r.position(2_451_545.0);
        assert!((0.0..360.0).contains(&pos.longitude_deg));
        assert_eq!(pos.distance_au, 0.0);
    }

    #[test]
    fn display_name_prefers_common_over_bsc5_designation() {
        let r = resolve_star("algol").unwrap();
        assert_eq!(r.display_name(), "Algol"); // common name, not "26Bet Per"
    }

    #[test]
    fn named_bsc5_entries_only_nonempty_names() {
        let named: Vec<_> = named_bsc5_entries().collect();
        assert!(!named.is_empty());
        assert!(named.iter().all(|e| !e.name.is_empty()));
        assert!(named.len() < BSC5_CATALOG.len()); // fewer than total
    }

    #[test]
    fn markdown_stats_contains_hr_ordering_rationale() {
        let md = markdown_stats();
        assert!(
            md.contains("order of right ascension"),
            "markdown_stats() must document the HR ordering rationale; got:\n{md}"
        );
    }

    #[test]
    fn resolve_gamma_velorum_by_name_and_alias() {
        // Display name and the Regor alias both land on HR 3207.
        assert!(matches!(
            resolve_star("Gamma Velorum"),
            Some(ResolvedStar::Bsc5(e)) if e.hr == 3207
        ));
        assert!(matches!(
            resolve_star("Regor"),
            Some(ResolvedStar::Bsc5(e)) if e.hr == 3207
        ));
    }

    #[test]
    fn gamma_velorum_is_notable() {
        assert!(
            NOTABLE
                .iter()
                .any(|&(name, hr)| name == "Gamma Velorum" && hr == 3207)
        );
    }

    #[test]
    fn notable_is_strictly_hr_sorted() {
        assert!(
            NOTABLE.windows(2).all(|w| w[0].1 < w[1].1),
            "NOTABLE must be strictly ascending by HR number",
        );
    }
}
