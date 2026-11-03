//! The HKS script engine surface (topic hks): Wolf's behaviour scripts (`extracted/hks_src/
//! c0000*.lua`) ask the engine through `env(id, ...)` and `act(id, ...)`. This runs the exe's env
//! dispatcher FUN_140b36410 (chr context, id, Lua state) on Wolf from the snapshot for every env id
//! the scripts use, with the arguments the scripts pass, and writes what it returns to
//! `extracted/emu/hks_env.json` (test `hks_env_matches_the_exe`).
//!
//! The Lua state is a blank block: the four argument readers the handlers call are replaced by a
//! hook that answers from the case's arguments (Lua index 2 is the first argument after the id).
//! The string ids (`env("<name>", ...)`) are looked up in the exe's own name -> id maps
//! ([0x143d85568] env, [0x143d85558] act, std::map keyed by UTF-16 names, id at node +0x90), as
//! FUN_140b362d0 / FUN_140b34ca0 do.

use std::collections::BTreeMap;
use std::ffi::c_void;
use std::path::Path;
use std::sync::Mutex;

use serde_json::{json, Value};

use super::{uc, Emu};

/// The env dispatcher: FUN_140b36410(longlong *ctx, int id, lua_State *L); ctx[0] is the ChrIns.
pub(super) const ENV: u64 = 0x1_40b3_6410;
/// Lua argument readers: (L, index) -> int, (L, index, default) -> int, (L, index) -> float,
/// (L, index, default) -> float.
const ARG_INT: u64 = 0x1_4101_0dc0;
const ARG_INT_OR: u64 = 0x1_4101_0dd0;
const ARG_FLOAT: u64 = 0x1_4101_0d50;
const ARG_FLOAT_OR: u64 = 0x1_4101_0d60;
/// The name -> id maps of `env("name")` and `act("name")`.
pub(super) const ENV_NAMES: u64 = 0x1_43d8_5568;
pub(super) const ACT_NAMES: u64 = 0x1_43d8_5558;

/// The arguments of the case being run (after the id) and the reads the handler made.
static ARGS: Mutex<Vec<f64>> = Mutex::new(Vec::new());
static READS: Mutex<Vec<(u64, i32)>> = Mutex::new(Vec::new());

extern "C" fn on_arg(uc_: uc::Engine, addr: u64, _size: u32, _data: *mut c_void) {
    let (mut idx, mut def) = (0u64, 0u64);
    unsafe {
        uc::uc_reg_read(uc_, uc::RDX, &mut idx as *mut u64 as _);
        uc::uc_reg_read(uc_, uc::R8, &mut def as *mut u64 as _);
    }
    let idx = idx as u32 as i32;
    READS.lock().unwrap().push((addr, idx));
    let v = if idx >= 2 { ARGS.lock().unwrap().get(idx as usize - 2).copied() } else { None };
    let rax: u64 = match addr {
        ARG_INT => v.unwrap_or(0.0) as i64 as u64,
        ARG_INT_OR => v.map(|v| v as i64 as u64).unwrap_or(def & 0xffff_ffff),
        _ => 0,
    };
    let x = v.unwrap_or(0.0) as f32;
    let mut b = [0u8; 16];
    b[..4].copy_from_slice(&x.to_le_bytes());
    unsafe {
        uc::uc_reg_write(uc_, uc::RAX, &rax as *const u64 as _);
        if matches!(addr, ARG_FLOAT | ARG_FLOAT_OR) {
            uc::uc_reg_write(uc_, uc::XMM0, b.as_ptr() as _);
        }
    }
}

/// One `env(...)` / `act(...)` call site's id and arguments as written (constants resolved).
#[derive(Default)]
pub(super) struct Calls {
    pub(super) count: usize,
    /// Argument lists as written -> how many call sites pass them.
    pub(super) shapes: BTreeMap<Vec<String>, usize>,
}

