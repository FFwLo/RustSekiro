//! Event references and story flags for this map; original selection metadata.
//! Entry keeps the illusion sky enabled even when 62000705 suppresses sub-ID 10.
pub const PROFILE: &str = r#"
{
  "default": "morning",
  "map": "m20_00",
  "states": [
    {"flags":[],"name":"morning"},
    {"flags":[9800],"name":"afternoon"},
    {"flags":[9800,9801],"name":"evening"},
    {"flags":[9800,9801,9802],"name":"night"},
    {"flags":[9800,9801,9802,9803],"name":"dawn"},
    {"name":"illusion","flags":[8400,9303],"when_flags":[8400],"commands":[[12005210,9],[12005210,25]],"events":[12005826]},
    {"name":"illusion-cleared","flags":[8400,9303,62000705],"commands":[[12005210,25],[12005855,12]]}
  ]
}
"#;

#[cfg(test)]
mod tests {
    use crate::{lighting::LightSet, lighting_selection};
    use sekiro_formats::{emedf, gparam, msb};
    use std::collections::BTreeSet;

    #[test]
    fn senpou_native_illusion_region_round_trip() {
        use sekiro_formats::reader::{Reader, utf16z};
        use sekiro_sim::events::{EventEntry, EventScripts};
        let cache = sekiro_sim::cache::root();
        let map = "m20_00_00_00";
        let bytes = std::fs::read(cache.join(format!("raw/map/mapstudio/{map}.msb"))).unwrap();
        let mut table = 0x10;
        let (inside, outside) = loop {
            assert_ne!(table, 0, "native region table missing");
            let mut r = Reader::at(&bytes, table);
            r.i32().unwrap();
            let count = r.i32().unwrap();
            let name = r.offset64().unwrap();
            let entries: Vec<_> = (1..count).map(|_| r.offset64().unwrap()).collect();
            table = r.offset64().unwrap();
            if utf16z(&bytes, name).unwrap() != "POINT_PARAM_ST" {
                continue;
            }
            let at = entries
                .into_iter()
                .find(|at| {
                    let base = Reader::at(&bytes, at + 0x50).offset64().unwrap();
                    base > 0 && Reader::at(&bytes, at + base + 4).i32().unwrap() == 2002880
                })
                .unwrap();
            assert_eq!(Reader::at(&bytes, at + 0x10).u32().unwrap(), 5);
            let [x, y, z] = Reader::at(&bytes, at + 0x14).vec3().unwrap();
            let shape = Reader::at(&bytes, at + 0x48).offset64().unwrap();
            let [width, depth, height] = Reader::at(&bytes, at + shape).vec3().unwrap();
            let inside = [-x, y + height * 0.5, z];
            let outside = [-x + width + depth + height, inside[1], z];
            break (inside, outside);
        };
        let file = lighting_selection::script(&cache, map).unwrap();
        let mut scripts = EventScripts::new(
            &file,
            &[EventEntry {
                id: 12005826,
                from_label: None,
            }],
        )
        .unwrap();
        let initial = lighting_selection::select(&cache, map, Some("illusion"), None).unwrap();
        scripts.flags.extend(initial.flags.iter().copied());
        let mut library =
            lighting_selection::LightingSelection::load(&cache, map, initial, false).unwrap();
        let mut light = crate::lighting::MapLighting {
            source: library.initial.source.clone(),
            gparam: gparam::parse(
                &std::fs::read(cache.join("raw/param/drawparam/m20_00_0010.gparam")).unwrap(),
            )
            .unwrap(),
            hour: 12.,
            light_set: 0,
            applied: Some(0),
        };
        // Installed region 2002880 is a bottom-centred box in the hidden temple.
        for (position, sub) in [(inside, 0), (outside, 10), (inside, 0)] {
            let host = lighting_selection::PositionHost {
                cache: &cache,
                map,
                position,
            };
            for _ in 0..4 {
                scripts.step(1. / 60., &host);
            }
            let selected = scripts.cast_scene.gparam_sub_ids[&(20, 0)].0;
            println!("m20 region focus={position:?}: sub={selected}");
            assert_eq!(selected, sub);
            library.apply_sub(Some(selected), &mut light);
            assert_eq!(light.source, format!("m20_00_00{sub:02}"));
        }
        scripts.flags.insert(62000705);
        for _ in 0..4 {
            scripts.step(
                1. / 60.,
                &lighting_selection::PositionHost {
                    cache: &cache,
                    map,
                    position: outside,
                },
            );
        }
        assert_eq!(scripts.cast_scene.gparam_sub_ids[&(20, 0)].0, 0);
    }

    #[test]
    fn senpou_installed_story_lighting() {
        let cache = sekiro_sim::cache::root();
        let map = "m20_00_00_00";
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
            BTreeSet::from([0, 100, 110, 120, 130].map(|id| [id, 0, 0, 0]))
        );
        let events = lighting_selection::script(&cache, map).unwrap();
        for id in [12005210, 12005826, 12005855] {
            for (index, instruction) in events.event(id).unwrap().instructions.iter().enumerate() {
                println!("m20 event {id}:{index}: {}", emedf::format(instruction));
            }
        }
        for (name, hour, environment, sub, sky) in [
            ("morning", 6., 1, 0, "m900000"),
            ("afternoon", 12., 2, 0, "m900010"),
            ("evening", 18., 3, 0, "m900020"),
            ("night", 22., 4, 0, "m900030"),
            ("dawn", 2., 5, 0, "m900050"),
            ("illusion", 12., 2, 10, "m900085"),
            ("illusion-cleared", 12., 2, 0, "m900085"),
        ] {
            let s = lighting_selection::select(&cache, map, Some(name), None).unwrap();
            println!("m20 {name}: {s:?}");
            assert_eq!((s.hour, s.environment, s.sub_id), (hour, environment, sub));
            let g = gparam::parse(
                &std::fs::read(cache.join(format!("raw/param/drawparam/{}.gparam", s.source)))
                    .unwrap(),
            )
            .unwrap();
            for [id, _, _, _] in &configs {
                let light = LightSet::from_gparam(&g, *id, hour);
                println!("m20 {name} id={id} renderer={light:?}");
                println!(
                    "m20 {name} id={id} fog_upload={:?}",
                    crate::lighting::fog::GameFog::from_gparam(&g, *id, hour, light.sun_toward)
                );
                for group in &g.groups {
                    if ["LightSet", "Fog", "Tone Map", "ColorGrading"]
                        .iter()
                        .any(|name| group.name.contains(name))
                    {
                        for param in &group.params {
                            println!(
                                "m20 {name} id={id}: {} / {} = {:?}",
                                group.name,
                                param.name,
                                param.at(*id, hour)
                            );
                        }
                    }
                }
            }
            assert_eq!(s.sky.as_deref(), Some(sky), "{name}");
        }
        for arena in sekiro_sim::arena::ARENAS.iter().filter(|a| a.map == map) {
            let s = lighting_selection::select(&cache, map, None, Some(arena)).unwrap();
            println!("m20 arena {}: {s:?}", arena.name);
            assert_eq!(s.hour, arena.story_time(&arena.enemy_spec()).hour());
            assert!(arena.beaten.iter().all(|flag| s.flags.contains(flag)));
            if arena.name == "senpou-screens" {
                assert_eq!((s.environment, s.sub_id), (2, 10));
                assert_eq!(s.sky.as_deref(), Some("m900085"));
            }
        }
    }
}
