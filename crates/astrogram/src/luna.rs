//! LUNA® Astrology web-application extractor and writer.
//!
//! This module contains **only pure parsing and conversion** — no network I/O.
//! The HTTP orchestration (session cookie, pagination, per-chart fetching/writing)
//! lives in the `blackmoon` CLI binary.
//!
//! ## Read: three-step parse
//!
//! 1. [`parse_listing_page`] — `/phenomena?limit=100&page=N` HTML → [`ListingRow`] vec
//! 2. [`parse_cast_json`] — `/charts/cast.json?uniwheel=UUID` → [`CastMeta`]
//! 3. [`parse_sidebar`] — `/radix-charts/view?uniwheel=UUID` HTML → [`SidebarMeta`]
//!
//! Combine with [`luna_chart_to_chart`] to produce a canonical [`Chart`].
//!
//! ## Write: form-POST helpers
//!
//! 1. GET `/phenomena/add` → [`parse_form_tokens`] → [`FormTokens`]
//! 2. [`create_payload`] — build POST body from [`Chart`] + [`FormTokens`]
//! 3. POST to `/phenomena/add` (caller's responsibility)
//! 4. [`extract_phenom_id`] — pull phenomenon UUID from redirect URL / response HTML

use crate::capability::{CapabilitySet, ChartField};
use crate::chart::{Chart, CoordinateSystem, EventType, HouseSystem, Latitude, Longitude, Zodiac};
use crate::normalize::normalize_cp1252_str;
use reqwest::blocking::Client;
use scraper::{Html, Selector};
use std::time::Duration;
use thiserror::Error;

/// Fields recovered when reading a LUNA® Astrology account chart.
///
/// LUNA does not expose region as a separate field — the location string is
/// split on the first comma and only the city portion is placed in `Chart.city`.
/// The Rodden rating is recovered from the sidebar. Event type covers
/// `event` and `horary`; natal charts always become [`crate::chart::EventType::Unspecified`]
/// because LUNA does not store sex (M/F).
pub const READ_CAPS: CapabilitySet =
    CapabilitySet::new(&[ChartField::SourceRating, ChartField::EventType]);

/// Fields persisted when writing a LUNA® Astrology account chart.
/// Identical to [`READ_CAPS`].
pub const WRITE_CAPS: CapabilitySet = READ_CAPS;

/// Base URL for the LUNA® Astrology web application.
pub const BASE_URL: &str = "https://www.lunaastrology.com";
/// User-Agent string sent with all HTTP requests.
pub const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 \
     (KHTML, like Gecko) Chrome/148.0.0.0 Safari/537.36";

/// Errors specific to LUNA parsing.
#[derive(Debug, Error)]
pub enum LunaError {
    /// `cast.json` JSON is missing the top-level `"uniwheel"` key.
    #[error("JSON missing 'uniwheel' key")]
    MissingUniwheel,
    /// The UTC offset string (e.g. `"UTC+05:30"`) could not be parsed.
    #[error("invalid UTC offset: {0:?}")]
    InvalidOffset(String),
    /// A date or time string could not be parsed.
    #[error("invalid date or time: {0:?}")]
    InvalidDateTime(String),
    /// A latitude or longitude value is outside the valid range.
    #[error("invalid coordinate")]
    InvalidCoordinate,
    /// Underlying JSON parse failure.
    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),
    /// An HTTP request failed.
    #[error("HTTP: {0}")]
    Http(#[from] reqwest::Error),
    /// The `reqwest` client could not be constructed.
    #[error("HTTP client build error: {0}")]
    HttpClientBuild(String),
    /// A `CakePHP` form-token block was not found in the page.
    #[error("{0} form tokens not found in page")]
    FormTokensNotFound(String),
    /// The phenomenon UUID could not be extracted from the create response.
    #[error("phenomenon ID not found in create response")]
    PhenomIdNotFound,
}

impl LunaError {
    /// True when this error means the credential was rejected and the caller
    /// should fall through to the next credential in the chain.
    ///
    /// `FormTokensNotFound` means the listing/edit page rendered without the
    /// authenticated form tokens — the session cookie is no longer valid.
    /// HTTP 401/403 is delegated to [`crate::web_auth::is_unauthorized`].
    #[must_use]
    pub fn is_auth_failure(&self) -> bool {
        match self {
            Self::Http(e) => crate::web_auth::is_unauthorized(e),
            Self::FormTokensNotFound(_) => true,
            Self::MissingUniwheel
            | Self::InvalidOffset(_)
            | Self::InvalidDateTime(_)
            | Self::InvalidCoordinate
            | Self::Json(_)
            | Self::HttpClientBuild(_)
            | Self::PhenomIdNotFound => false,
        }
    }
}

/// One row from the `/phenomena` chart-listing page.
#[derive(Debug, Clone)]
pub struct ListingRow {
    /// The `chart_id` (uniwheel UUID) — session-scoped, not a stable key.
    pub chart_id: String,
    /// Chart name as shown in the listing (may be truncated with `…` at ~33 chars).
    pub name: String,
    /// `"natal"`, `"event"`, or `"horary"`.
    pub chart_type: String,
    /// Birth year (negative = BCE).
    pub year: i16,
    /// Birth month (1–12).
    pub month: u8,
    /// Birth day (1–31).
    pub day: u8,
    /// Birth hour (0–23).
    pub hour: u8,
    /// Birth minute (0–59).
    pub minute: u8,
    /// Birth second (0–59).
    pub second: u8,
}

/// Birth metadata from the `/charts/cast.json` endpoint.
#[derive(Debug, Clone)]
pub struct CastMeta {
    /// `"yyyy-mm-dd"`.
    pub date: String,
    /// `"HH:MM:SS"`.
    pub time: String,
    /// Latitude in decimal degrees (North positive).
    pub lat: f64,
    /// Longitude in decimal degrees (East positive).
    pub lon: f64,
    /// `"UTC±HH:MM:SS"`.
    pub offset_str: String,
    /// Zodiac system display name (e.g. `"Tropical"`).
    pub zodiac: String,
    /// Location string as returned by LUNA (e.g. `"London, England"`).
    pub location: String,
}

