//! Burrow placement, both fog walls and Headless Ape finishing flags.
use super::arena_c_support::{Host, events, staged};
use sekiro_sim::{arena_c, events::EventCommand, story::StoryTime};

#[test]
fn group_c_apes_burrow_stages_headless_ape_and_finishing_events() {
    let Some((arena, _)) = staged("apes-burrow", 2) else {
        return;
    };
    assert_eq!(
        arena.spec.story_time(&arena.spec.enemy_spec()),
        StoryTime::Afternoon
    );
    let mut fight = events(&arena_c::HEADLESS_APE);
    let phase = Host {
        entity: 1_700_850,
        bars: 1,
        message: 0,
        effect: 0,
    };
    let mut commands = Vec::new();
    for _ in 0..330 {
        commands.extend(fight.scripts.step(1.0 / 60.0, &phase));
    }
    assert!(commands.iter().any(|c| matches!(
        c,
        EventCommand::CharacterEnabled {
            entity: 1_700_852,
            enabled: true
        }
    )));
    assert_eq!(
        fight.scripts.cast_scene.animations.get(&1_700_852),
        Some(&(20020, false))
    );
    let mut host = Host {
        entity: 1_700_850,
        bars: 0,
        message: 30,
        effect: 201000,
    };
    let commands = fight.scripts.step(1.0 / 60.0, &host);
    assert!(
        commands
            .iter()
            .any(|c| matches!(c, EventCommand::BossDefeat { entity: 1_700_850 }))
    );
    assert!(fight.scripts.flag(9307));
    assert_eq!(fight.standing_fog_walls().count(), 0);
    host.message = 40;
    fight.scripts.step(1.0 / 60.0, &host);
    assert!(fight.scripts.flag(9314));
}
