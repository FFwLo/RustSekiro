//! Wolf's Shinobi Prosthetic tools: the equipped tool's TAE BulletBehavior spawns its Bullet rows
//! (player.bullets "v<variation>:<judge>"), with their fans (numShoot / shootAngle*), child chains
//! (intervalCreateBulletId, HitBulletID - e.g. the Shinobi Firecracker: carrier 710000 -> spark
//! 710001 -> 710002 -> burst 710003, 2 m, SpEffect 230110 + cool time 107100) and SpEffects.
//! Shuriken (weapon 70000, anims a070_*):
//!
//! F starts GroundSubAttackCombo1 (a070_400000); letting go early plays the Release
//! (a070_400100), whose TAE BulletBehavior (judge 150, frame 3) throws Bullet 700050;
//! holding through plays the full throw (judge 100, frame 21: Bullet 700000). Each throw
//! costs a Spirit Emblem. Bullets fly at initVellocity with the exe's bullet update
//! (enemy_bullet.rs steer / flight: homing, in / out of range accel and gravity, min / max
//! speed), live `life` seconds and hit with their
//! AtkParam: damage = prosthetic attackBasePhysics x atkPhysCorrection, posture
//! directAtkStamDamage. The enemy can guard or deflect them.

use bevy::prelude::*;

use crate::actor::{Actor, ActorSet, data_for};
use crate::anim::Hurtboxes;
use crate::combat::{BODY_RADIUS, apply_knockback, damage_kb, segment_distance};
use crate::data::{BulletSpec, Combat};
use crate::enemy::Enemy;
use crate::hud::CombatLog;
use crate::model::Dummies;
use crate::player::Player;

#[derive(Component)]
pub struct Projectile {
    spec: BulletSpec,
    /// The EquipParamWeapon row whose attackBase* (all five elements) it scales: the tool
    /// level's, or Kusabimaru 5000's for the Lightning Reversal bolt; 0 = none (20 physical).
    base: i64,
    vel: Vec3,
    /// The way it was shot: its children's when it has no speed (the items' carriers 3490 /
    /// 3510 / 3520 stand still for their life; their throw went world -Z whatever Wolf faced).
    aim: Vec3,
    travelled: f32,
    age: f32,
    /// The throw it belongs to (its children share the hit list).
    chain: u32,
    /// Age of the next intervalCreateBulletId child.
    next_interval: f32,
    /// Characters this bullet already hit.
    /// The characters it hit and its age then.
    hit: Vec<(Entity, f32)>,
    /// Its flying effect (sfxId_Bullet, or the parent's with isInheritSfxToChild).
    fx: Option<Entity>,
    /// The effect its children inherit (-1 = none).
    inherit_sfx: i64,
}

/// A one-shot game effect placed in the world (a bullet's sfxId_Hit / sfxId_Flick).
fn spawn_fx_at(commands: &mut Commands, id: i64, pos: Vec3, dir: Vec3) {
    if id > 0 {
        let at = Transform::from_translation(pos).with_rotation(Quat::from_rotation_arc(Vec3::NEG_Z, dir.normalize_or(Vec3::NEG_Z)));
        commands.spawn(crate::fxr::FxEffect::new(id, None, false, at));
    }
}

pub struct ProstheticPlugin;

