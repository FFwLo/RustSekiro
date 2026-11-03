//! The actions a plan may name, per boss, and how a plan becomes an act on his battle goal.
//!
//! Each entry is one of the boss's own action ids (the animation ids his Lua acts request),
//! with the approach distance and the AI-number change of the act that opens with it in his
//! battle script (`506001_battle.lua` for the Owl, `101000_battle.lua` for the Ashina soldier).
//! A plan runs exactly like one of his acts: an approach (`Approach_Act_Flex`), then the
//! attacks as `AttackTunableSpin` (a single one) or `ComboAttackTunableSpin` / `ComboRepeat` /
//! `ComboFinal` (a chain), so animations, TAE cancels, combo windows and reactions go through
//! the same goals as his own choices.

use super::plan::{Plan, Step, Tactic};
use crate::lua_ai::{ActChoice, ActGoal, NumberOnActivate};

/// What kind of action an id is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionKind {
    Attack,
    /// A step (`SpinStep`); never approaches first.
    Step,
}

/// One action a plan may name.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BossAction {
    pub id: i32,
    pub label: &'static str,
    pub kind: ActionKind,
    /// Approach until within this many metres (centre to centre, before the scripts subtract
    /// the hit radius) when the action opens a plan; 0 means no approach.
    pub approach: f32,
    /// The AI number his act sets when it runs: for the Owl, number 5, the pressure score
    /// `Kengeki_Activate` reads; for the soldier, number 5 marks his rushes as used. `None`
    /// when the act sets none.
    pub pressure: Option<NumberOnActivate>,
}

/// A named chain of actions offered to hosted controllers as one choice.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Combo {
    pub ids: &'static [i32],
    pub label: &'static str,
}

impl Combo {
    /// The choice value, such as `3000+3001+3002`.
    pub fn value(&self) -> String {
        self.ids
            .iter()
            .map(i32::to_string)
            .collect::<Vec<_>>()
            .join("+")
    }
}

/// A boss's action list and the battle-goal acts that stay reactions.
#[derive(Debug, Clone, Copy)]
pub struct BossActions {
    /// Character id (`c5060`).
    pub chr: &'static str,
    /// Who he is, for the hosted instructions ("the Great Shinobi Owl").
    pub name: &'static str,
    pub actions: &'static [BossAction],
    /// Act indices of his battle goal that are reactions: when his script weighs any of them,
    /// its own pick stands and the plan waits (see `lua_ai::ActHook`).
    pub reflex_acts: &'static [i32],
    /// The most actions one plan may hold.
    pub max_plan: usize,
    /// Chains offered to hosted controllers as one choice, each of at most `max_plan` ids.
    pub combos: &'static [Combo],
    /// His back steps (the habit "what Wolf does after a back step" starts at these).
    pub back_steps: &'static [i32],
    /// The scripted test controller's opener, and the plan it switches to once the opener's
    /// first action has been deflected twice (`ScriptedBrain::for_actions`).
    pub scripted: [&'static [i32]; 2],
    /// Curated two and three step plans offered to hosted controllers as single choices, in
    /// the text form of [`Plan::value`]; tactics and attacks mixed. Each must parse (checked by
    /// a test).
    pub chains: &'static [&'static str],
    /// The tactics offered as single choices (all of [`Tactic::ALL`] for both bosses).
    pub tactics: &'static [Tactic],
}

const fn attack(id: i32, label: &'static str, approach: f32, pressure: f64) -> BossAction {
    BossAction {
        id,
        label,
        kind: ActionKind::Attack,
        approach,
        pressure: Some(NumberOnActivate::Add(5, pressure)),
    }
}

/// An attack whose act sets no AI number.
const fn plain(id: i32, label: &'static str, approach: f32) -> BossAction {
    BossAction {
        id,
        label,
        kind: ActionKind::Attack,
        approach,
        pressure: None,
    }
}

/// An attack whose act sets AI number `slot` to `value`.
const fn setting(id: i32, label: &'static str, approach: f32, slot: i32, value: f64) -> BossAction {
    BossAction {
        pressure: Some(NumberOnActivate::Set(slot, value)),
        ..plain(id, label, approach)
    }
}

