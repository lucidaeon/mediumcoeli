//! Exit-code taxonomy for `starcat`.
//!
//! Deliberately duplicated in `blackmoon` (see `crates/blackmoon/src/exit.rs`)
//! rather than factored into a shared crate — each CLI owns its own copy,
//! carrying only the codes it actually emits. `starcat` never authenticates
//! against a remote account and never refuses a lossy write (those are
//! `blackmoon`-only concepts), so `ExitClass::{ChartParse, Auth, LossyRefused,
//! NeedInput}` (6, 7, 9, 10) are not part of this CLI's taxonomy — the gaps
//! are intentional; cross-app number alignment is a free nicety, not an
//! enforced invariant.
//!
//! `classify()` maps a caught `anyhow::Error` to an [`ExitClass`] by
//! downcasting to the concrete error types starcat can produce or propagate.
//! `main()` returns `std::process::ExitCode` via `classify(&e).exit_code()`.

/// The exit-code classes `starcat` can emit. See the module doc comment for
/// why `ChartParse`/`Auth`/`LossyRefused`/`NeedInput` are absent from this
/// list.
///
/// `Success` is never produced by `classify` (the `Ok` path in `main` returns
/// `ExitCode::SUCCESS` directly) — kept for taxonomy completeness.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ExitClass {
    #[allow(dead_code)]
    Success,
    Internal,
    Usage,
    Input,
    NotFound,
    Integrity,
    Network,
    Io,
}

impl ExitClass {
    pub fn code(self) -> u8 {
        match self {
            ExitClass::Success => 0,
            ExitClass::Internal => 1,
            ExitClass::Usage => 2,
            ExitClass::Input => 3,
            ExitClass::NotFound => 4,
            ExitClass::Integrity => 5,
            ExitClass::Network => 8,
            ExitClass::Io => 11,
        }
    }

    pub fn exit_code(self) -> std::process::ExitCode {
        std::process::ExitCode::from(self.code())
    }
}

/// A CLI usage error: a required flag/argument combination was not
/// satisfied, or a branch that only ever guides the user (e.g. bare `data
/// fetch` with no dataset) terminated without doing any work. Distinct from
/// clap's own parse errors (which exit 2 before `run()` is ever called) —
/// this covers post-parse usage failures.
///
/// Classifies as `ExitClass::Usage` (code 2).
#[derive(Debug)]
pub struct UsageError {
    /// Human-readable explanation of what usage requirement was not met.
    pub message: String,
}

impl std::fmt::Display for UsageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for UsageError {}

/// No ephemeris data (or other required local dataset) could be found —
/// neither `--jpl-data`/`--root`, `$STARCAT_JPL_DATA`, nor the platform data
/// dir resolved to a usable location.
///
/// Classifies as `ExitClass::NotFound` (code 4).
#[derive(Debug)]
pub struct NotFoundError {
    /// Human-readable explanation of what was missing.
    pub message: String,
}

impl std::fmt::Display for NotFoundError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for NotFoundError {}

/// The user-supplied input itself was invalid — an unresolvable body slug, an
/// out-of-range value, or similar — as opposed to a usage error (a flag
/// combination) or a missing resource (`NotFoundError`).
///
/// Classifies as `ExitClass::Input` (code 3).
#[derive(Debug)]
pub struct InputError {
    /// Human-readable explanation of what was invalid.
    pub message: String,
}

impl std::fmt::Display for InputError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for InputError {}

/// A `data verify` run found a present-but-corrupt file (size or BLAKE3
/// mismatch against the oracle record) — distinct from a merely absent file.
///
/// Classifies as `ExitClass::Integrity` (code 5).
#[derive(Debug)]
pub struct IntegrityError {
    /// Human-readable explanation naming the scope and failure count.
    pub message: String,
}

impl std::fmt::Display for IntegrityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for IntegrityError {}

