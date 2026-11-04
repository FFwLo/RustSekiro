//! `framewin`: which TAE windows are live on the frame a clip is replaced (row E2). Whole world
//! frames as `hits` runs them (the hit manager step FUN_1409a0dc0, the WorldChrMan pre tasks, the
//! General's chr frame FUN_140a4b570, Wolf's, the post tasks), with, per frame:
//! - each chr's TAE updates FUN_140b5c920 (anim, clip time before / after);
//! - the ChrActionFlag words the TAE handler FUN_140b51b70 sets (module0 [[chr+0x1ff8]] +0x70,
//!   u64; ActionCtrl [[chr+0x50]+0x50] +0x1f8, u32), read at the frame's start (what the hit step
//!   sees), after the hit step and after each chr frame; every write to Wolf's +0x70 with the
//!   stage it came in and the writing instruction;
//! - the attack records the hit step tests (FUN_14099b3f0, record +0x2c the owner's handle,
//!   compared with chr +0x8) and the attack issues FUN_1409a1290 by stage;
//! - the hits as applied (FUN_140b6e5a0 after the apply: attacker, guarded, deflected).
//!
//! `framewin [framewin_<case>...]` writes extracted/emu/framewin_<case>.json. Results:
//! docs/kb/frame-coverage.md "Frame order at a clip change".

use std::ffi::c_void;
use std::path::Path;
use std::sync::Mutex;

use serde_json::{json, Value};

use super::acttrace::{self, Input, ATTACK, CHR_FRAME, DT, GUARD, TASKS_POST, TASKS_PRE, WORLD_CHR_MAN};
use super::hits::{module, AFTER_APPLY, HIT_MANAGER, HIT_STEP};
use super::{proxy, turn, uc, Emu};

const TAE_UPDATE: u64 = 0x1_40b5_c920;
const SHAPE_TEST: u64 = 0x1_4099_b3f0;
const ATTACK_ISSUE: u64 = 0x1_409a_1290;
const SP_ADD: u64 = 0x1_409e_9c00;
const GENERAL_DEFLECT: u64 = 200220;
const HOOK_MEM_WRITE: i32 = 2048;

#[derive(Default)]
struct Seen {
    stage: &'static str,
    tae: Vec<(i64, f32, f32)>,
    /// Owner handles of the records the hit step tested.
    tested: Vec<i32>,
    issues: Vec<&'static str>,
    writes: Vec<Value>,
    /// (attacker chr, record guarded byte +0x164, deflected +0x1c7) after the apply.
    resolves: Vec<(u64, u8, u8)>,
}
static SEEN: Mutex<Seen> = Mutex::new(Seen { stage: "", tae: Vec::new(), tested: Vec::new(), issues: Vec::new(), writes: Vec::new(), resolves: Vec::new() });

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

extern "C" fn on_tae(uc: uc::Engine, _addr: u64, _size: u32, _data: *mut c_void) {
    SEEN.lock().unwrap().tae.push((reg(uc, uc::RCX) as u32 as i32 as i64, xmm(uc, 1), xmm(uc, 2)));
}

extern "C" fn on_shape(uc: uc::Engine, _addr: u64, _size: u32, _data: *mut c_void) {
    let rec = reg(uc, uc::RCX);
    SEEN.lock().unwrap().tested.push(i32::from_le_bytes(mem(uc, rec + 0x2c)));
}

extern "C" fn on_issue(_uc: uc::Engine, _addr: u64, _size: u32, _data: *mut c_void) {
    let mut s = SEEN.lock().unwrap();
    let st = s.stage;
    s.issues.push(st);
}

extern "C" fn on_resolve(uc: uc::Engine, _addr: u64, _size: u32, _data: *mut c_void) {
    let rec = reg(uc, uc::R8);
    let b = mem::<0x1c8>(uc, rec);
    SEEN.lock().unwrap().resolves.push((reg(uc, uc::RDX), b[0x164], b[0x1c7]));
}

extern "C" fn on_flag_write(uc: uc::Engine, _kind: i32, addr: u64, size: i32, value: i64, _data: *mut c_void) {
    let rip = reg(uc, uc::RIP);
    let mut s = SEEN.lock().unwrap();
    let st = s.stage;
    s.writes.push(json!({"stage": st, "rip": format!("{rip:#x}"), "at": addr & 0xf, "size": size, "value": format!("{value:#x}")}));
}

/// Wolf's pad: guard held over a frame range, attack pressed for 3 frames from a frame.
#[derive(Clone, Copy)]
enum Pad {
    Idle,
    Guard(usize, usize),
    Attack(usize),
}

#[derive(Clone, Copy)]
enum Foe {
    /// AI on, behaviour forced on frames 0-2.
    Attack(i32),
    /// AI off, guarding (ShieldBlock forced each frame), the deflect SpEffect added on a frame.
    Deflect(usize),
}

struct Case {
    name: String,
    frames: usize,
    pad: Pad,
    foe: Foe,
    dist: f32,
}

