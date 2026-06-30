//! Canonical format identity and registry.
//!
//! [`Format`] is the single enum every consumer (CLI, GUI) uses to name a chart
//! data format. [`FORMATS`] is the registry: slug, medium, credential shape,
//! file extensions, read/write direction, and per-field capabilities. The
//! capabilities live beside each writer (see each format module's
//! `READ_CAPS`/`WRITE_CAPS`).

use crate::capability::{CapabilitySet, ChartField};
use std::path::Path;

/// A chart data format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Format {
    /// Solar Fire `.SFcht` binary.
    Sfcht,
    /// Zeus `.zdb`.
    Zeus,
    /// Astrodatabank XML.
    Adb,
    /// AAF (Astrolog Ascii Format) — read-only.
    Aaf,
    /// lunaastrology.com account.
    Luna,
    /// astro.com account.
    Astrocom,
    /// astrotheoros.com account.
    Astrotheoros,
    /// JZOD v0.0.0 JSON — write-only.
    Json,
    /// Raw key: value text — write-only.
    Raw,
}

/// Whether a format is a local file or a remote web endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Local file.
    File,
    /// Remote web account.
    Web,
}

/// Credential shape of a format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Auth {
    /// No credentials (file formats).
    None,
    /// Token only.
    Token,
    /// user/pass login OR a token (login takes priority).
    LoginOrToken,
}

/// Canonical descriptor for one format.
pub struct FormatSpec {
    /// The format this row describes.
    pub format: Format,
    /// Single lowercase token: enum spelling, flag prefix, env prefix all derive from it.
    pub slug: &'static str,
    /// File or web.
    pub kind: Kind,
    /// Credential shape.
    pub auth: Auth,
    /// File extensions (lowercase, no dot). Empty for web formats.
    pub extensions: &'static [&'static str],
    /// Whether the library can read this format.
    pub can_read: bool,
    /// Whether the library can write this format.
    pub can_write: bool,
    /// Fields recovered when reading.
    pub read_caps: CapabilitySet,
    /// Fields persisted when writing.
    pub write_caps: CapabilitySet,
}

/// One row of the format-support matrix — the data behind
/// `blackmoon --capabilities`. Derived from [`FORMATS`]; never authored
/// separately, so it cannot drift from the registry.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CapabilityRow {
    /// Format slug (e.g. `"sfcht"`).
    pub slug: &'static str,
    /// `"file"` or `"web"`.
    pub kind: &'static str,
    /// Credential shape: `"none"`, `"token"`, or `"login|token"`.
    pub auth: &'static str,
    /// Whether the library can read this format.
    pub can_read: bool,
    /// Whether the library can write this format.
    pub can_write: bool,
    /// Human labels of the chart fields PRESERVED on write (sorted).
    pub preserved_on_write: Vec<&'static str>,
    /// Human labels of the chart fields DROPPED on write (sorted).
    pub dropped_on_write: Vec<&'static str>,
}

/// Build the format-support matrix from [`FORMATS`] — one [`CapabilityRow`] per
/// registry entry, in registry order. The single source of truth for "what does
/// this library read/write, and what survives a write".
#[must_use]
pub fn capability_matrix() -> Vec<CapabilityRow> {
    FORMATS
        .iter()
        .map(|spec| {
            let mut preserved: Vec<&'static str> = ChartField::ALL
                .iter()
                .filter(|&&f| spec.write_caps.preserves(f))
                .map(|f| f.label())
                .collect();
            let mut dropped: Vec<&'static str> = ChartField::ALL
                .iter()
                .filter(|&&f| !spec.write_caps.preserves(f))
                .map(|f| f.label())
                .collect();
            preserved.sort_unstable();
            dropped.sort_unstable();
            CapabilityRow {
                slug: spec.slug,
                kind: match spec.kind {
                    Kind::File => "file",
                    Kind::Web => "web",
                },
                auth: match spec.auth {
                    Auth::None => "none",
                    Auth::Token => "token",
                    Auth::LoginOrToken => "login|token",
                },
                can_read: spec.can_read,
                can_write: spec.can_write,
                preserved_on_write: preserved,
                dropped_on_write: dropped,
            }
        })
        .collect()
}

