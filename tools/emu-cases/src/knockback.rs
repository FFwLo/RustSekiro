//! `knockback`: who moves a character after a guard break, a block, a hit, a deflect and a
//! posture break, frame by frame (row E13). The duels are hits.rs's (the hit manager step
//! FUN_1409a0dc0, the WorldChrMan pre tasks, the General's chr frame FUN_140a4b570, Wolf's, the
//! post tasks), with Wolf's slash on the General standing (AI off; guarding = ShieldBlock forced)
//! and the General's slash 3000 on Wolf. Each frame records both chrs' position and yaw, the
//! knockback module ([chr+0x1ff8]+0xa8: +0x10 distance, +0x14 hold, +0x18 speed, +0x1c decay,
//! +0x20 direction, +0x30, +0x40 TAE 226 percent, +0x44 / +0x45 flags), the ChrCtrl +0x90
//! controller ([chr+0x50]+0x90, vtable 0x142a8f850: +0x20 / +0x30 the move / turn target, +0x40 /
//! +0x44 time left, +0x48 / +0x4c the totals, +0x50 mode, +0x54 on, +0x55 the push, +0x60 its
//! velocity, +0x74 its yaw rate, +0x80 / +0x84 its length / clock) and the physics +0x860 push
//! velocity; the hooks log the knockback starts FUN_140ba37e0 / FUN_140ba3190 / FUN_140ba3420,
//! the controller's setter FUN_140be6690 and every write into the controller from outside its
//! own tick FUN_140be6750.
//!
//! `knockback_ctl_*` drives the controller itself on the General standing (AI off): its fields
//! written as the setter FUN_140be6690 writes them (mode 0 a delta, 1 a target point, 2 a facing)
//! or the +0x55 push, then whole chr frames.
//!
//! Writes extracted/emu/knockback_<name>.json. Results: docs/kb/damage.md "Guard-break knockback".

use std::collections::BTreeMap;
use std::ffi::c_void;
use std::path::Path;
use std::sync::Mutex;

use serde_json::{json, Value};

use super::acttrace::{self, Input, ATTACK, CHR_FRAME, DT, GUARD, TASKS_POST, TASKS_PRE, WORLD_CHR_MAN};
use super::{proxy, turn, uc, Emu};

const HIT_MANAGER: u64 = 0x1_43d7_7ef0;
const HIT_STEP: u64 = 0x1_409a_0dc0;
const RESOLVE: u64 = 0x1_40b6_c880;
const AFTER_APPLY: u64 = 0x1_40b6_e5a0;
const TAE_UPDATE: u64 = 0x1_40b5_c920;
const SP_ADD: u64 = 0x1_409e_9c00;
const GENERAL_DEFLECT: u64 = 200220;
/// Knockback module starts: FUN_140ba37e0 (rcx module, rdx direction, xmm2 distance, r9 hold,
/// [rsp+0x28] decay, [rsp+0x30] byte), FUN_140ba3190 (rcx, edx KnockBackParam field or -1, xmm2,
/// r9, [rsp+0x28]), FUN_140ba3420 (rcx, rdx direction, xmm2 distance).
const KB_START: u64 = 0x1_40ba_37e0;
const KB_TIMES: u64 = 0x1_40ba_3190;
const KB_DIR: u64 = 0x1_40ba_3420;
/// The ChrCtrl +0x90 controller's setter and tick.
const CTL_SET: u64 = 0x1_40be_6690;
const CTL_TICK: u64 = 0x1_40be_6750;
const CTL_TICK_END: u64 = 0x1_40be_694e;
const HOOK_MEM_WRITE: i32 = 2048;
const HIT_SIZE: usize = 0x250;

