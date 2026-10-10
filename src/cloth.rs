//! Havok cloth from the game's own `*_c.hkx` (exported by sekiro-extract next to each model as
//! extracted/model_<x>.cloth.json): the Samurai General's haori, ropes, armour skirt (kusazuri),
//! hakama and shoulder armour, Wolf's scarf and robe.
//!
//! Per cloth, every frame (the hclClothState "#01#" operator chain):
//! 1. skin the reference buffer from the animated bones (p = sum boneWorld * local);
//! 2. hclMoveParticlesOperator: fixed particles (invMass 0) jump to their reference vertex;
//! 3. hclSimulateOperator: Verlet with the cloth's gravity and damping, `substeps` x
//!    `iterations`, constraint sets in `constraintExecution` order (-1 = collisions);
//! 4. MeshMeshDeform: display vertices follow the sim triangles (p = sum triFrame * local);
//!    SimpleMeshBoneDeform: cloth-driven bones (kusazuri / hakama) follow their triangle.
//!
//! Everything runs in Havok space (game space mirrored in X, like model.rs) so the exported
//! locals apply unchanged; the world-space sim gives the cloth its inertia.
//! Collisions (-1 steps): the cloth's capsules on their bones, per-particle masks and radii.
//! Bend links solved (min / max length); gap: bend-stiffness and transition sets; transfer motion is
//! disabled in every Sekiro cloth checked (c1020, Wolf bd_m_9040), so it is not implemented;
//! globalDampingPerSecond is read as the velocity fraction removed per second.

use bevy::prelude::*;
use serde_json::Value;
use std::collections::HashMap;

struct Link {
    a: usize,
    b: usize,
    rest: f32,
    stiffness: f32,
}

enum Set {
    /// hclStandardLinkConstraintSet: keeps the rest length both ways.
    Standard(Vec<Link>),
    /// hclStretchLinkConstraintSet: only stops stretching past the rest length.
    Stretch(Vec<Link>),
    /// hclLocalRangeConstraintSet: particle stays within `max` of its skinned reference vertex,
    /// and (normal component) between minN / maxN along the reference normal.
    LocalRange { items: Vec<(usize, usize, f32, f32, f32)>, stiffness: f32, normal: bool },
    /// hclBendLinkConstraintSet: (a, b, bendMinLength, stretchMaxLength, bendStiffness,
    /// stretchStiffness) - pushed apart below the min length, pulled in above the max.
    Bend(Vec<(usize, usize, f32, f32, f32, f32)>),
    /// hclBendStiffnessConstraintSet: links (weights A..D, bendStiffness, restCurvature, particles
    /// A..D; C-D is the hinge edge, A and B the opposite corners), maxRestPoseHeightSq, clamp,
    /// useRestPoseConfig. Solved as the exe's hclBendStiffnessConstraintSetMx (FUN_141526450).
    BendStiffness { links: Vec<([f32; 4], f32, f32, [usize; 4])>, max_height_sq: f32, clamp: bool, rest_pose: bool },
    Unsolved,
}

struct Slot {
    tri: usize,
    p: Vec4,
    n: Vec4,
}

/// hclCollidable capsule (plain or tapered) on a bone: endpoints in bone space via `offset`.
struct Collidable {
    bone: String,
    offset: Mat4,
    a: Vec3,
    b: Vec3,
    ra: f32,
    rb: f32,
}

struct Display {
    mesh: usize,
    verts: Vec<(usize, Vec<Slot>)>,
}

pub struct ClothDef {
    pub name: String,
    gravity: Vec3,
    damping: f32,
    rest: Vec<Vec3>,
    inv_mass: Vec<f32>,
    radius: Vec<f32>,
    /// staticCollisionMasks: bit i = collides with collidables[i].
    masks: Vec<u32>,
    collidables: Vec<Collidable>,
    tris: Vec<[usize; 3]>,
    sets: Vec<Set>,
    execution: Vec<i32>,
    substeps: u32,
    iterations: u32,
    moves: Vec<(usize, usize)>,
    /// Reference vertex skin: (bone name, local position with w = weight, local normal).
    refs: Vec<Vec<(String, Vec4, Vec4)>>,
    displays: Vec<Display>,
    bones: Vec<(String, usize, Mat4)>,
}

