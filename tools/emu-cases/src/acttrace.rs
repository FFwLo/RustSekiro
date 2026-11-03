//! `acttrace`: which functions whole character frames FUN_140a4b570 enter while Wolf (and the
//! General) is mid-action, to extend docs/kb/frame-coverage.md (made from one idle frame) to the
//! actions. Each scenario runs from the snapshot, Wolf's pad written each frame:
//! the pad frame [pad +0x38f8] (FUN_14095fd80: +0x67 + input id, per HKS ACTION_ARM through
//! FUN_140b258d0's map, read by FUN_1407ef960 into the ActionRequest module; +0x3c / +0x3e the
//! attack / guard held bytes FUN_1407ef120 copies to manipulator +0xf4 / +0xf5; +0x0 / +0x4 the
//! move stick).
//!
//! `acttrace [name...]` writes extracted/emu/acttrace_<name>.json in `trace`'s format (entered,
//! edges) plus the anim ids the TAE update FUN_140b5c920 ran per frame, for frame_tree.py --base.
//! `acttrace_general` runs the General's frames with the AI on (b143d7a376 = 0) and splits his
//! trace at his first attack anim (3000 .. 3999); `acttrace_general_forced` requests one;
//! `acttrace_duel[_block|_deflect|_hit]` runs both with the world tasks. Results and the call
//! trees: docs/kb/frame-coverage.md "Mid-action frames".

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::c_void;
use std::path::Path;
use std::sync::Mutex;

use serde_json::json;

use super::{proxy, turn, uc, Emu};

const WORLD_CHR_MAN: u64 = 0x1_43d7_a1e0;
/// One character's frame (rcx WorldChrMan, xmm1 dt, r8 chr).
const CHR_FRAME: u64 = 0x1_40a4_b570;
/// ACTION_ARM -> pad input id (the map at 0x143d852a8).
const ARM_INPUT: u64 = 0x1_40b2_58d0;
/// TAE update of the selected clip (rcx anim id, ...), one per clip per update.
const TAE_UPDATE: u64 = 0x1_40b5_c920;
const ENEMY_VT: u64 = 0x1_42a2_7f28;
/// The menu state flags (FUN_1408c58e0).
const MENU_STATE: u64 = 0x1_408c_58e0;
const DT: f32 = 1.0 / 60.0;

static ANIMS: Mutex<Vec<i64>> = Mutex::new(Vec::new());

extern "C" fn on_tae(uc: uc::Engine, _addr: u64, _size: u32, _data: *mut c_void) {
    let mut rcx = 0u64;
    unsafe { uc::uc_reg_read(uc, uc::RCX, &mut rcx as *mut u64 as _) };
    ANIMS.lock().unwrap().push(rcx as u32 as i32 as i64);
}

/// HKS ACTION_ARM ids (c0000_define.lua).
const ATTACK: u32 = 0;
const SUB_ATTACK: u32 = 1;
const GUARD: u32 = 2;
const WIRE_SHOOT: u32 = 3;
const JUMP: u32 = 4;
const SP_MOVE: u32 = 5;
const USE_ITEM: u32 = 7;

/// Pad state of one frame: arms held, move stick (forward, side).
struct Input {
    arms: &'static [u32],
    stick: [f32; 2],
}

const NONE: Input = Input { arms: &[], stick: [0.0, 0.0] };

struct Act {
    name: &'static str,
    frames: usize,
    input: fn(usize) -> Input,
}

