//! Hit stop on the snapshot (topic modticks): SprjChrHitStopModule ([chr+0x1ff8]+0x90), its start
//! FUN_140b9dad0 and its tick FUN_140b9d1b0, then whole character frames FUN_140a4b570 with a hit
//! stop running, to see which clock the ChrIns pre-update FUN_1409f78a0 hands the rest of the frame
//! (chr+0x108) and what step the Havok behaviour (the animation) takes.
//!
//! `modticks` writes extracted/emu/modticks_hitstop.json (module ticks) and modticks_frames.json.

use std::ffi::c_void;
use std::path::Path;
use std::sync::Mutex;

use serde_json::{json, Value};

use super::{uc, Emu};

const WORLD_CHR_MAN: u64 = 0x1_43d7_a1e0;
const ENEMY_VT: u64 = 0x1_42a2_7f28;
/// SprjChrHitStopModule start (rcx module, xmm1 seconds): +0x14 = seconds, +0x10 = the ease-in time
/// WorldChrManDbg +0x12c, +0x20 = 1.
const HIT_STOP_START: u64 = 0x1_40b9_dad0;
/// SprjChrHitStopModule tick (rcx module, xmm1 dt): +0x18 = the scaled dt.
const HIT_STOP_TICK: u64 = 0x1_40b9_d1b0;
/// One character's frame (rcx WorldChrMan, xmm1 dt, r8 chr).
const CHR_FRAME: u64 = 0x1_40a4_b570;
/// Hands the behaviour character its step (rcx the behaviour, rdx FD4Time; [rdx+8] = dt * speed).
const BEH_STEP: u64 = 0x1_40ff_f000;

static STEPS: Mutex<Vec<f32>> = Mutex::new(Vec::new());

extern "C" fn on_beh_step(uc: uc::Engine, _addr: u64, _size: u32, _data: *mut c_void) {
    let mut rdx = 0u64;
    let mut v = [0f32; 1];
    unsafe {
        uc::uc_reg_read(uc, uc::RDX, &mut rdx as *mut u64 as _);
        uc::uc_mem_read(uc, rdx + 8, v.as_mut_ptr() as _, 4);
    }
    STEPS.lock().unwrap().push(v[0]);
}

fn f32_at(emu: &mut Emu, at: u64) -> f32 {
    f32::from_le_bytes(emu.peek(at))
}

fn module(emu: &mut Emu, chr: u64) -> u64 {
    let bag = emu.peek_u64(chr + 0x1ff8);
    emu.peek_u64(bag + 0x90)
}

/// (state +0x20, timer +0x10, length +0x14, scaled dt +0x18)
fn state(emu: &mut Emu, m: u64) -> Value {
    json!({"state": emu.peek::<1>(m + 0x20)[0], "timer": f32_at(emu, m + 0x10), "length": f32_at(emu, m + 0x14), "scaled": f32_at(emu, m + 0x18)})
}

fn try_u64(emu: &mut Emu, at: u64) -> Option<u64> {
    if at < 0x10000 || at > 0x7fff_ffff_ffff || at % 8 != 0 {
        return None;
    }
    let mut b = [0u8; 8];
    if unsafe { uc::uc_mem_read(emu.uc, at, b.as_mut_ptr() as _, 8) } == 0 {
        return Some(u64::from_le_bytes(b));
    }
    let (from, bytes) = emu.st.snap.as_mut()?.window(at)?;
    emu.map(from, bytes.len() as u64);
    emu.raw_write(from, &bytes);
    Some(u64::from_le_bytes(emu.read(at)))
}

/// The General (NpcParam 1020... as guard.rs finds him: chr +0x6c == 1020).
fn general(emu: &mut Emu) -> u64 {
    let wcm = emu.peek_u64(WORLD_CHR_MAN);
    for off in (0..0x2000).step_by(8) {
        let Some(p) = try_u64(emu, wcm + off) else { continue };
        if try_u64(emu, p).is_none() {
            continue;
        }
        for o2 in (0..0x4000).step_by(8) {
            if let Some(q) = try_u64(emu, p + o2) {
                if try_u64(emu, q) == Some(ENEMY_VT) && u32::from_le_bytes(emu.peek(q + 0x6c)) == 1020 {
                    return q;
                }
            }
        }
    }
    panic!("no General in the snapshot");
}

