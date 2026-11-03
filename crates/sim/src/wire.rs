//! Authored wire points and solved wire flight, using FwLo's MIT engine research.
use crate::{
    body::{Body, Ground},
    collision::CollisionWorld,
    input::angle_diff,
};
use glam::{EulerRot, Quat, Vec3};
use serde::Deserialize;
use std::{collections::HashMap, path::Path};

#[derive(Debug, Clone, Deserialize)]
pub struct Point {
    pub position: Option<[f32; 3]>,
    pub variation: u32,
    pub model: String,
    pub source_part: String,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Search {
    pub action_distance_near: f32,
    pub action_distance_far: f32,
    pub find_distance_far: f32,
    /// The view box hooks must project into: metres above and below the camera's axis and to
    /// each side of it, at [`VIEW_BOX_DEPTH`] in front of the camera.
    pub filtering_distance_height_upper: f32,
    pub filtering_distance_height_lower: f32,
    pub filtering_distance_width: f32,
    /// The narrower box a marked hook stays marked in (its soft lock).
    pub softlock_distance_height_upper: f32,
    pub softlock_distance_height_lower: f32,
    pub softlock_distance_width: f32,
}
/// The `WireVariationParam` fields the runtime uses. Its `recognitionAngle*` limits are not
/// read: the game tests them against each edge's own direction, which `grapple.json` does not
/// carry (docs/FIDELITY.md, "Grapple target search").
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Variation {
    pub id: u32,
    pub target_marker_type: u8,
    pub extend_wire_action_distance: f32,
    pub chr_physics_homing_id_offset: u32,
}
#[derive(Debug, Clone)]
pub struct Target {
    pub state: MarkerState,
    pub index: usize,
    pub point: Vec3,
    pub arrival: Vec3,
    pub rideable: bool,
}
/// Recognition and firing are separate: the HUD previews a point before it is usable.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MarkerState {
    Hidden,
    Distant { fill: f32 },
    Ready,
}
impl Search {
    /// The marker state of a hook `distance` metres from Wolf whose variation extends the
    /// wire's reach by `extension`: the game adds the extension to both the usable and the
    /// shown distance (docs/MOBILITY.md section 4.6), so a kite row with 30 m is usable to 45 m
    /// and shown to 55 m.
    pub fn marker_state(&self, distance: f32, extension: f32) -> MarkerState {
        let action_far = self.action_distance_far + extension;
        let find_far = self.find_distance_far + extension;
        if !distance.is_finite() || distance < self.action_distance_near || distance > find_far {
            return MarkerState::Hidden;
        }
        if distance <= action_far {
            MarkerState::Ready
        } else {
            MarkerState::Distant {
                fill: ((find_far - distance) / (find_far - action_far)).clamp(0.0, 1.0),
            }
        }
    }
}
/// The search's check ray starts this far in front of the camera (m); a hook must lie beyond
/// the plane through that start (`WireActionTargetMng`, 1.6.0.0 0x14328917c).
pub const CHECK_RAY_START: f32 = 0.8;
/// How far beyond [`CHECK_RAY_START`] the view box sits (m): the manager's default view
/// filtering distance (field +0xb4, debug menu built at 0x140efccc0).
pub const VIEW_FILTER_DISTANCE: f32 = 15.0;
/// The view box's distance in front of the camera (m).
pub const VIEW_BOX_DEPTH: f32 = CHECK_RAY_START + VIEW_FILTER_DISTANCE;
/// The player back filter (degrees): a hook more than 180 minus this from the camera's
/// horizontal look direction, seen from Wolf, is skipped (field +0x148, 105 by default).
pub const BACK_FILTER_ANGLE: f32 = 105.0;
/// [`BACK_FILTER_ANGLE`] while locked on (field +0x14c): hooks up to 135 degrees from the
/// camera's look direction, seen from Wolf, pass.
pub const BACK_FILTER_ANGLE_LOCKED: f32 = 45.0;

