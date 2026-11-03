use super::gyoubu_arena::{Host, stage};
use sekiro_sim::arena_b::ISSHIN;
use sekiro_sim::events::{EventCommand, EventScripts};

#[test]
fn shura_arena_changes_from_emma_to_isshin_and_burns_the_roof() {
    let Some((arena, mut scripts, mut host)) = stage("shura") else {
        return;
    };
    for _ in 0..180 {
        scripts.step(1.0 / 60.0, &host);
    }
    assert_eq!(scripts.cast_scene.enabled.get(&1117551), Some(&false));
    host.bars = 0;
    let mut commands = Vec::new();
    for _ in 0..700 {
        commands.extend(scripts.step(1.0 / 60.0, &host));
    }
    assert!(
        commands
            .iter()
            .any(|c| matches!(c, EventCommand::Cutscene { id: 11010080, .. }))
    );
    assert!(commands.contains(&EventCommand::CharacterEnabled {
        entity: 1110920,
        enabled: true
    }));
    assert_eq!(scripts.cast_scene.enabled.get(&1117550), Some(&false));
    assert_eq!(scripts.cast_scene.enabled.get(&1117551), Some(&true));
    assert_eq!(scripts.cast_scene.lighting, Some(4));
    let cache = sekiro_sim::cache::root();
    let warp = sekiro_sim::events::warp_point(&cache, 11, 1, 1112812).unwrap();
    assert!(
        arena
            .layout
            .wolf
            .position
            .distance(glam::Vec3::from_array(warp.position))
            < 0.1
    );
    let effects = sekiro_sim::arena_b::map_effects(&cache, ISSHIN.map).unwrap();
    let msb = sekiro_formats::msb::parse(
        &std::fs::read(cache.join("raw/map/MapStudio/m11_01_00_00.msb")).unwrap(),
    )
    .unwrap();
    for group in [1117550, 1117551] {
        assert!(
            msb.parts
                .iter()
                .any(|p| p.entity_group_ids.contains(&group)),
            "floor event group {group} has MSB parts"
        );
    }
    for (&entity, &on) in &scripts.cast_scene.effects {
        if on {
            let effect = effects
                .iter()
                .find(|e| e.entity == entity)
                .expect("active event effect has an MSB placement");
            assert!(
                cache
                    .join(format!(
                        "raw/sfx/sfxbnd_m11.ffxbnd.d/f{:09}.fxr",
                        effect.effect
                    ))
                    .is_file(),
                "active placed effect {} is extracted",
                effect.effect
            );
        }
    }
    let mut isshin = EventScripts::load(&cache, ISSHIN.map, ISSHIN.events).unwrap();
    isshin.flags.extend(ISSHIN.flags);
    let mut host = Host {
        boss: 1110920,
        bars: 2,
        message: 0,
        effect: 0,
        in_throw: false,
    };
    for _ in 0..180 {
        isshin.step(1.0 / 60.0, &host);
    }
    host.bars = 1;
    for _ in 0..190 {
        isshin.step(1.0 / 60.0, &host);
    }
    assert!(isshin.flag(11115922));
    host.message = 40;
    assert!(
        isshin
            .step(1.0 / 60.0, &host)
            .contains(&EventCommand::Immortality {
                entity: 1110920,
                on: false
            })
    );
    host.effect = 201000;
    host.message = 50;
    for _ in 0..90 {
        isshin.step(1.0 / 60.0, &host);
    }
    assert!(isshin.flag(9316));
    assert!(!isshin.flag(11115722));
    let fog = sekiro_sim::fogwall::load(&cache, ISSHIN.map, ISSHIN.fog_events).unwrap();
    assert!(fog.iter().all(|wall| wall.active(|f| isshin.flag(f))));
    isshin.set_flag(11110100, true);
    assert!(fog.iter().all(|wall| !wall.active(|f| isshin.flag(f))));
    // Exercise the real duel's cinematic, MSB placement and sequential entity replacement.
    let mut duel = sekiro_sim::duel::Duel::load_with(&cache, 6.0, arena.spec.enemy_spec()).unwrap();
    arena.place(&mut duel);
    duel.god = true;
    duel.ai = None;
    duel.rules.fighters[1].vitality.hp = 0;
    duel.rules.fighters[1].vitality.deathblows_left = 0;
    for _ in 0..900 {
        duel.step(&sekiro_sim::InputFrame::default(), 1.0 / 60.0);
        if duel.spec.chr == "c5430" {
            break;
        }
    }
    assert_eq!(
        duel.spec.chr,
        "c5430",
        "HP {}, marks {}, flags {:?}, log {:?}",
        duel.rules.fighters[1].vitality.hp,
        duel.rules.fighters[1].vitality.deathblows_left,
        duel.events.as_ref().unwrap().scripts.flags,
        duel.events.as_ref().unwrap().scripts.log
    );
    assert!((duel.enemy.body.position[0] - 114.278).abs() < 0.1);
    assert!((duel.enemy.body.position[2] - 235.094).abs() < 0.1);
    assert_eq!(
        duel.events.as_ref().unwrap().standing_fog_walls().count(),
        1
    );
}
