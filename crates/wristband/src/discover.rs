//! Per-OS, per-browser profile and cookie-store discovery.
//!
//! The single public entry point is [`locate_store`], which resolves real
//! platform-specific base directories and returns the path to the browser's
//! cookie store (or the profile directory containing it for Firefox).
//!
//! # Testability
//!
//! Real root resolution is separated from the search logic through an
//! injectable [`Roots`] struct. [`locate_store`] builds a real `Roots` from
//! `dirs`/`#[cfg(target_os = …)]`; [`locate_store_in`] accepts any `Roots`
//! and is what tests call, passing a synthetic tempdir tree instead of `$HOME`.
//!
//! # Discovery rules (summary)
//!
//! - **Chromium family** — per-OS browser root under `%LOCALAPPDATA%` (Windows,
//!   except Opera which uses `%APPDATA%`), `~/Library/Application Support/<Name>`
//!   (macOS), or `~/.config/<slug>` (Linux). The cookie DB is a file named
//!   `Cookies`; when `profile` is `None` the file with the most-recent mtime
//!   among all `Cookies` candidates in the browser dir tree is chosen.
//!
//! - **Firefox** — per-OS base dirs; within each base, try the root directly,
//!   one directory deep, and the `Profiles/*/` glob. Among all `cookies.sqlite`
//!   files found across every base dir, pick the newest by mtime and return its
//!   PARENT (the profile directory).
//!
//! - **Safari (macOS only)** — `~/Library/Cookies/Cookies.binarycookies`, with
//!   a sandboxed fallback under `~/Library/Containers/com.apple.Safari/…`.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::Browser;
use crate::error::WristbandError;

// ---------------------------------------------------------------------------
// StorePath — what locate_store returns
// ---------------------------------------------------------------------------

/// The resolved on-disk location of a browser cookie store.
///
/// Callers match on the variant to know which backend reader to invoke.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum StorePath {
    /// A Chromium-family `SQLite` cookie database.
    ///
    /// Field `0` — path to the `Cookies` `SQLite` file.
    /// Field `1` — the browser root directory (parent of the profile dir; used by
    /// subsequent tasks to locate `Local State` / the OS encryption key).
    ChromiumSqlite(PathBuf, PathBuf),

    /// A Firefox profile directory (the directory that contains `cookies.sqlite`,
    /// `containers.json`, etc.).
    FirefoxSqlite(PathBuf),

    /// The path to a Safari `Cookies.binarycookies` file (macOS only).
    SafariBinary(PathBuf),
}

// ---------------------------------------------------------------------------
// Roots — injectable base directories for testability
// ---------------------------------------------------------------------------

/// Platform base directories used by [`locate_store_in`].
///
/// [`locate_store`] populates this from real OS APIs; tests inject a fake
/// `Roots` pointing at a `tempfile::TempDir` tree so they never touch `$HOME`.
#[derive(Debug, Clone)]
pub(crate) struct Roots {
    /// Candidate base directories for **Firefox** profiles.
    ///
    /// Each entry may contain `cookies.sqlite` directly, one level deep, or
    /// under a `Profiles/*/` glob.
    pub(crate) firefox_bases: Vec<PathBuf>,

    /// Base directory for **Chromium-family** browsers (local / non-roaming).
    ///
    /// On Windows this is `%LOCALAPPDATA%`; on macOS
    /// `~/Library/Application Support`; on Linux `~/.config`.
    /// Most Chromium browsers use this base.
    pub(crate) chromium_base: Option<PathBuf>,

    /// Roaming base directory for Chromium browsers that use `%APPDATA%` on
    /// Windows — specifically **Opera**.
    ///
    /// On Windows this is `%APPDATA%` (`dirs::data_dir()`).  On macOS and
    /// Linux there is no roaming/local distinction, so this field mirrors
    /// `chromium_base` (set to the same value).  The field is always
    /// `Some(…)` when `chromium_base` is `Some(…)`.
    pub(crate) chromium_base_roaming: Option<PathBuf>,

    /// The user's home directory, used for Safari paths.
    pub(crate) home: Option<PathBuf>,
}

// ---------------------------------------------------------------------------
// Chromium browser metadata
// ---------------------------------------------------------------------------

/// Returns the sub-directory name used by this Chromium browser under the
/// platform chromium base directory.
///
/// On macOS and Linux these are the directory names directly under
/// `~/Library/Application Support` / `~/.config`.  On Windows they sit under
/// `%LOCALAPPDATA%` (except Opera which uses `%APPDATA%`).
fn chromium_dir_name(browser: Browser) -> &'static str {
    match browser {
        Browser::Chrome => "Google/Chrome",
        Browser::Chromium => "Chromium",
        Browser::Brave => "BraveSoftware/Brave-Browser",
        Browser::Edge => "Microsoft Edge",
        Browser::Opera => "Opera Software/Opera Stable",
        Browser::Vivaldi => "Vivaldi",
        Browser::Whale => "Naver/Whale",
        // Firefox and Safari are not Chromium-family; caller should never ask.
        Browser::Firefox | Browser::Safari => unreachable!("not a Chromium browser"),
    }
}