/// Downcast a caught `anyhow::Error` to the [`ExitClass`] `main` should exit
/// with. Checked in order from most specific to least; the first match wins.
#[must_use]
pub fn classify(err: &anyhow::Error) -> ExitClass {
    if err.downcast_ref::<IntegrityError>().is_some() {
        return ExitClass::Integrity;
    }
    if err.downcast_ref::<UsageError>().is_some() {
        return ExitClass::Usage;
    }
    if err.downcast_ref::<InputError>().is_some() {
        return ExitClass::Input;
    }
    if err.downcast_ref::<NotFoundError>().is_some() {
        return ExitClass::NotFound;
    }
    if let Some(e) = err.downcast_ref::<pericynthion::error::PericynthionError>() {
        use pericynthion::error::PericynthionError as E;
        return match e {
            E::Io { .. } => ExitClass::Io,
            E::Header(_) => ExitClass::Integrity,
            E::UnknownAyanamshaSlug { .. } | E::DraconicNodeUnavailable => ExitClass::Input,
        };
    }
    if let Some(e) = err.downcast_ref::<pericynthion::BodyResolveError>() {
        use pericynthion::BodyResolveError as E;
        return match e {
            E::Unknown(_) | E::NotMinorBody(_) => ExitClass::Input,
            E::NotCovered(_) => ExitClass::NotFound,
        };
    }
    if let Some(e) = err.downcast_ref::<pericynthion::horizons::HorizonsError>() {
        return horizons_error_class(e);
    }
    if let Some(e) = err.downcast_ref::<pericynthion::FetchError>() {
        use pericynthion::FetchError as E;
        return match e {
            E::Http { .. } => ExitClass::Network,
            E::Io { .. } => ExitClass::Io,
            E::Verify { .. } => ExitClass::Integrity,
        };
    }
    ExitClass::Internal
}

/// Map a single [`pericynthion::horizons::HorizonsError`] to its [`ExitClass`].
///
/// `Http`/`Json`/`Base64` are all transport/wire-format failures talking to
/// the Horizons API — `Network` (8). `NoSpk` means Horizons answered but has
/// no SPK for the requested body/span — a missing resource, not a transport
/// fault — so `NotFound` (4). `Io` is a local filesystem failure — `Io` (11).
#[must_use]
pub fn horizons_error_class(e: &pericynthion::horizons::HorizonsError) -> ExitClass {
    use pericynthion::horizons::HorizonsError as E;
    match e {
        E::Http(_) | E::Json(_) | E::Base64(_) => ExitClass::Network,
        E::NoSpk(_) => ExitClass::NotFound,
        E::Io { .. } => ExitClass::Io,
    }
}

