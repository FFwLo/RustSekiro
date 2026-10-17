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
//! Bend links solved (min / max length), bend stiffness as the exe's Mx solver; gap: transition sets; transfer motion is
//! disabled in every Sekiro cloth checked (c1020, Wolf bd_m_9040), so it is not implemented;
//! globalDampingPerSecond is read as the velocity fraction removed per second, of the velocity
//! relative to the model: the sim runs in model space (`carry`, the exe's pose space).

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
    /// hclCompressibleLinkConstraintSet: (link, compressionLength) - free between the compression
    /// and the rest length, pulled back to rest above it and pushed out to it below (exe
    /// FUN_1414d62e0 / the Mx FUN_141519460: a dead zone [compressionLength, restLength]).
    Compressible(Vec<(Link, f32)>),
    /// hclVolumeConstraintMx (exe FUN_141571de0): shape matching. `frames` = (frameVector,
    /// particle, weight), `apply` = (frameVector, particle, stiffness).
    Volume { frames: Vec<(Vec3, usize, f32)>, apply: Vec<(Vec3, usize, f32)> },
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
    /// ParticleData friction: the share of the tangential motion against a collidable removed.
    friction: Vec<f32>,
    /// staticCollisionMasks: bit i = collides with collidables[i].
    masks: Vec<u32>,
    collidables: Vec<Collidable>,
    tris: Vec<[usize; 3]>,
    sets: Vec<Set>,
    execution: Vec<i32>,
    substeps: u32,
    iterations: u32,
    moves: Vec<(usize, usize)>,
    /// Reference vertex skin: (index into `ref_bones`, local position with w = weight, local
    /// normal). Bone names resolved to indices once at load: the fur's thousands of entries
    /// hashed their bone's name every frame (0.5 ms a cloth).
    refs: Vec<Vec<(usize, Vec4, Vec4)>>,
    /// The bones the reference vertices skin from, each once.
    ref_bones: Vec<String>,
    /// References taken from an earlier cloth of the same character instead of bones (the Owl's
    /// coat feathers c5060_coat#1 ride the coat sim): that cloth's name and, per reference vertex,
    /// its triangle slots there (cloth_export.rs "ref_deform").
    ref_deform: Option<(String, Vec<(usize, Vec<Slot>)>)>,
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
            let mut ref_bones: Vec<String> = Vec::new();
            ClothDef {
                name: c["name"].as_str().unwrap_or("").to_string(),
                gravity: Vec3::new(fl(&c["gravity"][0]), fl(&c["gravity"][1]), fl(&c["gravity"][2])),
                damping: fl(&c["damping"]),
                rest: particles.iter().map(|p| v4(p).truncate()).collect(),
                inv_mass: particles.iter().map(|p| fl(&p[3])).collect(),
                radius: particles.iter().map(|p| fl(&p[4])).collect(),
                friction: particles.iter().map(|p| fl(&p[5])).collect(),
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
                        "compressible" => Set::Compressible(
                            arr(&s["links"]).iter().map(|l| (Link { a: us(&l[0]), b: us(&l[1]), rest: fl(&l[2]), stiffness: fl(&l[4]) }, fl(&l[3]))).collect(),
                        ),
                        "volume" => {
                            let rows = |k: &str| -> Vec<(Vec3, usize, f32)> {
                                arr(&s[k]).iter().map(|r| (Vec3::new(fl(&r[0]), fl(&r[1]), fl(&r[2])), us(&r[3]), fl(&r[4]))).collect()
                            };
                            Set::Volume { frames: rows("frames"), apply: rows("apply") }
                        }
                        _ => Set::Unsolved,
                    })
                    .collect(),
                // An empty constraintExecution (the Chained Ogre's four fur sims, c1070's hair,
                // c1190's mino, c5020 / c5021's hair, c5050's beard, c5060's coat: 15 cloths) is
                // Havok's "every set in its stored order", collision last; read literally it solved
                // nothing and the fur flew off ("particle 8 4.5 m from the root", restart every
                // few frames).
                execution: {
                    let e: Vec<i32> = arr(&c["execution"]).iter().map(|e| e.as_i64().unwrap_or(-1) as i32).collect();
                    let n_sets = arr(&c["constraints"]).len() as i32;
                    if e.is_empty() && n_sets > 0 { (0..n_sets).chain([-1]).collect() } else { e }
                },
                substeps: c["substeps"].as_u64().unwrap_or(1).max(1) as u32,
                iterations: c["iterations"].as_u64().unwrap_or(1).max(1) as u32,
                // Pairs beyond the particle count (an older export's second sim cloth) are
                // dropped: as "jumped" fixed particles they restarted the cloth every frame.
                moves: arr(&c["move"]).iter().map(|m| (us(&m[0]), us(&m[1]))).filter(|&(_, p)| p < arr(&c["particles"]).len()).collect(),
                refs: arr(&c["ref"])
                    .iter()
                    .map(|r| {
                        arr(r)
                            .iter()
                            .map(|e| {
                                let name = e[0].as_str().unwrap_or("");
                                let b = ref_bones.iter().position(|n| n == name).unwrap_or_else(|| {
                                    ref_bones.push(name.to_string());
                                    ref_bones.len() - 1
                                });
                                (b, v4(&e[1]), v4(&e[2]))
                            })
                            .collect()
                    })
                    .collect(),
                ref_bones: ref_bones.clone(),
                ref_deform: c["ref_deform"]["cloth"].as_str().map(|src| {
                    let verts = arr(&c["ref_deform"]["verts"])
                        .iter()
                        .map(|v| (us(&v[0]), arr(&v[1]).iter().map(|s| Slot { tri: us(&s[0]), p: v4(&s[1]), n: v4(&s[2]) }).collect()))
                        .collect();
                    (src.to_string(), verts)
                }),
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
    /// The mesh's entity (model.rs): a cloth whose meshes are all hidden (another kimono of the
    /// Folding Screen Monkeys, NpcParam modelDispMask) is carried rigidly, not simulated.
    pub entity: Entity,
    pub mesh: usize,
    pub pos: Vec<[f32; 3]>,
    pub normal: Vec<[f32; 3]>,
    /// Bind-pose tangents (xyz, handedness w); empty without a normal map.
    pub tangent: Vec<[f32; 4]>,
    pub joints: Vec<Entity>,
    pub inv: Vec<Mat4>,
    pub idx: Vec<[u16; 4]>,
    pub weights: Vec<[f32; 4]>,
    /// The mesh's triangles, for the cloths whose deform carries no normals (`rebuild_normals`).
    pub tris: Vec<[u32; 3]>,
}

/// hclUpdateAllVertexFramesOperator: the position-only MeshMeshDeform (Wolf's coat
/// bodybese01 / 02 / 03, c7400's haori; their display normals export as 0) leaves the normals to
/// this operator, which rebuilds them from the deformed triangles. Here: the area-weighted
/// triangle normal of each such vertex, turned to the side of its skinned normal (the mirrored
/// winding makes the cross product's side unreliable).
/// MeshMeshDeform: the display vertices follow the sim triangles (p = sum triFrame * local), then
/// the normals the deform does not carry are rebuilt (`rebuild_normals`).
fn deform_display(disp: &Display, frames: &[Mat4], tris: &[[u32; 3]], pos: &mut [[f32; 3]], nrm: &mut [[f32; 3]]) {
    let mut no_normal: Vec<usize> = Vec::new();
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
            let n = mirror(n.truncate()).normalize_or_zero();
            if n == Vec3::ZERO {
                no_normal.push(*v);
            } else {
                *dn = n.into();
            }
        }
    }
    rebuild_normals(tris, pos, nrm, &no_normal);
}

