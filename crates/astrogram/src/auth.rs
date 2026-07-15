//! Web-provider credential assembly and authentication.
//!
//! This module owns the logic a CLI *or* a GUI would otherwise reimplement per
//! web target: fold the available credential sources into the canonical
//! fall-through chain (`cookie → token → login`), validate half-supplied
//! login credentials, enforce a non-empty chain, then run the underlying
//! session `authenticate` and construct the `WebProvider`.
//!
//! # Two phases
//!
//! Assembly (`AuthPlan::assemble`) is pure and network-free: it builds and
//! validates the chain and captures the cookie-disclosure facts. Connection
//! (`AuthPlan::connect`) performs the network probe and yields the
//! authenticated `WebProvider` plus an `AuthReport`. A front-end that needs
//! to narrate its own disclosure (which browser store won, the chosen
//! User-Agent) between the two — as `blackmoon` does on stderr — drives the two
//! phases directly; a GUI that just wants a session calls the one-shot
//! `WebProvider::authenticate` convenience.
//!
//! This module is compiled only with the `cookie-import` feature, since the
//! disclosure carries `Browser` identities from the cookie importer.

use crate::astrocom::{AstrocomCredential, AstrocomSession};
use crate::astrotheoros::{AstrotheorosCredential, AstrotheorosSession};
use crate::cookie_import::{Browser, CredentialOutcome, ProviderCredential};
use crate::format::Format;
use crate::luna::LunaSession;
use crate::provider::WebProvider;
use std::collections::{HashMap, HashSet};

/// Which kind of credential occupies a position in the fall-through chain. Lets
/// a front-end name *which* source authenticated after a fall-through.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SourceKind {
    /// A browser session cookie imported via [`crate::cookie_import`].
    Cookie,
    /// An explicitly supplied token (Clerk triple / astro.com `cid` / LUNA cookie).
    Token,
    /// Account email + password login.
    Login,
}

/// True when the chain's only credential is a browser cookie — a stale cookie
/// then has no token/login to fall back to.
#[must_use]
pub fn only_cookie_source(kinds: &[SourceKind]) -> bool {
    kinds.len() == 1 && kinds[0] == SourceKind::Cookie
}

/// Cookie-import disclosure facts, so a front-end can tell the user which
/// browser/profile a session cookie came from before authenticating. Data
/// only — formatting (labels, oxford joins, expiry lines) is the caller's.
#[derive(Debug, Clone)]
pub struct CookieDisclosure {
    /// The provider's primary registrable domain (e.g. `astrotheoros.com`).
    pub domain: String,
    /// Every store that held a usable session: `(browser, profile, freshness)`.
    pub found_in: Vec<(Browser, String, i64)>,
    /// Browser of the winning (freshest) store.
    pub winner_browser: Browser,
    /// Profile label of the winning store.
    pub winner_profile: String,
}

/// Raw credential inputs a front-end gathers from its own flags / widgets.
///
/// `cookie` is the *already imported* browser credential (see
/// [`crate::cookie_import::import_credential`]); import is a separate,
/// consent-gated step whose divined User-Agent the caller needs before
/// choosing the request UA. `token` is the raw token string for the target
/// (parsed here). `user`/`pass` are the login pair (validated here). The
/// `luna_*` fields carry LUNA read-behaviour forwarded into the provider.
pub struct CredentialInputs {
    /// Imported browser credential for this target, if cookie access was granted
    /// and a usable session was found.
    pub cookie: Option<CredentialOutcome>,
    /// Raw token string (`--astrotheoros-token` triple, astro.com `cid`, LUNA cookie).
    pub token: Option<String>,
    /// Login email, if supplied.
    pub user: Option<String>,
    /// Login password, if supplied.
    pub pass: Option<String>,
    /// LUNA `resume_from` prefix (ignored by other targets).
    pub luna_resume_from: Option<String>,
    /// LUNA normalize-in-place flag (ignored by other targets).
    pub luna_normalize: bool,
}

