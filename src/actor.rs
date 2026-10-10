//! A character playing Sekiro animations: timeline clock, root motion,
//! HP and posture (Sekiro calls posture "stamina" internally).
//!
//! Runs in FixedUpdate at 60 Hz, Sekiro's simulation rate.

use bevy::prelude::*;

use crate::config::GameConfig;
use crate::data::{CharData, Combat, calc_correct};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Side {
    Player,
    Enemy,
}

#[derive(Component)]
pub struct Actor {
    pub side: Side,
    /// Behavior state name (player) or a label (enemy). Shown in the debug HUD.
    pub state: String,
    /// Anim key into combat_data ("a050_300000"), or "" for procedural states.
    pub anim: String,
    pub t: f32,
    pub prev_t: f32,
    /// Facing angle (radians, Bevy Y rotation; 0 faces -Z).
    pub yaw: f32,
    pub hp: f32,
    pub hp_max: f32,
    /// Posture damage taken, 0..posture_max. Full = posture break.
    pub posture: f32,
    pub posture_max: f32,
    /// How far posture may overshoot max once broken: NpcParam maxDebtStamina (c1020 -30,
    /// stamina goes into debt below 0), so a heavy breaking hit lengthens the recovery.
    pub posture_debt: f32,
    /// NPC guard arc half-angle [deg] (NpcParam guardAngle); 0 = use the TAE ShieldBlock ArgB.
    pub guard_angle: f32,
    /// Base posture recovery per second (before StaminaControlParam and HP scaling).
    pub posture_regen: f32,
    /// StaminaControlParam row used for per-state recovery ratios.
    pub stamina_ctrl_row: i64,
    pub since_posture_damage: f32,
    /// Fractional posture recovery carried to the next frame (exe: ChrDataModule+0x15c).
    pub regen_carry: f32,
    /// Remaining hit stop (AtkParam hitStopTime): the animation clock is frozen.
    pub hit_stop: f32,
    /// Resident SpEffect ids (NpcParam spEffectID slots for enemies).
    pub resident: Vec<i64>,
    /// AttackBehavior windows already resolved in the current anim: (judge, start).
    pub hits_done: Vec<(i64, i32)>,
    /// This character's bullets that touched someone (target, the bullet's AtkParam): combat.rs
    /// resolves them like a connected AttackBehavior window (enemy_bullet.rs fires NPC bullets).
    pub bullet_hits: Vec<(Entity, crate::data::Attack)>,
    /// Additive layer (HKS AddDeflectGuardBlend): an anim whose TAE runs on top of the base
    /// state without replacing it, e.g. AddHardDeflectGuard's deflect window. Empty = none.
    pub add_anim: String,
    pub add_t: f32,
    /// Vertical motion for jumps (Sekiro jump arcs are not root motion: TAE 920 sets the
    /// launch velocity from ChrPhysicsVelocityChangeParam, then data::FALL_TYPES applies).
    pub vel_y: f32,
    /// Gravity applied to vel_y this frame: the move uses the exact step v dt + g dt^2 / 2 (the live
    /// game's per-frame heights match the analytic arc).
    pub grav_y: f32,
    pub airborne: bool,
    /// Fall control: ChrPhysicsVelocityChangeParam.fallType, launch velocity (decays by
    /// the fall type's horizontal acceleration) and the stick-driven part on top.
    pub fall_type: u8,
    pub air_base: Vec3,
    /// Height where a SetNoGravity root lift began (enemies; actor.rs advance puts them back).
    pub lift_ground: Option<f32>,
    /// Root-motion scale from TAE 760 BoostRootMotionToReachTarget (1.0 outside it).
    pub root_scale: f32,
    /// Root-motion direction offset (radians): guard walk moves along the stick, between its
    /// directional clips (the game's blend), not straight along the clip.
    pub root_yaw: f32,
    /// Locomotion clip choice (HKS MoveSpeedIndex 0 walk / 1 run, from the stick; MoveDirection
    /// 0 F / 1 B / 2 L / 3 R relative to the facing) and the move-start clip's length when the
    /// move began from a standstill (StandMoveStart a000_0001xx / 0004xx, then the loop
    /// StandMoveLoop a000_0002xx / 0005xx); 0 = straight into the loop.
    pub move_index: u8,
    pub move_dir: u8,
    pub move_start: f32,
    /// Weapon style None (TAE 32 SetWeaponStyle): the sword is sheathed.
    pub sheathed: bool,
    /// Crouching (HKS STYLE_TYPE_CROUCH): idle and locomotion use the Crouch* clips, the stand
    /// ids + 5000 (c0000.hkx CMSGs: CrouchIdle 5000, CrouchWalkStart_F 5100, ...).
    pub crouch: bool,
    /// Lower-body twist (exe WalkTwist -> behavior variable TwistMasterAngle, radians, + = legs
    /// turned right): the move direction's angle from the shown directional clip, so diagonal
    /// moves run along the stick instead of sliding; converges to `twist_target` at 0.1 rad per
    /// 1/30 s (SpinJointBehavior, constant-rate mode). anim.rs applies it.
    pub twist: f32,
    pub twist_target: f32,
    /// Attack auto-homing target while not locked on (CSChrAutoHomingModule +0x38).
    pub homing: Option<Entity>,
    /// One of this actor's attacks connected (hit, block or deflect) since it last looked:
    /// exe action module +0x58 bit 4, set in the hit processing FUN_1409e5fd0 (HKS env 2004).
    pub attack_hit: bool,
    /// TAE 920 already applied in the current anim.
    pub vel_change_done: bool,
    /// Attack multipliers from area scaling: HP damage, posture damage dealt.
    pub atk_rate: f32,
    pub stam_atk_rate: f32,
    /// Engine knockback (AtkParam knockbackDist_*, KnockBackParam times): horizontal
    /// velocity held for `kb_cont` seconds, then decelerated to 0 over `kb_dec_total`.
    pub kb_vel: Vec3,
    pub kb_cont: f32,
    pub kb_dec: f32,
    pub kb_dec_total: f32,
    /// Procedural horizontal velocity for locomotion and jumps (world space).
    pub move_vel: Vec3,
    /// NPC anim set (the "Anime ID offset" SpEffects 200030-200034, stateInfo 270-274: a000-a400,
    /// e.g. the spear General c1021 a100, Isshin's second phase a100) and the anims this character
    /// has. A behavior state (CMSG offsetType 15 = AnimIdOffset) then plays a<group>00_<id>.
    /// gap: when the set lacks the id the a000 anim plays (the exe's fallback is not traced).
    pub anim_group: u32,
    pub anim_keys: Option<std::sync::Arc<std::collections::HashSet<String>>>,
}

