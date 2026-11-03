//! `animstart`: the order of one chr frame FUN_140a4b570, to settle when a pressed action's clip
//! starts. Wolf (pad as acttrace) or the General (AI on, the forced request as acttrace) run whole
//! chr frames from the snapshot; code hooks on the frame's stages log, in call order: the ChrIns
//! pre-update FUN_1409f78a0, the chr's vtable slots 22-30, the pad manipulator FUN_1407ef120 /
//! its button map FUN_1407ef960, the HKS env FUN_140b36410 and act FUN_140b32410 dispatchers
//! (with the caller of the first one and the act ids), the behaviour module before Havok
//! FUN_140b45440, the Havok behaviour step FUN_141015fa0, the behaviour module after Havok
//! FUN_140b44980, the TAE update FUN_140b5c920 (anim, clip time before / after) and every write of
//! the TimeAct module's current anim ([chr+0x1ff8]+0x10 +0xf8).
//!
//! `animstart [animstart_<case>...]` writes extracted/emu/animstart_<case>.json: per frame the
//! ordered log. Results: docs/kb/frame-coverage.md "One frame's order".

use std::collections::HashMap;
use std::ffi::c_void;
use std::path::Path;
use std::sync::Mutex;

use serde_json::{json, Value};

use super::acttrace::{self, Input, ATTACK, CHR_FRAME, DT, GUARD, SUB_ATTACK, WORLD_CHR_MAN};
use super::{proxy, turn, uc, Emu};

const PLAYER_VT: u64 = 0x1_42a2_b338;
const ENEMY_VT: u64 = 0x1_42a2_7f28;
const TAE_UPDATE: u64 = 0x1_40b5_c920;
const HKS_ENV: u64 = 0x1_40b3_6410;
const HKS_ACT: u64 = 0x1_40b3_2410;
const HOOK_MEM_WRITE: i32 = 2048;

const FIXED: [(u64, &str); 10] = [
    (0x1_40b5_1b70, "tae_action_flag"),
    (0x1_40b5_8530, "tae_replay_window"),
    (0x1_409f_78a0, "pre_update"),
    (0x1_407e_f120, "pad_vf9"),
    (0x1_407e_f960, "pad_map"),
    (0x1_40b4_5440, "beh_pre"),
    (0x1_4101_5fa0, "havok_step"),
    (0x1_40b4_4980, "beh_post"),
    (0x1_40b5_c730, "tae_replay"),
    (0x1_407e_6570, "ai_request_copy"),
];

static NAMES: Mutex<Option<HashMap<u64, String>>> = Mutex::new(None);
static LOG: Mutex<Vec<Value>> = Mutex::new(Vec::new());
static CHR: Mutex<u64> = Mutex::new(0);

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

fn ret_addr(uc: uc::Engine) -> u64 {
    let (sp, mut ret) = (reg(uc, uc::RSP), 0u64);
    unsafe { uc::uc_mem_read(uc, sp, &mut ret as *mut u64 as _, 8) };
    ret
}

/// Appends to the log, folding a run of the same entry into a count.
fn push(v: Value) {
    let mut log = LOG.lock().unwrap();
    if let Some(last) = log.last_mut() {
        if last["at"] == v["at"] && v.get("id").is_none() && last.get("id").is_none() {
            let n = last["n"].as_u64().unwrap_or(1);
            last["n"] = json!(n + 1);
            return;
        }
    }
    log.push(v);
}

extern "C" fn on_stage(uc: uc::Engine, addr: u64, _size: u32, _data: *mut c_void) {
    let name = NAMES.lock().unwrap().as_ref().and_then(|m| m.get(&addr).cloned()).unwrap_or_default();
    // Only the chr this run is about (rcx is the chr for its slots and pre-update).
    let chr = *CHR.lock().unwrap();
    if name.starts_with("vf") && reg(uc, uc::RCX) != chr {
        return;
    }
    push(json!({"at": name, "from": format!("{:#x}", ret_addr(uc))}));
}

extern "C" fn on_hks(uc: uc::Engine, addr: u64, _size: u32, _data: *mut c_void) {
    let id = reg(uc, uc::RDX) as u32 as i32;
    if addr == HKS_ACT {
        push(json!({"at": "hks_act", "id": id, "from": format!("{:#x}", ret_addr(uc))}));
    } else {
        let mut log = LOG.lock().unwrap();
        match log.last_mut() {
            Some(l) if l["at"] == "hks_env" => l["n"] = json!(l["n"].as_u64().unwrap_or(1) + 1),
            _ => log.push(json!({"at": "hks_env", "n": 1, "from": format!("{:#x}", ret_addr(uc))})),
        }
    }
}

