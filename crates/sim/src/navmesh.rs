//! A map's navigation mesh, and how it keeps a map foe's walk on it (#227).
//!
//! Each file of `map/<map>/<map>.nvmhktbnd` is an `hkaiNavMesh` in the map's own space; the
//! MSB's map offset event (the transform every collision part of the map also carries) places
//! it in the world, mirrored into Bevy space like the rest of the layout. The faces stop short of
//! the drops the AI is not meant to take: at the Outskirts start the mesh ends about 0.5 m
//! before the ravine's edge.
//!
//! An NPC's walk and run states ask for no fall-prevention assist in `c9997.hks` (only attacks,
//! steps, damage, throws and other actions call `FallPreventionAssist`), so the `f` bands that
//! hold those actions ([`crate::hit_filter::FALL_ASSIST_ACTION`]) do not hold a walk. What keeps
//! a walking foe from following Wolf off a cliff is the AI's navigation: the engine moves it
//! along the navmesh, and a foe whose target is off the mesh walks in place at its edge.
//! [`NavGround`] models that: a grounded foe on the mesh that walks is cut where its move would
//! leave the mesh. The exe's own boundary rule (path follow or a clamp of the move, its height
//! tolerance) is not traced; `docs/FIDELITY.md` lists the approximation.

use std::collections::HashMap;
use std::path::Path;

use glam::{Quat, Vec2, Vec3};

use crate::body::Ground;

/// Grid cell size over the horizontal plane, in metres.
const CELL: f32 = 4.0;
/// How far above or below the feet a face may be and still carry them: the faces sit within a
/// few tenths of a metre of the floor. Not traced.
const REACH: f32 = 1.0;
/// Sub-step of the cut, in metres.
const CUT_STEP: f32 = 0.05;
/// How wide a gap in the mesh a walk may cross, in metres: the mesh leaves holes round rocks,
/// posts and bushes, which the engine's path goes round. Not traced.
const HOLE: f32 = 2.0;

/// One face: its corners in Bevy space and its horizontal box.
#[derive(Debug)]
struct Face {
    corners: Vec<Vec3>,
    min: Vec2,
    max: Vec2,
}

/// A map's navigation mesh in Bevy space.
#[derive(Debug, Default)]
pub struct NavMesh {
    faces: Vec<Face>,
    /// Face indices by [`CELL`] square their box touches.
    grid: HashMap<(i32, i32), Vec<u32>>,
}

fn cell(x: f32, z: f32) -> (i32, i32) {
    ((x / CELL).floor() as i32, (z / CELL).floor() as i32)
}

