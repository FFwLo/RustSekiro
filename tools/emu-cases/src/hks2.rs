//! The HKS engine surface, step 2 (topic hks2): Wolf's env ids in poses other than idle, and the
//! act dispatcher run.
//!
//! `hks2_env`: every env handler field Wolf's scripts reach (named `<module>+0x<off>`, the names
//! src/hks_env.rs uses) is read on the snapshot ("idle"), then each pose writes some of them, runs
//! the env dispatcher FUN_140b36410 for one id and restores them -> `hks2_env.json` (test
//! `hks_env_poses_match_the_exe`).
//!
//! `hks2_act`: the act dispatcher FUN_140b32410(ctx, id, lua_State) for every act id and argument
//! list Wolf's scripts pass, each on a fresh emulator; what it wrote in Wolf's modules (a byte diff)
//! and the named fields after it -> `hks2_act.json` (test `hks_act_matches_the_exe`).

use std::collections::BTreeMap;
use std::ffi::c_void;
use std::path::Path;
use std::sync::Mutex;

use serde_json::{json, Value};

use super::{uc, Emu};

/// The act dispatcher: FUN_140b32410(longlong *ctx, int id, lua_State *L); ctx[0] is the ChrIns.
const ACT: u64 = 0x1_40b3_2410;
/// FUN_140bbfc00(physics, float *out): the physics position (GetFallHeight takes its y).
const PHYS_POS: u64 = 0x1_40bb_fc00;
/// The wire target notice singleton ([0x143d8e058] +8 == 1: a wire point in reach).
const WIRE_TARGET: u64 = 0x1_43d8_e058;
/// DebugFlags: +0x0 player no death (on in the snapshot), +0xb all no death.
const DEBUG_FLAGS: u64 = 0x1_43d7_a366;
/// The fall drop FUN_140bc2210 calls deadly (a debug variable, FUN_140795380 range 0..10000).
const FALL_DEATH_DROP: u64 = 0x1_43b0_9efc;

static ARGS: Mutex<Vec<f64>> = Mutex::new(Vec::new());

/// The Lua argument readers (as in hks.rs): answer from ARGS (Lua index 2 = the first argument).
extern "C" fn on_arg(uc_: uc::Engine, addr: u64, _size: u32, _data: *mut c_void) {
    let (mut idx, mut def) = (0u64, 0u64);
    unsafe {
        uc::uc_reg_read(uc_, uc::RDX, &mut idx as *mut u64 as _);
        uc::uc_reg_read(uc_, uc::R8, &mut def as *mut u64 as _);
    }
    let idx = idx as u32 as i32;
    let v = if idx >= 2 { ARGS.lock().unwrap().get(idx as usize - 2).copied() } else { None };
    let rax: u64 = match addr {
        0x1_4101_0dc0 => v.unwrap_or(0.0) as i64 as u64,
        0x1_4101_0dd0 => v.map(|v| v as i64 as u64).unwrap_or(def & 0xffff_ffff),
        _ => 0,
    };
    let x = v.unwrap_or(0.0) as f32;
    let mut b = [0u8; 16];
    b[..4].copy_from_slice(&x.to_le_bytes());
    unsafe {
        uc::uc_reg_write(uc_, uc::RAX, &rax as *const u64 as _);
        uc::uc_reg_write(uc_, uc::XMM0, b.as_ptr() as _);
    }
}

fn hook_args(emu: &mut Emu) {
    for a in [0x1_4101_0dc0u64, 0x1_4101_0dd0, 0x1_4101_0d50, 0x1_4101_0d60] {
        emu.raw_write(a, &[0xc3]);
        emu.hook(uc::HOOK_CODE, on_arg as *const c_void, a, a);
    }
}

#[derive(Clone, Copy, Debug)]
enum Ty {
    U8,
    I16,
    U16,
    I32,
    U32,
    F32,
    U64,
}

impl Ty {
    fn get(self, b: &[u8]) -> f64 {
        match self {
            Ty::U8 => b[0] as f64,
            Ty::I16 => i16::from_le_bytes([b[0], b[1]]) as f64,
            Ty::U16 => u16::from_le_bytes([b[0], b[1]]) as f64,
            Ty::I32 => i32::from_le_bytes(b[..4].try_into().unwrap()) as f64,
            Ty::U32 => u32::from_le_bytes(b[..4].try_into().unwrap()) as f64,
            Ty::F32 => f32::from_le_bytes(b[..4].try_into().unwrap()) as f64,
            Ty::U64 => u64::from_le_bytes(b[..8].try_into().unwrap()) as f64,
        }
    }
    fn put(self, v: f64) -> Vec<u8> {
        match self {
            Ty::U8 => vec![v as u8],
            Ty::I16 => (v as i16).to_le_bytes().to_vec(),
            Ty::U16 => (v as u16).to_le_bytes().to_vec(),
            Ty::I32 => (v as i32).to_le_bytes().to_vec(),
            Ty::U32 => (v as u32).to_le_bytes().to_vec(),
            Ty::F32 => (v as f32).to_le_bytes().to_vec(),
            Ty::U64 => (v as u64).to_le_bytes().to_vec(),
        }
    }
}

