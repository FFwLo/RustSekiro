//! Event references and story flags for this map; original selection metadata.
pub const PROFILE: &str = r#"
{
  "default": "memory",
  "map": "m10_00",
  "fixed_environment": true,
  "states": [
    {"flags":[],"name":"memory"},
    {"flags":[8304],"name":"father-memory"}
  ]
}
"#;

#[cfg(test)]
mod tests {
    use crate::lighting::{LightSet, fog::GameFog};
    use crate::lighting_selection::{RegionSelection, select};
    use sekiro_formats::{emedf, emevd::Emevd, gparam, msb};
    use std::collections::BTreeSet;

    #[test]
    fn hirata_installed_memory_states_regions_and_arenas() {
        let cache = sekiro_sim::cache::root();
        let map = "m10_00_00_00";
        let events =
            Emevd::parse(&std::fs::read(cache.join(format!("raw/event/{map}.emevd"))).unwrap())
                .unwrap();
        for event in &events.events {
            for (index, instruction) in event.instructions.iter().enumerate() {
                let text = emedf::format(instruction);
                if matches!(instruction.code(), (2003, 80 | 81 | 88)) || text.contains("8304") {
                    println!("M10 event={} instruction={index} {text}", event.id);
                }
            }
        }
        assert!(
            !events
                .events
                .iter()
                .flat_map(|e| &e.instructions)
                .any(|i| i.code() == (2003, 88)),
            "A new native sub-ID command requires a profile audit"
        );
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
            BTreeSet::from([[0; 4], [100, 0, 0, 0], [110, 0, 0, 0], [120, 0, 0, 0]])
        );
        for p in layout
            .parts
            .iter()
            .filter(|p| p.kind == msb::PartKind::Player)
        {
            println!(
                "M10 start={} position={:?} rotation={:?}",
                p.name, p.position, p.rotation
            );
        }
        let g = gparam::parse(
            &std::fs::read(cache.join("raw/param/drawparam/m10_00_0000.gparam")).unwrap(),
        )
        .unwrap();
        assert!(
            g.groups
                .iter()
                .flat_map(|g| &g.params)
                .flat_map(|p| &p.entries)
                .all(|e| e.time == 0.)
        );
        for name in ["memory", "father-memory"] {
            let selection = select(&cache, map, Some(name), None).unwrap();
            assert_eq!(
                (selection.hour, selection.environment, selection.sub_id),
                (0., 0, 0)
            );
            assert_eq!(selection.source, "m10_00_0000");
            assert_eq!(selection.sky.as_deref(), Some("m900060"));
            println!("M10 state={name} {selection:?}");
            let mut region = RegionSelection::default();
            for config in configs.iter().chain(std::iter::once(&[0; 4])) {
                region.enter(*config);
                assert_eq!(region.0, *config);
                let id = config[0];
                for group in &g.groups {
                    if ["LightSet", "Fog", "VolumetricFog", "Tone", "Color"]
                        .iter()
                        .any(|s| group.name.contains(s))
                    {
                        for p in &group.params {
                            println!(
                                "M10 raw state={name} id={id} {} / {} = {:?}",
                                group.name,
                                p.name,
                                p.at(id, selection.hour)
                            );
                        }
                    }
                }
                let light = LightSet::from_gparam(&g, id, selection.hour);
                println!(
                    "M10 renderer state={name} id={id} light={light:?} fog={:?}",
                    GameFog::from_gparam(&g, id, selection.hour, light.sun_toward)
                );
            }
        }
        let first = select(&cache, map, None, None).unwrap();
        assert!(!first.flags.contains(&8304));
        assert!(
            select(&cache, map, Some("father-memory"), None)
                .unwrap()
                .flags
                .contains(&8304)
        );
        for arena in sekiro_sim::arena::ARENAS
            .iter()
            .filter(|a| a.map.starts_with("m10_00"))
        {
            let selection = select(&cache, arena.map, None, Some(arena)).unwrap();
            assert_eq!(selection.hour, arena.story_time(&arena.enemy_spec()).hour());
            assert_eq!((selection.environment, selection.sub_id), (0, 0));
            for flag in arena.beaten {
                assert!(selection.flags.contains(flag));
            }
            if let Some(events) = sekiro_sim::arena::boss_events(&arena.enemy_spec()) {
                for flag in events.flags {
                    assert!(selection.flags.contains(flag));
                }
            }
            println!("M10 arena={} {selection:?}", arena.name);
        }
    }
}
