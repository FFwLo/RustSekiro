//! A character playing Sekiro animations: timeline clock, root motion,
//! HP and posture (Sekiro calls posture "stamina" internally).
//!
//! Runs in FixedUpdate at 60 Hz, Sekiro's simulation rate.

use bevy::prelude::*;

use crate::config::GameConfig;
use crate::data::{CharData, Combat, calc_correct};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Side {
    Player,
    Enemy,
}

/// The super-armour pool (exe CSSuperArmorDurability, re-scout 2026-10-13): max = NpcParam
/// superArmorDurability (vfunc 0x2c8, FUN_140a12de0: +0x118), 0 = none (Wolf, most enemies).
/// A hit drains it (FUN_140bdb470, <decomp ref>; `combat::super_armor_hit`) and re-arms
/// `timer` to 5 x (superArmorRecoverCorrection + 1) s (vfunc 0x2d8, FUN_140a12e90: 7 s at 0.4,
/// 25 s at 4.0); when it runs out the pool snaps back to full (<decomp ref>, no gradual
/// refill). Emptied, it breaks: the stateInfo-120 dmgLv rows drop out (FUN_140bdbaf0) for that
/// hit, and the next tick refills it (the hold is TAE ChrActionFlag 71, which no Sekiro anim
/// has). While it holds, `combat::damage_motion_level` takes them.
/// The worn stateInfo-155 rows add their changeSuperArmorPoint to the max (`bonus`,
/// FUN_140bdb690 <decomp ref>: the sum kept at +0x24, a rise also added to the pool at
/// once): the headless Ape's 3510991 +120, "revived Kensei" 3540110 +200 (no source puts it on
/// anyone: no TAE, NpcParam, EMEVD, AI or exe reference; likely unused), the
/// "SA up" 2090 +40.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SuperArmor {
    pub max: f32,
    pub cur: f32,
    /// The stateInfo-155 bonus last seen.
    pub bonus: f32,
    /// Seconds to the refill; <= 0 idle (the exe's -0.01).
    pub timer: f32,
    pub broken: bool,
    pub delay: f32,
}

impl SuperArmor {
    pub fn new(max: f32, recover_correction: f32) -> Self {
        Self { max, cur: max, bonus: 0.0, timer: -0.01, broken: false, delay: 5.0 * (recover_correction + 1.0) }
    }

    /// The pool's full size: NpcParam's plus the stateInfo-155 bonus.
    pub fn full(&self) -> f32 {
        self.max + self.bonus
    }

    /// FUN_140bdbaf0: on, unbroken and not empty.
    pub fn holds(&self) -> bool {
        self.full() > 0.0 && self.cur > 0.0 && !self.broken
    }

    /// FUN_140bdb660: back to full and unbroken (a posture break, a respawn).
    pub fn refill(&mut self) {
        self.cur = self.full();
        self.broken = false;
        self.timer = -0.01;
    }

    /// The tick (FUN_140bdbcb0 -> <decomp ref>). `reacting`: the action flag that holds a
    /// broken pool, (chr+0x50)+0x8c bit 9 (<decomp ref>), set only by TAE ChrActionFlag 71
    /// "SetBool32_0x7C_9" (FUN_140b51b70 case 0x47, <decomp ref>); `instant`: a worn
    /// stateInfo-331 SpEffect skips the wait (FUN_140bfdb20).
    /// `bonus`: the worn stateInfo-155 rows' changeSuperArmorPoint sum.
    pub fn tick(&mut self, dt: f32, reacting: bool, instant: bool, bonus: f32) {
        if bonus.abs() >= 1.192_092_9e-7 {
            let rise = bonus - self.bonus;
            self.bonus = bonus;
            if rise > 0.0 {
                self.cur += rise;
            }
        } else {
            self.bonus = 0.0;
        }
        if self.full() <= 0.0 {
            return;
        }
        if self.broken {
            if !reacting {
                self.refill();
            }
            return;
        }
        if self.timer > 0.0 {
            self.timer -= dt;
            if self.timer <= 0.0 || instant {
                self.refill();
            }
        }
    }

    /// FUN_140bdb470: drain by `amount` from the pool cut to its full size (a bonus that ended
    /// leaves no extra); empty -> broken. A broken pool still drains and restarts the timer.
    /// Returns true on the break. Checked against the exe in tools/emu-cases
    /// (super_armor_hit_matches_the_exe). gap: the +0x18 reserve that soaks the overflow
    /// before a break is 0 wherever it is set (FUN_140bdb690); its other source is not traced.
    pub fn hit(&mut self, amount: f32) -> bool {
        let full = self.full();
        if full <= 0.0 || amount <= 0.0 {
            return false;
        }
        let was = self.broken;
        self.cur = self.cur.min(full) - amount;
        self.timer = self.delay;
        if self.cur <= 0.0 {
            self.cur = 0.0;
            self.broken = true;
            self.timer = -0.01;
            return !was;
        }
        false
    }
}

