// `import_blackmoon` and `import_charts_from` are large CLI dispatchers;
// splitting them produces worse code than the lint resolves.
#![allow(clippy::too_many_lines)]
// clap collects /// comments on Cli/args as user-facing --help text; adding
// rustdoc-style backticks here would surface as literal characters in output.
#![allow(clippy::doc_markdown)]

use anyhow::{Context, Result, bail};
use astrogram::consolidate;
use astrogram::cookie_import::Browser;
use astrogram::format::{Format, Kind};
use astrogram::normalize::normalize_chart;
use astrogram::util::{expand_now, utc_timestamp};
use clap::Parser;
use std::collections::HashMap;
use std::io::Write as _;
use std::path::{Path, PathBuf};

pub use astrogram::format::Format as Target;

mod consolidate_ui;
mod exit;
mod path;
mod providers;
use path::display_path;
use providers::WebProvider;

// ── format value parser ───────────────────────────────────────────────────────

/// One line of tab-completion help for a format slug: kind, read/write
/// direction, and credential shape (e.g. `"file, read+write, no auth"`).
/// Built from the `FormatSpec` — the single source of truth — so it can never
/// drift from the registry the way a hand-maintained help string could.
fn format_help(spec: &astrogram::format::FormatSpec) -> String {
    use astrogram::format::{Auth, Kind};
    let kind = match spec.kind {
        Kind::File => "file",
        Kind::Web => "web",
    };
    let direction = match (spec.can_read, spec.can_write) {
        (true, true) => "read+write",
        (true, false) => "read-only",
        (false, true) => "write-only",
        (false, false) => "unusable",
    };
    let auth = match spec.auth {
        Auth::None => "no auth",
        Auth::Token => "token auth",
        Auth::LoginOrToken => "login or token auth",
    };
    format!("{kind}, {direction}, {auth}")
}

/// Value parser for `--from`/`--to`/`--target`: validates against the format
/// registry and exposes every slug as a clap *possible value* with per-value
/// help, so shells tab-complete them (and `--help` lists them). `FORMATS`
/// stays the single source of truth — the candidate set (and its help text)
/// is read from it at command-build time, so a newly registered format
/// completes with no further wiring.
fn format_parser() -> impl clap::builder::TypedValueParser {
    use clap::builder::{PossibleValue, PossibleValuesParser, TypedValueParser as _};
    let values: Vec<PossibleValue> = Format::all()
        .iter()
        .map(|spec| PossibleValue::new(spec.slug).help(format_help(spec)))
        .collect();
    // The parser only yields values already validated against the slugs it
    // was built from, so `from_slug` cannot miss.
    PossibleValuesParser::new(values)
        .map(|s| Format::from_slug(&s).expect("possible value is a registered slug"))
}

// ── fill-value parser ──────────────────────────────────────────────────────────

/// Value parser for `--fill-house`/`--fill-zodiac`/`--fill-locus`: surfaces
/// `accepted_slugs()` as clap *possible values* (so `--help` lists them and
/// shells tab-complete) but does NOT reject a value at parse time. The raw
/// `String` is passed through unchanged; validation happens later, where the
/// value is actually consulted (`resolve_fill`), so a supplied-but-invalid
/// slug fails as bad *input* ([`exit::InputError`], exit 3) rather than a
/// structural clap usage error (exit 2) — and only when the fill is genuinely
/// needed. The `accepted` list drives completion/`--help` only; actual
/// acceptance is wider than it enumerates (case-insensitivity, `_`-to-`-`
/// folding, aliases like `"whole"`/`"geo"`), which is exactly why this parser
/// must not gate on the list.
///
/// `describe` supplies the per-slug accurate domain description (from
/// `astrogram::chart`, adjacent to `accepted_slugs()`) rendered as each
/// candidate's completion/`--help` text; a slug with no description (the
/// three Solar-Fire-specific house codes with no documented semantics) still
/// completes, just without help text.
#[derive(Clone)]
struct FillValueParser {
    accepted: &'static [&'static str],
    describe: fn(&str) -> Option<&'static str>,
}

impl clap::builder::TypedValueParser for FillValueParser {
    type Value = String;

    fn parse_ref(
        &self,
        _cmd: &clap::Command,
        _arg: Option<&clap::Arg>,
        value: &std::ffi::OsStr,
    ) -> Result<Self::Value, clap::Error> {
        // Accept any string; `resolve_fill` validates against `from_str_slug`
        // and classifies a bad value as `exit::InputError` (exit 3).
        Ok(value.to_string_lossy().into_owned())
    }

    fn possible_values(
        &self,
    ) -> Option<Box<dyn Iterator<Item = clap::builder::PossibleValue> + '_>> {
        Some(Box::new(self.accepted.iter().map(|s| {
            let pv = clap::builder::PossibleValue::new(*s);
            match (self.describe)(s) {
                Some(help) => pv.help(help),
                None => pv,
            }
        })))
    }
}

fn fill_house_parser() -> impl clap::builder::TypedValueParser {
    FillValueParser {
        accepted: astrogram::chart::HouseSystem::accepted_slugs(),
        describe: astrogram::chart::HouseSystem::slug_description,
    }
}

fn fill_zodiac_parser() -> impl clap::builder::TypedValueParser {
    FillValueParser {
        accepted: astrogram::chart::Zodiac::accepted_slugs(),
        describe: astrogram::chart::Zodiac::slug_description,
    }
}

fn fill_locus_parser() -> impl clap::builder::TypedValueParser {
    FillValueParser {
        accepted: astrogram::chart::CoordinateSystem::accepted_slugs(),
        describe: astrogram::chart::CoordinateSystem::slug_description,
    }
}

// ── cookie-import: browser value parser + grant-decision types ────────────────

/// The CLI slug for a `Browser` variant — the single source of truth both
/// [`parse_browser`] and the `--grant-cookie-access` completion candidates
/// build from, so the accepted strings and the tab-completed strings can
/// never drift apart. `wristband::Browser` carries a human [`Browser::display_name`]
/// but no CLI slug of its own, hence this local mapping.
///
/// `Browser` is `#[non_exhaustive]` (wristband may add variants), so this
/// match needs a wildcard arm; it panics rather than silently mis-slugging a
/// browser this table hasn't been taught about yet.
fn browser_slug(b: Browser) -> &'static str {
    match b {
        Browser::Chrome => "chrome",
        Browser::Chromium => "chromium",
        Browser::Brave => "brave",
        Browser::Edge => "edge",
        Browser::Opera => "opera",
        Browser::Vivaldi => "vivaldi",
        Browser::Whale => "whale",
        Browser::Firefox => "firefox",
        Browser::Safari => "safari",
        other => unreachable!(
            "wristband::Browser grew a variant ({other:?}) with no CLI slug mapped in \
             browser_slug() — add one"
        ),
    }
}

/// Parse `--grant-cookie-access[=<browser>]` into a `GrantArg`.
///
/// - `"all"` → `GrantArg::All` (enumerate all installed stores)
/// - a browser name → `GrantArg::One(Browser::…)`
/// - anything else → `Err` with the list of valid names
fn parse_browser(s: &str) -> Result<GrantArg, String> {
    if s == "all" {
        return Ok(GrantArg::All);
    }
    if let Some(b) = Browser::all().iter().find(|b| browser_slug(**b) == s) {
        return Ok(GrantArg::One(*b));
    }
    let mut valid = vec!["all".to_string()];
    valid.extend(Browser::all().iter().map(|b| browser_slug(*b).to_string()));
    Err(format!(
        "unknown browser '{s}'; valid values: {}",
        valid.join(", ")
    ))
}

/// Value parser for `--grant-cookie-access[=<browser>]`: delegates to
/// [`parse_browser`] for acceptance (unchanged) but additionally advertises
/// `all` plus every `Browser` slug as clap *possible values*, each with the
/// browser's [`Browser::display_name`] as help, so shells tab-complete them.
/// The candidate set is read from `Browser::all()` — the single source of
/// truth in `wristband` — at command-build time.
#[derive(Clone)]
struct BrowserValueParser;

impl clap::builder::TypedValueParser for BrowserValueParser {
    type Value = GrantArg;

    fn parse_ref(
        &self,
        cmd: &clap::Command,
        _arg: Option<&clap::Arg>,
        value: &std::ffi::OsStr,
    ) -> Result<Self::Value, clap::Error> {
        let s = value.to_string_lossy();
        parse_browser(&s).map_err(|msg| {
            clap::Error::raw(clap::error::ErrorKind::InvalidValue, msg).with_cmd(cmd)
        })
    }

    fn possible_values(
        &self,
    ) -> Option<Box<dyn Iterator<Item = clap::builder::PossibleValue> + '_>> {
        let all = std::iter::once(
            clap::builder::PossibleValue::new("all")
                .help("search every installed browser's cookie store (also the bare-flag default)"),
        );
        let per_browser = Browser::all()
            .iter()
            .map(|b| clap::builder::PossibleValue::new(browser_slug(*b)).help(b.display_name()));
        Some(Box::new(all.chain(per_browser)))
    }
}

/// `--ua` value: the keyword `browser` opts into mimicking the cookie-source
/// browser; the bare flag selects the static spoof; any other string is verbatim.
#[derive(Clone, Debug)]
enum UaArg {
    /// `--ua browser`: mimic the cookie-source browser's own divined UA.
    Browser,
    /// Bare `--ua`: use the fixed static UA.
    Static,
    /// `--ua <string>`: send this verbatim.
    Custom(String),
}

/// Parse a `--ua` value. Empty (the `default_missing_value` for the bare flag)
/// selects `Static`; the keyword `browser` selects `Browser`; any other string
/// is `Custom`.
fn parse_ua(s: &str) -> Result<UaArg, String> {
    if s.is_empty() {
        Ok(UaArg::Static)
    } else if s.eq_ignore_ascii_case("browser") {
        Ok(UaArg::Browser)
    } else {
        Ok(UaArg::Custom(s.to_string()))
    }
}

/// Thin clap-level wrapper so clap's type system sees a single concrete type
/// for `--grant-cookie-access[=<browser>]` rather than a doubly-nested `Option`.
#[derive(Clone, Copy, Debug, PartialEq)]
enum GrantArg {
    /// `"all"` / bare flag — search every installed store.
    All,
    /// A specific browser slug.
    One(Browser),
}

/// The consent decision derived from `--grant-cookie-access[=<browser>]`.
///
/// - `None` (flag absent) → `NoGrant` — cookies are never touched.
/// - `Some(GrantArg::All)` (bare flag or `=all`) → `AllStores` — all installed stores.
/// - `Some(GrantArg::One(b))` (specific browser) → `One(b)` — that browser only.
#[derive(Debug, PartialEq)]
enum GrantChoice {
    /// Flag was absent — fall back to token/login auth.
    NoGrant,
    /// Bare `--grant-cookie-access` (or `=all`) — search all installed stores.
    AllStores,
    /// `--grant-cookie-access=<browser>` — restrict to one browser.
    One(Browser),
}

/// Pure mapping from the clap field value to a `GrantChoice`.
///
/// `None` = flag absent (no cookie access); `Some(GrantArg::All)` = all
/// stores; `Some(GrantArg::One(b))` = that specific browser.
fn grant_choice(flag: &Option<GrantArg>) -> GrantChoice {
    match flag {
        None => GrantChoice::NoGrant,
        Some(GrantArg::All) => GrantChoice::AllStores,
        Some(GrantArg::One(b)) => GrantChoice::One(*b),
    }
}

// ── capabilities flag ─────────────────────────────────────────────────────────

/// `--capabilities` output format. Bare flag → text; `=json` → JSON.
#[derive(Clone, Copy, Debug)]
enum CapsFormat {
    Text,
    Json,
}

/// Parse the `--capabilities` value. Empty (the bare-flag default) → text.
fn parse_caps_format(s: &str) -> Result<CapsFormat, String> {
    match s {
        "" | "text" => Ok(CapsFormat::Text),
        "json" => Ok(CapsFormat::Json),
        other => Err(format!(
            "unknown capabilities format {other:?} (expected text or json)"
        )),
    }
}

/// Value parser for `--capabilities`: delegates to [`parse_caps_format`] for
/// acceptance (unchanged — still strictly rejects anything but `""`/`text`/
/// `json`) but additionally advertises `text`/`json` as clap *possible
/// values* with help, so shells tab-complete them. The bare-flag empty string
/// stays hidden from completion (it is a `default_missing_value`, not
/// something a user types).
#[derive(Clone)]
struct CapsFormatParser;

impl clap::builder::TypedValueParser for CapsFormatParser {
    type Value = CapsFormat;

    fn parse_ref(
        &self,
        cmd: &clap::Command,
        _arg: Option<&clap::Arg>,
        value: &std::ffi::OsStr,
    ) -> Result<Self::Value, clap::Error> {
        let s = value.to_string_lossy();
        parse_caps_format(&s).map_err(|msg| {
            clap::Error::raw(clap::error::ErrorKind::InvalidValue, msg).with_cmd(cmd)
        })
    }

    fn possible_values(
        &self,
    ) -> Option<Box<dyn Iterator<Item = clap::builder::PossibleValue> + '_>> {
        Some(Box::new(
            [
                clap::builder::PossibleValue::new("text")
                    .help("human-readable table (the bare-flag default)"),
                clap::builder::PossibleValue::new("json").help("structured JSON output"),
            ]
            .into_iter(),
        ))
    }
}

/// Render the capability matrix as a text table or pretty JSON.
fn render_capabilities(rows: &[astrogram::format::CapabilityRow], fmt: CapsFormat) -> String {
    match fmt {
        CapsFormat::Json => {
            serde_json::to_string_pretty(rows).unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}"))
        }
        CapsFormat::Text => {
            let mut out = String::new();
            out.push_str(&format!(
                "{:<13} {:<5} {:<4} {:<5} {:<13} {}\n",
                "slug", "kind", "read", "write", "auth", "fields dropped on write"
            ));
            for r in rows {
                let dropped = if r.dropped_on_write.is_empty() {
                    "(full fidelity)".to_string()
                } else {
                    r.dropped_on_write.join(", ")
                };
                out.push_str(&format!(
                    "{:<13} {:<5} {:<4} {:<5} {:<13} {}\n",
                    r.slug,
                    r.kind,
                    if r.can_read { "yes" } else { "no" },
                    if r.can_write { "yes" } else { "no" },
                    r.auth,
                    dropped,
                ));
            }
            out
        }
    }
}

// ── CLI ───────────────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(
    name = "blackmoon",
    version,
    about = "Astrology data converter — reads any target, writes any target.",
    long_about = "\
Reads one or more source targets (files or web endpoints), merges and deduplicates,
then writes to an output target.  Target type is detected from the file
extension (.SFcht, .zdb, .xml, .aaf, .jhd) or specified with --from / --to.

