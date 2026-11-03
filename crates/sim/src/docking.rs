//! Collision-backed action-edge selection and body docking.
use crate::body::{Body, Ground, root_motion_world};
use crate::input::angle_diff;
use glam::Vec3;
use serde::Deserialize;
use std::path::Path;

// Placeholder, not traced: the exe owns these search and placement tunables.
pub const GROUND_REACH: f32 = 1.0;
pub const TOP_TOLERANCE: f32 = 0.4;
pub const HAND_HEIGHT: f32 = 1.5;
pub const FACING_LIMIT: f32 = 45.0;
/// Where the hanging root sits under and out from the lip, read from the hang idle clips:
/// HangIdle (a000_020000) and HangFootHoldIdle (a000_020010) both hold the hands 1.55 m over
/// the root and 0.635 m in front of it, so with the hands on the lip the root is 1.55 m under
/// it and 0.635 m out. The pelvis is then 0.55 m (plain) or 0.18 m (foot hold) in front of the
/// root, so it stays off the face.
pub const HANG_DEPTH: f32 = 1.55;
pub const HANG_OUTWARD: f32 = 0.635;
/// Where the climb ends, in from the lip: HangToStand / HangFootHoldToStand (a000_217500 /
/// 217600) carry the root 1.10 m forward and 1.60 m up from the hang, 1.10 - 0.635 m past it.
pub const CLIMB_INWARD: f32 = 0.465;
/// RustSekiro's stop distance from either end of a hang ledge (its edge chain).
pub const HANG_END_MARGIN: f32 = 0.35;
/// How close two hang edges' ends must be to join them into one ledge: the extractor writes
/// a chain's shared corners as the same vertex, so this only absorbs float noise.
const CHAIN_JOIN: f32 = 0.01;
pub const FLOOR_TOLERANCE: f32 = 0.3;
pub const END_MARGIN: f32 = 0.2;
pub const OUTWARD_PROBE: f32 = 0.6;
pub const MIN_DROP: f32 = 0.5;
pub const SIDE_TOLERANCE: f32 = 0.1;
pub const CLIMB_FALLBACK_DURATION: f32 = 0.833;
// DockingTranslationOffset_Cover is (0, 0, 0.7) in c0000.hkx's behavior graph variable initial
// values (FwLo's MIT docking notes): the docked root stands 0.7 m out from the wall. Only its
// sign in the edge's frame (out from the wall, away from the face) is assumed.
pub const DOCKING_TRANSLATION_OFFSET_COVER: f32 = 0.7;
pub const COVER_HEIGHT_TOLERANCE: f32 = 0.8;
pub const COVER_WALL_PROBE: f32 = 1.0;
pub const COVER_PROBE_LIFT: f32 = 0.6;
pub const COVER_BLOCK_TOLERANCE: f32 = 0.05;
/// Placeholder away-stick cover exit: half-width in degrees of the stick cone around straight
/// away from the wall, which is 180 degrees in the docking frame ([`Target::right`]).
pub const COVER_AWAY_ANGLE: f32 = 30.0;

/// Whether a `DockingStickAngle` (degrees, docking frame) points away from the wall within
/// [`COVER_AWAY_ANGLE`].
pub fn cover_stick_away(docking_stick_angle: f32) -> bool {
    docking_stick_angle.abs() > 180.0 - COVER_AWAY_ANGLE
}

/// Air hang (env 3045): how far out from the face a fall or jump catches the lip, short range
/// (type 1) and long range (type 2), and the band of lip heights over the feet. Not traced (the
/// exe's search); Fireguy32's RustSekiro values.
pub const AIR_HANG_SHORT: f32 = 1.0;
pub const AIR_HANG_LONG: f32 = 2.0;
pub const AIR_HANG_BAND: (f32, f32) = (1.0, 2.6);
/// c0000_define `AIR_HANG_TYPE_SHORT_LANGE` and `AIR_HANG_TYPE_LONG_LANGE`.
pub const AIR_HANG_TYPE_SHORT: u8 = 1;
pub const AIR_HANG_TYPE_LONG: u8 = 2;
/// The climb-up and drop clips: `HangToStand` / `HangFootHoldToStand` and `HangToFreeFall` /
/// `HangFootHoldToFreeFall` (variation 0 / 1).
pub(crate) const CLIMB_CLIPS: [&str; 2] = ["a000_217500", "a000_217600"];
const DROP_CLIPS: [&str; 2] = ["a000_217520", "a000_217620"];

