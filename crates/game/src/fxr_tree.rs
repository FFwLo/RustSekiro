//! The FXR node tree at run time: nodes, emitters, particles and property sampling.
//!
//! Adapted from FwLo's RustSekiro `src/fxr.rs` (MIT, see `CREDITS.md`), which plays effects from
//! the named tree of EvenTorset's FXR library (Unlicense; the same tree
//! [`sekiro_formats::fxr_json`] builds from our own reader). Semantics follow that library's
//! action definitions: the node tree (2000 root, 2200 basic nodes, 2001 references, 2202 levels
//! of detail), configs (1004 particles, 1005 child nodes), emitters (399 once, 300 periodic, 301
//! by distance), emitter shapes (400-405), spread (500-503), particle movement
//! (55/60/84/105/64/65), node movement (1/15/34/83/106/120/122), and properties over
//! Constant0 / ParticleAge / EmissionTime / ActiveTime with Linear / Stepped / Hermite / Bezier
//! keyframes and RandomDelta / RandomFraction / RandomRange modifiers. Hermite keyframes blend as
//! the exe does (`game_hermite`, traced by FwLo).
//!
//! The tree is data only; drawing lives in `fxr.rs`.

use bevy::prelude::*;
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

mod property;
pub use property::{Action, Field, int, prop, scalar};

// ---------------------------------------------------------------- properties

/// Deterministic per-particle randomness: the k-th random number of a seed.
#[derive(Clone, Copy)]
pub struct Rnd {
    pub seed: u32,
    pub k: u32,
}

