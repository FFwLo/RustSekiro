//! The character's body in the world: position, facing, velocity and ground contact.
//!
//! World space is Bevy's (right-handed, Y up). A character with yaw 0 faces -Z, and positive yaw
//! turns it to the left (counter-clockwise seen from above). Animation root motion arrives in
//! the game's source space and is mirrored on X here, like the clips the renderer plays.
//!
//! Movement on the ground comes only from root motion; turning comes from root-motion yaw plus
//! steering toward the stick at the TAE turn speed. In the air, a velocity set by a TAE velocity
//! change (jumps) integrates under gravity. Ground is a [`Ground`]: a flat plane, or the map's
//! hit collision ([`crate::collision::CollisionWorld`]), which also blocks horizontal moves.

use crate::input::angle_diff;
use crate::tae::VelocityChange;
use glam::Vec2;

/// Mirror source X and rotate horizontal root motion by the facing's (sin yaw, cos yaw).
pub fn root_motion_world(root_motion: [f32; 4], (sin, cos): (f32, f32)) -> [f32; 2] {
    let [rx, _, rz, _] = root_motion;
    [-cos * rx + sin * rz, sin * rx + cos * rz]
}

/// Exe fall-control table values from FwLo's RustSekiro (MIT), `src/data.rs`.
#[derive(Debug, Clone, Copy)]
pub struct FallControl {
    pub horizontal_accel: f32,
    pub vertical_accel: f32,
    pub stick_accel: f32,
    pub stick_max: f32,
    pub steerable: bool,
}

pub const FALL_TYPES: [FallControl; 4] = [
    FallControl {
        horizontal_accel: -0.1,
        vertical_accel: -13.72,
        stick_accel: 10.0,
        stick_max: 2.0,
        steerable: true,
    },
    FallControl {
        horizontal_accel: -0.1,
        vertical_accel: -13.72,
        stick_accel: 10.0,
        stick_max: 2.0,
        steerable: false,
    },
    FallControl {
        horizontal_accel: -15.0,
        vertical_accel: -9.8,
        stick_accel: 9.0,
        stick_max: 6.0,
        steerable: true,
    },
    FallControl {
        horizontal_accel: -0.1,
        vertical_accel: -13.72,
        stick_accel: 10.0,
        stick_max: 2.0,
        steerable: true,
    },
];

/// How far a walkable floor can rise over a metre of horizontal travel: the tangent of the
/// steepest walkable slope ([`crate::collision::MAX_SLOPE_DEGREES`]), for telling one floor a
/// tick on from another ([`Body::carried_onto`]).
fn max_rise_per_metre() -> f32 {
    crate::collision::MAX_SLOPE_DEGREES.to_radians().tan()
}

/// Horizontal air integration described by FwLo's RustSekiro (MIT).
fn air_velocity(mut v: Vec2, stick: Vec2, control: FallControl, dt: f32) -> Vec2 {
    let decay = control.horizontal_accel.abs() * dt;
    if v.length() > decay {
        v -= v.normalize() * decay;
    }
    if control.steerable {
        let a = stick.clamp_length_max(1.0) * control.stick_accel * dt;
        let speed = v.length();
        if speed > 1e-6 {
            let direction = v / speed;
            let along = a.dot(direction);
            let across = a - direction * along;
            v += across
                + direction
                    * if along <= 0.0 {
                        along
                    } else {
                        along.min((control.stick_max - speed).max(0.0))
                    };
        } else {
            v += a.clamp_length_max((control.stick_max - speed).max(0.0));
        }
    }
    v
}

/// Downward acceleration in m/s²: Havok world gravity (9.8) plus the extra vertical
/// acceleration of the normal fall type (13.72) from the exe's fall-control table. Measured
/// in the running game as a constant 23.52 m/s² over five jumps (traced by FwLo's
/// RustSekiro, `docs/kb/movement.md`); a 9.64 m/s jump rises about 2 m and lands after 0.82 s.
pub const GRAVITY: f32 = 9.8 + 13.72;

