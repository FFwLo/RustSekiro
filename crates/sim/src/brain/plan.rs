//! What the boss does next: a [`Plan`] of 1 to 3 [`Step`]s, each either one of his own attacks
//! (or steps) or a [`Tactic`] run through the engine's own movement goals (waiting, guarding,
//! moving to a distance band, strafing, keeping or leaving a range).
//!
//! A plan has a text form that hosted models answer with: steps joined by `>`, with consecutive
//! attack ids joined by `+` (they run as one combo chain), for example `walk_in4>hold2>3013` or
//! `3000+3001`. Tactic tokens are the name and the number that picks its variant: `hold3.5`,
//! `walk_in2.5`, `strafe_left`.
//!
//! Distances are metres from Wolf (centre to centre, the observation's `dist`), never map
//! coordinates, so a tactic means the same thing on any floor.

use serde::Serialize;

use crate::lua_ai::ActGoal;

/// A movement tactic. Each runs through one native goal of the AI runtime
/// (`lua_ai::runtime::NativeGoal`) with a lifetime cap, after which it counts as done.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Tactic {
    /// Face Wolf and let the battle script answer an incoming hit for up to 1.5 s.
    Deflect,
    /// Wait for a nearby attack wind-up, then request the boss's own back step.
    DodgeBack,
    /// Stand still facing Wolf for this many seconds (Wait, goal 2000).
    Hold(f32),
    /// Hold the guard for this many seconds (Guard, 2101, action 3100).
    Guard(f32),
    /// Walk toward Wolf until within this distance (MoveToSomewhere, 2019, walking).
    WalkIn(f32),
    /// Run toward Wolf until within this distance (MoveToSomewhere, 2019, running).
    RunIn(f32),
    /// Walk around Wolf to the boss's left for about 2 s (SidewayMove, 2017).
    StrafeLeft,
    /// Walk around Wolf to the boss's right for about 2 s (SidewayMove, 2017).
    StrafeRight,
    /// Walk backwards, facing Wolf, until this far away (LeaveTarget, 2016, walking).
    BackOff(f32),
    /// Turn away and run until this far away (LeaveTarget, 2016, running, facing away).
    Retreat(f32),
    /// Hold 4 to 6 m from Wolf for 2.5 s, moving as needed (KeepDist, 2018, walking).
    KeepRange,
}

/// The goal ids' global names in `goal_list.lua`.
const WAIT: &str = "GOAL_COMMON_Wait";
const GUARD: &str = "GOAL_COMMON_Guard";
const MOVE_TO: &str = "GOAL_COMMON_MoveToSomewhere";
const SIDEWAY: &str = "GOAL_COMMON_SidewayMove";
const KEEP_DIST: &str = "GOAL_COMMON_KeepDist";
const LEAVE: &str = "GOAL_COMMON_LeaveTarget";

/// The action id of his guard (his `Parry` goal requests the same).
const GUARD_ACTION: f64 = 3100.0;

impl Tactic {
    /// Every tactic variant a plan may use, in the order they are offered.
    pub const ALL: [Tactic; 17] = [
        Tactic::Deflect,
        Tactic::DodgeBack,
        Tactic::Hold(1.0),
        Tactic::Hold(2.0),
        Tactic::Hold(3.5),
        Tactic::Guard(1.0),
        Tactic::Guard(2.5),
        Tactic::WalkIn(6.0),
        Tactic::WalkIn(4.0),
        Tactic::WalkIn(2.5),
        Tactic::RunIn(3.0),
        Tactic::StrafeLeft,
        Tactic::StrafeRight,
        Tactic::BackOff(6.0),
        Tactic::BackOff(9.0),
        Tactic::Retreat(12.0),
        Tactic::KeepRange,
    ];

    /// The text form, such as `hold3.5` or `strafe_left`.
    pub fn token(&self) -> String {
        match *self {
            Tactic::Deflect => "deflect".into(),
            Tactic::DodgeBack => "dodge_back".into(),
            Tactic::Hold(s) => format!("hold{s}"),
            Tactic::Guard(s) => format!("guard{s}"),
            Tactic::WalkIn(d) => format!("walk_in{d}"),
            Tactic::RunIn(d) => format!("run_in{d}"),
            Tactic::StrafeLeft => "strafe_left".into(),
            Tactic::StrafeRight => "strafe_right".into(),
            Tactic::BackOff(d) => format!("back_off{d}"),
            Tactic::Retreat(d) => format!("retreat{d}"),
            Tactic::KeepRange => "keep_range".into(),
        }
    }

    pub fn from_token(token: &str) -> Option<Tactic> {
        Self::ALL.into_iter().find(|t| t.token() == token)
    }

    /// What it does, for the instructions and the choice list.
    pub fn label(&self) -> String {
        match *self {
            Tactic::Deflect => "ready for a script-governed guard or deflect, up to 1.5 s".into(),
            Tactic::DodgeBack => {
                "back step when Wolf starts attacking within 2.5 m, up to 1.5 s".into()
            }
            Tactic::Hold(s) => format!("stand still facing Wolf for {s} s"),
            Tactic::Guard(s) => format!("hold your guard for {s} s"),
            Tactic::WalkIn(d) => format!("walk slowly toward Wolf until {d} m away"),
            Tactic::RunIn(d) => format!("run at Wolf until {d} m away"),
            Tactic::StrafeLeft => "walk sideways to your left around Wolf for 2 s".into(),
            Tactic::StrafeRight => "walk sideways to your right around Wolf for 2 s".into(),
            Tactic::BackOff(d) => format!("walk backwards facing Wolf until {d} m away"),
            Tactic::Retreat(d) => format!("turn and run away until {d} m from Wolf"),
            Tactic::KeepRange => "hold 4 to 6 m from Wolf for 2.5 s".into(),
        }
    }

