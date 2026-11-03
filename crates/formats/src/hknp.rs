//! Map hit collision: Havok 2016 `hknpPhysicsSceneData` tagfiles whose bodies use FromSoftware's
//! `fsnpCustomParamCompressedMeshShape` (an `hknpCompressedMeshShape` plus per-triangle
//! material data), decoded to plain triangle lists.
//!
//! Compressed mesh layout, as documented by the public readers (DSMapStudio's
//! `HavokCollisionResource` / HKX2 `hknpCompressedMeshShapeData`, soulstruct-havok type dumps):
//!
//! - The mesh tree holds sections. Each section owns a run of primitives
//!   (`primitives.data`: start index `>> 8`, count `& 0xFF`) and a run of local "packed"
//!   vertices starting at `firstPackedVertex`; `sharedVertices.data` gives the number of packed
//!   vertices (`& 0xFF`) and the start of the section's entries in `sharedVerticesIndex`
//!   (`>> 8`).
//! - A primitive is four byte indices. An index below the packed count is a packed vertex
//!   (32 bits: x 11, y 11, z 10, dequantised with the section's `codecParms`: offset xyz then
//!   scale xyz). Otherwise it selects a shared vertex through `sharedVerticesIndex`
//!   (64 bits: x 21, y 21, z 22, dequantised over the mesh tree's domain AABB).
//! - Indices `(a, b, c, d)` with `c != d` are a quad, split as `(a, b, c)` and `(a, c, d)`.
//!   `DE AD DE AD` marks an empty primitive slot: over every `h` and `l` binder of the nine
//!   exported maps no authored triangle's shape key points at one, and every shape's
//!   `numConvexShapes` is 0, so there is no convex shape behind it to decode (#189).
//! - Authored triangles the builder dropped have the all-ones (invalid) shape key; with those
//!   and the empty slots, the triangles decoded are exactly the authored ones with a valid key.
//!
//! The FromSoftware wrapper adds `triangleIndexToShapeKey` (authored triangle index to shape
//! key, the key left-aligned in `numShapeKeyBits`) and `pParam`, whose per-triangle
//! `primitiveDataIndex` selects a `PrimitiveData` with the hit material id
//! (`materialNameData`). The shape key of a triangle is
//! `(section << 8) | (primitive_in_section << 1) | second_triangle_of_quad`, so sections hold
//! at most 128 primitives and `numShapeKeyBits` is the section index bits plus 8. On
//! m11_00_00_00 every decoded triangle finds its key this way.
//!
//! Object binders' physics files also use convex polytopes (boxes, capsules and hulls), which
//! [`decode_bodies`] turns into triangles. Output is in the file's own (FromSoftware,
//! left-handed) space with the body transform applied.

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};

use crate::hkx::{Compendium, TagFile, Value};
use crate::reader::{Result, bail};

/// A triangle soup with one material id per triangle.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct CollisionMesh {
    pub vertices: Vec<[f32; 3]>,
    /// Three per triangle.
    pub indices: Vec<u32>,
    /// One per triangle: the FromSoftware hit material id, or `u32::MAX` when unknown.
    pub materials: Vec<u32>,
}

impl CollisionMesh {
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    /// Appends another mesh.
    pub fn extend(&mut self, other: &CollisionMesh) {
        let base = self.vertices.len() as u32;
        self.vertices.extend_from_slice(&other.vertices);
        self.indices.extend(other.indices.iter().map(|i| i + base));
        self.materials.extend_from_slice(&other.materials);
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        postcard::to_stdvec(self).map_err(|e| crate::Error::Format(e.to_string()))
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        postcard::from_bytes(data).map_err(|e| crate::Error::Format(e.to_string()))
    }
}

/// Counters from decoding, for logs and sanity checks.
#[derive(Debug, Clone, Default)]
pub struct DecodeStats {
    pub bodies: usize,
    pub sections: usize,
    pub primitives: usize,
    pub quads: usize,
    /// Empty `DE AD DE AD` primitive slots (no triangle and no convex shape, see the module
    /// notes).
    pub convex_skipped: usize,
    /// Triangles whose shape key had no material entry.
    pub unmatched_material: usize,
    /// Bodies [`decode_bodies`] or [`decode_extern`] skipped for their shape type.
    pub other_shapes: usize,
    /// Dynamic bodies [`decode_bodies`] left out.
    pub dynamic: usize,
}

