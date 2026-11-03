//! FXR particle runtime (topic fxr2): the exe's particle pool update FUN_141dd6f60(pool, node, dt,
//! emitter) run frame by frame on one config of each effect RSekiro plays most (deflect sparks
//! 252001, hit sparks 201001 / 201002, blood 220502, firecracker 300071), at 30 and 60 fps.
//!
//! One call per frame does what the game does for a particle node: age every particle by dt x its
//! time scale (+8) and drop the ones whose age reached their duration, ask the emitter for this
//! frame's count (vtable +8: PeriodicEmitter 300 FUN_141daabd0 -> FUN_141dab630, OneTimeEmitter
//! 399 FUN_141daf530), give each new particle its random (+0x28, an FXR RNG draw), time scale
//! (duration without / with its modifiers, FUN_141dd5c90) and speed (particleModifier's speed), place
//! it with the spread (slot 6, FUN_141dd5880) and the shape (slot 5, FUN_141dd5800), then run the
//! particle movement object (pool +0x50, vtable +0x18 / +0x20 / +0x10; 55 FUN_141dbbe50 ->
//! FUN_141dbc0f0, 60 FUN_141dbe330, 65 FUN_141dbf570, 105 FUN_141dbd3f0).
//!
//! The config is built in the binary layout the loader leaves (fxr.ts writeAction: an action is
//! 0x40 bytes, type at +0, handler index +4, field / property counts +8 / +0x10, fields +0x20,
//! properties +0x30; a config's actions at [cfg +0x18], slot i at +0x40 i, type 1004 at cfg +0). The
//! node is a stand-in with its world matrix at +0x70 (identity), active time +0xc0 and random +0xcc.
//! The particle record (0x2c): +0 age, +4 this frame's step, +8 time scale, +0xc position, +0x18
//! direction, +0x24 speed, +0x28 random. Writes extracted/emu/fxr2_frames.json for
//! src/sim_tests/emu_fxr2.rs.

use super::fxr::{mod_fields, prop_fields, table, PROP_TABLE, MOD_TABLE, RNG};
use super::*;

const POOL_UPDATE: u64 = 0x1_41dd_6f60;
const FILES: &[i64] = &[252001, 201001, 201002, 220502, 300071];
/// (type, constructor(obj, node, cfg / action), object size).
const EMITTERS: &[(i64, u64, u64)] = &[(300, 0x1_41da_98d0, 0x20), (399, 0x1_41da_ef30, 0x18)];
const MOVERS: &[(i64, u64, u64)] = &[(55, 0x1_41db_bc70, 0x10), (60, 0x1_41db_e150, 0x10), (65, 0x1_41db_f2a0, 0x50), (105, 0x1_41db_d050, 0x50)];
/// Spread (slot 6) and shape (slot 5) handler tables (FUN_141dd19a0 / FUN_141dd1950: entry 0x30
/// at [table +8] indexed by the action's +4).
const SPREAD_HANDLERS: u64 = 0x1_43f4_3a50;
const SHAPE_HANDLERS: u64 = 0x1_43f4_3a78;
/// Effect globals (FUN_141dc6760 / 6770 / 6670 and the negative-interval value).
const FPS: u64 = 0x1_43bd_eb38;
const COUNT_SCALE: u64 = 0x1_43bd_eb3c;
const INTERVAL_SCALE: u64 = 0x1_43bd_eb40;
const NEG_INTERVAL: u64 = 0x1_43bb_e8e0;
const CAPACITY: u32 = 256;
const NODE_RANDOM: u32 = 0x1357_9bdf;

