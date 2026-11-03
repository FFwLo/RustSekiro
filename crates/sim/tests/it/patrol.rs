use std::sync::Arc;

use sekiro_sim::InputFrame;
use sekiro_sim::collision::CollisionWorld;
use sekiro_sim::duel::{
    Duel,
    population::{Population, SightCone},
};

/// An Outskirts ronin (c1010_0003) walks its two-point MSB route and stands for the route's
/// arrival wait at each point, as NonBattleAct scripts it (#116): the goal parameter `false`
/// reads back as 0 (walk), and a fresh NonBattleAct reads its pooled slot's last result
/// (GetLastResult Success) to queue the arrival wait. It used to run back and forth.
#[test]
fn outskirts_patrol_walks_and_waits_at_its_points() {
    let cache = sekiro_sim::cache::root();
    let map = "m11_00_00_00";
    let dir = cache.join(format!("maps/{map}"));
    if !dir.join("collision.bin").exists() || !cache.join("anim/c1010").is_dir() {
        return;
    }
    let mut wolf = outskirts(&cache, map, &dir);
    let ronin = |wolf: &Duel| {
        let foe = wolf
            .population
            .as_ref()
            .unwrap()
            .foes
            .iter()
            .find(|f| f.part == "c1010_0003")
            .unwrap();
        let patrol = foe.duel.ai.as_ref().unwrap().state.patrol.clone().unwrap();
        (foe.duel.enemy.body.position, patrol)
    };
    let (_, patrol) = ronin(&wolf);
    let wait = patrol.arrival(0).expect("point 0 has an arrival entry").1 as f32;
    assert!(wait >= 5.0, "MSB arrival wait {wait}");
    let dt = 1.0 / 60.0;
    let mut track = Vec::new();
    let mut arrived = None;
    for step in 0..60 * 40 {
        wolf.step(&InputFrame::default(), dt);
        let t = step as f32 * dt;
        let (at, patrol) = ronin(&wolf);
        track.push((t, at));
        if arrived.is_none() && patrol.previous == 0 {
            arrived = Some((t, at));
        }
        if arrived.is_some_and(|(t0, _)| t > t0 + wait) {
            break;
        }
    }
    let (t0, at0) = arrived.expect("the ronin reaches its first point");
    let flat = |a: [f32; 3], b: [f32; 3]| (a[0] - b[0]).hypot(a[2] - b[2]);
    // Walking pace: at most 2 m in any second (running is about 5 m/s).
    let fastest = track
        .windows(61)
        .map(|w| flat(w[0].1, w[60].1))
        .fold(0.0f32, f32::max);
    // The arrival wait: still at the point most of the wait later (turning in place allowed).
    let stood = track
        .iter()
        .filter(|(t, _)| *t >= t0 && *t <= t0 + wait * 0.8)
        .map(|(_, at)| flat(*at, at0))
        .fold(0.0f32, f32::max);
    eprintln!("ronin: arrived {t0:.1} s, fastest {fastest:.2} m/s, drift {stood:.2} m in the wait");
    assert!(fastest < 2.0, "the patrol runs ({fastest:.2} m/s)");
    assert!(
        stood < 0.5,
        "the patrol leaves its point during the wait ({stood:.2} m)"
    );
}

/// An Outskirts soldier with no route and no small act (c1010_0001, MSB init_anim_id_type -1)
/// stands guard as NonBattleAct scripts it (#115): GetSmallActAnimId is -1 there, so it Stays
/// at its post (POINT_INIT_POSE). It used to queue a 20 s WaitWithAnime of anim 0, an
/// AttackNonCancel of the idle it was already playing.
#[test]
fn outskirts_guard_stays_at_its_post() {
    let cache = sekiro_sim::cache::root();
    let map = "m11_00_00_00";
    let dir = cache.join(format!("maps/{map}"));
    if !dir.join("collision.bin").exists() || !cache.join("anim/c1010").is_dir() {
        return;
    }
    let mut wolf = outskirts(&cache, map, &dir);
    fn guard(wolf: &mut Duel) -> &mut Duel {
        wolf.population
            .as_mut()
            .unwrap()
            .foes
            .iter_mut()
            .find(|f| f.part == "c1010_0001")
            .unwrap()
            .duel
            .as_mut()
    }
    let dt = 1.0 / 60.0;
    let mut stacks = std::collections::BTreeSet::new();
    for _ in 0..60 * 2 {
        wolf.step(&InputFrame::default(), dt);
        let ai = guard(&mut wolf).ai.as_ref().unwrap();
        assert!(
            !ai.state.battle,
            "the guard does not see Wolf from the issue's spawn"
        );
        stacks.insert(ai.goal_stack().join(">"));
    }
    eprintln!("guard goals {stacks:?}");
    assert!(stacks.contains("NonBattleAct>Stay"), "{stacks:?}");
    assert!(
        !stacks.iter().any(|s| s.contains("WaitWithAnime")),
        "{stacks:?}"
    );
}