fn rebuild_normals(tris: &[[u32; 3]], pos: &[[f32; 3]], nrm: &mut [[f32; 3]], marked: &[usize]) {
    if marked.is_empty() {
        return;
    }
    let mut acc = vec![Vec3::ZERO; pos.len()];
    for t in tris {
        let [a, b, c] = t.map(|i| i as usize);
        if a >= pos.len() || b >= pos.len() || c >= pos.len() {
            continue;
        }
        let (pa, pb, pc) = (Vec3::from(pos[a]), Vec3::from(pos[b]), Vec3::from(pos[c]));
        let n = (pb - pa).cross(pc - pa);
        acc[a] += n;
        acc[b] += n;
        acc[c] += n;
    }
    let side: f32 = marked.iter().map(|&v| acc[v].dot(Vec3::from(nrm[v]))).sum();
    let sign = if side < 0.0 { -1.0 } else { 1.0 };
    for &v in marked {
        let n = (acc[v] * sign).normalize_or_zero();
        if n != Vec3::ZERO {
            nrm[v] = n.into();
        }
    }
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
    /// This frame's results from `simulate` (parallel over the characters), written to the
    /// world by `apply`.
    out: ClothOut,
    /// Microseconds spent on this cloth since the last "cloth cost" log, and per phase
    /// (references, restart, solve, bones, skinning, cloth vertices + tangents).
    cost_us: u64,
    phase_us: [u64; 6],
    /// Frames in a row the sim blew up (a particle > 3 m from the root) and restarted; at
    /// `GIVE_UP` the cloth is carried rigidly for good (one such cloth, c1040's fur, cost
    /// 24 ms a frame restarting).
    blowups: u32,
    rigid: bool,
    /// The model root last frame (Havok space): the sim runs in model space (`carry`).
    root_m: Option<Mat4>,
    /// Last frame's capsules (Havok space): the exe's Substep Collidables walks each collidable
    /// from its old pose to its new one over the substeps (14156.c:12749).
    caps_prev: Vec<Option<(Vec3, Vec3, f32, f32)>>,
    /// SHINOBI_CLOTH_PROBE: the sim's swing off the animated cloth (deg, the LocalRange
    /// particles' centre from the fixed particles' centre, sim vs skinned references) and the
    /// LocalRange particles' mean / max gap from their references (m).
    off_anim: (f32, f32, f32),
}

const GIVE_UP: u32 = 5;

/// What a cloth hands from the parallel step to the serial one: its driven bones' world
/// transforms and the display meshes' vertices (position, normal, tangent).
#[derive(Default)]
struct ClothOut {
    bones: Vec<(Entity, GlobalTransform)>,
    #[allow(clippy::type_complexity)]
    meshes: Vec<(Handle<Mesh>, Vec<[f32; 3]>, Vec<[f32; 3]>, Vec<[f32; 4]>)>,
}

