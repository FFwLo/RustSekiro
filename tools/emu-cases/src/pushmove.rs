//! pushmove (row E14): what the aimove push left open (docs/kb/movement.md "A moving enemy
//! against Wolf"): an enemy pushing a Wolf who moves too, and how two characters' proxies meet
//! (the filter and the response are the same code for any pair: enemy against enemy).
//! Whole world frames as aimove (pre tasks, the General's chr frame FUN_140a4b570, Wolf's, the
//! post tasks), the AI on, the General updated every frame; the General is put in front of Wolf
//! at Wolf's snapshot spot as aimove's `push_ledge` (both proxies' bodies are then where the
//! broadphase holds them). Per frame both feet, yaw, anim, physics +0xb10 and every
//! hknpCharacterProxy::integrate (FUN_1418f8b60) call on either proxy: the velocity it was handed
//! (+0xb0), the (lifted) position at entry, and the position and velocity it left.
//!
//!   emu-cases <exe> <out> pushmove_x key=value.. --snapshot <dir>   one run, printed
//!   emu-cases <exe> <out> pushmove_f_<name>.. --snapshot <dir>      -> pushmove_<name>.json
//!   emu-cases <exe> <out> pushmove_bp --snapshot <dir>   the chrs' proxy bodies (broadphase slot,
//!                                                        filter info) and a body moved and rebuilt

use std::ffi::c_void;
use std::path::Path;
use std::sync::Mutex;

use serde_json::{json, Value};

use super::aicpp::{ai_chain, f32s, physics, pos, WORLD_CHR_MAN};
use super::aimove::{anim, general, i32_at, setup, CHR_FRAME, TASKS_POST, TASKS_PRE};
use super::{proxy, turn, uc, Emu};

/// The manipulator's request copy (AI +0x7430 -> ActionRequest +0x170).
const REQUEST_COPY: u64 = 0x1_407e_6570;

fn rd(e: uc::Engine, at: u64) -> u64 {
    let mut b = [0u8; 8];
    unsafe { uc::uc_mem_read(e, at, b.as_mut_ptr() as _, 8) };
    u64::from_le_bytes(b)
}
fn v4(e: uc::Engine, at: u64) -> [f32; 4] {
    std::array::from_fn(|i| f32::from_bits(rd(e, at + 4 * i as u64) as u32))
}

/// Per integrate call: (proxy, velocity in, position in). The exit is read from the proxy at the
/// next call's entry or after the frame (integrate's own return is not hooked).
static INTEG: Mutex<Vec<(u64, [f32; 4], [f32; 4])>> = Mutex::new(Vec::new());
/// Velocities (x, z) put into a proxy at integrate (y kept): (proxy, v).
static SET: Mutex<Vec<(u64, [f32; 2])>> = Mutex::new(Vec::new());

extern "C" fn on_integ(e: uc::Engine, _addr: u64, _size: u32, _d: *mut c_void) {
    let mut rcx = 0u64;
    unsafe { uc::uc_reg_read(e, uc::RCX, &mut rcx as *mut u64 as _) };
    if let Some((_, [x, z])) = SET.lock().unwrap().iter().find(|s| s.0 == rcx).copied() {
        let y = v4(e, rcx + 0xb0)[1];
        let b: Vec<u8> = [x, y, z].iter().flat_map(|v| v.to_le_bytes()).collect();
        unsafe { uc::uc_mem_write(e, rcx + 0xb0, b.as_ptr() as _, 12) };
    }
    INTEG.lock().unwrap().push((rcx, v4(e, rcx + 0xb0), v4(e, rcx + 0x80)));
}

static FORCE: Mutex<(u64, i32)> = Mutex::new((0, -1));

extern "C" fn on_force(e: uc::Engine, _addr: u64, _size: u32, _d: *mut c_void) {
    let (ai, v) = *FORCE.lock().unwrap();
    if ai != 0 && v >= 0 {
        unsafe { uc::uc_mem_write(e, ai + 0x7430, v.to_le_bytes().as_ptr() as _, 4) };
    }
}