const fn step(id: i32, label: &'static str) -> BossAction {
    BossAction {
        id,
        label,
        kind: ActionKind::Step,
        approach: 0.0,
        pressure: Some(NumberOnActivate::Set(5, 0.0)),
    }
}

/// A step whose act sets no AI number.
const fn plain_step(id: i32, label: &'static str) -> BossAction {
    BossAction {
        pressure: None,
        ..step(id, label)
    }
}

/// The Great Shinobi Owl (c5060) in phase 1. Approach distances and pressure from the act that
/// uses each id in `506001_battle.lua` (Act01 to Act15, Act30); descriptions from his TAE and
/// AtkParam rows (`docs/OWL.md`). Firecrackers (3010) use Act07's 3.5 m approach and reset
/// pressure number 5 to zero. The other tool acts (3017 explosive slash, 3030
/// poison balls, 3069 smoke screen) were added with their bullets; their approach distances are
/// ours, read from where their bullets land (the reach of each in `docs/OWL.md`, "Tools"), and
/// their pressure is 0 until the acts that use them are read. Left out: warps, grabs, phase-2
/// acts other than the poison and the deflect counters, which stay reactions.
pub const OWL: BossActions = BossActions {
    chr: "c5060",
    name: "the Great Shinobi Owl",
    actions: &[
        attack(3000, "kesa slash, combo opener", 4.8, 2.0),
        attack(3001, "second slash", 5.8, 7.0),
        attack(3002, "third slash, combo finisher", 4.8, 10.0),
        attack(3003, "jumping forward roll, slash down", 10.7, 9.0),
        attack(3005, "front kick (posture only)", 2.8, 1.0),
        attack(3009, "shuriken", 9.7, 1.0),
        setting(
            3010,
            "firecrackers scattered in front (unblockable bangs)",
            3.5,
            5,
            0.0,
        ),
        attack(3011, "rushing slash", 11.0, 1.0),
        attack(3014, "second shuriken", 9.7, 2.0),
        attack(3015, "rushing slash curving left", 9.0, 5.0),
        attack(3017, "explosive slash (fire)", 5.0, 0.0),
        attack(3018, "shuriken then dash slash", 11.0, 9.0),
        attack(
            3030,
            "poison balls, three rows of poison floor (phase 2)",
            5.0,
            0.0,
        ),
        attack(3031, "recovery-sealing throw", 5.5, 6.0),
        attack(3032, "recovery-sealing throw follow-up", 9.0, 8.0),
        attack(3063, "rushing follow-up, two hits", 4.8, 10.0),
        attack(3069, "smoke screen (blinds, breaks lock-on)", 5.5, 0.0),
        step(5201, "back step"),
        step(5202, "side step (his act picks it with Wolf on his right)"),
        step(5203, "side step (his act picks it with Wolf on his left)"),
    ],
    // Act21 turns to punish a heal or face a Wolf behind him; Act40 answers SpEffect 110125.
    reflex_acts: &[21, 40],
    max_plan: 3,
    combos: &[Combo {
        ids: &[3000, 3001, 3002],
        label: "three-slash combo",
    }],
    back_steps: &[5201],
    // Kick (3005); after two deflected kicks, the recovery-sealing throw (3031).
    scripted: [&[3005], &[3031]],
    tactics: &Tactic::ALL,
    chains: &[
        "deflect>3000+3001",
        "deflect>3005",
        "guard1>deflect>3000",
        "dodge_back>3003",
        "dodge_back>3011",
        "walk_in4>3005",
        "walk_in2.5>3005",
        "walk_in4>3000+3001",
        "hold2>3011",
        "hold1>3003",
        "hold3.5>3018",
        "back_off6>3009",
        "back_off9>3009",
        "retreat12>3009",
        "retreat12>3014",
        "back_off9>3011",
        "strafe_left>3000+3001",
        "strafe_right>3000+3001",
        "strafe_left>3005",
        "strafe_right>3015",
        "strafe_left>3009",
        "walk_in2.5>3031",
        "hold2>3031",
        "back_off6>3010",
        "guard1>3000+3001",
        "guard2.5>3005",
        "run_in3>3000+3001",
        "run_in3>3005",
        "keep_range>3009",
        "keep_range>3017",
        "retreat12>run_in3>3003",
        "retreat12>hold2>3011",
        "walk_in6>walk_in2.5>3005",
        "hold2>walk_in2.5>3031",
        "3009>back_off6",
        "3005>back_off6",
        "3000+3001>retreat12",
        "3005>strafe_left",
        "strafe_left>strafe_right>3003",
        "back_off6>hold2>3017",
        "walk_in4>hold1>3005",
        "3069>retreat12",
        "3010>back_off6",
    ],
};

