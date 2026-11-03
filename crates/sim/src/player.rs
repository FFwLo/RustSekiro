//! The player character driven by the game's own HKS scripts and behaviour graph.
//!
//! Each tick runs, in order:
//!
//! 1. `Update()`.
//! 2. `<State>_onUpdate()` for every active state that has such a hook.
//! 3. Queued behaviour events are applied; `_onDeactivate` hooks of left states and then
//!    `_onActivate` hooks of entered states run. Hooks may fire more events; this repeats up to
//!    [`MAX_EVENT_ROUNDS`] times.
//! 4. Clips that have ended send the end event their CMSG asks for (see
//!    [`BehaviorRuntime::queue_clip_end_events`]), and step 3 repeats for those.
//! 5. Active clips advance by `dt`.
//!
//! The order of steps 1 to 3 within a frame is our best reading of the engine, not verified.
//! Engine queries the runtime can answer (animation end, behaviour variables, node activity)
//! come from [`BehaviorRuntime`]; the rest come from [`PlayerEnv`], which a fuller simulation
//! will fill from TAE events, SpEffects, params and physics.

use crate::behavior::{AnimOffsets, BehaviorRuntime, ClipDurations, ClipState, HookCall};
use sekiro_formats::hkb::Graph;
use sekiro_hks::host::CallRecord;
use sekiro_hks::{CommandId, Host, PLAYER_SCRIPTS, Value, Vm, VmError};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub const MAX_EVENT_ROUNDS: usize = 8;

// env ids, see docs/HKS-INTERFACE.md.
pub const ENV_LANDED: i32 = 201;
pub const ENV_ARM_STYLE: i32 = 207;
pub const ENV_WEAPON_CATEGORY: i32 = 225;
pub const ENV_REALLY_LANDED: i32 = 248;
pub const ENV_VERTICAL_VELOCITY: i32 = 3003;
pub const ENV_ANIM_END: i32 = 339;
pub const ENV_HP: i32 = 1000;
/// Remaining posture ("stamina"); the NPC script divides it by env 2010 for `GetStaminaRatio`.
pub const ENV_POSTURE: i32 = 1001;
pub const ENV_MAX_POSTURE: i32 = 2010;
pub const ENV_STANDBY_STATE: i32 = 1105;
pub const ENV_ACTION_REQUEST: i32 = 1106;
pub const ENV_ACTION_HELD: i32 = 1108;
pub const ENV_MOVE_CANCEL: i32 = 2000;
pub const ENV_ACTION_UNLOCKED: i32 = 3033;
pub const ENV_SP_EFFECT_REF: i32 = 3036;
pub const ENV_CAN_RELEASE_CROUCH: i32 = 3037;
pub const ACT_SET_VARIABLE: i32 = 148;
pub const ACT_RESET_INPUT: i32 = 9101;
pub const ACT_FACE_DIRECTION: i32 = 3025;
pub const ACT_FACE_LOCK_TARGET: i32 = 2019;
pub const ACT_RELEASE_LOCK_ANGLE: i32 = 2018;
/// Sets bit 21 of the action state's flag word, which makes the character's per-frame update
/// skip posture recovery (`b32410` case 0x6e, read by `a04850`); named
/// `SetStaminaRecoveryDisabled` in the community HKS list.
pub const ACT_SET_STAMINA_RECOVERY_DISABLED: i32 = 110;
pub const ENV_FALLING: i32 = 200;
pub const ENV_FALL_HEIGHT: i32 = 224;
pub const ENV_DT: i32 = 333;
pub const ENV_LOCKED_ON: i32 = 1118;
pub const ENV_DAMAGE_TYPE: i32 = 202;
pub const ENV_DAMAGE_DIRECTION: i32 = 222;
pub const ENV_DAMAGE_LEVEL: i32 = 236;
pub const ENV_GUARD_LEVEL: i32 = 237;
pub const ENV_GUARD_MOTION_LEVEL: i32 = 258;
pub const ENV_TOOK_DAMAGE: i32 = 256;
pub const ENV_DAMAGE_ELEMENT: i32 = 285;
pub const ENV_DAMAGE_BEHAVIOR_ID: i32 = 334;
pub const ENV_DAMAGE_MOTION_DISABLED: i32 = 349;
pub const ENV_AI_ACTION: i32 = 106;
/// The AI's queued action as the script's cancel windows read it ("AI combo action type",
/// "AI attack cancel type", "AI step type"; env names from the community env list).
pub const ENV_AI_COMBO_ACTION: i32 = 107;
pub const ENV_AI_STEP_ACTION: i32 = 118;
pub const ENV_AI_CANCEL_ACTION: i32 = 120;
pub const ENV_ANIM_EXISTS: i32 = 1114;
pub const ENV_GUARD_REPELLED: i32 = 3018;
pub const ENV_JUST_GUARD_REPELLED: i32 = 3019;
pub const ENV_DAMAGE_FRONT_BACK: i32 = 3032;
pub const ENV_GUARD_REPEL_DIR: i32 = 3056;
pub const ENV_JUST_GUARD_REPEL_DIR: i32 = 3057;