/// One run: the General `dist` m in front of Wolf (along Wolf's facing, turned to face him);
/// `force` = the attack request for the first 3 frames; Wolf's stick per frame, or his proxy's
/// velocity (x, z, m/s in Wolf's frame: forward toward the General, side to his right) put in at
/// integrate; the General's the same when `gvel` is set.
#[derive(Clone)]
pub(super) struct Run {
    pub name: String,
    pub dist: f32,
    pub lateral: f32,
    pub frames: usize,
    pub force: Option<i32>,
    pub ai: bool,
    pub stick: Option<[f32; 2]>,
    pub stick_from: usize,
    pub wvel: Option<[f32; 2]>,
    pub wvel_from: usize,
    pub gvel: Option<[f32; 2]>,
    pub dt: f32,
    /// The broadphase brought up to the bodies after the placing and after each chr frame.
    pub bp: bool,
    /// Wolf's chr frame run (off: his proxy never steps, his body rests where it is).
    pub wframe: bool,
    /// Wolf's body collision filter info (hknpBody +0x6c) replaced, every frame, by this one:
    /// an enemy's (0x2000069, the value every humanoid enemy body in the snapshot carries) makes
    /// his body an enemy body to the filter, so the General's proxy meets "another enemy".
    pub wfilter: Option<u32>,
}

/// hknpBroadPhase (world +0x488) vtable +0x28 (FUN_141985120, "UpdateBroadPhase"): the bodies
/// marked dirty (vtable +0x20, FUN_141985050, which the body transform setter FUN_14190b5a0 ->
/// FUN_141902930 calls) are moved in its trees. The game does it in the hknp world step, which
/// the emulator does not run: without it a body moved by its proxy stays in the broadphase where
/// the snapshot had it, and another proxy passes through it (aimove's caveat).
fn update_broadphase(emu: &mut Emu, world: u64) {
    let bp = emu.peek_u64(world + 0x488);
    let vt = emu.peek_u64(bp);
    let f = emu.peek_u64(vt + 0x28);
    let bodies = emu.peek_u64(world + 0x28);
    let r = emu.call(f, &[bp, bodies, 0], &[]);
    assert!(r.stopped.is_none(), "broadphase update stopped {:?}", r.stopped);
}

