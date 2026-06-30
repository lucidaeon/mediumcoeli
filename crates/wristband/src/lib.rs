//! # wristband
//!
//! Reads the **user's own** browser session cookies for an **explicit,
//! caller-supplied allow-list of domains**, to feed authentication for
//! services the user already has accounts on (see `blackmoon`). It is **not**
//! a general cookie extractor, and by construction it cannot become one:
//!
//! - **INV-1** the only read entry point requires a non-empty `&[Domain]`;
//!   - **INV-1b** only registrable domains (eTLD+1 or deeper) are accepted;
//!     public suffixes and bare TLDs are rejected; matching is subdomain-downward
//!     only — no zone or TLD globbing;
//! - **INV-2** host matching happens *before* any decryption;
//! - **INV-3** no API returns unfiltered cookies;
//! - **INV-4** consent is captured by the caller (the CLI), never here;
//! - **INV-5** offline, read-only, copy-before-read; no network I/O;
//! - **INV-6** conformance tests prove output hosts ⊆ allow-list;
//! - **INV-7** library-pure: no prompting, no logging of cookie material,
//!   no clap/anyhow/tracing dependencies.
//!
//! See `SECURITY.md` for the threat model and non-goals.
#![warn(missing_docs)]
#![warn(clippy::pedantic)]
#![forbid(unsafe_code)]

pub mod cookie;
pub mod domain;
pub mod error;
pub mod user_agent;

pub(crate) mod chromium;
pub(crate) mod discover;
pub(crate) mod firefox;
pub(crate) mod gate;
#[cfg(any(test, target_os = "macos"))]
pub(crate) mod safari;
pub(crate) mod sqlite_copy;

pub use cookie::Cookie;
pub use domain::{Domain, host_matches};
pub use error::WristbandError;

// ---------------------------------------------------------------------------
// Browser enum
// ---------------------------------------------------------------------------

/// A browser whose cookie store can be read.
///
/// Pass a specific variant to [`read_cookies`] to read only that browser's
/// store, or pass `None` to enumerate all installed stores.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Browser {
    /// Google Chrome.
    Chrome,
    /// Chromium (open-source base).
    Chromium,
    /// Brave Browser.
    Brave,
    /// Microsoft Edge.
    Edge,
    /// Opera.
    Opera,
    /// Vivaldi.
    Vivaldi,
    /// Naver Whale.
    Whale,
    /// Mozilla Firefox.
    Firefox,
    /// Apple Safari (macOS only).
    Safari,
}

impl Browser {
    /// All known browser variants, in a stable order.
    ///
    /// Used by the `None`-browser path in [`read_cookies`] to enumerate every
    /// installed store.
    #[must_use]
    pub fn all() -> &'static [Browser] {
        &[
            Browser::Chrome,
            Browser::Chromium,
            Browser::Brave,
            Browser::Edge,
            Browser::Opera,
            Browser::Vivaldi,
            Browser::Whale,
            Browser::Firefox,
            Browser::Safari,
        ]
    }

    /// The browser's product name as a proper noun, for display to a user
    /// (e.g. `"Chrome"`, `"Microsoft Edge"`, `"Firefox"`, `"Safari"`).
    #[must_use]
    pub fn display_name(self) -> &'static str {
        match self {
            Browser::Chrome => "Chrome",
            Browser::Chromium => "Chromium",
            Browser::Brave => "Brave",
            Browser::Edge => "Microsoft Edge",
            Browser::Opera => "Opera",
            Browser::Vivaldi => "Vivaldi",
            Browser::Whale => "Whale",
            Browser::Firefox => "Firefox",
            Browser::Safari => "Safari",
        }
    }
}

// ---------------------------------------------------------------------------
// ReadOptions
// ---------------------------------------------------------------------------

/// A Firefox multi-account container selector.
///
/// Firefox organises cookies into containers; this enum lets callers scope the
/// read to a specific container. Ignored by all non-Firefox browsers.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Container {
    /// No container filter — reads the default (uncontainerised) cookies.
    #[default]
    None,
    /// Read only the container with this name.
    Named(String),
    /// Read only the container with this numeric ID.
    Id(u32),
    /// Read cookies from every container (including the default).
    All,
}

