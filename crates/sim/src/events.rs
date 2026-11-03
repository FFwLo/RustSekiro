//! Map event scripts (EMEVD) at run time: a small interpreter for the instructions boss fights
//! use, so a fight's phase changes, boss bar and defeat come from the game's own event script.
//!
//! The game runs every event of `event/<map>.emevd` once per frame. Each event is a list of
//! instructions (decoded by [`sekiro_formats::emevd`], named by [`sekiro_formats::emedf`]):
//!
//! - Conditions (`If...`, banks 0 to 4) take a condition group as their first argument. Group 0
//!   ("MAIN") blocks the event until the condition holds; a positive group `n` collects
//!   conditions that must all hold (AND), a negative one conditions of which one must hold (OR).
//!   `IfConditionGroup` tests a group, either blocking (MAIN) or as a condition of another
//!   group. Once a MAIN condition passes, the collected groups are cleared.
//! - Control flow: skips, gotos to `Label<n>` instructions, ends (end type 0 ends the event,
//!   1 restarts it), and waits in seconds or frames.
//! - Commands on characters, flags, the boss health bar and the like, which the interpreter
//!   hands to its caller as [`EventCommand`]s.
//!
//! That semantics follows the community documentation (DarkScript3, SoulsFormats); the exact
//! per-frame order of the engine's event loop is not traced. We run only the events a fight
//! names (see [`crate::arena::BossEvents`]), and characters other than the ones the caller maps
//! are unknown: their conditions are false and commands on them are dropped.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use sekiro_formats::emedf::{self, Arg};
use sekiro_formats::emevd::{Emevd, Instruction};

/// The entity id the event scripts use for the player.
pub const PLAYER_ENTITY: i32 = 10000;

/// EMEVD 3[3]: group/state at bytes 0/1, entities at 4/8, radius at 12,
/// and entity count at 16. State 1 waits inside the radius; state 0 waits outside.
pub(crate) fn radius_condition(ins: &Instruction, a: [f32; 3], b: [f32; 3]) -> Option<bool> {
    let distance = glam::Vec3::from_array(a).distance(glam::Vec3::from_array(b));
    Some((distance <= ins.f32_at(12)?) == (ins.u8_at(1)? != 0))
}

/// What an event asks of the world.
#[derive(Debug, Clone, PartialEq)]
pub enum EventCommand {
    /// EMEVD 2003[5]: an event-authored shot from a character or region source.
    ShootBullet {
        owner: i32,
        source: i32,
        dummy: i32,
        behavior: i32,
        angles: [i32; 3],
    },
    /// Event 2004[19] changes the fight's NpcThinkParam row.
    AiParam {
        entity: i32,
        row: i32,
    },
    /// Event 2004[17] supplies GetEventRequest(slot) to Lua.
    AiCommand {
        entity: i32,
        command: i32,
        slot: u8,
    },
    AiReplan {
        entity: i32,
    },
    /// The arena placeholder replaces this cinematic with a timed black-screen boundary.
    Cutscene {
        id: i32,
        point: i32,
        area: u8,
        block: u8,
        player: i32,
    },
    /// `SetSpEffect` (2004[8]).
    SetSpEffect {
        entity: i32,
        id: i32,
    },
    /// `ClearSpEffect` (2004[21]).
    ClearSpEffect {
        entity: i32,
        id: i32,
    },
    /// `EzstateInstructionRequest` (2004[6]): a command the character's HKS script reads with
    /// `env(105, slot)` (the NPC script's `ExecEventTrandition` plays the matching event
    /// animation: 506000 is the Owl's heat-up `W_Event20005`).
    EzStateRequest {
        entity: i32,
        command: i32,
        slot: u8,
    },
    /// `DisplayBossHealthBar` (2003[11]): shows or hides the boss bar with an NpcName text id.
    BossBar {
        shown: bool,
        entity: i32,
        slot: i16,
        name: i32,
    },
    /// `SetCharacterImmortality` (2004[12]): HP stops at 1.
    Immortality {
        entity: i32,
        on: bool,
    },
    /// `SetCharacterInvincibility` (2004[15]): no damage at all.
    Invincibility {
        entity: i32,
        on: bool,
    },
    /// `SetCharacterHPBarDisplay` (2004[30]): the small bar over the character's head.
    HpBarDisplay {
        entity: i32,
        on: bool,
    },
    /// `HandleBossDefeat` (2003[12]): the boss is beaten (banner, music end, rewards).
    BossDefeat {
        entity: i32,
    },
    CharacterEnabled {
        entity: i32,
        enabled: bool,
    },
    AiEnabled {
        entity: i32,
        enabled: bool,
    },
    /// SetCharacterGravity (2004[10]), used by the hidden monkeys health controller.
    Gravity {
        entity: i32,
        enabled: bool,
    },
    CharacterKilled {
        entity: i32,
    },
    WarpToDummy {
        entity: i32,
        target: i32,
        dummy: i32,
        copy_floor: i32,
    },
}