fn v4(v: &Value) -> Vec4 {
    let f = |i: usize| v[i].as_f64().unwrap_or(0.0) as f32;
    Vec4::new(f(0), f(1), f(2), f(3))
}

fn us(v: &Value) -> usize {
    v.as_u64().unwrap_or(0) as usize
}

fn fl(v: &Value) -> f32 {
    v.as_f64().unwrap_or(0.0) as f32
}

fn links(s: &Value) -> Vec<Link> {
    s["links"].as_array().into_iter().flatten().map(|l| Link { a: us(&l[0]), b: us(&l[1]), rest: fl(&l[2]), stiffness: fl(&l[3]) }).collect()
}

/// Cloths of `model_<x>.bin` (its `.cloth.json`), if exported.
pub fn load(model_file: &std::path::Path) -> Vec<ClothDef> {
    let Ok(text) = std::fs::read_to_string(model_file.with_extension("cloth.json")) else { return Vec::new() };
    let Ok(j) = serde_json::from_str::<Value>(&text) else { return Vec::new() };
    let arr = |v: &Value| v.as_array().cloned().unwrap_or_default();
    arr(&j["cloths"])
        .iter()
        .map(|c| {
            let particles = arr(&c["particles"]);
            ClothDef {
                name: c["name"].as_str().unwrap_or("").to_string(),
                gravity: Vec3::new(fl(&c["gravity"][0]), fl(&c["gravity"][1]), fl(&c["gravity"][2])),
                damping: fl(&c["damping"]),
                rest: particles.iter().map(|p| v4(p).truncate()).collect(),
                inv_mass: particles.iter().map(|p| fl(&p[3])).collect(),
                radius: particles.iter().map(|p| fl(&p[4])).collect(),
                masks: arr(&c["masks"]).iter().map(|m| m.as_u64().unwrap_or(0) as u32).collect(),
                collidables: arr(&c["collidables"])
                    .iter()
                    .filter(|k| k["shape"]["kind"] == "capsule")
                    .filter_map(|k| {
                        let (sh, o) = (&k["shape"], &k["offset"]);
                        let v3 = |v: &Value| Vec3::new(fl(&v[0]), fl(&v[1]), fl(&v[2]));
                        Some(Collidable {
                            bone: k["bone"].as_str()?.to_string(),
                            offset: Mat4::from_cols(v4(&o[0]), v4(&o[1]), v4(&o[2]), v4(&o[3])),
                            a: v3(&sh["a"]),
                            b: v3(&sh["b"]),
                            ra: fl(&sh["ra"]),
                            rb: fl(&sh["rb"]),
                        })
                    })
                    .collect(),
                tris: arr(&c["tris"]).iter().map(|t| [us(&t[0]), us(&t[1]), us(&t[2])]).collect(),
                sets: arr(&c["constraints"])
                    .iter()
                    .map(|s| match s["kind"].as_str().unwrap_or("") {
                        "standard" => Set::Standard(links(s)),
                        "stretch" => Set::Stretch(links(s)),
                        "bend" => Set::Bend(
                            arr(&s["links"]).iter().map(|l| (us(&l[0]), us(&l[1]), fl(&l[2]), fl(&l[3]), fl(&l[4]), fl(&l[5]))).collect(),
                        ),
                        "bend_stiffness" => Set::BendStiffness {
                            links: arr(&s["links"])
                                .iter()
                                .map(|l| ([fl(&l[0]), fl(&l[1]), fl(&l[2]), fl(&l[3])], fl(&l[4]), fl(&l[5]), [us(&l[6]), us(&l[7]), us(&l[8]), us(&l[9])]))
                                .collect(),
                            max_height_sq: fl(&s["maxRestPoseHeightSq"]),
                            clamp: s["clamp"].as_u64().unwrap_or(0) != 0,
                            rest_pose: s["useRestPoseConfig"].as_u64().unwrap_or(0) != 0,
                        },
                        "local_range" => Set::LocalRange {
                            items: arr(&s["items"]).iter().map(|i| (us(&i[0]), us(&i[1]), fl(&i[2]), fl(&i[3]), fl(&i[4]))).collect(),
                            stiffness: fl(&s["stiffness"]),
                            normal: s["normal"].as_u64().unwrap_or(0) != 0,
                        },
                        _ => Set::Unsolved,
                    })
                    .collect(),
                execution: arr(&c["execution"]).iter().map(|e| e.as_i64().unwrap_or(-1) as i32).collect(),
                substeps: c["substeps"].as_u64().unwrap_or(1).max(1) as u32,
                iterations: c["iterations"].as_u64().unwrap_or(1).max(1) as u32,
                moves: arr(&c["move"]).iter().map(|m| (us(&m[0]), us(&m[1]))).collect(),
                refs: arr(&c["ref"])
                    .iter()
                    .map(|r| arr(r).iter().map(|e| (e[0].as_str().unwrap_or("").to_string(), v4(&e[1]), v4(&e[2]))).collect())
                    .collect(),
                displays: arr(&c["displays"])
                    .iter()
                    .map(|d| Display {
                        mesh: us(&d["mesh"]),
                        verts: arr(&d["verts"])
                            .iter()
                            .map(|v| (us(&v[0]), arr(&v[1]).iter().map(|s| Slot { tri: us(&s[0]), p: v4(&s[1]), n: v4(&s[2]) }).collect()))
                            .collect(),
                    })
                    .collect(),
                bones: arr(&c["bones"])
                    .iter()
                    .filter_map(|b| {
                        let l = &b["local"];
                        Some((b["bone"].as_str()?.to_string(), us(&b["tri"]), Mat4::from_cols(v4(&l[0]), v4(&l[1]), v4(&l[2]), v4(&l[3]))))
                    })
                    .collect(),
            }
        })
        .collect()
}