/// Every `env(` / `act(` call in Wolf's four script files: id -> calls.
pub(super) fn survey(hks: &Path) -> (BTreeMap<String, Calls>, BTreeMap<String, Calls>, BTreeMap<String, f64>) {
    let files = ["c0000.lua", "c0000_cmsg.lua", "c0000_define.lua", "c0000_transition.lua"];
    let texts: Vec<String> = files
        .iter()
        .map(|f| String::from_utf8_lossy(&std::fs::read(hks.join(f)).unwrap_or_else(|_| panic!("{} missing", hks.join(f).display()))).into_owned())
        .collect();
    // Constants: NAME = number at the start of a line (c0000_define.lua); TRUE is the engine's 1.
    let mut consts = BTreeMap::from([("TRUE".to_string(), 1.0)]);
    for t in &texts {
        for line in t.lines() {
            let Some((n, v)) = line.split_once('=') else { continue };
            let n = n.trim();
            let ok = n.chars().next().is_some_and(|c| c.is_ascii_uppercase() || c == '_') && n.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_');
            if let (true, Ok(v)) = (ok, v.trim().parse::<f64>()) {
                consts.entry(n.to_string()).or_insert(v);
            }
        }
    }
    let mut out = (BTreeMap::new(), BTreeMap::new());
    for t in &texts {
        let b = t.as_bytes();
        for (kind, map) in [("env(", &mut out.0), ("act(", &mut out.1)] {
            let mut from = 0;
            while let Some(p) = t[from..].find(kind) {
                let at = from + p;
                from = at + kind.len();
                if at > 0 && (b[at - 1].is_ascii_alphanumeric() || b[at - 1] == b'_') {
                    continue;
                }
                // Arguments up to the matching ')', split at top-level commas.
                let (mut depth, mut args, mut cur, mut quote) = (0, Vec::new(), String::new(), None);
                for c in t[from..].chars() {
                    match (quote, c) {
                        (Some(q), _) => {
                            cur.push(c);
                            if c == q {
                                quote = None;
                            }
                        }
                        (None, '"' | '\'') => {
                            quote = Some(c);
                            cur.push(c);
                        }
                        (None, '(' | '[' | '{') => {
                            depth += 1;
                            cur.push(c);
                        }
                        (None, ')' | ']' | '}') if depth == 0 => {
                            args.push(cur.trim().to_string());
                            break;
                        }
                        (None, ')' | ']' | '}') => {
                            depth -= 1;
                            cur.push(c);
                        }
                        (None, ',') if depth == 0 => args.push(std::mem::take(&mut cur).trim().to_string()),
                        _ => cur.push(c),
                    }
                }
                let id = args.remove(0);
                let e: &mut Calls = map.entry(id).or_default();
                e.count += 1;
                *e.shapes.entry(args).or_insert(0) += 1;
            }
        }
    }
    (out.0, out.1, consts)
}

/// A UTF-16 MSVC string inside a map node: data inline or behind a pointer (capacity > 7).
fn wstring(emu: &mut Emu, at: u64) -> String {
    let (len, cap) = (emu.peek_u64(at + 0x10), emu.peek_u64(at + 0x18));
    let data = if cap > 7 { emu.peek_u64(at) } else { at };
    let units: Vec<u16> = (0..len.min(256)).map(|i| u16::from_le_bytes(emu.peek::<2>(data + 2 * i))).collect();
    String::from_utf16_lossy(&units)
}

/// A string id as the lookup sees it: FUN_140b362d0 copies the name into a fixed string of 31
/// UTF-16 units (DLFixedStdAllocator, 0x20 with the terminator), and the map's keys are cut the same.
pub(super) fn name_key(id_text: &str) -> String {
    let units: Vec<u16> = id_text.trim_matches('"').encode_utf16().take(31).collect();
    String::from_utf16_lossy(&units)
}

