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
//! refchart oracle yet.
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
//!     [--house whole-sign,equal-from-asc,placidus,regiomontanus,porphyry,alcabitius,morinus] \
//!     [--dd | --dms | --ddm | --dm]      # coord format (page: --dm; text: --dd) \
//!     [--jpl-data PATH] \
//!     [--text | --page]                  # output style (default = jzod)
//!     [--asteroids ceres,vesta,...]      # asteroid apparent positions (all output modes)
//!     [--spk PATH]                       # explicit BSP file; auto-discovered when omitted
//! ```
//!
//! # JPL data resolution
//!
//! Resolution order:
//!
//! 1. `--jpl-data PATH` — any directory in the JPL mirror hierarchy.
//! 2. `$STARCAT_JPL_DATA` env var (same as `--jpl-data`).
//!
//! No default path — one of the two must be supplied.
//!
//! `PATH` may point to the de441 dir, `ascii/`, `Linux/`, `planets/`,
//! `eph/`, `ftp/`, or the `ssd.jpl.nasa.gov` root. Binary and ASCII
//! datasets are both supported; the library auto-discovers and opens
//! whichever is present.
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
use pericynthion::jpl::discover;
use pericynthion::jpl::oracle;
use pericynthion::lots::Sect;
use pericynthion::spk::SpkEphemeris;
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
  draconic     0° = Moon's North Node — use --draconic (jzod + --text)
  antiscia     solstice-axis / equinox-axis reflections — use --antiscia (jzod + --text)

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
  Houses   Whole Sign, Equal-from-ASC, Placidus, Regiomontanus, Porphyry,
           Alcabitius, Morinus (need lat + lon; geo/topo only)
  Derived  Antiscion / Contra-antiscion: solstice/equinox-axis reflections
           of every rendered longitude — appended when --antiscia is passed

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
    /// List stellar catalogue contents: named fixed stars and open clusters.
    ///
    /// Use `--bodies` for supported computation bodies, `--points` for
    /// mathematical points (angles, nodes, Lilith, lots), `--stars` for named
    /// fixed stars, `--clusters` for open clusters, or `--all` for everything.
    /// `--verbose` expands the stars listing from the 33 common-name entries to
    /// all 3,157 named BSC5P entries. At least one of the primary flags is required.
    /// For body availability (data present?) see `starcat placements`.
    Catalogue {
        /// List supported computation bodies (planets, dwarf planets, asteroids,
        /// centaurs, KBOs, TNOs). Same content as the pre-flag `starcat catalogue`.
        #[arg(long)]
        bodies: bool,
        /// List mathematical points: angles (Asc/Desc/MC/IC/Vx/Ax), lunar nodes,
        /// Black Moon Lilith, the eight Hermetic lots, and derived views
        /// (Antiscion / Contra-antiscion — see `--antiscia` on `compute`).
        #[arg(long)]
        points: bool,
        /// List named fixed stars. Default: 33 common-name stars (NOTABLE).
        /// With --verbose: all 3,157 named BSC5P entries.
        #[arg(long)]
        stars: bool,
        /// List open clusters used as astrological fixed points
        /// (Aculeus, Acumen, Capulus).
        #[arg(long)]
        clusters: bool,
        /// Equivalent to --bodies --points --stars --clusters.
        #[arg(long)]
        all: bool,
        /// Expand the stars listing to all 3,157 named BSC5P entries (Yale
        /// Bright Star Catalogue 5th edition). Default shows only the 33
        /// common-name stars. No effect on --bodies, --points, or --clusters.
        #[arg(long)]
        verbose: bool,
    },
    /// Fetch SPK ephemerides for a body class from the JPL Horizons API.
    Horizons(HorizonsArgs),
    /// Inspect the local JPL mirror: verify files or list the packaging subset.
    Data(DataArgs),
    /// Print a shell completion script to stdout.
    #[command(hide = true)]
    GenerateCompletion { shell: Option<clap_complete::Shell> },
    /// Print the generated placements catalog as Markdown (feeds docs/placements.md).
    /// With --verify, also fetches and smoke-tests unsupported catalog bodies,
    /// printing `name<TAB>note` for each confirmed body (for piping to promote script).
    #[command(hide = true)]
    Placements {
        /// Discover, fetch (unless --dry-run), and verify unsupported catalog bodies.
        /// Prints confirmed bodies to stdout as `name\tnote`. No markdown output.
        #[arg(long)]
        verify: bool,
        /// With --verify: skip live Horizons fetching; only check files already on disk.
        #[arg(long, requires = "verify")]
        dry_run: bool,
    },
}

/// Arguments for the `data` subcommand.
#[derive(Args, Debug)]
struct DataArgs {
    #[command(subcommand)]
    cmd: DataCmd,
}

/// `data` sub-operations.
#[derive(Subcommand, Debug)]
enum DataCmd {
    /// Verify mirror files against the built-in BLAKE3 oracle.
    ///
    /// `verify` (default scope) checks only the files for the placements
    /// starcat supports; `verify all` checks the entire ~190 GB oracle.
    Verify(VerifyArgs),
    /// List the data files needed to package starcat's supported placements,
    /// one per line. Paths are printed exactly as supplied (relative or
    /// absolute) — never canonicalized — so CI/CD can gather them directly.
    Prod(ProdArgs),
    /// Report every catalogued body + the fixed stars: their data file(s),
    /// source URL, and whether each is cached locally. Read-only; no network.
    Provenance(ProvenanceArgs),
}

/// Arguments for `data verify`.
#[derive(Args, Debug)]
struct VerifyArgs {
    /// What to verify: `supported` (default) for the supported-placements
    /// subset, or `all` for the entire oracle (~190 GB, several minutes).
    #[arg(value_enum, default_value_t = VerifyScope::Supported)]
    scope: VerifyScope,
    /// Mirror root — the directory that directly contains `ssd.jpl.nasa.gov/`.
    /// Falls back to `$STARCAT_JPL_DATA`, walked up to the mirror root.
    #[arg(long)]
    root: Option<PathBuf>,
}

/// Arguments for the `horizons` subcommand.
#[derive(Args, Debug)]
struct HorizonsArgs {
    /// Which class of bodies to fetch (the in-house catalog list for it).
    noun: HorizonsNoun,
    /// SPK span start (Horizons format, e.g. `1900-01-01`). Defaults to
    /// Uranus's discovery, 1781-03-13.
    #[arg(long)]
    from: Option<String>,
    /// SPK span stop. Defaults to the 2038 32-bit `time_t` overflow.
    #[arg(long)]
    to: Option<String>,
    /// Directory to write `<naif_id>.bsp` files into. Falls back to
    /// `$STARCAT_HORIZONS_DATA`. Kept separate from the JPL mirror.
    #[arg(long)]
    out: Option<PathBuf>,
}

/// Class of minor body for `horizons`, mapping to a
/// [`pericynthion::placements::Category`].
#[derive(ValueEnum, Debug, Clone, Copy)]
enum HorizonsNoun {
    /// Dwarf planets.
    Dp,
    /// Asteroids.
    Ast,
    /// Centaurs.
    Cent,
    /// Kuiper-belt objects.
    Kbo,
    /// Trans-Neptunian objects.
    Tno,
}

impl HorizonsNoun {
    fn category(self) -> pericynthion::placements::Category {
        use pericynthion::placements::Category;
        match self {
            HorizonsNoun::Dp => Category::DwarfPlanet,
            HorizonsNoun::Ast => Category::Asteroid,
            HorizonsNoun::Cent => Category::Centaur,
            HorizonsNoun::Kbo => Category::Kbo,
            HorizonsNoun::Tno => Category::Tno,
        }
    }
}

/// Arguments for `data prod`.
#[derive(Args, Debug)]
struct ProdArgs {
    /// Mirror root — the directory that directly contains `ssd.jpl.nasa.gov/`.
    /// Falls back to `$STARCAT_JPL_DATA`, walked up to the mirror root. The
    /// listed paths keep whichever form (relative or absolute) was supplied.
    #[arg(long)]
    root: Option<PathBuf>,
}

/// Arguments for `data provenance`.
#[derive(Args, Debug)]
struct ProvenanceArgs {
    /// JPL mirror root (dir containing `ssd.jpl.nasa.gov/`). Falls back to
    /// `$STARCAT_JPL_DATA`. If absent, JPL files report "not cached".
    #[arg(long)]
    root: Option<PathBuf>,
    /// Horizons SPK dir. Falls back to `$STARCAT_HORIZONS_DATA`. If absent,
    /// Horizons files report "not cached".
    #[arg(long)]
    horizons: Option<PathBuf>,
    /// Emit JSON instead of a table.
    #[arg(long)]
    json: bool,
}

/// Scope for `data verify`.
#[derive(ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
enum VerifyScope {
    /// Only the DE441 + small-body files for supported placements (~3 GB).
    Supported,
    /// The complete mirror oracle (~190 GB).
    All,
}

