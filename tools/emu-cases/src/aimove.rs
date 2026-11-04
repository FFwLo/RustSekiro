//! `aimove`: how the General's Lua goals become his moves, turns and actions (topic aimove).
//! Whole world frames as acttrace's duel (the pre-update tasks, the General's chr frame
//! FUN_140a4b570, Wolf's, the post-update tasks), the AI on (b143d7a376 = 0), the General updated
//! every frame (chr +0x110 = 0), Wolf put at a distance from him (`push_ledge*`: he is put next to
//! Wolf, whose body the broadphase still holds). Per frame: the targeting state,
//! AiThink's move point +0x73d0 / move type +0x7410 / target angles +0x73e0 / attack request
//! +0x7430 (read at the manipulator's copy FUN_1407e6570), the manipulator's output M +0x380 (move
//! vector), M +0x20 (turn), his anim, position and yaw, and every AddSubGoal (FUN_1405ce510:
//! goal id, life, params) the goals made.
//!
//! `aimove_sched` runs the search scheduler FUN_140614be0 (DAT_143d547d8 +0x28) on chosen ring
//! contents. Output: extracted/emu/aimove_frames_<scenario>.json, aimove_sched.json.

use std::ffi::c_void;
use std::path::Path;
use std::sync::Mutex;

use serde_json::{json, Value};

use super::aicpp::{ai_chain, f32s, physics, pos, try_u64, WORLD_CHR_MAN};
use super::{proxy, turn, uc, Emu};

pub(super) const CHR_FRAME: u64 = 0x1_40a4_b570;
const ENEMY_VT: u64 = 0x1_42a2_7f28;
const MENU_STATE: u64 = 0x1_408c_58e0;
/// AddSubGoal's core (goal, id, life, params 0-4 (dwords), floats 5..): FUN_1405ce510.
pub(super) const ADD_SUB_GOAL: u64 = 0x1_405c_e510;
/// The manipulator's request copy (AI +0x7430 -> ActionRequest +0x170), after the think.
const REQUEST_COPY: u64 = 0x1_407e_6570;
/// The move mode of the current goal (FUN_1406040b0(ai)): 4 = along the path (FUN_1409eeb40).
const MOVE_MODE: u64 = 0x1_4060_40b0;
/// The search interval scheduler: (ring = DAT_143d547d8 + 0x28, seconds) -> seconds.
const SCHED: u64 = 0x1_4061_4be0;
const SCHED_GLOBAL: u64 = 0x1_43d5_47d8;
pub(super) const TASKS_PRE: [u64; 2] = [0x1_40a5_09f0, 0x1_40a4_db20];
pub(super) const TASKS_POST: [u64; 2] = [0x1_40a5_0880, 0x1_40a4_88b0];
const DT: f32 = 1.0 / 30.0;

/// (frame, goal id, life bits, params raw) of each AddSubGoal.
static GOALS: Mutex<Vec<(usize, i32, u32, Vec<u32>)>> = Mutex::new(Vec::new());
static FRAME: Mutex<usize> = Mutex::new(0);
/// (AI object, the request it held at the copy this frame).
static REQ: Mutex<(u64, i32)> = Mutex::new((0, -1));

fn rd(uc: uc::Engine, at: u64) -> u64 {
    let mut b = [0u8; 8];
    unsafe { uc::uc_mem_read(uc, at, b.as_mut_ptr() as _, 8) };
    u64::from_le_bytes(b)
}

extern "C" fn on_goal(uc: uc::Engine, _addr: u64, _size: u32, _data: *mut c_void) {
    let (mut rdx, mut r9, mut rsp) = (0u64, 0u64, 0u64);
    // The life is a float in xmm2 (r8 is never read): the low lane of the 16-byte register.
    let mut xmm2 = [0u8; 16];
    unsafe {
        uc::uc_reg_read(uc, uc::RDX, &mut rdx as *mut u64 as _);
        uc::uc_reg_read(uc, uc::XMM0 + 2, xmm2.as_mut_ptr() as _);
        uc::uc_reg_read(uc, uc::R9, &mut r9 as *mut u64 as _);
        uc::uc_reg_read(uc, uc::RSP, &mut rsp as *mut u64 as _);
    }
    // param_4 in r9, param_5.. on the stack from rsp + 0x28 (8-byte slots).
    let mut p = vec![r9 as u32];
    for i in 0..14 {
        p.push(rd(uc, rsp + 0x28 + 8 * i) as u32);
    }
    let f = *FRAME.lock().unwrap();
    GOALS.lock().unwrap().push((f, rdx as u32 as i32, u32::from_le_bytes(xmm2[..4].try_into().unwrap()), p));
}