// ---------------------------------------------------------------------------
// Helpers: find newest file by mtime
// ---------------------------------------------------------------------------

/// Return the mtime of `path`, or `UNIX_EPOCH` if unavailable.
fn mtime(path: &Path) -> SystemTime {
    path.metadata()
        .and_then(|m| m.modified())
        .unwrap_or(SystemTime::UNIX_EPOCH)
}

/// Walk `root` recursively, collecting all files named `filename`.
///
/// At most `max_depth` directory levels below `root` are visited.  Symlinks
/// are NOT followed to avoid loops.
fn collect_named(root: &Path, filename: &str, max_depth: usize) -> Vec<PathBuf> {
    let mut found = Vec::new();
    collect_named_inner(root, filename, max_depth, &mut found);
    found
}

fn collect_named_inner(dir: &Path, filename: &str, depth: usize, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() && depth > 0 {
            collect_named_inner(&path, filename, depth - 1, out);
        } else if path.is_file() && path.file_name().and_then(|n| n.to_str()) == Some(filename) {
            out.push(path);
        }
    }
}

/// Pick the entry in `candidates` with the most-recent mtime.
///
/// Returns `None` if `candidates` is empty.
fn newest(candidates: Vec<PathBuf>) -> Option<PathBuf> {
    candidates.into_iter().max_by_key(|p| mtime(p))
}

// ---------------------------------------------------------------------------
// Firefox discovery
// ---------------------------------------------------------------------------

/// Find Firefox `cookies.sqlite` files across `bases`.
///
/// For each base directory the search tries:
/// 1. `<base>/cookies.sqlite` — rare but possible.
/// 2. `<base>/<one-level>/cookies.sqlite`.
/// 3. `<base>/Profiles/*/cookies.sqlite` — the common layout.
///
/// All candidates are collected and the caller chooses the newest.
fn find_firefox_dbs(bases: &[PathBuf]) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for base in bases {
        // Root-level (uncommon but valid).
        let root_db = base.join("cookies.sqlite");
        if root_db.is_file() {
            found.push(root_db);
        }
        // One level deep (e.g. Firefox on some Linux layouts stores profiles
        // directly under the app-data dir).
        if let Ok(entries) = std::fs::read_dir(base) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    let db = p.join("cookies.sqlite");
                    if db.is_file() {
                        found.push(db);
                    }
                }
            }
        }
        // Profiles/* glob.
        let profiles_dir = base.join("Profiles");
        if profiles_dir.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&profiles_dir) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    if p.is_dir() {
                        let db = p.join("cookies.sqlite");
                        if db.is_file() {
                            found.push(db);
                        }
                    }
                }
            }
        }
    }
    found
}

/// Resolve a Firefox profile directory: find all `cookies.sqlite` across
/// `roots.firefox_bases`, pick the newest by mtime, and return its parent.
///
/// When `profile` is `Some(p)` the function first tries an exact match:
/// any found path whose parent directory name equals `p` is returned
/// immediately without mtime comparison.
fn locate_firefox(roots: &Roots, profile: Option<&str>) -> Result<StorePath, WristbandError> {
    let candidates = find_firefox_dbs(&roots.firefox_bases);

    if candidates.is_empty() {
        return Err(WristbandError::NoStore("Firefox".to_owned()));
    }

    // Exact profile-name match when requested.
    if let Some(name) = profile {
        let matched = candidates.iter().find(|p| {
            p.parent()
                .and_then(|pp| pp.file_name())
                .and_then(|n| n.to_str())
                == Some(name)
        });
        if let Some(db) = matched {
            let profile_dir = db
                .parent()
                .expect("cookies.sqlite always has a parent")
                .to_path_buf();
            return Ok(StorePath::FirefoxSqlite(profile_dir));
        }
    }

    // Fall back to newest-mtime.
    let best = newest(candidates).expect("non-empty checked above");
    let profile_dir = best
        .parent()
        .expect("cookies.sqlite always has a parent")
        .to_path_buf();
    Ok(StorePath::FirefoxSqlite(profile_dir))
}

// ---------------------------------------------------------------------------
// Chromium discovery
// ---------------------------------------------------------------------------

/// Locate the Chromium `Cookies` database for `browser`.
///
/// When `profile` is `Some(p)`, only the file under the sub-directory named
/// `p` inside the browser root is considered.  When `None`, the newest
/// `Cookies` file across all sub-directories is used.
fn locate_chromium(
    roots: &Roots,
    browser: Browser,
    profile: Option<&str>,
) -> Result<StorePath, WristbandError> {
    let browser_root = chromium_browser_root(roots, browser)
        .ok_or_else(|| WristbandError::NoStore(format!("{browser:?}")))?;

    if !browser_root.is_dir() {
        return Err(WristbandError::NoStore(format!("{browser:?}")));
    }

    let candidates: Vec<PathBuf> = if let Some(name) = profile {
        // Only look inside the named profile sub-directory.
        let db = browser_root.join(name).join("Cookies");
        if db.is_file() { vec![db] } else { vec![] }
    } else {
        // Walk up to 3 levels deep to cover:
        //   <browser_root>/Cookies                       (no sub-profile)
        //   <browser_root>/Default/Cookies               (single profile)
        //   <browser_root>/Profile 1/Cookies             (named profile)
        //   <browser_root>/User Data/Default/Cookies     (Windows layout)
        collect_named(&browser_root, "Cookies", 3)
    };

    if candidates.is_empty() {
        return Err(WristbandError::NoStore(format!("{browser:?}")));
    }

    let db = newest(candidates).expect("non-empty checked above");
    Ok(StorePath::ChromiumSqlite(db, browser_root))
}

