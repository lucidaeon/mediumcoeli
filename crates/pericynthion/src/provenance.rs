//! Provenance join: which data file backs which catalogued body/star, where it
//! comes from (URL), and its cache-relative path. Pure — no env, no disk.

use crate::jpl::oracle::{self, STAR_CLASS_ALL, SourceKind};
use crate::placements::CATALOG;

/// JPL Horizons API endpoint (duplicated here so provenance compiles without
/// the `horizons` cargo feature, which gates `crate::horizons`).
pub const HORIZONS_API_URL: &str = "https://ssd.jpl.nasa.gov/api/horizons.api";

/// Where a provider's file is resolved from at the CLI layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RootKind {
    /// Under `$STARCAT_JPL_DATA` mirror root (path = `rel_path` joined to it).
    JplMirror,
    /// Inlined into the binary from the CDS `catalog.gz` (now baked verbatim as
    /// source in `bsc5_catalogue.rs`; no build step or decompression at run time).
    CdsBuild,
    /// Under `$STARCAT_HORIZONS_DATA` (file = `rel_path`, i.e. `<naif>.bsp`).
    HorizonsDir,
}

/// One way to obtain the data backing a body or star.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Provider {
    /// Upstream family.
    pub kind: SourceKind,
    /// How the CLI resolves a local path.
    pub root_kind: RootKind,
    /// Path relative to its root (full mirror-relative path for JPL/CDS,
    /// `<naif>.bsp` for Horizons).
    pub rel_path: String,
    /// Where it comes from on the web.
    pub source_url: String,
    /// Optional coverage gloss.
    pub coverage: Option<&'static str>,
}

fn jpl_url(prefix: &str, name: &str) -> String {
    format!("https://{prefix}/{name}")
}

fn horizons_url(naif_id: i32) -> String {
    // Representative GET form of the SPK request (see crate::horizons::spk_query).
    format!("{HORIZONS_API_URL}?format=json&EPHEM_TYPE=SPK&COMMAND={naif_id}")
}

/// True for a path in starcat's production default entourage (the DE441
/// integration and its `asteroids_de441` perturber SPKs). Used only to rank
/// providers: many DE integrations declare the same bodies, but starcat computes
/// production placements from DE441, so it is reported as the primary source.
fn is_de441_family(path: &str) -> bool {
    path.contains("/de441/") || path.contains("/asteroids_de441/")
}

/// Manifest providers (JPL mirror + CDS) whose `provides` names this body.
///
/// Many DE integrations back the same body (every DE binary provides the ten
/// planets; both the DE430 and DE441 asteroid companions carry the main belt).
/// The production DE441 family is sorted first so it is the reported primary
/// source; the older integrations follow as selectable alternates in mirror
/// order. The sort is stable, so equal-rank rows keep their manifest order.
fn manifest_providers(body: &str) -> Vec<Provider> {
    let mut out = Vec::new();
    for d in oracle::manifest_dirs() {
        for f in d.files {
            if f.provides.contains(&body) {
                let root_kind = match d.kind {
                    SourceKind::JplMirror => RootKind::JplMirror,
                    SourceKind::CdsCatalog => RootKind::CdsBuild,
                    SourceKind::HorizonsSpk => RootKind::HorizonsDir,
                };
                out.push(Provider {
                    kind: d.kind,
                    root_kind,
                    rel_path: format!("{}/{}", d.prefix, f.name),
                    source_url: jpl_url(d.prefix, f.name),
                    coverage: f.coverage,
                });
            }
        }
    }
    out.sort_by_key(|p| u8::from(!is_de441_family(&p.rel_path)));
    out
}

/// Every provider that can supply `name` (a catalogued body display name):
/// any bundle/DE441 row that lists it, plus a synthesized Horizons provider
/// if the body carries an MPC number.
#[must_use]
pub fn providers_for_body(name: &str) -> Vec<Provider> {
    let mut out = manifest_providers(name);
    if let Some(p) = CATALOG.iter().find(|p| p.name == name)
        && let Some(naif) = p.horizons_naif_id()
    {
        out.push(Provider {
            kind: SourceKind::HorizonsSpk,
            root_kind: RootKind::HorizonsDir,
            rel_path: format!("{naif}.bsp"),
            source_url: horizons_url(naif),
            coverage: None,
        });
    }
    out
}

