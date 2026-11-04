//! Each prosthetic tool used at the General against the user's own sekiro.exe: tools/emu-cases
//! `prosth` equips the tool the game's way (its inventory handle into PlayerGameData's ChrAsm,
//! the switch ACTION_ARM_CHANGE_WEAPON_L) and runs whole world frames from the press
//! (kb/frame-coverage.md "Prosthetic tools in the exe"). Skips when extracted/emu/prosth_*.json
//! is missing.

use super::*;
use serde_json::Value;

fn load(name: &str) -> Option<Value> {
    let text = std::fs::read_to_string(crate::paths::emu().join(format!("prosth_{name}.json"))).ok()?;
    serde_json::from_str(&text).ok()
}

/// The anim id of a clip name "a071_400100" (71400100).
fn anim_id(name: &str) -> Option<i64> {
    let (g, n) = name.strip_prefix('a')?.split_once('_')?;
    Some(g.parse::<i64>().ok()? * 1_000_000 + n.parse::<i64>().ok()?)
}

/// What one run shows, by frame from the press (frame 0: the first frame the button is down).
#[derive(Debug, Default, PartialEq)]
struct Run {
    /// Each tool clip (a07x_4xxxxx) and the first frame it steps.
    clips: Vec<(i64, usize)>,
    /// (frame, BehaviorParam id) of each TAE bullet behaviour that spawned.
    spawns: Vec<(usize, i64)>,
    /// (frame, BulletParam id) of each bullet the first time it is live.
    bullets: Vec<(usize, i64)>,
    /// (frame, emblems paid).
    paid: Vec<(usize, u32)>,
    /// Frames Wolf's hits took the General's HP or posture.
    hits: Vec<usize>,
}

fn tool_clip(id: i64) -> bool {
    (70_000_000..80_000_000).contains(&id)
}

fn exe_run(v: &Value) -> Run {
    let mut run = Run::default();
    let frames = v["frames"].as_array().unwrap();
    let mut emblems = 20;
    let mut seen = std::collections::HashMap::new();
    for (f, fr) in frames.iter().enumerate() {
        for c in fr["wolf_tae"].as_array().unwrap() {
            let id = c[0].as_i64().unwrap();
            if tool_clip(id) && !run.clips.iter().any(|x| x.0 == id) {
                run.clips.push((id, f));
            }
        }
        for e in fr["events"].as_array().unwrap() {
            if e["fn"] == "spawn" && e["phase"] == "wolf" {
                run.spawns.push((f, e["ints"][1].as_i64().unwrap()));
            }
        }
        // Each of Wolf's bullets the first frame it is live in a slot (its first update): a slot
        // not live the frame before, or holding another bullet. The General's (BulletParam ids
        // from 1,000,000: his behaviour 1500 -> 10001500 at frame 72 of every run) are left out.
        let mut now = std::collections::HashMap::new();
        for s in fr["slots"].as_array().unwrap() {
            let (slot, id) = (s["slot"].as_i64().unwrap(), s["bullet"].as_i64().unwrap());
            if seen.get(&slot) != Some(&id) && id < 1_000_000 {
                run.bullets.push((f, id));
            }
            now.insert(slot, id);
        }
        seen = now;
        for h in fr["hits"].as_array().unwrap() {
            if h["attacker"] == "wolf" && (h["hp_dmg"].as_i64().unwrap_or(0) > 0 || h["posture_dmg"].as_i64().unwrap_or(0) > 0) && !run.hits.contains(&f) {
                run.hits.push(f);
            }
        }
        let e = fr["wolf"]["emblems"].as_i64().unwrap() as u32;
        if e < emblems {
            run.paid.push((f, emblems - e));
        }
        emblems = e;
    }
    run.bullets.sort();
    run
}