impl Actor {
    /// Starts a knockback of `dist` metres along `dir` with KnockBackParam times
    /// (`cont` at full speed, then linear to 0 over `dec`): v = dist / (cont + dec / 2).
    pub fn knockback(&mut self, dir: Vec3, dist: f32, cont: f32, dec: f32) {
        let dir = dir.with_y(0.0).normalize_or_zero();
        if dist <= 0.0 || dir == Vec3::ZERO || cont + dec <= 0.0 {
            return;
        }
        self.kb_vel = dir * (dist / (cont + dec * 0.5));
        self.kb_cont = cont;
        self.kb_dec = dec;
        self.kb_dec_total = dec;
    }

    pub fn new(side: Side, hp: f32, posture_max: f32, posture_regen: f32, stamina_ctrl_row: i64) -> Self {
        Self {
            side,
            state: "StandIdle".into(),
            anim: String::new(),
            t: 0.0,
            prev_t: 0.0,
            yaw: 0.0,
            hp,
            hp_max: hp,
            posture: 0.0,
            posture_max,
            posture_debt: 0.0,
            guard_angle: 0.0,
            posture_regen,
            stamina_ctrl_row,
            since_posture_damage: 99.0,
            regen_carry: 0.0,
            hit_stop: 0.0,
            resident: Vec::new(),
            hits_done: Vec::new(),
            bullet_hits: Vec::new(),
            anim_group: 0,
            anim_keys: None,
            add_anim: String::new(),
            add_t: 0.0,
            vel_y: 0.0,
            grav_y: 0.0,
            airborne: false,
            fall_type: 0,
            air_base: Vec3::ZERO,
            lift_ground: None,
            root_scale: 1.0,
            root_yaw: 0.0,
            move_index: 0,
            move_dir: 0,
            move_start: 0.0,
            sheathed: false,
            crouch: false,
            twist: 0.0,
            twist_target: 0.0,
            homing: None,
            attack_hit: false,
            vel_change_done: false,
            atk_rate: 1.0,
            stam_atk_rate: 1.0,
            kb_vel: Vec3::ZERO,
            kb_cont: 0.0,
            kb_dec: 0.0,
            kb_dec_total: 0.0,
            move_vel: Vec3::ZERO,
        }
    }

