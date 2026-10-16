//! FXR player: the game's own effects (sfx/sfxbnd_commoneffects.ffxbnd f<id>.fxr, converted to
//! extracted/fxr/<id>.json by tools/fxr_extract.py with @cccode/fxr; textures extracted/fxr_tex/<id>.png
//! from the archive's s<id>.tpf). Semantics follow the fxr library's action definitions
//! (reference/fxr src/actions/*.yml, public domain): the node tree (2000 root, 2200 basic nodes,
//! 2001 references, 2002 levels of detail, 2202 node emitters), configs (1004 particles, 1005 child nodes), emitters
//! (399 once, 300 periodic, 301 by distance), emitter shapes (400-405), spread (500-503), particle
//! movement (55/60/84/105/64/65), properties over Constant0 / ParticleAge / EmissionTime / ActiveTime
//! with Linear / Stepped / Hermite / Bezier keyframes and RandomDelta / RandomFraction modifiers.
//! Drawn: BillboardEx (603), MultiTextureBillboardEx (604: all three layers, `MultiMaterial`,
//! fx_multi.wgsl), PointSprite (600),
//! Line (601, one pixel wide), QuadLine (602), Tracer / LegacyTracer (10012 / 606), PointLight (609), Model (605: the
//! s<model>.flver meshes exported to extracted/fxr_model), and the screen effects Distortion (607),
//! RadialBlur (608) and the tracers' distortionIntensity (`DistortMaterial`, fx_distort.wgsl).
//! Soft particles: `soften` + `FxDepth`. WindForce (10300) is not applied: unk_ds3_f1_31 = 1 (plants
//! only) in all but 400 / 401, whose particle force speed (732) is ~0.1.
//! Hermite keyframes blend as the exe does (`game_hermite`); Bezier keyframes are the exe's
//! Hermite basis with p1 / p2 as the tangents (FUN_141d94300 -> FUN_140433c00), the same curve.
//! gap: normal maps, lighting / specular, node random turns. Sounds: nodeAudio 75 (`start_audio`).

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

