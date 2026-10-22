//! Enemy brains: FromSoftware's own compiled AI scripts (Lua 5.0 bytecode from the user's
//! script/aicommon.luabnd and script/mXX_XX_00_00.luabnd, unpacked by tools/extract) run
//! unchanged in a small Lua 5.0 VM (ai/lua50.rs), on a reimplementation of the engine side:
//!   * the goal tree (Activate / Update / Terminate / Interupt, life, subgoal queues),
//!   * the native goals the scripts bottom out in (CommonAttack, MoveToSomewhere,
//!     SidewayMove, LeaveTarget, SpinStep, Wait, ...),
//!   * the `ai` object the scripts query (distances, SpEffects, timers, numbers, ...).
//!
//! Rust gives the brain a world snapshot each fixed frame and gets back one action request
//! (attack anim, move direction, turn), which enemy.rs carries out. Engine rule for requests
//! (from the NPC TAEs): an action can start when the current anim is free or has its AI
//! cancel flag active: 23 combo attack (goals with ENABLE_COMBO_ATK_CANCEL), 86 attack,
//! 79 step, 78 move.
//!
//! The plan comes from the character's logic script (NpcThinkParam logicId, e.g.
//! 102000_logic.lua) the way the engine runs it: with no top goal left, ExecTableLogic ->
//! Logic.Main -> COMMON_EzSetup picks top goals from the target state (stealth.rs): the
//! non-battle act at home, the caution search of a spot, the alert transitions, or the
//! battle goal. A target-state change drops the plan (FUN_1405ab7d0 -> FUN_14061f500).

mod lua50;

use lua50::{Host, LuaResult, Obj, TableRef, Value, Vm, load_chunk};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use bevy::math::Vec3;
use serde_json::Value as Json;

/// What the brain sees this frame.
#[derive(Default, Clone)]
pub struct Snapshot {
    pub dist: f32,
    /// Signed angle (deg) from the enemy's facing to the target, positive to the right.
    pub angle: f32,
    /// Bearing (deg) of the enemy around the target, for SidewayMove.
    pub bearing: f32,
    pub target_guard: bool,
    /// The enemy itself is in its guard stance (IsTargetGuard(TARGET_SELF)).
    pub self_guard: bool,
    pub sp_self: HashSet<i64>,
    pub sp_target: HashSet<i64>,
    pub sp_new: Vec<i64>,
    /// SpEffects that were on last frame and are gone now (INTERUPT_InactivateSpecialEffect).
    pub sp_gone: Vec<i64>,
    pub hp_rate: f32,
    pub target_hp_rate: f32,
    /// Posture left (Sekiro's "stamina"): max - accumulated posture damage.
    pub sp: f32,
    pub sp_rate: f32,
    /// The target's (Wolf's or the rival's) posture left and its rate (GetSp / GetSpRate(TARGET_ENE_0)).
    pub target_sp: f32,
    pub target_sp_rate: f32,
    /// The current anim is idle or locomotion (any action may start).
    pub free: bool,
    /// Active NPC cancel flags on the current anim (23 combo, 86 attack, 79 step, 78 move).
    pub flags: Vec<i64>,
    /// EzState id of the AI-requested anim that is playing, its clock and whether it ended.
    pub ez: Option<i64>,
    pub t: f32,
    pub anim_done: bool,
    pub ez_started: Option<i64>,
    pub ez_failed: Option<i64>,
    /// Something other than the AI replaced its action (reaction, parry).
    pub interrupted: bool,
    /// The player's attack just raised the AI notify (INTERUPT_ParryTiming).
    pub parry_timing: bool,
    /// ConsecutiveGuardCount (engine-side clash count, timer 13).
    pub guard_count: i32,
    /// The player just started using an item (INTERUPT_UseItem).
    pub target_use_item: bool,
    /// World positions: the enemy and its facing, and its home (POINT_INITIAL) and home
    /// facing (POINT_INIT_POSE).
    pub me: Vec3,
    pub fwd: Vec3,
    /// The current target's (TARGET_ENE_0) position and facing; zero facing = a spot.
    pub target: Vec3,
    pub target_fwd: Vec3,
    pub home: Vec3,
    pub home_fwd: Vec3,
    /// The target system (stealth.rs): state, previous state, AI_TARGET_TYPE, whether the state
    /// changed this frame, Wolf seen at the last search, seconds since he was last seen, and the
    /// latest sound target's AiSoundParam id / rank. `dist` / `angle` / `bearing` are to the
    /// current target (TARGET_ENE_0): Wolf, or the spot of a caution target.
    pub t_state: u8,
    pub t_prev: u8,
    pub t_kind: u8,
    pub t_changed: bool,
    pub visible: bool,
    pub forgetting: f32,
    pub sound_id: i64,
    pub sound_rank: i64,
    /// The sound target was taken or changed this frame: INTERUPT_ChangeSoundTarget (46).
    pub sound_new: bool,
    /// The target state changed in a way that drops the plan (stealth.rs Targeting::replan).
    pub t_replan: bool,
    /// Deathblows left (Enemy::ninsatsu, NpcParam ninsatuNum; at least 1).
    pub ninsatsu: u32,
    pub ninsatsu_max: u32,
    /// Map-event AI commands by slot (EMEVD Request Character AI Command; enemy.rs phase rules).
    pub event_req: Vec<(i64, i64)>,
    /// A command was written this frame: INTERUPT_EventRequest (39).
    pub event_req_new: bool,
    /// The AI chr's own dummy polys its observe slots read (`Brain::observe_dmys`): (id, world
    /// position, the dummy's +Z axis).
    pub dummies: Vec<(i16, Vec3, Vec3)>,
    /// The map's event flags that are on (the running boss script's, enemy/script.rs).
    pub event_flags: Vec<i64>,
    /// Wolf's lock-on (camera.rs LockOn) is on this enemy while it targets him:
    /// IsLockOnTarget(TARGET_ENE_0 / TARGET_LOCALPLAYER, TARGET_SELF).
    pub locked_on: bool,
    /// GetTeamOrder(ORDER_TYPE_Role): enemy.rs assign_team_roles (0 Attack, 1 Torimaki, 2
    /// Kankyaku, -1 none).
    pub team_role: f64,
    /// The loaded map's MSB regions by entity id (enemy.rs map_regions): IsInsideTargetRegion
    /// and SetEventMoveTarget. None = no map: every region check is false.
    pub regions: Option<std::sync::Arc<std::collections::HashMap<i64, crate::enemy::WakeRegion>>>,
    /// This chr's id (Entity bits): its own trigger regions never alert it (FUN_1406074a0).
    pub me_id: u64,
    /// The sound target's record serial (GetLatestSoundTargetInstanceID; stealth.rs Sound).
    pub sound_serial: i64,
    /// Every live trigger region (enemy.rs TriggerRegions, RegisterTriggerRegion).
    pub triggers: Vec<TriggerRegion>,
    /// TARGET_EVENT: the character the map script named (EMEVD 2004[11] Set Character Event
    /// Target; enemy/script.rs). None: not set, or Wolf (then it reads as the current target).
    pub event: Option<EventView>,
}

/// What the AI sees of its TARGET_EVENT character (the Headless Ape's partner ape, m17 11705880;
/// Lady Butterfly for her illusions, m10 11005820; the Old Dragons' body for their parts, m25
/// 12505803).
#[derive(Default, Clone, Debug)]
pub struct EventView {
    pub pos: Vec3,
    pub fwd: Vec3,
    pub hp_rate: f32,
    /// Posture left and its rate (GetSp / GetSpRate).
    pub sp_left: f32,
    pub sp_rate: f32,
    /// The SpEffects it wears, those new this frame and those gone (its observe interrupts).
    pub sp: HashSet<i64>,
    pub sp_new: Vec<i64>,
    pub sp_gone: Vec<i64>,
}

/// RegisterTriggerRegion's sphere (exe FUN_1405e7280 -> FUN_140642110, 14064.c:648): keyed by
/// (registrant, category, unit id; a repeat replaces it), static at its centre, `life` seconds
/// (FUN_140643250 counts it down each frame). The unit id is a tag the getters hand back.
#[derive(Clone, Debug, PartialEq)]
pub struct TriggerRegion {
    pub owner: u64,
    pub cat: i64,
    pub unit: i64,
    pub at: Vec3,
    pub radius: f32,
    pub life: f32,
}

impl TriggerRegion {
    /// FUN_1406424f0 (14064.c:1611): a sphere holds a point within its radius, in 3D.
    pub fn contains(&self, p: Vec3) -> bool {
        p.distance_squared(self.at) < self.radius * self.radius
    }
}

#[cfg(test)]
impl Snapshot {
    /// A snapshot already in battle with Wolf as the target (the duel; tests).
    pub fn battle() -> Snapshot {
        Snapshot { t_state: 3, t_prev: 3, t_kind: 3, visible: true, fwd: Vec3::Z, home_fwd: Vec3::Z, ..Default::default() }
    }
}

/// Target slots the AI cleared this frame (ClearEnemyTarget, ClearSoundTarget, ...).
pub const CLEAR_ENEMY: u8 = 1;
pub const CLEAR_SOUND: u8 = 2;
pub const CLEAR_INDICATION: u8 = 4;
pub const CLEAR_MEMORY: u8 = 8;

/// One action request.
#[derive(Default, Debug, Clone, PartialEq)]
pub struct Command {
    pub anim: Option<i64>,
    /// 'F', 'B', 'L', 'R'
    pub mv: Option<char>,
    pub walk: bool,
    pub face: bool,
    pub turn: bool,
    /// RequestEmergencyQuickTurn (FUN_1405e76e0 -> AI +0x9054, cleared every frame
    /// 14060.c:7485; the HKS's env(330)): a turn clip even under 45 degrees.
    pub emergency: bool,
    /// The guard goal's request 9910 (c9997 ACTION_TYPE_GUARD, env(106)) this frame: the guard
    /// stance (enemy.rs guard_stance), held while it is asked for.
    pub guard: bool,
    /// The same with 9920 (ACTION_TYPE_GENERAL_STANCE, c9997.lua:259): the general stance.
    pub general: bool,
    /// The point a move / turn is relative to when it is not the current target: home
    /// (POINT_INITIAL), the home facing (POINT_INIT_POSE).
    pub at: Option<Vec3>,
    /// CLEAR_* bits.
    pub clear: u8,
    /// Event flags the AI set this frame, in order (SetEventFlag(flag, on)).
    pub flags: Vec<(i64, bool)>,
    /// Trigger regions registered this frame (RegisterTriggerRegion), for enemy.rs's store.
    pub triggers: Vec<TriggerRegion>,
    /// The anim request is an EndureAttack goal's (the HKS's env 123).
    pub endure: bool,
}

/// TAE flag 122 (SetBool32_0x1E0): the window for an EndureAttack goal's request (env 123).
pub(crate) const FLAG_ENDURE: i64 = 122;

/// The guard request (c9997.lua:258 ACTION_TYPE_GUARD), GOAL_COMMON_Guard's usual action.
const ACTION_TYPE_GUARD: f64 = 9910.0;
/// The general stance request (c9997.lua:259 ACTION_TYPE_GENERAL_STANCE).
const ACTION_TYPE_GENERAL_STANCE: f64 = 9920.0;

const SUCCESS: i32 = 1;
const CONTINUE: i32 = 0;
const FAILED: i32 = -1;
/// An Update that returned something other than a number (neither result).
const OTHER: i32 = 2;

const OBJ_AI: u8 = 0;
const OBJ_GOAL: u8 = 1;

/// Engine functions the scripts call by global name.
const NATIVE_GLOBALS: [&str; 9] = [
    "REGISTER_GOAL",
    "REGISTER_LOGIC_FUNC",
    "REGISTER_GOAL_NO_UPDATE",
    "REGISTER_GOAL_NO_INTERUPT",
    "REGISTER_GOAL_NO_SUB_GOAL",
    "REGISTER_GOAL_UPDATE_TIME",
    "REGISTER_DBG_GOAL_PARAM",
    "ENABLE_COMBO_ATK_CANCEL",
    "REGISTER_GOAL_USE_AVOID_CHR",
];

/// The engine's leaf goals the scripts bottom out in, by goal id (GOAL_COMMON_*).
#[derive(Clone, Copy, PartialEq, Debug)]
enum Native {
    /// 2200 CommonAttack(ezState, target, successDist, turnAngle, turnTime, frontAngle, ...)
    Attack,
    /// 2020 SpinStep(ezState, target, ?, dirType, dist): a step anim, AI step flag 79.
    SpinStep,
    /// 2019 MoveToSomewhere(target, dirType, dist, turnTarget, walk, ...)
    Approach,
    /// 2016 LeaveTarget(target, dist, turnTarget, walk, guard)
    Leave,
    /// 2017 SidewayMove(target, right(1)/left(0), angle, turnTarget, walk, guard)
    Sideway,
    /// 2018 KeepDist(target, min, max, ...)
    Keep,
    /// 2000 Wait(life, target, ...): stand, facing the target when it is the enemy.
    Wait,
    /// 2101 Guard(life, ezState, target, ...): hold the guard anim for the goal's life.
    Guard,
    /// 2 Stay(life, ?, turnTarget): stand still, turned to turnTarget.
    Stay,
    /// 4 BackToHome(life, ...): walk back to POINT_INITIAL.
    BackToHome,
    /// 5100 ConfirmCautionTarget(life, ezState, target, time, waitTarget): CSGoalConfirmCautionTarget
    /// (vtable 0x14294b048). Activate FUN_1405c35a0: CommonAttack(ezState, target, 9999, 180,
    /// 1.5, 20) when ezState >= 0, then Wait(time, waitTarget). Terminate FUN_1405c3770: still in
    /// CAUTION and no new sound / indication / corpse target (INTERUPT 46 / 53 / 55) -> the
    /// caution targets are cleared, so the enemy gives up and goes home.
    ConfirmCaution,
}

fn native(id: i64) -> Option<Native> {
    Some(match id {
        2200 => Native::Attack,
        2020 => Native::SpinStep,
        2019 => Native::Approach,
        // MoveToSomewhereSmooth(target, dirType, dist, turnTarget, walk, ...): the patrol's walk
        // between points without a stop (common_non_battle_act.lua); same params up to walk.
        // gap: what "smooth" changes (no stop at the point, by the name) is not traced.
        2013 => Native::Approach,
        2016 => Native::Leave,
        2017 => Native::Sideway,
        2018 => Native::Keep,
        2000 => Native::Wait,
        2101 => Native::Guard,
        2 => Native::Stay,
        4 => Native::BackToHome,
        5100 => Native::ConfirmCaution,
        _ => return None,
    })
}

impl Native {
    /// Actions that already started run to their own end when their life runs out.
    fn ignore_life(self) -> bool {
        matches!(self, Native::Attack | Native::SpinStep)
    }
}

#[derive(Clone, Copy)]
enum Cb {
    Activate,
    Update,
    Terminate,
    Interrupt,
}

impl Cb {
    fn table_key(self) -> &'static str {
        match self {
            Cb::Activate => "Activate",
            Cb::Update => "Update",
            Cb::Terminate => "Terminate",
            Cb::Interrupt => "Interrupt",
        }
    }
}

/// A registered goal kind: a table goal (g_GoalTable) or a function goal
/// (GOAL_COMMON_X = id with X_Activate / X_Update / X_Terminate / X_Interupt).
#[derive(Default)]
struct Def {
    name: Option<String>,
    no_update: bool,
    no_interrupt: bool,
    /// REGISTER_GOAL_NO_SUB_GOAL: its Update runs while it has subgoals too (the scripts'
    /// Update_Default_NoSubGoal returns Continue until they are done; ApproachSettingDirection
    /// ends its MoveToSomewhere when its own life is out).
    no_sub_goal: bool,
    combo: bool,
    tbl: Option<TableRef>,
    /// activate, update, terminate, interupt
    fns: Option<[Value; 4]>,
}

/// TimingSetTimer(timer, time, when) / TimingSetNumber(slot, value, when): applied on
/// activation (AI_TIMING_SET__ACTIVATE) or on success (UPDATE_SUCCESS).
struct Timing {
    timer: bool,
    id: Value,
    v: Value,
    when: f64,
}

struct Goal {
    kind: Option<i64>,
    life: f64,
    p: Vec<Value>,
    subs: Vec<u32>,
    started: bool,
    terminated: bool,
    t: f64,
    had_sub: bool,
    num: HashMap<i64, Value>,
    timing: Vec<Timing>,
    parent_combo: bool,
    /// Under GOAL_COMMON_EndureAttack (SetEnableEndureCancel_forGoal): its action is the HKS's
    /// endure request (env 123), taken while the clip's TAE flag 122 is on.
    parent_endure: bool,
    // Native goal state.
    ez: Option<f64>,
    succ: f64,
    turn_time: f64,
    front: f64,
    phase: u8,
    busy: bool,
    req: bool,
    start_bearing: f64,
    /// SetLifeEndSuccess (0x1405faa20: goal +0x4a0 bit 0x20): its life running out is a success.
    life_end_success: bool,
}

impl Goal {
    /// GetParam: unset goal params read as 0, like the engine (e.g. EndureAttack's 5th).
    fn param(&self, i: usize) -> Value {
        match self.p.get(i) {
            None | Some(Value::Nil) => Value::Num(0.0),
            Some(v) => v.clone(),
        }
    }
    /// The raw param (1-based in the scripts' `p` table), as a number or `default`.
    fn raw(&self, i: usize, default: f64) -> f64 {
        self.p.get(i).and_then(Value::num).unwrap_or(default)
    }
}

/// Script constants read once the scripts are loaded.
struct Consts {
    target_self: f64,
    target_ene_0: f64,
    target_event: f64,
    dir_b: f64,
    dir_l: f64,
    dir_r: f64,
    timing_activate: f64,
    timing_success: f64,
    int_parry: f64,
    int_use_item: f64,
    int_sp: f64,
    int_sp_off: f64,
    int_sound: f64,
    point_initial: f64,
    point_init_pose: f64,
    point_move_point: f64,
    point_back_to_home: f64,
    point_event: f64,
    team_formation: f64,
}

struct AiState {
    defs: HashMap<i64, Def>,
    goals: HashMap<u32, Goal>,
    next_goal: u32,
    root: Option<u32>,
    /// AddTopGoal requests.
    top_queue: Vec<u32>,
    replan: bool,
    /// The interrupt being dispatched: (kind, SpEffect id).
    interrupt: Option<(f64, f64)>,
    w: Snapshot,
    cmd: Command,
    log: Vec<String>,
    warned: HashSet<String>,
    timers: HashMap<i64, f64>,
    numbers: HashMap<i64, Value>,
    snum: HashMap<String, Value>,
    id_timers: HashMap<i64, f64>,
    atk_passed: HashMap<i64, f64>,
    /// AddObserveSpecialEffectAttribute(target, id): SpEffect id -> the targets watched for it
    /// (TARGET_SELF -1, TARGET_ENE_0 0, TARGET_EVENT 20, TARGET_LOCALPLAYER 21).
    observed: HashMap<i64, Vec<i64>>,
    battle_goal: i64,
    think_id: i64,
    /// NpcThinkParam logicId (the logic script) and the row itself (GetExcelParam).
    logic_id: i64,
    think: Json,
    /// AI_EXCEL_THINK_PARAM_TYPE__<field> value -> NpcThinkParam field name.
    excel: HashMap<i64, String>,
    /// The top goals the logic added (AddTopGoal), run in order; the first is the root.
    tops: Vec<u32>,
    /// Target of the last ai:TurnTo (for IsLookToTarget).
    turn_id: Option<Value>,
    hit_radius: f32,
    rng: u64,
    k: Consts,
    /// The MSB patrol route (Brain::set_patrol), None = no route (GetMovePointNumber -1).
    patrol: Option<Patrol>,
    /// SetEventMoveTarget's region spot: POINT_EVENT (None = home).
    event_target: Option<Vec3>,
    /// The observe slots in the order they were added (the exe's per-AI std::list, any kind
    /// under one byte id, no cap; GetAreaObserveSlot lists changes in this order).
    observe: Vec<(i64, Observe)>,
    /// The observe slots whose target was inside last frame, and this frame's changes (slot ids
    /// that went in / out): INTERUPT_Inside_ObserveArea (16) / Outside (40) with
    /// GetAreaObserveSlotNum / GetAreaObserveSlot(type, index).
    observe_in: HashSet<i64>,
    observe_changed: (Vec<i64>, Vec<i64>),
    /// RegisterTriggerRegionObserver's categories (the exe's per-AI subscription table,
    /// FUN_140642ab0), the regions this chr stood in last frame (owner, category, unit) and
    /// this frame's entries / exits as (category, unit): INTERUPT_InvadeTriggerRegion (48) /
    /// LeaveTriggerRegion (49) with GetInvade / GetLeaveTriggerRegion*.
    trigger_observers: Vec<i64>,
    trigger_in: Vec<(u64, i64, i64)>,
    trigger_changed: (Vec<(i64, i64)>, Vec<(i64, i64)>),
}

