//! Installed-data staging checks shared by the three group-d arenas.

use sekiro_sim::InputFrame;
use sekiro_sim::arena::{Arena, ArenaSpec};
use sekiro_sim::body::Ground;
use sekiro_sim::duel::Duel;
use sekiro_sim::events::EventLog;

pub fn stage(name: &str, entity: i32, walls: usize) -> Option<(Arena, Duel)> {
    let cache = sekiro_sim::cache::root();
    let spec = ArenaSpec::by_name(name).unwrap();
    if !cache
        .join(format!("maps/{}/collision.bin", spec.map))
        .is_file()
        || !cache
            .join(format!("maps/{}/layout.json", spec.map))
            .is_file()
    {
        eprintln!("skipping {name}: {} is not extracted", spec.map);
        return None;
    }
    let arena = Arena::load(&cache, spec, false).unwrap();
    let mut duel = Duel::load_with(&cache, 6.0, spec.enemy_spec()).unwrap();
    arena.place(&mut duel);
    let events = duel.events.as_ref().expect("reviewed fight events load");
    assert_eq!(events.boss_entity, entity);
    assert_eq!(arena.fog_walls.len(), walls);
    assert!(arena.world.triangle_count() > 0);
    for spot in [arena.layout.wolf, arena.layout.boss] {
        let floor = arena
            .world
            .floor(spot.position.x, spot.position.z, spot.position.y + 1.0);
        eprintln!("{name}: MSB {:?}, collision floor {floor:?}", spot.position);
        assert!(floor.is_some(), "{name}: no hit collision under {spot:?}");
        assert!(
            (floor.unwrap() - spot.position.y).abs() < 0.6,
            "{name}: floor is not at the arena's story elevation"
        );
    }
    // This is a staging smoke, not a claim about the boss's attacks.
    duel.enemy_active = false;
    let start = duel.player.body.position;
    for _ in 0..180 {
        duel.step(&InputFrame::default(), 1.0 / 60.0);
    }
    let events = duel.events.as_ref().unwrap();
    eprintln!(
        "{name}: {} triangles; Wolf {:?}, boss {:?}; bar {:?}; skipped {:?}",
        arena.world.triangle_count(),
        duel.player.body.position,
        duel.enemy.body.position,
        events.boss_bar,
        events
            .scripts
            .log
            .iter()
            .filter(|e| matches!(e, EventLog::Skipped { .. }))
            .collect::<Vec<_>>()
    );
    assert!(
        duel.player.body.position[1] > start[1] - 1.0,
        "Wolf fell out of the arena"
    );
    assert!(
        events.boss_bar.is_some(),
        "the real event must show the boss bar"
    );
    assert_eq!(events.standing_fog_walls().count(), walls);
    Some((arena, duel))
}
