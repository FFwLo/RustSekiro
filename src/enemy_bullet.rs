//! NPC bullets: an enemy anim's TAE BulletBehavior (type 2) fires its Bullet row (BehaviorParam
//! 200000000 + behaviorVariationId * 1000 + judge, refType 1 -> enemy.bullets "<judge>": the
//! bandits' arrows 3040, Shichimen's spirit balls, the riflemen of the valley). It flies like the
//! player's prosthetic bullets (prosthetic.rs: initVellocity, accelOutRange / gravityOutRange past
//! `dist`, life, numShoot fans, HitBulletID / intervalCreateBulletId children) and, touching Wolf,
//! queues its AtkParam on the shooter (Actor::bullet_hits) so combat.rs resolves it like a
//! connected AttackBehavior: dodge frames, deflect, guard, damage.
//! The boss scripts' Shoot Bullet (2003[5], enemy/script.rs) fires its rows the same way from a
//! character's dummy poly (ScriptShots). A bullet touching Wolf also puts its spEffectId0-4 on
//! him (the Divine Dragon's updraft 106100, its lightning signs 3531061-5), harmless or not, and
//! one whose AtkParam is lightning (spAttribute 6 / 10, the dragon's 52000555) comes at him as an
//! attack for the Lightning Reversal (combat.rs). A life of -1 lasts (the signs 53100681).
//! Flight as the exe's bullet update (FUN_140b0ded0): homing (`steer`, FUN_140b11270), then
//! acceleration, gravity and the speed limits (`flight`, FUN_140b14650), then the move; the map
//! collision stops a bullet unless isPenetrateMap.
//! Past `dist` its hits lose damageDamp / staminaDamp / knockbackDamp percent per second
//! (Attack::decayed, after sekiro-rs bullets.rs).
//! FollowType 3 (follow the target: the Divine Dragon's 52000980) rides on Wolf.
//! gap: EmittePosType and FollowType 1 / 2 (the shooter; no enemy row; the exe takes
//! FUN_140b14b20 for odd FollowTypes or 2) are not modelled; nor the force erase (isHitOtherBulletForceEraseA /
//! B: which bullets erase is not in the params), so the signs stay until hit once.

use std::sync::Arc;

use bevy::prelude::*;

use crate::actor::{Actor, ActorSet};
use crate::anim::Hurtboxes;
use crate::combat::{BODY_RADIUS, segment_distance};
use crate::data::{BulletSpec, Combat};
use crate::enemy::Enemy;
use crate::enemy::script::ScriptDef;
use crate::model::Dummies;
use crate::player::{CAPSULE_HALF_HEIGHT, Player};

#[derive(Component)]
pub struct EnemyBullet {
    spec: BulletSpec,
    pub(crate) shooter: Entity,
    /// The shooter's enemy kind (its Bullet rows).
    kind: usize,
    vel: Vec3,
    /// The way it was fired (a bullet fired at speed 0 speeds up along it).
    facing: Vec3,
    travelled: f32,
    /// Seconds flown past `dist` (the damp decay).
    out_of_range: f32,
    age: f32,
    next_interval: f32,
    /// Height of the floor it lands on (Wolf's feet when it was fired).
    floor: f32,
    pub(crate) hit: bool,
    /// Its age when it last hit Wolf (dmgHitRecordLifeTime).
    last_hit: f32,
    /// Fired by a boss script: its rows.
    script: Option<Arc<ScriptDef>>,
    /// The shot it belongs to: a fan's pellets and their children share it (isUseSharedHitList).
    chain: u32,
    /// Its flying effect (sfxId_Bullet, or the parent's with isInheritSfxToChild), riding on it.
    fx: Option<Entity>,
    /// The effect its children inherit (-1: none).
    inherit_sfx: i64,
    /// Its hit went to combat.rs with its effects (Attack::bullet_fx).
    fx_handed: bool,
    /// A growing bullet's next attack re-issue (age, s) and whether its last one went.
    reissue_at: f32,
    reissued: bool,
}

impl EnemyBullet {
    /// Its Bullet param row (the boss audit, sim_tests boss_audit).
    pub(crate) fn row(&self) -> i64 {
        self.spec.bullet_id
    }
}

/// A growing bullet's attack re-issue period: the countdown at bullet+0x20 is reset to
/// 0x3e2aaaab (1/6 s) after each (140b0.c:8412 / 8631).
const REISSUE_PERIOD: f32 = 1.0 / 6.0;

/// A boss script's Shoot Bullet this frame: its first Bullet row, the owner (whose attack it is)
/// and where it leaves from (`source`'s dummy poly).
pub struct ScriptShot {
    pub def: Arc<ScriptDef>,
    pub row: i64,
    pub owner: Entity,
    pub source: Entity,
    pub dmy: i16,
    /// A region source's arena position (the shot leaves from there, not `source`).
    pub at: Option<Vec3>,
}

