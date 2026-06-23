//! # astrogram
//!
//! Astrology data-format conversion library.
//!
//! `astrogram` reduces transcription errors when moving chart data between
//! astrology platforms. It reads structured chart exports from one platform
//! and writes them in the format expected by another, removing the manual
//! copy-paste step where mistakes happen.
//!
//! ## Supported formats
//!
//! - **Read:** Solar Fire `.SFcht` binary, Astrodatabank XML,
//!   AAF (Astrolog Ascii Format), Zeus `.zdb`.
//! - **Write:** Solar Fire `.SFcht` binary, Astrodatabank XML, Zeus `.zdb`.
//! - **Extract** (authenticated web): `lunaastrology.com`, `astro.com`, `astrotheoros.com`.
//!
//! Deferred formats (Quick\*Chart, Solar Fire text export, `JZOD`,
//! `Nechepso`, `TimeCycles`, SQL/`SQLite`) and additional extractors are
//! not yet implemented.
//!
//! ## Canonical chart type
//!
//! [`chart::Chart`] is the in-memory representation every reader produces and
//! every writer consumes. Sign-convention mismatches between formats (Solar
//! Fire's `+West` longitude vs. ISO 6709's `+East`, etc.) are resolved at the
//! format boundary — never inside `Chart`.
//!
//! ## Format specifications
//!
//! Authoritative format docs and a Kaitai Struct definition for `.SFcht`
//! are in the `research/` directory at the repository root (symlinked from
//! the external research archive). Reference Python prototypes live there
//! too; treat them as oracles, not as source material to transliterate.

#![cfg_attr(docsrs, feature(doc_cfg))]
#![warn(missing_docs)]
#![warn(rustdoc::missing_crate_level_docs)]
#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]

/// AAF (Astrolog Ascii Format) parser.
pub mod aaf;
pub mod adbxml;
/// astro.com HTTP API helpers.
pub mod astrocom;
pub mod astrotheoros;
pub mod capability;
pub mod chart;
pub mod consolidate;
/// Format-agnostic bytes dispatch: [`convert::read_bytes`] / [`convert::write_bytes`].
pub mod convert;
/// Cookie-import facade (requires the `cookie-import` Cargo feature).
///
/// Provides `import_credential` — a closed facade over the `wristband`
/// crate that maps web providers to their session cookies and extracts the
/// credential material to seed a fall-through auth chain.  Consumers depend
/// only on `astrogram`; the `wristband` crate is never named directly.
#[cfg(feature = "cookie-import")]
pub mod cookie_import;
pub mod decision_log;
pub mod error;
pub mod format;
/// JZOD v0.0.0 writer.
pub mod jzod;
pub mod luna;
pub mod normalize;
/// Raw key: value text writer (debug/inspection).
pub mod raw;
pub mod sfcht;
#[cfg(test)]
mod test_support;
pub mod transcript;
pub mod util;
pub mod web_auth;
pub mod zeus;
