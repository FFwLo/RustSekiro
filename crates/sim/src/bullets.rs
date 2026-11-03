//! Bullets: thrown and shot attacks (the Owl's shuriken, firecrackers, poison balls, smoke screen
//! and recovery-sealing powder), as the data describes them.
//!
//! The data trail, per bullet:
//!
//! 1. A TAE `BulletBehavior` event (type 2) on the shooter's clip names a behaviour judge and
//!    the dummy poly it leaves from.
//! 2. `BehaviorParam` row `200000000 + behaviorVariationId * 1000 + judge` with `refType` 1
//!    names a `Bullet` param row.
//! 3. The Bullet row gives the flight: `initVellocity`, `accelInRange` / `accelOutRange` and
//!    `gravityInRange` / `gravityOutRange` (in and out of `dist`, "the distance where decay
//!    starts", def), `minVellocity` / `maxVellocity`, `life` seconds, `hitRadius` growing to
//!    `hitRadiusMax` over `spreadTime`, how many leave at once (`numShoot`) and their fan
//!    (`shootAngle` + `shootAngleInterval` per bullet horizontally, `shootAngleXZ` +
//!    `shootAngleXInterval` per bullet in elevation, positive up), `atkId_Bullet` (the
//!    AtkParam_Npc row its hits deal), `spEffectId0..4` (applied to whoever it hits),
//!    `isPenetrate` / `isPenetrateMap` (keeps flying through characters / the map),
//!    `dmgHitRecordLifeTime` (seconds before the same target can be hit again; 0 means once),
//!    `isUseSharedHitList` (bullets of one shot share that record) and `damageDamp`,
//!    `staminaDamp`, `knockbackDamp` (percent lost per second past `dist`, def).
//! 4. Child bullets: `HitBulletID` starts where the bullet ends, on a character, on the map or
//!    when its life runs out (`launchConditionType` "whether a bullet is made on impact or at the
//!    end of life", 0 on all of the Owl's rows); `intervalCreateBulletId` starts every
//!    `intervalCreateTimeMin` seconds after `intervalCreateWaitTime` while the bullet lives. The
//!    Owl's firecracker is a thrown parent (190), a lit firecracker on the floor (195, 0.8 s),
//!    then the bang (200: 1.8 m, AtkParam 50600160) with two flame bursts (201).
//! 5. An AtkParam row with `opposeTarget` 0 does not target hostile characters: AtkParam 1,
//!    "[System] dummy bullets (do not hit characters, hit maps)", is the attack of every carrier
//!    and of the explosive slash's flames, which are only looks.
//!
//! Ours (not traced): bullets aim at the target's chest when it is within `lockShootLimitAng` of
//! the shooter's facing (FwLo's RustSekiro, `docs/kb/enemy.md`), else along the facing, and the
//! elevation angles add to that aim; children leave along the parent's travel direction; the
//! random spreads (`shootAngleYMaxRandom`, `lifeRandomRange`, ...) are left out so runs repeat;
//! speed is clamped to `minVellocity..maxVellocity` only while an acceleration acts; the map stops
//! a bullet only at the floor under it (walls are not tested); the decay is linear; hits do not
//! freeze the shooter.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use glam::Vec3;

use crate::body::Ground;
use crate::character::Character;
use crate::hits::{AttackData, Combatant, HitEvent, can_be_hit, segment_distance};
use crate::tae::TaeKind;

/// The deepest chain of child bullets followed (the Owl's go three deep).
const MAX_CHILD_DEPTH: usize = 4;

/// What a bullet looks like to the player: a reading of what it does, so the renderer can draw a
/// simple stand-in (the real looks are FXR effects, not extracted).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BulletLook {
    /// Lightning, including the castle Tomoe bow's blue-lightning bolt.
    Lightning,
    /// Does nothing visible to the player (perilous-attack warning dummies, effect dummies).
    Hidden,
    /// A flying attack that deals damage (shuriken).
    Missile,
    /// Thrown or placed, deals nothing itself, makes child bullets: a firecracker in flight or
    /// lit on the floor, a poison ball, the smoke and powder throws.
    Carrier,
    /// A damaging area: the firecracker's bang.
    Blast,
    /// An area that hits nobody: the flames of a firecracker or of the explosive slash.
    Flame,
    /// An area that builds up poison.
    Poison,
    /// An area that blinds (AtkParam `atkBehaviorId` 17).
    Smoke,
    /// An area that leaves a timed status (the recovery seal).
    Seal,
}