impl ClothDef {
    /// FLVER mesh indices this cloth draws.
    pub fn display_meshes(&self) -> impl Iterator<Item = usize> + '_ {
        self.displays.iter().map(|d| d.mesh)
    }
}

/// A display mesh switched from GPU skinning to per-frame CPU vertices (world space): cloth
/// vertices follow the sim, the rest are skinned here from their joints.
pub struct DisplayMesh {
    pub handle: Handle<Mesh>,
    pub mesh: usize,
    pub pos: Vec<[f32; 3]>,
    pub normal: Vec<[f32; 3]>,
    /// Bind-pose tangents (xyz, handedness w); empty without a normal map.
    pub tangent: Vec<[f32; 4]>,
    pub joints: Vec<Entity>,
    pub inv: Vec<Mat4>,
    pub idx: Vec<[u16; 4]>,
    pub weights: Vec<[f32; 4]>,
}

pub struct ClothInst {
    pub def: ClothDef,
    /// Skeleton root of the model (model space -> world for the first frame).
    pub root: Entity,
    /// Bone entities by name (animated bones and FLVER nodes).
    pub bones: HashMap<String, Entity>,
    pub displays: Vec<DisplayMesh>,
    x: Vec<Vec3>,
    prev: Vec<Vec3>,
    started: bool,
    /// Last substep length (time-corrected Verlet).
    last_h: f32,
}

impl ClothInst {
    pub fn new(def: ClothDef, root: Entity, bones: HashMap<String, Entity>) -> Self {
        ClothInst { def, root, bones, displays: Vec::new(), x: Vec::new(), prev: Vec::new(), started: false, last_h: 0.0 }
    }
}

/// All cloths of a character.
#[derive(Component, Default)]
pub struct Cloths(pub Vec<ClothInst>);

pub struct ClothPlugin;