impl NavMesh {
    /// Builds the mesh from navmesh pieces in the map's own (FromSoftware) space, placed by the
    /// map offset's `position` and Y rotation in degrees.
    pub fn from_pieces<'a>(
        pieces: impl IntoIterator<Item = &'a sekiro_formats::hkx::navmesh::NavMesh>,
        position: [f32; 3],
        rotation_y: f32,
    ) -> Self {
        // FromSoftware's left-handed space mirrored across X, as the layout does: the offset's
        // translation mirrored and its yaw negated.
        let translation = Vec3::new(-position[0], position[1], position[2]);
        let rotation = Quat::from_rotation_y(-rotation_y.to_radians());
        let mut mesh = Self::default();
        for piece in pieces {
            let vertices: Vec<Vec3> = piece
                .vertices
                .iter()
                .map(|v| translation + rotation * Vec3::new(-v[0], v[1], v[2]))
                .collect();
            for face in &piece.faces {
                let corners: Vec<Vec3> = face.iter().map(|&i| vertices[i as usize]).collect();
                if corners.len() < 3 {
                    continue;
                }
                let flat = corners.iter().map(|c| Vec2::new(c.x, c.z));
                let min = flat.clone().fold(Vec2::splat(f32::INFINITY), Vec2::min);
                let max = flat.fold(Vec2::splat(f32::NEG_INFINITY), Vec2::max);
                let index = mesh.faces.len() as u32;
                let (lo, hi) = (cell(min.x, min.y), cell(max.x, max.y));
                for cx in lo.0..=hi.0 {
                    for cz in lo.1..=hi.1 {
                        mesh.grid.entry((cx, cz)).or_default().push(index);
                    }
                }
                mesh.faces.push(Face { corners, min, max });
            }
        }
        mesh
    }

    /// The navmesh of map `map` from `cache/raw/map/<map>/<map>.nvmhktbnd.d`, placed by the
    /// MSB's map offset (`None`: none, the map's own space is the world's). `Ok(None)` when the
    /// cache has no navmesh for the map.
    pub fn load_map(
        cache: &Path,
        map: &str,
        offset: Option<&sekiro_formats::msb::MapOffset>,
    ) -> Result<Option<Self>, String> {
        let dir = cache.join(format!("raw/map/{map}/{map}.nvmhktbnd.d"));
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(format!("{}: {e}", dir.display())),
        };
        let mut paths: Vec<_> = entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("hkx")))
            .collect();
        paths.sort();
        let pieces = paths
            .iter()
            .map(|p| {
                let bytes = std::fs::read(p).map_err(|e| format!("{}: {e}", p.display()))?;
                sekiro_formats::hkx::navmesh::NavMesh::from_bytes(&bytes)
                    .map_err(|e| format!("{}: {e}", p.display()))
            })
            .collect::<Result<Vec<_>, String>>()?;
        let (position, rotation_y) = offset.map_or(([0.0; 3], 0.0), |o| (o.position, o.rotation_y));
        Ok(Some(Self::from_pieces(&pieces, position, rotation_y)))
    }

    /// Number of faces.
    pub fn face_count(&self) -> usize {
        self.faces.len()
    }

    /// The face over or under `(x, z)` nearest `y`, within [`REACH`] of it, and its height
    /// there.
    fn face_near(&self, x: f32, z: f32, y: f32) -> Option<(&Face, f32)> {
        let p = Vec2::new(x, z);
        self.grid
            .get(&cell(x, z))?
            .iter()
            .map(|&i| &self.faces[i as usize])
            .filter(|f| p.cmpge(f.min).all() && p.cmple(f.max).all())
            .filter_map(|f| Some((f, face_height(&f.corners, p)?)))
            .filter(|(_, h)| (h - y).abs() <= REACH)
            .min_by(|a, b| (a.1 - y).abs().total_cmp(&(b.1 - y).abs()))
    }

    /// The height of the face over or under `(x, z)` nearest `y`, within [`REACH`] of it.
    pub fn floor_near(&self, x: f32, z: f32, y: f32) -> Option<f32> {
        self.face_near(x, z, y).map(|(_, h)| h)
    }

    /// Whether feet at `feet` stand on the mesh.
    pub fn covers(&self, feet: [f32; 3]) -> bool {
        self.face_near(feet[0], feet[2], feet[1]).is_some()
    }

    /// `delta` kept on the mesh for feet at `feet`, as a body slides along a wall: the whole
    /// move when the feet are off the mesh already (a foe knocked or spawned off it walks back
    /// freely), it ends on the mesh or it only crosses a hole in it (the mesh carries on within
    /// [`HOLE`] past its end); else the part up to the mesh's edge, then the rest
    /// along that edge, again only as far as the mesh goes. So a foe walking at a small hole
    /// (the mesh leaves out rocks and posts) slides round it, and one walking at a drop stops
    /// at the edge, sliding along it only as far as its move runs along it.
    pub fn hold(&self, feet: [f32; 3], delta: [f32; 2]) -> [f32; 2] {
        if !self.covers(feet) {
            return delta;
        }
        let (kept, edge) = self.cut(feet, delta);
        let Some(edge) = edge else {
            return kept;
        };
        if self.mesh_ahead(feet, delta) {
            return delta;
        }
        let rest = Vec2::from(delta) - Vec2::from(kept);
        let along = edge * rest.dot(edge);
        let from = [feet[0] + kept[0], feet[1], feet[2] + kept[1]];
        let (slid, _) = self.cut(from, along.to_array());
        [kept[0] + slid[0], kept[1] + slid[1]]
    }

    /// Whether the mesh carries on within [`HOLE`] past the end of `delta` from `feet`, along it:
    /// the move only crosses a hole the mesh leaves for a rock, post or bush, which the foe's
    /// path would take it round, not a drop.
    fn mesh_ahead(&self, feet: [f32; 3], delta: [f32; 2]) -> bool {
        let d = Vec2::from(delta);
        let Some(dir) = d.try_normalize() else {
            return false;
        };
        let end = Vec2::new(feet[0], feet[2]) + d;
        let steps = (HOLE / CUT_STEP).ceil() as usize;
        (1..=steps).any(|i| {
            let p = end + dir * (i as f32 * CUT_STEP);
            self.face_near(p.x, p.y, feet[1]).is_some()
        })
    }

    /// The part of `delta` up to the last [`CUT_STEP`] that keeps feet at `feet` (on the mesh)
    /// on it, and when it is cut short, the direction of the face edge nearest the first point
    /// off the mesh (unit length, seen from above).
    fn cut(&self, feet: [f32; 3], delta: [f32; 2]) -> ([f32; 2], Option<Vec2>) {
        let at = |t: f32| Vec2::new(feet[0] + delta[0] * t, feet[2] + delta[1] * t);
        let face = |t: f32| {
            let p = at(t);
            self.face_near(p.x, p.y, feet[1])
        };
        if face(1.0).is_some() {
            return (delta, None);
        }
        let steps = (delta[0].hypot(delta[1]) / CUT_STEP).ceil().max(1.0) as usize;
        let mut kept = 0.0;
        let mut last = face(0.0);
        for i in 1..steps {
            let t = i as f32 / steps as f32;
            match face(t) {
                Some(f) => (kept, last) = (t, Some(f)),
                None => break,
            }
        }
        let off = at(kept + 1.0 / steps as f32);
        let edge = last.and_then(|(f, _)| nearest_edge(&f.corners, off));
        ([delta[0] * kept, delta[1] * kept], edge)
    }
}