/// A child bullet made at intervals.
#[derive(Debug, Clone, PartialEq)]
pub struct IntervalChild {
    pub def: Arc<BulletDef>,
    /// Seconds before the first one.
    pub wait: f32,
    /// Seconds between them.
    pub every: f32,
}

/// One Bullet param row, with the attack its hits deal and its children.
#[derive(Debug, Clone, PartialEq)]
pub struct BulletDef {
    pub id: i32,
    /// The attack its hits deal; `None` when it does not hit characters.
    pub attack: Option<AttackData>,
    /// Metres per second.
    pub speed: f32,
    pub min_speed: f32,
    pub max_speed: f32,
    /// Acceleration along the flight within and past [`BulletDef::range`], m/s².
    pub accel: [f32; 2],
    /// Downward acceleration within and past [`BulletDef::range`], m/s².
    pub gravity: [f32; 2],
    /// `dist`: where the out-of-range values and the decay start; 0 for never.
    pub range: f32,
    /// Seconds.
    pub life: f32,
    pub hit_radius: f32,
    /// Radius reached after `spread_time` seconds (equal to `hit_radius` when it does not grow).
    pub hit_radius_max: f32,
    pub spread_time: f32,
    pub count: i32,
    /// Horizontal angle of the first bullet and the step between bullets, degrees.
    pub first_angle: f32,
    pub angle_step: f32,
    /// Elevation of the first bullet and the step between bullets, degrees (positive up).
    pub first_pitch: f32,
    pub pitch_step: f32,
    /// The bullet aims at the target when it is within this many degrees of the facing.
    pub aim_limit: f32,
    /// Keeps flying after hitting a character.
    pub penetrate: bool,
    /// Flies through the map.
    pub penetrate_map: bool,
    /// Bullets of the same shot share one hit record.
    pub shared_hits: bool,
    /// Seconds before the same target can be hit again; 0 means once per bullet.
    pub rehit: f32,
    /// Percent per second lost past `range`: HP damage, posture damage, knockback.
    pub damp: [f32; 3],
    /// SpEffects applied to whoever it hits.
    pub sp_effects: Vec<i32>,
    /// `HitBulletID`, made where this bullet ends.
    pub child: Option<Arc<BulletDef>>,
    /// `intervalCreateBulletId`.
    pub interval: Option<IntervalChild>,
    pub look: BulletLook,
    /// `EmittePosType` 7: made at the target instead of at the shooter's dummy poly. Only the
    /// perilous-attack warning dummies use it (10001600, 50601980 to 50601987). That 7 means
    /// "at the target" is inferred: those rows also set `generationMaxDistWhenOwner`, which only
    /// makes sense for a bullet made away from its owner, and the warning effect lands on Wolf.
    pub emit_at_target: bool,
    /// `generationMaxDistWhenOwner`: not made when where it would be made is farther than this
    /// from the shooter, metres (def); 0 for no limit. 5 m for the soldier's warning, 9 m for the
    /// Owl's.
    pub max_owner_dist: f32,
}

/// `EmittePosType` of a bullet made at its target ([`BulletDef::emit_at_target`]).
const EMIT_AT_TARGET: i32 = 7;

/// What a SpEffect does, as far as a bullet's look is concerned.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct EffectKind {
    pub poison: bool,
    /// Lasts a while and changes the target's state (`stateInfo`), like the recovery seal.
    pub timed_state: bool,
}

impl BulletDef {
    /// Whether its hits deal HP or posture damage.
    pub fn damages(&self) -> bool {
        self.attack.as_ref().is_some_and(attack_damages)
    }

    /// The hit radius at `age` seconds.
    pub fn radius_at(&self, age: f32) -> f32 {
        if self.spread_time > 0.0 && self.hit_radius_max > self.hit_radius {
            let t = (age / self.spread_time).clamp(0.0, 1.0);
            self.hit_radius + (self.hit_radius_max - self.hit_radius) * t
        } else {
            self.hit_radius
        }
    }

