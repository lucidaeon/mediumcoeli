//! Astrotheoros.com session management and RSC parsing.
//!
//! Authentication uses Clerk's two-step flow: step 1 POSTs the email to
//! `sign_ins` (identifies, returns a `sign_in_id`); step 2 POSTs the password to
//! `attempt_first_factor` (verifies, returns a JWT + `session_id`). The JWT
//! expires every 60 seconds; [`AstrotheorosSession`] auto-refreshes it via the
//! session token endpoint when fewer than 20 seconds remain.
//!
//! Reading charts uses the Next.js RSC wire format: `GET /app` with an `rsc: 1`
//! header returns newline-delimited `<hex>:<json>` lines. [`parse_rsc_response`]
//! locates the `"charts":[` line, strips the `$D` date prefix, maps `"$undefined"`
//! to `null`, and deserialises the result into `Vec<ApiChartEntry>`.

use crate::capability::{CapabilitySet, ChartField};
use crate::chart::{Chart, CoordinateSystem, EventType, HouseSystem, Latitude, Longitude, Zodiac};
use reqwest::blocking::Client;
use std::cell::RefCell;
use std::time::Duration;
use thiserror::Error;

/// Fields recovered when reading an astrotheoros.com chart.
///
/// `parse_rsc_response` / `entry_to_chart` set `region: None` and
/// `event_type: EventType::Unspecified` — neither field is exposed by the API.
pub const READ_CAPS: CapabilitySet = CapabilitySet::new(&[]);

/// Fields persisted when writing an astrotheoros.com chart.
///
/// `chart_to_create_body` folds `region` into the freeform `locationName`
/// (`"city, region"`), so the region text genuinely lands on astrotheoros and
/// shows in its UI. This is asymmetric with [`READ_CAPS`]: the API stores only
/// a single location string with no structured region column, so `region`
/// cannot be recovered on read (splitting the label is unreliable). Region is
/// therefore a write capability only — mirroring astrocom's read-only Region.
pub const WRITE_CAPS: CapabilitySet = CapabilitySet::new(&[ChartField::Region]);

/// The account-wide render settings astrotheoros applies to every chart.
///
/// astrotheoros stores house system and zodiac globally, not per chart, so a
/// written chart renders with these regardless of its source values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AstrotheorosSettings {
    /// Global house system (`houseSystem` setting).
    pub house_system: HouseSystem,
    /// Global zodiac (`zodiacMode` + `ayanamsha` settings).
    pub zodiac: Zodiac,
}

/// Map an astrotheoros `houseSystem` code to a canonical [`HouseSystem`].
///
/// Unknown codes map to `HouseSystem::Other(0)`.
#[must_use]
pub fn map_house_system(code: &str) -> HouseSystem {
    match code {
        "P" => HouseSystem::Placidus,
        "K" => HouseSystem::Koch,
        "R" => HouseSystem::Regiomontanus,
        "C" => HouseSystem::Campanus,
        "O" => HouseSystem::Porphyry,
        "A" => HouseSystem::Alcabitius,
        "W" | "WA" => HouseSystem::WholeSign,
        "E" | "X" => HouseSystem::Equal,
        _ => HouseSystem::Other(0),
    }
}

/// Map an astrotheoros `zodiacMode` + `ayanamsha` to a canonical [`Zodiac`].
///
/// `TROPICAL` (or any unrecognised mode) maps to `Zodiac::Tropical`; `SIDEREAL`
/// resolves the ayanamsha, defaulting unknown ayanamshas to `Zodiac::Other(0)`.
#[must_use]
pub fn map_zodiac(mode: &str, ayanamsha: &str) -> Zodiac {
    if mode != "SIDEREAL" {
        return Zodiac::Tropical;
    }
    match ayanamsha {
        "LAHIRI" => Zodiac::Lahiri,
        "FAGAN_ALLEN" | "FAGAN_BRADLEY" => Zodiac::FaganAllen,
        "RAMAN" => Zodiac::Raman,
        "KRISHNAMURTI" => Zodiac::Krishnamurti,
        "DELUCE" => Zodiac::DeLuce,
        "DJWHAL_KHUL" => Zodiac::DjwhalKhul,
        "SRI_YUKTESWAR" => Zodiac::SriYukteswar,
        _ => Zodiac::Other(0),
    }
}

/// Astrotheoros.com base URL.
pub const BASE_URL: &str = "https://astrotheoros.com";
/// Clerk API base URL for astrotheoros.com.
pub const CLERK_URL: &str = "https://clerk.astrotheoros.com";

/// URL-encoded JSON describing the Next.js route tree for `/app`.
///
/// Must match the server's route structure; stable unless Astrotheoros restructures.
const ROUTER_STATE_TREE: &str = concat!(
    "%5B%22%22%2C%7B%22children%22%3A%5B%22(dashboard)%22%2C%7B%22children%22%3A",
    "%5B%22app%22%2C%7B%22children%22%3A%5B%5B%22chartIds%22%2C%22%22%2C%22oc%22%5D%2C",
    "%7B%22children%22%3A%5B%22__PAGE__%22%2C%7B%7D%2Cnull%2C%22refetch%22%5D%7D%2C",
    "null%2Cnull%5D%7D%2Cnull%2Cnull%5D%7D%2Cnull%2Cnull%5D%7D%2Cnull%2Cnull%5D",
);

