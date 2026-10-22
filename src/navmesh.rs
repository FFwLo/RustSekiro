//! Enemy navigation over the game's own navmesh: each map block's `nvmhktbnd` holds one Havok
//! `hkaiNavMesh` per piece (`n<area>_<piece>.hkx`), walkable convex polygons in the block's own
//! space that the MSB map offset places in the world (sr_map.rs loads them, through sekiro-rs's
//! tagfile reader). Polygons are linked where they share an edge: inside a piece by the edges'
//! own `oppositeFace`, between pieces by the block NVA's connectors (a face and edge of one piece
//! against a face of the other; pieces without connectors are islands, as in the game). Paths are an A*
//! over those shared edges, straightened with a funnel; the line queries walk a straight line
//! across the links until it leaves the mesh.
//!
//! The AI's mesh queries (ai.rs: CheckDoesExistPath, IsExistMeshOnLine,
//! GetExistMeshOnLineDistSpecifyAngle) and its moves (MoveToSomewhere, BackToHome, patrol
//! points) read the installed mesh; with none installed (own map, no map) they answer as before.
//! gap: the edges' flags and user data, the pieces' user edges (jumps, ladders), the connectors'
//! conditions (all taken as open), the `hkaiDirectedGraphExplicitCost` cluster graph, `onav`
//! object meshes and the exe's search costs are not read: every link costs its length; blocks
//! do not link to each other (no NVA connector names another block).
use bevy::math::{Vec2, Vec3, Vec3Swizzles};
use std::collections::{BinaryHeap, HashMap};
use std::sync::{Arc, RwLock};

/// A piece's polygons placed in the world (Bevy space): vertex positions, each face's corner
/// indices into them, and for each face edge (corner k to k + 1) the face across it (`None` on
/// the piece's boundary).
pub struct Piece {
    pub vertices: Vec<Vec3>,
    pub faces: Vec<Vec<u32>>,
    pub opposite: Vec<Vec<Option<u32>>>,
}

/// A link between two pieces (an NVA connector point): face `from.1` of piece `from.0` reaches
/// face `to.1` of piece `to.0` through its edge `a`-`b`.
pub struct Join {
    pub from: (usize, u32),
    pub to: (usize, u32),
    pub a: Vec3,
    pub b: Vec3,
}

/// A link to a neighbouring polygon through the edge both share (from `a` to `b`).
#[derive(Clone, Copy, Debug)]
struct Link {
    to: u32,
    a: Vec3,
    b: Vec3,
}

struct Poly {
    corners: Vec<Vec3>,
    min: Vec2,
    max: Vec2,
    links: Vec<Link>,
}

/// How far below or above a polygon a body's feet may be and still stand on it.
const BELOW: f32 = 1.0;
const ABOVE: f32 = 2.0;
/// How far off the mesh (horizontally) a body may stand and still start a search from its
/// nearest polygon: enemies placed or pushed just past an edge.
const NEAR: f32 = 1.5;
/// The most polygons one search opens before it gives up (no path).
const SEARCH_LIMIT: usize = 20_000;
const CELL: f32 = 4.0;

#[derive(Default)]
pub struct NavMesh {
    polys: Vec<Poly>,
    grid: HashMap<(i32, i32), Vec<u32>>,
    /// Each polygon's connected area (the lowest polygon index in it): a search between two
    /// areas ends before it starts.
    area: Vec<u32>,
}

fn cell(p: Vec2) -> (i32, i32) {
    ((p.x / CELL).floor() as i32, (p.y / CELL).floor() as i32)
}

/// Twice the signed area of the triangle (a, b, c) in XZ.
fn area2(a: Vec2, b: Vec2, c: Vec2) -> f32 {
    (b - a).perp_dot(c - a)
}

