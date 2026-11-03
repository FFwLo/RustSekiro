//! Havok Cloth (`<chr>_c.hkx`, `hclClothContainer` tagfiles): the per-character cloth simulations
//! FromSoftware uses for scarves, robes, capes, sashes, sleeves and hair.
//!
//! Every cloth is flattened to the pieces a runtime solver needs, in the file's own (FromSoftware,
//! left-handed) model space, so the runtime needs one formula per kind of deformer:
//!
//! - skin (reference buffer): `p = sum(boneWorld[bone] * local)` (`local.w` is the weight, normals
//!   carry `w = 0`);
//! - mesh-mesh deform (display vertices): `p = sum(triFrame(now) * local)`;
//! - mesh-bone deform (cloth-driven bones): `boneWorld = triFrame(now) * local`.
//!
//! The deformer block layouts, the triangle frame convention (columns `v0 - c`, `v1 - c`, their
//! cross product, and the centroid `c`) and the operator chain of the `#01#` state follow FwLo's
//! MIT-licensed RustSekiro (`tools/sekiro-extract/src/cloth.rs` and `cloth_export.rs`), checked
//! here against the FLVER bind pose of the same model: every reference skin and display deform
//! must reproduce the rest pose (the exported error figures say so).

use std::collections::HashSet;

use glam::{Mat4, Vec3, Vec4};
use serde::{Deserialize, Serialize};

use super::{TagFile, Value, read_skeleton};
use crate::flver::Flver;
use crate::reader::{Result, bail};

/// All the cloths of one exported model.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ClothSet {
    pub cloths: Vec<Cloth>,
}

impl ClothSet {
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        postcard::to_stdvec(self).map_err(|e| crate::Error::Format(e.to_string()))
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        postcard::from_bytes(data).map_err(|e| crate::Error::Format(e.to_string()))
    }
}

/// One simulated cloth (an `hclClothData` with its first `hclSimClothData`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Cloth {
    pub name: String,
    /// The model part (FLVER file label) whose display meshes this cloth deforms.
    pub part: String,
    pub gravity: [f32; 3],
    /// `globalDampingPerSecond`: the velocity fraction removed per second.
    pub damping: f32,
    /// Rest pose, inverse mass (0 = fixed) and radius per particle.
    pub particles: Vec<Particle>,
    /// Simulation triangles (particle indices).
    pub tris: Vec<[u16; 3]>,
    pub constraints: Vec<Constraint>,
    pub substeps: u32,
    pub iterations: u32,
    /// Order the constraint sets are solved in; `-1` is the collision pass. Empty means every set
    /// in stored order, collisions last.
    pub execution: Vec<i32>,
    pub collidables: Vec<Collidable>,
    /// One bit per collidable, per particle.
    pub masks: Vec<u32>,
    /// `hclMoveParticlesOperator`: (reference vertex, particle) pairs for the fixed particles.
    pub moves: Vec<(u16, u16)>,
    /// The bones the reference vertices skin from.
    pub ref_bones: Vec<String>,
    /// Per reference vertex: its bone entries (empty when unused).
    pub refs: Vec<Vec<RefEntry>>,
    pub displays: Vec<Display>,
    pub bones: Vec<DrivenBone>,
    /// The largest distance between a deformer's rest-pose result and the rest pose it should
    /// reproduce (reference skin against the particles, display deform against the FLVER).
    pub bind_error: f32,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
pub struct Particle {
    pub position: [f32; 3],
    pub inv_mass: f32,
    pub radius: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Constraint {
    /// Keeps the rest length both ways: (a, b, rest, stiffness).
    Standard(Vec<(u16, u16, f32, f32)>),
    /// Only stops stretching past the rest length.
    Stretch(Vec<(u16, u16, f32, f32)>),
    /// (a, b, bendMinLength, stretchMaxLength, bendStiffness, stretchStiffness).
    Bend(Vec<(u16, u16, f32, f32, f32, f32)>),
    /// `hclBendStiffnessConstraintSet`.
    BendStiffness {
        links: Vec<BendStiffnessLink>,
        max_rest_pose_height_sq: f32,
        clamp: bool,
        use_rest_pose: bool,
    },
    /// (particle, reference vertex, max distance, max normal distance, min normal distance).
    LocalRange {
        items: Vec<(u16, u16, f32, f32, f32)>,
        stiffness: f32,
        normal: bool,
    },
    /// Present only to keep the set indices of `execution` meaningful.
    Other(String),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct BendStiffnessLink {
    pub weights: [f32; 4],
    pub stiffness: f32,
    pub rest_curvature: f32,
    pub particles: [u16; 4],
}

/// A capsule on a bone: end points in the bone's offset frame.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Collidable {
    pub name: String,
    pub bone: String,
    /// Column-major offset matrix from the collidable's frame to the bone.
    pub offset: [[f32; 4]; 4],
    pub a: [f32; 3],
    pub b: [f32; 3],
    pub radius_a: f32,
    pub radius_b: f32,
}

/// One reference-vertex skin entry: (index into [`Cloth::ref_bones`], local position with the
/// weight in `w`, local normal).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct RefEntry {
    pub bone: u16,
    pub local: [f32; 4],
    pub normal: [f32; 4],
}