#[derive(Args, Debug)]
struct ComputeArgs {
    /// Date in YYYY-MM-DD form (proleptic; negative years allowed for BCE).
    /// Required to compute a chart.
    #[arg(long)]
    date: Option<String>,

    /// Time in `HH:MM[:SS]` form, in the zone specified by `--tz` or `--lmt`.
    /// Required to compute a chart.
    #[arg(long)]
    time: Option<String>,

    /// Which calendar the date is recorded in. No default — caller must choose.
    /// Required to compute a chart.
    #[arg(long)]
    calendar: Option<CalendarArg>,

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

    /// Comma-separated house system(s) to emit. Defaults to all seven
    /// always-on systems (whole-sign,equal-from-asc,placidus,regiomontanus,porphyry,alcabitius,morinus).
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

    /// Any directory in the JPL mirror hierarchy: the de441 dir itself,
    /// `ascii/`, `Linux/`, `planets/`, `eph/`, `ftp/`, or the
    /// `ssd.jpl.nasa.gov` root. Binary and ASCII datasets are both
    /// supported. Falls back to `$STARCAT_JPL_DATA` when omitted; one
    /// or the other must be set.
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

    /// Comma-separated body slugs (catalog names, case-insensitive) to compute
    /// alongside the classical bodies. Accepts any body in the placements catalog
    /// (asteroids, centaurs, KBOs, TNOs, dwarf planets). Example:
    /// `--asteroids ceres,chiron,eris`. Bundled bodies (Ceres, Pallas, Juno,
    /// Vesta, Hygiea) are available from the JPL mirror; all others must be
    /// fetched first with `starcat horizons <class>`.
    #[arg(long = "asteroids", value_delimiter = ',')]
    asteroids: Vec<String>,

    /// Explicit path to a DAF/SPK file (e.g. `sb441-n16.bsp`), opened in
    /// addition to the auto-discovered sb441 bundle and any `.bsp` files in
    /// `$STARCAT_HORIZONS_DATA`. A body is computed from whichever opened SPK
    /// covers its NAIF id.
    #[arg(long = "spk")]
    spk: Option<PathBuf>,

    /// Compute and render a chart containing every body starcat currently
    /// supports — all planets/points/lots plus **all** named asteroids
    /// automatically, when the SPK is available. All output modes
    /// (`--text`, `--page`, jzod) work normally.
    ///
    /// For just the list of supported points and bodies (no chart, no inputs),
    /// see `starcat catalogue`.
    #[arg(long = "omniscient")]
    omniscient: bool,

    /// Comma-separated fixed star names to include in the chart. Accepts common
    /// names (Sirius, Algol), Robson/Brady names (Rasalhague, Sadalmelek),
    /// multi-word concatenated (ZubenElgenubi), HR numbers (936, HR936), or
    /// BSC5P designations (26Bet Per). See `starcat catalogue --stars`.
    #[arg(long = "stars", value_delimiter = ',')]
    stars: Vec<String>,

    /// Append Antiscion / Contra-antiscion reflections to the output — a
    /// sub-table in `--text`, and per-body antiscion fields in the default JZOD.
    /// Each body's antiscion reflects across the Cancer/Capricorn (solstice)
    /// axis; the contra-antiscion reflects across the Aries/Libra (equinox) axis.
    /// No-op in `--page` mode.
    #[arg(long = "antiscia")]
    antiscia: bool,

    /// Re-project all longitudes into the draconic zodiac (0° = Moon's mean
    /// North Node) before rendering. Applies to the default JZOD output (the
    /// chart `zodiac` becomes `draconic`) and to `--text`. The node variant is
    /// controlled by `--nodes`. No-op in `--page` mode.
    #[arg(long = "draconic")]
    draconic: bool,
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
        Command::Catalogue {
            bodies,
            points,
            stars,
            clusters,
            all,
            verbose,
        } => cmd_catalogue(bodies, points, stars, clusters, all, verbose),
        Command::Horizons(args) => cmd_horizons(&args),
        Command::Data(args) => cmd_data(&args),
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
        Command::Placements { verify, dry_run } => {
            if verify {
                cmd_placements_verify(dry_run)
            } else {
                print!(
                    "{}{}",
                    pericynthion::placements::markdown(),
                    pericynthion::stars::markdown_stats(),
                );
                Ok(())
            }
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

fn cmd_catalogue(
    bodies: bool,
    points: bool,
    stars: bool,
    clusters: bool,
    all: bool,
    verbose: bool,
) -> Result<()> {
    if !bodies && !points && !stars && !clusters && !all {
        anyhow::bail!(
            "specify at least one of --bodies, --points, --stars, --clusters, or --all\n\
             See `starcat catalogue --help`. For body availability see `starcat placements`."
        );
    }

    let show_bodies = all || bodies;
    let show_points = all || points;
    let show_stars = all || stars;
    let show_clusters = all || clusters;

    if show_bodies {
        if all {
            println!("## Bodies");
            println!();
        }
        print!("{}", pericynthion::placements::supported_list());
    }

    if show_points {
        if all {
            println!();
            println!("## Points");
            println!();
        }
        print_points_catalogue();
    }

    if show_stars {
        let named_all: Vec<_> = pericynthion::named_bsc5_entries().collect();
        if named_all.is_empty() {
            eprintln!("BSC5 catalog not loaded — run `just fetch bsc5` then rebuild.");
        } else {
            if all {
                println!();
                println!("## Fixed Stars");
                println!();
            }
            println!("HR\tDesignation\tCommon Name\tVmag");
            if verbose {
                for e in &named_all {
                    let common = pericynthion::stars::NOTABLE
                        .iter()
                        .find(|(_, hr)| *hr == e.hr)
                        .map(|(n, _)| *n)
                        .unwrap_or("");
                    let vmag = e
                        .vmag
                        .map_or_else(|| "—".to_string(), |v| format!("{v:.2}"));
                    println!("{}\t{}\t{}\t{}", e.hr, e.name, common, vmag);
                }
            } else {
                for (common, hr) in pericynthion::stars::NOTABLE {
                    let (desig, vmag) = match pericynthion::stars::BscEntry::by_hr(*hr) {
                        Some(e) => {
                            let vmag = e
                                .vmag
                                .map_or_else(|| "—".to_string(), |v| format!("{v:.2}"));
                            (e.name, vmag)
                        }
                        None => ("", "—".to_string()),
                    };
                    println!("{}\t{}\t{}\t{}", hr, desig, common, vmag);
                }
            }
        }
    }

    if show_clusters {
        if all {
            println!();
            println!("## Clusters");
            println!();
        }
        println!("Name\tObject\tRA (deg)\tDec (deg)");
        for c in &pericynthion::CLUSTERS {
            println!(
                "{}\t{}\t{:.3}\t{:.3}",
                c.name, c.object, c.ra_deg, c.dec_deg
            );
        }
    }

    Ok(())
}

fn print_points_catalogue() {
    println!("Group\tName\tNotes");
    let rows: &[(&str, &str, &str)] = &[
        (
            "Angles",
            "Ascendant (Asc)",
            "requires observer latitude and longitude",
        ),
        ("Angles", "Descendant (Desc)", "Asc + 180°"),
        ("Angles", "Midheaven (MC)", "requires observer longitude"),
        ("Angles", "Imum Coeli (IC)", "MC + 180°"),
        (
            "Angles",
            "Vertex (Vx)",
            "requires observer latitude and longitude",
        ),
        ("Angles", "Anti-Vertex (Ax)", "Vx + 180°"),
        ("Nodes", "Mean North Node", "geocentric, smoothed"),
        ("Nodes", "Mean South Node", "Mean North Node + 180°"),
        ("Nodes", "True North Node", "osculating"),
        ("Nodes", "True South Node", "True North Node + 180°"),
        (
            "Lilith",
            "Mean Lilith",
            "Black Moon Lilith, mean lunar apogee",
        ),
        ("Lilith", "Mean Priapus", "Mean Lilith + 180°"),
        ("Lilith", "True Lilith", "osculating lunar apogee"),
        (
            "Lots",
            "Fortune",
            "always computed; formula inverts by sect",
        ),
        ("Lots", "Spirit", "always computed; formula inverts by sect"),
        ("Lots", "Exaltation", "always computed"),
        ("Lots", "Eros", "requires Venus"),
        ("Lots", "Necessity", "requires Mercury"),
        ("Lots", "Courage", "requires Mars"),
        ("Lots", "Victory", "requires Jupiter"),
        ("Lots", "Nemesis", "requires Saturn"),
        (
            "Derived",
            "Antiscion",
            "solstice-axis reflection (180° − λ) mod 360° — see --antiscia on compute",
        ),
        (
            "Derived",
            "Contra-antiscion",
            "equinox-axis reflection (360° − λ) mod 360° — see --antiscia on compute",
        ),
    ];
    for (group, name, notes) in rows {
        println!("{group}\t{name}\t{notes}");
    }
}

/// Maps a `BodyResolveError` from the placements library to the exact CLI
/// error strings that starcat presents to the user.  Extracted so that unit
/// tests can assert the byte-identical strings without exercising the full
/// `cmd_compute` pipeline.
fn body_resolve_cli_error(e: pericynthion::placements::BodyResolveError) -> anyhow::Error {
    use pericynthion::placements::BodyResolveError as E;
    match e {
        E::Unknown(s) => anyhow::anyhow!("unknown body {s:?} (not in the placements catalog)"),
        E::NotMinorBody(n) => {
            anyhow::anyhow!("{n} is not an SPK minor body (computed from DE441, not --asteroids)")
        }
        E::NotCovered(n) => anyhow::anyhow!(
            "{n} is not available locally — fetch it first with \
             `starcat horizons <class>` (e.g. its category) into $STARCAT_HORIZONS_DATA"
        ),
    }
}

// Called once per process from `main`; taking ComputeArgs by value lets the
// body freely consume fields (e.g. `args.bodies` via `.clone()`-then-drop)
// without lifetime juggling. The allocation cost is zero in CLI context.
#[allow(clippy::needless_pass_by_value)]
fn cmd_compute(args: ComputeArgs) -> Result<()> {
    // === Output format (read before any partial moves of `args`) ===
    let fmt = CoordFormat::from_args(&args);

    // === Ephemeris file ===
    let dir = resolve_jpl_dir(args.jpl_data.as_deref())?;

    // === Compute always builds a chart — date/time/calendar are required ===
    let missing: Vec<&str> = [
        args.date.is_none().then_some("--date"),
        args.time.is_none().then_some("--time"),
        args.calendar.is_none().then_some("--calendar"),
    ]
    .into_iter()
    .flatten()
    .collect();
    if !missing.is_empty() {
        bail!("a chart needs: {}", missing.join(", "));
    }
    let date_str = args.date.as_deref().unwrap();
    let time_str = args.time.as_deref().unwrap();
    let calendar_arg = args.calendar.unwrap();

    // === Parse date and time ===
    let (year, month, day) =
        parse_date(date_str).with_context(|| format!("invalid --date {date_str:?}"))?;
    let (hour, minute, second) =
        parse_time(time_str).with_context(|| format!("invalid --time {time_str:?}"))?;
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

    let (header, source) = discover::open_dataset(&dir)
        .with_context(|| format!("locate + open JPL ephemeris under {}", dir.display()))?;
    let ephem = Ephemeris::new(&*source, &header).context("build ephemeris facade")?;

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
    let calendar: Calendar = calendar_arg.into();

    // === Open every available SPK: sb441 bundle + Horizons dir + explicit --spk ===
    let horizons_dir = resolve_horizons_dir(None).ok();
    let spk_files = pericynthion::spk::open_all_sources(
        Some(&dir),
        horizons_dir.as_deref(),
        args.spk.as_deref(),
    )
    .context("opening SPK sources")?;
    let spk_refs: Vec<&SpkEphemeris> = spk_files.iter().collect();
    let covered = |id: i32| spk_refs.iter().any(|s| s.center_of(id).is_some());

    // === Asteroids: slug → NAIF id (error clearly on unknown slug) ===
    // --omniscient computes every body covered by the open SPKs.
    let asteroid_naif_ids: Vec<i32> = if args.omniscient {
        pericynthion::placements::omniscient_body_ids(covered)
    } else {
        let mut ids = Vec::new();
        for slug in &args.asteroids {
            let id = pericynthion::placements::resolve_body_id(slug, covered)
                .map_err(body_resolve_cli_error)?;
            ids.push(id);
        }
        ids
    };

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
        asteroids: asteroid_naif_ids,
    };
    // Resolve --stars names; warn and skip unknowns; silently skip empty/whitespace entries.
    let resolved_stars: Vec<pericynthion::ResolvedStar> = args
        .stars
        .iter()
        .filter_map(|name| {
            if name.trim().is_empty() {
                return None;
            }
            match pericynthion::resolve_star(name) {
                Some(rs) => Some(rs),
                None => {
                    eprintln!("warning: unknown star {name:?} — skipped");
                    None
                }
            }
        })
        .collect();

    let computed =
        pericynthion::chart::compute_with_spk(&ephem, &spk_refs, &request, &resolved_stars)
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
        // Derive the draconic node longitude from the selected node mode, when
        // --draconic is requested. None → tropical output.
        let draconic_node: Option<f64> = if args.draconic {
            match (args.nodes, &computed.nodes) {
                (NodesMode::Mean, Some(n)) => Some(n.mean_nn_deg),
                (NodesMode::True, Some(n)) => Some(n.true_nn_deg),
                _ => None,
            }
        } else {
            None
        };
        let chart = pericynthion::jzod::to_jzod_chart(
            &computed,
            &birth,
            uuid::Uuid::new_v4().to_string(),
            draconic_node,
            args.antiscia,
        );
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
        print_text(
            &computed,
            fmt,
            args.nodes,
            args.lilith,
            args.antiscia,
            args.draconic,
        );
    }