/// Errors specific to astrotheoros.com sessions.
#[derive(Debug, Error)]
pub enum AstrotheorosError {
    /// An HTTP request failed.
    #[error("HTTP: {0}")]
    Http(#[from] reqwest::Error),
    /// The `reqwest` client could not be constructed.
    #[error("HTTP client build error: {0}")]
    HttpClientBuild(String),
    /// Clerk login step 1 failed — `sign_in` id not found in response.
    #[error("Clerk identify failed: {0}")]
    ClerkIdentifyFailed(String),
    /// Clerk login step 2 failed — JWT or `session_id` not in response.
    #[error("Clerk auth failed: {0}")]
    ClerkAuthFailed(String),
    /// JWT could not be refreshed.
    #[error("JWT refresh failed: {0}")]
    JwtRefreshFailed(String),
    /// JSON serialization or deserialization failed.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    /// Atlas timezone lookup returned unexpected shape.
    #[error("atlas response missing timezone or utcOffset")]
    AtlasResponseInvalid,
    /// Chart create response missing entry id.
    #[error("create response missing entry.id")]
    CreateResponseInvalid,
    /// A coordinate was outside its valid range.
    #[error("invalid coordinate from API: {0}")]
    InvalidCoordinate(String),
    /// Delete returned non-success.
    #[error("delete returned success=false for id {0}")]
    DeleteFailed(String),
}

impl AstrotheorosError {
    /// True when this error means the credential was rejected and the caller
    /// should fall through to the next credential in the chain.
    ///
    /// `ClerkIdentifyFailed` / `ClerkAuthFailed` are rejected logins;
    /// `JwtRefreshFailed` means the browser session (its `__client` cookie)
    /// is no longer valid for refresh. HTTP 401/403 is delegated to
    /// [`crate::web_auth::is_unauthorized`].
    #[must_use]
    pub fn is_auth_failure(&self) -> bool {
        match self {
            Self::Http(e) => crate::web_auth::is_unauthorized(e),
            Self::ClerkIdentifyFailed(_) | Self::ClerkAuthFailed(_) | Self::JwtRefreshFailed(_) => {
                true
            }
            Self::HttpClientBuild(_)
            | Self::Json(_)
            | Self::AtlasResponseInvalid
            | Self::CreateResponseInvalid
            | Self::InvalidCoordinate(_)
            | Self::DeleteFailed(_) => false,
        }
    }
}

/// A single Astrotheoros credential source for the fall-through chain.
///
/// Canonical chain order is `Cookie` → `Token` → `Login` (see
/// [`AstrotheorosSession::authenticate`]).
#[derive(Debug)]
pub enum AstrotheorosCredential {
    /// Clerk cookies harvested from a logged-in browser. `client_cookie` is
    /// the `__client` cookie required for token refresh (see
    /// [`AstrotheorosSession::from_browser`]).
    Cookie {
        /// The `__session` JWT.
        jwt: String,
        /// The Clerk session id (`sid` claim).
        session_id: String,
        /// The `__client_uat` timestamp cookie value.
        client_uat: String,
        /// The `__client` cookie, when present.
        client_cookie: Option<String>,
    },
    /// A `jwt:session_id:client_uat` token triple (e.g. `--astrotheoros-token`).
    Token {
        /// The session JWT.
        jwt: String,
        /// The Clerk session id.
        session_id: String,
        /// The `__client_uat` timestamp.
        client_uat: String,
    },
    /// Email + password for a full Clerk login.
    Login {
        /// Account email.
        email: String,
        /// Account password.
        password: String,
    },
}

/// Per-record callback for [`AstrotheorosSession::write_charts`]:
/// `(orig_index, new_index, total_new, source, status, landed_entry)`.
pub type WriteRecordFn<'a> =
    dyn FnMut(usize, usize, usize, &Chart, &str, Option<&ApiChartEntry>) + 'a;

/// One chart entry as returned by the astrotheoros.com API.
/// Month is 0-indexed (0 = January) — matches JS `Date.getMonth()`.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiChartEntry {
    /// Chart UUID (stable identifier).
    pub id: String,
    /// Subject name.
    pub name: String,
    /// Day of month, 1-indexed.
    pub day: u8,
    /// Month, 0-indexed (0 = January, 11 = December).
    pub month: u8,
    /// Year.
    pub year: i16,
    /// Hour (24-hour format).
    pub hour: u8,
    /// Minute.
    pub minute: u8,
    /// IANA timezone identifier.
    pub timezone: String,
    /// Historical UTC offset in whole hours (DST-aware).
    pub utc_offset: i32,
    /// Human-readable location string.
    pub location_name: String,
    /// North-positive latitude in decimal degrees.
    pub latitude: f64,
    /// East-positive longitude in decimal degrees.
    pub longitude: f64,
    /// Whether chart is marked favorite.
    #[serde(default)]
    pub favorite: Option<bool>,
    /// Transit location name (optional).
    #[serde(default)]
    pub t_location_name: Option<String>,
    /// Transit latitude (optional).
    #[serde(default)]
    pub t_latitude: Option<f64>,
    /// Transit longitude (optional).
    #[serde(default)]
    pub t_longitude: Option<f64>,
    /// Transit timezone (optional).
    #[serde(default)]
    pub t_timezone: Option<String>,
}

/// Parse the Next.js RSC wire-format response from `GET /app`.
///
/// The response is newline-delimited `<hex>:<json>` lines. The charts array
/// lives on the line containing `"charts":[`. The `$D` date prefix and
/// `"$undefined"` sentinel are normalised before JSON parsing.
///
/// Returns an empty vec if no charts line is found (not an error).
#[must_use]
pub fn parse_rsc_response(text: &str) -> Vec<ApiChartEntry> {
    for line in text.lines() {
        if !line.contains("\"charts\":[") {
            continue;
        }
        let Some(colon) = line.find(':') else {
            continue;
        };
        let json_str = &line[colon + 1..];
        // Strip $D date prefix: "$D2026-..." → "2026-..."
        let json_str = regex_lite_replace_d_prefix(json_str);
        // Map RSC undefined sentinel to JSON null
        let json_str = json_str.replace("\"$undefined\"", "null");
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&json_str) else {
            continue;
        };
        // Structure: ["$", "$L<component>", null, {charts:[...], settings:{...}}]
        let props = if let Some(arr) = value.as_array() {
            arr.get(3).cloned().unwrap_or(serde_json::Value::Null)
        } else {
            value
        };
        if let Some(charts_val) = props.get("charts") {
            if let Ok(entries) = serde_json::from_value::<Vec<ApiChartEntry>>(charts_val.clone()) {
                return entries;
            }
        }
    }
    Vec::new()
}

/// Extract the account-wide settings from a Next.js RSC response.
///
/// The settings object rides in the same `props` payload as the charts array
/// (`{"charts":[…],"settings":{…}}`). Returns `None` if no settings object with
/// a `houseSystem` field is found.
#[must_use]
pub fn parse_rsc_settings(text: &str) -> Option<AstrotheorosSettings> {
    for line in text.lines() {
        if !line.contains("\"settings\":") {
            continue;
        }
        let colon = line.find(':')?;
        let json_str = &line[colon + 1..];
        let json_str = regex_lite_replace_d_prefix(json_str);
        let json_str = json_str.replace("\"$undefined\"", "null");
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&json_str) else {
            continue;
        };
        let props = value
            .as_array()
            .and_then(|a| a.get(3).cloned())
            .unwrap_or(value);
        let settings = props.get("settings")?;
        let house = settings.get("houseSystem")?.as_str()?;
        let mode = settings
            .get("zodiacMode")
            .and_then(|v| v.as_str())
            .unwrap_or("TROPICAL");
        let ayan = settings
            .get("ayanamsha")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        return Some(AstrotheorosSettings {
            house_system: map_house_system(house),
            zodiac: map_zodiac(mode, ayan),
        });
    }
    None
}

/// Replace `"$D<iso>"` occurrences with `"<iso>"` without pulling in regex.
fn regex_lite_replace_d_prefix(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(pos) = rest.find("\"$D") {
        out.push_str(&rest[..pos]);
        out.push('"');
        // Skip the "$D" prefix (3 chars after the opening quote)
        rest = &rest[pos + 3..]; // now at the ISO content
    }
    out.push_str(rest);
    out
}