#[derive(Component)]
pub struct Actor {
    pub side: Side,
    /// Enemy: its kind in `Combat::kinds` (0 for the player).
    pub kind: usize,
    /// Behavior state name (player) or a label (enemy). Shown in the debug HUD.
    pub state: String,
    /// Anim key into combat_data ("a050_300000"), or "" for procedural states.
    pub anim: String,
    pub t: f32,
    pub prev_t: f32,
    /// The generator the current clip blends in from while its TAE Blend lasts (the behaviour
    /// graph keeps playing it and mixes its world-from-model in): (clip key, its clock), the key
    /// empty for a procedural state whose motion is `blend_from_vel`. None once the blend is over
    /// or when the transition drops the source motion (data::blend_keeps_src).
    pub blend_from: Option<(String, f32)>,
    /// The from clip loops (c0000.hkx MODE_LOOPING): its clock wraps past its end.
    pub blend_from_loops: bool,
    pub blend_from_vel: Vec3,
    /// The velocity that moved the body in the last step (`move_vel` as `advance` applied it;
    /// the action starters zero `move_vel` before they play).
    pub last_move_vel: Vec3,
    /// Seconds into the blend-in and its length (the clip's TAE Blend; < 0 = not read yet).
    pub blend_t: f32,
    pub blend_len: f32,
    /// Facing angle (radians, Bevy Y rotation; 0 faces -Z).
    pub yaw: f32,
    /// The drawn facing while it lags `yaw` (Wolf's free turns: `ShownYaw`); None draws `yaw`.
    pub shown: Option<ShownYaw>,
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
    /// The posture bounds of the TAE 960 StaminaControlParam type this frame (`stamina_limits`,
    /// as posture damage): None without a type. Set by regen_posture, kept by add_posture.
    pub posture_limits: Option<(f32, f32)>,
    /// The clip's TAE SpEffects of the last frame and the ones that outlive their TAE window
    /// (`run_sp_effect_chains`): (SpEffect id, seconds left).
    pub tae_prev: Vec<i64>,
    pub sp_chained: Vec<(i64, f32)>,
    /// Hit stop (`HitStop`, started by combat.rs start_hit_stop): scales this actor's clock.
    pub hit_stop: HitStop,
    /// This frame's step of the character's own clock (`advance`: the hit-stop-scaled dt, 0 in the
    /// hold); None before the first advance. The exe runs posture regen (FUN_140a04850), status
    /// recovery (FUN_140bda460) and the SpEffect timers (FUN_140a02a10 -> FUN_140c06b70) on the
    /// per-character FD4Time chr+0x100 whose float chr+0x108 FUN_1409f78a0 writes from
    /// HitStop+0x18 (<decomp ref>); only the fall / Havok take the raw dt (round 64).
    pub clock_dt: Option<f32>,
    /// Debug menu "no posture damage" (player_cheats sets it each tick): add_posture adds
    /// nothing, so one blocked hit can't fill the bar inside Resolve (round 65).
    pub no_posture: bool,
    /// Resident SpEffect ids (NpcParam spEffectID slots for enemies).
    pub resident: Vec<i64>,
    /// AttackBehavior windows already resolved in the current anim, per defender: (defender,
    /// judge, start). One window hits each target once, so a swing through two enemies hurts both.
    pub hits_done: Vec<(Entity, i64, i32)>,
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
    /// Wolf's Lightning Reversal charge: the SpEffect a lightning hit in the air put on him
    /// (9490 "charged enchantment [weak lightning]" stateInfo 356 / 9495 "[strong thunder]" 151,
    /// effectEndurance 30 s) and its time left. gap: the exe applies it (no param references
    /// 9490-9495); the weak one's strength 1-5 is not traced (9490 here).
    pub electro: Option<(i64, f32)>,
    /// Its opacity: the last TAE 193 keyframe value (CharData::opacity_key), 1 at spawn.
    pub opacity: f32,
    /// The camera's fade of it (Wolf only: camera.rs cam_fade_step), times `opacity` when drawn.
    pub cam_alpha: f32,
    /// Fall control: ChrPhysicsVelocityChangeParam.fallType, launch velocity (decays by
    /// the fall type's horizontal acceleration) and the stick-driven part on top.
    pub fall_type: u8,
    pub air_base: Vec3,
    /// Height where a SetNoGravity root lift began (enemies; actor.rs advance puts them back).
    pub lift_ground: Option<f32>,
    /// The lift's height over `lift_ground` where the current clip's took over (a lifted clip
    /// chained from another: Lady Butterfly's 3010 at 4 m into 3200), the clip it was taken for,
    /// and the fall speed back down after the lift.
    pub lift_off: f32,
    pub lift_anim: String,
    pub lift_vy: f32,
    /// Root-motion scale from TAE 760 BoostRootMotionToReachTarget (1.0 outside it).
    pub root_scale: f32,
    /// The clip's root turn is scaled by this (enemy turn-in-place clips turned to the asked
    /// angle, enemy.rs start_turn); 1 again on every play.
    pub yaw_scale: f32,
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
    /// The last hit on this actor came from behind (HKS damage_angle_FB == B): Wolf's death
    /// picks GroundDeathStart_B by it (BEH_R_DEATH, c0000_transition 974-983).
    pub hit_from_back: bool,
    /// ... in the 4-way back sector (HKS damage_angle == B, |front| >= |side|): a small-blow
    /// posture break lands face down only then (c0000_transition 1678-1684).
    pub hit_back_quadrant: bool,
    /// The behaviorRefId of the status row whose damage emptied HP (status.rs): 590 terror,
    /// 500 a poison tick (damage level none); 0 otherwise. BEH_R_DEATH picks the death by it.
    pub death_ref: i64,
    /// The killing direct hit's (HKS damage level env 236, element env 285 = spAttribute),
    /// set by combat.rs; BEH_R_DEATH picks Wolf's death start by it (player.rs death_starts).
    pub death_hit: Option<(i64, i64)>,
    /// A lightning charge enchantment (SpEffect 9490-9495) an enemy hit put on Wolf on the
    /// ground; player.rs sets off its shock on his next tick (`lightning_shock`).
    pub shock: Option<i64>,
    /// The aging row worn (SpEffect ref 591, 9620 "Aging_invoking"; status.rs set_aging): its
    /// maxHpRate on hp_max and the row in `resident` for its damage rates; player.rs plays the
    /// aged form while it is up.
    pub aged: Option<i64>,
    /// The super-armour pool ([`SuperArmor`]; enemies with NpcParam superArmorDurability).
    pub super_armor: SuperArmor,
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
    /// Travel left for the current knockback (`limit_knockback`; INFINITY = none).
    pub kb_left: f32,
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
    /// A negative `dist` pulls: the Spear's Combo2 pull (v7800 judges 115-117,
    /// knockbackDist_DirectHit -8 / -6 / -4 "Spear_pull") moves the target back along `dir`,
    /// toward the attacker; the caller caps it at the bodies' gap (`limit_knockback`).
    pub fn knockback(&mut self, dir: Vec3, dist: f32, cont: f32, dec: f32) {
        let dir = dir.with_y(0.0).normalize_or_zero() * dist.signum();
        let dist = dist.abs();
        if dist <= 0.0 || dir == Vec3::ZERO || cont + dec <= 0.0 {
            return;
        }
        self.kb_vel = dir * (dist / (cont + dec * 0.5));
        self.kb_cont = cont;
        self.kb_dec = dec;
        self.kb_dec_total = dec;
        self.kb_left = f32::INFINITY;
    }

    /// Caps the current knockback's travel (a pull stops at the attacker's capsule).
    pub fn limit_knockback(&mut self, max: f32) {
        self.kb_left = max.max(0.0);
    }

    pub fn new(side: Side, hp: f32, posture_max: f32, posture_regen: f32, stamina_ctrl_row: i64) -> Self {
        Self {
            side,
            kind: 0,
            state: "StandIdle".into(),
            anim: String::new(),
            t: 0.0,
            prev_t: 0.0,
            blend_from: None,
            blend_from_loops: false,
            blend_from_vel: Vec3::ZERO,
            last_move_vel: Vec3::ZERO,
            blend_t: 0.0,
            blend_len: 0.0,
            yaw: 0.0,
            shown: None,
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
            posture_limits: None,
            tae_prev: Vec::new(),
            sp_chained: Vec::new(),
            hit_stop: HitStop::default(),
            clock_dt: None,
            no_posture: false,
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
            electro: None,
            opacity: 1.0,
            cam_alpha: 1.0,
            fall_type: 0,
            air_base: Vec3::ZERO,
            lift_ground: None,
            lift_off: 0.0,
            lift_anim: String::new(),
            lift_vy: 0.0,
            root_scale: 1.0,
            root_yaw: 0.0,
            yaw_scale: 1.0,
            move_index: 0,
            move_dir: 0,
            move_start: 0.0,
            sheathed: false,
            crouch: false,
            twist: 0.0,
            twist_target: 0.0,
            homing: None,
            attack_hit: false,
            hit_from_back: false,
            hit_back_quadrant: false,
            death_ref: 0,
            death_hit: None,
            shock: None,
            aged: None,
            super_armor: SuperArmor::default(),
            vel_change_done: false,
            atk_rate: 1.0,
            stam_atk_rate: 1.0,
            kb_vel: Vec3::ZERO,
            kb_cont: 0.0,
            kb_dec: 0.0,
            kb_dec_total: 0.0,
            kb_left: f32::INFINITY,
            move_vel: Vec3::ZERO,
        }
    }

