//! `aiint`: the interrupts the General's Lua AI reads and his sound target (topic aiint, row E11).
//! Whole world frames as aimove (pre-update tasks, the General's chr frame, Wolf's, post-update
//! tasks; AI on, the General updated every frame). Per frame: the interrupt mask the goal tree is
//! offered (hook at FUN_1405ada60: AI +0x40 goal tree, rdx = AI +0x68 mask, gathered by
//! FUN_14060c460 from the targeting flags FUN_14061f0a0, the LoseSightTarget bit of
//! FUN_1405b48e0 and the rest), the targeting state / kind, the target list (kind 3 = the sound
//! target: its slot bytes), the targeting list's "sound changed" flag (ts +0x20), the AI sound
//! records (manager [0x143d55070] +0x30 areas of 0xe0, area +0xb8 record list, 0x70 per record)
//! and every AddSubGoal.
//!
//! Sounds are raised as FUN_1407577b0 does (TAE CreateAISound): source {1, Wolf's handle, -1}
//! (FUN_14060f1b0, FUN_14060f1d0), AiSoundParam id (FUN_14060f270), FUN_14060ef20, then
//! FUN_14064a5f0(manager, pos, source, extra, id) at a chosen spot near the General.
//! Output: extracted/emu/aiint_<scenario>.json.

use std::ffi::c_void;
use std::path::Path;
use std::sync::Mutex;

use serde_json::{json, Value};

use super::aicpp::{ai_chain, f32s, physics, pos, WORLD_CHR_MAN};
use super::aimove::{anim, general, i32_at, setup, ADD_SUB_GOAL, CHR_FRAME, TASKS_POST, TASKS_PRE};
use super::{proxy, turn, uc, Emu};

/// The goal tree's interrupt dispatch (goal tree AI +0x40, mask pointer).
const DISPATCH: u64 = 0x1_405a_da60;
const SOUND_MGR: u64 = 0x1_43d5_5070;
const SRC_HANDLE: u64 = 0x1_4060_f1b0;
const SRC_DESC: u64 = 0x1_4060_f1d0;
const PARAM_REF: u64 = 0x1_4060_f270;
const EXTRA: u64 = 0x1_4060_ef20;
const EMIT: u64 = 0x1_4064_a5f0;
/// One area's record list aging: (list, dt in xmm1).
const AGE: u64 = 0x1_4061_1a00;
const DT: f32 = 1.0 / 30.0;

static MASKS: Mutex<Vec<(usize, u64)>> = Mutex::new(Vec::new());
static GOALS: Mutex<Vec<(usize, i32, f32, Vec<u32>)>> = Mutex::new(Vec::new());
static FRAME: Mutex<usize> = Mutex::new(0);

fn rd(uc: uc::Engine, at: u64) -> u64 {
    let mut b = [0u8; 8];
    unsafe { uc::uc_mem_read(uc, at, b.as_mut_ptr() as _, 8) };
    u64::from_le_bytes(b)
}

extern "C" fn on_dispatch(uc: uc::Engine, _addr: u64, _size: u32, _data: *mut c_void) {
    let mut rdx = 0u64;
    unsafe { uc::uc_reg_read(uc, uc::RDX, &mut rdx as *mut u64 as _) };
    let f = *FRAME.lock().unwrap();
    MASKS.lock().unwrap().push((f, rd(uc, rdx)));
}

extern "C" fn on_goal(uc: uc::Engine, _addr: u64, _size: u32, _data: *mut c_void) {
    let (mut rdx, mut r9, mut rsp) = (0u64, 0u64, 0u64);
    let mut xmm2 = [0u8; 16];
    unsafe {
        uc::uc_reg_read(uc, uc::RDX, &mut rdx as *mut u64 as _);
        uc::uc_reg_read(uc, uc::XMM0 + 2, xmm2.as_mut_ptr() as _);
        uc::uc_reg_read(uc, uc::R9, &mut r9 as *mut u64 as _);
        uc::uc_reg_read(uc, uc::RSP, &mut rsp as *mut u64 as _);
    }
    let mut p = vec![r9 as u32];
    for i in 0..6 {
        p.push(rd(uc, rsp + 0x28 + 8 * i) as u32);
    }
    let f = *FRAME.lock().unwrap();
    GOALS.lock().unwrap().push((f, rdx as u32 as i32, f32::from_le_bytes(xmm2[..4].try_into().unwrap()), p));
}