/// The fields the env handlers read and the acts write, by the names src/hks_env.rs uses.
/// `<module>+0x<off>`: module "flags" (bag +0x0 SprjChrActionFlagModule), "data" (+0x18), "talk"
/// (+0x50), "phys" (+0x68), "request" (+0x80), "throw" (+0x88), "damage" (+0x98), "knockback"
/// (+0xa8), "fdp" (+0xc0 FdpDataExModule), "docking" (+0x128), "wire" (+0x130), "chr" (PlayerIns),
/// "ctrlflags" (PlayerCtrl +0x50 block), "turn" (BehaviorTurn, [PlayerCtrl +0x250]); and through
/// pointers "weapons" ([chr +0x120], ints), "throw_entry" ([throw +0x10]), "life" ([[chr +0x18]
/// +0x30], IsTrueDeath's +0x8c), "wire_target" ([0x143d8e058]).
#[rustfmt::skip]
const FIELDS: &[(&str, Ty)] = &[
    ("flags+0x10", Ty::U32), ("flags+0x14", Ty::I32), ("flags+0x20", Ty::U32), ("flags+0x28", Ty::U32), ("flags+0x38", Ty::I32), ("flags+0x3c", Ty::I32),
    ("flags+0x40", Ty::I32), ("flags+0x44", Ty::I32), ("flags+0x4c", Ty::I32), ("flags+0x54", Ty::U32), ("flags+0x58", Ty::U32),
    ("flags+0x5c", Ty::I32), ("flags+0x60", Ty::I16), ("flags+0x62", Ty::I16), ("flags+0x64", Ty::I16), ("flags+0x66", Ty::I16),
    ("flags+0x68", Ty::I32), ("flags+0x70", Ty::U32), ("flags+0x9d0", Ty::I32), ("flags+0xaf0", Ty::U64), ("flags+0xaf8", Ty::F32),
    ("data+0x130", Ty::I32), ("data+0x228", Ty::U8),
    ("debug+0x0", Ty::U8), ("debug+0xb", Ty::U8),
    ("talk+0x14", Ty::I32), ("talk+0x18", Ty::I32), ("talk+0x28", Ty::U8),
    ("phys+0x74", Ty::F32), ("phys+0x3c0", Ty::I32), ("phys+0x3d0", Ty::F32), ("phys+0x3e0", Ty::F32), ("phys+0xa2", Ty::U8), ("phys+0x100", Ty::U8), ("phys+0x164", Ty::F32), ("phys+0x854", Ty::F32), ("phys+0x900", Ty::F32), ("phys+0x8d0", Ty::F32),
    ("phys+0x929", Ty::U8), ("phys+0x93a", Ty::U8), ("phys+0xa48", Ty::U8), ("phys+0xab4", Ty::U8), ("phys+0xab5", Ty::U8),
    ("request+0x188", Ty::U32),
    ("throw+0x63", Ty::U8), ("throw+0x79", Ty::U8), ("throw+0x7b", Ty::U8),
    ("damage+0x50", Ty::F32), ("damage+0x54", Ty::F32), ("damage+0x58", Ty::F32), ("damage+0x60", Ty::I32),
    ("knockback+0x10", Ty::F32), ("knockback+0x44", Ty::U16),
    ("fdp+0x11", Ty::U8),
    ("docking+0x7c0", Ty::U8), ("docking+0x7cc", Ty::U8), ("docking+0x844", Ty::U8), ("docking+0x84c", Ty::U8), ("docking+0x84d", Ty::U8),
    ("docking+0x8a0", Ty::U8), ("docking+0x8a1", Ty::U8), ("docking+0x8a2", Ty::U8),
    ("docking+0x7bc", Ty::I32), ("docking+0x7c8", Ty::I32), ("docking+0x7d4", Ty::I32), ("docking+0x7e0", Ty::I32), ("docking+0x7ec", Ty::I32),
    ("docking+0x7f8", Ty::I32), ("docking+0x804", Ty::I32), ("docking+0x810", Ty::I32), ("docking+0x81c", Ty::I32), ("docking+0x828", Ty::I32),
    ("docking+0x834", Ty::I32), ("docking+0x840", Ty::I32),
    ("ctrl+0x258", Ty::U8), ("ctrl+0x350", Ty::I32),
    ("wire+0x10", Ty::U8), ("wire+0x11", Ty::U8),
    ("chr+0x1070", Ty::U8), ("chr+0x11cc", Ty::I32), ("chr+0x1acd", Ty::U8), ("chr+0x2008", Ty::I32), ("chr+0x200c", Ty::I32),
    ("ctrlflags+0x88", Ty::U32), ("ctrlflags+0x8c", Ty::U32), ("ctrlflags+0x9c", Ty::U32), ("ctrlflags+0x4f8", Ty::I32),
    ("ctrlflags+0x51d", Ty::U8),
    ("turn+0x78", Ty::F32), ("turn+0x7c", Ty::F32),
    ("weapons+0x0", Ty::I32),
    ("throw_entry+0x18", Ty::U8),
    ("life+0x8c", Ty::U8),
    ("wire_target+0x8", Ty::U8),
];

/// GetVariableChangeValue's list: flags +0x3c8 {int id, float value, int} x count (+0x9d0).
const VAR_LIST: u64 = 0x3c8;
/// ActionDuration: request +0xe8 + 4 x arm (seconds held).
const HOLD: u64 = 0xe8;

