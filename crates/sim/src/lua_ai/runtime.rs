//! The goal machinery around the Lua scripts: goal objects, the AI object's methods, and the
//! engine-side ("native") leaf goals.
//!
//! The scripts drive a tree of goals. Each goal has a numeric kind (`GOAL_COMMON_Attack` = 2100,
//! the soldier's battle goal 101000, ...), a lifetime, numbered parameters and a queue of
//! subgoals that run front first. A goal kind is implemented in one of three ways:
//!
//! - a *table goal*: registered by `RegisterTableGoal` (a Lua table with `Activate`, `Update`,
//!   ... called through the script helpers `ActivateTableGoal`, `UpdateTableGoal`, ...);
//! - a *script goal*: named in the `.luainfo` table (2100 is `Attack`), whose callbacks are the
//!   globals `Attack_Activate`, `Attack_Update`, `Attack_Terminate`;
//! - a *native goal*: implemented by the engine (waiting, moving, guarding, attacking). These
//!   are re-implemented here in [`NativeGoal`].
//!
//! See `docs/AI.md` for what is engine behaviour read from the executable and what is our
//! reconstruction.

use std::collections::{HashMap, HashSet, VecDeque};
use std::f32::consts::PI;
use std::rc::Rc;

use super::lua50::{Host, LuaError, LuaResult, Obj, Value, Vm};
use super::world::{AiActor, AiWorld, ThinkParams};

/// A plan's attack waits for the target to be within this many degrees of his facing.
pub const FACE_FIRST_DEG: f32 = 30.0;
/// The most seconds that wait takes before the attack goes ahead anyway.
pub const FACE_FIRST_SECS: f32 = 1.5;

pub const RESULT_FAILED: i32 = -1;
pub const RESULT_CONTINUE: i32 = 0;
pub const RESULT_SUCCESS: i32 = 1;

// Private plan wrappers, outside the game's goal-id space.
const PLAN_APPROACH: i32 = -10001;
const PLAN_DEFLECT: i32 = -10002;
const PLAN_DODGE_BACK: i32 = -10003;

const OBJ_AI: u8 = 0;
const OBJ_GOAL: u8 = 1;

/// `POINT_INITIAL` (`ai_define.lua`): where the character was placed.
const POINT_INITIAL: i32 = 100;

/// `GOAL_COMMON_Stay` (`goal_list.lua`).
const GOAL_STAY: i32 = 2;

/// The engine's leaf goals, by goal id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeGoal {
    PlanApproach,
    PlanDeflect,
    PlanDodgeBack,
    /// 2000 Wait, 2 Stay, 5000 WaitCancelTiming: idle until the lifetime ends.
    Wait,
    /// 2019 MoveToSomewhere and its variants: move until within a distance of a target.
    MoveTo,
    /// 4 BackToHome(life, ...): walk back to the initial position.
    BackToHome,
    /// 2101 Guard: hold the guard action for the lifetime.
    Guard,
    /// 2017 SidewayMove and variants: strafe around the target.
    Sideway,
    /// 2018 KeepDist: move until the distance is inside a range.
    KeepDist,
    /// 2016 LeaveTarget: back away until a distance.
    Leave,
    /// 2200 CommonAttack, 2020 SpinStep, 2113 Parry, 2107 ApproachStep: request an action and
    /// wait for its animation.
    Action,
    /// 5100 ConfirmCautionTarget(life, anim, target, time, waitTarget): the look round at a
    /// caution spot (CSGoalConfirmCautionTarget, FUN_1405c35a0 / FUN_1405c3770, traced by
    /// FwLo's RustSekiro): CommonAttack(anim) at the target when the anim is set, Wait(time),
    /// and on Terminate give the spot up (the sound target is cleared) while still in caution
    /// with no new sound, so the logic goes back to NonBattleAct.
    ConfirmCaution,
    /// Anything else the engine implements: succeeds at once (logged).
    Unsupported,
}

impl NativeGoal {
    pub fn for_id(id: i32) -> Option<NativeGoal> {
        Some(match id {
            PLAN_APPROACH => NativeGoal::PlanApproach,
            PLAN_DEFLECT => NativeGoal::PlanDeflect,
            PLAN_DODGE_BACK => NativeGoal::PlanDodgeBack,
            2000 | 2 | 5000 => NativeGoal::Wait,
            2019 | 2013 | 2025 | 2026 | 2027 | 2023 | 2024 | 2028 => NativeGoal::MoveTo,
            4 => NativeGoal::BackToHome,
            2101 => NativeGoal::Guard,
            2017 | 2030 | 2036 => NativeGoal::Sideway,
            2018 => NativeGoal::KeepDist,
            2016 => NativeGoal::Leave,
            2200 | 2020 | 2113 | 2107 => NativeGoal::Action,
            5100 => NativeGoal::ConfirmCaution,
            2031..=2037 | 2500..=2504 => NativeGoal::Unsupported,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoalImpl {
    Table,
    Script,
    Native(NativeGoal),
    /// No implementation found: succeeds when its subgoals are done.
    Empty,
}

/// A goal instance.
#[derive(Debug, Clone)]
pub struct Goal {
    pub kind: i32,
    pub imp: GoalImpl,
    /// Remaining lifetime in seconds; negative means unlimited.
    pub life: f32,
    pub initial_life: f32,
    pub params: Vec<Value>,
    pub subgoals: VecDeque<u32>,
    pub activated: bool,
    pub numbers: HashMap<i32, f64>,
    pub timers: HashMap<i32, f32>,
    pub parent: Option<u32>,
    /// Native goal state.
    pub phase: u8,
    pub elapsed: f32,
    pub started_anim: bool,
    pub restart_clock: Option<f32>,
    /// AI numbers and timers to set when this goal activates or succeeds
    /// (`TimingSetNumber`, `TimingSetTimer`).
    pub timing_sets: Vec<TimingSet>,
    /// Explicit lifetime result requested by the shipped goal script.
    pub life_end_success: Option<bool>,
    /// Ours, set for goals an outside plan queues: the goal succeeds when its lifetime ends (a
    /// move or leave that times out is not a failure that drops the rest of the plan) and
    /// `KeepDist` keeps holding its range until then instead of succeeding inside it.
    pub timed: bool,
    /// Ours, for `LeaveTarget` run by a plan: face the way he runs instead of the target.
    pub away: bool,
    /// Ours, set for the attack steps of an outside plan: the attack is not requested until
    /// the target is within [`FACE_FIRST_DEG`] of his facing (he turns toward it meanwhile, at
    /// his own turn speed, as a `Wait` goal does), for at most [`FACE_FIRST_SECS`]. The game's
    /// own acts approach or stand facing the target, so a plan that follows a tactic that
    /// leaves him facing away (`retreat12`, a strafe) must turn before it swings.
    pub face_first: bool,
    /// Ours: the index of the plan step this goal belongs to (see [`ActGoal::step`]).
    pub plan_step: Option<usize>,
}

/// When a [`TimingSet`] applies (`AI_TIMING_SET__*` in `ai_define.lua`).
pub const TIMING_ACTIVATE: i32 = 0;
pub const TIMING_UPDATE_SUCCESS: i32 = 1;

/// A deferred `SetNumber` or `SetTimer` on the AI, attached to a goal: `goal:TimingSetNumber(slot,
/// value, timing)`. The value is the one computed when the script attached it (scripts).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TimingSet {
    pub timer: bool,
    pub slot: i32,
    pub value: f64,
    pub timing: i32,
}

impl Goal {
    fn param(&self, i: usize) -> f64 {
        self.params.get(i).and_then(Value::num).unwrap_or(0.0)
    }
    /// A flag parameter as the exe reads it: goal parameters are stored as floats
    /// (AddGoal FUN_1405ce510) and the native goals test a walk flag as `0.0 < param`
    /// (ApproachTarget's param 3 in FUN_1405c7e80, KeepDist's param 4 in FUN_1405c7c40, both
    /// passed to the move request FUN_1405e6640). A Lua `false` and the brain's `0.0` both
    /// read as unset.
    fn param_bool(&self, i: usize) -> bool {
        match self.params.get(i) {
            Some(Value::Num(x)) => *x > 0.0,
            Some(Value::Bool(b)) => *b,
            _ => false,
        }
    }
}

/// One line of the AI log.
#[derive(Debug, Clone, PartialEq)]
pub enum AiEvent {
    Logic(i32),
    GoalStart {
        depth: usize,
        kind: i32,
        name: String,
    },
    GoalEnd {
        kind: i32,
        name: String,
        result: i32,
    },
    Action(i32),
    Interrupt(i32),
    Unknown(String),
    Error(String),
}

/// What the native goals asked for this tick.
#[derive(Debug, Clone, Default)]
pub struct Request {
    pub action: i32,
    pub move_level: f32,
    pub move_yaw: Option<f32>,
    pub face_yaw: Option<f32>,
    /// The action comes from a goal registered with `ENABLE_COMBO_ATK_CANCEL`.
    pub combo: bool,
    /// The action comes from an `EndureAttack` goal, whose `Activate` turns on the endure
    /// cancel (`SetEnableEndureCancel_forGoal`, `attack_endure.lua`): the NPC script reads it as
    /// `env(123)` and takes it in any state that offers AI cancels.
    pub endure: bool,
    /// The shipped AttackImmediateAction wrapper enables HKS env(125).
    pub immediate: bool,
}

/// `GOAL_COMMON_EndureAttack` (`goal_list.lua`).
const GOAL_ENDURE_ATTACK: i32 = 2260;

/// The global the scripts' own `Common_Battle_Activate` is kept under while the act hook
/// (see [`ActHook`]) stands in for it.
pub const SCRIPT_BATTLE_ACTIVATE: &str = "__script_Common_Battle_Activate";

/// How an act subgoal sets an AI number when it activates, as the scripts'
/// `goal:TimingSetNumber(slot, value, AI_TIMING_SET__ACTIVATE)` does.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NumberOnActivate {
    /// `GetNumber(slot) + n`, read when the act is queued (the scripts compute it then).
    Add(i32, f64),
    /// A fixed value.
    Set(i32, f64),
}

/// One subgoal of an act chosen outside the scripts.
#[derive(Debug, Clone, PartialEq)]
pub struct ActGoal {
    /// Approach first with the scripts' `Approach_Act_Flex` until within this many metres
    /// (already less the hit radius, as the acts pass it), before queuing this goal.
    pub approach: Option<f64>,
    /// Succeed when the lifetime ends (see [`Goal::timed`]).
    pub timed: bool,
    /// Face the way he runs (see [`Goal::away`]).
    pub away: bool,
    /// An attack step: turn to face the target first when it is more than
    /// [`FACE_FIRST_DEG`] off his facing (see [`Goal::face_first`]).
    pub face_first: bool,
    /// Index of the plan step this goal (and its approach) belongs to; the hook reports each
    /// first activation in [`ActHook::started`].
    pub step: Option<usize>,
    /// The goal id's global name in `goal_list.lua`, such as `GOAL_COMMON_ComboFinal`.
    pub goal: &'static str,
    /// Lifetime in seconds.
    pub life: f64,
    /// Goal parameters after the lifetime, as numbers (`TARGET_ENE_0` is 0).
    pub params: Vec<f64>,
    pub number: Option<NumberOnActivate>,
}

/// An act chosen outside the scripts, queued on the battle goal in place of the act its own
/// `Common_Battle_Activate` would pick.
#[derive(Debug, Clone, PartialEq)]
pub struct ActChoice {
    pub goals: Vec<ActGoal>,
}

/// Who chose the act at one battle-goal activation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActSlot {
    /// The external [`ActChoice`] was queued.
    External,
    /// The script's own weighted pick (no external choice was waiting).
    Script,
    /// A reaction the script set up itself (subgoals already queued, or a reflex act weighted):
    /// the script's pick runs and any external choice keeps waiting.
    Reflex,
}

/// Lets something outside the scripts choose the battle goal's acts. Installed by
/// [`super::AiBrain::enable_act_hook`]: the global `Common_Battle_Activate` (the shared helper
/// every table battle goal calls once it has weighted its acts) is replaced by a host function
/// that queues [`ActHook::choice`] instead, unless the moment is a reaction.
#[derive(Debug, Clone, Default)]
pub struct ActHook {
    /// Live reach eligibility for brain-owned attack goals only.
    pub reachable: Option<std::collections::HashSet<i32>>,
    /// Relative multipliers for Lua's offered Kengeki callback indices.
    pub counter_weights: Vec<(i32, f64)>,
    /// One local posture-break response, replacing the script's queued follow-up.
    pub break_response: bool,
    /// Retained until its first attack starts, so a delayed Lua replan cannot steal it.
    pub break_followup: Option<ActChoice>,
    /// Stop reserving the follow-up slot after the break's reward window.
    pub break_until: f32,
    /// Act indices of the battle goal that are reactions (punishing a heal, turning to a
    /// target behind); when the script weighs any of them, its own pick stands.
    pub reflex_acts: Vec<i32>,
    /// The act to queue at the next battle-goal activation; taken when used.
    pub choice: Option<ActChoice>,
    /// Every activation since the last drain, with the AI clock.
    pub slots: Vec<(f32, ActSlot)>,
    /// Plan steps whose goals activated since the last drain, with the AI clock (the first
    /// activation of each goal that carries a [`ActGoal::step`]).
    pub started: Vec<(f32, usize)>,
}

/// The AI-state SpEffects (SpEffectParam names): 200000 normal, 200001 non-combat caution,
/// 200002 combat caution, 200004 find/battle. One replaces another (spCategory 100, priority 1).
pub const AI_STATE_EFFECTS: [i32; 4] = [200000, 200001, 200002, 200004];

/// A heard AI sound held as the target slot (FwLo's `stealth::Sound`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SoundTarget {
    /// Where it was made.
    pub pos: [f32; 3],
    /// Its AiSoundParam row and rank (AI_SOUND_RANK__NORMAL 0, IMPORTANT 1).
    pub id: i32,
    pub rank: i32,
    /// Seconds until forgotten.
    pub forget: f32,
}