/// A scalar's largest value: the constant, or a curve's largest keyframe.
fn peak(v: &Value) -> f32 {
    match v["keyframes"].as_array() {
        Some(k) => k.iter().map(|f| num(&f["value"])).fold(0.0, f32::max),
        None => num(v),
    }
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

/// The game's Hermite keyframe blend (FXR property function 6; the evaluators registered for
/// ids 0x60-0x63 / 0x1060-0x1063 at 1400f FUN_141d98ff0, float body FUN_141d94960, vec4
/// FUN_141d92a50): `x` is the 0..1 position between the two keyframes, `t1` / `t2` the earlier
/// keyframe's two angles (radians). Two fixed Hermite curves h(0, m0, 1, m1, x)
/// (FUN_141d9bfe0) bent into circle arcs, an ease-in `lo` and an ease-out `hi`; each angle blends
/// from one of them (angle 0) through the straight line (pi/4) to the other (pi/2), and the two
/// results are blended over x.
fn game_hermite(t1: f32, t2: f32, x: f32) -> f32 {
    const Q: f32 = std::f32::consts::FRAC_PI_4;
    let h = |m0: f32, m1: f32| {
        let (x2, x3) = (x * x, x * x * x);
        (x3 - 2.0 * x2 + x) * m0 + (-2.0 * x3 + 3.0 * x2) + (x3 - x2) * m1
    };
    let lo = 1.0 - (1.0 - h(0.7, 1.5).powi(2)).max(0.0).sqrt();
    let hi = (1.0 - (h(1.5, 0.7) - 1.0).powi(2)).max(0.0).sqrt();
    let s = if t1 <= Q { lo + t1 / Q * (x - lo) } else { x + (t1 - Q) / Q * (hi - x) };
    let e = if t2 <= Q { hi + t2 / Q * (x - hi) } else { x + (t2 - Q) / Q * (lo - x) };
    s + x * (e - s)
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
                                let k: [f32; 4] = std::array::from_fn(|c| game_hermite(t1[c], t2[c], x));
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

pub(crate) fn game_rot(r: [f32; 4]) -> Quat {
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
    /// ParticleAccelerationRandomTurns (84): the particle age of its next turn.
    next_turn: f32,
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
    /// NodeTranslation (15): the offset applied so far (its `translation` by active time).
    offset: Vec3,
    /// Its nodeAudio (75) has been started.
    audio: bool,
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
    /// A keyed effect's event length (s): it stops when its own age passes this, not when the
    /// anim's clock passes the event end. A frame hitch runs several fixed ticks at once and
    /// could step a short event (Ichimonji's arc 440080, 0.13 s) from start to end before the
    /// effect ticked once: it started stopped and drew nothing.
    pub window: f32,
    /// A placed effect that plays until stopped (the boss fog wall, fog_wall.rs): no age limit.
    pub keep: bool,
    /// The spawner already played the root's own sound `s<id>` (sound.rs, TAE 96 / 118), so its
    /// nodeAudio of the same id is not played again.
    pub own_sound_played: bool,
    root: Option<NodeInst>,
    age: f32,
    /// The nodeAudio sounds playing, moved with their node and stopped with the effect.
    sounds: Vec<crate::fmod::EventHandle>,
}

impl FxEffect {
    /// Seconds since it started.
    pub fn age(&self) -> f32 {
        self.age
    }

    pub fn new(id: i64, anchor: Option<Anchor>, follow: bool, at: Transform) -> Self {
        FxEffect { id, anchor, follow, at, stop: false, key: None, window: f32::INFINITY, keep: false, own_sound_played: false, root: None, age: 0.0, sounds: Vec::new() }
    }
}

#[derive(Resource, Default)]
pub struct FxrLib {
    cache: HashMap<i64, Option<Arc<Value>>>,
    models: HashMap<i64, Option<Arc<crate::model::FxModel>>>,
    /// Normal maps / masks of the screen effects in the effects read so far: their batches (and
    /// textures) are set up at once, so a short effect is not over before its texture has loaded.
    screen_tex: Vec<i64>,
    /// The game camera this frame: the parent of NodeAttachToCamera (46) nodes.
    cam: Option<Transform>,
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
    // Levels of detail (LevelsOfDetail 2002): the closest (first) child only. (2202 is the
    // NodeEmitter, reference/fxr src/fxr.ts NodeType: all its children are templates.)
    let kids: Vec<Value> = if ty == 2002 { kids.into_iter().take(1).collect() } else { kids };
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
        offset: Vec3::ZERO,
        audio: false,
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
        // RectangleEmitterShape: sizeX by sizeY in the node's XY plane, as the disk (401).
        // gap: `distribution` (taken as uniform).
        402 => {
            let (x, y) = (s("sizeX", 1.0, rnd), s("sizeY", 1.0, rnd));
            (Vec3::new((rnd.next() - 0.5) * x, (rnd.next() - 0.5) * y, 0.0), Vec3::Z)
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

/// GPUStandardParticle (10000, reference/fxr src/actions/10000.yml, enums/EmitterShape.yml): the
/// appearance carries its own emitter. A point of its emitterShape (0 / 3 line along Z, 1 box with
/// an inner box of emitterDistribution blocked, 2 box weighted to the centre below distribution 1,
/// 4 cylinder about Y with the inner fraction blocked) inside emitterSize (taken as the full
/// extents: Demon of Hatred's 670240 cylinder 10 m across matches the clip's burst, 1:22:01 of
/// sekiro_all_bosses), turned by emitterRotation, plus particleOffset (+ a random Min..Max); its
/// velocity particleSpeed + a random speedMin..speedMax, with unk_ds3_f1_5 = 1 ("limits the
/// direction to one of the box's axes") turned so its Z is the shape's outward axis. Both in the
/// node's frame. gap: Line's direction, Unk3 / Unk5 (at the node).
fn gpu_emit(a: &Value, rnd: &mut Rnd) -> (Vec3, Vec3) {
    let size = vec4(&a["emitterSize"]);
    let dist = num(&a["emitterDistribution"]).clamp(0.0, 1.0);
    let shape = int(a, "emitterShape", 1);
    let mut u = || rnd.next() * 2.0 - 1.0;
    let (p, out) = match shape {
        0 | 3 => {
            let t = dist + (1.0 - dist) * (u() * 0.5 + 0.5);
            (Vec3::new(0.0, 0.0, t * size[2]), Vec3::Z)
        }
        1 | 2 => {
            let mut v = [u(), u(), u()];
            let ax = ((u() * 0.5 + 0.5) * 3.0) as usize % 3;
            if shape == 1 {
                let side = if u() < 0.0 { -1.0 } else { 1.0 };
                v[ax] = side * (dist + (1.0 - dist) * (u() * 0.5 + 0.5));
            } else {
                let k = dist + (1.0 - dist) * (u() * 0.5 + 0.5);
                v = v.map(|x| x * k);
            }
            let big = (0..3).max_by(|&i, &j| v[i].abs().total_cmp(&v[j].abs())).unwrap_or(2);
            let mut o = [0.0; 3];
            o[big] = v[big].signum();
            (Vec3::new(v[0] * size[0], v[1] * size[1], v[2] * size[2]) * 0.5, Vec3::from(o))
        }
        4 => {
            let r = (dist * dist + (1.0 - dist * dist) * (u() * 0.5 + 0.5)).sqrt();
            let ang = (u() * 0.5 + 0.5) * std::f32::consts::TAU;
            let radial = Vec3::new(ang.cos(), 0.0, ang.sin());
            (Vec3::new(radial.x * r * size[0], u() * size[1], radial.z * r * size[2]) * 0.5, radial)
        }
        _ => (Vec3::ZERO, Vec3::Z),
    };
    let mut r3 = |k: &str| {
        let (lo, hi, base) = (vec4(&a[format!("{k}Min")]), vec4(&a[format!("{k}Max")]), vec4(&a[k]));
        Vec3::from_array(std::array::from_fn(|i| base[i] + lo[i] + (hi[i] - lo[i]) * (u() * 0.5 + 0.5)))
    };
    let offset = r3("particleOffset");
    let mut v = r3("particleSpeed");
    if int(a, "unk_ds3_f1_5", 0) == 1 {
        v = Quat::from_rotation_arc(Vec3::Z, out) * v;
    }
    let rot = game_rot(vec4(&a["emitterRotation"]));
    let g = |v: Vec3| game_vec([v.x, v.y, v.z, 0.0]);
    (rot * g(p + offset), rot * g(v))
}

/// GPUStandardParticle's own emissions this frame (emissionIntervalMin..Max, 0 = every 1/30 s
/// frame as the field is in frames; emissionParticleCount + a random Min..Max each;
/// emissionCountLimit when limitEmissionCount).
fn gpu_emissions(a: &Value, n: &mut NodeInst, active_t: f32) -> i64 {
    let limit = (a["limitEmissionCount"].as_bool() == Some(true)).then(|| int(a, "emissionCountLimit", 0));
    let mut count = 0;
    let mut guard = 0;
    while n.emit_next <= active_t && guard < 8 && limit.is_none_or(|l| n.emissions < l) {
        let mut r = Rnd { seed: n.seed ^ (n.emissions as u32).wrapping_mul(0x2545_f491), k: 0 };
        let (lo, hi) = (int(a, "emissionParticleCountMin", 0), int(a, "emissionParticleCountMax", 0));
        count += int(a, "emissionParticleCount", 10) + lo + ((hi - lo) as f32 * r.next()).round() as i64;
        let (imin, imax) = (num(&a["emissionIntervalMin"]), num(&a["emissionIntervalMax"]));
        n.emit_next += (imin + (imax - imin) * r.next()).max(1.0 / 30.0);
        n.emissions += 1;
        guard += 1;
    }
    count
}

fn step_node(n: &mut NodeInst, lib: &mut FxrLib, parent: Transform, dt: f32, stop: bool) {
    // NodeAttachToCamera (46, reference/fxr src/actions/46.yml): the node rides on the camera,
    // with its rotation unless followRotation is off (Butterfly 650935, Monk 839653).
    let mv = n.cfg.as_ref().map(|c| &c["nodeMovement"]);
    let parent = match (mv.filter(|m| m["type"].as_i64() == Some(46)), lib.cam) {
        (Some(m), Some(cam)) if m["followRotation"].as_bool() == Some(false) => Transform::from_translation(cam.translation),
        (Some(_), Some(cam)) => Transform { scale: Vec3::ONE, ..cam },
        _ => parent,
    };
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
    // Node movement: speed / acceleration along its Z (1 / 106 / 122; NodeSpeed 120 with its
    // speedMultiplierZ; NodeAccelerationRandomTurns 83 with turns), spin (34), an offset by time
    // (NodeTranslation 15). Field meanings: reference/fxr src/fxr.ts.
    if let Some(c) = n.cfg.clone() {
        let mv = &c["nodeMovement"];
        let mut rnd = Rnd { seed: n.seed ^ 0xabc, k: 0 };
        match mv["type"].as_i64() {
            Some(1) | Some(106) | Some(122) | Some(83) | Some(120) | Some(113) => {
                let ty = mv["type"].as_i64();
                let v = if ty == Some(120) {
                    scalar(mv, "speedZ", 0.0, &mut rnd, 0.0) * scalar(mv, "speedMultiplierZ", active_t, &mut rnd, 1.0)
                } else {
                    scalar(mv, "speedZ", 0.0, &mut rnd, 0.0) + scalar(mv, "accelerationZ", active_t, &mut rnd, 0.0) * scalar(mv, "accelerationMultiplierZ", active_t, &mut rnd, 1.0) * active_t
                };
                // 83: a random turn of up to maxTurnAngle every turnInterval s.
                // gap: the turn's axes and distribution (taken as yaw and pitch, uniform).
                let every = scalar(mv, "turnInterval", active_t, &mut rnd, 0.0);
                if ty == Some(83) && every > 0.0 && ((active_t / every).floor() > ((active_t - dt) / every).floor()) {
                    let max = scalar(mv, "maxTurnAngle", active_t, &mut rnd, 0.0).to_radians();
                    let mut tr = Rnd { seed: n.seed ^ ((active_t / every) as u32).wrapping_mul(0x9e37), k: 0 };
                    let (yaw, pitch) = ((tr.next() * 2.0 - 1.0) * max, (tr.next() * 2.0 - 1.0) * max);
                    n.local.rotation = n.local.rotation * Quat::from_euler(EulerRot::YXZ, yaw, pitch, 0.0);
                }
                // 113 NodeAccelerationSpin: NodeAcceleration plus a spin as 34's, with multipliers
                // (Genichiro / Tomoe 671004, 650923, 650603).
                if ty == Some(113) {
                    let s = |k: &str, rnd: &mut Rnd| scalar(mv, &format!("angularSpeed{k}"), active_t, rnd, 0.0).to_radians() * scalar(mv, &format!("angularSpeedMultiplier{k}"), active_t, rnd, 1.0) * dt;
                    let (x, y, z) = (s("X", &mut rnd), -s("Y", &mut rnd), -s("Z", &mut rnd));
                    n.local.rotation = n.local.rotation * Quat::from_euler(EulerRot::YXZ, y, x, z);
                }
                let fwd = n.local.rotation * Vec3::Z;
                n.local.translation += fwd * v * dt;
                n.local.translation.y -= scalar(mv, "accelerationY", active_t, &mut rnd, 0.0) * active_t * dt;
            }
            Some(15) => {
                let o = game_vec(prop(mv.get("translation"), active_t, &mut rnd, [0.0; 4]));
                n.local.translation += o - n.offset;
                n.offset = o;
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
            let gpu = c["appearance"]["type"].as_i64() == Some(10000);
            match if gpu { -1 } else { em["type"].as_i64().unwrap_or(399) } {
                -1 => count = gpu_emissions(&c["appearance"], n, active_t),
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
                let (p, d) = if gpu { gpu_emit(&c["appearance"], &mut prnd) } else { emit_point(&c, active_t, &mut prnd) };
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
                let a = &c["appearance"];
                let life = if gpu {
                    num(&a["particleDuration"]) * a.get("particleDurationMultiplier").map_or(1.0, num)
                } else {
                    scalar(pa, "duration", 0.0, &mut prnd, -1.0)
                };
                let attached = int(pa, "attachment", 1) != 0;
                let world_p = n.world.transform_point(p);
                let world_d = (n.world.rotation * d).normalize_or(Vec3::Z);
                n.particles.push(Particle {
                    pos: if attached { p } else { world_p },
                    dir: if attached { d.normalize_or(Vec3::Z) } else { world_d },
                    // A GPU particle's speed is its velocity's length (its direction in `dir`).
                    speed: if gpu { d.length() } else { scalar(&c["particleModifier"], "speed", active_t, &mut prnd, 0.0) },
                    age: 0.0,
                    life,
                    emit_t: active_t,
                    seed: pseed,
                    attached,
                    trail: Vec::new(),
                    next_seg: 0.0,
                    next_turn: 0.0,
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
            if gpu_move(&c["appearance"], p, n.world.rotation, dt) {
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
                    // 84 ParticleAccelerationRandomTurns: a random turn of up to maxTurnAngle every
                    // turnInterval s (0: every 1/30 s frame), each particle its own (the Way of
                    // Tomoe's bolt 671032: 2 tracers a frame fanning into a web, not one line).
                    // gap: the turn's axis distribution (taken as uniform around the direction).
                    if pmt == 84 && p.age >= p.next_turn {
                        let max = scalar(&pm, "maxTurnAngle", p.age, &mut r, 0.0).to_radians();
                        if max > 0.0 {
                            let mut t = Rnd { seed: p.seed ^ (p.next_turn * 1000.0) as u32 ^ 0x84, k: 0 };
                            let axis = Quat::from_axis_angle(p.dir, t.next() * std::f32::consts::TAU) * p.dir.any_orthonormal_vector();
                            p.dir = (Quat::from_axis_angle(axis, (t.next() * 2.0 - 1.0) * max) * p.dir).normalize_or(p.dir);
                        }
                        p.next_turn += num(&pm["turnInterval"]).max(1.0 / 30.0);
                    }
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

/// A GPUStandardParticle's flight (reference/fxr src/actions/10000.yml): acceleration
/// particleAccelerationX/Y/Z + a random particleAccelerationMin..Max, in the node's frame;
/// unkParticleAcceleration is no acceleration here: the exe bakes it (properties1[16], FUN_14040b820)
/// into parameter row 6, which only the CS_VFX_StandardParticleForce* shaders read, as a factor on
/// the scene's force fields' push (cb1); without force fields it does nothing. particleGravity
/// down; with particleRandomTurns a turn of up to particleRandomTurnAngle degrees every random
/// 0..particleRandomTurnIntervalMax s (at least a 1/30 s frame). False for other appearances.
/// The game's drag (velocity *= exp(-k dt), shader parameter row 7) is unk_ds3_p1_17
/// (properties1[17], FUN_14040b820), 0 on every extracted GPU node, so not ported.
fn gpu_move(a: &Value, p: &mut Particle, node_rot: Quat, dt: f32) -> bool {
    if a["type"].as_i64() != Some(10000) {
        return false;
    }
    let mut r = Rnd { seed: p.seed ^ 0x6a09, k: 0 };
    let (lo, hi) = (vec4(&a["particleAccelerationMin"]), vec4(&a["particleAccelerationMax"]));
    let mut acc: [f32; 4] = std::array::from_fn(|i| lo[i] + (hi[i] - lo[i]) * r.next());
    for (i, k) in ["particleAccelerationX", "particleAccelerationY", "particleAccelerationZ"].iter().enumerate() {
        acc[i] += scalar(a, k, p.age, &mut r, 0.0);
    }
    let to_frame = if p.attached { Quat::IDENTITY } else { node_rot };
    let down = if p.attached { node_rot.inverse() * Vec3::NEG_Y } else { Vec3::NEG_Y };
    let mut v = p.dir * p.speed + (to_frame * game_vec(acc) + down * scalar(a, "particleGravity", p.age, &mut r, 0.0)) * dt;
    if a["particleRandomTurns"].as_bool() == Some(true) && p.age >= p.next_seg {
        let mut t = Rnd { seed: p.seed ^ (p.next_seg * 1000.0) as u32, k: 0 };
        let max = scalar(a, "particleRandomTurnAngle", p.age, &mut t, 0.0).to_radians();
        let axis = Quat::from_axis_angle(v.normalize_or(Vec3::Z), t.next() * std::f32::consts::TAU) * v.normalize_or(Vec3::Z).any_orthonormal_vector();
        v = Quat::from_axis_angle(axis, (t.next() * 2.0 - 1.0) * max) * v;
        p.next_seg += (t.next() * num(&a["particleRandomTurnIntervalMax"])).max(1.0 / 30.0);
    }
    p.speed = v.length();
    p.dir = v.normalize_or(p.dir);
    p.pos += v * dt;
    true
}

/// NodeAudio (75, reference/fxr src/actions/75.yml: `sound` = the sound id, played as event
/// `s<id>` like the TAE's effect sounds) of each node that has become active since the last call,
/// with its place. The game plays these from inside the effect: the perilous kanji 600035's root,
/// 210 of the 510 extracted ones another id than their effect's (child effects, bullets).
/// gap: unk_ds3_f1_1 / f1_2 (taken as play once, follow the node).
fn start_audio(n: &mut NodeInst, out: &mut Vec<(i64, Vec3)>) {
    if !n.audio && n.time > n.delay {
        n.audio = true;
        if let Some(c) = &n.cfg {
            let na = &c["nodeAudio"];
            if na["type"].as_i64() == Some(75) {
                if let Some(id) = na["sound"].as_i64().filter(|v| *v > 0) {
                    out.push((id, n.world.translation));
                }
            }
        }
    }
    for c in &mut n.children {
        start_audio(c, out);
    }
}

/// Advances every effect; anchors move them.
/// The root's Unk10500 rateOfTime (reference/fxr src/actions/10500.yml: "how fast time passes for
/// the entire effect", by active time): 42 of the extracted effects run at 0.5-3 (300168 1.5, the
/// boss effect 650680 1.375, 304200 0.5); 1 without it.
fn time_rate(def: &Value, age: f32) -> f32 {
    scalar(&def["root"]["unk10500"], "rateOfTime", age, &mut Rnd { seed: 0, k: 0 }, 1.0).max(0.0)
}

fn tick(mut commands: Commands, time: Res<Time>, mut lib: ResMut<FxrLib>, globals: Query<&GlobalTransform>, mut fx: Query<(Entity, &mut FxEffect)>, mut fmod: Option<NonSendMut<crate::fmod::Fmod>>, cams: Query<(&Camera, &GlobalTransform), With<Camera3d>>) {
    let dt = time.delta_secs().min(1.0 / 15.0);
    lib.cam = cams.iter().filter(|(c, _)| c.is_active).min_by_key(|(c, _)| c.order).map(|(_, g)| g.compute_transform());
    // SHINOBI_FXR_LIVE=1: the playing effects (id, age, stopped, keyed) twice a second.
    if std::env::var("SHINOBI_FXR_LIVE").is_ok() && (time.elapsed_secs() * 2.0) as u32 != ((time.elapsed_secs() - dt) * 2.0) as u32 {
        info!("fxr live {:?}", fx.iter().map(|(_, f)| (f.id, (f.age * 10.0).round() / 10.0, f.stop, f.key.is_some())).collect::<Vec<_>>());
    }
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
        let rate = lib.get(f.id).map_or(1.0, |d| time_rate(&d, f.age));
        f.age += dt;
        let too_old = !f.keep && f.age > 20.0;
        let Some(root) = f.root.as_mut() else { continue };
        // Substeps of at most 1/60 s, each laying trail points: one long frame (a hitch, a screenshot)
        // turned a 2000 deg/s spin (Ichimonji 440080) into a single chord.
        let steps = (dt * rate * 60.0).ceil().clamp(1.0, 8.0);
        for _ in 0..steps as u32 {
            step_node(root, &mut lib, Transform { scale: Vec3::ONE, ..place }, dt * rate / steps, stop || too_old);
            grow_trails(root, dt * rate / steps);
        }
        let mut start = Vec::new();
        start_audio(root, &mut start);
        let done = root.finished || (!f.keep && f.age > 30.0);
        if let Some(fm) = fmod.as_mut() {
            for (sound, at) in start {
                if f.own_sound_played && sound == f.id {
                    continue;
                }
                if let Some(ev) = fm.play_tracked(&format!("s{sound:09}"), at) {
                    f.sounds.push(ev);
                }
            }
            for ev in &f.sounds {
                if done { fm.stop(*ev) } else { fm.set_position(*ev, place.translation) }
            }
        }
        if done {
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
    /// The scene depth (`FxDepth`): the bend's depth test.
    #[texture(2, sample_type = "depth", multisampled = true)]
    depth: Handle<Image>,
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
    soft: Vec<f32>,
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
    /// Soft distance (`soften`; 0 = hard).
    soft: f32,
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
            self.soft.push(q.soft);
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
const ATTR_SOFT: bevy::mesh::MeshVertexAttribute = bevy::mesh::MeshVertexAttribute::new("FxSoft", 0x5ec1_0650, bevy::render::render_resource::VertexFormat::Float32);

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
    /// The scene depth (`FxDepth`).
    #[texture(8, sample_type = "depth", multisampled = true)]
    depth: Handle<Image>,
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
            ATTR_SOFT.at_shader_location(8),
        ])?];
        descriptor.primitive.cull_mode = None;
        Ok(())
    }
}

/// The sprite / tracer / model batches' material: unlit StandardMaterial plus the soft fade
/// (fx_soft.wgsl).
pub type SoftMaterial = bevy::pbr::ExtendedMaterial<StandardMaterial, SoftExt>;

#[derive(Asset, TypePath, bevy::render::render_resource::AsBindGroup, Debug, Clone)]
pub struct SoftExt {
    /// The scene depth (`FxDepth`).
    #[texture(100, sample_type = "depth", multisampled = true)]
    depth: Handle<Image>,
    /// Lit particles' light (`ParticleLight`, Bevy units): [ambient, sun] rgb.
    #[uniform(101)]
    light: [Vec4; 2],
}

impl bevy::pbr::MaterialExtension for SoftExt {
    fn fragment_shader() -> bevy::shader::ShaderRef {
        "embedded://sv1/fx_soft.wgsl".into()
    }
}

#[derive(Resource, Default)]
struct MultiBatches(HashMap<MultiKey, (Entity, Handle<Mesh>)>);

#[derive(Default)]
struct Buf {
    pos: Vec<[f32; 3]>,
    uv: Vec<[f32; 2]>,
    col: Vec<[f32; 4]>,
    /// x: the soft distance (`soften`; 0 = hard), read by fx_soft.wgsl as uv_b.
    soft: Vec<[f32; 2]>,
}

impl Buf {
    /// One model particle: its triangles placed by `tf`, UVs as authored.
    fn model(&mut self, m: &crate::model::FxModel, tf: Transform, col: [f32; 4]) {
        for &i in &m.indices {
            let i = i as usize;
            self.pos.push(tf.transform_point(m.pos[i]).to_array());
            self.uv.push(m.uv[i]);
            self.col.push(col);
            self.soft.push([0.0; 2]);
        }
    }
    /// `lit`: 0 unlit, else 1 + the particle's sat(N.L) toward the sun (fx_soft.wgsl lights it).
    fn quad(&mut self, c: [Vec3; 4], uv: [[f32; 2]; 4], col: [f32; 4], soft: f32, lit: f32) {
        for i in [0, 1, 2, 0, 2, 3] {
            self.pos.push(c[i].to_array());
            self.uv.push(uv[i]);
            self.col.push(col);
            self.soft.push([soft, lit]);
        }
    }
}

/// A tracer's flipbook frame: (u start, u width, v low, v high) of its cell in the atlas.
fn tracer_frame(a: &Value, age: f32, rnd: &mut Rnd) -> (f32, f32, f32, f32) {
    // Corners (u, v): (0, 1), (1, 1), (1, 0), (0, 0) of the cell.
    let ([f0, f1, _, f3], _, _) = frame_uvs(a, age, rnd, [1.0, 1.0]);
    (f0[0], f1[0] - f0[0], f3[1], f0[1])
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
/// The unlit stand-in for a lit appearance (gap where the game's lit shading is not ported).
const LIT_UNLIT: f32 = 0.125;

/// SHINOBI_FX_LIT=1: the ported lit shading (sprites / GPU particles). Off by default until the
/// light scale (g_ps_envCbColScale, g_vLightMask.x: writers not found, HANDOFF 4.58) is read live.
static LIT_PORT: std::sync::LazyLock<bool> = std::sync::LazyLock::new(|| std::env::var("SHINOBI_FX_LIT").is_ok());
/// SHINOBI_FX_ORIENT=1: the exe's orientation modes 4 / 6 / 7 (FUN_140440a60). Off by default
/// until its runtime constants and mode 7's per-particle vector are read live (HANDOFF 4.58).
static ORIENT_PORT: std::sync::LazyLock<bool> = std::sync::LazyLock::new(|| std::env::var("SHINOBI_FX_ORIENT").is_ok());

fn particle_color(a: &Value, c: &Value, p: &Particle, active_t: f32, rnd: &mut Rnd) -> [f32; 4] {
    let c1 = prop(a.get("color1"), p.age, rnd, [1.0; 4]).map(|v| v.clamp(0.0, 1.0));
    let c2 = prop(a.get("color2"), p.emit_t, rnd, [1.0; 4]).map(|v| v.clamp(0.0, 1.0));
    let c3 = prop(a.get("color3"), p.age, rnd, [1.0; 4]);
    let pm = prop(c["particleModifier"].get("color"), active_t, rnd, [1.0; 4]);
    // Lit appearances: x 1/8 where they are still drawn unlit (models, multi-texture, tracers);
    // the sprite paths undo it and light them (`LIT_UNLIT`, fx_soft.wgsl).
    // Not on additive tracers: the 48 pairs behind the 1/8 are all normal blend (603 / 606 blend 2
    // and 6), and the game's tracer pixel shader (GXFfxtessellateTracerTexture.ppo) lights by
    // saturate(colour) x light, not a fixed scale. The reference clip (sekiro_all_skills 6:45,
    // Mortal Draw 440003: lit additive 26030 / 2010 tracers, rgbMultiplier 2.5) shows the arc
    // brighter than the scene; at 1/8 it drew as faint streaks. gap: the game's light term there.
    let additive_tracer = matches!(a["type"].as_i64(), Some(606 | 10012)) && matches!(a["blendMode"].as_i64(), Some(0 | 4 | 7));
    let lit = matches!(a["lighting"].as_i64(), Some(-2 | 0)) && !additive_tracer;
    let rgbm = scalar(a, "rgbMultiplier", active_t, rnd, 1.0) * if lit { LIT_UNLIT } else { 1.0 };
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
    px: f32,
    to_sun: Vec3,
) {
    if let Some(c) = &n.cfg {
        let a = &c["appearance"];
        let at = a["type"].as_i64().unwrap_or(0);
        let active_t = (n.time - n.delay).max(0.0);
        let blend = int(a, "blendMode", 2);
        // Lit appearances (lighting -2 / 0): the game's lit particle shaders
        // (PS_VFX_SingleTextureAllLight / BumpLight) light the camera-facing quad: N.L between
        // its front (toward the camera) and the sun. gap: the normalMap's bend of N, the
        // billboard shadow term (g_ps_BillboardShadowIntensity), the Dir-only variant.
        let lit_app = *LIT_PORT && matches!(a["lighting"].as_i64(), Some(-2 | 0));
        let lit_at = |p: Vec3| if lit_app { 1.0 + to_sun.dot((cam.translation() - p).normalize_or(-cam.forward().as_vec3())).max(0.0) } else { 0.0 };
        let model = if at == 605 { lib.model(int(a, "model", -1)) } else { None };
        let tex = match at {
            604 => int(a, "layer1", 1),
            _ => int(a, "texture", 1),
        };
        let (cam_r, cam_u, cam_f) = (cam.right().as_vec3(), cam.up().as_vec3(), cam.forward().as_vec3());
        // A pure distortion layer (distortionIntensity > 0 with rgbMultiplier 0: the weapon trail
        // 401000's second tracer, normalMap 6000) shows only as refraction: its colour quads are
        // not drawn (else a black smear), only its DistortBuf ones.
        // Tracers: also a dark one (color1 starting at <= 0.13 grey) with a normalMap, 274 of the
        // 335 distorting tracers. That is the usual weapon-trail haze layer, e.g. Ichimonji 440080's black
        // 0.6 m band, which drew over its white crescent; the clips show no dark arc.
        let dark_tracer = matches!(at, 606 | 10012) && int(a, "normalMap", 0) > 0 && {
            let c = prop(a.get("color1"), 0.0, &mut Rnd { seed: 0, k: 0 }, [1.0; 4]);
            c[0].max(c[1]).max(c[2]) <= 0.13
        };
        let distortion_only = peak(&a["distortionIntensity"]) > 0.0 && (a["rgbMultiplier"].as_f64() == Some(0.0) || dark_tracer);
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
                    // Orientation, the exe's billboard builder (FUN_140440a60, the switch at
                    // 14044.c:10384; 600's twin 14043.c:9564): 1 the camera's rows; 2 the node's
                    // rotation; 4 the camera's rows with the up row a constant (world up);
                    // 6 a look-at from the camera to the particle, rolled to world up; 7 a cylinder
                    // about the particle's own vector (pfVar10[2..4], node space, turned by the node:
                    // its direction here), turned to the camera; 0 / 8+ identity. gap: 3 / 5 (a
                    // row replaced by a runtime constant, taken as camera-facing) and the constants
                    // DAT_143f083c0 / 085d0 / 08390 (zero in the file, set at run time: up = Y).
                    let (right, up) = match int(a, "orientation", 1) {
                        // The previous reading (default until the port is confirmed): 4 / 7 a yaw
                        // toward the camera about world Y / the node's Y, 3 global.
                        o @ (3 | 4 | 7) if !*ORIENT_PORT => {
                            if o == 3 {
                                (Vec3::X, Vec3::Y)
                            } else {
                                let axis = if o == 7 { n.world.rotation * Vec3::Y } else { Vec3::Y };
                                let to_cam = (cam.translation() - world).reject_from(axis).normalize_or(cam_f);
                                (axis.cross(to_cam).normalize_or(cam_r), axis)
                            }
                        }
                        0 => (Vec3::X, Vec3::Y),
                        2 => (n.world.rotation * Vec3::X, n.world.rotation * Vec3::Y),
                        4 => (cam_r, Vec3::Y),
                        6 => {
                            let d = (world - cam.translation()).normalize_or(cam_f);
                            let r = d.cross(Vec3::Y).normalize_or(cam_r);
                            (r, r.cross(d))
                        }
                        7 => {
                            let v = if p.attached { n.world.rotation * p.dir } else { p.dir };
                            let axis = v.normalize_or(n.world.rotation * Vec3::Y);
                            let to_cam = (cam.translation() - world).reject_from(axis).normalize_or(cam_f);
                            (axis.cross(to_cam).normalize_or(cam_r), axis)
                        }
                        o if o >= 8 => (Vec3::X, Vec3::Y),
                        _ => (cam_r, cam_u),
                    };
                    let (cr, sr) = (rz.cos(), rz.sin());
                    let (r2, u2) = (right * cr + up * sr, up * cr - right * sr);
                    let (hw, hh) = (r2 * w * 0.5, u2 * h * 0.5);
                    let (corners, soft) = soften(a, w.abs().max(h.abs()), cam, [world - hw - hh, world + hw - hh, world + hw + hh, world - hw + hh]);
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
                            soft,
                        };
                        multi.entry((tex, l2, l3, modes, blend)).or_default().quad(&q);
                        continue;
                    }
                    let uv = frame_uv(a, p.age, &mut rnd);
                    let col = if lit_app { [col[0] / LIT_UNLIT, col[1] / LIT_UNLIT, col[2] / LIT_UNLIT, col[3]] } else { col };
                    out.entry((tex, blend)).or_default().quad(corners, uv, col, soft, lit_at(world));
                }
                // GPUStandardParticle (10000): a camera-facing square of particleSizeX + a random
                // Min..Max (x particleSizeMultiplier; Y the same unless particleUniformScale is off),
                // coloured `color` x rgbMultiplier / alphaMultiplier (the lit 1/8 rule of
                // particle_color); with traceParticles stretched along its on-screen motion by its
                // travel over particleTraceLength frames. gap: that trace length's unit (taken as
                // 1/30 s frames, the field's "time: inv"), bloom / bloomColor, colorMin / Max.
                10000 => {
                    let mut sr = Rnd { seed: p.seed ^ 0x51ce, k: 0 };
                    let mult = a.get("particleSizeMultiplier").map_or(1.0, num);
                    let mut size = |k: &str| (num(&a[k]) + num(&a[format!("{k}Min")]) + (num(&a[format!("{k}Max")]) - num(&a[format!("{k}Min")])) * sr.next()) * mult;
                    let w = size("particleSizeX");
                    let h = if a["particleUniformScale"].as_bool() != Some(false) { w } else { size("particleSizeY") };
                    let base = vec4(&a["color"]);
                    let rgbm = num(&a["rgbMultiplier"]) * if matches!(a["lighting"].as_i64(), Some(-2 | 0)) && !*LIT_PORT { LIT_UNLIT } else { 1.0 };
                    let hue = hue_squared([base[0], base[1], base[2]]);
                    let col = [hue[0] * rgbm, hue[1] * rgbm, hue[2] * rgbm, (base[3] * num(&a["alphaMultiplier"]) * fade).clamp(0.0, 1.0)];
                    let vel = if p.attached { n.world.rotation * p.dir } else { p.dir } * p.speed;
                    let on_screen = vel.reject_from(cam_f);
                    let (along, side) = if a["traceParticles"].as_bool() == Some(true) && on_screen.length() > 1e-4 {
                        let axis = on_screen.normalize();
                        let len = h + p.speed * num(&a["particleTraceLength"]) / 30.0;
                        (axis * len * 0.5, axis.cross(cam_f).normalize_or(cam_r) * w * 0.5)
                    } else {
                        (cam_u * h * 0.5, cam_r * w * 0.5)
                    };
                    let (corners, soft) = soften(a, w.max(h), cam, [world - side - along, world + side - along, world + side + along, world - side + along]);
                    out.entry((tex, blend)).or_default().quad(corners, frame_uv(a, p.age, &mut rnd), col, soft, lit_at(world));
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
                // Line (601): a line primitive, a pixel wide, length / lengthMultiplier along the
                // particle's direction like QuadLine's (reference/fxr fxr.ts class Line); drawn as a
                // one-pixel quad. Its colours: color1-3 as the other appearances, times the mean of
                // startColor (ActiveTime) and endColor. gap: the gradient from the start colour to
                // the end colour along the line (one colour per quad here), alphaThreshold. Used once:
                // c1700's 120014.
                601 => {
                    let len = scalar(a, "length", p.emit_t, &mut rnd, 1.0) * scalar(a, "lengthMultiplier", p.age, &mut rnd, 1.0) * sy;
                    let sc = prop(a.get("startColor"), active_t, &mut rnd, [1.0; 4]).map(|v| v.clamp(0.0, 1.0));
                    let ec = prop(a.get("endColor"), active_t, &mut rnd, [1.0; 4]).map(|v| v.clamp(0.0, 1.0));
                    let col: [f32; 4] = std::array::from_fn(|i| col[i] * (sc[i] + ec[i]) * 0.5);
                    let d = if p.attached { n.world.rotation * p.dir } else { p.dir };
                    let side = d.cross(cam_f).normalize_or(cam_r) * px * world.distance(cam.translation()) * 0.5;
                    let tail = world - d * len;
                    out.entry((-1, blend)).or_default().quad([tail - side, world - side, world + side, tail + side], [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]], col, 0.0, 0.0);
                }
                602 => {
                    let wl = scalar(a, "width", p.emit_t, &mut rnd, 1.0) * scalar(a, "widthMultiplier", p.age, &mut rnd, 1.0) * sx;
                    let len = scalar(a, "length", p.emit_t, &mut rnd, 1.0) * scalar(a, "lengthMultiplier", p.age, &mut rnd, 1.0) * sy;
                    let d = if p.attached { n.world.rotation * p.dir } else { p.dir };
                    let side = d.cross(cam_f).normalize_or(cam_r) * wl * 0.5;
                    let tail = world - d * len;
                    out.entry((-1, blend)).or_default().quad([tail - side, world - side, world + side, tail + side], [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]], col, 0.0, 0.0);
                }
                // Distortion (607): a camera-facing (orientation as the billboards) quad, sizeX / Y;
                // its normal map at UV x normalMapScale + normalMapOffset + normalMapSpeed x age,
                // intensity = g_waveStrength, 1 / radius = g_clampDistanceInverse (radius 1: the
                // ellipse inscribing the quad). RadialBlur (608): width / height, its mask, blurRadius =
                // g_radialDistance, toward the particle's centre on screen. 607's mode is 1 NormalMap in
                // all 45 extracted ones (the only one drawn). gap: 607's shape (1 Hemiellipsoid in one: drawn flat), 608's
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
                    // The texture (reference/fxr 10012.yml): the flipbook frame (totalFrames /
                    // columns, frameIndex + frameIndexOffset) across the trail, and along it
                    // textureFraction of the width per segment, fixed where each segment was laid
                    // (attachedUV off), or for the whole trail (on). Was the whole atlas across the
                    // trail and 0..1 along it: Genichiro's lightning (33020, 16 bolts stacked) drew
                    // as dotted straight lines (manual test #5).
                    let (fu0, fu_w, fv_lo, fv_hi) = tracer_frame(a, p.age, &mut rnd);
                    let frac = scalar(a, "textureFraction", p.age, &mut rnd, 1.0);
                    let interval = num(&a["segmentInterval"]).max(1.0 / 60.0);
                    let (born0, born1) = (p.trail[0].2, p.trail[p.trail.len() - 1].2);
                    let along = |place: f32| -> f32 {
                        if a["attachedUV"].as_bool() != Some(false) {
                            place * frac
                        } else {
                            (born0 + place * (born1 - born0)) / interval * frac
                        }
                    };
                    let tu = |u: f32| fu0 + u * fu_w;
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
                        // The width runs along the point's source axis: the exe's tracer builder
                        // (FUN_14042c4d0, the orientation switch at 14042da96) writes, per point,
                        // the vector the domain shader spreads the width along
                        // (GXFfxtessellateTracerTexture.dpo: lerp(TEXCOORD4.xyz) x (v - 0.5)):
                        // mode 0 travel x camera, 1 the stored node Z itself, 2 / 3 constants. The
                        // face normal is travel x that axis. (LocalZ was read as travel x Z before.)
                        let side = |x: Vec3| if x == Vec3::ZERO { travel } else { x };
                        let s0 = side(x0) * w * 0.5;
                        let s1 = side(x1) * w * 0.5;
                        let (k0, k1) = (fade(u0), fade(u1));
                        let c0 = [col[0], col[1], col[2], col[3] * k0.min(k1)];
                        let quv = [[u0, 1.0], [u1, 1.0], [u1, 0.0], [u0, 0.0]];
                        let (t0, t1) = (tu(along(u0)), tu(along(u1)));
                        let tuv = [[t0, fv_hi], [t1, fv_hi], [t1, fv_lo], [t0, fv_lo]];
                        if bend > 0.0 && nm > 0 {
                            let a_fade = (scalar(a, "alphaMultiplier", active_t, &mut rnd, 1.0) * k0.min(k1)).clamp(0.0, 1.0);
                            dist.entry(nm).or_default().quad([a0 - s0, a1 - s1, a1 + s1, a0 + s0], quv, quv, [1.0, 1.0, 1.0, a_fade], [2.0, bend, 1.0, 0.0], a0);
                        }
                        if distortion_only {
                            continue;
                        }
                        let b = out.entry((tex, blend)).or_default();
                        b.quad([a0 - s0, a1 - s1, a1 + s1, a0 + s0], tuv, c0, 0.0, 0.0);
                        if cross {
                            let (c0x, c1x) = (Vec3::X * w * 0.5, Vec3::X * w * 0.5);
                            b.quad([a0 - c0x, a1 - c1x, a1 + c1x, a0 + c0x], tuv, c0, 0.0, 0.0);
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
        collect(ch, lib, cam, out, dist, multi, lights, px, to_sun);
    }
}

/// Soft particles, as the game's soft shaders (GXFfxtessellateSoftTexture.gpo / GXFfxsoftTracer.vpo,
/// cbuffer g_*_softSpriteScaleOffset = (scale, offset)): h = scale x size / 2; the particle's view
/// depth w is pulled to w - (h + offset) (nearer than near + 1 m it goes back toward w by up to
/// 1), drawn at the same place on screen, and fades over the soft distance 2 offset + h where it
/// meets the scene (fx_soft.wgsl, fx_multi.wgsl). Only where the appearance's depthBlend is set
/// (603 / 604; reference/fxr 603.yml: "display in front of the object if they are close enough,
/// and fade out with distance from the object's surface"; 1102 of 1596 billboards). gap: scale /
/// offset read as unkDepthBlend1 / unkDepthBlend2 (the one float pair of that shape: 1 / 0.5 /
/// 0.25 and 0 / -0.5 / -1) and size as the particle's larger side; the other appearances have no
/// depthBlend field and are drawn hard (what picks GXFfxsoftTracer for a trail is not traced).
fn soften(a: &Value, size: f32, cam: &GlobalTransform, c: [Vec3; 4]) -> ([Vec3; 4], f32) {
    if a["depthBlend"].as_bool() != Some(true) {
        return (c, 0.0);
    }
    // Bevy's default perspective near plane (camera.rs sets none).
    const NEAR: f32 = 0.1;
    let scale = a.get("unkDepthBlend1").and_then(Value::as_f64).unwrap_or(1.0) as f32;
    let off = a.get("unkDepthBlend2").and_then(Value::as_f64).unwrap_or(0.0) as f32;
    let h = scale * size * 0.5;
    let (eye, fwd) = (cam.translation(), cam.forward().as_vec3());
    let pull = |p: Vec3| {
        let w = (p - eye).dot(fwd);
        if w <= 1e-3 {
            return p;
        }
        let mut nw = w - (h + off);
        if nw < NEAR + 1.0 {
            nw += (NEAR + 1.0 - nw).min(1.0) * (w - nw);
        }
        eye + (p - eye) * (nw / w)
    };
    (c.map(pull), 2.0 * off + h)
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
            // local Z (the facing; width = travel x Z at draw), 2 global vertical, 3 global X, 4 cross (vertical here, X added at draw),
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
    assets: Res<AssetServer>,
    mut batches: ResMut<Batches>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<SoftMaterial>>,
    fx_depth: Res<FxDepth>,
    mut dist_materials: ResMut<Assets<DistortMaterial>>,
    mut dist_batches: ResMut<DistortBatches>,
    mut multi_materials: ResMut<Assets<MultiMaterial>>,
    mut multi_batches: ResMut<MultiBatches>,
    camera: Query<(&GlobalTransform, Option<&Projection>, Option<&Camera>), With<Camera3d>>,
    mut fx: Query<&mut FxEffect>,
    old_lights: Query<Entity, With<FxLight>>,
    mut lib: ResMut<FxrLib>,
    (map_light, soft_mats, mut uploads): (Option<Res<crate::map::MapLighting>>, Query<&MeshMaterial3d<SoftMaterial>>, ResMut<FxUploads>),
) {
    let Some((cam, proj, view)) = camera.iter().next() else { return };
    // Lit particles' light (game units -> the map's radiance scale, ENV_PER_UNIT): ambient, sun.
    let (amb, sun, to_sun) = map_light.as_deref().map(crate::map::particle_light).unwrap_or((Vec3::splat(0.4), Vec3::splat(1.5), Vec3::new(0.3, 0.8, 0.5).normalize()));
    let light = [(amb * crate::map::ENV_PER_UNIT).extend(0.0), (sun * crate::map::ENV_PER_UNIT).extend(0.0)];
    for m in &soft_mats {
        if materials.get(&m.0).is_some_and(|x| x.extension.light != light) {
            if let Some(mut x) = materials.get_mut(&m.0) {
                x.extension.light = light;
            }
        }
    }
    // One screen pixel's height at 1 m (Line 601 is drawn a pixel wide).
    let fov = match proj {
        Some(Projection::Perspective(p)) => p.fov,
        _ => std::f32::consts::FRAC_PI_4,
    };
    let rows = view.and_then(|c| c.physical_viewport_size()).map_or(1080.0, |s| s.y.max(1) as f32);
    let px = 2.0 * (fov * 0.5).tan() / rows;
    let mut bufs: HashMap<(i64, i64), Buf> = HashMap::new();
    let mut dist: HashMap<i64, DistortBuf> = HashMap::new();
    let mut multi: HashMap<MultiKey, MultiBuf> = HashMap::new();
    let mut lights = Vec::new();
    for mut f in &mut fx {
        if let Some(root) = f.root.as_mut() {
            collect(root, &mut lib, cam, &mut bufs, &mut dist, &mut multi, &mut lights, px, to_sun);
        }
    }
    if std::env::var("SHINOBI_FXR_LOG").is_ok() && !(bufs.is_empty() && dist.is_empty() && multi.is_empty()) {
        let mut v: Vec<String> = bufs.iter().map(|(k, b)| format!("{}/{}:{}", k.0, k.1, b.pos.len() / 6)).collect();
        v.extend(dist.iter().map(|(k, b)| format!("screen {k}:{}", b.pos.len() / 6)));
        v.extend(multi.iter().map(|(k, b)| format!("multi {}+{}+{}/{}:{}", k.0, k.1, k.2, k.4, b.pos.len() / 6)));
        v.sort();
        info!("fxr quads {}", v.join(" "));
    }
    // Upload: one mesh per (texture, blend mode).
    for (key, buf) in bufs.iter() {
        if !batches.map.contains_key(key) {
            let (tex, blend) = *key;
            let image = if tex <= MODEL_KEY {
                // A Model appearance: its mesh's albedo (extracted/tex, written by the model export).
                let m = lib.model(MODEL_KEY - tex);
                if std::env::var("SHINOBI_FXR_LOG").is_ok() {
                    info!("fxr model {} albedo {:?}", MODEL_KEY - tex, m.as_ref().map(|m| m.albedo.clone()));
                }
                m.filter(|m| !m.albedo.is_empty()).map(|m| assets.load(format!("tex/{}.dds", m.albedo)))
            } else {
                // U repeats (a tracer lays textureFraction of the width per segment, past 1), V
                // clamps (flipbook rows stay apart).
                (tex > 0).then(|| {
                    assets.load_builder().with_settings(|s: &mut bevy::image::ImageLoaderSettings| {
                        s.sampler = bevy::image::ImageSampler::Descriptor(bevy::image::ImageSamplerDescriptor {
                            address_mode_u: bevy::image::ImageAddressMode::Repeat,
                            ..bevy::image::ImageSamplerDescriptor::linear()
                        });
                    }).load(format!("fxr_tex/{tex}.png"))
                })
            };
            // BlendMode: 0 / 4 / 7 additive, 1 opaque, 3 multiply, 2 / 6 normal (5 subtract: normal).
            let alpha_mode = match blend {
                0 | 4 | 7 => AlphaMode::Add,
                1 => AlphaMode::Opaque,
                3 => AlphaMode::Multiply,
                _ => AlphaMode::Blend,
            };
            let mat = materials.add(SoftMaterial {
                base: StandardMaterial { base_color: Color::WHITE, base_color_texture: image, unlit: true, alpha_mode, cull_mode: None, double_sided: true, ..default() },
                extension: SoftExt { depth: fx_depth.image.clone(), light },
            });
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
        let mat = dist_materials.add(DistortMaterial { tex: image, depth: fx_depth.image.clone() });
        let mesh = meshes.add(Mesh::new(bevy::mesh::PrimitiveTopology::TriangleList, bevy::asset::RenderAssetUsages::default()));
        let e = commands.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(mat), Transform::IDENTITY, bevy::light::NotShadowCaster, bevy::camera::visibility::NoFrustumCulling)).id();
        dist_batches.0.insert(*tex, (e, mesh));
    }
    let new_mesh = || Mesh::new(bevy::mesh::PrimitiveTopology::TriangleList, bevy::asset::RenderAssetUsages::default());
    for (tex, (_, mesh)) in dist_batches.0.iter() {
        let empty = DistortBuf::default();
        let b = dist.get(tex).unwrap_or(&empty);
        // Never empty: a degenerate triangle far below (and the padding to the bucket).
        let c = uploads.cap(mesh, b.pos.len() + 3);
        let below = [0.0, -50.0, 0.0];
        let mut m = new_mesh();
        m.insert_attribute(Mesh::ATTRIBUTE_POSITION, pad_to(b.pos.clone(), c, below));
        m.insert_attribute(Mesh::ATTRIBUTE_UV_0, pad_to(b.uv.clone(), c, [0.0; 2]));
        m.insert_attribute(Mesh::ATTRIBUTE_UV_1, pad_to(b.tex_uv.clone(), c, [0.0; 2]));
        m.insert_attribute(Mesh::ATTRIBUTE_COLOR, pad_to(b.col.clone(), c, [0.0; 4]));
        m.insert_attribute(ATTR_FX, pad_to(b.fx.clone(), c, [0.0; 4]));
        m.insert_attribute(ATTR_CENTER, pad_to(b.center.clone(), c, below));
        uploads.put(&mut meshes, mesh, m);
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
            depth: fx_depth.image.clone(),
        });
        let mesh = meshes.add(Mesh::new(bevy::mesh::PrimitiveTopology::TriangleList, bevy::asset::RenderAssetUsages::default()));
        let e = commands.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(mat), Transform::IDENTITY, bevy::light::NotShadowCaster, bevy::camera::visibility::NoFrustumCulling)).id();
        multi_batches.0.insert(*key, (e, mesh));
    }
    for (key, (_, mesh)) in multi_batches.0.iter() {
        let empty = MultiBuf::default();
        let b = multi.get(key).unwrap_or(&empty);
        let c = uploads.cap(mesh, b.pos.len() + 3);
        let mut m = new_mesh();
        m.insert_attribute(Mesh::ATTRIBUTE_POSITION, pad_to(b.pos.clone(), c, [0.0, -50.0, 0.0]));
        m.insert_attribute(Mesh::ATTRIBUTE_UV_0, pad_to(b.uv1.clone(), c, [0.0; 2]));
        m.insert_attribute(Mesh::ATTRIBUTE_UV_1, pad_to(b.uv2.clone(), c, [0.0; 2]));
        m.insert_attribute(ATTR_UV3N, pad_to(b.uv3n.clone(), c, [0.0; 4]));
        m.insert_attribute(Mesh::ATTRIBUTE_COLOR, pad_to(b.col.clone(), c, [0.0; 4]));
        m.insert_attribute(ATTR_C1, pad_to(b.c1.clone(), c, [0.0; 4]));
        m.insert_attribute(ATTR_C2, pad_to(b.c2.clone(), c, [0.0; 4]));
        m.insert_attribute(ATTR_C3, pad_to(b.c3.clone(), c, [0.0; 4]));
        m.insert_attribute(ATTR_SOFT, pad_to(b.soft.clone(), c, 0.0));
        uploads.put(&mut meshes, mesh, m);
    }
    for (key, (_, mesh)) in batches.map.iter() {
        let empty = Buf::default();
        let buf = bufs.get(key).unwrap_or(&empty);
        // Never empty (a degenerate triangle far below), see ffx.rs; padded to the bucket.
        let c = uploads.cap(mesh, buf.pos.len() + 3);
        let mut m = new_mesh();
        m.insert_attribute(Mesh::ATTRIBUTE_POSITION, pad_to(buf.pos.clone(), c, [0.0, -50.0, 0.0]));
        m.insert_attribute(Mesh::ATTRIBUTE_UV_0, pad_to(buf.uv.clone(), c, [0.0; 2]));
        m.insert_attribute(Mesh::ATTRIBUTE_UV_1, pad_to(buf.soft.clone(), c, [0.0; 2]));
        m.insert_attribute(Mesh::ATTRIBUTE_COLOR, pad_to(buf.col.clone(), c, [0.0; 4]));
        m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; c]);
        uploads.put(&mut meshes, mesh, m);
    }
    // Lights: rebuilt each frame, the brightest 8. They use the sun's bridge (map.rs SCENE_TO_BEVY lux
    // per game unit). The unit is matched at 1 m, so 4 pi x 4500 lm per diffuseMultiplier;
    // before, x 4000 lm, about 14x dimmer. gap: the game's falloff curve (Bevy's is 1/d^2 within range).
    for e in &old_lights {
        commands.entity(e).despawn();
    }
    lights.sort_by(|a, b| b.2.total_cmp(&a.2));
    for (p, c, k, r) in lights.into_iter().take(8) {
        // Spawned after this frame's propagation: its GlobalTransform given, or it would light
        // from the origin.
        commands.spawn((PointLight { color: c, intensity: k * 4.0 * std::f32::consts::PI * crate::map::SCENE_TO_BEVY, range: r.max(0.5), shadow_maps_enabled: false, ..default() }, Transform::from_translation(p), GlobalTransform::from_translation(p), FxLight));
    }
}