/// Ours: Wolf at the origin facing the General `dist` m ahead (AI off), the tool alone in the
/// prosthetic list, the button held `hold` ticks from the first; `frames` ticks.
fn sim_run(tool: i64, hold: usize, dist: f32, frames: usize) -> Run {
    let mut app = app_geom("c1020", None);
    app.world_mut().resource_mut::<crate::config::GameConfig>().player.prosthetics = vec![tool];
    {
        let world = app.world_mut();
        let e = boss_in_play(world);
        world.get_mut::<Enemy>(e).unwrap().ai_off = true;
        world.get_mut::<Transform>(e).unwrap().translation = Vec3::new(0.0, CAPSULE_HALF_HEIGHT, dist);
        world.query::<&mut Player>().single_mut(world).unwrap().emblems = 20;
    }
    for _ in 0..3 {
        app.update();
    }
    let mut run = Run::default();
    let mut emblems = 20;
    let mut seen = std::collections::HashSet::new();
    let foe = boss_in_play(app.world_mut());
    let (mut hp, mut posture) = {
        let a = app.world().get::<Actor>(foe).unwrap();
        (a.hp, a.posture)
    };
    for f in 0..frames {
        {
            let mut pad = app.world_mut().resource_mut::<PadInput>();
            pad.prosthetic_held = f < hold;
            if f == 0 {
                pad.press(Action::Prosthetic);
            }
        }
        app.update();
        let world = app.world_mut();
        let (anim, t, e) = {
            let mut q = world.query::<(&Actor, &Player)>();
            let (a, p) = q.single(world).unwrap();
            (a.anim.clone(), a.t, p.emblems)
        };
        if let Some(id) = anim_id(&anim).filter(|&id| tool_clip(id)) {
            if !run.clips.iter().any(|x| x.0 == id) && t > 0.0 {
                run.clips.push((id, f));
            }
        }
        // Each bullet the first tick it is still there after its first (launch-only) update: the
        // exe's first frame with it live in a slot. One that ends in that update (the whistle's
        // 790001, life 0) is never live in a slot, in either.
        let mut q = world.query::<(Entity, &crate::prosthetic::Projectile)>();
        for (ent, p) in q.iter(world) {
            if p.launched() && seen.insert(ent) {
                run.bullets.push((f, p.bullet_id()));
            }
        }
        if e < emblems {
            run.paid.push((f, emblems - e));
        }
        emblems = e;
        let a = world.get::<Actor>(foe).unwrap();
        if a.hp < hp - 1e-3 || a.posture > posture + 1e-3 {
            run.hits.push(f);
        }
        (hp, posture) = (a.hp, a.posture);
    }
    run.bullets.sort();
    run
}

const TOOLS: [&str; 13] = ["firecracker", "shuriken_tap", "shuriken_hold", "flame_vent", "okinaga_vent", "axe", "mist_raven", "sabimaru", "umbrella", "divine_abduction", "spear", "spiral_spear", "whistle"];

fn both(name: &str) -> Option<(Run, Run)> {
    let v = load(name)?;
    let exe = exe_run(&v);
    let ours = sim_run(v["tool"].as_i64().unwrap(), v["hold"].as_u64().unwrap() as usize, v["dist"].as_f64().unwrap() as f32, v["frames"].as_array().unwrap().len());
    Some((exe, ours))
}