    Ok(())
}

/// Resolve the JPL start path from CLI args + env.
///
/// Precedence:
///   1. `--jpl-data PATH` → use as the start node.
///   2. `$STARCAT_JPL_DATA` → use as the start node.
///
/// The returned path is passed to `discover::open_dataset`, which walks up and
/// down the JPL mirror hierarchy to find the actual header + data files.
fn resolve_jpl_dir(data_dir_arg: Option<&std::path::Path>) -> Result<PathBuf> {
    if let Some(d) = data_dir_arg {
        return Ok(d.to_path_buf());
    }
    if let Ok(env) = std::env::var("STARCAT_JPL_DATA") {
        return Ok(PathBuf::from(env));
    }
    bail!(
        "no JPL data location supplied. Pass --jpl-data PATH or set the \
         STARCAT_JPL_DATA environment variable to any directory in the JPL \
         mirror hierarchy (the de441 dir, ascii/, Linux/, planets/, eph/, \
         ftp/, or the ssd.jpl.nasa.gov root). Binary and ASCII datasets are \
         both supported."
    );
}

/// Resolve the JPL mirror root (the directory directly containing
/// `ssd.jpl.nasa.gov/`) from `--root` or `$STARCAT_JPL_DATA`.
///
/// The returned path preserves whichever form (relative or absolute) was
/// supplied — it is never canonicalized.
fn resolve_mirror_root(root_arg: Option<&std::path::Path>) -> Result<PathBuf> {
    if let Some(r) = root_arg {
        return Ok(r.to_path_buf());
    }
    if let Ok(env) = std::env::var("STARCAT_JPL_DATA") {
        let start = PathBuf::from(env);
        return oracle::mirror_root_from(&start).ok_or_else(|| {
            anyhow::anyhow!(
                "could not find a directory containing `ssd.jpl.nasa.gov/` by walking up \
                 from $STARCAT_JPL_DATA. Pass --root pointing directly at the mirror root \
                 (the directory that contains `ssd.jpl.nasa.gov/`)."
            )
        });
    }
    bail!(
        "no mirror root supplied. Pass --root PATH (the directory containing \
         `ssd.jpl.nasa.gov/`) or set $STARCAT_JPL_DATA to any path within the mirror."
    )
}

/// Dispatch the `data` subcommand.
fn cmd_data(args: &DataArgs) -> Result<()> {
    match &args.cmd {
        DataCmd::Verify(v) => cmd_data_verify(v),
        DataCmd::Prod(p) => cmd_data_prod(p),
        DataCmd::Provenance(p) => cmd_data_provenance(p),
    }
}

/// Dispatch `data verify` by scope.
///
/// `Supported` checks the [`oracle::production_entries`] subset — the files
/// needed for the placements starcat supports — and treats a *missing* file as
/// a failure (all of them are required). `All` checks integrity of whatever is
/// actually present across the full oracle under the JPL path: absent files are
/// skipped (not a failure), but any present file that fails its size/hash check
/// is. Either path exits non-zero when a verified file fails.
/// Format a path for b3sum-style verify output.
///
/// Normalises double slashes (from a trailing-slash env var) and strips the
/// cwd prefix so the path is relative when the file lives below the caller.
fn display_verify_path(full: &std::path::Path) -> String {
    let cwd = std::env::current_dir().ok();
    let rel = cwd
        .as_deref()
        .and_then(|d| full.strip_prefix(d).ok())
        .map(|p| p.to_string_lossy().into_owned());
    let s = rel.unwrap_or_else(|| full.to_string_lossy().into_owned());
    // Collapse any double-slashes introduced by a trailing-slash env var.
    let mut out = s.replace("//", "/");
    // Repeat in case of triple-slash edge case.
    while out.contains("//") {
        out = out.replace("//", "/");
    }
    out
}