/// Metadata scraped from the `/radix-charts/view` sidebar.
#[derive(Debug, Clone)]
pub struct SidebarMeta {
    /// Display name as shown in the sidebar (e.g. `"Whole Sign"`, `"Placidus"`).
    pub house_system: String,
    /// Zodiac system display name (e.g. `"Tropical"`).
    pub zodiac: String,
    /// IANA timezone abbreviation from the "Timezone" row (e.g. `"LMT"`, `"EST"`).
    pub tz_abbrev: String,
    /// `true` when the timezone abbreviation contains `"LMT"`.
    pub is_lmt: bool,
    /// Rodden rating code (e.g. `"AA"`, `"X"`).
    pub rodden_code: String,
    /// Rodden rating description (e.g. `"Birth Certificate"`).
    pub rodden_desc: String,
    /// Phenomenon UUID from the `/phenomena/edit/<uuid>` link in the sidebar.
    /// `None` when viewing a chart owned by another user (no edit link rendered).
    pub phenom_id: Option<String>,
}

/// Fully-hydrated LUNA chart (assembled from JSON + sidebar).
#[derive(Debug, Clone)]
pub struct LunaChart {
    /// The `chart_id` (uniwheel UUID).
    pub chart_id: String,
    /// Chart name as displayed in LUNA.
    pub name: String,
    /// `"natal"`, `"event"`, or `"horary"`.
    pub chart_type: String,
    /// `"yyyy-mm-dd"`.
    pub date: String,
    /// `"HH:MM:SS"`.
    pub time: String,
    /// Latitude in decimal degrees (North positive).
    pub lat: f64,
    /// Longitude in decimal degrees (East positive).
    pub lon: f64,
    /// `"UTC±HH:MM:SS"`.
    pub offset_str: String,
    /// Location string as returned by LUNA (e.g. `"London, England"`).
    pub location: String,
    /// Zodiac system display name.
    pub zodiac: String,
    /// House system display name.
    pub house_system: String,
    /// Timezone abbreviation (e.g. `"LMT"`, `"EST"`).
    pub tz_abbrev: String,
    /// `true` when `tz_abbrev` contains `"LMT"`.
    pub is_lmt: bool,
    /// Rodden rating code (e.g. `"AA"`).
    pub rodden_code: String,
    /// Rodden rating description.
    pub rodden_desc: String,
    /// Optional free-text notes.
    pub notes: String,
}

// --- listing parser ---

/// Parse the `/phenomena?limit=N&page=P` HTML page into chart-listing rows.
///
/// Rows with missing UUID or datetime are silently skipped.
///
/// # Panics
///
/// Panics if the static CSS selectors fail to compile (impossible at runtime).
#[must_use]
pub fn parse_listing_page(html: &str) -> Vec<ListingRow> {
    let doc = Html::parse_document(html);
    let sel_row = Selector::parse("tr[data-chart-url]").unwrap();
    let sel_name = Selector::parse("a.font-lg").unwrap();
    let sel_badge = Selector::parse(".badge").unwrap();
    let sel_td = Selector::parse("td[data-sort]").unwrap();

    let mut rows = Vec::new();
    for row in doc.select(&sel_row) {
        let chart_url = row.value().attr("data-chart-url").unwrap_or("");
        let Some(chart_id) = extract_uuid(chart_url) else {
            continue;
        };

        let name = row
            .select(&sel_name)
            .next()
            .map(|e| e.text().collect::<String>().trim().to_string())
            .unwrap_or_default();

        let chart_type = row.select(&sel_badge).next().map_or_else(
            || "natal".to_string(),
            |e| e.text().collect::<String>().trim().to_lowercase(),
        );

        let dt = row
            .select(&sel_td)
            .filter_map(|td| td.value().attr("data-sort"))
            .find(|v| looks_like_datetime(v))
            .and_then(parse_listing_datetime);

        let Some((year, month, day, hour, minute, second)) = dt else {
            continue;
        };

        rows.push(ListingRow {
            chart_id,
            name,
            chart_type,
            year,
            month,
            day,
            hour,
            minute,
            second,
        });
    }
    rows
}

fn extract_uuid(s: &str) -> Option<String> {
    let idx = s.find("uniwheel=")?;
    let rest = &s[idx + "uniwheel=".len()..];
    if rest.len() >= 36 {
        Some(rest[..36].to_string())
    } else {
        None
    }
}

fn looks_like_datetime(s: &str) -> bool {
    s.len() >= 19 && s.as_bytes().first().is_some_and(u8::is_ascii_digit) && s.contains('T')
}

fn parse_listing_datetime(s: &str) -> Option<(i16, u8, u8, u8, u8, u8)> {
    // Format: "YYYY-MM-DDTHH:MM:SS±HH:MM" or "YYYY-MM-DDTHH:MM:SS+HH:MM"
    let s = s.get(..19)?; // "YYYY-MM-DDTHH:MM:SS"
    let (date, time) = s.split_once('T')?;
    let date_parts: Vec<&str> = date.split('-').collect();
    let time_parts: Vec<&str> = time.split(':').collect();
    if date_parts.len() < 3 || time_parts.len() < 3 {
        return None;
    }
    let year: i16 = date_parts[0].parse().ok()?;
    let month: u8 = date_parts[1].parse().ok()?;
    let day: u8 = date_parts[2].parse().ok()?;
    let hour: u8 = time_parts[0].parse().ok()?;
    let minute: u8 = time_parts[1].parse().ok()?;
    let second: u8 = time_parts[2].parse().ok()?;
    Some((year, month, day, hour, minute, second))
}

// --- cast.json parser ---