/// Action-arm ids used as the argument of env 1106 and 1108.
pub mod arm {
    pub const ATTACK: i32 = 0;
    pub const GUARD: i32 = 2;
    pub const JUMP: i32 = 4;
    pub const STEP: i32 = 5;
}

/// The engine-side facts the script asks about that the behaviour runtime cannot answer.
#[derive(Debug, Clone)]
pub struct PlayerEnv {
    pub in_water: bool,
    pub can_dive: bool,
    pub can_surface: bool,
    /// This tick's HKS movement mode: Some(false) surface, Some(true) underwater.
    pub water_mode: std::cell::Cell<Option<bool>>,
    /// The fall-prevention walls this tick's `act(3016)` calls asked for
    /// ([`crate::hit_filter::fall_assist_walls`]); the script asks every frame.
    pub fall_assist: std::cell::Cell<u8>,
    pub wire_candidate: bool,
    pub wire_action: std::cell::Cell<bool>,
    pub wire_precompute: std::cell::Cell<Option<(i32, i32)>>,
    pub wire_prefer: std::cell::Cell<bool>,
    pub hang_candidate: bool,
    /// env 3020: a wall within reach of the chest while airborne ([`crate::docking::wall_contact`]).
    pub wall_contact: bool,
    /// env 3045: the air catch type of the airborne hang candidate (0 none, 1 short, 2 long).
    pub air_hang_type: u8,
    pub cover_candidate: bool,
    pub cover_active: bool,
    pub docking_blocked: [bool; 2],
    pub docking_variation: u8,
    pub docking_end: u8,
    pub hang_can_climb: bool,
    pub docking_searches: std::cell::Cell<u8>,
    pub action_guide: std::cell::Cell<Option<(i32, i32)>>,
    pub hp: f32,
    /// Remaining and maximum posture (env 1001, 2010).
    pub posture: f32,
    pub max_posture: f32,
    pub landed: bool,
    /// Right and left hand weapon motion categories (50 is the katana).
    pub weapon_category: [i32; 2],
    /// env 207; 0 is sheathed (non-combat), 1 one-handed.
    pub arm_style: i32,
    /// Actions requested this frame (env 1106), by action-arm id.
    pub requested: HashSet<i32>,
    /// How long each action button has been held, in milliseconds (env 1108). The unit is
    /// unverified; the scripts only compare against zero and against each other.
    pub held_ms: HashMap<i32, f32>,
    /// Active behaviour reference ids (env 3036). Will come from TAE events and SpEffects.
    pub sp_effect_refs: HashSet<i32>,
    /// TAE 605 values, read by env 3063 in milliseconds for HKS start-time handovers.
    pub havok_variables: Vec<(i32, f32)>,
    /// Unlocked action types (env 3033).
    pub unlocked: HashSet<i32>,
    /// Airborne and moving down (env 200).
    pub falling: bool,
    /// Height of the last fall in centimetres (env 224).
    pub fall_height: f32,
    /// Vertical velocity in centimetres per second (env 3003).
    pub vertical_velocity: f32,
    /// Floor contact predicted for this tick's downward movement (land-ready reactions).
    pub landing_ready: bool,
    /// Frame time in seconds; env 333 reports it in milliseconds (the script divides by 1000).
    pub dt: f32,
    /// Lock-on active (env 1118).
    pub locked_on: bool,
    /// The player moves freely: no lock-on and no auto-aim target. `MoveDirection` (the strafe
    /// class the directional move starts, loops and stops select on) then stays forward.
    pub free_movement: bool,
    /// The script seeded a sideways or backward `MoveDirection` in free movement (a move start
    /// out of an attack's move cancel or a landing reads `MoveAngle` off the old facing). It was
    /// held at forward; the body turns to the stick instead. The body reads and clears it.
    pub free_move_turn: std::cell::Cell<bool>,
    /// env 1105 ("standby state") from TAE cancel windows; `None` uses the clip-end
    /// approximation.
    pub standby: Option<bool>,
    /// env 2000 ("can cancel into movement") from TAE cancel windows; `None` as above.
    pub move_cancel: Option<bool>,
    /// Counts `act(9101)` (reset input acceptance) calls; the input layer clears its buffer.
    pub input_resets: std::cell::Cell<u32>,
    /// `act(3025, degrees)`: turn to face a direction relative to the current facing
    /// (positive to the right, like `TurnAngle`: the script passes `TurnAngle` itself to face
    /// the stick, and `stickAngle - 90` to tilt a right step). The body applies and clears it.
    pub face_turn: std::cell::Cell<Option<f32>>,
    /// `act(2019)`: turn to the lock-on target immediately. The body applies and clears it.
    pub face_lock_target: std::cell::Cell<bool>,
    /// `act(2018)`: release the lock-on angle lock for this step, so a locked-on character turns
    /// toward the stick instead of the target. The script asks on every sprinting frame
    /// (`STYLE_TYPE_SPRINT` in `c0000.hks` `Update`) and as the sprint starts from a step. The
    /// body reads and clears it.
    pub release_lock_angle: std::cell::Cell<bool>,
    /// `act(110)` ("SetStaminaRecoveryDisabled" in the community list) was called during the
    /// latest tick: the engine skips the character's whole posture recovery update while the
    /// flag it sets (bit 21 of the action state's flag word +0x88) is up. Cleared at the start of
    /// every tick (that the engine clears it each frame is inferred). The shipped scripts never
    /// call it. Read through [`PlayerBehavior::stamina_recovery_disabled`].
    pub recovery_disabled: std::cell::Cell<bool>,
    /// The hit being reacted to this step, if any (env 202, 222, 236, 237, 256, 285, ...).
    pub damage: Option<IncomingDamage>,
    /// TAE ChrActionFlag 24, queried by the NPC's damage-break branch through env 349.
    pub damage_motion_disabled: bool,
    /// NPC only: the action the AI requests (env 106), an animation-id-like code.
    pub ai_action: i32,
    /// NPC only: the action offered to an attack's cancel window: env 107 (combo), 120 (attack
    /// cancel) and 118 (step), 0 when the window is closed.
    pub ai_cancel_actions: [i32; 3],
    /// Full animation ids this character has (env 1114, "does the animation exist").
    pub existing_anims: std::rc::Rc<HashSet<i64>>,
    /// Fixed answers for any other `(id, first argument)`; `None` matches any argument.
    pub overrides: Vec<(i32, Option<i32>, f32)>,
}

