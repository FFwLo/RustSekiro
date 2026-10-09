//! Wolf's Shinobi Prosthetic: the Loaded Shuriken (weapon 70000, anims a070_*).
//!
//! F starts GroundSubAttackCombo1 (a070_400000); letting go early plays the Release
//! (a070_400100), whose TAE BulletBehavior (judge 150, frame 3) throws Bullet 700050;
//! holding through plays the full throw (judge 100, frame 21: Bullet 700000). Each throw
//! costs a Spirit Emblem. Bullets fly at initVellocity, decelerate (accelOutRange) and
//! drop (gravityOutRange) past `dist` metres, live `life` seconds and hit with their
//! AtkParam: damage = prosthetic attackBasePhysics x atkPhysCorrection, posture
//! directAtkStamDamage. The enemy can guard or deflect them.

use bevy::prelude::*;

use crate::actor::{Actor, ActorSet, Side, data_for};
use crate::anim::Hurtboxes;
use crate::combat::{BODY_RADIUS, apply_knockback, damage_kb, enemy_damage_state, segment_distance};
use crate::data::{BulletSpec, Combat, STATE_INFO_JUST_GUARD};
use crate::enemy::Enemy;
use crate::hud::CombatLog;
use crate::model::Dummies;
use crate::player::{FLAG_SHIELD_BLOCK, Player};

/// The prosthetic's weapon row (attackBase* for bullet damage).
const PROSTHETIC_WEAPON: i64 = 70000;

#[derive(Component)]
pub struct Projectile {
    spec: BulletSpec,
    vel: Vec3,
    travelled: f32,
    age: f32,
}

pub struct ProstheticPlugin;

impl Plugin for ProstheticPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(FixedUpdate, (spawn_bullets, fly_bullets).chain().after(ActorSet::Advance).before(ActorSet::Resolve))
            .add_systems(Update, draw_bullets.run_if(resource_exists::<GizmoConfigStore>));
    }
}