pub struct FxrPlugin;

impl Plugin for FxrPlugin {
    fn build(&self, app: &mut App) {
        bevy::asset::embedded_asset!(app, "fx_distort.wgsl");
        bevy::asset::embedded_asset!(app, "fx_multi.wgsl");
        bevy::asset::embedded_asset!(app, "fx_soft.wgsl");
        app.add_plugins((MaterialPlugin::<DistortMaterial>::default(), MaterialPlugin::<MultiMaterial>::default(), MaterialPlugin::<SoftMaterial>::default()));
        app.init_resource::<FxrLib>()
            .init_resource::<Batches>()
            .init_resource::<DistortBatches>()
            .init_resource::<MultiBatches>()
            .init_resource::<FxUploads>()
            // After the transforms propagate: anchored effects read the pose the models are drawn
            // with this frame (anim.rs draws between the last two sim poses), and a bullet spawned
            // this frame already has its GlobalTransform.
            .add_systems(PostUpdate, (tick, draw).chain().after(TransformSystems::Propagate).after(crate::model::WeaponLocation))
            .add_systems(Startup, |mut commands: Commands, mut images: ResMut<Assets<Image>>| {
                commands.insert_resource(FxDepth { image: images.add(depth_image(UVec2::ONE)) });
            })
            .add_systems(Update, soft_depth)
            .add_plugins(bevy::render::extract_resource::ExtractResourcePlugin::<FxDepth>::default());
        if let Some(render_app) = app.get_sub_app_mut(bevy::render::RenderApp) {
            use bevy::render::{Render, RenderSystems};
            render_app.init_resource::<FxRenderUploads>();
            render_app.add_systems(bevy::render::ExtractSchedule, extract_fx_uploads);
            render_app.add_systems(Render, write_fx_uploads.in_set(RenderSystems::PrepareAssets).after(bevy::render::mesh::allocator::allocate_and_free_meshes));
        }
        if std::env::var("SHINOBI_NO_SOFT").is_err()
            && let Some(render_app) = app.get_sub_app_mut(bevy::render::RenderApp)
        {
            use bevy::core_pipeline::core_3d::{main_opaque_pass_3d, main_transparent_pass_3d};
            // Before the transmissive pass too: the distortions (DistortMaterial) read it there.
            render_app.add_systems(
                bevy::core_pipeline::schedule::Core3d,
                copy_fx_depth.after(main_opaque_pass_3d).before(bevy::pbr::main_transmissive_pass_3d).before(main_transparent_pass_3d),
            );
        }
        if std::env::var("SHINOBI_FPS_LOG").is_ok() {
            app.add_systems(Update, fps_log);
        }
        if std::env::var("SHINOBI_FXR_TEST").is_ok() {
            app.add_systems(Update, test_shots);
        }
    }
}