/// An enemy's MSB patrol route in world space (enemy.rs PatrolRoute) and where it is on it:
/// the exe keeps it in AiMovePointCtrl (AiThink +0x9060): the current point at +0x9078 and the
/// previous one at +0x907c (GetMovePointNumber FUN_1405e3910). It steps (FUN_1405afe20,
/// 1405a.c:11008) when a move goal to POINT_MOVE_POINT ends in success (Terminate callbacks
/// FUN_1405c7b80 / c8050 / ca9b0) or on Lua StepNextMovePoint: prev = cur, cur = the next point
/// by PatrolType (table 0x143af9550, FUN_1405c1f40 skips empty slots, 64 tries) or -1. Types:
/// 0 loop, 1 back and forth (turns at the last point), 2 random, 3 loop over slots 0-15, 4 the
/// nearest of slots 16-31, 5 on to the last point, then -1 (its pre-step hook FUN_1405c2710
/// ends the route there), 6 and up never move (cur -1). At the start prev = -1 and cur = the
/// first point (FUN_1405b06a0). With cur -1, NonBattleAct plays the last point's arrival anim
/// and stays (common_non_battle_act.lua). Type 5's hook at the last point passes that region's
/// position (+0x14) and rotation (+0x20) to the chr's vtable +0x168: FUN_1409f60c0 (1409f.c:3833)
/// sets the initial position / rotation (ChrIns +0xe0 / +0xf0) and flag +0x1a15 bit 3, which
/// IsChangeInitialPosition reads (FUN_1405e4fc0): the end of the route is its new home pose
/// (`new_home`; POINT_INITIAL / POINT_INIT_POSE and BackToHome follow it). Only 0 / 1 / 5
/// occur in the MSBs. gap: the points here are the used slots only, so types 3 / 4 (unused)
/// step over them all.
#[derive(Clone, Debug, Default)]
pub struct Patrol {
    pub kind: i64,
    pub points: Vec<Vec3>,
    /// Each point's MSB region index: the arrival entries match by region.
    pub regions: Vec<i64>,
    /// (region index, arrival anim id, arrival wait seconds): the MSB WREntries.
    pub arrivals: Vec<(i64, i64, f64)>,
    /// Each point's region facing as an actor yaw (empty: unknown).
    pub yaws: Vec<f32>,
    /// Each point's MSB region shape (None or empty: a point region, or unknown).
    pub areas: Vec<Option<crate::enemy::WakeRegion>>,
    /// The home pose a finished type 5 route left (its last point, facing its region).
    pub new_home: Option<(Vec3, f32)>,
    pub cur: i64,
    pub prev: i64,
    back: bool,
}

impl Patrol {
    /// `regions` empty: each point its own region (its index).
    pub fn new(kind: i64, points: Vec<Vec3>, regions: Vec<i64>, arrivals: Vec<(i64, i64, f64)>) -> Self {
        let regions = if regions.len() == points.len() { regions } else { (0..points.len() as i64).collect() };
        let cur = if points.is_empty() || !(0..=5).contains(&kind) { -1 } else { 0 };
        Self { kind, points, regions, arrivals, yaws: Vec::new(), areas: Vec::new(), new_home: None, cur, prev: -1, back: false }
    }

    /// Reached the current point: it becomes the previous one, the next is picked by the type
    /// (-1: the route is over).
    fn step(&mut self, rng: u64) {
        let n = self.points.len() as i64;
        if n == 0 || self.cur < 0 {
            return;
        }
        self.prev = self.cur;
        // Type 5's pre-step hook (FUN_1405c2710): leaving the last point ends the route there.
        if self.kind == 5 && self.cur + 1 >= n {
            let at = self.points[self.cur as usize];
            if let Some(&yaw) = self.yaws.get(self.cur as usize) {
                self.new_home = Some((at, yaw));
            }
        }
        self.cur = match self.kind {
            0 | 3 | 4 if n > 1 => (self.cur + 1) % n,
            1 if n > 1 => {
                if (self.back && self.cur == 0) || (!self.back && self.cur == n - 1) {
                    self.back = !self.back;
                }
                if self.back { self.cur - 1 } else { self.cur + 1 }
            }
            2 if n > 1 => ((self.cur as u64 + 1 + rng % (n as u64 - 1)) % n as u64) as i64,
            5 if self.cur + 1 < n => self.cur + 1,
            // Nowhere else to go (one point, the end of a type 5 route, type 6+): -1.
            _ => -1,
        };
    }

    /// Whether `feet` reached the current point: inside its region shape, as the exe's
    /// MoveToSomewhere(POINT_MOVE_POINT) update asks (FUN_1405caa00, 1405c.c:7420: IsInsideMovePoint
    /// FUN_1405e55c0 -> FUN_1405ae370 -> the shape test FUN_1409825b0, 14098.c:2274). None: no
    /// shape (a point region: the exe's test is exact equality, so only IsArrived, the caller's
    /// radius test, ends those).
    pub fn inside_current(&self, feet: Vec3) -> Option<bool> {
        let area = self.areas.get(usize::try_from(self.cur).ok()?)?.as_ref()?;
        Some(area.contains(feet))
    }

    /// Point `point`'s arrival (anim, wait), by its region.
    fn arrival(&self, point: i64) -> Option<(i64, f64)> {
        let region = *self.regions.get(usize::try_from(point).ok()?)?;
        self.arrivals.iter().find(|a| a.0 == region).map(|a| (a.1, a.2))
    }

    /// IsRouteEnd (FUN_1405e6250 -> FUN_1405c20c0, 1405c.c:1341): no point after the previous one.
    fn route_end(&self) -> bool {
        self.prev + 1 >= self.points.len() as i64
    }
}

fn ai_obj() -> Value {
    Value::Obj(Obj { kind: OBJ_AI, id: 0 })
}

fn goal_obj(id: u32) -> Value {
    Value::Obj(Obj { kind: OBJ_GOAL, id })
}

impl AiState {
    /// Does an observed SpEffect change raise the interrupt? Only on the character the script
    /// named: the Headless Ape's 510010_battle.lua:25 watches TARGET_EVENT (its partner ape)
    /// for 3510900, which its own a100_003007 scream (TAE AddSpEffect 3510900) also carries;
    /// fired from its own row it set Number 1 / Timer 5 = 14 s (Goal.Interrupt) and the battle
    /// goal's partner branch (Act24 backstep 400-500) ran all fight (boss fidelity 2026-10-15).
    /// A TARGET_EVENT watch (the partner chr the map set) is `observes_event`'s. gap: an ended
    /// SpEffect (`new` false) is not split by owner, any self / target watch counts.
    fn observes(&self, id: i64, s: &Snapshot, new: bool) -> bool {
        let Some(watch) = self.observed.get(&id) else { return false };
        let me = self.k.target_self as i64;
        let ene = |t: &i64| *t == self.k.target_ene_0 as i64 || *t == 21;
        if !new {
            return watch.iter().any(|t| *t == me || ene(t));
        }
        (watch.contains(&me) && s.sp_self.contains(&id)) || (watch.iter().any(ene) && s.sp_target.contains(&id))
    }

    fn watches_event(&self, id: i64) -> bool {
        self.observed.get(&id).is_some_and(|w| w.contains(&(self.k.target_event as i64)))
    }

    /// An observed SpEffect on the TARGET_EVENT character came on (`new`) or went: the Headless
    /// Ape's partner's 3510900 / 3510901 (510010_battle.lua:25-26).
    fn observes_event(&self, id: i64, new: bool) -> bool {
        self.watches_event(id) && self.w.event.as_ref().is_some_and(|e| if new { e.sp_new.contains(&id) } else { e.sp_gone.contains(&id) })
    }

    /// The TARGET_EVENT character when `id` names it and the map set one.
    fn event_of(&self, id: Option<&Value>) -> Option<&EventView> {
        if is(id, self.k.target_event) { self.w.event.as_ref() } else { None }
    }
}

fn key(v: Option<&Value>) -> i64 {
    v.and_then(Value::num).unwrap_or(0.0) as i64
}

/// Free walkable distance from the centre `from` along `dir` (unit, level), at most `dist`: the
/// first collision `ray` hits at centre height less `radius`, or the first 0.5 m step whose
/// `floor` (feet height under a centre) jumps more than map::STEP_HEIGHT up or down (a ledge
/// or a cliff face ends the navmesh there). Inferred: the exe walks the navmesh
/// (GetExistMeshOnLineDistSpecifyAngle), which we do not have.
pub(crate) fn mesh_line_dist(from: Vec3, dir: Vec3, dist: f32, radius: f32, floor: impl Fn(Vec3) -> f32, ray: impl Fn(Vec3, Vec3, f32) -> Option<f32>) -> f32 {
    const STEP: f32 = 0.5;
    let wall = ray(from, dir, dist + radius).map_or(dist, |h| (h - radius).clamp(0.0, dist));
    let half = from.y - floor(from);
    let mut last = floor(from);
    let mut s = 0.0;
    while s < wall {
        let next = (s + STEP).min(wall);
        let f = floor((from + dir * next).with_y(last + half));
        if (f - last).abs() > crate::terrain::STEP_HEIGHT {
            return s;
        }
        last = f;
        s = next;
    }
    wall
}

fn is(v: Option<&Value>, k: f64) -> bool {
    v.and_then(Value::num) == Some(k)
}

fn first(r: Vec<Value>) -> Value {
    r.into_iter().next().unwrap_or_default()
}

/// An observe slot (AddObserveArea / Region / ChrDmySphere / ChrDmyArea): the exe node's kind
/// byte 1-4; the target is a TARGET_* id.
#[derive(Clone, Debug)]
enum Observe {
    /// observer, target, AI_DIR, angle, dist
    Area(Value, Value, Value, f64, f64),
    /// target, MSB region id
    Region(Value, i64),
    /// target, dummy, radius
    DmySphere(Value, i16, f64),
    /// target, dummy, vertical angle, horizontal angle, far, near (back-off)
    DmyArea(Value, i16, f64, f64, f64, f64),
}

fn angle_in(center: f64, width: f64, a: f64) -> bool {
    ((a - center + 540.0).rem_euclid(360.0) - 180.0).abs() <= width / 2.0
}

impl AiState {
    fn warn_once(&mut self, key: String) {
        if self.warned.insert(key.clone()) {
            self.log.push(format!("stub: {key}"));
        }
    }

    fn goal(&self, id: u32) -> &Goal {
        &self.goals[&id]
    }

    fn goal_mut(&mut self, id: u32) -> &mut Goal {
        self.goals.get_mut(&id).expect("live goal")
    }

    fn new_goal(&mut self, kind: Option<i64>, life: f64, p: Vec<Value>) -> u32 {
        let id = self.next_goal;
        self.next_goal += 1;
        self.goals.insert(id, Goal {
            kind,
            life,
            p,
            subs: Vec::new(),
            started: false,
            terminated: false,
            t: 0.0,
            had_sub: false,
            num: HashMap::new(),
            timing: Vec::new(),
            parent_combo: false,
            parent_endure: false,
            ez: None,
            succ: 0.0,
            turn_time: 0.0,
            front: 0.0,
            phase: 0,
            busy: false,
            req: false,
            start_bearing: 0.0,
            life_end_success: false,
        });
        id
    }

    /// A goal whose life ran out while it still went on (FUN_1405d06c0, 1405d.c:1970-1985): a
    /// success only if it or a goal above it has SetLifeEndSuccess (FUN_1405d1b50 walks up),
    /// else a failure, which fails its parent and drops the queued subgoals (the scripts' own
    /// common_approach_lifeSuccess / common_Leave_lifeSuccess wrappers rely on it). gap: the
    /// other exception, FUN_140605ea0 (+0x7420), is not traced.
    fn life_end(&self, g: u32) -> i32 {
        let mut cur = Some(g);
        for _ in 0..64 {
            let Some(id) = cur else { break };
            if self.goals.get(&id).is_some_and(|x| x.life_end_success) {
                return SUCCESS;
            }
            cur = self.goals.iter().find(|(_, x)| x.subs.contains(&id)).map(|(k, _)| *k);
        }
        FAILED
    }

    fn life_over(&self, g: u32) -> bool {
        let goal = self.goal(g);
        goal.life >= 0.0 && goal.t >= goal.life
    }

    /// `ai:AddTopGoal` / `goal:AddSubGoal` arguments: (id, life, params...).
    fn goal_from_args(&mut self, a: &[Value]) -> u32 {
        let kind = a.first().and_then(Value::num).map(|n| n as i64);
        let life = a.get(1).and_then(Value::num).unwrap_or(-1.0);
        let p = a.iter().skip(2).cloned().collect();
        self.new_goal(kind, life, p)
    }

    fn def(&self, kind: Option<i64>) -> Option<&Def> {
        kind.and_then(|k| self.defs.get(&k))
    }