/// Convert an `ApiChartEntry` to a canonical `Chart`.
///
/// Month is converted from 0-indexed API convention to 1-indexed `Chart` convention.
/// `is_lmt` is always `false` — astrotheoros does not support LMT.
///
/// # Errors
/// Returns [`AstrotheorosError::InvalidCoordinate`] if lat/lon are out of range.
pub fn entry_to_chart(entry: &ApiChartEntry) -> Result<Chart, AstrotheorosError> {
    let latitude = Latitude::new(entry.latitude)
        .map_err(|_| AstrotheorosError::InvalidCoordinate(format!("lat={}", entry.latitude)))?;
    let longitude = Longitude::new(entry.longitude)
        .map_err(|_| AstrotheorosError::InvalidCoordinate(format!("lon={}", entry.longitude)))?;
    Ok(Chart {
        name: entry.name.clone(),
        secondary_name: None,
        city: Some(entry.location_name.clone()),
        region: None,
        longitude,
        latitude,
        year: entry.year,
        month: entry.month + 1, // 0-indexed → 1-indexed
        day: entry.day,
        hour: entry.hour,
        minute: entry.minute,
        second: 0,
        tz_offset_hours: f64::from(entry.utc_offset),
        tz_abbreviation: Some(entry.timezone.clone()),
        is_lmt: false,
        event_type: EventType::Unspecified,
        source_rating: None,
        house_system: HouseSystem::Placidus,
        zodiac: Zodiac::Tropical,
        coordinate_system: CoordinateSystem::Geocentric,
        sub_charts: vec![],
        notes: None,
    })
}

/// Build the `{"data": {...}}` JSON body for `POST /api/chart`.
///
/// `iana_tz` and `utc_offset` must come from a prior `GET /api/atlas` call
/// for the chart's birth location and time; they are not stored in `Chart`.
/// Month is converted from 1-indexed `Chart` convention to 0-indexed API convention.
///
/// `locationName` is the freeform place label astrotheoros stores and displays.
/// When the chart carries a `region`, it is appended as `"city, region"` so the
/// region survives into astrotheoros (see [`WRITE_CAPS`]); the chart's explicit
/// latitude/longitude — not this string — drive the chart math.
#[must_use]
pub fn chart_to_create_body(chart: &Chart, iana_tz: &str, utc_offset: i32) -> serde_json::Value {
    let location_name = match (chart.city.as_deref(), chart.region.as_deref()) {
        (Some(city), Some(region)) if !region.is_empty() => format!("{city}, {region}"),
        (Some(city), _) => city.to_string(),
        (None, _) => String::new(),
    };
    serde_json::json!({
        "data": {
            "name": chart.name,
            "day": chart.day,
            "month": chart.month - 1,   // 1-indexed → 0-indexed
            "year": chart.year,
            "hour": chart.hour,
            "minute": chart.minute,
            "timezone": iana_tz,
            "utcOffset": utc_offset,
            "manualUtcOffset": null,
            "locationName": location_name,
            "latitude": chart.latitude.degrees(),
            "longitude": chart.longitude.degrees(),
            "tUseBirthLocation": true,
            "tLatitude": null,
            "tLongitude": null,
        }
    })
}

/// Convert a local calendar datetime to Unix milliseconds, treating the time as UTC.
///
/// Used to determine the approximate historical moment for `GET /api/atlas`.
/// The result is intentionally naive (no DST/offset applied) — the atlas call
/// itself returns the historically correct offset for the supplied coordinates.
///
/// Uses the proleptic Gregorian calendar via Julian Day Numbers.
#[must_use]
pub fn calendar_to_unix_ms(year: i16, month_1indexed: u8, day: u8, hour: u8, minute: u8) -> i64 {
    const UNIX_EPOCH_JDN: i64 = 2_440_588;
    let days = jdn(year, month_1indexed, day) - UNIX_EPOCH_JDN;
    let secs = days * 86_400 + i64::from(hour) * 3_600 + i64::from(minute) * 60;
    secs * 1_000
}

/// Compute the Julian Day Number for a proleptic Gregorian date.
fn jdn(year: i16, month: u8, day: u8) -> i64 {
    let y = i64::from(year);
    let m = i64::from(month);
    let d = i64::from(day);
    let a = (14 - m) / 12;
    let yy = y + 4_800 - a;
    let mm = m + 12 * a - 3;
    d + (153 * mm + 2) / 5 + 365 * yy + yy / 4 - yy / 100 + yy / 400 - 32_045
}

// ── Base64url decoder (no external crate) ─────────────────────────────────────

