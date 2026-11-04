//! collide: what the proxy pilot left open (docs/kb/movement.md "Character proxy against the
//! exe" gaps): the push between Wolf's and an enemy's proxies, step-up onto a ledge, the
//! post-step floor follow and the walk-off distance at an edge.
//!
//!   emu-cases <exe> <out> collide_probe [who] --snapshot <dir>  which proxy functions a chr frame reaches
//!   collide_push key=value..      one push experiment, printed (dist, wolf, enemy, heading, lateral, away, ai)
//!   collide_push_cases            Wolf into the idle c1150 dog -> collide_push.json
//!   collide_steps [half] [cell]   sharp floor steps round Wolf (sphere casts)
//!   collide_stepwalk [name]       Wolf across the steps found -> collide_steps.json
//!   collide_snap [x z] [wide|fine] [off=value..]  the floor follow depth -> collide_snap.json
//!   collide_cast x z              the ground casts at one point (debug)
//!   collide_rimleave [off=value..] the down walks with the ground check's cast -> collide_rimleave.json

use std::collections::BTreeMap;
use std::ffi::c_void;
use std::path::Path;
use std::sync::Mutex;

use super::proxy::{f32s, flush_tb, nearest_enemy, proxy_of, setup, CHR_FRAME};
use serde_json::json;

use super::{uc, Emu};

/// The proxy path, hooked to see which a chr frame reaches.
const WATCH: [(u64, &str); 14] = [
    (0x1_40a4_b570, "chr frame"),
    (0x1_4101_5fa0, "behaviour world step"),
    (0x1_40fe_7310, "CharaProxySceneModifier vf3"),
    (0x1_4104_3cd0, "physics pre-step"),
    (0x1_40bc_ac10, "proxy lift"),
    (0x1_40bc_c570, "proxy velocity"),
    (0x1_4093_bb50, "world proxy pass"),
    (0x1_4093_cfd0, "proxy queue"),
    (0x1_418f_82a0, "checkSupport"),
    (0x1_418f_8b60, "integrate"),
    (0x1_4104_4050, "post-step"),
    (0x1_407d_e780, "post-step 2"),
    (0x1_40bc_4390, "post-step 3"),
    (0x1_40bc_a6d0, "floor follow"),
];

/// Per watched function: the rcx of each call.
static HITS: Mutex<BTreeMap<u64, Vec<u64>>> = Mutex::new(BTreeMap::new());

extern "C" fn on_hit(e: uc::Engine, addr: u64, _size: u32, _d: *mut c_void) {
    let mut rcx = 0u64;
    unsafe { uc::uc_reg_read(e, uc::RCX, &mut rcx as *mut u64 as _) };
    HITS.lock().unwrap().entry(addr).or_default().push(rcx);
}

fn probe(new_emu: &dyn Fn() -> Emu, args: &[&str]) {
    let (mut emu, w, wproxy) = setup(new_emu);
    for (a, _) in WATCH {
        emu.hook(uc::HOOK_CODE, on_hit as *const c_void, a, a);
    }
    let wcm = emu.peek_u64(0x1_43d7_a1e0);
    let spot = f32s(&mut emu, w.phys + 0x80, 3);
    let (chr, ph, wrap, eproxy) = nearest_enemy(&mut emu, &spot);
    println!("wolf {:#x} phys {:#x} proxy {wproxy:#x}; enemy {chr:#x} phys {ph:#x} wrapper {wrap:#x} proxy {eproxy:#x}", w.player, w.phys);
    println!("enemy at {:?}", f32s(&mut emu, ph + 0x80, 3));
    let who = args.get(1).copied().unwrap_or("enemy");
    for f in 0..3 {
        HITS.lock().unwrap().clear();
        for (name, c) in [("enemy", chr), ("wolf", w.player)] {
            if who != "both" && who != name {
                continue;
            }
            flush_tb(&emu);
            let r = emu.call(CHR_FRAME, &[wcm, 0, c], &[0.0, 1.0 / 60.0]);
            println!("frame {f} {name}: stopped {:x?}", r.stopped);
        }
        for (a, rcx) in HITS.lock().unwrap().iter() {
            let n = WATCH.iter().find(|w| w.0 == *a).unwrap().1;
            println!("  {a:#x} {n}: {} calls, rcx {:x?}", rcx.len(), &rcx[..rcx.len().min(4)]);
        }
        println!("  enemy at {:?}, wolf at {:?}", f32s(&mut emu, ph + 0x80, 3), f32s(&mut emu, w.phys + 0x80, 3));
    }
    let _ = proxy_of;
}