/// The camera the hook search looks through.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WireCamera {
    pub position: Vec3,
    /// Unit look direction.
    pub forward: Vec3,
}
impl WireCamera {
    /// The orbit camera at `distance` behind `focus`, turned to `yaw` (looking along
    /// `rotate_y(yaw) * -Z`) and pitched down by `pitch` radians.
    pub fn orbit(focus: Vec3, yaw: f32, pitch: f32, distance: f32) -> Self {
        let forward = Quat::from_euler(EulerRot::YXZ, yaw, -pitch, 0.0) * Vec3::NEG_Z;
        Self {
            position: focus - forward * distance,
            forward,
        }
    }
    /// The camera's up axis: square to its look direction, with right along the horizon.
    pub fn up(&self) -> Option<Vec3> {
        let right = self.forward.cross(Vec3::Y).try_normalize()?;
        Some(right.cross(self.forward))
    }
    /// Where `point` sits in this camera's view: (right, up, depth), with right taken along
    /// the horizon as the game's view box is.
    fn view(&self, point: Vec3) -> Option<(f32, f32, f32)> {
        let up = self.up()?;
        let right = self.forward.cross(up);
        let d = point - self.position;
        Some((d.dot(right), d.dot(up), d.dot(self.forward)))
    }
}

/// The follow camera's rig as the simulation knows it: how high above Wolf's feet it looks and
/// how far back it sits before walls pull it in. The duel copies the camera director's values
/// in each step, so the hook search, the camera on screen and replays all use the same ones.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraRig {
    pub focus_height: f32,
    pub distance: f32,
}
impl Default for CameraRig {
    /// LockCamParam row 0's defaults: 4.5 m from a focus 1.5 m above the feet.
    fn default() -> Self {
        let p = crate::camera::CameraParams::default();
        Self {
            focus_height: p.chr_org_offset_y,
            distance: p.cam_dist,
        }
    }
}

/// Height above the feet the hook search's sight ray starts at (m). Not traced.
pub const SIGHT_LIFT: f32 = 1.2;
/// How far short of the hook the sight ray stops (m), so the hook's own mesh does not block
/// it. Not traced.
pub const SIGHT_MARGIN: f32 = 0.1;

/// The hook search's sight ray from `chest` (the feet plus [`SIGHT_LIFT`]) to the hook at
/// `point`: its unit direction and length, for [`CollisionWorld::raycast`].
pub fn sight_ray(chest: Vec3, point: Vec3) -> (Vec3, f32) {
    let ray = point - chest;
    let length = ray.length();
    (ray / length, (length - SIGHT_MARGIN).max(0.0))
}

#[derive(Default)]
pub struct Wire {
    /// The follow camera's rig the hook search builds its camera from (see
    /// [`Self::search_camera`]).
    pub rig: CameraRig,
    pub points: Vec<Point>,
    pub search: Option<Search>,
    pub variations: HashMap<u32, Variation>,
    pub offsets: HashMap<u32, Vec3>,
    pub collision: Option<CollisionWorld>,
    /// Best usable hook, also consumed by HKS and dramatic camera angles.
    pub candidate: Option<Target>,
    /// Best Ready hook, or best Distant hook when none is usable.
    pub preview: Option<Target>,
    pub active: Option<Target>,
    start: Option<Vec3>,
    elapsed: f32,
    duration: f32,
    arrived: bool,
    clip: String,
    pub tip: Option<Vec3>,
    pub rope_dummy: Option<i16>,
    /// Virtual support for authored rideable ornaments absent from terrain collision. It holds
    /// Wolf only while he stays over it: [`Self::leave_perch`] drops it once he is outside its
    /// [`PERCH_RADIUS`] footprint, and the next grapple shot drops it too (#230).
    pub perch: Option<Vec3>,
}
impl Wire {
    /// Ends the perch once the feet at `feet` are outside its footprint (walked or knocked off,
    /// docked, warped), so it never holds Wolf up again there (#230). A jump straight up keeps
    /// it, as he is still over it and lands back on it. FwLo's RustSekiro drops `Player::perch`
    /// by the same rule.
    pub fn leave_perch(&mut self, feet: Vec3) {
        self.perch = self
            .perch
            .filter(|p| (p.x - feet.x).hypot(p.z - feet.z) <= PERCH_RADIUS);
    }
    pub fn can_fire(&self) -> bool {
        self.candidate.is_some()
    }
    pub fn flight_time(&self) -> Option<(f32, f32)> {
        (self.start.is_some() || self.arrived).then_some((self.elapsed, self.duration))
    }

