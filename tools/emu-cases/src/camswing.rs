//! camswing: the free camera's swing while Wolf runs with the move stick held sideways (E32), on
//! the Naomori-arena snapshot. Each frame is the game's own loop: Wolf's pad frame written (the
//! move stick, SP_MOVE held for a sprint), Wolf's whole chr frame FUN_140a4b570 (the pad
//! manipulator turns the stick into a world move from the camera, his locomotion and proxy move
//! him), then the follow camera FUN_140734790 -> FUN_14073c260. So the stick -> camera -> move
//! loop is closed by the exe itself; nothing is put in by hand.
//!
//! `camswing[:name]` -> extracted/emu/camswing_<name>.json: per frame Wolf's position and yaw,
//! the manipulator's move vector (+0xc0), the follow camera's fields 0x0..0x420 before and after.

use std::path::Path;

use serde_json::json;

use super::acttrace::{arm_inputs, press, Input, CHR_FRAME, MENU_STATE, SP_MOVE, WORLD_CHR_MAN};
use super::{chr_cam, proxy, turn, Emu};

const FRAME: u64 = 0x1_4073_4790;
const FIELDS: u64 = 0x420;

struct Scenario {
    name: &'static str,
    frames: usize,
    dt: f32,
    input: fn(usize) -> Input,
}

const RIGHT: [f32; 2] = [0.0, 1.0];
const LEFT: [f32; 2] = [0.0, -1.0];

const SCENARIOS: [Scenario; 6] = [
    // The move stick held right / left at a run, then let go.
    Scenario { name: "run_right", frames: 480, dt: 1.0 / 60.0, input: |f| Input { arms: &[], stick: if f < 420 { RIGHT } else { [0.0; 2] } } },
    Scenario { name: "run_left", frames: 300, dt: 1.0 / 60.0, input: |_| Input { arms: &[], stick: LEFT } },
    // Sprint: SP_MOVE held with the stick (as acttrace's sprint).
    Scenario { name: "sprint_right", frames: 480, dt: 1.0 / 60.0, input: |_| Input { arms: &[SP_MOVE], stick: RIGHT } },
    // Half the stick: a walk.
    Scenario { name: "walk_right", frames: 300, dt: 1.0 / 60.0, input: |_| Input { arms: &[], stick: [0.0, 0.4] } },
    // Diagonal forward-right at a run.
    Scenario { name: "run_fwd_right", frames: 300, dt: 1.0 / 60.0, input: |_| Input { arms: &[], stick: [0.7071, 0.7071] } },
    // 30 fps.
    Scenario { name: "run_right_30", frames: 240, dt: 1.0 / 30.0, input: |_| Input { arms: &[], stick: RIGHT } },
];

fn f32s<const N: usize>(emu: &mut Emu, at: u64) -> [f32; N] {
    std::array::from_fn(|i| f32::from_le_bytes(emu.peek(at + 4 * i as u64)))
}

fn one(new_emu: &dyn Fn() -> Emu, out: &Path, sc: &Scenario) {
    let t = std::time::Instant::now();
    let mut emu = new_emu();
    let w = turn::wolf(&mut emu);
    let ids = arm_inputs(&mut emu);
    emu.limit = 500_000_000;
    // As acttrace / proxy: the CRT's ISA level 1 (no AVX2), a plain longjmp for the HKS GC, no menu.
    emu.raw_write(0x1_43c1_d520, &1u32.to_le_bytes());
    emu.raw_write(proxy::LONGJMP, proxy::PLAIN_LONGJMP);
    emu.raw_write(MENU_STATE, &[0x31, 0xc0, 0xc3]);
    let menus = emu.call(0x1_40d8_72c0, &[], &[]).rax;
    if menus != 0 {
        emu.u64(menus + 0x100, 0);
    }
    let wcm = emu.peek_u64(WORLD_CHR_MAN);
    let cam = chr_cam(&mut emu);
    let follow = emu.peek_u64(cam + 0x60);
    let world = emu.peek_u64(0x1_43d6_d640);
    let world = emu.peek_u64(world + 0x98);
    let mut frames = Vec::new();
    let mut stopped = None;
    for f in 0..sc.frames {
        let inp = (sc.input)(f);
        press(&mut emu, w.stick, &ids, &inp);
        proxy::flush_tb(&emu);
        let r = emu.call(CHR_FRAME, &[wcm, 0, w.player], &[0.0, sc.dt]);
        if r.stopped.is_some() {
            stopped = r.stopped;
            break;
        }
        let pos: [f32; 3] = f32s(&mut emu, w.phys + 0x80);
        let yaw = f32::from_le_bytes(emu.peek(w.phys + 0x74));
        let mc0: [f32; 4] = f32s(&mut emu, w.manip + 0xc0);
        let before: Vec<f32> = (0..FIELDS / 4).map(|i| f32::from_le_bytes(emu.peek(follow + 4 * i))).collect();
        let r = emu.call(FRAME, &[cam, 0, w.player, world, 1], &[0.0, sc.dt]);
        if r.stopped.is_some() {
            stopped = r.stopped;
            break;
        }
        let after: Vec<f32> = (0..FIELDS / 4).map(|i| f32::from_le_bytes(emu.read(follow + 4 * i))).collect();
        frames.push(json!({"stick": inp.stick, "sprint": !inp.arms.is_empty(), "wolf": pos, "yaw": yaw, "mc0": mc0, "before": before, "after": after}));
        if f % 60 == 0 {
            println!("  {} f{f}: wolf {:.2?} yaw {:.3} cam yaw {:.4}", sc.name, pos, yaw, after[0x174 / 4]);
        }
    }
    let cy = |i: usize| frames.get(i).map_or(0.0, |v| v["after"][0x174 / 4].as_f64().unwrap_or(0.0));
    println!("{}: {} frames in {:.0} s, cam yaw {:.3} -> {:.3}, stopped {:?}", sc.name, frames.len(), t.elapsed().as_secs_f32(), cy(0), cy(frames.len().saturating_sub(1)), stopped.map(|(a, e)| (format!("{a:#x}"), e)));
    let body = json!({"functions": format!("{CHR_FRAME:#x} then {FRAME:#x}"), "name": sc.name, "dt": sc.dt, "fields": format!("ChrExFollowCam 0x0..{FIELDS:#x} as f32"), "frames": frames});
    std::fs::write(out.join(format!("camswing_{}.json", sc.name)), serde_json::to_string(&body).unwrap()).unwrap();
}

/// Dispatch for main.rs: `camswing` (all) or `camswing:<name>`.
pub(crate) fn run(new_emu: &dyn Fn() -> Emu, out: &Path, want: &[&str]) {
    for w in want.iter().filter(|w| w.starts_with("camswing")) {
        let name = w.strip_prefix("camswing:");
        for sc in SCENARIOS.iter().filter(|s| name.is_none_or(|n| n == s.name)) {
            one(new_emu, out, sc);
        }
    }
}
