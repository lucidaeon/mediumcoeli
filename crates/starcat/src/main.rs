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
use pericynthion::coords::acds::{ac_rad, ds_rad};
use pericynthion::coords::apparent::{
    EclipticPosition, apparent_ecliptic_position, apparent_ecliptic_position_topocentric,
    heliocentric_ecliptic_position,
};
use pericynthion::coords::mcic::{ic_rad, mc_rad};
use pericynthion::coords::nutation::nutation;
use pericynthion::coords::obliquity::mean_obliquity_rad;
use pericynthion::coords::sidereal_time::gast_rad;
use pericynthion::coords::topocentric::ObserverLocation;
use pericynthion::coords::{body_is_retrograde, signed_daily_motion};
use pericynthion::ephemeris::Ephemeris;
use pericynthion::geo::{parse_lat, parse_lon};
use pericynthion::houses::{
    HouseCusps, equal_as_rad, placidus_rad, porphyry_rad, regiomontanus_rad, whole_sign_rad,
};
use pericynthion::jpl::{discover, header::parse as parse_header, reader::EphemerisFile};
use pericynthion::lots::{
    Sect, courage_rad, eros_rad, exaltation_rad, fortune_rad, necessity_rad, nemesis_rad, sect,
    spirit_rad, victory_rad,
};
use pericynthion::time::calendar::{Calendar, CivilDate};
use pericynthion::time::delta_t::jd_ut_to_jd_tt;
use pericynthion::time::zone::{Zone, civil_to_jd_ut};
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
/// presentation order used when `--house` is omitted (all five).
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
    const ALL: &'static [Self] = &[
        Self::WholeSign,
        Self::EqualFromAsc,
        Self::Placidus,
        Self::Regiomontanus,
        Self::Porphyry,
        Self::Alcabitius,
        Self::Morinus,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::WholeSign => "Whole Sign",
            Self::EqualFromAsc => "Equal (from ASC)",
            Self::Placidus => "Placidus",
            Self::Regiomontanus => "Regiomontanus",
            Self::Porphyry => "Porphyry",
            Self::Alcabitius => "Alcabitius",
            #[cfg(feature = "noref-houses")]
            Self::Koch => "Koch",
            #[cfg(feature = "noref-houses")]
            Self::Campanus => "Campanus",
            Self::Morinus => "Morinus",
            #[cfg(feature = "noref-houses")]
            Self::Meridian => "Meridian",
            #[cfg(feature = "noref-houses")]
            Self::EqualFromMc => "Equal (from MC)",
            #[cfg(feature = "noref-houses")]
            Self::Horizontal => "Horizontal",
            #[cfg(feature = "noref-houses")]
            Self::Topocentric => "Topocentric (Polich-Page)",
            #[cfg(feature = "noref-houses")]
            Self::Krusinski => "Krusinski-Pisa-Goeldi",
            #[cfg(feature = "noref-houses")]
            Self::Sripati => "Sripati",
            #[cfg(feature = "noref-houses")]
            Self::Vehlow => "Vehlow Equal",
            #[cfg(feature = "noref-houses")]
            Self::Carter => "Carter Poli-Equatorial",
            #[cfg(feature = "noref-houses")]
            Self::PullenSd => "Pullen (Sinusoidal Delta)",
            #[cfg(feature = "noref-houses")]
            Self::PullenSr => "Pullen (Sinusoidal Ratio)",
        }
    }

    fn slug(self) -> &'static str {
        match self {
            Self::WholeSign => "whole_sign",
            Self::EqualFromAsc => "equal_asc",
            Self::Placidus => "placidus",
            Self::Regiomontanus => "regiomontanus",
            Self::Porphyry => "porphyry",
            Self::Alcabitius => "alcabitius",
            #[cfg(feature = "noref-houses")]
            Self::Koch => "koch",
            #[cfg(feature = "noref-houses")]
            Self::Campanus => "campanus",
            Self::Morinus => "morinus",
            #[cfg(feature = "noref-houses")]
            Self::Meridian => "meridian",
            #[cfg(feature = "noref-houses")]
            Self::EqualFromMc => "equal_mc",
            #[cfg(feature = "noref-houses")]
            Self::Horizontal => "horizontal",
            #[cfg(feature = "noref-houses")]
            Self::Topocentric => "topocentric",
            #[cfg(feature = "noref-houses")]
            Self::Krusinski => "krusinski",
            #[cfg(feature = "noref-houses")]
            Self::Sripati => "sripati",
            #[cfg(feature = "noref-houses")]
            Self::Vehlow => "vehlow",
            #[cfg(feature = "noref-houses")]
            Self::Carter => "carter",
            #[cfg(feature = "noref-houses")]
            Self::PullenSd => "pullen_sd",
            #[cfg(feature = "noref-houses")]
            Self::PullenSr => "pullen_sr",
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

    // UTC offset string for JZOD birth data (computed from zone or LMT longitude).
    let utc_offset_str: String = if args.lmt {
        let lon_deg = args
            .lon
            .as_deref()
            .and_then(|s| parse_lon(s).ok())
            .unwrap_or(0.0);
        let h = lon_deg / 15.0;
        let sign = if h >= 0.0 { '+' } else { '-' };
        let abs_h = h.abs();
        let hh = abs_h.floor() as u32;
        let mm = ((abs_h - hh as f64) * 60.0).round() as u32;
        format!("{sign}{hh:02}:{mm:02}")
    } else {
        args.tz.clone().unwrap_or_else(|| "+00:00".to_string())
    };

    // === Time scales ===
    let calendar: Calendar = args.calendar.into();
    let jd_ut = civil_to_jd_ut(civil, calendar, zone);
    let jd_tt = jd_ut_to_jd_tt(jd_ut);

    // === Ephemeris file ===
    let (header_path, binary_path) = resolve_jpl_paths(args.jpl_data.as_deref())?;
    let header_src = std::fs::read_to_string(&header_path)
        .with_context(|| format!("read {}", header_path.display()))?;
    let header = parse_header(&header_src).context("parse JPL ASCII header")?;
    let file = EphemerisFile::open(&binary_path, &header)
        .with_context(|| format!("open {}", binary_path.display()))?;
    let ephem = Ephemeris::new(&file, &header).context("build ephemeris facade")?;

    // === Coordinate mode ===
    let mode = if args.helio {
        CoordMode::Heliocentric
    } else if let Some(obs) = resolve_observer(args.lat.as_deref(), args.lon.as_deref())? {
        CoordMode::Topocentric(obs)
    } else {
        CoordMode::Geocentric
    };

    // === Bodies ===
    let bodies: Vec<Body> = match args.bodies.clone() {
        Some(list) => list.into_iter().map(Body::from).collect(),
        None => match mode {
            CoordMode::Heliocentric => Body::ALL_HELIOCENTRIC.to_vec(),
            _ => Body::ALL.to_vec(),
        },
    };

    // === Compute each body ===
    let mut positions: Vec<(Body, EclipticPosition)> = Vec::with_capacity(bodies.len());
    for body in bodies {
        let pos = match &mode {
            CoordMode::Geocentric => apparent_ecliptic_position(&ephem, body, jd_tt),
            CoordMode::Topocentric(obs) => {
                apparent_ecliptic_position_topocentric(&ephem, body, jd_tt, obs)
            }
            CoordMode::Heliocentric => heliocentric_ecliptic_position(&ephem, body, jd_tt),
        }
        .with_context(|| format!("compute position for {body:?}"))?;
        positions.push((body, pos));
    }

    // === Angles ===
    let angle_lon = args.lon.as_deref().and_then(|s| parse_lon(s).ok());
    let angle_lat = args.lat.as_deref().and_then(|s| parse_lat(s).ok());
    let mut angles = angle_lon.map(|lon| compute_angles(jd_tt, lon, angle_lat));
    let is_helio = matches!(mode, CoordMode::Heliocentric);

    // === Lunar nodes (geo/topo only — heliocentric omits Nn/Sn since
    // the node is defined relative to Earth's orbital plane). Riding
    // along with the angles struct; emitted only when a longitude is
    // present so the angle block has anything to attach to. ===
    if !is_helio {
        if let Some(a) = angles.as_mut() {
            let (nn, sn) = compute_nodes(jd_tt, args.nodes, &ephem)?;
            a.nn_deg = Some(nn);
            a.sn_deg = Some(sn);
            a.nodes_mode = Some(match args.nodes {
                NodesMode::Mean => "mean",
                NodesMode::True => "true",
            });
            let (lilith, priapus) = compute_lilith(jd_tt, args.lilith, &ephem)?;
            a.lilith_deg = Some(lilith);
            a.priapus_deg = Some(priapus);
            a.lilith_mode = Some(match args.lilith {
                LilithMode::Mean => "mean",
                LilithMode::True => "true",
            });
        }
    }

    // === Lots — need ASC + Sun + Moon for Fortune/Spirit/Exaltation
    // (always emitted); each downstream lot additionally requires its
    // associated planet: Eros↔Venus, Necessity↔Mercury, Courage↔Mars,
    // Victory↔Jupiter, Nemesis↔Saturn. Geo/topo only. ===
    let lots = if is_helio {
        None
    } else {
        angles.as_ref().and_then(|a| a.ac_deg).and_then(|ac_deg| {
            let sun = find_body_lon(&positions, Body::Sun);
            let moon = find_body_lon(&positions, Body::Moon);
            let mercury = find_body_lon(&positions, Body::Mercury);
            let venus = find_body_lon(&positions, Body::Venus);
            let mars = find_body_lon(&positions, Body::Mars);
            let jupiter = find_body_lon(&positions, Body::Jupiter);
            let saturn = find_body_lon(&positions, Body::Saturn);
            sun.zip(moon)
                .map(|(s, m)| compute_lots(ac_deg, s, m, mercury, venus, mars, jupiter, saturn))
        })
    };

    // === Lunar phase (geo/topo only; requires both Sun and Moon in body list) ===
    let lunar_phase = if is_helio {
        None
    } else {
        find_body_lon(&positions, Body::Sun)
            .zip(find_body_lon(&positions, Body::Moon))
            .map(|(sun, moon)| pericynthion::coords::phase::lunar_phase(moon, sun))
    };

    let is_jzod = !args.text && !args.page;

    // === Houses — JZOD (default) computes all systems; human renderers use --house filter ===
    let house_systems: Vec<HouseArg> = if is_jzod {
        HouseArg::ALL.to_vec()
    } else {
        args.houses
            .clone()
            .unwrap_or_else(|| HouseArg::ALL.to_vec())
    };
    let houses = if is_helio || house_systems.is_empty() {
        None
    } else {
        match (angle_lon, angle_lat) {
            (Some(lon), Some(lat)) => {
                use std::f64::consts::TAU;
                let ramc = (gast_rad(jd_tt) + lon.to_radians()).rem_euclid(TAU);
                let nut = nutation(jd_tt);
                let obliquity = mean_obliquity_rad(jd_tt) + nut.delta_epsilon;
                let lat_rad = lat.to_radians();
                pericynthion::coords::acds::ac_rad(ramc, obliquity, lat_rad)
                    .map(|ac| compute_houses(ramc, obliquity, ac, lat_rad, &house_systems))
            }
            _ => None,
        }
    };

    // === Output ===
    if is_jzod {
        let obs_lat = args.lat.as_deref().and_then(|s| parse_lat(s).ok());
        let obs_lon = args.lon.as_deref().and_then(|s| parse_lon(s).ok());
        print_jzod(
            jd_ut,
            jd_tt,
            &mode,
            &positions,
            angles.as_ref(),
            lots.as_ref(),
            houses.as_ref(),
            lunar_phase.as_ref(),
            &ephem,
            year,
            month,
            day,
            hour,
            minute,
            second,
            &utc_offset_str,
            obs_lat,
            obs_lon,
        )?;
    } else if args.page {
        #[cfg(feature = "page")]
        {
            if house_systems.len() != 1 {
                bail!(
                    "page rendering requires exactly one --house system; got {} ({:?}). \
                     Specify e.g. --house placidus or --house whole-sign.",
                    house_systems.len(),
                    house_systems
                );
            }
            print_page(
                &args,
                jd_ut,
                jd_tt,
                &mode,
                &positions,
                angles.as_ref(),
                lots.as_ref(),
                houses.as_ref(),
                lunar_phase.as_ref(),
                fmt,
                &ephem,
            );
        }
    } else {
        print_text(
            jd_ut,
            jd_tt,
            &mode,
            &positions,
            angles.as_ref(),
            lots.as_ref(),
            houses.as_ref(),
            lunar_phase.as_ref(),
            fmt,
        );
    }
    Ok(())
}

