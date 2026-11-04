//! air: Wolf in the air on the snapshot, whole chr frames FUN_140a4b570 with his pad written per
//! frame (acttrace's way: pad frame +0x67 + input id, +0x3c / +0x3e held, +0x0 / +0x4 the stick).
//! Per frame it records what the physics module ([chr+0x1ff8]+0x68) does in the air:
//! - FUN_140bcc420 (entry): the ChrActionFlag bytes +0x92b / +0x92c (27 SetNoGravity, set by the
//!   TAE flag handler FUN_140b51b70 case 0x1b) / +0x92d before it turns them into +0x92a;
//! - FUN_140bccd50 (entry, the physics step): +0x92a, +0x931, +0xa2 / +0xa3, the air velocity
//!   +0x160, the ground velocity +0xa50, the gravity scale +0x920 and the fall module (+0x1c0):
//!   row +0x10, +0x6c request, +0x6d, the launch timer +0x180, homing +0xd9, +0x1c4;
//! - hknpCharacterProxy::integrate (proxy's hook): the velocity the step asked for;
//! - the TAE update FUN_140b5c920 (Wolf's clips: id, prev, cur, length);
//! - FUN_140b97b30 (fall damage, from FUN_140b97a30) and after the frame the fall module
//!   [bag+0x70] +0x14 / +0x1c / +0x20 (height, damage, factor), HP, the fall top +0x854.
//!
//!   emu-cases <exe> <out> air_probe [fwd side frames] --snapshot <dir>   prints a stick's walk
//!   emu-cases <exe> <out> air [scenario...] --snapshot <dir>  -> air_frames.json (air_<name>.json)
//!   emu-cases <exe> <out> air_drops [h...] --snapshot <dir>   -> air_drops.json (fall damage)
//!   emu-cases <exe> <out> air_falldmg --snapshot <dir>  -> air_falldmg.json (the fall-damage
//!     rules called directly: SpEffects, landing materials, HP left, the void fall)
//!
//! gap: no enemy stands under Wolf (the head kick's relaunch is not run); an air art never starts
//! from the jump (the ground art ran instead); the SpEffect type 0x155 and the materials are
//! varied in air_falldmg's direct calls only, not in whole-frame drops; NoGravity is forced on a
//! vertical jump only.

use std::collections::BTreeMap;
use std::ffi::c_void;
use std::path::Path;
use std::sync::Mutex;

use serde_json::{json, Value};

use super::{proxy, uc, Emu};

const WORLD_CHR_MAN: u64 = 0x1_43d7_a1e0;
const CHR_FRAME: u64 = 0x1_40a4_b570;
const ARM_INPUT: u64 = 0x1_40b2_58d0;
const MENU_STATE: u64 = 0x1_408c_58e0;
const TAE_UPDATE: u64 = 0x1_40b5_c920;
/// Physics module: the ChrActionFlag gravity bytes -> +0x92a (FUN_140bcc420), the step (FUN_140bccd50).
const NO_GRAVITY: u64 = 0x1_40bc_c420;
const STEP: u64 = 0x1_40bc_cd50;
/// The fall module's air step (+0x1c0: FUN_140bac120) and its sibling FUN_140bacc10.
const FALL_STEP: u64 = 0x1_40ba_c120;
const FALL_PRE: u64 = 0x1_40ba_cc10;
/// Landing: FUN_140b97a30 (bag +0x70 module) -> FUN_140b97b30 (the fall damage).
const LAND_CHECK: u64 = 0x1_40b9_7a30;
const FALL_DAMAGE: u64 = 0x1_40b9_7b30;
/// FUN_1409efea0(chr): the module whose +0xc0 is the world move stick the air control reads.
const MOVE_MODULE: u64 = 0x1_409e_fea0;
const DT: f32 = 1.0 / 60.0;

const ATTACK: u32 = 0;
const GUARD: u32 = 2;
const JUMP: u32 = 4;

/// The physics module watched by the hooks, and the frame's log.
static PHYS: Mutex<u64> = Mutex::new(0);
static LOG: Mutex<Vec<Value>> = Mutex::new(Vec::new());

/// The vertical jump's launch clip a000_201011 (TAE 920 row 100 at 0).
const JUMP_START: u32 = 201011;

