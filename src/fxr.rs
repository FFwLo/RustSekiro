//! FXR player: the game's own effects (sfx/sfxbnd_commoneffects.ffxbnd f<id>.fxr, converted to
//! extracted/fxr/<id>.json by tools/fxr_extract.py with @cccode/fxr; textures extracted/fxr_tex/<id>.png
//! from the archive's s<id>.tpf). Semantics follow the fxr library's action definitions
//! (reference/fxr src/actions/*.yml, public domain): the node tree (2000 root, 2200 basic nodes,
//! 2001 references, 2202 levels of detail), configs (1004 particles, 1005 child nodes), emitters
//! (399 once, 300 periodic, 301 by distance), emitter shapes (400-405), spread (500-503), particle
//! movement (55/60/84/105/64/65), properties over Constant0 / ParticleAge / EmissionTime / ActiveTime
//! with Linear / Stepped / Hermite / Bezier keyframes and RandomDelta / RandomFraction modifiers.
//! Drawn: BillboardEx (603), MultiTextureBillboardEx (604: all three layers, `MultiMaterial`,
//! fx_multi.wgsl), PointSprite (600),
//! QuadLine (602), Tracer / LegacyTracer (10012 / 606), PointLight (609), Model (605: the
//! s<model>.flver meshes exported to extracted/fxr_model), and the screen effects Distortion (607),
//! RadialBlur (608) and the tracers' distortionIntensity (`DistortMaterial`, fx_distort.wgsl).
//! gap: WindForce (10300), normal maps, lighting / specular, node random turns, sounds (nodeAudio), and the
//! Hermite curves are the library's approximation (the game's formula is not known).

use bevy::prelude::*;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

// ---------------------------------------------------------------- properties

/// Deterministic per-particle randomness: the k-th random number of a seed.
#[derive(Clone, Copy)]
struct Rnd {
    seed: u32,
    k: u32,
}

impl Rnd {
    fn next(&mut self) -> f32 {
        self.k = self.k.wrapping_add(1);
        let mut x = self.seed ^ self.k.wrapping_mul(0x9E37_79B9);
        x ^= x >> 16;
        x = x.wrapping_mul(0x7feb_352d);
        x ^= x >> 15;
        x = x.wrapping_mul(0x846c_a68b);
        x ^= x >> 16;
        (x % 1_000_000) as f32 / 1_000_000.0
    }
}

fn num(v: &Value) -> f32 {
    v.as_f64().unwrap_or(0.0) as f32
}

/// A value as up to 4 components.
fn vec4(v: &Value) -> [f32; 4] {
    match v {
        Value::Array(a) => {
            let mut o = [0.0; 4];
            for (i, x) in a.iter().take(4).enumerate() {
                o[i] = num(x);
            }
            if a.len() == 1 {
                o = [o[0]; 4];
            }
            o
        }
        Value::Bool(b) => [if *b { 1.0 } else { 0.0 }; 4],
        _ => [num(v); 4],
    }
}

fn lerp4(a: [f32; 4], b: [f32; 4], k: [f32; 4]) -> [f32; 4] {
    [a[0] + (b[0] - a[0]) * k[0], a[1] + (b[1] - a[1]) * k[1], a[2] + (b[2] - a[2]) * k[2], a[3] + (b[3] - a[3]) * k[3]]
}

/// fxr.ts cssCubicBezier: y at x for the curve (0,0) (x1,y1) (x2,y2) (1,1).
fn css_bezier(x1: f32, y1: f32, x2: f32, y2: f32, x: f32) -> f32 {
    if x <= 0.0 || x >= 1.0 {
        return x.clamp(0.0, 1.0);
    }
    let bez = |a: f32, b: f32, t: f32| t * ((t + 3.0 * (a - b) * t + (3.0 * b - 6.0 * a)) * t + 3.0 * a);
    let slope = |a: f32, b: f32, t: f32| 3.0 * (t * (t + 3.0 * (a - b) * t + 2.0 * (b - 2.0 * a)) + a);
    let mut t = x;
    for _ in 0..8 {
        let s = slope(x1, x2, t);
        if s.abs() < 1e-5 {
            break;
        }
        t -= (bez(x1, x2, t) - x) / s;
        t = t.clamp(0.0, 1.0);
    }
    bez(y1, y2, t)
}

/// fxr.ts approxHermite (the library's approximation of the game's Curve2 interpolation).
fn approx_hermite(t1: f32, t2: f32, x: f32) -> f32 {
    let f = |i: usize, x: f32| -> f32 {
        match i {
            0 => css_bezier(0.3, 0.1, 0.7, 0.9, x),
            1 => css_bezier(0.135, 0.135, 0.525, 1.0, x),
            2 => css_bezier(0.015, 0.675, 0.33, 1.0, x),
            3 => css_bezier(0.475, 0.0, 0.865, 0.865, x),
            4 => x,
            5 => css_bezier(0.015, 0.9, 0.5, 0.5, x),
            6 => css_bezier(0.71, 0.0, 0.985, 0.37, x),
            7 => css_bezier(0.525, 0.525, 0.965, 0.07, x),
            _ => css_bezier(0.065, 1.4, 0.935, -0.4, x),
        }
    };
    let (t1x2, t2x2) = (t1 * 2.0, t2 * 2.0);
    let ix = t1x2.floor().clamp(0.0, 1.0) as usize;
    let iy = t2x2.floor().clamp(0.0, 1.0) as usize;
    let i = ix + 3 * iy;
    let fx = t1x2 - ix as f32;
    let a = f(i, x) + (f(i + 1, x) - f(i, x)) * fx;
    let b = f(i + 3, x) + (f(i + 4, x) - f(i + 3, x)) * fx;
    a + (b - a) * (t2x2 - iy as f32)
}

/// A property at argument `t`, with its modifiers drawn from `rnd`.
fn prop(v: Option<&Value>, t: f32, rnd: &mut Rnd, default: [f32; 4]) -> [f32; 4] {
    let Some(v) = v else { return default };
    if v.is_null() {
        return default;
    }
    let base = match v.get("function").and_then(Value::as_str) {
        Some(func) => {
            let kf = v["keyframes"].as_array().map(|a| a.as_slice()).unwrap_or(&[]);
            if kf.is_empty() {
                default
            } else {
                let last = num(&kf[kf.len() - 1]["position"]);
                let t = if v["loop"].as_bool() == Some(true) && last > 0.0 { t.rem_euclid(last) } else { t };
                let i = kf.iter().rposition(|k| num(&k["position"]) <= t);
                match i {
                    None => vec4(&kf[0]["value"]),
                    Some(i) if i + 1 >= kf.len() => vec4(&kf[i]["value"]),
                    Some(i) => {
                        let (a, b) = (&kf[i], &kf[i + 1]);
                        let (pa, pb) = (num(&a["position"]), num(&b["position"]));
                        let x = if pb > pa { (t - pa) / (pb - pa) } else { 1.0 };
                        let (va, vb) = (vec4(&a["value"]), vec4(&b["value"]));
                        match func {
                            "Stepped" => va,
                            "Hermite" => {
                                let (t1, t2) = (vec4(&a["t1"]), vec4(&a["t2"]));
                                let k: [f32; 4] = std::array::from_fn(|c| approx_hermite(t1[c] / std::f32::consts::PI * 2.0, t2[c] / std::f32::consts::PI * 2.0, x));
                                lerp4(va, vb, k)
                            }
                            "Bezier" => {
                                let (p1, p2) = (vec4(&a["p1"]), vec4(&b["p2"]));
                                let cubic = |v1: f32, c1: f32, c2: f32, v2: f32| {
                                    let u = 1.0 - x;
                                    u * u * u * v1 + 3.0 * u * u * x * c1 + 3.0 * u * x * x * c2 + x * x * x * v2
                                };
                                std::array::from_fn(|c| cubic(va[c], va[c] + p1[c] / 3.0, vb[c] - p2[c] / 3.0, vb[c]))
                            }
                            _ => lerp4(va, vb, [x; 4]),
                        }
                    }
                }
            }
        }
        None => match v.get("value") {
            Some(inner) => vec4(inner),
            None => vec4(v),
        },
    };
    let mut out = base;
    if let Some(mods) = v.get("modifiers").and_then(Value::as_array) {
        for m in mods {
            let max = vec4(&m["max"]);
            match m["type"].as_str() {
                Some("RandomDelta") => {
                    for c in 0..4 {
                        out[c] += (rnd.next() * 2.0 - 1.0) * max[c];
                    }
                }
                Some("RandomFraction") => {
                    for c in 0..4 {
                        out[c] += out[c] * (rnd.next() * 2.0 - 1.0) * max[c];
                    }
                }
                Some("RandomRange") => {
                    let min = vec4(&m["min"]);
                    for c in 0..4 {
                        out[c] += min[c] + (max[c] - min[c]) * rnd.next();
                    }
                }
                _ => {}
            }
        }
    }
    out
}

fn scalar(a: &Value, key: &str, t: f32, rnd: &mut Rnd, default: f32) -> f32 {
    prop(a.get(key), t, rnd, [default; 4])[0]
}

fn int(a: &Value, key: &str, default: i64) -> i64 {
    match a.get(key) {
        Some(Value::Number(n)) => n.as_i64().unwrap_or(n.as_f64().unwrap_or(default as f64) as i64),
        Some(Value::Bool(b)) => *b as i64,
        Some(v) => v.get("value").and_then(Value::as_f64).map_or(default, |f| f as i64),
        None => default,
    }
}

// ---------------------------------------------------------------- game space

/// Game (FromSoft) space -> ours: X mirrored (model.rs, the root motion export); a rotation's Y and Z
/// angles flip with it. FXR rotations are degrees, applied Z -> X -> Y.
fn game_vec(v: [f32; 4]) -> Vec3 {
    Vec3::new(-v[0], v[1], v[2])
}

fn game_rot(r: [f32; 4]) -> Quat {
    let (x, y, z) = (r[0].to_radians(), -r[1].to_radians(), -r[2].to_radians());
    Quat::from_rotation_y(y) * Quat::from_rotation_x(x) * Quat::from_rotation_z(z)
}

// ---------------------------------------------------------------- runtime

struct Particle {
    /// World position (attachment None) or position in the node's frame.
    pos: Vec3,
    dir: Vec3,
    speed: f32,
    age: f32,
    life: f32,
    emit_t: f32,
    seed: u32,
    attached: bool,
    /// Tracer points (world position, source axis - zero = by travel, see TracerOrientationMode -
    /// and age at creation).
    trail: Vec<(Vec3, Vec3, f32)>,
    next_seg: f32,
    dead: bool,
    /// A tracer particle whose life (or effect) ended: it no longer moves or adds points, its
    /// segments age out over segmentDuration (the weapon trail fading after the swing), then it goes.
    ended: bool,
}