/// What the events may ask about the world. Unknown entities answer `None`, which makes the
/// condition false.
pub trait EventHost {
    /// Optional world conditions for encounters with independently simulated actors.
    fn encounter_condition(&self, _instruction: &Instruction) -> Option<bool> {
        None
    }
    fn in_state(&self, _entity: i32, _state: i32) -> Option<bool> {
        None
    }
    fn cutscene_finished(&self, _id: i32) -> bool {
        true
    }
    fn has_sp_effect(&self, entity: i32, id: i32) -> Option<bool>;
    /// Whether the character's animation raises event message `id` (TAE 231
    /// `SetEzStateRequestID`), the condition `IfCharacterHasEventMessage` (4[8]) waits for.
    fn has_event_message(&self, entity: i32, id: i32) -> Option<bool>;
    fn is_dead(&self, entity: i32) -> Option<bool>;
    fn hp(&self, entity: i32) -> Option<i32>;
    fn max_hp(&self, _entity: i32) -> Option<i32> {
        None
    }
    fn inside_area(&self, _entity: i32, _area: i32) -> Option<bool> {
        None
    }
    fn health_bars(&self, _entity: i32) -> Option<i32> {
        None
    }
}

/// What happened, for logs and tests.
#[derive(Debug, Clone, PartialEq)]
pub enum EventLog {
    Ended(i64),
    Restarted(i64),
    Flag {
        flag: i32,
        on: bool,
    },
    Command {
        event: i64,
        command: EventCommand,
    },
    /// An instruction the interpreter does not run (listed once per instruction).
    Skipped {
        event: i64,
        bank: i32,
        id: i32,
    },
}

/// One event to run and where it starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EventEntry {
    pub id: i64,
    /// Start right after `Label<n>` instead of at the top (to join a fight that is already
    /// under way, after the parts of its event that play the intro).
    pub from_label: Option<u8>,
}

#[derive(Debug, Clone)]
struct Running {
    finished_groups: HashMap<i8, bool>,
    staged: bool,
    id: i64,
    code: Vec<Instruction>,
    entry: usize,
    pc: usize,
    /// Seconds or frames left of the current wait.
    wait: Option<f32>,
    groups: HashMap<i8, Vec<usize>>,
    ended: bool,
}

impl Running {
    fn reset(&mut self) {
        self.finished_groups.clear();
        self.staged = false;
        self.pc = self.entry;
        self.wait = None;
        self.groups.clear();
        self.ended = false;
    }
}

/// The events of one fight, their flags and log.
#[derive(Debug, Clone)]
pub struct EventScripts {
    /// Supporting actors retained separately from the arena's primary combatant.
    pub cast_scene: crate::arena::scene::SceneState,
    pub arena_c_state: crate::arena_c::EventState,
    running: Vec<Running>,
    /// Event flags that are on. They persist over a death and reload, as the game's do.
    pub flags: HashSet<i32>,
    pub log: Vec<EventLog>,
    skipped: HashSet<(i64, i32, i32)>,
    random_state: u64,
}

/// Instructions run per event and step before the event is made to wait (a loop guard).
const STEP_BUDGET: usize = 512;

fn label_code(ins: &Instruction) -> Option<i32> {
    (ins.bank == 1014).then_some(ins.id)
}

fn args(ins: &Instruction) -> Vec<Arg> {
    emedf::decode(ins)
        .map(|(_, a)| a.into_iter().map(|(_, v)| v).collect())
        .unwrap_or_default()
}

fn compare(cmp: i64, a: i64, b: i64) -> bool {
    match cmp {
        0 => a == b,
        1 => a != b,
        2 => a > b,
        3 => a < b,
        4 => a >= b,
        5 => a <= b,
        _ => false,
    }
}

impl EventScripts {
    /// Prepares `entries` from `file`, each with its parameters filled in from the first
    /// `InitializeEvent` that starts it.
    pub fn new(file: &Emevd, entries: &[EventEntry]) -> Result<Self, String> {
        let mut running = Vec::new();
        for e in entries {
            let ev = file
                .event(e.id)
                .ok_or_else(|| format!("event {} is not in the script", e.id))?;
            let start = file.starts_of(e.id).into_iter().next();
            let mut code = ev.resolve(start.as_ref().map_or(&[][..], |s| &s.args));
            crate::arena_c::join_intro(e.id, &mut code)?;
            let entry = match e.from_label {
                None => 0,
                Some(n) => {
                    code.iter()
                        .position(|i| label_code(i) == Some(n as i32))
                        .ok_or_else(|| format!("event {} has no Label{n}", e.id))?
                        + 1
                }
            };
            running.push(Running {
                finished_groups: HashMap::new(),
                staged: false,
                id: e.id,
                code,
                entry,
                pc: entry,
                wait: None,
                groups: HashMap::new(),
                ended: false,
            });
        }
        Ok(Self {
            cast_scene: Default::default(),
            arena_c_state: Default::default(),
            running,
            flags: HashSet::new(),
            log: Vec::new(),
            skipped: HashSet::new(),
            random_state: 1,
        })
    }

