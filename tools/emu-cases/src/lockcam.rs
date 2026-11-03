//! The lock-on camera (FUN_140734790 -> FUN_14073c260 with the follow camera's lock flag +0x2a0)
//! on the snapshot, and the pad -> manipulator mapping. Pilot names: `lockcam_probe`, `lockcam`.

use std::path::Path;

use serde_json::{json, Value};

use crate::{chr_cam, Emu};

const WORLD_CHR_MAN: u64 = 0x1_43d7_a1e0;
const LOCK_TGT_MAN: u64 = 0x1_43d7_8058;

fn f32s<const N: usize>(emu: &mut Emu, at: u64) -> [f32; N] {
    std::array::from_fn(|i| f32::from_le_bytes(emu.peek(at + 4 * i as u64)))
}

/// Prints what the lock camera reads on the snapshot: Wolf's lock module, the lock target
/// manager's entries, the ChrCam's lock helper (+0x90) and the follow camera's lock fields.
pub fn probe(emu: &mut Emu) {
    let player = emu.st.snap.as_ref().expect("lockcam needs --snapshot").player;
    let cam = chr_cam(emu);
    let follow = emu.peek_u64(cam + 0x60);
    let bag = emu.peek_u64(player + 0x1ff8);
    let lock = emu.peek_u64(bag + 0x88);
    println!("player {player:#x} chrcam {cam:#x} follow {follow:#x} lock module {lock:#x}");
    if lock != 0 {
        let e = emu.peek_u64(lock + 0x10);
        println!("  lock module +0x10 entry {e:#x}");
    }
    println!("  chrcam +0x10d0 {:?} +0x100 {:#x}", emu.peek::<4>(cam + 0x10d0), u32::from_le_bytes(emu.peek(cam + 0x100)));
    let h = cam + 0x90;
    println!("  helper +0x0 {:#x} +0x10 {:?} +0x74 {:?} +0x80 {:#x} +0x100 vt {:#x}", emu.peek_u64(h), f32s::<4>(emu, h + 0x10), emu.peek::<2>(h + 0x74), emu.peek_u64(h + 0x80), emu.peek_u64(h + 0x100));
    println!("  helper +0x1020 {:?} +0x1030 {:?}", f32s::<4>(emu, h + 0x1020), f32s::<4>(emu, h + 0x1030));
    println!("  follow +0x2a0 {:?} +0x280 {:?} +0x3ac {:?}", emu.peek::<4>(follow + 0x2a0), f32s::<4>(emu, follow + 0x280), emu.peek::<4>(follow + 0x3ac));
    println!("  DAT_143afe850 {} window DAT_143afe848 {:?}", u32::from_le_bytes(emu.peek(0x1_43af_e850)), f32s::<2>(emu, 0x1_43af_e848));
    let man = emu.peek_u64(LOCK_TGT_MAN);
    println!("lock tgt man {man:#x}: +0x2830 {:?} +0x2838 {:#x} +0x29bc {:?}", emu.peek::<4>(man + 0x2830), emu.peek_u64(man + 0x2838), emu.peek::<4>(man + 0x29bc));
    let wcm = emu.peek_u64(WORLD_CHR_MAN);
    let mut e = emu.peek_u64(man + 0x10);
    let mut n = 0;
    while e != 0 && n < 200 {
        let owner = emu.peek_u64(e + 0x88);
        let handle = if owner != 0 { u32::from_le_bytes(emu.peek(owner + 0x78)) } else { 0 };
        let r = emu.call(0x1_40a4_a050, &[wcm, handle as u64], &[]);
        let chr = r.rax;
        let pos = if chr != 0 {
            let b = emu.peek_u64(chr + 0x1ff8);
            let ph = emu.peek_u64(b + 0x68);
            f32s::<3>(emu, ph + 0x80)
        } else {
            [0.0; 3]
        };
        if n < 12 {
            println!(
                "  entry {e:#x} pt {:?} m0 {:?} +0x40 {:?} +0x50 {:?} +0x60 {:?} flags {:#x} handle {handle:#x} chr {chr:#x} pos {pos:?}",
                f32s::<4>(emu, e),
                f32s::<4>(emu, e + 0x10),
                f32s::<4>(emu, e + 0x40),
                f32s::<4>(emu, e + 0x50),
                f32s::<4>(emu, e + 0x60),
                emu.peek::<1>(e + 0x90)[0],
            );
        }
        e = emu.peek_u64(e + 0x80);
        n += 1;
    }
    println!("  {n} entries");
    let slots: Vec<String> = (0..4).map(|i| format!("{:#x}", emu.peek_u64(0x1_429a_7040 + 8 * i))).collect();
    println!("CSPositionRecorder_mSecond vtable {slots:?}");
}