/// Soft particles read the scene depth (the game's g_depthTexture, t3 in GXFfxtessellateSoft*.ppo /
/// GXFfxsoftTracer.ppo): `copy_fx_depth` copies the camera's depth buffer after the opaque pass
/// into this image, which the effect materials bind. (Not Bevy's DepthPrepass: it draws every
/// mesh twice, and with MSAA it writes the alpha-to-coverage hair / leaves as solid, so their
/// edges showed the clear colour.) Never written (SHINOBI_NO_SOFT=1, or a size / MSAA mismatch),
/// it stays 0 = infinitely far and the particles keep hard edges.
#[derive(Resource, Clone, bevy::render::extract_resource::ExtractResource)]
pub struct FxDepth {
    image: Handle<Image>,
}

/// The copy target: the camera depth's format and sample count (Bevy's Depth32Float, MSAA 4; the
/// camera sets no Msaa, so Bevy's default 4 holds).
fn depth_image(size: UVec2) -> Image {
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
    let mut img = Image::new_uninit(
        Extent3d { width: size.x.max(1), height: size.y.max(1), depth_or_array_layers: 1 },
        TextureDimension::D2,
        TextureFormat::Depth32Float,
        bevy::asset::RenderAssetUsages::RENDER_WORLD,
    );
    img.texture_descriptor.sample_count = 4;
    // RENDER_ATTACHMENT: wgpu requires it of multisampled textures.
    img.texture_descriptor.usage = TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST | TextureUsages::RENDER_ATTACHMENT;
    img
}

