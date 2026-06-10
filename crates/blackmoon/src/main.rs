// `import_blackmoon` and `import_charts_from` are large CLI dispatchers;
// splitting them produces worse code than the lint resolves.
#![allow(clippy::too_many_lines)]
// clap collects /// comments on Cli/args as user-facing --help text; adding
// rustdoc-style backticks here would surface as literal characters in output.
#![allow(clippy::doc_markdown)]

use anyhow::{Context, Result, bail};
use astrogram::astro::AstroSession;
use astrogram::luna::LunaSession;
use astrogram::normalize::normalize_chart;
use astrogram::util::{expand_now, utc_timestamp};
use astrogram::{adbxml, consolidate, sfcht, zeus};
use clap::{Parser, ValueEnum};
use std::collections::HashMap;
use std::io::Write as _;
use std::path::{Path, PathBuf};

mod consolidate_ui;

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
  blackmoon --from luna --session $COOKIE --output charts.SFcht
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
    #[arg(long, value_enum)]
    from: Option<Target>,

    /// Output target — overrides the extension of --output (or use for a web endpoint).
    #[arg(long, value_enum)]
    to: Option<Target>,

    /// Alias for --from / --to.  Used when both sides share the same target
    /// (e.g. `--target luna --normalize`) or as a shorthand for either
    /// direction when the other side is inferred from a file extension.
    #[arg(long, value_enum)]
    target: Option<Target>,

    /// Map non-cp1252 characters to ASCII equivalents in all text fields.
    /// Without --output, edits each input file in-place.
    #[arg(long)]
    normalize: bool,

    /// LUNA® session cookie (LUNA_ASTROLOGY_APP).  Required when --from luna
    /// or --to luna.
    #[arg(long, env = "LUNA_ASTROLOGY_APP", hide_env_values = true)]
    luna_session: Option<String>,

    /// Delay between web endpoint HTTP requests in milliseconds.
    #[arg(long, default_value = "500")]
    delay: u64,

    /// Skip LUNA® charts until the first whose name starts with this prefix
    /// (case-insensitive).  Useful for resuming an interrupted fetch.
    #[arg(long)]
    luna_resume_from: Option<String>,

    /// astro.com session cookie (cid).  Required when --from astro or --to astro,
    /// unless --astro-user / --astro-pass are provided (login takes priority).
    #[arg(long, env = "ASTRO_COM_CID", hide_env_values = true)]
    astro_session: Option<String>,

    /// astro.com account email.  When combined with --astro-pass, blackmoon logs
    /// in automatically and derives a fresh cid (no manual cookie needed).
    #[arg(long, env = "ASTRO_COM_USER", hide_env_values = true)]
    astro_user: Option<String>,

    /// astro.com account password.  Use with --astro-user.
    #[arg(long, env = "ASTRO_COM_PASS", hide_env_values = true)]
    astro_pass: Option<String>,

    /// Delete charts from astro.com by nhor ID.  Accepts one or more comma-separated
    /// IDs (e.g. `--astro-delete 32,33,42`).  Requires --astro-user/--astro-pass.
    #[arg(long, value_delimiter = ',')]
    astro_delete: Vec<u32>,

    /// Delete charts from LUNA by phenomenon UUID.  Accepts one or more
    /// comma-separated UUIDs (e.g. `--luna-delete <uuid1>,<uuid2>`).
    /// Requires --luna-session.
    #[arg(long, value_delimiter = ',')]
    luna_delete: Vec<String>,

    /// In-place consolidation mode for `--target luna`: fetch every chart,
    /// cluster duplicate candidates by spacetime, prompt the user to choose
    /// which to keep, then delete the rest from LUNA.  Decisions persist to
    /// --decision-log so an interrupted run can be resumed.
    #[arg(long)]
    consolidate: bool,

    /// JSONL file recording each user decision (one record per keystroke,
    /// fsync'd before the next prompt).  Defaults to
    /// `$XDG_CACHE_HOME/blackmoon/luna-decisions.jsonl` (or
    /// `~/.cache/blackmoon/luna-decisions.jsonl`).
    #[arg(long)]
    decision_log: Option<PathBuf>,

    /// Print per-record detail (duplicate names, per-chart fetch status).
    #[arg(long, short)]
    verbose: bool,

    /// Print a shell completion script to stdout.
    #[arg(long = "generate-completion", value_name = "SHELL", num_args = 0..=1, default_missing_value = "auto", hide = true)]
    generate_completion: Option<String>,
}

