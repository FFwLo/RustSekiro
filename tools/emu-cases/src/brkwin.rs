//! `brkwin`: how long a posture-broken enemy stays broken (row E31). The General's posture is
//! preset to 1 and Wolf's slash 5000010 lands on him (hits.rs's w_open placement); then whole
//! world frames (the hit manager step, the WorldChrMan pre tasks, both chr frames, the post tasks)
//! run until he has been out of his break anim for a while. Per frame: the General's TAE clips
//! with their times (TAE update FUN_140b5c920), the TimeAct module's current anim, his throw module
//! (bag +0x88) +0x7a (the state TAE 932 writes each frame) / +0x7b (his own), posture +0x148 / max,
//! HP, and the action-flag module's anim-end bits (+0xaf0, HKS env 339 IsAnimEnd).
//!
//! `brkwin [brkwin_<name>...]` writes extracted/emu/brkwin_<name>.json. Results:
//! docs/kb/guard-deflect.md "Deathblow window (E31)".

use std::ffi::c_void;
use std::path::Path;
use std::sync::Mutex;

use serde_json::{json, Value};

use super::acttrace::{self, Input, ATTACK, CHR_FRAME, DT, TASKS_POST, TASKS_PRE, WORLD_CHR_MAN};
use super::{proxy, turn, uc, Emu};

const HIT_MANAGER: u64 = 0x1_43d7_7ef0;
const HIT_STEP: u64 = 0x1_409a_0dc0;
const TAE_UPDATE: u64 = 0x1_40b5_c920;

static ANIMS: Mutex<Vec<(i64, f32, f32)>> = Mutex::new(Vec::new());

extern "C" fn on_tae(uc: uc::Engine, _addr: u64, _size: u32, _data: *mut c_void) {
    let mut rcx = 0u64;
    let mut x1 = [0u8; 16];
    let mut x2 = [0u8; 16];
    unsafe {
        uc::uc_reg_read(uc, uc::RCX, &mut rcx as *mut u64 as _);
        uc::uc_reg_read(uc, uc::XMM0 + 1, x1.as_mut_ptr() as _);
        uc::uc_reg_read(uc, uc::XMM0 + 2, x2.as_mut_ptr() as _);
    }
    let f = |b: &[u8; 16]| f32::from_le_bytes(b[..4].try_into().unwrap());
    ANIMS.lock().unwrap().push((rcx as u32 as i32 as i64, f(&x1), f(&x2)));
    ORDER.lock().unwrap().push(format!("tae {} {:.4}->{:.4}", rcx as u32 as i32, f(&x1), f(&x2)));
}

/// In call order within a frame: TAE updates, writes to the anim-end bits (action-flag module
/// +0xaf0) with the writer, env 339 (IsAnimEnd) asks.
static ORDER: Mutex<Vec<String>> = Mutex::new(Vec::new());
const HOOK_MEM_WRITE: i32 = 2048;
/// The HKS env dispatcher (rcx ctx, rdx env id).
const ENV: u64 = 0x1_40b3_6410;

extern "C" fn on_end_write(uc: uc::Engine, _kind: i32, addr: u64, size: i32, value: i64, _data: *mut c_void) {
    let mut rip = 0u64;
    unsafe { uc::uc_reg_read(uc, uc::RIP, &mut rip as *mut u64 as _) };
    ORDER.lock().unwrap().push(format!("endbits+{:#x}/{size} = {value:#x} at {rip:#x}", addr & 7));
}

extern "C" fn on_env(uc: uc::Engine, _addr: u64, _size: u32, _data: *mut c_void) {
    let mut rdx = 0u64;
    unsafe { uc::uc_reg_read(uc, uc::RDX, &mut rdx as *mut u64 as _) };
    if rdx as u32 == 339 {
        ORDER.lock().unwrap().push("env339".into());
    }
}