/// The physics module's proxy mode and the fields its update (FUN_140bc3xxx before
/// FUN_140bc4390) decides it from.
fn modes(emu: &mut Emu, ph: u64) -> String {
    let b = |emu: &mut Emu, o: u64| emu.peek::<1>(ph + o)[0];
    format!(
        "b10 {} 100 {} 934 {} 936 {} 937 {} 92a {} 928 {} bbc {:#x} a2 {} a3 {} 190 {}",
        u32::from_le_bytes(emu.peek(ph + 0xb10)),
        b(emu, 0x100),
        b(emu, 0x934),
        b(emu, 0x936),
        b(emu, 0x937),
        b(emu, 0x92a),
        b(emu, 0x928),
        b(emu, 0xbbc),
        b(emu, 0xa2),
        b(emu, 0xa3),
        u32::from_le_bytes(emu.peek(ph + 0x190)),
    )
}

/// The enemy's integrate hook: the proxy and the velocity (x, z) put in (y kept), and the calls.
static ENEMY_VEL: Mutex<(u64, Option<[f32; 2]>, usize)> = Mutex::new((0, None, 0));

extern "C" fn on_enemy_integrate(e: uc::Engine, _addr: u64, _size: u32, _d: *mut c_void) {
    let mut rcx = 0u64;
    unsafe { uc::uc_reg_read(e, uc::RCX, &mut rcx as *mut u64 as _) };
    let mut g = ENEMY_VEL.lock().unwrap();
    if rcx != g.0 {
        return;
    }
    g.2 += 1;
    if let Some([x, z]) = g.1 {
        let mut y = [0u8; 4];
        unsafe { uc::uc_mem_read(e, rcx + 0xb4, y.as_mut_ptr() as _, 4) };
        let b: Vec<u8> = [x, f32::from_le_bytes(y), z].iter().flat_map(|v| v.to_le_bytes()).collect();
        unsafe { uc::uc_mem_write(e, rcx + 0xb0, b.as_ptr() as _, 12) };
    }
}

fn kv(args: &[&str], key: &str, default: f32) -> f32 {
    args.iter().find_map(|a| a.strip_prefix(key).and_then(|r| r.strip_prefix('=')).map(|v| v.parse().unwrap())).unwrap_or(default)
}