    /// Whether this bullet, or any of its children, ever hits a character.
    pub fn hits_anything(&self) -> bool {
        self.attack.is_some()
            || self.child.as_ref().is_some_and(|c| c.hits_anything())
            || self
                .interval
                .as_ref()
                .is_some_and(|c| c.def.hits_anything())
    }

    fn choose_look(&mut self, effects: &dyn Fn(i32) -> EffectKind) {
        let moving = self.speed > 0.0 || self.gravity.iter().any(|g| *g > 0.0);
        let area = !moving && self.hit_radius_max.max(self.hit_radius) >= 1.0;
        let kinds: Vec<EffectKind> = self.sp_effects.iter().map(|&id| effects(id)).collect();
        let hits = self.attack.is_some();
        let blinds = self
            .attack
            .as_ref()
            .and_then(|a| a.profile.as_ref())
            .is_some_and(|p| p.behavior_id != 0);
        let has_children = self.child.is_some() || self.interval.is_some();
        self.look = if self
            .attack
            .as_ref()
            .and_then(|a| a.profile.as_ref())
            .is_some_and(|p| matches!(p.sp_attribute, 6 | 10))
        {
            BulletLook::Lightning
        } else if self.damages() {
            if moving {
                BulletLook::Missile
            } else {
                BulletLook::Blast
            }
        } else if hits && blinds {
            BulletLook::Smoke
        } else if hits && kinds.iter().any(|k| k.poison) {
            BulletLook::Poison
        } else if hits && kinds.iter().any(|k| k.timed_state) {
            BulletLook::Seal
        } else if area && !hits {
            BulletLook::Flame
        } else if has_children {
            BulletLook::Carrier
        } else {
            BulletLook::Hidden
        };
    }
}

pub(crate) fn attack_damages(a: &AttackData) -> bool {
    a.atk_phys > 0
        || a.atk_stam > 0
        || a.profile.as_ref().is_some_and(|p| {
            p.attack_power.iter().any(|v| *v > 0.0) || p.direct_stam > 0 || p.blocked_stam > 0
        })
}

/// Reads one Bullet row and its children. `atk` finds an AtkParam row by id; `behavior` is the
/// judge's BehaviorParam row, whose `category` and `stamina` every bullet of the chain carries.
fn read_def<'t>(
    id: i32,
    bullets: &sekiro_formats::param::Table,
    atk: &dyn Fn(i32) -> Option<sekiro_formats::param::Row<'t>>,
    behavior: &sekiro_formats::param::Row<'_>,
    effects: &dyn Fn(i32) -> EffectKind,
    depth: usize,
) -> Option<BulletDef> {
    if depth > MAX_CHILD_DEPTH {
        return None;
    }
    let row = bullets.find(id)?;
    let f = |name: &str| row.f32(name).unwrap_or(0.0);
    let i = |name: &str| row.int(name).unwrap_or(0) as i32;
    let attack = atk(i("atkId_Bullet"))
        .filter(|a| a.int("opposeTarget").unwrap_or(1) != 0)
        .map(|a| AttackData::from_rows(&a, behavior));
    let child = |field: &str| {
        let cid = i(field);
        (cid > 0)
            .then(|| read_def(cid, bullets, atk, behavior, effects, depth + 1))
            .flatten()
            .map(Arc::new)
    };
    let interval = child("intervalCreateBulletId").map(|def| IntervalChild {
        def,
        wait: f("intervalCreateWaitTime").max(0.0),
        every: f("intervalCreateTimeMin").max(0.01),
    });
    let hit_radius = f("hitRadius").max(0.05);
    let mut def = BulletDef {
        id,
        attack,
        speed: f("initVellocity").max(0.0),
        min_speed: f("minVellocity").max(0.0),
        max_speed: f("maxVellocity"),
        accel: [f("accelInRange"), f("accelOutRange")],
        gravity: [f("gravityInRange"), f("gravityOutRange")],
        range: f("dist").max(0.0),
        life: f("life").max(0.0),
        hit_radius,
        hit_radius_max: f("hitRadiusMax").max(hit_radius),
        spread_time: f("spreadTime").max(0.0),
        count: i("numShoot").max(1),
        first_angle: i("shootAngle") as f32,
        angle_step: i("shootAngleInterval") as f32,
        first_pitch: i("shootAngleXZ") as f32,
        pitch_step: i("shootAngleXInterval") as f32,
        aim_limit: i("lockShootLimitAng") as f32,
        penetrate: i("isPenetrate") != 0,
        penetrate_map: i("isPenetrateMap") != 0,
        shared_hits: i("isUseSharedHitList") != 0,
        rehit: f("dmgHitRecordLifeTime").max(0.0),
        damp: [
            i("damageDamp") as f32,
            i("staminaDamp") as f32,
            i("knockbackDamp") as f32,
        ],
        sp_effects: (0..5)
            .map(|k| i(&format!("spEffectId{k}")))
            .filter(|&e| e > 0)
            .collect(),
        child: child("HitBulletID"),
        interval,
        look: BulletLook::Hidden,
        emit_at_target: i("EmittePosType") == EMIT_AT_TARGET,
        max_owner_dist: f("generationMaxDistWhenOwner").max(0.0),
    };
    def.choose_look(effects);
    Some(def)
}

