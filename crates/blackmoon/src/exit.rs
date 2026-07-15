//! Exit-code taxonomy for `blackmoon`.
//!
//! Deliberately duplicated in `starcat` (see `crates/starcat/src/exit.rs`)
//! rather than factored into a shared crate — each CLI owns its own copy,
//! carrying only the codes it actually emits. blackmoon never verifies a
//! data file's blake3 integrity (that's a `starcat`-only concept), so
//! `ExitClass::Integrity` (5) is not part of this CLI's taxonomy — the gap
//! at 5 is intentional; cross-app number alignment is a free nicety, not an
//! enforced invariant.
//!
//! `classify()` maps a caught `anyhow::Error` to an [`ExitClass`] by
//! downcasting to the concrete error types blackmoon can produce or
//! propagate. `main()` returns `std::process::ExitCode` via
//! `classify(&e).exit_code()`.

/// The exit-code classes `blackmoon` can emit. See the module doc comment
/// for why `Integrity` (5) is absent from this list.
///
/// `Success` is never produced by `classify` (the `Ok` path in `main`
/// returns `ExitCode::SUCCESS` directly). `Input` (3) is produced by
/// [`InputError`] — a supplied `--fill-*` value that is not a recognised
/// slug (distinct from a structural usage error, which exits 2).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ExitClass {
    #[allow(dead_code)]
    Success,
    Internal,
    Usage,
    Input,
    NotFound,
    ChartParse,
    Auth,
    Network,
    LossyRefused,
    NeedInput,
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
            ExitClass::ChartParse => 6,
            ExitClass::Auth => 7,
            ExitClass::Network => 8,
            ExitClass::LossyRefused => 9,
            ExitClass::NeedInput => 10,
            ExitClass::Io => 11,
        }
    }

    pub fn exit_code(self) -> std::process::ExitCode {
        std::process::ExitCode::from(self.code())
    }
}

/// A required conversion value was missing but `--strict` refused a lossy
/// write instead of silently dropping fields.
///
/// Classifies as `ExitClass::LossyRefused` (code 9).
#[derive(Debug)]
pub struct LossyRefusedError {
    /// Human-readable explanation, e.g. naming the sink and affected count.
    pub message: String,
}

impl std::fmt::Display for LossyRefusedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for LossyRefusedError {}

/// A CLI usage error: a required flag/argument combination was not
/// satisfied. Distinct from clap's own parse errors (which exit 2 before
/// `run()` is ever called) — this covers post-parse usage failures, e.g. a
/// required output target that no combination of flags supplied.
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

/// No input charts were available to convert (no input files/paths given,
/// or a given directory contained none).
///
/// Classifies as `ExitClass::NotFound` (code 4).
#[derive(Debug)]
pub struct NoInputError {
    /// Human-readable explanation of what was missing.
    pub message: String,
}

impl std::fmt::Display for NoInputError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for NoInputError {}

/// A supplied `--fill-house`/`--fill-zodiac`/`--fill-locus` value was not a
/// recognised slug for its field. Distinct from a structural clap usage error
/// (exit 2): the flag was well-formed, but its *value* is out of range, so the
/// failure is classified as bad input rather than bad usage.
///
/// Validation happens where the value is consulted (`resolve_fill`), not at
/// clap-parse time, so this only fires when the fill is actually needed. The
/// flag's `possible_values` (completion + `--help` listing) are unaffected.
///
/// Classifies as `ExitClass::Input` (code 3) — see `classify`.
#[derive(Debug)]
pub struct InputError {
    /// Human label for the field, e.g. "house system".
    pub label: String,
    /// The offending value the user supplied, e.g. "xyzzy".
    pub value: String,
    /// The flag it was supplied through, e.g. "--fill-house".
    pub flag: String,
    /// The accepted values for that flag, e.g. `["placidus", "koch", ...]`.
    pub accepted: Vec<String>,
}

impl std::fmt::Display for InputError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "invalid {} '{}' for {} (accepted: {})",
            self.label,
            self.value,
            self.flag,
            self.accepted.join(", "),
        )
    }
}

impl std::error::Error for InputError {}

