//! `frameorder` (rows E3 / E4): which frame the one-shot TAE events land in. Wolf's whole chr
//! frames FUN_140a4b570 from the snapshot with the pad fed as acttrace does; hooks log, in call
//! order: the TAE update FUN_140b5c920 (clip id, time before / after), the Havok behaviour step
//! FUN_141015fa0, the queue replay FUN_140b5c730, the event dispatcher's handlers for type 32
//! SetWeaponStyle FUN_140b509b0, 66 / 67 AddSpEffect FUN_140b56500 (r8 = 0 / 1) and 401
//! FUN_140b56750 (each with the event's arg [rdx+8] and its entered byte +0x18), and the
//! SpEffect add FUN_1409e9c00 on Wolf (id, return address). Each clip seen has its 32 / 66 / 67 /
//! 401 events listed (start, end, type) as FUN_1425e5640 walks them.
//!
//! `frameorder [frameorder_<act>...]` writes extracted/emu/frameorder_<act>.json. Results:
//! docs/kb/frame-coverage.md "One-shot TAE events' frame".

use std::collections::BTreeMap;
use std::ffi::c_void;
use std::path::Path;
use std::sync::Mutex;

use serde_json::{json, Value};

use super::acttrace::{self, Input, ATTACK, CHR_FRAME, DT, GUARD, JUMP, SP_MOVE, SUB_ATTACK, WORLD_CHR_MAN};
use super::{proxy, taewin, turn, uc, Emu};

const TAE_UPDATE: u64 = 0x1_40b5_c920;
const HAVOK_STEP: u64 = 0x1_4101_5fa0;
const REPLAY: u64 = 0x1_40b5_c730;
const ON_STYLE: u64 = 0x1_40b5_09b0;
const ON_ADD: u64 = 0x1_40b5_6500;
const ON_401: u64 = 0x1_40b5_6750;
const SP_ADD: u64 = 0x1_409e_9c00;

static WOLF: Mutex<u64> = Mutex::new(0);
static LOG: Mutex<Vec<Value>> = Mutex::new(Vec::new());

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

fn mem(uc: uc::Engine, at: u64, n: usize) -> Vec<u8> {
    let mut b = vec![0u8; n];
    unsafe { uc::uc_mem_read(uc, at, b.as_mut_ptr() as _, n) };
    b
}

/// The dispatched event (rdx): +8 -> its arg (u32), +0x18 entered.
fn event(uc: uc::Engine) -> (i64, u8) {
    let ev = reg(uc, uc::RDX);
    let b = mem(uc, ev, 0x20);
    let data = u64::from_le_bytes(b[8..16].try_into().unwrap());
    let arg = if data == 0 { -1 } else { i32::from_le_bytes(mem(uc, data, 4).try_into().unwrap()) as i64 };
    (arg, b[0x18])
}

extern "C" fn on_code(uc: uc::Engine, addr: u64, _size: u32, _data: *mut c_void) {
    let v = match addr {
        TAE_UPDATE => json!({"at": "tae", "clip": reg(uc, uc::RCX) as u32 as i32, "prev": xmm(uc, 1), "cur": xmm(uc, 2)}),
        HAVOK_STEP => json!({"at": "havok_step"}),
        REPLAY => json!({"at": "replay"}),
        ON_STYLE => {
            let (arg, entered) = event(uc);
            json!({"at": "ev32", "arg": arg, "entered": entered})
        }
        ON_ADD | ON_401 => {
            let (arg, entered) = event(uc);
            let kind = if addr == ON_401 { 401 } else if reg(uc, uc::R8) as u8 == 1 { 67 } else { 66 };
            json!({"at": format!("ev{kind}"), "arg": arg, "entered": entered})
        }
        _ => {
            if reg(uc, uc::RCX) != *WOLF.lock().unwrap() {
                return;
            }
            let rsp = reg(uc, uc::RSP);
            let ret = u64::from_le_bytes(mem(uc, rsp, 8).try_into().unwrap());
            json!({"at": "sp_add", "id": reg(uc, uc::RDX) as u32 as i32, "ret": format!("{ret:#x}")})
        }
    };
    LOG.lock().unwrap().push(v);
}

struct Act {
    name: &'static str,
    frames: usize,
    input: fn(usize) -> Input,
}

const NONE: Input = Input { arms: &[], stick: [0.0, 0.0] };
const fn arms(a: &'static [u32]) -> Input {
    Input { arms: a, stick: [0.0, 0.0] }
}