/// Parse the `/charts/cast.json?uniwheel=UUID` JSON response.
///
/// # Errors
///
/// Returns [`LunaError::MissingUniwheel`] if the top-level `"uniwheel"` key is absent.
pub fn parse_cast_json(json: &str) -> Result<CastMeta, LunaError> {
    let v: serde_json::Value = serde_json::from_str(json)?;
    let uw = v.get("uniwheel").ok_or(LunaError::MissingUniwheel)?;

    Ok(CastMeta {
        date: uw["datepicker"].as_str().unwrap_or("").to_string(),
        time: uw["eventTime"].as_str().unwrap_or("").to_string(),
        lat: uw["latitude"].as_f64().unwrap_or(0.0),
        lon: uw["longitude"].as_f64().unwrap_or(0.0),
        offset_str: uw["offset"].as_str().unwrap_or("UTC+00:00:00").to_string(),
        zodiac: uw["zodiac"].as_str().unwrap_or("Tropical").to_string(),
        location: uw["location"].as_str().unwrap_or("").to_string(),
    })
}

// --- sidebar parser ---

/// Parse the `/radix-charts/view?uniwheel=UUID` sidebar for house system, zodiac,
/// Rodden rating, and timezone info.
///
/// Uses a text-line scan (matching the Python oracle) rather than CSS selectors,
/// because the sidebar is not well-structured HTML.
///
/// # Panics
///
/// Panics if the static CSS selector for Rodden rating fails to compile (impossible at runtime).
pub fn parse_sidebar(html: &str) -> SidebarMeta {
    let doc = Html::parse_document(html);

    // Collect all text nodes in document order, trimmed and de-blanked.
    let lines: Vec<String> = doc
        .root_element()
        .text()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .collect();

    let line_refs: Vec<&str> = lines.iter().map(String::as_str).collect();

    let house_system = find_after(&line_refs, "house system")
        .unwrap_or("")
        .to_string();
    let zodiac = find_after(&line_refs, "zodiac").unwrap_or("").to_string();
    let tz_abbrev = find_after(&line_refs, "timezone").unwrap_or("").to_string();
    let is_lmt = tz_abbrev.to_uppercase().contains("LMT");

    // Rodden rating: look for an <a> with href containing "rodden-rating"
    let sel_rodden = Selector::parse(r#"a[href*="rodden-rating"]"#).unwrap();
    let rodden_text = doc
        .select(&sel_rodden)
        .next()
        .map(|e| e.text().collect::<String>().trim().to_string())
        .unwrap_or_default();

    let (rodden_code, rodden_desc) = parse_rodden_text(&rodden_text);

    let phenom_id = extract_phenom_id(html);

    SidebarMeta {
        house_system,
        zodiac,
        tz_abbrev,
        is_lmt,
        rodden_code,
        rodden_desc,
        phenom_id,
    }
}

/// Find the first non-empty line that follows a line matching `label` (case-insensitive).
fn find_after<'a>(lines: &[&'a str], label: &str) -> Option<&'a str> {
    let label = label.to_lowercase();
    for (i, line) in lines.iter().enumerate() {
        if line.to_lowercase() == label {
            let end = (i + 4).min(lines.len());
            for candidate in &lines[i + 1..end] {
                let v = candidate.trim();
                if !v.is_empty() && v.to_lowercase() != label {
                    return Some(candidate);
                }
            }
        }
    }
    None
}

/// Parse `"(B) - Bio/autobiography"` into `("B", "Bio/autobiography")`.
fn parse_rodden_text(s: &str) -> (String, String) {
    let s = s.trim();
    if s.is_empty() {
        return (String::new(), String::new());
    }
    // Expect "(CODE) - description" or "(CODE) description"
    if let Some(close) = s.find(')') {
        let code = s[1..close].trim().to_string();
        let rest = s[close + 1..]
            .trim_start_matches([' ', '-', '\u{2013}', '\u{2014}'])
            .trim()
            .to_string();
        (code, rest)
    } else {
        (s.trim_matches(['(', ')']).to_string(), String::new())
    }
}

// --- public conversion helpers ---

/// Map a LUNA chart-type string to [`EventType`].
///
/// LUNA does not store sex (M/F) — natal charts always become [`EventType::Unspecified`].
#[must_use]
pub fn luna_type_to_event_type(chart_type: &str) -> EventType {
    match chart_type.to_lowercase().as_str() {
        "event" => EventType::Event,
        "horary" => EventType::Horary,
        _ => EventType::Unspecified,
    }
}

/// Map a LUNA house-system display name to [`HouseSystem`].
#[must_use]
pub fn luna_house_system(name: &str) -> HouseSystem {
    match name.to_lowercase().as_str() {
        "campanus" => HouseSystem::Campanus,
        "koch" => HouseSystem::Koch,
        "meridian" => HouseSystem::Meridian,
        "morinus" => HouseSystem::Morinus,
        "placidus" => HouseSystem::Placidus,
        "porphyry" => HouseSystem::Porphyry,
        "regiomontanus" => HouseSystem::Regiomontanus,
        "topocentric" => HouseSystem::Topocentric,
        "equal" | "equal-ac" | "equal ac" => HouseSystem::Equal,
        "whole sign" | "whole-sign" | "whole-sign-equal-houses" => HouseSystem::WholeSign,
        "alcabitus" => HouseSystem::Alcabitius,
        "zero aries" | "0-aries" | "0 aries" => HouseSystem::ZeroAries,
        _ => HouseSystem::Other(0),
    }
}

/// Map a LUNA zodiac display name to [`Zodiac`].
#[must_use]
pub fn luna_zodiac(name: &str) -> Zodiac {
    match name.to_lowercase().as_str() {
        "fagan-bradley" | "fagan bradley" | "fagan/bradley" => Zodiac::FaganAllen,
        "lahiri" => Zodiac::Lahiri,
        "deluce" | "de luce" => Zodiac::DeLuce,
        "raman" => Zodiac::Raman,
        "usha-shashi" | "usha shashi" => Zodiac::UshaShashi,
        "krishnamurti" => Zodiac::Krishnamurti,
        "djwhal-khul" | "djwhal khul" => Zodiac::DjwhalKhul,
        "yukteshwar" | "sri yukteswar" => Zodiac::SriYukteswar,
        _ => Zodiac::Tropical,
    }
}

