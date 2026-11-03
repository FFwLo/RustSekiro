//! proxy: Wolf's character proxy (capsule) against the General, the arena's rock and ground, on the
//! snapshot. Each frame runs Wolf's whole chr frame (FUN_140a4b570) with the stick let go; inside
//! it the behaviour world step calls CSHkBehCharaProxySceneModifier::vf3 (FUN_140fe7310), which per
//! character runs the physics module's pre-step (FUN_140bcac10: the proxy is put at the chr's
//! position lifted by +0x140 = +0x188, 0.3 m; FUN_140bcc570: the velocity), the world's proxy pass
//! (FUN_14093bb50 -> FUN_14093cfd0: checkSupport FUN_1418f82a0, then the velocity into proxy +0xb0
//! and hknpCharacterProxy::integrate FUN_1418f8b60 with gravity (0, -9.8, 0)) and the post-step
//! (FUN_141044050 -> FUN_140bc4390 -> FUN_140bca6d0: the proxy read back, lowered, copied to the
//! physics module +0x80). A hook at integrate's entry puts the scenario's requested velocity (x, z)
//! into proxy +0xb0 and keeps the game's y (gravity, the fall), so the move is ours and the
//! collision is the exe's.
//!
//!   emu-cases <exe> <out> proxy_map [dx dz] --snapshot <dir> casts around Wolf's spot
//!   emu-cases <exe> <out> proxy [scenario] --snapshot <dir> -> proxy_frames.json (or proxy_<name>.json)

use std::ffi::c_void;
use std::path::Path;
use std::sync::Mutex;

use serde_json::{json, Value};

use super::{uc, Emu};

/// One character's frame: FUN_140a4b570(WorldChrMan, 0, chr, dt in xmm1).
pub(super) const CHR_FRAME: u64 = 0x1_40a4_b570;
/// hknpCharacterProxy::integrate (FUN_1418f8b60).
pub(super) const INTEGRATE: u64 = 0x1_418f_8b60;
/// The world's linear sphere cast (camwall: FUN_14094dc70).
const CAST: u64 = 0x1_4094_dc70;
/// The proxy wrapper's position setter FUN_14104dfd0 (wrapper, &pos): hknp proxy +0x80, then its body.
const SET_POSITION: u64 = 0x1_4104_dfd0;

extern "C" {
    fn uc_ctl(uc: uc::Engine, control: u32, ...) -> i32;
}

/// Drops Unicorn's translated code between frames (uc_ctl_flush_tb: UC_CTL_TB_FLUSH 10, write).
/// Without it the host crashed on the 39th chr frame of one engine whatever the frame did (the
/// translation cache filling up in the middle of a run); with it 300 frames run.
pub(super) fn flush_tb(emu: &Emu) {
    unsafe { uc_ctl(emu.uc, 10 | (1 << 30)) };
}