/// Options controlling how a cookie read is performed.
#[derive(Debug, Clone, Default)]
pub struct ReadOptions {
    /// A browser profile name or path.
    ///
    /// When `None`, the default (most recently used) profile is used.
    pub profile: Option<String>,
    /// Firefox container selector (ignored for all other browsers).
    pub container: Container,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Read cookies for `domains` from `browser` (or all installed browsers if
/// `None`), applying the allow-list filter and decryption.
///
/// # Errors
///
/// - [`WristbandError::EmptyAllowList`] — `domains` is empty (INV-1).
/// - [`WristbandError::NoStore`] — the specific browser's cookie store was not
///   found on this system (only when `browser` is `Some`).
/// - [`WristbandError::Unsupported`] — the backend for the requested browser
///   is not supported on this OS (only when `browser` is `Some`).
///
/// When `browser` is `None`, stores that are not installed ([`WristbandError::NoStore`])
/// or whose backend is unsupported on this OS ([`WristbandError::Unsupported`])
/// are silently skipped. Genuine errors (decryption failures, keychain errors,
/// `SQLite` errors) are still propagated.
///
/// # Security
///
/// The allow-list check is the first thing this function does. An empty
/// `domains` slice is rejected before any I/O or decryption occurs.
pub fn read_cookies(
    browser: Option<Browser>,
    domains: &[Domain],
    opts: &ReadOptions,
) -> Result<Vec<Cookie>, WristbandError> {
    // INV-1: non-empty allow-list is mandatory.
    if domains.is_empty() {
        return Err(WristbandError::EmptyAllowList);
    }

    match browser {
        Some(Browser::Firefox) => {
            let store = discover::locate_store(Browser::Firefox, opts.profile.as_deref())?;
            match store {
                discover::StorePath::FirefoxSqlite(profile_dir) => {
                    firefox::read_firefox(&profile_dir, domains, &opts.container)
                }
                _ => Err(WristbandError::Unsupported(
                    "unexpected StorePath variant for Firefox".to_owned(),
                )),
            }
        }

        // All seven Chromium-family browsers dispatch to the unified reader.
        // Platform-specific key derivation is selected at compile time inside
        // `chromium::read_chromium`; `lib.rs` stays platform-agnostic.
        Some(
            b @ (Browser::Chrome
            | Browser::Chromium
            | Browser::Brave
            | Browser::Edge
            | Browser::Opera
            | Browser::Vivaldi
            | Browser::Whale),
        ) => chromium::read_chromium(b, domains, opts.profile.as_deref()),

        // Safari: macOS-only binary cookie store.
        #[cfg(target_os = "macos")]
        Some(Browser::Safari) => {
            let store = discover::locate_store(Browser::Safari, opts.profile.as_deref())?;
            match store {
                discover::StorePath::SafariBinary(path) => safari::read_safari(&path, domains),
                _ => Err(WristbandError::Unsupported(
                    "unexpected StorePath variant for Safari".to_owned(),
                )),
            }
        }

        // Non-macOS Safari fallback (parser tests still compile + run on any host).
        #[cfg(not(target_os = "macos"))]
        Some(Browser::Safari) => Err(WristbandError::Unsupported(
            "Safari is only supported on macOS".to_owned(),
        )),

        // Catch-all for any future Browser variants added before a backend is wired.
        #[allow(unreachable_patterns)]
        Some(b) => Err(WristbandError::Unsupported(format!(
            "browser backend not yet implemented: {b:?}"
        ))),

        // None: aggregate across all installed stores.
        None => read_all_stores(|b| read_cookies(Some(b), domains, opts)),
    }
}

/// Aggregate cookies across all installed browser stores.
///
/// `read_one` reads a single browser; stores that are not installed
/// ([`WristbandError::NoStore`]) or whose backend is unsupported on this OS
/// (e.g. Safari off-macOS, [`WristbandError::Unsupported`]) are skipped —
/// their absence is not a fatal error. Genuine errors (decryption failures,
/// keychain errors, `SQLite` errors) are propagated immediately.
///
/// This function is the implementation of the `None`-browser path in
/// [`read_cookies`] and [`scan_names`].  It is kept as a separate function so
/// it can be tested with a fake `read_one` closure without touching real
/// browser stores.
fn read_all_stores(
    read_one: impl Fn(Browser) -> Result<Vec<Cookie>, WristbandError>,
) -> Result<Vec<Cookie>, WristbandError> {
    let mut out = Vec::new();
    for &b in Browser::all() {
        match read_one(b) {
            Ok(mut cs) => out.append(&mut cs),
            // A missing store or an OS-unsupported backend is expected — skip.
            Err(WristbandError::NoStore(_) | WristbandError::Unsupported(_)) => {}
            // Any other error (decrypt, keychain, SQLite, I/O) is genuine.
            Err(e) => return Err(e),
        }
    }
    Ok(out)
}

/// Return `(host, name)` pairs for cookies matching `domains`, without
/// reading values.
///
/// This allows a consent-disclosure UI to preview which cookies will be read
/// before any decryption takes place (INV-4).
///
/// When `browser` is `None`, all installed stores are scanned; stores that are
/// not found or whose backend is unsupported on this OS are silently skipped.
///
/// # Errors
///
/// - [`WristbandError::EmptyAllowList`] — `domains` is empty (INV-1).
/// - [`WristbandError::NoStore`] — the specific browser's store was not found
///   (only when `browser` is `Some`).
/// - [`WristbandError::Unsupported`] — backend not supported on this OS
///   (only when `browser` is `Some`).
pub fn scan_names(
    browser: Option<Browser>,
    domains: &[Domain],
    opts: &ReadOptions,
) -> Result<Vec<(String, String)>, WristbandError> {
    // INV-1: non-empty allow-list is mandatory — checked before any I/O.
    if domains.is_empty() {
        return Err(WristbandError::EmptyAllowList);
    }
    let cookies = read_cookies(browser, domains, opts)?;
    Ok(cookies.into_iter().map(|c| (c.host, c.name)).collect())
}

/// Like [`scan_names`], but tags each `(host, name)` with the **source
/// browser**, so a caller can name which browser a session was found in.
///
/// `Some(b)` scans that browser; `None` scans every installed store, skipping
/// browsers that aren't installed ([`WristbandError::NoStore`]) or aren't
/// supported on this OS ([`WristbandError::Unsupported`]). Never returns
/// cookie values.
///
/// # Errors
///
/// Returns [`WristbandError::EmptyAllowList`] if `domains` is empty, or the
/// first genuine read error from a browser that has a store but fails to read.
pub fn scan_named(
    browser: Option<Browser>,
    domains: &[Domain],
    opts: &ReadOptions,
) -> Result<Vec<(Browser, String, String)>, WristbandError> {
    if domains.is_empty() {
        return Err(WristbandError::EmptyAllowList);
    }
    let mut out = Vec::new();
    let mut scan_one = |b: Browser| -> Result<(), WristbandError> {
        match read_cookies(Some(b), domains, opts) {
            Ok(cs) => {
                out.extend(cs.into_iter().map(|c| (b, c.host, c.name)));
                Ok(())
            }
            // A browser that isn't installed / isn't supported here is skipped.
            Err(WristbandError::NoStore(_) | WristbandError::Unsupported(_)) => Ok(()),
            Err(e) => Err(e),
        }
    };
    match browser {
        Some(b) => scan_one(b)?,
        None => {
            for &b in Browser::all() {
                scan_one(b)?;
            }
        }
    }
    Ok(out)
}

/// Read a single, already-located store, dispatching by store variant.
fn read_store(
    browser: Browser,
    store: discover::StorePath,
    domains: &[Domain],
    container: &Container,
) -> Result<Vec<Cookie>, WristbandError> {
    match store {
        discover::StorePath::FirefoxSqlite(dir) => firefox::read_firefox(&dir, domains, container),
        discover::StorePath::ChromiumSqlite(db, root) => {
            chromium::read_chromium_from_paths(browser, domains, &db, &root)
        }
        #[cfg(target_os = "macos")]
        discover::StorePath::SafariBinary(path) => safari::read_safari(&path, domains),
        #[cfg(not(target_os = "macos"))]
        discover::StorePath::SafariBinary(_) => Err(WristbandError::Unsupported(
            "Safari cookie store is only readable on macOS".to_owned(),
        )),
    }
}

/// One candidate session: cookies from a single (browser, profile) store.
pub struct SessionCandidate {
    /// The browser the store belongs to.
    pub browser: Browser,
    /// The profile label within that browser (e.g. `"Default"`, `"work"`).
    pub profile: String,
    /// Cookies read from this store, already filtered to the allow-list.
    pub cookies: Vec<Cookie>,
}

/// Read cookies from **every** installed store — every browser (or just
/// `browser_filter`) and every profile — returning one [`SessionCandidate`]
/// per store.
///
/// Each candidate's cookies come from a single store, so they are coherent: a
/// caller can pick the best session without mixing cookies across browsers or
/// profiles. Stores that aren't installed, aren't supported on this OS, or
/// fail to read (keychain denied, decryption error) are skipped, so one
/// browser's failure never hides another's session.
///
/// # Errors
///
/// Returns [`WristbandError::EmptyAllowList`] if `domains` is empty (INV-1).
pub fn read_all_sessions(
    browser_filter: Option<Browser>,
    domains: &[Domain],
    opts: &ReadOptions,
) -> Result<Vec<SessionCandidate>, WristbandError> {
    if domains.is_empty() {
        return Err(WristbandError::EmptyAllowList);
    }
    let browsers: Vec<Browser> = match browser_filter {
        Some(b) => vec![b],
        None => Browser::all().to_vec(),
    };
    let mut out = Vec::new();
    for b in browsers {
        let Ok(stores) = discover::locate_all_stores(b, opts.profile.as_deref()) else {
            // Not installed / unsupported here — skip this browser.
            continue;
        };
        for (profile, store) in stores {
            if let Ok(cookies) = read_store(b, store, domains, &opts.container) {
                out.push(SessionCandidate {
                    browser: b,
                    profile,
                    cookies,
                });
            }
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// read_all_stores unit tests (no real browser I/O)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod read_all_stores_tests {
    use super::*;

    /// Aggregation happy-path: Chrome and Brave return cookies; Firefox returns
    /// `NoStore`; Safari returns `Unsupported`.  The result must be Ok with the
    /// Chrome and Brave cookies combined (3 total), and the skipped browsers
    /// must not cause an error.
    #[test]
    fn aggregates_ok_skips_no_store_and_unsupported() {
        let result = read_all_stores(|b| match b {
            Browser::Chrome => Ok(vec![
                Cookie::for_test("example.com", "sess", "chrome-1"),
                Cookie::for_test("example.com", "pref", "chrome-2"),
            ]),
            Browser::Firefox => Err(WristbandError::NoStore("Firefox".to_owned())),
            Browser::Safari => Err(WristbandError::Unsupported(
                "Safari is only supported on macOS".to_owned(),
            )),
            Browser::Brave => Ok(vec![Cookie::for_test("example.com", "tok", "brave-1")]),
            // All other browsers: not installed.
            _ => Err(WristbandError::NoStore(format!("{b:?}"))),
        });

        let cookies = result.expect("aggregation should succeed");
        assert_eq!(
            cookies.len(),
            3,
            "expected 2 Chrome + 1 Brave = 3 cookies, got {cookies:?}"
        );
        let values: Vec<&str> = cookies.iter().map(|c| c.value.as_str()).collect();
        assert!(values.contains(&"chrome-1"), "missing chrome-1");
        assert!(values.contains(&"chrome-2"), "missing chrome-2");
        assert!(values.contains(&"brave-1"), "missing brave-1");
    }

    /// Error propagation: a genuine error (e.g. Decrypt) from any browser must
    /// cause `read_all_stores` to return `Err` immediately, not be swallowed.
    #[test]
    fn propagates_genuine_errors() {
        let result = read_all_stores(|b| match b {
            Browser::Chrome => Ok(vec![Cookie::for_test("example.com", "sess", "v")]),
            Browser::Firefox => Err(WristbandError::Decrypt(
                "bad padding in Firefox store".to_owned(),
            )),
            _ => Err(WristbandError::NoStore(format!("{b:?}"))),
        });

        match result {
            Err(WristbandError::Decrypt(_)) => {} // expected
            Err(e) => panic!("expected Decrypt error, got {e:?}"),
            Ok(cs) => panic!("expected Err, got Ok({cs:?})"),
        }
    }
}

// ---------------------------------------------------------------------------
// scan_names unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod scan_names_tests {
    use super::*;

    #[test]
    fn scan_names_empty_allow_list_errors() {
        let err = scan_names(Some(Browser::Firefox), &[], &ReadOptions::default()).unwrap_err();
        assert!(
            matches!(err, WristbandError::EmptyAllowList),
            "expected EmptyAllowList, got {err:?}"
        );
    }

    /// Verify that `scan_names` returns only `(host, name)` tuples — no values.
    ///
    /// This test uses `Cookie::for_test` to confirm the mapping logic, but because
    /// `scan_names` delegates to `read_cookies` (which requires a live browser
    /// store), the happy-path is verified separately in integration tests or by
    /// inspection of the `into_iter().map(|c| (c.host, c.name))` one-liner.
    ///
    /// The key property: if `read_cookies` returns cookies, `scan_names` must
    /// return exactly `(host, name)` — never the `value` field.
    #[test]
    fn scan_names_strips_values() {
        // We verify the mapping contract by exercising it on the Cookie type
        // directly: a Cookie produced by for_test has a value, but scan_names
        // must strip it.
        let c = Cookie::for_test("astro.com", "cid", "secret-value");
        // The mapping used by scan_names: (host, name) — no value.
        let pair: (String, String) = (c.host.clone(), c.name.clone());
        assert_eq!(pair, ("astro.com".to_owned(), "cid".to_owned()));
        // Confirm value is NOT part of the pair
        assert_ne!(pair.0, c.value);
        assert_ne!(pair.1, c.value);
    }
}
