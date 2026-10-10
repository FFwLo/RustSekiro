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
    pub sp_self: HashSet<i64>,
    pub sp_target: HashSet<i64>,
    pub sp_new: Vec<i64>,
    pub hp_rate: f32,
    pub target_hp_rate: f32,
    /// Posture left (Sekiro's "stamina"): max - accumulated posture damage.
    pub sp: f32,
    pub sp_rate: f32,
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
    /// The target state changed in a way that drops the plan (stealth.rs Targeting::replan).
    pub t_replan: bool,
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
    /// The point a move / turn is relative to when it is not the current target: home
    /// (POINT_INITIAL), the home facing (POINT_INIT_POSE).
    pub at: Option<Vec3>,
    /// CLEAR_* bits.
    pub clear: u8,
}

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
    // Native goal state.
    ez: Option<f64>,
    succ: f64,
    turn_time: f64,
    front: f64,
    phase: u8,
    busy: bool,
    req: bool,
    start_bearing: f64,
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
    dir_b: f64,
    dir_l: f64,
    dir_r: f64,
    timing_activate: f64,
    timing_success: f64,
    int_parry: f64,
    int_use_item: f64,
    int_sp: f64,
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
    observed: HashMap<i64, Value>,
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
}

fn ai_obj() -> Value {
    Value::Obj(Obj { kind: OBJ_AI, id: 0 })
}

fn goal_obj(id: u32) -> Value {
    Value::Obj(Obj { kind: OBJ_GOAL, id })
}

fn key(v: Option<&Value>) -> i64 {
    v.and_then(Value::num).unwrap_or(0.0) as i64
}

fn is(v: Option<&Value>, k: f64) -> bool {
    v.and_then(Value::num) == Some(k)
}

