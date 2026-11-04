//! camleft: the follow camera's leftovers (E18) on the Naomori-arena snapshot, frame by frame
//! through FUN_140734790 -> FUN_14073c260 with every cast hooked (camwall's hooks):
//! - `pin_*`: the stick swings the arm into the wall about 9 m along +X, then Wolf is walked into
//!   the wall, so the focus is pinned against it: the fulcrum cast starts at the wall (its 1 cm
//!   floor) and the chased position closes on the focus (the final cast's length goes to 0).
//! - `attach_*`: TAE 152 AttachCameraToNode, as its handler FUN_140b51750 writes it each frame
//!   it runs on the camera's chr: FUN_140832860(DummyPolyID, BlendStrength) -> singleton
//!   DAT_143d5c0d0 +0x60 / +0x64, FUN_140832880(unk1, unk2) -> +0x68 / +0x6c (read back by
//!   FUN_140831190 / FUN_1408311b0 / FUN_1408311a0). Wolf's dummy 143 is recorded too.
//! - `slow_end_*`: TAE 153 with SlowEnd (+0x85, read by FUN_140831e70) on and off.
//! After each camera frame the singleton's per-frame clear and timers run (the block of
//! FUN_140838b50 that taecam runs). -> extracted/emu/camleft_frames.json

use std::path::Path;

use super::camwall::{hook_casts, world_and_filter, Log};
use super::*;

const FRAME: u64 = 0x1_4073_4790;
const FIELDS: u64 = 0x420;
const SINGLETON: u64 = 0x1_43d5_c0d0;
const CLEAR_FROM: u64 = 0x1_4083_8f1e;
const CLEAR_TO: u64 = 0x1_4083_8f84;
const DUMMY_POS: u64 = 0x1_409e_6db0;
const RSI: i32 = 43;
const R14: i32 = 112;
const XMM6: i32 = 128;

#[derive(Clone, Copy, Default)]
struct Ev {
    /// TAE 152: DummyPolyID, BlendStrength, unk1, unk2.
    attach: Option<(i32, f32, f32, f32)>,
    /// TAE 153 SlowStart path: CamDistTargetOverride, p, first frame, SlowEnd.
    slow: Option<(f32, f32, bool, bool)>,
    /// TAE 153 plain path: CamDistTargetOverride, StartInterpolationSpeed, SlowEnd.
    dist: Option<(f32, f32, bool)>,
    stick: [f32; 2],
    step: [f32; 3],
}

struct Scenario {
    name: &'static str,
    frames: usize,
    dt: f32,
    at: [f32; 3],
    ev: fn(usize) -> Ev,
}

fn prog(f: usize, s: usize, e: usize) -> Option<(f32, bool)> {
    (s..e).contains(&f).then(|| (((f - s) as f32 / (e - s) as f32).min(1.0), f == s))
}

