//! Cookie-import facade — provider→domain mapping and cookie→session glue.
//!
//! This module is a **closed facade** over the `wristband` crate: it re-exports
//! only the types a GUI or CLI needs, so downstream crates depend solely on
//! `astrogram` and never need to name `wristband` directly.
//!
//! # Feature gate
//!
//! This module is compiled only when the `cookie-import` Cargo feature is
//! enabled:
//!
//! ```toml
//! astrogram = { version = "…", features = ["cookie-import"] }
//! ```
//!
//! Enabling the feature adds `wristband` as a dependency.  The base `astrogram`
//! build (no features) does **not** compile `wristband`.
//!
//! # Consent
//!
//! This module performs no prompting (INV-4 / INV-7).  The caller — a CLI
//! flag such as `--grant-cookie-access`, or a GUI checkbox — is responsible
//! for obtaining the user's consent before calling `import_credential`.
//!
//! # Astrotheoros (Clerk) note
//!
//! Astrotheoros uses Clerk for authentication. The browser's `__session`
//! cookie **is** the active Clerk session JWT, and the Clerk `session_id` is
//! the `sid` claim inside it; the `__client_uat` cookie supplies the
//! client-auth timestamp. That trio is exactly what
//! [`crate::astrotheoros::AstrotheorosSession::from_jwt`] needs, so cookie
//! import yields a credential that builds a self-refreshing session (no
//! login).  Clerk may suffix these cookies with an instance hash
//! (`__session_<hash>`); the importer accepts the exact name or a suffixed
//! variant.

use crate::astrocom::AstrocomCredential;
use crate::astrotheoros::AstrotheorosCredential;
use crate::error::ChartError;
use crate::format::Format;
use wristband::{Container, Cookie, Domain, ReadOptions, WristbandError};

/// Re-exported from `wristband` so callers depend only on `astrogram`.
///
/// Pass a specific variant to [`import_credential`] to restrict the
/// read to one browser, or `None` to enumerate all installed stores.
pub use wristband::Browser;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Raw credential material for a provider, extracted from a browser store.
///
/// This carries the cookie material so a caller can fold it into a credential
/// chain alongside token/login sources and let `authenticate` build + probe
/// each in order.
#[derive(Debug)]
pub enum ProviderCredential {
    /// LUNA session-cookie value.
    Luna(String),
    /// astro.com `cid` cookie (read-capable).
    Astrocom(AstrocomCredential),
    /// Astrotheoros Clerk cookie material.
    Astrotheoros(AstrotheorosCredential),
}

/// The outcome of a credential import: the freshest store's credential plus
/// the (browser, profile, domain) disclosure metadata for the winning store.
pub struct CredentialOutcome {
    /// The credential built from the winning (freshest) store.
    pub credential: ProviderCredential,
    /// The browser the winning store belongs to.
    pub browser: Browser,
    /// The winning store's profile label.
    pub profile: String,
    /// The provider's primary registrable domain.
    pub domain: String,
    /// Every store that held a usable session: `(browser, profile, freshness)`.
    pub found_in: Vec<(Browser, String, i64)>,
    /// Divined User-Agent of the browser the winning cookie came from.
    pub cookie_ua: Option<String>,
}

