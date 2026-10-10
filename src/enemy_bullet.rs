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
//! gap: EmittePosType / FollowType (homing, attached bullets) and the map collision are not
//! modelled; nor the force erase (isHitOtherBulletForceEraseA / B: which bullets erase is not in
//! the params), so the signs stay until hit once.

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
    shooter: Entity,
    /// The shooter's enemy kind (its Bullet rows).
    kind: usize,
    vel: Vec3,
    travelled: f32,
    age: f32,
    next_interval: f32,
    /// Height of the floor it lands on (Wolf's feet when it was fired).
    floor: f32,
    hit: bool,
    /// Fired by a boss script: its rows.
    script: Option<Arc<ScriptDef>>,
}

/// A boss script's Shoot Bullet this frame: its first Bullet row, the owner (whose attack it is)
/// and where it leaves from (`source`'s dummy poly).
pub struct ScriptShot {
    pub def: Arc<ScriptDef>,
    pub row: i64,
    pub owner: Entity,
    pub source: Entity,
    pub dmy: i16,
}

#[derive(Resource, Default)]
pub struct ScriptShots(pub Vec<ScriptShot>);

pub struct EnemyBulletPlugin;

impl Plugin for EnemyBulletPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ScriptShots>()
            .add_systems(FixedUpdate, (fire, fire_scripted, fly).chain().after(ActorSet::Advance).before(ActorSet::Resolve))
            .add_systems(Update, draw.run_if(resource_exists::<GizmoConfigStore>));
    }
}

/// A bullet that can hurt: an AtkParam with damage or posture damage (the gong's 60 m "sound"
/// bullets and the system dummies have none).
fn harmful(spec: &BulletSpec) -> bool {
    spec.attack.as_ref().is_some_and(|a| a.oppose_target != 0 && (a.npc_damage() > 0.0 || a.atk_stam > 0.0 || a.direct_atk_stam_damage > 0.0 || matches!(a.sp_attribute, 6 | 10)))
}

/// The Bullet row's SpEffects for Wolf (not the Lightning Reversal charges 9490 / 9495: the
/// catch in combat.rs gives those).
fn sp_effects(spec: &BulletSpec) -> impl Iterator<Item = i64> {
    [spec.sp_effect_id0, spec.sp_effect_id1, spec.sp_effect_id2, spec.sp_effect_id3, spec.sp_effect_id4].into_iter().filter(|id| *id > 0 && !matches!(id, 9490 | 9495))
}

fn fire(
    mut commands: Commands,
    combat: Res<Combat>,
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
        let crosses = |ev: &crate::data::Event| ((ev.start > a.prev_t && ev.start <= a.t) || (a.prev_t == 0.0 && ev.start == 0.0)) && d.fires(ev);
        for ev in anim.events.iter().filter(|ev| ev.kind == 2 && crosses(ev)) {
            let judge = ev.arg_i64("BehaviorJudgeID").unwrap_or(-1);
            let Some(spec) = d.bullets.get(&judge.to_string()) else { continue };
            let dmy = ev.arg_i64("DummyPolyID").unwrap_or(-1) as i16;
            let from = dummies
                .and_then(|dm| dm.0.get(&dmy))
                .and_then(|&m| markers.get(m).ok())
                .map(|g| g.translation())
                .unwrap_or(tf.translation + a.forward() * 0.5 + Vec3::Y * 0.6);
            // Aimed at Wolf's chest when he is within lockShootLimitAng of the facing.
            let mut dir = a.forward();
            if let Some(t) = target {
                let to = (t + Vec3::Y * 0.3 - from).normalize_or_zero();
                if to.with_y(0.0).normalize_or_zero().angle_between(a.forward()).to_degrees() <= spec.lock_shoot_limit_ang.max(1.0) {
                    dir = to;
                }
            }
            let floor = target.map_or(tf.translation.y - CAPSULE_HALF_HEIGHT, |t| t.y - CAPSULE_HALF_HEIGHT);
            spawn_row(&mut commands, spec, ent, a.kind, from, dir, floor, None);
        }
    }
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
        let from = dummies
            .and_then(|dm| dm.0.get(&shot.dmy))
            .and_then(|&m| markers.get(m).ok())
            .map(|g| g.translation())
            .unwrap_or(stf.translation + Vec3::Y);
        let floor = floor.unwrap_or(otf.translation.y);
        spawn_row(&mut commands, spec, shot.owner, oa.kind, from, sa.forward(), floor, Some(shot.def.clone()));
    }
}