extern "C" fn on_tae(uc: uc::Engine, _addr: u64, _size: u32, _data: *mut c_void) {
    let id = reg(uc, uc::RCX) as u32 as i32;
    LOG.lock().unwrap().push(json!({"at": "tae", "anim": id, "t0": xmm(uc, 1), "t1": xmm(uc, 2)}));
}

extern "C" fn on_anim_write(uc: uc::Engine, _kind: i32, _addr: u64, _size: i32, value: i64, _data: *mut c_void) {
    let rip = reg(uc, uc::RIP);
    LOG.lock().unwrap().push(json!({"at": "anim_write", "anim": value as i32, "rip": format!("{rip:#x}")}));
}

struct Case {
    name: &'static str,
    /// Wolf's input on frame f (None: the General's forced attack 3000 on frames `press`..+3).
    arm: Option<u32>,
    press: usize,
    hold: usize,
    frames: usize,
}

const CASES: &[Case] = &[
    Case { name: "attack", arm: Some(ATTACK), press: 2, hold: 3, frames: 8 },
    Case { name: "guard", arm: Some(GUARD), press: 2, hold: 2, frames: 6 },
    Case { name: "prosthetic", arm: Some(SUB_ATTACK), press: 2, hold: 3, frames: 6 },
    Case { name: "general", arm: None, press: 2, hold: 3, frames: 7 },
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
    let (chr, vt) = match c.arm {
        Some(_) => (w.player, PLAYER_VT),
        None => {
            let g = acttrace::general(&mut emu);
            let ai = acttrace::ai_of(&mut emu, g);
            emu.raw_write(0x1_43d7_a376, &[0]);
            acttrace::every_frame(&mut emu, g);
            emu.hook(uc::HOOK_CODE, acttrace::on_request_copy as *const c_void, acttrace::AI_REQUEST_COPY, acttrace::AI_REQUEST_COPY);
            *acttrace::FORCE.lock().unwrap() = (ai, -1);
            (g, ENEMY_VT)
        }
    };
    *CHR.lock().unwrap() = chr;
    let mut names: HashMap<u64, String> = FIXED.iter().map(|(a, n)| (*a, n.to_string())).collect();
    for slot in 22..=30u64 {
        let f = emu.peek_u64(vt + 8 * slot);
        names.entry(f).or_insert(format!("vf{slot}"));
    }
    for a in names.keys().copied().collect::<Vec<_>>() {
        emu.hook(uc::HOOK_CODE, on_stage as *const c_void, a, a);
    }
    *NAMES.lock().unwrap() = Some(names);
    emu.hook(uc::HOOK_CODE, on_hks as *const c_void, HKS_ENV, HKS_ENV);
    emu.hook(uc::HOOK_CODE, on_hks as *const c_void, HKS_ACT, HKS_ACT);
    emu.hook(uc::HOOK_CODE, on_tae as *const c_void, TAE_UPDATE, TAE_UPDATE);
    let bag = emu.peek_u64(chr + 0x1ff8);
    let ta = emu.peek_u64(bag + 0x10);
    emu.hook(HOOK_MEM_WRITE, on_anim_write as *const c_void, ta + 0xf8, ta + 0xfb);
    let mut frames = Vec::new();
    for f in 0..c.frames {
        let on = (c.press..c.press + c.hold).contains(&f);
        match c.arm {
            Some(a) => acttrace::press(&mut emu, w.stick, &ids, &Input { arms: if on { std::slice::from_ref(match a {
                ATTACK => &ATTACK,
                GUARD => &GUARD,
                _ => &SUB_ATTACK,
            }) } else { &[] }, stick: [0.0, 0.0] }),
            None => acttrace::FORCE.lock().unwrap().1 = if on { 3000 } else { -1 },
        }
        proxy::flush_tb(&emu);
        LOG.lock().unwrap().clear();
        let r = emu.call(CHR_FRAME, &[wcm, 0, chr], &[0.0, DT]);
        let log = std::mem::take(&mut *LOG.lock().unwrap());
        println!("{} f{f}{}:", c.name, if on { " (pressed)" } else { "" });
        for e in &log {
            println!("    {e}");
        }
        frames.push(json!({"f": f, "pressed": on, "anim": acttrace::anim(&mut emu, chr), "log": log}));
        if let Some((a, e)) = r.stopped {
            println!("  stopped at {a:#x} error {e}");
            break;
        }
    }
    let v = json!({"case": c.name, "dt": DT, "press": c.press, "frames": frames});
    std::fs::write(out.join(format!("animstart_{}.json", c.name)), serde_json::to_string(&v).unwrap()).unwrap();
}

pub fn run(new_emu: &dyn Fn() -> Emu, out: &Path, want: &[&str]) {
    let names: Vec<&str> = want.iter().filter_map(|w| w.strip_prefix("animstart_")).collect();
    for c in CASES {
        if names.is_empty() || names.contains(&c.name) {
            run_case(new_emu, out, c);
        }
    }
}