fn scenarios() -> Vec<Scenario> {
    vec![
        // The arm swung into the wall (wall_yaw_sweep cuts from frame 103), then Wolf walked
        // along +X into the wall at 1.8 m/s and held there.
        Scenario {
            name: "pin_wall",
            frames: 420,
            dt: 1.0 / 60.0,
            at: [6.0, 0.0, 0.0],
            ev: |f| Ev { stick: if f < 130 { [0.0, 0.5] } else { [0.0; 2] }, step: if (130..330).contains(&f) { [0.03, 0.0, 0.0] } else { [0.0; 3] }, ..Default::default() },
        },
        // The same with the arm swung further (along the wall) and pitched down into the ground.
        Scenario {
            name: "pin_wall_down",
            frames: 360,
            dt: 1.0 / 60.0,
            at: [6.0, 0.0, 0.0],
            ev: |f| Ev { stick: if f < 100 { [-0.6, 0.5] } else { [0.0; 2] }, step: if (100..300).contains(&f) { [0.03, 0.0, 0.0] } else { [0.0; 3] }, ..Default::default() },
        },
        // a000_710100's AttachCameraToNode (dummy 143, 0.5, 0.25, 2.0) over 1.4 s.
        Scenario { name: "attach_710100", frames: 200, dt: 1.0 / 60.0, at: [0.0; 3], ev: |f| Ev { attach: (5..89).contains(&f).then_some((143, 0.5, 0.25, 2.0)), ..Default::default() } },
        // a212_510310's chain: (1.0, 1.0, 1.5), then (0.1, 0.6, 1.0), then nothing.
        Scenario {
            name: "attach_510310",
            frames: 200,
            dt: 1.0 / 60.0,
            at: [0.0; 3],
            ev: |f| Ev { attach: if f < 60 { Some((143, 1.0, 1.0, 1.5)) } else if f < 80 { Some((143, 0.1, 0.6, 1.0)) } else { None }, ..Default::default() },
        },
        // a000_710102's (0.1, 0.5, 2.0) with the right stick held across it, and Wolf walking.
        Scenario {
            name: "attach_stick_walk",
            frames: 180,
            dt: 1.0 / 60.0,
            at: [0.0; 3],
            ev: |f| Ev { attach: (0..90).contains(&f).then_some((143, 0.1, 0.5, 2.0)), stick: if f < 120 { [0.3, 1.0] } else { [0.0; 2] }, step: [0.02, 0.0, 0.01], ..Default::default() },
        },
        // 30 fps.
        Scenario { name: "attach_30fps", frames: 100, dt: 1.0 / 30.0, at: [0.0; 3], ev: |f| Ev { attach: (3..45).contains(&f).then_some((143, 0.5, 0.25, 2.0)), ..Default::default() } },
        // 153 with SlowStart, SlowEnd on and off; the plain path with SlowEnd off.
        Scenario { name: "slow_end_on", frames: 120, dt: 1.0 / 60.0, at: [0.0; 3], ev: |f| Ev { slow: prog(f, 10, 40).map(|(p, first)| (2.2, p, first, true)), ..Default::default() } },
        Scenario { name: "slow_end_off", frames: 120, dt: 1.0 / 60.0, at: [0.0; 3], ev: |f| Ev { slow: prog(f, 10, 40).map(|(p, first)| (2.2, p, first, false)), ..Default::default() } },
        Scenario { name: "plain_slow_end_off", frames: 120, dt: 1.0 / 60.0, at: [0.0; 3], ev: |f| Ev { dist: (5..65).contains(&f).then_some((2.5, 0.5, false)), ..Default::default() } },
        Scenario { name: "plain_slow_end_on", frames: 120, dt: 1.0 / 60.0, at: [0.0; 3], ev: |f| Ev { dist: (5..65).contains(&f).then_some((2.5, 0.5, true)), ..Default::default() } },
    ]
}

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

fn f32s(emu: &mut Emu, at: u64, n: u64) -> Vec<f32> {
    (0..n).map(|i| f32::from_le_bytes(emu.peek(at + 4 * i))).collect()
}

fn poke(emu: &mut Emu, at: u64, bytes: &[u8]) {
    emu.peek::<1>(at);
    emu.raw_write(at, bytes);
}

