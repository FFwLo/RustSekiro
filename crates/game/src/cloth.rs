//! Havok cloth on the skinned characters, driven by the game's own cloth data
//! (`cache/models/<id>.cloth.bin`, from `sekiro-extract models`, decoded by
//! [`sekiro_formats::hkx::cloth`]): Wolf's scarf and hair, Genichiro's cape, sashes and skirt, the
//! Owl's coat, ponytail and haori, the soldier's sash, haori and straw cape.
//!
//! The solver and its constraint formulas are adapted from FwLo's MIT-licensed RustSekiro
//! (`src/cloth.rs`); the data reading is ours. Per cloth, every frame (the `#01#` operator chain of
//! the cloth state):
//!
//! 1. skin the reference vertices from the animated bones (`p = sum(boneWorld * local)`);
//! 2. fixed particles (inverse mass 0) move to their reference vertex, in even parts over the
//!    substeps;
//! 3. Verlet with the cloth's gravity and damping, `substeps` x `iterations`, the constraint sets
//!    in `execution` order (`-1` is the collision pass against the cloth's capsules on bones);
//! 4. display vertices follow their sim triangles (`p = sum(triFrame * local)`) and the cloth-driven
//!    bones (skirt and hakama plates, the ponytail) follow theirs.
//!
//! Everything is solved in Havok world space, the game's world mirrored in X, so the exported
//! locals apply unchanged and the world-space solve gives the cloth its inertia. The display meshes
//! are taken off the GPU skin: each is replaced by a per-character copy without joint attributes
//! whose vertices are skinned on the CPU every frame (the cloth's own vertices then follow the
//! simulation instead). `--no-cloth` turns the whole thing off.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use bevy::camera::primitives::Aabb;
use bevy::camera::visibility::{DynamicSkinnedMeshBounds, NoFrustumCulling};
use bevy::gltf::{GltfMaterialName, GltfMeshName};
use bevy::math::Affine3A;
use bevy::mesh::skinning::{SkinnedMesh, SkinnedMeshInverseBindposes};
use bevy::mesh::{Mesh, VertexAttributeValues};
use bevy::prelude::*;
use sekiro_formats::hkx::cloth as data;

use crate::character::{PresentationLod, Rig};
use crate::perf_log::SystemTimer;

/// Cloth further than this from the camera (plus its own rest reach) is carried rigidly in its
/// rest pose on the model root instead of being solved.
const RANGE: f32 = 15.0;

// FwLo's MIT RustSekiro SHINOBI_CLOTH_RANGE; keep the shipped 15 m default.
fn cloth_range() -> f32 {
    static RANGE_OVERRIDE: std::sync::OnceLock<f32> = std::sync::OnceLock::new();
    *RANGE_OVERRIDE.get_or_init(|| {
        std::env::var("SHINOBI_CLOTH_RANGE")
            .ok()
            .and_then(|v| v.parse::<f32>().ok())
            .filter(|r| r.is_finite() && *r >= 0.0)
            .unwrap_or(RANGE)
    })
}

/// A cloth that blows up this many frames running is carried rigidly for good.
const GIVE_UP: u32 = 5;

/// Longest frame the solver integrates; a hitch must not fling the cloth.
const MAX_DT: f32 = 1.0 / 20.0;

pub fn plugin(cache: PathBuf) -> ClothPlugin {
    ClothPlugin {
        cache,
        enabled: !std::env::args().any(|a| a == "--no-cloth"),
    }
}

pub struct ClothPlugin {
    cache: PathBuf,
    enabled: bool,
}

impl Plugin for ClothPlugin {
    fn build(&self, app: &mut App) {
        if !self.enabled {
            return;
        }
        app.insert_resource(Library {
            dir: self.cache.join("models"),
            loaded: HashMap::new(),
        })
        .add_systems(Update, attach.after(crate::character::bind_rigs))
        .add_systems(
            PostUpdate,
            (simulate, apply)
                .chain()
                .after(bevy::transform::TransformSystems::Propagate)
                .before(bevy::camera::visibility::VisibilitySystems::CalculateBounds),
        );
    }
}

struct Link {
    a: usize,
    b: usize,
    rest: f32,
    stiffness: f32,
}

enum Set {
    /// Keeps the rest length both ways.
    Standard(Vec<Link>),
    /// Only stops stretching past the rest length.
    Stretch(Vec<Link>),
    /// A particle stays within `max` of its skinned reference vertex and, with `normal`, between
    /// the min and max distance along the reference normal.
    LocalRange {
        items: Vec<(usize, usize, f32, f32, f32)>,
        stiffness: f32,
        normal: bool,
    },
    /// (a, b, bendMinLength, stretchMaxLength, bendStiffness, stretchStiffness): pushed apart
    /// below the min length, pulled in above the max.
    Bend(Vec<(usize, usize, f32, f32, f32, f32)>),
    /// `hclBendStiffnessConstraintSet`, solved as the exe's `hclBendStiffnessConstraintSetMx`.
    BendStiffness {
        links: Vec<([f32; 4], f32, f32, [usize; 4])>,
        max_height_sq: f32,
        clamp: bool,
        rest_pose: bool,
    },
    Unsolved,
}

struct Slot {
    tri: usize,
    p: Vec4,
    n: Vec4,
}

struct Display {
    mesh: u32,
    unit_normal: bool,
    verts: Vec<(usize, Vec<Slot>)>,
}

/// An `hclCollidable` capsule on a bone.
struct Capsule {
    offset: Mat4,
    a: Vec3,
    b: Vec3,
    ra: f32,
    rb: f32,
}