/// Keeps `FxDepth` at the camera's size (re-binding the effect materials on a resize) and lets
/// the camera's depth buffer be copied from.
fn soft_depth(
    fx: Option<Res<FxDepth>>,
    mut images: ResMut<Assets<Image>>,
    mut soft: ResMut<Assets<SoftMaterial>>,
    mut multi: ResMut<Assets<MultiMaterial>>,
    mut dist: ResMut<Assets<DistortMaterial>>,
    mut cams: Query<(&Camera, &mut Camera3d)>,
) {
    let Some(fx) = fx else { return };
    if std::env::var("SHINOBI_NO_SOFT").is_ok() {
        return;
    }
    for (cam, mut c3) in &mut cams {
        let usage = bevy::render::render_resource::TextureUsages::from(c3.depth_texture_usages);
        if !usage.contains(bevy::render::render_resource::TextureUsages::COPY_SRC) {
            c3.depth_texture_usages = (usage | bevy::render::render_resource::TextureUsages::COPY_SRC).into();
        }
        let Some(size) = cam.physical_target_size() else { continue };
        let Some(img) = images.get(&fx.image) else { continue };
        if img.size() == size {
            continue;
        }
        let _ = images.insert(&fx.image, depth_image(size));
        for _ in soft.iter_mut() {}
        for _ in multi.iter_mut() {}
        for _ in dist.iter_mut() {}
    }
}