/// The Ashina soldier with a one-handed sword (c1010, NpcThinkParam 10100000, battle goal
/// 101000). Approach distances, combos and AI numbers from the act that uses each id in
/// `101000_battle.lua` (Act01 to Act09, Act11, Act14, Act15, Act24); descriptions from his
/// BehaviorParam names (the judge ids in his TAE) and AtkParam rows (10100xxx: the thrust 3013
/// cannot be guarded, the low sweep 3014 can be neither guarded nor deflected, the kick and the
/// pounces deal 1 HP).
///
/// - 3001 and 3007 only follow 3000 and 3005 in his acts; as openers they use their act's
///   distance (ours).
/// - The stone throw's act (Act07) never approaches, so neither does a plan opening with it.
/// - His acts set AI number 5 to 1 when he rushes (3010, 3011): his script only weighs the
///   rushes from afar while it is 0 under SpEffect 200050.
/// - Left out: 3004 and 3006 (Act04) and the side steps 5202 and 5203 (Act22), which his
///   script names but his TAE (`c1010.tae`) has no clips for; 3013 toward an event point
///   (Act10) and 3030 (Act12, a tutorial dodge); the deflect counters (3050 to 3076,
///   `Kengeki*`), which stay reactions.
pub const SOLDIER: BossActions = BossActions {
    chr: "c1010",
    name: "an Ashina soldier with a one-handed sword",
    actions: &[
        plain(3000, "sweeping slash, combo opener", 2.5),
        plain(3001, "second sweeping slash (follows 3000)", 2.5),
        plain(3002, "running sweeping slash", 7.0),
        plain(3003, "leaping slash", 7.1),
        plain(3005, "kick (posture only)", 3.7),
        plain(3007, "rising slash (follows the kick)", 3.7),
        plain(3008, "flurry of three slashes", 4.7),
        plain(3009, "stone throw", 0.0),
        setting(3010, "pouncing leap (posture only)", 5.0, 5, 1.0),
        setting(
            3011,
            "running lariat, then a pounce (posture only)",
            7.6,
            5,
            1.0,
        ),
        plain(3012, "left sweep, then an overhead slam", 3.5),
        plain(
            3013,
            "perilous thrust (cannot be guarded; deflect or counter it)",
            5.0,
        ),
        plain(
            3014,
            "perilous low sweep (cannot be guarded or deflected; jump)",
            3.5,
        ),
        plain_step(5201, "back step"),
        plain_step(5211, "short back step (his act uses it within 4 m)"),
    ],
    // Act23 strafes (only with Wolf down, or in the tutorial), Act25 backs away (SpEffect
    // 3170200 on Wolf), Act27 waits and strafes (no path to Wolf, Wolf hanging, or SpEffect
    // 3101540): reactions his script weighs before any distance band.
    reflex_acts: &[23, 25, 27],
    max_plan: 3,
    combos: &[
        Combo {
            ids: &[3000, 3001],
            label: "both sweeping slashes",
        },
        Combo {
            ids: &[3005, 3007],
            label: "kick, then rising slash",
        },
    ],
    back_steps: &[5201, 5211],
    // Both sweeping slashes; after two deflected openers, the kick and rising slash.
    scripted: [&[3000, 3001], &[3005, 3007]],
    tactics: &Tactic::ALL,
    chains: &[
        "deflect>3000+3001",
        "deflect>3005+3007",
        "guard1>deflect>3008",
        "dodge_back>3002",
        "dodge_back>3014",
        "walk_in4>3013",
        "walk_in2.5>3013",
        "walk_in6>3013",
        "hold2>3002",
        "hold1>3002",
        "hold3.5>3013",
        "back_off6>3009",
        "back_off9>3009",
        "retreat12>3009",
        "strafe_left>3005+3007",
        "strafe_right>3005+3007",
        "strafe_left>3000+3001",
        "strafe_right>3008",
        "walk_in4>3000+3001",
        "walk_in2.5>3008",
        "walk_in2.5>3014",
        "hold2>3014",
        "hold1>3008",
        "guard1>3000+3001",
        "guard2.5>3012",
        "back_off6>3002",
        "back_off6>3003",
        "retreat12>run_in3>3003",
        "retreat12>hold2>3002",
        "run_in3>3000+3001",
        "run_in3>3008",
        "keep_range>3009",
        "keep_range>3013",
        "hold1>3003",
        "3000+3001>back_off6",
        "3008>back_off6",
        "3005+3007>strafe_left",
        "3013>back_off6",
        "walk_in4>hold1>3013",
        "strafe_left>strafe_right>3013",
        "back_off6>hold2>3013",
        "walk_in6>walk_in2.5>3008",
        "hold2>walk_in2.5>3013",
        "strafe_right>hold1>3008",
        "back_off9>run_in3>3011",
    ],
};

