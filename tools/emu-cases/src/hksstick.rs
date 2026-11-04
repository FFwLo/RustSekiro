//! `hksstick`: which frame's stick Wolf's HKS reads. Whole chr frames FUN_140a4b570 from the
//! snapshot, the pad fed as acttrace does (the stick floats [pad +0x38f8], the arm bytes); the
//! stick changes on frame `at`. Hooks log, in call order: the locomotion variables
//! FUN_140b4afd0 (behaviour +0xbac MoveSpeedLevel, +0xbb0 MoveAngle, the four angle / level
//! pairs from +0xb30), the Havok behaviour step FUN_141015fa0, the frame's first HKS env call
//! FUN_140b36410 and every HKS act FUN_140b32410 (id) with the stick-derived fields read right
//! then: the behaviour block +0xb30..+0xbb4, the manipulator's move vector (+0xc0) and its
//! raw stick (+0x340), and the physics yaw (+0x74).
//!
//! `hksstick [hksstick_<case>...]` writes extracted/emu/hksstick_<case>.json. Results:
//! docs/kb/frame-coverage.md "The HKS's stick".

use std::ffi::c_void;
use std::path::Path;
use std::sync::Mutex;

use serde_json::{json, Value};

use super::acttrace::{self, Input, CHR_FRAME, DT, SP_MOVE, WORLD_CHR_MAN};
use super::{proxy, turn, uc, Emu};

const LOCO_VARS: u64 = 0x1_40b4_afd0;
const HAVOK_STEP: u64 = 0x1_4101_5fa0;
const HKS_ENV: u64 = 0x1_40b3_6410;
const HKS_ACT: u64 = 0x1_40b3_2410;

/// (behaviour object, manipulator, physics) of the run.
static OBJ: Mutex<(u64, u64, u64)> = Mutex::new((0, 0, 0));
static LOG: Mutex<Vec<Value>> = Mutex::new(Vec::new());
static ENV_SEEN: Mutex<bool> = Mutex::new(false);

fn reg(uc: uc::Engine, id: i32) -> u64 {
    let mut v = 0u64;
    unsafe { uc::uc_reg_read(uc, id, &mut v as *mut u64 as _) };
    v
}