    fn random(&mut self) -> f64 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng >> 11) as f64 / (1u64 << 53) as f64
    }

    fn can_act(&self, flag: i64) -> bool {
        self.w.free || self.w.flags.contains(&flag)
    }

    fn dist(&self) -> f64 {
        self.w.dist as f64
    }

    fn angle(&self) -> f64 {
        self.w.angle as f64
    }

    /// World position of a target / point id; None = the current target (TARGET_ENE_0), whose
    /// distance and angle come with the snapshot.
    fn pos_of(&self, id: Option<&Value>) -> Option<Vec3> {
        let k = &self.k;
        let w = &self.w;
        let id = id.and_then(Value::num)?;
        if id == k.target_self {
            Some(w.me)
        } else if id == k.target_event && w.event.is_some() {
            w.event.as_ref().map(|e| e.pos)
        } else if id == k.point_move_point && self.patrol.as_ref().is_some_and(|p| p.cur >= 0) {
            let p = self.patrol.as_ref()?;
            p.points.get(p.cur as usize).copied()
        } else if id == k.point_event && self.event_target.is_some() {
            self.event_target
        } else if [k.point_initial, k.point_move_point, k.point_back_to_home, k.point_event, k.team_formation].contains(&id) {
            // No patrol route or event points without a map: they all mean home.
            Some(w.home)
        } else if id == k.point_init_pose {
            // A pose: home, facing its initial direction.
            Some(w.home + w.home_fwd * 10.0)
        } else {
            None
        }
    }

    fn dist_to(&self, id: Option<&Value>) -> f64 {
        // A character's distance is 3D, as the current target's (`Snapshot::dist`).
        if let Some(e) = self.event_of(id) {
            return (e.pos - self.w.me).length() as f64;
        }
        match self.pos_of(id) {
            Some(_) if is(id, self.k.point_init_pose) => (self.w.home - self.w.me).with_y(0.0).length() as f64,
            Some(p) => (p - self.w.me).with_y(0.0).length() as f64,
            None => self.dist(),
        }
    }

    /// The distance the native goals compare their parameters against: GetDist's, minus the AI
    /// chr's own radius. CommonAttack's successDist (FUN_1405c6d30, calls at 0x1405c7073 /
    /// 0x1405c70cd), LeaveTarget (0x1405c80d0 -> 0x1405c8163) and KeepDist (0x1405c7c40 ->
    /// 0x1405c7cb9) all call the GetDist thunk 0x1405e2c10 (r8b 1); the scripts' distances are
    /// in the same terms. gap: MoveToSomewhere's arrival test (successDist) is not a distance
    /// thunk call (its move subgoals 0x1405caa00 / 0x1405c9df0 are not traced); read the same.
    fn goal_dist(&self, id: Option<&Value>) -> f64 {
        self.dist_to(id) - self.hit_radius as f64
    }

    /// Signed angle (deg) from the enemy's facing to a target / point, positive to the right.
    fn angle_to(&self, id: Option<&Value>) -> f64 {
        match self.pos_of(id) {
            Some(p) => {
                let d = (p - self.w.me).with_y(0.0).normalize_or_zero();
                let fwd = self.w.fwd.with_y(0.0).normalize_or_zero();
                d.dot(fwd.cross(Vec3::Y)).atan2(d.dot(fwd)).to_degrees() as f64
            }
            None => self.angle(),
        }
    }

    /// Position and facing of an observer / target id: TARGET_SELF = the AI chr, a point = its
    /// spot (no facing), anything else = the current target (TARGET_ENE_0).
    fn frame_of(&self, id: Option<&Value>) -> (Vec3, Vec3) {
        if is(id, self.k.target_self) {
            return (self.w.me, self.w.fwd);
        }
        if let Some(e) = self.event_of(id) {
            return (e.pos, e.fwd);
        }
        match self.pos_of(id) {
            Some(p) => (p, Vec3::ZERO),
            None => (self.w.target, self.w.target_fwd),
        }
    }

    fn inside_ex(&self, observer: Option<&Value>, target: Option<&Value>, dir: Option<&Value>, width: f64, range: f64) -> bool {
        let (o, fwd) = self.frame_of(observer);
        let (t, _) = self.frame_of(target);
        let d = (t - o).with_y(0.0);
        let fwd = fwd.with_y(0.0).normalize_or_zero();
        if fwd == Vec3::ZERO {
            // gap: an observer without a facing (a point, a caution spot); only the range counts.
            return d.length() as f64 <= range;
        }
        let d = d.normalize_or_zero();
        let angle = d.dot(fwd.cross(Vec3::Y)).atan2(d.dot(fwd)).to_degrees() as f64;
        angle_in(self.dir_center(dir), width, angle) && (t - o).with_y(0.0).length() as f64 <= range
    }

    fn dir_center(&self, dir: Option<&Value>) -> f64 {
        if is(dir, self.k.dir_b) {
            180.0
        } else if is(dir, self.k.dir_l) {
            -90.0
        } else if is(dir, self.k.dir_r) {
            90.0
        } else {
            0.0
        }
    }

    /// An observe slot's target inside it now.
    fn observe_inside(&self, slot: i64) -> bool {
        let w = &self.w;
        let dmy = |id: i16| w.dummies.iter().find(|d| d.0 == id).map(|d| (d.1, d.2));
        self.observe.iter().find(|o| o.0 == slot).is_some_and(|(_, o)| match o {
            // The target within `dist` (3D) of the observer and in its horizontal cone (full
            // angle `ang`) about the AI_DIR.
            Observe::Area(ob, t, d, ang, dist) => {
                let (o, _) = self.frame_of(Some(ob));
                let (at, _) = self.frame_of(Some(t));
                (at - o).length() as f64 <= *dist && self.inside_ex(Some(ob), Some(t), Some(d), *ang, f64::INFINITY)
            }
            Observe::Region(t, region) => {
                let at = self.pos_of(Some(t)).unwrap_or(w.target);
                w.regions.as_ref().and_then(|r| r.get(region)).is_some_and(|r| r.contains(at))
            }
            // AddObserveChrDmySphere(slot, target, dmy, radius): the target strictly within the
            // radius of the AI chr's own dummy.
            Observe::DmySphere(t, id, r) => {
                let (at, _) = self.frame_of(Some(t));
                dmy(*id).is_some_and(|(p, _)| ((at - p).length() as f64) < *r)
            }
            // AddObserveChrDmyArea(slot, target, dmy, A, B, C, D) (SprjAiChrDmyObserveArea,
            // FUN_140606710 -> FUN_140605ee0, 14060.c:5023-5234): from the dummy moved back D
            // along its +Z, the target chr's position farther than D and within C (3D); the
            // horizontal projections within B / 2 of each other, and the elevation within A / 2
            // of the axis's.
            Observe::DmyArea(t, id, va, ha, far, near) => {
                let (at, _) = self.frame_of(Some(t));
                dmy(*id).is_some_and(|(p, axis)| {
                    let origin = p - axis * *near as f32;
                    let to = at - origin;
                    let dist = to.length() as f64;
                    let (hd, hx) = (to.with_y(0.0).normalize_or_zero(), axis.with_y(0.0).normalize_or_zero());
                    let h = hd.dot(hx.cross(Vec3::Y)).atan2(hd.dot(hx)).to_degrees() as f64;
                    let elev = |v: Vec3| v.y.atan2(v.with_y(0.0).length()).to_degrees() as f64;
                    dist > *near && dist <= *far && angle_in(0.0, *ha, h) && (elev(to) - elev(axis)).abs() <= *va / 2.0
                })
            }
        })
    }

    /// The AI chr's dummy polys the observe slots read (enemy.rs fills Snapshot::dummies).
    fn observe_dmys(&self) -> Vec<i16> {
        let mut v: Vec<i16> = self
            .observe
            .iter()
            .filter_map(|(_, o)| match o {
                Observe::DmySphere(_, d, _) | Observe::DmyArea(_, d, ..) => Some(*d),
                _ => None,
            })
            .collect();
        v.sort_unstable();
        v.dedup();
        v
    }

    /// Adds or replaces an observe slot: the same id of the same kind keeps its place and its
    /// inside state (a re-add every think leaves the edges alone); another kind starts outside.
    fn add_observe(&mut self, slot: i64, o: Observe) {
        match self.observe.iter_mut().find(|x| x.0 == slot) {
            Some(x) => {
                if std::mem::discriminant(&x.1) != std::mem::discriminant(&o) {
                    self.observe_in.remove(&slot);
                }
                x.1 = o;
            }
            None => self.observe.push((slot, o)),
        }
    }

    /// This frame's observe-slot changes against last frame's (a fresh slot starts outside:
    /// the node's prev byte is 0, so a target already inside raises the enter on its first
    /// update).
    fn observe_edges(&mut self) {
        let slots: Vec<i64> = self.observe.iter().map(|o| o.0).collect();
        let now: HashSet<i64> = slots.iter().copied().filter(|&s| self.observe_inside(s)).collect();
        let went_in = slots.iter().copied().filter(|s| now.contains(s) && !self.observe_in.contains(s)).collect();
        let went_out = slots.iter().copied().filter(|s| !now.contains(s) && self.observe_in.contains(s)).collect();
        self.observe_in = now;
        self.observe_changed = (went_in, went_out);
    }

    /// This frame's trigger-region entries and exits (FUN_140642de0, 14064.c:2269): every live
    /// region of an observed category holding this chr, but not its own; one that was not held
    /// last frame is an entry, one held then and not now (left or expired) an exit. Up to 16.
    fn trigger_edges(&mut self) {
        let w = &self.w;
        let now: Vec<(u64, i64, i64)> = w
            .triggers
            .iter()
            .filter(|r| r.owner != w.me_id && self.trigger_observers.contains(&r.cat) && r.contains(w.me))
            .map(|r| (r.owner, r.cat, r.unit))
            .take(16)
            .collect();
        let went_in = now.iter().filter(|k| !self.trigger_in.contains(k)).map(|k| (k.1, k.2)).collect();
        let went_out = self.trigger_in.iter().filter(|k| !now.contains(k)).map(|k| (k.1, k.2)).collect();
        self.trigger_in = now;
        self.trigger_changed = (went_in, went_out);
    }

    fn sp_of(&self, target: Option<&Value>) -> &HashSet<i64> {
        if is(target, self.k.target_self) {
            &self.w.sp_self
        } else if let Some(e) = self.event_of(target) {
            &e.sp
        } else {
            &self.w.sp_target
        }
    }

    fn apply_timing(&mut self, g: u32, when: f64) {
        let Some(goal) = self.goals.get(&g) else { return };
        let due: Vec<(bool, i64, Value)> = goal
            .timing
            .iter()
            .filter(|t| t.when == when)
            .map(|t| (t.timer, key(Some(&t.id)), t.v.clone()))
            .collect();
        for (timer, id, v) in due {
            if timer {
                self.timers.insert(id, v.num().unwrap_or(0.0));
            } else {
                self.numbers.insert(id, v);
            }
        }
    }

    fn active_ids(&self, g: u32, out: &mut HashSet<i64>) {
        let Some(goal) = self.goals.get(&g) else { return };
        if let Some(k) = goal.kind {
            out.insert(k);
        }
        for &s in &goal.subs {
            self.active_ids(s, out);
        }
    }

    /// Drops the goals no longer reachable from the root.
    fn sweep(&mut self) {
        let mut keep = HashSet::new();
        let mut stack: Vec<u32> = self.root.into_iter().chain(self.tops.iter().copied()).chain(self.top_queue.iter().copied()).collect();
        while let Some(g) = stack.pop() {
            if keep.insert(g) {
                if let Some(goal) = self.goals.get(&g) {
                    stack.extend(&goal.subs);
                }
            }
        }
        self.goals.retain(|id, _| keep.contains(id));
    }

    fn goal_method(&mut self, id: u32, name: &str, a: &[Value]) -> Vec<Value> {
        if !self.goals.contains_key(&id) {
            return vec![Value::Num(0.0)];
        }
        let n = |i: usize| a.get(i).and_then(Value::num).unwrap_or(0.0);
        match name {
            "GetParam" => vec![self.goal(id).param(n(0) as usize)],
            // The life left (scripts end themselves on `GetLife() <= 0`; read at Activate it is
            // the whole life). An endless one (-1) stays -1.
            "GetLife" => {
                let g = self.goal(id);
                vec![Value::Num(if g.life >= 0.0 { g.life - g.t } else { g.life })]
            }
            "GetLifeRemain" => vec![Value::Num(self.goal(id).life - self.goal(id).t)],
            "GetSubGoalNum" => vec![Value::Num(self.goal(id).subs.len() as f64)],
            "SetNumber" => {
                let v = a.get(1).cloned().unwrap_or_default();
                self.goal_mut(id).num.insert(n(0) as i64, v);
                vec![]
            }
            "GetNumber" => vec![self.goal(id).num.get(&(n(0) as i64)).cloned().unwrap_or(Value::Num(0.0))],
            "GetBattleGoalId" => vec![Value::Num(self.battle_goal as f64)],
            "IsExistParam" => vec![Value::Bool(self.goal(id).p.get(n(0) as usize).is_some_and(|v| !v.is_nil()))],
            // GOAL_RESULT_Continue: the patrol-route branches that read it are not reached.
            "GetLastResult" => vec![Value::Num(0.0)],
            // The setters return the goal: scripts chain them, e.g. 500000_battle.lua Act27
            // AddSubGoal(...):SetLifeEndSuccess(true):TimingSetNumber(3, 1, ...).
            "SetLifeEndSuccess" => {
                self.goal_mut(id).life_end_success = !matches!(a.first(), Some(Value::Bool(false)));
                vec![goal_obj(id)]
            }
            "AddGoalScopedTeamRecord" | "SetFailedEndOption" | "SetManagementGoal" => vec![goal_obj(id)],
            "SetTargetRange" => vec![goal_obj(id)],
            "AddSubGoal" | "AddSubGoal_Front" => {
                let combo = self.def(self.goal(id).kind).is_some_and(|d| d.combo);
                let endure = self.def(self.goal(id).kind).is_some_and(|d| d.name.as_deref().is_some_and(|n| n.ends_with("EndureAttack")));
                let g = self.goal_from_args(a);
                self.goal_mut(g).parent_combo = combo;
                self.goal_mut(g).parent_endure = endure;
                let parent = self.goal_mut(id);
                parent.had_sub = true;
                if name == "AddSubGoal" {
                    parent.subs.push(g);
                } else {
                    parent.subs.insert(0, g);
                }
                vec![goal_obj(g)]
            }
            "TimingSetTimer" | "TimingSetNumber" => {
                // The scripts pass an undefined UPDATE_SUCCESS (nil) for "on success".
                let when = a.get(2).and_then(Value::num).unwrap_or(self.k.timing_success);
                let t = Timing {
                    timer: name == "TimingSetTimer",
                    id: a.first().cloned().unwrap_or_default(),
                    v: a.get(1).cloned().unwrap_or_default(),
                    when,
                };
                self.goal_mut(id).timing.push(t);
                vec![goal_obj(id)]
            }
            _ => {
                self.warn_once(format!("goal:{name}"));
                vec![Value::Num(0.0)]
            }
        }
    }

    fn ai_method(&mut self, name: &str, a: &[Value]) -> Vec<Value> {
        let r = self.ai_method_raw(name, a);
        // Debug: SHINOBI_AI_CALLS=1 logs every `ai` call the scripts make (numbers / booleans).
        static CALLS: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        if *CALLS.get_or_init(|| std::env::var("SHINOBI_AI_CALLS").is_ok()) {
            let show = |v: &[Value]| v.iter().map(|v| match v { Value::Num(x) => format!("{x}"), Value::Bool(b) => format!("{b}"), _ => "_".into() }).collect::<Vec<_>>().join(",");
            eprintln!("ai {name}({}) -> {}", show(a), show(&r));
        }
        r
    }

    fn ai_method_raw(&mut self, name: &str, a: &[Value]) -> Vec<Value> {
        let n = |i: usize| a.get(i).and_then(Value::num).unwrap_or(0.0);
        let num = |v: f64| vec![Value::Num(v)];
        let boolean = |b: bool| vec![Value::Bool(b)];
        let w = &self.w;
        match name {
            // GetDist / GetDist_Point (thunks 1405e2c10 / 1405e2fe0: `mov r8b,1; jmp FUN_1405f1540`):
            // the 3D distance to the target minus the AI chr's own radius (vtable +0x68 of its chr,
            // the radius FUN_1405bb920 also pushes its own probe out by). GetOriginDist (1405e3fb0,
            // r8 = 0) is the plain distance. 3D for the target (Snapshot::dist); gap: points
            // (dist_to with an id) are still horizontal, the radius field +0x68 reads is not
            // traced (NpcParam.hitRadius here), and TARGET_LOCALPLAYER (21) reads 0.4 in one
            // player state (== 2 at singleton +0x88) in the exe.
            "GetDist" | "GetDist_Point" => num(self.dist_to(a.first()) - self.hit_radius as f64),
            "GetOriginDist" => num(self.dist_to(a.first())),
            // Height of the target over the AI chr (positive = above): 101030 Act27 backs off to
            // height / tan when Wolf is >= 3 m up, 106000 / 118000 pick acts on |dy| > 1.8 via the
            // signed GetDistY. Both read 0 before (2026-10-12 round 22). gap: whether GetDistY is
            // the absolute value in the exe is not traced; 106000 compares it with -1.8, so signed.
            "GetDistYSigned" | "GetDistY" => num((self.frame_of(a.first()).0.y - self.w.me.y) as f64),
            // Between the two named (the Old Dragons' parts: TARGET_EVENT to TARGET_ENE_0, 530010
            // Act); one not modelled (TARGET_TEAM_LEADER) reads as the AI chr to its target.
            "GetDistAtoB" => {
                let known = |v: Option<&Value>| is(v, self.k.target_ene_0) || self.pos_of(v).is_some();
                num(if known(a.first()) && known(a.get(1)) { (self.frame_of(a.first()).0 - self.frame_of(a.get(1)).0).length() as f64 } else { self.dist() })
            }
            "GetMapHitRadius" => num(self.hit_radius as f64),
            "GetRandam_Int" => {
                let (lo, hi) = (n(0).floor(), n(1).floor());
                // Empty range (all act weights 0): the minimum, so no act is picked.
                if hi < lo {
                    return num(lo);
                }
                let r = self.random();
                num((lo + (r * (hi - lo + 1.0)).floor()).min(hi))
            }
            "GetRandam_Float" => {
                let r = self.random();
                num(n(0) + (n(1) - n(0)) * r)
            }
            "HasSpecialEffectId" => boolean(self.sp_of(a.first()).contains(&key(a.get(1)))),
            "HasSpecialEffectAttribute" => boolean(false),
            // By their target argument, as GetHpRate (TARGET_SELF: the enemy itself; Common_Parry,
            // common_common_func_NTC.lua:380, asks IsTargetGuard(TARGET_SELF); 140010_battle.lua
            // compares GetSpRate(TARGET_SELF) with (TARGET_ENE_0)).
            "IsTargetGuard" => boolean(if is(a.first(), self.k.target_self) { w.self_guard } else { w.target_guard }),
            "GetSp" => num(if is(a.first(), self.k.target_self) { w.sp } else { self.event_of(a.first()).map_or(w.target_sp, |e| e.sp_left) } as f64),
            "GetSpRate" => num(if is(a.first(), self.k.target_self) { w.sp_rate } else { self.event_of(a.first()).map_or(w.target_sp_rate, |e| e.sp_rate) } as f64),
            "GetHpRate" => num(if is(a.first(), self.k.target_self) { w.hp_rate } else { self.event_of(a.first()).map_or(w.target_hp_rate, |e| e.hp_rate) } as f64),
            "GetHp" => num(1.0),
            // The General (102000_battle.lua) goes all out on his last one (<= 1).
            "GetNinsatsuNum" => num(w.ninsatsu.max(1) as f64),
            // Bosses compare it with the left count: fewer left = a later phase (500000_battle.lua).
            "GetNinsatsuMaxNum" => num(w.ninsatsu_max.max(1) as f64),
            "GetNpcThinkParamID" => num(self.think_id as f64),
            // NpcThinkParam fields by AI_EXCEL_THINK_PARAM_TYPE__<field name>.
            "GetExcelParam" => {
                let field = self.excel.get(&key(a.first())).cloned().unwrap_or_default();
                num(self.think[field.as_str()].as_f64().unwrap_or(0.0))
            }
            // Event requests default to -1 (FUN_1405b2090 fills the slots with -1.0); the map's
            // Request Character AI Command sets one (the Demon of Hatred's battle reads 10 / 20).
            // The 4 slots (AI +0x743c) are read as they are (FUN_1405b36b0) and written by the
            // command (FUN_1405b4730, which also sets a received bit that FUN_1405b4f10 keeps for
            // one update: IsEventRequestReceived, used by no script); only the whole AI reset
            // (FUN_140602020 -> FUN_1405b2520, all -1) clears them - here a respawn's new Enemy.
            "GetEventRequest" => {
                let slot = a.first().and_then(|v| v.num()).unwrap_or(0.0) as i64;
                num(w.event_req.iter().find(|r| r.0 == slot).map_or(-1.0, |r| r.1 as f64))
            }
            // A boolean: the scripts test it bare (`if ai:IsEventFlag(12505901) then`, the Okami
            // warriors' 530000_battle.lua) as well as `== true`. Flags the boss script does not
            // run are off.
            "IsEventFlag" => boolean(w.event_flags.contains(&key(a.first()))),
            // The game's event flags are world-wide: the Okami's leader (504000_battle.lua
            // 19625680-93) and Lady Butterfly (509000_battle.lua 19952081/2, read by her
            // illusions' 151010_battle.lua) signal other characters with them. enemy.rs writes
            // them to the fight (the boss script's flags, or AiFlags); this AI sees its own write
            // at once.
            "SetEventFlag" => {
                let (id, on) = (key(a.first()), a.get(1).is_some_and(Value::truthy));
                self.w.event_flags.retain(|f| *f != id);
                if on {
                    self.w.event_flags.push(id);
                }
                self.cmd.flags.push((id, on));
                vec![]
            }
            // FUN_1405b1b20 (1405b.c:588): the order float of type 0 (ORDER_TYPE_Role) / 1
            // (CallHelp, gap: its writers are not traced, 0 here), else -1.
            "GetTeamOrder" => num(match n(0) as i64 {
                0 => w.team_role,
                1 => 0.0,
                _ => -1.0,
            }),
            "GetOddsParam" | "GetOddsParamIdOffset" | "DbgGetForceActIdx" | "DbgGetForceKengekiActIdx"
            | "GetChangeBattleStateCount" | "GetSmallActPreWaitTime" | "GetSmallActPostWaitTime" => num(0.0),
            // The MSB patrol route (Patrol): the current / previous point and a point's arrival
            // anim and wait (WREntry); none -> -1, and NonBattleAct stays at home.
            "GetMovePointNumber" => num(self.patrol.as_ref().map_or(-1, |p| p.cur) as f64),
            "GetPrevMovePointNumber" => num(self.patrol.as_ref().map_or(-1, |p| p.prev) as f64),
            "GetMovePointAnimId" => num(self.patrol.as_ref().and_then(|p| p.arrival(n(0) as i64)).map_or(-1, |a| a.0) as f64),
            "GetMovePointWaitTime" => num(self.patrol.as_ref().and_then(|p| p.arrival(n(0) as i64)).map_or(0.0, |a| a.1)),
            "GetMovePointType" => num(self.patrol.as_ref().map_or(0, |p| p.kind) as f64),
            "IsRouteEnd" => vec![Value::Bool(self.patrol.as_ref().is_none_or(Patrol::route_end))],
            "GetSmallActAnimId" => num(-1.0),
            // gap: the engine's move-point effect range; here the distance from home, which is what
            // NonBattleAct / _COMMON_SetBattleActLogic compare with the back-home distances.
            "GetMovePointEffectRange" => num((w.home - w.me).with_y(0.0).length() as f64),
            "GetActTypeOnNonBattleFailedPathEnd" => num(self.think["actTypeOnNonBtlFailedPath"].as_f64().unwrap_or(0.0)),
            "GetToTargetAngle" => num(self.angle_to(a.first())),
            "IsInsideTarget" => boolean(angle_in(self.dir_center(a.get(1)), n(2), self.angle_to(a.first()))),
            // IsInsideTargetEx(observer, target, dir, angle, dist): is `target` inside the
            // observer's dir cone and range. The scripts use both orders: (TARGET_ENE_0,
            // TARGET_SELF, ...) = "am I in front of the player" (101200 / 125000 parry gates,
            // 61 sidestep side picks), (TARGET_SELF, POINT_EVENT, ...) = from the AI chr.
            "IsInsideTargetEx" => boolean(self.inside_ex(a.first(), a.get(1), a.get(2), n(3), n(4))),
            // The target system (stealth.rs).
            "IsBattleState" => boolean(w.t_state == 3),
            "IsFindState" => boolean(w.t_state == 2),
            "IsCautionState" => boolean(w.t_state == 1),
            "IsChangeState" => boolean(w.t_changed),
            "GetPrevTargetState" => num(w.t_prev as f64),
            "GetCurrTargetType" => num(w.t_kind as f64),
            "IsSearchTarget" => boolean(w.t_kind != 0),
            "IsVisibleCurrTarget" | "IsVisibleTarget" => boolean(w.t_kind == 3 && w.visible),
            "GetTopNormalEnemyForgettingTime" => num(w.forgetting as f64),
            "GetLatestSoundTargetID" => num(w.sound_id as f64),
            // The sound record's serial (FUN_1405e3680: entry +0x58, the global counter of
            // FUN_14060f840), -1 (0xFFFFFFFF) without a sound target.
            "GetLatestSoundTargetInstanceID" => num(if w.sound_id > 0 { w.sound_serial as f64 } else { -1.0 }),
            "GetLatestSoundTargetRank" => num(w.sound_rank as f64),
            "ClearEnemyTarget" => {
                self.cmd.clear |= CLEAR_ENEMY;
                vec![]
            }
            "ClearSoundTarget" => {
                self.cmd.clear |= CLEAR_SOUND;
                vec![]
            }
            "ClearIndicationPosTarget" => {
                self.cmd.clear |= CLEAR_INDICATION;
                vec![]
            }
            "ClearLastMemoryTargetPos" => {
                self.cmd.clear |= CLEAR_MEMORY;
                vec![]
            }
            // IsInsideTargetRegion(target, region): the target's feet in the MSB region with that
            // entity id (FUN_1406068f0 -> FUN_140678380: the id's region from the tree
            // FUN_1406d35d0, then its shape test). An id no MSB has is never inside: the Folding
            // Screen Monkeys' 99028xx checks (126000_battle.lua Act17) all fall through to their
            // 20028xx twins.
            "IsInsideTargetRegion" => {
                let at = self.pos_of(a.first()).unwrap_or(w.target);
                let id = n(1) as i64;
                boolean(w.regions.as_ref().and_then(|r| r.get(&id)).is_some_and(|r| r.contains(at)))
            }
            // SetEventMoveTarget(region): POINT_EVENT becomes the region's spot (126000 Act17
            // walks there). An unknown id leaves it as it was.
            "SetEventMoveTarget" => {
                if let Some(r) = w.regions.as_ref().and_then(|r| r.get(&(n(0) as i64))) {
                    self.event_target = Some(r.at);
                }
                vec![]
            }
            // AddObserveArea(slot, observer, target, dir, angle, dist) / IsInsideObserve(slot): the
            // target in the observer's cone (102000 / 136000 pick their acts by it), read live each
            // call; the slot list and its Inside / Outside edges as the exe's (`Observe`,
            // add_observe, observe_edges).
            "AddObserveArea" => {
                let v = |i: usize| a.get(i).cloned().unwrap_or_default();
                self.add_observe(n(0) as i64, Observe::Area(v(1), v(2), v(3), n(4), n(5)));
                vec![]
            }
            // AddObserveRegion(slot, target, region): the target inside the MSB region (the castle's
            // force-walk regions 101000 / 145000 slot 30, 106000 / 121010 / 145000 region slots).
            "AddObserveRegion" => {
                self.add_observe(n(0) as i64, Observe::Region(a.get(1).cloned().unwrap_or_default(), n(2) as i64));
                vec![]
            }
            // The dummy-poly slots (the Okami 504000's eight spheres, the Orochi sight sphere,
            // the common battle-stealth slot 101: dmy 7, 90 / 120 deg, 30 m, 4 m back).
            "AddObserveChrDmySphere" => {
                self.add_observe(n(0) as i64, Observe::DmySphere(a.get(1).cloned().unwrap_or_default(), n(2) as i16, n(3)));
                vec![]
            }
            "AddObserveChrDmyArea" => {
                let t = a.get(1).cloned().unwrap_or_default();
                self.add_observe(n(0) as i64, Observe::DmyArea(t, n(2) as i16, n(3), n(4), n(5), n(6)));
                vec![]
            }
            // Removed without an edge (no Outside interrupt).
            "DeleteObserve" => {
                let slot = n(0) as i64;
                self.observe.retain(|o| o.0 != slot);
                self.observe_in.remove(&slot);
                vec![]
            }
            "IsInsideObserve" => boolean(self.observe_inside(n(0) as i64)),
            // RegisterTriggerRegion(category, unit, life s, radius m, target, dir, extra): a sphere
            // at the target's position + its dir vector x |hit radius + extra| (FUN_14060c250,
            // 14060.c:6625-6696), stored for every AI by enemy.rs. The caution logic leaves one
            // where it heard a sound (common_common_logic_func.lua:319 / 348), tagged with the
            // sound's serial, so allies that heard the same sound stop on reaching it.
            "RegisterTriggerRegion" => {
                let (o, fwd) = self.frame_of(a.get(4));
                let fwd = fwd.with_y(0.0).normalize_or_zero();
                let yaw = (self.dir_center(a.get(5)) as f32).to_radians();
                // The dir's angle as IsInsideTargetEx reads it (positive = fwd x Y).
                let dir = fwd * yaw.cos() + fwd.cross(Vec3::Y) * yaw.sin();
                let reach = if is(a.get(4), self.k.target_self) { self.hit_radius } else { 0.0 };
                let at = o + dir * (reach + n(6) as f32).abs();
                self.cmd.triggers.push(TriggerRegion { owner: w.me_id, cat: n(0) as i64, unit: n(1) as i64, at, radius: n(3) as f32, life: n(2) as f32 });
                vec![]
            }
            "RegisterTriggerRegionObserver" => {
                let cat = n(0) as i64;
                if !self.trigger_observers.contains(&cat) && self.trigger_observers.len() < 16 {
                    self.trigger_observers.push(cat);
                }
                vec![]
            }
            // FUN_1405e75f0 (1405e.c:5952) passes a literal 1 to the removal (FUN_140642ba0), not
            // the category: RemoveTriggerRegionObserver(1000) leaves observer 1000 in place.
            // gap: read from the decompile, not checked in the disassembly.
            "RemoveTriggerRegionObserver" => {
                self.trigger_observers.retain(|&c| c != 1);
                vec![]
            }
            // The per-frame lists (AI +0x7460, FUN_1405b37f0 / 36d0 / 3800 and the Leave ones).
            "GetInvadeTriggerRegionInfoNum" => num(self.trigger_changed.0.len() as f64),
            "GetLeaveTriggerRegionInfoNum" => num(self.trigger_changed.1.len() as f64),
            "GetInvadeTriggerRegionCategory" | "GetInvadeTriggerRegionUnitID" | "GetLeaveTriggerRegionCategory" | "GetLeaveTriggerRegionUnitID" => {
                let list = if name.starts_with("GetInvade") { &self.trigger_changed.0 } else { &self.trigger_changed.1 };
                let e = list.get(n(0) as usize).copied().unwrap_or((0, 0));
                num(if name.ends_with("Category") { e.0 } else { e.1 } as f64)
            }
            // (type 1 inside / 2 outside, index): the index-th slot that changed this frame.
            "GetAreaObserveSlotNum" => {
                let c = &self.observe_changed;
                num(match n(0) as i64 { 1 => c.0.len(), 2 => c.1.len(), _ => 0 } as f64)
            }
            "GetAreaObserveSlot" => {
                let c = &self.observe_changed;
                let list = match n(0) as i64 { 1 => &c.0, 2 => &c.1, _ => &c.0 };
                match list.get(n(1) as usize) {
                    Some(&slot) if n(0) as i64 != 0 => num(slot as f64),
                    _ => boolean(false),
                }
            }
            "IsLadderAct" | "IsStartAttack"
            | "IsValidPlatoon" | "IsPlatoonLeader" | "TeamHelp_IsValidReply" | "IsForceBattleGoal" => boolean(false),
            // IsLockOnTarget(locker, target): the player locked on to the AI chr (136000_battle.lua:116
            // keeps its distance then, 115000_battle.lua:281, 101000_logic.lua:6); no NPC locks on.
            "IsLockOnTarget" => boolean(!is(a.first(), self.k.target_self) && is(a.get(1), self.k.target_self) && w.locked_on),
            "IsChangeInitialPosition" => boolean(self.patrol.as_ref().is_some_and(|p| p.new_home.is_some())),
            // CheckDoesExistPath too: the fallback's number 0 is neither `== true` nor `== false` in Lua
            // (126000 Act16, 720000 approach branches never ran). gap: no navmesh, so a path always
            // exists; 101000 / 136000_battle.lua's no-path branch (acts 7 / 27) still runs for Wolf
            // hanging or perched through its SpEffect twins 109220 / 109221.
            "IsExistMeshOnLine" | "CheckDoesExistPath" | "CheckDoesExistPathWithSetPoint" | "IsFinishAttackCoolTime" => boolean(true),
            // (from, angle, dist, AI_SPA_DIR_TYPE[, radius, _]): how far the walkable ground goes from
            // `from` along the angle off the direction to the target (TargetF / ToTarget; Absolute
            // off +z). SpaceCheck (common_common_func_FDP.lua:43-50) compares it with 0.95 x the
            // step / LeaveTarget distance. gap: no navmesh; the map collision stands in
            // (mesh_line_dist), and without a map the whole distance is free.
            "GetExistMeshOnLineDistSpecifyAngleEx" | "GetExistMeshOnLineDistSpecifyAngle" => {
                let (from, fwd) = self.frame_of(a.first());
                let to_t = (self.w.target - from).with_y(0.0).normalize_or_zero();
                let base = if n(3) == 2.0 { Vec3::Z } else if to_t != Vec3::ZERO { to_t } else { fwd.with_y(0.0).normalize_or_zero() };
                let ang = (n(1) as f32).to_radians();
                let dir = (base * ang.cos() + base.cross(Vec3::Y) * ang.sin()).normalize_or_zero();
                let radius = if name.ends_with("Ex") { n(4) as f32 } else { 0.0 };
                let dist = n(2) as f32;
                let free = if dir == Vec3::ZERO {
                    dist
                } else {
                    let floor = |c: Vec3| crate::terrain::ground_y(c);
                    mesh_line_dist(from, dir, dist, radius, floor, crate::terrain::raycast)
                };
                num(free as f64)
            }
            "IsLookToTarget" => {
                let width = if n(0) > 0.0 { n(0) } else { 20.0 };
                let id = self.turn_id.clone();
                boolean(angle_in(0.0, width, self.angle_to(id.as_ref())))
            }
            "TurnTo" => {
                self.cmd.turn = true;
                self.cmd.at = self.pos_of(a.first());
                self.turn_id = a.first().cloned();
                vec![]
            }
            "TeamHelp_ValidateCall" | "TeamHelp_ValidateReply" | "ClearForceBattleGoal" | "ReqPlatoonState"
            | "SetEnableEndureCancel_forGoal"
            | "ClearCorpsePosTarget" => vec![],
            "RequestEmergencyQuickTurn" => {
                self.cmd.emergency = true;
                vec![]
            }
            "SetAIPredictionMoveTargetSpecifyTargetDir" | "PrintText"
            | "DbgSetLastActIdx" | "DbgSetLastKengekiActIdx" => vec![],
            "GetEzStateAnimId" => vec![a.first().cloned().unwrap_or_default()],
            "DoEzAction" => {
                self.cmd.anim = a.get(1).and_then(Value::num).map(|v| v as i64);
                vec![]
            }
            "SetTimer" => {
                self.timers.insert(key(a.first()), n(1));
                vec![]
            }
            "GetTimer" => num(self.timers.get(&key(a.first())).copied().unwrap_or(0.0)),
            "IsFinishTimer" => boolean(self.timers.get(&key(a.first())).copied().unwrap_or(0.0) <= 0.0),
            "StartIdTimer" => {
                self.id_timers.insert(key(a.first()), 0.0);
                vec![]
            }
            "GetIdTimer" => num(self.id_timers.get(&key(a.first())).copied().unwrap_or(9999.0)),
            "SetNumber" => {
                self.numbers.insert(key(a.first()), a.get(1).cloned().unwrap_or_default());
                vec![]
            }
            "GetNumber" => vec![self.numbers.get(&key(a.first())).cloned().unwrap_or(Value::Num(0.0))],
            "SetStringIndexedNumber" => {
                let k = a.first().and_then(Value::as_str).unwrap_or("").to_owned();
                self.snum.insert(k, a.get(1).cloned().unwrap_or_default());
                vec![]
            }
            "GetStringIndexedNumber" => {
                let k = a.first().and_then(Value::as_str).unwrap_or("");
                vec![self.snum.get(k).cloned().unwrap_or(Value::Num(0.0))]
            }
            "StartAttackPassedTimer" => {
                self.atk_passed.insert(key(a.first()), 0.0);
                vec![]
            }
            "GetAttackPassedTime" => num(self.atk_passed.get(&key(a.first())).copied().unwrap_or(9999.0)),
            "AddObserveSpecialEffectAttribute" => {
                let watch = self.observed.entry(key(a.get(1))).or_default();
                if !watch.contains(&key(a.first())) {
                    watch.push(key(a.first()));
                }
                vec![]
            }
            "IsInterupt" => boolean(self.interrupt.is_some_and(|(k, _)| is(a.first(), k))),
            "GetSpecialEffectActivateInterruptType" => num(self.interrupt.filter(|(k, _)| *k == self.k.int_sp).map_or(0.0, |(_, sp)| sp)),
            // An observed SpEffect ending (115000 / 118020 / 120000 / 102020 battle goals wait for
            // 109220 / 5026-5028 to end); never raised before (2026-10-12 round 22).
            "GetSpecialEffectInactivateInterruptType" => num(self.interrupt.filter(|(k, _)| *k == self.k.int_sp_off).map_or(0.0, |(_, sp)| sp)),
            "Replanning" => {
                self.replan = true;
                vec![]
            }
            "AddTopGoal" => {
                let g = self.goal_from_args(a);
                self.top_queue.push(g);
                vec![goal_obj(g)]
            }
            "GetTopGoal" => vec![self.root.map_or(Value::Nil, goal_obj)],
            "IsActiveGoal" | "HasGoal" => {
                let mut ids = HashSet::new();
                if let Some(r) = self.root {
                    self.active_ids(r, &mut ids);
                }
                boolean(ids.contains(&key(a.first())))
            }
            _ => {
                self.warn_once(format!("ai:{name}"));
                num(0.0)
            }
        }
    }

    fn native_activate(&mut self, g: u32, n: Native) {
        let bearing = self.w.bearing as f64;
        let goal = self.goal_mut(g);
        let or = |v: Value, d: f64| if v.truthy() { v.num().unwrap_or(d) } else { d };
        match n {
            Native::Attack => {
                goal.ez = goal.param(0).num();
                goal.succ = or(goal.param(2), 9999.0);
                goal.turn_time = or(goal.param(4), 0.0);
                goal.front = or(goal.param(5), 90.0);
            }
            Native::SpinStep | Native::Guard => goal.ez = goal.param(0).num(),
            Native::Approach | Native::Leave | Native::Sideway | Native::Keep => goal.start_bearing = bearing,
            Native::Wait | Native::Stay | Native::BackToHome => {}
            Native::ConfirmCaution => {
                // FUN_1405c35a0: the look-around anim (CommonAttack 0x898, -1 life, turn 180 deg
                // in 1.5 s, front 20 deg), then Wait for the time.
                let (ez, target, time, wait_target) = (goal.param(0), goal.param(1), goal.param(2), goal.param(3));
                let mut subs = Vec::new();
                if ez.num().is_some_and(|e| e >= 0.0) {
                    let p = vec![ez, target, Value::Num(9999.0), Value::Num(180.0), Value::Num(1.5), Value::Num(20.0)];
                    subs.push(self.new_goal(Some(2200), -1.0, p));
                }
                let life = time.num().unwrap_or(0.0);
                subs.push(self.new_goal(Some(2000), life, vec![wait_target]));
                let goal = self.goal_mut(g);
                goal.subs = subs;
                goal.had_sub = true;
            }
        }
    }

    /// Native goals with an engine-side Terminate.
    fn native_terminate(&mut self, g: u32, n: Native) {
        // Not when a new sound raised INTERUPT_ChangeSoundTarget (46) this frame: its listen
        // reaction replaced the plan, and clearing the sound made the same sound "new" again
        // next frame, whose 46 then replanned the reaction away (round 26). gap: 53 / 55
        // (indication / corpse) are not raised yet.
        if n == Native::ConfirmCaution && self.w.t_state == 1 && !self.w.sound_new {
            // FUN_1405c3770: targeting vfuncs 0x28 / 0x30 / 0x38 / 0x40 (gap: taken as the sound,
            // indication, memory and corpse slots).
            let _ = g;
            self.cmd.clear |= CLEAR_SOUND | CLEAR_INDICATION | CLEAR_MEMORY;
        }
    }

    /// The requested -> run phases shared by the action goals; None while the anim runs.
    fn action_phase(&mut self, g: u32, flag: i64) -> Option<i32> {
        let goal = self.goal(g);
        let (ez, endure) = (goal.ez, goal.parent_endure);
        let w_ez = self.w.ez.map(|e| e as f64);
        match goal.phase {
            0 => {
                if !self.can_act(flag) {
                    return Some(CONTINUE);
                }
                if goal.turn_time > 0.0 && self.w.free && goal.t < goal.turn_time && !angle_in(0.0, goal.front, self.angle()) {
                    self.cmd.turn = true;
                    return Some(CONTINUE);
                }
                self.cmd.anim = ez.map(|e| e as i64);
                self.cmd.endure = endure;
                let goal = self.goal_mut(g);
                goal.phase = 1;
                goal.busy = true;
                Some(CONTINUE)
            }
            1 => {
                if w_ez == ez {
                    self.goal_mut(g).phase = 2;
                } else if self.w.ez_failed.map(|e| e as f64) == ez {
                    return Some(FAILED);
                } else {
                    self.cmd.anim = ez.map(|e| e as i64);
                    self.cmd.endure = endure;
                }
                Some(CONTINUE)
            }
            _ if w_ez != ez => Some(FAILED),
            _ => None,
        }
    }

    fn native_update(&mut self, g: u32, n: Native) -> i32 {
        match n {
            Native::Attack => {
                // An endure request (Goal.Parry's guard / deflect) also cuts a clip at flag 122:
                // c9997 ExecAIAction takes env 123 first, and the exe posts it while that flag is
                // on (FUN_140b36410; a000_008010's hit reaction opens it at 0.23 s, 86 at 0.5 s).
                let flag = if self.goal(g).parent_endure && self.w.flags.contains(&FLAG_ENDURE) { FLAG_ENDURE } else if self.goal(g).parent_combo { 23 } else { 86 };
                if let Some(r) = self.action_phase(g, flag) {
                    return r;
                }
                let succ = self.goal(g).succ;
                let d = self.goal_dist(None);
                // At the cancel window the exe decides both ways (FUN_1405c6d30: a cancel flag on
                // and successDist < GetDist -> -1): out of range the goal fails there, not at the
                // clip's end, and the replan sees the window's TAE SpEffects (c1180's a200_003004
                // holds 3118061 0.9-1.2 s, 118020_battle.lua Act10 -> 3007; live: 3007 started
                // 55-57 ticks into all four of the game's 3004s, flag 23 opens at 56).
                // gap: which of the exe's flag bits (0x8 / 0x100) is TAE 23 and which 86 is not traced.
                if self.w.t > 0.1 && self.w.flags.contains(&23) {
                    return if d <= succ { SUCCESS } else { FAILED };
                }
                if self.w.anim_done {
                    return if d <= succ { SUCCESS } else { FAILED };
                }
                CONTINUE
            }
            Native::SpinStep => {
                if let Some(r) = self.action_phase(g, 79) {
                    return r;
                }
                let f = &self.w.flags;
                if self.w.anim_done || (self.w.t > 0.1 && (f.contains(&86) || f.contains(&78))) {
                    return SUCCESS;
                }
                CONTINUE
            }
            Native::Approach | Native::Leave | Native::Sideway | Native::Keep => {
                let goal = self.goal(g);
                // MoveToSomewhere / LeaveTarget move relative to their first param (a target or a
                // point such as POINT_INITIAL); the others to the current target.
                let target = matches!(n, Native::Approach | Native::Leave).then(|| goal.param(0));
                let mut at = self.pos_of(target.as_ref());
                let d = self.goal_dist(target.as_ref());
                let (mv, walk) = match n {
                    Native::Approach => {
                        // FUN_1405caa00 (1405c.c:7455): IsArrived first (FUN_1405e4f20 ->
                        // FUN_1406052e0 -> FUN_1405b9070, 1405b.c:6531: distance < the chr's
                        // radius + the goal's distance, strict; the target's own extent +0x1c is 0
                        // for POINT_MOVE_POINT), then for a patrol point IsInsideMovePoint (its
                        // region). gap: the exe's 2D / 3D switch (+0x20) is not mapped to the calc
                        // type; read in XZ as goal_dist.
                        let on_route = is(target.as_ref(), self.k.point_move_point);
                        let inside = on_route.then(|| self.patrol.as_ref().and_then(|p| p.inside_current(self.w.me - Vec3::Y * (crate::player::CAPSULE_HALF_HEIGHT + 0.05)))).flatten();
                        if d < goal.raw(2, 0.0) || inside == Some(true) {
                            // At the patrol point: the route steps on (Patrol::step).
                            if on_route {
                                let r = (self.random() * 1e9) as u64;
                                if let Some(p) = self.patrol.as_mut() {
                                    p.step(r);
                                }
                            }
                            return SUCCESS;
                        }
                        // A patrol walks its round: NonBattleAct passes walk false ([103, 0, 0,
                        // -1, false, ..]) yet the game's guards walk between their points (the
                        // user, 2026-10-17). gap: how the exe turns that false into a walk (the
                        // HKS reads MoveSpeedLevel) is not traced.
                        ('F', on_route || goal.p.get(4).is_some_and(|v| matches!(v, Value::Bool(true))))
                    }
                    Native::Leave => {
                        if d >= goal.raw(1, 0.0) {
                            return SUCCESS;
                        }
                        // Turned toward itself (turnTarget TARGET_SELF): it turns its back and
                        // goes, walking or running by param 3 (Gyoubu's horse, 508000_battle.lua
                        // Act25 / Kengeki08: LeaveTarget(.., 15, TARGET_SELF, false); c5080 has
                        // no WalkBack state). gap: SprjGoalLeaveTarget's turn rule is not traced
                        // in the exe; read from the parameter's name.
                        if is(goal.p.get(2), self.k.target_self) {
                            // (No point: the current target, from its bearing and distance.)
                            let b = self.w.bearing.to_radians();
                            let p = at.unwrap_or(self.w.me + Vec3::new(-b.sin(), 0.0, -b.cos()) * d as f32);
                            at = Some(self.w.me * 2.0 - p);
                            ('F', goal.p.get(3).is_some_and(|v| matches!(v, Value::Bool(true))))
                        } else {
                            ('B', true)
                        }
                    }
                    Native::Sideway => {
                        let moved = ((self.w.bearing as f64 - goal.start_bearing + 540.0).rem_euclid(360.0) - 180.0).abs();
                        if moved >= goal.raw(2, 45.0) {
                            return SUCCESS;
                        }
                        (if is(goal.p.get(1), 1.0) { 'R' } else { 'L' }, true)
                    }
                    _ => {
                        let (lo, hi) = (goal.raw(1, 0.0), goal.raw(2, 99.0));
                        if d >= lo && d <= hi {
                            return SUCCESS;
                        }
                        (if d < lo { 'B' } else { 'F' }, true)
                    }
                };
                // The move's guard action (its last param: MoveToSomewhere 8, as ApproachTarget /
                // ApproachSettingDirection pass it on; LeaveTarget 4; SidewayMove / KeepDist 5),
                // e.g. LeaveTarget(4, TARGET_ENE_0, d, TARGET_ENE_0, true, 9910): it moves in the
                // guard stance (ACTION_TYPE_GUARD; enemy.rs guard_stance); 9920 in the general
                // stance (ACTION_TYPE_GENERAL_STANCE; common_common_func2.lua:705-708 KeepDist /
                // SidewayMove).
                let guard_at = match n {
                    Native::Approach => 8,
                    Native::Leave => 4,
                    _ => 5,
                };
                match goal.raw(guard_at, -1.0) {
                    ACTION_TYPE_GUARD => self.cmd.guard = true,
                    ACTION_TYPE_GENERAL_STANCE => self.cmd.general = true,
                    _ => {}
                }
                if !self.can_act(78) {
                    return CONTINUE;
                }
                self.cmd.mv = Some(mv);
                self.cmd.walk = walk;
                self.cmd.face = true;
                self.cmd.at = at;
                CONTINUE
            }
            Native::BackToHome => {
                // gap: the engine's arrival radius; 1 m, the distance NonBattleAct accepts as home.
                if (self.w.home - self.w.me).with_y(0.0).length() <= 1.0 {
                    return SUCCESS;
                }
                if self.can_act(78) {
                    self.cmd.mv = Some('F');
                    self.cmd.walk = true;
                    self.cmd.face = true;
                    self.cmd.at = Some(self.w.home);
                }
                CONTINUE
            }
            Native::Stay => {
                let t = self.goal(g).param(1);
                if self.w.free && !is(Some(&t), self.k.target_self) && !angle_in(0.0, 20.0, self.angle_to(Some(&t))) {
                    self.cmd.turn = true;
                    self.cmd.at = self.pos_of(Some(&t));
                }
                CONTINUE
            }
            Native::ConfirmCaution => {
                if self.goal(g).subs.is_empty() { SUCCESS } else { CONTINUE }
            }
            Native::Wait => {
                if is(Some(&self.goal(g).param(0)), self.k.target_ene_0) && self.w.free {
                    self.cmd.turn = true;
                }
                CONTINUE
            }
            Native::Guard => {
                let (req, ez) = (self.goal(g).req, self.goal(g).ez);
                // 9910 is no anim but the HKS's ACTION_TYPE_GUARD (c9997.lua:258): the stance.
                if ez == Some(ACTION_TYPE_GUARD) {
                    self.cmd.guard = true;
                    return CONTINUE;
                }
                if ez == Some(ACTION_TYPE_GENERAL_STANCE) {
                    self.cmd.general = true;
                    return CONTINUE;
                }
                if !req && ez.is_some_and(|e| e > 0.0) && self.can_act(86) {
                    self.cmd.anim = ez.map(|e| e as i64);
                    self.goal_mut(g).req = true;
                }
                CONTINUE
            }
        }
    }
}