fn cmd_data_verify(args: &VerifyArgs) -> Result<()> {
    let root = resolve_mirror_root(args.root.as_deref())?;
    match args.scope {
        VerifyScope::Supported => verify_required_subset(&root),
        VerifyScope::All => verify_present_integrity(&root),
    }
}

/// A run fails on the required subset when any file is not OK — a missing or
/// corrupt file both count, because every file in the subset is needed.
fn required_subset_failed(reports: &[oracle::VerifyReport]) -> bool {
    reports
        .iter()
        .any(|r| !matches!(r.status, oracle::VerifyStatus::Ok))
}

/// A present-integrity run fails only when a file that IS present fails its
/// check. Absent files are not a failure (you simply do not have them yet).
fn present_integrity_failed(reports: &[oracle::VerifyReport]) -> bool {
    reports.iter().any(|r| {
        !matches!(
            r.status,
            oracle::VerifyStatus::Ok | oracle::VerifyStatus::Missing
        )
    })
}

/// Verify the supported-placements subset: every file must be present AND pass.
fn verify_required_subset(root: &std::path::Path) -> Result<()> {
    let entries = oracle::production_entries();
    let reports: Vec<oracle::VerifyReport> = entries
        .iter()
        .map(|e| oracle::verify_entry(root, e))
        .collect();
    let ok = reports
        .iter()
        .filter(|r| matches!(r.status, oracle::VerifyStatus::Ok))
        .count();
    for (entry, report) in entries.iter().zip(&reports) {
        let full = root.join(&entry.path);
        if matches!(report.status, oracle::VerifyStatus::Ok) {
            println!("{}  {}", entry.blake3_hex, display_verify_path(&full));
        } else {
            eprintln!(
                "FAIL {}  {} — {:?}",
                entry.blake3_hex,
                display_verify_path(&full),
                report.status
            );
        }
    }
    println!("{ok}/{} supported data files verified OK", reports.len());
    if required_subset_failed(&reports) {
        std::process::exit(1);
    }
    Ok(())
}

/// Verify integrity of whatever oracle files are present under `root`. Absent
/// files are skipped (not a failure); a present file that fails exits non-zero.
fn verify_present_integrity(root: &std::path::Path) -> Result<()> {
    eprintln!("Note: hashing present mirror files can take several minutes.");
    let entries = oracle::entries();
    let reports: Vec<oracle::VerifyReport> = entries
        .iter()
        .map(|e| oracle::verify_entry(root, e))
        .collect();
    let present: Vec<(&oracle::OracleEntry, &oracle::VerifyReport)> = entries
        .iter()
        .zip(&reports)
        .filter(|(_, r)| !matches!(r.status, oracle::VerifyStatus::Missing))
        .collect();
    let ok = present
        .iter()
        .filter(|(_, r)| matches!(r.status, oracle::VerifyStatus::Ok))
        .count();
    let absent = reports.len() - present.len();
    for (entry, report) in &present {
        let full = root.join(&entry.path);
        if matches!(report.status, oracle::VerifyStatus::Ok) {
            println!("{}  {}", entry.blake3_hex, display_verify_path(&full));
        } else {
            eprintln!(
                "FAIL {}  {} — {:?}",
                entry.blake3_hex,
                display_verify_path(&full),
                report.status
            );
        }
    }
    println!(
        "{ok}/{} present files verified OK ({absent} absent, skipped)",
        present.len()
    );
    if present_integrity_failed(&reports) {
        std::process::exit(1);
    }
    Ok(())
}

fn prod_paths(jpl_root: &std::path::Path, horizons_dir: &std::path::Path) -> Vec<String> {
    pericynthion::production_file_paths(jpl_root, horizons_dir)
        .iter()
        .map(|p| display_verify_path(p))
        .collect()
}

/// List the data files needed to package starcat's supported placements,
/// one per line, paths as supplied.
fn cmd_data_prod(args: &ProdArgs) -> Result<()> {
    let jpl_root = resolve_mirror_root(args.root.as_deref())?;
    // Horizons dir is optional for prod listing; fall back to a bare label so
    // the centaur/KBO/TNO files still appear even if the env var is unset.
    let horizons_dir =
        resolve_horizons_dir(None).unwrap_or_else(|_| PathBuf::from("$STARCAT_HORIZONS_DATA"));
    for line in prod_paths(&jpl_root, &horizons_dir) {
        println!("{line}");
    }
    Ok(())
}

/// `data provenance` — read-only report. Never exits non-zero.
fn cmd_data_provenance(args: &ProvenanceArgs) -> Result<()> {
    use pericynthion::placements::{CATALOG, Category};
    // Roots are optional here: resolve from flags/env, but missing is fine.
    let jpl_root = resolve_mirror_root(args.root.as_deref()).ok();
    let horizons_dir = resolve_horizons_dir(args.horizons.as_deref()).ok();
    let jr = jpl_root.as_deref();
    let hr = horizons_dir.as_deref();

    if args.json {
        return print_provenance_json(jr, hr);
    }

    for p in CATALOG
        .iter()
        .filter(|p| p.category != Category::MathematicalPoint)
    {
        let provs = pericynthion::providers_for_body(p.name);
        if provs.is_empty() {
            continue;
        }
        println!("{}  [{}]", p.name, p.category.label());
        for pr in &provs {
            let cached = if pericynthion::provider_cached(pr, jr, hr) {
                "cached"
            } else {
                "absent"
            };
            println!(
                "    {:?}  {}  {}  ({cached})",
                pr.kind, pr.rel_path, pr.source_url
            );
        }
    }

    // Fixed stars: print BOTH facts.
    println!("Fixed stars (BSC5P)  [Fixed stars]");
    let compiled = !pericynthion::stars::BSC5_CATALOG.is_empty();
    println!(
        "    compiled into binary: {} ({} entries)",
        if compiled { "yes" } else { "no" },
        pericynthion::stars::BSC5_CATALOG.len()
    );
    for pr in pericynthion::fixed_star_providers() {
        let cached = if pericynthion::provider_cached(&pr, jr, hr) {
            "cached"
        } else {
            "absent"
        };
        println!("    source: {}  ({cached})", pr.source_url);
    }
    Ok(())
}

/// JSON form of the provenance report.
fn print_provenance_json(
    jpl_root: Option<&std::path::Path>,
    horizons_dir: Option<&std::path::Path>,
) -> Result<()> {
    use pericynthion::placements::{CATALOG, Category};
    let mut bodies = Vec::new();
    for p in CATALOG
        .iter()
        .filter(|p| p.category != Category::MathematicalPoint)
    {
        let provs: Vec<serde_json::Value> = pericynthion::providers_for_body(p.name)
            .iter()
            .map(|pr| {
                serde_json::json!({
                    "kind": format!("{:?}", pr.kind),
                    "rel_path": pr.rel_path,
                    "source_url": pr.source_url,
                    "coverage": pr.coverage,
                    "cached": pericynthion::provider_cached(pr, jpl_root, horizons_dir),
                })
            })
            .collect();
        if provs.is_empty() {
            continue;
        }
        bodies.push(serde_json::json!({
            "name": p.name, "category": p.category.label(), "providers": provs,
        }));
    }
    let stars: Vec<serde_json::Value> = pericynthion::fixed_star_providers()
        .iter()
        .map(|pr| {
            serde_json::json!({
                "source_url": pr.source_url,
                "coverage": pr.coverage,
                "cached": pericynthion::provider_cached(pr, jpl_root, horizons_dir),
            })
        })
        .collect();
    let doc = serde_json::json!({
        "bodies": bodies,
        "fixed_stars": {
            "compiled_into_binary": !pericynthion::stars::BSC5_CATALOG.is_empty(),
            "compiled_entries": pericynthion::stars::BSC5_CATALOG.len(),
            "sources": stars,
        },
    });
    println!("{}", serde_json::to_string_pretty(&doc)?);
    Ok(())
}

/// Resolve the directory for Horizons-fetched SPKs from `--out` or
/// `$STARCAT_HORIZONS_DATA`. Deliberately separate from the JPL mirror.
fn resolve_horizons_dir(out: Option<&std::path::Path>) -> Result<PathBuf> {
    if let Some(o) = out {
        return Ok(o.to_path_buf());
    }
    if let Ok(env) = std::env::var("STARCAT_HORIZONS_DATA") {
        return Ok(PathBuf::from(env));
    }
    bail!(
        "no output directory. Pass --out PATH or set $STARCAT_HORIZONS_DATA \
         (kept separate from the JPL mirror in $STARCAT_JPL_DATA)."
    )
}