Each write is preceded by a read of the output target (if it already exists)
so no duplicate records are ever added.

A folder path reads every chart file inside it (and its subfolders).

Examples:
  blackmoon input.zdb --output out.SFcht
  blackmoon \"path/to/your/charts-folder\" --output \"path/to/your/collection.SFcht\"
  blackmoon a.SFcht b.zdb export.xml --output merged.SFcht
  blackmoon --from luna --luna-token $BLACKMOON_LUNA_TOKEN --output charts.SFcht
  blackmoon --from astrotheoros --astrotheoros-user $USER --astrotheoros-pass $PASS --output charts.SFcht
  blackmoon charts.SFcht --normalize
  blackmoon *.SFcht --normalize

VERBOSITY
  --verbose    Print per-record detail (duplicate names, per-chart fetch/write
               status, field-loss transcript).
  --quiet      Suppress non-essential output (progress, per-record detail);
               errors and required prompts still print. Mutually exclusive
               with --verbose.
  Both also read from $BLACKMOON_VERBOSE / $BLACKMOON_QUIET; a flag combining
  both on one invocation is a usage error.

MACHINE OUTPUT
  A sink counts as machine output when it is a parsable format written to
  stdout (--to json with no --output, or any file-kind format piped out
  via -o -/--output -). In that mode stdout carries only the serialized
  document — no progress narration — and diagnostics go to stderr instead.
  Whether a missing fill value (house system / zodiac / locus the sink
  requires but the source didn't provide — see --fill-house / --fill-zodiac /
  --fill-locus) prompts or fails depends on stdin, not on the sink: if stdin
  is a TTY, blackmoon prompts on stderr (even when writing a machine sink to
  stdout — the prompt never touches stdout); if stdin is not a TTY (piped
  input, non-interactive/FaaS invocation), it fails instead with exit code
  10, naming the flag and its accepted values.

EXIT CODES
  0   success
  1   internal error (unclassified)
  2   usage error (bad flag/argument combination)
  3   input error (e.g. an unrecognised --fill-house/--fill-zodiac/--fill-locus
      value)
  4   not found (no input charts available to convert)
  6   chart-parse error (malformed source file/record)
  7   auth error (web endpoint login/session failure)
  8   network error (web endpoint request failure)
  9   lossy-refused (--strict refused a conversion that would drop data)
  10  need-input (a required fill value was missing and stdin is not a TTY,
      so there was no way to prompt for it — see MACHINE OUTPUT above)
  11  I/O error"
)]
struct Cli {
    /// Input files (.SFcht, .zdb, .xml, .aaf, .jhd), or directories containing them.
    /// Omit when --from a web endpoint.
    #[arg(env = "BLACKMOON_INPUTS", value_delimiter = ',')]
    inputs: Vec<PathBuf>,

    /// Output file.  Target detected from extension; overridden by --to.
    /// Use `now.{ext}` to substitute the current UTC timestamp automatically
    /// (e.g. `--output now.SFcht`).  When --from a web endpoint and --output is omitted,
    /// defaults to `{timestamp}.SFcht`.
    #[arg(short, long, alias = "out", env = "BLACKMOON_OUTPUT")]
    output: Option<PathBuf>,

    /// Source target — required when the source is not a file (web endpoint).
    #[arg(long, value_parser = format_parser())]
    from: Option<Target>,

    /// Output target — overrides the extension of --output (or use for a web endpoint).
    #[arg(long, value_parser = format_parser(), env = "BLACKMOON_TO")]
    to: Option<Target>,

    /// Alias for --from / --to.  Used when both sides share the same target
    /// (e.g. `--target luna --normalize`) or as a shorthand for either
    /// direction when the other side is inferred from a file extension.
    #[arg(long, value_parser = format_parser(), env = "BLACKMOON_TARGET")]
    target: Option<Target>,

    /// Map non-cp1252 characters to ASCII equivalents in all text fields.
    /// Without --output, edits each input file in-place.
    #[arg(long, env = "BLACKMOON_NORMALIZE")]
    normalize: bool,

    /// LUNA® auth token (session cookie).  Required when --from luna or --to luna.
    #[arg(long, env = "BLACKMOON_LUNA_TOKEN", hide_env_values = true)]
    luna_token: Option<String>,

    /// Delay between web endpoint HTTP requests in milliseconds.
    #[arg(long, default_value = "500")]
    delay: u64,

    /// Skip LUNA® charts until the first whose name starts with this prefix
    /// (case-insensitive).  Useful for resuming an interrupted fetch.
    #[arg(long)]
    luna_resume_from: Option<String>,

    /// astro.com auth token (the cid).  Required when --from astrocom or --to astrocom,
    /// unless --astrocom-user / --astrocom-pass are provided (login takes priority).
    #[arg(long, env = "BLACKMOON_ASTROCOM_TOKEN", hide_env_values = true)]
    astrocom_token: Option<String>,

    /// astro.com account email.  When combined with --astrocom-pass, blackmoon logs
    /// in automatically and derives a fresh cid (no manual cookie needed).
    #[arg(long, env = "BLACKMOON_ASTROCOM_USER", hide_env_values = true)]
    astrocom_user: Option<String>,

    /// astro.com account password.  Use with --astrocom-user.
    #[arg(long, env = "BLACKMOON_ASTROCOM_PASS", hide_env_values = true)]
    astrocom_pass: Option<String>,

    /// astrotheoros.com account email.  When combined with --astrotheoros-pass,
    /// blackmoon logs in automatically.
    #[arg(long, env = "BLACKMOON_ASTROTHEOROS_USER", hide_env_values = true)]
    astrotheoros_user: Option<String>,

    /// astrotheoros.com account password.  Use with --astrotheoros-user.
    #[arg(long, env = "BLACKMOON_ASTROTHEOROS_PASS", hide_env_values = true)]
    astrotheoros_pass: Option<String>,

    /// Auth token as "jwt:session_id:client_uat" (colon-delimited). Prefer user/pass.
    #[arg(long, env = "BLACKMOON_ASTROTHEOROS_TOKEN", hide_env_values = true)]
    astrotheoros_token: Option<String>,

    /// Delete every chart on the web target after an interactive confirmation
    /// prompt.  Requires a web target: --target luna / astrocom / astrotheoros.
    #[arg(long)]
    clear: bool,

    /// In-place consolidation mode: fetch every chart from the web target,
    /// cluster duplicate candidates by spacetime, prompt the user to choose
    /// which to keep, then delete the rest.  Decisions persist to
    /// --decision-log so an interrupted run can be resumed.
    /// Requires a web target: --target luna / astrocom / astrotheoros.
    #[arg(long)]
    consolidate: bool,

    /// JSONL file recording each user decision (one record per keystroke,
    /// fsync'd before the next prompt).  Defaults to
    /// `$XDG_CACHE_HOME/blackmoon/luna-decisions.jsonl` (or
    /// `~/.cache/blackmoon/luna-decisions.jsonl`).
    #[arg(long)]
    decision_log: Option<PathBuf>,

    /// Skip the post-write read-back transcript (web targets only).
    #[arg(long)]
    no_verify: bool,

    /// Refuse a conversion that would drop data the sink cannot store
    /// (exit non-zero) instead of warning and proceeding.
    #[arg(long, env = "BLACKMOON_STRICT")]
    strict: bool,

    /// Value to fill house_system with when writing to a format that requires it
    /// but the source did not provide one (e.g. placidus, koch, whole-sign).
    #[arg(long, env = "BLACKMOON_FILL_HOUSE", value_parser = fill_house_parser())]
    fill_house: Option<String>,
    /// Value to fill zodiac with in the same situation (e.g. tropical, lahiri).
    #[arg(long, env = "BLACKMOON_FILL_ZODIAC", value_parser = fill_zodiac_parser())]
    fill_zodiac: Option<String>,
    /// Value to fill the locus (coordinate system) with: geocentric | heliocentric.
    #[arg(long, env = "BLACKMOON_FILL_LOCUS", value_parser = fill_locus_parser())]
    fill_locus: Option<String>,

    /// Print per-record detail (duplicate names, per-chart fetch status).
    #[arg(long, short, env = "BLACKMOON_VERBOSE", conflicts_with = "quiet")]
    verbose: bool,

    /// Suppress non-essential output (progress, per-record detail); errors
    /// and required prompts still print. Mutually exclusive with --verbose.
    #[arg(long, short, env = "BLACKMOON_QUIET")]
    quiet: bool,

    /// Import a session from a browser cookie store — the verb *grant* is the
    /// explicit consent (INV-4).  Omit the value (bare flag) or pass `all` to
    /// search all installed browsers; pass a browser name to restrict to one
    /// of the browsers `wristband` supports (see `Browser::all()`; shell
    /// completion enumerates the current set).
    /// When present, blackmoon reads the provider's session cookie(s) from the
    /// browser instead of requiring --{provider}-token or --{provider}-user/pass.
    #[arg(long, value_name = "BROWSER", num_args = 0..=1, default_missing_value = "all",
          value_parser = BrowserValueParser, overrides_with = "grant_cookie_access")]
    grant_cookie_access: Option<GrantArg>,

    /// Browser profile name or path to use with --grant-cookie-access.
    /// Defaults to the newest-modified store for the chosen browser.
    #[arg(long, value_name = "NAME", requires = "grant_cookie_access")]
    cookies_profile: Option<String>,

    /// User-Agent control (requires --grant-cookie-access). `--ua browser`
    /// mimics the cookie-source browser's own UA; bare `--ua` uses a fixed
    /// static spoof; `--ua <string>` sends that string verbatim. Without --ua, a
    /// granted run sends blackmoon's honest self-reported UA — no browser
    /// impersonation by default.
    #[arg(long, value_name = "STRING", num_args = 0..=1, default_missing_value = "",
          value_parser = parse_ua, requires = "grant_cookie_access", overrides_with = "ua")]
    ua: Option<UaArg>,

    /// Print a shell completion script to stdout.
    #[arg(long = "generate-completion", value_name = "SHELL", num_args = 0..=1,
          default_missing_value = "auto", hide = true, value_parser = ShellArgParser)]
    generate_completion: Option<String>,

    /// Print the format-support matrix (which formats blackmoon reads/writes,
    /// with what auth, and which chart fields survive a write) and exit.
    /// Bare --capabilities prints a table; --capabilities=json prints JSON.
    #[arg(long, value_name = "FORMAT", num_args = 0..=1, default_missing_value = "",
          value_parser = CapsFormatParser)]
    capabilities: Option<CapsFormat>,
}

/// Resolved output verbosity, derived from `--quiet`/`--verbose` (clap enforces
/// they can't both be set, but `resolve` still has a defined answer if it ever
/// receives both — verbose wins).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Verbosity {
    Quiet,
    Normal,
    Verbose,
}

impl Verbosity {
    fn resolve(quiet: bool, verbose: bool) -> Self {
        if verbose {
            Verbosity::Verbose
        } else if quiet {
            Verbosity::Quiet
        } else {
            Verbosity::Normal
        }
    }

    fn is_quiet(self) -> bool {
        self == Verbosity::Quiet
    }

    fn is_verbose(self) -> bool {
        self == Verbosity::Verbose
    }
}

// ── shell-completion arg parser ────────────────────────────────────────────────

/// Value parser for `--generate-completion[=<shell>]`: passes the raw string
/// through unchanged (parsing/validation stays in `run()`, which special-cases
/// `"auto"` before falling back to `clap_complete::Shell`'s own `FromStr`) but
/// additionally advertises `auto` plus every [`clap_complete::Shell`] variant
/// as clap *possible values*, so shells tab-complete them. The candidate set
/// is read from `Shell::value_variants()` — `clap_complete`'s own registry —
/// at command-build time, so a shell `clap_complete` adds later completes
/// with no further wiring here.
#[derive(Clone)]
struct ShellArgParser;

impl clap::builder::TypedValueParser for ShellArgParser {
    type Value = String;

    fn parse_ref(
        &self,
        _cmd: &clap::Command,
        _arg: Option<&clap::Arg>,
        value: &std::ffi::OsStr,
    ) -> Result<Self::Value, clap::Error> {
        // Accept any string; `run()` validates it (via `"auto"` or
        // `Shell::from_str`) and reports an unknown shell as its own error.
        Ok(value.to_string_lossy().into_owned())
    }

    fn possible_values(
        &self,
    ) -> Option<Box<dyn Iterator<Item = clap::builder::PossibleValue> + '_>> {
        use clap::ValueEnum;
        // `clap_complete::Shell::to_possible_value()` doesn't attach help, so
        // it's added here (mirroring the enum's own doc comments). `Shell` is
        // `#[non_exhaustive]`, so this match needs a wildcard arm; it panics
        // rather than silently completing a shell with no help mapped.
        fn shell_help(s: &clap_complete::Shell) -> &'static str {
            match s {
                clap_complete::Shell::Bash => "Bourne Again SHell",
                clap_complete::Shell::Elvish => "Elvish shell",
                clap_complete::Shell::Fish => "Friendly Interactive SHell",
                clap_complete::Shell::PowerShell => "PowerShell",
                clap_complete::Shell::Zsh => "Z SHell",
                other => unreachable!(
                    "clap_complete::Shell grew a variant ({other:?}) with no help mapped in \
                     shell_help() — add one"
                ),
            }
        }
        let auto = std::iter::once(
            clap::builder::PossibleValue::new("auto")
                .help("detect the shell from $SHELL (the bare-flag default)"),
        );
        let shells = clap_complete::Shell::value_variants()
            .iter()
            .map(|s| s.to_possible_value().unwrap().help(shell_help(s)));
        Some(Box::new(auto.chain(shells)))
    }
}

// ── main ──────────────────────────────────────────────────────────────────────

/// Process entry point. `Cli::parse()` (inside `run`) already exits the
/// process with clap's own code 2 on a parse error — this wrapper only
/// handles errors surfaced after parsing succeeds, via `exit::classify`.
fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e:#}");
            exit::classify(&e).exit_code()
        }
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    if let Some(shell_str) = &cli.generate_completion {
        use clap::CommandFactory;
        let shell = if shell_str == "auto" {
            detect_shell()
                .context("could not detect shell from $SHELL; pass it explicitly (e.g. --generate-completion zsh)")?
        } else {
            shell_str.parse::<clap_complete::Shell>().map_err(|_| {
                anyhow::anyhow!(
                    "unknown shell '{shell_str}'; valid values: bash, elvish, fish, powershell, zsh"
                )
            })?
        };
        clap_complete::generate(
            shell,
            &mut Cli::command(),
            "blackmoon",
            &mut std::io::stdout(),
        );
        return Ok(());
    }

    if let Some(fmt) = cli.capabilities {
        print!(
            "{}",
            render_capabilities(&astrogram::format::capability_matrix(), fmt)
        );
        return Ok(());
    }

    let nothing = cli.inputs.is_empty()
        && cli.output.is_none()
        && cli.from.is_none()
        && cli.to.is_none()
        && cli.target.is_none()
        && !cli.normalize
        && !cli.consolidate
        && !cli.clear;

    if nothing {
        use clap::CommandFactory;
        Cli::command().print_long_help()?;
        return Ok(());
    }

    // In-place normalize on file inputs — only when no explicit source/sink
    // override is present.  --from luna --normalize routes through cmd_convert
    // so luna is treated as both source and sink.
    if cli.normalize
        && cli.output.is_none()
        && cli.to.is_none()
        && cli.from.is_none()
        && cli.target.is_none()
    {
        return cmd_normalize_inplace(&cli.inputs);
    }

    // Everything else is a convert/merge.
    cmd_convert(&cli)
}

