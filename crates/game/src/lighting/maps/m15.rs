//! Event references and story flags for this map; original selection metadata.
//! Common event 100 keeps sky group 8907015 for Depths and routes world dawn back to morning.
//! Map event 11505290 requests afternoon only after the palanquin interaction/completion flag.
pub const PROFILE: &str = r#"
{
  "default": "morning",
  "map": "m15_00",
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
    use super::super::{RegionSelection, select};
    use crate::lighting::{LightSet, fog::GameFog};
    use sekiro_formats::{gparam, msb};
    use std::collections::BTreeSet;

    #[test]
    fn depths_installed_story_and_region_lighting() {
        let cache = sekiro_sim::cache::root();
        let events = sekiro_formats::emevd::Emevd::parse(
            &std::fs::read(cache.join("raw/event/m15_00_00_00.emevd")).unwrap(),
        )
        .unwrap();
        let transition = events.event(11505290).unwrap();
        assert_eq!(transition.instructions[0].i32_at(4), Some(51500050));
        assert_eq!(transition.instructions[1].code(), (3, 24));
        assert_eq!(transition.instructions[4].code(), (2003, 81));
        assert_eq!(transition.instructions[4].u8_at(0), Some(2));
        assert!(
            !events
                .events
                .iter()
                .flat_map(|e| &e.instructions)
                .any(|i| i.code() == (2003, 88))
        );
        let g = gparam::parse(
            &std::fs::read(cache.join("raw/param/drawparam/m15_00_0000.gparam")).unwrap(),
        )
        .unwrap();
        let layout =
            msb::parse(&std::fs::read(cache.join("raw/map/mapstudio/m15_00_00_00.msb")).unwrap())
                .unwrap();
        let configs: BTreeSet<_> = layout
            .parts
            .iter()
            .filter(|p| p.kind == msb::PartKind::Collision)
            .filter_map(|p| p.gparam)
            .collect();
        for p in layout
            .parts
            .iter()
            .filter(|p| matches!(p.kind, msb::PartKind::Player | msb::PartKind::Enemy))
        {
            println!(
                "M15 placement {} entity={} position={:?} rotation={:?}",
                p.name, p.entity_id, p.position, p.rotation
            );
        }
        assert_eq!(
            configs,
            BTreeSet::from([[0; 4], [100, 0, 0, 0], [110, 0, 0, 0], [120, 0, 0, 0]])
        );
        for (state, hour, environment) in [
            ("morning", 6., 1),
            ("afternoon", 12., 2),
            ("evening", 18., 3),
            ("night", 22., 4),
            ("dawn", 6., 1),
        ] {
            let s = select(&cache, "m15_00_00_00", Some(state), None).unwrap();
            assert_eq!((s.hour, s.environment, s.sub_id), (hour, environment, 0));
            assert_eq!(s.source, "m15_00_0000");
            assert_eq!(s.sky.as_deref(), Some("m900015"));
            println!(
                "M15 state={state} hour={hour} environment={environment} sky={:?}",
                s.sky
            );
            let mut region = RegionSelection::default();
            for config in configs.iter().chain(std::iter::once(&[0; 4])) {
                region.enter(*config);
                assert_eq!(region.0, *config);
                let id = config[0];
                for group in &g.groups {
                    if [
                        "LightSet",
                        "Fog",
                        "VolumetricFog",
                        "Tone Map",
                        "ColorGrading",
                    ]
                    .iter()
                    .any(|name| group.name.starts_with(name))
                    {
                        for p in &group.params {
                            println!(
                                "M15 raw state={state} id={id} {} / {} = {:?}",
                                group.name,
                                p.name,
                                p.at(id, hour)
                            );
                        }
                    }
                }
                let light = LightSet::from_gparam(&g, id, hour);
                println!("M15 renderer state={state} id={id} light={light:?}");
                let fog = GameFog::from_gparam(&g, id, hour, light.sun_toward);
                println!("M15 renderer state={state} id={id} fog={fog:?}");
            }
        }
        for arena in sekiro_sim::arena::ARENAS
            .iter()
            .filter(|a| a.map.starts_with("m15"))
        {
            let s = select(&cache, arena.map, None, Some(arena)).unwrap();
            assert_eq!(s.hour, arena.story_time(&arena.enemy_spec()).hour());
            println!(
                "M15 arena={} hour={} environment={} sky={:?}",
                arena.name, s.hour, s.environment, s.sky
            );
        }
    }
}
