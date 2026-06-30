//! User-Agent selection for `astrogram`'s web sessions. Owns the fixed `STATIC`
//! spoof, composes the compile-time `self_reported` string, and resolves a
//! [`UaChoice`] to the final UA. The cookie-source browser's own UA is divined
//! by `wristband` and carried in [`UaChoice::Cookie`]. astrotheoros is out of
//! scope and keeps its own constant.

/// The fixed User-Agent historically hardcoded in the codebase (a desktop
/// Chrome spoof). Selected by a bare `--ua`.
pub const STATIC: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 \
     (KHTML, like Gecko) Chrome/148.0.0.0 Safari/537.36";

/// Truncate a `CARGO_PKG_VERSION` (`major.minor.patch`) to `major.minor`.
#[must_use]
pub fn major_minor(version: &str) -> String {
    let mut it = version.split('.');
    let major = it.next().unwrap_or("0");
    let minor = it.next().unwrap_or("0");
    format!("{major}.{minor}")
}

/// An application's product token for the self-reported User-Agent — the app
/// identity an `astrogram` consumer MUST declare. Construct with [`AppProduct::new`].
#[derive(Debug, Clone)]
pub struct AppProduct(String);

impl AppProduct {
    /// Build `"{name}/{major.minor}"` from a product name and a full
    /// `CARGO_PKG_VERSION` (patch dropped). E.g. `("Blackmoon", "0.2.2")` →
    /// `Blackmoon/0.2`.
    #[must_use]
    pub fn new(name: &str, version: &str) -> Self {
        Self(format!("{name}/{}", major_minor(version)))
    }
}

impl std::fmt::Display for AppProduct {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Compose the self-reported UA: `Mozilla/5.0 <app> Astrogram/<maj.min>`.
#[must_use]
pub fn self_reported(app: &AppProduct) -> String {
    let astro = major_minor(env!("CARGO_PKG_VERSION"));
    format!("Mozilla/5.0 {app} Astrogram/{astro}")
}

/// Which User-Agent a web session should send.
pub enum UaChoice {
    /// The compile-time self-reported UA (default; no cookie grant).
    SelfReported,
    /// The cookie-source browser's own divined UA.
    Cookie(String),
    /// The fixed [`STATIC`] spoof (bare `--ua`).
    Static,
    /// A caller-supplied verbatim UA (`--ua <string>`).
    Custom(String),
}

/// A frontend-neutral expression of *what the operator asked for*, decoupled
/// from any CLI flag type. Each frontend (CLI flags, GUI widgets) maps its own
/// input to a `UaIntent`, then calls [`choose`] — so the User-Agent *policy*
/// lives in one place and cannot drift between frontends.
pub enum UaIntent {
    /// No explicit override — send the honest self-reported UA.
    Default,
    /// Mimic the cookie-source browser's own divined UA (opt-in).
    MimicBrowser,
    /// The fixed [`STATIC`] spoof.
    Static,
    /// A verbatim UA string.
    Custom(String),
}

/// Decide which [`UaChoice`] to send, given whether cookie access was granted,
/// the operator's [`UaIntent`], and the cookie-source browser's divined UA (when
/// a cookie actually authenticated the session).
///
/// **Granting cookie access never implies impersonation:** without an explicit
/// [`UaIntent::MimicBrowser`] the result is [`UaChoice::SelfReported`], even when
/// a `cookie_ua` is available; `MimicBrowser` itself falls back to honest when no
/// cookie was used. UA overrides are gated on `grant` (the override only makes
/// sense for cookie-bound requests), so `grant == false` always yields
/// [`UaChoice::SelfReported`]. Shared by every frontend so the privacy default
/// cannot regress in one of them.
#[must_use]
pub fn choose(grant: bool, intent: UaIntent, cookie_ua: Option<String>) -> UaChoice {
    if !grant {
        return UaChoice::SelfReported;
    }
    match intent {
        UaIntent::Custom(s) => UaChoice::Custom(s),
        UaIntent::Static => UaChoice::Static,
        UaIntent::MimicBrowser => cookie_ua.map_or(UaChoice::SelfReported, UaChoice::Cookie),
        UaIntent::Default => UaChoice::SelfReported,
    }
}

/// A short human label for a [`UaChoice`] variant, for disclosure output.
#[must_use]
pub fn ua_kind_label(choice: &UaChoice) -> &'static str {
    match choice {
        UaChoice::SelfReported => "self-reported",
        UaChoice::Cookie(_) => "browser",
        UaChoice::Static => "static",
        UaChoice::Custom(_) => "custom",
    }
}