/// A bullet in flight.
#[derive(Debug, Clone, PartialEq)]
pub struct Bullet {
    pub source_action: Option<i32>,
    /// Static map-script owner, preserved by child and interval shots.
    pub event_owner: Option<i32>,
    /// Unique per bullet in this fight (from 1), so a renderer can follow it.
    pub serial: u64,
    pub shooter: usize,
    /// The behaviour judge of the shot it belongs to.
    pub judge: i32,
    pub position: Vec3,
    pub velocity: Vec3,
    /// Seconds left.
    pub life: f32,
    /// Seconds since it was made.
    pub age: f32,
    /// Current hit radius.
    pub radius: f32,
    pub def: Arc<BulletDef>,
    /// Horizontal direction it leaves along when it does not move (yaw).
    yaw: f32,
    travelled: f32,
    /// Seconds since it passed its range (damage decay).
    out_of_range: f32,
    /// The shot (one TAE bullet event) it belongs to, for shared hit records.
    shot: u64,
    /// Fight clock of its last hit on each character.
    hits: HashMap<usize, f32>,
    intervals_made: u32,
}

impl Bullet {
    pub fn look(&self) -> BulletLook {
        self.def.look
    }
}

/// A bullet's contact with a character.
#[derive(Debug, Clone, PartialEq)]
pub struct BulletHit {
    pub source_action: Option<i32>,
    pub event_owner: Option<i32>,
    pub hit: HitEvent,
    /// SpEffects it leaves on the character.
    pub sp_effects: Vec<i32>,
    /// The attack deals no damage, has no damage level and no behaviour identification: it only
    /// carries its SpEffects (the poison floor), so no hit reaction or guard plays (ours).
    pub effects_only: bool,
    /// The Bullet row that hit.
    pub bullet: i32,
}

/// Every bullet in flight, and which bullet windows already fired.
#[derive(Debug, Default)]
pub struct Bullets {
    pub live: Vec<Bullet>,
    /// (shooter, judge, window start) of bullet events already fired.
    fired: HashSet<(usize, i32, u32)>,
    /// Shared hit records: (shot, character) -> fight clock of the last hit.
    shared: HashMap<(u64, usize), f32>,
    next_shot: u64,
    next_serial: u64,
    clock: f32,
}

/// World yaw of a direction (the simulation's convention, yaw 0 facing -Z).
fn yaw_of(dir: Vec3) -> f32 {
    (-dir.x).atan2(-dir.z)
}

fn dir_of(yaw: f32, pitch: f32) -> Vec3 {
    Vec3::new(
        -yaw.sin() * pitch.cos(),
        pitch.sin(),
        -yaw.cos() * pitch.cos(),
    )
}

fn angle_diff(a: f32, b: f32) -> f32 {
    let d = (a - b).rem_euclid(std::f32::consts::TAU);
    if d > std::f32::consts::PI {
        d - std::f32::consts::TAU
    } else {
        d
    }
}