    /// The most seconds it can take (its goal's lifetime); at the cap it counts as done.
    pub fn cap(&self) -> f32 {
        match *self {
            Tactic::Deflect | Tactic::DodgeBack => 1.5,
            Tactic::Hold(s) | Tactic::Guard(s) => s,
            Tactic::WalkIn(_) => 5.0,
            Tactic::RunIn(_) => 4.0,
            Tactic::StrafeLeft | Tactic::StrafeRight => 2.0,
            Tactic::BackOff(_) => 4.0,
            Tactic::Retreat(_) => 5.0,
            Tactic::KeepRange => 2.5,
        }
    }

    /// The native goal that runs it. Parameters follow the scripts' own calls: target
    /// (`TARGET_ENE_0` is 0), then each goal's own list (see `NativeGoal`).
    pub fn goal(&self, step: usize) -> ActGoal {
        let life = f64::from(self.cap());
        let (goal, params, away): (&'static str, Vec<f64>, bool) = match *self {
            Tactic::Deflect => ("PLAN_Deflect", vec![0.0], false),
            Tactic::DodgeBack => ("PLAN_DodgeBack", vec![5201.0, 0.0, 0.0, 2.0, 0.0], false),
            Tactic::Hold(_) => (WAIT, vec![0.0], false),
            Tactic::Guard(_) => (GUARD, vec![GUARD_ACTION, 0.0], false),
            // MoveToSomewhere(target, dirType, dist, turnTarget, walk)
            Tactic::WalkIn(d) => (MOVE_TO, vec![0.0, 0.0, f64::from(d), 0.0, 1.0], false),
            Tactic::RunIn(d) => (MOVE_TO, vec![0.0, 0.0, f64::from(d), 0.0, 0.0], false),
            // SidewayMove(target, right 0 / left 1, angle, turnTarget, walk)
            Tactic::StrafeLeft => (SIDEWAY, vec![0.0, 1.0, 0.0, 0.0, 1.0], false),
            Tactic::StrafeRight => (SIDEWAY, vec![0.0, 0.0, 0.0, 0.0, 1.0], false),
            // LeaveTarget(target, dist, turnTarget, walk)
            Tactic::BackOff(d) => (LEAVE, vec![0.0, f64::from(d), 0.0, 1.0], false),
            Tactic::Retreat(d) => (LEAVE, vec![0.0, f64::from(d), 0.0, 0.0], true),
            // KeepDist(target, minDist, maxDist, turnTarget, walk)
            Tactic::KeepRange => (KEEP_DIST, vec![0.0, 4.0, 6.0, 0.0, 1.0], false),
        };
        ActGoal {
            goal,
            life,
            params,
            number: None,
            approach: None,
            timed: true,
            away,
            face_first: false,
            step: Some(step),
        }
    }
}

/// One step of a plan.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Step {
    /// One of the boss's own action ids (an attack or a step).
    Act(i32),
    Tactic(Tactic),
}

/// What the boss does next, 1 to 3 steps run in order as one act.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(into = "String")]
pub struct Plan {
    pub steps: Vec<Step>,
}

impl From<Plan> for String {
    fn from(p: Plan) -> String {
        p.value()
    }
}

impl Plan {
    /// A plan of the boss's own action ids (a chain when more than one).
    pub fn new(actions: &[i32]) -> Self {
        Self {
            steps: actions.iter().map(|&a| Step::Act(a)).collect(),
        }
    }

    pub fn from_steps(steps: Vec<Step>) -> Self {
        Self { steps }
    }

    pub fn len(&self) -> usize {
        self.steps.len()
    }

    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    /// The action ids in it, in order (tactics left out).
    pub fn actions(&self) -> Vec<i32> {
        self.steps
            .iter()
            .filter_map(|s| match s {
                Step::Act(a) => Some(*a),
                Step::Tactic(_) => None,
            })
            .collect()
    }

    /// The text form (`walk_in4>hold2>3013`, `3000+3001`); empty for an empty plan.
    pub fn value(&self) -> String {
        let mut out = String::new();
        let mut prev_act = false;
        for step in &self.steps {
            match step {
                Step::Act(a) => {
                    if !out.is_empty() {
                        out.push(if prev_act { '+' } else { '>' });
                    }
                    out.push_str(&a.to_string());
                    prev_act = true;
                }
                Step::Tactic(t) => {
                    if !out.is_empty() {
                        out.push('>');
                    }
                    out.push_str(&t.token());
                    prev_act = false;
                }
            }
        }
        out
    }
}

impl std::fmt::Display for Plan {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.value())
    }
}

/// One plan of a [`PlanSet`] and how likely the controller thinks it is the best choice.
#[derive(Debug, Clone, PartialEq)]
pub struct Weighted {
    /// `None` leaves that act to the Lua AI (the controller's `lua` choice).
    pub plan: Option<Plan>,
    /// Relative weight (a probability from the controller, or 1).
    pub weight: f32,
}