/// Registered goal and logic information plus all per-character AI state. Implements the
/// scripts' [`Host`].
pub struct AiState {
    previous_event_requests: HashMap<i32, i32>,
    pub think: ThinkParams,
    pub names: HashMap<i32, String>,
    pub table_goals: HashSet<i32>,
    pub combo_cancel: HashSet<i32>,
    pub goals: Vec<Option<Goal>>,
    /// Each goal slot's last update result, kept when the slot is freed and reused: the
    /// exe's 32 pooled goals (FUN_1405ad1d0) keep +0x1f8 across AddGoal (FUN_1405ce510 never
    /// resets it), so `goal:GetLastResult` (FUN_1405fa890) in a fresh goal's Activate reads
    /// the result its slot last stored; -1 before the slot's first update (FUN_1405cd180).
    pub slot_results: Vec<i32>,
    pub top: u32,
    pub replan: bool,
    pub world: AiWorld,
    /// The character assigned to TARGET_EVENT (20) by map command 2004[11].
    pub event_target: Option<AiActor>,
    /// Remaining and initial deathblow marks, supplied by the fight's NpcParam vitality.
    pub ninsatsu: (i32, i32),
    /// Metres taken off `GetDist` / `GetDist_Point` (the engine measures from the AI character's
    /// own radius, FwLo's static notes); 0 keeps the centre distance.
    pub dist_offset: f32,
    pub request: Request,
    /// Enabled and cleared by the shipped immediate-action wrapper's lifecycle callbacks.
    pub immediate_action: bool,
    pub numbers: HashMap<i32, f64>,
    pub string_numbers: HashMap<String, f64>,
    pub event_requests: HashMap<i32, i32>,
    pub event_move_target: Option<[f32; 3]>,
    pub event_flags: HashSet<i32>,
    /// Lua writes awaiting publication to the encounter's other actors.
    pub event_flag_changes: HashMap<i32, bool>,
    pub map_regions: Option<super::regions::MapRegions>,
    pub patrol: Option<super::patrol::Patrol>,
    pub timers: HashMap<i32, f32>,
    /// Seconds since each action id was last requested.
    pub action_age: HashMap<i32, f32>,
    pub time: f32,
    pub rng: u64,
    pub log: Vec<AiEvent>,
    pub unknown: HashSet<String>,
    pub battle: bool,
    /// AI target state (AI_TARGET_STATE: 0 none, 1 caution, 2 find, 3 battle) and its value at
    /// the previous logic run; the engine keeps both (state object +0x158 and +0x15C).
    pub target_state: i32,
    pub prev_target_state: i32,
    /// The sound target slot (the exe's targeting system, FwLo's RustSekiro `src/stealth.rs`):
    /// the last AI sound heard, a CAUTION target while no enemy is seen.
    pub sound: Option<SoundTarget>,
    /// A sound target was taken or changed since the last tick: INTERUPT_ChangeSoundTarget.
    pub sound_new: bool,
    /// The AI-state SpEffect the last alert anim put on me ([`AI_STATE_EFFECTS`]): the anims
    /// add it in their TAE (c1010's 1010 adds 200001, 1040 200004, 401020 200002, 101000 and
    /// 401000 200000) and it lasts (effectEndurance -1) until another replaces it (FwLo's
    /// `ai_sp`). None until an anim sets one.
    pub ai_mark: Option<i32>,
    /// The interrupt being dispatched (INTERUPT_* id), answered by `IsInterupt`.
    pub interrupt: Option<i32>,
    /// Edge detection for the world's interrupt sources.
    last_parry_timing: bool,
    /// SpEffects the scripts observe (`AddObserveSpecialEffectAttribute(target, id)`), with
    /// whether each was active on the last tick.
    pub observed_effects: Vec<(i32, i32, bool)>,
    /// The SpEffect whose activation or end is being dispatched
    /// (`GetSpecialEffectActivateInterruptType`, `GetSpecialEffectInactivateInterruptType`).
    pub effect_interrupt: Option<i32>,
    /// External act choices (the boss brain); `None` runs the scripts unchanged.
    pub act_hook: Option<ActHook>,
    /// Areas the scripts watch (`AddObserveArea(slot, observer, target, dir, angle, dist)`):
    /// slot to (target, direction type, angle, distance).
    pub observe_areas: HashMap<i32, (i32, i32, f32, f32)>,
}

impl AiState {
    pub fn new(think: ThinkParams, names: HashMap<i32, String>, seed: u64) -> Self {
        let mut s = Self {
            think,
            names,
            table_goals: HashSet::new(),
            combo_cancel: HashSet::new(),
            goals: Vec::new(),
            slot_results: Vec::new(),
            top: 0,
            replan: false,
            world: AiWorld::default(),
            ninsatsu: (0, 0),
            dist_offset: 0.0,
            event_target: None,
            previous_event_requests: HashMap::new(),
            request: Request::default(),
            immediate_action: false,
            numbers: HashMap::new(),
            string_numbers: HashMap::new(),
            event_requests: HashMap::new(),
            event_move_target: None,
            event_flags: HashSet::new(),
            event_flag_changes: HashMap::new(),
            map_regions: None,
            patrol: None,
            timers: HashMap::new(),
            action_age: HashMap::new(),
            time: 0.0,
            rng: seed | 1,
            log: Vec::new(),
            unknown: HashSet::new(),
            battle: false,
            target_state: 0,
            prev_target_state: 0,
            sound: None,
            sound_new: false,
            ai_mark: None,
            interrupt: None,
            last_parry_timing: false,
            observed_effects: Vec::new(),
            effect_interrupt: None,
            act_hook: None,
            observe_areas: HashMap::new(),
        };
        s.top = s.new_goal(0, -1.0, Vec::new(), None);
        s
    }

    /// An AI sound reached me from `pos`: it becomes my sound target for
    /// SoundTargetForgetTime. A held sound of a higher rank is kept (the exe listener takes a
    /// rank 0 sound only over none or another rank 0 one; FwLo's `Targeting::hear`). A new id
    /// raises INTERUPT_ChangeSoundTarget (gap: the exe compares the record serial; a refreshed
    /// footstep is one sound here, as in FwLo's port).
    pub fn hear(&mut self, pos: [f32; 3], id: i32, rank: i32) {
        if self.sound.is_some_and(|s| s.rank > rank) {
            return;
        }
        self.sound_new |= self.sound.is_none_or(|s| s.id != id);
        let forget = self
            .think
            .fields
            .get("SoundTargetForgetTime")
            .copied()
            .unwrap_or(0.0) as f32;
        self.sound = Some(SoundTarget {
            pos,
            id,
            rank,
            forget,
        });
    }

    /// Counts the sound target's forget time down.
    pub fn age_sound(&mut self, dt: f32) {
        if let Some(s) = &mut self.sound {
            s.forget -= dt;
            if s.forget <= 0.0 {
                self.sound = None;
            }
        }
    }

    pub fn name(&self, kind: i32) -> String {
        self.names
            .get(&kind)
            .cloned()
            .unwrap_or_else(|| kind.to_string())
    }

    pub fn goal(&self, id: u32) -> &Goal {
        self.goals[id as usize].as_ref().expect("live goal")
    }

    pub fn goal_mut(&mut self, id: u32) -> &mut Goal {
        self.goals[id as usize].as_mut().expect("live goal")
    }

    fn new_goal(&mut self, kind: i32, life: f32, params: Vec<Value>, parent: Option<u32>) -> u32 {
        let imp = if let Some(n) = NativeGoal::for_id(kind) {
            GoalImpl::Native(n)
        } else if self.table_goals.contains(&kind) {
            GoalImpl::Table
        } else if self.names.contains_key(&kind) {
            GoalImpl::Script
        } else {
            GoalImpl::Empty
        };
        let g = Goal {
            kind,
            imp,
            life,
            initial_life: life,
            params,
            subgoals: VecDeque::new(),
            activated: false,
            numbers: HashMap::new(),
            timers: HashMap::new(),
            parent,
            phase: 0,
            elapsed: 0.0,
            started_anim: false,
            restart_clock: None,
            timing_sets: Vec::new(),
            life_end_success: None,
            timed: false,
            away: false,
            face_first: false,
            plan_step: None,
        };
        // The lowest free slot, as the exe's pool hands out goals: FUN_1405ade50 scans its 32
        // slots from 0 for the first whose +0xa0 is -1 (FUN_1405cff90; freeing, FUN_1405cedd0,
        // sets it back to -1). `slot_results` relies on this order.
        if let Some(i) = self.goals.iter().position(Option::is_none) {
            self.goals[i] = Some(g);
            i as u32
        } else {
            self.goals.push(Some(g));
            (self.goals.len() - 1) as u32
        }
    }