impl ClothInst {
    pub fn new(def: ClothDef, root: Entity, bones: HashMap<String, Entity>) -> Self {
        ClothInst { def, root, bones, displays: Vec::new(), x: Vec::new(), prev: Vec::new(), started: false, last_h: 0.0, out: ClothOut::default(), cost_us: 0, phase_us: [0; 6], blowups: 0, rigid: false, root_m: None, caps_prev: Vec::new(), off_anim: (0.0, 0.0, 0.0) }
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
            app.add_systems(PostUpdate, (simulate, apply).chain().after(bevy::transform::TransformSystems::Propagate));
            app.init_resource::<ClothUploads>();
            if let Some(render_app) = app.get_sub_app_mut(bevy::render::RenderApp) {
                use bevy::render::{Render, RenderSystems};
                render_app.init_resource::<ClothUploads>();
                render_app.add_systems(bevy::render::ExtractSchedule, extract_cloth_uploads);
                render_app.add_systems(Render, write_cloth_uploads.in_set(RenderSystems::PrepareAssets).after(bevy::render::mesh::allocator::allocate_and_free_meshes));
            }
        }
        if std::env::var("SHINOBI_CAPE_TURN").is_ok() {
            app.add_systems(Update, cape_drive.after(crate::player::read_input));
            app.add_systems(Last, cape_probe);
        }
        if std::env::var("SHINOBI_CLOTH_PROBE").is_ok() {
            app.add_systems(Last, enemy_cloth_probe);
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

/// A cloth-driven bone without the shear and stretch of its triangle: the triangle frame's
/// columns are edge vectors (the exported locals carry scales of 35-39 on the Owl's Hair..Hair06),
/// so a triangle squashed in a fast move sheared and twisted the bone and everything skinned to it
/// (the Owl's ponytail balled up in his deathblow, the user 2026-10-16). The bone takes the
/// nearest rotation (polar decomposition, as a matrix becomes a Havok hkQsTransform), each axis
/// at its rest-pose length. gap: hclSimpleMeshBoneDeformOperator's output step in the exe is not
/// traced.
fn rigid_like(g: Mat4, rest: Mat4) -> Mat4 {
    let m = bevy::math::Mat3::from_mat4(g);
    if m.determinant().abs() < 1e-9 {
        return g;
    }
    let flip = m.determinant() < 0.0;
    let mut r = if flip { m * bevy::math::Mat3::from_diagonal(Vec3::new(1.0, 1.0, -1.0)) } else { m };
    for _ in 0..20 {
        let next = (r + r.inverse().transpose()) * 0.5;
        let done = (next - r).to_cols_array().iter().all(|d| d.abs() < 1e-6);
        r = next;
        if done {
            break;
        }
    }
    if flip {
        r *= bevy::math::Mat3::from_diagonal(Vec3::new(1.0, 1.0, -1.0));
    }
    let len = |v: Vec4| v.truncate().length();
    Mat4::from_cols((r.x_axis * len(rest.x_axis)).extend(0.0), (r.y_axis * len(rest.y_axis)).extend(0.0), (r.z_axis * len(rest.z_axis)).extend(0.0), g.w_axis)
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

/// A standard link's stiffness comes premultiplied with 1 / (invMassA + invMassB): every
/// Sekiro cloth checked has it so (c5060 ponytail: stretch 0.7, standard 2.1875 = 0.7 / 0.32
/// between two 0.16 particles and 4.375 = 0.7 / 0.16 to a fixed one; c1020 hakama 3.125 / 1.5625
/// at 0.32; Wolf's muffler2 0.938 / 0.469 at 1.067). A stretch link's is not (1.0 / 0.7 / 0.5 at
/// any mass). Dividing the standard one again made light cloth overshoot: the Owl's ponytail
/// (invMass 0.16) exploded standing still (the user, 2026-10-16).
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
    let k = if stretch_only { l.stiffness / (wa + wb) } else { l.stiffness };
    let corr = d * ((len - l.rest) / len * k);
    x[l.a] += corr * wa;
    x[l.b] -= corr * wb;
}

/// hclCompressibleLinkConstraintSet's link: untouched between `comp` and the rest length,
/// otherwise solved back to the nearer bound.
fn solve_compressible(x: &mut [Vec3], w: &[f32], l: &Link, comp: f32) {
    let len = x[l.b].distance(x[l.a]);
    let rest = if len > l.rest { l.rest } else if len < comp { comp } else { return };
    solve_link(x, w, &Link { a: l.a, b: l.b, rest, stiffness: l.stiffness }, false);
}

/// Cloth further than this from the camera is not simulated: it is carried rigidly in its
/// bind pose on the model root. The Options menu's "Cloth physics" (config `[video] cloth`:
/// on 15 m, near 6 m, off = none, negative here); `SHINOBI_CLOTH_RANGE=<m>` overrides it for one
/// run (0 = simulate everything). With a line-up of 20 enemies (127 cloth pieces) the
/// simulation took the sandbox from 68 to 22 fps.
/// Model-space simulation: the particles (current and previous, so their velocity relative to
/// the model is kept) move with the model root from `old` to `new` (world, Havok space).
fn carry(x: &mut [Vec3], prev: &mut [Vec3], old: Mat4, new: Mat4) {
    let d = new * old.inverse();
    for p in x.iter_mut().chain(prev.iter_mut()) {
        *p = d.transform_point3(*p);
    }
}

/// Rotation of the polar decomposition M = R S (S symmetric): R = M (M^T M)^(-1/2), as the exe's
/// FUN_141572020: eigen-decompose A = M^T M (hkMatrix3::diagonalizeSymmetric, 20 sweeps), S =
/// V sqrt(L) V^T (negative eigenvalues 0), R = M S^-1; a singular S (det exactly 0) gives zeros.
fn polar_rotation(m: Mat3) -> Mat3 {
    let mut a = m.transpose() * m;
    let mut v = Mat3::IDENTITY;
    for _ in 0..20 {
        let mut off = 0.0f32;
        for (p, q) in [(0, 1), (0, 2), (1, 2)] {
            let apq = a.col(q)[p];
            off += apq.abs();
            if apq.abs() < 1e-12 {
                continue;
            }
            let (app, aqq) = (a.col(p)[p], a.col(q)[q]);
            let theta = (aqq - app) / (2.0 * apq);
            let t = theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt());
            let t = if theta == 0.0 { 1.0 } else { t };
            let c = 1.0 / (t * t + 1.0).sqrt();
            let sn = t * c;
            // Rotation G in the (p, q) plane: columns p and q mix by (c, s).
            let mut g = Mat3::IDENTITY;
            *g.col_mut(p) = Vec3::ZERO;
            *g.col_mut(q) = Vec3::ZERO;
            g.col_mut(p)[p] = c;
            g.col_mut(p)[q] = -sn;
            g.col_mut(q)[p] = sn;
            g.col_mut(q)[q] = c;
            for k in 0..3 {
                if k != p && k != q {
                    g.col_mut(k)[k] = 1.0;
                }
            }
            a = g.transpose() * a * g;
            v *= g;
        }
        if off < 1e-9 {
            break;
        }
    }
    let root = Vec3::new(a.col(0)[0].max(0.0).sqrt(), a.col(1)[1].max(0.0).sqrt(), a.col(2)[2].max(0.0).sqrt());
    let s = v * Mat3::from_diagonal(root) * v.transpose();
    if s.determinant() == 0.0 {
        return Mat3::ZERO;
    }
    m * s.inverse()
}

/// hclVolumeConstraintMx (exe FUN_141571de0 -> FUN_14157ec10 / ec70 / 141572020 / 14157edd0):
/// c = sum w p; M = sum w (p - c) frameVector^T; R = polar rotation of M; each apply row moves its
/// particle by stiffness * (c + R frameVector - p) (no inverse mass in the exe's code, and the
/// solver stiffness factor, 1.0 here, multiplies in).
fn solve_volume(x: &mut [Vec3], frames: &[(Vec3, usize, f32)], apply: &[(Vec3, usize, f32)]) {
    let get = |i: usize| x.get(i).copied();
    let mut c = Vec3::ZERO;
    for &(_, p, w) in frames {
        let Some(q) = get(p) else { return };
        c += q * w;
    }
    let mut cols = [Vec3::ZERO; 3];
    for &(fv, p, w) in frames {
        let d = x[p] - c;
        cols[0] += d * (w * fv.x);
        cols[1] += d * (w * fv.y);
        cols[2] += d * (w * fv.z);
    }
    let r = polar_rotation(Mat3::from_cols(cols[0], cols[1], cols[2]));
    for &(fv, p, k) in apply {
        if let Some(q) = x.get_mut(p) {
            *q += (c + r * fv - *q) * k;
        }
    }
}

/// Sim steps a frame of `dt` seconds runs: the nearest count of 1/60 s steps, 1 to 4.
fn frame_steps(dt: f32) -> u32 {
    ((dt * 60.0).round() as u32).clamp(1, 4)
}

fn cloth_range(setting: crate::config::ClothSetting) -> f32 {
    static RANGE: std::sync::OnceLock<Option<f32>> = std::sync::OnceLock::new();
    RANGE.get_or_init(|| std::env::var("SHINOBI_CLOTH_RANGE").ok().and_then(|v| v.parse().ok())).unwrap_or(setting.range())
}

/// The simulation, one task per character (Bevy's parallel query: with 20 enemies in the
/// sandbox the serial loop took 68 -> 22 fps). Reads the skeletons, writes only into each
/// cloth's `out`; `apply` puts the results into the world.
#[allow(clippy::type_complexity)]
fn simulate(
    time: Res<Time>,
    mut actors: Query<&mut Cloths>,
    globals: Query<&GlobalTransform>,
    transforms: Query<&Transform>,
    children: Query<&Children>,
    cameras: Query<Entity, With<Camera3d>>,
    config: Option<Res<crate::config::GameConfig>>,
    shown: Query<&Visibility>,
) {
    let t0 = std::time::Instant::now();
    let dt = time.delta_secs().min(1.0 / 20.0);
    let range = cloth_range(config.map_or(Default::default(), |c| c.video.cloth));
    // Cost probe: SHINOBI_CLOTH_NO_MESH=1 skips the bone and display mesh outputs (step 4).
    let no_mesh = std::env::var("SHINOBI_CLOTH_NO_MESH").is_ok();
    let cam = cameras.iter().next().and_then(|e| transforms.get(e).ok()).map(|t| t.translation);
    let time = &time;
    let (globals, transforms, children, shown) = (&globals, &transforms, &children, &shown);
    actors.par_iter_mut().for_each(|mut cloths| {
        // World transforms this character's cloth-driven bones (and their children) take
        // this frame, ahead of `apply`: the display meshes skin from them.
        let mut overrides: HashMap<Entity, GlobalTransform> = HashMap::new();
        for i in 0..cloths.0.len() {
            let (before, rest) = cloths.0.split_at_mut(i);
            let c = &mut rest[0];
            let tc = std::time::Instant::now();
            c.out.bones.clear();
            c.out.meshes.clear();
            // A chained cloth's references: the earlier cloth's particles this frame through its
            // deformer slots (Havok world space, as the bone-skinned references).
            let chained: Option<Vec<Option<(Vec3, Vec3)>>> = c.def.ref_deform.as_ref().map(|(src, verts)| {
                let mut out = vec![None; c.def.refs.len()];
                let Some(s) = before.iter().find(|b| b.def.name == *src).filter(|b| b.started && b.x.len() == b.def.rest.len()) else { return out };
                let frames: Vec<Mat4> = s.def.tris.iter().map(|&t| tri_frame(&s.x, t)).collect();
                for (v, slots) in verts {
                    let (mut p, mut n) = (Vec4::ZERO, Vec4::ZERO);
                    for sl in slots {
                        if let Some(f) = frames.get(sl.tri) {
                            p += *f * sl.p;
                            n += *f * sl.n;
                        }
                    }
                    if let Some(o) = out.get_mut(*v) {
                        *o = Some((p.truncate(), n.truncate().normalize_or_zero()));
                    }
                }
                out
            });
            // Its source not running yet: carried rigidly this frame.
            let chain_missing = chained.as_ref().is_some_and(|r| r.iter().all(Option::is_none));
            // Out of range: carried rigidly (the restart pose every frame), no simulation. The
            // range is measured to the model root plus the cloth's own rest reach: the Demon of
            // Hatred's root sits 5 m behind its head, so its mane 8 m from the camera was culled
            // (bind pose floating above the crouched body) by the plain 15 m rule.
            let rest_far = c.def.rest.iter().map(|p| p.length()).fold(0.0, f32::max);
            let hidden = !c.displays.is_empty() && c.displays.iter().all(|d| shown.get(d.entity).is_ok_and(|v| *v == Visibility::Hidden));
            // Beyond the range only (not hidden, rigid or off): the cloth keeps its last
            // simulated shape and rides the root, so a boss seen from afar does not have its
            // cloth snap to the bind pose (which stands out stiff from the body).
            let far = range >= 0.0
                && match cam {
                    Some(cam) if range > 0.0 => globals.get(c.root).is_ok_and(|g| g.translation().distance(cam) > range + rest_far),
                    _ => false,
                };
            let culled = c.rigid || chain_missing || hidden || range < 0.0 || far;
            let hold = far && !c.rigid && !chain_missing && !hidden && c.started;
            let bone_h = |name: &str| c.bones.get(name).and_then(|&e| globals.get(e).ok()).map(|g| havok(g.to_matrix()));
            // One world matrix per bone: the reference bones by index, the collidables' by name.
            let ref_mats: Vec<Option<Mat4>> = c.def.ref_bones.iter().map(|n| bone_h(n)).collect();
            let bone_mats: HashMap<&str, Mat4> = c.def.collidables.iter().filter_map(|k| Some((k.bone.as_str(), bone_h(&k.bone)?))).collect();
            // Capsules in Havok world space: (a, b, ra, rb).
            let capsules: Vec<Option<(Vec3, Vec3, f32, f32)>> = c
                .def
                .collidables
                .iter()
                .map(|k| bone_mats.get(k.bone.as_str()).map(|m| { let m = *m * k.offset; (m.transform_point3(k.a), m.transform_point3(k.b), k.ra, k.rb) }))
                .collect();
            // 1. Reference buffer (Havok world space).
            let refs: Vec<Option<(Vec3, Vec3)>> = if let Some(r) = chained { r } else { c
                .def
                .refs
                .iter()
                .map(|r| {
                    if r.is_empty() {
                        return None;
                    }
                    let (mut p, mut n) = (Vec4::ZERO, Vec4::ZERO);
                    for (b, l, ln) in r {
                        let m = ref_mats.get(*b).copied().flatten()?;
                        p += m * *l;
                        n += m * *ln;
                    }
                    Some((p.truncate(), n.truncate().normalize_or_zero()))
                })
                .collect() };
            let mut tp = tc; // phase 0: the bone matrices, capsules and reference vertices above
            let mut lap = |slot: usize, tp: &mut std::time::Instant, acc: &mut [u64; 6]| {
                let now = std::time::Instant::now();
                acc[slot] += (now - *tp).as_micros() as u64;
                *tp = now;
            };
            let mut phase = c.phase_us;
            lap(0, &mut tp, &mut phase);
            let def = &c.def;
            let w = &def.inv_mass;
            if c.started && debug_now(time) {
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
                // Where the particles and the references sit (a constant offset or a mirrored
                // axis shows here as a centroid difference).
                let cx = c.x.iter().copied().sum::<Vec3>() / c.x.len().max(1) as f32;
                let (rs, rn) = refs.iter().flatten().fold((Vec3::ZERO, 0), |(s, n), (q, _)| (s + *q, n + 1));
                let fixed_c = def.moves.iter().filter_map(|&(_, p)| c.x.get(p)).copied().sum::<Vec3>() / def.moves.len().max(1) as f32;
                let fixed_r = def.moves.iter().filter_map(|&(v, _)| refs.get(v).and_then(|r| *r)).map(|(q, _)| q).sum::<Vec3>() / def.moves.len().max(1) as f32;
                info!("cloth {}: centroid particles {:?} refs {:?} | fixed particles {:?} their refs {:?} (root {:?})", def.name, cx, rs / rn.max(1) as f32, fixed_c, fixed_r, root);
                let cam_d = cam.and_then(|cam| globals.get(c.root).ok().map(|g| g.translation().distance(cam))).unwrap_or(-1.0);
                info!("cloth {}: refs {}/{} bones {}/{} far {far:.2} ref-far {rf:.2} rest-far {rest_far:.2} camera {cam_d:.1} m (range {range}, culled {culled})", def.name, refs.iter().flatten().count(), refs.len(), ref_mats.iter().flatten().count(), def.ref_bones.len());
                // The reference bones by name with where they are (Havok space), to compare with
                // the exporter's bind pose when the references sit off the particles.
                let where_: Vec<String> = def.ref_bones.iter().zip(&ref_mats).map(|(n, m)| format!("{n} {:?}", m.map(|m| m.w_axis.truncate()))).collect();
                info!("cloth {}: ref bones {}", def.name, where_.join("; "));
            }
            // First frame or a teleport (a fixed particle jumped > 1 m): restart from the bind pose
            // placed on the model root.
            // A particle count that no longer matches the definition (another cloth def took this slot)
            // restarts too.
            // The jump that counts as a teleport scales with the cloth (half its bind-pose
            // extent, 1 m at least): the Guardian Ape's fur, 4 m across, moved its fixed
            // particles more than 1 m in a frame just by turning and restarted every few frames.
            let jump = (0.5 * def.rest.iter().map(|p| p.length()).fold(0.0, f32::max)).max(1.0);
            let jumped = c.started
                && !culled
                && (c.x.len() != def.rest.len() || def.moves.iter().any(|&(v, p)| refs.get(v).and_then(|r| *r).is_some_and(|(q, _)| c.x.get(p).is_none_or(|x| q.distance(*x) > jump))));
            let restarted = !c.started || jumped || (culled && !hold);
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
            // The game simulates in model space (the pose buffers'): the character's motion reaches
            // the particles only through Transfer Motion (FUN_141578d50, the root's delta blended
            // onto current and previous positions), off in all 435 exported cloths. So the
            // particles ride along with the model root (no inertia, and the damping acts on motion
            // relative to it, not as air drag: Wolf's 0.99 trailed his cape ~50 deg in a run).
            if let Ok(root) = globals.get(c.root) {
                let m = havok(root.to_matrix());
                if let Some(old) = c.root_m.filter(|_| !restarted) {
                    carry(&mut c.x, &mut c.prev, old, m);
                    let d = m * old.inverse();
                    for (a, b, _, _) in c.caps_prev.iter_mut().flatten() {
                        (*a, *b) = (d.transform_point3(*a), d.transform_point3(*b));
                    }
                }
                c.root_m = Some(m);
            }
            // 2. Move fixed particles: from where they were to their reference vertex, reached in
            // even parts over the substeps (the exe's Simulate lerps each fixed particle by
            // (substep + 1) / substeps before integrating that substep).
            let fixed: Vec<(usize, Vec3, Vec3)> = def
                .moves
                .iter()
                .filter_map(|&(v, p)| refs.get(v).and_then(|r| *r).filter(|_| p < c.x.len()).map(|(q, _)| (p, if restarted { q } else { c.x[p] }, q)))
                .collect();
            if dt <= 0.0 || hold {
                for &(p, _, to) in &fixed {
                    c.x[p] = to;
                    c.prev[p] = to;
                }
            }
            lap(1, &mut tp, &mut phase);
            // 3. Simulate.
            if dt > 0.0 && !culled {
                // The game steps its cloth once per frame, and it is locked to 60 fps (the
                // update-time mode table FUN_141045c40 = [frame dt, 1/30, 1/60]); constraint
                // stiffness acts per step (bend stiffness, local range), so a slower frame must
                // not be one long step: it runs as several ~1/60 s steps (at most 3: dt is
                // capped at 1/20). Faster frames stay one step. gap: the game's behaviour
                // under its own frame drops (frame dt there) is not reproduced.
                let frame_steps = frame_steps(dt);
                let total = def.substeps * frame_steps;
                let h = dt / total as f32;
                let keep = (1.0 - def.damping).clamp(0.0, 1.0).powf(h);
                // Time-corrected Verlet: the last step's displacement covered last_h; scale it to
                // this step's h, or uneven frame times jitter the cloth (seen as flicker).
                let ratio = if c.last_h > 0.0 { (h / c.last_h).clamp(0.25, 4.0) } else { 1.0 };
                c.last_h = h;
                if restarted || c.caps_prev.len() != capsules.len() {
                    c.caps_prev = capsules.clone();
                }
                let swept = |f: f32| -> Vec<Option<(Vec3, Vec3, f32, f32)>> {
                    capsules
                        .iter()
                        .zip(&c.caps_prev)
                        .map(|(now, was)| now.map(|(a, b, ra, rb)| match was { Some((a0, b0, _, _)) => (a0.lerp(a, f), b0.lerp(b, f), ra, rb), None => (a, b, ra, rb) }))
                        .collect()
                };
                for step in 0..total {
                    let f = (step + 1) as f32 / total as f32;
                    let (caps, caps_was) = (swept(f), swept(step as f32 / total as f32));
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
                                if *SKIP == "collide" {
                                    continue;
                                }
                                collide(&mut c.x, &mut c.prev, w, (&def.radius, &def.friction, &def.masks), &caps, &caps_was);
                                continue;
                            }
                            let Some(set) = usize::try_from(e).ok().and_then(|e| def.sets.get(e)) else { continue };
                            if !SKIP.is_empty() && *SKIP == match set { Set::Standard(_) => "standard", Set::Stretch(_) => "stretch", Set::LocalRange { .. } => "local_range", Set::Bend(_) => "bend", Set::BendStiffness { .. } => "bend_stiffness", Set::Compressible(_) => "compressible", Set::Volume { .. } => "volume", Set::Unsolved => "transition" } {
                                continue;
                            }
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
                                Set::Compressible(ls) => {
                                    ls.iter().for_each(|(l, comp)| solve_compressible(&mut c.x, w, l, *comp))
                                }
                                Set::Volume { frames, apply } => solve_volume(&mut c.x, frames, apply),
                                Set::Unsolved => {}
                            }
                        }
                    }
                }
            }
            c.caps_prev = capsules;
            if *PROBE {
                let lr: Vec<(usize, Vec3)> = def
                    .sets
                    .iter()
                    .filter_map(|st| if let Set::LocalRange { items, .. } = st { Some(items) } else { None })
                    .flatten()
                    .filter_map(|&(p, r, ..)| Some((p, refs.get(r).copied().flatten()?.0)))
                    .filter(|&(p, _)| p < c.x.len() && w[p] > 0.0)
                    .collect();
                let fixed: Vec<(usize, Vec3)> = def.moves.iter().filter_map(|&(v, p)| Some((p, refs.get(v).copied().flatten()?.0))).filter(|&(p, _)| p < c.x.len()).collect();
                if !lr.is_empty() && !fixed.is_empty() {
                    let mean = |v: &[(usize, Vec3)], sim: bool| v.iter().map(|&(p, q)| if sim { c.x[p] } else { q }).sum::<Vec3>() / v.len() as f32;
                    let (ds, dr) = (mean(&lr, true) - mean(&fixed, true), mean(&lr, false) - mean(&fixed, false));
                    let gaps: Vec<f32> = lr.iter().map(|&(p, q)| c.x[p].distance(q)).collect();
                    c.off_anim = (ds.angle_between(dr).to_degrees(), gaps.iter().sum::<f32>() / gaps.len() as f32, gaps.iter().copied().fold(0.0, f32::max));
                }
            }
            // A particle more than 3 m further from the model root than the cloth's own
            // reference vertices (or its rest pose: the Demon of Hatred's mane rests 10.2 m
            // out while its crouched references reach 7.1 m, which restarted it every frame
            // straight from the bind pose) means the sim blew up: log and restart.
            if let Ok(root) = globals.get(c.root) {
                let r = havok(root.to_matrix()).w_axis.truncate();
                let reach = refs.iter().flatten().map(|(q, _)| q.distance(r)).fold(rest_far, f32::max) + 3.0;
                if let Some((i, far)) = c.x.iter().map(|p| p.distance(r)).enumerate().max_by(|a, b| a.1.total_cmp(&b.1)).filter(|(_, d)| !(*d < reach)) {
                    c.blowups += 1;
                    if c.blowups < GIVE_UP {
                        warn!("cloth {}: particle {i} {far:.1} m from the root (inv mass {}, dt {dt:.3}); restarting", def.name, w[i]);
                    } else if !c.rigid {
                        warn!("cloth {}: blew up {GIVE_UP} frames running; carried rigidly from now on", def.name);
                        c.rigid = true;
                    }
                    c.started = false;
                } else if !culled {
                    c.blowups = 0;
                }
            }
            lap(2, &mut tp, &mut phase);
            if no_mesh {
                c.phase_us = phase;
                continue;
            }
            // 4a. Cloth-driven bones (and whatever hangs from them).
            for (name, t, local) in &def.bones {
                let (Some(&e), Some(&tri)) = (c.bones.get(name), def.tris.get(*t)) else { continue };
                let g = rigid_like(havok(tri_frame(&c.x, tri) * *local), havok(tri_frame(&def.rest, tri) * *local));
                if debug_now(time) {
                    let old = globals.get(e).map(|g| g.to_matrix()).unwrap_or_default();
                    info!("  bone {name}: cloth {:?} anim {:?} | x {:?} vs {:?}", g.w_axis.truncate(), old.w_axis.truncate(), g.x_axis.truncate(), old.x_axis.truncate());
                }
                let gt = GlobalTransform::from(bevy::math::Affine3A::from_mat4(g));
                propagate(e, gt, transforms, children, &mut overrides);
                c.out.bones.push((e, gt));
            }
            lap(3, &mut tp, &mut phase);
            // 4b. Display meshes.
            let frames: Vec<Mat4> = def.tris.iter().map(|&t| tri_frame(&c.x, t)).collect();
            for dm in &c.displays {
                let Some(disp) = def.displays.iter().find(|d| d.mesh == dm.mesh) else { continue };
                let joint_m: Vec<Mat4> = dm.joints.iter().zip(&dm.inv).map(|(&j, inv)| overrides.get(&j).copied().or_else(|| globals.get(j).ok().copied()).map(|g| g.to_matrix() * *inv).unwrap_or(Mat4::IDENTITY)).collect();
                let (mut pos, mut nrm, mut tan) = skin(dm, &joint_m);
                lap(4, &mut tp, &mut phase);
                deform_display(disp, &frames, &dm.tris, &mut pos, &mut nrm);
                if debug_now(time) {
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
                c.out.meshes.push((dm.handle.clone(), pos, nrm, tangents));
                lap(5, &mut tp, &mut phase);
            }
            c.phase_us = phase;
            c.cost_us += tc.elapsed().as_micros() as u64;
        }
    });
    COST[0].fetch_add(t0.elapsed().as_micros() as u64, std::sync::atomic::Ordering::Relaxed);
}

