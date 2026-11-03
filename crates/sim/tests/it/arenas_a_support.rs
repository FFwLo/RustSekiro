use sekiro_sim::arena::{Arena, ArenaSpec, boss_events};
use sekiro_sim::body::Ground;
use sekiro_sim::events::{EventCommand, EventHost, EventScripts};

struct StandingBoss(i32, bool);
impl EventHost for StandingBoss {
    fn has_sp_effect(&self, entity: i32, _: i32) -> Option<bool> {
        (entity == self.0 || entity == 10000).then_some(false)
    }
    fn has_event_message(&self, entity: i32, _: i32) -> Option<bool> {
        (entity == self.0 || entity == 10000).then_some(false)
    }
    fn is_dead(&self, entity: i32) -> Option<bool> {
        (entity == self.0 || entity == 10000).then_some(entity == self.0 && self.1)
    }
    fn hp(&self, entity: i32) -> Option<i32> {
        (entity == self.0 || entity == 10000).then_some(100)
    }
    fn max_hp(&self, entity: i32) -> Option<i32> {
        self.hp(entity)
    }
    fn health_bars(&self, entity: i32) -> Option<i32> {
        (entity == self.0).then_some(2)
    }
}

pub(super) fn check(name: &str, entity: i32, fog_flags: Option<(i32, i32)>) {
    let cache = sekiro_sim::cache::root();
    let spec = ArenaSpec::by_name(name).unwrap();
    if !cache
        .join(format!("maps/{}/collision.bin", spec.map))
        .exists()
    {
        eprintln!("skipping {name}: map {} is not extracted", spec.map);
        return;
    }
    let arena = Arena::load(&cache, spec, false).unwrap();
    let enemy = spec.enemy_spec();
    let row = boss_events(&enemy).unwrap();
    assert_eq!(row.boss_entity, entity);
    for spot in [arena.layout.wolf, arena.layout.boss] {
        // The monkeys' health controller is deliberately below the temple and has no floor.
        if name == "senpou-screens" && spot == arena.layout.boss {
            continue;
        }
        let y = arena
            .world
            .floor(spot.position.x, spot.position.z, spot.position.y + 1.0)
            .expect("authored start has a map collision floor");
        assert!(
            (y - spot.position.y).abs() < 0.5,
            "{name}: {spot:?}, floor {y}"
        );
    }
    assert!(arena.world.triangle_count() > 100);
    match fog_flags {
        Some(flags) => {
            assert_eq!(arena.fog_walls.len(), 1);
            let wall = arena.fog_walls[0].wall;
            assert_eq!((wall.start_flag, wall.end_flag), flags);
            assert!(wall.active(|f| row.flags.contains(&f)));
            assert!(!wall.active(|f| row.flags.contains(&f) || f == wall.end_flag));
        }
        None => assert!(arena.fog_walls.is_empty()),
    }
    let mut scripts = EventScripts::load(&cache, row.map, row.events).unwrap();
    scripts.flags.extend(row.flags.iter().copied());
    sekiro_sim::arenas_a::seed_scene(&cache, row, &mut scripts).unwrap();
    let mut shown = false;
    for _ in 0..240 {
        let commands = scripts.step(1.0 / 60.0, &StandingBoss(entity, false));
        if name == "hirata-hall" {
            assert!(
                !commands.iter().any(|c| matches!(
                    c,
                    EventCommand::CharacterEnabled {
                        entity: 1_000_810,
                        enabled: true
                    }
                )),
                "Butterfly's second placement must wait for the first phase to die"
            );
        }
        shown |= commands.iter().any(
            |c| matches!(c, EventCommand::BossBar { entity: e, shown: true, .. } if *e == entity),
        );
    }
    assert!(shown, "{name}: script must show the boss bar");
    if name == "hirata-hall" {
        let mut enabled = false;
        for _ in 0..120 {
            enabled |= scripts
                .step(1.0 / 60.0, &StandingBoss(entity, true))
                .iter()
                .any(|c| {
                    matches!(
                        c,
                        EventCommand::CharacterEnabled {
                            entity: 1_000_810,
                            enabled: true
                        }
                    )
                });
        }
        assert!(
            enabled,
            "event 11005812 must enable Butterfly's second placement after death"
        );
    }
    if name == "senpou-screens" {
        assert_eq!(scripts.cast_scene.enabled.get(&8_907_085), Some(&true));
        // All four InitializeEvent slots must select the intact screen, not just slot zero.
        for (intact, defeated) in [
            (2_001_830, 2_001_840),
            (2_001_832, 2_001_842),
            (2_001_831, 2_001_841),
            (2_001_833, 2_001_843),
        ] {
            assert_eq!(scripts.cast_scene.enabled.get(&intact), Some(&true));
            assert_eq!(scripts.cast_scene.enabled.get(&defeated), Some(&false));
        }
        assert!(
            !scripts.flag(12_005_826),
            "the bell must not ring without an action-button host"
        );
    }
    let donor = sekiro_sim::registry::Registry::load(&cache)
        .unwrap()
        .anim_chr(&enemy.chr, enemy.npc_param);
    if cache.join(format!("anim/{donor}/skeleton.bin")).is_file() {
        let mut duel = sekiro_sim::duel::Duel::load_with(&cache, 6.0, enemy.clone()).unwrap();
        duel.use_simple_ai();
        duel.enemy_active = false;
        arena.place(&mut duel);
        let report = duel.step(&sekiro_sim::InputFrame::default(), 1.0 / 60.0);
        assert!(report.player.behavior.script_errors.is_empty());
        assert!(report.enemy.behavior.script_errors.is_empty());
        assert_eq!(duel.events.as_ref().unwrap().boss_entity, entity);
        if name == "senpou-screens" {
            assert!(
                !duel.enemy.body.gravity_enabled,
                "12005813 disables controller gravity"
            );
            let height = duel.enemy.body.position[1];
            for _ in 0..30 {
                duel.step(&sekiro_sim::InputFrame::default(), 1.0 / 60.0);
            }
            assert!(
                (duel.enemy.body.position[1] - height).abs() < 0.01,
                "the hidden health controller must stay at its placement without a floor"
            );
        }
        assert!(
            (glam::Vec3::from(duel.player.body.position) - arena.layout.wolf.position).length()
                < 0.5
        );
    }
    eprintln!(
        "{name}: entity {entity}, Wolf {:?} from {}, boss {:?}, {} triangles, {} fog walls, story {}",
        arena.layout.wolf.position,
        arena.layout.wolf_source,
        arena.layout.boss.position,
        arena.world.triangle_count(),
        arena.fog_walls.len(),
        spec.story_time(&enemy).name()
    );
}