/// Downcast a caught `anyhow::Error` to the [`ExitClass`] `main` should exit
/// with. Checked in order from most specific to least; the first match wins.
#[must_use]
pub fn classify(err: &anyhow::Error) -> ExitClass {
    if err.downcast_ref::<InputError>().is_some() {
        return ExitClass::Input;
    }
    if err.downcast_ref::<NeedInputError>().is_some() {
        return ExitClass::NeedInput;
    }
    if err.downcast_ref::<LossyRefusedError>().is_some() {
        return ExitClass::LossyRefused;
    }
    if err.downcast_ref::<UsageError>().is_some() {
        return ExitClass::Usage;
    }
    if err.downcast_ref::<NoInputError>().is_some() {
        return ExitClass::NotFound;
    }
    if let Some(e) = err.downcast_ref::<astrogram::provider::ProviderError>() {
        return match e {
            astrogram::provider::ProviderError::Luna(inner) => {
                if inner.is_auth_failure() {
                    ExitClass::Auth
                } else {
                    ExitClass::Network
                }
            }
            astrogram::provider::ProviderError::Astrocom(inner) => {
                if inner.is_auth_failure() {
                    ExitClass::Auth
                } else {
                    ExitClass::Network
                }
            }
            astrogram::provider::ProviderError::Astrotheoros(inner) => {
                if inner.is_auth_failure() {
                    ExitClass::Auth
                } else {
                    ExitClass::Network
                }
            }
            astrogram::provider::ProviderError::Other(_) => ExitClass::Internal,
        };
    }
    // The individual web-session error types are also checked directly, in
    // case a future call site propagates one without going through
    // `ProviderError` (e.g. `resolve_provider`'s own `AstrotheorosSession::authenticate`
    // call, which is `.context()`-wrapped but not `ProviderError`-wrapped).
    if let Some(e) = err.downcast_ref::<astrogram::luna::LunaError>() {
        return if e.is_auth_failure() {
            ExitClass::Auth
        } else {
            ExitClass::Network
        };
    }
    if let Some(e) = err.downcast_ref::<astrogram::astrocom::AstrocomError>() {
        return if e.is_auth_failure() {
            ExitClass::Auth
        } else {
            ExitClass::Network
        };
    }
    if let Some(e) = err.downcast_ref::<astrogram::astrotheoros::AstrotheorosError>() {
        return if e.is_auth_failure() {
            ExitClass::Auth
        } else {
            ExitClass::Network
        };
    }
    if let Some(e) = err.downcast_ref::<astrogram::error::ChartError>() {
        return match e {
            astrogram::error::ChartError::Parse(_)
            | astrogram::error::ChartError::Utf8(_)
            | astrogram::error::ChartError::LongitudeOutOfRange(_)
            | astrogram::error::ChartError::LatitudeOutOfRange(_) => ExitClass::ChartParse,
            astrogram::error::ChartError::Io { .. } => ExitClass::Io,
            astrogram::error::ChartError::UnsupportedDirection(_)
            | astrogram::error::ChartError::MissingGenerator => ExitClass::Usage,
        };
    }
    if err.downcast_ref::<astrogram::aaf::AafError>().is_some() {
        return ExitClass::ChartParse;
    }
    if err.downcast_ref::<astrogram::jhd::JhdError>().is_some() {
        return ExitClass::ChartParse;
    }
    ExitClass::Internal
}

/// A required conversion value (house system / zodiac / locus / …) was
/// missing and stdin is not a TTY, so there was no way to prompt for it
/// interactively.
///
/// Classifies as `ExitClass::NeedInput` (code 10) — see `classify`. Produced
/// by `resolve_fill` when stdin is non-interactive.
#[derive(Debug)]
pub struct NeedInputError {
    /// Human label for the missing value, e.g. "house system".
    pub label: String,
    /// The flag that would have supplied it, e.g. "--fill-house".
    pub flag: String,
    /// The accepted values for that flag, e.g. `["placidus", "koch", ...]`.
    pub accepted: Vec<String>,
    /// The sink format's slug, e.g. "json".
    pub sink_slug: String,
}

impl std::fmt::Display for NeedInputError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} requires a {} but the source provided none; pass {} (accepted: {}) — stdin is not a TTY, so there is no prompt",
            self.sink_slug,
            self.label,
            self.flag,
            self.accepted.join(", "),
        )
    }
}

impl std::error::Error for NeedInputError {}

#[cfg(test)]
mod need_input_tests {
    use super::*;