/// Microseconds spent in `simulate` / `apply`, and frames: logged every 600 frames
/// ("cloth cost") so the simulation's share of a frame can be read off any run.
static COST: [std::sync::atomic::AtomicU64; 3] = [std::sync::atomic::AtomicU64::new(0), std::sync::atomic::AtomicU64::new(0), std::sync::atomic::AtomicU64::new(0)];

/// Skins a display mesh's vertices (position, normal, bind tangent) from its joints, in
/// parallel chunks on the compute pool: the big meshes (fur, 50k+ vertices) are the cost of
/// the cloth, not the solver.
#[allow(clippy::type_complexity)]
fn skin(dm: &DisplayMesh, joint_m: &[Mat4]) -> (Vec<[f32; 3]>, Vec<[f32; 3]>, Vec<Vec3>) {
    const CHUNK: usize = 4096;
    let n = dm.pos.len();
    let one = |range: std::ops::Range<usize>| {
        let mut pos: Vec<[f32; 3]> = Vec::with_capacity(range.len());
        let mut nrm: Vec<[f32; 3]> = Vec::with_capacity(range.len());
        let mut tan: Vec<Vec3> = Vec::with_capacity(range.len());
        for i in range {
            let (p, nn) = (&dm.pos[i], &dm.normal[i]);
            let (mut sp, mut sn, mut st) = (Vec3::ZERO, Vec3::ZERO, Vec3::ZERO);
            let bt = dm.tangent.get(i).map_or(Vec3::ZERO, |t| Vec3::new(t[0], t[1], t[2]));
            for k in 0..4 {
                let wt = dm.weights[i][k];
                if wt <= 0.0 && k > 0 {
                    continue;
                }
                let m = joint_m.get(dm.idx[i][k] as usize).copied().unwrap_or(Mat4::IDENTITY);
                sp += m.transform_point3(Vec3::from(*p)) * wt;
                sn += m.transform_vector3(Vec3::from(*nn)) * wt;
                st += m.transform_vector3(bt) * wt;
            }
            pos.push(sp.into());
            nrm.push(sn.normalize_or_zero().into());
            tan.push(st);
        }
        (pos, nrm, tan)
    };
    if n <= CHUNK {
        return one(0..n);
    }
    let parts = bevy::tasks::ComputeTaskPool::get().scope(|sc| {
        for start in (0..n).step_by(CHUNK) {
            let one = &one;
            sc.spawn(async move { one(start..(start + CHUNK).min(n)) });
        }
    });
    let (mut pos, mut nrm, mut tan) = (Vec::with_capacity(n), Vec::with_capacity(n), Vec::with_capacity(n));
    for (p, nn, t) in parts {
        pos.extend(p);
        nrm.extend(nn);
        tan.extend(t);
    }
    (pos, nrm, tan)
}