#[derive(Resource, Default)]
pub struct ScriptShots(pub Vec<ScriptShot>);

pub struct EnemyBulletPlugin;

impl Plugin for EnemyBulletPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ScriptShots>()
            .init_resource::<TargetTrack>()
            .add_message::<crate::gore::PlaceStain>()
            .add_systems(FixedUpdate, (track, fire, fire_worn, fire_scripted, fly).chain().after(ActorSet::Advance).after(crate::anim::PoseSet).before(ActorSet::Resolve))
            .add_systems(Update, draw.run_if(resource_exists::<GizmoConfigStore>));
    }
}

/// A bullet that can hurt: an AtkParam with damage or posture damage (the gong's 60 m "sound"
/// bullets and the system dummies have none), or a reaction of its own (atkBehaviorId: the
/// Divine Dragon's 0-damage blast 52000820 / wind 52000830; round 39).
/// SpEffectVfx 48100 (SpEffect 211000): initSfxId 600035 at initDmyId 220.
const PERILOUS_SFX: i64 = 600035;
const PERILOUS_DMY: i16 = 220;

fn harmful(spec: &BulletSpec) -> bool {
    spec.attack.as_ref().is_some_and(|a| a.oppose_target != 0 && (a.npc_damage() > 0.0 || a.atk_stam > 0.0 || a.direct_atk_stam_damage > 0.0 || matches!(a.sp_attribute, 6 | 10) || a.atk_behavior_id != 0))
}

/// The Bullet row's SpEffects for Wolf (not the Lightning Reversal charges 9490-9495: the
/// catch / ground shock in combat.rs takes those from the hit, see `charge`).
fn sp_effects(spec: &BulletSpec) -> impl Iterator<Item = i64> {
    [spec.sp_effect_id0, spec.sp_effect_id1, spec.sp_effect_id2, spec.sp_effect_id3, spec.sp_effect_id4].into_iter().filter(|id| *id > 0 && !(9490..=9495).contains(id))
}

/// The Bullet row's lightning charge (SpEffect 9490-9495). A lightning arrow (c7110 71100521:
/// spEffectId1 9492, its AtkParam carries none; a000_003039 / 003047 / 003048, c7100 71000520)
/// charges and shocks Wolf through this row alone, so it rides on the hit's slot 0 for combat.rs.
fn charge(spec: &BulletSpec) -> Option<i64> {
    [spec.sp_effect_id0, spec.sp_effect_id1, spec.sp_effect_id2, spec.sp_effect_id3, spec.sp_effect_id4].into_iter().find(|id| (9490..=9495).contains(id))
}

/// Wolf's recent moves for predictive shots: the exe keeps a 240-entry ring of {delta, dt} on
/// each character (FUN_1405ac4e0 writes it; FUN_1405ac1b0, 1405a.c:9189, averages it).
#[derive(Resource, Default)]
pub(crate) struct TargetTrack {
    last: Option<Vec3>,
    ring: std::collections::VecDeque<(Vec3, f32)>,
}

impl TargetTrack {
    pub(crate) fn push(&mut self, pos: Vec3, dt: f32) {
        if let Some(last) = self.last {
            self.ring.push_front((pos - last, dt));
            self.ring.truncate(240);
        }
        self.last = Some(pos);
    }

    /// The average velocity over the last `window` seconds: sum(delta) / sum(dt).
    pub(crate) fn velocity(&self, window: f32) -> Vec3 {
        let (mut d, mut t) = (Vec3::ZERO, 0.0);
        for &(delta, dt) in &self.ring {
            if t >= window {
                break;
            }
            d += delta;
            t += dt;
        }
        if t > 0.0 { d / t } else { Vec3::ZERO }
    }
}

fn track(time: Res<Time>, player: Query<&Transform, With<Player>>, mut track: ResMut<TargetTrack>) {
    if let Ok(tf) = player.single() {
        track.push(tf.translation, time.delta_secs());
    }
}

/// The predictive aim (FUN_140b05b80, 140b0.c:3144): the target walks on at its average velocity
/// in 1/30 s steps while a bullet flies its speed profile (initVellocity, accelIn / OutRange
/// past `dist`, min / max), until the bullet has flown as far as the moved target is. No
/// gravity; the plain target when the speed or the velocity is about zero.
pub fn predict(spec: &BulletSpec, from: Vec3, target: Vec3, vel: Vec3) -> Vec3 {
    const STEP: f32 = 1.0 / 30.0;
    if vel.length_squared() < 1e-6 || spec.init_vellocity.max(spec.min_vellocity) <= 1e-4 {
        return target;
    }
    let (mut at, mut speed, mut flown) = (target, spec.init_vellocity, 0.0f32);
    for _ in 0..(4.0 / STEP) as usize {
        if flown >= at.distance(from) {
            break;
        }
        at += vel * STEP;
        speed += if flown > spec.dist { spec.accel_out_range } else { spec.accel_in_range } * STEP;
        speed = if speed < spec.min_vellocity { spec.min_vellocity } else { speed.min(spec.max_vellocity) };
        flown += speed * STEP;
    }
    at
}