impl Plugin for ClothPlugin {
    fn build(&self, app: &mut App) {
        // SHINOBI_NO_CLOTH=1: no simulation (visual checks of the skinned meshes underneath).
        if std::env::var("SHINOBI_NO_CLOTH").is_err() {
            app.add_systems(PostUpdate, simulate.after(bevy::transform::TransformSystems::Propagate));
        }
    }
}

/// Game space <-> Havok space: mirror X (an involution, so the same both ways).
const MIRROR: Mat4 = Mat4::from_cols(Vec4::new(-1.0, 0.0, 0.0, 0.0), Vec4::Y, Vec4::Z, Vec4::W);

fn havok(m: Mat4) -> Mat4 {
    MIRROR * m * MIRROR
}

fn mirror(v: Vec3) -> Vec3 {
    Vec3::new(-v.x, v.y, v.z)
}

/// Frame of a sim triangle (sekiro-extract cloth::tri_frame): columns v0 - c, v1 - c, their cross, c.
fn tri_frame(x: &[Vec3], t: [usize; 3]) -> Mat4 {
    let (v0, v1, v2) = (x[t[0]], x[t[1]], x[t[2]]);
    let c = (v0 + v1 + v2) / 3.0;
    let (a, b) = (v0 - c, v1 - c);
    Mat4::from_cols(a.extend(0.0), b.extend(0.0), a.cross(b).extend(0.0), c.extend(1.0))
}

/// One bend-stiffness link, as the exe (FUN_14152b490 single links; the batched SIMD path is the
/// same math): v = sum w_i p_i, plus with the rest-pose config the rest bend: the unit bisector of
/// the two triangle normals (e x (A - C), (B - C) x e; e = D - C) times hA hB restCurvature (hA, hB
/// the corners' heights over the hinge). Each particle moves by w_i invMass_i bendStiffness v
/// (bendStiffness is negative: it pulls v back to zero). With clamp, a link whose rest bend height
/// squared exceeds maxRestPoseHeightSq is skipped (stiffness 0). The stiffness factor the solver
/// passes in is 1.0 (strategy table FUN_14156d230 -> constant 1.0 in modes 0 / 1).
fn solve_bend_stiffness(x: &mut [Vec3], w: &[f32], l: &([f32; 4], f32, f32, [usize; 4]), max_height_sq: f32, clamp: bool, rest_pose: bool) {
    let (wt, stiff, rest_curv, ix) = (l.0, l.1, l.2, l.3);
    if ix.iter().any(|&i| i >= x.len()) {
        return;
    }
    let (a, b, c, d) = (x[ix[0]], x[ix[1]], x[ix[2]], x[ix[3]]);
    let mut v = a * wt[0] + b * wt[1] + c * wt[2] + d * wt[3];
    let mut k = stiff;
    if rest_pose {
        let e = d - c;
        let na = e.cross(a - c);
        let nb = (b - c).cross(e);
        let (la, lb, le2) = (na.length(), nb.length(), e.length_squared());
        let h = if le2 > 0.0 { la * lb / le2 } else { 0.0 };
        let height = h * rest_curv;
        if clamp && max_height_sq < height * height {
            k = 0.0;
        }
        let unit = |n: Vec3, l: f32| if l > 0.0 { n / l } else { Vec3::ZERO };
        v += (unit(na, la) + unit(nb, lb)).normalize_or_zero() * height;
    }
    if k == 0.0 {
        return;
    }
    for i in 0..4 {
        x[ix[i]] += v * (wt[i] * k * w[ix[i]]);
    }
}

fn solve_link(x: &mut [Vec3], w: &[f32], l: &Link, stretch_only: bool) {
    let (wa, wb) = (w[l.a], w[l.b]);
    if wa + wb <= 0.0 {
        return;
    }
    let d = x[l.b] - x[l.a];
    let len = d.length();
    if len < 1e-6 || (stretch_only && len <= l.rest) {
        return;
    }
    let corr = d * ((len - l.rest) / len * l.stiffness / (wa + wb));
    x[l.a] += corr * wa;
    x[l.b] -= corr * wb;
}