const ACTS: [Act; 11] = [
    Act { name: "idle", frames: 60, input: |_| NONE },
    // Two taps: the first slash and the combo's second (pressed again at f24).
    Act { name: "attack", frames: 70, input: |f| if f < 3 || (24..27).contains(&f) { Input { arms: &[ATTACK], stick: [0.0, 0.0] } } else { NONE } },
    Act { name: "guard", frames: 60, input: |f| if f < 40 { Input { arms: &[GUARD], stick: [0.0, 0.0] } } else { NONE } },
    // A guard tap: the deflect window with nothing to deflect.
    Act { name: "deflect", frames: 40, input: |f| if f < 2 { Input { arms: &[GUARD], stick: [0.0, 0.0] } } else { NONE } },
    Act { name: "jump", frames: 70, input: |f| if f < 3 { Input { arms: &[JUMP], stick: [0.0, 0.0] } } else { NONE } },
    Act { name: "step", frames: 45, input: |f| if f < 3 { Input { arms: &[SP_MOVE], stick: [0.0, 1.0] } } else { Input { arms: &[], stick: [0.0, 1.0] } } },
    Act { name: "sprint", frames: 60, input: |_| Input { arms: &[SP_MOVE], stick: [1.0, 0.0] } },
    // Attack and guard together: FUN_1407ef960 sets ACTION_ARM_SPECIAL_ATTACK (32).
    Act { name: "art", frames: 90, input: |f| if f < 3 { Input { arms: &[ATTACK, GUARD], stick: [0.0, 0.0] } } else { NONE } },
    Act { name: "prosthetic", frames: 70, input: |f| if f < 3 { Input { arms: &[SUB_ATTACK], stick: [0.0, 0.0] } } else { NONE } },
    Act { name: "grapple", frames: 60, input: |f| if f < 3 { Input { arms: &[WIRE_SHOOT], stick: [0.0, 0.0] } } else { NONE } },
    Act { name: "item", frames: 70, input: |f| if f < 3 { Input { arms: &[USE_ITEM], stick: [0.0, 0.0] } } else { NONE } },
];

/// What a run entered: counts per function and (return address, function) edges.
#[derive(Default)]
struct Tr {
    entered: BTreeMap<u64, u64>,
    edges: BTreeSet<(u64, u64)>,
}

impl Tr {
    fn take(&mut self, emu: &mut Emu) {
        for (a, n) in std::mem::take(&mut emu.st.entered) {
            *self.entered.entry(a).or_default() += n;
        }
        self.edges.extend(std::mem::take(&mut emu.st.edges));
    }

    fn write(&self, emu: &Emu, out: &Path, name: &str, extra: serde_json::Value) {
        let list: Vec<_> = self.entered.iter().map(|(&a, &n)| json!({"va": format!("{a:#x}"), "count": n, "size": emu.sizes.get(&a).copied().unwrap_or(0)})).collect();
        let edges: Vec<_> = self.edges.iter().map(|(r, f)| [format!("{r:#x}"), format!("{f:#x}")]).collect();
        let v = json!({"function": format!("{CHR_FRAME:#x}"), "entered": list, "edges": edges, "run": extra});
        std::fs::write(out.join(format!("acttrace_{name}.json")), serde_json::to_string(&v).unwrap()).unwrap();
        println!("  -> acttrace_{name}.json: {} functions", self.entered.len());
    }
}

fn arm_inputs(emu: &mut Emu) -> BTreeMap<u32, i32> {
    (0..33u32).map(|arm| (arm, emu.call(ARM_INPUT, &[arm as u64], &[]).rax as u32 as i32)).collect()
}

fn start_trace(emu: &mut Emu) {
    assert!(!emu.sizes.is_empty(), "acttrace needs functions.json (DSEKIRO_FUNCTIONS)");
    emu.st.starts = emu.sizes.keys().copied().collect();
    emu.st.entered.clear();
    emu.st.edges.clear();
    emu.limit = 500_000_000;
    // As proxy::setup: the CRT's ISA level 1 (Unicorn lacks AVX2) and a plain longjmp for the HKS GC.
    emu.raw_write(0x1_43c1_d520, &1u32.to_le_bytes());
    emu.raw_write(proxy::LONGJMP, proxy::PLAIN_LONGJMP);
    // The menu state FUN_1408c58e0 is 0x800008 in the snapshot (bit 8: a menu id 0x11 / 0xb4 / 0xb6
    // open, FUN_1408b6380; 0x800000), and FUN_1407ef960 then drops every button. The emulator copy
    // gets `xor eax, eax; ret` there: no menu, as in play.
    emu.raw_write(MENU_STATE, &[0x31, 0xc0, 0xc3]);
    // FUN_140e13700 reads the open menu window instead (FUN_140d872c0 +0x100, set in the snapshot).
    let menus = emu.call(0x1_40d8_72c0, &[], &[]).rax;
    if menus != 0 {
        emu.u64(menus + 0x100, 0);
    }
    emu.hook(uc::HOOK_CODE, on_tae as *const c_void, TAE_UPDATE, TAE_UPDATE);
}