struct NodeInst {
    node: Arc<Value>,
    /// Active config (stateConfigMap[0]).
    cfg: Option<Arc<Value>>,
    local: Transform,
    world: Transform,
    time: f32,
    delay: f32,
    duration: f32,
    emit_next: f32,
    emissions: i64,
    last_emit_at: Option<Vec3>,
    particles: Vec<Particle>,
    children: Vec<NodeInst>,
    /// For node emitters (1005): templates to spawn per emission.
    templates: Vec<Arc<Value>>,
    seed: u32,
    finished: bool,
    /// NodeAccelerationPartialFollow / NodeSpeedPartialFollow followFactor fell to 0: the node stays
    /// where it was in the world (no longer follows its parent).
    detached: Option<Transform>,
}

/// What an effect rides on.
#[derive(Clone, Copy, Debug)]
pub enum Anchor {
    /// A dummy (or any entity): its global transform.
    Entity(Entity),
    /// A blade from hilt to tip: origin at the hilt, +Z along the blade (for a weapon whose own
    /// dummies are not modelled: the Mortal Blade's 12200 / 12220 ride on Kusabimaru's 300 -> 301).
    Blade(Entity, Entity),
}

/// One playing effect.
#[derive(Component)]
pub struct FxEffect {
    pub id: i64,
    /// The dummy (or actor) it rides on, and whether it follows it.
    pub anchor: Option<Anchor>,
    pub follow: bool,
    /// Placement when not following (world).
    pub at: Transform,
    /// Stop emitting (the TAE event ended / External(0) >= 1 then terminate).
    pub stop: bool,
    /// Owner key: (actor, anim, event index), to stop slotted effects with their event.
    pub key: Option<(Entity, String, usize)>,
    root: Option<NodeInst>,
    age: f32,
}

impl FxEffect {
    pub fn new(id: i64, anchor: Option<Anchor>, follow: bool, at: Transform) -> Self {
        FxEffect { id, anchor, follow, at, stop: false, key: None, root: None, age: 0.0 }
    }
}

#[derive(Resource, Default)]
pub struct FxrLib {
    cache: HashMap<i64, Option<Arc<Value>>>,
    models: HashMap<i64, Option<Arc<crate::model::FxModel>>>,
    /// Normal maps / masks of the screen effects in the effects read so far: their batches (and
    /// textures) are set up at once, so a short effect is not over before its texture has loaded.
    screen_tex: Vec<i64>,
}

/// The screen-effect textures in an effect: Distortion's normalMap, RadialBlur's mask, a
/// distorting tracer's normalMap.
fn screen_textures(v: &Value, out: &mut Vec<i64>) {
    match v {
        Value::Object(m) => {
            if let Some(a) = m.get("appearance") {
                let id = match a["type"].as_i64() {
                    Some(607) => a["normalMap"].as_i64(),
                    Some(608) => a["mask"].as_i64(),
                    Some(606 | 10012) if num(&a["distortionIntensity"]) > 0.0 => a["normalMap"].as_i64(),
                    _ => None,
                };
                if let Some(id) = id.filter(|id| *id > 0 && !out.contains(id)) {
                    out.push(id);
                }
            }
            m.values().for_each(|c| screen_textures(c, out));
        }
        Value::Array(a) => a.iter().for_each(|c| screen_textures(c, out)),
        _ => {}
    }
}

/// Batch key of a Model appearance: texture ids are >= -1, models live below this.
const MODEL_KEY: i64 = -1_000_000;

impl FxrLib {
    pub fn get(&mut self, id: i64) -> Option<Arc<Value>> {
        self.cache
            .entry(id)
            .or_insert_with(|| {
                let p = crate::paths::extracted().join(format!("fxr/{id}.json"));
                let v = std::fs::read_to_string(p).ok().and_then(|s| serde_json::from_str::<Value>(&s).ok());
                if let Some(v) = &v {
                    screen_textures(v, &mut self.screen_tex);
                }
                v.map(Arc::new)
            })
            .clone()
    }
    pub fn has(&mut self, id: i64) -> bool {
        self.get(id).is_some()
    }
    fn model(&mut self, id: i64) -> Option<Arc<crate::model::FxModel>> {
        self.models.entry(id).or_insert_with(|| crate::model::load_fx_model(id).map(Arc::new)).clone()
    }
}

fn config_of(node: &Value) -> Option<Arc<Value>> {
    let i = node["stateConfigMap"].as_array().and_then(|a| a.first()).and_then(Value::as_u64).unwrap_or(0) as usize;
    node["configs"].as_array().and_then(|c| c.get(i)).cloned().map(Arc::new)
}

fn make_node(lib: &mut FxrLib, node: &Value, seed: u32, depth: u32) -> Option<NodeInst> {
    if depth > 12 {
        return None;
    }
    let ty = node["type"].as_i64().unwrap_or(0);
    // ReferenceNode: another effect's root.
    if ty == 2001 {
        let r = lib.get(node["sfx"].as_i64().unwrap_or(-1))?;
        return make_node(lib, &r["root"], seed ^ 0x55, depth + 1);
    }
    let cfg = config_of(node);
    let mut rnd = Rnd { seed, k: 0 };
    let (mut local, mut delay, mut duration) = (Transform::IDENTITY, 0.0, -1.0);
    if let Some(c) = &cfg {
        let attrs = &c["nodeAttributes"];
        delay = scalar(attrs, "delay", 0.0, &mut rnd, 0.0);
        duration = scalar(attrs, "duration", 0.0, &mut rnd, -1.0);
        let tr = &c["nodeTransform"];
        let mut off = vec4(&tr["offset"]);
        let mut rot = vec4(&tr["rotation"]);
        if tr["type"].as_i64() == Some(36) {
            let (ov, rv) = (vec4(&tr["offsetVariance"]), vec4(&tr["rotationVariance"]));
            for i in 0..3 {
                off[i] += (rnd.next() * 2.0 - 1.0) * ov[i];
                rot[i] += (rnd.next() * 2.0 - 1.0) * rv[i];
            }
        }
        local = Transform::from_translation(game_vec(off)).with_rotation(game_rot(rot));
    }
    let is_node_emitter = cfg.as_ref().is_some_and(|c| c["type"].as_i64() == Some(1005) || c["type"].as_str() == Some("1005"));
    let kids: Vec<Value> = node["nodes"].as_array().cloned().unwrap_or_default();
    // Levels of detail: the closest (first) child only.
    let kids: Vec<Value> = if ty == 2202 { kids.into_iter().take(1).collect() } else { kids };
    let (children, templates) = if is_node_emitter {
        (Vec::new(), kids.into_iter().map(Arc::new).collect())
    } else {
        (kids.iter().enumerate().filter_map(|(i, k)| make_node(lib, k, seed.wrapping_mul(31).wrapping_add(i as u32 + 1), depth + 1)).collect(), Vec::new())
    };
    Some(NodeInst {
        node: Arc::new(node.clone()),
        cfg,
        local,
        world: Transform::IDENTITY,
        time: 0.0,
        delay,
        duration,
        emit_next: 0.0,
        emissions: 0,
        last_emit_at: None,
        particles: Vec::new(),
        children,
        templates,
        seed,
        finished: false,
        detached: None,
    })
}

/// A direction inside a cone of `angle` degrees around `n`.
fn spread_dir(n: Vec3, angle: f32, rnd: &mut Rnd) -> Vec3 {
    if angle <= 0.0 {
        return n;
    }
    let (t, b) = n.any_orthonormal_pair();
    let phi = rnd.next() * std::f32::consts::TAU;
    let cos_max = angle.to_radians().cos();
    let cos_t = 1.0 - rnd.next() * (1.0 - cos_max);
    let sin_t = (1.0 - cos_t * cos_t).max(0.0).sqrt();
    (n * cos_t + (t * phi.cos() + b * phi.sin()) * sin_t).normalize_or(n)
}

/// Emission point and direction in the node's frame, from its emitter shape and spread.
fn emit_point(c: &Value, t: f32, rnd: &mut Rnd) -> (Vec3, Vec3) {
    let shape = &c["emitterShape"];
    let st = shape["type"].as_i64().unwrap_or(400);
    let dirmode = int(shape, "direction", 0);
    let s = |k: &str, d: f32, rnd: &mut Rnd| scalar(shape, k, t, rnd, d);
    let (pos, outward) = match st {
        401 => {
            let r = s("radius", 1.0, rnd) * rnd.next().sqrt();
            let a = rnd.next() * std::f32::consts::TAU;
            let p = Vec3::new(a.cos() * r, a.sin() * r, 0.0);
            (p, p.normalize_or(Vec3::Z))
        }
        403 => {
            let r = s("radius", 1.0, rnd);
            let d = Vec3::new(rnd.next() * 2.0 - 1.0, rnd.next() * 2.0 - 1.0, rnd.next() * 2.0 - 1.0).normalize_or(Vec3::Y);
            let inside = shape["emitInside"].as_bool().unwrap_or(true);
            (d * if inside { r * rnd.next().cbrt() } else { r }, d)
        }
        404 => {
            let (x, y, z) = (s("sizeX", 1.0, rnd), s("sizeY", 1.0, rnd), s("sizeZ", 1.0, rnd));
            let p = Vec3::new((rnd.next() - 0.5) * x, (rnd.next() - 0.5) * y, (rnd.next() - 0.5) * z);
            (game_vec([p.x, p.y, p.z, 0.0]), Vec3::Z)
        }
        405 => {
            let (r, h) = (s("radius", 1.0, rnd), s("height", 1.0, rnd));
            let a = rnd.next() * std::f32::consts::TAU;
            let inside = shape["emitInside"].as_bool().unwrap_or(true);
            let rr = if inside { r * rnd.next().sqrt() } else { r };
            let y_axis = shape["yAxis"].as_bool().unwrap_or(true);
            let radial = Vec3::new(a.cos(), 0.0, a.sin());
            let along = (rnd.next() - 0.5) * h;
            if y_axis {
                (radial * rr + Vec3::Y * along, radial)
            } else {
                (Vec3::new(a.cos() * rr, a.sin() * rr, along), Vec3::new(a.cos(), a.sin(), 0.0))
            }
        }
        _ => (Vec3::ZERO, Vec3::Z),
    };
    // InitialDirection: 0 by shape, 1 / 2 global up / down, 3 global north, 4-6 local.
    let base = match dirmode {
        1 | 4 => Vec3::Y,
        2 | 5 => Vec3::NEG_Y,
        3 | 6 => Vec3::Z,
        _ => outward,
    };
    let sp = &c["directionSpread"];
    let dir = match sp["type"].as_i64().unwrap_or(500) {
        501 => spread_dir(base, scalar(sp, "angle", t, rnd, 30.0), rnd),
        502 | 503 => spread_dir(base, scalar(sp, "angleX", t, rnd, 30.0).max(scalar(sp, "angleY", t, rnd, 30.0)), rnd),
        _ => base,
    };
    (pos, dir)
}

