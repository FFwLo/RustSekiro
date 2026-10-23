//! Wall hug (cover) and ledge hang: Wolf docked to a map edge (`crate::traversal::LedgeEdges`, kind 3 hug /
//! kind 2 hang). HKS c0000_transition.lua BEH_A_COVER_START / GROUND_HANG_START /
//! AIR_HANG_START pick the start; the c0000.hkx CustomDockingGenerator nodes hold Wolf at the
//! edge with the behavior variables DockingTranslationOffset_Cover (0, 0, 0.7), _Hang
//! (0, 1.6, -0.7), _HangStart (0, 0, -0.4), _HangBackStart (0, 0, 0.4), _HangLeftStart
//! (-0.4, 0, 0) and _HangRightStart (0.4, 0, 0) (their initial values, hkbBehaviorGraphData
//! variableInitialValues). docs/kb/docking.md has the research.
//!
//! The button is ACTION_ARM_WALL_HANG (key guide Kc@20 "talk / examine / wall hug / hang / air
//! hang grab", PC default E). Built 2026-10-17:
//! - **Edge frame**: `along` = a -> b (flat), the wall is on the side of `c` (the edge record's
//!   third vertex: on the gate map every hug edge with a wall found within 0.2 m has it on c's
//!   side, `edge_wall_side_probe`; the one hang edge's c lies on its cliff face, 10 m down).
//!   `out` points from the wall into the open: where Wolf stands to hug, where he hangs.
//! - **Docking**: the start clips blend Wolf from where he was to the docked spot (smoothstep
//!   over the clip, as the generators' blendType 0); held, the clips' root motion carries him
//!   along the edge and `hold` puts him back on its line (blendType 1), at the ends he stops.
//! - **Hug**: StandToCover2{Back,Left,Right,Left180,Right180} by the angle to the wall
//!   (c0000_transition 4518, c0000_define 290-297, stick idle); CoverIdle; CoverWalk / Run
//!   Start / Loop / End _L / _R along the wall; the button or a step -> CoverEnd.
//! - **Hang**: from the ground at the top of a hang edge (button): StandToHang1* -> 2* by
//!   HangStartEdgeAngle; in the air near one with the button held (env 1108 > 0):
//!   FreeFallToHang(LongRange)(Left / Right); HangIdle (HangFootHold* on foothold edges,
//!   variation 1); HangMove{Start_L/R, L/R, End_L/R}; button -> HangToStand (climb),
//!   a step -> HangToFreeFall; jump (any stick) -> HangJumpReady -> WallJumpStart_F_<type>
//!   (the kick off it; BEH_A_HANG_END_CONNECT is validated before BEH_A_HANG_CLIMB).
//! - **Wall jump** (BEH_A_WALL_JUMP): jump in the air with a wall within reach of the chest,
//!   once per flight (WALL_JUMP_COUNT_MAX 1): WallJumpReady_<contact F/B/L/R> ->
//!   WallJumpStart_<contact>_<type N/F/B/L/R>, whose own TAE 920 launches (decide's generic
//!   velocity change); the fall and landing are the ground jump's (the same clips:
//!   a000_201030 / 201040 / 201045). From a hug, jump -> CoverToWallJumpReady (the wall behind,
//!   contact B) -> WallJumpStart_B_<type>: no stick is the vertical jump up the wall.
//!
//! gaps (marked below): the exe's search box (FUN_140b7d810 / FUN_140b76110, DAT_143d85db4
//! unread), the offsets' frame (taken as: out from the wall, up), the start-angle sign (taken:
//! positive = to Wolf's right), the TAE air-grab windows (SP_EF_REF_TAE_ENABLE_AIR_HANG_GRAB),
//! the peak move at the edge ends.
//! - **Deathblows** (player.rs dock_deathblow): attack while hugging / hanging with an enemy that
//!   has not found Wolf in a start row's reach: ThrowParam 0060 / 0070 (hug, enemy left / right)
//!   or 0080 / 0130 (hang, enemy facing away / toward) start -> 0061 / 0071 / 0081 / 0131 main,
//!   through decide's DeathblowStart; the hang one keeps Wolf at the edge and returns to the hang.
//! - **Peek**: at a wall end with the stick still on that way, CoverAction{Start,Loop,End} where
//!   the edge's COVER_EDGE_TYPE allows that side; E from it -> CoverEndFromCoverLook.
//! - **Corners** (hang): at an edge end with the stick on, the next hang edge within CORNER_JOIN
//!   of it: in line -> carry on, round an outside corner HangCornerMove*, into an inside one
//!   HangInsideCornerMove* (FootHold sets on foothold edges).

use bevy::prelude::*;

use crate::actor::Actor;
use crate::data::CharData;
use crate::traversal::LedgeEdge;
use crate::player::CAPSULE_HALF_HEIGHT;

/// DockingTranslationOffset_Cover z: the hugging Wolf's distance out from the edge line.
pub const HUG_OUT: f32 = 0.7;
/// DockingTranslationOffset_Hang: the hanging Wolf's feet 1.6 m under and 0.7 m out from the
/// edge (gap: the frame of the offset, see the module doc).
pub const HANG_DOWN: f32 = 1.6;
pub const HANG_OUT: f32 = 0.7;
/// gap: the exe's search box. How far out from a hug edge Wolf may stand to dock, and how far
/// back from a hang edge's lip (on its top) to start the ground hang.
pub const HUG_REACH: f32 = 1.5;
pub const HANG_TOP_REACH: f32 = 1.2;
/// gap: air hang reach (env 3045 type 1 short / 2 long range): out from the face, and the
/// band of edge heights over the feet a fall or jump catches.
pub const AIR_HANG_SHORT: f32 = 1.0;
pub const AIR_HANG_LONG: f32 = 2.0;
pub const AIR_HANG_BAND: (f32, f32) = (1.0, 2.6);
/// How far from an edge end Wolf stops (gap: the exe's end check).
pub const END_MARGIN: f32 = 0.35;
/// c0000_define SP_EF_REF_TAE_ENABLE_AIR_HANG_GRAB: the jumps' TAE window that catches a lip
/// without the button (docs/kb/docking.md section 2).
pub const REF_ENABLE_AIR_HANG_GRAB: i64 = 5;
/// Stick deflection that moves Wolf along the edge.
const MOVE_STICK: f32 = 0.3;