/// The pad side: the options bytes (FUN_1407b2bd0 +10 / +11: the camera X / Y signs read by
/// FUN_1409668f0 / FUN_140966960), then Wolf's PadManipulator update FUN_1407f0f50 run on a
/// made-up pad frame (the pad object's +0x38f8, as FUN_140960650 fills it: +0x0 move y, +0x4
/// move x, +0x28 camera yaw, +0x2c camera pitch, +0x18 / +0x1c mouse), one slot at a time:
/// which frame slot lands in which manipulator field (M+0x80 right stick, +0xc0 the move turned
/// by the camera, +0xd0 mouse), against the follow camera's view (+0x30 row) and yaw.
fn pad_probe(emu: &mut Emu, out: &Path) {
    let o = emu.peek_u64(0x1_43d5_aac0);
    let opts = emu.peek_u64(o + 0x50);
    let ob = emu.peek::<16>(opts);
    println!("options {opts:#x}: {ob:?}");
    let player = emu.st.snap.as_ref().unwrap().player;
    let cam = chr_cam(emu);
    let follow = emu.peek_u64(cam + 0x60);
    let view: [f32; 3] = f32s(emu, follow + 0x30);
    let yaw = f32::from_le_bytes(emu.peek(follow + 0x174));
    let chr_ctrl = emu.peek_u64(player + 0x50);
    let m = match emu.peek_u64(chr_ctrl + 0x358) {
        0 => emu.peek_u64(chr_ctrl + 0x120),
        m => m,
    };
    let ctrl = emu.peek_u64(m + 0x300);
    let ctrl = emu.peek_u64(ctrl + 0x50);
    println!("M {m:#x} ctrl {ctrl:#x} (+0x10 chr {:#x}, player {player:#x}); camera view {view:?} yaw {yaw}", emu.peek_u64(ctrl + 0x10));
    println!("M +0x90 {:?} +0x420 {:?}", f32s::<4>(emu, m + 0x90), f32s::<4>(emu, m + 0x420));
    println!("ctrl +0x130 {:?}", f32s::<12>(emu, ctrl + 0x130));
    println!("player +0x1190 {:?}", f32s::<12>(emu, player + 0x1190));
    let phys = {
        let bag = emu.peek_u64(player + 0x1ff8);
        emu.peek_u64(bag + 0x68)
    };
    println!("physics +0x70 {:?}", f32s::<4>(emu, phys + 0x70));
    let pad = emu.alloc(0x4000);
    let frame = emu.alloc(0x100);
    emu.u64(pad + 0x38f8, frame);
    let m_before: Vec<[u8; 4]> = (0..0x440u64).step_by(4).map(|k| emu.peek(m + k)).collect();
    let mut cases = Vec::new();
    // (slot, value): one frame field set at a time.
    for (slot, v) in [(0x0u64, 1.0f32), (0x0, -1.0), (0x4, 1.0), (0x4, -1.0), (0x28, 1.0), (0x2c, 1.0), (0x18, 1.0), (0x1c, 1.0)] {
        for k in (0..0x100).step_by(4) {
            emu.f32(frame + k, 0.0);
        }
        emu.f32(frame + slot, v);
        for (k, b) in m_before.iter().enumerate() {
            emu.raw_write(m + 4 * k as u64, b);
        }
        // Twice: the move counts only when last frame's (M+0x340) points the same way.
        emu.call(0x1_407f_0f50, &[m, 0, ctrl, pad], &[]);
        let r = emu.call(0x1_407f_0f50, &[m, 0, ctrl, pad], &[]);
        let (s80, sc0, sd0, s10, s20): ([f32; 4], [f32; 4], [f32; 4], [f32; 4], [f32; 4]) = (f32s(emu, m + 0x80), f32s(emu, m + 0xc0), f32s(emu, m + 0xd0), f32s(emu, m + 0x10), f32s(emu, m + 0x20));
        println!("frame +{slot:#x} = {v}: M+0x80 {s80:?} +0xc0 {sc0:?} +0xd0 {sd0:?} +0x10 {s10:?} +0x20 {s20:?} stopped {:?}", r.stopped);
        cases.push(json!({"slot": slot, "value": v, "m80": s80, "mc0": sc0, "md0": sd0, "m10": s10, "m20": s20, "stopped": r.stopped.is_some()}));
    }
    // The same after the camera turned: 40 camera frames (FUN_140734790) with the stick's yaw
    // full, then the move up / right again, against the turned camera's view.
    for (k, b) in m_before.iter().enumerate() {
        emu.raw_write(m + 4 * k as u64, b);
    }
    let world = emu.peek_u64(0x1_43d6_d640);
    let world = emu.peek_u64(world + 0x98);
    for _ in 0..40 {
        emu.poke_f32s(m + 0x80, &[0.0, 1.0]);
        emu.call(0x1_4073_4790, &[cam, 0, player, world, 1], &[0.0, 1.0 / 60.0]);
    }
    emu.poke_f32s(m + 0x80, &[0.0, 0.0]);
    let view2: [f32; 3] = f32s(emu, follow + 0x30);
    let yaw2 = f32::from_le_bytes(emu.peek(follow + 0x174));
    println!("turned camera: view {view2:?} yaw {yaw2}; M +0x90 {:?}; player +0x1190 {:?}", f32s::<4>(emu, m + 0x90), f32s::<12>(emu, player + 0x1190));
    let mut turned = Vec::new();
    for (slot, v) in [(0x0u64, 1.0f32), (0x4, 1.0)] {
        for k in (0..0x100).step_by(4) {
            emu.f32(frame + k, 0.0);
        }
        emu.f32(frame + slot, v);
        emu.poke_f32s(m + 0x340, &[0.0; 4]);
        emu.call(0x1_407f_0f50, &[m, 0, ctrl, pad], &[]);
        emu.call(0x1_407f_0f50, &[m, 0, ctrl, pad], &[]);
        let sc0: [f32; 4] = f32s(emu, m + 0xc0);
        println!("turned, frame +{slot:#x} = {v}: M+0xc0 {sc0:?}");
        turned.push(json!({"slot": slot, "value": v, "mc0": sc0}));
    }
    // The frame's camera slots from the right stick's vector action 0x52 (FUN_1409668f0 -> +0x28
    // yaw, FUN_140966960 -> +0x2c pitch): a pad object whose vector getter (vtable +0x20)
    // returns (0.6, 0.8), with the snapshot's options and with option bytes 10 / 11 cleared.
    let vec52 = emu.alloc(16);
    emu.f32(vec52, 0.6);
    emu.f32(vec52 + 4, 0.8);
    let getter = emu.stub("vector_action", vec52, None);
    let vt = emu.vtable(&[(0x20, getter)], 0x100);
    let fake = emu.alloc(0x40);
    emu.u64(fake, vt);
    let mut stick = Vec::new();
    for bytes in [[ob[10], ob[11]], [0, 0]] {
        emu.raw_write(opts + 10, &bytes);
        let y = emu.call(0x1_4096_68f0, &[fake], &[]).xmm0;
        let p = emu.call(0x1_4096_6960, &[fake], &[]).xmm0;
        println!("options 10 / 11 = {bytes:?}: stick (0.6, 0.8) -> frame +0x28 yaw {y}, +0x2c pitch {p}");
        stick.push(json!({"options": bytes.to_vec(), "action": [0.6, 0.8], "yaw": y, "pitch": p}));
    }
    emu.raw_write(opts + 10, &[ob[10], ob[11]]);
    let body = json!({
        "function": "0x1407f0f50",
        "options": ob.to_vec(),
        "camera_view": view,
        "camera_yaw": yaw,
        "cases": cases,
        "turned": {"camera_view": view2, "camera_yaw": yaw2, "cases": turned},
        "stick": stick,
    });
    std::fs::write(out.join("lockcam_pad.json"), serde_json::to_string(&body).unwrap()).unwrap();
}