fn step_node(n: &mut NodeInst, lib: &mut FxrLib, parent: Transform, dt: f32, stop: bool) {
    n.world = match n.detached {
        Some(w) => w * n.local,
        None => parent * n.local,
    };
    if n.time < n.delay {
        n.time += dt;
        for c in &mut n.children {
            step_node(c, lib, n.world, dt, stop);
        }
        return;
    }
    let active_t = n.time - n.delay;
    let alive = !stop && (n.duration < 0.0 || active_t < n.duration);
    // Node movement: speed / acceleration along its Z (1 / 106 / 122), spin (34).
    if let Some(c) = n.cfg.clone() {
        let mv = &c["nodeMovement"];
        let mut rnd = Rnd { seed: n.seed ^ 0xabc, k: 0 };
        match mv["type"].as_i64() {
            Some(1) | Some(106) | Some(122) => {
                let v = scalar(mv, "speedZ", 0.0, &mut rnd, 0.0) + scalar(mv, "accelerationZ", active_t, &mut rnd, 0.0) * active_t;
                let fwd = n.local.rotation * Vec3::Z;
                n.local.translation += fwd * v * dt;
                n.local.translation.y -= scalar(mv, "accelerationY", active_t, &mut rnd, 0.0) * active_t * dt;
            }
            Some(34) => {
                let s = |k: &str, rnd: &mut Rnd| scalar(mv, k, active_t, rnd, 0.0).to_radians() * dt;
                let (x, y, z) = (s("angularSpeedX", &mut rnd), -s("angularSpeedY", &mut rnd), -s("angularSpeedZ", &mut rnd));
                n.local.rotation = n.local.rotation * Quat::from_euler(EulerRot::YXZ, y, x, z);
            }
            _ => {}
        }
        // followFactor (106 / 122): at 0 the node lets go of its parent and stays in the world.
        if matches!(mv["type"].as_i64(), Some(106) | Some(122)) && n.detached.is_none() && mv.get("followFactor").is_some() {
            if scalar(mv, "followFactor", active_t, &mut rnd, 0.0) <= 0.0 {
                n.detached = Some(parent);
            }
        }
        n.world = match n.detached {
            Some(w) => w * n.local,
            None => parent * n.local,
        };
        // Emission.
        if alive {
            let em = &c["emitter"];
            let mut count = 0i64;
            let total = int(em, "totalEmissions", -1);
            match em["type"].as_i64().unwrap_or(399) {
                399 => {
                    if n.emissions == 0 {
                        count = 1;
                    }
                }
                300 => {
                    let interval = scalar(em, "interval", active_t, &mut rnd, 1.0).max(0.0);
                    let per = scalar(em, "perEmission", active_t, &mut rnd, 1.0).max(0.0).round() as i64;
                    let mut guard = 0;
                    while n.emit_next <= active_t && guard < 8 && (total < 0 || n.emissions < total) {
                        count += per;
                        n.emissions += 1;
                        n.emit_next += interval.max(1.0 / 60.0);
                        guard += 1;
                    }
                }
                301 => {
                    let th = scalar(em, "threshold", active_t, &mut rnd, 0.1).max(0.01);
                    let here = n.world.translation;
                    match n.last_emit_at {
                        None => {
                            n.last_emit_at = Some(here);
                            count = 1;
                        }
                        Some(p) if p.distance(here) >= th => {
                            n.last_emit_at = Some(here);
                            count = 1;
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
            if em["type"].as_i64() == Some(399) && count > 0 {
                n.emissions = 1;
            }
            let max_conc = int(em, "maxConcurrent", -1);
            for i in 0..count {
                if max_conc > 0 && (n.particles.len() + n.children.len()) as i64 >= max_conc {
                    break;
                }
                let pseed = n.seed.wrapping_mul(2_654_435_761).wrapping_add((n.emissions * 64 + i) as u32);
                let mut prnd = Rnd { seed: pseed, k: 0 };
                let (p, d) = emit_point(&c, active_t, &mut prnd);
                if !n.templates.is_empty() {
                    // NodeEmitter: child nodes at the emission point (200 all / 201 one at random).
                    let pick: Vec<Arc<Value>> = if c["nodeSelector"]["type"].as_i64() == Some(201) {
                        vec![n.templates[(prnd.next() * n.templates.len() as f32) as usize % n.templates.len()].clone()]
                    } else {
                        n.templates.clone()
                    };
                    for t in pick {
                        if let Some(mut child) = make_node(lib, &t, pseed ^ 0x77, 1) {
                            let rot = Quat::from_rotation_arc(Vec3::Z, d.normalize_or(Vec3::Z));
                            child.local = Transform::from_translation(p).with_rotation(rot) * child.local;
                            n.children.push(child);
                        }
                    }
                    continue;
                }
                let pa = &c["particleAttributes"];
                let life = scalar(pa, "duration", 0.0, &mut prnd, -1.0);
                let attached = int(pa, "attachment", 1) != 0;
                let world_p = n.world.transform_point(p);
                let world_d = (n.world.rotation * d).normalize_or(Vec3::Z);
                n.particles.push(Particle {
                    pos: if attached { p } else { world_p },
                    dir: if attached { d } else { world_d },
                    speed: scalar(&c["particleModifier"], "speed", active_t, &mut prnd, 0.0),
                    age: 0.0,
                    life,
                    emit_t: active_t,
                    seed: pseed,
                    attached,
                    trail: Vec::new(),
                    next_seg: 0.0,
                    dead: false,
                    ended: false,
                });
            }
        }
        // Particle movement.
        let pm = c["particleMovement"].clone();
        let pmt = pm["type"].as_i64().unwrap_or(0);
        for p in &mut n.particles {
            p.age += dt;
            if p.ended {
                p.dead = p.trail.is_empty();
                continue;
            }
            // A negative particle duration (-1, or the common -1/30) lives as long as its node: it
            // ends with the node's own duration as well as with the effect.
            if p.life >= 0.0 && p.age > p.life || (stop || !alive) && p.life < 0.0 {
                p.ended = p.trail.len() > 1;
                p.dead = !p.ended;
                continue;
            }
            let mut r = Rnd { seed: p.seed ^ 0x5151, k: 0 };
            let gravity = scalar(&pm, "gravity", p.age, &mut r, 0.0);
            let mut vel = p.dir * p.speed;
            match pmt {
                60 | 64 | 65 => {
                    let s = scalar(&pm, "speed", p.age, &mut r, 0.0) * scalar(&pm, "speedMultiplier", p.age, &mut r, 1.0);
                    vel = p.dir * (p.speed + s);
                }
                55 | 84 | 105 => {
                    let a = scalar(&pm, "acceleration", p.age, &mut r, 0.0) * scalar(&pm, "accelerationMultiplier", p.age, &mut r, 1.0);
                    p.speed += a * dt;
                    vel = p.dir * p.speed;
                }
                _ => {}
            }
            // Gravity (positive = down) bends the direction (the docs: it changes the current one).
            if gravity != 0.0 {
                let g = if p.attached { n.world.rotation.inverse() * Vec3::NEG_Y } else { Vec3::NEG_Y };
                let v = vel + g * gravity * dt;
                let s = v.length();
                if s > 1e-5 {
                    p.dir = v / s;
                    p.speed = s - if matches!(pmt, 60 | 64 | 65) { scalar(&pm, "speed", p.age, &mut r, 0.0) } else { 0.0 };
                }
                vel = v;
            }
            p.pos += vel * dt;
        }
        n.particles.retain(|p| !p.dead);
    }
    n.time += dt;
    let world = n.world;
    for ch in &mut n.children {
        step_node(ch, lib, world, dt, stop || !alive);
    }
    n.children.retain(|c| !c.finished);
    let emitting = alive && n.cfg.is_some();
    n.finished = !emitting && n.particles.is_empty() && n.children.is_empty() && (stop || n.duration >= 0.0 && n.time - n.delay >= n.duration || n.cfg.is_none());
}

/// Advances every effect; anchors move them.
fn tick(mut commands: Commands, time: Res<Time>, mut lib: ResMut<FxrLib>, globals: Query<&GlobalTransform>, mut fx: Query<(Entity, &mut FxEffect)>) {
    let dt = time.delta_secs().min(1.0 / 15.0);
    for (e, mut f) in &mut fx {
        let anchor = f.anchor.and_then(|a| match a {
            Anchor::Entity(e) => globals.get(e).ok().map(|g| g.compute_transform()),
            Anchor::Blade(h, t) => {
                let (h, t) = (globals.get(h).ok()?.translation(), globals.get(t).ok()?.translation());
                let dir = (t - h).normalize_or(Vec3::Z);
                Some(Transform::from_translation(h).with_rotation(Quat::from_rotation_arc(Vec3::Z, dir)))
            }
        });
        if f.root.is_none() {
            let Some(def) = lib.get(f.id) else {
                commands.entity(e).despawn();
                continue;
            };
            if !f.follow {
                if let Some(a) = anchor {
                    f.at = a;
                }
            }
            f.root = make_node(&mut lib, &def["root"], (f.id as u32).wrapping_mul(97).wrapping_add(e.index().index()), 0);
        }
        let place = if f.follow { anchor.unwrap_or(f.at) } else { f.at };
        let stop = f.stop;
        f.age += dt;
        let too_old = f.age > 20.0;
        let Some(root) = f.root.as_mut() else { continue };
        step_node(root, &mut lib, Transform { scale: Vec3::ONE, ..place }, dt, stop || too_old);
        if root.finished || f.age > 30.0 {
            commands.entity(e).despawn();
        }
    }
}

// ---------------------------------------------------------------- drawing

#[derive(Resource, Default)]
struct Batches {
    /// (texture id, blend mode) -> its mesh entity and buffers.
    map: HashMap<(i64, i64), (Entity, Handle<Mesh>)>,
}

/// Screen-effect quads (`DistortMaterial`), one mesh per texture (normal map or mask).
#[derive(Default)]
struct DistortBuf {
    pos: Vec<[f32; 3]>,
    uv: Vec<[f32; 2]>,
    tex_uv: Vec<[f32; 2]>,
    col: Vec<[f32; 4]>,
    fx: Vec<[f32; 4]>,
    center: Vec<[f32; 3]>,
}

impl DistortBuf {
    /// `uv` the particle's own square, `tex_uv` the texture's; `fx` as fx_distort.wgsl.
    fn quad(&mut self, c: [Vec3; 4], uv: [[f32; 2]; 4], tex_uv: [[f32; 2]; 4], col: [f32; 4], fx: [f32; 4], center: Vec3) {
        for i in [0, 1, 2, 0, 2, 3] {
            self.pos.push(c[i].to_array());
            self.uv.push(uv[i]);
            self.tex_uv.push(tex_uv[i]);
            self.col.push(col);
            self.fx.push(fx);
            self.center.push(center.to_array());
        }
    }
}

const ATTR_FX: bevy::mesh::MeshVertexAttribute = bevy::mesh::MeshVertexAttribute::new("FxDistortParams", 0x5ec1_0607, bevy::render::render_resource::VertexFormat::Float32x4);
const ATTR_CENTER: bevy::mesh::MeshVertexAttribute = bevy::mesh::MeshVertexAttribute::new("FxDistortCenter", 0x5ec1_0608, bevy::render::render_resource::VertexFormat::Float32x3);

/// Draws Distortion (607), RadialBlur (608) and tracer distortion: reads the frame behind it (the
/// opaque pass, Bevy's view transmission texture), so it renders in the Transmissive3d phase
/// (alpha mode Opaque there), blended premultiplied with no depth write (fx_distort.wgsl).
#[derive(Asset, TypePath, bevy::render::render_resource::AsBindGroup, Clone)]
pub struct DistortMaterial {
    #[texture(0)]
    #[sampler(1)]
    tex: Handle<Image>,
}

impl Material for DistortMaterial {
    fn vertex_shader() -> bevy::shader::ShaderRef {
        "embedded://sv1/fx_distort.wgsl".into()
    }
    fn fragment_shader() -> bevy::shader::ShaderRef {
        "embedded://sv1/fx_distort.wgsl".into()
    }
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Opaque
    }
    fn reads_view_transmission_texture(&self) -> bool {
        true
    }
    fn enable_prepass() -> bool {
        false
    }
    fn enable_shadows() -> bool {
        false
    }
    fn specialize(
        _pipeline: &bevy::pbr::MaterialPipeline,
        descriptor: &mut bevy::render::render_resource::RenderPipelineDescriptor,
        layout: &bevy::mesh::MeshVertexBufferLayoutRef,
        _key: bevy::pbr::MaterialPipelineKey<Self>,
    ) -> Result<(), bevy::render::render_resource::SpecializedMeshPipelineError> {
        descriptor.vertex.buffers = vec![layout.0.get_layout(&[
            Mesh::ATTRIBUTE_POSITION.at_shader_location(0),
            Mesh::ATTRIBUTE_UV_0.at_shader_location(1),
            Mesh::ATTRIBUTE_UV_1.at_shader_location(2),
            Mesh::ATTRIBUTE_COLOR.at_shader_location(3),
            ATTR_FX.at_shader_location(4),
            ATTR_CENTER.at_shader_location(5),
        ])?];
        if let Some(f) = descriptor.fragment.as_mut() {
            for t in f.targets.iter_mut().flatten() {
                t.blend = Some(bevy::render::render_resource::BlendState::PREMULTIPLIED_ALPHA_BLENDING);
            }
        }
        if let Some(ds) = descriptor.depth_stencil.as_mut() {
            ds.depth_write_enabled = Some(false);
        }
        descriptor.primitive.cull_mode = None;
        Ok(())
    }
}

#[derive(Resource, Default)]
struct DistortBatches(HashMap<i64, (Entity, Handle<Mesh>)>);

/// MultiTextureBillboardEx quads, one mesh per `MultiKey`.
#[derive(Default)]
struct MultiBuf {
    pos: Vec<[f32; 3]>,
    uv1: Vec<[f32; 2]>,
    uv2: Vec<[f32; 2]>,
    uv3n: Vec<[f32; 4]>,
    col: Vec<[f32; 4]>,
    c1: Vec<[f32; 4]>,
    c2: Vec<[f32; 4]>,
    c3: Vec<[f32; 4]>,
}

/// One multi-texture quad: per corner the layers' UVs (layer 1 with its next frame), the particle
/// colour and the three layer colours (c1.w: the frame fraction).
struct MultiQuad {
    corners: [Vec3; 4],
    uv1: [[f32; 2]; 4],
    uv1_next: [[f32; 2]; 4],
    uv2: [[f32; 2]; 4],
    uv3: [[f32; 2]; 4],
    col: [f32; 4],
    c1: [f32; 4],
    c2: [f32; 4],
    c3: [f32; 4],
}

impl MultiBuf {
    fn quad(&mut self, q: &MultiQuad) {
        for i in [0, 1, 2, 0, 2, 3] {
            self.pos.push(q.corners[i].to_array());
            self.uv1.push(q.uv1[i]);
            self.uv2.push(q.uv2[i]);
            self.uv3n.push([q.uv3[i][0], q.uv3[i][1], q.uv1_next[i][0], q.uv1_next[i][1]]);
            self.col.push(q.col);
            self.c1.push(q.c1);
            self.c2.push(q.c2);
            self.c3.push(q.c3);
        }
    }
}

/// (layer1, layer2, layer3 textures, TexBlendType | TexBlendType2 << 4 | TexBlendType3 << 8 |
/// ColorBlendType << 12 | premultiplyAlpha << 16, blendMode).
type MultiKey = (i64, i64, i64, i64, i64);

const ATTR_UV3N: bevy::mesh::MeshVertexAttribute = bevy::mesh::MeshVertexAttribute::new("FxMultiUv3Next", 0x5ec1_0604, bevy::render::render_resource::VertexFormat::Float32x4);
const ATTR_C1: bevy::mesh::MeshVertexAttribute = bevy::mesh::MeshVertexAttribute::new("FxMultiColor1", 0x5ec1_0641, bevy::render::render_resource::VertexFormat::Float32x4);
const ATTR_C2: bevy::mesh::MeshVertexAttribute = bevy::mesh::MeshVertexAttribute::new("FxMultiColor2", 0x5ec1_0642, bevy::render::render_resource::VertexFormat::Float32x4);
const ATTR_C3: bevy::mesh::MeshVertexAttribute = bevy::mesh::MeshVertexAttribute::new("FxMultiColor3", 0x5ec1_0643, bevy::render::render_resource::VertexFormat::Float32x4);

/// MultiTextureBillboardEx (604): its three layers blended as the game's
/// GXFfxtessellateBlendMultiTexture shader does (fx_multi.wgsl). Output premultiplied, so one
/// alpha mode serves normal and additive blends (all extracted 604s: blendMode 0 / 2 / 4 / 6 / 7).
#[derive(Asset, TypePath, bevy::render::render_resource::AsBindGroup, Clone)]
pub struct MultiMaterial {
    #[texture(0)]
    #[sampler(1)]
    t0: Handle<Image>,
    #[texture(2)]
    #[sampler(3)]
    t1: Handle<Image>,
    #[texture(4)]
    #[sampler(5)]
    t2: Handle<Image>,
    #[uniform(6)]
    modes: UVec4,
    #[uniform(7)]
    flags: UVec4,
}

impl Material for MultiMaterial {
    fn vertex_shader() -> bevy::shader::ShaderRef {
        "embedded://sv1/fx_multi.wgsl".into()
    }
    fn fragment_shader() -> bevy::shader::ShaderRef {
        "embedded://sv1/fx_multi.wgsl".into()
    }
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Premultiplied
    }
    fn enable_prepass() -> bool {
        false
    }
    fn enable_shadows() -> bool {
        false
    }
    fn specialize(
        _pipeline: &bevy::pbr::MaterialPipeline,
        descriptor: &mut bevy::render::render_resource::RenderPipelineDescriptor,
        layout: &bevy::mesh::MeshVertexBufferLayoutRef,
        _key: bevy::pbr::MaterialPipelineKey<Self>,
    ) -> Result<(), bevy::render::render_resource::SpecializedMeshPipelineError> {
        descriptor.vertex.buffers = vec![layout.0.get_layout(&[
            Mesh::ATTRIBUTE_POSITION.at_shader_location(0),
            Mesh::ATTRIBUTE_UV_0.at_shader_location(1),
            Mesh::ATTRIBUTE_UV_1.at_shader_location(2),
            ATTR_UV3N.at_shader_location(3),
            Mesh::ATTRIBUTE_COLOR.at_shader_location(4),
            ATTR_C1.at_shader_location(5),
            ATTR_C2.at_shader_location(6),
            ATTR_C3.at_shader_location(7),
        ])?];
        descriptor.primitive.cull_mode = None;
        Ok(())
    }
}

#[derive(Resource, Default)]
struct MultiBatches(HashMap<MultiKey, (Entity, Handle<Mesh>)>);

#[derive(Default)]
struct Buf {
    pos: Vec<[f32; 3]>,
    uv: Vec<[f32; 2]>,
    col: Vec<[f32; 4]>,
}

impl Buf {
    /// One model particle: its triangles placed by `tf`, UVs as authored.
    fn model(&mut self, m: &crate::model::FxModel, tf: Transform, col: [f32; 4]) {
        for &i in &m.indices {
            let i = i as usize;
            self.pos.push(tf.transform_point(m.pos[i]).to_array());
            self.uv.push(m.uv[i]);
            self.col.push(col);
        }
    }
    fn quad(&mut self, c: [Vec3; 4], uv: [[f32; 2]; 4], col: [f32; 4]) {
        for i in [0, 1, 2, 0, 2, 3] {
            self.pos.push(c[i].to_array());
            self.uv.push(uv[i]);
            self.col.push(col);
        }
    }
}

fn frame_uv(a: &Value, age: f32, rnd: &mut Rnd) -> [[f32; 2]; 4] {
    frame_uvs(a, age, rnd, [1.0, 1.0]).0
}

/// The flipbook frame's corners, the next frame's and the fraction between them (the frame index
/// before flooring), each corner's UV inside the frame scaled by `scale` (layer1ScaleU / V).
fn frame_uvs(a: &Value, age: f32, rnd: &mut Rnd, scale: [f32; 2]) -> ([[f32; 2]; 4], [[f32; 2]; 4], f32) {
    let cols = int(a, "columns", 1).max(1) as f32;
    let total = int(a, "totalFrames", 1).max(1) as f32;
    let rows = (total / cols).ceil().max(1.0);
    let fi = scalar(a, "frameIndex", age, rnd, 0.0) + scalar(a, "frameIndexOffset", age, rnd, 0.0);
    let rect = |f: f32| {
        let f = f.rem_euclid(total);
        let (cx, cy) = (f % cols, (f / cols).floor());
        [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]].map(|[u, v]: [f32; 2]| [(cx + u * scale[0]) / cols, (cy + v * scale[1]) / rows])
    };
    (rect(fi.floor()), rect(fi.floor() + 1.0), fi - fi.floor())
}

/// A 604 layer's UVs: the quad's 0..1 square x layerNScaleU / V + layerNOffsetU / V +
/// layerNSpeedU / V x age (the speeds are ParticleAge properties, taken at the current age).
fn layer_uv(a: &Value, n: u32, age: f32, rnd: &mut Rnd) -> [[f32; 2]; 4] {
    let su = scalar(a, &format!("layer{n}ScaleU"), age, rnd, 1.0);
    let sv = scalar(a, &format!("layer{n}ScaleV"), age, rnd, 1.0);
    let ou = scalar(a, &format!("layer{n}OffsetU"), 0.0, rnd, 0.0) + scalar(a, &format!("layer{n}SpeedU"), age, rnd, 0.0) * age;
    let ov = scalar(a, &format!("layer{n}OffsetV"), 0.0, rnd, 0.0) + scalar(a, &format!("layer{n}SpeedV"), age, rnd, 0.0) * age;
    [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]].map(|[u, v]: [f32; 2]| [u * su + ou, v * sv + ov])
}

