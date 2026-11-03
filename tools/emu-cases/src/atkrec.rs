//! The attack record a TAE behaviour event builds (topic atkrec).
//!
//! TAE InvokeAttackBehavior (event 1) FUN_140b50ae0 and PCBehavior (event 307) FUN_140b55510
//! turn a judge into a BehaviorParam row (chr vfunc 0x1c0 judge + hand, or 0x1c8 the PC row
//! itself) and call FUN_140b679e0 / FUN_140b67740: the attack info FUN_14083d220 +
//! FUN_14084fc50 (AtkParam id, the hand's weapon, SpEffect rates FUN_140bf8900), the behaviour's
//! posture cost (vfunc 0x1f8, PlayerIns::vf63 FUN_140a240e0), then FUN_14099fc00 builds one hit
//! entry per AtkParam shape (FUN_1410b6150 -> FUN_1410b6880: the five amounts FUN_14083ddc0,
//! flick, SpEffects, radius and dummies FUN_1410b6760) and registers each FUN_14099f310.
//!
//! - `atkrec_rows`: every attack row (refType 0) of BehaviorParam_PC on Wolf with each hand
//!   (vfunc 0x330's, -1, -2) and the General's BehaviorParam 210200xxx / 210201xxx: the info and
//!   every entry, built by the exe's own functions on the snapshot.
//! - `atkrec_judges`: Wolf's vfunc 0x1c0 (judge, hand) / 0x1c8 (judge) and the General's 0x1c0.
//! - `atkrec_frames`: whole chr frames FUN_140a4b570 with Wolf's pad (acttrace's attack, art,
//!   jump, step, sprint) and the General's forced attack 3000, every event / info / entry hooked.
//!
//! Writes extracted/emu/atkrec_{rows,judges,frames}.json for src/sim_tests/emu_atkrec.rs.

use std::ffi::c_void;
use std::path::Path;
use std::sync::Mutex;

use serde_json::{json, Value};

use super::acttrace::{self, Input, ATTACK, GUARD, JUMP, SP_MOVE, SUB_ATTACK};
use super::guard::vfunc;
use super::{proxy, read_u64, turn, uc, Emu, State};

const WORLD_CHR_MAN: u64 = 0x1_43d7_a1e0;
const CHR_FRAME: u64 = 0x1_40a4_b570;
/// The hit manager (DAT_143d77ef0) the entries register in.
const HIT_MANAGER: u64 = 0x1_43d7_7ef0;
/// TAE AttackBehavior (rcx the TAE context, +0x10 chr; rdx the event, +8 its data).
const INVOKE_ATTACK: u64 = 0x1_40b5_0ae0;
/// TAE PCBehavior.
const INVOKE_PC: u64 = 0x1_40b5_5510;
/// The attack info's constructor and fill (rcx info, rdx chr, r8 behaviour row, r9 hand, [5] flags).
const INFO_INIT: u64 = 0x1_4083_d220;
const INFO_FILL: u64 = 0x1_4084_fc50;
/// The entries' build and registration (rcx manager, edx row, r8 info, r9b 0, [5] old handle,
/// [6] attack type, [7] byte pointer, [8] float time).
const ISSUE: u64 = 0x1_4099_fc00;
/// One shape's entry registered (rdx the entry).
const REGISTER: u64 = 0x1_4099_f310;
/// Drops a registered attack (rcx manager, edx handle).
const DROP: u64 = 0x1_409a_0b80;
const DT: f32 = 1.0 / 60.0;

static LOG: Mutex<Vec<Value>> = Mutex::new(Vec::new());

/// Reads `n` bytes in a hook (mapping the snapshot's window first).
fn mem(uc: uc::Engine, st: &mut State, at: u64, n: usize) -> Vec<u8> {
    (0..n.div_ceil(8)).flat_map(|i| read_u64(uc, st, at + 8 * i as u64).to_le_bytes()).take(n).collect()
}

fn reg(uc: uc::Engine, r: i32) -> u64 {
    let mut v = 0u64;
    unsafe { uc::uc_reg_read(uc, r, &mut v as *mut u64 as _) };
    v
}