/// A display mesh that follows the cloth.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Display {
    /// FLVER mesh index within the part.
    pub mesh: u32,
    /// The deformer's triangle frames have a unit-length normal axis (the position-only
    /// deformers) instead of the plain cross product of the two edge vectors.
    pub unit_normal: bool,
    pub verts: Vec<DisplayVertex>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct DisplayVertex {
    pub index: u32,
    /// Triangle slots: (sim triangle, local position, local normal).
    pub slots: Vec<(u16, [f32; 4], [f32; 4])>,
}

/// A skeleton bone that rides a sim triangle (`hclSimpleMeshBoneDeformOperator`):
/// `boneWorld = triFrame * local`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DrivenBone {
    pub bone: String,
    pub tri: u16,
    pub local: [[f32; 4]; 4],
}

/// Reads every cloth of a `_c.hkx` tagfile for the model part `part` whose FLVER is `flver`.
/// Display buffers are matched to FLVER meshes by node name and vertex count and checked against
/// the FLVER's bind positions. `log` receives one line per cloth and per anomaly.
pub fn read(
    file: &TagFile,
    part: &str,
    flver: &Flver,
    log: &mut dyn FnMut(String),
) -> Result<Vec<Cloth>> {
    let skeleton = read_skeleton(file.first("hkaSkeleton")?)?;
    let bone_names: Vec<String> = skeleton.bones.iter().map(|b| b.name.clone()).collect();
    let bind = skeleton.model_space(&skeleton.reference_pose());
    let mut cloths = Vec::new();
    for data in file.objects_of("hclClothData") {
        match read_cloth(&data, part, flver, &bone_names, &bind, log) {
            Ok(Some(c)) => cloths.push(c),
            Ok(None) => {}
            Err(e) => log(format!("  cloth {}: skipped: {e}", name_of(&data))),
        }
    }
    Ok(cloths)
}

fn name_of(v: &Value<'_>) -> String {
    v.get("name").and_then(|n| n.string()).unwrap_or_default()
}

fn ptr_list<'a>(v: Value<'a>, member: &str) -> Result<Vec<Value<'a>>> {
    let mut out = Vec::new();
    for e in v.get(member)?.elements()? {
        if let Some(o) = e.deref()? {
            out.push(o);
        }
    }
    Ok(out)
}

/// Raw little-endian reads at absolute offsets in the DATA section (out of range reads zero).
struct Mem<'a>(&'a [u8]);

impl Mem<'_> {
    fn bytes<const N: usize>(&self, at: usize) -> [u8; N] {
        self.0
            .get(at..at + N)
            .and_then(|s| s.try_into().ok())
            .unwrap_or([0; N])
    }
    fn u8(&self, at: usize) -> u8 {
        self.bytes::<1>(at)[0]
    }
    fn u16(&self, at: usize) -> u16 {
        u16::from_le_bytes(self.bytes(at))
    }
    fn i16(&self, at: usize) -> i16 {
        i16::from_le_bytes(self.bytes(at))
    }
    fn u32(&self, at: usize) -> u32 {
        u32::from_le_bytes(self.bytes(at))
    }
    fn f32(&self, at: usize) -> f32 {
        f32::from_le_bytes(self.bytes(at))
    }
    fn v4(&self, at: usize) -> [f32; 4] {
        [
            self.f32(at),
            self.f32(at + 4),
            self.f32(at + 8),
            self.f32(at + 12),
        ]
    }
    fn m4(&self, at: usize) -> Mat4 {
        Mat4::from_cols_array(&std::array::from_fn(|i| self.f32(at + i * 4)))
    }
}

fn u16s(v: Value<'_>, member: &str) -> Result<Vec<u16>> {
    Ok(v.get(member)?
        .elements()?
        .iter()
        .map(|e| Mem(&e.file.data).u16(e.offset))
        .collect())
}

fn m4s(v: Value<'_>, member: &str) -> Result<Vec<Mat4>> {
    Ok(v.get(member)?
        .elements()?
        .iter()
        .map(|e| Mem(&e.file.data).m4(e.offset))
        .collect())
}

fn cols(m: &Mat4) -> [[f32; 4]; 4] {
    m.to_cols_array_2d()
}

fn round6(v: f32) -> f32 {
    ((v as f64 * 1e6).round() / 1e6) as f32
}

fn v4(v: Vec4) -> [f32; 4] {
    [round6(v.x), round6(v.y), round6(v.z), round6(v.w)]
}

/// The triangle frame used by the mesh-mesh and mesh-bone deformers: columns `v0 - c`, `v1 - c`,
/// their cross product, and the centroid `c`.
pub fn tri_frame(v0: Vec3, v1: Vec3, v2: Vec3) -> Mat4 {
    let c = (v0 + v1 + v2) / 3.0;
    let (a, b) = (v0 - c, v1 - c);
    Mat4::from_cols(
        a.extend(0.0),
        b.extend(0.0),
        a.cross(b).extend(0.0),
        c.extend(1.0),
    )
}

/// The triangle frame with a unit-length third axis (the frame of the position-only
/// `...MeshMeshDeformPOperator` deformers).
pub fn tri_frame_unit(v0: Vec3, v1: Vec3, v2: Vec3) -> Mat4 {
    let mut f = tri_frame(v0, v1, v2);
    f.z_axis = f.z_axis.truncate().normalize_or_zero().extend(0.0);
    f
}

