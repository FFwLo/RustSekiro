//! Wolf's Shinobi Prosthetic tools: the equipped tool's TAE BulletBehavior spawns its Bullet rows
//! (player.bullets "v<variation>:<judge>"), with their fans (numShoot / shootAngle*), child chains
//! (intervalCreateBulletId, HitBulletID - e.g. the Shinobi Firecracker: carrier 710000 -> spark
//! 710001 -> 710002 -> burst 710003, 2 m, SpEffect 230110 + cool time 107100) and SpEffects.
//! Shuriken (weapon 70000, anims a070_*):
//!
//! F starts GroundSubAttackCombo1 (a070_400000); letting go early plays the Release
//! (a070_400100), whose TAE BulletBehavior (judge 150, frame 3) throws Bullet 700050;
//! holding through plays the full throw (judge 100, frame 21: Bullet 700000). Each throw
//! costs a Spirit Emblem. Bullets fly at initVellocity, decelerate (accelOutRange) and
//! drop (gravityOutRange) past `dist` metres, live `life` seconds and hit with their
//! AtkParam: damage = prosthetic attackBasePhysics x atkPhysCorrection, posture
//! directAtkStamDamage. The enemy can guard or deflect them.

use bevy::prelude::*;

use crate::actor::{Actor, ActorSet, data_for};
use crate::anim::Hurtboxes;
use crate::combat::{BODY_RADIUS, apply_knockback, damage_kb, enemy_damage_state, segment_distance};
use crate::data::{BulletSpec, Combat, STATE_INFO_JUST_GUARD};
use crate::enemy::Enemy;
use crate::hud::CombatLog;
use crate::model::Dummies;
use crate::player::{FLAG_SHIELD_BLOCK, Player};


#[derive(Component)]
pub struct Projectile {
    spec: BulletSpec,
    /// Base damage of the tool level that fired it (EquipParamWeapon attackBasePhysics / Fire).
    base: (f32, f32),
    vel: Vec3,
    travelled: f32,
    age: f32,
    /// The throw it belongs to (its children share the hit list).
    chain: u32,
    /// Age of the next intervalCreateBulletId child.
    next_interval: f32,
    /// Characters this bullet already hit.
    hit: Vec<Entity>,
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
    config: Res<crate::config::GameConfig>,
    mut player: Query<(&Actor, &Transform, &mut Player, Option<&Dummies>)>,
    enemies: Query<(Entity, &Transform), (With<Enemy>, Without<Player>)>,
    markers: Query<&GlobalTransform>,
    lock: Option<Res<crate::camera::LockOn>>,
    mut log: Option<ResMut<CombatLog>>,
    mut chain: Local<u32>,
) {
    for (a, tf, mut p, dummies) in &mut player {
        if a.anim.is_empty() || a.t == a.prev_t {
            continue;
        }
        let d = data_for(&combat, a.side);
        let Some(anim) = d.anim(&a.anim) else { continue };
        // Upgrade-gated throws (StateInfo 907, 996-999) are skipped: only judge 100/150.
        let crossing = |e: &&crate::data::Event| e.kind == 2 && e.start > a.prev_t && e.start <= a.t && e.ungated();
        // The equipped tool level's bullets: "v<variation>:<judge>" (BehaviorParam_PC 100000000 +
        // variation * 1000 + judge), else the Shuriken LV1's bare judge.
        let tool = crate::player::equipped_tool(&combat, &config, p.tool_slot);
        // Its Spirit Emblem cost (EquipParamWeapon resourceItemA) is paid once per use, by the
        // anim's first bullet. gap: the exe's exact consume point is not traced.
        let first = anim.events.iter().filter(|e| e.kind == 2 && e.ungated()).map(|e| e.start).fold(f32::INFINITY, f32::min);
        for e in anim.events.iter().filter(crossing) {
            let spec = e.arg_i64("BehaviorJudgeID").and_then(|j| {
                tool.and_then(|t| d.bullets.get(&format!("v{}:{j}", t.variation))).or_else(|| d.bullets.get(&j.to_string()))
            });
            let Some(spec) = spec else { continue };
            let cost = tool.map_or(1, |t| t.emblems.max(1));
            if e.start == first {
                if p.emblems < cost {
                    continue;
                }
                p.emblems -= cost;
            }
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
            *chain += 1;
            spawn_row(&mut commands, spec, from, dir, *chain, tool.map_or((20.0, 0.0), |t| (t.attack_base_physics, t.attack_base_fire)));
            if let (Some(log), true) = (log.as_mut(), e.start == first) {
                let name = tool.map_or_else(|| "prosthetic".to_string(), |t| combat.weapon_name(t.id));
                log.push(format!("{name} ({} emblems)", p.emblems), Color::srgb(0.8, 0.85, 1.0));
            }
        }
    }
}