/// Hang edges: kind 2, variation 0 (`HANG_EDGE_TYPE_DISABLE_HOLD_FOOT`) or 1 (`ENABLE_HOLD_FOOT`).
fn is_hang(edge: &MapEdge) -> bool {
    edge.kind == 2 && edge.variation <= 1
}

/// Cover edges Wolf docks to: kind 3, variations 0 to 3 and 10 to 13.
fn is_cover_edge(edge: &MapEdge) -> bool {
    edge.kind == 3 && matches!(edge.variation, 0..=3 | 10..=13)
}

/// Held hang poses: idle, shimmy start, loop and stop (plain and foothold sets).
pub(crate) fn is_hang_clip(name: &str) -> bool {
    name.starts_with("a000_020")
}

pub(crate) fn is_cover_clip(name: &str) -> bool {
    name.starts_with("a000_021")
        || name.starts_with("a000_022")
        || matches!(
            name,
            "a000_205010"
                | "a000_205011"
                | "a000_205020"
                | "a000_205021"
                | "a000_205030"
                | "a000_205031"
        )
}

/// Endpoint bits in edge order (1 at the start, 2 at the end) mapped into the docking frame
/// ([`Target::right`]): 1 left, 2 right. The script reads env 3011 as
/// `DOCKING_TGT_END_TYPE_LEFT` / `_RIGHT` for hang and cover alike, and refuses a shimmy toward
/// that side, so edge order would block the wrong way on edges that run right to left.
fn side_end(bits: u8, edge: &MapEdge, outward: Vec3) -> u8 {
    let d = Vec3::from(edge.end) - Vec3::from(edge.start);
    if d.dot(docking_right(outward)) > 0.0 {
        bits
    } else {
        ((bits & 1) << 1) | (bits >> 1)
    }
}

/// Where the climb from a hang at `point` ends, before the floor is probed.
fn climb_top(point: Vec3, outward: Vec3) -> Vec3 {
    point - outward * CLIMB_INWARD
}

/// Check one known face, without searching the edge again or trying its opposite side.
fn hang_floor(point: Vec3, outward: Vec3, ground: &dyn Ground) -> Option<Vec3> {
    let top = climb_top(point, outward);
    let y = ground.floor(top.x, top.z, point.y + FLOOR_TOLERANCE)?;
    if (y - point.y).abs() > FLOOR_TOLERANCE {
        return None;
    }
    let outside = point + outward * OUTWARD_PROBE;
    if ground
        .floor(outside.x, outside.z, point.y + FLOOR_TOLERANCE)
        .is_some_and(|h| h >= point.y - MIN_DROP)
    {
        return None;
    }
    Some(Vec3::new(top.x, y, top.z))
}

#[derive(Debug, Clone, Deserialize)]
pub struct MapEdge {
    pub kind: u8,
    pub variation: u8,
    pub start: [f32; 3],
    pub end: [f32; 3],
    pub source_part: String,
    pub chain: u32,
}

impl MapEdge {
    /// The edge's run, start to end, flattened to the ground plane.
    fn horizontal(&self) -> Vec3 {
        let d = Vec3::from(self.end) - Vec3::from(self.start);
        Vec3::new(d.x, 0.0, d.z)
    }

    /// The point a fraction `t` of the way from start to end.
    fn at(&self, t: f32) -> Vec3 {
        Vec3::from(self.start).lerp(Vec3::from(self.end), t)
    }

    /// The unclamped fraction along the edge of `position`'s horizontal projection: 0 at the
    /// start, 1 at the end, and 0 on an edge with no horizontal run.
    fn along(&self, position: Vec3) -> f32 {
        let horizontal = self.horizontal();
        let length_squared = horizontal.length_squared();
        if length_squared > 1e-8 {
            (position - Vec3::from(self.start)).dot(horizontal) / length_squared
        } else {
            0.0
        }
    }