/// SetNoGravity forced in the emulator copy (ChrActionFlag 27's byte +0x92c written at
/// FUN_140bcc420's entry, as the TAE flag handler would have set it that frame), relative to the
/// frame the jump's launch clip had its first TAE window (`launch`).
struct Ng {
    mode: u8,
    frame: usize,
    launch: Option<usize>,
}
impl Ng {
    fn forced(&self) -> bool {
        match (self.mode, self.launch) {
            // Up to and including the launch's frame: the head kick's / wire's [0, s] flag with the
            // 920 at s (a000_213110, a000_2020x0), the flag's last window and the 920 together.
            (1, None) => self.frame >= 2,
            (1, Some(l)) => self.frame == l,
            // Mid-flight: 11 frames in the rise.
            (2, Some(l)) => (l + 6..=l + 16).contains(&self.frame),
            // The frames right after the launch (the fall module's launch timer +0x180 running).
            (3, Some(l)) => (l + 1..=l + 3).contains(&self.frame),
            // Mid-fall, then let go.
            (4, Some(l)) => (l + 30..=l + 38).contains(&self.frame),
            _ => false,
        }
    }
}
static NG: Mutex<Ng> = Mutex::new(Ng { mode: 0, frame: 0, launch: None });

fn rd<const N: usize>(e: uc::Engine, at: u64) -> [u8; N] {
    let mut b = [0u8; N];
    unsafe { uc::uc_mem_read(e, at, b.as_mut_ptr() as _, N) };
    b
}
fn rf(e: uc::Engine, at: u64, n: usize) -> Vec<f32> {
    (0..n).map(|i| f32::from_le_bytes(rd(e, at + 4 * i as u64))).collect()
}
fn reg(e: uc::Engine, r: i32) -> u64 {
    let mut v = 0u64;
    unsafe { uc::uc_reg_read(e, r, &mut v as *mut u64 as _) };
    v
}

extern "C" fn on_hook(e: uc::Engine, addr: u64, _size: u32, _d: *mut c_void) {
    let rcx = reg(e, uc::RCX);
    let phys = *PHYS.lock().unwrap();
    let rec = match addr {
        TAE_UPDATE => {
            let rsp = reg(e, uc::RSP);
            let chr = u64::from_le_bytes(rd(e, rsp + 0x28));
            let mut x = [[0u8; 16]; 4];
            for (i, r) in x.iter_mut().enumerate() {
                unsafe { uc::uc_reg_read(e, uc::XMM0 + i as i32, r.as_mut_ptr() as _) };
            }
            let f = |i: usize| f32::from_le_bytes(x[i][..4].try_into().unwrap());
            let mut ng = NG.lock().unwrap();
            if rcx as u32 == JUMP_START && f(1) == 0.0 && ng.launch.is_none() {
                ng.launch = Some(ng.frame);
            }
            json!({"at": "tae", "id": rcx as u32 as i32, "prev": f(1), "cur": f(2), "len": f(3), "chr": format!("{chr:#x}")})
        }
        NO_GRAVITY if rcx == phys => {
            let before = rd::<6>(e, rcx + 0x92a);
            let forced = NG.lock().unwrap().forced();
            if forced {
                unsafe { uc::uc_mem_write(e, rcx + 0x92c, [1u8].as_ptr() as _, 1) };
            }
            json!({"at": "nograv", "b": before, "forced": forced})
        }
        STEP if rcx == phys => {
            let fall = rcx + 0x1c0;
            json!({"at": "step", "b92a": rd::<1>(e, rcx + 0x92a)[0], "b931": rd::<1>(e, rcx + 0x931)[0],
                "a2": rd::<2>(e, rcx + 0xa2), "b100": rd::<1>(e, rcx + 0x100)[0], "bf3": rd::<1>(e, rcx + 0xbf3)[0],
                "vel": rf(e, rcx + 0x160, 3), "gvel": rf(e, rcx + 0xa50, 3), "gscale": rf(e, rcx + 0x920, 1)[0],
                "pos": rf(e, rcx + 0x80, 3), "prev_pos": rf(e, rcx + 0x90, 3), "dt": rf(e, reg(e, uc::R9) + 8, 1)[0],
                "fall_row": i32::from_le_bytes(rd(e, fall + 0x10)), "fall_68": i32::from_le_bytes(rd(e, fall + 0x68)),
                "fall_6c": rd::<2>(e, fall + 0x6c), "fall_timer": rf(e, fall + 0x180, 1)[0], "fall_d9": rd::<1>(e, fall + 0xd9)[0],
                "fall_1c4": rf(e, fall + 0x1c4, 4), "fall_table": rf(e, fall + 0x78, 4),
                "root": rf(e, rcx + 0x860, 4), "add": rf(e, rcx + 0x880, 4)})
        }
        FALL_STEP | FALL_PRE if rcx == phys + 0x1c0 => {
            // (fall, out, in, FD4Time): the velocity going in.
            let r8 = reg(e, uc::R8);
            json!({"at": if addr == FALL_STEP { "fall_step" } else { "fall_pre" }, "in": rf(e, r8, 4)})
        }
        LAND_CHECK | FALL_DAMAGE => json!({"at": if addr == LAND_CHECK { "land_check" } else { "fall_damage" }}),
        _ => return,
    };
    LOG.lock().unwrap().push(rec);
}

