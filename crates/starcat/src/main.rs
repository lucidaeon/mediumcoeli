//! `starcat` — command-line astrological ephemeris.
//!
//! Computes tropical ecliptic-of-date apparent positions (longitude,
//! latitude, distance) for the classical bodies (Sun, Moon, Mercury
//! through Pluto) at a given civil date and location, using JPL DE441
//! as the underlying ephemeris source. Three coord modes ship:
//! geocentric (default), topocentric (`--lat` + `--lon`), heliocentric
//! (`--helio`). Also emits the chart axes (Ac/Ds, Mc/Ic,
//! Vx/Ax, Nn/Sn, Lil/Pri — see "Chart points" below), the eight
//! Hermetic lots, and seven always-on house systems (Whole Sign,
//! Equal-from-Ac, Placidus, Regiomontanus, Porphyry, Alcabitius,
//! Morinus). An additional twelve house systems (Koch, Campanus,
//! Meridian, Equal-from-MC, Horizontal, Topocentric, Krusinski,
//! Sripati, Vehlow Equal, Carter Poli-Equatorial, Pullen Sinusoidal
//! Delta, Pullen Sinusoidal Ratio) are available with the
//! `noref-houses` Cargo feature — these have implementations but no
//! refchart oracle yet; see `docs/superpowers/plans/HOUSE_PROMOTION.md`.
//!
//! # Chart points
//!
//! Two-letter labels, `UPPERlower` in display, lowercase in JSON:
//!
//! | Code | Concept | Notes |
//! |------|---------|-------|
//! | Ac / Ds | Ascendant / Descendant | needs lat + lon |
//! | Mc / Ic | Medium Coeli / Imum Coeli | needs lon |
//! | Vx / Ax | Vertex / Anti-Vertex | needs lat + lon; degenerate at equator + poles |
//! | Nn / Sn | North / South Node | `--nodes mean\|true` (default `true`) |
//! | Lil / Pri | Black Moon Lilith / Priapus | `--lilith mean\|true` (default `true`) |
//!
//! Computation-mode aliases on `--nodes` and `--lilith`:
//! `mean` ≡ `average`; `true` ≡ `apparent` ≡ `osculating`.
//!
//! # Usage
//!
//! ```text
//! starcat compute \
//!     --date 1955-11-12 \
//!     --time 22:04:00 \
//!     --calendar gregorian \
//!     --tz=-08:00 \
//!     [--lat 34.1389 --lon=-118.3525]   # topocentric \
//!     [--helio]                          # heliocentric \
//!     [--bodies sun,moon,mercury,...] \
//!     [--house whole-sign,equal-from-asc,placidus,regiomontanus,porphyry] \
//!     [--dd | --dms | --ddm | --dm]      # coord format (page: --dm; text: --dd) \
//!     [--jpl-data DIR] \
//!     [--text | --page]                  # output style (default = jzod)
//! ```
//!
//! # JPL data resolution
//!
//! The library needs both the ASCII header and binary ephemeris file
//! from a JPL DE-series release. Resolution order:
//!
//! 1. `--jpl-data DIR` (directory containing both files).
//! 2. `$STARCAT_JPL_DATA` env var (same as `--jpl-data`).
//!
//! No default path — one of the two must be supplied.
//!
//! When given a directory, the library auto-discovers the highest-
//! numbered ephemeris release (DE441, DE442, …) and picks
//! little-endian binaries on x86/ARM hosts.
//!
//! For ancient charts with no civil time zone, use `--lmt` + `--lon`
//! to derive Local Mean Time from the observer's longitude:
//!
//! ```text
//! starcat compute \
//!     --date 0120-02-08 \
//!     --time 18:35:00 \
//!     --calendar julian \
//!     --lmt --lon 36.157   # Antioch, east-of-Greenwich degrees \
//!
//! ```

// jd_ut/jd_tt, ac_rad/ramc/ac_deg/mc_deg, etc. are astronomically distinct.
#![allow(clippy::similar_names)]
// ComputeArgs is a clap derive of the full CLI surface — many boolean flags
// are inherent to the command-line shape.
#![allow(clippy::struct_excessive_bools)]
// A few CLI-orchestration functions naturally span >100 lines; splitting
// them produces worse code than the lint resolves.
#![allow(clippy::too_many_lines)]
// JsonBody.{longitude,latitude,daily_speed}_deg, distance_au — the `_deg`/`_au`
// suffix names the unit, not a redundancy with the struct.
#![allow(clippy::struct_field_names)]
// Serde's serialize_with / SerializeMap signatures force &T parameters even
// when T is small and Copy; we can't change the upstream API.
#![allow(clippy::trivially_copy_pass_by_ref, clippy::ref_option)]

use anyhow::{Context, Result, bail};
use clap::{Args, Parser, Subcommand, ValueEnum};
use pericynthion::body::Body;
use pericynthion::chart::{ChartRequest, ComputedChart, ModeRequest};
use pericynthion::coords::topocentric::ObserverLocation;
use pericynthion::ephemeris::Ephemeris;
use pericynthion::geo::{parse_lat, parse_lon};
use pericynthion::houses::{HouseCusps, HouseSystem};
use pericynthion::jpl::{discover, header::parse as parse_header, reader::EphemerisFile};
use pericynthion::lots::Sect;
use pericynthion::time::calendar::{Calendar, CivilDate};
use pericynthion::time::zone::Zone;
use pericynthion::time::{parse_date, parse_time, parse_tz};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "starcat",
    version,
    about = "Astrological ephemeris from JPL DE441",
    long_about = "\
starcat — astrological ephemeris from JPL DE441

COORDINATE SYSTEM
  geocentric      apparent position from Earth's centre (default)
  topocentric     parallax-corrected; add --lat + --lon
  heliocentric    Sun-centred; add --helio

ZODIAC
  tropical     ecliptic longitude from the true vernal equinox (current)
  sidereal     tropical minus ayanamsha — 47+ calibrations (roadmap)
  draconic     0° = Moon's mean North Node (roadmap)

CHART POINTS EMITTED
  Bodies   geocentric/topocentric: Sun, Moon, Mercury, Venus, Mars,
           Jupiter, Saturn, Uranus, Neptune, Pluto
           heliocentric: Earth replaces Sun
  Angles   MC, IC (need longitude); ASC, DSC, Vx, Ax (need lat + lon)
           Nn, Sn (lunar nodes; geo/topo only — see --nodes)
           Lil, Pri (Black Moon Lilith / Priapus; geo/topo only — see --lilith)
  Lots     Fortune, Spirit, Exaltation (need ASC + Sun + Moon),
           Eros (+Venus), Necessity (+Mercury), Courage (+Mars),
           Victory (+Jupiter), Nemesis (+Saturn), Sect; geo/topo only
  Houses   Whole Sign, Equal-from-ASC, Placidus, Regiomontanus, Porphyry
           (need lat + lon; geo/topo only)

Run 'starcat compute --help' for the full argument reference.",
    arg_required_else_help = true
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

// One CLI struct per process — boxing the big variant would only add ceremony.
#[allow(clippy::large_enum_variant)]
#[derive(Subcommand, Debug)]
enum Command {
    /// Compute tropical ecliptic-of-date apparent geocentric positions.
    Compute(ComputeArgs),
    /// Print a shell completion script to stdout.
    #[command(hide = true)]
    GenerateCompletion { shell: Option<clap_complete::Shell> },
}