/// longjmp (FUN_1426963a0, the CRT's): its unwinding path calls RtlUnwindEx, a stub here, and
/// spun forever. The HKS garbage collector's incremental mark step (FUN_141432a00 ->
/// FUN_141431c70, setjmp; FUN_141432dc0 jumps out when its work budget runs out) uses it every
/// 24 frames or so. The emulator copy gets a plain longjmp in its place (own code, not the game's):
/// rax = the value (1 for 0), rbx rsp rbp rsi rdi r12-r15 and xmm6-15 back from the _JUMP_BUFFER
/// (+0x08..+0x48, +0x60..), jump to its rip (+0x50); no C++ unwinding (the GC is C).
pub(super) const LONGJMP: u64 = 0x1_4269_63a0;
pub(super) const PLAIN_LONGJMP: &[u8] = &[
    0x48, 0x89, 0xd0, 0x48, 0x85, 0xc0, 0x75, 0x05, 0xb8, 0x01, 0x00, 0x00, 0x00, //
    0x48, 0x8b, 0x59, 0x08, 0x48, 0x8b, 0x61, 0x10, 0x48, 0x8b, 0x69, 0x18, 0x48, 0x8b, 0x71, 0x20, //
    0x48, 0x8b, 0x79, 0x28, 0x4c, 0x8b, 0x61, 0x30, 0x4c, 0x8b, 0x69, 0x38, 0x4c, 0x8b, 0x71, 0x40, //
    0x4c, 0x8b, 0x79, 0x48, 0xf3, 0x0f, 0x6f, 0x71, 0x60, 0xf3, 0x0f, 0x6f, 0x79, 0x70, //
    0xf3, 0x44, 0x0f, 0x6f, 0x81, 0x80, 0x00, 0x00, 0x00, 0xf3, 0x44, 0x0f, 0x6f, 0x89, 0x90, 0x00, 0x00, 0x00, //
    0xf3, 0x44, 0x0f, 0x6f, 0x91, 0xa0, 0x00, 0x00, 0x00, 0xf3, 0x44, 0x0f, 0x6f, 0x99, 0xb0, 0x00, 0x00, 0x00, //
    0xf3, 0x44, 0x0f, 0x6f, 0xa1, 0xc0, 0x00, 0x00, 0x00, 0xf3, 0x44, 0x0f, 0x6f, 0xa9, 0xd0, 0x00, 0x00, 0x00, //
    0xf3, 0x44, 0x0f, 0x6f, 0xb1, 0xe0, 0x00, 0x00, 0x00, 0xf3, 0x44, 0x0f, 0x6f, 0xb9, 0xf0, 0x00, 0x00, 0x00, //
    0xff, 0x61, 0x50,
];

fn rd<const N: usize>(e: uc::Engine, at: u64) -> [u8; N] {
    let mut b = [0u8; N];
    unsafe { uc::uc_mem_read(e, at, b.as_mut_ptr() as _, N) };
    b
}
fn v4(e: uc::Engine, at: u64) -> [f32; 4] {
    std::array::from_fn(|i| f32::from_le_bytes(rd(e, at + 4 * i as u64)))
}

/// The integrate hook: the proxy it watches, the velocity (x, z) it puts in, and per call the
/// velocity as the game set it and the (lifted) position.
pub(super) struct Integrate {
    pub proxy: u64,
    pub set: Option<[f32; 2]>,
    pub calls: Vec<([f32; 4], [f32; 4])>,
}
pub(super) static INT: Mutex<Integrate> = Mutex::new(Integrate { proxy: 0, set: None, calls: Vec::new() });

extern "C" fn on_integrate(e: uc::Engine, _addr: u64, _size: u32, _d: *mut c_void) {
    let mut rcx = 0u64;
    unsafe { uc::uc_reg_read(e, uc::RCX, &mut rcx as *mut u64 as _) };
    let mut g = INT.lock().unwrap();
    if rcx != g.proxy {
        return;
    }
    let vel = v4(e, rcx + 0xb0);
    let pos = v4(e, rcx + 0x80);
    if let Some([x, z]) = g.set {
        let b: Vec<u8> = [x, vel[1], z].iter().flat_map(|v| v.to_le_bytes()).collect();
        unsafe { uc::uc_mem_write(e, rcx + 0xb0, b.as_ptr() as _, 12) };
    }
    g.calls.push((vel, pos));
}

pub(super) fn f32s(emu: &mut Emu, at: u64, n: usize) -> Vec<f32> {
    (0..n).map(|i| f32::from_le_bytes(emu.peek(at + 4 * i as u64))).collect()
}

/// A chr's FD proxy wrapper (physics module +0xc8, else +0xc0) and its hknpCharacterProxy (+0x40).
pub(super) fn proxy_of(emu: &mut Emu, phys: u64) -> (u64, u64) {
    let w = match emu.peek_u64(phys + 0xc8) {
        0 => emu.peek_u64(phys + 0xc0),
        w => w,
    };
    if w == 0 {
        return (0, 0);
    }
    (w, emu.peek_u64(w + 0x40))
}