/// The exe's name -> id map at `global` (std::map: head at +8; nodes left +0, parent +8, right
/// +0x10, is-nil +0x19; name at +0x68, id at +0x90).
pub(super) fn names(emu: &mut Emu, global: u64) -> BTreeMap<String, u32> {
    let map = emu.peek_u64(global);
    let mut out = BTreeMap::new();
    if map == 0 {
        return out;
    }
    let head = emu.peek_u64(map + 8);
    let mut todo = vec![emu.peek_u64(head + 8)];
    while let Some(n) = todo.pop() {
        if n == 0 || n == head || emu.peek::<1>(n + 0x19)[0] != 0 || out.len() > 10_000 {
            continue;
        }
        let name = wstring(emu, n + 0x68);
        out.insert(name, u32::from_le_bytes(emu.peek::<4>(n + 0x90)));
        todo.push(emu.peek_u64(n));
        todo.push(emu.peek_u64(n + 0x10));
    }
    out
}

/// The class name (MSVC RTTI) of an object whose vtable lies in the image.
pub(super) fn class_name(emu: &mut Emu, obj: u64) -> Option<String> {
    let vt = emu.peek_u64(obj);
    if !(0x1_4000_0000..0x1_4500_0000).contains(&vt) {
        return None;
    }
    let col = emu.peek_u64(vt - 8);
    if !(0x1_4000_0000..0x1_4500_0000).contains(&col) {
        return None;
    }
    let td = 0x1_4000_0000 + u32::from_le_bytes(emu.peek::<4>(col + 0xc)) as u64;
    let raw: Vec<u8> = (0..96).map(|i| emu.peek::<1>(td + 0x10 + i)[0]).take_while(|&c| c != 0).collect();
    Some(String::from_utf8_lossy(&raw).into_owned())
}