/// True when the sink is a parsable format destined for stdout (`--to
/// json`/jzod, or any other file-kind format via `-o -`/`--output -`) — the
/// machine-output contract: stdout carries only the serialized document
/// (written once). This does not affect whether a required conversion
/// decision prompts or fails — that gate is stdin's terminal-ness, not the
/// sink (see `resolve_fill`).
///
/// A file-kind format only ever reaches stdout via `-o -`/`--output -`; web
/// targets never write to stdout (`out_path` is `None` for them) — so this
/// check covers `--to json` (JZOD) to stdout as well as every other file
/// format piped out with `-o -`. Every writable file-kind format is
/// structured data (JZOD, SFcht, raw key:value, …), not prose, so `target`
/// does not need to narrow the check further; it is taken anyway so the
/// call reads as "this sink, this destination" and stays open to a
/// per-format carve-out later without changing the signature.
fn is_machine_output(_target: Format, out_path: Option<&Path>) -> bool {
    out_path.is_some_and(|p| p == Path::new("-"))
}

/// Render the interactive prompt line for a fill spec, e.g.
/// `Value for house system (placidus, koch, campanus, …) [placidus]: `.
/// Lists every accepted slug so the user need not consult docs to answer,
/// and shows the suggested default in brackets. Shared by the TTY prompt in
/// `resolve_fill` and its own unit test; the non-interactive-stdin failure
/// path uses [`exit::NeedInputError`]'s `Display` (see `resolve_fill`), which
/// lists the same `spec.accepted` values alongside the flag name.
fn fill_prompt_line(spec: &astrogram::pipeline::FillSpec) -> String {
    format!(
        "Value for {} ({}) [{}]: ",
        spec.label,
        spec.accepted.join(", "),
        spec.default_slug
    )
}

/// Flag → TTY prompt (with suggested value) → error.
///
/// A supplied `--fill-*` value that is not a recognised slug returns a typed
/// [`exit::InputError`] (exit 3) — validation happens here, where the value is
/// consumed, rather than at clap-parse time, so a bad value reads as bad input
/// rather than a structural usage error, and is only checked when the fill is
/// actually needed.
///
/// With no flag, the prompt gate is stdin's terminal-ness alone: if stdin is a
/// tty, prompt on stderr (safe even when the sink is stdout — the prompt never
/// touches stdout); if stdin is NOT a tty (piped input, machine-output
/// pipeline, FaaS), return a typed [`exit::NeedInputError`] (exit 10) rather
/// than blocking on a read that can never receive an answer.
fn resolve_fill(
    spec: &astrogram::pipeline::FillSpec,
    flag: Option<&str>,
    sink: Format,
) -> Result<astrogram::pipeline::FillValue> {
    use std::io::IsTerminal;
    let label = spec.label;
    // A flag/prompt value that fails to parse is an out-of-range *input*, not
    // a usage error — classify it accordingly (exit 3) and list the accepted
    // values so the user can correct it.
    let parse = |s: &str| {
        (spec.parse)(s).ok_or_else(|| {
            anyhow::Error::from(exit::InputError {
                label: label.to_string(),
                value: s.to_string(),
                flag: format!("--fill-{}", spec.flag_suffix),
                accepted: spec.accepted.iter().map(|s| (*s).to_string()).collect(),
            })
        })
    };
    if let Some(s) = flag {
        return parse(s);
    }
    if std::io::stdin().is_terminal() {
        eprintln!(
            "{} stores {label} per chart; your source did not provide one.",
            sink.spec().slug
        );
        eprint!("{}", fill_prompt_line(spec));
        std::io::stderr().flush().ok();
        let mut line = String::new();
        std::io::stdin().read_line(&mut line)?;
        let chosen = line.trim();
        let chosen = if chosen.is_empty() {
            spec.default_slug
        } else {
            chosen
        };
        return parse(chosen);
    }
    // stdin is not a tty (piped input, or a machine-output pipeline with no
    // terminal to prompt on) — there is no way to obtain an answer
    // interactively, so surface the typed need-input error (exit 10).
    Err(exit::NeedInputError {
        label: label.to_string(),
        flag: format!("--fill-{}", spec.flag_suffix),
        accepted: spec.accepted.iter().map(|s| (*s).to_string()).collect(),
        sink_slug: sink.spec().slug.to_string(),
    }
    .into())
}

/// Resolve and apply fill values for the fields the sink demands that each
/// chart's source lacked. Values are resolved once per field (flag → TTY
/// prompt → error) and applied only to charts whose source did NOT carry the
/// field, to avoid overwriting genuine values from SFcht sources.
///
/// The fill policy table (label, flag suffix, default, parser) lives in
/// [`astrogram::pipeline::FILL_SPECS`]; this function keeps only terminal I/O.
fn apply_fills(
    merged: &mut [astrogram::chart::Chart],
    fills: &[astrogram::capability::ChartField],
    source_of: &std::collections::HashMap<providers::DatetimeKey, Format>,
    cli: &Cli,
    sink: Format,
) -> Result<()> {
    use astrogram::pipeline::{apply_fill_value, fill_spec, fill_targets};

    // CLI flag lookup by suffix — the flag surface is CLI-owned.
    let flag_for = |suffix: &str| -> Option<&str> {
        match suffix {
            "house" => cli.fill_house.as_deref(),
            "zodiac" => cli.fill_zodiac.as_deref(),
            "locus" => cli.fill_locus.as_deref(),
            // Must track `astrogram::pipeline::FILL_SPECS` suffixes: a new FillSpec
            // whose suffix is not listed here silently falls back to the TTY prompt
            // instead of reading its CLI flag.
            _ => None,
        }
    };

    for &field in fills {
        let Some(spec) = fill_spec(field) else {
            // Unreachable as long as the pin test in pipeline.rs passes:
            // every NON_OMITTABLE field must have a FillSpec in FILL_SPECS.
            bail!(
                "no fill spec for {:?} — add it to FILL_SPECS in astrogram/src/pipeline.rs",
                field
            );
        };
        let flag_val = flag_for(spec.flag_suffix);
        let value = resolve_fill(spec, flag_val, sink)?;
        let targets = fill_targets(merged, field, source_of, sink);
        apply_fill_value(merged, value, &targets);
    }
    Ok(())
}

// ── convert / merge ───────────────────────────────────────────────────────────

fn is_web_target(t: Target) -> bool {
    matches!(t.spec().kind, Kind::Web)
}

/// Credential-source classification lives in the library so every consumer
/// shares it; the CLI only formats it (see [`source_label`]).
use astrogram::auth::{CookieDisclosure, SourceKind};

/// Human label for the chain position `idx` given the kinds present.
fn source_label(kinds: &[SourceKind], idx: usize) -> &'static str {
    match kinds.get(idx) {
        Some(SourceKind::Cookie) => "browser cookie",
        Some(SourceKind::Token) => "token",
        Some(SourceKind::Login) => "login",
        None => "unknown source",
    }
}

/// Print the "found ... logged in on ..." cookie-import disclosure — env
/// narration about which browser/profile the session cookie came from (INV-4),
/// dropped entirely under `--quiet`. `verbose` (independent of `quiet`)
/// additionally appends per-store cookie expiry. Reads the structured
/// [`CookieDisclosure`] the library returns; all formatting is CLI-side.
fn print_disclosure(d: &CookieDisclosure, quiet: bool, verbose: bool) {
    if quiet {
        return;
    }
    let labels: Vec<String> = d
        .found_in
        .iter()
        .map(|(b, p, _)| store_label(*b, p))
        .collect();
    let label_refs: Vec<&str> = labels.iter().map(String::as_str).collect();
    let winner = store_label(d.winner_browser, &d.winner_profile);
    if d.found_in.len() > 1 {
        eprintln!(
            "found {} logged in on {}. Using {} as it is the most recent.",
            d.domain,
            oxford_join(&label_refs),
            winner,
        );
    } else {
        eprintln!(
            "found {} logged in on {}.",
            d.domain,
            oxford_join(&label_refs)
        );
    }
    if verbose {
        let now = now_secs() as i64;
        for (b, p, freshness) in &d.found_in {
            let label = store_label(*b, p);
            if *freshness > 1_000_000_000 {
                let delta = freshness - now;
                let when = if delta >= 0 {
                    format!("session expires in {delta}s")
                } else {
                    format!("session expired {}s ago (stale on-disk snapshot)", -delta)
                };
                eprintln!("  {label}: {when}");
            } else {
                eprintln!("  {label}: session present (no expiry signal)");
            }
        }
    }
}

/// Announce which credential source authenticated, naming a fall-through when
/// the chain advanced past earlier sources. Env narration — dropped under
/// `--quiet`.
fn announce_source(kinds: &[SourceKind], used: usize, quiet: bool) {
    if quiet {
        return;
    }
    let label = source_label(kinds, used);
    if used > 0 {
        eprintln!("authenticated via {label} (earlier source(s) were stale).");
    } else {
        eprintln!("authenticated via {label}.");
    }
}

/// Map the clap `--ua` flag to the frontend-neutral [`astrogram::user_agent::UaIntent`]. The *policy*
/// (which UA to send, and the opt-in impersonation default) lives in
/// [`astrogram::user_agent::choose`] so a GUI shares it verbatim — this is only
/// the CLI's flag→intent translation.
fn ua_intent(ua: &Option<UaArg>) -> astrogram::user_agent::UaIntent {
    use astrogram::user_agent::UaIntent;
    match ua {
        None => UaIntent::Default,
        Some(UaArg::Browser) => UaIntent::MimicBrowser,
        Some(UaArg::Static) => UaIntent::Static,
        Some(UaArg::Custom(s)) => UaIntent::Custom(s.clone()),
    }
}

/// Resolve the [`astrogram::user_agent::UaChoice`] for this run. `grant` is whether
/// `--grant-cookie-access` is active; `cookie_ua` is the divined browser UA when
/// a cookie was actually used. Thin wrapper that maps the flag and defers the
/// policy to the shared [`astrogram::user_agent::choose`].
fn select_ua_choice(
    grant: bool,
    ua: &Option<UaArg>,
    cookie_ua: Option<String>,
) -> astrogram::user_agent::UaChoice {
    astrogram::user_agent::choose(grant, ua_intent(ua), cookie_ua)
}

/// blackmoon's product identity for the self-reported UA.
fn blackmoon_app_product() -> astrogram::user_agent::AppProduct {
    astrogram::user_agent::AppProduct::new("Blackmoon", env!("CARGO_PKG_VERSION"))
}

/// blackmoon's JZOD generator identity: its own version, plus the astrogram
/// and jzod components it's built against. blackmoon converts formats — it
/// never computes ephemeris positions — so this `generator` is the only
/// producer provenance a JZOD export carries (see `astrogram::jzod`).
fn blackmoon_generator() -> jzod::Generator {
    jzod::Generator {
        name: "blackmoon".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        components: vec![
            jzod::Component {
                name: "astrogram".into(),
                version: astrogram::ASTROGRAM_VERSION.into(),
            },
            jzod::Component {
                name: "jzod".into(),
                version: jzod::JZOD_VERSION.into(),
            },
        ],
    }
}

/// Human name for a web target in the "no usable X cookie" note.
fn cookie_site(target: Target) -> &'static str {
    match target {
        Target::Astrotheoros => "astrotheoros.com",
        Target::Astrocom => "astro.com",
        Target::Luna => "LUNA",
        other => unreachable!("cookie_site called for non-web target {other:?}"),
    }
}

/// Pull the raw `(token, user, pass)` flag values for a web target. LUNA has no
/// login flow, so it exposes only a token.
fn provider_flags(target: Target, cli: &Cli) -> (Option<String>, Option<String>, Option<String>) {
    match target {
        Target::Astrotheoros => (
            cli.astrotheoros_token.clone(),
            cli.astrotheoros_user.clone(),
            cli.astrotheoros_pass.clone(),
        ),
        Target::Astrocom => (
            cli.astrocom_token.clone(),
            cli.astrocom_user.clone(),
            cli.astrocom_pass.clone(),
        ),
        Target::Luna => (cli.luna_token.clone(), None, None),
        other => unreachable!("provider_flags called for non-web target {other:?}"),
    }
}

/// Translate a library assembly error into blackmoon's flag-worded message.
/// The *rule* (empty chain, half-credential, bad token) is decided in
/// `astrogram::auth`; only the flag names are CLI presentation.
fn assemble_error(target: Target, e: astrogram::auth::AuthError) -> anyhow::Error {
    use astrogram::auth::AuthError;
    match e {
        AuthError::NoCredentials => match target {
            Target::Astrotheoros => anyhow::anyhow!(
                "no astrotheoros.com credentials: pass --grant-cookie-access, \
                 --astrotheoros-token, or --astrotheoros-user/--pass"
            ),
            Target::Astrocom => anyhow::anyhow!(
                "no astro.com credentials: pass --grant-cookie-access, \
                 --astrocom-token, or --astrocom-user/--pass"
            ),
            Target::Luna => {
                anyhow::anyhow!("no LUNA credentials: pass --grant-cookie-access or --luna-token")
            }
            other => anyhow::anyhow!("no credentials for {other:?}"),
        },
        AuthError::MissingPass => match target {
            Target::Astrotheoros => anyhow::anyhow!(
                "--astrotheoros-pass (or BLACKMOON_ASTROTHEOROS_PASS) required with --astrotheoros-user"
            ),
            Target::Astrocom => anyhow::anyhow!(
                "--astrocom-pass (or BLACKMOON_ASTROCOM_PASS) required with --astrocom-user"
            ),
            other => anyhow::anyhow!("a login password requires an email ({other:?})"),
        },
        AuthError::MissingUser => match target {
            Target::Astrotheoros => anyhow::anyhow!(
                "--astrotheoros-user (or BLACKMOON_ASTROTHEOROS_USER) required with --astrotheoros-pass"
            ),
            Target::Astrocom => anyhow::anyhow!(
                "--astrocom-user (or BLACKMOON_ASTROCOM_USER) required with --astrocom-pass"
            ),
            other => anyhow::anyhow!("a login email requires a password ({other:?})"),
        },
        AuthError::BadAstrotheorosToken(_) => {
            anyhow::anyhow!("--astrotheoros-token must be 'jwt:session_id:client_uat'")
        }
        other => anyhow::anyhow!("{other}"),
    }
}