/// Spawns one Bullet row at `pos`: numShoot bullets fanned from `dir` by shootAngle +
/// i * shootAngleInterval (yaw, degrees) and tilted by shootAngleXZ (-90 = straight down).
fn spawn_row(commands: &mut Commands, spec: &BulletSpec, pos: Vec3, dir: Vec3, chain: u32, base: (f32, f32)) {
    let flat = dir.with_y(0.0).normalize_or(Vec3::NEG_Z);
    // An area burst (standing still, >= 1 m: the firecracker's 710003): its crackle.
    // gap: the FXR (sfxId_Bullet) is not decoded; sparks from vfx.rs stand in.
    if spec.init_vellocity == 0.0 && spec.hit_radius >= 1.0 {
        for k in 0..4 {
            let a = k as f32 * std::f32::consts::FRAC_PI_2 + chain as f32;
            let p = pos + Vec3::new(a.cos(), 0.3 + 0.2 * k as f32, a.sin()) * 0.5;
            commands.spawn(crate::vfx::Clash::bundle(p, Vec3::new(a.cos(), 0.6, a.sin()), crate::vfx::Kind::Deflect));
        }
    }
    for i in 0..spec.num_shoot.max(1) {
        let yaw = (spec.shoot_angle + i as f32 * spec.shoot_angle_interval).to_radians();
        let mut d = if spec.shoot_angle == 0.0 && spec.shoot_angle_interval == 0.0 { dir } else { Quat::from_rotation_y(-yaw) * flat };
        if spec.shoot_angle_xz != 0.0 {
            let right = d.cross(Vec3::Y).normalize_or(Vec3::X);
            d = Quat::from_axis_angle(right, spec.shoot_angle_xz.to_radians()) * d;
        }
        commands.spawn((
            Projectile { spec: spec.clone(), base, vel: d * spec.init_vellocity, travelled: 0.0, age: 0.0, chain, next_interval: spec.interval_create_wait_time, hit: Vec::new() },
            Transform::from_translation(pos),
            Name::new(format!("Bullet {}", spec.bullet_id)),
        ));
    }
}

/// A bullet that can touch characters: it carries damage, posture or SpEffects (the chains'
/// carriers are AtkParam "dummy bullet - do not hit the enemy" rows with none of them).
fn hits_characters(spec: &BulletSpec) -> bool {
    let sp = [spec.sp_effect_id0, spec.sp_effect_id1, spec.sp_effect_id2, spec.sp_effect_id3, spec.sp_effect_id4].iter().any(|v| *v > 0);
    sp || spec.attack.as_ref().is_some_and(|a| a.atk_phys_correction > 0.0 || a.atk_fire_correction > 0.0 || a.atk_stam > 0.0 || a.direct_atk_stam_damage > 0.0)
}

/// c9997.lua SP_EFFECT_REF_BURST_*: an NPC with a "special attack character" resident
/// (1000055, SpEffect 230100: the beasts) reacts to the critical burst (1000057), everyone else to
/// the non-critical one (1000056).
const REF_BURST_ENABLE: i64 = 1000055;
const REF_BURST_NONCRITICAL: i64 = 1000056;
const REF_BURST_CRITICAL: i64 = 1000057;

/// Characters hit per bullet chain (isUseSharedHitList).
#[derive(Resource, Default)]
struct SharedHits(std::collections::HashMap<u32, Vec<Entity>>);

