//! camera2: the follow camera's remaining wall cases and the lock-on manipulator, on the
//! Naomori-arena snapshot.
//!
//! - `camera2_walls`: FUN_140734790 -> FUN_14073c260 with every cast hooked (camwall's hooks):
//!   the escape-wall option on (option byte +0xd -> UseEscapeWall DAT_143d599c0) at the wall about
//!   9 m along +X and at the ground, Wolf walked and put through that wall (the focus pull-in,
//!   FUN_14094cf90 ray + sphere cast), the wall at 30 fps, and lock-on at the wall and the ground
//!   (a target handed over each frame as `lockcam` does). -> camera2_walls.json
//! - `camera2_manip`: Wolf's whole PlayerIns update (FUN_140a37560, as `turn`) with a lock point
//!   in his lock module (bag +0x108, +0x38 -> the point, read by FUN_140b2dab0), so the pad
//!   manipulator takes its lock-on path FUN_1407f1710. -> camera2_manip.json

use std::path::Path;

use serde_json::{json, Value};

use super::camwall::{hook_casts, world_and_filter, Log};
use super::turn::{fd4_time, wolf, PLAYER_UPDATE};
use super::{chr_cam, Emu};

const FRAME: u64 = 0x1_4073_4790;
const HAND_OVER: u64 = 0x1_409c_5f20;
const LOCK_TGT_MAN: u64 = 0x1_43d7_8058;
const FIELDS: u64 = 0x420;

fn f32s<const N: usize>(emu: &mut Emu, at: u64) -> [f32; N] {
    std::array::from_fn(|i| f32::from_le_bytes(emu.peek(at + 4 * i as u64)))
}

/// One scenario. Per frame: the right stick (M+0x80: pitch, yaw), Wolf's move (metres a frame,
/// world; `jump` puts him there instead) and the lock point as an offset from Wolf's start (world
/// X / Y / Z), None for no lock that frame. `at` puts Wolf there before the first frame (offset
/// from the snapshot's spot); `escape` sets the escape-wall option.
struct Scenario {
    name: &'static str,
    frames: usize,
    dt: f32,
    at: [f32; 3],
    escape: bool,
    input: fn(usize) -> Input,
}

#[derive(Default)]
struct Input {
    stick: [f32; 2],
    step: [f32; 3],
    jump: Option<[f32; 3]>,
    lock: Option<[f32; 3]>,
}

fn stick(s: [f32; 2]) -> Input {
    Input { stick: s, ..Default::default() }
}
fn walk(step: [f32; 3]) -> Input {
    Input { step, ..Default::default() }
}

