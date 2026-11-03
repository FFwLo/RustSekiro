//! Wolf's turn and move from the move stick, on the snapshot: the pad manipulator, the behaviour
//! variables the HKS reads (MoveSpeedLevel, MoveAngle, TurnAngle) and the turn controller.

use std::collections::BTreeMap;
use std::ffi::c_void;
use std::path::Path;
use std::sync::Mutex;

use serde_json::{json, Value};

use super::{uc, Emu};

/// Per watched function: (calls, rcx of the first call).
static HITS: Mutex<BTreeMap<u64, (usize, u64)>> = Mutex::new(BTreeMap::new());

extern "C" fn on_hit(uc: uc::Engine, addr: u64, _size: u32, _data: *mut c_void) {
    let mut rcx = 0u64;
    unsafe { uc::uc_reg_read(uc, uc::RCX, &mut rcx as *mut u64 as _) };
    let mut h = HITS.lock().unwrap();
    let e = h.entry(addr).or_insert((0, rcx));
    e.0 += 1;
}

/// Logged calls: (function, return address, rcx, 4 floats at rdx).
static LOG: Mutex<Vec<(u64, u64, u64, [f32; 4])>> = Mutex::new(Vec::new());

extern "C" fn on_log(uc: uc::Engine, addr: u64, _size: u32, _data: *mut c_void) {
    let (mut rcx, mut rdx, mut rsp, mut ret) = (0u64, 0u64, 0u64, 0u64);
    let mut v = [0f32; 4];
    unsafe {
        uc::uc_reg_read(uc, uc::RCX, &mut rcx as *mut u64 as _);
        uc::uc_reg_read(uc, uc::RDX, &mut rdx as *mut u64 as _);
        uc::uc_reg_read(uc, uc::RSP, &mut rsp as *mut u64 as _);
        uc::uc_mem_read(uc, rsp, &mut ret as *mut u64 as _, 8);
        uc::uc_mem_read(uc, rdx, v.as_mut_ptr() as _, 16);
    }
    LOG.lock().unwrap().push((addr, ret, rcx, v));
}

/// Functions of the turn path, hooked to see which a frame function reaches.
const WATCH: [(u64, &str); 13] = [
    (0x1_4096_03d0, "pad stick read"),
    (0x1_407f_0a70, "PadManipulator update"),
    (0x1_407f_0f50, "manipulator free stick"),
    (0x1_407f_1710, "manipulator lock-on stick"),
    (0x1_407d_a6d0, "BehaviorTurn update"),
    (0x1_407d_af30, "BehaviorTurn free"),
    (0x1_407d_ac80, "BehaviorTurn other"),
    (0x1_407d_ab40, "turn clamp"),
    (0x1_407f_3c90, "PlayerCtrl root motion"),
    (0x1_40b4_afd0, "MoveSpeedLevel / MoveAngle"),
    (0x1_40b4_c600, "TurnAngle"),
    (0x1_407d_ed90, "ChrCtrl input"),
    (0x1_40a0_4850, "ChrIns input"),
];

/// FD4Time { vtable, dt } on the emulator heap.
pub(super) fn fd4_time(emu: &mut Emu, dt: f32) -> u64 {
    let t = emu.alloc(0x10);
    emu.u64(t, 0x1_4294_8018);
    emu.f32(t + 8, dt);
    t
}

/// trace <addr> [this]: runs a frame function on Wolf (rcx = PlayerIns, rdx = FD4Time 1/60) and
/// lists which functions of the turn path it reaches.
pub(super) fn trace(new_emu: &dyn Fn() -> Emu, args: &[&str]) {
    for a in args {
        let va = u64::from_str_radix(a.trim_start_matches("0x"), 16).unwrap();
        let mut emu = new_emu();
        let player = emu.st.snap.as_ref().expect("trace needs --snapshot").player;
        for (w, _) in WATCH {
            emu.hook(uc::HOOK_CODE, on_hit as *const c_void, w, w);
        }
        HITS.lock().unwrap().clear();
        let t = fd4_time(&mut emu, 1.0 / 60.0);
        let r = emu.call(va, &[player, t], &[]);
        let mut calls = r.calls.clone();
        calls.sort();
        calls.dedup();
        println!("trace {va:#x}: {} blocks, stopped {:?}, stubs {:?}", r.blocks.len(), r.stopped.map(|(a, e)| (format!("{a:#x}"), e)), calls);
        if r.stopped.is_some() {
            println!("  last blocks {:x?}", emu.st.trail);
        }
        for (w, (n, rcx)) in HITS.lock().unwrap().iter() {
            let name = WATCH.iter().find(|x| x.0 == *w).map(|x| x.1).unwrap_or("");
            println!("  {w:#x} {name}: {n} (rcx {rcx:#x})");
        }
    }
}