/// How far past a floor's edge the capsule's rounded bottom still rests on it: the 0.4 m
/// radius ([`crate::collision::RADIUS`]) touches the edge with a contact normal within the
/// walkable slope (60 degrees, [`crate::collision::MAX_SLOPE_DEGREES`]) until the centre is
/// `r sin 60` = 0.35 m out. The game's cliff-top fall-prevention walls stand up to about 0.2 m
/// past the floor's end, so a point foot would drop before they hold it (#119).
const RIM_SUPPORT: f32 = crate::collision::RADIUS * 0.866;
/// How far above its lowest point the capsule's rounded bottom is at [`RIM_SUPPORT`] from its
/// axis: `r - r cos 60` = 0.2 m. A floor there higher than the feet meets the bottom's side
/// and holds the body up, as the crown of a round trunk holds Wolf while the walls keep his
/// axis past the trunk's walkable top (#155).
const RIM_LIFT: f32 = crate::collision::RADIUS * 0.5;

/// Authored water collision, separate from supporting terrain.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WaterSurface {
    pub height: f32,
    pub material: u32,
}

impl WaterSurface {
    pub fn deep(self) -> bool {
        deep_water(self.material)
    }
}

/// The HitMtrlParam row of a collision triangle's material id, which is its
/// HitMaterialSpecialSettingParam row times 100 plus its HitMtrlParam row
/// ([`crate::hit_filter::RIDEABLE_SPECIAL`]): the `l` binder's deep water is 22 and 722 alike.
/// `None` for a triangle without a material (`u32::MAX`).
pub const fn hit_mtrl_row(material: u32) -> Option<u32> {
    if material == u32::MAX {
        None
    } else {
        Some(material % 100)
    }
}

/// Whether hit material `material` is water ([`hit_mtrl_row`] 20 to 24 or 35 to 38): a plane
/// bodies swim or wade in, which [`deep_water`] narrows down.
pub const fn water(material: u32) -> bool {
    matches!(hit_mtrl_row(material), Some(20..=24 | 35..=38))
}

/// Whether hit material `material` is deep water: a surface bodies stand in, never a floor or
/// wall ([`crate::collision::CollisionWorld`] leaves its triangles out of both).
pub const fn deep_water(material: u32) -> bool {
    matches!(hit_mtrl_row(material), Some(22 | 36 | 38))
}

/// Something the character can stand on.
pub trait Ground {
    /// The ground this one wraps, if it is a wrapper. Every method a wrapper does not override
    /// forwards to it, so a wrapper only writes the queries it changes and can never silently
    /// drop one (#169). A wrapper that changes the floor ([`Ground::floor`]) must also override
    /// the rest of the floor family ([`Ground::height`], [`Ground::lowest_floor`],
    /// [`Ground::hug_floor`], [`Ground::floor_with_material`]), since those forward too.
    fn inner(&self) -> Option<&dyn Ground> {
        None
    }

    /// Water planes above or below the body; underwater floor materials are not surfaces.
    fn water_surface(&self, x: f32, z: f32, top: f32) -> Option<WaterSurface> {
        self.inner()?.water_surface(x, z, top)
    }
    /// Lowest walkable collision at this horizontal point, for the placeholder void boundary.
    fn lowest_floor(&self, x: f32, z: f32) -> Option<f32> {
        if let Some(inner) = self.inner() {
            return inner.lowest_floor(x, z);
        }
        let mut lowest = self.height(x, z)?;
        while let Some(next) = self.floor(x, z, lowest - 0.01) {
            if next >= lowest {
                break;
            }
            lowest = next;
        }
        Some(lowest)
    }
    /// Height of the walkable surface below `(x, z)`, if any.
    fn height(&self, x: f32, z: f32) -> Option<f32>;

    /// The first map action event wall a body's capsule meets moving from feet `from` to feet
    /// `to` (see [`crate::hit_filter`]); such walls are never floors or walls.
    fn trigger_contact(
        &self,
        from: [f32; 3],
        to: [f32; 3],
    ) -> Option<crate::collision::TriggerContact> {
        self.inner()?.trigger_contact(from, to)
    }

    /// Whether a capsule at `feet` is still within reach of the named event wall part.
    fn near_trigger(&self, part: &str, feet: [f32; 3]) -> bool {
        self.inner().is_some_and(|g| g.near_trigger(part, feet))
    }

    /// Whether an authored fall return wall (hit filter 15) lies below `(x, y, z)`.
    fn fall_return_below(&self, x: f32, y: f32, z: f32) -> bool {
        self.inner().is_some_and(|g| g.fall_return_below(x, y, z))
    }