fn i32_at(b: &[u8], o: usize) -> i32 {
    i32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}
fn f32_at(b: &[u8], o: usize) -> f32 {
    f32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}
fn words(b: &[u8]) -> Vec<u32> {
    b.chunks(4).map(|c| u32::from_le_bytes(c.try_into().unwrap())).collect()
}

/// The attack info (FUN_14083d220 / FUN_14084fc50): +8 AtkParam id, +0xc 1 = AtkParam_Pc, +0x14
/// the BehaviorParam category byte, +0x18 the flags, +0x2c / +0x60-0x6c flat adds, +0x70 ..
/// +0xf8 rates (1 unless an SpEffect sets them, FUN_140bf8900), +0x128 hand, +0x12c the hand's
/// weapon (110000 bare hands with flags & 0x60), +0x134 a second weapon, +0x138 the weapon
/// attack power is used (flags & 0x68 == 0 and AtkParam +0x84), +0x139 FUN_1409f3860, +0x14d
/// AtkParam +0x192 bit 0.
fn info_json(b: &[u8]) -> Value {
    json!({"atk": i32_at(b, 8), "pc": i32_at(b, 0xc), "category": i32_at(b, 0x14), "flags": i32_at(b, 0x18),
        "flat": [f32_at(b, 0x2c), f32_at(b, 0x60), f32_at(b, 0x64), f32_at(b, 0x68), f32_at(b, 0x6c)],
        "rates": (0x70..0xf8).step_by(4).map(|o| f32_at(b, o)).collect::<Vec<_>>(),
        "hand": i32_at(b, 0x128), "weapon": i32_at(b, 0x12c), "w130": i32_at(b, 0x130), "weapon2": i32_at(b, 0x134),
        "power": b[0x138], "b139": b[0x139], "b14d": b[0x14d], "raw": words(b)})
}

/// One shape's hit entry (FUN_1410b6880 / FUN_1410b6760): floats 0-4 the five amounts
/// (FUN_14083ddc0), +0x20 AtkParam +0x6a, +0x34 FUN_140850f00, +0x38 FUN_1408476f0, +0x3c
/// FUN_140841b10, +0x40 the flick FUN_14084c650, +0x48 attack type, +0x4c behaviour row, +0x50
/// AtkParam id, +0x54 Pc, +0x60 radius, +0x64 shape kind, +0x8c-0xa8 eight SpEffect ids,
/// +0xd0 / +0xd4 the dummies, +0x11c the weapon.
fn entry_json(b: &[u8]) -> Value {
    json!({"amounts": (0..5).map(|i| f32_at(b, 4 * i)).collect::<Vec<_>>(), "f20": f32_at(b, 0x20),
        "f34": f32_at(b, 0x34), "f38": f32_at(b, 0x38), "f3c": f32_at(b, 0x3c), "flick": i32_at(b, 0x40),
        "type": i32_at(b, 0x48), "beh": i32_at(b, 0x4c), "atk": i32_at(b, 0x50), "pc": i32_at(b, 0x54),
        "radius": f32_at(b, 0x60), "kind": i32_at(b, 0x64),
        "sp": (0..8).map(|i| i32_at(b, 0x8c + 4 * i)).collect::<Vec<_>>(),
        "dmy1": i32_at(b, 0xd0), "dmy2": i32_at(b, 0xd4), "weapon": i32_at(b, 0x11c), "raw": words(b)})
}

extern "C" fn on_attack(uc: uc::Engine, _addr: u64, _size: u32, data: *mut c_void) {
    let st = unsafe { &mut *(data as *mut State) };
    let ctx = reg(uc, uc::RCX);
    let ev = reg(uc, uc::RDX);
    let chr = read_u64(uc, st, ctx + 0x10);
    let d = read_u64(uc, st, ev + 8);
    let b = mem(uc, st, d, 0x10);
    let ver = u16::from_le_bytes(mem(uc, st, ev + 0x1a, 2)[..2].try_into().unwrap());
    // AttackBehavior: +0 AttackType, +4 slot, +8 judge, +0xc direction byte (version >= 8),
    // +0xd Source (1 / 2 -> hand -1 / -2), +0xe StateInfo.
    LOG.lock().unwrap().push(json!({"ev": 1, "chr": chr, "type": i32_at(&b, 0), "slot": i32_at(&b, 4), "judge": i32_at(&b, 8),
        "dir": b[0xc], "source": b[0xd], "state": i16::from_le_bytes([b[0xe], b[0xf]]), "ver": ver}));
}