fn fire(
    mut commands: Commands,
    combat: Res<Combat>,
    track: Res<TargetTrack>,
    enemies: Query<(Entity, &Actor, &Transform, &Enemy, Option<&Dummies>)>,
    player: Query<&Transform, With<Player>>,
    markers: Query<&GlobalTransform>,
) {
    let target = player.single().ok().map(|t| t.translation);
    for (ent, a, tf, e, dummies) in &enemies {
        let d = combat.data_of(a);
        if a.anim.is_empty() || a.t == a.prev_t || e.is_dead() {
            continue;
        }
        let Some(anim) = d.anim(&a.anim) else { continue };
        // Gated by its live SpEffects (combat::event_gates: Lady Butterfly's 3509200 -> 184).
        let gates = crate::combat::event_gates(&combat, a, &e.clash);
        let crosses = |ev: &crate::data::Event| ((ev.start > a.prev_t && ev.start <= a.t) || (a.prev_t == 0.0 && ev.start == 0.0)) && d.fires_on(ev, gates.as_deref());
        for ev in anim.events.iter().filter(|ev| ev.kind == 2 && crosses(ev)) {
            // Shifted by its worn SpEffects (data::shift_judge: the Nightjars' 3145040, 700 -> 701).
            let judge = crate::data::shift_judge(ev.arg_i64("BehaviorJudgeID").unwrap_or(-1), &crate::combat::worn_sp(a, &e.clash));
            let Some(spec) = d.bullets.get(&judge.to_string()) else { continue };
            let dmy = ev.arg_i64("DummyPolyID").unwrap_or(-1) as i16;
            let from = dummies
                .and_then(|dm| dm.0.get(&dmy))
                .and_then(|&m| markers.get(m).ok())
                .map(|g| g.translation())
                .unwrap_or(tf.translation + a.forward() * 0.5 + Vec3::Y * 0.6);
            // Aimed at Wolf's chest when he is within lockShootLimitAng of the facing; a row with
            // predictionShootObserveTime leads him by his average velocity over that window
            // (FUN_140b0acf0), and that aim too must lie within the angle.
            let mut dir = a.forward();
            if let Some(t) = target {
                let mut aim = t + Vec3::Y * 0.3;
                let window = crate::data::prediction_observe_time(spec.bullet_id);
                if window > 0.0 {
                    aim = predict(spec, from, aim, track.velocity(window));
                }
                let to = (aim - from).normalize_or_zero();
                if to.with_y(0.0).normalize_or_zero().angle_between(a.forward()).to_degrees() <= spec.lock_shoot_limit_ang.max(1.0) {
                    dir = to;
                }
            }
            let floor = target.map_or(tf.translation.y - CAPSULE_HALF_HEIGHT, |t| t.y - CAPSULE_HALF_HEIGHT);
            spawn_row(&mut commands, spec, ent, a.kind, from, dir, floor, None);
        }
    }
}

/// SpEffect stateInfo 275: a worn effect runs its behaviorId every motionInterval at its dmypolyId.
/// The Demon of Hatred's resident 3702080 / 3702081 (right / left foot, dummies 13 / 16, every
/// 0.3 s) -> BehaviorParam 1802 -> Bullet 70200950 (radius 2.5 m, life 0.3 s, no damage,
/// spEffectId0 3702085 "near the feet" on whom it touches); his AI (702000_battle.lua) answers
/// 3702085 on Wolf with Act19's stomp 3046. The live fight creates 70200950 ~6 times a second
/// for the whole fight (rec_20261016_072651_boss_demon_of_hatred, 6056 in 952 s).
/// gap: when the first one fires after the effect is put on is not traced (at once here).
#[allow(clippy::type_complexity)]
fn fire_worn(
    mut commands: Commands,
    time: Res<Time>,
    combat: Res<Combat>,
    enemies: Query<(Entity, &Actor, &Transform, &Enemy, Option<&Dummies>)>,
    player: Query<&Transform, With<Player>>,
    markers: Query<&GlobalTransform>,
    mut clocks: Local<std::collections::HashMap<(Entity, i64), f32>>,
) {
    const PERIODIC_BEHAVIOR: i64 = 275;
    let dt = time.delta_secs();
    let floor = player.single().ok().map(|t| t.translation.y - CAPSULE_HALF_HEIGHT);
    let mut seen = std::collections::HashSet::new();
    for (ent, a, tf, e, dummies) in &enemies {
        if e.is_dead() || e.disabled {
            continue;
        }
        let d = combat.data_of(a);
        for &id in &a.resident {
            let Some(sp) = d.sp_effects.get(&id.to_string()) else { continue };
            if sp.state_info != PERIODIC_BEHAVIOR || sp.behavior_id <= 0 || sp.motion_interval <= 0.0 {
                continue;
            }
            let Some(spec) = d.bullets.get(&format!("sp{}", sp.behavior_id)) else { continue };
            seen.insert((ent, id));
            let clock = clocks.entry((ent, id)).or_insert(0.0);
            *clock -= dt;
            if *clock > 0.0 {
                continue;
            }
            *clock += sp.motion_interval;
            let from = dummies
                .and_then(|dm| dm.0.get(&(sp.dmypoly_id as i16)))
                .and_then(|&m| markers.get(m).ok())
                .map(|g| g.translation())
                .unwrap_or(tf.translation);
            spawn_row(&mut commands, spec, ent, a.kind, from, a.forward(), floor.unwrap_or(tf.translation.y - CAPSULE_HALF_HEIGHT), None);
        }
    }
    clocks.retain(|k, _| seen.contains(k));
}