impl Default for PlayerEnv {
    fn default() -> Self {
        Self {
            in_water: false,
            can_dive: false,
            can_surface: false,
            water_mode: std::cell::Cell::new(None),
            fall_assist: std::cell::Cell::new(0),
            wire_candidate: false,
            wire_action: std::cell::Cell::new(false),
            wire_precompute: std::cell::Cell::new(None),
            wire_prefer: std::cell::Cell::new(false),
            hang_candidate: false,
            wall_contact: false,
            air_hang_type: 0,
            cover_candidate: false,
            cover_active: false,
            docking_blocked: [false; 2],
            docking_variation: 0,
            docking_end: 0,
            hang_can_climb: false,
            docking_searches: std::cell::Cell::new(0),
            action_guide: std::cell::Cell::new(None),
            hp: 100.0,
            posture: 100.0,
            max_posture: 100.0,
            landed: true,
            weapon_category: [50, 70],
            arm_style: 1,
            requested: HashSet::new(),
            held_ms: HashMap::new(),
            sp_effect_refs: HashSet::new(),
            havok_variables: Vec::new(),
            // Main weapon (5) and the basics; progression unlocks are a save-file matter.
            unlocked: [0, 1, 2, 3, 4, 5].into_iter().collect(),
            falling: false,
            fall_height: 0.0,
            vertical_velocity: 0.0,
            landing_ready: false,
            dt: 1.0 / 30.0,
            locked_on: false,
            free_movement: false,
            free_move_turn: std::cell::Cell::new(false),
            standby: None,
            move_cancel: None,
            input_resets: std::cell::Cell::new(0),
            face_turn: std::cell::Cell::new(None),
            face_lock_target: std::cell::Cell::new(false),
            release_lock_angle: std::cell::Cell::new(false),
            recovery_disabled: std::cell::Cell::new(false),
            damage: None,
            damage_motion_disabled: false,
            ai_action: 0,
            ai_cancel_actions: [0; 3],
            existing_anims: std::rc::Rc::default(),
            overrides: Vec::new(),
        }
    }
}