impl std::ops::AddAssign<&DecodeStats> for DecodeStats {
    fn add_assign(&mut self, other: &DecodeStats) {
        self.bodies += other.bodies;
        self.sections += other.sections;
        self.primitives += other.primitives;
        self.quads += other.quads;
        self.convex_skipped += other.convex_skipped;
        self.unmatched_material += other.unmatched_material;
        self.other_shapes += other.other_shapes;
        self.dynamic += other.dynamic;
    }
}

/// The collision of a map's placed objects (`cache/maps/<id>/object_collision.bin`, postcard):
/// the static and keyframed bodies of each placed object's physics binder (`<model>.hkx` in its
/// objbnd), placed with the object's MSB transform, in Bevy space like `collision.bin`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ObjectCollision {
    pub parts: Vec<ObjectCollisionPart>,
    pub mesh: CollisionMesh,
}

/// One placed object's triangles in [`ObjectCollision::mesh`]: `count` triangles from `first`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ObjectCollisionPart {
    /// MSB part name, the key of its row (model included) in `layout.json`.
    pub name: String,
    /// MSB entity id, for the events that switch or break the object.
    pub entity_id: i32,
    /// `ObjectParam.collisionType` of the model's row (0 without a row, as the exe reads it),
    /// which selects the bodies' Havok collision group.
    pub collision_type: u8,
    pub first: u32,
    pub count: u32,
}

impl ObjectCollision {
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        postcard::to_stdvec(self).map_err(|e| crate::Error::Format(e.to_string()))
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        postcard::from_bytes(data).map_err(|e| crate::Error::Format(e.to_string()))
    }
}

/// One of a map's hit binders placed by its MSB collision parts, in Bevy space like
/// `collision.bin`, with each part's triangle range (postcard):
///
/// - `cache/maps/<id>/fall_assist.bin`: the `f<map>.hkxbhd` binder, the fall-prevention assist
///   walls only the assist's sphere casts meet (docs/OBJECT-COLLISION.md),
/// - `cache/maps/<id>/collision_lo.bin`: the `l<map>.hkxbhd` binder, the low-resolution hit
///   collision characters and the camera collide with (docs/HIT-FILTERS.md, #189).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct PartCollision {
    pub parts: Vec<PartRange>,
    pub mesh: CollisionMesh,
}

/// One collision part's triangles in [`PartCollision::mesh`]: `count` triangles from `first`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct PartRange {
    /// MSB collision part name, the key of its row in `layout.json`'s `collisions`.
    pub name: String,
    pub first: u32,
    pub count: u32,
}

impl PartCollision {
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        postcard::to_stdvec(self).map_err(|e| crate::Error::Format(e.to_string()))
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        postcard::from_bytes(data).map_err(|e| crate::Error::Format(e.to_string()))
    }
}

/// Decodes every compressed-mesh body in a collision tagfile. Bodies are read for their shape
/// and transform only, so one without a name or with an unexpected motion type still counts.
pub fn decode(
    data: &[u8],
    compendium: Option<&Compendium>,
) -> Result<(CollisionMesh, DecodeStats)> {
    let file = TagFile::parse(data, compendium)?;
    let mut mesh = CollisionMesh::default();
    let mut stats = DecodeStats::default();
    for (body, shape) in shaped_bodies(&file)? {
        if !shape.is_a("hknpCompressedMeshShape") {
            continue;
        }
        let part = decode_shape(shape, &mut stats)?;
        extend_placed(&mut mesh, body, &part)?;
        stats.bodies += 1;
    }
    Ok((mesh, stats))
}

