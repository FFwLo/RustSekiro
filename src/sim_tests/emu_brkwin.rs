//! The deathblow window against the exe (row E31; tools/emu-cases `brkwin`,
//! extracted/emu/brkwin_*.json): the General's posture preset to 1, Wolf's slash 5000010 breaks him
//! in whole world frames, which run on until he has left his break anim. The exe's window is his
//! throw module's +0x7a = 103 (TAE 932's break span of TrunkCollapseFront / Back); he leaves the
//! break pose through its AI move window (flag 78, frame 88 of 90: c9997 TrunkCollapse*_onUpdate ->
//! DamageCommonFunction -> ExecActiveTransition), before IsAnimEnd. docs/kb/guard-deflect.md
//! "Deathblow window (E31)".

use super::*;
use serde_json::Value;

fn load(name: &str) -> Option<Value> {
    let text = std::fs::read_to_string(crate::paths::emu().join(format!("brkwin_{name}.json"))).ok()?;
    Some(serde_json::from_str(&text).unwrap())
}

/// The exe's run, counted from the first frame of the break anim.
struct ExeBreak {
    clip: i64,
    /// Frames with the break state 103 / 108 in +0x7a.
    open: usize,
    /// The clip time after its last update before another anim played.
    exit_t: f32,
    /// Posture left / max on each frame of the break.
    posture: Vec<f32>,
}

fn exe_break(v: &Value) -> ExeBreak {
    let fr = v["frames"].as_array().unwrap();
    let g = |x: &Value, k: &str| x["general"][k].as_i64().unwrap();
    let b = fr.iter().position(|x| matches!(g(x, "anim") % 1_000_000, 8300 | 8301 | 8310)).expect("no break anim in the exe run");
    let clip = g(&fr[b], "anim");
    let open = fr[b..].iter().take_while(|x| matches!(g(x, "throw_7a"), 103 | 108)).count();
    let held = fr[b..].iter().take_while(|x| g(x, "anim") == clip).count();
    assert!(b + held < fr.len(), "the exe run ended inside the break anim");
    let last = &fr[b + held - 1]["general"]["tae"];
    let exit_t = last.as_array().unwrap().iter().find(|c| c[0].as_i64() == Some(clip)).map(|c| c[2].as_f64().unwrap() as f32).unwrap();
    let posture = fr[b..b + held].iter().map(|x| g(x, "posture").max(0) as f32 / g(x, "posture_max") as f32).collect();
    ExeBreak { clip, open, exit_t, posture }
}

/// Sv1 the same way: the General broken (from behind: TrunkCollapseBack), his AI held off (Sv1's
/// map-script hold), one frame per update: (break clip, window frames, frames until another anim,
/// posture left / max per break frame, the clip's AI move window open at a clip time).
fn sim_break(behind: bool, frames: usize, exit_t: f32) -> (String, usize, usize, Vec<f32>, bool) {
    let mut app = app();
    {
        let world = app.world_mut();
        world.resource_scope(|world, combat: Mut<Combat>| {
            let mut q = world.query_filtered::<(&mut Enemy, &mut Actor), Without<crate::enemy::Cast>>();
            let (mut e, mut a) = q.single_mut(world).unwrap();
            e.ai_off = true;
            a.posture = a.posture_max;
            e.on_posture_break(&mut a, &combat, behind);
        });
    }
    let clip = enemy_state(&mut app).1;
    let (mut open, mut held, mut open_done) = (0, None, false);
    let mut posture = Vec::new();
    let mut move_open = false;
    for f in 0..frames {
        app.update();
        let world = app.world_mut();
        let ent = boss_in_play(world);
        let combat = world.resource::<Combat>();
        let (a, e) = (world.get::<Actor>(ent).unwrap(), world.get::<Enemy>(ent).unwrap());
        if f == 0 {
            // The update that ended at the exit time: the next frame's HKS reads the flags it
            // dispatched (ExecActiveTransition: combo 23, attack 86, step 79, move 78).
            let at = crate::tae_window::At::Step(exit_t - 1.0 / 60.0, exit_t);
            move_open = [23, 86, 79, 78].iter().any(|&fl| combat.data_of(a).flag(&clip, at, fl));
        }
        if !open_done && matches!(crate::player::def_throw_state(combat, a, e), 103 | 108) {
            open += 1;
        } else {
            open_done = true;
        }
        if a.anim != clip {
            held = Some(f);
            break;
        }
        posture.push((a.posture_max - a.posture).max(0.0) / a.posture_max);
    }
    (clip, open, held.expect("Sv1 never left the break anim"), posture, move_open)
}

#[test]
pub(crate) fn deathblow_window_matches_the_exe() {
    let mut ran = 0;
    for (name, behind) in [("front_ai_off", false), ("back_ai_off", true), ("front_ai_on", false)] {
        let Some(v) = load(name) else { continue };
        ran += 1;
        let x = exe_break(&v);
        let (clip, open, held, posture, move_open) = sim_break(behind, 240, x.exit_t);
        assert_eq!(clip, format!("a{:03}_{:06}", x.clip / 1_000_000, x.clip % 1_000_000), "{name}: break anim");
        // The window: 151 frames in the exe (clip 0.017 - 2.517 s at 60 fps: TAE 932 0-75 of 90).
        // Sv1 one more: its enemy tick reads the step before this frame's clip advance.
        assert!(open.abs_diff(x.open) <= 1, "{name}: deathblow window {open} frames, exe {}", x.open);
        // The exe left the pose through the clip's AI cancel windows (a walk / turn the AI asked
        // for); Sv1 with its AI held plays the clip out (no timer).
        assert!(move_open, "{name}: the exe left {clip} at {:.4} s, outside its AI cancel windows in Sv1's data", x.exit_t);
        assert!(x.exit_t > 2.9 && x.exit_t < 3.0, "{name}: exe exit {:.4}", x.exit_t);
        assert_eq!(held, 181, "{name}: Sv1 held the break clip {held} frames");
        // Posture: held empty through the window (StaminaControlParam type 5), then 30 % (type 6).
        for (k, (s, e)) in posture.iter().zip(&x.posture).enumerate() {
            if k + 2 < x.open || k > x.open + 2 {
                assert!((s - e).abs() < 0.02, "{name}: posture left on break frame {k}: Sv1 {s:.3}, exe {e:.3}");
            }
        }
    }
    if ran == 0 {
        eprintln!("skip: no brkwin_*.json (tools/emu-cases brkwin)");
    }
}