fn one(new_emu: &dyn Fn() -> Emu, r: &Run, print: bool) -> Value {
    let t = std::time::Instant::now();
    let mut emu = new_emu();
    let w = turn::wolf(&mut emu);
    let g = general(&mut emu);
    let (_, ai, _) = ai_chain(&mut emu, g).unwrap();
    let wcm = emu.peek_u64(WORLD_CHR_MAN);
    emu.raw_write(0x1_43d7_a376, &[if r.ai { 0 } else { 1 }]);
    let _ = emu.peek::<4>(g + 0x110);
    emu.raw_write(g + 0x110, &0u32.to_le_bytes());
    setup(&mut emu);
    let _ = emu.peek::<4>(ai + 0x7430);
    emu.hook(uc::HOOK_CODE, on_integ as *const c_void, proxy::INTEGRATE, proxy::INTEGRATE);
    if r.force.is_some() {
        emu.hook(uc::HOOK_CODE, on_force as *const c_void, REQUEST_COPY, REQUEST_COPY);
    }
    let gphys = physics(&mut emu, g);
    let wp = pos(&mut emu, w.player);
    let wyaw = f32::from_le_bytes(emu.peek(w.phys + 0x74));
    // Wolf's forward (-sin yaw, -cos yaw) and right.
    let fwd = [-wyaw.sin(), -wyaw.cos()];
    let right = [-fwd[1], fwd[0]];
    let at = [wp[0] + r.dist * fwd[0] + r.lateral * right[0], wp[1], wp[2] + r.dist * fwd[1] + r.lateral * right[1]];
    let (gwrap, gproxy) = proxy::proxy_of(&mut emu, gphys);
    proxy::place(&mut emu, gphys, gwrap, at);
    let world = emu.peek_u64(gproxy + 0x40);
    if r.bp {
        update_broadphase(&mut emu, world);
    }
    emu.poke_f32s(gphys + 0x74, &[wyaw + std::f32::consts::PI]);
    let (_, wproxy) = proxy::proxy_of(&mut emu, w.phys);
    let world_v = |v: [f32; 2], f: [f32; 2], rt: [f32; 2]| [v[0] * f[0] + v[1] * rt[0], v[0] * f[1] + v[1] * rt[1]];
    let blk = emu.alloc(0x40);
    emu.poke_f32s(blk + 8, &[r.dt]);
    let info = json!({
        "wolf_box": f32s::<8>(&mut emu, wproxy + 0x90), "general_box": f32s::<8>(&mut emu, gproxy + 0x90),
        "wolf_body": i32_at(&mut emu, wproxy + 0x38), "general_body": i32_at(&mut emu, gproxy + 0x38),
        "wolf_lift": f32s::<2>(&mut emu, w.phys + 0x188), "general_lift": f32s::<2>(&mut emu, gphys + 0x188),
        "wolf_keep": f32s::<4>(&mut emu, wproxy + 0x140), "general_keep": f32s::<4>(&mut emu, gproxy + 0x140),
        "fwd": fwd, "start_w": wp, "start_g": at,
    });
    if print {
        println!("{info}");
    }
    let mut frames = Vec::new();
    let mut stopped = None;
    for f in 0..r.frames {
        *FORCE.lock().unwrap() = (ai, match r.force { Some(v) if f < 3 => v, _ => -1 });
        INTEG.lock().unwrap().clear();
        {
            let mut s = SET.lock().unwrap();
            s.clear();
            if let Some(v) = r.wvel.filter(|_| f >= r.wvel_from) {
                s.push((wproxy, world_v(v, fwd, right)));
            }
            if let Some(v) = r.gvel {
                s.push((gproxy, world_v(v, fwd, right)));
            }
        }
        if let Some(fi) = r.wfilter {
            let id = u32::from_le_bytes(emu.peek(wproxy + 0x38)) as u64 & 0xff_ffff;
            let b = emu.peek_u64(world + 0x28) + id * 0xb0;
            emu.raw_write(b + 0x6c, &fi.to_le_bytes());
        }
        let st = r.stick.filter(|_| f >= r.stick_from).unwrap_or([0.0, 0.0]);
        emu.poke_f32s(w.stick, &st);
        proxy::flush_tb(&emu);
        let mut stop = None;
        for task in TASKS_PRE {
            stop = stop.or(emu.call(task, &[wcm, blk], &[]).stopped);
        }
        let gi = INTEG.lock().unwrap().len();
        stop = stop.or(emu.call(CHR_FRAME, &[wcm, 0, g], &[0.0, r.dt]).stopped);
        if r.bp {
            update_broadphase(&mut emu, world);
        }
        let gproxy_after = f32s::<4>(&mut emu, gproxy + 0x80);
        let gvel_after = f32s::<4>(&mut emu, gproxy + 0xb0);
        let wi = INTEG.lock().unwrap().len();
        // Both feet between the two chr frames (the General's frame moved only him?).
        let (g_mid, w_mid) = (pos(&mut emu, g), pos(&mut emu, w.player));
        if r.wframe {
            stop = stop.or(emu.call(CHR_FRAME, &[wcm, 0, w.player], &[0.0, r.dt]).stopped);
        }
        if r.bp {
            update_broadphase(&mut emu, world);
        }
        let wproxy_after = f32s::<4>(&mut emu, wproxy + 0x80);
        let wvel_after = f32s::<4>(&mut emu, wproxy + 0xb0);
        for task in TASKS_POST {
            stop = stop.or(emu.call(task, &[wcm, blk], &[]).stopped);
        }
        let calls = INTEG.lock().unwrap().clone();
        let pick = |p: u64, from: usize, to: usize| -> Vec<Value> {
            calls[from..to.min(calls.len())].iter().filter(|c| c.0 == p).map(|c| json!({"v": &c.1[..3], "at": &c.2[..3]})).collect()
        };
        let gp = pos(&mut emu, g);
        let wpn = pos(&mut emu, w.player);
        let rec = json!({
            "g": gp, "w": wpn, "gyaw": f32s::<1>(&mut emu, gphys + 0x74)[0], "wyaw": f32s::<1>(&mut emu, w.phys + 0x74)[0],
            "ganim": anim(&mut emu, g), "wanim": anim(&mut emu, w.player),
            "gmode": i32_at(&mut emu, gphys + 0xb10), "wmode": i32_at(&mut emu, w.phys + 0xb10),
            // Each chr's integrate calls in its own frame (and any of the other's proxy there).
            "g_in_g": pick(gproxy, gi, wi), "w_in_g": pick(wproxy, gi, wi),
            "w_in_w": pick(wproxy, wi, calls.len()), "g_in_w": pick(gproxy, wi, calls.len()),
            "g_mid": g_mid, "w_mid": w_mid,
            "gproxy_out": &gproxy_after[..3], "gvel_out": &gvel_after[..3],
            "wproxy_out": &wproxy_after[..3], "wvel_out": &wvel_after[..3],
            "stick": st,
            // hknpBody +0x98 (-1: not in the broadphase; the General's in the snapshot) and +0x44.
            "gbody": body_bp(&mut emu, world, gproxy), "wbody": body_bp(&mut emu, world, wproxy),
        });
        if print {
            let d = ((gp[0] - wpn[0]).powi(2) + (gp[2] - wpn[2]).powi(2)).sqrt();
            let dw = [wpn[0] - wp[0], wpn[2] - wp[2]];
            let along = dw[0] * fwd[0] + dw[1] * fwd[1];
            let side = dw[0] * right[0] + dw[1] * right[1];
            println!(
                "f{f:3} {:?} {:?} d {d:.4} wolf moved fwd {along:+.4} side {side:+.4} dy {:+.3} | g anim {} mode {} w anim {} mode {} | gint {} wint {} gv {:?} wv {:?}",
                rec["gbody"], rec["wbody"],
                gp[1] - wpn[1],
                rec["ganim"], rec["gmode"], rec["wanim"], rec["wmode"],
                rec["g_in_g"].as_array().unwrap().len(), rec["w_in_w"].as_array().unwrap().len(),
                rec["g_in_g"].get(0).map(|c| c["v"].clone()), rec["w_in_w"].get(0).map(|c| c["v"].clone()),
            );
        }
        frames.push(rec);
        if stop.is_some() {
            stopped = stop.map(|(a, e)| format!("{a:#x} error {e}"));
            println!("  {} stopped at f{f}: {stopped:?}", r.name);
            break;
        }
    }
    println!("{}: {} frames, {:.0} s", r.name, frames.len(), t.elapsed().as_secs_f32());
    json!({"name": r.name, "dist": r.dist, "lateral": r.lateral, "force": r.force, "ai": r.ai, "stick": r.stick, "stick_from": r.stick_from,
        "wvel": r.wvel, "wvel_from": r.wvel_from, "gvel": r.gvel, "dt": r.dt, "info": info, "frames": frames, "stopped": stopped})
}

