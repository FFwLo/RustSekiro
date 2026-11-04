//! wire: the Grappling Hook on the snapshot (row E7). Whole chr frames FUN_140a4b570 of Wolf with
//! his pad written per frame (air.rs / acttrace's way), and before each frame the wire point
//! search: the singleton [0x143d8e058] (built by FUN_140ef94e0) whose task FUN_140efaf80 copies
//! the WirePointSearchParam row into +0x9c (actionDistanceFar) / +0xa0 (near) / +0xa4
//! (findDistanceFar) / +0xa8.. and runs the search FUN_140efd8e0 (candidates FUN_140ef9880, the
//! camera's forward from PlayerIns +0x1190 by FUN_140efb2c0). Its result: +8 state (0 none, 1 in
//! action range, 2 found only), +0x10 the hook (a point on the edge V1 -> V2, +0x20 the fraction),
//! +0x28 the edge handle, +0x30 the edge's extra distance.
//!
//! The camera: the snapshot's camera rows at PlayerIns +0x1190 (3 x vec4) are turned about Y so
//! the camera looks along `cam` degrees (game yaw, forward = (-sin, 0, -cos)); Wolf's physics yaw
//! (+0x74) is put the same way unless the scenario says otherwise.
//!
//!   emu-cases <exe> <out> wire_probe --snapshot <dir>         prints the camera rows, the singleton
//!   emu-cases <exe> <out> wire_search --snapshot <dir>        -> wire_search.json (the target per
//!                                                               camera yaw / distance)
//!   emu-cases <exe> <out> wire [scenario...] --snapshot <dir> -> wire_frames.json (the move frames)

use std::collections::BTreeMap;
use std::ffi::c_void;
use std::path::Path;
use std::sync::Mutex;

use serde_json::{json, Value};

use super::{proxy, uc, Emu};

const WORLD_CHR_MAN: u64 = 0x1_43d7_a1e0;
const CHR_FRAME: u64 = 0x1_40a4_b570;
const ARM_INPUT: u64 = 0x1_40b2_58d0;
const MENU_STATE: u64 = 0x1_408c_58e0;
const TAE_UPDATE: u64 = 0x1_40b5_c920;
const STEP: u64 = 0x1_40bc_cd50;
/// The wire target singleton and its frame task.
const WIRE_TARGET: u64 = 0x1_43d8_e058;
const WIRE_UPDATE: u64 = 0x1_40ef_af80;
const DT: f32 = 1.0 / 60.0;
const WIRE_SHOOT: u32 = 3;

static PHYS: Mutex<u64> = Mutex::new(0);
/// The candidate filters of the search and their return sites: per-edge prefilter FUN_140eeda30
/// (called from FUN_140ef0120 +0x267 / +0x4c9), the screen test FUN_140efbc60, FUN_140efb6f0,
/// the obstacle cast FUN_140efba50 (from FUN_140ef9880 +0x757 / +0x778 / +0xe9a).
const EDGE_FILTER: u64 = 0x1_40ee_da30;
const SCREEN: u64 = 0x1_40ef_bc60;
const PICK: u64 = 0x1_40ef_b6f0;
const OBSTACLE: u64 = 0x1_40ef_ba50;
const RETS: [(u64, &str); 5] = [
    (0x1_40ef_0120 + 0x26c, "edge"),
    (0x1_40ef_0120 + 0x4ce, "edge"),
    (0x1_40ef_9880 + 0x75c, "screen"),
    (0x1_40ef_9880 + 0x77d, "pick"),
    (0x1_40ef_9880 + 0xe9f, "obstacle"),
];
static FILT: Mutex<Vec<Value>> = Mutex::new(Vec::new());
static PEND: Mutex<Vec<Value>> = Mutex::new(Vec::new());