/// Decodes the static and keyframed bodies of an object's physics tagfile, in order, as one
/// mesh. The serialised `motionType` of a `bodyCinfo` is 0 for static props, 1 for bone-driven
/// doors and tree trunks with no mass, and 2 for the massive fragments of a broken object's
/// `_1.hkx`; dynamic bodies (2) are counted and left out, and an unknown type is an error.
/// Shapes are compressed meshes and the convex polytopes objects use: `hknpConvexPolytopeShape`
/// and the box shape derived from it as their hull pushed out by the convex radius, and
/// `hknpCapsuleShape` from its segment and radius. Bodies of other shapes are skipped and
/// counted.
pub fn decode_bodies(
    data: &[u8],
    compendium: Option<&Compendium>,
) -> Result<(CollisionMesh, DecodeStats)> {
    let file = TagFile::parse(data, compendium)?;
    let mut mesh = CollisionMesh::default();
    let mut stats = DecodeStats::default();
    for (body, shape) in shaped_bodies(&file)? {
        let compressed = shape.is_a("hknpCompressedMeshShape");
        if !compressed && !shape.is_a("hknpConvexPolytopeShape") {
            stats.other_shapes += 1;
            continue;
        }
        match body.get("motionType")?.int()? {
            0 | 1 => {}
            2 => {
                stats.dynamic += 1;
                continue;
            }
            other => return bail(format!("unknown body motion type {other}")),
        }
        let part = if compressed {
            decode_shape(shape, &mut stats)?
        } else {
            decode_polytope(shape)?
        };
        extend_placed(&mut mesh, body, &part)?;
        stats.bodies += 1;
    }
    Ok((mesh, stats))
}

/// Decodes every extern-mesh body (`hknpExternMeshShape` over an `hkGeometry`) in a tagfile as
/// one mesh. The map's fall-prevention assist binder (`f<map>.hkxbhd`, docs/OBJECT-COLLISION.md)
/// holds its walls this way, one file per collision model. Bodies of other shapes are skipped
/// and counted. The walls carry no hit filter group: in m11_00's and m11_01's binders every
/// body's `collisionFilterInfo`, `materialId` and `userData` are 0 and every triangle's
/// material is -1 (kept as `u32::MAX`), so the exe gives them their group when it loads them
/// (93, docs/OBJECT-COLLISION.md).
pub fn decode_extern(
    data: &[u8],
    compendium: Option<&Compendium>,
) -> Result<(CollisionMesh, DecodeStats)> {
    let file = TagFile::parse(data, compendium)?;
    let mut mesh = CollisionMesh::default();
    let mut stats = DecodeStats::default();
    for (body, shape) in shaped_bodies(&file)? {
        if !shape.is_a("hknpExternMeshShape") {
            stats.other_shapes += 1;
            continue;
        }
        let Some(geometry) = shape.get("geometry")?.deref()? else {
            continue;
        };
        let Some(geometry) = geometry.get("geometry")?.deref()? else {
            continue;
        };
        let part = decode_geometry(geometry)?;
        stats.primitives += part.triangle_count();
        extend_placed(&mut mesh, body, &part)?;
        stats.bodies += 1;
    }
    Ok((mesh, stats))
}

/// An `hkGeometry`: vertices as four floats each, triangles as `a`, `b`, `c` and `material`.
fn decode_geometry(geometry: Value<'_>) -> Result<CollisionMesh> {
    let mut part = CollisionMesh::default();
    for v in geometry.get("vertices")?.elements()? {
        match v.floats()?[..] {
            [x, y, z, ..] => part.vertices.push([x, y, z]),
            _ => return bail("geometry vertex has fewer than three floats"),
        }
    }
    for t in geometry.get("triangles")?.elements()? {
        let corner = |name: &str| -> Result<u32> {
            let i = t.get(name)?.int()?;
            match u32::try_from(i) {
                Ok(i) if (i as usize) < part.vertices.len() => Ok(i),
                _ => bail(format!("geometry triangle corner {i} out of range")),
            }
        };
        part.indices
            .extend([corner("a")?, corner("b")?, corner("c")?]);
        part.materials
            .push(u32::try_from(t.get("material")?.int()?).unwrap_or(u32::MAX));
    }
    Ok(part)
}

/// Each `bodyCinfo` of the file's physics systems that has a shape, with that shape.
fn shaped_bodies(file: &TagFile) -> Result<Vec<(Value<'_>, Value<'_>)>> {
    let mut out = Vec::new();
    for system in file.objects_of("hknpPhysicsSystemData") {
        for body in system.get("bodyCinfos")?.elements()? {
            if let Some(shape) = body.get("shape")?.deref()? {
                out.push((body, shape));
            }
        }
    }
    Ok(out)
}

