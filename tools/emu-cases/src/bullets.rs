//! `bullets`: a bullet's flight and hit in whole world frames. Bullets move in MoveMapStep's task
//! FUN_140a74560 (task group 0x47, registered at MoveMapStep +0x270), which calls FUN_140a74ce0 ->
//! the BulletMan step FUN_140b1c050([0x143d84f70], FD4Time): the slots' update FUN_140b1edd0 ->
//! FUN_140b13770 per live slot (0x80 slots x 0xd30 at [BulletMan]+0; +8 handle, +0x10 position,
//! +0x4b4 BulletParam id, +0xb60 metres flown, +0xcb0 the fly state FUN_140b0ded0 runs on (+0xcd4
//! its age), +0xc6c flags bit 1 live), the other lists FUN_140b1f390 / FUN_140b1f4b0 /
//! FUN_140b1f020, then the counters FUN_140b1eee0. Group 0x47 runs before the hit manager's task
//! (FUN_140a746f0, group 0x4b) and the chr frames (FUN_140a70020, group 0x52).
//!
//! Each frame here: the bullet step, the hit manager step FUN_1409a0dc0, the WorldChrMan pre
//! tasks, the General's chr frame, Wolf's, the post tasks (hits.rs plus the bullet step in front).
//! The bullet is Wolf's: he uses a Pellet (goods 3020; items.rs) and at the spawn
//! FUN_140b1ad20(BulletMan, out, record) the record's BulletParam id (+0x10) and emit matrix (+0x40,
//! three rows x y z, the position in the 4th column) are rewritten in the emulator so the
//! shuriken's Bullet 700000 leaves his hand toward the General's chest or his side; the spawn
//! re-aims it at him (kb: in plan toward his root, at a fixed 6.3 deg rise).
//!
//! `bullets [bullets_<name>...]` writes extracted/emu/bullets_<name>.json: per frame each live
//! slot (position, metres flown, flags), the General's and Wolf's state (hits::chr_state), the hits
//! the resolve saw and in which phase of the frame, and per phase the watched functions entered.

use std::collections::BTreeMap;
use std::ffi::c_void;
use std::path::Path;
use std::sync::Mutex;

use serde_json::{json, Value};

use super::acttrace::{self, Input, CHR_FRAME, DT, TASKS_POST, TASKS_PRE, WORLD_CHR_MAN};
use super::hits::{self, AFTER_APPLY, HIT_MANAGER, HIT_SIZE, HIT_STEP, RESOLVE};
use super::{items, proxy, turn, uc, Emu};

const BULLET_MAN: u64 = 0x1_43d8_4f70;
/// BulletMan step (rcx BulletMan, rdx FD4Time).
const BULLET_STEP: u64 = 0x1_40b1_c050;
/// The spawn (rcx BulletMan, rdx out handle, r8 the spawn record).
const SPAWN: u64 = 0x1_40b1_ad20;
const SLOTS: u64 = 0x80;
const SLOT_SIZE: u64 = 0xd30;
const USE_ITEM: u32 = 7;
const TAE_UPDATE: u64 = 0x1_40b5_c920;

const WATCH: [(u64, &str); 10] = [
    (SPAWN, "spawn"),
    (0x1_40b1_3770, "slot_update"),
    (0x1_40b0_ded0, "fly"),
    (0x1_409a_1290, "attack_issue"),
    (0x1_4099_c530, "record_update"),
    (0x1_4099_b3f0, "shape_test"),
    (0x1_4099_cbe0, "hit_queue"),
    (0x1_4099_56d0, "deliver"),
    (RESOLVE, "resolve"),
    (0x1_40b6_e6a0, "apply"),
];

/// The rewrite at the spawn: (BulletParam id, target point) while armed; what was written.
struct Aim {
    armed: Option<(i32, [f32; 3])>,
    spawned: Vec<Value>,
}
static AIM: Mutex<Aim> = Mutex::new(Aim { armed: None, spawned: Vec::new() });

#[derive(Default)]
struct Seen {
    resolves: Vec<(u64, u64, Vec<u8>)>,
    applied: Vec<Vec<u8>>,
    anims: Vec<(i64, f32, f32)>,
}
static SEEN: Mutex<Seen> = Mutex::new(Seen { resolves: Vec::new(), applied: Vec::new(), anims: Vec::new() });

