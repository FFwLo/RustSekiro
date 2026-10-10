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
    /// Base damage of the weapon that fired it (EquipParamWeapon attackBasePhysics / Fire /
    /// Thunder: the tool level's, or Kusabimaru 5000's for the Lightning Reversal bolt).
    base: (f32, f32, f32),
    vel: Vec3,
    travelled: f32,
    age: f32,
    /// The throw it belongs to (its children share the hit list).
    chain: u32,
    /// Age of the next intervalCreateBulletId child.
    next_interval: f32,
    /// Characters this bullet already hit.
    hit: Vec<Entity>,
    /// Its flying effect (sfxId_Bullet, or the parent's with isInheritSfxToChild).
    fx: Option<Entity>,
    /// The effect its children inherit (-1 = none).
    inherit_sfx: i64,
}

/// Is the game's effect for this FFX id extracted (extracted/fxr/<id>.json)?
fn has_fxr(id: i64) -> bool {
    id > 0 && crate::paths::extracted().join(format!("fxr/{id}.json")).exists()
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
    time: Res<Time>,
    mut chain: Local<u32>,
) {
    for (a, tf, mut p, dummies) in &mut player {
        let dt = time.delta_secs();
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
        // (And the sword's: the Lightning Reversal's TAE 4 BulletBehavior_Midair in a050_308900.)
        let tool_anim = crate::player::is_tool_anim(&a.anim);
        let crossing = |e: &&crate::data::Event| (e.kind == 2 || (e.kind == 4 && !tool_anim)) && crosses(e);
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
            // The sword's bullets (outside the tool groups): Kusabimaru's variation 5000
            // (BehaviorParam_PC 105000184 -> Bullet 500184) and its attackBase*.
            let sword = (!tool_anim).then(|| d.bullets.get(&format!("v5000:{judge}"))).flatten();
            let spec = sword.or_else(|| tool.and_then(|t| d.bullets.get(&format!("v{}:{judge}", t.variation)))).or_else(|| d.bullets.get(&judge.to_string()));
            let Some(spec) = spec else { continue };
            let base = if sword.is_some() {
                let w = combat.param("EquipParamWeapon", 5000);
                let f = |k: &str| w[k].as_f64().unwrap_or(0.0) as f32;
                (f("attackBasePhysics"), f("attackBaseFire"), f("attackBaseThunder"))
            } else {
                tool.map_or((20.0, 0.0, 0.0), |t| (t.attack_base_physics, t.attack_base_fire, 0.0))
            };
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
            spawn_row(&mut commands, spec, from, dir, *chain, base, -1);
        }
    }
}