extern "C" fn on_pc(uc: uc::Engine, _addr: u64, _size: u32, data: *mut c_void) {
    let st = unsafe { &mut *(data as *mut State) };
    let ctx = reg(uc, uc::RCX);
    let ev = reg(uc, uc::RDX);
    let chr = read_u64(uc, st, ctx + 0x10);
    let d = read_u64(uc, st, ev + 8);
    let b = mem(uc, st, d, 0x10);
    // PCBehavior: short +0 attack type, short +2 slot, +4 flags (bit 2: the PC row itself), +8 judge.
    LOG.lock().unwrap().push(json!({"ev": 307, "chr": chr, "type": i16::from_le_bytes([b[0], b[1]]), "slot": i16::from_le_bytes([b[2], b[3]]),
        "flags": i32_at(&b, 4), "judge": i32_at(&b, 8)}));
}

extern "C" fn on_fill(uc: uc::Engine, _addr: u64, _size: u32, data: *mut c_void) {
    let st = unsafe { &mut *(data as *mut State) };
    let sp = reg(uc, uc::RSP);
    let flags = read_u64(uc, st, sp + 0x28) as u32;
    LOG.lock().unwrap().push(json!({"fill": reg(uc, uc::R8) as u32 as i32, "chr": reg(uc, uc::RDX), "hand": reg(uc, uc::R9) as u32 as i32, "flags": flags}));
}

extern "C" fn on_issue(uc: uc::Engine, _addr: u64, _size: u32, data: *mut c_void) {
    let st = unsafe { &mut *(data as *mut State) };
    let sp = reg(uc, uc::RSP);
    let info = reg(uc, uc::R8);
    let b = mem(uc, st, info, 0x150);
    let kind = read_u64(uc, st, sp + 0x30) as u32 as i32;
    LOG.lock().unwrap().push(json!({"issue": reg(uc, uc::RDX) as u32 as i32, "type": kind, "info": info_json(&b)}));
}

extern "C" fn on_register(uc: uc::Engine, _addr: u64, _size: u32, data: *mut c_void) {
    let st = unsafe { &mut *(data as *mut State) };
    let e = reg(uc, uc::RDX);
    let b = mem(uc, st, e, 0x130);
    LOG.lock().unwrap().push(json!({"entry": entry_json(&b)}));
}

fn hooks(emu: &mut Emu) {
    emu.hook(uc::HOOK_CODE, on_attack as *const c_void, INVOKE_ATTACK, INVOKE_ATTACK);
    emu.hook(uc::HOOK_CODE, on_pc as *const c_void, INVOKE_PC, INVOKE_PC);
    emu.hook(uc::HOOK_CODE, on_fill as *const c_void, INFO_FILL, INFO_FILL);
    emu.hook(uc::HOOK_CODE, on_issue as *const c_void, ISSUE, ISSUE);
    emu.hook(uc::HOOK_CODE, on_register as *const c_void, REGISTER, REGISTER);
}

/// As acttrace's start: the CRT's ISA level 1, a plain longjmp, no menu open.
fn setup(emu: &mut Emu) {
    emu.limit = 500_000_000;
    emu.raw_write(0x1_43c1_d520, &1u32.to_le_bytes());
    emu.raw_write(proxy::LONGJMP, proxy::PLAIN_LONGJMP);
    emu.raw_write(acttrace::MENU_STATE, &[0x31, 0xc0, 0xc3]);
    let menus = emu.call(0x1_40d8_72c0, &[], &[]).rax;
    if menus != 0 {
        emu.peek::<8>(menus + 0x100);
        emu.raw_write(menus + 0x100, &0u64.to_le_bytes());
    }
}

