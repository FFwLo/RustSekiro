//! Event references and story flags for this map; original selection metadata.
pub const PROFILE: &str = r#"
{
  "default": "morning",
  "map": "m13_00",
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
    use crate::{lighting::LightSet, lighting_selection};
    use sekiro_formats::{emedf, gparam, msb};
    use std::collections::BTreeSet;

    /// Exercise the installed events and authored rows, rather than a synthetic profile.
    #[test]
    fn dungeon_installed_story_lighting() {
        let cache = sekiro_sim::cache::root();
        let map = "m13_00_00_00";
        let layout =
            msb::parse(&std::fs::read(cache.join(format!("raw/map/mapstudio/{map}.msb"))).unwrap())
                .unwrap();
        let regions: BTreeSet<_> = layout
            .parts
            .iter()
            .filter(|p| p.kind == msb::PartKind::Collision)
            .filter_map(|p| p.gparam)
            .collect();
        println!("m13 installed collision selections: {regions:?}");
        for part in &layout.parts {
            if part.kind == msb::PartKind::Collision
                && let Some(config) = part.gparam
            {
                println!("m13 native collision {}: {config:?}", part.name);
            }
        }
        let events = lighting_selection::script(&cache, map).unwrap();
        for event in &events.events {
            for instruction in &event.instructions {
                if let Some((def, args)) = emedf::decode(instruction)
                    && (def.name.contains("Gparam")
                        || def.name.contains("Lighting")
                        || def.name.contains("Envmap")
                        || def.name.contains("MapPart"))
                {
                    println!("m13 event {}: {} {args:?}", event.id, def.name);
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
                ("m13_00_0000", 0)
            );
            println!("m13 {name}: {selected:?}");
            let g = gparam::parse(
                &std::fs::read(
                    cache.join(format!("raw/param/drawparam/{}.gparam", selected.source)),
                )
                .unwrap(),
            )
            .unwrap();
            for [id, _, _, _] in &regions {
                let native = g.param("LightSet", "DiffColor0").unwrap();
                println!(
                    "m13 {name} collision {id}: authored_row={} renderer={:?}",
                    native.entries.iter().any(|e| e.id == *id),
                    LightSet::from_gparam(&g, *id, hour)
                );
                for group in &g.groups {
                    if ["LightSet", "FogParam", "VolumetricFog", "Tone Map"]
                        .iter()
                        .any(|name| group.name.contains(name))
                    {
                        for param in &group.params {
                            println!(
                                "m13 {name} collision {id}: {} / {} = {:?}",
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
        }
    }
}