    pub fn play(&mut self, state: &str, anim: &str) {
        self.state = state.to_string();
        self.anim = self.grouped(anim);
        self.t = 0.0;
        self.prev_t = 0.0;
        self.hits_done.clear();
        self.vel_change_done = false;
    }

    /// `key` in this actor's anim set: a000_<id> -> a<group>00_<id> when the set has that clip.
    /// An a000 entry without a clip (the Corrupted Monk's a000_013700 Todome / 020200 final death
    /// are empty placeholders; the clips are a100_013700 / a100_020200) takes the first set that
    /// has one. gap: the exe's lookup order is not traced.
    pub fn grouped(&self, key: &str) -> String {
        let Some(keys) = self.anim_keys.as_ref().filter(|_| key.starts_with("a000_")) else { return key.to_string() };
        let alt = |g: u32| format!("a{g}00{}", &key[4..]);
        if self.anim_group > 0 && keys.contains(&alt(self.anim_group)) {
            return alt(self.anim_group);
        }
        if !keys.contains(key) {
            if let Some(g) = (1..=4).find(|g| keys.contains(&alt(*g))) {
                return alt(g);
            }
        }
        key.to_string()
    }

    /// Plays a behavior state through the data's state -> anim table.
    pub fn play_state(&mut self, data: &CharData, state: &str) -> bool {
        match data.anim_key(state) {
            Some(key) => {
                let key = key.to_string();
                self.play(state, &key);
                true
            }
            None => {
                warn!("state {state} has no exported anim");
                false
            }
        }
    }

    /// Switches to `state` at the current time and keeps the hits already landed: HKS
    /// `SetVariable("StartTime_00", env(3063, 0) / 1000)` before a land-continuation event
    /// (AirComboAttackN -> LandAirComboAttackN share the first frames of their TAE).
    pub fn continue_state(&mut self, data: &CharData, state: &str) -> bool {
        let (t, hits) = (self.t, std::mem::take(&mut self.hits_done));
        if !self.play_state(data, state) {
            self.hits_done = hits;
            return false;
        }
        self.t = t;
        self.prev_t = t;
        self.hits_done = hits;
        true
    }

    pub fn procedural(&mut self, state: &str) {
        if self.state != state || !self.anim.is_empty() {
            self.play(state, "");
        }
    }

    /// Locomotion clip: the move start (a000_0001xx walk / 0004xx run) or loop (0002xx / 0005xx)
    /// for move_index and move_dir (c0000.hkx StandWalk/RunStart / Loop *_CMSG animIds).
    pub fn move_clip(&self, start: bool) -> String {
        self.move_clip_as(self.move_index, start)
    }