fn reg(uc: uc::Engine, id: i32) -> u64 {
    let mut v = 0u64;
    unsafe { uc::uc_reg_read(uc, id, &mut v as *mut u64 as _) };
    v
}

fn xmm(uc: uc::Engine, i: i32) -> f32 {
    let mut b = [0u8; 16];
    unsafe { uc::uc_reg_read(uc, uc::XMM0 + i, b.as_mut_ptr() as _) };
    f32::from_le_bytes(b[..4].try_into().unwrap())
}

fn rd_f32s(uc: uc::Engine, at: u64, n: usize) -> Vec<f32> {
    let mut b = vec![0u8; 4 * n];
    unsafe { uc::uc_mem_read(uc, at, b.as_mut_ptr() as _, 4 * n) };
    b.chunks(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect()
}

/// At the spawn's entry: the record's id and matrix rewritten (z axis = the aim direction).
extern "C" fn on_spawn(uc: uc::Engine, _addr: u64, _size: u32, _data: *mut c_void) {
    let rec = reg(uc, uc::R8);
    let mut aim = AIM.lock().unwrap();
    let Some((id, target)) = aim.armed.take() else { return };
    let before = rd_f32s(uc, rec + 0x40, 12);
    let hand = [before[3], before[7], before[11]];
    let d = [target[0] - hand[0], target[1] - hand[1], target[2] - hand[2]];
    let n = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    let z = [d[0] / n, d[1] / n, d[2] / n];
    // x = up x z, y = z x x.
    let xl = (z[2] * z[2] + z[0] * z[0]).sqrt();
    let x = [z[2] / xl, 0.0, -z[0] / xl];
    let y = [z[1] * x[2] - z[2] * x[1], z[2] * x[0] - z[0] * x[2], z[0] * x[1] - z[1] * x[0]];
    let m: Vec<f32> = (0..3).flat_map(|i| [x[i], y[i], z[i], hand[i]]).collect();
    let bytes: Vec<u8> = m.iter().flat_map(|v| v.to_le_bytes()).collect();
    unsafe {
        uc::uc_mem_write(uc, rec + 0x10, id.to_le_bytes().as_ptr() as _, 4);
        uc::uc_mem_write(uc, rec + 0x40, bytes.as_ptr() as _, bytes.len());
    }
    aim.spawned.push(json!({"bullet": id, "hand": hand, "dir": z, "matrix_before": before}));
}

extern "C" fn on_resolve(uc: uc::Engine, _addr: u64, _size: u32, _data: *mut c_void) {
    let mut b = vec![0u8; HIT_SIZE];
    unsafe { uc::uc_mem_read(uc, reg(uc, uc::R8), b.as_mut_ptr() as _, HIT_SIZE) };
    SEEN.lock().unwrap().resolves.push((reg(uc, uc::RCX), reg(uc, uc::RDX), b));
}

extern "C" fn on_after(uc: uc::Engine, _addr: u64, _size: u32, _data: *mut c_void) {
    let mut b = vec![0u8; HIT_SIZE];
    unsafe { uc::uc_mem_read(uc, reg(uc, uc::R8), b.as_mut_ptr() as _, HIT_SIZE) };
    SEEN.lock().unwrap().applied.push(b);
}

extern "C" fn on_tae(uc: uc::Engine, _addr: u64, _size: u32, _data: *mut c_void) {
    SEEN.lock().unwrap().anims.push((reg(uc, uc::RCX) as u32 as i32 as i64, xmm(uc, 1), xmm(uc, 2)));
}

fn f32at(emu: &mut Emu, at: u64) -> f32 {
    f32::from_le_bytes(emu.peek(at))
}

/// The live slots: index, handle, position, metres flown, flags.
fn slots(emu: &mut Emu, pool: u64) -> Vec<Value> {
    let mut out = Vec::new();
    for i in 0..SLOTS {
        let s = pool + i * SLOT_SIZE;
        let flags = emu.peek::<1>(s + 0xc6c)[0];
        if flags & 1 == 0 {
            continue;
        }
        let pos: Vec<f32> = (0..3).map(|k| f32at(emu, s + 0x10 + 4 * k)).collect();
        out.push(json!({
            "slot": i, "handle": i32::from_le_bytes(emu.peek(s + 8)), "bullet": i32::from_le_bytes(emu.peek(s + 0x4b4)),
            "pos": pos, "flown": f32at(emu, s + 0xb60), "age": f32at(emu, s + 0xcd4), "flags": flags,
        }));
    }
    out
}

struct Case {
    name: &'static str,
    /// Wolf's distance in front of the General.
    dist: f32,
    /// The aim point's offset from the General's root (x along the General's right, y up).
    aim: [f32; 2],
    bullet: i32,
    /// Frames after the spawn.
    after: usize,
}

const CASES: &[Case] = &[
    Case { name: "shuriken_4", dist: 4.0, aim: [0.0, 1.2], bullet: 700000, after: 30 },
    Case { name: "shuriken_8", dist: 8.0, aim: [0.0, 1.2], bullet: 700000, after: 30 },
    // Aimed 4 m to his side (34 deg off): the spawn still throws it at him.
    Case { name: "shuriken_aside", dist: 6.0, aim: [4.0, 1.2], bullet: 700000, after: 30 },
    // 12 m to his side (63 deg off, past lockShootLimitAng 45): also thrown at him, as `aside`.
    Case { name: "shuriken_wide", dist: 6.0, aim: [12.0, 1.2], bullet: 700000, after: 45 },
];

fn run_case(new_emu: &dyn Fn() -> Emu, out: &Path, c: &Case) {
    let t = std::time::Instant::now();
    let mut emu = new_emu();
    let w = turn::wolf(&mut emu);
    let ids = acttrace::arm_inputs(&mut emu);
    let g = acttrace::general(&mut emu);
    let wcm = emu.peek_u64(WORLD_CHR_MAN);
    let mgr = emu.peek_u64(HIT_MANAGER);
    let bm = emu.peek_u64(BULLET_MAN);
    let pool = emu.peek_u64(bm);
    // The Pellet as items.rs: the goods in an inventory entry, the quick item override.
    let item = 0x4000_0000u32 | 3020;
    let inv = items::inventory(&mut emu, w.player);
    let entry = inv.iter().find(|e| e.1 == item).copied().or_else(|| inv.iter().find(|e| e.1 >> 28 == 4 && e.1 != 0x4000_0bb8 && e.1 & 0xfff_ffff >= 3000).copied());
    let (_, _, _, at) = entry.expect("no goods entry");
    emu.raw_write(at + 4, &item.to_le_bytes());
    emu.raw_write(at + 8, &3i32.to_le_bytes());
    let pgd = emu.peek_u64(w.player + 0x2000);
    emu.raw_write(pgd + 0x278 + 0x358, &item.to_le_bytes());
    // As hits.rs: ISA level 1, the plain longjmp, no menu; the General updated every frame, AI off.
    emu.limit = 500_000_000;
    emu.raw_write(0x1_43c1_d520, &1u32.to_le_bytes());
    emu.raw_write(proxy::LONGJMP, proxy::PLAIN_LONGJMP);
    emu.raw_write(acttrace::MENU_STATE, &[0x31, 0xc0, 0xc3]);
    let menus = emu.call(0x1_40d8_72c0, &[], &[]).rax;
    if menus != 0 {
        emu.u64(menus + 0x100, 0);
    }
    acttrace::every_frame(&mut emu, g);
    emu.hook(uc::HOOK_CODE, on_spawn as *const c_void, SPAWN, SPAWN);
    emu.hook(uc::HOOK_CODE, on_resolve as *const c_void, RESOLVE, RESOLVE);
    emu.hook(uc::HOOK_CODE, on_after as *const c_void, AFTER_APPLY, AFTER_APPLY);
    emu.hook(uc::HOOK_CODE, on_tae as *const c_void, TAE_UPDATE, TAE_UPDATE);
    emu.st.starts = WATCH.iter().map(|w| w.0).collect();
    // The General stays where the snapshot has him (his hurt shapes are not moved by the proxy, as
    // hits.rs); Wolf `dist` m in front of him, facing him.
    let gp = acttrace::pos(&mut emu, g);
    let gphys = hits::module(&mut emu, g, 0x68);
    let gyaw = f32at(&mut emu, gphys + 0x74);
    let (fx, fz) = (-gyaw.sin(), -gyaw.cos());
    let wpos = [gp[0] + c.dist * fx, gp[1], gp[2] + c.dist * fz];
    let wphys = hits::module(&mut emu, w.player, 0x68);
    let (wrapper, _) = proxy::proxy_of(&mut emu, wphys);
    proxy::place(&mut emu, wphys, wrapper, wpos);
    emu.poke_f32s(wphys + 0x74, &[gyaw + std::f32::consts::PI]);
    // The aim: the General's root raised aim[1], moved aim[0] along his right (-fz, fx).
    let target = [gp[0] + c.aim[0] * -fz, gp[1] + c.aim[1], gp[2] + c.aim[0] * fx];
    *AIM.lock().unwrap() = Aim { armed: Some((c.bullet, target)), spawned: Vec::new() };
    let time = turn::fd4_time(&mut emu, DT);
    let blk = emu.alloc(0x40);
    emu.poke_f32s(blk + 8, &[DT]);
    let (wdmod, gdmod) = (hits::module(&mut emu, w.player, 0x98), hits::module(&mut emu, g, 0x98));
    println!("bullets {}: Wolf {:#x} at {wpos:?}, General {g:#x} at {gp:?} yaw {gyaw:.3}, BulletMan {bm:#x} pool {pool:#x}", c.name, w.player);
    let dt_bits = DT.to_bits() as u64;
    let mut frames = Vec::new();
    let mut spawn_at: Option<usize> = None;
    let mut stopped = None;
    for f in 0..200 {
        let held = if f < 3 { Input { arms: &[USE_ITEM], stick: [0.0, 0.0] } } else { Input { arms: &[], stick: [0.0, 0.0] } };
        acttrace::press(&mut emu, w.stick, &ids, &held);
        proxy::flush_tb(&emu);
        *SEEN.lock().unwrap() = Seen::default();
        let mut phases: Vec<(&str, BTreeMap<&str, u64>, usize)> = Vec::new();
        let mut stop = None;
        let mut phase = |emu: &mut Emu, name: &'static str, stop: &mut Option<(u64, i32)>, run: &mut dyn FnMut(&mut Emu) -> Option<(u64, i32)>| {
            emu.st.entered.clear();
            let before = SEEN.lock().unwrap().resolves.len();
            if stop.is_none() {
                *stop = run(emu);
            }
            let counts: BTreeMap<&str, u64> = WATCH.iter().filter_map(|(a, n)| emu.st.entered.get(a).map(|&k| (*n, k))).collect();
            let hits_here = SEEN.lock().unwrap().resolves.len() - before;
            phases.push((name, counts, hits_here));
        };
        phase(&mut emu, "bullets", &mut stop, &mut |e| e.call(BULLET_STEP, &[bm, time], &[]).stopped);
        let moved = slots(&mut emu, pool);
        phase(&mut emu, "hit_step", &mut stop, &mut |e| e.call(HIT_STEP, &[mgr, dt_bits], &[0.0, DT]).stopped);
        phase(&mut emu, "pre_tasks", &mut stop, &mut |e| {
            let mut s = None;
            for task in TASKS_PRE {
                s = s.or(e.call(task, &[wcm, blk], &[]).stopped);
            }
            s
        });
        SEEN.lock().unwrap().anims.clear();
        phase(&mut emu, "general", &mut stop, &mut |e| e.call(CHR_FRAME, &[wcm, 0, g], &[0.0, DT]).stopped);
        let g_anims = std::mem::take(&mut SEEN.lock().unwrap().anims);
        phase(&mut emu, "wolf", &mut stop, &mut |e| e.call(CHR_FRAME, &[wcm, 0, w.player], &[0.0, DT]).stopped);
        let w_anims = std::mem::take(&mut SEEN.lock().unwrap().anims);
        phase(&mut emu, "post_tasks", &mut stop, &mut |e| {
            let mut s = None;
            for task in TASKS_POST {
                s = s.or(e.call(task, &[wcm, blk], &[]).stopped);
            }
            s
        });
        let seen = std::mem::take(&mut *SEEN.lock().unwrap());
        // Which phase each resolve fell in.
        let mut owner = Vec::new();
        for (name, _, n) in &phases {
            owner.extend(std::iter::repeat(*name).take(*n));
        }
        let mut hits_v = Vec::new();
        for (k, (dmod, att, before)) in seen.resolves.iter().enumerate() {
            let after = seen.applied.get(k).cloned().unwrap_or_default();
            let mut h = if after.len() == HIT_SIZE { hits::hit_fields(&after) } else { json!({}) };
            h["in"] = hits::hit_fields(before);
            h["phase"] = json!(owner.get(k).copied().unwrap_or("?"));
            h["defender"] = json!(if *dmod == wdmod { "wolf" } else if *dmod == gdmod { "general" } else { "other" });
            h["attacker"] = json!(if *att == w.player { "wolf" } else if *att == g { "general" } else { "other" });
            hits_v.push(h);
        }
        let spawned = std::mem::take(&mut AIM.lock().unwrap().spawned);
        if spawn_at.is_none() && !spawned.is_empty() {
            spawn_at = Some(f);
        }
        let live = slots(&mut emu, pool);
        let (mut ws, mut gs) = (hits::chr_state(&mut emu, w.player), hits::chr_state(&mut emu, g));
        ws["tae"] = json!(w_anims);
        gs["tae"] = json!(g_anims);
        let ph: Vec<Value> = phases.iter().map(|(n, cts, k)| json!({"phase": n, "counts": cts, "hits": k})).collect();
        if spawn_at.is_some() || !hits_v.is_empty() {
            let clips = |a: &[(i64, f32, f32)]| a.iter().filter(|c| !(790000..800000).contains(&c.0)).map(|c| format!("{}@{:.3}", c.0, c.2)).collect::<Vec<_>>();
            println!("  f{f} slots {} moved {} | general hp {} posture {} {:?} | {}", serde_json::to_string(&live).unwrap(), serde_json::to_string(&moved).unwrap(), gs["hp"], gs["posture"], clips(&g_anims), serde_json::to_string(&ph).unwrap());
            for h in &hits_v {
                println!("    hit {} -> {} in {}: hp {} posture {} guarded {}", h["attacker"], h["defender"], h["phase"], h["hp_dmg"], h["posture_dmg"], h["guarded"]);
            }
            if !spawned.is_empty() {
                println!("    spawned {}", serde_json::to_string(&spawned).unwrap());
            }
        }
        frames.push(json!({"f": f, "slots": live, "slots_moved": moved, "wolf": ws, "general": gs, "hits": hits_v, "phases": ph, "spawned": spawned}));
        if stop.is_some() {
            stopped = stop.map(|(a, e)| format!("{a:#x} error {e}"));
            println!("  stopped {stopped:?} trail {:x?}", emu.st.trail);
            break;
        }
        if spawn_at.is_some_and(|s| f >= s + c.after) {
            break;
        }
    }
    println!("bullets {} ({} frames, spawn {spawn_at:?}, {:.0} s)", c.name, frames.len(), t.elapsed().as_secs_f32());
    let v = json!({
        "name": c.name, "dt": DT, "dist": c.dist, "aim": c.aim, "bullet": c.bullet, "target": target,
        "general_pos": gp, "general_yaw": gyaw, "wolf_pos": wpos, "spawn_frame": spawn_at,
        "frames": frames, "stopped": stopped,
    });
    std::fs::write(out.join(format!("bullets_{}.json", c.name)), serde_json::to_string(&v).unwrap()).unwrap();
}

/// `bullets` runs every case; `bullets_<name>` one.
pub fn run(new_emu: &dyn Fn() -> Emu, out: &Path, want: &[&str]) {
    let names: Vec<&str> = want.iter().filter_map(|w| w.strip_prefix("bullets_")).collect();
    for c in CASES {
        if names.is_empty() || names.contains(&c.name) {
            run_case(new_emu, out, c);
        }
    }
}