/// Providers for the fixed-star catalogue (the `catalog.gz` CDS row).
#[must_use]
pub fn fixed_star_providers() -> Vec<Provider> {
    let mut out = Vec::new();
    for d in oracle::manifest_dirs() {
        for f in d.files {
            if f.provides.contains(&STAR_CLASS_ALL) {
                out.push(Provider {
                    kind: d.kind,
                    root_kind: RootKind::CdsBuild,
                    rel_path: f.name.to_string(),
                    source_url: jpl_url(d.prefix, f.name),
                    coverage: f.coverage,
                });
            }
        }
    }
    out
}

/// One observed file mapped back to the oracle's mirror URL(s), for
/// `generator`/provenance reporting. Distinct from [`jzod::DataSource`]
/// (which this crate does not depend on outside the `jzod` feature) so a
/// bare, non-jzod build of this crate stays fully independent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataSourceInfo {
    /// One or more mirror URLs for the same byte-identical artifact.
    pub urls: Vec<String>,
    /// Local cache basename, if the artifact is (or would be) on disk.
    /// `None` for the baked-in fixed-star catalogue, which has no real
    /// on-disk path.
    pub cached: Option<String>,
}

/// Map an observed file (as recorded in [`crate::chart::SourceUse::path`])
/// back to the oracle's mirror URL(s) it can be fetched from.
///
/// Resolution order:
/// 1. The `catalog.gz` fixed-star sentinel (no real on-disk path — the
///    catalogue is baked into the binary): returns every mirror carrying
///    [`STAR_CLASS_ALL`] (`catalog.gz` and `ybsc5.gz`, both byte-identical
///    but hosted under different names), `cached: None`.
/// 2. Any oracle file whose basename matches `path`'s basename: returns
///    every mirror URL for that basename (there can be more than one, e.g.
///    a file mirrored under two hosts with the same name), `cached` set to
///    the basename.
/// 3. Otherwise, treat `path` as a Horizons-fetched `<naif>.bsp` (not in the
///    oracle, since Horizons SPKs are generated on demand, never mirrored):
///    synthesizes the Horizons API URL from the numeric stem, `cached` set
///    to the basename. If the stem isn't numeric, the URL list is empty.
#[must_use]
pub fn urls_for_observed(path: &std::path::Path) -> DataSourceInfo {
    let basename = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();

    if basename == "catalog.gz" {
        let urls = oracle::manifest_dirs()
            .iter()
            .flat_map(|d| d.files.iter().map(move |f| (d, f)))
            .filter(|(_, f)| f.provides.contains(&STAR_CLASS_ALL))
            .map(|(d, f)| jpl_url(d.prefix, f.name))
            .collect();
        return DataSourceInfo { urls, cached: None };
    }

    let urls: Vec<String> = oracle::manifest_dirs()
        .iter()
        .flat_map(|d| d.files.iter().map(move |f| (d, f)))
        .filter(|(_, f)| f.name == basename)
        .map(|(d, f)| jpl_url(d.prefix, f.name))
        .collect();
    if !urls.is_empty() {
        return DataSourceInfo {
            urls,
            cached: Some(basename.to_string()),
        };
    }

    // Not in the oracle: a Horizons-generated SPK, named `<naif>.bsp`.
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    let urls = stem
        .parse::<i32>()
        .map(|naif| vec![horizons_url(naif)])
        .unwrap_or_default();
    DataSourceInfo {
        urls,
        cached: Some(basename.to_string()),
    }
}

/// One observed data source folded to a reporting row: the provenance key, the
/// local cache basename (if any), and every mirror URL it resolves to.
///
/// Produced by [`observed_sources`]. This is the shared, format-neutral shape
/// that both the JZOD serializer (`ephemeris.sources`) and the CLI's human
/// "data sources" section build from, so the fold rule lives in one place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceRow {
    /// Provenance category or body key (e.g. `"planets"`, `"fixed_stars"`).
    pub key: String,
    /// Local cache basename, or `None` for a built-in source (the baked-in
    /// fixed-star catalogue) — mirrors [`DataSourceInfo::cached`].
    pub cached: Option<String>,
    /// Every mirror URL for this source (possibly empty).
    pub urls: Vec<String>,
}