    /// The unit horizontal normal to the left of the run (start to end, seen from above):
    /// one of the edge's two faces. `None` on an edge with no horizontal run.
    fn side(&self) -> Option<Vec3> {
        let d = self.horizontal();
        Vec3::new(-d.z, 0.0, d.x).try_normalize()
    }

    pub fn closest(&self, position: Vec3) -> Vec3 {
        self.at(self.along(position).clamp(0.0, 1.0))
    }
}

/// How far short of a hang stop still counts as at the ledge's end ([`span`]).
const END_EPSILON: f32 = 0.01;

/// The docking point nearest `position` along the ledge or wall through edge `index`. Past a
/// shared corner it moves once onto the chain's next edge; it stops in from the chain's real
/// ends only, by `stop` or less on a short edge, so a short lone edge holds at its midpoint.
/// Returns the edge, the point on it and the end bits in edge order (1 at the start, 2 at the
/// end, 3 both on a short lone edge), set within `zone` of the stop. Hangs stop
/// [`HANG_END_MARGIN`] in, with [`END_EPSILON`] for the hang stop clips' root drift back off
/// the stop (2 to 7 mm in the foot-hold set), which would otherwise clear the bit and restart
/// the shimmy; cover runs to the end itself and marks the last [`END_MARGIN`].
fn span(
    edges: &[MapEdge],
    links: &[[Option<usize>; 2]],
    index: usize,
    position: Vec3,
    stop: f32,
    zone: f32,
) -> (usize, Vec3, u8) {
    let link = |i: usize| links.get(i).copied().unwrap_or_default();
    let mut index = index;
    let mut s = edges[index].along(position);
    let onward = if s < 0.0 {
        link(index)[0]
    } else if s > 1.0 {
        link(index)[1]
    } else {
        None
    };
    if let Some(j) = onward {
        index = j;
        s = edges[index].along(position);
    }
    let [prev, next] = link(index);
    let (at_start, at_end) = (prev.is_none(), next.is_none());
    let length = edges[index].horizontal().length();
    let ends = u8::from(at_start) + u8::from(at_end);
    let margin = stop.min(length / f32::from(ends.max(1)));
    let low = if at_start { margin } else { 0.0 };
    let high = if at_end { length - margin } else { length };
    let distance = (s * length).clamp(low, high.max(low));
    let t = if length > 0.0 { distance / length } else { 0.5 };
    let bits = u8::from(at_start && distance <= margin + zone)
        | (u8::from(at_end && length - distance <= margin + zone) << 1);
    (index, edges[index].at(t), bits)
}

/// [`span`] for a hang: stops [`HANG_END_MARGIN`] in from the ledge's ends.
fn hang_span(
    edges: &[MapEdge],
    links: &[[Option<usize>; 2]],
    index: usize,
    position: Vec3,
) -> (usize, Vec3, u8) {
    span(edges, links, index, position, HANG_END_MARGIN, END_EPSILON)
}

/// The outward normal on chain edge `to` after moving on from `from`: on the same side of the
/// ledge. Chain neighbours run the same way ([`MAX_JOINT_TURN`]), so that is the same side of
/// each edge's run.
fn chain_outward(edges: &[MapEdge], from: usize, to: usize, outward: Vec3) -> Vec3 {
    let sign = match edges[from].side() {
        Some(side) if side.dot(outward) < 0.0 => -1.0,
        _ => 1.0,
    };
    edges[to].side().map_or(outward, |side| side * sign)
}

/// The sharpest turn, in degrees, between two edges that still join into one ledge or wall. The
/// joints in m11_00's hang chains split cleanly into under 30 and 75 to 105 degrees, and the
/// real game has dedicated hang corner moves for the sharp ones (env 3048/3049 outer,
/// 3059/3060 inner; docs/MOBILITY.md), so a sharp joint is a ledge end, not a slide round.
const MAX_JOINT_TURN: f32 = 30.0;