/// Particle colour: color1 * color2 * color3 * the node's particleModifier colour, rgbMultiplier,
/// alphaMultiplier (HDR above 1 feeds the bloom).
/// Lit particles (lighting -2 / 0, LightingMode in reference/fxr) are drawn unlit here, at 1/8 of
/// their rgbMultiplier: where the game ships a lit config next to an unlit one for the same node
/// (other states, same texture), the unlit one's rgbMultiplier is the lit one's x 0.125 in all 48
/// such pairs of the extracted effects (the blood 220505: 15 -> 1.875, 37.5 -> 4.6875).
/// gap: the scene light / shadowDarkness / specular on lit particles.
fn particle_color(a: &Value, c: &Value, p: &Particle, active_t: f32, rnd: &mut Rnd) -> [f32; 4] {
    let c1 = prop(a.get("color1"), p.age, rnd, [1.0; 4]).map(|v| v.clamp(0.0, 1.0));
    let c2 = prop(a.get("color2"), p.emit_t, rnd, [1.0; 4]).map(|v| v.clamp(0.0, 1.0));
    let c3 = prop(a.get("color3"), p.age, rnd, [1.0; 4]);
    let pm = prop(c["particleModifier"].get("color"), active_t, rnd, [1.0; 4]);
    let lit = matches!(a["lighting"].as_i64(), Some(-2 | 0));
    let rgbm = scalar(a, "rgbMultiplier", active_t, rnd, 1.0) * if lit { 0.125 } else { 1.0 };
    let am = scalar(a, "alphaMultiplier", active_t, rnd, 1.0);
    let k: [f32; 4] = std::array::from_fn(|i| c1[i] * c2[i] * c3[i] * pm[i]);
    let h = hue_squared([k[0], k[1], k[2]]);
    [h[0] * rgbm, h[1] * rgbm, h[2] * rgbm, (k[3] * am).clamp(0.0, 1.0)]
}