pub(super) fn setup(new_emu: &dyn Fn() -> Emu) -> (Emu, super::turn::Wolf, u64) {
    let mut emu = new_emu();
    emu.limit = 200_000_000;
    // The CRT's ISA level (__isa_available, DAT_143c1d520): 5 picks AVX2 string code Unicorn lacks.
    emu.raw_write(0x1_43c1_d520, &1u32.to_le_bytes());
    emu.raw_write(LONGJMP, PLAIN_LONGJMP);
    // AI off in the emulator copy (DebugFlags +0x10); only Wolf's frame runs anyway.
    emu.raw_write(0x1_43d7_a376, &[1]);
    let w = super::turn::wolf(&mut emu);
    let (_, proxy) = proxy_of(&mut emu, w.phys);
    emu.hook(uc::HOOK_CODE, on_integrate as *const c_void, INTEGRATE, INTEGRATE);
    let mut g = INT.lock().unwrap();
    g.proxy = proxy;
    g.set = None;
    g.calls.clear();
    drop(g);
    (emu, w, proxy)
}

/// The world's sphere cast from `from` along `d` (the camera's filter): (fraction, normal).
pub(super) fn cast(emu: &mut Emu, from: [f32; 3], d: [f32; 3], radius: f32) -> Option<(f32, [f32; 3])> {
    let w = emu.peek_u64(0x1_43d6_d640);
    let world = emu.peek_u64(w + 0x98);
    let wcm = emu.peek_u64(0x1_43d7_a1e0);
    let filter = u32::from_le_bytes(emu.peek(wcm + 0x3550)) as u64;
    let start = emu.alloc(16);
    let dir = emu.alloc(16);
    let frac = emu.alloc(16);
    let point = emu.alloc(16);
    let normal = emu.alloc(16);
    emu.raw_write(start, &[from[0], from[1], from[2], 0.0].iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<_>>());
    emu.raw_write(dir, &[d[0], d[1], d[2], 0.0].iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<_>>());
    let r = emu.call(CAST, &[world, filter, start, dir, radius.to_bits() as u64, point, normal, frac], &[]);
    (r.rax as u8 != 0).then(|| (emu.rf32(frac), std::array::from_fn(|i| emu.rf32(normal + 4 * i as u64))))
}

/// The ground under (x, z): a 0.05 m sphere dropped from `top`, 6 m.
pub(super) fn ground(emu: &mut Emu, x: f32, z: f32, top: f32) -> Option<f32> {
    cast(emu, [x, top, z], [0.0, -6.0, 0.0], 0.05).map(|(t, _)| top - 6.0 * t - 0.05)
}

/// proxy_map: casts around Wolf's spot: 72 flat headings at four heights (0.4 m sphere), and the
/// ground on a 1 m grid.
fn map(new_emu: &dyn Fn() -> Emu, near: Option<(f32, f32)>) {
    let (mut emu, w, _) = setup(new_emu);
    let pos = f32s(&mut emu, w.phys + 0x80, 3);
    // `proxy_map <dx> <dz>`: the top surface on a 0.1 m grid, 1 m around that offset.
    if let Some((dx, dz)) = near {
        for i in -10..=10 {
            let mut row = String::new();
            for j in -10..=10 {
                let (x, z) = (pos[0] + dx + i as f32 * 0.1, pos[2] + dz + j as f32 * 0.1);
                row += &match ground(&mut emu, x, z, pos[1] + 4.0) {
                    Some(y) => format!("{:6.2}", y - pos[1]),
                    None => "    --".into(),
                };
            }
            println!("x {:+.1} {row}", dx + i as f32 * 0.1);
        }
        return;
    }
    for h in [0.2f32, 0.5, 1.0, 1.5] {
        for k in 0..72 {
            let a = (k * 5) as f32;
            let r = a.to_radians();
            let d = [r.sin() * 20.0, 0.0, r.cos() * 20.0];
            if let Some((t, n)) = cast(&mut emu, [pos[0], pos[1] + h, pos[2]], d, 0.4) {
                println!("wall h {h} heading {a}: {:.3} m normal {n:.3?}", t * 20.0);
            }
        }
    }
    for i in -12..=12 {
        let mut row = String::new();
        for j in -12..=12 {
            row += &match ground(&mut emu, pos[0] + i as f32, pos[2] + j as f32, pos[1] + 6.0) {
                Some(y) => format!("{:6.2}", y - pos[1]),
                None => "    --".into(),
            };
        }
        println!("x {i:+3} {row}");
    }
}

