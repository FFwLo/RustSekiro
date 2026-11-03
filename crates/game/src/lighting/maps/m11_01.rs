//! Event references and story flags for this map; original selection metadata.
pub const PROFILE: &str = r#"
{
  "default": "morning",
  "map": "m11_01",
  "states": [
    {"flags":[],"name":"morning"},
    {"flags":[9301],"name":"gyoubu-defeated"},
    {"flags":[9301,9303],"name":"genichiro-defeated"},
    {"flags":[9800],"name":"afternoon"},
    {"flags":[9800,9801],"name":"evening"},
    {"flags":[9800,9801,9802],"name":"night"},
    {"flags":[9800,9801,9802,9803],"name":"dawn"},
    {"flags":[9800,9801,9802,8302],"name":"invasion"},
    {"commands":[[11115820,24],[11115820,25],[11115820,28]],"flags":[11115802,11115801],"when_flags":[11115802],"name":"tomoe"},
    {"commands":[[11115910,29]],"flags":[9800,9801],"when_flags":[1270,1276],"name":"shura-fire"},
    {"commands":[[11115860,54]],"flags":[9303,9304,9305,9306,8301,11115722],"when_flags":[8301,9303,11115722],"name":"owl-fight"}
  ]
}
"#;

#[cfg(test)]
mod tests {
    use crate::{lighting::LightSet, lighting_selection};
    use sekiro_formats::{emedf, gparam, msb};
    use std::collections::BTreeSet;

    /// Snow deposits belong to the native roof, while tatami's static facet excludes snow.
    #[test]
    fn genichiro_settled_snow_selection() {
        let cache = sekiro_sim::cache::root();
        let arena = sekiro_sim::arena::ArenaSpec::by_name("genichiro").unwrap();
        let selected = lighting_selection::select(&cache, arena.map, None, Some(arena)).unwrap();
        assert_eq!(
            (selected.hour, selected.environment, selected.sub_id),
            (6., 1, 0)
        );
        assert_eq!(selected.source, "m11_01_0000");
        assert_eq!(selected.sky.as_deref(), Some("m900000"));
        assert!(!selected.flags.contains(&11115802));

        let effects = sekiro_sim::arena_b::map_effects(&cache, arena.map).unwrap();
        assert_eq!(
            effects
                .iter()
                .find(|effect| effect.entity == 1114350)
                .unwrap()
                .effect,
            839500
        );
        let events = lighting_selection::script(&cache, arena.map).unwrap();
        for (event_id, code) in [(11115800, (2006, 2)), (11115820, (2006, 1))] {
            assert!(
                events
                    .events
                    .iter()
                    .find(|event| event.id == event_id)
                    .unwrap()
                    .instructions
                    .iter()
                    .any(|instruction| instruction.code() == code
                        && instruction.i32_at(0) == Some(1114350)),
                "morning spawns snowfall; Tomoe removes it"
            );
        }

        let roof = sekiro_formats::flver::parse(
            &std::fs::read(cache.join(
                "raw/map/m11_01_00_00/m11_01_00_00_015070.mapbnd.d/m11_01_00_00_015070.flver",
            ))
            .unwrap(),
        )
        .unwrap();
        let bytes = std::fs::read(cache.join("maps/m11_01_00_00/models/m015070.glb")).unwrap();
        let json_len = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
        let exported: serde_json::Value =
            serde_json::from_slice(&bytes[20..20 + json_len]).unwrap();
        for (stem, snow_slot, has_snow) in [
            ("M[m11_01]_Asset_SnowGround_Multi", 11, true),
            ("M[m11_01]_Floor1_Multi", 12, true),
            ("M[m11_01]_tatami_multi", 12, false),
        ] {
            let native = sekiro_formats::mtd::parse(
                &std::fs::read(cache.join(format!("raw/mtd/allmaterialbnd.mtdbnd.d/{stem}.mtd")))
                    .unwrap(),
            )
            .unwrap();
            let kind = sekiro_formats::map_layers::family(&native.shader);
            let facet = sekiro_formats::map_layers::variant(&native).unwrap();
            assert_eq!(
                sekiro_formats::map_layers::slots(kind, facet).contains(&snow_slot),
                has_snow,
                "{stem}"
            );
            for (index, material) in roof
                .materials
                .iter()
                .enumerate()
                .filter(|(_, material)| material.mtd.ends_with(&format!("{stem}.mtd")))
            {
                let layers = &exported["materials"][index]["extras"]["sekiro_layers"];
                assert_eq!(
                    layers["variant"].as_u64(),
                    Some(facet as u64),
                    "{}",
                    material.name
                );
                assert_eq!(
                    layers["snow"]["uri"].is_string(),
                    has_snow,
                    "{}",
                    material.name
                );
                if has_snow {
                    assert!(sekiro_formats::map_layers::game_params(&native, kind)[14][1] > 0.);
                }
            }
            assert!(
                roof.materials
                    .iter()
                    .any(|material| material.mtd.ends_with(&format!("{stem}.mtd"))),
                "roof uses {stem}"
            );
        }
    }

