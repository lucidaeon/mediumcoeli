use crate::capability::{CapabilitySet, ChartField};
use crate::chart::{Chart, EventType};
use crate::luna::USER_AGENT;
use reqwest::blocking::Client;
use scraper::{Html, Selector};
use std::time::Duration;

/// Fields recovered when reading an astro.com account chart.
///
/// astro.com exports charts via AAF, which carries region (the country
/// sub-locality field).  Event type is stored in the `ssx`/`btyp` form fields
/// on write but the AAF export format does not return it — event type is always
/// [`crate::chart::EventType::Unspecified`] on read.
pub const READ_CAPS: CapabilitySet = CapabilitySet::new(&[ChartField::Region]);

/// Fields persisted when writing an astro.com account chart.
///
/// The `ssx` form field carries [`crate::chart::EventType`] (male/female/event).
/// Region is not a writable field — astro.com resolves the country via its own
/// atlas from the city name; the `region` value in the source [`Chart`] is not sent.
pub const WRITE_CAPS: CapabilitySet = CapabilitySet::new(&[ChartField::EventType]);

/// Base URL for the astro.com web application.
pub const ASTROCOM_URL: &str = "https://www.astro.com";
/// Login page URL — shows the register/login form; also sets a temporary `cid`.
pub const LOGIN_PAGE: &str = "https://www.astro.com/cgi/scus.cgi?act=lgi";
/// Login POST endpoint.
pub const LOGIN_POST: &str = "https://www.astro.com/cgi/scus.cgi";
/// Account data (listing, delete, swap) endpoint.
pub const AWD_URL: &str = "https://www.astro.com/cgi/awd.cgi";

/// Errors specific to astro.com HTTP sessions.
#[derive(Debug, thiserror::Error)]
pub enum AstrocomError {
    /// An HTTP request failed.
    #[error("HTTP: {0}")]
    Http(#[from] reqwest::Error),
    /// The `reqwest` client could not be constructed.
    #[error("HTTP client build error: {0}")]
    HttpClientBuild(String),
    /// Login failed — credentials rejected or cid not found in response.
    #[error("login failed — check credentials (final URL: {0})")]
    LoginFailed(String),
    /// No `<pre>` block in the AAF export response — session may be invalid.
    #[error("no <pre> block in AAF response — session cookie may be invalid")]
    AafNotFound,
    /// The `nhor` ID could not be extracted from the create response.
    #[error("could not extract nhor ID from create response")]
    NhorNotFound,
    /// The `unid_token` was not found in the listing page.
    #[error("could not find unid_token in listing page")]
    UnidTokenNotFound,
    /// Deletion verification failed — some nhor IDs still present.
    #[error("delete failed — nhor IDs still present: {0:?}")]
    DeleteVerifyFailed(Vec<u32>),
    /// AAF parse error.
    #[error("AAF parse error: {0}")]
    AafParse(String),
}

/// Authenticated HTTP session for an astro.com account.
pub struct AstrocomSession {
    client: Client,
    cid: String,
    delay: Duration,
}

impl AstrocomSession {
    fn build_client(cid: &str) -> Result<Client, AstrocomError> {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::COOKIE,
            reqwest::header::HeaderValue::from_str(&format!("cid={cid}"))
                .map_err(|e| AstrocomError::HttpClientBuild(e.to_string()))?,
        );
        headers.insert(
            reqwest::header::ACCEPT,
            reqwest::header::HeaderValue::from_static(
                "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
            ),
        );
        headers.insert(
            reqwest::header::ACCEPT_LANGUAGE,
            reqwest::header::HeaderValue::from_static("en-US,en;q=0.9"),
        );
        Client::builder()
            .default_headers(headers)
            .user_agent(USER_AGENT)
            .http1_only()
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(|e| AstrocomError::HttpClientBuild(e.to_string()))
    }