    /// [`EventScripts::new`] from `cache/raw/event/<map>.emevd`.
    pub fn load(cache: &Path, map: &str, entries: &[EventEntry]) -> Result<Self, String> {
        let path = cache.join(format!("raw/event/{map}.emevd"));
        let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let file = Emevd::parse(&bytes).map_err(|e| e.to_string())?;
        let mut scripts = Self::new(&file, entries)?;
        for running in &mut scripts.running {
            if let Some((at, code)) = crate::arenas_a::join(map, running.id, entries) {
                if running.code.get(at).map(Instruction::code) != Some(code) {
                    return Err(format!("event {}: staged entry {at} changed", running.id));
                }
                running.entry = at;
                running.pc = at;
            }
        }
        let initial = scripts.running.clone();
        for running in initial {
            if crate::arenas_a::all_slots(map, running.id)
                || crate::arena::depths_palace::all_slots(map, running.id)
            {
                for start in file.starts_of(running.id).iter().skip(1) {
                    let mut extra = running.clone();
                    extra.code = file.event(running.id).unwrap().resolve(&start.args);
                    scripts.running.push(extra);
                }
            }
        }
        Ok(scripts)
    }

    /// Starts every event over (after the fight reloads), keeping the flags.
    pub fn restart(&mut self) {
        self.cast_scene = Default::default();
        self.arena_c_state = Default::default();
        for r in &mut self.running {
            r.reset();
        }
    }

    pub fn flag(&self, id: i32) -> bool {
        self.flags.contains(&id)
    }
    /// Keep an authored event's program counter and wait across an entity hand-over.
    pub fn continue_event_from(&mut self, previous: &Self, id: i64) {
        for current in self.running.iter_mut().filter(|event| event.id == id) {
            if let Some(old) = previous
                .running
                .iter()
                .find(|event| event.id == id && event.code == current.code)
            {
                *current = old.clone();
            }
        }
    }

    pub fn set_flag(&mut self, id: i32, on: bool) {
        let changed = if on {
            self.flags.insert(id)
        } else {
            self.flags.remove(&id)
        };
        if changed {
            self.log.push(EventLog::Flag { flag: id, on });
        }
    }

    /// Whether event `id` has ended.
    pub fn ended(&self, id: i64) -> bool {
        self.running.iter().any(|r| r.id == id && r.ended)
    }

    /// Runs every event for one step of `dt` seconds; returns the commands they issued.
    pub fn step(&mut self, dt: f32, host: &dyn EventHost) -> Vec<EventCommand> {
        let mut out = Vec::new();
        for k in 0..self.running.len() {
            let mut r = std::mem::replace(
                &mut self.running[k],
                Running {
                    finished_groups: HashMap::new(),
                    staged: false,
                    id: 0,
                    code: Vec::new(),
                    entry: 0,
                    pc: 0,
                    wait: None,
                    groups: HashMap::new(),
                    ended: true,
                },
            );
            self.run(&mut r, dt, host, &mut out);
            self.running[k] = r;
        }
        out
    }

    fn eval_group(&self, r: &Running, group: i8, host: &dyn EventHost) -> bool {
        let Some(list) = r.groups.get(&group) else {
            return false;
        };
        if list.is_empty() {
            return false;
        }
        let mut each = list.iter().map(|&i| self.eval(r, &r.code[i], host));
        if group > 0 {
            each.all(|v| v)
        } else {
            each.any(|v| v)
        }
    }

    /// Evaluates condition instruction `ins` (its group argument aside).
    fn eval(&self, r: &Running, ins: &Instruction, host: &dyn EventHost) -> bool {
        let a = args(ins);
        let i = |k: usize| a.get(k).map_or(0, |v| v.as_i64());
        match ins.code() {
            (3, 12) => crate::event_values::comparison(ins, &self.flags),
            (3, 1) => {
                let mut flags = (ins.i32_at(4).unwrap_or(0)..=ins.i32_at(8).unwrap_or(0))
                    .map(|flag| self.flag(flag));
                match ins.u8_at(1) {
                    Some(0) => flags.all(|on| on),
                    Some(1) => flags.all(|on| !on),
                    Some(2) => flags.any(|on| on),
                    Some(3) => flags.any(|on| !on),
                    _ => false,
                }
            }
            (4, 2) => crate::arenas_a::hp_ratio_condition(ins, host),
            (4, 21) => {
                host.in_state(ins.i32_at(4).unwrap_or(0), ins.i32_at(8).unwrap_or(0))
                    == Some(ins.args.get(12).copied().unwrap_or(0) != 0)
            }
            (0, 0) => self.eval_group(r, i(2) as i8, host) == (i(1) != 0),
            (3, 0) => self.flag(i(3) as i32) == (i(1) != 0),
            (3, 2) => host.inside_area(i(2) as i32, i(3) as i32) == Some(i(1) != 0),
            // DarkScript3's death state: 0 alive, 1 dead.
            (4, 0) => host.is_dead(i(1) as i32) == Some(i(2) == 1),
            (4, 5) => host.has_sp_effect(i(1) as i32, i(2) as i32) == Some(i(3) != 0),
            (4, 8) => host.has_event_message(i(1) as i32, i(2) as i32) == Some(i(3) != 0),
            (4, 14) => host
                .hp(i(1) as i32)
                .is_some_and(|hp| compare(i(2), hp as i64, i(3))),
            (4, 37) => host
                .health_bars(i(1) as i32)
                .is_some_and(|bars| compare(i(2), bars as i64, i(3))),
            (3, 31) => host.cutscene_finished(i(1) as i32),
            _ => host.encounter_condition(ins).unwrap_or(false),
        }
    }