/// Render world: the camera's depth (opaque pass done) into `FxDepth`, when size and sample
/// count match (Bevy's own prepass copies its depth the same way, prepass/node.rs).
fn copy_fx_depth(
    view: bevy::render::renderer::ViewQuery<&bevy::render::view::ViewDepthTexture>,
    fx: Option<Res<FxDepth>>,
    images: Res<bevy::render::render_asset::RenderAssets<bevy::render::texture::GpuImage>>,
    mut ctx: bevy::render::renderer::RenderContext,
) {
    let depth = view.into_inner();
    let Some(fx) = fx else { return };
    let Some(dst) = images.get(&fx.image) else { return };
    let (s, d) = (&depth.texture, &dst.texture);
    if s.size() != d.size() || s.sample_count() != d.sample_count() || s.format() != d.format() {
        return;
    }
    ctx.command_encoder().copy_texture_to_texture(s.as_image_copy(), d.as_image_copy(), s.size());
}

/// SHINOBI_FPS_LOG=1: vsync off and the mean frame time every 240 frames (for costing changes).
fn fps_log(time: Res<Time>, mut acc: Local<(u32, f32)>, mut windows: Query<&mut Window>) {
    for mut w in &mut windows {
        if w.present_mode != bevy::window::PresentMode::AutoNoVsync {
            w.present_mode = bevy::window::PresentMode::AutoNoVsync;
        }
    }
    acc.0 += 1;
    acc.1 += time.delta_secs();
    if acc.0 == 240 {
        info!("frame {:.2} ms ({:.0} fps)", acc.1 / 240.0 * 1000.0, 240.0 / acc.1);
        *acc = (0, 0.0);
    }
}