/// One cloth's data, shared by every character of the same model.
struct ClothDef {
    name: String,
    part: String,
    gravity: Vec3,
    damping: f32,
    rest: Vec<Vec3>,
    /// The largest distance from the model root in the rest pose.
    rest_far: f32,
    inv_mass: Vec<f32>,
    radius: Vec<f32>,
    masks: Vec<u32>,
    capsules: Vec<Capsule>,
    capsule_bones: Vec<String>,
    tris: Vec<[usize; 3]>,
    sets: Vec<Set>,
    execution: Vec<i32>,
    substeps: u32,
    iterations: u32,
    moves: Vec<(usize, usize)>,
    /// Reference vertex skin: (index into `ref_bones`, local position with w = weight, local
    /// normal).
    refs: Vec<Vec<(usize, Vec4, Vec4)>>,
    ref_bones: Vec<String>,
    displays: Vec<Display>,
    bones: Vec<(String, usize, Mat4)>,
}

fn vec4(v: [f32; 4]) -> Vec4 {
    Vec4::from(v)
}

impl ClothDef {
    fn new(c: &data::Cloth) -> Self {
        let rest: Vec<Vec3> = c.particles.iter().map(|p| Vec3::from(p.position)).collect();
        let links = |l: &[(u16, u16, f32, f32)]| {
            l.iter()
                .map(|&(a, b, rest, stiffness)| Link {
                    a: a as usize,
                    b: b as usize,
                    rest,
                    stiffness,
                })
                .collect::<Vec<_>>()
        };
        let sets: Vec<Set> = c
            .constraints
            .iter()
            .map(|s| match s {
                data::Constraint::Standard(l) => Set::Standard(links(l)),
                data::Constraint::Stretch(l) => Set::Stretch(links(l)),
                data::Constraint::Bend(l) => Set::Bend(
                    l.iter()
                        .map(|&(a, b, min, max, bk, sk)| (a as usize, b as usize, min, max, bk, sk))
                        .collect(),
                ),
                data::Constraint::BendStiffness {
                    links,
                    max_rest_pose_height_sq,
                    clamp,
                    use_rest_pose,
                } => Set::BendStiffness {
                    links: links
                        .iter()
                        .map(|l| {
                            (
                                l.weights,
                                l.stiffness,
                                l.rest_curvature,
                                l.particles.map(usize::from),
                            )
                        })
                        .collect(),
                    max_height_sq: *max_rest_pose_height_sq,
                    clamp: *clamp,
                    rest_pose: *use_rest_pose,
                },
                data::Constraint::LocalRange {
                    items,
                    stiffness,
                    normal,
                } => Set::LocalRange {
                    items: items
                        .iter()
                        .map(|&(p, r, max, max_n, min_n)| {
                            (p as usize, r as usize, max, max_n, min_n)
                        })
                        .collect(),
                    stiffness: *stiffness,
                    normal: *normal,
                },
                data::Constraint::Other(_) => Set::Unsolved,
            })
            .collect();
        // An empty constraintExecution is Havok's "every set in its stored order", collisions
        // last; read literally it solves nothing and the cloth falls free.
        let execution = if c.execution.is_empty() && !sets.is_empty() {
            (0..sets.len() as i32).chain([-1]).collect()
        } else {
            c.execution.clone()
        };
        Self {
            name: c.name.clone(),
            part: c.part.clone(),
            gravity: Vec3::from(c.gravity),
            damping: c.damping,
            rest_far: rest.iter().map(|p| p.length()).fold(0.0, f32::max),
            rest,
            inv_mass: c.particles.iter().map(|p| p.inv_mass).collect(),
            radius: c.particles.iter().map(|p| p.radius).collect(),
            masks: c.masks.clone(),
            capsules: c
                .collidables
                .iter()
                .map(|k| Capsule {
                    offset: Mat4::from_cols_array_2d(&k.offset),
                    a: Vec3::from(k.a),
                    b: Vec3::from(k.b),
                    ra: k.radius_a,
                    rb: k.radius_b,
                })
                .collect(),
            capsule_bones: c.collidables.iter().map(|k| k.bone.clone()).collect(),
            tris: c.tris.iter().map(|t| t.map(usize::from)).collect(),
            sets,
            execution,
            substeps: c.substeps.max(1),
            iterations: c.iterations.max(1),
            moves: c
                .moves
                .iter()
                .map(|&(v, p)| (v as usize, p as usize))
                .filter(|&(_, p)| p < c.particles.len())
                .collect(),
            refs: c
                .refs
                .iter()
                .map(|r| {
                    r.iter()
                        .map(|e| (e.bone as usize, vec4(e.local), vec4(e.normal)))
                        .collect()
                })
                .collect(),
            ref_bones: c.ref_bones.clone(),
            displays: c
                .displays
                .iter()
                .map(|d| Display {
                    mesh: d.mesh,
                    unit_normal: d.unit_normal,
                    verts: d
                        .verts
                        .iter()
                        .map(|v| {
                            (
                                v.index as usize,
                                v.slots
                                    .iter()
                                    .map(|(t, p, n)| Slot {
                                        tri: *t as usize,
                                        p: vec4(*p),
                                        n: vec4(*n),
                                    })
                                    .collect(),
                            )
                        })
                        .collect(),
                })
                .collect(),
            bones: c
                .bones
                .iter()
                .map(|b| {
                    (
                        b.bone.clone(),
                        b.tri as usize,
                        Mat4::from_cols_array_2d(&b.local),
                    )
                })
                .collect(),
        }
    }
}

/// The parsed cloth of each model id, loaded once.
#[derive(Resource)]
struct Library {
    dir: PathBuf,
    loaded: HashMap<String, Option<Arc<Vec<ClothDef>>>>,
}

impl Library {
    fn get(&mut self, id: &str) -> Option<Arc<Vec<ClothDef>>> {
        self.loaded
            .entry(id.to_string())
            .or_insert_with(|| {
                let bytes = std::fs::read(self.dir.join(format!("{id}.cloth.bin"))).ok()?;
                match data::ClothSet::from_bytes(&bytes) {
                    Ok(set) => Some(Arc::new(set.cloths.iter().map(ClothDef::new).collect())),
                    Err(e) => {
                        warn!("{id}.cloth.bin: {e}");
                        None
                    }
                }
            })
            .clone()
    }
}