    /// The walkable surface under `(x, z)` that is no higher than `top` (multi-level ground).
    fn floor(&self, x: f32, z: f32, top: f32) -> Option<f32> {
        match self.inner() {
            Some(inner) => inner.floor(x, z, top),
            None => self.height(x, z),
        }
    }

    /// [`Ground::floor`] for a body hugging a wall, which also stands on the wall-hug parts
    /// ([`crate::hit_filter::WALL_HUG`]). Grounds without them have just their floor.
    fn hug_floor(&self, x: f32, z: f32, top: f32) -> Option<f32> {
        match self.inner() {
            Some(inner) => inner.hug_floor(x, z, top),
            None => self.floor(x, z, top),
        }
    }

    /// The highest the feet of a body at `feet` can rise straight up, by at most `rise`, before
    /// its head meets a ceiling: below `feet[1] + rise` when `Some`, `None` when nothing stops
    /// it that soon. No ceilings by default.
    fn ceiling(&self, feet: [f32; 3], rise: f32) -> Option<f32> {
        self.inner()?.ceiling(feet, rise)
    }

    /// Whether the head sphere of a body at `feet` touches anything, so a push-out there has
    /// work to do (the gate on [`Body`]'s push-out after a rise). True by default, so a ground
    /// that does not know always gets the push-out.
    fn head_contact(&self, feet: [f32; 3]) -> bool {
        self.inner().is_none_or(|g| g.head_contact(feet))
    }

    /// The horizontal displacement a body at `feet` actually makes when it tries to move by
    /// `delta` (walls stop it and it slides along them). Unobstructed by default.
    fn slide(&self, feet: [f32; 3], delta: [f32; 2]) -> [f32; 2] {
        match self.inner() {
            Some(inner) => inner.slide(feet, delta),
            None => delta,
        }
    }

    /// [`Ground::slide`] of each of `deltas` from the same `feet`: the probe fan of
    /// [`crate::docking::wall_contact`]. A ground that can share the work between the slides
    /// overrides it, with each result still exactly that slide's (#167).
    fn slide_fan(&self, feet: [f32; 3], deltas: [[f32; 2]; 8]) -> [[f32; 2]; 8] {
        deltas.map(|d| self.slide(feet, d))
    }

    /// [`Ground::slide`] that also stops at the fall-prevention walls in `walls`
    /// ([`crate::hit_filter::FALL_ASSIST_MOVE`], `FALL_ASSIST_ACTION` bits), for a body whose
    /// script asks for the assist. Grounds without such walls just slide.
    fn slide_assisted(&self, feet: [f32; 3], delta: [f32; 2], walls: u8) -> [f32; 2] {
        match self.inner() {
            Some(inner) => inner.slide_assisted(feet, delta, walls),
            None => self.slide(feet, delta),
        }
    }

    /// Highest ledge a grounded body steps onto, and the furthest it follows the floor down
    /// before it starts falling.
    fn max_step(&self) -> f32 {
        self.inner().map_or(0.3, |g| g.max_step())
    }

    /// The hit material (HitMtrlParam row: 1 cobblestone, 4 wood, 16 tile, 28 tatami, ...) of
    /// the floor under `(x, z)` no higher than `top`, when the ground knows it. Footstep
    /// sounds are chosen by it.
    fn material(&self, x: f32, z: f32, top: f32) -> Option<u32> {
        self.inner()?.material(x, z, top)
    }

    /// [`Ground::floor`] and the [`Ground::material`] of that same surface, from one query, so
    /// the two cannot disagree.
    fn floor_with_material(&self, x: f32, z: f32, top: f32) -> Option<(f32, Option<u32>)> {
        match self.inner() {
            Some(inner) => inner.floor_with_material(x, z, top),
            None => self.floor(x, z, top).map(|h| (h, self.material(x, z, top))),
        }
    }
}

/// An infinite flat plane.
#[derive(Debug, Clone, Copy, Default)]
pub struct FlatGround {
    pub height: f32,
}