impl Rnd {
    pub fn next(&mut self) -> f32 {
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

pub fn num(v: &Value) -> f32 {
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
    std::array::from_fn(|i| a[i] + (b[i] - a[i]) * k[i])
}

/// The game's Hermite keyframe blend (FXR property function 6): `x` is the 0..1 position between
/// the two keyframes, `t1` / `t2` the earlier keyframe's two angles (radians). Two fixed Hermite
/// curves bent into circle arcs, an ease-in `lo` and an ease-out `hi`; each angle blends from one
/// of them (angle 0) through the straight line (pi/4) to the other (pi/2), and the two results are
/// blended over x.
fn game_hermite(t1: f32, t2: f32, x: f32) -> f32 {
    const Q: f32 = std::f32::consts::FRAC_PI_4;
    let h = |m0: f32, m1: f32| {
        let (x2, x3) = (x * x, x * x * x);
        (x3 - 2.0 * x2 + x) * m0 + (-2.0 * x3 + 3.0 * x2) + (x3 - x2) * m1
    };
    let lo = 1.0 - (1.0 - h(0.7, 1.5).powi(2)).max(0.0).sqrt();
    let hi = (1.0 - (h(1.5, 0.7) - 1.0).powi(2)).max(0.0).sqrt();
    let s = if t1 <= Q {
        lo + t1 / Q * (x - lo)
    } else {
        x + (t1 - Q) / Q * (hi - x)
    };
    let e = if t2 <= Q {
        hi + t2 / Q * (x - hi)
    } else {
        x + (t2 - Q) / Q * (lo - x)
    };
    s + x * (e - s)
}

/// A property at argument `t`, with its modifiers drawn from `rnd`.
#[cfg(test)]
fn json_prop(v: Option<&Value>, t: f32, rnd: &mut Rnd, default: [f32; 4]) -> [f32; 4] {
    let Some(v) = v else { return default };
    if v.is_null() {
        return default;
    }
    let base = match v.get("function").and_then(Value::as_str) {
        Some(func) => {
            let kf = v["keyframes"]
                .as_array()
                .map(|a| a.as_slice())
                .unwrap_or(&[]);
            if kf.is_empty() {
                default
            } else {
                let last = num(&kf[kf.len() - 1]["position"]);
                let t = if v["loop"].as_bool() == Some(true) && last > 0.0 {
                    t.rem_euclid(last)
                } else {
                    t
                };
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
                                let k: [f32; 4] =
                                    std::array::from_fn(|c| game_hermite(t1[c], t2[c], x));
                                lerp4(va, vb, k)
                            }
                            "Bezier" => {
                                let (p1, p2) = (vec4(&a["p1"]), vec4(&b["p2"]));
                                let cubic = |v1: f32, c1: f32, c2: f32, v2: f32| {
                                    let u = 1.0 - x;
                                    u * u * u * v1
                                        + 3.0 * u * u * x * c1
                                        + 3.0 * u * x * x * c2
                                        + x * x * x * v2
                                };
                                std::array::from_fn(|c| {
                                    cubic(va[c], va[c] + p1[c] / 3.0, vb[c] - p2[c] / 3.0, vb[c])
                                })
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

// ---------------------------------------------------------------- game space

/// Game (FromSoft) space -> ours: X mirrored; a rotation's Y and Z angles flip with it. FXR
/// rotations are degrees, applied Z -> X -> Y.
pub fn game_vec(v: [f32; 4]) -> Vec3 {
    Vec3::new(-v[0], v[1], v[2])
}

pub fn game_rot(r: [f32; 4]) -> Quat {
    let (x, y, z) = (r[0].to_radians(), -r[1].to_radians(), -r[2].to_radians());
    Quat::from_rotation_y(y) * Quat::from_rotation_x(x) * Quat::from_rotation_z(z)
}

// ---------------------------------------------------------------- library

/// Installed effects by id: parsed with our own reader and converted to the named tree on first
/// use, plus the texture files of every effect archive.
#[derive(Default)]
pub struct FxrLib {
    files: HashMap<i64, PathBuf>,
    textures: HashMap<i64, PathBuf>,
    cache: HashMap<i64, Option<Arc<Value>>>,
    compiled: HashMap<i64, Arc<NodeDef>>,
}

impl FxrLib {
    /// Indexes every `sfxbnd_*.ffxbnd.d` folder under `<cache>/raw/sfx` (the common effects first:
    /// an id in several areas comes from the first).
    pub fn scan(cache: &std::path::Path) -> Self {
        let mut lib = FxrLib::default();
        let Ok(dirs) = std::fs::read_dir(cache.join("raw/sfx")) else {
            return lib;
        };
        let mut dirs: Vec<_> = dirs
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        dirs.sort();
        for dir in dirs {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                match path.extension().and_then(|e| e.to_str()) {
                    Some("fxr") => {
                        if let Some(id) = stem.strip_prefix('f').and_then(|s| s.parse::<i64>().ok())
                        {
                            lib.files.entry(id).or_insert(path);
                        }
                    }
                    Some("tpf") => {
                        if let Some(id) = stem.strip_prefix('s').and_then(|s| s.parse::<i64>().ok())
                        {
                            lib.textures.entry(id).or_insert(path);
                        }
                    }
                    _ => {}
                }
            }
        }
        lib
    }

    pub fn effect_count(&self) -> usize {
        self.files.len()
    }

    pub fn texture_path(&self, id: i64) -> Option<&PathBuf> {
        self.textures.get(&id)
    }

    /// The effect's tree; `None` when it is not installed or does not parse.
    pub fn get(&mut self, id: i64) -> Option<Arc<Value>> {
        if let Some(hit) = self.cache.get(&id) {
            return hit.clone();
        }
        let tree = self
            .files
            .get(&id)
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|b| sekiro_formats::fxr::parse(&b).ok())
            .map(|fxr| sekiro_formats::fxr_json::to_json(&fxr).0)
            .map(Arc::new);
        self.cache.insert(id, tree.clone());
        tree
    }

    pub fn has(&mut self, id: i64) -> bool {
        self.get(id).is_some()
    }

    pub fn compiled(&mut self, id: i64) -> Option<Arc<NodeDef>> {
        if let Some(node) = self.compiled.get(&id) {
            return Some(node.clone());
        }
        let tree = self.get(id)?;
        let node = Arc::new(NodeDef::new(&tree["root"]));
        self.compiled.insert(id, node.clone());
        Some(node)
    }

    pub fn uses_action(&mut self, id: i64, slot: &str, action: i64) -> bool {
        fn walk(lib: &mut FxrLib, node: &Value, slot: &str, action: i64, depth: usize) -> bool {
            if depth > 12 {
                return false;
            }
            if node["configs"]
                .as_array()
                .is_some_and(|cs| cs.iter().any(|c| c[slot]["type"].as_i64() == Some(action)))
            {
                return true;
            }
            if let Some(id) = node["sfx"].as_i64()
                && let Some(tree) = lib.get(id)
                && walk(lib, &tree["root"], slot, action, depth + 1)
            {
                return true;
            }
            node["nodes"]
                .as_array()
                .is_some_and(|cs| cs.iter().any(|n| walk(lib, n, slot, action, depth + 1)))
        }
        self.get(id)
            .is_some_and(|tree| walk(self, &tree["root"], slot, action, 0))
    }

    /// Whether the effect (or an effect it references) draws a model (appearance 605), which the
    /// player does not draw yet: a stand-in for the model's own subject should stay.
    pub fn has_model(&mut self, id: i64) -> bool {
        fn walk(lib: &mut FxrLib, node: &Value, depth: u32) -> bool {
            if depth > 8 {
                return false;
            }
            let own = node["configs"].as_array().is_some_and(|cs| {
                cs.iter()
                    .any(|c| c["appearance"]["type"].as_i64() == Some(605))
            });
            if own {
                return true;
            }
            if let Some(r) = node["sfx"].as_i64()
                && let Some(tree) = lib.get(r)
                && walk(lib, &tree["root"], depth + 1)
            {
                return true;
            }
            node["nodes"]
                .as_array()
                .is_some_and(|kids| kids.iter().any(|k| walk(lib, k, depth + 1)))
        }
        let Some(tree) = self.get(id) else {
            return false;
        };
        walk(self, &tree["root"], 0)
    }

    /// Installs a tree under `id` without a file (tests).
    #[cfg(test)]
    pub fn insert_for_test(&mut self, id: i64, tree: Value) {
        self.compiled.remove(&id);
        self.cache.insert(id, Some(Arc::new(tree)));
    }
}

// ---------------------------------------------------------------- runtime

pub struct Particle {
    /// World position (attachment None) or position in the node's frame.
    pub pos: Vec3,
    pub dir: Vec3,
    pub speed: f32,
    pub age: f32,
    pub life: f32,
    pub emit_t: f32,
    pub seed: u32,
    pub attached: bool,
    /// Tracer points (world position, source axis - zero = by travel - and age at creation).
    pub trail: Vec<(Vec3, Vec3, f32)>,
    pub next_seg: f32,
    pub dead: bool,
    /// A tracer particle whose life (or effect) ended: it no longer moves or adds points, its
    /// segments age out over segmentDuration, then it goes.
    pub ended: bool,
}

pub struct NodeInst {
    /// Active config (stateConfigMap[0]).
    pub cfg: Option<Arc<Config>>,
    pub local: Transform,
    pub world: Transform,
    pub time: f32,
    pub delay: f32,
    pub duration: f32,
    emit_next: f32,
    emissions: i64,
    last_emit_at: Option<Vec3>,
    pub particles: Vec<Particle>,
    pub children: Vec<NodeInst>,
    /// For node emitters (1005): templates to spawn per emission.
    templates: Vec<Arc<NodeDef>>,
    seed: u32,
    pub finished: bool,
    /// NodeAccelerationPartialFollow / NodeSpeedPartialFollow followFactor fell to 0: the node
    /// stays where it was in the world.
    detached: Option<Transform>,
    /// NodeTranslation (15): the offset applied so far.
    offset: Vec3,
}

pub struct Config {
    kind: Option<i64>,
    node_attributes: Action,
    node_transform: Action,
    node_movement: Action,
    node_selector: Action,
    emitter: Action,
    emitter_shape: Action,
    direction_spread: Action,
    particle_attributes: Action,
    pub particle_modifier: Action,
    particle_movement: Action,
    pub appearance: Action,
}

impl Config {
    fn new(c: &Value) -> Self {
        Self {
            kind: c["type"].as_i64(),
            node_attributes: Action::new(&c["nodeAttributes"]),
            node_transform: Action::new(&c["nodeTransform"]),
            node_movement: Action::new(&c["nodeMovement"]),
            node_selector: Action::new(&c["nodeSelector"]),
            emitter: Action::new(&c["emitter"]),
            emitter_shape: Action::new(&c["emitterShape"]),
            direction_spread: Action::new(&c["directionSpread"]),
            particle_attributes: Action::new(&c["particleAttributes"]),
            particle_modifier: Action::new(&c["particleModifier"]),
            particle_movement: Action::new(&c["particleMovement"]),
            appearance: Action::new(&c["appearance"]),
        }
    }
}

pub struct NodeDef {
    kind: i64,
    reference: i64,
    cfg: Option<Arc<Config>>,
    children: Vec<Arc<NodeDef>>,
}

impl NodeDef {
    fn new(node: &Value) -> Self {
        let i = node["stateConfigMap"]
            .as_array()
            .and_then(|a| a.first())
            .and_then(Value::as_u64)
            .unwrap_or(0) as usize;
        let kind = node["type"].as_i64().unwrap_or(0);
        Self {
            kind,
            reference: node["sfx"].as_i64().unwrap_or(-1),
            cfg: node["configs"]
                .as_array()
                .and_then(|c| c.get(i))
                .map(|c| Arc::new(Config::new(c))),
            children: node["nodes"]
                .as_array()
                .into_iter()
                .flatten()
                .take(if kind == 2202 { 1 } else { usize::MAX })
                .map(|n| Arc::new(Self::new(n)))
                .collect(),
        }
    }
}

pub fn make_node(lib: &mut FxrLib, node: &NodeDef, seed: u32, depth: u32) -> Option<NodeInst> {
    if depth > 12 {
        return None;
    }
    // ReferenceNode: another effect's root.
    if node.kind == 2001 {
        let r = lib.compiled(node.reference)?;
        return make_node(lib, &r, seed ^ 0x55, depth + 1);
    }
    let cfg = node.cfg.clone();
    let mut rnd = Rnd { seed, k: 0 };
    let (mut local, mut delay, mut duration) = (Transform::IDENTITY, 0.0, -1.0);
    if let Some(c) = &cfg {
        let attrs = &c.node_attributes;
        delay = scalar(attrs, Field::Delay, 0.0, &mut rnd, 0.0);
        duration = scalar(attrs, Field::Duration, 0.0, &mut rnd, -1.0);
        let tr = &c.node_transform;
        let mut off = tr[Field::Offset].vector;
        let mut rot = tr[Field::Rotation].vector;
        if tr[Field::Type].as_i64() == Some(36) {
            let (ov, rv) = (
                tr[Field::OffsetVariance].vector,
                tr[Field::RotationVariance].vector,
            );
            for i in 0..3 {
                off[i] += (rnd.next() * 2.0 - 1.0) * ov[i];
                rot[i] += (rnd.next() * 2.0 - 1.0) * rv[i];
            }
        }
        // The pitch sign of a node's authored rotation is the inverse of `game_rot`'s: designers
        // rotate +90 about X to stand a Z-axis emitter upright (64 of the rising disc/point
        // nodes in the common effects), and a +X rotation that sent Z downward would bury the
        // brazier sparks (+45) and the ground-fire smoke (+90) in the floor.
        rot[0] = -rot[0];
        local = Transform::from_translation(game_vec(off)).with_rotation(game_rot(rot));
    }
    let is_node_emitter = cfg.as_ref().is_some_and(|c| c.kind == Some(1005));
    let kids = &node.children;
    let (children, templates) = if is_node_emitter {
        (Vec::new(), kids.clone())
    } else {
        (
            kids.iter()
                .enumerate()
                .filter_map(|(i, k)| {
                    make_node(
                        lib,
                        k,
                        seed.wrapping_mul(31).wrapping_add(i as u32 + 1),
                        depth + 1,
                    )
                })
                .collect(),
            Vec::new(),
        )
    };
    Some(NodeInst {
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
fn emit_point(c: &Config, t: f32, rnd: &mut Rnd, frame: Quat) -> (Vec3, Vec3) {
    let shape = &c.emitter_shape;
    let st = shape[Field::Type].as_i64().unwrap_or(400);
    let dirmode = int(shape, Field::Direction, 0);
    let s = |k: Field, d: f32, rnd: &mut Rnd| scalar(shape, k, t, rnd, d);
    let (pos, outward) = match st {
        401 => {
            let r = s(Field::Radius, 1.0, rnd) * rnd.next().sqrt();
            let a = rnd.next() * std::f32::consts::TAU;
            let p = Vec3::new(a.cos() * r, a.sin() * r, 0.0);
            (p, p.normalize_or(Vec3::Z))
        }
        403 => {
            let r = s(Field::Radius, 1.0, rnd);
            let d = Vec3::new(
                rnd.next() * 2.0 - 1.0,
                rnd.next() * 2.0 - 1.0,
                rnd.next() * 2.0 - 1.0,
            )
            .normalize_or(Vec3::Y);
            let inside = shape[Field::EmitInside].as_bool().unwrap_or(true);
            (d * if inside { r * rnd.next().cbrt() } else { r }, d)
        }
        404 => {
            let (x, y, z) = (
                s(Field::SizeX, 1.0, rnd),
                s(Field::SizeY, 1.0, rnd),
                s(Field::SizeZ, 1.0, rnd),
            );
            let p = Vec3::new(
                (rnd.next() - 0.5) * x,
                (rnd.next() - 0.5) * y,
                (rnd.next() - 0.5) * z,
            );
            (game_vec([p.x, p.y, p.z, 0.0]), Vec3::Z)
        }
        402 => {
            let (x, y) = (s(Field::SizeX, 1.0, rnd), s(Field::SizeY, 1.0, rnd));
            (
                Vec3::new((rnd.next() - 0.5) * x, (rnd.next() - 0.5) * y, 0.0),
                Vec3::Z,
            )
        }
        405 => {
            let (r, h) = (s(Field::Radius, 1.0, rnd), s(Field::Height, 1.0, rnd));
            let a = rnd.next() * std::f32::consts::TAU;
            let inside = shape[Field::EmitInside].as_bool().unwrap_or(true);
            let rr = if inside { r * rnd.next().sqrt() } else { r };
            let y_axis = shape[Field::YAxis].as_bool().unwrap_or(true);
            let radial = Vec3::new(a.cos(), 0.0, a.sin());
            let along = (rnd.next() - 0.5) * h;
            if y_axis {
                (radial * rr + Vec3::Y * along, radial)
            } else {
                (
                    Vec3::new(a.cos() * rr, a.sin() * rr, along),
                    Vec3::new(a.cos(), a.sin(), 0.0),
                )
            }
        }
        _ => (Vec3::ZERO, Vec3::Z),
    };
    // InitialDirection (reference/fxr, as FwLo's RustSekiro reads it): 0 by shape, 1 / 2 global
    // up / down, 3 global north, 4 / 5 local up / down, 6 local north (the node's +Z). The
    // particle direction is in the node's frame, so a global axis is turned back by its rotation.
    // 6 was world up until #60: the Outskirts blowing snow 839542 / 839543 (8.3 m/s along the
    // region's yaw) then shot straight up, and every 6-driven fog bank rose instead of drifting.
    let base = match dirmode {
        1 => frame.inverse() * Vec3::Y,
        2 => frame.inverse() * Vec3::NEG_Y,
        3 => frame.inverse() * Vec3::Z,
        4 => Vec3::Y,
        5 => Vec3::NEG_Y,
        6 => Vec3::Z,
        _ => outward,
    };
    let sp = &c.direction_spread;
    let dir = match sp[Field::Type].as_i64().unwrap_or(500) {
        501 => spread_dir(base, scalar(sp, Field::Angle, t, rnd, 30.0), rnd),
        502 | 503 => spread_dir(
            base,
            scalar(sp, Field::AngleX, t, rnd, 30.0).max(scalar(sp, Field::AngleY, t, rnd, 30.0)),
            rnd,
        ),
        _ => base,
    };
    (pos, dir)
}

/// Advances one node (and its children) by `dt`; `parent` is the parent's world transform.
pub fn step_node(n: &mut NodeInst, lib: &mut FxrLib, parent: Transform, dt: f32, stop: bool) {
    let previous = n.world.translation;
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
    if let Some(c) = n.cfg.clone() {
        if c.appearance[Field::Type].as_i64() == Some(10000) {
            weather(n, &c.appearance, dt, alive, previous);
            n.time += dt;
            n.finished = !alive && n.particles.is_empty();
            return;
        }
        let mv = &c.node_movement;
        let mut rnd = Rnd {
            seed: n.seed ^ 0xabc,
            k: 0,
        };
        match mv[Field::Type].as_i64() {
            Some(1) | Some(106) | Some(122) | Some(83) | Some(120) => {
                let ty = mv[Field::Type].as_i64();
                let v = if ty == Some(120) {
                    scalar(mv, Field::SpeedZ, 0.0, &mut rnd, 0.0)
                        * scalar(mv, Field::SpeedMultiplierZ, active_t, &mut rnd, 1.0)
                } else {
                    scalar(mv, Field::SpeedZ, 0.0, &mut rnd, 0.0)
                        + scalar(mv, Field::AccelerationZ, active_t, &mut rnd, 0.0)
                            * scalar(mv, Field::AccelerationMultiplierZ, active_t, &mut rnd, 1.0)
                            * active_t
                };
                // 83: a random turn of up to maxTurnAngle every turnInterval s.
                let every = scalar(mv, Field::TurnInterval, active_t, &mut rnd, 0.0);
                if ty == Some(83)
                    && every > 0.0
                    && ((active_t / every).floor() > ((active_t - dt) / every).floor())
                {
                    let max = scalar(mv, Field::MaxTurnAngle, active_t, &mut rnd, 0.0).to_radians();
                    let mut tr = Rnd {
                        seed: n.seed ^ ((active_t / every) as u32).wrapping_mul(0x9e37),
                        k: 0,
                    };
                    let (yaw, pitch) =
                        ((tr.next() * 2.0 - 1.0) * max, (tr.next() * 2.0 - 1.0) * max);
                    n.local.rotation *= Quat::from_euler(EulerRot::YXZ, yaw, pitch, 0.0);
                }
                let fwd = n.local.rotation * Vec3::Z;
                n.local.translation += fwd * v * dt;
                n.local.translation.y -=
                    scalar(mv, Field::AccelerationY, active_t, &mut rnd, 0.0) * active_t * dt;
            }
            Some(15) => {
                let o = game_vec(prop(
                    mv.get(Field::Translation),
                    active_t,
                    &mut rnd,
                    [0.0; 4],
                ));
                n.local.translation += o - n.offset;
                n.offset = o;
            }
            Some(34) => {
                let s =
                    |k: Field, rnd: &mut Rnd| scalar(mv, k, active_t, rnd, 0.0).to_radians() * dt;
                let (x, y, z) = (
                    s(Field::AngularSpeedX, &mut rnd),
                    -s(Field::AngularSpeedY, &mut rnd),
                    -s(Field::AngularSpeedZ, &mut rnd),
                );
                n.local.rotation *= Quat::from_euler(EulerRot::YXZ, y, x, z);
            }
            _ => {}
        }
        // followFactor (106 / 122): at 0 the node lets go of its parent and stays in the world.
        if matches!(mv[Field::Type].as_i64(), Some(106) | Some(122))
            && n.detached.is_none()
            && mv.get(Field::FollowFactor).is_some()
            && scalar(mv, Field::FollowFactor, active_t, &mut rnd, 0.0) <= 0.0
        {
            n.detached = Some(parent);
        }
        n.world = match n.detached {
            Some(w) => w * n.local,
            None => parent * n.local,
        };
        if alive {
            emit(n, &c, active_t, &mut rnd, lib);
        }
        move_particles(n, &c, dt, stop, alive);
    }
    n.time += dt;
    let world = n.world;
    for ch in &mut n.children {
        step_node(ch, lib, world, dt, stop || !alive);
    }
    n.children.retain(|c| !c.finished);
    let emitting = alive && n.cfg.is_some();
    n.finished = !emitting
        && n.particles.is_empty()
        && n.children.is_empty()
        && (stop || n.duration >= 0.0 && n.time - n.delay >= n.duration || n.cfg.is_none());
}

/// CPU fallback for GPUStandardParticle. Integer durations/intervals use the FXR 30 Hz clock.
fn weather(n: &mut NodeInst, a: &Action, dt: f32, alive: bool, previous: Vec3) {
    for p in &mut n.particles {
        let mut rnd = Rnd { seed: p.seed, k: 0 };
        let follow = scalar(a, Field::ParticleFollowFactor, p.age, &mut rnd, 0.);
        if n.time > 0. {
            p.pos += (n.world.translation - previous) * follow;
        }
        p.age += dt;
        p.pos += p.dir * p.speed * dt;
    }
    n.particles.retain(|p| p.age < p.life);
    let mut rnd = Rnd {
        seed: n.seed.wrapping_add(n.emissions as u32),
        k: 0,
    };
    let interval = int(a, Field::EmissionIntervalMin, 0).max(1) as f32 / 30.;
    let authored = int(a, Field::EmissionParticleCount, 0).max(0) as usize;
    let thinning = pool_thinning(a, authored, interval);
    let mut guard = 0;
    while alive && n.emit_next <= n.time && guard < 4 {
        // The authored count, thinned evenly over emissions when the pool cannot hold it.
        let quota = |k: i64| (k as f64 * authored as f64 * thinning).floor() as usize;
        let count = quota(n.emissions + 1) - quota(n.emissions);
        let size = game_vec(prop(a.get(Field::EmitterSize), 0., &mut rnd, [1.; 4]));
        let rotation = game_rot(prop(a.get(Field::EmitterRotation), 0., &mut rnd, [0.; 4]));
        for i in 0..count.min(WEATHER_POOL.saturating_sub(n.particles.len())) {
            let seed = n
                .seed
                .wrapping_add((n.emissions as u32).wrapping_mul(4001))
                .wrapping_add(i as u32);
            let mut r = Rnd { seed, k: 0 };
            let spread = Vec3::new(r.next() - 0.5, r.next() - 0.5, r.next() - 0.5) * size;
            let vmin = game_vec(prop(a.get(Field::ParticleSpeedMin), 0., &mut r, [0.; 4]));
            let vmax = game_vec(prop(a.get(Field::ParticleSpeedMax), 0., &mut r, [0.; 4]));
            let velocity = game_vec(prop(a.get(Field::ParticleSpeed), 0., &mut r, [0.; 4]))
                + vmin
                + (vmax - vmin) * Vec3::new(r.next(), r.next(), r.next());
            let life = int(a, Field::ParticleDuration, 30).max(1) as f32 / 30.
                * scalar(a, Field::ParticleDurationMultiplier, 0., &mut r, 1.);
            n.particles.push(Particle {
                pos: n.world.translation + rotation * spread,
                dir: velocity.normalize_or(Vec3::NEG_Y),
                speed: velocity.length(),
                age: 0.,
                life,
                emit_t: n.time,
                seed,
                attached: false,
                trail: Vec::new(),
                next_seg: 0.,
                dead: false,
                ended: false,
            });
        }
        n.emissions += 1;
        n.emit_next += interval;
        guard += 1;
    }
}

/// Particles one GPUStandardParticle node keeps on the CPU.
const WEATHER_POOL: usize = 2048;

/// The fraction of each emission to keep so the steady population (count per emission times
/// emissions per lifetime) fits [`WEATHER_POOL`]. Without it a dense emitter fills the pool in
/// its first second and then emits nothing until that batch expires together, so the effect
/// arrives in waves (camera snow 839500: 40 per 1/30 s for 6 s, 7200 wanted, #84).
fn pool_thinning(a: &Action, count: usize, interval: f32) -> f64 {
    let mut rnd = Rnd { seed: 0, k: 0 };
    let life = int(a, Field::ParticleDuration, 30).max(1) as f32 / 30.
        * scalar(a, Field::ParticleDurationMultiplier, 0., &mut rnd, 1.).max(0.);
    let steady = count as f64 * (f64::from(life) / f64::from(interval)).ceil();
    if steady > WEATHER_POOL as f64 {
        WEATHER_POOL as f64 / steady
    } else {
        1.
    }
}

fn emit(n: &mut NodeInst, c: &Config, active_t: f32, rnd: &mut Rnd, lib: &mut FxrLib) {
    let em = &c.emitter;
    let mut count = 0i64;
    let total = int(em, Field::TotalEmissions, -1);
    match em[Field::Type].as_i64().unwrap_or(399) {
        399 => {
            if n.emissions == 0 {
                count = 1;
            }
        }
        300 => {
            let interval = scalar(em, Field::Interval, active_t, rnd, 1.0).max(0.0);
            let per = scalar(em, Field::PerEmission, active_t, rnd, 1.0)
                .max(0.0)
                .round() as i64;
            let mut guard = 0;
            while n.emit_next <= active_t && guard < 8 && (total < 0 || n.emissions < total) {
                count += per;
                n.emissions += 1;
                n.emit_next += interval.max(1.0 / 60.0);
                guard += 1;
            }
        }
        301 => {
            let th = scalar(em, Field::Threshold, active_t, rnd, 0.1).max(0.01);
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
    if em[Field::Type].as_i64().unwrap_or(399) == 399 && count > 0 {
        n.emissions = 1;
    }
    let max_conc = int(em, Field::MaxConcurrent, -1);
    for i in 0..count {
        if max_conc > 0 && (n.particles.len() + n.children.len()) as i64 >= max_conc {
            break;
        }
        // A bounded pool: runaway emitters never grow without limit.
        if n.particles.len() >= 2048 {
            break;
        }
        let pseed = n
            .seed
            .wrapping_mul(2_654_435_761)
            .wrapping_add((n.emissions * 64 + i) as u32);
        let mut prnd = Rnd { seed: pseed, k: 0 };
        let active = active_t;
        let (p, d) = emit_point(c, active, &mut prnd, n.world.rotation);
        if !n.templates.is_empty() {
            // NodeEmitter: child nodes at the emission point (200 all / 201 one at random).
            let pick = if c.node_selector[Field::Type].as_i64() == Some(201) {
                let i = (prnd.next() * n.templates.len() as f32) as usize % n.templates.len();
                &n.templates[i..i + 1]
            } else {
                &n.templates[..]
            };
            for t in pick {
                if let Some(mut child) = make_node(lib, t, pseed ^ 0x77, 1) {
                    let rot = Quat::from_rotation_arc(Vec3::Z, d.normalize_or(Vec3::Z));
                    child.local = Transform::from_translation(p).with_rotation(rot) * child.local;
                    n.children.push(child);
                }
            }
            continue;
        }
        let pa = &c.particle_attributes;
        let life = scalar(pa, Field::Duration, 0.0, &mut prnd, -1.0);
        let attached = int(pa, Field::Attachment, 1) != 0;
        let world_p = n.world.transform_point(p);
        let world_d = (n.world.rotation * d).normalize_or(Vec3::Z);
        n.particles.push(Particle {
            pos: if attached { p } else { world_p },
            dir: if attached { d } else { world_d },
            speed: scalar(&c.particle_modifier, Field::Speed, active_t, &mut prnd, 0.0),
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

fn move_particles(n: &mut NodeInst, c: &Config, dt: f32, stop: bool, alive: bool) {
    let pm = &c.particle_movement;
    let pmt = pm[Field::Type].as_i64().unwrap_or(0);
    let world_rot_inv = n.world.rotation.inverse();
    for p in &mut n.particles {
        p.age += dt;
        if p.ended {
            p.dead = p.trail.is_empty();
            continue;
        }
        // A negative particle duration (-1, or the common -1/30) lives as long as its node.
        if p.life >= 0.0 && p.age > p.life || (stop || !alive) && p.life < 0.0 {
            p.ended = p.trail.len() > 1;
            p.dead = !p.ended;
            continue;
        }
        let mut r = Rnd {
            seed: p.seed ^ 0x5151,
            k: 0,
        };
        let gravity = scalar(pm, Field::Gravity, p.age, &mut r, 0.0);
        let mut vel = p.dir * p.speed;
        match pmt {
            60 | 64 | 65 => {
                let s = scalar(pm, Field::Speed, p.age, &mut r, 0.0)
                    * scalar(pm, Field::SpeedMultiplier, p.age, &mut r, 1.0);
                vel = p.dir * (p.speed + s);
            }
            55 | 84 | 105 => {
                let a = scalar(pm, Field::Acceleration, p.age, &mut r, 0.0)
                    * scalar(pm, Field::AccelerationMultiplier, p.age, &mut r, 1.0);
                p.speed += a * dt;
                vel = p.dir * p.speed;
            }
            _ => {}
        }
        // Gravity (positive = down) bends the direction.
        if gravity != 0.0 {
            let g = if p.attached {
                world_rot_inv * Vec3::NEG_Y
            } else {
                Vec3::NEG_Y
            };
            let v = vel + g * gravity * dt;
            let s = v.length();
            if s > 1e-5 {
                p.dir = v / s;
                p.speed = s - if matches!(pmt, 60 | 64 | 65) {
                    scalar(pm, Field::Speed, p.age, &mut r, 0.0)
                } else {
                    0.0
                };
            }
            vel = v;
        }
        p.pos += vel * dt;
    }
    n.particles.retain(|p| !p.dead);
}

/// Tracer segments: a new point every segmentInterval, each kept segmentDuration.
pub fn grow_trails(n: &mut NodeInst) {
    if let Some(c) = n.cfg.clone() {
        let a = &c.appearance;
        if matches!(a[Field::Type].as_i64(), Some(606) | Some(10012)) {
            let interval =
                (a[Field::SegmentInterval].as_f64().unwrap_or(0.0) as f32).max(1.0 / 60.0);
            let seg_life = (a[Field::SegmentDuration].as_f64().unwrap_or(0.0) as f32).max(0.02);
            let max_segs = a[Field::ConcurrentSegments].as_i64().unwrap_or(100).max(2) as usize;
            // TracerOrientationMode: 0 Travel (by the path and the camera, at draw time), 1 the
            // node's local Z, 2 global vertical, 3 global X, 4 cross (vertical here, X added at
            // draw), 5 the global diagonal.
            let axis = match int(a, Field::Orientation, 0) {
                1 => n.world.rotation * Vec3::Z,
                2 | 4 => Vec3::Y,
                3 => Vec3::X,
                5 => Vec3::new(-1.0, 1.0, 1.0).normalize(),
                _ => Vec3::ZERO,
            };
            for p in &mut n.particles {
                let world = if p.attached {
                    n.world.transform_point(p.pos)
                } else {
                    p.pos
                };
                if p.ended {
                    let age = p.age;
                    p.trail.retain(|(_, _, t)| age - t <= seg_life);
                    continue;
                }
                // Points are laid every segmentInterval and stay where they were laid; the head
                // follows the particle until the next one is laid.
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
        grow_trails(ch);
    }
}

// ---------------------------------------------------------------- appearance helpers

/// The flipbook frame's corners, the next frame's and the fraction between them, each corner's UV
/// inside the frame scaled by `scale`.
pub fn frame_uvs(
    a: &Action,
    age: f32,
    rnd: &mut Rnd,
    scale: [f32; 2],
) -> ([[f32; 2]; 4], [[f32; 2]; 4], f32) {
    let cols = int(a, Field::Columns, 1).max(1) as f32;
    let total = int(a, Field::TotalFrames, 1).max(1) as f32;
    let rows = (total / cols).ceil().max(1.0);
    let fi = scalar(a, Field::FrameIndex, age, rnd, 0.0)
        + scalar(a, Field::FrameIndexOffset, age, rnd, 0.0);
    let rect = |f: f32| {
        let f = f.rem_euclid(total);
        let (cx, cy) = (f % cols, (f / cols).floor());
        [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]]
            .map(|[u, v]: [f32; 2]| [(cx + u * scale[0]) / cols, (cy + v * scale[1]) / rows])
    };
    (rect(fi.floor()), rect(fi.floor() + 1.0), fi - fi.floor())
}

/// Particle colour: color1 * color2 * color3 * the node's particleModifier colour, rgbMultiplier,
/// alphaMultiplier (HDR above 1 feeds the bloom). Lit particles (lighting -2 / 0) are drawn
/// unlit at 1/8 of their rgbMultiplier: where the game ships a lit config next to an unlit one
/// for the same node, the unlit one's rgbMultiplier is the lit one's x 0.125 in all 48 such pairs
/// (FwLo's measurement).
pub fn particle_color(
    a: &Action,
    c: &Config,
    p: &Particle,
    active_t: f32,
    rnd: &mut Rnd,
) -> [f32; 4] {
    let c1 = prop(a.get(Field::Color1), p.age, rnd, [1.0; 4]).map(|v| v.clamp(0.0, 1.0));
    let c2 = prop(a.get(Field::Color2), p.emit_t, rnd, [1.0; 4]).map(|v| v.clamp(0.0, 1.0));
    let c3 = prop(a.get(Field::Color3), p.age, rnd, [1.0; 4]);
    let pm = prop(
        c.particle_modifier.get(Field::Color),
        active_t,
        rnd,
        [1.0; 4],
    );
    let lit = matches!(a[Field::Lighting].as_i64(), Some(-2 | 0));
    let rgbm = scalar(a, Field::RgbMultiplier, active_t, rnd, 1.0) * if lit { 0.125 } else { 1.0 };
    let am = scalar(a, Field::AlphaMultiplier, active_t, rnd, 1.0);
    let k: [f32; 4] = std::array::from_fn(|i| c1[i] * c2[i] * c3[i] * pm[i]);
    let h = hue_squared([k[0], k[1], k[2]]);
    [
        h[0] * rgbm,
        h[1] * rgbm,
        h[2] * rgbm,
        (k[3] * am).clamp(0.0, 1.0),
    ]
}

/// The game's effect pixel shaders split the particle colour into its length and direction,
/// square the texture times the direction and scale by the length (FwLo's disassembly of
/// GXFfxtexture.ppo): the hue is gamma 2, the brightness linear. Here: c_i^2 / |c|, exactly the
/// shader's (texture x c/|c|)^2 x |c| for a texture of one. An earlier version multiplied by
/// sqrt 3 so a grey kept its value; the disassembly has no such factor (it made every effect
/// 1.73 times too bright, the map fires a floodlight).
fn hue_squared(c: [f32; 3]) -> [f32; 3] {
    let len = (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt();
    if len < 1e-5 {
        return [0.0; 3];
    }
    c.map(|v| v * v / len)
}

/// The appearance's camera-distance fade: hidden nearer than minDistance, opacity rising linearly
/// to 1 at minFadeDistance; the same toward maxDistance from maxFadeDistance; hidden past the hard
/// cut-offs. -1 turns each off.
pub fn view_fade(a: &Action, d: f32) -> f32 {
    let f = |k: Field| a[k].as_f64().map(|v| v as f32).unwrap_or(-1.0);
    let (min_d, min_f, max_f, max_d) = (
        f(Field::MinDistance),
        f(Field::MinFadeDistance),
        f(Field::MaxFadeDistance),
        f(Field::MaxDistance),
    );
    let (min_t, max_t) = (
        f(Field::MinDistanceThreshold),
        f(Field::MaxDistanceThreshold),
    );
    if (min_t >= 0.0 && d < min_t) || (max_t >= 0.0 && d > max_t) {
        return 0.0;
    }
    let mut k = 1.0;
    if min_d >= 0.0 && min_f >= 0.0 {
        k *= if min_f > min_d {
            ((d - min_d) / (min_f - min_d)).clamp(0.0, 1.0)
        } else if d < min_d {
            0.0
        } else {
            1.0
        };
    }
    if max_d >= 0.0 && max_f >= 0.0 {
        k *= if max_d > max_f {
            ((max_d - d) / (max_d - max_f)).clamp(0.0, 1.0)
        } else if d > max_d {
            0.0
        } else {
            1.0
        };
    }
    k
}

/// segmentSubdivision: each completed segment is split into `sub` + 1 pieces (Catmull-Rom through
/// the points), so a fast swing drawn from points a frame apart reads as an arc. Returns the points
/// with their place along the trail (0 the oldest, 1 the source).
pub fn subdivide(segs: &[(Vec3, Vec3, f32)], sub: usize, out: &mut Vec<(Vec3, Vec3, f32)>) {
    let n = segs.len();
    let total = (n - 1) as f32;
    out.clear();
    for i in 0..n - 1 {
        out.push((segs[i].0, segs[i].1, i as f32 / total));
        if i + 2 >= n {
            continue;
        }
        let (p0, p1, p2, p3) = (
            segs[i.saturating_sub(1)].0,
            segs[i].0,
            segs[i + 1].0,
            segs[i + 2].0,
        );
        for k in 1..=sub {
            let t = k as f32 / (sub + 1) as f32;
            let (t2, t3) = (t * t, t * t * t);
            let p = 0.5
                * (2.0 * p1
                    + (p2 - p0) * t
                    + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2
                    + (3.0 * p1 - p0 - 3.0 * p2 + p3) * t3);
            let axis = segs[i].1.lerp(segs[i + 1].1, t).normalize_or_zero();
            out.push((p, axis, (i as f32 + t) / total));
        }
    }
    out.push((segs[n - 1].0, segs[n - 1].1, 1.0));
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn compiled_properties_match_json_values_and_random_sequence() {
        let modifiers = json!([
            {"type": "RandomDelta", "max": [0.2, 0.3, 0.4, 0.5]},
            {"type": "RandomFraction", "max": 0.4},
            {"type": "Unknown", "max": 100.0},
            {"type": "RandomRange", "min": [-0.2], "max": [0.7]}
        ]);
        // The scalar/vector constants and curve layouts produced by fxr_json.
        let mut shapes = vec![
            Value::Null,
            json!(true),
            json!(2.5),
            json!([0.7]),
            json!([0.1, 0.3, 0.9]),
            json!({"value": [1.0, 0.5, 0.2, 0.8]}),
            json!({"function": "Linear", "keyframes": [], "modifiers": modifiers}),
        ];
        for function in ["Linear", "Stepped", "Hermite", "Bezier", "Unknown"] {
            for looping in [false, true] {
                shapes.push(json!({"function": function, "loop": looping,
                    "keyframes": [
                        {"position": 0.2, "value": [0.1, 0.4, 0.7, 1.0],
                            "t1": [0.2, 0.4, 0.6, 0.8], "t2": 1.2, "p1": [0.5, -0.2, 0.3, 1.0]},
                        {"position": 0.8, "value": [0.9, 0.2, 0.1, 0.0], "p2": 0.7},
                        {"position": 0.8, "value": [0.6]},
                        {"position": 1.4, "value": 0.0}],
                    "modifiers": modifiers}));
            }
        }
        shapes.push(json!({"value": [0.3, 0.5, 0.7, 1.0], "modifiers": modifiers}));
        for value in &shapes {
            let action = Action::new(&json!({"color": value}));
            for seed in [0, 7, u32::MAX] {
                for k in [0, 9, u32::MAX - 2] {
                    for t in [-1.0, 0.0, 0.2, 0.5, 0.8, 1.0, 1.4, 4.0] {
                        let mut old = Rnd { seed, k };
                        let mut new = old;
                        let expected = json_prop(Some(value), t, &mut old, [0.6; 4]);
                        let actual = prop(action.get(Field::Color), t, &mut new, [0.6; 4]);
                        assert_eq!(
                            actual.map(f32::to_bits),
                            expected.map(f32::to_bits),
                            "{value}, t={t}"
                        );
                        assert_eq!(new.k, old.k, "random draws changed for {value}");
                        assert_eq!(
                            prop(action.get(Field::Size), t, &mut new, [0.6; 4]),
                            [0.6; 4]
                        );
                        assert_eq!(new.next().to_bits(), old.next().to_bits());
                    }
                }
            }
        }
    }

    #[test]
    fn linear_and_hermite_keyframes_blend() {
        let mut rnd = Rnd { seed: 1, k: 0 };
        let linear = json!({"function": "Linear", "loop": false, "keyframes": [
            {"position": 0.0, "value": 0.0}, {"position": 2.0, "value": 4.0}]});
        assert!((json_prop(Some(&linear), 1.0, &mut rnd, [0.0; 4])[0] - 2.0).abs() < 1e-5);
        assert_eq!(json_prop(Some(&linear), 5.0, &mut rnd, [0.0; 4])[0], 4.0);
        // Both angles pi/4 is the straight line.
        let straight = std::f32::consts::FRAC_PI_4;
        let hermite = json!({"function": "Hermite", "loop": false, "keyframes": [
            {"position": 0.0, "value": 0.0, "t1": straight, "t2": straight},
            {"position": 1.0, "value": 1.0, "t1": straight, "t2": straight}]});
        assert!((json_prop(Some(&hermite), 0.25, &mut rnd, [0.0; 4])[0] - 0.25).abs() < 1e-4);
        assert_eq!(json_prop(None, 0.0, &mut rnd, [7.0; 4])[0], 7.0);
        assert_eq!(
            json_prop(Some(&json!([1.0, 2.0, 3.0])), 0.0, &mut rnd, [0.0; 4])[..3],
            [1.0, 2.0, 3.0]
        );
    }

    #[test]
    fn modifiers_stay_inside_their_bounds() {
        let v = json!({"value": 10.0, "modifiers": [{"type": "RandomDelta", "max": 2.0}]});
        for seed in 0..50 {
            let mut rnd = Rnd { seed, k: 0 };
            let x = json_prop(Some(&v), 0.0, &mut rnd, [0.0; 4])[0];
            assert!((8.0..=12.0).contains(&x), "{x}");
        }
    }

    #[test]
    fn a_periodic_emitter_spawns_and_expires() {
        // One node: a 0.1 s node emitting 3 particles of 0.2 s every 0.05 s.
        let tree = json!({"type": 2200, "stateConfigMap": [0], "nodes": [], "configs": [{
            "type": 1004,
            "nodeAttributes": {"type": 128, "delay": 0.0, "duration": 0.1},
            "emitter": {"type": 300, "interval": 0.05, "perEmission": 3.0, "totalEmissions": -1.0, "maxConcurrent": -1.0},
            "emitterShape": {"type": 400, "direction": 0},
            "directionSpread": {"type": 500},
            "particleAttributes": {"type": 129, "duration": 0.2, "attachment": 1},
            "particleModifier": {"type": 131, "speed": 1.0},
            "appearance": {"type": 603}}]});
        let mut lib = FxrLib::default();
        let mut node = make_node(&mut lib, &NodeDef::new(&tree), 7, 0).unwrap();
        step_node(&mut node, &mut lib, Transform::IDENTITY, 1.0 / 60.0, false);
        assert_eq!(node.particles.len(), 3);
        for _ in 0..120 {
            step_node(&mut node, &mut lib, Transform::IDENTITY, 1.0 / 60.0, false);
        }
        assert!(node.particles.is_empty());
        assert!(node.finished);
    }

    #[test]
    fn weather_falls_in_world_space_follows_camera_and_expires() {
        let tree = json!({"type":2200,"configs":[{"nodeAttributes":{"duration":-1.},"appearance":{
            "type":10000,"emissionParticleCount":2,"emitterSize":[2.,2.,2.],"particleDuration":30,
            "particleSpeed":[0.,-1.,0.],"particleFollowFactor":0.5}}],"nodes":[]});
        let mut lib = FxrLib::default();
        let mut n = make_node(&mut lib, &NodeDef::new(&tree), 7, 0).unwrap();
        step_node(&mut n, &mut lib, Transform::IDENTITY, 1. / 60., false);
        assert_eq!(n.particles.len(), 2);
        let first = n.particles[0].pos;
        step_node(
            &mut n,
            &mut lib,
            Transform::from_xyz(4., 0., 0.),
            1. / 60.,
            false,
        );
        assert!((n.particles[0].pos.x - first.x - 2.).abs() < 1e-5);
        assert!(n.particles[0].pos.y < first.y);
        for _ in 0..90 {
            step_node(
                &mut n,
                &mut lib,
                Transform::from_xyz(4., 0., 0.),
                1. / 60.,
                true,
            );
        }
        assert!(n.finished && n.particles.is_empty());
    }

    /// #84: the installed camera snow (839500) keeps falling around a camera that runs at
    /// Wolf's 6 m/s. It used to fill its 2048-particle pool in 1.7 s, then emit nothing until
    /// that batch expired at 6 s, leaving 2 s in every 6 with no flake within 5 m.
    #[test]
    fn camera_snow_falls_steadily_around_a_running_camera() {
        fn near(n: &NodeInst, at: Vec3, out: &mut usize) {
            *out += n
                .particles
                .iter()
                .filter(|p| (p.pos - at).xz().length() < 5.)
                .count();
            for c in &n.children {
                near(c, at, out);
            }
        }
        let mut lib = FxrLib::scan(&sekiro_sim::cache::root());
        let def = lib.compiled(839500).expect("camera snow 839500 installed");
        let mut n = make_node(&mut lib, &def, 1, 0).unwrap();
        let dt = 1. / 60.;
        let mut counts = Vec::new();
        for f in 0..60 * 20 {
            let at = Vec3::new(6. * f as f32 * dt, 1.6, 0.);
            step_node(&mut n, &mut lib, Transform::from_translation(at), dt, false);
            if f >= 60 * 8 && f % 6 == 0 {
                let mut c = 0;
                near(&n, at, &mut c);
                counts.push(c);
            }
        }
        let mean = counts.iter().sum::<usize>() as f32 / counts.len() as f32;
        let (min, max) = (counts.iter().min().unwrap(), counts.iter().max().unwrap());
        assert!(mean > 50., "mean {mean}");
        assert!(
            *min as f32 > 0.6 * mean && (*max as f32) < 1.4 * mean,
            "near counts {min}..{max}, mean {mean}"
        );
    }

    /// #60: InitialDirection 6 is the node's local +Z and 1 is world up, whatever the node's
    /// turn. A node yawed 90 degrees and pitched sends its 6 along its own Z and its 1 straight up.
    #[test]
    fn initial_direction_six_is_local_north_and_one_is_global_up() {
        let mut rnd = Rnd { seed: 3, k: 0 };
        let frame = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2) * Quat::from_rotation_x(0.3);
        let world = |dir: i64, rnd: &mut Rnd| {
            let tree = json!({"type":2200,"configs":[{"appearance":{"type":603},
                "emitterShape":{"type":400,"direction":dir}}],"nodes":[]});
            let node = NodeDef::new(&tree);
            let c = node.cfg.as_ref().unwrap();
            frame * emit_point(c, 0.0, rnd, frame).1
        };
        assert!(world(6, &mut rnd).distance(frame * Vec3::Z) < 1e-5);
        assert!(world(1, &mut rnd).distance(Vec3::Y) < 1e-5);
        assert!(world(2, &mut rnd).distance(Vec3::NEG_Y) < 1e-5);
        assert!(world(3, &mut rnd).distance(Vec3::Z) < 1e-5);
        assert!(world(4, &mut rnd).distance(frame * Vec3::Y) < 1e-5);
    }

    /// #60: the Outskirts blowing snow 839543 (start 3, beside the stone wall) blows its 8.3 m/s
    /// streaks along its node's Z, not up. With direction 6 read as world up they shot straight
    /// up at 8 to 60 m/s.
    #[test]
    fn blowing_snow_blows_sideways_not_up() {
        let mut lib = FxrLib::scan(&sekiro_sim::cache::root());
        let def = lib.compiled(839543).expect("blowing snow 839543 installed");
        let mut n = make_node(&mut lib, &def, 1, 0).unwrap();
        let dt = 1. / 30.;
        let streaks = |n: &NodeInst| {
            let s = &n.children[0].children[0];
            s.particles
                .iter()
                .map(|p| (p.seed, s.world.transform_point(p.pos)))
                .collect::<HashMap<_, _>>()
        };
        for _ in 0..15 {
            step_node(&mut n, &mut lib, Transform::IDENTITY, dt, false);
        }
        let before = streaks(&n);
        step_node(&mut n, &mut lib, Transform::IDENTITY, dt, false);
        let after = streaks(&n);
        let v: Vec<Vec3> = after
            .iter()
            .filter_map(|(k, p)| before.get(k).map(|q| (*p - *q) / dt))
            .collect();
        assert!(v.len() > 20, "{} streaks", v.len());
        let mean = v.iter().sum::<Vec3>() / v.len() as f32;
        assert!(
            (7.0..10.0).contains(&mean.length()) && mean.xz().length() > 2.0 * mean.y.abs(),
            "mean streak velocity {mean:?}"
        );
    }
}