/// Makes the `count` bullets of `def` leaving `origin` along `(yaw, pitch)`.
fn launch(
    def: &Arc<BulletDef>,
    shooter: usize,
    judge: i32,
    shot: u64,
    origin: Vec3,
    yaw: f32,
    pitch: f32,
) -> Vec<Bullet> {
    (0..def.count)
        .map(|k| {
            let offset = (def.first_angle + def.angle_step * k as f32).to_radians();
            let elevation = (def.first_pitch + def.pitch_step * k as f32).to_radians();
            // Positive shoot angles turn to the shooter's right (ours).
            let byaw = yaw - offset;
            let dir = dir_of(byaw, pitch + elevation);
            Bullet {
                source_action: None,
                event_owner: None,
                serial: 0,
                shooter,
                judge,
                position: origin,
                velocity: dir * def.speed,
                life: def.life,
                age: 0.0,
                radius: def.radius_at(0.0),
                def: def.clone(),
                yaw: byaw,
                travelled: 0.0,
                out_of_range: 0.0,
                shot,
                hits: HashMap::new(),
                intervals_made: 0,
            }
        })
        .collect()
}

fn launch_children(def: &Arc<BulletDef>, parent: &Bullet, origin: Vec3) -> Vec<Bullet> {
    let mut children = launch(
        def,
        parent.shooter,
        parent.judge,
        parent.shot,
        origin,
        parent.yaw,
        0.0,
    );
    for child in &mut children {
        child.event_owner = parent.event_owner;
        child.source_action = parent.source_action;
    }
    children
}

/// `attack` scaled by a bullet's decay past its range.
fn decayed(attack: &AttackData, damp: [f32; 3], seconds: f32) -> AttackData {
    let mut a = attack.clone();
    if seconds <= 0.0 {
        return a;
    }
    let scale = |pct: f32| (1.0 - pct / 100.0 * seconds).max(0.0);
    let (hp, stam, kb) = (scale(damp[0]), scale(damp[1]), scale(damp[2]));
    a.atk_phys = (a.atk_phys as f32 * hp) as i32;
    a.atk_stam = (a.atk_stam as f32 * stam) as i32;
    if let Some(p) = a.profile.as_mut() {
        for v in &mut p.attack_power {
            *v *= hp;
        }
        for v in [
            &mut p.direct_stam,
            &mut p.blocked_stam,
            &mut p.deflected_stam,
        ] {
            *v = (*v as f32 * stam) as i16;
        }
        for v in &mut p.knockback_dist {
            *v *= kb;
        }
    }
    a
}

impl Bullets {
    /// Fire an authored map point without replacing its origin with AI aim or a dummy.
    pub fn spawn_event(
        &mut self,
        def: &Arc<BulletDef>,
        owner: i32,
        shooter: usize,
        judge: i32,
        origin: Vec3,
        yaw: f32,
    ) {
        let shot = self.next_shot;
        self.next_shot += 1;
        let mut made = launch(def, shooter, judge, shot, origin, yaw, 0.0);
        for bullet in &mut made {
            bullet.event_owner = Some(owner);
        }
        self.live.extend(made);
        self.number_new();
    }
    /// Fires the bullets whose TAE window opened this step on any character, aimed at the
    /// other character.
    pub fn spawn(&mut self, chars: &[&Character], fighters: &[&Combatant]) {
        let mut live_windows = HashSet::new();
        for (si, shooter) in chars.iter().enumerate() {
            for e in &shooter.tae_frame().attacks {
                let TaeKind::Bullet { judge, dummy_poly } = e.kind else {
                    continue;
                };
                let key = (si, judge, e.start.to_bits());
                live_windows.insert(key);
                if !self.fired.insert(key) {
                    continue;
                }
                let Some(def) = fighters[si].bullets.get(&judge) else {
                    continue;
                };
                let model = shooter.bone_source_matrices();
                let to_world = shooter.source_to_world();
                let body = Vec3::from_array(shooter.body.position);
                let origin = i16::try_from(dummy_poly)
                    .ok()
                    .and_then(|d| {
                        fighters[si]
                            .rig
                            .positions(d, &model, to_world)
                            .first()
                            .copied()
                    })
                    .unwrap_or(body + Vec3::Y * 1.2);
                let facing = shooter.body.yaw;
                // Aim at the first other character's chest when it is in front.
                let target = chars
                    .iter()
                    .enumerate()
                    .find(|(ti, _)| *ti != si)
                    .map(|(ti, t)| {
                        Vec3::from_array(t.body.position) + Vec3::Y * fighters[ti].height * 0.6
                    });
                let origin = match target {
                    Some(t) if def.emit_at_target => t,
                    _ => origin,
                };
                if def.max_owner_dist > 0.0 && origin.distance(body) > def.max_owner_dist {
                    continue;
                }
                let (yaw, pitch) = match target {
                    Some(t) if def.aim_limit > 0.0 => {
                        let to = t - origin;
                        let yaw = yaw_of(to);
                        let flat = (to.x * to.x + to.z * to.z).sqrt();
                        if angle_diff(yaw, facing).abs().to_degrees() <= def.aim_limit {
                            (yaw, to.y.atan2(flat))
                        } else {
                            (facing, 0.0)
                        }
                    }
                    _ => (facing, 0.0),
                };
                let shot = self.next_shot;
                self.next_shot += 1;
                let source = shooter
                    .behavior
                    .runtime
                    .main_clip()
                    .and_then(|c| crate::lua_ai::anim_id_from_clip(&c.animation));
                let mut made = launch(def, si, judge, shot, origin, yaw, pitch);
                for bullet in &mut made {
                    bullet.source_action = source;
                }
                self.live.extend(made);
            }
        }
        self.fired.retain(|k| live_windows.contains(k));
        self.number_new();
    }