impl NavMesh {
    pub fn build(pieces: &[Piece], joins: &[Join]) -> Self {
        let mut mesh = Self::default();
        let mut first = Vec::with_capacity(pieces.len());
        for piece in pieces {
            // Faces with fewer than three corners or corners past the vertices are left out.
            let mut index = Vec::with_capacity(piece.faces.len());
            for face in &piece.faces {
                let ok = face.len() >= 3 && face.iter().all(|&i| (i as usize) < piece.vertices.len());
                index.push(ok.then(|| {
                    let corners: Vec<Vec3> = face.iter().map(|&i| piece.vertices[i as usize]).collect();
                    let flat = corners.iter().map(|c| c.xz());
                    let min = flat.clone().fold(Vec2::INFINITY, Vec2::min);
                    let max = flat.fold(Vec2::NEG_INFINITY, Vec2::max);
                    mesh.polys.push(Poly { corners, min, max, links: Vec::new() });
                    mesh.polys.len() as u32 - 1
                }));
            }
            for (f, face) in piece.faces.iter().enumerate() {
                let Some(p) = index[f] else { continue };
                for (k, &i) in face.iter().enumerate() {
                    let (a, b) = (piece.vertices[i as usize], piece.vertices[face[(k + 1) % face.len()] as usize]);
                    if let Some(q) = piece.opposite.get(f).and_then(|o| o.get(k)).copied().flatten().and_then(|o| index.get(o as usize).copied().flatten()) {
                        mesh.polys[p as usize].links.push(Link { to: q, a, b });
                    }
                }
            }
            first.push(index);
        }
        let poly = |(piece, face): (usize, u32)| first.get(piece).and_then(|i| i.get(face as usize)).copied().flatten();
        for join in joins {
            if let (Some(p), Some(q)) = (poly(join.from), poly(join.to)) {
                mesh.polys[p as usize].links.push(Link { to: q, a: join.a, b: join.b });
            }
        }
        mesh.area = (0..mesh.polys.len() as u32).collect();
        fn root(area: &mut [u32], mut p: u32) -> u32 {
            while area[p as usize] != p {
                area[p as usize] = area[area[p as usize] as usize];
                p = area[p as usize];
            }
            p
        }
        for p in 0..mesh.polys.len() {
            for k in 0..mesh.polys[p].links.len() {
                let (a, b) = (root(&mut mesh.area, p as u32), root(&mut mesh.area, mesh.polys[p].links[k].to));
                mesh.area[a.max(b) as usize] = a.min(b);
            }
        }
        for p in 0..mesh.area.len() as u32 {
            mesh.area[p as usize] = root(&mut mesh.area, p);
        }
        for (index, poly) in mesh.polys.iter().enumerate() {
            let (lo, hi) = (cell(poly.min), cell(poly.max));
            for x in lo.0..=hi.0 {
                for z in lo.1..=hi.1 {
                    mesh.grid.entry((x, z)).or_default().push(index as u32);
                }
            }
        }
        mesh
    }

    pub fn polygon_count(&self) -> usize {
        self.polys.len()
    }

    #[cfg(test)]
    pub(crate) fn corners(&self, poly: u32) -> Vec<Vec3> {
        self.polys[poly as usize].corners.clone()
    }

    /// How many polygons the links reach from `from` (itself included).
    #[cfg(test)]
    pub(crate) fn reach(&self, from: u32) -> usize {
        let mut seen = std::collections::HashSet::from([from]);
        let mut stack = vec![from];
        while let Some(p) = stack.pop() {
            for l in &self.polys[p as usize].links {
                if seen.insert(l.to) {
                    stack.push(l.to);
                }
            }
        }
        seen.len()
    }

    /// The polygon's height under `p` (XZ), from the triangle fan holding it; `None` outside.
    fn height_at(&self, poly: u32, p: Vec2) -> Option<f32> {
        let c = &self.polys[poly as usize].corners;
        for k in 1..c.len() - 1 {
            let (a, b, d) = (c[0], c[k], c[k + 1]);
            let (a2, b2, d2) = (a.xz(), b.xz(), d.xz());
            let area = area2(a2, b2, d2);
            if area.abs() < 1e-9 {
                continue;
            }
            let u = area2(p, b2, d2) / area;
            let v = area2(a2, p, d2) / area;
            let w = 1.0 - u - v;
            if u >= -1e-4 && v >= -1e-4 && w >= -1e-4 {
                return Some(a.y * u + b.y * v + d.y * w);
            }
        }
        None
    }