/// Wolf held at an edge.
#[derive(Clone, Debug)]
pub struct Dock {
    pub edge: LedgeEdge,
    /// Flat unit vector a -> b, and out from the wall (horizontal).
    pub along: Vec3,
    pub out: Vec3,
    pub len: f32,
    /// Where along a -> b (metres).
    pub s: f32,
    /// Feet and yaw when the start clip began (the blend source).
    pub from: (Vec3, f32),
    /// The hang's climb: feet where it began.
    pub climb_from: Option<Vec3>,
    /// What decided the wall side (the log).
    pub side_from: &'static str,
}

impl Dock {
    fn new(e: &LedgeEdge, feet: Vec3, yaw: f32) -> Option<Dock> {
        Dock::new_hint(e, feet, yaw, None)
    }
    /// `hint`: the out of the edge Wolf comes from (the next edge along a ledge), taken when the
    /// collision cannot tell the side.
    fn new_hint(e: &LedgeEdge, feet: Vec3, yaw: f32, hint: Option<Vec3>) -> Option<Dock> {
        let d = (e.b - e.a).with_y(0.0);
        let len = d.length();
        if len < 2.0 * END_MARGIN {
            return None;
        }
        let along = d / len;
        let left = Vec3::Y.cross(along);
        let mid = (e.a + e.b) * 0.5;
        let side = (e.c - mid).dot(left);
        // The wall is on c's side; out is the other way. The map's collision decides first:
        // c's side was read off our gate map's edges and is not decoded, and on sekiro-rs's
        // maps it put Wolf's hang inside the ledge, facing it the wrong way.
        let (out, side_from) = match (wall_side(e, left), hint) {
            (Some(w), _) => (-w, "collision"),
            (None, Some(h)) => (if h.dot(left) >= 0.0 { left } else { -left }, "previous edge"),
            (None, None) => (if side >= 0.0 { -left } else { left }, "edge c"),
        };
        let s = ((feet - e.a).dot(along)).clamp(END_MARGIN, len - END_MARGIN);
        Some(Dock { edge: *e, along, out, len, s, from: (feet, yaw), climb_from: None, side_from })
    }
    pub fn hug(&self) -> bool {
        self.edge.kind == 3
    }
    /// A dock on the edge at its middle (checks of map edges).
    #[cfg(test)]
    pub fn new_from_edge(e: &LedgeEdge) -> Option<Dock> {
        Dock::new(e, (e.a + e.b) * 0.5, 0.0)
    }
    /// A 6 m edge along X at z = 0, its face to +Z, Wolf 1 m out (sim tests).
    #[cfg(test)]
    pub fn new_for_test(kind: u8) -> Dock {
        let e = LedgeEdge { a: Vec3::new(-3.0, 0.0, 0.0), b: Vec3::new(3.0, 0.0, 0.0), c: Vec3::new(0.0, -0.2, -0.1), kind, variation: 0 };
        Dock::new(&e, Vec3::new(0.0, 0.0, 1.0), 0.0).unwrap()
    }
    /// The edge line at `s` (its height runs from a to b).
    pub fn point(&self, s: f32) -> Vec3 {
        let k = (s / self.len).clamp(0.0, 1.0);
        self.edge.a.lerp(self.edge.b, k).with_y(self.edge.a.y + (self.edge.b.y - self.edge.a.y) * k)
    }
    /// Where Wolf's feet go, docked at `s`.
    pub fn feet(&self, s: f32) -> Vec3 {
        let p = self.point(s);
        if self.hug() {
            let f = p + self.out * HUG_OUT;
            // On the floor there (the edge runs along the wall's foot, a little above it).
            let y = crate::terrain::terrain().and_then(|t| t.floor(f.x, f.z, p.y + 0.5)).unwrap_or_else(|| crate::terrain::ground_y(f + Vec3::Y * CAPSULE_HALF_HEIGHT));
            f.with_y(y)
        } else {
            p + self.out * HANG_OUT - Vec3::Y * HANG_DOWN
        }
    }
    /// Wolf's docked facing: back to the wall when hugging, toward the wall when hanging.
    pub fn yaw(&self) -> f32 {
        let f = if self.hug() { self.out } else { -self.out };
        f32::atan2(-f.x, -f.z)
    }
    /// Climbed up: on top of the ledge, a step in from the lip.
    pub fn top(&self, s: f32) -> Vec3 {
        self.point(s) - self.out * 0.5
    }
    /// Wolf's right in the docked facing.
    pub fn right(&self) -> Vec3 {
        let f = if self.hug() { self.out } else { -self.out };
        f.cross(Vec3::Y)
    }
}

/// The side of an edge its wall (hug) or ledge top (hang) is on, asked of the map's collision:
/// the one side with a wall within reach of the chest (hug), or with a floor at the lip's
/// height (hang). None without a map or when both or neither side answers.
fn wall_side(e: &LedgeEdge, left: Vec3) -> Option<Vec3> {
    let t = crate::terrain::terrain()?;
    // One look at a point of the edge: +1 the wall / top is on `left`'s side, -1 the other, 0 unsure.
    let look = |m: Vec3| -> i32 {
        let side = |dir: Vec3| -> (bool, bool) {
            if e.kind == 3 {
                (crate::terrain::raycast(m + Vec3::Y * 1.0, dir, 0.6).is_some(), false)
            } else {
                // The top: a floor at the lip's height just in; the drop: none within 1 m under it.
                let p = m + dir * 0.4;
                let top = t.floor(p.x, p.z, m.y + 0.4).is_some_and(|h| (h - m.y).abs() < 0.4);
                let q = m + dir * 0.6;
                let drop = t.floor(q.x, q.z, m.y + 0.4).is_none_or(|h| h < m.y - 1.0);
                (top, drop)
            }
        };
        let ((l, ld), (r, rd)) = (side(left), side(-left));
        match (l, r) {
            (true, false) if e.kind == 3 || rd => 1,
            (false, true) if e.kind == 3 || ld => -1,
            (true, false) | (false, true) => if l { 1 } else { -1 },
            _ if e.kind == 2 && ld != rd => if rd { 1 } else { -1 },
            _ => 0,
        }
    };
    let votes: i32 = [0.25, 0.5, 0.75].iter().map(|&k| look(e.a.lerp(e.b, k))).sum();
    match votes.signum() {
        1 => Some(left),
        -1 => Some(-left),
        _ => None,
    }
}

