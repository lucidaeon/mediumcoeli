// `import_blackmoon` and `import_charts_from` are large CLI dispatchers;
// splitting them produces worse code than the lint resolves.
#![allow(clippy::too_many_lines)]
// clap collects /// comments on Cli/args as user-facing --help text; adding
// rustdoc-style backticks here would surface as literal characters in output.
#![allow(clippy::doc_markdown)]

use anyhow::{Context, Result, bail};
use astrogram::astrocom::AstrocomSession;
use astrogram::astrotheoros::AstrotheorosSession;
use astrogram::format::{Format, Kind};
use astrogram::luna::LunaSession;
use astrogram::normalize::normalize_chart;
use astrogram::util::{expand_now, utc_timestamp};
use astrogram::{aaf, adbxml, consolidate, jzod, raw, sfcht, zeus};
use clap::Parser;
use std::collections::{HashMap, HashSet};
use std::io::Write as _;
use std::path::{Path, PathBuf};

pub use astrogram::format::Format as Target;

mod consolidate_ui;
mod providers;
use providers::WebProvider;

// ── format value parser ───────────────────────────────────────────────────────

fn parse_format(s: &str) -> Result<Target, String> {
    Format::from_slug(s).ok_or_else(|| {
        let slugs: Vec<&str> = Format::all().iter().map(|spec| spec.slug).collect();
        format!(
            "unknown format '{s}'; expected one of: {}",
            slugs.join(", ")
        )
    })
}

// ── CLI ───────────────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(
    name = "blackmoon",
    about = "Astrology data converter — reads any target, writes any target.",
    long_about = "\
Reads one or more source targets (files or web endpoints), merges and deduplicates,
then writes to an output target.  Target type is detected from the file
extension (.SFcht, .zdb, .xml) or specified with --from / --to.

Each write is preceded by a read of the output target (if it already exists)
so no duplicate records are ever added.

Examples:
  blackmoon input.zdb --output out.SFcht
  blackmoon a.SFcht b.zdb export.xml --output merged.SFcht
  blackmoon --from luna --luna-token $LUNA_TOKEN --output charts.SFcht
  blackmoon --from astrotheoros --astrotheoros-user $USER --astrotheoros-pass $PASS --output charts.SFcht
  blackmoon charts.SFcht --normalize
  blackmoon *.SFcht --normalize"
)]
struct Cli {
    /// Input files (.SFcht, .zdb, .xml).  Omit when --from a web endpoint.
    inputs: Vec<PathBuf>,

    /// Output file.  Target detected from extension; overridden by --to.
    /// Use `now.{ext}` to substitute the current UTC timestamp automatically
    /// (e.g. `--output now.SFcht`).  When --from a web endpoint and --output is omitted,
    /// defaults to `{timestamp}.SFcht`.
    #[arg(short, long, alias = "out")]
    output: Option<PathBuf>,

    /// Source target — required when the source is not a file (web endpoint).
    #[arg(long, value_parser = parse_format)]
    from: Option<Target>,

    /// Output target — overrides the extension of --output (or use for a web endpoint).
    #[arg(long, value_parser = parse_format)]
    to: Option<Target>,

    /// Alias for --from / --to.  Used when both sides share the same target
    /// (e.g. `--target luna --normalize`) or as a shorthand for either
    /// direction when the other side is inferred from a file extension.
    #[arg(long, value_parser = parse_format)]
    target: Option<Target>,

    /// Map non-cp1252 characters to ASCII equivalents in all text fields.
    /// Without --output, edits each input file in-place.
    #[arg(long)]
    normalize: bool,

    /// LUNA® auth token (session cookie).  Required when --from luna or --to luna.
    #[arg(long, env = "LUNA_TOKEN", hide_env_values = true)]
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
    #[arg(long, env = "ASTROCOM_TOKEN", hide_env_values = true)]
    astrocom_token: Option<String>,

    /// astro.com account email.  When combined with --astrocom-pass, blackmoon logs
    /// in automatically and derives a fresh cid (no manual cookie needed).
    #[arg(long, env = "ASTROCOM_USER", hide_env_values = true)]
    astrocom_user: Option<String>,

    /// astro.com account password.  Use with --astrocom-user.
    #[arg(long, env = "ASTROCOM_PASS", hide_env_values = true)]
    astrocom_pass: Option<String>,