/// `collide_push key=value..`: Wolf put `dist` m from the nearest enemy with a proxy (the c1150
/// dog) against `heading`, `lateral` m to its side, walked at `wolf` m/s along `heading`; the
/// enemy's frame run first, and with `enemy` m/s his proxy steps toward Wolf (bbc 0x40, the
/// player's always-step flag, set on him) with that velocity.
fn push(new_emu: &dyn Fn() -> Emu, args: &[&str]) {
    let dist = kv(args, "dist", 1.6);
    let wolf_v = kv(args, "wolf", 3.0);
    let enemy_v = kv(args, "enemy", 0.0);
    let heading = kv(args, "heading", 90.0);
    let lateral = kv(args, "lateral", 0.0);
    let frames = kv(args, "frames", 40.0) as usize;
    let (mut emu, w, wproxy) = setup(new_emu);
    let wcm = emu.peek_u64(0x1_43d7_a1e0);
    let spot = f32s(&mut emu, w.phys + 0x80, 3);
    let (chr, ph, _, eproxy) = nearest_enemy(&mut emu, &spot);
    let e = f32s(&mut emu, ph + 0x80, 3);
    println!("enemy {chr:#x} npc {} at {e:?} box {:?} {}", u32::from_le_bytes(emu.peek(chr + 0x68)), f32s(&mut emu, eproxy + 0x90, 8), modes(&mut emu, ph));
    let d = super::proxy::heading(heading, 1.0);
    let side = [d[1], -d[0]];
    let at = [e[0] - d[0] * dist + side[0] * lateral, e[2] - d[1] * dist + side[1] * lateral];
    let g = super::proxy::ground(&mut emu, at[0], at[1], e[1] + 2.0).unwrap_or(e[1]);
    let (wrap, _) = proxy_of(&mut emu, w.phys);
    super::proxy::place(&mut emu, w.phys, wrap, [at[0], g, at[1]]);
    // `away=<m>`: the enemy put that far on along the heading first (his proxy and body through
    // the game's setter), Wolf still walking at the spot he was.
    // `ai=1`: the snapshot's disable-AI debug flag (DebugFlags +0x10, set by proxy::setup) cleared
    // again, so the dog's AI and its own move run in his frame.
    if kv(args, "ai", 0.0) != 0.0 {
        emu.raw_write(0x1_43d7_a376, &[0]);
    }
    let away = kv(args, "away", 0.0);
    if away != 0.0 {
        let (_, _, ewrap, _) = nearest_enemy(&mut emu, &spot);
        super::proxy::place(&mut emu, ph, ewrap, [e[0] + d[0] * away, e[1], e[2] + d[1] * away]);
    }
    if enemy_v != 0.0 {
        let f = emu.peek::<1>(ph + 0xbbc)[0] | 0x40;
        emu.raw_write(ph + 0xbbc, &[f]);
        emu.hook(uc::HOOK_CODE, on_enemy_integrate as *const c_void, super::proxy::INTEGRATE, super::proxy::INTEGRATE);
        *ENEMY_VEL.lock().unwrap() = (eproxy, Some([-d[0] * enemy_v, -d[1] * enemy_v]), 0);
    }
    for f in 0..frames {
        {
            let mut i = super::proxy::INT.lock().unwrap();
            i.calls.clear();
            i.set = Some([d[0] * wolf_v, d[1] * wolf_v]);
        }
        ENEMY_VEL.lock().unwrap().2 = 0;
        flush_tb(&emu);
        let r = emu.call(CHR_FRAME, &[wcm, 0, chr], &[0.0, 1.0 / 60.0]);
        assert!(r.stopped.is_none());
        let ep = f32s(&mut emu, eproxy + 0x80, 3);
        emu.poke_f32s(w.stick, &[0.0, 0.0]);
        flush_tb(&emu);
        let r = emu.call(CHR_FRAME, &[wcm, 0, w.player], &[0.0, 1.0 / 60.0]);
        assert!(r.stopped.is_none());
        let p = f32s(&mut emu, w.phys + 0x80, 3);
        let q = f32s(&mut emu, ph + 0x80, 3);
        let wp = f32s(&mut emu, wproxy + 0x80, 3);
        let flat = ((p[0] - q[0]).powi(2) + (p[2] - q[2]).powi(2)).sqrt();
        let n = ENEMY_VEL.lock().unwrap().2;
        println!(
            "{f:2} wolf {p:.3?} enemy {q:.3?} flat {flat:.4} dy {:.3} | wolf proxy y {:+.3} enemy proxy y {:+.3} (int {n}) {}",
            p[1] - q[1],
            wp[1] - p[1],
            ep[1] - q[1],
            modes(&mut emu, ph)
        );
    }
}

/// One push scenario: approach heading (deg), lateral offset (m), frame time, frames.
struct Push {
    name: String,
    heading: f32,
    lateral: f32,
    dt: f32,
    frames: usize,
}

