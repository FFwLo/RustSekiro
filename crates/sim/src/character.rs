//! A simulated character: script and behaviour graph, TAE events, the body, and its control
//! (the player's input or an NPC's brain).
//!
//! [`Character::tick`] runs one simulation step:
//!
//! 1. Control: the player's buffered presses, hold times and stick, or the NPC brain's
//!    requested action and movement.
//! 2. What the engine tells the script: movement variables, action requests gated by the
//!    previous step's TAE cancel windows, active behaviour reference ids, ground contact, lock-on
//!    and the hit being reacted to.
//! 3. The script and behaviour graph ([`PlayerBehavior::tick`]).
//! 4. TAE events of the clips now playing, which feed the next step's answers.
//! 5. The body: root motion of the full-body clip, steering at the TAE turn speed, jump
//!    velocity changes and gravity.

use crate::behavior::{AnimOffsets, ClipDurations};
use crate::body::{Body, BodyInput, FlatGround, Ground};
use crate::clips::ClipLibrary;
use crate::homing::{ACT_STOP_AUTO_AIM, ACT_UPDATE_AUTO_AIM, AutoAimLimits, reach_scale};
use crate::input::{InputFrame, InputState, angle_diff};
use crate::player::{IncomingDamage, LoadError, PlayerBehavior, TickReport};
use crate::tae::{PlayingClip, TaeDb, TaeFrame, TaeRuntime, cancel};
use glam::{Mat4, Vec3};
use sekiro_formats::anim::Skeleton;
use sekiro_formats::hkb::NodeId;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

/// Steering speed when no TAE `SetTurnSpeed` is active, degrees per second: the exe's turn
/// controller default (traced by FwLo's RustSekiro, `docs/kb/movement.md`).
pub const DEFAULT_TURN_SPEED: f32 = 720.0;

/// One step's outcome, for HUDs, logs and tests.
#[derive(Debug, Clone, Default)]
pub struct StepReport {
    pub behavior: TickReport,
    /// Full-body clip and its time after the step.
    pub animation: Option<String>,
    pub anim_time: f32,
    pub position: [f32; 3],
    pub yaw: f32,
    pub grounded: bool,
    pub sp_effects: Vec<i32>,
    pub behavior_refs: Vec<i32>,
    pub action_flags: Vec<i32>,
    pub requested: Vec<i32>,
    /// True when the full-body clip started (or restarted) this step.
    pub clip_started: bool,
    /// Authored crossfade into the full-body clip in seconds (TAE `Blend`).
    pub blend_in: f32,
    /// Target root-motion weight this step (1 when no outgoing motion is blended).
    pub root_blend_weight: f32,
    /// Seconds this character's own clock advanced in the step: the step's length, less while a
    /// hit stop holds or eases it (0 while held). The renderer draws between steps with it.
    pub advanced: f32,
    /// How far the model is faded out by the playing clips' TAE 193 events: 0 fully drawn (the
    /// default), 1 not drawn at all (the Owl's warps; 1 minus [`crate::tae::TaeFrame::opacity`]).
    pub fade: f32,
}

/// What an NPC's brain asks of its body this step. The NPC script reads these as the engine's
/// AI interface: `env(106)` for the action and the `MoveSpeedLevel` / `MoveAngle` variables.
#[derive(Debug, Clone, Default)]
pub struct NpcControl {
    /// The requested action (env 106): an animation-like code such as 3000 for attack
    /// `a000_003000`, 9910 for guard, or 0 for none.
    pub action: i32,
    /// Movement speed level: 0 stop, up to 0.75 walk, above that run.
    pub move_level: f32,
    /// World yaw to move along, if moving.
    pub move_yaw: Option<f32>,
    /// World yaw to face (usually toward the target).
    pub face_yaw: Option<f32>,
    /// Behaviour reference ids the AI state contributes (battle, caution, ...).
    pub refs: HashSet<i32>,
    /// The action comes from a combo goal (one the scripts registered with
    /// `ENABLE_COMBO_ATK_CANCEL`), so a TAE "AI combo attack queued" window (flag 23) accepts it.
    pub combo: bool,
}

/// TAE action flags that end an NPC animation for a queued AI action: 23 combo attack, 86
/// attack, 79 step ("End If AI ... Queued" in the TAE template).
pub mod ai_cancel {
    pub const COMBO: i32 = 23;
    pub const STEP: i32 = 79;
    pub const ATTACK: i32 = 86;
}

/// Step actions of the shared NPC script (`a000_005xxx`).
fn is_step_action(action: i32) -> bool {
    (5000..6000).contains(&action)
}

/// Which brain drives the character.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlKind {
    Player,
    Npc,
}

pub struct Character {
    pub docking: crate::docking::Docking,
    pub wire: crate::wire::Wire,
    pub chr: String,
    pub kind: ControlKind,
    pub behavior: crate::shared::BehaviorSlot,
    pub clips: ClipLibrary,
    pub tae: Arc<TaeDb>,
    pub tae_runtime: TaeRuntime,
    pub body: Body,
    /// Player input state (unused for NPCs).
    pub input: InputState,
    /// NPC control for the next step (unused for the player).
    pub npc: NpcControl,
    /// Lock-on target position, set by the world (player only).
    pub lock_target: Option<[f32; 3]>,
    /// Living enemies the attack auto-aim may pick, set by the world before each step
    /// (player only).
    pub aim_candidates: Vec<[f32; 3]>,
    /// The attack auto-aim target, an index into [`Self::aim_candidates`] (player only).
    pub homing: Option<usize>,
    /// LockCamParam limits of the auto-aim.
    pub auto_aim: AutoAimLimits,
    /// The character an NPC fights (Wolf), set by the world; its attacks aim at it.
    pub npc_target: Option<[f32; 3]>,
    /// A hit to react to on the next step; cleared after it.
    pub pending_damage: Option<IncomingDamage>,
    /// The Havok skeleton the clips animate (source space).
    pub skeleton: Arc<Skeleton>,
    pub ground: Box<dyn Ground>,
    /// The map's navigation mesh a map foe walks on ([`crate::navmesh::NavGround`]); `None`
    /// for Wolf, arena bosses and maps without one.
    pub navmesh: Option<Arc<crate::navmesh::NavMesh>>,
    /// The duel owns this flag while a paired throw holds both characters.
    pub throw_ignore_walls: bool,
    /// The lower-body twist in degrees (the engine's `TwistLowerRootAngle`, positive after a
    /// turn to the left): how far the visible body still lags behind [`Body::yaw`] after a
    /// free move start turned the character to the stick. Unwinds at the default turn speed.
    pub twist: f32,
    /// The locked-on walk twist in degrees, positive with the legs turned to the right (the
    /// exe's WalkTwist output, the graph's `TwistMasterAngle`): how far the legs and the
    /// travel turn off the directional walk or run clip playing so the move goes along the
    /// stick. Eases toward [`Self::walk_twist_target`] at [`WALK_TWIST_RATE`]. The renderer turns
    /// the Master bone by it and turns Spine..Spine2 back, so the chest keeps facing the target.
    pub walk_twist: f32,
    /// What the walk twist eases toward: the stick's angle from the clip's direction while a
    /// locked-on walk or run plays with the stick pushed, else 0. Root motion is turned by it.
    pub walk_twist_target: f32,
    /// World yaw of the last step's root-motion travel, when the clip carried the body faster
    /// than [`HEADING_MIN_SPEED`]; `TurnAngle` is measured from it (else from the facing).
    heading: Option<f32>,
    free_move_pending: bool,
    /// Clip times before the last step, by clip node.
    last_times: HashMap<NodeId, (String, f32)>,
    last_main: Option<PlayingClip>,
    last_motion_state: Option<crate::behavior::StateRef>,
    motion_blend: Option<MotionBlend>,
    last_anim_effects: Vec<i32>,
}