/// Dispatch for main.rs: `lockcam_probe`, `lockcam`.
pub fn run(want: &[&str], new_emu: &dyn Fn() -> Emu, out: &Path) {
    if want.contains(&"lockcam_probe") {
        probe(&mut new_emu());
        pad_probe(&mut new_emu(), out);
    }
    if want.contains(&"lockcam") {
        frames(new_emu, out);
    }
}

/// One lock-on scenario. Per frame: the right stick (M+0x80, pitch / yaw), Wolf's move (metres
/// a frame, world) and the lock point (world) or None for no lock that frame. `at` builds the lock
/// point from Wolf's start position `w`, the snapshot camera's forward `f` and right `r` (X / Z).
struct LockScenario {
    /// The target's matrix moves with the point (else it stays where the scenario starts it:
    /// the point moves on the target, as a lock dummy does in an animation).
    rigid: bool,
    name: &'static str,
    frames: usize,
    dt: f32,
    input: fn(usize, Ctx) -> ([f32; 2], [f32; 3], Option<[f32; 3]>),
}

#[derive(Clone, Copy)]
struct Ctx {
    w: [f32; 3],
    f: [f32; 3],
    r: [f32; 3],
    /// The snapshot's own lock entry point (the one lockable enemy, ~29 m off, 8 m below).
    real: [f32; 3],
}

