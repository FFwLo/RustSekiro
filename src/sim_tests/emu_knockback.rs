//! The enemy's knockback against the exe (tools/emu-cases `knockback`, extracted/emu/knockback_*.json):
//! hits.rs's duels with Wolf's slash on the General standing (AI off) open, guarding, guarding with
//! his posture nearly gone (guard break), open with it nearly gone (posture break). Each frame the
//! knockback module ([chr+0x1ff8]+0xa8), the ChrCtrl +0x90 controller and both positions are read;
//! the hooks log the knockback starts FUN_140ba3420 (hit resolve: direction, distance) and
//! FUN_140ba3190 (HKS act SetDamageAnimType: the KnockBackParam field). docs/kb/damage.md
//! "Guard-break knockback".

use super::*;
use serde_json::Value;

fn load(name: &str) -> Option<Value> {
    let path = crate::paths::emu().join(format!("knockback_{name}.json"));
    let text = std::fs::read_to_string(path).ok()?;
    Some(serde_json::from_str(&text).unwrap())
}

/// The exe's run: the hit frame, the KnockBackParam field the reaction picked (FUN_140ba3190,
/// None: none), the General's speed field (+0x30 of the module, `kb`[8]) after the hit, and the
/// General's per-frame horizontal travel from the hit frame on.
struct ExeRun {
    hit: usize,
    field: Option<i64>,
    speed: f32,
    /// Module +0x14: the push's hold + decay (the field's two times).
    time: f32,
    controller_used: bool,
    steps: Vec<f32>,
}

fn exe_run(v: &Value) -> Option<ExeRun> {
    let fr = v["frames"].as_array().unwrap();
    let hit = fr.iter().position(|x| x["hits"].as_array().unwrap().iter().any(|h| h["defender"] == "general"))?;
    let field = fr[hit..]
        .iter()
        .flat_map(|x| x["events"].as_array().unwrap().iter())
        .find(|e| e["at"] == "kb_times" && e["chr"] == "general")
        .map(|e| e["field"].as_i64().unwrap());
    let speed = fr[hit]["general"]["kb"][8].as_f64().unwrap() as f32;
    let time = fr[hit]["general"]["kb"][1].as_f64().unwrap() as f32;
    // The controller: on (+0x54) or its push (+0x55) in any frame, or written from outside its tick.
    let controller_used = fr.iter().any(|x| {
        let f = &x["general"]["ctl_flags"];
        f[0].as_i64() != Some(0) || f[1].as_i64() != Some(0) || x["events"].as_array().unwrap().iter().any(|e| e["chr"] == "general" && (e["at"] == "ctl_set" || e["at"] == "ctl_write"))
    });
    let pos = |x: &Value| Vec3::new(x["general"]["pos"][0].as_f64().unwrap() as f32, 0.0, x["general"]["pos"][2].as_f64().unwrap() as f32);
    let steps = (hit + 1..fr.len()).map(|k| (pos(&fr[k]) - pos(&fr[k - 1])).length()).collect();
    Some(ExeRun { hit, field, speed, time, controller_used, steps })
}

/// Sv1's duel: the General 2 m in front of Wolf, AI idle, held until the hit in a ShieldBlock pose
/// (a just-guard one when `deflect`) when `guard`, else in his idle, `left` posture before his break; Wolf slashes
/// on frame 0. Returns per frame (the General's anim, position, knockback hold + decay left, its
/// speed, log).
fn sim_duel(guard: bool, deflect: bool, left: Option<f32>, frames: usize) -> Vec<(String, Vec3, f32, f32, String)> {
    let mut app = app_geom("c1020", None);
    {
        let world = app.world_mut();
        let mut q = world.query_filtered::<&mut Transform, With<Enemy>>();
        q.single_mut(world).unwrap().translation.z = 2.0;
        world.resource_mut::<crate::enemy::EnemyDebug>().mode = crate::enemy::AiMode::Idle;
    }
    for _ in 0..10 {
        app.update();
    }
    let pose = {
        let world = app.world_mut();
        let a = world.query_filtered::<&Actor, With<Enemy>>().single(world).unwrap();
        let combat = world.resource::<Combat>();
        let d = combat.data_of(a);
        let mut found: Vec<(String, f32)> = d
            .anims
            .iter()
            .flat_map(|(k, an)| an.events.iter().filter(|e| e.kind == 0 && e.flag_type() == Some(crate::player::FLAG_SHIELD_BLOCK)).map(move |e| (k.clone(), e.start + 0.05)))
            .filter(|(k, t)| d.flag(k, *t, crate::player::FLAG_SHIELD_BLOCK) && d.has_state_info(k, *t, crate::data::STATE_INFO_JUST_GUARD) == deflect)
            .collect();
        found.sort_by(|x, y| x.0.cmp(&y.0));
        // Open: his idle a000_000000 held (Sv1's AI-idle stance may carry ShieldBlock; the exe's
        // AI-off General stood unguarded).
        if guard { found.into_iter().next().expect("a guard pose") } else { ("a000_000000".to_string(), 0.1) }
    };
    {
        let world = app.world_mut();
        let mut a = world.query_filtered::<&mut Actor, With<Enemy>>().single_mut(world).unwrap();
        if let Some(l) = left {
            a.posture = a.posture_max - l;
        }
    }
    app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
    let mut out = Vec::new();
    let mut lines = 0;
    let mut held = true;
    for _ in 0..frames {
        if held {
            let world = app.world_mut();
            let mut a = world.query_filtered::<&mut Actor, With<Enemy>>().single_mut(world).unwrap();
            if a.anim != pose.0 {
                a.play(if guard { "Guard" } else { "Idle" }, &pose.0);
            }
            a.prev_t = pose.1;
            a.t = pose.1;
        }
        app.update();
        let log = {
            let l = &app.world().resource::<CombatLog>().lines;
            let s = l[lines.min(l.len())..].iter().map(|x| x.0.clone()).collect::<Vec<_>>().join(" | ");
            lines = l.len();
            s
        };
        if log.contains("blocked") || log.contains("hit:") || log.contains("DEFLECT") {
            held = false;
        }
        let world = app.world_mut();
        let (a, tf) = world.query_filtered::<(&Actor, &Transform), With<Enemy>>().single(world).unwrap();
        out.push((a.anim.clone(), tf.translation.with_y(0.0), a.kb_cont + a.kb_dec, a.kb_vel.length(), log));
    }
    out
}