    fn drop_goal(&mut self, id: u32) {
        let subs: Vec<u32> = self.goal(id).subgoals.iter().copied().collect();
        for s in subs {
            self.drop_goal(s);
        }
        self.goals[id as usize] = None;
    }

    fn random(&mut self) -> f64 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng >> 11) as f64 / (1u64 << 53) as f64
    }

    fn actor(&self, target: i32) -> Option<AiActor> {
        match target {
            -1 => Some(self.world.me.clone()),
            0 | 21 => self.world.target.clone(),
            20 => self.event_target.clone(),
            _ => None,
        }
    }

    /// Position of a target or point id.
    fn position(&self, target: i32) -> Option<[f32; 3]> {
        match target {
            -1 => Some(self.world.me.position),
            // Unseen, the current target is the caution spot (FwLo's `Snapshot`).
            0 | 21 => self
                .world
                .target
                .as_ref()
                .map(|t| t.position)
                .or(self.sound.map(|s| s.pos)),
            20 => self.event_target.as_ref().map(|t| t.position),
            100 | 111 | 121 | 125 => Some(self.world.home),
            102 => self.event_move_target,
            103 => self.patrol.as_ref().and_then(|p| p.position()),
            _ => None,
        }
    }

    /// How far I am from my move point, which NonBattleAct compares with its walk-home
    /// distance (#217). The exe (FUN_1405afea0 over FUN_1405b04d0, stored at AI +0x9084 and
    /// read by GetMovePointEffectRange) measures in 3D from my position to the segment between
    /// the current route point and the previous one (or my initial position, POINT 100, before
    /// the first step), to the last point once a route has ended, and to my initial position
    /// with no route. Its first case, an active event move area (AI +0x9160), is not modelled.
    fn move_point_effect_range(&self) -> f32 {
        let me = glam::Vec3::from_array(self.world.me.position);
        let home = self.world.home;
        let (a, b) = match &self.patrol {
            Some(p) if p.current >= 0 => {
                let b = (p.previous >= 0 && p.previous != p.current)
                    .then(|| p.point(p.previous))
                    .flatten();
                (p.point(p.current), Some(b.unwrap_or(home)))
            }
            Some(p) if p.previous >= 0 => (p.point(p.previous), None),
            _ => (Some(home), None),
        };
        let a = glam::Vec3::from_array(a.unwrap_or(home));
        let Some(b) = b.map(glam::Vec3::from_array) else {
            return me.distance(a);
        };
        // FUN_1419ef320: the nearest point of the segment a..b, clamped to its ends.
        let ab = b - a;
        let along = ab.dot(me - a);
        let t = if along <= 0.0 {
            0.0
        } else {
            (along / ab.length_squared()).min(1.0)
        };
        me.distance(a + ab * t)
    }

    fn dist(&self, target: i32) -> f32 {
        match self.position(target) {
            Some(p) => {
                let m = self.world.me.position;
                ((p[0] - m[0]).powi(2) + (p[2] - m[2]).powi(2)).sqrt()
            }
            None => 9999.0,
        }
    }

    fn yaw_to(&self, target: i32) -> Option<f32> {
        let p = self.position(target)?;
        let m = self.world.me.position;
        let (dx, dz) = (p[0] - m[0], p[2] - m[2]);
        if dx == 0.0 && dz == 0.0 {
            return None;
        }
        Some((-dx).atan2(-dz))
    }

    /// Signed angle in degrees from my facing to the target, positive to the left.
    fn angle_to(&self, target: i32) -> f32 {
        match self.yaw_to(target) {
            Some(y) => wrap(y - self.world.me.yaw).to_degrees(),
            None => 0.0,
        }
    }

    /// Whether `target` lies inside a cone of `angle` degrees centred on direction `dir`
    /// (AI_DIR_TYPE: 1 front, 2 back, 3 left, 4 right; 0 counts as front).
    /// Whether the target of watched area `slot` is inside it.
    fn inside_observe(&self, slot: i32) -> bool {
        self.observe_areas
            .get(&slot)
            .is_some_and(|&(target, dir, angle, dist)| {
                self.position(target).is_some()
                    && self.inside(target, dir, angle)
                    && self.dist(target) <= dist
            })
    }

    fn inside(&self, target: i32, dir: i32, angle: f32) -> bool {
        let centre = match dir {
            2 | 6 => 180.0,
            3 | 7 => 90.0,
            4 | 8 => -90.0,
            _ => 0.0,
        };
        wrap((self.angle_to(target) - centre).to_radians())
            .to_degrees()
            .abs()
            <= angle * 0.5
    }

    fn has_effect(&self, target: i32, id: i32) -> bool {
        // The engine marks the AI state with SpEffects: 200001 non-combat caution, 200002
        // combat caution, 200004 find/battle (SpEffectParam names).
        if target == -1 && id == 200004 && self.target_state >= 2 {
            return true;
        }
        if target == -1 && AI_STATE_EFFECTS.contains(&id) && self.ai_mark == Some(id) {
            return true;
        }
        self.actor(target)
            .is_some_and(|a| a.sp_effects.contains(&id))
    }

    fn excel(&self, idx: i32) -> f64 {
        const NAMES: [&str; 31] = [
            "",
            "maxBackhomeDist",
            "backhomeDist",
            "backhomeBattleDist",
            "nonBattleActLife",
            "BattleStartDist",
            "bMoveOnHearSound",
            "idAttackCannotMove",
            "battleGoalID",
            "BackHome_LookTargetTime",
            "BackHome_LookTargetDist",
            "BackHomeLife_OnHitEneWal",
            "callHelp_IsCall",
            "callHelp_IsReply",
            "callHelp_MyPeerId",
            "callHelp_CallPeerId",
            "callHelp_DelayTime",
            "callHelp_CallActionId",
            "callHelp_ReplyBehaviorType",
            "callHelp_ForgetTimeByArrival",
            "callHelp_MinWaitTime",
            "callHelp_MaxWaitTime",
            "callHelp_ReplyActionId",
            "thinkAttr_doAdmirer",
            "goalAction_ToDisappear",
            "goalAction_ToCaution",
            "goalAction_ToCautionImportant",
            "goalAction_ToFind",
            "changeStateAction_ToNormal",
            "goalAction_ToCautionIndicationTarget",
            "goalAction_ToCautionCorpseTarget",
        ];
        let name = NAMES.get(idx as usize).copied().unwrap_or("");
        self.think.fields.get(name).copied().unwrap_or(0.0)
    }

    fn log(&mut self, e: AiEvent) {
        self.log.push(e);
    }
}

fn wrap(a: f32) -> f32 {
    let mut a = a % (2.0 * PI);
    if a > PI {
        a -= 2.0 * PI;
    } else if a < -PI {
        a += 2.0 * PI;
    }
    a
}

fn num(v: f64) -> Vec<Value> {
    vec![Value::Num(v)]
}

fn boolean(b: bool) -> Vec<Value> {
    vec![Value::Bool(b)]
}

fn goal_obj(id: u32) -> Value {
    Value::Obj(Obj { kind: OBJ_GOAL, id })
}

pub fn ai_obj() -> Value {
    Value::Obj(Obj {
        kind: OBJ_AI,
        id: 0,
    })
}

impl Host for AiState {
    fn call_host(&mut self, _vm: &mut Vm, name: &str, args: &[Value]) -> LuaResult<Vec<Value>> {
        let n = |i: usize| args.get(i).and_then(Value::num).unwrap_or(0.0);
        match name {
            "REGISTER_GOAL" => {
                self.table_goals.insert(n(0) as i32);
                if let Some(s) = args.get(1).and_then(Value::as_str) {
                    self.names
                        .entry(n(0) as i32)
                        .or_insert_with(|| s.to_owned());
                }
            }
            "ENABLE_COMBO_ATK_CANCEL" => {
                self.combo_cancel.insert(n(0) as i32);
            }
            "Common_Battle_Activate" => return self.battle_activate(_vm, args),
            "Common_Kengeki_Activate" => return self.kengeki_activate(_vm, args),
            "REGISTER_LOGIC_FUNC"
            | "REGISTER_GOAL_NO_SUB_GOAL"
            | "REGISTER_GOAL_NO_UPDATE"
            | "REGISTER_GOAL_NO_INTERUPT"
            | "REGISTER_GOAL_UPDATE_TIME"
            | "REGISTER_GOAL_USE_AVOID_CHR"
            | "REGISTER_DBG_GOAL_PARAM"
            | "print" => {}
            "loadstring" => {
                // Only `GOAL_COMMON_If` builds code at run time, always the same shape:
                // "return function (arg) OnIf_<id>(arg.ai, arg.goal, arg.codeNo) end".
                let src = args.first().and_then(Value::as_str).unwrap_or("");
                let Some(start) = src.find("OnIf_") else {
                    return Ok(vec![Value::Nil, Value::str("loadstring unsupported")]);
                };
                let end = src[start..].find('(').map_or(src.len(), |e| start + e);
                return Ok(vec![Value::Host(Rc::from(format!(
                    "ifchunk:{}",
                    &src[start..end]
                )))]);
            }
            _ if name.starts_with("ifchunk:") => {
                return Ok(vec![Value::Host(Rc::from(format!(
                    "ifcall:{}",
                    &name[8..]
                )))]);
            }
            _ if name.starts_with("ifcall:") => {
                let Some(Value::Table(t)) = args.first() else {
                    return Ok(vec![]);
                };
                let (ai, goal, code) = {
                    let t = t.borrow();
                    (t.get_str("ai"), t.get_str("goal"), t.get_str("codeNo"))
                };
                let f = _vm.get_global(&name[7..]);
                if !f.is_nil() {
                    return _vm.call(self, &f, &[ai, goal, code]);
                }
            }
            _ => {
                if self.unknown.insert(name.to_owned()) {
                    self.log(AiEvent::Unknown(name.to_owned()));
                }
            }
        }
        Ok(vec![])
    }

    fn call_method(
        &mut self,
        _vm: &mut Vm,
        obj: Obj,
        name: &str,
        args: &[Value],
    ) -> LuaResult<Vec<Value>> {
        // args[0] is the object itself (method-call syntax).
        let a = &args[1.min(args.len())..];
        if obj.kind == OBJ_GOAL {
            return self.goal_method(obj.id, name, a);
        }
        self.ai_method(name, a)
    }
}