/// The target list: (kind, node address) per node.
fn nodes(emu: &mut Emu, ts: u64) -> Vec<(u32, u64)> {
    let mut out = Vec::new();
    let mut n = emu.peek_u64(ts + 0x18);
    while n != 0 && out.len() < 16 {
        let vt = emu.peek_u64(n);
        let kf = emu.peek_u64(vt + 0x10);
        let kind = emu.call(kf, &[n], &[]).rax as u32;
        out.push((kind, n));
        n = emu.peek_u64(n + 0x10);
    }
    out
}

/// Every AI sound record: (area, serial +0x48, rank +0x3c, id +0x40, pos +0x10, radius +0x2c,
/// +0x30, +0x34 bytes, raw u32 words 0..0x70).
fn records(emu: &mut Emu) -> Vec<Value> {
    let mgr = emu.peek_u64(SOUND_MGR);
    let n = u32::from_le_bytes(emu.peek(mgr + 0x28));
    let arr = emu.peek_u64(mgr + 0x30);
    let mut out = Vec::new();
    for a in 0..n.min(64) as u64 {
        let list = emu.peek_u64(arr + a * 0xe0 + 0xb8);
        if list == 0 {
            continue;
        }
        let (b, e) = (emu.peek_u64(list + 8), emu.peek_u64(list + 0x10));
        let mut r = b;
        while r < e && out.len() < 64 {
            let w: Vec<u32> = (0..0x1c).map(|i| u32::from_le_bytes(emu.peek(r + 4 * i))).collect();
            out.push(json!({"area": a, "serial": w[0x12] & 0xffff, "rank": w[0xf] as i32, "id": w[0x10] as i32,
                "pos": f32s::<3>(emu, r + 0x10), "radius": f32::from_bits(w[0xb]), "w": w}));
            r += 0x70;
        }
    }
    out
}

/// Raises AiSoundParam `id` at `at` with Wolf (`chr`) as its source, as FUN_1407577b0 does.
fn emit(emu: &mut Emu, chr: u64, id: i32, at: [f32; 3]) {
    let buf = emu.alloc(0x100);
    let (src, desc, p, x, posv) = (buf, buf + 0x10, buf + 0x40, buf + 0x50, buf + 0x60);
    assert!(emu.call(SRC_HANDLE, &[src, chr, 0xffff_ffff], &[]).stopped.is_none());
    assert!(emu.call(SRC_DESC, &[desc, src], &[]).stopped.is_none());
    assert!(emu.call(PARAM_REF, &[p, id as u32 as u64], &[]).stopped.is_none());
    assert!(emu.call(EXTRA, &[x], &[]).stopped.is_none());
    emu.poke_f32s(posv, &[at[0], at[1], at[2], 1.0]);
    let mgr = emu.peek_u64(SOUND_MGR);
    let r = emu.call(EMIT, &[mgr, posv, desc, x, p], &[]);
    assert!(r.stopped.is_none(), "emit stopped {:?}", r.stopped);
}

struct Scn {
    name: &'static str,
    frames: usize,
    /// Wolf `dist` m in front of the General (None: left on his ledge, unseen).
    wolf: Option<f32>,
    /// (frame, AiSoundParam id, metres in front of the General).
    sounds: &'static [(usize, i32, f32)],
    /// (frame, metres, bearing deg from the General's start facing): Wolf moved there.
    moves: &'static [(usize, f32, f32)],
}

