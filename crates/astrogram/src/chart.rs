//! Canonical in-memory chart representation.
//!
//! Every format reader produces a [`Chart`]; every writer consumes one.
//! All coordinates use ISO 6709 sign conventions: East longitude positive,
//! North latitude positive. Sign-convention conversion happens at the format
//! boundary, never here.

use crate::error::ChartError;

/// Longitude in decimal degrees, ISO 6709 (East positive).
///
/// Valid range: -180.0 ..= 180.0. Construct via [`Longitude::new`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Longitude(f64);

impl Longitude {
    /// Create a `Longitude`, returning an error if `degrees` is outside -180..=180.
    ///
    /// # Errors
    ///
    /// Returns [`ChartError::LongitudeOutOfRange`] if `degrees` is outside `[-180.0, 180.0]`.
    pub fn new(degrees: f64) -> Result<Self, ChartError> {
        if !(-180.0..=180.0).contains(&degrees) {
            return Err(ChartError::LongitudeOutOfRange(degrees));
        }
        Ok(Self(degrees))
    }

    /// Return the value in decimal degrees.
    #[must_use]
    pub fn degrees(self) -> f64 {
        self.0
    }
}

/// Latitude in decimal degrees, ISO 6709 (North positive).
///
/// Valid range: -90.0 ..= 90.0. Construct via [`Latitude::new`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Latitude(f64);

impl Latitude {
    /// Create a `Latitude`, returning an error if `degrees` is outside -90..=90.
    ///
    /// # Errors
    ///
    /// Returns [`ChartError::LatitudeOutOfRange`] if `degrees` is outside `[-90.0, 90.0]`.
    pub fn new(degrees: f64) -> Result<Self, ChartError> {
        if !(-90.0..=90.0).contains(&degrees) {
            return Err(ChartError::LatitudeOutOfRange(degrees));
        }
        Ok(Self(degrees))
    }

    /// Return the value in decimal degrees.
    #[must_use]
    pub fn degrees(self) -> f64 {
        self.0
    }
}

/// Chart subject type.
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventType {
    Unspecified,
    Male,
    Female,
    Event,
    Horary,
}

impl From<u8> for EventType {
    fn from(n: u8) -> Self {
        match n {
            1 => Self::Male,
            2 => Self::Female,
            3 => Self::Event,
            4 => Self::Horary,
            _ => Self::Unspecified,
        }
    }
}

/// House system. Variants cover the 32 systems observed in Solar Fire;
/// `Other` carries the raw id for any system not yet named here.
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HouseSystem {
    Campanus,
    Koch,
    Meridian,
    Morinus,
    Placidus,
    Porphyry,
    Regiomontanus,
    Topocentric,
    Equal,
    ZeroAries,
    SolarSign,
    WholeSign,
    HinduBhava,
    Alcabitius,
    Other(u8),
}

impl From<u8> for HouseSystem {
    fn from(n: u8) -> Self {
        match n {
            1 => Self::Campanus,
            2 => Self::Koch,
            3 => Self::Meridian,
            4 => Self::Morinus,
            5 => Self::Placidus,
            6 => Self::Porphyry,
            7 => Self::Regiomontanus,
            8 => Self::Topocentric,
            9 => Self::Equal,
            10 => Self::ZeroAries,
            11 => Self::SolarSign,
            26 => Self::WholeSign,
            27 => Self::HinduBhava,
            28 => Self::Alcabitius,
            other => Self::Other(other),
        }
    }
}

/// Zodiac system. Variants cover the 17 systems observed in Solar Fire;
/// `Other` carries the raw id for any system not yet named here.
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Zodiac {
    Tropical,
    FaganAllen,
    Lahiri,
    DeLuce,
    Raman,
    UshaShashi,
    Krishnamurti,
    DjwhalKhul,
    Draconic,
    Svp,
    SriYukteswar,
    JnBhasin,
    LarryEly,
    TakraI,
    TakraII,
    SundaraRajan,
    ShillPond,
    Other(u8),
}

impl From<u8> for Zodiac {
    fn from(n: u8) -> Self {
        match n {
            1 => Self::Tropical,
            2 => Self::FaganAllen,
            3 => Self::Lahiri,
            4 => Self::DeLuce,
            5 => Self::Raman,
            6 => Self::UshaShashi,
            7 => Self::Krishnamurti,
            8 => Self::DjwhalKhul,
            9 => Self::Draconic,
            10 => Self::Svp,
            11 => Self::SriYukteswar,
            12 => Self::JnBhasin,
            13 => Self::LarryEly,
            14 => Self::TakraI,
            15 => Self::TakraII,
            16 => Self::SundaraRajan,
            17 => Self::ShillPond,
            other => Self::Other(other),
        }
    }
}

/// Coordinate reference frame.
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoordinateSystem {
    Geocentric,
    Heliocentric,
}

impl From<u8> for CoordinateSystem {
    fn from(n: u8) -> Self {
        match n {
            2 => Self::Heliocentric,
            _ => Self::Geocentric,
        }
    }
}

/// A secondary chart attached to a primary chart (e.g. a progressed or relocated chart).
#[allow(missing_docs)]
#[derive(Debug, Clone, PartialEq)]
pub struct SubChart {
    pub name: String,
    pub city: Option<String>,
    pub region: Option<String>,
    /// ISO 6709: East positive.
    pub longitude: Longitude,
    /// ISO 6709: North positive.
    pub latitude: Latitude,
    pub year: i16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
    /// Timezone offset in hours, ISO 6709: East positive.
    pub tz_offset_hours: f64,
    pub tz_abbreviation: Option<String>,
    pub is_lmt: bool,
    pub notes: Option<String>,
}

/// Canonical in-memory chart representation.
///
/// All coordinate values use ISO 6709 sign conventions regardless of source
/// format. Every reader converts at the boundary; every writer converts back.
#[allow(missing_docs)]
#[derive(Debug, Clone, PartialEq)]
pub struct Chart {
    pub name: String,
    pub secondary_name: Option<String>,
    pub city: Option<String>,
    pub region: Option<String>,
    /// ISO 6709: East positive.
    pub longitude: Longitude,
    /// ISO 6709: North positive.
    pub latitude: Latitude,
    /// Signed to support BCE dates (negative values).
    pub year: i16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
    /// Timezone offset in hours, ISO 6709: East positive.
    pub tz_offset_hours: f64,
    pub tz_abbreviation: Option<String>,
    pub is_lmt: bool,
    pub event_type: EventType,
    /// Source reliability rating and description (e.g. Rodden Rating).
    pub source_rating: Option<String>,
    pub house_system: HouseSystem,
    pub zodiac: Zodiac,
    pub coordinate_system: CoordinateSystem,
    pub sub_charts: Vec<SubChart>,
    pub notes: Option<String>,
}