/// Which edges can join into one ledge or wall: hang edges of the same part, chain and
/// variation (the variation picks the hang clip set), or dockable cover edges of the same part
/// across chains and variations. A cover wall is split into chains where its variation changes,
/// as m11_00's snake pit wall (`h013600`) runs chain 2 (variation 0), 3 (10, the narrow ledge)
/// and 4 (0) end to start on shared vertices, and Wolf hugs it in one run (#147); the docked
/// edge's own variation still sets env 3017. A cover edge Wolf cannot dock to joins nothing, so
/// the wall ends where it starts.
fn join_key(edge: &MapEdge) -> Option<(&str, u8, u32, u8)> {
    let part = edge.source_part.as_str();
    if is_hang(edge) {
        Some((part, edge.kind, edge.chain, edge.variation))
    } else if is_cover_edge(edge) {
        Some((part, edge.kind, 0, 0))
    } else {
        None
    }
}

/// The chain neighbours of every edge: `[prev, next]`, the edge joined at its start and at its
/// end. A join needs both edges to share a [`join_key`], one's end on the other's start (the
/// extractor stores a chain end to start, one way round) and a turn of at most
/// [`MAX_JOINT_TURN`].
fn chain_links(edges: &[MapEdge]) -> Vec<[Option<usize>; 2]> {
    let mut groups: std::collections::HashMap<(&str, u8, u32, u8), Vec<usize>> = Default::default();
    for (i, edge) in edges.iter().enumerate() {
        if let Some(key) = join_key(edge) {
            groups.entry(key).or_default().push(i);
        }
    }
    let min_cos = MAX_JOINT_TURN.to_radians().cos();
    let mut links = vec![[None; 2]; edges.len()];
    for group in groups.values() {
        for &a in group {
            let Some(da) = edges[a].horizontal().try_normalize() else {
                continue;
            };
            let corner = Vec3::from(edges[a].end);
            let next = group.iter().copied().find(|&b| {
                b != a
                    && links[b][0].is_none()
                    && Vec3::from(edges[b].start).distance(corner) <= CHAIN_JOIN
                    && edges[b]
                        .horizontal()
                        .try_normalize()
                        .is_some_and(|db| da.dot(db) >= min_cos)
            });
            if let Some(b) = next {
                links[a][1] = Some(b);
                links[b][0] = Some(a);
            }
        }
    }
    links
}

#[derive(Debug, Clone)]
pub struct Target {
    pub edge: usize,
    pub point: Vec3,
    pub outward: Vec3,
    pub floor: Vec3,
    /// The ledge or wall ends Wolf is at: 1 left, 2 right, in the docking frame
    /// ([`Target::right`]); 3 both on a short hang edge.
    pub end: u8,
    pub angle: f32,
}

/// [`Target::right`] from a bare outward normal; the frame and its evidence are documented there.
fn docking_right(outward: Vec3) -> Vec3 {
    Vec3::new(outward.z, 0.0, -outward.x)
}

impl Target {
    /// Right in the docking frame, which faces the wall ([`Target::yaw`]) for hang and cover
    /// alike: Wolf's own right while hanging, but his own left in cover, where his back is to
    /// the wall. The script's sides (env 3011 end, env 3051/3052 blocked, `DockingStickAngle`)
    /// are all in this frame. The clips show it: CoverMoveL (a000_021402) moves the root to
    /// source -X and HangMoveL (a000_020202) to source +X, so the cover "left" shuffle carries
    /// Wolf to his own right (issue #172).
    pub fn right(&self) -> Vec3 {
        docking_right(self.outward)
    }
    pub fn hang_position(&self) -> Vec3 {
        self.point + self.outward * HANG_OUTWARD - Vec3::Y * HANG_DEPTH
    }
    /// The docking frame's yaw: facing the wall, which is Wolf's facing in a hang.
    pub fn yaw(&self) -> f32 {
        self.outward.x.atan2(self.outward.z)
    }
    /// RustSekiro held_state: a hang blocks only travel beyond the ledge end on that side.
    pub fn end_blocked(&self, left: bool) -> bool {
        self.end & if left { 1 } else { 2 } != 0
    }
}