fn floats(uc: uc::Engine, at: u64, n: usize) -> Vec<f32> {
    let mut b = vec![0u8; 4 * n];
    unsafe { uc::uc_mem_read(uc, at, b.as_mut_ptr() as _, b.len()) };
    b.chunks(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect()
}

/// The stick-derived fields right now.
fn state(uc: uc::Engine) -> Value {
    let (beh, manip, phys) = *OBJ.lock().unwrap();
    json!({
        "level": floats(uc, beh + 0xbac, 1)[0],
        "move_angle": floats(uc, beh + 0xbb0, 1)[0],
        "block": floats(uc, beh + 0xb30, 0x22),
        "move": floats(uc, manip + 0xc0, 3),
        "raw": floats(uc, manip + 0x340, 2),
        "yaw": floats(uc, phys + 0x74, 1)[0],
    })
}

extern "C" fn on_code(uc: uc::Engine, addr: u64, _size: u32, _data: *mut c_void) {
    let v = match addr {
        LOCO_VARS => {
            if reg(uc, uc::RCX) != OBJ.lock().unwrap().0 {
                return;
            }
            json!({"at": "loco_vars"})
        }
        HAVOK_STEP => json!({"at": "havok_step"}),
        HKS_ENV => {
            let mut seen = ENV_SEEN.lock().unwrap();
            if *seen {
                return;
            }
            *seen = true;
            json!({"at": "hks_env_first", "s": state(uc)})
        }
        _ => json!({"at": "hks_act", "id": reg(uc, uc::RDX) as u32 as i32, "s": state(uc)}),
    };
    LOG.lock().unwrap().push(v);
}

struct Case {
    name: &'static str,
    frames: usize,
    /// The pad on frame f: (stick [forward, side], step pressed).
    input: fn(usize) -> ([f32; 2], bool),
}

fn rot(deg: f32) -> [f32; 2] {
    let r = deg.to_radians();
    [r.cos(), r.sin()]
}

const CASES: &[Case] = &[
    // A run at full stick, half stick from frame 8 (same way: the gate takes it at once).
    Case { name: "half", frames: 14, input: |f| (if f < 8 { [1.0, 0.0] } else { [0.5, 0.0] }, false) },
    // The run let go on frame 8 (the run stop).
    Case { name: "release", frames: 14, input: |f| (if f < 8 { [1.0, 0.0] } else { [0.0, 0.0] }, false) },
    // A run, the stick turned 40 deg on frame 8 (inside the 45 deg gate) with the step pressed
    // on frames 8-9: the step's direction.
    Case { name: "step_turn", frames: 16, input: |f| (if f < 8 { [1.0, 0.0] } else { rot(40.0) }, (8..10).contains(&f)) },
];

fn run_case(new_emu: &dyn Fn() -> Emu, out: &Path, c: &Case) {
    let mut emu = new_emu();
    let w = turn::wolf(&mut emu);
    let ids = acttrace::arm_inputs(&mut emu);
    let wcm = emu.peek_u64(WORLD_CHR_MAN);
    emu.limit = 500_000_000;
    emu.raw_write(0x1_43c1_d520, &1u32.to_le_bytes());
    emu.raw_write(proxy::LONGJMP, proxy::PLAIN_LONGJMP);
    emu.raw_write(acttrace::MENU_STATE, &[0x31, 0xc0, 0xc3]);
    let menus = emu.call(0x1_40d8_72c0, &[], &[]).rax;
    if menus != 0 {
        emu.u64(menus + 0x100, 0);
    }
    // Map the fields the hooks read.
    let _ = emu.peek::<0x90>(w.beh + 0xb30);
    let _ = emu.peek::<16>(w.manip + 0xc0);
    let _ = emu.peek::<8>(w.manip + 0x340);
    let _ = emu.peek::<4>(w.phys + 0x74);
    *OBJ.lock().unwrap() = (w.beh, w.manip, w.phys);
    for a in [LOCO_VARS, HAVOK_STEP, HKS_ENV, HKS_ACT] {
        emu.hook(uc::HOOK_CODE, on_code as *const c_void, a, a);
    }
    let mut frames = Vec::new();
    for f in 0..c.frames {
        let (stick, step) = (c.input)(f);
        let arms: &[u32] = if step { &[SP_MOVE] } else { &[] };
        acttrace::press(&mut emu, w.stick, &ids, &Input { arms, stick });
        proxy::flush_tb(&emu);
        LOG.lock().unwrap().clear();
        *ENV_SEEN.lock().unwrap() = false;
        let r = emu.call(CHR_FRAME, &[wcm, 0, w.player], &[0.0, DT]);
        let log = std::mem::take(&mut *LOG.lock().unwrap());
        let level = f32::from_le_bytes(emu.peek(w.beh + 0xbac));
        let yaw = f32::from_le_bytes(emu.peek(w.phys + 0x74));
        println!("{} f{f} stick {stick:?} step {step}: level {level} yaw {yaw} anim {}", c.name, acttrace::anim(&mut emu, w.player));
        frames.push(json!({"f": f, "stick": stick, "step": step, "anim": acttrace::anim(&mut emu, w.player), "yaw": yaw, "log": log}));
        if let Some((a, e)) = r.stopped {
            println!("  stopped at {a:#x} error {e}");
            break;
        }
    }
    let v = json!({"case": c.name, "dt": DT, "frames": frames});
    std::fs::write(out.join(format!("hksstick_{}.json", c.name)), serde_json::to_string(&v).unwrap()).unwrap();
}

pub fn run(new_emu: &dyn Fn() -> Emu, out: &Path, want: &[&str]) {
    let names: Vec<&str> = want.iter().filter_map(|w| w.strip_prefix("hksstick_")).collect();
    for c in CASES {
        if names.is_empty() || names.contains(&c.name) {
            run_case(new_emu, out, c);
        }
    }
}