#[derive(Args, Debug)]
struct ComputeArgs {
    /// Date in YYYY-MM-DD form (proleptic; negative years allowed for BCE).
    #[arg(long)]
    date: String,

    /// Time in `HH:MM[:SS]` form, in the zone specified by `--tz` or `--lmt`.
    #[arg(long)]
    time: String,

    /// Which calendar the date is recorded in. No default — caller must choose.
    #[arg(long)]
    calendar: CalendarArg,

    /// UT offset for the recorded time, as ±HH:MM (e.g. -05:00). Mutually
    /// exclusive with `--lmt`.
    #[arg(long, conflicts_with = "lmt")]
    tz: Option<String>,

    /// Optional human-readable timezone name (e.g. `PST`, `CET`) — used only
    /// in the page banner. Pure display string; no calendar logic.
    /// (Available only with the `page` feature.)
    #[cfg(feature = "page")]
    #[arg(long = "tz-name")]
    tz_name: Option<String>,

    /// Use Local Mean Time derived from `--lon`. Mutually exclusive with
    /// `--tz`. For pre-railway / ancient charts with no civil zone.
    #[arg(long, requires = "lon")]
    lmt: bool,

    /// Geographic latitude — any format: decimal degrees (`34.14`),
    /// DMS (`39° 44' 28" N`), or DDM (`39° 44.477' N`). Required for
    /// topocentric positions and the Ac/Vx angles.
    #[arg(long)]
    lat: Option<String>,

    /// Geographic longitude — any format: decimal degrees (`36.157`),
    /// DMS (`36° 9' 25" E`), or DDM (`36° 9.417' E`). Required by `--lmt`;
    /// when paired with `--lat`, positions are computed topocentric.
    #[arg(long)]
    lon: Option<String>,

    /// Compute heliocentric ecliptic positions (Sun-centred) instead of
    /// geocentric. Earth replaces the Sun in the default body list.
    #[arg(long)]
    helio: bool,

    /// Comma-separated bodies to compute. Defaults to all ten classical
    /// bodies (sun,moon,mercury,venus,mars,jupiter,saturn,uranus,neptune,pluto).
    #[arg(long, value_delimiter = ',')]
    bodies: Option<Vec<BodyArg>>,

    /// Comma-separated house system(s) to emit. Defaults to all three
    /// implemented systems (whole-sign,equal-from-asc,placidus).
    #[arg(long = "house", value_delimiter = ',')]
    houses: Option<Vec<HouseArg>>,

    /// Lunar-node computation mode: `mean` (Meeus polynomial; aliases:
    /// `average`) or `true` (osculating from Moon state; aliases:
    /// `apparent`, `osculating`). Default: `true`.
    #[arg(long = "nodes", default_value = "true")]
    nodes: NodesMode,

    /// Black Moon Lilith computation mode: `mean` (polynomial; alias:
    /// `average`) or `true` (osculating apogee from Moon state; aliases:
    /// `apparent`, `osculating`). Default: `true`.
    // https://web.archive.org/web/20260603210459/https://www.chani.com/astro-education/how-to-work-with-black-moon-lilith
    #[arg(long = "lilith", default_value = "true")]
    lilith: LilithMode,

    /// Directory containing a JPL DE-series ephemeris release (must
    /// contain a `header.NNN` and matching `linux_*.NNN` or
    /// `xnp_*.NNN` binary). Falls back to `$STARCAT_JPL_DATA` when
    /// omitted; one or the other must be set.
    #[arg(long)]
    jpl_data: Option<PathBuf>,

    /// Emit JZOD-format JSON (default). Explicit flag; no-op when neither
    /// `--text` nor `--page` is given. Mutually exclusive with `--text` / `--page`.
    #[arg(long, visible_alias = "json", group = "output_mode")]
    jzod: bool,

    /// Emit plain text (banner + placements list). Defaults to `--dd` coord
    /// format. Mutually exclusive with `--page` / `--jzod`.
    #[arg(long, group = "output_mode")]
    text: bool,

    /// Emit the page renderer (banner + 4-column placements table sorted in
    /// zodiacal order from H1). Defaults to `--dm` coord format.
    /// No-op when built without the `page` feature.
    /// Mutually exclusive with `--text` / `--jzod`.
    #[arg(long, group = "output_mode")]
    page: bool,

    /// Format longitudes/latitudes as decimal degrees (default).
    /// e.g. `10.5042° Sco`. Mutually exclusive with `--dms` / `--ddm` / `--dm` / `--d`.
    #[arg(long, group = "coord_format")]
    dd: bool,

    /// Format longitudes/latitudes as degrees-minutes-seconds (seconds truncated).
    /// e.g. `10°30'15" Sco`. Mutually exclusive with `--dd` / `--ddm` / `--dm` / `--d`.
    #[arg(long, group = "coord_format")]
    dms: bool,

    /// Format longitudes/latitudes as degrees and decimal minutes.
    /// e.g. `10°30.252' Sco`. Mutually exclusive with `--dd` / `--dms` / `--dm` / `--d`.
    #[arg(long, group = "coord_format")]
    ddm: bool,

    /// Format longitudes/latitudes as degrees-minutes (arcseconds truncated).
    /// e.g. `10°30' Sco`. Mutually exclusive with `--dd` / `--dms` / `--ddm` / `--d`.
    #[arg(long, group = "coord_format")]
    dm: bool,

    /// Format longitudes/latitudes as integer degrees only (arcminutes and
    /// arcseconds truncated). e.g. `10° Sco`.
    /// Mutually exclusive with `--dd` / `--dms` / `--ddm` / `--dm`.
    #[arg(long = "d", group = "coord_format")]
    d: bool,
}

/// Output format for sexagesimal coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CoordFormat {
    /// Decimal degrees (`10.5042°`). Default.
    Dd,
    /// Degrees-minutes-seconds, seconds truncated (`10°30'15"`).
    Dms,
    /// Degrees and decimal minutes (`10°30.252'`).
    Ddm,
    /// Degrees-minutes only, arcseconds truncated (`10°30'`).
    Dm,
    /// Integer degrees only, arcminutes and arcseconds truncated (`10°`).
    D,
}

impl CoordFormat {
    fn from_args(args: &ComputeArgs) -> Self {
        if args.dms {
            Self::Dms
        } else if args.ddm {
            Self::Ddm
        } else if args.dm {
            Self::Dm
        } else if args.d {
            Self::D
        } else {
            // Page rendering defaults to --dm to match the banner's coord style;
            // text and jzod default to --dd.
            #[cfg(feature = "page")]
            if args.page {
                return Self::Dm;
            }
            Self::Dd
        }
    }
}

#[derive(ValueEnum, Debug, Clone, Copy)]
enum CalendarArg {
    Julian,
    Gregorian,
    /// Auto-detect: Julian before 1582-10-15, Gregorian on or after.
    Auto,
}

impl From<CalendarArg> for Calendar {
    fn from(c: CalendarArg) -> Self {
        match c {
            CalendarArg::Julian => Self::Julian,
            CalendarArg::Gregorian => Self::Gregorian,
            CalendarArg::Auto => Self::Auto,
        }
    }
}

/// Computation mode for the lunar nodes (Nn / Sn).
///
/// `Mean` uses the Meeus polynomial (smoothed, monotonically retrograde);
/// `True` uses the Moon's osculating orbital plane (matches refchart's
/// "Nod" entries, can be stationary or briefly direct).
#[derive(ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
enum NodesMode {
    /// Closed-form polynomial mean node.
    #[value(alias = "average")]
    Mean,
    /// Instantaneous osculating node from the Moon's state vector.
    #[value(alias = "apparent", alias = "osculating")]
    True,
}