#[derive(Debug, Default)]
pub struct Docking {
    pub edges: Vec<MapEdge>,
    pub candidate: Option<Target>,
    pub active: Option<Target>,
    /// env 3045 for the airborne candidate: 0 none, [`AIR_HANG_TYPE_SHORT`] or [`AIR_HANG_TYPE_LONG`].
    pub air_hang_type: u8,
    /// Skip the airborne edge scan: the script is not asking for an air hang (act 3028).
    pub skip_air_search: bool,
    climb_start: Option<Vec3>,
    /// Each edge's chain neighbours ([`chain_links`]), built from `edges`; call
    /// [`Docking::rebuild_links`] after changing them.
    links: Vec<[Option<usize>; 2]>,
}

impl Docking {
    pub fn new(edges: Vec<MapEdge>) -> Self {
        let mut docking = Self {
            edges,
            ..Self::default()
        };
        docking.rebuild_links();
        docking
    }

    /// Rejoin the edges into ledges and walls after `edges` was set or changed.
    pub fn rebuild_links(&mut self) {
        self.links = chain_links(&self.edges);
    }

    pub fn is_cover(&self, target: &Target) -> bool {
        self.edges[target.edge].kind == 3
    }

    fn cover_target(&self, index: usize, position: Vec3, ground: &dyn Ground) -> Option<Target> {
        // Joined neighbours are dockable too ([`join_key`]), so wherever [`span`] moves on to is.
        if !is_cover_edge(&self.edges[index]) {
            return None;
        }
        let (index, point, bits) = span(&self.edges, &self.links, index, position, 0.0, END_MARGIN);
        let edge = &self.edges[index];
        // One end at most, the start first: cover never reports both.
        let bits = if bits & 1 != 0 { 1 } else { bits };
        let side = edge.side()?;
        // A floor at the wall's height and Wolf's, as a wall-hugging body stands: on the
        // wall-hug parts too ([`crate::hit_filter::WALL_HUG`]).
        let floor = |p: Vec3| {
            ground
                .hug_floor(p.x, p.z, position.y + TOP_TOLERANCE)
                .filter(|&y| {
                    (point.y - y).abs() <= COVER_HEIGHT_TOLERANCE
                        && (position.y - y).abs() <= TOP_TOLERANCE
                })
        };
        for outward in [side, -side] {
            if (position - point).dot(outward) < 0.0 {
                continue;
            }
            let feet = point + outward * DOCKING_TRANSLATION_OFFSET_COVER;
            // Under the docked feet, always. The snake pit's narrow ledge (variation 10, #147)
            // has no character floor there, only the wall-hug slab (`h900100`, hit filter 25),
            // so the hug floor is the real one and no stand-in at the wall's foot is needed (#189).
            // Once the hug ends the slab holds him no more, as in the exe: he rests on the
            // ledge's lip where it is under his capsule and drops where `l` has no ledge
            // (docs/HIT-FILTERS.md, `snake_pit_hug_exit_rests_on_the_lip`).
            let Some(y) = floor(feet) else {
                continue;
            };
            let clearance = ground.slide([feet.x, y, feet.z], [0.0; 2]);
            if clearance[0].hypot(clearance[1]) > COVER_BLOCK_TOLERANCE {
                continue;
            }
            let delta = -outward * COVER_WALL_PROBE;
            // Reject kerbs and floor contacts: cover needs a wall above the step zone.
            let slid = ground.slide([feet.x, y + COVER_PROBE_LIFT, feet.z], [delta.x, delta.z]);
            if Vec3::new(slid[0] - delta.x, 0.0, slid[1] - delta.z).length() < COVER_BLOCK_TOLERANCE
            {
                continue;
            }
            return Some(Target {
                edge: index,
                point,
                outward,
                floor: Vec3::new(feet.x, y, feet.z),
                end: side_end(bits, edge, outward),
                angle: 0.0,
            });
        }
        None
    }

    /// Recheck only the docked face's floor and drop, not the edge search.
    pub fn hang_can_climb(&self, target: &Target, ground: &dyn Ground) -> bool {
        is_hang(&self.edges[target.edge])
            && hang_floor(target.point, target.outward, ground).is_some()
    }
    pub fn cover_blocked(&self, target: &Target, ground: &dyn Ground, left: bool) -> bool {
        let delta = target.right() * if left { -END_MARGIN } else { END_MARGIN };
        let slid = ground.slide(target.floor.to_array(), [delta.x, delta.z]);
        Vec3::new(slid[0] - delta.x, 0.0, slid[1] - delta.z).length() > COVER_BLOCK_TOLERANCE
    }

