//! Throw checks against the exe, second pass (topic throw2): the sight casts, the SpEffect pair,
//! the pick among chrs and the fall orbit, run in tools/emu-cases throw2.rs on the snapshot.

use super::*;

fn load(name: &str) -> Option<serde_json::Value> {
    let text = std::fs::read_to_string(crate::paths::emu().join(name)).ok()?;
    Some(serde_json::from_str(&text).unwrap())
}

fn v3(c: &serde_json::Value) -> Vec3 {
    Vec3::new(c[0].as_f64().unwrap() as f32, c[1].as_f64().unwrap() as f32, c[2].as_f64().unwrap() as f32)
}

fn f(c: &serde_json::Value) -> f32 {
    c.as_f64().unwrap() as f32
}

/// FUN_140be3da0 on Wolf -> the General (rows 11020000 / 0020 / 0150), 1,200 placements near and
/// far over the arena: player.rs throw_sight_ok, run on the exe's own casts (FUN_14094d8d0 with
/// the start, delta and radius each case recorded), gives the exe's answer on every case, and
/// builds the same casts (start, delta, radius) as the exe. Skipped without
/// extracted/emu/throw2_sight.json.
#[test]
pub(crate) fn throw_sight_matches_the_exe() {
    let Some(v) = load("throw2_sight.json") else { return };
    let app = app();
    let combat = app.world().resource::<Combat>();
    let m = &v["meta"];
    assert_eq!([f(&m["fracs"][0]), f(&m["fracs"][1])], crate::player::THROW_SIGHT_FRACS);
    assert_eq!(f(&m["wolf_height"]), crate::player::WOLF_PHYS_HEIGHT);
    assert_eq!(f(&m["wolf_radius"]), crate::player::CAPSULE_RADIUS);
    let npc = combat.param("NpcParam", 10200000);
    assert_eq!(f(&m["enemy_height"]), f(&npc["hitHeight"]));
    assert_eq!(f(&m["enemy_radius"]), f(&npc["hitRadius"]));
    let cases = v["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 1200);
    let (mut bad, mut blocked) = (Vec::new(), 0);
    for c in cases {
        let rays = c["rays"].as_array().unwrap();
        let calls = std::cell::Cell::new(0usize);
        let geometry_ok = std::cell::Cell::new(true);
        let cast = |start: Vec3, delta: Vec3, radius: f32| {
            let r = &rays[calls.get()];
            calls.set(calls.get() + 1);
            if start.distance(v3(&r["start"])) > 1e-3 || delta.distance(v3(&r["delta"])) > 1e-3 || (radius - f(&r["radius"])).abs() > 1e-5 {
                geometry_ok.set(false);
            }
            r["hit"].as_array().map(|_| v3(&r["hit"]))
        };
        let row = c["row"].as_i64().unwrap();
        let exe = c["sight"].as_i64().unwrap() == 1;
        blocked += !exe as usize;
        let ours = crate::player::throw_sight_ok(combat, row, v3(&c["wolf"]), f(&m["wolf_height"]), f(&m["wolf_radius"]), v3(&c["enemy"]), f(&m["enemy_height"]), f(&m["enemy_radius"]), &cast);
        if ours != exe || !geometry_ok.get() {
            bad.push(format!("row {row} wolf {} enemy {}: exe {exe} ours {ours} casts as the exe's {}", c["wolf"], c["enemy"], geometry_ok.get()));
        }
    }
    assert!(bad.is_empty(), "{} of {} differ:\n{}", bad.len(), cases.len(), bad.join("\n"));
    assert!(blocked > 300 && blocked < 900, "blocked {blocked}");
}

/// The additionalConditionSpecialEffect pair of FUN_140be3a90 (row 11020190 with the pair poked:
/// the thrower's id on Wolf, the defender's on the General, each worn or not; the General wears
/// 200030 in the snapshot): player.rs speffect_pair_ok on all 16, and the row's own pair
/// (forDef 200030 = his a000 anim set) through throw_speffects_ok.
#[test]
pub(crate) fn throw_speffect_pair_matches_the_exe() {
    let Some(v) = load("throw2_speffect.json") else { return };
    let cases = v["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 16);
    for c in cases {
        let (atk, def) = (c["atk_sp"].as_i64().unwrap(), c["def_sp"].as_i64().unwrap());
        let (wolf_has, gen_has) = (c["wolf_has"].as_bool().unwrap(), c["enemy_has"].as_bool().unwrap());
        // The General wears 200030 anyway.
        let ours = crate::player::speffect_pair_ok(atk, def, &|_| wolf_has, &|id| gen_has || id == 200030);
        assert_eq!(ours, c["row_ok"].as_i64().unwrap() == 1, "{c}");
    }
    let app = app();
    let combat = app.world().resource::<Combat>();
    let mut general = Actor::new(crate::actor::Side::Enemy, 100.0, 100.0, 0.0, 0);
    assert!(crate::player::throw_speffects_ok(combat, 11020190, &|_| false, &|id| crate::combat::wears_sp(&general, id)));
    general.anim_group = 1;
    assert!(!crate::player::throw_speffects_ok(combat, 11020190, &|_| false, &|id| crate::combat::wears_sp(&general, id)));
}

/// The input throw's pick (FUN_140be0e50): two c1010 and the General broken around Wolf, 120
/// placements; the exe stops at the first chr in WorldChrMan's order with a passing row, never
/// by distance. player.rs deathblow_pick on the same candidates (in reach = that chr alone
/// passed; spawn order = the exe's chr order) takes the same chr on all.
#[test]
pub(crate) fn deathblow_pick_matches_the_exe() {
    let Some(v) = load("throw2_pick.json") else { return };
    let cases = v["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 120);
    let mut world = bevy::ecs::world::World::new();
    let (mut several, mut not_nearest) = (0, 0);
    for c in cases {
        let chrs = c["chrs"].as_array().unwrap();
        // Spawn in the exe's order so Entity order is its order within one chr number.
        let mut idx: Vec<usize> = (0..chrs.len()).collect();
        idx.sort_by_key(|&k| chrs[k]["order"].as_u64().unwrap());
        let mut ents = vec![Entity::PLACEHOLDER; chrs.len()];
        for k in idx {
            ents[k] = world.spawn_empty().id();
        }
        let wolf = v3(&c["wolf"]);
        let cands: Vec<_> = chrs.iter().enumerate().map(|(k, x)| (ents[k], v3(&x["pos"]), x["alone"].as_i64().unwrap() == 1, true, x["chr"].as_i64().unwrap())).collect();
        let passing: Vec<_> = cands.iter().filter(|x| x.2).collect();
        if passing.len() > 1 {
            several += 1;
            let nearest = passing.iter().min_by(|a, b| a.1.distance(wolf).total_cmp(&b.1.distance(wolf))).unwrap().0;
            let exe = c["picked"].as_i64().unwrap();
            not_nearest += (exe >= 0 && ents[exe as usize] != nearest) as usize;
        }
        // The lock target plays no part: lock the last chr.
        let ours = crate::player::deathblow_pick(cands.clone(), Some(ents[2]), wolf, false);
        let exe = c["picked"].as_i64().unwrap();
        assert_eq!(ours, (exe >= 0).then(|| ents[exe as usize]), "{c}");
    }
    assert!(several > 50 && not_nearest > 20, "several {several}, not the nearest {not_nearest}");
}

/// The fall orbit FUN_140be3ba0: its solver FUN_140bb2850 on 400 starts / velocities / targets
/// (player.rs fall_orbit_closest: the same nearest point and time within 1 cm / 0.01 s), and the
/// whole test with Wolf still above the General at random heights, offsets and fall tops (450:
/// plunge 11020150, kick 11020160, stealth plunge 11020030) through throw_fall_orbit_ok.
#[test]
pub(crate) fn fall_orbit_matches_the_exe() {
    let Some(v) = load("throw2_fall.json") else { return };
    let acc = &v["accel"];
    assert_eq!([f(&acc[0]), f(&acc[1])], crate::player::ORBIT_ACCEL);
    let orbit = v["orbit"].as_array().unwrap();
    assert_eq!(orbit.len(), 400);
    for o in orbit {
        let (q, t) = crate::player::fall_orbit_closest(v3(&o["p0"]), v3(&o["v"]), v3(&o["target"]));
        let (eq, et) = (v3(&o["closest"]), f(&o["time"]));
        assert!(q.distance(eq) < 0.01 && (t - et).abs() < 0.01, "{o}: ours {q} {t}");
    }
    let app = app();
    let combat = app.world().resource::<Combat>();
    let cases = v["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 450);
    let mut passed = 0;
    for c in cases {
        let exe = c["ok"].as_i64().unwrap() == 1;
        passed += exe as usize;
        let ours = crate::player::throw_fall_orbit_ok(combat, c["row"].as_i64().unwrap(), v3(&c["wolf"]), f(&c["fall_top"]), Vec3::ZERO, v3(&c["enemy"]));
        assert_eq!(ours, exe, "{c}");
    }
    assert!(passed > 100 && passed < 400, "passed {passed}");
}