fn param_rows(out: &Path, name: &str) -> Vec<Value> {
    let path = out.parent().unwrap().join(format!("json/params/{name}.json"));
    let v: Value = serde_json::from_str(&std::fs::read_to_string(&path).expect("params json")).unwrap();
    match &v["rows"] {
        Value::Array(a) => a.clone(),
        Value::Object(o) => o.values().cloned().collect(),
        _ => Vec::new(),
    }
}

/// One behaviour row issued as FUN_140b679e0 does (info built by the exe, the posture cost, the
/// entries), the attack dropped again.
fn issue_row(emu: &mut Emu, chr: u64, row: i32, hand: i32, flags: u32, scratch: u64) -> Value {
    let info = scratch;
    let byte = scratch + 0x200;
    emu.write(info, &[0u8; 0x160]);
    emu.u8(byte, 0xff);
    LOG.lock().unwrap().clear();
    let r0 = emu.call(INFO_INIT, &[info], &[]);
    let r1 = emu.call(INFO_FILL, &[info, chr, row as u32 as u64, hand as u32 as u64, flags as u64], &[]);
    let cost_fn = vfunc(emu, chr, 0x1f8);
    let rc = emu.call(cost_fn, &[chr, row as u32 as u64, info], &[]);
    let mgr = emu.peek_u64(HIT_MANAGER);
    let r2 = emu.call(ISSUE, &[mgr, row as u32 as u64, info, 0, u32::MAX as u64, 0, byte, 10000f32.to_bits() as u64], &[]);
    let handle = r2.rax as u32 as i32;
    if handle != -1 {
        emu.call(DROP, &[mgr, handle as u32 as u64], &[]);
    }
    let log = std::mem::take(&mut *LOG.lock().unwrap());
    let issue = log.iter().find(|v| v.get("issue").is_some()).cloned();
    let entries: Vec<Value> = log.iter().filter_map(|v| v.get("entry").cloned()).collect();
    let stopped = [r0.stopped, r1.stopped, rc.stopped, r2.stopped].iter().flatten().next().map(|(a, e)| format!("{a:#x} error {e}"));
    json!({"row": row, "hand": hand, "flags": flags, "cost": rc.xmm0, "handle": handle,
        "info": issue.map(|i| i["info"].clone()), "entries": entries, "stopped": stopped})
}

fn rows(new_emu: &dyn Fn() -> Emu, out: &Path) {
    let t = std::time::Instant::now();
    let pc = param_rows(out, "BehaviorParam_PC");
    let npc = param_rows(out, "BehaviorParam");
    let atk_rows = |rows: &[Value], keep: &dyn Fn(i64) -> bool| -> Vec<i32> {
        let mut v: Vec<i32> = rows.iter().filter(|r| r["refType"].as_i64() == Some(0) && keep(r["id"].as_i64().unwrap_or(-1))).map(|r| r["id"].as_i64().unwrap() as i32).collect();
        v.sort();
        v
    };
    let wolf_rows = atk_rows(&pc, &|_| true);
    let general_rows = atk_rows(&npc, &|id| (210_200_000..210_202_000).contains(&id));
    let mut cases = Vec::new();
    let mut hands_seen = Value::Null;
    for (who, list) in [("wolf", &wolf_rows), ("general", &general_rows)] {
        for chunk in list.chunks(60) {
            let mut emu = new_emu();
            setup(&mut emu);
            hooks(&mut emu);
            let chr = if who == "wolf" { emu.st.snap.as_ref().expect("atkrec needs --snapshot").player } else { acttrace::general(&mut emu) };
            let hand_fn = vfunc(&mut emu, chr, 0x330);
            let hand = emu.call(hand_fn, &[chr], &[]).rax as u32 as i32;
            let weapon_fn = vfunc(&mut emu, chr, 0x338);
            if who == "wolf" && hands_seen.is_null() {
                let w: Vec<Value> = [hand, -1, -2, 0, 1].iter().map(|&h| json!([h, emu.call(weapon_fn, &[chr, h as u32 as u64], &[]).rax as u32 as i32])).collect();
                hands_seen = json!({"hand": hand, "weapons": w});
            }
            let scratch = emu.alloc(0x400);
            let mut hands: Vec<i32> = if who == "wolf" { vec![hand, -1, -2] } else { vec![hand] };
            hands.dedup();
            for (i, &row) in chunk.iter().enumerate() {
                for &h in &hands {
                    let mut c = issue_row(&mut emu, chr, row, h, 1, scratch);
                    c["chr"] = json!(who);
                    cases.push(c);
                }
                if i % 10 == 9 {
                    proxy::flush_tb(&emu);
                }
            }
        }
        println!("atkrec_rows {who}: {} rows ({:.0} s)", list.len(), t.elapsed().as_secs_f32());
    }
    let v = json!({"functions": ["0x14083d220", "0x14084fc50", "vfunc 0x1f8", "0x14099fc00", "0x14099f310"], "flags": 1, "wolf_hands": hands_seen, "cases": cases});
    std::fs::write(out.join("atkrec_rows.json"), serde_json::to_string(&v).unwrap()).unwrap();
}

