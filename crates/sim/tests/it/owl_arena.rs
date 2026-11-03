//! The Great Shinobi Owl on the Ashina Castle roof (`--arena owl`): his MSB placement in
//! `m11_01_00_00`, Wolf at the roof's player start, both on the map's hit collision within the
//! arena radius. Skips when the map (`sekiro-extract map m11_01_00_00`) or the Owl
//! (`sekiro-extract anims c5060`) is not extracted.

use std::collections::BTreeMap;
use std::time::Instant;

use sekiro_sim::InputFrame;
use sekiro_sim::arena::{Arena, ArenaSpec};
use sekiro_sim::duel::Duel;
use sekiro_sim::lua_ai::AiEvent;

const DT: f32 = 1.0 / 60.0;

fn load() -> Option<(Arena, Duel)> {
    let cache = sekiro_sim::cache::root();
    let spec = ArenaSpec::by_name("owl").unwrap();
    if !cache
        .join("maps")
        .join(spec.map)
        .join("collision.bin")
        .is_file()
        || !cache.join("anim/c5060/tae.json").exists()
        || !cache.join("raw/script/m11_01_00_00.luabnd.d").is_dir()
    {
        eprintln!("skipping: the castle roof or the Owl is not extracted");
        return None;
    }
    let t = Instant::now();
    let arena = Arena::load(&cache, spec, false).unwrap();
    eprintln!(
        "arena collision: {} triangles in {:.2} s; Wolf from {}",
        arena.world.triangle_count(),
        t.elapsed().as_secs_f32(),
        arena.layout.wolf_source
    );
    let mut duel = Duel::load_with(&cache, 6.0, spec.enemy_spec()).unwrap();
    arena.place(&mut duel);
    Some((arena, duel))
}

#[test]
fn owl_fights_on_the_castle_roof() {
    let Some((arena, mut duel)) = load() else {
        return;
    };
    assert!(duel.ai.is_some(), "the Owl's AI did not load");
    let start = [duel.player.body.position, duel.enemy.body.position];
    eprintln!("Wolf at {:?}, Owl at {:?}", start[0], start[1]);
    // The MSB puts both on the roof: the floor is within a few centimetres of the part heights.
    for (spot, placed) in [arena.layout.wolf, arena.layout.boss].iter().zip(start) {
        assert!(
            (spot.position.y - placed[1]).abs() < 0.3,
            "{spot:?} placed at {placed:?}"
        );
    }
    let mut lowest = [f32::MAX; 2];
    let mut actions: BTreeMap<i32, usize> = BTreeMap::new();
    let mut hits_on_wolf = 0;
    for _ in 0..1800 {
        // Keep Wolf alive and unbroken so the Owl fights the whole time.
        let wolf = &mut duel.rules.fighters[0];
        wolf.vitality.hp = wolf.vitality.max_hp;
        wolf.posture.remaining = wolf.posture.max;
        let r = duel.step(&InputFrame::default(), DT);
        assert!(
            r.enemy.behavior.script_errors.is_empty(),
            "{:?}",
            r.enemy.behavior.script_errors
        );
        hits_on_wolf += r.hits.iter().filter(|(h, _)| h.attacker == 1).count();
        for (i, p) in [r.player.position, r.enemy.position]
            .into_iter()
            .enumerate()
        {
            lowest[i] = lowest[i].min(p[1]);
        }
        for e in duel.ai.as_mut().unwrap().drain_log() {
            match e {
                AiEvent::Action(a) => *actions.entry(a).or_default() += 1,
                AiEvent::Error(e) => panic!("AI error: {e}"),
                _ => {}
            }
        }
    }
    let attacks: usize = actions
        .iter()
        .filter(|&(&a, _)| (3000..3100).contains(&a))
        .map(|(_, n)| n)
        .sum();
    eprintln!(
        "30 s: {attacks} Owl attacks, {hits_on_wolf} hits on Wolf, actions {actions:?}; \
         lowest Wolf {:.2} (start {:.2}), Owl {:.2} (start {:.2}); Owl ends at {:?}",
        lowest[0], start[0][1], lowest[1], start[1][1], duel.enemy.body.position
    );
    for i in 0..2 {
        assert!(
            lowest[i] > start[i][1] - 2.0,
            "{} fell to {} from {}",
            ["Wolf", "the Owl"][i],
            lowest[i],
            start[i][1]
        );
    }
    assert!(attacks >= 5, "{actions:?}");
}

/// Off the roof's edge Wolf falls through the event walls under the castle (`h900011`, hit
/// filter 13, where he used to land and die, first), and the first filter-15 wall's fall return
/// (`h900010`) puts him back where he last stood, alive (`docs/HIT-FILTERS.md`). On `l` (#189)
/// the walls are tilted planes, not `h`'s flat sheets at y = -8.6 and -23.6: under this point
/// he passes `h900011` at y = 2.5, `h900001` (13) at -1.8 and `h900010` at -2.2.
#[test]
fn a_fall_off_the_roof_returns_wolf_to_his_start() {
    let Some((arena, mut duel)) = load() else {
        return;
    };
    duel.enemy_active = false;
    duel.step(&InputFrame::default(), DT);
    let start = duel.player.body.position;
    let away = arena.layout.center() + glam::Vec3::X * (arena.spec.radius + 20.0);
    duel.player.body.position = [away.x, start[1], away.z];
    let mut lowest = start[1];
    let mut back = false;
    for _ in 0..(60.0 * 8.0) as usize {
        duel.step(&InputFrame::default(), DT);
        assert!(!duel.rules.fighters[0].dead(), "the fall killed Wolf");
        lowest = lowest.min(duel.player.body.position[1]);
        if duel.player.body.position == start {
            back = true;
            break;
        }
    }
    let parts: Vec<_> = duel
        .helper_events
        .iter()
        .map(|e| (e.part.as_str(), e.hit_filter))
        .collect();
    assert!(
        back && parts.first() == Some(&("h900011", 13)),
        "back {back}, contacts {parts:?}, lowest {lowest}: {:?}",
        duel.player.body.position
    );
    let fall_return = duel
        .helper_events
        .iter()
        .find(|e| e.hit_filter == 15)
        .unwrap_or_else(|| panic!("no filter-15 contact: {parts:?}"));
    assert_eq!(fall_return.part, "h900010", "{parts:?}");
    assert!(lowest < fall_return.point[1], "{lowest}");
    assert_eq!(fall_return.returned_to, Some(start));
}