fn scenarios() -> Vec<Scenario> {
    vec![
        // Escape-wall option on: the stick swings the arm through the wall, Wolf walks to it,
        // the stick pitches the camera to the ground and back.
        Scenario { name: "esc_wall_yaw_sweep", frames: 360, dt: 1.0 / 60.0, at: [6.0, 0.0, 0.0], escape: true, input: |f| stick(if f < 300 { [0.0, 0.5] } else { [0.0; 2] }) },
        Scenario { name: "esc_wall_walk_x", frames: 300, dt: 1.0 / 60.0, at: [0.0; 3], escape: true, input: |f| walk(if f < 230 { [0.04, 0.0, 0.0] } else { [0.0; 3] }) },
        Scenario { name: "esc_ground_down_up", frames: 160, dt: 1.0 / 60.0, at: [0.0; 3], escape: true, input: |f| stick(if f < 60 { [-1.0, 0.0] } else if f < 100 { [1.0, 0.0] } else { [0.0; 2] }) },
        // Focus pull-in: Wolf walked through the wall at 6 m/s (the lagging focus crosses it
        // after him), and put across it in one frame.
        Scenario { name: "pull_walk_through_wall", frames: 150, dt: 1.0 / 60.0, at: [6.0, 0.0, 0.0], escape: false, input: |f| walk(if f < 70 { [0.1, 0.0, 0.0] } else { [0.0; 3] }) },
        Scenario {
            name: "pull_jump_across_wall",
            frames: 120,
            dt: 1.0 / 60.0,
            at: [6.0, 0.0, 0.0],
            escape: false,
            input: |f| Input { jump: (f == 20).then_some([12.0, 0.0, 0.0]), ..Default::default() },
        },
        Scenario {
            name: "pull_jump_back",
            frames: 120,
            dt: 1.0 / 60.0,
            at: [12.0, 0.0, 0.0],
            escape: false,
            input: |f| Input { jump: (f == 30).then_some([6.0, 0.0, 0.0]), ..Default::default() },
        },
        // 30 fps at the wall (the position chase once a frame).
        Scenario { name: "wall_yaw_sweep_30fps", frames: 180, dt: 1.0 / 30.0, at: [6.0, 0.0, 0.0], escape: false, input: |f| stick(if f < 150 { [0.0, 0.5] } else { [0.0; 2] }) },
        Scenario {
            name: "wall_squeeze_30fps",
            frames: 200,
            dt: 1.0 / 30.0,
            at: [6.0, 0.0, 0.0],
            escape: false,
            input: |f| Input {
                stick: if f < 80 { [0.0, 0.5] } else { [0.0; 2] },
                step: if (80..125).contains(&f) {
                    [0.04, 0.0, 0.0]
                } else if (125..165).contains(&f) {
                    [-0.08, 0.0, 0.0]
                } else {
                    [0.0; 3]
                },
                ..Default::default()
            },
        },
        // Locked on by the wall: the target 5 m along -X (away from the wall) puts the camera
        // behind Wolf toward the wall; Wolf walks to the wall while locked; a target circling
        // Wolf sweeps the arm through the wall; a target high overhead tips the camera to the
        // ground; the wall at 30 fps.
        Scenario { name: "lock_wall_behind", frames: 180, dt: 1.0 / 60.0, at: [7.5, 0.0, 0.0], escape: false, input: |_| Input { lock: Some([-5.0, 1.2, 0.0]), ..Default::default() } },
        Scenario {
            name: "lock_walk_to_wall",
            frames: 240,
            dt: 1.0 / 60.0,
            at: [4.0, 0.0, 0.0],
            escape: false,
            input: |f| Input { step: if f < 150 { [0.03, 0.0, 0.0] } else { [0.0; 3] }, lock: Some([-3.0, 1.2, 0.0]), ..Default::default() },
        },
        Scenario {
            name: "lock_wall_circling",
            frames: 300,
            dt: 1.0 / 60.0,
            at: [6.5, 0.0, 0.0],
            escape: false,
            input: |f| {
                let a = f as f32 / 60.0 * 0.9;
                Input { lock: Some([-4.0 * a.cos(), 1.2, 4.0 * a.sin()]), ..Default::default() }
            },
        },
        Scenario { name: "lock_ground_overhead", frames: 150, dt: 1.0 / 60.0, at: [0.0; 3], escape: false, input: |_| Input { lock: Some([0.0, 6.0, -1.5]), ..Default::default() } },
        Scenario { name: "lock_wall_behind_30fps", frames: 90, dt: 1.0 / 30.0, at: [7.5, 0.0, 0.0], escape: false, input: |_| Input { lock: Some([-5.0, 1.2, 0.0]), ..Default::default() } },
        Scenario { name: "lock_wall_escape", frames: 180, dt: 1.0 / 60.0, at: [7.5, 0.0, 0.0], escape: true, input: |_| Input { lock: Some([-5.0, 1.2, 0.0]), ..Default::default() } },
    ]
}