impl CredentialInputs {
    /// A [`CredentialInputs`] with every field empty — a convenience base a
    /// front-end can spread its populated fields over.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            cookie: None,
            token: None,
            user: None,
            pass: None,
            luna_resume_from: None,
            luna_normalize: false,
        }
    }
}

/// Errors from credential assembly or connection.
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    /// A login password was supplied without an accompanying email.
    #[error("a login password requires an accompanying email")]
    MissingUser,
    /// A login email was supplied without an accompanying password.
    #[error("a login email requires an accompanying password")]
    MissingPass,
    /// No credential source was supplied at all.
    #[error("no credentials supplied for this target")]
    NoCredentials,
    /// The `--astrotheoros-token` value was not a valid `jwt:session_id:client_uat` triple.
    #[error(transparent)]
    BadAstrotheorosToken(#[from] crate::astrotheoros::TokenTripleError),
    /// The target is not a web provider (only Luna / Astrocom / Astrotheoros are).
    #[error("authentication is only supported for web providers (Luna, Astrocom, Astrotheoros)")]
    NotWebProvider,
    /// Every credential in the chain was rejected (or a non-auth error stopped
    /// the chain). `site` names the provider for the message.
    #[error("{site} authentication failed for every source")]
    AllFailed {
        /// Human-readable provider name (matches [`WebProvider::site_display`]).
        site: &'static str,
        /// The underlying session error.
        #[source]
        source: SessionError,
    },
}

