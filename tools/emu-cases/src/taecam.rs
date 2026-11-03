//! The TAE camera events on the follow camera (FUN_140734790 -> FUN_14073c260), frame by frame on
//! the snapshot: 151 CameraLookAtTarget, 153 CameraModule3 (distance), 155 SetLockParamID (param
//! slot) and 156 SetCameraAngle. Each frame writes the camera singleton DAT_143d5c0d0 fields the
//! TAE handlers write (FUN_140b517a0 / FUN_140b51680 / FUN_140b51420 / FUN_140b51490), runs the
//! camera frame, then the singleton's own per-frame clear and timers (FUN_140838b50 from
//! +0x3ce: FUN_140833e40, then +0x44 / +0x68 / +0x7c count down by dt). Writes taecam_frames.json.

use super::*;

/// The camera singleton (DAT_143d5c0d0) and the CameraSetParam manager (DAT_143d59938).
const SINGLETON: u64 = 0x1_43d5_c0d0;
const SET_MAN: u64 = 0x1_43d5_9938;
/// FUN_140838b50 +0x3ce .. +0x434: clear the TAE camera block and count its timers down.
const CLEAR_FROM: u64 = 0x1_4083_8f1e;
const CLEAR_TO: u64 = 0x1_4083_8f84;
/// FUN_14073a1e0(chr, out matrix, &dummy) and FUN_1409e6db0(chr, out position, &dummy).
const DUMMY_MATRIX: u64 = 0x1_4073_a1e0;
const DUMMY_POS: u64 = 0x1_409e_6db0;

/// One frame's TAE camera events, as the handlers write them.
#[derive(Clone, Copy, Default)]
struct Ev {
    /// 151: DummyPolyID, unk1 (focus rate), unk2 (+0x44), unk3 (+0x48), the four limits (deg).
    look: Option<(i32, f32, f32, f32, [f32; 4])>,
    /// 153: CamDistTargetOverride, StartInterpolationSpeed, TransitionKeyframe,
    /// EndInterpolationSpeed, SlowStart (with p and the first-frame flag).
    dist: Option<(f32, f32, f32, f32, Option<(f32, bool)>)>,
    /// 155: LockParamID.
    slot: Option<i32>,
    /// 156: VerticalAngle, HorizontalAngle (deg), InterpolationType, OverrideVertical,
    /// OverrideHorizontal, p, first frame.
    angle: Option<(i32, i32, i32, bool, bool, f32, bool)>,
    /// The right stick (M+0x80: pitch, yaw) and Wolf's yaw (physics +0x74), when set.
    stick: [f32; 2],
    wolf_yaw: Option<f32>,
}

/// An event over frames [s, e): its progress p = (f - s) / (e - s) and the first-frame flag.
fn prog(f: usize, s: usize, e: usize) -> Option<(f32, bool)> {
    (s..e).contains(&f).then(|| (((f - s) as f32 / (e - s) as f32).min(1.0), f == s))
}

struct Scenario {
    name: &'static str,
    frames: usize,
    ev: fn(usize) -> Ev,
}

const LOOK_FULL: [f32; 4] = [-40.0, 70.0, -180.0, 180.0];