fn walls(new_emu: &dyn Fn() -> Emu, out: &Path, only: Option<&str>) {
    let mut all = Vec::new();
    for sc in scenarios().into_iter().filter(|s| only.is_none_or(|o| o == s.name)) {
        let mut emu = new_emu();
        let mut log = Box::new(Log::default());
        hook_casts(&mut emu, &mut *log);
        let player = emu.st.snap.as_ref().expect("camera2 needs --snapshot").player;
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
        let gdm = emu.peek_u64(0x1_43d5_aac0);
        let opts = emu.peek_u64(gdm + 0x50);
        emu.peek::<1>(opts + 0xd);
        emu.raw_write(opts + 0xd, &[sc.escape as u8]);
        let man = emu.peek_u64(LOCK_TGT_MAN);
        let entry = emu.peek_u64(man + 0x10);
        let pt0: [f32; 3] = f32s(&mut emu, entry);
        let t0: [f32; 3] = [0u64, 1, 2].map(|row| f32::from_le_bytes(emu.peek(entry + 0x1c + 0x10 * row)));
        let spot: [f32; 3] = f32s(&mut emu, phys + 0x80);
        let start: [f32; 3] = std::array::from_fn(|i| spot[i] + sc.at[i]);
        emu.poke_f32s(phys + 0x80, &start);
        let mut frames = Vec::new();
        let (mut stopped, mut hits) = (None, 0);
        for fr in 0..sc.frames {
            let inp = (sc.input)(fr);
            emu.poke_f32s(m + 0x80, &inp.stick);
            emu.poke_f32s(m + 0xd0, &[0.0, 0.0]);
            let pos: [f32; 3] = f32s(&mut emu, phys + 0x80);
            let pos: [f32; 3] = match inp.jump {
                Some(j) => std::array::from_fn(|i| spot[i] + j[i]),
                None => std::array::from_fn(|i| pos[i] + inp.step[i]),
            };
            emu.poke_f32s(phys + 0x80, &pos);
            let target = inp.lock.map(|o| std::array::from_fn::<f32, 3, _>(|i| start[i] + o[i]));
            if let Some(p) = target {
                emu.poke_f32s(entry, &p);
                for row in 0..3usize {
                    emu.poke_f32s(entry + 0x1c + 0x10 * row as u64, &[t0[row] + p[row] - pt0[row]]);
                }
                let r = emu.call(HAND_OVER, &[cam, entry, 0], &[]);
                if r.stopped.is_some() {
                    stopped = r.stopped;
                    break;
                }
            }
            let before: Vec<f32> = (0..FIELDS / 4).map(|i| f32::from_le_bytes(emu.peek(follow + 4 * i))).collect();
            log.casts.clear();
            log.fades.clear();
            let r = emu.call(FRAME, &[cam, 0, player, world, 1], &[0.0, sc.dt]);
            if r.stopped.is_some() {
                stopped = r.stopped;
                break;
            }
            let after: Vec<f32> = (0..FIELDS / 4).map(|i| f32::from_le_bytes(emu.read(follow + 4 * i))).collect();
            let casts: Vec<Value> = log
                .casts
                .iter()
                .map(|c| {
                    hits += c.hit.unwrap_or(0).min(1) as usize;
                    let frac = if c.func.starts_with("focus") { None } else { c.frac };
                    json!({"func": c.func, "site": format!("{:#x}", c.ret), "start": &c.start[..3], "dir": &c.dir[..3], "radius": c.radius, "hit": c.hit, "frac": frac})
                })
                .collect();
            let frame_t = target.map(|p| std::array::from_fn::<f32, 3, _>(|row| t0[row] + p[row] - pt0[row]));
            frames.push(json!({"stick": inp.stick, "mouse": [0.0, 0.0], "wolf": pos, "target": target, "target_frame": frame_t, "before": before, "after": after, "casts": casts}));
        }
        let sites: std::collections::BTreeSet<String> = frames.iter().flat_map(|f| f["casts"].as_array().unwrap().iter().map(|c| format!("{}@{}", c["func"].as_str().unwrap(), c["site"].as_str().unwrap()))).collect();
        println!("{}: {} frames, {hits} cast hits, stopped {:?}; casts {sites:?}", sc.name, frames.len(), stopped.map(|(a, e)| (format!("{a:#x}"), e)));
        all.push(json!({"name": sc.name, "dt": sc.dt, "escape": sc.escape, "frames": frames}));
    }
    let body = json!({
        "function": format!("{FRAME:#x} -> 0x14073c260"),
        "fields": format!("ChrExFollowCam 0x0..{FIELDS:#x} as f32"),
        "casts": "per frame, in call order: func, return site, start, cast vector (dir), radius, hit (al), frac (fraction of dir at the hit; none for the focus ray)",
        "scenarios": all,
    });
    std::fs::write(out.join("camera2_walls.json"), serde_json::to_string(&body).unwrap()).unwrap();
}

/// A lock-on manipulator scenario: the pad stick ([0] forward, [1] side) and the lock point
/// (offset from Wolf's start, world) per frame, None = no lock point.
struct ManipScenario {
    name: &'static str,
    dt: f32,
    frames: usize,
    input: fn(usize) -> ([f32; 2], Option<[f32; 3]>),
}

const MANIP: [ManipScenario; 9] = [
    // Standing, the target 5 m off to +X: the facing turns to it.
    ManipScenario { name: "lock_idle_target_x", dt: 1.0 / 60.0, frames: 40, input: |_| ([0.0, 0.0], Some([5.0, 1.0, 0.0])) },
    ManipScenario { name: "lock_idle_target_behind", dt: 1.0 / 60.0, frames: 40, input: |_| ([0.0, 0.0], Some([0.0, 1.0, 5.0])) },
    // A press from rest while locked (the free manipulator takes it a frame late).
    ManipScenario { name: "lock_press_forward", dt: 1.0 / 60.0, frames: 40, input: |f| (if f < 5 { [0.0, 0.0] } else { [1.0, 0.0] }, Some([0.0, 1.0, -5.0])) },
    // Strafing round a target ahead; a flick from forward to the side (the free gate's let-go).
    ManipScenario { name: "lock_strafe", dt: 1.0 / 60.0, frames: 60, input: |_| ([0.0, 1.0], Some([0.0, 1.0, -5.0])) },
    ManipScenario { name: "lock_flick", dt: 1.0 / 60.0, frames: 40, input: |f| (if f < 15 { [1.0, 0.0] } else { [0.0, -1.0] }, Some([0.0, 1.0, -5.0])) },
    ManipScenario { name: "lock_half_diagonal", dt: 1.0 / 60.0, frames: 40, input: |_| ([0.5, 0.5], Some([3.0, 0.5, -3.0])) },
    // The target high above and below (the angles' pitch).
    ManipScenario { name: "lock_target_high", dt: 1.0 / 60.0, frames: 30, input: |_| ([0.0, 0.0], Some([2.0, 4.0, -2.0])) },
    ManipScenario { name: "lock_30fps", dt: 1.0 / 30.0, frames: 30, input: |_| ([0.0, 1.0], Some([5.0, 1.0, 0.0])) },
    // Locked, then the lock let go mid-strafe (the free path again, with its gate).
    ManipScenario { name: "lock_then_free", dt: 1.0 / 60.0, frames: 40, input: |f| ([0.0, 1.0], (f < 20).then_some([0.0, 1.0, -5.0])) },
];