    /// Validate native Castle event selection and inspect the actual renderer conversion.
    #[test]
    fn castle_installed_story_lighting() {
        let cache = sekiro_sim::cache::root();
        let map = "m11_01_00_00";
        let layout =
            msb::parse(&std::fs::read(cache.join(format!("raw/map/mapstudio/{map}.msb"))).unwrap())
                .unwrap();
        let json: serde_json::Value = serde_json::from_slice(
            &std::fs::read(cache.join("maps/m11_01_00_00/layout.json")).unwrap(),
        )
        .unwrap();
        for part in json["collisions"].as_array().unwrap() {
            let name = part["name"].as_str().unwrap();
            let config = layout
                .parts
                .iter()
                .find(|p| p.name == name)
                .unwrap()
                .gparam
                .unwrap();
            assert_eq!(
                part["light_set"].as_i64().unwrap(),
                i64::from(config[0]),
                "{name}"
            );
            assert_eq!(&config[1..], &[0, 0, 0], "{name}");
        }
        let regions: BTreeSet<_> = layout
            .parts
            .iter()
            .filter(|p| p.kind == msb::PartKind::Collision)
            .filter_map(|p| p.gparam)
            .collect();
        assert_eq!(
            regions,
            BTreeSet::from([[0, 0, 0, 0], [100, 0, 0, 0], [110, 0, 0, 0], [120, 0, 0, 0]])
        );
        for part in &layout.parts {
            if part.kind == msb::PartKind::Collision {
                println!("castle collision {}: {:?}", part.name, part.gparam);
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
                    println!("castle event {}:{index}: {} {args:?}", event.id, def.name);
                }
            }
        }
        for (name, hour, environment, sub, source) in [
            ("morning", 6., 1, 0, "m11_01_0000"),
            ("afternoon", 12., 2, 0, "m11_01_0000"),
            ("evening", 18., 3, 0, "m11_01_0000"),
            ("night", 22., 4, 0, "m11_01_0000"),
            ("dawn", 2., 5, 0, "m11_01_0000"),
            ("invasion", 22., 4, 20, "m11_01_0020"),
            ("tomoe", 18., 1, 10, "m11_01_0010"),
            ("shura-fire", 18., 3, 40, "m11_01_0740"),
            ("owl-fight", 18., 3, 50, "m11_01_0750"),
        ] {
            let selected = lighting_selection::select(&cache, map, Some(name), None).unwrap();
            println!("castle {name}: {selected:?}");
            assert_eq!(
                (selected.hour, selected.environment, selected.sub_id),
                (hour, environment, sub)
            );
            assert_eq!(selected.source, source);
            let sky = match name {
                "morning" => "m900000",
                "afternoon" => "m900010",
                "evening" | "shura-fire" | "owl-fight" => "m900020",
                "night" => "m900030",
                "dawn" => "m900050",
                "invasion" => "m900095",
                "tomoe" => "m900080",
                _ => unreachable!(),
            };
            assert_eq!(selected.sky.as_deref(), Some(sky));
            let g = gparam::parse(
                &std::fs::read(cache.join(format!("raw/param/drawparam/{source}.gparam"))).unwrap(),
            )
            .unwrap();
            for [id, _, _, _] in &regions {
                let set = LightSet::from_gparam(&g, *id, hour);
                println!("castle {name} row {id} renderer: {set:?}");
                println!(
                    "castle {name} row {id} fog: {:?}",
                    crate::lighting::fog::GameFog::from_gparam(&g, *id, hour, set.sun_toward)
                );
                for group in &g.groups {
                    if ["LightSet", "Fog", "Tone Map", "ColorGrading"]
                        .iter()
                        .any(|name| group.name.contains(name))
                    {
                        for param in &group.params {
                            println!(
                                "castle {name} row {id} native {} / {} = {:?}",
                                group.name,
                                param.name,
                                param.at(*id, hour)
                            );
                        }
                    }
                }
            }
        }
        assert_eq!(
            lighting_selection::sub_file(&cache, map, 50).unwrap(),
            "m11_01_0750"
        );
        for arena in sekiro_sim::arena::ARENAS.iter().filter(|a| a.map == map) {
            let selected = lighting_selection::select(&cache, map, None, Some(arena)).unwrap();
            assert_eq!(
                selected.hour,
                arena.story_time(&arena.enemy_spec()).hour(),
                "{}",
                arena.name
            );
            println!("castle arena {}: {selected:?}", arena.name);
            match arena.name {
                "owl" => assert_eq!(
                    (selected.source.as_str(), selected.sub_id),
                    ("m11_01_0750", 50)
                ),
                "shura" => assert_eq!(
                    (selected.source.as_str(), selected.sub_id),
                    ("m11_01_0740", 40)
                ),
                "genichiro" => assert_eq!(
                    (selected.source.as_str(), selected.sub_id),
                    ("m11_01_0000", 0)
                ),
                _ => unreachable!(),
            }
        }
        let tomoe = sekiro_sim::arena::ArenaSpec {
            enemy: "c7110:71100000",
            ..*sekiro_sim::arena::ArenaSpec::by_name("genichiro").unwrap()
        };
        let selected = lighting_selection::select(&cache, map, None, Some(&tomoe)).unwrap();
        assert_eq!(
            (selected.hour, selected.environment, selected.sub_id),
            (18., 1, 10)
        );
        assert_eq!(selected.source, "m11_01_0010");
        assert_eq!(selected.sky.as_deref(), Some("m900080"));
        println!("castle native Tomoe ArenaSpec: {selected:?}");
    }
}