impl Ground for FlatGround {
    fn height(&self, _x: f32, _z: f32) -> Option<f32> {
        Some(self.height)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Body {
    /// EMEVD 2004[10] can suspend an invisible controller below a map without a fake floor.
    pub gravity_enabled: bool,
    /// Selected by the latest velocity change, reset on landing or walking off a ledge.
    pub fall_type: u8,
    pub position: [f32; 3],
    pub yaw: f32,
    pub velocity: [f32; 3],
    pub grounded: bool,
    /// Highest point of the current airborne phase.
    pub fall_top: f32,
    /// Height fallen on the last landing (env 224).
    pub last_fall_height: f32,
    /// Seconds left of the riding state (SpEffect 4100): reset to
    /// [`crate::hit_filter::SP_EFFECT_4100_ENDURANCE`] on each fall-assisted tick that starts
    /// on a rideable floor, counted down otherwise. While above 0 the rideable objects'
    /// fall-prevention walls hold the body ([`crate::hit_filter::riding_walls`]).
    pub riding: f32,
    /// The height of a floor over the feet of an airborne body that they came onto over its
    /// edge, from the side, and that it is still landing on ([`Body::landing_floor`]).
    pub carried_onto: Option<f32>,
}

impl Default for Body {
    fn default() -> Self {
        Self {
            gravity_enabled: true,
            fall_type: 0,
            position: [0.0; 3],
            yaw: 0.0,
            velocity: [0.0; 3],
            grounded: true,
            fall_top: 0.0,
            last_fall_height: 0.0,
            riding: 0.0,
            carried_onto: None,
        }
    }
}

/// What moves the body this tick.
#[derive(Debug, Clone, Default)]
pub struct BodyInput {
    /// Paired throws reserve space once, then keep authored relative root motion intact.
    pub ignore_walls: bool,
    /// HKS act 3037 = surface, act 3011 = dive. None uses ordinary physics.
    pub water_mode: Option<bool>,
    /// The fall-prevention walls this tick's HKS `act(3016)` asked for
    /// ([`crate::hit_filter::fall_assist_walls`]); 0 moves freely past them.
    pub fall_assist: u8,
    /// World-space horizontal movement stick, with magnitude at most one.
    pub stick: Vec2,
    /// Root-motion delta in the character's local source space: `[x, y, z, yaw]`.
    pub root_motion: [f32; 4],
    /// World yaw to steer toward, if the stick is pushed.
    pub steer_to: Option<f32>,
    /// Steering speed in degrees per second (0 disables steering).
    pub turn_speed: f32,
    /// A velocity change that starts this tick.
    pub velocity_change: Option<VelocityChange>,
}

impl Body {
    /// Unit forward vector for the current yaw.
    pub fn forward(&self) -> [f32; 2] {
        [-self.yaw.sin(), -self.yaw.cos()]
    }

    /// True while airborne and moving down.
    pub fn falling(&self) -> bool {
        !self.grounded && self.velocity[1] < 0.0
    }

    /// The feet height the capsule's rim holds the body at when no floor is under its centre,
    /// from the highest of eight points [`RIM_SUPPORT`] around the feet within a step of them.
    /// A floor no higher than the feet stands them on it. A higher one meets the rounded
    /// bottom's side ([`RIM_LIFT`]): it holds the feet where they are, or lifts them until the
    /// bottom rests on it. Before #155 only floors no higher than the feet counted, so at a
    /// round trunk's side, where the floor under the centre had dropped down the trunk's flank
    /// below its crown, the body fell although the walls still held it.
    fn rim_support(&self, ground: &dyn Ground) -> Option<f32> {
        let y = self.position[1];
        self.rim_floors(ground)
            .map(|(h, _)| h)
            .reduce(f32::max)
            .map(|h| if h <= y { h } else { y.max(h - RIM_LIFT) })
    }

    /// The feet height a step down to the floor `centre` under the body's centre ends at: no
    /// lower than where the capsule's rounded bottom still rests on the highest floor under the
    /// [`RIM_SUPPORT`] points ([`RIM_LIFT`] below it), and no higher than the feet. On a round
    /// trunk the floor under the centre runs down the trunk's flank while the bottom still rests
    /// on its crown, and the feet followed it below the walls' hold (#155). A slope gentler
    /// than 30 degrees or a stair (rim floors less than [`RIM_LIFT`] over the centre's) keeps its
    /// step down. Only a rideable object's floor holds it ([`crate::hit_filter::rideable`]), as
    /// in [`Self::rim_floors`]: off a map ledge the body steps down as before.
    fn rounded_step_down(&self, centre: f32, ground: &dyn Ground) -> f32 {
        let y = self.position[1];
        self.rim_floors(ground)
            .reduce(|a, b| if b.0 > a.0 { b } else { a })
            .filter(|&(_, m)| crate::hit_filter::rideable(m))
            .map_or(centre, |(h, _)| centre.max((h - RIM_LIFT).min(y)))
    }

    /// The floors under the [`RIM_SUPPORT`] points within a step below the feet, or within a
    /// step above them on a rideable object ([`crate::hit_filter::rideable`]), each with its
    /// material ([`Ground::floor_with_material`]). Only a rideable object's crown holds the
    /// rounded bottom from above the feet (#155): a higher map floor at the rim is a ledge whose
    /// edge is a wall, not a surface the body rests on.
    fn rim_floors<'a>(
        &self,
        ground: &'a dyn Ground,
    ) -> impl Iterator<Item = (f32, Option<u32>)> + 'a {
        let [x, y, z] = self.position;
        let step = ground.max_step();
        (0..8)
            .filter_map(move |i| {
                let (s, c) = (i as f32 * std::f32::consts::FRAC_PI_4).sin_cos();
                let (px, pz) = (x + RIM_SUPPORT * c, z + RIM_SUPPORT * s);
                ground
                    .floor_with_material(px, pz, y + step)
                    .filter(|&(h, m)| h <= y + 0.05 || crate::hit_filter::rideable(m))
                    .or_else(|| ground.floor_with_material(px, pz, y + 0.05))
            })
            .filter(move |(h, _)| y - h < step)
    }