/// `collide_push_cases`: Wolf walking at 3 m/s into the idle c1150 dog (the nearest enemy with a
/// proxy, at his own snapshot spot: NpcParam 11500000 hitRadius 0.5 / hitHeight 1.0) from eight
/// headings, two side offsets (he slides round) and at 30 fps -> collide_push.json. The dog's
/// frame runs first each frame; idle he takes no proxy step (physics +0xb10 mode 1), so his
/// proxy rests at his feet while Wolf's (+0xbbc 0x40: the player steps every frame) moves lifted.
fn push_cases(new_emu: &dyn Fn() -> Emu, out: &Path) {
    let mut list: Vec<Push> = (0..8).map(|k| Push { name: format!("heading_{}", k * 45), heading: (k * 45) as f32, lateral: 0.0, dt: 1.0 / 60.0, frames: 45 }).collect();
    for lat in [0.3f32, 0.6] {
        list.push(Push { name: format!("heading_0_side_{lat}"), heading: 0.0, lateral: lat, dt: 1.0 / 60.0, frames: 60 });
    }
    list.push(Push { name: "heading_0_30fps".into(), heading: 0.0, lateral: 0.0, dt: 1.0 / 30.0, frames: 25 });
    let mut done = Vec::new();
    for sc in list {
        let (mut emu, w, wproxy) = setup(new_emu);
        let wcm = emu.peek_u64(0x1_43d7_a1e0);
        let spot = f32s(&mut emu, w.phys + 0x80, 3);
        let (chr, ph, _, eproxy) = nearest_enemy(&mut emu, &spot);
        let e = f32s(&mut emu, ph + 0x80, 3);
        let d = super::proxy::heading(sc.heading, 1.0);
        let side = [d[1], -d[0]];
        let at = [e[0] - d[0] * 1.6 + side[0] * sc.lateral, e[2] - d[1] * 1.6 + side[1] * sc.lateral];
        let g = super::proxy::ground(&mut emu, at[0], at[1], e[1] + 2.0).unwrap_or(e[1]);
        let (wrap, _) = proxy_of(&mut emu, w.phys);
        super::proxy::place(&mut emu, w.phys, wrap, [at[0], g, at[1]]);
        let req = [d[0] * 3.0, d[1] * 3.0];
        let mut frames = Vec::new();
        for _ in 0..sc.frames {
            {
                let mut i = super::proxy::INT.lock().unwrap();
                i.calls.clear();
                i.set = Some(req);
            }
            flush_tb(&emu);
            let r = emu.call(CHR_FRAME, &[wcm, 0, chr], &[0.0, sc.dt]);
            assert!(r.stopped.is_none(), "{}: enemy frame stopped", sc.name);
            emu.poke_f32s(w.stick, &[0.0, 0.0]);
            flush_tb(&emu);
            let r = emu.call(CHR_FRAME, &[wcm, 0, w.player], &[0.0, sc.dt]);
            assert!(r.stopped.is_none(), "{}: wolf frame stopped", sc.name);
            let integrated = super::proxy::INT.lock().unwrap().calls.len();
            let p = f32s(&mut emu, w.phys + 0x80, 3);
            let q = f32s(&mut emu, ph + 0x80, 3);
            let mode = u32::from_le_bytes(emu.peek(ph + 0xb10));
            frames.push(json!({"wolf": p, "enemy": q, "enemy_mode": mode, "integrated": integrated}));
        }
        let last = frames.last().cloned().unwrap_or_default();
        println!("{}: end {last}", sc.name);
        done.push(json!({
            "name": sc.name, "dt": sc.dt, "req": req, "start": [at[0], g, at[1]],
            "wolf_box": f32s(&mut emu, wproxy + 0x90, 8), "enemy_box": f32s(&mut emu, eproxy + 0x90, 8),
            "enemy_npc": u32::from_le_bytes(emu.peek(chr + 0x68)), "lift": f32s(&mut emu, w.phys + 0x188, 2),
            "frames": frames,
        }));
    }
    let body = json!({"function": "0x140a4b570 (the c1150 dog's chr frame, then Wolf's) -> 0x1418f8b60", "scenarios": done});
    std::fs::write(out.join("collide_push.json"), serde_json::to_string(&body).unwrap()).unwrap();
}

/// The world's sphere cast (FUN_14094dc70, as proxy::cast) with its buffers allocated once, for
/// thousands of casts (the emulator heap is 1 MB); Unicorn's code cache flushed every 256.
struct Caster {
    world: u64,
    filter: u64,
    buf: u64,
    n: usize,
}

impl Caster {
    fn new(emu: &mut Emu) -> Caster {
        let w = emu.peek_u64(0x1_43d6_d640);
        let world = emu.peek_u64(w + 0x98);
        let wcm = emu.peek_u64(0x1_43d7_a1e0);
        let filter = u32::from_le_bytes(emu.peek(wcm + 0x3550)) as u64;
        Caster { world, filter, buf: emu.alloc(80), n: 0 }
    }
    fn cast(&mut self, emu: &mut Emu, from: [f32; 3], d: [f32; 3], radius: f32) -> Option<(f32, [f32; 3])> {
        self.n += 1;
        if self.n % 256 == 0 {
            flush_tb(emu);
        }
        let (start, dir, point, normal, frac) = (self.buf, self.buf + 16, self.buf + 32, self.buf + 48, self.buf + 64);
        emu.raw_write(start, &[from[0], from[1], from[2], 0.0].iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<_>>());
        emu.raw_write(dir, &[d[0], d[1], d[2], 0.0].iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<_>>());
        let r = emu.call(0x1_4094_dc70, &[self.world, self.filter, start, dir, radius.to_bits() as u64, point, normal, frac], &[]);
        (r.rax as u8 != 0).then(|| (emu.rf32(frac), std::array::from_fn(|i| emu.rf32(normal + 4 * i as u64))))
    }
}

