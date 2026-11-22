use alloc::string::{String, ToString};
use core::{
    fmt::{Display, Formatter, Result as FmtResult},
    str::FromStr,
};

use serde::{Deserialize, Serialize};
use time::UtcOffset;

use crate::{
    cty::error::CtyError,
    grid_locator::{GridLocator, GridLocatorError},
};

/// A continent as `cty.dat` abbreviates it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Continent {
    Africa,
    Antarctica,
    Asia,
    Europe,
    NorthAmerica,
    Oceania,
    SouthAmerica,
}

impl Continent {
    /// Returns the two-letter abbreviation `cty.dat` writes.
    pub const fn abbreviation(&self) -> &'static str {
        match self {
            Continent::Africa => "AF",
            Continent::Antarctica => "AN",
            Continent::Asia => "AS",
            Continent::Europe => "EU",
            Continent::NorthAmerica => "NA",
            Continent::Oceania => "OC",
            Continent::SouthAmerica => "SA",
        }
    }
}

impl FromStr for Continent {
    type Err = CtyError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "AF" => Ok(Continent::Africa),
            "AN" => Ok(Continent::Antarctica),
            "AS" => Ok(Continent::Asia),
            "EU" => Ok(Continent::Europe),
            "NA" => Ok(Continent::NorthAmerica),
            "OC" => Ok(Continent::Oceania),
            "SA" => Ok(Continent::SouthAmerica),
            _ => Err(CtyError::UnknownContinent(s.to_string())),
        }
    }
}

impl Display for Continent {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.pad(self.abbreviation())
    }
}

/// Where an entity sits and which zones it counts for.
///
/// Signs follow the common convention rather than the one `cty.dat` stores:
/// `longitude` grows towards the east and `utc_offset` is ahead of UTC, both of
/// which the file writes inverted.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Location {
    /// CQ zone, between 1 and 40.
    pub cq_zone: u8,

    /// ITU zone, between 1 and 90.
    pub itu_zone: u8,

    /// Continent the location belongs to.
    pub continent: Continent,

    /// Latitude in degrees, positive towards the north.
    pub latitude: f64,

    /// Longitude in degrees, positive towards the east.
    pub longitude: f64,

    /// Local time offset from UTC.
    pub utc_offset: UtcOffset,
}

impl Location {
    /// Calculates the grid locator of the representative coordinates.
    pub fn grid_locator(&self) -> Result<GridLocator, GridLocatorError> {
        GridLocator::from_lnglat(self.longitude, self.latitude)
    }
}

/// One DXCC entity as declared by a `cty.dat` record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entity {
    /// Full entity name, such as `Japan`.
    pub name: String,

    /// Canonical prefix used as the contest multiplier, such as `JA` or `JD/o`.
    ///
    /// The `*` that marks a WAEDC-only entity in the file is not kept here.
    pub primary_prefix: String,

    /// Whether the entity is on the DARC WAEDC list only and DXCC does not recognise it.
    pub waedc_only: bool,

    /// Representative location of the entity.
    pub location: Location,
}

/// The values an alias replaces on the location its entity declares.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Overrides {
    /// CQ zone, written as `(#)`.
    pub cq_zone: Option<u8>,

    /// ITU zone, written as `[#]`.
    pub itu_zone: Option<u8>,

    /// Continent, written as `{aa}`.
    pub continent: Option<Continent>,

    /// Latitude and longitude, written as `<#/#>`.
    pub coordinates: Option<(f64, f64)>,

    /// Local time offset from UTC, written as `~#~`.
    pub utc_offset: Option<UtcOffset>,
}

impl Overrides {
    /// Returns the location with every override applied.
    pub fn apply(&self, location: Location) -> Location {
        let (latitude, longitude) = self
            .coordinates
            .unwrap_or((location.latitude, location.longitude));

        Location {
            cq_zone: self.cq_zone.unwrap_or(location.cq_zone),
            itu_zone: self.itu_zone.unwrap_or(location.itu_zone),
            continent: self.continent.unwrap_or(location.continent),
            latitude,
            longitude,
            utc_offset: self.utc_offset.unwrap_or(location.utc_offset),
        }
    }

    /// Returns whether no override is set.
    pub fn is_empty(&self) -> bool {
        *self == Overrides::default()
    }
}