struct Objs(BTreeMap<&'static str, u64>);

fn objs(emu: &mut Emu) -> Objs {
    let chr = emu.st.snap.as_ref().expect("hks2 needs --snapshot").player;
    let bag = emu.peek_u64(chr + 0x1ff8);
    let ctrl = emu.peek_u64(chr + 0x50);
    let mut m = BTreeMap::new();
    m.insert("chr", chr);
    for (n, off) in [("flags", 0u64), ("data", 0x18), ("talk", 0x50), ("phys", 0x68), ("fall", 0x70), ("request", 0x80), ("throw", 0x88), ("damage", 0x98), ("knockback", 0xa8), ("fdp", 0xc0), ("docking", 0x128), ("wire", 0x130), ("homing", 0x108)] {
        m.insert(n, emu.peek_u64(bag + off));
    }
    m.insert("ctrl", ctrl);
    m.insert("ctrlflags", emu.peek_u64(ctrl + 0x50));
    m.insert("turn", emu.peek_u64(ctrl + 0x250));
    m.insert("weapons", emu.peek_u64(chr + 0x120));
    let throw = m["throw"];
    m.insert("throw_entry", emu.peek_u64(throw + 0x10));
    let c18 = emu.peek_u64(chr + 0x18);
    m.insert("life", emu.peek_u64(c18 + 0x30));
    m.insert("wire_target", emu.peek_u64(WIRE_TARGET));
    m.insert("debug", DEBUG_FLAGS);
    Objs(m)
}

/// A field name -> (address, type). Names outside FIELDS take the type from a suffix-less guess:
/// "request+0x<e8 + 4 arm>" f32 (hold times), "ctrlflags+0x<4f8 + 4 i>" i32, "docking+0x<7c0 +
/// 12 i>" u8.
fn field(o: &Objs, name: &str) -> (u64, Ty) {
    let (obj, off) = name.split_once("+0x").unwrap_or_else(|| panic!("field {name}"));
    let off = u64::from_str_radix(off, 16).unwrap();
    let base = *o.0.get(obj).unwrap_or_else(|| panic!("object {obj}"));
    assert!(base != 0, "{obj} is null");
    let ty = FIELDS.iter().find(|f| f.0 == name).map(|f| f.1).unwrap_or(match obj {
        "request" => Ty::F32,
        "docking" | "phys" | "chr" => Ty::U8,
        _ => Ty::I32,
    });
    (base + off, ty)
}

fn read_field(emu: &mut Emu, o: &Objs, name: &str) -> f64 {
    let (at, ty) = field(o, name);
    let b: [u8; 8] = {
        emu.peek::<1>(at);
        emu.peek::<1>(at + 7);
        let mut b = [0u8; 8];
        unsafe { uc::uc_mem_read(emu.uc, at, b.as_mut_ptr() as _, 8) };
        b
    };
    ty.get(&b)
}

/// Writes one field (or a GetVariableChangeValue entry "var[<id>]"); returns what to restore.
fn write_field(emu: &mut Emu, o: &Objs, name: &str, v: f64, n_var: &mut u32) -> Vec<(u64, Vec<u8>)> {
    let mut old = Vec::new();
    let mut put = |emu: &mut Emu, at: u64, b: Vec<u8>| {
        emu.peek::<1>(at);
        let mut o = vec![0u8; b.len()];
        unsafe { uc::uc_mem_read(emu.uc, at, o.as_mut_ptr() as _, b.len()) };
        old.push((at, o));
        emu.raw_write(at, &b);
    };
    if let Some(id) = name.strip_prefix("var[").and_then(|r| r.strip_suffix(']')) {
        let flags = o.0["flags"];
        let at = flags + VAR_LIST + 12 * *n_var as u64;
        put(emu, at, [id.parse::<i32>().unwrap().to_le_bytes(), (v as f32).to_le_bytes(), 0i32.to_le_bytes()].concat());
        *n_var += 1;
        put(emu, flags + 0x9d0, n_var.to_le_bytes().to_vec());
    } else {
        let (at, ty) = field(o, name);
        put(emu, at, ty.put(v));
    }
    old
}

fn restore(emu: &mut Emu, old: Vec<(u64, Vec<u8>)>) {
    for (at, b) in old.into_iter().rev() {
        emu.raw_write(at, &b);
    }
}

fn env(emu: &mut Emu, ctx: u64, lua: u64, id: i64, args: &[f64]) -> (i32, bool) {
    *ARGS.lock().unwrap() = args.to_vec();
    let r = emu.call(super::hks::ENV, &[ctx, id as u64, lua], &[]);
    (r.rax as u32 as i32, r.stopped.is_some())
}

type Pose = (&'static str, i64, Vec<f64>, Vec<(String, f64)>);

fn s(n: &str, v: f64) -> (String, f64) {
    (n.to_string(), v)
}

/// The poses: (what, env id, arguments, fields written).
fn poses(consts: &BTreeMap<String, f64>, env_shapes: &BTreeMap<String, super::hks::Calls>, pos_y: f64) -> Vec<Pose> {
    let c = |n: &str| consts.get(n).copied().unwrap_or_else(|| n.parse().unwrap_or_else(|_| panic!("const {n}")));
    let script_args = |id: &str| -> Vec<Vec<f64>> {
        env_shapes.get(id).map(|cs| cs.shapes.keys().filter_map(|a| a.iter().map(|x| consts.get(x).copied().or_else(|| x.parse().ok())).collect()).collect()).unwrap_or_default()
    };
    let mut p: Vec<Pose> = Vec::new();
    // 3063 GetVariableChangeValue: the entry with the asked id, another id, none.
    for a in script_args("3063") {
        let id = a[0];
        p.push(("var change: the asked id", 3063, a.clone(), vec![s(&format!("var[{}]", id + 5.0), 9.0), s(&format!("var[{id}]"), 0.75)]));
        p.push(("var change: another id only", 3063, a.clone(), vec![s(&format!("var[{}]", id + 5.0), 9.0)]));
    }
    // 1108 ActionDuration: the held seconds of each arm the scripts ask.
    for a in script_args("1108") {
        let arm = a[0] as u64;
        p.push(("hold time", 1108, a.clone(), vec![s(&format!("request+{:#x}", HOLD + 4 * arm), 0.4167)]));
    }
    p.push(("hold time, arm 32 (last < 0x21)", 1108, vec![32.0], vec![s(&format!("request+{:#x}", HOLD + 4 * 32), 1.25)]));
    // 2000 IsMoveCancelPossible / 349 IsDamageMotionOff: TAE ctrl flags.
    p.push(("move cancel: both bits", 2000, vec![], vec![s("ctrlflags+0x88", 4.0), s("ctrlflags+0x9c", 16.0)]));
    p.push(("move cancel: +0x9c bit 4 alone", 2000, vec![], vec![s("ctrlflags+0x9c", 16.0)]));
    p.push(("move cancel: other bits", 2000, vec![], vec![s("ctrlflags+0x88", 3.0), s("ctrlflags+0x9c", 15.0)]));
    p.push(("damage motion off: +0x88 bit 16", 349, vec![], vec![s("ctrlflags+0x88", 65536.0)]));
    p.push(("damage motion off: +0x88 bit 17", 349, vec![], vec![s("ctrlflags+0x88", 131072.0)]));
    // 105 GetCommandIDFromEvent.
    for a in script_args("105") {
        p.push(("event command", 105, a.clone(), vec![s(&format!("ctrlflags+{:#x}", 0x4f8 + 4 * a[0] as u64), 123.0)]));
    }
    // The hit record (flags module).
    p.push(("damage type", 202, vec![], vec![s("flags+0x5c", 4.0)]));
    p.push(("damage level", 236, vec![], vec![s("flags+0x38", 5.0)]));
    p.push(("special attribute", 285, vec![], vec![s("flags+0x4c", 3.0)]));
    p.push(("received any damage: +0x54 bit 2", 256, vec![], vec![s("flags+0x54", 4.0)]));
    p.push(("received any damage: other bits", 256, vec![], vec![s("flags+0x54", 3.0)]));
    for a in script_args("334") {
        p.push(("behavior id at +0x40", 334, a.clone(), vec![s("flags+0x40", a[0])]));
        p.push(("behavior id at +0x44", 334, a.clone(), vec![s("flags+0x44", a[0])]));
        p.push(("behavior id: another", 334, a.clone(), vec![s("flags+0x40", a[0] + 1.0), s("flags+0x44", a[0] - 1.0)]));
    }
    for a in script_args("339") {
        let bit = a[0] as u32;
        p.push(("anim end: the bit", 339, a.clone(), vec![s("flags+0xaf0", (1u64 << bit) as f64)]));
        p.push(("anim end: the next bit", 339, a.clone(), vec![s("flags+0xaf0", (1u64 << (bit + 1)) as f64)]));
    }
    for (id, off) in [(3018, 0x60), (3019, 0x62), (3056, 0x64), (3057, 0x66)] {
        for v in [12.0, 30001.0, -1.0, -7.0] {
            p.push(("guard behaviour (i16)", id, vec![], vec![s(&format!("flags+{off:#x}"), v)]));
        }
    }
    for dir in 0..10 {
        p.push(("damage direction", 3032, vec![], vec![s("flags+0x68", dir as f64)]));
        p.push(("received damage direction", 222, vec![], vec![s("flags+0x68", dir as f64)]));
    }
    for lvl in 0..13 {
        for g in 0..6 {
            p.push(("guard level action", 237, vec![], vec![s("flags+0x38", lvl as f64), s("flags+0x3c", g as f64)]));
        }
    }
    // Throws.
    for id in [1112, 206, 205, 3031] {
        p.push(("throw entry type 1", id, vec![], vec![s("throw_entry+0x18", 1.0)]));
        p.push(("throw entry type 3, flags +0x70 bit 4", id, vec![], vec![s("throw_entry+0x18", 3.0), s("flags+0x70", 16.0)]));
    }
    p.push(("throw active byte", 3031, vec![], vec![s("throw+0x63", 1.0)]));
    // Modules.
    p.push(("attack direction", 1119, vec![], vec![s("damage+0x60", 2.0)]));
    p.push(("knockback distance 2.7", 1121, vec![], vec![s("knockback+0x10", 2.7)]));
    p.push(("knockback distance -1.5", 1121, vec![], vec![s("knockback+0x10", -1.5)]));
    p.push(("talk ezstate ref", 3054, vec![], vec![s("talk+0x18", 77.0)]));
    p.push(("lip sync", 3058, vec![], vec![s("talk+0x28", 1.0)]));
    p.push(("save state", 3055, vec![], vec![s("chr+0x2008", 3.0)]));
    p.push(("safe position return state", 3061, vec![], vec![s("chr+0x200c", 2.0)]));
    p.push(("map visibility (low nibble)", 3040, vec![], vec![s("chr+0x1acd", 0x35 as f64)]));
    p.push(("weapon switch type", 207, vec![], vec![s("weapons+0x0", 3.0)]));
    p.push(("true death", 3028, vec![], vec![s("life+0x8c", 1.0)]));
    p.push(("true death: bit 1 only", 3028, vec![], vec![s("life+0x8c", 2.0)]));
    p.push(("wall jump contact +0xab4", 3020, vec![], vec![s("phys+0xab4", 1.0)]));
    p.push(("wall jump contact +0xab5", 3020, vec![], vec![s("phys+0xab5", 1.0)]));
    p.push(("crouch exit impossible", 3037, vec![], vec![s("phys+0x93a", 0.0)]));
    p.push(("hang and climb (docking +0x7c0 + 12 x 11)", 3050, vec![], vec![s("docking+0x844", 1.0)]));
    p.push(("obstacle at docking start", 3051, vec![], vec![s("docking+0x84c", 1.0)]));
    p.push(("obstacle at docking end", 3052, vec![], vec![s("docking+0x84d", 1.0)]));
    p.push(("wire target in reach", 3000, vec![], vec![s("wire_target+0x8", 1.0)]));
    p.push(("wire target in reach, wire busy", 3000, vec![], vec![s("wire_target+0x8", 1.0), s("wire+0x10", 1.0), s("wire+0x11", 0.0)]));
    p.push(("wire target in reach, wire busy but +0x11", 3000, vec![], vec![s("wire_target+0x8", 1.0), s("wire+0x10", 1.0), s("wire+0x11", 1.0)]));
    p.push(("wire target value 2", 3000, vec![], vec![s("wire_target+0x8", 2.0)]));
    // In the air: the physics ground flags and vertical speed.
    let air = vec![s("phys+0xa2", 0.0), s("phys+0x929", 0.0), s("phys+0x164", -4.2)];
    for id in [201, 248, 3003, 200] {
        p.push(("in the air", id, vec![], air.clone()));
    }
    p.push(("in the air, rising", 3003, vec![], vec![s("phys+0xa2", 0.0), s("phys+0x929", 0.0), s("phys+0x164", 6.5)]));
    p.push(("landing, +0xa48 set", 248, vec![], vec![s("phys+0xa48", 1.0)]));
    for (what, extra) in [
        ("in the air, phys +0x100 clear", vec![s("phys+0x100", 0.0)]),
        ("in the air, phys +0x100 set, +0x900 = 0.5", vec![s("phys+0x100", 1.0), s("phys+0x900", 0.5)]),
        ("in the air, phys +0x100 set, +0x900 = 1", vec![s("phys+0x100", 1.0), s("phys+0x900", 1.0)]),
        ("in the air, phys +0x100 clear, ctrl flags +0x88 bit 5", vec![s("phys+0x100", 0.0), s("ctrlflags+0x88", 32.0)]),
    ] {
        p.push((what, 200, vec![], [air.clone(), extra].concat()));
    }
    // GetFallHeight: physics +0x854 (the fall's top) over the position's y, x100.
    for h in [3.5, 0.25, 12.0, 70.0] {
        p.push(("fall height", 224, vec![], vec![s("phys+0x854", pos_y + h)]));
    }
    p.push(("fall height, ctrl flags +0x9c bit 1 (10000)", 224, vec![], vec![s("ctrlflags+0x9c", 2.0)]));
    // The 59 m cap: FUN_140a58a70 = can't die (FUN_140bd7d00: data +0x228 bit 2, or DebugFlags +0xb,
    // or the player with DebugFlags +0x0) and not a deadly fall (FUN_140bc2210).
    p.push(("fall height 70, mortal", 224, vec![], vec![s("phys+0x854", pos_y + 70.0), s("debug+0x0", 0.0)]));
    p.push(("fall height 10000, mortal", 224, vec![], vec![s("ctrlflags+0x9c", 2.0), s("debug+0x0", 0.0)]));
    p.push(("fall height 70, data +0x228 bit 2", 224, vec![], vec![s("phys+0x854", pos_y + 70.0), s("debug+0x0", 0.0), s("data+0x228", 4.0)]));
    p.push(("fall height 70, all no death", 224, vec![], vec![s("phys+0x854", pos_y + 70.0), s("debug+0x0", 0.0), s("debug+0xb", 1.0)]));
    p.push(("fall height 70, phys +0x8d0 = 31", 224, vec![], vec![s("phys+0x854", pos_y + 70.0), s("phys+0x8d0", 31.0)]));
    p.push(("fall height 70, phys +0x8d0 = 29", 224, vec![], vec![s("phys+0x854", pos_y + 70.0), s("phys+0x8d0", 29.0)]));
    p.push(("fall height 200", 224, vec![], vec![s("phys+0x854", pos_y + 200.0)]));
    // The env ids Sv1 answered on idle, in other poses.
    p.push(("not in the stay state", 1105, vec![], vec![s("flags+0x10", 0.0)]));
    p.push(("locked on", 1118, vec![], vec![s("chr+0x1070", 1.0)]));
    p.push(("attack connected", 2004, vec![], vec![s("flags+0x58", 16.0)]));
    p.push(("dt 1/30", 333, vec![], vec![s("flags+0xaf8", 1.0 / 30.0)]));
    p.push(("HP 1", 1000, vec![], vec![s("data+0x130", 1.0)]));
    let _ = c;
    p
}

pub fn run(new_emu: &dyn Fn() -> Emu, out: &Path, want: &[&str]) {
    let hks = out.parent().unwrap().join("hks_src");
    let (env_calls, act_calls, consts) = super::hks::survey(&hks);
    if want.iter().any(|w| *w == "hks2" || *w == "hks2_env") {
        env_poses(&mut new_emu(), out, &env_calls, &consts);
    }
    if want.iter().any(|w| *w == "hks2" || *w == "hks2_act") {
        acts(new_emu, out, &act_calls, &consts);
    }
    if want.iter().any(|w| *w == "hks2" || *w == "hks2_move") {
        moves(new_emu, out);
    }
}

fn env_poses(emu: &mut Emu, out: &Path, env_calls: &BTreeMap<String, super::hks::Calls>, consts: &BTreeMap<String, f64>) {
    let o = objs(emu);
    hook_args(emu);
    let ctx = emu.alloc(8);
    emu.u64(ctx, o.0["chr"]);
    let lua = emu.alloc(0x400);
    emu.write(lua, &[0u8; 0x400]);
    // The physics position (FUN_140bbfc00) GetFallHeight subtracts.
    let buf = emu.alloc(0x10);
    emu.call(PHYS_POS, &[o.0["phys"], buf], &[]);
    let pos_y = emu.rf32(buf + 4) as f64;
    emu.peek::<4>(FALL_DEATH_DROP);

    let mut idle = serde_json::Map::new();
    for (n, _) in FIELDS {
        idle.insert(n.to_string(), json!(read_field(emu, &o, n)));
    }
    let poses = poses(consts, env_calls, pos_y);
    // Fields a pose names outside FIELDS (hold times, event commands) get their idle value too.
    for (_, _, _, sets) in &poses {
        for (n, _) in sets {
            if !n.starts_with("var[") && !idle.contains_key(n) {
                idle.insert(n.clone(), json!(read_field(emu, &o, n)));
            }
        }
    }
    let mut cases = Vec::new();
    let mut stopped = 0;
    for (what, id, args, sets) in &poses {
        let (before, _) = env(emu, ctx, lua, *id, args);
        let mut n_var = 0;
        let mut old = Vec::new();
        for (n, v) in sets {
            old.extend(write_field(emu, &o, n, *v, &mut n_var));
        }
        let (ret, st) = env(emu, ctx, lua, *id, args);
        restore(emu, old);
        stopped += st as usize;
        let set: serde_json::Map<String, Value> = sets.iter().map(|(n, v)| (n.clone(), json!(v))).collect();
        cases.push(json!({"what": what, "id": id, "args": args, "set": set, "idle_ret": before, "ret": ret, "stopped": st}));
    }
    let body = json!({
        "function": format!("{:#x}", super::hks::ENV),
        "pos_y": pos_y,
        "fall_death_drop": emu.rf32(FALL_DEATH_DROP),
        "idle": idle,
        "cases": cases,
    });
    std::fs::write(out.join("hks2_env.json"), serde_json::to_string_pretty(&body).unwrap()).unwrap();
    println!("hks2_env: {} poses over {} ids, {} stopped early", cases.len(), poses.iter().map(|p| p.1).collect::<std::collections::BTreeSet<_>>().len(), stopped);
}

/// Regions diffed around an act: (object, length).
const DIFF: &[(&str, u64)] = &[
    ("chr", 0x2100),
    ("flags", 0xb00),
    ("data", 0x200),
    ("talk", 0x40),
    ("phys", 0xc20),
    ("request", 0x200),
    ("throw", 0x100),
    ("damage", 0x100),
    ("knockback", 0x60),
    ("fdp", 0x40),
    ("docking", 0x900),
    ("wire", 0x40),
    ("ctrl", 0x400),
    ("homing", 0x100),
    ("ctrlflags", 0x600),
    ("turn", 0x100),
];

fn dump(emu: &mut Emu, o: &Objs) -> Vec<(String, Vec<u8>)> {
    DIFF.iter()
        .filter(|(n, _)| o.0[n] != 0)
        .map(|(n, len)| {
            let base = o.0[n];
            let mut b = vec![0u8; *len as usize];
            for page in (0..*len).step_by(0x1000) {
                emu.peek::<1>(base + page);
            }
            emu.peek::<1>(base + len - 1);
            unsafe { uc::uc_mem_read(emu.uc, base, b.as_mut_ptr() as _, b.len()) };
            (n.to_string(), b)
        })
        .collect()
}

fn acts(new_emu: &dyn Fn() -> Emu, out: &Path, act_calls: &BTreeMap<String, super::hks::Calls>, consts: &BTreeMap<String, f64>) {
    let mut cases = Vec::new();
    let mut act_names = BTreeMap::new();
    let (mut ran, mut stopped) = (0, 0);
    for (id_text, calls) in act_calls {
        // Each act runs on the snapshot with the fields the acts set cleared first (the snapshot's
        // Wolf already has e.g. bits 0, 28 and 30 of flags +0x10 set), so what an act sets shows in
        // the diff.
        #[rustfmt::skip]
        let clear = vec![
            s("flags+0x10", 0.0), s("flags+0x20", 0.0), s("flags+0x28", 0.0), s("ctrl+0x258", 0.0), s("throw+0x79", 0.0), s("throw+0x7b", 0.0),
            s("knockback+0x44", 0.0), s("fdp+0x11", 1.0), s("docking+0x8a0", 0.0), s("docking+0x8a1", 0.0), s("docking+0x8a2", 0.0),
        ];
        let mut shapes: Vec<(Vec<String>, Option<Vec<f64>>, Vec<(String, f64)>)> = calls
            .shapes
            .keys()
            .map(|a| (a.clone(), a.iter().map(|x| consts.get(x).copied().or_else(|| x.parse().ok())).collect(), clear.clone()))
            .collect();
        // TurnToDirection(deg) is called with script locals ("angle + ..."): run it on a few angles.
        if id_text == "3025" {
            shapes = [30.0, -90.0, 180.0].iter().map(|d| (vec![format!("{d}")], Some(vec![*d]), clear.clone())).collect();
        }
        // The turns again from a physics yaw of 1 rad (the snapshot's Wolf faces yaw 0).
        if id_text == "3025" || id_text == "159" {
            let more: Vec<_> = shapes.iter().map(|(t, a, c)| (t.clone(), a.clone(), [c.clone(), vec![s("phys+0x74", 1.0)]].concat())).collect();
            shapes.extend(more);
        }
        // TurnTowardAttacker reads the damage module's hit direction (+0x50, normalised in place).
        if id_text == "159" {
            let mut more = Vec::new();
            for (x, z) in [(1.0, 0.0), (0.0, 1.0), (0.6, 0.8), (0.0, -1.0), (-3.0, 0.0), (-0.6, -0.8)] {
                for (t, a, c) in &shapes[..2] {
                    let dir = vec![s("phys+0x74", 1.0), s("damage+0x50", x), s("damage+0x54", 0.0), s("damage+0x58", z)];
                    more.push((t.clone(), a.clone(), [c.clone(), dir].concat()));
                }
            }
            shapes.extend(more);
        }
        for (text, args, pre) in shapes {
            let mut emu = new_emu();
            if act_names.is_empty() {
                act_names = super::hks::names(&mut emu, super::hks::ACT_NAMES);
            }
            let id = id_text.parse::<i64>().ok().or_else(|| act_names.get(&super::hks::name_key(id_text)).map(|&v| v as i64));
            let mut case = json!({"id": id, "id_text": id_text, "args_text": text, "sites": calls.shapes.get(&text).copied().unwrap_or(0)});
            let (Some(id), Some(args)) = (id, args) else {
                cases.push(case);
                continue;
            };
            let o = objs(&mut emu);
            hook_args(&mut emu);
            let ctx = emu.alloc(8);
            emu.u64(ctx, o.0["chr"]);
            let lua = emu.alloc(0x400);
            emu.write(lua, &[0u8; 0x400]);
            let mut n_var = 0;
            for (n, v) in &pre {
                write_field(&mut emu, &o, n, *v, &mut n_var);
            }
            let before = dump(&mut emu, &o);
            let fields_before: BTreeMap<&str, f64> = FIELDS.iter().map(|(n, _)| (*n, read_field(&mut emu, &o, n))).collect();
            *ARGS.lock().unwrap() = args.clone();
            let r = emu.call(ACT, &[ctx, id as u64, lua], &[]);
            ran += 1;
            let after = dump(&mut emu, &o);
            // Changed 4-byte words: "<object>+0x<off>": [before, after] as u32 (and as f32).
            let mut diff = serde_json::Map::new();
            for ((n, a), (_, b)) in before.iter().zip(&after) {
                for w in (0..a.len()).step_by(4) {
                    if a[w..w + 4] != b[w..w + 4] {
                        let x = u32::from_le_bytes(a[w..w + 4].try_into().unwrap());
                        let y = u32::from_le_bytes(b[w..w + 4].try_into().unwrap());
                        diff.insert(format!("{n}+{w:#x}"), json!([format!("{x:#x}"), format!("{y:#x}"), f32::from_bits(x), f32::from_bits(y)]));
                    }
                }
            }
            let mut fields = serde_json::Map::new();
            for (n, _) in FIELDS {
                let v = read_field(&mut emu, &o, n);
                if v != fields_before[n] {
                    fields.insert(n.to_string(), json!([fields_before[n], v]));
                }
            }
            // AddSpEffect lands in the SpEffect list (chr +0x11d0): ask GetSpEffectID after it.
            if matches!(id, 2002 | 3004) {
                case["env_after_1116"] = json!(env(&mut emu, ctx, lua, 1116, &args).0);
            }
            case["args"] = json!(args);
            case["pre"] = json!(pre.iter().map(|(n, v)| (n.clone(), json!(v))).collect::<serde_json::Map<_, _>>());
            case["diff"] = Value::Object(diff);
            case["fields"] = Value::Object(fields);
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
            cases.push(case);
        }
    }
    let body = json!({"function": format!("{ACT:#x}"), "cases": cases});
    std::fs::write(out.join("hks2_act.json"), serde_json::to_string_pretty(&body).unwrap()).unwrap();
    println!("hks2_act: {} act ids, {ran} cases run, {stopped} stopped early", act_calls.len());
}

// ---- Movement gaps (`hks2_move`): the free pad manipulator FUN_1407f0f50 in Wolf's PlayerIns
// update (FUN_140a37560, as in turn.rs), with the switches it reads.

/// A debug menu bool (registered with FUN_1425e1aa0, name at 0x1429d4e78): hold the camera yaw the
/// move stick is turned by while the stick is held (manipulator +0x420 / +0x424 / +0x428).
const CAM_HOLD: u64 = 0x1_43d5_b49c;
/// Its re-sample angle (degrees; a debug menu float FUN_1405a7be0, range 0..180).
const CAM_HOLD_DEG: u64 = 0x1_43b0_14b0;
/// The two globals behind ctrl flags +0x88 bit 18 in the manipulator: 0x143d7a34a must be set
/// for the bit to do anything; then 0x143b066e4 set halves the move vector, clear skips the turn.
const WALK_GATE: u64 = 0x1_43d7_a34a;
const WALK_HALF: u64 = 0x1_43b0_66e4;
/// The pad's stick getter (FUN_1409603d0: [pad +0x38f8] +0, forward).
const PAD_FWD: u64 = 0x1_4096_03d0;

static PAD: Mutex<u64> = Mutex::new(0);

extern "C" fn on_pad(uc_: uc::Engine, _addr: u64, _size: u32, _data: *mut c_void) {
    let mut rcx = 0u64;
    unsafe { uc::uc_reg_read(uc_, uc::RCX, &mut rcx as *mut u64 as _) };
    *PAD.lock().unwrap() = rcx;
}

/// A C string or UTF-16 string at `at` (whichever reads as text).
fn text_at(emu: &mut Emu, at: u64) -> String {
    let b: Vec<u8> = (0..96).map(|i| emu.peek::<1>(at + i)[0]).collect();
    if b[1] == 0 {
        let u: Vec<u16> = b.chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]])).take_while(|&c| c != 0).collect();
        String::from_utf16_lossy(&u)
    } else {
        String::from_utf8_lossy(&b.into_iter().take_while(|&c| c != 0).collect::<Vec<_>>()).into_owned()
    }
}