impl Plugin for ProstheticPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(FixedUpdate, (spawn_bullets.after(crate::anim::PoseSet), fly_bullets).chain().after(ActorSet::Advance).before(ActorSet::Resolve))
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
    enemies: Query<(&Enemy, &Actor, &Transform), Without<Player>>,
    markers: Query<&GlobalTransform>,
    lock: Option<Res<crate::camera::LockOn>>,
    mut log: Option<ResMut<CombatLog>>,
    time: Res<Time>,
    mut chain: Local<u32>,
) {
    for (a, tf, mut p, dummies) in &mut player {
        // Wolf's clock: SpEffect timers stop in a hit stop (actor.rs clock_dt, round 64).
        let dt = a.clock(time.delta_secs());
        p.timed.retain_mut(|(_, left)| {
            *left -= dt;
            *left > 0.0
        });
        if a.anim.is_empty() || a.t == a.prev_t {
            continue;
        }
        let d = data_for(&combat, &a);
        let Some(anim) = d.anim(&a.anim) else { continue };
        // Upgrade-gated throws (StateInfo 905-946: the tool level's resident) fire for their level.
        // An event fires when the anim time crosses its start (frame 0 on the anim's first step:
        // the Umbrella's consumption dummy 999).
        let crosses = |e: &crate::data::Event| ((e.start > a.prev_t && e.start <= a.t) || (a.prev_t == 0.0 && e.start == 0.0)) && d.fires(e);
        // TAE 4 BulletBehavior_Midair: the sword's (the Lightning Reversal's a050_308900) and the
        // tools' air throws, whose anims (a070..a079_403000 / 403050) hold only TAE 4 bullets:
        // a071_403000 judge 120 -> BehaviorParam_PC 107100120 -> Bullet 710020 (wepCost 1). The
        // air throw fired nothing and cost nothing before (round 26). gap: the exe's TAE 4
        // handler is not traced (read as TAE 2 in the air).
        let tool_anim = crate::player::is_tool_anim(&a.anim);
        let crossing = |e: &&crate::data::Event| (e.kind == 2 || e.kind == 4) && crosses(e);
        // The equipped tool level's bullets: "v<variation>:<judge>" (BehaviorParam_PC 100000000 +
        // variation * 1000 + judge), else the Shuriken LV1's bare judge.
        let tool = crate::player::equipped_tool(&combat, &config, p.tool_slot);
        // Spirit Emblems: every behaviour (TAE BulletBehavior 2 / AttackBehavior 1) whose row has
        // wepCost pays the tool's cost when it fires (CharData::behavior). gap: the exe's consume call is not
        // traced; a bullet that can't be paid is not fired (the press already checked the cost).
        // Only the tool anims (a070..a079) resolve their judges through the tool's variation; the
        // combat art anims (a100..a110) through the art's, paying the art's resourceItemA (Dragon Flash's
        // 105004220, Ashina Cross's 105005200, Mortal Draw's consumption dummy 105007999).
        let art_anim = a.anim.get(1..4).and_then(|g| g.parse::<i64>().ok()).is_some_and(|g| (100..=110).contains(&g));
        let art_cost = crate::player::art_cost(&combat, &config);
        let (var, cost) = if art_anim {
            (d.art_variation.unwrap_or(5000), art_cost)
        } else {
            (tool.map_or(7000, |t| t.variation), tool.map_or(1, |t| t.emblems.max(1)))
        };
        let paying = tool_anim || (art_anim && art_cost > 0);
        let costs = |j: i64| paying && d.behavior(var, j).is_some_and(|b| b.wep_cost != 0);
        let payer = if art_anim { combat.weapon_name(config.player.combat_art) } else { tool.map_or_else(|| "prosthetic".to_string(), |t| combat.weapon_name(t.id)) };
        for e in anim.events.iter().filter(|e| e.kind == 1 && crosses(e)) {
            if e.arg_i64("BehaviorJudgeID").is_some_and(costs) {
                // gap: the exe's wepCost consume is not traced; only the press gate (env 3035 ->
                // FUN_14084dd10) checks the balance, so the TAE 1 / 940 payment is taken as unconditional.
                p.emblems = p.emblems.saturating_sub(cost);
                if let Some(log) = log.as_mut() {
                    log.push(format!("{payer} ({} emblems)", p.emblems), Color::srgb(0.8, 0.85, 1.0));
                }
            }
        }
        // TAE 940 BehaviorParam_AddSpEffect: the behaviour's cost, and its refType-2 SpEffect on Wolf
        // for its effectEndurance (Divine Abduction 107700100 -> 107700: ref 309 for 3 s).
        for e in anim.events.iter().filter(|e| e.kind == 940 && tool_anim && crosses(e)) {
            let Some(b) = e.arg_i64("BehaviorJudgeId").and_then(|j| d.behavior(var, j)).copied() else { continue };
            if b.wep_cost != 0 {
                p.emblems = p.emblems.saturating_sub(cost);
                if let Some(log) = log.as_mut() {
                    log.push(format!("{payer} ({} emblems)", p.emblems), Color::srgb(0.8, 0.85, 1.0));
                }
            }
            // All nine refType-2 rows Wolf's TAE reaches have an effectEndurance > 0 (3 s, 30 s,
            // 0.1 s); a 0 / -1 one (while applied / forever) would be skipped here.
            let endurance = d.sp_effects.get(&b.ref_id.to_string()).map_or(0.0, |s| s.effect_endurance);
            if b.ref_type == 2 && endurance > 0.0 {
                p.timed.retain(|(id, _)| *id != b.ref_id);
                p.timed.push((b.ref_id, endurance));
            }
        }
        for e in anim.events.iter().filter(crossing) {
            let judge = e.arg_i64("BehaviorJudgeID").unwrap_or(-1);
            let paid = costs(judge);
            if paid {
                if p.emblems < cost {
                    continue;
                }
                p.emblems -= cost;
                if let Some(log) = log.as_mut() {
                    log.push(format!("{payer} ({} emblems)", p.emblems), Color::srgb(0.8, 0.85, 1.0));
                }
            }
            // A combat art's bullets: its own variation's rows only (One Mind 105005210 -> Bullet
            // 500510), never the equipped tool's (the umbrella's 760010 on the same judge).
            let art = art_anim.then(|| d.bullets.get(&format!("v{var}:{judge}"))).flatten();
            // The sword's bullets (outside the tool groups): Kusabimaru's variation 5000
            // (BehaviorParam_PC 105000184 -> Bullet 500184) and its attackBase*.
            let sword = (!tool_anim && !art_anim).then(|| d.bullets.get(&format!("v5000:{judge}"))).flatten();
            // The bare judge's bullet (700000 / 700050 ...) only when the tool has no
            // BehaviorParam_PC row of its own: a row that is not a bullet (refType != 1, as Sen
            // Throw's 107004100 / 107004150: refType 0, refId 0) fires nothing.
            let own_row = tool.and_then(|t| d.behavior(t.variation, judge)).is_some_and(|b| b.ref_type != 1);
            let spec = if art_anim {
                art
            } else {
                // The tool's rows only from the tool anims (round 49: a sword anim's judge with no
                // v5000 row fell through to them, firing a tool bullet unpaid; none in the data
                // today).
                sword
                    .or_else(|| tool.filter(|_| tool_anim).and_then(|t| d.bullets.get(&format!("v{}:{judge}", t.variation))))
                    .or_else(|| (tool_anim && !own_row).then(|| d.bullets.get(&judge.to_string())).flatten())
            };
            let Some(spec) = spec else { continue };
            // gap: an art bullet's attackBase* is taken as Kusabimaru's (the art's own
            // EquipParamWeapon row is not traced as its source).
            // The EquipParamWeapon row whose attackBase* (all five elements) the bullet scales:
            // Kusabimaru for sword / art bullets, else the tool level's own row (0 = none).
            let base = if sword.is_some() || art.is_some() { 5000 } else { tool.map_or(0, |t| t.id) };
            // From the event's DummyPoly (2: the throwing hand); without a model (headless),
            // chest height in front of Wolf.
            let dmy = e.arg_i64("DummyPolyID").unwrap_or(-1) as i16;
            // combat.rs's lookup (round 59): 1<model><dummy> ids (Lightning Reversal 10100, the
            // Sabimaru's 21130) were missed and spawned at the headless point.
            let from = dummies
                .and_then(|dm| crate::combat::dmy_entity(dm, dmy as i64))
                .and_then(|m| markers.get(m).ok())
                .map(|g| g.translation())
                .unwrap_or(tf.translation + a.forward() * 0.4 + Vec3::Y * 0.5);
            // Target: the lock target, else LockCamParam's bullet auto-capture (bulletMaxRadius 30 m,
            // bulletAngRange 15 deg of the facing; the same CSChrAutoHomingModule limits, copied by
            // FUN_140b2e0e0); the throw bends toward it within Bullet lockShootLimitAng.
            let mut dir = a.forward();
            let row = combat.param("LockCamParam", 0);
            let (max_r, ang) = (row["bulletMaxRadius"].as_f64().unwrap_or(30.0) as f32, row["bulletAngRange"].as_f64().unwrap_or(15.0) as f32);
            // Only living enemies, as melee auto-homing (player.rs auto_homing_target): a body
            // lying in Mode::Dead after a kill bent the throw past the next enemy (round 23).
            let alive = |e: &Enemy, ea: &Actor| ea.hp > 0.0 && !e.is_dead();
            let locked = lock.as_ref().and_then(|l| l.target).and_then(|e| enemies.get(e).ok()).filter(|(e, ea, _)| alive(e, ea)).map(|(.., t)| t);
            let captured = || {
                enemies
                    .iter()
                    .filter(|(e, ea, _)| alive(e, ea))
                    .map(|(.., t)| t)
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
            spawn_row(&mut commands, spec, from, dir, *chain, base, -1);
            // spEffectIDForShooter onto Wolf (round 59: never applied, so the Flame Vent's
            // 106020 / the Spirit's 106030 enchantment never lit the sword; its stateInfo gates
            // the sword's judges through resident_gates). gap: children's rows (fired in flight)
            // are not applied.
            let shooter = spec.sp_effect_id_for_shooter;
            let endurance = d.sp_effects.get(&shooter.to_string()).map_or(0.0, |s| s.effect_endurance);
            if shooter > 0 && endurance > 0.0 {
                p.timed.retain(|(id, _)| *id != shooter);
                p.timed.push((shooter, endurance));
            }
        }
    }
}

/// Spawns one Bullet row at `pos`: numShoot bullets fanned from `dir` by shootAngle +
/// i * shootAngleInterval (yaw, degrees) and tilted by shootAngleXZ (-90 = straight down).
pub(crate) fn spawn_row(commands: &mut Commands, spec: &BulletSpec, pos: Vec3, dir: Vec3, chain: u32, base: i64, inherited: i64) {
    let flat = dir.with_y(0.0).normalize_or(Vec3::NEG_Z);
    let sfx = if spec.sfx_id_bullet > 0 { spec.sfx_id_bullet } else { inherited };
    let inherit_sfx = if spec.is_inherit_sfx_to_child != 0 { sfx } else { -1 };
    for i in 0..spec.num_shoot.max(1) {
        let yaw = (spec.shoot_angle + i as f32 * spec.shoot_angle_interval).to_radians();
        let mut d = if spec.shoot_angle == 0.0 && spec.shoot_angle_interval == 0.0 { dir } else { Quat::from_rotation_y(-yaw) * flat };
        if spec.shoot_angle_xz != 0.0 {
            let right = d.cross(Vec3::Y).normalize_or(Vec3::X);
            d = Quat::from_axis_angle(right, spec.shoot_angle_xz.to_radians()) * d;
        }
        let at = Transform::from_translation(pos).with_rotation(Quat::from_rotation_arc(Vec3::NEG_Z, d.normalize_or(Vec3::NEG_Z)));
        // With its GlobalTransform from the start: effects and hit tests reading it before the
        // next propagation would see the origin.
        let bullet = commands.spawn((at, GlobalTransform::from(at), Name::new(format!("Bullet {}", spec.bullet_id)))).id();
        // The flying effect rides on the bullet (FxEffect anchored to it).
        if std::env::var("SHINOBI_FFX_LOG").is_ok() {
            info!("bullet {} fxr {sfx} at {:.2}", spec.bullet_id, pos);
        }
        let fx = (sfx > 0).then(|| commands.spawn(crate::fxr::FxEffect::new(sfx, Some(crate::fxr::Anchor::Entity(bullet)), true, at)).id());
        commands.entity(bullet).insert(Projectile {
            spec: spec.clone(),
            base,
            vel: d * spec.init_vellocity,
            aim: d.normalize_or(Vec3::NEG_Z),
            travelled: 0.0,
            age: 0.0,
            chain,
            next_interval: spec.interval_create_wait_time,
            hit: Vec::new(),
            fx,
            inherit_sfx,
        });
    }
}

/// A bullet that can touch characters: it carries damage, posture or SpEffects (the chains'
/// carriers are AtkParam "dummy bullet - do not hit the enemy" rows with none of them).
fn hits_characters(spec: &BulletSpec) -> bool {
    let sp = [spec.sp_effect_id0, spec.sp_effect_id1, spec.sp_effect_id2, spec.sp_effect_id3, spec.sp_effect_id4].iter().any(|v| *v > 0);
    sp || spec.attack.as_ref().is_some_and(deals_something)
}

/// An AtkParam row that does HP (any of the five elements) or posture damage. Magic / dark were
/// left out: Lazulite Sacred Flame 72300 (magic only) and Projected Force 760000-760020 (dark)
/// did nothing (2026-10-12 round 22).
fn deals_something(a: &crate::data::Attack) -> bool {
    a.atk_phys_correction > 0.0
        || a.atk_mag_correction > 0.0
        || a.atk_fire_correction > 0.0
        || a.atk_thun_correction > 0.0
        || a.atk_dark_correction > 0.0
        || a.atk_stam > 0.0
        || a.direct_atk_stam_damage > 0.0
}

/// c9997.lua SP_EFFECT_REF_BURST_*: an NPC with a "special attack character" resident
/// (1000055, SpEffect 230100: the beasts) reacts to the critical burst (1000057), everyone else to
/// the non-critical one (1000056).
const REF_BURST_ENABLE: i64 = 1000055;
const REF_BURST_NONCRITICAL: i64 = 1000056;
const REF_BURST_CRITICAL: i64 = 1000057;
/// SP_EFFECT_REF_NO_BURST_REACTION (c9997.lua 548): the enemy's firecracker cool time.
const REF_NO_BURST_REACTION: i64 = 1000044;
/// SpEffect 220018's behaviorRefId, "enemy reaction playback: blinding (ash bag)", and the ash's
/// cool time row 3510 (30 s).
const REF_BLINDING: i64 = 1000058;
const ASH_COOL_TIME: i64 = 3510;
/// SP_EFFECT_REF_FIRE_ACTION_ENABLE / NO_FIRE_FIAR_REACTION (c9997.lua 544 / 589).
const REF_FIRE_ACTION_ENABLE: i64 = 1000040;
const REF_NO_FIRE_REACTION: i64 = 1000240;

/// Characters hit per bullet chain (isUseSharedHitList), with the time of the hit: the entry
/// runs out after dmgHitRecordLifeTime as a bullet's own record (round 68: the chain list kept
/// it for the chain's life, so the Flame Vent's second hit never landed).
#[derive(Resource, Default)]
struct SharedHits(std::collections::HashMap<u32, Vec<(Entity, f32)>>);

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
    mut statuses: ResMut<crate::status::Statuses>,
    mut effects: Query<&mut crate::fxr::FxEffect>,
    lock: Option<Res<crate::camera::LockOn>>,
    players: Query<&crate::player::Player>,
    mut ai_sounds: Option<ResMut<crate::enemy::AiSounds>>,
) {
    let dt = time.delta_secs();
    let rows = &combat.player.bullet_rows;
    // The attack-occurrence SpEffect of a tool bullet's hit (data::atk_occurrence; FUN_140b6e6a0
    // is the one hit path): category 2, so only a wepParamChange 0 / 2 row lands. Wolf's
    // enchantments are all 1 (the sword's): none. gap: an attacker-aimed one is applied on sword
    // hits only.
    let occurrence: Vec<i64> = players
        .iter()
        .next()
        .and_then(|p| crate::data::atk_occurrence(&p.timed.iter().map(|t| t.0).collect::<Vec<_>>(), crate::data::HIT_CAT_TOOL, false, false))
        .filter(|o| !o.2)
        .map_or(Vec::new(), |o| o.1);
    // Homing aims at the locked target (gap: the exe's bullet target for Wolf's throws is taken
    // as his lock-on target; without one nothing homes).
    // A live one only (round 59: a throw just after the kill curved into the corpse).
    let aim = lock
        .and_then(|l| l.target)
        .and_then(|t| enemies.get(t).ok())
        .filter(|(_, a, _, e, _)| a.hp > 0.0 && !e.is_dead())
        .map(|(_, _, tf, ..)| tf.translation);
    // The exported rows, else a goods' (items.rs bullet_row: the throwables' chains).
    let row = |id: i64| if id > 0 { rows.get(&id.to_string()).or_else(|| crate::items::bullet_row(id)) } else { None };
    for (be, mut b, mut btf) in &mut bullets {
        // Its AtkParam_Pc AppearAiSoundId as it starts, at the bullet (FUN_1409a1290's bullet
        // branch; enemy_bullet.rs fly).
        if b.age == 0.0 {
            if let (Some(s), Some(atk)) = (ai_sounds.as_mut(), b.spec.attack.as_ref()) {
                s.emit(&combat, crate::data::atk_ai_sounds(true, atk.atk_param_id).0, btf.translation, None, None);
            }
        }
        b.age += dt;
        // The exe's bullet update, shared with the enemies' (enemy_bullet.rs): homing, then the
        // in / out of range accel and gravity held within min..maxVellocity.
        let from = btf.translation;
        let facing = b.vel.normalize_or(b.aim);
        b.vel = crate::enemy_bullet::steer(&b.spec, b.vel, b.travelled, from, aim, dt);
        b.vel = crate::enemy_bullet::flight(&b.spec, b.vel, facing, b.travelled, dt);
        let mut to = from + b.vel * dt;
        // The map stops it unless it passes through (isPenetrateMap); with no map, the floor y 0.
        let mut landed = false;
        let step = to - from;
        if b.spec.is_penetrate_map == 0 && step.length() > 1e-5 {
            if let Some(t) = crate::map::terrain() {
                if let Some(d) = t.raycast(from, step.normalize(), step.length()) {
                    to = from + step.normalize() * d;
                    landed = true;
                }
            } else if to.y <= 0.02 && b.vel.y < 0.0 {
                to.y = 0.02;
                landed = true;
            }
        }
        b.travelled += (to - from).length();
        btf.translation = to;
        let dir = b.vel.normalize_or(b.aim);
        // intervalCreateBulletId: a child every intervalCreateTimeMin s while it flies.
        if let Some(child) = row(b.spec.interval_create_bullet_id) {
            while b.age >= b.next_interval && b.spec.interval_create_time_min > 0.0 {
                b.next_interval += b.spec.interval_create_time_min;
                spawn_row(&mut commands, child, to, dir, b.chain, b.base, b.inherit_sfx);
            }
        }

        let mut ended = landed || b.age > b.spec.life.max(0.0) + 1e-4;
        // A carrier that hits nothing itself but turns into what does (the Fistful of Ash's
        // throw 3511 -> its cloud 3512) bursts on reaching a body. gap: the exe's end condition
        // for such a carrier is not traced; flying its whole second put the cloud past the enemy.
        if !ended && !hits_characters(&b.spec) && row(b.spec.hit_bullet_id).is_some_and(hits_characters) {
            ended = enemies.iter().any(|(_, da, dtf, e, _)| {
                let feet = dtf.translation - Vec3::Y * crate::player::CAPSULE_HALF_HEIGHT;
                da.hp > 0.0 && !e.is_dead() && segment_distance(from, to, feet + Vec3::Y * 0.3, feet + Vec3::Y * 1.5) <= b.spec.hit_radius.max(0.2) + BODY_RADIUS
            });
        }
        if hits_characters(&b.spec) {
            for (ee, mut da, dtf, mut e, hurt) in &mut enemies {
                // A hit record runs out after dmgHitRecordLifeTime, as the enemies' (round 59:
                // the Flame Vent 720001, record 0.6 s of life 0.9, landed once).
                let record = b.spec.dmg_hit_record_life_time;
                let recorded = b.hit.iter().any(|&(h, at)| h == ee && !(record > 0.0 && b.age - at >= record - 1e-4));
                if da.hp <= 0.0 || e.is_dead() || recorded {
                    continue;
                }
                let now = time.elapsed_secs();
                if b.spec.is_use_shared_hit_list != 0 && shared.0.get(&b.chain).is_some_and(|v| v.iter().any(|&(h, at)| h == ee && !(record > 0.0 && now - at >= record - 1e-4))) {
                    continue;
                }
                // hitRadius growing to hitRadiusMax over spreadTime, as enemy_bullet.rs (round 59:
                // the Flame Vent 720001 1.0 -> 1.6 m over 0.6 s).
                let r = if b.spec.hit_radius_max > b.spec.hit_radius && b.spec.spread_time > 0.0 {
                    b.spec.hit_radius + (b.spec.hit_radius_max - b.spec.hit_radius) * (b.age / b.spec.spread_time).min(1.0)
                } else {
                    b.spec.hit_radius
                };
                // The part group of the capsule it touched (env(1120)).
                let mut part = 0u8;
                let hit = match hurt {
                    Some(h) => h.0.iter().enumerate().any(|(i, &(ma, mb, hr))| match (markers.get(ma), markers.get(mb)) {
                        (Ok(pa), Ok(pb)) => {
                            let touch = segment_distance(from, to, pa.translation(), pb.translation()) <= r + hr;
                            if touch {
                                part = h.1.get(i).copied().unwrap_or(0);
                            }
                            touch
                        }
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
                // Invulnerable (TAE 950 / 951, 954 IFrames: a knock-down, a throw) as for the
                // sword (round 59: a shuriken interrupted those); not recorded, as resolve.
                if b.spec.attack.as_ref().is_some_and(|atk| crate::combat::dodged(&combat, &da, None, atk)) {
                    continue;
                }
                b.hit.retain(|&(h, _)| h != ee);
                let age = b.age;
                b.hit.push((ee, age));
                if b.spec.is_use_shared_hit_list != 0 {
                    let list = shared.0.entry(b.chain).or_default();
                    list.retain(|&(h, _)| h != ee);
                    list.push((ee, time.elapsed_secs()));
                }
                if b.spec.is_penetrate == 0 {
                    ended = true;
                }
                let msg = hit_enemy(&combat, &b, from, to, part, &mut da, dtf, &mut e, sounds.as_deref_mut());
                // sfxId_Flick where it was deflected, sfxId_Hit where it landed or was blocked.
                match msg.as_deref() {
                    Some(m) if m.contains("deflect") => spawn_fx_at(&mut commands, b.spec.sfx_id_flick, to, -dir),
                    Some(_) => spawn_fx_at(&mut commands, b.spec.sfx_id_hit, to, -dir),
                    None => {}
                }
                if let (Some(log), Some(m)) = (log.as_mut(), msg.as_ref()) {
                    log.push(m.clone(), Color::srgb(0.8, 0.85, 1.0));
                }
                // The burn / poison build-up of a bullet that landed or was blocked (Bullet and
                // AtkParam spEffectId0-4, e.g. the Flame Vent's 720000 -> 9109), and c9997's
                // SP_DAMAGE_BURNING -> W_FireReaction once it burns, unless the state's transition
                // rank forbids rank 3 (ExecDebuffReaction 2961: not out of a large blow or a
                // posture break).
                if let Some(m) = msg.filter(|m| m.starts_with("hit") || m.contains("blocks")) {
                    let mut sps: Vec<i64> = [b.spec.sp_effect_id0, b.spec.sp_effect_id1, b.spec.sp_effect_id2, b.spec.sp_effect_id3, b.spec.sp_effect_id4].to_vec();
                    // Only the Bullet row's: FUN_1410b7910 fills a bullet hit's slots 0-4 from
                    // BulletParam +0x6c (FUN_1410b6880, 1410b.c:8413); the AtkParam's +0x20 ids
                    // are read on melee hits only (14099.c:9681), as enemy_bullet.rs (R36 C).
                    sps.extend(&occurrence);
                    let lines = statuses.hit(&combat, ee, &da, &sps, m.contains("blocks"));
                    // c9997 ExecDamage runs ExecDebuffReaction before ExecGuardBlock, and
                    // SP_DAMAGE_BURNING asks env(3041) GetStatusActivation STATUS_BURNING: the hit
                    // whose build-up breaks the burn gauge (guarded or not) flails him, later hits
                    // while he burns do not.
                    let ignited = lines.iter().any(|l| l == "BURNING");
                    for line in lines {
                        if let Some(log) = log.as_mut() {
                            log.push(line, Color::srgb(0.7, 0.55, 0.9));
                        }
                    }
                    let fire_reaction = combat.data_of(&da).states.contains_key("FireReaction");
                    // ExecDebuffReaction runs before ExecSpReactionLarge: on the igniting hit the
                    // DamageFire hit_enemy just started gives way to W_FireReaction.
                    let just_damage_fire = da.state == "DamageFire" && da.t == 0.0;
                    if ignited && fire_reaction && !da.posture_broken() && !e.is_broken() && (just_damage_fire || rank_allows(transition_rank(&da.state), 3)) {
                        e.react(&mut da, &combat, "FireReaction");
                        add_reaction_cool_time(&combat, &da, &mut e, REF_NO_FIRE_REACTION);
                    }
                }
                // Only a hit that stops it (not piercing) ends the sweep: a piercing bullet on its
                // landing / last frame still reaches everyone it crosses (round 49; before, only
                // the first - the Flame Vent puff 720000, the firecracker burst 710003 / 710013).
                if b.spec.is_penetrate == 0 {
                    break;
                }
            }
        }
        // Landed on the map: its AtkParam HitAiSoundId there (the Ceramic Shard's 3500: the
        // enemies hear where it broke).
        if landed {
            // Its sfxId_Hit where it struck the map (Five-color Rice's 310163 marking the ground).
            spawn_fx_at(&mut commands, b.spec.sfx_id_hit, to, -dir);
            if let (Some(s), Some(atk)) = (ai_sounds.as_mut(), b.spec.attack.as_ref()) {
                s.emit(&combat, crate::data::atk_ai_sounds(true, atk.atk_param_id).1, to, None, None);
            }
        }
        if ended {
            // HitBulletID: what it turns into when it ends (the firecracker's spark -> burst).
            if let Some(child) = row(b.spec.hit_bullet_id) {
                spawn_row(&mut commands, child, to, dir, b.chain, b.base, b.inherit_sfx);
            }
            // Its effect stops emitting where the bullet ended; what it spawned plays out.
            if let Some(mut f) = b.fx.and_then(|f| effects.get_mut(f).ok()) {
                f.stop = true;
                f.follow = false;
                f.anchor = None;
                f.at = *btf;
            }
            commands.entity(be).despawn();
        }
    }
    // Forget the hit lists of chains that have no bullets left.
    let live: std::collections::HashSet<u32> = bullets.iter().map(|(_, b, _)| b.chain).collect();
    shared.0.retain(|c, _| live.contains(c));
}

/// c9997.lua: the damage transition rank an enemy state passes to DamageCommonFunction /
/// LandCommonFunction in its _onUpdate (DAMAGE_TRANSITION_RANK__0..4); every other state passes
/// DAMAGE_TRANSITION_RANK__NONE (-1).
pub(crate) fn transition_rank(state: &str) -> i32 {
    match state {
        "DamageLargeBlow" | "DamageFling" | "DamageUpper" | "DamageAerialBlow" => 0,
        "TrunkCollapseLarge" | "TrunkCollapseBurst" | "TrunkCollapseBurst1" | "TrunkCollapseBurst2" | "TrunkCollapseBurst3" | "TrunkCollapseLightningStart" | "TrunkCollapseLightningLoop" | "TrunkCollapseLightningEnd" => 1,
        "TrunkCollapseFront" | "TrunkCollapseBack" | "TrunkCollapseAerial" | "GuardBreakRight" | "GuardBreakLeft" | "LandTrunk" => 2,
        s if s.starts_with("AttackBoundEmptyStaminaEnemy") => 2,
        "FireFearReaction" | "DamageWeak" | "DamageFire" | "DamageLightningStart" | "DamageLightningLoop" | "DamageLightningEnd" | "FireReaction" => 3,
        "JumpMoveEnd" | "ThrowNearReaction" | "ThrowFarReaction" | "AssassinationBloodReaction" | "HideReaction" | "BackRealityReaction" | "SpecialPoisonReaction"
        | "FingerWhistleReaction" | "LandDefault" | "LandHeavy" | "LandUpward" | "LandDownward" | "LandThrowDefFront" | "LandThrowDefBack" | "LandThrowDefAntiAir"
        | "LadderFallLand" => 4,
        s if ["DamageBlow", "DamagePush", "DamageWire", "DamageAerialFront", "DamageAerialBack", "GuardDamage", "JustGuardDamage", "GuardKick", "AttackBoundEnemy", "AttackNoBoundEnemy"]
            .iter()
            .any(|p| s.starts_with(p)) =>
        {
            4
        }
        _ => -1,
    }
}

/// c9997.lua IsEnabledTransitionRank (913): may a reaction of rank `dest` interrupt a state of
/// rank `cur`.
pub(crate) fn rank_allows(cur: i32, dest: i32) -> bool {
    match dest {
        0 => true,
        1 => cur != 0,
        2 | 3 => !(0..=2).contains(&cur),
        4 => !(0..=3).contains(&cur),
        _ => false,
    }
}

/// The cycle chain an SpEffect Wolf's bullet put on an enemy sets off (the exe's rule, "bug
/// fixes" round 43: a row fires its cycleOccurrenceSpEffectId while its
/// invocationConditionsStateChange1-3 gate holds, i.e. the owner wears an SpEffect of that
/// stateInfo, FUN_140c060d0). The Finger Whistle's madness:
/// - 230518 "enemy AI activation" (bullets 790004-6) -> 230510 "invoking, parent", gated on 959;
///   959 is the stateInfo of 230500 "madness can be activated", NpcParam spEffectID9 of the
///   dogs, chickens, monkeys ... (80 rows) -> 230515 "invoking" (COMMON_SP_EFFECT_CONFUSE):
///   effectEndurance -1, changeTeamType 29, so it stays on and the beast turns on everyone.
/// - The minor version: 230519 -> 230530 (ref 1000035, the FingerWhistleReaction) -> 230511 ->
///   230516 (COMMON_SP_EFFECT_CONFUSE_GHOST), the same gate.
/// A lasting row (endurance > 0, or -1 for good) goes on `sp_chained`, where the AI sees it
/// (common_table_ai_common.lua:235 replans on it) and combat::team_of reads its team.
/// It ends when its 959 gate fails (isWaitModeDelete, actor::run_sp_effect_chains): never while
/// the beast keeps 230500. Its look is status.rs `lasting_glow` (300205 / 304205 on dummy 7).
pub(crate) fn madden(a: &mut Actor, e: &Enemy, id: i64) {
    let state_info = |id: i64| crate::data::sp_cycle(id).map_or(0, |c| c.state_info);
    let mut worn: Vec<i64> = a.resident.iter().chain(e.clash.iter().map(|c| &c.0)).chain(a.sp_chained.iter().map(|c| &c.0)).map(|id| state_info(*id)).collect();
    let mut id = id;
    for _ in 0..4 {
        let Some(next) = crate::data::sp_cycle(id).map(|c| c.cycle).filter(|n| *n > 0) else { return };
        let Some(r) = crate::data::sp_cycle(next) else { return };
        if !r.gate.iter().filter(|c| **c > 0).all(|c| worn.contains(c)) {
            return;
        }
        let endurance = r.endurance;
        if endurance != 0.0 {
            let secs = if endurance < 0.0 { f32::INFINITY } else { endurance };
            a.sp_chained.retain(|c| c.0 != next);
            a.sp_chained.push((next, secs));
            worn.push(state_info(next));
        }
        id = next;
    }
}

/// One bullet hitting the enemy: its SpEffects (the firecracker's burst reaction and cool time),
/// then its AtkParam (deflect / block / damage). Returns the log line.
#[allow(clippy::too_many_arguments)]
fn hit_enemy(
    combat: &Combat,
    b: &Projectile,
    from: Vec3,
    to: Vec3,
    part: u8,
    da: &mut Actor,
    dtf: &Transform,
    e: &mut Enemy,
    mut sounds: Option<&mut crate::sound::SoundQueue>,
) -> Option<String> {
    let sps = [b.spec.sp_effect_id0, b.spec.sp_effect_id1, b.spec.sp_effect_id2, b.spec.sp_effect_id3, b.spec.sp_effect_id4];
    let sp_of = |id: i64| combat.player.sp_effects.get(&id.to_string()).or_else(|| crate::items::sp_row(id));
    let refs: Vec<i64> = sps.iter().filter(|v| **v > 0).filter_map(|id| sp_of(*id)).map(|s| s.behavior_ref_id).collect();
    // The Fistful of Ash's cool time (3510, 30 s, put on with the cloud): already blinded.
    let ash_cool = e.clash.iter().any(|c| c.0 == ASH_COOL_TIME);
    // The bullet's SpEffects on the enemy for their effectEndurance (the reversal bolt's
    // 3520085 "Divine Dragon: Lightning Reversal hit", 1 s, which the dragon's map script
    // waits for), before any early return: the firecracker's 230110 / 230111 are in the map AI
    // hold's wake list, and its burst does no HP damage (round 19).
    for id in sps.iter().filter(|v| **v > 0) {
        let secs = crate::data::sp_effect_endurance(combat, *id);
        e.add_timed(*id, secs.max(0.1));
        madden(da, e, *id);
    }
    if refs.contains(&REF_BURST_NONCRITICAL) || refs.contains(&REF_BURST_CRITICAL) {
        // The burst's AtkParam posture, as any bullet hit's: the beasts' burst (710013 / 710213 /
        // 710313, AtkParam 7100104: directAtkStamDamage 30, staminaPhysicsAttribute 10) times
        // NpcParam attriAStaminaDmgRate 3.0 (c1150 dogs, c1250 monkeys ...: 90, a dog has 15);
        // 0 on the General and the soldiers (round 30: it never did posture).
        if let Some(atk) = b.spec.attack.as_ref() {
            let pd = if atk.direct_atk_stam_damage > 0.0 { atk.direct_atk_stam_damage } else { atk.atk_stam };
            let (_, armor_posture, _) = crate::combat::armor_rates(combat, e, da, atk, None);
            let pd = (pd * crate::combat::def_stamina_rate(combat, da, atk.stamina_physics_attribute) * armor_posture).floor();
            if pd > 0.0 {
                da.add_posture(pd);
            }
        }
        // c9997.lua ExecDamageBreak (2550-2560) / the critical burst at 0 posture (2524): a burst
        // break plays W_TrunkCollapseBurst (8330 + GetRandomIndex 0-2, the ones it has), a rank 1
        // reaction; without those clips the plain break (combat.rs, TrunkCollapseFront).
        if da.posture_broken() && !e.is_broken() && rank_allows(transition_rank(&da.state), 1) {
            let d = combat.data_of(da);
            let clips: Vec<&str> = ["TrunkCollapseBurst1", "TrunkCollapseBurst2", "TrunkCollapseBurst3"].into_iter().filter(|s| d.anim_key(s).is_some()).collect();
            if !clips.is_empty() {
                let s = clips[(e.rand() * clips.len() as f32) as usize % clips.len()];
                da.play_state(d, s);
                e.on_posture_break(da, combat, false, 4.0);
                return Some("firecracker: POSTURE BROKEN".into());
            }
        }
        let npc = combat.npc(da);
        let special = (0..32)
            .filter_map(|i| npc[format!("spEffectID{i}")].as_i64().filter(|v| *v > 0))
            .any(|id| combat.data_of(da).sp_effects.get(&id.to_string()).is_some_and(|s| s.behavior_ref_id == REF_BURST_ENABLE));
        let wanted = if special { REF_BURST_CRITICAL } else { REF_BURST_NONCRITICAL };
        if !refs.contains(&wanted) {
            return None;
        }
        // The cool time is the enemy's own (c9997.lua 873: no SP_DAMAGE_BURST under
        // SP_EFFECT_REF_NO_BURST_REACTION 1000044). Its reaction clip a000_020110 adds it by TAE,
        // gated on the bullet's 107100 (stateInfo 976): c1070 251070 15 s, c1130 251130 14 s,
        // c5080 255080 20 s; the General, the soldiers, dogs and monkeys have none and react to
        // every firecracker (round 30; it was a flat 30 s for everyone).
        if enemy_refs(combat, da, e).contains(&REF_NO_BURST_REACTION) {
            return Some("firecracker: enemy is used to it (cool time)".into());
        }
        // c9997.lua GetSpDamage -> SP_DAMAGE_BURST -> W_AssassinationBloodReaction (a000_020110),
        // a rank 4 reaction (line 3091): not out of a rank 0-3 state (large blows, posture
        // breaks, burning / lightning reactions).
        if !e.is_broken() && rank_allows(transition_rank(&da.state), 4) {
            e.react(da, combat, "AssassinationBloodReaction");
            add_reaction_cool_time(combat, da, e, REF_NO_BURST_REACTION);
            return Some("firecracker: enemy staggered".into());
        }
        return None;
    }
    // The Fistful of Ash (220018, ref 1000058 "enemy reaction: blinding"): the enemy reels.
    // gap: c9997's blinding reaction is not traced; the soldiers have no clip of their own for
    // it, so the firecracker's flinch (W_AssassinationBloodReaction, rank 4) stands in.
    if refs.contains(&REF_BLINDING) {
        if !ash_cool && !e.is_broken() && rank_allows(transition_rank(&da.state), 4) && combat.data_of(da).states.contains_key("AssassinationBloodReaction") {
            e.react(da, combat, "AssassinationBloodReaction");
            return Some("ash: enemy blinded".into());
        }
        return Some(if ash_cool { "ash: enemy is used to it (cool time)" } else { "ash: no effect" }.into());
    }
    let atk = b.spec.attack.as_ref()?;
    if !deals_something(atk) {
        // A bullet whose AtkParam deals nothing but whose Bullet row carries build-up still
        // lands it: the Kodachi LV3-B poison mist (Bullet 750300-750302, spEffectId0 9009
        // poizonAttackPower 31; AtkParam 7503150 all 0) never poisoned (round 26).
        let carries = [b.spec.sp_effect_id0, b.spec.sp_effect_id1, b.spec.sp_effect_id2, b.spec.sp_effect_id3, b.spec.sp_effect_id4].iter().any(|&id| id > 0);
        return carries.then(|| "hit: build-up only".to_string());
    }
    let dd = combat.data_of(da);
    // As a sword hit (combat.rs resolve, guard_contact): only inside the guard arc, block or
    // deflect by the repel values and the guard type's disableGuard / disableJustGuard columns
    // (the Flame Vent's and Projected Force's rows). The bullet's weapon is taken as the sword's
    // (every Wolf weapon has attackBaseRepel 10).
    let contact = crate::combat::guard_contact(combat, dd, da, Some(&*e), (&da.anim, da.t), None, true, atk, 5000);
    let facing = !da.anim.is_empty() && da.forward().angle_between((from - dtf.translation).with_y(0.0).normalize_or_zero()).to_degrees() <= contact.half_angle;
    // A guarding enemy takes the bullet as a sword's deflect / block (combat.rs resolve, `bullet`):
    // the posture through its guard (combat.rs guard_posture: staminaGuardDef, the guard family), the
    // c9997 ExecGuardBlock reaction (guard_damage_table by dmgLevel and guardLevel) and the clash
    // SpEffects the AI counts (200210/211 deflect, 200215/216 block). Round 35: the deflect did
    // nothing and the block added the raw repelLostStamDamage with no reaction. gap: the clash ids
    // on a bullet are the melee path's (inferred).
    let deflect = facing && contact.guarded == crate::guard::Guarded::Deflect;
    if deflect || (facing && contact.guarded == crate::guard::Guarded::Block) {
        if let Some(q) = sounds.as_mut() {
            q.0.extend(crate::sound::guard_sounds(combat, atk, deflect).into_iter().map(|se| (se, to)));
        }
        let level = atk.level(false);
        let refs = enemy_refs(combat, da, e);
        let glv = combat.npc(da)["guardLevel"].as_i64().unwrap_or(4);
        let aerial = crate::combat::enemy_is_aerial(combat, da, e, dtf.translation.y);
        if deflect {
            // A deflect never breaks the defender (as the sword's).
            let pd = crate::combat::guard_posture(combat, da, &contact, None, &[], atk.atk_stam, true, atk);
            da.posture = (da.posture + pd).min(da.posture_max - 1.0);
            da.since_posture_damage = 0.0;
            let pick = crate::combat::enemy_guard_block(dd, &refs, level, glv, atk.just_deflect_action == 2, aerial);
            crate::combat::guard_pick(dd, da, e, combat, true, pick, from, dtf.translation, level, aerial, part);
            crate::combat::clash_after(dd, da, e, if atk.just_deflect_action == 1 { 200211 } else { 200210 });
            return Some(format!("enemy deflects the shuriken (+{pd:.0} posture)"));
        }
        let pd = crate::combat::guard_posture(combat, da, &contact, None, &[], atk.repel_lost_stam_damage, false, atk);
        da.add_posture(pd);
        // The parts of a tool's damage the guard lets through (the Flame Vent's fire: 67 %).
        if b.base > 0 {
            let chip = crate::combat::npc_guard_chip(combat, da, &contact, atk, crate::combat::player_attack_elements(combat, atk, b.base, da, 0.0));
            da.hp = (da.hp - chip).max(0.0);
        }
        if da.posture_broken() {
            e.react_from(da, combat, true, if atk.deflect_action == 2 { "GuardBreakLeft" } else { "GuardBreakRight" });
        } else {
            let pick = crate::combat::enemy_guard_block(dd, &refs, level, glv, atk.deflect_action == 2, aerial);
            crate::combat::guard_pick(dd, da, e, combat, true, pick, from, dtf.translation, level, aerial, part);
        }
        crate::combat::clash_after(dd, da, e, if atk.deflect_action == 1 { 200216 } else { 200215 });
        return Some(format!("enemy blocks the shuriken (+{pd:.0} posture)"));
    }
    // The weapon row's attackBase* per element times the AtkParam corrections, each scaled by
    // the NPC's <element>DamageCutRate, as a sword hit (combat.rs player_attack_damage).
    // Without a tool row: 20 physical; a goods' bullet (base -1, items.rs) its AtkParam's flat
    // atkPhys (Oil's and the Ceramic Shard's 1).
    let dmg = if b.base > 0 {
        crate::combat::player_attack_damage(combat, atk, b.base, da, 0.0)
    } else if b.base < 0 {
        atk.atk_phys
    } else {
        20.0 * atk.atk_phys_correction / 100.0
    };
    let pd = if atk.direct_atk_stam_damage > 0.0 { atk.direct_atk_stam_damage } else { atk.atk_stam };
    // The armour part and the enemy's posture SpEffects, as on a sword hit (combat.rs resolve).
    let (armor_hp, armor_posture, _) = crate::combat::armor_rates(combat, e, da, atk, None);
    let def_rate = crate::combat::def_stamina_rate(combat, da, atk.stamina_physics_attribute) * armor_posture;
    // An apparition's ghost barrier: the tools are not enchanted (Confetti is the sword's).
    let (ghost_hp, _) = crate::combat::ghost_barrier(combat, da, atk, &[]);
    let (dmg, pd) = (dmg * armor_hp * ghost_hp, pd * def_rate);
    da.hp = (da.hp - dmg).max(0.0);
    da.add_posture(pd);
    // The super-armour drain, as a sword hit's (combat::super_armor_hit).
    crate::combat::super_armor_hit(da, &e.clash, true, atk.atk_param_id);
    tool_hit_push(combat, da, e, from, dtf.translation, atk.knockback_hit, atk.dmg_level, dmg);
    // c9997 GetSpDamage: a lightning hit (DAMAGE_ELEMENT_LIGHTNING 6) on one with
    // SP_EFFECT_REF_LIGHTNING_DAMAGE_ENABLE (1000041: its NpcParam's 6021) and without
    // NO_LIGHTNING_DAMAGE (1000260) is SP_DAMAGE_LIGHTNING -> W_DamageLightningStart, a rank 3
    // reaction (line 2995) even mid-swing.
    let lightning = atk.sp_attribute == 6 && {
        let refs = enemy_refs(combat, da, e);
        refs.contains(&1000041) && !refs.contains(&1000260)
    };
    // SP_DAMAGE_FIRE (c9997 GetSpDamage 879, ExecSpReactionLarge 2986): a fire hit
    // (DAMAGE_ELEMENT_FIRE 2: the Flame Vent's v7200 rows) on one with
    // SP_EFFECT_REF_FIRE_ACTION_ENABLE (1000040: its NpcParam's 6020, the Villager zombies c1500,
    // the General's 10209800) and without NO_FIRE_FIAR_REACTION (1000240, its DamageFire clip's
    // own cool time) -> W_DamageFire, rank 3 (round 30: it played the level reaction).
    let fire = atk.sp_attribute == 2 && {
        let refs = enemy_refs(combat, da, e);
        refs.contains(&REF_FIRE_ACTION_ENABLE) && !refs.contains(&REF_NO_FIRE_REACTION)
    };
    if lightning && !da.posture_broken() && !e.is_broken() && rank_allows(transition_rank(&da.state), 3) {
        e.react(da, combat, "DamageLightningStart");
    } else if fire && dd.anim_key("DamageFire").is_some() && !da.posture_broken() && !e.is_broken() && rank_allows(transition_rank(&da.state), 3) {
        e.react(da, combat, "DamageFire");
        add_reaction_cool_time(combat, da, e, REF_NO_FIRE_REACTION);
    } else if !da.posture_broken() && !e.is_broken() {
        match bullet_reaction(combat, da, e, atk.dmg_level, from, dtf.translation) {
            Some(s) => e.react(da, combat, &s),
            // No reaction (mid-swing, or none for that level: the Divine Dragon has none):
            // c9997 ExecNoSyncAddDamage, the part's additive flinch.
            None => crate::combat::no_sync_add_damage(dd, da, part, from, dtf.translation),
        }
    }
    if let Some(q) = sounds.as_mut() {
        for se in crate::sound::hit_sounds(combat, atk, crate::sound::defender_materials(combat, da)) {
            q.0.push((se, to));
        }
    }
    Some(format!("hit: -{dmg:.0} HP, +{pd:.0} posture"))
}

/// After a tool hit's damage: its knockback, none under ChrActionFlag 24 (no damage motion, as
/// combat.rs resolve; exe b6bfe0, sekiro-rs fight.rs:660-666), and the map AI hold's damage wake
/// (enemy.rs ai_hold: hurt by Wolf with HP lost) whether or not a reaction plays - a held enemy
/// hit with only the additive flinch never woke.
pub(crate) fn tool_hit_push(combat: &Combat, da: &mut Actor, e: &mut Enemy, from: Vec3, at: Vec3, knockback: f32, level: i64, dmg: f32) {
    let dd = combat.data_of(da);
    let no_motion = !da.anim.is_empty() && dd.flag(&da.anim, da.t, crate::combat::FLAG_NO_DAMAGE_MOTION);
    if !no_motion {
        apply_knockback(combat, da, from, at, None, knockback, damage_kb(level), "knockbackRate_vsPlayer_DirectHit");
    }
    if dmg > 0.0 {
        e.hurt_by_wolf = true;
    }
}

/// A tool hit's reaction, as a sword hit's (combat.rs resolve): the damage level the HKS sees
/// (0 under ChrActionFlag 24, else damage_motion_level's super-armour swap) and c9997's aerial
/// table for an enemy in the air (enemy_damage_state_air). None = the additive flinch.
pub(crate) fn bullet_reaction(combat: &Combat, da: &Actor, e: &Enemy, level: i64, from: Vec3, at: Vec3) -> Option<String> {
    let dd = combat.data_of(da);
    let level = if dd.flag(&da.anim, da.t, crate::combat::FLAG_NO_DAMAGE_MOTION) { 0 } else { crate::combat::damage_motion_level(dd, da, level) };
    crate::combat::enemy_damage_state_air(dd, level, da, from, at, crate::combat::enemy_is_aerial(combat, da, e, at.y))
}

/// A reaction clip's TAE SpEffects with the reaction's own block ref (SP_EFFECT_REF_NO_BURST_REACTION
/// 1000044, NO_FIRE_FIAR_REACTION 1000240), kept for their effectEndurance past the clip's short
/// TAE window, when their invocationConditionsStateChange is one of the enemy's stateInfos now
/// (the firecracker's 107100: 976). An endurance-0 window row runs on as its replaceSpEffectId
/// (c1020 DamageFire a000_008200: 287410 over 0-6.4 s -> 287411 5 s), as status.rs reads it.
pub(crate) fn add_reaction_cool_time(combat: &Combat, da: &Actor, e: &mut Enemy, block_ref: i64) {
    let d = combat.data_of(da);
    let Some(anim) = d.anim(&da.anim) else { return };
    let mut ids: Vec<i64> = da.resident.clone();
    ids.extend(e.clash.iter().map(|c| c.0));
    let states: Vec<i64> = ids.iter().filter_map(|id| d.sp_effects.get(&id.to_string()).or_else(|| combat.player.sp_effects.get(&id.to_string()))).map(|s| s.state_info).filter(|&s| s > 0).collect();
    let on = |s: &crate::data::SpEffect| {
        let conds = [s.invocation_conditions_state_change1, s.invocation_conditions_state_change2, s.invocation_conditions_state_change3];
        conds.iter().all(|&c| c <= 0) || conds.iter().any(|c| *c > 0 && states.contains(c))
    };
    let mut add = Vec::new();
    for ev in anim.events.iter().filter(|ev| matches!(ev.kind, 67 | 401)) {
        let Some(id) = ev.arg_i64("SpEffectID") else { continue };
        let Some(s) = d.sp_effects.get(&id.to_string()).filter(|s| s.behavior_ref_id == block_ref && on(s)) else { continue };
        if s.effect_endurance > 0.0 {
            add.push((id, ev.start + s.effect_endurance));
        } else if let Some(r) = d.sp_effects.get(&s.replace_sp_effect_id.to_string()).filter(|r| r.behavior_ref_id == block_ref && r.effect_endurance > 0.0) {
            add.push((s.replace_sp_effect_id, ev.end + r.effect_endurance));
        }
    }
    for (id, secs) in add {
        e.add_timed(id, secs.max(0.1));
    }
}

/// The behaviorRefIds of an enemy's SpEffects now: its residents, its anim's and the timed ones.
pub(crate) fn enemy_refs(combat: &Combat, a: &Actor, e: &Enemy) -> Vec<i64> {
    let d = combat.data_of(a);
    let mut ids: Vec<i64> = a.resident.clone();
    if !a.anim.is_empty() {
        ids.extend(d.sp_effects_at(&a.anim, a.t).iter().map(|(id, _)| *id));
    }
    ids.extend(e.clash.iter().map(|c| c.0));
    ids.iter().filter_map(|id| d.sp_effects.get(&id.to_string())).map(|s| s.behavior_ref_id).collect()
}

/// Debug (H, with the hitboxes): bullets as lines, area bursts as their hit spheres.
fn draw_bullets(show: Option<Res<crate::combat::ShowHurtboxes>>, bullets: Query<(&Projectile, &Transform)>, mut gizmos: Gizmos) {
    if !show.is_some_and(|s| s.0) {
        return;
    }
    for (b, tf) in &bullets {
        if b.vel.length_squared() < 1e-4 {
            gizmos.sphere(Isometry3d::from_translation(tf.translation), b.spec.hit_radius.max(0.05), Color::srgb(1.0, 0.6, 0.2));
            continue;
        }
        let back = tf.translation - b.vel.normalize_or_zero() * 0.25;
        gizmos.line(back, tf.translation, Color::srgb(0.85, 0.9, 1.0));
    }
}

/// Wolf's behaviour script for the prosthetic button, ported from c0000_transition.lua:
/// BEH_A_GROUND_SUB_ATTACK (press, line 3523), BEH_A_GROUND_SUB_ATTACK_RELEASE (line 3750) with
/// its validate (6225), the sub-guard (Loaded Umbrella) damage reactions (1371-1386, 676-690) and
/// the Mist Raven's atemi (1152). Hanging, cover, forced crouch and swimming are left out.
pub mod hks {
    /// SpEffect behaviorRefIds (c0000_define.lua).
    pub const REF_ENABLE_SPRINT_ACTION: i64 = 1;
    pub const REF_SUB_ATTACK_COMBO: i64 = 300;
    pub const REF_SUB_ATTACK_RELEASE: i64 = 302;
    pub const REF_ATEMI_KAWARIMI: i64 = 305;
    pub const REF_DAMAGE_AFTER_KAWARIMI_FROM_ADD_DAMAGE: i64 = 307;
    pub const REF_USED_TEKIMAWASHI: i64 = 309;
    pub const REF_ENABLE_ADD_SUBATTACK_FAILED: i64 = 310;
    pub const REF_WEP_FORCE_SUB_ATTACK_RELEASE: i64 = 314;
    pub const REF_WEP_DISABLE_DERIVE_SUB_ATTACK_COMBO: i64 = 315;
    pub const REF_TRANSITION_COMBO_1_WP070: i64 = 317;
    pub const REF_TRANSITION_COMBO_2_WP070: i64 = 318;
    pub const REF_TRANSITION_COMBO_3_WP070: i64 = 319;
    pub const REF_TRANSITION_SUB_ATTACK_HOLD: i64 = 321;
    pub const REF_WEP_ENABLE_SUB_ATTACK_VARIATION: i64 = 322;
    pub const REF_WEP_ENABLE_SUB_ATTACK_HOLD: i64 = 325;
    pub const REF_SUB_ATTACK_USE_CHECK: i64 = 326;
    /// SP_EF_REF_ENABLE_PRESS_SUB_ATTACK (c0000_define 675; SpEffect 100368).
    pub const REF_ENABLE_PRESS_SUB_ATTACK: i64 = 311;
    /// SP_EF_REF_TAE_ENABLE_CANCEL_FLAMETHROWER (SpEffect 100290 "only the flamethrower cancels").
    pub const REF_ONLY_FLAMETHROWER_CANCEL: i64 = 329;
    pub const REF_POISON: i64 = 500;
    pub const REF_BURN: i64 = 1000130;

    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    pub enum Style {
        Stand,
        Sprint,
        Crouch,
    }

    /// What the script reads at a press: env(225, HAND_LEFT) = `cat` (the tool's wepmotionCategory),
    /// env(3035, ACTION_ARM_SHINOBI_WEP_ACTION) = `enable_action`, env(3036, ref) = `refs` (the
    /// anim's TAE SpEffects and the tool's resident), env(1118) = `locked`.
    pub struct Press<'a> {
        pub cat: i64,
        pub state: &'a str,
        pub style: Style,
        pub enable_action: bool,
        /// SP_EF_REF_TAE_ENABLE_SUB_ATTACK_COMBO and the last use was the same tool
        /// (g_beforeSubAttackType == subWeaponCategory).
        pub enable_combo: bool,
        pub refs: &'a dyn Fn(i64) -> bool,
        pub locked: bool,
        pub moving: bool,
    }

    /// BEH_A_GROUND_ATTACK out of a tool move (line 3199; ground 3251-3324, crouch 3211-3220): the
    /// attack button inside SP_EF_REF_TAE_ENABLE_SUB_ATTACK_DERIVE_ATTACK (301) follows up with the
    /// tool. `step` = the stick for the Sabimaru's directed cut (_SetStepAngle: None = neutral, else
    /// degrees, + = right). None: an ordinary attack.
    /// The Prosthetic Arts skill each follow-up asks for is `derive_unlock`.
    pub fn derive_attack(cat: i64, state: &str, crouch: bool, enable_combo: bool, refs: &dyn Fn(i64) -> bool, step: Option<f32>) -> Option<&'static str> {
        const REF_DERIVE_ATTACK: i64 = 301;
        const REF_GIMMICK_AXE_DERIVE_ATTACK_HORIZONTAL: i64 = 308;
        if !refs(REF_DERIVE_ATTACK) {
            return None;
        }
        if crouch {
            return match state {
                "CrouchSubAttackGuardStart" | "CrouchSubAttackGuardEnd" => Some("GroundSubAttackDeriveAttackCombo1"),
                "CrouchSubAttackCombo1Moveable" | "CrouchSubAttackLockOnMoveable" if cat == 79 => Some("GroundChargeSubAttackDeriveAttackCombo1"),
                "CrouchSubAttackCombo1ReleaseMoveable" | "CrouchSubAttackLockOnReleaseMoveable" if cat == 79 => Some("GroundSubAttackDeriveAttackCombo1"),
                _ => None,
            };
        }
        if !enable_combo {
            return None;
        }
        let combo2 = state == "GroundSubAttackCombo2";
        Some(match cat {
            70 | 71 => "GroundSubAttackDeriveAttackCombo1",
            78 if !combo2 && refs(REF_WEP_ENABLE_SUB_ATTACK_VARIATION) => match state {
                "GroundSubAttackVariationCombo1Release" | "SprintToSubAttackVariationRelease" | "LandAirSubAttackCombo1Variation" => "GroundSubAttackVariationDeriveAttackCombo1",
                _ => "GroundChargeSubAttackVariationDeriveAttackCombo1",
            },
            78 if !combo2 => "GroundSubAttackDeriveAttackCombo1",
            78 => "GroundSubAttackDeriveAttackCombo2",
            73 if refs(REF_GIMMICK_AXE_DERIVE_ATTACK_HORIZONTAL) => "GroundSubAttackDeriveAttackHorizontal",
            73 => "GroundSubAttackDeriveAttackCombo1",
            74 if state == "LandAirSubAttackMove" => "LandAirSubAttackMoveDeriveAttack",
            74 => "GroundSubAttackDeriveAttackCombo1",
            75 if refs(REF_WEP_DISABLE_DERIVE_SUB_ATTACK_COMBO) => return None,
            75 => {
                // _SetStepAngle: neutral V, |angle| < 45 F, > 135 B, else L / R.
                let dir = match step {
                    None => 0,
                    Some(a) if a.abs() < 45.0 => 1,
                    Some(a) if a.abs() > 135.0 => 2,
                    Some(a) if a < 0.0 => 3,
                    Some(_) => 4,
                };
                let derived = state.starts_with("GroundDeriveSubAttackCombo");
                match (derived, dir) {
                    (true, 0) => "GroundDeriveSubAttackDeriveDirectivityAttack_V",
                    (true, 1) => "GroundDeriveSubAttackDeriveDirectivityAttack_F",
                    (true, 2) => "GroundDeriveSubAttackDeriveDirectivityAttack_B",
                    (true, 3) => "GroundDeriveSubAttackDeriveDirectivityAttack_L",
                    (true, _) => "GroundDeriveSubAttackDeriveDirectivityAttack_R",
                    (false, 0) => "GroundSubAttackDeriveDirectivityAttack_V",
                    (false, 1) => "GroundSubAttackDeriveDirectivityAttack_F",
                    (false, 2) => "GroundSubAttackDeriveDirectivityAttack_B",
                    (false, 3) => "GroundSubAttackDeriveDirectivityAttack_L",
                    (false, _) => "GroundSubAttackDeriveDirectivityAttack_R",
                }
            }
            76 => "GroundSubAttackDeriveAttackCombo1",
            79 => match state {
                "GroundSubAttackCombo1Moveable" | "GroundSubAttackCombo1Move" | "GroundSubAttackLockOnMoveable" | "GroundSubAttackLockOnMove" => "GroundChargeSubAttackDeriveAttackCombo1",
                _ => "GroundSubAttackDeriveAttackCombo1",
            },
            72 => match state {
                "GroundSubAttackCombo1" | "SprintSubAttack" | "GroundSubAttackHoldEnd" => "GroundChargeSubAttackDeriveAttackCombo1",
                _ => "GroundSubAttackDeriveAttackCombo1",
            },
            77 => "GroundSubAttackDeriveAttackCombo1",
            _ => return None,
        })
    }

    /// The press's state (the W_ event without its prefix). "SubAttackFailed" /
    /// "AddSubAttackFailed" are the empty-handed clack.
    pub fn press(p: &Press) -> &'static str {
        let r = |id: i64| (p.refs)(id);
        let cat = p.cat;
        let sprint = p.style == Style::Sprint || r(REF_ENABLE_SPRINT_ACTION);
        let crouch = p.style == Style::Crouch;
        let fails = !(70..=79).contains(&cat)
            || (!p.enable_action && !matches!(cat, 75 | 77 | 78))
            || p.state == "GroundSubAttackGuardEndToSubAttackFailed"
            || (!p.enable_action && cat == 77 && !r(REF_USED_TEKIMAWASHI))
            || (!p.enable_action && cat == 78 && !p.enable_combo);
        let failed = || if r(REF_ENABLE_ADD_SUBATTACK_FAILED) { "AddSubAttackFailed" } else { "SubAttackFailed" };
        if fails {
            return failed();
        }
        match cat {
            70 => {
                // The first TAE transition ref that is up picks the combo step.
                let step = if r(REF_TRANSITION_COMBO_1_WP070) {
                    0
                } else if r(REF_TRANSITION_COMBO_2_WP070) {
                    1
                } else if r(REF_TRANSITION_COMBO_3_WP070) {
                    2
                } else {
                    0
                };
                if sprint {
                    "SprintSubAttack"
                } else if crouch {
                    ["CrouchSubAttackCombo1", "CrouchSubAttackCombo2", "CrouchSubAttackCombo3"][step]
                } else {
                    ["GroundSubAttackCombo1", "GroundSubAttackCombo2", "GroundSubAttackCombo3"][step]
                }
            }
            71 => {
                if sprint {
                    "SprintSubAttack"
                } else if crouch {
                    "CrouchSubAttackCombo1"
                } else {
                    "GroundSubAttackCombo1"
                }
            }
            72 => {
                if sprint {
                    "SprintSubAttack"
                } else if !r(REF_WEP_FORCE_SUB_ATTACK_RELEASE) && r(REF_WEP_ENABLE_SUB_ATTACK_HOLD) && p.enable_combo {
                    "GroundSubAttackHoldStart"
                } else {
                    "GroundSubAttackCombo1"
                }
            }
            73 => {
                if p.enable_combo {
                    "GroundSubAttackCombo2"
                } else if sprint {
                    "SprintSubAttack"
                } else {
                    "GroundSubAttackCombo1"
                }
            }
            74 => {
                if r(REF_DAMAGE_AFTER_KAWARIMI_FROM_ADD_DAMAGE) || r(REF_POISON) || r(REF_BURN) {
                    "SubAttackJumpReady"
                } else if sprint {
                    "SprintToSubAttackJumpAtemiReady"
                } else {
                    "SubAttackJumpAtemiReady"
                }
            }
            75 => {
                let s = p.state;
                let a = p.enable_action;
                if a && sprint {
                    "SprintSubAttack"
                } else if p.enable_combo {
                    if !r(REF_WEP_DISABLE_DERIVE_SUB_ATTACK_COMBO)
                        && (s == "GroundSubAttackDeriveAttackCombo1" || s.starts_with("GroundSubAttackDeriveDirectivityAttack"))
                    {
                        "GroundDeriveSubAttackCombo1"
                    } else if matches!(s, "GroundSubAttackCombo1" | "LandAirSubAttackCombo1" | "SprintSubAttack") {
                        "GroundSubAttackCombo2"
                    } else if a && s == "GroundSubAttackCombo2" {
                        "GroundSubAttackCombo3"
                    } else if s == "GroundSubAttackCombo3" {
                        "GroundSubAttackCombo4"
                    } else if a && s == "GroundSubAttackCombo4" {
                        "GroundSubAttackCombo5"
                    } else if s == "GroundSubAttackCombo5" {
                        "GroundSubAttackCombo6"
                    } else if a && s == "GroundDeriveSubAttackCombo1" {
                        "GroundDeriveSubAttackCombo2"
                    } else if s == "GroundDeriveSubAttackCombo2" {
                        "GroundDeriveSubAttackCombo3"
                    } else if a && s == "GroundDeriveSubAttackCombo3" {
                        "GroundDeriveSubAttackCombo4"
                    } else if s == "GroundDeriveSubAttackCombo4" {
                        "GroundDeriveSubAttackCombo5"
                    } else if a && s == "GroundDeriveSubAttackCombo5" {
                        "GroundDeriveSubAttackCombo6"
                    } else if a {
                        "GroundSubAttackCombo1"
                    } else {
                        failed()
                    }
                } else if a {
                    "GroundSubAttackCombo1"
                } else {
                    failed()
                }
            }
            76 => {
                // Only the sprint style, not the sprint-action ref, opens it from a sprint.
                if p.style == Style::Sprint {
                    "SprintToSubAttackGuardStart"
                } else if crouch {
                    "CrouchSubAttackGuardStart"
                } else {
                    "GroundSubAttackGuardStart"
                }
            }
            77 => {
                if sprint {
                    if r(REF_USED_TEKIMAWASHI) { "SprintToSubAttackSpecialEffect" } else { "SprintSubAttack" }
                } else if r(REF_USED_TEKIMAWASHI) {
                    "GroundSubAttackSpecialEffect"
                } else {
                    "GroundSubAttackCombo1"
                }
            }
            78 => {
                let combo_from = matches!(
                    p.state,
                    "GroundSubAttackCombo1"
                        | "GroundSubAttackCombo1Release"
                        | "SprintSubAttack"
                        | "SprintToSubAttackRelease"
                        | "LandAirSubAttackCombo1"
                        | "GroundSubAttackVariationCombo1Release"
                        | "SprintToSubAttackVariationRelease"
                        | "LandAirSubAttackCombo1Variation"
                );
                if sprint {
                    if r(REF_WEP_ENABLE_SUB_ATTACK_VARIATION) { "SprintToSubAttackVariation" } else { "SprintSubAttack" }
                } else if p.enable_combo && combo_from {
                    "GroundSubAttackCombo2"
                } else if r(REF_WEP_ENABLE_SUB_ATTACK_VARIATION) {
                    "GroundSubAttackVariationCombo1"
                } else {
                    "GroundSubAttackCombo1"
                }
            }
            // 79, the Finger Whistle: an upper-body action Wolf can walk with.
            _ => match (crouch, p.moving, p.locked) {
                (true, _, true) => "CrouchSubAttackLockOnMoveable",
                (true, _, false) => "CrouchSubAttackCombo1Moveable",
                (false, true, true) => "GroundSubAttackLockOnMove",
                (false, true, false) => "GroundSubAttackCombo1Move",
                (false, false, true) => "GroundSubAttackLockOnMoveable",
                (false, false, false) => "GroundSubAttackCombo1Moveable",
            },
        }
    }

    /// BEH_A_GROUND_SUB_ATTACK's validate (6220), the Flame Vent part: holding the button inside
    /// SP_EF_REF_TAE_TRANSITION_SUB_ATTACK_HOLD with a level that can spew (WEP_ENABLE_SUB_ATTACK_HOLD)
    /// counts as a press; so does SP_EF_REF_TAE_SUB_ATTACK_USE_CHECK without the emblems (the hold
    /// loop checks every 10 frames and fails into the clack).
    /// gap: ref 321's only SpEffect (100260) is put on by no TAE, BehaviorParam, SpEffect chain or exe
    /// constant found, so only it counts the hold. The flame's own follow-up window (the combo
    /// window 300 while "only the flamethrower cancels", 100290 ref 329; a072_400000 f75-96)
    /// counted it too until 2026-10-16: holding the button from the first burst went on into the
    /// spew. The skills recordings (rec_20261015_184118 tick 22283, rec_20261015_203558 tick
    /// 18923) only enter 400200 on a new press in that window (a 1.8 s hold from the first press
    /// did nothing); there the press path (`press`, enable_combo) takes it.
    pub fn held_press(cat: i64, held: bool, enable_action: bool, refs: &dyn Fn(i64) -> bool) -> bool {
        let transition = refs(REF_TRANSITION_SUB_ATTACK_HOLD);
        // The umbrella's clause (round 59): 076 held inside SP_EF_REF_ENABLE_PRESS_SUB_ATTACK
        // (311: 100368, the recovery windows of most a000 / a050 / a07x clips) opens it.
        (cat == 72 && held && transition && refs(REF_WEP_ENABLE_SUB_ATTACK_HOLD))
            || (cat == 76 && held && refs(REF_ENABLE_PRESS_SUB_ATTACK))
            || (!enable_action && refs(REF_SUB_ATTACK_USE_CHECK))
    }

    /// BEH_A_GROUND_SUB_ATTACK's validate (6220), its flamethrower clause: inside
    /// SP_EF_REF_TAE_ENABLE_CANCEL_FLAMETHROWER (329, "only the flamethrower cancels", a072 clips)
    /// only a Flame Vent level that can spew (072 with WEP_ENABLE_SUB_ATTACK_HOLD 325) takes a press
    /// (round 50; before, any tool's tap there replayed Combo1 and paid again).
    pub fn flame_gate(cat: i64, refs: &dyn Fn(i64) -> bool) -> bool {
        !refs(REF_ONLY_FLAMETHROWER_CANCEL) || (cat == 72 && refs(REF_WEP_ENABLE_SUB_ATTACK_HOLD))
    }

    /// BEH_A_GROUND_SUB_ATTACK_RELEASE's validate (6225): inside SP_EF_REF_TAE_ENABLE_SUB_ATTACK_RELEASE
    /// with the button up, or a tool that always releases (SP_EF_REF_WEP_FORCE_SUB_ATTACK_RELEASE).
    pub fn releases(refs: &dyn Fn(i64) -> bool, held: bool) -> bool {
        refs(REF_SUB_ATTACK_RELEASE) && (refs(REF_WEP_FORCE_SUB_ATTACK_RELEASE) || !held)
    }

    /// The release's state (line 3750) for the state it leaves.
    pub fn release(cat: i64, state: &str, crouch: bool, moving: bool, locked: bool) -> Option<&'static str> {
        // BEH_A_AIR_SUB_ATTACK_RELEASE (line 3087): in the air only the umbrella closes.
        if state.starts_with("AirSubAttack") {
            return (cat == 76).then_some("AirSubAttackGuardEnd");
        }
        let sprint = state == "SprintSubAttack";
        match cat {
            70 => match state {
                "GroundSubAttackCombo1" => Some("GroundSubAttackCombo1Release"),
                "GroundSubAttackCombo2" => Some("GroundSubAttackCombo2Release"),
                "GroundSubAttackCombo3" => Some("GroundSubAttackCombo3Release"),
                "CrouchSubAttackCombo1" => Some("CrouchSubAttackCombo1Release"),
                "CrouchSubAttackCombo2" => Some("CrouchSubAttackCombo2Release"),
                "CrouchSubAttackCombo3" => Some("CrouchSubAttackCombo3Release"),
                "SprintSubAttack" => Some("SprintToSubAttackRelease"),
                _ => None,
            },
            71 => match state {
                "GroundSubAttackCombo1" => Some("GroundSubAttackCombo1Release"),
                "CrouchSubAttackCombo1" => Some("CrouchSubAttackCombo1Release"),
                "SprintSubAttack" => Some("SprintToSubAttackRelease"),
                _ => None,
            },
            76 => Some("GroundSubAttackGuardEnd"),
            79 => Some(match (crouch, moving, locked) {
                (true, true, true) => "CrouchSubAttackLockOnReleaseMove",
                (true, true, false) => "CrouchSubAttackCombo1ReleaseMove",
                (true, false, true) => "CrouchSubAttackLockOnReleaseMoveable",
                (true, false, false) => "CrouchSubAttackCombo1ReleaseMoveable",
                (false, true, true) => "GroundSubAttackLockOnReleaseMove",
                (false, true, false) => "GroundSubAttackCombo1ReleaseMove",
                (false, false, true) => "GroundSubAttackLockOnReleaseMoveable",
                (false, false, false) => "GroundSubAttackCombo1ReleaseMoveable",
            }),
            72 if sprint => Some("SprintToSubAttackRelease"),
            72 if matches!(state, "GroundSubAttackHoldStart" | "GroundSubAttackHoldLoop" | "GroundSubAttackHoldMove") => Some("GroundSubAttackHoldEnd"),
            72 => Some("GroundSubAttackCombo1Release"),
            78 => match state {
                "SprintSubAttack" => Some("SprintToSubAttackRelease"),
                "SprintToSubAttackVariation" => Some("SprintToSubAttackVariationRelease"),
                "GroundSubAttackVariationCombo1" => Some("GroundSubAttackVariationCombo1Release"),
                "GroundSubAttackCombo1" => Some("GroundSubAttackCombo1Release"),
                _ => None,
            },
            73 if sprint => Some("SprintToSubAttackRelease"),
            73 => Some("GroundSubAttackCombo1Release"),
            _ => None,
        }
    }

    /// HKS AIR_SUB_ATTACK_COUNT_MAX (c0000_define 208): tool uses per jump for 071/072/074/075/077/078/079
    /// (g_airSubAttackCount, back to 0 in _LandReset).
    pub const AIR_SUB_ATTACK_COUNT_MAX: u32 = 1;

    /// BEH_A_AIR_SUB_ATTACK (line 3007): the prosthetic pressed in the air. `count` = the uses so far
    /// this jump. Returns the state and whether it counts as a use; None = the press is dropped
    /// (W_AddActionInputSubAttack, the input blend).
    /// ACTION_UNLOCK_TYPE_AIR_SUB_ATTACK (lines 2992 / 3026) is checked by the caller.
    pub fn air_press(cat: i64, enable_action: bool, count: u32, locked: bool, refs: &dyn Fn(i64) -> bool) -> Option<(&'static str, bool)> {
        let counted = matches!(cat, 71 | 72 | 74 | 75 | 77 | 78 | 79);
        if counted && count >= AIR_SUB_ATTACK_COUNT_MAX {
            return None;
        }
        let enable = (70..=79).contains(&cat) && (enable_action || (cat == 77 && refs(REF_USED_TEKIMAWASHI)));
        if !enable {
            return Some((if refs(REF_ENABLE_ADD_SUBATTACK_FAILED) { "AddSubAttackFailed" } else { "SubAttackFailedAir" }, false));
        }
        let state = match cat {
            73 => "AirSubAttackStart",
            74 if refs(REF_DAMAGE_AFTER_KAWARIMI_FROM_ADD_DAMAGE) || refs(REF_POISON) || refs(REF_BURN) => "AirSubAttackMoveReady",
            74 => "AirSubAttackMoveAtemiReady",
            76 => "AirSubAttackGuardStart",
            77 if refs(REF_USED_TEKIMAWASHI) => "AirSubAttackSpecialEffect",
            79 if locked => "AirSubAttackLockOn",
            70 if refs(REF_TRANSITION_COMBO_2_WP070) && !refs(REF_TRANSITION_COMBO_1_WP070) => "AirSubAttackCombo2",
            70 if refs(REF_TRANSITION_COMBO_3_WP070) && !refs(REF_TRANSITION_COMBO_1_WP070) && !refs(REF_TRANSITION_COMBO_2_WP070) => "AirSubAttackCombo3",
            78 if refs(REF_WEP_ENABLE_SUB_ATTACK_VARIATION) => "AirSubAttackCombo1Variation",
            // 077 without ref 309 takes the last branch, "071 or 077" (HKS 3083): Combo1, the air
            // use a077_403000 (round 49; before, dropped as no event).
            _ => "AirSubAttackCombo1",
        };
        Some((state, counted))
    }

    /// The ACTION_UNLOCK_TYPE a ground follow-up needs (lines 3257 / 3274 / 3308 / 3320): Chasing
    /// Slice (070 / 071 / 078 but after Combo2), Fang and Blade (073 / 074 / 078 Combo2), Projected
    /// Force (076 / 079), Living Force (072 / 077). None: the crouch ones and the Sabimaru (075)
    /// check none.
    pub fn derive_unlock(cat: i64, state: &str, crouch: bool) -> Option<u32> {
        use crate::combat::*;
        if crouch {
            return None;
        }
        Some(match cat {
            70 | 71 => UNLOCK_SUB_ATTACK_DIRAVE_ATTACK_1,
            78 if state != "GroundSubAttackCombo2" => UNLOCK_SUB_ATTACK_DIRAVE_ATTACK_1,
            73 | 74 | 78 => UNLOCK_SUB_ATTACK_DIRAVE_ATTACK_2,
            76 | 79 => UNLOCK_SUB_ATTACK_SHOT_ATTACK,
            72 | 77 => UNLOCK_SUB_ATTACK_ENCHANT,
            _ => return None,
        })
    }

    /// BEH_R_LAND for the air tool states (lines 1888-1970): while SP_EF_REF_TAE_ENABLE_ORIGINAL_LAND_ACTION
    /// (201) is up the state goes on as its Land version from the same time (`true`); the loops land
    /// from their start (`false`). None: the plain landing.
    /// W_LandAirSubAttackStart's state is LandAirSubAttackStart, whose CMSG the game spells
    /// "LandAirSubAttacStart_CMSG" (c0000.hkx; anim 403050), so that is its exported name.
    /// Without ref 201 the start and the loop land as W_LandAirSubAttackLoop (lines 1892 / 1932).
    /// gap: for the tools but the Firecracker (73) the HKS checks a fall of 20 m or more
    /// (FALL_HEIGHT_LONG_STIFF_LAND, the free-fall landing) first; fall height is not tracked.
    pub fn air_land(_cat: i64, state: &str, ref_201: bool) -> Option<(&'static str, bool)> {
        Some(match state {
            "AirSubAttackDeflectEasySmall" | "AirSubAttackGuardLoop" => ("LandAirSubAttackGuardLoop", false),
            "AirSubAttackStart" if ref_201 => ("LandAirSubAttacStart", true),
            "AirSubAttackStart" | "AirSubAttackLoop" => ("LandAirSubAttackLoop", false),
            "AirSubAttackDeriveAttack" if ref_201 => ("LandAirSubAttackDeriveAttack", true),
            "AirSubAttackDeriveAttackLoop" if ref_201 => ("LandAirSubAttackDeriveAttackLoop", false),
            "AirSubAttackMoveReady" if ref_201 => ("LandAirSubAttackMoveReady", true),
            s if s == "AirSubAttackMoveReady" || s.starts_with("AirSubAttackMoveStart_") => ("LandAirSubAttackMove", false),
            "AirSubAttackCombo1" if ref_201 => ("LandAirSubAttackCombo1", true),
            "AirSubAttackCombo2" if ref_201 => ("LandAirSubAttackCombo2", true),
            "AirSubAttackCombo3" if ref_201 => ("LandAirSubAttackCombo3", true),
            "AirSubAttackSpecialEffect" if ref_201 => ("LandAirSubAttackSpecialEffect", true),
            "AirSubAttackLockOn" if ref_201 => ("LandAirSubAttackLockOn", true),
            "AirSubAttackCombo1Variation" if ref_201 => ("LandAirSubAttackCombo1Variation", true),
            "AirSubAttackGuardStart" if ref_201 => ("LandAirSubAttackGuardStart", true),
            "AirSubAttackGuardEnd" if ref_201 => ("LandAirSubAttackGuardEnd", true),
            "AirSubAttackMoveAtemiReady" if ref_201 => ("LandAirSubAttackMoveAtemiReady", true),
            _ => return None,
        })
    }

    /// STATE_TYPE_*_SUB_GUARD states (c0000_cmsg.lua g_paramHkbState): the Loaded Umbrella is open.
    pub fn sub_guard(state: &str) -> bool {
        matches!(
            state,
            "LandAirSubAttackGuardStart"
                | "LandAirSubAttackGuardLoop"
                | "GroundSubAttackDeflectEasy"
                | "GroundSubAttackDeflectEasyLarge_F"
                | "GroundSubAttackDeflectEasyLarge_B"
                | "GroundSubAttackGuardAction"
                | "GroundSubAttackGuardStart"
                | "SprintToSubAttackGuardStart"
                | "GroundSubAttackGuardLoop"
                | "CrouchSubAttackGuardStart"
                | "AirSubAttackDeflectEasySmall"
                | "AirSubAttackGuardStart"
                | "AirSubAttackGuardLoop"
        ) || state.starts_with("GroundSubAttackGuardMoveLoop")
    }

    /// A blocked or deflected hit on the open umbrella (line 1371; also from GroundSubAttackGuardEnd):
    /// damage levels small / middle / large / push / small blow / fling -> GroundSubAttackDeflectEasy;
    /// ex-large / upper / ex-blast -> GroundSubAttackDeflectEasyLarge by the hit's front/back side.
    /// None = another level (minimum: the additive reaction, `umbrella_add`).
    pub fn umbrella_guard(level: i64, from_behind: bool) -> Option<&'static str> {
        match level {
            1 | 2 | 3 | 5 | 6 | 7 => Some("GroundSubAttackDeflectEasy"),
            4 | 9 | 10 => Some(if from_behind { "GroundSubAttackDeflectEasyLarge_B" } else { "GroundSubAttackDeflectEasyLarge_F" }),
            _ => None,
        }
    }

    /// The additive guard reaction on the open umbrella (BEH_ADD_R_GUARD_DAMAGE, line 676): a just
    /// deflect -> GroundSubAttackDeflectHardAdd (GroundSubAttackBreakDeflectAdd when it emptied the
    /// attacker's posture), else GroundSubAttackDeflectEasyAdd.
    pub fn umbrella_add(just: bool, attacker_broken: bool) -> &'static str {
        match (just, attacker_broken) {
            (true, true) => "GroundSubAttackBreakDeflectAdd",
            (true, false) => "GroundSubAttackDeflectHardAdd",
            _ => "GroundSubAttackDeflectEasyAdd",
        }
    }

    /// The Mist Raven's atemi (line 1152): a hit inside SP_EF_REF_TAE_ENABLE_ATEMI_KAWARIMI with the
    /// tool usable becomes W_SubAttackJumpReady (the feathers and the leap away).
    pub fn mist_raven_atemi(cat: i64, enable_action: bool, refs: &dyn Fn(i64) -> bool) -> bool {
        cat == 74 && enable_action && refs(REF_ATEMI_KAWARIMI)
    }

    /// SubAttackJumpReady's end (FireStateEndEvent, line 536): SubAttackJumpStart_<dir> from the
    /// stick's angle to the facing (degrees, + = right, as player::directional_jump); no stick = V.
    /// _SetJumpDirection (c0000_transition.lua 4975, PRM_GROUND_JUMP_*_STICK_RANGE in
    /// c0000_define.lua): F within +-18.75 (ends included), the others strictly inside their
    /// ranges (FR 18.75-67.5, R -112.5, BR -157.5, mirrored for the left), anything else
    /// (also an exact border such as 67.5) B.
    pub fn jump_start(stick_angle: Option<f32>) -> &'static str {
        let Some(a) = stick_angle else { return "SubAttackJumpStart_V" };
        let inside = |lo: f32, hi: f32| a > lo && a < hi;
        if (-18.75..=18.75).contains(&a) {
            "SubAttackJumpStart_F"
        } else if inside(18.75, 67.5) {
            "SubAttackJumpStart_FR"
        } else if inside(67.5, 112.5) {
            "SubAttackJumpStart_R"
        } else if inside(112.5, 157.5) {
            "SubAttackJumpStart_BR"
        } else if inside(-157.5, -112.5) {
            "SubAttackJumpStart_BL"
        } else if inside(-112.5, -67.5) {
            "SubAttackJumpStart_L"
        } else if inside(-67.5, -18.75) {
            "SubAttackJumpStart_FL"
        } else {
            "SubAttackJumpStart_B"
        }
    }

    /// AirSubAttackMoveReady's end (line 552): W_AirSubAttackMoveStart, the same eight ways
    /// (AirSubAttackMoveStart_<dir>, a074_41901x).
    pub fn air_move_start(stick_angle: Option<f32>) -> &'static str {
        match jump_start(stick_angle) {
            "SubAttackJumpStart_F" => "AirSubAttackMoveStart_F",
            "SubAttackJumpStart_FR" => "AirSubAttackMoveStart_FR",
            "SubAttackJumpStart_R" => "AirSubAttackMoveStart_R",
            "SubAttackJumpStart_BR" => "AirSubAttackMoveStart_BR",
            "SubAttackJumpStart_B" => "AirSubAttackMoveStart_B",
            "SubAttackJumpStart_BL" => "AirSubAttackMoveStart_BL",
            "SubAttackJumpStart_L" => "AirSubAttackMoveStart_L",
            "SubAttackJumpStart_FL" => "AirSubAttackMoveStart_FL",
            _ => "AirSubAttackMoveStart_V",
        }
    }
}

#[cfg(test)]
mod tests {
    /// c9997's transition ranks: burning (3) may not cut a posture break or a large blow, the
    /// Firecracker's stagger (4) not a burning reaction either; both cut ordinary states.
    #[test]
    fn reactions_respect_the_transition_rank() {
        use super::{rank_allows, transition_rank};
        assert!(rank_allows(transition_rank("Idle"), 3) && rank_allows(transition_rank("Idle"), 4));
        assert!(!rank_allows(transition_rank("TrunkCollapseFront"), 3));
        assert!(!rank_allows(transition_rank("DamageLargeBlow"), 4));
        assert!(rank_allows(transition_rank("FireReaction"), 3) && !rank_allows(transition_rank("FireReaction"), 4));
        assert!(rank_allows(transition_rank("GuardDamageSmall_RighttoLeft"), 4));
        assert_eq!(transition_rank("AttackBoundEmptyStaminaEnemy_Left"), 2);
    }
}