/// Appends `part`, in its body's own space, to `mesh` with the body transform applied.
fn extend_placed(mesh: &mut CollisionMesh, body: Value<'_>, part: &CollisionMesh) -> Result<()> {
    let position = body.get("position")?.floats()?;
    let orientation = body.get("orientation")?.floats()?;
    let transform = glam::Affine3A::from_rotation_translation(
        glam::Quat::from_xyzw(
            orientation[0],
            orientation[1],
            orientation[2],
            orientation[3],
        )
        .normalize(),
        glam::Vec3::new(position[0], position[1], position[2]),
    );
    let base = mesh.vertices.len() as u32;
    mesh.vertices.extend(
        part.vertices
            .iter()
            .map(|v| transform.transform_point3((*v).into()).to_array()),
    );
    mesh.indices.extend(part.indices.iter().map(|i| i + base));
    mesh.materials.extend_from_slice(&part.materials);
    Ok(())
}

/// A convex polytope as closed triangles. Its vertices are the hull shrunk by the convex radius
/// (an Outskirts chest, `o114740`, has its box top at 1.006 m, radius 0.05, and its FLVER top at
/// 1.056 m), so the face planes move out by the radius and [`expanded_hull`] rebuilds the faces
/// from them. A capsule (`hknpCapsuleShape`) stores a thin box around its segment `a` to `b` as
/// the hull and its radius as the convex radius, so it is built from the segment ([`capsule`]).
fn decode_polytope(shape: Value<'_>) -> Result<CollisionMesh> {
    let radius = shape.get("convexRadius")?.f32()?;
    let point = |v: Value<'_>| -> Result<glam::Vec3> {
        match v.floats()?[..] {
            [x, y, z, ..] => Ok(glam::Vec3::new(x, y, z)),
            _ => bail("polytope vector has fewer than three floats"),
        }
    };
    if shape.is_a("hknpCapsuleShape") {
        let (a, b) = (point(shape.get("a")?)?, point(shape.get("b")?)?);
        return Ok(capsule(a, b, radius));
    }
    let vertices: Vec<glam::Vec3> = shape
        .get("vertices")?
        .elements()?
        .into_iter()
        .map(point)
        .collect::<Result<_>>()?;
    let planes: Vec<[f32; 4]> = shape
        .get("planes")?
        .elements()?
        .iter()
        .map(|p| match p.floats()?[..] {
            [x, y, z, d] => Ok([x, y, z, d]),
            _ => bail("polytope plane is not four floats"),
        })
        .collect::<Result<_>>()?;
    let faces = shape.get("faces")?.elements()?;
    if faces.len() != planes.len() {
        return bail(format!(
            "polytope has {} faces and {} planes",
            faces.len(),
            planes.len()
        ));
    }
    let indices = shape.get("indices")?.ints()?;
    let mut corners = Vec::with_capacity(faces.len());
    for face in &faces {
        let first = face.get("firstIndex")?.int()? as usize;
        let count = face.get("numIndices")?.int()? as usize;
        let Some(face) = indices.get(first..first + count) else {
            return bail("polytope face indices out of range");
        };
        if face.iter().any(|&i| i as usize >= vertices.len()) {
            return bail("polytope vertex out of range");
        }
        corners.push(face.iter().map(|&i| i as usize).collect());
    }
    Ok(expanded_hull(&vertices, &planes, &corners, radius))
}