/// The game's effect pixel shaders (shader/gxffxshader.shaderbnd GXFfxtexture.ppo /
/// GXFfxsoftTracer.ppo, disassembled with d3dcompiler_47 D3DDisassemble) split the particle colour
/// into its length and direction (`dp3` / `sqrt` / `div r3.xyz, v0.xyz, |v0|`), multiply the texture
/// by the direction, square that (`mul r1.xyz, r1.xyz, r1.xyz`) and scale by the length: the hue is
/// gamma 2, the brightness (rgbMultiplier included) linear. Here: c_i^2 / |c|, times sqrt 3 so a
/// grey keeps its value. gap: that sqrt 3 stands for g_vColorScale (cb0[9], set by the exe, not
/// traced); the texture's own square is left to the PNG's sRGB decode (~ x^2.2).
fn hue_squared(c: [f32; 3]) -> [f32; 3] {
    let len = (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt();
    if len < 1e-5 {
        return [0.0; 3];
    }
    c.map(|v| v * v / len * 3f32.sqrt())
}

/// Distortion / RadialBlur colour: `color` (clamped) x the node's particleModifier colour,
/// rgbMultiplier / alphaMultiplier (the shaders' g_color, no hue squaring there).
/// The appearance's camera-distance fade (reference/fxr src/actions/603.yml): hidden nearer than
/// minDistance, opacity rising linearly to 1 at minFadeDistance; the same toward maxDistance from
/// maxFadeDistance; hidden past the hard cut-offs minDistanceThreshold / maxDistanceThreshold. -1
/// turns each off. (The Ichimonji dust 440081: 2.5 / 5 m, so it clears off the camera.)
fn view_fade(a: &Value, d: f32) -> f32 {
    let f = |k: &str| a[k].as_f64().map(|v| v as f32).unwrap_or(-1.0);
    let (min_d, min_f, max_f, max_d) = (f("minDistance"), f("minFadeDistance"), f("maxFadeDistance"), f("maxDistance"));
    let (min_t, max_t) = (f("minDistanceThreshold"), f("maxDistanceThreshold"));
    if (min_t >= 0.0 && d < min_t) || (max_t >= 0.0 && d > max_t) {
        return 0.0;
    }
    let mut k = 1.0;
    if min_d >= 0.0 && min_f >= 0.0 {
        k *= if min_f > min_d { ((d - min_d) / (min_f - min_d)).clamp(0.0, 1.0) } else if d < min_d { 0.0 } else { 1.0 };
    }
    if max_d >= 0.0 && max_f >= 0.0 {
        k *= if max_d > max_f { ((max_d - d) / (max_d - max_f)).clamp(0.0, 1.0) } else if d > max_d { 0.0 } else { 1.0 };
    }
    k
}

fn screen_fx_color(a: &Value, c: &Value, p: &Particle, active_t: f32, rnd: &mut Rnd) -> [f32; 4] {
    let k = prop(a.get("color"), p.age, rnd, [1.0; 4]).map(|v| v.clamp(0.0, 1.0));
    let pm = prop(c["particleModifier"].get("color"), active_t, rnd, [1.0; 4]);
    let rgbm = scalar(a, "rgbMultiplier", active_t, rnd, 1.0);
    let am = scalar(a, "alphaMultiplier", active_t, rnd, 1.0);
    [k[0] * pm[0] * rgbm, k[1] * pm[1] * rgbm, k[2] * pm[2] * rgbm, (k[3] * pm[3] * am).clamp(0.0, 1.0)]
}

#[allow(clippy::too_many_arguments)]
fn collect(
    n: &NodeInst,
    lib: &mut FxrLib,
    cam: &GlobalTransform,
    out: &mut HashMap<(i64, i64), Buf>,
    dist: &mut HashMap<i64, DistortBuf>,
    multi: &mut HashMap<MultiKey, MultiBuf>,
    lights: &mut Vec<(Vec3, Color, f32, f32)>,
) {
    if let Some(c) = &n.cfg {
        let a = &c["appearance"];
        let at = a["type"].as_i64().unwrap_or(0);
        let active_t = (n.time - n.delay).max(0.0);
        let blend = int(a, "blendMode", 2);
        let model = if at == 605 { lib.model(int(a, "model", -1)) } else { None };
        let tex = match at {
            604 => int(a, "layer1", 1),
            _ => int(a, "texture", 1),
        };
        let (cam_r, cam_u, cam_f) = (cam.right().as_vec3(), cam.up().as_vec3(), cam.forward().as_vec3());
        // A pure distortion layer (distortionIntensity > 0 with rgbMultiplier 0: the weapon trail
        // 401000's second tracer, normalMap 6000) shows only as refraction: its colour quads are
        // not drawn (else a black smear), only its DistortBuf ones.
        let distortion_only = num(&a["distortionIntensity"]) > 0.0 && a["rgbMultiplier"].as_f64() == Some(0.0);
        for p in n.particles.iter() {
            let mut rnd = Rnd { seed: p.seed ^ 0x9999, k: 0 };
            let world = if p.attached { n.world.transform_point(p.pos) } else { p.pos };
            let fade = view_fade(a, world.distance(cam.translation()));
            if fade <= 0.0 {
                continue;
            }
            let mut col = particle_color(a, c, p, active_t, &mut rnd);
            col[3] *= fade;
            let pmod = &c["particleModifier"];
            let sx = scalar(pmod, "scaleX", p.emit_t, &mut rnd, 1.0);
            let sy = if pmod["uniformScale"].as_bool() == Some(true) { sx } else { scalar(pmod, "scaleY", p.emit_t, &mut rnd, 1.0) };
            match at {
                600 | 603 | 604 => {
                    let w = scalar(a, "width", p.age, &mut rnd, 1.0) * sx;
                    let h = if a["uniformScale"].as_bool() == Some(true) { w } else { scalar(a, "height", p.age, &mut rnd, 1.0) * sy };
                    let off = game_vec(prop(a.get("offsetX").map(|_| a), 0.0, &mut rnd, [0.0; 4]));
                    let _ = off;
                    let rz = (scalar(a, "rotationZ", 0.0, &mut rnd, 0.0) + scalar(a, "angularSpeedZ", p.age, &mut rnd, 0.0) * scalar(a, "angularSpeedMultiplierZ", p.age, &mut rnd, 1.0) * p.age).to_radians();
                    // Orientation (OrientationMode): 1 / 6 camera, 7 / 4 yaw toward the camera, 2 the node's
                    // -Z, 0 / 3 global south.
                    let (right, up) = match int(a, "orientation", 1) {
                        0 | 3 => (Vec3::X, Vec3::Y),
                        2 => (n.world.rotation * Vec3::X, n.world.rotation * Vec3::Y),
                        4 | 7 => {
                            let axis = if int(a, "orientation", 1) == 7 { n.world.rotation * Vec3::Y } else { Vec3::Y };
                            let to_cam = (cam.translation() - world).reject_from(axis).normalize_or(cam_f);
                            (axis.cross(to_cam).normalize_or(cam_r), axis)
                        }
                        _ => (cam_r, cam_u),
                    };
                    let (cr, sr) = (rz.cos(), rz.sin());
                    let (r2, u2) = (right * cr + up * sr, up * cr - right * sr);
                    let (hw, hh) = (r2 * w * 0.5, u2 * h * 0.5);
                    let corners = [world - hw - hh, world + hw - hh, world + hw + hh, world - hw + hh];
                    let l2 = int(a, "layer2", 0);
                    if at == 604 && l2 > 0 {
                        // The layers (fx_multi.wgsl); unk_ds3_f2_10..13 are the shader's
                        // g_ps_TexBlendType / 2 / 3 / ColorBlendType (same order in its cbuffer;
                        // f2_10's documented effect matches TexBlendType's alpha path).
                        let l3 = int(a, "layer3", 0).max(0);
                        let m = |k: &str| (int(a, k, 0) & 0xf) as i64;
                        let modes = m("unk_ds3_f2_10") | m("unk_ds3_f2_11") << 4 | m("unk_ds3_f2_12") << 8 | m("unk_ds3_f2_13") << 12 | ((a["premultiplyAlpha"].as_bool() == Some(true)) as i64) << 16;
                        let scale1 = [scalar(a, "layer1ScaleU", p.age, &mut rnd, 1.0), scalar(a, "layer1ScaleV", p.age, &mut rnd, 1.0)];
                        let (f_uv, f_next, f_t) = frame_uvs(a, p.age, &mut rnd, scale1);
                        let o1 = [
                            scalar(a, "layer1OffsetU", 0.0, &mut rnd, 0.0) + scalar(a, "layer1SpeedU", p.age, &mut rnd, 0.0) * p.age,
                            scalar(a, "layer1OffsetV", 0.0, &mut rnd, 0.0) + scalar(a, "layer1SpeedV", p.age, &mut rnd, 0.0) * p.age,
                        ];
                        let shift = |uv: [[f32; 2]; 4]| uv.map(|[u, v]| [u + o1[0], v + o1[1]]);
                        let lc = |k: &str, rnd: &mut Rnd| prop(a.get(k), p.age, rnd, [1.0; 4]).map(|v| v.clamp(0.0, 1.0));
                        let c1 = lc("layer1Color", &mut rnd);
                        let interp = a["interpolateFrames"].as_bool() != Some(false);
                        let q = MultiQuad {
                            corners,
                            uv1: shift(f_uv),
                            uv1_next: shift(f_next),
                            uv2: layer_uv(a, 2, p.age, &mut rnd),
                            uv3: layer_uv(a, 3, p.age, &mut rnd),
                            col,
                            c1: [c1[0], c1[1], c1[2], if interp { f_t } else { 0.0 }],
                            c2: lc("layer2Color", &mut rnd),
                            c3: lc("layer3Color", &mut rnd),
                        };
                        multi.entry((tex, l2, l3, modes, blend)).or_default().quad(&q);
                        continue;
                    }
                    let uv = frame_uv(a, p.age, &mut rnd);
                    out.entry((tex, blend)).or_default().quad(corners, uv, col);
                }
                // Model (605): sizeX/Y/Z scale the mesh, rotationX/Y/Z + angularSpeed* x
                // angularSpeedMultiplier* x age turn it (degrees, game axes), in the node's frame.
                // gap: blendMode 0 is drawn as normal blending here (the leaves are cut-out meshes),
                // not additive like the sprites; no lighting.
                605 => {
                    let Some(m) = model.as_deref() else { continue };
                    // uniformScale: sizeX on every axis (the shuriken 300080: 0.15 / 1 / 1); the
                    // particle modifier's scaleX / Y / Z the same way.
                    let sz = scalar(a, "sizeX", p.age, &mut rnd, 1.0);
                    let size = if a["uniformScale"].as_bool() == Some(true) {
                        Vec3::splat(sz)
                    } else {
                        Vec3::new(sz, scalar(a, "sizeY", p.age, &mut rnd, 1.0), scalar(a, "sizeZ", p.age, &mut rnd, 1.0))
                    };
                    let pm = if pmod["uniformScale"].as_bool() == Some(true) { Vec3::splat(sx) } else { Vec3::new(sx, sy, scalar(pmod, "scaleZ", p.emit_t, &mut rnd, 1.0)) };
                    let size = size * pm;
                    let mut ang = [0.0f32; 4];
                    for (i, ax) in ["X", "Y", "Z"].iter().enumerate() {
                        let speed = scalar(a, &format!("angularSpeed{ax}"), p.age, &mut rnd, 0.0) * scalar(a, &format!("angularSpeedMultiplier{ax}"), p.age, &mut rnd, 1.0);
                        ang[i] = scalar(a, &format!("rotation{ax}"), 0.0, &mut rnd, 0.0) + speed * p.age;
                    }
                    let rot = n.world.rotation * game_rot(ang);
                    let key = (MODEL_KEY - int(a, "model", 0), if blend == 0 { 2 } else { blend });
                    out.entry(key).or_default().model(m, Transform { translation: world, rotation: rot, scale: size }, col);
                }
                602 => {
                    let wl = scalar(a, "width", p.emit_t, &mut rnd, 1.0) * scalar(a, "widthMultiplier", p.age, &mut rnd, 1.0) * sx;
                    let len = scalar(a, "length", p.emit_t, &mut rnd, 1.0) * scalar(a, "lengthMultiplier", p.age, &mut rnd, 1.0) * sy;
                    let d = if p.attached { n.world.rotation * p.dir } else { p.dir };
                    let side = d.cross(cam_f).normalize_or(cam_r) * wl * 0.5;
                    let tail = world - d * len;
                    out.entry((-1, blend)).or_default().quad([tail - side, world - side, world + side, tail + side], [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]], col);
                }
                // Distortion (607): a camera-facing (orientation as the billboards) quad, sizeX / Y;
                // its normal map at UV x normalMapScale + normalMapOffset + normalMapSpeed x age,
                // intensity = g_waveStrength, 1 / radius = g_clampDistanceInverse (radius 1: the
                // ellipse inscribing the quad). RadialBlur (608): width / height, its mask, blurRadius =
                // g_radialDistance, toward the particle's centre on screen. gap: 607's mode (all
                // extracted ones: 1 NormalMap) and shape (1 Hemiellipsoid in one: drawn flat), 608's
                // iterations (all 1), 607's blendMode (drawn over, not added); 608's blendMode 0 / 4 / 7
                // add as the sprites' do.
                607 | 608 => {
                    let (sw, sh) = if at == 607 { ("sizeX", "sizeY") } else { ("width", "height") };
                    let w = scalar(a, sw, p.age, &mut rnd, 1.0) * sx;
                    let h = if a["uniformScale"].as_bool() == Some(true) { w } else { scalar(a, sh, p.age, &mut rnd, 1.0) * sy };
                    let (right, up) = match (at, int(a, "orientation", 1)) {
                        (607, 0 | 3) => (Vec3::X, Vec3::Y),
                        (607, 2) => (n.world.rotation * Vec3::X, n.world.rotation * Vec3::Y),
                        _ => (cam_r, cam_u),
                    };
                    let (hw, hh) = (right * w * 0.5, up * h * 0.5);
                    let corners = [world - hw - hh, world + hw - hh, world + hw + hh, world - hw + hh];
                    let uv = [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];
                    let mut col = screen_fx_color(a, c, p, active_t, &mut rnd);
                    col[3] *= fade;
                    if at == 607 {
                        let nm = int(a, "normalMap", 0);
                        if nm <= 0 {
                            continue;
                        }
                        let su = scalar(a, "normalMapScaleU", 0.0, &mut rnd, 1.0);
                        let sv = scalar(a, "normalMapScaleV", 0.0, &mut rnd, 1.0);
                        let ou = scalar(a, "normalMapOffsetU", 0.0, &mut rnd, 0.0) + scalar(a, "normalMapSpeedU", p.age, &mut rnd, 0.0) * p.age;
                        let ov = scalar(a, "normalMapOffsetV", 0.0, &mut rnd, 0.0) + scalar(a, "normalMapSpeedV", p.age, &mut rnd, 0.0) * p.age;
                        let tuv = uv.map(|[u, v]| [u * su + ou, v * sv + ov]);
                        let fx = [0.0, scalar(a, "intensity", p.age, &mut rnd, 1.0), 1.0 / scalar(a, "radius", p.age, &mut rnd, 1.0).max(1e-3), 0.0];
                        dist.entry(nm).or_default().quad(corners, uv, tuv, col, fx, world);
                    } else {
                        let mask = int(a, "mask", 0);
                        if mask <= 0 {
                            continue;
                        }
                        let add = if matches!(blend, 0 | 4 | 7) { 1.0 } else { 0.0 };
                        let fx = [1.0, scalar(a, "blurRadius", p.age, &mut rnd, 0.5), 0.0, add];
                        dist.entry(mask).or_default().quad(corners, uv, uv, col, fx, world);
                    }
                }
                606 | 10012 => {
                    let w = (scalar(a, "width", p.age, &mut rnd, 1.0) * scalar(a, "widthMultiplier", p.emit_t, &mut rnd, 1.0)).abs();
                    // distortionIntensity (ActiveTime) bends the frame through the trail's normalMap,
                    // falling off toward its edges (fx_distort.wgsl kind 2).
                    let bend = scalar(a, "distortionIntensity", active_t, &mut rnd, 0.0);
                    let nm = int(a, "normalMap", 0);
                    let cross = int(a, "orientation", 0) == 4;
                    if p.trail.len() < 2 {
                        continue;
                    }
                    let segs = subdivide(&p.trail, int(a, "segmentSubdivision", 0).clamp(0, 16) as usize);
                    let sf = scalar(a, "startFadeEndpoint", p.age, &mut rnd, 0.0).max(0.0) / 100.0;
                    let ef = scalar(a, "endFadeEndpoint", p.age, &mut rnd, 0.0).max(0.0) / 100.0;
                    let fade = |u: f32| -> f32 {
                        let mut k = 1.0;
                        if sf > 0.0 {
                            k *= (u / sf).min(1.0);
                        }
                        if ef > 0.0 {
                            k *= ((1.0 - u) / ef).min(1.0);
                        }
                        k
                    };
                    for w2 in segs.windows(2) {
                        let ((a0, x0, u0), (a1, x1, u1)) = (w2[0], w2[1]);
                        let d = (a1 - a0).normalize_or(cam_r);
                        let travel = d.cross(cam_f).normalize_or(cam_u);
                        let s0 = if x0 == Vec3::ZERO { travel } else { x0 } * w * 0.5;
                        let s1 = if x1 == Vec3::ZERO { travel } else { x1 } * w * 0.5;
                        let (k0, k1) = (fade(u0), fade(u1));
                        let c0 = [col[0], col[1], col[2], col[3] * k0.min(k1)];
                        let quv = [[u0, 1.0], [u1, 1.0], [u1, 0.0], [u0, 0.0]];
                        if bend > 0.0 && nm > 0 {
                            let a_fade = (scalar(a, "alphaMultiplier", active_t, &mut rnd, 1.0) * k0.min(k1)).clamp(0.0, 1.0);
                            dist.entry(nm).or_default().quad([a0 - s0, a1 - s1, a1 + s1, a0 + s0], quv, quv, [1.0, 1.0, 1.0, a_fade], [2.0, bend, 1.0, 0.0], a0);
                        }
                        if distortion_only {
                            continue;
                        }
                        let b = out.entry((tex, blend)).or_default();
                        b.quad([a0 - s0, a1 - s1, a1 + s1, a0 + s0], quv, c0);
                        if cross {
                            let (c0x, c1x) = (Vec3::X * w * 0.5, Vec3::X * w * 0.5);
                            b.quad([a0 - c0x, a1 - c1x, a1 + c1x, a0 + c0x], [[u0, 1.0], [u1, 1.0], [u1, 0.0], [u0, 0.0]], c0);
                        }
                    }
                }
                609 => {
                    let dc = prop(a.get("diffuseColor"), p.age, &mut rnd, [1.0; 4]);
                    let mult = scalar(a, "diffuseMultiplier", active_t, &mut rnd, 1.0);
                    let radius = scalar(a, "radius", active_t, &mut rnd, 10.0);
                    lights.push((world, Color::linear_rgb(dc[0], dc[1], dc[2]), mult * dc[3].max(0.0), radius));
                }
                _ => {}
            }
        }
    }
    for ch in &n.children {
        collect(ch, lib, cam, out, dist, multi, lights);
    }
}