fn first(r: Vec<Value>) -> Value {
    r.into_iter().next().unwrap_or_default()
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
            ez: None,
            succ: 0.0,
            turn_time: 0.0,
            front: 0.0,
            phase: 0,
            busy: false,
            req: false,
            start_bearing: 0.0,
        });
        id
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
        match self.pos_of(id) {
            Some(_) if is(id, self.k.point_init_pose) => (self.w.home - self.w.me).with_y(0.0).length() as f64,
            Some(p) => (p - self.w.me).with_y(0.0).length() as f64,
            None => self.dist(),
        }
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

    fn sp_of(&self, target: Option<&Value>) -> &HashSet<i64> {
        if is(target, self.k.target_self) { &self.w.sp_self } else { &self.w.sp_target }
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
            "GetLife" => vec![Value::Num(self.goal(id).life)],
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
            "AddGoalScopedTeamRecord" | "SetFailedEndOption" | "SetLifeEndSuccess" | "SetManagementGoal" => vec![],
            "SetTargetRange" => vec![goal_obj(id)],
            "AddSubGoal" | "AddSubGoal_Front" => {
                let combo = self.def(self.goal(id).kind).is_some_and(|d| d.combo);
                let g = self.goal_from_args(a);
                self.goal_mut(g).parent_combo = combo;
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
        let n = |i: usize| a.get(i).and_then(Value::num).unwrap_or(0.0);
        let num = |v: f64| vec![Value::Num(v)];
        let boolean = |b: bool| vec![Value::Bool(b)];
        let w = &self.w;
        match name {
            "GetDist" | "GetDist_Point" => num(self.dist_to(a.first())),
            "GetDistYSigned" => num(0.0),
            "GetDistAtoB" => num(self.dist()),
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
            "IsTargetGuard" => boolean(w.target_guard),
            "GetSp" => num(w.sp as f64),
            "GetSpRate" => num(w.sp_rate as f64),
            "GetHpRate" => num(if is(a.first(), self.k.target_self) { w.hp_rate } else { w.target_hp_rate } as f64),
            "GetHp" | "GetNinsatsuNum" => num(1.0),
            "GetNpcThinkParamID" => num(self.think_id as f64),
            // NpcThinkParam fields by AI_EXCEL_THINK_PARAM_TYPE__<field name>.
            "GetExcelParam" => {
                let field = self.excel.get(&key(a.first())).cloned().unwrap_or_default();
                num(self.think[field.as_str()].as_f64().unwrap_or(0.0))
            }
            // Event requests default to -1 (FUN_1405b2090 fills the slots with -1.0).
            "GetEventRequest" => num(-1.0),
            "GetTeamOrder" | "GetOddsParam" | "GetOddsParamIdOffset" | "DbgGetForceActIdx" | "DbgGetForceKengekiActIdx"
            | "GetChangeBattleStateCount" | "GetMovePointWaitTime" | "GetSmallActPreWaitTime" | "GetSmallActPostWaitTime"
            | "GetMovePointType" => num(0.0),
            // No patrol route (MSB) and no small acts: NonBattleAct stays at home.
            "GetMovePointNumber" | "GetPrevMovePointNumber" | "GetMovePointAnimId" | "GetSmallActAnimId" => num(-1.0),
            // gap: the engine's move-point effect range; here the distance from home, which is what
            // NonBattleAct / _COMMON_SetBattleActLogic compare with the back-home distances.
            "GetMovePointEffectRange" => num((w.home - w.me).with_y(0.0).length() as f64),
            "GetActTypeOnNonBattleFailedPathEnd" => num(self.think["actTypeOnNonBtlFailedPath"].as_f64().unwrap_or(0.0)),
            "GetToTargetAngle" => num(self.angle_to(a.first())),
            "IsInsideTarget" => boolean(angle_in(self.dir_center(a.get(1)), n(2), self.angle_to(a.first()))),
            "IsInsideTargetEx" => {
                boolean(angle_in(self.dir_center(a.get(2)), n(3), self.angle_to(a.first())) && self.dist_to(a.first()) <= n(4))
            }
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
            "GetLatestSoundTargetID" | "GetLatestSoundTargetInstanceID" => num(w.sound_id as f64),
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
            "IsInsideTargetRegion" | "IsLadderAct" | "GetAreaObserveSlot" | "IsInsideObserve" | "IsStartAttack"
            | "IsValidPlatoon" | "IsPlatoonLeader" | "TeamHelp_IsValidReply" | "IsForceBattleGoal" | "IsChangeInitialPosition"
            | "IsLockOnTarget" => boolean(false),
            "IsExistMeshOnLine" | "CheckDoesExistPathWithSetPoint" | "IsFinishAttackCoolTime" => boolean(true),
            "GetExistMeshOnLineDistSpecifyAngleEx" | "GetExistMeshOnLineDistSpecifyAngle" => {
                vec![a.get(2).cloned().unwrap_or_default()]
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
            | "RegisterTriggerRegionObserver" | "RegisterTriggerRegion" | "SetEventMoveTarget" | "SetEnableEndureCancel_forGoal"
            | "SetEventFlag" | "ClearCorpsePosTarget" => vec![],
            "RequestEmergencyQuickTurn" | "SetAIPredictionMoveTargetSpecifyTargetDir" | "PrintText"
            | "DbgSetLastActIdx" | "DbgSetLastKengekiActIdx" | "DeleteObserve" | "AddObserveArea"
            | "AddObserveRegion" | "AddObserveChrDmyArea" => vec![],
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
                self.observed.insert(key(a.get(1)), a.first().cloned().unwrap_or_default());
                vec![]
            }
            "IsInterupt" => boolean(self.interrupt.is_some_and(|(k, _)| is(a.first(), k))),
            "GetSpecialEffectActivateInterruptType" => num(self.interrupt.map_or(0.0, |(_, sp)| sp)),
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
        if n == Native::ConfirmCaution && self.w.t_state == 1 {
            // FUN_1405c3770: targeting vfuncs 0x28 / 0x30 / 0x38 / 0x40 (gap: taken as the sound,
            // indication, memory and corpse slots).
            let _ = g;
            self.cmd.clear |= CLEAR_SOUND | CLEAR_INDICATION | CLEAR_MEMORY;
        }
    }

    /// The requested -> run phases shared by the action goals; None while the anim runs.
    fn action_phase(&mut self, g: u32, flag: i64) -> Option<i32> {
        let goal = self.goal(g);
        let ez = goal.ez;
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
                let flag = if self.goal(g).parent_combo { 23 } else { 86 };
                if let Some(r) = self.action_phase(g, flag) {
                    return r;
                }
                let succ = self.goal(g).succ;
                if self.w.t > 0.1 && self.w.flags.contains(&23) && self.dist() <= succ {
                    return SUCCESS;
                }
                if self.w.anim_done {
                    return if self.dist() <= succ { SUCCESS } else { FAILED };
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
                let at = self.pos_of(target.as_ref());
                let d = self.dist_to(target.as_ref());
                let (mv, walk) = match n {
                    Native::Approach => {
                        if d <= goal.raw(2, 0.0) {
                            return SUCCESS;
                        }
                        ('F', goal.p.get(4).is_some_and(|v| matches!(v, Value::Bool(true))))
                    }
                    Native::Leave => {
                        if d >= goal.raw(1, 0.0) {
                            return SUCCESS;
                        }
                        ('B', true)
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
                    self.defs.entry(k).or_default();
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
        } else if st.def(kind).is_some() {
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
            return Ok(CONTINUE);
        }
    }
    let goal = st.goal(g);
    if goal.life >= 0.0 && goal.t >= goal.life {
        // Life over: actions that already started run to their own end; an action that
        // never got to start fails (dropping the rest of its combo).
        match nat {
            Some(n) if n.ignore_life() => {
                if !goal.busy {
                    return Ok(FAILED);
                }
            }
            _ => return Ok(SUCCESS),
        }
    }
    if let Some(n) = nat {
        return Ok(st.native_update(g, n));
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

pub struct Brain {
    vm: Vm,
    st: AiState,
    pub log: Vec<String>,
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
            k: Consts {
                target_self: -1.0,
                target_ene_0: 0.0,
                dir_b: 2.0,
                dir_l: 3.0,
                dir_r: 4.0,
                timing_activate: 0.0,
                timing_success: 1.0,
                int_parry: 24.0,
                int_use_item: 12.0,
                int_sp: 43.0,
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
            dir_b: c("AI_DIR_TYPE_B", 2.0),
            dir_l: c("AI_DIR_TYPE_L", 3.0),
            dir_r: c("AI_DIR_TYPE_R", 4.0),
            timing_activate: c("AI_TIMING_SET__ACTIVATE", 0.0),
            timing_success: c("AI_TIMING_SET__UPDATE_SUCCESS", 1.0),
            int_parry: c("INTERUPT_ParryTiming", 24.0),
            int_use_item: c("INTERUPT_UseItem", 12.0),
            int_sp: c("INTERUPT_ActivateSpecialEffect", 43.0),
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
        let (int_parry, int_use_item, int_sp) = (st.k.int_parry, st.k.int_use_item, st.k.int_sp);
        if s.parry_timing {
            interrupt_goals(vm, st, int_parry, 0.0)?;
        }
        if s.target_use_item {
            interrupt_goals(vm, st, int_use_item, 0.0)?;
        }
        for &id in &s.sp_new {
            if st.observed.contains_key(&id) {
                interrupt_goals(vm, st, int_sp, id as f64)?;
            }
        }
        // A target-state change drops the plan (FUN_1405ab7d0 -> FUN_14061f500).
        if std::mem::take(&mut st.replan) || s.t_replan {
            reset_root(vm, st)?;
        }
        if st.root.is_none() {
            plan(vm, st)?;
        }
        let Some(root) = st.root else { return Ok(()) };
        let r = tick_goal(vm, st, root, dt)?;
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
        Ok(())
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
        let f = |s: &str| vm.get_global(&format!("{short}_{s}"));
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