/// `collide_steps [half] [cell]`: the ground (0.05 m sphere dropped 16 m from 8 m over Wolf) on a
/// grid round Wolf's spot (only hits on floors, normal y over 0.85), and every sharp step between flat runs: a rise of 0.15-0.8 m from one
/// cell to the next along x or z with the two cells before and after level (within 0.04 m).
fn steps(new_emu: &dyn Fn() -> Emu, args: &[&str]) {
    let half: f32 = args.get(1).map_or(30.0, |a| a.parse().unwrap());
    let cell: f32 = args.get(2).map_or(0.25, |a| a.parse().unwrap());
    let (mut emu, w, _) = setup(new_emu);
    let pos = f32s(&mut emu, w.phys + 0x80, 3);
    let n = (2.0 * half / cell) as i32 + 1;
    let top = pos[1] + 8.0;
    let mut c = Caster::new(&mut emu);
    let mut h = vec![f32::NAN; (n * n) as usize];
    for i in 0..n {
        for j in 0..n {
            let (x, z) = (pos[0] - half + i as f32 * cell, pos[2] - half + j as f32 * cell);
            if let Some((t, _)) = c.cast(&mut emu, [x, top, z], [0.0, -16.0, 0.0], 0.05).filter(|(_, nrm)| nrm[1] > 0.85) {
                h[(i * n + j) as usize] = top - 16.0 * t - 0.05;
            }
        }
    }
    let at = |i: i32, j: i32| if (0..n).contains(&i) && (0..n).contains(&j) { h[(i * n + j) as usize] } else { f32::NAN };
    for i in 0..n {
        for j in 0..n {
            for (di, dj) in [(1, 0), (0, 1)] {
                // Low run a0 a1, high run b0 b1 (either way round).
                let (a0, a1, b0, b1) = (at(i - di, j - dj), at(i, j), at(i + di, j + dj), at(i + 2 * di, j + 2 * dj));
                let rise = b0 - a1;
                if !(0.15..=0.8).contains(&rise.abs()) || (a0 - a1).abs() > 0.04 || (b0 - b1).abs() > 0.04 {
                    continue;
                }
                let (x, z) = (pos[0] - half + i as f32 * cell, pos[2] - half + j as f32 * cell);
                println!("step {rise:+.3} m from ({x:.2}, {:.3}, {z:.2}) along {}", a1, if di == 1 { "+x" } else { "+z" });
            }
        }
    }
}

/// `collide_snap [x z]`: Wolf put h m over the ground (at his spot, or `x z` off it), standing,
/// one frame at a time: whether the post-step puts him down on the floor (grounded, physics +0xa2)
/// or he falls, for h 0.02 .. 1.0.
fn snap(new_emu: &dyn Fn() -> Emu, out: &Path, args: &[&str]) {
    let mut cases = Vec::new();
    let off: (f32, f32) = (args.get(1).map_or(0.0, |a| a.parse().unwrap()), args.get(2).map_or(0.0, |a| a.parse().unwrap()));
    let hs: Vec<f32> = match args.get(3) {
        Some(&"fine") => (0..=10).map(|i| 0.6 + 0.02 * i as f32).collect(),
        Some(&"wide") => (0..=24).map(|i| 0.02 + 0.04 * i as f32).collect(),
        _ => vec![0.02, 0.05, 0.1, 0.15, 0.2, 0.25, 0.3, 0.35, 0.4, 0.5, 0.6, 0.8],
    };
    for h in hs {
        let (mut emu, w, wproxy) = setup(new_emu);
        let wcm = emu.peek_u64(0x1_43d7_a1e0);
        let spot = f32s(&mut emu, w.phys + 0x80, 3);
        let (x, z) = (spot[0] + off.0, spot[2] + off.1);
        let g = super::proxy::ground(&mut emu, x, z, spot[1] + 2.0).unwrap();
        let (wrap, _) = proxy_of(&mut emu, w.phys);
        super::proxy::place(&mut emu, w.phys, wrap, [x, g + h, z]);
        // `0x184=0.5`: a physics module float changed in the emulator copy (which constant the
        // snap depth follows).
        for a in &args[4.min(args.len())..] {
            if let Some((o, v)) = a.split_once('=') {
                let o = u64::from_str_radix(o.trim_start_matches("0x"), 16).unwrap();
                emu.poke_f32s(w.phys + o, &[v.parse().unwrap()]);
            }
        }
        let mut line = format!("h {h:.2} ground {g:.3}:");
        let mut frames = Vec::new();
        for _ in 0..4 {
            super::proxy::INT.lock().unwrap().set = Some([0.0, 0.0]);
            emu.poke_f32s(w.stick, &[0.0, 0.0]);
            flush_tb(&emu);
            let r = emu.call(CHR_FRAME, &[wcm, 0, w.player], &[0.0, 1.0 / 60.0]);
            assert!(r.stopped.is_none());
            let calls = std::mem::take(&mut super::proxy::INT.lock().unwrap().calls);
            let p = f32s(&mut emu, w.phys + 0x80, 3);
            let a2 = emu.peek::<2>(w.phys + 0xa2);
            let lifted = calls.first().map_or(f32::NAN, |c| c.1[1] - g);
            frames.push(json!({"y": p[1] - g, "a2": a2[0]}));
            line += &format!(" | y {:+.4} a2 {:?} lifted {lifted:+.3} vy {:+.3} proxy {:+.3}", p[1] - g, a2, calls.first().map_or(f32::NAN, |c| c.0[1]), f32s(&mut emu, wproxy + 0x80, 3)[1] - g);
        }
        println!("{line}");
        cases.push(json!({"h": h, "ground": g, "frames": frames}));
    }
    // Only the plain run (Wolf's spot, no poked constant) is the case file.
    if args.len() <= 4 {
        let body = json!({"function": "0x140a4b570 (Wolf's chr frame, standing) -> 0x140bca6d0", "dt": 1.0 / 60.0, "offset": [off.0, off.1], "cases": cases});
        std::fs::write(out.join("collide_snap.json"), serde_json::to_string(&body).unwrap()).unwrap();
    }
}