/// segmentSubdivision (reference/fxr src/actions/10012.yml): each completed segment (all but the
/// leading one, which ends at the source) is split into `sub` + 1 pieces, so a fast swing laid a
/// few points a frame apart draws as an arc, not a fan of flat sheets. Returns the points with
/// their place along the trail (0 the oldest, 1 the source). gap: the game's curve kind
/// (Catmull-Rom through the points here).
fn subdivide(segs: &[(Vec3, Vec3, f32)], sub: usize) -> Vec<(Vec3, Vec3, f32)> {
    let n = segs.len();
    let total = (n - 1) as f32;
    let mut out = Vec::with_capacity(n * (sub + 1));
    for i in 0..n - 1 {
        out.push((segs[i].0, segs[i].1, i as f32 / total));
        if i + 2 >= n {
            continue;
        }
        let (p0, p1, p2, p3) = (segs[i.saturating_sub(1)].0, segs[i].0, segs[i + 1].0, segs[i + 2].0);
        for k in 1..=sub {
            let t = k as f32 / (sub + 1) as f32;
            let (t2, t3) = (t * t, t * t * t);
            let p = 0.5 * (2.0 * p1 + (p2 - p0) * t + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2 + (3.0 * p1 - p0 - 3.0 * p2 + p3) * t3);
            let axis = segs[i].1.lerp(segs[i + 1].1, t).normalize_or_zero();
            out.push((p, axis, (i as f32 + t) / total));
        }
    }
    out.push((segs[n - 1].0, segs[n - 1].1, 1.0));
    out
}