/// The shrunk hull `vertices`, with faces `corners` on `planes` (`[nx, ny, nz, d]`, outward
/// normals, inside where `n.x + d <= 0`), pushed out by `radius` as closed triangles wound
/// counter-clockwise about their outward normal. Every face plane moves out by the radius, and
/// so that sharp edges and corners do not stick far out of the rounded hull (the shrunk hull
/// plus a ball of `radius`), an edge between faces more than 90 degrees apart and a corner with
/// such a pair of faces get a bevel plane at `radius` from them, along their faces' mean normal.
/// Each plane's face is a square on it larger than the hull, clipped by every other plane, so
/// neighbouring faces end on the same edge and corner. The result holds the rounded hull and
/// leaves it only near edges and corners, by at most `radius / cos(half the angle between the
/// faces' normals) - radius` at an edge (0.41 times the radius on a box, 0.73 at its corners).
fn expanded_hull(
    vertices: &[glam::Vec3],
    planes: &[[f32; 4]],
    corners: &[Vec<usize>],
    radius: f32,
) -> CollisionMesh {
    use glam::DVec3;
    let radius = radius as f64;
    let normals: Vec<Option<(DVec3, f64)>> = planes
        .iter()
        .map(|p| {
            let n = DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64);
            let length = n.length();
            (length > 1e-6).then(|| (n / length, p[3] as f64 / length))
        })
        .collect();
    let mut moved: Vec<(DVec3, f64)> = Vec::new();
    let mut add = |n: DVec3, d: f64| {
        // Faces split on one plane would each build the same face.
        if !moved
            .iter()
            .any(|&(m, e)| m.dot(n) > 1.0 - 1e-9 && (e - d).abs() < 1e-6)
        {
            moved.push((n, d));
        }
    };
    for &(n, d) in normals.iter().flatten() {
        add(n, d - radius);
    }
    let mut edges: BTreeMap<(usize, usize), Vec<DVec3>> = BTreeMap::new();
    let mut at_vertex: Vec<Vec<DVec3>> = vec![Vec::new(); vertices.len()];
    for (face, normal) in corners.iter().zip(&normals) {
        let Some((n, _)) = *normal else {
            continue;
        };
        for (k, &a) in face.iter().enumerate() {
            let b = face[(k + 1) % face.len()];
            edges.entry((a.min(b), a.max(b))).or_default().push(n);
            at_vertex[a].push(n);
        }
    }
    let sharp = |normals: &[DVec3]| {
        normals
            .iter()
            .enumerate()
            .any(|(i, a)| normals[i + 1..].iter().any(|b| a.dot(*b) < 0.0))
    };
    let mut bevel = |normals: &[DVec3], at: glam::Vec3| {
        if let Some(m) = normals.iter().sum::<DVec3>().try_normalize()
            && sharp(normals)
        {
            add(m, -m.dot(at.as_dvec3()) - radius);
        }
    };
    for (&(a, _), normals) in &edges {
        bevel(normals, vertices[a]);
    }
    for (v, normals) in at_vertex.iter().enumerate() {
        bevel(normals, vertices[v]);
    }
    let centre = vertices
        .iter()
        .fold(DVec3::ZERO, |sum, v| sum + v.as_dvec3())
        / vertices.len().max(1) as f64;
    let extent = vertices
        .iter()
        .map(|v| v.as_dvec3().distance(centre))
        .fold(0.0, f64::max);
    let half = 4.0 * (extent + radius) + 1.0;
    let mut mesh = CollisionMesh::default();
    for (i, &(n, d)) in moved.iter().enumerate() {
        let u = n.any_orthonormal_vector();
        let w = n.cross(u);
        let on_plane = centre - n * (n.dot(centre) + d);
        let mut face: Vec<DVec3> = [(1.0, 1.0), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)]
            .map(|(a, b)| on_plane + (u * a + w * b) * half)
            .to_vec();
        for (j, &(m, e)) in moved.iter().enumerate() {
            if j != i && face.len() >= 3 {
                face = clip(&face, m, e);
            }
        }
        if face.len() < 3 {
            continue;
        }
        let base = mesh.vertices.len() as u32;
        mesh.vertices
            .extend(face.iter().map(|c| c.as_vec3().to_array()));
        for k in 1..face.len() as u32 - 1 {
            mesh.indices.extend([base, base + k, base + k + 1]);
            mesh.materials.push(u32::MAX);
        }
    }
    mesh
}

/// The part of convex polygon `face` where `n.x + d <= 0` (Sutherland-Hodgman), in order.
fn clip(face: &[glam::DVec3], n: glam::DVec3, d: f64) -> Vec<glam::DVec3> {
    let mut out = Vec::with_capacity(face.len() + 1);
    for (k, &p) in face.iter().enumerate() {
        let q = face[(k + 1) % face.len()];
        let (sp, sq) = (n.dot(p) + d, n.dot(q) + d);
        if sp <= 0.0 {
            out.push(p);
        }
        if (sp <= 0.0) != (sq <= 0.0) {
            out.push(p + (q - p) * (sp / (sp - sq)));
        }
    }
    out
}