    fn is_condition(ins: &Instruction) -> bool {
        if ins.code() == (4, 2) {
            return true;
        }
        matches!(
            ins.code(),
            (0, 0)
                | (3, 12)
                | (3, 1)
                | (3, 0)
                | (3, 2)
                | (3, 8)
                | (3, 31)
                | (4, 0)
                | (4, 5)
                | (4, 8)
                | (4, 14)
                | (4, 21)
                | (4, 37)
                | (4, 9)
                | (3, 3)
                | (5, 2)
        )
    }

    fn goto(r: &mut Running, label: i64) {
        let n = label as i32;
        let after = r.code[r.pc + 1..]
            .iter()
            .position(|i| label_code(i) == Some(n))
            .map(|p| p + r.pc + 1);
        let at = after.or_else(|| r.code.iter().position(|i| label_code(i) == Some(n)));
        match at {
            Some(at) => r.pc = at + 1,
            None => r.pc = r.code.len(),
        }
    }

    fn end(&mut self, r: &mut Running, end_type: i64) {
        if end_type == 1 {
            r.reset();
            self.log.push(EventLog::Restarted(r.id));
        } else {
            r.ended = true;
            if crate::arena::depths_palace::completion_flag(r.id) {
                self.set_flag(r.id as i32, true);
            }
            self.log.push(EventLog::Ended(r.id));
        }
    }