/// The one sharp ledge `collide_steps` (floors only) finds in reach: a stone block's edge along
/// x at z 4.13 (14 m along -x, 20 m along -z from Wolf), its rise over the ground north of it
/// 0.69 m at x 187.39, 0.61 at 187.49, 0.46 at 187.59, 0.30 at 187.69 (0.1 m grid). Walked onto
/// along -z from 1 m out, and off its top along +z; one low 0.21 m step at (189.04, 2.13).
/// name, start (x, z), heading deg.
const STEP_WALKS: [(&str, f32, f32, f32); 11] = [
    ("up_x187.39", 187.39, 4.83, 180.0),
    ("up_x187.49", 187.49, 4.83, 180.0),
    ("up_x187.59", 187.59, 4.83, 180.0),
    ("up_x187.69", 187.69, 4.83, 180.0),
    ("up_x187.79", 187.79, 4.83, 180.0),
    ("up_0.21", 189.04, 1.23, 0.0),
    ("down_x187.39", 187.39, 3.33, 0.0),
    ("down_x187.49", 187.49, 3.33, 0.0),
    ("down_x187.59", 187.59, 3.33, 0.0),
    ("down_x187.69", 187.69, 3.33, 0.0),
    ("down_0.21", 189.04, 3.03, 180.0),
];