    /// The locomotion clip shown at time t (the state's clock): (key, time in it, loops). In
    /// Locomotion the move start plays first, then the loop; under an upper action the legs run
    /// the loop on the action's clock.
    pub fn locomotion_clip_at(&self, t: f32) -> (String, f32, bool) {
        if self.state == "Locomotion" && t < self.move_start {
            (self.move_clip(true), t, false)
        } else if self.state == "Locomotion" {
            (self.move_clip(false), t - self.move_start, true)
        } else {
            (self.move_clip(false), t, true)
        }
    }

    /// move_clip for a given MoveSpeedIndex. Sheathed, the forward clips come from the a010 set
    /// (c0000_a00x.anibnd: walk / run start and loop, forward only; the sword-less arm swing).
    pub fn move_clip_as(&self, index: u8, start: bool) -> String {
        let base = match (index, start) {
            (0, true) => 100,
            (0, false) => 200,
            (_, true) => 400,
            (_, false) => 500,
        };
        if self.crouch {
            return format!("a000_{:06}", 5000 + base + self.move_dir.min(3) as u32);
        }
        let group = if self.sheathed && self.move_dir == 0 { "a010" } else { "a000" };
        format!("{group}_{:06}", base + self.move_dir.min(3) as u32)
    }

    /// The clip a player shows and its time: the TAE anim, else the procedural idle (StandIdle
    /// a000_000000 / CrouchIdle a000_005000) or locomotion clip. For reading the TAE of what is
    /// on screen (crouch's stealth SpEffect 109200 rides on the crouch idle / move clips).
    pub fn shown_clip(&self, d: &CharData) -> (String, f32) {
        if !self.anim.is_empty() {
            return (self.anim.clone(), self.t);
        }
        let (key, t) = if self.state == "Locomotion" {
            let (key, t, _) = self.locomotion_clip_at(self.t);
            (key, t)
        } else {
            ((if self.crouch { "a000_005000" } else { "a000_000000" }).to_string(), self.t)
        };
        let len = d.length(&key);
        (key, if len > 0.0 { t.rem_euclid(len) } else { t })
    }

    pub fn frame(&self) -> f32 {
        self.t * crate::data::TAE_FPS
    }

    pub fn forward(&self) -> Vec3 {
        Quat::from_rotation_y(self.yaw) * Vec3::NEG_Z
    }

    pub fn add_posture(&mut self, amount: f32) {
        if amount > 0.0 {
            self.posture = (self.posture + amount).min(self.posture_max + self.posture_debt);
            self.since_posture_damage = 0.0;
        }
    }

    pub fn posture_broken(&self) -> bool {
        self.posture >= self.posture_max
    }
}

pub fn data_for<'a>(combat: &'a Combat, side: Side) -> &'a CharData {
    match side {
        Side::Player => &combat.player,
        Side::Enemy => &combat.enemy,
    }
}