/// Fetch SPKs for every minor body in a class, skipping ones already on disk.
fn cmd_horizons(args: &HorizonsArgs) -> Result<()> {
    use pericynthion::horizons::{self, FetchTarget};
    let category = args.noun.category();
    let dir = resolve_horizons_dir(args.out.as_deref())?;
    let (def_start, def_stop) = horizons::default_span();
    let start = args.from.as_deref().unwrap_or(def_start);
    let stop = args.to.as_deref().unwrap_or(def_stop);

    // Candidates: minor bodies in this class with an MPC number. Skip any whose
    // <naif_id>.bsp is already present (idempotent re-runs; courteous to JPL).
    let mut targets = Vec::new();
    let mut already = 0_usize;
    for placement in pericynthion::placements::CATALOG
        .iter()
        .filter(|p| p.category == category)
    {
        let (Some(command), Some(naif_id)) =
            (placement.horizons_command(), placement.horizons_naif_id())
        else {
            continue;
        };
        if dir.join(format!("{naif_id}.bsp")).exists() {
            println!("skip {} ({naif_id}.bsp already present)", placement.name);
            already += 1;
            continue;
        }
        targets.push(FetchTarget {
            label: placement.name.to_string(),
            command,
            naif_id,
        });
    }

    if targets.is_empty() {
        println!("nothing to fetch ({already} body/bodies already present)");
        return Ok(());
    }

    eprintln!(
        "Fetching {} body/bodies for {:?}, {start} .. {stop}, into {} \
         (sequential, throttled — be kind to JPL)",
        targets.len(),
        args.noun,
        dir.display()
    );
    let failures = horizons::fetch_all(&targets, &dir, start, stop, |t, res| match res {
        Ok((path, n)) => println!("ok   {:<12} {:>9} bytes  {}", t.label, n, path.display()),
        Err(e) => eprintln!("FAIL {:<12} {e}", t.label),
    })?;
    if failures > 0 {
        bail!("{failures} body/bodies failed to fetch");
    }
    Ok(())
}

/// Verify (and optionally fetch) every unsupported catalog body, printing
/// `name\tnote` to stdout for each one confirmed computable.
fn cmd_placements_verify(dry_run: bool) -> Result<()> {
    use pericynthion::horizons::{self, DEFAULT_START, DEFAULT_STOP, THROTTLE};
    use pericynthion::placements::CATALOG;
    use pericynthion::spk::{SpkEphemeris, locate_n373_bsp};

    // --- Locate n373 from $STARCAT_JPL_DATA ---
    let n373: Option<SpkEphemeris> = std::env::var("STARCAT_JPL_DATA")
        .ok()
        .and_then(|v| locate_n373_bsp(std::path::Path::new(&v)))
        .and_then(|p| SpkEphemeris::open(&p).ok());

    if n373.is_some() {
        eprintln!("sb441-n373.bsp found — checking KBO perturbers");
    } else {
        eprintln!("sb441-n373.bsp not found (STARCAT_JPL_DATA unset or mirror absent)");
    }

    // --- Horizons output dir (optional) ---
    let horizons_dir: Option<PathBuf> = std::env::var("STARCAT_HORIZONS_DATA")
        .ok()
        .map(PathBuf::from);

    for body in CATALOG
        .iter()
        .filter(|p| !p.supported && p.mpc_number.is_some())
    {
        let sb441_id = body.sb441_naif_id().unwrap();
        let horizons_id = body.horizons_naif_id().unwrap();

        // (a) try n373 first
        if let Some(ref spk) = n373 {
            if spk.state(sb441_id, 0.0).is_ok() {
                println!("{}\tsmall-body SPK (sb441-n373.bsp)", body.name);
                continue;
            }
        }

        // (b) try existing Horizons SPK on disk
        if let Some(ref dir) = horizons_dir {
            let bsp_path = dir.join(format!("{horizons_id}.bsp"));
            if bsp_path.is_file() {
                match SpkEphemeris::open(&bsp_path) {
                    Ok(spk) => {
                        if spk.state(horizons_id, 0.0).is_ok() {
                            println!("{}\tHorizons SPK; fetch with `starcat horizons`", body.name);
                            continue;
                        }
                        eprintln!("  skip {}: .bsp on disk but state() failed", body.name);
                    }
                    Err(e) => {
                        eprintln!("  skip {}: .bsp on disk but open failed: {e}", body.name);
                    }
                }
                continue;
            }
        }

        // (c) live fetch (skipped in dry_run or when no output dir)
        if !dry_run {
            if let Some(ref dir) = horizons_dir {
                let command = body.horizons_command().unwrap();
                eprint!("  fetching {} from Horizons ... ", body.name);
                std::thread::sleep(THROTTLE);
                match horizons::fetch_spk(&command, DEFAULT_START, DEFAULT_STOP) {
                    Ok(bytes) => {
                        let bsp_path = dir.join(format!("{horizons_id}.bsp"));
                        if let Err(e) = std::fs::write(&bsp_path, &bytes) {
                            eprintln!("write failed: {e}");
                            continue;
                        }
                        match SpkEphemeris::open(&bsp_path) {
                            Ok(spk) if spk.state(horizons_id, 0.0).is_ok() => {
                                eprintln!("ok ({} bytes)", bytes.len());
                                println!(
                                    "{}\tHorizons SPK; fetch with `starcat horizons`",
                                    body.name
                                );
                            }
                            _ => eprintln!("fetched but state() failed"),
                        }
                    }
                    Err(e) => eprintln!("fetch failed: {e}"),
                }
            }
        }
    }
    Ok(())
}

// =============================================================================
// Output rendering
// =============================================================================

/// Format a [`pericynthion::coords::tithi::Tithi`] as a display line.
///
/// Produces `"Tithi: <name> (#<index>) <pct>%"` where `<pct>` is the
/// intra-tithi progress rounded to the nearest whole percent.
fn tithi_line(t: &pericynthion::coords::tithi::Tithi) -> String {
    format!(
        "Tithi: {} (#{}) {:.0}%",
        t.name,
        t.index,
        t.fraction * 100.0
    )
}

/// Build antiscia/contra-antiscia rows for a list of `(label, longitude)` points.
///
/// Returns a `Vec<(String, f64, f64)>` of `(label, antiscion_deg, contra_antiscion_deg)`.
/// Delegates the reflection math to [`pericynthion::antiscia`]. No ephemeris required.
fn antiscia_rows(points: &[(&str, f64)]) -> Vec<(String, f64, f64)> {
    points
        .iter()
        .map(|&(label, lon)| {
            (
                label.to_string(),
                pericynthion::antiscia::antiscion(lon),
                pericynthion::antiscia::contra_antiscion(lon),
            )
        })
        .collect()
}