#[allow(clippy::type_complexity)]
fn simulate(
    time: Res<Time>,
    mut actors: Query<&mut Cloths>,
    mut globals: Query<&mut GlobalTransform>,
    transforms: Query<&Transform>,
    children: Query<&Children>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let dt = time.delta_secs().min(1.0 / 20.0);
    for mut cloths in &mut actors {
        for c in cloths.0.iter_mut() {
            let bone_h = |name: &str| c.bones.get(name).and_then(|&e| globals.get(e).ok()).map(|g| havok(g.to_matrix()));
            let bone_mats: HashMap<&str, Mat4> = c.def.refs.iter().flatten().map(|r| r.0.as_str()).chain(c.def.collidables.iter().map(|k| k.bone.as_str())).filter_map(|n| Some((n, bone_h(n)?))).collect();
            // Capsules in Havok world space: (a, b, ra, rb).
            let capsules: Vec<Option<(Vec3, Vec3, f32, f32)>> = c
                .def
                .collidables
                .iter()
                .map(|k| bone_mats.get(k.bone.as_str()).map(|m| { let m = *m * k.offset; (m.transform_point3(k.a), m.transform_point3(k.b), k.ra, k.rb) }))
                .collect();
            // 1. Reference buffer (Havok world space).
            let refs: Vec<Option<(Vec3, Vec3)>> = c
                .def
                .refs
                .iter()
                .map(|r| {
                    if r.is_empty() {
                        return None;
                    }
                    let (mut p, mut n) = (Vec4::ZERO, Vec4::ZERO);
                    for (b, l, ln) in r {
                        let m = bone_mats.get(b.as_str())?;
                        p += *m * *l;
                        n += *m * *ln;
                    }
                    Some((p.truncate(), n.truncate().normalize_or_zero()))
                })
                .collect();
            let def = &c.def;
            let w = &def.inv_mass;
            if c.started && debug_now(&time) {
                let root = globals.get(c.root).map(|g| havok(g.to_matrix()).w_axis.truncate()).unwrap_or_default();
                let far = c.x.iter().map(|p| p.distance(root)).fold(0.0, f32::max);
                let rf = refs.iter().flatten().map(|(q, _)| q.distance(root)).fold(0.0, f32::max);
                // How far simulated particles sit from their skinned reference (LocalRange pairs).
                let gaps: Vec<f32> = def
                    .sets
                    .iter()
                    .filter_map(|st| if let Set::LocalRange { items, .. } = st { Some(items) } else { None })
                    .flatten()
                    .filter_map(|&(p, r, ..)| refs.get(r).and_then(|v| *v).map(|(q, _)| c.x.get(p).map_or(0.0, |x| x.distance(q))))
                    .collect();
                let mean = gaps.iter().sum::<f32>() / gaps.len().max(1) as f32;
                let max = gaps.iter().copied().fold(0.0, f32::max);
                info!("cloth {}: ref gap mean {mean:.3} max {max:.3} ({} pairs)", def.name, gaps.len());
                info!("cloth {}: refs {}/{} bones {}/{} far {far:.2} ref-far {rf:.2}", def.name, refs.iter().flatten().count(), refs.len(), bone_mats.len(), def.refs.iter().flatten().map(|r| &r.0).collect::<std::collections::HashSet<_>>().len());
            }
            // First frame or a teleport (a fixed particle jumped > 1 m): restart from the bind pose
            // placed on the model root.
            // A particle count that no longer matches the definition (another cloth def took this slot)
            // restarts too.
            let jumped = c.started
                && (c.x.len() != def.rest.len() || def.moves.iter().any(|&(v, p)| refs.get(v).and_then(|r| *r).is_some_and(|(q, _)| c.x.get(p).is_none_or(|x| q.distance(*x) > 1.0))));
            let restarted = !c.started || jumped;
            if restarted {
                if jumped {
                    info!("cloth {}: restart (fixed particle jumped)", def.name);
                }
                let Ok(root) = globals.get(c.root) else { continue };
                let m = havok(root.to_matrix());
                c.x = def.rest.iter().map(|&p| m.transform_point3(p)).collect();
                c.prev = c.x.clone();
                c.started = true;
            }
            // 2. Move fixed particles: from where they were to their reference vertex, reached in
            // even parts over the substeps (the exe's Simulate lerps each fixed particle by
            // (substep + 1) / substeps before integrating that substep).
            let fixed: Vec<(usize, Vec3, Vec3)> = def
                .moves
                .iter()
                .filter_map(|&(v, p)| refs.get(v).and_then(|r| *r).filter(|_| p < c.x.len()).map(|(q, _)| (p, if restarted { q } else { c.x[p] }, q)))
                .collect();
            if dt <= 0.0 {
                for &(p, _, to) in &fixed {
                    c.x[p] = to;
                    c.prev[p] = to;
                }
            }
            // 3. Simulate.
            if dt > 0.0 {
                let h = dt / def.substeps as f32;
                let keep = (1.0 - def.damping).clamp(0.0, 1.0).powf(h);
                // Time-corrected Verlet: the last step's displacement covered last_h; scale it to
                // this step's h, or uneven frame times jitter the cloth (seen as flicker).
                let ratio = if c.last_h > 0.0 { (h / c.last_h).clamp(0.25, 4.0) } else { 1.0 };
                c.last_h = h;
                for step in 0..def.substeps {
                    let f = (step + 1) as f32 / def.substeps as f32;
                    for &(p, from, to) in &fixed {
                        c.x[p] = from.lerp(to, f);
                        c.prev[p] = c.x[p];
                    }
                    let r = if step == 0 { ratio } else { 1.0 };
                    for i in 0..c.x.len() {
                        if w[i] <= 0.0 {
                            continue;
                        }
                        let v = (c.x[i] - c.prev[i]) * keep * r;
                        c.prev[i] = c.x[i];
                        c.x[i] += v + def.gravity * h * h;
                    }
                    for _ in 0..def.iterations {
                        for &e in &def.execution {
                            if e == -1 {
                                collide(&mut c.x, w, &def.radius, &def.masks, &capsules);
                                continue;
                            }
                            let Some(set) = usize::try_from(e).ok().and_then(|e| def.sets.get(e)) else { continue };
                            match set {
                                Set::Standard(ls) => ls.iter().for_each(|l| solve_link(&mut c.x, w, l, false)),
                                Set::Stretch(ls) => ls.iter().for_each(|l| solve_link(&mut c.x, w, l, true)),
                                Set::LocalRange { items, stiffness, normal } => {
                                    for &(p, r, max, max_n, min_n) in items {
                                        let (Some(Some((q, n))), true) = (refs.get(r), p < c.x.len() && w[p] > 0.0) else { continue };
                                        let mut d = c.x[p] - *q;
                                        if *normal && *n != Vec3::ZERO {
                                            let dn = d.dot(*n);
                                            d += *n * (dn.clamp(min_n, max_n) - dn);
                                        }
                                        if d.length() > max {
                                            d = d.normalize_or_zero() * max;
                                        }
                                        let cur = c.x[p];
                                        c.x[p] = cur + (*q + d - cur) * *stiffness;
                                    }
                                }
                                Set::Bend(ls) => {
                                    for &(pa, pb, min, max, bend_k, stretch_k) in ls {
                                        let len = c.x[pb].distance(c.x[pa]);
                                        let (rest, k) = if len < min { (min, bend_k) } else if len > max { (max, stretch_k) } else { continue };
                                        solve_link(&mut c.x, w, &Link { a: pa, b: pb, rest, stiffness: k }, false);
                                    }
                                }
                                Set::BendStiffness { links, max_height_sq, clamp, rest_pose } => {
                                    links.iter().for_each(|l| solve_bend_stiffness(&mut c.x, w, l, *max_height_sq, *clamp, *rest_pose))
                                }
                                Set::Unsolved => {}
                            }
                        }
                    }
                }
            }
            // A particle more than 3 m from the model root means the sim blew up: log and restart.
            if let Ok(root) = globals.get(c.root) {
                let r = havok(root.to_matrix()).w_axis.truncate();
                if let Some((i, far)) = c.x.iter().map(|p| p.distance(r)).enumerate().max_by(|a, b| a.1.total_cmp(&b.1)).filter(|(_, d)| !(*d < 3.0)) {
                    warn!("cloth {}: particle {i} {far:.1} m from the root (inv mass {}, dt {dt:.3}); restarting", def.name, w[i]);
                    c.started = false;
                }
            }
            // 4a. Cloth-driven bones (and whatever hangs from them).
            for (name, t, local) in &def.bones {
                let (Some(&e), Some(&tri)) = (c.bones.get(name), def.tris.get(*t)) else { continue };
                let g = havok(tri_frame(&c.x, tri) * *local);
                if debug_now(&time) {
                    let old = globals.get(e).map(|g| g.to_matrix()).unwrap_or_default();
                    info!("  bone {name}: cloth {:?} anim {:?} | x {:?} vs {:?}", g.w_axis.truncate(), old.w_axis.truncate(), g.x_axis.truncate(), old.x_axis.truncate());
                }
                set_global(e, GlobalTransform::from(bevy::math::Affine3A::from_mat4(g)), &mut globals, &transforms, &children);
            }
            // 4b. Display meshes.
            let frames: Vec<Mat4> = def.tris.iter().map(|&t| tri_frame(&c.x, t)).collect();
            for dm in &c.displays {
                let Some(disp) = def.displays.iter().find(|d| d.mesh == dm.mesh) else { continue };
                let joint_m: Vec<Mat4> = dm.joints.iter().zip(&dm.inv).map(|(&j, inv)| globals.get(j).map(|g| g.to_matrix() * *inv).unwrap_or(Mat4::IDENTITY)).collect();
                let mut pos: Vec<[f32; 3]> = Vec::with_capacity(dm.pos.len());
                let mut nrm: Vec<[f32; 3]> = Vec::with_capacity(dm.pos.len());
                let mut tan: Vec<Vec3> = Vec::with_capacity(dm.pos.len());
                for (i, (p, n)) in dm.pos.iter().zip(&dm.normal).enumerate() {
                    let (mut sp, mut sn, mut st) = (Vec3::ZERO, Vec3::ZERO, Vec3::ZERO);
                    let bt = dm.tangent.get(i).map_or(Vec3::ZERO, |t| Vec3::new(t[0], t[1], t[2]));
                    for k in 0..4 {
                        let wt = dm.weights[i][k];
                        if wt <= 0.0 && k > 0 {
                            continue;
                        }
                        let m = joint_m.get(dm.idx[i][k] as usize).copied().unwrap_or(Mat4::IDENTITY);
                        sp += m.transform_point3(Vec3::from(*p)) * wt;
                        sn += m.transform_vector3(Vec3::from(*n)) * wt;
                        st += m.transform_vector3(bt) * wt;
                    }
                    pos.push(sp.into());
                    nrm.push(sn.normalize_or_zero().into());
                    tan.push(st);
                }
                for (v, slots) in &disp.verts {
                    let (mut p, mut n) = (Vec4::ZERO, Vec4::ZERO);
                    for s in slots {
                        if let Some(f) = frames.get(s.tri) {
                            p += *f * s.p;
                            n += *f * s.n;
                        }
                    }
                    if let (Some(dp), Some(dn)) = (pos.get_mut(*v), nrm.get_mut(*v)) {
                        *dp = mirror(p.truncate()).into();
                        *dn = (mirror(n.truncate()).normalize_or_zero() * if std::env::var("SHINOBI_CLOTH_FLIP_N").is_ok() { -1.0 } else { 1.0 }).into();
                    }
                }
                if debug_now(&time) {
                    let root = globals.get(c.root).map(|g| g.translation()).unwrap_or_default();
                    let far = |s: &[usize]| s.iter().map(|&i| Vec3::from(pos[i]).distance(root)).fold(0.0, f32::max);
                    let def_ix: Vec<usize> = disp.verts.iter().map(|v| v.0).filter(|&i| i < pos.len()).collect();
                    let skin_ix: Vec<usize> = (0..pos.len()).filter(|i| !def_ix.contains(i)).collect();
                    info!("  display mesh {}: cloth verts far {:.2}, skinned far {:.2}", dm.mesh, far(&def_ix), far(&skin_ix));
                }
                // Tangents: the bind tangent carried by the vertex's skin (it turns with the body),
                // made perpendicular to the deformed normal. Regenerating them every frame
                // (mikktspace on the moving cloth) flipped them across UV seams: the coat's normal
                // map then lit scattered pixels dark, changing frame to frame (the flicker).
                let tangents: Vec<[f32; 4]> = (!dm.tangent.is_empty())
                    .then(|| {
                        tan.iter()
                            .zip(&nrm)
                            .zip(&dm.tangent)
                            .map(|((t, n), bt)| {
                                let n = Vec3::from(*n);
                                let mut t = (*t - n * n.dot(*t)).normalize_or_zero();
                                if t == Vec3::ZERO {
                                    t = n.any_orthonormal_vector();
                                }
                                [t.x, t.y, t.z, bt[3]]
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                if let Some(mut mesh) = meshes.get_mut(&dm.handle) {
                    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
                    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, nrm);
                    if !tangents.is_empty() {
                        mesh.insert_attribute(Mesh::ATTRIBUTE_TANGENT, tangents);
                    }
                }
            }
        }
    }
}

/// The -1 execution step: pushes each particle out of the capsules its mask selects (capsule
/// radius tapered along the axis, plus the particle radius).
fn collide(x: &mut [Vec3], w: &[f32], radius: &[f32], masks: &[u32], capsules: &[Option<(Vec3, Vec3, f32, f32)>]) {
    for (i, p) in x.iter_mut().enumerate() {
        let mask = masks.get(i).copied().unwrap_or(0);
        if mask == 0 || w[i] <= 0.0 {
            continue;
        }
        for (k, cap) in capsules.iter().enumerate().take(32) {
            let Some((a, b, ra, rb)) = *cap else { continue };
            if mask & (1 << k) == 0 {
                continue;
            }
            let ab = b - a;
            let t = ((*p - a).dot(ab) / ab.length_squared().max(1e-8)).clamp(0.0, 1.0);
            let q = a + ab * t;
            let r = ra + (rb - ra) * t + radius.get(i).copied().unwrap_or(0.0);
            let d = *p - q;
            let len = d.length();
            if len < r && len > 1e-6 {
                *p = q + d * (r / len);
            }
        }
    }
}

/// Overrides a bone's world transform after propagation, and re-propagates to its children.
fn set_global(e: Entity, g: GlobalTransform, globals: &mut Query<&mut GlobalTransform>, transforms: &Query<&Transform>, children: &Query<&Children>) {
    if let Ok(mut gt) = globals.get_mut(e) {
        *gt = g;
    }
    if let Ok(kids) = children.get(e) {
        for &k in kids {
            if let Ok(t) = transforms.get(k) {
                set_global(k, g.mul_transform(*t), globals, transforms, children);
            }
        }
    }
}

/// SHINOBI_CLOTH_DEBUG=<seconds>: logs the cloth state once, at that time.
fn debug_now(time: &Time) -> bool {
    let Some(at) = std::env::var("SHINOBI_CLOTH_DEBUG").ok().and_then(|v| v.parse::<f32>().ok()) else { return false };
    time.elapsed_secs() > at && time.elapsed_secs() - time.delta_secs() <= at
}