/// Whether Wolf's feet are near enough an edge's segment to look at it (the finders run over
/// every edge of the play world each tick; the collision is asked only for the near ones).
fn near(e: &LedgeEdge, feet: Vec3, reach: f32) -> bool {
    let ab = e.b - e.a;
    let k = ((feet - e.a).dot(ab) / ab.length_squared().max(1e-6)).clamp(0.0, 1.0);
    (e.a + ab * k).distance_squared(feet) < reach * reach
}

/// Signed angle (degrees) from `from` to `to` about +Y, positive to the right (gap: the exe's
/// sign, see the module doc).
fn angle_right(from: Vec3, to: Vec3) -> f32 {
    let (f, t) = (from.with_y(0.0).normalize_or_zero(), to.with_y(0.0).normalize_or_zero());
    let cross_y = f.z * t.x - f.x * t.z;
    (-cross_y).atan2(f.dot(t)).to_degrees()
}

fn forward(yaw: f32) -> Vec3 {
    Quat::from_rotation_y(yaw) * Vec3::NEG_Z
}

/// The nearest hug edge Wolf can dock to: in front of its wall (on the out side), within reach.
pub fn find_hug(edges: &[LedgeEdge], feet: Vec3, yaw: f32) -> Option<Dock> {
    edges
        .iter()
        .filter(|e| e.kind == 3 && near(e, feet, HUG_REACH + 1.0))
        .filter_map(|e| Dock::new(e, feet, yaw))
        .filter_map(|d| {
            let p = d.point(d.s);
            let off = (feet - p).dot(d.out);
            let on = ((feet - d.edge.a).dot(d.along)) >= -0.2 && ((feet - d.edge.a).dot(d.along)) <= d.len + 0.2;
            (on && (-0.2..=HUG_REACH).contains(&off) && (feet.y - p.y).abs() < 0.8).then_some((d, off))
        })
        .min_by(|x, y| x.1.total_cmp(&y.1))
        .map(|(d, _)| d)
}

/// A hang edge whose top Wolf stands on, close to the lip.
pub fn find_hang_top(edges: &[LedgeEdge], feet: Vec3, yaw: f32) -> Option<Dock> {
    edges
        .iter()
        .filter(|e| e.kind == 2 && near(e, feet, AIR_HANG_LONG + AIR_HANG_BAND.1 + 1.0))
        .filter_map(|e| Dock::new(e, feet, yaw))
        .filter_map(|d| {
            let p = d.point(d.s);
            let back = -(feet - p).dot(d.out);
            let on = ((feet - d.edge.a).dot(d.along)) >= 0.0 && ((feet - d.edge.a).dot(d.along)) <= d.len;
            (on && (-0.1..=HANG_TOP_REACH).contains(&back) && (feet.y - p.y).abs() < 0.5).then_some((d, back))
        })
        .min_by(|x, y| x.1.total_cmp(&y.1))
        .map(|(d, _)| d)
}

/// A hang edge a fall or jump can catch: its lip in the band over the feet, Wolf off its face
/// within the long range. Returns the dock and whether it is the long-range catch.
pub fn find_hang_air(edges: &[LedgeEdge], feet: Vec3, yaw: f32) -> Option<(Dock, bool)> {
    edges
        .iter()
        .filter(|e| e.kind == 2 && near(e, feet, AIR_HANG_LONG + AIR_HANG_BAND.1 + 1.0))
        .filter_map(|e| Dock::new(e, feet, yaw))
        .filter_map(|d| {
            let p = d.point(d.s);
            let off = (feet - p).dot(d.out);
            let up = p.y - feet.y;
            let on = ((feet - d.edge.a).dot(d.along)) >= 0.0 && ((feet - d.edge.a).dot(d.along)) <= d.len;
            (on && (-0.3..=AIR_HANG_LONG).contains(&off) && (AIR_HANG_BAND.0..=AIR_HANG_BAND.1).contains(&up)).then_some((d, off))
        })
        .min_by(|x, y| x.1.total_cmp(&y.1))
        .map(|(d, off)| (d, off > AIR_HANG_SHORT))
}

/// The hug start clip by the angle from Wolf's facing to the wall (c0000_transition 4518, stick
/// idle): facing it (|a| <= 45) the 180 turns, back to it (|a| > 135) Back, else Left / Right.
pub fn hug_start_state(d: &Dock, yaw: f32, stick: Vec3) -> &'static str {
    // Stick held: by the stick's angle to the wall, -150..-30 MoveToCoverLeft, 30..150
    // MoveToCoverRight, beyond Back, else the 180s (c0000_transition 4518).
    if stick.length_squared() > 0.01 {
        let a = angle_right(stick, -d.out);
        return if a.abs() > 150.0 {
            "StandToCover2Back"
        } else if (-150.0..=-30.0).contains(&a) {
            "MoveToCoverLeft"
        } else if (30.0..=150.0).contains(&a) {
            "MoveToCoverRight"
        } else if a >= 0.0 {
            "StandToCover2Right180"
        } else {
            "StandToCover2Left180"
        };
    }
    let a = angle_right(forward(yaw), -d.out);
    if a.abs() > 135.0 {
        "StandToCover2Back"
    } else if a.abs() <= 45.0 {
        if a >= 0.0 { "StandToCover2Right180" } else { "StandToCover2Left180" }
    } else if a > 0.0 {
        "StandToCover2Right"
    } else {
        "StandToCover2Left"
    }
}

/// The ground hang start's first clip by HangStartEdgeAngle (c0000_transition 4571): from Wolf's
/// facing to the drop, -45..45 1Back, 45..135 1Left, -135..-45 1Right, else the plain one.
/// The FootHold set on foothold edges (variation 1).
pub fn hang_start_state(d: &Dock, yaw: f32) -> String {
    let a = angle_right(forward(yaw), d.out);
    let dir = if a.abs() <= 45.0 {
        "1Back"
    } else if (45.0..=135.0).contains(&a) {
        "1Left"
    } else if (-135.0..=-45.0).contains(&a) {
        "1Right"
    } else {
        ""
    };
    let set = if d.edge.variation == 1 { "StandToHangFootHold" } else { "StandToHang" };
    if dir.is_empty() {
        if set == "StandToHang" { set.to_string() } else { format!("{set}1") }
    } else {
        format!("{set}{dir}")
    }
}