impl AiState {
    fn kengeki_activate(&mut self, vm: &mut Vm, args: &[Value]) -> LuaResult<Vec<Value>> {
        if let (Some(hook), Some(Value::Table(weights))) = (&self.act_hook, args.get(2)) {
            let mut table = weights.borrow_mut();
            let offered: Vec<_> = hook
                .counter_weights
                .iter()
                .filter_map(|&(id, factor)| {
                    let weight = table.get(&Value::Num(f64::from(id))).num().unwrap_or(0.0);
                    (weight > 0.0).then_some((id, weight, factor))
                })
                .collect();
            // Keep the script's choice when every offered callback was vetoed.
            // This includes a sole mandatory counter, and adds no fallback candidate.
            let mut weights: Vec<_> = (1..=50)
                .map(|id| {
                    let weight = table.get(&Value::Num(f64::from(id))).num().unwrap_or(0.0);
                    let factor = offered
                        .iter()
                        .find(|(i, _, _)| *i == id)
                        .map_or(1.0, |(_, _, f)| *f);
                    (id, weight, factor)
                })
                .collect();
            if offered.iter().any(|(_, _, factor)| *factor != 1.0)
                && weights
                    .iter()
                    .any(|(_, weight, factor)| weight * factor > 0.0)
            {
                for (id, weight, factor) in weights.drain(..) {
                    table.set(
                        Value::Num(f64::from(id)),
                        Value::Num((weight * factor * 100.0).round()),
                    );
                }
            }
        }
        let script = vm.get_global("__brain_kengeki_activate");
        vm.call(self, &script, args)
    }

    /// Stands in for the scripts' `Common_Battle_Activate(ai, goal, weights, acts, after,
    /// params)` while an [`ActHook`] is installed: queues the external act on the battle goal,
    /// or calls the script's own function.
    fn battle_activate(&mut self, vm: &mut Vm, args: &[Value]) -> LuaResult<Vec<Value>> {
        if self
            .act_hook
            .as_ref()
            .is_some_and(|h| h.break_followup.is_some())
        {
            return Ok(vec![]);
        }
        let goal = match args.get(1) {
            Some(Value::Obj(o))
                if o.kind == OBJ_GOAL
                    && self.goals.get(o.id as usize).is_some_and(Option::is_some) =>
            {
                Some(o.id)
            }
            _ => None,
        };
        let time = self.time;
        let slot = match (&self.act_hook, goal) {
            (Some(hook), Some(g)) => {
                // A reaction the script already queued (Common_ActivateAct: a heal, a step
                // away), or a reflex act among the weighted ones.
                let queued = !self.goal(g).subgoals.is_empty();
                let reflex_weighted = match args.get(2) {
                    Some(Value::Table(t)) => {
                        let t = t.borrow();
                        hook.reflex_acts
                            .iter()
                            .any(|&a| t.get(&Value::Num(a as f64)).num().unwrap_or(0.0) > 0.0)
                    }
                    _ => false,
                };
                if queued || reflex_weighted {
                    ActSlot::Reflex
                } else if hook.choice.is_some() {
                    ActSlot::External
                } else {
                    ActSlot::Script
                }
            }
            _ => ActSlot::Script,
        };
        if let Some(hook) = self.act_hook.as_mut() {
            hook.slots.push((time, slot));
        }
        if slot == ActSlot::External
            && let (Some(g), Some(choice)) =
                (goal, self.act_hook.as_mut().and_then(|h| h.choice.take()))
        {
            self.queue_act(vm, g, &choice)?;
            return Ok(vec![]);
        }
        let script = vm.get_global(SCRIPT_BATTLE_ACTIVATE);
        if script.is_nil() {
            return Ok(vec![]);
        }
        vm.call(self, &script, args)
    }

    /// Queues an external act on battle goal `g` the way the scripts' acts do: an approach,
    /// then the attack goals with their AI-number settings.
    fn queue_act(&mut self, vm: &mut Vm, g: u32, choice: &ActChoice) -> LuaResult<()> {
        let n = Value::Num;
        for step in &choice.goals {
            if let Some(d) = step.approach {
                // Evaluate the script's range check when this step runs, after earlier
                // tactics, rather than against the world at whole-plan queue time.
                if let Some(Value::Obj(sub)) = self
                    .add_goal(g, false, &[n(f64::from(PLAN_APPROACH)), n(6.0), n(d)])
                    .into_iter()
                    .next()
                {
                    self.goal_mut(sub.id).plan_step = step.step;
                }
            }
            let kind = match step.goal {
                "PLAN_Deflect" => Some(f64::from(PLAN_DEFLECT)),
                "PLAN_DodgeBack" => Some(f64::from(PLAN_DODGE_BACK)),
                _ => vm.get_global(step.goal).num(),
            };
            let Some(kind) = kind else {
                self.log(AiEvent::Error(format!(
                    "act goal {} is not defined",
                    step.goal
                )));
                continue;
            };
            let mut a = vec![n(kind), n(step.life)];
            a.extend(step.params.iter().copied().map(n));
            let Some(Value::Obj(sub)) = self.add_goal(g, false, &a).into_iter().next() else {
                continue;
            };
            {
                let goal = self.goal_mut(sub.id);
                goal.timed = step.timed;
                goal.away = step.away;
                goal.face_first = step.face_first;
                goal.plan_step = step.step;
            }
            if let Some(set) = step.number {
                let (slot, value) = match set {
                    NumberOnActivate::Add(slot, v) => {
                        (slot, self.numbers.get(&slot).copied().unwrap_or(0.0) + v)
                    }
                    NumberOnActivate::Set(slot, v) => (slot, v),
                };
                self.goal_mut(sub.id).timing_sets.push(TimingSet {
                    timer: false,
                    slot,
                    value,
                    timing: TIMING_ACTIVATE,
                });
            }
        }
        Ok(())
    }

    fn add_goal(&mut self, parent: u32, front: bool, a: &[Value]) -> Vec<Value> {
        let n = |i: usize| a.get(i).and_then(Value::num).unwrap_or(0.0);
        let kind = n(0) as i32;
        let life = n(1) as f32;
        let params: Vec<Value> = a.iter().skip(2).cloned().collect();
        let id = self.new_goal(kind, life, params, Some(parent));
        let p = self.goal_mut(parent);
        if front {
            p.subgoals.push_front(id);
        } else {
            p.subgoals.push_back(id);
        }
        vec![goal_obj(id)]
    }

    fn goal_method(&mut self, id: u32, name: &str, a: &[Value]) -> LuaResult<Vec<Value>> {
        let n = |i: usize| a.get(i).and_then(Value::num).unwrap_or(0.0);
        if self.goals.get(id as usize).is_none_or(Option::is_none) {
            return Ok(vec![]);
        }
        Ok(match name {
            "AddSubGoal" => self.add_goal(id, false, a),
            "AddSubGoal_Front" => self.add_goal(id, true, a),
            "ClearSubGoal" => {
                let subs: Vec<u32> = self.goal_mut(id).subgoals.drain(..).collect();
                for s in subs {
                    self.drop_goal(s);
                }
                vec![]
            }
            // Unset parameters read as 0, like the engine's zero-filled parameter array. The exe
            // stores goal parameters as floats (AddGoal FUN_1405ce510, GetParam FUN_1405cfb00),
            // so a Lua boolean comes back as a number: NonBattleAct's walk test
            // `GetParam(2) == 0` holds for the `false` _COMMON_AddNonBattleGoal passes.
            "GetParam" => vec![match self.goal(id).params.get(n(0) as usize) {
                None | Some(Value::Nil) => Value::Num(0.0),
                Some(Value::Bool(b)) => Value::Num(if *b { 1.0 } else { 0.0 }),
                Some(v) => v.clone(),
            }],
            "GetLastResult" => num(f64::from(
                self.slot_results.get(id as usize).copied().unwrap_or(-1),
            )),
            "GetLife" => num(self.goal(id).life as f64),
            "GetSubGoalNum" => num(self.goal(id).subgoals.len() as f64),
            "GetNumber" => num(*self.goal(id).numbers.get(&(n(0) as i32)).unwrap_or(&0.0)),
            "SetNumber" => {
                self.goal_mut(id).numbers.insert(n(0) as i32, n(1));
                vec![]
            }
            "SetTimer" => {
                self.goal_mut(id).timers.insert(n(0) as i32, n(1) as f32);
                vec![]
            }
            "GetTimer" => num(*self.goal(id).timers.get(&(n(0) as i32)).unwrap_or(&0.0) as f64),
            "IsFinishTimer" => {
                boolean(*self.goal(id).timers.get(&(n(0) as i32)).unwrap_or(&0.0) <= 0.0)
            }
            "GetBattleGoalId" => num(self.think.battle_goal_id as f64),
            "GetId" => num(self.goal(id).kind as f64),
            "IsInterruptSubGoalChanged" => boolean(false),
            "TimingSetNumber" | "TimingSetTimer" => {
                let set = TimingSet {
                    timer: name == "TimingSetTimer",
                    slot: n(0) as i32,
                    value: n(1),
                    timing: n(2) as i32,
                };
                self.goal_mut(id).timing_sets.push(set);
                vec![goal_obj(id)]
            }
            "SetLifeEndSuccess" => {
                self.goal_mut(id).life_end_success = Some(a.first().is_some_and(Value::truthy));
                vec![goal_obj(id)]
            }
            "SetTargetRange" | "SetFailedEndOption" | "SetNoUpdate" | "SetManagementGoal"
            | "SetTableGoal" | "SetNormalGoal" => vec![goal_obj(id)],
            _ => self.default_method(&format!("goal:{name}")),
        })
    }

    fn default_method(&mut self, name: &str) -> Vec<Value> {
        if self.unknown.insert(name.to_owned()) {
            self.log(AiEvent::Unknown(name.to_owned()));
        }
        let m = name.rsplit(':').next().unwrap_or(name);
        if m.starts_with("Is") || m.starts_with("Has") || m.starts_with("Check") {
            boolean(false)
        } else if m.starts_with("Get") {
            num(0.0)
        } else {
            vec![]
        }
    }