/// Writes one frame's pad: every mapped input byte (+0x67 + id), the attack / guard held bytes, the stick.
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

fn wolf_act(new_emu: &dyn Fn() -> Emu, out: &Path, act: &Act) {
    let t = std::time::Instant::now();
    let mut emu = new_emu();
    let w = turn::wolf(&mut emu);
    let ids = arm_inputs(&mut emu);
    let frame = w.stick;
    let wcm = emu.peek_u64(WORLD_CHR_MAN);
    start_trace(&mut emu);
    let mut tr = Tr::default();
    let mut per_frame = Vec::new();
    let mut stopped = None;
    for f in 0..act.frames {
        press(&mut emu, frame, &ids, &(act.input)(f));
        ANIMS.lock().unwrap().clear();
        proxy::flush_tb(&emu);
        let r = emu.call(CHR_FRAME, &[wcm, 0, w.player], &[0.0, DT]);
        let n_new = emu.st.entered.keys().filter(|a| !tr.entered.contains_key(a)).count();
        tr.take(&mut emu);
        let anims: BTreeSet<i64> = ANIMS.lock().unwrap().iter().copied().collect();
        per_frame.push(json!({"anims": anims, "new": n_new}));
        if r.stopped.is_some() {
            stopped = r.stopped.map(|(a, e)| format!("{a:#x} error {e}"));
            break;
        }
    }
    let anims: BTreeSet<i64> = per_frame.iter().flat_map(|p| p["anims"].as_array().unwrap().iter().map(|v| v.as_i64().unwrap())).collect();
    println!("{} ({} frames, {:.0} s): {} functions, anims {:?}, stopped {:?}", act.name, per_frame.len(), t.elapsed().as_secs_f32(), tr.entered.len(), anims, stopped);
    tr.write(&emu, out, act.name, json!({"chr": "wolf", "dt": DT, "frames": per_frame, "stopped": stopped, "arm_inputs": ids}));
}

fn try_u64(emu: &mut Emu, at: u64) -> Option<u64> {
    if !(0x10000..=0x7fff_ffff_ffff).contains(&at) || at % 8 != 0 {
        return None;
    }
    let mut b = [0u8; 8];
    if unsafe { uc::uc_mem_read(emu.uc, at, b.as_mut_ptr() as _, 8) } == 0 {
        return Some(u64::from_le_bytes(b));
    }
    let (from, bytes) = emu.st.snap.as_mut()?.window(at)?;
    emu.map(from, bytes.len() as u64);
    emu.raw_write(from, &bytes);
    Some(u64::from_le_bytes(emu.read(at)))
}