fn print_text(
    computed: &ComputedChart,
    fmt: CoordFormat,
    nodes_mode: NodesMode,
    lilith_mode: LilithMode,
    show_antiscia: bool,
    show_draconic: bool,
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

    // Resolve tropical North Node longitude (mean or true per --nodes).
    // Used both for --draconic projection and for point-table display.
    let node_lon = match (nodes_mode, &computed.nodes) {
        (NodesMode::Mean, Some(n)) => Some(n.mean_nn_deg),
        (NodesMode::True, Some(n)) => Some(n.true_nn_deg),
        _ => None,
    };

    // When --draconic is requested and the node is available, project the chart.
    // Latitude, speed, and retrograde flags are invariant under the shift and
    // are kept from `computed`.
    let drac = if show_draconic {
        node_lon.map(|nn| pericynthion::draconic::project_chart(computed, nn))
    } else {
        None
    };

    if drac.is_some() {
        println!("Zodiac : draconic");
    }

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

    // Collect (label, longitude) for antiscia input — bodies first.
    let mut antiscia_pts: Vec<(&str, f64)> = Vec::new();

    for (idx, cb) in computed.bodies.iter().enumerate() {
        // Under --draconic, use the projected body longitude; lat/distance unchanged.
        let lon_deg = drac
            .as_ref()
            .and_then(|d| d.bodies.get(idx).map(|&(_, l)| l))
            .unwrap_or(cb.position.longitude_deg);
        println!(
            "{:<8} {} {} {:>14.6}",
            cb.body.name(),
            format_zodiac_lon(lon_deg, fmt),
            format_signed_lat(cb.position.latitude_deg, fmt),
            cb.position.distance_au
        );
        antiscia_pts.push((cb.body.name(), lon_deg));
    }
    // Asteroids share the body table: same columns, appended after planets.
    for (idx, ca) in computed.asteroids.iter().enumerate() {
        let lon_deg = drac
            .as_ref()
            .and_then(|d| d.asteroids.get(idx).map(|&(_, l)| l))
            .unwrap_or(ca.position.longitude_deg);
        println!(
            "{:<8} {} {} {:>14.6}",
            ca.name,
            format_zodiac_lon(lon_deg, fmt),
            format_signed_lat(ca.position.latitude_deg, fmt),
            ca.position.distance_au
        );
        antiscia_pts.push((ca.name, lon_deg));
    }

    if let Some(ang) = &computed.angles {
        // Select Nn/Sn based on nodes_mode (tropical or draconic).
        let (nn_deg, sn_deg) = match (nodes_mode, &computed.nodes) {
            (NodesMode::Mean, Some(n)) => (Some(n.mean_nn_deg), Some(n.mean_sn_deg)),
            (NodesMode::True, Some(n)) => (Some(n.true_nn_deg), Some(n.true_sn_deg)),
            _ => (None, None),
        };
        // Select Lil/Pri based on lilith_mode (tropical or draconic).
        let (lil_deg, pri_deg) = match (lilith_mode, &computed.lilith) {
            (LilithMode::Mean, Some(l)) => (Some(l.mean_lilith_deg), Some(l.mean_priapus_deg)),
            (LilithMode::True, Some(l)) => (Some(l.true_lilith_deg), Some(l.true_priapus_deg)),
            _ => (None, None),
        };

        // When draconic, build a lookup from label → draconic lon for angles/nodes/lilith.
        // DraconicChart uses separate Vecs keyed by static label strings.
        let drac_angle_lon = |label: &str| -> Option<f64> {
            drac.as_ref()
                .and_then(|d| d.angles.iter().find(|&&(l, _)| l == label).map(|&(_, v)| v))
        };
        // Node labels in DraconicChart: "MeanNn", "MeanSn", "TrueNn", "TrueSn".
        let drac_node_lon = |drac_label: &str| -> Option<f64> {
            drac.as_ref().and_then(|d| {
                d.nodes
                    .iter()
                    .find(|&&(l, _)| l == drac_label)
                    .map(|&(_, v)| v)
            })
        };
        // Lilith labels in DraconicChart: "MeanLilith", "MeanPriapus", "TrueLilith", "TruePriapus".
        let drac_lilith_lon = |drac_label: &str| -> Option<f64> {
            drac.as_ref().and_then(|d| {
                d.lilith
                    .iter()
                    .find(|&&(l, _)| l == drac_label)
                    .map(|&(_, v)| v)
            })
        };

        // Resolve point longitudes — draconic when available, tropical otherwise.
        let mc_lon = drac_angle_lon("Mc").unwrap_or(ang.mc_deg);
        let ic_lon = drac_angle_lon("Ic").unwrap_or(ang.ic_deg);
        let ac_lon = ang.ac_deg.map(|v| drac_angle_lon("Ac").unwrap_or(v));
        let ds_lon = ang.ds_deg.map(|v| drac_angle_lon("Ds").unwrap_or(v));
        let vx_lon = ang.vx_deg.map(|v| drac_angle_lon("Vx").unwrap_or(v));
        let ax_lon = ang.ax_deg.map(|v| drac_angle_lon("Ax").unwrap_or(v));
        let nn_lon = nn_deg.map(|v| {
            let dl = match nodes_mode {
                NodesMode::Mean => "MeanNn",
                NodesMode::True => "TrueNn",
            };
            drac_node_lon(dl).unwrap_or(v)
        });
        let sn_lon = sn_deg.map(|v| {
            let dl = match nodes_mode {
                NodesMode::Mean => "MeanSn",
                NodesMode::True => "TrueSn",
            };
            drac_node_lon(dl).unwrap_or(v)
        });
        let lil_lon = lil_deg.map(|v| {
            let dl = match lilith_mode {
                LilithMode::Mean => "MeanLilith",
                LilithMode::True => "TrueLilith",
            };
            drac_lilith_lon(dl).unwrap_or(v)
        });
        let pri_lon = pri_deg.map(|v| {
            let dl = match lilith_mode {
                LilithMode::Mean => "MeanPriapus",
                LilithMode::True => "TruePriapus",
            };
            drac_lilith_lon(dl).unwrap_or(v)
        });

        println!();
        println!("{:<8} {:>lon_w$}", "Point", "Longitude", lon_w = lon_w);
        println!("{}", "-".repeat(8 + 1 + lon_w));
        // Display labels use the standardized 2-letter UPPERlower convention:
        // Ac / Ds (Ascendant axis), Mc / Ic (meridian axis), Vx / Ax (vertex
        // axis), Nn / Sn (lunar nodes).
        for (label, lon) in [
            ("Mc", Some(mc_lon)),
            ("Ic", Some(ic_lon)),
            ("Ac", ac_lon),
            ("Ds", ds_lon),
            ("Vx", vx_lon),
            ("Ax", ax_lon),
            ("Nn", nn_lon),
            ("Sn", sn_lon),
            ("Lil", lil_lon),
            ("Pri", pri_lon),
        ] {
            if let Some(lon_deg) = lon {
                println!("{:<8} {}", label, format_zodiac_lon(lon_deg, fmt));
                antiscia_pts.push((label, lon_deg));
            }
        }
    }

    // Fixed stars — only emitted when --stars was supplied.
    if !computed.stars.is_empty() {
        println!();
        println!("{:<16} {:>lon_w$}", "Star", "Longitude", lon_w = lon_w);
        println!("{}", "-".repeat(16 + 1 + lon_w));
        for (idx, star) in computed.stars.iter().enumerate() {
            let lon_deg = drac
                .as_ref()
                .and_then(|d| d.stars.get(idx).map(|&(_, l)| l))
                .unwrap_or(star.position.longitude_deg);
            println!("{:<16} {}", star.name, format_zodiac_lon(lon_deg, fmt));
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
        let base_lots: &[(&str, f64)] = &[
            ("Fortune", l.fortune_deg),
            ("Spirit", l.spirit_deg),
            ("Exaltation", l.exaltation_deg),
        ];
        let opt_lots: &[(&str, Option<f64>)] = &[
            ("Necessity", l.necessity_deg),
            ("Eros", l.eros_deg),
            ("Courage", l.courage_deg),
            ("Victory", l.victory_deg),
            ("Nemesis", l.nemesis_deg),
        ];
        let drac_lot_lon = |label: &str, tropical: f64| -> f64 {
            drac.as_ref()
                .and_then(|d| d.lots.iter().find(|&&(l, _)| l == label).map(|&(_, v)| v))
                .unwrap_or(tropical)
        };
        let mut rows: Vec<(&str, f64)> = base_lots
            .iter()
            .map(|&(label, v)| (label, drac_lot_lon(label, v)))
            .collect();
        for &(label, val) in opt_lots {
            if let Some(v) = val {
                rows.push((label, drac_lot_lon(label, v)));
            }
        }
        for (label, lon_deg) in rows {
            println!("{:<11} {}", label, format_zodiac_lon(lon_deg, fmt));
        }
    }

    // Antiscion / contra-antiscion sub-table — gated behind --antiscia.
    // Applied to whatever longitudes were rendered above (tropical or draconic).
    if show_antiscia && !antiscia_pts.is_empty() {
        let ant_lon_w = lon_col_width(fmt);
        println!();
        println!(
            "{:<8} {:>ant_lon_w$} {:>ant_lon_w$}",
            "Point",
            "Antiscion",
            "C-Antiscion",
            ant_lon_w = ant_lon_w,
        );
        println!("{}", "-".repeat(8 + 1 + ant_lon_w + 1 + ant_lon_w));
        for (label, ant, con) in antiscia_rows(&antiscia_pts) {
            println!(
                "{:<8} {} {}",
                label,
                format_zodiac_lon(ant, fmt),
                format_zodiac_lon(con, fmt),
            );
        }
    }

    if let Some(lp) = &computed.lunar_phase {
        println!();
        println!(
            "Lunar Phase: {}  {:.2}°  day {} of 28",
            lp.phase.label(),
            lp.synodic_arc_deg,
            lp.lunation_day
        );
    }

    if let Some(t) = &computed.tithi {
        println!("{}", tithi_line(t));
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

/// Thin wrapper over `ComputedChart::sorted_placements`, mapping the CLI `NodesMode` to the library `NodeVariant`.
#[cfg(feature = "page")]
fn page_collect_placements(
    computed: &ComputedChart,
    primary_house: Option<&HouseCusps>,
    start_lon: f64,
    nodes_mode: NodesMode,
) -> Vec<(String, f64)> {
    let nv = match nodes_mode {
        NodesMode::Mean => pericynthion::chart::NodeVariant::Mean,
        NodesMode::True => pericynthion::chart::NodeVariant::True,
    };
    computed.sorted_placements(primary_house, start_lon, nv)
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
    // date/time are guaranteed present by cmd_compute's pre-flight check
    let (hour, minute, second) = args
        .time
        .as_deref()
        .and_then(|s| parse_time(s).ok())
        .unwrap_or((0, 0, 0.0));
    let (year, month, day) = args
        .date
        .as_deref()
        .and_then(|s| parse_date(s).ok())
        .unwrap_or((0, 1, 1));
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

    // calendar is guaranteed present by cmd_compute's pre-flight check
    let calendar_str = match args.calendar.unwrap_or(CalendarArg::Gregorian) {
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
            lp.phase.label(),
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
        // Asteroid retrograde by display name
        if let Some(ca) = computed.asteroids.iter().find(|a| a.name == label) {
            return ca.retrograde;
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

    // Lunar phase line natural width (inner content only, rendered flush-left).
    // Example: "Lunar Phase  Waxing Crescent  45.23°  day 5 of 28"
    // Must be included in panel width calculation so the box never truncates it.
    let phase_line_width = phase_str
        .as_deref()
        .map(|s| "Lunar Phase  ".chars().count() + s.chars().count())
        .unwrap_or(0);
    // The phase line sits inside the bottom box (same border accounting as panel).
    let phase_natural_width = if phase_line_width > 0 {
        phase_line_width + 4
    } else {
        0
    };

    // ── TOP BOX = INPUTS ──
    // This box holds ONLY user-supplied CLI inputs: date/time, location,
    // calendar, coordinate system, zodiac, and house system.
    // Computed values (JD, sect, lunar phase, placements) must NOT go here —
    // they belong in the BOTTOM BOX below.
    let banner_rows: Vec<(&str, &str)> = vec![
        (date_time_str.as_str(), coords_str.as_str()),
        (calendar_str, mode_str),
        (zodiac_str, house_str),
    ];
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
        .max(panel_natural_width)
        .max(phase_natural_width);

    // If table is narrower than target, distribute the slack across columns
    // (round-robin from col 0) so the table totals `target_width`.
    let mut extra = target_width.saturating_sub(table_width(&col_widths));
    let mut idx = 0;
    while extra > 0 {
        col_widths[idx % 4] += 1;
        idx += 1;
        extra -= 1;
    }

    // ── TOP BOX = INPUTS render ──
    let inside = target_width - 4;
    let bar = "─".repeat(target_width - 2);
    println!("╭{bar}╮");
    for (l, r) in &banner_rows {
        println!("{}", banner_row(inside, l, r));
    }
    println!("╰{bar}╯");

    // ── divider: everything ABOVE is user INPUT, everything BELOW is computed OUTPUT ──

    // ── BOTTOM BOX = OUTPUTS ──
    // This box holds computed results: JD, sect, lunar phase, and the
    // placements table. Any new computed field belongs here, not in the top box.

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
    // Lunar Phase panel row: rendered immediately below JD/sect, flush-left,
    // inside the same bordered box. Only emitted when computed.lunar_phase is Some.
    let lunar_phase_panel_text = phase_str.as_deref().map(|s| {
        let inner = target_width - 4;
        let content = format!("Lunar Phase  {s}");
        format!("{content:<inner$}")
    });
    // Style:
    // - Top border: plain `─` (no column tee marks above the panel row).
    // - Row 1 rule (under JD/sect panel): plain `─` — not a data-column row.
    // - Row 2 rule: if lunar phase present, another plain `─` under it;
    //   otherwise this is the column-header rule with standard `┼`.
    // - Row 3 rule (when lunar phase present): column-header rule with `┼`.
    let panel_rule = HorizontalLine::full('─', '─', '├', '┤');
    let column_rule = HorizontalLine::full('─', '┼', '├', '┤');
    // Insert panels in reverse order: second panel first (lunar phase), then
    // JD/sect — each Panel::header prepends a row, so the last insert ends up
    // at row 0, giving us: row 0 = JD/sect, row 1 = lunar phase (if any),
    // then the column headers row.
    if let Some(lp_text) = lunar_phase_panel_text {
        // Lunar phase present: 3 header rows (JD/sect, lunar phase, col headers).
        // JD/sect and lunar phase are grouped with NO rule between them (they're
        // all top-of-chart scalars); the only rules are row 2 (plain ─ under the
        // lunar-phase row, separating the header block from the table) and row 3
        // (┼-intersected column rule under the column-header row).
        table.with(Panel::header(lp_text));
        table.with(Panel::header(panel_text));
        table.with(
            Style::rounded()
                .intersection_top('─')
                .horizontals([(2, panel_rule), (3, column_rule)]),
        );
    } else {
        // No lunar phase: 2 header rows (JD/sect, col headers).
        // Row 1 rule (under JD/sect): plain ─, row 2 rule (under col headers): ┼.
        table.with(Panel::header(panel_text));
        table.with(
            Style::rounded()
                .intersection_top('─')
                .horizontals([(1, panel_rule), (2, column_rule)]),
        );
    }
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
#[cfg(test)]
fn zodiac_sign(lon_deg: f64) -> &'static str {
    jzod::coord::Sign::split_longitude(lon_deg).0.abbrev()
}

/// Display-precision (sign, degree-in-sign) split, delegated to
/// [`jzod::coord::Sign::split_longitude`] which owns the cusp-rounding
/// invariant (29.999…° snaps up to the next sign rather than printing
/// `30.0000° Ari`).
fn split_sign(lon_deg: f64) -> (&'static str, f64) {
    let (sign, deg_in_sign) = jzod::coord::Sign::split_longitude(lon_deg);
    (sign.abbrev(), deg_in_sign)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_subcommand_tree_is_wired() {
        use clap::CommandFactory;
        let cli = Cli::command();
        let data = cli
            .get_subcommands()
            .find(|c| c.get_name() == "data")
            .expect("`data` subcommand exists");
        let names: Vec<&str> = data
            .get_subcommands()
            .map(clap::Command::get_name)
            .collect();
        assert!(names.contains(&"verify"), "data has `verify`: {names:?}");
        assert!(names.contains(&"prod"), "data has `prod`: {names:?}");
        // The old top-level name is gone.
        assert!(
            cli.get_subcommands().all(|c| c.get_name() != "verify-data"),
            "verify-data must be renamed to data"
        );
    }

    #[test]
    fn data_verify_parses_supported_and_all() {
        use clap::Parser;
        // `data verify` → supported (default); `data verify all` → all.
        let sup = Cli::try_parse_from(["starcat", "data", "verify"]).unwrap();
        let all = Cli::try_parse_from(["starcat", "data", "verify", "all"]).unwrap();
        match (sup.command, all.command) {
            (Command::Data(a), Command::Data(b)) => {
                assert!(matches!(
                    a.cmd,
                    DataCmd::Verify(VerifyArgs {
                        scope: VerifyScope::Supported,
                        ..
                    })
                ));
                assert!(matches!(
                    b.cmd,
                    DataCmd::Verify(VerifyArgs {
                        scope: VerifyScope::All,
                        ..
                    })
                ));
            }
            _ => panic!("expected Data command"),
        }
    }

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

    #[test]
    fn catalogue_is_a_top_level_command() {
        use clap::CommandFactory;
        let cli = Cli::command();
        assert!(
            cli.get_subcommands().any(|c| c.get_name() == "catalogue"),
            "catalogue is a top-level subcommand"
        );
    }

    #[test]
    fn horizons_noun_parses_and_maps_to_category() {
        use clap::Parser;
        use pericynthion::placements::Category;
        let cli = Cli::try_parse_from(["starcat", "horizons", "cent"]).unwrap();
        match cli.command {
            Command::Horizons(args) => {
                assert!(matches!(args.noun, HorizonsNoun::Cent));
                assert_eq!(args.noun.category(), Category::Centaur);
            }
            _ => panic!("expected Horizons"),
        }
        // The noun is required.
        assert!(Cli::try_parse_from(["starcat", "horizons"]).is_err());
    }

    #[test]
    fn resolve_body_id_prefers_available_scheme() {
        // Pretend only the Horizons (20M) Chiron is present.
        let covered = |id: i32| id == 20_002_060;
        assert_eq!(
            pericynthion::placements::resolve_body_id("chiron", covered).unwrap(),
            20_002_060
        );
        // Unknown slug → error mentioning the name.
        assert!(pericynthion::placements::resolve_body_id("nonsuch", |_| true).is_err());
        // Known body but not present anywhere → error mentioning the body name.
        let none = |_id: i32| false;
        let err = pericynthion::placements::resolve_body_id("chiron", none)
            .unwrap_err()
            .to_string();
        assert!(err.contains("chiron") || err.contains("Chiron"));
    }

    #[test]
    fn resolve_body_id_prefers_sb441_over_horizons() {
        // Both sb441 (2_002_060) and Horizons (20_002_060) covered → prefer sb441.
        let covered = |id: i32| id == 2_002_060 || id == 20_002_060;
        assert_eq!(
            pericynthion::placements::resolve_body_id("chiron", covered).unwrap(),
            2_002_060
        );
    }

    #[test]
    fn resolve_body_id_non_spk_body_errors() {
        // Sun has no MPC number → library yields NotMinorBody, which contains "sun"/"Sun".
        let err = pericynthion::placements::resolve_body_id("sun", |_| true)
            .unwrap_err()
            .to_string();
        assert!(err.to_lowercase().contains("sun"));
    }

    #[test]
    fn body_resolve_cli_error_strings_are_exact() {
        use pericynthion::placements::BodyResolveError;

        // Unknown variant: slug name appears quoted.
        let msg =
            body_resolve_cli_error(BodyResolveError::Unknown("foobar".to_string())).to_string();
        assert_eq!(
            msg,
            r#"unknown body "foobar" (not in the placements catalog)"#
        );

        // NotMinorBody: body name appears, DE441 mentioned, no "de441" file reference.
        let msg = body_resolve_cli_error(BodyResolveError::NotMinorBody("Sun")).to_string();
        assert_eq!(
            msg,
            "Sun is not an SPK minor body (computed from DE441, not --asteroids)"
        );

        // NotCovered: must contain the exact fetch instructions and env var.
        let msg = body_resolve_cli_error(BodyResolveError::NotCovered("Eris")).to_string();
        assert_eq!(
            msg,
            "Eris is not available locally — fetch it first with \
             `starcat horizons <class>` (e.g. its category) into $STARCAT_HORIZONS_DATA"
        );
        // Belt-and-suspenders substring assertions for the key landmarks.
        assert!(msg.contains("fetch it first"));
        assert!(msg.contains("starcat horizons"));
        assert!(msg.contains("$STARCAT_HORIZONS_DATA"));
    }

    #[test]
    fn omniscient_body_ids_returns_covered_subset() {
        // Only Chiron's sb441 id is "covered".
        let chiron_sb441 = 2_002_060_i32;
        let covered = move |id: i32| id == chiron_sb441;
        let ids = pericynthion::placements::omniscient_body_ids(covered);
        assert!(ids.contains(&chiron_sb441));
        // Non-covered bodies must not appear.
        assert!(!ids.contains(&2_000_001_i32)); // Ceres sb441 — not covered here
    }

    #[test]
    fn omniscient_is_a_bare_flag() {
        use clap::Parser;
        // `--omniscient` takes no value now (its only mode was `bodies`).
        let cli = Cli::try_parse_from(["starcat", "compute", "--omniscient"]).unwrap();
        match cli.command {
            Command::Compute(args) => assert!(args.omniscient),
            _ => panic!("expected Compute"),
        }
        // The old value forms (`ls`, `prod`, `bodies`) are no longer accepted.
        assert!(Cli::try_parse_from(["starcat", "compute", "--omniscient", "ls"]).is_err());
    }

    #[test]
    fn catalogue_stars_flag_parses() {
        let args =
            Cli::try_parse_from(["starcat", "catalogue", "--stars"]).expect("--stars should parse");
        assert!(matches!(args.command, Command::Catalogue { stars, .. } if stars));
    }

    #[test]
    fn catalogue_all_flag_parses() {
        let args =
            Cli::try_parse_from(["starcat", "catalogue", "--all"]).expect("--all should parse");
        assert!(matches!(args.command, Command::Catalogue { all, .. } if all));
    }

    #[test]
    fn catalogue_no_flags_is_valid_parse() {
        // No flags → parses OK (runtime error is handled in cmd_catalogue, not clap)
        Cli::try_parse_from(["starcat", "catalogue"])
            .expect("catalogue with no flags should parse");
    }

    #[test]
    fn catalogue_bodies_flag_parses() {
        let args = Cli::try_parse_from(["starcat", "catalogue", "--bodies"])
            .expect("--bodies should parse");
        assert!(matches!(args.command, Command::Catalogue { bodies, .. } if bodies));
    }

    #[test]
    fn catalogue_points_flag_parses() {
        let args = Cli::try_parse_from(["starcat", "catalogue", "--points"])
            .expect("--points should parse");
        assert!(matches!(args.command, Command::Catalogue { points, .. } if points));
    }

    #[test]
    fn catalogue_verbose_flag_parses() {
        let args = Cli::try_parse_from(["starcat", "catalogue", "--stars", "--verbose"])
            .expect("--stars --verbose should parse");
        assert!(
            matches!(args.command, Command::Catalogue { stars, verbose, .. } if stars && verbose)
        );
    }

    #[test]
    fn compute_stars_flag_parses() {
        let args = Cli::try_parse_from([
            "starcat",
            "compute",
            "--date",
            "2000-01-01",
            "--time",
            "12:00",
            "--tz",
            "+00:00",
            "--stars",
            "Sirius,Algol",
        ])
        .expect("--stars should parse");
        if let Command::Compute(a) = args.command {
            assert_eq!(a.stars, vec!["Sirius", "Algol"]);
        } else {
            panic!("expected Compute");
        }
    }

    #[test]
    fn provider_cached_resolves_against_roots() {
        use pericynthion::providers_for_body;
        use pericynthion::{Provider, RootKind};
        let tmp = tempdir::TempDir::new("prov-cache").unwrap();
        let hz = tmp.path().join("hz");
        std::fs::create_dir_all(&hz).unwrap();
        // Chiron Horizons file present:
        std::fs::write(hz.join("20002060.bsp"), b"x").unwrap();
        let prov: Provider = providers_for_body("Chiron").pop().unwrap();
        assert_eq!(prov.root_kind, RootKind::HorizonsDir);
        assert!(pericynthion::provider_cached(
            &prov,
            None,
            Some(hz.as_path())
        ));
        // Absent dir -> not cached:
        assert!(!pericynthion::provider_cached(
            &prov,
            None,
            Some(tmp.path())
        ));
    }

    #[test]
    fn prod_paths_include_n373_and_horizons_bodies() {
        let jpl = std::path::Path::new("/m");
        let hz = std::path::Path::new("/hz");
        let paths = super::prod_paths(jpl, hz);
        assert!(paths.iter().any(|p| p.ends_with("Linux/de441/header.441")));
        assert!(paths.iter().any(|p| p.ends_with("sb441-n16.bsp")));
        assert!(paths.iter().any(|p| p.ends_with("sb441-n373.bsp")));
        // Chiron's Horizons SPK under the horizons dir:
        assert!(paths.iter().any(|p| p.ends_with("20002060.bsp")));
    }

    #[test]
    fn verify_scopes_treat_missing_files_differently() {
        use pericynthion::jpl::oracle::{VerifyReport, VerifyStatus};
        let missing = VerifyReport {
            path: "a".into(),
            status: VerifyStatus::Missing,
        };
        let ok = VerifyReport {
            path: "b".into(),
            status: VerifyStatus::Ok,
        };
        let corrupt = VerifyReport {
            path: "c".into(),
            status: VerifyStatus::HashMismatch {
                expected: "x",
                actual: "y".into(),
            },
        };
        // Required subset: a missing file IS a failure.
        assert!(required_subset_failed(&[missing.clone(), ok.clone()]));
        // Present-integrity: missing alongside OK is fine (absent is allowed).
        assert!(!present_integrity_failed(&[missing.clone(), ok.clone()]));
        // Present-integrity: a present corrupt file fails even amid absences.
        assert!(present_integrity_failed(&[missing, corrupt]));
    }

    #[test]
    fn tithi_line_formats_name_index_pct() {
        use pericynthion::coords::tithi::tithi;
        // tithi(6.0, 0.0): arc=6°, index=1 (Pratipada), fraction=0.5 → 50%
        let t = tithi(6.0, 0.0);
        assert_eq!(super::tithi_line(&t), "Tithi: Pratipada (#1) 50%");
    }

    #[test]
    fn tithi_line_amavasya() {
        use pericynthion::coords::tithi::tithi;
        // tithi(348.0, 0.0): arc=348°, index=30 (Amavasya), fraction=0.0 → 0%
        let t = tithi(348.0, 0.0);
        assert_eq!(super::tithi_line(&t), "Tithi: Amavasya (#30) 0%");
    }

    /// `antiscia_rows` must produce `(label, antiscion_deg, contra_antiscion_deg)`
    /// triples matching the pericynthion reflections.
    ///
    /// Sun at 0°: antiscion = 180°, contra = 0°.
    /// Moon at 90°: antiscion = 90°, contra = 270°.
    #[test]
    fn antiscia_rows_synthetic() {
        let points: &[(&str, f64)] = &[("Sun", 0.0), ("Moon", 90.0)];
        let rows = super::antiscia_rows(points);
        assert_eq!(rows.len(), 2);
        let (ref label0, ant0, con0) = rows[0];
        assert_eq!(label0, "Sun");
        assert!(
            (ant0 - 180.0).abs() < 1e-12,
            "Sun antiscion expected 180°, got {ant0}"
        );
        assert!(
            (con0 - 0.0).abs() < 1e-12,
            "Sun contra expected 0°, got {con0}"
        );
        let (ref label1, ant1, con1) = rows[1];
        assert_eq!(label1, "Moon");
        assert!(
            (ant1 - 90.0).abs() < 1e-12,
            "Moon antiscion expected 90°, got {ant1}"
        );
        assert!(
            (con1 - 270.0).abs() < 1e-12,
            "Moon contra expected 270°, got {con1}"
        );
    }
}