extern "C" fn on_filter(e: uc::Engine, addr: u64, _size: u32, _d: *mut c_void) {
    if let Some(&(_, name)) = RETS.iter().find(|r| r.0 == addr) {
        let al = reg(e, uc::RAX) as u8;
        let mut p = PEND.lock().unwrap();
        if let Some(i) = p.iter().rposition(|v| v["fn"] == name) {
            let mut v = p.remove(i);
            v["ret"] = json!(al);
            FILT.lock().unwrap().push(v);
        }
        return;
    }
    let rcx = reg(e, uc::RCX);
    let rdx = reg(e, uc::RDX);
    let v = match addr {
        EDGE_FILTER => {
            let a = rf(e, rcx + 0x40, 3);
            let b = rf(e, rcx + 0x50, 3);
            json!({"fn": "edge", "edge": format!("{rcx:#x}"), "a": a, "b": b, "type": rd::<1>(e, rcx + 8)[0],
                "var": i32::from_le_bytes(rd(e, rcx + 0xc)), "b80": rd::<1>(e, rcx + 0x80)[0]})
        }
        SCREEN => json!({"fn": "screen", "p": rf(e, rdx, 3)}),
        PICK => json!({"fn": "pick", "rdx": format!("{rdx:#x}")}),
        OBSTACLE => json!({"fn": "obstacle", "p": rf(e, rdx, 3)}),
        _ => return,
    };
    PEND.lock().unwrap().push(v);
}
static LOG: Mutex<Vec<Value>> = Mutex::new(Vec::new());

fn rd<const N: usize>(e: uc::Engine, at: u64) -> [u8; N] {
    let mut b = [0u8; N];
    unsafe { uc::uc_mem_read(e, at, b.as_mut_ptr() as _, N) };
    b
}
fn rf(e: uc::Engine, at: u64, n: usize) -> Vec<f32> {
    (0..n).map(|i| f32::from_le_bytes(rd(e, at + 4 * i as u64))).collect()
}
fn reg(e: uc::Engine, r: i32) -> u64 {
    let mut v = 0u64;
    unsafe { uc::uc_reg_read(e, r, &mut v as *mut u64 as _) };
    v
}

extern "C" fn on_hook(e: uc::Engine, addr: u64, _size: u32, _d: *mut c_void) {
    let rcx = reg(e, uc::RCX);
    let phys = *PHYS.lock().unwrap();
    let rec = match addr {
        TAE_UPDATE => {
            let mut x = [[0u8; 16]; 4];
            for (i, r) in x.iter_mut().enumerate() {
                unsafe { uc::uc_reg_read(e, uc::XMM0 + i as i32, r.as_mut_ptr() as _) };
            }
            let f = |i: usize| f32::from_le_bytes(x[i][..4].try_into().unwrap());
            json!({"at": "tae", "id": rcx as u32 as i32, "prev": f(1), "cur": f(2), "len": f(3)})
        }
        STEP if rcx == phys => {
            let fall = rcx + 0x1c0;
            json!({"at": "step", "vel": rf(e, rcx + 0x160, 3), "fall_row": i32::from_le_bytes(rd(e, fall + 0x10)),
                "fall_type": rd::<1>(e, fall + 0x6d)[0], "fall_timer": rf(e, fall + 0x180, 1)[0], "homing": rd::<1>(e, fall + 0xd9)[0],
                "b92a": rd::<1>(e, rcx + 0x92a)[0], "gscale": rf(e, rcx + 0x920, 1)[0]})
        }
        _ => return,
    };
    LOG.lock().unwrap().push(rec);
}

#[derive(Clone, Copy)]
struct Input {
    arms: &'static [u32],
    stick: [f32; 2],
}
const NONE: Input = Input { arms: &[], stick: [0.0, 0.0] };

fn arm_inputs(emu: &mut Emu) -> BTreeMap<u32, i32> {
    (0..33u32).map(|arm| (arm, emu.call(ARM_INPUT, &[arm as u64], &[]).rax as u32 as i32)).collect()
}

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
    emu.raw_write(frame + 0x3c, &[0]);
    emu.raw_write(frame + 0x3e, &[0]);
    emu.poke_f32s(frame, &inp.stick);
}

