use alloc::{
    string::{String, ToString},
    vec::Vec,
};

use time::UtcOffset;

use crate::cty::{
    entity::{Continent, Entity, Location, Overrides},
    error::CtyError,
};

/// The characters that open an override group after an alias.
const OVERRIDE_OPENINGS: [char; 5] = ['(', '[', '<', '{', '~'];

/// One alias of a `cty.dat` record, stripped of its override groups.
#[derive(Debug, Clone, PartialEq)]
pub struct Alias {
    /// Upper-cased callsign or prefix the alias matches.
    pub pattern: String,

    /// Whether the alias matches a whole callsign rather than a prefix.
    pub exact: bool,

    /// Values the alias replaces on the location of its entity.
    pub overrides: Overrides,
}

/// Reads the eight colon-separated fields that open a `cty.dat` record.
pub fn parse_header(header: &str) -> Result<Entity, CtyError> {
    let header = header.trim();
    let fields: Vec<&str> = header.split(':').map(str::trim).collect();
    if fields.len() < 8 {
        return Err(CtyError::MalformedHeader(header.to_string()));
    }

    let invalid = |field: &'static str| CtyError::InvalidHeaderField {
        header: header.to_string(),
        field,
    };

    let cq_zone = fields[1].parse().map_err(|_| invalid("CQ zone"))?;
    let itu_zone = fields[2].parse().map_err(|_| invalid("ITU zone"))?;
    let continent = fields[3].parse::<Continent>()?;
    let latitude = fields[4].parse().map_err(|_| invalid("latitude"))?;
    let longitude = fields[5].parse::<f64>().map_err(|_| invalid("longitude"))?;
    let utc_offset = fields[6]
        .parse::<f64>()
        .map_err(|_| invalid("UTC offset"))?;
    let primary_prefix = fields[7];

    Ok(Entity {
        name: fields[0].to_string(),
        waedc_only: primary_prefix.starts_with('*'),
        primary_prefix: primary_prefix.trim_start_matches('*').to_string(),
        location: Location {
            cq_zone,
            itu_zone,
            continent,
            latitude,
            longitude: -longitude,
            utc_offset: parse_utc_offset(utc_offset)?,
        },
    })
}

/// Reads one entry of the alias list, or nothing when the entry is blank.
pub fn parse_alias(alias: &str) -> Result<Option<Alias>, CtyError> {
    let alias = alias.trim();
    if alias.is_empty() {
        return Ok(None);
    }

    let (exact, body) = match alias.strip_prefix('=') {
        Some(rest) => (true, rest),
        None => (false, alias),
    };

    let split = body.find(OVERRIDE_OPENINGS).unwrap_or(body.len());
    let pattern = body[..split].trim();
    if pattern.is_empty() {
        return Ok(None);
    }

    Ok(Some(Alias {
        pattern: pattern.to_ascii_uppercase(),
        exact,
        overrides: parse_overrides(alias, &body[split..])?,
    }))
}

/// Converts the local time offset `cty.dat` stores, which grows towards the west.
fn parse_utc_offset(hours: f64) -> Result<UtcOffset, CtyError> {
    if !hours.is_finite() {
        return Err(CtyError::UtcOffsetOutOfRange(hours));
    }

    let scaled = -hours * 3600.0;
    let seconds = (if scaled >= 0.0 {
        scaled + 0.5
    } else {
        scaled - 0.5
    }) as i32;
    UtcOffset::from_whole_seconds(seconds).map_err(|_| CtyError::UtcOffsetOutOfRange(hours))
}

