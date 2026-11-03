//! FLVER2 model reader (Sekiro uses version 0x2001A).
//!
//! Layout knowledge comes from SoulsFormatsNEXT (`FLVER/FLVER2/*.cs`, `FLVER/*.cs`),
//! reimplemented here. Only little-endian PC files are supported, and EdgeGeom compressed
//! buffers (a PS3 feature) are rejected.
//!
//! Everything is kept in FromSoftware's own coordinate system (left-handed, +Y up, +Z forward).
//! Conversion to glTF/Bevy happens in the exporter; see `docs/FORMATS.md`.

use crate::reader::{Reader, Result, bail, sjisz, utf16z};

/// Face set flag bits.
pub mod face_set_flags {
    pub const LOD1: u32 = 0x0100_0000;
    pub const LOD2: u32 = 0x0200_0000;
    pub const LOD_EX: u32 = 0x0400_0000;
    pub const EDGE_COMPRESSED: u32 = 0x4000_0000;
    pub const MOTION_BLUR: u32 = 0x8000_0000;
}

#[derive(Debug, Clone)]
pub struct Flver {
    pub version: u32,
    pub bbox_min: [f32; 3],
    pub bbox_max: [f32; 3],
    pub dummies: Vec<Dummy>,
    pub materials: Vec<Material>,
    pub nodes: Vec<Node>,
    pub meshes: Vec<Mesh>,
    pub layouts: Vec<Vec<LayoutMember>>,
}

/// A dummy polygon: a named attachment point (hitbox ends, effect spawns, weapon sockets).
#[derive(Debug, Clone)]
pub struct Dummy {
    pub position: [f32; 3],
    /// ARGB as stored.
    pub color: [u8; 4],
    pub forward: [f32; 3],
    pub reference_id: i16,
    /// Node the position is expressed relative to (usually a root-level node).
    pub parent_bone: i16,
    pub upward: [f32; 3],
    /// Node the dummy follows when animated.
    pub attach_bone: i16,
    pub flag1: bool,
    pub use_upward: bool,
}

impl Dummy {
    /// The dummy's frame in its parent node's space (source space): +Z along `forward`, +Y along
    /// `upward` made orthogonal to it, origin at `position`. A part held on a dummy (the katana
    /// on WepAbsorpPosParam `right_0`, its scabbard on `right_1`) has this frame as its model
    /// space, as FwLo's MIT RustSekiro (`model.rs::dummy_frame`) found and the real game shows.
    pub fn frame(&self) -> glam::Mat4 {
        use glam::Vec3;
        let z = Vec3::from(self.forward).normalize_or(Vec3::Z);
        let up = Vec3::from(self.upward).normalize_or(Vec3::Y);
        let x = up.cross(z).normalize_or(Vec3::X);
        let y = z.cross(x);
        glam::Mat4::from_cols(
            x.extend(0.0),
            y.extend(0.0),
            z.extend(0.0),
            Vec3::from(self.position).extend(1.0),
        )
    }
}

#[derive(Debug, Clone)]
pub struct Material {
    pub name: String,
    pub mtd: String,
    pub textures: Vec<Texture>,
    pub index: i32,
}

#[derive(Debug, Clone)]
pub struct Texture {
    /// Shader parameter name, e.g. `g_DiffuseTexture`.
    pub param: String,
    /// Texture path; often empty in Sekiro (resolved via the MTD instead).
    pub path: String,
    pub scale: [f32; 2],
}

/// A transform node: bones, dummy owners and mesh owners share this table.
#[derive(Debug, Clone)]
pub struct Node {
    pub name: String,
    pub translation: [f32; 3],
    /// Euler angles in radians, applied X then Z then Y (see [`Node::local_matrix`]).
    pub rotation: [f32; 3],
    pub scale: [f32; 3],
    pub parent: i16,
    pub first_child: i16,
    pub next_sibling: i16,
    pub previous_sibling: i16,
    pub bbox_min: [f32; 3],
    pub bbox_max: [f32; 3],
    pub flags: i32,
}

impl Node {
    /// Local transform as a column-vector matrix (`world = parent * local`), in the file's own
    /// coordinate system. SoulsFormats composes S * Rx * Rz * Ry * T in row-vector convention,
    /// which in column-vector form is T * Ry * Rz * Rx * S.
    pub fn local_matrix(&self) -> glam::Mat4 {
        let [rx, ry, rz] = self.rotation;
        let rotation = glam::Quat::from_rotation_y(ry)
            * glam::Quat::from_rotation_z(rz)
            * glam::Quat::from_rotation_x(rx);
        glam::Mat4::from_scale_rotation_translation(
            self.scale.into(),
            rotation,
            self.translation.into(),
        )
    }

    pub fn local_rotation(&self) -> glam::Quat {
        let [rx, ry, rz] = self.rotation;
        glam::Quat::from_rotation_y(ry)
            * glam::Quat::from_rotation_z(rz)
            * glam::Quat::from_rotation_x(rx)
    }
}

#[derive(Debug, Clone)]
pub struct Mesh {
    /// True when vertices carry bone weights.
    pub dynamic: bool,
    pub material: usize,
    /// Node that owns the mesh (its default bone).
    pub node: i32,
    /// Mesh-local bone palette. Empty in Sekiro: vertex bone indices then index the node table.
    pub bone_indices: Vec<i32>,
    pub face_sets: Vec<FaceSet>,
    pub buffers: Vec<VertexBufferInfo>,
    pub vertices: Vertices,
}

