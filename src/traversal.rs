//! The map's traversal points the core's moves use: wire points and the wire's target pick
//! (docs/kb/grapple.md; the engine's "wire"), hang and wall-hug edges. The host's map fills
//! the `GrapplePoints` and `LedgeEdges` resources (this game's map.rs from its exports).

use bevy::prelude::*;

/// One wire point of the map (json "grapple", tools/sekiro-extract map export): the hook the
/// rope attaches to, the landing spot, the point's facing and its WireVariationParam row. Bevy
/// space (X mirrored on load).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GrapplePoint {
    pub hook: Vec3,
    pub land: Vec3,
    pub fwd: Vec3,
    pub variation: i64,
}

/// The loaded map's wire points (empty without a map, in the sandbox, and on maps exported
/// before the grapple export).
#[derive(Resource, Default)]
pub struct GrapplePoints(pub Vec<GrapplePoint>);

/// A hang (kind 2) or wall-hug (kind 3) edge from the map's and objects' `.edge` files
/// (kb/grapple.md section 1): the record's three vertices raw, in Bevy space. `a`-`b` is the
/// edge's segment; what `c` means per kind is not decoded (gap). Data for the ledge hang and
/// wall hug moves (HKS BEH_A_GROUND_HANG_START, COVER_*); nothing uses it yet but the gizmos.
#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
pub struct LedgeEdge {
    pub a: Vec3,
    pub b: Vec3,
    pub c: Vec3,
    pub kind: u8,
    pub variation: i64,
}

/// The loaded map's hang / hug edges (empty on maps exported before 2026-10-11's ledge export).
#[derive(Resource, Default)]
pub struct LedgeEdges(pub Vec<LedgeEdge>);

/// The point the wire would fire at ([`grapple_target`]).
#[derive(Clone, Copy, Debug)]
#[allow(dead_code)] // read by the player move (VXf)
pub struct GrappleTarget {
    pub index: usize,
    pub point: GrapplePoint,
    /// Wolf's centre to the hook.
    pub dist: f32,
    /// Inside the row's action distance (the hook can be fired; outside it the marker is shown
    /// as the grey `MENU_Wire_Distance` disc).
    pub in_action_range: bool,
    /// Rideable (landing on the point, LandWireMove) or a swing-through point (targetMarkerType
    /// 1 ring with an arrow: rows 20/23/24 "can't ride", 41 kite).
    pub ride: bool,
}

/// WirePointSearchParam row 0 (WireSetParam 0 = rows [0, 100], the default set).
pub const WIRE_ACTION_NEAR: f32 = 3.0;
pub const WIRE_ACTION_FAR: f32 = 15.0;
pub const WIRE_FIND_FAR: f32 = 25.0;
/// filteringDistanceHeightUpper / Lower 10.2 and filteringDistanceWidth 14.3: the rectangle in
/// the camera's plane a point must be inside to be a candidate; softlockDistanceHeight 10.2 /
/// Width 7.15 the smaller one the current target keeps itself inside. Taken as metres in the
/// camera plane: the height values one-sided, the widths the full width (soft lock = half the
/// filter width). // gap: the exe may scale them with depth (screen fractions); untraced.
const WIRE_FILTER_HEIGHT_UPPER: f32 = 10.2;
const WIRE_FILTER_HEIGHT_LOWER: f32 = 10.2;
const WIRE_FILTER_WIDTH: f32 = 14.3;
const WIRE_SOFTLOCK_HEIGHT: f32 = 10.2;
const WIRE_SOFTLOCK_WIDTH: f32 = 7.15;

/// WireVariationParam: recognitionAngleUp/Down/Left/Right (degrees, the player's side of the
/// point's reference direction), extendWireActionDistance (m, added to actionDistanceFar) and
/// whether the point is ridden (targetMarkerType 0) or swung through (1). Rows 0/3/20/23/24
/// are 90/90/180/180, 1/2/21/22 and 5 (extend 10) 90 all round, 40 90/90/180/180 extend 30
/// (swung through: targetMarkerType 1), 41 90/50/30/10 extend 30; enemy rows 10700..70210 are
/// the SpEffect's (never map points).
pub(crate) fn wire_variation(row: i64) -> ([f32; 4], f32, bool) {
    match row {
        1 | 2 | 21 | 22 => ([90.0, 90.0, 90.0, 90.0], 0.0, row < 20),
        5 => ([90.0, 90.0, 90.0, 90.0], 10.0, true),
        20 | 23 | 24 => ([90.0, 90.0, 180.0, 180.0], 0.0, false),
        40 => ([90.0, 90.0, 180.0, 180.0], 30.0, false),
        41 => ([90.0, 50.0, 30.0, 10.0], 30.0, false),
        // gap: rows 45 (homing +53105) and the enemy rows: taken as row 0.
        _ => ([90.0, 90.0, 180.0, 180.0], 0.0, true),
    }
}