struct Rig {
    emu: Emu,
    w: super::turn::Wolf,
    wrap: u64,
    wcm: u64,
    ids: BTreeMap<u32, i32>,
    bag: u64,
    target: u64,
    time: u64,
    /// The snapshot's camera rows (PlayerIns +0x1190, 12 floats).
    cam0: Vec<f32>,
    wolf0: Vec<f32>,
    /// The view matrix the screen test reads ([singleton] +0x10, 16 floats).
    view0: Vec<f32>,
}

fn rig(new_emu: &dyn Fn() -> Emu) -> Rig {
    let (mut emu, w, _) = proxy::setup(new_emu);
    emu.limit = 500_000_000;
    emu.raw_write(MENU_STATE, &[0x31, 0xc0, 0xc3]);
    let menus = emu.call(0x1_40d8_72c0, &[], &[]).rax;
    if menus != 0 {
        emu.u64(menus + 0x100, 0);
    }
    *PHYS.lock().unwrap() = w.phys;
    for a in [TAE_UPDATE, STEP] {
        emu.hook(uc::HOOK_CODE, on_hook as *const c_void, a, a);
    }
    for a in [EDGE_FILTER, SCREEN, PICK, OBSTACLE].into_iter().chain(RETS.iter().map(|r| r.0)) {
        emu.hook(uc::HOOK_CODE, on_filter as *const c_void, a, a);
    }
    let ids = arm_inputs(&mut emu);
    let (wrap, _) = proxy::proxy_of(&mut emu, w.phys);
    let wcm = emu.peek_u64(WORLD_CHR_MAN);
    let bag = emu.peek_u64(w.player + 0x1ff8);
    let target = emu.peek_u64(WIRE_TARGET);
    let time = super::turn::fd4_time(&mut emu, DT);
    let cam0 = proxy::f32s(&mut emu, w.player + 0x1190, 12);
    let wolf0 = proxy::f32s(&mut emu, w.phys + 0x80, 3);
    let view0 = { let q = emu.peek_u64(target); proxy::f32s(&mut emu, q + 0x10, 16) };
    Rig { emu, w, wrap, wcm, ids, bag, target, time, cam0, wolf0, view0 }
}