/// The enemy nearest a point that has a proxy (the General): chr, physics module, wrapper, proxy.
pub(super) fn nearest_enemy(emu: &mut Emu, at: &[f32]) -> (u64, u64, u64, u64) {
    let mut best = (f32::MAX, (0, 0, 0, 0));
    for c in super::damage::chrs(emu).into_iter().skip(1) {
        let bag = emu.peek_u64(c + 0x1ff8);
        let ph = emu.peek_u64(bag + 0x68);
        let p = f32s(emu, ph + 0x80, 3);
        let d = (p[0] - at[0]).powi(2) + (p[1] - at[1]).powi(2) + (p[2] - at[2]).powi(2);
        let (wr, px) = proxy_of(emu, ph);
        if d < best.0 && px != 0 {
            best = (d, (c, ph, wr, px));
        }
    }
    best.1
}

/// Puts a chr at `p` (feet): the physics module's position (+0x80, last frame's +0x90; the
/// pre-step puts the proxy there each frame) and the proxy itself through the game's setter.
pub(super) fn place(emu: &mut Emu, phys: u64, wrapper: u64, p: [f32; 3]) {
    let v = [p[0], p[1], p[2], 1.0];
    emu.poke_f32s(phys + 0x80, &v);
    emu.poke_f32s(phys + 0x90, &v);
    let at = emu.alloc(16);
    emu.raw_write(at, &v.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<_>>());
    let r = emu.call(SET_POSITION, &[wrapper, at], &[]);
    assert!(r.stopped.is_none(), "set position stopped {:?}", r.stopped);
}

/// Where Wolf starts and what he walks into.
#[derive(Clone, Copy)]
enum Setup {
    /// From the snapshot spot plus an offset; the General at an offset when given.
    Spot([f32; 3], Option<[f32; 3]>),
    /// In front of the rock face 7.7 m along heading 60 deg (the face the 0.4 m sphere cast meets
    /// at 1 m): `out` metres from the contact along the face's flat normal; the requested
    /// velocity is turned by `angle` deg from straight into the face.
    Wall { out: f32, angle: f32 },
}

/// One scenario: the frame time, the frames, the setup and the requested velocity (x, z m/s, or
/// for a wall the speed along the turned approach) per frame.
struct Scenario {
    name: &'static str,
    dt: f32,
    frames: usize,
    setup: Setup,
    vel: fn(usize) -> [f32; 2],
}

pub(super) fn heading(deg: f32, speed: f32) -> [f32; 2] {
    let r = deg.to_radians();
    [r.sin() * speed, r.cos() * speed]
}