#[allow(clippy::too_many_arguments)]
fn fly_bullets(
    mut commands: Commands,
    time: Res<Time>,
    combat: Res<Combat>,
    mut shared: Local<SharedHits>,
    mut bullets: Query<(Entity, &mut Projectile, &mut Transform), (Without<Actor>, Without<Enemy>)>,
    mut enemies: Query<(Entity, &mut Actor, &Transform, &mut Enemy, Option<&Hurtboxes>)>,
    markers: Query<&GlobalTransform>,
    mut log: Option<ResMut<CombatLog>>,
    mut sounds: Option<ResMut<crate::sound::SoundQueue>>,
) {
    let dt = time.delta_secs();
    let rows = &combat.player.bullet_rows;
    let row = |id: i64| if id > 0 { rows.get(&id.to_string()) } else { None };
    for (be, mut b, mut btf) in &mut bullets {
        b.age += dt;
        // Past `dist`: decelerate along the flight and fall.
        if b.travelled > b.spec.dist && b.spec.dist > 0.0 {
            let speed = b.vel.length();
            let new_speed = (speed + b.spec.accel_out_range * dt).max(0.0);
            b.vel = b.vel.normalize_or_zero() * new_speed - Vec3::Y * b.spec.gravity_out_range * dt;
        }
        let from = btf.translation;
        let mut to = from + b.vel * dt;
        // The floor (y = 0) stops it unless it passes through the map (isPenetrateMap).
        let landed = to.y <= 0.02 && b.vel.y < 0.0;
        if landed {
            to.y = 0.02;
        }
        b.travelled += (to - from).length();
        btf.translation = to;
        let dir = b.vel.normalize_or(Vec3::NEG_Z);
        // intervalCreateBulletId: a child every intervalCreateTimeMin s while it flies.
        if let Some(child) = row(b.spec.interval_create_bullet_id) {
            while b.age >= b.next_interval && b.spec.interval_create_time_min > 0.0 {
                b.next_interval += b.spec.interval_create_time_min;
                spawn_row(&mut commands, child, to, dir, b.chain, b.base);
            }
        }

        let mut ended = landed || b.age > b.spec.life.max(0.0) + 1e-4;
        if hits_characters(&b.spec) {
            for (ee, mut da, dtf, mut e, hurt) in &mut enemies {
                if da.hp <= 0.0 || e.is_dead() || b.hit.contains(&ee) {
                    continue;
                }
                if b.spec.is_use_shared_hit_list != 0 && shared.0.get(&b.chain).is_some_and(|v| v.contains(&ee)) {
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
                b.hit.push(ee);
                if b.spec.is_use_shared_hit_list != 0 {
                    shared.0.entry(b.chain).or_default().push(ee);
                }
                if b.spec.is_penetrate == 0 {
                    ended = true;
                }
                let msg = hit_enemy(&combat, &b, from, to, &mut da, dtf, &mut e, time.elapsed_secs(), sounds.as_deref_mut());
                if let (Some(log), Some(m)) = (log.as_mut(), msg) {
                    log.push(m, Color::srgb(0.8, 0.85, 1.0));
                }
                if ended {
                    break;
                }
            }
        }
        if ended {
            // HitBulletID: what it turns into when it ends (the firecracker's spark -> burst).
            if let Some(child) = row(b.spec.hit_bullet_id) {
                spawn_row(&mut commands, child, to, dir, b.chain, b.base);
            }
            commands.entity(be).despawn();
        }
    }
    // Forget the hit lists of chains that have no bullets left.
    let live: std::collections::HashSet<u32> = bullets.iter().map(|(_, b, _)| b.chain).collect();
    shared.0.retain(|c, _| live.contains(c));
}

/// One bullet hitting the enemy: its SpEffects (the firecracker's burst reaction and cool time),
/// then its AtkParam (deflect / block / damage). Returns the log line.
#[allow(clippy::too_many_arguments)]
fn hit_enemy(
    combat: &Combat,
    b: &Projectile,
    from: Vec3,
    to: Vec3,
    da: &mut Actor,
    dtf: &Transform,
    e: &mut Enemy,
    now: f32,
    mut sounds: Option<&mut crate::sound::SoundQueue>,
) -> Option<String> {
    let sps = [b.spec.sp_effect_id0, b.spec.sp_effect_id1, b.spec.sp_effect_id2, b.spec.sp_effect_id3, b.spec.sp_effect_id4];
    let sp_of = |id: i64| combat.player.sp_effects.get(&id.to_string());
    let refs: Vec<i64> = sps.iter().filter(|v| **v > 0).filter_map(|id| sp_of(*id)).map(|s| s.behavior_ref_id).collect();
    if refs.contains(&REF_BURST_NONCRITICAL) || refs.contains(&REF_BURST_CRITICAL) {
        let npc = combat.param("NpcParam", combat.foe.npc_row);
        let special = (0..32)
            .filter_map(|i| npc[format!("spEffectID{i}")].as_i64().filter(|v| *v > 0))
            .any(|id| combat.enemy.sp_effects.get(&id.to_string()).is_some_and(|s| s.behavior_ref_id == REF_BURST_ENABLE));
        let wanted = if special { REF_BURST_CRITICAL } else { REF_BURST_NONCRITICAL };
        if !refs.contains(&wanted) {
            return None;
        }
        // The cool-time SpEffect (stateInfo 976, effectEndurance 30 s) blocks the next reaction.
        // gap: what the exe does with stateInfo 976 is not traced; read as "no reaction meanwhile".
        if now < e.burst_until {
            return Some("firecracker: enemy is used to it (cool time)".into());
        }
        let cool = sps.iter().filter_map(|id| sp_of(*id)).filter(|s| s.state_info == 976).map(|s| s.effect_endurance).fold(0.0, f32::max);
        e.burst_until = now + cool;
        // c9997.lua GetSpDamage -> SP_DAMAGE_BURST -> W_AssassinationBloodReaction (a000_020110).
        // gap: the damage transition rank check (rank 4) is not modelled; it interrupts anything.
        if !e.is_broken() {
            e.react(da, combat, "AssassinationBloodReaction");
            return Some("firecracker: enemy staggered".into());
        }
        return None;
    }
    let atk = b.spec.attack.as_ref()?;
    if !(atk.atk_phys_correction > 0.0 || atk.atk_fire_correction > 0.0 || atk.atk_stam > 0.0 || atk.direct_atk_stam_damage > 0.0) {
        return None;
    }
    let dd = &combat.enemy;
    let facing = da.forward().dot((from - dtf.translation).with_y(0.0).normalize_or_zero()) > 0.0;
    if facing && !da.anim.is_empty() && dd.has_state_info(&da.anim, da.t, STATE_INFO_JUST_GUARD) {
        if let Some(q) = sounds.as_mut() {
            q.0.extend(crate::sound::guard_sounds(combat, atk, true).into_iter().map(|se| (se, to)));
        }
        return Some("enemy deflects the shuriken".to_string());
    }
    if facing && !da.anim.is_empty() && dd.flag(&da.anim, da.t, FLAG_SHIELD_BLOCK) {
        if let Some(q) = sounds.as_mut() {
            q.0.extend(crate::sound::guard_sounds(combat, atk, false).into_iter().map(|se| (se, to)));
        }
        da.add_posture(atk.repel_lost_stam_damage);
        return Some(format!("enemy blocks the shuriken (+{:.0} posture)", atk.repel_lost_stam_damage));
    }
    // The tool's attackBasePhysics / attackBaseFire times the AtkParam corrections; fire scaled by
    // the NPC's fireDamageCutRate.
    let fire_cut = combat.param("NpcParam", combat.foe.npc_row)["fireDamageCutRate"].as_f64().unwrap_or(1.0) as f32;
    let dmg = b.base.0 * atk.atk_phys_correction / 100.0 + b.base.1 * atk.atk_fire_correction / 100.0 * fire_cut;
    let pd = if atk.direct_atk_stam_damage > 0.0 { atk.direct_atk_stam_damage } else { atk.atk_stam };
    da.hp = (da.hp - dmg).max(0.0);
    da.add_posture(pd);
    apply_knockback(combat, da, from, dtf.translation, atk.knockback_hit, damage_kb(atk.dmg_level), "knockbackRate_vsPlayer_DirectHit");
    if !e.is_attacking() && !da.posture_broken() && !e.is_broken() {
        if let Some(s) = enemy_damage_state(atk.dmg_level, da, from, dtf.translation) {
            e.react(da, combat, &s);
        }
    }
    if let Some(q) = sounds.as_mut() {
        for se in crate::sound::hit_sounds(combat, atk, crate::sound::defender_materials(combat, crate::actor::Side::Enemy)) {
            q.0.push((se, to));
        }
    }
    Some(format!("hit: -{dmg:.0} HP, +{pd:.0} posture"))
}

fn draw_bullets(bullets: Query<(&Projectile, &Transform)>, mut gizmos: Gizmos) {
    for (b, tf) in &bullets {
        if b.vel.length_squared() < 1e-4 {
            gizmos.sphere(Isometry3d::from_translation(tf.translation), b.spec.hit_radius.max(0.05), Color::srgb(1.0, 0.6, 0.2));
            continue;
        }
        let back = tf.translation - b.vel.normalize_or_zero() * 0.25;
        gizmos.line(back, tf.translation, Color::srgb(0.85, 0.9, 1.0));
    }
}
