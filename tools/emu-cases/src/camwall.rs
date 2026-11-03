//! camwall: the follow camera at walls and the ground. FUN_14073c260 (the ChrCam update, run
//! through the frame function FUN_140734790 as `camera_frames` does) sphere-casts from the focus
//! (+0xc0) through the physics world [[0x143d6d640]+0x98]: toward the fulcrum (+0xe0), the orbit
//! point (+0xf0), the chased position (+0x100) and, for the obstacle fade, from +0x100 toward
//! Wolf. Every cast's arguments and result (hit, fraction of the cast vector) are recorded per
//! frame next to the ChrExFollowCam fields, so the sim test can feed RSekiro the game's own cast
//! results (the map collision stays RSekiro's).
//!
//!   emu-cases <exe> <out> camwall_probe --snapshot <dir>   casts around Wolf's spot (where the walls are)
//!   emu-cases <exe> <out> camwall --snapshot <dir>         the scenarios -> camwall_frames.json

use std::ffi::c_void;
use std::path::Path;

use serde_json::{json, Value};

use super::{chr_cam, uc, Emu};

/// The frame function (FUN_140734790) and the ChrCam update it calls (FUN_14073c260, 22,857 B).
const FRAME: u64 = 0x1_4073_4790;
const UPDATE: u64 = 0x1_4073_c260;
const UPDATE_SIZE: usize = 22_857;
/// The casts FUN_14073c260 calls. FUN_14094dc70 / FUN_14094dfc0 (world, filter, &start, &dir,
/// radius, out point, out normal, &fraction) are Havok linear sphere casts (dfc0 with another
/// collector, used while ChrExFollowCam +0x3ad is set); FUN_14094ea80 (world, collector,
/// group 0x76, &start, &dir, radius) is the obstacle fade's all-hits cast; FUN_14094cf90 /
/// FUN_14094d140 the ray of the focus pull-in (Wolf -> the chased focus).
const CASTS: [(u64, &str); 5] = [
    (0x1_4094_dc70, "sphere"),
    (0x1_4094_dfc0, "sphere_b"),
    (0x1_4094_ea80, "fade"),
    (0x1_4094_cf90, "focus"),
    (0x1_4094_d140, "focus_b"),
];
/// The fade request of a hit obstacle (FUN_140b21d40, CSCamFadeSlot manager at camera +0x3b0).
const FADE_REQUEST: u64 = 0x1_40b2_1d40;
const FIELDS: u64 = 0x420;

#[derive(Default)]
struct Cast {
    func: &'static str,
    ret: u64,
    start: [f32; 4],
    dir: [f32; 4],
    radius: f32,
    frac_at: u64,
    hit: Option<u8>,
    frac: Option<f32>,
}

#[derive(Default)]
struct Log {
    casts: Vec<Cast>,
    /// Each fade request's hit: (kind at +8: 1 = character, 6 = object; the character's vtable or
    /// the object's +0x58 / +0x5b).
    fades: Vec<(u16, u64)>,
}

fn reg(e: uc::Engine, id: i32) -> u64 {
    let mut v = 0u64;
    unsafe { uc::uc_reg_read(e, id, &mut v as *mut u64 as _) };
    v
}
fn mem<const N: usize>(e: uc::Engine, at: u64) -> [u8; N] {
    let mut b = [0u8; N];
    unsafe { uc::uc_mem_read(e, at, b.as_mut_ptr() as _, N) };
    b
}
fn q(e: uc::Engine, at: u64) -> u64 {
    u64::from_le_bytes(mem(e, at))
}
fn f(e: uc::Engine, at: u64) -> f32 {
    f32::from_le_bytes(mem(e, at))
}
fn v4(e: uc::Engine, at: u64) -> [f32; 4] {
    std::array::from_fn(|i| f(e, at + 4 * i as u64))
}

/// At a cast's entry: its arguments (Windows x64: rcx, rdx, r8, r9, then the stack from +0x28).
extern "C" fn on_cast(e: uc::Engine, addr: u64, _size: u32, data: *mut c_void) {
    let log = unsafe { &mut *(data as *mut Log) };
    let Some(&(_, func)) = CASTS.iter().find(|c| c.0 == addr) else { return };
    let sp = reg(e, uc::RSP);
    let mut c = Cast { func, ret: q(e, sp), ..Default::default() };
    if func == "fade" {
        c.start = v4(e, reg(e, uc::R9));
        c.dir = v4(e, q(e, sp + 0x28));
        c.radius = f(e, sp + 0x30);
    } else {
        c.start = v4(e, reg(e, uc::R8));
        c.dir = v4(e, reg(e, uc::R9));
        c.radius = f(e, sp + 0x28);
        c.frac_at = q(e, sp + 0x40);
    }
    log.casts.push(c);
}