fn body_bp(emu: &mut Emu, world: u64, proxy: u64) -> [u32; 7] {
    let id = u32::from_le_bytes(emu.peek(proxy + 0x38)) as u64 & 0xff_ffff;
    let b = emu.peek_u64(world + 0x28) + id * 0xb0;
    [0x98u64, 0x44, 0x6c, 0x40, 0x30, 0x34, 0x38].map(|o| u32::from_le_bytes(emu.peek(b + o)))
}

fn kv<'a>(args: &[&'a str], key: &str) -> Option<&'a str> {
    args.iter().find_map(|a| a.strip_prefix(key).and_then(|r| r.strip_prefix('=')))
}
fn pair(s: &str) -> [f32; 2] {
    let mut it = s.split(',').map(|x| x.parse::<f32>().unwrap());
    [it.next().unwrap(), it.next().unwrap()]
}

fn from_args(args: &[&str]) -> Run {
    Run {
        name: kv(args, "name").unwrap_or("x").into(),
        dist: kv(args, "dist").map_or(1.6, |v| v.parse().unwrap()),
        lateral: kv(args, "lateral").map_or(0.0, |v| v.parse().unwrap()),
        frames: kv(args, "frames").map_or(60, |v| v.parse().unwrap()),
        force: kv(args, "force").map(|v| v.parse().unwrap()),
        ai: kv(args, "ai").map_or(true, |v| v != "0"),
        stick: kv(args, "stick").map(pair),
        stick_from: kv(args, "stick_from").map_or(0, |v| v.parse().unwrap()),
        wvel: kv(args, "wvel").map(pair),
        wvel_from: kv(args, "wvel_from").map_or(0, |v| v.parse().unwrap()),
        gvel: kv(args, "gvel").map(pair),
        dt: kv(args, "dt").map_or(1.0 / 30.0, |v| v.parse().unwrap()),
        bp: kv(args, "bp").map_or(true, |v| v != "0"),
        wframe: kv(args, "wframe").map_or(true, |v| v != "0"),
        wfilter: kv(args, "wfilter").map(|v| u32::from_str_radix(v.trim_start_matches("0x"), 16).unwrap()),
    }
}