/// The start variant whose own root motion (path and turn) brings Wolf closest to the docked spot:
/// the angle tables' sign could not be settled from the HKS alone, and a wrong pick played a
/// clip that walked away from the edge and was dragged back (the user: "not smooth"). Each
/// candidate is (the state to play, the clip whose motion counts); a clip without root motion
/// counts as standing still.
pub fn best_start(d: &CharData, candidates: &[(String, String)], from: (Vec3, f32), target: (Vec3, f32)) -> Option<String> {
    candidates
        .iter()
        .filter(|(st, motion)| d.anim_key(st).is_some() && d.anim_key(motion).is_some())
        .map(|(st, motion)| {
            let key = d.anim_key(motion).unwrap();
            let len = d.length(key);
            let (end, yaw) = match (d.root_at(key, 0.0), d.root_at(key, len)) {
                (Some((p0, y0)), Some((pe, ye))) => {
                    let frame = Quat::from_rotation_y(from.1 - y0);
                    (from.0 + frame * Vec3::new(pe.x - p0.x, d.root_y_at(key, len) - d.root_y_at(key, 0.0), pe.y - p0.y), from.1 + (ye - y0))
                }
                _ => from,
            };
            let cost = end.distance(target.0) + angle_delta(yaw, target.1).abs() * 0.5;
            (st.clone(), cost)
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(st, _)| st)
}

/// The ground hang start by best_start over the four (FootHold sets on foothold edges): the
/// first clip (StandToHang1X, 0.1 s, no motion) is played, the second's motion decides.
pub fn hang_start_best(d: &CharData, dock: &Dock, from: (Vec3, f32)) -> String {
    let set = if dock.edge.variation == 1 { "StandToHangFootHold" } else { "StandToHang" };
    let cands: Vec<(String, String)> = ["", "Back", "Left", "Right"]
        .iter()
        .map(|dir| {
            let first = if dir.is_empty() { if set == "StandToHang" { set.to_string() } else { format!("{set}1") } } else { format!("{set}1{dir}") };
            (first, format!("{set}2{dir}"))
        })
        .collect();
    best_start(d, &cands, from, (dock.feet(dock.s), dock.yaw())).unwrap_or_else(|| hang_start_state(dock, from.1))
}

/// The hug start by best_start over the five StandToCover2 clips (stick idle).
pub fn hug_start_best(d: &CharData, dock: &Dock, from: (Vec3, f32)) -> &'static str {
    const ALL: [&str; 5] = ["StandToCover2Back", "StandToCover2Left", "StandToCover2Right", "StandToCover2Left180", "StandToCover2Right180"];
    let cands: Vec<(String, String)> = ALL.iter().map(|s| (s.to_string(), s.to_string())).collect();
    let best = best_start(d, &cands, from, (dock.feet(dock.s), dock.yaw()));
    ALL.into_iter().find(|s| best.as_deref() == Some(*s)).unwrap_or_else(|| hug_start_state(dock, from.1, Vec3::ZERO))
}

/// The second clip of a ground hang start (StandToHang1X -> StandToHang2X).
pub fn hang_start_next(state: &str) -> Option<String> {
    for set in ["StandToHangFootHold", "StandToHang"] {
        if let Some(rest) = state.strip_prefix(set) {
            if rest.starts_with('2') {
                return None;
            }
            let rest = rest.strip_prefix('1').unwrap_or(rest);
            return Some(format!("{set}2{rest}"));
        }
    }
    None
}

/// The air catch by the angle from Wolf's facing to the wall (c0000_transition 4602).
pub fn air_hang_state(d: &Dock, yaw: f32, long: bool) -> String {
    let a = angle_right(forward(yaw), -d.out);
    let side = if a < -45.0 { "Left" } else if a > 45.0 { "Right" } else { "" };
    let foot = if d.edge.variation == 1 { "FootHold" } else { "" };
    let range = if long { "LongRange" } else { "" };
    format!("FreeFallToHang{foot}{range}{side}")
}

pub fn is_start(state: &str) -> bool {
    state.starts_with("StandToCover2") || state.starts_with("MoveToCover") || state.starts_with("StandToHang") || state.starts_with("FreeFallToHang") || is_corner(state)
}

/// The hang corner moves (HangCornerMove* round an outside corner, HangInsideCornerMove* into an
/// inside one): blended from the old edge to the next like a start.
pub fn is_corner(state: &str) -> bool {
    state.starts_with("HangCornerMove") || state.starts_with("HangInsideCornerMove")
}

/// How the next hang edge meets this one at the end Wolf moves to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Turn {
    /// The same face goes on (another edge record in line): Wolf just carries on.
    Straight,
    /// Round an outside corner (the face turns away from the hands).
    Outside,
    /// Into an inside corner (the face turns toward them).
    Inside,
}

/// gap: how close the next edge's end must be (the exe's corner checks, env 3048 / 3049 / 3059 /
/// 3060, are not traced).
pub const CORNER_JOIN: f32 = 0.6;

/// At the end of a hang edge, the hang edge that goes on from it (an end within CORNER_JOIN of
/// this one's), docked at that end, and how it turns.
pub fn next_hang_edge(edges: &[LedgeEdge], d: &Dock, toward_b: bool) -> Option<(Dock, Turn)> {
    let end = if toward_b { d.point(d.len) } else { d.point(0.0) };
    let travel = if toward_b { d.along } else { -d.along };
    edges
        .iter()
        .filter(|e| e.kind == d.edge.kind && !(e.a == d.edge.a && e.b == d.edge.b))
        .filter_map(|e| {
            let (da, db) = (e.a.distance(end), e.b.distance(end));
            if da.min(db) > CORNER_JOIN {
                return None;
            }
            // The side carries over only along the same line (a corner's face turns).
            let straight = (e.b - e.a).with_y(0.0).normalize_or_zero().dot(d.along).abs() > 0.9;
            let mut nd = Dock::new_hint(e, end, 0.0, straight.then_some(d.out))?;
            nd.s = if da <= db { END_MARGIN } else { nd.len - END_MARGIN };
            // The new face's normal against the way Wolf was moving.
            let k = nd.out.dot(travel);
            let turn = if nd.out.dot(d.out) > 0.7 {
                Turn::Straight
            } else if k > 0.5 {
                Turn::Outside
            } else if k < -0.5 {
                Turn::Inside
            } else {
                return None;
            };
            Some((nd, turn, da.min(db)))
        })
        .min_by(|x, y| x.2.total_cmp(&y.2))
        .map(|(nd, t, _)| (nd, t))
}