    fn run(&mut self, r: &mut Running, dt: f32, host: &dyn EventHost, out: &mut Vec<EventCommand>) {
        if !r.staged {
            if let Some(entry) = crate::arena_b::entry(r.id, &self.flags) {
                r.pc = entry;
                r.entry = entry;
            }
            r.staged = true;
        }
        let mut dt_left = dt;
        for _ in 0..STEP_BUDGET {
            if let Some(pc) = crate::arena_b::after_intro(r.id, r.pc, &self.flags) {
                r.pc = pc;
            }
            if r.ended {
                return;
            }
            let Some(ins) = r.code.get(r.pc).cloned() else {
                r.ended = true;
                self.log.push(EventLog::Ended(r.id));
                return;
            };
            let a = args(&ins);
            let i = |k: usize| a.get(k).map_or(0, |v| v.as_i64());
            let f = |k: usize| a.get(k).map_or(0.0, |v| v.as_f32());
            if crate::arenas_a::unavailable_condition(r.id, ins.code())
                && host.encounter_condition(&ins).is_none()
            {
                if self.skipped.insert((r.id, ins.bank, ins.id)) {
                    self.log.push(EventLog::Skipped {
                        event: r.id,
                        bank: ins.bank,
                        id: ins.id,
                    });
                }
                return;
            }
            if Self::is_condition(&ins) {
                let group = ins.args.first().copied().unwrap_or(0) as i8;
                if group == 0 {
                    if !self.eval(r, &ins, host) {
                        return;
                    }
                    r.finished_groups = r
                        .groups
                        .keys()
                        .map(|&g| (g, self.eval_group(r, g, host)))
                        .collect();
                    r.groups.clear();
                } else {
                    r.groups.entry(group).or_default().push(r.pc);
                }
                r.pc += 1;
                continue;
            }
            let mut next = r.pc + 1;
            if ins.code() == (2003, 88) {
                self.cast_scene.instruction(&ins, &a);
            }
            if self.arena_c_state.instruction(&ins) {
                r.pc = next;
                continue;
            }
            if ins.code() == (2011, 1) {
                self.cast_scene.enabled.insert(
                    ins.i32_at(0).unwrap_or(0),
                    ins.args.get(4).copied().unwrap_or(0) != 0,
                );
                r.pc = next;
                continue;
            }
            if self.cast_scene.instruction(&ins, &a) {
                r.pc = next;
                continue;
            }
            let mut command = None;
            match ins.code() {
                (1000, 0..=3) => {
                    let state = ins.args.get(1).copied().unwrap_or(0) != 0;
                    let group = ins.args.get(2).copied().unwrap_or(0) as i8;
                    // 1/3 test the groups just constructed; 0/2 use the completed MAIN.
                    // Monk 12505970 builds group 2 immediately before opcode 1.
                    let compiled = ins.id == 0 || ins.id == 2;
                    let passed = if compiled {
                        r.finished_groups.get(&group).copied().unwrap_or(false)
                    } else {
                        self.eval_group(r, group, host)
                    };
                    if passed == state {
                        let value = ins.args.first().copied().unwrap_or(0);
                        if ins.id <= 1 {
                            next += usize::from(value);
                        } else {
                            self.end(r, i64::from(value));
                            if !r.ended {
                                return;
                            }
                            continue;
                        }
                    }
                }
                // SetNetworkUpdateRate only controls replication to remote players.
                // The offline simulation has no network character updates to schedule.
                (2004, 34) => {}
                (2004, 17) => {
                    command = Some(EventCommand::AiCommand {
                        entity: ins.i32_at(0).unwrap_or(0),
                        command: ins.i32_at(4).unwrap_or(-1),
                        slot: ins.args.get(8).copied().unwrap_or(0),
                    })
                }
                (2004, 20) => {
                    command = Some(EventCommand::AiReplan {
                        entity: ins.i32_at(0).unwrap_or(0),
                    })
                }
                (2004, 19) => {
                    command = Some(EventCommand::AiParam {
                        entity: ins.i32_at(0).unwrap_or(0),
                        row: ins.i32_at(4).unwrap_or(0),
                    })
                }
                // GotoIfConditionGroupStateCompiled uses the last completed MAIN's results.
                (1000, 107) => {
                    let label = ins.args.first().copied().unwrap_or(0);
                    let state = ins.args.get(1).copied().unwrap_or(0) != 0;
                    let group = ins.args.get(2).copied().unwrap_or(0) as i8;
                    if r.finished_groups.get(&group).copied().unwrap_or(false) == state {
                        Self::goto(r, i64::from(label));
                        continue;
                    }
                }
                (2002, 4) | (2002, 13) => {
                    let command = EventCommand::Cutscene {
                        id: i(0) as i32,
                        point: i(2) as i32,
                        area: i(3) as u8,
                        block: i(4) as u8,
                        player: i(5) as i32,
                    };
                    self.log.push(EventLog::Command {
                        event: r.id,
                        command: command.clone(),
                    });
                    out.push(command);
                    r.pc = next;
                    // The host must see the request before IfOngoingCutsceneFinished is tested.
                    return;
                }
                // EndUnconditionally(end type).
                (1000, 4) => {
                    self.end(r, i(0));
                    if !r.ended {
                        return; // a restart waits for the next step
                    }
                    continue;
                }
                // GotoIfConditionGroupStateUncompiled(label, state, group).
                (1000, 101) => {
                    if self.eval_group(r, i(2) as i8, host) == (i(1) != 0) {
                        Self::goto(r, i(0));
                        continue;
                    }
                }
                // GotoUnconditionally(label).
                (1000, 103) => {
                    Self::goto(r, i(0));
                    continue;
                }
                // WaitFixedTimeSeconds / WaitFixedTimeFrames (one frame per step).
                (1001, 0) | (1001, 1) | (1, 0) if ins.bank != 1 || ins.args.first() == Some(&0) => {
                    let left = r.wait.get_or_insert(if ins.bank == 1 {
                        ins.f32_at(4).unwrap_or(0.0)
                    } else if ins.id == 0 {
                        f(0)
                    } else {
                        i(0) as f32
                    });
                    if *left > 0.0 {
                        *left -= if ins.id == 0 { dt_left } else { 1.0 };
                        dt_left = 0.0;
                        if *left > 0.0 {
                            return;
                        }
                    }
                    r.wait = None;
                }
                // WaitForEventFlag(state, type, flag).
                (1003, 0) => {
                    if self.flag(i(2) as i32) != (i(0) != 0) {
                        return;
                    }
                }
                // SkipIfEventFlag(lines, state, type, flag).
                (1003, 1) => {
                    if self.flag(i(3) as i32) == (i(1) != 0) {
                        next += i(0) as usize;
                    }
                }
                // EndIfEventFlag(end type, state, type, flag).
                (1003, 2) => {
                    if self.flag(i(3) as i32) == (i(1) != 0) {
                        self.end(r, i(0));
                        if !r.ended {
                            return;
                        }
                        continue;
                    }
                }
                // GotoIfEventFlag(label, state, type, flag).
                (1003, 101) => {
                    if self.flag(i(3) as i32) == (i(1) != 0) {
                        Self::goto(r, i(0));
                        continue;
                    }
                }
                (1014, _) => {}
                (2003, 17) => {
                    // A reproducible uniform choice among the authored flags.
                    let first = ins.i32_at(0).unwrap_or(0);
                    let last = ins.i32_at(4).unwrap_or(first);
                    self.random_state = self
                        .random_state
                        .wrapping_mul(6364136223846793005)
                        .wrapping_add(1);
                    let count = i64::from(last) - i64::from(first) + 1;
                    let chosen = first + ((self.random_state >> 32) % count.max(1) as u64) as i32;
                    for flag in first..=last {
                        self.set_flag(flag, flag == chosen && ins.u8_at(8).unwrap_or(0) != 0);
                    }
                }
                (2003, 31) => {
                    let start = ins.i32_at(0).unwrap_or(0);
                    let bits = ins.u8_at(4).unwrap_or(0).min(32);
                    let limit = ins.i32_at(8).unwrap_or(0).max(0) as u32;
                    let value = crate::event_values::read(&self.flags, start, bits)
                        .saturating_add(1)
                        .min(limit);
                    for bit in 0..bits {
                        self.set_flag(start + i32::from(bit), value & (1 << bit) != 0);
                    }
                }
                (2003, 22) => {
                    for flag in ins.i32_at(0).unwrap_or(0)..=ins.i32_at(4).unwrap_or(0) {
                        self.set_flag(flag, ins.u8_at(8).unwrap_or(0) != 0);
                    }
                }
                // SetEventFlag / SetNetworkconnectedEventFlag(flag, state: 0 off, 1 on, 2 toggle).
                (2003, 2) | (2003, 62) => {
                    let flag = i(0) as i32;
                    let on = match i(1) {
                        2 => !self.flag(flag),
                        s => s != 0,
                    };
                    self.set_flag(flag, on);
                }
                // BatchSetNetworkconnectedEventFlags(first, last, state).
                (2003, 63) => {
                    let (a0, b0) = (i(0).min(i(1)) as i32, i(0).max(i(1)) as i32);
                    for flag in a0..=b0 {
                        self.set_flag(flag, i(2) != 0);
                    }
                }
                (2003, 11) => {
                    command = Some(EventCommand::BossBar {
                        shown: i(0) != 0,
                        entity: i(1) as i32,
                        slot: i(2) as i16,
                        name: i(3) as i32,
                    })
                }
                (2003, 5) => {
                    command = Some(EventCommand::ShootBullet {
                        owner: ins.i32_at(0).unwrap_or(0),
                        source: ins.i32_at(4).unwrap_or(0),
                        dummy: ins.i32_at(8).unwrap_or(-1),
                        behavior: ins.i32_at(12).unwrap_or(0),
                        angles: std::array::from_fn(|k| ins.i32_at(16 + k * 4).unwrap_or(0)),
                    })
                }
                (2003, 12) => {
                    command = Some(EventCommand::BossDefeat {
                        entity: i(0) as i32,
                    })
                }
                (2003, 74) => {
                    command = Some(EventCommand::BossDefeat {
                        entity: ins.i32_at(0).unwrap_or(0),
                    })
                }
                (2004, 5) => {
                    command = Some(EventCommand::CharacterEnabled {
                        entity: i(0) as i32,
                        enabled: i(1) != 0,
                    })
                }
                (2004, 1) => {
                    command = Some(EventCommand::AiEnabled {
                        entity: ins.i32_at(0).unwrap_or(0),
                        enabled: ins.u8_at(4).unwrap_or(1) != 0,
                    })
                }
                (2004, 4) => {
                    command = Some(EventCommand::CharacterKilled {
                        entity: ins.i32_at(0).unwrap_or(0),
                    })
                }
                (2004, 10) => {
                    command = Some(EventCommand::Gravity {
                        entity: ins.i32_at(0).unwrap_or(0),
                        enabled: ins.u8_at(4).unwrap_or(1) != 0,
                    })
                }
                (2004, 42) if ins.i32_at(4) == Some(2) => {
                    command = Some(EventCommand::WarpToDummy {
                        entity: ins.i32_at(0).unwrap_or(0),
                        target: ins.i32_at(8).unwrap_or(0),
                        dummy: ins.i32_at(12).unwrap_or(-1),
                        copy_floor: ins.i32_at(16).unwrap_or(0),
                    })
                }
                (2004, 6) => {
                    command = Some(EventCommand::EzStateRequest {
                        entity: i(0) as i32,
                        command: i(1) as i32,
                        slot: i(2) as u8,
                    })
                }
                (2004, 8) => {
                    command = Some(EventCommand::SetSpEffect {
                        entity: i(0) as i32,
                        id: i(1) as i32,
                    })
                }
                (2004, 21) => {
                    command = Some(EventCommand::ClearSpEffect {
                        entity: i(0) as i32,
                        id: i(1) as i32,
                    })
                }
                (2004, 12) => {
                    command = Some(EventCommand::Immortality {
                        entity: i(0) as i32,
                        on: i(1) != 0,
                    })
                }
                (2004, 15) => {
                    command = Some(EventCommand::Invincibility {
                        entity: i(0) as i32,
                        on: i(1) != 0,
                    })
                }
                (2004, 30) => {
                    command = Some(EventCommand::HpBarDisplay {
                        entity: i(0) as i32,
                        on: i(1) != 0,
                    })
                }
                (bank, id) => {
                    if self.skipped.insert((r.id, bank, id)) {
                        self.log.push(EventLog::Skipped {
                            event: r.id,
                            bank,
                            id,
                        });
                    }
                }
            }
            if let Some(c) = command {
                self.log.push(EventLog::Command {
                    event: r.id,
                    command: c.clone(),
                });
                out.push(c);
            }
            r.pc = next;
        }
    }
}