    pub fn play(&mut self, state: &str, anim: &str) {
        // The transition keeps the source generator's motion through the blend-in (c0000.hkx
        // TaeBlend) or drops it (TaeBlend_NoSrcMotion): see `advance`.
        // A clip restarted on its own key is the generator looping (hkbClipGenerator
        // MODE_LOOPING), not a transition: no blend-in.
        let to = self.grouped(anim);
        if to != self.anim || to.is_empty() {
            self.blend_from = if !to.is_empty() && crate::data::blend_keeps_src(state, self.side == Side::Player) { Some((self.anim.clone(), self.t)) } else { None };
            self.blend_from_loops = self.side == Side::Player && crate::data::loops_c0000(&self.anim);
            self.blend_from_vel = self.last_move_vel;
            self.blend_t = 0.0;
            self.blend_len = -1.0;
        }
        self.state = state.to_string();
        self.anim = to;
        self.t = 0.0;
        self.prev_t = 0.0;
        self.hits_done.clear();
        self.vel_change_done = false;
        self.yaw_scale = 1.0;
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
                // Once per state and character kind (the callers retry every frame).
                static SEEN: std::sync::Mutex<Option<std::collections::HashSet<(Side, usize, String)>>> = std::sync::Mutex::new(None);
                let first = SEEN.lock().map_or(true, |mut s| s.get_or_insert_with(Default::default).insert((self.side, self.kind, state.to_string())));
                if first {
                    warn!("state {state} has no exported anim (kind {}; logged once)", self.kind);
                }
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

    /// This frame's step of the character's clock (`clock_dt`), `real` before its first advance.
    pub fn clock(&self, real: f32) -> f32 {
        self.clock_dt.unwrap_or(real)
    }

    pub fn add_posture(&mut self, amount: f32) {
        if amount > 0.0 && !self.no_posture {
            self.posture = self.bound_posture(self.posture + amount);
            self.since_posture_damage = 0.0;
        }
    }

    /// The posture setter's bounds (exe FUN_140bd6710 -> FUN_140bd4e20, on every change: hits and
    /// regen): from full to NpcParam maxDebtStamina below empty, then inside the StaminaControlParam
    /// type's limits when TAE 960 sets one (`posture_limits`).
    pub fn bound_posture(&self, v: f32) -> f32 {
        let v = v.clamp(0.0, self.posture_max + self.posture_debt);
        match self.posture_limits {
            Some((lo, hi)) => v.clamp(lo, hi),
            None => v,
        }
    }

    pub fn posture_broken(&self) -> bool {
        self.posture >= self.posture_max
    }
}

/// An actor's character data: Wolf's, or its enemy kind's.
pub fn data_for<'a>(combat: &'a Combat, a: &Actor) -> &'a CharData {
    combat.data_of(a)
}