/// Wolf at the issue's spawn (#116), out of the Outskirts soldiers' sight, with the map's
/// population.
pub(crate) fn outskirts(cache: &std::path::Path, map: &str, dir: &std::path::Path) -> Duel {
    let mut wolf = Duel::load(cache, 5.0).unwrap();
    wolf.god = true;
    wolf.player.body.position = [-83.0, -44.6, -37.5];
    wolf.player.ground = Box::new(CollisionWorld::load_map(dir).unwrap());
    let world = Arc::new(CollisionWorld::load_map(dir).unwrap());
    let state = sekiro_sim::world_events::evaluate(
        cache,
        map,
        sekiro_sim::world_events::FirstVisit::for_map(map).unwrap(),
    )
    .unwrap();
    let population =
        Population::load_map(cache, map, world, wolf.player.body.position, Some(&state)).unwrap();
    wolf.set_population(population);
    wolf
}

/// An Outskirts soldier fighting Wolf at close range keeps him as its target (#216). In
/// FIND/BATTLE the exe senses with NpcThinkParam's perceive cone from an eye set
/// eye_BackOffsetDist_perceive (4 m) behind the soldier's feet, to Wolf's feet, so a Wolf at
/// arm's length stays seen. The old sense used the normal cone from head height to Wolf's
/// chest, which put him below its 15 deg downward limit under about 1.5 m, and after
/// SightTargetForgetTime (15 s) the soldier dropped out of battle.
#[test]
fn outskirts_soldier_keeps_wolf_at_close_range() {
    let cache = sekiro_sim::cache::root();
    let map = "m11_00_00_00";
    let dir = cache.join(format!("maps/{map}"));
    if !dir.join("collision.bin").exists() || !cache.join("anim/c1010").is_dir() {
        return;
    }
    let mut wolf = outskirts(&cache, map, &dir);
    fn soldier(wolf: &Duel) -> &Duel {
        wolf.population
            .as_ref()
            .unwrap()
            .foes
            .iter()
            .find(|f| f.part == "c1010_0000")
            .unwrap()
            .duel
            .as_ref()
    }
    // Wolf `gap` m in front of the soldier, on its ground.
    let place = |wolf: &mut Duel, gap: f32| {
        let body = &soldier(wolf).enemy.body;
        let front = sekiro_sim::arena::forward(body.yaw) * gap;
        wolf.player.body.position = [
            body.position[0] + front.x,
            body.position[1],
            body.position[2] + front.z,
        ];
    };
    // Stealth stays as the exe has it: the unalerted (normal) cone's eye sits
    // eye_BackOffsetDist_normal (4 m) behind the soldier and starts eye_BeginDist_normal (4 m)
    // out, so Wolf 1 m behind its feet (3 m from the eye) is unseen, and 4 m in front is seen.
    {
        let foe = soldier(&wolf);
        let fields = &foe.ai.as_ref().unwrap().state.think.fields;
        let normal = SightCone::from_think("normal", |name: &str| {
            fields.get(name).copied().unwrap_or(0.0) as f32
        });
        let body = &foe.enemy.body;
        let feet = glam::Vec3::from_array(body.position);
        let front = sekiro_sim::arena::forward(body.yaw);
        let radius = wolf.fighters[0].radius;
        assert!(
            !normal.contains(feet, body.yaw, feet - front, radius),
            "an unalerted soldier sees Wolf 1 m behind it ({normal:?})"
        );
        assert!(
            normal.contains(feet, body.yaw, feet + front * 4.0, radius),
            "an unalerted soldier misses Wolf 4 m in front of it ({normal:?})"
        );
    }
    let dt = 1.0 / 60.0;
    place(&mut wolf, 4.0);
    let mut engaged = false;
    for _ in 0..60 * 5 {
        place(&mut wolf, 4.0);
        wolf.step(&InputFrame::default(), dt);
        if soldier(&wolf).ai.as_ref().unwrap().state.battle {
            engaged = true;
            break;
        }
    }
    assert!(engaged, "the soldier sees Wolf 4 m in front of it");
    // Close range for longer than SightTargetForgetTime (15 s).
    for step in 0..60 * 20 {
        place(&mut wolf, 0.8);
        wolf.step(&InputFrame::default(), dt);
        let ai = soldier(&wolf).ai.as_ref().unwrap();
        assert!(
            ai.state.battle,
            "the soldier drops out of battle {:.1} s into the close fight ({})",
            step as f32 * dt,
            ai.goal_stack().join(">")
        );
    }
}