impl Rig {
    fn anim(&mut self) -> i32 {
        let m = self.emu.peek_u64(self.bag + 0x10);
        i32::from_le_bytes(self.emu.peek(m + 0xf8))
    }
    fn pos(&mut self) -> Vec<f32> {
        proxy::f32s(&mut self.emu, self.w.phys + 0x80, 3)
    }
    /// The camera rows turned so the camera looks along game yaw `deg` (rows = the snapshot's
    /// turned about Y by the difference to the snapshot camera's own yaw), pitch kept.
    fn camera(&mut self, deg: f32) {
        let c = self.cam0.clone();
        let base = self.cam_yaw0();
        let d = deg.to_radians() - base;
        let (s, co) = d.sin_cos();
        // Each row is (x, y, z, w) of a 3x4 matrix whose columns are the basis vectors: turning the
        // basis about Y is R * M (rows x and z mix).
        let mut m = c.clone();
        for col in 0..3 {
            let (x, z) = (c[col], c[8 + col]);
            m[col] = co * x + s * z;
            m[8 + col] = -s * x + co * z;
        }
        // The position orbits Wolf: his spot now plus the snapshot's offset turned.
        let p = self.pos();
        let (ox, oy, oz) = (c[3] - self.wolf0[0], c[7] - self.wolf0[1], c[11] - self.wolf0[2]);
        m[3] = p[0] + co * ox + s * oz;
        m[7] = p[1] + oy;
        m[11] = p[2] - s * ox + co * oz;
        self.emu.poke_f32s(self.w.player + 0x1190, &m);
        // The view the screen test projects with ([singleton] +0x10: rows right, up, forward,
        // position; FUN_140734240): the same turn, row vectors.
        let q = self.emu.peek_u64(self.target);
        let mut v = self.view0.clone();
        for row in 0..3 {
            let (x, z) = (self.view0[4 * row], self.view0[4 * row + 2]);
            v[4 * row] = co * x + s * z;
            v[4 * row + 2] = -s * x + co * z;
        }
        v[12] = m[3];
        v[13] = m[7];
        v[14] = m[11];
        self.emu.poke_f32s(q + 0x10, &v);
    }
    /// The snapshot camera's forward yaw (column 2 = the forward basis vector, game yaw convention
    /// forward = (-sin, 0, -cos)).
    fn cam_yaw0(&self) -> f32 {
        let c = &self.cam0;
        let f = [c[2], c[6], c[10]];
        f32::atan2(-f[0], -f[2])
    }
    fn search(&mut self) -> Option<String> {
        proxy::flush_tb(&self.emu);
        let r = self.emu.call(WIRE_UPDATE, &[self.target, self.time], &[]);
        r.stopped.map(|(a, e)| format!("{a:#x} error {e}"))
    }
    fn target_state(&mut self) -> Value {
        let t = self.target;
        let q = self.emu.peek_u64(t);
        json!({
            "state": self.emu.peek::<1>(t + 8)[0],
            "hook": proxy::f32s(&mut self.emu, t + 0x10, 3),
            "frac": proxy::f32s(&mut self.emu, t + 0x20, 1)[0],
            "edge": format!("{:#x}", self.emu.peek_u64(t + 0x28)),
            "extend": proxy::f32s(&mut self.emu, t + 0x30, 1)[0],
            "far_near_find": proxy::f32s(&mut self.emu, t + 0x9c, 3),
            "b98": self.emu.peek::<1>(t + 0x98)[0],
            "view": proxy::f32s(&mut self.emu, q + 0x10, 20),
            "a8": proxy::f32s(&mut self.emu, t + 0xa8, 6),
            "flags134": self.emu.peek::<16>(t + 0x130),
        })
    }
    fn frame(&mut self, inp: &Input) -> Result<Value, String> {
        if let Some(e) = self.search() {
            return Err(format!("search {e}"));
        }
        let target = self.target_state();
        press(&mut self.emu, self.w.stick, &self.ids, inp);
        LOG.lock().unwrap().clear();
        proxy::INT.lock().unwrap().calls.clear();
        proxy::flush_tb(&self.emu);
        let r = self.emu.call(CHR_FRAME, &[self.wcm, 0, self.w.player], &[0.0, DT]);
        if let Some((a, e)) = r.stopped {
            return Err(format!("{a:#x} error {e}"));
        }
        let log = std::mem::take(&mut *LOG.lock().unwrap());
        let calls = std::mem::take(&mut proxy::INT.lock().unwrap().calls);
        let ph = self.w.phys;
        Ok(json!({
            "arms": inp.arms, "target": target, "log": log,
            "integrate": calls.iter().map(|(v, p)| json!({"vel": &v[..3], "pos": &p[..3]})).collect::<Vec<_>>(),
            "pos": self.pos(),
            "yaw": proxy::f32s(&mut self.emu, ph + 0x74, 1)[0],
            "vel": proxy::f32s(&mut self.emu, ph + 0x160, 3),
            "a2": self.emu.peek::<2>(ph + 0xa2),
            "fall_row": i32::from_le_bytes(self.emu.peek(ph + 0x1c0 + 0x10)),
            "fall_type": self.emu.peek::<1>(ph + 0x1c0 + 0x6d)[0],
            "anim": self.anim(),
        }))
    }
    /// Wolf put at `p` (feet) facing game yaw `yaw` deg.
    fn put(&mut self, p: [f32; 3], yaw: f32) {
        proxy::place(&mut self.emu, self.w.phys, self.wrap, p);
        self.emu.poke_f32s(self.w.phys + 0x74, &[yaw.to_radians()]);
    }
}

/// The game yaw (deg) that faces from `a` to `b` (forward = (-sin, 0, -cos)).
fn yaw_to(a: [f32; 3], b: [f32; 3]) -> f32 {
    f32::atan2(-(b[0] - a[0]), -(b[2] - a[2])).to_degrees()
}