    fn ai_method(&mut self, name: &str, a: &[Value]) -> LuaResult<Vec<Value>> {
        let n = |i: usize| a.get(i).and_then(Value::num).unwrap_or(0.0);
        let t = |i: usize| a.get(i).and_then(Value::num).unwrap_or(0.0) as i32;
        Ok(match name {
            "AddTopGoal" => {
                let top = self.top;
                self.add_goal(top, false, a)
            }
            "GetTopGoal" => vec![goal_obj(self.top)],
            "HasGoal" => {
                let k = t(0);
                boolean(self.goals.iter().flatten().any(|g| g.kind == k))
            }
            "Replanning" => {
                self.replan = true;
                vec![]
            }
            "GetDistXZ" => num(self.dist(t(0)) as f64),
            "GetDist" | "GetDist_Point" => {
                num((self.dist(t(0)) - self.dist_offset).max(0.0) as f64)
            }
            "IsInsideTargetRegion" => boolean(
                self.position(t(0))
                    .and_then(|position| self.map_regions.as_ref()?.contains(t(1), position))
                    .unwrap_or(false),
            ),
            "GetDistYSigned" | "GetDistY" => {
                let height = self
                    .position(t(0))
                    .map_or(0.0, |p| p[1] - self.world.me.position[1]);
                num(if name == "GetDistY" {
                    height.abs()
                } else {
                    height
                } as f64)
            }
            "GetRandam_Int" => {
                let (lo, hi) = (n(0).floor(), n(1).floor());
                let r = self.random();
                num((lo + (r * (hi - lo + 1.0)).floor()).min(hi))
            }
            "GetRandam_Float" => {
                let r = self.random();
                num(n(0) + r * (n(1) - n(0)))
            }
            // Reviewed boss duels retain Wolf as their sole enemy target.
            "GetHpLastTarget" => {
                num(self.world.target.as_ref().map_or(0.0, |target| target.hp) as f64)
            }
            "GetHpRate" => num(self.actor(t(0)).map_or(1.0, |a| {
                if a.max_hp > 0.0 {
                    (a.hp / a.max_hp) as f64
                } else {
                    1.0
                }
            })),
            "GetHp" => num(self.actor(t(0)).map_or(0.0, |a| a.hp as f64)),
            "GetSp" => num(self.actor(t(0)).map_or(0.0, |a| a.posture as f64)),
            "GetSpRate" => num(self.actor(t(0)).map_or(1.0, |a| {
                if a.max_posture > 0.0 {
                    (a.posture / a.max_posture) as f64
                } else {
                    1.0
                }
            })),
            "HasSpecialEffectId" => boolean(self.has_effect(t(0), t(1))),
            "IsTargetGuard" => boolean(self.actor(t(0)).is_some_and(|a| a.guarding)),
            "IsInsideTarget" => boolean(self.inside(t(0), t(1), n(2) as f32)),
            "IsInsideTargetEx" => {
                boolean(self.inside(t(0), t(2), n(3) as f32) && self.dist(t(0)) <= n(4) as f32)
            }
            "IsLookToTarget" => {
                let ang = if a.len() > 1 { n(1) as f32 } else { 30.0 };
                boolean(self.inside(t(0), 1, ang))
            }
            "GetToTargetAngle" => num(self.angle_to(t(0)) as f64),
            "GetMapHitRadius" => num(self.world.hit_radius as f64),
            "GetExcelParam" => num(self.excel(t(0))),
            "GetNpcThinkParamID" => num(self.think.id as f64),
            "GetNinsatsuNum" => num(self.ninsatsu.0 as f64),
            "GetNinsatsuMaxNum" => num(self.ninsatsu.1 as f64),
            "GetNumber" => num(*self.numbers.get(&t(0)).unwrap_or(&0.0)),
            "SetNumber" => {
                self.numbers.insert(t(0), n(1));
                vec![]
            }
            "GetStringIndexedNumber" => {
                let k = a.first().and_then(Value::as_str).unwrap_or("").to_owned();
                num(*self.string_numbers.get(&k).unwrap_or(&0.0))
            }
            "SetStringIndexedNumber" => {
                let k = a.first().and_then(Value::as_str).unwrap_or("").to_owned();
                self.string_numbers.insert(k, n(1));
                vec![]
            }
            "SetTimer" | "StartIdTimer" | "TimingSetTimer" => {
                self.timers.insert(t(0), n(1) as f32);
                vec![]
            }
            "GetTimer" | "GetIdTimer" => num(*self.timers.get(&t(0)).unwrap_or(&0.0) as f64),
            "IsFinishTimer" => boolean(*self.timers.get(&t(0)).unwrap_or(&0.0) <= 0.0),
            "GetAttackPassedTime" => num(*self.action_age.get(&t(0)).unwrap_or(&9999.0) as f64),
            "StartAttackPassedTimer" => {
                self.action_age.insert(t(0), 0.0);
                vec![]
            }
            "IsBattleState" => boolean(self.target_state == 3),
            "IsFindState" => boolean(self.target_state == 2),
            "IsCautionState" => boolean(self.target_state == 1),
            "IsChangeState" => boolean(self.target_state != self.prev_target_state),
            "GetPrevTargetState" => num(self.prev_target_state as f64),
            "GetMovePointNumber" => num(self.patrol.as_ref().map_or(-1, |p| p.current) as f64),
            "GetPrevMovePointNumber" => num(self.patrol.as_ref().map_or(-1, |p| p.previous) as f64),
            "GetMovePointType" => num(self.patrol.as_ref().map_or(0, |p| p.kind) as f64),
            "GetMovePointAnimId" => num(self
                .patrol
                .as_ref()
                .and_then(|p| p.arrival(t(0)))
                .map_or(-1, |a| a.0) as f64),
            "GetMovePointWaitTime" => num(self
                .patrol
                .as_ref()
                .and_then(|p| p.arrival(t(0)))
                .map_or(0, |a| a.1) as f64),
            "GetMovePointEffectRange" => num(self.move_point_effect_range() as f64),
            "IsRouteEnd" => boolean(self.patrol.as_ref().is_none_or(|p| p.ended())),
            // The MSB enemy's small act (its init_anim_id_type, part data +0x22) names a row
            // whose anim id GetSmallActAnimId returns; with no row (-1, as for nearly every
            // map enemy) it returns -1 (FUN_140604450 over FUN_14060c1b0) and the waits 0
            // (FUN_1406044d0), so NonBattleAct stays at home instead of queueing a
            // WaitWithAnime of anim 0. The rows' table is not identified: -1 for all
            // (docs/FIDELITY.md), as FwLo's `src/ai.rs` returns.
            "GetSmallActAnimId" => num(-1.0),
            "GetSmallActPreWaitTime" | "GetSmallActPostWaitTime" => num(0.0),
            "StepNextMovePoint" => {
                if let Some(p) = &mut self.patrol {
                    p.step(self.rng);
                }
                vec![]
            }
            "IsVisibleTarget" | "IsVisibleCurrTarget" => boolean(self.world.target.is_some()),
            "IsSearchTarget" => boolean(self.world.target.is_some() || self.sound.is_some()),
            // AI_TARGET_TYPE__NORMAL_ENEMY 3, __SOUND 4 (ai_define.lua).
            "GetCurrTargetType" => num(if self.world.target.is_some() {
                3.0
            } else if self.sound.is_some() {
                4.0
            } else {
                0.0
            }),
            "GetLatestSoundTargetID" => num(self.sound.map_or(0, |s| s.id) as f64),
            "GetLatestSoundTargetRank" => num(self.sound.map_or(0, |s| s.rank) as f64),
            // The sound record's serial, -1 without one (FwLo: FUN_1405e3680); ours is the id.
            "GetLatestSoundTargetInstanceID" => num(self.sound.map_or(-1, |s| s.id) as f64),
            "IsInterupt" => boolean(self.interrupt == Some(t(0))),
            "IsLadderAct"
            | "IsForceBattleGoal"
            | "TeamHelp_IsValidReply"
            | "IsTouchBreakableObject"
            | "IsLockOnTarget" => boolean(false),
            "CheckDoesExistPath"
            | "CheckDoesExistPathWithSetPoint"
            | "IsExistMeshOnLine"
            | "IsFinishAttackCoolTime" => boolean(true),
            "GetEventRequest" => num(self.event_requests.get(&t(0)).copied().unwrap_or(-1) as f64),
            "IsEventFlag" => boolean(self.event_flags.contains(&t(0))),
            "SetEventFlag" => {
                let on = a.get(1).is_some_and(|v| match v {
                    Value::Num(n) => *n != 0.0,
                    _ => v.truthy(),
                });
                if on {
                    self.event_flags.insert(t(0));
                } else {
                    self.event_flags.remove(&t(0));
                }
                self.event_flag_changes.insert(t(0), on);
                vec![]
            }
            "SetEventMoveTarget" => {
                self.event_move_target = self
                    .map_regions
                    .as_ref()
                    .and_then(|regions| regions.point(t(0)));
                vec![]
            }
            "GetTeamOrder" | "GetChangeBattleStateCount" | "GetOddsParamIdOffset" => num(0.0),
            "TurnTo" => {
                self.request.face_yaw = self.yaw_to(t(0));
                vec![]
            }
            "SetEnableImmediateAction_forGoal" => {
                self.immediate_action = true;
                vec![]
            }
            "ClearEnableImmediateAction_forGoal" => {
                self.immediate_action = false;
                vec![]
            }
            "DoEzAction" => {
                if n(1) > 0.0 {
                    self.request.action = t(1);
                }
                vec![]
            }
            "AddObserveSpecialEffectAttribute" => {
                let (target, id) = (t(0), t(1));
                if !self
                    .observed_effects
                    .iter()
                    .any(|&(o, e, _)| o == target && e == id)
                {
                    // Already active when first observed: no activation edge (ours).
                    let active = self.has_effect(target, id);
                    self.observed_effects.push((target, id, active));
                }
                vec![]
            }
            "GetSpecialEffectActivateInterruptType" | "GetSpecialEffectInactivateInterruptType" => {
                num(self.effect_interrupt.unwrap_or(0) as f64)
            }
            "AddObserveArea" => {
                // The observer is always the AI itself in the scripts (TARGET_SELF).
                self.observe_areas
                    .insert(t(0), (t(2), t(3), n(4) as f32, n(5) as f32));
                vec![]
            }
            "IsInsideObserve" => boolean(self.inside_observe(t(0))),
            "DeleteObserve" => {
                self.observe_areas.remove(&t(0));
                vec![]
            }
            "AddObserveRegion"
            | "AddObserveChrDmyArea"
            | "PrintText"
            | "ReqPlatoonState"
            | "ClearForceBattleGoal"
            | "RequestEmergencyQuickTurn"
            | "DbgSetLastActIdx"
            | "DbgSetLastKengekiActIdx"
            | "SetAIPredictionMoveTargetSpecifyTargetDir"
            | "ClearEnemyTarget"
            | "RemoveTriggerRegionObserver"
            | "SetTableLogic"
            | "SetNormalLogic"
            | "ClearIndicationPosTarget"
            | "ClearLastMemoryTargetPos"
            | "SetEnableEndureCancel_forGoal"
            | "ClearEnableEndureCancel_forGoal" => vec![],
            "ClearSoundTarget" => {
                self.sound = None;
                vec![]
            }
            "DbgGetForceActIdx" | "DbgGetForceKengekiActIdx" => num(0.0),
            // Free floor in every direction: the whole probed distance is walkable. The engine
            // probes its navmesh here; our arenas are open (see docs/AI.md).
            "GetExistMeshOnLineDistSpecifyAngleEx" | "GetExistMeshOnLineDistSpecifyAngle" => {
                num(n(2))
            }
            _ => self.default_method(&format!("ai:{name}")),
        })
    }
}