/// At the instruction after a cast's call in FUN_14073c260: its result (al) and the fraction.
extern "C" fn on_return(e: uc::Engine, addr: u64, _size: u32, data: *mut c_void) {
    let log = unsafe { &mut *(data as *mut Log) };
    if let Some(c) = log.casts.iter_mut().rev().find(|c| c.ret == addr && c.hit.is_none()) {
        let hit = reg(e, uc::RAX) as u8;
        c.hit = Some(hit);
        if hit != 0 && c.frac_at != 0 {
            c.frac = Some(f(e, c.frac_at));
        }
    }
}

extern "C" fn on_fade(e: uc::Engine, _addr: u64, _size: u32, data: *mut c_void) {
    let log = unsafe { &mut *(data as *mut Log) };
    let hit = reg(e, uc::RDX);
    if hit == 0 {
        return;
    }
    let kind = u16::from_le_bytes(mem(e, hit + 8));
    let owner = q(e, hit + 0x10);
    let what = match (kind, owner) {
        (_, 0) => 0,
        (1, o) => q(e, o),
        (_, o) => (u16::from_le_bytes(mem(e, o + 0x58)) as u64) | ((mem::<1>(e, o + 0x5b)[0] as u64) << 16),
    };
    log.fades.push((kind, what));
}

/// Hooks the casts and their return sites in FUN_14073c260 (direct calls found in its bytes).
fn hook_casts(emu: &mut Emu, log: *mut Log) -> Vec<u64> {
    let add = |emu: &mut Emu, cb: *const c_void, at: u64| {
        let mut h = 0usize;
        let r = unsafe { uc::uc_hook_add(emu.uc, &mut h, uc::HOOK_CODE, cb, log as *mut c_void, at, at) };
        assert_eq!(r, 0, "hook at {at:#x}");
    };
    for (at, _) in CASTS {
        add(emu, on_cast as *const c_void, at);
    }
    add(emu, on_fade as *const c_void, FADE_REQUEST);
    let mut code = vec![0u8; UPDATE_SIZE];
    unsafe { uc::uc_mem_read(emu.uc, UPDATE, code.as_mut_ptr() as _, UPDATE_SIZE) };
    let mut sites = Vec::new();
    for i in 0..UPDATE_SIZE - 5 {
        if code[i] != 0xe8 {
            continue;
        }
        let rel = i32::from_le_bytes(code[i + 1..i + 5].try_into().unwrap()) as i64;
        let next = UPDATE + i as u64 + 5;
        let to = (next as i64 + rel) as u64;
        if CASTS.iter().any(|c| c.0 == to) {
            add(emu, on_return as *const c_void, next);
            sites.push(next);
        }
    }
    sites
}

/// The physics world (param_4 of FUN_14073c260) and the camera's cast filter
/// ([WorldChrMan 0x143d7a1e0] +0x3550).
fn world_and_filter(emu: &mut Emu) -> (u64, u64) {
    let w = emu.peek_u64(0x1_43d6_d640);
    let world = emu.peek_u64(w + 0x98);
    let wcm = emu.peek_u64(0x1_43d7_a1e0);
    let filter = u32::from_le_bytes(emu.peek(wcm + 0x3550)) as u64;
    (world, filter)
}

/// camwall_probe: sphere casts (FUN_14094dc70, radius 0.05) from Wolf's focus in 24 flat
/// directions (15 m) and 8 directions 40 deg down (8 m): where the walls and the ground are.
fn probe(emu: &mut Emu) {
    let cam = chr_cam(emu);
    let follow = emu.peek_u64(cam + 0x60);
    let focus: [f32; 4] = std::array::from_fn(|i| f32::from_le_bytes(emu.peek(follow + 0xc0 + 4 * i as u64)));
    let (world, filter) = world_and_filter(emu);
    println!("focus {focus:?}, filter {filter:#x}");
    let start = emu.alloc(16);
    let dir = emu.alloc(16);
    let frac = emu.alloc(16);
    let point = emu.alloc(16);
    let normal = emu.alloc(16);
    for (n, len, down) in [(24, 15.0f32, 0.0f32), (8, 8.0, 40.0), (8, 8.0, 70.0)] {
        for k in 0..n {
            let a = (k as f32) * std::f32::consts::TAU / n as f32;
            let (s, c) = a.sin_cos();
            let (ds, dc) = down.to_radians().sin_cos();
            let d = [s * dc * len, -ds * len, c * dc * len, 0.0];
            emu.raw_write(start, &focus.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<_>>());
            emu.raw_write(dir, &d.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<_>>());
            emu.raw_write(frac, &[0u8; 4]);
            let r = emu.call(CASTS[0].0, &[world, filter, start, dir, 0.05f32.to_bits() as u64, point, normal, frac], &[]);
            let t = emu.rf32(frac);
            let nrm: [f32; 3] = std::array::from_fn(|i| emu.rf32(normal + 4 * i as u64));
            println!(
                "  heading {:5.1} deg (x {:+.2} z {:+.2}) down {down:4.1}: {}",
                a.to_degrees(),
                s,
                c,
                if r.rax as u8 != 0 { format!("hit at {:.2} m, normal {nrm:.2?}", t * len) } else { "clear".into() }
            );
        }
    }
}