struct MoveScenario {
    name: &'static str,
    frames: usize,
    /// (forward, side) on the pad.
    stick: fn(usize) -> [f32; 2],
    /// The manipulator's camera yaw (+0x94) each frame.
    cam: fn(usize) -> f32,
    /// Global bytes written before every frame.
    globals: &'static [(u64, u8)],
    /// ctrl flags +0x88 bits OR-ed in before every frame.
    ctrl_88: u32,
}

#[rustfmt::skip]
const MOVES: &[MoveScenario] = &[
    // The camera turns 4 deg a frame while the stick is held forward.
    MoveScenario { name: "cam_turn", frames: 40, stick: |_| [1.0, 0.0], cam: |f| (4.0 * f as f32).to_radians(), globals: &[], ctrl_88: 0 },
    MoveScenario { name: "cam_turn_hold", frames: 40, stick: |_| [1.0, 0.0], cam: |f| (4.0 * f as f32).to_radians(), globals: &[(CAM_HOLD, 1)], ctrl_88: 0 },
    MoveScenario { name: "cam_turn_hold_release", frames: 40, stick: |f| if (15..18).contains(&f) { [0.0, 0.0] } else { [1.0, 0.0] }, cam: |f| (4.0 * f as f32).to_radians(), globals: &[(CAM_HOLD, 1)], ctrl_88: 0 },
    // ctrl flags +0x88 bit 18, alone and with the two globals.
    MoveScenario { name: "walk_none", frames: 12, stick: |_| [1.0, 0.0], cam: |_| 0.0, globals: &[], ctrl_88: 0 },
    MoveScenario { name: "walk_bit", frames: 12, stick: |_| [1.0, 0.0], cam: |_| 0.0, globals: &[], ctrl_88: 1 << 18 },
    MoveScenario { name: "walk_bit_gate", frames: 12, stick: |_| [1.0, 0.0], cam: |_| 0.0, globals: &[(WALK_GATE, 1)], ctrl_88: 1 << 18 },
    MoveScenario { name: "walk_bit_gate_half", frames: 12, stick: |_| [1.0, 0.0], cam: |_| 0.0, globals: &[(WALK_GATE, 1), (WALK_HALF, 1)], ctrl_88: 1 << 18 },
];