/// A hit stop scenario: started with `length` before the first tick, restarted with `restart`
/// (frame, seconds) before that frame's tick.
struct Case {
    length: f32,
    restart: Option<(usize, f32)>,
}

const LENGTHS: [f32; 7] = [0.03, 0.05, 0.1, 0.13, 0.2, 0.3, 0.5];

fn unit_cases() -> Vec<Case> {
    let mut v: Vec<Case> = LENGTHS.iter().map(|&length| Case { length, restart: None }).collect();
    for (f, s) in [(1, 0.1), (3, 0.2), (6, 0.1), (9, 0.05)] {
        v.push(Case { length: 0.2, restart: Some((f, s)) });
    }
    v
}

pub fn run(new_emu: &dyn Fn() -> Emu, out: &Path, want: &[&str]) {
    if want.contains(&"modticks") || want.contains(&"modticks_unit") {
        unit(new_emu, out);
    }
    if want.contains(&"modticks") || want.contains(&"modticks_frames") {
        frames(new_emu, out);
    }
    if want.contains(&"modticks") || want.contains(&"modticks_defender") {
        defender(new_emu, out);
    }
}

/// The defender's start FUN_140b9cfb0 (rcx the defender's module, rdx the hit's five amounts,
/// r8 the attack: [+0x10] +0x1cc = AtkParam hitStopTime_Defencer) on Wolf and the General as they
/// are in the snapshot (mode +0x2d 0), with FUN_140b9d8c0 (the defender's hitStopType_Defencer:
/// NpcParam +0x125, a player 1) read alongside.
fn defender(new_emu: &dyn Fn() -> Emu, out: &Path) {
    const DEF_START: u64 = 0x1_40b9_cfb0;
    const DEF_TYPE: u64 = 0x1_40b9_d8c0;
    let mut cases = Vec::new();
    for who in ["wolf", "general"] {
        for amount in [0.0f32, 0.1] {
            for time in [0.0f32, 0.05, 0.1, 0.2] {
                let mut emu = new_emu();
                let chr = match who {
                    "wolf" => emu.st.snap.as_ref().expect("modticks needs --snapshot").player,
                    _ => general(&mut emu),
                };
                let m = module(&mut emu, chr);
                let ty = emu.call(DEF_TYPE, &[m], &[]);
                assert!(ty.stopped.is_none(), "type stopped {:?}", ty.stopped);
                let amounts = emu.alloc(0x80);
                emu.poke_f32s(amounts, &[amount, 0.0, 0.0, 0.0, 0.0]);
                let attack = emu.alloc(0x40);
                let atk = emu.alloc(0x200);
                emu.u64(attack + 0x10, atk);
                emu.f32(atk + 0x1cc, time);
                // amounts +0x50 / +0x54: the attack's param ids FUN_140b60ec0 looks up (isArrowAtk); none.
                emu.u32(amounts + 0x50, u32::MAX);
                emu.u32(amounts + 0x54, u32::MAX);
                let r = emu.call(DEF_START, &[m, amounts, attack], &[]);
                assert!(r.stopped.is_none(), "{who} defender start stopped {:?}", r.stopped);
                let s = state(&mut emu, m);
                println!("defender {who} amount {amount} time {time}: type {} started {} {s}", ty.rax & 0xff, r.rax & 0xff);
                cases.push(json!({"who": who, "amount": amount, "time": time, "def_type": ty.rax & 0xff, "started": r.rax & 0xff == 1,
                    "forced": emu.peek::<1>(m + 0x31)[0] == 1, "mode": emu.peek::<1>(m + 0x2d)[0], "module": s}));
            }
        }
    }
    let v = json!({"functions": ["0x140b9cfb0", "0x140b9d8c0"], "cases": cases});
    std::fs::write(out.join("modticks_defender.json"), serde_json::to_string_pretty(&v).unwrap()).unwrap();
}