/// The wire point Wolf would fire at: of the points within findDistanceFar whose recognition
/// angles admit him and that sit inside the camera-plane rectangle, the one nearest the view
/// axis; `prev` (last frame's target) is kept while it stays inside the smaller soft-lock
/// rectangle (WirePointSearchParam 0). A collision triangle between Wolf's chest and the hook
/// hides the point. `cam_fwd` unit; `wolf` his centre.
pub fn grapple_target(cam_pos: Vec3, cam_fwd: Vec3, wolf: Vec3, prev: Option<usize>, points: &[GrapplePoint]) -> Option<GrappleTarget> {
    let right = cam_fwd.cross(Vec3::Y).try_normalize().unwrap_or(Vec3::X);
    let up = right.cross(cam_fwd).normalize();
    let chest = wolf + Vec3::Y * 0.3;
    // (index, dist, lateral, vertical, depth, ride, action far) of every candidate.
    let mut found: Vec<(usize, f32, f32, f32, f32, bool, f32)> = Vec::new();
    for (i, p) in points.iter().enumerate() {
        let to_hook = p.hook - wolf;
        let dist = to_hook.length();
        if dist > WIRE_FIND_FAR || dist < 1e-3 {
            continue;
        }
        let (angles, extend, ride) = wire_variation(p.variation);
        // Wolf in the point's frame: ahead = the reference direction, horizontal angle to the
        // right positive, vertical angle up positive. // gap: which side "left" is (mirrored X)
        // and whether the angles are read from the hook or from the landing point.
        let fwd = p.fwd.try_normalize().unwrap_or(Vec3::Z);
        let pright = fwd.cross(Vec3::Y).try_normalize().unwrap_or(Vec3::X);
        let to_wolf = -to_hook / dist;
        let horizontal = to_wolf.x.hypot(to_wolf.z).max(1e-4);
        let h = pright.dot(to_wolf).atan2(fwd.dot(to_wolf)).to_degrees();
        let v = (to_wolf.y / horizontal).atan().to_degrees();
        let ok_v = if v >= 0.0 { v <= angles[0] } else { -v <= angles[1] };
        let ok_h = if h >= 0.0 { h <= angles[3] } else { -h <= angles[2] };
        if !ok_v || !ok_h {
            continue;
        }
        // The camera-plane rectangle.
        let rel = p.hook - cam_pos;
        let depth = rel.dot(cam_fwd);
        if depth <= 0.0 {
            continue;
        }
        let lateral = rel.dot(right);
        let vertical = rel.dot(up);
        if lateral.abs() > WIRE_FILTER_WIDTH / 2.0 || vertical > WIRE_FILTER_HEIGHT_UPPER || -vertical > WIRE_FILTER_HEIGHT_LOWER {
            continue;
        }
        // Nothing in the way (the rope has no collision: kb/grapple.md).
        if let Some(t) = crate::terrain::raycast(chest, (p.hook - chest).normalize(), (p.hook - chest).length() - 0.3) {
            if t < (p.hook - chest).length() - 0.3 {
                continue;
            }
        }
        found.push((i, dist, lateral, vertical, depth, ride, WIRE_ACTION_FAR + extend));
    }
    if found.is_empty() {
        return None;
    }
    let pick = prev
        .and_then(|k| found.iter().find(|f| f.0 == k && f.2.abs() <= WIRE_SOFTLOCK_WIDTH / 2.0 && f.3.abs() <= WIRE_SOFTLOCK_HEIGHT))
        .or_else(|| found.iter().min_by(|a, b| (a.2 / a.4).hypot(a.3 / a.4).total_cmp(&(b.2 / b.4).hypot(b.3 / b.4))))?;
    let &(index, dist, _, _, _, ride, far) = pick;
    Some(GrappleTarget { index, point: points[index], dist, in_action_range: (WIRE_ACTION_NEAR..=far).contains(&dist), ride })
}
