//! Browser User-Agent divination: read the chosen browser's on-disk version
//! (beside the cookie stores wristband already locates under consent) and
//! interpolate a per-browser UA template; fall back to a maintained pinned
//! version when detection fails. Version detection only — never fingerprinting.
//!
//! ## Chromium engine vs. product version
//!
//! A Chromium-family UA carries the **Chromium engine** version in its
//! `Chrome/<v>` token and, optionally, the browser's own **product** version in
//! a trailing token (`OPR/`, `Edg/`, `Whale/`). These can be two different
//! numbers: Chrome and Chromium store the Chromium version directly in their
//! on-disk `Last Version` file, and Brave and Edge *prefix* their version with
//! the Chromium major (Brave `145.1.87.192` = Chromium 145), so the engine token
//! is recoverable from disk for all four. Vivaldi, Opera, and Whale store a
//! product version unrelated to the engine (e.g. Vivaldi `8.0.4033.54`), which
//! must never land in the `Chrome/<v>` token — those fall back to the maintained
//! [`CHROMIUM_PIN`]. Modern Brave and Vivaldi emit **no** product token at all
//! (reduced UA) and so render as a plain Chrome string. See *Detection
//! confidence* below.
//!
//! ## UA reduction
//!
//! Modern browsers freeze the low-order digits of the version they report, so a
//! detected on-disk build must be *reduced* to match what the browser actually
//! sends — emitting the full build is an over-precise fingerprinting tell:
//! - **Chrome / Chromium**: `Chrome/<major>.0.0.0` (e.g. `149.0.7827.201` →
//!   `149.0.0.0`). [`CHROMIUM_PIN`] is already in this form.
//! - **Firefox**: `<major>.<minor>` only — patch dropped (`152.0.3` → `152.0`) —
//!   and a frozen, dotted macOS token (`Intel Mac OS X 10.15`) distinct from the
//!   underscored WebKit/Blink one. See [`firefox_os_token`].
//!
//! ## Detection confidence
//!
//! We prefer a *detected* version over a maintained pin wherever the on-disk
//! value can be reduced to exactly what the browser sends. The `Chrome/<v>`
//! engine token falls into three tiers:
//! - **Detected engine** (Chrome, Chromium, Brave, Edge): the browser's own
//!   major tracks the Chromium major, so the detected version reduces straight to
//!   the engine token (Brave `Last Version` `145.1.87.192` → `Chrome/145.0.0.0`).
//! - **Detected product, pinned engine** (Opera, Whale): the product token uses
//!   the detected version, but the engine is unknowable from disk — the only
//!   genuine extrapolation left — so it uses [`CHROMIUM_PIN`].
//! - **Pinned engine, no token** (Vivaldi): engine pinned, no product token.
//!
//! Firefox and Safari are detected outright (Firefox `compatibility.ini`, Safari
//! the app bundle's `Info.plist` `CFBundleShortVersionString`).

use crate::Browser;

/// Maintained Chromium engine version for the `Chrome/<v>` UA token. Reduced-UA
/// form (`<major>.0.0.0`) — what current Chromium browsers actually send. The
/// `Chrome/<v>` token always carries the *Chromium* version, never a derivative
/// browser's own product version.
pub const CHROMIUM_PIN: &str = "148.0.0.0";

/// The platform OS token used inside UA strings.
#[must_use]
pub fn os_token() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "Macintosh; Intel Mac OS X 10_15_7"
    }
    #[cfg(target_os = "windows")]
    {
        "Windows NT 10.0; Win64; x64"
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        "X11; Linux x86_64"
    }
}

/// The OS token Firefox uses inside its UA. Distinct from [`os_token`] on macOS:
/// Firefox freezes the macOS version at a dotted `10.15` (`Intel Mac OS X 10.15`)
/// rather than the underscored `10_15_7` that WebKit/Blink browsers emit. Windows
/// and Linux match [`os_token`].
#[must_use]
pub fn firefox_os_token() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "Macintosh; Intel Mac OS X 10.15"
    }
    #[cfg(not(target_os = "macos"))]
    {
        os_token()
    }
}

