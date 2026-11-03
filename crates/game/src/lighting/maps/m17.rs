//! Event references and story flags for this map; original selection metadata.
pub const PROFILE: &str = r#"
{
  "default": "morning",
  "map": "m17_00",
  "states": [
    {"flags":[],"name":"morning"},
    {"flags":[9800],"name":"afternoon"},
    {"flags":[9800,9801],"name":"evening"},
    {"flags":[9800,9801,9802],"name":"night"},
    {"flags":[9800,9801,9802,9803],"name":"dawn"}
  ]
}
"#;

#[cfg(test)]
mod tests {
    use crate::{lighting::LightSet, lighting::fog::GameFog, lighting_selection};
    use sekiro_formats::{emedf, gparam, msb, tpf};
    use std::collections::BTreeSet;

    /// Compare actual event execution and renderer inputs with the installed map data.
    #[test]
    fn valley_installed_story_lighting() {
        let cache = sekiro_sim::cache::root();
        let map = "m17_00_00_00";
        let layout =
            msb::parse(&std::fs::read(cache.join(format!("raw/map/mapstudio/{map}.msb"))).unwrap())
                .unwrap();
        let configs: BTreeSet<_> = layout
            .parts
            .iter()
            .filter(|p| p.kind == msb::PartKind::Collision)
            .filter_map(|p| p.gparam)
            .collect();
        assert_eq!(
            configs,
            [0, 100, 110, 120, 130]
                .map(|id| [id, 0, 0, 0])
                .into_iter()
                .collect()
        );
        for part in &layout.parts {
            if part.kind == msb::PartKind::Collision {
                println!("m17 collision {}: {:?}", part.name, part.gparam);
            }
        }
        for entry in std::fs::read_dir(cache.join(format!("raw/map/{map}"))).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy();
            if name.contains("_envmap_") && name.ends_with(".tpf") {
                let bytes = std::fs::read(&path).unwrap();
                let textures = tpf::parse(&bytes).unwrap();
                let cubes: BTreeSet<_> = textures
                    .iter()
                    .filter(|t| t.name.to_ascii_lowercase().contains("gilm"))
                    .filter_map(|t| t.name.rsplit('_').next())
                    .collect();
                println!(
                    "m17 native {name}: {} textures, GILM variants={cubes:?}",
                    textures.len()
                );
            }
        }
        let events = lighting_selection::script(&cache, map).unwrap();
        for event in &events.events {
            for (index, instruction) in event.instructions.iter().enumerate() {
                if let Some((def, args)) = emedf::decode(instruction)
                    && (def.name.contains("Gparam")
                        || def.name.contains("Lighting")
                        || def.name.contains("Envmap")
                        || def.name.contains("MapPart"))
                {
                    println!("m17 event {}:{index}: {} {args:?}", event.id, def.name);
                }
            }
        }
        for (name, hour, environment, sky) in [
            ("morning", 6., 1, "m900000"),
            ("afternoon", 12., 2, "m900010"),
            ("evening", 18., 3, "m900020"),
            ("night", 22., 4, "m900030"),
            ("dawn", 2., 5, "m900050"),
        ] {
            let selected = lighting_selection::select(&cache, map, Some(name), None).unwrap();
            assert_eq!((selected.hour, selected.environment), (hour, environment));
            assert_eq!(selected.sky.as_deref(), Some(sky));
            assert_eq!(
                (selected.source.as_str(), selected.sub_id),
                ("m17_00_0000", 0)
            );
            println!("m17 {name}: {selected:?}");
            let g = gparam::parse(
                &std::fs::read(
                    cache.join(format!("raw/param/drawparam/{}.gparam", selected.source)),
                )
                .unwrap(),
            )
            .unwrap();
            for [id, _, _, _] in &configs {
                let light = LightSet::from_gparam(&g, *id, hour);
                let native = g.param("LightSet", "DiffColor0").unwrap();
                println!(
                    "m17 {name} row {id}: authored={} renderer={light:?} fog={:?}",
                    native.entries.iter().any(|e| e.id == *id),
                    GameFog::from_gparam(&g, *id, hour, light.sun_toward)
                );
                for group in &g.groups {
                    if ["LightSet", "Fog", "Tone Map", "Bloom"]
                        .iter()
                        .any(|name| group.name.contains(name))
                    {
                        for param in &group.params {
                            println!(
                                "m17 {name} row {id}: {} / {} = {:?}",
                                group.name,
                                param.name,
                                param.at(*id, hour)
                            );
                        }
                    }
                }
            }
        }
        for arena in sekiro_sim::arena::ARENAS.iter().filter(|a| a.map == map) {
            let selected = lighting_selection::select(&cache, map, None, Some(arena)).unwrap();
            assert_eq!(selected.hour, arena.story_time(&arena.enemy_spec()).hour());
            assert_eq!((selected.hour, selected.environment), (12., 2));
            assert_eq!(selected.sky.as_deref(), Some("m900010"));
            println!("m17 arena {}: {selected:?}", arena.name);
        }
    }
}