    /// astrotheoros.com account email.  When combined with --astrotheoros-pass,
    /// blackmoon logs in automatically.
    #[arg(long, env = "ASTROTHEOROS_USER", hide_env_values = true)]
    astrotheoros_user: Option<String>,

    /// astrotheoros.com account password.  Use with --astrotheoros-user.
    #[arg(long, env = "ASTROTHEOROS_PASS", hide_env_values = true)]
    astrotheoros_pass: Option<String>,

    /// Auth token as "jwt:session_id:client_uat" (colon-delimited). Prefer user/pass.
    #[arg(long, env = "ASTROTHEOROS_TOKEN", hide_env_values = true)]
    astrotheoros_token: Option<String>,

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
    #[arg(long)]
    strict: bool,

    /// Value to fill house_system with when writing to a format that requires it
    /// but the source did not provide one (e.g. placidus, koch, whole-sign).
    #[arg(long)]
    fill_house: Option<String>,
    /// Value to fill zodiac with in the same situation (e.g. tropical, lahiri).
    #[arg(long)]
    fill_zodiac: Option<String>,
    /// Value to fill the locus (coordinate system) with: geocentric | heliocentric.
    #[arg(long)]
    fill_locus: Option<String>,

    /// Print per-record detail (duplicate names, per-chart fetch status).
    #[arg(long, short)]
    verbose: bool,

    /// Print a shell completion script to stdout.
    #[arg(long = "generate-completion", value_name = "SHELL", num_args = 0..=1, default_missing_value = "auto", hide = true)]
    generate_completion: Option<String>,
}

// ── main ──────────────────────────────────────────────────────────────────────