/// Every tool's clips (the deploy and the use, by the frame they first step) and its emblem
/// payment, frame for frame, and each tool bullet's first live frame: the Flame Vent's 295 ending
/// at life 0.3 into 296 (life counted down, prosthetic.rs), the whistle's 790001 -> 790003 ->
/// 790004 chain, the firecracker's carriers and their first sparks (enemy_bullet::interval_child).
/// Not compared: the firecracker's bursts 710003 / 710013 and the sparks after the first (their
/// life is drawn from lifeRandomRange; the exe's random stream is not ours), and hits wherever
/// the General's walk-in (he steps in from his 1040 in the exe, stands here) changes the geometry
/// (axe, Sabimaru; the spears' contacts fall on the same frames in both).
#[test]
pub(crate) fn prosthetic_tools_follow_the_exe_frame_for_frame() {
    let mut n = 0;
    for name in TOOLS {
        let Some((exe, ours)) = both(name) else { continue };
        n += 1;
        assert_eq!(ours.clips, exe.clips, "{name}: clips");
        assert_eq!(ours.paid, exe.paid, "{name}: emblems paid");
        // The firecracker: the carriers (to frame 35) are fixed and the first sparks 710002 /
        // 710012 start at 37 in both; how many of those live past their first update, and all
        // after, ride the random life draws (life 0 + lifeRandomRange 0.5: a draw under a frame
        // ends one unseen, and the exe's run has one of each first live at 38).
        let fixed = |r: &Run| r.bullets.iter().copied().filter(|&(f, _)| name != "firecracker" || f <= 35).collect::<Vec<_>>();
        assert_eq!(fixed(&ours), fixed(&exe), "{name}: bullets");
        if name == "firecracker" {
            for spark in [710002, 710012] {
                let first = |r: &Run| r.bullets.iter().find(|b| b.1 == spark).map(|b| b.0);
                assert_eq!(first(&ours), first(&exe), "firecracker: first {spark}");
            }
        }
        if ["shuriken_tap", "shuriken_hold", "flame_vent", "okinaga_vent", "mist_raven", "umbrella", "divine_abduction", "whistle", "firecracker", "spear", "spiral_spear"].contains(&name) {
            assert_eq!(ours.hits, exe.hits, "{name}: hits");
        }
    }
    if n == 0 {
        eprintln!("skip: no extracted/emu/prosth_*.json (tools/emu-cases prosth)");
    }
}

/// The spear's thrust: three overlapping AttackBehavior windows (root 107 level 0, middle 106
/// level 1, tip 105 level 2). In the exe each touches the General (the hit manager's contact
/// FUN_14099cbe0 puts him on its record's list: middle at 47, tip at 49) but only the root's hit
/// at 45 lands: his victim level 0 is hit, and 1 / 2 are not below it (combat::victim_level).
/// Ours lands the same one hit; before, every window hit (45, 47, 49).
#[test]
pub(crate) fn the_spears_three_windows_hit_once_like_the_exe() {
    let Some(v) = load("spear") else { return };
    let touched: Vec<(usize, i64)> = v["frames"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .flat_map(|(f, fr)| fr["events"].as_array().unwrap().iter().filter(|e| e["fn"] == "hit_list_add").map(move |e| (f, e["atk"].as_i64().unwrap())))
        .collect();
    if touched.is_empty() {
        eprintln!("skip: prosth_spear.json has no hit records (an older run)");
        return;
    }
    assert_eq!(touched, vec![(45, 107800107), (47, 107800106), (49, 107800105)], "the exe's contacts");
    let exe = exe_run(&v);
    assert_eq!(exe.hits, vec![45]);
    let ours = sim_run(78000, 3, v["dist"].as_f64().unwrap() as f32, 60);
    assert_eq!(ours.hits, exe.hits);
    // The rule itself: a lower level after a hit lands again; equal or higher does not; 255 always.
    let mut w = World::new();
    let (a, b) = (w.spawn_empty().id(), w.spawn_empty().id());
    let mut l = Vec::new();
    assert!(crate::combat::victim_level(&mut l, a, 1));
    assert!(!crate::combat::victim_level(&mut l, a, 1));
    assert!(!crate::combat::victim_level(&mut l, a, 2));
    assert!(crate::combat::victim_level(&mut l, a, 0));
    assert!(!crate::combat::victim_level(&mut l, a, 0));
    assert!(crate::combat::victim_level(&mut l, a, -1));
    assert!(crate::combat::victim_level(&mut l, b, 2));
}

/// Exploration: every recorded tool, the exe's run and ours side by side.
#[test]
#[ignore]
pub(crate) fn prosthetic_tools_print_against_the_exe() {
    for name in TOOLS {
        let Some((exe, ours)) = both(name) else { continue };
        println!("{name}:\n  exe  {exe:?}\n  ours {ours:?}");
    }
}