/// Resolve a [`UaChoice`] to the final UA string. `app` is used only by
/// [`UaChoice::SelfReported`].
#[must_use]
pub fn resolve(choice: UaChoice, app: &AppProduct) -> String {
    match choice {
        UaChoice::SelfReported => self_reported(app),
        UaChoice::Cookie(s) | UaChoice::Custom(s) => s,
        UaChoice::Static => STATIC.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn major_minor_drops_patch() {
        assert_eq!(major_minor("0.2.2"), "0.2");
        assert_eq!(major_minor("1.0"), "1.0");
        assert_eq!(major_minor("3"), "3.0");
    }

    #[test]
    fn app_product_composes_name_and_major_minor() {
        let app = AppProduct::new("Blackmoon", "0.2.2");
        assert_eq!(app.to_string(), "Blackmoon/0.2");
    }

    #[test]
    fn self_reported_requires_app_and_has_both_products() {
        let app = AppProduct::new("Blackmoon", "0.2.2");
        let ua = self_reported(&app);
        assert!(ua.starts_with("Mozilla/5.0 Blackmoon/0.2 Astrogram/"));
    }

    #[test]
    fn resolve_maps_each_kind() {
        let app = AppProduct::new("Blackmoon", "0.2.2");
        assert_eq!(resolve(UaChoice::Static, &app), STATIC);
        assert_eq!(resolve(UaChoice::Cookie("X".into()), &app), "X");
        assert_eq!(resolve(UaChoice::Custom("Y".into()), &app), "Y");
        assert!(resolve(UaChoice::SelfReported, &app).contains("Blackmoon/0.2"));
    }

    #[test]
    fn choose_keeps_browser_impersonation_opt_in() {
        use UaIntent::{Custom, Default, MimicBrowser, Static};
        // No grant -> always honest, regardless of intent or an available cookie.
        assert!(matches!(
            choose(false, MimicBrowser, Some("UA".into())),
            UaChoice::SelfReported
        ));
        assert!(matches!(
            choose(false, Default, None),
            UaChoice::SelfReported
        ));
        // Granted but no explicit override -> honest even when a cookie is on hand
        // (the regression guard: cookie *access* never implies *impersonation*).
        assert!(matches!(
            choose(true, Default, Some("UA".into())),
            UaChoice::SelfReported
        ));
        // Explicit opt-in mimics the browser; falls back to honest with no cookie.
        assert!(matches!(
            choose(true, MimicBrowser, Some("UA".into())),
            UaChoice::Cookie(ref s) if s == "UA"
        ));
        assert!(matches!(
            choose(true, MimicBrowser, None),
            UaChoice::SelfReported
        ));
        // Static / Custom pass through under grant.
        assert!(matches!(
            choose(true, Static, Some("UA".into())),
            UaChoice::Static
        ));
        assert!(matches!(
            choose(true, Custom("X".into()), None),
            UaChoice::Custom(ref s) if s == "X"
        ));
    }

    #[test]
    fn ua_kind_label_names_each_variant() {
        assert_eq!(ua_kind_label(&UaChoice::SelfReported), "self-reported");
        assert_eq!(ua_kind_label(&UaChoice::Cookie("x".into())), "browser");
        assert_eq!(ua_kind_label(&UaChoice::Static), "static");
        assert_eq!(ua_kind_label(&UaChoice::Custom("x".into())), "custom");
    }
}