impl Mesh {
    /// The LOD0, non-motion-blur face set, if any.
    pub fn lod0(&self) -> Option<&FaceSet> {
        self.face_sets.iter().find(|f| f.flags == 0)
    }
}

#[derive(Debug, Clone)]
pub struct FaceSet {
    pub flags: u32,
    pub triangle_strip: bool,
    pub cull_backfaces: bool,
    pub indices: Vec<u32>,
}

impl FaceSet {
    /// Authored detail level, independent of motion-blur and other face flags.
    pub fn detail_level(&self) -> usize {
        if self.flags & face_set_flags::LOD_EX != 0 {
            3
        } else if self.flags & face_set_flags::LOD2 != 0 {
            2
        } else if self.flags & face_set_flags::LOD1 != 0 {
            1
        } else {
            0
        }
    }

    /// Triangle list indices. Strips are unrolled with alternating winding, dropping
    /// degenerate triangles and honouring 0xFFFF primitive restarts.
    pub fn triangles(&self, vertex_count: usize) -> Vec<u32> {
        if !self.triangle_strip {
            return self.indices.clone();
        }
        let restarts = vertex_count < 0xFFFF;
        let mut out = Vec::with_capacity(self.indices.len() * 3);
        let mut flip = false;
        for w in self.indices.windows(3) {
            let (a, b, c) = (w[0], w[1], w[2]);
            if restarts && (a == 0xFFFF || b == 0xFFFF || c == 0xFFFF) {
                flip = false;
                continue;
            }
            if a != b && b != c && a != c {
                if flip {
                    out.extend([c, b, a]);
                } else {
                    out.extend([a, b, c]);
                }
            }
            flip = !flip;
        }
        out
    }
}