/// Writes the frame's cloth results: the driven bones' world transforms (propagated to what
/// hangs from them) and the display meshes' vertices.
/// A cloth display mesh's interleaved GPU vertex bytes (Mesh::create_packed_vertex_buffer_data)
/// and where its position / normal / tangent sit in each vertex: `apply` patches those and
/// `write_cloth_uploads` writes them over the mesh's slab in place.
struct Packed {
    bytes: Vec<u8>,
    stride: usize,
    pos: usize,
    nrm: Option<usize>,
    tan: Option<usize>,
}

impl Packed {
    fn of(mesh: &Mesh) -> Option<Packed> {
        let mut at = 0;
        let (mut pos, mut nrm, mut tan) = (None, None, None);
        for (attr, _) in mesh.attributes() {
            if attr.id == Mesh::ATTRIBUTE_POSITION.id {
                pos = Some(at);
            } else if attr.id == Mesh::ATTRIBUTE_NORMAL.id {
                nrm = Some(at);
            } else if attr.id == Mesh::ATTRIBUTE_TANGENT.id {
                tan = Some(at);
            }
            at += attr.format.size() as usize;
        }
        Some(Packed { bytes: mesh.create_packed_vertex_buffer_data(), stride: mesh.get_vertex_size() as usize, pos: pos?, nrm, tan })
    }