/// What happened in one tick.
#[derive(Debug, Clone, Default)]
pub struct TickReport {
    pub frame: u64,
    pub state_path: Vec<String>,
    pub animation: Option<String>,
    pub anim_time: f32,
    pub events: Vec<String>,
    pub hooks: Vec<String>,
    pub script_errors: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("reading {0}: {1}")]
    Io(PathBuf, std::io::Error),
    #[error(transparent)]
    Format(#[from] sekiro_formats::Error),
    #[error(transparent)]
    Vm(#[from] VmError),
}

/// The engine side of the HKS interface for one character.
struct Adapter<'a> {
    rt: &'a mut BehaviorRuntime,
    env: &'a PlayerEnv,
    events: &'a mut Vec<String>,
    log: Option<&'a mut Vec<CallRecord>>,
    /// The state whose hook is running, if any.
    hook_state: Option<&'a str>,
}

fn num(v: Option<&Value>) -> Option<i32> {
    v.and_then(Value::as_number).map(|n| n as i32)
}

fn truth(b: bool) -> f32 {
    if b { 1.0 } else { 0.0 }
}

impl Adapter<'_> {
    fn answer_env(&self, id: CommandId<'_>, args: &[Value]) -> f32 {
        let CommandId::Number(id) = id else {
            // Named command 3064: the aging check. No aging in this simulation.
            return 0.0;
        };
        let arg = num(args.first());
        if let Some((_, _, v)) = self
            .env
            .overrides
            .iter()
            .find(|(i, a, _)| *i == id && (a.is_none() || *a == arg))
        {
            return *v;
        }
        let env = self.env;
        match id {
            3025 => truth(env.in_water),
            3038 => truth(env.can_dive),
            3039 => truth(env.can_surface),
            3000 => truth(env.wire_candidate),
            3020 => truth(env.wall_contact),
            3011 => env.docking_end as f32,
            3017 => env.docking_variation as f32,
            3029 => 0.0,
            3044 => truth(env.hang_candidate && env.docking_searches.get() & 2 != 0),
            3043 => truth(env.cover_candidate && env.docking_searches.get() & 1 != 0),
            3046 if arg == Some(1) => env.docking_variation as f32,
            3050 => truth(env.hang_can_climb),
            3051 | 3052 => truth(env.docking_blocked[(id - 3051) as usize]),
            3045 => {
                if env.docking_searches.get() & 4 != 0 {
                    env.air_hang_type as f32
                } else {
                    0.0
                }
            }
            3048 | 3049 | 3059 | 3060 => 0.0,
            3061 => -1.0,
            ENV_FALLING => truth(env.falling),
            ENV_LANDED => truth(env.landed),
            ENV_REALLY_LANDED => truth(env.landed || env.landing_ready),
            ENV_FALL_HEIGHT => env.fall_height,
            ENV_VERTICAL_VELOCITY => env.vertical_velocity,
            ENV_DT => env.dt * 1000.0,
            ENV_LOCKED_ON => truth(env.locked_on),
            ENV_ARM_STYLE => env.arm_style as f32,
            ENV_WEAPON_CATEGORY => match arg {
                Some(1) => env.weapon_category[0] as f32,
                Some(0) => env.weapon_category[1] as f32,
                _ => 0.0,
            },
            ENV_ANIM_END => truth(self.rt.anim_ended(arg.unwrap_or(0), self.hook_state)),
            3063 => {
                arg.and_then(|id| env.havok_variables.iter().find(|(key, _)| *key == id))
                    .map(|(_, value)| *value)
                    .unwrap_or(0.0)
                    * 1000.0
            }
            ENV_HP => env.hp,
            ENV_POSTURE => env.posture,
            ENV_MAX_POSTURE => env.max_posture,
            // Approximation until TAE cancel windows are wired: a state counts as standby once
            // its animation has ended or when it loops.
            ENV_STANDBY_STATE if env.standby.is_some() => truth(env.standby == Some(true)),
            ENV_MOVE_CANCEL if env.move_cancel.is_some() => truth(env.move_cancel == Some(true)),
            ENV_STANDBY_STATE | ENV_MOVE_CANCEL => truth(
                self.hook_state
                    .and_then(|s| self.rt.state_clip(s))
                    .or_else(|| self.rt.main_clip())
                    .is_some_and(|c| c.at_end() || c.looping),
            ),
            ENV_ACTION_REQUEST => truth(arg.is_some_and(|a| env.requested.contains(&a))),
            ENV_ACTION_HELD => arg
                .and_then(|a| env.held_ms.get(&a).copied())
                .unwrap_or(0.0),
            ENV_ACTION_UNLOCKED => truth(arg.is_some_and(|a| env.unlocked.contains(&a))),
            ENV_AI_ACTION => env.ai_action as f32,
            ENV_AI_COMBO_ACTION => env.ai_cancel_actions[0] as f32,
            ENV_AI_CANCEL_ACTION => env.ai_cancel_actions[1] as f32,
            ENV_AI_STEP_ACTION => env.ai_cancel_actions[2] as f32,
            ENV_ANIM_EXISTS => truth(arg.is_some_and(|a| env.existing_anims.contains(&(a as i64)))),
            ENV_TOOK_DAMAGE => truth(env.damage.is_some()),
            ENV_DAMAGE_TYPE => env.damage.map_or(0.0, |d| d.damage_type as f32),
            ENV_DAMAGE_LEVEL => env.damage.map_or(0.0, |d| d.level as f32),
            ENV_DAMAGE_MOTION_DISABLED => truth(env.damage_motion_disabled),
            ENV_DAMAGE_DIRECTION => env.damage.map_or(0.0, |d| d.direction as f32),
            ENV_DAMAGE_FRONT_BACK => env.damage.map_or(0.0, |d| d.front_back as f32),
            ENV_GUARD_LEVEL => env.damage.map_or(0.0, |d| d.guard_level as f32),
            ENV_GUARD_MOTION_LEVEL => env.damage.map_or(0.0, |d| d.guard_motion_level as f32),
            ENV_DAMAGE_ELEMENT => env.damage.map_or(0.0, |d| d.element as f32),
            ENV_DAMAGE_BEHAVIOR_ID => truth(
                env.damage
                    .zip(arg)
                    .is_some_and(|(d, a)| d.behavior_id != 0 && d.behavior_id == a),
            ),
            ENV_GUARD_REPELLED => env.damage.map_or(0.0, |d| d.repelled as f32),
            ENV_JUST_GUARD_REPELLED => env.damage.map_or(0.0, |d| d.just_repelled as f32),
            ENV_GUARD_REPEL_DIR | ENV_JUST_GUARD_REPEL_DIR => {
                env.damage.map_or(0.0, |d| d.repel_dir as f32)
            }
            ENV_SP_EFFECT_REF => truth(arg.is_some_and(|a| env.sp_effect_refs.contains(&a))),
            ENV_CAN_RELEASE_CROUCH => 1.0,
            _ => 0.0,
        }
    }

    /// A script write to a behaviour variable. In free movement every move start runs forward
    /// with the body turned to the stick: the directional starts, loops and stops are the
    /// locked-on strafes (#26, #144). The idle start gets that turn before the script runs, but
    /// a move cancel out of an attack recovery (whose TAE still disables turning) or a landing
    /// straight into a move start seeded a strafe from `MoveAngle` instead.
    // TODO(#162): with the engine's env answers ported, check whether the move start can turn
    // before the script runs (so `MoveAngle` is already about 0) and this hold can go.
    fn set_script_variable(&mut self, name: &str, value: f32) {
        let held = self.env.free_movement && name == "MoveDirection" && value != 0.0;
        if held {
            self.env.free_move_turn.set(true);
        }
        self.rt.set_variable(name, if held { 0.0 } else { value });
    }

    fn record(&mut self, function: &str, args: Vec<Value>, result: Vec<Value>) {
        if let Some(log) = self.log.as_deref_mut() {
            log.push(CallRecord {
                function: function.to_owned(),
                args,
                result,
            });
        }
    }
}