fn resolve_provider(target: Target, cli: &Cli) -> Result<WebProvider> {
    use astrogram::auth::{AuthPlan, CredentialInputs};
    use astrogram::cookie_import;

    let verbosity = Verbosity::resolve(cli.quiet, cli.verbose);
    let quiet = verbosity.is_quiet();
    let choice = grant_choice(&cli.grant_cookie_access);
    let want_cookie = choice != GrantChoice::NoGrant;
    let browser: Option<Browser> = match &choice {
        GrantChoice::AllStores | GrantChoice::NoGrant => None,
        GrantChoice::One(b) => Some(*b),
    };

    // Import the browser session cookie up front (consent-gated). Its divined
    // User-Agent feeds the UA choice; an import failure is a soft note, never
    // an error — the token/login sources still get a chance.
    let cookie = if want_cookie {
        match cookie_import::import_credential(target, browser, cli.cookies_profile.as_deref()) {
            Ok(out) => Some(out),
            Err(e) => {
                if !quiet {
                    eprintln!(
                        "note: no usable {} cookie ({e}); trying other sources",
                        cookie_site(target)
                    );
                }
                None
            }
        }
    } else {
        None
    };

    // Map clap flags → library inputs. Precedence (cookie → token → login),
    // half-credential validation, and the non-empty-chain rule all live in
    // astrogram::auth::AuthPlan::assemble.
    let (token, user, pass) = provider_flags(target, cli);
    let inputs = CredentialInputs {
        cookie,
        token,
        user,
        pass,
        luna_resume_from: cli.luna_resume_from.clone(),
        luna_normalize: cli.normalize,
    };
    let plan = AuthPlan::assemble(target, inputs).map_err(|e| assemble_error(target, e))?;

    // astrotheoros: warn when the browser cookie is the sole credential (a
    // stale cookie then has no fallback).
    if !quiet && target == Target::Astrotheoros && plan.only_cookie() {
        eprintln!(
            "note: browser cookie is the only astrotheoros.com credential — \
             if it is stale there is no fallback; set BLACKMOON_ASTROTHEOROS_USER/BLACKMOON_ASTROTHEOROS_PASS \
             (or --astrotheoros-token) to enable login fallback"
        );
    }
    if let Some(d) = plan.disclosure() {
        print_disclosure(d, quiet, verbosity.is_verbose());
    }

    let ua_choice = select_ua_choice(want_cookie, &cli.ua, plan.cookie_ua().map(str::to_string));
    let ua_label = astrogram::user_agent::ua_kind_label(&ua_choice);
    let app = blackmoon_app_product();
    let ua = astrogram::user_agent::resolve(ua_choice, &app);
    if !quiet {
        eprintln!("user-agent ({ua_label}): {ua}");
    }

    let (provider, report) = plan.connect(cli.delay, &ua)?;
    announce_source(&report.kinds, report.used, quiet);
    Ok(provider)
}

/// Display label for a (browser, profile) store: just the browser's proper
/// name for a default profile, else `"Browser (profile)"`.
fn store_label(browser: Browser, profile: &str) -> String {
    if profile.is_empty() || profile.eq_ignore_ascii_case("default") {
        browser.display_name().to_string()
    } else {
        format!("{} ({profile})", browser.display_name())
    }
}

/// Join names with commas and a serial "and":
/// `[a]` → `a`, `[a, b]` → `a and b`, `[a, b, c]` → `a, b, and c`.
fn oxford_join(items: &[&str]) -> String {
    match items {
        [] => String::new(),
        [a] => (*a).to_string(),
        [a, b] => format!("{a} and {b}"),
        [rest @ .., last] => format!("{}, and {last}", rest.join(", ")),
    }
}