/// TAE 760 BoostRootMotionToReachTarget (exe: the TAE handler FUN_140b293a0 stores its args,
/// FUN_1407f0a70 sets the root-motion scale chr+0x2fc each frame from FUN_140842980): the
/// arrival point lies ArriveDist short of the target, turned by ArriveAngle; the scale is
/// clamp(distance to it, EnableRangeMin, EnableRangeMax) / ReferenceDist (RangeMin when
/// already inside ArriveDist, 1.0 without the event). So a slash lengthens its lunge toward
/// a far target and shortens it on a close one. Target: the lock-on target for Wolf, the
/// player for the enemy; unlocked, Wolf's attack auto-homing target (player.rs).
fn boost_root_motion(
    combat: Res<Combat>,
    lock: Option<Res<crate::camera::LockOn>>,
    mut q: Query<(Entity, &mut Actor, &Transform)>,
) {
    let positions: Vec<(Entity, Side, Vec3)> = q.iter().map(|(e, a, t)| (e, a.side, t.translation)).collect();
    for (_, mut a, tf) in &mut q {
        let d = data_for(&combat, a.side);
        let ev = if a.anim.is_empty() { None } else { d.events_at(&a.anim, a.t).find(|e| e.kind == 760) };
        let Some(ev) = ev.filter(|e| e.args.get("IsEnable").and_then(|v| v.as_bool()).unwrap_or(false)) else {
            a.root_scale = 1.0;
            continue;
        };
        let target = match a.side {
            // FUN_1407f0a70: the lock target, else the attack's auto-homing target.
            Side::Player => lock.as_ref().and_then(|l| l.target).or(a.homing).and_then(|t| positions.iter().find(|p| p.0 == t)).map(|p| p.2),
            Side::Enemy => positions.iter().find(|p| p.1 == Side::Player).map(|p| p.2),
        };
        let Some(target) = target else { continue };
        let f = |k: &str| ev.args.get(k).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
        let (reference, min, max, angle, arrive) =
            (f("ReferenceDist"), f("EnableRangeMin"), f("EnableRangeMax"), f("ArriveAngleFromTarget"), f("ArriveDistFromTarget"));
        if reference.abs() < f32::MIN_POSITIVE {
            a.root_scale = 1.0;
            continue;
        }
        let away = tf.translation - target;
        let dist = if away.length() < arrive {
            min
        } else {
            let dir = Quat::from_rotation_y(angle.to_radians()) * away.normalize_or_zero();
            // The exe's order (0x140842afd-b10: comiss min, then max), not f32::clamp: Gyoubu's
            // (c5080) events have EnableRangeMin 6 > Max 4.
            let d = ((target + dir * arrive) - tf.translation).length();
            if d <= min {
                min
            } else if d >= max {
                max
            } else {
                d
            }
        };
        a.root_scale = dist / reference;
    }
}

/// Advances clocks, applies root motion and procedural motion.
pub fn advance(time: Res<Time>, combat: Res<Combat>, mut q: Query<(&mut Actor, &mut Transform)>) {
    let dt = time.delta_secs();
    for (mut a, mut tf) in &mut q {
        a.prev_t = a.t;
        if a.hit_stop > 0.0 {
            a.hit_stop -= dt;
            continue;
        }
        a.t += dt;
        a.since_posture_damage += dt;
        let data = data_for(&combat, a.side);
        // Additive layer clock: it ends after max(clip length, last TAE event).
        if !a.add_anim.is_empty() {
            a.add_t += dt;
            let end = data.anim(&a.add_anim).map_or(0.0, |an| an.events.iter().map(|e| e.end).fold(an.duration.unwrap_or(0.0), f32::max));
            if a.add_t > end {
                a.add_anim.clear();
            }
        }
        if !a.anim.is_empty() {
            if let (Some((p0, y0)), Some((p1, y1))) = (data.root_at(&a.anim, a.prev_t), data.root_at(&a.anim, a.t)) {
                // The root track is in the clip's start frame: the step turns with the facing the
                // clip started from (current yaw minus the clip's own turn so far), not the current
                // facing. Live (rec_c1010_20261009, behind deathblow a200_511200 / ThrowDef13200, both
                // spin 180 deg): the enemy ends 0.6 m in front of Wolf; turning the steps with the
                // current yaw slid him 1 m sideways.
                let delta = Quat::from_rotation_y(a.yaw - y0 + a.root_yaw) * Vec3::new(p1.x - p0.x, 0.0, p1.y - p0.y) * a.root_scale;
                tf.translation += delta;
                a.yaw += y1 - y0;
                // While ChrActionFlag 27 SetNoGravity holds a grounded character, the clip's own
                // vertical root motion lifts it (the vault 511900 goes 1.5 m up; live: Wolf's
                // height +1.53 m, landing 1.4 s in).
                if a.side != Side::Enemy && !a.airborne && data.flag(&a.anim, a.t, crate::player::FLAG_NO_GRAVITY) {
                    tf.translation.y += data.root_y_at(&a.anim, a.t) - data.root_y_at(&a.anim, a.prev_t);
                }
            }
        }
        // Enemies have no fall of their own: lifted (SetNoGravity), the height is the clip's vertical
        // root above where the lift began; after it (or an interrupted leap) they are put back there
        // instead of left hanging.
        if a.side == Side::Enemy && !a.airborne {
            let lifted = !a.anim.is_empty() && data.flag(&a.anim, a.t, crate::player::FLAG_NO_GRAVITY);
            let rise = if lifted { data.root_y_at(&a.anim, a.t).max(0.0) } else { 0.0 };
            match (lifted, a.lift_ground) {
                (true, None) => {
                    a.lift_ground = Some(tf.translation.y);
                    tf.translation.y += rise;
                }
                (true, Some(g)) => tf.translation.y = g + rise,
                (false, Some(g)) => {
                    tf.translation.y = g;
                    a.lift_ground = None;
                }
                (false, None) => {}
            }
        }
        let v = a.move_vel;
        tf.translation += v * dt;
        if a.kb_cont > 0.0 {
            tf.translation += a.kb_vel * dt;
            a.kb_cont -= dt;
        } else if a.kb_dec > 0.0 {
            tf.translation += a.kb_vel * (a.kb_dec / a.kb_dec_total.max(1e-4)) * dt;
            a.kb_dec -= dt;
        }
        // Gravity and landing are handled by the owner (player.rs, data::FALL_TYPES).
        if a.airborne {
            tf.translation.y += (a.vel_y - 0.5 * a.grav_y * dt) * dt;
        }
        tf.rotation = Quat::from_rotation_y(a.yaw);
    }
}