const SCNS: [Scn; 3] = [
    // Rank 0 2140 (6 m, 1 s) at 2 m, refreshed at f15, again at f75 after it ran out; rank 1 2100
    // (2 m) at f95; rank 0 at f100 under it; rank 1 2110 (4 m: a second record) at f110; rank 0
    // 1000 (0.5 m) 5 m off at f120 (out of reach).
    Scn { name: "snd", frames: 150, wolf: None, sounds: &[(5, 2140, 2.0), (15, 2140, 2.0), (75, 2140, 2.0), (95, 2100, 1.0), (100, 2140, 2.0), (110, 2110, 1.5), (120, 1000, 5.0)], moves: &[] },
    // Two rank 0 records in one frame (2140 at 2 m, 2900 at 1 m), then 2900 alone refreshed.
    Scn { name: "snd_two", frames: 60, wolf: None, sounds: &[(5, 2140, 2.0), (5, 2900, 1.0), (20, 2900, 1.0), (40, 2140, 2.0)], moves: &[] },
    // Wolf 6 m in front (BATTLE), moved 6 m behind him at f60 (out of sight), back in front at f110.
    Scn { name: "lose", frames: 160, wolf: Some(6.0), sounds: &[], moves: &[(60, 6.0, 180.0), (110, 6.0, 0.0)] },
];

