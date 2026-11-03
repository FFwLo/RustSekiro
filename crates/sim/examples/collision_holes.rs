//! Headless render/collision audit. See docs/COLLISION-HOLES.md for thresholds and limits.
//! collision_holes [cache/maps] [map ids...] [--spacing 0.5] [--probes 2048]
use glam::{Quat, Vec2, Vec3};
use rayon::prelude::*;
use sekiro_formats::hknp::CollisionMesh;
use sekiro_sim::body::{Body, BodyInput};
use sekiro_sim::collision::{CollisionWorld, RADIUS};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::path::Path;
use std::time::Instant;

type Error = Box<dyn std::error::Error + Send + Sync>;
type Key = (i32, i32, i32);
const GAP: f32 = 0.3;
const LAYER: f32 = 0.25;
const NEAR: f32 = 2.0;
const DROP: f32 = 5.0;
const SUBSTEP: f32 = RADIUS * 0.25;
const REACH_TILE: f32 = 4.0;

struct Reach {
    voxels: HashSet<Key>,
    columns: BTreeMap<i32, BTreeMap<i32, (i32, i32)>>,
}
impl Reach {
    fn new(mesh: &CollisionMesh) -> Self {
        let mut voxels = HashSet::new();
        for t in mesh.indices.as_chunks::<3>().0 {
            let p = t.map(|i| Vec3::from(mesh.vertices[i as usize]));
            if (p[1] - p[0]).cross(p[2] - p[0]).normalize_or_zero().y.abs()
                < 50_f32.to_radians().cos()
            {
                continue;
            }
            let margin = Vec3::new(NEAR + 0.5, 2.0, NEAR + 0.5);
            let lo = (p[0].min(p[1]).min(p[2]) - margin) / REACH_TILE;
            let hi = (p[0].max(p[1]).max(p[2]) + margin + Vec3::Y * 2.0) / REACH_TILE;
            for x in lo.x.floor() as i32..=hi.x.floor() as i32 {
                for z in lo.z.floor() as i32..=hi.z.floor() as i32 {
                    for y in lo.y.floor() as i32..=hi.y.floor() as i32 {
                        voxels.insert((x, z, y));
                    }
                }
            }
        }
        let mut columns = BTreeMap::<i32, BTreeMap<i32, (i32, i32)>>::new();
        for &(x, z, y) in &voxels {
            let bounds = columns.entry(x).or_default().entry(z).or_insert((y, y));
            bounds.0 = bounds.0.min(y);
            bounds.1 = bounds.1.max(y);
        }
        Self { voxels, columns }
    }
    fn contains(&self, p: Vec3) -> bool {
        self.voxels.contains(&(
            (p.x / REACH_TILE).floor() as i32,
            (p.z / REACH_TILE).floor() as i32,
            (p.y / REACH_TILE).floor() as i32,
        ))
    }
    fn overlaps(&self, t: [Vec3; 3]) -> bool {
        let lo = t[0].min(t[1]).min(t[2]) / REACH_TILE;
        let hi = t[0].max(t[1]).max(t[2]) / REACH_TILE;
        for zs in self
            .columns
            .range(lo.x.floor() as i32..=hi.x.floor() as i32)
            .map(|(_, zs)| zs)
        {
            for &(y0, y1) in zs
                .range(lo.z.floor() as i32..=hi.z.floor() as i32)
                .map(|(_, y)| y)
            {
                if y0 <= hi.y.floor() as i32 && y1 >= lo.y.floor() as i32 {
                    return true;
                }
            }
        }
        false
    }
}

// Render meshes have very large triangles and repeated instances. A horizontal uniform
// grid duplicates those triangles across many buckets; a BVH bounds both memory and queries.
struct RenderWalls {
    triangles: Vec<[Vec3; 3]>,
    nodes: Vec<Node>,
}
struct Node {
    lo: Vec3,
    hi: Vec3,
    first: usize,
    count: usize,
    children: Option<[usize; 2]>,
}
impl RenderWalls {
    fn new(mut triangles: Vec<[Vec3; 3]>) -> Self {
        fn build(tris: &mut [[Vec3; 3]], first: usize, nodes: &mut Vec<Node>) -> usize {
            let mut lo = Vec3::splat(f32::INFINITY);
            let mut hi = Vec3::splat(f32::NEG_INFINITY);
            for t in tris.iter() {
                for p in t {
                    lo = lo.min(*p);
                    hi = hi.max(*p);
                }
            }
            let index = nodes.len();
            nodes.push(Node {
                lo: lo - Vec3::splat(1e-5),
                hi: hi + Vec3::splat(1e-5),
                first,
                count: tris.len(),
                children: None,
            });
            if tris.len() > 8 {
                let size = hi - lo;
                let axis = if size.x >= size.y && size.x >= size.z {
                    0
                } else if size.y >= size.z {
                    1
                } else {
                    2
                };
                let middle = tris.len() / 2;
                tris.select_nth_unstable_by(middle, |a, b| {
                    (a[0][axis] + a[1][axis] + a[2][axis])
                        .total_cmp(&(b[0][axis] + b[1][axis] + b[2][axis]))
                });
                let (left, right) = tris.split_at_mut(middle);
                let l = build(left, first, nodes);
                let r = build(right, first + middle, nodes);
                nodes[index].children = Some([l, r]);
            }
            index
        }
        let mut nodes = Vec::new();
        if !triangles.is_empty() {
            build(&mut triangles, 0, &mut nodes);
        }
        Self { triangles, nodes }
    }
    fn raycast(&self, origin: Vec3, dir: Vec3, max: f32) -> Option<f32> {
        fn intersects(n: &Node, o: Vec3, d: Vec3, max: f32) -> bool {
            let mut near = 0_f32;
            let mut far = max;
            for axis in 0..3 {
                if d[axis].abs() < 1e-8 {
                    if o[axis] < n.lo[axis] || o[axis] > n.hi[axis] {
                        return false;
                    }
                } else {
                    let a = (n.lo[axis] - o[axis]) / d[axis];
                    let b = (n.hi[axis] - o[axis]) / d[axis];
                    near = near.max(a.min(b));
                    far = far.min(a.max(b));
                    if near > far {
                        return false;
                    }
                }
            }
            true
        }
        let mut stack = Vec::with_capacity(64);
        if !self.nodes.is_empty() {
            stack.push(0);
        }
        let mut best = max;
        let mut hit = false;
        while let Some(i) = stack.pop() {
            let n = &self.nodes[i];
            if !intersects(n, origin, dir, best) {
                continue;
            }
            if let Some([l, r]) = n.children {
                stack.push(r);
                stack.push(l);
                continue;
            }
            for [a, b, c] in &self.triangles[n.first..n.first + n.count] {
                let e1 = *b - *a;
                let e2 = *c - *a;
                let p = dir.cross(e2);
                let det = e1.dot(p);
                if det.abs() < 1e-9 {
                    continue;
                }
                let inverse = 1.0 / det;
                let s = origin - *a;
                let u = s.dot(p) * inverse;
                if !(0.0..=1.0).contains(&u) {
                    continue;
                }
                let q = s.cross(e1);
                let v = dir.dot(q) * inverse;
                if v < 0.0 || u + v > 1.0 {
                    continue;
                }
                let t = e2.dot(q) * inverse;
                if t >= 0.0 && t <= best {
                    best = t;
                    hit = true;
                }
            }
        }
        hit.then_some(best)
    }
}