    /// Cover displacement uses the authored movement clip's root distance.
    pub fn cover_step(
        &mut self,
        body: &mut Body,
        animation: Option<&str>,
        stick_yaw: Option<f32>,
        root_distance: f32,
        ground: &dyn Ground,
    ) -> bool {
        let name = animation.unwrap_or("");
        let entry = name.starts_with("a000_21810") || matches!(name, "a000_218111" | "a000_218112");
        if self.active.is_none() && entry {
            self.active = self.candidate.clone().filter(|t| self.is_cover(t));
        }
        let Some(target) = self.active.as_ref().filter(|t| self.is_cover(t)) else {
            return false;
        };
        if matches!(name, "a000_218490" | "a000_218491") || !(entry || is_cover_clip(name)) {
            self.active = None;
            body.grounded = true;
            body.velocity = [0.0; 3];
            return false;
        }
        let mut target = target.clone();
        // Only locomotion clips slide. Look-out stays anchored at the endpoint.
        let id = name
            .strip_prefix("a000_")
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(0);
        if (21100..21700).contains(&id)
            && let Some(yaw) = stick_yaw
            // HKS DOCKING_MOVE_INPUT_ANGLE_MIN/MAX: only the along-wall cone moves.
            && {
                let angle = angle_diff(yaw, target.yaw() + std::f32::consts::PI).abs().to_degrees();
                angle > 30.0 && angle < 150.0
            }
        {
            let edge = &self.edges[target.edge];
            let tangent = (Vec3::from(edge.end) - Vec3::from(edge.start)).normalize();
            let stick = Vec3::new(-yaw.sin(), 0.0, -yaw.cos());
            // Past the edge's end [`span`] carries the step onto the wall's next edge.
            let next = target.point + tangent * root_distance * stick.dot(tangent).signum();
            if let Some(updated) = self.cover_target(
                target.edge,
                Vec3::new(next.x, target.floor.y, next.z)
                    + target.outward * DOCKING_TRANSLATION_OFFSET_COVER,
                ground,
            ) {
                target = updated;
            }
        }
        body.position = target.floor.to_array();
        body.yaw = angle_diff(target.yaw() + std::f32::consts::PI, 0.0);
        body.velocity = [0.0; 3];
        body.grounded = true;
        body.last_fall_height = 0.0;
        self.active = Some(target);
        true
    }
    pub fn load(path: &Path) -> Result<Self, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let edges = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        Ok(Self::new(edges))
    }

    /// Infer the outward side from an upper floor on one side and a drop on the other.
    pub fn target(&self, index: usize, position: Vec3, ground: &dyn Ground) -> Option<Target> {
        let edge = self.edges.get(index)?;
        if !is_hang(edge) {
            return None;
        }
        let (index, point, end) = hang_span(&self.edges, &self.links, index, position);
        let edge = &self.edges[index];
        let side = edge.side()?;
        for outward in [side, -side] {
            let Some(floor) = hang_floor(point, outward, ground) else {
                continue;
            };
            return Some(Target {
                edge: index,
                point,
                outward,
                floor,
                end: side_end(end, edge, outward),
                angle: 0.0,
            });
        }
        None
    }

    pub fn search(&mut self, body: &Body, ground: &dyn Ground) {
        self.candidate = None;
        self.air_hang_type = 0;
        if self.active.is_some() {
            return;
        }
        if !body.grounded {
            if !self.skip_air_search {
                self.air_search(body, ground);
            }
            return;
        }
        let p = Vec3::from(body.position);
        let mut nearest = GROUND_REACH;
        for i in 0..self.edges.len() {
            let edge = &self.edges[i];
            if edge.kind == 3 {
                let point = edge.closest(p);
                let distance = Vec3::new(p.x - point.x, 0.0, p.z - point.z).length();
                if distance <= nearest
                    && let Some(mut target) = self.cover_target(i, p, ground)
                    && angle_diff(body.yaw, target.yaw()).abs().to_degrees() <= FACING_LIMIT
                {
                    target.angle =
                        angle_diff(target.yaw() + std::f32::consts::PI, body.yaw).to_degrees();
                    nearest = distance;
                    self.candidate = Some(target);
                }
                continue;
            }
            if !is_hang(edge) {
                continue;
            }
            let point = edge.closest(p);
            let offset = p - point;

            if offset.x.hypot(offset.z) > nearest
                || (offset.y.abs() > TOP_TOLERANCE
                    && (offset.y + HAND_HEIGHT).abs() > TOP_TOLERANCE)
            {
                continue;
            }
            let Some(mut target) = self.target(i, p, ground) else {
                continue;
            };
            let offset = p - target.point;
            let distance = Vec3::new(offset.x, 0.0, offset.z).length();
            let from_top =
                offset.y.abs() <= TOP_TOLERANCE && offset.dot(target.outward) <= SIDE_TOLERANCE;
            let from_below = (target.point.y - (p.y + HAND_HEIGHT)).abs() <= TOP_TOLERANCE
                && offset.dot(target.outward) >= 0.0;
            if distance > nearest || !(from_top || from_below) {
                continue;
            }
            // yaw is measured around +Y, forward is (-sin yaw, -cos yaw).
            let desired = if from_top {
                (-target.outward.x).atan2(-target.outward.z)
            } else {
                target.yaw()
            };
            if angle_diff(body.yaw, desired).abs().to_degrees() > FACING_LIMIT {
                continue;
            }
            target.angle = angle_diff(target.yaw(), body.yaw).to_degrees();
            nearest = distance;
            self.candidate = Some(target);
        }
    }

    /// The air catch (env 3045): a hang edge whose lip is in [`AIR_HANG_BAND`] over the feet,
    /// with the body off its face within [`AIR_HANG_LONG`]; the nearest one wins. Hands the
    /// wall jump and plain falls into the ground hang's own docking.
    fn air_search(&mut self, body: &Body, ground: &dyn Ground) {
        let p = Vec3::from(body.position);
        let best = (0..self.edges.len())
            .filter_map(|i| {
                let edge = &self.edges[i];
                if !is_hang(edge) {
                    return None;
                }
                // Beside the edge's span, not past its ends (the unclamped projection).
                let s = edge.along(p);
                let point = edge.at(s.clamp(0.0, 1.0));
                // Cheap rejections before `target`'s collision probes.
                if !(0.0..=1.0).contains(&s)
                    || !(AIR_HANG_BAND.0..=AIR_HANG_BAND.1).contains(&(point.y - p.y))
                    || Vec3::new(p.x - point.x, 0.0, p.z - point.z).length() > AIR_HANG_LONG
                {
                    return None;
                }
                let target = self.target(i, p, ground)?;
                let off = (p - target.point).dot(target.outward);
                (-0.3..=AIR_HANG_LONG)
                    .contains(&off)
                    .then_some((target, off))
            })
            .min_by(|a, b| a.1.abs().total_cmp(&b.1.abs()));
        if let Some((mut target, off)) = best {
            target.angle = angle_diff(target.yaw(), body.yaw).to_degrees();
            self.air_hang_type = if off > AIR_HANG_SHORT {
                AIR_HANG_TYPE_LONG
            } else {
                AIR_HANG_TYPE_SHORT
            };
            self.candidate = Some(target);
        }
    }

    /// Returns true while docking owns movement, bypassing gravity and collision pushes.
    pub fn step(
        &mut self,
        body: &mut Body,
        animation: Option<&str>,
        time: f32,
        root_progress: Option<f32>,
        root_motion: [f32; 4],
        ground: &dyn Ground,
    ) -> bool {
        let name = animation.unwrap_or("");
        // Only the hang starts dock: StandToHang* and FreeFallToHang* (2171xx plain, 2172xx
        // foot hold). The air grab (AirHangGrab*, 2177xx) is still a free fall.
        if self.active.is_none() && (name.starts_with("a000_2171") || name.starts_with("a000_2172"))
        {
            self.active = self.candidate.clone();
        }
        let Some(target) = self.active.as_mut() else {
            return false;
        };
        if self.edges[target.edge].kind == 3 {
            return false;
        }
        if DROP_CLIPS.contains(&name) {
            body.grounded = false;
            body.velocity = [0.0; 3];
            body.fall_top = body.position[1];
            self.active = None;
            self.climb_start = None;
            return false;
        }
        let position = if CLIMB_CLIPS.contains(&name) {
            let start = *self.climb_start.get_or_insert(Vec3::from(body.position));
            // Precomputed climb displacement is fitted to the collision floor; progress
            // follows the clip's root track, with a time fallback for missing tracks.
            start.lerp(
                target.floor,
                root_progress
                    .unwrap_or(time / CLIMB_FALLBACK_DURATION)
                    .clamp(0.0, 1.0),
            )
        } else if self.climb_start.is_some() {
            body.position = target.floor.to_array();
            body.grounded = true;
            body.velocity = [0.0; 3];
            body.last_fall_height = 0.0;
            self.active = None;
            self.climb_start = None;
            return true;
        } else if is_hang_clip(name) {
            // RustSekiro hold: keep the authored root displacement along the edge.
            // Outward is (sin yaw, 0, cos yaw), so no angle round-trip is needed.
            let [x, z] = root_motion_world(root_motion, (target.outward.x, target.outward.z));
            let moved = target.point + Vec3::new(x, 0.0, z);
            let (index, point, end) = hang_span(&self.edges, &self.links, target.edge, moved);
            let lift = target.floor.y - target.point.y;
            let geometric = |outward: Vec3| climb_top(point, outward) + Vec3::Y * lift;
            if index == target.edge {
                target.floor = geometric(target.outward);
            } else {
                // A new edge: probe its floor; if that fails, `hang_can_climb` blocks the climb.
                target.outward = chain_outward(&self.edges, target.edge, index, target.outward);
                target.edge = index;
                target.floor = hang_floor(point, target.outward, ground)
                    .unwrap_or_else(|| geometric(target.outward));
            }
            target.point = point;
            target.end = side_end(end, &self.edges[index], target.outward);
            target.hang_position()
        } else if name.starts_with("a000_217") && !name.starts_with("a000_2177") {
            target.hang_position()
        } else {
            self.active = None;
            return false;
        };
        body.position = position.to_array();
        body.yaw = target.yaw();
        body.velocity = [0.0; 3];
        body.grounded = false;
        body.fall_top = position.y;
        true
    }
}