fn with_id(id: CommandId<'_>, args: &[Value]) -> Vec<Value> {
    let mut all = vec![match id {
        CommandId::Number(n) => Value::Number(n as f32),
        CommandId::Name(s) => Value::str(s),
    }];
    all.extend_from_slice(args);
    all
}

impl Host for Adapter<'_> {
    fn env(&mut self, id: CommandId<'_>, args: &[Value]) -> Value {
        let v = Value::Number(self.answer_env(id, args));
        self.record("env", with_id(id, args), vec![v.clone()]);
        v
    }

    fn act(&mut self, id: CommandId<'_>, args: &[Value]) -> Value {
        if id == CommandId::Number(3011) {
            self.env.water_mode.set(Some(true));
        }
        if id == CommandId::Number(3037) {
            self.env.water_mode.set(Some(false));
        }
        if id == CommandId::Number(3016)
            && let Some(arg) = num(args.first())
        {
            self.env
                .fall_assist
                .set(self.env.fall_assist.get() | crate::hit_filter::fall_assist_walls(arg));
        }
        if id == CommandId::Number(3018) {
            self.env.wire_action.set(num(args.first()) == Some(1));
        }
        if id == CommandId::Number(3023) {
            self.env
                .wire_precompute
                .set(num(args.first()).zip(num(args.get(1))));
        }
        if id == CommandId::Number(3035) {
            self.env.wire_prefer.set(true);
        }
        if let CommandId::Number(id @ 3026..=3028) = id {
            self.env
                .docking_searches
                .set(self.env.docking_searches.get() | (1 << (id - 3026)));
        }
        if id == CommandId::Number(3030) {
            self.env
                .action_guide
                .set(num(args.first()).zip(num(args.get(1))));
        }
        if id == CommandId::Number(ACT_FACE_DIRECTION)
            && let Some(deg) = args.first().and_then(Value::as_number)
        {
            self.env.face_turn.set(Some(deg));
        }
        if id == CommandId::Number(ACT_FACE_LOCK_TARGET) {
            self.env.face_lock_target.set(true);
        }
        if id == CommandId::Number(ACT_RELEASE_LOCK_ANGLE) {
            self.env.release_lock_angle.set(true);
        }
        if id == CommandId::Number(ACT_SET_STAMINA_RECOVERY_DISABLED) {
            self.env.recovery_disabled.set(true);
        }
        if id == CommandId::Number(ACT_RESET_INPUT) {
            self.env.input_resets.set(self.env.input_resets.get() + 1);
        }
        if id == CommandId::Number(ACT_SET_VARIABLE)
            && let (Some(name), Some(value)) =
                (args.first(), args.get(1).and_then(Value::as_number))
        {
            self.set_script_variable(&name.to_display(), value);
        }
        self.record("act", with_id(id, args), Vec::new());
        Value::Nil
    }

    fn call(&mut self, name: &str, args: &[Value]) -> Vec<Value> {
        let first = args.first().map(Value::to_display).unwrap_or_default();
        let result = match name {
            "hkbFireEvent" => {
                self.events.push(first.clone());
                self.rt.fire_event(&first);
                Vec::new()
            }
            "hkbGetVariable" => vec![Value::Number(self.rt.variable(&first).unwrap_or(0.0))],
            "hkbSetVariable" => {
                if let Some(v) = args.get(1).and_then(Value::as_number) {
                    self.set_script_variable(&first, v);
                }
                Vec::new()
            }
            "hkbIsNodeActive" => vec![Value::Bool(self.rt.is_node_active(&first))],
            _ => Vec::new(),
        };
        self.record(name, args.to_vec(), result.clone());
        result
    }
}