/// The corner clip: Wolf's own side (he faces the wall), FootHold set on foothold edges.
pub fn corner_state(turn: Turn, right: bool, foothold: bool) -> String {
    let kind = if turn == Turn::Inside { "HangInsideCornerMove" } else { "HangCornerMove" };
    format!("{kind}{}{}", if foothold { "FootHold" } else { "" }, if right { "Right" } else { "Left" })
}

pub fn is_docked_state(state: &str) -> bool {
    is_start(state) || is_peek(state) || (state.starts_with("Cover") && state != "CoverEnd" && state != "CoverEndFromCoverLook") || (state.starts_with("Hang") && !state.ends_with("ToFreeFall"))
}

/// The hug peek at a wall end (STYLE_TYPE_COVER_LOOK): CoverAction{Start,Loop,End}{L,R} (the
/// game spells the left start CoveActionStartL).
pub fn is_peek(state: &str) -> bool {
    state.starts_with("CoverAction") || state == "CoveActionStartL"
}

/// May Wolf peek out on that side (the clip letter)? The hug edge's variation is HKS
/// COVER_EDGE_TYPE: 0 no look, 1 free look, 2 left look only, 3 right look only, 10-13 the same
/// with the peak move (gap: the peak move itself is not done).
pub fn peek_allowed(variation: i64, side: &str) -> bool {
    match variation % 10 {
        1 => true,
        2 => side == "L",
        3 => side == "R",
        _ => false,
    }
}

/// The jump-off ready poses out of a hug / hang (CoverToWallJumpReady: the wall behind;
/// Hang(FootHold)JumpReady: the wall in front).
pub fn is_jump_ready(state: &str) -> bool {
    state == "CoverToWallJumpReady" || state == "HangJumpReady" || state == "HangFootHoldJumpReady"
}

/// Docked: the idle / move states from the stick along the edge. Returns the state to play,
/// if it should change. `ended`: the current clip ran out.
pub fn held_state(d: &Dock, a: &Actor, stick: Vec3, walk: bool, ended: bool) -> Option<String> {
    // The hug clips' _L / _R are named from the front (Wolf faces out): CoverRun _L carries him
    // to his own right (sim test: the stick to his right ran him that way under _R otherwise
    // backwards). The hang ones are his own left / right (facing the wall).
    let along = if d.hug() { -stick.dot(d.right()) } else { stick.dot(d.right()) };
    let side = if along > MOVE_STICK { Some("R") } else if along < -MOVE_STICK { Some("L") } else { None };
    // At an end of the edge, no further that way.
    let toward_b = |side: &str| ((side == "R") == (d.right().dot(d.along) > 0.0)) != d.hug();
    let blocked = |side: &str| if toward_b(side) { d.s >= d.len - END_MARGIN - 1e-3 } else { d.s <= END_MARGIN + 1e-3 };
    let pushing = side;
    let side = side.filter(|s| !blocked(s));
    let s = a.state.as_str();
    if d.hug() {
        // Peeking (CoverAction): held while the stick stays on that way, else its end.
        if is_peek(s) {
            let x = if s.ends_with('L') { "L" } else { "R" };
            if pushing == Some(x) {
                return if s.contains("Start") { ended.then(|| format!("CoverActionLoop{x}")) } else { ended.then(|| format!("CoverActionLoop{x}")) };
            }
            return if s.contains("End") { ended.then(|| "CoverIdle".to_string()) } else { Some(format!("CoverActionEnd{x}")) };
        }
        // At a wall end with the stick still on that way: the peek, where the edge allows it.
        if let Some(x) = pushing.filter(|x| blocked(x) && peek_allowed(d.edge.variation, x)) {
            if !s.contains("Start_") {
                return Some(if x == "L" { "CoveActionStartL".to_string() } else { "CoverActionStartR".to_string() });
            }
        }
        let gait = if walk { "Walk" } else { "Run" };
        match (side, s) {
            (Some(x), _) if s.starts_with(&format!("Cover{gait}Start_{x}")) => ended.then(|| format!("Cover{gait}Loop_{x}")),
            (Some(x), _) if s.starts_with(&format!("Cover{gait}Loop_{x}")) => ended.then(|| format!("Cover{gait}Loop_{x}")),
            (Some(x), _) => Some(format!("Cover{gait}Start_{x}")),
            (None, _) if s.contains("Start_") || s.contains("Loop_") => {
                let x = if s.ends_with('L') { "L" } else { "R" };
                let g = if s.starts_with("CoverRun") { "Run" } else { "Walk" };
                Some(format!("Cover{g}End_{x}"))
            }
            (None, "CoverIdle") => ended.then(|| "CoverIdle".to_string()),
            (None, _) if s.contains("End_") => ended.then(|| "CoverIdle".to_string()),
            (None, _) => Some("CoverIdle".to_string()),
        }
    } else {
        let foot = if d.edge.variation == 1 { "FootHold" } else { "" };
        let idle = format!("Hang{foot}Idle");
        let start = |x: &str| format!("Hang{foot}MoveStart_{x}");
        // The plain set's loop is HangMoveL / HangMoveR; the foothold one HangFootHoldMoveLoop_L / _R.
        let lp = |x: &str| if foot.is_empty() { format!("HangMove{x}") } else { format!("HangFootHoldMoveLoop_{x}") };
        let end = |x: &str| format!("Hang{foot}MoveEnd_{x}");
        match side {
            Some(x) if s == start(x) || s == lp(x) => ended.then(|| lp(x)),
            Some(x) => Some(start(x)),
            None if s.contains("MoveStart_") || s.starts_with("HangMoveL") || s.starts_with("HangMoveR") || s.contains("MoveLoop_") => {
                let x = if s.ends_with('L') { "L" } else { "R" };
                Some(end(x))
            }
            None if s == idle => ended.then_some(idle),
            None if s.contains("MoveEnd_") => ended.then_some(idle),
            None => Some(idle),
        }
    }
}