    /// True while the surface holding the body up is rideable ([`crate::hit_filter::rideable`]):
    /// the floor under its centre when that is within a step, otherwise the highest rim floor,
    /// the one [`Self::rim_support`] stands it on. Held by the fall-prevention walls at a
    /// branch's edge, the centre is often past the branch while the rim still rests on it.
    fn on_rideable(&self, ground: &dyn Ground) -> bool {
        let [x, y, z] = self.position;
        ground
            .floor_with_material(x, z, y + 0.1)
            .filter(|(h, _)| y - h < ground.max_step())
            .or_else(|| {
                self.rim_floors(ground)
                    .reduce(|best, p| if p.0 > best.0 { p } else { best })
            })
            .is_some_and(|(_, m)| crate::hit_filter::rideable(m))
    }

    /// Stops a rise from the feet height `from` to the current one where the head meets a
    /// ceiling, and the upward speed with it, as the character proxy's contact does; otherwise
    /// a jump carries the head into a low roof and the fall lands on its underside, which reads
    /// as a floor (#160).
    ///
    /// A face the head meets off its top, at its shoulder, does not stop the rise: the body is
    /// pushed off it sideways at the height it rose to, by the slide's own push-out (a slide of
    /// nothing with the fall-prevention walls `walls`), as the proxy slides along such a contact.
    /// Before, the push waited for the next tick's slide, so a beam edge beside the head's centre
    /// line went up to a tick's rise into the head (#168). The push-out runs only when the head
    /// touches something ([`Ground::head_contact`]), so a jump in open air costs what it did
    /// before. `walls` is `None` for a move that
    /// ignores walls.
    fn stop_rise_at_ceiling(&mut self, from: f32, ground: &dyn Ground, walls: Option<u8>) {
        let rise = self.position[1] - from;
        if rise <= 0.0 {
            return;
        }
        if let Some(top) = ground.ceiling([self.position[0], from, self.position[2]], rise) {
            self.position[1] = top.max(from);
            self.velocity[1] = self.velocity[1].min(0.0);
        }
        if let Some(walls) = walls
            && ground.head_contact(self.position)
        {
            let [dx, dz] = ground.slide_assisted(self.position, [0.0, 0.0], walls);
            self.position[0] += dx;
            self.position[2] += dz;
        }
    }