    /// Build a session from a known `cid` cookie value.
    ///
    /// # Errors
    /// - [`AstrocomError::HttpClientBuild`] if the `cid` value produces an invalid
    ///   cookie header or the `reqwest::Client` cannot be constructed.
    pub fn from_cid(cid: &str, delay_ms: u64) -> Result<Self, AstrocomError> {
        Ok(Self {
            client: Self::build_client(cid)?,
            cid: cid.to_string(),
            delay: Duration::from_millis(delay_ms),
        })
    }

    /// Log in with email + password; returns an authenticated session.
    ///
    /// Two-step flow confirmed by live inspection (2026-06-07):
    ///   1. GET `LOGIN_PAGE` — extract temp `cid` from hidden form field.
    ///   2. POST `LOGIN_POST` with credentials + temp cid.
    ///   3. Extract real cid from redirect URL (`;;cid=<value>`) or hidden field.
    ///
    /// # Errors
    /// - [`AstrocomError::HttpClientBuild`] if the anonymous `reqwest::Client`
    ///   cannot be constructed.
    /// - [`AstrocomError::Http`] if the login GET or POST returns a network error
    ///   or a non-2xx status.
    /// - [`AstrocomError::LoginFailed`] if the server responds with 2xx but no
    ///   valid `cid` appears in the redirect URL or response body.
    ///
    /// # Panics
    /// Panics if the CSS selector literal `input[name="cid"]` is invalid —
    /// this is a compile-time constant and cannot happen in practice.
    pub fn login(email: &str, pass: &str, delay_ms: u64) -> Result<Self, AstrocomError> {
        let anon_client = Client::builder()
            .user_agent(USER_AGENT)
            .http1_only()
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(|e| AstrocomError::HttpClientBuild(e.to_string()))?;

        let page_html = anon_client
            .get(LOGIN_PAGE)
            .send()?
            .error_for_status()?
            .text()?;
        let temp_cid = {
            let doc = Html::parse_document(&page_html);
            let sel = Selector::parse(r#"input[name="cid"]"#).unwrap();
            doc.select(&sel)
                .next()
                .and_then(|n| n.value().attr("value"))
                .unwrap_or("")
                .to_string()
        };

        let payload = login_payload(email, pass, &temp_cid);
        let resp = anon_client
            .post(LOGIN_POST)
            .form(&payload)
            .send()?
            .error_for_status()?;
        let final_url = resp.url().as_str().to_string();
        let body = resp.text()?;

        let cid = if let Some(c) = extract_cid_from_url(&final_url) {
            c.to_string()
        } else {
            let doc = Html::parse_document(&body);
            let sel = Selector::parse(r#"input[name="cid"]"#).unwrap();
            doc.select(&sel)
                .next()
                .and_then(|n| n.value().attr("value"))
                .filter(|v| !v.is_empty() && v.contains('-'))
                .map(str::to_string)
                .ok_or_else(|| AstrocomError::LoginFailed(final_url))?
        };

        Self::from_cid(&cid, delay_ms)
    }

    fn get_text(&self, url: &str) -> Result<String, AstrocomError> {
        Ok(self.client.get(url).send()?.error_for_status()?.text()?)
    }

    fn sleep(&self) {
        if !self.delay.is_zero() {
            std::thread::sleep(self.delay);
        }
    }

    /// Returns the `cid` session cookie (needed for create payloads).
    #[must_use]
    pub fn cid(&self) -> &str {
        &self.cid
    }

    /// Fetch all charts from the account via the AAF bulk export.
    ///
    /// Returns `(charts, nhor_ids)` parallel vecs.  `nhor_ids[i]` is the
    /// integer chart ID for `charts[i]`; `0` means no match (chart will be
    /// created, not edited, on write-back).
    ///
    /// # Errors
    /// - [`AstrocomError::Http`] if the listing or AAF export request fails.
    /// - [`AstrocomError::AafNotFound`] if the export response contains no `<pre>` block
    ///   (session cookie may be expired).
    /// - [`AstrocomError::AafParse`] if the extracted AAF text cannot be parsed.
    pub fn fetch_charts(&self) -> Result<(Vec<crate::chart::Chart>, Vec<u32>), AstrocomError> {
        use crate::normalize::normalize_chart;
        use std::collections::HashMap;

        let list_html = self.get_text(&format!("{AWD_URL}?lang=e"))?;
        let listing = parse_listing(&list_html);
        let name_to_nhor: HashMap<String, u32> = listing
            .into_iter()
            .map(|l| (l.name.to_lowercase(), l.nhor_id))
            .collect();
        self.sleep();

        let aaf_html = self.get_text(&format!("{AWD_URL}?lang=e&act=aaf"))?;
        let aaf_text = extract_aaf(&aaf_html).ok_or(AstrocomError::AafNotFound)?;
        let charts = crate::aaf::parse_file(&aaf_text)
            .map_err(|e| AstrocomError::AafParse(e.to_string()))?;

        let (charts_out, nhor_ids_out) = charts
            .into_iter()
            .map(|mut chart| {
                normalize_chart(&mut chart);
                let name_lc = chart.name.to_lowercase();
                let id = name_to_nhor
                    .get(&name_lc)
                    .or_else(|| {
                        if let Some(pos) = chart.name.find(", ") {
                            name_to_nhor.get(&chart.name[pos + 2..].to_lowercase())
                        } else {
                            None
                        }
                    })
                    .copied()
                    .unwrap_or(0);
                (chart, id)
            })
            .unzip();

        Ok((charts_out, nhor_ids_out))
    }

    /// Write new charts to astro.com (skips charts with a non-zero `nhor_id`).
    ///
    /// - `on_start(current, total, name)`: called before each create.
    /// - `on_result(status)`: called after with `"created nhor=N"` or `"[!] …"`.
    ///
    /// # Errors
    /// Always returns `Ok(())`; per-chart failures are reported via the
    /// `on_result` closure as `"[!] create: …"` strings rather than propagated.
    pub fn write_charts(
        &self,
        charts: &[crate::chart::Chart],
        nhor_ids: &[u32],
        on_start: &dyn Fn(usize, usize, &str),
        on_result: &dyn Fn(&str),
    ) -> Result<(), AstrocomError> {
        let new_charts: Vec<_> = charts
            .iter()
            .zip(nhor_ids.iter())
            .filter(|(_, id)| **id == 0)
            .collect();
        let total = new_charts.len();
        for (i, (chart, _)) in new_charts.iter().enumerate() {
            on_start(
                i + 1,
                total,
                &chart.name.chars().take(40).collect::<String>(),
            );
            match self.create_one(chart) {
                Ok(id) => on_result(&format!("created nhor={id}")),
                Err(e) => on_result(&format!("[!] create: {e}")),
            }
            self.sleep();
        }
        Ok(())
    }

    /// Create a single chart on astro.com.  Returns the `nhor` ID.
    ///
    /// # Errors
    /// - [`AstrocomError::Http`] if any HTTP request fails.
    /// - [`AstrocomError::NhorNotFound`] if the `nhor` ID cannot be extracted from
    ///   the create response URL or body.
    pub fn create_one(&self, chart: &crate::chart::Chart) -> Result<u32, AstrocomError> {
        let form_url = format!("{ASTROCOM_URL}/cgi/ade.cgi?lang=e");
        let post_url = format!("{ASTROCOM_URL}/cgi/ade.cgi");

        let form_html = self.get_text(&form_url)?;
        let sprev = extract_sprev(&form_html).unwrap_or_default();

        // Resolve the city via the autocomplete API, mirroring the browser's JS flow:
        // the user selects from autocomplete which sets `scit` to the label and `spli`
        // to the atlas identifier.  Response format: "label|spli_value||count"
        let city_q = chart.city.as_deref().unwrap_or("").trim();
        let (scit_label, spli_val) = if city_q.is_empty() {
            (chart.city.clone().unwrap_or_default(), None)
        } else {
            let ac_url = format!(
                "{ASTROCOM_URL}/cgi/adejs.cgi?func=place_query&q={}&sctr=&lang=e",
                urlencoding_simple(city_q)
            );
            let raw = self.get_text(&ac_url)?;
            let first = raw.lines().next().unwrap_or("");
            let mut parts = first.splitn(2, '|');
            let label = parts.next().unwrap_or(city_q).trim().to_string();
            let rest = parts.next().unwrap_or("");
            let spli = rest.split("||").next().unwrap_or("").to_string();
            if spli.is_empty() {
                (city_q.to_string(), None)
            } else {
                (label, Some(spli))
            }
        };

        // Build the payload matching what the browser submits after autocomplete:
        // scit = autocomplete label, spli = atlas identifier, js = true, sprev = ""
        let mut payload = create_payload(chart, &self.cid, &sprev);
        if let Some(pos) = payload.iter().position(|(k, _)| k == "scit") {
            payload[pos].1 = scit_label;
        }
        payload.push(("js".into(), "true".into()));
        if let Some(ref spli) = spli_val {
            payload.push(("spli".into(), spli.clone()));
        }

        let resp = self
            .client
            .post(&post_url)
            .form(&payload)
            .send()?
            .error_for_status()?;
        let final_url = resp.url().as_str().to_string();
        let body = resp.text()?;

        if let Some(nhor) = parse_nhor_from_url(&final_url).or_else(|| parse_nhor_from_url(&body)) {
            return Ok(nhor);
        }

        // Server returned the confirmation form (sprev now has embedded city data).
        // Re-submit mirroring the form state the server returned.
        let conf_sprev = extract_sprev(&body).unwrap_or_default();
        let conf_spli = extract_spli_options(&body);
        let conf_extset = extract_hidden_field(&body, "extset").unwrap_or_else(|| "close".into());
        let mut confirm_payload = create_payload(chart, &self.cid, &conf_sprev);
        if let Some(pos) = confirm_payload.iter().position(|(k, _)| k == "extset") {
            confirm_payload[pos].1 = conf_extset;
        }
        if !conf_spli.is_empty() {
            confirm_payload.push(("spli".into(), conf_spli[0].clone()));
        }
        confirm_payload.push(("js".into(), "true".into()));
        let resp2 = self
            .client
            .post(&post_url)
            .form(&confirm_payload)
            .send()?
            .error_for_status()?;
        let url2 = resp2.url().as_str().to_string();
        let body2 = resp2.text()?;
        parse_nhor_from_url(&url2)
            .or_else(|| parse_nhor_from_url(&body2))
            .ok_or(AstrocomError::NhorNotFound)
    }

    /// Delete charts by nhor ID.  Verifies deletion and errors if any remain.
    ///
    /// # Errors
    /// - [`AstrocomError::Http`] if any HTTP request fails.
    /// - [`AstrocomError::UnidTokenNotFound`] if the `unid_token` is absent from
    ///   the listing page (session cookie may be expired).
    /// - [`AstrocomError::DeleteVerifyFailed`] if any of the requested `nhor` IDs
    ///   are still present after deletion completes.
    pub fn delete_charts(
        &self,
        email: &str,
        pass: &str,
        nhor_ids: &[u32],
    ) -> Result<(), AstrocomError> {
        let listing_html = self.get_text(&format!("{AWD_URL}?lang=e"))?;
        let token = extract_unid_token(&listing_html).ok_or(AstrocomError::UnidTokenNotFound)?;

        let mut params = format!(
            "{AWD_URL}?act=del&conf=1&lang=e&unid_token={}",
            urlencoding_simple(&token)
        );
        for &id in nhor_ids {
            use std::fmt::Write;
            write!(params, "&del{id}=on").unwrap();
        }
        self.get_text(&params)?;
        self.sleep();

        let payload = delete_payload(email, pass, nhor_ids);
        self.client
            .post(AWD_URL)
            .form(&payload)
            .send()?
            .error_for_status()?;
        self.sleep();

        let listing2 = self.get_text(&format!("{AWD_URL}?lang=e"))?;
        let remaining = parse_listing(&listing2);
        let still_present: Vec<u32> = nhor_ids
            .iter()
            .filter(|&&id| remaining.iter().any(|l| l.nhor_id == id))
            .copied()
            .collect();
        if still_present.is_empty() {
            Ok(())
        } else {
            Err(AstrocomError::DeleteVerifyFailed(still_present))
        }
    }
}

fn urlencoding_simple(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
            _ => format!("%{:02X}", c as u32),
        })
        .collect()
}