/// Computation mode for Black Moon Lilith (and Priapus).
///
/// `Mean` uses the Meeus polynomial for the Moon's mean apogee;
/// `True` uses the Laplace-Runge-Lenz eccentricity vector from the Moon
/// state (osculating apogee).
#[derive(ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
enum LilithMode {
    /// Closed-form polynomial mean apogee.
    #[value(alias = "average")]
    Mean,
    /// Osculating apogee from the Moon's state vector.
    #[value(alias = "apparent", alias = "osculating")]
    True,
}

/// Which house system(s) the CLI should emit. Order is the canonical
/// presentation order used when `--house` is omitted (all seven).
/// This is a thin clap shim that converts to `pericynthion::houses::HouseSystem`.
#[derive(ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
enum HouseArg {
    WholeSign,
    EqualFromAsc,
    Placidus,
    Regiomontanus,
    Porphyry,
    Alcabitius,
    #[cfg(feature = "noref-houses")]
    Koch,
    #[cfg(feature = "noref-houses")]
    Campanus,
    Morinus,
    #[cfg(feature = "noref-houses")]
    Meridian,
    #[cfg(feature = "noref-houses")]
    EqualFromMc,
    #[cfg(feature = "noref-houses")]
    Horizontal,
    #[cfg(feature = "noref-houses")]
    Topocentric,
    #[cfg(feature = "noref-houses")]
    Krusinski,
    #[cfg(feature = "noref-houses")]
    Sripati,
    #[cfg(feature = "noref-houses")]
    Vehlow,
    #[cfg(feature = "noref-houses")]
    Carter,
    #[cfg(feature = "noref-houses")]
    PullenSd,
    #[cfg(feature = "noref-houses")]
    PullenSr,
}

impl HouseArg {
    fn to_house_system(self) -> HouseSystem {
        match self {
            Self::WholeSign => HouseSystem::WholeSign,
            Self::EqualFromAsc => HouseSystem::EqualFromAsc,
            Self::Placidus => HouseSystem::Placidus,
            Self::Regiomontanus => HouseSystem::Regiomontanus,
            Self::Porphyry => HouseSystem::Porphyry,
            Self::Alcabitius => HouseSystem::Alcabitius,
            #[cfg(feature = "noref-houses")]
            Self::Koch => HouseSystem::Koch,
            #[cfg(feature = "noref-houses")]
            Self::Campanus => HouseSystem::Campanus,
            Self::Morinus => HouseSystem::Morinus,
            #[cfg(feature = "noref-houses")]
            Self::Meridian => HouseSystem::Meridian,
            #[cfg(feature = "noref-houses")]
            Self::EqualFromMc => HouseSystem::EqualFromMc,
            #[cfg(feature = "noref-houses")]
            Self::Horizontal => HouseSystem::Horizontal,
            #[cfg(feature = "noref-houses")]
            Self::Topocentric => HouseSystem::Topocentric,
            #[cfg(feature = "noref-houses")]
            Self::Krusinski => HouseSystem::Krusinski,
            #[cfg(feature = "noref-houses")]
            Self::Sripati => HouseSystem::Sripati,
            #[cfg(feature = "noref-houses")]
            Self::Vehlow => HouseSystem::Vehlow,
            #[cfg(feature = "noref-houses")]
            Self::Carter => HouseSystem::Carter,
            #[cfg(feature = "noref-houses")]
            Self::PullenSd => HouseSystem::PullenSd,
            #[cfg(feature = "noref-houses")]
            Self::PullenSr => HouseSystem::PullenSr,
        }
    }
}

#[derive(ValueEnum, Debug, Clone, Copy)]
enum BodyArg {
    Sun,
    Moon,
    Mercury,
    Venus,
    Earth,
    Mars,
    Jupiter,
    Saturn,
    Uranus,
    Neptune,
    Pluto,
}

impl From<BodyArg> for Body {
    fn from(b: BodyArg) -> Self {
        match b {
            BodyArg::Sun => Self::Sun,
            BodyArg::Moon => Self::Moon,
            BodyArg::Mercury => Self::Mercury,
            BodyArg::Venus => Self::Venus,
            BodyArg::Earth => Self::Earth,
            BodyArg::Mars => Self::Mars,
            BodyArg::Jupiter => Self::Jupiter,
            BodyArg::Saturn => Self::Saturn,
            BodyArg::Uranus => Self::Uranus,
            BodyArg::Neptune => Self::Neptune,
            BodyArg::Pluto => Self::Pluto,
        }
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Compute(args) => cmd_compute(args),
        Command::GenerateCompletion { shell } => {
            use clap::CommandFactory;
            let Some(shell) = shell.or_else(detect_shell) else {
                anyhow::bail!(
                    "could not detect shell from $SHELL; pass it explicitly (e.g. generate-completion zsh)"
                );
            };
            clap_complete::generate(
                shell,
                &mut Cli::command(),
                "starcat",
                &mut std::io::stdout(),
            );
            Ok(())
        }
    }
}

fn detect_shell() -> Option<clap_complete::Shell> {
    let shell = std::env::var("SHELL").ok()?;
    let name = std::path::Path::new(&shell).file_name()?.to_str()?;
    name.parse().ok()
}

#[cfg(test)]
fn resolve_observer(lat_s: Option<&str>, lon_s: Option<&str>) -> Result<Option<ObserverLocation>> {
    let Some(lat_s) = lat_s else { return Ok(None) };
    let lat = parse_lat(lat_s).with_context(|| format!("invalid --lat {lat_s:?}"))?;
    let lon_s = lon_s.ok_or_else(|| anyhow::anyhow!("--lat requires --lon"))?;
    let lon = parse_lon(lon_s).with_context(|| "invalid longitude for topocentric".to_string())?;
    Ok(Some(ObserverLocation {
        lat_deg: lat,
        lon_deg: lon,
        elev_m: 0.0,
    }))
}