const SCENARIOS: [Scenario; 17] = [
    // 153: an override at a start speed, then EndInterpolationSpeed for TransitionKeyframe s.
    Scenario { name: "dist_override", frames: 200, ev: |f| Ev { dist: prog(f, 5, 65).map(|_| (2.5, 0.5, 0.5, 0.1, None)), ..Default::default() } },
    Scenario { name: "dist_start_zero_long_end", frames: 200, ev: |f| Ev { dist: prog(f, 0, 40).map(|_| (2.0, 0.0, 1.0, 1.0, None)), ..Default::default() } },
    Scenario { name: "dist_slow_start", frames: 160, ev: |f| Ev { dist: prog(f, 10, 30).map(|p| (2.2, 0.0, 0.5, 0.1, Some(p))), ..Default::default() } },
    // a200_510000-like: 3.0 at 0.5, then a SlowStart back to 4.0, then nothing.
    Scenario {
        name: "dist_chain",
        frames: 200,
        ev: |f| Ev {
            dist: prog(f, 0, 20).map(|_| (3.0, 0.5, 1.0, 0.1, None)).or(prog(f, 20, 47).map(|_| (1.5, 1.0, 0.0, 0.0, None))).or(prog(f, 47, 60).map(|p| (4.0, 1.0, 0.0, 1.0, Some(p)))),
            ..Default::default()
        },
    },
    // 155: the deathblow slot and back; a switch in the middle of a blend (2 -> 3 -> 1); the
    // death slot.
    Scenario { name: "slot_41", frames: 150, ev: |f| Ev { slot: (5..65).contains(&f).then_some(41), ..Default::default() } },
    Scenario { name: "slot_2_then_3", frames: 150, ev: |f| Ev { slot: if (0..15).contains(&f) { Some(2) } else if (15..50).contains(&f) { Some(3) } else { None }, ..Default::default() } },
    Scenario { name: "slot_8", frames: 200, ev: |f| Ev { slot: (0..90).contains(&f).then_some(8), ..Default::default() } },
    // 156: both angles, the short way; clockwise and counter-clockwise far turns; pitch only;
    // Wolf turning during the event.
    Scenario { name: "angle_both", frames: 60, ev: |f| Ev { angle: prog(f, 5, 35).map(|(p, first)| (20, 90, 0, true, true, p, first)), ..Default::default() } },
    Scenario { name: "angle_cw", frames: 60, ev: |f| Ev { angle: prog(f, 0, 40).map(|(p, first)| (0, -150, 1, false, true, p, first)), ..Default::default() } },
    Scenario { name: "angle_ccw", frames: 60, ev: |f| Ev { angle: prog(f, 0, 40).map(|(p, first)| (0, 150, 2, false, true, p, first)), ..Default::default() } },
    Scenario { name: "angle_pitch_only", frames: 50, ev: |f| Ev { angle: prog(f, 0, 30).map(|(p, first)| (-10, 0, 0, true, false, p, first)), ..Default::default() } },
    Scenario {
        name: "angle_wolf_turns",
        frames: 50,
        ev: |f| Ev { angle: prog(f, 0, 40).map(|(p, first)| (10, 50, 0, true, true, p, first)), wolf_yaw: Some(f as f32 * 0.02), ..Default::default() },
    },
    // 151: the deathblow look-at (dmy 142, rate 0.5, then unk3 0.1 for unk2 1 s); no rate; a
    // narrow yaw cone; a cone with the stick turning against it; a213's -160..-159 cone.
    Scenario { name: "look_rate", frames: 140, ev: |f| Ev { look: (5..45).contains(&f).then_some((142, 0.5, 1.0, 0.1, LOOK_FULL)), ..Default::default() } },
    Scenario { name: "look_no_rate", frames: 120, ev: |f| Ev { look: (0..40).contains(&f).then_some((142, 0.0, 0.5, 0.07, LOOK_FULL)), ..Default::default() } },
    Scenario { name: "look_cone", frames: 120, ev: |f| Ev { look: (0..60).contains(&f).then_some((142, 0.1, 0.0, 0.0, [-20.0, 30.0, -100.0, 60.0])), ..Default::default() } },
    Scenario {
        name: "look_cone_stick",
        frames: 120,
        ev: |f| Ev { look: (0..80).contains(&f).then_some((142, 0.1, 0.0, 0.0, [-20.0, 30.0, -30.0, 30.0])), stick: if f < 60 { [0.5, 1.0] } else { [0.0; 2] }, ..Default::default() },
    },
    Scenario { name: "look_a213", frames: 120, ev: |f| Ev { look: (0..80).contains(&f).then_some((142, 0.5, 1.0, 0.1, [-10.0, 20.0, -160.0, -159.0])), ..Default::default() } },
];

/// Runs a stretch of code of the image (no call frame of its own) until `to`.
fn run_block(emu: &mut Emu, from: u64, to: u64, regs: &[(i32, u64)], xmm: &[(i32, f32)]) {
    for &(r, v) in regs {
        emu.set_reg(r, v);
    }
    for &(r, v) in xmm {
        let b = f32_bits(v);
        unsafe { uc::uc_reg_write(emu.uc, r, b.as_ptr() as _) };
    }
    emu.set_reg(uc::RSP, (STACK + STACK_SIZE - 0x2000) & !0xf);
    emu.st.running = true;
    let r = unsafe { uc::uc_emu_start(emu.uc, from, to, 0, 100_000) };
    emu.st.running = false;
    assert!(r == 0 && emu.reg(uc::RIP) == to, "block {from:#x} stopped at {:#x} (error {r})", emu.reg(uc::RIP));
}