/// Castle Genichiro (c7100, row 71001000, think 71000000).
/// Distances and chains are from GOAL_Rival_710000_Battle in the castle bundle's
/// `710000_battle.lua`; labels are from c7100.tae judges, BehaviorParam 271000xxx
/// and AtkParam_Npc 71000xxx. Every entry has a decoded a000_<id> clip and TAE.
/// Act01's four-hit branch exceeds our three-step plan cap, so its first three
/// hits and its 3010/3025 alternate finishers are offered separately.
/// Follow-ups retain their parent act's approach if chosen as openers (ours).
/// Act15 sets number 7 to 1; Act24 sets number 2 to 1.
/// Left out: undefined registered Act07/08/18/19/40/41/42/45/46/47,
/// grabs 3020/3021 (Act11's 3020 is a throw start), and lightning 3039/3042/3043
/// (their functions and attack rows belong to c7110, not castle c7100).
/// Kengeki counters and Act16's downed-Wolf punish remain Lua reactions.
/// c7110 has its own battle goal and receives Lua control at the map hand-over.
pub const GENICHIRO: BossActions = BossActions {
    chr: "c7100",
    name: "Genichiro Ashina",
    actions: &[
        // Act01: a000_003000..003003, judges 100..103; 3010 judges 320/321,
        // 3025 judge 272 (perilous thrust) plus Mikiri dummy 940.
        plain(3000, "right kesa slash, combo opener", 3.6),
        plain(3001, "left kesa slash", 3.6),
        plain(3002, "left elbow", 3.6),
        plain(3003, "right body slash, combo finisher", 3.6),
        plain(3010, "two spinning slashes", 3.6),
        plain(3025, "perilous thrust (deflect or Mikiri)", 3.6),
        // Act02/03/10/05/06: a000_003004/5/6/7/16, judges 110/111,
        // 120, 330/331, 140 (unguardable and undeflectable sweep), 120.
        plain(3004, "right then left body slashes", 3.2),
        plain(3005, "draw slash", 2.2),
        plain(3006, "advancing two-slash combo after sheathing bow", 4.8),
        plain(3007, "perilous low sweep from left step (jump)", 5.9),
        plain(3016, "running draw slash", 5.2),
        // Act30 never approaches: a000_003009 bullet judge 500, followed by
        // a000_003044 bullet judges 530..533 (four arrows); also Act24.
        plain(3009, "bow shot", 0.0),
        plain(3044, "jumping four-arrow volley", 0.0),
        // Act34: sweep then a000_003011, bullet judge 500.
        plain(3011, "bow shot after sweep", 5.9),
        // Act15: a000_003014 bullet judge 500, then 003015 judge 150.
        setting(3014, "braking bow shot", 8.9, 7, 1.0),
        setting(3015, "diving slash after braking bow shot", 8.9, 7, 1.0),
        // Act09: a000_003040/41, Floating Passage judges 290..292.
        plain(3040, "Floating Passage, opening four slashes", 4.5),
        plain(3041, "Floating Passage, final three slashes", 4.5),
        // Act11 opener a000_003037 judge 370; Act31's 003045 judge 390.
        plain(3037, "circling slash", 3.2),
        plain(3045, "roundhouse kick after body slash", 3.6),
        // Act20 explicitly offers a000_003062 outside Kengeki: judge 250,
        // perilous back-step thrust, plus Mikiri dummy 940.
        plain(3062, "perilous back-step thrust (deflect or Mikiri)", 3.2),
        // Act22's side steps and Act24's back step have their own TAE clips.
        BossAction {
            pressure: Some(NumberOnActivate::Set(2, 1.0)),
            ..plain_step(5201, "back step")
        },
        plain_step(5202, "right side step"),
        plain_step(5203, "left side step"),
    ],
    // Act21 turns when Wolf is behind; Act28 responds to Wolf's throw/down
    // effects; Act16 punishes downed Wolf. Kengeki returns before the slot.
    reflex_acts: &[16, 21, 28],
    max_plan: 3,
    combos: &[
        Combo {
            ids: &[3000, 3001, 3002],
            label: "kesa slashes then elbow",
        },
        Combo {
            ids: &[3001, 3010, 3025],
            label: "left slash, spinning slashes, perilous thrust (Act01 suffix)",
        },
        Combo {
            ids: &[3014, 3015],
            label: "braking bow shot then diving slash",
        },
        Combo {
            ids: &[3040, 3041],
            label: "Floating Passage",
        },
        Combo {
            ids: &[3009, 3044],
            label: "bow shot then jumping arrow volley",
        },
        Combo {
            ids: &[3003, 3045],
            label: "body slash then roundhouse kick",
        },
        Combo {
            ids: &[3007, 3011],
            label: "perilous sweep then bow shot",
        },
    ],
    back_steps: &[5201],
    scripted: [&[3000, 3001, 3002], &[3009, 3044]],
    // Act23/25/26/27/28 supply strafe, leave, wait and approach goals;
    // Goal.Parry supplies guard/deflect and back steps through his own graph.
    tactics: &Tactic::ALL,
    chains: &[
        "deflect>3000+3001",
        "guard1>3005",
        "dodge_back>3016",
        "walk_in2.5>3000+3001",
        "run_in3>3040+3041",
        "hold1>3007",
        "strafe_left>3004",
        "strafe_right>3025",
        "back_off9>3009+3044",
        "keep_range>3014+3015",
        "retreat12>3009",
        "3005>back_off6",
        "hold2>walk_in4>3016",
        "3007>strafe_right",
        "guard1>deflect>3025",
    ],
};