/// TAE 760 BoostRootMotionToReachTarget (exe: the TAE handler FUN_140b293a0 stores its args,
/// FUN_1407f0a70 sets the root-motion scale chr+0x2fc each frame from FUN_140842980): the
/// arrival point lies ArriveDist short of the target, turned by ArriveAngle; the scale is
/// clamp(distance to it, EnableRangeMin, EnableRangeMax) / ReferenceDist (RangeMin when
/// already inside ArriveDist, 1.0 without the event). So a slash lengthens its lunge toward
/// a far target and shortens it on a close one. Target: the lock-on target for Wolf, the
/// player (or the rival it fights) for the enemy; unlocked, Wolf's attack auto-homing target (player.rs).
fn boost_root_motion(
    combat: Res<Combat>,
    lock: Option<Res<crate::camera::LockOn>>,
    mut q: Query<(Entity, &mut Actor, &Transform)>,
) {
    let positions: Vec<(Entity, Side, Vec3)> = q.iter().map(|(e, a, t)| (e, a.side, t.translation)).collect();
    for (_, mut a, tf) in &mut q {
        let d = data_for(&combat, &a);
        let ev = if a.anim.is_empty() { None } else { d.events_at(&a.anim, a.t).find(|e| e.kind == 760) };
        let Some(ev) = ev.filter(|e| e.args.get("IsEnable").and_then(|v| v.as_bool()).unwrap_or(false)) else {
            a.root_scale = 1.0;
            continue;
        };
        let target = match a.side {
            // FUN_1407f0a70: the lock target, else the attack's auto-homing target.
            Side::Player => lock.as_ref().and_then(|l| l.target).or(a.homing).and_then(|t| positions.iter().find(|p| p.0 == t)).map(|p| p.2),
            // The rival NPC it fights (enemy.rs think sets homing), else Wolf.
            Side::Enemy => a.homing.and_then(|t| positions.iter().find(|p| p.0 == t)).or_else(|| positions.iter().find(|p| p.1 == Side::Player)).map(|p| p.2),
        };
        // No target (the lock target gone mid-lunge, unlocked with nothing to home on): unscaled,
        // as without the event. Before, the last frame's scale (up to Max / Reference) stayed and
        // carried into the next 760 clip (round 51). gap: the exe's no-target branch of
        // FUN_140842980 is not traced.
        let Some(target) = target else {
            a.root_scale = 1.0;
            continue;
        };
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

/// Wolf's drawn facing: PlayerCtrl's GetRotation (vtable 0x1429d5508 slot 0x40, FUN_1407f2e90)
/// returns the yaw of the turn delay controller (BehaviorTurn +0x50, FUN_1407da4a0 -> +0x20),
/// not the physics yaw (+0x74) the generic ChrCtrl's (FUN_1407dcd70) returns. The controller
/// (FUN_1407e7970 -> FUN_1407e7da0) chases the physics yaw at 1.1x the turn rate, turning the
/// short way at full speed at once (the angular speed min(speed + rate * p0, rate * p1), action
/// +0x320 / +0x324 = 1, 1 unless TAE 704 sets them - c0000 has none), and lands on it when the
/// rest is within one step (speed 0). Emulated on the snapshot (tools/emu-cases `turn`): a 90 deg
/// snap is drawn over 7 frames at 0.2304 rad (792 deg/s). Here it is engaged by the turns the
/// player's turn controller makes (`player::Turn`) and dropped once it lands; a yaw other code
/// sets is drawn at once. gap: that the model matrix takes GetRotation is inferred from the
/// override, not traced to the render; whether the exe's warps and SetRotation reset the delay
/// controller is not traced.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ShownYaw {
    /// Delay controller +0x24.
    pub yaw: f32,
    /// Its angular speed [rad/s] (+0x30).
    pub speed: f32,
    /// The turn rate in force [deg/s] (FUN_1407daa20's pick), set by the player each tick.
    pub rate: f32,
}

impl ShownYaw {
    /// One frame of the drawn facing toward the physics yaw: the yaw to draw; dropped (None) once
    /// it lands.
    pub fn draw(shown: &mut Option<ShownYaw>, yaw: f32, dt: f32) -> f32 {
        let Some(s) = shown.as_mut() else { return yaw };
        s.chase(yaw, dt);
        let drawn = s.yaw;
        if s.speed == 0.0 {
            *shown = None;
        }
        drawn
    }

    /// One step toward the physics yaw `target`.
    pub fn chase(&mut self, target: f32, dt: f32) {
        let diff = wrap_angle(target - self.yaw);
        let speed = self.rate.to_radians() * 1.1;
        if diff.abs() <= speed * dt {
            self.yaw = target;
            self.speed = 0.0;
        } else {
            self.speed = speed;
            self.yaw += diff.signum() * speed * dt;
        }
    }
}

/// An angle wrapped into [-pi, pi) (a half turn reads as -pi, as the exe's turn request does).
pub fn wrap_angle(a: f32) -> f32 {
    (a + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

/// The ease-in before the stop (WorldChrManDbg +0x12c; FUN_140a51800 sets 0, the 2026-11-03
/// snapshot holds 0).
pub const HIT_STOP_EASE_IN: f32 = 0.0;
/// The full stop of every hit stop (WorldChrManDbg +0x130; FUN_140a51800 sets 0.06, snapshot 0.06).
pub const HIT_STOP_HOLD: f32 = 0.06;

/// A character's hit stop (exe SprjChrHitStopModule, ChrIns module bag +0x90): the start
/// FUN_140b9dad0 and the tick FUN_140b9d1b0 that FUN_1409f78a0 runs first each frame and whose
/// result +0x18 becomes the character's clock chr+0x108. State +0x20: 1 eases in over
/// HIT_STOP_EASE_IN (dt * (left / ease_in)^2), 2 holds the clock at 0 for HIT_STOP_HOLD, 3 eases
/// back over the hit stop's length (dt * (1 - left / length)^2); timer +0x10, length +0x14. A
/// state that runs out hands its frame to the next state's rule only from 1 to 2 (the frame that
/// ends the hold still advances nothing, the frame whose ease timer reaches 0 returns the eased
/// dt, not the whole). A new start replaces a running one; 0 starts nothing (its callers test
/// > 0). Checked frame by frame against the exe (tools/emu-cases modticks, test
/// hit_stop_module_matches_the_exe). gap: the debug switch +0x11d (ease-out 0.04 from +0x134
/// instead of the length) is off in the game. Rules first from AKJama/sekiro-rs
/// (crates/sim/src/combat/knockback.rs `HitStop`; MIT / Apache).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct HitStop {
    /// +0x20: 0 idle, 1 ease in, 2 hold, 3 ease out.
    state: u8,
    /// +0x10
    timer: f32,
    /// +0x14
    length: f32,
}

impl HitStop {
    /// FUN_140b9dad0.
    pub fn start(&mut self, seconds: f32) {
        if seconds > 0.0 {
            *self = Self { state: 1, timer: HIT_STOP_EASE_IN, length: seconds };
        }
    }

    pub fn active(&self) -> bool {
        self.state != 0
    }

    /// (state +0x20, timer +0x10, length +0x14), as the exe's module holds them.
    #[cfg(test)]
    pub(crate) fn module_fields(&self) -> (u8, f32, f32) {
        (self.state, self.timer, self.length)
    }

    /// FUN_140b9d1b0: runs `dt` of real time; returns how far the character's clock moves.
    pub fn split(&mut self, dt: f32) -> f32 {
        let (ease_in, hold, ease_out) = (HIT_STOP_EASE_IN, HIT_STOP_HOLD, self.length);
        if self.timer > 0.0 {
            self.timer -= dt;
        }
        if self.state == 0 {
            return dt;
        }
        if self.state == 1 && ease_in <= 0.0 {
            (self.state, self.timer) = (2, hold - dt);
        }
        if self.state == 2 && hold <= 0.0 {
            (self.state, self.timer) = (3, ease_out - dt);
        }
        if self.state == 3 && ease_out <= 0.0 {
            *self = Self::default();
        }
        let scaled;
        match self.state {
            1 => {
                let k = (self.timer / ease_in).clamp(0.0, 1.0);
                scaled = k * k * dt;
                if self.timer > 0.0 {
                    return scaled;
                }
                if hold > 0.0 {
                    (self.state, self.timer) = (2, hold);
                    return scaled;
                }
                if ease_out > 0.0 {
                    (self.state, self.timer) = (3, ease_out);
                    return scaled;
                }
            }
            2 => {
                scaled = 0.0;
                if self.timer > 0.0 {
                    return scaled;
                }
                if ease_out > 0.0 {
                    (self.state, self.timer) = (3, ease_out);
                    return scaled;
                }
            }
            3 => {
                let k = (1.0 - self.timer / ease_out).clamp(0.0, 1.0);
                scaled = k * k * dt;
                if self.timer > 0.0 {
                    return scaled;
                }
            }
            _ => return dt,
        }
        *self = Self::default();
        scaled
    }
}

/// Advances clocks, applies root motion and procedural motion.
pub fn advance(time: Res<Time>, combat: Res<Combat>, mut q: Query<(&mut Actor, &mut Transform)>) {
    let real_dt = time.delta_secs();
    for (mut a, mut tf) in &mut q {
        a.prev_t = a.t;
        // The hit stop scales the whole step (clip, TAE, root motion, pushes), as the exe runs
        // every module on the character's own time (FUN_140b9d1b0 -> FUN_1409f78a0).
        let dt = a.hit_stop.split(real_dt);
        a.clock_dt = Some(dt.max(0.0));
        if dt <= 0.0 {
            // A turn decided during the hold is drawn (and hit-tested) at once (round 58: the
            // rotation waited for the hold's end).
            a.shown = None;
            tf.rotation = Quat::from_rotation_y(a.yaw);
            fall(&mut a, &mut tf, real_dt);
            continue;
        }
        a.t += dt;
        a.since_posture_damage += dt;
        let data = data_for(&combat, &a);
        // A clip without a keyframe yet takes its group idle's opacity (1 if none; the Monk's
        // phantoms' a200_000000 holds 0), so a vanish cut short (Owl a000_003037: 0 from 0.27 to
        // 0.7 s, back to 1 at 0.8 s; hit or deflected in between) does not leave him invisible.
        a.opacity = match data.opacity_key(&a.anim, a.t) {
            Some(o) => o,
            None => data.opacity_key(&format!("a{:03}_000000", a.anim_group * 100), 0.0).unwrap_or(1.0),
        };
        // Additive layer clock: it ends after max(clip length, last TAE event).
        if !a.add_anim.is_empty() {
            a.add_t += dt;
            let end = data.anim(&a.add_anim).map_or(0.0, |an| an.events.iter().map(|e| e.end).fold(an.duration.unwrap_or(0.0), f32::max));
            if a.add_t > end {
                a.add_anim.clear();
            }
        }
        // The blend-in weight. Havok's blending transition (hkbBlendingTransitionEffectUtils::
        // generateInternal FUN_14140cfc0 -> FUN_1413bf6c0, <decomp ref>) mixes the world-from-model
        // too: from x (1 - w) + to x w over the clip's TAE Blend, w the smoothstep 3t^2 - 2t^3
        // (FUN_141408650, curve 0 on every c0000.hkx effect), the from generator still running.
        // TaeBlend_NoSrcMotion effects (Havok flag 1) take the to generator's at once (play()).
        // Live (rec_* recordings, 2727 clip switches): an attack out of a run keeps most of the
        // run's speed through its first 0.1 s; the head-kick relaunch a000_213111 moves 0.040 m in
        // frames 0-6 (the clip's 0.27 m x the weight). gap: a looping from clip is not wrapped.
        if a.blend_len < 0.0 {
            a.blend_len = if a.anim.is_empty() { 0.0 } else { data.blend_in(&a.anim).unwrap_or(0.0) };
        }
        // No source motion (TaeBlend_NoSrcMotion, or nothing to blend from): the to clip moves
        // at full weight from its first frame. Weighting it by the blend alone ramped its root
        // motion up from zero - SprintStartFromStep_F -> SprintLoop dropped 8.6 -> 0 m/s and took
        // 0.3 s to get back up (user: "slows down then speeds up").
        let w = if a.blend_from.is_some() && a.blend_len > 0.0 && a.blend_t < a.blend_len {
            let b = (a.blend_t / a.blend_len).clamp(0.0, 1.0);
            b * b * (3.0 - 2.0 * b)
        } else {
            1.0
        };
        a.blend_t += dt;
        let (mut from_delta, mut from_yaw) = (Vec3::ZERO, 0.0);
        if w >= 1.0 {
            a.blend_from = None;
        } else if let Some((k, t0)) = a.blend_from.clone() {
            let mut next_t0 = t0 + dt;
            if k.is_empty() {
                from_delta = a.blend_from_vel * dt;
            } else {
                // A looping from clip (c0000.hkx MODE_LOOPING) wraps: the step is its tail plus
                // its head when the clock passes the end.
                let len = data.length(&k);
                let wrap = a.blend_from_loops && len > 0.0 && t0 + dt > len;
                let step = if wrap {
                    match (data.root_at(&k, t0), data.root_at(&k, len), data.root_at(&k, 0.0), data.root_at(&k, t0 + dt - len)) {
                        (Some((p0, y0)), Some((pe, ye)), Some((ps, ys)), Some((p1, y1))) => Some((p0, y0, pe - p0 + (p1 - ps), (ye - y0) + (y1 - ys))),
                        _ => None,
                    }
                } else {
                    match (data.root_at(&k, t0), data.root_at(&k, t0 + dt)) {
                        (Some((p0, y0)), Some((p1, y1))) => Some((p0, y0, p1 - p0, y1 - y0)),
                        _ => None,
                    }
                };
                if let Some((_, y0, d, dy)) = step {
                    from_delta = Quat::from_rotation_y(a.yaw - y0 * a.yaw_scale + a.root_yaw) * Vec3::new(d.x, 0.0, d.y) * a.root_scale;
                    from_yaw = dy * a.yaw_scale;
                }
                if wrap {
                    next_t0 = t0 + dt - len;
                }
            }
            a.blend_from = Some((k, next_t0));
        }
        if a.anim.is_empty() && from_delta != Vec3::ZERO && !a.airborne {
            tf.translation += from_delta * (1.0 - w);
            a.yaw += from_yaw * (1.0 - w);
        }
        if !a.anim.is_empty() {
            if let (Some((p0, y0)), Some((p1, y1))) = (data.root_at(&a.anim, a.prev_t), data.root_at(&a.anim, a.t)) {
                // The root track is in the clip's start frame: the step turns with the facing the
                // clip started from (current yaw minus the clip's own turn so far), not the current
                // facing. Live (rec_c1010_20261009, behind deathblow a200_511200 / ThrowDef13200, both
                // spin 180 deg): the enemy ends 0.6 m in front of Wolf; turning the steps with the
                // current yaw slid him 1 m sideways.
                let delta = Quat::from_rotation_y(a.yaw - y0 * a.yaw_scale + a.root_yaw) * Vec3::new(p1.x - p0.x, 0.0, p1.y - p0.y) * a.root_scale * w + from_delta * (1.0 - w);
                // In the air (falling, not held by ChrActionFlag 27 SetNoGravity) the clip's
                // horizontal root motion is not applied; the air velocity moves the body. Live
                // (30 head kicks a000_213115 / 213114 / 213110 in the rec_* recordings): Wolf moves
                // only while the kick holds him (frames 0-6), then goes straight up; the clip's
                // 0.27 m step back at frames 17-20 never shows. gap: the exe's branch is not traced.
                let held = data.flag(&a.anim, a.t, crate::player::FLAG_NO_GRAVITY);
                if !a.airborne || held {
                    tf.translation += delta;
                }
                a.yaw += (y1 - y0) * a.yaw_scale * w + from_yaw * (1.0 - w);
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
            // A lifted clip that follows another lifted one goes on from the height it is at (the
            // recordings: Lady Butterfly's 3200 / 3240 start in the air after 3010 / 3024, then
            // 3202 / 3242 come down 4.5 / 2.8 m, rec_20261015_225710); restarting from the ground
            // snapped her down and back up. Out of the lift it falls back to the ground at
            // WORLD_GRAVITY. gap: the enemies' fall acceleration (their fall type) is not traced.
            match (lifted, a.lift_ground) {
                (true, None) => {
                    a.lift_ground = Some(tf.translation.y);
                    a.lift_off = 0.0;
                    a.lift_anim = a.anim.clone();
                    a.lift_vy = 0.0;
                    tf.translation.y += rise;
                }
                (true, Some(g)) => {
                    if a.lift_anim != a.anim {
                        a.lift_off = (tf.translation.y - g - rise).max(0.0);
                        a.lift_anim = a.anim.clone();
                        a.lift_vy = 0.0;
                    }
                    tf.translation.y = g + a.lift_off + rise;
                }
                (false, Some(g)) => {
                    a.lift_vy -= crate::data::WORLD_GRAVITY * dt;
                    tf.translation.y += a.lift_vy * dt;
                    if tf.translation.y <= g {
                        tf.translation.y = g;
                        a.lift_ground = None;
                        a.lift_off = 0.0;
                        a.lift_anim.clear();
                        a.lift_vy = 0.0;
                    }
                }
                (false, None) => {}
            }
        }
        let v = a.move_vel;
        tf.translation += v * dt;
        a.last_move_vel = v;
        // The knockback's distance over this step, integrated exactly (as sekiro-rs knockback.rs
        // `covered`): the rest of the full-speed hold, then the linear fall over the decay, whose
        // area from r0 to r1 seconds left is v / (2 dec) * (r0^2 - r1^2). Whole steps at the
        // start speed overshot the asked distance by 3-76 % (2026-10-12 bug hunt).
        if a.kb_cont > 0.0 || a.kb_dec > 0.0 {
            let hold = dt.min(a.kb_cont.max(0.0));
            a.kb_cont -= hold;
            let r0 = a.kb_dec.max(0.0);
            let r1 = if a.kb_cont > 0.0 { r0 } else { (r0 - (dt - hold)).max(0.0) };
            a.kb_dec = r1;
            let fall = (r0 * r0 - r1 * r1) / (2.0 * a.kb_dec_total.max(1e-4));
            let step = a.kb_vel * (hold + fall);
            let len = step.length();
            if len > a.kb_left {
                tf.translation += step * (a.kb_left / len);
                (a.kb_cont, a.kb_dec, a.kb_left) = (0.0, 0.0, 0.0);
            } else {
                tf.translation += step;
                a.kb_left -= len;
            }
        }
        fall(&mut a, &mut tf, real_dt);
        // The drawn facing chases a turn the turn controller snapped (ShownYaw); landed, it is
        // `yaw` again.
        let yaw = a.yaw;
        let drawn = ShownYaw::draw(&mut a.shown, yaw, dt);
        tf.rotation = Quat::from_rotation_y(drawn);
    }
}

/// The vertical fall step. Gravity and landing are handled by the owner (player.rs,
/// data::FALL_TYPES). It runs on the RAW dt, hit stop or not, as the owner's gravity does: the exe
/// gives the fall module FUN_140bac120 and the Havok step the unscaled time (FUN_1407e1e20,
/// <decomp ref>). Round 58: before, the position took the hit-stop clock while vel_y kept
/// building on real time, so an air hit's arc came out lower and shorter.
fn fall(a: &mut Actor, tf: &mut Transform, dt: f32) {
    if a.airborne {
        tf.translation.y += (a.vel_y - 0.5 * a.grav_y * dt) * dt;
    }
}

/// The clip's TAE SpEffects past their TAE window, as status.rs reads SpEffectParam: one with
/// effectEndurance > 0 lasts that long from when the event put it on; one with endurance 0 lasts
/// the window and then becomes its replaceSpEffectId, as does a timed one when it runs out.
/// Wolf's reaction / guard / throw-defence clips put 109450 on (staminaRecoverSpeedRate 0, the
/// window), which becomes 109451 "Stopping core recovery after being hit" (rate 0, 1 s): posture
/// regen stays off a second after the window (round 30; it came back at the window's end).
/// The exe (FUN_140b56500 for TAE 66 and 67 alike) never removes a TAE effect at the window's
/// end: endurance -1 stays (enemies: enemy.rs wear_for_good), 0 is gone a tick after the window.
/// Wolf's TAE -1 rows all have a category and are kept in `Player.held` (player.rs): the Todome
/// "recovery prohibited" rows and 110091, the resurrection heartbeat parent (category 155, cut
/// by 110093). `held` is not in worn_sp; 110091's heartbeat cycle 110092 (vfx 5010) is played
/// from `held` by status.rs `heartbeat`.
pub(crate) fn run_sp_effect_chains(data: &crate::data::CharData, a: &mut Actor, dt: f32) {
    let row = |id: i64| data.sp_effects.get(&id.to_string());
    let now: Vec<i64> = if a.anim.is_empty() { Vec::new() } else { data.sp_effects_at(&a.anim, a.t).iter().map(|(id, _)| *id).collect() };
    let mut add: Vec<(i64, f32)> = Vec::new();
    for &id in &now {
        if !a.tae_prev.contains(&id) {
            if let Some(s) = row(id).filter(|s| s.effect_endurance > 0.0) {
                add.push((id, s.effect_endurance));
            }
        }
    }
    for &id in &a.tae_prev {
        if !now.contains(&id) {
            if let Some(s) = row(id).filter(|s| s.effect_endurance == 0.0) {
                if let Some(r) = row(s.replace_sp_effect_id).filter(|r| r.effect_endurance > 0.0) {
                    add.push((s.replace_sp_effect_id, r.effect_endurance));
                }
            }
        }
    }
    let mut kept = Vec::new();
    for (id, left) in a.sp_chained.drain(..) {
        let left = left - dt;
        if left > 0.0 {
            kept.push((id, left));
        } else if let Some(r) = row(id).and_then(|s| row(s.replace_sp_effect_id).map(|r| (s.replace_sp_effect_id, r))).filter(|(_, r)| r.effect_endurance > 0.0) {
            add.push((r.0, r.1.effect_endurance));
        }
    }
    for (id, secs) in add {
        kept.retain(|c| c.0 != id);
        kept.push((id, secs));
    }
    // A lasting row whose invocationConditionsStateChange gate fails waits (stays on, inactive)
    // or, with isWaitModeDelete, is deleted then (FUN_140c04090, <decomp ref>). The
    // whistle's madness 230515 / 230516 (gate 959) is one: it lasts while the beast wears its
    // NpcParam resident 230500 (959), i.e. until it dies, and goes if that is cleared.
    let states: Vec<i64> = a.resident.iter().chain(kept.iter().map(|c| &c.0)).chain(now.iter()).filter_map(|id| crate::data::sp_cycle(*id)).map(|c| c.state_info).filter(|s| *s > 0).collect();
    kept.retain(|(id, _)| crate::data::sp_cycle(*id).is_none_or(|c| !c.wait_delete || c.gate.iter().filter(|g| **g > 0).all(|g| states.contains(g))));
    a.sp_chained = kept;
    a.tae_prev = now;
}

/// Posture recovery, as in the exe's ChrIns update (FUN_140a04850, <decomp ref>; checked
/// frame by frame in tools/emu-cases `damage_regen`, test posture_regen_matches_the_exe):
///   points/s = (int)(speed x (base x TAE 225 % / 100 x level multiplier x ratio(type) / 100))
///   (`regen_frame`), then this frame's whole points of carry + points/s x dt go in and the
///   fraction is carried (ChrDataModule +0x15c), and the posture setter bounds the result
///   (`Actor::bound_posture`; the TAE 960 type's StaminaControlParam limits, `stamina_limits`).
/// base: vfunc 0x218 = NpcParam staminaRecoverBaseVel (FUN_140a13a70) or CalcCorrectGraph 504
/// (player, FUN_140a2a5d0), plus the worn SpEffects' staminaRecoverChangeSpeed (FUN_140bfbe40);
/// speed: their staminaRecoverSpeedRate product (FUN_140bfe360).
/// The TAE event 960 type is -1 ("none", 100%) when no event is active.
/// The level multiplier chr.f[0x10d0] is Dark Souls' equip-load class (FUN_14084d0c0: weight /
/// (40 + stat) > 0.7 -> 0.8, > 1.0 or SpEffect stateInfo 102 -> 0.7, else 1.0). Every Sekiro
/// weapon/protector weighs 0, so it is 1.0 here (1.0 on all 90 characters of the snapshot).
/// The exe skips the whole step while action flag (chr+0x50)+0x50 +0x88 bit 21 is set (gap: its
/// setter is not identified); it runs at full posture too (the carry keeps moving) and while
/// broken.
pub fn regen_posture(time: Res<Time>, combat: Res<Combat>, config: Res<GameConfig>, mut q: Query<&mut Actor>) {
    let real_dt = time.delta_secs();
    let graph = combat.param("CalcCorrectGraph", 51).clone();
    for mut a in &mut q {
        // The character's own clock: a hit stop holds regen and the SpEffect timers (round 64).
        let dt = a.clock(real_dt);
        run_sp_effect_chains(data_for(&combat, &a), &mut a, dt);
        let mut worn = crate::combat::worn_sp(&a, &[]);
        worn.sort();
        worn.dedup();
        let bonus: f32 = worn.iter().filter_map(|id| crate::data::sp_cycle(*id)).filter(|c| c.state_info == 155).map(|c| c.sa_point).sum();
        if a.super_armor.max > 0.0 || bonus != 0.0 || a.super_armor.bonus != 0.0 {
            let reacting = !a.anim.is_empty() && data_for(&combat, &a).flag(&a.anim, a.t, 71);
            let instant = worn.iter().any(|id| crate::data::sp_cycle(*id).is_some_and(|c| c.state_info == 331));
            if a.posture_broken() {
                a.super_armor.bonus = bonus;
                a.super_armor.refill();
            } else {
                a.super_armor.tick(dt, reacting, instant, bonus);
            }
        }
        // regen_delay > 0 is an experiment (the exe has no delay); it holds the carry at 0.
        if a.since_posture_damage < config.posture.regen_delay {
            a.regen_carry = 0.0;
            continue;
        }
        let data = data_for(&combat, &a);
        let ty = if a.anim.is_empty() { None } else { data.stamina_ratio_type(&a.anim, a.t) };
        let ty = ty.unwrap_or(config.posture.default_ratio_type);
        let row = combat.param("StaminaControlParam", a.stamina_ctrl_row);
        // (A row the export missed read as 100 % silently: c1180's 1070000 gives 0 % in a swing.)
        if row.is_null() && a.stamina_ctrl_row > 0 {
            static MISSING: std::sync::Mutex<Vec<i64>> = std::sync::Mutex::new(Vec::new());
            let mut m = MISSING.lock().unwrap_or_else(|e| e.into_inner());
            if !m.contains(&a.stamina_ctrl_row) {
                m.push(a.stamina_ctrl_row);
                warn!("StaminaControlParam {} not exported: posture regen at 100 %", a.stamina_ctrl_row);
            }
        }
        // FUN_1410cf5c0: the type's staminaRecoverRatio, 100 for a missing row.
        let ratio = (ty >= 0).then(|| row[format!("staminaRecoverRatio_forType{ty:03}")].as_f64().unwrap_or(100.0) as f32);
        let hp_factor = if config.posture.hp_scales_regen { calc_correct(&graph, a.hp / a.hp_max) } else { 1.0 };
        // Resident HP-conditional SpEffects (NpcParam spEffectID slots) and the clip's TAE ones
        // multiply regen through staminaRecoverSpeedRate (the exe multiplies all active ones,
        // FUN_140bfe360) and add staminaRecoverChangeSpeed to the base (kb/enemy.md: rate * (base
        // + change); the TAE one was never read - c1010's battle idle 300240 +40/s - round 28).
        let hp_pct = a.hp / a.hp_max * 100.0;
        // Each row once: a TAE row with effectEndurance > 0 is also in sp_chained from its first
        // frame, and counted twice its rate was squared and its change doubled (c1010's 300240:
        // +80/s) while the window lasted (round 51).
        let mut ids: Vec<i64> = a
            .resident
            .iter()
            .copied()
            .filter(|id| data.sp_effects.get(&id.to_string()).is_some_and(|s| s.condition_hp <= 0.0 || hp_pct <= s.condition_hp))
            .collect();
        if !a.anim.is_empty() {
            ids.extend(data.sp_effects_at(&a.anim, a.t).iter().map(|(id, _)| *id));
        }
        ids.extend(a.sp_chained.iter().map(|(id, _)| *id));
        let mut seen = Vec::new();
        ids.retain(|id| if seen.contains(id) { false } else { seen.push(*id); true });
        let worn: Vec<&crate::data::SpEffect> = ids.iter().filter_map(|id| data.sp_effects.get(&id.to_string())).collect();
        let resident: f32 = worn.iter().map(|s| s.stamina_recover_speed_rate).product();
        let change: f32 = worn.iter().map(|s| s.stamina_recover_change_speed).sum();
        // TAE 225 SetSPRegenRatePercent (c1020 guard / deflect 3100-3102: 33 %, fire reactions 0 %),
        // the exe's byte (chr+0x50)+0x50 +0x14.
        let tae_pct = if a.anim.is_empty() {
            100
        } else {
            data.events_at(&a.anim, a.t).find(|e| e.kind == 225).and_then(|e| e.arg_i64("RegenRatePercent")).unwrap_or(100)
        };
        a.posture_limits = stamina_limits(row, ty, a.posture_max as i32).map(|(lo, hi)| (a.posture_max - hi as f32, a.posture_max - lo as f32));
        let (whole, carry) = regen_frame(a.posture_regen + change, resident * hp_factor, tae_pct as f32, ratio, dt, a.regen_carry);
        a.regen_carry = carry;
        a.posture = a.bound_posture(a.posture - whole as f32);
    }
}

/// One frame of FUN_140a04850's recovery: (whole points, new carry). `base` (vfunc 0x218) and
/// `speed` (FUN_140bfe360) as in `regen_posture`; `tae_pct` the TAE 225 percent (100 without
/// one); `ratio` the StaminaControlParam type's staminaRecoverRatio (None without a type).
/// The points per second are cut to a whole number first (cvttss2si, ChrDataModule +0x230:
/// 247 for 20 x 12.3975), then carry + points/s x dt is cut again (+0x22c), the rest carried.
pub fn regen_frame(base: f32, speed: f32, tae_pct: f32, ratio: Option<f32>, dt: f32, carry: f32) -> (i32, f32) {
    let mut pct = tae_pct * 0.01 * EQUIP_LOAD_RATE;
    if let Some(r) = ratio {
        pct *= r * 0.01;
    }
    let per_sec = (speed * (base * pct)) as i32;
    let acc = carry + per_sec as f32 * dt;
    let whole = acc as i32;
    (whole, acc - whole as f32)
}

/// chr.f[0x10d0], the equip-load class (see `regen_posture`): 1.0 for every character.
const EQUIP_LOAD_RATE: f32 = 1.0;

/// The posture (stamina) limits of a StaminaControlParam type (exe FUN_140bd4e20, <decomp ref>):
/// staminaMaxRatio / staminaMinRatio (FUN_1410cf390 row +4 / FUN_1410cf4b0 +6) percent of `max`,
/// cut to whole points; the setter keeps the remaining posture inside them while TAE 960 sets the
/// type. Row 1000000-1000200: type 5 (the break anims, a000_008300 0-2.5 s) holds it at 0, type 6
/// (their end) at 30 %, type 7 (a000_012000's end) full; row 0 (Wolf) type 5 holds him full.
/// Returns (lowest, highest) remaining posture, None for no type.
pub fn stamina_limits(row: &serde_json::Value, ty: i64, max: i32) -> Option<(i32, i32)> {
    if !(0..16).contains(&ty) || row.is_null() {
        return None;
    }
    let pct = |k: &str| row[format!("{k}_forType{ty:03}")].as_i64().unwrap_or(0);
    let (a, b) = ((pct("staminaMaxRatio") * max as i64 / 100) as i32, (pct("staminaMinRatio") * max as i64 / 100) as i32);
    Some((a.min(b), a.max(b)))
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
/// start anim stops against the enemy - the c1010 vault start moved 0.18 m with him 0.95 m away),
/// and an enemy grab's pair (Wolf "Grabbed", the grabber ThrowAtk<id>: its ThrowParam absorb puts
/// the c1360 Rappa on Wolf's dummy 173, 0.52 m in, inside the 0.8 m radii).
pub(crate) fn in_throw(a: &Actor) -> bool {
    a.state.starts_with("ThrowDef")
        || a.state.starts_with("ThrowAtk")
        || matches!(a.state.as_str(), "Deathblow" | "ThrowBreak" | "BreakKickJump" | "Mikiri" | "PlungeDeathblow" | "WireThrowLand" | "WireThrowTodome" | "Grabbed")
}

/// ChrActionFlag 39 (TAE name "Set_0x78_7"): on Wolf's throw and plunge anims (a2xx_510300,
/// 511400, 511500, ...) where the pair passes through each other, and on the hidden ground
/// zombie's crawl (c1500 a100_020010-12) that has to get under Wolf (150000_battle.lua Act16:
/// GetDist < 0.2). Read as "character proxy off". gap: the exe's use of chr+0x78 bit 7 is not
/// traced; this is the data pattern.
const FLAG_PROXY_OFF: i64 = 39;

fn proxy_off(combat: &Combat, a: &Actor) -> bool {
    !a.anim.is_empty() && combat.data_of(a).flag(&a.anim, a.t, FLAG_PROXY_OFF)
}

/// Characters don't overlap: pairs closer than the sum of their radii are pushed
/// apart horizontally, half each (both weigh in like the game's char proxies). The proxies are
/// upright capsules, so a pair whose heights don't meet (Wolf over an enemy's head after a
/// head kick) is left alone: an enemy spans its feet to NpcParam hitHeight, Wolf his capsule
/// (2 x CAPSULE_HALF_HEIGHT; gap: his proxy height is not traced).
fn separate(combat: Res<Combat>, mut last: Local<std::collections::HashMap<Entity, Vec3>>, mut q: Query<(Entity, &mut Actor, &mut Transform)>) {
    let radius = |a: &Actor| {
        if a.side == Side::Player {
            PLAYER_BODY_RADIUS
        } else {
            combat.param("NpcParam", combat.foe_of(a).npc_row)["hitRadius"].as_f64().unwrap_or(0.5) as f32
        }
    };
    let half = crate::player::CAPSULE_HALF_HEIGHT;
    // Feet and top heights (enemies stand 0.05 m higher, enemy.rs spawn_lineup).
    let span = |a: &Actor, t: &Transform| {
        if a.side == Side::Player {
            (t.translation.y - half, t.translation.y + half)
        } else {
            let feet = t.translation.y - half - 0.05;
            (feet, feet + combat.param("NpcParam", combat.foe_of(a).npc_row)["hitHeight"].as_f64().unwrap_or(2.0 * half as f64) as f32)
        }
    };
    let mut items: Vec<_> = q.iter_mut().collect();
    for i in 0..items.len() {
        for j in i + 1..items.len() {
            let (l, r) = items.split_at_mut(j);
            let (ea, a, ta) = &mut l[i];
            let (eb, b, tb) = &mut r[0];
            // Throw pairs pass through each other (the game's throws drop the character proxies).
            if a.hp <= 0.0 || b.hp <= 0.0 || in_throw(a) || in_throw(b) || proxy_off(&combat, a) || proxy_off(&combat, b) {
                continue;
            }
            let ((a0, a1), (b0, b1)) = (span(a, ta), span(b, tb));
            if a0 >= b1 || b0 >= a1 {
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
                    // The one that walked in is stopped; one standing still is not shoved. Live
                    // (rec_2026101x_*): an idle Wolf (a000_000000) with an enemy within 1.3 m moved
                    // in 32 of 1267 ticks while the enemy closed in 148 times and stopped at 0.95 m;
                    // Wolf running into an enemy (a000_0004xx / 5xx, 9306 ticks within 1.2 m) and the
                    // enemy both median 0.0 m/s along the line between them. Before, the pair split
                    // the overlap half each, so an enemy walking up slid Wolf in his idle pose.
                    // Each side's share = its own approach this tick (both, or neither: half each).
                    // gap: Havok's char proxy response is not traced; this is the recorded result.
                    _ => {
                        let came = |e: Entity, now: Vec3, toward: Vec3| last.get(&e).map_or(0.0, |p| (now - *p).with_y(0.0).dot(toward).max(0.0));
                        let (ca, cb) = (came(*ea, ta.translation, n), came(*eb, tb.translation, -n));
                        if ca + cb > 1e-5 { (ca / (ca + cb), cb / (ca + cb)) } else { (0.5, 0.5) }
                    }
                };
                let push = n * (min - len);
                ta.translation -= push * wa;
                tb.translation += push * wb;
                // A body pushed in the air stops against the other (its launch velocity goes;
                // the stick's air control still adds its bit each tick). Live (rec_20261016_* boss
                // fights, 19 head kicks off a forward jump over a perilous sweep): Wolf holds at
                // the contact distance (1.14 m from the Monk, 0.95 m from the Owl) at 0.7 m/s
                // until the kick lands 19 ticks on. Before, the jump's full speed stayed, so he slid
                // round the body and shot past it at 6-7.5 m/s and the kick missed.
                // gap: Havok's char proxy response is not traced; this is the recorded result.
                if wa > 0.0 && a.airborne {
                    a.air_base = Vec3::ZERO;
                }
                if wb > 0.0 && b.airborne {
                    b.air_base = Vec3::ZERO;
                }
            }
        }
    }
    last.clear();
    for (e, _, t) in &items {
        last.insert(*e, t.translation);
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
