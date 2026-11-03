use sekiro_sim::arena::{Arena, ArenaSpec};
use sekiro_sim::body::Ground;
use sekiro_sim::duel::Duel;
use sekiro_sim::events::{EventCommand, EventHost, EventScripts};

pub(super) struct Host {
    pub boss: i32,
    pub bars: i32,
    pub message: i32,
    pub effect: i32,
    pub in_throw: bool,
}

impl EventHost for Host {
    fn in_state(&self, entity: i32, state: i32) -> Option<bool> {
        (state == 10 && (entity == self.boss || entity == 10000)).then_some(self.in_throw)
    }
    fn has_sp_effect(&self, entity: i32, id: i32) -> Option<bool> {
        (entity == self.boss).then_some(id == self.effect)
    }
    fn has_event_message(&self, entity: i32, id: i32) -> Option<bool> {
        (entity == self.boss).then_some(id == self.message)
    }
    fn is_dead(&self, entity: i32) -> Option<bool> {
        Some(entity == self.boss && self.bars == 0)
    }
    fn hp(&self, entity: i32) -> Option<i32> {
        Some(if entity == self.boss && self.bars == 0 {
            0
        } else {
            100
        })
    }
    fn health_bars(&self, entity: i32) -> Option<i32> {
        (entity == self.boss).then_some(self.bars)
    }
}

pub(super) fn stage(name: &str) -> Option<(Arena, EventScripts, Host)> {
    let cache = sekiro_sim::cache::root();
    let spec = ArenaSpec::by_name(name).unwrap();
    if !cache
        .join(format!("maps/{}/collision.bin", spec.map))
        .is_file()
    {
        eprintln!("skipping {name}: map {} is not extracted", spec.map);
        return None;
    }
    let arena = Arena::load(&cache, spec, false).unwrap();
    for spot in [arena.layout.wolf, arena.layout.boss] {
        let floor = arena
            .world
            .floor(spot.position.x, spot.position.z, spot.position.y + 1.0)
            .expect("MSB start has collision floor");
        assert!(
            (floor - spot.position.y).abs() < 1.5,
            "{name}: {spot:?}, floor {floor}"
        );
    }
    let row = sekiro_sim::arena::boss_events(&spec.enemy_spec()).unwrap();
    let mut scripts = EventScripts::load(&cache, row.map, row.events).unwrap();
    scripts.flags.extend(row.flags);
    assert!(!arena.fog_walls.is_empty());
    assert!(
        arena
            .fog_walls
            .iter()
            .all(|p| p.wall.active(|f| scripts.flag(f)))
    );
    eprintln!(
        "{name}: Wolf {:?}, boss {:?}; {} triangles, {} active fog walls",
        arena.layout.wolf.position,
        arena.layout.boss.position,
        arena.world.triangle_count(),
        arena.fog_walls.len()
    );
    // End-user simulation startup: the real HKS, skeleton and donor data must load.
    let mut duel = Duel::load_with(&cache, 6.0, spec.enemy_spec()).unwrap();
    arena.place(&mut duel);
    duel.enemy_active = false;
    for _ in 0..60 {
        duel.step(&sekiro_sim::InputFrame::default(), 1.0 / 60.0);
    }
    assert!((duel.player.body.position[1] - arena.layout.wolf.position.y).abs() < 1.5);
    let host = Host {
        boss: row.boss_entity,
        bars: 3,
        message: 0,
        effect: 0,
        in_throw: false,
    };
    Some((arena, scripts, host))
}

#[test]
fn gyoubu_arena_starts_and_releases_its_three_walls_on_defeat() {
    let Some((arena, mut scripts, mut host)) = stage("gyoubu") else {
        return;
    };
    let mut commands = Vec::new();
    for _ in 0..300 {
        commands.extend(scripts.step(1.0 / 60.0, &host));
    }
    assert!(commands.iter().any(|c| matches!(
        c,
        EventCommand::BossBar {
            shown: true,
            entity: 1100800,
            name: 905081,
            ..
        }
    )));
    host.message = 10;
    assert!(
        scripts
            .step(1.0 / 60.0, &host)
            .contains(&EventCommand::Immortality {
                entity: 1100800,
                on: false
            })
    );
    host.message = 50;
    assert!(
        scripts
            .step(1.0 / 60.0, &host)
            .contains(&EventCommand::Immortality {
                entity: 1100800,
                on: true
            })
    );
    host.bars = 0;
    host.message = 50;
    for _ in 0..60 {
        scripts.step(1.0 / 60.0, &host);
    }
    assert!(scripts.flag(9301));
    assert!(
        arena
            .fog_walls
            .iter()
            .all(|p| !p.wall.active(|f| scripts.flag(f)))
    );
}