/// Fold observed provenance into one [`SourceRow`] per key, first-seen wins.
///
/// Walks `provenance` in order, keeping the first [`crate::chart::SourceUse`]
/// seen for each `key` and resolving its path through [`urls_for_observed`].
/// A key could in principle be observed against more than one path across the
/// fold, so the first mapping is kept. `compute_with_spk` already sorts and
/// de-duplicates `(key, path)` pairs, so the surviving row is deterministic.
///
/// The returned order follows the (already-sorted) `provenance` order. Both the
/// JZOD `ephemeris.sources` map and the CLI's human "data sources" section are
/// built from this list so the fold rule is defined exactly once.
#[must_use]
pub fn observed_sources(provenance: &[crate::chart::SourceUse]) -> Vec<SourceRow> {
    let mut seen = std::collections::BTreeSet::new();
    let mut rows = Vec::new();
    for u in provenance {
        if !seen.insert(u.key.clone()) {
            continue;
        }
        let info = urls_for_observed(&u.path);
        rows.push(SourceRow {
            key: u.key.clone(),
            cached: info.cached,
            urls: info.urls,
        });
    }
    rows
}

/// Catalogued minor bodies with no bundle provider — the ones `data prod`
/// must list under the Horizons dir. Returns (display name, horizons naif id).
#[must_use]
pub fn production_horizons_targets() -> Vec<(&'static str, i32)> {
    CATALOG
        .iter()
        .filter(|p| p.mpc_number.is_some())
        .filter(|p| {
            // No bundle (JplMirror/CdsCatalog) row lists this body.
            !manifest_providers(p.name)
                .iter()
                .any(|pr| pr.kind != SourceKind::HorizonsSpk)
        })
        .filter_map(|p| p.horizons_naif_id().map(|n| (p.name, n)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_computable_catalog_body_has_a_provider() {
        use crate::placements::{CATALOG, Category};
        for p in CATALOG
            .iter()
            .filter(|p| p.category != Category::MathematicalPoint)
        {
            assert!(
                !providers_for_body(p.name).is_empty(),
                "no provider for {}",
                p.name
            );
        }
    }

    #[test]
    fn planets_resolve_to_de441_jpl_mirror() {
        let provs = providers_for_body("Mars");
        assert!(
            provs.iter().any(|p| p.kind == SourceKind::JplMirror
                && p.rel_path.ends_with("linux_m13000p17000.441"))
        );
    }

    #[test]
    fn bundle_body_has_both_bundle_and_horizons_providers() {
        // Eris is in sb441-n373.bsp AND fetchable from Horizons.
        let provs = providers_for_body("Eris");
        assert!(provs.iter().any(|p| p.rel_path.ends_with("sb441-n373.bsp")));
        assert!(provs.iter().any(|p| p.kind == SourceKind::HorizonsSpk));
    }

    #[test]
    fn horizons_only_body_has_only_horizons_provider() {
        // Chiron is in no bundle; Horizons file is 20002060.bsp (20_000_000 + 2060).
        let provs = providers_for_body("Chiron");
        assert_eq!(provs.len(), 1);
        assert_eq!(provs[0].kind, SourceKind::HorizonsSpk);
        assert_eq!(provs[0].rel_path, "20002060.bsp");
        assert!(provs[0].source_url.starts_with(HORIZONS_API_URL));
    }

    #[test]
    fn jpl_url_is_https_prefix_name() {
        let p = providers_for_body("Ceres")
            .into_iter()
            .find(|p| p.kind == SourceKind::JplMirror)
            .unwrap();
        assert_eq!(
            p.source_url,
            "https://ssd.jpl.nasa.gov/ftp/eph/small_bodies/asteroids_de441/sb441-n16.bsp"
        );
    }

    #[test]
    fn fixed_stars_come_from_cds_v50() {
        let provs = fixed_star_providers();
        let cds = provs
            .iter()
            .find(|p| p.kind == SourceKind::CdsCatalog)
            .unwrap();
        assert_eq!(cds.rel_path, "catalog.gz");
        assert_eq!(
            cds.source_url,
            "https://cdsarc.cds.unistra.fr/ftp/cats/V/50/catalog.gz"
        );
        assert_eq!(cds.root_kind, RootKind::CdsBuild);
    }

    #[test]
    fn production_horizons_targets_are_the_unbundled_minor_bodies() {
        let names: Vec<&str> = production_horizons_targets()
            .iter()
            .map(|(n, _)| *n)
            .collect();
        assert!(names.contains(&"Chiron"));
        assert!(names.contains(&"Albion"));
        assert!(!names.contains(&"Eris")); // bundled in n373
        assert!(!names.contains(&"Ceres")); // bundled in n16
    }

    #[test]
    fn fixed_star_sentinel_yields_both_mirror_urls_no_cache() {
        let info = urls_for_observed(std::path::Path::new("catalog.gz"));
        assert_eq!(info.urls.len(), 2);
        assert!(
            info.urls
                .iter()
                .any(|u| u.contains("cdsarc.cds.unistra.fr"))
        );
        assert!(info.urls.iter().any(|u| u.contains("tdc-www.harvard.edu")));
        assert!(info.cached.is_none());
    }

    #[test]
    fn horizons_bsp_synthesizes_api_url_and_cached_name() {
        let info = urls_for_observed(std::path::Path::new("/x/horizons/20000002.bsp"));
        assert!(
            info.urls[0].contains("horizons.api?") && info.urls[0].contains("COMMAND=20000002")
        );
        assert_eq!(info.cached.as_deref(), Some("20000002.bsp"));
    }

    #[test]
    fn observed_sources_folds_first_seen_wins_per_key() {
        use crate::chart::SourceUse;
        use std::path::PathBuf;
        let provenance = vec![
            SourceUse {
                key: "asteroids".to_string(),
                path: PathBuf::from("/data/nasa/small_bodies/asteroids_de441/sb441-n16.bsp"),
            },
            // Duplicate key with a different path — must be ignored (first wins).
            SourceUse {
                key: "asteroids".to_string(),
                path: PathBuf::from("/somewhere/else/sb441-n373.bsp"),
            },
            SourceUse {
                key: "fixed_stars".to_string(),
                path: PathBuf::from("catalog.gz"),
            },
        ];
        let rows = observed_sources(&provenance);
        assert_eq!(rows.len(), 2, "duplicate key folded to one row");
        assert_eq!(rows[0].key, "asteroids");
        assert_eq!(rows[0].cached.as_deref(), Some("sb441-n16.bsp"));
        assert_eq!(
            rows[0].urls,
            vec![
                "https://ssd.jpl.nasa.gov/ftp/eph/small_bodies/asteroids_de441/sb441-n16.bsp"
                    .to_string()
            ]
        );
        // Fixed-star sentinel: no cache, two mirror URLs.
        assert_eq!(rows[1].key, "fixed_stars");
        assert!(rows[1].cached.is_none());
        assert_eq!(rows[1].urls.len(), 2);
    }

    #[test]
    fn observed_sources_empty_provenance_is_empty() {
        assert!(observed_sources(&[]).is_empty());
    }

    #[test]
    fn mirrored_bundle_file_yields_its_jpl_mirror_url_with_cache() {
        // A real oracle-known file (not the star sentinel, not a Horizons SPK)
        // resolves to its single JPL mirror URL, cached under its own basename.
        let info = urls_for_observed(std::path::Path::new(
            "/data/nasa/small_bodies/asteroids_de441/sb441-n16.bsp",
        ));
        assert_eq!(info.urls.len(), 1);
        assert_eq!(
            info.urls[0],
            "https://ssd.jpl.nasa.gov/ftp/eph/small_bodies/asteroids_de441/sb441-n16.bsp"
        );
        assert_eq!(info.cached.as_deref(), Some("sb441-n16.bsp"));
    }
}