/// Read the freshest provider credential from the browser store(s).
///
/// Reads every `(browser, profile)` store, builds a [`ProviderCredential`]
/// from each, and returns the freshest — raw credential material the caller
/// places first in a fall-through chain (cookie → token → login).
///
/// # Errors
/// - [`ChartError::UnsupportedDirection`] — `format` is not a web provider.
/// - [`ChartError::Parse`] — no usable credential in any store, or a
///   `wristband` read error.
pub fn import_credential(
    format: Format,
    browser: Option<Browser>,
    profile: Option<&str>,
) -> Result<CredentialOutcome, ChartError> {
    let domains = provider_domains(format)?;
    let domain = primary_domain(&domains);
    let opts = ReadOptions {
        profile: profile.map(str::to_owned),
        container: Container::None,
    };
    let candidates =
        wristband::read_all_sessions(browser, &domains, &opts).map_err(|e| wristband_err(&e))?;

    let mut found_in: Vec<(Browser, String, i64)> = Vec::new();
    let mut best: Option<(i64, Browser, String, ProviderCredential)> = None;
    let mut last_err: Option<ChartError> = None;
    for cand in candidates {
        if cand.cookies.is_empty() {
            continue;
        }
        let freshness = session_freshness(format, &cand.cookies);
        match credential_from_cookies(format, &cand.cookies) {
            Ok(credential) => {
                found_in.push((cand.browser, cand.profile.clone(), freshness));
                let is_fresher = match &best {
                    None => true,
                    Some((best_freshness, _, _, _)) => freshness > *best_freshness,
                };
                if is_fresher {
                    best = Some((freshness, cand.browser, cand.profile, credential));
                }
            }
            Err(e) => last_err = Some(e),
        }
    }

    match best {
        Some((_, browser, winning_profile, credential)) => {
            // Divine the UA from the WINNING store's profile (the freshest cookie's
            // profile), not the request-level `--cookies-profile` filter (often None).
            let cookie_ua = Some(wristband::user_agent::divine(
                browser,
                Some(winning_profile.as_str()),
            ));
            Ok(CredentialOutcome {
                credential,
                browser,
                profile: winning_profile,
                domain,
                found_in,
                cookie_ua,
            })
        }
        None => Err(last_err.unwrap_or_else(|| {
            ChartError::Parse(
                "no usable credential found in any installed browser/profile".to_owned(),
            )
        })),
    }
}

/// The provider's primary registrable domain — the allowed host with the
/// fewest labels (e.g. `astrotheoros.com` over `clerk.astrotheoros.com`).
fn primary_domain(domains: &[Domain]) -> String {
    domains
        .iter()
        .map(Domain::host)
        .min_by_key(|h| h.matches('.').count())
        .unwrap_or("")
        .to_owned()
}

/// A comparable freshness score for a candidate's cookies (higher = fresher).
/// For Clerk (Astrotheoros) this is the latest `__session` JWT `exp`; other
/// providers expose no recency signal, so any present session scores `0`.
fn session_freshness(format: Format, cookies: &[Cookie]) -> i64 {
    match format {
        Format::Astrotheoros => cookies
            .iter()
            .filter(|c| c.name == "__session" || c.name.starts_with("__session_"))
            .filter_map(|c| crate::astrotheoros::jwt_exp(&c.value))
            .max()
            .unwrap_or(i64::MIN),
        _ => 0,
    }
}

// ---------------------------------------------------------------------------
// Internal building blocks
// ---------------------------------------------------------------------------

/// Build the `wristband` allow-list for `format`.
///
/// # Errors
///
/// Returns [`ChartError::UnsupportedDirection`] when `format` is not a web
/// provider (i.e. not `Luna`, `Astrocom`, or `Astrotheoros`).
fn provider_domains(format: Format) -> Result<Vec<Domain>, ChartError> {
    match format {
        Format::Luna => Ok(vec![
            Domain::explicit("lunaastrology.com")
                .expect("lunaastrology.com is a valid registrable domain"),
        ]),
        Format::Astrocom => Ok(vec![
            Domain::explicit("astro.com").expect("astro.com is a valid registrable domain"),
        ]),
        Format::Astrotheoros => Ok(vec![
            Domain::explicit("astrotheoros.com")
                .expect("astrotheoros.com is a valid registrable domain"),
            Domain::explicit("clerk.astrotheoros.com")
                .expect("clerk.astrotheoros.com is a valid registrable domain"),
        ]),
        _ => Err(ChartError::UnsupportedDirection(
            "cookie import is only supported for web providers \
             (Luna, Astrocom, Astrotheoros)",
        )),
    }
}

/// The cookie names this facade reads for each provider.
///
/// Only these names are ever extracted from the browser store; all other
/// cookies within the allowed domains are silently ignored.
fn provider_cookie_names(format: Format) -> &'static [&'static str] {
    match format {
        Format::Luna => &["LUNA_ASTROLOGY_APP"],
        Format::Astrocom => &["cid"],
        Format::Astrotheoros => &["__session", "__client_uat"],
        _ => &[],
    }
}