    /// The floor an airborne body whose feet were at `start` when this tick began lands on at
    /// its feet now: the highest one no more than a step above `start`, unless that one is above
    /// the feet and was come at from below (#168). The character proxy's contact normals point
    /// from the surface to the capsule, so the underside of a roof is only ever a ceiling to it;
    /// the mesh's winding does not say which side of a face is solid, so here the side is the
    /// one the feet came from. A floor above the feet was come at from below when a floor was
    /// already over them, within a step, where this tick began, and they had not been carried
    /// onto it over its edge on an earlier tick ([`Body::carried_onto`]); otherwise they came
    /// over its edge this tick, as onto a stair tread or a ledge, and it still counts. The
    /// body then lands on the highest floor it passed through instead.
    fn landing_floor(&mut self, start: [f32; 3], ground: &dyn Ground) -> Option<f32> {
        // How far above the feet a floor must be to count as over them, against roundoff.
        const OVER: f32 = 1e-3;
        let [x, _, z] = self.position;
        let (from, top) = (start[1], start[1] + ground.max_step());
        let floor = ground.floor(x, z, top);
        let Some(h) = floor.filter(|&h| h > from + OVER) else {
            self.carried_onto = None;
            return floor;
        };
        // The same floor a tick on: within roundoff, or as far as a walkable slope rises over
        // this tick's travel.
        let travel = (x - start[0]).hypot(z - start[2]);
        let carried = self
            .carried_onto
            .is_some_and(|c| (c - h).abs() <= 0.01 + travel * max_rise_per_metre());
        let under = ground
            .floor(start[0], start[2], top)
            .is_some_and(|f| f > from + OVER);
        if under && !carried {
            self.carried_onto = None;
            ground.floor(x, z, from + OVER)
        } else {
            self.carried_onto = Some(h);
            floor
        }
    }