/// The outgoing generator keeps advancing through TAE Blend (FwLo's actor.rs).
struct MotionBlend {
    animation: String,
    start: f32,
    looping: bool,
    twist: f32,
    elapsed: f32,
    duration: f32,
}

/// The locked-on strafe class for a stick `angle` degrees from the facing (positive right),
/// given the `current` class: forward within 55 degrees (65 while already forward), back past
/// 125 (115 while already back), otherwise left (2) or right (3).
fn strafe_direction(current: i32, angle: f32) -> i32 {
    let (front, back) = match current {
        0 => (65.0, 125.0),
        1 => (55.0, 115.0),
        _ => (55.0, 125.0),
    };
    if angle.abs() < front {
        0
    } else if angle.abs() > back {
        1
    } else if angle < 0.0 {
        2
    } else {
        3
    }
}

/// The clip direction of each locked-on strafe class (`MoveDirection` 0 forward, 1 back, 2
/// left, 3 right), degrees from the facing, positive right.
const STRAFE_CLIP_ANGLE: [f32; 4] = [0.0, 180.0, -90.0, 90.0];

/// How fast the walk twist eases toward its target, degrees per second: 0.1 rad per 1/30 s,
/// the exe's SpinJointBehavior on the Master joint in constant-rate mode (traced by FwLo's
/// RustSekiro, `docs/kb/movement.md`).
pub const WALK_TWIST_RATE: f32 = 0.1 * 30.0 * (180.0 / std::f32::consts::PI);

/// SpEffect behaviour reference ids the TAE puts on walk, run and sprint animations
/// (`SP_EF_REF_TAE_MOVING_WALK/RUN/SPRINT`; the script's `GetLocomotionType`).
const MOVING_REFS: [i32; 3] = [90, 91, 92];

/// Root-motion speed in m/s above which the travel direction, not the facing, is what
/// `TurnAngle` is measured from (a walk start already moves faster).
const HEADING_MIN_SPEED: f32 = 1.0;

/// The player.
pub type PlayerCharacter = Character;

fn load_skeleton(cache: &Path, chr: &str) -> Arc<Skeleton> {
    let path = cache.join(format!("anim/{chr}/skeleton.bin"));
    crate::shared::load(path.clone(), || {
        std::fs::read(&path)
            .ok()
            .and_then(|b| Skeleton::from_bytes(&b).ok())
    })
    .unwrap_or_else(|| {
        Arc::new(Skeleton {
            name: chr.to_owned(),
            bones: Vec::new(),
        })
    })
}

impl Character {
    /// Move out the live actor, leaving only an inert shell without VM, graph or assets.
    pub(crate) fn take(&mut self) -> Option<Self> {
        self.is_present()
            .then(|| std::mem::replace(self, Self::vacant()))
    }

    pub fn is_present(&self) -> bool {
        self.behavior.is_present()
    }

    fn vacant() -> Self {
        Self {
            chr: String::new(),
            kind: ControlKind::Player,
            behavior: crate::shared::BehaviorSlot::empty(),
            clips: ClipLibrary::default(),
            tae: Arc::default(),
            tae_runtime: TaeRuntime::default(),
            body: Body::default(),
            docking: Default::default(),
            wire: Default::default(),
            input: Default::default(),
            npc: Default::default(),
            lock_target: None,
            aim_candidates: Vec::new(),
            homing: None,
            auto_aim: AutoAimLimits::default(),
            npc_target: None,
            pending_damage: None,
            skeleton: Arc::new(Skeleton {
                name: String::new(),
                bones: Vec::new(),
            }),
            ground: Box::new(FlatGround::default()),
            navmesh: None,
            throw_ignore_walls: false,
            twist: 0.0,
            walk_twist: 0.0,
            walk_twist_target: 0.0,
            heading: None,
            free_move_pending: false,
            last_times: HashMap::new(),
            last_main: None,
            last_motion_state: None,
            motion_blend: None,
            last_anim_effects: Vec::new(),
        }
    }
    fn assemble(
        cache: &Path,
        chr: &str,
        kind: ControlKind,
        scripts: &[&str],
        graph: &str,
        offsets: AnimOffsets,
    ) -> Result<Self, LoadError> {
        let tae = crate::shared::load(cache.join("anim").join(chr), || {
            Some(TaeDb::load_from_cache(cache, chr))
        })
        .unwrap();
        let clips = ClipLibrary::shared(cache.join("anim").join(chr));
        for (id, src) in tae.hkx_aliases() {
            clips.add_alias(id, src);
        }
        let script_paths: Vec<_> = scripts
            .iter()
            .map(|s| cache.join("raw/action/script").join(s))
            .collect();
        let mut behavior = PlayerBehavior::load_scripts(
            &script_paths,
            &cache.join("raw/chr").join(graph),
            Box::new(clips.clone()),
            offsets,
        )?;
        behavior.env.standby = Some(true);
        behavior.env.move_cancel = Some(true);
        behavior.env.existing_anims = std::rc::Rc::new(tae.anim_ids().collect());
        Ok(Self {
            chr: chr.to_owned(),
            kind,
            behavior: behavior.into(),
            clips,
            tae,
            tae_runtime: TaeRuntime::for_kind(kind == ControlKind::Npc && chr == crate::demon::CHR),
            body: Body::default(),
            docking: crate::docking::Docking::default(),
            wire: crate::wire::Wire::default(),
            input: InputState::default(),
            npc: NpcControl::default(),
            lock_target: None,
            aim_candidates: Vec::new(),
            homing: None,
            auto_aim: AutoAimLimits::load(cache),
            npc_target: None,
            pending_damage: None,
            skeleton: load_skeleton(cache, chr),
            ground: Box::new(FlatGround::default()),
            navmesh: None,
            throw_ignore_walls: false,
            twist: 0.0,
            walk_twist: 0.0,
            walk_twist_target: 0.0,
            heading: None,
            free_move_pending: false,
            last_times: HashMap::new(),
            last_main: None,
            last_motion_state: None,
            motion_blend: None,
            last_anim_effects: Vec::new(),
        })
    }