/// The recorded runs (name -> key=value list).
/// The recorded runs (name -> key=value list), all from Wolf's snapshot spot with the General
/// 1.6 m in front of him, the AI on, 1/30 s, the broadphase brought up each chr frame.
pub(super) const RUNS: &[(&str, &str)] = &[
    // The General's forced 3000 lunge at a Wolf who stands (aimove push_ledge again, broadphase on).
    ("lunge_still", "force=3000 frames=36"),
    // ... who walks at him (1.5 m/s), backs off (1 and 0.5 m/s) or crosses in front (1.5 m/s).
    ("lunge_in", "force=3000 wvel=1.5,0 frames=36"),
    ("lunge_away", "force=3000 wvel=-1,0 frames=36"),
    ("lunge_away_slow", "force=3000 wvel=-0.5,0 frames=36"),
    ("lunge_side", "force=3000 wvel=0,1.5 frames=36"),
    // Wolf walking at 3 m/s into the General standing (AI off, physics mode 1).
    ("walk_into_idle", "ai=0 wvel=3,0 frames=16"),
    // Enemy against enemy: the lunge at a standing Wolf whose body carries an enemy's collision
    // filter info (the humanoid 0x2000069, and the c1150 dogs' 0x2000066).
    ("lunge_enemy_body", "force=3000 wfilter=0x2000069 frames=24"),
    ("lunge_dog_body", "force=3000 wfilter=0x2000066 frames=24"),
];