/// One scenario: per frame the right stick (M+0x80: pitch, yaw), the mouse (M+0xd0, degrees)
/// and Wolf's move (metres a frame); `at` puts Wolf there (offset from the snapshot's spot)
/// before the first frame; `escape` sets the option byte that the frame function copies into
/// UseEscapeWall (DAT_143d599c0, option +0xd).
struct Scenario {
    name: &'static str,
    frames: usize,
    dt: f32,
    at: [f32; 3],
    escape: bool,
    input: fn(usize) -> ([f32; 2], [f32; 2], [f32; 3]),
}

const NONE: [f32; 2] = [0.0; 2];
const STILL: [f32; 3] = [0.0; 3];

fn scenarios() -> Vec<Scenario> {
    vec![
        // The stick pitches the camera down to the ground (pitch -1), holds it there, lets go.
        Scenario { name: "ground_pitch_down", frames: 150, dt: 1.0 / 60.0, at: [0.0; 3], escape: false, input: |f| (if f < 80 { [-1.0, 0.0] } else { NONE }, NONE, STILL) },
        // Down to the ground, then back up: the arm grows back at the hit-wall rates.
        Scenario { name: "ground_down_then_up", frames: 160, dt: 1.0 / 60.0, at: [0.0; 3], escape: false, input: |f| (if f < 60 { [-1.0, 0.0] } else if f < 100 { [1.0, 0.0] } else { NONE }, NONE, STILL) },
        Scenario { name: "ground_pitch_down_30fps", frames: 75, dt: 1.0 / 30.0, at: [0.0; 3], escape: false, input: |f| (if f < 40 { [-1.0, 0.0] } else { NONE }, NONE, STILL) },
        // Half down while turning: the arm slides along the ground.
        Scenario { name: "ground_down_and_yaw", frames: 150, dt: 1.0 / 60.0, at: [0.0; 3], escape: false, input: |f| (if f < 120 { [-0.7071, 0.7071] } else { NONE }, NONE, STILL) },
        // The wall about 9-10 m along +X (camwall_probe: hits at 7.2-9.4 m on headings 60-90 deg,
        // normal about -X): Wolf walks there at 2.4 m/s with the camera behind him (+Z). The
        // fulcrum trails the sideways walk, so the camera swings off the wall; only the obstacle
        // fade's cast hits (a map object, frames 64-83).
        Scenario { name: "wall_walk_x", frames: 300, dt: 1.0 / 60.0, at: [0.0; 3], escape: false, input: |f| (NONE, NONE, if f < 230 { [0.04, 0.0, 0.0] } else { STILL }) },
        // The same, then back along -X: the arm grows back at the hit-wall distance rates.
        Scenario { name: "wall_walk_x_then_back", frames: 330, dt: 1.0 / 60.0, at: [0.0; 3], escape: false, input: |f| (NONE, NONE, if f < 220 { [0.04, 0.0, 0.0] } else if f < 300 { [-0.06, 0.0, 0.0] } else { STILL }) },
        // Wolf 3.4 m short of the wall; the stick turns the camera round at half speed, so the
        // arm sweeps through the wall and out again.
        Scenario { name: "wall_yaw_sweep", frames: 420, dt: 1.0 / 60.0, at: [6.0, 0.0, 0.0], escape: false, input: |f| (if f < 360 { [0.0, 0.5] } else { NONE }, NONE, STILL) },
        // Near the wall, the camera turned and pitched down: wall and ground together.
        Scenario { name: "wall_corner_down", frames: 240, dt: 1.0 / 60.0, at: [6.0, 0.0, 0.0], escape: false, input: |f| (if f < 180 { [-0.5, 0.5] } else { NONE }, NONE, STILL) },
        // Near the wall the stick swings the arm into it (cut from frame 103 of wall_yaw_sweep),
        // then Wolf walks toward the camera and the wall (1.2 m/s), squeezing the arm, and
        // walks back (2.4 m/s): the arm grows back at the hit-wall distance rates.
        Scenario {
            name: "wall_squeeze",
            frames: 400,
            dt: 1.0 / 60.0,
            at: [6.0, 0.0, 0.0],
            escape: false,
            input: |f| {
                let stick = if f < 160 { [0.0, 0.5] } else { NONE };
                let step = if (160..250).contains(&f) {
                    [0.02, 0.0, 0.0]
                } else if (250..330).contains(&f) {
                    [-0.04, 0.0, 0.0]
                } else {
                    STILL
                };
                (stick, NONE, step)
            },
        },
        // Escape-wall option on: no fulcrum / orbit casts, the wings instead.
        Scenario { name: "ground_pitch_down_escape", frames: 120, dt: 1.0 / 60.0, at: [0.0; 3], escape: true, input: |f| (if f < 80 { [-1.0, 0.0] } else { NONE }, NONE, STILL) },
    ]
}