/// Runs one tick of the goal tree.
pub fn tick(vm: &mut Vm, st: &mut AiState, world: AiWorld) {
    st.world = world;
    if let Some(&id) = st
        .world
        .me
        .sp_effects
        .iter()
        .rev()
        .find(|id| AI_STATE_EFFECTS.contains(id))
    {
        st.ai_mark = Some(id);
    }
    let dt = st.world.dt;
    st.time += dt;
    st.request = Request::default();
    for v in st.timers.values_mut() {
        *v -= dt;
    }
    for v in st.action_age.values_mut() {
        *v += dt;
    }
    if st.world.target.is_some() {
        st.battle = true;
    }
    let mut fired = Vec::new();
    // Table logic such as c5310 handles map commands in Interrupt, while Main only waits.
    if st.previous_event_requests != st.event_requests {
        st.previous_event_requests.clone_from(&st.event_requests);
        if let Some(kind) = vm.get_global("INTERUPT_EventRequest").num() {
            fired.push(kind as i32);
        }
    }
    if st.world.parry_timing && !st.last_parry_timing {
        // INTERUPT_FindAttack and INTERUPT_ParryTiming.
        fired.extend([1, 24]);
    }
    st.last_parry_timing = st.world.parry_timing;
    if st.world.damaged {
        fired.push(2);
    }
    if st.sound_new
        && let Some(kind) = vm.get_global("INTERUPT_ChangeSoundTarget").num()
    {
        fired.push(kind as i32);
    }
    for kind in fired {
        dispatch_interrupt(vm, st, kind);
    }
    // Observed SpEffects: INTERUPT_ActivateSpecialEffect (43) on the step one turns on,
    // INTERUPT_InactivateSpecialEffect (44) on the step it turns off.
    let mut edges = Vec::new();
    for i in 0..st.observed_effects.len() {
        let (target, id, was) = st.observed_effects[i];
        let now = st.has_effect(target, id);
        if now != was {
            edges.push((if now { 43 } else { 44 }, id));
        }
        st.observed_effects[i].2 = now;
    }
    for (kind, id) in edges {
        st.effect_interrupt = Some(id);
        dispatch_interrupt(vm, st, kind);
    }
    st.effect_interrupt = None;
    // Watched areas: INTERUPT_Inside_ObserveArea (16) on every step a target is inside one
    // (ours: the Owl's heat-up checks a timer in the handler, so a target already inside when
    // the timer runs out must still raise it).
    let mut slots: Vec<i32> = st.observe_areas.keys().copied().collect();
    slots.sort_unstable();
    if slots.iter().any(|&s| st.inside_observe(s)) {
        dispatch_interrupt(vm, st, 16);
    }
    let top = st.top;
    if st.replan || st.goal(top).subgoals.is_empty() {
        st.replan = false;
        let subs: Vec<u32> = st.goal_mut(top).subgoals.drain(..).collect();
        for s in subs {
            terminate(vm, st, s);
        }
        // A seen target moves the AI to the find state, and the next logic run to battle.
        if st.world.target.is_some() {
            st.target_state = if st.target_state < 2 { 2 } else { 3 };
        }
        let logic = st.think.logic_id;
        st.log(AiEvent::Logic(logic));
        let r = vm.call_global(st, "ExecTableLogic", &[ai_obj(), Value::Num(logic as f64)]);
        if let Err(e) = r {
            st.log(AiEvent::Error(e.to_string()));
        }
        st.prev_target_state = st.target_state;
    }
    let subs: Vec<u32> = st.goal(top).subgoals.iter().copied().collect();
    if let Some(&front) = subs.first() {
        let r = update(vm, st, front, 1);
        if r != RESULT_CONTINUE {
            st.goal_mut(top).subgoals.retain(|&g| g != front);
            terminate(vm, st, front);
        }
    }
    if st.request.face_yaw.is_none() && st.battle {
        st.request.face_yaw = st.yaw_to(0);
    }
    st.sound_new = false;
}

/// Offers interrupt `kind` to the logic, then to the active goals from the deepest up, until
/// one handles it. Our reconstruction of the dispatch order; see docs/AI.md.
fn dispatch_interrupt(vm: &mut Vm, st: &mut AiState, kind: i32) {
    st.interrupt = Some(kind);
    st.log(AiEvent::Interrupt(kind));
    // A parry reflex normally clears the battle goal, dropping its remaining plan.
    // During readiness, give that same script a local queue so its response finishes
    // this tactic and preserves the following attack. Other interrupts remain normal.
    if kind == 1 || kind == 24 {
        let mut at = st.top;
        while let Some(&next) = st.goal(at).subgoals.front() {
            at = next;
            let k = st.goal(at).kind;
            if k == PLAN_DODGE_BACK || k == PLAN_DEFLECT {
                if k == PLAN_DEFLECT && kind == 24 && st.goal(at).phase == 0 {
                    let battle = st.think.battle_goal_id;
                    let handled = vm
                        .call_global(
                            st,
                            "InterruptTableGoal_Common",
                            &[ai_obj(), goal_obj(at), Value::Num(f64::from(battle))],
                        )
                        .ok()
                        .flatten()
                        .and_then(|v| v.into_iter().next())
                        .is_some_and(|v| v.truthy());
                    if handled && !st.goal(at).subgoals.is_empty() {
                        st.goal_mut(at).phase = 1;
                    }
                }
                st.interrupt = None;
                return;
            }
        }
    }
    let logic = st.think.logic_id as f64;
    let top = st.top;
    let handled = vm
        .call_global(
            st,
            "InterruptTableLogic_Common",
            &[ai_obj(), goal_obj(top), Value::Num(logic)],
        )
        .ok()
        .flatten()
        .and_then(|v| v.into_iter().next())
        .is_some_and(|v| v.truthy());
    if !handled {
        let mut chain = Vec::new();
        let mut id = st.top;
        while let Some(&next) = st.goal(id).subgoals.front() {
            chain.push(next);
            id = next;
        }
        for &g in chain.iter().rev() {
            if st.goals.get(g as usize).is_none_or(Option::is_none) {
                continue;
            }
            let r = if st.goal(g).imp == GoalImpl::Table {
                let k = st.goal(g).kind as f64;
                vm.call_global(
                    st,
                    "InterruptTableGoal_Common",
                    &[ai_obj(), goal_obj(g), Value::Num(k)],
                )
                .ok()
                .flatten()
                .and_then(|v| v.into_iter().next())
            } else {
                call_cb(vm, st, g, "Interrupt")
            };
            if r.is_some_and(|v| v.truthy()) {
                break;
            }
        }
    }
    st.interrupt = None;
}

fn call_cb(vm: &mut Vm, st: &mut AiState, id: u32, which: &str) -> Option<Value> {
    let g = st.goal(id);
    let (kind, imp) = (g.kind, g.imp);
    let r: LuaResult<Option<Vec<Value>>> = match imp {
        GoalImpl::Table => {
            let f = format!("{which}TableGoal");
            vm.call_global(st, &f, &[ai_obj(), goal_obj(id), Value::Num(kind as f64)])
        }
        GoalImpl::Script => {
            let n = st.name(kind);
            let suffix = if which == "Interrupt" {
                "Interupt"
            } else {
                which
            };
            vm.call_global(st, &format!("{n}_{suffix}"), &[ai_obj(), goal_obj(id)])
        }
        _ => Ok(None),
    };
    match r {
        Ok(v) => v.and_then(|v| v.into_iter().next()),
        Err(e) => {
            let msg = match e {
                LuaError::Runtime(m) | LuaError::Chunk(m) => m,
            };
            st.log(AiEvent::Error(format!("{} {which}: {msg}", st.name(kind))));
            None
        }
    }
}

/// Applies goal `id`'s timing sets for `timing` to the AI's numbers and timers.
fn apply_timing_sets(st: &mut AiState, id: u32, timing: i32) {
    let sets: Vec<TimingSet> = st
        .goal(id)
        .timing_sets
        .iter()
        .filter(|t| t.timing == timing)
        .copied()
        .collect();
    for t in sets {
        if t.timer {
            st.timers.insert(t.slot, t.value as f32);
        } else {
            st.numbers.insert(t.slot, t.value);
        }
    }
}

fn terminate(vm: &mut Vm, st: &mut AiState, id: u32) {
    if st.goals.get(id as usize).is_none_or(Option::is_none) {
        return;
    }
    let subs: Vec<u32> = st.goal_mut(id).subgoals.drain(..).collect();
    for s in subs {
        terminate(vm, st, s);
    }
    if st.goal(id).activated {
        call_cb(vm, st, id, "Terminate");
    }
    // ConfirmCautionTarget's Terminate (FUN_1405c3770, FwLo's trace): still in CAUTION and no
    // new sound this tick (INTERUPT_ChangeSoundTarget), so the caution target is given up.
    if st.goal(id).imp == GoalImpl::Native(NativeGoal::ConfirmCaution)
        && st.target_state == 1
        && !st.sound_new
    {
        st.sound = None;
    }
    st.goals[id as usize] = None;
}

/// Updates goal `id` (activating it first) and returns its result, which its slot keeps
/// (`AiState::slot_results`; the exe writes goal +0x1f8 on every update, FUN_1405d06c0).
fn update(vm: &mut Vm, st: &mut AiState, id: u32, depth: usize) -> i32 {
    let result = update_goal(vm, st, id, depth);
    let slot = id as usize;
    if st.slot_results.len() <= slot {
        st.slot_results.resize(slot + 1, -1);
    }
    st.slot_results[slot] = result;
    result
}