// Called once per process from `main`; taking ComputeArgs by value lets the
// body freely consume fields (e.g. `args.bodies` via `.clone()`-then-drop)
// without lifetime juggling. The allocation cost is zero in CLI context.
#[allow(clippy::needless_pass_by_value)]
fn cmd_compute(args: ComputeArgs) -> Result<()> {
    // === Output format (read before any partial moves of `args`) ===
    let fmt = CoordFormat::from_args(&args);

    // === Parse date and time ===
    let (year, month, day) =
        parse_date(&args.date).with_context(|| format!("invalid --date {:?}", args.date))?;
    let (hour, minute, second) =
        parse_time(&args.time).with_context(|| format!("invalid --time {:?}", args.time))?;
    let civil = CivilDate {
        year,
        month,
        day,
        hour,
        minute,
        second,
    };

    // === Zone ===
    let zone = if args.lmt {
        let lon_s = args
            .lon
            .as_deref()
            .expect("clap enforces --lon when --lmt is set");
        Zone::Lmt {
            longitude_east: parse_lon(lon_s).with_context(|| format!("invalid --lon {lon_s:?}"))?,
        }
    } else if let Some(tz) = &args.tz {
        parse_tz(tz)?
    } else {
        bail!("either --tz or --lmt (with --lon) must be supplied")
    };

    // === Ephemeris file ===
    let (header_path, binary_path) = resolve_jpl_paths(args.jpl_data.as_deref())?;
    let header_src = std::fs::read_to_string(&header_path)
        .with_context(|| format!("read {}", header_path.display()))?;
    let header = parse_header(&header_src).context("parse JPL ASCII header")?;
    let file = EphemerisFile::open(&binary_path, &header)
        .with_context(|| format!("open {}", binary_path.display()))?;
    let ephem = Ephemeris::new(&file, &header).context("build ephemeris facade")?;

    // === Coordinate mode request ===
    let mode_request = if args.helio {
        ModeRequest::Heliocentric
    } else if args.lat.is_some() {
        ModeRequest::Topocentric
    } else {
        ModeRequest::Geocentric
    };

    // === Observer location (for Topocentric) ===
    let obs_lat = args.lat.as_deref().and_then(|s| parse_lat(s).ok());
    let obs_lon = args.lon.as_deref().and_then(|s| parse_lon(s).ok());

    // === Bodies ===
    let bodies: Option<Vec<Body>> = args
        .bodies
        .clone()
        .map(|list| list.into_iter().map(Body::from).collect());

    // === House systems ===
    let is_jzod = !args.text && !args.page;
    let house_systems: Vec<HouseSystem> = if is_jzod {
        HouseSystem::DEFAULT_SET.to_vec()
    } else {
        args.houses
            .as_ref()
            .map(|v| v.iter().copied().map(HouseArg::to_house_system).collect())
            .unwrap_or_else(|| HouseSystem::DEFAULT_SET.to_vec())
    };

    // === Calendar ===
    let calendar: Calendar = args.calendar.into();

    // === Build request and compute ===
    let request = ChartRequest {
        civil,
        calendar,
        zone,
        mode: mode_request,
        lat_deg: obs_lat,
        lon_deg: obs_lon,
        bodies,
        houses: house_systems,
    };
    let computed = pericynthion::chart::compute(&ephem, &request)
        .with_context(|| "chart computation failed")?;

    // === Output ===
    if is_jzod {
        let birth = pericynthion::jzod::ChartBirth {
            year,
            month,
            day,
            hour,
            minute,
            second: second.floor() as u8,
            lat: obs_lat,
            lon: obs_lon,
        };
        let chart =
            pericynthion::jzod::to_jzod_chart(&computed, &birth, uuid::Uuid::new_v4().to_string());
        println!(
            "{}",
            jzod::to_string_pretty(&jzod::JzodDocument::new(vec![chart]))
        );
    } else if args.page {
        #[cfg(feature = "page")]
        {
            let page_house_count = args
                .houses
                .as_ref()
                .map_or(HouseSystem::DEFAULT_SET.len(), Vec::len);
            if page_house_count != 1 {
                bail!(
                    "page rendering requires exactly one --house system; got {} ({:?}). \
                     Specify e.g. --house placidus or --house whole-sign.",
                    page_house_count,
                    args.houses
                );
            }
            print_page(&args, &computed, fmt);
        }
    } else {
        print_text(&computed, fmt, args.nodes, args.lilith);
    }
    Ok(())
}

/// Resolve the (header, binary) pair from CLI args + env.
///
/// Precedence:
///   1. `--jpl-data DIR` → autodiscover within DIR.
///   2. `$STARCAT_JPL_DATA` → autodiscover.
fn resolve_jpl_paths(data_dir_arg: Option<&std::path::Path>) -> Result<(PathBuf, PathBuf)> {
    let dir = if let Some(d) = data_dir_arg {
        d.to_path_buf()
    } else if let Ok(env) = std::env::var("STARCAT_JPL_DATA") {
        PathBuf::from(env)
    } else {
        bail!(
            "no JPL data location supplied. Pass --jpl-data DIR or set the \
             STARCAT_JPL_DATA environment variable to a directory \
             containing header.NNN and a matching linux_*.NNN binary."
        );
    };
    let paths = discover::discover(&dir)
        .with_context(|| format!("autodiscover JPL ephemeris in {}", dir.display()))?;
    Ok((paths.header, paths.binary))
}

// =============================================================================
// Output rendering
// =============================================================================

fn phase_name_str(name: pericynthion::coords::phase::LunarPhaseName) -> &'static str {
    use pericynthion::coords::phase::LunarPhaseName as P;
    match name {
        P::NewMoon => "new moon",
        P::Crescent => "crescent",
        P::FirstQuarter => "first quarter",
        P::Gibbous => "gibbous",
        P::FullMoon => "full moon",
        P::Disseminating => "disseminating",
        P::LastQuarter => "last quarter",
        P::Balsamic => "balsamic",
    }
}

fn print_text(
    computed: &ComputedChart,
    fmt: CoordFormat,
    nodes_mode: NodesMode,
    lilith_mode: LilithMode,
) {
    println!("JD UT  : {:.6}", computed.jd_ut);
    println!("JD TT  : {:.6}", computed.jd_tt);
    let coord_label = match &computed.mode {
        pericynthion::chart::CoordMode::Geocentric => "geocentric".to_string(),
        pericynthion::chart::CoordMode::Topocentric(obs) => {
            format!(
                "topocentric (lat={} lon={})",
                format_signed_deg(obs.lat_deg, fmt, 2),
                format_signed_deg(obs.lon_deg, fmt, 3),
            )
        }
        pericynthion::chart::CoordMode::Heliocentric => "heliocentric".to_string(),
    };
    println!("Coords : {coord_label}");
    println!();
    let lon_w = lon_col_width(fmt);
    let lat_w = lat_col_width(fmt);
    println!(
        "{:<8} {:>lon_w$} {:>lat_w$} {:>14}",
        "Body",
        "Longitude",
        "Latitude",
        "Distance (AU)",
        lon_w = lon_w,
        lat_w = lat_w,
    );
    println!("{}", "-".repeat(8 + 1 + lon_w + 1 + lat_w + 1 + 14));
    for cb in &computed.bodies {
        println!(
            "{:<8} {} {} {:>14.6}",
            cb.body.name(),
            format_zodiac_lon(cb.position.longitude_deg, fmt),
            format_signed_lat(cb.position.latitude_deg, fmt),
            cb.position.distance_au
        );
    }

    if let Some(ang) = &computed.angles {
        // Select Nn/Sn based on nodes_mode
        let (nn_deg, sn_deg) = match (nodes_mode, &computed.nodes) {
            (NodesMode::Mean, Some(n)) => (Some(n.mean_nn_deg), Some(n.mean_sn_deg)),
            (NodesMode::True, Some(n)) => (Some(n.true_nn_deg), Some(n.true_sn_deg)),
            _ => (None, None),
        };
        // Select Lil/Pri based on lilith_mode
        let (lil_deg, pri_deg) = match (lilith_mode, &computed.lilith) {
            (LilithMode::Mean, Some(l)) => (Some(l.mean_lilith_deg), Some(l.mean_priapus_deg)),
            (LilithMode::True, Some(l)) => (Some(l.true_lilith_deg), Some(l.true_priapus_deg)),
            _ => (None, None),
        };

        println!();
        println!("{:<8} {:>lon_w$}", "Point", "Longitude", lon_w = lon_w);
        println!("{}", "-".repeat(8 + 1 + lon_w));
        // Display labels use the standardized 2-letter UPPERlower convention:
        // As / Ds (Ascendant axis), Mc / Ic (meridian axis), Vx / Ax (vertex
        // axis), Nn / Sn (lunar nodes).
        for (label, lon) in [
            ("Mc", Some(ang.mc_deg)),
            ("Ic", Some(ang.ic_deg)),
            ("Ac", ang.ac_deg),
            ("Ds", ang.ds_deg),
            ("Vx", ang.vx_deg),
            ("Ax", ang.ax_deg),
            ("Nn", nn_deg),
            ("Sn", sn_deg),
            ("Lil", lil_deg),
            ("Pri", pri_deg),
        ] {
            if let Some(lon_deg) = lon {
                println!("{:<8} {}", label, format_zodiac_lon(lon_deg, fmt));
            }
        }
    }

    if let Some(l) = &computed.lots {
        println!();
        println!(
            "Sect   : {}",
            match l.sect {
                Sect::Day => "day",
                Sect::Night => "night",
            }
        );
        println!("{:<11} {:>lon_w$}", "Lot", "Longitude", lon_w = lon_w);
        println!("{}", "-".repeat(11 + 1 + lon_w));
        // Ordering: Fortune, Spirit, Exaltation always; downstream lots in
        // the captain-specified sequence Necessity → Eros → Courage →
        // Victory → Nemesis, each only when its planet is present.
        let mut rows: Vec<(&str, f64)> = vec![
            ("Fortune", l.fortune_deg),
            ("Spirit", l.spirit_deg),
            ("Exaltation", l.exaltation_deg),
        ];
        for (label, val) in [
            ("Necessity", l.necessity_deg),
            ("Eros", l.eros_deg),
            ("Courage", l.courage_deg),
            ("Victory", l.victory_deg),
            ("Nemesis", l.nemesis_deg),
        ] {
            if let Some(v) = val {
                rows.push((label, v));
            }
        }
        for (label, lon_deg) in rows {
            println!("{:<11} {}", label, format_zodiac_lon(lon_deg, fmt));
        }
    }

    if let Some(lp) = &computed.lunar_phase {
        println!();
        println!(
            "Lunar Phase: {}  {:.2}°  day {} of 28",
            phase_name_str(lp.phase),
            lp.synodic_arc_deg,
            lp.lunation_day
        );
    }

    if !computed.houses.is_empty() {
        for (sys, cusps) in &computed.houses {
            if let Some(c) = cusps {
                print_house_cusps(sys.label(), c, fmt)
            } else {
                println!();
                println!("{}: undefined at this latitude (circumpolar)", sys.label());
            }
        }
    }
}