/// The direction (unit length, seen from above) of the edge of polygon `corners` nearest `p`.
fn nearest_edge(corners: &[Vec3], p: Vec2) -> Option<Vec2> {
    let flat: Vec<Vec2> = corners.iter().map(|c| Vec2::new(c.x, c.z)).collect();
    (0..flat.len())
        .map(|i| (flat[i], flat[(i + 1) % flat.len()]))
        .filter(|(a, b)| a.distance_squared(*b) > 1e-8)
        .min_by(|x, y| segment_distance(p, *x).total_cmp(&segment_distance(p, *y)))
        .map(|(a, b)| (b - a).normalize())
}

/// Squared distance from `p` to the segment `a`-`b`.
fn segment_distance(p: Vec2, (a, b): (Vec2, Vec2)) -> f32 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0);
    p.distance_squared(a + ab * t)
}

/// The height of the convex polygon `corners` at `p`, if `p` is inside it (seen from above).
fn face_height(corners: &[Vec3], p: Vec2) -> Option<f32> {
    let a = corners[0];
    corners[1..].windows(2).find_map(|w| {
        let (b, c) = (w[0], w[1]);
        let d = (b.z - c.z) * (a.x - c.x) + (c.x - b.x) * (a.z - c.z);
        if d.abs() < 1e-9 {
            return None;
        }
        let l1 = ((b.z - c.z) * (p.x - c.x) + (c.x - b.x) * (p.y - c.z)) / d;
        let l2 = ((c.z - a.z) * (p.x - c.x) + (a.x - c.x) * (p.y - c.z)) / d;
        let l3 = 1.0 - l1 - l2;
        const EPS: f32 = -1e-4;
        (l1 >= EPS && l2 >= EPS && l3 >= EPS).then_some(l1 * a.y + l2 * b.y + l3 * c.y)
    })
}