impl Host for AiState {
    fn call_host(&mut self, vm: &mut Vm, name: &str, args: &[Value]) -> LuaResult<Vec<Value>> {
        let id = || args.first().and_then(Value::num).map(|n| n as i64);
        let truthy = || args.get(1).is_some_and(Value::truthy);
        match name {
            "REGISTER_GOAL" => {
                if let Some(k) = id() {
                    let def = self.defs.entry(k).or_default();
                    def.name = args.get(1).and_then(Value::as_str).map(str::to_owned);
                }
            }
            "REGISTER_GOAL_NO_UPDATE" => {
                if let Some(k) = id() {
                    self.defs.entry(k).or_default().no_update = truthy();
                }
            }
            "REGISTER_GOAL_NO_INTERUPT" => {
                if let Some(k) = id() {
                    self.defs.entry(k).or_default().no_interrupt = truthy();
                }
            }
            "ENABLE_COMBO_ATK_CANCEL" => {
                if let Some(k) = id() {
                    self.defs.entry(k).or_default().combo = true;
                }
            }
            "REGISTER_GOAL_NO_SUB_GOAL" => {
                if let Some(k) = id() {
                    self.defs.entry(k).or_default().no_sub_goal = truthy();
                }
            }
            "REGISTER_LOGIC_FUNC" | "REGISTER_GOAL_UPDATE_TIME" | "REGISTER_DBG_GOAL_PARAM" | "REGISTER_GOAL_USE_AVOID_CHR"
            | "print" => {}
            // The guard count is kept engine-side (clash SpEffects 200210/1/5/6 in combat.rs).
            "Get_ConsecutiveGuardCount" => return Ok(vec![Value::Num(self.w.guard_count as f64)]),
            "loadstring" => {
                // Only GOAL_COMMON_If builds code at run time, always the same shape:
                // "return function (arg) OnIf_<id>(arg.ai, arg.goal, arg.codeNo) end".
                let src = args.first().and_then(Value::as_str).unwrap_or("");
                let Some(start) = src.find("OnIf_") else {
                    return Ok(vec![Value::Nil, Value::str("loadstring unsupported")]);
                };
                let end = src[start..].find('(').map_or(src.len(), |e| start + e);
                return Ok(vec![Value::Host(Rc::from(format!("ifchunk:{}", &src[start..end])))]);
            }
            _ if name.starts_with("ifchunk:") => {
                return Ok(vec![Value::Host(Rc::from(format!("ifcall:{}", &name[8..])))]);
            }
            _ if name.starts_with("ifcall:") => {
                let Some(Value::Table(t)) = args.first() else { return Ok(vec![]) };
                let (ai, goal, code) = {
                    let t = t.borrow();
                    (t.get_str("ai"), t.get_str("goal"), t.get_str("codeNo"))
                };
                let f = vm.get_global(&name[7..]);
                return vm.call(self, &f, &[ai, goal, code]);
            }
            _ => return Err(lua50::LuaError::Runtime(format!("attempt to call global '{name}'"))),
        }
        Ok(vec![])
    }