/// TAE BulletBehavior (type 2) crossing in the player's prosthetic anim -> a projectile
/// from the throwing hand toward the target (within lockShootLimitAng of the facing).
fn spawn_bullets(
    mut commands: Commands,
    combat: Res<Combat>,
    mut player: Query<(&Actor, &Transform, &mut Player, Option<&Dummies>)>,
    enemies: Query<(Entity, &Transform), (With<Enemy>, Without<Player>)>,
    markers: Query<&GlobalTransform>,
    lock: Option<Res<crate::camera::LockOn>>,
    mut log: Option<ResMut<CombatLog>>,
) {
    for (a, tf, mut p, dummies) in &mut player {
        if a.anim.is_empty() || a.t == a.prev_t {
            continue;
        }
        let d = data_for(&combat, a.side);
        let Some(anim) = d.anim(&a.anim) else { continue };
        // Upgrade-gated throws (StateInfo 907, 996-999) are skipped: only judge 100/150.
        let crossing = |e: &&crate::data::Event| e.kind == 2 && e.start > a.prev_t && e.start <= a.t && e.ungated();
        for e in anim.events.iter().filter(crossing) {
            let Some(spec) = e.arg_i64("BehaviorJudgeID").and_then(|j| d.bullets.get(&j.to_string())) else { continue };
            if p.emblems == 0 {
                continue;
            }
            p.emblems -= 1;
            // From the event's DummyPoly (2: the throwing hand); without a model (headless),
            // chest height in front of Wolf.
            let dmy = e.arg_i64("DummyPolyID").unwrap_or(-1) as i16;
            let from = dummies
                .and_then(|dm| dm.0.get(&dmy))
                .and_then(|&m| markers.get(m).ok())
                .map(|g| g.translation())
                .unwrap_or(tf.translation + a.forward() * 0.4 + Vec3::Y * 0.5);
            // Target: the lock target, else LockCamParam's bullet auto-capture (bulletMaxRadius 30 m,
            // bulletAngRange 15 deg of the facing; the same CSChrAutoHomingModule limits, copied by
            // FUN_140b2e0e0); the throw bends toward it within Bullet lockShootLimitAng.
            let mut dir = a.forward();
            let row = combat.param("LockCamParam", 0);
            let (max_r, ang) = (row["bulletMaxRadius"].as_f64().unwrap_or(30.0) as f32, row["bulletAngRange"].as_f64().unwrap_or(15.0) as f32);
            let locked = lock.as_ref().and_then(|l| l.target).and_then(|e| enemies.get(e).ok()).map(|(_, t)| t);
            let captured = || {
                enemies
                    .iter()
                    .map(|(_, t)| t)
                    .filter(|t| {
                        let v = t.translation - tf.translation;
                        v.length() <= max_r && v.with_y(0.0).normalize_or_zero().angle_between(a.forward()).to_degrees() <= ang
                    })
                    .min_by(|x, y| x.translation.distance(tf.translation).total_cmp(&y.translation.distance(tf.translation)))
            };
            if let Some(t) = locked.or_else(captured) {
                let to = (t.translation + Vec3::Y * 0.3 - from).normalize_or_zero();
                if to.with_y(0.0).normalize_or_zero().angle_between(a.forward()).to_degrees() <= spec.lock_shoot_limit_ang.max(1.0) {
                    dir = to;
                }
            }
            commands.spawn((
                Projectile { spec: spec.clone(), vel: dir * spec.init_vellocity, travelled: 0.0, age: 0.0 },
                Transform::from_translation(from),
                Name::new("Shuriken"),
            ));
            if let Some(log) = log.as_mut() {
                log.push(format!("shuriken ({} emblems)", p.emblems), Color::srgb(0.8, 0.85, 1.0));
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn fly_bullets(
    mut commands: Commands,
    time: Res<Time>,
    combat: Res<Combat>,
    mut bullets: Query<(Entity, &mut Projectile, &mut Transform), (Without<Actor>, Without<Enemy>)>,
    mut enemies: Query<(&mut Actor, &Transform, &mut Enemy, Option<&Hurtboxes>)>,
    markers: Query<&GlobalTransform>,
    mut log: Option<ResMut<CombatLog>>,
    mut sounds: Option<ResMut<crate::sound::SoundQueue>>,
) {
    let dt = time.delta_secs();
    for (be, mut b, mut btf) in &mut bullets {
        b.age += dt;
        if b.age > b.spec.life.max(0.05) {
            commands.entity(be).despawn();
            continue;
        }
        // Past `dist`: decelerate along the flight and fall.
        if b.travelled > b.spec.dist {
            let speed = b.vel.length();
            let new_speed = (speed + b.spec.accel_out_range * dt).max(0.0);
            b.vel = b.vel.normalize_or_zero() * new_speed - Vec3::Y * b.spec.gravity_out_range * dt;
        }
        let from = btf.translation;
        let to = from + b.vel * dt;
        b.travelled += (to - from).length();
        btf.translation = to;

        for (mut da, dtf, mut e, hurt) in &mut enemies {
            if da.hp <= 0.0 || e.is_dead() {
                continue;
            }
            let r = b.spec.hit_radius;
            let hit = match hurt {
                Some(h) => h.0.iter().any(|&(ma, mb, hr)| match (markers.get(ma), markers.get(mb)) {
                    (Ok(pa), Ok(pb)) => segment_distance(from, to, pa.translation(), pb.translation()) <= r + hr,
                    _ => false,
                }),
                None => {
                    let feet = dtf.translation - Vec3::Y * crate::player::CAPSULE_HALF_HEIGHT;
                    segment_distance(from, to, feet + Vec3::Y * 0.3, feet + Vec3::Y * 1.5) <= r + BODY_RADIUS
                }
            };
            if !hit {
                continue;
            }
            commands.entity(be).despawn();
            let Some(atk) = b.spec.attack.as_ref() else { break };
            let dd = &combat.enemy;
            let facing = da.forward().dot((from - dtf.translation).with_y(0.0).normalize_or_zero()) > 0.0;
            let msg;
            if facing && !da.anim.is_empty() && dd.has_state_info(&da.anim, da.t, STATE_INFO_JUST_GUARD) {
                if let Some(q) = sounds.as_mut() {
                    q.0.extend(crate::sound::guard_sounds(&combat, atk, true).into_iter().map(|se| (se, to)));
                }
                msg = "enemy deflects the shuriken".to_string();
            } else if facing && !da.anim.is_empty() && dd.flag(&da.anim, da.t, FLAG_SHIELD_BLOCK) {
                if let Some(q) = sounds.as_mut() {
                    q.0.extend(crate::sound::guard_sounds(&combat, atk, false).into_iter().map(|se| (se, to)));
                }
                da.add_posture(atk.repel_lost_stam_damage);
                msg = format!("enemy blocks the shuriken (+{:.0} posture)", atk.repel_lost_stam_damage);
            } else {
                let base = combat.param("EquipParamWeapon", PROSTHETIC_WEAPON)["attackBasePhysics"].as_f64().unwrap_or(0.0) as f32;
                let dmg = base * atk.atk_phys_correction / 100.0;
                let pd = if atk.direct_atk_stam_damage > 0.0 { atk.direct_atk_stam_damage } else { atk.atk_stam };
                da.hp = (da.hp - dmg).max(0.0);
                da.add_posture(pd);
                apply_knockback(&combat, &mut da, from, dtf.translation, atk.knockback_hit, damage_kb(atk.dmg_level), "knockbackRate_vsPlayer_DirectHit");
                if !e.is_attacking() && !da.posture_broken() && !e.is_broken() {
                    if let Some(s) = enemy_damage_state(atk.dmg_level, &da, from, dtf.translation) {
                        e.react(&mut da, &combat, &s);
                    }
                }
                if let Some(q) = sounds.as_mut() {
                    for se in crate::sound::hit_sounds(&combat, atk, crate::sound::defender_materials(&combat, crate::actor::Side::Enemy)) {
                        q.0.push((se, to));
                    }
                }
                msg = format!("shuriken hit: -{dmg:.0} HP, +{pd:.0} posture");
            }
            if let Some(log) = log.as_mut() {
                log.push(msg, Color::srgb(0.8, 0.85, 1.0));
            }
            break;
        }
        let _ = Side::Enemy;
    }
}

fn draw_bullets(bullets: Query<(&Projectile, &Transform)>, mut gizmos: Gizmos) {
    for (b, tf) in &bullets {
        let back = tf.translation - b.vel.normalize_or_zero() * 0.25;
        gizmos.line(back, tf.translation, Color::srgb(0.85, 0.9, 1.0));
    }
}