/// Build a [`ProviderCredential`] from a store's cookies (no session built).
///
/// # Errors
/// - [`ChartError::Parse`] — a required cookie name is missing.
/// - [`ChartError::UnsupportedDirection`] — `format` is not a web provider.
fn credential_from_cookies(
    format: Format,
    cookies: &[Cookie],
) -> Result<ProviderCredential, ChartError> {
    let names = provider_cookie_names(format);
    let find = |name: &str| -> Option<&str> {
        if !names.contains(&name) {
            return None;
        }
        cookies
            .iter()
            .find(|c| c.name == name)
            .map(|c| c.value.as_str())
    };
    // Pick a cookie by exact name, else by a Clerk instance-suffixed variant
    // (`{name}_<hash>`, e.g. `__client_OLW8fVLx`). Named provider cookies
    // (those in `names`) are excluded from the prefix match so that
    // `pick("__client")` does not accidentally match `__client_uat`.
    let pick = |name: &str| -> Option<&str> {
        cookies
            .iter()
            .find(|c| c.name == name)
            .or_else(|| {
                let prefix = format!("{name}_");
                cookies
                    .iter()
                    .find(|c| c.name.starts_with(&prefix) && !names.contains(&c.name.as_str()))
            })
            .map(|c| c.value.as_str())
    };

    match format {
        Format::Luna => {
            let value = find("LUNA_ASTROLOGY_APP").ok_or_else(|| {
                ChartError::Parse(
                    "required cookie 'LUNA_ASTROLOGY_APP' not found in browser store \
                     for lunaastrology.com"
                        .to_owned(),
                )
            })?;
            Ok(ProviderCredential::Luna(value.to_owned()))
        }
        Format::Astrocom => {
            let cid = find("cid").ok_or_else(|| {
                ChartError::Parse(
                    "required cookie 'cid' not found in browser store for astro.com".to_owned(),
                )
            })?;
            Ok(ProviderCredential::Astrocom(AstrocomCredential::Cookie(
                cid.to_owned(),
            )))
        }
        Format::Astrotheoros => {
            let jwt = cookies
                .iter()
                .filter(|c| c.name == "__session" || c.name.starts_with("__session_"))
                .max_by_key(|c| crate::astrotheoros::jwt_exp(&c.value).unwrap_or(i64::MIN))
                .map(|c| c.value.as_str())
                .ok_or_else(|| {
                    ChartError::Parse(
                        "required cookie '__session' not found for astrotheoros.com \
                         (are you logged in to astrotheoros.com in this browser?)"
                            .to_owned(),
                    )
                })?;
            let client_uat = pick("__client_uat").ok_or_else(|| {
                ChartError::Parse(
                    "required cookie '__client_uat' not found for astrotheoros.com".to_owned(),
                )
            })?;
            let session_id = crate::astrotheoros::jwt_sid(jwt).ok_or_else(|| {
                ChartError::Parse(
                    "could not read the Clerk session id ('sid' claim) from the \
                     '__session' JWT"
                        .to_owned(),
                )
            })?;
            let client_cookie = pick("__client").map(str::to_owned);
            Ok(ProviderCredential::Astrotheoros(
                AstrotheorosCredential::Cookie {
                    jwt: jwt.to_owned(),
                    session_id,
                    client_uat: client_uat.to_owned(),
                    client_cookie,
                },
            ))
        }
        _ => Err(ChartError::UnsupportedDirection(
            "cookie import is only supported for web providers \
             (Luna, Astrocom, Astrotheoros)",
        )),
    }
}