#[derive(Default)]
struct Seen {
    hits: Vec<(u64, u64, Vec<u8>)>,
    applied: Vec<Vec<u8>>,
    anims: Vec<(i64, f32, f32)>,
    events: Vec<Value>,
    /// The controllers watched for writes: (address, owner name).
    ctls: Vec<(u64, &'static str)>,
    /// Knockback modules: (address, owner name).
    kbs: Vec<(u64, &'static str)>,
}
static SEEN: Mutex<Seen> = Mutex::new(Seen { hits: Vec::new(), applied: Vec::new(), anims: Vec::new(), events: Vec::new(), ctls: Vec::new(), kbs: Vec::new() });

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
fn mem<const N: usize>(uc: uc::Engine, at: u64) -> [u8; N] {
    let mut b = [0u8; N];
    unsafe { uc::uc_mem_read(uc, at, b.as_mut_ptr() as _, N) };
    b
}
fn mf32(uc: uc::Engine, at: u64) -> f32 {
    f32::from_le_bytes(mem(uc, at))
}
fn read_hit(uc: uc::Engine, at: u64) -> Vec<u8> {
    let mut b = vec![0u8; HIT_SIZE];
    unsafe { uc::uc_mem_read(uc, at, b.as_mut_ptr() as _, HIT_SIZE) };
    b
}
fn who(list: &[(u64, &'static str)], at: u64) -> &'static str {
    list.iter().find(|(a, _)| *a == at).map_or("other", |x| x.1)
}

extern "C" fn on_resolve(uc: uc::Engine, _a: u64, _s: u32, _d: *mut c_void) {
    let hit = read_hit(uc, reg(uc, uc::R8));
    SEEN.lock().unwrap().hits.push((reg(uc, uc::RCX), reg(uc, uc::RDX), hit));
}
extern "C" fn on_after(uc: uc::Engine, _a: u64, _s: u32, _d: *mut c_void) {
    let hit = read_hit(uc, reg(uc, uc::R8));
    SEEN.lock().unwrap().applied.push(hit);
}
extern "C" fn on_tae(uc: uc::Engine, _a: u64, _s: u32, _d: *mut c_void) {
    SEEN.lock().unwrap().anims.push((reg(uc, uc::RCX) as u32 as i32 as i64, xmm(uc, 1), xmm(uc, 2)));
}
extern "C" fn on_kb_start(uc: uc::Engine, _a: u64, _s: u32, _d: *mut c_void) {
    let rsp = reg(uc, uc::RSP);
    let dir = reg(uc, uc::RDX);
    let ret = u64::from_le_bytes(mem(uc, rsp));
    let mut s = SEEN.lock().unwrap();
    let chr = who(&s.kbs, reg(uc, uc::RCX));
    let e = json!({"at": "kb_start", "chr": chr, "dir": [mf32(uc, dir), mf32(uc, dir + 4), mf32(uc, dir + 8)], "dist": xmm(uc, 2),
        "hold": f32::from_bits(reg(uc, uc::R9) as u32), "decay": mf32(uc, rsp + 0x28), "flag": mem::<1>(uc, rsp + 0x30)[0], "ret": format!("{ret:#x}")});
    s.events.push(e);
}
extern "C" fn on_kb_times(uc: uc::Engine, _a: u64, _s: u32, _d: *mut c_void) {
    let rsp = reg(uc, uc::RSP);
    let ret = u64::from_le_bytes(mem(uc, rsp));
    let mut s = SEEN.lock().unwrap();
    let chr = who(&s.kbs, reg(uc, uc::RCX));
    let e = json!({"at": "kb_times", "chr": chr, "field": reg(uc, uc::RDX) as u32 as i32, "x2": xmm(uc, 2),
        "r9": f32::from_bits(reg(uc, uc::R9) as u32), "a5": mf32(uc, rsp + 0x28), "ret": format!("{ret:#x}")});
    s.events.push(e);
}
extern "C" fn on_kb_dir(uc: uc::Engine, _a: u64, _s: u32, _d: *mut c_void) {
    let dir = reg(uc, uc::RDX);
    let rsp = reg(uc, uc::RSP);
    let ret = u64::from_le_bytes(mem(uc, rsp));
    let mut s = SEEN.lock().unwrap();
    let chr = who(&s.kbs, reg(uc, uc::RCX));
    let e = json!({"at": "kb_dir", "chr": chr, "dir": [mf32(uc, dir), mf32(uc, dir + 8)], "dist": xmm(uc, 2), "ret": format!("{ret:#x}")});
    s.events.push(e);
}
extern "C" fn on_ctl_set(uc: uc::Engine, _a: u64, _s: u32, _d: *mut c_void) {
    let rsp = reg(uc, uc::RSP);
    let ret = u64::from_le_bytes(mem(uc, rsp));
    let iface = reg(uc, uc::RDX);
    let vt = u64::from_le_bytes(mem(uc, iface));
    let mut s = SEEN.lock().unwrap();
    let chr = who(&s.ctls, reg(uc, uc::RCX));
    let e = json!({"at": "ctl_set", "chr": chr, "iface_vtable": format!("{vt:#x}"), "ret": format!("{ret:#x}")});
    s.events.push(e);
}
extern "C" fn on_ctl_write(uc: uc::Engine, _kind: i32, addr: u64, size: i32, value: i64, _d: *mut c_void) {
    let rip = reg(uc, uc::RIP);
    if (CTL_TICK..CTL_TICK_END).contains(&rip) {
        return;
    }
    let mut s = SEEN.lock().unwrap();
    let Some(&(base, chr)) = s.ctls.iter().find(|(a, _)| (*a..*a + 0x90).contains(&addr)) else { return };
    let e = json!({"at": "ctl_write", "chr": chr, "off": format!("{:#x}", addr - base), "size": size, "value": value, "f": f32::from_bits(value as u32), "rip": format!("{rip:#x}")});
    s.events.push(e);
}

fn f32at(emu: &mut Emu, at: u64) -> f32 {
    f32::from_le_bytes(emu.peek(at))
}
fn i32at(emu: &mut Emu, at: u64) -> i32 {
    i32::from_le_bytes(emu.peek(at))
}
fn module(emu: &mut Emu, chr: u64, off: u64) -> u64 {
    let bag = emu.peek_u64(chr + 0x1ff8);
    emu.peek_u64(bag + off)
}
fn floats(emu: &mut Emu, at: u64, from: u64, to: u64) -> Vec<f32> {
    (from..to).step_by(4).map(|o| f32at(emu, at + o)).collect()
}
fn ctl(emu: &mut Emu, chr: u64) -> u64 {
    emu.peek_u64(chr + 0x50) + 0x90
}

/// One chr after a frame: HP, posture, position, yaw, the knockback module, the controller.
fn chr_state(emu: &mut Emu, chr: u64) -> Value {
    let data = module(emu, chr, 0x18);
    let ph = module(emu, chr, 0x68);
    let kb = module(emu, chr, 0xa8);
    let c = ctl(emu, chr);
    let kbf = emu.peek::<2>(kb + 0x44);
    let cf = emu.peek::<2>(c + 0x54);
    json!({
        "hp": i32at(emu, data + 0x130), "posture": i32at(emu, data + 0x148),
        "pos": acttrace::pos(emu, chr), "yaw": f32at(emu, ph + 0x74),
        "push_vel": floats(emu, ph, 0x860, 0x870),
        "kb": floats(emu, kb, 0x10, 0x44), "kb_flags": kbf,
        "ctl": floats(emu, c, 0x20, 0x50), "ctl_mode": i32at(emu, c + 0x50), "ctl_flags": cf,
        "ctl_push": floats(emu, c, 0x60, 0x88),
    })
}

fn hit_fields(b: &[u8]) -> Value {
    let f = |o: usize| f32::from_le_bytes(b[o..o + 4].try_into().unwrap());
    let i = |o: usize| i32::from_le_bytes(b[o..o + 4].try_into().unwrap());
    json!({
        "guarded": b[0x164], "deflected": b[0x1c7], "guard_break": b[0x1c4], "broken": b[0x1c6],
        "knockback": f(0x68), "hp_dmg": i(0x1e0), "posture_dmg": i(0x1e4), "dir": [f(0x150), f(0x158)],
    })
}

#[derive(Clone, Copy)]
enum Pad {
    Idle,
    Guard,
    Attack(usize),
}

#[derive(Clone, Copy)]
enum Foe {
    Attack(i32),
    Stand { guard: bool, deflect_at: Option<usize> },
}

struct Duel {
    name: &'static str,
    frames: usize,
    pad: Pad,
    foe: Foe,
    dist: f32,
    wolf_posture: Option<i32>,
    general_posture: Option<i32>,
}

fn duels() -> Vec<Duel> {
    let stand = |guard| Foe::Stand { guard, deflect_at: None };
    let d = |name, frames, pad, foe, dist, wolf_posture, general_posture| Duel { name, frames, pad, foe, dist, wolf_posture, general_posture };
    vec![
        // Wolf breaks the General's guard: guarding with 5 posture left.
        d("w_break", 90, Pad::Attack(0), stand(true), 2.0, None, Some(5)),
        d("w_guard", 70, Pad::Attack(0), stand(true), 2.0, None, None),
        d("w_open", 70, Pad::Attack(0), stand(false), 2.0, None, None),
        d("w_broken", 90, Pad::Attack(0), stand(false), 2.0, None, Some(5)),
        d("w_deflect", 70, Pad::Attack(0), Foe::Stand { guard: true, deflect_at: Some(26) }, 2.0, None, None),
        // The General breaks Wolf's guard (hits_g_break) and his open hit.
        d("g_break", 90, Pad::Guard, Foe::Attack(3000), 2.5, Some(40), None),
        d("g_open", 75, Pad::Idle, Foe::Attack(3000), 2.5, None, None),
    ]
}

fn pad(p: Pad, f: usize) -> Input {
    const NONE: &[u32] = &[];
    const G: &[u32] = &[GUARD];
    const A: &[u32] = &[ATTACK];
    let arms = match p {
        Pad::Guard => G,
        Pad::Attack(at) if (at..at + 3).contains(&f) => A,
        _ => NONE,
    };
    Input { arms, stick: [0.0, 0.0] }
}

/// The run's common start (as hits.rs): CRT ISA level 1, the plain longjmp, no menu open,
/// the General updated every frame, the hooks in.
fn start(new_emu: &dyn Fn() -> Emu) -> (Emu, turn::Wolf, u64) {
    let mut emu = new_emu();
    let w = turn::wolf(&mut emu);
    let g = acttrace::general(&mut emu);
    emu.limit = 500_000_000;
    emu.raw_write(0x1_43c1_d520, &1u32.to_le_bytes());
    emu.raw_write(proxy::LONGJMP, proxy::PLAIN_LONGJMP);
    emu.raw_write(acttrace::MENU_STATE, &[0x31, 0xc0, 0xc3]);
    let menus = emu.call(0x1_40d8_72c0, &[], &[]).rax;
    if menus != 0 {
        emu.u64(menus + 0x100, 0);
    }
    acttrace::every_frame(&mut emu, g);
    emu.hook(uc::HOOK_CODE, on_resolve as *const c_void, RESOLVE, RESOLVE);
    emu.hook(uc::HOOK_CODE, on_after as *const c_void, AFTER_APPLY, AFTER_APPLY);
    emu.hook(uc::HOOK_CODE, on_tae as *const c_void, TAE_UPDATE, TAE_UPDATE);
    emu.hook(uc::HOOK_CODE, on_kb_start as *const c_void, KB_START, KB_START);
    emu.hook(uc::HOOK_CODE, on_kb_times as *const c_void, KB_TIMES, KB_TIMES);
    emu.hook(uc::HOOK_CODE, on_kb_dir as *const c_void, KB_DIR, KB_DIR);
    emu.hook(uc::HOOK_CODE, on_ctl_set as *const c_void, CTL_SET, CTL_SET);
    emu.st.starts = [CTL_TICK, KB_START, KB_TIMES, CTL_SET].into_iter().collect();
    let (wc, gc) = (ctl(&mut emu, w.player), ctl(&mut emu, g));
    let (wk, gk) = (module(&mut emu, w.player, 0xa8), module(&mut emu, g, 0xa8));
    for c in [wc, gc] {
        let _ = emu.peek::<4>(c);
        let _ = emu.peek::<4>(c + 0x8c);
        emu.hook(HOOK_MEM_WRITE, on_ctl_write as *const c_void, c, c + 0x8f);
    }
    {
        let mut s = SEEN.lock().unwrap();
        *s = Seen::default();
        s.ctls = vec![(wc, "wolf"), (gc, "general")];
        s.kbs = vec![(wk, "wolf"), (gk, "general")];
    }
    (emu, w, g)
}

/// One world frame: the hit step, the pre tasks, the General's chr frame, Wolf's, the post tasks.
fn frame(emu: &mut Emu, w: u64, g: u64, blk: u64) -> Option<String> {
    let wcm = emu.peek_u64(WORLD_CHR_MAN);
    let mgr = emu.peek_u64(HIT_MANAGER);
    proxy::flush_tb(emu);
    emu.st.entered.clear();
    let mut stop = emu.call(HIT_STEP, &[mgr, DT.to_bits() as u64], &[0.0, DT]).stopped;
    for task in TASKS_PRE {
        stop = stop.or(emu.call(task, &[wcm, blk], &[]).stopped);
    }
    stop = stop.or(emu.call(CHR_FRAME, &[wcm, 0, g], &[0.0, DT]).stopped);
    stop = stop.or(emu.call(CHR_FRAME, &[wcm, 0, w], &[0.0, DT]).stopped);
    for task in TASKS_POST {
        stop = stop.or(emu.call(task, &[wcm, blk], &[]).stopped);
    }
    stop.map(|(a, e)| format!("{a:#x} error {e}"))
}

fn take_frame(emu: &mut Emu, f: usize, w: u64, g: u64) -> Value {
    let seen = {
        let mut s = SEEN.lock().unwrap();
        Seen { hits: std::mem::take(&mut s.hits), applied: std::mem::take(&mut s.applied), anims: std::mem::take(&mut s.anims), events: std::mem::take(&mut s.events), ..Seen::default() }
    };
    let wdmod = module(emu, w, 0x98);
    let hits: Vec<Value> = seen
        .hits
        .iter()
        .enumerate()
        .map(|(k, (dmod, _, before))| {
            let mut h = seen.applied.get(k).map_or_else(|| hit_fields(before), |a| hit_fields(a));
            h["defender"] = json!(if *dmod == wdmod { "wolf" } else { "general" });
            h
        })
        .collect();
    let counts: BTreeMap<String, u64> = emu.st.entered.iter().map(|(a, c)| (format!("{a:#x}"), *c)).collect();
    let anims: Vec<Value> = seen.anims.iter().filter(|c| !(790000..800000).contains(&c.0)).map(|c| json!([c.0, c.1, c.2])).collect();
    json!({"f": f, "wolf": chr_state(emu, w), "general": chr_state(emu, g), "hits": hits, "events": seen.events, "tae": anims, "counts": counts})
}

fn duel(new_emu: &dyn Fn() -> Emu, out: &Path, d: &Duel) {
    let t = std::time::Instant::now();
    let (mut emu, w, g) = start(new_emu);
    let ids = acttrace::arm_inputs(&mut emu);
    let ai = acttrace::ai_of(&mut emu, g);
    if let Foe::Attack(_) = d.foe {
        emu.raw_write(0x1_43d7_a376, &[0]);
    }
    emu.hook(uc::HOOK_CODE, acttrace::on_request_copy as *const c_void, acttrace::AI_REQUEST_COPY, acttrace::AI_REQUEST_COPY);
    let wdata = module(&mut emu, w.player, 0x18);
    emu.raw_write(wdata + 0x130, &5000i32.to_le_bytes());
    emu.raw_write(wdata + 0x134, &5000i32.to_le_bytes());
    if let Some(p) = d.wolf_posture {
        emu.raw_write(wdata + 0x148, &p.to_le_bytes());
    }
    if let Some(p) = d.general_posture {
        let gdata = module(&mut emu, g, 0x18);
        let _ = emu.peek::<4>(gdata + 0x148);
        emu.raw_write(gdata + 0x148, &p.to_le_bytes());
    }
    let (def, att) = match d.foe {
        Foe::Attack(_) => (w.player, g),
        Foe::Stand { .. } => (g, w.player),
    };
    let dp = acttrace::pos(&mut emu, def);
    let dphys = module(&mut emu, def, 0x68);
    let yaw = f32at(&mut emu, dphys + 0x74);
    let at = [dp[0] - d.dist * yaw.sin(), dp[1], dp[2] - d.dist * yaw.cos()];
    let aphys = module(&mut emu, att, 0x68);
    let (wrapper, _) = proxy::proxy_of(&mut emu, aphys);
    proxy::place(&mut emu, aphys, wrapper, at);
    emu.poke_f32s(aphys + 0x74, &[yaw + std::f32::consts::PI]);
    let ones = emu.alloc(0x10);
    emu.poke_f32s(ones, &[1.0; 4]);
    let gm0 = module(&mut emu, g, 0);
    let blk = emu.alloc(0x40);
    emu.poke_f32s(blk + 8, &[DT]);
    println!("knockback {}: Wolf {:#x} General {:#x}", d.name, w.player, g);
    let mut frames = Vec::new();
    let mut stopped = None;
    for f in 0..d.frames {
        acttrace::press(&mut emu, w.stick, &ids, &pad(d.pad, f));
        *acttrace::FORCE.lock().unwrap() = (ai, match d.foe {
            Foe::Attack(b) if f < 3 => b,
            _ => -1,
        });
        if let Foe::Stand { guard, deflect_at } = d.foe {
            if guard {
                let flags = emu.peek_u64(gm0 + 0x70) | 4;
                emu.raw_write(gm0 + 0x70, &flags.to_le_bytes());
                emu.raw_write(gm0 + 0xb08, &900u32.to_le_bytes());
            }
            if deflect_at == Some(f) {
                let r = emu.call(SP_ADD, &[g, GENERAL_DEFLECT, g, 0, 0, ones, 0x3f80_0000, 0, 0, 0], &[]);
                assert!(r.stopped.is_none(), "SpEffect add stopped {:?}", r.stopped);
            }
        }
        let stop = frame(&mut emu, w.player, g, blk);
        let v = take_frame(&mut emu, f, w.player, g);
        if !v["hits"].as_array().unwrap().is_empty() || !v["events"].as_array().unwrap().is_empty() {
            println!("  f{f} hits {} events {}", v["hits"], v["events"]);
        }
        frames.push(v);
        if stop.is_some() {
            println!("  stopped {stop:?}");
            stopped = stop;
            break;
        }
    }
    println!("knockback {} ({} frames, {:.0} s)", d.name, frames.len(), t.elapsed().as_secs_f32());
    let v = json!({"name": d.name, "dt": DT, "dist": d.dist, "wolf_posture": d.wolf_posture, "general_posture": d.general_posture, "frames": frames, "stopped": stopped});
    std::fs::write(out.join(format!("knockback_{}.json", d.name)), serde_json::to_string(&v).unwrap()).unwrap();
}

/// A drive of the controller on the General (AI off, standing): `set` writes its fields after
/// frame `at`, then `frames` world frames.
struct Ctl {
    name: &'static str,
    frames: usize,
    at: usize,
    /// mode, target (move), target (turn: yaw in [1]), time, total
    mode: Option<(i32, [f32; 4], [f32; 4], f32)>,
    /// the +0x55 push: velocity, yaw rate, length
    push: Option<([f32; 3], f32, f32)>,
}

fn ctls() -> Vec<Ctl> {
    let c = |name, mode, push| Ctl { name, frames: 40, at: 2, mode, push };
    vec![
        c("ctl_m0_delta", Some((0, [0.6, 0.0, 0.0, 0.0], [0.0, 0.5, 0.0, 0.0], 0.3)), None),
        c("ctl_m0_long", Some((0, [0.0, 0.0, -1.2, 0.0], [0.0; 4], 0.5)), None),
        c("ctl_m1_point", Some((1, [0.8, 0.0, 0.4, 0.0], [0.0; 4], 0.25)), None),
        c("ctl_m2_face", Some((2, [0.0; 4], [0.0, 1.0, 0.0, 0.0], 0.2)), None),
        c("ctl_push", None, Some(([0.0, 0.0, 2.0], 0.0, 0.3))),
        c("ctl_push_turn", None, Some(([1.5, 0.0, 0.0], 3.0, 0.25))),
    ]
}

fn ctl_run(new_emu: &dyn Fn() -> Emu, out: &Path, c: &Ctl) {
    let t = std::time::Instant::now();
    let (mut emu, w, g) = start(new_emu);
    let blk = emu.alloc(0x40);
    emu.poke_f32s(blk + 8, &[DT]);
    // Wolf far off so he does not touch the General.
    let gp = acttrace::pos(&mut emu, g);
    let wphys = module(&mut emu, w.player, 0x68);
    let (wrapper, _) = proxy::proxy_of(&mut emu, wphys);
    proxy::place(&mut emu, wphys, wrapper, [gp[0] + 8.0, gp[1], gp[2] + 8.0]);
    let cb = ctl(&mut emu, g);
    let mut frames = Vec::new();
    let mut stopped = None;
    for f in 0..c.frames {
        if f == c.at {
            let gph = module(&mut emu, g, 0x68);
            let p = acttrace::pos(&mut emu, g);
            let yaw = f32at(&mut emu, gph + 0x74);
            if let Some((mode, mv, turn, time)) = c.mode {
                // Mode 1: a point relative to where he stands; mode 2: a yaw relative to his.
                let mv = if mode == 1 { [p[0] + mv[0], p[1] + mv[1], p[2] + mv[2], 1.0] } else { mv };
                let turn = if mode == 2 { [0.0, yaw + turn[1], 0.0, 0.0] } else { turn };
                emu.poke_f32s(cb + 0x20, &mv);
                emu.poke_f32s(cb + 0x30, &turn);
                emu.poke_f32s(cb + 0x40, &[time, time, time, time]);
                emu.raw_write(cb + 0x50, &mode.to_le_bytes());
                emu.raw_write(cb + 0x54, &[1]);
            }
            if let Some((v, yr, len)) = c.push {
                emu.poke_f32s(cb + 0x60, &[v[0], v[1], v[2], 0.0]);
                emu.poke_f32s(cb + 0x74, &[yr]);
                emu.poke_f32s(cb + 0x80, &[len, 0.0]);
                emu.raw_write(cb + 0x55, &[1]);
            }
        }
        let stop = frame(&mut emu, w.player, g, blk);
        frames.push(take_frame(&mut emu, f, w.player, g));
        if stop.is_some() {
            println!("  stopped {stop:?}");
            stopped = stop;
            break;
        }
    }
    println!("knockback {} ({} frames, {:.0} s)", c.name, frames.len(), t.elapsed().as_secs_f32());
    let v = json!({"name": c.name, "dt": DT, "at": c.at, "mode": c.mode.map(|m| json!({"mode": m.0, "move": m.1, "turn": m.2, "time": m.3})),
        "push": c.push.map(|p| json!({"vel": p.0, "yaw_rate": p.1, "len": p.2})), "frames": frames, "stopped": stopped});
    std::fs::write(out.join(format!("knockback_{}.json", c.name)), serde_json::to_string(&v).unwrap()).unwrap();
}

/// `knockback` runs every case; `knockback_<name>` one; `knockback_<prefix>*` those starting with it.
pub fn run(new_emu: &dyn Fn() -> Emu, out: &Path, want: &[&str]) {
    let names: Vec<&str> = want.iter().filter_map(|w| w.strip_prefix("knockback_")).collect();
    let on = |n: &str| names.is_empty() || names.iter().any(|w| w.strip_suffix('*').map_or(*w == n, |p| n.starts_with(p)));
    for d in duels() {
        if on(d.name) {
            duel(new_emu, out, &d);
        }
    }
    for c in ctls() {
        if on(c.name) {
            ctl_run(new_emu, out, &c);
        }
    }
}