/// Posture recovery, as in the exe's ChrIns update (FUN_140a04850):
///   points/s = baseRecover x (ratio(type) / 100 if a StaminaControlParam type is set) x level multiplier
/// added in whole points each frame with the fraction carried over.
/// baseRecover: NpcParam.staminaRecoverBaseVel (enemy) or CalcCorrectGraph 504 (player).
/// The TAE event 960 type is -1 ("none", 100%) when no event is active.
/// The level multiplier chr.f[0x10d0] is Dark Souls' equip-load class (FUN_14084d0c0: weight /
/// (40 + stat) > 0.7 -> 0.8, > 1.0 or SpEffect stateInfo 102 -> 0.7, else 1.0). Every Sekiro
/// weapon/protector weighs 0, so it is 1.0 here.
pub fn regen_posture(time: Res<Time>, combat: Res<Combat>, config: Res<GameConfig>, mut q: Query<&mut Actor>) {
    let dt = time.delta_secs();
    let graph = combat.param("CalcCorrectGraph", 51).clone();
    for mut a in &mut q {
        if a.posture <= 0.0 || a.since_posture_damage < config.posture.regen_delay {
            a.regen_carry = 0.0;
            continue;
        }
        let data = data_for(&combat, a.side);
        let ty = if a.anim.is_empty() { None } else { data.stamina_ratio_type(&a.anim, a.t) };
        let ty = ty.unwrap_or(config.posture.default_ratio_type);
        let row = combat.param("StaminaControlParam", a.stamina_ctrl_row);
        let ratio = if ty < 0 { 1.0 } else { row[format!("staminaRecoverRatio_forType{ty:03}")].as_f64().unwrap_or(100.0) as f32 / 100.0 };
        let hp_factor = if config.posture.hp_scales_regen { calc_correct(&graph, a.hp / a.hp_max) } else { 1.0 };
        // Resident HP-conditional SpEffects (NpcParam spEffectID slots) multiply regen
        // through staminaRecoverSpeedRate; the exe multiplies all active ones (FUN_140bfe360).
        let hp_pct = a.hp / a.hp_max * 100.0;
        let resident: f32 = a
            .resident
            .iter()
            .filter_map(|id| data.sp_effects.get(&id.to_string()))
            .filter(|s| s.condition_hp <= 0.0 || hp_pct <= s.condition_hp)
            .map(|s| s.stamina_recover_speed_rate)
            .product();
        // TAE 225 SetSPRegenRatePercent (c1020 guard / deflect 3100-3102: 33 %, fire reactions 0 %).
        let tae_pct = if a.anim.is_empty() {
            1.0
        } else {
            data.events_at(&a.anim, a.t)
                .find(|e| e.kind == 225)
                .and_then(|e| e.arg_i64("RegenRatePercent"))
                .map_or(1.0, |p| p as f32 / 100.0)
        };
        let acc = a.posture_regen * ratio * hp_factor * resident * tae_pct * dt + a.regen_carry;
        let whole = acc.floor();
        a.regen_carry = acc - whole;
        a.posture = (a.posture - whole).max(0.0);
    }
}