/// Convert a [`WristbandError`] to [`ChartError`].
fn wristband_err(e: &WristbandError) -> ChartError {
    ChartError::Parse(e.to_string())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::astrocom::AstrocomCredential;
    use crate::astrotheoros::AstrotheorosCredential;
    use wristband::Cookie;

    // -----------------------------------------------------------------------
    // provider_domains
    // -----------------------------------------------------------------------

    #[test]
    fn provider_domains_astrocom_returns_one_domain() {
        let domains = provider_domains(Format::Astrocom).expect("Astrocom is a web format");
        assert_eq!(domains.len(), 1);
        assert_eq!(domains[0].host(), "astro.com");
    }

    #[test]
    fn provider_domains_luna_returns_one_domain() {
        let domains = provider_domains(Format::Luna).expect("Luna is a web format");
        assert_eq!(domains.len(), 1);
        assert_eq!(domains[0].host(), "lunaastrology.com");
    }

    #[test]
    fn provider_domains_astrotheoros_returns_two_domains() {
        let domains = provider_domains(Format::Astrotheoros).expect("Astrotheoros is a web format");
        assert_eq!(domains.len(), 2);
        let hosts: Vec<&str> = domains.iter().map(Domain::host).collect();
        assert!(hosts.contains(&"astrotheoros.com"));
        assert!(hosts.contains(&"clerk.astrotheoros.com"));
    }

    #[test]
    fn provider_domains_sfcht_errors() {
        let err = provider_domains(Format::Sfcht).unwrap_err();
        assert!(
            matches!(err, ChartError::UnsupportedDirection(_)),
            "expected UnsupportedDirection, got {err:?}"
        );
    }

    // -----------------------------------------------------------------------
    // credential_from_cookies — offline cookie→credential mapping
    // -----------------------------------------------------------------------

    /// A fake `__session` JWT whose payload decodes to `{"sid":"sess_TEST123",…}`.
    /// (`eyJhbGciOiJSUzI1NiJ9` = `{"alg":"RS256"}`; middle segment is the
    /// base64url of the payload; signature is a placeholder — the importer does
    /// not verify it.)
    const FAKE_SESSION_JWT: &str =
        "eyJhbGciOiJSUzI1NiJ9.eyJzaWQiOiAic2Vzc19URVNUMTIzIiwgImV4cCI6IDk5OTk5OTk5OTl9.sig";

    #[test]
    fn credential_from_cookies_astrocom_yields_cookie_cid() {
        let cookies = vec![Cookie::for_test("astro.com", "cid", "cid-xyz")];
        match credential_from_cookies(Format::Astrocom, &cookies) {
            Ok(ProviderCredential::Astrocom(AstrocomCredential::Cookie(cid))) => {
                assert_eq!(cid, "cid-xyz");
            }
            other => panic!("expected Astrocom Cookie credential, got {other:?}"),
        }
    }

    #[test]
    fn credential_from_cookies_astrocom_missing_cid_errors() {
        let cookies = vec![Cookie::for_test("astro.com", "other-cookie", "value")];
        match credential_from_cookies(Format::Astrocom, &cookies) {
            Err(e) => {
                let msg = e.to_string();
                assert!(
                    msg.contains("cid"),
                    "error should name the missing cookie, got: {msg}"
                );
            }
            Ok(_) => panic!("expected Err for missing cid"),
        }
    }

    #[test]
    fn credential_from_cookies_luna_yields_cookie_string() {
        let cookies = vec![Cookie::for_test(
            "lunaastrology.com",
            "LUNA_ASTROLOGY_APP",
            "luna-tok",
        )];
        match credential_from_cookies(Format::Luna, &cookies) {
            Ok(ProviderCredential::Luna(tok)) => assert_eq!(tok, "luna-tok"),
            other => panic!("expected Luna credential, got {other:?}"),
        }
    }

    #[test]
    fn credential_from_cookies_luna_missing_cookie_errors() {
        let cookies: Vec<Cookie> = vec![];
        match credential_from_cookies(Format::Luna, &cookies) {
            Err(e) => {
                let msg = e.to_string();
                assert!(
                    msg.contains("LUNA_ASTROLOGY_APP"),
                    "error should name the missing cookie, got: {msg}"
                );
            }
            Ok(_) => panic!("expected Err for missing LUNA_ASTROLOGY_APP"),
        }
    }

    #[test]
    fn credential_from_cookies_astrotheoros_yields_cookie_with_sid() {
        let cookies = vec![
            Cookie::for_test("astrotheoros.com", "__session", FAKE_SESSION_JWT),
            Cookie::for_test("astrotheoros.com", "__client_uat", "1234567890"),
        ];
        match credential_from_cookies(Format::Astrotheoros, &cookies) {
            Ok(ProviderCredential::Astrotheoros(AstrotheorosCredential::Cookie {
                session_id,
                client_cookie,
                ..
            })) => {
                assert_eq!(session_id, "sess_TEST123");
                assert!(client_cookie.is_none(), "no __client cookie supplied");
            }
            other => panic!("expected Astrotheoros Cookie credential, got {other:?}"),
        }
    }

    #[test]
    fn credential_from_cookies_astrotheoros_accepts_clerk_suffixed_cookies() {
        // Clerk often suffixes the cookies with an instance hash; the importer
        // accepts a `__session_<hash>` / `__client_uat_<hash>` variant.
        let cookies = vec![
            Cookie::for_test("astrotheoros.com", "__session_OLW8fVLx", FAKE_SESSION_JWT),
            Cookie::for_test("astrotheoros.com", "__client_uat_OLW8fVLx", "1234567890"),
        ];
        assert!(matches!(
            credential_from_cookies(Format::Astrotheoros, &cookies),
            Ok(ProviderCredential::Astrotheoros(
                AstrotheorosCredential::Cookie { .. }
            ))
        ));
    }

    #[test]
    fn credential_from_cookies_astrotheoros_missing_session_errors() {
        // No `__session` cookie → clear error, not a broken credential.
        let cookies = vec![Cookie::for_test("astrotheoros.com", "__client_uat", "123")];
        match credential_from_cookies(Format::Astrotheoros, &cookies) {
            Err(e) => assert!(
                e.to_string().contains("__session"),
                "error should name the missing __session cookie, got: {e}"
            ),
            Ok(_) => panic!("expected Err when __session is absent"),
        }
    }

    #[test]
    fn credential_from_cookies_sfcht_errors() {
        let cookies: Vec<Cookie> = vec![];
        match credential_from_cookies(Format::Sfcht, &cookies) {
            Err(ChartError::UnsupportedDirection(_)) => {} // expected
            Err(e) => panic!("expected UnsupportedDirection, got {e:?}"),
            Ok(_) => panic!("expected Err for non-web format"),
        }
    }

    // -----------------------------------------------------------------------
    // cookie_ua field — structural + divine-shape
    // -----------------------------------------------------------------------

    /// Compile-time proof that `CredentialOutcome` exposes `cookie_ua`.
    /// (Fails to compile if the field is renamed or removed.)
    #[allow(dead_code)]
    fn _has_cookie_ua(o: &CredentialOutcome) -> Option<&String> {
        o.cookie_ua.as_ref()
    }

    #[test]
    fn outcome_carries_cookie_ua_field() {
        // Structural: a CredentialOutcome exposes cookie_ua and a divined UA is
        // a Mozilla/5.0 string. (Field-presence + shape; import_credential itself
        // is environment-gated and covered by existing wristband-backed tests.)
        let ua = wristband::user_agent::divine(wristband::Browser::Chrome, None);
        assert!(ua.starts_with("Mozilla/5.0"));
    }

    // -----------------------------------------------------------------------
    // import_credential — real-browser path (requires live store)
    // -----------------------------------------------------------------------

    #[test]
    #[ignore = "requires a live browser cookie store with an active astro.com session"]
    fn import_credential_astrocom_live() {
        // Skip cleanly when no active astro.com session is present anywhere.
        let Ok(outcome) = import_credential(Format::Astrocom, None, None) else {
            eprintln!("no active astro.com session in any browser — skipping live test");
            return;
        };
        assert!(matches!(
            outcome.credential,
            ProviderCredential::Astrocom(AstrocomCredential::Cookie(_))
        ));
        assert_eq!(outcome.domain, "astro.com");
    }

    // -----------------------------------------------------------------------
    // Regression: __client_uat must never be mistaken for __client
    // -----------------------------------------------------------------------

    /// Regression test: when a store has `__client_uat` but no `__client`,
    /// `credential_from_cookies` must not treat `__client_uat` as a `__client`
    /// value.
    ///
    /// Before the fix, `pick("__client")` matched `__client_uat` via the
    /// `__client_` prefix fallback, setting `client_cookie = Some(<uat>)` and
    /// (downstream) routing the session build to `from_browser` with a
    /// malformed `__client`, causing a 401 on Clerk token refresh.
    #[test]
    fn client_uat_not_confused_with_client() {
        // Store has __session (valid JWT with sid) and __client_uat — but NO __client.
        let cookies = vec![
            Cookie::for_test("astrotheoros.com", "__session", FAKE_SESSION_JWT),
            Cookie::for_test("astrotheoros.com", "__client_uat", "1234567890"),
        ];
        match credential_from_cookies(Format::Astrotheoros, &cookies) {
            Ok(ProviderCredential::Astrotheoros(AstrotheorosCredential::Cookie {
                client_cookie,
                ..
            })) => {
                assert!(
                    client_cookie.is_none(),
                    "__client_uat must NOT be matched as __client; got client_cookie = {client_cookie:?}"
                );
            }
            other => panic!("expected Astrotheoros Cookie credential, got {other:?}"),
        }
    }
}