    fn call_method(&mut self, vm: &mut Vm, obj: Obj, name: &str, args: &[Value]) -> LuaResult<Vec<Value>> {
        // args[0] is the object itself (method-call syntax).
        let a = &args[1.min(args.len())..];
        if obj.kind == OBJ_GOAL && name == "ClearSubGoal" {
            if self.goals.contains_key(&obj.id) {
                clear_subs(vm, self, obj.id)?;
            }
            return Ok(vec![]);
        }
        Ok(if obj.kind == OBJ_GOAL { self.goal_method(obj.id, name, a) } else { self.ai_method(name, a) })
    }
}

/// Calls a goal kind's script callback, if it has one.
fn call_def(vm: &mut Vm, st: &mut AiState, g: u32, cb: Cb, extra: &[Value]) -> LuaResult<Option<Value>> {
    let Some(def) = st.def(st.goal(g).kind) else { return Ok(None) };
    let (f, mut args) = if let Some(tbl) = &def.tbl {
        (tbl.borrow().get_str(cb.table_key()), vec![Value::Table(tbl.clone()), ai_obj(), goal_obj(g)])
    } else if let Some(fns) = &def.fns {
        (fns[cb as usize].clone(), vec![ai_obj(), goal_obj(g)])
    } else {
        return Ok(None);
    };
    if f.is_nil() {
        return Ok(None);
    }
    args.extend_from_slice(extra);
    Ok(Some(first(vm.call(st, &f, &args)?)))
}

fn terminate(vm: &mut Vm, st: &mut AiState, g: u32) -> LuaResult<()> {
    let goal = st.goal(g);
    if !goal.started || goal.terminated {
        return Ok(());
    }
    st.goal_mut(g).terminated = true;
    for s in st.goal(g).subs.clone() {
        terminate(vm, st, s)?;
    }
    match st.goal(g).kind.and_then(native) {
        Some(n) => st.native_terminate(g, n),
        None => {
            call_def(vm, st, g, Cb::Terminate, &[])?;
        }
    }
    Ok(())
}

fn clear_subs(vm: &mut Vm, st: &mut AiState, g: u32) -> LuaResult<()> {
    for s in st.goal(g).subs.clone() {
        terminate(vm, st, s)?;
    }
    st.goal_mut(g).subs.clear();
    Ok(())
}

fn tick_goal(vm: &mut Vm, st: &mut AiState, g: u32, dt: f64) -> LuaResult<i32> {
    let kind = st.goal(g).kind;
    let nat = kind.and_then(native);
    if !st.goal(g).started {
        st.goal_mut(g).started = true;
        st.apply_timing(g, st.k.timing_activate);
        if let Some(n) = nat {
            st.native_activate(g, n);
        } else if st.def(kind).is_some_and(|d| d.tbl.is_some() || d.fns.is_some()) {
            // (A def from REGISTER_GOAL_NO_INTERUPT etc. alone has no script: unknown goal.)
            call_def(vm, st, g, Cb::Activate, &[])?;
        } else {
            st.warn_once(format!("goal id {}", kind.map_or("nil".into(), |k| k.to_string())));
            return Ok(FAILED);
        }
    }
    st.goal_mut(g).t += dt;
    if let Some(&s) = st.goal(g).subs.first() {
        let r = tick_goal(vm, st, s, dt)?;
        if r == SUCCESS {
            st.apply_timing(s, st.k.timing_success);
            terminate(vm, st, s)?;
            let subs = &mut st.goal_mut(g).subs;
            if !subs.is_empty() {
                subs.remove(0);
            }
        } else if r == FAILED {
            terminate(vm, st, s)?;
            clear_subs(vm, st, g)?;
            return Ok(FAILED);
        }
        if !st.goal(g).subs.is_empty() {
            if nat.is_some() || !st.def(kind).is_some_and(|d| d.no_sub_goal && !d.no_update) {
                // Still going on with its subgoals: its own life is checked all the same.
                if st.life_over(g) {
                    let r = st.life_end(g);
                    clear_subs(vm, st, g)?;
                    return Ok(r);
                }
                return Ok(CONTINUE);
            }
            // Its Update decides while the subgoals run.
            let r = match call_def(vm, st, g, Cb::Update, &[Value::Num(dt)])? {
                None | Some(Value::Nil | Value::Bool(false)) => CONTINUE,
                Some(Value::Num(n)) => n as i32,
                Some(_) => OTHER,
            };
            let r = if r == CONTINUE && st.life_over(g) { st.life_end(g) } else { r };
            if r == SUCCESS || r == FAILED {
                clear_subs(vm, st, g)?;
            }
            return Ok(r);
        }
    }
    let goal = st.goal(g);
    if goal.life >= 0.0 && goal.t >= goal.life {
        // Life over: actions that already started run to their own end; an action that
        // never got to start fails (dropping the rest of its combo). Waiting goals end by
        // their life (gap: their own updates are not traced; read as a success).
        match nat {
            Some(n) if n.ignore_life() => {
                if !goal.busy {
                    return Ok(FAILED);
                }
            }
            Some(Native::Wait | Native::Stay | Native::Guard) => return Ok(SUCCESS),
            _ => {}
        }
    }
    if let Some(n) = nat {
        let r = st.native_update(g, n);
        let ends = r == CONTINUE && st.life_over(g) && !n.ignore_life();
        return Ok(if ends { st.life_end(g) } else { r });
    }
    let Some(def) = st.def(kind) else { return Ok(FAILED) };
    if def.no_update {
        return Ok(if st.goal(g).subs.is_empty() { SUCCESS } else { CONTINUE });
    }
    let r = match call_def(vm, st, g, Cb::Update, &[Value::Num(dt)])? {
        None | Some(Value::Nil | Value::Bool(false)) => CONTINUE,
        Some(Value::Num(n)) => n as i32,
        Some(_) => OTHER,
    };
    let goal = st.goal(g);
    if r == CONTINUE && goal.had_sub && goal.subs.is_empty() {
        return Ok(SUCCESS);
    }
    if r == CONTINUE && st.life_over(g) {
        return Ok(st.life_end(g));
    }
    Ok(r)
}

/// Drops the plan: every top goal ends (Terminate), the logic runs again next.
fn reset_root(vm: &mut Vm, st: &mut AiState) -> LuaResult<()> {
    for r in std::mem::take(&mut st.tops) {
        terminate(vm, st, r)?;
    }
    st.root = None;
    Ok(())
}

/// A new plan: the logic script's Main (ExecTableLogic -> Logic.Main) adds the top goals.
/// Without a logic script the battle goal is the plan (the duel before stealth existed).
fn plan(vm: &mut Vm, st: &mut AiState) -> LuaResult<()> {
    st.top_queue.clear();
    let exec = vm.get_global("ExecTableLogic");
    let has_logic = match vm.get_global("g_LogicTable") {
        Value::Table(t) => !t.borrow().get(&Value::Num(st.logic_id as f64)).is_nil(),
        _ => false,
    };
    if has_logic && !exec.is_nil() {
        let logic = Value::Num(st.logic_id as f64);
        vm.call(st, &exec, &[ai_obj(), logic])?;
    } else {
        let g = st.new_goal(Some(st.battle_goal), -1.0, Vec::new());
        st.top_queue.push(g);
    }
    st.tops = std::mem::take(&mut st.top_queue);
    st.root = st.tops.first().copied();
    Ok(())
}

/// Offers an interrupt to the root goal's Interrupt (Goal.Parry, Kengeki, ...).
fn interrupt_goals(vm: &mut Vm, st: &mut AiState, kind: f64, sp: f64) -> LuaResult<bool> {
    if st.root.is_none() {
        plan(vm, st)?;
    }
    let Some(root) = st.root else { return Ok(false) };
    st.interrupt = Some((kind, sp));
    let mut handled = Ok(false);
    if st.def(st.goal(root).kind).is_some_and(|d| !d.no_interrupt) {
        handled = call_def(vm, st, root, Cb::Interrupt, &[goal_obj(root)]).map(|r| matches!(r, Some(Value::Bool(true))));
    }
    st.interrupt = None;
    // A handled interrupt replaces the plan: a fresh root does not Activate on top of it.
    if handled.as_ref().is_ok_and(|h| *h) {
        st.goal_mut(root).started = true;
    }
    handled
}

/// The logic-level interrupt chain of a table-logic enemy (exe FUN_1405d3fd0):
/// InterruptTableLogic(ai, goal, logic, kind), then COMMON_Interrupt, then
/// InterruptTableLogic_Common - where common_table_ai_common.lua:247 turns a new sound into the
/// ListenSound 700 / 710 reaction (Wait 0-0.3 s, EndureAttack) of enemies with SpEffect
/// 241000 / chuboss reaction. A handled interrupt's AddTopGoal list becomes the plan.
/// gap: only interrupt 46 is sent this way yet; the others still go to the root goal only.
fn interrupt_logic(vm: &mut Vm, st: &mut AiState, kind: f64) -> LuaResult<bool> {
    if st.root.is_none() {
        plan(vm, st)?;
    }
    let Some(root) = st.root else { return Ok(false) };
    let has_logic = match vm.get_global("g_LogicTable") {
        Value::Table(t) => !t.borrow().get(&Value::Num(st.logic_id as f64)).is_nil(),
        _ => false,
    };
    if !has_logic {
        return Ok(false);
    }
    let logic = Value::Num(st.logic_id as f64);
    // The exe calls InterruptTableLogic only for the types the logic registered (FUN_1405e5cc0);
    // InterruptInfoTable has no entry for 46, so that one goes on to the common chain.
    let registered = match vm.get_global("g_LogicTable") {
        Value::Table(t) => match t.borrow().get(&logic) {
            Value::Table(l) => match l.borrow().get(&Value::Str(Rc::from("InterruptInfoTable"))) {
                Value::Table(i) => !i.borrow().get(&Value::Num(kind)).is_nil(),
                _ => false,
            },
            _ => false,
        },
        _ => false,
    };
    st.interrupt = Some((kind, 0.0));
    st.top_queue.clear();
    let mut handled = false;
    for (name, args) in [
        ("InterruptTableLogic", vec![ai_obj(), goal_obj(root), logic.clone(), Value::Num(kind)]),
        ("COMMON_Interrupt", vec![ai_obj(), goal_obj(root)]),
        ("InterruptTableLogic_Common", vec![ai_obj(), goal_obj(root), logic.clone()]),
    ] {
        if name == "InterruptTableLogic" && !registered {
            continue;
        }
        let f = vm.get_global(name);
        if f.is_nil() {
            continue;
        }
        let r = match vm.call(st, &f, &args) {
            Ok(r) => r,
            Err(e) => {
                st.interrupt = None;
                return Err(e);
            }
        };
        if r.first().is_some_and(|v| v.truthy()) {
            handled = true;
            break;
        }
    }
    st.interrupt = None;
    if handled && !st.top_queue.is_empty() {
        let queued = std::mem::take(&mut st.top_queue);
        reset_root(vm, st)?;
        st.tops = queued;
        st.root = st.tops.first().copied();
    }
    Ok(handled)
}

pub struct Brain {
    vm: Vm,
    st: AiState,
    pub log: Vec<String>,
}

impl Brain {
    /// Gives the brain its MSB patrol route (enemy.rs PatrolRoute), starting at its first point.
    pub fn set_patrol(&mut self, patrol: Patrol) {
        self.st.patrol = Some(patrol);
    }