/// Build a `source_rating` string from a LUNA Rodden code + description.
///
/// Returns `None` if both are empty.
#[must_use]
pub fn map_rodden_rating(code: &str, desc: &str) -> Option<String> {
    let code = code.trim();
    let desc = desc.trim();
    if code.is_empty() && desc.is_empty() {
        return None;
    }
    let s = if !code.is_empty() && !desc.is_empty() {
        format!("{code} {desc}")
    } else {
        code.to_string()
    };
    Some(s[..s.len().min(32)].to_string())
}

// --- UTC offset parser ---

fn parse_offset_str(s: &str) -> Result<f64, LunaError> {
    let rest = s
        .strip_prefix("UTC")
        .ok_or_else(|| LunaError::InvalidOffset(s.to_string()))?;
    if rest.is_empty() {
        return Ok(0.0);
    }
    let sign = if rest.starts_with('-') {
        -1.0_f64
    } else {
        1.0_f64
    };
    let hms = &rest[1..];
    let parts: Vec<&str> = hms.split(':').collect();
    if parts.is_empty() {
        return Err(LunaError::InvalidOffset(s.to_string()));
    }
    let h: f64 = parts.first().and_then(|p| p.parse().ok()).unwrap_or(0.0);
    let m: f64 = parts.get(1).and_then(|p| p.parse().ok()).unwrap_or(0.0);
    let sec: f64 = parts.get(2).and_then(|p| p.parse().ok()).unwrap_or(0.0);
    Ok(sign * (h + m / 60.0 + sec / 3600.0))
}

// --- date/time parsers ---

fn parse_date(s: &str) -> Result<(i16, u8, u8), LunaError> {
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() < 3 {
        return Err(LunaError::InvalidDateTime(s.to_string()));
    }
    let year: i16 = parts[0]
        .parse()
        .map_err(|_| LunaError::InvalidDateTime(s.to_string()))?;
    let month: u8 = parts[1]
        .parse()
        .map_err(|_| LunaError::InvalidDateTime(s.to_string()))?;
    let day: u8 = parts[2]
        .parse()
        .map_err(|_| LunaError::InvalidDateTime(s.to_string()))?;
    Ok((year, month, day))
}

fn parse_time(s: &str) -> (u8, u8, u8) {
    let parts: Vec<&str> = s.split(':').collect();
    let hour: u8 = parts.first().and_then(|p| p.parse().ok()).unwrap_or(0);
    let minute: u8 = parts.get(1).and_then(|p| p.parse().ok()).unwrap_or(0);
    let second: u8 = parts.get(2).and_then(|p| p.parse().ok()).unwrap_or(0);
    (hour, minute, second)
}

// --- main conversion ---

/// Convert a fully-hydrated [`LunaChart`] into the canonical [`Chart`] type.
///
/// The `location` field is split on the first comma; the part before becomes
/// `city`, the rest is discarded (LUNA doesn't expose a separate region field).
///
/// # Errors
///
/// Returns an error for unparseable dates, times, or coordinates.
pub fn luna_chart_to_chart(luna: &LunaChart) -> Result<Chart, LunaError> {
    let (year, month, day) = parse_date(&luna.date)?;
    let (hour, minute, second) = parse_time(&luna.time);
    let tz_offset_hours = parse_offset_str(&luna.offset_str)?;
    let latitude = Latitude::new(luna.lat).map_err(|_| LunaError::InvalidCoordinate)?;
    let longitude = Longitude::new(luna.lon).map_err(|_| LunaError::InvalidCoordinate)?;

    let city = luna
        .location
        .split(',')
        .next()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    let source_rating = map_rodden_rating(&luna.rodden_code, &luna.rodden_desc);
    let notes = if luna.notes.is_empty() {
        None
    } else {
        Some(luna.notes.clone())
    };

    Ok(Chart {
        name: luna.name.clone(),
        secondary_name: None,
        city,
        region: None,
        longitude,
        latitude,
        year,
        month,
        day,
        hour,
        minute,
        second,
        tz_offset_hours,
        tz_abbreviation: if luna.tz_abbrev.is_empty() {
            None
        } else {
            Some(luna.tz_abbrev.clone())
        },
        is_lmt: luna.is_lmt,
        event_type: luna_type_to_event_type(&luna.chart_type),
        source_rating,
        house_system: luna_house_system(&luna.house_system),
        zodiac: luna_zodiac(&luna.zodiac),
        coordinate_system: CoordinateSystem::Geocentric,
        sub_charts: vec![],
        notes,
    })
}

// ── write helpers ─────────────────────────────────────────────────────────────

/// `CakePHP` form security tokens required for all LUNA write operations.
///
/// All three must be scraped fresh from the corresponding GET form page and
/// included in every POST/PUT body.  Omitting any one causes HTTP 400.
#[derive(Debug, Clone)]
pub struct FormTokens {
    /// `_csrfToken` hidden input value.
    pub csrf: String,
    /// `_Token[fields]` hidden input value.
    pub fields: String,
    /// `_Token[unlocked]` hidden input value.
    pub unlocked: String,
}