    /// Loads Wolf from the extracted cache.
    pub fn load_from_cache(cache: &Path) -> Result<Self, LoadError> {
        let scripts: Vec<&str> = sekiro_hks::PLAYER_SCRIPTS.to_vec();
        Self::assemble(
            cache,
            "c0000",
            ControlKind::Player,
            &scripts,
            "c0000.behbnd.d/c0000.hkx",
            AnimOffsets::player_default(),
        )
    }

    /// Loads an NPC such as `c1010` (Ashina soldier): the shared NPC script `c9997.hks`, then
    /// its own script, driven by the shared NPC behaviour graph its `behbnd` references.
    pub fn load_npc(cache: &Path, chr: &str) -> Result<Self, LoadError> {
        Self::load_npc_with_anim(cache, chr, chr)
    }

    /// Model variants keep their own identity and use the NpcParam animation donor.
    pub fn load_npc_with_anim(cache: &Path, chr: &str, anim_chr: &str) -> Result<Self, LoadError> {
        let own = format!("{anim_chr}.hks");
        let mut character = Self::assemble(
            cache,
            anim_chr,
            ControlKind::Npc,
            &["c9997.hks", &own],
            &format!("{anim_chr}.behbnd.d/c9997.hkx"),
            AnimOffsets::default(),
        )?;
        character.chr = chr.into();
        Ok(character)
    }

    /// The TAE state computed by the last step.
    pub fn tae_frame(&self) -> &TaeFrame {
        &self.tae_runtime.frame
    }

    /// Where this character's attacks aim (the exe's attack target point): Wolf's lock-on
    /// target, else his attack auto-aim target; an NPC's combat target.
    pub fn attack_target(&self) -> Option<[f32; 3]> {
        match self.kind {
            ControlKind::Player => self.lock_target.or_else(|| {
                self.homing
                    .and_then(|i| self.aim_candidates.get(i).copied())
            }),
            ControlKind::Npc => self.npc_target,
        }
    }

    /// The player moves with no lock-on and no auto-aim target, so moves run along the stick
    /// instead of strafing.
    fn free_movement(&self) -> bool {
        self.kind == ControlKind::Player && self.attack_target().is_none()
    }

    /// Runs the script and behaviour graph for one step, watching the script's auto-aim
    /// calls (`act(156)` refresh, `act(157)` stop) for the player.
    fn tick_behavior(&mut self, dt: f32) -> TickReport {
        let _timer = crate::perf::Scope::start("sim behavior HKS");
        if self.kind != ControlKind::Player {
            return self.behavior.tick(dt);
        }
        // Reuse the engine-call log to see the script's act calls, keeping any outer log.
        let outer = self.behavior.call_log.replace(Vec::new());
        let report = self.behavior.tick(dt);
        let calls = std::mem::replace(&mut self.behavior.call_log, outer).unwrap_or_default();
        let act = |id: i32| {
            calls.iter().any(|c| {
                c.function == "act" && c.args.first().and_then(|v| v.as_number()) == Some(id as f32)
            })
        };
        if act(ACT_STOP_AUTO_AIM) {
            self.homing = None;
        }
        if act(ACT_UPDATE_AUTO_AIM) && self.lock_target.is_none() {
            let aim = self.input.stick_world_yaw().unwrap_or(self.body.yaw);
            self.homing = self
                .auto_aim
                .pick(self.body.position, aim, &self.aim_candidates);
        }
        if let Some(log) = self.behavior.call_log.as_mut() {
            log.extend(calls);
        }
        report
    }

    /// World yaw from this character toward `p`.
    pub fn yaw_to(&self, p: [f32; 3]) -> f32 {
        let dx = p[0] - self.body.position[0];
        let dz = p[2] - self.body.position[2];
        (-dx).atan2(-dz)
    }

    /// The yaw the character is drawn with: the body's facing minus the lower-body twist that
    /// has not unwound yet, so a free move start turns the model smoothly while the body (and
    /// its root motion) already points along the stick.
    pub fn visual_yaw(&self) -> f32 {
        angle_diff(self.body.yaw - self.twist.to_radians(), 0.0)
    }

    /// Free movement (not locked on, no attack target): when the stick is pushed while Wolf is
    /// not already walking, running or sprinting and may start moving, the body turns to the
    /// stick at once and the turn becomes the lower-body twist. The script then sees the stick
    /// ahead (`TurnAngle` 0, so the forward move start) and the twist (`TwistLowerRootAngle`,
    /// so its 180 degree turn-and-go start past 135 degrees). The directional move starts,
    /// loops and stops it would otherwise pick from `TurnAngle` (back, left, right) are the
    /// locked-on strafes; played in free movement they carried Wolf sideways or backwards
    /// while steering turned him to face the stick.
    fn free_move_start_turn(&mut self) {
        let tae = &self.tae_runtime.frame;
        let moving = MOVING_REFS.iter().any(|r| tae.behavior_refs.contains(r));
        let ended = self
            .behavior
            .runtime
            .main_clip()
            .is_some_and(|c| c.at_end());
        let may_move = !tae.restricted || tae.flag(cancel::MOVE) || ended;
        // TAE 224 limits steering of the current clip, not the heading of a new
        // movement start. Stops have a zero rate but allow a new move immediately.
        // Keep the explicit turn prohibition (e.g. the sprint quick-turn wind-up).
        let may_turn = !tae.flag(cancel::DISABLE_TURN) && !tae.fixed_rotation;
        if !self.free_movement()
            || !self.body.grounded
            || (moving && !self.free_move_pending)
            || !may_move
            || !may_turn
        {
            return;
        }
        self.turn_body_to_stick();
    }

    /// The free move start's turn: the body faces the stick at once and the turn becomes the
    /// lower-body twist, which unwinds so the model turns smoothly.
    fn turn_body_to_stick(&mut self) {
        let Some(stick) = self.input.stick_world_yaw() else {
            return;
        };
        let turn = angle_diff(stick, self.body.yaw);
        self.snap_yaw(stick);
        self.twist = angle_diff((self.twist.to_radians()) + turn, 0.0).to_degrees();
        self.free_move_pending = false;
    }

    /// A new facing invalidates travel measured before the snap. Continuous steering
    /// remains in Body::step; action/throw/teleport snaps go through this handoff.
    pub fn snap_yaw(&mut self, yaw: f32) {
        self.body.yaw = angle_diff(yaw, 0.0);
        self.heading = None;
    }

    /// A lock release is a new free-movement start even with the stick held.
    /// Defer the turn through restricted reactions/throws until movement is permitted.
    pub fn release_move_lock(&mut self) {
        self.behavior.runtime.set_variable("MoveDirection", 0.0);
        self.walk_twist = 0.0;
        self.walk_twist_target = 0.0;
        self.snap_yaw(self.body.yaw);
        self.free_move_pending = true;
    }