/// Reduce a batch of `starcat horizons` per-body failures to the single
/// [`ExitClass`] the whole run should exit with.
///
/// A single failing class exits as that class. A mixed batch exits by this
/// precedence — most local/actionable cause first — regardless of how many
/// bodies failed for each reason: **`Io`(11) > `NotFound`(4) > `Network`(8) >
/// `Internal`(1)** (the `Internal` fallback only fires on an empty slice,
/// which callers should not pass). Every failure's individual cause is
/// already printed to stderr per-body as the batch runs, so a mixed cause
/// remains visible to the user regardless of which single class the process
/// exits with.
#[must_use]
pub fn horizons_batch_class<'a>(
    errors: impl IntoIterator<Item = &'a pericynthion::horizons::HorizonsError>,
) -> ExitClass {
    let mut saw_network = false;
    let mut saw_not_found = false;
    let mut saw_io = false;
    for e in errors {
        match horizons_error_class(e) {
            ExitClass::Io => saw_io = true,
            ExitClass::NotFound => saw_not_found = true,
            ExitClass::Network => saw_network = true,
            _ => {}
        }
    }
    if saw_io {
        ExitClass::Io
    } else if saw_not_found {
        ExitClass::NotFound
    } else if saw_network {
        ExitClass::Network
    } else {
        ExitClass::Internal
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_match_canonical_taxonomy() {
        // starcat's trimmed subset — no ChartParse(6)/Auth(7)/LossyRefused(9)/
        // NeedInput(10); see the module doc comment for why. The gaps are
        // intentional.
        use ExitClass::*;
        let pairs = [
            (Success, 0u8),
            (Internal, 1),
            (Usage, 2),
            (Input, 3),
            (NotFound, 4),
            (Integrity, 5),
            (Network, 8),
            (Io, 11),
        ];
        for (c, n) in pairs {
            assert_eq!(c.code(), n);
        }
    }
}

#[cfg(test)]
mod classify_tests {
    use super::*;

    fn anyhow_of<E: std::error::Error + Send + Sync + 'static>(e: E) -> anyhow::Error {
        anyhow::Error::new(e)
    }

    #[test]
    fn usage_error_classifies_usage() {
        let e = UsageError {
            message: "specify a dataset".to_string(),
        };
        assert_eq!(classify(&anyhow_of(e)), ExitClass::Usage);
    }

    #[test]
    fn not_found_error_classifies_not_found() {
        let e = NotFoundError {
            message: "no ephemeris data found".to_string(),
        };
        assert_eq!(classify(&anyhow_of(e)), ExitClass::NotFound);
    }

    #[test]
    fn input_error_classifies_input() {
        let e = InputError {
            message: "unknown body \"foobar\" (not in the placements catalog)".to_string(),
        };
        assert_eq!(classify(&anyhow_of(e)), ExitClass::Input);
    }

    #[test]
    fn integrity_error_classifies_integrity() {
        let e = IntegrityError {
            message: "1 file(s) failed verification".to_string(),
        };
        assert_eq!(classify(&anyhow_of(e)), ExitClass::Integrity);
    }

    #[test]
    fn pericynthion_io_error_classifies_io() {
        let e = pericynthion::error::PericynthionError::Io {
            path: std::path::PathBuf::from("/nonexistent"),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "not found"),
        };
        assert_eq!(classify(&anyhow_of(e)), ExitClass::Io);
    }

    #[test]
    fn pericynthion_header_error_classifies_integrity() {
        let e = pericynthion::error::PericynthionError::Header(
            pericynthion::error::HeaderError::UnexpectedEnd { group: 1040 },
        );
        assert_eq!(classify(&anyhow_of(e)), ExitClass::Integrity);
    }

    #[test]
    fn unknown_ayanamsha_slug_classifies_input() {
        let e = pericynthion::error::PericynthionError::UnknownAyanamshaSlug {
            slug: "bogus".to_string(),
            known: "lahiri, fagan-bradley".to_string(),
        };
        assert_eq!(classify(&anyhow_of(e)), ExitClass::Input);
    }

    #[test]
    fn draconic_node_unavailable_classifies_input() {
        let e = pericynthion::error::PericynthionError::DraconicNodeUnavailable;
        assert_eq!(classify(&anyhow_of(e)), ExitClass::Input);
    }

    #[test]
    fn body_resolve_unknown_classifies_input() {
        let e = pericynthion::BodyResolveError::Unknown("nope".to_string());
        assert_eq!(classify(&anyhow_of(e)), ExitClass::Input);
    }

    #[test]
    fn body_resolve_not_minor_body_classifies_input() {
        let e = pericynthion::BodyResolveError::NotMinorBody("Earth");
        assert_eq!(classify(&anyhow_of(e)), ExitClass::Input);
    }

    #[test]
    fn body_resolve_not_covered_classifies_not_found() {
        let e = pericynthion::BodyResolveError::NotCovered("Chiron");
        assert_eq!(classify(&anyhow_of(e)), ExitClass::NotFound);
    }

    // `FetchError::Http`/`HorizonsError::Http` both wrap a `reqwest::Error`.
    // `reqwest::Error` has no public constructor (every builder in its
    // `error` module is `pub(crate)`), so the only way to get one from
    // outside the crate is to provoke it through a public API. `Proxy::https`
    // with an unparseable scheme takes the same `Url::parse(..).map_err(..)`
    // builder-error path a real request would, but never touches a socket or
    // a client/runtime — synchronous, sub-millisecond, no network. `reqwest`
    // is a dev-dependency here purely for this (see Cargo.toml comment).
    fn a_reqwest_http_error() -> reqwest::Error {
        reqwest::Proxy::https("not a valid url").expect_err("scheme is deliberately unparseable")
    }

    #[test]
    fn horizons_http_classifies_network() {
        let e = pericynthion::horizons::HorizonsError::Http(a_reqwest_http_error());
        assert_eq!(classify(&anyhow_of(e)), ExitClass::Network);
    }

    #[test]
    fn horizons_io_classifies_io() {
        let e = pericynthion::horizons::HorizonsError::Io {
            path: std::path::PathBuf::from("/nonexistent/out.bsp"),
            source: std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied"),
        };
        assert_eq!(classify(&anyhow_of(e)), ExitClass::Io);
    }

    #[test]
    fn horizons_no_spk_classifies_not_found() {
        let e = pericynthion::horizons::HorizonsError::NoSpk("No matches found.".to_string());
        assert_eq!(classify(&anyhow_of(e)), ExitClass::NotFound);
    }

    #[test]
    fn horizons_batch_mixed_http_and_io_prefers_io() {
        // Precedence: Io(11) > NotFound(4) > Network(8) > Internal(1) — see
        // `horizons_batch_class`'s doc comment. A mixed {Http, Io} batch must
        // resolve to Io, the most local/actionable cause.
        let http = pericynthion::horizons::HorizonsError::Http(a_reqwest_http_error());
        let io = pericynthion::horizons::HorizonsError::Io {
            path: std::path::PathBuf::from("/nonexistent/out.bsp"),
            source: std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied"),
        };
        let class = horizons_batch_class([&http, &io]);
        assert_eq!(class, ExitClass::Io);
        // Order-independence: same result regardless of which failed first.
        let class_reversed = horizons_batch_class([&io, &http]);
        assert_eq!(class_reversed, ExitClass::Io);
    }

    #[test]
    fn horizons_batch_mixed_notfound_and_network_prefers_notfound() {
        let no_spk = pericynthion::horizons::HorizonsError::NoSpk("no matches".to_string());
        let http = pericynthion::horizons::HorizonsError::Http(a_reqwest_http_error());
        assert_eq!(horizons_batch_class([&no_spk, &http]), ExitClass::NotFound);
    }

    #[test]
    fn horizons_batch_single_class_is_that_class() {
        let http = pericynthion::horizons::HorizonsError::Http(a_reqwest_http_error());
        assert_eq!(horizons_batch_class([&http]), ExitClass::Network);
    }

    #[test]
    fn fetch_error_io_classifies_io() {
        let e = pericynthion::FetchError::Io {
            path: std::path::PathBuf::from("/nonexistent"),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "not found"),
        };
        assert_eq!(classify(&anyhow_of(e)), ExitClass::Io);
    }

    #[test]
    fn fetch_error_verify_classifies_integrity() {
        let e = pericynthion::FetchError::Verify {
            path: std::path::PathBuf::from("/data/header.441"),
            expected: "abc123",
            actual: "def456".to_string(),
        };
        assert_eq!(classify(&anyhow_of(e)), ExitClass::Integrity);
    }

    #[test]
    fn fallback_classifies_internal() {
        let e = anyhow::anyhow!("some unclassified error");
        assert_eq!(classify(&e), ExitClass::Internal);
    }

    #[test]
    fn classify_sees_through_context_wrapping() {
        use anyhow::Context;
        let e = NotFoundError {
            message: "no ephemeris data found".to_string(),
        };
        let wrapped: anyhow::Result<()> = Err(anyhow_of(e)).context("resolving jpl dir");
        let err = wrapped.unwrap_err();
        assert_eq!(classify(&err), ExitClass::NotFound);
    }
}