/// Reduce a Chromium build version to the UA-Reduction form `<major>.0.0.0`
/// (e.g. `149.0.7827.201` → `149.0.0.0`) — what current Chrome/Chromium send.
#[must_use]
pub fn reduce_chromium(version: &str) -> String {
    let major = version.split('.').next().unwrap_or("0");
    format!("{major}.0.0.0")
}

/// Reduce a Firefox version to its frozen `<major>.<minor>` UA form, dropping the
/// patch (e.g. `152.0.3` → `152.0`). Firefox has reported only major.minor since
/// its UA freeze.
#[must_use]
pub fn reduce_firefox(version: &str) -> String {
    let mut it = version.split('.');
    let major = it.next().unwrap_or("0");
    let minor = it.next().unwrap_or("0");
    format!("{major}.{minor}")
}

/// Maintained pinned *own-version* fallback per browser, used when on-disk
/// detection fails. For the browsers that carry a product token (Opera, Edge,
/// Whale) or are not Chromium at all (Firefox, Safari) this is that browser's
/// own version; Chrome/Chromium/Brave/Vivaldi have no product token, so their
/// fallback is simply the [`CHROMIUM_PIN`] that fills their `Chrome/<v>` token.
#[must_use]
pub fn fallback_version(browser: Browser) -> &'static str {
    match browser {
        Browser::Chrome | Browser::Chromium | Browser::Brave | Browser::Vivaldi => CHROMIUM_PIN,
        Browser::Edge => "148.0.0.0",
        Browser::Opera => "114.0.0.0",
        Browser::Whale => "4.31.304.16",
        Browser::Firefox => "140.0",
        Browser::Safari => "26.5",
    }
}

/// Build a Chromium-family UA: the `Chrome/<chromium>` engine token plus an
/// optional trailing product token carrying `product`. Chrome, Chromium, Brave,
/// and Vivaldi emit no product token (modern reduced UA) and ignore `product`.
#[must_use]
pub fn chromium_ua(browser: Browser, chromium: &str, product: &str) -> String {
    let os = os_token();
    let base = format!(
        "Mozilla/5.0 ({os}) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/{chromium} Safari/537.36"
    );
    match browser {
        Browser::Edge => format!("{base} Edg/{product}"),
        Browser::Opera => format!("{base} OPR/{product}"),
        Browser::Whale => format!("{base} Whale/{product}"),
        // Chrome, Chromium, Brave, Vivaldi: no product token.
        _ => base,
    }
}

/// Build a non-Chromium UA (Firefox / Safari) with its single `version`.
#[must_use]
pub fn non_chromium_ua(browser: Browser, version: &str) -> String {
    if browser == Browser::Firefox {
        let os = firefox_os_token();
        return format!("Mozilla/5.0 ({os}; rv:{version}) Gecko/20100101 Firefox/{version}");
    }
    // Safari is the only other non-Chromium browser; treat any non-Firefox here
    // as Safari-shaped rather than panicking.
    let os = os_token();
    format!(
        "Mozilla/5.0 ({os}) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/{version} Safari/605.1.15"
    )
}

/// The `Chrome/<v>` engine version for a Chromium-family browser given its
/// detected own version. High confidence where the browser's own major tracks
/// the Chromium major (Chrome, Chromium, Brave, Edge): the detected version
/// reduces straight to the engine token. Otherwise (Vivaldi, Opera, Whale) the
/// on-disk value is a product version unrelated to the engine, so use the
/// maintained [`CHROMIUM_PIN`] — the only remaining extrapolation.
#[must_use]
fn chromium_engine(browser: Browser, own: &str) -> String {
    match browser {
        Browser::Chrome | Browser::Chromium | Browser::Brave | Browser::Edge => {
            reduce_chromium(own)
        }
        _ => CHROMIUM_PIN.to_string(),
    }
}