fn manip(new_emu: &dyn Fn() -> Emu, out: &Path) {
    let mut scenarios = Vec::new();
    for sc in &MANIP {
        let mut emu = new_emu();
        let w = wolf(&mut emu);
        let ctrl = emu.peek_u64(w.player + 0x50);
        let bt = emu.peek_u64(ctrl + 0x250);
        let dc = emu.peek_u64(bt + 0x50);
        let bag = emu.peek_u64(w.player + 0x1ff8);
        let lock_mod = emu.peek_u64(bag + 0x108);
        assert!(lock_mod != 0, "no lock module at bag +0x108");
        let lock_was = emu.peek_u64(lock_mod + 0x38);
        let point = emu.alloc(16);
        let start: [f32; 3] = f32s(&mut emu, w.phys + 0x80);
        let mut frames = Vec::new();
        let mut stopped = None;
        for f in 0..sc.frames {
            let (stick, lock) = (sc.input)(f);
            emu.poke_f32s(w.stick, &stick);
            let target = lock.map(|o| std::array::from_fn::<f32, 3, _>(|i| start[i] + o[i]));
            match target {
                Some(p) => {
                    emu.poke_f32s(point, &[p[0], p[1], p[2], 1.0]);
                    emu.u64(lock_mod + 0x38, point);
                }
                None => emu.u64(lock_mod + 0x38, 0),
            }
            let t = fd4_time(&mut emu, sc.dt);
            let r = emu.call(PLAYER_UPDATE, &[w.player, t], &[]);
            if r.stopped.is_some() {
                stopped = r.stopped;
                break;
            }
            let g = |emu: &mut Emu, at: u64| -> [f32; 4] { f32s(emu, at) };
            let (m10, m20, m40, m90, mc0, m340) = (g(&mut emu, w.manip + 0x10), g(&mut emu, w.manip + 0x20), g(&mut emu, w.manip + 0x40), g(&mut emu, w.manip + 0x90), g(&mut emu, w.manip + 0xc0), g(&mut emu, w.manip + 0x340));
            let m43c = f32::from_le_bytes(emu.peek(w.manip + 0x43c));
            let lvl: [f32; 2] = f32s(&mut emu, w.beh + 0xbac);
            let pos: [f32; 3] = f32s(&mut emu, w.phys + 0x80);
            let yaw = f32::from_le_bytes(emu.peek(w.phys + 0x74));
            let shown = f32::from_le_bytes(emu.peek(dc + 0x24));
            frames.push(json!({
                "stick": stick, "target": target, "m10": m10, "m20": m20, "m40": m40, "m90": m90, "mc0": mc0, "m340": m340, "m43c": m43c,
                "level": lvl[0], "move_angle": lvl[1], "pos": pos, "yaw": yaw, "shown": shown,
            }));
        }
        let last = |k: &str| frames.last().map_or(0.0, |v: &Value| v[k].as_f64().unwrap_or(f64::NAN));
        println!("{}: {} frames, yaw {:.4} move_angle {:.4} level {:.3}, lock +0x38 was {lock_was:#x}, stopped {:?}", sc.name, frames.len(), last("yaw"), last("move_angle"), last("level"), stopped.map(|(a, e)| (format!("{a:#x}"), e)));
        scenarios.push(json!({"name": sc.name, "dt": sc.dt, "start": start, "frames": frames}));
    }
    let body = json!({"function": format!("{PLAYER_UPDATE:#x} (lock point at [bag+0x108]+0x38)"), "scenarios": scenarios});
    std::fs::write(out.join("camera2_manip.json"), serde_json::to_string(&body).unwrap()).unwrap();
}

/// Dispatch for main.rs: `camera2_walls[:name]`, `camera2_manip`.
pub(crate) fn run(new_emu: &dyn Fn() -> Emu, out: &Path, want: &[&str]) {
    if let Some(w) = want.iter().find(|w| w.starts_with("camera2_walls")) {
        walls(new_emu, out, w.strip_prefix("camera2_walls:"));
    }
    if want.contains(&"camera2_manip") {
        manip(new_emu, out);
    }
}