/// `collide_stepwalk [name]`: Wolf walking at 3 m/s across each step of STEP_WALKS, 60 frames at
/// 1/60 (requested velocity into the proxy at integrate, y the game's), with the ground profile
/// along the line (0.05 m sphere, every 0.05 m) -> collide_steps.json (or collide_steps_<name>).
fn step_walks(new_emu: &dyn Fn() -> Emu, out: &Path, only: Option<&str>) {
    let mut done = Vec::new();
    for (name, x0, z0, hd) in STEP_WALKS {
        if only.is_some_and(|o| o != name) {
            continue;
        }
        let (mut emu, w, _) = setup(new_emu);
        let wcm = emu.peek_u64(0x1_43d7_a1e0);
        let spot = f32s(&mut emu, w.phys + 0x80, 3);
        let d = super::proxy::heading(hd, 1.0);
        let mut c = Caster::new(&mut emu);
        let mut profile = Vec::new();
        for k in 0..=50 {
            let s = k as f32 * 0.05;
            let (x, z) = (x0 + d[0] * s, z0 + d[1] * s);
            let top = spot[1] + 8.0;
            let y = c.cast(&mut emu, [x, top, z], [0.0, -16.0, 0.0], 0.05).map(|(t, _)| top - 16.0 * t - 0.05);
            profile.push(json!([s, y]));
        }
        let Some(g) = profile[0][1].as_f64().map(|g| g as f32) else {
            println!("{name}: no ground at the start; profile {profile:?}");
            continue;
        };
        let (wrap, _) = proxy_of(&mut emu, w.phys);
        super::proxy::place(&mut emu, w.phys, wrap, [x0, g, z0]);
        let req = [d[0] * 3.0, d[1] * 3.0];
        let mut frames = Vec::new();
        for _ in 0..60 {
            {
                let mut i = super::proxy::INT.lock().unwrap();
                i.calls.clear();
                i.set = Some(req);
            }
            emu.poke_f32s(w.stick, &[0.0, 0.0]);
            flush_tb(&emu);
            let r = emu.call(CHR_FRAME, &[wcm, 0, w.player], &[0.0, 1.0 / 60.0]);
            assert!(r.stopped.is_none(), "{name}: frame stopped");
            let calls = std::mem::take(&mut super::proxy::INT.lock().unwrap().calls);
            let (vel, lifted) = calls.first().copied().unwrap_or_default();
            let p = f32s(&mut emu, w.phys + 0x80, 3);
            let a2 = emu.peek::<2>(w.phys + 0xa2);
            let along = (p[0] - x0) * d[0] + (p[2] - z0) * d[1];
            frames.push(json!({"pos": p, "along": along, "vel": &vel[..3], "lifted": &lifted[..3], "a2": a2}));
        }
        let last = &frames[frames.len() - 1];
        println!("{name}: start y {g:.3} end along {:.3} y {:.3} ({:+.3})", last["along"], last["pos"][1], last["pos"][1].as_f64().unwrap() as f32 - g);
        done.push(json!({"name": name, "start": [x0, g, z0], "heading": hd, "req": req, "dt": 1.0 / 60.0, "profile": profile, "frames": frames}));
    }
    let name = only.map_or("collide_steps".to_string(), |o| format!("collide_steps_{o}"));
    let body = json!({"function": "0x140a4b570 (Wolf's chr frame) -> 0x1418f8b60", "scenarios": done});
    std::fs::write(out.join(format!("{name}.json")), serde_json::to_string(&body).unwrap()).unwrap();
}