fn main() -> Result<()> {
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

    let nothing = cli.inputs.is_empty()
        && cli.output.is_none()
        && cli.from.is_none()
        && cli.to.is_none()
        && cli.target.is_none()
        && !cli.normalize
        && !cli.consolidate;

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

// ── fill helpers ─────────────────────────────────────────────────────────────

fn parse_house(s: &str) -> Result<astrogram::chart::HouseSystem> {
    use astrogram::chart::HouseSystem::*;
    Ok(match s.to_ascii_lowercase().replace('_', "-").as_str() {
        "placidus" => Placidus,
        "koch" => Koch,
        "campanus" => Campanus,
        "regiomontanus" => Regiomontanus,
        "porphyry" => Porphyry,
        "equal" => Equal,
        "whole-sign" | "whole" => WholeSign,
        "alcabitius" => Alcabitius,
        "topocentric" => Topocentric,
        "meridian" => Meridian,
        "morinus" => Morinus,
        "zero-aries" | "zeroaries" => ZeroAries,
        "solar-sign" | "solarsign" => SolarSign,
        "hindu-bhava" | "hindubhava" => HinduBhava,
        other => bail!("unknown house system '{other}'"),
    })
}

fn parse_zodiac(s: &str) -> Result<astrogram::chart::Zodiac> {
    use astrogram::chart::Zodiac::*;
    Ok(match s.to_ascii_lowercase().as_str() {
        "tropical" => Tropical,
        "fagan-allen" | "faganallen" => FaganAllen,
        "lahiri" => Lahiri,
        "raman" => Raman,
        "krishnamurti" => Krishnamurti,
        "draconic" => Draconic,
        other => bail!("unknown zodiac '{other}'"),
    })
}

fn parse_locus(s: &str) -> Result<astrogram::chart::CoordinateSystem> {
    use astrogram::chart::CoordinateSystem::*;
    Ok(match s.to_ascii_lowercase().as_str() {
        "geocentric" | "geo" => Geocentric,
        "heliocentric" | "helio" => Heliocentric,
        other => bail!("unknown locus '{other}' (expected geocentric|heliocentric)"),
    })
}

/// Flag → TTY prompt (with suggested value) → error.
/// `flag_suffix` is the exact flag name suffix (e.g. "house" for `--fill-house`).
fn resolve_fill<T>(
    label: &str,
    flag_suffix: &str,
    flag: Option<&str>,
    suggested: &str,
    parse: impl Fn(&str) -> Result<T>,
    sink: Format,
) -> Result<T> {
    use std::io::IsTerminal;
    if let Some(s) = flag {
        return parse(s);
    }
    if std::io::stdin().is_terminal() {
        eprintln!(
            "{} stores {label} per chart; your source did not provide one.",
            sink.spec().slug
        );
        eprint!("Value for {label} [{suggested}]: ");
        std::io::stderr().flush().ok();
        let mut line = String::new();
        std::io::stdin().read_line(&mut line)?;
        let chosen = line.trim();
        let chosen = if chosen.is_empty() { suggested } else { chosen };
        return parse(chosen);
    }
    bail!(
        "{} requires a {label} but the source provided none; pass --fill-{flag_suffix} (non-interactive)",
        sink.spec().slug,
    )
}

/// Resolve and apply fill values for the fields the sink demands that each
/// chart's source lacked. Values are resolved once per field (flag → TTY
/// prompt → error) and applied only to charts whose source did NOT carry the
/// field, to avoid overwriting genuine values from SFcht sources.
fn apply_fills(
    merged: &mut [astrogram::chart::Chart],
    fills: &[astrogram::capability::ChartField],
    source_of: &std::collections::HashMap<providers::DatetimeKey, Format>,
    cli: &Cli,
    sink: Format,
) -> Result<()> {
    use astrogram::capability::ChartField;

    enum Fill {
        House(astrogram::chart::HouseSystem),
        Zodiac(astrogram::chart::Zodiac),
        Locus(astrogram::chart::CoordinateSystem),
    }

    for &field in fills {
        let resolved = match field {
            ChartField::HouseSystem => Fill::House(resolve_fill(
                "house system",
                "house",
                cli.fill_house.as_deref(),
                "placidus",
                parse_house,
                sink,
            )?),
            ChartField::Zodiac => Fill::Zodiac(resolve_fill(
                "zodiac",
                "zodiac",
                cli.fill_zodiac.as_deref(),
                "tropical",
                parse_zodiac,
                sink,
            )?),
            ChartField::CoordinateSystem => Fill::Locus(resolve_fill(
                "locus",
                "locus",
                cli.fill_locus.as_deref(),
                "geocentric",
                parse_locus,
                sink,
            )?),
            _ => continue, // only NON_OMITTABLE fields ever appear in `fills`
        };
        for c in merged.iter_mut() {
            let src = source_of.get(&providers::key(c)).copied().unwrap_or(sink);
            if src.read_caps().preserves(field) {
                continue; // this chart's source carried a real value — keep it
            }
            match resolved {
                Fill::House(v) => c.house_system = v,
                Fill::Zodiac(v) => c.zodiac = v,
                Fill::Locus(v) => c.coordinate_system = v,
            }
        }
    }
    Ok(())
}

// ── convert / merge ───────────────────────────────────────────────────────────

fn is_web_target(t: Target) -> bool {
    matches!(t.spec().kind, Kind::Web)
}

fn resolve_provider(target: Target, cli: &Cli) -> Result<WebProvider> {
    match target {
        Target::Luna => {
            let cookie = cli
                .luna_token
                .as_deref()
                .context("--luna-token (or LUNA_TOKEN) is required when --from/--to luna")?;
            let session = LunaSession::new(cookie, cli.delay).context("building LUNA session")?;
            Ok(WebProvider::Luna {
                session,
                resume_from: cli.luna_resume_from.clone(),
                normalize: cli.normalize,
                listing_keys: std::collections::HashSet::new(),
                phenom_ids: Vec::new(),
            })
        }
        Target::Astrocom => match (&cli.astrocom_user, &cli.astrocom_pass) {
            (Some(user), Some(pass)) => {
                println!("astro.com: logging in as {user}…");
                let session = AstrocomSession::login(user, pass, cli.delay)
                    .context("astro.com login failed")?;
                Ok(WebProvider::Astrocom {
                    session,
                    creds: Some((user.clone(), pass.clone())),
                    nhor_id_map: std::collections::HashMap::new(),
                })
            }
            (Some(_), None) => {
                bail!("--astrocom-pass (or ASTROCOM_PASS) required with --astrocom-user")
            }
            (None, Some(_)) => {
                bail!("--astrocom-user (or ASTROCOM_USER) required with --astrocom-pass")
            }
            (None, None) => {
                let cid = cli.astrocom_token.as_deref().context(
                    "--astrocom-token (or ASTROCOM_TOKEN) is required when --from/--to astrocom",
                )?;
                let session = AstrocomSession::from_cid(cid, cli.delay)
                    .context("building astro.com session")?;
                Ok(WebProvider::Astrocom {
                    session,
                    creds: None,
                    nhor_id_map: std::collections::HashMap::new(),
                })
            }
        },
        Target::Astrotheoros => match (&cli.astrotheoros_user, &cli.astrotheoros_pass) {
            (Some(user), Some(pass)) => {
                println!("astrotheoros.com: logging in as {user}…");
                let session = AstrotheorosSession::login(user, pass, cli.delay)
                    .context("astrotheoros.com login failed")?;
                Ok(WebProvider::Astrotheoros {
                    session,
                    uuid_map: std::collections::HashMap::new(),
                })
            }
            (Some(_), None) => {
                bail!(
                    "--astrotheoros-pass (or ASTROTHEOROS_PASS) required with --astrotheoros-user"
                )
            }
            (None, Some(_)) => {
                bail!(
                    "--astrotheoros-user (or ASTROTHEOROS_USER) required with --astrotheoros-pass"
                )
            }
            (None, None) => {
                let token = cli.astrotheoros_token.as_deref().context(
                    "--astrotheoros-user/--astrotheoros-pass (or ASTROTHEOROS_USER/PASS) \
                     required when --from/--to astrotheoros",
                )?;
                let parts: Vec<&str> = token.splitn(3, ':').collect();
                if parts.len() != 3 {
                    bail!("--astrotheoros-token must be 'jwt:session_id:client_uat'");
                }
                let session =
                    AstrotheorosSession::from_jwt(parts[0], parts[1], parts[2], cli.delay)
                        .context("building astrotheoros.com session")?;
                Ok(WebProvider::Astrotheoros {
                    session,
                    uuid_map: std::collections::HashMap::new(),
                })
            }
        },
        other => unreachable!("resolve_provider called for non-web target {other:?}"),
    }
}

fn cmd_convert(cli: &Cli) -> Result<()> {
    let from = cli.from.or(cli.target);
    let to = cli.to.or(cli.target);

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
                p.display()
            )
        })?,
        (None, None) => bail!("--output (or --to luna / --to astrocom) is required"),
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
    // the output stream contains only the serialized chart data.
    let to_stdout = out_path.is_some_and(|p| p == Path::new("-"));

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
            existing = p.read_existing()?;
        }
    } else if let Some(p) = out_path {
        if p.exists() {
            existing = read_file_target(p, out_target)
                .with_context(|| format!("reading existing output {}", p.display()))?;
            if !to_stdout {
                println!("{}: {} charts (existing)", p.display(), existing.len());
            }
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
    for chart in &existing {
        source_of.entry(providers::key(chart)).or_insert(out_target);
    }
    let mut batches: Vec<Vec<astrogram::chart::Chart>> = vec![existing];

    if let Some(p) = &mut in_provider {
        let charts = p.read_input()?;
        for chart in &charts {
            source_of
                .entry(providers::key(chart))
                .or_insert(from.expect("in_provider is Some only when --from/--target is set"));
        }
        batches.push(charts);
    } else if from.map(is_web_target).unwrap_or(false) && from == Some(out_target) {
        // normalize-in-place for web: source == sink, use out_provider for reading.
        // (is_web_target guard ensures out_provider is Some — file-to-file same-target
        // is a degenerate case that falls through to the file-inputs else branch.)
        let charts = out_provider.as_mut().unwrap().read_input()?;
        for chart in &charts {
            source_of.entry(providers::key(chart)).or_insert(out_target);
        }
        batches.push(charts);
    } else {
        if cli.inputs.is_empty() {
            bail!(
                "at least one input file is required (or use --from / --target luna / --target astro)"
            );
        }
        for path in &cli.inputs {
            let target = Format::from_path(path).with_context(|| {
                format!(
                    "cannot detect target from '{}'; rename the file or use --from to specify",
                    path.display()
                )
            })?;
            let charts = read_file_target(path, target)
                .with_context(|| format!("reading {}", path.display()))?;
            if !to_stdout {
                println!("{}: {} charts", path.display(), charts.len());
            }
            for chart in &charts {
                source_of.entry(providers::key(chart)).or_insert(target);
            }
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
        bail!(
            "--strict: {dropped} chart(s) would lose data writing to {}; aborting",
            out_target.spec().slug
        );
    }

    // 5b. Apply fills: resolve per-chart values the sink demands but each
    // chart's source never carried (e.g. house_system/zodiac/locus for ADB→SFcht).
    // Union fill_fields over all distinct source formats so mixed batches work.
    {
        let mut needed: Vec<astrogram::capability::ChartField> = Vec::new();
        let sources: std::collections::HashSet<Format> = source_of.values().copied().collect();
        for src in &sources {
            for f in astrogram::capability::fill_fields(*src, out_target) {
                if !needed.contains(&f) {
                    needed.push(f);
                }
            }
        }
        if !needed.is_empty() {
            apply_fills(&mut merged, &needed, &source_of, cli, out_target)?;
        }
    }

    // 6. Write.
    if let Some(p) = &mut out_provider {
        if cli.normalize {
            println!("Charts to write ({}):", merged.len());
            for chart in &merged {
                println!("  {}", chart.name);
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
        p.write_charts(&merged)?;
        if !cli.no_verify {
            if let Err(e) = verify_and_report(p, &merged) {
                eprintln!("write succeeded; readback verification failed: {e}");
            }
        }
    } else {
        let p = out_path.expect("out_path set for file target");
        if cli.verbose && !to_stdout {
            for name in &skipped {
                println!("  dup: {name}");
            }
        }
        write_file_target(p, out_target, &merged)?;
        if !to_stdout {
            if existing_count > 0 {
                println!("  existing: {existing_count:>6}");
            }
            println!("  in:       {new_input_count:>6}");
            println!("  dupes:    {dupes:>6}");
            println!("  out:      {:>6}", merged.len());
            println!("wrote {}", p.display());
        }
    }
    Ok(())
}

// ── normalize in-place ────────────────────────────────────────────────────────

fn cmd_normalize_inplace(inputs: &[PathBuf]) -> Result<()> {
    if inputs.is_empty() {
        bail!("at least one input file is required for --normalize");
    }
    for path in inputs {
        let target = Format::from_path(path)
            .with_context(|| format!("cannot detect target from '{}'", path.display()))?;
        let mut charts = read_file_target(path, target)
            .with_context(|| format!("reading {}", path.display()))?;
        for chart in &mut charts {
            normalize_chart(chart);
        }
        write_file_target(path, target, &charts)
            .with_context(|| format!("writing {}", path.display()))?;
        println!("normalised {} charts in {}", charts.len(), path.display());
    }
    Ok(())
}

// ── consolidate ───────────────────────────────────────────────────────────────

fn cmd_consolidate(provider: WebProvider, cli: &Cli) -> Result<()> {
    use astrogram::consolidate::group_candidates;
    use astrogram::decision_log::{Choice, DecisionLog};

    let log_path = cli
        .decision_log
        .clone()
        .unwrap_or_else(default_decision_log_path);

    println!("Decision log: {}", log_path.display());

    let (charts, ids) = provider
        .fetch_all_with_ids()
        .with_context(|| format!("fetching charts from {}", provider.site_display()))?;
    println!("Fetched {} charts.", charts.len());

    let all_groups = group_candidates(&charts);
    let groups: Vec<Vec<usize>> = all_groups.into_iter().filter(|g| g.len() > 1).collect();
    if groups.is_empty() {
        println!("No candidate groups found.  Nothing to consolidate.");
        return Ok(());
    }
    println!("Found {} candidate group(s).", groups.len());

    let prior = DecisionLog::read_all(&log_path).context("reading decision log")?;
    let already_decided: std::collections::HashSet<String> =
        prior.iter().map(|r| r.group_id.clone()).collect();
    if !already_decided.is_empty() {
        println!(
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
        println!("Quit before completion.  Decisions so far are in the log.");
    }

    let all = DecisionLog::read_all(&log_path).context("re-reading decision log")?;
    let drops: Vec<String> = all
        .iter()
        .filter(|r| matches!(r.choice, Choice::Drop) && !r.phenom_id.is_empty())
        .map(|r| r.phenom_id.clone())
        .collect();
    if drops.is_empty() {
        println!("No drops to apply.");
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
        print!("[{:>3}/{}] {id}  ", i + 1, total);
        let _ = std::io::stdout().flush();
        match provider.delete_one(id) {
            Ok(()) => println!("deleted"),
            Err(e) => {
                println!("[!] {e}");
                failed += 1;
            }
        }
    }
    println!("Deleted {}/{total} chart(s).", total - failed);
    Ok(())
}

fn default_decision_log_path() -> PathBuf {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("blackmoon").join("luna-decisions.jsonl")
}

// ── target I/O ────────────────────────────────────────────────────────────────

fn read_file_target(path: &Path, target: Target) -> Result<Vec<astrogram::chart::Chart>> {
    match target {
        Target::Sfcht => {
            let bytes = std::fs::read(path)?;
            let (_, charts) = sfcht::parse_file(&bytes)?;
            Ok(charts)
        }
        Target::Zeus => {
            let text = std::fs::read_to_string(path)?;
            Ok(zeus::parse_file(&text)?)
        }
        Target::Adb => {
            let text = std::fs::read_to_string(path)?;
            Ok(adbxml::parse_file(&text)?)
        }
        Target::Aaf => {
            let text = std::fs::read_to_string(path)?;
            Ok(aaf::parse_file(&text)?)
        }
        Target::Luna => bail!("use --from luna rather than passing a file path"),
        Target::Astrocom => bail!("use --from astrocom rather than passing a file path"),
        Target::Astrotheoros => bail!("use --from astrotheoros rather than passing a file path"),
        Target::Json => bail!("JZOD (json) is a write-only format; reading is not supported"),
        Target::Raw => bail!("raw is a write-only format; reading is not supported"),
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
    match target {
        Target::Sfcht => {
            // stdout mode: no existing file to read description from (None is fine).
            let existing_desc = if path != Path::new("-") {
                std::fs::read(path)
                    .ok()
                    .and_then(|b| sfcht::parse_file(&b).ok())
                    .map(|(hdr, _)| hdr.description)
            } else {
                None
            };
            let bytes = sfcht::write_file_with_description(charts, existing_desc.as_deref())?;
            write_bytes_to(path, &bytes)?;
        }
        Target::Zeus => {
            let text = zeus::write_file(charts);
            write_bytes_to(path, text.as_bytes())?;
        }
        Target::Adb => {
            let text = adbxml::write_file(charts);
            write_bytes_to(path, text.as_bytes())?;
        }
        Target::Json => {
            let text = jzod::write_file(charts);
            write_bytes_to(path, text.as_bytes())?;
        }
        Target::Raw => {
            let text = raw::write_file(charts);
            write_bytes_to(path, text.as_bytes())?;
        }
        Target::Aaf => bail!("AAF is a read-only format; choose a writable --to/--output"),
        Target::Luna => bail!("use --to luna for writing to LUNA"),
        Target::Astrocom => bail!("use --to astrocom for writing to astro.com"),
        Target::Astrotheoros => bail!("use --to astrotheoros for writing to astrotheoros.com"),
    }
    Ok(())
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

/// Temporal identity of a chart: birth datetime only (NOT name — name is a
/// field we report changes to, so it must not gate the source↔landed pairing).
fn temporal_key(c: &astrogram::chart::Chart) -> (i16, u8, u8, u8, u8, u8) {
    (c.year, c.month, c.day, c.hour, c.minute, c.second)
}

/// True if two or more charts share an exact birth datetime (temporal key),
/// making readback pairing among that group best-effort (input order).
fn has_tied_datetimes(charts: &[astrogram::chart::Chart]) -> bool {
    let mut seen = HashSet::new();
    charts.iter().any(|c| !seen.insert(temporal_key(c)))
}

/// Pair each source chart to the landed chart sharing its temporal key.
///
/// Returns, per source (in order), the index into `landed` it matched, or
/// `None` if unmatched (creation failed / skipped as a pre-existing duplicate).
/// Each landed chart is consumed once; tied datetimes pair in input order.
fn pair_landed(
    sources: &[astrogram::chart::Chart],
    landed: &[astrogram::chart::Chart],
) -> Vec<Option<usize>> {
    let mut used = vec![false; landed.len()];
    sources
        .iter()
        .map(|s| {
            let key = temporal_key(s);
            let found = landed
                .iter()
                .enumerate()
                .find(|(i, l)| !used[*i] && temporal_key(l) == key)
                .map(|(i, _)| i);
            if let Some(i) = found {
                used[i] = true;
            }
            found
        })
        .collect()
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

/// Print one chart's full field-by-field `source → landed` transcript.
fn print_transcript(name: &str, m: &[astrogram::transcript::FieldMapping]) {
    use astrogram::transcript::FieldStatus::{Dropped, Filled, Preserved};
    println!("{name}");
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
        println!("  {:<18}{:<22}{glyph} {to}{note}", fm.label, from);
    }
    println!("  → {}", transcript_summary(m));
}

/// Read written charts back from a web sink and print per-chart transcripts.
fn verify_and_report(provider: &WebProvider, written: &[astrogram::chart::Chart]) -> Result<()> {
    if has_tied_datetimes(written) {
        eprintln!(
            "note: some charts share a birth datetime; readback pairing for those is best-effort (input order)"
        );
    }
    let global = provider.fetch_global_settings()?;
    let (landed_all, _ids) = provider.fetch_all_with_ids()?;
    let pairing = pair_landed(written, &landed_all);
    let mut verified = 0;
    for (src, maybe_idx) in written.iter().zip(pairing) {
        match maybe_idx {
            None => println!("{}\n  not found on readback — skipped", src.name),
            Some(i) => {
                let mut landed = landed_all[i].clone();
                let notes: &[(astrogram::capability::ChartField, &'static str)] =
                    if let Some(g) = &global {
                        landed.house_system = g.house_system;
                        landed.zodiac = g.zodiac;
                        landed.coordinate_system = g.coordinate_system;
                        &g.field_notes
                    } else {
                        &[]
                    };
                let mappings = astrogram::transcript::diff(src, &landed, notes);
                print_transcript(&src.name, &mappings);
                verified += 1;
            }
        }
    }
    println!(
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
    use astrogram::capability::lost_fields;
    let mut affected: Vec<(&str, Vec<&'static str>)> = Vec::new();
    for chart in merged {
        let source = source_of
            .get(&providers::key(chart))
            .copied()
            .unwrap_or(sink);
        let lost = lost_fields(chart, source, sink);
        if !lost.is_empty() {
            affected.push((
                chart.name.as_str(),
                lost.iter().map(|f| f.label()).collect(),
            ));
        }
    }
    if !affected.is_empty() && !to_stdout {
        let sink_name = sink.spec().slug;
        let all_lost: std::collections::BTreeSet<&str> = affected
            .iter()
            .flat_map(|(_, fs)| fs.iter().copied())
            .collect();
        let lost_list = all_lost.into_iter().collect::<Vec<_>>().join(", ");
        println!("{sink_name} does not store: {lost_list}.");
        println!("  {} chart(s) carry data in those fields:", affected.len());
        for (name, fs) in &affected {
            println!("    - {name:<24} {}", fs.join(", "));
        }
    }
    affected.len()
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
            "LUNA_TOKEN",
            "ASTROCOM_TOKEN",
            "ASTROCOM_USER",
            "ASTROCOM_PASS",
            "ASTROTHEOROS_TOKEN",
            "ASTROTHEOROS_USER",
            "ASTROTHEOROS_PASS",
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
    fn resolve_provider_luna_token_succeeds() {
        let _guard = clear_cred_env();
        let cli = Cli::parse_from(["blackmoon", "--luna-token", "abc123"]);
        // LunaSession::new only builds a reqwest client — no network call.
        assert!(resolve_provider(Target::Luna, &cli).is_ok());
    }

    #[test]
    fn resolve_provider_astrocom_token_only() {
        let _guard = clear_cred_env();
        let cli = Cli::parse_from(["blackmoon", "--astrocom-token", "test_cid"]);
        let provider = resolve_provider(Target::Astrocom, &cli).unwrap();
        // creds must be None (token path, not login path).
        assert!(matches!(
            provider,
            WebProvider::Astrocom { creds: None, .. }
        ));
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
    fn resolve_provider_astrotheoros_token_only() {
        let _guard = clear_cred_env();
        let cli = Cli::parse_from(["blackmoon", "--astrotheoros-token", "jwt:sess:uat"]);
        // AstrotheorosSession::from_jwt only constructs a struct — no network call.
        assert!(resolve_provider(Target::Astrotheoros, &cli).is_ok());
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
        source_of.insert(providers::key(&c), Target::Sfcht);
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
    /// registry slug, per the format's auth.
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
                format!("{upper}_USER"),
                format!("{upper}_PASS"),
                format!("{upper}_TOKEN"),
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
mod convert_tests {
    use super::*;

    #[test]
    fn pairing_matches_by_datetime_ignoring_name() {
        use astrogram::chart::{
            Chart, CoordinateSystem, EventType, HouseSystem, Latitude, Longitude, Zodiac,
        };
        let mk = |name: &str, day: u8| Chart {
            name: name.into(),
            secondary_name: None,
            city: Some("c".into()),
            region: None,
            longitude: Longitude::new(0.0).unwrap(),
            latitude: Latitude::new(0.0).unwrap(),
            year: 2000,
            month: 1,
            day,
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
            coordinate_system: CoordinateSystem::Geocentric,
            sub_charts: vec![],
            notes: None,
        };
        let sources = vec![mk("Source A", 1), mk("Source B", 2)];
        // Landed: renamed + reordered.
        let landed = vec![mk("Renamed B", 2), mk("Renamed A", 1)];
        let pairing = pair_landed(&sources, &landed);
        assert_eq!(pairing, vec![Some(1), Some(0)]);
    }

    #[test]
    fn pairing_reports_unmatched_as_none() {
        use astrogram::chart::{
            Chart, CoordinateSystem, EventType, HouseSystem, Latitude, Longitude, Zodiac,
        };
        let mk = |day: u8| Chart {
            name: "x".into(),
            secondary_name: None,
            city: Some("c".into()),
            region: None,
            longitude: Longitude::new(0.0).unwrap(),
            latitude: Latitude::new(0.0).unwrap(),
            year: 2000,
            month: 1,
            day,
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
            coordinate_system: CoordinateSystem::Geocentric,
            sub_charts: vec![],
            notes: None,
        };
        let sources = vec![mk(1), mk(2)];
        let landed = vec![mk(1)]; // chart 2 failed to create
        assert_eq!(pair_landed(&sources, &landed), vec![Some(0), None]);
    }

    #[test]
    fn pairing_tied_datetimes_consume_in_order() {
        use astrogram::chart::{
            Chart, CoordinateSystem, EventType, HouseSystem, Latitude, Longitude, Zodiac,
        };
        let mk = || Chart {
            name: "x".into(),
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
            coordinate_system: CoordinateSystem::Geocentric,
            sub_charts: vec![],
            notes: None,
        };
        let sources = vec![mk(), mk()];
        let landed = vec![mk(), mk()];
        assert_eq!(pair_landed(&sources, &landed), vec![Some(0), Some(1)]);
    }

    #[test]
    fn resolve_fill_house_parses_flag() {
        use astrogram::chart::HouseSystem;
        assert_eq!(parse_house("placidus").unwrap(), HouseSystem::Placidus);
        assert_eq!(parse_house("whole-sign").unwrap(), HouseSystem::WholeSign);
        assert!(parse_house("nonsense").is_err());
    }

    #[test]
    fn fills_needed_adb_to_sfcht() {
        // sanity: the convert path will need fills here.
        let f = astrogram::capability::fill_fields(Target::Adb, Target::Sfcht);
        assert_eq!(f.len(), 3);
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
        source_of.insert(providers::key(&merged[0]), Target::Sfcht);
        source_of.insert(providers::key(&merged[1]), Target::Adb);

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
    fn has_tied_datetimes_detects_shared_birth_moment() {
        use astrogram::chart::{
            Chart, CoordinateSystem, EventType, HouseSystem, Latitude, Longitude, Zodiac,
        };
        let mk = |name: &str, day: u8| Chart {
            name: name.into(),
            secondary_name: None,
            city: Some("c".into()),
            region: None,
            longitude: Longitude::new(0.0).unwrap(),
            latitude: Latitude::new(0.0).unwrap(),
            year: 2000,
            month: 1,
            day,
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
            coordinate_system: CoordinateSystem::Geocentric,
            sub_charts: vec![],
            notes: None,
        };
        assert!(!has_tied_datetimes(&[mk("A", 1), mk("B", 2)]));
        assert!(has_tied_datetimes(&[mk("A", 1), mk("B", 1)]));
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
}