/// Reads an event warp point directly from the local MSB, whose exported layout currently
/// contains parts rather than gameplay regions. Region base data stores entity id at +4.
/// Positions/yaw use the same mirrored-X convention as the map's exported parts.
pub fn warp_point(
    cache: &Path,
    area: u8,
    block: u8,
    entity: i32,
) -> Option<crate::deathblow::Placement> {
    use sekiro_formats::reader::{Reader, utf16z};
    let bytes =
        std::fs::read(cache.join(format!("raw/map/MapStudio/m{area:02}_{block:02}_00_00.msb")))
            .ok()?;
    if !bytes.starts_with(b"MSB ") {
        return None;
    }
    let mut table = 0x10;
    for _ in 0..32 {
        if table == 0 {
            return None;
        }
        let mut r = Reader::at(&bytes, table);
        r.i32().ok()?;
        let count = usize::try_from(r.i32().ok()?).ok()?;
        if !(1..=100_000).contains(&count) {
            return None;
        }
        let name = r.offset64().ok()?;
        let mut entries = Vec::with_capacity(count - 1);
        for _ in 1..count {
            entries.push(r.offset64().ok()?);
        }
        table = r.offset64().ok()?;
        if utf16z(&bytes, name).ok()? != "POINT_PARAM_ST" {
            continue;
        }
        for entry in entries {
            let base = Reader::at(&bytes, entry.checked_add(0x50)?)
                .offset64()
                .ok()?;
            if base == 0
                || Reader::at(&bytes, entry.checked_add(base)?.checked_add(4)?)
                    .i32()
                    .ok()?
                    != entity
            {
                continue;
            }
            let mut r = Reader::at(&bytes, entry.checked_add(0x14)?);
            let [x, y, z] = r.vec3().ok()?;
            let rotation = r.vec3().ok()?;
            return Some(crate::deathblow::Placement {
                position: [-x, y, z],
                yaw: -rotation[1].to_radians(),
            });
        }
        return None;
    }
    None
}