fn judges(new_emu: &dyn Fn() -> Emu, out: &Path) {
    let mut emu = new_emu();
    setup(&mut emu);
    let wolf = emu.st.snap.as_ref().expect("atkrec needs --snapshot").player;
    let gen = acttrace::general(&mut emu);
    let mut cases = Vec::new();
    for (who, chr) in [("wolf", wolf), ("general", gen)] {
        let hand_fn = vfunc(&mut emu, chr, 0x330);
        let hand = emu.call(hand_fn, &[chr], &[]).rax as u32 as i32;
        let f1c0 = vfunc(&mut emu, chr, 0x1c0);
        let f1c8 = vfunc(&mut emu, chr, 0x1c8);
        let mut hands: Vec<i32> = if who == "wolf" { vec![hand, -1, -2] } else { vec![hand] };
        hands.dedup();
        for j in 0..1000u64 {
            for &h in &hands {
                let r = emu.call(f1c0, &[chr, j, h as u32 as u64, 0], &[]).rax as u32 as i32;
                cases.push(json!({"chr": who, "via": "0x1c0", "judge": j, "hand": h, "row": r}));
            }
            let r = emu.call(f1c8, &[chr, j], &[]).rax as u32 as i32;
            cases.push(json!({"chr": who, "via": "0x1c8", "judge": j, "row": r}));
            if j % 100 == 99 {
                proxy::flush_tb(&emu);
            }
        }
    }
    println!("atkrec_judges: {} calls", cases.len());
    std::fs::write(out.join("atkrec_judges.json"), serde_json::to_string(&json!({"cases": cases})).unwrap()).unwrap();
}

struct Run {
    name: &'static str,
    frames: usize,
    input: fn(usize) -> Input,
}

const NONE: Input = Input { arms: &[], stick: [0.0, 0.0] };

/// acttrace's inputs, plus a held three-slash combo.
const RUNS: [Run; 7] = [
    Run { name: "attack", frames: 70, input: |f| if f < 3 || (24..27).contains(&f) { Input { arms: &[ATTACK], stick: [0.0, 0.0] } } else { NONE } },
    Run { name: "combo", frames: 110, input: |f| if f % 24 < 3 { Input { arms: &[ATTACK], stick: [0.0, 0.0] } } else { NONE } },
    Run { name: "art", frames: 90, input: |f| if f < 3 { Input { arms: &[ATTACK, GUARD], stick: [0.0, 0.0] } } else { NONE } },
    Run { name: "jump", frames: 70, input: |f| if f < 3 { Input { arms: &[JUMP], stick: [0.0, 0.0] } } else { NONE } },
    Run { name: "step", frames: 45, input: |f| if f < 3 { Input { arms: &[SP_MOVE], stick: [0.0, 1.0] } } else { Input { arms: &[], stick: [0.0, 1.0] } } },
    Run { name: "sprint", frames: 60, input: |_| Input { arms: &[SP_MOVE], stick: [1.0, 0.0] } },
    Run { name: "prosthetic", frames: 70, input: |f| if f < 3 { Input { arms: &[SUB_ATTACK], stick: [0.0, 0.0] } } else { NONE } },
];