/// The General's guard break as the exe moves him: the hit gives his knockback module a
/// direction and distance (FUN_140ba3420), the HKS reaction picks KnockBackParam field 14
/// guardBrake (FUN_140ba3190), and his row 0 has guardBrake 0 / 0, so the speed is 0: no engine
/// push; he moves by his GuardBreak clip's root motion alone. The ChrCtrl +0x90 controller
/// (FUN_140be6750) stays off in every duel: it is not a knockback (throws, warps, turns). Sv1
/// before: the block's guard_S push (row 0: 0.05 / 0.2 s) kept running under the break clip.
#[test]
pub(crate) fn enemy_guard_break_knockback_matches_the_exe() {
    let Some(brk) = load("w_break") else {
        eprintln!("skipped: no extracted/emu/knockback_w_break.json (tools/emu-cases knockback)");
        return;
    };
    let exe = exe_run(&brk).expect("the exe's slash broke the guard");
    assert_eq!(exe.field, Some(14), "guardBrake is KnockBackParam field 14");
    assert_eq!(exe.speed, 0.0, "row 0 guardBrake 0 / 0: no push");
    for name in ["w_break", "w_guard", "w_open", "w_broken", "w_deflect", "g_break", "g_open"] {
        if let Some(v) = load(name) {
            assert!(!exe_run(&v).is_some_and(|r| r.controller_used), "{name}: the ChrCtrl +0x90 controller ran");
        }
    }
    // (case, guarding, deflect pose, posture left before the break, the field the exe's reaction
    // picked, the travel tolerance: None = not compared)
    let cases = [
        ("w_break", true, false, Some(1.0), 14, Some(0.1)),
        // The exe's General walked (a000_001040) into the hit and its blend carries that motion
        // into the break clip; Sv1's held idle does not: 14 % short.
        ("w_broken", false, false, Some(1.0), 14, Some(0.15)),
        ("w_guard", true, false, None, 11, None),
        ("w_open", false, false, None, 2, Some(0.1)),
        ("w_deflect", true, true, None, 11, None),
    ];
    let mut failed = Vec::new();
    let mut check = |ok: bool, what: String| {
        if !ok {
            failed.push(what);
        }
    };
    for (name, guard, deflect, left, field, tol) in cases {
        let Some(v) = load(name) else { continue };
        let exe = exe_run(&v).unwrap_or_else(|| panic!("{name}: no hit in the exe"));
        check(exe.field == Some(field), format!("{name}: KnockBackParam field {:?}, not {field}", exe.field));
        let fr = sim_duel(guard, deflect, left, 90);
        let h = fr.iter().position(|f| f.4.contains("blocked") || f.4.contains("hit:") || f.4.contains("DEFLECT")).unwrap_or_else(|| panic!("{name}: no hit in Sv1"));
        // The push's times (module +0x14 = hold + decay of the picked field); none = no push.
        let st = fr[h..h + 3].iter().map(|f| f.2).fold(0.0f32, f32::max);
        let pushing = fr[h..].iter().any(|f| f.2 > 0.0);
        let n = exe.steps.len().min(fr.len() - h - 1).min(45);
        let te: f32 = exe.steps[..n].iter().sum();
        let ts: f32 = (h + 1..h + 1 + n).map(|k| (fr[k].1 - fr[k - 1].1).length()).sum();
        println!("{name}: exe speed {:.3} time {:.3} travel {te:.3} over {n} frames; Sv1 time {st:.3} pushing {pushing} travel {ts:.3} ({})", exe.speed, exe.time, fr[h].0);
        if exe.speed == 0.0 {
            check(!pushing, format!("{name}: Sv1 pushes, the exe does not: {:?}", fr[h..h + 5].iter().map(|f| f.2).collect::<Vec<_>>()));
        } else {
            check((st - exe.time).abs() < 1e-3, format!("{name}: push hold + decay exe {} sim {st}", exe.time));
        }
        // gap: the push's distance differs by the facing factor where the direction does
        // (the exe's attack motion vector, Sv1's root to root, docs/kb/damage.md 2026-10-12), so
        // guarding (the hold drifts the General sideways in Sv1) is not compared by travel; nor the
        // deflect (the exe's General goes on to his counter a000_003057).
        if let Some(tol) = tol {
            check((te - ts).abs() < tol * te.max(0.1), format!("{name}: travel after the hit exe {te} sim {ts}"));
        }
    }
    assert!(failed.is_empty(), "{failed:#?}");
}