fn module(emu: &mut Emu, chr: u64, off: u64) -> u64 {
    let bag = emu.peek_u64(chr + 0x1ff8);
    emu.peek_u64(bag + off)
}
fn i32at(emu: &mut Emu, at: u64) -> i32 {
    i32::from_le_bytes(emu.peek(at))
}

/// A run: the AI on or off (the snapshot's disable-AI flag), Wolf in front of or behind the General.
struct Run {
    name: &'static str,
    ai: bool,
    behind: bool,
    frames: usize,
}

const RUNS: [Run; 3] = [
    Run { name: "front_ai_off", ai: false, behind: false, frames: 260 },
    Run { name: "front_ai_on", ai: true, behind: false, frames: 260 },
    Run { name: "back_ai_off", ai: false, behind: true, frames: 260 },
];

fn general_state(emu: &mut Emu, g: u64) -> Value {
    let data = module(emu, g, 0x18);
    let th = module(emu, g, 0x88);
    let m0 = module(emu, g, 0);
    json!({
        "hp": i32at(emu, data + 0x130), "posture": i32at(emu, data + 0x148), "posture_max": i32at(emu, data + 0x14c),
        "throw_7a": emu.peek::<1>(th + 0x7a)[0], "throw_7b": emu.peek::<1>(th + 0x7b)[0],
        "anim_end_bits": emu.peek_u64(m0 + 0xaf0),
        "anim": acttrace::anim(emu, g),
    })
}