/// The log of one frame, the entries grouped under the issue before them.
fn frame_log(f: usize, anim: i64) -> Value {
    let log = std::mem::take(&mut *LOG.lock().unwrap());
    json!({"frame": f, "anim": anim, "log": log})
}

fn frames(new_emu: &dyn Fn() -> Emu, out: &Path, want: &[&str]) {
    let mut runs = Vec::new();
    for run in &RUNS {
        if !want.is_empty() && !want.contains(&run.name) {
            continue;
        }
        let t = std::time::Instant::now();
        let mut emu = new_emu();
        let w = turn::wolf(&mut emu);
        let ids = acttrace::arm_inputs(&mut emu);
        let wcm = emu.peek_u64(WORLD_CHR_MAN);
        setup(&mut emu);
        hooks(&mut emu);
        LOG.lock().unwrap().clear();
        let mut fr = Vec::new();
        let mut stopped = None;
        for f in 0..run.frames {
            acttrace::press(&mut emu, w.stick, &ids, &(run.input)(f));
            proxy::flush_tb(&emu);
            let r = emu.call(CHR_FRAME, &[wcm, 0, w.player], &[0.0, DT]);
            let anim = acttrace::anim(&mut emu, w.player);
            fr.push(frame_log(f, anim));
            if let Some((a, e)) = r.stopped {
                stopped = Some(format!("{a:#x} error {e}"));
                break;
            }
        }
        let n: usize = fr.iter().map(|f| f["log"].as_array().unwrap().len()).sum();
        println!("atkrec_frames {}: {} frames, {n} log lines, stopped {stopped:?} ({:.0} s)", run.name, fr.len(), t.elapsed().as_secs_f32());
        runs.push(json!({"name": run.name, "chr": "wolf", "dt": DT, "frames": fr, "stopped": stopped}));
    }
    if want.is_empty() || want.contains(&"general") {
        let t = std::time::Instant::now();
        let mut emu = new_emu();
        let g = acttrace::general(&mut emu);
        let ai = acttrace::ai_of(&mut emu, g);
        let wcm = emu.peek_u64(WORLD_CHR_MAN);
        emu.raw_write(0x1_43d7_a376, &[0]);
        emu.hook(uc::HOOK_CODE, acttrace::on_request_copy as *const c_void, acttrace::AI_REQUEST_COPY, acttrace::AI_REQUEST_COPY);
        acttrace::every_frame(&mut emu, g);
        setup(&mut emu);
        hooks(&mut emu);
        LOG.lock().unwrap().clear();
        let mut fr = Vec::new();
        let mut stopped = None;
        for f in 0..130 {
            proxy::flush_tb(&emu);
            acttrace::force_attack(ai, f);
            let r = emu.call(CHR_FRAME, &[wcm, 0, g], &[0.0, DT]);
            let anim = acttrace::anim(&mut emu, g);
            fr.push(frame_log(f, anim));
            if let Some((a, e)) = r.stopped {
                stopped = Some(format!("{a:#x} error {e}"));
                break;
            }
        }
        let n: usize = fr.iter().map(|f| f["log"].as_array().unwrap().len()).sum();
        println!("atkrec_frames general: {} frames, {n} log lines, stopped {stopped:?} ({:.0} s)", fr.len(), t.elapsed().as_secs_f32());
        runs.push(json!({"name": "general", "chr": "general", "dt": DT, "forced_attack": 3000, "frames": fr, "stopped": stopped}));
    }
    std::fs::write(out.join("atkrec_frames.json"), serde_json::to_string(&json!({"runs": runs})).unwrap()).unwrap();
}

pub(crate) fn run(new_emu: &dyn Fn() -> Emu, out: &Path, want: &[&str]) {
    let all = want.contains(&"atkrec");
    let on = |n: &str| all || want.contains(&n);
    if on("atkrec_judges") {
        judges(new_emu, out);
    }
    if on("atkrec_rows") {
        rows(new_emu, out);
    }
    if on("atkrec_frames") {
        let sub: Vec<&str> = want.iter().filter_map(|w| w.strip_prefix("atkrec_frames_")).collect();
        frames(new_emu, out, &sub);
    }
}