pub struct PlayerBehavior {
    pub vm: Vm,
    pub runtime: BehaviorRuntime,
    pub env: PlayerEnv,
    durations: Box<dyn ClipDurations>,
    frame: u64,
    /// When set, every engine call of the next ticks is appended here.
    pub call_log: Option<Vec<CallRecord>>,
}

impl PlayerBehavior {
    /// Loads the player scripts from `script_dir`, the behaviour graph from `behavior_hkx` and
    /// clip lengths through `durations`, then runs `Initialize` and the initial state hooks.
    pub fn load(
        script_dir: &Path,
        behavior_hkx: &Path,
        durations: Box<dyn ClipDurations>,
    ) -> Result<Self, LoadError> {
        let scripts: Vec<PathBuf> = PLAYER_SCRIPTS.iter().map(|c| script_dir.join(c)).collect();
        Self::load_scripts(
            &scripts,
            behavior_hkx,
            durations,
            AnimOffsets::player_default(),
        )
    }

    /// Loads any character: its script chunks in load order (for NPCs the shared `c9997.hks`
    /// first, then their own), its behaviour graph and clip lengths, then runs `Initialize` (if
    /// the scripts define it) and the initial state hooks.
    pub fn load_scripts(
        scripts: &[PathBuf],
        behavior_hkx: &Path,
        durations: Box<dyn ClipDurations>,
        offsets: AnimOffsets,
    ) -> Result<Self, LoadError> {
        let read = |p: &Path| std::fs::read(p).map_err(|e| LoadError::Io(p.to_owned(), e));
        let graph: Arc<Graph> = crate::shared::load(behavior_hkx.to_owned(), || {
            sekiro_formats::hkb::parse(&std::fs::read(behavior_hkx).ok()?).ok()
        })
        .ok_or_else(|| {
            LoadError::Io(
                behavior_hkx.to_owned(),
                std::io::Error::other("cannot load shared behaviour graph"),
            )
        })?;
        let runtime = BehaviorRuntime::new(graph, offsets);
        let mut me = Self {
            vm: Vm::new(),
            runtime,
            env: PlayerEnv::default(),
            durations,
            frame: 0,
            call_log: None,
        };
        me.vm.seed(1);
        let mut events = Vec::new();
        for path in scripts {
            let data = read(path)?;
            let chunk = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let mut host = Adapter {
                rt: &mut me.runtime,
                env: &me.env,
                events: &mut events,
                log: None,
                hook_state: None,
            };
            me.vm.load(&mut host, &chunk, &data)?;
        }
        let mut errors = Vec::new();
        me.call(&[("Initialize".to_owned(), None)], &mut events, &mut errors);
        let hooks = me.runtime.initial_hooks();
        me.run_hooks(&hooks, &mut events, &mut errors, &mut Vec::new());
        me.settle(&mut events, &mut errors, &mut Vec::new());
        Ok(me)
    }