/// Castle 711000 battle goal Act13/17, Act18 and Act19 respectively.
/// c7100 has no corresponding functions or lightning BehaviorParam rows.
pub const TOMOE: BossActions = BossActions {
    chr: "c7110",
    name: "Genichiro, Way of Tomoe",
    actions: &[
        plain(3000, "right kesa slash", 3.6),
        plain(3001, "left kesa slash", 3.6),
        plain(3002, "left elbow", 3.6),
        plain(3003, "right body slash", 3.6),
        plain(3010, "spinning slashes", 3.6),
        plain(3025, "perilous thrust", 3.6),
        plain(3004, "body slashes", 3.0),
        plain(3005, "draw slash", 2.2),
        plain(3006, "advancing slashes", 4.8),
        plain(3007, "perilous sweep", 5.9),
        plain(3016, "running draw slash", 5.2),
        plain(3040, "Floating Passage opener", 4.5),
        plain(3041, "Floating Passage finisher", 4.5),
        plain(3067, "Passage perilous finisher", 4.5),
        plain(3037, "circling slash", 3.5),
        setting(3012, "bow attack", 0.0, 2, 1.0),
        plain(3014, "braking bow shot", 8.9),
        plain(3015, "diving slash", 8.9),
        setting(3008, "advancing attack", 12.5, 7, 1.0),
        plain(3009, "bow shot", 0.0),
        plain(3011, "bow shot after sweep", 5.9),
        plain(3045, "roundhouse kick", 3.6),
        plain_step(5201, "back step"),
        plain_step(5202, "side step right"),
        plain_step(5203, "side step left"),
        setting(3039, "lightning bow", 0.0, 2, 1.0),
        plain(3043, "advancing lightning jump slash", 8.3),
        plain(3042, "lightning jump slash", 3.5),
    ],
    reflex_acts: &[16, 21, 28],
    max_plan: 3,
    combos: &[
        Combo {
            ids: &[3000, 3001, 3002],
            label: "sword combo",
        },
        Combo {
            ids: &[3040, 3041, 3067],
            label: "Floating Passage with perilous finish",
        },
        Combo {
            ids: &[3014, 3015],
            label: "bow then diving slash",
        },
        Combo {
            ids: &[3012, 3039],
            label: "bow then lightning",
        },
    ],
    back_steps: &[5201],
    scripted: [&[3039], &[3042]],
    tactics: &Tactic::ALL,
    chains: &["walk_in4>3042", "back_off9>3039", "deflect>3000+3001"],
};