/// Decode a base64url-encoded byte string (with or without padding).
///
/// Translates the URL-safe alphabet (`-`, `_`) to standard (`+`, `/`) then
/// decodes each 4-byte chunk, respecting `=` padding.  Returns `None` if the
/// input contains an invalid character.
fn base64url_decode(s: &str) -> Option<Vec<u8>> {
    let pad = (4 - s.len() % 4) % 4;
    let padded: String = s
        .chars()
        .map(|c| match c {
            '-' => '+',
            '_' => '/',
            c => c,
        })
        .chain(std::iter::repeat_n('=', pad))
        .collect();

    let mut out = Vec::with_capacity(padded.len() * 3 / 4);
    let bytes = padded.as_bytes();

    let decode_char = |b: u8| -> Option<u8> {
        match b {
            b'A'..=b'Z' => Some(b - b'A'),
            b'a'..=b'z' => Some(b - b'a' + 26),
            b'0'..=b'9' => Some(b - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            b'=' => Some(0),
            _ => None,
        }
    };

    for chunk in bytes.chunks(4) {
        if chunk.len() < 4 {
            return None;
        }
        let b = [
            decode_char(chunk[0])?,
            decode_char(chunk[1])?,
            decode_char(chunk[2])?,
            decode_char(chunk[3])?,
        ];
        out.push((b[0] << 2) | (b[1] >> 4));
        if chunk[2] != b'=' {
            out.push((b[1] << 4) | (b[2] >> 2));
        }
        if chunk[3] != b'=' {
            out.push((b[2] << 6) | b[3]);
        }
    }
    Some(out)
}

// ── JWT helpers ───────────────────────────────────────────────────────────────

/// Extract the `exp` (Unix seconds) field from a JWT payload without verifying
/// the signature.
///
/// Returns `None` if the token is malformed or the payload lacks an `exp` field.
#[must_use]
pub fn jwt_exp(token: &str) -> Option<i64> {
    let parts: Vec<&str> = token.splitn(3, '.').collect();
    if parts.len() < 2 {
        return None;
    }
    let bytes = base64url_decode(parts[1])?;
    let json: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    json["exp"].as_i64()
}

/// Extract the Clerk session id (the `sid` claim) from a `__session` JWT.
///
/// Clerk's session token carries the session id in its payload, so a session
/// imported from a browser `__session` cookie can recover its `session_id`
/// without a network call. Returns `None` if the token is malformed or has no
/// `sid` claim.
#[must_use]
pub fn jwt_sid(token: &str) -> Option<String> {
    let parts: Vec<&str> = token.splitn(3, '.').collect();
    if parts.len() < 2 {
        return None;
    }
    let bytes = base64url_decode(parts[1])?;
    let json: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    json["sid"].as_str().map(str::to_string)
}

/// Extract the `__client_uat` cookie value from a `Set-Cookie` response header.
///
/// Returns `None` if no such cookie is present.
#[must_use]
pub fn extract_client_uat(headers: &reqwest::header::HeaderMap) -> Option<String> {
    for value in headers.get_all("set-cookie") {
        let Ok(s) = value.to_str() else { continue };
        if let Some(rest) = s.strip_prefix("__client_uat=") {
            let end = rest.find(';').unwrap_or(rest.len());
            return Some(rest[..end].to_string());
        }
    }
    None
}

/// Extract the `__client` cookie (Clerk's long-lived client JWT) from a
/// `Set-Cookie` response header.
///
/// The match is on the exact `__client=` prefix so it never picks up the
/// sibling `__client_uat=` cookie (which shares the `__client` stem). Returns
/// `None` if no `__client` cookie is present. This value authenticates the
/// token-refresh call to Clerk; without it `POST …/sessions/{id}/tokens`
/// returns 401.
#[must_use]
pub fn extract_client(headers: &reqwest::header::HeaderMap) -> Option<String> {
    for value in headers.get_all("set-cookie") {
        let Ok(s) = value.to_str() else { continue };
        if let Some(rest) = s.strip_prefix("__client=") {
            let end = rest.find(';').unwrap_or(rest.len());
            return Some(rest[..end].to_string());
        }
    }
    None
}

// ── Session struct ────────────────────────────────────────────────────────────

/// Authenticated HTTP session for an astrotheoros.com account.
///
/// The Clerk `__session` JWT expires every 60 seconds.
/// [`AstrotheorosSession`] auto-refreshes it before each API call when
/// fewer than 20 seconds remain.
pub struct AstrotheorosSession {
    client: Client,
    jwt: RefCell<String>,
    session_id: String,
    client_uat: String,
    /// The Clerk `__client` cookie (long-lived client JWT). `login` leaves this
    /// `None` because its cookie jar already carries `__client`; a session built
    /// from imported browser cookies seeds it here so the token-refresh call to
    /// Clerk is authenticated (otherwise `/tokens` returns 401).
    client_cookie: Option<String>,
    /// Force a token refresh before the first request, regardless of the JWT's
    /// `exp`. Set for browser-imported sessions because the on-disk `__session`
    /// is a lazily-flushed snapshot that may be stale even when not yet expired;
    /// minting a fresh token guarantees a current one for `session_id`.
    force_refresh: std::cell::Cell<bool>,
    delay: Duration,
}

impl AstrotheorosSession {
    fn build_client(user_agent: &str) -> Result<Client, AstrotheorosError> {
        Client::builder()
            .user_agent(user_agent)
            .timeout(Duration::from_secs(60))
            .cookie_store(true)
            .build()
            .map_err(|e| AstrotheorosError::HttpClientBuild(e.to_string()))
    }

    /// Log in with email + password and return an authenticated session.
    ///
    /// Uses the Clerk two-step flow:
    ///   1. `POST /v1/client/sign_ins` — identify by email, get `sign_in_id`.
    ///   2. `POST /v1/client/sign_ins/{id}/attempt_first_factor` — verify password,
    ///      extract JWT, `session_id`, `__client_uat`, and the `__client` cookie
    ///      (the last needed so the later token refresh is authenticated).
    ///
    /// `user_agent` is the User-Agent to send (required).
    ///
    /// # Errors
    /// - [`AstrotheorosError::HttpClientBuild`] if the reqwest client cannot be built.
    /// - [`AstrotheorosError::Http`] on any network error.
    /// - [`AstrotheorosError::ClerkIdentifyFailed`] if step 1 does not return a `sign_in_id`.
    /// - [`AstrotheorosError::ClerkAuthFailed`] if step 2 does not return a valid JWT/session.
    pub fn login(
        email: &str,
        pass: &str,
        delay_ms: u64,
        user_agent: &str,
    ) -> Result<Self, AstrotheorosError> {
        let client = Self::build_client(user_agent)?;

        // Step 1: identify
        let step1_url = format!("{CLERK_URL}/v1/client/sign_ins");
        let step1_resp = client
            .post(&step1_url)
            .header("Origin", BASE_URL)
            .form(&[("identifier", email), ("locale", "en-US")])
            .send()?
            .error_for_status()?;
        // Clerk sets the long-lived `__client` cookie on the first sign-in
        // request; capture it before `.json()` consumes the response so the
        // later token refresh can authenticate (its explicit Cookie header
        // suppresses the jar, so the value must be carried explicitly).
        let client_cookie_step1 = extract_client(step1_resp.headers());
        let step1_json: serde_json::Value = step1_resp.json()?;
        let sign_in_id = step1_json["response"]["id"]
            .as_str()
            .ok_or_else(|| AstrotheorosError::ClerkIdentifyFailed(step1_json.to_string()))?
            .to_string();

        // Step 2: verify password
        let step2_url = format!("{CLERK_URL}/v1/client/sign_ins/{sign_in_id}/attempt_first_factor");
        let step2_resp = client
            .post(&step2_url)
            .header("Origin", BASE_URL)
            .form(&[("strategy", "password"), ("password", pass)])
            .send()?
            .error_for_status()?;
        let client_uat = extract_client_uat(step2_resp.headers()).ok_or_else(|| {
            AstrotheorosError::ClerkAuthFailed("__client_uat not in response".into())
        })?;
        // Prefer a `__client` refreshed by step 2, else the one from step 1.
        let client_cookie = extract_client(step2_resp.headers()).or(client_cookie_step1);
        let step2_json: serde_json::Value = step2_resp.json()?;
        let session = step2_json["client"]["sessions"]
            .as_array()
            .and_then(|a| a.first())
            .ok_or_else(|| AstrotheorosError::ClerkAuthFailed(step2_json.to_string()))?;
        let session_id = session["id"]
            .as_str()
            .ok_or_else(|| AstrotheorosError::ClerkAuthFailed("session id missing".into()))?
            .to_string();
        let jwt = session["last_active_token"]["jwt"]
            .as_str()
            .ok_or_else(|| AstrotheorosError::ClerkAuthFailed("jwt missing".into()))?
            .to_string();

        Ok(Self {
            client,
            jwt: RefCell::new(jwt),
            session_id,
            client_uat,
            // Captured from the sign-in Set-Cookie headers so the JWT refresh
            // (which fires once the 60 s token nears expiry) is authenticated.
            // `None` only if Clerk did not send `__client`, in which case the
            // session still works until the first refresh is needed.
            client_cookie,
            force_refresh: std::cell::Cell::new(false),
            delay: Duration::from_millis(delay_ms),
        })
    }

    /// Build a session from existing Clerk credentials (useful for testing or
    /// resuming a session without a fresh login).
    ///
    /// `user_agent` is the User-Agent to send (required).
    ///
    /// # Errors
    /// - [`AstrotheorosError::HttpClientBuild`] if the reqwest client cannot be built.
    pub fn from_jwt(
        jwt: &str,
        session_id: &str,
        client_uat: &str,
        delay_ms: u64,
        user_agent: &str,
    ) -> Result<Self, AstrotheorosError> {
        Ok(Self {
            client: Self::build_client(user_agent)?,
            jwt: RefCell::new(jwt.to_string()),
            session_id: session_id.to_string(),
            client_uat: client_uat.to_string(),
            client_cookie: None,
            force_refresh: std::cell::Cell::new(false),
            delay: Duration::from_millis(delay_ms),
        })
    }

    /// Build a session from cookies harvested from a logged-in browser.
    ///
    /// Same as [`from_jwt`](Self::from_jwt) but also carries the Clerk
    /// `__client` cookie (the long-lived client JWT), which the token-refresh
    /// call to Clerk requires — without it `POST …/sessions/{id}/tokens`
    /// returns 401.
    ///
    /// `user_agent` is the User-Agent to send (required).
    ///
    /// # Errors
    /// - [`AstrotheorosError::HttpClientBuild`] if the reqwest client cannot be built.
    pub fn from_browser(
        jwt: &str,
        session_id: &str,
        client_uat: &str,
        client_cookie: &str,
        delay_ms: u64,
        user_agent: &str,
    ) -> Result<Self, AstrotheorosError> {
        Ok(Self {
            client: Self::build_client(user_agent)?,
            jwt: RefCell::new(jwt.to_string()),
            session_id: session_id.to_string(),
            client_uat: client_uat.to_string(),
            client_cookie: Some(client_cookie.to_string()),
            // The on-disk __session may be a stale snapshot; refresh first.
            force_refresh: std::cell::Cell::new(true),
            delay: Duration::from_millis(delay_ms),
        })
    }

    /// Refresh the Clerk JWT if fewer than 20 seconds remain before expiry.
    fn refresh_jwt_if_needed(&self) -> Result<(), AstrotheorosError> {
        use std::time::{SystemTime, UNIX_EPOCH};
        let now: i64 = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            .try_into()
            .unwrap_or(i64::MAX);
        // Browser-imported sessions force one refresh up front (the on-disk
        // token may be stale); otherwise refresh only when <20s remain.
        let exp = jwt_exp(&self.jwt.borrow()).unwrap_or(0);
        if !self.force_refresh.get() && exp - now > 20 {
            return Ok(());
        }
        let refresh_url = format!("{CLERK_URL}/v1/client/sessions/{}/tokens", self.session_id);
        let current_jwt = self.jwt.borrow().clone();
        let resp = self
            .client
            .post(&refresh_url)
            .header("Origin", BASE_URL)
            .header(
                "Cookie",
                match &self.client_cookie {
                    // The __client cookie authenticates the client to Clerk's
                    // FAPI; the refresh endpoint 401s without it.
                    Some(c) => format!(
                        "__client={c}; __client_uat={}; clerk_active_context={}",
                        self.client_uat, self.session_id
                    ),
                    None => format!(
                        "__client_uat={}; clerk_active_context={}",
                        self.client_uat, self.session_id
                    ),
                },
            )
            .form(&[("organization_id", ""), ("token", current_jwt.as_str())])
            .send()?
            .error_for_status()?;
        let json: serde_json::Value = resp.json()?;
        let new_jwt = json["jwt"]
            .as_str()
            .ok_or_else(|| AstrotheorosError::JwtRefreshFailed(json.to_string()))?;
        *self.jwt.borrow_mut() = new_jwt.to_string();
        self.force_refresh.set(false);
        Ok(())
    }

    /// Cheap authenticated probe: make one real authenticated app request.
    ///
    /// Delegates to [`fetch_settings`](Self::fetch_settings), which calls
    /// `refresh_jwt_if_needed` and then GETs the
    /// account page. This is the right auth check for every credential source:
    ///
    /// - A fresh `login`/token session (valid JWT) authenticates via the JWT
    ///   directly — no refresh is forced, so it does **not** depend on the Clerk
    ///   `__client` cookie that the refresh endpoint requires.
    /// - A browser-imported session sets `force_refresh = true` at construction,
    ///   so the refresh fires *inside* this call carrying its `__client`; a
    ///   stale browser session makes that refresh 401 →
    ///   [`AstrotheorosError::is_auth_failure`] is true and the chain falls through.
    /// - An expired token with no `__client` 401s on the needed refresh and
    ///   correctly falls through.
    /// - A valid-but-empty account returns settings successfully (an empty
    ///   account is **not** an auth failure — see plan non-goals).
    ///
    /// Forcing a refresh here instead would 401 for a fresh `login` session
    /// (whose `__client` lives only in the cookie jar, which the refresh's
    /// explicit `Cookie` header suppresses).
    ///
    /// # Errors
    /// Propagates the request error; auth failures are classifiable via
    /// [`AstrotheorosError::is_auth_failure`].
    pub fn probe(&self) -> Result<(), AstrotheorosError> {
        self.fetch_settings().map(|_| ())
    }

    /// Build the `Cookie` header value for authenticated app requests.
    fn auth_cookies(&self) -> String {
        format!(
            "__session={}; clerk_active_context={}; __client_uat={}",
            self.jwt.borrow(),
            self.session_id,
            self.client_uat,
        )
    }

    /// Sleep for the configured inter-request delay (no-op if delay is zero).
    fn sleep(&self) {
        if !self.delay.is_zero() {
            std::thread::sleep(self.delay);
        }
    }

    /// Fetch all charts from the account via the Next.js RSC endpoint.
    ///
    /// Returns `(charts, uuids)` as parallel vecs. `uuids[i]` is the UUID for `charts[i]`.
    /// An empty UUID means the chart was fetched but its UUID could not be determined —
    /// this should not happen in practice.
    ///
    /// An account with zero charts returns `Ok((vec![], vec![]))`.
    ///
    /// # Errors
    /// - [`AstrotheorosError::Http`] on any network failure.
    /// - [`AstrotheorosError::InvalidCoordinate`] if a chart entry has an out-of-range coordinate.
    pub fn fetch_charts(&self) -> Result<(Vec<Chart>, Vec<String>), AstrotheorosError> {
        self.refresh_jwt_if_needed()?;

        // RSC page URL
        let url = format!("{BASE_URL}/app");

        let rsc_text = self
            .client
            .get(&url)
            .header("Cookie", self.auth_cookies())
            .header("rsc", "1")
            .header("next-router-state-tree", ROUTER_STATE_TREE)
            .send()?
            .error_for_status()?
            .text()?;

        let entries = parse_rsc_response(&rsc_text);

        let mut charts = Vec::with_capacity(entries.len());
        let mut uuids = Vec::with_capacity(entries.len());
        for entry in &entries {
            let chart = entry_to_chart(entry)?;
            uuids.push(entry.id.clone());
            charts.push(chart);
        }
        Ok((charts, uuids))
    }

    /// Fetch the account-wide render settings (house system, zodiac).
    ///
    /// Issues the same `GET /app` RSC request as [`AstrotheorosSession::fetch_charts`] and
    /// extracts the `settings` object.
    ///
    /// # Errors
    /// - [`AstrotheorosError::Http`] on any network failure.
    /// - [`AstrotheorosError::AtlasResponseInvalid`] if no settings object is found.
    pub fn fetch_settings(&self) -> Result<AstrotheorosSettings, AstrotheorosError> {
        self.refresh_jwt_if_needed()?;
        let url = format!("{BASE_URL}/app");
        let rsc_text = self
            .client
            .get(&url)
            .header("Cookie", self.auth_cookies())
            .header("rsc", "1")
            .header("next-router-state-tree", ROUTER_STATE_TREE)
            .send()?
            .error_for_status()?
            .text()?;
        parse_rsc_settings(&rsc_text).ok_or(AstrotheorosError::AtlasResponseInvalid)
    }

    /// Resolve the historical IANA timezone and UTC offset for a birth location.
    ///
    /// `unix_ms` is the birth datetime as Unix milliseconds (use `calendar_to_unix_ms`).
    /// The offset reflects DST at that historical moment, not today's offset.
    ///
    /// # Errors
    /// - [`AstrotheorosError::Http`] on network failure.
    /// - [`AstrotheorosError::AtlasResponseInvalid`] if the response lacks `timezone`/`utcOffset`.
    fn atlas_lookup(
        &self,
        lat: f64,
        lon: f64,
        unix_ms: i64,
    ) -> Result<(String, i32), AstrotheorosError> {
        let url = format!(
            "{BASE_URL}/api/atlas?time={unix_ms}&latitude={lat}&longitude={lon}&disableIllinoisTreatment=true"
        );
        let json: serde_json::Value = self
            .client
            .get(&url)
            .header("Cookie", self.auth_cookies())
            .send()?
            .error_for_status()?
            .json()?;
        let tz = json["timezone"]
            .as_str()
            .ok_or(AstrotheorosError::AtlasResponseInvalid)?
            .to_string();
        #[allow(clippy::cast_possible_truncation)]
        let offset = json["utcOffset"]
            .as_i64()
            .ok_or(AstrotheorosError::AtlasResponseInvalid)? as i32;
        Ok((tz, offset))
    }

    /// Create a single chart on astrotheoros.com. Returns the full landed
    /// [`ApiChartEntry`] echoed by the create response — shape-identical to the
    /// entries returned by the `/app` readback, so callers can verify a write
    /// without a separate readback (use `.id` for just the UUID).
    ///
    /// Pre-calls `GET /api/atlas` with the chart's birth location and time to resolve
    /// the historical IANA timezone and UTC offset, then `POST /api/chart`.
    ///
    /// # Errors
    /// - [`AstrotheorosError::Http`] on any network failure.
    /// - [`AstrotheorosError::AtlasResponseInvalid`] if atlas lookup fails.
    /// - [`AstrotheorosError::CreateResponseInvalid`] if the create response lacks a valid `entry`.
    pub fn create_one(&self, chart: &Chart) -> Result<ApiChartEntry, AstrotheorosError> {
        self.refresh_jwt_if_needed()?;

        let unix_ms =
            calendar_to_unix_ms(chart.year, chart.month, chart.day, chart.hour, chart.minute);
        let lat = chart.latitude.degrees();
        let lon = chart.longitude.degrees();
        // The atlas only has IANA timezone data from roughly 1900 onward.
        // For older dates (or any atlas failure), fall back to the chart's
        // stored offset so the chart can still be created.
        let (iana_tz, utc_offset) = match self.atlas_lookup(lat, lon, unix_ms) {
            Ok(pair) => pair,
            Err(AstrotheorosError::AtlasResponseInvalid | AstrotheorosError::Http(_)) => {
                let tz = chart
                    .tz_abbreviation
                    .clone()
                    .unwrap_or_else(|| "UTC".to_string());
                // tz offsets are small (within ±14h), so truncation is impossible here.
                #[allow(clippy::cast_possible_truncation)]
                let offset = chart.tz_offset_hours.round() as i32;
                (tz, offset)
            }
            Err(e) => return Err(e),
        };

        let body = chart_to_create_body(chart, &iana_tz, utc_offset);
        let url = format!("{BASE_URL}/api/chart");
        let resp_json: serde_json::Value = self
            .client
            .post(&url)
            .header("Cookie", self.auth_cookies())
            .json(&body)
            .send()?
            .error_for_status()?
            .json()?;
        let entry: ApiChartEntry = serde_json::from_value(resp_json["entry"].clone())
            .map_err(|_| AstrotheorosError::CreateResponseInvalid)?;
        Ok(entry)
    }

    /// Write new charts to astrotheoros.com (skips charts with a non-empty UUID).
    ///
    /// Calls `on_record(orig_index, new_index, total_new, source, status, landed)`
    /// after each create completes:
    /// - `orig_index` — the chart's position in `charts` (for status bookkeeping)
    /// - `new_index` — 1-based index among the newly-created charts
    /// - `total_new` — count of charts that will be created
    /// - `source` — the chart that was sent
    /// - `status` — `"created uuid=…"` or `"[!] create: …"`
    /// - `landed` — the entry echoed by the create response (`None` on failure),
    ///   which the caller can convert + diff for readback-free verification
    ///
    /// Per-chart failures surface via `on_record`; the method always returns `Ok(())`.
    ///
    /// # Errors
    /// Always returns `Ok(())`; per-chart failures surface via the `on_record` closure.
    pub fn write_charts(
        &self,
        charts: &[Chart],
        uuids: &[String],
        on_record: &mut WriteRecordFn<'_>,
    ) -> Result<(), AstrotheorosError> {
        let new: Vec<(usize, &Chart)> = charts
            .iter()
            .enumerate()
            .filter(|(i, _)| uuids[*i].is_empty())
            .collect();
        let total = new.len();
        for (n, (orig_i, chart)) in new.iter().enumerate() {
            match self.create_one(chart) {
                Ok(entry) => {
                    let status = format!("created uuid={}", entry.id);
                    on_record(*orig_i, n + 1, total, chart, &status, Some(&entry));
                }
                Err(e) => {
                    let status = format!("[!] create: {e}");
                    on_record(*orig_i, n + 1, total, chart, &status, None);
                }
            }
            self.sleep();
        }
        Ok(())
    }

    /// Delete a single chart by UUID.
    ///
    /// # Errors
    /// - [`AstrotheorosError::Http`] on network failure.
    /// - [`AstrotheorosError::DeleteFailed`] if the server returns `success: false`.
    pub fn delete_one(&self, uuid: &str) -> Result<(), AstrotheorosError> {
        self.refresh_jwt_if_needed()?;
        let url = format!("{BASE_URL}/api/chart");
        let body = serde_json::json!({"data": {"id": uuid}});
        let resp_json: serde_json::Value = self
            .client
            .delete(&url)
            .header("Cookie", self.auth_cookies())
            .json(&body)
            .send()?
            .error_for_status()?
            .json()?;
        if resp_json["success"].as_bool() != Some(true) {
            return Err(AstrotheorosError::DeleteFailed(uuid.to_string()));
        }
        Ok(())
    }

    /// Build a session from a single credential (the offline/login half of
    /// [`Self::authenticate`], factored out for testing). Does **not** probe.
    ///
    /// `user_agent` is the User-Agent to send (required); forwarded to each constructor.
    ///
    /// # Errors
    /// Propagates the underlying constructor error (`from_browser`/`from_jwt`
    /// build the client offline; `login` performs the network Clerk flow).
    fn build_one(
        cred: &AstrotheorosCredential,
        delay_ms: u64,
        user_agent: &str,
    ) -> Result<Self, AstrotheorosError> {
        match cred {
            AstrotheorosCredential::Cookie {
                jwt,
                session_id,
                client_uat,
                client_cookie,
            } => match client_cookie {
                Some(c) => Self::from_browser(jwt, session_id, client_uat, c, delay_ms, user_agent),
                None => Self::from_jwt(jwt, session_id, client_uat, delay_ms, user_agent),
            },
            AstrotheorosCredential::Token {
                jwt,
                session_id,
                client_uat,
            } => Self::from_jwt(jwt, session_id, client_uat, delay_ms, user_agent),
            AstrotheorosCredential::Login { email, password } => {
                Self::login(email, password, delay_ms, user_agent)
            }
        }
    }

    /// Authenticate against an ordered chain of credentials, falling through on
    /// auth failure. Returns the live session and the index of the credential
    /// that authenticated.
    ///
    /// Each non-login credential is built offline then [`probed`](Self::probe);
    /// a `Login` credential authenticates during construction and is probed too
    /// (cheap, and confirms the fresh session is usable). Fall-through happens
    /// only on [`AstrotheorosError::is_auth_failure`]; any other error stops the
    /// chain immediately.
    ///
    /// `user_agent` is the User-Agent to send (required); forwarded to each constructor.
    ///
    /// # Errors
    /// - The last auth failure if every credential is rejected.
    /// - The first non-auth error (network/JSON) encountered.
    /// - [`AstrotheorosError::ClerkAuthFailed`] if `chain` is empty.
    pub fn authenticate(
        chain: &[AstrotheorosCredential],
        delay_ms: u64,
        user_agent: &str,
    ) -> Result<(Self, usize), AstrotheorosError> {
        let attempts: Vec<_> = chain
            .iter()
            .map(|cred| {
                move || -> Result<Self, AstrotheorosError> {
                    let session = Self::build_one(cred, delay_ms, user_agent)?;
                    session.probe()?;
                    Ok(session)
                }
            })
            .collect();
        crate::web_auth::try_chain(attempts, AstrotheorosError::is_auth_failure).map_err(
            |e| match e {
                crate::web_auth::ChainError::Empty => {
                    AstrotheorosError::ClerkAuthFailed("no credentials supplied".to_string())
                }
                crate::web_auth::ChainError::AllFailed(inner) => inner,
            },
        )
    }

    /// Delete multiple charts by UUID.
    ///
    /// Per-chart failures are reported via `on_result`; the method always returns `Ok(())`.
    ///
    /// - `on_start(current, total, uuid)`: called before each delete.
    /// - `on_result(status)`: called after with `"deleted"` or `"[!] …"`.
    ///
    /// # Errors
    /// Always returns `Ok(())`; per-chart failures are reported via `on_result` rather than propagated.
    pub fn delete_charts(
        &self,
        uuids: &[String],
        on_start: &dyn Fn(usize, usize, &str),
        on_result: &dyn Fn(&str),
    ) -> Result<(), AstrotheorosError> {
        let total = uuids.len();
        for (i, uuid) in uuids.iter().enumerate() {
            on_start(i + 1, total, uuid);
            match self.delete_one(uuid) {
                Ok(()) => on_result("deleted"),
                Err(e) => on_result(&format!("[!] {e}")),
            }
            self.sleep();
        }
        Ok(())
    }
}

#[cfg(test)]
mod settings_tests {
    use super::*;
    use crate::chart::{HouseSystem, Zodiac};

    const CHAIN_JWT: &str =
        "eyJhbGciOiJSUzI1NiJ9.eyJzaWQiOiAic2Vzc19URVNUMTIzIiwgImV4cCI6IDk5OTk5OTk5OTl9.sig";

    #[test]
    fn authenticate_empty_chain_errors() {
        // unwrap_err() requires T: Debug, which AstrotheorosSession doesn't
        // implement (reqwest::blocking::Client is not Debug). Use match instead.
        match AstrotheorosSession::authenticate(&[], 0, "test/1.0") {
            Err(err) => assert!(matches!(err, AstrotheorosError::ClerkAuthFailed(_))),
            Ok(_) => panic!("expected Err for empty chain"),
        }
    }

    #[test]
    fn authenticate_token_builds_offline_session_shape() {
        // A Token credential builds a session struct with no network (from_jwt).
        // We assert the credential→constructor wiring compiles and the enum is
        // constructable; the network probe is exercised by the live test below.
        let cred = AstrotheorosCredential::Token {
            jwt: CHAIN_JWT.to_string(),
            session_id: "sess_TEST123".to_string(),
            client_uat: "1234567890".to_string(),
        };
        // build_one is the offline half of authenticate; see Step 3.
        let session =
            AstrotheorosSession::build_one(&cred, 0, "test/1.0").expect("token builds offline");
        assert_eq!(session.session_id, "sess_TEST123");
    }

    #[test]
    fn from_jwt_takes_required_user_agent_and_builds() {
        // UA is now required (&str), no Option/default.
        assert!(AstrotheorosSession::from_jwt(CHAIN_JWT, "sess_x", "uat_x", 0, "test/1.0").is_ok());
    }

    #[test]
    #[ignore = "requires ASTROGRAM_ASTROTHEOROS_USER/PASS and network"]
    fn authenticate_falls_through_stale_token_to_login() {
        let (Ok(email), Ok(password)) = (
            std::env::var("ASTROGRAM_ASTROTHEOROS_USER"),
            std::env::var("ASTROGRAM_ASTROTHEOROS_PASS"),
        ) else {
            eprintln!("ASTROGRAM_ASTROTHEOROS_USER/PASS unset — skipping");
            return;
        };
        // A deliberately bogus token (stale) followed by real login creds: the
        // chain must fall through to index 1.
        let chain = vec![
            AstrotheorosCredential::Token {
                jwt: "eyJhbGciOiJSUzI1NiJ9.eyJzaWQiOiJzZXNzX0RFQUQiLCJleHAiOjF9.x".to_string(),
                session_id: "sess_DEAD".to_string(),
                client_uat: "1".to_string(),
            },
            AstrotheorosCredential::Login { email, password },
        ];
        match AstrotheorosSession::authenticate(&chain, 0, "test/1.0") {
            Ok((_, idx)) => assert_eq!(idx, 1, "should authenticate via login, not stale token"),
            Err(e) => {
                eprintln!("network/login unavailable ({e}) — skipping assertion");
            }
        }
    }

    #[test]
    fn jwt_sid_extracts_session_id_from_payload() {
        // header.payload.sig where payload = base64url({"sid":"sess_TEST123",…}).
        let token =
            "eyJhbGciOiJSUzI1NiJ9.eyJzaWQiOiAic2Vzc19URVNUMTIzIiwgImV4cCI6IDk5OTk5OTk5OTl9.sig";
        assert_eq!(jwt_sid(token).as_deref(), Some("sess_TEST123"));
        assert_eq!(jwt_sid("not-a-jwt"), None);
        assert_eq!(jwt_sid(""), None);
    }

    #[test]
    fn house_codes_map() {
        assert_eq!(map_house_system("P"), HouseSystem::Placidus);
        assert_eq!(map_house_system("A"), HouseSystem::Alcabitius);
        assert_eq!(map_house_system("W"), HouseSystem::WholeSign);
        assert_eq!(map_house_system("WA"), HouseSystem::WholeSign);
        assert_eq!(map_house_system("R"), HouseSystem::Regiomontanus);
        assert_eq!(map_house_system("E"), HouseSystem::Equal);
        assert_eq!(map_house_system("X"), HouseSystem::Equal);
        assert_eq!(map_house_system("??"), HouseSystem::Other(0));
    }

    #[test]
    fn zodiac_modes_map() {
        assert_eq!(map_zodiac("TROPICAL", ""), Zodiac::Tropical);
        assert_eq!(map_zodiac("SIDEREAL", "LAHIRI"), Zodiac::Lahiri);
        assert_eq!(map_zodiac("SIDEREAL", "RAMAN"), Zodiac::Raman);
        assert_eq!(map_zodiac("SIDEREAL", "UNKNOWN_AYAN"), Zodiac::Other(0));
        assert_eq!(map_zodiac("WHATEVER", ""), Zodiac::Tropical);
    }

    #[test]
    fn parse_settings_from_rsc_line() {
        // Minimal RSC line: <hex>:<json> where json props carry charts + settings.
        let line = "a:[\"$\",\"$Lx\",null,{\"charts\":[],\"settings\":{\"houseSystem\":\"A\",\"zodiacMode\":\"SIDEREAL\",\"ayanamsha\":\"LAHIRI\"}}]";
        let s = parse_rsc_settings(line).expect("settings parsed");
        assert_eq!(s.house_system, HouseSystem::Alcabitius);
        assert_eq!(s.zodiac, Zodiac::Lahiri);
    }

    #[test]
    fn parse_settings_absent_returns_none() {
        assert!(parse_rsc_settings("garbage line with no settings").is_none());
    }

    #[test]
    fn astrotheoros_auth_failure_classification() {
        assert!(AstrotheorosError::ClerkAuthFailed("x".into()).is_auth_failure());
        assert!(AstrotheorosError::JwtRefreshFailed("x".into()).is_auth_failure());
        assert!(AstrotheorosError::ClerkIdentifyFailed("x".into()).is_auth_failure());
        // Not credential problems:
        assert!(!AstrotheorosError::CreateResponseInvalid.is_auth_failure());
        assert!(!AstrotheorosError::AtlasResponseInvalid.is_auth_failure());
        assert!(!AstrotheorosError::InvalidCoordinate("x".into()).is_auth_failure());
    }

    #[test]
    #[ignore = "requires ASTROGRAM_ASTROTHEOROS_USER/PASS and network"]
    fn probe_live_smoke() {
        let (Ok(user), Ok(pass)) = (
            std::env::var("ASTROGRAM_ASTROTHEOROS_USER"),
            std::env::var("ASTROGRAM_ASTROTHEOROS_PASS"),
        ) else {
            eprintln!("ASTROGRAM_ASTROTHEOROS_USER/PASS unset — skipping live probe");
            return;
        };
        let Ok(session) = AstrotheorosSession::login(&user, &pass, 0, "test/1.0") else {
            eprintln!("login failed (offline?) — skipping");
            return;
        };
        session
            .probe()
            .expect("probe of a fresh login session should succeed");
    }

    #[test]
    #[ignore = "requires ASTROGRAM_ASTROTHEOROS_USER/PASS and network"]
    fn login_session_can_refresh_jwt() {
        // Regression: a login session must be able to refresh its 60 s JWT.
        // `login` captures the `__client` cookie, so the forced refresh below
        // (which mimics a long-running session crossing the expiry threshold)
        // must authenticate — before the fix it 401'd because `__client` was
        // only in the cookie jar, which the refresh's explicit Cookie header
        // suppresses.
        let (Ok(user), Ok(pass)) = (
            std::env::var("ASTROGRAM_ASTROTHEOROS_USER"),
            std::env::var("ASTROGRAM_ASTROTHEOROS_PASS"),
        ) else {
            eprintln!("ASTROGRAM_ASTROTHEOROS_USER/PASS unset — skipping live refresh test");
            return;
        };
        let Ok(session) = AstrotheorosSession::login(&user, &pass, 0, "test/1.0") else {
            eprintln!("login failed (offline?) — skipping");
            return;
        };
        assert!(
            session.client_cookie.is_some(),
            "login should capture the __client cookie for refresh"
        );
        session.force_refresh.set(true);
        session
            .refresh_jwt_if_needed()
            .expect("a login session must be able to refresh its JWT");
    }
}

/// Wire-fidelity regression tests: prove that the User-Agent string we build is
/// the exact `User-Agent` header a server receives. The missing link in the rest
/// of the suite, which only covers UA *construction*. Hermetic — a one-shot
/// loopback HTTP server captures the header; nothing leaves the host, and no
/// network/echo capability is added to shipping code.
#[cfg(test)]
mod wire_ua_tests {
    use super::*;

    /// Send a real `build_client(ua)` GET to a one-shot loopback server and
    /// return the `User-Agent` header value that server actually received.
    fn ua_seen_on_wire(ua: &str) -> String {
        use std::io::{Read, Write};
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let addr = listener.local_addr().expect("addr");
        let server = std::thread::spawn(move || {
            let (mut sock, _) = listener.accept().expect("accept");
            // Read until end-of-headers (a no-body GET ends with CRLFCRLF).
            let mut buf = Vec::new();
            let mut chunk = [0u8; 1024];
            loop {
                match sock.read(&mut chunk) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        buf.extend_from_slice(&chunk[..n]);
                        if buf.windows(4).any(|w| w == b"\r\n\r\n") {
                            break;
                        }
                    }
                }
            }
            let _ = sock
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
            String::from_utf8_lossy(&buf).into_owned()
        });

        AstrotheorosSession::build_client(ua)
            .expect("build client")
            .get(format!("http://{addr}/"))
            .send()
            .expect("loopback send")
            .error_for_status()
            .expect("loopback 200");

        let request = server.join().expect("server thread");
        request
            .lines()
            .filter_map(|l| l.split_once(':'))
            .find(|(name, _)| name.trim().eq_ignore_ascii_case("user-agent"))
            .map(|(_, v)| v.trim().to_string())
            .unwrap_or_default()
    }

    #[test]
    fn build_client_transmits_user_agent_verbatim() {
        // Whatever UA we set is exactly what the wire carries — proving the
        // string->wire link that the construction unit tests cannot.
        let sentinel = "X-Sentinel/9.9 (wire-fidelity-probe)";
        assert_eq!(ua_seen_on_wire(sentinel), sentinel);
    }

    #[test]
    fn default_self_reported_ua_reaches_wire_and_is_not_a_browser() {
        // The honest default UA round-trips intact and carries no browser
        // markers — the regression guard for keeping cookie *read* access
        // decoupled from UA *impersonation*. If a future change ever puts a
        // browser UA on a default run, the negative assertions fail loudly.
        let app = crate::user_agent::AppProduct::new("Blackmoon", "0.2.2");
        let honest = crate::user_agent::self_reported(&app);

        let seen = ua_seen_on_wire(&honest);
        assert_eq!(seen, honest, "honest UA must reach the wire verbatim");
        assert!(seen.starts_with("Mozilla/5.0 Blackmoon/"), "{seen}");
        assert!(seen.contains("Astrogram/"), "{seen}");
        for marker in ["Safari/", "Chrome/", "Firefox/", "Gecko", "AppleWebKit"] {
            assert!(
                !seen.contains(marker),
                "honest default UA leaked browser marker {marker:?}: {seen}"
            );
        }
    }
}