/// The format registry — one row per [`Format`].
pub const FORMATS: &[FormatSpec] = &[
    FormatSpec {
        format: Format::Sfcht,
        slug: "sfcht",
        kind: Kind::File,
        auth: Auth::None,
        extensions: &["sfcht"],
        can_read: true,
        can_write: true,
        read_caps: crate::sfcht::READ_CAPS,
        write_caps: crate::sfcht::WRITE_CAPS,
    },
    FormatSpec {
        format: Format::Zeus,
        slug: "zeus",
        kind: Kind::File,
        auth: Auth::None,
        extensions: &["zdb"],
        can_read: true,
        can_write: true,
        read_caps: crate::zeus::READ_CAPS,
        write_caps: crate::zeus::WRITE_CAPS,
    },
    FormatSpec {
        format: Format::Adb,
        slug: "adb",
        kind: Kind::File,
        auth: Auth::None,
        extensions: &["xml"],
        can_read: true,
        can_write: true,
        read_caps: crate::adbxml::READ_CAPS,
        write_caps: crate::adbxml::WRITE_CAPS,
    },
    FormatSpec {
        format: Format::Aaf,
        slug: "aaf",
        kind: Kind::File,
        auth: Auth::None,
        extensions: &["aaf"],
        can_read: true,
        can_write: false,
        read_caps: crate::aaf::READ_CAPS,
        write_caps: crate::aaf::WRITE_CAPS,
    },
    FormatSpec {
        format: Format::Luna,
        slug: "luna",
        kind: Kind::Web,
        auth: Auth::Token,
        extensions: &[],
        can_read: true,
        can_write: true,
        read_caps: crate::luna::READ_CAPS,
        write_caps: crate::luna::WRITE_CAPS,
    },
    FormatSpec {
        format: Format::Astrocom,
        slug: "astrocom",
        kind: Kind::Web,
        auth: Auth::LoginOrToken,
        extensions: &[],
        can_read: true,
        can_write: true,
        read_caps: crate::astrocom::READ_CAPS,
        write_caps: crate::astrocom::WRITE_CAPS,
    },
    FormatSpec {
        format: Format::Astrotheoros,
        slug: "astrotheoros",
        kind: Kind::Web,
        auth: Auth::LoginOrToken,
        extensions: &[],
        can_read: true,
        can_write: true,
        read_caps: crate::astrotheoros::READ_CAPS,
        write_caps: crate::astrotheoros::WRITE_CAPS,
    },
    FormatSpec {
        format: Format::Json,
        slug: "json",
        kind: Kind::File,
        auth: Auth::None,
        extensions: &["json"],
        can_read: false,
        can_write: true,
        read_caps: crate::jzod::READ_CAPS,
        write_caps: crate::jzod::WRITE_CAPS,
    },
    FormatSpec {
        format: Format::Raw,
        slug: "raw",
        kind: Kind::File,
        auth: Auth::None,
        extensions: &["raw"],
        can_read: false,
        can_write: true,
        read_caps: crate::raw::READ_CAPS,
        write_caps: crate::raw::WRITE_CAPS,
    },
];