/// The docked state changes in the log (SHINOBI_DOCK_LOG=0 off).
pub fn debug_dock(state: &str) {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if *ON.get_or_init(|| std::env::var("SHINOBI_DOCK_LOG").map_or(true, |v| v != "0")) {
        info!("dock: {state}");
    }
}

/// Plays a docked state. HangMoveL has no clip in the CMSG (animId 0): its loop is a000_020202,
/// the left twin of HangMoveR's a000_020203.
pub fn play(a: &mut Actor, d: &CharData, state: &str) -> bool {
    let again = a.state == state;
    if !again {
        debug_dock(state);
    }
    let ok = if state == "HangMoveL" && d.anim_key(state).is_none() && d.anim("a000_020202").is_some() {
        a.play(state, "a000_020202");
        true
    } else {
        a.play_state(d, state)
    };
    // A loop played on (the shimmy, the hug walk / run, the idles) keeps its full step: a blend-in
    // at every cycle dipped the speed each time round (the user: "shimmying is slow").
    if ok && again {
        a.blend_from = None;
        a.blend_len = 0.0;
    }
    ok
}

/// How long a start without root motion (the catches FreeFallToHang*, 3.3 s long, the hug's
/// StandToCover2Back) takes to put Wolf on the edge, and after how long it hands over to the
/// held states (the user: the catch was "so slow"). gap: their TAE cancel windows are not read.
pub const NO_ROOT_BLEND: f32 = 0.3;
pub const NO_ROOT_DONE: f32 = 0.5;

/// After a clip's root motion is done, how long its pose settles before the next state takes over
/// (the user: the grabs "too slow when grabbing at angle": StandToHang2 moves for 0.6 s of its
/// 1.4 s, the rest is the hands settling). gap: the clips' hkb end transitions are not read.
pub const SETTLE: f32 = 0.15;

/// When a clip's root motion is done: from then on its root (xz, height, yaw) stays within 2 cm
/// and 2 deg of where the clip ends. None without root motion.
pub fn motion_end(d: &CharData, key: &str) -> Option<f32> {
    let len = d.length(key);
    let (pe, ye) = d.root_at(key, len)?;
    let he = d.root_y_at(key, len);
    let step = 1.0 / 60.0;
    let mut t = len;
    while t > step {
        let (p, y) = d.root_at(key, t - step)?;
        if p.distance(pe) > 0.02 || (d.root_y_at(key, t - step) - he).abs() > 0.02 || (y - ye).abs() > 0.035 {
            break;
        }
        t -= step;
    }
    Some(t)
}

/// Is the start (corner, climb) clip done: its end; with root motion SETTLE past the motion's
/// end; without, NO_ROOT_DONE in.
pub fn start_done(d: &CharData, a: &Actor, ended: bool) -> bool {
    ended
        || match motion_end(d, &a.anim) {
            Some(m) => a.t >= m + SETTLE,
            None => a.t >= NO_ROOT_DONE,
        }
}

/// Where a guided clip (a start, a corner, the climb) puts Wolf at time `t`: the clip's own root
/// motion from where it began (the same frame as actor.rs advance: the clip-start yaw, so a turn
/// in the clip carries its path round), plus the gap left at the clip's end to the docked spot
/// eased in over the clip. Without root motion: a smoothstep onto the spot over NO_ROOT_BLEND.
#[allow(clippy::too_many_arguments)]
pub fn guided(d: &CharData, key: &str, t: f32, from: (Vec3, f32), target: (Vec3, f32)) -> (Vec3, f32) {
    let len = d.length(key).max(1e-3);
    let smooth = |k: f32| {
        let k = k.clamp(0.0, 1.0);
        k * k * (3.0 - 2.0 * k)
    };
    match (d.root_at(key, 0.0), d.root_at(key, t.min(len)), d.root_at(key, len)) {
        (Some((p0, y0)), Some((pt, yt)), Some((pe, ye))) => {
            let frame = Quat::from_rotation_y(from.1 - y0);
            let at = |p: Vec2, tt: f32| from.0 + frame * Vec3::new(p.x - p0.x, d.root_y_at(key, tt) - d.root_y_at(key, 0.0), p.y - p0.y);
            let (now, end) = (at(pt, t.min(len)), at(pe, len));
            // The gap eases in over the clip's motion (it hands over SETTLE after: start_done).
            let span = motion_end(d, key).unwrap_or(len).max(0.1);
            let k = smooth(t / span);
            let yaw_now = from.1 + (yt - y0);
            let yaw_end = from.1 + (ye - y0);
            (now + (target.0 - end) * k, yaw_now + angle_delta(yaw_end, target.1) * k)
        }
        _ => {
            let k = smooth(t / NO_ROOT_BLEND);
            (from.0.lerp(target.0, k), from.1 + angle_delta(from.1, target.1) * k)
        }
    }
}

/// Wall jump (c0000_transition BEH_A_WALL_JUMP; c0000_define WALL_JUMP_COUNT_MAX 1): how far
/// from the chest a wall counts as touched. gap: the exe's contact check (it hands the HKS
/// ContactWallAngle); 0.8 m of rays in eight directions round the facing.
pub const WALL_REACH: f32 = 0.8;
pub const WALL_JUMP_COUNT_MAX: u8 = 1;

/// Wall jumps allowed a flight: WALL_JUMP_COUNT_MAX, lifted on the parkour test map for wall-to-
/// wall chaining (the user: the shaft "was for wall to wall chaining"; map.rs sets it).
pub static WALL_JUMP_LIMIT: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(WALL_JUMP_COUNT_MAX);

pub fn wall_jump_limit() -> u8 {
    WALL_JUMP_LIMIT.load(std::sync::atomic::Ordering::Relaxed)
}