pub(super) fn run(new_emu: &dyn Fn() -> Emu, out: &Path, want: &[&str]) {
    if want.first() == Some(&"pushmove_bp") {
        let mut emu = new_emu();
        let wolf = turn::wolf(&mut emu);
        let (_, px) = proxy::proxy_of(&mut emu, wolf.phys);
        let world = emu.peek_u64(px + 0x40);
        let bp = emu.peek_u64(world + 0x488);
        let vt = emu.peek_u64(bp);
        println!("world {world:#x} broadphase {bp:#x} vtable {vt:#x}");
        let id = u32::from_le_bytes(emu.peek(px + 0x38)) as u64 & 0xff_ffff;
        let bodies = emu.peek_u64(world + 0x28);
        let show = |emu: &mut Emu, tag: &str| {
            let b = f32s::<44>(emu, bodies + id * 0xb0);
            let raw: [u8; 0xb0] = emu.peek(bodies + id * 0xb0);
            println!("{tag}: body {id} floats {:?}", b.iter().map(|x| (x * 1000.0).round() / 1000.0).collect::<Vec<_>>());
            println!("   raw u32 {:x?}", raw.chunks(4).map(|c| u32::from_le_bytes(c.try_into().unwrap())).collect::<Vec<_>>());
            println!("   bp min/max dirty {} {}", i32_at(emu, bp + 0x98), i32_at(emu, bp + 0x9c));
        };
        for c in super::damage::chrs(&mut emu) {
            let ph = physics(&mut emu, c);
            let (_, cp) = proxy::proxy_of(&mut emu, ph);
            let npc = u32::from_le_bytes(emu.peek(c + 0x68));
            let at = pos(&mut emu, c);
            let info = if cp != 0 { body_bp(&mut emu, world, cp) } else { [0; 7] };
            println!("chr {c:#x} npc {npc} at {at:?} proxy {cp:#x} body [+0x98 +0x44 +0x6c +0x40] {info:x?}");
        }
        show(&mut emu, "snapshot");
        let wp = pos(&mut emu, wolf.player);
        let (wrap, _) = proxy::proxy_of(&mut emu, wolf.phys);
        proxy::place(&mut emu, wolf.phys, wrap, [wp[0] + 1.0, wp[1], wp[2]]);
        show(&mut emu, "placed +1 x");
        update_broadphase(&mut emu, world);
        show(&mut emu, "after update");
        let g = general(&mut emu);
        let gph = physics(&mut emu, g);
        let (gw, gpx) = proxy::proxy_of(&mut emu, gph);
        let gid = u32::from_le_bytes(emu.peek(gpx + 0x38)) as u64 & 0xff_ffff;
        let gp = pos(&mut emu, g);
        println!("general at {gp:?} proxy {gpx:#x} body id {gid} world {:#x}", emu.peek_u64(gpx + 0x40));
        let raw: [u8; 0xb0] = emu.peek(bodies + gid * 0xb0);
        println!("   general raw u32 {:x?}", raw.chunks(4).map(|c| u32::from_le_bytes(c.try_into().unwrap())).collect::<Vec<_>>());
        proxy::place(&mut emu, gph, gw, [wp[0], wp[1], wp[2] - 1.6]);
        update_broadphase(&mut emu, world);
        let raw: [u8; 0xb0] = emu.peek(bodies + gid * 0xb0);
        println!("   general placed raw u32 {:x?}", raw.chunks(4).map(|c| u32::from_le_bytes(c.try_into().unwrap())).collect::<Vec<_>>());
        return;
    }
    if want.first() == Some(&"pushmove_x") {
        let r = from_args(&want[1..]);
        let v = one(new_emu, &r, true);
        std::fs::write(out.join(format!("pushmove_x_{}.json", r.name)), serde_json::to_string(&v).unwrap()).unwrap();
        return;
    }
    for (name, spec) in RUNS {
        if want.iter().any(|w| w.strip_prefix("pushmove_f_") == Some(name) || *w == "pushmove") {
            let args: Vec<&str> = spec.split_whitespace().collect();
            let mut r = from_args(&args);
            r.name = name.to_string();
            let v = one(new_emu, &r, false);
            let body = json!({"frame": format!("{CHR_FRAME:#x} -> {:#x}", proxy::INTEGRATE), "scenarios": [v]});
            std::fs::write(out.join(format!("pushmove_{name}.json")), serde_json::to_string(&body).unwrap()).unwrap();
        }
    }
}