/// Pad state of one frame: arms held, move stick (forward, side).
#[derive(Clone, Copy)]
struct Input {
    arms: &'static [u32],
    stick: [f32; 2],
}
const NONE: Input = Input { arms: &[], stick: [0.0, 0.0] };

fn arm_inputs(emu: &mut Emu) -> BTreeMap<u32, i32> {
    (0..33u32).map(|arm| (arm, emu.call(ARM_INPUT, &[arm as u64], &[]).rax as u32 as i32)).collect()
}

/// As acttrace: every mapped input byte, the held bytes, the stick.
fn press(emu: &mut Emu, frame: u64, ids: &BTreeMap<u32, i32>, inp: &Input) {
    for id in ids.values().filter(|&&i| (0..0x80).contains(&i)) {
        emu.raw_write(frame + 0x67 + *id as u64, &[0]);
    }
    for a in inp.arms {
        let id = ids[a];
        if id >= 0 {
            emu.raw_write(frame + 0x67 + id as u64, &[1]);
        }
    }
    emu.raw_write(frame + 0x3c, &[inp.arms.contains(&ATTACK) as u8]);
    emu.raw_write(frame + 0x3e, &[inp.arms.contains(&GUARD) as u8]);
    emu.poke_f32s(frame, &inp.stick);
}

struct Rig {
    emu: Emu,
    w: super::turn::Wolf,
    wrap: u64,
    wcm: u64,
    ids: BTreeMap<u32, i32>,
    bag: u64,
}

/// proxy::setup (ISA 1, plain longjmp, AI off, the integrate hook) plus acttrace's menu patches
/// and this module's hooks.
fn rig(new_emu: &dyn Fn() -> Emu) -> Rig {
    let (mut emu, w, _) = proxy::setup(new_emu);
    emu.limit = 500_000_000;
    emu.raw_write(MENU_STATE, &[0x31, 0xc0, 0xc3]);
    let menus = emu.call(0x1_40d8_72c0, &[], &[]).rax;
    if menus != 0 {
        emu.u64(menus + 0x100, 0);
    }
    *PHYS.lock().unwrap() = w.phys;
    for a in [TAE_UPDATE, NO_GRAVITY, STEP, FALL_STEP, FALL_PRE, LAND_CHECK, FALL_DAMAGE] {
        emu.hook(uc::HOOK_CODE, on_hook as *const c_void, a, a);
    }
    let ids = arm_inputs(&mut emu);
    let (wrap, _) = proxy::proxy_of(&mut emu, w.phys);
    let wcm = emu.peek_u64(WORLD_CHR_MAN);
    let bag = emu.peek_u64(w.player + 0x1ff8);
    Rig { emu, w, wrap, wcm, ids, bag }
}