/// PlayerIns update (vtable 0x142a2b338 slot 0xb0): input, manipulator, behaviour variables,
/// the behaviour and the turn controller in one call.
pub(super) const PLAYER_UPDATE: u64 = 0x1_40a3_7560;

/// Wolf's objects for a frame run.
pub(super) struct Wolf {
    pub player: u64,
    pub manip: u64,
    pub phys: u64,
    /// The pad's stick floats ([pad + 0x38f8]): [0] forward, [1] side, as FUN_1409603d0/e0 read them.
    pub stick: u64,
    /// SprjBehaviorModule-like object FUN_140b4afd0 runs on (+0xbac MoveSpeedLevel, +0xbb0 MoveAngle).
    pub beh: u64,
}

pub(super) fn wolf(emu: &mut Emu) -> Wolf {
    let player = emu.st.snap.as_ref().expect("needs --snapshot").player;
    let ctrl = emu.peek_u64(player + 0x50);
    let manip = match emu.peek_u64(ctrl + 0x358) {
        0 => emu.peek_u64(ctrl + 0x120),
        m => m,
    };
    let bag = emu.peek_u64(player + 0x1ff8);
    let phys = emu.peek_u64(bag + 0x68);
    // One hooked frame with the stick let go finds the pad and the behaviour object.
    for (w, _) in WATCH {
        emu.hook(uc::HOOK_CODE, on_hit as *const c_void, w, w);
    }
    HITS.lock().unwrap().clear();
    let t = fd4_time(emu, 1.0 / 60.0);
    let r = emu.call(PLAYER_UPDATE, &[player, t], &[]);
    assert!(r.stopped.is_none(), "first frame stopped {:?}", r.stopped);
    let h = HITS.lock().unwrap().clone();
    let pad = h[&0x1_4096_03d0].1;
    let beh = h[&0x1_40b4_afd0].1;
    let stick = emu.peek_u64(pad + 0x38f8);
    Wolf { player, manip, phys, stick, beh }
}

fn f32s(emu: &mut Emu, at: u64, n: usize) -> Vec<f32> {
    (0..n).map(|i| f32::from_le_bytes(emu.peek(at + 4 * i as u64))).collect()
}

/// `turn probe fwd side frames`: frames with one stick, printing what moves.
fn probe(new_emu: &dyn Fn() -> Emu, args: &[&str]) {
    let v: Vec<f32> = args[..3].iter().map(|a| a.parse().unwrap()).collect();
    let mut emu = new_emu();
    let w = wolf(&mut emu);
    println!("player {:#x} manip {:#x} phys {:#x} stick {:#x} beh {:#x}", w.player, w.manip, w.phys, w.stick, w.beh);
    println!("stick before {:?}, manip +0x94 {:?}", f32s(&mut emu, w.stick, 2), f32s(&mut emu, w.manip + 0x90, 4));
    let bag = emu.peek_u64(w.player + 0x1ff8);
    let act = emu.peek_u64(bag);
    let m_b8 = emu.peek_u64(bag + 0xb8);
    let ctrl = emu.peek_u64(w.player + 0x50);
    let bt = emu.peek_u64(ctrl + 0x250);
    let dc = emu.peek_u64(bt + 0x50);
    println!("action +0x308.. {:?} +0x70 {:#x} +0x10 {:#x}; module 0xb8 +0x16c {:?}", f32s(&mut emu, act + 0x308, 9), emu.peek_u64(act + 0x70), emu.peek_u64(act + 0x10), f32s(&mut emu, m_b8 + 0x16c, 1));
    println!("BehaviorTurn {bt:#x}: {:?}; delay {dc:#x}: {:?}", f32s(&mut emu, bt, 0x20), f32s(&mut emu, dc, 0x28));
    // Who sets the physics rotation, the turn clamp's inputs, BehaviorTurn's outputs.
    for a in [0x1_407d_e0c0u64, 0x1_40bc_29b0, 0x1_407d_ab40, 0x1_407d_a6d0, 0x1_407f_2e90, 0x1_407d_a4a0] {
        emu.hook(uc::HOOK_CODE, on_log as *const c_void, a, a);
    }
    for f in 0..v[2] as usize {
        emu.poke_f32s(w.stick, &[v[0], v[1]]);
        let t = fd4_time(&mut emu, 1.0 / 60.0);
        LOG.lock().unwrap().clear();
        let r = emu.call(PLAYER_UPDATE, &[w.player, t], &[]);
        for (a, ret, rcx, v) in LOG.lock().unwrap().iter() {
            println!("   call {a:#x} from {ret:#x} rcx {rcx:#x} [rdx] {v:.4?}");
        }
        for extra in &args[3..] {
            let va = u64::from_str_radix(extra.trim_start_matches("0x"), 16).unwrap();
            LOG.lock().unwrap().clear();
            let r2 = emu.call(va, &[w.player, t], &[]);
            println!("   then {va:#x}: {} blocks stopped {:?}", r2.blocks.len(), r2.stopped);
            for (a, ret, rcx, v) in LOG.lock().unwrap().iter() {
                println!("     call {a:#x} from {ret:#x} rcx {rcx:#x} [rdx] {v:.4?}");
            }
        }
        let yaw = f32s(&mut emu, w.phys + 0x70, 4);
        let pos = f32s(&mut emu, w.phys + 0x80, 3);
        let m10 = f32s(&mut emu, w.manip + 0x10, 4);
        let m20 = f32s(&mut emu, w.manip + 0x20, 4);
        let mc0 = f32s(&mut emu, w.manip + 0xc0, 4);
        let lvl = f32s(&mut emu, w.beh + 0xbac, 2);
        println!("f{f} stop {:?} euler {yaw:.4?} pos {pos:.3?} m10 {m10:.3?} m20 {m20:.4?} mc0 {mc0:.3?} level/angle {lvl:.3?}", r.stopped);
        println!("   delay +0x20.. {:.4?}", f32s(&mut emu, dc + 0x20, 0x1e));
    }
}