fn at(c: Ctx, fwd: f32, right: f32, up: f32) -> [f32; 3] {
    std::array::from_fn(|i| c.w[i] + c.f[i] * fwd + c.r[i] * right + if i == 1 { up } else { 0.0 })
}

const NONE: [f32; 2] = [0.0; 2];
const STILL: [f32; 3] = [0.0; 3];

const LOCK_SCENARIOS: [LockScenario; 16] = [
    LockScenario { rigid: true, name: "lock_front_5m", frames: 90, dt: 1.0 / 60.0, input: |_, c| (NONE, STILL, Some(at(c, 5.0, 0.0, 1.2))) },
    LockScenario { rigid: true, name: "lock_right_5m", frames: 120, dt: 1.0 / 60.0, input: |_, c| (NONE, STILL, Some(at(c, 0.0, 5.0, 1.2))) },
    LockScenario { rigid: true, name: "lock_behind_left", frames: 120, dt: 1.0 / 60.0, input: |_, c| (NONE, STILL, Some(at(c, -4.0, -2.0, 1.2))) },
    LockScenario { rigid: true, name: "lock_high", frames: 90, dt: 1.0 / 60.0, input: |_, c| (NONE, STILL, Some(at(c, 6.0, 2.0, 3.0))) },
    LockScenario { rigid: true, name: "lock_low", frames: 90, dt: 1.0 / 60.0, input: |_, c| (NONE, STILL, Some(at(c, 6.0, -1.0, -2.5))) },
    // 4 m round Wolf at 3 m/s (0.75 rad/s), starting ahead.
    LockScenario {
        rigid: true,
        name: "lock_circling",
        frames: 180,
        dt: 1.0 / 60.0,
        input: |i, c| {
            let a = i as f32 / 60.0 * 0.75;
            (NONE, STILL, Some(at(c, 4.0 * a.cos(), 4.0 * a.sin(), 1.2)))
        },
    },
    // Wolf strafes right at 4 m/s (away from the wall about 10 m along +X) round a target 5 m ahead (the target stays put).
    LockScenario {
        rigid: true,
        name: "lock_wolf_strafes",
        frames: 90,
        dt: 1.0 / 60.0,
        input: |_, c| (NONE, std::array::from_fn(|i| c.r[i] * 4.0 / 60.0), Some(at(c, 5.0, 0.0, 1.2))),
    },
    LockScenario { rigid: true, name: "lock_close", frames: 60, dt: 1.0 / 60.0, input: |_, c| (NONE, STILL, Some(at(c, 0.8, 0.3, 1.0))) },
    // The point goes over Wolf's head, ahead to behind, in 2 s.
    LockScenario {
        rigid: true,
        name: "lock_overhead",
        frames: 120,
        dt: 1.0 / 60.0,
        input: |i, c| (NONE, STILL, Some(at(c, 2.0 - 4.0 * i as f32 / 119.0, 0.2, 2.5))),
    },
    LockScenario { rigid: true, name: "lock_real_target", frames: 90, dt: 1.0 / 60.0, input: |_, c| (NONE, STILL, Some(c.real)) },
    LockScenario { rigid: true, name: "lock_right_5m_30fps", frames: 60, dt: 1.0 / 30.0, input: |_, c| (NONE, STILL, Some(at(c, 0.0, 5.0, 1.2))) },
    LockScenario {
        rigid: true,
        name: "lock_then_release",
        frames: 120,
        dt: 1.0 / 60.0,
        input: |i, c| (NONE, STILL, (i < 60).then(|| at(c, 0.0, 5.0, 1.2))),
    },
    LockScenario { rigid: true, name: "lock_with_stick", frames: 60, dt: 1.0 / 60.0, input: |_, c| ([0.5, 1.0], STILL, Some(at(c, 0.0, 5.0, 1.2))) },
    // A target moving away fast (6 m/s along the camera's forward) from 3 m ahead.
    LockScenario {
        rigid: true,
        name: "lock_target_runs_off",
        frames: 120,
        dt: 1.0 / 60.0,
        input: |i, c| (NONE, STILL, Some(at(c, 3.0 + 0.1 * i as f32, -1.0, 1.2))),
    },
    // The lock point jumps 0.5 m up on the target at frame 30 (the CSPositionRecorder average).
    LockScenario {
        rigid: false,
        name: "lock_point_jumps",
        frames: 90,
        dt: 1.0 / 60.0,
        input: |i, c| (NONE, STILL, Some(at(c, 5.0, 0.0, if i < 30 { 1.2 } else { 1.7 }))),
    },
    LockScenario {
        rigid: false,
        name: "lock_point_jumps_30fps",
        frames: 45,
        dt: 1.0 / 30.0,
        input: |i, c| (NONE, STILL, Some(at(c, 5.0, 0.0, if i < 15 { 1.2 } else { 1.7 }))),
    },
];