/// (fields, properties with defaults) of an action, in the SDT order (reference/fxr src/actions).
fn layout(ty: i64) -> (&'static [(&'static str, f64)], &'static [(&'static str, &'static [f32])]) {
    match ty {
        300 => (&[("unk_ds3_f1_1", 1.0)], &[("interval", &[1.0]), ("perEmission", &[1.0]), ("totalEmissions", &[-1.0]), ("maxConcurrent", &[-1.0])]),
        400 => (&[("direction", 0.0)], &[]),
        401 => (&[("direction", 0.0)], &[("radius", &[1.0]), ("distribution", &[0.0])]),
        402 => (&[("direction", 0.0)], &[("sizeX", &[1.0]), ("sizeY", &[1.0]), ("distribution", &[0.0])]),
        403 => (&[("emitInside", 1.0)], &[("radius", &[1.0])]),
        404 => (&[("direction", 0.0), ("emitInside", 1.0)], &[("sizeX", &[1.0]), ("sizeY", &[1.0]), ("sizeZ", &[1.0])]),
        405 => (&[("direction", 0.0), ("emitInside", 1.0)], &[("radius", &[1.0]), ("height", &[1.0])]),
        501 => (&[], &[("angle", &[30.0]), ("distribution", &[0.0])]),
        502 | 503 => (&[], &[("angleX", &[30.0]), ("angleY", &[30.0]), ("distribution", &[0.0])]),
        129 => (&[("attachment", 1.0)], &[("duration", &[-1.0])]),
        131 => (&[("uniformScale", 0.0)], &[("speed", &[0.0]), ("scaleX", &[1.0]), ("scaleY", &[1.0]), ("scaleZ", &[1.0]), ("color", &[1.0, 1.0, 1.0, 1.0])]),
        55 => (&[("unk_ds3_f1_0", 0.0), ("unk_ds3_f1_1", 0.0)], &[("gravity", &[0.0]), ("acceleration", &[0.0]), ("accelerationMultiplier", &[1.0])]),
        60 => (&[("unk_ds3_f1_0", 0.0), ("unk_ds3_f1_1", 0.0)], &[("gravity", &[0.0]), ("speed", &[0.0]), ("speedMultiplier", &[1.0])]),
        65 => (&[("unk_ds3_f1_0", 0.0), ("turnInterval", 0.0), ("followRotation", 0.0)], &[("gravity", &[0.0]), ("speed", &[0.0]), ("speedMultiplier", &[1.0]), ("maxTurnAngle", &[0.0]), ("followFactor", &[0.0])]),
        105 => (&[("unk_ds3_f1_0", 0.0), ("turnInterval", 0.0), ("followRotation", 0.0)], &[("gravity", &[0.0]), ("acceleration", &[0.0]), ("accelerationMultiplier", &[1.0]), ("maxTurnAngle", &[0.0]), ("followFactor", &[0.0])]),
        _ => (&[], &[]),
    }
}

struct Builder<'a> {
    props_t: &'a HashMap<u32, (u64, u64)>,
    mods_t: &'a HashMap<u32, (u64, u64)>,
}

impl Builder<'_> {
    /// Writes one property record (0x28) at `at` with its fields and modifiers.
    fn prop(&self, emu: &mut Emu, at: u64, p: Option<&Value>, default: &[f32]) {
        let p = match p {
            Some(v @ Value::Object(_)) => v.clone(),
            Some(v @ (Value::Number(_) | Value::Array(_))) => json!({ "value": v }),
            Some(Value::Bool(b)) => json!({ "value": *b as i32 }),
            _ if default.len() == 1 => json!({ "value": default[0] }),
            _ => json!({ "value": default }),
        };
        let (enum_a, w, n) = prop_fields(&p).expect("property layout");
        let (index, _) = self.props_t[&enum_a];
        let fields = emu.alloc(4 * w.len() as u64 + 16);
        emu.write(fields, &w.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<_>>());
        let ms: Vec<(u32, Vec<u32>)> = p["modifiers"].as_array().map(|a| a.iter().filter_map(|m| mod_fields(m, n)).collect()).unwrap_or_default();
        let mods = emu.alloc(0x20 * ms.len() as u64 + 16);
        for (i, (ea, mw)) in ms.iter().enumerate() {
            let (mi, _) = self.mods_t[ea];
            let mf = emu.alloc(4 * mw.len() as u64 + 16);
            emu.write(mf, &mw.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<_>>());
            let mut m = vec![0u8; 0x20];
            m[0..2].copy_from_slice(&(*ea as u16).to_le_bytes());
            m[3] = 1;
            m[4..8].copy_from_slice(&(mi as u32).to_le_bytes());
            m[8..12].copy_from_slice(&(mw.len() as u32).to_le_bytes());
            m[0x10..0x18].copy_from_slice(&mf.to_le_bytes());
            emu.write(mods + 0x20 * i as u64, &m);
        }
        let mut pb = vec![0u8; 0x28];
        pb[0..2].copy_from_slice(&(enum_a as u16).to_le_bytes());
        pb[3] = 1;
        pb[4..8].copy_from_slice(&(index as u32).to_le_bytes());
        pb[8..12].copy_from_slice(&(w.len() as u32).to_le_bytes());
        pb[0x10..0x18].copy_from_slice(&fields.to_le_bytes());
        pb[0x18..0x20].copy_from_slice(&mods.to_le_bytes());
        pb[0x20..0x24].copy_from_slice(&(ms.len() as u32).to_le_bytes());
        emu.write(at, &pb);
    }

    /// Writes an action (0x40) at `at` from its JSON (missing keys take the defaults).
    fn action(&self, emu: &mut Emu, at: u64, a: &Value, handler: u32) {
        let ty = a["type"].as_i64().unwrap_or(0);
        let (fields, props) = layout(ty);
        let fptr = emu.alloc(4 * fields.len() as u64 + 16);
        for (i, (k, d)) in fields.iter().enumerate() {
            let bits = match a.get(*k) {
                Some(Value::Bool(b)) => *b as u32,
                Some(Value::Number(n)) if n.is_i64() => n.as_i64().unwrap() as u32,
                Some(Value::Number(n)) => (n.as_f64().unwrap() as f32).to_bits(),
                _ if d.fract() != 0.0 => (*d as f32).to_bits(),
                _ => *d as i32 as u32,
            };
            emu.u32(fptr + 4 * i as u64, bits);
        }
        let pptr = emu.alloc(0x28 * props.len() as u64 + 16);
        for (i, (k, d)) in props.iter().enumerate() {
            self.prop(emu, pptr + 0x28 * i as u64, a.get(*k), d);
        }
        let mut b = vec![0u8; 0x40];
        b[0..2].copy_from_slice(&(ty as u16).to_le_bytes());
        b[4..8].copy_from_slice(&handler.to_le_bytes());
        b[8..12].copy_from_slice(&(fields.len() as u32).to_le_bytes());
        b[0x10..0x14].copy_from_slice(&(props.len() as u32).to_le_bytes());
        b[0x20..0x28].copy_from_slice(&fptr.to_le_bytes());
        b[0x30..0x38].copy_from_slice(&pptr.to_le_bytes());
        emu.write(at, &b);
    }
}