/// Tracer segments: a new point every segmentInterval, each kept segmentDuration.
fn grow_trails(n: &mut NodeInst, dt: f32) {
    if let Some(c) = n.cfg.clone() {
        let a = &c["appearance"];
        if matches!(a["type"].as_i64(), Some(606) | Some(10012)) {
            let interval = num(&a["segmentInterval"]).max(1.0 / 60.0);
            let seg_life = num(&a["segmentDuration"]).max(0.02);
            let max_segs = a["concurrentSegments"].as_i64().unwrap_or(100).max(2) as usize;
            // TracerOrientationMode: 0 Travel (by the path and the camera, at draw time), 1 the node's
            // local Z, 2 global vertical, 3 global X, 4 cross (vertical here, X added at draw),
            // 5 the global diagonal.
            let axis = match int(a, "orientation", 0) {
                1 => n.world.rotation * Vec3::Z,
                2 | 4 => Vec3::Y,
                3 => Vec3::X,
                5 => Vec3::new(-1.0, 1.0, 1.0).normalize(),
                _ => Vec3::ZERO,
            };
            for p in &mut n.particles {
                let world = if p.attached { n.world.transform_point(p.pos) } else { p.pos };
                if p.ended {
                    let age = p.age;
                    p.trail.retain(|(_, _, t)| age - t <= seg_life);
                    continue;
                }
                // Points are laid every segmentInterval and stay where they were laid; the head (the
                // last point) follows the particle until the next one is laid. The first frame lays
                // the start and the head, so even a 2-frame swing leaves a segment.
                if p.trail.is_empty() {
                    p.trail.push((world, axis, p.age));
                    p.trail.push((world, axis, p.age));
                    p.next_seg = p.age + interval;
                } else if p.age >= p.next_seg {
                    p.trail.push((world, axis, p.age));
                    p.next_seg = p.age + interval;
                } else if let Some(last) = p.trail.last_mut() {
                    last.0 = world;
                    last.1 = axis;
                }
                let age = p.age;
                p.trail.retain(|(_, _, t)| age - t <= seg_life);
                if p.trail.len() > max_segs {
                    let k = p.trail.len() - max_segs;
                    p.trail.drain(0..k);
                }
            }
        }
    }
    for ch in &mut n.children {
        grow_trails(ch, dt);
    }
}

#[derive(Component)]
struct FxLight;