#[derive(Clone)]
struct Primitive {
    positions: Vec<Vec3>,
    indices: Vec<u32>,
    name: String,
}
#[derive(Clone, Serialize, Deserialize)]
struct Hole {
    id: usize,
    centroid: [f32; 3],
    area: f32,
    kind: String,
    confirmed: bool,
    movement_checked: bool,
    nearest_player_start: usize,
    start_distance: f32,
    samples: usize,
    witness: Option<[f32; 3]>,
    evidence: Vec<Walk>,
    screenshot: Option<String>,
    visual_review: String,
}
#[derive(Clone, Serialize, Deserialize)]
struct Walk {
    start: [f32; 3],
    end: [f32; 3],
    drop: f32,
    render_below_ticks: usize,
}
fn vec3(v: &Value) -> Vec3 {
    Vec3::new(
        v[0].as_f64().unwrap() as f32,
        v[1].as_f64().unwrap() as f32,
        v[2].as_f64().unwrap() as f32,
    )
}
fn integer(v: &Value) -> Result<usize, Error> {
    v.as_u64()
        .map(|x| x as usize)
        .ok_or_else(|| "missing GLB integer".into())
}
fn word(b: &[u8], off: usize) -> Result<u32, Error> {
    Ok(u32::from_le_bytes(
        b.get(off..off + 4).ok_or("truncated GLB")?.try_into()?,
    ))
}
fn excluded(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    [
        "water",
        "river",
        "ocean",
        "sky",
        "foliage",
        "grass",
        "leaf",
        "leaves",
        "bush",
        "ivy",
        "treebranch",
        "fog",
        "cloud",
        "decal",
        "shadow",
        "dummy",
        "invisible",
        "billboard",
        "flower",
    ]
    .iter()
    .any(|s| n.contains(s))
}
// The extractor writes static identity nodes, embedded binary buffers and triangle primitives.
// Validate that contract instead of silently auditing a different scene.
fn glb(path: &Path) -> Result<Vec<Primitive>, Error> {
    let b = std::fs::read(path)?;
    if word(&b, 0)? != 0x46546c67 || word(&b, 4)? != 2 || word(&b, 8)? as usize != b.len() {
        return Err("invalid GLB header".into());
    }
    let jl = word(&b, 12)? as usize;
    if word(&b, 16)? != 0x4e4f534a {
        return Err("missing GLB JSON".into());
    }
    let j: Value = serde_json::from_slice(b.get(20..20 + jl).ok_or("truncated JSON")?)?;
    let bo = 20 + jl;
    let bl = word(&b, bo)? as usize;
    if word(&b, bo + 4)? != 0x004e4942 {
        return Err("missing embedded GLB buffer".into());
    }
    let bin = b.get(bo + 8..bo + 8 + bl).ok_or("truncated binary")?;
    for n in j["nodes"].as_array().ok_or("missing nodes")? {
        if ["matrix", "translation", "rotation", "scale", "children"]
            .iter()
            .any(|k| !n[*k].is_null())
        {
            return Err("nonidentity GLB node unsupported".into());
        }
    }
    let read = |id: usize, components: usize| -> Result<Vec<Vec<u8>>, Error> {
        let a = &j["accessors"][id];
        if !a["sparse"].is_null() {
            return Err("sparse accessor unsupported".into());
        }
        let view = &j["bufferViews"][integer(&a["bufferView"])?];
        if integer(&view["buffer"])? != 0 {
            return Err("external buffer unsupported".into());
        }
        let size = match integer(&a["componentType"])? {
            5126 | 5125 => 4,
            5123 => 2,
            5121 => 1,
            _ => return Err("accessor component unsupported".into()),
        } * components;
        let offset = view["byteOffset"].as_u64().unwrap_or(0) as usize
            + a["byteOffset"].as_u64().unwrap_or(0) as usize;
        let stride = view["byteStride"].as_u64().unwrap_or(size as u64) as usize;
        (0..integer(&a["count"])?)
            .map(|i| {
                Ok(bin
                    .get(offset + i * stride..offset + i * stride + size)
                    .ok_or("accessor out of bounds")?
                    .to_vec())
            })
            .collect()
    };
    let mut out = Vec::new();
    for mesh in j["meshes"].as_array().ok_or("missing meshes")? {
        for p in mesh["primitives"].as_array().ok_or("missing primitives")? {
            if p["mode"].as_u64().unwrap_or(4) != 4 {
                return Err("nontriangle primitive".into());
            }
            let mat = &j["materials"][integer(&p["material"])?];
            let name = mat["name"].as_str().unwrap_or("");
            if excluded(name) || mat["alphaMode"].as_str() == Some("BLEND") {
                continue;
            }
            let a = integer(&p["attributes"]["POSITION"])?;
            if j["accessors"][a]["componentType"] != 5126 || j["accessors"][a]["type"] != "VEC3" {
                return Err("position accessor unsupported".into());
            }
            let positions = read(a, 3)?
                .iter()
                .map(|b| {
                    Vec3::new(
                        f32::from_le_bytes(b[0..4].try_into().unwrap()),
                        f32::from_le_bytes(b[4..8].try_into().unwrap()),
                        f32::from_le_bytes(b[8..12].try_into().unwrap()),
                    )
                })
                .collect();
            let indices = read(integer(&p["indices"])?, 1)?
                .iter()
                .map(|b| match b.len() {
                    1 => u32::from(b[0]),
                    2 => u32::from(u16::from_le_bytes(b[..].try_into().unwrap())),
                    _ => u32::from_le_bytes(b[..].try_into().unwrap()),
                })
                .collect();
            out.push(Primitive {
                positions,
                indices,
                name: name.into(),
            });
        }
    }
    Ok(out)
}
fn key(p: Vec3, spacing: f32) -> Key {
    (
        (p.x / spacing).floor() as i32,
        (p.z / spacing).floor() as i32,
        (p.y / LAYER).round() as i32,
    )
}
fn point(k: Key, spacing: f32) -> Vec3 {
    Vec3::new(
        (k.0 as f32 + 0.5) * spacing,
        k.2 as f32 * LAYER,
        (k.1 as f32 + 0.5) * spacing,
    )
}
fn height(x: f32, z: f32, t: [Vec3; 3]) -> Option<f32> {
    let [a, b, c] = t;
    let d = (b.z - c.z) * (a.x - c.x) + (c.x - b.x) * (a.z - c.z);
    if d.abs() < 1e-9 {
        return None;
    }
    let u = ((b.z - c.z) * (x - c.x) + (c.x - b.x) * (z - c.z)) / d;
    let v = ((c.z - a.z) * (x - c.x) + (a.x - c.x) * (z - c.z)) / d;
    (u >= -1e-5 && v >= -1e-5 && u + v <= 1.00001)
        .then_some(u * a.y + v * b.y + (1.0 - u - v) * c.y)
}
#[cfg(test)]
fn raster(t: [Vec3; 3], spacing: f32, out: &mut HashMap<Key, f32>) {
    raster_reachable(t, spacing, out, None);
}
fn raster_reachable(
    t: [Vec3; 3],
    spacing: f32,
    out: &mut HashMap<Key, f32>,
    reach: Option<&Reach>,
) {
    let n = (t[1] - t[0]).cross(t[2] - t[0]).normalize_or_zero();
    if n.y < 50_f32.to_radians().cos() {
        return;
    }
    let lo = t[0].min(t[1]).min(t[2]);
    let hi = t[0].max(t[1]).max(t[2]);
    let x0 = (lo.x / spacing).floor() as i32;
    let x1 = (hi.x / spacing).floor() as i32;
    let z0 = (lo.z / spacing).floor() as i32;
    let z1 = (hi.z / spacing).floor() as i32;
    let mut tile = |xa: i32, xb: i32, za: i32, zb: i32| {
        for x in xa..=xb {
            for z in za..=zb {
                let px = (x as f32 + 0.5) * spacing;
                let pz = (z as f32 + 0.5) * spacing;
                if let Some(y) = height(px, pz, t)
                    && reach.is_none_or(|r| r.contains(Vec3::new(px, y, pz)))
                {
                    out.entry((x, z, (y / LAYER).round() as i32)).or_insert(y);
                }
            }
        }
    };
    if let Some(reach) =
        reach.filter(|_| (x1 as i64 - x0 as i64 + 1) * (z1 as i64 - z0 as i64 + 1) > 4096)
    {
        let first_cell = |tile: i32| ((tile as f32 * REACH_TILE / spacing) - 0.5).ceil() as i32;
        for (&x, zs) in reach
            .columns
            .range((lo.x / REACH_TILE).floor() as i32..=(hi.x / REACH_TILE).floor() as i32)
        {
            for &z in zs
                .range((lo.z / REACH_TILE).floor() as i32..=(hi.z / REACH_TILE).floor() as i32)
                .map(|(z, _)| z)
            {
                tile(
                    x0.max(first_cell(x)),
                    x1.min(first_cell(x + 1) - 1),
                    z0.max(first_cell(z)),
                    z1.min(first_cell(z + 1) - 1),
                );
            }
        }
    } else {
        tile(x0, x1, z0, z1);
    }
}
fn supported(world: &CollisionWorld, p: Vec3) -> bool {
    world
        .floor(p.x, p.z, p.y + GAP)
        .is_some_and(|y| (y - p.y).abs() <= GAP)
}
fn nearby(world: &CollisionWorld, p: Vec3) -> bool {
    [-NEAR, 0.0, NEAR].iter().any(|dx| {
        [-NEAR, 0.0, NEAR].iter().any(|dz| {
            let Some(y) = world.floor(p.x + dx, p.z + dz, p.y + 1.0) else {
                return false;
            };
            if (y - p.y).abs() > 1.0 {
                return false;
            }
            let from = Vec3::new(p.x + dx, y + 1.0, p.z + dz);
            let travel = p + Vec3::Y - from;
            let length = travel.length();
            length < 0.01 || world.raycast(from, travel / length, length).is_none()
        })
    })
}
fn settle(body: &mut Body, world: &CollisionWorld) -> bool {
    let start = Vec3::from(body.position);
    if world.raycast(start + Vec3::Y * 0.1, Vec3::Y, 1.6).is_some() {
        return false;
    }
    for _ in 0..30 {
        body.step(&BodyInput::default(), 1.0 / 60.0, world);
    }
    body.grounded && Vec3::from(body.position).distance(start) < 0.2
}
fn cluster(cells: &HashMap<Key, f32>, spacing: f32, starts: &[Vec3]) -> Vec<Hole> {
    let mut todo: HashSet<Key> = cells.keys().copied().collect();
    let mut holes = Vec::new();
    let mut ordered: Vec<_> = todo.iter().copied().collect();
    ordered.sort_unstable();
    for root in ordered {
        if !todo.remove(&root) {
            continue;
        }
        let mut q = VecDeque::from([root]);
        let mut members = Vec::new();
        while let Some(k) = q.pop_front() {
            members.push(k);
            for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1), (0, 0)] {
                for dy in -3..=3 {
                    let next = (k.0 + dx, k.1 + dz, k.2 + dy);
                    if todo.contains(&next) && (cells[&next] - cells[&k]).abs() <= 0.75 {
                        todo.remove(&next);
                        q.push_back(next);
                    }
                }
            }
        }
        let center = members
            .iter()
            .map(|k| Vec3::new(point(*k, spacing).x, cells[k], point(*k, spacing).z))
            .sum::<Vec3>()
            / members.len() as f32;
        let sample = members
            .iter()
            .min_by(|a, b| {
                point(**a, spacing)
                    .distance_squared(center)
                    .total_cmp(&point(**b, spacing).distance_squared(center))
            })
            .unwrap();
        let mut witness = point(*sample, spacing);
        witness.y = cells[sample];
        let (index, distance) = starts
            .iter()
            .enumerate()
            .map(|(i, p)| (i, p.distance(center)))
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .unwrap_or((0, f32::MAX));
        let area = members
            .iter()
            .map(|k| (k.0, k.1))
            .collect::<HashSet<_>>()
            .len() as f32
            * spacing
            * spacing;
        holes.push(Hole {
            id: 0,
            centroid: center.to_array(),
            area,
            kind: "floor_gap".into(),
            confirmed: false,
            movement_checked: false,
            nearest_player_start: index,
            start_distance: distance,
            samples: members.len(),
            witness: Some(witness.to_array()),
            evidence: Vec::new(),
            screenshot: None,
            visual_review: "pending".into(),
        });
    }
    holes.sort_by(|a, b| {
        (b.area / (1.0 + b.start_distance / 100.0))
            .total_cmp(&(a.area / (1.0 + a.start_distance / 100.0)))
            .then_with(|| a.centroid[0].total_cmp(&b.centroid[0]))
    });
    holes
}
// No move passed to slide exceeds a quarter radius, independent of requested speed.
fn sweep(world: &CollisionWorld, mut p: Vec3, delta: Vec2) -> Vec3 {
    let count = (delta.length() / SUBSTEP).ceil().max(1.0) as usize;
    let d = delta / count as f32;
    for _ in 0..count {
        let s = world.slide(p.to_array(), d.to_array());
        p.x += s[0];
        p.z += s[1];
    }
    p
}
fn render_ground(grid: &HashMap<Key, f32>, p: Vec3, spacing: f32) -> bool {
    let k = key(p, spacing);
    (-2..=24).any(|dy| {
        grid.get(&(k.0, k.1, k.2 + dy))
            .is_some_and(|h| *h >= p.y - 0.3 && *h <= p.y + 6.0)
    })
}
fn enclosed(walls: &RenderWalls, p: Vec3) -> bool {
    // Odd intersection parity along three horizontal rays. Open architecture is inconclusive.
    [Vec3::X, Vec3::Z, Vec3::new(0.6, 0.0, 0.8)]
        .iter()
        .all(|dir| {
            let mut origin = p;
            let mut distance = 0.0;
            let mut hits = 0;
            while distance < 50.0 && hits < 128 {
                let Some(d) = walls.raycast(origin, *dir, 50.0 - distance) else {
                    break;
                };
                distance += d + 0.01;
                origin += *dir * (d + 0.01);
                hits += 1;
            }
            hits % 2 == 1 && hits < 128
        })
}
fn real_check(
    h: &mut Hole,
    world: &CollisionWorld,
    walls: &RenderWalls,
    grid: &HashMap<Key, f32>,
    spacing: f32,
) {
    h.movement_checked = true;
    let p = Vec3::from(h.witness.unwrap_or(h.centroid));
    if h.kind == "wall_crossing" || h.kind == "enclosure_entry" {
        let start = Vec3::from(h.evidence[0].start);
        let direction = (p - start).with_y(0.0).normalize_or_zero();
        if direction.length_squared() < 0.5 {
            return;
        }
        let mut body = Body {
            position: start.to_array(),
            ..Body::default()
        };
        if !settle(&mut body, world) {
            return;
        }
        let steps = ((start.distance(p) + 1.0) / 0.04).ceil() as usize;
        for _ in 0..steps {
            let old = Vec3::from(body.position);
            body.step(
                &BodyInput {
                    root_motion: [-direction.x * 0.04, 0.0, direction.z * 0.04, 0.0],
                    ..BodyInput::default()
                },
                1.0 / 60.0,
                world,
            );
            let end = Vec3::from(body.position);
            let travel = (end - old).with_y(0.0);
            let len = travel.length();
            if len > 0.001 && walls.raycast(old + Vec3::Y, travel / len, len).is_some() {
                h.confirmed = true;
            }
        }
        h.evidence.push(Walk {
            start: start.to_array(),
            end: body.position,
            drop: start.y - body.position[1],
            render_below_ticks: 0,
        });
        return;
    }
    for d in [
        Vec2::X,
        -Vec2::X,
        Vec2::Y,
        -Vec2::Y,
        Vec2::new(0.707, 0.707),
        Vec2::new(-0.707, 0.707),
    ] {
        let mut spawn = None;
        for r in [0.75, 1.5, 2.5, 4.0, 6.0, 10.0] {
            let q = p + Vec3::new(d.x, 0.0, d.y) * r;
            if let Some(y) = world.floor(q.x, q.z, p.y + 0.75)
                && (y - p.y).abs() < 1.0
            {
                spawn = Some((Vec3::new(q.x, y, q.z), r));
                break;
            }
        }
        let Some((start, r)) = spawn else {
            continue;
        };
        let mut b = Body {
            position: start.to_array(),
            ..Body::default()
        };
        if !settle(&mut b, world) {
            continue;
        }
        let mut ticks = 0;
        let mut max_drop = 0_f32;
        // Cross the representative cell slowly, then let gravity settle for two seconds.
        let steps = ((r + 0.75) / 0.04).ceil() as usize;
        for tick in 0..steps + 120 {
            let delta = if tick < steps { -d * 0.04 } else { Vec2::ZERO };
            b.step(
                &BodyInput {
                    root_motion: [-delta.x, 0.0, delta.y, 0.0],
                    ..BodyInput::default()
                },
                1.0 / 60.0,
                world,
            );
            let q = Vec3::from(b.position);
            let cell = key(q, spacing);
            let target_ground = (-4..=4).any(|dy| {
                grid.get(&(cell.0, cell.1, key(p, spacing).2 + dy))
                    .is_some_and(|y| (*y - p.y).abs() < 1.0 && *y > q.y + DROP)
            });
            if target_ground && (q - p).with_y(0.0).length() < 1.0 {
                ticks += 1;
            }
            max_drop = max_drop.max(start.y - q.y);
        }
        let confirmed = max_drop > DROP && ticks > 5;
        h.confirmed |= confirmed;
        h.evidence.push(Walk {
            start: start.to_array(),
            end: b.position,
            drop: max_drop,
            render_below_ticks: ticks,
        });
    }
}
fn rng(s: &mut u64) -> f32 {
    *s ^= *s << 13;
    *s ^= *s >> 7;
    *s ^= *s << 17;
    (*s >> 40) as f32 / (1_u32 << 24) as f32
}
fn audit(root: &Path, id: &str, spacing: f32, probes: usize, recheck: bool) -> Result<(), Error> {
    let at = Instant::now();
    let prior: Option<Value> = if recheck {
        Some(serde_json::from_slice(&std::fs::read(format!(
            "cache/holes/{id}.json"
        ))?)?)
    } else {
        None
    };
    if let Some(p) = &prior
        && (p["thresholds"]["spacing"].as_f64() != Some(f64::from(spacing))
            || p["thresholds"]["probes"].as_u64() != Some(probes as u64))
    {
        return Err("recheck requires the original spacing and probe count".into());
    }
    let dir = root.join(id);
    let layout: Value = serde_json::from_slice(&std::fs::read(dir.join("layout.json"))?)?;
    let raw = CollisionMesh::from_bytes(&std::fs::read(dir.join("collision.bin"))?)?;
    let reach = Reach::new(&raw);
    let seeds: Vec<Vec3> = raw
        .indices
        .as_chunks::<3>()
        .0
        .iter()
        .filter_map(|t| {
            let p = t.map(|i| Vec3::from(raw.vertices[i as usize]));
            ((p[1] - p[0]).cross(p[2] - p[0]).normalize_or_zero().y.abs()
                >= 50_f32.to_radians().cos())
            .then_some((p[0] + p[1] + p[2]) / 3.0)
        })
        .collect();
    let world = CollisionWorld::from_mesh(&raw);
    drop(raw);
    let starts: Vec<Vec3> = layout["players"]
        .as_array()
        .ok_or("no players")?
        .iter()
        .map(|p| vec3(&p["translation"]))
        .collect();
    let models: Vec<_> = layout["models"]
        .as_object()
        .ok_or("no models")?
        .iter()
        .filter_map(|(n, m)| m["glb"].as_str().map(|p| (n.clone(), dir.join(p))))
        .collect();
    let loaded: Vec<Result<_, Error>> = models
        .par_iter()
        .map(|(name, path)| Ok((name.clone(), glb(path)?)))
        .collect();
    let mut assets = HashMap::new();
    for result in loaded {
        let (name, p) = result?;
        assets.insert(name, p);
    }
    eprintln!(
        "{id}: loaded {} models, {} walkable collision seeds ({:.1}s)",
        assets.len(),
        seeds.len(),
        at.elapsed().as_secs_f32()
    );
    let mut grid = HashMap::new();
    let mut wall_mesh = Vec::new();
    let mut material_counts = BTreeMap::<String, usize>::new();
    let mut triangles = 0;
    for row in layout["pieces"].as_array().ok_or("no pieces")? {
        let Some(primitives) = assets.get(row["model"].as_str().unwrap_or("")) else {
            continue;
        };
        let r = &row["rotation"];
        let rot = Quat::from_xyzw(
            r[0].as_f64().unwrap() as f32,
            r[1].as_f64().unwrap() as f32,
            r[2].as_f64().unwrap() as f32,
            r[3].as_f64().unwrap() as f32,
        );
        let translation = vec3(&row["translation"]);
        let scale = vec3(&row["scale"]);
        for p in primitives {
            let positions: Vec<_> = p
                .positions
                .iter()
                .map(|v| translation + rot * (*v * scale))
                .collect();
            for t in p.indices.as_chunks::<3>().0 {
                let t = t.map(|i| positions[i as usize]);
                triangles += 1;
                if !reach.overlaps(t) {
                    continue;
                }
                raster_reachable(t, spacing, &mut grid, Some(&reach));
                let ny = (t[1] - t[0]).cross(t[2] - t[0]).normalize_or_zero().y;
                if ny.abs() < 0.6428 {
                    wall_mesh.push(t);
                }
            }
            *material_counts.entry(p.name.clone()).or_default() += p.indices.len() / 3;
        }
    }
    drop(assets);
    eprintln!(
        "{id}: rasterized {triangles} render triangles, {} layered cells, {} wall triangles ({:.1}s)",
        grid.len(),
        wall_mesh.len(),
        at.elapsed().as_secs_f32()
    );
    let candidates: HashMap<Key, f32> = if recheck {
        HashMap::new()
    } else {
        grid.par_iter()
            .filter_map(|(k, y)| {
                let mut p = point(*k, spacing);
                p.y = *y;
                (!supported(&world, p) && nearby(&world, p)).then_some((*k, *y))
            })
            .collect()
    };
    let flagged_cells = prior
        .as_ref()
        .and_then(|p| p["flagged_cells"].as_u64())
        .unwrap_or(candidates.len() as u64);
    let mut holes: Vec<Hole> = if let Some(p) = &prior {
        serde_json::from_value(p["holes"].clone())?
    } else {
        cluster(&candidates, spacing, &starts)
    };
    if recheck {
        for h in &mut holes {
            h.confirmed = false;
            h.movement_checked = false;
            h.visual_review = "pending".into();
            if h.kind == "floor_gap" {
                h.evidence.clear();
            } else {
                h.evidence.truncate(1);
            }
        }
    }
    eprintln!(
        "{id}: clustered {} candidates ({:.1}s)",
        holes.len(),
        at.elapsed().as_secs_f32()
    );
    let walls = RenderWalls::new(wall_mesh);
    let events: Vec<_> = if recheck {
        Vec::new()
    } else {
        (0..probes)
            .into_par_iter()
            .filter_map(|i| {
                if seeds.is_empty() {
                    return None;
                }
                let mut s = 0x5e61726f_u64;
                s ^= (i as u64 + 1).wrapping_mul(0x9e3779b97f4a7c15);
                let mut p = seeds[(rng(&mut s) * seeds.len() as f32) as usize % seeds.len()];
                let start = p;
                let mut heading = rng(&mut s) * std::f32::consts::TAU;
                for _ in 0..64 {
                    heading += (rng(&mut s) - 0.5) * 1.5;
                    let delta = Vec2::new(heading.cos(), heading.sin()) * (0.5 + rng(&mut s) * 2.5);
                    let old = p;
                    let q = sweep(&world, p, delta);
                    let travel = q - old;
                    let len = travel.length();
                    if len > 0.01 {
                        let origin = old + Vec3::Y * 1.0;
                        if let Some(hit) = walls.raycast(origin, travel / len, len) {
                            let contact = origin + travel / len * hit;
                            // Collision within the same ray segment or 30 cm either side explains a visual wall.
                            if world
                                .raycast(origin - travel / len * GAP, travel / len, len + 2.0 * GAP)
                                .is_none()
                            {
                                let kind = if enclosed(&walls, q + Vec3::Y) {
                                    "enclosure_entry"
                                } else {
                                    "wall_crossing"
                                };
                                return Some((contact, kind, old));
                            }
                        }
                    }
                    p = q;
                    match world.floor(p.x, p.z, p.y + 0.6) {
                        Some(y) if p.y - y <= 0.75 => p.y = y,
                        floor => {
                            if render_ground(&grid, p, spacing)
                                && floor.is_none_or(|y| p.y - y > DROP)
                            {
                                return Some((p, "probe_floor_gap", start));
                            }
                            break;
                        }
                    }
                }
                None
            })
            .collect()
    };
    let probe_events = prior
        .as_ref()
        .and_then(|p| p["probe_events"].as_u64())
        .unwrap_or(events.len() as u64);
    eprintln!(
        "{id}: finished probes, {probe_events} events ({:.1}s)",
        at.elapsed().as_secs_f32()
    );
    for (p, kind, start) in events {
        if holes
            .iter()
            .any(|h| Vec3::from(h.centroid).distance(p) < 1.0)
        {
            continue;
        }
        let (index, distance) = starts
            .iter()
            .enumerate()
            .map(|(i, s)| (i, s.distance(p)))
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .unwrap_or((0, f32::MAX));
        holes.push(Hole {
            id: 0,
            centroid: p.to_array(),
            area: 0.0,
            kind: kind.into(),
            confirmed: false,
            movement_checked: false,
            nearest_player_start: index,
            start_distance: distance,
            samples: 1,
            witness: Some(p.to_array()),
            evidence: vec![Walk {
                start: start.to_array(),
                end: p.to_array(),
                drop: 0.0,
                render_below_ticks: 0,
            }],
            screenshot: None,
            visual_review: "pending".into(),
        });
    }
    // Only highest ranked suspects use full Body gravity and ground-following.
    for h in holes.iter_mut().filter(|h| h.kind == "floor_gap").take(30) {
        real_check(h, &world, &walls, &grid, spacing);
    }
    for h in holes.iter_mut().filter(|h| h.kind != "floor_gap").take(10) {
        real_check(h, &world, &walls, &grid, spacing);
    }
    for (i, h) in holes.iter_mut().enumerate() {
        h.id = i + 1;
    }
    let elapsed = at.elapsed().as_secs_f64();
    let scan_seconds = prior
        .as_ref()
        .and_then(|p| p["scan_seconds"].as_f64().or_else(|| p["seconds"].as_f64()))
        .unwrap_or(elapsed);
    let validation_seconds = if recheck { elapsed } else { 0.0 };
    let output = json!({"map":id,"seconds":scan_seconds+validation_seconds,"scan_seconds":scan_seconds,"validation_seconds":validation_seconds,"thresholds":{"spacing":spacing,"floor_tolerance":GAP,"slope_degrees":50,"layer_quantization":LAYER,"nearby_horizontal":NEAR,"nearby_vertical":1.0,"fall":DROP,"radius":RADIUS,"substep":SUBSTEP,"seed":"0x5e61726f","probes":probes,"walk_moves":64,"body_limit":30},"render_triangles":triangles,"render_cells":grid.len(),"flagged_cells":flagged_cells,"probe_events":probe_events,"collision_seeds":seeds.len(),"holes":holes,"retained_material_triangles":material_counts.iter().map(|(name,count)|json!({"material":name,"triangles":count})).collect::<Vec<_>>()});
    std::fs::create_dir_all("cache/holes")?;
    std::fs::write(
        format!("cache/holes/{id}.json"),
        serde_json::to_vec_pretty(&output)?,
    )?;
    println!(
        "{id}: {} found, {} body-confirmed; {elapsed:.2}s",
        holes.len(),
        holes.iter().filter(|h| h.confirmed).count()
    );
    Ok(())
}
fn main() -> Result<(), Error> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--report") {
        return report();
    }
    let mut root = "cache/maps";
    let mut ids = Vec::new();
    let mut spacing = 0.5;
    let mut probes = 2048;
    let mut recheck = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--recheck" => recheck = true,
            "--spacing" => {
                i += 1;
                spacing = args.get(i).ok_or("missing spacing")?.parse()?;
            }
            "--probes" => {
                i += 1;
                probes = args.get(i).ok_or("missing probes")?.parse()?;
            }
            s if s.starts_with('m') => ids.push(s.to_string()),
            s => root = s,
        }
        i += 1;
    }
    if !(0.25..=0.5).contains(&spacing) {
        return Err("spacing must be 0.25..0.5".into());
    }
    if ids.is_empty() {
        ids = std::fs::read_dir(root)?
            .filter_map(Result::ok)
            .filter(|e| e.path().join("collision.bin").is_file())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|s| !s.starts_with("m89"))
            .collect();
        ids.sort();
    }
    for id in ids {
        if !id.starts_with("m89") {
            audit(Path::new(root), &id, spacing, probes, recheck)?;
        }
    }
    Ok(())
}

