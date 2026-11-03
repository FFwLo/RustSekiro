//! Watering hole placement and message-driven Guardian Ape phase/defeat events.
use super::arena_c_support::{Host, events, staged};
use sekiro_sim::{arena_c, events::EventCommand, story::StoryTime};

#[test]
fn group_c_watering_hole_stages_guardian_ape_and_phase_events() {
    let Some((arena, _)) = staged("watering-hole", 1) else {
        return;
    };
    assert_eq!(
        arena.spec.story_time(&arena.spec.enemy_spec()),
        StoryTime::Afternoon
    );
    let mut fight = events(&arena_c::GUARDIAN_APE);
    let mut host = Host {
        entity: 1_700_800,
        bars: 1,
        message: 20,
        effect: 0,
    };
    let mut commands = Vec::new();
    for step in 0..780 {
        commands.extend(fight.scripts.step(1.0 / 60.0, &host));
        if step < 660 {
            assert!(
                !commands.iter().any(|c| matches!(
                    c,
                    EventCommand::AiParam {
                        entity: 1_700_800,
                        ..
                    }
                )),
                "the 12-second phase wait was skipped"
            );
        }
    }
    assert!(commands.iter().any(|c| matches!(
        c,
        EventCommand::EzStateRequest {
            entity: 1_700_800,
            command: 20000,
            ..
        }
    )));
    host.message = 30;
    host.effect = 201000;
    let commands = fight.scripts.step(1.0 / 60.0, &host);
    assert!(
        commands
            .iter()
            .any(|c| matches!(c, EventCommand::BossDefeat { entity: 1_700_800 }))
    );
    assert!(fight.scripts.flag(9304));
    assert_eq!(fight.standing_fog_walls().count(), 0);
}