/// SHINOBI_FXR_TEST=<id>: plays that effect 1.5 m (SHINOBI_FXR_DIST) in front of Wolf at chest height, its +Z to
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
    // SHINOBI_FXR_WARM=1: one more play 2 s before, so its textures and batches exist.
    if f == start || (f == start - 120 && std::env::var("SHINOBI_FXR_WARM").is_ok()) {
        let Ok(w) = wolf.single() else { return };
        // Wolf faces his -Z (the camera sits on his +Z side).
        let fwd = (w.rotation() * Vec3::NEG_Z).with_y(0.0).normalize_or(Vec3::NEG_Z);
        let right = fwd.cross(Vec3::Y);
        let ahead = std::env::var("SHINOBI_FXR_DIST").ok().and_then(|v| v.parse::<f32>().ok()).unwrap_or(1.5);
        let at = Transform::from_translation(w.translation() + fwd * ahead + Vec3::Y * 1.3).looking_to(-right, Vec3::Y);
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

    /// NodeTranslation (15) offsets the node by its `translation` curve (300169: (0, -0.4, 0) ->
    /// 0 over 0.3 s); NodeSpeed (120) moves it along Z at speedZ x speedMultiplierZ.
    /// A NodeEmitter (2202) keeps every child as a template (the perilous kanji 600035 and 65
    /// other effects have several); LevelsOfDetail (2002) draws only its first child.
    #[test]
    fn node_emitters_keep_all_children_levels_of_detail_one() {
        let mut lib = FxrLib::default();
        let kid = serde_json::json!({"type": 2200, "stateConfigMap": [0], "configs": [{"type": 1004}]});
        let emitter = serde_json::json!({"type": 2202, "stateConfigMap": [0], "configs": [{"type": 1005}], "nodes": [kid, kid, kid]});
        let n = make_node(&mut lib, &emitter, 1, 0).unwrap();
        assert_eq!((n.templates.len(), n.children.len()), (3, 0));
        let lod = serde_json::json!({"type": 2002, "nodes": [kid, kid]});
        assert_eq!(make_node(&mut lib, &lod, 1, 0).unwrap().children.len(), 1);
    }

    /// Manual test #5: a tracer samples one flipbook cell across the trail. Genichiro's lightning
    /// 33020 (totalFrames 16, columns 0 = one column) is 16 bolts stacked: frame 3 is v 3/16..4/16.
    #[test]
    fn a_tracer_draws_one_flipbook_frame() {
        let a = serde_json::json!({"type": 10012, "texture": 33020, "frameIndex": 3, "frameIndexOffset": 0, "columns": 0, "totalFrames": 16});
        let (u0, uw, lo, hi) = tracer_frame(&a, 0.0, &mut Rnd { seed: 1, k: 0 });
        assert_eq!((u0, uw), (0.0, 1.0));
        assert!((lo - 3.0 / 16.0).abs() < 1e-6 && (hi - 4.0 / 16.0).abs() < 1e-6, "{lo} {hi}");
    }

    /// A node's nodeAudio (75) starts once, when the node becomes active (after its delay).
    #[test]
    fn node_audio_starts_once_after_the_delay() {
        let mut lib = FxrLib::default();
        let node = serde_json::json!({"type": 2200, "stateConfigMap": [0], "configs": [{"type": 1004,
            "nodeAttributes": {"type": 128, "duration": 1.0, "delay": 0.1},
            "nodeAudio": {"type": 75, "sound": 600035}}]});
        let mut n = make_node(&mut lib, &node, 1, 0).unwrap();
        let mut out = Vec::new();
        step_node(&mut n, &mut lib, Transform::IDENTITY, 0.05, false);
        start_audio(&mut n, &mut out);
        assert!(out.is_empty());
        for _ in 0..4 {
            step_node(&mut n, &mut lib, Transform::IDENTITY, 0.05, false);
            start_audio(&mut n, &mut out);
        }
        assert_eq!(out.iter().map(|(s, _)| *s).collect::<Vec<_>>(), vec![600035]);
    }

    #[test]
    fn node_translation_and_node_speed_move_the_node() {
        let mut lib = FxrLib::default();
        let config = |mv: serde_json::Value| serde_json::json!({"type": 2200, "stateConfigMap": [0], "configs": [{"nodeMovement": mv}]});
        let tr = config(serde_json::json!({"type": 15, "translation": {"function": "Linear", "loop": false, "keyframes": [{"position": 0, "value": [0, -0.4, 0]}, {"position": 0.3, "value": [0, 0, 0]}]}}));
        let mut n = make_node(&mut lib, &tr, 1, 0).unwrap();
        step_node(&mut n, &mut lib, Transform::IDENTITY, 0.0, false);
        assert!((n.local.translation.y + 0.4).abs() < 1e-4);
        for _ in 0..20 {
            step_node(&mut n, &mut lib, Transform::IDENTITY, 1.0 / 60.0, false);
        }
        assert!(n.local.translation.length() < 1e-4, "{:?}", n.local.translation);
        let sp = config(serde_json::json!({"type": 120, "speedZ": 2.0, "speedMultiplierZ": 0.5}));
        let mut n = make_node(&mut lib, &sp, 1, 0).unwrap();
        for _ in 0..60 {
            step_node(&mut n, &mut lib, Transform::IDENTITY, 1.0 / 60.0, false);
        }
        assert!((n.local.translation.z - 1.0).abs() < 1e-3, "{:?}", n.local.translation);
    }

    /// NodeAccelerationSpin (113) moves along Z as NodeAcceleration and turns by its angular
    /// speeds; NodeAttachToCamera (46) puts the node on the camera.
    #[test]
    fn node_spin_and_camera_attach_move_the_node() {
        let mut lib = FxrLib::default();
        let config = |mv: serde_json::Value| serde_json::json!({"type": 2200, "stateConfigMap": [0], "configs": [{"nodeMovement": mv}]});
        let spin = config(serde_json::json!({"type": 113, "speedZ": 1.0, "angularSpeedY": 90.0, "angularSpeedMultiplierY": 2.0}));
        let mut n = make_node(&mut lib, &spin, 1, 0).unwrap();
        for _ in 0..30 {
            step_node(&mut n, &mut lib, Transform::IDENTITY, 1.0 / 60.0, false);
        }
        let turned = (n.local.rotation * Vec3::Z).angle_between(Vec3::Z).to_degrees();
        assert!((turned - 90.0).abs() < 1.0, "{turned}");
        assert!(n.local.translation.length() > 0.3, "{:?}", n.local.translation);
        let cam = Transform::from_xyz(3.0, 2.0, 1.0).with_rotation(Quat::from_rotation_y(1.0));
        lib.cam = Some(cam);
        let on_cam = config(serde_json::json!({"type": 46, "followRotation": true}));
        let mut n = make_node(&mut lib, &on_cam, 1, 0).unwrap();
        step_node(&mut n, &mut lib, Transform::from_xyz(-5.0, 0.0, 0.0), 1.0 / 60.0, false);
        assert!(n.world.translation.distance(cam.translation) < 1e-4);
        assert!(n.world.rotation.angle_between(cam.rotation) < 1e-4);
        let still = config(serde_json::json!({"type": 46, "followRotation": false}));
        let mut n = make_node(&mut lib, &still, 1, 0).unwrap();
        step_node(&mut n, &mut lib, Transform::IDENTITY, 1.0 / 60.0, false);
        assert!(n.world.translation.distance(cam.translation) < 1e-4 && n.world.rotation.angle_between(Quat::IDENTITY) < 1e-4);
    }

    /// Demon of Hatred's embers 670240 (GPUStandardParticle 10000): a cylinder 10 m across emits
    /// 100 + 50 particles a 1/30 s frame while the 1 s node lives, each flying outward level with
    /// the ground at 15-22.5 m/s (10-15 for the 5 m one) for 6 s.
    /// unkParticleAcceleration (1 on the Demon's embers 670240 / 670271) only scales the scene's
    /// force fields (shader row 6, CS_VFX_StandardParticleForce*): alone it does not speed a GPU
    /// particle up (it was added to acceleration Z, so the embers kept flying faster).
    #[test]
    fn gpu_unk_acceleration_is_no_push() {
        let a = serde_json::json!({"type": 10000, "unkParticleAcceleration": 1.0});
        let mut p = Particle { pos: Vec3::ZERO, dir: Vec3::X, speed: 5.0, age: 0.0, life: 6.0, emit_t: 0.0, seed: 1, attached: false, trail: Vec::new(), next_seg: 0.0, next_turn: 0.0, dead: false, ended: false };
        for _ in 0..30 {
            assert!(gpu_move(&a, &mut p, Quat::IDENTITY, 1.0 / 30.0));
        }
        assert!((p.speed - 5.0).abs() < 1e-4 && p.dir.abs_diff_eq(Vec3::X, 1e-5), "{} {}", p.speed, p.dir);
    }

    /// ParticleAccelerationRandomTurns (84) turns each particle on its own: the Way of Tomoe's bolt
    /// 671032 emits 2 tracers a frame from one point, and they fan out into a web (they all flew
    /// one straight line, drawn as 3-4 thin bolts where the clip has a forked web).
    #[test]
    fn random_turn_particles_fan_out() {
        let mut lib = FxrLib::default();
        let Some(def) = lib.get(671032) else { return };
        let mut n = make_node(&mut lib, &def["root"], 7, 0).unwrap();
        for _ in 0..8 {
            step_node(&mut n, &mut lib, Transform::IDENTITY, 1.0 / 30.0, false);
        }
        fn dirs(n: &NodeInst, out: &mut Vec<Vec3>) {
            if n.cfg.as_ref().is_some_and(|c| c["particleMovement"]["type"].as_i64() == Some(84) && c["particleMovement"]["maxTurnAngle"].as_f64().unwrap_or(0.0) > 0.0) {
                out.extend(n.particles.iter().filter(|p| p.speed > 0.0).map(|p| p.dir));
            }
            for c in &n.children {
                dirs(c, out);
            }
        }
        let mut ds = Vec::new();
        dirs(&n, &mut ds);
        assert!(ds.len() >= 10, "{} turning particles", ds.len());
        let spread = ds.iter().map(|d| d.angle_between(ds[0]).to_degrees()).fold(0.0, f32::max);
        assert!(spread > 20.0, "the bolts fly within {spread} deg of each other");
    }

    #[test]
    fn gpu_particles_burst_out_of_their_cylinder() {
        let mut lib = FxrLib::default();
        let Some(def) = lib.get(670240) else { return };
        let mut n = make_node(&mut lib, &def["root"], 7, 0).unwrap();
        step_node(&mut n, &mut lib, Transform::IDENTITY, 1.0 / 30.0, false);
        fn all(n: &NodeInst, out: &mut Vec<(Vec3, Vec3)>) {
            if n.cfg.as_ref().is_some_and(|c| c["appearance"]["type"].as_i64() == Some(10000)) {
                out.extend(n.particles.iter().map(|p| (p.pos, p.dir * p.speed)));
            }
            for c in &n.children {
                all(c, out);
            }
        }
        let mut ps = Vec::new();
        all(&n, &mut ps);
        assert_eq!(ps.len(), 150);
        for (pos, v) in &ps {
            let r = pos.xz().length();
            assert!(r > 2.4 && r < 5.0 + 22.6 / 30.0 && pos.y.abs() <= 1.25 + 22.6 / 30.0 * 0.2, "{pos}");
            // Within one random turn (11.25 deg) of level and outward.
            let tilt = 11.25f32.to_radians().sin() + 1e-3;
            assert!(v.y.abs() <= v.length() * tilt && v.length() >= 10.0 - 1e-3 && v.length() <= 22.5 + 0.2, "{v}");
            assert!(v.xz().normalize().dot(pos.xz().normalize()) > 0.97, "{pos} {v}");
        }
        for _ in 0..30 {
            step_node(&mut n, &mut lib, Transform::IDENTITY, 1.0 / 30.0, false);
        }
        let mut ps = Vec::new();
        all(&n, &mut ps);
        assert!(ps.len() >= 150 * 29 && ps.len() <= 150 * 31, "{}", ps.len());
    }

    /// The effect's time rate (root Unk10500 rateOfTime): 650680 runs at 1.375, a missing one at 1.
    #[test]
    fn effects_run_at_their_time_rate() {
        assert_eq!(time_rate(&serde_json::json!({"root": {}}), 0.0), 1.0);
        assert_eq!(time_rate(&serde_json::json!({"root": {"unk10500": {"rateOfTime": 0.5}}}), 2.0), 0.5);
        if let Some(d) = FxrLib::default().get(650680) {
            assert_eq!(time_rate(&d, 0.0), 1.375);
        }
    }

    /// RectangleEmitterShape (402): points inside sizeX x sizeY on the node's XY plane.
    #[test]
    fn rectangle_emitter_stays_inside_its_size() {
        let c = serde_json::json!({"emitterShape": {"type": 402, "sizeX": 1.5, "sizeY": 0.5, "direction": 0}});
        let mut rnd = Rnd { seed: 7, k: 0 };
        for _ in 0..200 {
            let (p, d) = emit_point(&c, 0.0, &mut rnd);
            assert!(p.x.abs() <= 0.75 && p.y.abs() <= 0.25 && p.z == 0.0 && d == Vec3::Z);
        }
    }

    /// The exe's Hermite blend: both angles pi/4 is the straight line, the ends stay put, and
    /// 0 / pi/2 bend it to the ease-in / ease-out arcs (FUN_141d94960).
    #[test]
    fn hermite_matches_the_exe_shape() {
        use std::f32::consts::{FRAC_PI_2, FRAC_PI_4};
        for x in [0.0, 0.25, 0.5, 0.75, 1.0] {
            assert!((game_hermite(FRAC_PI_4, FRAC_PI_4, x) - x).abs() < 1e-6);
        }
        for (t1, t2) in [(0.0, 0.0), (FRAC_PI_2, FRAC_PI_2), (0.0, FRAC_PI_2), (FRAC_PI_2, 0.0)] {
            assert!(game_hermite(t1, t2, 0.0).abs() < 1e-6);
            assert!((game_hermite(t1, t2, 1.0) - 1.0).abs() < 1e-6);
        }
        assert!(game_hermite(0.0, FRAC_PI_2, 0.5) < 0.5);
        assert!(game_hermite(FRAC_PI_2, 0.0, 0.5) > 0.5);
    }

    /// Manual (HANDOFF 4.57): SHINOBI_FXR_IDS=<file of ids> builds and runs each effect for 3 s
    /// headless and lists those that fail to load / build, panic, emit nothing, or use textures
    /// missing from fxr_tex. Run with --ignored --nocapture.
    #[test]
    #[ignore]
    fn zz_listed_effects_build_and_emit() {
        let Ok(list) = std::env::var("SHINOBI_FXR_IDS") else { return };
        let ids: Vec<i64> = std::fs::read_to_string(list).unwrap().split_whitespace().filter_map(|v| v.parse().ok()).collect();
        fn count(n: &NodeInst) -> usize {
            n.particles.len() + n.children.iter().map(count).sum::<usize>()
        }
        fn textures(v: &Value, out: &mut Vec<i64>) {
            match v {
                Value::Object(m) => {
                    if let Some(a) = m.get("appearance").filter(|a| a.is_object()) {
                        for k in ["texture", "normalMap", "mask", "texture1", "texture2", "texture3"] {
                            if let Some(t) = a[k].as_i64().filter(|t| *t > 0) {
                                out.push(t);
                            }
                        }
                    }
                    m.values().for_each(|c| textures(c, out));
                }
                Value::Array(a) => a.iter().for_each(|c| textures(c, out)),
                _ => {}
            }
        }
        let (mut ok, mut bad) = (0, Vec::new());
        for id in ids {
            let mut lib = FxrLib::default();
            let Some(def) = lib.get(id) else {
                bad.push(format!("{id}: no json"));
                continue;
            };
            let mut tex = Vec::new();
            textures(&def, &mut tex);
            tex.retain(|t| !crate::paths::extracted().join(format!("fxr_tex/{t}.png")).exists());
            tex.dedup();
            let run = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let mut root = make_node(&mut lib, &def["root"], 7, 0)?;
                let mut most = 0;
                for _ in 0..180 {
                    step_node(&mut root, &mut lib, Transform::IDENTITY, 1.0 / 60.0, false);
                    grow_trails(&mut root, 1.0 / 60.0);
                    most = most.max(count(&root));
                }
                Some(most)
            }));
            match run {
                Err(_) => bad.push(format!("{id}: panic")),
                Ok(None) => bad.push(format!("{id}: no root node")),
                Ok(Some(0)) => bad.push(format!("{id}: no particles in 3 s{}", if tex.is_empty() { String::new() } else { format!(", missing tex {tex:?}") })),
                Ok(Some(_)) if !tex.is_empty() => bad.push(format!("{id}: missing tex {tex:?}")),
                Ok(Some(_)) => ok += 1,
            }
        }
        eprintln!("FXRCHK ok {ok}, flagged {}", bad.len());
        for b in &bad {
            eprintln!("FXRCHK {b}");
        }
    }
}