/// The underlying session error behind [`AuthError::AllFailed`].
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    /// astrotheoros.com session error.
    #[error(transparent)]
    Astrotheoros(#[from] crate::astrotheoros::AstrotheorosError),
    /// astro.com session error.
    #[error(transparent)]
    Astrocom(#[from] crate::astrocom::AstrocomError),
    /// LUNA session error.
    #[error(transparent)]
    Luna(#[from] crate::luna::LunaError),
}

/// The per-target assembled credential chain (private detail of [`AuthPlan`]).
enum Chain {
    Astrotheoros(Vec<AstrotheorosCredential>),
    Astrocom(Vec<AstrocomCredential>),
    Luna(Vec<String>),
}

/// A validated, network-free credential chain ready to authenticate.
///
/// Built by `AuthPlan::assemble`; consumed by `AuthPlan::connect`. Between
/// the two a caller may inspect [`kinds`](Self::kinds),
/// [`only_cookie`](Self::only_cookie), [`disclosure`](Self::disclosure), and
/// [`cookie_ua`](Self::cookie_ua) to narrate before the network round-trip.
pub struct AuthPlan {
    kinds: Vec<SourceKind>,
    disclosure: Option<CookieDisclosure>,
    cookie_ua: Option<String>,
    chain: Chain,
    luna_resume_from: Option<String>,
    luna_normalize: bool,
}

impl AuthPlan {
    /// The kinds of credential in the chain, in fall-through order.
    #[must_use]
    pub fn kinds(&self) -> &[SourceKind] {
        &self.kinds
    }

    /// Whether the chain's only source is a browser cookie (no fallback).
    #[must_use]
    pub fn only_cookie(&self) -> bool {
        only_cookie_source(&self.kinds)
    }

    /// The cookie-import disclosure, when a browser cookie is in the chain.
    #[must_use]
    pub fn disclosure(&self) -> Option<&CookieDisclosure> {
        self.disclosure.as_ref()
    }

    /// The divined User-Agent of the winning browser cookie, when present.
    #[must_use]
    pub fn cookie_ua(&self) -> Option<&str> {
        self.cookie_ua.as_deref()
    }

    /// Assemble and validate the credential chain for `target` from `inputs`.
    ///
    /// Folds the available sources into the canonical `cookie → token → login`
    /// order, parsing the astrotheoros token triple, validating that a login
    /// email and password are supplied together, and enforcing a non-empty
    /// chain. No network I/O.
    ///
    /// # Errors
    /// - [`AuthError::MissingUser`] / [`AuthError::MissingPass`] for a
    ///   half-supplied login.
    /// - [`AuthError::BadAstrotheorosToken`] if the token is not a valid triple.
    /// - [`AuthError::NoCredentials`] if no source is supplied.
    /// - [`AuthError::NotWebProvider`] if `target` is not a web provider.
    // The three arms are structurally parallel but bind distinct per-provider
    // credential enums, so they resist extraction without heavier generics.
    #[allow(clippy::too_many_lines)]
    pub fn assemble(target: Format, inputs: CredentialInputs) -> Result<Self, AuthError> {
        let CredentialInputs {
            cookie,
            token,
            user,
            pass,
            luna_resume_from,
            luna_normalize,
        } = inputs;

        let mut kinds: Vec<SourceKind> = Vec::new();
        let mut disclosure: Option<CookieDisclosure> = None;
        let mut cookie_ua: Option<String> = None;

        match target {
            Format::Astrotheoros => {
                let mut chain: Vec<AstrotheorosCredential> = Vec::new();
                if let Some(out) = cookie
                    && let ProviderCredential::Astrotheoros(c) = out.credential
                {
                    disclosure = Some(disclosure_of(
                        out.domain,
                        out.found_in,
                        out.browser,
                        out.profile,
                    ));
                    cookie_ua = out.cookie_ua;
                    kinds.push(SourceKind::Cookie);
                    chain.push(c);
                }
                if let Some(token) = token {
                    let cred = AstrotheorosCredential::parse_token_triple(&token)?;
                    kinds.push(SourceKind::Token);
                    chain.push(cred);
                }
                push_login(&mut kinds, user, pass, |email, password| {
                    chain.push(AstrotheorosCredential::Login { email, password });
                })?;
                if chain.is_empty() {
                    return Err(AuthError::NoCredentials);
                }
                Ok(Self {
                    kinds,
                    disclosure,
                    cookie_ua,
                    chain: Chain::Astrotheoros(chain),
                    luna_resume_from: None,
                    luna_normalize: false,
                })
            }
            Format::Astrocom => {
                let mut chain: Vec<AstrocomCredential> = Vec::new();
                if let Some(out) = cookie
                    && let ProviderCredential::Astrocom(c) = out.credential
                {
                    disclosure = Some(disclosure_of(
                        out.domain,
                        out.found_in,
                        out.browser,
                        out.profile,
                    ));
                    cookie_ua = out.cookie_ua;
                    kinds.push(SourceKind::Cookie);
                    chain.push(c);
                }
                if let Some(cid) = token {
                    kinds.push(SourceKind::Token);
                    chain.push(AstrocomCredential::Cookie(cid));
                }
                push_login(&mut kinds, user, pass, |email, password| {
                    chain.push(AstrocomCredential::Login { email, password });
                })?;
                if chain.is_empty() {
                    return Err(AuthError::NoCredentials);
                }
                Ok(Self {
                    kinds,
                    disclosure,
                    cookie_ua,
                    chain: Chain::Astrocom(chain),
                    luna_resume_from: None,
                    luna_normalize: false,
                })
            }
            Format::Luna => {
                let mut cookies: Vec<String> = Vec::new();
                if let Some(out) = cookie
                    && let ProviderCredential::Luna(tok) = out.credential
                {
                    disclosure = Some(disclosure_of(
                        out.domain,
                        out.found_in,
                        out.browser,
                        out.profile,
                    ));
                    cookie_ua = out.cookie_ua;
                    kinds.push(SourceKind::Cookie);
                    cookies.push(tok);
                }
                if let Some(token) = token {
                    kinds.push(SourceKind::Token);
                    cookies.push(token);
                }
                // LUNA has no login flow; user/pass are inapplicable and ignored.
                if cookies.is_empty() {
                    return Err(AuthError::NoCredentials);
                }
                Ok(Self {
                    kinds,
                    disclosure,
                    cookie_ua,
                    chain: Chain::Luna(cookies),
                    luna_resume_from,
                    luna_normalize,
                })
            }
            _ => Err(AuthError::NotWebProvider),
        }
    }

    /// Run the network probe over the assembled chain and build the provider.
    ///
    /// `user_agent` is the request User-Agent (required). Returns the
    /// authenticated `WebProvider` and an `AuthReport` naming which source
    /// authenticated.
    ///
    /// # Errors
    /// [`AuthError::AllFailed`] if every credential is rejected, or the first
    /// non-auth (network/parse) error stopping the chain.
    pub fn connect(
        self,
        delay_ms: u64,
        user_agent: &str,
    ) -> Result<(WebProvider, AuthReport), AuthError> {
        let Self {
            kinds,
            disclosure,
            cookie_ua,
            chain,
            luna_resume_from,
            luna_normalize,
        } = self;

        let (provider, used) =
            match chain {
                Chain::Astrotheoros(chain) => {
                    let (session, used) = AstrotheorosSession::authenticate(
                        &chain, delay_ms, user_agent,
                    )
                    .map_err(|e| AuthError::AllFailed {
                        site: "astrotheoros.com",
                        source: e.into(),
                    })?;
                    (
                        WebProvider::Astrotheoros {
                            session,
                            uuid_map: HashMap::new(),
                        },
                        used,
                    )
                }
                Chain::Astrocom(chain) => {
                    let auth = AstrocomSession::authenticate(&chain, delay_ms, user_agent)
                        .map_err(|e| AuthError::AllFailed {
                            site: "astro.com",
                            source: e.into(),
                        })?;
                    (
                        WebProvider::Astrocom {
                            session: auth.session,
                            creds: auth.login,
                            nhor_id_map: HashMap::new(),
                        },
                        auth.source,
                    )
                }
                Chain::Luna(cookies) => {
                    let refs: Vec<&str> = cookies.iter().map(String::as_str).collect();
                    let (session, used) = LunaSession::authenticate(&refs, delay_ms, user_agent)
                        .map_err(|e| AuthError::AllFailed {
                            site: "LUNA",
                            source: e.into(),
                        })?;
                    (
                        WebProvider::Luna {
                            session,
                            resume_from: luna_resume_from,
                            normalize: luna_normalize,
                            listing_keys: HashSet::new(),
                            phenom_ids: Vec::new(),
                        },
                        used,
                    )
                }
            };

        let report = AuthReport {
            kinds,
            used,
            disclosure,
            cookie_ua,
        };
        Ok((provider, report))
    }
}

/// The structured outcome of authentication: which sources were tried, which
/// one authenticated, and the cookie-disclosure facts.
pub struct AuthReport {
    /// The kinds of credential in the chain, in fall-through order.
    pub kinds: Vec<SourceKind>,
    /// Index in `kinds` of the source that authenticated.
    pub used: usize,
    /// Cookie-import disclosure, when a browser cookie was in the chain.
    pub disclosure: Option<CookieDisclosure>,
    /// Divined User-Agent of the winning browser cookie, when present.
    pub cookie_ua: Option<String>,
}

impl WebProvider {
    /// One-shot authentication: assemble the chain from `inputs`, then connect.
    ///
    /// Equivalent to `AuthPlan::assemble` followed by `AuthPlan::connect`.
    /// A front-end that needs to narrate between assembly and the network probe
    /// (e.g. print which cookie store won, or the chosen User-Agent) should
    /// drive the two phases directly instead.
    ///
    /// # Errors
    /// Any [`AuthError`] from assembly or connection.
    pub fn authenticate(
        target: Format,
        inputs: CredentialInputs,
        delay_ms: u64,
        user_agent: &str,
    ) -> Result<(WebProvider, AuthReport), AuthError> {
        AuthPlan::assemble(target, inputs)?.connect(delay_ms, user_agent)
    }
}

fn disclosure_of(
    domain: String,
    found_in: Vec<(Browser, String, i64)>,
    winner_browser: Browser,
    winner_profile: String,
) -> CookieDisclosure {
    CookieDisclosure {
        domain,
        found_in,
        winner_browser,
        winner_profile,
    }
}

/// Validate and push a login credential. `push` appends the target-specific
/// `Login` variant when both parts are present.
fn push_login(
    kinds: &mut Vec<SourceKind>,
    user: Option<String>,
    pass: Option<String>,
    push: impl FnOnce(String, String),
) -> Result<(), AuthError> {
    match (user, pass) {
        (Some(email), Some(password)) => {
            kinds.push(SourceKind::Login);
            push(email, password);
            Ok(())
        }
        (Some(_), None) => Err(AuthError::MissingPass),
        (None, Some(_)) => Err(AuthError::MissingUser),
        (None, None) => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token_triple() -> String {
        // header.payload.sig where payload decodes to {"sid":"sess_T","exp":9_999_999_999}.
        "eyJhbGciOiJSUzI1NiJ9.eyJzaWQiOiAic2Vzc19UIiwgImV4cCI6IDk5OTk5OTk5OTl9.sig:sess_T:1700000000"
            .to_string()
    }

    // AuthPlan is deliberately not Debug (its chain holds secrets), so negative
    // cases match on the Result rather than calling unwrap_err.
    #[test]
    fn assemble_empty_inputs_is_no_credentials() {
        assert!(matches!(
            AuthPlan::assemble(Format::Astrotheoros, CredentialInputs::empty()),
            Err(AuthError::NoCredentials)
        ));
    }

    #[test]
    fn assemble_user_without_pass_errors() {
        let inputs = CredentialInputs {
            user: Some("a@b.com".into()),
            ..CredentialInputs::empty()
        };
        assert!(matches!(
            AuthPlan::assemble(Format::Astrocom, inputs),
            Err(AuthError::MissingPass)
        ));
    }

    #[test]
    fn assemble_pass_without_user_errors() {
        let inputs = CredentialInputs {
            pass: Some("pw".into()),
            ..CredentialInputs::empty()
        };
        assert!(matches!(
            AuthPlan::assemble(Format::Astrotheoros, inputs),
            Err(AuthError::MissingUser)
        ));
    }

    #[test]
    fn assemble_orders_token_then_login_for_astrotheoros() {
        let inputs = CredentialInputs {
            token: Some(token_triple()),
            user: Some("a@b.com".into()),
            pass: Some("pw".into()),
            ..CredentialInputs::empty()
        };
        let plan = AuthPlan::assemble(Format::Astrotheoros, inputs).expect("assembles");
        assert_eq!(plan.kinds(), &[SourceKind::Token, SourceKind::Login]);
        assert!(!plan.only_cookie());
        assert!(plan.disclosure().is_none());
    }

    #[test]
    fn assemble_astrotheoros_bad_token_errors() {
        let inputs = CredentialInputs {
            token: Some("not-a-triple".into()),
            ..CredentialInputs::empty()
        };
        assert!(matches!(
            AuthPlan::assemble(Format::Astrotheoros, inputs),
            Err(AuthError::BadAstrotheorosToken(_))
        ));
    }

    #[test]
    fn assemble_luna_ignores_login_and_takes_token() {
        let inputs = CredentialInputs {
            token: Some("luna-cookie".into()),
            // user/pass are inapplicable to LUNA and must not error or count.
            user: Some("a@b.com".into()),
            pass: Some("pw".into()),
            ..CredentialInputs::empty()
        };
        let plan = AuthPlan::assemble(Format::Luna, inputs).expect("assembles");
        assert_eq!(plan.kinds(), &[SourceKind::Token]);
    }

    #[test]
    fn assemble_non_web_target_errors() {
        assert!(matches!(
            AuthPlan::assemble(Format::Sfcht, CredentialInputs::empty()),
            Err(AuthError::NotWebProvider)
        ));
    }

    #[test]
    fn only_cookie_source_predicate() {
        assert!(only_cookie_source(&[SourceKind::Cookie]));
        assert!(!only_cookie_source(&[
            SourceKind::Cookie,
            SourceKind::Token
        ]));
        assert!(!only_cookie_source(&[SourceKind::Token]));
        assert!(!only_cookie_source(&[]));
    }
}