    fn patch(&mut self, pos: &[[f32; 3]], nrm: &[[f32; 3]], tan: &[[f32; 4]]) {
        let put = |bytes: &mut [u8], at: usize, v: &[f32]| {
            for (k, f) in v.iter().enumerate() {
                bytes[at + 4 * k..at + 4 * k + 4].copy_from_slice(&f.to_le_bytes());
            }
        };
        for (i, v) in self.bytes.chunks_exact_mut(self.stride).enumerate() {
            if let Some(p) = pos.get(i) {
                put(v, self.pos, p);
            }
            if let (Some(o), Some(n)) = (self.nrm, nrm.get(i)) {
                put(v, o, n);
            }
            if let (Some(o), Some(t)) = (self.tan, tan.get(i)) {
                put(v, o, t);
            }
        }
    }
}

/// This frame's cloth vertex writes: (mesh, vertex size, interleaved bytes). Filled by `apply`,
/// moved to the render world by `extract_cloth_uploads`.
#[derive(Resource, Default)]
struct ClothUploads(Vec<(AssetId<Mesh>, usize, Vec<u8>)>);

fn extract_cloth_uploads(mut main: ResMut<bevy::render::MainWorld>, mut render: ResMut<ClothUploads>) {
    if let Some(mut up) = main.get_resource_mut::<ClothUploads>() {
        render.0 = std::mem::take(&mut up.0);
    }
}

/// Bevy 0.19 frees and re-allocates a modified mesh's whole slab every frame
/// (allocate_and_free_meshes: "TODO: Consider explicitly reusing allocations for changed meshes
/// of the same size"), 4.4 ms of render-world time a frame for 47 cloth meshes (round 90 perf
/// trace). The Mesh asset stays as it is; its vertices are written over its slab in place.
fn write_cloth_uploads(mut ups: ResMut<ClothUploads>, allocator: Res<bevy::render::mesh::allocator::MeshAllocator>, queue: Res<bevy::render::renderer::RenderQueue>) {
    for (id, stride, bytes) in ups.0.drain(..) {
        if let Some(slice) = allocator.mesh_vertex_slice(&id) {
            if (slice.range.end - slice.range.start) as usize * stride == bytes.len() {
                queue.write_buffer(slice.buffer, slice.range.start as u64 * stride as u64, &bytes);
            }
        }
    }
}

fn apply(mut actors: Query<&mut Cloths>, mut globals: Query<&mut GlobalTransform>, transforms: Query<&Transform>, children: Query<&Children>, meshes: Res<Assets<Mesh>>, mut packed: Local<HashMap<AssetId<Mesh>, Packed>>, mut uploads: ResMut<ClothUploads>) {
    // Cost probe: SHINOBI_CLOTH_NO_UPLOAD=1 keeps the bones but leaves the meshes alone.
    let no_upload = std::env::var("SHINOBI_CLOTH_NO_UPLOAD").is_ok();
    let t0 = std::time::Instant::now();
    let mut pieces = 0;
    let mut costs: Vec<(u64, String)> = Vec::new();
    for mut cloths in &mut actors {
        for c in cloths.0.iter_mut() {
            let ph: Vec<String> = c.phase_us.iter().map(|us| format!("{:.2}", *us as f64 / 300.0 / 1000.0)).collect();
            costs.push((c.cost_us, format!("{} ({} particles, {} substeps x {} iterations{}; refs/restart/solve/bones/skin/verts {})", c.def.name, c.def.rest.len(), c.def.substeps, c.def.iterations, if c.rigid { ", rigid" } else { "" }, ph.join("/"))));
            for (e, g) in c.out.bones.drain(..) {
                set_global(e, g, &mut globals, &transforms, &children);
            }
            for (handle, pos, nrm, tangents) in c.out.meshes.drain(..) {
                pieces += 1;
                if no_upload {
                    continue;
                }
                let id = handle.id();
                if !packed.contains_key(&id) {
                    let Some(p) = meshes.get(&handle).and_then(Packed::of) else { continue };
                    packed.insert(id, p);
                }
                let p = packed.get_mut(&id).unwrap();
                p.patch(&pos, &nrm, &tangents);
                uploads.0.push((id, p.stride, p.bytes.clone()));
            }
        }
    }
    use std::sync::atomic::Ordering::Relaxed;
    COST[1].fetch_add(t0.elapsed().as_micros() as u64, Relaxed);
    let frames = COST[2].fetch_add(1, Relaxed) + 1;
    if frames % 300 == 0 {
        let (sim, app) = (COST[0].swap(0, Relaxed), COST[1].swap(0, Relaxed));
        costs.sort_by(|a, b| b.0.cmp(&a.0));
        let top: Vec<String> = costs.iter().take(3).map(|(us, n)| format!("{n} {:.2}", *us as f64 / 300.0 / 1000.0)).collect();
        // Once a minute at 60 fps, more than a frame's worth only. Phases per cloth: references,
        // restart, solve, bones, skinning, cloth vertices + tangents.
        if sim as f64 / 300.0 > 1000.0 && frames % 3600 == 300 {
            info!("cloth cost: simulate {:.2} ms, apply {:.2} ms per frame over 300 frames ({} cloths, {pieces} display meshes); top ms: {}", sim as f64 / 300.0 / 1000.0, app as f64 / 300.0 / 1000.0, costs.len(), top.join(", "));
        }
        for mut cloths in &mut actors {
            for c in cloths.0.iter_mut() {
                c.cost_us = 0;
                c.phase_us = [0; 6];
            }
        }
    }
}

/// A bone's world transform this frame and, from it, its children's (as set_global, into a
/// map instead of the world).
fn propagate(e: Entity, g: GlobalTransform, transforms: &Query<&Transform>, children: &Query<&Children>, out: &mut HashMap<Entity, GlobalTransform>) {
    out.insert(e, g);
    if let Ok(kids) = children.get(e) {
        for &k in kids {
            if let Ok(t) = transforms.get(k) {
                propagate(k, g.mul_transform(*t), transforms, children, out);
            }
        }
    }
}

/// The -1 execution step (the exe's capsule case, FUN_1415802d0): pushes each particle out of
/// the capsules its mask selects (capsule radius tapered along the axis, plus the particle
/// radius), along the nearest direction (no side from the previous position, no swept test).
/// Friction then rewrites the previous position: that share of the particle's motion along the
/// surface, relative to the surface's own motion this substep (`was` = the capsules a substep
/// ago), is taken out.
fn collide(x: &mut [Vec3], prev: &mut [Vec3], w: &[f32], (radius, friction, masks): (&[f32], &[f32], &[u32]), capsules: &[Option<(Vec3, Vec3, f32, f32)>], was: &[Option<(Vec3, Vec3, f32, f32)>]) {
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
                let fr = friction.get(i).copied().unwrap_or(0.0);
                if fr > 0.0 {
                    let n = d / len;
                    let moved = was.get(k).copied().flatten().map_or(Vec3::ZERO, |(a0, b0, _, _)| q - a0.lerp(b0, t));
                    let rel = (*p - prev[i]) - moved;
                    prev[i] += (rel - n * rel.dot(n)) * fr;
                }
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
    static AT: std::sync::OnceLock<Option<f32>> = std::sync::OnceLock::new();
    let Some(at) = *AT.get_or_init(|| std::env::var("SHINOBI_CLOTH_DEBUG").ok().and_then(|v| v.parse::<f32>().ok())) else { return false };
    time.elapsed_secs() > at && time.elapsed_secs() - time.delta_secs() <= at
}