/// The handler index of action `ty` in a handler table (entries 0x30, the type's u16 at +0).
fn handler_index(emu: &mut Emu, tab: u64, ty: i64) -> Option<u32> {
    let base = emu.peek_u64(tab + 8);
    (0..256u64).find(|i| u16::from_le_bytes(emu.peek(base + 0x30 * i)) as i64 == ty).map(|i| i as u32)
}

/// Every particle config (emitter 300 / 399 with ParticleAttributes 129) under `v`, once each.
fn configs(v: &Value, path: String, out: &mut Vec<(String, Value)>, seen: &mut HashSet<String>) {
    match v {
        Value::Object(o) => {
            let em = o.get("emitter").and_then(|e| e["type"].as_i64());
            let pa = o.get("particleAttributes").and_then(|e| e["type"].as_i64());
            if matches!(em, Some(300 | 399)) && pa == Some(129) && seen.insert(v.to_string()) {
                out.push((path.clone(), v.clone()));
            }
            for (k, x) in o {
                configs(x, format!("{path}/{k}"), out, seen);
            }
        }
        Value::Array(a) => {
            for (i, x) in a.iter().enumerate() {
                configs(x, format!("{path}/{i}"), out, seen);
            }
        }
        _ => {}
    }
}

/// Runs one config through the pool update for `frames` frames of `dt`, the node's world matrix
/// (+0x70, 3x4 rows [R | t]) `node_m`: (per-frame rows, the emitter's cap).
fn simulate(emu: &mut Emu, cfg_json: &Value, path: &str, fps: u32, frames: usize, node_m: &[f32; 12]) -> (Vec<Value>, u32) {
    let props_t = table(emu, PROP_TABLE);
    let mods_t = table(emu, MOD_TABLE);
    let b = Builder { props_t: &props_t, mods_t: &mods_t };
    emu.st.heap_next = HEAP;
    let actions = emu.alloc(0x40 * 15);
    emu.write(actions, &vec![0u8; 0x40 * 15]);
    let cfg = emu.alloc(0x40);
    emu.write(cfg, &[0u8; 0x40]);
    emu.write(cfg, &1004u16.to_le_bytes());
    emu.u64(cfg + 0x18, actions);
    let slot = |i: u64| actions + 0x40 * i;
    let shape_ty = cfg_json["emitterShape"]["type"].as_i64().unwrap_or(400);
    let spread_ty = cfg_json["directionSpread"]["type"].as_i64().unwrap_or(500);
    let hs = handler_index(emu, SHAPE_HANDLERS, shape_ty).unwrap_or(0);
    let hp = handler_index(emu, SPREAD_HANDLERS, spread_ty).unwrap_or(0);
    b.action(emu, slot(4), &cfg_json["emitter"], 0);
    let or = |k: &str, ty: i64| if cfg_json[k].is_object() { cfg_json[k].clone() } else { json!({ "type": ty }) };
    b.action(emu, slot(5), &or("emitterShape", 400), hs);
    b.action(emu, slot(6), &or("directionSpread", 500), hp);
    b.action(emu, slot(7), &cfg_json["particleModifier"], 0);
    b.action(emu, slot(8), &cfg_json["particleAttributes"], 0);
    let pm_ty = cfg_json["particleMovement"]["type"].as_i64();
    if pm_ty.is_some() {
        b.action(emu, slot(10), &cfg_json["particleMovement"], 0);
    }
    // the node: world matrix +0x70, active time +0xc0, random +0xcc
    let node = emu.alloc(0x200);
    emu.write(node, &[0u8; 0x200]);
    for (i, v) in node_m.iter().enumerate() {
        emu.f32(node + 0x70 + 4 * i as u64, *v);
    }
    emu.u32(node + 0xcc, NODE_RANDOM);
    let em_ty = cfg_json["emitter"]["type"].as_i64().unwrap();
    let &(_, ctor, size) = EMITTERS.iter().find(|e| e.0 == em_ty).unwrap();
    let em = emu.alloc(size + 0x10);
    emu.write(em, &vec![0u8; size as usize]);
    let r = emu.call(ctor, &[em, node, cfg], &[]);
    assert!(r.stopped.is_none(), "emitter ctor stopped {:x?}", r.stopped);
    let mover = match pm_ty.and_then(|t| MOVERS.iter().find(|m| m.0 == t)) {
        Some(&(_, mctor, msize)) => {
            let m = emu.alloc(msize + 0x10);
            emu.write(m, &vec![0u8; msize as usize]);
            let r = emu.call(mctor, &[m, node, slot(10), 0], &[]);
            assert!(r.stopped.is_none(), "mover ctor stopped {:x?}", r.stopped);
            m
        }
        None => 0,
    };
    // the pool: +0 frame, +0x30 bounds, +0x50 movement, +0x68 records, +0x70 config,
    // +0x78 count, +0x7c base, +0x80 capacity
    let pool = emu.alloc(0x100);
    emu.write(pool, &[0u8; 0x100]);
    let recs = emu.alloc(0x2c * CAPACITY as u64 + 16);
    emu.write(recs, &vec![0u8; 0x2c * CAPACITY as usize]);
    emu.u64(pool + 0x50, mover);
    emu.u64(pool + 0x68, recs);
    emu.u64(pool + 0x70, cfg);
    emu.u32(pool + 0x80, CAPACITY);
    let rng = emu.call(RNG, &[], &[]).rax;
    let dt = 1.0 / fps as f32;
    let mut rows = Vec::new();
    let mut stopped = None;
    for k in 0..frames {
        let t = k as f32 * dt;
        emu.f32(node + 0xc0, t);
        let state: Vec<u32> = (0..4).map(|i| u32::from_le_bytes(emu.peek(rng + 4 + 4 * i))).collect();
        let before = u32::from_le_bytes(emu.read(pool + 0x78));
        let r = emu.call(POOL_UPDATE, &[pool, node, dt.to_bits() as u64, em], &[0.0, 0.0, dt]);
        proxy::flush_tb(&emu);
        if r.stopped.is_some() {
            stopped = r.stopped;
            break;
        }
        let n = u32::from_le_bytes(emu.read(pool + 0x78));
        let ps: Vec<Vec<f32>> = (0..n as u64)
            .map(|i| {
                let p = recs + 0x2c * i;
                let mut v: Vec<f32> = (0..10).map(|j| emu.rf32(p + 4 * j)).collect();
                v.push(f32::from_bits(u32::from_le_bytes(emu.read(p + 0x28))));
                v
            })
            .collect();
        let ps: Vec<Value> = ps.iter().map(|v| json!({"age": v[0], "step": v[1], "scale": v[2], "pos": &v[3..6], "dir": &v[6..9], "speed": v[9], "random": v[10].to_bits()})).collect();
        let e: Vec<f32> = (0..2).map(|j| emu.rf32(em + 0x10 + 4 * j)).collect();
        rows.push(json!({"t": t, "rng": state, "before": before, "count": n, "particles": ps, "emitter_next": e[0], "emitter_count": e[1].to_bits()}));
    }
    if let Some(s) = stopped {
        println!("fxr2: {path} {fps} fps stopped at {s:x?} after {} frames", rows.len());
    }
    let cap = u32::from_le_bytes(emu.read(em + 0x18));
    (rows, cap)
}