/// Resolve the on-disk root directory for a Chromium-family browser, honoring
/// Opera's roaming-base (`%APPDATA%`) exception on Windows. `None` when the
/// platform base directory is unknown.
fn chromium_browser_root(roots: &Roots, browser: Browser) -> Option<PathBuf> {
    let base = if browser == Browser::Opera {
        roots
            .chromium_base_roaming
            .as_deref()
            .or(roots.chromium_base.as_deref())
    } else {
        roots.chromium_base.as_deref()
    }?;
    Some(base.join(chromium_dir_name(browser)))
}

// ---------------------------------------------------------------------------
// All-stores enumeration (every profile, not just the newest)
// ---------------------------------------------------------------------------

/// Profile label for a Chromium `Cookies` path, relative to the browser root
/// (e.g. `"Default"`, `"Profile 1"`). Empty/root maps to `"Default"`.
fn chromium_profile_label(db: &Path, browser_root: &Path) -> String {
    db.parent()
        .and_then(|p| p.strip_prefix(browser_root).ok())
        .map(|rel| rel.to_string_lossy().replace('\\', "/"))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Default".to_owned())
}

/// Profile label for a Firefox profile directory. Firefox names dirs like
/// `xxxxxxxx.default-release`; the label is the part after the first dot.
fn firefox_profile_label(profile_dir: &Path) -> String {
    profile_dir
        .file_name()
        .and_then(|n| n.to_str())
        .map_or_else(
            || "default".to_owned(),
            |name| name.split_once('.').map_or(name, |(_, s)| s).to_owned(),
        )
}

/// Every Chromium store for `browser` (one per profile), each with its label.
fn all_chromium(
    roots: &Roots,
    browser: Browser,
) -> Result<Vec<(String, StorePath)>, WristbandError> {
    let browser_root = chromium_browser_root(roots, browser)
        .ok_or_else(|| WristbandError::NoStore(format!("{browser:?}")))?;
    if !browser_root.is_dir() {
        return Err(WristbandError::NoStore(format!("{browser:?}")));
    }
    let candidates = collect_named(&browser_root, "Cookies", 3);
    if candidates.is_empty() {
        return Err(WristbandError::NoStore(format!("{browser:?}")));
    }
    Ok(candidates
        .into_iter()
        .map(|db| {
            let label = chromium_profile_label(&db, &browser_root);
            (label, StorePath::ChromiumSqlite(db, browser_root.clone()))
        })
        .collect())
}

/// Every Firefox profile store (one per profile dir with a `cookies.sqlite`).
fn all_firefox(roots: &Roots) -> Result<Vec<(String, StorePath)>, WristbandError> {
    let dbs = find_firefox_dbs(&roots.firefox_bases);
    if dbs.is_empty() {
        return Err(WristbandError::NoStore("Firefox".to_owned()));
    }
    Ok(dbs
        .into_iter()
        .filter_map(|db| {
            let dir = db.parent()?.to_path_buf();
            let label = firefox_profile_label(&dir);
            Some((label, StorePath::FirefoxSqlite(dir)))
        })
        .collect())
}

/// Locate **every** store for `browser`, one per profile, each tagged with a
/// profile label. `profile = Some(p)` restricts to that single profile.
///
/// Stores are not collapsed to the newest — the caller decides which to use.
pub(crate) fn locate_all_stores(
    browser: Browser,
    profile: Option<&str>,
) -> Result<Vec<(String, StorePath)>, WristbandError> {
    locate_all_stores_in(&real_roots(), browser, profile)
}

/// Testable core of [`locate_all_stores`] with injected [`Roots`].
pub(crate) fn locate_all_stores_in(
    roots: &Roots,
    browser: Browser,
    profile: Option<&str>,
) -> Result<Vec<(String, StorePath)>, WristbandError> {
    if let Some(p) = profile {
        return Ok(vec![(
            p.to_owned(),
            locate_store_in(roots, browser, Some(p))?,
        )]);
    }
    match browser {
        Browser::Firefox => all_firefox(roots),
        // Safari has a single store (no profiles).
        Browser::Safari => all_safari(roots),
        chromium => all_chromium(roots, chromium),
    }
}

// ---------------------------------------------------------------------------
// Safari discovery
// ---------------------------------------------------------------------------

