//! Resolves callsigns to DXCC entities using the AD1C `cty.dat` country file.
//!
//! The file is a sequence of records terminated by `;`. Each record opens with a
//! header line of eight colon-separated fields and continues with a
//! comma-separated alias list. An alias starting with `=` matches one whole
//! callsign; every other alias matches a prefix. An alias may carry bracketed
//! groups that override the zone, coordinates, continent or UTC offset its
//! entity declares, and [`Resolution`] reports the location with those applied.
//!
//! A primary prefix starting with `*` marks a WAEDC entity that DXCC does not
//! recognise, such as Sicily or Shetland. [`Scope::Dxcc`] resolves such aliases
//! to the parent DXCC entity instead, while [`Scope::Waedc`] keeps them.
//!
//! The country file itself is not bundled. Obtain a current one from
//! <https://www.country-files.com/> and hand its contents to [`CtyDatabase::parse`].
//!
//! # Examples
//!
//! ```
//! use callfind::cty::CtyDatabase;
//!
//! let cty = CtyDatabase::parse(
//!     "Japan: 25: 45: AS: 36.40: -138.38: -9.0: JA:\n    7J,7K,7L,7M,7N,8J,8N,JA,JE,JF,JG,JH,JI,JJ,JK,JL,JM,JN,JO,JP,JQ,JR,JS;",
//! )?;
//!
//! let found = cty.lookup("JL1HIS").expect("JL1HIS must resolve");
//! assert_eq!(found.entity.primary_prefix, "JA");
//! assert_eq!(found.location.cq_zone, 25);
//! assert_eq!(format!("{:6}", found.location.grid_locator()?), "PM96ej");
//! # Ok::<(), anyhow::Error>(())
//! ```

mod entity;
mod error;
mod parse;

use std::collections::{HashMap, hash_map::Entry as MapEntry};

use crate::cty::parse::{Alias, parse_alias, parse_header};

pub use crate::cty::{
    entity::{Continent, Entity, Location, Overrides},
    error::CtyError,
};

/// Segments that indicate an operating condition rather than a location.
const NON_LOCATION_SEGMENTS: &[&str] = &["P", "M", "MM", "AM", "QRP", "A", "LH"];

/// The award programme a lookup resolves for.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Scope {
    /// Folds a WAEDC-only entity into the DXCC entity that surrounds it.
    #[default]
    Dxcc,

    /// Keeps a WAEDC-only entity, which CQ-sponsored contests count separately.
    Waedc,
}

/// How a callsign matched the alias that resolved it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MatchKind {
    /// The alias is a whole callsign, written with a leading `=`.
    Callsign,

    /// The alias is a prefix.
    Prefix,
}

/// The entity a callsign resolves to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Resolution<'a> {
    /// Entity the callsign belongs to.
    pub entity: &'a Entity,

    /// Alias that matched, as the file writes it without its override groups.
    pub matched: &'a str,

    /// Whether the alias matched a whole callsign or a prefix.
    pub kind: MatchKind,

    /// Location of the entity with the overrides of the matching alias applied.
    pub location: Location,
}

/// The entity an alias points at, together with what it overrides.
#[derive(Debug, Clone)]
struct AliasTarget {
    entity: usize,
    overrides: Overrides,
}

/// Every entity one alias resolves to, kept per [`Scope`].
///
/// The file may list the same alias twice, as `4U1VIC` appears under both
/// Vienna Intl Ctr and Austria, in which case the first record wins within each
/// scope.
#[derive(Debug, Clone)]
struct AliasEntry {
    waedc: AliasTarget,
    dxcc: Option<AliasTarget>,
}

impl AliasEntry {
    fn target(&self, scope: Scope) -> Option<&AliasTarget> {
        match scope {
            Scope::Dxcc => self.dxcc.as_ref(),
            Scope::Waedc => Some(&self.waedc),
        }
    }
}

/// Holds every DXCC entity in a `cty.dat` file and resolves callsigns against them.
#[derive(Debug, Clone)]
pub struct CtyDatabase {
    entities: Vec<Entity>,
    callsigns: HashMap<String, AliasEntry>,
    prefixes: HashMap<String, AliasEntry>,
    longest_prefix: usize,
}

impl CtyDatabase {
    /// Parses the contents of a `cty.dat` file.
    pub fn parse(text: &str) -> Result<CtyDatabase, CtyError> {
        let mut database = CtyDatabase {
            entities: Vec::new(),
            callsigns: HashMap::new(),
            prefixes: HashMap::new(),
            longest_prefix: 0,
        };

        for record in text.split(';') {
            let record = record.trim();
            if record.is_empty() {
                continue;
            }

            let (header, aliases) = record.split_once('\n').unwrap_or((record, ""));
            let entity = parse_header(header)?;
            let waedc_only = entity.waedc_only;
            let index = database.entities.len();
            database.entities.push(entity);

            for alias in aliases.split(',') {
                let Some(alias) = parse_alias(alias)? else {
                    continue;
                };

                if alias.exact {
                    insert_alias(&mut database.callsigns, alias, index, waedc_only);
                } else {
                    database.longest_prefix = database.longest_prefix.max(alias.pattern.len());
                    insert_alias(&mut database.prefixes, alias, index, waedc_only);
                }
            }
        }

        Ok(database)
    }