/// The boss scripts' Shoot Bullet: from the source's dummy poly (its root + 1 m without a model),
/// along its facing.
fn fire_scripted(
    mut commands: Commands,
    mut shots: ResMut<ScriptShots>,
    actors: Query<(&Actor, &Transform, Option<&Dummies>)>,
    player: Query<&Transform, With<Player>>,
    markers: Query<&GlobalTransform>,
) {
    let floor = player.single().ok().map(|t| t.translation.y - CAPSULE_HALF_HEIGHT);
    for shot in shots.0.drain(..) {
        let (Ok((oa, otf, _)), Ok((sa, stf, dummies))) = (actors.get(shot.owner), actors.get(shot.source)) else { continue };
        let Some(spec) = shot.def.bullet_rows.get(&shot.row.to_string()) else { continue };
        let from = shot.at.unwrap_or_else(|| {
            dummies.and_then(|dm| dm.0.get(&shot.dmy)).and_then(|&m| markers.get(m).ok()).map(|g| g.translation()).unwrap_or(stf.translation + Vec3::Y)
        });
        let floor = floor.unwrap_or(otf.translation.y);
        spawn_row(&mut commands, spec, shot.owner, oa.kind, from, sa.forward(), floor, Some(shot.def.clone()));
    }
}

/// One Bullet row at `pos`: numShoot bullets fanned by shootAngle + i * shootAngleInterval (yaw)
/// and tilted by shootAngleXZ, as prosthetic::spawn_row.
#[allow(clippy::too_many_arguments)]
pub(crate) fn spawn_row(commands: &mut Commands, spec: &BulletSpec, shooter: Entity, kind: usize, pos: Vec3, dir: Vec3, floor: f32, script: Option<Arc<ScriptDef>>) {
    static CHAIN: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);
    let chain = CHAIN.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    spawn_chain(commands, spec, shooter, kind, pos, dir, floor, script, chain, -1);
}

/// [`spawn_row`] in an existing shot's chain (a child bullet).
#[allow(clippy::too_many_arguments)]
fn spawn_chain(commands: &mut Commands, spec: &BulletSpec, shooter: Entity, kind: usize, pos: Vec3, dir: Vec3, floor: f32, script: Option<Arc<ScriptDef>>, chain: u32, inherited: i64) {
    let flat = dir.with_y(0.0).normalize_or(Vec3::NEG_Z);
    // The game's Bullet effect fields, as prosthetic::spawn_row: its own sfxId_Bullet, else the
    // parent's when that one passes it on (isInheritSfxToChild).
    let sfx = if spec.sfx_id_bullet > 0 { spec.sfx_id_bullet } else { inherited };
    let inherit_sfx = if spec.is_inherit_sfx_to_child != 0 { sfx } else { -1 };
    for i in 0..spec.num_shoot.max(1) {
        let yaw = (spec.shoot_angle + i as f32 * spec.shoot_angle_interval).to_radians();
        let mut d = if spec.shoot_angle == 0.0 && spec.shoot_angle_interval == 0.0 { dir } else { Quat::from_rotation_y(-yaw) * flat };
        if spec.shoot_angle_xz != 0.0 {
            let right = d.cross(Vec3::Y).normalize_or(Vec3::X);
            d = Quat::from_axis_angle(right, spec.shoot_angle_xz.to_radians()) * d;
        }
        // With its GlobalTransform from the start: the effect riding on it reads it this frame.
        let at = Transform::from_translation(pos).with_rotation(Quat::from_rotation_arc(Vec3::NEG_Z, d.normalize_or(Vec3::NEG_Z)));
        let bullet = commands.spawn((at, GlobalTransform::from(at))).id();
        let fx = (sfx > 0).then(|| commands.spawn(crate::fxr::FxEffect::new(sfx, Some(crate::fxr::Anchor::Entity(bullet)), true, at)).id());
        commands.entity(bullet).insert((
            EnemyBullet {
                spec: spec.clone(),
                shooter,
                kind,
                vel: d * spec.init_vellocity,
                facing: d,
                travelled: 0.0,
                out_of_range: 0.0,
                age: 0.0,
                reissue_at: REISSUE_PERIOD,
                reissued: false,
                next_interval: spec.interval_create_wait_time,
                floor,
                hit: false,
                last_hit: 0.0,
                script: script.clone(),
                chain,
                fx,
                inherit_sfx,
                fx_handed: false,
            },
            Name::new(format!("Enemy bullet {}", spec.bullet_id)),
        ));
    }
}

