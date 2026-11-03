//! Audit the installed blend data without changing simulation timing.
use sekiro_formats::hkb::{NodeKind, parse};
use sekiro_sim::tae::TaeDb;

#[test]
fn installed_transition_effects_and_tae_blends() {
    let cache = sekiro_sim::cache::root();
    for (chr, graph_name) in [
        ("c0000", "c0000"),
        ("c5060", "c9997"),
        ("c1010", "c9997"),
        ("c7100", "c9997"),
    ] {
        let path = cache.join(format!("raw/chr/{chr}.behbnd.d/{graph_name}.hkx"));
        let Ok(bytes) = std::fs::read(path) else {
            eprintln!("skipping {chr}: graph not extracted");
            continue;
        };
        let graph = parse(&bytes).unwrap();
        for (id, effect) in graph.effects.iter().enumerate() {
            eprintln!(
                "{chr} effect {id} {} {} duration={:?} curve={:?} choices={:?}",
                effect.type_name, effect.name, effect.duration, effect.blend_curve, effect.choices
            );
            if let Some(duration) = effect.duration {
                assert_eq!(duration, 0.0);
                assert_eq!(effect.blend_curve, Some(0));
            }
        }
        for node in &graph.nodes {
            match &node.kind {
                NodeKind::Cmsg(c) if (3000..3100).contains(&c.anim_id) => {
                    eprintln!(
                        "{chr} CMSG {} id={} generatorChanged={:?}",
                        node.name, c.anim_id, c.generator_changed_effect
                    );
                }
                NodeKind::StateMachine(sm) => {
                    for state in &sm.states {
                        if state.name.starts_with("Attack30") {
                            for t in &state.transitions {
                                eprintln!("{chr} {} transition effect={:?}", state.name, t.effect);
                            }
                        }
                    }
                    for t in &sm.wildcard_transitions {
                        if sm
                            .states
                            .iter()
                            .any(|s| s.id == t.to_state && s.name.starts_with("Attack30"))
                        {
                            eprintln!("{chr} wildcard target={} effect={:?}", t.to_state, t.effect);
                        }
                    }
                }
                _ => {}
            }
        }
        let db = TaeDb::load_from_cache(&cache, chr);
        for id in 3000..3100 {
            let name = format!("a000_{id:06}");
            if db.anim(&name).is_some() {
                eprintln!("{chr} {name} Blend={:?}", db.blend_in(&name));
            }
        }
        if chr == "c5060" {
            assert_eq!(db.blend_in("a000_003003"), Some(0.2));
            assert_eq!(db.blend_in("a000_003001"), None);
            assert_eq!(db.blend_in("a000_008700"), Some(1.0 / 30.0));
        }
    }
}