/// A capsule around the segment `a` to `b` as closed triangles wound outward: two hemispheres
/// and the cylinder between them, 12 segments around and 4 rings per hemisphere, with every
/// vertex on the surface.
fn capsule(a: glam::Vec3, b: glam::Vec3, radius: f32) -> CollisionMesh {
    use std::f32::consts::{FRAC_PI_2, TAU};
    const AROUND: u32 = 12;
    const RINGS: u32 = 4;
    let axis = (b - a).try_normalize().unwrap_or(glam::Vec3::Y);
    let u = axis.any_orthonormal_vector();
    let w = axis.cross(u);
    // Ring centres and radii from the one next to `a`'s pole to the one next to `b`'s.
    let mut rings = Vec::new();
    for k in 1..=RINGS {
        let up = FRAC_PI_2 * (RINGS - k) as f32 / RINGS as f32;
        rings.push((a - axis * radius * up.sin(), radius * up.cos()));
    }
    for k in 0..RINGS {
        let up = FRAC_PI_2 * k as f32 / RINGS as f32;
        rings.push((b + axis * radius * up.sin(), radius * up.cos()));
    }
    let mut mesh = CollisionMesh::default();
    mesh.vertices.push((a - axis * radius).to_array());
    for &(centre, r) in &rings {
        for j in 0..AROUND {
            let t = TAU * j as f32 / AROUND as f32;
            mesh.vertices
                .push((centre + (u * t.cos() + w * t.sin()) * r).to_array());
        }
    }
    let top = mesh.vertices.len() as u32;
    mesh.vertices.push((b + axis * radius).to_array());
    let last = rings.len() as u32 - 1;
    let at = |ring: u32, j: u32| 1 + ring * AROUND + j % AROUND;
    for j in 0..AROUND {
        mesh.indices.extend([0, at(0, j + 1), at(0, j)]);
        for ring in 0..last {
            mesh.indices
                .extend([at(ring, j), at(ring, j + 1), at(ring + 1, j + 1)]);
            mesh.indices
                .extend([at(ring, j), at(ring + 1, j + 1), at(ring + 1, j)]);
        }
        mesh.indices.extend([at(last, j), at(last, j + 1), top]);
    }
    mesh.materials = vec![u32::MAX; mesh.indices.len() / 3];
    mesh
}

fn u32s(bytes: &[u8]) -> Vec<u32> {
    bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| u32::from_le_bytes(*b))
        .collect()
}

/// Shape key -> hit material id, from the FromSoftware per-triangle parameters.
fn material_by_key(shape: Value<'_>) -> Result<HashMap<u32, u32>> {
    let mut out = HashMap::new();
    if !shape.has("triangleIndexToShapeKey") || !shape.has("pParam") {
        return Ok(out);
    }
    let bits = shape.get("numShapeKeyBits")?.int()? as u32;
    if bits == 0 || bits > 32 {
        return Ok(out);
    }
    let Some(param) = shape.get("pParam")?.deref()? else {
        return Ok(out);
    };
    let keys = u32s(shape.get("triangleIndexToShapeKey")?.array_bytes()?);
    let primitive_materials: Vec<u32> = param
        .get("primitiveDataArray")?
        .elements()?
        .iter()
        .map(|p| Ok(p.get("materialNameData")?.int()? as u32))
        .collect::<Result<_>>()?;
    let triangles = param.get("triangleDataArray")?.elements()?;
    for (tri, raw) in triangles.iter().zip(keys) {
        let key = raw >> (32 - bits);
        let pd = tri.get("primitiveDataIndex")?.int()? as usize;
        if let Some(&m) = primitive_materials.get(pd) {
            out.entry(key).or_insert(m);
        }
    }
    Ok(out)
}