/// `collide_rimleave [off=value..]`: the down walks of STEP_WALKS again with the ground check
/// FUN_140bce730 (called by the post-step FUN_140bca6d0, its result is +0xa2) read after every
/// frame from the physics fields it leaves: the cast's start +0xc50 and vector +0xc60, its hit
/// point +0xc70, normal +0xc80 and hit byte +0xc90, +0xca0 (the allowed gap less the extra
/// depth). Prints the module's constants (+0xd4 radius, +0x818 the probe's share of it, +0x184,
/// +0x8c4, +0xc0c maxSlope deg). `0x818=0.6` pokes a physics float (which constant moves the
/// leave point) and only prints; the plain run -> collide_rimleave.json.
fn rim_leave(new_emu: &dyn Fn() -> Emu, out: &Path, args: &[&str]) {
    let pokes: Vec<(u64, f32)> = args[1..]
        .iter()
        .filter_map(|a| a.split_once('='))
        .map(|(o, v)| (u64::from_str_radix(o.trim_start_matches("0x"), 16).unwrap(), v.parse().unwrap()))
        .collect();
    let mut done = Vec::new();
    for (name, x0, z0, hd) in STEP_WALKS {
        if !name.starts_with("down") {
            continue;
        }
        let (mut emu, w, _) = setup(new_emu);
        let wcm = emu.peek_u64(0x1_43d7_a1e0);
        let spot = f32s(&mut emu, w.phys + 0x80, 3);
        let d = super::proxy::heading(hd, 1.0);
        let top = spot[1] + 8.0;
        let g = Caster::new(&mut emu).cast(&mut emu, [x0, top, z0], [0.0, -16.0, 0.0], 0.05).map(|(t, _)| top - 16.0 * t - 0.05).unwrap();
        let (wrap, _) = proxy_of(&mut emu, w.phys);
        super::proxy::place(&mut emu, w.phys, wrap, [x0, g, z0]);
        for &(o, v) in &pokes {
            emu.poke_f32s(w.phys + o, &[v]);
        }
        let consts: BTreeMap<&str, f32> = [("d4", 0xd4), ("818", 0x818), ("184", 0x184), ("8c4", 0x8c4), ("c0c", 0xc0c), ("920", 0x920), ("844", 0x844), ("848", 0x848)].into_iter().map(|(k, o)| (k, f32s(&mut emu, w.phys + o, 1)[0])).collect();
        let flags = emu.peek::<2>(w.phys + 0x840);
        println!("{name}: {consts:?} +0x840/841 {flags:?}");
        let req = [d[0] * 3.0, d[1] * 3.0];
        let mut frames = Vec::new();
        for _ in 0..40 {
            {
                let mut i = super::proxy::INT.lock().unwrap();
                i.calls.clear();
                i.set = Some(req);
            }
            emu.poke_f32s(w.stick, &[0.0, 0.0]);
            flush_tb(&emu);
            let r = emu.call(CHR_FRAME, &[wcm, 0, w.player], &[0.0, 1.0 / 60.0]);
            assert!(r.stopped.is_none(), "{name}: frame stopped");
            let p = f32s(&mut emu, w.phys + 0x80, 3);
            let a2 = emu.peek::<1>(w.phys + 0xa2)[0];
            let along = (p[0] - x0) * d[0] + (p[2] - z0) * d[1];
            let (start, ray, hit, normal) = (f32s(&mut emu, w.phys + 0xc50, 3), f32s(&mut emu, w.phys + 0xc60, 3), f32s(&mut emu, w.phys + 0xc70, 3), f32s(&mut emu, w.phys + 0xc80, 3));
            let hit_byte = emu.peek::<1>(w.phys + 0xc90)[0];
            let ca0 = f32s(&mut emu, w.phys + 0xca0, 1)[0];
            // The second cast (+0x841 on: radius +0x844 x +0xd4, its height against the first's
            // within +0x848): start +0xcb0, vector +0xcc0, hit +0xcd0, normal +0xce0, byte +0xcf0.
            let (start2, hit2, normal2) = (f32s(&mut emu, w.phys + 0xcb0, 3), f32s(&mut emu, w.phys + 0xcd0, 3), f32s(&mut emu, w.phys + 0xce0, 3));
            let hit2_byte = emu.peek::<1>(w.phys + 0xcf0)[0];
            let hit2_along = (hit2[0] - x0) * d[0] + (hit2[2] - z0) * d[1];
            let hit_along = (hit[0] - x0) * d[0] + (hit[2] - z0) * d[1];
            println!("  along {along:.3} y {:+.4} a2 {a2} start {:+.3} ray {:+.3} hit along {hit_along:.3} y {:+.4} n.y {:.3} byte {hit_byte} ca0 {ca0:.3} | 2nd start {:+.3} hit along {hit2_along:.3} y {:+.4} n.y {:.3} byte {hit2_byte}", p[1] - g, start[1] - g, ray[1], hit[1] - g, normal[1], start2[1] - g, hit2[1] - g, normal2[1]);
            frames.push(json!({"pos": p, "along": along, "a2": a2, "start": start, "ray": ray, "hit": hit, "normal": normal, "hit_byte": hit_byte, "ca0": ca0, "start2": start2, "hit2": hit2, "normal2": normal2, "hit2_byte": hit2_byte}));
        }
        done.push(json!({"name": name, "start": [x0, g, z0], "heading": hd, "req": req, "dt": 1.0 / 60.0, "consts": consts, "frames": frames}));
    }
    if pokes.is_empty() {
        let body = json!({"function": "0x140a4b570 (Wolf's chr frame) -> 0x140bca6d0 -> 0x140bce730", "scenarios": done});
        std::fs::write(out.join("collide_rimleave.json"), serde_json::to_string(&body).unwrap()).unwrap();
    }
}

pub(super) fn run(new_emu: &dyn Fn() -> Emu, out: &Path, args: &[&str]) {
    match args.first() {
        Some(&"collide_rimleave") => rim_leave(new_emu, out, args),
        Some(&"collide_cast") => {
            let (mut emu, w, _) = setup(new_emu);
            let spot = f32s(&mut emu, w.phys + 0x80, 3);
            let (x, z): (f32, f32) = (args[1].parse().unwrap(), args[2].parse().unwrap());
            let mut c = Caster::new(&mut emu);
            for top in [spot[1] + 8.0, spot[1] + 2.0, -18.0, -19.0] {
                println!("top {top}: caster {:?} proxy::cast {:?}", c.cast(&mut emu, [x, top, z], [0.0, -16.0, 0.0], 0.05), super::proxy::cast(&mut emu, [x, top, z], [0.0, -16.0, 0.0], 0.05));
            }
        }
        Some(&"collide_stepwalk") => step_walks(new_emu, out, args.get(1).copied()),
        Some(&"collide_snap") => snap(new_emu, out, args),
        Some(&"collide_steps") => steps(new_emu, args),
        Some(&"collide_push_cases") => push_cases(new_emu, out),
        Some(&"collide_probe") => probe(new_emu, args),
        Some(&"collide_push") => push(new_emu, args),
        _ => {}
    }
}