    /// The home pose its finished type 5 route left (enemy.rs moves its home there).
    pub fn patrol_home(&self) -> Option<(Vec3, f32)> {
        self.st.patrol.as_ref().and_then(|p| p.new_home)
    }

    /// The patrol's (current, previous) point, None without a route.
    pub fn patrol_points(&self) -> Option<(i64, i64)> {
        self.st.patrol.as_ref().map(|p| (p.cur, p.prev))
    }
}

/// Loaded first: constants and goal ids, then the table-goal helpers (RegisterTableGoal,
/// g_GoalTable) every other file registers through.
const FIRST: [&str; 5] = ["ai_define.lua", "goal_list.lua", "logic_list.lua", "event_list.lua", "table_ai_common.lua"];

/// Script load order: aicommon.luabnd's files, then the character's battle goal from the
/// first map luabnd that has it (the copies are byte-identical).
fn script_order(dir: &Path, battle_goal: i64, logic_id: i64) -> Result<Vec<PathBuf>, String> {
    let common = dir.join("aicommon.luabnd.d");
    let mut rest: Vec<PathBuf> = std::fs::read_dir(&common)
        .map_err(|e| format!("{}: {e}", common.display()))?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "lua"))
        .filter(|p| !FIRST.iter().any(|f| p.ends_with(f)))
        .collect();
    rest.sort();
    let mut out: Vec<PathBuf> = FIRST.iter().map(|f| common.join(f)).collect();
    out.extend(rest);
    let mut maps: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| e.to_string())?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with('m') && n.ends_with(".luabnd.d")))
        .collect();
    maps.sort();
    let file = format!("{battle_goal:06}_battle.lua");
    let battle = maps
        .iter()
        .map(|m| m.join(&file))
        .find(|p| p.is_file())
        .ok_or_else(|| format!("{file} not in {}/m*.luabnd.d", dir.display()))?;
    out.push(battle);
    // The logic script (NpcThinkParam logicId, e.g. 102000_logic.lua) if the maps have it.
    let logic = format!("{logic_id:06}_logic.lua");
    if let Some(p) = maps.iter().map(|m| m.join(&logic)).find(|p| p.is_file()) {
        out.push(p);
    }
    Ok(out)
}