/// SHINOBI_CAPE_TURN=<deg> (90 / 180): Wolf runs ahead (stick forward) from 3 s for 2 s, then
/// the stick turns by that many degrees (to the right) for 1 s, then lets go; the game exits at
/// 8 s. SHINOBI_CAPE_SHOTS=<dir> screenshots it at 10 fps from 4.5 s. For comparing the cape with
/// the user's clips (manual test #2): `cape_probe` logs each of Wolf's cloths' swing.
fn cape_drive(time: Res<Time>, mut pad: ResMut<crate::player::PadInput>, mut exit: MessageWriter<AppExit>, mut commands: Commands, mut next: Local<f32>) {
    let deg: f32 = std::env::var("SHINOBI_CAPE_TURN").ok().and_then(|v| v.parse().ok()).unwrap_or(90.0);
    let t = time.elapsed_secs();
    let a = deg.to_radians();
    pad.stick = match t {
        t if (3.0..5.0).contains(&t) => Vec2::new(0.0, 1.0),
        t if (5.0..6.0).contains(&t) => Vec2::new(a.sin(), a.cos()),
        _ => Vec2::ZERO,
    };
    if let Ok(dir) = std::env::var("SHINOBI_CAPE_SHOTS") {
        if t >= 4.5 && t <= 7.5 && t >= *next {
            *next = t + 0.1;
            let path = format!("{dir}/c{:04}.png", (t * 100.0) as u32);
            commands.spawn(bevy::render::view::screenshot::Screenshot::primary_window()).observe(bevy::render::view::screenshot::save_to_disk(path));
        }
    }
    if t > 8.0 {
        exit.write(AppExit::Success);
    }
}

/// With `cape_drive`: per frame, each of Wolf's cloths' swing from hanging straight down: the
/// angle (deg) of the free particles' centre from its fixed particles' centre, and that offset in
/// Wolf's own frame (x, y, z; from 2 s, standing, for the rest pose).
fn cape_probe(time: Res<Time>, wolf: Query<(&Cloths, &GlobalTransform), With<crate::player::Player>>) {
    let t = time.elapsed_secs();
    if !(2.0..8.0).contains(&t) {
        return;
    }
    let Ok((cloths, g)) = wolf.single() else { return };
    let mut line = format!("cape t {t:.3} yaw {:.0}", g.rotation().to_euler(EulerRot::YXZ).0.to_degrees());
    line += &swings(cloths, g);
    info!("{line}");
}

/// Each cloth's swing from hanging straight down (`cape_probe`): " | name deg (x,y,z)", the free
/// particles' centre from the fixed ones' in the model's own frame.
fn swings(cloths: &Cloths, g: &GlobalTransform) -> String {
    let inv = g.rotation().inverse();
    let mut line = String::new();
    for c in &cloths.0 {
        let (mut fixed, mut nf, mut free, mut nfree) = (Vec3::ZERO, 0.0, Vec3::ZERO, 0.0);
        for (i, p) in c.x.iter().enumerate() {
            if c.def.inv_mass.get(i).copied().unwrap_or(0.0) <= 0.0 {
                fixed += mirror(*p);
                nf += 1.0;
            } else {
                free += mirror(*p);
                nfree += 1.0;
            }
        }
        if nf == 0.0 || nfree == 0.0 {
            continue;
        }
        let d = free / nfree - fixed / nf;
        let ang = d.normalize_or(Vec3::NEG_Y).angle_between(Vec3::NEG_Y).to_degrees();
        let l = inv * d;
        line += &format!(" | {} {ang:.0} ({:.3},{:.3},{:.3}) anim {:.0} gap {:.2}/{:.2}", c.def.name, l.x, l.y, l.z, c.off_anim.0, c.off_anim.1, c.off_anim.2);
    }
    line
}

/// SHINOBI_CLOTH_SKIP=<standard|stretch|local_range|bend|bend_stiffness|compressible|volume|collide>:
/// leaves that constraint kind out of every solve (finding what makes a cloth jitter).
static SKIP: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| std::env::var("SHINOBI_CLOTH_SKIP").unwrap_or_default());

static PROBE: std::sync::LazyLock<bool> = std::sync::LazyLock::new(|| std::env::var("SHINOBI_CLOTH_PROBE").is_ok() || std::env::var("SHINOBI_CAPE_TURN").is_ok());

/// SHINOBI_CLOTH_PROBE=1 (with SHINOBI_FIGHT / SHINOBI_DUEL): every 0.1 s each enemy's anim,
/// speed (m/s), yaw rate (deg/s) and cloth swings, for comparing a boss's cloth with the clips.
fn enemy_cloth_probe(time: Res<Time>, enemies: Query<(Entity, &Cloths, &GlobalTransform, &crate::actor::Actor), With<crate::enemy::Enemy>>, mut last: Local<std::collections::HashMap<Entity, (f32, Vec3, f32)>>, mut commands: Commands, mut next: Local<f32>) {
    let t = time.elapsed_secs();
    for (e, cloths, g, a) in &enemies {
        let pos = g.translation();
        let yaw = g.rotation().to_euler(EulerRot::YXZ).0.to_degrees();
        let Some(&(t0, p0, y0)) = last.get(&e) else {
            last.insert(e, (t, pos, yaw));
            continue;
        };
        let dt = t - t0;
        // SHINOBI_CLOTH_PROBE_DT=<s>: log interval (default 0.1; 0 = every frame, for jitter).
        let every: f32 = std::env::var("SHINOBI_CLOTH_PROBE_DT").ok().and_then(|v| v.parse().ok()).unwrap_or(0.1);
        if dt < every || dt <= 0.0 {
            continue;
        }
        last.insert(e, (t, pos, yaw));
        let speed = (pos - p0).with_y(0.0).length() / dt;
        let turn = ((yaw - y0 + 540.0).rem_euclid(360.0) - 180.0) / dt;
        info!("cloth t {t:.2} anim {} speed {speed:.1} turn {turn:.0}{}", a.anim, swings(cloths, g));
    }
    // SHINOBI_CLOTH_SHOTS=<dir>[:<from s>:<to s>]: screenshots at 10 fps in that window.
    if let Ok(v) = std::env::var("SHINOBI_CLOTH_SHOTS") {
        let mut it = v.rsplitn(3, ':');
        let (to, from) = (it.next().and_then(|x| x.parse().ok()).unwrap_or(30.0f32), it.next().and_then(|x| x.parse().ok()).unwrap_or(0.0f32));
        let dir = it.next().unwrap_or(&v).to_string();
        if (from..to).contains(&t) && t >= *next {
            *next = t + 0.1;
            commands.spawn(bevy::render::view::screenshot::Screenshot::primary_window()).observe(bevy::render::view::screenshot::save_to_disk(format!("{dir}/k{:05}.png", (t * 100.0) as u32)));
        }
    }
}

#[cfg(test)]
mod tests {
    /// The exe's capsule collision (FUN_1415802d0): out along the nearest direction, and the
    /// particle's friction rewrites its previous position along the surface.
    #[test]
    fn a_capsule_pushes_out_and_its_friction_takes_the_slide() {
        use super::collide;
        use bevy::prelude::Vec3;
        // A particle 0.05 m inside a still capsule along x, sliding 0.1 m along the axis this
        // step: pushed out to radius + particle radius; friction 1 leaves no slide in prev.
        let cap = [Some((Vec3::ZERO, Vec3::X * 2.0, 0.2, 0.2))];
        for (fr, slide) in [(0.0, 0.1), (1.0, 0.0)] {
            let mut x = vec![Vec3::new(1.0, 0.15, 0.0)];
            let mut prev = vec![Vec3::new(0.9, 0.15, 0.0)];
            collide(&mut x, &mut prev, &[1.0], (&[0.01], &[fr], &[1]), &cap, &cap);
            assert!((x[0].y - 0.21).abs() < 1e-5, "{x:?}");
            assert!(((x[0].x - prev[0].x) - slide).abs() < 1e-5, "friction {fr}: {prev:?}");
        }
    }

    /// A compressible link (Genichiro's kusazuri / mantle, 69 cloths) is free between its
    /// compression and rest lengths and solved back to the nearer one outside them.
    #[test]
    fn a_compressible_link_has_a_dead_zone() {
        use bevy::prelude::Vec3;
        // Full stiffness, premultiplied with 1 / (invMassA + invMassB) as the data has it.
        let l = super::Link { a: 0, b: 1, rest: 1.0, stiffness: 0.5 };
        let w = [1.0, 1.0];
        let len = |d: f32| {
            let mut x = [Vec3::ZERO, Vec3::X * d];
            super::solve_compressible(&mut x, &w, &l, 0.5);
            x[1].distance(x[0])
        };
        assert!((len(0.8) - 0.8).abs() < 1e-5, "inside the dead zone it moved");
        assert!((len(1.5) - 1.0).abs() < 1e-5, "a stretched link was not pulled back to rest");
        assert!((len(0.2) - 0.5).abs() < 1e-5, "a squashed link was not pushed out to its compression length");
    }