fn report() -> Result<(), Error> {
    use std::fmt::Write;
    let mut files: Vec<_> = std::fs::read_dir("cache/holes")?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.file_name().is_some_and(|n| {
                let n = n.to_string_lossy();
                n.starts_with('m') && n.ends_with(".json") && !n.contains(".review.")
            })
        })
        .collect();
    files.sort();
    let mut rows = Vec::new();
    for path in files {
        let mut data: Value = serde_json::from_slice(&std::fs::read(&path)?)?;
        let id = data["map"].as_str().ok_or("missing map")?.to_string();
        data["thresholds"]["reach_tile"] = json!(REACH_TILE);
        data["thresholds"]["probe_body_limit"] = json!(10);
        data["thresholds"]["witness_radius"] = json!(1.0);
        data["thresholds"]["settle_ticks"] = json!(30);
        data["thresholds"]["headroom"] = json!(1.6);
        let review_path = format!("cache/holes/{id}.review.json");
        let review: Value = if Path::new(&review_path).is_file() {
            let bytes = std::fs::read(review_path)?;
            serde_json::from_slice(bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&bytes))?
        } else {
            json!({"reviews":[]})
        };
        // Windows PowerShell's JSON object keys are case insensitive. Material names are not.
        if let Some(materials) = data["retained_material_triangles"].as_object() {
            data["retained_material_triangles"] = json!(
                materials
                    .iter()
                    .map(|(name, count)| json!({"material":name,"triangles":count}))
                    .collect::<Vec<_>>()
            );
        }
        for hole in data["holes"].as_array_mut().ok_or("missing holes")? {
            let reproduced = hole["body_confirmed"]
                .as_bool()
                .unwrap_or_else(|| hole["confirmed"].as_bool().unwrap_or(false));
            hole["body_confirmed"] = json!(reproduced);
            hole["confirmed"] = json!(false);
            hole["visual_review"] = json!("pending");
            hole["screenshot"] = Value::Null;
            hole["review_reason"] = Value::Null;
            for r in review["reviews"].as_array().into_iter().flatten() {
                if r["id"] == hole["id"]
                    && r["centroid"].as_array().is_some_and(|a| a.len() == 3)
                    && vec3(&r["centroid"]).distance(vec3(&hole["centroid"])) < 1.0
                {
                    hole["screenshot"] = r["screenshot"].clone();
                    hole["visual_review"] = r["verdict"].clone();
                    hole["review_reason"] = r["reason"].clone();
                    hole["confirmed"] = json!(reproduced && r["verdict"] == "retain");
                    break;
                }
            }
        }
        let holes = data["holes"].as_array().unwrap();
        let found = holes.len();
        let reproduced = holes.iter().filter(|h| h["body_confirmed"] == true).count();
        let confirmed = holes.iter().filter(|h| h["confirmed"] == true).count();
        let rejected = holes
            .iter()
            .filter(|h| h["visual_review"] == "reject")
            .count();
        let checked = holes
            .iter()
            .filter(|h| h["movement_checked"] == true)
            .count();
        data["counts"] = json!({"found":found,"body_reproduced":reproduced,"confirmed":confirmed,"false_positives_rejected":rejected,"movement_checked":checked});
        std::fs::write(path, serde_json::to_vec_pretty(&data)?)?;
        rows.push(data);
    }
    let total_time: f64 = rows.iter().filter_map(|r| r["seconds"].as_f64()).sum();
    let mut out = String::from(
        "# Collision holes\n\nHeadless audit of all nine gameplay maps; m89 is excluded because it has no walking terrain.\nCoordinates are Bevy world metres (source X mirrored).\nRaw candidates are render/collision discrepancies, not automatically gameplay defects.\n`confirmed` requires both a Body reproduction and a retained visual review; unchecked and inconclusive candidates remain unconfirmed.\n\n## Totals\n\n| Map | Found | Body reproduced | Visually confirmed | Rejected false positives | Body checked | Audit seconds |\n| --- | ---: | ---: | ---: | ---: | ---: | ---: |\n",
    );
    for r in &rows {
        let c = &r["counts"];
        writeln!(
            out,
            "| {} | {} | {} | {} | {} | {} | {:.2} |",
            r["map"].as_str().unwrap(),
            c["found"],
            c["body_reproduced"],
            c["confirmed"],
            c["false_positives_rejected"],
            c["movement_checked"],
            r["seconds"].as_f64().unwrap()
        )?;
    }
    let sum = |key: &str| {
        rows.iter()
            .filter_map(|r| r["counts"][key].as_u64())
            .sum::<u64>()
    };
    writeln!(
        out,
        "| Total | {} | {} | {} | {} | {} | {total_time:.2} |",
        sum("found"),
        sum("body_reproduced"),
        sum("confirmed"),
        sum("false_positives_rejected"),
        sum("movement_checked")
    )?;
    writeln!(
        out,
        "\nSummed headless scan and validation time: {total_time:.2} seconds ({:.2} minutes).\nParallel batch wall time is recorded in STATUS.md.\n",
        total_time / 60.0
    )?;
    out.push_str("These timings include the final full scans and the latest cached validation pass per map, excluding exploratory scans, compilation and screenshot review.\n\n");
    out.push_str("## Method and thresholds\n\nA conservative 4 m voxel mask around walkable collision removes distant background surfaces before raster storage; large triangles iterate only occupied horizontal tiles.\nRender walls use a BVH to avoid duplicating large triangles into thousands of horizontal buckets.\nThe floor scan rasterizes upward-facing triangles from every placed map-piece GLB at 0.5 m cell centres, preserving separate height layers at 0.25 m quantization.\nIt accepts slopes up to 50 degrees and flags no collision within 0.3 m above or below the exact render height.\nWater, sky, foliage, grass, leaf, bush, ivy, flowers, clouds, fog, decals, shadow/dummy/invisible helpers and blended materials are excluded by exported material/MTD names.\nA candidate needs a collision floor within 2 m horizontally and 1 m vertically, with an unobstructed collision ray to it.\nFour-neighbour components connect heights differing by at most 0.75 m; area is the union of their horizontal grid cells.\nThe reachability filter clips large unsupported surfaces to their nearby supported edges, so area is a sampled discrepancy area rather than the whole decorative mesh area.\n\nEach map runs 2,048 deterministic probes, starting at walkable collision triangle centroids, with 64 random moves of 0.5 to 3 m.\nSeed is 0x5e61726f XOR probe index times 0x9e3779b97f4a7c15.\nRayon runs independent probes in parallel; capsule radius is the Body's 0.4 m, and every slide call moves at most 0.1 m.\nProbes flag absent floor or drops over 5 m under sampled render ground, and render-wall crossings with no collision hit on the movement ray extended by 0.3 m.\nOdd render-wall intersection parity along three horizontal rays labels enclosure-entry candidates, but does not prove watertightness or distinguish a hollow room from solid rock.\nWall matching is a conservative ray test, not a full nearest-triangle distance calculation.\n\nThe top 30 ranked floor candidates and first 10 independent probe candidates use Body at 60 Hz, 0.04 m per tick, in six directions for floor gaps.\nStarts need 1.6 m headroom and 30 stable standing ticks with less than 0.2 m displacement.\nFloor reproduction requires a drop over 5 m and more than five ticks over 5 m below the witness render-height layer while within 1 m horizontally of that witness.\nWall reproduction requires an actual Body movement segment to cross a retained render triangle.\nRanking is area divided by (1 + nearest-start distance / 100); nearest-start distance is a geometric proxy, not a proven navigable route.\nCentroid is descriptive; witness is an actual sampled cell used for checks.\n\nThis audit uses default viewer map pieces without event-controlled objects, rendering, animation or player HKS.\nIt cannot certify collision completeness, detect sub-grid floor gaps, or prove every candidate is accessible using jumping, grappling or swimming.\nUnreproduced and unchecked candidates are preserved for investigation.\n\n## Ranked candidates\n\n");
    for r in &rows {
        writeln!(
            out,
            "### {}\n\n| ID | Kind | Area m² | Centroid x, y, z | Nearest start (distance m) | Status | Screenshot |\n| ---: | --- | ---: | --- | --- | --- | --- |",
            r["map"].as_str().unwrap()
        )?;
        for h in r["holes"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|h| h["visual_review"] != "reject")
            .take(10)
        {
            let p = vec3(&h["centroid"]);
            let status = if h["confirmed"] == true {
                "confirmed"
            } else if h["body_confirmed"] == true {
                "Body reproduced, visual pending/inconclusive"
            } else if h["movement_checked"] == true {
                "not reproduced"
            } else {
                "unchecked"
            };
            writeln!(
                out,
                "| {} | {} | {:.2} | {:.2}, {:.2}, {:.2} | {} ({:.1}) | {} | {} |",
                h["id"],
                h["kind"].as_str().unwrap(),
                h["area"].as_f64().unwrap(),
                p.x,
                p.y,
                p.z,
                h["nearest_player_start"],
                h["start_distance"].as_f64().unwrap(),
                status,
                h["screenshot"].as_str().unwrap_or("pending")
            )?;
        }
        out.push('\n');
        let confirmed: Vec<_> = r["holes"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|h| h["confirmed"] == true)
            .take(3)
            .collect();
        if !confirmed.is_empty() {
            out.push_str("Visually confirmed reproductions (ranked):\n\n| ID | Area m² | Witness x, y, z | Screenshot |\n| ---: | ---: | --- | --- |\n");
            for h in confirmed {
                let p = vec3(&h["witness"]);
                writeln!(
                    out,
                    "| {} | {:.2} | {:.2}, {:.2}, {:.2} | {} |",
                    h["id"],
                    h["area"].as_f64().unwrap(),
                    p.x,
                    p.y,
                    p.z,
                    h["screenshot"].as_str().unwrap_or("pending")
                )?;
            }
            out.push('\n');
        }
        for h in r["holes"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|h| !h["review_reason"].is_null())
        {
            writeln!(
                out,
                "Review {}: {}.\n",
                h["id"],
                h["review_reason"].as_str().unwrap_or("")
            )?;
        }
    }
    out.push_str("## Reproduce\n\n```powershell\n$env:CARGO_TARGET_DIR='target/alt'\n$env:CARGO_BUILD_JOBS='6'\n$env:RAYON_NUM_THREADS='6'\n& \"$env:USERPROFILE\\.cargo\\bin\\cargo.exe\" test --release -p sekiro-sim --example collision_holes -j 6\n& \"$env:USERPROFILE\\.cargo\\bin\\cargo.exe\" build --release -p sekiro-sim --example collision_holes -j 6\n.\\target\\alt\\release\\examples\\collision_holes.exe cache/maps --spacing 0.5 --probes 2048\n.\\target\\alt\\release\\examples\\collision_holes.exe --report\n```\n\nPass map IDs after `cache/maps` to audit a subset.\nUse `--recheck` with map IDs to repeat Body validation of cached candidates without rerunning the random probes or clustering; geometry is reloaded, and scan and validation times are recorded separately.\nRecheck is valid only when geometry, spacing and probe count are unchanged.\n`--report` merges ID-and-coordinate-matched reviews from `cache/holes/<map>.review.json`, annotates per-map JSON and regenerates this document.\nScreenshot review is manual and must be repeated if geometry or thresholds change.\nViewer command: `target/alt/release/sekiro-rs.exe --map <id> --groups off --camera x,y,z,yaw,pitch --screenshot <absolute repo path> --at 1`.\nCapture from the reproduction start plus 1.4 m eye height when a high camera is inside a ceiling.\nMibu review captures use `--groups display --hide m9,m8` to remove occluding far/helper pieces; the audit still loads all map pieces.\nEvery viewer must be capped at 90 seconds and killed by its exact process ID on timeout.\n\n## Verification and changed files\n\nSix tests pass: missing-patch detection, solid-floor detection, a 100 m thin-wall sweep, Body reproduction and unstable-start rejection, BVH ray equivalence, and bounded rasterization of a 20 km background floor.\nThe tool uses a dev-only workspace Rayon dependency in `crates/sim/Cargo.toml`; implementation and tests are in `crates/sim/examples/collision_holes.rs`.\nCargo.lock records the dev-only Rayon dependency automatically.\nThis report and STATUS.md are the tracked documentation changes.\nNo collision engine code, protected brain code, game install files, links, junctions or commits are changed by this task.\nFinal workspace check results are recorded in STATUS.md.\n");
    std::fs::write("docs/COLLISION-HOLES.md", out)?;
    println!(
        "Reported {} maps; summed scan time {total_time:.2}s",
        rows.len()
    );
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn quad(m: &mut CollisionMesh, x0: f32, x1: f32, z0: f32, z1: f32) {
        let b = m.vertices.len() as u32;
        m.vertices
            .extend([[x0, 0.0, z0], [x0, 0.0, z1], [x1, 0.0, z1], [x1, 0.0, z0]]);
        m.indices.extend([b, b + 1, b + 2, b, b + 2, b + 3]);
    }
    fn scan(m: &CollisionMesh) -> Vec<Hole> {
        let world = CollisionWorld::from_mesh(m);
        let reach = Reach::new(m);
        let mut render = CollisionMesh::default();
        quad(&mut render, -3.0, 3.0, -3.0, 3.0);
        let mut grid = HashMap::new();
        for t in render.indices.as_chunks::<3>().0 {
            raster_reachable(
                t.map(|i| Vec3::from(render.vertices[i as usize])),
                0.5,
                &mut grid,
                Some(&reach),
            );
        }
        let cells = grid
            .into_iter()
            .filter(|(k, y)| {
                let mut p = point(*k, 0.5);
                p.y = *y;
                !supported(&world, p) && nearby(&world, p)
            })
            .collect();
        cluster(&cells, 0.5, &[Vec3::ZERO])
    }
    #[test]
    fn missing_patch_found() {
        let mut m = CollisionMesh::default();
        quad(&mut m, -3.0, -1.0, -3.0, 3.0);
        quad(&mut m, 1.0, 3.0, -3.0, 3.0);
        quad(&mut m, -1.0, 1.0, -3.0, -1.0);
        quad(&mut m, -1.0, 1.0, 1.0, 3.0);
        let h = scan(&m);
        assert_eq!(h.len(), 1);
        assert!((h[0].area - 4.0).abs() < 0.01);
        assert!(Vec3::from(h[0].centroid).length() < 0.01);
    }
    #[test]
    fn solid_floor_no_holes() {
        let mut m = CollisionMesh::default();
        quad(&mut m, -3.0, 3.0, -3.0, 3.0);
        assert!(scan(&m).is_empty());
    }
    #[test]
    fn fast_probe_cannot_tunnel() {
        let mut m = CollisionMesh::default();
        m.vertices.extend([
            [0.0, -1.0, -10.0],
            [0.0, 5.0, -10.0],
            [0.0, 5.0, 10.0],
            [0.0, -1.0, 10.0],
        ]);
        m.indices.extend([0, 1, 2, 0, 2, 3]);
        let w = CollisionWorld::from_mesh(&m);
        let end = sweep(&w, Vec3::new(-10.0, 0.0, 0.0), Vec2::new(100.0, 0.0));
        assert!(end.x <= -RADIUS + 0.01, "{end:?}");
    }
    #[test]
    fn render_bvh_matches_triangle_rays() {
        let mut mesh = CollisionMesh::default();
        for x in -4..=4 {
            quad(&mut mesh, x as f32, x as f32 + 0.9, -3.0, 3.0);
        }
        let world = CollisionWorld::from_mesh(&mesh);
        let render = RenderWalls::new(
            mesh.indices
                .as_chunks::<3>()
                .0
                .iter()
                .map(|t| t.map(|i| Vec3::from(mesh.vertices[i as usize])))
                .collect(),
        );
        for x in -50..=50 {
            let origin = Vec3::new(x as f32 * 0.1, 3.0, 0.5);
            for direction in [-Vec3::Y, Vec3::new(0.3, -1.0, 0.2).normalize()] {
                let expected = world.raycast(origin, direction, 10.0);
                let actual = render.raycast(origin, direction, 10.0);
                assert_eq!(
                    expected.is_some(),
                    actual.is_some(),
                    "origin={origin:?}, direction={direction:?}, expected={expected:?}, actual={actual:?}"
                );
                if let (Some(a), Some(b)) = (expected, actual) {
                    assert!((a - b).abs() < 1e-5);
                }
            }
        }
    }
    #[test]
    fn distant_background_floor_is_bounded_by_playable_tiles() {
        let mut collision = CollisionMesh::default();
        quad(&mut collision, -3.0, 3.0, -3.0, 3.0);
        let reach = Reach::new(&collision);
        let mut grid = HashMap::new();
        raster_reachable(
            [
                Vec3::new(-10000.0, 0.0, -10000.0),
                Vec3::new(-10000.0, 0.0, 10000.0),
                Vec3::new(10000.0, 0.0, 10000.0),
            ],
            0.5,
            &mut grid,
            Some(&reach),
        );
        assert!(!grid.is_empty());
        assert!(grid.len() <= 1024, "{} cells", grid.len());
        assert!(!reach.contains(Vec3::new(100.0, 0.0, 100.0)));
    }
    #[test]
    fn body_reproduces_missing_patch_but_rejects_unstable_start() {
        let mut m = CollisionMesh::default();
        quad(&mut m, -4.0, -1.0, -4.0, 4.0);
        quad(&mut m, 1.0, 4.0, -4.0, 4.0);
        let world = CollisionWorld::from_mesh(&m);
        let empty = RenderWalls::new(Vec::new());
        let mut grid = HashMap::new();
        raster(
            [
                Vec3::new(-4.0, 0.0, -4.0),
                Vec3::new(-4.0, 0.0, 4.0),
                Vec3::new(4.0, 0.0, 4.0),
            ],
            0.5,
            &mut grid,
        );
        raster(
            [
                Vec3::new(-4.0, 0.0, -4.0),
                Vec3::new(4.0, 0.0, 4.0),
                Vec3::new(4.0, 0.0, -4.0),
            ],
            0.5,
            &mut grid,
        );
        let cells = grid
            .iter()
            .filter(|(k, y)| {
                let mut p = point(**k, 0.5);
                p.y = **y;
                !supported(&world, p) && nearby(&world, p)
            })
            .map(|(k, y)| (*k, *y))
            .collect();
        let mut h = cluster(&cells, 0.5, &[Vec3::ZERO]).remove(0);
        real_check(&mut h, &world, &empty, &grid, 0.5);
        assert!(
            h.confirmed,
            "{:?}",
            h.evidence.iter().map(|w| w.drop).collect::<Vec<_>>()
        );
        assert!(!settle(
            &mut Body::default(),
            &CollisionWorld::from_mesh(&CollisionMesh::default())
        ));
    }
}