const IDENTITY_M: [f32; 12] = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0];

pub(crate) fn run(new_emu: &dyn Fn() -> Emu, out: &Path, _want: &[&str]) {
    let dir = out.parent().unwrap().join("fxr");
    let mut chosen = Vec::new();
    let mut seen = HashSet::new();
    for id in FILES {
        let Ok(s) = std::fs::read_to_string(dir.join(format!("{id}.json"))) else { continue };
        let v: Value = serde_json::from_str(&s).unwrap();
        configs(&v, id.to_string(), &mut chosen, &mut seen);
    }
    println!("fxr2: {} particle configs", chosen.len());
    let mut globals = Value::Null;
    let mut cases = Vec::new();
    for (path, cfg_json) in &chosen {
        for fps in [30u32, 60] {
            let mut emu = new_emu();
            if globals.is_null() {
                globals = json!({
                    "fps": u32::from_le_bytes(emu.peek(FPS)),
                    "count_scale": f32::from_le_bytes(emu.peek(COUNT_SCALE)),
                    "interval_scale": f32::from_le_bytes(emu.peek(INTERVAL_SCALE)),
                    "negative_interval_bits": u32::from_le_bytes(emu.peek(NEG_INTERVAL)),
                });
                println!("fxr2: globals {globals}");
            }
            let (rows, cap) = simulate(&mut emu, cfg_json, path, fps, (2.5 * fps as f32) as usize, &IDENTITY_M);
            let dt = 1.0 / fps as f32;
            cases.push(json!({"path": path, "fps": fps, "dt": dt, "config": cfg_json, "node_random": NODE_RANDOM, "emitter_cap": cap, "frames": rows}));
        }
        println!("fxr2: {path} done");
    }
    // InitialDirection (the shape's `direction` field, frames by FUN_141db46a0): a point shape
    // with no spread under a turned and moved node, each mode 0-6 with attachment 0 (world) and 1
    // (the node), one frame: the new particle's direction and position in the pool's frame.
    let (sy, cy) = 0.7f32.sin_cos();
    let (sx, cx) = 0.4f32.sin_cos();
    // R = Ry(0.7) Rx(0.4) as columns-are-axes rows [R | t], t = (1, 2, 3)
    let r = [[cy, sy * sx, sy * cx], [0.0, cx, -sx], [-sy, cy * sx, cy * cx]];
    let turned: [f32; 12] = [r[0][0], r[0][1], r[0][2], 1.0, r[1][0], r[1][1], r[1][2], 2.0, r[2][0], r[2][1], r[2][2], 3.0];
    let mut dirs = Vec::new();
    for attach in [0, 1] {
        for mode in 0..=6 {
            let cfg = json!({
                "emitter": {"type": 399},
                "emitterShape": {"type": 400, "direction": mode},
                "directionSpread": {"type": 500},
                "particleModifier": {"type": 131, "speed": 1.0},
                "particleAttributes": {"type": 129, "duration": 1.0, "attachment": attach},
            });
            let mut emu = new_emu();
            let (rows, _) = simulate(&mut emu, &cfg, "direction", 60, 1, &turned);
            let p = &rows[0]["particles"][0];
            dirs.push(json!({"mode": mode, "attachment": attach, "dir": p["dir"], "pos": p["pos"]}));
        }
    }
    let body = json!({
        "about": "FXR particle pool update FUN_141dd6f60 of sekiro.exe, frame by frame, on the 2026-11-03 snapshot",
        "globals": globals,
        "cases": cases,
        "node_matrix": turned,
        "directions": dirs,
    });
    std::fs::write(out.join("fxr2_frames.json"), serde_json::to_string(&body).unwrap()).unwrap();
}