fn cases() -> Vec<Case> {
    let c = |name: &str, frames, pad, foe, dist| Case { name: name.into(), frames, pad, foe, dist };
    let mut v = vec![
        c("g_open", 48, Pad::Idle, Foe::Attack(3000), 2.5),
        c("g_guard", 48, Pad::Guard(0, 99), Foe::Attack(3000), 2.5),
        c("w_deflect", 34, Pad::Attack(0), Foe::Deflect(26), 2.0),
    ];
    // Guard held from 0 and let go on frame r (the first frame the pad has it up).
    for r in [38, 39, 40, 41] {
        v.push(c(&format!("g_release{r}"), 46, Pad::Guard(0, r), Foe::Attack(3000), 2.5));
    }
    // Wolf attacks into the General's slash: his own attack's windows on the frame he is hit.
    for a in [10, 14, 18] {
        v.push(c(&format!("g_trade{a}"), 50, Pad::Attack(a), Foe::Attack(3000), 2.5));
    }
    v
}

fn pad(p: Pad, f: usize) -> Input {
    let arms: &'static [u32] = match p {
        Pad::Guard(a, b) if (a..b).contains(&f) => &[GUARD],
        Pad::Attack(at) if (at..at + 3).contains(&f) => &[ATTACK],
        _ => &[],
    };
    Input { arms, stick: [0.0, 0.0] }
}

/// The ChrActionFlag words of a chr: module0 +0x70 (u64), ActionCtrl +0x1f8 (u32).
fn flags(emu: &mut Emu, chr: u64) -> Value {
    let m0 = module(emu, chr, 0);
    let ctrl = emu.peek_u64(chr + 0x50);
    let ac = emu.peek_u64(ctrl + 0x50);
    json!([format!("{:#x}", emu.peek_u64(m0 + 0x70)), format!("{:#x}", u32::from_le_bytes(emu.peek(ac + 0x1f8)))])
}

fn stage(s: &'static str) {
    SEEN.lock().unwrap().stage = s;
}