/// Extract the `cid` value from an astro.com redirect URL.
///
/// After login, astro.com redirects to a URL like:
/// `/cgi/awd.cgi?lang=e;;cid=<value>` (double-semicolon is intentional).
/// The cid also appears as a hidden `<input name="cid">` in the response body.
#[must_use]
pub fn extract_cid_from_url(url: &str) -> Option<&str> {
    let start = url.find("cid=")? + "cid=".len();
    let rest = &url[start..];
    let end = rest.find(['&', ';', ' ']).unwrap_or(rest.len());
    let val = &rest[..end];
    if val.is_empty() { None } else { Some(val) }
}

/// Build the login form payload for `POST /cgi/scus.cgi`.
///
/// Field names and values confirmed by live inspection of `/cgi/scus.cgi?act=lgi`.
/// The `cid` field must come from a prior GET of the login page — it's a
/// server-generated temporary session token that gets upgraded after auth.
#[must_use]
pub fn login_payload(email: &str, pass: &str, temp_cid: &str) -> Vec<(String, String)> {
    vec![
        ("eml".into(), email.to_string()),
        ("eml1".into(), email.to_string()),
        ("pwd".into(), pass.to_string()),
        ("tit".into(), String::new()),
        ("fnm".into(), String::new()),
        ("nam".into(), String::new()),
        ("ctr".into(), String::new()),
        ("lan".into(), String::new()),
        ("sec".into(), String::new()),
        ("lang".into(), "e".to_string()),
        ("cid".into(), temp_cid.to_string()),
        ("submit".into(), "Login".to_string()),
    ]
}