fn scenarios() -> Vec<Scenario> {
    let s = |name, dt, frames, setup, vel| Scenario { name, dt, frames, setup, vel };
    let spot = Setup::Spot([0.0; 3], None);
    let general = Setup::Spot([0.0; 3], Some([1.6, 0.0, 0.0]));
    vec![
        // Standing still: the ground hold (one frame of gravity each frame, put back).
        s("stand", 1.0 / 60.0, 20, spot, |_| [0.0, 0.0]),
        // The General 1.6 m along +X, his frame run too (AI off). Not compared: in the snapshot his
        // physics does not step (he stays where he was put, 0.15 m over the ground) and Wolf's
        // proxy walks through his; the push between characters is elsewhere (gap in the kb).
        s("enemy_straight", 1.0 / 60.0, 40, general, |_| [3.0, 0.0]),
        // The rock face: straight in, 30 and 60 deg off (sliding), at 60 and 30 fps.
        s("wall_head_on", 1.0 / 60.0, 50, Setup::Wall { out: 1.0, angle: 0.0 }, |_| [3.0, 0.0]),
        s("wall_30", 1.0 / 60.0, 50, Setup::Wall { out: 1.0, angle: 30.0 }, |_| [3.0, 0.0]),
        s("wall_60", 1.0 / 60.0, 50, Setup::Wall { out: 1.0, angle: 60.0 }, |_| [3.0, 0.0]),
        s("wall_30_30fps", 1.0 / 30.0, 25, Setup::Wall { out: 1.0, angle: 30.0 }, |_| [3.0, 0.0]),
        s("wall_sprint_30", 1.0 / 60.0, 40, Setup::Wall { out: 1.5, angle: 30.0 }, |_| [8.0, 0.0]),
        // Over the cliff edge 2-3 m along -Z: the fall.
        s("ledge_fall", 1.0 / 60.0, 90, spot, |f| if f < 60 { [0.0, -3.0] } else { [0.0, 0.0] }),
        s("ledge_fall_30fps", 1.0 / 30.0, 45, spot, |f| if f < 30 { [0.0, -3.0] } else { [0.0, 0.0] }),
        // Along the gentle ground: the floor followed up and down.
        s("ground_plus_z", 1.0 / 60.0, 120, spot, |_| [0.0, 4.0]),
        s("ground_plus_x", 1.0 / 60.0, 100, spot, |_| [3.0, 0.0]),
        s("ground_minus_x", 1.0 / 60.0, 120, spot, |_| [-4.0, 0.0]),
        s("ground_minus_x_30fps", 1.0 / 30.0, 60, spot, |_| [-4.0, 0.0]),
    ]
}

/// The rock face: (contact sphere centre at 1 m, face normal) from the 0.4 m cast along heading
/// 60, and the same cast at 0.5 and 1.5 m (the face's lean).
fn rock_face(emu: &mut Emu, start: &[f32]) -> (Vec<Value>, [f32; 3], [f32; 3]) {
    let d = heading(60.0, 20.0);
    let mut casts = Vec::new();
    let mut at1 = None;
    for h in [0.5f32, 1.0, 1.5] {
        let from = [start[0], start[1] + h, start[2]];
        if let Some((t, n)) = cast(emu, from, [d[0], 0.0, d[1]], 0.4) {
            let c = [from[0] + d[0] * t, from[1], from[2] + d[1] * t];
            casts.push(json!({"height": h, "centre": c, "normal": n}));
            if h == 1.0 {
                at1 = Some((c, n));
            }
        }
    }
    let (c, n) = at1.expect("the rock face cast missed");
    (casts, c, n)
}