/// Wall jump (c0000_transition `BEH_A_WALL_JUMP`): how far out from the chest a wall counts as
/// touched. Not traced: the exe's contact check behind env 3020; a 0.8 m probe in eight
/// directions round the facing, as in Fireguy32's RustSekiro port.
pub const WALL_REACH: f32 = 0.8;
/// Height of the wall-contact probe over the feet (the chest).
pub const WALL_PROBE_LIFT: f32 = 1.0;
/// Shortfall of the probe below which nothing counts as touched.
pub const WALL_TOLERANCE: f32 = 0.05;

/// The nearest wall within [`WALL_REACH`] of the chest of a body at `feet` facing `yaw`: the
/// angle in degrees from the facing to it, positive to the right. This is what the engine hands
/// the HKS as `ContactWallAngle` and answers env 3020 with; the script turns it into
/// `Selector_WallContactType` (-45..45 F, beyond +-135 B, -135..-45 L, else R).
pub fn wall_contact(feet: Vec3, yaw: f32, ground: &dyn Ground) -> Option<f32> {
    let chest = [feet.x, feet.y + WALL_PROBE_LIFT, feet.z];
    // Ray k is k * 45 degrees to the right of the facing.
    let dirs: [Vec3; 8] =
        std::array::from_fn(|k| crate::arena::forward(yaw - (k as f32 * 45.0).to_radians()));
    let slid = ground.slide_fan(
        chest,
        dirs.map(|dir| [dir.x * WALL_REACH, dir.z * WALL_REACH]),
    );
    (0..8)
        .filter_map(|k| {
            let right = k as f32 * 45.0;
            let dir = dirs[k];
            let slid = slid[k];
            let shortfall = WALL_REACH - Vec3::new(slid[0], 0.0, slid[1]).dot(dir);
            let angle = if right > 180.0 { right - 360.0 } else { right };
            (shortfall > WALL_TOLERANCE).then_some((angle, shortfall))
        })
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(angle, _)| angle)
}