/// The product-token version (`OPR/`, `Edg/`, `Whale/`). Edge reduces its token
/// to UA-Reduction form to match the engine; Opera and Whale send their full
/// product version. Ignored by browsers that emit no product token.
#[must_use]
fn chromium_product_version(browser: Browser, own: &str) -> String {
    match browser {
        Browser::Edge => reduce_chromium(own),
        _ => own.to_string(),
    }
}

/// Compose a UA from a browser and its (optional) on-disk-detected own version.
/// Pure: holds the Chromium-vs-product policy without touching the filesystem,
/// so it is exhaustively unit-testable. See [`divine`] for the disk-backed entry.
#[must_use]
fn compose(browser: Browser, detected: Option<String>) -> String {
    let own = detected.unwrap_or_else(|| fallback_version(browser).to_string());
    match browser {
        // Firefox reports only major.minor (patch frozen out); Safari renders its
        // detected `Info.plist` version verbatim.
        Browser::Firefox => non_chromium_ua(browser, &reduce_firefox(&own)),
        Browser::Safari => non_chromium_ua(browser, &own),
        // Chromium family: resolve the engine token (detected-and-reduced where
        // the major tracks Chromium, else the pin) and the optional product token
        // independently. `chromium_ua` drops the product token where unused.
        _ => chromium_ua(
            browser,
            &chromium_engine(browser, &own),
            &chromium_product_version(browser, &own),
        ),
    }
}

/// Divine the User-Agent of `browser`: detect its installed version on disk and
/// interpolate the per-browser template, falling back to the maintained pinned
/// version when detection fails. Always returns a usable UA string.
#[must_use]
pub fn divine(browser: Browser, profile: Option<&str>) -> String {
    compose(browser, detect_version(browser, profile))
}

/// Parse a Chromium-family `Last Version` file body into a version string.
fn parse_chromium_last_version(body: &str) -> Option<String> {
    let v = body.trim();
    (!v.is_empty()).then(|| v.to_string())
}

/// Parse a Firefox profile `compatibility.ini` body, extracting the numeric
/// prefix of its `LastVersion=` line (e.g. `140.0.1_2025…` -> `140.0.1`).
fn parse_firefox_compatibility(body: &str) -> Option<String> {
    let line = body
        .lines()
        .find_map(|l| l.trim().strip_prefix("LastVersion="))?;
    let v = line.split('_').next().unwrap_or("").trim();
    (!v.is_empty()).then(|| v.to_string())
}

/// Extract `CFBundleShortVersionString` from an XML `Info.plist` body. macOS app
/// bundles store it as `<key>CFBundleShortVersionString</key><string>V</string>`
/// (Safari's is XML text, so a pure read suffices — no subprocess, no plist
/// crate). Returns the `V` between the first `<string>…</string>` that follows
/// the key.
fn parse_plist_short_version(body: &str) -> Option<String> {
    let after_key = body.split("<key>CFBundleShortVersionString</key>").nth(1)?;
    let open = after_key.find("<string>")? + "<string>".len();
    let close = after_key[open..].find("</string>")?;
    let v = after_key[open..open + close].trim();
    (!v.is_empty()).then(|| v.to_string())
}