/// One deformed vertex before flattening: (vertex, [(slot, weight)], local position, local
/// normal).
type DeformEntry = (u16, Vec<(u16, f32)>, [f32; 4], [f32; 4]);

/// A flattened deformer entry: (vertex, [(bone or triangle slot, local position with the weight
/// premultiplied, local normal)]).
type Flat = Vec<(u16, Vec<(u16, Vec4, Vec4)>)>;

/// hkPackedVector3: three i16 mantissas in the high half of a float, scaled by the float whose
/// high half is the fourth value.
fn unpack(m: &Mem<'_>, at: usize) -> [f32; 4] {
    let v = [m.i16(at), m.i16(at + 2), m.i16(at + 4), m.i16(at + 6)];
    let scale = f32::from_bits((v[3] as u16 as u32) << 16);
    let f = |x: i16| ((x as i32) << 16) as f32 * scale;
    [f(v[0]), f(v[1]), f(v[2]), scale]
}

/// `hclObjectSpaceDeformer`: blocks of 16 vertices, N-blend lists picked in `controlBytes` order
/// (byte = 3 - blend count + 1: 3 one blend, 2 two, 1 three, 0 four); local positions and normals
/// come in the same block order.
fn object_space_deformer(op: Value<'_>) -> Result<Vec<DeformEntry>> {
    let m = Mem(&op.file.data);
    let def = op.get("objectSpaceDeformer")?;
    let lists = [
        "oneBlendEntries",
        "twoBlendEntries",
        "threeBlendEntries",
        "fourBlendEntries",
    ];
    let arrays = lists
        .iter()
        .map(|l| def.get(l)?.elements())
        .collect::<Result<Vec<_>>>()?;
    let ctrl = def.get("controlBytes")?.ints()?;
    let first = |names: &[&str]| -> Result<Vec<Value<'_>>> {
        match names.iter().find(|n| op.has(n)) {
            Some(n) => op.get(n)?.elements(),
            None => Ok(Vec::new()),
        }
    };
    let packed = first(&["localPNs", "localPNTs", "localPs"])?;
    let unpacked = first(&["localUnpackedPNs", "localUnpackedPNTs", "localUnpackedPs"])?;
    let mut next = [0usize; 4];
    let mut out = Vec::new();
    for (block, &c) in ctrl.iter().enumerate() {
        let Some(k) = 3usize.checked_sub(c as usize) else {
            return bail("deformer control byte above 3 (more than four blends) is not decoded");
        };
        let Some(entry) = arrays[k].get(next[k]) else {
            return bail("deformer has more control bytes than blend entries");
        };
        next[k] += 1;
        let b = entry.offset;
        let nb = k + 1;
        for v in 0..16 {
            let vertex = m.u16(b + v * 2);
            let bones = (0..nb)
                .map(|j| {
                    let bone = m.u16(b + 32 + (v * nb + j) * 2);
                    let w = if nb == 1 {
                        1.0
                    } else {
                        m.u8(b + 32 + 16 * nb * 2 + v * nb + j) as f32 / 255.0
                    };
                    (bone, w)
                })
                .collect();
            let (pos, nrm) = if let Some(e) = unpacked.get(block) {
                (m.v4(e.offset + v * 16), m.v4(e.offset + 256 + v * 16))
            } else if let Some(e) = packed.get(block) {
                (
                    unpack(&m, e.offset + v * 8),
                    unpack(&m, e.offset + 128 + v * 8),
                )
            } else {
                ([0.0; 4], [0.0; 4])
            };
            out.push((vertex, bones, pos, nrm));
        }
    }
    Ok(out)
}

/// `hclBoneSpaceDeformer`: blocks of 16 (vertex, bone) pairs (4 vertices x 4 bones, 5 x 3, 8 x 2,
/// 16 x 1) in `controlBytes` order; 16 local vectors per block, premultiplied by the weight.
fn bone_space_deformer(op: Value<'_>) -> Result<Flat> {
    let m = Mem(&op.file.data);
    let def = op.get("boneSpaceDeformer")?;
    let lists = [
        "oneBlendEntries",
        "twoBlendEntries",
        "threeBlendEntries",
        "fourBlendEntries",
    ];
    let arrays = lists
        .iter()
        .map(|l| def.get(l)?.elements())
        .collect::<Result<Vec<_>>>()?;
    let ctrl = def.get("controlBytes")?.ints()?;
    let first = |names: &[&str]| -> Result<Vec<Value<'_>>> {
        match names.iter().find(|n| op.has(n)) {
            Some(n) => op.get(n)?.elements(),
            None => Ok(Vec::new()),
        }
    };
    let locals = first(&["localPNs", "localPNTs", "localPNTBs", "localPs"])?;
    let locals_unpacked = first(&[
        "localUnpackedPNs",
        "localUnpackedPNTs",
        "localUnpackedPNTBs",
        "localUnpackedPs",
    ])?;
    let blocks = if locals_unpacked.is_empty() {
        &locals
    } else {
        &locals_unpacked
    };
    let mut next = [0usize; 4];
    let mut out = Vec::new();
    for (block, &c) in ctrl.iter().enumerate() {
        let Some(k) = 3usize.checked_sub(c as usize) else {
            continue;
        };
        // More control bytes than entries of that kind (some cloths): stop at the array's end.
        let Some(entry) = arrays[k].get(next[k]) else {
            continue;
        };
        next[k] += 1;
        let b = entry.offset;
        let nb = k + 1;
        let nv = 16 / nb;
        let bone_off = nv * 2;
        for v in 0..nv {
            let vertex = m.u16(b + v * 2);
            let mut slots = Vec::new();
            for j in 0..nb {
                let slot = v * nb + j;
                let bone = m.u16(b + bone_off + slot * 2);
                let local = blocks
                    .get(block)
                    .map_or([0.0; 4], |e| m.v4(e.offset + slot * 16));
                // The position-and-normal blocks (384 bytes) follow the 16 positions with 16
                // packed normals.
                let normal = blocks
                    .get(block)
                    .filter(|e| e.file.types.size(e.ty) >= 384)
                    .map_or([0.0; 4], |e| {
                        let n = unpack(&m, e.offset + 256 + slot * 8);
                        [n[0], n[1], n[2], 0.0]
                    });
                slots.push((bone, Vec4::from(local), Vec4::from(normal)));
            }
            out.push((vertex, slots));
        }
    }
    Ok(out)
}