    /// The locked-on walk twist (exe WalkTwist, facts from FwLo's RustSekiro,
    /// `docs/kb/movement.md`): while a locked-on walk or run plays with the stick pushed, the
    /// script's directional clip covers only its 60 degree cone (`MoveDirection`), so the exe
    /// sets `TwistMasterAngle` to the stick's angle minus the clip's direction. The graph's
    /// twist modifiers turn the Master bone (the legs and the travel) by it and turn the spine
    /// back, so a diagonal runs along the stick with the chest on the target. Anything else
    /// (stick released, sprinting, attacking) lets it return to 0. Called after the TAE update,
    /// so the walk and run behaviour refs are this step's.
    fn update_walk_twist(&mut self, lock_angle_released: bool, dt: f32) {
        let tae = &self.tae_runtime.frame;
        let walking = tae.behavior_refs.contains(&MOVING_REFS[0])
            || tae.behavior_refs.contains(&MOVING_REFS[1]);
        let stick = self.input.stick_world_yaw();
        self.walk_twist_target = match stick {
            Some(stick)
                if self.kind == ControlKind::Player
                    && self.lock_target.is_some()
                    && !lock_angle_released
                    && self.body.grounded
                    && walking =>
            {
                let angle = -angle_diff(stick, self.body.yaw).to_degrees();
                let class = self
                    .behavior
                    .runtime
                    .variable("MoveDirection")
                    .unwrap_or(0.0) as usize;
                let clip = STRAFE_CLIP_ANGLE.get(class).copied().unwrap_or(0.0);
                angle_diff((angle - clip).to_radians(), 0.0).to_degrees()
            }
            _ => 0.0,
        };
        let step = WALK_TWIST_RATE * dt;
        self.walk_twist += (self.walk_twist_target - self.walk_twist).clamp(-step, step);
    }

    fn set_engine_variables(&mut self) {
        let lock_yaw = self.lock_target.map(|t| self.yaw_to(t));
        let rt = &mut self.behavior.runtime;
        match self.kind {
            ControlKind::Player => {
                rt.set_variable("TwistLowerRootAngle", self.twist);
                // Radians, positive with the legs turned right (FwLo's RustSekiro, actor.rs).
                rt.set_variable("TwistMasterAngle", self.walk_twist.to_radians());
                let level = self.input.stick_level();
                // Stick direction relative to `from` in degrees, positive to the RIGHT: the script
                // quick-turns right for TurnAngle > 0 and picks the right strafe for MoveAngle > 0.
                let stick_from = |from: f32| {
                    self.input
                        .stick_world_yaw()
                        .map(|y| -angle_diff(y, from).to_degrees())
                        .unwrap_or(0.0)
                };
                let turn = stick_from(self.body.yaw);
                // `TurnAngle` is measured from where the body is travelling (the facing when it
                // stands still or walks in place). In free movement they are the same. Locked on
                // they differ: a back step travels toward the stick while Wolf faces the target,
                // and the script brakes a locked sprint start when |TurnAngle| >= 135
                // (`BEH_A_GROUND_MOVE_STOP`, SprintStartFromStep reads TurnAngle). Measured from
                // the facing, the back sprint start (a000_001152, turning 180 degrees by root
                // motion) could never play: every locked back sprint braked into the sprint stop
                // slide toward the enemy. Inferred from the script and clips, not traced in the exe.
                let turn_from_travel = stick_from(self.heading.unwrap_or(self.body.yaw));
                // FwLo's live-facing correction: action angles are against the current facing.
                // The engine's stick level runs 0..2; full keyboard or stick deflection runs.
                rt.set_variable("MoveSpeedLevel", level * 2.0);
                for name in ["TurnAngle", "TurnAngleReal"] {
                    rt.set_variable(name, turn_from_travel);
                }
                for name in ["MoveAngle", "MoveAngleReal", "JumpAngle", "KickAngle"] {
                    rt.set_variable(name, turn);
                }
                // Attack targeting remains on the existing combat path.
                for name in ["AttackAngle", "SubAttackAngle"] {
                    rt.set_variable(name, lock_yaw.map_or(turn, stick_from));
                }
                for name in [
                    "JumpStickLevel",
                    "AttackStickLevel",
                    "SubAttackStickLevel",
                    "KickStickLevel",
                ] {
                    rt.set_variable(name, level);
                }
                // Locked on, the engine keeps the strafe direction (`MoveDirection`, an engine
                // variable the move loop's selectors read: 0 forward, 1 back, 2 left, 3 right)
                // following the stick; the script only seeds it when a move starts. The exe's
                // WalkTwist classifier uses 60 degree cones with 5 degrees of hysteresis (traced
                // by FwLo's RustSekiro, `docs/kb/movement.md`). Without it, a strafe kept its
                // first direction: right, then left on the stick, still went right.
                if self.lock_target.is_some() && level > 0.0 {
                    let current = rt.variable("MoveDirection").unwrap_or(0.0) as i32;
                    rt.set_variable("MoveDirection", strafe_direction(current, turn) as f32);
                }
            }
            ControlKind::Npc => {
                // The NPC script reads MoveAngle with positive to the right (`GetMoveDir`).
                let angle = self
                    .npc
                    .move_yaw
                    .map(|y| -angle_diff(y, self.body.yaw).to_degrees())
                    .unwrap_or(0.0);
                rt.set_variable("MoveSpeedLevel", self.npc.move_level);
                rt.set_variable("MoveAngle", angle);
                rt.set_variable("TurnAngle", angle);
            }
        }
    }