/// Detect the installed browser's version from its on-disk source.
/// `None` when no source exists (e.g. browser not installed).
#[must_use]
pub fn detect_version(browser: Browser, profile: Option<&str>) -> Option<String> {
    let path = crate::discover::version_source(browser, profile)?;
    let body = std::fs::read_to_string(&path).ok()?;
    match browser {
        Browser::Firefox => parse_firefox_compatibility(&body),
        Browser::Safari => parse_plist_short_version(&body),
        _ => parse_chromium_last_version(&body),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_chromium_last_version_trims() {
        // Pure parse check: a "Last Version" file body trims to a version.
        assert_eq!(
            super::parse_chromium_last_version("148.0.7274.0\n"),
            Some("148.0.7274.0".to_string())
        );
        assert_eq!(super::parse_chromium_last_version("   "), None);
    }

    #[test]
    fn parse_firefox_compatibility_extracts_lastversion() {
        let ini = "[Compatibility]\nLastVersion=140.0.1_20250601_/Applications/Firefox.app\nLastOSABI=Darwin_aarch64-gcc3\n";
        assert_eq!(
            super::parse_firefox_compatibility(ini),
            Some("140.0.1".to_string())
        );
        assert_eq!(
            super::parse_firefox_compatibility("[Compatibility]\n"),
            None
        );
    }

    #[test]
    fn chromium_ua_appends_product_token_only_where_real() {
        // Chromium engine token always present; product token only for the
        // browsers that actually emit one.
        let chrome = chromium_ua(Browser::Chrome, "131.0.0.0", "131.0.0.0");
        assert!(chrome.ends_with("Chrome/131.0.0.0 Safari/537.36"));
        assert!(!chrome.contains("Edg/") && !chrome.contains("OPR/"));

        let edge = chromium_ua(Browser::Edge, "131.0.0.0", "131.0.2");
        assert!(edge.contains("Chrome/131.0.0.0"));
        assert!(edge.ends_with("Edg/131.0.2"));

        let opera = chromium_ua(Browser::Opera, "128.0.0.0", "114.0.5282.21");
        assert!(opera.contains("Chrome/128.0.0.0"));
        assert!(opera.ends_with("OPR/114.0.5282.21"));
    }

    #[test]
    fn non_chromium_ua_per_family() {
        let ff = non_chromium_ua(Browser::Firefox, "140.0");
        assert!(ff.contains("rv:140.0)") && ff.ends_with("Firefox/140.0"));

        let safari = non_chromium_ua(Browser::Safari, "18.5");
        assert!(safari.contains("Version/18.5 Safari/605.1.15"));
    }

    #[test]
    fn vivaldi_uses_chromium_pin_not_product_version_and_no_token() {
        // Vivaldi's on-disk `Last Version` holds its *product* version (e.g.
        // 8.0.4033.54), which must never land in the `Chrome/<v>` token, and
        // modern Vivaldi emits no `Vivaldi/` token. Regression: previously
        // produced `Chrome/8.0.4033.54 ... Vivaldi/8.0.4033.54`. The real UA is
        // a plain Chrome string at the maintained Chromium pin.
        let ua = compose(Browser::Vivaldi, Some("8.0.4033.54".into()));
        assert_eq!(
            ua,
            format!(
                "Mozilla/5.0 ({}) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/{CHROMIUM_PIN} Safari/537.36",
                os_token()
            )
        );
        assert!(!ua.contains("Vivaldi"));
        assert!(!ua.contains("8.0.4033.54"));
    }

    #[test]
    fn brave_engine_derived_from_chromium_major_no_token() {
        // Brave's detected version leads with the Chromium major (real sample:
        // 145.1.87.192 = Chromium 145 + Brave 1.87.192), so reduce it straight to
        // the engine token — high confidence, not the static pin. No `Brave/`
        // token (modern reduced UA).
        let ua = compose(Browser::Brave, Some("145.1.87.192".into()));
        assert!(ua.ends_with("Chrome/145.0.0.0 Safari/537.36"), "{ua}");
        assert!(!ua.contains("Brave") && !ua.contains("1.87"), "{ua}");
    }

    #[test]
    fn edge_derives_engine_and_reduces_its_own_token() {
        // Edge tracks the Chromium major; both the `Chrome/` engine token and the
        // `Edg/` product token reduce to it.
        let ua = compose(Browser::Edge, Some("149.0.3537.57".into()));
        assert!(ua.contains("Chrome/149.0.0.0"), "{ua}");
        assert!(ua.ends_with("Edg/149.0.0.0"), "{ua}");
    }

    #[test]
    fn chrome_detected_build_is_reduced_to_major_only() {
        // Chrome/Chromium are the one family whose `Last Version` *is* the
        // Chromium version, but it must be reduced to UA-Reduction form: the
        // full on-disk build (149.0.7827.201) is never what Chrome sends.
        // Regression: previously emitted `Chrome/149.0.7827.201`.
        for browser in [Browser::Chrome, Browser::Chromium] {
            let ua = compose(browser, Some("149.0.7827.201".into()));
            assert!(ua.ends_with("Chrome/149.0.0.0 Safari/537.36"), "{ua}");
            assert!(!ua.contains("7827"), "{ua}");
        }
    }

    #[test]
    fn firefox_reduces_to_major_minor_with_frozen_os_token() {
        // Firefox drops the patch (152.0.3 -> 152.0) and uses its own dotted
        // macOS token. Regression: previously emitted `rv:152.0.3 ... 10_15_7`.
        let ua = compose(Browser::Firefox, Some("152.0.3".into()));
        assert!(
            ua.contains("rv:152.0)") && ua.ends_with("Firefox/152.0"),
            "{ua}"
        );
        assert!(!ua.contains("152.0.3"), "{ua}");
        assert!(ua.contains(firefox_os_token()), "{ua}");
    }

    #[test]
    fn reduce_helpers_freeze_low_order_digits() {
        assert_eq!(reduce_chromium("149.0.7827.201"), "149.0.0.0");
        assert_eq!(reduce_chromium("148"), "148.0.0.0");
        assert_eq!(reduce_firefox("152.0.3"), "152.0");
        assert_eq!(reduce_firefox("140"), "140.0");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn firefox_macos_token_is_dotted_not_underscored() {
        assert_eq!(firefox_os_token(), "Macintosh; Intel Mac OS X 10.15");
        assert_ne!(firefox_os_token(), os_token());
    }

    #[test]
    fn opera_splits_chromium_pin_and_detected_product_token() {
        let ua = compose(Browser::Opera, Some("114.0.5282.21".into()));
        assert!(ua.contains(&format!("Chrome/{CHROMIUM_PIN}")));
        assert!(ua.ends_with("OPR/114.0.5282.21"));
    }

    #[test]
    fn parse_plist_short_version_extracts_safari_version() {
        // Safari's XML Info.plist: the value is the first <string> after the key.
        let xml = "<plist><dict>\n\
            <key>CFBundleName</key><string>Safari</string>\n\
            <key>CFBundleShortVersionString</key>\n<string>26.5</string>\n\
            <key>CFBundleVersion</key><string>21624.2.5.11.4</string>\n\
            </dict></plist>";
        assert_eq!(parse_plist_short_version(xml), Some("26.5".to_string()));
        // Key absent -> None (falls back to the pin).
        assert_eq!(parse_plist_short_version("<plist></plist>"), None);
    }

    #[test]
    fn safari_version_detected_from_bundle_when_present() {
        // Live, macOS-only: when Safari is installed, detection reads a real
        // version from the app bundle's Info.plist (not the maintained pin).
        // Skips cleanly where Safari isn't present (CI / non-macOS).
        match detect_version(Browser::Safari, None) {
            Some(v) => {
                eprintln!("detected Safari version: {v}");
                assert!(
                    v.chars().next().is_some_and(|c| c.is_ascii_digit()),
                    "expected a numeric version, got {v:?}"
                );
            }
            None => eprintln!("Safari not detectable here — skipping"),
        }
    }

    #[test]
    fn fallback_version_is_nonempty_for_every_browser() {
        for &b in Browser::all() {
            assert!(!fallback_version(b).is_empty());
        }
    }

    #[test]
    fn safari_falls_back_to_pinned_version_when_undetectable() {
        // With no detected version, Safari uses the maintained pin and still
        // renders a valid UA. (Real detection reads the app bundle Info.plist —
        // exercised by `divine` on macOS.)
        let ua = compose(Browser::Safari, None);
        assert!(ua.starts_with("Mozilla/5.0"));
        assert!(ua.contains(&format!("Version/{}", fallback_version(Browser::Safari))));
        assert!(ua.ends_with("Safari/605.1.15"));
    }
}