/// The module alone on Wolf's module: start, then ticks at dt 1/60 and 1/30.
fn unit(new_emu: &dyn Fn() -> Emu, out: &Path) {
    let mut cases = Vec::new();
    for dt in [1.0f32 / 60.0, 1.0 / 30.0] {
        for c in unit_cases() {
            let mut emu = new_emu();
            let player = emu.st.snap.as_ref().expect("modticks needs --snapshot").player;
            let m = module(&mut emu, player);
            let before = state(&mut emu, m);
            let r = emu.call(HIT_STOP_START, &[m], &[0.0, c.length]);
            assert!(r.stopped.is_none(), "start stopped {:?}", r.stopped);
            let mut frames = Vec::new();
            for f in 0..60 {
                if let Some((rf, s)) = c.restart {
                    if rf == f {
                        emu.call(HIT_STOP_START, &[m], &[0.0, s]);
                    }
                }
                let r = emu.call(HIT_STOP_TICK, &[m], &[0.0, dt]);
                assert!(r.stopped.is_none(), "tick stopped {:?}", r.stopped);
                let s = state(&mut emu, m);
                let idle = s["state"] == 0;
                frames.push(s);
                if idle && c.restart.map_or(true, |(rf, _)| f > rf) {
                    break;
                }
            }
            println!("hit stop {} dt {dt:.4} restart {:?}: {} frames", c.length, c.restart, frames.len());
            cases.push(json!({"dt": dt, "length": c.length, "restart": c.restart.map(|(f, s)| json!([f, s])), "before": before, "frames": frames}));
        }
    }
    let mut emu = new_emu();
    let dbg = emu.peek_u64(0x1_43d7_a388);
    let consts = json!({"ease_in_12c": f32_at(&mut emu, dbg + 0x12c), "hold_130": f32_at(&mut emu, dbg + 0x130), "ease_out_134": f32_at(&mut emu, dbg + 0x134), "switch_11d": emu.peek::<1>(dbg + 0x11d)[0]});
    println!("WorldChrManDbg {consts}");
    let v = json!({"functions": ["0x140b9dad0", "0x140b9d1b0"], "dbg": consts, "cases": cases});
    std::fs::write(out.join("modticks_hitstop.json"), serde_json::to_string_pretty(&v).unwrap()).unwrap();
}

/// Whole character frames with a hit stop: what the pre-update leaves in chr+0x108 and the step
/// the behaviour (animation) gets, for Wolf and the General at dt 1/60 and 1/30.
fn frames(new_emu: &dyn Fn() -> Emu, out: &Path) {
    let mut scen = Vec::new();
    for who in ["wolf", "general"] {
        for dt in [1.0f32 / 60.0, 1.0 / 30.0] {
            for length in [0.0f32, 0.1] {
                let t = std::time::Instant::now();
                let mut emu = new_emu();
                emu.limit = 500_000_000;
                let chr = match who {
                    "wolf" => emu.st.snap.as_ref().expect("modticks needs --snapshot").player,
                    _ => general(&mut emu),
                };
                let wcm = emu.peek_u64(WORLD_CHR_MAN);
                let m = module(&mut emu, chr);
                if length > 0.0 {
                    emu.call(HIT_STOP_START, &[m], &[0.0, length]);
                }
                emu.hook(uc::HOOK_CODE, on_beh_step as *const c_void, BEH_STEP, BEH_STEP);
                let mut frames = Vec::new();
                let n = if length > 0.0 { (0.06 + length) / dt + 4.0 } else { 3.0 } as usize;
                for _ in 0..n {
                    STEPS.lock().unwrap().clear();
                    let r = emu.call(CHR_FRAME, &[wcm, 0, chr], &[0.0, dt]);
                    let steps = STEPS.lock().unwrap().clone();
                    let s = state(&mut emu, m);
                    frames.push(json!({"chr_dt": f32_at(&mut emu, chr + 0x108), "module": s, "beh_steps": steps,
                        "stopped": r.stopped.map(|(a, e)| format!("{a:#x} error {e}"))}));
                }
                println!("frames {who} dt {dt:.4} hit stop {length}: {} frames, {:.1} s", frames.len(), t.elapsed().as_secs_f32());
                scen.push(json!({"who": who, "dt": dt, "length": length, "frames": frames}));
            }
        }
    }
    let v = json!({"functions": ["0x140a4b570", "0x1409f78a0", "0x140b9d1b0", "0x140fff000"], "scenarios": scen});
    std::fs::write(out.join("modticks_frames.json"), serde_json::to_string_pretty(&v).unwrap()).unwrap();
}