    /// Returns the DXCC entity a callsign belongs to.
    pub fn lookup(&self, callsign: &str) -> Option<Resolution<'_>> {
        self.lookup_in(callsign, Scope::Dxcc)
    }

    /// Returns the entity a callsign belongs to within the given scope.
    ///
    /// A callsign alias wins over a prefix alias, and a longer prefix wins over
    /// a shorter one.
    pub fn lookup_in(&self, callsign: &str, scope: Scope) -> Option<Resolution<'_>> {
        let callsign = callsign.trim().to_ascii_uppercase();
        if callsign.is_empty() || !callsign.is_ascii() {
            return None;
        }

        let found = self.resolve(&self.callsigns, &callsign, scope, MatchKind::Callsign);
        if found.is_some() {
            return found;
        }

        let segment = effective_segment(&callsign);
        let found = self.resolve(&self.callsigns, segment, scope, MatchKind::Callsign);
        if found.is_some() {
            return found;
        }

        let limit = segment.len().min(self.longest_prefix);
        (1..=limit).rev().find_map(|length| {
            self.resolve(&self.prefixes, &segment[..length], scope, MatchKind::Prefix)
        })
    }

    /// Returns every entity in the order the file declares them.
    pub fn entities(&self) -> &[Entity] {
        &self.entities
    }

    /// Returns the number of entities loaded, WAEDC-only ones included.
    pub fn len(&self) -> usize {
        self.entities.len()
    }

    /// Returns whether no entity was loaded.
    pub fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }

    fn resolve<'a>(
        &'a self,
        aliases: &'a HashMap<String, AliasEntry>,
        key: &str,
        scope: Scope,
        kind: MatchKind,
    ) -> Option<Resolution<'a>> {
        let (matched, entry) = aliases.get_key_value(key)?;
        let target = entry.target(scope)?;
        let entity = &self.entities[target.entity];

        Some(Resolution {
            entity,
            matched,
            kind,
            location: target.overrides.apply(entity.location),
        })
    }
}

/// Records one alias, leaving an earlier record of the same alias in place.
fn insert_alias(
    aliases: &mut HashMap<String, AliasEntry>,
    alias: Alias,
    entity: usize,
    waedc_only: bool,
) {
    let target = AliasTarget {
        entity,
        overrides: alias.overrides,
    };

    match aliases.entry(alias.pattern) {
        MapEntry::Occupied(mut occupied) => {
            let entry = occupied.get_mut();
            if entry.dxcc.is_none() && !waedc_only {
                entry.dxcc = Some(target);
            }
        }
        MapEntry::Vacant(vacant) => {
            vacant.insert(AliasEntry {
                dxcc: (!waedc_only).then(|| target.clone()),
                waedc: target,
            });
        }
    }
}

/// Picks the slash-separated segment that identifies the operating location.
///
/// Segments describing an operating condition are dropped. When more than one
/// segment survives, the shorter one is the location prefix, because a home
/// callsign is longer than the prefix placed in front of it.
pub fn effective_segment(callsign: &str) -> &str {
    let mut segments = callsign.split('/').filter(|segment| {
        !segment.is_empty()
            && !NON_LOCATION_SEGMENTS.contains(segment)
            && !segment.chars().all(|c| c.is_ascii_digit())
    });

    let Some(first) = segments.next() else {
        return callsign.split('/').next().unwrap_or(callsign);
    };

    segments.fold(first, |shortest, segment| {
        if segment.len() < shortest.len() {
            segment
        } else {
            shortest
        }
    })
}

#[cfg(test)]
mod tests {
    use time::UtcOffset;

    use super::{Continent, CtyDatabase, MatchKind, Scope, effective_segment};

    const SAMPLE: &str = "\
Japan:                    25:  45:  AS:   36.40:  -138.38:    -9.0:  JA:
    7J,7K,7L,7M,7N,8J,8N,JA,JE,JF,JG,JH,JI,JJ,JK,JL,JM,JN,JO,JP,JQ,JR,JS,
    =JD1BHH/6;
Ogasawara:                27:  45:  AS:   27.05:  -142.20:    -9.0:  JD/o:
    JD1,=8N1OGA;
Sicily:                   15:  28:  EU:   37.50:   -14.00:    -1.0:  *IT9:
    IT9;
Italy:                    15:  28:  EU:   42.83:   -12.50:    -1.0:  I:
    I,IT,IG9(33)[37]{AF};
Vienna Intl Ctr:          15:  28:  EU:   48.20:   -16.30:    -1.0:  *4U1V:
    =4U1VIC;
Austria:                  15:  28:  EU:   47.33:   -13.33:    -1.0:  OE:
    OE,=4U1VIC;
Nepal:                    22:  42:  AS:   27.70:   -85.33:   -5.75:  9N:
    9N;
Hawaii:                   31:  61:  OC:   21.12:   157.48:    10.0:  KH6:
    KH6,NH6<21.00/157.00>~10.0~;
";