fn scenario(new_emu: &dyn Fn() -> Emu, s: &Scn) -> Value {
    let t = std::time::Instant::now();
    let mut emu = new_emu();
    let w = turn::wolf(&mut emu);
    let g = general(&mut emu);
    let (_m, ai, ts) = ai_chain(&mut emu, g).unwrap();
    let wcm = emu.peek_u64(WORLD_CHR_MAN);
    emu.raw_write(0x1_43d7_a376, &[0]);
    let _ = emu.peek::<4>(g + 0x110);
    emu.raw_write(g + 0x110, &0u32.to_le_bytes());
    setup(&mut emu);
    let _ = emu.peek::<8>(ai + 0x68);
    emu.hook(uc::HOOK_CODE, on_dispatch as *const c_void, DISPATCH, DISPATCH);
    emu.hook(uc::HOOK_CODE, on_goal as *const c_void, ADD_SUB_GOAL, ADD_SUB_GOAL);
    MASKS.lock().unwrap().clear();
    GOALS.lock().unwrap().clear();
    let gphys = physics(&mut emu, g);
    let gp0 = pos(&mut emu, g);
    let gyaw0 = f32::from_le_bytes(emu.peek(gphys + 0x74));
    let fwd = |d: f32, bearing: f32| {
        let b = gyaw0 + bearing.to_radians();
        [gp0[0] - d * b.sin(), gp0[1], gp0[2] - d * b.cos()]
    };
    let put_wolf = |emu: &mut Emu, d: f32, bearing: f32| {
        let at = fwd(d, bearing);
        let (wwrap, _) = proxy::proxy_of(emu, w.phys);
        proxy::place(emu, w.phys, wwrap, at);
        emu.poke_f32s(w.phys + 0x74, &[gyaw0 + bearing.to_radians() + std::f32::consts::PI]);
    };
    if let Some(d) = s.wolf {
        put_wolf(&mut emu, d, 0.0);
    }
    let blk = emu.alloc(0x40);
    emu.poke_f32s(blk + 8, &[DT]);
    let think = u32::from_le_bytes(emu.peek(ai + 0x28));
    let mut frames = Vec::new();
    let mut stopped = None;
    for f in 0..s.frames {
        *FRAME.lock().unwrap() = f;
        for &(_, d, b) in s.moves.iter().filter(|m| m.0 == f) {
            put_wolf(&mut emu, d, b);
        }
        // Sounds go in before the frame (as a chr raised them in the previous frame's update).
        let gp = pos(&mut emu, g);
        let gy = f32s::<1>(&mut emu, gphys + 0x74)[0];
        for &(_, id, d) in s.sounds.iter().filter(|x| x.0 == f) {
            let at = [gp[0] - d * gy.sin(), gp[1], gp[2] - d * gy.cos()];
            emit(&mut emu, w.player, id, at);
        }
        proxy::flush_tb(&emu);
        let mut stop = None;
        for task in TASKS_PRE {
            stop = stop.or(emu.call(task, &[wcm, blk], &[]).stopped);
        }
        stop = stop.or(emu.call(CHR_FRAME, &[wcm, 0, g], &[0.0, DT]).stopped);
        stop = stop.or(emu.call(CHR_FRAME, &[wcm, 0, w.player], &[0.0, DT]).stopped);
        for task in TASKS_POST {
            stop = stop.or(emu.call(task, &[wcm, blk], &[]).stopped);
        }
        // The records age in each area's AI update (FUN_140652020 -> FUN_140611a00(list, dt):
        // life +0x30 - dt, gone at <= 0), which these tasks do not run: run it after the frame.
        let mgr = emu.peek_u64(SOUND_MGR);
        let (n, arr) = (u32::from_le_bytes(emu.peek(mgr + 0x28)), emu.peek_u64(mgr + 0x30));
        for a in 0..n.min(64) as u64 {
            let list = emu.peek_u64(arr + a * 0xe0 + 0xb8);
            if list != 0 {
                stop = stop.or(emu.call(AGE, &[list], &[0.0, DT]).stopped);
            }
        }
        let gp = pos(&mut emu, g);
        let wp = pos(&mut emu, w.player);
        let list: Vec<Value> = nodes(&mut emu, ts)
            .into_iter()
            .map(|(k, n)| {
                let words: Vec<u32> = (0..0x30).map(|i| u32::from_le_bytes(emu.peek(n + 4 * i))).collect();
                json!({"kind": k, "w": words})
            })
            .collect();
        let masks: Vec<u64> = MASKS.lock().unwrap().iter().filter(|m| m.0 == f).map(|m| m.1).collect();
        let rec = json!({
            "g": gp, "gyaw": f32s::<1>(&mut emu, gphys + 0x74)[0], "ganim": anim(&mut emu, g), "w": wp,
            "state": i32_at(&mut emu, ts + 0x158), "kind": i32_at(&mut emu, ts + 0x160),
            "flags": i32_at(&mut emu, ts + 0x150), "snd_changed": emu.peek::<1>(ts + 0x20)[0],
            "masks": masks, "last_mask": emu.peek_u64(ai + 0xb730), "list": list, "records": records(&mut emu),
        });
        if f % 15 == 0 || !masks.is_empty() && masks.iter().any(|m| *m & !(1u64 << 43 | 1 << 44) != 0) {
            println!("  {} f{f} anim {} state {} kind {} masks {:x?} records {} ({:.0} s)", s.name, rec["ganim"], rec["state"], rec["kind"], masks, rec["records"].as_array().unwrap().len(), t.elapsed().as_secs_f32());
        }
        frames.push(rec);
        if stop.is_some() {
            stopped = stop.map(|(a, e)| format!("{a:#x} error {e}"));
            println!("  {} stopped at f{f}: {stopped:?}", s.name);
            break;
        }
    }
    let goals: Vec<Value> = GOALS.lock().unwrap().iter().map(|(f, id, life, p)| json!({"f": f, "id": id, "life": life, "p": p})).collect();
    println!("{}: {} frames, {} goals, {:.0} s", s.name, frames.len(), goals.len(), t.elapsed().as_secs_f32());
    json!({"name": s.name, "think": think, "dt": DT, "sounds": s.sounds, "moves": s.moves, "wolf": s.wolf,
        "frames": frames, "goals": goals, "stopped": stopped})
}

pub(super) fn run(new_emu: &dyn Fn() -> Emu, out: &Path, want: &[&str]) {
    let names: Vec<&str> = want.iter().filter_map(|w| w.strip_prefix("aiint_")).collect();
    for s in &SCNS {
        if names.is_empty() || names.contains(&s.name) {
            let body = json!({"frame": format!("{CHR_FRAME:#x}"), "scenarios": [scenario(new_emu, s)]});
            std::fs::write(out.join(format!("aiint_{}.json", s.name)), serde_json::to_string(&body).unwrap()).unwrap();
        }
    }
}