/// An Outskirts guard with no route (c1010_0001) that chased Wolf off its post and then lost
/// him walks back to its post (#217). NonBattleAct walks home when GetMovePointEffectRange
/// exceeds its walk-home distance; the exe returns the distance from the foe to its move
/// point, which for a foe with no route is its initial position (FUN_1405afea0). It used to
/// return 0, so the guard stood wherever it lost Wolf.
#[test]
fn outskirts_guard_walks_home_after_losing_wolf() {
    let cache = sekiro_sim::cache::root();
    let map = "m11_00_00_00";
    let dir = cache.join(format!("maps/{map}"));
    if !dir.join("collision.bin").exists() || !cache.join("anim/c1010").is_dir() {
        return;
    }
    let mut wolf = outskirts(&cache, map, &dir);
    fn guard(wolf: &Duel) -> &Duel {
        wolf.population
            .as_ref()
            .unwrap()
            .foes
            .iter()
            .find(|f| f.part == "c1010_0001")
            .unwrap()
            .duel
            .as_ref()
    }
    let flat = |a: [f32; 3], b: [f32; 3]| (a[0] - b[0]).hypot(a[2] - b[2]);
    let post = guard(&wolf).enemy.body.position;
    let dt = 1.0 / 60.0;
    // Wolf 4 m in front of the guard until it fights him, then keeps backing off so the guard
    // follows him away from its post.
    let mut engaged = false;
    for _ in 0..60 * 12 {
        let body = &guard(&wolf).enemy.body;
        let front = sekiro_sim::arena::forward(body.yaw) * 4.0;
        wolf.player.body.position = [
            body.position[0] + front.x,
            body.position[1],
            body.position[2] + front.z,
        ];
        wolf.step(&InputFrame::default(), dt);
        engaged |= guard(&wolf).ai.as_ref().unwrap().state.battle;
        if engaged && flat(guard(&wolf).enemy.body.position, post) > 8.0 {
            break;
        }
    }
    assert!(engaged, "the guard fights Wolf");
    let lost_at = guard(&wolf).enemy.body.position;
    let away = flat(lost_at, post);
    assert!(
        away > 6.0,
        "the guard follows Wolf {away:.1} m off its post"
    );
    // Wolf leaves, 60 m off (out of sight in the air, within the 100 m activation range); the
    // guard forgets him and goes back to NonBattleAct.
    let far = [post[0] + 60.0, post[1], post[2]];
    let mut stacks = std::collections::BTreeSet::new();
    let mut closest = away;
    for _ in 0..60 * 60 {
        wolf.player.body.position = far;
        wolf.step(&InputFrame::default(), dt);
        let foe = guard(&wolf);
        let ai = foe.ai.as_ref().unwrap();
        if !ai.state.battle {
            stacks.insert(ai.goal_stack().join(">"));
        }
        closest = closest.min(flat(foe.enemy.body.position, post));
        if closest < 1.0 {
            break;
        }
    }
    eprintln!(
        "guard lost Wolf {away:.1} m from its post, came within {closest:.1} m; goals {stacks:?}"
    );
    assert!(
        closest < 1.0,
        "the guard stays {closest:.1} m from its post after losing Wolf ({stacks:?})"
    );
    assert!(
        stacks.iter().any(|s| s.contains("BackToHome")),
        "{stacks:?}"
    );
    // Home again, it stands guard there.
    let mut settled = std::collections::BTreeSet::new();
    for _ in 0..60 * 3 {
        wolf.player.body.position = far;
        wolf.step(&InputFrame::default(), dt);
        let foe = guard(&wolf);
        settled.insert(foe.ai.as_ref().unwrap().goal_stack().join(">"));
        let off = flat(foe.enemy.body.position, post);
        assert!(
            off < 1.5,
            "the guard wanders {off:.1} m off its post ({settled:?})"
        );
    }
    assert!(settled.contains("NonBattleAct>Stay"), "{settled:?}");
}