/// Spawns one Bullet row at `pos`: numShoot bullets fanned from `dir` by shootAngle +
/// i * shootAngleInterval (yaw, degrees) and tilted by shootAngleXZ (-90 = straight down).
fn spawn_row(commands: &mut Commands, spec: &BulletSpec, pos: Vec3, dir: Vec3, chain: u32, base: (f32, f32, f32), inherited: i64) {
    let flat = dir.with_y(0.0).normalize_or(Vec3::NEG_Z);
    let sfx = if spec.sfx_id_bullet > 0 { spec.sfx_id_bullet } else { inherited };
    let inherit_sfx = if spec.is_inherit_sfx_to_child != 0 { sfx } else { -1 };
    // An area burst (standing still, >= 1 m: the firecracker's 710003) without its FXR extracted:
    // sparks from vfx.rs stand in.
    if spec.init_vellocity == 0.0 && spec.hit_radius >= 1.0 && !has_fxr(sfx) {
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
        let at = Transform::from_translation(pos).with_rotation(Quat::from_rotation_arc(Vec3::NEG_Z, d.normalize_or(Vec3::NEG_Z)));
        let bullet = commands.spawn((at, Name::new(format!("Bullet {}", spec.bullet_id)))).id();
        // The flying effect rides on the bullet (FxEffect anchored to it).
        let fx = (sfx > 0).then(|| commands.spawn(crate::fxr::FxEffect::new(sfx, Some(crate::fxr::Anchor::Entity(bullet)), true, at)).id());
        commands.entity(bullet).insert(Projectile {
            spec: spec.clone(),
            base,
            vel: d * spec.init_vellocity,
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
    mut statuses: ResMut<crate::status::Statuses>,
    mut effects: Query<&mut crate::fxr::FxEffect>,
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
                spawn_row(&mut commands, child, to, dir, b.chain, b.base, b.inherit_sfx);
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
                b.hit.push(ee);
                if b.spec.is_use_shared_hit_list != 0 {
                    shared.0.entry(b.chain).or_default().push(ee);
                }
                if b.spec.is_penetrate == 0 {
                    ended = true;
                }
                let msg = hit_enemy(&combat, &b, from, to, part, &mut da, dtf, &mut e, time.elapsed_secs(), sounds.as_deref_mut());
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
                    sps.extend(b.spec.attack.as_ref().map(|a| a.sp_effects()).unwrap_or_default());
                    let lines = statuses.hit(&combat, ee, &da, &sps, m.contains("blocks"));
                    // gap: catching fire flails him even through his guard (as seen in the game); the
                    // exe's damage call that sets STATUS_BURNING is not traced.
                    let ignited = lines.iter().any(|l| l == "BURNING");
                    for line in lines {
                        if let Some(log) = log.as_mut() {
                            log.push(line, Color::srgb(0.7, 0.55, 0.9));
                        }
                    }
                    let burning = statuses.has_ref(&combat, ee, crate::status::REF_BURNING) && combat.data_of(&da).states.contains_key("FireReaction");
                    if (m.starts_with("hit") || ignited) && burning && !da.posture_broken() && !e.is_broken() && rank_allows(transition_rank(&da.state), 3) {
                        e.react(&mut da, &combat, "FireReaction");
                    }
                }
                if ended {
                    break;
                }
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
        "TrunkCollapseLarge" | "TrunkCollapseBurst" | "TrunkCollapseLightningStart" | "TrunkCollapseLightningLoop" | "TrunkCollapseLightningEnd" => 1,
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
    now: f32,
    mut sounds: Option<&mut crate::sound::SoundQueue>,
) -> Option<String> {
    let sps = [b.spec.sp_effect_id0, b.spec.sp_effect_id1, b.spec.sp_effect_id2, b.spec.sp_effect_id3, b.spec.sp_effect_id4];
    let sp_of = |id: i64| combat.player.sp_effects.get(&id.to_string());
    let refs: Vec<i64> = sps.iter().filter(|v| **v > 0).filter_map(|id| sp_of(*id)).map(|s| s.behavior_ref_id).collect();
    if refs.contains(&REF_BURST_NONCRITICAL) || refs.contains(&REF_BURST_CRITICAL) {
        let npc = combat.npc(da);
        let special = (0..32)
            .filter_map(|i| npc[format!("spEffectID{i}")].as_i64().filter(|v| *v > 0))
            .any(|id| combat.data_of(da).sp_effects.get(&id.to_string()).is_some_and(|s| s.behavior_ref_id == REF_BURST_ENABLE));
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
        // c9997.lua GetSpDamage -> SP_DAMAGE_BURST -> W_AssassinationBloodReaction (a000_020110),
        // a rank 4 reaction (line 3091): not out of a rank 0-3 state (large blows, posture
        // breaks, burning / lightning reactions).
        if !e.is_broken() && rank_allows(transition_rank(&da.state), 4) {
            e.react(da, combat, "AssassinationBloodReaction");
            return Some("firecracker: enemy staggered".into());
        }
        return None;
    }
    let atk = b.spec.attack.as_ref()?;
    // The bullet's SpEffects on the enemy for their effectEndurance (the reversal bolt's
    // 3520085 "Divine Dragon: Lightning Reversal hit", 1 s, which the dragon's map script
    // waits for).
    for id in sps.iter().filter(|v| **v > 0) {
        let secs = crate::data::sp_effect_endurance(combat, *id);
        e.add_timed(*id, secs.max(0.1));
    }
    if !(atk.atk_phys_correction > 0.0 || atk.atk_fire_correction > 0.0 || atk.atk_thun_correction > 0.0 || atk.atk_stam > 0.0 || atk.direct_atk_stam_damage > 0.0) {
        return None;
    }
    let dd = combat.data_of(da);
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
    let fire_cut = combat.npc(da)["fireDamageCutRate"].as_f64().unwrap_or(1.0) as f32;
    let thun_cut = combat.npc(da)["thunderDamageCutRate"].as_f64().unwrap_or(1.0) as f32;
    let dmg = b.base.0 * atk.atk_phys_correction / 100.0 + b.base.1 * atk.atk_fire_correction / 100.0 * fire_cut + b.base.2 * atk.atk_thun_correction / 100.0 * thun_cut;
    let pd = if atk.direct_atk_stam_damage > 0.0 { atk.direct_atk_stam_damage } else { atk.atk_stam };
    da.hp = (da.hp - dmg).max(0.0);
    da.add_posture(pd);
    apply_knockback(combat, da, from, dtf.translation, atk.knockback_hit, damage_kb(atk.dmg_level), "knockbackRate_vsPlayer_DirectHit");
    // c9997 GetSpDamage: a lightning hit (DAMAGE_ELEMENT_LIGHTNING 6) on one with
    // SP_EFFECT_REF_LIGHTNING_DAMAGE_ENABLE (1000041: its NpcParam's 6021) and without
    // NO_LIGHTNING_DAMAGE (1000260) is SP_DAMAGE_LIGHTNING -> W_DamageLightningStart, a rank 3
    // reaction (line 2995) even mid-swing.
    let lightning = atk.sp_attribute == 6 && {
        let refs = enemy_refs(combat, da, e);
        refs.contains(&1000041) && !refs.contains(&1000260)
    };
    if lightning && !da.posture_broken() && !e.is_broken() && rank_allows(transition_rank(&da.state), 3) {
        e.react(da, combat, "DamageLightningStart");
    } else if !da.posture_broken() && !e.is_broken() {
        match enemy_damage_state(atk.dmg_level, da, from, dtf.translation).filter(|_| !e.is_attacking()) {
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
    /// constant found; the flame's own follow-up window stands in for it: the combo window (300) while
    /// "only the flamethrower cancels" (100290, ref 329; a072_400000 f75-96, 400100 f21-33).
    pub fn held_press(cat: i64, held: bool, enable_action: bool, refs: &dyn Fn(i64) -> bool) -> bool {
        let transition = refs(REF_TRANSITION_SUB_ATTACK_HOLD) || (refs(REF_SUB_ATTACK_COMBO) && refs(REF_ONLY_FLAMETHROWER_CANCEL));
        (cat == 72 && held && transition && refs(REF_WEP_ENABLE_SUB_ATTACK_HOLD)) || (!enable_action && refs(REF_SUB_ATTACK_USE_CHECK))
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
            // 077 without ref 309 falls through every branch (no event).
            77 => return None,
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