impl Rig {
    fn hp(&mut self) -> [i32; 2] {
        let data = self.emu.peek_u64(self.bag + 0x18);
        [i32::from_le_bytes(self.emu.peek(data + 0x130)), i32::from_le_bytes(self.emu.peek(data + 0x134))]
    }
    fn anim(&mut self) -> i32 {
        let m = self.emu.peek_u64(self.bag + 0x10);
        i32::from_le_bytes(self.emu.peek(m + 0xf8))
    }
    /// One frame with `inp`; the frame's record.
    fn frame(&mut self, inp: &Input, dt: f32) -> Result<Value, String> {
        press(&mut self.emu, self.w.stick, &self.ids, inp);
        LOG.lock().unwrap().clear();
        proxy::INT.lock().unwrap().calls.clear();
        proxy::flush_tb(&self.emu);
        let r = self.emu.call(CHR_FRAME, &[self.wcm, 0, self.w.player], &[0.0, dt]);
        if let Some((a, e)) = r.stopped {
            return Err(format!("{a:#x} error {e}"));
        }
        let log = std::mem::take(&mut *LOG.lock().unwrap());
        let calls = std::mem::take(&mut proxy::INT.lock().unwrap().calls);
        let ph = self.w.phys;
        let fm = self.emu.peek_u64(self.bag + 0x70);
        // The world stick the fall module's air control reads (FUN_1409efea0(chr) +0xc0) and the
        // accel it last applied (fall +0x5a0 = stick accel x stick).
        let m = self.emu.call(MOVE_MODULE, &[self.w.player], &[]).rax;
        let world_stick = if m != 0 { proxy::f32s(&mut self.emu, m + 0xc0, 4) } else { Vec::new() };
        Ok(json!({
            "stick": inp.stick, "world_stick": world_stick,
            "stick_acc": proxy::f32s(&mut self.emu, ph + 0x1c0 + 0x5a0, 3), "arms": inp.arms,
            "log": log,
            "integrate": calls.iter().map(|(v, p)| json!({"vel": &v[..3], "pos": &p[..3]})).collect::<Vec<_>>(),
            "pos": proxy::f32s(&mut self.emu, ph + 0x80, 3),
            "yaw": proxy::f32s(&mut self.emu, ph + 0x74, 1)[0],
            "vel": proxy::f32s(&mut self.emu, ph + 0x160, 3),
            "vel170": proxy::f32s(&mut self.emu, ph + 0x170, 3),
            "gvel": proxy::f32s(&mut self.emu, ph + 0xa50, 3),
            "top": proxy::f32s(&mut self.emu, ph + 0x854, 1)[0],
            "a2": self.emu.peek::<2>(ph + 0xa2),
            "b92a": self.emu.peek::<6>(ph + 0x92a),
            "fall_timer": proxy::f32s(&mut self.emu, ph + 0x1c0 + 0x180, 1)[0],
            "fall_row": i32::from_le_bytes(self.emu.peek(ph + 0x1c0 + 0x10)),
            "fall_type": self.emu.peek::<1>(ph + 0x1c0 + 0x6d)[0],
            "land": {"b24": self.emu.peek::<2>(fm + 0x24), "kind": i32::from_le_bytes(self.emu.peek(fm + 0x14)), "damage": i32::from_le_bytes(self.emu.peek(fm + 0x1c)), "factor": f32::from_le_bytes(self.emu.peek(fm + 0x20))},
            "hp": self.hp(),
            "anim": self.anim(),
        }))
    }
}

/// `air_probe fwd side frames`: Wolf's walk with one stick (which way the camera turns it).
fn probe(new_emu: &dyn Fn() -> Emu, args: &[&str]) {
    let v: Vec<f32> = args.iter().filter_map(|a| a.parse().ok()).collect();
    let (fwd, side, n) = (v.first().copied().unwrap_or(1.0), v.get(1).copied().unwrap_or(0.0), v.get(2).copied().unwrap_or(30.0) as usize);
    let mut r = rig(new_emu);
    for f in 0..n {
        let fr = r.frame(&Input { arms: &[], stick: [fwd, side] }, DT).unwrap();
        println!("f{f} anim {} pos {:?} yaw {} a2 {:?} vel {:?}", fr["anim"], fr["pos"], fr["yaw"], fr["a2"], fr["vel"]);
    }
}

/// One air scenario: frames and the pad per frame.
struct Scenario {
    name: &'static str,
    frames: usize,
    input: fn(usize) -> Input,
    /// Ng mode (0: the game's own flags only).
    nograv: u8,
}

fn jump(f: usize) -> &'static [u32] {
    if f < 3 { &[JUMP] } else { &[] }
}