    fn fill_env(&mut self, dt: f32) {
        let authored_airborne = self.authored_air_height() > 0.0;
        let free_movement = self.free_movement();
        let tae = &self.tae_runtime.frame;
        let behavior = &mut *self.behavior;
        let env = &mut behavior.env;
        env.requested = self
            .input
            .pending_actions()
            .filter(|a| self.input.requested(*a, tae))
            .collect();
        env.held_ms = self.input.held().collect();
        env.sp_effect_refs = tae.behavior_refs.clone();
        env.havok_variables.clone_from(&tae.havok_variables);
        // Charged air reactions have no ordinary attack accept flag. Keep the
        // held attack available to HKS, which owns the charged-state transition.
        if !self.body.grounded
            && (tae.behavior_refs.contains(&510) || tae.behavior_refs.contains(&511))
            && !behavior.runtime.main_clip().is_some_and(|c| {
                matches!(
                    c.animation.as_str(),
                    "a050_308900" | "a050_308901" | "a050_308910" | "a050_308911"
                )
            })
            && self.input.held_ms(crate::input::arm::ATTACK) > 0.0
        {
            env.requested.insert(crate::input::arm::ATTACK);
        }
        if self.kind == ControlKind::Npc {
            env.sp_effect_refs.extend(self.npc.refs.iter().copied());
            env.ai_action = self.npc.action;
            // Which cancel window offers the queued action to the attack's update (ours: the
            // window flag must be on and the request must be of the matching kind).
            let a = self.npc.action;
            let open = |flag: i32, ok: bool| if ok && a > 0 && tae.flag(flag) { a } else { 0 };
            env.ai_cancel_actions = [
                open(ai_cancel::COMBO, self.npc.combo),
                open(ai_cancel::ATTACK, !is_step_action(a)),
                open(ai_cancel::STEP, is_step_action(a)),
            ];
        }
        // Free animations (no action flags at all) are standby; otherwise the move-cancel window
        // marks both standby and move cancel. Ended clips count as standby too.
        let ended = behavior.runtime.main_clip().is_some_and(|c| c.at_end());
        let move_window = !tae.restricted || tae.flag(cancel::MOVE) || ended;
        env.standby = Some(move_window);
        env.move_cancel = Some(move_window);
        env.landed = self.body.grounded && !authored_airborne;
        let [x, y, z] = self.body.position;
        // An overhead pond does not flood a cave separated from it by solid collision.
        let water = self.ground.water_surface(x, z, f32::INFINITY).filter(|w| {
            self.ground
                .floor(x, z, w.height + 0.01)
                .is_none_or(|floor| floor <= y + self.ground.max_step())
        });
        env.in_water = water.is_some_and(|w| {
            w.deep()
                && y <= w.height + 0.01
                && self
                    .ground
                    .floor(x, z, w.height + 0.01)
                    .is_none_or(|floor| floor < w.height - self.ground.max_step())
        });
        env.can_dive = env.in_water;
        env.can_surface = env.in_water && water.is_some_and(|w| y >= w.height - 0.05);
        if env.in_water && water.is_some_and(|w| y < w.height - 0.05) {
            env.landed = self
                .ground
                .floor(x, z, y + 0.01)
                .is_some_and(|floor| y - floor <= 0.01);
        }
        // The waterside action restriction belongs to swimming water, not shallow floors.
        if env.in_water {
            env.sp_effect_refs.insert(110005);
        }
        env.landing_ready = self.wire.active.is_some()
            && self.body.falling()
            && self
                .ground
                .floor(x, z, y)
                .is_some_and(|floor| y - floor <= -self.body.velocity[1] * dt);
        env.falling = self.body.falling();
        // c0000_transition.lua multiplies env 224 by 0.01 and compares 2000 raw.
        env.fall_height = self.body.last_fall_height * 100.0;
        env.vertical_velocity = self.body.velocity[1] * 100.0;
        env.dt = dt;
        env.locked_on = self.lock_target.is_some();
        env.free_movement = free_movement;
        env.damage = self.pending_damage.take();
        env.damage_motion_disabled = tae.flag(cancel::DISABLE_DAMAGE_MOTION);
    }