#[derive(Debug, Clone, Copy)]
pub struct VertexBufferInfo {
    pub layout: usize,
    pub vertex_size: usize,
    pub vertex_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayoutMember {
    pub kind: LayoutType,
    pub semantic: LayoutSemantic,
    pub index: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutType {
    Float1,
    Float2,
    Float3,
    Float4,
    Color,
    UByte4,
    Byte4,
    UByte4Norm,
    Byte4Norm,
    Short2,
    Short4,
    UShort2,
    UShort4,
    Short4Norm,
    Half2,
    Half4,
    Byte4E,
}

impl LayoutType {
    fn from_u32(v: u32) -> Result<Self> {
        use LayoutType::*;
        Ok(match v {
            0 => Float1,
            1 => Float2,
            2 => Float3,
            3 => Float4,
            16 => Color,
            17 => UByte4,
            18 => Byte4,
            19 => UByte4Norm,
            20 => Byte4Norm,
            21 => Short2,
            22 => Short4,
            23 => UShort2,
            24 => UShort4,
            26 => Short4Norm,
            45 => Half2,
            46 => Half4,
            47 => Byte4E,
            240 => return bail("EdgeGeom compressed vertex layouts are not supported"),
            _ => return bail(format!("unknown FLVER layout type {v}")),
        })
    }

    pub fn size(self) -> usize {
        use LayoutType::*;
        match self {
            Float1 | Color | UByte4 | Byte4 | UByte4Norm | Byte4Norm | Short2 | UShort2
            | Byte4E | Half2 => 4,
            Float2 | Short4 | UShort4 | Short4Norm | Half4 => 8,
            Float3 => 12,
            Float4 => 16,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutSemantic {
    Position,
    BoneWeights,
    BoneIndices,
    Normal,
    Uv,
    Tangent,
    Bitangent,
    VertexColor,
}

impl LayoutSemantic {
    fn from_u32(v: u32) -> Result<Self> {
        use LayoutSemantic::*;
        Ok(match v {
            0 => Position,
            1 => BoneWeights,
            2 => BoneIndices,
            3 => Normal,
            5 => Uv,
            6 => Tangent,
            7 => Bitangent,
            10 => VertexColor,
            _ => return bail(format!("unknown FLVER layout semantic {v}")),
        })
    }
}

/// Decoded vertex attributes, one entry per vertex in each populated array.
#[derive(Debug, Clone, Default)]
pub struct Vertices {
    pub positions: Vec<[f32; 3]>,
    /// Further position streams (some meshes carry a second one, e.g. for cloth).
    pub extra_positions: Vec<Vec<[f32; 3]>>,
    pub normals: Vec<[f32; 3]>,
    pub extra_normals: Vec<Vec<[f32; 3]>>,
    /// The normal's W component; for unweighted meshes this can carry a bone index.
    pub normal_w: Vec<i32>,
    /// One array per tangent channel; W is the bitangent sign.
    pub tangents: Vec<Vec<[f32; 4]>>,
    pub bitangents: Vec<[f32; 4]>,
    /// One array per UV channel.
    pub uvs: Vec<Vec<[f32; 2]>>,
    /// Packed semantic-5 UByte4Norm streams are material blend data, not texture UVs.
    /// Keep all four bytes and their stream order independently of the actual UV sets.
    pub blends: Vec<Vec<[u8; 4]>>,
    /// One array per colour channel, RGBA 0..1.
    pub colors: Vec<Vec<[f32; 4]>>,
    pub bone_weights: Vec<[f32; 4]>,
    pub bone_indices: Vec<[u16; 4]>,
}

impl Vertices {
    pub fn len(&self) -> usize {
        self.positions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.positions.is_empty()
    }
}

/// Bind-pose model-space matrices of a FLVER node table (`world = parent * local`), in the
/// file's own coordinate system. A broken parent chain stops at the table length.
pub fn world_matrices(nodes: &[Node]) -> Vec<glam::Mat4> {
    fn resolve(
        i: usize,
        nodes: &[Node],
        world: &mut [Option<glam::Mat4>],
        depth: usize,
    ) -> glam::Mat4 {
        if let Some(m) = world[i] {
            return m;
        }
        let local = nodes[i].local_matrix();
        let p = nodes[i].parent;
        let m = if p >= 0 && depth < nodes.len() {
            resolve(p as usize, nodes, world, depth + 1) * local
        } else {
            local
        };
        world[i] = Some(m);
        m
    }
    let mut world = vec![None; nodes.len()];
    (0..nodes.len())
        .map(|i| resolve(i, nodes, &mut world, 0))
        .collect()
}

/// A node's bind-pose transform, split once into what each vertex attribute needs.
struct NodeFrame {
    matrix: glam::Mat4,
    /// Rotation and scale, for tangents and bitangents.
    linear: glam::Mat3,
    /// Inverse transpose of `linear`, for normals (nodes can scale unevenly).
    normal: glam::Mat3,
    /// -1 when the node mirrors, which flips the tangent handedness.
    handedness: f32,
}

impl NodeFrame {
    /// `None` for identity nodes, whose vertices stay as stored.
    fn new(matrix: glam::Mat4) -> Option<Self> {
        if matrix.abs_diff_eq(glam::Mat4::IDENTITY, 1e-6) {
            return None;
        }
        let linear = glam::Mat3::from_mat4(matrix);
        Some(Self {
            matrix,
            linear,
            normal: linear.inverse().transpose(),
            handedness: if linear.determinant() < 0.0 {
                -1.0
            } else {
                1.0
            },
        })
    }
}

impl Mesh {
    /// Node table index for a vertex bone index: through the mesh bone palette when it has
    /// one (out-of-range entries fall back to node 0), otherwise the index itself.
    pub fn palette_node(&self, index: usize) -> usize {
        if self.bone_indices.is_empty() {
            index
        } else {
            self.bone_indices.get(index).copied().unwrap_or(0).max(0) as usize
        }
    }

    /// The mesh's vertices in model space, as the game draws them at rest.
    ///
    /// Unweighted (non-dynamic) vertices are stored relative to the node their normal W
    /// names (through the bone palette, see [`Mesh::palette_node`]), so the renderer places
    /// each one with that node's matrix.
    /// Map objects use this heavily: a bamboo clump keeps every leaf cluster in its own
    /// node's frame, scaled and rotated into place by the node.
    /// The FLVER header bounds only enclose the vertices after this transform.
    /// Weighted meshes are already in model space (bind pose) and come back unchanged, as do
    /// meshes whose vertices all name identity nodes.
    /// `world` is [`world_matrices`] of the same FLVER's nodes.
    ///
    /// A Float3 normal has no W and decodes it as 0, which reads as node 0.
    /// No placed piece or object in m11_00_00_00 (733 models) or m11_01_00_00 (790 models)
    /// has an unweighted Float3 normal, and none has a bone palette, so neither case is
    /// special-cased here.
    pub fn model_space_vertices(&self, world: &[glam::Mat4]) -> std::borrow::Cow<'_, Vertices> {
        use glam::{Mat3, Vec3};
        use std::borrow::Cow;
        let v = &self.vertices;
        if self.dynamic || v.normal_w.len() != v.len() {
            return Cow::Borrowed(v);
        }
        let frames: Vec<Option<NodeFrame>> = world.iter().map(|&m| NodeFrame::new(m)).collect();
        let frame_of = |i: usize| {
            usize::try_from(v.normal_w[i])
                .ok()
                .and_then(|b| frames.get(self.palette_node(b)))
                .and_then(Option::as_ref)
        };
        if (0..v.len()).all(|i| frame_of(i).is_none()) {
            return Cow::Borrowed(v);
        }
        let mut out = v.clone();
        let point =
            |m: &NodeFrame, p: [f32; 3]| m.matrix.transform_point3(Vec3::from(p)).to_array();
        let direction = |m: &Mat3, d: [f32; 3]| {
            let d = *m * Vec3::from(d);
            d.try_normalize().unwrap_or(d).to_array()
        };
        for i in 0..v.len() {
            let Some(f) = frame_of(i) else { continue };
            out.positions[i] = point(f, v.positions[i]);
            for s in &mut out.extra_positions {
                if let Some(p) = s.get_mut(i) {
                    *p = point(f, *p);
                }
            }
            if let Some(n) = out.normals.get_mut(i) {
                *n = direction(&f.normal, *n);
            }
            for s in &mut out.extra_normals {
                if let Some(n) = s.get_mut(i) {
                    *n = direction(&f.normal, *n);
                }
            }
            for s in &mut out.tangents {
                if let Some(t) = s.get_mut(i) {
                    let [x, y, z] = direction(&f.linear, [t[0], t[1], t[2]]);
                    *t = [x, y, z, t[3] * f.handedness];
                }
            }
            // Bitangents are not exported and what their W means under a mirroring node is
            // unverified, so only the direction is transformed and W is kept as stored.
            if let Some(t) = out.bitangents.get_mut(i) {
                let [x, y, z] = direction(&f.linear, [t[0], t[1], t[2]]);
                *t = [x, y, z, t[3]];
            }
        }
        Cow::Owned(out)
    }
}

pub fn is_flver(data: &[u8]) -> bool {
    data.starts_with(b"FLVER\0")
}

const HEADER_SIZE: usize = 0x80;
const DUMMY_SIZE: usize = 0x40;
const MATERIAL_SIZE: usize = 0x20;
const NODE_SIZE: usize = 0x80;
const MESH_SIZE: usize = 0x30;
const FACE_SET_SIZE: usize = 0x20;
const VERTEX_BUFFER_SIZE: usize = 0x20;
const LAYOUT_SIZE: usize = 0x10;
const TEXTURE_SIZE: usize = 0x20;

pub fn parse(data: &[u8]) -> Result<Flver> {
    let mut r = Reader::new(data);
    r.magic(b"FLVER\0")?;
    if r.bytes(2)? != b"L\0" {
        return bail("big-endian FLVER is not supported");
    }
    let version = r.u32()?;
    if version < 0x20010 {
        return bail(format!(
            "FLVER version {version:#x} is older than supported"
        ));
    }
    let data_offset = r.u32()? as usize;
    let _data_length = r.u32()?;
    let dummy_count = r.u32()? as usize;
    let material_count = r.u32()? as usize;
    let node_count = r.u32()? as usize;
    let mesh_count = r.u32()? as usize;
    let vertex_buffer_count = r.u32()? as usize;
    let bbox_min = r.vec3()?;
    let bbox_max = r.vec3()?;
    r.seek(0x48);
    let header_index_size = r.u8()? as u32;
    let unicode = r.u8()? != 0;
    r.seek(0x50);
    let face_set_count = r.u32()? as usize;
    let layout_count = r.u32()? as usize;
    let texture_count = r.u32()? as usize;

    let string = |offset: usize| -> Result<String> {
        if unicode {
            utf16z(data, offset)
        } else {
            sjisz(data, offset)
        }
    };

    let mut pos = HEADER_SIZE;
    let mut table = |count: usize, size: usize| {
        let start = pos;
        pos += count * size;
        start
    };
    let dummies_at = table(dummy_count, DUMMY_SIZE);
    let materials_at = table(material_count, MATERIAL_SIZE);
    let nodes_at = table(node_count, NODE_SIZE);
    let meshes_at = table(mesh_count, MESH_SIZE);
    let face_sets_at = table(face_set_count, FACE_SET_SIZE);
    let buffers_at = table(vertex_buffer_count, VERTEX_BUFFER_SIZE);
    let layouts_at = table(layout_count, LAYOUT_SIZE);
    let textures_at = table(texture_count, TEXTURE_SIZE);
    crate::reader::slice(data, 0, pos)?;

    let mut dummies = Vec::with_capacity(dummy_count);
    for i in 0..dummy_count {
        let mut r = Reader::at(data, dummies_at + i * DUMMY_SIZE);
        let position = r.vec3()?;
        let color: [u8; 4] = r.bytes(4)?.try_into().unwrap();
        let forward = r.vec3()?;
        let reference_id = r.i16()?;
        let parent_bone = r.i16()?;
        let upward = r.vec3()?;
        let attach_bone = r.i16()?;
        let flag1 = r.u8()? != 0;
        let use_upward = r.u8()? != 0;
        dummies.push(Dummy {
            position,
            color,
            forward,
            reference_id,
            parent_bone,
            upward,
            attach_bone,
            flag1,
            use_upward,
        });
    }

    let mut textures = Vec::with_capacity(texture_count);
    for i in 0..texture_count {
        let mut r = Reader::at(data, textures_at + i * TEXTURE_SIZE);
        let path_offset = r.u32()? as usize;
        let param_offset = r.u32()? as usize;
        let scale = [r.f32()?, r.f32()?];
        textures.push(Texture {
            param: string(param_offset)?,
            path: string(path_offset)?,
            scale,
        });
    }

    let mut materials = Vec::with_capacity(material_count);
    for i in 0..material_count {
        let mut r = Reader::at(data, materials_at + i * MATERIAL_SIZE);
        let name_offset = r.u32()? as usize;
        let mtd_offset = r.u32()? as usize;
        let tex_count = r.u32()? as usize;
        let tex_index = r.u32()? as usize;
        let _string_bytes = r.u32()?;
        let _gx_offset = r.u32()?;
        let index = r.i32()?;
        let Some(tex) = textures.get(tex_index..tex_index + tex_count) else {
            return bail(format!("material {i} texture range out of bounds"));
        };
        materials.push(Material {
            name: string(name_offset)?,
            mtd: string(mtd_offset)?,
            textures: tex.to_vec(),
            index,
        });
    }

    let mut nodes = Vec::with_capacity(node_count);
    for i in 0..node_count {
        let mut r = Reader::at(data, nodes_at + i * NODE_SIZE);
        let translation = r.vec3()?;
        let name_offset = r.u32()? as usize;
        let rotation = r.vec3()?;
        let parent = r.i16()?;
        let first_child = r.i16()?;
        let scale = r.vec3()?;
        let next_sibling = r.i16()?;
        let previous_sibling = r.i16()?;
        let bbox_min = r.vec3()?;
        let flags = r.i32()?;
        let bbox_max = r.vec3()?;
        if parent >= node_count as i16 {
            return bail(format!("node {i} parent {parent} out of range"));
        }
        nodes.push(Node {
            name: string(name_offset)?,
            translation,
            rotation,
            scale,
            parent,
            first_child,
            next_sibling,
            previous_sibling,
            bbox_min,
            flags,
            bbox_max,
        });
    }

    let mut layouts = Vec::with_capacity(layout_count);
    for i in 0..layout_count {
        let mut r = Reader::at(data, layouts_at + i * LAYOUT_SIZE);
        let member_count = r.u32()? as usize;
        r.skip(8);
        let member_offset = r.u32()? as usize;
        let mut members = Vec::with_capacity(member_count);
        let mut m = Reader::at(data, member_offset);
        let mut struct_offset = 0;
        for _ in 0..member_count {
            let _stream = m.i32()?;
            let offset = m.u32()? as usize;
            if offset != struct_offset {
                return bail(format!(
                    "layout {i} member offset {offset} != {struct_offset}"
                ));
            }
            let kind = LayoutType::from_u32(m.u32()?)?;
            let semantic = LayoutSemantic::from_u32(m.u32()?)?;
            let index = m.i32()?;
            struct_offset += kind.size();
            members.push(LayoutMember {
                kind,
                semantic,
                index,
            });
        }
        layouts.push(members);
    }

    let read_face_set = |index: usize| -> Result<FaceSet> {
        let mut r = Reader::at(data, face_sets_at + index * FACE_SET_SIZE);
        let flags = r.u32()?;
        let triangle_strip = r.u8()? != 0;
        let cull_backfaces = r.u8()? != 0;
        let _unk06 = r.i16()?;
        let index_count = r.u32()? as usize;
        let indices_offset = r.u32()? as usize;
        let _length = r.u32()?;
        r.skip(4);
        let mut index_size = r.u32()?;
        if index_size == 0 {
            index_size = header_index_size;
        }
        if flags & face_set_flags::EDGE_COMPRESSED != 0 {
            return bail("EdgeGeom compressed face sets are not supported");
        }
        let start = data_offset + indices_offset;
        let indices = match index_size {
            16 => crate::reader::slice(data, start, index_count * 2)?
                .as_chunks::<2>()
                .0
                .iter()
                .map(|c| u16::from_le_bytes(*c) as u32)
                .collect(),
            32 => crate::reader::slice(data, start, index_count * 4)?
                .as_chunks::<4>()
                .0
                .iter()
                .map(|c| u32::from_le_bytes(*c))
                .collect(),
            other => return bail(format!("unsupported index size {other}")),
        };
        Ok(FaceSet {
            flags,
            triangle_strip,
            cull_backfaces,
            indices,
        })
    };

    let uv_factor = if version >= 0x2000E { 2048.0 } else { 1024.0 };

    let mut meshes = Vec::with_capacity(mesh_count);
    for i in 0..mesh_count {
        let mut r = Reader::at(data, meshes_at + i * MESH_SIZE);
        let dynamic = r.u8()? != 0;
        r.skip(3);
        let material = r.u32()? as usize;
        r.skip(8);
        let node = r.i32()?;
        let bone_count = r.u32()? as usize;
        let _bbox_offset = r.u32()?;
        let bone_offset = r.u32()? as usize;
        let mesh_face_sets = r.u32()? as usize;
        let face_set_offset = r.u32()? as usize;
        let buffer_count = r.u32()? as usize;
        let buffer_offset = r.u32()? as usize;
        if material >= materials.len() {
            return bail(format!("mesh {i} material {material} out of range"));
        }
        let ints = |offset: usize, count: usize| -> Result<Vec<i32>> {
            Ok(crate::reader::slice(data, offset, count * 4)?
                .as_chunks::<4>()
                .0
                .iter()
                .map(|c| i32::from_le_bytes(*c))
                .collect())
        };
        let bone_indices = ints(bone_offset, bone_count)?;
        let mut face_sets = Vec::with_capacity(mesh_face_sets);
        for fs in ints(face_set_offset, mesh_face_sets)? {
            if fs < 0 || fs as usize >= face_set_count {
                return bail(format!("mesh {i} face set {fs} out of range"));
            }
            face_sets.push(read_face_set(fs as usize)?);
        }
        let mut buffers = Vec::with_capacity(buffer_count);
        let mut vertices = Vertices::default();
        let buffer_indices = ints(buffer_offset, buffer_count)?;
        let vertex_count = match buffer_indices.first() {
            Some(&b) => {
                Reader::at(data, buffers_at + b as usize * VERTEX_BUFFER_SIZE + 12).u32()? as usize
            }
            None => 0,
        };
        for b in buffer_indices {
            if b < 0 || b as usize >= vertex_buffer_count {
                return bail(format!("mesh {i} vertex buffer {b} out of range"));
            }
            let mut r = Reader::at(data, buffers_at + b as usize * VERTEX_BUFFER_SIZE);
            let buffer_index = r.u32()?;
            if buffer_index & 0x6000_0000 != 0 {
                return bail("EdgeGeom compressed vertex buffers are not supported");
            }
            let layout = r.u32()? as usize;
            let vertex_size = r.u32()? as usize;
            let count = r.u32()? as usize;
            r.skip(8);
            let _length = r.u32()?;
            let offset = r.u32()? as usize;
            let Some(members) = layouts.get(layout) else {
                return bail(format!("vertex buffer {b} layout {layout} out of range"));
            };
            let layout_size: usize = members.iter().map(|m| m.kind.size()).sum();
            if layout_size != vertex_size {
                return bail(format!(
                    "vertex buffer {b}: stride {vertex_size} != layout size {layout_size}"
                ));
            }
            if count != vertex_count {
                return bail(format!(
                    "vertex buffer {b}: {count} vertices, mesh expects {vertex_count}"
                ));
            }
            let bytes =
                crate::reader::slice(data, data_offset + offset, vertex_size * vertex_count)?;
            read_buffer(bytes, members, vertex_size, uv_factor, &mut vertices)?;
            buffers.push(VertexBufferInfo {
                layout,
                vertex_size,
                vertex_count,
            });
        }
        if vertices.positions.len() != vertex_count {
            return bail(format!(
                "mesh {i}: {} positions for {vertex_count} vertices (layouts {:?})",
                vertices.positions.len(),
                buffers.iter().map(|b| b.layout).collect::<Vec<_>>()
            ));
        }
        meshes.push(Mesh {
            dynamic,
            material,
            node,
            bone_indices,
            face_sets,
            buffers,
            vertices,
        });
    }

    Ok(Flver {
        version,
        bbox_min,
        bbox_max,
        dummies,
        materials,
        nodes,
        meshes,
        layouts,
    })
}

/// Decodes one vertex buffer, appending its streams to `out`.
fn read_buffer(
    bytes: &[u8],
    members: &[LayoutMember],
    stride: usize,
    uv_factor: f32,
    out: &mut Vertices,
) -> Result<()> {
    use LayoutSemantic as S;
    use LayoutType as T;

    // Assign channel slots for repeatable semantics before decoding.
    let mut slots = Vec::with_capacity(members.len());
    let mut position_claimed = !out.positions.is_empty();
    let mut normal_claimed = !out.normals.is_empty();
    for m in members {
        let slot = match m.semantic {
            S::Position if position_claimed => {
                out.extra_positions
                    .push(Vec::with_capacity(bytes.len() / stride));
                out.extra_positions.len()
            }
            S::Position => {
                position_claimed = true;
                0
            }
            S::Normal if normal_claimed => {
                out.extra_normals
                    .push(Vec::with_capacity(bytes.len() / stride));
                out.extra_normals.len()
            }
            S::Normal => {
                normal_claimed = true;
                0
            }
            S::Uv if m.kind == T::UByte4Norm => {
                let slot = usize::try_from(m.index.saturating_sub(1)).unwrap_or(0);
                if out.blends.len() <= slot {
                    out.blends.resize_with(slot + 1, Vec::new);
                }
                out.blends[slot].reserve(bytes.len() / stride);
                slot
            }
            S::Uv => {
                let first = out.uvs.len();
                let n = match m.kind {
                    T::Float4 | T::Short4 | T::Half4 => 2,
                    _ => 1,
                };
                for _ in 0..n {
                    out.uvs.push(Vec::with_capacity(bytes.len() / stride));
                }
                first
            }
            S::Tangent => {
                out.tangents.push(Vec::with_capacity(bytes.len() / stride));
                out.tangents.len() - 1
            }
            S::VertexColor => {
                out.colors.push(Vec::with_capacity(bytes.len() / stride));
                out.colors.len() - 1
            }
            _ => 0,
        };
        slots.push(slot);
    }

    let unsupported = |m: &LayoutMember| {
        bail(format!(
            "unsupported vertex member {:?} {:?}",
            m.kind, m.semantic
        ))
    };
    let ubn = |b: u8| (b as f32 - 127.0) / 127.0;
    let sbn = |b: u8| (b as i8) as f32 / 127.0;
    let shn = |c: &[u8], i: usize| i16::from_le_bytes([c[i * 2], c[i * 2 + 1]]) as f32 / 32767.0;
    let i16_at = |c: &[u8], i: usize| i16::from_le_bytes([c[i * 2], c[i * 2 + 1]]);
    let u16_at = |c: &[u8], i: usize| u16::from_le_bytes([c[i * 2], c[i * 2 + 1]]);
    let f32_at = |c: &[u8], i: usize| f32::from_le_bytes(c[i * 4..i * 4 + 4].try_into().unwrap());

    for vertex in bytes.chunks_exact(stride) {
        let mut at = 0;
        for (m, &slot) in members.iter().zip(&slots) {
            let c = &vertex[at..at + m.kind.size()];
            at += m.kind.size();
            match m.semantic {
                S::Position => match m.kind {
                    T::Float3 | T::Float4 => {
                        let p = [f32_at(c, 0), f32_at(c, 1), f32_at(c, 2)];
                        match slot {
                            0 => out.positions.push(p),
                            k => out.extra_positions[k - 1].push(p),
                        }
                    }
                    _ => return unsupported(m),
                },
                S::BoneWeights => {
                    let w = match m.kind {
                        T::Color => c.map_n(|b| (b as i8) as f32 / 127.0),
                        T::UByte4Norm => c.map_n(|b| b as f32 / 255.0),
                        T::Short4 => {
                            let mut w = [0.0; 4];
                            for (k, v) in w.iter_mut().enumerate() {
                                let raw = u16_at(c, k) as i32;
                                let raw = if raw >= 0x8000 {
                                    raw - 0x8000
                                } else {
                                    raw + 0x8000
                                };
                                *v = raw as f32 / 65535.0;
                            }
                            w
                        }
                        T::Short4Norm => [shn(c, 0), shn(c, 1), shn(c, 2), shn(c, 3)],
                        _ => return unsupported(m),
                    };
                    out.bone_weights.push(w);
                }
                S::BoneIndices => {
                    let idx = match m.kind {
                        T::UByte4 | T::Byte4 | T::Byte4E => {
                            [c[0] as u16, c[1] as u16, c[2] as u16, c[3] as u16]
                        }
                        T::UShort2 => [u16_at(c, 0), u16_at(c, 1), 0, 0],
                        T::UShort4 => [u16_at(c, 0), u16_at(c, 1), u16_at(c, 2), u16_at(c, 3)],
                        _ => return unsupported(m),
                    };
                    out.bone_indices.push(idx);
                }
                S::Normal => {
                    let (n, w) = match m.kind {
                        // No W stored; 0 is indistinguishable from node 0 (see
                        // `Mesh::model_space_vertices` for why that is safe today).
                        T::Float3 => ([f32_at(c, 0), f32_at(c, 1), f32_at(c, 2)], 0),
                        T::Float4 => (
                            [f32_at(c, 0), f32_at(c, 1), f32_at(c, 2)],
                            f32_at(c, 3) as i32,
                        ),
                        T::Color | T::UByte4 | T::UByte4Norm | T::Byte4E => {
                            ([ubn(c[0]), ubn(c[1]), ubn(c[2])], c[3] as i32)
                        }
                        T::Byte4 => ([sbn(c[3]), sbn(c[2]), sbn(c[1])], c[0] as i32),
                        T::Short4Norm => ([shn(c, 0), shn(c, 1), shn(c, 2)], i16_at(c, 3) as i32),
                        T::Half4 => {
                            let u = |i| (u16_at(c, i) as f32 - 32767.0) / 32767.0;
                            ([u(0), u(1), u(2)], i16_at(c, 3) as i32)
                        }
                        _ => return unsupported(m),
                    };
                    if slot == 0 {
                        out.normals.push(n);
                        out.normal_w.push(w);
                    } else {
                        out.extra_normals[slot - 1].push(n);
                    }
                }
                S::Uv if m.kind == T::UByte4Norm => {
                    out.blends[slot].push(c.try_into().expect("four-byte blend member"));
                }
                S::Uv => {
                    let s = |i| i16_at(c, i) as f32 / uv_factor;
                    match m.kind {
                        T::Float2 => out.uvs[slot].push([f32_at(c, 0), f32_at(c, 1)]),
                        T::Float3 => out.uvs[slot].push([f32_at(c, 0), f32_at(c, 1)]),
                        T::Float4 => {
                            out.uvs[slot].push([f32_at(c, 0), f32_at(c, 1)]);
                            out.uvs[slot + 1].push([f32_at(c, 2), f32_at(c, 3)]);
                        }
                        T::Color | T::UByte4 | T::Byte4 | T::Short2 | T::Half2 => {
                            out.uvs[slot].push([s(0), s(1)])
                        }
                        T::Short4 | T::Half4 => {
                            out.uvs[slot].push([s(0), s(1)]);
                            out.uvs[slot + 1].push([s(2), s(3)]);
                        }
                        _ => return unsupported(m),
                    }
                }
                S::Tangent => {
                    let t = match m.kind {
                        T::Float4 => [f32_at(c, 0), f32_at(c, 1), f32_at(c, 2), f32_at(c, 3)],
                        T::Color | T::UByte4 | T::UByte4Norm | T::Byte4E => c.map_n(ubn),
                        T::Byte4Norm => [sbn(c[3]), sbn(c[2]), sbn(c[1]), sbn(c[0])],
                        T::Short4Norm => [shn(c, 0), shn(c, 1), shn(c, 2), shn(c, 3)],
                        _ => return unsupported(m),
                    };
                    out.tangents[slot].push(t);
                }
                S::Bitangent => match m.kind {
                    T::Color | T::UByte4 | T::UByte4Norm | T::Byte4E => {
                        out.bitangents.push(c.map_n(ubn))
                    }
                    _ => return unsupported(m),
                },
                S::VertexColor => {
                    let col = match m.kind {
                        T::Float4 => [f32_at(c, 0), f32_at(c, 1), f32_at(c, 2), f32_at(c, 3)],
                        T::Color | T::UByte4Norm => c.map_n(|b| b as f32 / 255.0),
                        _ => return unsupported(m),
                    };
                    out.colors[slot].push(col);
                }
            }
        }
    }
    Ok(())
}

trait MapN {
    fn map_n(&self, f: impl Fn(u8) -> f32) -> [f32; 4];
}

impl MapN for [u8] {
    fn map_n(&self, f: impl Fn(u8) -> f32) -> [f32; 4] {
        [f(self[0]), f(self[1]), f(self[2]), f(self[3])]
    }
}

#[cfg(test)]
mod blend_tests {
    use super::*;

    #[test]
    fn packed_blend_stream_keeps_bytes_and_does_not_consume_uv_channels() {
        let members = [
            LayoutMember {
                kind: LayoutType::UByte4Norm,
                semantic: LayoutSemantic::Uv,
                index: 1,
            },
            LayoutMember {
                kind: LayoutType::Short4,
                semantic: LayoutSemantic::Uv,
                index: 0,
            },
        ];
        let mut bytes = vec![23, 45, 67, 89];
        for value in [2048i16, -4096, 1024, 6144] {
            bytes.extend(value.to_le_bytes());
        }
        let mut vertices = Vertices::default();
        read_buffer(&bytes, &members, 12, 2048., &mut vertices).unwrap();
        assert_eq!(vertices.blends, vec![vec![[23, 45, 67, 89]]]);
        assert_eq!(vertices.uvs, vec![vec![[1., -2.]], vec![[0.5, 3.]]]);
    }

    #[test]
    fn packed_blend_indices_survive_sparse_out_of_order_layouts() {
        let members = [
            LayoutMember {
                kind: LayoutType::UByte4Norm,
                semantic: LayoutSemantic::Uv,
                index: 5,
            },
            LayoutMember {
                kind: LayoutType::UByte4Norm,
                semantic: LayoutSemantic::Uv,
                index: 1,
            },
        ];
        let mut vertices = Vertices::default();
        read_buffer(
            &[0, 200, 0, 0, 128, 0, 0, 0],
            &members,
            8,
            2048.,
            &mut vertices,
        )
        .unwrap();
        assert_eq!(vertices.blends[0], vec![[128, 0, 0, 0]]);
        assert_eq!(vertices.blends[4], vec![[0, 200, 0, 0]]);
        assert!(vertices.blends[1].is_empty());
    }
}

#[cfg(test)]
mod model_space_tests {
    use super::*;
    use glam::Vec3;

    fn node(translation: [f32; 3], rotation: [f32; 3], scale: [f32; 3], parent: i16) -> Node {
        Node {
            name: String::new(),
            translation,
            rotation,
            scale,
            parent,
            first_child: -1,
            next_sibling: -1,
            previous_sibling: -1,
            bbox_min: [0.0; 3],
            bbox_max: [0.0; 3],
            flags: 0,
        }
    }

    fn mesh(dynamic: bool, normal_w: Vec<i32>) -> Mesh {
        let n = normal_w.len();
        Mesh {
            dynamic,
            material: 0,
            node: 0,
            bone_indices: vec![],
            face_sets: vec![],
            buffers: vec![],
            vertices: Vertices {
                positions: vec![[1.0, 0.0, 0.0]; n],
                normals: vec![[1.0, 0.0, 0.0]; n],
                normal_w,
                tangents: vec![vec![[0.0, 1.0, 0.0, -1.0]; n]],
                bitangents: vec![[0.0, 0.0, 1.0, -1.0]; n],
                ..Default::default()
            },
        }
    }

    #[test]
    fn unweighted_vertices_follow_the_node_their_normal_w_names() {
        // Node 1: up 5 m, turned 90 degrees about Z, scaled 2x along its local X only,
        // under a parent shifted 1 m along Z.
        let nodes = [
            node([0.0; 3], [0.0; 3], [1.0; 3], -1),
            node(
                [0.0, 5.0, 0.0],
                [0.0, 0.0, std::f32::consts::FRAC_PI_2],
                [2.0, 1.0, 1.0],
                2,
            ),
            node([0.0, 0.0, 1.0], [0.0; 3], [1.0; 3], -1),
        ];
        let world = world_matrices(&nodes);
        let mut m = mesh(false, vec![0, 1]);
        // A diagonal normal separates the inverse transpose from the plain linear part.
        let diagonal = Vec3::new(1.0, 1.0, 0.0).normalize();
        m.vertices.normals = vec![diagonal.to_array(); 2];
        let v = m.model_space_vertices(&world);
        assert_eq!(v.positions[0], [1.0, 0.0, 0.0]);
        assert_eq!(v.normals[0], diagonal.to_array());
        let p = Vec3::from(v.positions[1]);
        assert!(p.abs_diff_eq(Vec3::new(0.0, 7.0, 1.0), 1e-5), "{p}");
        // (R S)^-T = R S^-1 for a rotation R and scale S; the parent only translates.
        let rotation = nodes[1].local_rotation();
        let scale = Vec3::from(nodes[1].scale);
        let expected = (rotation * (diagonal / scale)).normalize();
        let linear_only = (rotation * (diagonal * scale)).normalize();
        assert!(!expected.abs_diff_eq(linear_only, 0.1));
        let n = Vec3::from(v.normals[1]);
        assert!(n.abs_diff_eq(expected, 1e-5), "{n}, expected {expected}");
        let t = v.tangents[0][1];
        assert!(Vec3::new(t[0], t[1], t[2]).abs_diff_eq(-Vec3::X, 1e-5));
        assert_eq!(t[3], -1.0);
    }

    #[test]
    fn a_mirroring_node_negates_x_and_flips_the_tangent_handedness() {
        let nodes = [node([0.0; 3], [0.0; 3], [-1.0, 1.0, 1.0], -1)];
        let world = world_matrices(&nodes);
        let v = mesh(false, vec![0])
            .model_space_vertices(&world)
            .into_owned();
        assert_eq!(v.positions[0], [-1.0, 0.0, 0.0]);
        assert_eq!(v.normals[0], [-1.0, 0.0, 0.0]);
        assert_eq!(v.tangents[0][0], [0.0, 1.0, 0.0, 1.0]);
        // Bitangent W is unverified under mirroring and kept as stored.
        assert_eq!(v.bitangents[0], [0.0, 0.0, 1.0, -1.0]);
    }

    #[test]
    fn normal_w_goes_through_the_bone_palette() {
        let nodes = [
            node([0.0; 3], [0.0; 3], [1.0; 3], -1),
            node([0.0, 5.0, 0.0], [0.0; 3], [1.0; 3], -1),
        ];
        let world = world_matrices(&nodes);
        let mut m = mesh(false, vec![0]);
        m.bone_indices = vec![1];
        assert_eq!(m.palette_node(0), 1);
        let v = m.model_space_vertices(&world);
        assert_eq!(v.positions[0], [1.0, 5.0, 0.0]);
    }

    #[test]
    fn weighted_and_identity_bound_meshes_stay_borrowed() {
        let nodes = [
            node([0.0; 3], [0.0; 3], [1.0; 3], -1),
            node([0.0, 5.0, 0.0], [0.0; 3], [1.0; 3], -1),
        ];
        let world = world_matrices(&nodes);
        for m in [mesh(true, vec![1, 1]), mesh(false, vec![0, 0])] {
            assert!(matches!(
                m.model_space_vertices(&world),
                std::borrow::Cow::Borrowed(_)
            ));
        }
    }
}