fn update_goal(vm: &mut Vm, st: &mut AiState, id: u32, depth: usize) -> i32 {
    if depth > 24 {
        return RESULT_FAILED;
    }
    let dt = st.world.dt;
    if st.time
        >= st
            .act_hook
            .as_ref()
            .map_or(f32::INFINITY, |h| h.break_until)
        && let Some(hook) = st.act_hook.as_mut()
    {
        hook.break_followup = None;
    }
    if !st.goal(id).activated {
        st.goal_mut(id).activated = true;
        let kind = st.goal(id).kind;
        let name = st.name(kind);
        st.log(AiEvent::GoalStart { depth, kind, name });
        if let Some(step) = st.goal(id).plan_step {
            let time = st.time;
            if let Some(hook) = st.act_hook.as_mut() {
                hook.started.push((time, step));
            }
        }
        apply_timing_sets(st, id, TIMING_ACTIVATE);
        call_cb(vm, st, id, "Activate");
        if kind == PLAN_APPROACH {
            let d = st.goal(id).param(0);
            if let Err(e) = vm.call_global(
                st,
                "Approach_Act_Flex",
                &[
                    ai_obj(),
                    goal_obj(id),
                    Value::Num(d),
                    Value::Num(d),
                    Value::Num(d + 3.0),
                    Value::Num(30.0),
                    Value::Num(0.0),
                    Value::Num(3.0),
                    Value::Num(5.0),
                ],
            ) {
                st.log(AiEvent::Error(e.to_string()));
                return RESULT_FAILED;
            }
        }
        if st.goals.get(id as usize).is_none_or(Option::is_none) {
            return RESULT_FAILED;
        }
    }
    // Replace the post-break follow-up after Activate, and retain ownership until
    // the attack starts. Observed effects can replan one tick after the hit.
    if st.goal(id).kind == st.think.battle_goal_id
        && let Some(choice) = st.act_hook.as_ref().and_then(|h| h.break_followup.clone())
    {
        let attack = choice.goals.iter().find_map(|g| {
            g.params
                .first()
                .copied()
                .map(|p| p as i32)
                .filter(|id| (3000..3100).contains(id))
        });
        if !st.act_hook.as_ref().unwrap().break_response
            && attack.is_some()
            && st.world.current_anim == attack
            && st.goals.iter().flatten().any(|g| {
                g.started_anim
                    && g.params
                        .first()
                        .and_then(Value::num)
                        .is_some_and(|p| Some(p as i32) == attack)
            })
        {
            st.act_hook.as_mut().unwrap().break_followup = None;
        } else if st.act_hook.as_ref().unwrap().break_response
            || !st
                .goal(id)
                .subgoals
                .iter()
                .any(|&g| st.goal(g).plan_step.is_some())
        {
            let subs: Vec<_> = st.goal_mut(id).subgoals.drain(..).collect();
            for s in subs {
                terminate(vm, st, s);
            }
            let hook = st.act_hook.as_mut().unwrap();
            hook.choice = None;
            if hook.break_response {
                hook.slots.push((st.time, ActSlot::External));
            }
            hook.break_response = false;
            if let Err(e) = st.queue_act(vm, id, &choice) {
                st.log(AiEvent::Error(e.to_string()));
                st.act_hook.as_mut().unwrap().break_followup = None;
            }
        }
    }
    {
        let g = st.goal_mut(id);
        g.elapsed += dt;
        for v in g.timers.values_mut() {
            *v -= dt;
        }
    }
    // Subgoals first, front to back; a finished one is removed and the next starts next tick.
    let mut emptied = false;
    if let Some(&front) = st.goal(id).subgoals.front() {
        let r = update(vm, st, front, depth + 1);
        if r != RESULT_CONTINUE {
            st.goal_mut(id).subgoals.pop_front();
            let kind = st.goals[front as usize].as_ref().map_or(0, |g| g.kind);
            let name = st.name(kind);
            st.log(AiEvent::GoalEnd {
                kind,
                name,
                result: r,
            });
            if r == RESULT_SUCCESS && st.goals[front as usize].is_some() {
                apply_timing_sets(st, front, TIMING_UPDATE_SUCCESS);
            }
            terminate(vm, st, front);
            if r == RESULT_FAILED {
                let subs: Vec<u32> = st.goal_mut(id).subgoals.drain(..).collect();
                for s in subs {
                    terminate(vm, st, s);
                }
                return RESULT_FAILED;
            }
            emptied = st.goal(id).subgoals.is_empty();
        }
    }
    let imp = st.goal(id).imp;
    let result = match imp {
        GoalImpl::Native(n) => native_update(st, id, n),
        GoalImpl::Table | GoalImpl::Script => {
            match call_cb(vm, st, id, "Update").and_then(|v| v.num()) {
                Some(r) => r as i32,
                None if st.goals[id as usize].is_none() => RESULT_FAILED,
                None => {
                    if st.goal(id).subgoals.is_empty() {
                        RESULT_SUCCESS
                    } else {
                        RESULT_CONTINUE
                    }
                }
            }
        }
        GoalImpl::Empty => {
            if st.goal(id).subgoals.is_empty() {
                RESULT_SUCCESS
            } else {
                RESULT_CONTINUE
            }
        }
    };
    if st.goals.get(id as usize).is_none_or(Option::is_none) {
        return RESULT_FAILED;
    }
    // A goal whose last subgoal just succeeded succeeds too, unless its Update decided
    // otherwise or added new subgoals (the script goals' Update functions return Continue and
    // rely on this).
    if emptied && result == RESULT_CONTINUE && st.goal(id).subgoals.is_empty() {
        return RESULT_SUCCESS;
    }
    let g = st.goal_mut(id);
    if result == RESULT_CONTINUE && g.life >= 0.0 {
        g.life -= dt;
        if g.life < 0.0 {
            // Lifetime over: waiting-style goals succeed, the rest fail (our reading of how
            // the scripts use lifetimes, see docs/AI.md).
            let ok = g.life_end_success.unwrap_or(
                matches!(
                    g.imp,
                    GoalImpl::Native(NativeGoal::Wait | NativeGoal::Guard | NativeGoal::Sideway)
                ) || g.timed,
            );
            return if ok { RESULT_SUCCESS } else { RESULT_FAILED };
        }
    }
    result
}

fn native_update(st: &mut AiState, id: u32, n: NativeGoal) -> i32 {
    let g = st.goal(id).clone();
    match n {
        NativeGoal::PlanApproach => {
            if g.subgoals.is_empty() {
                RESULT_SUCCESS
            } else {
                RESULT_CONTINUE
            }
        }
        NativeGoal::PlanDeflect => {
            st.request.face_yaw = st.yaw_to(0);
            if g.phase == 1 && g.subgoals.is_empty() {
                RESULT_SUCCESS
            } else {
                RESULT_CONTINUE
            }
        }
        NativeGoal::PlanDodgeBack => {
            st.request.face_yaw = st.yaw_to(0);
            if g.phase == 0 {
                if !st.world.target_attacking || st.dist(0) > 2.5 {
                    return RESULT_CONTINUE;
                }
                st.goal_mut(id).phase = 1;
            }
            action_update(st, id, &g)
        }
        NativeGoal::Wait => {
            // Wait(life, turnTarget, ...) but Stay(life, ?, turnTarget): NonBattleAct's
            // `Stay(life, 0, POINT_INIT_POSE)` turns to its post, not to TARGET_ENE_0 (FwLo's
            // `src/ai.rs`, Native::Stay).
            let target = g.param(if g.kind == GOAL_STAY { 1 } else { 0 }) as i32;
            if target != -1 && target != -2 {
                st.request.face_yaw = st.yaw_to(target);
            }
            RESULT_CONTINUE
        }
        NativeGoal::MoveTo => {
            // MoveToSomewhere(target, dirType, dist, turnTarget, walk, ...).
            let target = g.param(0) as i32;
            let dist = g.param(2) as f32;
            let walk = g.param_bool(4);
            // The exe's IsArrived (FUN_1405b9070, read by the move update FUN_1405caa00, as
            // FwLo's `src/ai.rs` traced it) arrives closer than my own radius plus the goal's
            // distance, strictly, for every target; its GetDist subtracts the same radius, so
            // the scripts' distances agree with it. Our GetDist is centre to centre (bar
            // `dist_offset`), so the radius is added here only for the patrol point, whose
            // distance no script reads; for other targets the arrival stays in GetDist's terms
            // (docs/FIDELITY.md, #114). A negative distance counts as 0 (not traced), so the
            // goal can still end. Reaching the patrol point steps the route, as the exe's move
            // goals do in their terminate callbacks; no AI script calls StepNextMovePoint, so
            // this is the route's only step.
            let radius = if target == 103 {
                st.world.hit_radius
            } else {
                0.0
            };
            if st.dist(target) < dist.max(0.0) + radius {
                if target == 103
                    && let Some(p) = &mut st.patrol
                {
                    p.step(st.rng);
                }
                return RESULT_SUCCESS;
            }
            st.request.move_yaw = st.yaw_to(target);
            st.request.move_level = if walk { 0.5 } else { 1.0 };
            st.request.face_yaw = st.yaw_to(target);
            RESULT_CONTINUE
        }
        NativeGoal::BackToHome => {
            // BackToHome_With_Parry passes TARGET_ENE_0 as a parameter, but the goal walks to
            // the initial position. As in FwLo's `src/ai.rs`, it arrives within 1 m, the
            // distance NonBattleAct accepts as home, measured in 3D as NonBattleAct's
            // GetMovePointEffectRange does: on the ground plane a foe could stop 1 m off and
            // still be more than 1 m away in 3D, and NonBattleAct then sent it home again
            // every tick (the exe's arrival radius is not traced, docs/FIDELITY.md).
            let home = st.position(POINT_INITIAL).map(glam::Vec3::from_array);
            let me = glam::Vec3::from_array(st.world.me.position);
            if home.is_none_or(|h| me.distance(h) <= 1.0) {
                return RESULT_SUCCESS;
            }
            st.request.move_yaw = st.yaw_to(POINT_INITIAL);
            st.request.move_level = 0.5;
            st.request.face_yaw = st.request.move_yaw;
            RESULT_CONTINUE
        }
        NativeGoal::Guard => {
            // Guard(actionId, target, ...).
            let action = g.param(0) as i32;
            if action > 0 {
                st.request.action = action;
            }
            st.request.face_yaw = st.yaw_to(g.param(1) as i32);
            RESULT_CONTINUE
        }
        NativeGoal::Sideway => {
            // SidewayMove(target, right(0)/left(1), angle, turnTarget, walk, ...).
            let target = g.param(0) as i32;
            let left = g.param(1) as i32 == 1;
            if let Some(y) = st.yaw_to(target) {
                let side = if left { PI / 2.0 } else { -PI / 2.0 };
                st.request.move_yaw = Some(y + side);
                st.request.face_yaw = Some(y);
                st.request.move_level = 0.5;
            }
            RESULT_CONTINUE
        }
        NativeGoal::KeepDist => {
            // KeepDist(target, minDist, maxDist, turnTarget, walk, ...).
            let target = g.param(0) as i32;
            let (lo, hi) = (g.param(1) as f32, g.param(2) as f32);
            let d = st.dist(target);
            let Some(y) = st.yaw_to(target) else {
                return RESULT_SUCCESS;
            };
            st.request.face_yaw = Some(y);
            st.request.move_level = if g.param_bool(4) { 0.5 } else { 1.0 };
            if d < lo {
                st.request.move_yaw = Some(y + PI);
            } else if d > hi {
                st.request.move_yaw = Some(y);
            } else if g.timed {
                // A plan's keep-range holds its range until the lifetime ends.
                st.request.move_level = 0.0;
            } else {
                return RESULT_SUCCESS;
            }
            RESULT_CONTINUE
        }
        NativeGoal::Leave => {
            // LeaveTarget(target, dist, turnTarget, walk, ...).
            let target = g.param(0) as i32;
            if st.dist(target) >= g.param(1) as f32 {
                return RESULT_SUCCESS;
            }
            if let Some(y) = st.yaw_to(target) {
                st.request.move_yaw = Some(y + PI);
                st.request.face_yaw = Some(if g.away { y + PI } else { y });
                st.request.move_level = if g.param_bool(3) { 0.5 } else { 1.0 };
            }
            RESULT_CONTINUE
        }
        NativeGoal::Action => action_update(st, id, &g),
        NativeGoal::ConfirmCaution => {
            // Activate (FUN_1405c35a0, FwLo's trace): CommonAttack(anim, target, 9999, 180,
            // 1.5, 20) when the anim is set, then Wait(time, waitTarget).
            if !g.started_anim {
                st.goal_mut(id).started_anim = true;
                let p = |i: usize| g.params.get(i).cloned().unwrap_or(Value::Nil);
                if g.params
                    .first()
                    .and_then(Value::num)
                    .is_some_and(|a| a >= 0.0)
                {
                    let look = [180.0, 1.5, 20.0].map(Value::Num);
                    let mut a = vec![Value::Num(2200.0), Value::Num(-1.0), p(0), p(1)];
                    a.push(Value::Num(9999.0));
                    a.extend(look);
                    st.add_goal(id, false, &a);
                }
                st.add_goal(id, false, &[Value::Num(2000.0), p(2), p(3)]);
                return RESULT_CONTINUE;
            }
            if st.goal(id).subgoals.is_empty() {
                RESULT_SUCCESS
            } else {
                RESULT_CONTINUE
            }
        }
        NativeGoal::Unsupported => RESULT_SUCCESS,
    }
}

/// Whether the goal or one of its ancestors is an attack step of an outside plan (the script
/// goals such as `AttackTunableSpin` queue the native attack below the flagged goal).
fn under_face_first(st: &AiState, g: &Goal) -> bool {
    let mut at = Some(g);
    for _ in 0..4 {
        let Some(goal) = at else { return false };
        if goal.face_first {
            return true;
        }
        at = goal.parent.and_then(|p| st.goals.get(p as usize)?.as_ref());
    }
    false
}