    pub fn load(cache: &Path, map: &Path) -> Result<Self, String> {
        let read = |p: &Path| -> Result<serde_json::Value, String> {
            serde_json::from_slice(&std::fs::read(p).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())
        };
        let rows = |name: &str| -> Result<Vec<serde_json::Value>, String> {
            read(&cache.join(format!("params/{name}.json")))?["rows"]
                .as_array()
                .cloned()
                .ok_or_else(|| format!("missing {name} rows"))
        };
        let search = rows("WirePointSearchParam")?
            .into_iter()
            .find(|r| r["id"] == 0)
            .ok_or("missing wire search row 0")?;
        let variations: Vec<Variation> = rows("WireVariationParam")?
            .into_iter()
            .map(serde_json::from_value)
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string())?;
        let mut offsets = HashMap::new();
        for row in rows("ChrPhysicsHomingParam")? {
            let id = row["id"].as_u64().unwrap_or(0) as u32;
            offsets.insert(
                id,
                Vec3::new(
                    row["targetOffsetX"].as_f64().unwrap_or(0.0) as f32,
                    row["targetOffsetY"].as_f64().unwrap_or(0.0) as f32,
                    row["targetOffsetZ"].as_f64().unwrap_or(0.0) as f32,
                ),
            );
        }
        Ok(Self {
            points: serde_json::from_value(read(&map.join("grapple.json"))?)
                .map_err(|e| e.to_string())?,
            search: Some(serde_json::from_value(search).map_err(|e| e.to_string())?),
            variations: variations.into_iter().map(|v| (v.id, v)).collect(),
            offsets,
            // The player's collision: helper parts the player passes through do not stop the
            // line of sight or the flight (`crate::hit_filter`).
            collision: Some(CollisionWorld::load_map(map)?),
            ..Self::default()
        })
    }
    /// The camera the hook search looks through: the follow camera orbiting [`Self::rig`]'s
    /// focus above Wolf's feet at the step's camera yaw and pitch (radians, down positive),
    /// pulled in front of walls as the camera on screen is. It is built from the step's input
    /// and the simulation's own state only, so runs replay the same at any frame rate.
    pub fn search_camera(&self, body: &Body, camera_yaw: f32, camera_pitch: f32) -> WireCamera {
        let focus = Vec3::from(body.position) + Vec3::Y * self.rig.focus_height;
        let full = WireCamera::orbit(focus, camera_yaw, camera_pitch, self.rig.distance);
        let distance = self.collision.as_ref().map_or(self.rig.distance, |w| {
            crate::camera::wall_pulled_distance(w, focus, -full.forward, self.rig.distance)
        });
        WireCamera::orbit(focus, camera_yaw, camera_pitch, distance)
    }

    /// The game's hook search (`WireActionTargetMng`, docs/MOBILITY.md section 4.6): only
    /// hooks in front of the camera and inside its view box are seen, the one nearest the
    /// centre of the screen is marked, and the marked hook is the one the wire fires at.
    /// `camera_yaw` and `camera_pitch` are the step's camera ([`crate::InputFrame`]);
    /// `locked_on` widens the back filter as the game does while locked on.
    pub fn search(&mut self, body: &Body, camera_yaw: f32, camera_pitch: f32, locked_on: bool) {
        let held = self
            .preview
            .as_ref()
            .filter(|t| t.state == MarkerState::Ready)
            .map(|t| t.index);
        self.candidate = None;
        self.preview = None;
        let Some(s) = &self.search else { return };
        let camera = self.search_camera(body, camera_yaw, camera_pitch);
        let origin = Vec3::from(body.position);
        let chest = origin + Vec3::Y * SIGHT_LIFT;
        let look = Vec3::new(camera.forward.x, 0.0, camera.forward.z).normalize_or_zero();
        let back_angle = if locked_on {
            BACK_FILTER_ANGLE_LOCKED
        } else {
            BACK_FILTER_ANGLE
        };
        let back_cos = (180.0 - back_angle).to_radians().cos();
        // A hook the camera can see inside a box (half width, above, below; metres at
        // VIEW_BOX_DEPTH): its state and its distance from the screen centre, as tangents of
        // the view angles, which rank as the game's screen distance does.
        let seen = |index: usize, (width, upper, lower): (f32, f32, f32)| {
            let p = self.points.get(index)?;
            let v = self.variations.get(&p.variation)?;
            let point = Vec3::from(p.position?);
            let delta = point - origin;
            let state = s.marker_state(delta.length(), v.extend_wire_action_distance);
            if state == MarkerState::Hidden {
                return None;
            }
            // The player back filter: horizontally, from Wolf, within 75 degrees (135 locked
            // on) of the camera's look direction; skipped straight above or below Wolf.
            if Vec3::new(delta.x, 0.0, delta.z)
                .try_normalize()
                .is_some_and(|flat| flat.dot(look) < back_cos)
            {
                return None;
            }
            // In front of the check ray's start and inside the box.
            let (x, y, z) = camera.view(point)?;
            if z <= CHECK_RAY_START {
                return None;
            }
            let (x, y) = (x / z, y / z);
            if x.abs() > width / VIEW_BOX_DEPTH
                || y > upper / VIEW_BOX_DEPTH
                || -y > lower / VIEW_BOX_DEPTH
            {
                return None;
            }
            Some((state, x.hypot(y), point, v))
        };
        let clear = |point: Vec3| {
            let (dir, length) = sight_ray(chest, point);
            !self
                .collision
                .as_ref()
                .is_some_and(|w| w.raycast(chest, dir, length).is_some())
        };
        // The soft lock: a marked, usable hook stays marked while it stays usable, unblocked
        // and inside the narrower soft-lock box, even when another hook is nearer the centre.
        let softlock = (
            s.softlock_distance_width,
            s.softlock_distance_height_upper,
            s.softlock_distance_height_lower,
        );
        let mut chosen = held
            .filter(|&i| self.active.as_ref().is_none_or(|t| t.index != i))
            .and_then(|i| seen(i, softlock).map(|hook| (i, hook)))
            .filter(|(_, (state, _, point, _))| *state == MarkerState::Ready && clear(*point));
        if chosen.is_none() {
            let filter = (
                s.filtering_distance_width,
                s.filtering_distance_height_upper,
                s.filtering_distance_height_lower,
            );
            let mut seen_hooks: Vec<_> = (0..self.points.len())
                .filter(|&i| self.active.as_ref().is_none_or(|t| t.index != i))
                .filter_map(|i| seen(i, filter).map(|hook| (i, hook)))
                .collect();
            // Usable hooks first, then nearest the screen centre (comparator 0x140efeb60), then
            // nearest Wolf; the first in that order with a clear sight ray is marked, so rays
            // are cast only until one is clear, as the game does.
            seen_hooks.sort_by(|(_, (a, ac, ap, _)), (_, (b, bc, bp, _))| {
                (*a != MarkerState::Ready)
                    .cmp(&(*b != MarkerState::Ready))
                    .then(ac.total_cmp(bc))
                    .then(ap.distance(origin).total_cmp(&bp.distance(origin)))
            });
            chosen = seen_hooks
                .into_iter()
                .find(|(_, (_, _, point, _))| clear(*point));
        }
        if let Some((index, (state, _, point, v))) = chosen {
            let offset = self
                .offsets
                .get(&(300000000 + v.chr_physics_homing_id_offset))
                .copied()
                .unwrap_or_default();
            let delta = point - origin;
            let forward = Vec3::new(delta.x, 0.0, delta.z).normalize_or_zero();
            let right = Vec3::new(-forward.z, 0.0, forward.x);
            self.preview = Some(Target {
                state,
                index,
                point,
                arrival: point + right * offset.x + Vec3::Y * offset.y + forward * offset.z,
                rideable: v.target_marker_type == 0,
            });
        }
        // The marker and the pull are one selection: only the marked hook can be fired at.
        self.candidate = self
            .preview
            .as_ref()
            .filter(|t| t.state == MarkerState::Ready)
            .cloned();
    }
    pub fn step(
        &mut self,
        body: &mut Body,
        animation: Option<&str>,
        travel_window: Option<(f32, f32)>,
        dt: f32,
        ground: &dyn Ground,
    ) -> bool {
        self.advance(body, animation, 0.0, None, travel_window, dt, ground)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn advance(
        &mut self,
        body: &mut Body,
        animation: Option<&str>,
        time: f32,
        tae: Option<&crate::tae::TaeAnim>,
        travel_window: Option<(f32, f32)>,
        dt: f32,
        ground: &dyn Ground,
    ) -> bool {
        use crate::tae::TaeKind;
        let shot = animation.is_some_and(|a| {
            matches!(
                a,
                "a000_202000" | "a000_202100" | "a000_202600" | "a000_202700"
            )
        });
        let moving = animation.is_some_and(|a| {
            matches!(
                a,
                "a000_202010"
                    | "a000_202011"
                    | "a000_202012"
                    | "a000_202110"
                    | "a000_202112"
                    | "a000_202610"
                    | "a000_202710"
                    | "a000_202020"
                    | "a000_201030"
            )
        });
        let changed = self.clip != animation.unwrap_or_default();
        self.clip = animation.unwrap_or_default().to_owned();
        if shot && changed {
            debug_assert!(
                self.candidate
                    .as_ref()
                    .is_none_or(|t| t.state == MarkerState::Ready)
            );
            self.active = self.candidate.clone();
            // The shot leaves the perch it was fired from behind (#230).
            self.perch = None;
            self.arrived = false;
            self.start = None;
            self.elapsed = 0.0;
            self.duration = 0.0;
            self.tip = Some(Vec3::from(body.position) + Vec3::Y);
        }
        if !shot && !moving {
            self.active = None;
            self.start = None;
            self.tip = None;
            self.rope_dummy = None;
            return false;
        }
        let Some(target) = self.active.clone() else {
            return false;
        };
        let event = |kind| {
            tae.and_then(|a| {
                a.events.iter().find(|e| {
                    e.start <= time
                        && time < e.end
                        && matches!(e.kind, TaeKind::Wire { kind: k, .. } if k == kind)
                })
            })
        };
        self.rope_dummy = event(715).and_then(|e| {
            let TaeKind::Wire { args, .. } = &e.kind else {
                return None;
            };
            let model = i32::from_le_bytes(args.get(..4)?.try_into().ok()?);
            if model != 3 {
                return None;
            }
            Some(i16::from_le_bytes(args.get(4..6)?.try_into().ok()?))
        });
        if event(901).is_some() {
            self.tip = Some(target.point);
        } else if let Some(e) = event(900) {
            let TaeKind::Wire { args, .. } = &e.kind else {
                unreachable!()
            };
            let speed = args
                .get(..4)
                .and_then(|b| b.try_into().ok())
                .map(f32::from_le_bytes)
                .unwrap_or(50.0);
            let tip = self.tip.get_or_insert(Vec3::from(body.position) + Vec3::Y);
            let d = target.point - *tip;
            *tip += d.normalize_or_zero() * (speed * dt).min(d.length());
        }
        if event(902).is_some() {
            let d = target.point - Vec3::from(body.position);
            let yaw = (-d.x).atan2(-d.z);
            let speed: f32 = if matches!(animation, Some("a000_202600" | "a000_202700")) {
                2700.0
            } else {
                1080.0
            };
            body.yaw +=
                angle_diff(yaw, body.yaw).clamp(-speed.to_radians() * dt, speed.to_radians() * dt);
        }
        if self.arrived {
            return body.grounded && target.rideable;
        }
        body.fall_type = 2;
        if shot {
            if event(905).is_some() || animation == Some("a000_202000") {
                body.velocity = [0.0; 3];
                return true;
            }
            return false;
        }
        let launch = tae
            .and_then(|a| {
                a.events
                    .iter()
                    .find(|e| matches!(e.kind, TaeKind::VelocityChange(1100)))
            })
            .map(|e| e.start);
        if self.start.is_none() && launch.is_some_and(|t| time < t) {
            // FwLo traced row 1101 and Linear/EaseIn curve ids; the latter's shape is a fit.
            if let Some(e) = event(904) {
                let blend = e.progress(time);
                let d = target.point - Vec3::from(body.position);
                let horizontal = Vec3::new(d.x, 0.0, d.z).normalize_or_zero();
                body.velocity =
                    (horizontal * 30.0 * blend + Vec3::Y * 30.0 * blend * blend).to_array();
                let current = Vec3::from(body.position);
                let delta = Vec3::from(body.velocity) * dt;
                let hit = self.sweep(current, delta);
                body.position = (current
                    + hit.map_or(delta, |d| delta.normalize_or_zero() * (d - 0.05).max(0.0)))
                .to_array();
                if hit.is_some() {
                    self.release();
                    body.grounded = false;
                }
            }
            return true;
        }
        self.start.get_or_insert_with(|| {
            self.elapsed = 0.0;
            let authored_arrival = tae
                .and_then(|a| {
                    a.events
                        .iter()
                        .find(|e| matches!(e.kind, TaeKind::CameraSlot(19)))
                })
                .zip(launch)
                .map(|(e, start)| e.start - start);
            self.duration = authored_arrival
                .unwrap_or_else(|| {
                    if tae.is_some() {
                        if matches!(animation, Some("a000_202011" | "a000_202012")) {
                            0.83
                        } else {
                            0.67
                        }
                    } else {
                        travel_window.map_or(0.83, |(a, b)| b - a)
                    }
                })
                .max(dt);
            let d = target.arrival - Vec3::from(body.position);
            body.velocity = (d / self.duration + Vec3::Y * (9.8 * self.duration)).to_array();
            Vec3::from(body.position)
        });
        let remaining = (self.duration - self.elapsed).max(0.0);
        let step = if remaining <= dt + 1e-6 {
            remaining
        } else {
            dt
        };
        let current = Vec3::from(body.position);
        let mut velocity = Vec3::from(body.velocity);
        if remaining > 0.0 {
            velocity.x = (target.arrival.x - current.x) / remaining;
            velocity.z = (target.arrival.z - current.z) / remaining;
        }
        let next = current + velocity * step - Vec3::Y * (9.8 * step * step);
        velocity.y -= 19.6 * step;
        body.velocity = velocity.to_array();
        self.elapsed += step;
        let delta = next - current;
        // Sweep feet and capsule centres, stopping before any collision rather than tunnelling.
        let hit = self.sweep(current, delta);
        if let Some(hit) = hit {
            body.position =
                (current + delta.normalize_or_zero() * (hit - 0.05).max(0.0)).to_array();
            self.release();
            body.grounded = false;
            return true;
        }
        body.position = next.to_array();
        body.grounded = false;
        body.fall_top = next.y;
        if self.elapsed >= self.duration {
            self.arrived = true;
            self.start = None;
            body.grounded = target.rideable;
            if body.grounded {
                self.perch = Some(Vec3::new(
                    next.x,
                    ground
                        .floor(next.x, next.z, next.y + 0.1)
                        .filter(|y| (next.y - y).abs() <= 0.6)
                        .unwrap_or(next.y),
                    next.z,
                ));
                body.velocity = [0.0; 3];
            }
            body.last_fall_height = 0.0;
        }
        true
    }

    fn release(&mut self) {
        self.active = None;
        self.start = None;
        self.tip = None;
        self.rope_dummy = None;
    }

    /// How far the body at feet `current` flies along `delta` before it meets a face: rays along
    /// its centre line at the feet, the capsule's sphere centres and over the head, and the top
    /// half of the head sphere swept along the move, so an overhang beside the centre line
    /// stops it too (#168).
    fn sweep(&self, current: Vec3, delta: Vec3) -> Option<f32> {
        self.collision.as_ref().and_then(|w| {
            [0.05, 0.4, 1.2, 1.55]
                .into_iter()
                .filter_map(|h| {
                    w.raycast(
                        current + Vec3::Y * h,
                        delta.normalize_or_zero(),
                        delta.length() + 0.001,
                    )
                })
                .chain(w.head_sweep(current, delta))
                .min_by(f32::total_cmp)
        })
    }
}

