//! When a pressed action's clip starts, against the user's own sekiro.exe: tools/emu-cases
//! `animstart` runs Wolf's whole chr frames FUN_140a4b570 on the Naomori-arena snapshot with the
//! frame's stages hooked (kb/frame-coverage.md "One frame's order"). The pad is read in vf22, the
//! HKS runs inside the Havok behaviour step after the current clip's TAE update, and the clip it
//! asks for takes its first window [0, dt] in the next frame's step. Skips when
//! extracted/emu/animstart_*.json is missing.

use super::*;

fn emu_json(name: &str) -> Option<serde_json::Value> {
    let text = std::fs::read_to_string(crate::paths::emu().join(format!("{name}.json"))).ok()?;
    serde_json::from_str(&text).ok()
}

/// The exe's first frame whose TAE update of `clip` starts at 0, and that update's end time.
fn exe_first_step(v: &serde_json::Value, clip: i64) -> Option<(usize, f32)> {
    v["frames"].as_array()?.iter().enumerate().find_map(|(i, f)| {
        f["log"].as_array()?.iter().find(|e| e["at"] == "tae" && e["anim"].as_i64() == Some(clip) && e["t0"].as_f64() == Some(0.0)).map(|e| (i, e["t1"].as_f64().unwrap() as f32))
    })
}

/// Attack, guard and the Loaded Axe pressed on frame 2 (held 3 / 2 / 3 frames): the exe's clip
/// takes its first step on frame 3, over [0, 1/60]. Ours: the first frame Wolf is in that clip
/// past 0, and its time then.
#[test]
pub(crate) fn a_pressed_clip_starts_the_frame_after_the_press() {
    let cases: [(&str, Action, i64, &str); 3] = [
        ("attack", Action::Attack, 50300000, "a050_300000"),
        ("guard", Action::Guard, 50203000, "a050_203000"),
        ("prosthetic", Action::Prosthetic, 73400000, "a073_400000"),
    ];
    let mut compared = 0;
    for (name, action, id, clip) in cases {
        let Some(v) = emu_json(&format!("animstart_{name}")) else { continue };
        let press = v["press"].as_u64().unwrap() as usize;
        let (exe, exe_t) = exe_first_step(&v, id).unwrap_or_else(|| panic!("{name}: the exe never stepped {id}"));
        assert_eq!(exe, press + 1, "{name}: the exe's first step");
        let hold = if name == "guard" { 2 } else { 3 };
        let mut app = app();
        app.world_mut().resource_mut::<crate::config::GameConfig>().player.prosthetics = vec![73000];
        move_enemy_far(&mut app);
        let mut ours = None;
        for f in 0..press + 6 {
            {
                let mut pad = app.world_mut().resource_mut::<PadInput>();
                let on = (press..press + hold).contains(&f);
                match action {
                    Action::Attack => pad.attack_held = on,
                    Action::Guard => pad.guard_held = on,
                    _ => pad.prosthetic_held = on,
                }
                if f == press {
                    pad.press(action);
                }
            }
            app.update();
            let (_, anim, t, _) = player_state(&mut app);
            if ours.is_none() && anim == clip && t > 0.0 {
                ours = Some((f, t));
            }
        }
        let (f, t) = ours.unwrap_or_else(|| panic!("{name}: ours never played {clip}"));
        assert_eq!(f, exe, "{name}: first step of {clip} (exe frame {exe}, ours {f})");
        assert!((t - exe_t).abs() < 1e-4, "{name}: clip time after the first step exe {exe_t} ours {t}");
        compared += 1;
    }
    if compared > 0 {
        assert_eq!(compared, 3);
    }
}