fn frames(new_emu: &dyn Fn() -> Emu, out: &Path, only: Option<&str>) {
    let mut all = Vec::new();
    for sc in scenarios().into_iter().filter(|s| only.is_none_or(|o| o == s.name)) {
        let mut emu = new_emu();
        let mut log = Box::new(Log::default());
        let sites = hook_casts(&mut emu, &mut *log);
        let player = emu.st.snap.as_ref().expect("camwall needs --snapshot").player;
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
        // The options (GameDataMan [0x143d5aac0] +0x50): +0xd is copied into UseEscapeWall each frame.
        let gdm = emu.peek_u64(0x1_43d5_aac0);
        let opts = emu.peek_u64(gdm + 0x50);
        let escape_was = emu.peek::<1>(opts + 0xd)[0];
        emu.raw_write(opts + 0xd, &[sc.escape as u8]);
        let spot: [f32; 3] = std::array::from_fn(|i| f32::from_le_bytes(emu.peek(phys + 0x80 + 4 * i as u64)));
        emu.poke_f32s(phys + 0x80, &std::array::from_fn::<f32, 3, _>(|i| spot[i] + sc.at[i]));
        let mut out_frames = Vec::new();
        let mut stopped = None;
        let mut hits = 0;
        for fr in 0..sc.frames {
            let (stick, mouse, step) = (sc.input)(fr);
            emu.poke_f32s(m + 0x80, &stick);
            emu.poke_f32s(m + 0xd0, &mouse);
            let pos: [f32; 3] = std::array::from_fn(|i| f32::from_le_bytes(emu.peek(phys + 0x80 + 4 * i as u64)));
            let pos: [f32; 3] = std::array::from_fn(|i| pos[i] + step[i]);
            emu.poke_f32s(phys + 0x80, &pos);
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
                    json!({
                        "func": c.func,
                        "site": format!("{:#x}", c.ret),
                        "start": &c.start[..3],
                        "dir": &c.dir[..3],
                        "radius": c.radius,
                        "hit": c.hit,
                        "frac": c.frac,
                    })
                })
                .collect();
            out_frames.push(json!({"stick": stick, "mouse": mouse, "wolf": pos, "before": before, "after": after, "casts": casts, "fades": log.fades.iter().map(|(k, w)| json!([k, format!("{w:#x}")])).collect::<Vec<_>>()}));
        }
        let first = out_frames.first().map(|f| f["casts"].as_array().unwrap().iter().map(|c| format!("{}@{}", c["func"].as_str().unwrap(), c["site"].as_str().unwrap())).collect::<Vec<_>>());
        println!(
            "{}: {} frames, {} cast hits, option +0xd was {escape_was}, stopped {:?}; frame 0 casts {:?}",
            sc.name,
            out_frames.len(),
            hits,
            stopped.map(|(a, e)| (format!("{a:#x}"), e)),
            first
        );
        all.push(json!({"name": sc.name, "dt": sc.dt, "escape": sc.escape, "frames": out_frames}));
        if sites.is_empty() {
            println!("  no cast call sites found in FUN_14073c260");
        }
        drop(emu);
        drop(log);
    }
    let body = json!({
        "function": format!("{FRAME:#x} -> {UPDATE:#x}"),
        "fields": format!("ChrExFollowCam 0x0..{FIELDS:#x} as f32"),
        "casts": "per frame, in call order: func, return site, start, cast vector (dir), radius, hit (al), frac (fraction of dir at the hit)",
        "scenarios": all,
    });
    std::fs::write(out.join("camwall_frames.json"), serde_json::to_string(&body).unwrap()).unwrap();
}

pub(crate) fn run(new_emu: &dyn Fn() -> Emu, out: &Path, want: &[&str]) {
    if want.contains(&"camwall_probe") {
        probe(&mut new_emu());
    }
    if let Some(w) = want.iter().find(|w| w.starts_with("camwall") && **w != "camwall_probe") {
        frames(new_emu, out, w.strip_prefix("camwall:"));
    }
}
