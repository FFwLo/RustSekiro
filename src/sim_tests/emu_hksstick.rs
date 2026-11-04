//! Which frame's stick Wolf's HKS reads, against the user's own sekiro.exe: tools/emu-cases
//! `hksstick` runs Wolf's whole chr frames FUN_140a4b570 on the Naomori-arena snapshot with the
//! stick changed on frame 8 and logs, at the frame's first HKS env call, the stick-derived
//! behaviour fields (MoveSpeedLevel +0xbac, the angle / level pairs from +0xb30) that
//! FUN_140b4afd0 wrote earlier in the same frame (vf22, before the Havok step). The exe's HKS
//! reads its own frame's stick; Sv1's script stands for the exe's HKS of the frame before
//! (kb/frame-coverage.md "One frame's order"), so it reads the stick taken the tick before
//! (`player::Turn::hks`). Skips when extracted/emu/hksstick_*.json is missing.

use super::*;

fn emu_json(name: &str) -> Option<serde_json::Value> {
    let text = std::fs::read_to_string(crate::paths::emu().join(format!("{name}.json"))).ok()?;
    serde_json::from_str(&text).ok()
}

/// The exe's frame: (stick [forward, side], step pressed, MoveSpeedLevel and the first angle /
/// level pair's level at the HKS, the HKS fired an event: act 9101 ResetInputQueue).
fn exe_frames(v: &serde_json::Value) -> Vec<([f32; 2], bool, f32, f32, bool)> {
    let f = |x: &serde_json::Value| x.as_f64().unwrap() as f32;
    v["frames"]
        .as_array()
        .unwrap()
        .iter()
        .map(|fr| {
            let log = fr["log"].as_array().unwrap();
            let s = &log.iter().find(|e| e["at"] == "hks_env_first").expect("an HKS env call")["s"];
            let fired = log.iter().any(|e| e["at"] == "hks_act" && e["id"] == 9101);
            ([f(&fr["stick"][0]), f(&fr["stick"][1])], fr["step"].as_bool().unwrap(), f(&s["level"]), f(&s["block"][4]), fired)
        })
        .collect()
}

/// A run at full stick then half (`half`), let go (`release`), or turned 40 deg with the step
/// pressed (`step_turn`), all on frame 8. The exe's HKS on frame f reads frame f's stick (the
/// manipulator's 45 deg gate takes a press from rest a frame late): MoveSpeedLevel 0.5 / 0 on
/// frame 8 itself, the run stop fired on frame 8 (act 9101). Ours: the script on tick f + 1 reads
/// the same MoveSpeedLevel / AttackStickLevel (the pair's level is at +0x10 of each 0x20 block).
#[test]
pub(crate) fn the_script_reads_the_stick_of_its_buttons_frame() {
    let mut compared = 0;
    for name in ["half", "release", "step_turn"] {
        let Some(v) = emu_json(&format!("hksstick_{name}")) else { continue };
        let exe = exe_frames(&v);
        // The exe's own rule: frame f's HKS sees frame f's stick (past the gate's first frame).
        for (f, (stick, _, level, pair, _)) in exe.iter().enumerate().skip(1) {
            let want = (stick[0] * stick[0] + stick[1] * stick[1]).sqrt();
            assert!((level - want).abs() < 1e-4 && (pair - want).abs() < 1e-4, "{name} f{f}: the exe's HKS read level {level} / {pair}, the frame's stick {want}");
        }
        if name == "release" {
            assert!(exe[8].4 && !exe[7].4, "release: the exe's run stop fires on the let-go frame");
        }
        let mut app = app();
        move_enemy_far(&mut app);
        let mut ours = Vec::new();
        for (stick, step, _, _, _) in exe.iter().chain(std::iter::once(exe.last().unwrap())) {
            {
                let mut pad = app.world_mut().resource_mut::<PadInput>();
                pad.stick = Vec2::new(-stick[1], stick[0]);
                pad.dodge_held = *step;
                if *step && !ours.last().is_some_and(|(_, _, s): &(f64, f64, bool)| *s) {
                    pad.press(Action::Step);
                }
            }
            app.update();
            let s = app.world().get_non_send::<crate::hks_wolf::WolfScript>().expect("the script runs");
            ours.push((s.engine_var("MoveSpeedLevel").unwrap_or(0.0), s.engine_var("AttackStickLevel").unwrap_or(0.0), *step));
        }
        for (f, (_, _, level, pair, _)) in exe.iter().enumerate() {
            let (l, p, _) = ours[f + 1];
            assert!((l as f32 - level).abs() < 1e-4, "{name}: exe frame {f} MoveSpeedLevel {level}, ours tick {} {l}", f + 1);
            assert!((p as f32 - pair).abs() < 1e-4, "{name}: exe frame {f} AttackStickLevel {pair}, ours tick {} {p}", f + 1);
        }
        compared += 1;
    }
    if compared > 0 {
        assert_eq!(compared, 3);
    }
}