/// A guard chasing Wolf toward the Outskirts start ravine stops at the edge (#227). Wolf hangs
/// over the drop, as he was in the #217 run. The NPC script asks for no fall-prevention assist
/// while walking, so the `f` bands at the edge did not hold the guard, and it walked off the map
/// near (-81.8, -51.5, -57.8) and fell for good. A foe's walk now keeps to the map's navmesh,
/// which ends about 0.5 m before the edge.
#[test]
fn outskirts_guard_chasing_wolf_stops_at_the_ravine_edge() {
    let cache = sekiro_sim::cache::root();
    let map = "m11_00_00_00";
    let dir = cache.join(format!("maps/{map}"));
    let navmesh = cache.join(format!("raw/map/{map}/{map}.nvmhktbnd.d"));
    if !dir.join("collision.bin").exists()
        || !cache.join("anim/c1010").is_dir()
        || !navmesh.is_dir()
    {
        return;
    }
    let mut wolf = outskirts(&cache, map, &dir);
    fn guard(wolf: &Duel) -> &Duel {
        wolf.population
            .as_ref()
            .unwrap()
            .foes
            .iter()
            .find(|f| f.part == "c1010_0001")
            .unwrap()
            .duel
            .as_ref()
    }
    let dt = 1.0 / 60.0;
    // Wolf 4 m in front of the guard until it fights him.
    for _ in 0..60 * 5 {
        let body = &guard(&wolf).enemy.body;
        let front = sekiro_sim::arena::forward(body.yaw) * 4.0;
        wolf.player.body.position = [
            body.position[0] + front.x,
            body.position[1],
            body.position[2] + front.z,
        ];
        wolf.step(&InputFrame::default(), dt);
        if guard(&wolf).ai.as_ref().unwrap().state.battle {
            break;
        }
    }
    assert!(
        guard(&wolf).ai.as_ref().unwrap().state.battle,
        "the guard fights Wolf"
    );
    // Wolf over the ravine, past the edge the guard walks to (the old code dropped it there
    // after about 10.6 s).
    let mut nearest = f32::INFINITY;
    let mut lowest = f32::INFINITY;
    for _ in 0..60 * 13 {
        wolf.player.body.position = [-83.0, -40.0, -75.0];
        wolf.step(&InputFrame::default(), dt);
        let at = guard(&wolf).enemy.body.position;
        nearest = nearest.min(at[2]);
        lowest = lowest.min(at[1]);
    }
    eprintln!("guard came to z {nearest:.2}, lowest y {lowest:.2}");
    assert!(
        nearest < -55.0,
        "the guard chases Wolf to the edge (z {nearest:.2})"
    );
    assert!(
        lowest > -49.0,
        "the guard stays on the ground above the ravine (lowest y {lowest:.2})"
    );
}

/// An Outskirts soldier carried off the start ravine's edge dies of the fall (#234). In the
/// #227 run c1010_0000's attack combo (a000_003012 here) took it past the edge's `f` band
/// (part h000300, which it had already run onto) at about (-81.8, -47.3, -57.7), and it fell
/// for good. The exe's fall-dead routine kills an NPC that meets a filter-15 event wall, here
/// `h900000` under the ravine, fades its model out over 0.5 s and holds its body.
#[test]
fn outskirts_soldier_falling_into_the_ravine_dies() {
    let cache = sekiro_sim::cache::root();
    let map = "m11_00_00_00";
    let dir = cache.join(format!("maps/{map}"));
    if !dir.join("collision.bin").exists() || !cache.join("anim/c1010").is_dir() {
        return;
    }
    let mut wolf = outskirts(&cache, map, &dir);
    fn soldier(wolf: &mut Duel) -> &mut Duel {
        wolf.population
            .as_mut()
            .unwrap()
            .foes
            .iter_mut()
            .find(|f| f.part == "c1010_0000")
            .unwrap()
            .duel
            .as_mut()
    }
    // Just past the edge where the attack left the ground, as in the live run.
    let body = &mut soldier(&mut wolf).enemy.body;
    body.position = [-81.84, -47.29, -57.9];
    body.grounded = false;
    let dt = 1.0 / 60.0;
    let mut track = Vec::new();
    for _ in 0..60 * 4 {
        wolf.step(&InputFrame::default(), dt);
        let foe = soldier(&mut wolf);
        track.push(foe.enemy.body.position);
    }
    let foe = soldier(&mut wolf);
    let lowest = track.iter().map(|p| p[1]).fold(f32::INFINITY, f32::min);
    eprintln!(
        "soldier: {:?}, hp {}, lowest y {lowest:.2}, opacity {}",
        foe.enemy_fall.death,
        foe.rules.fighters[1].vitality.hp,
        foe.enemy_fall_opacity()
    );
    assert!(
        foe.rules.fighters[1].dead(),
        "the soldier survives the fall (lowest y {lowest:.2})"
    );
    assert_eq!(
        foe.enemy_fall.death,
        Some(sekiro_sim::npc_fall::FallDeath::EventWall("h900000".into()))
    );
    assert_eq!(foe.enemy_fall_opacity(), 0.0, "the model fades out");
    // Held where the fade ended: the last second does not move.
    let last = track[track.len() - 60..].to_vec();
    assert!(
        last.iter().all(|p| *p == last[0]),
        "the body is not held ({:?} to {:?})",
        last[0],
        last[59]
    );
}