/// One scenario: the pad stick ([0] forward, [1] side) each frame, the frame time, and what is
/// poked before the run (camera yaw in the manipulator, a TAE SetTurnSpeed in the action module).
struct Scenario {
    name: &'static str,
    dt: f32,
    frames: usize,
    stick: fn(usize) -> [f32; 2],
    /// The manipulator's camera yaw (+0x94), written every frame.
    cam_yaw: f32,
    /// TAE 224 SetTurnSpeed [deg/s] in the action module (+0x308 and the four direction slots
    /// +0x30c..+0x318), written every frame; -1 = none (the snapshot's).
    tae_turn: f32,
}

fn dir(deg: f32, len: f32) -> [f32; 2] {
    let r = deg.to_radians();
    [len * r.cos(), len * r.sin()]
}

const SCENARIOS: [Scenario; 14] = [
    Scenario { name: "forward", dt: 1.0 / 60.0, frames: 20, stick: |_| [1.0, 0.0], cam_yaw: 0.0, tae_turn: -1.0 },
    Scenario { name: "side", dt: 1.0 / 60.0, frames: 30, stick: |_| [0.0, 1.0], cam_yaw: 0.0, tae_turn: -1.0 },
    Scenario { name: "other_side", dt: 1.0 / 60.0, frames: 30, stick: |_| [0.0, -1.0], cam_yaw: 0.0, tae_turn: -1.0 },
    Scenario { name: "back", dt: 1.0 / 60.0, frames: 30, stick: |_| [-1.0, 0.0], cam_yaw: 0.0, tae_turn: -1.0 },
    Scenario { name: "back_left_135", dt: 1.0 / 60.0, frames: 30, stick: |_| dir(-135.0, 1.0), cam_yaw: 0.0, tae_turn: -1.0 },
    Scenario { name: "half_diagonal", dt: 1.0 / 60.0, frames: 30, stick: |_| [0.5, 0.5], cam_yaw: 0.0, tae_turn: -1.0 },
    // Stick length 0 .. 1.2 in 0.05 steps, two frames each, straight ahead.
    Scenario { name: "levels", dt: 1.0 / 60.0, frames: 50, stick: |f| [(f / 2) as f32 * 0.05, 0.0], cam_yaw: 0.0, tae_turn: -1.0 },
    // The stick swept round at 10 / 40 / 60 deg per frame.
    Scenario { name: "sweep_10", dt: 1.0 / 60.0, frames: 40, stick: |f| dir(10.0 * f as f32, 1.0), cam_yaw: 0.0, tae_turn: -1.0 },
    Scenario { name: "sweep_40", dt: 1.0 / 60.0, frames: 20, stick: |f| dir(40.0 * f as f32, 1.0), cam_yaw: 0.0, tae_turn: -1.0 },
    Scenario { name: "sweep_60", dt: 1.0 / 60.0, frames: 20, stick: |f| dir(60.0 * f as f32, 1.0), cam_yaw: 0.0, tae_turn: -1.0 },
    // Forward, a flick to the side, let go.
    Scenario { name: "flick_release", dt: 1.0 / 60.0, frames: 50, stick: |f| if f < 10 { [1.0, 0.0] } else if f < 30 { [0.0, -1.0] } else { [0.0, 0.0] }, cam_yaw: 0.0, tae_turn: -1.0 },
    Scenario { name: "side_30fps", dt: 1.0 / 30.0, frames: 20, stick: |_| [0.0, 1.0], cam_yaw: 0.0, tae_turn: -1.0 },
    Scenario { name: "camera_yaw", dt: 1.0 / 60.0, frames: 30, stick: |f| if f < 15 { [1.0, 0.0] } else { [0.0, 1.0] }, cam_yaw: 1.0, tae_turn: -1.0 },
    Scenario { name: "tae_turn_360", dt: 1.0 / 60.0, frames: 40, stick: |_| [0.0, 1.0], cam_yaw: 0.0, tae_turn: 360.0 },
];