/// Premultiplies object-space entries by their per-slot matrices (`boneFromSkinMesh` or
/// `triangleFromMesh`), remapping slots through `subset`.
fn flatten_object(entries: Vec<DeformEntry>, mats: &[Mat4], subset: &[u16]) -> Flat {
    let map = |b: u16| subset.get(b as usize).copied().unwrap_or(b);
    entries
        .into_iter()
        .map(|(v, bs, l, n)| {
            let (lp, ln) = (
                Vec4::new(l[0], l[1], l[2], 1.0),
                Vec4::new(n[0], n[1], n[2], 0.0),
            );
            let out = bs
                .into_iter()
                .map(|(b, w)| {
                    let m = mats.get(b as usize).copied().unwrap_or(Mat4::IDENTITY);
                    (map(b), (m * lp) * w, (m * ln) * w)
                })
                .collect();
            (v, out)
        })
        .collect()
}

fn flatten_bone(op: Value<'_>, subset: &[u16]) -> Result<Flat> {
    let map = |b: u16| subset.get(b as usize).copied().unwrap_or(b);
    Ok(bone_space_deformer(op)?
        .into_iter()
        .map(|(v, bs)| (v, bs.into_iter().map(|(b, l, n)| (map(b), l, n)).collect()))
        .collect())
}

fn skin_entries(op: Value<'_>) -> Result<Flat> {
    let subset = u16s(op, "transformSubset")?;
    if op.type_name().starts_with("hclBoneSpace") {
        return flatten_bone(op, &subset);
    }
    Ok(flatten_object(
        object_space_deformer(op)?,
        &m4s(op, "boneFromSkinMeshTransforms")?,
        &subset,
    ))
}

fn mesh_deform_entries(op: Value<'_>) -> Result<Flat> {
    let subset = u16s(op, "inputTrianglesSubset")?;
    if op.type_name().starts_with("hclBoneSpace") {
        return flatten_bone(op, &subset);
    }
    // triangleFromMeshTransforms are indexed by subset slot; the subset maps the slot to the
    // triangle.
    let mats = m4s(op, "triangleFromMeshTransforms")?;
    let map = |b: u16| subset.get(b as usize).copied().unwrap_or(b);
    Ok(flatten_object(object_space_deformer(op)?, &mats, &[])
        .into_iter()
        .map(|(v, bs)| (v, bs.into_iter().map(|(b, l, n)| (map(b), l, n)).collect()))
        .collect())
}

/// Solves the frame's linear part for a direction: `m_lin * x = v` (None when degenerate).
fn inv3_apply(m: &Mat4, v: Vec3) -> Option<Vec3> {
    let (a, b, c) = (
        m.x_axis.truncate(),
        m.y_axis.truncate(),
        m.z_axis.truncate(),
    );
    let det = a.dot(b.cross(c));
    if det.abs() < 1e-12 {
        return None;
    }
    Some(Vec3::new(
        v.dot(b.cross(c)) / det,
        a.dot(v.cross(c)) / det,
        a.dot(b.cross(v)) / det,
    ))
}

fn constraint_set(set: Value<'_>) -> Result<Constraint> {
    let m = Mem(&set.file.data);
    let rows = |member: &str| -> Result<Vec<usize>> {
        Ok(set
            .get(member)?
            .elements()?
            .iter()
            .map(|e| e.offset)
            .collect())
    };
    let ty = set.type_name();
    Ok(match ty {
        "hclStandardLinkConstraintSet" | "hclStretchLinkConstraintSet" => {
            let links = rows("links")?
                .into_iter()
                .map(|e| {
                    (
                        m.u16(e),
                        m.u16(e + 2),
                        round6(m.f32(e + 4)),
                        round6(m.f32(e + 8)),
                    )
                })
                .collect();
            if ty.contains("Stretch") {
                Constraint::Stretch(links)
            } else {
                Constraint::Standard(links)
            }
        }
        "hclBendLinkConstraintSet" => Constraint::Bend(
            rows("links")?
                .into_iter()
                .map(|e| {
                    (
                        m.u16(e),
                        m.u16(e + 2),
                        round6(m.f32(e + 4)),
                        round6(m.f32(e + 8)),
                        round6(m.f32(e + 12)),
                        round6(m.f32(e + 16)),
                    )
                })
                .collect(),
        ),
        "hclBendStiffnessConstraintSet" => Constraint::BendStiffness {
            links: rows("links")?
                .into_iter()
                .map(|e| BendStiffnessLink {
                    weights: [m.f32(e), m.f32(e + 4), m.f32(e + 8), m.f32(e + 12)],
                    stiffness: m.f32(e + 16),
                    rest_curvature: m.f32(e + 20),
                    particles: [m.u16(e + 24), m.u16(e + 26), m.u16(e + 28), m.u16(e + 30)],
                })
                .collect(),
            max_rest_pose_height_sq: set.get("maxRestPoseHeightSq")?.f32()?,
            clamp: set.get("clampBendStiffness")?.int()? != 0,
            use_rest_pose: set.get("useRestPoseConfig")?.int()? != 0,
        },
        "hclLocalRangeConstraintSet" => Constraint::LocalRange {
            items: rows("localConstraints")?
                .into_iter()
                .map(|e| {
                    (
                        m.u16(e),
                        m.u16(e + 2),
                        round6(m.f32(e + 4)),
                        round6(m.f32(e + 8)),
                        round6(m.f32(e + 12)),
                    )
                })
                .collect(),
            stiffness: set.get("stiffness")?.f32()?,
            normal: set.get("applyNormalComponent")?.int()? != 0,
        },
        other => Constraint::Other(other.to_string()),
    })
}