pub fn moves(new_emu: &dyn Fn() -> Emu, out: &Path) {
    let mut emu = new_emu();
    // What the switches are: their debug menu names and the snapshot's values.
    let mut info = serde_json::Map::new();
    for (k, at) in [("cam_hold_name", 0x1_429d_4e78u64), ("next_bool_name", 0x1_429d_4eb0), ("cam_hold_deg_name", 0x1_429d_4ef0)] {
        info.insert(k.into(), json!(text_at(&mut emu, at)));
    }
    emu.peek::<4>(CAM_HOLD_DEG);
    for (k, at) in [("cam_hold", CAM_HOLD), ("walk_gate", WALK_GATE), ("walk_half", WALK_HALF)] {
        info.insert(k.into(), json!(emu.peek::<1>(at)[0]));
    }
    info.insert("cam_hold_deg".into(), json!(emu.rf32(CAM_HOLD_DEG)));
    // The pad object and its input block.
    emu.hook(uc::HOOK_CODE, on_pad as *const c_void, PAD_FWD, PAD_FWD);
    let w = super::turn::wolf(&mut emu);
    let pad = *PAD.lock().unwrap();
    info.insert("pad".into(), json!(format!("{pad:#x} {}", super::hks::class_name(&mut emu, pad).unwrap_or_default())));
    info.insert("pad_block".into(), json!(format!("{:#x} (pad {:+#x})", w.stick, w.stick as i64 - pad as i64)));
    let blk: Vec<f32> = (0..16).map(|i| f32::from_le_bytes(emu.peek(w.stick + 4 * i))).collect();
    info.insert("pad_block_floats".into(), json!(blk));
    println!("hks2_move: {}", serde_json::to_string(&info).unwrap());

    let mut scenarios = Vec::new();
    for sc in MOVES {
        let mut emu = new_emu();
        let w = super::turn::wolf(&mut emu);
        let ctrl = emu.peek_u64(w.player + 0x50);
        let ctrlflags = emu.peek_u64(ctrl + 0x50);
        let mut frames = Vec::new();
        for f in 0..sc.frames {
            emu.poke_f32s(w.stick, &(sc.stick)(f));
            emu.poke_f32s(w.manip + 0x94, &[(sc.cam)(f)]);
            for (at, v) in sc.globals {
                emu.peek::<1>(*at);
                emu.raw_write(*at, &[*v]);
            }
            if sc.ctrl_88 != 0 {
                let v = u32::from_le_bytes(emu.peek(ctrlflags + 0x88)) | sc.ctrl_88;
                emu.raw_write(ctrlflags + 0x88, &v.to_le_bytes());
            }
            let t = super::turn::fd4_time(&mut emu, 1.0 / 60.0);
            let r = emu.call(super::turn::PLAYER_UPDATE, &[w.player, t], &[]);
            let rd = |emu: &mut Emu, at: u64, n: u64| -> Vec<f32> { (0..n).map(|i| f32::from_le_bytes(emu.peek(at + 4 * i))).collect() };
            let m10 = rd(&mut emu, w.manip + 0x10, 3);
            let mc0 = rd(&mut emu, w.manip + 0xc0, 3);
            let held = rd(&mut emu, w.manip + 0x420, 1)[0];
            let latch = emu.peek::<1>(w.manip + 0x424)[0];
            let resample = rd(&mut emu, w.manip + 0x428, 1)[0];
            let lvl = rd(&mut emu, w.beh + 0xbac, 1)[0];
            let yaw = rd(&mut emu, w.phys + 0x74, 1)[0];
            let c88 = u32::from_le_bytes(emu.peek(ctrlflags + 0x88));
            frames.push(json!({
                "stick": (sc.stick)(f), "cam": (sc.cam)(f), "move": [m10[0], m10[2]], "world": [mc0[0], mc0[2]],
                "held_cam": held, "latch": latch, "resample": resample, "level": lvl, "yaw": yaw,
                "ctrl_88": c88, "stopped": r.stopped.is_some(),
            }));
        }
        scenarios.push(json!({"name": sc.name, "globals": sc.globals.iter().map(|(a, v)| json!([format!("{a:#x}"), v])).collect::<Vec<_>>(), "ctrl_88": sc.ctrl_88, "frames": frames}));
    }
    let body = json!({"function": format!("{:#x}", super::turn::PLAYER_UPDATE), "info": info, "scenarios": scenarios});
    std::fs::write(out.join("hks2_move.json"), serde_json::to_string_pretty(&body).unwrap()).unwrap();
    println!("hks2_move: {} scenarios", MOVES.len());
}