    /// Loads from the standard extracted cache layout under `cache_root`.
    pub fn load_from_cache(cache_root: &Path) -> Result<Self, LoadError> {
        Self::load(
            &cache_root.join("raw/action/script"),
            &cache_root.join("raw/chr/c0000.behbnd.d/c0000.hkx"),
            Box::new(
                crate::clips::ClipLibrary::new(cache_root.join("anim/c0000"))
                    .with_tae_dir(&cache_root.join("raw/chr/c0000.anibnd.d")),
            ),
        )
    }

    /// Calls each `(function, hook state)` that exists in the scripts.
    fn call(
        &mut self,
        calls: &[(String, Option<String>)],
        events: &mut Vec<String>,
        errors: &mut Vec<String>,
    ) {
        for (name, state) in calls {
            if !self.vm.has_function(name) {
                continue;
            }
            let mut host = Adapter {
                rt: &mut self.runtime,
                env: &self.env,
                events,
                log: self.call_log.as_mut(),
                hook_state: state.as_deref(),
            };
            if let Err(e) = self.vm.call_global(&mut host, name, &[]) {
                errors.push(format!("{name}: {e}"));
            }
        }
    }

    fn run_hooks(
        &mut self,
        hooks: &[HookCall],
        events: &mut Vec<String>,
        errors: &mut Vec<String>,
        ran: &mut Vec<String>,
    ) {
        let calls: Vec<(String, Option<String>)> = hooks
            .iter()
            .map(|h| (h.function_name(), Some(h.state.clone())))
            .filter(|(n, _)| self.vm.has_function(n))
            .collect();
        ran.extend(calls.iter().map(|(n, _)| n.clone()));
        self.call(&calls, events, errors);
    }

    fn settle(
        &mut self,
        events: &mut Vec<String>,
        errors: &mut Vec<String>,
        ran: &mut Vec<String>,
    ) {
        for _ in 0..MAX_EVENT_ROUNDS {
            if !self.runtime.has_pending_events() && !self.runtime.bindings_changed() {
                break;
            }
            let hooks = self.runtime.process_events(self.durations.as_mut());
            self.run_hooks(&hooks, events, errors, ran);
        }
    }

    /// Runs one frame of `dt` seconds.
    pub fn tick(&mut self, dt: f32) -> TickReport {
        let mut events = Vec::new();
        let mut errors = Vec::new();
        let mut ran = Vec::new();
        self.env.recovery_disabled.set(false);
        self.env.water_mode.set(None);
        self.env.fall_assist.set(0);
        self.call(&[("Update".to_owned(), None)], &mut events, &mut errors);
        let updates = self.runtime.update_hooks();
        self.run_hooks(&updates, &mut events, &mut errors, &mut ran);
        self.settle(&mut events, &mut errors, &mut ran);
        // Clips that ended and were not already handled by the script send their end events.
        if self.runtime.queue_clip_end_events() > 0 {
            events.push("(clip end)".to_owned());
            self.settle(&mut events, &mut errors, &mut ran);
        }
        self.runtime.advance(dt);
        // A clip that reached its end in this advance sends its end event now, as Havok does
        // inside the graph update, so the next script update already runs in the following
        // state. Otherwise a held stick sees the ended jump wind-up as "free to move" and the
        // script cancels a running jump into locomotion before the take-off.
        if self.runtime.queue_clip_end_events() > 0 {
            events.push("(clip end)".to_owned());
            self.settle(&mut events, &mut errors, &mut ran);
        }
        let clip: Option<ClipState> = self.runtime.main_clip().cloned();
        let report = TickReport {
            frame: self.frame,
            state_path: self.runtime.main_state_path(),
            animation: clip.as_ref().map(|c| c.animation.clone()),
            anim_time: clip.map(|c| c.time).unwrap_or(0.0),
            events,
            hooks: ran,
            script_errors: errors,
        };
        self.frame += 1;
        report
    }