fn read_cloth(
    data: &Value<'_>,
    part: &str,
    flver: &Flver,
    bone_names: &[String],
    bind: &[Mat4],
    log: &mut dyn FnMut(String),
) -> Result<Option<Cloth>> {
    let name = name_of(data);
    let m = Mem(&data.file.data);
    let buffers = ptr_list(*data, "bufferDefinitions")?;
    let buf_name = |i: i32| {
        buffers
            .get(i as usize)
            .and_then(|b| b.get("name").ok()?.string().ok())
            .unwrap_or_default()
    };
    let buf_verts = |i: i32| {
        buffers
            .get(i as usize)
            .and_then(|b| b.get("numVertices").ok()?.int().ok())
            .unwrap_or(0)
            .max(0) as usize
    };
    let ops = ptr_list(*data, "operators")?;
    let states = ptr_list(*data, "clothStateDatas")?;
    let state = states
        .iter()
        .find(|s| name_of(s).starts_with("#01#"))
        .or(states.first());
    let Some(state) = state else {
        return Ok(None);
    };
    let state_ops: Vec<usize> = state
        .get("operators")?
        .ints()?
        .into_iter()
        .map(|i| i as usize)
        .collect();
    let Some(sim) = ptr_list(*data, "simClothDatas")?.into_iter().next() else {
        return Ok(None);
    };
    let pose: Vec<Vec3> = match ptr_list(sim, "simClothPoses")?.first() {
        Some(p) => p
            .get("positions")?
            .elements()?
            .iter()
            .map(|e| Vec3::new(m.f32(e.offset), m.f32(e.offset + 4), m.f32(e.offset + 8)))
            .collect(),
        None => Vec::new(),
    };
    let particle_data = sim.get("particleDatas")?.elements()?;
    let particles: Vec<Particle> = pose
        .iter()
        .enumerate()
        .map(|(i, q)| {
            let (inv_mass, radius) = particle_data
                .get(i)
                .map_or((1.0, 0.01), |e| (m.f32(e.offset + 4), m.f32(e.offset + 8)));
            Particle {
                position: [round6(q.x), round6(q.y), round6(q.z)],
                inv_mass: round6(inv_mass),
                radius: round6(radius),
            }
        })
        .collect();
    let tri_indices = u16s(sim, "triangleIndices")?;
    let tris: Vec<[u16; 3]> = tri_indices.as_chunks::<3>().0.to_vec();
    let info = sim.get("simulationInfo")?;
    let g = info.get("gravity")?.floats()?;
    let constraints = ptr_list(sim, "staticConstraintSets")?
        .into_iter()
        .map(constraint_set)
        .collect::<Result<Vec<_>>>()?;

    // Collidables: capsules on bones, placed by collidableTransformMap.
    let ctm = sim.get("collidableTransformMap")?;
    let ctm_bones = ctm.get("transformIndices")?.ints()?;
    let ctm_offsets = m4s(ctm, "offsets")?;
    let mut collidables = Vec::new();
    for (i, c) in ptr_list(sim, "perInstanceCollidables")?
        .into_iter()
        .enumerate()
    {
        let Some(shape) = c.get("shape")?.deref()? else {
            continue;
        };
        let v3 = |v: Value<'_>| -> Result<[f32; 3]> {
            let f = v.floats()?;
            Ok([f[0], f[1], f[2]])
        };
        let (a, b, ra, rb) = match shape.type_name() {
            "hclCapsuleShape" => {
                let r = shape.get("radius")?.f32()?;
                (v3(shape.get("start")?)?, v3(shape.get("end")?)?, r, r)
            }
            "hclTaperedCapsuleShape" => (
                v3(shape.get("small")?)?,
                v3(shape.get("big")?)?,
                shape.get("smallRadius")?.f32()?,
                shape.get("bigRadius")?.f32()?,
            ),
            other => {
                log(format!(
                    "  cloth {name}: collidable shape {other} not supported"
                ));
                continue;
            }
        };
        let Some(bone) = ctm_bones
            .get(i)
            .and_then(|&b| bone_names.get(b as usize))
            .cloned()
        else {
            continue;
        };
        collidables.push(Collidable {
            name: name_of(&c),
            bone,
            offset: cols(&ctm_offsets.get(i).copied().unwrap_or(Mat4::IDENTITY)),
            a,
            b,
            radius_a: ra,
            radius_b: rb,
        });
    }
    let masks: Vec<u32> = sim
        .get("staticCollisionMasks")?
        .elements()?
        .iter()
        .map(|e| m.u32(e.offset))
        .collect();

    let frame_with = |t: u16, unit: bool| -> Option<Mat4> {
        let tri = tris.get(t as usize)?;
        let (a, b, c) = (
            *pose.get(tri[0] as usize)?,
            *pose.get(tri[1] as usize)?,
            *pose.get(tri[2] as usize)?,
        );
        Some(if unit {
            tri_frame_unit(a, b, c)
        } else {
            tri_frame(a, b, c)
        })
    };
    let frame_of = |t: u16| frame_with(t, false);

    let mut cloth = Cloth {
        name: name.clone(),
        part: part.to_string(),
        gravity: [g[0], g[1], g[2]],
        damping: info.get("globalDampingPerSecond")?.f32()?,
        particles,
        tris: tris.clone(),
        constraints,
        substeps: 1,
        iterations: 1,
        execution: Vec::new(),
        collidables,
        masks,
        ..Cloth::default()
    };

    let mut ref_buf = -1;
    let mut skin_ops: Vec<(i32, Flat)> = Vec::new();
    let mut moved = false;
    for &oi in &state_ops {
        let Some(op) = ops.get(oi) else { continue };
        let ty = op.type_name();
        if ty.contains("Skin") && !ty.contains("MeshMesh") {
            skin_ops.push((
                op.get("outputBufferIndex")?.int()? as i32,
                skin_entries(*op)?,
            ));
        } else if ty == "hclMoveParticlesOperator" {
            // The operator of this container's first sim cloth only: containers with two sim
            // cloths (c1060's mino, the handmaidens' sleeves) have a second operator whose
            // pairs name particles of the other sim.
            let sim_index = op.get("simClothIndex")?.int()?;
            if sim_index != 0 || moved {
                continue;
            }
            moved = true;
            ref_buf = op.get("refBufferIdx")?.int()? as i32;
            let n = cloth.particles.len() as u16;
            cloth.moves = op
                .get("vertexParticlePairs")?
                .elements()?
                .iter()
                .map(|e| (m.u16(e.offset), m.u16(e.offset + 2)))
                .filter(|&(_, p)| p < n)
                .collect();
        } else if ty == "hclSimulateOperator" {
            if let Some(config) = op.get("simulateOpConfigs")?.elements()?.first() {
                cloth.substeps = config.get("subSteps")?.int()?.max(1) as u32;
                cloth.iterations = config.get("numberOfSolveIterations")?.int()?.max(1) as u32;
                cloth.execution = config
                    .get("constraintExecution")?
                    .ints()?
                    .into_iter()
                    .map(|e| e as i32)
                    .collect();
            }
        } else if ty.contains("MeshMeshDeform") {
            let out = op.get("outputBufferIdx")?.int()? as i32;
            let entries = mesh_deform_entries(*op)?;
            let (bname, nv) = (buf_name(out), buf_verts(out));
            // Which frame the deformer was authored with is decided by the rest pose: the right
            // one reproduces the FLVER to float precision, the other misses by centimetres.
            let mut best: Option<(usize, bool, f32)> = None;
            for (mi, mesh) in flver.meshes.iter().enumerate() {
                let node = flver
                    .nodes
                    .get(mesh.node.max(0) as usize)
                    .map_or("", |n| n.name.as_str());
                if node != bname || mesh.vertices.len() != nv {
                    continue;
                }
                for unit in [false, true] {
                    let mut worst = 0f32;
                    for (vi, slots) in &entries {
                        let mut acc = Vec4::ZERO;
                        for (t, l, _) in slots {
                            if let Some(f) = frame_with(*t, unit) {
                                acc += f * *l;
                            }
                        }
                        if let Some(p) = mesh.vertices.positions.get(*vi as usize) {
                            worst = worst
                                .max((0..3).map(|k| (p[k] - acc[k]).abs()).fold(0.0, f32::max));
                        }
                    }
                    if best.is_none_or(|b| worst < b.2) {
                        best = Some((mi, unit, worst));
                    }
                }
            }
            let Some((mesh, unit_normal, err)) = best else {
                log(format!(
                    "  cloth {name}: no FLVER mesh for display buffer {bname} ({nv} verts)"
                ));
                continue;
            };
            cloth.bind_error = cloth.bind_error.max(err);
            // How well the stored normals reproduce the FLVER's at rest: the mean cosine over
            // the deformed vertices that carry one (zero for position-only deformers).
            let mesh_ref = &flver.meshes[mesh];
            let (mut cos_sum, mut counted, mut without) = (0f32, 0usize, 0usize);
            for (vi, slots) in &entries {
                let mut n = Vec4::ZERO;
                for (t, _, ln) in slots {
                    if let Some(f) = frame_with(*t, unit_normal) {
                        n += f * *ln;
                    }
                }
                let n = n.truncate();
                if n.length() < 1e-9 {
                    without += 1;
                } else if let Some(b) = mesh_ref.vertices.normals.get(*vi as usize) {
                    cos_sum += n.normalize().dot(Vec3::from(*b).normalize_or_zero());
                    counted += 1;
                }
            }
            log(format!(
                "  cloth {name}: mesh {mesh} normals: mean cosine {:.4} over {counted}, {without} without",
                cos_sum / counted.max(1) as f32
            ));
            log(format!(
                "  cloth {name}: {ty} -> mesh {mesh} ({bname}, {nv} verts, {} deformed), bind err {err:.4}",
                entries.len()
            ));
            let mut seen = HashSet::new();
            cloth.displays.push(Display {
                mesh: mesh as u32,
                unit_normal,
                verts: entries
                    .iter()
                    .filter(|(v, _)| seen.insert(*v))
                    .map(|(v, slots)| DisplayVertex {
                        index: *v as u32,
                        slots: slots.iter().map(|(t, l, n)| (*t, v4(*l), v4(*n))).collect(),
                    })
                    .collect(),
            });
        } else if ty == "hclSimpleMeshBoneDeformOperator" {
            let locals = m4s(*op, "localBoneTransforms")?;
            for (i, e) in op.get("triangleBonePairs")?.elements()?.iter().enumerate() {
                // boneOffset / triangleOffset are byte offsets (hkMatrix4 = 64, triangle = 3 x u16).
                let b = m.u16(e.offset) as usize / 64;
                let t = m.u16(e.offset + 2) / 6;
                let local = locals.get(i).copied().unwrap_or(Mat4::IDENTITY);
                let err = frame_of(t).zip(bind.get(b)).map(|(f, bw)| {
                    let prod = f * local;
                    prod.to_cols_array()
                        .iter()
                        .zip(bw.to_cols_array())
                        .map(|(a, b)| (a - b).abs())
                        .fold(0.0f32, f32::max)
                });
                // A pair that does not reproduce its bone's bind pose is padding, not a driven
                // bone (c1020 sholderarmor's second pair).
                if err.is_none_or(|e| e > 0.01) {
                    continue;
                }
                let Some(bone) = bone_names.get(b) else {
                    continue;
                };
                cloth.bones.push(DrivenBone {
                    bone: bone.clone(),
                    tri: t,
                    local: cols(&local),
                });
            }
        }
    }

    // No MeshMeshDeform display: the sim mesh is the drawn mesh itself (fur, hair, bandages: the
    // sim buffer is gathered straight into a display buffer of the FLVER mesh's name). Each
    // vertex rides its particle's own triangle at a corner of the centroid frame (local (1,0,0),
    // (0,1,0) or (-1,-1,0): exact for any deformation), with the FLVER bind normal taken into
    // that frame.
    if cloth.displays.is_empty() && !tris.is_empty() {
        let n = pose.len();
        let is_sim = |i: i32| buf_name(i).contains("SimBuffer");
        for &oi in &state_ops {
            let Some(op) = ops.get(oi) else { continue };
            let ty = op.type_name();
            if ty != "hclCopyVerticesOperator" && ty != "hclGatherAllVerticesOperator" {
                continue;
            }
            let (inp, out) = (
                op.get("inputBufferIdx")?.int()? as i32,
                op.get("outputBufferIdx")?.int()? as i32,
            );
            let (bname, nv) = (buf_name(out), buf_verts(out));
            if !is_sim(inp)
                || is_sim(out)
                || bname.starts_with("Previous Positions")
                || bname.is_empty()
            {
                continue;
            }
            let map: Vec<Option<u16>> = if ty == "hclGatherAllVerticesOperator" {
                u16s(*op, "vertexInputFromVertexOutput")?
                    .into_iter()
                    .map(|i| (usize::from(i) < n).then_some(i))
                    .collect()
            } else {
                (0..nv.min(n) as u16).map(Some).collect()
            };
            let Some(mi) = flver.meshes.iter().position(|mesh| {
                mesh.vertices.len() == nv
                    && flver
                        .nodes
                        .get(mesh.node.max(0) as usize)
                        .is_some_and(|nd| nd.name == bname)
                    && mesh.vertices.positions.iter().zip(&map).all(|(v, pi)| {
                        pi.is_none_or(|pi| {
                            (0..3).all(|k| (v[k] - pose[pi as usize][k]).abs() < 0.01)
                        })
                    })
            }) else {
                log(format!(
                    "  cloth {name}: no FLVER mesh for the gathered sim buffer {bname} ({nv} verts)"
                ));
                continue;
            };
            let corner = [
                Vec4::new(1.0, 0.0, 0.0, 1.0),
                Vec4::new(0.0, 1.0, 0.0, 1.0),
                Vec4::new(-1.0, -1.0, 0.0, 1.0),
            ];
            let mesh = &flver.meshes[mi];
            let verts: Vec<DisplayVertex> = map
                .iter()
                .enumerate()
                .filter_map(|(v, pi)| {
                    let pi = (*pi)?;
                    let (t, k) = tris.iter().enumerate().find_map(|(t, tri)| {
                        tri.iter().position(|&c| c == pi).map(|k| (t as u16, k))
                    })?;
                    let f = frame_of(t)?;
                    let nrm = mesh
                        .vertices
                        .normals
                        .get(v)
                        .copied()
                        .unwrap_or([0.0, 1.0, 0.0]);
                    let ln = inv3_apply(&f, Vec3::from(nrm))?;
                    Some(DisplayVertex {
                        index: v as u32,
                        slots: vec![(t, v4(corner[k]), v4(ln.extend(0.0)))],
                    })
                })
                .collect();
            log(format!(
                "  cloth {name}: sim mesh is the display -> mesh {mi} ({bname}: {} of {nv} verts on a particle's triangle, {ty})",
                verts.len()
            ));
            cloth.displays.push(Display {
                mesh: mi as u32,
                unit_normal: false,
                verts,
            });
            break;
        }
    }

    // Reference buffer skin: drives the fixed particles and the local-range targets.
    let mut ref_err = 0f32;
    let mut refs: Vec<Vec<RefEntry>> = vec![Vec::new(); buf_verts(ref_buf)];
    for (_, entries) in skin_ops.iter().filter(|(o, _)| *o == ref_buf) {
        for (vi, bs) in entries {
            let mut acc = Vec4::ZERO;
            for (b, l, _) in bs {
                acc += bind.get(*b as usize).copied().unwrap_or(Mat4::IDENTITY) * *l;
            }
            if refs.len() == pose.len()
                && let Some(q) = pose.get(*vi as usize)
            {
                ref_err = ref_err.max((0..3).map(|k| (q[k] - acc[k]).abs()).fold(0.0, f32::max));
            }
            if let Some(slot) = refs.get_mut(*vi as usize) {
                *slot = bs
                    .iter()
                    .filter_map(|(b, l, n)| {
                        let bone = bone_names.get(*b as usize)?;
                        let idx = match cloth.ref_bones.iter().position(|x| x == bone) {
                            Some(i) => i,
                            None => {
                                cloth.ref_bones.push(bone.clone());
                                cloth.ref_bones.len() - 1
                            }
                        };
                        Some(RefEntry {
                            bone: idx as u16,
                            local: v4(*l),
                            normal: v4(*n),
                        })
                    })
                    .collect();
            }
        }
    }
    cloth.refs = refs;
    cloth.bind_error = cloth.bind_error.max(ref_err);
    log(format!(
        "  cloth {name}: {} particles, {} ref verts (bind err {ref_err:.4}), {} displays, {} bones, {} sets, {} substeps x {} iterations",
        cloth.particles.len(),
        cloth.refs.len(),
        cloth.displays.len(),
        cloth.bones.len(),
        cloth.constraints.len(),
        cloth.substeps,
        cloth.iterations
    ));
    Ok(Some(cloth))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn triangle_frame_of_a_right_triangle() {
        let f = tri_frame(Vec3::ZERO, Vec3::X * 3.0, Vec3::Y * 3.0);
        assert_eq!(f.w_axis, Vec4::new(1.0, 1.0, 0.0, 1.0));
        assert_eq!(f.x_axis, Vec4::new(-1.0, -1.0, 0.0, 0.0));
        assert_eq!(f.y_axis, Vec4::new(2.0, -1.0, 0.0, 0.0));
        assert!(f.z_axis.z > 0.0);
    }

    #[test]
    fn packed_vectors_scale_the_widened_mantissas() {
        // The fourth word is the high half of the scale float (0x3F80 -> 1.0); each mantissa
        // is widened by 16 bits before scaling.
        let words: [i16; 4] = [1, -1, 0, 0x3F80];
        let bytes: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
        assert_eq!(unpack(&Mem(&bytes), 0), [65536.0, -65536.0, 0.0, 1.0]);
    }

    #[test]
    fn clothset_roundtrips() {
        let set = ClothSet {
            cloths: vec![Cloth {
                name: "scarf".into(),
                part: "BD_M_9040".into(),
                particles: vec![Particle::default()],
                constraints: vec![Constraint::Other("hclTransitionConstraintSet".into())],
                ..Cloth::default()
            }],
        };
        assert_eq!(ClothSet::from_bytes(&set.to_bytes().unwrap()).unwrap(), set);
    }

    /// The installed characters' cloth reproduces its rest pose through every deformer.
    #[test]
    fn installed_cloth_reproduces_the_bind_pose() {
        let raw = crate::cache::root().join("raw/chr");
        let mut checked = 0;
        for id in ["c1010", "c5060", "c7100", "c7110"] {
            let dir = raw.join(format!("{id}.chrbnd.d"));
            let (Ok(hkx), Ok(flver)) = (
                std::fs::read(dir.join(format!("{id}_c.hkx"))),
                std::fs::read(dir.join(format!("{id}.flver"))),
            ) else {
                eprintln!("skipping {id}: not unpacked");
                continue;
            };
            let flver = crate::flver::parse(&flver).unwrap();
            let file = TagFile::parse(&hkx, None).unwrap();
            let mut lines = Vec::new();
            let cloths = read(&file, id, &flver, &mut |l| lines.push(l)).unwrap();
            assert!(
                !cloths.is_empty(),
                "{id}: no cloth read
{}",
                lines.join(
                    "
"
                )
            );
            for c in &cloths {
                assert!(
                    c.bind_error < 0.01,
                    "{id} {}: bind error {}
{}",
                    c.name,
                    c.bind_error,
                    lines.join(
                        "
"
                    )
                );
                assert!(c.particles.len() == c.masks.len() || c.masks.is_empty());
                checked += 1;
            }
        }
        eprintln!("{checked} cloths checked");
    }
}