fn find_body_lon(positions: &[(Body, EclipticPosition)], body: Body) -> Option<f64> {
    positions
        .iter()
        .find(|(b, _)| *b == body)
        .map(|(_, p)| p.longitude_deg)
}

struct Angles {
    mc_deg: f64,
    ic_deg: f64,
    ac_deg: Option<f64>,
    ds_deg: Option<f64>,
    /// Vertex (western prime-vertical / ecliptic intersection). Requires
    /// latitude; degenerate at equator and poles.
    vx_deg: Option<f64>,
    /// Anti-Vertex = Vx + 180°. Same nullability as `vx_deg`.
    ax_deg: Option<f64>,
    /// North Node of the Moon's orbit. Mode controlled by `--nodes`.
    /// `None` for heliocentric mode (geocentric construct only).
    nn_deg: Option<f64>,
    /// South Node = Nn + 180°. Same nullability as `nn_deg`.
    sn_deg: Option<f64>,
    /// "mean" or "true" — which Nn computation was used. `None` when
    /// nodes are not emitted (heliocentric mode).
    nodes_mode: Option<&'static str>,
    /// Black Moon Lilith (lunar apogee). Mode controlled by `--lilith`.
    lilith_deg: Option<f64>,
    /// Priapus (lunar perigee) = Lilith + 180°.
    priapus_deg: Option<f64>,
    /// "mean" or "true" — which Lilith computation was used.
    lilith_mode: Option<&'static str>,
}

