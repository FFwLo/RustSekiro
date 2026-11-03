//! Event references and story flags for this map; original selection metadata.
pub const PROFILE: &str = r#"
{
  "seed_rules": [{"when":[8306],"set":[9810]}],
  "sub_files": {"30":"m11_02_0930"},
  "default": "prologue",
  "map": "m11_02",
  "event_sub_files": {"20":"m11_02_0920","30":"m11_02_0930"},
  "states": [
    {"flags":[8306,9810],"name":"prologue"},
    {"flags":[],"name":"morning"},
    {"flags":[9800],"name":"afternoon"},
    {"flags":[9800,9801],"name":"evening"},
    {"flags":[9800,9801,9802],"name":"night"},
    {"flags":[9800,9801,9802,9803],"name":"dawn"},
    {"flags":[9800,9801,9802,8302],"name":"invasion"},
    {"commands":[[11125860,19]],"flags":[9800,9801,9802,8302,9312],"name":"sword-saint-defeated"},
    {"commands":[[11125872,12]],"flags":[9800,9801,9802,8302],"name":"sword-saint-storm"}
  ]
}
"#;

#[cfg(test)]
mod tests {
    use crate::lighting_selection::select;

    #[test]
    #[ignore = "slow: inspect installed Reservoir environment containers and bank references"]
    fn reservoir_native_environment_census() {
        let cache = sekiro_sim::cache::root().join("raw");
        for variant in 0..6 {
            let path = cache.join(format!(
                "map/m11_02_00_00/m11_02_00_00_envmap_{variant:02}.tpf"
            ));
            let data = std::fs::read(path).unwrap();
            let textures = sekiro_formats::tpf::parse(&data).unwrap();
            let giiv: Vec<_> = textures
                .iter()
                .filter(|t| t.name.to_ascii_lowercase().contains("giiv"))
                .map(|t| &t.name)
                .collect();
            println!(
                "NATIVE environment {variant}: {} textures, GIIV {giiv:?}",
                textures.len()
            );
        }
        for file in ["0000", "0020", "0900", "0920", "0930"] {
            let data =
                std::fs::read(cache.join(format!("param/drawparam/m11_02_{file}.gparam"))).unwrap();
            let g = sekiro_formats::gparam::parse(&data).unwrap();
            for group in &g.groups {
                for param in &group.params {
                    if param.name.contains("BankId") || param.name.contains("LightId") {
                        println!("BANK m11_02_{file} {}: {:?}", param.name, param.entries);
                    }
                }
            }
        }
    }

    #[test]
    fn reservoir_installed_story_and_region_inputs() {
        let cache = sekiro_sim::cache::root();
        let map = "m11_02_00_00";
        let msb = sekiro_formats::msb::parse(
            &std::fs::read(cache.join(format!("raw/map/mapstudio/{map}.msb"))).unwrap(),
        )
        .unwrap();
        let configs: std::collections::BTreeSet<_> = msb
            .parts
            .iter()
            .filter(|p| p.kind == sekiro_formats::msb::PartKind::Collision)
            .filter_map(|p| p.gparam)
            .collect();
        assert_eq!(
            configs,
            [[0, 0, 0, 0], [100, 0, 0, 0], [110, 0, 0, 0], [120, 0, 0, 0]]
                .into_iter()
                .collect()
        );
        for (name, hour, environment, sub, file) in [
            ("prologue", 0., 0, 0, "0000"),
            ("morning", 6., 1, 0, "0000"),
            ("afternoon", 12., 2, 0, "0000"),
            ("evening", 18., 3, 0, "0000"),
            ("night", 22., 4, 0, "0000"),
            ("dawn", 2., 5, 0, "0000"),
            ("invasion", 22., 4, 20, "0020"),
            ("sword-saint-defeated", 22., 4, 20, "0920"),
            ("sword-saint-storm", 22., 4, 30, "0930"),
        ] {
            let s = select(&cache, map, Some(name), None).unwrap();
            assert_eq!(
                (s.hour, s.environment, s.sub_id),
                (hour, environment, sub),
                "{name}"
            );
            assert_eq!(s.source, format!("m11_02_{file}"));
            println!("STATE {name}: {s:?}");
            let g = sekiro_formats::gparam::parse(
                &std::fs::read(cache.join(format!("raw/param/drawparam/{}.gparam", s.source)))
                    .unwrap(),
            )
            .unwrap();
            for config in &configs {
                let id = config[0];
                println!(
                    "RENDER {name} {config:?}: {:?}",
                    crate::lighting::LightSet::from_gparam(&g, id, hour)
                );
                println!(
                    "FOG {name} {config:?}: {:?}",
                    crate::lighting::fog::GameFog::from_gparam(
                        &g,
                        id,
                        hour,
                        crate::lighting::LightSet::from_gparam(&g, id, hour).sun_toward,
                    )
                );
                for group in &g.groups {
                    if [
                        "LightSet",
                        "VolumetricFog",
                        "Tone Map",
                        "Fog",
                        "LightScattering",
                    ]
                    .iter()
                    .any(|prefix| group.name.starts_with(prefix))
                    {
                        for param in &group.params {
                            println!(
                                "RAW {name} id={id} {} / {} = {:?}",
                                group.name,
                                param.name,
                                param.at(id, hour)
                            );
                        }
                    }
                }
            }
        }
        for arena in sekiro_sim::arena::ARENAS.iter().filter(|a| a.map == map) {
            let s = select(&cache, map, None, Some(arena)).unwrap();
            assert_eq!(
                s.hour,
                arena.story_time(&arena.enemy_spec()).hour(),
                "{}",
                arena.name
            );
            println!("ARENA {}: {s:?}", arena.name);
        }
    }
}
