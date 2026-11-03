//! Silvergrass story starts, hit collision, fog and the Tomoe-to-Isshin event hand-over.
use super::arena_c_support::{Host, events, staged};
use sekiro_sim::{arena_c, events::EventCommand, story::StoryTime};

#[test]
fn group_c_silvergrass_stages_story_fights_and_isshin_storm() {
    for (name, time) in [
        ("silvergrass-genichiro", StoryTime::PastNight),
        ("silvergrass", StoryTime::Night),
        ("silvergrass-isshin", StoryTime::Night),
    ] {
        let Some((arena, mut duel)) = staged(name, if name == "silvergrass" { 2 } else { 1 })
        else {
            return;
        };
        assert_eq!(arena.spec.story_time(&arena.spec.enemy_spec()), time);
        assert!(arena.layout.wolf_source.starts_with("story warp region"));
        assert_eq!(
            arena_c::regions::contains(
                &super::arena_c_support::cache(),
                arena.spec.map,
                1_122_802,
                arena.layout.wolf.position.to_array()
            ),
            Some(true)
        );
        if name == "silvergrass" {
            // The arena hand-over is independent of Tomoe's combat AI or deathblow tuning.
            duel.rules.fighters[1].vitality.deathblows_left = 0;
            for _ in 0..420 {
                duel.step(&sekiro_sim::InputFrame::default(), 1.0 / 60.0);
            }
            assert_eq!(duel.spec.chr, "c5400");
            assert_eq!(duel.events.as_ref().unwrap().boss_entity, 1_120_860);
            assert_eq!(duel.events.as_ref().unwrap().boss_bar, Some(905400));
            assert!(arena.world.gate());
        }
    }
    let mut fight = events(&arena_c::RESERVOIR_TOMOE);
    let host = Host {
        entity: 1_120_830,
        bars: 0,
        message: 0,
        effect: 0,
    };
    let mut commands = Vec::new();
    for _ in 0..180 {
        commands.extend(fight.scripts.step(1.0 / 60.0, &host));
    }
    assert!(commands.iter().any(|c| matches!(
        c,
        EventCommand::CharacterEnabled {
            entity: 1_120_860,
            enabled: true
        }
    )));
    assert!(commands.iter().any(|c| matches!(
        c,
        EventCommand::Cutscene {
            point: 1_122_800,
            ..
        }
    )));
    let mut fight = events(&arena_c::SWORD_SAINT);
    let host = Host {
        entity: 1_120_860,
        bars: 2,
        message: 0,
        effect: 200031,
    };
    fight.scripts.step(1.0 / 60.0, &host);
    assert!(fight.scripts.flag(11_125_862));
    assert_eq!(
        fight.scripts.cast_scene.enabled.get(&1_127_820),
        Some(&false)
    );
    assert_eq!(fight.scripts.arena_c_state.gparam, Some((11, 2, 30, 5.0)));
}

#[test]
fn group_c_finishing_boundary_switches_independently_of_fog() {
    use glam::Vec3;
    use sekiro_formats::hknp::CollisionMesh;
    use sekiro_sim::collision::CollisionWorld;
    let wall = |z| CollisionMesh {
        vertices: vec![[0.0, 0.0, z], [2.0, 0.0, z], [0.0, 2.0, z]],
        indices: vec![0, 1, 2],
        materials: vec![0],
    };
    let mut world = CollisionWorld::from_mesh_with_gate(&CollisionMesh::default(), &wall(2.0));
    world.append_event_part(1_124_860, &wall(1.0), sekiro_sim::hit_filter::PLAYER, true);
    let cast = || world.raycast(Vec3::new(0.5, 0.5, 0.0), Vec3::Z, 3.0);
    world.set_gate(true);
    assert_eq!(cast(), Some(1.0));
    world.set_event_part(1_124_860, false);
    assert_eq!(cast(), Some(2.0));
    world.set_gate(false);
    assert_eq!(cast(), None);
    world.set_event_part(1_124_860, true);
    assert_eq!(cast(), Some(1.0));
}