const RSI: i32 = 43;
const R14: i32 = 112;
const XMM6: i32 = 128;

fn f32s(emu: &mut Emu, at: u64, n: u64) -> Vec<f32> {
    (0..n).map(|i| f32::from_le_bytes(emu.peek(at + 4 * i))).collect()
}

/// Pilot taecam: every scenario from the snapshot's state; per frame the events written, the
/// ChrExFollowCam fields 0x0..0x420 before / after, the singleton 0x0..0xa0 (after the camera
/// frame and after the clear), the CameraSetParam manager's blended CameraParam (+8, 0x1e0
/// bytes) and its slot state (+0x3c8..+0x400), and Wolf's dummy 142 (world matrix, position).
pub(crate) fn taecam_frames(new_emu: &dyn Fn() -> Emu, out: &Path) {
    const F: u64 = 0x1_4073_4790;
    const FIELDS: u64 = 0x420;
    let dt = 1.0f32 / 60.0;
    let mut scenarios = Vec::new();
    for sc in &SCENARIOS {
        let mut emu = new_emu();
        let player = emu.st.snap.as_ref().expect("taecam needs --snapshot").player;
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
        let single = emu.peek_u64(SINGLETON);
        let man = emu.peek_u64(SET_MAN);
        let scratch = emu.alloc(0x100);
        let ctx = emu.alloc(0x20);
        emu.f32(ctx + 8, dt);
        let mut frames = Vec::new();
        for f in 0..sc.frames {
            let ev = (sc.ev)(f);
            emu.poke_f32s(m + 0x80, &ev.stick);
            emu.poke_f32s(m + 0xd0, &[0.0, 0.0]);
            if let Some(y) = ev.wolf_yaw {
                emu.poke_f32s(phys + 0x74, &[y]);
            }
            // The handlers' stores (each only while its event runs on the camera's chr).
            if let Some((dmy, rate, after_t, after_rate, lim)) = ev.look {
                // FUN_1408328d0 / 1408328f0 / 1408328a0.
                poke(&mut emu, single + 0x3c, &dmy.to_le_bytes());
                emu.poke_f32s(single + 0x40, &[rate, after_t, after_rate]);
                emu.poke_f32s(single + 0x4c, &lim.map(|d| d * 0.017453292));
                poke(&mut emu, single + 0x5c, &[1]);
                poke(&mut emu, single + 0x9b, &[1]);
            }
            if let Some((dist, start, trans, end, slow)) = ev.dist {
                emu.poke_f32s(single + 0x70, &[dist]);
                match slow {
                    // FUN_140832830: +0x78 p, +0x84 first frame, +0x85 SlowEnd (all true here).
                    Some((p, first)) => {
                        emu.poke_f32s(single + 0x78, &[p]);
                        poke(&mut emu, single + 0x84, &[first as u8, 1]);
                    }
                    // FUN_140832810: +0x74 StartInterpolationSpeed, +0x85.
                    None => {
                        emu.poke_f32s(single + 0x74, &[start]);
                        poke(&mut emu, single + 0x85, &[1]);
                    }
                }
                poke(&mut emu, single + 0x9b, &[1]);
                // FUN_1408327f0: +0x7c TransitionKeyframe, +0x80 EndInterpolationSpeed.
                emu.poke_f32s(single + 0x7c, &[trans, end]);
            }
            if let Some(slot) = ev.slot {
                // FUN_140737430.
                poke(&mut emu, man + 0x3e8, &slot.to_le_bytes());
            }
            if let Some((v, h, interp, ov, oh, p, first)) = ev.angle {
                // FUN_140b51490: type 0/1/2 -> 1/2/3; FUN_1408327a0.
                poke(&mut emu, single + 0x90, &(interp + 1).to_le_bytes());
                poke(&mut emu, single + 0x98, &[first as u8, ov as u8, oh as u8, 1]);
                emu.poke_f32s(single + 0x88, &[v as f32 * 0.017453292, h as f32 * 0.017453292]);
                emu.poke_f32s(single + 0x94, &[p]);
            }
            let before = f32s(&mut emu, follow, FIELDS / 4);
            let before_flags: [u8; 4] = emu.peek(follow + 0x1a0);
            let single_in = f32s(&mut emu, single, 0xa0 / 4);
            let r = emu.call(F, &[cam, 0, player, world, 1], &[0.0, dt]);
            assert!(r.stopped.is_none(), "{} frame {f}: stopped {:?}", sc.name, r.stopped);
            let after = f32s(&mut emu, follow, FIELDS / 4);
            let after_flags: [u8; 4] = emu.peek(follow + 0x1a0);
            let blended = f32s(&mut emu, man + 8, 0x1e0 / 4);
            let man_state: Vec<Value> = (0..14u64)
                .map(|i| {
                    let b: [u8; 4] = emu.peek(man + 0x3c8 + 4 * i);
                    json!([i32::from_le_bytes(b), f32::from_le_bytes(b)])
                })
                .collect();
            // Wolf's dummy 142: the world matrix (rows as the exe stores them) and position.
            emu.u32(scratch + 0xf0, 142);
            emu.call(DUMMY_MATRIX, &[player, scratch, scratch + 0xf0], &[]);
            let dmy_matrix = f32s(&mut emu, scratch, 16);
            emu.call(DUMMY_POS, &[player, scratch + 0x40, scratch + 0xf0], &[]);
            let dmy_pos = f32s(&mut emu, scratch + 0x40, 4);
            let wolf: [f32; 4] = std::array::from_fn(|i| f32::from_le_bytes(emu.peek(phys + 0x70 + 4 * i as u64)));
            let single_cam = f32s(&mut emu, single, 0xa0 / 4);
            let single_bytes: Vec<u8> = (0x84..0xa0u64).map(|o| emu.peek::<1>(single + o)[0]).collect();
            run_block(&mut emu, CLEAR_FROM, CLEAR_TO, &[(R14, ctx), (RSI, 0)], &[(XMM6, 0.0)]);
            let single_out = f32s(&mut emu, single, 0xa0 / 4);
            frames.push(json!({
                "stick": ev.stick,
                "wolf": [f32::from_le_bytes(emu.peek(phys + 0x80)), f32::from_le_bytes(emu.peek(phys + 0x84)), f32::from_le_bytes(emu.peek(phys + 0x88))],
                "wolf_rot": wolf,
                "before": before,
                "after": after,
                "flags_before": before_flags,
                "flags_after": after_flags,
                "single_in": single_in,
                "single_cam": single_cam,
                "single_bytes": single_bytes,
                "single_out": single_out,
                "blended": blended,
                "man": man_state,
                "dmy_matrix": dmy_matrix,
                "dmy_pos": dmy_pos,
            }));
        }
        let pick = |v: &Value, o: usize| v["after"][o / 4].as_f64().unwrap_or(f64::NAN);
        let (a, z) = (frames.first().unwrap(), frames.last().unwrap());
        println!(
            "{}: {} frames, dist {:.3} -> {:.3}, yaw {:.3} -> {:.3}, pitch {:.3} -> {:.3}, limits {:.3} {:.3}",
            sc.name,
            frames.len(),
            pick(a, 0x214),
            pick(z, 0x214),
            pick(a, 0x174),
            pick(z, 0x174),
            pick(a, 0x170),
            pick(z, 0x170),
            pick(z, 0x230),
            pick(z, 0x234)
        );
        scenarios.push(json!({"name": sc.name, "dt": dt, "frames": frames}));
    }
    let body = json!({
        "function": format!("{F:#x}"),
        "fields": format!("ChrExFollowCam 0x0..{FIELDS:#x} as f32; single = DAT_143d5c0d0 0x0..0xa0 (in: before the frame, cam: after it, out: after FUN_140838b50's clear); blended = [DAT_143d59938] +8"),
        "scenarios": scenarios,
    });
    std::fs::write(out.join("taecam_frames.json"), serde_json::to_string(&body).unwrap()).unwrap();
}

/// Writes bytes into the game's memory (mapping the page from the snapshot first).
fn poke(emu: &mut Emu, at: u64, bytes: &[u8]) {
    emu.peek::<1>(at);
    emu.raw_write(at, bytes);
}