    /// Whether the script called `act(110)` during the latest tick, which stops the character's
    /// posture recovery for that frame ([`ACT_SET_STAMINA_RECOVERY_DISABLED`]).
    pub fn stamina_recovery_disabled(&self) -> bool {
        self.env.recovery_disabled.get()
    }

    /// The animation playing in the full-body slot.
    pub fn animation(&self) -> Option<&str> {
        self.runtime.main_clip().map(|c| c.animation.as_str())
    }
}

/// What the engine reports about the hit a character is reacting to, for one step.
///
/// Codes follow `docs/HKS-COMBAT.md`: `damage_type` is env 202 (3 guard, 99999 ordinary hit,
/// 1000/1003 guarded-and-deflected for the attacker, 1001 guard break, ...), `level` env 236
/// (`DAMAGE_LEVEL_*`), `direction` env 222 and `front_back` env 3032 (`SELECTOR_DAMAGE_DIR_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct IncomingDamage {
    pub damage_type: i32,
    pub level: i32,
    pub direction: i32,
    pub front_back: i32,
    /// env 237: guard level action (above zero lets the full guard reaction play).
    pub guard_level: i32,
    /// env 258: the defender's guard motion level (NpcParam `guardLevel`), the column of the
    /// NPC script's `guard_damage_table`. At 0 every blocked hit falls through to a hit
    /// reaction.
    pub guard_motion_level: i32,
    /// env 285: special attribute (element), 0 for none.
    pub element: i32,
    /// env 3018 / 3019: how the attacker's own blocked or deflected attack recoils.
    pub repelled: i32,
    pub just_repelled: i32,
    /// env 3056 / 3057: deflect direction (1 left, 2 right).
    pub repel_dir: i32,
    /// env 334: the hit's AtkParam `atkBehaviorId` ("behaviour identification value"; 17
    /// blinding), 0 for none. `env(334, v)` is true when it equals `v`.
    pub behavior_id: i32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fall_queries_expose_centimetres_to_hks() {
        let hkx = crate::cache::root().join("raw/chr/c1010.behbnd.d/c9997.hkx");
        let Ok(data) = std::fs::read(hkx) else {
            return;
        };
        let graph = sekiro_formats::hkb::parse(&data).unwrap();
        let mut runtime = BehaviorRuntime::new(Arc::new(graph), AnimOffsets::player_default());
        let env = PlayerEnv {
            fall_height: 1680.0,
            vertical_velocity: -2100.0,
            ..Default::default()
        };
        let mut events = Vec::new();
        let mut host = Adapter {
            rt: &mut runtime,
            env: &env,
            events: &mut events,
            log: None,
            hook_state: None,
        };
        assert_eq!(
            host.env(CommandId::Number(ENV_FALL_HEIGHT), &[])
                .as_number(),
            Some(1680.0)
        );
        assert_eq!(
            host.env(CommandId::Number(ENV_VERTICAL_VELOCITY), &[])
                .as_number(),
            Some(-2100.0)
        );
    }

    /// `act(110)` raises the recovery-disabled flag; nothing else does. Skips without `cache/`.
    #[test]
    fn act_110_disables_posture_recovery() {
        let hkx = crate::cache::root().join("raw/chr/c1010.behbnd.d/c9997.hkx");
        let Ok(data) = std::fs::read(&hkx) else {
            eprintln!("skipping: cache not extracted");
            return;
        };
        let graph = sekiro_formats::hkb::parse(&data).unwrap();
        let mut runtime = BehaviorRuntime::new(Arc::new(graph), AnimOffsets::player_default());
        let env = PlayerEnv::default();
        let mut events = Vec::new();
        let mut host = Adapter {
            rt: &mut runtime,
            env: &env,
            events: &mut events,
            log: None,
            hook_state: None,
        };
        host.act(CommandId::Number(ACT_FACE_LOCK_TARGET), &[]);
        host.act(CommandId::Number(ACT_SET_VARIABLE + 1), &[]);
        assert!(!env.recovery_disabled.get());
        host.act(CommandId::Number(ACT_SET_STAMINA_RECOVERY_DISABLED), &[]);
        assert!(env.recovery_disabled.get());
    }
}