/// Scrape `CakePHP` security tokens from the form whose `action` contains
/// `action_fragment`.
///
/// Returns `None` when the matching form is not found.
///
/// # Panics
///
/// Panics if the static CSS selectors fail to compile (impossible at runtime).
#[must_use]
pub fn parse_form_tokens(html: &str, action_fragment: &str) -> Option<FormTokens> {
    let doc = Html::parse_document(html);
    let form_sel = Selector::parse("form").expect("valid selector");
    let input_sel = Selector::parse("input").expect("valid selector");

    for form in doc.select(&form_sel) {
        let action = form.value().attr("action").unwrap_or("");
        if !action.contains(action_fragment) {
            continue;
        }
        let mut csrf = String::new();
        let mut fields = String::new();
        let mut unlocked = String::new();
        for input in form.select(&input_sel) {
            let name = input.value().attr("name").unwrap_or("");
            let value = input.value().attr("value").unwrap_or("");
            match name {
                "_csrfToken" => csrf = value.to_string(),
                "_Token[fields]" => fields = value.to_string(),
                "_Token[unlocked]" => unlocked = value.to_string(),
                _ => {}
            }
        }
        return Some(FormTokens {
            csrf,
            fields,
            unlocked,
        });
    }
    None
}

/// Map a [`Chart`]'s [`EventType`] to a LUNA chart type string.
#[must_use]
pub fn chart_type_str(et: EventType) -> &'static str {
    match et {
        EventType::Horary => "horary",
        EventType::Event => "event",
        _ => "natal",
    }
}

/// Map a `source_rating` string (e.g. `"AA"`, `"B Bio/autobiography"`) to a
/// LUNA `chart_source_id` integer.
///
/// Uses the first whitespace-delimited token as the Rodden code.  Returns `99`
/// (Undetermined) for unknown or absent ratings.
#[must_use]
pub fn source_id_for_rating(rating: Option<&str>) -> u32 {
    let code = rating
        .map_or("", |s| {
            s.trim().split_ascii_whitespace().next().unwrap_or("")
        })
        .to_uppercase();
    match code.as_str() {
        "AA" => 1,
        "A" => 3,
        "B" => 5,
        "C" => 6,
        "DD" => 9,
        "X" => 10,
        "XX" => 12,
        _ => 99,
    }
}

/// Build the ordered form-field pairs for `POST /phenomena/add`.
///
/// The caller is responsible for HTTP submission and CSRF token freshness.
/// Coordinates are passed in ISO 6709 sign convention (East positive, North
/// positive), which matches LUNA's convention.
#[must_use]
pub fn create_payload(chart: &Chart, tokens: &FormTokens) -> Vec<(String, String)> {
    let location = match (&chart.city, &chart.region) {
        (Some(c), Some(r)) => format!("{c}, {r}"),
        (Some(c), None) => c.clone(),
        (None, Some(r)) => r.clone(),
        (None, None) => String::new(),
    };
    let date = format!("{:04}-{:02}-{:02}", chart.year, chart.month, chart.day);
    let time = format!("{:02}:{:02}:{:02}", chart.hour, chart.minute, chart.second);
    let name = if chart.name.len() > 100 {
        &chart.name[..100]
    } else {
        &chart.name
    };

    vec![
        ("_csrfToken".to_string(), tokens.csrf.clone()),
        ("_Token[fields]".to_string(), tokens.fields.clone()),
        ("_Token[unlocked]".to_string(), tokens.unlocked.clone()),
        ("name".to_string(), name.to_string()),
        (
            "type".to_string(),
            chart_type_str(chart.event_type).to_string(),
        ),
        ("tags".to_string(), String::new()),
        ("primary_radix_chart[event_date]".to_string(), date),
        ("primary_radix_chart[event_time]".to_string(), time),
        ("primary_radix_chart[location]".to_string(), location),
        (
            "primary_radix_chart[latitude]".to_string(),
            format!("{:.6}", chart.latitude.degrees()),
        ),
        (
            "primary_radix_chart[longitude]".to_string(),
            format!("{:.6}", chart.longitude.degrees()),
        ),
        (
            "primary_radix_chart[chart_source_id]".to_string(),
            source_id_for_rating(chart.source_rating.as_deref()).to_string(),
        ),
    ]
}

/// Build the ordered form-field pairs for `POST /phenomena/edit/<phenom-id>`.
///
/// Identical to [`create_payload`] but prepends `_method=PUT` (`CakePHP` method tunnel).
/// The caller submits this as a `POST` to `/phenomena/edit/<phenom_id>`.
#[must_use]
pub fn edit_payload(chart: &Chart, tokens: &FormTokens) -> Vec<(String, String)> {
    let mut payload = vec![("_method".to_string(), "PUT".to_string())];
    payload.extend(create_payload(chart, tokens));
    payload
}

/// Build the ordered form-field pairs for `POST /phenomena/delete/<phenom-id>`.
///
/// LUNA's delete form uses `_method=POST` (not `DELETE`) — the delete route is
/// reached by `POST`ing to `/phenomena/delete/<uuid>` directly.  The CSRF and token
/// envelope must come from the delete form on the edit page, not the edit form.
#[must_use]
pub fn delete_payload(tokens: &FormTokens) -> Vec<(String, String)> {
    vec![
        ("_method".to_string(), "POST".to_string()),
        ("_csrfToken".to_string(), tokens.csrf.clone()),
        ("_Token[fields]".to_string(), tokens.fields.clone()),
        ("_Token[unlocked]".to_string(), tokens.unlocked.clone()),
    ]
}

/// Extract a LUNA phenomenon UUID from a redirect URL or response HTML body.
///
/// Looks for a path segment matching `/phenomena/edit/<UUID>` or
/// `/phenomena/view/<UUID>`.
#[must_use]
pub fn extract_phenom_id(text: &str) -> Option<String> {
    // UUID pattern: 8-4-4-4-12 hex digits
    let pat = "/phenomena/";
    let mut pos = 0;
    while let Some(idx) = text[pos..].find(pat) {
        let start = pos + idx + pat.len();
        // skip "edit/" or "view/"
        let rest = &text[start..];
        let after_verb = rest.find('/').map_or(rest, |i| &rest[i + 1..]);
        if after_verb.len() >= 36 {
            let candidate = &after_verb[..36];
            if is_uuid(candidate) {
                return Some(candidate.to_string());
            }
        }
        pos = pos + idx + 1;
    }
    None
}