    /// Wolf's coat (bodybese01 / 02 / 03) has no normals in its deform data (position-only
    /// MeshMeshDeform; the game rebuilds them, hclUpdateAllVertexFramesOperator): after a deform
    /// step from the rest pose every coat display vertex has a unit normal, on the side of its
    /// bind normal.
    /// Containers with two sim cloths export both (cloth_export.rs, 2026-10-13): the Corrupted
    /// Monk's, Emma's and the Owl's second sims carry their own particles, skinned references, fixed
    /// particles and driven bones, and their own solver config (the Owl's coat had the second sim's
    /// empty constraint order and 1 substep before).
    /// Manual test #2: the sim runs in model space, so a model that runs or turns carries its
    /// cloth along with no velocity of its own (the game: no Transfer Motion), while motion relative
    /// to the model is kept.
    #[test]
    fn cloth_rides_with_the_model_root() {
        use bevy::prelude::{Mat4, Quat, Vec3};
        let old = Mat4::from_translation(Vec3::new(1.0, 0.0, 0.0));
        let new = Mat4::from_rotation_translation(Quat::from_rotation_y(1.0), Vec3::new(1.5, 0.0, 0.2));
        let (mut x, mut prev) = (vec![Vec3::new(1.0, -0.5, 0.3)], vec![Vec3::new(1.0, -0.4, 0.3)]);
        let local = (old.inverse().transform_point3(x[0]), old.inverse().transform_point3(prev[0]));
        super::carry(&mut x, &mut prev, old, new);
        assert!(new.inverse().transform_point3(x[0]).distance(local.0) < 1e-5);
        assert!(new.inverse().transform_point3(prev[0]).distance(local.1) < 1e-5);
    }

    /// The volume constraint (shape matching): a rotated, translated copy of the rest shape is
    /// its own target (nothing moves), and a squashed copy is pulled back toward the rotated
    /// rest shape.
    #[test]
    fn volume_constraint_matches_the_shape() {
        use bevy::prelude::{Quat, Vec3};
        let rest = [Vec3::new(1.0, 0.0, 0.0), Vec3::new(-1.0, 0.0, 0.0), Vec3::new(0.0, 1.0, 0.0), Vec3::new(0.0, -1.0, 0.0), Vec3::new(0.0, 0.0, 1.0), Vec3::new(0.0, 0.0, -1.0), Vec3::new(0.3, 0.2, -0.5), Vec3::new(-0.3, -0.2, 0.5)];
        let n = rest.len();
        let w = 1.0 / n as f32;
        let c0: Vec3 = rest.iter().sum::<Vec3>() / n as f32;
        let frames: Vec<_> = rest.iter().enumerate().map(|(i, p)| (*p - c0, i, w)).collect();
        let apply: Vec<_> = rest.iter().enumerate().map(|(i, p)| (*p - c0, i, 1.0)).collect();
        let q = Quat::from_rotation_y(0.7) * Quat::from_rotation_x(0.3);
        let moved: Vec<Vec3> = rest.iter().map(|p| q * *p + Vec3::new(2.0, -1.0, 0.5)).collect();
        let mut x = moved.clone();
        super::solve_volume(&mut x, &frames, &apply);
        for (a, b) in x.iter().zip(&moved) {
            assert!(a.distance(*b) < 1e-4, "{a:?} vs {b:?}");
        }
        // Squash it along y: the solve pulls it back toward the rigid copy.
        let c1: Vec3 = moved.iter().sum::<Vec3>() / n as f32;
        let mut x: Vec<Vec3> = moved.iter().map(|p| c1 + (*p - c1) * Vec3::new(1.0, 0.5, 1.0)).collect();
        let err = |x: &[Vec3]| x.iter().zip(&moved).map(|(a, b)| a.distance(*b)).sum::<f32>();
        let before = err(&x);
        super::solve_volume(&mut x, &frames, &apply);
        assert!(err(&x) < before * 0.9, "{} -> {}", before, err(&x));
    }

    /// A slow frame runs several ~1/60 s steps (the game's cloth is stepped per 60 fps frame).
    #[test]
    fn slow_frames_run_several_sixtieth_second_steps() {
        assert_eq!(super::frame_steps(1.0 / 144.0), 1);
        assert_eq!(super::frame_steps(1.0 / 60.0), 1);
        assert_eq!(super::frame_steps(1.0 / 30.0), 2);
        assert_eq!(super::frame_steps(1.0 / 20.0), 3);
        assert_eq!(super::frame_steps(0.2), 4);
    }

    #[test]
    fn second_sim_cloths_are_exported() {
        for (model, name, bones) in [("c5000", "c5000_Sleeve#1", 26), ("c5005", "c5005_Sleeve#1", 26), ("c7400", "c7400_sleeve#1", 12), ("c5060", "c5060_Chainmail#1", 2)] {
            let defs = super::load(&crate::paths::root().join(format!("extracted/model_{model}.bin")));
            if defs.is_empty() {
                println!("no extracted cloth: skipped");
                return;
            }
            let d = defs.iter().find(|d| d.name == name).unwrap_or_else(|| panic!("{name} missing"));
            assert_eq!(d.bones.len(), bones, "{name}");
            assert!(!d.moves.is_empty() && d.moves.iter().all(|&(v, p)| p < d.rest.len() && !d.refs[v].is_empty()), "{name}");
            assert!(!d.execution.is_empty(), "{name}");
        }
        let defs = super::load(&crate::paths::root().join("extracted/model_c5060.bin"));
        let coat = defs.iter().find(|d| d.name == "c5060_coat").unwrap();
        assert!(!coat.execution.is_empty() && coat.substeps == 2);
        // The coat feathers ride the coat sim (ref_deform), and draw their fur mesh 4 themselves.
        let feathers = defs.iter().position(|d| d.name == "c5060_coat#1").expect("c5060_coat#1");
        let (src, verts) = defs[feathers].ref_deform.as_ref().expect("chained references");
        assert_eq!(src, "c5060_coat");
        assert!(defs.iter().position(|d| d.name == *src).unwrap() < feathers, "the source runs first");
        assert!(defs[feathers].moves.iter().all(|&(v, _)| verts.iter().any(|(rv, _)| *rv == v)));
        assert_eq!(defs[feathers].display_meshes().collect::<Vec<_>>(), vec![4]);
        assert!(!coat.display_meshes().any(|m| m == 4));
    }

    #[test]
    fn coat_normals_after_deform() {
        let model = crate::paths::root().join("extracted/model_c0000_bd_m_9040.bin");
        let defs = super::load(&model);
        if defs.is_empty() {
            println!("no extracted cloth: skipped");
            return;
        }
        let mut checked = 0;
        for def in defs.iter().filter(|d| d.name.contains("bodyb")) {
            let frames: Vec<bevy::math::Mat4> = def.tris.iter().map(|&t| super::tri_frame(&def.rest, t)).collect();
            for disp in &def.displays {
                assert!(disp.verts.iter().all(|(_, s)| s.iter().all(|s| s.n == bevy::math::Vec4::ZERO)), "{} mesh {} now carries normals", def.name, disp.mesh);
                let (mut pos, bind, idx) = crate::model::test_mesh(&model, disp.mesh).expect("mesh");
                let tris: Vec<[u32; 3]> = idx.chunks_exact(3).map(|t| [t[0], t[1], t[2]]).collect();
                let mut nrm = bind.clone();
                super::deform_display(disp, &frames, &tris, &mut pos, &mut nrm);
                let same = disp.verts.iter().filter(|(v, _)| bevy::math::Vec3::from(nrm[*v]).dot(bevy::math::Vec3::from(bind[*v])) > 0.0).count();
                for (v, _) in &disp.verts {
                    let l = bevy::math::Vec3::from(nrm[*v]).length();
                    assert!((l - 1.0).abs() < 1e-3, "{} mesh {} vertex {v}: |n| {l}", def.name, disp.mesh);
                }
                println!("{} mesh {}: {} vertices, {same} on the bind normal's side", def.name, disp.mesh, disp.verts.len());
                assert!(same * 10 >= disp.verts.len() * 9, "{} mesh {}: only {same} of {} on the bind side", def.name, disp.mesh, disp.verts.len());
                checked += disp.verts.len();
            }
        }
        assert!(checked > 0);
    }
}