/// Supported targets.  Type is detected from file extension when not
/// specified; --from / --to override.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum Target {
    /// Solar Fire chart file (.SFcht, cp1252).
    Sfcht,
    /// Zeus chart database (.zdb, UTF-8 semicolon-delimited text).
    Zeus,
    /// Astrodatabank XML export (.xml, export_format 160715).
    Adb,
    /// lunaastrology.com account.  Requires --session.
    Luna,
    /// astro.com account.  Requires --astro-session.
    Astro,
}

impl Target {
    fn from_path(path: &Path) -> Option<Self> {
        match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
            "sfcht" => Some(Self::Sfcht),
            "zdb" => Some(Self::Zeus),
            "xml" => Some(Self::Adb),
            _ => None,
        }
    }
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
        && cli.astro_delete.is_empty()
        && cli.luna_delete.is_empty()
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

// ── convert / merge ───────────────────────────────────────────────────────────

/// `(name, year, month, day, hour, minute, second)` — uniquely identifies a
/// chart for ID-lookup purposes when merging cross-source IDs.
type DatetimeKey = (String, i16, u8, u8, u8, u8, u8);

fn cmd_convert(cli: &Cli) -> Result<()> {
    // --target fills in --from / --to when those are absent.
    let from = cli.from.or(cli.target);
    let to = cli.to.or(cli.target);

    // Resolve astro.com session: credentials take priority over manual cid.
    let astro_cid: Option<String> = match (&cli.astro_user, &cli.astro_pass) {
        (Some(user), Some(pass)) => Some({
            println!("astro.com: logging in as {user}…");
            AstroSession::login(user, pass, cli.delay)
                .context("astro.com login failed")?
                .cid()
                .to_string()
        }),
        (Some(_), None) => bail!("--astro-pass (or ASTRO_COM_PASS) required with --astro-user"),
        (None, Some(_)) => bail!("--astro-user (or ASTRO_COM_USER) required with --astro-pass"),
        (None, None) => cli.astro_session.clone(),
    };

    // --astro-delete: remove charts from astro.com by nhor ID.
    if !cli.astro_delete.is_empty() {
        let cid = astro_cid.as_deref().context(
            "--astro-user/--astro-pass (or --astro-session) required for --astro-delete",
        )?;
        let email = cli.astro_user.as_deref().unwrap_or("");
        let pass = cli.astro_pass.as_deref().unwrap_or("");
        if pass.is_empty() {
            bail!("--astro-pass required for --astro-delete (password needed for deletion)");
        }
        let astro_session =
            AstroSession::from_cid(cid, cli.delay).context("building astro.com session")?;
        astro_session
            .delete_charts(email, pass, &cli.astro_delete)
            .context("astro.com delete")?;
        println!("Deleted {} chart(s).", cli.astro_delete.len());
        return Ok(());
    }

    // --luna-delete: remove charts from LUNA by phenom UUID.
    if !cli.luna_delete.is_empty() {
        let cookie = cli
            .luna_session
            .as_deref()
            .context("--luna-session is required for --luna-delete")?;
        let luna_session = LunaSession::new(cookie, cli.delay).context("building LUNA session")?;
        let total = cli.luna_delete.len();
        let mut failed: Vec<(String, String)> = Vec::new();
        for (i, phenom_id) in cli.luna_delete.iter().enumerate() {
            if i > 0 {
                std::thread::sleep(std::time::Duration::from_millis(cli.delay));
            }
            print!("[{:>3}/{}] {phenom_id}  ", i + 1, total);
            let _ = std::io::stdout().flush();
            match luna_session.delete_phenom(phenom_id) {
                Ok(()) => println!("deleted"),
                Err(e) => {
                    println!("[!] {e}");
                    failed.push((phenom_id.clone(), e.to_string()));
                }
            }
        }
        let deleted = total - failed.len();
        println!("Deleted {deleted}/{total} chart(s).");
        if !failed.is_empty() {
            bail!(
                "{} delete(s) failed; first: {} — {}",
                failed.len(),
                failed[0].0,
                failed[0].1
            );
        }
        return Ok(());
    }

    // --target luna --consolidate: fetch, group, prompt, delete.
    if cli.consolidate {
        let target = cli.target.or(cli.from).or(cli.to);
        if target != Some(Target::Luna) {
            bail!("--consolidate currently supports only --target luna");
        }
        return cmd_luna_consolidate(cli);
    }

    // Resolve --output: expand `now.ext` and supply defaults for web targets.
    // When --from luna/astro --normalize with no --output, the source is also the sink.
    let resolved_output: Option<PathBuf> = match &cli.output {
        Some(p) => Some(expand_now(p, now_secs())),
        None if matches!(from, Some(Target::Luna | Target::Astro)) && !cli.normalize => {
            Some(PathBuf::from(format!("{}.SFcht", utc_timestamp())))
        }
        None => None,
    };

    // --from luna/astro --normalize with no --output → same target for source and sink.
    let effective_to = if matches!(from, Some(Target::Luna | Target::Astro))
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
        (None, Some(p)) => Target::from_path(p).with_context(|| {
            format!(
                "cannot detect target from '{}'; use --to to specify",
                p.display()
            )
        })?,
        (None, None) => bail!("--output (or --to luna / --to astro) is required"),
    };

    let out_path = if matches!(out_target, Target::Luna | Target::Astro) {
        None
    } else {
        Some(
            resolved_output
                .as_ref()
                .context("--output is required when writing to a file target")?,
        )
    };

    // 1. Read existing output target (read-before-write dedup).
    //    For LUNA: fetch listing pages only (cheap) — no per-chart HTTP.
    //    Dedup against listing keys happens after the input is read (step 2).
    let mut existing: Vec<astrogram::chart::Chart> = Vec::new();
    let mut existing_nhor_ids: Vec<u32> = Vec::new();
    let mut luna_listing_keys: std::collections::HashSet<(String, i16, u8, u8, u8, u8, u8)> =
        std::collections::HashSet::new();
    if out_target == Target::Luna && from != Some(Target::Luna) {
        let cookie = cli
            .luna_session
            .as_deref()
            .context("--luna-session is required when --to luna")?;
        println!("LUNA (existing): reading…");
        let luna_session = LunaSession::new(cookie, cli.delay).context("building LUNA session")?;
        let listing = luna_session
            .fetch_listing()
            .context("reading LUNA listing")?;
        println!("Found {} charts in LUNA", listing.len());
        luna_listing_keys = listing
            .into_iter()
            .map(|r| (r.name, r.year, r.month, r.day, r.hour, r.minute, r.second))
            .collect();
    } else if out_target == Target::Astro && from != Some(Target::Astro) {
        let session = astro_cid
            .as_deref()
            .context("--astro-session is required when --to astro")?;
        println!("astro.com (existing): reading…");
        let astro_session =
            AstroSession::from_cid(session, cli.delay).context("building astro.com session")?;
        let (astro_existing, ids) = astro_session
            .fetch_charts()
            .context("fetching astro.com charts")?;
        existing_nhor_ids = ids;
        existing = astro_existing;
        println!("astro.com: {} charts (existing)", existing.len());
    } else if let Some(p) = out_path {
        if p.exists() {
            existing = read_file_target(p, out_target)
                .with_context(|| format!("reading existing output {}", p.display()))?;
            println!("{}: {} charts (existing)", p.display(), existing.len());
        }
    }

    // 2. Read input sources.
    let mut batches: Vec<Vec<astrogram::chart::Chart>> = vec![existing];

    let mut phenom_ids: Vec<String> = Vec::new(); // Luna
    let mut nhor_ids: Vec<u32> = Vec::new(); // Astro

    if from == Some(Target::Luna) {
        let cookie = cli
            .luna_session
            .as_deref()
            .context("--luna-session is required when --from luna")?;
        let luna_session = LunaSession::new(cookie, cli.delay).context("building LUNA session")?;
        let (luna_charts, ids) = luna_session
            .fetch_charts(
                cli.luna_resume_from.as_deref(),
                cli.normalize,
                &|i, total, name| {
                    print!("[{i:>3}/{total}] {name:<40}  ");
                    let _ = std::io::stdout().flush();
                },
                &|status| {
                    if !status.is_empty() {
                        println!("{status}");
                    }
                },
            )
            .context("fetching LUNA charts")?;
        phenom_ids = ids;
        batches.push(luna_charts);
    } else if from == Some(Target::Astro) {
        let session = astro_cid
            .as_deref()
            .context("--astro-session is required when --from astro")?;
        let astro_session =
            AstroSession::from_cid(session, cli.delay).context("building astro.com session")?;
        let (astro_charts, ids) = astro_session
            .fetch_charts()
            .context("fetching astro.com charts")?;
        nhor_ids = ids;
        batches.push(astro_charts);
    } else {
        if cli.inputs.is_empty() {
            bail!(
                "at least one input file is required (or use --from / --target luna / --target astro)"
            );
        }
        for path in &cli.inputs {
            let target = Target::from_path(path).with_context(|| {
                format!(
                    "cannot detect target from '{}'; rename the file or use --from to specify",
                    path.display()
                )
            })?;
            let charts = read_file_target(path, target)
                .with_context(|| format!("reading {}", path.display()))?;
            println!("{}: {} charts", path.display(), charts.len());
            batches.push(charts);
        }
    }

    // 2b. Filter input against the LUNA listing (name + full datetime).
    //     This runs before merge so consolidate never sees already-present charts.
    if !luna_listing_keys.is_empty() {
        let before: usize = batches.iter().map(Vec::len).sum();
        for batch in &mut batches {
            batch.retain(|c| {
                !luna_listing_keys.contains(&(
                    c.name.clone(),
                    c.year,
                    c.month,
                    c.day,
                    c.hour,
                    c.minute,
                    c.second,
                ))
            });
        }
        let skipped_luna = before - batches.iter().map(Vec::len).sum::<usize>();
        if skipped_luna > 0 {
            println!("  {skipped_luna} already in LUNA — skipped");
        }
    }

    // 3. Merge + dedup.
    //    Build ID lookups keyed by (name, year, month, day, hour, min, sec)
    //    from the web input batch before merge reorders/drops entries.
    let web_batch = batches.last();

    let phenom_lookup: HashMap<DatetimeKey, String> = if phenom_ids.is_empty() {
        HashMap::new()
    } else {
        web_batch
            .map(|batch| {
                batch
                    .iter()
                    .zip(phenom_ids.iter())
                    .map(|(c, pid)| {
                        (
                            (
                                c.name.clone(),
                                c.year,
                                c.month,
                                c.day,
                                c.hour,
                                c.minute,
                                c.second,
                            ),
                            pid.clone(),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default()
    };

    let nhor_lookup: HashMap<DatetimeKey, u32> = {
        let mut map: HashMap<DatetimeKey, u32> = HashMap::new();
        // IDs from the existing astro.com fetch (batches[0]).
        for (c, &id) in batches[0].iter().zip(existing_nhor_ids.iter()) {
            map.insert(
                (
                    c.name.clone(),
                    c.year,
                    c.month,
                    c.day,
                    c.hour,
                    c.minute,
                    c.second,
                ),
                id,
            );
        }
        // IDs from --from astro input batch (last batch).
        if !nhor_ids.is_empty() {
            if let Some(batch) = web_batch {
                for (c, &id) in batch.iter().zip(nhor_ids.iter()) {
                    map.insert(
                        (
                            c.name.clone(),
                            c.year,
                            c.month,
                            c.day,
                            c.hour,
                            c.minute,
                            c.second,
                        ),
                        id,
                    );
                }
            }
        }
        map
    };

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

    // Resolve per-chart web IDs for the merged set (post-normalize name may differ).
    let key = |c: &astrogram::chart::Chart| {
        (
            c.name.clone(),
            c.year,
            c.month,
            c.day,
            c.hour,
            c.minute,
            c.second,
        )
    };
    let merged_phenom_ids: Vec<String> = merged
        .iter()
        .map(|c| phenom_lookup.get(&key(c)).cloned().unwrap_or_default())
        .collect();
    let merged_nhor_ids: Vec<u32> = merged
        .iter()
        .map(|c| nhor_lookup.get(&key(c)).copied().unwrap_or(0))
        .collect();

    // 5. Write.
    if out_target == Target::Luna {
        if cli.normalize {
            println!("Charts to write ({}):", merged.len());
            for chart in &merged {
                println!("  {}", chart.name);
            }
            eprint!(
                "About to write {} chart{} to your LUNA account. Proceed? [y/N] ",
                merged.len(),
                if merged.len() == 1 { "" } else { "s" }
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
        let cookie = cli
            .luna_session
            .as_deref()
            .context("--luna-session is required when --to luna")?;
        let luna_session = LunaSession::new(cookie, cli.delay).context("building LUNA session")?;
        luna_session
            .write_charts(
                &merged,
                &merged_phenom_ids,
                &|i, total, name| {
                    print!("[{i:>3}/{total}] {name:<40}  ");
                    let _ = std::io::stdout().flush();
                },
                &|status| println!("{status}"),
            )
            .context("writing to LUNA")?;
    } else if out_target == Target::Astro {
        if cli.normalize {
            println!("Charts to write ({}):", merged.len());
            for chart in &merged {
                println!("  {}", chart.name);
            }
            eprint!(
                "About to write {} chart{} to your astro.com account. Proceed? [y/N] ",
                merged.len(),
                if merged.len() == 1 { "" } else { "s" }
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
        let session = astro_cid
            .as_deref()
            .context("--astro-session is required when --to astro")?;
        let astro_session =
            AstroSession::from_cid(session, cli.delay).context("building astro.com session")?;
        astro_session
            .write_charts(
                &merged,
                &merged_nhor_ids,
                &|i, total, name| {
                    print!("[{i:>3}/{total}] {name:<40}  ");
                    let _ = std::io::stdout().flush();
                },
                &|status| println!("{status}"),
            )
            .context("writing to astro.com")?;
    } else {
        let p = out_path.expect("out_path set for non-luna target");
        if cli.verbose {
            for name in &skipped {
                println!("  dup: {name}");
            }
        }
        write_file_target(p, out_target, &merged)?;
        if existing_count > 0 {
            println!("  existing: {existing_count:>6}");
        }
        println!("  in:       {new_input_count:>6}");
        println!("  dupes:    {dupes:>6}");
        println!("  out:      {:>6}", merged.len());
        println!("wrote {}", p.display());
    }
    Ok(())
}

// ── normalize in-place ────────────────────────────────────────────────────────

fn cmd_normalize_inplace(inputs: &[PathBuf]) -> Result<()> {
    if inputs.is_empty() {
        bail!("at least one input file is required for --normalize");
    }
    for path in inputs {
        let target = Target::from_path(path)
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

fn cmd_luna_consolidate(cli: &Cli) -> Result<()> {
    use astrogram::consolidate::group_candidates;
    use astrogram::decision_log::{Choice, DecisionLog};

    let cookie = cli
        .luna_session
        .as_deref()
        .context("--luna-session is required for --consolidate")?;
    let log_path = cli
        .decision_log
        .clone()
        .unwrap_or_else(default_decision_log_path);

    println!("Decision log: {}", log_path.display());

    let session = LunaSession::new(cookie, cli.delay).context("building LUNA session")?;
    let (charts, phenom_ids) = session
        .fetch_charts(
            cli.luna_resume_from.as_deref(),
            false,
            &|i, total, name| {
                print!("[{i:>3}/{total}] {name:<40}  ");
                let _ = std::io::stdout().flush();
            },
            &|status| println!("{status}"),
        )
        .context("fetching LUNA charts")?;
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
    // Scope the stdin/stdout locks so they're released before the apply
    // phase: `Stdin`'s mutex is *not* reentrant, so a `read_line` for the
    // confirmation prompt below would deadlock if `stdin_lock` were still
    // alive.
    let outcome = {
        let stdin = std::io::stdin();
        let mut stdin_lock = stdin.lock();
        let stdout = std::io::stdout();
        let mut stdout_lock = stdout.lock();
        consolidate_ui::run_loop(
            &groups,
            &charts,
            &phenom_ids,
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
        "About to delete {} chart(s) from LUNA.  Proceed? [y/N] ",
        drops.len()
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
    for (i, pid) in drops.iter().enumerate() {
        if i > 0 {
            std::thread::sleep(std::time::Duration::from_millis(cli.delay));
        }
        print!("[{:>3}/{}] {pid}  ", i + 1, total);
        let _ = std::io::stdout().flush();
        match session.delete_phenom(pid) {
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
        Target::Luna => bail!("use --from luna rather than passing a file path"),
        Target::Astro => bail!("use --from astro rather than passing a file path"),
    }
}

fn write_file_target(
    path: &Path,
    target: Target,
    charts: &[astrogram::chart::Chart],
) -> Result<()> {
    match target {
        Target::Sfcht => {
            let bytes = sfcht::write_file(charts)?;
            std::fs::write(path, bytes)?;
        }
        Target::Zeus => {
            let text = zeus::write_file(charts);
            std::fs::write(path, text)?;
        }
        Target::Adb => {
            let text = adbxml::write_file(charts);
            std::fs::write(path, text)?;
        }
        Target::Luna => bail!("use --to luna for writing to LUNA"),
        Target::Astro => bail!("use --to astro for writing to astro.com"),
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