    /// The polygon a body with its feet at `p` stands on: the one under it within its reach
    /// nearest in height, else the nearest polygon edge within [`NEAR`].
    pub fn locate(&self, p: Vec3) -> Option<u32> {
        let candidates = self.grid.get(&cell(p.xz()))?;
        let mut best: Option<(f32, u32)> = None;
        for &i in candidates {
            if let Some(y) = self.height_at(i, p.xz())
                && (-BELOW..=ABOVE).contains(&(p.y - y))
                && best.is_none_or(|(d, _)| (p.y - y).abs() < d)
            {
                best = Some(((p.y - y).abs(), i));
            }
        }
        if let Some((_, i)) = best {
            return Some(i);
        }
        let mut near: Option<(f32, u32)> = None;
        let (lo, hi) = (cell(p.xz() - NEAR), cell(p.xz() + NEAR));
        for x in lo.0..=hi.0 {
            for z in lo.1..=hi.1 {
                for &i in self.grid.get(&(x, z)).into_iter().flatten() {
                    let poly = &self.polys[i as usize];
                    let d = poly.corners.iter().zip(poly.corners.iter().cycle().skip(1)).map(|(&a, &b)| {
                        let q = closest_on_segment(p, a, b);
                        if (-BELOW..=ABOVE).contains(&(p.y - q.y)) { (q - p).xz().length() } else { f32::INFINITY }
                    }).fold(f32::INFINITY, f32::min);
                    if d <= NEAR && near.is_none_or(|(n, _)| d < n) {
                        near = Some((d, i));
                    }
                }
            }
        }
        near.map(|(_, i)| i)
    }

    /// The polygons from `from`'s to `to`'s, by A* over the links (each step costs the distance
    /// between the shared edges' middles).
    fn corridor(&self, start: u32, goal: u32, from: Vec3, to: Vec3) -> Option<Vec<(u32, Option<Link>)>> {
        #[derive(PartialEq)]
        struct Open(f32, u32);
        impl Eq for Open {}
        impl PartialOrd for Open {
            fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
                Some(self.cmp(o))
            }
        }
        impl Ord for Open {
            fn cmp(&self, o: &Self) -> std::cmp::Ordering {
                o.0.total_cmp(&self.0)
            }
        }
        let mut came: HashMap<u32, (u32, Link)> = HashMap::new();
        let mut cost: HashMap<u32, (f32, Vec3)> = HashMap::from([(start, (0.0, from))]);
        let mut open = BinaryHeap::from([Open(from.distance(to), start)]);
        let mut opened = 0;
        while let Some(Open(_, poly)) = open.pop() {
            if poly == goal {
                let mut path = vec![(goal, None)];
                let mut at = goal;
                while let Some(&(prev, link)) = came.get(&at) {
                    path.push((prev, Some(link)));
                    at = prev;
                }
                path.reverse();
                return Some(path);
            }
            opened += 1;
            if opened > SEARCH_LIMIT {
                return None;
            }
            let (g, here) = cost[&poly];
            for link in &self.polys[poly as usize].links {
                let mid = (link.a + link.b) / 2.0;
                let next = g + here.distance(mid);
                if cost.get(&link.to).is_none_or(|&(c, _)| next < c) {
                    cost.insert(link.to, (next, mid));
                    came.insert(link.to, (poly, *link));
                    open.push(Open(next + mid.distance(to), link.to));
                }
            }
        }
        None
    }

    /// A walk from `from` to `to` along the mesh: the corners where it turns, ending at `to`.
    /// `None` when either end is off the mesh or no link joins them.
    pub fn path(&self, from: Vec3, to: Vec3) -> Option<Vec<Vec3>> {
        let (start, goal) = (self.locate(from)?, self.locate(to)?);
        if self.area[start as usize] != self.area[goal as usize] {
            return None;
        }
        if start == goal {
            return Some(vec![to]);
        }
        let corridor = self.corridor(start, goal, from, to)?;
        // Each crossed edge as (left, right) seen walking through it.
        let mut portals = Vec::with_capacity(corridor.len() + 1);
        for &(poly, link) in &corridor {
            let Some(link) = link else { continue };
            let centre = self.centre(poly);
            let (l, r) = if area2(centre.xz(), link.a.xz(), link.b.xz()) > 0.0 { (link.b, link.a) } else { (link.a, link.b) };
            portals.push((l, r));
        }
        portals.push((to, to));
        Some(funnel(from, &portals))
    }

    fn centre(&self, poly: u32) -> Vec3 {
        let c = &self.polys[poly as usize].corners;
        c.iter().copied().sum::<Vec3>() / c.len() as f32
    }

    /// How far a body may walk from `from` straight along `dir` (level) before the mesh ends, at
    /// most `max`; 0 off the mesh. `radius` keeps the body that far short of the end.
    pub fn line_dist(&self, from: Vec3, dir: Vec3, max: f32, radius: f32) -> f32 {
        let Some(mut poly) = self.locate(from) else { return 0.0 };
        let d = dir.xz().normalize_or_zero();
        if d == Vec2::ZERO {
            return max;
        }
        let o = from.xz();
        let mut came: Option<u32> = None;
        for _ in 0..4096 {
            let c = &self.polys[poly as usize].corners;
            // Where the line leaves this polygon: the furthest crossing of its edges' lines.
            let mut exit: Option<(f32, Vec3, Vec3)> = None;
            for k in 0..c.len() {
                let (a, b) = (c[k], c[(k + 1) % c.len()]);
                let e = b.xz() - a.xz();
                let denom = d.perp_dot(e);
                if denom.abs() < 1e-9 {
                    continue;
                }
                let t = (a.xz() - o).perp_dot(e) / denom;
                let s = (a.xz() - o).perp_dot(d) / denom;
                if (-1e-4..=1.0 + 1e-4).contains(&s) && t > -1e-4 && exit.is_none_or(|(x, _, _)| t > x) {
                    exit = Some((t, a, b));
                }
            }
            let Some((t, _, _)) = exit else { return 0.0 };
            if t >= max {
                return max;
            }
            let point = o + d * t;
            let next = self.polys[poly as usize]
                .links
                .iter()
                .filter(|l| Some(l.to) != came && on_segment(point, l.a.xz(), l.b.xz()))
                .map(|l| l.to)
                .next();
            match next {
                Some(n) => {
                    came = Some(poly);
                    poly = n;
                }
                None => return (t - radius).clamp(0.0, max),
            }
        }
        max
    }
}