pub struct ActorPlugin;

impl Plugin for ActorPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Time::<Fixed>::from_hz(60.0))
            .add_systems(FixedUpdate, (boost_root_motion, advance, separate, regen_posture).chain().in_set(ActorSet::Advance));
    }
}

/// Character hit radii for body push-out. Enemy: NpcParam.hitRadius (0.5).
/// gap: the player's character-proxy radius lives in c0000's physics hkx (not
/// extracted); 0.4 m stands in.
pub const PLAYER_BODY_RADIUS: f32 = 0.4;

/// Wolf's paired-throw states and the enemy's throw reactions (not the start throws: live, Wolf's
/// start anim stops against the enemy - the c1010 vault start moved 0.18 m with him 0.95 m away).
fn in_throw(a: &Actor) -> bool {
    a.state.starts_with("ThrowDef")
        || matches!(a.state.as_str(), "Deathblow" | "ThrowBreak" | "BreakKickJump" | "Mikiri" | "PlungeDeathblow")
}

/// Characters don't overlap: pairs closer than the sum of their radii are pushed
/// apart horizontally, half each (both weigh in like the game's char proxies).
fn separate(combat: Res<Combat>, mut q: Query<(&Actor, &mut Transform)>) {
    let npc_radius = combat.param("NpcParam", combat.foe.npc_row)["hitRadius"].as_f64().unwrap_or(0.5) as f32;
    let radius = |a: &Actor| if a.side == Side::Player { PLAYER_BODY_RADIUS } else { npc_radius };
    let mut items: Vec<_> = q.iter_mut().collect();
    for i in 0..items.len() {
        for j in i + 1..items.len() {
            let (l, r) = items.split_at_mut(j);
            let (a, ta) = &mut l[i];
            let (b, tb) = &mut r[0];
            // Throw pairs pass through each other (the game's throws drop the character proxies).
            if a.hp <= 0.0 || b.hp <= 0.0 || in_throw(a) || in_throw(b) {
                continue;
            }
            let min = radius(a) + radius(b);
            let d = (tb.translation - ta.translation).with_y(0.0);
            let len = d.length();
            if len < min {
                let n = if len > 1e-4 { d / len } else { Vec3::X };
                // A start throw walks into its target without moving it (live: the broken
                // enemy stays put while Wolf's 501900 stops against him); the sprint deflect's
                // slide likewise stops against the enemy (gap: proxy weights not traced).
                let stops = |x: &Actor| x.state == "DeathblowStart" || x.state == "SprintToDeflectGuard";
                let (wa, wb) = match (stops(a), stops(b)) {
                    (true, false) => (1.0, 0.0),
                    (false, true) => (0.0, 1.0),
                    _ => (0.5, 0.5),
                };
                let push = n * (min - len);
                ta.translation -= push * wa;
                tb.translation += push * wb;
            }
        }
    }
}

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum ActorSet {
    /// Player/enemy decision making (reads input, picks states).
    Decide,
    /// Clock + root motion.
    Advance,
    /// Hit detection and reactions.
    Resolve,
}