    /// Gives every bullet made since the last call its serial.
    fn number_new(&mut self) {
        for b in self.live.iter_mut().filter(|b| b.serial == 0) {
            self.next_serial += 1;
            b.serial = self.next_serial;
        }
    }

    /// Moves every bullet by `dt` seconds, makes child bullets, and returns the contacts with
    /// characters other than each bullet's shooter. `ground` stops bullets at the floor.
    pub fn advance(
        &mut self,
        dt: f32,
        chars: &[&Character],
        fighters: &[&Combatant],
        ground: &dyn Ground,
    ) -> Vec<BulletHit> {
        self.clock += dt;
        let clock = self.clock;
        let shared = &mut self.shared;
        let mut hits = Vec::new();
        let mut born = Vec::new();
        self.live.retain_mut(|b| {
            let def = b.def.clone();
            let from = b.position;
            // Flight.
            let in_range = def.range <= 0.0 || b.travelled <= def.range;
            let r = usize::from(!in_range);
            if def.accel[r] != 0.0 {
                let mut speed = (b.velocity.length() + def.accel[r] * dt).max(def.min_speed);
                if def.max_speed > 0.0 {
                    speed = speed.min(def.max_speed);
                }
                b.velocity = b.velocity.normalize_or_zero() * speed.max(0.0);
            }
            b.velocity.y -= def.gravity[r] * dt;
            let mut to = from + b.velocity * dt;
            b.travelled += (to - from).length();
            if !in_range {
                b.out_of_range += dt;
            }
            b.age += dt;
            b.life -= dt;
            b.radius = def.radius_at(b.age);
            if b.velocity.x.abs() + b.velocity.z.abs() > 1e-3 {
                b.yaw = yaw_of(b.velocity);
            }
            // The floor.
            let mut ended = false;
            if !def.penetrate_map
                && b.velocity.y < 0.0
                && let Some(floor) = ground.floor(to.x, to.z, from.y.max(to.y) + 0.05)
                && to.y <= floor
            {
                to.y = floor;
                ended = true;
            }
            b.position = to;
            // Characters.
            if let Some(attack) = def.attack.as_ref() {
                for (di, defender) in chars.iter().enumerate() {
                    if di == b.shooter || !can_be_hit(defender.tae_frame()) {
                        continue;
                    }
                    let f = fighters[di];
                    let base = Vec3::from_array(defender.body.position);
                    let axis = (
                        base + Vec3::Y * f.radius,
                        base + Vec3::Y * (f.height - f.radius).max(f.radius),
                    );
                    if segment_distance(from, to, axis.0, axis.1) > b.radius + f.radius {
                        continue;
                    }
                    let last = if def.shared_hits {
                        shared.get(&(b.shot, di)).copied()
                    } else {
                        b.hits.get(&di).copied()
                    };
                    let allowed = match last {
                        None => true,
                        Some(t) => def.rehit > 0.0 && clock - t >= def.rehit - 1e-4,
                    };
                    if !allowed {
                        continue;
                    }
                    if def.shared_hits {
                        shared.insert((b.shot, di), clock);
                    }
                    b.hits.insert(di, clock);
                    // Direction: along the flight, or out from the centre of a still area.
                    let travel = Vec3::new(b.velocity.x, 0.0, b.velocity.z);
                    let flat = if travel.length() > 0.5 {
                        travel.normalize()
                    } else {
                        Vec3::new(base.x - to.x, 0.0, base.z - to.z).normalize_or(Vec3::Z)
                    };
                    let attack = decayed(attack, def.damp, b.out_of_range);
                    let effects_only = !attack_damages(&attack)
                        && attack.damage_level <= 0
                        && attack.profile.as_ref().is_none_or(|p| p.behavior_id == 0);
                    hits.push(BulletHit {
                        source_action: b.source_action,
                        event_owner: b.event_owner,
                        hit: HitEvent {
                            attacker: b.shooter,
                            defender: di,
                            judge: b.judge,
                            attack,
                            position: to.to_array(),
                            direction: [flat.x, flat.z],
                        },
                        sp_effects: def.sp_effects.clone(),
                        effects_only,
                        bullet: def.id,
                    });
                    if !def.penetrate {
                        ended = true;
                    }
                }
            }
            // Children made at intervals while it lives.
            if let Some(iv) = &def.interval
                && b.age >= iv.wait
            {
                let due = ((b.age - iv.wait) / iv.every).floor() as u32 + 1;
                while b.intervals_made < due {
                    born.extend(launch_children(&iv.def, b, to));
                    b.intervals_made += 1;
                }
            }
            if ended || b.life <= 0.0 {
                if let Some(child) = &def.child {
                    // Children start no lower than the floor under them.
                    let mut at = to;
                    if let Some(floor) = ground.floor(at.x, at.z, at.y + 0.5) {
                        at.y = at.y.max(floor);
                    }
                    born.extend(launch_children(child, b, at));
                }
                return false;
            }
            true
        });
        self.live.extend(born);
        self.number_new();
        let shots: HashSet<u64> = self.live.iter().map(|b| b.shot).collect();
        self.shared.retain(|(shot, _), _| shots.contains(shot));
        hits
    }

