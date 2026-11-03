use super::gyoubu_arena::{Host, stage};
use sekiro_sim::events::{EventCommand, EventHost};

/// The stub host with Wolf and the Demon standing inside every map region.
struct Arrived(Host);

impl EventHost for Arrived {
    fn in_state(&self, entity: i32, state: i32) -> Option<bool> {
        self.0.in_state(entity, state)
    }
    fn has_sp_effect(&self, entity: i32, id: i32) -> Option<bool> {
        self.0.has_sp_effect(entity, id)
    }
    fn has_event_message(&self, entity: i32, id: i32) -> Option<bool> {
        self.0.has_event_message(entity, id)
    }
    fn is_dead(&self, entity: i32) -> Option<bool> {
        self.0.is_dead(entity)
    }
    fn hp(&self, entity: i32) -> Option<i32> {
        self.0.hp(entity)
    }
    fn health_bars(&self, entity: i32) -> Option<i32> {
        self.0.health_bars(entity)
    }
    fn inside_area(&self, _entity: i32, _area: i32) -> Option<bool> {
        Some(true)
    }
}

#[test]
fn demon_arena_starts_at_night_waits_for_wolf_and_runs_mark_and_defeat_events() {
    let Some((arena, mut scripts, host)) = stage("demon") else {
        return;
    };
    // The staged row keeps the fight flag for a flat duel; the arena removes it (`Arena::place`).
    assert!(scripts.flag(11_105_901));
    scripts.flags.remove(&11_105_901);
    let cache = sekiro_sim::cache::root();
    let warp = sekiro_sim::events::warp_point(&cache, 11, 0, 1102957).unwrap();
    assert!(
        arena
            .layout
            .wolf
            .position
            .distance(glam::Vec3::from_array(warp.position))
            < 0.001
    );
    assert_eq!(
        arena.spec.story_time(&arena.spec.enemy_spec()),
        sekiro_sim::story::StoryTime::Night
    );
    let walls = |s: &sekiro_sim::events::EventScripts| {
        arena
            .fog_walls
            .iter()
            .filter(|p| p.wall.active(|f| s.flag(f)))
            .count()
    };
    // Retry state: immortal and waiting, no bar and no walls until Wolf is in region 1102910.
    let mut commands = Vec::new();
    for _ in 0..30 {
        commands.extend(scripts.step(1.0 / 60.0, &host));
    }
    assert!(commands.contains(&EventCommand::Immortality {
        entity: 1100900,
        on: true
    }));
    assert!(
        !commands
            .iter()
            .any(|c| matches!(c, EventCommand::BossBar { .. }))
    );
    assert!(!scripts.flag(11105901));
    assert_eq!(walls(&scripts), 0);
    let mut host = Arrived(host);
    let mut commands = Vec::new();
    for _ in 0..30 {
        commands.extend(scripts.step(1.0 / 60.0, &host));
    }
    assert!(commands.iter().any(|c| matches!(
        c,
        EventCommand::BossBar {
            shown: true,
            entity: 1100900,
            name: 907021,
            ..
        }
    )));
    assert!(scripts.flag(11105901));
    assert_eq!(walls(&scripts), 3);
    assert!(!commands.contains(&EventCommand::Immortality {
        entity: 1100900,
        on: false
    }));
    host.0.in_throw = true;
    assert!(
        scripts
            .step(1.0 / 60.0, &host)
            .contains(&EventCommand::Immortality {
                entity: 1100900,
                on: false
            })
    );
    host.0.in_throw = false;
    host.0.bars = 2;
    scripts.step(1.0 / 60.0, &host);
    assert!(scripts.flag(11105902));
    host.0.bars = 1;
    let commands = scripts.step(1.0 / 60.0, &host);
    assert!(commands.contains(&EventCommand::SetSpEffect {
        entity: 1100900,
        id: 3702005
    }));
    host.0.message = 30;
    scripts.step(1.0 / 60.0, &host);
    assert!(scripts.flag(9313));
    assert_eq!(walls(&scripts), 0);
}