/// The lock-on camera over many frames: each frame the lock target manager's hand-over
/// FUN_1409c5f20(ChrCam, entry, 0) (as FUN_1409c5fe0 calls it while locked: entry -> the ChrCam's
/// lock helper +0x90, ChrCam +0x10d0 = 1), then FUN_140734790 (-> follow camera +0x2a0 = 1,
/// +0x280 = the lock point, FUN_14073c260). The entry is the snapshot's one lock target entry,
/// moved rigidly (point and matrix translation) to the scenario's lock point. Writes the
/// ChrExFollowCam fields 0x0..0x420 (f32) per frame to lockcam_frames.json.
pub fn frames(new_emu: &dyn Fn() -> Emu, out: &Path) {
    const F: u64 = 0x1_4073_4790;
    const HAND_OVER: u64 = 0x1_409c_5f20;
    const FIELDS: u64 = 0x420;
    let mut scenarios = Vec::new();
    for sc in &LOCK_SCENARIOS {
        let mut emu = new_emu();
        let player = emu.st.snap.as_ref().expect("lockcam needs --snapshot").player;
        let cam = chr_cam(&mut emu);
        let follow = emu.peek_u64(cam + 0x60);
        let world = emu.peek_u64(0x1_43d6_d640);
        let world = emu.peek_u64(world + 0x98);
        let chr_ctrl = emu.peek_u64(player + 0x50);
        let m = match emu.peek_u64(chr_ctrl + 0x358) {
            0 => emu.peek_u64(chr_ctrl + 0x120),
            m => m,
        };
        let bag = emu.peek_u64(player + 0x1ff8);
        let phys = emu.peek_u64(bag + 0x68);
        let man = emu.peek_u64(LOCK_TGT_MAN);
        let entry = emu.peek_u64(man + 0x10);
        assert!(entry != 0, "no lock target entry in the snapshot");
        let pt0: [f32; 3] = f32s(&mut emu, entry);
        let t0: [f32; 3] = [0u64, 1, 2].map(|row| f32::from_le_bytes(emu.peek(entry + 0x1c + 0x10 * row)));
        let w: [f32; 3] = f32s(&mut emu, phys + 0x80);
        // The camera's view on X / Z: its final matrix's +0x30 row (the manipulator's move
        // up comes out along it, pad_probe); the game is left-handed, so right = (f.z, -f.x).
        let z: [f32; 3] = f32s(&mut emu, follow + 0x30);
        let n = (z[0] * z[0] + z[2] * z[2]).sqrt();
        let fwd = [z[0] / n, 0.0, z[2] / n];
        let ctx = Ctx { w, f: fwd, r: [fwd[2], 0.0, -fwd[0]], real: pt0 };
        let mut frames = Vec::new();
        let mut stopped = None;
        let mut first: Option<[f32; 3]> = None;
        for i in 0..sc.frames {
            let (stick, step, target) = (sc.input)(i, ctx);
            emu.poke_f32s(m + 0x80, &stick);
            let pos: [f32; 3] = f32s(&mut emu, phys + 0x80);
            let pos: [f32; 3] = std::array::from_fn(|k| pos[k] + step[k]);
            emu.poke_f32s(phys + 0x80, &pos);
            // The target matrix's translation (its rotation stays the snapshot's).
            let mut frame_t = None;
            if let Some(p) = target {
                emu.poke_f32s(entry, &p);
                let anchor = if sc.rigid || frames.is_empty() { p } else { first.unwrap_or(p) };
                first.get_or_insert(p);
                for row in 0..3usize {
                    emu.poke_f32s(entry + 0x1c + 0x10 * row as u64, &[t0[row] + anchor[row] - pt0[row]]);
                }
                frame_t = Some(std::array::from_fn::<f32, 3, _>(|row| t0[row] + anchor[row] - pt0[row]));
                let r = emu.call(HAND_OVER, &[cam, entry, 0], &[]);
                if r.stopped.is_some() {
                    stopped = r.stopped;
                    break;
                }
            }
            let before: Vec<f32> = (0..FIELDS / 4).map(|k| f32::from_le_bytes(emu.peek(follow + 4 * k))).collect();
            let r = emu.call(F, &[cam, 0, player, world, 1], &[0.0, sc.dt]);
            if r.stopped.is_some() {
                stopped = r.stopped;
                break;
            }
            let after: Vec<f32> = (0..FIELDS / 4).map(|k| f32::from_le_bytes(emu.read(follow + 4 * k))).collect();
            let helper: [f32; 3] = f32s(&mut emu, cam + 0x90 + 0x1020);
            frames.push(json!({"stick": stick, "wolf": pos, "target": target, "target_frame": frame_t, "helper": helper, "before": before, "after": after}));
        }
        let ang = |v: &Value, o: usize| v["after"][o / 4].as_f64().unwrap_or(0.0);
        println!(
            "{}: {} frames, pitch {:.3} -> {:.3}, yaw {:.3} -> {:.3}, stopped {:?}",
            sc.name,
            frames.len(),
            frames.first().map(|v| ang(v, 0x170)).unwrap_or(0.0),
            frames.last().map(|v| ang(v, 0x170)).unwrap_or(0.0),
            frames.first().map(|v| ang(v, 0x174)).unwrap_or(0.0),
            frames.last().map(|v| ang(v, 0x174)).unwrap_or(0.0),
            stopped.map(|(a, e)| (format!("{a:#x}"), e))
        );
        scenarios.push(json!({"name": sc.name, "dt": sc.dt, "frames": frames}));
    }
    let body = json!({"function": format!("{F:#x}"), "fields": format!("ChrExFollowCam 0x0..{FIELDS:#x} as f32"), "scenarios": scenarios});
    std::fs::write(out.join("lockcam_frames.json"), serde_json::to_string(&body).unwrap()).unwrap();
}