#[allow(clippy::type_complexity)]
fn fly(
    mut commands: Commands,
    time: Res<Time>,
    combat: Res<Combat>,
    mut bullets: Query<(Entity, &mut EnemyBullet, &mut Transform), (Without<Actor>, Without<Player>)>,
    mut player: Query<(Entity, &Transform, Option<&Hurtboxes>, &mut Player, &Actor)>,
    mut shooters: Query<&mut Actor, (With<Enemy>, Without<Player>)>,
    markers: Query<&GlobalTransform>,
    mut stains: MessageWriter<crate::gore::PlaceStain>,
    (mut statuses, config, mut perilous): (ResMut<crate::status::Statuses>, Res<crate::config::GameConfig>, Option<ResMut<crate::combat::Perilous>>),
    (mut fxr, wolf_dummies, mut effects): (Option<ResMut<crate::fxr::FxrLib>>, Query<&crate::model::Dummies, With<Player>>, Query<&mut crate::fxr::FxEffect>),
    // Each shot's hit on Wolf until when (its dmgHitRecordLifeTime; none: for good).
    (mut shared_hit, mut clock): (Local<std::collections::HashMap<u32, f32>>, Local<f32>),
    mut ai_sounds: Option<ResMut<crate::enemy::AiSounds>>,
) {
    let dt = time.delta_secs();
    *clock += dt;
    let now = *clock;
    let mut wolf = player.single_mut().ok();
    for (be, mut b, mut btf) in &mut bullets {
        let script = b.script.clone();
        let rows = script.as_ref().map_or(&combat.kind(b.kind).data.bullet_rows, |s| &s.bullet_rows);
        let row = |id: i64| if id > 0 { rows.get(&id.to_string()) } else { None };
        // AtkParam AppearAiSoundId: a bullet issues its own attack as it starts (FUN_140b0fc60 ->
        // FUN_1409a1290's bullet branch, 1409a.c:2243-2276): at the bullet, owned by its shooter.
        // A growing one (spreadTime > 0, hitRadiusMax set and not hitRadius) re-issues it every
        // 1/6 s while it grows and once more when it has (FUN_140b20b70 from 140b0.c:8408 /
        // 8627), each with the sound again.
        let growing = b.spec.spread_time > 0.0 && b.spec.hit_radius_max >= 0.0 && b.spec.hit_radius_max != b.spec.hit_radius;
        let reissue = growing && !b.reissued && b.age > 0.0 && (b.age > b.spec.spread_time || b.age >= b.reissue_at);
        if reissue {
            if b.age > b.spec.spread_time {
                b.reissued = true;
            } else {
                b.reissue_at += REISSUE_PERIOD;
            }
        }
        if b.age == 0.0 || reissue {
            if let (Some(s), Some(atk)) = (ai_sounds.as_mut(), b.spec.attack.as_ref()) {
                let team = shooters.get(b.shooter).ok().map(|a| crate::combat::team_of(&combat, a));
                s.emit(&combat, crate::data::atk_ai_sounds(false, atk.atk_param_id).0, btf.translation, Some(b.shooter), team);
            }
        }
        b.age += dt;
        let aim = wolf.as_ref().map(|w| w.1.translation + Vec3::Y * 0.3);
        // FollowType 3 "follow target": it rides on Wolf (his chest, where `fire` aims).
        // gap: EmittePosType 6 "above and behind the target" has no traced offset.
        let follow = aim.filter(|_| b.spec.follow_type == 3);
        if let Some(at) = follow {
            btf.translation = at;
        }
        let from = btf.translation;
        b.vel = steer(&b.spec, b.vel, b.travelled, from, aim, dt);
        b.vel = flight(&b.spec, b.vel, b.facing, b.travelled, dt);
        let mut to = if follow.is_some() { from } else { from + b.vel * dt };
        // The map stops it (isPenetrateMap: not); without one, the floor it was fired over.
        let mut landed = false;
        let step = to - from;
        if b.spec.is_penetrate_map == 0 && step.length() > 1e-5 {
            if let Some(t) = crate::terrain::terrain() {
                if let Some(d) = t.raycast(from, step.normalize(), step.length()) {
                    to = from + step.normalize() * d;
                    landed = true;
                }
            } else if to.y <= b.floor + 0.02 && b.vel.y < 0.0 {
                to.y = b.floor + 0.02;
                landed = true;
            }
        }
        b.travelled += (to - from).length();
        if b.travelled > b.spec.dist {
            b.out_of_range += dt;
        }
        btf.translation = to;
        // A bullet standing still (a speed-0 carrier) hands on the way it was fired, not world -Z.
        let dir = b.vel.normalize_or(b.facing);
        if let Some(child) = row(b.spec.interval_create_bullet_id) {
            while b.age >= b.next_interval && b.spec.interval_create_time_min > 0.0 {
                b.next_interval += b.spec.interval_create_time_min;
                spawn_chain(&mut commands, child, b.shooter, b.kind, to, dir, b.floor, script.clone(), b.chain, b.inherit_sfx);
            }
        }
        let expired = b.spec.life >= 0.0 && b.age > b.spec.life + 1e-4;
        let mut ended = landed || expired;
        // isUseSharedHitList: one hit on Wolf per shot (a fan's pellets, its children), as
        // prosthetic.rs SharedHits; the others fly through him. Till the hit record runs out.
        let shared_done = b.spec.is_use_shared_hit_list != 0 && shared_hit.get(&b.chain).is_some_and(|until| now < *until);
        // A penetrating bullet's hit record runs out after dmgHitRecordLifeTime (c1030's fire
        // 10300240: life 5, record 0.5): it may hit again (round 29; gap: inferred from the
        // values, no exe trace).
        let recorded = b.hit && !(b.spec.dmg_hit_record_life_time > 0.0 && b.age - b.last_hit >= b.spec.dmg_hit_record_life_time - 1e-4);
        // A carrier with no damage and no SpEffects still touches Wolf when it targets him
        // (opposeTarget) or carries a HitBulletID: c1100's poison spit 11000020 ends on him and
        // its child 11000021 (spEffect 9011) comes there, not where the spit lands (round 29).
        let touches_wolf = harmful(&b.spec)
            || sp_effects(&b.spec).next().is_some()
            || b.spec.hit_bullet_id > 0
            || b.spec.attack.as_ref().is_some_and(|a| a.oppose_target != 0);
        if !recorded && !shared_done && touches_wolf {
            if let Some((we, wtf, hurt, p, wa)) = wolf.as_mut() {
                let (we, wtf, hurt, wa) = (*we, *wtf, *hurt, *wa);
                // hitRadius growing to hitRadiusMax over spreadTime (c1250 bullet 100: 0.2 -> 1.2 over
                // 0.8 s; gap: the param names only, linear).
                let r = if b.spec.hit_radius_max > b.spec.hit_radius && b.spec.spread_time > 0.0 {
                    b.spec.hit_radius + (b.spec.hit_radius_max - b.spec.hit_radius) * (b.age / b.spec.spread_time).min(1.0)
                } else {
                    b.spec.hit_radius
                };
                let touches = match hurt {
                    Some(h) => h.0.iter().any(|&(ma, mb, hr)| match (markers.get(ma), markers.get(mb)) {
                        (Ok(pa), Ok(pb)) => segment_distance(from, to, pa.translation(), pb.translation()) <= r + hr,
                        _ => false,
                    }),
                    None => {
                        let feet = wtf.translation - Vec3::Y * CAPSULE_HALF_HEIGHT;
                        segment_distance(from, to, feet + Vec3::Y * 0.3, feet + Vec3::Y * 1.5) <= r + BODY_RADIUS
                    }
                };
                // Wolf's iframes (TAE 950 / 951 / 954, combat.rs dodged) let it pass through, before
                // anything lands (2026-10-12 bug hunt round 10: a dodged bullet still poisoned or
                // burned him and was used up). His corpse in the death loop the same: its poison /
                // burn build-up and timed SpEffects landed after the loop's 8002-8009 cure
                // (2026-10-13 bug hunt round 23).
                let dodged = touches && (wa.hp <= 0.0 || crate::combat::dodged(&combat, wa, None, b.spec.attack.as_ref().unwrap_or(&crate::data::Attack::default())));
                if touches && !dodged {
                    b.hit = true;
                    b.last_hit = b.age;
                    // The perilous warning is a dummy bullet (Bullet <chr>980 / 982 / 983: 2 m, 0.1 s,
                    // SpEffect 211000) that has to reach Wolf: the kanji goes up on its hit, not for
                    // any enemy anywhere (round 31). 211000's SpEffectVfx 48100 "Kanji 危" plays
                    // initSfxId 600035 once on his dummy 220: the kanji is that effect, its time on
                    // screen the effect's own. The HUD text stands in only without the effect.
                    // Any warning bullet (<chr>980-989 all carry 211000; Isshin's 986 / 987, the
                    // Way of Tomoe's 984-989 incl. the lightning 988): the kanji is 211000's, not the
                    // id suffix's (the suffix only names the kind for the HUD stand-in).
                    let warn = b.spec.bullet_id % 1000;
                    if sp_effects(&b.spec).any(|id| id == 211000) {
                        let fx = fxr.as_mut().is_some_and(|l| l.has(PERILOUS_SFX));
                        if fx {
                            let at = wolf_dummies.get(we).ok().and_then(|d| d.0.get(&PERILOUS_DMY).copied()).unwrap_or(we);
                            commands.spawn(crate::fxr::FxEffect::new(PERILOUS_SFX, Some(crate::fxr::Anchor::Entity(at)), true, Transform::IDENTITY));
                        }
                        if let Some(p) = perilous.as_mut() {
                            p.raise(warn, !fx);
                        }
                    }
                    if b.spec.is_use_shared_hit_list != 0 {
                        let record = b.spec.dmg_hit_record_life_time;
                        shared_hit.insert(b.chain, if record > 0.0 { now + record - 1e-4 } else { f32::INFINITY });
                    }
                    if harmful(&b.spec) {
                        if let (Ok(mut sa), Some(mut atk)) = (shooters.get_mut(b.shooter), b.spec.attack.as_ref().map(|a| a.decayed([b.spec.damage_damp, b.spec.stamina_damp, b.spec.knockback_damp], b.out_of_range))) {
                            // Its hit / repel effect comes with the outcome (combat.rs).
                            atk.bullet_fx = Some((b.spec.sfx_id_hit, b.spec.sfx_id_flick, to, dir));
                            // A bullet's hit takes its SpEffect slots from the Bullet row alone (applied
                            // below); its AtkParam's spEffectId0-4 are never read for it (exe
                            // FUN_1410b7910 1410b.c:5026 -> FUN_1410b6880 1410b.c:8409-8417). Both
                            // applied twice the Demon of Hatred's 9116 burn (14 rows 70200xxx share it).
                            (atk.sp_effect_id0, atk.sp_effect_id1, atk.sp_effect_id2, atk.sp_effect_id3, atk.sp_effect_id4) = (charge(&b.spec).unwrap_or(-1), -1, -1, -1, -1);
                            sa.bullet_hits.push((we, atk));
                            b.fx_handed = true;
                        }
                    }
                    // Its poison / burn build-up goes onto Wolf's gauges (status.rs; gap: whether his
                    // guard keeps some off a bullet's own SpEffects is not traced: all of it lands).
                    let (build, own): (Vec<i64>, Vec<i64>) = sp_effects(&b.spec).partition(|id| crate::status::Statuses::is_build_up(&combat, *id));
                    statuses.hit_wolf(&combat, we, config.player.combat_art, &wa.item_sp, &build, crate::status::Guard::None);
                    // Its other SpEffects for their effectEndurance (one with none: a frame).
                    for id in own {
                        let key = id.to_string();
                        // The shooter's own rows too: the Demon's 3702085 (0.3 s) is only there.
                        let row = script.as_ref().and_then(|s| s.sp_effects.get(&key)).or_else(|| combat.kind(b.kind).data.sp_effects.get(&key)).or_else(|| combat.player.sp_effects.get(&key));
                        let left = row.map_or(0.0, |s| s.effect_endurance).max(dt);
                        p.timed.retain(|t| t.0 != id);
                        p.timed.push((id, left));
                    }
                    if b.spec.is_penetrate == 0 {
                        ended = true;
                    }
                }
            }
        }
        if ended {
            // Its AtkParam's stains where it ends (decalId1 / 2: the oil pot's 50010 / 50011).
            if let Some(atk) = &b.spec.attack {
                for id in [atk.decal_id1, atk.decal_id2].into_iter().filter(|id| *id > 0) {
                    stains.write(crate::gore::PlaceStain { id, at: to });
                }
            }
            // By its whole-life hit record: a penetrating bullet ends on a later frame than its touch.
            let launch = launches(b.spec.launch_condition_type, b.hit, landed, expired);
            if let Some(child) = row(b.spec.hit_bullet_id).filter(|_| launch) {
                spawn_chain(&mut commands, child, b.shooter, b.kind, to, dir, b.floor, script.clone(), b.chain, b.inherit_sfx);
            }
            // sfxId_Hit where it struck the ground, or Wolf when combat.rs does not judge the hit
            // (no damage: then it plays sfxId_Hit or, guarded / deflected, sfxId_Flick).
            if (landed || b.hit && !b.fx_handed) && b.spec.sfx_id_hit > 0 {
                let at = Transform::from_translation(to).with_rotation(Quat::from_rotation_arc(Vec3::NEG_Z, (-dir).normalize_or(Vec3::NEG_Z)));
                commands.spawn(crate::fxr::FxEffect::new(b.spec.sfx_id_hit, None, false, at));
            }
            // Its flying effect stops emitting where it ended; what it spawned plays out.
            if let Some(f) = b.fx {
                if let Ok(mut f) = effects.get_mut(f) {
                    f.stop = true;
                    f.follow = false;
                    f.anchor = None;
                    f.at = *btf;
                }
            }
            commands.entity(be).despawn();
        }
    }
    // Forget the shots that have no bullets left.
    let live: std::collections::HashSet<u32> = bullets.iter().map(|(_, b, _)| b.chain).collect();
    shared_hit.retain(|c, _| live.contains(c));
}