/// One Bullet row at `pos`: numShoot bullets fanned by shootAngle + i * shootAngleInterval (yaw)
/// and tilted by shootAngleXZ, as prosthetic::spawn_row.
#[allow(clippy::too_many_arguments)]
fn spawn_row(commands: &mut Commands, spec: &BulletSpec, shooter: Entity, kind: usize, pos: Vec3, dir: Vec3, floor: f32, script: Option<Arc<ScriptDef>>) {
    let flat = dir.with_y(0.0).normalize_or(Vec3::NEG_Z);
    for i in 0..spec.num_shoot.max(1) {
        let yaw = (spec.shoot_angle + i as f32 * spec.shoot_angle_interval).to_radians();
        let mut d = if spec.shoot_angle == 0.0 && spec.shoot_angle_interval == 0.0 { dir } else { Quat::from_rotation_y(-yaw) * flat };
        if spec.shoot_angle_xz != 0.0 {
            let right = d.cross(Vec3::Y).normalize_or(Vec3::X);
            d = Quat::from_axis_angle(right, spec.shoot_angle_xz.to_radians()) * d;
        }
        commands.spawn((
            EnemyBullet {
                spec: spec.clone(),
                shooter,
                kind,
                vel: d * spec.init_vellocity,
                travelled: 0.0,
                age: 0.0,
                next_interval: spec.interval_create_wait_time,
                floor,
                hit: false,
                script: script.clone(),
            },
            Transform::from_translation(pos),
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
    mut player: Query<(Entity, &Transform, Option<&Hurtboxes>, &mut Player)>,
    mut shooters: Query<&mut Actor, (With<Enemy>, Without<Player>)>,
    markers: Query<&GlobalTransform>,
) {
    let dt = time.delta_secs();
    let mut wolf = player.single_mut().ok();
    for (be, mut b, mut btf) in &mut bullets {
        let script = b.script.clone();
        let rows = script.as_ref().map_or(&combat.kind(b.kind).data.bullet_rows, |s| &s.bullet_rows);
        let row = |id: i64| if id > 0 { rows.get(&id.to_string()) } else { None };
        b.age += dt;
        if b.travelled > b.spec.dist && b.spec.dist > 0.0 {
            let speed = b.vel.length();
            let new_speed = (speed + b.spec.accel_out_range * dt).max(0.0);
            b.vel = b.vel.normalize_or_zero() * new_speed - Vec3::Y * b.spec.gravity_out_range * dt;
        }
        let from = btf.translation;
        let mut to = from + b.vel * dt;
        let landed = to.y <= b.floor + 0.02 && b.vel.y < 0.0;
        if landed {
            to.y = b.floor + 0.02;
        }
        b.travelled += (to - from).length();
        btf.translation = to;
        let dir = b.vel.normalize_or(Vec3::NEG_Z);
        if let Some(child) = row(b.spec.interval_create_bullet_id) {
            while b.age >= b.next_interval && b.spec.interval_create_time_min > 0.0 {
                b.next_interval += b.spec.interval_create_time_min;
                spawn_row(&mut commands, child, b.shooter, b.kind, to, dir, b.floor, script.clone());
            }
        }
        let expired = b.spec.life >= 0.0 && b.age > b.spec.life + 1e-4;
        let mut ended = landed || expired;
        if !b.hit && (harmful(&b.spec) || sp_effects(&b.spec).next().is_some()) {
            if let Some((we, wtf, hurt, p)) = wolf.as_mut() {
                let (we, wtf, hurt) = (*we, *wtf, *hurt);
                let r = b.spec.hit_radius;
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
                if touches {
                    b.hit = true;
                    if harmful(&b.spec) {
                        if let (Ok(mut sa), Some(atk)) = (shooters.get_mut(b.shooter), b.spec.attack.clone()) {
                            sa.bullet_hits.push((we, atk));
                        }
                    }
                    // Its SpEffects for their effectEndurance (one with none: a frame).
                    for id in sp_effects(&b.spec) {
                        let key = id.to_string();
                        let row = script.as_ref().and_then(|s| s.sp_effects.get(&key)).or_else(|| combat.player.sp_effects.get(&key));
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
            // launchConditionType 254: the child only when its life ran out.
            let launch = b.spec.launch_condition_type != 254 || expired;
            if let Some(child) = row(b.spec.hit_bullet_id).filter(|_| launch) {
                spawn_row(&mut commands, child, b.shooter, b.kind, to, dir, b.floor, script.clone());
            }
            commands.entity(be).despawn();
        }
    }
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