/// A foe's ground while it walks on the navmesh: `terrain`'s slides, cut by [`NavMesh::hold`].
pub struct NavGround<'a> {
    pub terrain: &'a dyn Ground,
    pub nav: &'a NavMesh,
}

impl Ground for NavGround<'_> {
    fn inner(&self) -> Option<&dyn Ground> {
        Some(self.terrain)
    }
    fn height(&self, x: f32, z: f32) -> Option<f32> {
        self.terrain.height(x, z)
    }
    fn slide(&self, feet: [f32; 3], delta: [f32; 2]) -> [f32; 2] {
        self.nav.hold(feet, self.terrain.slide(feet, delta))
    }
    fn slide_assisted(&self, feet: [f32; 3], delta: [f32; 2], walls: u8) -> [f32; 2] {
        self.nav
            .hold(feet, self.terrain.slide_assisted(feet, delta, walls))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 2 m square at height 1, placed by an offset of (10, 0, 0) turned 90 degrees.
    fn square() -> NavMesh {
        let piece = sekiro_formats::hkx::navmesh::NavMesh {
            vertices: vec![
                [0.0, 1.0, 0.0],
                [2.0, 1.0, 0.0],
                [2.0, 1.0, 2.0],
                [0.0, 1.0, 2.0],
            ],
            faces: vec![vec![0, 1, 2, 3]],
        };
        NavMesh::from_pieces([&piece], [10.0, 0.0, 0.0], 90.0)
    }

    #[test]
    fn placed_like_the_layout_and_holds_a_walk_at_the_edge() {
        let mesh = square();
        // The map-space corner (1, 1, 1) turned 90 degrees about Y (FromSoftware), shifted by
        // (10, 0, 0) and mirrored: (-(10 + 1), 1, -1).
        assert!(
            mesh.floor_near(-11.0, -1.0, 1.2)
                .is_some_and(|h| (h - 1.0).abs() < 1e-5)
        );
        assert_eq!(mesh.floor_near(-11.0, -1.0, 2.5), None, "another storey");
        assert!(!mesh.covers([-11.0, 1.0, 1.0]));
        // A move off the edge stops within a sub-step of it; one off the mesh is free.
        let held = mesh.hold([-11.0, 1.0, -1.0], [0.0, 3.0]);
        assert!((0.9..=1.01).contains(&held[1]), "{held:?}");
        // A move at the edge at a slant keeps its part along the edge.
        let slid = mesh.hold([-11.0, 1.0, -1.0], [0.5, 3.0]);
        assert!((slid[0] - 0.5).abs() < 0.01, "{slid:?}");
        assert!((0.9..=1.01).contains(&slid[1]), "{slid:?}");
        assert_eq!(mesh.hold([-11.0, 1.0, -1.0], [0.0, 0.5]), [0.0, 0.5]);
        assert_eq!(mesh.hold([-11.0, 1.0, 5.0], [0.0, 3.0]), [0.0, 3.0]);
    }

    /// Two 2 m squares at height 0 along Z, `gap` metres apart, unplaced.
    fn two_squares(gap: f32) -> NavMesh {
        let square = |z: f32| sekiro_formats::hkx::navmesh::NavMesh {
            vertices: vec![
                [0.0, 0.0, z],
                [2.0, 0.0, z],
                [2.0, 0.0, z + 2.0],
                [0.0, 0.0, z + 2.0],
            ],
            faces: vec![vec![0, 1, 2, 3]],
        };
        NavMesh::from_pieces([&square(0.0), &square(2.0 + gap)], [0.0; 3], 0.0)
    }

    #[test]
    fn a_walk_crosses_a_small_hole_but_not_a_wide_gap() {
        let feet = [-1.0, 0.0, 1.9];
        assert_eq!(two_squares(1.0).hold(feet, [0.0, 0.3]), [0.0, 0.3]);
        let held = two_squares(HOLE + 0.5).hold(feet, [0.0, 0.3]);
        assert!(held[1] <= 0.11, "{held:?}");
    }
}