/// Fixed-tick cinematic placeholder. Combat and input remain paused for its full lifetime.
#[derive(Debug, Clone)]
pub struct Cinematic {
    pub id: i32,
    pub elapsed: f32,
    pub warp: Option<crate::deathblow::Placement>,
    pub applied: bool,
}

impl Cinematic {
    pub const FADE: f32 = 0.5;
    pub const HOLD: f32 = 0.75;
    pub const DURATION: f32 = 2.0 * Self::FADE + Self::HOLD;

    pub fn alpha(&self) -> f32 {
        if self.elapsed < Self::FADE {
            self.elapsed / Self::FADE
        } else if self.elapsed < Self::FADE + Self::HOLD {
            1.0
        } else {
            ((Self::DURATION - self.elapsed) / Self::FADE).clamp(0.0, 1.0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sekiro_formats::emevd::{Event, RestBehavior};

    #[test]
    fn flag_range_distinguishes_all_off_from_all_on_and_any() {
        for (state, enabled, expected) in [
            (0, vec![], false),
            (0, vec![9800, 9801], true),
            (1, vec![], true),
            (1, vec![9800], false),
            (2, vec![9800], true),
            (2, vec![], false),
            (3, vec![9800], true),
            (3, vec![9800, 9801], false),
        ] {
            let mut gate = ins(3, 1, &[0, 9800, 9801]);
            gate.args[1] = state;
            let mut scripts = EventScripts::new(
                &file(vec![gate, ins(2003, 2, &[123, 1])]),
                &[EventEntry {
                    id: 1,
                    from_label: None,
                }],
            )
            .unwrap();
            scripts.flags.extend(enabled);
            scripts.step(0., &World::default());
            assert_eq!(scripts.flag(123), expected, "range state {state}");
        }
    }

    fn ins(bank: i32, id: i32, words: &[i32]) -> Instruction {
        Instruction {
            bank,
            id,
            args: words.iter().flat_map(|w| w.to_le_bytes()).collect(),
            layer: None,
        }
    }

    /// A condition with a group byte, then padding, then words (the 4[x] layout).
    fn cond(bank: i32, id: i32, group: i8, rest: &[u8]) -> Instruction {
        let mut args = vec![group as u8, 0, 0, 0];
        args.extend_from_slice(rest);
        Instruction {
            bank,
            id,
            args,
            layer: None,
        }
    }

    fn bytes(words: &[i32]) -> Vec<u8> {
        words.iter().flat_map(|w| w.to_le_bytes()).collect()
    }

    /// IfCharacterHasEventMessage(group, entity, message, has=1, cmp=0, count=1.0).
    fn has_message(group: i8, entity: i32, message: i32) -> Instruction {
        let mut rest = bytes(&[entity, message]);
        rest.extend_from_slice(&[1, 0, 0, 0]);
        rest.extend_from_slice(&1.0f32.to_le_bytes());
        cond(4, 8, group, &rest)
    }

    /// IfCharacterHasSpeffect(group, entity, id, has, cmp=0, count=1.0).
    fn has_effect(group: i8, entity: i32, id: i32, has: bool) -> Instruction {
        let mut rest = bytes(&[entity, id]);
        rest.extend_from_slice(&[has as u8, 0, 0, 0]);
        rest.extend_from_slice(&1.0f32.to_le_bytes());
        cond(4, 5, group, &rest)
    }

    #[derive(Default)]
    struct World {
        messages: HashSet<i32>,
        effects: HashSet<i32>,
        distance: f32,
    }

    impl EventHost for World {
        fn encounter_condition(&self, instruction: &Instruction) -> Option<bool> {
            (instruction.code() == (3, 3)).then(|| {
                radius_condition(instruction, [0.0; 3], [0.0, self.distance, 0.0]).unwrap()
            })
        }
        fn has_sp_effect(&self, entity: i32, id: i32) -> Option<bool> {
            (entity == 7).then(|| self.effects.contains(&id))
        }
        fn has_event_message(&self, entity: i32, id: i32) -> Option<bool> {
            (entity == 7).then(|| self.messages.contains(&id))
        }
        fn is_dead(&self, _: i32) -> Option<bool> {
            Some(false)
        }
        fn hp(&self, _: i32) -> Option<i32> {
            Some(1)
        }
    }

    fn file(code: Vec<Instruction>) -> Emevd {
        Emevd {
            events: vec![Event {
                id: 1,
                rest: RestBehavior::Default,
                instructions: code,
                parameters: Vec::new(),
            }],
            ..Emevd::default()
        }
    }

    #[test]
    fn radius_gate_waits_for_authored_state_after_network_noop() {
        for inside in [false, true] {
            // Real 3[3] layout, including the entity count (not the state) at byte 16.
            let mut gate = ins(3, 3, &[0, PLAYER_ENTITY, 7, 20.0f32.to_bits() as i32, 1]);
            gate.args[1] = u8::from(inside);
            let code = vec![
                ins(2004, 34, &[7, 1, 0]),
                gate,
                ins(2003, 2, &[11505801, 1]),
            ];
            let mut scripts = EventScripts::new(
                &file(code),
                &[EventEntry {
                    id: 1,
                    from_label: None,
                }],
            )
            .unwrap();
            let mut world = World {
                distance: if inside { 21.0 } else { 19.0 },
                ..World::default()
            };
            scripts.step(0.1, &world);
            assert!(!scripts.flag(11505801));
            assert!(
                scripts.log.is_empty(),
                "network update is an explicit no-op"
            );
            world.distance = if inside { 19.0 } else { 21.0 };
            scripts.step(0.1, &world);
            assert!(scripts.flag(11505801));
            assert!(scripts.ended(1));
        }
    }

    #[test]
    fn waits_on_main_conditions_and_or_groups() {
        let code = vec![
            has_message(0, 7, 20),
            ins(2004, 8, &[7, 200051]),
            // WaitFixedTimeSeconds(1.0)
            ins(1001, 0, &[f32::to_bits(1.0) as i32]),
            has_message(-1, 7, 60),
            has_effect(-1, 7, 3506080, false),
            // IfConditionGroup(MAIN, on, OR group 1)
            Instruction {
                bank: 0,
                id: 0,
                args: vec![0, 1, 0xFF, 0],
                layer: None,
            },
            ins(2003, 2, &[11110860, 1]),
        ];
        let mut ev = EventScripts::new(
            &file(code),
            &[EventEntry {
                id: 1,
                from_label: None,
            }],
        )
        .unwrap();
        let mut w = World::default();
        assert!(ev.step(0.1, &w).is_empty());
        w.messages.insert(20);
        w.effects.insert(3506080);
        let c = ev.step(0.1, &w);
        assert_eq!(
            c,
            vec![EventCommand::SetSpEffect {
                entity: 7,
                id: 200051
            }]
        );
        w.messages.clear();
        for _ in 0..12 {
            ev.step(0.1, &w);
        }
        assert!(
            !ev.flag(11110860),
            "the OR group held while the effect was on"
        );
        w.effects.clear();
        ev.step(0.1, &w);
        assert!(ev.flag(11110860));
        assert!(ev.ended(1));
    }

    #[test]
    fn flags_labels_and_restarts() {
        let code = vec![
            // GotoIfEventFlag(label 0, off, 0, 5)
            ins(1003, 101, &[0x0000_0000, 5]),
            ins(2003, 2, &[6, 1]),
            ins(1014, 0, &[]),
            ins(2003, 2, &[5, 1]),
            // EndUnconditionally(restart)
            ins(1000, 4, &[1]),
        ];
        let mut ev = EventScripts::new(
            &file(code),
            &[EventEntry {
                id: 1,
                from_label: Some(0),
            }],
        )
        .unwrap();
        let w = World::default();
        ev.step(0.1, &w);
        assert!(ev.flag(5));
        ev.step(0.1, &w);
        assert!(!ev.flag(6), "started after the label, so 6 is never set");
        assert!(!ev.ended(1), "restarted, not ended");
    }
}