/// The General (chr +0x6c == 1020, as modticks / guard find him).
fn general(emu: &mut Emu) -> u64 {
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

/// The General's AI object: manipulator [[chr+0x50]+0x358] (else +0x120), +0x340. Its +0x7430 is the
/// AI's attack request (SetAttackRequest); the manipulator FUN_1407e6570 copies it (else +0x7414,
/// StartGuard) to the ActionRequest module +0x170 every frame, AI on or off (kb/enemy.md).
fn ai_of(emu: &mut Emu, chr: u64) -> u64 {
    let ctrl = emu.peek_u64(chr + 0x50);
    let m = match emu.peek_u64(ctrl + 0x358) {
        0 => emu.peek_u64(ctrl + 0x120),
        m => m,
    };
    let ai = emu.peek_u64(m + 0x340);
    // Maps the request's page from the snapshot before raw writes; prints what the AI holds.
    let req: [i32; 2] = [i32::from_le_bytes(emu.peek(ai + 0x7430)), i32::from_le_bytes(emu.peek(ai + 0x7414))];
    println!("  AI of {chr:#x}: manipulator {m:#x}, AI {ai:#x}, attack / guard request {req:?}");
    ai
}

/// The attack the forced runs request: the General's first combo (behaviour 3000, a000_003000).
const FORCED_ATTACK: i32 = 3000;

/// The AI manipulator's request copy (runs only with the AI on, after the AI's think has set
/// +0x7430 for the frame).
const AI_REQUEST_COPY: u64 = 0x1_407e_6570;

/// (AI object, attack request) written at AI_REQUEST_COPY's entry; request < 0 leaves the AI's own.
static FORCE: Mutex<(u64, i32)> = Mutex::new((0, -1));

extern "C" fn on_request_copy(uc: uc::Engine, _addr: u64, _size: u32, _data: *mut c_void) {
    let (ai, v) = *FORCE.lock().unwrap();
    if ai != 0 && v >= 0 {
        unsafe { uc::uc_mem_write(uc, ai + 0x7430, v.to_le_bytes().as_ptr() as _, 4) };
    }
}

/// With the AI on, the General's attack request is FORCED_ATTACK for the first 3 frames (`f` from
/// the run's start): SetAttackRequest's field, set where the manipulator copies it, so his HKS
/// starts the attack the way the AI would. After that the AI's own requests stand.
fn force_attack(ai: u64, f: usize) {
    *FORCE.lock().unwrap() = (ai, if f < 3 { FORCED_ATTACK } else { -1 });
}

/// The chr's current anim (TimeAct module [chr+0x1ff8]+0x10, +0xf8; as taestate reads it).
fn anim(emu: &mut Emu, chr: u64) -> i64 {
    let bag = emu.peek_u64(chr + 0x1ff8);
    let m = emu.peek_u64(bag + 0x10);
    i32::from_le_bytes(emu.peek(m + 0xf8)) as i64
}

/// chr +0x110: the update omission (30 / 5 for chrs 36-90 m off in the snapshot, as throw2 found);
/// 0 updates the General every frame, as when he fights Wolf.
fn every_frame(emu: &mut Emu, chr: u64) {
    let _ = emu.peek::<4>(chr + 0x110);
    emu.raw_write(chr + 0x110, &0u32.to_le_bytes());
}

fn pos(emu: &mut Emu, chr: u64) -> [f32; 3] {
    let bag = emu.peek_u64(chr + 0x1ff8);
    let phys = emu.peek_u64(bag + 0x68);
    [0, 4, 8].map(|o| f32::from_le_bytes(emu.peek(phys + 0x80 + o)))
}

/// The General alone with the AI on, updated every frame: as he is (`force` false) or with
/// FORCED_ATTACK requested (`force`). His frames until he has been in an attack anim for `after` frames. Two traces: before the first attack
/// anim (`general_pre`) and from it on (`general_attack`).
fn general_act(new_emu: &dyn Fn() -> Emu, out: &Path, max: usize, after: usize, force: bool) {
    let t = std::time::Instant::now();
    let mut emu = new_emu();
    let g = general(&mut emu);
    let ai = ai_of(&mut emu, g);
    let wcm = emu.peek_u64(WORLD_CHR_MAN);
    emu.raw_write(0x1_43d7_a376, &[0]);
    if force {
        emu.hook(uc::HOOK_CODE, on_request_copy as *const c_void, AI_REQUEST_COPY, AI_REQUEST_COPY);
    }
    every_frame(&mut emu, g);
    start_trace(&mut emu);
    let (mut pre, mut atk) = (Tr::default(), Tr::default());
    let mut per_frame = Vec::new();
    let mut since: Option<usize> = None;
    let mut stopped = None;
    for f in 0..max {
        ANIMS.lock().unwrap().clear();
        proxy::flush_tb(&emu);
        if force {
            force_attack(ai, f);
        }
        let r = emu.call(CHR_FRAME, &[wcm, 0, g], &[0.0, DT]);
        let mut anims: BTreeSet<i64> = ANIMS.lock().unwrap().iter().copied().collect();
        anims.insert(anim(&mut emu, g));
        if since.is_none() && anims.iter().any(|a| (3000..4000).contains(&(a % 1_000_000))) {
            since = Some(f);
        }
        match since {
            Some(_) => atk.take(&mut emu),
            None => pre.take(&mut emu),
        }
        per_frame.push(json!({"anims": anims}));
        if f % 30 == 0 {
            println!("  general f{f} anims {anims:?} ({:.0} s)", t.elapsed().as_secs_f32());
        }
        if r.stopped.is_some() {
            stopped = r.stopped.map(|(a, e)| format!("{a:#x} error {e}"));
            break;
        }
        if since.is_some_and(|s| f >= s + after) {
            break;
        }
    }
    println!("general ({} frames, attack from {since:?}, {:.0} s): pre {} atk {} functions, stopped {stopped:?}", per_frame.len(), t.elapsed().as_secs_f32(), pre.entered.len(), atk.entered.len());
    let run = json!({"chr": "general", "ai": true, "forced_attack": force.then_some(FORCED_ATTACK), "dt": DT, "attack_from": since, "frames": per_frame, "stopped": stopped});
    let name = if force { "general_forced" } else { "general" };
    pre.write(&emu, out, &format!("{name}_pre"), run.clone());
    if since.is_some() {
        atk.write(&emu, out, &format!("{name}_attack"), run);
    }
}

/// WorldChrManImp's world tasks (rcx WorldChrMan, rdx a task data block with dt at +8): pre-update,
/// the second pre-update, post-update, group 0x51 (frame-coverage.md).
const TASKS_PRE: [u64; 2] = [0x1_40a5_09f0, 0x1_40a4_db20];
const TASKS_POST: [u64; 2] = [0x1_40a5_0880, 0x1_40a4_88b0];

/// Wolf and the General (AI on, FORCED_ATTACK requested at the start) in whole world frames: the pre-update tasks, the General's chr
/// frame, Wolf's, the post-update tasks, so the General's attack can reach Wolf. Wolf's pad:
/// `block` holds guard, `deflect` taps it (2 frames) every 9 frames from the General's attack anim
/// on, `hit` does nothing. Runs until `after` frames past the General's first attack anim.
/// Writes acttrace_duel_<kind>.json (both characters and the tasks) with each one's anims per frame.
fn duel(new_emu: &dyn Fn() -> Emu, out: &Path, kind: &str, max: usize, after: usize) {
    let t = std::time::Instant::now();
    let mut emu = new_emu();
    let w = turn::wolf(&mut emu);
    let ids = arm_inputs(&mut emu);
    let g = general(&mut emu);
    let ai = ai_of(&mut emu, g);
    let wcm = emu.peek_u64(WORLD_CHR_MAN);
    emu.raw_write(0x1_43d7_a376, &[0]);
    emu.hook(uc::HOOK_CODE, on_request_copy as *const c_void, AI_REQUEST_COPY, AI_REQUEST_COPY);
    every_frame(&mut emu, g);
    // The General 2.5 m in front of Wolf, facing him (forward = (-sin yaw, 0, -cos yaw)).
    let wp = pos(&mut emu, w.player);
    let yaw = f32::from_le_bytes(emu.peek(w.phys + 0x74));
    let at = [wp[0] - 2.5 * yaw.sin(), wp[1], wp[2] - 2.5 * yaw.cos()];
    let bag = emu.peek_u64(g + 0x1ff8);
    let gphys = emu.peek_u64(bag + 0x68);
    let (wrapper, _) = proxy::proxy_of(&mut emu, gphys);
    proxy::place(&mut emu, gphys, wrapper, at);
    emu.poke_f32s(gphys + 0x74, &[yaw + std::f32::consts::PI]);
    let gp = pos(&mut emu, g);
    println!("  duel {kind}: Wolf {wp:?} General {gp:?}, {:.2} m apart", ((wp[0] - gp[0]).powi(2) + (wp[2] - gp[2]).powi(2)).sqrt());
    start_trace(&mut emu);
    let blk = emu.alloc(0x40);
    emu.poke_f32s(blk + 8, &[DT]);
    let mut tr = Tr::default();
    let mut per_frame = Vec::new();
    let mut since: Option<usize> = None;
    let mut stopped = None;
    let hp = |emu: &mut Emu, c: u64| {
        let bag = emu.peek_u64(c + 0x1ff8);
        let data = emu.peek_u64(bag + 0x18);
        i32::from_le_bytes(emu.peek(data + 0x130))
    };
    for f in 0..max {
        let guard = match kind {
            "block" => true,
            "deflect" => since.is_some_and(|s| (f - s) % 9 < 2),
            _ => false,
        };
        let arms: &'static [u32] = if guard { &[GUARD] } else { &[] };
        press(&mut emu, w.stick, &ids, &Input { arms, stick: [0.0, 0.0] });
        proxy::flush_tb(&emu);
        let mut stop = None;
        for task in TASKS_PRE {
            stop = stop.or(emu.call(task, &[wcm, blk], &[]).stopped);
        }
        ANIMS.lock().unwrap().clear();
        force_attack(ai, f);
        stop = stop.or(emu.call(CHR_FRAME, &[wcm, 0, g], &[0.0, DT]).stopped);
        let mut ga: BTreeSet<i64> = ANIMS.lock().unwrap().drain(..).collect();
        ga.insert(anim(&mut emu, g));
        stop = stop.or(emu.call(CHR_FRAME, &[wcm, 0, w.player], &[0.0, DT]).stopped);
        let mut wa: BTreeSet<i64> = ANIMS.lock().unwrap().drain(..).collect();
        wa.insert(anim(&mut emu, w.player));
        for task in TASKS_POST {
            stop = stop.or(emu.call(task, &[wcm, blk], &[]).stopped);
        }
        tr.take(&mut emu);
        if since.is_none() && ga.iter().any(|a| (3000..4000).contains(&(a % 1_000_000))) {
            since = Some(f);
        }
        let (whp, ghp) = (hp(&mut emu, w.player), hp(&mut emu, g));
        if f % 30 == 0 || since == Some(f) {
            println!("  duel {kind} f{f} general {ga:?} wolf {wa:?} hp {whp} / {ghp} ({:.0} s)", t.elapsed().as_secs_f32());
        }
        per_frame.push(json!({"general": ga, "wolf": wa, "guard": guard, "wolf_hp": whp, "general_hp": ghp}));
        if stop.is_some() {
            stopped = stop.map(|(a, e)| format!("{a:#x} error {e}"));
            break;
        }
        if since.is_some_and(|s| f >= s + after) {
            break;
        }
    }
    let wa: BTreeSet<i64> = per_frame.iter().flat_map(|p| p["wolf"].as_array().unwrap().iter().map(|v| v.as_i64().unwrap())).collect();
    println!("duel {kind} ({} frames, attack from {since:?}, {:.0} s): {} functions, wolf anims {wa:?}, stopped {stopped:?}", per_frame.len(), t.elapsed().as_secs_f32(), tr.entered.len());
    tr.write(&emu, out, &format!("duel_{kind}"), json!({"chr": "wolf+general", "roots": TASKS_PRE.iter().chain(&[CHR_FRAME]).chain(&TASKS_POST).map(|a| format!("{a:#x}")).collect::<Vec<_>>(), "ai": true, "forced_attack": FORCED_ATTACK, "dt": DT, "attack_from": since, "frames": per_frame, "stopped": stopped}));
}

pub fn run(new_emu: &dyn Fn() -> Emu, out: &Path, want: &[&str]) {
    let names: Vec<&str> = want.iter().filter_map(|w| w.strip_prefix("acttrace_")).collect();
    for act in &ACTS {
        if names.is_empty() || names.contains(&act.name) {
            wolf_act(new_emu, out, act);
        }
    }
    if names.is_empty() || names.contains(&"general") {
        general_act(new_emu, out, 600, 60, false);
    }
    if names.is_empty() || names.contains(&"general_forced") {
        general_act(new_emu, out, 150, 120, true);
    }
    for kind in ["block", "deflect", "hit"] {
        if names.contains(&format!("duel_{kind}").as_str()) || names.contains(&"duel") {
            duel(new_emu, out, kind, 600, 90);
        }
    }
}