pub fn env_cases(emu: &mut Emu, out: &Path) {
    let wolf = emu.st.snap.as_ref().expect("hks needs --snapshot").player;
    let hks = out.parent().unwrap().join("hks_src");
    let (env, act, consts) = survey(&hks);
    let env_names = names(emu, ENV_NAMES);
    let act_names = names(emu, ACT_NAMES);
    // Wolf's module bag (chr +0x1ff8): the class behind each slot the handlers read.
    let bag = emu.peek_u64(wolf + 0x1ff8);
    let mut modules = serde_json::Map::new();
    for off in (0..0x140u64).step_by(8) {
        let m = emu.peek_u64(bag + off);
        if m != 0 {
            modules.insert(format!("{off:#x}"), json!(class_name(emu, m).unwrap_or_default()));
        }
    }
    for (name, off) in [("chr", 0u64), ("chr+0x50", 0x50), ("chr+0x11d0", 0x11d0), ("chr+0x120", 0x120)] {
        let p = if off == 0 { wolf } else { emu.peek_u64(wolf + off) };
        if p != 0 {
            modules.insert(name.into(), json!(class_name(emu, p).unwrap_or_default()));
        }
    }

    for a in [ARG_INT, ARG_INT_OR, ARG_FLOAT, ARG_FLOAT_OR] {
        emu.raw_write(a, &[0xc3]);
        emu.hook(uc::HOOK_CODE, on_arg as *const c_void, a, a);
    }
    let ctx = emu.alloc(8);
    emu.u64(ctx, wolf);
    let lua = emu.alloc(0x400);
    emu.write(lua, &[0u8; 0x400]);

    let resolve = |a: &str| -> Option<f64> { a.parse::<f64>().ok().or_else(|| consts.get(a).copied()) };
    let mut ids = Vec::new();
    let (mut ran, mut stopped) = (0, 0);
    for (id_text, calls) in &env {
        let id = match id_text.parse::<i64>() {
            Ok(v) => Some(v),
            Err(_) => env_names.get(&name_key(id_text)).map(|&v| v as i64),
        };
        let mut cases = Vec::new();
        for (shape, n) in &calls.shapes {
            let args: Option<Vec<f64>> = shape.iter().map(|a| resolve(a)).collect();
            let mut case = json!({"args_text": shape, "sites": n});
            if let (Some(id), Some(args)) = (id, args) {
                *ARGS.lock().unwrap() = args.clone();
                READS.lock().unwrap().clear();
                let r = emu.call(ENV, &[ctx, id as u64, lua], &[]);
                ran += 1;
                case["args"] = json!(args);
                case["ret"] = json!(r.rax as u32 as i32);
                case["arg_reads"] = json!(READS.lock().unwrap().iter().map(|(f, i)| format!("{f:#x}:{i}")).collect::<Vec<_>>());
                if let Some((at, e)) = r.stopped {
                    stopped += 1;
                    case["stopped"] = json!(format!("{at:#x} error {e}"));
                }
                if !r.calls.is_empty() {
                    let mut c = r.calls.clone();
                    c.sort();
                    c.dedup();
                    case["stubs"] = json!(c);
                }
                if !r.wild.is_empty() {
                    case["wild_reads"] = json!(r.wild.len());
                }
            }
            cases.push(case);
        }
        ids.push(json!({"id": id, "id_text": id_text, "count": calls.count, "cases": cases}));
    }
    // Wolf's SpEffects (SpecialEffect chr +0x11d0: entries from +8, next +0x70; id +0x58, param
    // +0x60, its behaviorRefId +0x3c0 and stateInfo +0x15e) for env 3036 / 3064 / 1116.
    let mut sp = Vec::new();
    let sp_head = emu.peek_u64(wolf + 0x11d0);
    let mut e = emu.peek_u64(sp_head + 8);
    while e != 0 && sp.len() < 200 {
        let param = emu.peek_u64(e + 0x60);
        let p = |emu: &mut Emu, o: u64| if param == 0 { 0 } else { u32::from_le_bytes(emu.peek::<4>(param + o)) };
        sp.push(json!({
            "id": i32::from_le_bytes(emu.peek::<4>(e + 0x58)),
            "behavior_ref": p(emu, 0x3c0) as i32,
            "state_info": p(emu, 0x15e) & 0xffff,
            "param_326": p(emu, 0x326) & 0xff,
            "param_16b": p(emu, 0x16b) & 0xff,
            "entry_1c": u32::from_le_bytes(emu.peek::<4>(e + 0x1c)),
            "entry_27": emu.peek::<1>(e + 0x27)[0],
            "entry_68": emu.peek::<1>(e + 0x68)[0],
        }));
        e = emu.peek_u64(e + 0x70);
    }

    // Pokes: one field written, the env id run again, the field restored. Each checks the field a
    // handler reads (the doc comments of src/hks_env.rs) by seeing its answer move with it.
    let flags = emu.peek_u64(bag); // SprjChrActionFlagModule
    let req = emu.peek_u64(bag + 0x80); // SprjChrActionRequestModule
    let phys = emu.peek_u64(bag + 0x68); // SprjChrPhysicsModule
    let data = emu.peek_u64(bag + 0x18); // SprjChrDataModule
    let ctrl = emu.peek_u64(wolf + 0x50);
    let ctrl_flags = emu.peek_u64(ctrl + 0x50); // PlayerCtrl +0x50
    let sp_first = emu.peek_u64(sp_head + 8);
    let sp_param = if sp_first != 0 { emu.peek_u64(sp_first + 0x60) } else { 0 };
    let sp_first_id = if sp_first != 0 { i32::from_le_bytes(emu.peek::<4>(sp_first + 0x58)) } else { 0 };
    let f = |v: f32| v.to_le_bytes().to_vec();
    let u = |v: u32| v.to_le_bytes().to_vec();
    // The request module's per-state list (+0x1b8 set: FUN_140b2cd10) gets a one-entry buffer.
    let list = emu.alloc(16);
    let req_state = u32::from_le_bytes(emu.peek::<4>(req + 0xc8));
    modules.insert("request_mode_1b8".into(), json!(emu.peek::<1>(req + 0x1b8)[0]));
    modules.insert("request_state_c8".into(), json!(req_state));
    modules.insert("hp_max".into(), json!(u32::from_le_bytes(emu.peek::<4>(data + 0x134))));
    #[rustfmt::skip]
    let pokes: Vec<(&str, i64, Vec<f64>, Vec<(u64, Vec<u8>)>)> = vec![
        ("ActionRequest arm 0: request module +0x20 bit 0 (stay state)", 1106, vec![0.0], vec![(req + 0x20, vec![1])]),
        ("ActionRequest arm 2: +0x20 bit 2 (stay state)", 1106, vec![2.0], vec![(req + 0x20, vec![4])]),
        ("ActionRequest arm 2: +0x20 not read out of the stay state (flags +0x10 bit 0 clear)", 1106, vec![2.0], vec![(req + 0x20, vec![4]), (flags + 0x10, vec![0])]),
        ("ActionRequest arm 2: out of the stay state, the list [+0x50, +0x58) entry {id = +0xc8, mask}", 1106, vec![2.0],
            vec![(flags + 0x10, vec![0]), (list, [req_state.to_le_bytes().to_vec(), u(0), 4u64.to_le_bytes().to_vec()].concat()), (req + 0x50, list.to_le_bytes().to_vec()), (req + 0x58, (list + 16).to_le_bytes().to_vec())]),
        ("ActionRequest arm 2: the list entry of another state id is not read", 1106, vec![2.0],
            vec![(flags + 0x10, vec![0]), (list, [(req_state + 1).to_le_bytes().to_vec(), u(0), 4u64.to_le_bytes().to_vec()].concat()), (req + 0x50, list.to_le_bytes().to_vec()), (req + 0x58, (list + 16).to_le_bytes().to_vec())]),
        ("ActionDuration arm 2: request module +0xe8 + 4 arm (s) x 1000", 1108, vec![2.0], vec![(req + 0xf0, f(0.25))]),
        ("GetVariableChangeValue 2: flags +0x3c8 {id, value} x count +0x9d0", 3063, vec![2.0], vec![(flags + 0x3c8, [u(7), f(9.0), u(0), u(2), f(1.5), u(0)].concat()), (flags + 0x9d0, u(2))]),
        ("IsMoveCancelPossible: ctrl +0x50 -> +0x88 bit 2 and +0x9c bit 4", 2000, vec![], vec![(ctrl_flags + 0x88, vec![4]), (ctrl_flags + 0x9c, vec![0x10])]),
        ("IsMoveCancelPossible: +0x88 bit 2 alone", 2000, vec![], vec![(ctrl_flags + 0x88, vec![4])]),
        ("IsLockedOn: chr +0x1070", 1118, vec![], vec![(wolf + 0x1070, vec![1])]),
        ("IsStayState: flags +0x10 bit 0 cleared", 1105, vec![], vec![(flags + 0x10, vec![0])]),
        ("GetDamageLevel: flags +0x38", 236, vec![], vec![(flags + 0x38, u(5))]),
        ("GetSpecialAttribute: flags +0x4c", 285, vec![], vec![(flags + 0x4c, u(3))]),
        ("GetReceivedDamageType: flags +0x5c", 202, vec![], vec![(flags + 0x5c, u(4))]),
        ("GetDamageDirection: flags +0x68 = 3 (2..5 -> 1)", 3032, vec![], vec![(flags + 0x68, u(3))]),
        ("GetReceivedDamageDirection: flags +0x68 = 0 -> 2", 222, vec![], vec![(flags + 0x68, u(0))]),
        ("GetReceivedDamageDirection: flags +0x68 = 3 -> 3", 222, vec![], vec![(flags + 0x68, u(3))]),
        ("GetBehaviorID 17: flags +0x40", 334, vec![17.0], vec![(flags + 0x40, u(17))]),
        ("GetBehaviorID 17: flags +0x44", 334, vec![17.0], vec![(flags + 0x44, u(17))]),
        ("IsAnimEnd 1: flags +0xaf0 bit 1", 339, vec![1.0], vec![(flags + 0xaf0, vec![2])]),
        ("GetDT: flags +0xaf8 (s) x 1000", 333, vec![], vec![(flags + 0xaf8, f(0.033))]),
        ("IsAtkContactTarget: flags +0x58 bit 4", 2004, vec![], vec![(flags + 0x58, vec![0x10])]),
        ("GetVerticalSpeed: physics +0x164 x 1000", 3003, vec![], vec![(phys + 0x164, f(-2.5))]),
        ("IsTrulyLanding: physics +0xa48 set", 248, vec![], vec![(phys + 0xa48, vec![1])]),
        ("IsTrulyLanding: physics +0x929 clear", 248, vec![], vec![(phys + 0x929, vec![0])]),
        ("GetHP: data module +0x130", 1000, vec![], vec![(data + 0x130, u(321))]),
        ("GetSpEffectID: the first entry's id", 1116, vec![sp_first_id as f64], vec![]),
        ("GetBehaviorRefID 999: the first entry's param +0x3c0", 3036, vec![999.0], vec![(sp_param + 0x3c0, u(999))]),
        ("GetBehaviorRefID 999: param +0x326 set and entry +0x27 clear", 3036, vec![999.0], vec![(sp_param + 0x3c0, u(999)), (sp_param + 0x326, vec![1]), (sp_first + 0x27, vec![0])]),
        ("GetBehaviorRefID 999: param +0x326 set, entry +0x27 set", 3036, vec![999.0], vec![(sp_param + 0x3c0, u(999)), (sp_param + 0x326, vec![1]), (sp_first + 0x27, vec![1])]),
    ];
    let owner = emu.peek_u64(req + 8);
    let owner_bag = emu.peek_u64(owner + 0x1ff8);
    let owner_flags = emu.peek_u64(owner_bag);
    modules.insert("request_owner".into(), json!(format!("{owner:#x} flags {owner_flags:#x} (wolf {wolf:#x}, flags {flags:#x})")));
    let mut poked = Vec::new();
    // What each poke should give if the handler reads what the doc comment says.
    let expect = [1, 1, 0, 1, 0, 250, 1500, 1, 0, 1, 0, 5, 3, 4, 1, 2, 3, 1, 1, 1, 33, 1, -2500, 0, 0, 321, 1, 1, 0, 1];
    assert_eq!(expect.len(), pokes.len());
    for ((what, id, args, writes), expect) in pokes.into_iter().zip(expect) {
        let old: Vec<(u64, Vec<u8>)> = writes
            .iter()
            .map(|(at, b)| {
                emu.peek::<1>(*at);
                let mut o = vec![0u8; b.len()];
                unsafe { uc::uc_mem_read(emu.uc, *at, o.as_mut_ptr() as _, b.len()) };
                (*at, o)
            })
            .collect();
        *ARGS.lock().unwrap() = args.clone();
        let before = emu.call(ENV, &[ctx, id as u64, lua], &[]).rax as u32 as i32;
        for (at, b) in &writes {
            emu.raw_write(*at, b);
        }
        let r = emu.call(ENV, &[ctx, id as u64, lua], &[]);
        for (at, b) in &old {
            emu.raw_write(*at, b);
        }
        poked.push(json!({"what": what, "id": id, "args": args, "before": before, "after": r.rax as u32 as i32, "expect": expect, "stopped": r.stopped.is_some()}));
    }

    let acts: Vec<Value> = act
        .iter()
        .map(|(id_text, calls)| {
            let id = id_text.parse::<i64>().ok().or_else(|| act_names.get(&name_key(id_text)).map(|&v| v as i64));
            json!({"id": id, "id_text": id_text, "count": calls.count,
                   "shapes": calls.shapes.iter().map(|(s, n)| json!({"args_text": s, "sites": n})).collect::<Vec<_>>()})
        })
        .collect();
    let body = json!({
        "function": format!("{ENV:#x}"),
        "wolf": format!("{wolf:#x}"),
        "modules": modules,
        "env_names": env_names,
        "act_names": act_names,
        "sp_effects": sp,
        "pokes": poked,
        "env": ids,
        "act": acts,
    });
    std::fs::write(out.join("hks_env.json"), serde_json::to_string_pretty(&body).unwrap()).unwrap();
    println!(
        "hks_env: {} env ids ({} calls), {} cases run, {} stopped early; {} act ids ({} calls); names env {} act {}",
        env.len(),
        env.values().map(|c| c.count).sum::<usize>(),
        ran,
        stopped,
        act.len(),
        act.values().map(|c| c.count).sum::<usize>(),
        env_names.len(),
        act_names.len()
    );
}