fn run_case(new_emu: &dyn Fn() -> Emu, out: &Path, c: &Case) {
    let t = std::time::Instant::now();
    let mut emu = new_emu();
    let w = turn::wolf(&mut emu);
    let ids = acttrace::arm_inputs(&mut emu);
    let g = acttrace::general(&mut emu);
    let ai = acttrace::ai_of(&mut emu, g);
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
    if let Foe::Attack(_) = c.foe {
        emu.raw_write(0x1_43d7_a376, &[0]);
    }
    emu.hook(uc::HOOK_CODE, acttrace::on_request_copy as *const c_void, acttrace::AI_REQUEST_COPY, acttrace::AI_REQUEST_COPY);
    acttrace::every_frame(&mut emu, g);
    emu.hook(uc::HOOK_CODE, on_tae as *const c_void, TAE_UPDATE, TAE_UPDATE);
    emu.hook(uc::HOOK_CODE, on_shape as *const c_void, SHAPE_TEST, SHAPE_TEST);
    emu.hook(uc::HOOK_CODE, on_issue as *const c_void, ATTACK_ISSUE, ATTACK_ISSUE);
    emu.hook(uc::HOOK_CODE, on_resolve as *const c_void, AFTER_APPLY, AFTER_APPLY);
    let wm0 = module(&mut emu, w.player, 0);
    let _ = emu.peek::<8>(wm0 + 0x70);
    emu.hook(HOOK_MEM_WRITE, on_flag_write as *const c_void, wm0 + 0x70, wm0 + 0x77);
    let wdata = module(&mut emu, w.player, 0x18);
    emu.raw_write(wdata + 0x130, &5000i32.to_le_bytes());
    emu.raw_write(wdata + 0x134, &5000i32.to_le_bytes());
    let (def, att) = match c.foe {
        Foe::Attack(_) => (w.player, g),
        Foe::Deflect(_) => (g, w.player),
    };
    let dp = acttrace::pos(&mut emu, def);
    let dphys = module(&mut emu, def, 0x68);
    let yaw = f32::from_le_bytes(emu.peek(dphys + 0x74));
    let at = [dp[0] - c.dist * yaw.sin(), dp[1], dp[2] - c.dist * yaw.cos()];
    let aphys = module(&mut emu, att, 0x68);
    let (wrapper, _) = proxy::proxy_of(&mut emu, aphys);
    proxy::place(&mut emu, aphys, wrapper, at);
    emu.poke_f32s(aphys + 0x74, &[yaw + std::f32::consts::PI]);
    let ones = emu.alloc(0x10);
    emu.poke_f32s(ones, &[1.0; 4]);
    let gm0 = module(&mut emu, g, 0);
    let blk = emu.alloc(0x40);
    emu.poke_f32s(blk + 8, &[DT]);
    let (wh, gh) = (i32::from_le_bytes(emu.peek(w.player + 8)), i32::from_le_bytes(emu.peek(g + 8)));
    let who = |h: i32| if h == wh { "wolf" } else if h == gh { "general" } else { "other" };
    println!("framewin {}: Wolf {:#x} ({wh:#x}) General {g:#x} ({gh:#x})", c.name, w.player);
    let dt_bits = DT.to_bits() as u64;
    let mut frames = Vec::new();
    let mut stopped = None;
    for f in 0..c.frames {
        acttrace::press(&mut emu, w.stick, &ids, &pad(c.pad, f));
        let req = match c.foe {
            Foe::Attack(b) if f < 3 => b,
            _ => -1,
        };
        *acttrace::FORCE.lock().unwrap() = (ai, req);
        if let Foe::Deflect(at) = c.foe {
            let fl = emu.peek_u64(gm0 + 0x70) | 4;
            emu.raw_write(gm0 + 0x70, &fl.to_le_bytes());
            emu.raw_write(gm0 + 0xb08, &900u32.to_le_bytes());
            if at == f {
                let r = emu.call(SP_ADD, &[g, GENERAL_DEFLECT, g, 0, 0, ones, 0x3f80_0000, 0, 0, 0], &[]);
                assert!(r.stopped.is_none(), "SpEffect add stopped {:?}", r.stopped);
            }
        }
        proxy::flush_tb(&emu);
        *SEEN.lock().unwrap() = Seen::default();
        let pre = json!({"wolf": flags(&mut emu, w.player), "general": flags(&mut emu, g)});
        stage("hit_step");
        let mut stop = emu.call(HIT_STEP, &[mgr, dt_bits], &[0.0, DT]).stopped;
        let after_hit = json!({"wolf": flags(&mut emu, w.player), "general": flags(&mut emu, g)});
        stage("tasks_pre");
        for task in TASKS_PRE {
            stop = stop.or(emu.call(task, &[wcm, blk], &[]).stopped);
        }
        SEEN.lock().unwrap().tae.clear();
        stage("general");
        stop = stop.or(emu.call(CHR_FRAME, &[wcm, 0, g], &[0.0, DT]).stopped);
        let g_tae = std::mem::take(&mut SEEN.lock().unwrap().tae);
        stage("wolf");
        stop = stop.or(emu.call(CHR_FRAME, &[wcm, 0, w.player], &[0.0, DT]).stopped);
        let w_tae = std::mem::take(&mut SEEN.lock().unwrap().tae);
        stage("tasks_post");
        for task in TASKS_POST {
            stop = stop.or(emu.call(task, &[wcm, blk], &[]).stopped);
        }
        let post = json!({"wolf": flags(&mut emu, w.player), "general": flags(&mut emu, g)});
        let s = std::mem::take(&mut *SEEN.lock().unwrap());
        let tested: Vec<&str> = s.tested.iter().map(|&h| who(h)).collect();
        let hits: Vec<Value> = s.resolves.iter().map(|&(a, gd, df)| json!({"attacker": if a == w.player { "wolf" } else if a == g { "general" } else { "other" }, "guarded": gd, "deflected": df})).collect();
        let clips = |a: &[(i64, f32, f32)]| a.iter().filter(|c| !(790000..800000).contains(&c.0)).map(|c| format!("{}@{:.3}-{:.3}", c.0, c.1, c.2)).collect::<Vec<_>>();
        println!("  f{f} W {:?} G {:?} tested {tested:?} issues {:?} hits {hits:?} pre {pre} post {post}", clips(&w_tae), clips(&g_tae), s.issues);
        frames.push(json!({
            "f": f, "wolf_tae": w_tae, "general_tae": g_tae, "flags_pre": pre, "flags_after_hit": after_hit, "flags_post": post,
            "tested": tested, "issues": s.issues, "hits": hits, "wolf_flag_writes": s.writes,
            "wolf_hp": i32::from_le_bytes(emu.peek(wdata + 0x130)),
        }));
        if stop.is_some() {
            stopped = stop.map(|(a, e)| format!("{a:#x} error {e}"));
            println!("  stopped {stopped:?}");
            break;
        }
    }
    println!("framewin {} ({} frames, {:.0} s)", c.name, frames.len(), t.elapsed().as_secs_f32());
    let padv = match c.pad {
        Pad::Idle => json!("idle"),
        Pad::Guard(a, b) => json!({"guard": [a, b]}),
        Pad::Attack(a) => json!({"attack": a}),
    };
    let foe = match c.foe {
        Foe::Attack(b) => json!({"attack": b}),
        Foe::Deflect(at) => json!({"deflect_at": at}),
    };
    let v = json!({"name": c.name, "dt": DT, "pad": padv, "foe": foe, "dist": c.dist, "frames": frames, "stopped": stopped});
    std::fs::write(out.join(format!("framewin_{}.json", c.name)), serde_json::to_string(&v).unwrap()).unwrap();
}

/// `framewin` runs every case; `framewin_<name>` one; `framewin_<prefix>*` those starting with it.
pub fn run(new_emu: &dyn Fn() -> Emu, out: &Path, want: &[&str]) {
    let names: Vec<&str> = want.iter().filter_map(|w| w.strip_prefix("framewin_")).collect();
    for c in cases() {
        if names.is_empty() || names.iter().any(|n| n.strip_suffix('*').map_or(*n == c.name, |p| c.name.starts_with(p))) {
            run_case(new_emu, out, &c);
        }
    }
}