/// CommonAttack(ezStateId, target, successDist, turnAngle, turnTime, frontAngle, isCombo, ...)
/// and the other action goals: turn toward the target, request the action until its animation
/// plays, then wait for it to end (or, for combo steps, for the combo window).
fn action_update(st: &mut AiState, id: u32, g: &Goal) -> i32 {
    let action = g.param(0) as i32;
    let target = g.param(1) as i32;
    let combo = g.kind == 2200 && g.param_bool(6);
    if target >= 0 {
        st.request.face_yaw = st.yaw_to(target);
    }
    let playing = st.world.current_anim == Some(action);
    if !g.started_anim {
        // The plan step can be a table goal (ComboAttackTunableSpin) whose script adds this
        // CommonAttack, so the step is looked up through the parents.
        let in_plan = std::iter::successors(Some(g), |g| g.parent.map(|p| st.goal(p)))
            .any(|g| g.plan_step.is_some());
        if in_plan
            && st
                .act_hook
                .as_ref()
                .and_then(|h| h.reachable.as_ref())
                .is_some_and(|ids| !ids.contains(&action))
            && (3000..3100).contains(&action)
        {
            // A setup's target can move away before its attack activates.
            st.request.move_yaw = st.yaw_to(target);
            st.request.move_level = 1.0;
            return RESULT_CONTINUE;
        }
        let followup = st
            .act_hook
            .as_ref()
            .is_some_and(|h| h.break_followup.is_some());
        if g.phase == 0 && playing && followup {
            st.goal_mut(id).restart_clock = Some(st.world.current_anim_time);
        }
        let restarted = g
            .restart_clock
            .is_none_or(|time| st.world.current_anim_time < time);
        if playing && restarted && !(g.phase == 0 && followup) {
            st.goal_mut(id).started_anim = true;
            return RESULT_CONTINUE;
        }
        if !playing {
            st.goal_mut(id).restart_clock = None;
        }
        let face_first = under_face_first(st, g);
        let wait = if face_first { FACE_FIRST_SECS } else { 0.0 };
        if face_first
            && target >= 0
            && g.elapsed < FACE_FIRST_SECS
            && st.angle_to(target).abs() > FACE_FIRST_DEG
        {
            // Turn to the target first; the action is requested once he faces it. His idle
            // state does not turn (its TAE sets the turn speed to 0), so, as in the game's own
            // approach goals, he turns while stepping toward the target.
            st.request.move_yaw = st.yaw_to(target);
            st.request.move_level = 0.5;
            return RESULT_CONTINUE;
        }
        if !followup && g.elapsed > 2.0 + wait {
            return RESULT_FAILED;
        }
        if g.phase == 0 {
            st.log(AiEvent::Action(action));
            st.action_age.insert(action, 0.0);
            st.goal_mut(id).phase = 1;
        }
        st.request.action = action;
        let parent = g.parent.and_then(|p| st.goals.get(p as usize)?.as_ref());
        st.request.combo = parent.is_some_and(|p| st.combo_cancel.contains(&p.kind));
        // A break follow-up uses the same NPC combo window even when its single-attack
        // wrapper is not registered by this boss's Lua script.
        st.request.combo |= st
            .act_hook
            .as_ref()
            .is_some_and(|h| h.break_followup.is_some());
        st.request.endure = parent.is_some_and(|p| p.kind == GOAL_ENDURE_ATTACK);
        st.request.immediate = st.immediate_action;
        return RESULT_CONTINUE;
    }
    if !playing {
        return RESULT_SUCCESS;
    }
    if combo && st.world.combo_window {
        return RESULT_SUCCESS;
    }
    RESULT_CONTINUE
}

#[cfg(test)]
mod counter_choice_tests {
    use super::super::lua50::Table;
    use super::*;

    #[test]
    fn reweights_only_offered_callbacks_and_preserves_mandatory_choice() {
        let mut state = AiState::new(ThinkParams::default(), HashMap::new(), 1);
        state.act_hook = Some(ActHook {
            counter_weights: vec![(14, 0.0), (1, 1.0), (7, 10.0)],
            ..Default::default()
        });
        let mut vm = Vm::new();
        vm.set_global("__brain_kengeki_activate", Value::Host("print".into()));
        let table = Table::new_ref();
        table.borrow_mut().set(Value::Num(14.0), Value::Num(100.0));
        state
            .kengeki_activate(
                &mut vm,
                &[Value::Nil, Value::Nil, Value::Table(table.clone())],
            )
            .unwrap();
        assert_eq!(table.borrow().get(&Value::Num(14.0)).num(), Some(100.0));
        table.borrow_mut().set(Value::Num(1.0), Value::Num(100.0));
        state
            .kengeki_activate(
                &mut vm,
                &[Value::Nil, Value::Nil, Value::Table(table.clone())],
            )
            .unwrap();
        assert_eq!(table.borrow().get(&Value::Num(14.0)).num(), Some(0.0));
        assert_eq!(table.borrow().get(&Value::Num(1.0)).num(), Some(10000.0));
        assert_eq!(table.borrow().get(&Value::Num(7.0)).num(), Some(0.0));
        // Lua's offered NoAction is a valid fallback, even with one attack candidate.
        table.borrow_mut().set(Value::Num(14.0), Value::Num(100.0));
        table.borrow_mut().set(Value::Num(1.0), Value::Num(0.0));
        table.borrow_mut().set(Value::Num(50.0), Value::Num(20.0));
        state
            .kengeki_activate(
                &mut vm,
                &[Value::Nil, Value::Nil, Value::Table(table.clone())],
            )
            .unwrap();
        assert_eq!(table.borrow().get(&Value::Num(14.0)).num(), Some(0.0));
        assert_eq!(table.borrow().get(&Value::Num(50.0)).num(), Some(2000.0));
    }
}

#[cfg(test)]
mod goal_slot_tests {
    use super::*;

    /// A move and the goal it belongs to finish in the same tick; the goal added next takes
    /// the lowest free slot and its `GetLastResult` reads the Success that slot last stored,
    /// as a fresh NonBattleAct does after arriving at a patrol point (#116).
    #[test]
    fn a_new_goal_reads_the_success_its_slot_last_stored() {
        let mut st = AiState::new(ThinkParams::default(), HashMap::new(), 1);
        let mut vm = Vm::new();
        st.world.dt = 0.1;
        let top = st.top;
        // An empty goal (a stand-in for NonBattleAct) with one Wait (2000) child whose life
        // ends this tick.
        let act = st.new_goal(-7, -1.0, Vec::new(), Some(top));
        st.goal_mut(top).subgoals.push_back(act);
        let wait = st.new_goal(2000, 0.0, vec![Value::Num(-1.0)], Some(act));
        st.goal_mut(act).subgoals.push_back(wait);
        let fresh = |st: &mut AiState| {
            let id = st.new_goal(-7, -1.0, Vec::new(), Some(top));
            let r = st.goal_method(id, "GetLastResult", &[]).unwrap();
            st.goals[id as usize] = None;
            (id, r[0].num())
        };
        // Before any update the slot reads the constructor's -1.
        let (id, before) = fresh(&mut st);
        assert_eq!((id, before), (3, Some(-1.0)));
        update(&mut vm, &mut st, top, 0);
        assert!(st.goals[act as usize].is_none() && st.goals[wait as usize].is_none());
        let (id, after) = fresh(&mut st);
        assert_eq!(id, act, "the lowest free slot is reused");
        assert_eq!(after, Some(f64::from(RESULT_SUCCESS)));
    }

    /// MoveToSomewhere arrives strictly closer than the goal's distance, plus the AI's own
    /// radius for the patrol point (103) only, the target whose distance no script reads
    /// (#114). It used to accept equality.
    #[test]
    fn move_to_arrives_strictly_and_adds_the_radius_for_the_patrol_point() {
        let arrives = |target: i32, gap: f32| {
            let mut st = AiState::new(ThinkParams::default(), HashMap::new(), 1);
            st.world.hit_radius = 0.5;
            st.world.home = [0.0, 0.0, gap];
            st.patrol = Some(super::super::patrol::Patrol::from_points(
                0,
                &[[0.0, 0.0, gap], [5.0, 0.0, 0.0]],
            ));
            let params = vec![
                Value::Num(f64::from(target)),
                Value::Num(0.0),
                Value::Num(1.0),
            ];
            let id = st.new_goal(2019, -1.0, params, None);
            native_update(&mut st, id, NativeGoal::MoveTo) == RESULT_SUCCESS
        };
        assert!(arrives(103, 1.4));
        assert!(!arrives(103, 1.5), "the patrol point arrives at its edge");
        assert!(arrives(100, 0.9));
        assert!(
            !arrives(100, 1.0),
            "the initial position arrives at its edge"
        );
        assert!(!arrives(100, 1.4), "the initial position adds the radius");
    }

    /// A patrol visits every point in order, none skipped (#114): arriving at the current
    /// point steps the route once, and an update short of the point does not step it.
    #[test]
    fn a_patrol_visits_every_point_in_order() {
        let mut st = AiState::new(ThinkParams::default(), HashMap::new(), 1);
        st.world.hit_radius = 0.4;
        let points = [
            [0.0, 0.0, 0.0],
            [4.0, 0.0, 0.0],
            [4.0, 0.0, 4.0],
            [0.0, 0.0, 4.0],
        ];
        st.patrol = Some(super::super::patrol::Patrol::from_points(0, &points));
        let mut visited = Vec::new();
        for _ in 0..8 {
            let p = st.patrol.as_ref().unwrap();
            let (cur, at) = (p.current, p.position().unwrap());
            let params = vec![Value::Num(103.0), Value::Num(0.0), Value::Num(0.0)];
            let id = st.new_goal(2019, -1.0, params, None);
            // On the way: 2 m short of the point.
            st.world.me.position = [at[0], at[1], at[2] - 2.0];
            assert_eq!(
                native_update(&mut st, id, NativeGoal::MoveTo),
                RESULT_CONTINUE
            );
            assert_eq!(st.patrol.as_ref().unwrap().current, cur);
            // There.
            st.world.me.position = at;
            assert_eq!(
                native_update(&mut st, id, NativeGoal::MoveTo),
                RESULT_SUCCESS
            );
            st.goals[id as usize] = None;
            visited.push(st.patrol.as_ref().unwrap().previous);
        }
        assert_eq!(visited, [0, 1, 2, 3, 0, 1, 2, 3]);
    }

    /// Flags read as the exe's float test `0.0 < param`: Lua booleans, the brain's 1.0/0.0,
    /// and a negative value is unset.
    #[test]
    fn flag_parameters_are_set_only_when_positive() {
        let mut st = AiState::new(ThinkParams::default(), HashMap::new(), 1);
        let params = vec![
            Value::Bool(true),
            Value::Bool(false),
            Value::Num(1.0),
            Value::Num(0.0),
            Value::Num(-1.0),
        ];
        let id = st.new_goal(2019, -1.0, params, None);
        let g = st.goal(id);
        let flags: Vec<bool> = (0..6).map(|i| g.param_bool(i)).collect();
        assert_eq!(flags, [true, false, true, false, false, false]);
        let read = |st: &mut AiState, i: f64| {
            st.goal_method(id, "GetParam", &[Value::Num(i)]).unwrap()[0].num()
        };
        assert_eq!(read(&mut st, 0.0), Some(1.0));
        assert_eq!(read(&mut st, 1.0), Some(0.0));
    }
}