    pub fn clear(&mut self) {
        self.live.clear();
        self.fired.clear();
        self.shared.clear();
    }
}

/// Bullet definitions of an NPC's behaviour variation by judge id, with their children.
/// `effects` says which SpEffects build up poison or leave a timed state (for the looks).
pub fn load_bullets(
    behaviors: &sekiro_formats::param::Table,
    bullets: &sekiro_formats::param::Table,
    attacks: &sekiro_formats::param::Table,
    effects: &dyn Fn(i32) -> EffectKind,
    base: i32,
    variation: i32,
) -> HashMap<i32, Arc<BulletDef>> {
    let mut out = HashMap::new();
    let first = base + variation * 1000;
    let atk = |id: i32| (id > 0).then(|| attacks.find(id)).flatten();
    for row in behaviors.rows() {
        let id = row.id();
        if !(first..first + 1000).contains(&id) || row.int("refType").unwrap_or(-1) != 1 {
            continue;
        }
        if let Some(def) = read_def(
            row.int("refId").unwrap_or(-1) as i32,
            bullets,
            &atk,
            &row,
            effects,
            0,
        ) {
            out.insert(id - first, Arc::new(def));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directions_follow_the_yaw_convention() {
        // Yaw 0 faces -Z; yaw of that direction is 0 again.
        let d = dir_of(0.0, 0.0);
        assert!((d - Vec3::NEG_Z).length() < 1e-6);
        assert!((yaw_of(dir_of(1.0, 0.0)) - 1.0).abs() < 1e-5);
        assert!((angle_diff(0.1, std::f32::consts::TAU - 0.1) - 0.2).abs() < 1e-5);
        // Positive elevation is up.
        assert!(dir_of(0.0, 0.5).y > 0.0);
    }

    fn def(speed: f32, life: f32) -> BulletDef {
        BulletDef {
            id: 1,
            attack: None,
            speed,
            min_speed: 0.0,
            max_speed: speed,
            accel: [0.0; 2],
            gravity: [0.0; 2],
            range: 0.0,
            life,
            hit_radius: 0.25,
            hit_radius_max: 0.25,
            spread_time: 0.0,
            count: 1,
            first_angle: 0.0,
            angle_step: 0.0,
            first_pitch: 0.0,
            pitch_step: 0.0,
            aim_limit: 0.0,
            penetrate: false,
            penetrate_map: false,
            shared_hits: false,
            rehit: 0.0,
            damp: [0.0; 3],
            sp_effects: Vec::new(),
            child: None,
            interval: None,
            look: BulletLook::Hidden,
            emit_at_target: false,
            max_owner_dist: 0.0,
        }
    }

    #[test]
    fn radius_spreads_over_its_time() {
        let mut d = def(0.0, 1.7);
        d.hit_radius = 0.1;
        d.hit_radius_max = 3.2;
        d.spread_time = 1.0;
        assert!((d.radius_at(0.0) - 0.1).abs() < 1e-6);
        assert!((d.radius_at(0.5) - 1.65).abs() < 1e-5);
        assert!((d.radius_at(2.0) - 3.2).abs() < 1e-6);
    }

    #[test]
    fn a_child_starts_where_its_parent_ends() {
        let mut parent = def(10.0, 0.2);
        parent.penetrate_map = true;
        let mut fuse = def(0.0, 0.5);
        fuse.hit_radius = 0.5;
        parent.child = Some(Arc::new(fuse));
        let parent = Arc::new(parent);
        let mut bullets = Bullets::default();
        bullets.live.extend(launch(
            &parent,
            1,
            190,
            0,
            Vec3::new(0.0, 1.0, 0.0),
            0.0,
            0.0,
        ));
        let ground = crate::body::FlatGround::default();
        for _ in 0..13 {
            bullets.advance(1.0 / 60.0, &[], &[], &ground);
        }
        // 0.2 s at 10 m/s along -Z, then a still fuse.
        assert_eq!(bullets.live.len(), 1);
        let fuse = &bullets.live[0];
        assert_eq!(fuse.def.speed, 0.0);
        assert!((fuse.position.z + 2.0).abs() < 0.2, "{:?}", fuse.position);
        for _ in 0..40 {
            bullets.advance(1.0 / 60.0, &[], &[], &ground);
        }
        assert!(bullets.live.is_empty());
    }

    #[test]
    fn the_floor_stops_a_falling_bullet() {
        let mut ball = def(5.0, 3.0);
        ball.gravity = [9.8, 9.8];
        let ball = Arc::new(ball);
        let mut bullets = Bullets::default();
        bullets
            .live
            .extend(launch(&ball, 1, 700, 0, Vec3::new(0.0, 1.5, 0.0), 0.0, 0.0));
        let ground = crate::body::FlatGround::default();
        let mut steps = 0;
        while !bullets.live.is_empty() && steps < 300 {
            bullets.advance(1.0 / 60.0, &[], &[], &ground);
            steps += 1;
        }
        // About sqrt(2 * 1.5 / 9.8) = 0.55 s to fall 1.5 m.
        assert!((30..40).contains(&steps), "{steps}");
    }

    #[test]
    fn shooting_past_the_range_slows_and_falls() {
        let mut s = def(40.0, 1.0);
        s.range = 5.0;
        s.accel = [0.0, -40.0];
        s.gravity = [0.0, 2.5];
        s.max_speed = 40.0;
        let s = Arc::new(s);
        let mut bullets = Bullets::default();
        bullets
            .live
            .extend(launch(&s, 1, 150, 0, Vec3::new(0.0, 1.5, 0.0), 0.0, 0.0));
        let ground = crate::body::FlatGround::default();
        for _ in 0..30 {
            bullets.advance(1.0 / 60.0, &[], &[], &ground);
        }
        let b = &bullets.live[0];
        assert!(b.velocity.length() < 35.0, "{:?}", b.velocity);
        assert!(b.velocity.y < 0.0);
        assert!(b.out_of_range > 0.3);
    }
}