struct Lots {
    sect: Sect,
    fortune_deg: f64,
    spirit_deg: f64,
    exaltation_deg: f64,
    eros_deg: Option<f64>,
    necessity_deg: Option<f64>,
    courage_deg: Option<f64>,
    victory_deg: Option<f64>,
    nemesis_deg: Option<f64>,
}

/// House cusps in requested-system order. Each entry's `Option` distinguishes
/// "requested and computed" (`Some`) from "requested but undefined here"
/// (`None`) — typically Placidus at circumpolar latitudes.
type Houses = Vec<(HouseArg, Option<HouseCusps>)>;

#[derive(Debug)]
enum CoordMode {
    Geocentric,
    Topocentric(ObserverLocation),
    Heliocentric,
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

#[allow(clippy::too_many_arguments)]
fn compute_lots(
    ac_deg: f64,
    sun_deg: f64,
    moon_deg: f64,
    mercury_deg: Option<f64>,
    venus_deg: Option<f64>,
    mars_deg: Option<f64>,
    jupiter_deg: Option<f64>,
    saturn_deg: Option<f64>,
) -> Lots {
    let ac = ac_deg.to_radians();
    let sun = sun_deg.to_radians();
    let moon = moon_deg.to_radians();
    let s = sect(sun, ac);
    let deg = |r: f64| r.to_degrees().rem_euclid(360.0);
    let f = deg(fortune_rad(ac, sun, moon, s));
    let sp = deg(spirit_rad(ac, sun, moon, s));
    let ex = deg(exaltation_rad(ac, sun, moon, s));
    let er = venus_deg.map(|v| deg(eros_rad(ac, sun, moon, v.to_radians(), s)));
    let nec = mercury_deg.map(|m| deg(necessity_rad(ac, sun, moon, m.to_radians(), s)));
    let cou = mars_deg.map(|m| deg(courage_rad(ac, sun, moon, m.to_radians(), s)));
    let vic = jupiter_deg.map(|j| deg(victory_rad(ac, sun, moon, j.to_radians(), s)));
    let nem = saturn_deg.map(|sa| deg(nemesis_rad(ac, sun, moon, sa.to_radians(), s)));
    Lots {
        sect: s,
        fortune_deg: f,
        spirit_deg: sp,
        exaltation_deg: ex,
        eros_deg: er,
        necessity_deg: nec,
        courage_deg: cou,
        victory_deg: vic,
        nemesis_deg: nem,
    }
}

fn compute_houses(
    ramc_rad: f64,
    obliquity_rad: f64,
    ac_rad: f64,
    lat_rad: f64,
    systems: &[HouseArg],
) -> Houses {
    systems
        .iter()
        .copied()
        .map(|sys| {
            let cusps = match sys {
                HouseArg::WholeSign => Some(whole_sign_rad(ac_rad)),
                HouseArg::EqualFromAsc => Some(equal_as_rad(ac_rad)),
                HouseArg::Placidus => placidus_rad(ramc_rad, obliquity_rad, lat_rad),
                HouseArg::Regiomontanus => regiomontanus_rad(ramc_rad, obliquity_rad, lat_rad),
                HouseArg::Porphyry => Some(porphyry_rad(ac_rad, mc_rad(ramc_rad, obliquity_rad))),
                HouseArg::Alcabitius => {
                    pericynthion::houses::alcabitius_rad(ramc_rad, obliquity_rad, lat_rad)
                }
                #[cfg(feature = "noref-houses")]
                HouseArg::Koch => pericynthion::houses::koch_rad(ramc_rad, obliquity_rad, lat_rad),
                #[cfg(feature = "noref-houses")]
                HouseArg::Campanus => {
                    pericynthion::houses::campanus_rad(ramc_rad, obliquity_rad, lat_rad)
                }
                HouseArg::Morinus => {
                    pericynthion::houses::morinus_rad(ramc_rad, obliquity_rad, lat_rad)
                }
                #[cfg(feature = "noref-houses")]
                HouseArg::Meridian => {
                    pericynthion::houses::meridian_rad(ramc_rad, obliquity_rad, lat_rad)
                }
                #[cfg(feature = "noref-houses")]
                HouseArg::EqualFromMc => Some(pericynthion::houses::equal_mc_rad(
                    pericynthion::coords::mcic::mc_rad(ramc_rad, obliquity_rad),
                )),
                #[cfg(feature = "noref-houses")]
                HouseArg::Horizontal => {
                    pericynthion::houses::horizontal_rad(ramc_rad, obliquity_rad, lat_rad)
                }
                #[cfg(feature = "noref-houses")]
                HouseArg::Topocentric => {
                    pericynthion::houses::topocentric_rad(ramc_rad, obliquity_rad, lat_rad)
                }
                #[cfg(feature = "noref-houses")]
                HouseArg::Krusinski => {
                    pericynthion::houses::krusinski_rad(ramc_rad, obliquity_rad, lat_rad)
                }
                #[cfg(feature = "noref-houses")]
                HouseArg::Sripati => Some(pericynthion::houses::sripati_rad(
                    ac_rad,
                    pericynthion::coords::mcic::mc_rad(ramc_rad, obliquity_rad),
                )),
                #[cfg(feature = "noref-houses")]
                HouseArg::Vehlow => Some(pericynthion::houses::vehlow_rad(ac_rad)),
                #[cfg(feature = "noref-houses")]
                HouseArg::Carter => Some(pericynthion::houses::carter_rad(ac_rad, obliquity_rad)),
                #[cfg(feature = "noref-houses")]
                HouseArg::PullenSd => Some(pericynthion::houses::pullen_sd_rad(
                    ac_rad,
                    pericynthion::coords::mcic::mc_rad(ramc_rad, obliquity_rad),
                )),
                #[cfg(feature = "noref-houses")]
                HouseArg::PullenSr => Some(pericynthion::houses::pullen_sr_rad(
                    ac_rad,
                    pericynthion::coords::mcic::mc_rad(ramc_rad, obliquity_rad),
                )),
            };
            (sys, cusps)
        })
        .collect()
}

fn compute_angles(jd_tt: f64, lon_east_deg: f64, lat_deg: Option<f64>) -> Angles {
    use pericynthion::coords::vxax::{ax_rad, vx_rad};
    use std::f64::consts::TAU;
    let ramc = (gast_rad(jd_tt) + lon_east_deg.to_radians()).rem_euclid(TAU);
    let nut = nutation(jd_tt);
    let obliquity = mean_obliquity_rad(jd_tt) + nut.delta_epsilon;
    let mc = mc_rad(ramc, obliquity);
    let ic = ic_rad(mc);
    let ac = lat_deg.and_then(|lat| ac_rad(ramc, obliquity, lat.to_radians()));
    let ds = ac.map(ds_rad);
    let vx = lat_deg.and_then(|lat| vx_rad(ramc, obliquity, lat.to_radians()));
    let ax = vx.map(ax_rad);
    Angles {
        mc_deg: mc.to_degrees(),
        ic_deg: ic.to_degrees(),
        ac_deg: ac.map(f64::to_degrees),
        ds_deg: ds.map(f64::to_degrees),
        vx_deg: vx.map(f64::to_degrees),
        ax_deg: ax.map(f64::to_degrees),
        // Nodes and Lilith are filled in by the caller (need Ephemeris).
        nn_deg: None,
        sn_deg: None,
        nodes_mode: None,
        lilith_deg: None,
        priapus_deg: None,
        lilith_mode: None,
    }
}

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

/// Compute Black Moon Lilith in the selected mode, return (Lilith, Priapus)
/// in degrees.
fn compute_lilith(jd_tt: f64, mode: LilithMode, ephem: &Ephemeris) -> Result<(f64, f64)> {
    use pericynthion::coords::lilith::{mean_lilith_rad, priapus_rad, true_lilith_rad};
    let lilith = match mode {
        LilithMode::Mean => mean_lilith_rad(jd_tt),
        LilithMode::True => {
            true_lilith_rad(ephem, jd_tt).context("computing true Lilith from Moon state")?
        }
    };
    let priapus = priapus_rad(lilith);
    Ok((lilith.to_degrees(), priapus.to_degrees()))
}

/// Compute the lunar north node in the selected mode, return (Nn, Sn) in
/// degrees. Geocentric only — caller must not invoke for heliocentric mode.
fn compute_nodes(jd_tt: f64, mode: NodesMode, ephem: &Ephemeris) -> Result<(f64, f64)> {
    use pericynthion::coords::nodes::{mean_nn_rad, sn_rad, true_nn_rad};
    let nn = match mode {
        NodesMode::Mean => mean_nn_rad(jd_tt),
        NodesMode::True => {
            true_nn_rad(ephem, jd_tt).context("computing true lunar node from Moon state")?
        }
    };
    let sn = sn_rad(nn);
    Ok((nn.to_degrees(), sn.to_degrees()))
}

#[allow(clippy::too_many_arguments)]
fn print_text(
    jd_ut: f64,
    jd_tt: f64,
    mode: &CoordMode,
    positions: &[(Body, EclipticPosition)],
    angles: Option<&Angles>,
    lots: Option<&Lots>,
    houses: Option<&Houses>,
    lunar_phase: Option<&pericynthion::coords::phase::LunarPhase>,
    fmt: CoordFormat,
) {
    println!("JD UT  : {jd_ut:.6}");
    println!("JD TT  : {jd_tt:.6}");
    let coord_label = match mode {
        CoordMode::Geocentric => "geocentric".to_string(),
        CoordMode::Topocentric(obs) => {
            format!(
                "topocentric (lat={} lon={})",
                format_signed_deg(obs.lat_deg, fmt, 2),
                format_signed_deg(obs.lon_deg, fmt, 3),
            )
        }
        CoordMode::Heliocentric => "heliocentric".to_string(),
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
    for &(body, pos) in positions {
        println!(
            "{:<8} {} {} {:>14.6}",
            body.name(),
            format_zodiac_lon(pos.longitude_deg, fmt),
            format_signed_lat(pos.latitude_deg, fmt),
            pos.distance_au
        );
    }

    if let Some(ang) = angles {
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
            ("Nn", ang.nn_deg),
            ("Sn", ang.sn_deg),
            ("Lil", ang.lilith_deg),
            ("Pri", ang.priapus_deg),
        ] {
            if let Some(lon_deg) = lon {
                println!("{:<8} {}", label, format_zodiac_lon(lon_deg, fmt));
            }
        }
    }

    if let Some(l) = lots {
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

    if let Some(lp) = lunar_phase {
        println!();
        println!(
            "Lunar Phase: {}  {:.2}°  day {} of 28",
            phase_name_str(lp.phase),
            lp.synodic_arc_deg,
            lp.lunation_day
        );
    }

    if let Some(h) = houses {
        for (sys, cusps) in h {
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
fn page_sect_label(
    positions: &[(Body, EclipticPosition)],
    angles: Option<&Angles>,
) -> Option<&'static str> {
    let sun_lon = find_body_lon(positions, Body::Sun)?;
    let ac_deg = angles?.ac_deg?;
    match sect(sun_lon.to_radians(), ac_deg.to_radians()) {
        Sect::Day => Some("Diurnal"),
        Sect::Night => Some("Nocturnal"),
    }
}

/// Compact mode-descriptor for the banner's right column.
#[cfg(feature = "page")]
fn page_mode_str(mode: &CoordMode) -> &'static str {
    match mode {
        CoordMode::Geocentric => "Geocentric",
        CoordMode::Topocentric(_) => "Topocentric",
        CoordMode::Heliocentric => "Heliocentric",
    }
}

/// Collect all chart points (house cusps + bodies + angles + lots) into a
/// flat `(label, lon_deg)` list, then sort zodiacally from `start_lon`. The
/// resulting order goes H1 → next degree → … → wrapping back through Pisces
/// → finishing just before H1.
#[cfg(feature = "page")]
fn page_collect_placements(
    positions: &[(Body, EclipticPosition)],
    angles: Option<&Angles>,
    lots: Option<&Lots>,
    primary_house: Option<&HouseCusps>,
    start_lon: f64,
) -> Vec<(String, f64)> {
    let mut v: Vec<(String, f64)> = Vec::new();

    if let Some(hc) = primary_house {
        for h in 1_u8..=12 {
            v.push((format!("H{h}"), hc.cusp(h).to_degrees().rem_euclid(360.0)));
        }
    }
    for (body, pos) in positions {
        v.push((body.name().to_string(), pos.longitude_deg));
    }
    if let Some(ang) = angles {
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
        if let Some(d) = ang.nn_deg {
            v.push(("Nn".into(), d));
        }
        if let Some(d) = ang.sn_deg {
            v.push(("Sn".into(), d));
        }
    }
    if let Some(l) = lots {
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
#[allow(clippy::too_many_arguments)]
fn print_page(
    args: &ComputeArgs,
    jd_ut: f64,
    jd_tt: f64,
    mode: &CoordMode,
    positions: &[(Body, EclipticPosition)],
    angles: Option<&Angles>,
    lots: Option<&Lots>,
    houses: Option<&Houses>,
    lunar_phase: Option<&pericynthion::coords::phase::LunarPhase>,
    fmt: CoordFormat,
    ephem: &Ephemeris,
) {
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

    let observer = if let CoordMode::Topocentric(obs) = mode {
        Some(obs)
    } else {
        None
    };
    let coords_str = page_coords_str(observer);
    let sect_str = page_sect_label(positions, angles)
        .unwrap_or("–")
        .to_string();

    let calendar_str = match args.calendar {
        CalendarArg::Julian => "Julian",
        CalendarArg::Gregorian => "Gregorian",
        CalendarArg::Auto => "Auto",
    };
    let jd_ut_str = format!("JD UT {jd_ut:.4}");
    let mode_str = page_mode_str(mode);
    let zodiac_str = "Tropical"; // only mode shipped
    let primary_house_arg = args
        .houses
        .as_ref()
        .and_then(|v| v.first().copied())
        .unwrap_or(HouseArg::Placidus);
    let house_str = primary_house_arg.label();
    let phase_str = lunar_phase.map(|lp| {
        format!(
            "{}  {:.2}°  day {} of 28",
            phase_name_str(lp.phase),
            lp.synodic_arc_deg,
            lp.lunation_day
        )
    });

    // === Placements collection (needed before sizing) ===
    let primary_house_cusps = houses
        .and_then(|hs| hs.iter().find(|(sys, _)| *sys == primary_house_arg))
        .and_then(|(_, c)| c.as_ref());
    let start_lon = primary_house_cusps
        .map(|hc| hc.cusp(1).to_degrees().rem_euclid(360.0))
        .or_else(|| angles.and_then(|a| a.ac_deg))
        .unwrap_or(0.0);

    let placements =
        page_collect_placements(positions, angles, lots, primary_house_cusps, start_lon);

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
            return body_is_retrograde(ephem, body, jd_tt, matches!(mode, CoordMode::Heliocentric));
        }
        match label {
            "Nn" | "Sn" => match args.nodes {
                NodesMode::Mean => true,
                NodesMode::True => pericynthion::coords::nodes::true_nn_is_retrograde(ephem, jd_tt)
                    .unwrap_or(false),
            },
            "Lil" | "Pri" => match args.lilith {
                LilithMode::Mean => false,
                LilithMode::True => {
                    pericynthion::coords::lilith::true_lilith_is_retrograde(ephem, jd_tt)
                        .unwrap_or(false)
                }
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

// =============================================================================
// JZOD output
// =============================================================================

fn body_to_jzod_id(body: Body) -> jzod::BodyId {
    match body {
        Body::Sun => jzod::BodyId::Sun,
        Body::Moon => jzod::BodyId::Moon,
        Body::Mercury => jzod::BodyId::Mercury,
        Body::Venus => jzod::BodyId::Venus,
        Body::Earth => jzod::BodyId::Earth,
        Body::Mars => jzod::BodyId::Mars,
        Body::Jupiter => jzod::BodyId::Jupiter,
        Body::Saturn => jzod::BodyId::Saturn,
        Body::Uranus => jzod::BodyId::Uranus,
        Body::Neptune => jzod::BodyId::Neptune,
        Body::Pluto => jzod::BodyId::Pluto,
        Body::EarthMoonBarycenter => jzod::BodyId::EarthMoonBarycenter,
    }
}

/// Return the 1-based house number a longitude falls in given house cusps (radians).
fn jzod_house_for(lon_deg: f64, cusps: &HouseCusps) -> u8 {
    let lon = lon_deg.rem_euclid(360.0);
    for h in 1u8..=12 {
        let next = if h == 12 { 1 } else { h + 1 };
        let start = cusps.cusp(h).to_degrees().rem_euclid(360.0);
        let end = cusps.cusp(next).to_degrees().rem_euclid(360.0);
        let contains = if end > start {
            lon >= start && lon < end
        } else {
            lon >= start || lon < end
        };
        if contains {
            return h;
        }
    }
    1
}

#[allow(clippy::too_many_arguments)]
fn print_jzod(
    jd_ut: f64,
    jd_tt: f64,
    mode: &CoordMode,
    positions: &[(Body, EclipticPosition)],
    angles: Option<&Angles>,
    lots: Option<&Lots>,
    houses: Option<&Houses>,
    lunar_phase: Option<&pericynthion::coords::phase::LunarPhase>,
    ephem: &Ephemeris,
    year: i32,
    month: u8,
    day: u8,
    hour: u8,
    minute: u8,
    second: f64,
    utc_offset: &str,
    lat: Option<f64>,
    lon: Option<f64>,
) -> Result<()> {
    use pericynthion::coords::lilith::{mean_lilith_rad, priapus_rad, true_lilith_rad};
    use pericynthion::coords::nodes::{mean_nn_rad, sn_rad, true_nn_rad};

    let is_helio = matches!(mode, CoordMode::Heliocentric);
    let coord_system = match mode {
        CoordMode::Geocentric => jzod::CoordinateSystem::Geocentric,
        CoordMode::Topocentric(_) => jzod::CoordinateSystem::Topocentric,
        CoordMode::Heliocentric => jzod::CoordinateSystem::Heliocentric,
    };

    // Sect from Sun above/below horizon.
    let sun_lon = positions
        .iter()
        .find(|(b, _)| *b == Body::Sun)
        .map(|(_, p)| p.longitude_deg);
    let jzod_sect = angles.and_then(|a| a.ac_deg).zip(sun_lon).map(|(ac, s)| {
        match sect(s.to_radians(), ac.to_radians()) {
            Sect::Day => jzod::Sect::Diurnal,
            Sect::Night => jzod::Sect::Nocturnal,
        }
    });

    // Both node variants — geo/topo only, requires ASC (i.e. angles present).
    let has_points = !is_helio && angles.and_then(|a| a.ac_deg).is_some();
    let (nn_mean, sn_mean, nn_true, sn_true, nn_true_retro) = if has_points {
        let m = mean_nn_rad(jd_tt);
        let t = true_nn_rad(ephem, jd_tt).context("computing true north node")?;
        let t_retro = pericynthion::coords::nodes::true_nn_is_retrograde(ephem, jd_tt)
            .context("computing true node retrograde")?;
        (
            Some(m.to_degrees()),
            Some(sn_rad(m).to_degrees()),
            Some(t.to_degrees()),
            Some(sn_rad(t).to_degrees()),
            Some(t_retro),
        )
    } else {
        (None, None, None, None, None)
    };

    // Both BML variants — same gating as nodes.
    let (lil_mean, pri_mean, lil_true, pri_true, lil_true_retro) = if has_points {
        let m = mean_lilith_rad(jd_tt);
        let t = true_lilith_rad(ephem, jd_tt).context("computing true Black Moon Lilith")?;
        let t_retro = pericynthion::coords::lilith::true_lilith_is_retrograde(ephem, jd_tt)
            .context("computing true Lilith retrograde")?;
        (
            Some(m.to_degrees()),
            Some(priapus_rad(m).to_degrees()),
            Some(t.to_degrees()),
            Some(priapus_rad(t).to_degrees()),
            Some(t_retro),
        )
    } else {
        (None, None, None, None, None)
    };

    // calculated_at timestamp.
    let calculated_at = jzod::time::calculated_at_now();

    // House assignments for a longitude across all computed systems.
    let body_houses = |lon_deg: f64| -> std::collections::BTreeMap<String, u8> {
        let mut map = std::collections::BTreeMap::new();
        if let Some(hs) = houses {
            for (sys, cusps) in hs {
                if let Some(c) = cusps {
                    map.insert(sys.slug().to_string(), jzod_house_for(lon_deg, c));
                }
            }
        }
        map
    };

    // Bodies array.
    let bodies: Vec<jzod::placement::Body> = positions
        .iter()
        .map(|&(body, pos)| {
            let lon_at = |jd: f64| match mode {
                CoordMode::Heliocentric => heliocentric_ecliptic_position(ephem, body, jd)
                    .map_or(pos.longitude_deg, |p| p.longitude_deg),
                _ => apparent_ecliptic_position(ephem, body, jd)
                    .map_or(pos.longitude_deg, |p| p.longitude_deg),
            };
            let daily_speed = signed_daily_motion(lon_at(jd_tt - 0.5), lon_at(jd_tt + 0.5));
            let retrograde = body_is_retrograde(ephem, body, jd_tt, is_helio);
            jzod::placement::Body {
                id: body_to_jzod_id(body),
                position: jzod::coord::Position::from_longitude(pos.longitude_deg),
                ecliptic_latitude: jzod::coord::Degrees8(pos.latitude_deg),
                daily_speed: jzod::coord::Degrees8(daily_speed),
                retrograde,
                distance_au: Some(pos.distance_au),
                house: body_houses(pos.longitude_deg),
            }
        })
        .collect();

    // Angles array (ASC, DSC, MC, IC — in that order when present).
    let mut angles_vec: Vec<jzod::Angle> = Vec::new();
    if let Some(a) = angles {
        if let Some(ac) = a.ac_deg {
            angles_vec.push(jzod::Angle {
                id: jzod::AngleId::Ascendant,
                position: jzod::coord::Position::from_longitude(ac),
            });
        }
        if let Some(ds) = a.ds_deg {
            angles_vec.push(jzod::Angle {
                id: jzod::AngleId::Descendant,
                position: jzod::coord::Position::from_longitude(ds),
            });
        }
        angles_vec.push(jzod::Angle {
            id: jzod::AngleId::Midheaven,
            position: jzod::coord::Position::from_longitude(a.mc_deg),
        });
        angles_vec.push(jzod::Angle {
            id: jzod::AngleId::ImumCoeli,
            position: jzod::coord::Position::from_longitude(a.ic_deg),
        });
    }

    // Points array: vertex axis, then nodes (mean, true), then BML (mean, true).
    // Variant schema: Option A — suffixed IDs (resolves OQ-19).
    let mut points_vec: Vec<jzod::Point> = Vec::new();
    if let Some(a) = angles {
        if let Some(vx) = a.vx_deg {
            points_vec.push(jzod::Point {
                id: jzod::PointId::Vertex,
                position: jzod::coord::Position::from_longitude(vx),
                retrograde: false,
            });
        }
        if let Some(ax) = a.ax_deg {
            points_vec.push(jzod::Point {
                id: jzod::PointId::AntiVertex,
                position: jzod::coord::Position::from_longitude(ax),
                retrograde: false,
            });
        }
    }
    if let (Some(nm), Some(sm)) = (nn_mean, sn_mean) {
        points_vec.push(jzod::Point {
            id: jzod::PointId::NorthNodeMean,
            position: jzod::coord::Position::from_longitude(nm),
            retrograde: true,
        });
        points_vec.push(jzod::Point {
            id: jzod::PointId::SouthNodeMean,
            position: jzod::coord::Position::from_longitude(sm),
            retrograde: true,
        });
    }
    if let (Some(nt), Some(st), Some(tr)) = (nn_true, sn_true, nn_true_retro) {
        points_vec.push(jzod::Point {
            id: jzod::PointId::NorthNodeTrue,
            position: jzod::coord::Position::from_longitude(nt),
            retrograde: tr,
        });
        points_vec.push(jzod::Point {
            id: jzod::PointId::SouthNodeTrue,
            position: jzod::coord::Position::from_longitude(st),
            retrograde: tr,
        });
    }
    if let (Some(lm), Some(pm)) = (lil_mean, pri_mean) {
        points_vec.push(jzod::Point {
            id: jzod::PointId::BlackMoonLilithMean,
            position: jzod::coord::Position::from_longitude(lm),
            retrograde: false,
        });
        points_vec.push(jzod::Point {
            id: jzod::PointId::PriapusMean,
            position: jzod::coord::Position::from_longitude(pm),
            retrograde: false,
        });
    }
    if let (Some(lt), Some(pt), Some(lr)) = (lil_true, pri_true, lil_true_retro) {
        points_vec.push(jzod::Point {
            id: jzod::PointId::BlackMoonLilithTrue,
            position: jzod::coord::Position::from_longitude(lt),
            retrograde: lr,
        });
        points_vec.push(jzod::Point {
            id: jzod::PointId::PriapusTrue,
            position: jzod::coord::Position::from_longitude(pt),
            retrograde: lr,
        });
    }

    // Lots array.
    let mut lots_vec: Vec<jzod::Lot> = Vec::new();
    if let Some(l) = lots {
        lots_vec.push(jzod::Lot {
            id: jzod::LotId::LotOfFortune,
            position: jzod::coord::Position::from_longitude(l.fortune_deg),
        });
        lots_vec.push(jzod::Lot {
            id: jzod::LotId::LotOfSpirit,
            position: jzod::coord::Position::from_longitude(l.spirit_deg),
        });
        lots_vec.push(jzod::Lot {
            id: jzod::LotId::LotOfExaltation,
            position: jzod::coord::Position::from_longitude(l.exaltation_deg),
        });
        if let Some(d) = l.necessity_deg {
            lots_vec.push(jzod::Lot {
                id: jzod::LotId::LotOfNecessity,
                position: jzod::coord::Position::from_longitude(d),
            });
        }
        if let Some(d) = l.eros_deg {
            lots_vec.push(jzod::Lot {
                id: jzod::LotId::LotOfEros,
                position: jzod::coord::Position::from_longitude(d),
            });
        }
        if let Some(d) = l.courage_deg {
            lots_vec.push(jzod::Lot {
                id: jzod::LotId::LotOfCourage,
                position: jzod::coord::Position::from_longitude(d),
            });
        }
        if let Some(d) = l.victory_deg {
            lots_vec.push(jzod::Lot {
                id: jzod::LotId::LotOfVictory,
                position: jzod::coord::Position::from_longitude(d),
            });
        }
        if let Some(d) = l.nemesis_deg {
            lots_vec.push(jzod::Lot {
                id: jzod::LotId::LotOfNemesis,
                position: jzod::coord::Position::from_longitude(d),
            });
        }
    }

    // Houses keyed by system slug, using typed HouseCusp.
    let mut jzod_houses: jzod::Houses = jzod::Houses::new();
    if let Some(hs) = houses {
        for (sys, cusps) in hs {
            if let Some(c) = cusps {
                let mut system_cusps: jzod::HouseSystemCusps = jzod::HouseSystemCusps::new();
                for h in 1u8..=12 {
                    let lon_deg = c.cusp(h).to_degrees().rem_euclid(360.0);
                    let cusp = if *sys == HouseArg::WholeSign {
                        jzod::HouseCusp::whole_sign_from_longitude(lon_deg)
                    } else {
                        jzod::HouseCusp::from_longitude(lon_deg)
                    };
                    system_cusps.insert(h, cusp);
                }
                jzod_houses.insert(sys.slug().to_string(), system_cusps);
            }
        }
    }

    // Build the typed chart.
    let chart = jzod::Chart {
        uid: uuid::Uuid::new_v4().to_string(),
        chart_type: jzod::ChartType::Radix,
        name: None,
        gender: None,
        rodden_rating: None,
        birth: jzod::Birth {
            datetime: jzod::Datetime {
                year,
                month,
                day,
                hour,
                minute,
                second: second.floor() as u8,
                utc_offset: utc_offset.to_string(),
                iana_tz: None,
                unknown: false,
                tod_method: None,
            },
            location: jzod::Location {
                name: None,
                latitude: lat,
                longitude: lon,
            },
        },
        zodiac: jzod::Zodiac::Tropical,
        coordinate_system: coord_system,
        sect: jzod_sect,
        ephemeris: jzod::Ephemeris {
            source: "DE441".to_string(),
            calculated_at,
            jd_ut: Some(jd_ut),
            jd_tt: Some(jd_tt),
        },
        placements: jzod::Placements {
            bodies,
            angles: angles_vec,
            points: points_vec,
            lots: lots_vec,
        },
        houses: jzod_houses,
        lunar_phase: lunar_phase.map(|lp| {
            use pericynthion::coords::phase::LunarPhaseName as P;
            jzod::LunarPhase {
                synodic_arc_deg: lp.synodic_arc_deg,
                phase: match lp.phase {
                    P::NewMoon => jzod::LunarPhaseName::NewMoon,
                    P::Crescent => jzod::LunarPhaseName::Crescent,
                    P::FirstQuarter => jzod::LunarPhaseName::FirstQuarter,
                    P::Gibbous => jzod::LunarPhaseName::Gibbous,
                    P::FullMoon => jzod::LunarPhaseName::FullMoon,
                    P::Disseminating => jzod::LunarPhaseName::Disseminating,
                    P::LastQuarter => jzod::LunarPhaseName::LastQuarter,
                    P::Balsamic => jzod::LunarPhaseName::Balsamic,
                },
                lunation_day: lp.lunation_day,
            }
        }),
        nested: vec![],
    };

    let doc = jzod::JzodDocument::new(vec![chart]);
    println!("{}", jzod::to_string_pretty(&doc));
    Ok(())
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
    fn compute_angles_leo_asc_mc() {
        // 1955-11-13 06:04 UT, Universal City CA. Refchart resolved coords:
        // 34°N08'20" = 34.1389° lat, 118°W21'09" = -118.3525° lon.
        // Ar⌖26°07'43" = 26.129° MC, Le⌖05°19'30" = 125.325° Asc.
        use pericynthion::time::delta_t::jd_ut_to_jd_tt;
        let jd_tt = jd_ut_to_jd_tt(2_435_424.752_8);
        let ang = compute_angles(jd_tt, -118.352_500, Some(34.138_889));
        let mc_expected = 0.0 + 26.0 + 7.0 / 60.0 + 43.0 / 3600.0;
        let as_expected = 120.0 + 5.0 + 19.0 / 60.0 + 30.0 / 3600.0;
        assert!(
            (ang.mc_deg - mc_expected).abs() < 5.0 / 60.0,
            "Mc {:.4} expected {:.4}",
            ang.mc_deg,
            mc_expected
        );
        assert!(
            (ang.ac_deg.unwrap() - as_expected).abs() < 5.0 / 60.0,
            "As {:.4} expected {:.4}",
            ang.ac_deg.unwrap(),
            as_expected
        );
    }

    #[test]
    fn compute_angles_no_lat_omits_asc() {
        use pericynthion::time::delta_t::jd_ut_to_jd_tt;
        let jd_tt = jd_ut_to_jd_tt(2_435_424.752_8);
        let ang = compute_angles(jd_tt, -118.352_500, None);
        assert!(ang.ac_deg.is_none());
        let diff = (ang.ic_deg - ang.mc_deg).rem_euclid(360.0);
        assert!((diff - 180.0).abs() < 1e-6, "IC-MC diff {diff:.6}");
    }

    #[test]
    fn compute_angles_dsc_is_asc_plus_180() {
        use pericynthion::time::delta_t::jd_ut_to_jd_tt;
        let jd_tt = jd_ut_to_jd_tt(2_435_424.752_8);
        let ang = compute_angles(jd_tt, -118.352_500, Some(34.138_889));
        let ac = ang.ac_deg.expect("As present with lat");
        let ds = ang.ds_deg.expect("Ds present with lat");
        let diff = (ds - ac).rem_euclid(360.0);
        assert!((diff - 180.0).abs() < 1e-9, "Ds-As diff {diff:.9}");
    }

    #[test]
    fn compute_angles_no_lat_omits_dsc() {
        use pericynthion::time::delta_t::jd_ut_to_jd_tt;
        let jd_tt = jd_ut_to_jd_tt(2_435_424.752_8);
        let ang = compute_angles(jd_tt, -118.352_500, None);
        assert!(ang.ds_deg.is_none());
    }

    #[test]
    fn compute_lots_leo_asc_day_chart() {
        // Adèle Haenel: Sun=322.889° (Aqr), Moon=35.683° (Tau), ASC=124.919° (Leo).
        // Sun above horizon → day chart. refchart PF: Lib⌖17°42'46" = 197.713°.
        // Spirit (Day) = ASC + Sun − Moon. No planets → no Eros/Necessity/Courage/Victory/Nemesis.
        let lots = compute_lots(124.919, 322.889, 35.683, None, None, None, None, None);
        assert_eq!(lots.sect, Sect::Day);
        let expected_pf = 180.0 + 17.0 + 42.0 / 60.0 + 46.0 / 3600.0_f64;
        assert!(
            (lots.fortune_deg - expected_pf).abs() < 5.0 / 60.0,
            "Fortune {:.4} expected {:.4}",
            lots.fortune_deg,
            expected_pf
        );
        let expected_spirit = (124.919 + 322.889 - 35.683_f64).rem_euclid(360.0);
        assert!(
            (lots.spirit_deg - expected_spirit).abs() < 1e-3,
            "Spirit {:.4} expected {:.4}",
            lots.spirit_deg,
            expected_spirit
        );
        // Exaltation always emits — day = ASC + 18° − Sun.
        let expected_exalt = (124.919 + 18.0 - 322.889_f64).rem_euclid(360.0);
        assert!(
            (lots.exaltation_deg - expected_exalt).abs() < 1e-3,
            "Exaltation {:.4} expected {:.4}",
            lots.exaltation_deg,
            expected_exalt
        );
        assert!(lots.eros_deg.is_none(), "Eros absent without Venus");
        assert!(
            lots.necessity_deg.is_none(),
            "Necessity absent without Mercury"
        );
        assert!(lots.courage_deg.is_none(), "Courage absent without Mars");
        assert!(lots.victory_deg.is_none(), "Victory absent without Jupiter");
        assert!(lots.nemesis_deg.is_none(), "Nemesis absent without Saturn");
    }

    #[test]
    fn compute_lots_lilly_night_chart() {
        // Lilly: Sun=49.987°, Moon=284.760°, ASC=332.110° → night chart.
        let lots = compute_lots(332.110, 49.987, 284.760, None, None, None, None, None);
        assert_eq!(lots.sect, Sect::Night);
        // Night PF: ASC + Sun − Moon = 97.337°
        let expected_pf = (332.110 + 49.987 - 284.760_f64).rem_euclid(360.0);
        assert!(
            (lots.fortune_deg - expected_pf).abs() < 1e-3,
            "PF {:.4} expected {:.4}",
            lots.fortune_deg,
            expected_pf
        );
        // Exaltation night = ASC + 32° − Moon.
        let expected_exalt = (332.110 + 32.0 - 284.760_f64).rem_euclid(360.0);
        assert!(
            (lots.exaltation_deg - expected_exalt).abs() < 1e-3,
            "Exaltation {:.4} expected {:.4}",
            lots.exaltation_deg,
            expected_exalt
        );
    }

    #[test]
    fn compute_lots_emits_eros_when_venus_present() {
        // Leo ASC day chart: Sun=219.601°, Moon=324.291°, ASC=317.671°,
        // Venus=255.325° (DE441 apparent geo from acceptance test output).
        // Day Eros = ASC + Venus − Spirit, Spirit_day = ASC + Sun − Moon = 212.981°.
        // Eros_day = 317.671 + 255.325 − 212.981 = 360.015° → 0.015°.
        let lots = compute_lots(
            317.671,
            219.601,
            324.291,
            None,
            Some(255.325),
            None,
            None,
            None,
        );
        let eros = lots.eros_deg.expect("eros present with venus");
        let expected = (317.671 + 255.325 - 212.981_f64).rem_euclid(360.0);
        assert!(
            (eros - expected).abs() < 1e-3,
            "Eros {eros:.4} expected {expected:.4}"
        );
    }

    fn leo_asc_frame() -> (f64, f64, f64, f64) {
        use pericynthion::coords::acds::ac_rad;
        use pericynthion::coords::nutation::nutation;
        use pericynthion::coords::obliquity::mean_obliquity_rad;
        use pericynthion::coords::sidereal_time::gast_rad;
        use pericynthion::time::delta_t::jd_ut_to_jd_tt;
        let jd_tt = jd_ut_to_jd_tt(2_435_424.752_8);
        let lon_east = -118.352_500_f64;
        let lat = 34.138_889_f64.to_radians();
        let ramc = (gast_rad(jd_tt) + lon_east.to_radians()).rem_euclid(std::f64::consts::TAU);
        let nut = nutation(jd_tt);
        let obliquity = mean_obliquity_rad(jd_tt) + nut.delta_epsilon;
        let ac = ac_rad(ramc, obliquity, lat).expect("ac exists");
        (ramc, obliquity, ac, lat)
    }

    fn find_house(h: &Houses, sys: HouseArg) -> Option<&HouseCusps> {
        h.iter()
            .find(|(s, _)| *s == sys)
            .and_then(|(_, c)| c.as_ref())
    }

    #[test]
    fn compute_houses_leo_h1_equals_asc() {
        // Lightning-strike frame: Asc ≈ 125.33° (Leo 5°20').
        // Equal-from-Asc puts cusp(1) at the Asc itself; whole-sign puts
        // it at the start of Leo (120°).
        let (ramc, obliquity, ac, lat) = leo_asc_frame();
        let h = compute_houses(ramc, obliquity, ac, lat, HouseArg::ALL);
        let eq = find_house(&h, HouseArg::EqualFromAsc).expect("equal");
        let ws = find_house(&h, HouseArg::WholeSign).expect("whole-sign");
        assert!((eq.cusp(1) - ac).abs() < 1e-9);
        assert!((ws.cusp(1).to_degrees() - 120.0).abs() < 1e-9);
        assert!(find_house(&h, HouseArg::Placidus).is_some());
    }

    #[test]
    fn compute_houses_filter_only_whole_sign() {
        let (ramc, obliquity, ac, lat) = leo_asc_frame();
        let h = compute_houses(ramc, obliquity, ac, lat, &[HouseArg::WholeSign]);
        assert_eq!(h.len(), 1);
        assert_eq!(h[0].0, HouseArg::WholeSign);
        assert!(find_house(&h, HouseArg::EqualFromAsc).is_none());
        assert!(find_house(&h, HouseArg::Placidus).is_none());
    }

    #[test]
    fn compute_houses_filter_two_systems_preserves_order() {
        let (ramc, obliquity, ac, lat) = leo_asc_frame();
        let h = compute_houses(
            ramc,
            obliquity,
            ac,
            lat,
            &[HouseArg::Placidus, HouseArg::WholeSign],
        );
        assert_eq!(h.len(), 2);
        assert_eq!(h[0].0, HouseArg::Placidus);
        assert_eq!(h[1].0, HouseArg::WholeSign);
    }

    #[test]
    fn compute_houses_empty_filter_returns_empty() {
        let (ramc, obliquity, ac, lat) = leo_asc_frame();
        let h = compute_houses(ramc, obliquity, ac, lat, &[]);
        assert!(h.is_empty());
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