fn frames(new_emu: &dyn Fn() -> Emu, out: &Path, only: Option<&str>) {
    let mut all = Vec::new();
    for sc in scenarios().into_iter().filter(|s| only.is_none_or(|o| o == s.name)) {
        let mut emu = new_emu();
        let mut log = Box::new(Log::default());
        hook_casts(&mut emu, &mut *log);
        let player = emu.st.snap.as_ref().expect("camleft needs --snapshot").player;
        let cam = chr_cam(&mut emu);
        let follow = emu.peek_u64(cam + 0x60);
        let (world, _) = world_and_filter(&mut emu);
        let chr_ctrl = emu.peek_u64(player + 0x50);
        let m = match emu.peek_u64(chr_ctrl + 0x358) {
            0 => emu.peek_u64(chr_ctrl + 0x120),
            m => m,
        };
        let bag = emu.peek_u64(player + 0x1ff8);
        let phys = emu.peek_u64(bag + 0x68);
        let single = emu.peek_u64(SINGLETON);
        let scratch = emu.alloc(0x100);
        let ctx = emu.alloc(0x20);
        emu.f32(ctx + 8, sc.dt);
        let spot: [f32; 3] = std::array::from_fn(|i| f32::from_le_bytes(emu.peek(phys + 0x80 + 4 * i as u64)));
        emu.poke_f32s(phys + 0x80, &std::array::from_fn::<f32, 3, _>(|i| spot[i] + sc.at[i]));
        let flag348 = emu.peek::<1>(follow + 0x348)[0];
        let mut out_frames = Vec::new();
        for fr in 0..sc.frames {
            let ev = (sc.ev)(fr);
            emu.poke_f32s(m + 0x80, &ev.stick);
            emu.poke_f32s(m + 0xd0, &[0.0, 0.0]);
            let pos: [f32; 3] = std::array::from_fn(|i| f32::from_le_bytes(emu.peek(phys + 0x80 + 4 * i as u64)) + ev.step[i]);
            emu.poke_f32s(phys + 0x80, &pos);
            if let Some((dmy, blend, t, rate)) = ev.attach {
                // FUN_140832860 / FUN_140832880.
                poke(&mut emu, single + 0x60, &dmy.to_le_bytes());
                emu.poke_f32s(single + 0x64, &[blend, t, rate]);
                poke(&mut emu, single + 0x9b, &[1]);
            }
            if let Some((dist, p, first, end)) = ev.slow {
                // FUN_140832830: +0x70, +0x78 p, +0x84 first frame, +0x85 SlowEnd.
                emu.poke_f32s(single + 0x70, &[dist]);
                emu.poke_f32s(single + 0x78, &[p]);
                poke(&mut emu, single + 0x84, &[first as u8, end as u8]);
                poke(&mut emu, single + 0x9b, &[1]);
                emu.poke_f32s(single + 0x7c, &[0.5, 0.1]);
            }
            if let Some((dist, start, end)) = ev.dist {
                // FUN_140832810: +0x70, +0x74, +0x85 SlowEnd; FUN_1408327f0.
                emu.poke_f32s(single + 0x70, &[dist, start]);
                poke(&mut emu, single + 0x85, &[end as u8]);
                poke(&mut emu, single + 0x9b, &[1]);
                emu.poke_f32s(single + 0x7c, &[0.5, 0.1]);
            }
            let before = f32s(&mut emu, follow, FIELDS / 4);
            let single_in = f32s(&mut emu, single, 0xa0 / 4);
            log.casts.clear();
            log.fades.clear();
            proxy::flush_tb(&emu);
            let r = emu.call(FRAME, &[cam, 0, player, world, 1], &[0.0, sc.dt]);
            assert!(r.stopped.is_none(), "{} frame {fr}: stopped {:?}", sc.name, r.stopped);
            let after = f32s(&mut emu, follow, FIELDS / 4);
            emu.u32(scratch + 0xf0, 143);
            emu.call(DUMMY_POS, &[player, scratch + 0x40, scratch + 0xf0], &[]);
            let dmy_pos = f32s(&mut emu, scratch + 0x40, 4);
            let single_cam = f32s(&mut emu, single, 0xa0 / 4);
            run_block(&mut emu, CLEAR_FROM, CLEAR_TO, &[(R14, ctx), (RSI, 0)], &[(XMM6, 0.0)]);
            let single_out = f32s(&mut emu, single, 0xa0 / 4);
            let casts: Vec<Value> = log
                .casts
                .iter()
                .map(|c| json!({"func": c.func, "site": format!("{:#x}", c.ret), "start": &c.start[..3], "dir": &c.dir[..3], "radius": c.radius, "hit": c.hit, "frac": c.frac}))
                .collect();
            out_frames.push(json!({
                "stick": ev.stick, "wolf": pos, "before": before, "after": after, "casts": casts,
                "single_in": single_in, "single_cam": single_cam, "single_out": single_out, "dmy143": dmy_pos,
            }));
        }
        let pick = |v: &Value, o: usize| v["after"][o / 4].as_f64().unwrap_or(f64::NAN);
        let (a, z) = (out_frames.first().unwrap(), out_frames.last().unwrap());
        println!("{}: {} frames, +0x348 {flag348}, yaw {:.3} -> {:.3}, dist {:.3} -> {:.3}", sc.name, out_frames.len(), pick(a, 0x174), pick(z, 0x174), pick(a, 0x214), pick(z, 0x214));
        all.push(json!({"name": sc.name, "dt": sc.dt, "flag348": flag348, "frames": out_frames}));
    }
    let suffix = only.map_or(String::new(), |o| format!("_{o}"));
    let body = json!({
        "function": format!("{FRAME:#x}"),
        "fields": format!("ChrExFollowCam 0x0..{FIELDS:#x} as f32; single = DAT_143d5c0d0 0x0..0xa0 (in / cam / out as taecam); casts as camwall"),
        "scenarios": all,
    });
    std::fs::write(out.join(format!("camleft_frames{suffix}.json")), serde_json::to_string(&body).unwrap()).unwrap();
}

/// Dispatch for main.rs: `camleft` (all scenarios) or `camleft:<name>`.
pub(crate) fn run(new_emu: &dyn Fn() -> Emu, out: &Path, want: &[&str]) {
    for w in want.iter().filter(|w| w.starts_with("camleft")) {
        frames(new_emu, out, w.strip_prefix("camleft:"));
    }
}