fn is_uuid(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 36
        && b[8] == b'-'
        && b[13] == b'-'
        && b[18] == b'-'
        && b[23] == b'-'
        && b.iter()
            .enumerate()
            .all(|(i, &c)| i == 8 || i == 13 || i == 18 || i == 23 || c.is_ascii_hexdigit())
}

// ── fetch helpers ─────────────────────────────────────────────────────────────

/// Returns `true` when `name` starts with `prefix` (case-insensitive).
/// Used to locate the resume point in a LUNA listing scan.
#[must_use]
pub fn at_resume_point(name: &str, prefix: &str) -> bool {
    name.to_lowercase().starts_with(&prefix.to_lowercase())
}

/// Format the inline per-row status emitted by [`LunaSession::fetch_charts`]
/// after a successful fetch, flagging duplicate **candidates** of a
/// previously-fetched record in the same run.
///
/// `existing` and `listing_indices` are parallel slices: `existing[i]` was
/// fetched at listing position `listing_indices[i]` (1-based, matching the
/// `[N/total]` prefix the user saw on screen).
///
/// Returns `"ok"` when no spacetime match exists, or
/// `"ok  ⚠ candidate of #N"` when one does. The candidate signal is
/// informational — nothing is dropped at fetch time. See
/// [`crate::consolidate`] for the spacetime tolerance rule.
#[must_use]
pub fn candidate_status(
    chart: &crate::chart::Chart,
    existing: &[crate::chart::Chart],
    listing_indices: &[usize],
) -> String {
    match crate::consolidate::find_candidate(chart, existing) {
        Some(idx) => format!("ok  \u{26a0} candidate of #{}", listing_indices[idx]),
        None => "ok".to_string(),
    }
}

/// Returns `true` when a listing-row name requires a full chart fetch during
/// normalize mode: either it is truncated (ends with `…`) or contains chars
/// that `normalize_cp1252_str` would change.
#[must_use]
pub fn needs_fetch_for_normalize(name: &str) -> bool {
    name.ends_with('…') || normalize_cp1252_str(name) != name
}

// ── HTTP session ──────────────────────────────────────────────────────────────

/// Authenticated HTTP session for a LUNA® Astrology account.
///
/// All methods use the provided session cookie and observe the configured
/// inter-request delay.  Progress reporting is handled by caller-supplied
/// closures so the library never writes to stdout/stderr.
pub struct LunaSession {
    client: Client,
    delay: Duration,
}