    /// Runs one step of `dt` seconds. `frame` is the player's input; NPCs read [`Self::npc`].
    pub fn tick(&mut self, frame: &InputFrame, dt: f32) -> StepReport {
        let _timer = crate::perf::Scope::start("sim character");
        let interrupted_height = if self.pending_damage.is_some() {
            self.authored_air_height()
        } else {
            0.0
        };
        // All held hang clips clear the restriction: HKS gates climb/drop during idle
        // and shimmy, while their TAE lacks the ground combat step/jump cancel windows.
        let docking_idle = self.docking.active.is_some()
            && self.behavior.runtime.main_clip().is_some_and(|c| {
                crate::docking::is_hang_clip(&c.animation)
                    || crate::docking::is_cover_clip(&c.animation)
            });
        if self.kind == ControlKind::Player {
            let mut input_tae = self.tae_runtime.frame.clone();
            if docking_idle {
                input_tae.restricted = false;
            }
            self.input.update(frame, dt, &input_tae);
            if self.docking.active.is_none() {
                self.free_move_start_turn();
            }
        }
        self.set_engine_variables();
        self.fill_env(dt);
        if docking_idle {
            self.behavior.env.standby = Some(true);
            self.behavior.env.requested = self
                .input
                .pending_actions()
                .filter(|a| self.input.requested(*a, &TaeFrame::default()))
                .collect();
        }
        // The script asks for the air hang (act 3028) every update of a state that allows it,
        // so last step's ask gates this step's edge scan.
        self.docking.skip_air_search = self.behavior.env.docking_searches.get() & 4 == 0;
        self.docking.search(&self.body, self.ground.as_ref());
        // env 3020 and ContactWallAngle: the airborne wall contact the wall jump reads, only in
        // the TAE ref 104 (ENABLE_WALL_JUMP_CANCEL) window that `BEH_A_WALL_JUMP` requires. The
        // script itself allows one wall jump a flight (g_wallJumpCount, reset on landing).
        let wall = if !self.body.grounded
            && self.docking.active.is_none()
            && self.behavior.env.sp_effect_refs.contains(&104)
        {
            crate::docking::wall_contact(
                glam::Vec3::from(self.body.position),
                self.body.yaw,
                self.ground.as_ref(),
            )
        } else {
            None
        };
        self.behavior.env.wall_contact = wall.is_some();
        self.behavior
            .runtime
            .set_variable("ContactWallAngle", wall.unwrap_or(0.0));
        self.wire.search(
            &self.body,
            frame.camera_yaw,
            frame.camera_pitch,
            self.lock_target.is_some(),
        );
        self.behavior.env.wire_candidate = self.wire.can_fire();
        let wire_target = if frame.buttons.grapple {
            self.wire.candidate.as_ref()
        } else {
            self.wire.active.as_ref().or(self.wire.candidate.as_ref())
        };
        if let Some(t) = wire_target {
            let d = t.point - glam::Vec3::from(self.body.position);
            self.behavior
                .runtime
                .set_variable("WireDramaticAngle", d.y.atan2(d.x.hypot(d.z)).to_degrees());
        }
        self.behavior.env.wire_precompute.set(None);
        self.behavior.env.wire_prefer.set(false);
        let dock = self
            .docking
            .active
            .as_ref()
            .or(self.docking.candidate.as_ref());
        let behavior = &mut *self.behavior;
        let env = &mut behavior.env;
        env.docking_searches.set(0);
        env.action_guide.set(None);
        env.hang_candidate = self.body.grounded
            && self
                .docking
                .candidate
                .as_ref()
                .is_some_and(|t| !self.docking.is_cover(t));
        env.air_hang_type = self.docking.air_hang_type;
        env.cover_candidate = self
            .docking
            .candidate
            .as_ref()
            .is_some_and(|t| self.docking.is_cover(t));
        env.cover_active = self
            .docking
            .active
            .as_ref()
            .is_some_and(|t| self.docking.is_cover(t));
        env.docking_blocked = self.docking.active.as_ref().map_or([false; 2], |t| {
            if self.docking.is_cover(t) {
                [
                    self.docking.cover_blocked(t, self.ground.as_ref(), true),
                    self.docking.cover_blocked(t, self.ground.as_ref(), false),
                ]
            } else {
                [t.end_blocked(true), t.end_blocked(false)]
            }
        });
        env.docking_variation = dock.map_or(0, |t| self.docking.edges[t.edge].variation);
        env.docking_end = dock.map_or(0, |t| t.end);
        env.hang_can_climb = self
            .docking
            .active
            .as_ref()
            .is_some_and(|t| self.docking.hang_can_climb(t, self.ground.as_ref()));
        behavior
            .runtime
            .set_variable("HangStartEdgeAngle", dock.map_or(0.0, |t| t.angle));
        behavior
            .runtime
            .set_variable("CoverStartEdgeAngle", dock.map_or(0.0, |t| t.angle));
        // Measured in the docking frame, which faces the wall (`Target::right`): in cover,
        // Wolf's back is to the wall, so his own left is the script's right.
        let docking_yaw = self
            .docking
            .active
            .as_ref()
            .map_or(self.body.yaw, |t| t.yaw());
        let docking_angle = self
            .input
            .stick_world_yaw()
            .map_or(0.0, |yaw| -angle_diff(yaw, docking_yaw).to_degrees());
        behavior
            .runtime
            .set_variable("DockingStickAngle", docking_angle);
        // Placeholder away-stick exit: offer arm 12 to the existing HKS end transition.
        if env.cover_active
            && docking_idle
            && self.input.stick_world_yaw().is_some()
            && crate::docking::cover_stick_away(docking_angle)
        {
            env.requested.insert(crate::input::arm::ACTION);
        }
        let requested: Vec<i32> = self.behavior.env.requested.iter().copied().collect();
        let resets_before = self.behavior.env.input_resets.get();

        let behavior = self.tick_behavior(dt);
        self.behavior.env.damage = None;

        if self.behavior.env.input_resets.get() != resets_before {
            self.input.reset_requests();
        }
        let state_key = self.behavior.runtime.main_state_key();
        let state = self.behavior.runtime.main_state_name();
        let locked_step = self.lock_target.is_some() && state == "GroundStep";
        let positioning_jump = self.lock_target.is_some() && state.contains("GroundJump");
        // FwLo's live-facing correction: step/jump angles use the current facing,
        // not a snap to the lock target. The existing HKS tilt still applies.
        if self.behavior.env.face_lock_target.take()
            && !locked_step
            && !positioning_jump
            && let Some(t) = self.lock_target
        {
            self.snap_yaw(self.yaw_to(t));
        }
        if let Some(deg) = self.behavior.env.face_turn.take() {
            self.snap_yaw(self.body.yaw - deg.to_radians());
        }
        // A free move start the script began off the old facing (held at forward) runs along
        // the stick, as the idle start does.
        if self.behavior.env.free_move_turn.take() {
            self.turn_body_to_stick();
        }
        let lock_angle_released = self.behavior.env.release_lock_angle.take();

        // Clips now playing, with the time each had before this step (0 if it just started).
        let main_node = self.behavior.runtime.main_clip().map(|c| c.node);
        let mut playing: Vec<(NodeId, PlayingClip)> = self
            .behavior
            .runtime
            .clips()
            .map(|c| {
                let prev = match self.last_times.get(&c.node) {
                    Some((name, t)) if *name == c.animation && *t <= c.time => *t,
                    Some((name, _)) if *name == c.animation => 0.0,
                    _ => c.start_time,
                };
                (
                    c.node,
                    PlayingClip {
                        animation: c.animation.clone(),
                        prev_time: prev,
                        time: c.time,
                        looping: c.looping,
                    },
                )
            })
            .collect();
        playing.sort_by_key(|(n, _)| (Some(*n) != main_node, *n));
        let tae_timer = crate::perf::Scope::start("sim TAE");
        self.tae_runtime.update(
            &self.tae,
            &playing.iter().map(|(_, c)| c.clone()).collect::<Vec<_>>(),
            dt,
        );
        drop(tae_timer);
        // NPC animation-set effects persist after the TAE window, replacing the resident
        // category-100 choice. CMSG offset type 15 selects a000/a100/... (FwLo's notes).
        if self.kind == ControlKind::Npc {
            for id in &self.tae_runtime.frame.sp_effects {
                if let Some(info) = self.tae.sp_effects.get(id)
                    && (270..=274).contains(&info.state_info)
                    && !self.last_anim_effects.contains(id)
                {
                    self.behavior
                        .runtime
                        .offsets
                        .by_type
                        .insert(15, (info.state_info as u32 - 270) * 100);
                }
            }
        }
        self.last_anim_effects = self
            .tae_runtime
            .frame
            .sp_effects
            .iter()
            .copied()
            .filter(|id| {
                self.tae
                    .sp_effects
                    .get(id)
                    .is_some_and(|info| (270..=274).contains(&info.state_info))
            })
            .collect();
        self.last_times = playing
            .iter()
            .map(|(n, c)| (*n, (c.animation.clone(), c.time)))
            .collect();

        // Body.
        let main = playing
            .iter()
            .find(|(n, _)| Some(*n) == main_node)
            .map(|(_, c)| c.clone());
        // If a full-body reaction actually replaces an authored aerial clip, retain
        // its elevation as physical flight. Subsequent env(201)/fall transitions
        // then use the NPC's own damage/fall/landing graph instead of grounding it.
        if interrupted_height > 0.0
            && main.as_ref().map(|c| &c.animation) != self.last_main.as_ref().map(|c| &c.animation)
        {
            self.body.position[1] += interrupted_height;
            self.body.grounded = false;
            self.body.velocity[1] = 0.0;
        }
        let main_length = self
            .behavior
            .runtime
            .main_clip()
            .and_then(|c| c.duration)
            .unwrap_or(0.0);
        let changed =
            main.as_ref().map(|c| &c.animation) != self.last_main.as_ref().map(|c| &c.animation);
        if changed {
            let duration = main
                .as_ref()
                .and_then(|c| self.tae.blend_in(&c.animation))
                .unwrap_or(0.0);
            // FwLo's procedural locomotion plays its start/loop root directly. Those
            // clips already contain the acceleration; blending them in from idle
            // applies a second ramp and falls behind his recorded start speeds.
            let direction_layer = state_key == self.last_motion_state
                || self.behavior.runtime.main_is_locomotion_layer();
            self.motion_blend = self
                .last_main
                .as_ref()
                .filter(|_| {
                    self.kind == ControlKind::Player
                        && duration > 0.0
                        && !direction_layer
                        && self.behavior.runtime.blend_keeps_source()
                })
                .map(|c| MotionBlend {
                    animation: c.animation.clone(),
                    start: c.time,
                    looping: c.looping,
                    twist: self.walk_twist_target,
                    elapsed: 0.0,
                    duration,
                });
        }
        let mut root_motion = main.as_ref().map_or([0.0; 4], |c| {
            let (start, elapsed, looping) = if self.kind == ControlKind::Player && c.looping {
                let start = self
                    .last_main
                    .as_ref()
                    .filter(|p| p.animation == c.animation)
                    .map_or(c.prev_time, |p| p.time);
                let elapsed = if c.time < start || (changed && c.time == 0.0) {
                    dt
                } else {
                    (c.time - start).max(0.0)
                };
                (start, elapsed, true)
            } else {
                // Preserve non-looping target sampling and its end-event handover.
                let time = if c.prev_time == 0.0 && c.time == 0.0 {
                    dt
                } else {
                    c.time
                };
                let start = c.prev_time.min(main_length);
                (start, (time.min(main_length) - start).max(0.0), false)
            };
            self.clips.root_delta(&c.animation, start, elapsed, looping)
        });
        // Locked-on walks and runs travel along the clip's motion turned by the walk twist, so
        // a diagonal goes along the stick, not along the nearest of the four strafe clips.
        // Turned by the target (the stick's angle off the clip) rather than the eased value:
        // when the class changes, the clip switches at once, and the eased twist would carry
        // the new clip off the stick until it caught up.
        self.update_walk_twist(lock_angle_released, dt);
        if self.walk_twist_target != 0.0 {
            // Source X is mirrored (as in `Body::step`), so a turn to the right is a positive
            // turn about source Y: x' = x cos + z sin, z' = -x sin + z cos.
            let [x, z] = crate::motion::rotate_xz(
                [root_motion[0], root_motion[2]],
                self.walk_twist_target.to_radians(),
            );
            root_motion[0] = x;
            root_motion[2] = z;
        }
        let mut root_blend_weight = 1.0;
        if let Some(blend) = self.motion_blend.as_mut() {
            let mut source = self.clips.root_delta(
                &blend.animation,
                blend.start + blend.elapsed,
                dt,
                blend.looping,
            );
            let [x, z] = crate::motion::rotate_xz([source[0], source[2]], blend.twist.to_radians());
            source[0] = x;
            source[2] = z;
            let weight = crate::motion::smoothstep(blend.elapsed / blend.duration);
            root_blend_weight = weight;
            for i in [0, 2, 3] {
                root_motion[i] = source[i] * (1.0 - weight) + root_motion[i] * weight;
            }
            blend.elapsed += dt;
            if blend.elapsed >= blend.duration {
                self.motion_blend = None;
            }
        }
        let tae = &self.tae_runtime.frame;
        // TAE 760: lengthen or shorten the attack's travel so it arrives at its target.
        let target = self.attack_target();
        if let (Some(r), Some(t)) = (tae.reach_target.as_ref(), target) {
            let scale = reach_scale(r, self.body.position, t);
            root_motion[0] *= scale;
            root_motion[2] *= scale;
        }
        let facing_guard = self.kind == ControlKind::Player
            && self.lock_target.is_some()
            && tae.guard_judge.is_some()
            && tae.flag(cancel::MOVE);
        let turn_speed = if tae.flag(cancel::DISABLE_TURN) {
            0.0
        } else {
            tae.turn_speed_for(self.kind == ControlKind::Npc || self.lock_target.is_some())
                // A cancellable guard tracks its lock target even when its idle
                // locomotion rate is zero. Authored entry rates still take precedence.
                .filter(|speed| !facing_guard || *speed != 0.0)
                .unwrap_or(DEFAULT_TURN_SPEED)
        };
        let velocity_change = tae
            .velocity_changes
            .iter()
            .find_map(|id| self.tae.velocity_changes.get(id).copied());
        for id in &tae.velocity_scales {
            if let Some(scale) = self.tae.velocity_changes.get(id) {
                self.body.velocity[0] *= scale.horizontal_scale;
                self.body.velocity[2] *= scale.horizontal_scale;
                self.body.velocity[1] *= scale.vertical_scale;
            }
        }
        let steer_to = match self.kind {
            // TAE 703: the swing keeps its direction (what makes side-steps work).
            // Keep our data-backed step turn window (a000_2133xx: disabled to
            // 0.4 s and after 0.6 s); it tracks the target only inside that window.
            _ if tae.fixed_rotation || positioning_jump => None,
            // Locked on, the character keeps facing the target and strafes; an attack with an
            // auto-aim target turns toward it the same way. A sprint releases the lock-on angle
            // lock (act 2018) so Wolf turns to run along the stick, still locked on.
            ControlKind::Player => match target {
                Some(_) if lock_angle_released && self.lock_target.is_some() => {
                    self.input.stick_world_yaw()
                }
                Some(t) => Some(self.yaw_to(t)),
                None => self.input.stick_world_yaw(),
            },
            ControlKind::Npc => self.npc.face_yaw,
        };
        let body_input = BodyInput {
            ignore_walls: self.throw_ignore_walls,
            water_mode: self.behavior.env.water_mode.get(),
            fall_assist: self.behavior.env.fall_assist.get(),
            stick: if self.kind == ControlKind::Player {
                self.input
                    .stick_world_yaw()
                    .map_or(glam::Vec2::ZERO, |yaw| {
                        glam::Vec2::new(-yaw.sin(), -yaw.cos()) * self.input.stick_level()
                    })
            } else {
                glam::Vec2::ZERO
            },
            root_motion,
            steer_to,
            turn_speed,
            velocity_change,
        };
        // Where the clip carries the body: the root-motion offset from the facing (source X is
        // mirrored, as in `Body::step`), turned by the facing it is applied along.
        let [rx, _, rz, _] = root_motion;
        self.heading = (rx.hypot(rz) > HEADING_MIN_SPEED * dt && self.body.grounded)
            .then(|| angle_diff(self.body.yaw + rx.atan2(-rz), 0.0));
        let root_progress = main
            .as_ref()
            .filter(|c| crate::docking::CLIMB_CLIPS.contains(&c.animation.as_str()))
            .and_then(|c| {
                let clip = self.clips.clip(&c.animation)?;
                let rm = clip.root_motion.as_ref()?;
                let start = glam::Vec3::from_array(rm.sample(0.0)[..3].try_into().ok()?);
                let end = glam::Vec3::from_array(rm.sample(clip.duration)[..3].try_into().ok()?);
                let current = glam::Vec3::from_array(rm.sample(c.time)[..3].try_into().ok()?);
                let delta = end - start;
                (delta.length_squared() > 1e-6)
                    .then(|| (current - start).dot(delta) / delta.length_squared())
            });
        let travel_window = main
            .as_ref()
            .and_then(|c| self.tae.anim(&c.animation))
            .and_then(|a| {
                a.events
                    .iter()
                    .find(|e| matches!(e.kind, crate::tae::TaeKind::Wire { kind: 901, .. }))
            })
            .map(|e| (e.start, e.end))
            .or_else(|| {
                // Slice (a)'s act 3023 pre-evaluation: retain the requested clip's wire window.
                // Native root-motion warping remains a documented approximation.
                let (kind, animation) = self.behavior.env.wire_precompute.get()?;
                if !(2..=6).contains(&kind) {
                    return None;
                }
                let name = sekiro_formats::tae::anim_name(i64::from(animation));
                self.tae
                    .anim(&name)?
                    .events
                    .iter()
                    .find(|e| matches!(e.kind, crate::tae::TaeKind::Wire { kind: 901, .. }))
                    .map(|e| (e.start, e.end))
            });
        if !self.wire.advance(
            &mut self.body,
            main.as_ref().map(|c| c.animation.as_str()),
            main.as_ref().map_or(0.0, |c| c.time),
            main.as_ref().and_then(|c| self.tae.anim(&c.animation)),
            travel_window,
            dt,
            self.ground.as_ref(),
        ) && !self.docking.cover_step(
            &mut self.body,
            main.as_ref().map(|c| c.animation.as_str()),
            self.input.stick_world_yaw(),
            root_motion[0].hypot(root_motion[2]),
            self.ground.as_ref(),
        ) && !self.docking.step(
            &mut self.body,
            main.as_ref().map(|c| c.animation.as_str()),
            main.as_ref().map_or(0.0, |c| c.time),
            root_progress,
            root_motion,
            self.ground.as_ref(),
        ) {
            // A foe's walk keeps to the navmesh: the NPC script asks for no fall-prevention
            // assist in its walk and run states, which its other actions do (#227).
            let nav = self.navmesh.as_deref().filter(|_| {
                self.kind == ControlKind::Npc
                    && body_input.fall_assist == 0
                    && !body_input.ignore_walls
                    && self.body.grounded
            });
            if let Some(nav) = nav {
                self.body.step(
                    &body_input,
                    dt,
                    &crate::navmesh::NavGround {
                        terrain: self.ground.as_ref(),
                        nav,
                    },
                );
            } else if body_input.water_mode.is_some() || self.wire.perch.is_none() {
                self.body.step(&body_input, dt, self.ground.as_ref());
            } else {
                self.body.step(
                    &body_input,
                    dt,
                    &crate::wire::PerchGround {
                        terrain: self.ground.as_ref(),
                        perch: self.wire.perch,
                    },
                );
            }
        }
        self.wire.leave_perch(Vec3::from(self.body.position));
        if self.wire.active.is_none() {
            self.behavior.env.wire_action.set(false);
        }
        // The lower body catches up with the facing at the default turn speed.
        let unwind = DEFAULT_TURN_SPEED * dt;
        self.twist -= self.twist.clamp(-unwind, unwind);

        let mut refs: Vec<i32> = tae.behavior_refs.iter().copied().collect();
        refs.sort_unstable();
        let mut flags: Vec<i32> = tae.flags.iter().copied().collect();
        flags.sort_unstable();
        let clip_started = main.as_ref().is_some_and(|c| c.prev_time == 0.0);
        let blend_in = main
            .as_ref()
            .and_then(|c| self.tae.blend_in(&c.animation))
            .unwrap_or(0.0);
        self.last_main = main.clone();
        self.last_motion_state = state_key;
        StepReport {
            animation: main.as_ref().map(|c| c.animation.clone()),
            anim_time: main.map(|c| c.time).unwrap_or(0.0),
            behavior,
            position: self.body.position,
            yaw: self.body.yaw,
            grounded: self.body.grounded,
            sp_effects: tae.sp_effects.clone(),
            behavior_refs: refs,
            action_flags: flags,
            requested,
            clip_started,
            blend_in,
            root_blend_weight,
            advanced: dt,
            fade: 1.0 - tae.opacity(),
        }
    }

