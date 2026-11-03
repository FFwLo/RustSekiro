use sekiro_sim::duel::Duel;
use sekiro_sim::registry::Registry;

/// Every placed character must load; the sweep builds a `Duel` for each one, so it is slow.
#[test]
#[ignore = "slow: builds a Duel for every placed character (about a minute)"]
fn placed_roster_load_smoke() {
    let cache = sekiro_sim::cache::root();
    if !cache.join("params/NpcParam.json").exists() {
        eprintln!("skipping roster smoke: cache not extracted");
        return;
    }
    let roster = Registry::load(&cache).unwrap();
    let mut loaded = Vec::new();
    let mut unsupported = Vec::new();
    for chr in roster.placed.keys() {
        let result = roster.resolve(chr).and_then(|spec| {
            let anim = roster.anim_chr(chr, spec.npc_param);
            if !cache.join(format!("anim/{anim}/skeleton.bin")).exists() {
                return Err(format!("animation donor {anim} not extracted"));
            }
            let duel = Duel::load_with(&cache, 6.0, spec).map_err(|e| e.to_string())?;
            if duel.ai.is_none()
                || duel.enemy.tae.is_empty()
                || duel.enemy.skeleton.bones.is_empty()
            {
                return Err("missing AI, TAE or skeleton".into());
            }
            Ok(())
        });
        match result {
            Ok(()) => loaded.push(chr.as_str()),
            Err(error) => unsupported.push(format!("{chr}: {error}")),
        }
    }
    eprintln!(
        "roster: {} characters, {} placements, {} loaded, {} unsupported",
        roster.placed.len(),
        roster.placed.values().map(Vec::len).sum::<usize>(),
        loaded.len(),
        unsupported.len()
    );
    for chr in &loaded {
        eprintln!("LOAD {chr}");
    }
    for error in unsupported {
        eprintln!("UNSUPPORTED {error}");
    }
    for chr in ["c1010", "c5060", "c7100"] {
        assert!(loaded.contains(&chr), "{chr} failed to load");
    }
    let genichiro = roster.resolve("c7100:71001000").unwrap();
    assert_eq!(genichiro.think, 71_000_000);
    assert_eq!(roster.anim_chr("c1011", 10_110_000), "c1010");
}

/// The characters the project is built around, plus the registry's name resolution.
#[test]
fn key_characters_load_and_resolve() {
    let cache = sekiro_sim::cache::root();
    if !cache.join("params/NpcParam.json").exists() {
        eprintln!("skipping roster: cache not extracted");
        return;
    }
    let roster = Registry::load(&cache).unwrap();
    for chr in ["c1010", "c5060", "c7100"] {
        let spec = roster.resolve(chr).unwrap_or_else(|e| panic!("{chr}: {e}"));
        let anim = roster.anim_chr(chr, spec.npc_param);
        assert!(cache.join(format!("anim/{anim}/skeleton.bin")).exists());
        let duel = Duel::load_with(&cache, 6.0, spec).unwrap();
        assert!(
            duel.ai.is_some()
                && !duel.enemy.tae.is_empty()
                && !duel.enemy.skeleton.bones.is_empty(),
            "{chr}: missing AI, TAE or skeleton"
        );
    }
    let genichiro = roster.resolve("c7100:71001000").unwrap();
    assert_eq!(genichiro.think, 71_000_000);
    assert_eq!(roster.anim_chr("c1011", 10_110_000), "c1010");
}

#[test]
fn animation_set_selects_grouped_clips_and_tae() {
    let cache = sekiro_sim::cache::root();
    if !cache.join("anim/c5400/skeleton.bin").exists() {
        return;
    }
    let mut npc = sekiro_sim::Character::load_npc(&cache, "c5400").unwrap();
    npc.settle(1.0 / 60.0, 400);
    npc.tae_runtime.add_timed(200031, 0.1);
    for _ in 0..20 {
        npc.tick(&sekiro_sim::InputFrame::default(), 1.0 / 60.0);
    }
    npc.npc.action = 3000;
    npc.npc.refs.insert(sekiro_sim::lua_ai::REF_AI_BATTLE);
    for _ in 0..5 {
        npc.tick(&sekiro_sim::InputFrame::default(), 1.0 / 60.0);
    }
    let clip = npc.behavior.runtime.main_clip().unwrap().clone();
    assert!(clip.animation.starts_with("a100_"), "{}", clip.animation);
    assert!(npc.tae.anim(&clip.animation).is_some());
    assert!(npc.clip_duration(&clip.animation).is_some());
    // The TAE window has ended, but its category overwrite remains until replaced.
    assert!(!npc.tae_frame().sp_effects.contains(&200031));
}
