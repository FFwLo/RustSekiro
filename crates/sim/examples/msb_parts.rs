//! `msb_parts <map id> [name prefix]`: lists the parts of a map layout whose name starts with the
//! prefix, with the entity and per-part flags that decide whether the game shows them. Run from
//! the repo root, for example `cargo run -p sekiro-sim --example msb_parts -- m11_01_00_00 m01507`.
use sekiro_formats::msb;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let map = args.get(1).map_or("m11_00_00_00", String::as_str);
    let prefix = args.get(2).map_or("", String::as_str);
    let path = format!("cache/raw/map/mapstudio/{map}.msb");
    let msb = msb::parse(&std::fs::read(&path).expect("map layout")).expect("parse");
    for part in msb.parts.iter().filter(|p| p.name.starts_with(prefix)) {
        let groups: Vec<i32> = part
            .entity_group_ids
            .iter()
            .copied()
            .filter(|&g| g > 0)
            .collect();
        println!(
            "{:<20} {:?} entity={} groups={groups:?} shadow_only={} lod={} gparam={:?} \
             pos=[{:.2}, {:.2}, {:.2}]",
            part.name,
            part.kind,
            part.entity_id,
            part.is_shadow_only,
            part.lod_param_id,
            part.gparam,
            part.position[0],
            part.position[1],
            part.position[2],
        );
    }
}