/// Reads the override groups an alias carries, which may repeat in any order.
fn parse_overrides(alias: &str, mut rest: &str) -> Result<Overrides, CtyError> {
    let malformed = || CtyError::MalformedOverride(alias.to_string());
    let invalid = |field: &'static str| CtyError::InvalidOverride {
        alias: alias.to_string(),
        field,
    };

    let mut overrides = Overrides::default();
    while !rest.is_empty() {
        let opening = rest.as_bytes()[0];
        let closing = match opening {
            b'(' => ')',
            b'[' => ']',
            b'<' => '>',
            b'{' => '}',
            b'~' => '~',
            _ => return Err(malformed()),
        };

        let end = rest[1..].find(closing).ok_or_else(malformed)? + 1;
        let value = rest[1..end].trim();
        match opening {
            b'(' => overrides.cq_zone = Some(value.parse().map_err(|_| invalid("CQ zone"))?),
            b'[' => overrides.itu_zone = Some(value.parse().map_err(|_| invalid("ITU zone"))?),
            b'{' => overrides.continent = Some(value.parse()?),
            b'~' => {
                let hours = value.parse().map_err(|_| invalid("UTC offset"))?;
                overrides.utc_offset = Some(parse_utc_offset(hours)?);
            }
            b'<' => {
                let (latitude, longitude) = value
                    .split_once('/')
                    .ok_or_else(|| invalid("coordinates"))?;
                let latitude = latitude
                    .trim()
                    .parse()
                    .map_err(|_| invalid("coordinates"))?;
                let longitude = longitude
                    .trim()
                    .parse::<f64>()
                    .map_err(|_| invalid("coordinates"))?;
                overrides.coordinates = Some((latitude, -longitude));
            }
            _ => unreachable!("opening character is already matched"),
        }

        rest = rest[end + 1..].trim_start();
    }

    Ok(overrides)
}

#[cfg(test)]
mod tests {
    use time::UtcOffset;

    use super::{parse_alias, parse_header};
    use crate::cty::entity::Continent;

    const JAPAN: &str = "Japan:  25:  45:  AS:   36.40:  -138.38:    -9.0:  JA:";

    #[test]
    fn reads_a_record_header() {
        let entity = parse_header(JAPAN).expect("header must parse");

        assert_eq!(entity.name, "Japan");
        assert_eq!(entity.primary_prefix, "JA");
        assert!(!entity.waedc_only);
        assert_eq!(entity.location.cq_zone, 25);
        assert_eq!(entity.location.itu_zone, 45);
        assert_eq!(entity.location.continent, Continent::Asia);
        assert_eq!(entity.location.longitude, 138.38);
        assert_eq!(
            entity.location.utc_offset,
            UtcOffset::from_hms(9, 0, 0).unwrap()
        );
    }

    #[test]
    fn strips_the_waedc_marker() {
        let entity = parse_header("Sicily: 15: 28: EU: 37.50: -14.00: -1.0: *IT9:")
            .expect("header must parse");

        assert!(entity.waedc_only);
        assert_eq!(entity.primary_prefix, "IT9");
    }

    #[test]
    fn rejects_a_malformed_header() {
        assert!(parse_header("Japan: 25: 45: AS:").is_err());
        assert!(parse_header("Japan: 25: 45: XX: 36.40: -138.38: -9.0: JA:").is_err());
        assert!(parse_header("Japan: 25: 45: AS: 36.40: east: -9.0: JA:").is_err());
    }

    #[test]
    fn reads_a_plain_alias() {
        let alias = parse_alias("  7k  ")
            .expect("alias must parse")
            .expect("alias must exist");

        assert_eq!(alias.pattern, "7K");
        assert!(!alias.exact);
        assert!(alias.overrides.is_empty());
    }

    #[test]
    fn reads_an_exact_alias() {
        let alias = parse_alias("=JD1BHH/6")
            .expect("alias must parse")
            .expect("alias must exist");

        assert_eq!(alias.pattern, "JD1BHH/6");
        assert!(alias.exact);
    }

    #[test]
    fn reads_every_override_group() {
        let alias = parse_alias("KH6(31)[61]<21.00/157.00>{OC}~10.0~")
            .expect("alias must parse")
            .expect("alias must exist");

        assert_eq!(alias.pattern, "KH6");
        assert_eq!(alias.overrides.cq_zone, Some(31));
        assert_eq!(alias.overrides.itu_zone, Some(61));
        assert_eq!(alias.overrides.coordinates, Some((21.00, -157.00)));
        assert_eq!(alias.overrides.continent, Some(Continent::Oceania));
        assert_eq!(
            alias.overrides.utc_offset,
            Some(UtcOffset::from_hms(-10, 0, 0).unwrap())
        );
    }

    #[test]
    fn rejects_a_malformed_override() {
        assert!(parse_alias("4U1V(4").is_err());
        assert!(parse_alias("4U1V(x)").is_err());
        assert!(parse_alias("4U1V<21.00>").is_err());
    }

    #[test]
    fn skips_a_blank_alias() {
        assert_eq!(parse_alias("   ").expect("blank alias must parse"), None);
    }
}