/// A display mesh switched from GPU skinning to per-frame CPU vertices: the cloth's vertices
/// follow the simulation, the rest are skinned here from their joints.
struct DisplayMesh {
    mesh: u32,
    entity: Entity,
    handle: Handle<Mesh>,
    /// Bind-pose positions and normals (the mesh's own space), and tangents (xyz, handedness w;
    /// empty without a tangent attribute).
    pos: Vec<[f32; 3]>,
    normal: Vec<[f32; 3]>,
    tangent: Vec<[f32; 4]>,
    joints: Vec<Entity>,
    inverse_bind: Vec<Mat4>,
    idx: Vec<[u16; 4]>,
    weights: Vec<[f32; 4]>,
}

/// One simulated cloth of one character.
struct ClothInst {
    /// The character's root entity (the model space of the cloth data).
    owner: Entity,
    def: Arc<Vec<ClothDef>>,
    /// Index into `def`.
    which: usize,
    /// Bone entities of the reference vertices, the collision capsules and the driven bones.
    ref_ents: Vec<Option<Entity>>,
    capsule_ents: Vec<Option<Entity>>,
    bone_ents: Vec<Option<Entity>>,
    displays: Vec<DisplayMesh>,
    x: Vec<Vec3>,
    prev: Vec<Vec3>,
    started: bool,
    /// Last substep length (time-corrected Verlet).
    last_h: f32,
    blowups: u32,
    rigid: bool,
    /// This frame's results, written to the world by [`apply`].
    out: ClothOut,
}

#[derive(Default)]
struct ClothOut {
    bones: Vec<(Entity, GlobalTransform)>,
    meshes: Vec<MeshOut>,
}

struct MeshOut {
    handle: Handle<Mesh>,
    pos: Vec<[f32; 3]>,
    normal: Vec<[f32; 3]>,
    tangent: Vec<[f32; 4]>,
}

/// The cloths of a character, on its root entity.
#[derive(Component)]
struct Cloths(Vec<ClothInst>);

/// A root that has been looked at once (it may have no cloth).
#[derive(Component)]
struct ClothChecked;

fn float3(v: Option<&VertexAttributeValues>) -> Option<Vec<[f32; 3]>> {
    match v? {
        VertexAttributeValues::Float32x3(v) => Some(v.clone()),
        _ => None,
    }
}

/// `SEKIRO_CLOTH_ONLY=<text>`: simulate only the cloths whose name contains the text (to find
/// which one is responsible for something in a screenshot).
fn only(name: &str) -> bool {
    static ONLY: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    ONLY.get_or_init(|| std::env::var("SEKIRO_CLOTH_ONLY").ok())
        .as_deref()
        .is_none_or(|o| name.contains(o))
}