/// Locate the Safari `Cookies.binarycookies` file.
///
/// Checks the standard location first, then the sandboxed container path.
/// Safari is macOS-only; this function always fails with [`WristbandError::Unsupported`]
/// on other platforms.
/// Every Safari cookie store: the legacy (pre-sandbox) path, the sandboxed
/// container default store, and each per-profile `WebKit` `WebsiteDataStore`
/// (Safari 17+ introduced profiles, each with its own cookie store).
///
/// Label is `"legacy"`/`"default"` for the shared stores, or the shortened
/// `WebsiteDataStore` UUID for a profile store.
fn all_safari(roots: &Roots) -> Result<Vec<(String, StorePath)>, WristbandError> {
    let home = roots
        .home
        .as_deref()
        .ok_or_else(|| WristbandError::NoStore("Safari".to_owned()))?;

    let mut out: Vec<(String, StorePath)> = Vec::new();

    // Shared stores: legacy (pre-sandbox) and the sandboxed container default.
    for (label, rel) in [
        ("legacy", "Library/Cookies/Cookies.binarycookies"),
        (
            "default",
            "Library/Containers/com.apple.Safari/Data/Library/Cookies/Cookies.binarycookies",
        ),
    ] {
        let p = home.join(rel);
        if p.is_file() {
            out.push((label.to_owned(), StorePath::SafariBinary(p)));
        }
    }

    // Per-profile WebKit data stores (Safari 17+ profiles): one Cookies file
    // per `WebsiteDataStore/<uuid>/Cookies/Cookies.binarycookies`.
    let wds = home.join("Library/Containers/com.apple.Safari/Data/Library/WebKit/WebsiteDataStore");
    if let Ok(entries) = std::fs::read_dir(&wds) {
        for entry in entries.flatten() {
            let db = entry.path().join("Cookies/Cookies.binarycookies");
            if db.is_file() {
                // Shorten the UUID for display, e.g. "e3e988c7".
                let raw = entry.file_name().to_string_lossy().into_owned();
                let label = raw.split('-').next().unwrap_or(&raw).to_owned();
                out.push((label, StorePath::SafariBinary(db)));
            }
        }
    }

    if out.is_empty() {
        return Err(WristbandError::NoStore("Safari".to_owned()));
    }
    Ok(out)
}

/// Locate a single Safari store: the most-recently-modified among all Safari
/// cookie stores (legacy, container, and per-profile `WebsiteDataStore`s).
fn locate_safari(roots: &Roots) -> Result<StorePath, WristbandError> {
    all_safari(roots)?
        .into_iter()
        .max_by_key(|(_, sp)| match sp {
            StorePath::SafariBinary(p) => mtime(p),
            _ => SystemTime::UNIX_EPOCH,
        })
        .map(|(_, sp)| sp)
        .ok_or_else(|| WristbandError::NoStore("Safari".to_owned()))
}

// ---------------------------------------------------------------------------
// Real-root resolution (platform-specific)
// ---------------------------------------------------------------------------

/// Build the platform-appropriate [`Roots`] using real OS directories.
///
/// Uses the `dirs` crate for cross-platform home / config / data dir lookup.
/// Platform variants select the correct Firefox base directories.
#[cfg(target_os = "macos")]
fn real_roots() -> Roots {
    let home = dirs::home_dir();
    let app_support = dirs::data_local_dir(); // ~/Library/Application Support on macOS

    let firefox_bases: Vec<PathBuf> = app_support
        .map(|d| vec![d.join("Firefox")])
        .unwrap_or_default();

    // On macOS there is no roaming/local distinction; both fields are the same.
    let chromium_base = dirs::data_local_dir(); // ~/Library/Application Support
    Roots {
        firefox_bases,
        chromium_base_roaming: chromium_base.clone(),
        chromium_base,
        home,
    }
}

#[cfg(target_os = "linux")]
fn real_roots() -> Roots {
    let home = dirs::home_dir();
    let config = dirs::config_dir(); // ~/.config
    let data = dirs::data_dir(); // ~/.local/share

    // Firefox on Linux: XDG config + ~/.mozilla/firefox + Flatpak + Snap.
    let mut firefox_bases: Vec<PathBuf> = Vec::new();
    if let Some(ref h) = home {
        firefox_bases.push(h.join(".mozilla/firefox"));
    }
    if let Some(ref c) = config {
        firefox_bases.push(c.join("firefox"));
    }
    // Flatpak
    if let Some(ref h) = home {
        firefox_bases.push(h.join(".var/app/org.mozilla.firefox/.mozilla/firefox"));
    }
    // Snap
    if let Some(ref h) = home {
        firefox_bases.push(h.join("snap/firefox/current/.mozilla/firefox"));
    }
    let _ = data;

    // On Linux there is no roaming/local distinction; both fields are the same.
    Roots {
        firefox_bases,
        chromium_base_roaming: config.clone(),
        chromium_base: config, // ~/.config/<browser-slug>
        home,
    }
}

