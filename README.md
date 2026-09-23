# Callfind

A Rust library for resolving amateur radio callsigns to DXCC entities and generating Maidenhead grid locators from geographic coordinates.

## Features

- Parse AD1C `cty.dat` country files and resolve callsigns to entities.
- Match exact callsigns and longest prefixes, with handling for portable operation notation.
- Choose between DXCC and WAEDC lookup scopes.
- Retrieve CQ and ITU zones, continent, representative coordinates, and UTC offset, including alias-specific overrides.
- Generate grid locators of up to 10 characters and parse 4-, 6-, 8-, or 10-character locators.

## Setup

Rust and Cargo are required. This crate uses Rust edition 2024.

To use a local checkout of Callfind from another project, add it to that project's `Cargo.toml`, adjusting the path as needed:

```toml
[dependencies]
callfind = { path = "../callfind" }
```

## Resolve a callsign

The country file is not bundled. Obtain `cty.dat` from [Country Files](https://www.country-files.com/) and pass its contents to `CtyDatabase::parse`. The library does not download or update it automatically.

```rust
use std::{error::Error, fs};

use callfind::cty::{CtyDatabase, Scope};

fn main() -> Result<(), Box<dyn Error>> {
    let text = fs::read_to_string("cty.dat")?;
    let database = CtyDatabase::parse(&text)?;

    if let Some(found) = database.lookup("JL1HIS") {
        println!("{} [{}]", found.entity.name, found.entity.primary_prefix);
        println!("CQ{} / ITU{}", found.location.cq_zone, found.location.itu_zone);
        println!("Grid: {:6}", found.location.grid_locator()?);
    }

    // Resolve using the WAEDC scope.
    if let Some(found) = database.lookup_in("IT9ABC", Scope::Waedc) {
        println!("WAEDC: {}", found.entity.name);
    }

    Ok(())
}
```

`lookup` uses the DXCC scope and returns `None` if no match is found. It trims surrounding whitespace and ignores ASCII letter case. Exact callsign aliases take precedence over prefix aliases; otherwise, the longest matching prefix wins.

For slash-separated callsigns such as `F/JA1XYZ/P` or `W1AW/KH6`, the lookup ignores operating-condition segments (`P`, `M`, `MM`, `AM`, `QRP`, `A`, and `LH`) and numeric-only segments, then uses the shortest remaining segment as the operating location. This is a heuristic; results depend on both these rules and the supplied country file.

In a lookup result, `entity.location` contains the entity's representative location, while `location` includes any overrides from the matched alias. These coordinates and their derived grid locator describe a representative location, not the station's actual position. Longitude is positive eastward, latitude is positive northward, and the UTC offset is positive for time zones ahead of UTC.

## Work with grid locators

```rust
use callfind::grid_locator::GridLocator;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Arguments are longitude, then latitude, in degrees.
    let grid = GridLocator::from_lnglat(138.38, 36.40)?;
    assert_eq!(format!("{grid:4}"), "PM96");
    assert_eq!(format!("{grid:6}"), "PM96ej");
    println!("{grid}"); // Defaults to 10 characters for generated locators.

    let parsed: GridLocator = "pm96EJ".parse()?;
    assert_eq!(parsed.to_string(), "PM96ej");

    Ok(())
}
```

`from_lnglat` accepts `-180 <= longitude < 180` and `-90 <= latitude < 90`. Out-of-range or non-finite values return an error.

Use the formatting width to select the displayed length: `{:4}`, `{:6}`, `{:8}`, or `{:10}`. Locators parsed from strings retain their original length.

## Run the examples

Run these commands from the repository root:

```sh
# Look up multiple callsigns using a country file.
cargo run --example find_entity -- ./cty.dat JL1HIS IT9ABC W1AW/KH6

# Generate grid locators from longitude,latitude pairs.
cargo run --example calc_grid -- 138.38,36.40 -157.48,21.12
```

`find_entity` prints the entity name, matched alias, zones, grid locator, coordinates, and UTC offset. It also prints the WAEDC entity when it differs from the DXCC result.

## Development

```sh
cargo build
cargo test
cargo doc --open
```

## License

Licensed under either the [MIT License](LICENSE.MIT) or the [Apache License 2.0](LICENSE), at your option.