    /// Clip length lookup shared with the renderer.
    pub fn clip_duration(&mut self, animation: &str) -> Option<f32> {
        self.clips.duration(animation)
    }

    /// Source-space model matrices of every skeleton bone for the full-body clip as of the last
    /// step. Map points to the world with [`Self::source_to_world`].
    pub fn bone_source_matrices(&self) -> Vec<Mat4> {
        let Some(main) = &self.last_main else {
            return Vec::new();
        };
        let Some(clip) = self.clips.clip(&main.animation) else {
            return Vec::new();
        };
        let pose = clip.sample(main.time.min(clip.duration), &self.skeleton);
        self.skeleton.model_space(&pose)
    }

    /// NPC jumps can lift both authored foot targets while the physics root stays grounded.
    /// Use their support height relative to the clip's starting pose for env(201).
    /// The 15 cm contact tolerance avoids treating ordinary footwork as flight.
    pub fn authored_air_height(&self) -> f32 {
        if self.kind != ControlKind::Npc || !self.body.grounded {
            return 0.0;
        }
        let Some(main) = &self.last_main else {
            return 0.0;
        };
        let Some(clip) = self.clips.clip(&main.animation) else {
            return 0.0;
        };
        let feet: Option<Vec<_>> = ["L_Foot_Target", "R_Foot_Target"]
            .iter()
            .map(|name| self.skeleton.bone_index(name))
            .collect();
        let Some(feet) = feet else { return 0.0 };
        let current = self
            .skeleton
            .model_space(&clip.sample(main.time, &self.skeleton));
        let start = self.skeleton.model_space(&clip.sample(0.0, &self.skeleton));
        let support = |pose: &[Mat4]| {
            feet.iter()
                .map(|i| pose[*i].w_axis.y)
                .fold(f32::INFINITY, f32::min)
        };
        let lift = support(&current) - support(&start);
        if lift > 0.15 { lift } else { 0.0 }
    }

    /// Source model space to Bevy world space: mirror X, then place at the body.
    pub fn source_to_world(&self) -> Mat4 {
        Mat4::from_translation(Vec3::from_array(self.body.position))
            * Mat4::from_rotation_y(self.body.yaw)
            * Mat4::from_scale(Vec3::new(-1.0, 1.0, 1.0))
    }

    /// The name of the state the full-body machine is in.
    pub fn state(&self) -> String {
        self.state_name().to_owned()
    }

    pub fn state_name(&self) -> &str {
        self.behavior.runtime.main_state_name()
    }

    /// Steps without input until the character stands idle (`StandIdle` for the player, an
    /// idle state for NPCs), up to `max_steps`.
    pub fn settle(&mut self, dt: f32, max_steps: usize) -> usize {
        for i in 0..max_steps {
            let s = self.state_name();
            if s == "StandIdle" || (self.kind == ControlKind::Npc && s.starts_with("Idle")) {
                return i;
            }
            self.tick(&InputFrame::default(), dt);
        }
        max_steps
    }
}