/// How far from a ridden landing spot, horizontally, its perch holds Wolf up (m). Not traced:
/// the real game stands him on the object's own collision; this is about a branch tip's reach.
pub const PERCH_RADIUS: f32 = 0.6;

/// The ridden landing spot supplies floor only inside its [`PERCH_RADIUS`] footprint.
/// Everything else (walls, ceilings, water, triggers, fall return walls) is the terrain's,
/// through [`Ground::inner`].
pub struct PerchGround<'a> {
    pub terrain: &'a dyn Ground,
    pub perch: Option<Vec3>,
}

impl PerchGround<'_> {
    /// The perch's height when `(x, z)` is inside its footprint and it is no higher than `top`.
    fn perch_floor(&self, x: f32, z: f32, top: f32) -> Option<f32> {
        self.perch
            .filter(|p| (p.x - x).hypot(p.z - z) <= PERCH_RADIUS && p.y <= top)
            .map(|p| p.y)
    }

    /// The higher of a terrain floor and the perch under `(x, z)` no higher than `top`.
    fn with_perch(&self, terrain: Option<f32>, x: f32, z: f32, top: f32) -> Option<f32> {
        match (terrain, self.perch_floor(x, z, top)) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (a, b) => a.or(b),
        }
    }
}

impl Ground for PerchGround<'_> {
    fn inner(&self) -> Option<&dyn Ground> {
        Some(self.terrain)
    }
    fn height(&self, x: f32, z: f32) -> Option<f32> {
        self.floor(x, z, f32::INFINITY)
    }
    fn lowest_floor(&self, x: f32, z: f32) -> Option<f32> {
        match (
            self.terrain.lowest_floor(x, z),
            self.perch_floor(x, z, f32::INFINITY),
        ) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }
    fn floor(&self, x: f32, z: f32, top: f32) -> Option<f32> {
        self.with_perch(self.terrain.floor(x, z, top), x, z, top)
    }
    fn hug_floor(&self, x: f32, z: f32, top: f32) -> Option<f32> {
        self.with_perch(self.terrain.hug_floor(x, z, top), x, z, top)
    }
    fn floor_with_material(&self, x: f32, z: f32, top: f32) -> Option<(f32, Option<u32>)> {
        let terrain = self.terrain.floor_with_material(x, z, top);
        match self.perch_floor(x, z, top) {
            // The perch has no collision of its own, so no material: the terrain's under it.
            Some(p) if terrain.is_none_or(|(h, _)| p > h) => {
                Some((p, self.terrain.material(x, z, top)))
            }
            _ => terrain,
        }
    }
}