    #[test]
    fn need_input_error_message_names_flag_and_lists_accepted_values() {
        let e = NeedInputError {
            label: "house system".to_string(),
            flag: "--fill-house".to_string(),
            accepted: vec!["placidus".to_string(), "whole-sign".to_string()],
            sink_slug: "json".to_string(),
        };
        let msg = e.to_string();
        assert!(msg.contains("--fill-house"), "message: {msg}");
        assert!(msg.contains("placidus"), "message: {msg}");
        assert!(msg.contains("whole-sign"), "message: {msg}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_match_canonical_taxonomy() {
        // blackmoon's trimmed subset — no Integrity(5); see the module doc
        // comment for why. The gap at 5 is intentional.
        use ExitClass::*;
        let pairs = [
            (Success, 0u8),
            (Internal, 1),
            (Usage, 2),
            (Input, 3),
            (NotFound, 4),
            (ChartParse, 6),
            (Auth, 7),
            (Network, 8),
            (LossyRefused, 9),
            (NeedInput, 10),
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
    fn need_input_error_classifies_need_input() {
        let e = NeedInputError {
            label: "house system".to_string(),
            flag: "--fill-house".to_string(),
            accepted: vec!["placidus".to_string()],
            sink_slug: "json".to_string(),
        };
        assert_eq!(classify(&anyhow_of(e)), ExitClass::NeedInput);
    }

    #[test]
    fn lossy_refused_error_classifies_lossy_refused() {
        let e = LossyRefusedError {
            message: "--strict: 3 chart(s) would lose data".to_string(),
        };
        assert_eq!(classify(&anyhow_of(e)), ExitClass::LossyRefused);
    }

    #[test]
    fn usage_error_classifies_usage() {
        let e = UsageError {
            message: "--output is required".to_string(),
        };
        assert_eq!(classify(&anyhow_of(e)), ExitClass::Usage);
    }

    #[test]
    fn no_input_error_classifies_not_found() {
        let e = NoInputError {
            message: "at least one input file is required".to_string(),
        };
        assert_eq!(classify(&anyhow_of(e)), ExitClass::NotFound);
    }

    #[test]
    fn input_error_classifies_input() {
        let e = InputError {
            label: "house system".to_string(),
            value: "xyzzy".to_string(),
            flag: "--fill-house".to_string(),
            accepted: vec!["placidus".to_string(), "koch".to_string()],
        };
        assert_eq!(classify(&anyhow_of(e)), ExitClass::Input);
    }

    #[test]
    fn input_error_message_names_value_flag_and_accepted() {
        let e = InputError {
            label: "house system".to_string(),
            value: "xyzzy".to_string(),
            flag: "--fill-house".to_string(),
            accepted: vec!["placidus".to_string(), "koch".to_string()],
        };
        let msg = e.to_string();
        assert!(msg.contains("xyzzy"), "message: {msg}");
        assert!(msg.contains("--fill-house"), "message: {msg}");
        assert!(msg.contains("placidus"), "message: {msg}");
        assert!(msg.contains("koch"), "message: {msg}");
    }

    #[test]
    fn luna_form_tokens_not_found_classifies_auth() {
        let e = astrogram::luna::LunaError::FormTokensNotFound("edit".to_string());
        assert!(e.is_auth_failure());
        assert_eq!(classify(&anyhow_of(e)), ExitClass::Auth);
    }

    #[test]
    fn luna_error_via_provider_error_classifies_auth() {
        let inner = astrogram::luna::LunaError::FormTokensNotFound("edit".to_string());
        let e = astrogram::provider::ProviderError::Luna(inner);
        assert_eq!(classify(&anyhow_of(e)), ExitClass::Auth);
    }

    #[test]
    fn luna_non_auth_error_classifies_network() {
        let e = astrogram::luna::LunaError::MissingUniwheel;
        assert!(!e.is_auth_failure());
        assert_eq!(classify(&anyhow_of(e)), ExitClass::Network);
    }

    #[test]
    fn astrotheoros_clerk_auth_failed_classifies_auth() {
        let e = astrogram::astrotheoros::AstrotheorosError::ClerkAuthFailed("nope".to_string());
        assert!(e.is_auth_failure());
        assert_eq!(classify(&anyhow_of(e)), ExitClass::Auth);
    }

    #[test]
    fn astrotheoros_error_via_provider_error_classifies_auth() {
        let inner =
            astrogram::astrotheoros::AstrotheorosError::ClerkIdentifyFailed("nope".to_string());
        let e = astrogram::provider::ProviderError::Astrotheoros(inner);
        assert_eq!(classify(&anyhow_of(e)), ExitClass::Auth);
    }

    #[test]
    fn astrotheoros_non_auth_error_classifies_network() {
        let e = astrogram::astrotheoros::AstrotheorosError::AtlasResponseInvalid;
        assert!(!e.is_auth_failure());
        assert_eq!(classify(&anyhow_of(e)), ExitClass::Network);
    }

    #[test]
    fn astrocom_login_failed_classifies_auth() {
        let e = astrogram::astrocom::AstrocomError::LoginFailed("bad creds".to_string());
        assert!(e.is_auth_failure());
        assert_eq!(classify(&anyhow_of(e)), ExitClass::Auth);
    }

    #[test]
    fn astrocom_error_via_provider_error_classifies_auth() {
        let inner = astrogram::astrocom::AstrocomError::LoginFailed("bad creds".to_string());
        let e = astrogram::provider::ProviderError::Astrocom(inner);
        assert_eq!(classify(&anyhow_of(e)), ExitClass::Auth);
    }

    #[test]
    fn astrocom_non_auth_error_classifies_network() {
        let e = astrogram::astrocom::AstrocomError::NhorNotFound;
        assert!(!e.is_auth_failure());
        assert_eq!(classify(&anyhow_of(e)), ExitClass::Network);
    }

    #[test]
    fn provider_error_other_classifies_internal() {
        let e = astrogram::provider::ProviderError::Other("no credentials".to_string());
        assert_eq!(classify(&anyhow_of(e)), ExitClass::Internal);
    }

    #[test]
    fn chart_error_parse_classifies_chart_parse() {
        let e = astrogram::error::ChartError::Parse("bad record".to_string());
        assert_eq!(classify(&anyhow_of(e)), ExitClass::ChartParse);
    }

    #[test]
    fn chart_error_longitude_out_of_range_classifies_chart_parse() {
        let e = astrogram::error::ChartError::LongitudeOutOfRange(200.0);
        assert_eq!(classify(&anyhow_of(e)), ExitClass::ChartParse);
    }

    #[test]
    fn chart_error_latitude_out_of_range_classifies_chart_parse() {
        let e = astrogram::error::ChartError::LatitudeOutOfRange(200.0);
        assert_eq!(classify(&anyhow_of(e)), ExitClass::ChartParse);
    }

    #[test]
    fn chart_error_utf8_classifies_chart_parse() {
        // An overlong encoding of NUL: not valid UTF-8, but not a literal
        // `from_utf8` can reject at compile time either — avoids
        // `invalid_from_utf8` firing on a hardcoded bad literal.
        let bad: Vec<u8> = vec![0xC0, 0x80];
        let err = std::str::from_utf8(&bad).unwrap_err();
        let e = astrogram::error::ChartError::Utf8(err);
        assert_eq!(classify(&anyhow_of(e)), ExitClass::ChartParse);
    }

    #[test]
    fn chart_error_io_classifies_io() {
        let e = astrogram::error::ChartError::Io {
            path: std::path::PathBuf::from("/nonexistent"),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "not found"),
        };
        assert_eq!(classify(&anyhow_of(e)), ExitClass::Io);
    }

    #[test]
    fn chart_error_unsupported_direction_classifies_usage() {
        let e = astrogram::error::ChartError::UnsupportedDirection("read-only format");
        assert_eq!(classify(&anyhow_of(e)), ExitClass::Usage);
    }

    #[test]
    fn aaf_error_classifies_chart_parse() {
        let e = astrogram::aaf::AafError::MissingB {
            context: "#A93:...".to_string(),
        };
        assert_eq!(classify(&anyhow_of(e)), ExitClass::ChartParse);
    }

    #[test]
    fn jhd_error_classifies_chart_parse() {
        let e = astrogram::jhd::JhdError::TooFewLines(3);
        assert_eq!(classify(&anyhow_of(e)), ExitClass::ChartParse);
    }

    #[test]
    fn fallback_classifies_internal() {
        let e = anyhow::anyhow!("some unclassified error");
        assert_eq!(classify(&e), ExitClass::Internal);
    }

    #[test]
    fn classify_sees_through_context_wrapping() {
        use anyhow::Context;
        let e = astrogram::luna::LunaError::FormTokensNotFound("edit".to_string());
        let wrapped: anyhow::Result<()> = Err(anyhow_of(e)).context("resolving luna provider");
        let err = wrapped.unwrap_err();
        assert_eq!(classify(&err), ExitClass::Auth);
    }
}