fn scenarios() -> Vec<Scenario> {
    let s = |name, frames, input| Scenario { name, frames, input, nograv: 0 };
    let ng = |name, nograv| Scenario { name, frames: 75, input: |f| Input { arms: jump(f), stick: [0.0, 0.0] }, nograv };
    vec![
        // Vertical jump: a000_201011 (TAE 920 row 100), no stick.
        s("jump", 75, |f| Input { arms: jump(f), stick: [0.0, 0.0] }),
        // Stick forward all along: the forward jump and the air stick along the launch.
        s("jump_fwd", 75, |f| Input { arms: jump(f), stick: [1.0, 0.0] }),
        // A vertical jump steered sideways from frame 12 (in the air), and half a stick.
        s("jump_side", 75, |f| Input { arms: jump(f), stick: if f >= 12 { [0.0, 1.0] } else { [0.0, 0.0] } }),
        s("jump_half", 75, |f| Input { arms: jump(f), stick: if f >= 12 { [0.5, 0.0] } else { [0.0, 0.0] } }),
        // Forward jump, then the stick pulled back (braking the launch).
        s("jump_fwd_back", 75, |f| Input { arms: jump(f), stick: if f < 10 { [1.0, 0.0] } else { [-1.0, 0.0] } }),
        // Run 30 frames, then jump with the stick held (the run jump), then across.
        s("run_jump", 110, |f| Input { arms: if (30..33).contains(&f) { &[JUMP] } else { &[] }, stick: if f < 55 { [1.0, 0.0] } else { [0.0, -1.0] } }),
        // The equipped combat art (attack + guard): a107_3167xx, its TAE 920 launches and
        // SetNoGravity windows.
        s("art", 130, |f| Input { arms: if f < 3 { &[ATTACK, GUARD] } else { &[] }, stick: [0.0, 0.0] }),
        s("art_fwd", 130, |f| Input { arms: if f < 3 { &[ATTACK, GUARD] } else { &[] }, stick: [1.0, 0.0] }),
        // A jump, then the art pressed in the air (the air version), then a kick press.
        s("jump_art", 120, |f| Input { arms: if f < 3 { &[JUMP] } else if (14..17).contains(&f) { &[ATTACK, GUARD] } else { &[] }, stick: [0.0, 0.0] }),
        s("jump_kick", 90, |f| Input { arms: if f < 3 || (16..19).contains(&f) { &[JUMP] } else { &[] }, stick: [0.0, 0.0] }),
        // A jump with an air slash (attack in the air) and its landing.
        s("jump_slash", 100, |f| Input { arms: if f < 3 { &[JUMP] } else if (14..17).contains(&f) { &[ATTACK] } else { &[] }, stick: [0.0, 0.0] }),
        // The vertical jump with SetNoGravity forced (Ng modes): held up to the launch's frame,
        // mid-rise, right after the launch, mid-fall.
        ng("ng_launch", 1),
        ng("ng_rise", 2),
        ng("ng_timer", 3),
        ng("ng_fall", 4),
    ]
}

fn run_scenarios(new_emu: &dyn Fn() -> Emu, out: &Path, only: &[&str], edge: Option<[f32; 2]>) {
    let mut done = Vec::new();
    let mut list = scenarios();
    // The walk off the arena's cliff: the stick that heads -Z from the spot (air_probe), held
    // until airborne, then let go / held on.
    let edge_stick = edge.unwrap_or([1.0, 0.0]);
    static EDGE: Mutex<[f32; 2]> = Mutex::new([1.0, 0.0]);
    *EDGE.lock().unwrap() = edge_stick;
    list.push(Scenario { name: "edge_walk", frames: 120, input: |_| Input { arms: &[], stick: { let s = *EDGE.lock().unwrap(); [s[0] * 0.45, s[1] * 0.45] } }, nograv: 0 });
    list.push(Scenario { name: "edge_run", frames: 100, input: |_| Input { arms: &[], stick: *EDGE.lock().unwrap() }, nograv: 0 });
    for sc in list {
        if !only.is_empty() && !only.contains(&sc.name) {
            continue;
        }
        let t = std::time::Instant::now();
        let mut r = rig(new_emu);
        let start = proxy::f32s(&mut r.emu, r.w.phys + 0x80, 3);
        let mut frames = Vec::new();
        let mut stopped = None;
        *NG.lock().unwrap() = Ng { mode: sc.nograv, frame: 0, launch: None };
        for f in 0..sc.frames {
            NG.lock().unwrap().frame = f;
            match r.frame(&(sc.input)(f), DT) {
                Ok(v) => frames.push(v),
                Err(e) => {
                    stopped = Some(e);
                    break;
                }
            }
        }
        let anims: Vec<i64> = frames.iter().map(|v| v["anim"].as_i64().unwrap()).collect();
        let mut seen: Vec<i64> = Vec::new();
        for a in &anims {
            if seen.last() != Some(a) {
                seen.push(*a);
            }
        }
        println!("{}: {} frames {:.0} s, anims {seen:?}, end {:?} stopped {stopped:?}", sc.name, frames.len(), t.elapsed().as_secs_f32(), frames.last().map(|v| v["pos"].clone()));
        let launch = NG.lock().unwrap().launch;
        done.push(json!({"name": sc.name, "dt": DT, "start": start, "nograv": sc.nograv, "launch": launch, "frames": frames, "stopped": stopped}));
    }
    // A part run is named by its first scenario (the test reads every air_*.json with scenarios).
    let name = match only.first() {
        Some(n) => format!("air_{n}"),
        None => "air_frames".into(),
    };
    let body = json!({"function": format!("{CHR_FRAME:#x} -> {STEP:#x}"), "scenarios": done});
    std::fs::write(out.join(format!("{name}.json")), serde_json::to_string(&body).unwrap()).unwrap();
    println!("  -> {name}.json");
}