/// `turn`: every scenario from the snapshot's state, PlayerIns update (FUN_140a37560) once per
/// frame. Writes turn_frames.json: per frame the stick and, after the frame, the manipulator's
/// move vector (+0x10), yaw request (+0x24), camera-rotated stick (+0xc0), MoveSpeedLevel /
/// MoveAngle (behaviour +0xbac / +0xbb0), the physics yaw (+0x74) and the turn delay
/// controller's yaw and angular speed (BehaviorTurn +0x50: +0x24, +0x30).
fn cases(new_emu: &dyn Fn() -> Emu, out: &Path) {
    let mut scenarios = Vec::new();
    for sc in &SCENARIOS {
        let mut emu = new_emu();
        let w = wolf(&mut emu);
        let ctrl = emu.peek_u64(w.player + 0x50);
        let bt = emu.peek_u64(ctrl + 0x250);
        let dc = emu.peek_u64(bt + 0x50);
        let bag = emu.peek_u64(w.player + 0x1ff8);
        let act = emu.peek_u64(bag);
        let mut frames = Vec::new();
        let mut stopped = None;
        for f in 0..sc.frames {
            let stick = (sc.stick)(f);
            emu.poke_f32s(w.stick, &stick);
            emu.poke_f32s(w.manip + 0x94, &[sc.cam_yaw]);
            if sc.tae_turn >= 0.0 {
                emu.poke_f32s(act + 0x308, &[sc.tae_turn; 5]);
            }
            let t = fd4_time(&mut emu, sc.dt);
            let r = emu.call(PLAYER_UPDATE, &[w.player, t], &[]);
            if r.stopped.is_some() {
                stopped = r.stopped;
                break;
            }
            let m10 = f32s(&mut emu, w.manip + 0x10, 3);
            let mc0 = f32s(&mut emu, w.manip + 0xc0, 3);
            let req = f32s(&mut emu, w.manip + 0x24, 1)[0];
            let lvl = f32s(&mut emu, w.beh + 0xbac, 2);
            let yaw = f32s(&mut emu, w.phys + 0x74, 1)[0];
            let shown = f32s(&mut emu, dc + 0x24, 1)[0];
            let speed = f32s(&mut emu, dc + 0x30, 1)[0];
            frames.push(json!({
                "stick": stick, "move": [m10[0], m10[2]], "world": [mc0[0], mc0[2]], "request": req,
                "level": lvl[0], "move_angle": lvl[1], "yaw": yaw, "shown": shown, "speed": speed,
            }));
        }
        let last = |k: &str| frames.last().map_or(0.0, |v: &Value| v[k].as_f64().unwrap());
        println!("{}: {} frames, yaw {:.4} shown {:.4} level {:.3}, stopped {:?}", sc.name, frames.len(), last("yaw"), last("shown"), last("level"), stopped.map(|(a, e)| (format!("{a:#x}"), e)));
        scenarios.push(json!({"name": sc.name, "dt": sc.dt, "cam_yaw": sc.cam_yaw, "tae_turn": sc.tae_turn, "frames": frames}));
    }
    let body = json!({"function": format!("{PLAYER_UPDATE:#x}"), "scenarios": scenarios});
    std::fs::write(out.join("turn_frames.json"), serde_json::to_string(&body).unwrap()).unwrap();
}

/// `turn trace <addr>...`, `turn probe fwd side frames [fn...]` or `turn` (the cases).
pub(super) fn main(new_emu: &dyn Fn() -> Emu, out: &Path, args: &[&str]) {
    match args.first() {
        Some(&"trace") => trace(new_emu, &args[1..]),
        Some(&"probe") => probe(new_emu, &args[1..]),
        _ => cases(new_emu, out),
    }
}
