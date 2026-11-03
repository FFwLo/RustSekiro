//! Event references and story flags for this map; original selection metadata.
pub const PROFILE: &str = r#"
{
  "default": "morning",
  "map": "m11_00",
  "states": [
    {"flags":[],"name":"morning"},
    {"flags":[9301],"name":"gyoubu-defeated"},
    {"flags":[9301,9303],"name":"genichiro-defeated"},
    {"flags":[9800],"name":"afternoon"},
    {"flags":[9800,9801],"name":"evening"},
    {"flags":[9800,9801,9802],"name":"night"},
    {"flags":[9800,9801,9802,9803],"name":"dawn"},
    {"flags":[9800,9801,9802,8302],"name":"invasion"},
    {"commands":[[11115820,24],[11115820,26],[11115820,28]],"flags":[],"name":"tomoe","event_map":"m11_01_00_00"}
  ]
}
"#;

#[cfg(test)]
mod tests {
    use crate::lighting::LightSet;
    use crate::lighting_selection::{RegionSelection, select};
    use sekiro_formats::{gparam, msb};
    use std::collections::BTreeSet;

    #[test]
    fn installed_outskirts_states_regions_and_arenas() {
        let cache = sekiro_sim::cache::root();
        let layout =
            msb::parse(&std::fs::read(cache.join("raw/map/mapstudio/m11_00_00_00.msb")).unwrap())
                .unwrap();
        let configs: BTreeSet<_> = layout
            .parts
            .iter()
            .filter(|p| p.kind == msb::PartKind::Collision)
            .filter_map(|p| p.gparam)
            .collect();
        assert_eq!(
            configs,
            BTreeSet::from([[0, 0, 0, 0], [100, 0, 0, 0], [110, 0, 0, 0], [130, 0, 0, 0]])
        );
        let mut region = RegionSelection::default();
        for config in configs.iter().rev().chain(configs.iter()) {
            region.enter(*config);
            assert_eq!(region.0, *config);
        }
        for (name, hour, environment, sub_id, sky) in [
            ("morning", 6., 1, 0, "m900000"),
            ("afternoon", 12., 2, 0, "m900010"),
            ("evening", 18., 3, 0, "m900020"),
            ("night", 22., 4, 0, "m900030"),
            ("dawn", 2., 5, 0, "m900050"),
            ("invasion", 22., 4, 20, "m900095"),
            ("tomoe", 18., 1, 30, "m900080"),
        ] {
            let selected = select(&cache, "m11_00_00_00", Some(name), None).unwrap();
            assert_eq!(
                (selected.hour, selected.environment, selected.sub_id),
                (hour, environment, sub_id),
                "{name}"
            );
            assert_eq!(selected.source, format!("m11_00_00{sub_id:02}"));
            assert_eq!(selected.sky.as_deref(), Some(sky), "{name}");
            let g = gparam::parse(
                &std::fs::read(
                    cache.join(format!("raw/param/drawparam/{}.gparam", selected.source)),
                )
                .unwrap(),
            )
            .unwrap();
            println!("OUTSKIRTS {name}: {selected:?}");
            for config in &configs {
                let id = config[0];
                for group in &g.groups {
                    if !["LightSet", "Fog", "Tone Map", "ColorGrading"]
                        .iter()
                        .any(|name| group.name.contains(name))
                    {
                        continue;
                    }
                    for param in &group.params {
                        println!(
                            "RAW {name} id={id} {} / {} = {:?}",
                            group.name,
                            param.name,
                            param.at(id, hour)
                        );
                    }
                }
                println!(
                    "UPLOAD {name} id={id}: {:?}",
                    LightSet::from_gparam(&g, id, hour)
                );
            }
        }
        for (name, hour, environment, sub_id) in [("gyoubu", 6., 1, 0), ("demon", 22., 4, 20)] {
            let arena = sekiro_sim::arena::ArenaSpec::by_name(name).unwrap();
            let selected = select(&cache, arena.map, None, Some(arena)).unwrap();
            assert_eq!(selected.hour, arena.story_time(&arena.enemy_spec()).hour());
            assert_eq!(
                (selected.hour, selected.environment, selected.sub_id),
                (hour, environment, sub_id),
                "{name}"
            );
            println!("ARENA {name}: {selected:?}");
        }
    }
}