/// Falls: Wolf put `drop` m over his spot (idle), the fall top +0x854 set `height` m over the
/// spot's ground, frames until he has landed and settled. `mortal` clears the snapshot's debug
/// no-death flags (DebugFlags 0x143d7a366 +0x0 and +0xb) in the emulator copy.
fn drops(new_emu: &dyn Fn() -> Emu, out: &Path, args: &[&str]) {
    let mut cases: Vec<(f32, f32, bool)> = Vec::new();
    for h in [1.0f32, 2.0, 2.9, 3.0, 4.9, 5.0, 10.0, 15.0, 19.9, 20.0, 22.0, 25.0, 30.0, 35.0, 39.9, 40.0, 45.0, 60.0, 80.0] {
        cases.push((1.2, h, false));
    }
    for h in [10.0f32, 25.0, 39.9, 40.0, 45.0] {
        cases.push((1.2, h, true));
    }
    let want: Vec<f32> = args.iter().filter_map(|a| a.parse().ok()).collect();
    let mut done = Vec::new();
    for (drop, height, mortal) in cases {
        if !want.is_empty() && !want.contains(&height) {
            continue;
        }
        let t = std::time::Instant::now();
        let mut r = rig(new_emu);
        if mortal {
            r.emu.raw_write(0x1_43d7_a366, &[0]);
            r.emu.raw_write(0x1_43d7_a371, &[0]);
        }
        let spot = proxy::f32s(&mut r.emu, r.w.phys + 0x80, 3);
        // Two idle frames first, so the fall top is the ground's.
        for _ in 0..2 {
            r.frame(&NONE, DT).unwrap();
        }
        let ground = proxy::f32s(&mut r.emu, r.w.phys + 0x854, 1)[0];
        let at = [spot[0], spot[1] + drop, spot[2]];
        proxy::place(&mut r.emu, r.w.phys, r.wrap, at);
        let hp0 = r.hp();
        let mut frames = Vec::new();
        let mut landed = None;
        let mut stopped = None;
        for f in 0..400 {
            // The fall top: the placed frame counts as ground (+0x854 = the placed y), so it is
            // written before every airborne frame up to the landing (the physics leaves it alone
            // in the air).
            if landed.is_none() {
                r.emu.poke_f32s(r.w.phys + 0x854, &[ground + height]);
            }
            match r.frame(&NONE, DT) {
                Ok(v) => {
                    if landed.is_none() && v["a2"][0].as_u64() == Some(1) && f > 0 {
                        landed = Some(f);
                    }
                    frames.push(v);
                }
                Err(e) => {
                    stopped = Some(e);
                    break;
                }
            }
            if landed.is_some_and(|l| f >= l + 20) {
                break;
            }
        }
        let anims: Vec<i64> = frames.iter().map(|v| v["anim"].as_i64().unwrap()).collect();
        let mut seen: Vec<i64> = Vec::new();
        for a in &anims {
            if seen.last() != Some(a) {
                seen.push(*a);
            }
        }
        let hp1 = r.hp();
        println!("drop {drop} height {height} mortal {mortal}: landed f{landed:?} hp {hp0:?} -> {hp1:?} anims {seen:?} ({:.0} s) stopped {stopped:?}", t.elapsed().as_secs_f32());
        done.push(json!({"drop": drop, "height": height, "mortal": mortal, "ground": ground, "spot": spot, "hp0": hp0, "hp1": hp1, "landed": landed, "frames": frames, "stopped": stopped}));
    }
    let body = json!({"function": format!("{CHR_FRAME:#x} -> {LAND_CHECK:#x} -> {FALL_DAMAGE:#x}"), "drops": done});
    let name = if want.is_empty() { "air_drops".to_string() } else { format!("air_drops_{}", want.iter().map(|h| h.to_string()).collect::<Vec<_>>().join("_")) };
    std::fs::write(out.join(format!("{name}.json")), serde_json::to_string(&body).unwrap()).unwrap();
    println!("  -> {name}.json");
}

/// The fall-damage module's own entries, called on the landing module (bag +0x70):
/// FUN_140b98910 (module, h) -> FUN_140843660 the damage factor, FUN_140b98ab0 (module, h) ->
/// FUN_14084e5d0 the kill test, FUN_140b975f0 (module, b) the fall-return death (the void fall).
const FALL_FACTOR: u64 = 0x1_40b9_8910;
const FALL_KILL: u64 = 0x1_40b9_8ab0;
const FALL_VOID: u64 = 0x1_40b9_75f0;
/// The chr's death (FUN_14117b210) and the fall's damage reaction (FUN_14117ae70).
const CHR_DIE: u64 = 0x1_4117_b210;
const CHR_FALL_HURT: u64 = 0x1_4117_ae70;
static CALLED: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());
extern "C" fn on_called(_e: uc::Engine, addr: u64, _size: u32, _d: *mut c_void) {
    CALLED.lock().unwrap().push(if addr == CHR_DIE { "die" } else { "hurt" });
}