/// Whether HitBulletID comes when a bullet ends, by its launchConditionType (Smithbox SDT Param
/// Enums BULLET_LAUNCH_CONDITION_TYPE): 0 always, 1 hit water, 2 hit water or swamp (never:
/// the sim has no water; gap), 3 did not hit an enemy, 4 hit an enemy, 254 its life ran out,
/// 255 hit the ground or an enemy; other values (unknown) on any end. Its only enemy is Wolf.
/// 2026-10-12 bug hunt round 10: every end launched (the Genpei arrow 13100520's child, lct 1).
pub fn launches(lct: i64, hit_enemy: bool, landed: bool, expired: bool) -> bool {
    match lct {
        1 | 2 => false,
        3 => !hit_enemy,
        4 => hit_enemy,
        254 => expired,
        255 => hit_enemy || landed,
        _ => true,
    }
}

/// One frame of homing (exe FUN_140b11270): none for homingAngle < 0 or until the bullet has
/// flown homingBeginDist; from then on, while the target point is hormingStopRange or more
/// away, the flight turns toward it by at most homingAngle deg/s. The point is `aim` (Wolf's
/// chest, where `fire` aims) raised targetYOffsetRange. gap: the exe takes the target's root or
/// one of its dummy polys (FUN_140b0e680, the id at +0x34 is not traced); homingAngleX (a
/// separate pitch limit when >= 0) is not modelled.
pub fn steer(spec: &BulletSpec, vel: Vec3, travelled: f32, pos: Vec3, aim: Option<Vec3>, dt: f32) -> Vec3 {
    let Some(aim) = aim else { return vel };
    if spec.homing_angle < 0.0 || travelled <= spec.homing_begin_dist {
        return vel;
    }
    let to = aim + Vec3::Y * spec.target_y_offset_range - pos;
    let speed = vel.length();
    if to.length() < spec.horming_stop_range || speed <= 1.2e-7 || to.length_squared() < 1e-8 {
        return vel;
    }
    let (dir, want) = (vel / speed, to.normalize());
    let turn = dir.angle_between(want).min(spec.homing_angle.to_radians() * dt);
    let axis = dir.cross(want);
    if turn <= 0.0 || axis.length_squared() < 1e-12 {
        return vel;
    }
    Quat::from_axis_angle(axis.normalize(), turn) * vel
}

