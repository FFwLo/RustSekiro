use glam::Vec3Swizzles;
use std::path::PathBuf;

use sekiro_sim::arena::{Arena, ArenaSpec};
use sekiro_sim::body::Ground;
use sekiro_sim::duel::{Duel, FightEvents};
use sekiro_sim::{InputFrame, events::EventHost};

/// The cache ([`sekiro_sim::cache::root`]: `SEKIRO_CACHE` when set, else the repo's).
pub fn cache() -> PathBuf {
    sekiro_sim::cache::root()
}

pub fn staged(name: &str, fog: usize) -> Option<(Arena, Duel)> {
    let cache = cache();
    let spec = ArenaSpec::by_name(name).unwrap();
    if !cache
        .join(format!("maps/{}/collision.bin", spec.map))
        .is_file()
    {
        eprintln!(
            "skipping {name}: cache/maps/{}/collision.bin is missing",
            spec.map
        );
        return None;
    }
    let arena = Arena::load(&cache, spec, false).unwrap();
    assert_eq!(arena.fog_walls.len(), fog);
    let enemy = sekiro_sim::collision::EnemyGround(arena.world.clone());
    for (who, spot, ground) in [
        ("Wolf", arena.layout.wolf, &*arena.world as &dyn Ground),
        ("boss", arena.layout.boss, &enemy),
    ] {
        let floor = ground
            .floor(spot.position.x, spot.position.z, spot.position.y + 1.0)
            .unwrap_or_else(|| panic!("{name}: no collision below {who} at {spot:?}"));
        assert!(
            (floor - spot.position.y).abs() < 1.5,
            "{name}: {who} floor {floor}, spot {spot:?}"
        );
        assert!(spot.position.xz().distance(arena.layout.center().xz()) < spec.radius);
    }
    let enemy = spec.enemy_spec();
    let mut duel = Duel::load_with(&cache, 6.0, enemy).unwrap();
    arena.place(&mut duel);
    duel.enemy_active = false;
    duel.god = true;
    let start = [duel.player.body.position, duel.enemy.body.position];
    for _ in 0..360 {
        let r = duel.step(&InputFrame::default(), 1.0 / 60.0);
        assert!(
            r.player.behavior.script_errors.is_empty(),
            "{:?}",
            r.player.behavior.script_errors
        );
        assert!(
            r.enemy.behavior.script_errors.is_empty(),
            "{:?}",
            r.enemy.behavior.script_errors
        );
    }
    assert!(
        duel.events.as_ref().unwrap().boss_bar.is_some(),
        "{name}: no event boss bar"
    );
    assert!(
        arena.world.gate(),
        "{name}: event did not stand the fog gate"
    );
    assert!(
        (duel.player.body.position[1] - start[0][1]).abs() < 1.5,
        "{name}: Wolf fell"
    );
    eprintln!(
        "{name}: {} collision triangles; Wolf {} {:?}; boss {:?}; bar {:?}; {} fog walls",
        arena.world.triangle_count(),
        arena.layout.wolf_source,
        start[0],
        start[1],
        duel.events.as_ref().unwrap().boss_bar,
        arena.fog_walls.len()
    );
    Some((arena, duel))
}

pub struct Host {
    pub entity: i32,
    pub bars: i32,
    pub message: i32,
    pub effect: i32,
}

impl EventHost for Host {
    fn inside_area(&self, _: i32, _: i32) -> Option<bool> {
        Some(true)
    }
    fn has_sp_effect(&self, entity: i32, id: i32) -> Option<bool> {
        Some(entity == self.entity && id == self.effect)
    }
    fn has_event_message(&self, entity: i32, id: i32) -> Option<bool> {
        Some(entity == self.entity && id == self.message)
    }
    fn is_dead(&self, _: i32) -> Option<bool> {
        Some(false)
    }
    fn hp(&self, _: i32) -> Option<i32> {
        Some(100)
    }
    fn health_bars(&self, entity: i32) -> Option<i32> {
        (entity == self.entity).then_some(self.bars)
    }
}

pub fn events(row: &sekiro_sim::arena::BossEvents) -> FightEvents {
    FightEvents::load(&cache(), row).unwrap()
}