fn cmd_convert(cli: &Cli) -> Result<()> {
    let verbosity = Verbosity::resolve(cli.quiet, cli.verbose);
    let sink = providers::QuietAwareSink::new(crate::providers::CliSink, verbosity.is_quiet());
    let from = cli.from.or(cli.target);
    let to = cli.to.or(cli.target);

    // --clear: delete every chart on the web target after confirmation.
    if cli.clear {
        let target = cli
            .target
            .or(cli.from)
            .or(cli.to)
            .filter(|t| is_web_target(*t))
            .context("--clear requires a web target (--target luna / astrocom / astrotheoros)")?;
        let provider = resolve_provider(target, cli)?;
        return cmd_clear(provider);
    }

    // --consolidate: fetch every chart from a web target, cluster duplicates,
    // prompt the user, delete the rest.  Works for any web target.
    if cli.consolidate {
        let target = cli
            .target
            .or(cli.from)
            .or(cli.to)
            .filter(|t| is_web_target(*t))
            .context(
                "--consolidate requires a web target (--target luna / astrocom / astrotheoros)",
            )?;
        let provider = resolve_provider(target, cli)?;
        return cmd_consolidate(provider, cli);
    }

    // Resolve --output: expand `now.ext` and supply defaults for web targets.
    let resolved_output: Option<PathBuf> = match &cli.output {
        Some(p) => Some(expand_now(p, now_secs())),
        None if from.map(is_web_target).unwrap_or(false) && !cli.normalize => {
            Some(PathBuf::from(format!("{}.SFcht", utc_timestamp())))
        }
        None => None,
    };

    // --from luna/astro --normalize with no --output → same target for source and sink.
    let effective_to = if from.map(is_web_target).unwrap_or(false)
        && cli.normalize
        && cli.output.is_none()
        && to.is_none()
    {
        from
    } else {
        to
    };

    // Determine output target.
    let out_target = match (effective_to, resolved_output.as_deref()) {
        (Some(t), _) => t,
        (None, Some(p)) => Format::from_path(p).with_context(|| {
            format!(
                "cannot detect target from '{}'; use --to to specify",
                display_path(p)
            )
        })?,
        (None, None) => {
            return Err(exit::UsageError {
                message: "--output (or --to luna / --to astrocom) is required".to_string(),
            }
            .into());
        }
    };

    let out_path = if is_web_target(out_target) {
        None
    } else {
        Some(
            resolved_output
                .as_ref()
                .context("--output is required when writing to a file target")?,
        )
    };

    // When writing to stdout (`-o -`), route progress messages to stderr so
    // the output stream contains only the serialized chart data — the
    // machine-output contract (see `is_machine_output`).
    let to_stdout = is_machine_output(out_target, out_path.map(PathBuf::as_path));

    // Build providers for involved web targets only.
    // Only involved targets are constructed — no stray logins from env vars.
    let mut out_provider: Option<WebProvider> = if is_web_target(out_target) {
        Some(resolve_provider(out_target, cli)?)
    } else {
        None
    };
    let mut in_provider: Option<WebProvider> =
        if from.map(is_web_target).unwrap_or(false) && from != Some(out_target) {
            Some(resolve_provider(from.unwrap(), cli)?)
        } else {
            None
        };

    // 1. Read existing output target (read-before-write dedup).
    let mut existing: Vec<astrogram::chart::Chart> = Vec::new();
    if let Some(p) = &mut out_provider {
        if from != Some(out_target) {
            existing = p.read_existing(&sink)?;
        }
    } else if let Some(p) = out_path
        && p.exists()
    {
        existing = read_file_target(p, out_target)
            .with_context(|| format!("reading existing output {}", display_path(p)))?;
        if !to_stdout && !verbosity.is_quiet() {
            eprintln!("{}: {} charts (existing)", display_path(p), existing.len());
        }
    }

    // 2. Read input sources.
    // Build source_of alongside batches so each chart keeps its source Format.
    // Key: providers::DatetimeKey; Value: the Format it was read from.
    // Use .entry().or_insert() so the first occurrence wins (matches merge's keep-first dedup).
    // Keys here are EXACT (providers::key) while merge dedup is fuzzy (±2h, ±0.1°); that's
    // fine — the merge survivor is always one of the tagged input charts, so its exact key
    // is always present. The unwrap_or(sink) fallback in report_drops/apply_fills is therefore
    // unreachable for survivors; do not "align" the keying with the fuzzy merge.
    let mut source_of: HashMap<providers::DatetimeKey, Format> = HashMap::new();

    // Tag existing charts with out_target (they came from the output file/web target).
    astrogram::pipeline::record_sources(&mut source_of, &existing, out_target);
    let mut batches: Vec<Vec<astrogram::chart::Chart>> = vec![existing];

    if let Some(p) = &mut in_provider {
        let charts = p.read_input(&sink)?;
        astrogram::pipeline::record_sources(
            &mut source_of,
            &charts,
            from.expect("in_provider is Some only when --from/--target is set"),
        );
        batches.push(charts);
    } else if from.map(is_web_target).unwrap_or(false) && from == Some(out_target) {
        // normalize-in-place for web: source == sink, use out_provider for reading.
        // (is_web_target guard ensures out_provider is Some — file-to-file same-target
        // is a degenerate case that falls through to the file-inputs else branch.)
        let charts = out_provider.as_mut().unwrap().read_input(&sink)?;
        astrogram::pipeline::record_sources(&mut source_of, &charts, out_target);
        batches.push(charts);
    } else {
        if cli.inputs.is_empty() {
            return Err(exit::NoInputError {
                message: "at least one input file is required (or use --from / --target luna / --target astro)".to_string(),
            }
            .into());
        }
        // Expand any directory input into the chart files under it (recursive,
        // in-process — no shell glob). `--from` narrows a directory to one format.
        let mut expanded: Vec<std::path::PathBuf> = Vec::new();
        let mut from_dir: std::collections::HashSet<std::path::PathBuf> =
            std::collections::HashSet::new();
        let output_to_exclude: Option<&std::path::Path> = if is_web_target(out_target) || to_stdout
        {
            None
        } else {
            resolved_output.as_deref()
        };
        for path in &cli.inputs {
            if path.is_dir() {
                let astrogram::convert::DirScan {
                    files: scanned_files,
                    skipped,
                } = astrogram::convert::chart_files_under(path, cli.from)
                    .with_context(|| format!("scanning directory {}", display_path(path)))?;
                let scanned_count = scanned_files.len();
                let files =
                    astrogram::convert::without_output_file(scanned_files, output_to_exclude);
                if files.is_empty() {
                    return Err(exit::NoInputError {
                        message: format!("no chart files found under {}", display_path(path)),
                    }
                    .into());
                }
                if !to_stdout {
                    let files_word = if files.len() == 1 { "file" } else { "files" };
                    let skipped_word = if skipped == 1 { "file" } else { "files" };
                    let excluded_note = if files.len() < scanned_count {
                        " (excluding the output file)"
                    } else {
                        ""
                    };
                    eprintln!(
                        "read {} chart {files_word} under {}{excluded_note} (skipped {} non-chart {skipped_word})",
                        files.len(),
                        display_path(path),
                        skipped
                    );
                }
                for f in files {
                    from_dir.insert(f.clone());
                    expanded.push(f);
                }
            } else {
                expanded.push(path.clone());
            }
        }
        for path in &expanded {
            let target = Format::from_path(path).with_context(|| {
                format!(
                    "cannot detect target from '{}'; rename the file or use --from to specify",
                    display_path(path)
                )
            })?;
            let charts = read_file_target(path, target)
                .with_context(|| format!("reading {}", display_path(path)))?;
            if !to_stdout && !from_dir.contains(path) && !verbosity.is_quiet() {
                eprintln!("{}: {} charts", display_path(path), charts.len());
            }
            astrogram::pipeline::record_sources(&mut source_of, &charts, target);
            batches.push(charts);
        }
    }

    // 3. Merge + dedup.
    let existing_count: usize = batches[0].len();
    let new_input_count: usize = batches[1..].iter().map(Vec::len).sum();

    let (mut merged, skipped) = consolidate::merge_reporting(&batches);
    let dupes = skipped.len();

    // 4. Optional normalization.
    if cli.normalize {
        for chart in &mut merged {
            normalize_chart(chart);
        }
    }

    // 5. Report any fields the sink cannot store; abort if --strict.
    let dropped = report_drops(&merged, &source_of, out_target, to_stdout);
    if dropped > 0 && cli.strict {
        return Err(exit::LossyRefusedError {
            message: format!(
                "--strict: {dropped} chart(s) would lose data writing to {}; aborting",
                out_target.spec().slug
            ),
        }
        .into());
    }

    // 5b. Apply fills: resolve per-chart values the sink demands but each
    // chart's source never carried (e.g. house_system/zodiac/locus for ADB→SFcht).
    // The library unions fill_fields over all distinct source formats so mixed
    // batches work.
    {
        let needed = astrogram::pipeline::fill_fields_needed(&source_of, out_target);
        if !needed.is_empty() {
            apply_fills(&mut merged, &needed, &source_of, cli, out_target)?;
        }
    }

    // 6. Write.
    if let Some(p) = &mut out_provider {
        if cli.normalize {
            eprintln!("Charts to write ({}):", merged.len());
            for chart in &merged {
                eprintln!("  {}", chart.name);
            }
            eprint!(
                "About to write {} chart{} to your {} account. Proceed? [y/N] ",
                merged.len(),
                if merged.len() == 1 { "" } else { "s" },
                p.site_display(),
            );
            let mut answer = String::new();
            std::io::stdin()
                .read_line(&mut answer)
                .context("reading confirmation")?;
            if !matches!(answer.trim().to_lowercase().as_str(), "y" | "yes") {
                eprintln!("Aborted.");
                return Ok(());
            }
        }
        let inline = p.verifies_inline();
        let verify = !cli.no_verify;
        // Inline-verify providers (astrotheoros) fold account-wide globals
        // (house/zodiac) into the create-response entry before diffing.
        let global = if inline {
            p.fetch_global_settings()?
        } else {
            None
        };
        let mut verified = 0usize;
        let write_results = {
            // Live per-chart block, printed the instant each chart lands.
            let mut on_landed = |n: usize,
                                 total: usize,
                                 source: &astrogram::chart::Chart,
                                 landed: Option<&astrogram::chart::Chart>,
                                 status: &str| {
                let w = total.to_string().len();
                eprintln!("[{n:0>w$}/{total}] {}  {status}", source.name);
                if let Some(landed) = landed {
                    if verify {
                        let mut folded = landed.clone();
                        let notes: &[(astrogram::capability::ChartField, &'static str)] =
                            if let Some(g) = &global {
                                g.apply_to(&mut folded);
                                &g.field_notes
                            } else {
                                &[]
                            };
                        let mappings = astrogram::transcript::diff(source, &folded, notes);
                        print_transcript(&mappings);
                    }
                    verified += 1;
                }
            };
            p.write_charts(&merged, &sink, &mut on_landed)?
        };

        if inline {
            let created = write_results.iter().filter(|r| r.is_some()).count();
            eprintln!(
                "verified {verified}/{created} charts (create response — no readback) from {}",
                p.site_display()
            );
        } else if verify {
            if let Err(e) = verify_and_report(p, &merged, &write_results, verbosity.is_quiet()) {
                eprintln!("write succeeded; readback verification failed: {e}");
            }
        } else {
            // No transcript follows, so print the write results permanently here.
            let total_new = write_results.iter().filter(|r| r.is_some()).count();
            let w = total_new.to_string().len();
            let mut n = 0usize;
            for (chart, status) in merged.iter().zip(write_results.iter()) {
                if let Some(s) = status {
                    n += 1;
                    eprintln!("[{n:0>w$}/{total_new}] {}  {s}", chart.name);
                }
            }
        }
    } else {
        let p = out_path.expect("out_path set for file target");
        if verbosity.is_verbose() && !to_stdout {
            for name in &skipped {
                eprintln!("  dup: {name}");
            }
        }
        write_file_target(p, out_target, &merged)?;
        if !to_stdout {
            if existing_count > 0 {
                eprintln!("  existing: {existing_count:>6}");
            }
            eprintln!("  in:       {new_input_count:>6}");
            eprintln!("  dupes:    {dupes:>6}");
            eprintln!("  out:      {:>6}", merged.len());
            eprintln!("wrote {}", display_path(p));
        }
    }
    Ok(())
}

// ── normalize in-place ────────────────────────────────────────────────────────

fn cmd_normalize_inplace(inputs: &[PathBuf]) -> Result<()> {
    if inputs.is_empty() {
        return Err(exit::NoInputError {
            message: "at least one input file is required for --normalize".to_string(),
        }
        .into());
    }
    for path in inputs {
        let target = Format::from_path(path)
            .with_context(|| format!("cannot detect target from '{}'", display_path(path)))?;
        let mut charts = read_file_target(path, target)
            .with_context(|| format!("reading {}", display_path(path)))?;
        for chart in &mut charts {
            normalize_chart(chart);
        }
        write_file_target(path, target, &charts)
            .with_context(|| format!("writing {}", display_path(path)))?;
        eprintln!(
            "normalised {} charts in {}",
            charts.len(),
            display_path(path)
        );
    }
    Ok(())
}

// ── clear ─────────────────────────────────────────────────────────────────────

fn cmd_clear(provider: WebProvider) -> Result<()> {
    let sink = crate::providers::CliSink;
    let (charts, ids) = provider.fetch_all_with_ids(&sink)?;
    if charts.is_empty() {
        eprintln!("no charts found — nothing to delete.");
        return Ok(());
    }
    eprint!(
        "Delete all {} charts from {}? [y/N] ",
        charts.len(),
        provider.site_display()
    );
    let _ = std::io::stderr().flush();
    let mut answer = String::new();
    std::io::stdin()
        .read_line(&mut answer)
        .context("reading confirmation")?;
    if !matches!(answer.trim().to_lowercase().as_str(), "y" | "yes") {
        eprintln!("Aborted.");
        return Ok(());
    }
    let total = charts.len();
    let w = total.to_string().len();
    for (i, (chart, id)) in charts.iter().zip(ids.iter()).enumerate() {
        let n = i + 1;
        let name: String = chart.name.chars().take(40).collect();
        eprint!("[{n:0>w$}/{total}] {name}  ");
        let _ = std::io::stderr().flush();
        match provider.delete_one(id) {
            Ok(()) => eprintln!("deleted"),
            Err(e) => eprintln!("[!] {e}"),
        }
    }
    eprintln!("cleared {} charts from {}", total, provider.site_display());
    Ok(())
}

// ── consolidate ───────────────────────────────────────────────────────────────

fn cmd_consolidate(provider: WebProvider, cli: &Cli) -> Result<()> {
    use astrogram::consolidate::group_candidates;
    use astrogram::decision_log::{self, DecisionLog};

    let sink = crate::providers::CliSink;
    let log_path = cli
        .decision_log
        .clone()
        .unwrap_or_else(decision_log::default_path);

    eprintln!("Decision log: {}", display_path(&log_path));

    let (charts, ids) = provider
        .fetch_all_with_ids(&sink)
        .with_context(|| format!("fetching charts from {}", provider.site_display()))?;
    eprintln!("Fetched {} charts.", charts.len());

    let all_groups = group_candidates(&charts);
    let groups: Vec<Vec<usize>> = all_groups.into_iter().filter(|g| g.len() > 1).collect();
    if groups.is_empty() {
        eprintln!("No candidate groups found.  Nothing to consolidate.");
        return Ok(());
    }
    eprintln!("Found {} candidate group(s).", groups.len());

    let prior = DecisionLog::read_all(&log_path).context("reading decision log")?;
    let already_decided: std::collections::HashSet<String> =
        prior.iter().map(|r| r.group_id.clone()).collect();
    if !already_decided.is_empty() {
        eprintln!(
            "Resuming: {} group(s) already in decision log.",
            already_decided.len()
        );
    }

    let mut log = DecisionLog::open(&log_path).context("opening decision log")?;
    // Scope the stdin/stdout locks so they're released before the apply phase:
    // stdin's mutex is *not* reentrant, so the read_line for the confirmation
    // prompt below would deadlock if stdin_lock were still alive.
    let outcome = {
        let stdin = std::io::stdin();
        let mut stdin_lock = stdin.lock();
        let stdout = std::io::stdout();
        let mut stdout_lock = stdout.lock();
        consolidate_ui::run_loop(
            &groups,
            &charts,
            &ids,
            &already_decided,
            &mut log,
            &mut stdin_lock,
            &mut stdout_lock,
        )
        .context("consolidation loop")?
    };
    drop(log);

    if matches!(outcome, consolidate_ui::RunOutcome::QuitEarly) {
        eprintln!("Quit before completion.  Decisions so far are in the log.");
    }

    let all = DecisionLog::read_all(&log_path).context("re-reading decision log")?;
    let drops = decision_log::drops_to_apply(&all);
    if drops.is_empty() {
        eprintln!("No drops to apply.");
        return Ok(());
    }
    eprint!(
        "About to delete {} chart(s) from {}.  Proceed? [y/N] ",
        drops.len(),
        provider.site_display(),
    );
    let mut answer = String::new();
    std::io::stdin()
        .read_line(&mut answer)
        .context("reading confirmation")?;
    if !matches!(answer.trim().to_lowercase().as_str(), "y" | "yes") {
        eprintln!("Apply aborted.  Decisions remain in the log; re-run to resume.");
        return Ok(());
    }
    let total = drops.len();
    let mut failed = 0usize;
    for (i, id) in drops.iter().enumerate() {
        if i > 0 {
            std::thread::sleep(std::time::Duration::from_millis(cli.delay));
        }
        eprint!("[{:>3}/{}] {id}  ", i + 1, total);
        let _ = std::io::stderr().flush();
        match provider.delete_one(id) {
            Ok(()) => eprintln!("deleted"),
            Err(e) => {
                eprintln!("[!] {e}");
                failed += 1;
            }
        }
    }
    eprintln!("Deleted {}/{total} chart(s).", total - failed);
    Ok(())
}

// ── target I/O ────────────────────────────────────────────────────────────────

fn read_file_target(path: &Path, target: Target) -> Result<Vec<astrogram::chart::Chart>> {
    // Friendly messages for directions convert::read_bytes rejects as UnsupportedDirection.
    match target {
        Target::Luna => bail!("use --from luna rather than passing a file path"),
        Target::Astrocom => bail!("use --from astrocom rather than passing a file path"),
        Target::Astrotheoros => bail!("use --from astrotheoros rather than passing a file path"),
        Target::Json => bail!("JZOD (json) is a write-only format; reading is not supported"),
        Target::Raw => bail!("raw is a write-only format; reading is not supported"),
        _ => {}
    }
    if path == Path::new("-") {
        use std::io::Read as _;
        let mut buf = Vec::new();
        std::io::stdin().read_to_end(&mut buf)?;
        // Bare `?` (not `.map_err(|e| anyhow!("{e}"))`) so the concrete
        // `ChartError` survives inside the `anyhow::Error` — `exit::classify`
        // downcasts to it to distinguish `ChartParse` from `Io`.
        Ok(astrogram::convert::read_bytes(target, &buf)?)
    } else {
        Ok(astrogram::convert::read_path(target, path)?)
    }
}

/// Write bytes to a file or to stdout when `path` is `"-"`.
fn write_bytes_to(path: &Path, data: &[u8]) -> Result<()> {
    if path == Path::new("-") {
        std::io::stdout().write_all(data)?;
    } else {
        std::fs::write(path, data)?;
    }
    Ok(())
}

fn write_file_target(
    path: &Path,
    target: Target,
    charts: &[astrogram::chart::Chart],
) -> Result<()> {
    // Friendly messages for directions convert::write_bytes rejects as UnsupportedDirection.
    match target {
        Target::Aaf => bail!("AAF is a read-only format; choose a writable --to/--output"),
        Target::Jhd => bail!("JHD is a read-only format; choose a writable --to/--output"),
        Target::Luna => bail!("use --to luna for writing to LUNA"),
        Target::Astrocom => bail!("use --to astrocom for writing to astro.com"),
        Target::Astrotheoros => bail!("use --to astrotheoros for writing to astrotheoros.com"),
        _ => {}
    }
    // Read the file being overwritten so the writer can preserve format-specific
    // state (SFcht keeps its header description); `write_preserving` ignores it
    // for formats that carry no such state. No existing bytes when writing to
    // stdout. Path/stdout I/O and the write itself stay here, so a write failure
    // keeps its existing exit class.
    let existing = if path != Path::new("-") {
        std::fs::read(path).ok()
    } else {
        None
    };
    // Bare `?` (see `read_file_target`) so `ChartError` survives for
    // `exit::classify`.
    let out = astrogram::convert::write_preserving(
        target,
        charts,
        existing.as_deref(),
        Some(blackmoon_generator()),
    )?;
    write_bytes_to(path, &out)
}

fn detect_shell() -> Option<clap_complete::Shell> {
    let shell = std::env::var("SHELL").ok()?;
    let name = std::path::Path::new(&shell).file_name()?.to_str()?;
    name.parse().ok()
}

fn now_secs() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// One-line status tally for a chart's transcript.
fn transcript_summary(m: &[astrogram::transcript::FieldMapping]) -> String {
    use astrogram::transcript::FieldStatus::{Dropped, Filled, Preserved, Transformed};
    let (mut p, mut t, mut d, mut f) = (0, 0, 0, 0);
    for fm in m {
        match fm.status {
            Preserved => p += 1,
            Transformed => t += 1,
            Dropped => d += 1,
            Filled => f += 1,
            astrogram::transcript::FieldStatus::Absent => {}
        }
    }
    format!("{p} preserved, {t} transformed, {d} dropped, {f} filled")
}

/// Truncate a display string to `width` with an ellipsis.
fn clip(s: &str, width: usize) -> String {
    if s.chars().count() <= width {
        s.to_string()
    } else {
        let kept: String = s.chars().take(width.saturating_sub(1)).collect();
        format!("{kept}…")
    }
}

/// Print one chart's field-by-field `source → landed` transcript (no header line).
fn print_transcript(m: &[astrogram::transcript::FieldMapping]) {
    use astrogram::transcript::FieldStatus::{Dropped, Filled, Preserved};
    for fm in m {
        let glyph = if fm.status == Preserved { "=" } else { "→" };
        let from = match fm.status {
            Filled => "(filled)".to_string(),
            _ => clip(&fm.from, 20),
        };
        let to = match fm.status {
            Dropped => "(dropped)".to_string(),
            _ => clip(&fm.to, 20),
        };
        let note = fm.note.map(|n| format!(" ({n})")).unwrap_or_default();
        eprintln!("  {:<18}{:<22}{glyph} {to}{note}", fm.label, from);
    }
    eprintln!("  → {}", transcript_summary(m));
}

/// Read written charts back from a web sink and print per-chart transcripts.
/// Each block's header carries the chart's write status (`[n/N] name created uuid=…`
/// for newly-written charts, or just the name for pre-existing ones).
///
/// The "shared birth datetime" note and per-chart transcripts are
/// data-affecting disclosures (readback pairing caveats, field-level
/// preserved/transformed/dropped/filled outcomes) and are never suppressed by
/// `quiet`; only the read-back fetch's own progress narration is gated, via
/// [`providers::QuietAwareSink`].
fn verify_and_report(
    provider: &WebProvider,
    written: &[astrogram::chart::Chart],
    write_results: &[Option<String>],
    quiet: bool,
) -> Result<()> {
    if astrogram::transcript::has_tied_datetimes(written) {
        eprintln!(
            "note: some charts share a birth datetime; readback pairing for those is best-effort (input order)"
        );
    }
    let global = provider.fetch_global_settings()?;
    let sink = providers::QuietAwareSink::new(crate::providers::CliSink, quiet);
    let (landed_all, _ids) = provider.fetch_all_with_ids(&sink)?;
    let rows =
        astrogram::pipeline::verify_rows(written, &landed_all, write_results, global.as_ref());

    let total_new = write_results.iter().filter(|r| r.is_some()).count();
    let w = total_new.to_string().len();
    let mut new_idx = 0usize;
    let mut verified = 0;
    for row in &rows {
        let header = match &row.write_status {
            Some(s) => {
                new_idx += 1;
                format!("[{new_idx:0>w$}/{total_new}] {}  {s}", row.name)
            }
            None => row.name.clone(),
        };
        match &row.outcome {
            astrogram::pipeline::LandedOutcome::NotFound => {
                eprintln!("{header}\n  not found on readback — skipped");
            }
            astrogram::pipeline::LandedOutcome::Diffed(mappings) => {
                eprintln!("{header}");
                print_transcript(mappings);
                verified += 1;
            }
        }
    }
    eprintln!(
        "verified {}/{} charts (readback from {})",
        verified,
        written.len(),
        provider.site_display()
    );
    Ok(())
}

/// Print a neutral per-chart disclosure of fields the sink cannot store.
/// Returns the number of charts that lose data.
/// When `to_stdout` is true all output is suppressed.
fn report_drops(
    merged: &[astrogram::chart::Chart],
    source_of: &std::collections::HashMap<providers::DatetimeKey, Format>,
    sink: Format,
    to_stdout: bool,
) -> usize {
    let summary = astrogram::pipeline::drop_summary(merged, source_of, sink);
    if summary.affected > 0 && !to_stdout {
        let sink_name = sink.spec().slug;
        let lost_list = summary.fields.join(", ");
        eprintln!(
            "{sink_name} does not store: {lost_list}. ({} chart(s) affected)",
            summary.affected
        );
    }
    summary.affected
}

#[cfg(test)]
mod cookie_import_tests {
    use super::*;

    // ── parse_browser ─────────────────────────────────────────────────────────

    #[test]
    fn parse_browser_all_returns_grant_all() {
        assert_eq!(parse_browser("all"), Ok(GrantArg::All));
    }

    #[test]
    fn parse_browser_firefox_returns_one_firefox() {
        assert_eq!(
            parse_browser("firefox"),
            Ok(GrantArg::One(Browser::Firefox))
        );
    }

    #[test]
    fn parse_browser_chrome_returns_one_chrome() {
        assert_eq!(parse_browser("chrome"), Ok(GrantArg::One(Browser::Chrome)));
    }

    #[test]
    fn parse_browser_safari_returns_one_safari() {
        assert_eq!(parse_browser("safari"), Ok(GrantArg::One(Browser::Safari)));
    }

    #[test]
    fn parse_browser_unknown_returns_err() {
        let result = parse_browser("nope");
        assert!(result.is_err(), "expected Err for unknown browser 'nope'");
        let msg = result.unwrap_err();
        assert!(
            msg.contains("nope"),
            "error should name the unknown value, got: {msg}"
        );
    }

    #[test]
    fn parse_browser_all_valid_names_succeed() {
        // All documented browser slugs must parse without error.
        for name in &[
            "chrome", "chromium", "brave", "edge", "opera", "vivaldi", "whale", "firefox",
            "safari", "all",
        ] {
            assert!(
                parse_browser(name).is_ok(),
                "parse_browser({name:?}) should succeed"
            );
        }
    }

    // ── grant_choice ──────────────────────────────────────────────────────────

    #[test]
    fn grant_choice_none_flag_is_no_grant() {
        assert_eq!(grant_choice(&None), GrantChoice::NoGrant);
    }

    #[test]
    fn grant_choice_grant_all_is_all_stores() {
        assert_eq!(grant_choice(&Some(GrantArg::All)), GrantChoice::AllStores);
    }

    #[test]
    fn grant_choice_grant_one_chrome_is_one_chrome() {
        assert_eq!(
            grant_choice(&Some(GrantArg::One(Browser::Chrome))),
            GrantChoice::One(Browser::Chrome)
        );
    }

    #[test]
    fn grant_choice_grant_one_firefox_is_one_firefox() {
        assert_eq!(
            grant_choice(&Some(GrantArg::One(Browser::Firefox))),
            GrantChoice::One(Browser::Firefox)
        );
    }

    // ── CLI flag parsing ──────────────────────────────────────────────────────

    #[test]
    fn flag_absent_yields_no_grant() {
        let cli = Cli::parse_from(["blackmoon"]);
        assert_eq!(grant_choice(&cli.grant_cookie_access), GrantChoice::NoGrant);
    }

    #[test]
    fn bare_flag_yields_all_stores() {
        let cli = Cli::parse_from(["blackmoon", "--grant-cookie-access"]);
        assert_eq!(
            grant_choice(&cli.grant_cookie_access),
            GrantChoice::AllStores
        );
    }

    #[test]
    fn flag_equals_all_yields_all_stores() {
        let cli = Cli::parse_from(["blackmoon", "--grant-cookie-access=all"]);
        assert_eq!(
            grant_choice(&cli.grant_cookie_access),
            GrantChoice::AllStores
        );
    }

    #[test]
    fn flag_equals_firefox_yields_one_firefox() {
        let cli = Cli::parse_from(["blackmoon", "--grant-cookie-access=firefox"]);
        assert_eq!(
            grant_choice(&cli.grant_cookie_access),
            GrantChoice::One(Browser::Firefox)
        );
    }

    #[test]
    fn flag_equals_chrome_yields_one_chrome() {
        let cli = Cli::parse_from(["blackmoon", "--grant-cookie-access=chrome"]);
        assert_eq!(
            grant_choice(&cli.grant_cookie_access),
            GrantChoice::One(Browser::Chrome)
        );
    }

    #[test]
    fn repeated_bare_grant_flag_is_allowed_last_wins() {
        // Simulates `--grant-cookie-access` baked into a shell alias, then
        // the bare flag passed again on the command line.
        let cli = Cli::parse_from([
            "blackmoon",
            "--grant-cookie-access",
            "--grant-cookie-access",
        ]);
        assert_eq!(
            grant_choice(&cli.grant_cookie_access),
            GrantChoice::AllStores
        );
    }

    #[test]
    fn alias_default_then_explicit_browser_last_wins() {
        // Alias provides bare `--grant-cookie-access`; user overrides with a browser.
        let cli = Cli::parse_from([
            "blackmoon",
            "--grant-cookie-access",
            "--grant-cookie-access=firefox",
        ]);
        assert_eq!(
            grant_choice(&cli.grant_cookie_access),
            GrantChoice::One(Browser::Firefox)
        );
    }

    #[test]
    fn explicit_browser_then_alias_default_last_wins() {
        // Reverse order: explicit browser first, bare flag last → bare wins.
        let cli = Cli::parse_from([
            "blackmoon",
            "--grant-cookie-access=firefox",
            "--grant-cookie-access",
        ]);
        assert_eq!(
            grant_choice(&cli.grant_cookie_access),
            GrantChoice::AllStores
        );
    }
}

#[cfg(test)]
mod credential_tests {
    use super::*;

    #[test]
    fn credential_env_vars_use_blackmoon_prefix() {
        use clap::CommandFactory;
        let cmd = Cli::command();
        let env_of = |id: &str| -> Option<String> {
            cmd.get_arguments()
                .find(|a| a.get_id() == id)
                .and_then(|a| a.get_env())
                .map(|e| e.to_string_lossy().into_owned())
        };
        assert_eq!(
            env_of("astrotheoros_user").as_deref(),
            Some("BLACKMOON_ASTROTHEOROS_USER")
        );
        assert_eq!(
            env_of("astrotheoros_pass").as_deref(),
            Some("BLACKMOON_ASTROTHEOROS_PASS")
        );
        assert_eq!(
            env_of("astrotheoros_token").as_deref(),
            Some("BLACKMOON_ASTROTHEOROS_TOKEN")
        );
        assert_eq!(
            env_of("astrocom_user").as_deref(),
            Some("BLACKMOON_ASTROCOM_USER")
        );
        assert_eq!(
            env_of("astrocom_pass").as_deref(),
            Some("BLACKMOON_ASTROCOM_PASS")
        );
        assert_eq!(
            env_of("astrocom_token").as_deref(),
            Some("BLACKMOON_ASTROCOM_TOKEN")
        );
        assert_eq!(
            env_of("luna_token").as_deref(),
            Some("BLACKMOON_LUNA_TOKEN")
        );
    }

    #[test]
    fn file_only_convert_flags_expose_blackmoon_env_vars() {
        use clap::CommandFactory;
        let cmd = Cli::command();
        let env_of = |id: &str| -> Option<String> {
            cmd.get_arguments()
                .find(|a| a.get_id() == id)
                .and_then(|a| a.get_env())
                .map(|e| e.to_string_lossy().into_owned())
        };

        for (id, var) in [
            ("inputs", "BLACKMOON_INPUTS"),
            ("output", "BLACKMOON_OUTPUT"),
            ("to", "BLACKMOON_TO"),
            ("target", "BLACKMOON_TARGET"),
            ("normalize", "BLACKMOON_NORMALIZE"),
            ("strict", "BLACKMOON_STRICT"),
            ("fill_house", "BLACKMOON_FILL_HOUSE"),
            ("fill_zodiac", "BLACKMOON_FILL_ZODIAC"),
            ("fill_locus", "BLACKMOON_FILL_LOCUS"),
            ("verbose", "BLACKMOON_VERBOSE"),
            ("quiet", "BLACKMOON_QUIET"),
        ] {
            assert_eq!(env_of(id).as_deref(), Some(var), "arg id: {id}");
        }

        // Web-only, destructive, meta, and consent flags stay OFF the env surface.
        for id in [
            "from",
            "delay",
            "luna_resume_from",
            "no_verify",
            "decision_log",
            "clear",
            "consolidate",
            "generate_completion",
            "capabilities",
            "grant_cookie_access",
            "cookies_profile",
            "ua",
        ] {
            assert_eq!(env_of(id), None, "arg id must have no env: {id}");
        }
    }

    #[test]
    fn verbosity_resolves_precedence() {
        assert_eq!(Verbosity::resolve(false, false), Verbosity::Normal);
        assert_eq!(Verbosity::resolve(true, false), Verbosity::Quiet);
        assert_eq!(Verbosity::resolve(false, true), Verbosity::Verbose);
        // clap forbids both at once; if it ever reaches resolve, verbose wins
        assert_eq!(Verbosity::resolve(true, true), Verbosity::Verbose);
    }

    #[test]
    fn verbosity_is_quiet_and_is_verbose() {
        assert!(Verbosity::Quiet.is_quiet());
        assert!(!Verbosity::Quiet.is_verbose());
        assert!(Verbosity::Verbose.is_verbose());
        assert!(!Verbosity::Verbose.is_quiet());
        assert!(!Verbosity::Normal.is_quiet());
        assert!(!Verbosity::Normal.is_verbose());
    }

    #[test]
    fn quiet_and_verbose_conflict_in_clap() {
        let result = Cli::try_parse_from(["blackmoon", "--quiet", "--verbose"]);
        assert!(result.is_err(), "clap should reject --quiet with --verbose");
    }

    // The cookie-only-chain predicate now lives in `astrogram::auth`
    // (`only_cookie_source`) and is tested there.
}

#[cfg(test)]
mod version_tests {
    use super::*;
    use clap::{CommandFactory, error::ErrorKind};

    #[test]
    fn cli_supports_version_flag() {
        // `blackmoon --version` must be wired, same as starcat.
        let err = Cli::command()
            .try_get_matches_from(["blackmoon", "--version"])
            .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::DisplayVersion);
    }
}

#[cfg(test)]
mod provider_tests {
    use super::*;
    use clap::Parser;

    /// Serializes all env-mutating tests so that `unsafe env::remove_var` and
    /// the subsequent `Cli::parse_from` (which reads env vars via clap) never
    /// overlap across threads.  `cargo test` runs tests in parallel by default,
    /// so a per-module mutex is required for soundness.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Clear all credential env vars so that `Cli::parse_from` in tests is not
    /// contaminated by real credentials that may be set in the shell environment.
    /// Returns the `MutexGuard` so the caller holds the lock for the entire test
    /// body, preventing concurrent env access from another test in this module.
    fn clear_cred_env() -> std::sync::MutexGuard<'static, ()> {
        let guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        for var in &[
            "BLACKMOON_LUNA_TOKEN",
            "BLACKMOON_ASTROCOM_TOKEN",
            "BLACKMOON_ASTROCOM_USER",
            "BLACKMOON_ASTROCOM_PASS",
            "BLACKMOON_ASTROTHEOROS_TOKEN",
            "BLACKMOON_ASTROTHEOROS_USER",
            "BLACKMOON_ASTROTHEOROS_PASS",
        ] {
            // SAFETY: ENV_LOCK serializes all env reads and writes within this
            // module's tests.  No concurrent thread mutates or reads these vars
            // while the guard is held, so remove_var is sound here.
            unsafe { std::env::remove_var(var) };
        }
        guard
    }

    #[test]
    fn resolve_provider_luna_no_creds_bails() {
        let _guard = clear_cred_env();
        let cli = Cli::parse_from(["blackmoon", "--from", "luna"]);
        let err = resolve_provider(Target::Luna, &cli).unwrap_err();
        assert!(err.to_string().contains("luna-token"), "unexpected: {err}");
    }

    #[test]
    fn resolve_provider_luna_token_chain_assembled() {
        let _guard = clear_cred_env();
        let cli = Cli::parse_from(["blackmoon", "--luna-token", "abc123"]);
        // Chain assembly succeeded (token present). The result is either Ok (if the
        // session builds without a network probe) or an Err that is NOT a "no creds" bail.
        match resolve_provider(Target::Luna, &cli) {
            Ok(_) => {} // session built — acceptable
            Err(e) => {
                let msg = e.to_string();
                assert!(
                    !msg.contains("luna-token") && !msg.contains("no LUNA credentials"),
                    "unexpected early bail (no creds?): {msg}"
                );
            }
        }
    }

    #[test]
    fn resolve_provider_astrocom_token_chain_assembled() {
        let _guard = clear_cred_env();
        let cli = Cli::parse_from(["blackmoon", "--astrocom-token", "test_cid"]);
        // Chain assembly succeeded. Result is either Ok (session built) or a
        // network/auth error — NOT a "no credentials" bail.
        // When Ok, login field must be None (no login creds in chain).
        match resolve_provider(Target::Astrocom, &cli) {
            Ok(provider) => {
                assert!(
                    matches!(provider, WebProvider::Astrocom { creds: None, .. }),
                    "token-only chain must yield creds: None"
                );
            }
            Err(e) => {
                let msg = e.to_string();
                assert!(
                    !msg.contains("astrocom-token") && !msg.contains("no astro.com credentials"),
                    "unexpected early bail (no creds?): {msg}"
                );
            }
        }
    }

    #[test]
    fn resolve_provider_astrocom_half_creds_bails() {
        let _guard = clear_cred_env();
        let cli = Cli::parse_from(["blackmoon", "--astrocom-user", "user@example.com"]);
        let err = resolve_provider(Target::Astrocom, &cli).unwrap_err();
        assert!(
            err.to_string().contains("astrocom-pass"),
            "unexpected: {err}"
        );
    }

    #[test]
    fn resolve_provider_astrocom_no_creds_bails() {
        let _guard = clear_cred_env();
        let cli = Cli::parse_from(["blackmoon"]);
        let err = resolve_provider(Target::Astrocom, &cli).unwrap_err();
        assert!(
            err.to_string().contains("astrocom-token"),
            "unexpected: {err}"
        );
    }

    #[test]
    fn resolve_provider_astrotheoros_token_attempts_network() {
        let _guard = clear_cred_env();
        let cli = Cli::parse_from(["blackmoon", "--astrotheoros-token", "jwt:sess:uat"]);
        // authenticate probes the network; chain assembly succeeded so we get an
        // auth/network error rather than a "no credentials" bail.
        let err = resolve_provider(Target::Astrotheoros, &cli).unwrap_err();
        let msg = err.to_string();
        assert!(
            !msg.contains("no astrotheoros.com credentials"),
            "unexpected early bail (no creds?): {msg}"
        );
    }

    #[test]
    fn resolve_provider_astrotheoros_half_creds_bails() {
        let _guard = clear_cred_env();
        let cli = Cli::parse_from(["blackmoon", "--astrotheoros-user", "user@example.com"]);
        let err = resolve_provider(Target::Astrotheoros, &cli).unwrap_err();
        assert!(
            err.to_string().contains("astrotheoros-pass"),
            "unexpected: {err}"
        );
    }

    #[test]
    fn report_drops_counts_affected_charts() {
        use astrogram::chart::{
            Chart, CoordinateSystem, EventType, HouseSystem, Latitude, Longitude, Zodiac,
        };
        use std::collections::HashMap;
        let mut c = Chart {
            name: "Helio Native".into(),
            secondary_name: None,
            city: Some("c".into()),
            region: None,
            longitude: Longitude::new(0.0).unwrap(),
            latitude: Latitude::new(0.0).unwrap(),
            year: 2000,
            month: 1,
            day: 1,
            hour: 0,
            minute: 0,
            second: 0,
            tz_offset_hours: 0.0,
            tz_abbreviation: None,
            is_lmt: false,
            event_type: EventType::Unspecified,
            source_rating: None,
            house_system: HouseSystem::Placidus,
            zodiac: Zodiac::Tropical,
            coordinate_system: CoordinateSystem::Heliocentric,
            sub_charts: vec![],
            notes: None,
        };
        c.notes = Some("n".into());
        let mut source_of = HashMap::new();
        source_of.insert(astrogram::provider::key(&c), Target::Sfcht);
        let n = report_drops(&[c], &source_of, Target::Astrocom, false);
        assert_eq!(n, 1);
    }

    #[test]
    fn consolidate_dispatch_requires_web_target_not_just_luna() {
        // is_web_target drives the consolidate dispatch; all three web targets must pass.
        assert!(is_web_target(Target::Astrocom));
        assert!(is_web_target(Target::Astrotheoros));
        assert!(is_web_target(Target::Luna));
        // File targets must not pass (consolidate bails on them).
        assert!(!is_web_target(Target::Sfcht));
        assert!(!is_web_target(Target::Zeus));
        assert!(!is_web_target(Target::Adb));
        assert!(!is_web_target(Target::Aaf));
    }
}