/// Why a plan was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanError {
    Empty,
    TooLong(usize),
    Unknown(i32),
    /// A token that is neither an action id nor a tactic.
    UnknownToken(String),
}

impl std::fmt::Display for PlanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PlanError::Empty => write!(f, "empty plan"),
            PlanError::TooLong(n) => write!(f, "plan of {n} steps is too long"),
            PlanError::Unknown(id) => write!(f, "unknown action {id}"),
            PlanError::UnknownToken(_) => write!(f, "unknown step"),
        }
    }
}

/// One entry of the choice list offered to a hosted controller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    /// The text form of a plan (`3005`, `3000+3001`, `walk_in4>3013`).
    pub value: String,
    pub label: String,
}

impl BossActions {
    /// Lua Kengeki callback indices and their attack bodies, from each battle goal.
    /// An empty body is an authored fallback, never an invented no-counter option.
    pub fn counters(&self) -> &'static [(i32, &'static [i32])] {
        match self.chr {
            "c1010" => &[
                (1, &[3050]),
                (2, &[3051]),
                (3, &[3055]),
                (4, &[3056]),
                (5, &[3070]),
                (6, &[3071]),
                (7, &[3075]),
                (8, &[3076]),
                (30, &[5201]),
            ],
            "c5060" => &[
                (1, &[3061]),
                (2, &[3066]),
                (3, &[3064]),
                (4, &[3068, 3017]),
                (5, &[3063]),
                (6, &[3060]),
                (7, &[3065]),
                (8, &[3069]),
                (9, &[3090]),
                (10, &[3091]),
                (11, &[3026]),
                (12, &[3005]),
                (26, &[]),
            ],
            "c7100" => &[
                (1, &[3050]),
                (2, &[5201, 3044]),
                (3, &[3009]),
                (4, &[3055]),
                (7, &[3060]),
                (9, &[3018, 3015]),
                (10, &[3065]),
                (13, &[3075]),
                (14, &[3076]),
                (15, &[3031, 3019, 3029, 3036]),
                (17, &[3071]),
                (18, &[3004]),
                (19, &[3044]),
                (20, &[5202, 3007]),
                (21, &[3010]),
                (30, &[3063]),
                (31, &[3068]),
                (32, &[3018, 3015]),
                (33, &[3018, 3019, 3029]),
                (34, &[3037, 3020]),
                (35, &[3016]),
                (38, &[3030, 3067, 3025]),
                (39, &[3034, 3036, 3015]),
                (40, &[3028]),
                (43, &[3062]),
                (44, &[3067]),
                (45, &[3045]),
                (46, &[3039]),
                (47, &[3038]),
            ],
            "c7110" => &[
                (1, &[3050]),
                (2, &[5201]),
                (4, &[3055]),
                (7, &[3060]),
                (9, &[3018, 3043]),
                (10, &[3065]),
                (13, &[3075]),
                (14, &[3076]),
                (15, &[3031, 3019, 3029, 3036]),
                (17, &[3076]),
                (18, &[3004]),
                (20, &[5202, 3007]),
                (21, &[3010]),
                (22, &[3018, 3087]),
                (30, &[3063]),
                (31, &[3068]),
                (32, &[3018, 3043]),
                (33, &[3018, 3019, 3029]),
                (34, &[3037, 3020]),
                (35, &[3016]),
                (38, &[3030, 3067, 3025, 3032]),
                (39, &[3034, 3036, 3015]),
                (40, &[3028]),
                (41, &[3020]),
                (43, &[3062]),
                (44, &[3067]),
                (45, &[3042]),
                (46, &[3039]),
                (47, &[3043]),
            ],
            _ => &[],
        }
    }

    pub fn counter_ids(&self) -> std::collections::BTreeSet<i32> {
        self.counters()
            .iter()
            .flat_map(|(_, ids)| ids.iter().copied())
            // The scripts also name variants without a decoded clip in this character.
            // They remain Lua callbacks, but are not coach-editable moves.
            .filter(|id| match self.chr {
                "c1010" => ![3070, 3076].contains(id),
                "c7100" => *id != 3039,
                "c7110" => ![3060, 3065, 3075, 3020].contains(id),
                _ => true,
            })
            .collect()
    }

    pub fn get(&self, id: i32) -> Option<&BossAction> {
        self.actions.iter().find(|a| a.id == id)
    }

    pub fn ids(&self) -> Vec<i32> {
        self.actions.iter().map(|a| a.id).collect()
    }

    /// The boss's actions for a character id, if a table exists.
    pub fn for_chr(chr: &str) -> Option<&'static BossActions> {
        [&OWL, &SOLDIER, &GENICHIRO, &TOMOE]
            .into_iter()
            .find(|a| a.chr == chr)
    }

    /// Reads a plan's text form (`walk_in4>hold2>3013`, `3000+3001`) against this table.
    /// Every action id must be in the table and written plainly (`03000` is refused).
    pub fn parse(&self, value: &str) -> Result<Plan, PlanError> {
        let mut steps = Vec::new();
        for part in value.split('>') {
            if let Some(t) = Tactic::from_token(part) {
                steps.push(Step::Tactic(t));
                continue;
            }
            for id in part.split('+') {
                let n: i32 = id
                    .parse()
                    .map_err(|_| PlanError::UnknownToken(id.to_string()))?;
                if id != n.to_string() {
                    return Err(PlanError::UnknownToken(id.to_string()));
                }
                if self.get(n).is_none() {
                    return Err(PlanError::Unknown(n));
                }
                steps.push(Step::Act(n));
            }
        }
        if steps.len() > self.max_plan {
            return Err(PlanError::TooLong(steps.len()));
        }
        Ok(Plan::from_steps(steps))
    }

    /// A step's label for a multi-step choice: an action's label up to its first bracket or
    /// comma, a tactic's own.
    fn step_label(&self, step: &Step) -> String {
        match step {
            Step::Act(id) => self.get(*id).map_or_else(String::new, |a| {
                a.label
                    .split([',', '('])
                    .next()
                    .unwrap_or(a.label)
                    .trim()
                    .to_string()
            }),
            Step::Tactic(t) => t.label(),
        }
    }

    /// Every choice offered to a hosted controller, without `lua`: each of his actions, his
    /// named combos, each single tactic and his curated chains. At most 253 (the Decisions
    /// API takes 2 to 255 values, one of which is `lua`).
    pub fn choices(&self) -> Vec<Choice> {
        let mut out: Vec<Choice> = self
            .actions
            .iter()
            .map(|a| Choice {
                value: a.id.to_string(),
                label: a.label.to_string(),
            })
            .collect();
        out.extend(
            self.combos
                .iter()
                .filter(|c| c.ids.len() <= self.max_plan)
                .map(|c| Choice {
                    value: c.value(),
                    label: c.label.to_string(),
                }),
        );
        out.extend(self.tactics.iter().map(|t| Choice {
            value: t.token(),
            label: t.label(),
        }));
        out.extend(self.chains.iter().filter_map(|value| {
            let plan = self.parse(value).ok()?;
            let label = plan
                .steps
                .iter()
                .map(|s| self.step_label(s))
                .collect::<Vec<_>>()
                .join(", then ");
            Some(Choice {
                value: (*value).to_string(),
                label,
            })
        }));
        out
    }

    /// Turns a plan of action ids into an act on the battle goal (see [`Self::act_for_plan`]).
    pub fn act_for(&self, plan: &[i32], hit_radius: f32) -> Result<ActChoice, PlanError> {
        self.act_for_plan(&Plan::new(plan), hit_radius)
    }

    /// Turns a plan into an act on the battle goal. `hit_radius` is the boss's NpcParam
    /// `hitRadius`, which his acts subtract from every distance.
    ///
    /// Each run of consecutive action ids is one act: an approach when it opens with an attack
    /// that has an approach distance, then a single attack (`AttackTunableSpin`) or a chain
    /// (`ComboAttackTunableSpin`, `ComboRepeat`, ..., `ComboFinal`); a tactic is one native
    /// goal, not asked to approach.
    pub fn act_for_plan(&self, plan: &Plan, hit_radius: f32) -> Result<ActChoice, PlanError> {
        if plan.is_empty() {
            return Err(PlanError::Empty);
        }
        if plan.len() > self.max_plan {
            return Err(PlanError::TooLong(plan.len()));
        }
        let mut goals = Vec::new();
        let mut i = 0;
        while i < plan.steps.len() {
            if let Step::Tactic(t) = plan.steps[i] {
                let mut goal = t.goal(i);
                if t == Tactic::DodgeBack {
                    goal.params[0] = f64::from(*self.back_steps.last().unwrap_or(&5201));
                }
                goals.push(goal);
                i += 1;
                continue;
            }
            let start = i;
            let mut run = Vec::new();
            while let Some(Step::Act(id)) = plan.steps.get(i) {
                run.push(self.get(*id).copied().ok_or(PlanError::Unknown(*id))?);
                i += 1;
            }
            let first = run[0];
            let approach = (first.kind == ActionKind::Attack && first.approach > 0.0)
                .then(|| f64::from(first.approach - hit_radius).max(0.5));
            let last = run.len() - 1;
            for (k, a) in run.iter().enumerate() {
                let id = f64::from(a.id);
                // TARGET_ENE_0 = 0, AI_DIR_TYPE_B = 2 (ai_define.lua); 9999 is the scripts'
                // "any distance" success range.
                let (goal, params): (&'static str, Vec<f64>) = match a.kind {
                    ActionKind::Step => ("GOAL_COMMON_SpinStep", vec![id, 0.0, 0.0, 2.0, 0.0]),
                    ActionKind::Attack if last == 0 => (
                        "GOAL_COMMON_AttackTunableSpin",
                        vec![id, 0.0, 9999.0, 0.0, 0.0, 0.0, 0.0],
                    ),
                    ActionKind::Attack if k == 0 => (
                        "GOAL_COMMON_ComboAttackTunableSpin",
                        vec![id, 0.0, 9999.0, 0.0, 0.0, 0.0, 0.0],
                    ),
                    ActionKind::Attack if k == last => {
                        ("GOAL_COMMON_ComboFinal", vec![id, 0.0, 9999.0, 0.0, 0.0])
                    }
                    ActionKind::Attack => ("GOAL_COMMON_ComboRepeat", vec![id, 0.0, 9999.0, 0.0]),
                };
                goals.push(ActGoal {
                    goal,
                    life: if a.kind == ActionKind::Step {
                        3.0
                    } else {
                        10.0
                    },
                    params,
                    number: a.pressure,
                    approach: if k == 0 { approach } else { None },
                    timed: false,
                    away: false,
                    face_first: a.kind == ActionKind::Attack,
                    step: Some(start + k),
                });
            }
        }
        Ok(ActChoice { goals })
    }
}