/// `air_falldmg`: the fall damage rules called directly on the snapshot's Wolf (no frames): per
/// SpEffect set (none; 9620 aging = stateInfo 0x155; 640 and 4050 = stateInfo 47, 4050 with
/// SpEffectParam fallDamageRate 0) and landing material (the snapshot's, HitMtrlParam 22 / 36 /
/// 40), the factor and the kill test over heights, then the landing FUN_140b97b30 with the fall top
/// +0x854 set h over Wolf and HP full / at the return cost / low, and the void fall FUN_140b975f0.
fn falldmg(new_emu: &dyn Fn() -> Emu, out: &Path) {
    let heights = [4.9f32, 5.0, 6.0, 10.0, 19.9, 20.0, 25.0, 30.0, 39.9, 40.0, 45.0, 60.0, 79.9, 80.0, 100.0, 120.0];
    let mut sets = Vec::new();
    // The first set keeps the snapshot's debug no-death flags (DebugFlags 0x143d7a366 +0x0 / +0xb /
    // +0xc: HP stops at 1, the void fall costs nothing); the others clear them in the emulator copy.
    for (mortal, sp) in [(false, &[][..]), (true, &[][..]), (true, &[9620u32][..]), (true, &[640][..]), (true, &[4050][..])] {
        let t = std::time::Instant::now();
        let mut r = rig(new_emu);
        if mortal {
            for at in [0x1_43d7_a366u64, 0x1_43d7_a371, 0x1_43d7_a372] {
                r.emu.raw_write(at, &[0]);
            }
        }
        for a in [CHR_DIE, CHR_FALL_HURT] {
            r.emu.hook(uc::HOOK_CODE, on_called as *const c_void, a, a);
        }
        let player = r.w.player;
        if !sp.is_empty() {
            let list = r.emu.peek_u64(player + 0x11d0);
            let head = r.emu.peek_u64(list + 8);
            let (hid, hrow) = (u32::from_le_bytes(r.emu.peek(head + 0x58)), r.emu.peek_u64(head + 0x60));
            let mut file = super::guard::ParamFile::from_row(&mut r.emu, hrow, hid);
            for id in sp {
                super::guard::add_sp(&mut r.emu, &mut file, player, *id);
            }
        }
        let module = r.emu.peek_u64(r.bag + 0x70);
        let mtrl_at = r.emu.peek_u64(r.bag + 0xa0) + 0x10;
        let phys = r.w.phys;
        let data = r.emu.peek_u64(r.bag + 0x18);
        let (hp, hp_max) = (r.hp()[0], r.hp()[1]);
        // The game data FUN_1408433f0 reads (chr vtable +0x1b8): +0x8fc, +0x25c, +0x264.
        let vt = r.emu.peek_u64(player);
        let gd_fn = r.emu.peek_u64(vt + 0x1b8);
        let gd = r.emu.call(gd_fn, &[player], &[]).rax;
        let game_data = if gd != 0 {
            json!({"f8fc": proxy::f32s(&mut r.emu, gd + 0x8fc, 1)[0], "i25c": i32::from_le_bytes(r.emu.peek(gd + 0x25c)), "i264": i32::from_le_bytes(r.emu.peek(gd + 0x264))})
        } else {
            Value::Null
        };
        let snap_mtrl = u16::from_le_bytes(r.emu.peek(mtrl_at));
        let snap_mtrl_phys = u16::from_le_bytes(r.emu.peek(phys + 0x940));
        let y = proxy::f32s(&mut r.emu, phys + 0x80, 3)[1];
        let mut mats = Vec::new();
        for mtrl in [None, Some(22u16), Some(36), Some(40)] {
            let m = mtrl.unwrap_or(snap_mtrl);
            r.emu.raw_write(mtrl_at, &m.to_le_bytes());
            r.emu.raw_write(phys + 0x940, &mtrl.unwrap_or(snap_mtrl_phys).to_le_bytes());
            let mut rows = Vec::new();
            for &h in &heights {
                let f = r.emu.call(FALL_FACTOR, &[module, 0], &[0.0, h]);
                let k = r.emu.call(FALL_KILL, &[module, 0], &[0.0, h]);
                rows.push(json!({"h": h, "factor": f.xmm0, "kill": k.rax as u8, "stopped": f.stopped.or(k.stopped).map(|s| format!("{:#x} {}", s.0, s.1))}));
            }
            // The landing: HP full, just over the return cost, at it, low.
            let cost = (hp_max as f32 * 0.4) as i32;
            let mut lands = Vec::new();
            for &h in &[4.9f32, 5.0, 6.0, 19.9, 20.0, 25.0, 39.9, 40.0, 45.0, 79.9, 80.0] {
                for hp0 in [hp_max, cost + 1, cost, 100] {
                    r.emu.raw_write(data + 0x130, &hp0.to_le_bytes());
                    r.emu.raw_write(module + 0x25, &[0]);
                    r.emu.raw_write(module + 0x1c, &0i32.to_le_bytes());
                    let main = r.emu.peek_u64(r.bag);
                    r.emu.raw_write(main + 0x5c, &0u32.to_le_bytes());
                    r.emu.poke_f32s(phys + 0x854, &[y + h]);
                    CALLED.lock().unwrap().clear();
                    let run = r.emu.call(FALL_DAMAGE, &[module], &[]);
                    let hp1 = r.hp()[0];
                    lands.push(json!({"h": h, "hp0": hp0, "hp1": hp1, "type": u32::from_le_bytes(r.emu.peek(main + 0x5c)),
                        "b25": r.emu.peek::<1>(module + 0x25)[0], "damage": i32::from_le_bytes(r.emu.peek(module + 0x1c)),
                        "factor": f32::from_le_bytes(r.emu.peek(module + 0x20)), "called": CALLED.lock().unwrap().clone(),
                        "stopped": run.stopped.map(|s| format!("{:#x} {}", s.0, s.1))}));
                }
            }
            r.emu.poke_f32s(phys + 0x854, &[y]);
            mats.push(json!({"material": m, "factors": rows, "lands": lands}));
        }
        r.emu.raw_write(mtrl_at, &snap_mtrl.to_le_bytes());
        r.emu.raw_write(phys + 0x940, &snap_mtrl_phys.to_le_bytes());
        // The void fall (fall-return death) with both callers' flag.
        let cost = (hp_max as f32 * 0.4) as i32;
        let mut voids = Vec::new();
        for b in [0u64, 1] {
            for hp0 in [hp_max, cost + 1, cost, 100] {
                r.emu.raw_write(data + 0x130, &hp0.to_le_bytes());
                r.emu.raw_write(module + 0x25, &[0]);
                let main = r.emu.peek_u64(r.bag);
                r.emu.raw_write(main + 0x5c, &0u32.to_le_bytes());
                CALLED.lock().unwrap().clear();
                let run = r.emu.call(FALL_VOID, &[module, b], &[]);
                voids.push(json!({"flag": b, "hp0": hp0, "hp1": r.hp()[0], "ret": run.rax as u8, "type": u32::from_le_bytes(r.emu.peek(main + 0x5c)),
                    "b25": r.emu.peek::<1>(module + 0x25)[0], "called": CALLED.lock().unwrap().clone(),
                    "stopped": run.stopped.map(|s| format!("{:#x} {}", s.0, s.1))}));
            }
        }
        r.emu.raw_write(data + 0x130, &hp.to_le_bytes());
        println!("mortal {mortal} sp {sp:?}: material {snap_mtrl}/{snap_mtrl_phys}, game data {game_data}, {:.0} s", t.elapsed().as_secs_f32());
        sets.push(json!({"mortal": mortal, "sp": sp, "hp": hp, "hp_max": hp_max, "game_data": game_data, "snap_material": [snap_mtrl, snap_mtrl_phys], "materials": mats, "voids": voids}));
    }
    let body = json!({"function": format!("{FALL_FACTOR:#x} {FALL_KILL:#x} {FALL_DAMAGE:#x} {FALL_VOID:#x}"), "sets": sets});
    std::fs::write(out.join("air_falldmg.json"), serde_json::to_string(&body).unwrap()).unwrap();
    println!("  -> air_falldmg.json");
}

pub(super) fn run(new_emu: &dyn Fn() -> Emu, out: &Path, args: &[&str]) {
    match args.first() {
        Some(&"air_probe") => probe(new_emu, &args[1..]),
        Some(&"air_falldmg") => falldmg(new_emu, out),
        Some(&"air_drops") => drops(new_emu, out, &args[1..]),
        _ => {
            // `air [names] [edge=fwd,side]`
            let edge = args.iter().find_map(|a| a.strip_prefix("edge=")).map(|s| {
                let v: Vec<f32> = s.split(',').map(|x| x.parse().unwrap()).collect();
                [v[0], v[1]]
            });
            let names: Vec<&str> = args[1..].iter().copied().filter(|a| !a.starts_with("edge=")).collect();
            run_scenarios(new_emu, out, &names, edge)
        }
    }
}