    fn database() -> CtyDatabase {
        CtyDatabase::parse(SAMPLE).expect("sample must parse")
    }

    #[test]
    fn picks_location_segment() {
        assert_eq!(effective_segment("JL1HIS"), "JL1HIS");
        assert_eq!(effective_segment("JL1HIS/P"), "JL1HIS");
        assert_eq!(effective_segment("JL1HIS/1"), "JL1HIS");
        assert_eq!(effective_segment("W1AW/KH6"), "KH6");
        assert_eq!(effective_segment("JD1/JA1XYZ"), "JD1");
        assert_eq!(effective_segment("F/JA1XYZ/P"), "F");
    }

    #[test]
    fn resolves_entities() {
        let cty = database();
        let entity_of = |call| {
            cty.lookup(call)
                .map(|found| found.entity.primary_prefix.as_str())
        };

        assert_eq!(entity_of("JL1HIS"), Some("JA"));
        assert_eq!(entity_of("7L4ABC"), Some("JA"));
        assert_eq!(entity_of("JD1ABC"), Some("JD/o"));
        assert_eq!(entity_of("8N1OGA"), Some("JD/o"));
        assert_eq!(entity_of("JD1BHH/6"), Some("JA"));
        assert_eq!(entity_of("W1AW/KH6"), Some("KH6"));
        assert_eq!(cty.lookup("QQ9XYZ"), None);
        assert_eq!(cty.lookup(""), None);
    }

    #[test]
    fn reports_how_an_alias_matched() {
        let cty = database();

        let found = cty.lookup("8N1OGA").expect("8N1OGA must resolve");
        assert_eq!(found.kind, MatchKind::Callsign);
        assert_eq!(found.matched, "8N1OGA");

        let found = cty.lookup("JA1XYZ").expect("JA1XYZ must resolve");
        assert_eq!(found.kind, MatchKind::Prefix);
        assert_eq!(found.matched, "JA");
    }

    #[test]
    fn flips_the_signs_cty_dat_stores() {
        let cty = database();

        let japan = cty.lookup("JL1HIS").expect("JL1HIS must resolve").location;
        assert_eq!(japan.latitude, 36.40);
        assert_eq!(japan.longitude, 138.38);
        assert_eq!(japan.utc_offset, UtcOffset::from_hms(9, 0, 0).unwrap());

        let hawaii = cty.lookup("KH6XYZ").expect("KH6XYZ must resolve").location;
        assert_eq!(hawaii.longitude, -157.48);
        assert_eq!(hawaii.utc_offset, UtcOffset::from_hms(-10, 0, 0).unwrap());

        let nepal = cty.lookup("9N1XYZ").expect("9N1XYZ must resolve").location;
        assert_eq!(nepal.utc_offset, UtcOffset::from_hms(5, 45, 0).unwrap());
    }

    #[test]
    fn calculates_grid_locator_of_an_entity() {
        let cty = database();
        let japan = cty.lookup("JL1HIS").expect("JL1HIS must resolve").location;
        let grid_locator = japan.grid_locator().expect("coordinates must be in range");

        assert_eq!(format!("{grid_locator:4}"), "PM96");
        assert_eq!(format!("{grid_locator:6}"), "PM96ej");
    }

    #[test]
    fn applies_alias_overrides() {
        let cty = database();

        let italy = cty.lookup("IG9ABC").expect("IG9ABC must resolve");
        assert_eq!(italy.entity.name, "Italy");
        assert_eq!(italy.entity.location.cq_zone, 15);
        assert_eq!(italy.location.cq_zone, 33);
        assert_eq!(italy.location.itu_zone, 37);
        assert_eq!(italy.location.continent, Continent::Africa);
        assert_eq!(italy.location.latitude, italy.entity.location.latitude);

        let hawaii = cty.lookup("NH6XYZ").expect("NH6XYZ must resolve").location;
        assert_eq!((hawaii.latitude, hawaii.longitude), (21.00, -157.00));
        assert_eq!(hawaii.utc_offset, UtcOffset::from_hms(-10, 0, 0).unwrap());
    }

    #[test]
    fn folds_waedc_entities_into_dxcc() {
        let cty = database();
        let dxcc = |call| cty.lookup(call).map(|found| found.entity.name.as_str());
        let waedc = |call| {
            cty.lookup_in(call, Scope::Waedc)
                .map(|found| found.entity.name.as_str())
        };

        assert_eq!(dxcc("IT9ABC"), Some("Italy"));
        assert_eq!(waedc("IT9ABC"), Some("Sicily"));
        assert_eq!(dxcc("4U1VIC"), Some("Austria"));
        assert_eq!(waedc("4U1VIC"), Some("Vienna Intl Ctr"));
    }

    #[test]
    fn keeps_every_entity_the_file_declares() {
        let cty = database();

        assert_eq!(cty.len(), 8);
        assert_eq!(cty.entities().iter().filter(|e| e.waedc_only).count(), 2);
        assert_eq!(cty.entities()[2].primary_prefix, "IT9");
    }
}