extern "C" fn on_copy(uc: uc::Engine, _addr: u64, _size: u32, _data: *mut c_void) {
    let mut g = REQ.lock().unwrap();
    if g.0 != 0 {
        g.1 = rd(uc, g.0 + 0x7430) as u32 as i32;
    }
}

/// The General (chr +0x6c == 1020).
pub(super) fn general(emu: &mut Emu) -> u64 {
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

pub(super) fn anim(emu: &mut Emu, chr: u64) -> i64 {
    let bag = emu.peek_u64(chr + 0x1ff8);
    let m = emu.peek_u64(bag + 0x10);
    i32::from_le_bytes(emu.peek(m + 0xf8)) as i64
}

pub(super) fn i32_at(emu: &mut Emu, at: u64) -> i32 {
    i32::from_le_bytes(emu.peek(at))
}

/// One scenario: the General `dist` m from Wolf at bearing `bearing` deg (0 = in front of Wolf),
/// turned `face` deg from facing Wolf; Wolf's stick (forward, side) per frame; the attack request
/// forced for the first 3 frames when `force` is set.
struct Scn {
    name: &'static str,
    dist: f32,
    bearing: f32,
    face: f32,
    frames: usize,
    force: Option<i32>,
    stick: fn(usize) -> [f32; 2],
}

const STILL: fn(usize) -> [f32; 2] = |_| [0.0, 0.0];

const SCNS: [Scn; 11] = [
    Scn { name: "d3", dist: 3.0, bearing: 0.0, face: 0.0, frames: 240, force: None, stick: STILL },
    Scn { name: "d6", dist: 6.0, bearing: 0.0, face: 0.0, frames: 240, force: None, stick: STILL },
    Scn { name: "d10", dist: 10.0, bearing: 0.0, face: 0.0, frames: 240, force: None, stick: STILL },
    Scn { name: "d16", dist: 16.0, bearing: 0.0, face: 0.0, frames: 240, force: None, stick: STILL },
    Scn { name: "d6_side", dist: 6.0, bearing: 0.0, face: 90.0, frames: 180, force: None, stick: STILL },
    Scn { name: "d6_back", dist: 6.0, bearing: 0.0, face: 150.0, frames: 180, force: None, stick: STILL },
    // Wolf walks away from the General (stick back): the General follows.
    Scn { name: "d4_wolf_leaves", dist: 4.0, bearing: 0.0, face: 0.0, frames: 240, force: None, stick: |f| if f > 45 { [-1.0, 0.0] } else { [0.0, 0.0] } },
    // Forced attack 3000 from 1.6 m: his lunge into Wolf (the push).
    Scn { name: "push_attack", dist: 1.6, bearing: 0.0, face: 0.0, frames: 90, force: Some(3000), stick: STILL },
    Scn { name: "push_attack_far", dist: 2.6, bearing: 0.0, face: 0.0, frames: 90, force: Some(3000), stick: STILL },
    // The same on Wolf's ledge (he stays where the broadphase has him).
    Scn { name: "push_ledge", dist: 1.6, bearing: 0.0, face: 0.0, frames: 75, force: Some(3000), stick: STILL },
    Scn { name: "push_ledge_far", dist: 2.6, bearing: 0.0, face: 0.0, frames: 75, force: Some(3000), stick: STILL },
];

/// hknpCharacterProxy::integrate (FUN_1418f8b60) calls this frame: (proxy rcx, its velocity +0xb0).
static INTEG: Mutex<Vec<(u64, [f32; 4])>> = Mutex::new(Vec::new());

extern "C" fn on_integ(uc: uc::Engine, _addr: u64, _size: u32, _data: *mut c_void) {
    let mut rcx = 0u64;
    unsafe { uc::uc_reg_read(uc, uc::RCX, &mut rcx as *mut u64 as _) };
    let mut v = [0f32; 4];
    for (i, x) in v.iter_mut().enumerate() {
        *x = f32::from_bits(rd(uc, rcx + 0xb0 + 4 * i as u64) as u32);
    }
    INTEG.lock().unwrap().push((rcx, v));
}

static FORCE: Mutex<(u64, i32)> = Mutex::new((0, -1));

extern "C" fn on_force(uc: uc::Engine, _addr: u64, _size: u32, _data: *mut c_void) {
    let (ai, v) = *FORCE.lock().unwrap();
    if ai != 0 && v >= 0 {
        unsafe { uc::uc_mem_write(uc, ai + 0x7430, v.to_le_bytes().as_ptr() as _, 4) };
    }
}

pub(super) fn setup(emu: &mut Emu) {
    emu.limit = 500_000_000;
    emu.raw_write(0x1_43c1_d520, &1u32.to_le_bytes());
    emu.raw_write(proxy::LONGJMP, proxy::PLAIN_LONGJMP);
    emu.raw_write(MENU_STATE, &[0x31, 0xc0, 0xc3]);
    let menus = emu.call(0x1_40d8_72c0, &[], &[]).rax;
    if menus != 0 {
        emu.u64(menus + 0x100, 0);
    }
}

fn scenario(new_emu: &dyn Fn() -> Emu, s: &Scn) -> Value {
    let t = std::time::Instant::now();
    let mut emu = new_emu();
    let w = turn::wolf(&mut emu);
    let g = general(&mut emu);
    let (m, ai, ts) = ai_chain(&mut emu, g).unwrap();
    let wcm = emu.peek_u64(WORLD_CHR_MAN);
    emu.raw_write(0x1_43d7_a376, &[0]);
    let _ = emu.peek::<4>(g + 0x110);
    emu.raw_write(g + 0x110, &0u32.to_le_bytes());
    setup(&mut emu);
    // Pages the hooks read (raw uc reads) mapped first.
    let _ = emu.peek::<4>(ai + 0x7430);
    emu.hook(uc::HOOK_CODE, on_goal as *const c_void, ADD_SUB_GOAL, ADD_SUB_GOAL);
    emu.hook(uc::HOOK_CODE, on_copy as *const c_void, REQUEST_COPY, REQUEST_COPY);
    emu.hook(uc::HOOK_CODE, on_integ as *const c_void, proxy::INTEGRATE, proxy::INTEGRATE);
    if s.force.is_some() {
        emu.hook(uc::HOOK_CODE, on_force as *const c_void, REQUEST_COPY, REQUEST_COPY);
    }
    GOALS.lock().unwrap().clear();
    *REQ.lock().unwrap() = (ai, -1);
    // Wolf placed `dist` m from the General at his snapshot spot (his own floor; Wolf's
    // snapshot spot is on a ledge 10 m over the arena, and the General put there fell off it),
    // `bearing` deg from his facing; the General turned to face Wolf, then `face` deg off.
    // Forward = (-sin yaw, 0, -cos yaw).
    let gphys = physics(&mut emu, g);
    if s.name.starts_with("push_ledge") {
        // The other way round (acttrace's duel): Wolf stays at his snapshot spot and the General is
        // put `dist` m in front of him, facing him. The proxies' bodies are in the broadphase where
        // the snapshot left them (the Havok world step is not run here), so only a body that stays
        // put (Wolf) can be hit by the other's proxy cast; Wolf moved next to the General (the
        // other scenarios) is not in the broadphase there and the General passed through him.
        let wp = pos(&mut emu, w.player);
        let wyaw = f32::from_le_bytes(emu.peek(w.phys + 0x74));
        let at = [wp[0] - s.dist * wyaw.sin(), wp[1], wp[2] - s.dist * wyaw.cos()];
        let (gwrap, _) = proxy::proxy_of(&mut emu, gphys);
        proxy::place(&mut emu, gphys, gwrap, at);
        emu.poke_f32s(gphys + 0x74, &[wyaw + std::f32::consts::PI + s.face.to_radians()]);
    } else {
        let gp0 = pos(&mut emu, g);
        let gyaw0 = f32::from_le_bytes(emu.peek(gphys + 0x74));
        let b = gyaw0 + s.bearing.to_radians();
        let at = [gp0[0] - s.dist * b.sin(), gp0[1], gp0[2] - s.dist * b.cos()];
        let (wwrap, _) = proxy::proxy_of(&mut emu, w.phys);
        proxy::place(&mut emu, w.phys, wwrap, at);
        emu.poke_f32s(w.phys + 0x74, &[b + std::f32::consts::PI]);
        emu.poke_f32s(gphys + 0x74, &[b + s.face.to_radians()]);
    }
    let (_, gproxy) = proxy::proxy_of(&mut emu, gphys);
    let (_, wproxy) = proxy::proxy_of(&mut emu, w.phys);
    let blk = emu.alloc(0x40);
    emu.poke_f32s(blk + 8, &[DT]);
    let think = u32::from_le_bytes(emu.peek(ai + 0x28));
    let mut frames = Vec::new();
    let mut stopped = None;
    for f in 0..s.frames {
        *FRAME.lock().unwrap() = f;
        *FORCE.lock().unwrap() = (ai, match s.force { Some(v) if f < 3 => v, _ => -1 });
        REQ.lock().unwrap().1 = -1;
        INTEG.lock().unwrap().clear();
        emu.poke_f32s(w.stick, &(s.stick)(f));
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
        let gp = pos(&mut emu, g);
        let gy = f32s::<1>(&mut emu, gphys + 0x74)[0];
        let wpn = pos(&mut emu, w.player);
        let wy = f32s::<1>(&mut emu, w.phys + 0x74)[0];
        let mode = emu.call(MOVE_MODE, &[ai], &[]).rax & 0xff;
        let rec = json!({
            "g": gp, "gyaw": gy, "ganim": anim(&mut emu, g), "w": wpn, "wyaw": wy, "wanim": anim(&mut emu, w.player),
            "state": i32_at(&mut emu, ts + 0x158), "kind": i32_at(&mut emu, ts + 0x160),
            "movept": f32s::<4>(&mut emu, ai + 0x73d0), "movetype": i32_at(&mut emu, ai + 0x7410),
            "ang": f32s::<2>(&mut emu, ai + 0x73e0), "req": REQ.lock().unwrap().1, "guard_req": i32_at(&mut emu, ai + 0x7414),
            "mv": f32s::<4>(&mut emu, m + 0x380), "turn": f32s::<4>(&mut emu, m + 0x20), "m60": f32s::<8>(&mut emu, m + 0x60),
            "mode": mode,
            // Physics module +0xb10 (proxy mode: 1 idle, no step; 3 moved) and the integrate calls
            // on each one's proxy (velocity +0xb0) this frame.
            "gphys_mode": i32_at(&mut emu, gphys + 0xb10), "wphys_mode": i32_at(&mut emu, w.phys + 0xb10),
            "ginteg": INTEG.lock().unwrap().iter().filter(|c| c.0 == gproxy).map(|c| c.1).collect::<Vec<_>>(),
            "winteg": INTEG.lock().unwrap().iter().filter(|c| c.0 == wproxy).map(|c| c.1).collect::<Vec<_>>(),
            "gproxy": f32s::<4>(&mut emu, gproxy + 0x80),
        });
        if f % 30 == 0 {
            let d = ((gp[0] - wpn[0]).powi(2) + (gp[2] - wpn[2]).powi(2)).sqrt();
            println!("  {} f{f} anim {} state {} type {} mv {:?} d {d:.2} goals {} ({:.0} s)", s.name, rec["ganim"], rec["state"], rec["movetype"], rec["mv"], GOALS.lock().unwrap().len(), t.elapsed().as_secs_f32());
        }
        frames.push(rec);
        if stop.is_some() {
            stopped = stop.map(|(a, e)| format!("{a:#x} error {e}"));
            println!("  {} stopped at f{f}: {stopped:?}", s.name);
            break;
        }
    }
    let goals: Vec<Value> = GOALS.lock().unwrap().iter().map(|(f, id, life, p)| json!({"f": f, "id": id, "life": f32::from_bits(*life), "p": p})).collect();
    println!("{}: {} frames, {} goals, {:.0} s", s.name, frames.len(), goals.len(), t.elapsed().as_secs_f32());
    json!({"name": s.name, "think": think, "dist": s.dist, "face": s.face, "force": s.force, "dt": DT,
        "frames": frames, "goals": goals, "stopped": stopped})
}

/// The search scheduler (FUN_140614be0) on rings with chosen contents: the snapshot's own ring,
/// its cap (+0x68 of the ring = global +0x90) and flag (+0x69), then rows of (cursor, counts,
/// seconds) -> seconds.
fn sched(new_emu: &dyn Fn() -> Emu, out: &Path) {
    let mut emu = new_emu();
    let g = emu.peek_u64(SCHED_GLOBAL);
    let ring = g + 0x28;
    let snap_counts: [u8; 100] = emu.peek(ring);
    let cap = emu.peek::<2>(ring + 0x68);
    let cur = i32_at(&mut emu, ring + 0x64);
    let hdr: [f32; 2] = f32s(&mut emu, g + 0x20);
    println!("sched global {g:#x}: cursor {cur}, cap {cap:?}, +0x20 {hdr:?}, counts {snap_counts:?}");
    let mut cases = Vec::new();
    let mut seed = 7u32;
    for i in 0..400 {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        let fill = match i % 4 {
            0 => 0u32,
            1 => 2,
            2 => 3,
            _ => 4,
        };
        let mut counts = [0u8; 100];
        for (k, c) in counts.iter_mut().enumerate() {
            let h = (seed.wrapping_mul(2654435761).wrapping_add(k as u32 * 40503)) >> 7;
            *c = if fill == 0 { 0 } else { (h % (fill + 1)) as u8 };
        }
        // Runs of full buckets ahead so the walk forward is exercised.
        if i % 3 == 0 {
            for k in 0..(seed % 12) as usize {
                counts[(k + (seed as usize >> 8) % 100) % 100] = 3;
            }
        }
        let cursor = (seed >> 3) % 100;
        let secs = 0.2 + (seed % 1000) as f32 / 1000.0 * 0.2;
        let nolimit = (i % 25 == 24) as u8;
        emu.raw_write(ring, &counts);
        emu.raw_write(ring + 0x64, &cursor.to_le_bytes());
        emu.raw_write(ring + 0x68, &[3, nolimit]);
        let r = emu.call(SCHED, &[ring, secs.to_bits() as u64], &[0.0, secs]);
        assert!(r.stopped.is_none());
        let after: [u8; 100] = emu.peek(ring);
        let changed: Vec<usize> = (0..100).filter(|&k| after[k] != counts[k]).collect();
        cases.push(json!({"counts": counts.to_vec(), "cursor": cursor, "cap": 3, "nolimit": nolimit, "secs": secs,
            "out": f32::from_bits(r.rax as u32), "xmm0": r.xmm0, "bumped": changed}));
    }
    let body = json!({"function": format!("{SCHED:#x}"), "snapshot": {"cursor": cur, "cap": cap, "counts": snap_counts.to_vec(), "period": hdr}, "cases": cases});
    std::fs::write(out.join("aimove_sched.json"), serde_json::to_string(&body).unwrap()).unwrap();
    println!("aimove_sched: {} cases", 400);
}

pub(super) fn run(new_emu: &dyn Fn() -> Emu, out: &Path, want: &[&str]) {
    if want.contains(&"aimove_sched") || want.contains(&"aimove") {
        sched(new_emu, out);
    }
    let names: Vec<&str> = want.iter().filter_map(|w| w.strip_prefix("aimove_f_")).collect();
    if want.contains(&"aimove") || want.contains(&"aimove_frames") || !names.is_empty() {
        // One file per scenario (aimove_frames_<name>.json); a run takes about 2.5 s per frame,
        // so they are best run as separate processes.
        for s in &SCNS {
            if names.is_empty() || names.contains(&s.name) {
                let body = json!({"frame": format!("{CHR_FRAME:#x}"), "scenarios": [scenario(new_emu, s)]});
                std::fs::write(out.join(format!("aimove_frames_{}.json", s.name)), serde_json::to_string(&body).unwrap()).unwrap();
            }
        }
    }
}
