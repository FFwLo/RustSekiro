//! Read-only census of lighting instructions and collision draw-param keys.
use sekiro_formats::{emedf, emevd::Emevd, gparam, msb};
use std::{collections::BTreeSet, fs, path::Path};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let raw = Path::new("cache/raw");
    for file in fs::read_dir(raw.join("event"))? {
        let file = file?;
        let script = Emevd::parse(&fs::read(file.path())?)?;
        for event in &script.events {
            if !event
                .instructions
                .iter()
                .any(|i| matches!(i.code(), (2003, 80 | 81 | 88)))
            {
                continue;
            }
            println!("EVENT {} {}", file.file_name().to_string_lossy(), event.id);
            for (n, i) in event.instructions.iter().enumerate() {
                println!("{n}: {}", emedf::format(i));
            }
        }
    }
    for map in [
        "m10_00", "m11_00", "m11_01", "m11_02", "m13_00", "m15_00", "m17_00", "m20_00", "m25_00",
    ] {
        let layout = msb::parse(&fs::read(
            raw.join(format!("map/mapstudio/{map}_00_00.msb")),
        )?)?;
        let configs: BTreeSet<_> = layout
            .parts
            .iter()
            .filter(|p| p.kind == msb::PartKind::Collision)
            .filter_map(|p| p.gparam)
            .collect();
        println!("MAP {map} COLLISIONS {configs:?}");
        for file in fs::read_dir(raw.join("param/drawparam"))? {
            let file = file?;
            if !file.file_name().to_string_lossy().starts_with(map) {
                continue;
            }
            let g = gparam::parse(&fs::read(file.path())?)?;
            let keys: BTreeSet<_> = g
                .groups
                .iter()
                .flat_map(|g| &g.params)
                .flat_map(|p| &p.entries)
                .map(|e| (e.id, e.time as i32))
                .collect();
            let groups: Vec<_> = g.groups.iter().map(|g| &g.name).collect();
            println!(
                "FILE {} keys={keys:?} groups={groups:?}",
                file.file_name().to_string_lossy()
            );
        }
    }
    Ok(())
}