fn closest_on_segment(p: Vec3, a: Vec3, b: Vec3) -> Vec3 {
    let ab = b - a;
    let t = if ab.length_squared() > 0.0 { ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0) } else { 0.0 };
    a + ab * t
}

fn on_segment(p: Vec2, a: Vec2, b: Vec2) -> bool {
    let ab = b - a;
    let t = (p - a).dot(ab) / ab.length_squared().max(1e-9);
    (-1e-3..=1.0 + 1e-3).contains(&t) && (a + ab * t.clamp(0.0, 1.0)).distance(p) < 0.05
}

/// The shortest line from `start` through `portals` (each crossed edge's left and right ends;
/// the last is the goal twice): the string pulled tight, in XZ, keeping the portals' heights.
fn funnel(start: Vec3, portals: &[(Vec3, Vec3)]) -> Vec<Vec3> {
    // Mikko Mononen's "simple stupid funnel": `tri` > 0 when c is clockwise of a -> b, left the
    // counter-clockwise side of the walk.
    let tri = |a: Vec3, b: Vec3, c: Vec3| -area2(a.xz(), b.xz(), c.xz());
    let mut points = Vec::new();
    let (mut apex, mut left, mut right) = (start, start, start);
    let (mut left_i, mut right_i) = (0usize, 0usize);
    let mut i = 0;
    while i < portals.len() {
        let (l, r) = portals[i];
        if tri(apex, right, r) <= 0.0 {
            if apex == right || tri(apex, left, r) > 0.0 {
                right = r;
                right_i = i;
            } else {
                // The right crossed the left: the walk turns at the left.
                apex = left;
                points.push(apex);
                i = left_i + 1;
                (left, right) = (apex, apex);
                (left_i, right_i) = (left_i, left_i);
                continue;
            }
        }
        if tri(apex, left, l) >= 0.0 {
            if apex == left || tri(apex, right, l) < 0.0 {
                left = l;
                left_i = i;
            } else {
                apex = right;
                points.push(apex);
                i = right_i + 1;
                (left, right) = (apex, apex);
                (left_i, right_i) = (right_i, right_i);
                continue;
            }
        }
        i += 1;
    }
    let goal = portals.last().map_or(start, |p| p.0);
    if points.last() != Some(&goal) {
        points.push(goal);
    }
    points
}

static MESH: RwLock<Option<Arc<NavMesh>>> = RwLock::new(None);

/// Installs the play world's navmesh (the host's map); `None` takes it away.
pub fn install(mesh: Option<NavMesh>) {
    *MESH.write().unwrap() = mesh.map(Arc::new);
}

/// The installed navmesh, if the host brought one.
pub fn current() -> Option<Arc<NavMesh>> {
    #[cfg(test)]
    if let Some(mesh) = HERE.with(|m| m.borrow().clone()) {
        return Some(mesh);
    }
    MESH.read().unwrap().clone()
}

#[cfg(test)]
thread_local! {
    static HERE: std::cell::RefCell<Option<Arc<NavMesh>>> = const { std::cell::RefCell::new(None) };
}