#[cfg(test)]
mod chain_label_tests {
    use super::*;

    #[test]
    fn source_label_describes_each_chain_position() {
        let kinds = [SourceKind::Cookie, SourceKind::Token, SourceKind::Login];
        assert_eq!(source_label(&kinds, 0), "browser cookie");
        assert_eq!(source_label(&kinds, 1), "token");
        assert_eq!(source_label(&kinds, 2), "login");
        let two = [SourceKind::Cookie, SourceKind::Login];
        assert_eq!(source_label(&two, 1), "login");
    }
}

#[cfg(test)]
mod naming_contract {
    use super::*;
    use astrogram::format::{Auth, FORMATS};
    use clap::CommandFactory;
    use std::collections::HashSet;

    fn long_flags() -> HashSet<String> {
        Cli::command()
            .get_arguments()
            .filter_map(|a| a.get_long().map(String::from))
            .collect()
    }
    fn env_names() -> HashSet<String> {
        Cli::command()
            .get_arguments()
            .filter_map(|a| a.get_env().and_then(|e| e.to_str()).map(String::from))
            .collect()
    }

    /// Every credential flag/env in blackmoon's CLI must derive from the library
    /// registry slug, per the format's auth.  Env names carry the `BLACKMOON_`
    /// prefix so they cannot collide with bare library-level vars.
    #[test]
    fn credential_surface_matches_auth() {
        let longs = long_flags();
        let envs = env_names();
        for s in FORMATS {
            let upper = s.slug.to_uppercase();
            let cred_flags = [
                format!("{}-user", s.slug),
                format!("{}-pass", s.slug),
                format!("{}-token", s.slug),
            ];
            let cred_envs = [
                format!("BLACKMOON_{upper}_USER"),
                format!("BLACKMOON_{upper}_PASS"),
                format!("BLACKMOON_{upper}_TOKEN"),
            ];
            match s.auth {
                Auth::None => {
                    for f in &cred_flags {
                        assert!(!longs.contains(f), "{} must NOT expose --{f}", s.slug);
                    }
                    for e in &cred_envs {
                        assert!(!envs.contains(e), "{} must NOT expose env {e}", s.slug);
                    }
                }
                Auth::Token => {
                    // token-only (e.g. luna while login is deferred): the -token
                    // flag/env must exist; user/pass must NOT.
                    assert!(
                        longs.contains(&cred_flags[2]),
                        "missing flag --{} for {}",
                        cred_flags[2],
                        s.slug
                    );
                    assert!(
                        envs.contains(&cred_envs[2]),
                        "missing env {} for {}",
                        cred_envs[2],
                        s.slug
                    );
                    for f in &cred_flags[..2] {
                        assert!(
                            !longs.contains(f),
                            "{} must NOT expose --{f} (login deferred)",
                            s.slug
                        );
                    }
                    for e in &cred_envs[..2] {
                        assert!(
                            !envs.contains(e),
                            "{} must NOT expose env {e} (login deferred)",
                            s.slug
                        );
                    }
                }
                Auth::LoginOrToken => {
                    for f in &cred_flags {
                        assert!(longs.contains(f), "{} must expose --{f}", s.slug);
                    }
                    for e in &cred_envs {
                        assert!(envs.contains(e), "{} must expose env {e}", s.slug);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod ua_tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn ua_bare_parses_static_with_string_parses_custom() {
        let bare = Cli::parse_from(["blackmoon", "--grant-cookie-access", "--ua"]);
        assert!(matches!(bare.ua, Some(UaArg::Static)));
        let custom = Cli::parse_from(["blackmoon", "--grant-cookie-access", "--ua=Custom/1.0"]);
        assert!(matches!(custom.ua, Some(UaArg::Custom(ref s)) if s == "Custom/1.0"));
    }

    #[test]
    fn ua_browser_keyword_parses_browser_case_insensitively() {
        for kw in ["browser", "Browser", "BROWSER"] {
            let cli =
                Cli::parse_from(["blackmoon", "--grant-cookie-access", &format!("--ua={kw}")]);
            assert!(matches!(cli.ua, Some(UaArg::Browser)), "{kw}");
        }
    }

    #[test]
    fn ua_requires_grant_cookie_access() {
        // --ua without --grant-cookie-access is rejected by clap.
        let res = Cli::try_parse_from(["blackmoon", "--ua=Custom/1.0"]);
        assert!(res.is_err());
    }

    #[test]
    fn select_ua_choice_precedence() {
        use astrogram::user_agent::UaChoice;
        // No grant -> SelfReported regardless.
        assert!(matches!(
            select_ua_choice(false, &None, None),
            UaChoice::SelfReported
        ));
        // Grant, no --ua -> SelfReported even when a cookie authenticated the
        // session. Browser impersonation is opt-in; cookie *read* access never
        // implies UA *impersonation*.
        assert!(matches!(
            select_ua_choice(true, &None, Some("UA".into())),
            UaChoice::SelfReported
        ));
        // Grant, --ua browser, cookie present -> Cookie (explicit mimic).
        assert!(matches!(
            select_ua_choice(true, &Some(UaArg::Browser), Some("UA".into())),
            UaChoice::Cookie(ref s) if s == "UA"
        ));
        // Grant, --ua browser, but no cookie was used -> honest fallback.
        assert!(matches!(
            select_ua_choice(true, &Some(UaArg::Browser), None),
            UaChoice::SelfReported
        ));
        // Grant, bare --ua -> Static.
        assert!(matches!(
            select_ua_choice(true, &Some(UaArg::Static), Some("UA".into())),
            UaChoice::Static
        ));
        // Grant, custom --ua -> Custom.
        assert!(matches!(
            select_ua_choice(true, &Some(UaArg::Custom("X".into())), Some("UA".into())),
            UaChoice::Custom(ref s) if s == "X"
        ));
    }
}

#[cfg(test)]
mod app_product_tests {
    use super::*;

    #[test]
    fn blackmoon_app_product_is_major_minor() {
        assert_eq!(
            blackmoon_app_product().to_string(),
            format!(
                "Blackmoon/{}",
                astrogram::user_agent::major_minor(env!("CARGO_PKG_VERSION"))
            )
        );
    }
}

#[cfg(test)]
mod convert_tests {
    use super::*;

    #[test]
    fn resolve_fill_house_parses_flag() {
        use astrogram::chart::HouseSystem;
        assert_eq!(
            HouseSystem::from_str_slug("placidus"),
            Some(HouseSystem::Placidus)
        );
        assert_eq!(
            HouseSystem::from_str_slug("whole-sign"),
            Some(HouseSystem::WholeSign)
        );
        assert!(HouseSystem::from_str_slug("nonsense").is_none());
    }

    #[test]
    fn fill_prompt_line_lists_accepted_values_and_default_in_brackets() {
        use astrogram::pipeline::fill_spec;

        let spec = fill_spec(astrogram::capability::ChartField::HouseSystem)
            .expect("HouseSystem has a FillSpec");
        let line = fill_prompt_line(spec);
        assert!(
            spec.accepted.len() >= 3,
            "fixture spec should offer at least 3 accepted values"
        );
        for accepted in spec.accepted {
            assert!(line.contains(accepted), "line missing '{accepted}': {line}");
        }
        assert!(
            line.contains(&format!("[{}]", spec.default_slug)),
            "line missing default in brackets: {line}"
        );
    }

    /// The fill flags no longer reject a bad value at parse time (so a bad
    /// value can be classified as `InputError`/exit 3 downstream), but they
    /// must still expose their accepted slugs as clap *possible values* so the
    /// values tab-complete and list in `--help`.
    #[test]
    fn fill_flags_still_expose_accepted_slugs_as_possible_values() {
        use clap::CommandFactory;
        let cmd = Cli::command();
        let possible_of = |id: &str| -> Vec<String> {
            cmd.get_arguments()
                .find(|a| a.get_id() == id)
                .expect("arg present")
                .get_possible_values()
                .iter()
                .map(|p| p.get_name().to_string())
                .collect()
        };

        for (id, accepted) in [
            (
                "fill_house",
                astrogram::chart::HouseSystem::accepted_slugs(),
            ),
            ("fill_zodiac", astrogram::chart::Zodiac::accepted_slugs()),
            (
                "fill_locus",
                astrogram::chart::CoordinateSystem::accepted_slugs(),
            ),
        ] {
            let values = possible_of(id);
            assert_eq!(
                values,
                accepted
                    .iter()
                    .map(|s| (*s).to_string())
                    .collect::<Vec<_>>(),
                "possible values for {id} must mirror accepted_slugs()"
            );
        }
    }

    /// Every fill-flag candidate that `astrogram::chart`'s `slug_description`
    /// tables know a description for must surface that exact text as the
    /// candidate's completion/`--help` help — derived from the source tables,
    /// not a hardcoded copy, so the two can never drift apart. A slug with no
    /// known description (the three Solar-Fire-specific house codes) is
    /// allowed to have no help, but every *other* slug must have one.
    #[test]
    fn fill_flags_surface_slug_descriptions_as_help() {
        use clap::CommandFactory;
        let cmd = Cli::command();
        let help_of = |id: &str| -> std::collections::HashMap<String, Option<String>> {
            cmd.get_arguments()
                .find(|a| a.get_id() == id)
                .expect("arg present")
                .get_possible_values()
                .iter()
                .map(|p| {
                    (
                        p.get_name().to_string(),
                        p.get_help().map(|h| h.to_string()),
                    )
                })
                .collect()
        };

        for (id, accepted, describe) in [
            (
                "fill_house",
                astrogram::chart::HouseSystem::accepted_slugs(),
                astrogram::chart::HouseSystem::slug_description as fn(&str) -> Option<&'static str>,
            ),
            (
                "fill_zodiac",
                astrogram::chart::Zodiac::accepted_slugs(),
                astrogram::chart::Zodiac::slug_description,
            ),
            (
                "fill_locus",
                astrogram::chart::CoordinateSystem::accepted_slugs(),
                astrogram::chart::CoordinateSystem::slug_description,
            ),
        ] {
            let rendered = help_of(id);
            let mut any_described = false;
            for slug in accepted {
                let expected = describe(slug);
                let actual = rendered.get(*slug).expect("slug present in candidates");
                if let Some(expected) = expected {
                    any_described = true;
                    assert_eq!(
                        actual.as_deref(),
                        Some(expected),
                        "{id} candidate {slug:?} help must mirror slug_description()"
                    );
                }
            }
            assert!(
                any_described,
                "{id} should have at least one slug with a description"
            );
        }
    }

    /// A supplied-but-invalid fill value is NOT rejected at clap-parse time —
    /// it parses through as a raw `String`, to be validated (and classified as
    /// `InputError`/exit 3) later in `resolve_fill`.
    #[test]
    fn fill_flag_accepts_any_string_at_parse_time() {
        let cli = Cli::parse_from(["blackmoon", "--fill-house", "xyzzy"]);
        assert_eq!(cli.fill_house.as_deref(), Some("xyzzy"));
    }

    #[test]
    fn fills_needed_adb_to_sfcht() {
        // sanity: the convert path will need fills here.
        let f = astrogram::capability::fill_fields(Target::Adb, Target::Sfcht);
        assert_eq!(f.len(), 3);
    }

    #[test]
    fn is_machine_output_true_only_for_dash_output() {
        assert!(is_machine_output(Target::Json, Some(Path::new("-"))));
        assert!(is_machine_output(Target::Sfcht, Some(Path::new("-"))));
        assert!(!is_machine_output(
            Target::Json,
            Some(Path::new("out.json"))
        ));
        assert!(!is_machine_output(Target::Json, None));
    }

    // Under `cargo test`, stdin is not a tty, so the non-tty (no-flag) branch
    // of `resolve_fill` fires — the same branch a piped-input or non-interactive
    // machine-output pipeline hits at runtime. This is why these tests exercise
    // the NeedInputError path without any explicit "machine" toggle.
    #[test]
    fn resolve_fill_no_flag_non_tty_returns_need_input_without_prompting() {
        use astrogram::pipeline::fill_spec;

        let spec = fill_spec(astrogram::capability::ChartField::HouseSystem)
            .expect("HouseSystem has a FillSpec");
        let err = match resolve_fill(spec, None, Target::Json) {
            Ok(_) => panic!("missing flag with non-tty stdin must error, not prompt"),
            Err(e) => e,
        };
        let msg = err.to_string();
        assert!(msg.contains("--fill-house"), "message: {msg}");
        assert!(msg.contains("placidus"), "message: {msg}");
        assert!(
            err.downcast_ref::<exit::NeedInputError>().is_some(),
            "expected a typed NeedInputError, got: {msg}"
        );
    }

    #[test]
    fn resolve_fill_still_honors_explicit_flag() {
        use astrogram::pipeline::fill_spec;

        let spec = fill_spec(astrogram::capability::ChartField::HouseSystem)
            .expect("HouseSystem has a FillSpec");
        let value = resolve_fill(spec, Some("whole-sign"), Target::Json).expect("flag supplied");
        assert!(matches!(
            value,
            astrogram::pipeline::FillValue::House(astrogram::chart::HouseSystem::WholeSign)
        ));
    }

    /// A supplied-but-invalid `--fill-*` value is a bad *input*: `resolve_fill`
    /// returns a typed [`exit::InputError`] (classifies to exit 3), listing the
    /// accepted values — not a clap usage error and not a plain internal error.
    #[test]
    fn resolve_fill_invalid_flag_value_returns_input_error() {
        use astrogram::pipeline::fill_spec;

        let spec = fill_spec(astrogram::capability::ChartField::HouseSystem)
            .expect("HouseSystem has a FillSpec");
        let err = match resolve_fill(spec, Some("xyzzy"), Target::Json) {
            Ok(_) => panic!("an unrecognised fill value must error"),
            Err(e) => e,
        };
        let input = err
            .downcast_ref::<exit::InputError>()
            .expect("expected a typed InputError");
        assert_eq!(input.value, "xyzzy");
        assert_eq!(input.flag, "--fill-house");
        let msg = err.to_string();
        assert!(msg.contains("xyzzy"), "message: {msg}");
        assert!(msg.contains("--fill-house"), "message: {msg}");
        assert!(msg.contains("placidus"), "message: {msg}");
        assert_eq!(exit::classify(&err), exit::ExitClass::Input);
    }

    /// Source-aware fill: charts whose source DID carry the field are NOT
    /// overwritten; only charts whose source lacked it receive the fill value.
    #[test]
    fn apply_fills_does_not_clobber_sfcht_source_charts() {
        use astrogram::capability::ChartField;
        use astrogram::chart::{
            Chart, CoordinateSystem, EventType, HouseSystem, Latitude, Longitude, Zodiac,
        };
        use clap::Parser;
        use std::collections::HashMap;

        // Build two charts: one from SFcht (carries real WholeSign), one from ADB (no system).
        let make_chart = |name: &str| Chart {
            name: name.into(),
            secondary_name: None,
            city: Some("c".into()),
            region: None,
            longitude: Longitude::new(0.0).unwrap(),
            latitude: Latitude::new(0.0).unwrap(),
            year: 2000,
            month: 1,
            day: 1,
            hour: 0,
            minute: 0,
            second: 0,
            tz_offset_hours: 0.0,
            tz_abbreviation: None,
            is_lmt: false,
            event_type: EventType::Unspecified,
            source_rating: None,
            house_system: HouseSystem::WholeSign,
            zodiac: Zodiac::Tropical,
            coordinate_system: CoordinateSystem::Geocentric,
            sub_charts: vec![],
            notes: None,
        };

        let sfcht_chart = make_chart("SFcht Chart");
        let adb_chart = make_chart("ADB Chart");
        let mut merged = vec![sfcht_chart, adb_chart];

        let mut source_of: HashMap<providers::DatetimeKey, Format> = HashMap::new();
        source_of.insert(astrogram::provider::key(&merged[0]), Target::Sfcht);
        source_of.insert(astrogram::provider::key(&merged[1]), Target::Adb);

        // fill_fields for ADB→SFcht: all three settings needed.
        let fills = vec![
            ChartField::HouseSystem,
            ChartField::Zodiac,
            ChartField::CoordinateSystem,
        ];

        // CLI with explicit fill flags: placidus/tropical/geocentric.
        let cli = Cli::parse_from([
            "blackmoon",
            "--fill-house",
            "placidus",
            "--fill-zodiac",
            "tropical",
            "--fill-locus",
            "geocentric",
        ]);

        apply_fills(&mut merged, &fills, &source_of, &cli, Target::Sfcht).unwrap();

        // SFcht chart's WholeSign must NOT be overwritten.
        assert_eq!(
            merged[0].house_system,
            HouseSystem::WholeSign,
            "SFcht source chart must keep its genuine house system"
        );
        // ADB chart gets the fill value.
        assert_eq!(
            merged[1].house_system,
            HouseSystem::Placidus,
            "ADB source chart must receive the filled house system"
        );
    }

    #[test]
    fn transcript_summary_counts_statuses() {
        use astrogram::transcript::{FieldMapping, FieldStatus};
        let m = vec![
            FieldMapping {
                label: "name",
                from: "a".into(),
                to: "a".into(),
                status: FieldStatus::Preserved,
                note: None,
            },
            FieldMapping {
                label: "house system",
                from: "alcabitius".into(),
                to: "placidus".into(),
                status: FieldStatus::Transformed,
                note: Some("global setting"),
            },
            FieldMapping {
                label: "notes",
                from: "x".into(),
                to: String::new(),
                status: FieldStatus::Dropped,
                note: None,
            },
        ];
        let s = transcript_summary(&m);
        assert_eq!(s, "1 preserved, 1 transformed, 1 dropped, 0 filled");
    }

    #[test]
    fn capabilities_flag_parses_text_and_json() {
        let bare = Cli::parse_from(["blackmoon", "--capabilities"]);
        assert!(matches!(bare.capabilities, Some(CapsFormat::Text)));
        let json = Cli::parse_from(["blackmoon", "--capabilities=json"]);
        assert!(matches!(json.capabilities, Some(CapsFormat::Json)));
        let off = Cli::parse_from(["blackmoon", "x.SFcht"]);
        assert!(off.capabilities.is_none());
    }

    #[test]
    fn render_capabilities_text_and_json_contain_sfcht() {
        let rows = astrogram::format::capability_matrix();
        let text = render_capabilities(&rows, CapsFormat::Text);
        assert!(text.contains("sfcht"));
        let json = render_capabilities(&rows, CapsFormat::Json);
        // Valid JSON that round-trips and mentions a known slug.
        let v: serde_json::Value = serde_json::from_str(&json).expect("valid json");
        assert!(v.to_string().contains("sfcht"));
    }

    // `without_output_file` now lives in `astrogram::convert` and is tested there.
}

#[cfg(test)]
mod format_arg_tests {
    use super::*;
    use clap::CommandFactory;

    /// The tab-completable possible values for a `--from`/`--to`/`--target`-style
    /// arg, as clap sees them.
    fn possible_values(arg_id: &str) -> Vec<String> {
        let cmd = Cli::command();
        let arg = cmd
            .get_arguments()
            .find(|a| a.get_id() == arg_id)
            .unwrap_or_else(|| panic!("no --{arg_id} arg on Cli"));
        arg.get_possible_values()
            .iter()
            .map(|pv| pv.get_name().to_string())
            .collect()
    }

    #[test]
    fn from_to_target_offer_every_format_slug_as_possible_value() {
        // The completion surface must equal the format registry — the single
        // source of truth — so newly registered formats appear automatically.
        let expected: Vec<String> = Format::all().iter().map(|s| s.slug.to_string()).collect();
        for arg_id in ["from", "to", "target"] {
            assert_eq!(
                possible_values(arg_id),
                expected,
                "--{arg_id} possible values must match FORMATS slugs"
            );
        }
    }

    #[test]
    fn every_slug_parses_to_its_format() {
        for spec in Format::all() {
            let cli = Cli::try_parse_from(["blackmoon", "--from", spec.slug])
                .unwrap_or_else(|e| panic!("--from {} should parse: {e}", spec.slug));
            assert_eq!(cli.from, Some(spec.format), "slug {} round-trip", spec.slug);
        }
    }

    #[test]
    fn unknown_format_is_rejected() {
        let result = Cli::try_parse_from(["blackmoon", "--from", "bogus"]);
        assert!(result.is_err(), "unknown --from value must be rejected");
    }

    /// Every `--from`/`--to`/`--target` possible value must carry non-empty
    /// per-value help (kind/direction/auth), so zsh/fish completion — and
    /// `--help` — explain each slug instead of just listing it.
    #[test]
    fn from_to_target_possible_values_carry_help() {
        let cmd = Cli::command();
        for arg_id in ["from", "to", "target"] {
            let arg = cmd
                .get_arguments()
                .find(|a| a.get_id() == arg_id)
                .unwrap_or_else(|| panic!("no --{arg_id} arg on Cli"));
            for pv in arg.get_possible_values() {
                assert!(
                    pv.get_help().is_some(),
                    "--{arg_id} value {:?} is missing help text",
                    pv.get_name()
                );
            }
        }
    }

    /// The help text is derived from each `FormatSpec`, not hand-authored, so
    /// it states kind + direction + auth for a known format.
    #[test]
    fn format_help_describes_kind_direction_and_auth() {
        let sfcht = Format::Sfcht.spec();
        let help = format_help(sfcht);
        assert_eq!(help, "file, read+write, no auth");

        let luna = Format::Luna.spec();
        let help = format_help(luna);
        assert_eq!(help, "web, read+write, token auth");

        let aaf = Format::Aaf.spec();
        let help = format_help(aaf);
        assert_eq!(help, "file, read-only, no auth");
    }
}

#[cfg(test)]
mod completion_candidate_tests {
    use super::*;
    use clap::CommandFactory;

    fn arg<'a>(cmd: &'a clap::Command, id: &str) -> &'a clap::Arg {
        cmd.get_arguments()
            .find(|a| a.get_id() == id)
            .unwrap_or_else(|| panic!("no --{id} arg on Cli"))
    }

    fn possible_values(cmd: &clap::Command, id: &str) -> Vec<String> {
        arg(cmd, id)
            .get_possible_values()
            .iter()
            .map(|pv| pv.get_name().to_string())
            .collect()
    }

    /// `--capabilities` must tab-complete `json` (and `text`), each with
    /// help — completion is additive; the bare flag / `=json` acceptance
    /// (asserted by `capabilities_flag_parses_text_and_json` above) is
    /// unchanged.
    #[test]
    fn capabilities_candidates_include_json_with_help() {
        let cmd = Cli::command();
        let values = possible_values(&cmd, "capabilities");
        assert!(
            values.contains(&"json".to_string()),
            "--capabilities candidates {values:?} must include 'json'"
        );
        assert!(
            values.contains(&"text".to_string()),
            "--capabilities candidates {values:?} must include 'text'"
        );
        for pv in arg(&cmd, "capabilities").get_possible_values() {
            assert!(
                pv.get_help().is_some(),
                "--capabilities value {:?} is missing help text",
                pv.get_name()
            );
        }
    }

    /// Bad `--capabilities` values must still be rejected at parse time
    /// (unlike `--fill-*`, this flag has no alias concerns, so it stays a
    /// strict, gate-on-parse arg — completion is additive on top of it).
    #[test]
    fn capabilities_unknown_value_is_rejected() {
        let result = Cli::try_parse_from(["blackmoon", "--capabilities=xml"]);
        assert!(
            result.is_err(),
            "unknown --capabilities value must be rejected"
        );
    }

    /// `--grant-cookie-access` candidates must equal `"all"` plus every
    /// `wristband::Browser` slug — sourced from `Browser::all()`, not a
    /// hardcoded duplicate list — each with help text.
    #[test]
    fn grant_cookie_access_candidates_equal_browser_list() {
        let cmd = Cli::command();
        let values = possible_values(&cmd, "grant_cookie_access");
        let mut expected: Vec<String> = vec!["all".to_string()];
        expected.extend(Browser::all().iter().map(|b| browser_slug(*b).to_string()));
        assert_eq!(
            values, expected,
            "--grant-cookie-access candidates must be 'all' + Browser::all() slugs"
        );
        for pv in arg(&cmd, "grant_cookie_access").get_possible_values() {
            assert!(
                pv.get_help().is_some(),
                "--grant-cookie-access value {:?} is missing help text",
                pv.get_name()
            );
        }
    }

    /// Parsing an unknown browser is still rejected (completion is additive;
    /// `parse_browser`'s acceptance is unchanged by the wrapper).
    #[test]
    fn grant_cookie_access_unknown_browser_still_rejected() {
        let result = Cli::try_parse_from(["blackmoon", "--grant-cookie-access=netscape"]);
        assert!(result.is_err(), "unknown browser must still be rejected");
    }

    /// Every browser slug still round-trips through the real CLI parser
    /// (completion wiring must not tighten or otherwise change acceptance).
    #[test]
    fn every_browser_slug_still_parses() {
        for b in Browser::all() {
            let slug = browser_slug(*b);
            let cli = Cli::try_parse_from(["blackmoon", "--grant-cookie-access", slug])
                .unwrap_or_else(|e| panic!("--grant-cookie-access {slug} should still parse: {e}"));
            assert_eq!(cli.grant_cookie_access, Some(GrantArg::One(*b)));
        }
    }

    /// `--generate-completion` candidates must include `auto` plus every
    /// `clap_complete::Shell` variant, sourced from `Shell::value_variants()`
    /// (clap_complete's own registry), each with help text.
    #[test]
    fn generate_completion_candidates_equal_auto_plus_shells() {
        use clap::ValueEnum;
        let cmd = Cli::command();
        let values = possible_values(&cmd, "generate_completion");
        let mut expected: Vec<String> = vec!["auto".to_string()];
        expected.extend(
            clap_complete::Shell::value_variants()
                .iter()
                .map(|s| s.to_possible_value().unwrap().get_name().to_string()),
        );
        assert_eq!(
            values, expected,
            "--generate-completion candidates must be 'auto' + Shell::value_variants()"
        );
        for pv in arg(&cmd, "generate_completion").get_possible_values() {
            assert!(
                pv.get_help().is_some(),
                "--generate-completion value {:?} is missing help text",
                pv.get_name()
            );
        }
    }

    /// Parsing is unaffected by the completion wrapper: `Cli::generate_completion`
    /// is still the raw string, and any value still parses through (the real
    /// validation — `"auto"` vs. `Shell::from_str`, else an error — happens in
    /// `run()`, not at clap-parse time).
    #[test]
    fn generate_completion_any_string_still_parses_through() {
        let cli = Cli::parse_from(["blackmoon", "--generate-completion", "zsh"]);
        assert_eq!(cli.generate_completion.as_deref(), Some("zsh"));

        let cli = Cli::parse_from(["blackmoon", "--generate-completion", "bogus-shell"]);
        assert_eq!(cli.generate_completion.as_deref(), Some("bogus-shell"));
    }

    /// Sanity: generating a real zsh completion script still succeeds and
    /// mentions the new candidate values (spot-check a couple), proving the
    /// possible-values wiring reaches `clap_complete::generate`.
    #[test]
    fn zsh_completion_script_mentions_new_candidates() {
        use clap::CommandFactory;
        let mut buf = Vec::new();
        clap_complete::generate(
            clap_complete::Shell::Zsh,
            &mut Cli::command(),
            "blackmoon",
            &mut buf,
        );
        let script = String::from_utf8(buf).expect("valid utf8");
        assert!(script.contains("json"), "zsh script should mention 'json'");
        assert!(
            script.contains("chrome"),
            "zsh script should mention 'chrome'"
        );
        assert!(script.contains("zsh"), "zsh script should mention 'zsh'");
    }
}