/// The hooks near the snapshot spot (extracted/map_edges_m11_00_00_00.json "grapple"), game space.
const HOOKS: [(&str, [f32; 3]); 3] =
    [("o003600_0024", [192.3, -24.5, 26.0]), ("o003600_0023", [188.3, -21.3, 28.7]), ("o003610_0100", [184.6, -25.3, 19.2])];

fn probe(new_emu: &dyn Fn() -> Emu) {
    let mut r = rig(new_emu);
    let p = r.pos();
    let yaw = proxy::f32s(&mut r.emu, r.w.phys + 0x74, 1)[0];
    println!("wolf {p:?} yaw {:.2} deg; camera rows {:?} cam yaw {:.2}", yaw.to_degrees(), r.cam0, r.cam_yaw0().to_degrees());
    let ts = r.target_state();
    println!("target {:#x}: {ts}", r.target);
    for (n, h) in HOOKS {
        let d = ((h[0] - p[0]).powi(2) + (h[1] - p[1]).powi(2) + (h[2] - p[2]).powi(2)).sqrt();
        println!("{n} at {d:.2} m, yaw to {:.1}", yaw_to([p[0], p[1], p[2]], h));
    }
    let e = r.search();
    println!("search as is: {e:?} {}", r.target_state());
    for (n, h) in HOOKS {
        let mut r = rig(new_emu);
        let y = yaw_to([p[0], p[1], p[2]], h);
        r.camera(y);
        for i in 0..3 {
            FILT.lock().unwrap().clear();
            let e = r.search();
            println!("camera to {n} ({y:.1}) #{i}: {e:?} {}", r.target_state());
            for v in FILT.lock().unwrap().iter().filter(|v| v["fn"] != "edge" || v["ret"] == 1) {
                println!("   {v}");
            }
            for v in PEND.lock().unwrap().drain(..) {
                println!("   pending {v}");
            }
        }
    }
}

fn sweep(new_emu: &dyn Fn() -> Emu) {
    let mut r = rig(new_emu);
    for face in [false, true] {
        for k in 0..36 {
            let y = k as f32 * 10.0;
            r.camera(y);
            if face {
                r.emu.poke_f32s(r.w.phys + 0x74, &[y.to_radians()]);
            }
            for _ in 0..2 {
                FILT.lock().unwrap().clear();
                r.search();
            }
            let t = r.target_state();
            println!("face {face} cam {y}: state {} hook {}", t["state"], t["hook"]);
            if k % 6 == 0 || k == 10 {
                for v in FILT.lock().unwrap().iter().filter(|v| v["fn"] != "edge" || v["ret"] == 1) {
                    println!("   {v}");
                }
                for v in PEND.lock().unwrap().drain(..) {
                    println!("   pending {v}");
                }
            }
        }
    }
}

/// One move scenario: Wolf's spot (None: the snapshot's) at `dist` m from hook `hook` along the
/// line back to the snapshot spot, his yaw (None: kept), the camera's yaw offset from the
/// direction to the hook, the frames and the press frames.
struct Scenario {
    name: &'static str,
    hook: usize,
    dist: Option<f32>,
    face: Option<f32>,
    cam_off: f32,
    frames: usize,
    input: fn(usize) -> Input,
}

fn shoot(f: usize) -> Input {
    if (2..5).contains(&f) { Input { arms: &[WIRE_SHOOT], stick: [0.0, 0.0] } } else { NONE }
}