/// Extract the `unid_token` CSRF value from a JS assignment in an authenticated page.
///
/// Embedded as: `var unid_token = '<nonce>,<unix-ts>,<cid>,<hmac>';`
#[must_use]
pub fn extract_unid_token(html: &str) -> Option<String> {
    let start = html.find("unid_token")?;
    let quote_start = html[start..].find('\'')? + start + 1;
    let quote_end = html[quote_start..].find('\'')? + quote_start;
    let token = &html[quote_start..quote_end];
    if token.is_empty() {
        None
    } else {
        Some(token.to_string())
    }
}

/// Build the Step-2 delete confirmation POST payload.
///
/// `email`: account email (echoed in the step-1 hidden `mail` field).
/// `password`: account password.
/// `nhor_ids`: chart IDs to delete (batch).
#[must_use]
pub fn delete_payload(email: &str, password: &str, nhor_ids: &[u32]) -> Vec<(String, String)> {
    let mut fields = vec![
        ("act".into(), "del".into()),
        ("mail".into(), email.to_string()),
        ("pwrd".into(), password.to_string()),
        ("delnow".into(), "Yes".into()),
    ];
    for &id in nhor_ids {
        fields.push((format!("del{id}"), "on".into()));
    }
    fields
}

/// A chart entry from the astro.com account page.
pub struct AstrocomListing {
    /// astro.com internal chart ID (`nhor` URL parameter).
    pub nhor_id: u32,
    /// Chart name as shown in the account listing.
    pub name: String,
}