fn print_house_cusps(label: &str, hc: &HouseCusps, fmt: CoordFormat) {
    println!();
    println!("{label} houses");
    println!("{}", "-".repeat(label.len() + 7));
    for n in 1_u8..=12 {
        let lon_deg = hc.cusp(n).to_degrees().rem_euclid(360.0);
        println!("H{:<2}      {}", n, format_zodiac_lon(lon_deg, fmt));
    }
}

// =============================================================================
// Page renderer (banner + 4-col placements table)
// =============================================================================
//
// Everything in this section is gated behind the `page` feature so that the
// default build doesn't pull in iocraft + crossterm + taffy + futures + regex.

/// Format a `CivilDate` as `YYYY.MM.DD` (numeric month). BCE years get a leading `-`.
#[cfg(feature = "page")]
fn page_date_str(civil: CivilDate) -> String {
    let year_part = if civil.year < 0 {
        format!("-{:04}", -civil.year)
    } else {
        format!("{:04}", civil.year)
    };
    format!("{}.{:02}.{:02}", year_part, civil.month, civil.day)
}

/// Format the banner's geographic-coords slot in deg-min as `34°N08' 118°W21'`
/// (lat first, then lon). Returns `"–"` when no observer location.
#[cfg(feature = "page")]
fn page_coords_str(observer: Option<&ObserverLocation>) -> String {
    let Some(obs) = observer else {
        return "–".to_string();
    };
    format!(
        "{} {}",
        format_geo_deg_min(obs.lat_deg, 'N', 'S', 2),
        format_geo_deg_min(obs.lon_deg, 'E', 'W', 3),
    )
}

/// `+34.13889` lat → `34°N08'`, `-118.3525` lon → `118°W21'`. Uses the same
/// minute-rounding-with-carry rule as `--dm` so e.g. 34°59'50" rounds to
/// 35°00' rather than displaying an invalid `34°60'`.
#[cfg(feature = "page")]
fn format_geo_deg_min(deg: f64, pos: char, neg: char, deg_width: usize) -> String {
    let hemi = if deg >= 0.0 { pos } else { neg };
    let mag = deg.abs();
    let total_min = (mag * 60.0).round();
    let d = (total_min / 60.0).trunc();
    let m = total_min - d * 60.0;
    format!("{d:0>deg_width$.0}°{hemi}{m:02.0}'")
}

/// Diurnal / Nocturnal from Sun + Ascendant. Returns `Some("Diurnal" | "Nocturnal")`
/// when both are known; `None` for heliocentric mode or missing Ac.
#[cfg(feature = "page")]
fn page_sect_label(computed: &ComputedChart) -> Option<&'static str> {
    match computed.sect? {
        Sect::Day => Some("Diurnal"),
        Sect::Night => Some("Nocturnal"),
    }
}

/// Compact mode-descriptor for the banner's right column.
#[cfg(feature = "page")]
fn page_mode_str(mode: &pericynthion::chart::CoordMode) -> &'static str {
    match mode {
        pericynthion::chart::CoordMode::Geocentric => "Geocentric",
        pericynthion::chart::CoordMode::Topocentric(_) => "Topocentric",
        pericynthion::chart::CoordMode::Heliocentric => "Heliocentric",
    }
}