fn cases(new_emu: &dyn Fn() -> Emu, out: &Path, only: Option<&str>) {
    let mut done = Vec::new();
    for sc in scenarios() {
        if only.is_some_and(|o| o != sc.name) {
            continue;
        }
        let (mut emu, w, proxy) = setup(new_emu);
        let wcm = emu.peek_u64(0x1_43d7_a1e0);
        let spot = f32s(&mut emu, w.phys + 0x80, 3);
        let (wrap, _) = proxy_of(&mut emu, w.phys);
        let mut info = json!({});
        let mut turn = None;
        let mut enemy_chr = None;
        let start = match sc.setup {
            Setup::Spot(o, enemy) => {
                let at = [spot[0] + o[0], spot[1] + o[1], spot[2] + o[2]];
                if o != [0.0; 3] {
                    place(&mut emu, w.phys, wrap, at);
                }
                if let Some(e) = enemy {
                    let (chr, ph, ewrap, eproxy) = nearest_enemy(&mut emu, &spot);
                    let p = [spot[0] + e[0], spot[1] + e[1], spot[2] + e[2]];
                    place(&mut emu, ph, ewrap, p);
                    enemy_chr = Some((chr, ph));
                    info["enemy"] = json!({
                        "chr": format!("{chr:#x}"), "phys": format!("{ph:#x}"), "at": p,
                        "npc_param": u32::from_le_bytes(emu.peek(chr + 0x68)),
                        "proxy_box": f32s(&mut emu, eproxy + 0x90, 8),
                    });
                }
                at
            }
            Setup::Wall { out: o, angle } => {
                let (casts, c, n) = rock_face(&mut emu, &spot);
                let flat = {
                    let l = (n[0] * n[0] + n[2] * n[2]).sqrt();
                    [n[0] / l, n[2] / l]
                };
                let at = [c[0] + flat[0] * o, spot[1], c[2] + flat[1] * o];
                let g = ground(&mut emu, at[0], at[2], spot[1] + 2.0).unwrap_or(spot[1]);
                let at = [at[0], g, at[2]];
                place(&mut emu, w.phys, wrap, at);
                // Into the face is -flat, turned by `angle` (positive: toward +Y x into = left).
                let r = angle.to_radians();
                let (s, cs) = r.sin_cos();
                let into = [-flat[0], -flat[1]];
                turn = Some([into[0] * cs - into[1] * s, into[0] * s + into[1] * cs]);
                info["wall"] = json!({"casts": casts, "contact_centre": c, "normal": n, "angle": angle, "out": o});
                at
            }
        };
        let wolf_box = f32s(&mut emu, proxy + 0x90, 8);
        let mut frames = Vec::new();
        let mut stopped = None;
        for f in 0..sc.frames {
            emu.poke_f32s(w.stick, &[0.0, 0.0]);
            let v = (sc.vel)(f);
            let req = match turn {
                Some(d) => [d[0] * v[0], d[1] * v[0]],
                None => v,
            };
            {
                let mut g = INT.lock().unwrap();
                g.calls.clear();
                g.set = Some(req);
            }
            // The General's own frame first (his proxy and body follow his position), then Wolf's.
            if let Some((chr, _)) = enemy_chr {
                flush_tb(&emu);
                let r = emu.call(CHR_FRAME, &[wcm, 0, chr], &[0.0, sc.dt]);
                if r.stopped.is_some() {
                    stopped = r.stopped;
                    break;
                }
            }
            flush_tb(&emu);
            let r = emu.call(CHR_FRAME, &[wcm, 0, w.player], &[0.0, sc.dt]);
            if r.stopped.is_some() {
                stopped = r.stopped;
                break;
            }
            let calls = std::mem::take(&mut INT.lock().unwrap().calls);
            let (vel, lifted) = calls.first().copied().unwrap_or_default();
            let pos = f32s(&mut emu, w.phys + 0x80, 3);
            let floor = ground(&mut emu, pos[0], pos[2], pos[1] + 1.0);
            let flags = emu.peek::<4>(w.phys + 0xa0);
            let enemy = enemy_chr.map(|(_, ph)| f32s(&mut emu, ph + 0x80, 3));
            frames.push(json!({"req": req, "vel": &vel[..3], "lifted": &lifted[..3], "pos": pos, "ground": floor, "integrated": calls.len(), "a0": flags, "enemy": enemy}));
        }
        let last = frames.last().map(|v| v["pos"].clone());
        println!("{}: {} frames, start {start:?} end {last:?} stopped {:x?}", sc.name, frames.len(), stopped.map(|(a, e)| (format!("{a:#x}"), e)));
        let lift = f32s(&mut emu, w.phys + 0x188, 2);
        done.push(json!({"name": sc.name, "dt": sc.dt, "start": start, "spot": spot, "wolf_box": wolf_box, "lift": lift, "info": info, "frames": frames}));
    }
    let name = only.map_or("proxy_frames".to_string(), |o| format!("proxy_{o}"));
    let body = json!({"function": format!("{CHR_FRAME:#x} -> 0x140fe7310 -> {INTEGRATE:#x}"), "scenarios": done});
    std::fs::write(out.join(format!("{name}.json")), serde_json::to_string(&body).unwrap()).unwrap();
}

/// `proxy_map`, or `proxy [scenario]` (the cases).
pub(super) fn run(new_emu: &dyn Fn() -> Emu, out: &Path, args: &[&str]) {
    match args.first() {
        Some(&"proxy_map") => map(new_emu, args.get(1).zip(args.get(2)).map(|(x, z)| (x.parse().unwrap(), z.parse().unwrap()))),
        _ => cases(new_emu, out, args.get(1).copied()),
    }
}