/// Parse the nhor IDs and chart names from an `awd.cgi` account page.
///
/// astro.com does not use a `<select name="nhor">` dropdown; instead each
/// chart row has an Edit link of the form:
/// `<a href="/cgi/ade.cgi?&nhor=N&ract=..." title="edit birth data for Name">Edit</a>`
#[must_use]
pub fn parse_listing(html: &str) -> Vec<AstrocomListing> {
    let doc = Html::parse_document(html);
    let Ok(sel) = Selector::parse("a") else {
        return Vec::new();
    };
    doc.select(&sel)
        .filter_map(|a| {
            let href = a.value().attr("href")?;
            if !href.contains("/cgi/ade.cgi") {
                return None;
            }
            let nhor_id = parse_nhor_from_url(href)?;
            let title = a.value().attr("title")?;
            let name = title
                .trim()
                .strip_prefix("edit birth data for ")?
                .trim()
                .to_string();
            if name.is_empty() {
                return None;
            }
            Some(AstrocomListing { nhor_id, name })
        })
        .collect()
}

/// Extract the value of the `<input name="sprev">` field from a `ade.cgi` form page.
///
/// `sprev` is a server-side form-state token that must be echoed back on
/// create POST submissions; the server rejects submissions that omit it.
pub fn extract_sprev(html: &str) -> Option<String> {
    let doc = Html::parse_document(html);
    let sel = Selector::parse(r#"input[name="sprev"]"#).ok()?;
    doc.select(&sel)
        .next()?
        .value()
        .attr("value")
        .map(String::from)
}

/// Extract the value of any `<input type="hidden" name="…">` field.
#[must_use]
pub fn extract_hidden_field(html: &str, name: &str) -> Option<String> {
    let doc = Html::parse_document(html);
    let selector = format!(r#"input[name="{name}"]"#);
    let sel = Selector::parse(&selector).ok()?;
    doc.select(&sel)
        .next()?
        .value()
        .attr("value")
        .map(String::from)
}

/// Extract the value options from the `<select name="spli">` atlas-results
/// dropdown, returned when a city search yields multiple matches.  The first
/// option is the best match; subsequent options are alternatives.
#[must_use]
pub fn extract_spli_options(html: &str) -> Vec<String> {
    let doc = Html::parse_document(html);
    let Ok(sel) = Selector::parse(r#"select[name="spli"] option"#) else {
        return Vec::new();
    };
    doc.select(&sel)
        .filter_map(|opt| opt.value().attr("value").map(String::from))
        .collect()
}

/// Extract the text content of the first `<pre>` block from an astro.com
/// AAF export response.  `scraper` automatically decodes HTML entities.
#[must_use]
pub fn extract_aaf(html: &str) -> Option<String> {
    let doc = Html::parse_document(html);
    let sel = Selector::parse("pre").ok()?;
    let pre = doc.select(&sel).next()?;
    Some(pre.text().collect())
}

/// Extract the `nhor` integer from an astro.com redirect URL such as
/// `/cgi/awd.cgi?;nhor=5` or `/cgi/awd.cgi?lang=e&nhor=12`.
#[must_use]
pub fn parse_nhor_from_url(url: &str) -> Option<u32> {
    let start = url.find("nhor=")? + "nhor=".len();
    url[start..]
        .split(['&', ';', ' ', '#'])
        .next()?
        .parse()
        .ok()
}

/// Convert a UTC offset (hours, ISO 6709 East-positive) to the astro.com
/// `szon` field format.
///
/// | Input         | Output     |
/// |---------------|------------|
/// | `is_lmt=true` | `"lmt"`    |
/// | `−8.0`        | `"h8w"`    |
/// | `+5.5`        | `"h5e30"`  |
/// | `0.0`         | `"h0e"`    |
#[must_use]
pub fn offset_to_szon(offset: f64, is_lmt: bool) -> String {
    if is_lmt {
        return "lmt".to_string();
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    // offset is a UTC offset in hours; abs() is ≤ 14 → total_mins ≤ 840, fits u32
    let total_mins = (offset.abs() * 60.0).round() as u32;
    let hours = total_mins / 60;
    let mins = total_mins % 60;
    let hemi = if offset < 0.0 { 'w' } else { 'e' };
    if mins == 0 {
        format!("h{hours}{hemi}")
    } else {
        format!("h{hours}{hemi}{mins:02}")
    }
}

/// Build the form payload for creating a new chart via `POST /cgi/ade.cgi`.
///
/// **Coordinate caveat:** astro.com resolves coordinates from `scit` (city
/// name) via its server-side atlas.  Raw lat/lon are not accepted on create.
/// The resulting chart position may differ slightly from `chart.longitude` /
/// `chart.latitude`.
/// Build the form payload for creating a new chart via `POST /cgi/ade.cgi`.
///
/// `sprev` must come from a prior GET of the create form (`ade.cgi?lang=e`);
/// the server uses it as form-state validation and rejects submissions without
/// a matching token.
///
/// **Coordinate caveat:** astro.com resolves coordinates from `scit` (city
/// name) via its server-side atlas.  Raw lat/lon are not accepted on create.
#[must_use]
pub fn create_payload(chart: &Chart, cid: &str, sprev: &str) -> Vec<(String, String)> {
    let (last, first) = split_name(&chart.name);
    let ssx = match chart.event_type {
        EventType::Male => "m",
        EventType::Female => "f",
        _ => "e",
    };
    let szon = offset_to_szon(chart.tz_offset_hours, chart.is_lmt);

    vec![
        ("sfnm".into(), first),
        ("snam".into(), last),
        ("ssx".into(), ssx.into()),
        ("sown".into(), "n".into()),
        ("sday".into(), chart.day.to_string()),
        ("imon".into(), chart.month.to_string()),
        ("syar".into(), chart.year.to_string()),
        ("ihou".into(), chart.hour.to_string()),
        ("smin".into(), chart.minute.to_string()),
        ("sctr".into(), String::new()),
        ("scit".into(), chart.city.clone().unwrap_or_default()),
        ("szon".into(), szon),
        ("lang".into(), "e".into()),
        ("extset".into(), "close".into()),
        ("btyp".into(), "w2at".into()),
        ("sprev".into(), sprev.to_string()),
        ("cid".into(), cid.into()),
        ("subcon".into(), "continue".into()),
    ]
}

/// Build the form payload for editing an existing chart via `POST /cgi/ade.cgi`.
///
/// Edits do not need a sprev token — the server identifies the chart by nhor.
#[must_use]
pub fn edit_payload(chart: &Chart, nhor_id: u32, cid: &str) -> Vec<(String, String)> {
    let (last, first) = split_name(&chart.name);
    let ssx = match chart.event_type {
        EventType::Male => "m",
        EventType::Female => "f",
        _ => "e",
    };
    let szon = offset_to_szon(chart.tz_offset_hours, chart.is_lmt);
    vec![
        ("sfnm".into(), first),
        ("snam".into(), last),
        ("ssx".into(), ssx.into()),
        ("sday".into(), chart.day.to_string()),
        ("imon".into(), chart.month.to_string()),
        ("syar".into(), chart.year.to_string()),
        ("ihou".into(), chart.hour.to_string()),
        ("smin".into(), chart.minute.to_string()),
        ("scit".into(), chart.city.clone().unwrap_or_default()),
        ("szon".into(), szon),
        ("lang".into(), "e".into()),
        ("extset".into(), "close".into()),
        ("btyp".into(), "w2at".into()),
        ("cid".into(), cid.into()),
        ("nhor".into(), nhor_id.to_string()),
        ("subcon".into(), "continue".into()),
    ]
}

/// Split a chart name into `(snam/last, sfnm/first)` for the ade.cgi form.
///
/// astro.com requires `sfnm` (first name) to be non-empty for all chart types.
/// Names with a comma follow `"Last, First"` convention; names without a comma
/// are treated as a single given name and placed entirely in `sfnm`.
fn split_name(name: &str) -> (String, String) {
    if let Some(pos) = name.find(',') {
        let last = name[..pos].trim().to_string();
        let first = name[pos + 1..].trim().to_string();
        (last, first)
    } else {
        // No comma → single-word name (e.g. "Madonna") or compound event name
        // (e.g. "Lightning Strike"). astro.com requires sfnm non-empty; last is optional.
        (String::new(), name.trim().to_string())
    }
}
