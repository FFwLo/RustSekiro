//! Event references and story flags for this map; original selection metadata.
pub const PROFILE: &str = r#"
{
  "default": "palace",
  "map": "m25_00",
  "fixed_environment": true,
  "states": [
    {"flags":[],"name":"palace"},
    {"flags":[9309,9310],"name":"dragon-defeated"}
  ]
}
"#;

#[cfg(test)]
mod tests {

    use crate::lighting_selection::select;

    #[test]
    fn palace_and_dragon_defeat_keep_native_environment_and_arena_states() {
        let cache = sekiro_sim::cache::root();
        let map = "m25_00_00_00";
        let palace = select(&cache, map, None, None).unwrap();
        let defeated = select(&cache, map, Some("dragon-defeated"), None).unwrap();
        let script = super::super::script(&cache, map).unwrap();
        assert!(
            script
                .event(12505880)
                .unwrap()
                .instructions
                .iter()
                .any(|i| {
                    sekiro_formats::emedf::format(i) == "SetEventFlag(flag=9310, state=1)"
                })
        );
        assert!(
            defeated.flags.contains(&9310),
            "Divine Dragon defeat must use the native flag, not True Monk's 9309"
        );
        assert_eq!(defeated.source, palace.source);
        assert_eq!(defeated.sub_id, palace.sub_id);
        assert_eq!(defeated.environment, palace.environment);
        assert_eq!(defeated.sky, palace.sky);
        for arena in sekiro_sim::arena::ARENAS.iter().filter(|a| a.map == map) {
            let selection = select(&cache, map, None, Some(arena)).unwrap();
            assert_eq!(
                selection.hour,
                arena.story_time(&arena.enemy_spec()).hour(),
                "{}",
                arena.name
            );
            assert_eq!(selection.environment, palace.environment, "{}", arena.name);
            assert!(
                !selection.flags.contains(&9310),
                "{} must not start defeated",
                arena.name
            );
        }
    }
}