impl LunaSession {
    /// Build a session from the `LUNAASTROLOGY_COOKIE` env-var value.
    ///
    /// # Errors
    /// - [`LunaError::HttpClientBuild`] if the cookie header value is invalid
    ///   or the `reqwest::Client` cannot be constructed.
    pub fn new(session_cookie: &str, delay_ms: u64) -> Result<Self, LunaError> {
        let cookie = format!("LUNA_ASTROLOGY_APP={session_cookie}");
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::COOKIE,
            reqwest::header::HeaderValue::from_str(&cookie)
                .map_err(|e| LunaError::HttpClientBuild(e.to_string()))?,
        );
        let client = Client::builder()
            .default_headers(headers)
            .user_agent(USER_AGENT)
            .build()
            .map_err(|e| LunaError::HttpClientBuild(e.to_string()))?;
        Ok(Self {
            client,
            delay: Duration::from_millis(delay_ms),
        })
    }

    fn get_text(&self, url: &str) -> Result<String, LunaError> {
        Ok(self.client.get(url).send()?.error_for_status()?.text()?)
    }

    fn sleep(&self) {
        if !self.delay.is_zero() {
            std::thread::sleep(self.delay);
        }
    }

    /// Fetch the paginated chart listing without hydrating individual charts.
    ///
    /// Use this for read-before-write dedup: build a `HashSet` from the
    /// returned rows and filter input before any per-chart HTTP.
    ///
    /// # Errors
    /// - [`LunaError::Http`] if any paginated listing request fails.
    pub fn fetch_listing(&self) -> Result<Vec<ListingRow>, LunaError> {
        let mut listing = Vec::new();
        let mut page = 1u32;
        loop {
            let url = format!("{BASE_URL}/phenomena?limit=100&page={page}");
            let html = self.get_text(&url)?;
            let rows = parse_listing_page(&html);
            let done = rows.len() < 100;
            listing.extend(rows);
            if done {
                break;
            }
            page += 1;
            self.sleep();
        }
        Ok(listing)
    }

    /// Cheap authenticated probe: fetch the chart listing.
    ///
    /// A stale session cookie makes the listing page render without its
    /// authenticated form tokens → [`LunaError::FormTokensNotFound`] (or a 401),
    /// both fall-through signals for the credential chain.
    ///
    /// # Errors
    /// Propagates the listing error; auth failures are classifiable via
    /// [`LunaError::is_auth_failure`].
    pub fn probe(&self) -> Result<(), LunaError> {
        self.fetch_listing().map(|_| ())
    }

    /// Authenticate against an ordered list of candidate session-cookie values,
    /// falling through on auth failure. Returns the live session and the index
    /// of the cookie that authenticated.
    ///
    /// LUNA has no login flow, so each candidate is a session-cookie string
    /// (e.g. a browser import, then `--luna-token`). Fall-through happens only on
    /// [`LunaError::is_auth_failure`].
    ///
    /// # Errors
    /// - The last auth failure if every cookie is rejected.
    /// - The first non-auth error encountered.
    /// - [`LunaError::FormTokensNotFound`] if `cookies` is empty.
    pub fn authenticate(cookies: &[&str], delay_ms: u64) -> Result<(Self, usize), LunaError> {
        let attempts: Vec<_> = cookies
            .iter()
            .map(|cookie| {
                move || -> Result<Self, LunaError> {
                    let session = Self::new(cookie, delay_ms)?;
                    session.probe()?;
                    Ok(session)
                }
            })
            .collect();
        crate::web_auth::try_chain(attempts, LunaError::is_auth_failure).map_err(|e| match e {
            crate::web_auth::ChainError::Empty => {
                LunaError::FormTokensNotFound("no session cookie supplied".to_string())
            }
            crate::web_auth::ChainError::AllFailed(inner) => inner,
        })
    }

    /// Fully hydrate all charts in the account: listing pages, then per-chart
    /// `cast.json` and sidebar.
    ///
    /// - `resume_from`: when `Some(prefix)`, skips rows until a name starts
    ///   with that prefix (case-insensitive); useful to resume an interrupted fetch.
    /// - `normalize_scan`: when `true`, skips rows whose listing name is already
    ///   clean (no non-cp1252 chars, not truncated).
    /// - `on_start(current, total, name)`: called before fetching each chart.
    /// - `on_result(status)`: called after each chart with `"ok"`, `"clean"`,
    ///   `"[skip]"`, or an error message starting with `"[!]"`.
    ///
    /// Returns `(charts, phenom_ids)` parallel vecs.
    ///
    /// # Errors
    /// - [`LunaError::Http`] if the initial listing request fails; subsequent
    ///   per-chart HTTP errors are reported via `on_result` and do not propagate.
    ///
    /// # Panics
    /// Does not panic in practice. The `resume_from.unwrap()` inside the loop
    /// is only reachable when `skipping` is `true`, which requires
    /// `resume_from.is_some()`.
    pub fn fetch_charts(
        &self,
        resume_from: Option<&str>,
        normalize_scan: bool,
        on_start: &dyn Fn(usize, usize, &str),
        on_result: &dyn Fn(&str),
    ) -> Result<(Vec<crate::chart::Chart>, Vec<String>), LunaError> {
        use crate::normalize::normalize_chart;

        let listing = self.fetch_listing()?;
        let total = listing.len();
        let mut charts = Vec::new();
        let mut phenom_ids = Vec::new();
        // Listing positions (1-based) parallel to `charts`, so the inline
        // candidate flag can name an earlier row by the `[N/total]` prefix
        // the user saw on screen.
        let mut listing_positions: Vec<usize> = Vec::new();
        let mut skipping = resume_from.is_some();

        for (i, row) in listing.iter().enumerate() {
            if skipping {
                if at_resume_point(&row.name, resume_from.unwrap()) {
                    skipping = false;
                } else {
                    on_start(i + 1, total, &row.name.chars().take(40).collect::<String>());
                    on_result("[skip]");
                    continue;
                }
            }
            if normalize_scan && !needs_fetch_for_normalize(&row.name) {
                on_start(i + 1, total, &row.name.chars().take(40).collect::<String>());
                on_result("clean");
                continue;
            }

            on_start(i + 1, total, &row.name.chars().take(40).collect::<String>());

            let cast_url = format!("{BASE_URL}/charts/cast.json?uniwheel={}", row.chart_id);
            let cast_json = match self.get_text(&cast_url) {
                Ok(t) => t,
                Err(e) => {
                    on_result(&format!("[!] cast.json: {e}"));
                    continue;
                }
            };
            let cast = match parse_cast_json(&cast_json) {
                Ok(m) => m,
                Err(e) => {
                    on_result(&format!("[!] parse cast: {e}"));
                    continue;
                }
            };
            self.sleep();

            let view_url = format!("{BASE_URL}/radix-charts/view?uniwheel={}", row.chart_id);
            let sidebar_html = match self.get_text(&view_url) {
                Ok(t) => t,
                Err(e) => {
                    on_result(&format!("[!] sidebar: {e}"));
                    continue;
                }
            };
            let sidebar = parse_sidebar(&sidebar_html);
            self.sleep();

            let luna_chart = LunaChart {
                chart_id: row.chart_id.clone(),
                name: row.name.clone(),
                chart_type: row.chart_type.clone(),
                date: cast.date,
                time: cast.time,
                lat: cast.lat,
                lon: cast.lon,
                offset_str: cast.offset_str,
                location: cast.location,
                zodiac: cast.zodiac,
                house_system: sidebar.house_system,
                tz_abbrev: sidebar.tz_abbrev,
                is_lmt: sidebar.is_lmt,
                rodden_code: sidebar.rodden_code,
                rodden_desc: sidebar.rodden_desc,
                notes: String::new(),
            };
            let phenom_id = sidebar.phenom_id.unwrap_or_default();
            match luna_chart_to_chart(&luna_chart) {
                Ok(mut chart) => {
                    normalize_chart(&mut chart);
                    let status = candidate_status(&chart, &charts, &listing_positions);
                    on_result(&status);
                    charts.push(chart);
                    phenom_ids.push(phenom_id);
                    listing_positions.push(i + 1);
                }
                Err(e) => {
                    on_result(&format!("[!] convert: {e}"));
                }
            }
        }
        Ok((charts, phenom_ids))
    }

    /// Create a single chart in LUNA.  Returns the phenomenon UUID.
    ///
    /// # Errors
    /// - [`LunaError::Http`] if the form GET or create POST fails.
    /// - [`LunaError::FormTokensNotFound`] if the create form has no `CakePHP`
    ///   token block.
    /// - [`LunaError::PhenomIdNotFound`] if the server responds with 2xx but
    ///   the phenomenon UUID cannot be extracted from the redirect URL or body.
    pub fn create_one(&self, chart: &crate::chart::Chart) -> Result<String, LunaError> {
        let add_url = format!("{BASE_URL}/phenomena/add");
        let form_html = self.get_text(&add_url)?;
        let tokens = parse_form_tokens(&form_html, "/phenomena/add")
            .ok_or_else(|| LunaError::FormTokensNotFound("/phenomena/add".into()))?;
        let payload = create_payload(chart, &tokens);
        let resp = self
            .client
            .post(&add_url)
            .form(&payload)
            .send()?
            .error_for_status()?;
        let final_url = resp.url().as_str().to_string();
        let body = resp.text()?;
        extract_phenom_id(&final_url)
            .or_else(|| extract_phenom_id(&body))
            .ok_or(LunaError::PhenomIdNotFound)
    }

    /// Edit an existing chart in LUNA (PUT via `CakePHP` method tunnel).
    ///
    /// # Errors
    /// - [`LunaError::Http`] if the form GET or edit POST fails.
    /// - [`LunaError::FormTokensNotFound`] if the edit form has no `CakePHP`
    ///   token block.
    pub fn edit_one(&self, chart: &crate::chart::Chart, phenom_id: &str) -> Result<(), LunaError> {
        let edit_url = format!("{BASE_URL}/phenomena/edit/{phenom_id}");
        let form_html = self.get_text(&edit_url)?;
        let tokens = parse_form_tokens(&form_html, &format!("/phenomena/edit/{phenom_id}"))
            .ok_or_else(|| LunaError::FormTokensNotFound(format!("/phenomena/edit/{phenom_id}")))?;
        let payload = edit_payload(chart, &tokens);
        self.client
            .post(&edit_url)
            .form(&payload)
            .send()?
            .error_for_status()?;
        Ok(())
    }

    /// Delete a chart from LUNA by phenomenon UUID.
    ///
    /// Performs a `POST` to `/phenomena/delete/<phenom_id>` with the `CakePHP`
    /// `_method=DELETE` tunnel and a fresh CSRF + security-token envelope
    /// scraped from `/phenomena/edit/<phenom_id>` (the edit page is the
    /// canonical source for delete tokens — the listing's bulk-delete form
    /// would work too but requires page-2 navigation).
    ///
    /// # Errors
    /// - [`LunaError::Http`] if either HTTP request fails.
    /// - [`LunaError::FormTokensNotFound`] if the edit form is missing its
    ///   `CakePHP` token block (session cookie likely expired).
    pub fn delete_phenom(&self, phenom_id: &str) -> Result<(), LunaError> {
        let edit_url = format!("{BASE_URL}/phenomena/edit/{phenom_id}");
        let form_html = self.get_text(&edit_url)?;
        // Tokens must come from the delete form (action=/phenomena/delete/...), not the
        // edit form — each form on the page has its own _Token[fields] value.
        let tokens = parse_form_tokens(&form_html, &format!("/phenomena/delete/{phenom_id}"))
            .ok_or_else(|| {
                LunaError::FormTokensNotFound(format!("/phenomena/delete/{phenom_id}"))
            })?;
        let payload = delete_payload(&tokens);
        let delete_url = format!("{BASE_URL}/phenomena/delete/{phenom_id}");
        self.client
            .post(&delete_url)
            .form(&payload)
            .send()?
            .error_for_status()?;
        Ok(())
    }

    /// Write charts to LUNA: create new ones (empty `phenom_id`) or edit existing.
    ///
    /// `phenom_ids` must be either empty (→ create all) or the same length as
    /// `charts`.  Non-empty `phenom_id` at position `i` triggers an in-place edit.
    ///
    /// - `on_start(current, total, name)`: called before each write.
    /// - `on_result(status)`: called after with `"created <uuid>"`, `"edited <uuid>"`,
    ///   or an error message starting with `"[!]"`.
    ///
    /// # Errors
    /// Always returns `Ok(())`; per-chart failures are reported via the
    /// `on_result` closure as `"[!] create: …"` or `"[!] edit: …"` strings
    /// rather than propagated.
    pub fn write_charts(
        &self,
        charts: &[crate::chart::Chart],
        phenom_ids: &[String],
        on_start: &dyn Fn(usize, usize, &str),
        on_result: &dyn Fn(&str),
    ) -> Result<(), LunaError> {
        let total = charts.len();
        for (i, chart) in charts.iter().enumerate() {
            on_start(
                i + 1,
                total,
                &chart.name.chars().take(40).collect::<String>(),
            );
            let pid = phenom_ids.get(i).map_or("", String::as_str);
            if pid.is_empty() {
                match self.create_one(chart) {
                    Ok(id) => on_result(&format!("created {id}")),
                    Err(e) => on_result(&format!("[!] create: {e}")),
                }
            } else {
                match self.edit_one(chart, pid) {
                    Ok(()) => on_result(&format!("edited  {pid}")),
                    Err(e) => on_result(&format!("[!] edit: {e}")),
                }
            }
            self.sleep();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn luna_authenticate_empty_chain_errors() {
        // LunaSession does not implement Debug (reqwest Client), so we cannot
        // call .unwrap_err() — that would require the Ok type to be Debug.
        // Use a match instead; the panic arm avoids {:?} on the Ok variant.
        match LunaSession::authenticate(&[], 0) {
            Err(LunaError::FormTokensNotFound(_)) => {}
            Err(other) => panic!("expected FormTokensNotFound, got {other:?}"),
            Ok(_) => panic!("expected Err(FormTokensNotFound), got Ok"),
        }
    }

    #[test]
    fn luna_auth_failure_classification() {
        assert!(LunaError::FormTokensNotFound("/x".into()).is_auth_failure());
        // Not credential problems:
        assert!(!LunaError::MissingUniwheel.is_auth_failure());
        assert!(!LunaError::PhenomIdNotFound.is_auth_failure());
        assert!(!LunaError::InvalidCoordinate.is_auth_failure());
    }

    #[test]
    #[ignore = "requires LUNA_TOKEN and network"]
    fn probe_live_smoke() {
        let Ok(token) = std::env::var("LUNA_TOKEN") else {
            eprintln!("LUNA_TOKEN unset — skipping live probe");
            return;
        };
        let Ok(session) = LunaSession::new(&token, 0) else {
            eprintln!("client build failed — skipping");
            return;
        };
        // A present-but-stale token surfaces as an auth failure, which is a valid
        // outcome here; only assert it does not panic.
        let _ = session.probe();
    }
}