fn run_one(new_emu: &dyn Fn() -> Emu, out: &Path, r: &Run) {
    let t = std::time::Instant::now();
    let mut emu = new_emu();
    let w = turn::wolf(&mut emu);
    let ids = acttrace::arm_inputs(&mut emu);
    let g = acttrace::general(&mut emu);
    let _ai = acttrace::ai_of(&mut emu, g);
    let wcm = emu.peek_u64(WORLD_CHR_MAN);
    let mgr = emu.peek_u64(HIT_MANAGER);
    emu.limit = 500_000_000;
    emu.raw_write(0x1_43c1_d520, &1u32.to_le_bytes());
    emu.raw_write(proxy::LONGJMP, proxy::PLAIN_LONGJMP);
    emu.raw_write(acttrace::MENU_STATE, &[0x31, 0xc0, 0xc3]);
    let menus = emu.call(0x1_40d8_72c0, &[], &[]).rax;
    if menus != 0 {
        emu.u64(menus + 0x100, 0);
    }
    if r.ai {
        emu.raw_write(0x1_43d7_a376, &[0]);
    }
    acttrace::every_frame(&mut emu, g);
    emu.hook(uc::HOOK_CODE, on_tae as *const c_void, TAE_UPDATE, TAE_UPDATE);
    // Wolf's HP raised (an AI-on General may hit him after he recovers).
    let wdata = module(&mut emu, w.player, 0x18);
    let _ = emu.peek::<8>(wdata + 0x130);
    emu.raw_write(wdata + 0x130, &5000i32.to_le_bytes());
    emu.raw_write(wdata + 0x134, &5000i32.to_le_bytes());
    // The General one point from his break.
    let gdata = module(&mut emu, g, 0x18);
    let _ = emu.peek::<8>(gdata + 0x148);
    emu.raw_write(gdata + 0x148, &1i32.to_le_bytes());
    // Wolf 2 m in front of the General facing him (forward = (-sin yaw, 0, -cos yaw)), or behind
    // him facing his back.
    let dp = acttrace::pos(&mut emu, g);
    let gphys = module(&mut emu, g, 0x68);
    let yaw = f32::from_le_bytes(emu.peek(gphys + 0x74));
    let s = if r.behind { -2.0 } else { 2.0 };
    let at = [dp[0] - s * yaw.sin(), dp[1], dp[2] - s * yaw.cos()];
    let wphys = module(&mut emu, w.player, 0x68);
    let (wrapper, _) = proxy::proxy_of(&mut emu, wphys);
    proxy::place(&mut emu, wphys, wrapper, at);
    emu.poke_f32s(wphys + 0x74, &[if r.behind { yaw } else { yaw + std::f32::consts::PI }]);
    let gm0 = module(&mut emu, g, 0);
    let _ = emu.peek_u64(gm0 + 0xaf0);
    emu.hook(HOOK_MEM_WRITE, on_end_write as *const c_void, gm0 + 0xaf0, gm0 + 0xaf7);
    emu.hook(uc::HOOK_CODE, on_env as *const c_void, ENV, ENV);
    let blk = emu.alloc(0x40);
    emu.poke_f32s(blk + 8, &[DT]);
    println!("brkwin {}: Wolf {:#x} General {:#x}", r.name, w.player, g);
    let mut frames = Vec::new();
    let mut stopped = None;
    let dt_bits = DT.to_bits() as u64;
    let idle = Input { arms: &[], stick: [0.0, 0.0] };
    let atk = Input { arms: &[ATTACK], stick: [0.0, 0.0] };
    for f in 0..r.frames {
        acttrace::press(&mut emu, w.stick, &ids, if f < 3 { &atk } else { &idle });
        proxy::flush_tb(&emu);
        ORDER.lock().unwrap().clear();
        let mut stop = emu.call(HIT_STEP, &[mgr, dt_bits], &[0.0, DT]).stopped;
        for task in TASKS_PRE {
            stop = stop.or(emu.call(task, &[wcm, blk], &[]).stopped);
        }
        ANIMS.lock().unwrap().clear();
        stop = stop.or(emu.call(CHR_FRAME, &[wcm, 0, g], &[0.0, DT]).stopped);
        let g_anims = std::mem::take(&mut *ANIMS.lock().unwrap());
        stop = stop.or(emu.call(CHR_FRAME, &[wcm, 0, w.player], &[0.0, DT]).stopped);
        ANIMS.lock().unwrap().clear();
        for task in TASKS_POST {
            stop = stop.or(emu.call(task, &[wcm, blk], &[]).stopped);
        }
        let mut gs = general_state(&mut emu, g);
        let clips: Vec<_> = g_anims.iter().filter(|c| !(790000..800000).contains(&c.0)).cloned().collect();
        gs["tae"] = json!(clips);
        gs["order"] = json!(std::mem::take(&mut *ORDER.lock().unwrap()));
        if f % 10 == 0 || f < 50 {
            println!(
                "  f{f} anim {} posture {}/{} throw {} {} end {:#x} tae {:?}",
                gs["anim"], gs["posture"], gs["posture_max"], gs["throw_7a"], gs["throw_7b"], gs["anim_end_bits"].as_u64().unwrap_or(0), clips.iter().map(|c| format!("{}@{:.3}", c.0, c.2)).collect::<Vec<_>>()
            );
        }
        frames.push(json!({"f": f, "general": gs}));
        if stop.is_some() {
            stopped = stop.map(|(a, e)| format!("{a:#x} error {e}"));
            println!("  stopped {stopped:?}");
            break;
        }
    }
    println!("brkwin {} ({} frames, {:.0} s)", r.name, frames.len(), t.elapsed().as_secs_f32());
    let v = json!({"name": r.name, "dt": DT, "ai": r.ai, "behind": r.behind, "frames": frames, "stopped": stopped});
    std::fs::write(out.join(format!("brkwin_{}.json", r.name)), serde_json::to_string(&v).unwrap()).unwrap();
}

/// `brkwin` runs every run; `brkwin_<name>` one.
pub fn run(new_emu: &dyn Fn() -> Emu, out: &Path, want: &[&str]) {
    let names: Vec<&str> = want.iter().filter_map(|w| w.strip_prefix("brkwin_")).collect();
    for r in RUNS.iter().filter(|r| names.is_empty() || names.contains(&r.name)) {
        run_one(new_emu, out, r);
    }
}