/// Finds each root's model, loads its cloth data and takes the cloth's display meshes off the
/// GPU skin. Runs until the character's scene exists.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn attach(
    mut commands: Commands,
    mut library: ResMut<Library>,
    assets: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    bind_poses: Res<Assets<SkinnedMeshInverseBindposes>>,
    roots: Query<(Entity, &WorldAssetRoot, &Rig), (Without<Cloths>, Without<ClothChecked>)>,
    children: Query<&Children>,
    names: Query<&Name>,
    mesh_entities: Query<(&GltfMeshName, &GltfMaterialName, &Mesh3d, &SkinnedMesh)>,
) {
    for (root, scene, rig) in &roots {
        if !rig.bound {
            continue;
        }
        let Some(id) = assets.get_path(scene.0.id()).and_then(|p| {
            p.path()
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
        }) else {
            continue;
        };
        commands.entity(root).insert(ClothChecked);
        let Some(defs) = library.get(&id) else {
            continue;
        };
        let mut bones: HashMap<&str, Entity> = HashMap::new();
        let mut display_meshes: HashMap<(String, u32), Entity> = HashMap::new();
        for e in children.iter_descendants(root) {
            if let Ok((part, material, ..)) = mesh_entities.get(e)
                && let Some(index) = material
                    .0
                    .strip_prefix('m')
                    .and_then(|r| r.split(' ').next())
                    .and_then(|n| n.parse::<u32>().ok())
            {
                display_meshes.insert((part.0.clone(), index), e);
            }
            if let Ok(n) = names.get(e) {
                bones.entry(n.as_str()).or_insert(e);
            }
        }
        let find = |names: &[String]| -> Vec<Option<Entity>> {
            names
                .iter()
                .map(|n| bones.get(n.as_str()).copied())
                .collect()
        };
        let mut claimed: HashMap<Entity, usize> = HashMap::new();
        let mut cloths = Vec::new();
        let (mut missing_bones, mut missing_meshes) = (0, 0);
        for (which, def) in defs.iter().enumerate() {
            if !only(&def.name) {
                continue;
            }
            let driven: Vec<String> = def.bones.iter().map(|b| b.0.clone()).collect();
            let mut inst = ClothInst {
                owner: root,
                def: defs.clone(),
                which,
                ref_ents: find(&def.ref_bones),
                capsule_ents: find(&def.capsule_bones),
                bone_ents: find(&driven),
                displays: Vec::new(),
                x: Vec::new(),
                prev: Vec::new(),
                started: false,
                last_h: 0.0,
                blowups: 0,
                rigid: false,
                out: ClothOut::default(),
            };
            missing_bones += inst
                .ref_ents
                .iter()
                .chain(&inst.bone_ents)
                .filter(|e| e.is_none())
                .count();
            let mut wanted: Vec<u32> = def.displays.iter().map(|d| d.mesh).collect();
            wanted.sort_unstable();
            wanted.dedup();
            for index in wanted {
                let Some(&entity) = display_meshes.get(&(def.part.clone(), index)) else {
                    missing_meshes += 1;
                    continue;
                };
                if let Some(other) = claimed.get(&entity) {
                    warn!(
                        "cloth {}: mesh {index} of {} is already driven by cloth {other}; skipped",
                        def.name, def.part
                    );
                    continue;
                }
                let Ok((_, _, mesh3d, skin)) = mesh_entities.get(entity) else {
                    continue;
                };
                let Some(source) = meshes.get(&mesh3d.0) else {
                    continue;
                };
                let (Some(pos), Some(normal)) = (
                    float3(source.attribute(Mesh::ATTRIBUTE_POSITION)),
                    float3(source.attribute(Mesh::ATTRIBUTE_NORMAL)),
                ) else {
                    continue;
                };
                let (
                    Some(VertexAttributeValues::Uint16x4(idx)),
                    Some(VertexAttributeValues::Float32x4(weights)),
                ) = (
                    source.attribute(Mesh::ATTRIBUTE_JOINT_INDEX),
                    source.attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT),
                )
                else {
                    continue;
                };
                let tangent = match source.attribute(Mesh::ATTRIBUTE_TANGENT) {
                    Some(VertexAttributeValues::Float32x4(t)) => t.clone(),
                    _ => Vec::new(),
                };
                let Some(inverse_bind) = bind_poses.get(&skin.inverse_bindposes) else {
                    continue;
                };
                // A per-character copy without joint attributes (the shared asset would turn
                // every character of the model into the same cloth).
                let mut copy = source.clone();
                copy.remove_attribute(Mesh::ATTRIBUTE_JOINT_INDEX);
                copy.remove_attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT);
                copy.set_skinned_mesh_bounds(None);
                let display = DisplayMesh {
                    mesh: index,
                    entity,
                    pos,
                    normal,
                    tangent,
                    joints: skin.joints.clone(),
                    inverse_bind: inverse_bind.to_vec(),
                    idx: idx.clone(),
                    weights: weights.clone(),
                    handle: meshes.add(copy),
                };
                commands
                    .entity(entity)
                    .insert(Mesh3d(display.handle.clone()))
                    .remove::<(SkinnedMesh, DynamicSkinnedMeshBounds, NoFrustumCulling)>();
                claimed.insert(entity, which);
                inst.displays.push(display);
            }
            // A cloth that moves nothing visible (a hidden variant's hair) is not simulated.
            if inst.displays.is_empty() && def.bones.is_empty() {
                continue;
            }
            cloths.push(inst);
        }
        info!(
            "cloth {id}: {} of {} simulated ({} display meshes, {} bones missing, {} meshes not in the model)",
            cloths.len(),
            defs.len(),
            cloths.iter().map(|c| c.displays.len()).sum::<usize>(),
            missing_bones,
            missing_meshes
        );
        if !cloths.is_empty() {
            commands.entity(root).insert(Cloths(cloths));
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

/// Frame of a sim triangle: columns v0 - c, v1 - c, their cross, c (`unit` normalises the cross).
fn tri_frame(x: &[Vec3], t: [usize; 3], unit: bool) -> Mat4 {
    let (v0, v1, v2) = (x[t[0]], x[t[1]], x[t[2]]);
    let c = (v0 + v1 + v2) / 3.0;
    let (a, b) = (v0 - c, v1 - c);
    let n = a.cross(b);
    let n = if unit { n.normalize_or_zero() } else { n };
    Mat4::from_cols(a.extend(0.0), b.extend(0.0), n.extend(0.0), c.extend(1.0))
}

/// One bend-stiffness link, as the exe (`FUN_14152b490` single links; the batched SIMD path is
/// the same maths): `v = sum w_i p_i`, plus with the rest-pose config the rest bend, the unit
/// bisector of the two triangle normals times the corners' heights over the hinge times the rest
/// curvature. Each particle moves by `w_i invMass_i bendStiffness v` (the stiffness is negative:
/// it pulls `v` back to zero). With `clamp`, a link whose rest bend height squared exceeds
/// `max_height_sq` is skipped.
fn solve_bend_stiffness(
    x: &mut [Vec3],
    inv_mass: &[f32],
    link: &([f32; 4], f32, f32, [usize; 4]),
    max_height_sq: f32,
    clamp: bool,
    rest_pose: bool,
) {
    let (wt, stiff, rest_curvature, ix) = *link;
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
        let height = h * rest_curvature;
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
        x[ix[i]] += v * (wt[i] * k * inv_mass[ix[i]]);
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

/// The collision pass: pushes each particle out of the capsules its mask selects (the radius
/// tapered along the axis, plus the particle radius).
fn collide(
    x: &mut [Vec3],
    w: &[f32],
    radius: &[f32],
    masks: &[u32],
    capsules: &[Option<(Vec3, Vec3, f32, f32)>],
) {
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

/// The solve of one cloth, in place (steps 2 and 3).
fn solve(
    c: &mut ClothInst,
    refs: &[Option<(Vec3, Vec3)>],
    capsules: &[Option<(Vec3, Vec3, f32, f32)>],
    dt: f32,
    culled: bool,
    restarted: bool,
) {
    let defs = Arc::clone(&c.def);
    let def = &defs[c.which];
    let w = &def.inv_mass;
    // Fixed particles: from where they were to their reference vertex, reached in even parts
    // over the substeps (the exe's Simulate lerps each fixed particle by (substep + 1) /
    // substeps before integrating that substep).
    let fixed: Vec<(usize, Vec3, Vec3)> = def
        .moves
        .iter()
        .filter_map(|&(v, p)| {
            refs.get(v)
                .and_then(|r| *r)
                .filter(|_| p < c.x.len())
                .map(|(q, _)| (p, if restarted { q } else { c.x[p] }, q))
        })
        .collect();
    if dt <= 0.0 {
        for &(p, _, to) in &fixed {
            c.x[p] = to;
            c.prev[p] = to;
        }
        return;
    }
    if culled {
        return;
    }
    let h = dt / def.substeps as f32;
    let keep = (1.0 - def.damping).clamp(0.0, 1.0).powf(h);
    // Time-corrected Verlet: the last step's displacement covered last_h; scale it to this
    // step's h, or uneven frame times jitter the cloth.
    let ratio = if c.last_h > 0.0 {
        (h / c.last_h).clamp(0.25, 4.0)
    } else {
        1.0
    };
    c.last_h = h;
    for step in 0..def.substeps {
        let f = (step + 1) as f32 / def.substeps as f32;
        for &(p, from, to) in &fixed {
            c.x[p] = from.lerp(to, f);
            c.prev[p] = c.x[p];
        }
        let r = if step == 0 { ratio } else { 1.0 };
        for ((x, prev), &inv_mass) in c.x.iter_mut().zip(c.prev.iter_mut()).zip(w) {
            if inv_mass <= 0.0 {
                continue;
            }
            let v = (*x - *prev) * keep * r;
            *prev = *x;
            *x += v + def.gravity * h * h;
        }
        for _ in 0..def.iterations {
            for &e in &def.execution {
                if e == -1 {
                    collide(&mut c.x, w, &def.radius, &def.masks, capsules);
                    continue;
                }
                let Some(set) = usize::try_from(e).ok().and_then(|e| def.sets.get(e)) else {
                    continue;
                };
                match set {
                    Set::Standard(ls) => ls.iter().for_each(|l| solve_link(&mut c.x, w, l, false)),
                    Set::Stretch(ls) => ls.iter().for_each(|l| solve_link(&mut c.x, w, l, true)),
                    Set::LocalRange {
                        items,
                        stiffness,
                        normal,
                    } => {
                        for &(p, r, max, max_n, min_n) in items {
                            let (Some(Some((q, n))), true) =
                                (refs.get(r), p < c.x.len() && w[p] > 0.0)
                            else {
                                continue;
                            };
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
                            let (rest, k) = if len < min {
                                (min, bend_k)
                            } else if len > max {
                                (max, stretch_k)
                            } else {
                                continue;
                            };
                            solve_link(
                                &mut c.x,
                                w,
                                &Link {
                                    a: pa,
                                    b: pb,
                                    rest,
                                    stiffness: k,
                                },
                                false,
                            );
                        }
                    }
                    Set::BendStiffness {
                        links,
                        max_height_sq,
                        clamp,
                        rest_pose,
                    } => links.iter().for_each(|l| {
                        solve_bend_stiffness(&mut c.x, w, l, *max_height_sq, *clamp, *rest_pose)
                    }),
                    Set::Unsolved => {}
                }
            }
        }
    }
}

/// The simulation, one task per character. Reads the skeletons, writes only into each cloth's
/// `out`; [`apply`] puts the results into the world.
#[allow(clippy::type_complexity)]
fn simulate(
    time: Res<Time>,
    mut actors: Query<(&mut Cloths, Option<&PresentationLod>, Ref<Rig>)>,
    globals: Query<&GlobalTransform>,
    transforms: Query<&Transform>,
    children: Query<&Children>,
    cameras: Query<&GlobalTransform, With<Camera3d>>,
) {
    let _timer = SystemTimer::start("cloth simulate");
    let dt = time.delta_secs().min(MAX_DT);
    let camera = cameras.iter().next().map(|t| t.translation());
    let debug = debug_now(&time);
    let (globals, transforms, children) = (&globals, &transforms, &children);
    actors.par_iter_mut().for_each(|(mut cloths, lod, rig)| {
        // Far actors carry the restart pose only when their pose changes.
        // Hidden actors neither skin CPU cloth vertices nor dirty mesh assets for upload.
        if lod.is_some_and(|l| !l.draw || (!l.pose && !rig.is_changed())) {
            for c in &mut cloths.0 {
                c.out.bones.clear();
                c.out.meshes.clear();
                if lod.is_some_and(|l| !l.draw) {
                    c.started = false;
                }
            }
            return;
        }
        // World transforms this character's cloth-driven bones (and what hangs from them) take
        // this frame, ahead of `apply`: the display meshes skin from them.
        let mut overrides: HashMap<Entity, GlobalTransform> = HashMap::new();
        for c in cloths.0.iter_mut() {
            step(
                c,
                dt,
                camera,
                debug,
                lod.is_some_and(|l| !l.cloth),
                globals,
                transforms,
                children,
                &mut overrides,
            );
        }
    });
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn step(
    c: &mut ClothInst,
    dt: f32,
    camera: Option<Vec3>,
    debug: bool,
    force_rigid: bool,
    globals: &Query<&GlobalTransform>,
    transforms: &Query<&Transform>,
    children: &Query<&Children>,
    overrides: &mut HashMap<Entity, GlobalTransform>,
) {
    c.out.bones.clear();
    c.out.meshes.clear();
    let bone = |e: Option<Entity>| {
        e.and_then(|e| globals.get(e).ok())
            .map(|g| havok(g.to_matrix()))
    };
    let root = globals.get(c.owner).ok().copied();
    let defs = Arc::clone(&c.def);
    let def = &defs[c.which];
    // Out of range: carried rigidly (the restart pose every frame), no simulation. The range
    // is measured to the model root plus the cloth's own rest reach.
    let culled = c.rigid
        || force_rigid
        || match (camera, root) {
            (Some(cam), Some(r)) => r.translation().distance(cam) > cloth_range() + def.rest_far,
            _ => false,
        };
    // One world matrix per reference bone, and the collision capsules in Havok world space.
    let ref_mats: Vec<Option<Mat4>> = c.ref_ents.iter().map(|&e| bone(e)).collect();
    let capsules: Vec<Option<(Vec3, Vec3, f32, f32)>> = def
        .capsules
        .iter()
        .zip(&c.capsule_ents)
        .map(|(k, &e)| {
            bone(e).map(|m| {
                let m = m * k.offset;
                (m.transform_point3(k.a), m.transform_point3(k.b), k.ra, k.rb)
            })
        })
        .collect();
    // 1. The reference vertices (Havok world space).
    let refs: Vec<Option<(Vec3, Vec3)>> = def
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
        .collect();
    let Some(root) = root else { return };
    // First frame or a teleport (a fixed particle jumped): restart from the rest pose placed on
    // the model root. The jump that counts scales with the cloth (half its extent, 1 m at
    // least): a 4 m fur restarted every few frames when its owner just turned.
    let jump = (0.5 * def.rest_far).max(1.0);
    let jumped = c.started
        && !culled
        && (c.x.len() != def.rest.len()
            || def.moves.iter().any(|&(v, p)| {
                refs.get(v)
                    .and_then(|r| *r)
                    .is_some_and(|(q, _)| c.x.get(p).is_none_or(|x| q.distance(*x) > jump))
            }));
    let restarted = !c.started || jumped || culled;
    if restarted {
        if jumped {
            info!("cloth {}: restart (fixed particle jumped)", def.name);
        }
        let m = havok(root.to_matrix());
        c.x = def.rest.iter().map(|&p| m.transform_point3(p)).collect();
        c.prev = c.x.clone();
        c.started = true;
    }
    solve(c, &refs, &capsules, dt, culled, restarted);
    if debug {
        report(c, &refs, &capsules, culled);
    }
    // A particle more than 3 m further from the model root than the cloth's own reference
    // vertices (or its rest pose) means the sim blew up: log and restart.
    let r = havok(root.to_matrix()).w_axis.truncate();
    let reach = refs
        .iter()
        .flatten()
        .map(|(q, _)| q.distance(r))
        .fold(def.rest_far, f32::max)
        + 3.0;
    if let Some((i, far)) =
        c.x.iter()
            .map(|p| p.distance(r))
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .filter(|(_, d)| d.is_nan() || *d >= reach)
    {
        c.blowups += 1;
        if c.blowups < GIVE_UP {
            warn!(
                "cloth {}: particle {i} {far:.1} m from the root (inverse mass {}, dt {dt:.3}); restarting",
                def.name, def.inv_mass[i]
            );
        } else if !c.rigid {
            warn!(
                "cloth {}: blew up {GIVE_UP} frames running; carried rigidly from now on",
                def.name
            );
            c.rigid = true;
        }
        c.started = false;
    } else if !culled {
        c.blowups = 0;
    }
    // 4a. The cloth-driven bones (and whatever hangs from them).
    for ((_, t, local), e) in def.bones.iter().zip(&c.bone_ents) {
        let (Some(e), Some(&tri)) = (*e, def.tris.get(*t)) else {
            continue;
        };
        let g = havok(tri_frame(&c.x, tri, false) * *local);
        let gt = GlobalTransform::from(Affine3A::from_mat4(g));
        propagate(e, gt, transforms, children, overrides);
        c.out.bones.push((e, gt));
    }
    // 4b. The display meshes.
    if c.displays.is_empty() {
        return;
    }
    let frames: Vec<Mat4> = def
        .tris
        .iter()
        .map(|&t| tri_frame(&c.x, t, false))
        .collect();
    let unit_frames: Option<Vec<Mat4>> = def
        .displays
        .iter()
        .any(|d| d.unit_normal)
        .then(|| def.tris.iter().map(|&t| tri_frame(&c.x, t, true)).collect());
    for dm in &c.displays {
        let Some(mesh_world) = globals.get(dm.entity).ok().map(|g| g.affine()) else {
            continue;
        };
        let to_local = mesh_world.inverse();
        let joint_m: Vec<Mat4> = dm
            .joints
            .iter()
            .zip(&dm.inverse_bind)
            .map(|(&j, inv)| {
                overrides
                    .get(&j)
                    .copied()
                    .or_else(|| globals.get(j).ok().copied())
                    .map_or(Mat4::IDENTITY, |g| g.to_matrix() * *inv)
            })
            .collect();
        let (mut pos, mut normal, tangent_dir) = skin(dm, &joint_m);
        let (mut flipped, mut worst_move) = (0usize, 0f32);
        let (mut signed, mut moved_sum, mut counted) = (0f32, 0f32, 0usize);
        for display in def.displays.iter().filter(|d| d.mesh == dm.mesh) {
            let frames = if display.unit_normal {
                unit_frames.as_deref().unwrap_or(&frames)
            } else {
                &frames
            };
            for (v, slots) in &display.verts {
                let (mut p, mut n) = (Vec4::ZERO, Vec4::ZERO);
                for s in slots {
                    if let Some(f) = frames.get(s.tri) {
                        p += *f * s.p;
                        n += *f * s.n;
                    }
                }
                if let (Some(dp), Some(dn)) = (pos.get_mut(*v), normal.get_mut(*v)) {
                    let (np, nn) = (
                        mirror(p.truncate()),
                        mirror(n.truncate()).normalize_or_zero(),
                    );
                    if debug {
                        let moved = np.distance(Vec3::from(*dp));
                        let turned = nn.dot(Vec3::from(*dn));
                        flipped += usize::from(turned < 0.0);
                        worst_move = worst_move.max(moved);
                        signed += (np - Vec3::from(*dp)).dot(Vec3::from(*dn));
                        moved_sum += moved;
                        counted += 1;
                    }
                    *dp = np.into();
                    // Position-only deformers carry no normals: keep the skinned one.
                    if nn != Vec3::ZERO {
                        *dn = nn.into();
                    }
                }
            }
        }
        if debug {
            info!(
                "cloth {}: mesh {}: {flipped} normals turned over, cloth vertices up to {worst_move:.3} m (mean {:.4}, along the normal {:.4}) from their skinned place",
                def.name,
                dm.mesh,
                moved_sum / counted.max(1) as f32,
                signed / counted.max(1) as f32
            );
        }
        // Back from world space to the mesh entity's own space, which is what an unskinned mesh
        // is drawn in.
        for p in &mut pos {
            *p = to_local.transform_point3(Vec3::from(*p)).into();
        }
        for n in &mut normal {
            *n = to_local
                .transform_vector3(Vec3::from(*n))
                .normalize_or_zero()
                .into();
        }
        // Tangents: the bind tangent carried by the vertex's skin (it turns with the body),
        // made perpendicular to the deformed normal. Regenerating them every frame flips them
        // across UV seams and lights scattered pixels dark.
        let tangent: Vec<[f32; 4]> = if dm.tangent.is_empty() {
            Vec::new()
        } else {
            tangent_dir
                .iter()
                .zip(&normal)
                .zip(&dm.tangent)
                .map(|((t, n), bt)| {
                    let n = Vec3::from(*n);
                    let t = to_local.transform_vector3(*t);
                    let mut t = (t - n * n.dot(t)).normalize_or_zero();
                    if t == Vec3::ZERO {
                        t = n.any_orthonormal_vector();
                    }
                    [t.x, t.y, t.z, bt[3]]
                })
                .collect()
        };
        c.out.meshes.push(MeshOut {
            handle: dm.handle.clone(),
            pos,
            normal,
            tangent,
        });
    }
}

/// Skins a display mesh's vertices (position, normal, bind tangent direction) from its joints,
/// in parallel chunks on the compute pool when the mesh is big.
#[allow(clippy::type_complexity)]
fn skin(dm: &DisplayMesh, joint_m: &[Mat4]) -> (Vec<[f32; 3]>, Vec<[f32; 3]>, Vec<Vec3>) {
    const CHUNK: usize = 4096;
    let n = dm.pos.len();
    let one = |range: std::ops::Range<usize>| {
        let mut pos: Vec<[f32; 3]> = Vec::with_capacity(range.len());
        let mut nrm: Vec<[f32; 3]> = Vec::with_capacity(range.len());
        let mut tan: Vec<Vec3> = Vec::with_capacity(range.len());
        for i in range {
            let (p, nn) = (Vec3::from(dm.pos[i]), Vec3::from(dm.normal[i]));
            let bt = dm
                .tangent
                .get(i)
                .map_or(Vec3::ZERO, |t| Vec3::new(t[0], t[1], t[2]));
            let (mut sp, mut sn, mut st) = (Vec3::ZERO, Vec3::ZERO, Vec3::ZERO);
            for k in 0..4 {
                let wt = dm.weights[i][k];
                if wt <= 0.0 && k > 0 {
                    continue;
                }
                let m = joint_m
                    .get(dm.idx[i][k] as usize)
                    .copied()
                    .unwrap_or(Mat4::IDENTITY);
                sp += m.transform_point3(p) * wt;
                sn += m.transform_vector3(nn) * wt;
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
    let parts = bevy::tasks::ComputeTaskPool::get().scope(|scope| {
        for start in (0..n).step_by(CHUNK) {
            let one = &one;
            scope.spawn(async move { one(start..(start + CHUNK).min(n)) });
        }
    });
    let (mut pos, mut nrm, mut tan) = (
        Vec::with_capacity(n),
        Vec::with_capacity(n),
        Vec::with_capacity(n),
    );
    for (p, nn, t) in parts {
        pos.extend(p);
        nrm.extend(nn);
        tan.extend(t);
    }
    (pos, nrm, tan)
}

/// Writes the frame's cloth results: the driven bones' world transforms (propagated to what
/// hangs from them) and the display meshes' vertices.
fn apply(
    mut actors: Query<&mut Cloths>,
    mut globals: Query<&mut GlobalTransform>,
    transforms: Query<&Transform>,
    children: Query<&Children>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut bounds: Query<&mut Aabb>,
) {
    let _timer = SystemTimer::start("cloth apply");
    for mut cloths in &mut actors {
        for c in cloths.0.iter_mut() {
            for (e, g) in c.out.bones.drain(..) {
                set_global(e, g, &mut globals, &transforms, &children);
            }
            for out in c.out.meshes.drain(..) {
                if let Some(mut mesh) = meshes.get_mut(&out.handle) {
                    // CPU cloth replaces GPU skinning, so publish bounds from the actual output.
                    if let Some(aabb) = Aabb::enclosing(out.pos.iter().copied().map(Vec3::from)) {
                        for dm in c.displays.iter().filter(|d| d.handle == out.handle) {
                            if let Ok(mut bound) = bounds.get_mut(dm.entity) {
                                *bound = aabb;
                            }
                        }
                    }
                    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, out.pos);
                    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, out.normal);
                    if !out.tangent.is_empty() {
                        mesh.insert_attribute(Mesh::ATTRIBUTE_TANGENT, out.tangent);
                    }
                }
            }
        }
    }
}

/// `SEKIRO_CLOTH_DEBUG=<seconds>`: true for the one frame that crosses that time, when every
/// cloth logs its state.
fn debug_now(time: &Time) -> bool {
    static AT: std::sync::OnceLock<Option<f32>> = std::sync::OnceLock::new();
    let Some(at) = *AT.get_or_init(|| {
        std::env::var("SEKIRO_CLOTH_DEBUG")
            .ok()
            .and_then(|v| v.parse::<f32>().ok())
    }) else {
        return false;
    };
    time.elapsed_secs() > at && time.elapsed_secs() - time.delta_secs() <= at
}

fn report(
    c: &ClothInst,
    refs: &[Option<(Vec3, Vec3)>],
    capsules: &[Option<(Vec3, Vec3, f32, f32)>],
    culled: bool,
) {
    let def = &c.def[c.which];
    let gaps: Vec<f32> = def
        .sets
        .iter()
        .filter_map(|s| match s {
            Set::LocalRange { items, .. } => Some(items),
            _ => None,
        })
        .flatten()
        .filter_map(|&(p, r, ..)| {
            refs.get(r)
                .and_then(|v| *v)
                .map(|(q, _)| c.x.get(p).map_or(0.0, |x| x.distance(q)))
        })
        .collect();
    for (i, set) in def.sets.iter().enumerate() {
        if let Set::LocalRange {
            items,
            stiffness,
            normal,
        } = set
        {
            let (mut over, mut worst, mut sum_max) = (0, 0f32, 0f32);
            for &(p, r, max, ..) in items {
                sum_max += max;
                if let (Some(Some((q, _))), Some(x)) = (refs.get(r), c.x.get(p)) {
                    let gap = x.distance(*q);
                    if gap > max * 1.05 + 1e-3 {
                        over += 1;
                        worst = worst.max(gap - max);
                    }
                }
            }
            info!(
                "  set {i} local range: {} items, mean max {:.3}, stiffness {stiffness}, normal {normal}, {over} beyond their limit (worst +{worst:.3})",
                items.len(),
                sum_max / items.len().max(1) as f32
            );
        }
    }
    info!(
        "  execution {:?}, {} substeps x {} iterations, gravity {:?}, damping {}",
        def.execution, def.substeps, def.iterations, def.gravity, def.damping
    );
    info!(
        "cloth {}: {} particles ({} fixed), sets {}, capsules {}/{} resolved, ref gap mean {:.3} max {:.3}, displays {}, culled {culled}",
        def.name,
        def.rest.len(),
        def.inv_mass.iter().filter(|w| **w <= 0.0).count(),
        def.sets.len(),
        capsules.iter().flatten().count(),
        capsules.len(),
        gaps.iter().sum::<f32>() / gaps.len().max(1) as f32,
        gaps.iter().copied().fold(0.0, f32::max),
        c.displays.len()
    );
}

/// A bone's world transform this frame and, from it, its children's (as `set_global`, into a map
/// instead of the world).
fn propagate(
    e: Entity,
    g: GlobalTransform,
    transforms: &Query<&Transform>,
    children: &Query<&Children>,
    out: &mut HashMap<Entity, GlobalTransform>,
) {
    out.insert(e, g);
    if let Ok(kids) = children.get(e) {
        for &k in kids {
            if let Ok(t) = transforms.get(k) {
                propagate(k, g.mul_transform(*t), transforms, children, out);
            }
        }
    }
}

/// Overrides a bone's world transform after propagation, and re-propagates to its children.
fn set_global(
    e: Entity,
    g: GlobalTransform,
    globals: &mut Query<&mut GlobalTransform>,
    transforms: &Query<&Transform>,
    children: &Query<&Children>,
) {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn chain(n: usize) -> (Vec<Vec3>, Vec<f32>, Vec<Link>) {
        let x: Vec<Vec3> = (0..n).map(|i| Vec3::new(i as f32, 0.0, 0.0)).collect();
        let mut w = vec![1.0; n];
        w[0] = 0.0;
        let links = (0..n - 1)
            .map(|i| Link {
                a: i,
                b: i + 1,
                rest: 1.0,
                stiffness: 1.0,
            })
            .collect();
        (x, w, links)
    }

    #[test]
    fn links_pull_a_stretched_pair_back_to_rest_length() {
        let (mut x, w, links) = chain(2);
        x[1].x = 3.0;
        solve_link(&mut x, &w, &links[0], false);
        assert!((x[1].x - 1.0).abs() < 1e-6, "{:?}", x);
        assert_eq!(x[0].x, 0.0, "the fixed end does not move");
    }

    #[test]
    fn stretch_links_leave_a_slack_pair_alone() {
        let (mut x, w, links) = chain(2);
        x[1].x = 0.5;
        solve_link(&mut x, &w, &links[0], true);
        assert_eq!(x[1].x, 0.5);
    }

    #[test]
    fn capsules_push_particles_out_by_their_radius() {
        let mut x = vec![Vec3::new(0.0, 0.1, 0.0)];
        let capsule = Some((
            Vec3::new(-1.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            0.5,
            0.5,
        ));
        collide(&mut x, &[1.0], &[0.0], &[1], &[capsule]);
        assert!((x[0].y - 0.5).abs() < 1e-5, "{:?}", x[0]);
        // A particle whose mask has no bit for the capsule is not pushed.
        let mut y = vec![Vec3::new(0.0, 0.1, 0.0)];
        collide(&mut y, &[1.0], &[0.0], &[0], &[capsule]);
        assert_eq!(y[0].y, 0.1);
    }

    #[test]
    fn mirror_is_its_own_inverse() {
        let m = Mat4::from_rotation_y(0.7) * Mat4::from_translation(Vec3::new(1.0, 2.0, 3.0));
        assert!(havok(havok(m)).abs_diff_eq(m, 1e-5));
        assert_eq!(
            mirror(mirror(Vec3::new(1.0, 2.0, 3.0))),
            Vec3::new(1.0, 2.0, 3.0)
        );
    }

    #[test]
    fn a_hanging_chain_swings_down_under_gravity() {
        // Three particles from a fixed one, gravity along -Y: the free ones fall until the
        // links stop them, hanging below the pin.
        let n = 4;
        let (mut x, w, links) = chain(n);
        let mut prev = x.clone();
        let g = Vec3::new(0.0, -10.0, 0.0);
        let h: f32 = 1.0 / 120.0;
        for _ in 0..2000 {
            for i in 0..n {
                if w[i] > 0.0 {
                    let v = (x[i] - prev[i]) * 0.98;
                    prev[i] = x[i];
                    x[i] += v + g * h * h;
                }
            }
            for _ in 0..8 {
                links.iter().for_each(|l| solve_link(&mut x, &w, l, false));
            }
        }
        for (i, p) in x.iter().enumerate().skip(1) {
            assert!(p.y < -0.5 * i as f32, "particle {i} at {p:?}");
            assert!((p.distance(x[i - 1]) - 1.0).abs() < 1e-2);
        }
    }
}