#[allow(clippy::too_many_arguments)]
fn draw(
    mut commands: Commands,
    time: Res<Time>,
    assets: Res<AssetServer>,
    mut batches: ResMut<Batches>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut dist_materials: ResMut<Assets<DistortMaterial>>,
    mut dist_batches: ResMut<DistortBatches>,
    mut multi_materials: ResMut<Assets<MultiMaterial>>,
    mut multi_batches: ResMut<MultiBatches>,
    camera: Query<&GlobalTransform, With<Camera3d>>,
    mut fx: Query<&mut FxEffect>,
    old_lights: Query<Entity, With<FxLight>>,
    mut lib: ResMut<FxrLib>,
) {
    let Some(cam) = camera.iter().next() else { return };
    let mut bufs: HashMap<(i64, i64), Buf> = HashMap::new();
    let mut dist: HashMap<i64, DistortBuf> = HashMap::new();
    let mut multi: HashMap<MultiKey, MultiBuf> = HashMap::new();
    let mut lights = Vec::new();
    let dt = time.delta_secs();
    for mut f in &mut fx {
        if let Some(root) = f.root.as_mut() {
            grow_trails(root, dt);
            collect(root, &mut lib, cam, &mut bufs, &mut dist, &mut multi, &mut lights);
        }
    }
    if std::env::var("SHINOBI_FXR_LOG").is_ok() && !(bufs.is_empty() && dist.is_empty()) {
        let mut v: Vec<String> = bufs.iter().map(|(k, b)| format!("{}/{}:{}", k.0, k.1, b.pos.len() / 6)).collect();
        v.extend(dist.iter().map(|(k, b)| format!("screen {k}:{}", b.pos.len() / 6)));
        v.sort();
        info!("fxr quads {}", v.join(" "));
    }
    // Upload: one mesh per (texture, blend mode).
    for (key, buf) in bufs.iter() {
        if !batches.map.contains_key(key) {
            let (tex, blend) = *key;
            let image = if tex <= MODEL_KEY {
                // A Model appearance: its mesh's albedo (extracted/tex, written by the model export).
                lib.model(MODEL_KEY - tex).filter(|m| !m.albedo.is_empty()).map(|m| assets.load(format!("tex/{}.dds", m.albedo)))
            } else {
                (tex > 0).then(|| assets.load(format!("fxr_tex/{tex}.png")))
            };
            // BlendMode: 0 / 4 / 7 additive, 1 opaque, 3 multiply, 2 / 6 normal (5 subtract: normal).
            let alpha_mode = match blend {
                0 | 4 | 7 => AlphaMode::Add,
                1 => AlphaMode::Opaque,
                3 => AlphaMode::Multiply,
                _ => AlphaMode::Blend,
            };
            let mat = materials.add(StandardMaterial { base_color: Color::WHITE, base_color_texture: image, unlit: true, alpha_mode, cull_mode: None, double_sided: true, ..default() });
            let mesh = meshes.add(Mesh::new(bevy::mesh::PrimitiveTopology::TriangleList, bevy::asset::RenderAssetUsages::default()));
            let e = commands.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(mat), Transform::IDENTITY, bevy::light::NotShadowCaster, bevy::camera::visibility::NoFrustumCulling)).id();
            batches.map.insert(*key, (e, mesh));
        }
        let _ = buf;
    }
    // Screen effects: one mesh per texture. Normal maps and masks are data (no sRGB decode),
    // sampled wrapping (the game's g_WrapLinearSampler; the normal maps scroll).
    // Key 0: a warm-up batch (no texture, no quads) so the material's pipeline is compiled at
    // start, not when the first screen effect plays.
    let wanted: Vec<i64> = dist.keys().copied().chain(lib.screen_tex.iter().copied()).chain([0]).collect();
    for tex in &wanted {
        if dist_batches.0.contains_key(tex) {
            continue;
        }
        let image = if *tex == 0 { Handle::default() } else { assets.load_builder().with_settings(|s: &mut bevy::image::ImageLoaderSettings| {
            s.is_srgb = false;
            s.sampler = bevy::image::ImageSampler::Descriptor(bevy::image::ImageSamplerDescriptor {
                address_mode_u: bevy::image::ImageAddressMode::Repeat,
                address_mode_v: bevy::image::ImageAddressMode::Repeat,
                ..bevy::image::ImageSamplerDescriptor::linear()
            });
        }).load(format!("fxr_tex/{tex}.png")) };
        let mat = dist_materials.add(DistortMaterial { tex: image });
        let mesh = meshes.add(Mesh::new(bevy::mesh::PrimitiveTopology::TriangleList, bevy::asset::RenderAssetUsages::default()));
        let e = commands.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(mat), Transform::IDENTITY, bevy::light::NotShadowCaster, bevy::camera::visibility::NoFrustumCulling)).id();
        dist_batches.0.insert(*tex, (e, mesh));
    }
    for (tex, (_, mesh)) in dist_batches.0.iter() {
        let Some(mut m) = meshes.get_mut(mesh) else { continue };
        let empty = DistortBuf::default();
        let b = dist.get(tex).unwrap_or(&empty);
        // Never empty: a degenerate triangle far below.
        let pad = |v: &Vec<[f32; 3]>, x: [f32; 3]| [vec![x; 3], v.clone()].concat();
        m.insert_attribute(Mesh::ATTRIBUTE_POSITION, pad(&b.pos, [0.0, -50.0, 0.0]));
        m.insert_attribute(Mesh::ATTRIBUTE_UV_0, [vec![[0.0; 2]; 3], b.uv.clone()].concat());
        m.insert_attribute(Mesh::ATTRIBUTE_UV_1, [vec![[0.0; 2]; 3], b.tex_uv.clone()].concat());
        m.insert_attribute(Mesh::ATTRIBUTE_COLOR, [vec![[0.0; 4]; 3], b.col.clone()].concat());
        m.insert_attribute(ATTR_FX, [vec![[0.0; 4]; 3], b.fx.clone()].concat());
        m.insert_attribute(ATTR_CENTER, pad(&b.center, [0.0, -50.0, 0.0]));
    }
    // Multi-texture quads: one mesh per key; a missing layer 3 binds layer 2 (flags.z off).
    // Key (0, ..): a warm-up batch, so the pipeline is compiled at start (as the screen effects').
    let wanted: Vec<MultiKey> = multi.keys().copied().chain([(0, 0, 0, 0, 2)]).collect();
    for key in &wanted {
        if multi_batches.0.contains_key(key) {
            continue;
        }
        let &(l1, l2, l3, modes, blend) = key;
        let tex = |id: i64| if id > 0 { assets.load(format!("fxr_tex/{id}.png")) } else { Handle::default() };
        let mat = multi_materials.add(MultiMaterial {
            t0: tex(l1),
            t1: tex(l2),
            t2: tex(if l3 > 0 { l3 } else { l2 }),
            modes: UVec4::new((modes & 0xf) as u32, (modes >> 4 & 0xf) as u32, (modes >> 8 & 0xf) as u32, (modes >> 12 & 0xf) as u32),
            flags: UVec4::new((modes >> 16 & 1) as u32, matches!(blend, 0 | 4 | 7) as u32, (l3 > 0) as u32, 0),
        });
        let mesh = meshes.add(Mesh::new(bevy::mesh::PrimitiveTopology::TriangleList, bevy::asset::RenderAssetUsages::default()));
        let e = commands.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(mat), Transform::IDENTITY, bevy::light::NotShadowCaster, bevy::camera::visibility::NoFrustumCulling)).id();
        multi_batches.0.insert(*key, (e, mesh));
    }
    for (key, (_, mesh)) in multi_batches.0.iter() {
        let Some(mut m) = meshes.get_mut(mesh) else { continue };
        let empty = MultiBuf::default();
        let b = multi.get(key).unwrap_or(&empty);
        // Never empty: a degenerate triangle far below.
        m.insert_attribute(Mesh::ATTRIBUTE_POSITION, [vec![[0.0, -50.0, 0.0]; 3], b.pos.clone()].concat());
        m.insert_attribute(Mesh::ATTRIBUTE_UV_0, [vec![[0.0; 2]; 3], b.uv1.clone()].concat());
        m.insert_attribute(Mesh::ATTRIBUTE_UV_1, [vec![[0.0; 2]; 3], b.uv2.clone()].concat());
        m.insert_attribute(ATTR_UV3N, [vec![[0.0; 4]; 3], b.uv3n.clone()].concat());
        m.insert_attribute(Mesh::ATTRIBUTE_COLOR, [vec![[0.0; 4]; 3], b.col.clone()].concat());
        m.insert_attribute(ATTR_C1, [vec![[0.0; 4]; 3], b.c1.clone()].concat());
        m.insert_attribute(ATTR_C2, [vec![[0.0; 4]; 3], b.c2.clone()].concat());
        m.insert_attribute(ATTR_C3, [vec![[0.0; 4]; 3], b.c3.clone()].concat());
    }
    for (key, (_, mesh)) in batches.map.iter() {
        let Some(mut m) = meshes.get_mut(mesh) else { continue };
        let empty = Buf::default();
        let buf = bufs.get(key).unwrap_or(&empty);
        // Never empty (a degenerate triangle far below), see ffx.rs.
        let (mut pos, mut uv, mut col) = (vec![[0.0, -50.0, 0.0]; 3], vec![[0.0, 0.0]; 3], vec![[0.0; 4]; 3]);
        pos.extend_from_slice(&buf.pos);
        uv.extend_from_slice(&buf.uv);
        col.extend_from_slice(&buf.col);
        let n = pos.len();
        m.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
        m.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
        m.insert_attribute(Mesh::ATTRIBUTE_COLOR, col);
        m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; n]);
    }
    // Lights: rebuilt each frame, the brightest 8. gap: the game's light units are not traced;
    // diffuseMultiplier x 4000 lm reads like its glow.
    for e in &old_lights {
        commands.entity(e).despawn();
    }
    lights.sort_by(|a, b| b.2.total_cmp(&a.2));
    for (p, c, k, r) in lights.into_iter().take(8) {
        commands.spawn((PointLight { color: c, intensity: k * 4000.0, range: r.max(0.5), shadow_maps_enabled: false, ..default() }, Transform::from_translation(p), FxLight));
    }
}

pub struct FxrPlugin;

impl Plugin for FxrPlugin {
    fn build(&self, app: &mut App) {
        bevy::asset::embedded_asset!(app, "fx_distort.wgsl");
        bevy::asset::embedded_asset!(app, "fx_multi.wgsl");
        app.add_plugins((MaterialPlugin::<DistortMaterial>::default(), MaterialPlugin::<MultiMaterial>::default()));
        app.init_resource::<FxrLib>()
            .init_resource::<Batches>()
            .init_resource::<DistortBatches>()
            .init_resource::<MultiBatches>()
            .add_systems(Update, (tick, draw).chain());
        if std::env::var("SHINOBI_FXR_TEST").is_ok() {
            app.add_systems(Update, test_shots);
        }
    }
}

/// SHINOBI_FXR_TEST=<id>: plays that effect 1.5 m in front of Wolf at chest height, its +Z to
/// Wolf's right, 3 s in; SHINOBI_FXR_SHOTS=<dir> screenshots it (SHINOBI_FXR_SHOT_RANGE="from,to,step"
/// frames after the spawn, default 2,60,6), then exits. For checking one effect on its own.
fn test_shots(
    mut commands: Commands,
    mut frame: Local<u32>,
    mut exit: MessageWriter<AppExit>,
    mut cam: Query<&mut crate::camera::OrbitCamera>,
    wolf: Query<&GlobalTransform, With<crate::player::Player>>,
) {
    let Some(id) = std::env::var("SHINOBI_FXR_TEST").ok().and_then(|v| v.parse::<i64>().ok()) else { return };
    *frame += 1;
    let f = *frame;
    if let Ok(mut oc) = cam.single_mut() {
        oc.pitch = 5f32.to_radians();
    }
    let start = 180;
    if f == start {
        let Ok(w) = wolf.single() else { return };
        let fwd = (w.rotation() * Vec3::Z).with_y(0.0).normalize_or(Vec3::Z);
        let right = fwd.cross(Vec3::Y);
        let at = Transform::from_translation(w.translation() + fwd * 1.5 + Vec3::Y * 1.3).looking_to(-right, Vec3::Y);
        info!("fxr test {id} at {:.2}", at.translation);
        commands.spawn(FxEffect::new(id, None, false, at));
    }
    let Ok(dir) = std::env::var("SHINOBI_FXR_SHOTS") else { return };
    let r: Vec<u32> = std::env::var("SHINOBI_FXR_SHOT_RANGE").unwrap_or("2,60,6".into()).split(',').filter_map(|v| v.parse().ok()).collect();
    let (from, to, step) = if r.len() == 3 { (r[0], r[1], r[2].max(1)) } else { (2, 60, 6) };
    if f >= start + from && f <= start + to && (f - start - from) % step == 0 {
        let path = format!("{dir}/x{:03}.png", f - start);
        commands.spawn(bevy::render::view::screenshot::Screenshot::primary_window()).observe(bevy::render::view::screenshot::save_to_disk(path));
    }
    if f > start + to + 10 {
        exit.write(AppExit::Success);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn view_fade_follows_the_distance_fields() {
        // The Ichimonji dust 440081: minDistance 2.5, minFadeDistance 5, threshold 2.5.
        let a = serde_json::json!({"minDistance": 2.5, "minFadeDistance": 5.0, "minDistanceThreshold": 2.5, "maxFadeDistance": -1, "maxDistance": -1});
        assert_eq!(view_fade(&a, 1.0), 0.0);
        assert!((view_fade(&a, 3.75) - 0.5).abs() < 1e-5);
        assert_eq!(view_fade(&a, 9.0), 1.0);
        let far = serde_json::json!({"maxFadeDistance": 10.0, "maxDistance": 20.0});
        assert!((view_fade(&far, 15.0) - 0.5).abs() < 1e-5);
        assert_eq!(view_fade(&far, 25.0), 0.0);
        assert_eq!(view_fade(&serde_json::json!({}), 0.1), 1.0);
    }

    #[test]
    fn tracer_subdivision_keeps_the_leading_segment_simple() {
        let pts: Vec<(Vec3, Vec3, f32)> = (0..4).map(|i| (Vec3::new(i as f32, 0.0, 0.0), Vec3::Y, 0.0)).collect();
        let s = subdivide(&pts, 5);
        // 3 segments: the 2 completed ones split into 6 pieces, the leading one kept: 6 + 6 + 1 + 1 points.
        assert_eq!(s.len(), 14);
        assert_eq!(s[0].2, 0.0);
        assert_eq!(s[13].2, 1.0);
        assert!(s.windows(2).all(|w| w[1].0.x > w[0].0.x && w[1].2 > w[0].2));
        assert_eq!(subdivide(&pts, 0).len(), 4);
    }
}