impl Format {
    /// The full registry.
    #[must_use]
    pub fn all() -> &'static [FormatSpec] {
        FORMATS
    }

    /// This format's descriptor row.
    ///
    /// # Panics
    /// Never in practice: every `Format` variant has exactly one `FormatSpec`
    /// in [`FORMATS`] (enforced by the `every_format_has_exactly_one_spec` test).
    #[must_use]
    pub fn spec(self) -> &'static FormatSpec {
        FORMATS
            .iter()
            .find(|s| s.format == self)
            .expect("every Format has a FormatSpec")
    }

    /// Detect a file format by extension (case-insensitive).
    #[must_use]
    pub fn from_path(path: &Path) -> Option<Format> {
        let ext = path.extension()?.to_str()?.to_ascii_lowercase();
        FORMATS
            .iter()
            .find(|s| s.extensions.contains(&ext.as_str()))
            .map(|s| s.format)
    }

    /// Parse a format from its slug.
    #[must_use]
    pub fn from_slug(s: &str) -> Option<Format> {
        FORMATS
            .iter()
            .find(|spec| spec.slug == s)
            .map(|spec| spec.format)
    }

    /// Fields recovered when reading this format.
    #[must_use]
    pub fn read_caps(self) -> CapabilitySet {
        self.spec().read_caps
    }

    /// Fields persisted when writing this format.
    #[must_use]
    pub fn write_caps(self) -> CapabilitySet {
        self.spec().write_caps
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::ChartField;

    #[test]
    fn every_format_has_exactly_one_spec() {
        for f in Format::all().iter().map(|s| s.format) {
            let n = FORMATS.iter().filter(|s| s.format == f).count();
            assert_eq!(n, 1, "{f:?} must have exactly one FormatSpec");
        }
    }

    #[test]
    fn slug_roundtrips_through_from_slug() {
        for s in FORMATS {
            assert_eq!(
                Format::from_slug(s.slug),
                Some(s.format),
                "slug {} broke",
                s.slug
            );
        }
    }

    #[test]
    fn kind_determines_auth() {
        for s in FORMATS {
            match (s.kind, s.auth) {
                (Kind::File, Auth::None) | (Kind::Web, Auth::Token | Auth::LoginOrToken) => {}
                _ => panic!("{} has mismatched kind/auth", s.slug),
            }
        }
    }

    #[test]
    fn file_formats_have_extensions_web_do_not() {
        for s in FORMATS {
            match s.kind {
                Kind::File => assert!(!s.extensions.is_empty(), "{} needs an extension", s.slug),
                Kind::Web => assert!(s.extensions.is_empty(), "{} must have no extension", s.slug),
            }
        }
    }

    #[test]
    fn from_path_matches_extensions() {
        assert_eq!(
            Format::from_path(std::path::Path::new("x.SFcht")),
            Some(Format::Sfcht)
        );
        assert_eq!(
            Format::from_path(std::path::Path::new("x.zdb")),
            Some(Format::Zeus)
        );
        assert_eq!(
            Format::from_path(std::path::Path::new("x.xml")),
            Some(Format::Adb)
        );
        assert_eq!(
            Format::from_path(std::path::Path::new("x.aaf")),
            Some(Format::Aaf)
        );
        assert_eq!(Format::from_path(std::path::Path::new("x.txt")), None);
    }

    #[test]
    fn caps_reference_valid_fields_and_write_only_when_writable() {
        for s in FORMATS {
            if !s.can_write {
                assert_eq!(
                    s.write_caps.fields().len(),
                    0,
                    "{} is read-only but has write_caps",
                    s.slug
                );
            }
            // touch a field to ensure the vocab type is wired
            let _ = s.read_caps.preserves(ChartField::Region);
        }
    }

    #[test]
    fn capability_matrix_has_a_row_per_format() {
        let rows = capability_matrix();
        assert_eq!(rows.len(), FORMATS.len());
        assert_eq!(rows.len(), 9, "expected 9 registry formats");
    }

    #[test]
    fn capability_matrix_reports_direction_auth_and_loss() {
        let rows = capability_matrix();
        let sfcht = rows.iter().find(|r| r.slug == "sfcht").expect("sfcht row");
        assert!(sfcht.can_read && sfcht.can_write);
        assert_eq!(sfcht.kind, "file");
        assert_eq!(sfcht.auth, "none");
        assert!(sfcht.dropped_on_write.is_empty(), "sfcht is full-fidelity");

        let aaf = rows.iter().find(|r| r.slug == "aaf").expect("aaf row");
        assert!(aaf.can_read && !aaf.can_write, "aaf is read-only");

        let astrocom = rows
            .iter()
            .find(|r| r.slug == "astrocom")
            .expect("astrocom row");
        assert_eq!(astrocom.kind, "web");
        assert_eq!(astrocom.auth, "login|token");
        // astro.com cannot persist these per-chart:
        assert!(astrocom.dropped_on_write.contains(&"house system"));
        assert!(astrocom.dropped_on_write.contains(&"zodiac"));
    }
}