/// The nearest wall within reach of the chest: the angle (degrees, + to Wolf's right) from his
/// facing to it - the HKS ContactWallAngle.
pub fn wall_contact(chest: Vec3, yaw: f32) -> Option<f32> {
    let f = forward(yaw);
    (0..8)
        .filter_map(|k| {
            let d = Quat::from_rotation_y(-(k as f32) * std::f32::consts::FRAC_PI_4) * f;
            crate::terrain::raycast(chest, d, WALL_REACH).or_else(|| test_wall_hit(chest, d)).map(|t| (angle_right(f, d), t))
        })
        .min_by(|x, y| x.1.total_cmp(&y.1))
        .map(|(a, _)| a)
}

/// The world direction (from the chest) of the nearest wall `wall_contact` finds.
pub fn wall_contact_dir(chest: Vec3, yaw: f32) -> Option<Vec3> {
    let f = forward(yaw);
    (0..8)
        .filter_map(|k| {
            let d = Quat::from_rotation_y(-(k as f32) * std::f32::consts::FRAC_PI_4) * f;
            crate::terrain::raycast(chest, d, WALL_REACH).or_else(|| test_wall_hit(chest, d)).map(|t| (d, t))
        })
        .min_by(|x, y| x.1.total_cmp(&y.1))
        .map(|(d, _)| d)
}

/// Sim tests' walls: vertical planes (a point and the normal out of them), far from the arena
/// (x = 500) so no other test meets them; the map's global terrain stays untouched.
#[cfg(test)]
pub static TEST_WALLS: std::sync::Mutex<Vec<(Vec3, Vec3)>> = std::sync::Mutex::new(Vec::new());

#[cfg(test)]
fn test_wall_hit(o: Vec3, d: Vec3) -> Option<f32> {
    TEST_WALLS.lock().ok()?.iter().filter_map(|(p, n)| {
        let den = d.dot(*n);
        (den < -1e-4).then(|| (*p - o).dot(*n) / den).filter(|t| (0.0..=WALL_REACH).contains(t))
    }).min_by(|a, b| a.total_cmp(b))
}

#[cfg(not(test))]
fn test_wall_hit(_: Vec3, _: Vec3) -> Option<f32> {
    None
}

/// Selector_WallContactType from ContactWallAngle: -45..45 F, beyond +-135 B, -135..-45 L, else R.
pub fn contact_type(angle: f32) -> char {
    if angle > -45.0 && angle < 45.0 {
        'F'
    } else if angle > 135.0 || angle < -135.0 {
        'B'
    } else if angle > -135.0 && angle < -45.0 {
        'L'
    } else {
        'R'
    }
}

/// Selector_WallJumpType when the ready pose ends (c0000_transition 6720-6900): no stick ->
/// VERTICAL (N); else the stick's angle to the facing, F / B / L / R by the same cones, and a
/// jump into the wall (the type's side = the contact's) is the vertical one.
pub fn jump_type(contact: char, stick: Vec3, yaw: f32) -> char {
    if stick.length_squared() < 0.01 {
        return 'N';
    }
    let t = contact_type(angle_right(forward(yaw), stick));
    if t == contact { 'N' } else { t }
}

/// The take-off after a ready pose: WallJumpStart_<contact>_<type>.
pub fn wall_jump_start(contact: char, ty: char) -> String {
    format!("WallJumpStart_{contact}_{ty}")
}

/// FixedUpdate after the root motion: Wolf held on the edge. The start clips blend him from
/// where he began; held, he stays on the line, the clip's motion along it kept (and stopped at
/// the ends); the climb lifts him onto the top over HangToStand.
pub fn hold(combat: Res<crate::data::Combat>, mut q: Query<(&mut crate::player::Player, &mut Actor, &mut Transform)>) {
    let d = &combat.player;
    for (mut p, mut a, mut tf) in &mut q {
        let Some(dock) = p.dock.as_mut() else {
            a.docked = false;
            continue;
        };
        a.docked = true;
        a.airborne = false;
        a.vel_y = 0.0;
        a.move_vel = Vec3::ZERO;
        let feet = tf.translation - Vec3::Y * CAPSULE_HALF_HEIGHT;
        let len = d.length(&a.anim).max(1e-3);
        let k = (a.t / len).clamp(0.0, 1.0);
        let smooth = k * k * (3.0 - 2.0 * k);
        let state = a.state.clone();
        let _ = smooth;
        let (target, yaw) = if state.starts_with("HangToStand") || state.starts_with("HangFootHoldToStand") {
            let from = *dock.climb_from.get_or_insert(feet);
            guided(d, &a.anim, a.t, (from, dock.yaw()), (dock.top(dock.s), dock.yaw()))
        } else if hang_start_next(&state).is_some() {
            // The ground hang's first clip (StandToHang1X, 0.1 s, no motion): Wolf stays; the
            // second clip's own path takes him over the lip.
            dock.from
        } else if is_start(&state) {
            guided(d, &a.anim, a.t, dock.from, (dock.feet(dock.s), dock.yaw()))
        } else {
            // Held: keep the motion along the line, clamp to the edge.
            dock.s = (feet - dock.edge.a).dot(dock.along).clamp(END_MARGIN, dock.len - END_MARGIN);
            (dock.feet(dock.s), dock.yaw())
        };
        tf.translation = target + Vec3::Y * CAPSULE_HALF_HEIGHT;
        a.yaw = yaw;
        tf.rotation = Quat::from_rotation_y(yaw);
    }
}