fn scenarios() -> Vec<Scenario> {
    let s = |name, hook, dist, face, cam_off| Scenario { name, hook, dist, face, cam_off, frames: 170, input: shoot };
    vec![
        // From the snapshot spot (9.9 m, the hook 1.7 m up), Wolf facing yaw 0 (the hook 103 deg to
        // his left): the shoot's turn.
        s("h24", 0, None, None, 0.0),
        // Facing the hook.
        s("h24_face", 0, None, Some(0.0), 0.0),
        // The camera 15 deg off the hook.
        s("h24_cam15", 0, None, Some(0.0), 15.0),
        // Wolf put nearer / farther along the same line.
        s("h24_d5", 0, Some(5.0), Some(0.0), 0.0),
        s("h24_d13", 0, Some(13.0), Some(0.0), 0.0),
        // Out of action range (found only, 17 m): the press does nothing.
        s("h24_d17", 0, Some(17.0), Some(0.0), 0.0),
        // The high hook (3.2 m over the ledge's... 15.2 m away: put at 11 m).
        s("h23_d11", 1, Some(11.0), Some(0.0), 0.0),
        // The low hook below the ledge (o003610_0100).
        s("h100_d12", 2, Some(12.0), Some(0.0), 0.0),
    ]
}

fn run_scenarios(new_emu: &dyn Fn() -> Emu, out: &Path, only: &[&str]) {
    let mut done = Vec::new();
    for sc in scenarios() {
        if !only.is_empty() && !only.contains(&sc.name) {
            continue;
        }
        let t = std::time::Instant::now();
        let mut r = rig(new_emu);
        let hook = HOOKS[sc.hook].1;
        let p0 = r.pos();
        if let Some(d) = sc.dist {
            let to = [p0[0] - hook[0], p0[2] - hook[2]];
            let l = to[0].hypot(to[1]);
            let (x, z) = (hook[0] + to[0] / l * d, hook[2] + to[1] / l * d);
            // His floor under the spot (the proxy's ground cast from 3 m over the hook).
            let y = proxy::ground(&mut r.emu, x, z, hook[1] + 3.0).unwrap_or(p0[1]);
            r.put([x, y, z], 0.0);
        }
        let p = r.pos();
        let to_hook = yaw_to([p[0], p[1], p[2]], hook);
        if let Some(off) = sc.face {
            r.emu.poke_f32s(r.w.phys + 0x74, &[(to_hook + off).to_radians()]);
        }
        r.camera(to_hook + sc.cam_off);
        let start = r.pos();
        let yaw0 = proxy::f32s(&mut r.emu, r.w.phys + 0x74, 1)[0];
        let mut frames = Vec::new();
        let mut stopped = None;
        for f in 0..sc.frames {
            match r.frame(&(sc.input)(f)) {
                Ok(v) => frames.push(v),
                Err(e) => {
                    stopped = Some(e);
                    break;
                }
            }
        }
        let mut seen: Vec<i64> = Vec::new();
        for v in &frames {
            let a = v["anim"].as_i64().unwrap();
            if seen.last() != Some(&a) {
                seen.push(a);
            }
        }
        println!("{}: start {start:?} yaw {:.1} cam {:.1}; {} frames {:.0} s, anims {seen:?}, target {} end {:?} stopped {stopped:?}",
            sc.name, yaw0.to_degrees(), to_hook + sc.cam_off, frames.len(), t.elapsed().as_secs_f32(),
            frames.get(2).map_or(json!(null), |v| v["target"]["state"].clone()), frames.last().map(|v| v["pos"].clone()));
        done.push(json!({"name": sc.name, "dt": DT, "hook_name": HOOKS[sc.hook].0, "hook_export": hook, "start": start,
            "yaw0": yaw0, "cam_yaw": (to_hook + sc.cam_off).to_radians(), "frames": frames, "stopped": stopped}));
    }
    let name = match only.first() {
        Some(n) => format!("wire_{n}"),
        None => "wire_frames".into(),
    };
    let body = json!({"function": format!("{WIRE_UPDATE:#x} + {CHR_FRAME:#x}"), "scenarios": done});
    std::fs::write(out.join(format!("{name}.json")), serde_json::to_string(&body).unwrap()).unwrap();
    println!("  -> {name}.json");
}

pub(super) fn run(new_emu: &dyn Fn() -> Emu, out: &Path, args: &[&str]) {
    match args.first() {
        Some(&"wire_probe") => probe(new_emu),
        Some(&"wire_sweep") => sweep(new_emu),
        _ => run_scenarios(new_emu, out, &args[1..]),
    }
}