/// Per-frame FX meshes (the batches here, ffx.rs trails) written over their GPU slab in place,
/// as cloth.rs does: Bevy 0.19 frees and re-allocates a modified mesh's slab every frame
/// (2.7 ms a frame for these, bug fixes' perf trace). Vertex counts are padded to a capacity
/// bucket (degenerate triangles far below); the Mesh asset is only replaced when the bucket
/// changes, else its interleaved bytes go to `write_fx_uploads`.
#[derive(Resource, Default)]
pub(crate) struct FxUploads {
    ups: Vec<(AssetId<Mesh>, usize, Vec<u8>)>,
    caps: HashMap<AssetId<Mesh>, usize>,
}

impl FxUploads {
    /// The padded vertex count for `n` vertices of `mesh`: the current bucket while `n` fits and
    /// fills at least a quarter of it (or it is the smallest), else 3 x the next power of two
    /// (at least 192).
    pub(crate) fn cap(&self, mesh: &Handle<Mesh>, n: usize) -> usize {
        match self.caps.get(&mesh.id()) {
            Some(&c) if n <= c && (n * 4 >= c || c == 192) => c,
            _ => 3 * n.div_ceil(3).next_power_of_two().max(64),
        }
    }

    /// Hands over `m` (padded to `cap`): an in-place write when its bucket is unchanged.
    pub(crate) fn put(&mut self, meshes: &mut Assets<Mesh>, mesh: &Handle<Mesh>, m: Mesh) {
        let n = m.count_vertices();
        // SHINOBI_FX_REALLOC=1: the old path (replace the asset every frame), for perf comparisons.
        static REALLOC: std::sync::LazyLock<bool> = std::sync::LazyLock::new(|| std::env::var("SHINOBI_FX_REALLOC").is_ok());
        if self.caps.get(&mesh.id()) == Some(&n) && !*REALLOC {
            self.ups.push((mesh.id(), m.get_vertex_size() as usize, m.create_packed_vertex_buffer_data()));
        } else if let Some(mut x) = meshes.get_mut(mesh) {
            *x = m;
            self.caps.insert(mesh.id(), n);
        }
    }
}

/// `v` padded to `cap` with `x`.
pub(crate) fn pad_to<T: Clone>(mut v: Vec<T>, cap: usize, x: T) -> Vec<T> {
    v.resize(cap.max(v.len()), x);
    v
}

#[derive(Resource, Default)]
struct FxRenderUploads(Vec<(AssetId<Mesh>, usize, Vec<u8>)>);

fn extract_fx_uploads(mut main: ResMut<bevy::render::MainWorld>, mut render: ResMut<FxRenderUploads>) {
    if let Some(mut up) = main.get_resource_mut::<FxUploads>() {
        render.0 = std::mem::take(&mut up.ups);
    }
}

fn write_fx_uploads(mut ups: ResMut<FxRenderUploads>, allocator: Res<bevy::render::mesh::allocator::MeshAllocator>, queue: Res<bevy::render::renderer::RenderQueue>) {
    for (id, stride, bytes) in ups.0.drain(..) {
        if let Some(slice) = allocator.mesh_vertex_slice(&id) {
            if (slice.range.end - slice.range.start) as usize * stride == bytes.len() {
                queue.write_buffer(slice.buffer, slice.range.start as u64 * stride as u64, &bytes);
            }
        }
    }
}