/// One frame of speed (exe FUN_140b14650): within `dist` metres flown accelInRange along the
/// flight and gravityInRange down, beyond it the OutRange pair; then the speed is held within
/// minVellocity..maxVellocity (below the min: the min, else at most the max). A bullet at rest
/// speeds up along `facing`. gap: the range is assumed to be `dist` (the exe reads bullet
/// +0xabc, its source is not traced); the water slow-down (FUN_140b123d0) and the accel pause
/// at +0xb6c are not modelled.
pub fn flight(spec: &BulletSpec, vel: Vec3, facing: Vec3, travelled: f32, dt: f32) -> Vec3 {
    let (accel, gravity) = if travelled > spec.dist { (spec.accel_out_range, spec.gravity_out_range) } else { (spec.accel_in_range, spec.gravity_in_range) };
    let v = vel + (vel.normalize_or(facing) * accel - Vec3::Y * gravity) * dt;
    let speed = v.length();
    let held = if speed < spec.min_vellocity { spec.min_vellocity } else { speed.min(spec.max_vellocity) };
    v.normalize_or(facing) * held
}

/// Debug (H, with the hitboxes): enemy bullets as red lines / spheres.
fn draw(show: Option<Res<crate::combat::ShowHurtboxes>>, bullets: Query<(&EnemyBullet, &Transform)>, mut gizmos: Gizmos) {
    if !show.is_some_and(|s| s.0) {
        return;
    }
    for (b, tf) in &bullets {
        if b.vel.length_squared() < 1e-4 {
            gizmos.sphere(Isometry3d::from_translation(tf.translation), b.spec.hit_radius.clamp(0.05, 3.0), Color::srgb(1.0, 0.3, 0.2));
            continue;
        }
        gizmos.line(tf.translation - b.vel.normalize_or_zero() * 0.3, tf.translation, Color::srgb(1.0, 0.35, 0.3));
    }
}
