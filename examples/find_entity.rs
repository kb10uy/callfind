use std::{env::args, fs::read_to_string};

use anyhow::{Context as _, Result, bail};
use callfind::cty::{CtyDatabase, Scope};

fn main() -> Result<()> {
    let mut args = args().skip(1);
    let Some(path) = args.next() else {
        bail!("usage: find_entity <cty.dat> <callsign>...");
    };

    let text = read_to_string(&path).with_context(|| format!("cannot read {path}"))?;
    let cty = CtyDatabase::parse(&text).context("cannot parse the country file")?;
    eprintln!("{} entities loaded from {path}", cty.len());

    for callsign in args {
        let Some(found) = cty.lookup(&callsign) else {
            println!("{callsign}: unknown");
            continue;
        };

        let location = found.location;
        let grid = match location.grid_locator() {
            Ok(grid_locator) => format!("{grid_locator:6}"),
            Err(e) => e.to_string(),
        };
        println!(
            "{callsign}: {} [{}] via {} ({:?}) CQ{} ITU{} {} {} {:.2},{:.2} UTC{}",
            found.entity.name,
            found.entity.primary_prefix,
            found.matched,
            found.kind,
            location.cq_zone,
            location.itu_zone,
            location.continent,
            grid,
            location.latitude,
            location.longitude,
            location.utc_offset,
        );

        let waedc = cty.lookup_in(&callsign, Scope::Waedc);
        if let Some(waedc) = waedc
            && waedc.entity != found.entity
        {
            println!("    WAEDC: {}", waedc.entity.name);
        }
    }

    Ok(())
}