fn decode_shape(shape: Value<'_>, stats: &mut DecodeStats) -> Result<CollisionMesh> {
    let Some(data) = shape.get("data")?.deref()? else {
        return Ok(CollisionMesh::default());
    };
    let materials = material_by_key(shape)?;
    let tree = data.get("meshTree")?;
    let domain = tree.get("domain")?;
    let dmin = domain.get("min")?.floats()?;
    let dmax = domain.get("max")?.floats()?;
    let shared_scale = [
        (dmax[0] - dmin[0]) / ((1u32 << 21) - 1) as f32,
        (dmax[1] - dmin[1]) / ((1u32 << 21) - 1) as f32,
        (dmax[2] - dmin[2]) / ((1u32 << 22) - 1) as f32,
    ];
    let primitives = tree.get("primitives")?.array_bytes()?;
    let packed = u32s(tree.get("packedVertices")?.array_bytes()?);
    let shared: Vec<u64> = tree
        .get("sharedVertices")?
        .array_bytes()?
        .as_chunks::<8>()
        .0
        .iter()
        .map(|b| u64::from_le_bytes(*b))
        .collect();
    let shared_index: Vec<u16> = tree
        .get("sharedVerticesIndex")?
        .array_bytes()?
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| u16::from_le_bytes(*b))
        .collect();

    let mut mesh = CollisionMesh::default();
    for (si, section) in tree.get("sections")?.elements()?.iter().enumerate() {
        stats.sections += 1;
        let codec = section.get("codecParms")?.floats()?;
        if codec.len() < 6 {
            return bail("section codecParms has fewer than 6 floats");
        }
        let first_packed = section.get("firstPackedVertex")?.int()? as usize;
        let sv = section.get("sharedVertices")?.get("data")?.int()? as u32;
        let packed_count = (sv & 0xFF) as usize;
        let shared_start = (sv >> 8) as usize;
        let pr = section.get("primitives")?.get("data")?.int()? as u32;
        let prim_start = (pr >> 8) as usize;
        let prim_count = (pr & 0xFF) as usize;

        let vertex = |i: u8| -> Result<[f32; 3]> {
            let i = i as usize;
            if i < packed_count {
                let Some(&v) = packed.get(first_packed + i) else {
                    return bail("packed vertex index out of range");
                };
                Ok([
                    (v & 0x7FF) as f32 * codec[3] + codec[0],
                    ((v >> 11) & 0x7FF) as f32 * codec[4] + codec[1],
                    ((v >> 22) & 0x3FF) as f32 * codec[5] + codec[2],
                ])
            } else {
                let Some(&s) = shared_index.get(shared_start + i - packed_count) else {
                    return bail("shared vertex index out of range");
                };
                let Some(&v) = shared.get(s as usize) else {
                    return bail("shared vertex out of range");
                };
                Ok([
                    (v & 0x1F_FFFF) as f32 * shared_scale[0] + dmin[0],
                    ((v >> 21) & 0x1F_FFFF) as f32 * shared_scale[1] + dmin[1],
                    ((v >> 42) & 0x3F_FFFF) as f32 * shared_scale[2] + dmin[2],
                ])
            }
        };

        // Each section indexes at most 256 vertices; weld them once per section.
        let mut welded = [u32::MAX; 256];
        let mut index_of = |i: u8, mesh: &mut CollisionMesh| -> Result<u32> {
            if welded[i as usize] == u32::MAX {
                welded[i as usize] = mesh.vertices.len() as u32;
                mesh.vertices.push(vertex(i)?);
            }
            Ok(welded[i as usize])
        };
        for p in 0..prim_count {
            let o = (prim_start + p) * 4;
            let Some(ix) = primitives.get(o..o + 4) else {
                return bail("primitive index out of range");
            };
            if ix == [0xDE, 0xAD, 0xDE, 0xAD] {
                stats.convex_skipped += 1;
                continue;
            }
            stats.primitives += 1;
            let key = ((si as u32) << 8) | ((p as u32) << 1);
            let mut material_of = |key: u32| match materials.get(&key) {
                Some(&m) => m,
                None => {
                    if !materials.is_empty() {
                        stats.unmatched_material += 1;
                    }
                    u32::MAX
                }
            };
            let first_material = material_of(key);
            let [a, b, c, d] = [ix[0], ix[1], ix[2], ix[3]];
            let (a, b, c) = (
                index_of(a, &mut mesh)?,
                index_of(b, &mut mesh)?,
                index_of(c, &mut mesh)?,
            );
            mesh.indices.extend([a, b, c]);
            mesh.materials.push(first_material);
            if ix[2] != ix[3] {
                stats.quads += 1;
                let d = index_of(d, &mut mesh)?;
                mesh.indices.extend([a, c, d]);
                let second = material_of(key | 1);
                mesh.materials.push(second);
            }
        }
    }
    Ok(mesh)
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    /// Each undirected edge of `mesh`, with its vertices welded at 0.1 mm, and how many
    /// triangles use it.
    fn edge_uses(mesh: &CollisionMesh) -> HashMap<([i64; 3], [i64; 3]), usize> {
        let weld = |i: u32| mesh.vertices[i as usize].map(|c| (c * 1e4).round() as i64);
        let mut uses = HashMap::new();
        for t in mesh.indices.as_chunks::<3>().0 {
            for k in 0..3 {
                let (a, b) = (weld(t[k]), weld(t[(k + 1) % 3]));
                *uses.entry((a.min(b), a.max(b))).or_default() += 1;
            }
        }
        uses
    }

    #[test]
    fn a_pushed_out_box_is_closed_at_its_edges() {
        // A 1 m box shrunk by 0.1 m, laid out as Havok stores a box shape's faces.
        let vertices: Vec<Vec3> = (0..8)
            .map(|i| {
                let s = |bit: usize| if i & bit != 0 { 0.4 } else { -0.4 };
                Vec3::new(s(1), s(2), s(4))
            })
            .collect();
        let planes = [
            [1.0, 0.0, 0.0, -0.4],
            [-1.0, 0.0, 0.0, -0.4],
            [0.0, 1.0, 0.0, -0.4],
            [0.0, -1.0, 0.0, -0.4],
            [0.0, 0.0, 1.0, -0.4],
            [0.0, 0.0, -1.0, -0.4],
        ];
        let corners = vec![
            vec![1, 3, 7, 5],
            vec![0, 4, 6, 2],
            vec![2, 6, 7, 3],
            vec![0, 1, 5, 4],
            vec![4, 5, 7, 6],
            vec![0, 2, 3, 1],
        ];
        let mesh = expanded_hull(&vertices, &planes, &corners, 0.1);
        assert_eq!(mesh.triangle_count(), 12);
        let uses = edge_uses(&mesh);
        assert!(uses.values().all(|&n| n == 2), "{uses:?}");
        // The top face reaches the side faces: its corners are the box's, 0.5 m out.
        for v in &mesh.vertices {
            assert!(v.iter().all(|c| (c.abs() - 0.5).abs() < 1e-6), "{v:?}");
        }
        // Wound outward: every triangle's normal points away from the centre.
        for t in mesh.indices.as_chunks::<3>().0 {
            let [a, b, c] = t.map(|i| Vec3::from(mesh.vertices[i as usize]));
            assert!((b - a).cross(c - a).dot(a + b + c) > 0.0);
        }
    }

    #[test]
    fn a_sharp_edge_is_bevelled_near_the_rounded_hull() {
        // A wedge 10 degrees thick with its ridge along z at x = 1. The two side planes alone
        // would meet 11.5 radii out from the ridge; the bevel holds it at one radius.
        let (t, r) = (5f32.to_radians().tan(), 0.05);
        let side = |y: f32| {
            let n = Vec3::new(t, y, 0.0).normalize();
            [n.x, n.y, 0.0, -n.x]
        };
        let vertices = vec![
            Vec3::new(0.0, -t, -1.0),
            Vec3::new(0.0, t, -1.0),
            Vec3::new(1.0, 0.0, -1.0),
            Vec3::new(0.0, -t, 1.0),
            Vec3::new(0.0, t, 1.0),
            Vec3::new(1.0, 0.0, 1.0),
        ];
        let planes = [
            side(1.0),
            side(-1.0),
            [-1.0, 0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, -1.0],
            [0.0, 0.0, -1.0, -1.0],
        ];
        let corners = vec![
            vec![1, 4, 5, 2],
            vec![0, 2, 5, 3],
            vec![0, 3, 4, 1],
            vec![3, 5, 4],
            vec![0, 1, 2],
        ];
        let mesh = expanded_hull(&vertices, &planes, &corners, r);
        let uses = edge_uses(&mesh);
        assert!(uses.values().all(|&n| n == 2), "{uses:?}");
        let tip = mesh.vertices.iter().map(|v| v[0]).fold(f32::MIN, f32::max);
        assert!((tip - (1.0 + r)).abs() < 1e-5, "ridge out to {tip}");
    }

    #[test]
    fn a_capsule_is_closed_and_round() {
        let (a, b, r) = (Vec3::new(1.0, 0.0, 0.0), Vec3::new(1.0, 2.0, 0.5), 0.3);
        let mesh = capsule(a, b, r);
        assert!(edge_uses(&mesh).values().all(|&n| n == 2));
        assert_eq!(mesh.materials.len(), mesh.triangle_count());
        for v in &mesh.vertices {
            let p = Vec3::from(*v);
            let t = ((p - a).dot(b - a) / (b - a).length_squared()).clamp(0.0, 1.0);
            assert!((p.distance(a.lerp(b, t)) - r).abs() < 1e-5, "{p}");
        }
        for t in mesh.indices.as_chunks::<3>().0 {
            let [p, q, s] = t.map(|i| Vec3::from(mesh.vertices[i as usize]));
            let centre = (p + q + s) / 3.0;
            let k = ((centre - a).dot(b - a) / (b - a).length_squared()).clamp(0.0, 1.0);
            assert!((q - p).cross(s - p).dot(centre - a.lerp(b, k)) > 0.0);
        }
    }
}