fn angle_delta(from: f32, to: f32) -> f32 {
    (to - from + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

#[cfg(test)]
mod tests {
    use bevy::prelude::*;

    fn edge(kind: u8) -> crate::traversal::LedgeEdge {
        // A wall along +X at z = 0, c on its -Z side (the wall), the open side +Z.
        crate::traversal::LedgeEdge { a: Vec3::new(-2.0, 0.0, 0.0), b: Vec3::new(2.0, 0.0, 0.0), c: Vec3::new(0.0, -0.2, -0.1), kind, variation: 0 }
    }

    #[test]
    fn the_wall_is_on_cs_side() {
        let d = super::Dock::new(&edge(3), Vec3::new(0.0, 0.0, 1.0), 0.0).unwrap();
        assert!(d.out.distance(Vec3::Z) < 1e-5, "out {:?}", d.out);
        // Hugging: back to the wall, facing +Z.
        let f = Quat::from_rotation_y(d.yaw()) * Vec3::NEG_Z;
        assert!(f.distance(Vec3::Z) < 1e-5, "facing {f:?}");
        assert!((d.feet(d.s).z - super::HUG_OUT).abs() < 1e-5);
    }

    #[test]
    fn hug_found_in_front_of_the_wall_only() {
        let e = [edge(3)];
        assert!(super::find_hug(&e, Vec3::new(0.5, 0.0, 1.0), 0.0).is_some());
        assert!(super::find_hug(&e, Vec3::new(0.5, 0.0, -1.0), 0.0).is_none(), "behind the wall");
        assert!(super::find_hug(&e, Vec3::new(0.5, 0.0, 3.0), 0.0).is_none(), "too far");
    }

    #[test]
    fn hug_start_by_angle() {
        let d = super::Dock::new(&edge(3), Vec3::new(0.0, 0.0, 1.0), 0.0).unwrap();
        // Facing the wall (-Z, yaw 0): a 180.
        assert!(super::hug_start_state(&d, 0.0, Vec3::ZERO).ends_with("180"));
        // Back to the wall (facing +Z).
        assert_eq!(super::hug_start_state(&d, std::f32::consts::PI, Vec3::ZERO), "StandToCover2Back");
    }

    #[test]
    fn hang_catch_from_the_face_side() {
        let e = [edge(2)];
        // Falling past the face (out side +Z), lip 2 m over the feet.
        assert!(super::find_hang_air(&e, Vec3::new(0.0, -2.0, 0.6), 0.0).is_some());
        // Too low: the lip 4 m up.
        assert!(super::find_hang_air(&e, Vec3::new(0.0, -4.0, 0.6), 0.0).is_none());
        // On top near the lip: the ground start.
        assert!(super::find_hang_top(&e, Vec3::new(0.0, 0.0, -0.6), 0.0).is_some());
        assert_eq!(super::hang_start_next("StandToHang1Left").as_deref(), Some("StandToHang2Left"));
        assert_eq!(super::hang_start_next("StandToHang").as_deref(), Some("StandToHang2"));
    }

    /// Which side of each hug / hang edge of the gate map the wall is on: rays at 1 m (hug) /
    /// below the edge (hang) out to both sides of the a-b segment's middle, and where c lies.
    #[test]
    #[ignore]
    fn edge_wall_side_probe() {
        let dir = crate::paths::extracted();
        let id = "m11_01_00_00";
        let Some(t) = std::fs::read(dir.join(format!("map_{id}.hit"))).ok().and_then(|d| crate::terrain::Terrain::from_hit(&d)) else { return };
        let json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join(format!("map_{id}.json"))).unwrap()).unwrap();
        for e in crate::map::load_ledges(&json) {
            let mid = (e.a + e.b) * 0.5;
            let along = (e.b - e.a).with_y(0.0).normalize();
            // Left of a->b seen from above (+Y up).
            let left = Vec3::Y.cross(along);
            let probe = |h: f32| {
                let o = mid + Vec3::Y * h;
                (t.raycast(o, left, 1.5), t.raycast(o, -left, 1.5))
            };
            let floor = t.floor(mid.x, mid.z, mid.y + 0.5);
            let below = t.floor(mid.x + left.x * 0.5, mid.z + left.z * 0.5, mid.y - 0.1).map(|f| mid.y - f);
            let below_r = t.floor(mid.x - left.x * 0.5, mid.z - left.z * 0.5, mid.y - 0.1).map(|f| mid.y - f);
            println!(
                "kind {} var {} len {:.2} mid {:.2?} floor {:?} c.left {:.2} c.y {:.2} | ray L/R at +1 {:?} at +0.3 {:?} at -0.5 {:?} | drop L {:?} R {:?}",
                e.kind,
                e.variation,
                e.a.distance(e.b),
                mid,
                floor.map(|f| mid.y - f),
                (e.c - mid).dot(left),
                e.c.y - mid.y,
                probe(1.0),
                probe(0.3),
                probe(-0.5),
                below,
                below_r
            );
        }
    }
}

#[cfg(test)]
mod pose_probe {
    use bevy::prelude::*;

    /// Model-space bone positions of a clip frame (Bevy space, X mirrored as the game loads it).
    fn bones_at(lib: &crate::anim::Lib, key: &str, frame: usize) -> Vec<(String, Vec3)> {
        let clip = &lib.clips[key];
        let tracks = clip.track_to_bone.len();
        let f = frame.min(clip.frames - 1);
        let mut local: Vec<Transform> = lib.bones.iter().map(|b| b.pose).collect();
        for (ti, &b) in clip.track_to_bone.iter().enumerate() {
            if b >= 0 && (b as usize) < local.len() {
                local[b as usize] = clip.data[f * tracks + ti];
            }
        }
        let mut model: Vec<Mat4> = vec![Mat4::IDENTITY; local.len()];
        for i in 0..local.len() {
            let m = local[i].to_matrix();
            let p = lib.bones[i].parent;
            model[i] = if p >= 0 { model[p as usize] * m } else { m };
        }
        lib.bones.iter().enumerate().map(|(i, b)| (b.name.clone(), model[i].transform_point3(Vec3::ZERO))).collect()
    }

    /// Where the hands, head and pelvis sit against the clip root in the hang and hug idles (the
    /// docking offsets' check). `cargo test docking::pose_probe -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn dock_pose_probe() {
        let lib = crate::anim::load_lib(&crate::paths::extracted().join("anim_c0000.bin"));
        for (name, key) in [("HangIdle", "a000_020000"), ("CoverIdle", "a000_021000"), ("StandIdle", "a000_000000")] {
            if !lib.clips.contains_key(key) {
                println!("{name}: no clip {key}");
                continue;
            }
            let b = bones_at(&lib, key, 10);
            let get = |n: &str| b.iter().find(|(x, _)| x == n).map(|(_, p)| *p);
            println!("{name} {key}: R_Hand {:?} L_Hand {:?} Head {:?} Pelvis {:?} R_Foot {:?}", get("R_Hand"), get("L_Hand"), get("Head"), get("Pelvis"), get("R_Foot"));
        }
    }
}