    pub fn step(&mut self, input: &BodyInput, dt: f32, ground: &dyn Ground) {
        let mut travel = root_motion_world(input.root_motion, self.yaw.sin_cos());
        self.yaw -= input.root_motion[3];

        if let Some(target) = input.steer_to
            && input.turn_speed > 0.0
        {
            let max = input.turn_speed.to_radians() * dt;
            self.yaw += angle_diff(target, self.yaw).clamp(-max, max);
        }
        self.yaw = angle_diff(self.yaw, 0.0);

        if let Some(v) = input.velocity_change {
            self.fall_type = v.fall_type;
            let [fx, fz] = self.forward();
            let a = v.horizontal_angle.to_radians();
            let (sa, ca) = a.sin_cos();
            let (dx, dz) = (fx * ca - fz * sa, fx * sa + fz * ca);
            let h = [
                self.velocity[0] * v.horizontal_scale + dx * v.horizontal,
                self.velocity[2] * v.horizontal_scale + dz * v.horizontal,
            ];
            self.velocity = [h[0], self.velocity[1] * v.vertical_scale + v.vertical, h[1]];
            if self.velocity[1] > 0.0 {
                self.grounded = false;
                self.fall_top = self.position[1];
            }
        }

        if !self.grounded {
            let control = FALL_TYPES
                .get(self.fall_type as usize)
                .copied()
                .unwrap_or(FALL_TYPES[0]);
            let v = air_velocity(
                Vec2::new(self.velocity[0], self.velocity[2]),
                input.stick,
                control,
                dt,
            );
            self.velocity[0] = v.x;
            self.velocity[2] = v.y;
            travel[0] += self.velocity[0] * dt;
            travel[1] += self.velocity[2] * dt;
        }
        // Only an assisted slide reads the riding state, so only its ticks look for it.
        let assisted = input.fall_assist != 0 && !input.ignore_walls;
        self.riding = if assisted && self.grounded && self.on_rideable(ground) {
            crate::hit_filter::SP_EFFECT_4100_ENDURANCE
        } else {
            (self.riding - dt).max(0.0)
        };
        // The fall-prevention walls this tick's moves keep to; `None` when they ignore walls.
        let walls = (!input.ignore_walls)
            .then(|| crate::hit_filter::riding_walls(input.fall_assist, self.riding > 0.0));
        // Where the feet were before this tick's move, for the landing (#168).
        let start = self.position;
        let [mx, mz] = match walls {
            None => travel,
            Some(walls) => ground.slide_assisted(self.position, travel, walls),
        };
        self.position[0] += mx;
        self.position[2] += mz;

        if let Some(diving) = input.water_mode
            && (self.grounded || diving)
            && let Some(water) =
                ground.water_surface(self.position[0], self.position[2], f32::INFINITY)
            && water.deep()
            && ground
                .floor(self.position[0], self.position[2], water.height)
                .is_none_or(|floor| {
                    floor <= self.position[1] + ground.max_step()
                        && floor < water.height - ground.max_step()
                })
        {
            self.velocity = [0.0; 3];
            self.grounded = true;
            if diving {
                // Vertical distance is authored root motion, including up/down clips.
                let floor = ground.floor(
                    self.position[0],
                    self.position[2],
                    self.position[1] + ground.max_step(),
                );
                let from = self.position[1];
                self.position[1] = (self.position[1] + input.root_motion[1]).min(water.height);
                self.stop_rise_at_ceiling(from, ground, walls);
                if let Some(floor) = floor {
                    self.position[1] = self.position[1].max(floor);
                }
            } else {
                self.position[1] = water.height;
            }
            return;
        }

        if !self.gravity_enabled {
            let from = self.position[1];
            self.position[1] += input.root_motion[1] + self.velocity[1] * dt;
            self.stop_rise_at_ceiling(from, ground, walls);
            return;
        }
        let step = ground.max_step();
        if self.grounded {
            // Follow the floor up or down by a step, plus what a 50 degree slope rises over
            // this tick's travel, so a fast tick on a ramp or stairs keeps contact.
            let reach = step + (mx * mx + mz * mz).sqrt() * 1.2;
            let floor = match ground
                .floor(self.position[0], self.position[2], self.position[1] + reach)
                .filter(|h| self.position[1] - h < reach)
            {
                Some(h) if h < self.position[1] => Some(self.rounded_step_down(h, ground)),
                Some(h) => Some(h),
                None => self.rim_support(ground),
            };
            match floor {
                Some(h) => self.position[1] = h,
                None => {
                    self.fall_type = 0;
                    self.grounded = false;
                    self.fall_top = self.position[1];
                }
            }
            if self.grounded {
                return;
            }
        }
        // Exact constant-acceleration step, so the arc does not depend on the tick rate.
        let previous_y = self.position[1];
        let gravity = 9.8
            + FALL_TYPES
                .get(self.fall_type as usize)
                .unwrap_or(&FALL_TYPES[0])
                .vertical_accel
                .abs();
        self.position[1] += self.velocity[1] * dt - 0.5 * gravity * dt * dt;
        self.velocity[1] -= gravity * dt;
        self.stop_rise_at_ceiling(previous_y, ground, walls);
        self.fall_top = self.fall_top.max(self.position[1]);
        // Land on the highest floor the feet passed through this tick, or one up to a step
        // above them (a stair tread or ledge the body was carried into while airborne).
        let floor = self.landing_floor(start, ground);
        let water = ground
            .water_surface(self.position[0], self.position[2], previous_y + step)
            .filter(|w| w.deep() && floor.is_none_or(|h| h < w.height - ground.max_step()))
            .map(|w| w.height);
        let floor = match (floor, water) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (a, b) => a.or(b),
        };
        if let Some(h) = floor
            && self.velocity[1] <= 0.0
            && self.position[1] <= h
        {
            self.position[1] = h;
            self.velocity = [0.0; 3];
            self.grounded = true;
            self.fall_type = 0;
            self.last_fall_height = self.fall_top - h;
            self.carried_onto = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn material_rows_drop_the_special_setting() {
        assert_eq!(hit_mtrl_row(8), Some(8));
        assert_eq!(hit_mtrl_row(808), Some(8));
        assert_eq!(hit_mtrl_row(u32::MAX), None);
        assert!(deep_water(722) && water(721) && !deep_water(721));
        assert!(!water(u32::MAX) && !deep_water(u32::MAX));
    }

    fn close(actual: Vec2, expected: Vec2) {
        assert!(
            (actual - expected).length() < 1e-5,
            "{actual:?} != {expected:?}"
        );
    }

    #[test]
    fn air_decay_does_not_flip_small_velocity() {
        close(
            air_velocity(Vec2::new(3.0, 4.0), Vec2::ZERO, FALL_TYPES[0], 1.0),
            Vec2::new(2.94, 3.92),
        );
        close(
            air_velocity(Vec2::new(0.05, 0.0), Vec2::ZERO, FALL_TYPES[0], 1.0),
            Vec2::new(0.05, 0.0),
        );
    }

    #[test]
    fn grounded_stick_does_not_add_air_velocity() {
        let mut b = Body::default();
        b.step(
            &BodyInput {
                stick: Vec2::X,
                root_motion: [0.0, 0.0, -1.0, 0.0],
                ..Default::default()
            },
            0.1,
            &FlatGround::default(),
        );
        assert_eq!(b.position, [0.0, 0.0, -1.0]);
        assert_eq!(b.velocity, [0.0; 3]);
    }

    #[test]
    fn air_braking_and_across_steer_apply_above_cap() {
        close(
            air_velocity(Vec2::new(6.0, 0.0), -Vec2::X, FALL_TYPES[0], 0.1),
            Vec2::new(4.99, 0.0),
        );
        close(
            air_velocity(Vec2::new(6.0, 0.0), Vec2::Y, FALL_TYPES[0], 0.1),
            Vec2::new(5.99, 1.0),
        );
        close(
            air_velocity(Vec2::new(6.0, 0.0), Vec2::Y, FALL_TYPES[1], 0.1),
            Vec2::new(5.99, 0.0),
        );
    }

    #[test]
    fn air_along_acceleration_caps_only_speed_gain() {
        close(
            air_velocity(Vec2::new(1.9, 0.0), Vec2::X, FALL_TYPES[0], 0.1),
            Vec2::new(2.0, 0.0),
        );
        close(
            air_velocity(Vec2::new(6.0, 0.0), Vec2::X, FALL_TYPES[0], 0.1),
            Vec2::new(5.99, 0.0),
        );
        close(
            air_velocity(Vec2::ZERO, Vec2::X, FALL_TYPES[0], 1.0),
            Vec2::new(2.0, 0.0),
        );
        close(
            air_velocity(Vec2::new(8.0, 0.0), Vec2::X, FALL_TYPES[2], 0.1),
            Vec2::new(6.5, 0.0),
        );
    }

    #[test]
    fn selected_fall_type_persists_until_landing() {
        let mut b = Body {
            position: [0.0, 10.0, 0.0],
            grounded: false,
            fall_top: 10.0,
            ..Default::default()
        };
        let input = BodyInput {
            stick: Vec2::X,
            velocity_change: Some(VelocityChange {
                fall_type: 2,
                horizontal_scale: 0.0,
                vertical_scale: 0.0,
                horizontal: 0.0,
                vertical: 0.0,
                horizontal_angle: 0.0,
            }),
            ..Default::default()
        };
        let ground = FlatGround::default();
        b.step(&input, 0.1, &ground);
        assert!((b.velocity[1] + 1.96).abs() < 1e-5);
        b.step(&BodyInput::default(), 0.1, &ground);
        assert_eq!(b.fall_type, 2);
        assert!((b.velocity[1] + 3.92).abs() < 1e-5);
        while !b.grounded {
            b.step(&BodyInput::default(), 0.1, &ground);
        }
        assert_eq!(b.fall_type, 0);
    }

    #[test]
    fn root_motion_moves_forward_and_jump_lands() {
        let ground = FlatGround::default();
        let mut b = Body::default();
        // One metre of forward root motion (source -Z) at yaw 0 moves toward -Z.
        b.step(
            &BodyInput {
                root_motion: [0.0, 0.0, -1.0, 0.0],
                ..BodyInput::default()
            },
            1.0 / 30.0,
            &ground,
        );
        assert!((b.position[2] + 1.0).abs() < 1e-5);

        let jump = VelocityChange {
            fall_type: 0,
            horizontal_scale: 0.0,
            vertical_scale: 0.0,
            horizontal: 6.4,
            vertical: 9.64,
            horizontal_angle: 0.0,
        };
        b.step(
            &BodyInput {
                velocity_change: Some(jump),
                ..BodyInput::default()
            },
            1.0 / 60.0,
            &ground,
        );
        assert!(!b.grounded);
        let mut frames = 1;
        while !b.grounded && frames < 600 {
            b.step(&BodyInput::default(), 1.0 / 60.0, &ground);
            frames += 1;
        }
        let airtime = frames as f32 / 60.0;
        // The running game: 0.82 s in the air and a 1.98 m rise for this launch.
        assert!((0.8..0.86).contains(&airtime), "airtime {airtime}");
        assert!(
            (1.94..2.02).contains(&b.last_fall_height),
            "rise {}",
            b.last_fall_height
        );
        assert!(b.position[2] < -5.0, "jumped forward: {:?}", b.position);
    }

    #[test]
    fn steering_is_rate_limited() {
        let ground = FlatGround::default();
        let mut b = Body::default();
        b.step(
            &BodyInput {
                steer_to: Some(std::f32::consts::FRAC_PI_2),
                turn_speed: 90.0,
                ..BodyInput::default()
            },
            0.5,
            &ground,
        );
        assert!((b.yaw - std::f32::consts::FRAC_PI_4).abs() < 1e-5);
    }
}