/// acttrace's Wolf acts (the same pads).
const ACTS: &[Act] = &[
    Act { name: "attack", frames: 70, input: |f| if f < 3 || (24..27).contains(&f) { arms(&[ATTACK]) } else { NONE } },
    Act { name: "art", frames: 90, input: |f| if f < 3 { arms(&[ATTACK, GUARD]) } else { NONE } },
    Act { name: "deflect", frames: 40, input: |f| if f < 2 { arms(&[GUARD]) } else { NONE } },
    Act { name: "step", frames: 45, input: |f| if f < 3 { Input { arms: &[SP_MOVE], stick: [0.0, 1.0] } } else { Input { arms: &[], stick: [0.0, 1.0] } } },
    Act { name: "jump", frames: 70, input: |f| if f < 3 { arms(&[JUMP]) } else { NONE } },
    Act { name: "prosthetic", frames: 70, input: |f| if f < 3 { arms(&[SUB_ATTACK]) } else { NONE } },
    // Standing, the idle given two TAE 32 events (run_act).
    Act { name: "style", frames: 14, input: |_| NONE },
];

fn run_act(new_emu: &dyn Fn() -> Emu, out: &Path, a: &Act) {
    let t = std::time::Instant::now();
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
    *WOLF.lock().unwrap() = w.player;
    for at in [TAE_UPDATE, HAVOK_STEP, REPLAY, ON_STYLE, ON_ADD, ON_401, SP_ADD] {
        emu.hook(uc::HOOK_CODE, on_code as *const c_void, at, at);
    }
    let bag = emu.peek_u64(w.player + 0x1ff8);
    let module = emu.peek_u64(bag + 0x10);
    if a.name == "style" {
        // None of Wolf's acts above carries a TAE 32: his idle a000_000000 (playing in the
        // snapshot, about 2.37 s in) gets a synthetic list with SetWeaponStyle events (arg 1, the
        // sword in hand, as he is) 4.5 and 10.5 frames ahead, as taewin's synthetic lists.
        let entry = taewin::lookup(&mut emu, module, 0).expect("Wolf's a000_000000");
        // Frame 0's update of it is (2.36691, 2.38358] (frameorder_attack).
        let at = |k: f32| 2.3836 + k * DT;
        let anim = taewin::fake_anim(&mut emu, &[&[(at(3.5), at(3.5) + 0.1, 32), (at(9.5), at(9.5) + 0.1, 32)]]);
        let style = emu.alloc(0x10);
        emu.u32(style, 1);
        // Each event's data (+0x10 -> type +0, arg pointer +8): the arg pointer.
        let garr = emu.peek_u64(anim + 8);
        let base = emu.peek_u64(garr + 0x18);
        for i in 0..2 {
            emu.u64(base + 0x28 * i + 0x20, style);
        }
        emu.u64(entry + 8, anim);
    }
    let mut clips: BTreeMap<i32, Value> = BTreeMap::new();
    let mut frames = Vec::new();
    for f in 0..a.frames {
        acttrace::press(&mut emu, w.stick, &ids, &(a.input)(f));
        proxy::flush_tb(&emu);
        LOG.lock().unwrap().clear();
        let r = emu.call(CHR_FRAME, &[wcm, 0, w.player], &[0.0, DT]);
        let log = std::mem::take(&mut *LOG.lock().unwrap());
        for e in log.iter().filter(|e| e["at"] == "tae") {
            let id = e["clip"].as_i64().unwrap() as i32;
            if (790_000..800_000).contains(&(id % 1_000_000)) || clips.contains_key(&id) {
                continue;
            }
            let evs = taewin::lookup(&mut emu, module, id as u32).map(|entry| emu.peek_u64(entry + 8)).filter(|&an| an != 0).map(|an| taewin::read_events(&mut emu, an)).unwrap_or_default();
            let keep: Vec<Value> = evs.iter().filter(|e| matches!(e.2, 32 | 66 | 67 | 401)).map(|e| json!([e.0, e.1, e.2])).collect();
            clips.insert(id, json!(keep));
        }
        let evs: Vec<&Value> = log.iter().filter(|e| e["at"].as_str().is_some_and(|s| s.starts_with("ev") || s == "sp_add")).collect();
        if !evs.is_empty() {
            println!("{} f{f}: {}", a.name, serde_json::to_string(&evs).unwrap());
        }
        frames.push(json!({"f": f, "anim": acttrace::anim(&mut emu, w.player), "log": log}));
        if let Some((at, e)) = r.stopped {
            println!("  stopped at {at:#x} error {e}");
            break;
        }
    }
    println!("{}: {} frames, clips {:?}, {:.0} s", a.name, frames.len(), clips.keys().collect::<Vec<_>>(), t.elapsed().as_secs_f32());
    let clips: BTreeMap<String, Value> = clips.into_iter().map(|(k, v)| (k.to_string(), v)).collect();
    let v = json!({"act": a.name, "dt": DT, "clips": clips, "frames": frames});
    std::fs::write(out.join(format!("frameorder_{}.json", a.name)), serde_json::to_string(&v).unwrap()).unwrap();
}

pub fn run(new_emu: &dyn Fn() -> Emu, out: &Path, want: &[&str]) {
    let names: Vec<&str> = want.iter().filter_map(|w| w.strip_prefix("frameorder_")).collect();
    for a in ACTS {
        if names.is_empty() || names.contains(&a.name) {
            run_act(new_emu, out, a);
        }
    }
}