#[cfg(target_os = "windows")]
fn real_roots() -> Roots {
    let home = dirs::home_dir();

    // Firefox on Windows: Roaming AppData + MS Store sandbox.
    let mut firefox_bases: Vec<PathBuf> = Vec::new();
    if let Some(roaming) = dirs::data_dir() {
        firefox_bases.push(roaming.join("Mozilla").join("Firefox"));
    }
    // MS Store sandboxed path (approximate; varies by Windows version).
    if let Some(ref h) = home {
        firefox_bases.push(
            h.join("AppData/Local/Packages/Mozilla.Firefox_n80bbvh6b1sto/LocalCache/Roaming/Mozilla/Firefox"),
        );
    }

    // Most Chromium browsers live under %LOCALAPPDATA%; Opera is the exception
    // and uses %APPDATA% (roaming).
    let chromium_base = dirs::data_local_dir(); // %LOCALAPPDATA%
    let chromium_base_roaming = dirs::data_dir(); // %APPDATA%

    Roots {
        firefox_bases,
        chromium_base,
        chromium_base_roaming,
        home,
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn real_roots() -> Roots {
    let chromium_base = dirs::config_dir();
    Roots {
        firefox_bases: Vec::new(),
        chromium_base_roaming: chromium_base.clone(),
        chromium_base,
        home: dirs::home_dir(),
    }
}

// ---------------------------------------------------------------------------
// Public(crate) entry points
// ---------------------------------------------------------------------------

/// Locate the cookie store for `browser`, using real OS base directories.
///
/// This is the production entry point. Tests should use [`locate_store_in`]
/// with an injected [`Roots`] to avoid touching `$HOME`.
///
/// # Errors
///
/// - [`WristbandError::NoStore`] — no cookie store could be found for the
///   requested browser on the current OS.
/// - [`WristbandError::Unsupported`] — the browser/OS combination is not
///   supported (e.g. Safari on Linux).
pub(crate) fn locate_store(
    browser: Browser,
    profile: Option<&str>,
) -> Result<StorePath, WristbandError> {
    locate_store_in(&real_roots(), browser, profile)
}

/// Locate the cookie store for `browser`, using the provided [`Roots`].
///
/// This overload exists for testing: pass a [`Roots`] pointing at a
/// `tempfile::TempDir` tree and no real filesystem paths are consulted.
///
/// # Errors
///
/// - [`WristbandError::NoStore`] — no cookie store was found.
/// - [`WristbandError::Unsupported`] — browser/OS combination not yet
///   supported.
pub(crate) fn locate_store_in(
    roots: &Roots,
    browser: Browser,
    profile: Option<&str>,
) -> Result<StorePath, WristbandError> {
    match browser {
        Browser::Firefox => locate_firefox(roots, profile),
        Browser::Safari => {
            // Safari is macOS-only; the reader arm (Task 12) will enforce this
            // at compile time. Here we just attempt discovery and fall back.
            locate_safari(roots)
        }
        chromium => locate_chromium(roots, chromium, profile),
    }
}

/// The on-disk file to read a browser's version from, beside the stores this
/// module already locates. Chromium-family: `<user-data-root>/Last Version`.
/// Firefox: `<newest-profile>/compatibility.ini`. Safari: the app bundle's
/// `Info.plist` (macOS only; Safari is SIP-installed under `/Applications`).
/// Returns `None` when the file does not exist.
pub(crate) fn version_source(browser: Browser, profile: Option<&str>) -> Option<PathBuf> {
    let roots = real_roots();
    match browser {
        Browser::Firefox => {
            let dbs = find_firefox_dbs(&roots.firefox_bases);
            // Prefer an exact profile-name match, else newest by mtime.
            let chosen = profile
                .and_then(|p| {
                    dbs.iter()
                        .find(|db| {
                            db.parent()
                                .and_then(|d| d.file_name())
                                .and_then(|n| n.to_str())
                                == Some(p)
                        })
                        .cloned()
                })
                .or_else(|| newest(dbs.clone()));
            let dir = chosen?.parent()?.to_path_buf();
            let ini = dir.join("compatibility.ini");
            ini.is_file().then_some(ini)
        }
        Browser::Safari => {
            // Safari is macOS-only and SIP-installed; its version lives in the
            // app bundle's (XML) Info.plist. No version source on other OSes.
            #[cfg(target_os = "macos")]
            {
                let p = PathBuf::from("/Applications/Safari.app/Contents/Info.plist");
                p.is_file().then_some(p)
            }
            #[cfg(not(target_os = "macos"))]
            {
                None
            }
        }
        _ => {
            let f = chromium_browser_root(&roots, browser)?.join("Last Version");
            f.is_file().then_some(f)
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    // -----------------------------------------------------------------------
    // Helper: touch a file with a specific mtime offset
    // -----------------------------------------------------------------------

    /// Create a file at `path` (creating parent dirs as needed).
    fn touch(path: &Path) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create_dir_all");
        }
        fs::write(path, b"").expect("touch");
    }

    // -----------------------------------------------------------------------
    // Firefox discovery tests
    // -----------------------------------------------------------------------

    /// Layout: `<base>/Profiles/alpha.default/cookies.sqlite`
    ///          `<base>/Profiles/beta.release/cookies.sqlite`  ← newest
    ///
    /// Expected: `locate_store_in` returns `FirefoxSqlite(<base>/Profiles/beta.release)`.
    #[test]
    fn firefox_picks_newest_profile_in_profiles_dir() {
        let tmp = TempDir::new().unwrap();
        let base = tmp.path().to_path_buf();

        // Create alpha first (will be older).
        let alpha = base.join("Profiles/alpha.default/cookies.sqlite");
        touch(&alpha);

        // Small sleep so the second file gets a strictly newer mtime.
        std::thread::sleep(std::time::Duration::from_millis(10));

        // Create beta second (will be newer).
        let beta = base.join("Profiles/beta.release/cookies.sqlite");
        touch(&beta);

        let roots = Roots {
            firefox_bases: vec![base.clone()],
            chromium_base: None,
            chromium_base_roaming: None,
            home: None,
        };

        let result = locate_store_in(&roots, Browser::Firefox, None).unwrap();
        assert_eq!(
            result,
            StorePath::FirefoxSqlite(base.join("Profiles/beta.release")),
            "should pick the newest profile dir"
        );
    }

    /// Layout: `<base>/alpha.default/cookies.sqlite`  ← newest
    ///          `<base>/beta.release/cookies.sqlite`
    ///
    /// (Files directly one level under base — no `Profiles/` subdir.)
    #[test]
    fn firefox_picks_newest_one_level_deep() {
        let tmp = TempDir::new().unwrap();
        let base = tmp.path().to_path_buf();

        let older = base.join("beta.release/cookies.sqlite");
        touch(&older);

        std::thread::sleep(std::time::Duration::from_millis(10));

        let newer = base.join("alpha.default/cookies.sqlite");
        touch(&newer);

        let roots = Roots {
            firefox_bases: vec![base.clone()],
            chromium_base: None,
            chromium_base_roaming: None,
            home: None,
        };

        let result = locate_store_in(&roots, Browser::Firefox, None).unwrap();
        assert_eq!(result, StorePath::FirefoxSqlite(base.join("alpha.default")),);
    }

    /// Layout: multiple base dirs; only the second has cookies.sqlite.
    #[test]
    fn firefox_searches_multiple_bases() {
        let tmp = TempDir::new().unwrap();
        let base1 = tmp.path().join("empty_base");
        let base2 = tmp.path().join("real_base");

        let db = base2.join("Profiles/main.default/cookies.sqlite");
        touch(&db);

        let roots = Roots {
            firefox_bases: vec![base1, base2.clone()],
            chromium_base: None,
            chromium_base_roaming: None,
            home: None,
        };

        let result = locate_store_in(&roots, Browser::Firefox, None).unwrap();
        assert_eq!(
            result,
            StorePath::FirefoxSqlite(base2.join("Profiles/main.default")),
        );
    }

    /// `locate_all_stores_in` returns EVERY Chromium profile, not just the newest.
    #[test]
    fn locate_all_stores_returns_every_chromium_profile() {
        let tmp = TempDir::new().unwrap();
        let cbase = tmp.path().to_path_buf();
        let root = cbase.join("Google/Chrome");
        touch(&root.join("Default/Cookies"));
        touch(&root.join("Profile 1/Cookies"));

        let roots = Roots {
            firefox_bases: vec![],
            chromium_base: Some(cbase.clone()),
            chromium_base_roaming: Some(cbase),
            home: None,
        };

        let mut labels: Vec<String> = locate_all_stores_in(&roots, Browser::Chrome, None)
            .unwrap()
            .into_iter()
            .map(|(label, _)| label)
            .collect();
        labels.sort();
        assert_eq!(labels, vec!["Default".to_string(), "Profile 1".to_string()]);
    }

    /// `locate_all_stores_in` returns EVERY Firefox profile, labelled by name.
    #[test]
    fn locate_all_stores_returns_every_firefox_profile() {
        let tmp = TempDir::new().unwrap();
        let base = tmp.path().to_path_buf();
        touch(&base.join("aaa.default-release/cookies.sqlite"));
        touch(&base.join("bbb.work/cookies.sqlite"));

        let roots = Roots {
            firefox_bases: vec![base],
            chromium_base: None,
            chromium_base_roaming: None,
            home: None,
        };

        let mut labels: Vec<String> = locate_all_stores_in(&roots, Browser::Firefox, None)
            .unwrap()
            .into_iter()
            .map(|(label, _)| label)
            .collect();
        labels.sort();
        assert_eq!(
            labels,
            vec!["default-release".to_string(), "work".to_string()]
        );
    }

    /// `locate_all_stores_in` for Safari returns the container store AND each
    /// per-profile `WebKit` `WebsiteDataStore` (Safari 17+ profiles).
    #[test]
    fn locate_all_stores_returns_safari_profiles() {
        let tmp = TempDir::new().unwrap();
        let home = tmp.path().to_path_buf();
        let safari = home.join("Library/Containers/com.apple.Safari/Data/Library");
        // Container default store.
        touch(&safari.join("Cookies/Cookies.binarycookies"));
        // Two per-profile WebsiteDataStore cookie files.
        let wds = safari.join("WebKit/WebsiteDataStore");
        touch(&wds.join("e3e988c7-eb3b-47b7-b028-5aaa20db1609/Cookies/Cookies.binarycookies"));
        touch(&wds.join("58b3ecaf-b747-4b25-8655-994a85e4df71/Cookies/Cookies.binarycookies"));

        let roots = Roots {
            firefox_bases: vec![],
            chromium_base: None,
            chromium_base_roaming: None,
            home: Some(home),
        };

        let mut labels: Vec<String> = locate_all_stores_in(&roots, Browser::Safari, None)
            .unwrap()
            .into_iter()
            .map(|(label, _)| label)
            .collect();
        labels.sort();
        assert_eq!(
            labels,
            vec![
                "58b3ecaf".to_string(),
                "default".to_string(),
                "e3e988c7".to_string()
            ]
        );
    }

    /// No `cookies.sqlite` anywhere → `NoStore` error.
    #[test]
    fn firefox_returns_no_store_when_nothing_found() {
        let tmp = TempDir::new().unwrap();
        let base = tmp.path().to_path_buf();
        // Create a directory but no cookies.sqlite.
        fs::create_dir_all(base.join("Profiles/some.profile")).unwrap();

        let roots = Roots {
            firefox_bases: vec![base],
            chromium_base: None,
            chromium_base_roaming: None,
            home: None,
        };

        let err = locate_store_in(&roots, Browser::Firefox, None).unwrap_err();
        assert!(
            matches!(err, WristbandError::NoStore(ref name) if name == "Firefox"),
            "expected NoStore(Firefox), got {err:?}"
        );
    }

    /// Profile name exact match: when two profiles exist, the named one is
    /// chosen regardless of mtime.
    #[test]
    fn firefox_exact_profile_name_overrides_mtime() {
        let tmp = TempDir::new().unwrap();
        let base = tmp.path().to_path_buf();

        // Create "target" first (will be older by mtime).
        let target = base.join("Profiles/target.profile/cookies.sqlite");
        touch(&target);

        std::thread::sleep(std::time::Duration::from_millis(10));

        // Create "other" second (newer by mtime) — without the exact-match override
        // the mtime-based logic would pick this one.
        let other = base.join("Profiles/other.profile/cookies.sqlite");
        touch(&other);

        let roots = Roots {
            firefox_bases: vec![base.clone()],
            chromium_base: None,
            chromium_base_roaming: None,
            home: None,
        };

        // Request "target.profile" by name — it should win over "other.profile".
        let result = locate_store_in(&roots, Browser::Firefox, Some("target.profile")).unwrap();
        assert_eq!(
            result,
            StorePath::FirefoxSqlite(base.join("Profiles/target.profile")),
        );
    }

    // -----------------------------------------------------------------------
    // Chromium discovery tests
    // -----------------------------------------------------------------------

    /// Layout: `<chromium_base>/Google/Chrome/Default/Cookies`  (older)
    ///          `<chromium_base>/Google/Chrome/Profile 1/Cookies` (newer)
    ///
    /// Expected: returns `Profile 1/Cookies` as the DB, browser root is
    /// `<chromium_base>/Google/Chrome`.
    #[test]
    fn chromium_picks_newest_cookies_file() {
        let tmp = TempDir::new().unwrap();
        let chromium_base = tmp.path().to_path_buf();

        let older = chromium_base.join("Google/Chrome/Default/Cookies");
        touch(&older);

        std::thread::sleep(std::time::Duration::from_millis(10));

        let newer = chromium_base.join("Google/Chrome/Profile 1/Cookies");
        touch(&newer);

        let roots = Roots {
            firefox_bases: vec![],
            chromium_base: Some(chromium_base.clone()),
            chromium_base_roaming: None,
            home: None,
        };

        let result = locate_store_in(&roots, Browser::Chrome, None).unwrap();
        let browser_root = chromium_base.join("Google/Chrome");
        assert_eq!(
            result,
            StorePath::ChromiumSqlite(newer, browser_root),
            "should pick the newest Cookies file"
        );
    }

    /// Profile name override: picks the named sub-dir even if a newer sibling exists.
    #[test]
    fn chromium_named_profile_overrides_mtime() {
        let tmp = TempDir::new().unwrap();
        let chromium_base = tmp.path().to_path_buf();

        let target = chromium_base.join("Google/Chrome/Default/Cookies");
        touch(&target);

        std::thread::sleep(std::time::Duration::from_millis(10));

        let newer = chromium_base.join("Google/Chrome/Profile 1/Cookies");
        touch(&newer);

        let roots = Roots {
            firefox_bases: vec![],
            chromium_base: Some(chromium_base.clone()),
            chromium_base_roaming: None,
            home: None,
        };

        let result = locate_store_in(&roots, Browser::Chrome, Some("Default")).unwrap();
        let browser_root = chromium_base.join("Google/Chrome");
        assert_eq!(result, StorePath::ChromiumSqlite(target, browser_root),);
    }

    /// No browser root dir → `NoStore`.
    #[test]
    fn chromium_missing_browser_dir_returns_no_store() {
        let tmp = TempDir::new().unwrap();
        let roots = Roots {
            firefox_bases: vec![],
            chromium_base: Some(tmp.path().to_path_buf()),
            chromium_base_roaming: None,
            home: None,
        };
        let err = locate_store_in(&roots, Browser::Chrome, None).unwrap_err();
        assert!(matches!(err, WristbandError::NoStore(_)));
    }

    /// No `Cookies` file in the browser dir → `NoStore`.
    #[test]
    fn chromium_missing_cookies_file_returns_no_store() {
        let tmp = TempDir::new().unwrap();
        let chromium_base = tmp.path().to_path_buf();
        // Create the browser dir but no Cookies file.
        fs::create_dir_all(chromium_base.join("Google/Chrome/Default")).unwrap();

        let roots = Roots {
            firefox_bases: vec![],
            chromium_base: Some(chromium_base),
            chromium_base_roaming: None,
            home: None,
        };
        let err = locate_store_in(&roots, Browser::Chrome, None).unwrap_err();
        assert!(matches!(err, WristbandError::NoStore(_)));
    }

    // -----------------------------------------------------------------------
    // Safari discovery tests
    // -----------------------------------------------------------------------

    /// Standard location exists → returned.
    #[test]
    fn safari_finds_standard_location() {
        let tmp = TempDir::new().unwrap();
        let home = tmp.path().to_path_buf();

        let standard = home.join("Library/Cookies/Cookies.binarycookies");
        touch(&standard);

        let roots = Roots {
            firefox_bases: vec![],
            chromium_base: None,
            chromium_base_roaming: None,
            home: Some(home.clone()),
        };
        let result = locate_store_in(&roots, Browser::Safari, None).unwrap();
        assert_eq!(result, StorePath::SafariBinary(standard));
    }

    /// Standard missing, sandboxed present → sandboxed returned.
    #[test]
    fn safari_falls_back_to_sandboxed_location() {
        let tmp = TempDir::new().unwrap();
        let home = tmp.path().to_path_buf();

        let sandboxed = home
            .join("Library/Containers/com.apple.Safari/Data/Library/Cookies/Cookies.binarycookies");
        touch(&sandboxed);

        let roots = Roots {
            firefox_bases: vec![],
            chromium_base: None,
            chromium_base_roaming: None,
            home: Some(home),
        };
        let result = locate_store_in(&roots, Browser::Safari, None).unwrap();
        assert_eq!(result, StorePath::SafariBinary(sandboxed));
    }

    /// Neither location → `NoStore`.
    #[test]
    fn safari_returns_no_store_when_absent() {
        let tmp = TempDir::new().unwrap();
        let roots = Roots {
            firefox_bases: vec![],
            chromium_base: None,
            chromium_base_roaming: None,
            home: Some(tmp.path().to_path_buf()),
        };
        let err = locate_store_in(&roots, Browser::Safari, None).unwrap_err();
        assert!(matches!(err, WristbandError::NoStore(ref n) if n == "Safari"));
    }

    // -----------------------------------------------------------------------
    // Opera roaming-base selection test
    // -----------------------------------------------------------------------

    /// On Windows, Opera stores data under `%APPDATA%` (roaming), while every
    /// other Chromium-family browser uses `%LOCALAPPDATA%` (local).  This test
    /// verifies the selection logic by injecting two distinct base directories
    /// into `Roots` and checking that `locate_store_in` routes Opera to the
    /// roaming base and Chrome to the local base.
    ///
    /// The test is platform-independent: it exercises the selection code, not
    /// the real Windows `%APPDATA%` path.
    #[test]
    fn opera_uses_roaming_base_other_chromium_uses_local_base() {
        let tmp = TempDir::new().unwrap();

        // Two distinct sub-trees: one representing %LOCALAPPDATA% (local),
        // the other %APPDATA% (roaming).
        let local_base = tmp.path().join("local");
        let roaming_base = tmp.path().join("roaming");

        // Chrome Cookies lives ONLY under the local base.
        let chrome_cookies = local_base.join("Google/Chrome/Default/Cookies");
        touch(&chrome_cookies);

        // Opera Cookies lives ONLY under the roaming base.
        let opera_cookies = roaming_base.join("Opera Software/Opera Stable/Default/Cookies");
        touch(&opera_cookies);

        let roots = Roots {
            firefox_bases: vec![],
            chromium_base: Some(local_base.clone()),
            chromium_base_roaming: Some(roaming_base.clone()),
            home: None,
        };

        // Chrome should find its Cookies via the local base.
        let chrome_result = locate_store_in(&roots, Browser::Chrome, None).unwrap();
        let expected_chrome_root = local_base.join("Google/Chrome");
        assert_eq!(
            chrome_result,
            StorePath::ChromiumSqlite(chrome_cookies, expected_chrome_root),
            "Chrome should resolve under the local base"
        );

        // Opera should find its Cookies via the roaming base.
        let opera_result = locate_store_in(&roots, Browser::Opera, None).unwrap();
        let expected_opera_root = roaming_base.join("Opera Software/Opera Stable");
        assert_eq!(
            opera_result,
            StorePath::ChromiumSqlite(opera_cookies, expected_opera_root),
            "Opera should resolve under the roaming base"
        );
    }
}