/// A navmesh for this test thread only, so tests that run alongside never see it.
#[cfg(test)]
pub(crate) fn install_here(mesh: Option<NavMesh>) {
    HERE.with(|m| *m.borrow_mut() = mesh.map(Arc::new));
}

/// A strip of unit squares along +x at y 0, plus a leg going +z from the last one (an L, its
/// inside corner at (5, 0, 1)), as two pieces split after `split` squares, and the connector
/// across the seam (both ways).
#[cfg(test)]
pub(crate) fn ell(split: usize) -> (Vec<Piece>, Vec<Join>) {
    let square = |x: f32, z: f32| [Vec3::new(x, 0.0, z), Vec3::new(x + 1.0, 0.0, z), Vec3::new(x + 1.0, 0.0, z + 1.0), Vec3::new(x, 0.0, z + 1.0)];
    let cells: Vec<(f32, f32)> = (0..6).map(|x| (x as f32, 0.0)).chain((1..6).map(|z| (5.0, z as f32))).collect();
    let piece = |cells: &[(f32, f32)]| {
        let mut vertices: Vec<Vec3> = Vec::new();
        let mut faces = Vec::new();
        for &(x, z) in cells {
            let face = square(x, z)
                .iter()
                .map(|v| match vertices.iter().position(|w| w == v) {
                    Some(i) => i as u32,
                    None => {
                        vertices.push(*v);
                        vertices.len() as u32 - 1
                    }
                })
                .collect();
            faces.push(face);
        }
        let opposite = faces
            .iter()
            .enumerate()
            .map(|(f, face): (usize, &Vec<u32>)| {
                (0..4)
                    .map(|k| {
                        let (i, j) = (face[k], face[(k + 1) % 4]);
                        faces.iter().position(|g| g.contains(&i) && g.contains(&j) && g != face).filter(|&g| g != f).map(|g| g as u32)
                    })
                    .collect()
            })
            .collect();
        Piece { vertices, faces, opposite }
    };
    let (x, z) = cells[split];
    let (a, b) = (Vec3::new(x, 0.0, z), Vec3::new(x, 0.0, z + 1.0));
    let joins = vec![Join { from: (0, split as u32 - 1), to: (1, 0), a, b }, Join { from: (1, 0), to: (0, split as u32 - 1), a, b }];
    (vec![piece(&cells[..split]), piece(&cells[split..])], joins)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_turns_the_corner_once_across_pieces() {
        let (pieces, joins) = ell(4);
        let mesh = NavMesh::build(&pieces, &joins);
        assert_eq!(mesh.polygon_count(), 11);
        let path = mesh.path(Vec3::new(0.5, 0.0, 0.5), Vec3::new(5.5, 0.0, 5.5)).expect("the pieces join");
        // Straight to the inside corner of the L, then straight to the goal.
        assert_eq!(path.len(), 2, "{path:?}");
        assert!(path[0].distance(Vec3::new(5.0, 0.0, 1.0)) < 1e-4, "{path:?}");
        assert_eq!(path[1], Vec3::new(5.5, 0.0, 5.5));
    }

    #[test]
    fn the_line_ends_where_the_mesh_does() {
        let (pieces, joins) = ell(3);
        let mesh = NavMesh::build(&pieces, &joins);
        // Along the strip: 5.5 m to its far end, through the piece seam.
        let along = mesh.line_dist(Vec3::new(0.5, 0.0, 0.5), Vec3::X, 20.0, 0.0);
        assert!((along - 5.5).abs() < 1e-3, "{along}");
        assert_eq!(mesh.line_dist(Vec3::new(0.5, 0.0, 0.5), Vec3::X, 3.0, 0.0), 3.0);
        // Off the side: half a metre.
        assert!((mesh.line_dist(Vec3::new(0.5, 0.0, 0.5), Vec3::NEG_Z, 9.0, 0.0) - 0.5).abs() < 1e-3);
        // Pieces without a connector are islands.
        let mesh = NavMesh::build(&pieces, &[]);
        assert!(mesh.path(Vec3::new(0.5, 0.0, 0.5), Vec3::new(5.5, 0.0, 5.5)).is_none());
        assert_eq!(mesh.line_dist(Vec3::new(0.5, 0.0, 0.5), Vec3::X, 20.0, 0.0), 2.5);
        assert!(mesh.locate(Vec3::new(0.5, 20.0, 0.5)).is_none(), "far above the mesh");
    }
}