/// Collect all chart points (house cusps + bodies + angles + lots) into a
/// flat `(label, lon_deg)` list, then sort zodiacally from `start_lon`. The
/// resulting order goes H1 → next degree → … → wrapping back through Pisces
/// → finishing just before H1.
///
/// `nodes_mode` selects which variant (mean / true) of the lunar node to place
/// in the sorted list. Lilith is not included in page placements.
#[cfg(feature = "page")]
fn page_collect_placements(
    computed: &ComputedChart,
    primary_house: Option<&HouseCusps>,
    start_lon: f64,
    nodes_mode: NodesMode,
) -> Vec<(String, f64)> {
    let mut v: Vec<(String, f64)> = Vec::new();

    if let Some(hc) = primary_house {
        for h in 1_u8..=12 {
            v.push((format!("H{h}"), hc.cusp(h).to_degrees().rem_euclid(360.0)));
        }
    }
    for cb in &computed.bodies {
        v.push((cb.body.name().to_string(), cb.position.longitude_deg));
    }
    if let Some(ang) = &computed.angles {
        if let Some(d) = ang.ac_deg {
            v.push(("Ac".into(), d));
        }
        if let Some(d) = ang.ds_deg {
            v.push(("Ds".into(), d));
        }
        v.push(("Mc".into(), ang.mc_deg));
        v.push(("Ic".into(), ang.ic_deg));
        if let Some(d) = ang.vx_deg {
            v.push(("Vx".into(), d));
        }
        if let Some(d) = ang.ax_deg {
            v.push(("Ax".into(), d));
        }
        // Nodes: use the selected mode variant
        if let Some(n) = &computed.nodes {
            let (nn, sn) = match nodes_mode {
                NodesMode::Mean => (n.mean_nn_deg, n.mean_sn_deg),
                NodesMode::True => (n.true_nn_deg, n.true_sn_deg),
            };
            v.push(("Nn".into(), nn));
            v.push(("Sn".into(), sn));
        }
    }
    if let Some(l) = &computed.lots {
        v.push(("Fortune".into(), l.fortune_deg));
        v.push(("Spirit".into(), l.spirit_deg));
        if let Some(d) = l.eros_deg {
            v.push(("Eros".into(), d));
        }
    }

    v.sort_by(|a, b| {
        let a_rel = (a.1 - start_lon).rem_euclid(360.0);
        let b_rel = (b.1 - start_lon).rem_euclid(360.0);
        a_rel
            .partial_cmp(&b_rel)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    v
}

/// Minimum gap (in spaces) between the left and right strings on any
/// banner row before width-matching kicks in.
#[cfg(feature = "page")]
const BANNER_MIN_GAP: usize = 4;

/// Astrological retrograde glyph (U+211E, "prescription").
#[cfg(feature = "page")]
const RETROGRADE_GLYPH: char = '℞';

/// Render one banner row of `inside_width` characters, with `left` content
/// flush left and `right` flush right, separated by space-fill. Wraps in
/// `│ … │` border characters.
#[cfg(feature = "page")]
fn banner_row(inside_width: usize, left: &str, right: &str) -> String {
    let used = left.chars().count() + right.chars().count();
    let pad = inside_width.saturating_sub(used);
    format!("│ {}{}{} │", left, " ".repeat(pad), right)
}

#[cfg(feature = "page")]
fn print_page(args: &ComputeArgs, computed: &ComputedChart, fmt: CoordFormat) {
    use tabled::{
        builder::Builder,
        settings::{
            panel::Panel,
            style::{HorizontalLine, Style},
        },
    };

    // === Banner content assembly ===
    let (hour, minute, second) = parse_time(&args.time).unwrap_or((0, 0, 0.0));
    let (year, month, day) = parse_date(&args.date).unwrap_or((0, 1, 1));
    let civil = CivilDate {
        year,
        month,
        day,
        hour,
        minute,
        second,
    };
    // Single combined date+time line on the banner's left column.
    let date_time_str = {
        let date = page_date_str(civil);
        let hms = format!("{:02}:{:02}", civil.hour, civil.minute);
        let tz_name = args.tz_name.as_deref();
        let tz_off = args.tz.as_deref();
        match (tz_name, tz_off) {
            (Some(name), Some(off)) => format!("{date} {hms} {name} UTC{off}"),
            (None, Some(off)) => format!("{date} {hms} UTC{off}"),
            (Some(name), None) => format!("{date} {hms} {name} LMT"),
            (None, None) => format!("{date} {hms} LMT"),
        }
    };

    let observer = if let pericynthion::chart::CoordMode::Topocentric(obs) = &computed.mode {
        Some(obs)
    } else {
        None
    };
    let coords_str = page_coords_str(observer);
    let sect_str = page_sect_label(computed).unwrap_or("–").to_string();

    let calendar_str = match args.calendar {
        CalendarArg::Julian => "Julian",
        CalendarArg::Gregorian => "Gregorian",
        CalendarArg::Auto => "Auto",
    };
    let jd_ut_str = format!("JD UT {:.4}", computed.jd_ut);
    let mode_str = page_mode_str(&computed.mode);
    let zodiac_str = "Tropical"; // only mode shipped
    let primary_house_arg = args
        .houses
        .as_ref()
        .and_then(|v| v.first().copied())
        .unwrap_or(HouseArg::Placidus);
    let primary_house_sys = primary_house_arg.to_house_system();
    let house_str = primary_house_sys.label();
    let phase_str = computed.lunar_phase.as_ref().map(|lp| {
        format!(
            "{}  {:.2}°  day {} of 28",
            phase_name_str(lp.phase),
            lp.synodic_arc_deg,
            lp.lunation_day
        )
    });

    // === Placements collection (needed before sizing) ===
    let primary_house_cusps = computed
        .houses
        .iter()
        .find(|(sys, _)| *sys == primary_house_sys)
        .and_then(|(_, c)| c.as_ref());
    let start_lon = primary_house_cusps
        .map(|hc| hc.cusp(1).to_degrees().rem_euclid(360.0))
        .or_else(|| computed.angles.as_ref().and_then(|a| a.ac_deg))
        .unwrap_or(0.0);

    let placements = page_collect_placements(computed, primary_house_cusps, start_lon, args.nodes);

    // Annotate each placement with a retrograde flag based on its label.
    // The mapping covers the ten classical bodies, lunar nodes (Nn/Sn —
    // both share the orbital-plane direction), and Black Moon Lilith /
    // Priapus (share the apsides-axis direction). Angles, house cusps,
    // and lots are never marked.
    let retro_for = |label: &str| -> bool {
        if let Some(body) = match label {
            "Sun" => Some(Body::Sun),
            "Moon" => Some(Body::Moon),
            "Mercury" => Some(Body::Mercury),
            "Venus" => Some(Body::Venus),
            "Mars" => Some(Body::Mars),
            "Jupiter" => Some(Body::Jupiter),
            "Saturn" => Some(Body::Saturn),
            "Uranus" => Some(Body::Uranus),
            "Neptune" => Some(Body::Neptune),
            "Pluto" => Some(Body::Pluto),
            "Earth" => Some(Body::Earth),
            _ => None,
        } {
            return computed
                .bodies
                .iter()
                .find(|cb| cb.body == body)
                .is_some_and(|cb| cb.retrograde);
        }
        match label {
            "Nn" | "Sn" => match args.nodes {
                NodesMode::Mean => true,
                NodesMode::True => computed.nodes.as_ref().is_some_and(|n| n.true_retrograde),
            },
            "Lil" | "Pri" => match args.lilith {
                LilithMode::Mean => false,
                LilithMode::True => computed.lilith.as_ref().is_some_and(|l| l.true_retrograde),
            },
            _ => false,
        }
    };

    let half = placements.len().div_ceil(2);

    // Per-column cell content (header + data rows). Placement columns are
    // `(name, retrograde)` so the ℞ glyph can land at the rightmost slot
    // of the column at pad time; longitude columns are plain strings.
    let headers: [&str; 4] = ["Placement", "Longitude", "Placement", "Longitude"];
    let mut placement_cells: [Vec<(String, bool)>; 2] = Default::default();
    let mut longitude_cells: [Vec<String>; 2] = Default::default();
    placement_cells[0].push((headers[0].to_string(), false));
    placement_cells[1].push((headers[2].to_string(), false));
    longitude_cells[0].push(headers[1].to_string());
    longitude_cells[1].push(headers[3].to_string());
    for i in 0..half {
        let (l_lbl, l_retro, l_lon) = placements.get(i).map_or_else(
            || (String::new(), false, String::new()),
            |(lbl, lon)| (lbl.clone(), retro_for(lbl), format_zodiac_lon(*lon, fmt)),
        );
        let (r_lbl, r_retro, r_lon) = placements.get(i + half).map_or_else(
            || (String::new(), false, String::new()),
            |(lbl, lon)| (lbl.clone(), retro_for(lbl), format_zodiac_lon(*lon, fmt)),
        );
        placement_cells[0].push((l_lbl, l_retro));
        longitude_cells[0].push(l_lon);
        placement_cells[1].push((r_lbl, r_retro));
        longitude_cells[1].push(r_lon);
    }

    // Natural column widths.
    // - Placement columns: max(name_len + 2 if retrograde else name_len) over
    //   all cells. The `+2` reserves one separating space + the ℞ glyph.
    // - Longitude columns: max chars across header + cells.
    let placement_col_w = |cells: &[(String, bool)]| -> usize {
        cells
            .iter()
            .map(|(name, retro)| name.chars().count() + if *retro { 2 } else { 0 })
            .max()
            .unwrap_or(0)
    };
    let longitude_col_w =
        |cells: &[String]| -> usize { cells.iter().map(|s| s.chars().count()).max().unwrap_or(0) };
    let mut col_widths: [usize; 4] = [
        placement_col_w(&placement_cells[0]),
        longitude_col_w(&longitude_cells[0]),
        placement_col_w(&placement_cells[1]),
        longitude_col_w(&longitude_cells[1]),
    ];
    // Table total width = sum(col_widths) + 4×2 (cell padding) + 5 (vertical
    // borders, including the outer two and three between cells).
    let table_width = |w: &[usize; 4]| w.iter().sum::<usize>() + 13;

    // The top-of-table panel row holds `JD UT … sect`. Its natural width is
    // L + R + min-gap + 4 surrounding characters (outer borders + 1 padding
    // each side) — same formula as the banner.
    let panel_natural_width =
        jd_ut_str.chars().count() + sect_str.chars().count() + BANNER_MIN_GAP + 4;

    // Banner natural width = max row's (left + right + min gap) + 4 for the
    // surrounding `│ ` … ` │`. JD UT and sect have been moved to the
    // placements table's top panel — they're considered when sizing it too.
    let mut banner_rows: Vec<(&str, &str)> = vec![
        (date_time_str.as_str(), coords_str.as_str()),
        (calendar_str, mode_str),
        (zodiac_str, house_str),
    ];
    if let Some(s) = phase_str.as_deref() {
        banner_rows.push(("Lunar Phase", s));
    }
    let banner_max_content = banner_rows
        .iter()
        .map(|(l, r)| l.chars().count() + r.chars().count() + BANNER_MIN_GAP)
        .max()
        .unwrap_or(0);
    let banner_natural_width = banner_max_content + 4;

    // Target: whichever section wants more space. Everything narrower than
    // the target expands to match.
    let target_width = table_width(&col_widths)
        .max(banner_natural_width)
        .max(panel_natural_width);

    // If table is narrower than target, distribute the slack across columns
    // (round-robin from col 0) so the table totals `target_width`.
    let mut extra = target_width.saturating_sub(table_width(&col_widths));
    let mut idx = 0;
    while extra > 0 {
        col_widths[idx % 4] += 1;
        idx += 1;
        extra -= 1;
    }

    // === Banner render (manual box drawing) ===
    let inside = target_width - 4;
    let bar = "─".repeat(target_width - 2);
    println!("╭{bar}╮");
    for (l, r) in &banner_rows {
        println!("{}", banner_row(inside, l, r));
    }
    println!("╰{bar}╯");

    // === Placements table (via tabled) ===
    //
    // Cells are pre-padded to `col_widths` so tabled just draws borders.
    // - Placement cells: name flush left, ℞ flush right with the gap padded
    //   with spaces between (always ≥ 1 space — col_w was sized to allow it).
    // - Longitude cells: right-aligned.
    let pad_placement = |name: &str, retro: bool, width: usize| -> String {
        if retro {
            let gap = width.saturating_sub(name.chars().count() + 1);
            format!("{name}{}{RETROGRADE_GLYPH}", " ".repeat(gap))
        } else {
            format!("{name:<width$}")
        }
    };
    let pad_longitude = |s: &str, width: usize| format!("{s:>width$}");
    let mut builder = Builder::default();
    let n_rows = placement_cells[0].len();
    for row in 0..n_rows {
        let (l_name, l_retro) = &placement_cells[0][row];
        let (r_name, r_retro) = &placement_cells[1][row];
        builder.push_record([
            pad_placement(l_name, *l_retro, col_widths[0]),
            pad_longitude(&longitude_cells[0][row], col_widths[1]),
            pad_placement(r_name, *r_retro, col_widths[2]),
            pad_longitude(&longitude_cells[1][row], col_widths[3]),
        ]);
    }
    let mut table = builder.build();
    // Compose the panel-header text: JD UT flush-left, sect flush-right,
    // sized to the table's inner width (= `target_width − 4`: minus the two
    // outer borders and one padding space each side).
    let panel_text = {
        let inner = target_width - 4;
        let used = jd_ut_str.chars().count() + sect_str.chars().count();
        let pad = inner.saturating_sub(used);
        format!("{}{}{}", jd_ut_str, " ".repeat(pad), sect_str)
    };
    // Style:
    // - Top border: plain `─` (no column tee marks above the panel row).
    // - Row 1 rule (under panel): also a plain `─` — the JD UT / sect line
    //   is conceptually a banner row, not a table-column row, so no `┼`s.
    // - Row 2 rule (under column headers): standard `┼` intersections,
    //   marking the start of the data grid.
    let panel_rule = HorizontalLine::full('─', '─', '├', '┤');
    let column_rule = HorizontalLine::full('─', '┼', '├', '┤');
    table.with(Panel::header(panel_text)).with(
        Style::rounded()
            .intersection_top('─')
            .horizontals([(1, panel_rule), (2, column_rule)]),
    );
    println!("{table}");
}

// =============================================================================
// Coordinate formatting (`--dd` / `--dms` / `--ddm` / `--dm`)
// =============================================================================

/// Column width for a longitude rendered as "deg-in-sign + 3-letter sign".
fn lon_col_width(fmt: CoordFormat) -> usize {
    // DD     "29.9999° Sco" = 12
    // DMS    "29°59'59\" Sco" = 13
    // DDM    "29°59.999' Sco" = 14
    // DM     "29°59' Sco" = 10
    // D      "29° Sco" = 7
    match fmt {
        CoordFormat::Dd => 12,
        CoordFormat::Dms => 13,
        CoordFormat::Ddm => 14,
        CoordFormat::Dm => 10,
        CoordFormat::D => 7,
    }
}

/// Column width for a signed ecliptic latitude.
fn lat_col_width(fmt: CoordFormat) -> usize {
    // DD     "+89.9999°" = 9
    // DMS    "+89°59'59\"" = 10
    // DDM    "+89°59.999'" = 11
    // DM     "+89°59'" = 7
    // D      "+89°" = 4
    match fmt {
        CoordFormat::Dd => 9,
        CoordFormat::Dms => 10,
        CoordFormat::Ddm => 11,
        CoordFormat::Dm => 7,
        CoordFormat::D => 4,
    }
}

/// Render a zodiac longitude (deg-in-sign + sign abbreviation), with the
/// numeric piece in the requested sexagesimal format.
///
/// `Dm` mode rounds the longitude to the nearest arcminute *before*
/// the sign split, so a value like 209°59'45" (Lib 29°59'45") rolls
/// cleanly into 0°00' Sco rather than printing 30°00' Lib.
fn format_zodiac_lon(lon_deg: f64, fmt: CoordFormat) -> String {
    let normalized = match fmt {
        // Pre-truncate to the whole-degree to ensure sign split and displayed
        // integer agree (avoids split_sign rounding up into the next sign
        // while trunc() in format_unsigned_deg still shows the lower degree).
        CoordFormat::D => lon_deg.trunc().rem_euclid(360.0),
        _ => lon_deg,
    };
    let (zsign, deg_in_sign) = split_sign(normalized);
    let body = format_unsigned_deg(deg_in_sign, fmt, /*deg_width=*/ 2);
    let w = lon_col_width(fmt) - 4; // minus " " + 3-letter sign
    format!("{body:>w$} {zsign}")
}

/// Render a signed angle (latitude ±90°, or geographic longitude ±180°)
/// with explicit `+`/`-` prefix. `deg_width` is the integer-degrees slot
/// width — 2 for latitude, 3 for geographic longitude.
fn format_signed_deg(deg: f64, fmt: CoordFormat, deg_width: usize) -> String {
    let sign_ch = if deg < 0.0 { '-' } else { '+' };
    format!(
        "{}{}",
        sign_ch,
        format_unsigned_deg(deg.abs(), fmt, deg_width)
    )
}

/// Render a signed ecliptic latitude (±90°).
fn format_signed_lat(lat_deg: f64, fmt: CoordFormat) -> String {
    format_signed_deg(lat_deg, fmt, 2)
}

/// Render an unsigned magnitude in degrees as decimal degrees,
/// degrees-minutes-seconds, or degrees + decimal-minutes.
///
/// `deg_width` pads the integer degrees slot so columns line up across
/// rows. Boundary safety: 59.9999' would carry into the next degree;
/// we let the natural formatter handle it (display rarely lands there).
fn format_unsigned_deg(deg_total: f64, fmt: CoordFormat, deg_width: usize) -> String {
    match fmt {
        CoordFormat::Dd => format!("{:>width$.4}°", deg_total, width = deg_width + 5),
        CoordFormat::Dms => {
            let d = deg_total.trunc();
            let min_f = (deg_total - d) * 60.0;
            let m = min_f.trunc();
            let s = ((min_f - m) * 60.0).trunc();
            format!("{d:>deg_width$.0}°{m:02.0}'{s:02.0}\"")
        }
        CoordFormat::Ddm => {
            let d = deg_total.trunc();
            let m_dec = (deg_total - d) * 60.0;
            format!("{d:>deg_width$.0}°{m_dec:06.3}'")
        }
        CoordFormat::Dm => {
            let total_min = (deg_total * 60.0).trunc();
            let d = (total_min / 60.0).trunc();
            let m = total_min - d * 60.0;
            format!("{d:>deg_width$.0}°{m:02.0}'")
        }
        CoordFormat::D => format!("{:>width$.0}°", deg_total.trunc(), width = deg_width),
    }
}

/// Three-letter zodiac sign abbreviation for a tropical ecliptic
/// longitude in degrees `[0, 360)`.
fn zodiac_sign(lon_deg: f64) -> &'static str {
    const SIGNS: [&str; 12] = [
        "Ari", "Tau", "Gem", "Can", "Leo", "Vir", "Lib", "Sco", "Sag", "Cap", "Aqu", "Pis",
    ];
    // rem_euclid(360) is non-negative and < 360, /30 < 12 — fits in 0..12.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let idx = (lon_deg.rem_euclid(360.0) / 30.0).floor() as usize;
    SIGNS[idx]
}

/// Display-precision (sign, degree-in-sign) split that keeps the two
/// pieces consistent. Whole-sign cusps and other points landing exactly
/// on a sign boundary survive a `to_radians`/`to_degrees` round-trip as
/// 29.999…° instead of 30.000°; raw display would print `30.0000° Ari`
/// (number rounds up, sign doesn't). We round to the display precision
/// before splitting so both pieces agree.
fn split_sign(lon_deg: f64) -> (&'static str, f64) {
    let rounded = ((lon_deg.rem_euclid(360.0)) * 1e4).round() / 1e4;
    let normalised = rounded.rem_euclid(360.0);
    (zodiac_sign(normalised), normalised.rem_euclid(30.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lon_drives_topocentric() {
        let obs = resolve_observer(Some("34.14"), Some("-118.35"))
            .unwrap()
            .unwrap();
        assert!((obs.lat_deg - 34.14).abs() < 1e-6);
        assert!((obs.lon_deg - (-118.35)).abs() < 1e-6);
    }

    #[test]
    fn lat_without_lon_errors() {
        assert!(resolve_observer(Some("34.14"), None).is_err());
    }

    #[test]
    fn dm_stays_in_sign() {
        // Lib 29°30'00" stays Lib 29°30'.
        let s = format_zodiac_lon(180.0 + 29.5, CoordFormat::Dm);
        assert!(s.trim_end().ends_with(" Lib"), "got {s:?}");
        assert!(s.contains("29°30'"), "got {s:?}");
    }

    #[test]
    fn no_lat_returns_none() {
        assert!(resolve_observer(None, None).unwrap().is_none());
    }

    #[test]
    fn zodiac_sign_known_longitudes() {
        assert_eq!(zodiac_sign(0.0), "Ari");
        assert_eq!(zodiac_sign(29.999), "Ari");
        assert_eq!(zodiac_sign(30.0), "Tau");
        assert_eq!(zodiac_sign(150.0), "Vir");
        assert_eq!(zodiac_sign(359.9), "Pis");
    }

    #[test]
    fn split_sign_snaps_to_next_sign_at_boundary() {
        // Sign + degree-in-sign must agree at display precision. A whole-sign
        // cusp at 30° comes out of to_radians/to_degrees as 29.99999... and
        // would otherwise print as "30.0000° Ari" instead of "0.0000° Tau".
        let (sign, deg) = split_sign(30.0_f64 - 1e-12);
        assert_eq!(sign, "Tau");
        assert!(deg.abs() < 1e-3, "deg in sign {deg}");
    }

    #[test]
    fn split_sign_passes_through_normal_values() {
        let (sign, deg) = split_sign(15.5);
        assert_eq!(sign, "Ari");
        assert!((deg - 15.5).abs() < 1e-9);
    }

    #[test]
    fn dms_truncates_seconds() {
        // 10°30'59.9" → 10°30'59" (not 10°31'00")
        let deg = 10.0 + 30.0 / 60.0 + 59.9 / 3600.0;
        let s = format_unsigned_deg(deg, CoordFormat::Dms, 2);
        assert!(s.contains("59\""), "expected 59\" in {s:?}");
        assert!(!s.contains("00\""), "must not carry into 00\" — got {s:?}");
    }

    #[test]
    fn dm_truncates_stays_in_sign() {
        // 29°59'45" Lib (209.9958333°) → 29°59' Lib, not 0°00' Sco
        let s = format_zodiac_lon(209.995_833_3, CoordFormat::Dm);
        assert!(s.trim_end().ends_with(" Lib"), "expected Lib, got {s:?}");
        assert!(s.contains("29°59'"), "expected 29°59', got {s:?}");
    }

    #[test]
    fn dm_truncates_at_full_circle() {
        // 29°59'45" Pis (359.99583°) → 29°59' Pis, not 0°00' Ari
        let s = format_zodiac_lon(359.99583, CoordFormat::Dm);
        assert!(s.trim_end().ends_with(" Pis"), "expected Pis, got {s:?}");
        assert!(s.contains("29°59'"), "expected 29°59', got {s:?}");
    }

    #[test]
    fn d_truncates_to_whole_degree() {
        // 10°59'59.9" → 10° (not 11°)
        let deg = 10.0 + 59.0 / 60.0 + 59.9 / 3600.0;
        let s = format_unsigned_deg(deg, CoordFormat::D, 2);
        assert!(s == "10°", "expected 10°, got {s:?}");
    }

    #[test]
    fn d_zodiac_truncates_in_sign() {
        // 29°59'45" Lib → 29° Lib
        let s = format_zodiac_lon(209.995_833_3, CoordFormat::D);
        assert!(s.trim_end().ends_with(" Lib"), "expected Lib, got {s:?}");
        assert!(s.contains("29°"), "expected 29°, got {s:?}");
    }
}