impl Brain {
    /// `dir` is extracted/script (the unpacked luabnd folders).
    /// `think` is the NpcThinkParam row `think_id` (its battleGoalID / logicId pick the scripts).
    pub fn load(dir: &Path, think_id: i64, think: &Json, hit_radius: f32, seed: u32) -> Result<Self, String> {
        let battle_goal = think["battleGoalID"].as_i64().ok_or("NpcThinkParam row without battleGoalID")?;
        let logic_id = think["logicId"].as_i64().unwrap_or(-1);
        let files = script_order(dir, battle_goal, logic_id)?;
        let mut vm = Vm::new();
        for f in NATIVE_GLOBALS {
            vm.set_global(f, Value::Host(Rc::from(f)));
        }
        let mut rng = (seed as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
        rng ^= rng >> 29;
        let mut st = AiState {
            defs: HashMap::new(),
            goals: HashMap::new(),
            next_goal: 1,
            root: None,
            top_queue: Vec::new(),
            replan: false,
            interrupt: None,
            w: Snapshot::default(),
            cmd: Command::default(),
            log: Vec::new(),
            warned: HashSet::new(),
            timers: HashMap::new(),
            numbers: HashMap::new(),
            snum: HashMap::new(),
            id_timers: HashMap::new(),
            atk_passed: HashMap::new(),
            observed: HashMap::new(),
            battle_goal,
            think_id,
            logic_id,
            think: think.clone(),
            excel: HashMap::new(),
            tops: Vec::new(),
            turn_id: None,
            hit_radius,
            rng: rng | 1,
            patrol: None,
            event_target: None,
            observe: Vec::new(),
            observe_in: HashSet::new(),
            observe_changed: (Vec::new(), Vec::new()),
            trigger_observers: Vec::new(),
            trigger_in: Vec::new(),
            trigger_changed: (Vec::new(), Vec::new()),
            k: Consts {
                target_self: -1.0,
                target_ene_0: 0.0,
                target_event: 20.0,
                dir_b: 2.0,
                dir_l: 3.0,
                dir_r: 4.0,
                timing_activate: 0.0,
                timing_success: 1.0,
                int_parry: 24.0,
                int_use_item: 12.0,
                int_sp: 43.0,
                int_sp_off: 44.0,
                int_sound: 46.0,
                point_initial: 100.0,
                point_init_pose: 111.0,
                point_move_point: 103.0,
                point_back_to_home: 125.0,
                point_event: 102.0,
                team_formation: 24.0,
            },
        };
        for path in &files {
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            let data = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
            let r = load_chunk(&data).and_then(|p| vm.exec_chunk(&mut st, p));
            if let Err(e) = r {
                st.log.push(format!("{name}: {e}"));
            }
        }
        vm.set_global("Get_ConsecutiveGuardCount", Value::Host(Rc::from("Get_ConsecutiveGuardCount")));
        bind_goals(&vm, &mut st);
        let c = |name: &str, d: f64| vm.get_global(name).num().unwrap_or(d);
        st.k = Consts {
            target_self: c("TARGET_SELF", -1.0),
            target_ene_0: c("TARGET_ENE_0", 0.0),
            target_event: c("TARGET_EVENT", 20.0),
            dir_b: c("AI_DIR_TYPE_B", 2.0),
            dir_l: c("AI_DIR_TYPE_L", 3.0),
            dir_r: c("AI_DIR_TYPE_R", 4.0),
            timing_activate: c("AI_TIMING_SET__ACTIVATE", 0.0),
            timing_success: c("AI_TIMING_SET__UPDATE_SUCCESS", 1.0),
            int_parry: c("INTERUPT_ParryTiming", 24.0),
            int_use_item: c("INTERUPT_UseItem", 12.0),
            int_sp: c("INTERUPT_ActivateSpecialEffect", 43.0),
            int_sp_off: c("INTERUPT_InactivateSpecialEffect", 44.0),
            int_sound: c("INTERUPT_ChangeSoundTarget", 46.0),
            point_initial: c("POINT_INITIAL", 100.0),
            point_init_pose: c("POINT_INIT_POSE", 111.0),
            point_move_point: c("POINT_MOVE_POINT", 103.0),
            point_back_to_home: c("POINT_BackToHome", 125.0),
            point_event: c("POINT_EVENT", 102.0),
            team_formation: c("TARGET_TEAM_FORMATION", 24.0),
        };
        // GetExcelParam(AI_EXCEL_THINK_PARAM_TYPE__<field>) reads NpcThinkParam <field>.
        let mut k = Value::Nil;
        while let Some((name, v)) = vm.globals.borrow().next(&k) {
            if let (Some(n), Some(id)) = (name.as_str(), v.num()) {
                if let Some(field) = n.strip_prefix("AI_EXCEL_THINK_PARAM_TYPE__") {
                    st.excel.insert(id as i64, field.to_owned());
                }
            }
            k = name;
        }
        let log = std::mem::take(&mut st.log);
        Ok(Brain { vm, st, log })
    }

    pub fn tick(&mut self, s: &Snapshot, dt: f32) -> Command {
        let r = self.try_tick(s, dt as f64);
        self.st.sweep();
        self.log.append(&mut self.st.log);
        match r {
            Ok(()) => self.st.cmd.clone(),
            Err(e) => {
                let msg = e.to_string();
                if self.log.last() != Some(&msg) {
                    self.log.push(msg);
                }
                Command::default()
            }
        }
    }

    fn try_tick(&mut self, s: &Snapshot, dt: f64) -> LuaResult<()> {
        let (vm, st) = (&mut self.vm, &mut self.st);
        st.w = s.clone();
        st.cmd = Command::default();
        for v in st.timers.values_mut() {
            *v -= dt;
        }
        for v in st.id_timers.values_mut().chain(st.atk_passed.values_mut()) {
            *v += dt;
        }
        if let Some(ez) = s.ez_started {
            st.atk_passed.insert(ez, 0.0);
        }
        // An anim the AI did not request replaced its action (reaction, guard, deflect):
        // the plan is dropped and rebuilt, which is where Kengeki_Activate sees the clash.
        if s.interrupted {
            reset_root(vm, st)?;
        }
        // The player's attack notify (TAE flag 63): Goal.Interrupt -> Goal.Parry.
        let (int_parry, int_use_item, int_sp, int_sp_off) = (st.k.int_parry, st.k.int_use_item, st.k.int_sp, st.k.int_sp_off);
        if s.parry_timing {
            interrupt_goals(vm, st, int_parry, 0.0)?;
        }
        if s.target_use_item {
            interrupt_goals(vm, st, int_use_item, 0.0)?;
        }
        for &id in &s.sp_new {
            if st.observes(id, s, true) {
                interrupt_goals(vm, st, int_sp, id as f64)?;
            }
        }
        for &id in &s.sp_gone {
            if st.observes(id, s, false) {
                interrupt_goals(vm, st, int_sp_off, id as f64)?;
            }
        }
        // The same on the TARGET_EVENT character.
        if let Some(e) = &s.event {
            for (ids, new, int) in [(&e.sp_new, true, int_sp), (&e.sp_gone, false, int_sp_off)] {
                for &id in ids {
                    if st.observes_event(id, new) {
                        interrupt_goals(vm, st, int, id as f64)?;
                    }
                }
            }
        }
        // Observe areas (AddObserveArea / AddObserveRegion) the target entered or left.
        st.observe_edges();
        if !st.observe_changed.0.is_empty() {
            interrupt_goals(vm, st, 16.0, 0.0)?;
        }
        if !st.observe_changed.1.is_empty() {
            interrupt_goals(vm, st, 40.0, 0.0)?;
        }
        // Trigger regions entered (48: InterruptTableLogic_Common's caution stop,
        // common_table_ai_common.lua:286) or left (49).
        st.trigger_edges();
        if !st.trigger_changed.0.is_empty() && !interrupt_logic(vm, st, 48.0)? {
            interrupt_goals(vm, st, 48.0, 0.0)?;
        }
        if !st.trigger_changed.1.is_empty() {
            interrupt_goals(vm, st, 49.0, 0.0)?;
        }
        if s.sound_new {
            let k = st.k.int_sound;
            interrupt_logic(vm, st, k)?;
        }
        // A map-event AI command (INTERUPT_EventRequest 39: bit 39 of the update's interrupt
        // mask, set by FUN_1405b49e0 from the request's received bit, so for one update per
        // write, a repeat of the same value too). The goals' Interrupt first (FUN_1405ada60 ->
        // FUN_1405cfbd0; 507000_battle.lua replans on 10 / 20), then, unhandled, the logic's
        // (FUN_1405d3fd0: the Divine Dragon's trees, 531001_logic.lua request 10 -> a bloom).
        if s.event_req_new && !interrupt_goals(vm, st, 39.0, 0.0)? {
            interrupt_logic(vm, st, 39.0)?;
        }
        // A target-state change drops the plan (FUN_1405ab7d0 -> FUN_14061f500).
        if std::mem::take(&mut st.replan) || s.t_replan {
            reset_root(vm, st)?;
        }
        // A plan that ends is followed by the next one in the same frame, on this frame's state:
        // the hidden ground zombie (c1500 Act16 -> 20010) must see 20010's SpEffect 5030 (TAE
        // 0-0.133 s) when its attack goal ends at the combo window, to pick Act15 (rise and
        // grab 3015). (At most one follow-up plan per frame.)
        for pass in 0..2 {
            if st.root.is_none() {
                plan(vm, st)?;
            }
            let Some(root) = st.root else { return Ok(()) };
            let r = tick_goal(vm, st, root, if pass == 0 { dt } else { 0.0 })?;
            // Top goals added while running (AddTopGoal from a goal) queue behind the plan.
            let added = std::mem::take(&mut st.top_queue);
            st.tops.extend(added);
            if r != CONTINUE {
                terminate(vm, st, root)?;
                st.tops.retain(|&g| g != root);
                st.root = st.tops.first().copied();
            }
            if std::mem::take(&mut st.replan) {
                reset_root(vm, st)?;
            }
            if r == CONTINUE || st.cmd.anim.is_some() {
                break;
            }
        }
        Ok(())
    }

    /// The dummy polys of its own the AI's observe slots read (Snapshot::dummies).
    pub fn observe_dmys(&self) -> Vec<i16> {
        self.st.observe_dmys()
    }

    /// The active goal chain, e.g. "SamuraiTaisho_102000_Battle > 2221 > CommonAttack(3000)".
    pub fn describe(&self) -> String {
        let st = &self.st;
        let mut parts = Vec::new();
        let mut g = st.root;
        while let Some(goal) = g.and_then(|id| st.goals.get(&id)) {
            let name = st
                .def(goal.kind)
                .and_then(|d| d.name.clone())
                .unwrap_or_else(|| goal.kind.map_or("nil".into(), |k| k.to_string()));
            let name = name.strip_prefix("GOAL_COMMON_").or(name.strip_prefix("GOAL_")).unwrap_or(&name).to_owned();
            parts.push(match goal.ez {
                Some(ez) => format!("{name}({})", lua50::fmt_num(ez)),
                None => name,
            });
            g = goal.subs.first().copied();
        }
        parts.join(" > ")
    }
}

/// Binds the goal kinds: table goals from g_GoalTable, and function goals
/// (GOAL_COMMON_X = id with X_Activate / X_Update / X_Terminate / X_Interupt).
fn bind_goals(vm: &Vm, st: &mut AiState) {
    let entries = |t: &TableRef| {
        let mut out = Vec::new();
        let mut k = Value::Nil;
        while let Some((key, v)) = t.borrow().next(&k) {
            out.push((key.clone(), v));
            k = key;
        }
        out
    };
    if let Value::Table(gt) = vm.get_global("g_GoalTable") {
        for (k, v) in entries(&gt) {
            if let (Some(id), Value::Table(t)) = (k.num(), v) {
                st.defs.entry(id as i64).or_default().tbl = Some(t);
            }
        }
    }
    for (k, v) in entries(&vm.globals) {
        let (Some(name), Value::Num(id)) = (k.as_str(), v) else { continue };
        if !name.starts_with("GOAL_") {
            continue;
        }
        let id = id as i64;
        let short = name.strip_prefix("GOAL_COMMON_").or(name.strip_prefix("GOAL_")).unwrap_or(name);
        // Some ids drop the underscore in their functions: GOAL_COMMON_ComboTunable_SuccessAngle180
        // (2253) runs ComboTunableSuccessAngle180_Activate.
        let joined = short.replace('_', "");
        let f = |s: &str| {
            let v = vm.get_global(&format!("{short}_{s}"));
            if v.is_nil() { vm.get_global(&format!("{joined}_{s}")) } else { v }
        };
        let act = f("Activate");
        if !act.is_nil() || native(id).is_some() {
            let def = st.defs.entry(id).or_default();
            def.name.get_or_insert_with(|| name.to_owned());
            if !act.is_nil() {
                def.fns = Some([act, f("Update"), f("Terminate"), f("Interupt")]);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// NpcThinkParam row from the dumped params (extracted/json/params).
    fn think(id: i64) -> Option<Json> {
        let text = std::fs::read_to_string(crate::paths::root().join("extracted/json/params/NpcThinkParam.json")).ok()?;
        let v: Json = serde_json::from_str(&text).ok()?;
        v["rows"].as_array()?.iter().find(|r| r["id"].as_i64() == Some(id)).cloned()
    }

    fn load(think_id: i64, radius: f32, seed: u32) -> Option<Brain> {
        let dir = crate::paths::root().join("extracted/script");
        if !dir.join("aicommon.luabnd.d").exists() {
            return None;
        }
        Some(Brain::load(&dir, think_id, &think(think_id)?, radius, seed).expect("load"))
    }

    fn brain() -> Option<Brain> {
        load(10200000, 0.5, 7)
    }

    #[test]
    fn scripts_load_and_pick_an_act() {
        let Some(mut b) = brain() else { return };
        let errors: Vec<_> = b.log.iter().filter(|l| !l.starts_with("stub:")).cloned().collect();
        assert!(errors.is_empty(), "{errors:#?}");
        // Far away and free: the battle goal must approach (move) or start an attack.
        let mut s = Snapshot { dist: 8.0, free: true, hp_rate: 1.0, target_hp_rate: 1.0, sp: 1000.0, sp_rate: 1.0, ..Snapshot::battle() };
        s.sp_self.insert(200004); // battle state
        let mut acted = false;
        for _ in 0..30 {
            let c = b.tick(&s, 1.0 / 60.0);
            if c.anim.is_some() || c.mv.is_some() {
                acted = true;
                break;
            }
        }
        let errors: Vec<_> = b.log.iter().filter(|l| !l.starts_with("stub:")).cloned().collect();
        assert!(errors.is_empty(), "{errors:#?}");
        assert!(acted, "no action; goals: {}; log: {:?}", b.describe(), b.log);
    }

    /// IsTargetGuard / GetSp / GetSpRate read their target argument like GetHpRate: TARGET_SELF
    /// is the enemy itself (Common_Parry asks IsTargetGuard(TARGET_SELF); 140010_battle.lua
    /// compares GetSpRate(TARGET_SELF) with (TARGET_ENE_0)). They ignored it (2026-10-12 round 18).
    #[test]
    fn guard_and_posture_queries_follow_their_target() {
        let Some(mut b) = load(10100000, 1.0, 3) else { return };
        b.st.w = Snapshot { self_guard: true, target_guard: false, sp: 300.0, sp_rate: 0.5, target_sp: 900.0, target_sp_rate: 0.9, ..Snapshot::battle() };
        let me = Value::Num(b.st.k.target_self);
        let ene = Value::Num(0.0);
        let get = |b: &mut Brain, n: &str, t: &Value| b.st.ai_method_raw(n, std::slice::from_ref(t)).into_iter().next().unwrap();
        assert!(matches!(get(&mut b, "IsTargetGuard", &me), Value::Bool(true)));
        assert!(matches!(get(&mut b, "IsTargetGuard", &ene), Value::Bool(false)));
        assert_eq!(get(&mut b, "GetSp", &me).num(), Some(300.0));
        assert_eq!(get(&mut b, "GetSp", &ene).num(), Some(900.0));
        assert_eq!(get(&mut b, "GetSpRate", &me).num(), Some(0.5));
        assert!((get(&mut b, "GetSpRate", &ene).num().unwrap() - 0.9).abs() < 1e-6);
    }

    /// IsInsideTargetRegion / SetEventMoveTarget use the loaded map's MSB regions by entity id
    /// (FUN_1406d35d0). The Folding Screen Monkeys' Act17 (126000_battle.lua) picks its move
    /// point by where it and Wolf stand among the m20 20028xx regions; its 99028xx ids are in
    /// no MSB and stay false. Both were stubs (always false / ignored).
    #[test]
    fn monkeys_read_the_senpou_regions() {
        let Some(regions) = crate::enemy::map_regions_on("boss_12600050") else { return };
        let Some(mut b) = load(12600000, 0.5, 3) else { return };
        assert!(regions.contains_key(&2002820) && !regions.contains_key(&9902820));
        let up = Vec3::Y * 0.5;
        let me_at = regions[&2002821].at + up;
        let wolf_at = regions[&2002825].at + up;
        assert!(!regions[&2002821].contains(wolf_at), "test spots overlap");
        b.st.w = Snapshot { me: me_at, target: wolf_at, regions: Some(regions.clone()), ..Snapshot::battle() };
        let me = Value::Num(b.st.k.target_self);
        let ene = Value::Num(0.0);
        let inside = |b: &mut Brain, t: &Value, id: f64| b.st.ai_method_raw("IsInsideTargetRegion", &[t.clone(), Value::Num(id)]).into_iter().next().unwrap();
        assert!(matches!(inside(&mut b, &me, 2002821.0), Value::Bool(true)));
        assert!(matches!(inside(&mut b, &ene, 2002825.0), Value::Bool(true)));
        assert!(matches!(inside(&mut b, &ene, 2002821.0), Value::Bool(false)));
        assert!(matches!(inside(&mut b, &me, 9902820.0), Value::Bool(false)), "no MSB has 99028xx");
        let ev = Value::Num(b.st.k.point_event);
        b.st.ai_method_raw("SetEventMoveTarget", &[Value::Num(2002859.0)]);
        // GetDist is less the AI chr's radius (0.5 here).
        let want = (regions[&2002859].at - me_at).with_y(0.0).length() as f64 - 0.5;
        let got = b.st.ai_method_raw("GetDist", std::slice::from_ref(&ev))[0].num().unwrap();
        assert!((got - want).abs() < 1e-3 && want > 5.0, "POINT_EVENT is the region: {got} vs {want}");
    }

    /// AddObserveArea / IsInsideObserve / DeleteObserve: a slot's cone answers like
    /// IsInsideTargetEx (136000 Act23: slots 2 / 3, 180 deg at 4 / 10 m). It was always false.
    #[test]
    fn observe_areas_see_wolf() {
        let Some(mut b) = load(10100000, 1.0, 3) else { return };
        b.st.w = Snapshot { dist: 6.0, me: Vec3::ZERO, fwd: Vec3::Z, target: Vec3::new(0.0, 0.0, 6.0), target_fwd: Vec3::Z, ..Snapshot::battle() };
        let me = Value::Num(b.st.k.target_self);
        let add = |b: &mut Brain, slot: f64, dist: f64| {
            b.st.ai_method_raw("AddObserveArea", &[Value::Num(slot), me.clone(), Value::Num(0.0), Value::Num(1.0), Value::Num(180.0), Value::Num(dist)]);
        };
        let inside = |b: &mut Brain, slot: f64| matches!(b.st.ai_method_raw("IsInsideObserve", &[Value::Num(slot)])[0], Value::Bool(true));
        add(&mut b, 2.0, 4.0);
        add(&mut b, 3.0, 10.0);
        assert!(!inside(&mut b, 2.0) && inside(&mut b, 3.0));
        b.st.ai_method_raw("DeleteObserve", &[Value::Num(3.0)]);
        assert!(!inside(&mut b, 3.0) && !inside(&mut b, 9.0));
        // AddObserveRegion(slot, target, region): an MSB region (m20's 2002821 box here).
        let Some(regions) = crate::enemy::map_regions_on("boss_12600050") else { return };
        b.st.w.me = regions[&2002821].at + Vec3::Y * 0.5;
        b.st.w.regions = Some(regions);
        b.st.ai_method_raw("AddObserveRegion", &[Value::Num(30.0), me.clone(), Value::Num(2002821.0)]);
        b.st.ai_method_raw("AddObserveRegion", &[Value::Num(31.0), Value::Num(0.0), Value::Num(2002821.0)]);
        assert!(inside(&mut b, 30.0) && !inside(&mut b, 31.0));
    }

    /// The observe slots' enter / leave edges: INTERUPT_Inside_ObserveArea (16) /
    /// Outside (40) carry the changed slots (GetAreaObserveSlotNum / GetAreaObserveSlot).
    #[test]
    fn observe_areas_report_enter_and_leave() {
        let Some(mut b) = load(10100000, 1.0, 3) else { return };
        b.st.w = Snapshot { dist: 6.0, me: Vec3::ZERO, fwd: Vec3::Z, target: Vec3::new(0.0, 0.0, 6.0), ..Snapshot::battle() };
        let me = Value::Num(b.st.k.target_self);
        b.st.ai_method_raw("AddObserveArea", &[Value::Num(3.0), me, Value::Num(0.0), Value::Num(1.0), Value::Num(180.0), Value::Num(4.0)]);
        b.st.observe_edges();
        assert!(b.st.observe_changed == (vec![], vec![]));
        b.st.w.target = Vec3::new(0.0, 0.0, 2.0);
        b.st.observe_edges();
        assert_eq!(b.st.observe_changed, (vec![3], vec![]));
        let get = |b: &mut Brain, f: &str, a: &[Value]| b.st.ai_method_raw(f, a)[0].num();
        assert_eq!(get(&mut b, "GetAreaObserveSlotNum", &[Value::Num(1.0)]), Some(1.0));
        assert_eq!(get(&mut b, "GetAreaObserveSlot", &[Value::Num(1.0), Value::Num(0.0)]), Some(3.0));
        b.st.observe_edges();
        assert!(b.st.observe_changed == (vec![], vec![]), "no edge while it stays");
        b.st.w.target = Vec3::new(0.0, 0.0, 9.0);
        b.st.observe_edges();
        assert_eq!(b.st.observe_changed, (vec![], vec![3]));
    }

    /// The exe's observe list (re-scout of the AI observe node: kind byte, cur / prev): the
    /// dummy kinds read the AI chr's own dummy; a delete raises nothing; the same id and kind
    /// re-added keeps its state, another kind starts outside; changes list in insertion order;
    /// an area's range is 3D.
    #[test]
    fn observe_slots_follow_the_exe_list() {
        let Some(mut b) = load(10100000, 1.0, 3) else { return };
        let ene = Value::Num(0.0);
        let me = Value::Num(b.st.k.target_self);
        // Dummy 7 two metres up, looking +Z.
        b.st.w = Snapshot { me: Vec3::ZERO, fwd: Vec3::Z, target: Vec3::new(0.0, 2.0, 3.0), dummies: vec![(7, Vec3::new(0.0, 2.0, 0.0), Vec3::Z)], ..Snapshot::battle() };
        let raw = |b: &mut Brain, f: &str, a: &[f64], t: &Value| {
            let mut v: Vec<Value> = vec![Value::Num(a[0]), t.clone()];
            v.extend(a[1..].iter().map(|x| Value::Num(*x)));
            b.st.ai_method_raw(f, &v);
        };
        // ChrDmySphere: strictly within the radius of dummy 7.
        raw(&mut b, "AddObserveChrDmySphere", &[5.0, 7.0, 3.5], &ene);
        raw(&mut b, "AddObserveChrDmySphere", &[6.0, 7.0, 3.0], &ene);
        assert!(b.st.observe_inside(5) && !b.st.observe_inside(6));
        assert_eq!(b.st.observe_dmys(), vec![7]);
        // ChrDmyArea(101, ene, 7, 90, 120, 30, 4): origin 4 m behind the dummy; 3 m ahead is 7 m
        // from it (in), level; behind the origin out; 45 deg up from the origin in, 60 out.
        raw(&mut b, "AddObserveChrDmyArea", &[101.0, 7.0, 90.0, 120.0, 30.0, 4.0], &ene);
        assert!(b.st.observe_inside(101));
        b.st.w.target = Vec3::new(0.0, 2.0, -5.0);
        assert!(!b.st.observe_inside(101), "behind");
        b.st.w.target = Vec3::new(0.0, 2.0, -1.0);
        assert!(!b.st.observe_inside(101), "within the 4 m back-off");
        b.st.w.target = Vec3::new(0.0, 2.0 + 8.0 * 1.1, 4.0);
        assert!(!b.st.observe_inside(101), "above the 45 deg");
        b.st.w.target = Vec3::new(8.0 * 3f32.sqrt() * 0.9, 2.0, 4.0);
        assert!(b.st.observe_inside(101), "inside 60 deg to the side");
        b.st.w.target = Vec3::new(8.0 * 3f32.sqrt() * 1.2, 2.0, 4.0);
        assert!(!b.st.observe_inside(101), "past 60 deg to the side");
        // Edges in insertion order (6, then 5), not by id.
        b.st.observe.clear();
        b.st.observe_in.clear();
        b.st.w.target = Vec3::new(0.0, 2.0, 1.0);
        raw(&mut b, "AddObserveChrDmySphere", &[6.0, 7.0, 3.0], &ene);
        raw(&mut b, "AddObserveChrDmySphere", &[5.0, 7.0, 3.0], &ene);
        b.st.observe_edges();
        assert_eq!(b.st.observe_changed, (vec![6, 5], vec![]), "a fresh slot raises its enter at once");
        // Re-added as it was: no new edge. As another kind (an area that holds it too): reset,
        // so it enters again.
        raw(&mut b, "AddObserveChrDmySphere", &[6.0, 7.0, 3.0], &ene);
        b.st.observe_edges();
        assert_eq!(b.st.observe_changed, (vec![], vec![]));
        b.st.ai_method_raw("AddObserveArea", &[Value::Num(6.0), me.clone(), ene.clone(), Value::Num(0.0), Value::Num(360.0), Value::Num(9.0)]);
        b.st.observe_edges();
        assert_eq!(b.st.observe_changed, (vec![6], vec![]));
        // Deleted: gone without an Outside.
        b.st.ai_method_raw("DeleteObserve", &[Value::Num(5.0)]);
        b.st.observe_edges();
        assert_eq!(b.st.observe_changed, (vec![], vec![]));
        // Deleted and added again in one think: a fresh node, so the enter comes again.
        b.st.ai_method_raw("DeleteObserve", &[Value::Num(6.0)]);
        raw(&mut b, "AddObserveChrDmySphere", &[6.0, 7.0, 3.0], &ene);
        b.st.observe_edges();
        assert_eq!(b.st.observe_changed, (vec![6], vec![]));
        // The area's range is 3D: 2 m up and 1 m ahead is 2.24 m away.
        b.st.ai_method_raw("AddObserveArea", &[Value::Num(8.0), me, ene, Value::Num(0.0), Value::Num(360.0), Value::Num(2.0)]);
        assert!(!b.st.observe_inside(8));
    }

    /// RegisterTriggerRegion leaves a sphere in front of the AI chr (radius 5, 5 s, unit = the
    /// sound's serial); another AI observing the category gets INTERUPT_InvadeTriggerRegion's
    /// list on the frame it enters (category, unit) and the Leave list on the frame it leaves.
    /// Its own region and unobserved categories never count; RemoveTriggerRegionObserver(1000)
    /// leaves observer 1000 (the exe removes category 1).
    #[test]
    fn trigger_regions_report_invade_and_leave() {
        let Some(mut b) = load(10100000, 1.0, 3) else { return };
        b.st.w = Snapshot { me: Vec3::new(0.0, 0.0, 10.0), fwd: Vec3::Z, me_id: 1, sound_id: 1010, sound_serial: 42, ..Snapshot::battle() };
        let me = Value::Num(b.st.k.target_self);
        let get = |b: &mut Brain, f: &str, a: &[Value]| b.st.ai_method_raw(f, a)[0].num();
        assert_eq!(get(&mut b, "GetLatestSoundTargetInstanceID", &[]), Some(42.0));
        b.st.ai_method_raw("RegisterTriggerRegion", &[Value::Num(1000.0), Value::Num(42.0), Value::Num(5.0), Value::Num(5.0), me, Value::Num(1.0), Value::Num(0.0)]);
        let r = b.st.cmd.triggers.pop().expect("a region");
        assert_eq!((r.owner, r.cat, r.unit, r.radius, r.life), (1, 1000, 42, 5.0, 5.0));
        assert!((r.at - Vec3::new(0.0, 0.0, 10.0 + b.st.hit_radius)).length() < 1e-4, "{:?}", r.at);
        // Another chr's region 3 m off; this chr observes 1000.
        let other = crate::ai::TriggerRegion { owner: 7, at: Vec3::new(0.0, 0.0, 3.0), ..r.clone() };
        b.st.w.me = Vec3::ZERO;
        let own_here = crate::ai::TriggerRegion { unit: 43, at: Vec3::ZERO, ..r.clone() };
        b.st.w.triggers = vec![r.clone(), own_here, other.clone(), crate::ai::TriggerRegion { cat: 5, unit: 44, ..other.clone() }];
        b.st.trigger_edges();
        assert!(b.st.trigger_changed == (vec![], vec![]), "no observer yet");
        b.st.ai_method_raw("RegisterTriggerRegionObserver", &[Value::Num(1000.0)]);
        b.st.trigger_edges();
        assert_eq!(b.st.trigger_changed, (vec![(1000, 42)], vec![]));
        assert_eq!(get(&mut b, "GetInvadeTriggerRegionInfoNum", &[]), Some(1.0));
        assert_eq!(get(&mut b, "GetInvadeTriggerRegionCategory", &[Value::Num(0.0)]), Some(1000.0));
        assert_eq!(get(&mut b, "GetInvadeTriggerRegionUnitID", &[Value::Num(0.0)]), Some(42.0));
        b.st.trigger_edges();
        assert!(b.st.trigger_changed == (vec![], vec![]), "no edge while it stays");
        b.st.ai_method_raw("RemoveTriggerRegionObserver", &[Value::Num(1000.0)]);
        b.st.w.me = Vec3::new(0.0, 0.0, 9.0);
        b.st.trigger_edges();
        assert_eq!(b.st.trigger_changed, (vec![], vec![(1000, 42)]));
        assert_eq!(get(&mut b, "GetLeaveTriggerRegionUnitID", &[Value::Num(0.0)]), Some(42.0));
    }

    /// IsInsideTargetEx(observer, target, dir, angle, dist) measures from the observer's facing:
    /// (TARGET_ENE_0, TARGET_SELF, F, 90, 3) = "am I in front of the player" (101200 / 125000
    /// parry gates). It used the AI chr's own facing and ignored the second argument (2026-10-12
    /// round 21). CheckDoesExistPath answers a boolean (the number 0 broke `== true`).
    #[test]
    fn inside_target_ex_looks_from_the_observer() {
        let Some(mut b) = load(10100000, 1.0, 3) else { return };
        // Wolf 2 m in front of the enemy (enemy faces +Z), looking away from it (+Z too).
        b.st.w = Snapshot { dist: 2.0, me: Vec3::ZERO, fwd: Vec3::Z, target: Vec3::new(0.0, 0.0, 2.0), target_fwd: Vec3::Z, ..Snapshot::battle() };
        let me = Value::Num(b.st.k.target_self);
        let ene = Value::Num(0.0);
        let (f, bk) = (Value::Num(1.0), Value::Num(b.st.k.dir_b));
        let ex = |b: &mut Brain, o: &Value, t: &Value, d: &Value| {
            b.st.ai_method_raw("IsInsideTargetEx", &[o.clone(), t.clone(), d.clone(), Value::Num(90.0), Value::Num(3.0)]).into_iter().next().unwrap()
        };
        assert!(matches!(ex(&mut b, &ene, &me, &f), Value::Bool(false)), "Wolf faces away: I am behind him");
        assert!(matches!(ex(&mut b, &ene, &me, &bk), Value::Bool(true)));
        assert!(matches!(ex(&mut b, &me, &ene, &f), Value::Bool(true)), "Wolf is in my front cone");
        b.st.w.target_fwd = -Vec3::Z;
        assert!(matches!(ex(&mut b, &ene, &me, &f), Value::Bool(true)), "Wolf faces me");
        let path = b.st.ai_method_raw("CheckDoesExistPath", &[ene.clone(), f.clone(), Value::Num(0.0), Value::Num(0.0)]);
        assert!(matches!(path.first(), Some(Value::Bool(true))));
    }

    /// An observed SpEffect that ends raises INTERUPT_InactivateSpecialEffect with its id
    /// (GetSpecialEffectInactivateInterruptType); the activate getter stays 0 for it.
    #[test]
    fn an_ending_sp_effect_raises_the_inactivate_interrupt() {
        let Some(mut b) = load(10100000, 1.0, 3) else { return };
        b.st.interrupt = Some((b.st.k.int_sp_off, 5028.0));
        let get = |b: &mut Brain, n: &str| b.st.ai_method_raw(n, &[]).first().and_then(Value::num).unwrap();
        assert_eq!(get(&mut b, "GetSpecialEffectInactivateInterruptType"), 5028.0);
        assert_eq!(get(&mut b, "GetSpecialEffectActivateInterruptType"), 0.0);
        let off = Value::Num(b.st.k.int_sp_off);
        assert!(matches!(b.st.ai_method_raw("IsInterupt", &[off]).first(), Some(Value::Bool(true))));
        // A frame with the SpEffect gone runs the battle goal's Interrupt without error.
        b.st.interrupt = None;
        b.st.observed.insert(5028, vec![-1]);
        let mut s = Snapshot { dist: 3.0, free: true, hp_rate: 1.0, target_hp_rate: 1.0, sp: 600.0, sp_rate: 1.0, ..Snapshot::battle() };
        s.sp_self.insert(200004);
        s.sp_gone = vec![5028];
        b.tick(&s, 1.0 / 60.0);
        let errors: Vec<_> = b.log.iter().filter(|l| !l.starts_with("stub:")).cloned().collect();
        assert!(errors.is_empty(), "{errors:#?}");
    }

    /// The Headless Ape watches its partner (TARGET_EVENT) for 3510900; its own scream's
    /// 3510900 must not switch on the partner branch (Number 1, 510010_battle.lua:653).
    #[test]
    fn the_apes_own_scream_is_not_its_partners() {
        let Some(mut b) = load(51000100, 1.0, 3) else { return };
        let mut s = Snapshot { dist: 6.0, free: true, hp_rate: 1.0, target_hp_rate: 1.0, sp: 600.0, sp_rate: 1.0, ..Snapshot::battle() };
        s.sp_self.insert(200004);
        b.tick(&s, 1.0 / 60.0);
        assert!(b.st.observed.get(&3510900).is_some_and(|w| w == &vec![20]), "{:?}", b.st.observed.get(&3510900));
        s.sp_self.insert(3510900);
        s.sp_new = vec![3510900];
        b.tick(&s, 1.0 / 60.0);
        let n1 = b.st.numbers.get(&1).and_then(Value::num).unwrap_or(0.0);
        assert_eq!(n1, 0.0, "own 3510900 raised the partner interrupt");
    }

    /// Atlas flow.30b-headless-ape (sekiro-rs headless_ape.rs, TARGET_EVENT): the map names the
    /// partner ape (m17 11705880, 2004[11]); its 3510900 coming on raises the Headless Ape's
    /// interrupt (510010_battle.lua:25, Goal.Interrupt sets Number 1). GetDist /
    /// GetHpRate(TARGET_EVENT) read the partner, not Wolf.
    #[test]
    fn atlas_headless_ape_answers_its_partners_call() {
        let Some(mut b) = load(51000100, 1.0, 3) else { return };
        let mut s = Snapshot { dist: 6.0, free: true, hp_rate: 1.0, target_hp_rate: 1.0, sp: 600.0, sp_rate: 1.0, ..Snapshot::battle() };
        s.sp_self.insert(200004);
        let mut partner = EventView { pos: Vec3::new(0.0, 0.0, 12.0), fwd: Vec3::Z, hp_rate: 0.5, sp_left: 400.0, sp_rate: 1.0, ..Default::default() };
        s.event = Some(partner.clone());
        b.tick(&s, 1.0 / 60.0);
        let ev = Value::Num(b.st.k.target_event);
        let d = b.st.ai_method_raw("GetDist", std::slice::from_ref(&ev))[0].num().unwrap();
        assert!((d - (12.0 - 1.0)).abs() < 1e-3, "GetDist(TARGET_EVENT) {d}");
        let hp = b.st.ai_method_raw("GetHpRate", std::slice::from_ref(&ev))[0].num().unwrap();
        assert!((hp - 0.5).abs() < 1e-6, "GetHpRate(TARGET_EVENT) {hp}");
        partner.sp.insert(3510900);
        partner.sp_new = vec![3510900];
        s.event = Some(partner.clone());
        b.tick(&s, 1.0 / 60.0);
        assert_eq!(b.st.numbers.get(&1).and_then(Value::num), Some(1.0), "the partner's 3510900 did not raise the interrupt");
    }

    /// Round 25: a new sound target raises INTERUPT_ChangeSoundTarget through the logic chain;
    /// an Ashina soldier (241000 zako reaction) drops its plan and plays ListenSound 700 after a
    /// 0-0.3 s wait (common_table_ai_common.lua:247), once per sound id (timer 12: 5 s).
    #[test]
    fn a_new_sound_plays_the_listen_reaction() {
        let Some(mut b) = load(10100000, 1.0, 3) else { return };
        let mut s = Snapshot { t_state: crate::stealth::CAUTION, t_prev: crate::stealth::CAUTION, t_kind: 1, visible: false, free: true, hp_rate: 1.0, sp: 600.0, sp_rate: 1.0, ..Snapshot::battle() };
        s.sp_self.insert(241000);
        s.sound_id = 1310;
        s.sound_rank = 0;
        let mut anims = Vec::new();
        for i in 0..60 {
            s.sound_new = i == 10;
            anims.push(b.tick(&s, 1.0 / 60.0).anim);
        }
        let errors: Vec<_> = b.log.iter().filter(|l| !l.starts_with("stub:")).cloned().collect();
        assert!(errors.is_empty(), "{errors:#?}");
        assert!(anims[10..].contains(&Some(700)), "{anims:?}");
        assert!(!anims[..10].contains(&Some(700)));
    }

    /// R33 A2: SpaceCheck's GetExistMeshOnLineDistSpecifyAngle(Ex) read the asked distance back,
    /// so a backstep off a ledge or into a wall always passed. mesh_line_dist stops at a wall
    /// (less the radius) and at a floor step over STEP_HEIGHT; open flat ground is all free.
    #[test]
    fn mesh_line_dist_stops_at_walls_and_ledges() {
        let from = Vec3::new(0.0, 1.0, 0.0);
        let flat = |_: Vec3| 0.0;
        let none = |_: Vec3, _: Vec3, _: f32| None;
        assert_eq!(mesh_line_dist(from, Vec3::Z, 3.0, 0.4, flat, none), 3.0);
        let wall = |_: Vec3, _: Vec3, _: f32| Some(2.0);
        assert!((mesh_line_dist(from, Vec3::Z, 3.0, 0.4, flat, wall) - 1.6).abs() < 1e-5);
        let ledge = |c: Vec3| if c.z > 1.2 { -5.0 } else { 0.0 };
        assert_eq!(mesh_line_dist(from, Vec3::Z, 3.0, 0.0, ledge, none), 1.0);
        let slope = |c: Vec3| c.z * 0.3;
        assert_eq!(mesh_line_dist(from, Vec3::Z, 3.0, 0.0, slope, none), 3.0, "a walkable slope is free");
    }

    /// R33 A4: GetTeamOrder(ORDER_TYPE_Role) was always 0 (Attack), so in a group every
    /// thinkAttr_doAdmirer enemy (101000 / 136000) rushed Wolf. The exe (FUN_140640050) sorts each
    /// target's chrs nearest first: 1 Attack, 1 Torimaki, the rest Kankyaku; per target.
    #[test]
    fn team_roles_go_nearest_first_per_target() {
        let mut world = bevy::ecs::world::World::new();
        let e: Vec<bevy::ecs::entity::Entity> = (0..5).map(|_| world.spawn_empty().id()).collect();
        let roles = crate::enemy::assign_team_roles([(0, 6.0, e[0]), (0, 2.0, e[1]), (0, 9.0, e[2]), (0, 4.0, e[3]), (1, 8.0, e[4])]);
        let got: Vec<f64> = e.iter().map(|x| roles[x]).collect();
        assert_eq!(got, vec![2.0, 0.0, 2.0, 1.0, 0.0]);
        let Some(mut b) = load(10100000, 1.0, 3) else { return };
        b.st.w = Snapshot { team_role: 2.0, ..Snapshot::battle() };
        let role = b.st.ai_method_raw("GetTeamOrder", &[Value::Num(0.0)]).first().and_then(Value::num);
        assert_eq!(role, Some(2.0));
    }

    /// R33 A1: IsLockOnTarget(TARGET_ENE_0 / TARGET_LOCALPLAYER, TARGET_SELF) is Wolf's lock-on on
    /// the AI chr (the Rappa's 136000_battle.lua:116 keeps its distance then); it always read false.
    #[test]
    fn is_lock_on_target_is_wolfs_lock_on_me() {
        let Some(mut b) = load(10100000, 1.0, 3) else { return };
        let (ene, me) = (Value::Num(b.st.k.target_ene_0), Value::Num(b.st.k.target_self));
        let ask = |b: &mut Brain, x: &Value, y: &Value| b.st.ai_method_raw("IsLockOnTarget", &[x.clone(), y.clone()]).first().map(|v| matches!(v, Value::Bool(true)));
        b.st.w = Snapshot { locked_on: true, ..Snapshot::battle() };
        assert_eq!(ask(&mut b, &ene, &me), Some(true));
        assert_eq!(ask(&mut b, &me, &ene), Some(false), "the AI chr locks on to nothing");
        b.st.w.locked_on = false;
        assert_eq!(ask(&mut b, &ene, &me), Some(false));
    }

    /// GetDistYSigned / GetDistY give the target's height over the AI chr (both read 0 before).
    #[test]
    fn dist_y_is_the_target_height_over_me() {
        let Some(mut b) = load(10100000, 1.0, 3) else { return };
        b.st.w = Snapshot { me: Vec3::new(0.0, 1.0, 0.0), target: Vec3::new(0.0, 4.5, 3.0), ..Snapshot::battle() };
        let ene = Value::Num(0.0);
        for name in ["GetDistYSigned", "GetDistY"] {
            let v = b.st.ai_method_raw(name, std::slice::from_ref(&ene)).first().and_then(Value::num).unwrap();
            assert!((v - 3.5).abs() < 1e-6, "{name} {v}");
        }
    }

    /// The native goals (CommonAttack successDist, LeaveTarget, KeepDist) measure the same
    /// distance as the scripts' GetDist: centre distance minus the AI chr's own radius (the
    /// exe's goal updates all call the GetDist thunk 0x1405e2c10), not the centre distance
    /// (2026-10-12 bug hunt).
    #[test]
    fn native_goals_measure_distance_like_get_dist() {
        let Some(mut b) = load(10100000, 1.0, 3) else { return };
        b.st.w = Snapshot { dist: 2.5, free: true, hp_rate: 1.0, target_hp_rate: 1.0, sp: 600.0, sp_rate: 1.0, ..Snapshot::battle() };
        let get_dist = b.st.ai_method_raw("GetDist", &[]).first().and_then(Value::num).unwrap();
        assert!((b.st.goal_dist(None) - 1.5).abs() < 1e-6);
        assert!((get_dist - b.st.goal_dist(None)).abs() < 1e-6, "GetDist {get_dist}");
    }

    /// Patrol stepping by PatrolType (FUN_1405afe20 / table 0x143af9550): 0 loops, 1 turns at
    /// the last point, 5 ends with cur -1; arrivals match by region (a route passing one region
    /// twice gets its arrival both times).
    #[test]
    fn patrol_points_step_by_their_type() {
        let pts = vec![Vec3::ZERO, Vec3::X, Vec3::Z];
        let walk = |kind: i64| {
            let mut p = Patrol::new(kind, pts.clone(), vec![], vec![]);
            (0..5).map(|_| {
                p.step(0);
                p.cur
            }).collect::<Vec<_>>()
        };
        assert_eq!(walk(0), [1, 2, 0, 1, 2]);
        assert_eq!(walk(1), [1, 2, 1, 0, 1]);
        assert_eq!(walk(5), [1, 2, -1, -1, -1]);
        assert_eq!(Patrol::new(6, pts.clone(), vec![], vec![]).cur, -1, "type 6 never moves");
        let mut p = Patrol::new(5, pts.clone(), vec![7, 8, 7], vec![(7, 3000, 2.0)]);
        assert_eq!((p.arrival(0), p.arrival(1), p.arrival(2)), (Some((3000, 2.0)), None, Some((3000, 2.0))));
        assert!(!p.route_end());
        p.yaws = vec![0.0, 1.0, 2.0];
        p.step(0);
        p.step(0);
        assert_eq!(p.new_home, None);
        p.step(0);
        assert!(p.route_end() && p.prev == 2);
        // Its end is its new home pose (FUN_1405c2710 -> FUN_1409f60c0).
        assert_eq!(p.new_home, Some((Vec3::Z, 2.0)));
    }

    /// A move goal with the guard action 9910 as its last param (LeaveTarget(4, TARGET_ENE_0, 5,
    /// TARGET_ENE_0, true, 9910)) moves in the guard stance; with -1 it does not.
    #[test]
    fn a_move_goal_with_9910_moves_guarded() {
        let Some(mut b) = load(10100000, 1.0, 3) else { return };
        b.st.w = Snapshot { dist: 2.5, free: true, hp_rate: 1.0, target_hp_rate: 1.0, sp: 600.0, sp_rate: 1.0, ..Snapshot::battle() };
        let t = Value::Num(b.st.k.target_ene_0);
        for (guard, want) in [(9910.0, true), (-1.0, false)] {
            let g = b.st.new_goal(Some(2016), 4.0, vec![t.clone(), Value::Num(5.0), t.clone(), Value::Bool(true), Value::Num(guard)]);
            b.st.cmd = Command::default();
            b.st.native_update(g, Native::Leave);
            assert_eq!((b.st.cmd.mv, b.st.cmd.guard), (Some('B'), want), "guard {guard}");
        }
    }

    /// A goal whose life runs out while it goes on fails (and fails its parent) unless it or a
    /// goal above it has SetLifeEndSuccess (FUN_1405d06c0): a timed-out approach no longer lets
    /// the queued attack swing from out of reach (2026-10-12 bug hunt).
    #[test]
    fn a_timed_out_goal_fails_unless_life_end_success() {
        let Some(mut b) = load(10100000, 0.4, 3) else { return };
        b.st.w = Snapshot { dist: 10.0, free: true, hp_rate: 1.0, target_hp_rate: 1.0, sp: 600.0, sp_rate: 1.0, ..Snapshot::battle() };
        let approach = |st: &mut AiState| st.new_goal(Some(2019), 0.1, vec![Value::Num(0.0), Value::Num(0.0), Value::Num(1.0)]);
        let g = approach(&mut b.st);
        let mut r = CONTINUE;
        for _ in 0..10 {
            r = tick_goal(&mut b.vm, &mut b.st, g, 0.05).unwrap();
            if r != CONTINUE {
                break;
            }
        }
        assert_eq!(r, FAILED, "timed out 9 m short");
        let g = approach(&mut b.st);
        b.st.goal_mut(g).life_end_success = true;
        let mut r = CONTINUE;
        for _ in 0..10 {
            r = tick_goal(&mut b.vm, &mut b.st, g, 0.05).unwrap();
            if r != CONTINUE {
                break;
            }
        }
        assert_eq!(r, SUCCESS, "SetLifeEndSuccess");
    }

    #[test]
    fn ochimusha_loads_and_acts() {
        let Some(mut b) = load(10100000, 0.4, 3) else { return };
        let mut s = Snapshot { dist: 2.0, free: true, hp_rate: 1.0, target_hp_rate: 1.0, sp: 600.0, sp_rate: 1.0, ..Snapshot::battle() };
        s.sp_self.insert(200004);
        let acted = (0..60).any(|_| {
            let c = b.tick(&s, 1.0 / 60.0);
            c.anim.is_some() || c.mv.is_some()
        });
        let errors: Vec<_> = b.log.iter().filter(|l| !l.starts_with("stub:")).cloned().collect();
        assert!(errors.is_empty() && acted, "{errors:#?}; goals: {}", b.describe());
    }

    /// 504000_battle.lua Goal.Update: every frame all its flags off, then 19625680 on (no
    /// observe area, Wolf without 3504001-4) - written for the fight and seen by IsEventFlag.
    #[test]
    fn the_ai_sets_event_flags() {
        let Some(mut b) = load(50400000, 0.5, 3) else { return };
        let mut s = Snapshot { dist: 4.0, free: true, hp_rate: 1.0, target_hp_rate: 1.0, sp: 600.0, sp_rate: 1.0, ..Snapshot::battle() };
        s.sp_self.insert(200004);
        let set = (0..30).find_map(|_| {
            let c = b.tick(&s, 1.0 / 60.0);
            c.flags.iter().rposition(|f| *f == (19625680, true)).map(|i| c.flags[..i].to_vec())
        });
        let before = set.unwrap_or_else(|| panic!("19625680 never set; goals: {}", b.describe()));
        assert!(before.contains(&(19625685, false)) && before.contains(&(19625693, false)), "{before:?}");
    }

    /// SHINOBI_THINK=51000000 cargo test think_trace -- --ignored --nocapture : one think row's
    /// goals, requests and log (stubs too) over 3 s at 3 m, free.
    #[test]
    #[ignore]
    fn think_trace() {
        let id = std::env::var("SHINOBI_THINK").ok().and_then(|v| v.parse().ok()).unwrap_or(10200000);
        let Some(mut b) = load(id, 0.5, 7) else { return };
        let mut s = Snapshot { dist: 3.0, free: true, hp_rate: 1.0, target_hp_rate: 1.0, sp: 1000.0, sp_rate: 1.0, ..Snapshot::battle() };
        s.sp_self.insert(200004);
        // SHINOBI_NINSATSU=<left>,<max>: a boss phase (Enemy::ninsatsu).
        if let Some((l, m)) = std::env::var("SHINOBI_NINSATSU").ok().and_then(|v| v.split_once(',').and_then(|(l, m)| Some((l.parse().ok()?, m.parse().ok()?)))) {
            (s.ninsatsu, s.ninsatsu_max) = (l, m);
        }
        let mut last = String::new();
        for i in 0..180 {
            let c = b.tick(&s, 1.0 / 60.0);
            let line = format!("{} | anim {:?} mv {:?}", b.describe(), c.anim, c.mv);
            if line != last {
                println!("{i:3} {line}");
                last = line;
            }
        }
        let mut seen = std::collections::BTreeSet::new();
        for l in b.log.iter().filter(|l| seen.insert((*l).clone())) {
            println!("log: {l}");
        }
    }

    /// cargo test think_sweep -- --ignored --nocapture : every placed think row (roster.json) at
    /// a few distances; prints its script errors / unknown goals and whether it ever acted.
    #[test]
    #[ignore]
    fn think_sweep() {
        let Ok(text) = std::fs::read_to_string(crate::paths::root().join("extracted/enemies/roster.json")) else { return };
        let roster: Json = serde_json::from_str(&text).unwrap();
        let mut thinks = std::collections::BTreeMap::new();
        for e in roster.as_array().unwrap() {
            for p in e["placements"].as_array().into_iter().flatten() {
                if let Some(t) = p["think"].as_i64().filter(|t| *t > 0) {
                    thinks.entry(t).or_insert_with(|| e["chr"].as_str().unwrap_or("").to_string());
                }
            }
        }
        for (id, chr) in thinks {
            let Some(row) = think(id) else { continue };
            let dir = crate::paths::root().join("extracted/script");
            let mut b = match Brain::load(&dir, id, &row, 0.5, 7) {
                Ok(b) => b,
                Err(e) => {
                    println!("{chr} {id}: LOAD {e}");
                    continue;
                }
            };
            let (mut anim, mut mv) = (0, 0);
            for i in 0..1200 {
                let mut s = Snapshot { dist: [2.0, 5.0, 10.0][i / 400], free: true, hp_rate: 1.0, target_hp_rate: 1.0, sp: 1000.0, sp_rate: 1.0, ..Snapshot::battle() };
                s.sp_self.insert(200004);
                s.anim_done = i % 40 == 0;
                let c = b.tick(&s, 1.0 / 60.0);
                anim += c.anim.is_some() as u32;
                mv += c.mv.is_some() as u32;
            }
            let mut seen = std::collections::BTreeSet::new();
            let errs: Vec<_> = b.log.iter().filter(|l| !l.starts_with("stub:") && seen.insert((*l).clone())).take(4).cloned().collect();
            println!("{chr} {id}: anims {anim} moves {mv} {errs:?}");
        }
    }

    /// cargo test ai_stubs -- --ignored --nocapture : engine API the scripts reached that is stubbed.
    #[test]
    #[ignore]
    fn ai_stubs() {
        let Some(mut b) = brain() else { return };
        for i in 0..2000 {
            let mut s = Snapshot { dist: (i % 9) as f32, free: i % 3 == 0, hp_rate: 1.0, target_hp_rate: 1.0, sp: 1000.0, sp_rate: 1.0, ..Snapshot::battle() };
            s.sp_self.insert(200004);
            if i % 50 == 0 {
                s.sp_self.insert([200200, 200201, 200205, 200206, 200210, 200211, 200215][(i / 50) % 7]);
                s.interrupted = true;
            }
            s.flags = vec![23, 86, 79, 78];
            s.anim_done = i % 7 == 0;
            b.tick(&s, 1.0 / 60.0);
        }
        println!("{:#?}", b.log);
    }

    #[test]
    fn parry_timing_runs_goal_parry() {
        let Some(mut b) = brain() else { return };
        // Free, facing the player 2 m away, no clash history: Goal.Parry -> EndureAttack 3100 (guard).
        let mut s = Snapshot { dist: 2.0, free: true, hp_rate: 1.0, target_hp_rate: 1.0, sp: 1000.0, sp_rate: 1.0, parry_timing: true, ..Snapshot::battle() };
        s.sp_self.insert(200004);
        s.sp_self.insert(221000); // parry rank 0 (c1020 resident)
        let c = b.tick(&s, 1.0 / 60.0);
        assert_eq!(c.anim, Some(3100), "{c:?}; goals: {}; log: {:?}", b.describe(), b.log);
        // A thrust (109970) from a rank-0 parrier is always deflected (3101).
        let Some(mut b) = brain() else { return };
        s.sp_target.insert(109970);
        let c = b.tick(&s, 1.0 / 60.0);
        assert_eq!(c.anim, Some(3101), "{c:?}; goals: {}", b.describe());
    }

    #[test]
    fn the_parry_guard_cuts_a_hit_reaction_at_flag_122() {
        // In a hit reaction (not free): Goal.Parry's EndureAttack 3100 waits for TAE flag 122
        // (a000_008010 opens it at 0.23 s, its attack window 86 only at 0.5 s), then is the
        // HKS's endure request (env 123).
        let Some(mut b) = brain() else { return };
        let mut s = Snapshot { dist: 2.0, hp_rate: 1.0, target_hp_rate: 1.0, sp: 1000.0, sp_rate: 1.0, parry_timing: true, ..Snapshot::battle() };
        s.sp_self.insert(200004);
        s.sp_self.insert(221000);
        let c = b.tick(&s, 1.0 / 60.0);
        assert_eq!(c.anim, None, "no cancel window yet; goals: {}", b.describe());
        s.parry_timing = false;
        s.flags = vec![FLAG_ENDURE];
        let c = b.tick(&s, 1.0 / 60.0);
        assert_eq!((c.anim, c.endure), (Some(3100), true), "goals: {}", b.describe());
    }

    #[test]
    fn deflected_enemy_counters_with_kengeki() {
        let Some(mut b) = brain() else { return };
        // Enemy got deflected (200200) while 1.5 m away: Kengeki picks a counter
        // (3060/3061/3063, ComboFinal) that may start at the bound anim's flag 23.
        let mut s = Snapshot { dist: 1.5, hp_rate: 1.0, target_hp_rate: 1.0, sp: 1000.0, sp_rate: 1.0, interrupted: true, ..Snapshot::battle() };
        s.sp_self.insert(200004);
        s.sp_self.insert(200200);
        let c = b.tick(&s, 1.0 / 60.0);
        assert_eq!(c.anim, None, "must wait for the cancel flag");
        s.interrupted = false;
        s.flags = vec![23];
        let c = b.tick(&s, 1.0 / 60.0);
        assert!(matches!(c.anim, Some(3060 | 3061 | 3063)), "{c:?}; goals: {}; log: {:?}", b.describe(), b.log);
    }
}
