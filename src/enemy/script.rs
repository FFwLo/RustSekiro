//! Boss map scripts: the boss's own EMEVD events, run the way the game runs them
//! (tools/boss_scripts.py -> extracted/enemies/boss_scripts.json).
//!
//! Each event instance runs from its first instruction every frame until it blocks: a MAIN
//! condition that does not hold yet, or a wait. Conditions go into condition groups (AND 1..15,
//! OR -1..-15); `IF Condition Group` folds one group into another; a MAIN condition blocks until
//! it holds, then every group is compiled (its result kept for the compiled SKIP / GOTO / END
//! checks) and cleared. END ends the event (its own flag goes on) or restarts it.
//! Instruction semantics: DarkScript3 sekiro-common.emedf.json names and argument layouts.
//!
//! The fight's characters are the boss part and the parts its events bring in (the exporter's
//! `chars`), placed from their MSB spots relative to the boss part (`events::place`). Wolf is
//! 10000. What the script does to them goes through the same paths as the AI: SpEffects
//! (residents), AI commands (GetEventRequest), forced anims and EzState requests, spawning,
//! warps, the boss's death.
//! The characters it ever enables are its cast (`Cast`): they are not the fight's main enemy
//! in the checks, and a boss's death hands the fight over to a living cast member that runs a
//! script of its own (Lady Butterfly 1000800 -> 1000810, Genichiro -> Tomoe). Shoot Bullet
//! (2003[5]) fires through enemy_bullet.rs `ScriptShots`; NPC team hostility is combat.rs
//! `teams_hostile`. On the boss's own arena (map.rs boss_arena) the heights count too.
//! gaps: map objects, SFX, sounds, cameras, cutscenes (taken as done), lock-on points, NPC
//! parts, gravity / hit masks, draw masks, the damage type of a hit, region heights.

use bevy::prelude::*;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, OnceLock};

use super::{Enemy, Mode};
use crate::actor::Actor;
use crate::config::GameConfig;
use crate::data::Combat;
use crate::player::Player;

/// Wolf's entity id in the map scripts.
pub const PLAYER: i64 = 10000;

#[derive(serde::Deserialize, Clone, Debug)]
pub struct CharDef {
    pub chr: String,
    pub npc: i64,
    pub at: [f32; 3],
    pub yaw: f32,
}

#[derive(serde::Deserialize, Clone, Debug)]
pub struct Place {
    pub at: [f32; 3],
    pub yaw: f32,
}

#[derive(serde::Deserialize, Clone, Debug)]
pub struct Shape {
    /// 1 circle, 2 sphere, 3 cylinder, 4 rectangle, 5 box, 6 composite (MSB Shape.cs).
    pub shape: i32,
    pub size: Vec<f32>,
    /// A composite's regions (inside any of them).
    #[serde(default)]
    pub parts: Vec<ShapePart>,
}

#[derive(serde::Deserialize, Clone, Debug)]
pub struct ShapePart {
    pub shape: i32,
    pub size: Vec<f32>,
    pub at: [f32; 3],
    pub yaw: f32,
}

/// Whether a boss-frame point (x right, z ahead) is in a shape at `at` / `yaw` (degrees), seen
/// from above (gap: heights are not checked; the arena drops them).
fn shape_contains(shape: i32, size: &[f32], at: [f32; 3], yaw: f32, x: f32, z: f32) -> bool {
    let (dx, dz) = (x - at[0], z - at[2]);
    let (sin, cos) = yaw.to_radians().sin_cos();
    // The map's yaw turns +Z toward +X: right = (cos, -sin), ahead = (sin, cos).
    let (lx, lz) = (dx * cos - dz * sin, dx * sin + dz * cos);
    let s = |i: usize| size.get(i).copied().unwrap_or(0.0);
    match shape {
        1..=3 => lx * lx + lz * lz <= s(0) * s(0),
        4 | 5 => lx.abs() <= s(0) * 0.5 && lz.abs() <= s(1) * 0.5,
        _ => false,
    }
}

#[derive(serde::Deserialize, Clone, Debug)]
pub struct EventDef {
    pub id: i64,
    pub slot: i64,
    /// 0 default, 1 restart, 2 end (what resting at an idol does to it; unused here).
    #[serde(default)]
    pub rest: i64,
    pub ins: Vec<(i32, i32, Vec<f64>)>,
    /// It brought a character of the fight in on a condition on one outside it (Tomoe's 0 bars
    /// bring Isshin in, m11_02 11125830): the fight starts after it (its flags as it left them).
    #[serde(default)]
    pub done: bool,
    /// The instruction it starts at: its last Change Character Enable State (boss, on), when
    /// it brings the boss itself in (the duel starts with the boss there; the cutscene, the talk
    /// before it and the world-state branches are behind it: m11_01 11115860, the Owl).
    #[serde(default)]
    pub start: usize,
}

#[derive(serde::Deserialize, Clone)]
pub struct ScriptDef {
    pub map: String,
    pub boss: i64,
    pub chars: HashMap<i64, CharDef>,
    #[serde(default)]
    pub places: HashMap<i64, Place>,
    #[serde(default)]
    pub regions: HashMap<i64, Shape>,
    pub events: Vec<EventDef>,
    #[serde(default)]
    pub flags_on: Vec<i64>,
    /// Shoot Bullet (2003[5]): behavior id (BehaviorParam 200000000 + variation * 1000 + judge,
    /// refType 1) -> its first Bullet row, and every row of those chains (HitBulletID /
    /// intervalCreateBulletId) with its AtkParam_Npc (tools/boss_scripts.py).
    #[serde(default)]
    pub bullets: HashMap<String, i64>,
    #[serde(default)]
    pub bullet_rows: HashMap<String, crate::data::BulletSpec>,
    /// The SpEffectParam rows those bullets put on (the lightning signs' 3531061-5).
    #[serde(default)]
    pub sp_effects: HashMap<String, crate::data::SpEffect>,
}

/// The boss scripts by NpcParam row.
pub fn scripts() -> &'static HashMap<i64, Arc<ScriptDef>> {
    static S: OnceLock<HashMap<i64, Arc<ScriptDef>>> = OnceLock::new();
    S.get_or_init(|| {
        let p = crate::paths::root().join("extracted/enemies/boss_scripts.json");
        let Ok(text) = std::fs::read_to_string(&p) else { return HashMap::new() };
        let raw: HashMap<String, ScriptDef> = match serde_json::from_str(&text) {
            Ok(r) => r,
            Err(e) => {
                warn!("boss_scripts.json: {e}");
                return HashMap::new();
            }
        };
        raw.into_iter().filter_map(|(k, v)| Some((k.parse().ok()?, Arc::new(v)))).collect()
    })
}

/// A character of a running script, by its map entity id.
#[derive(Component, Clone, Copy, Debug)]
pub struct Scripted(pub i64);

/// A character a boss script brought in besides the boss (its cast: tentacles, phantoms,
/// illusions, the next boss).
#[derive(Component, Clone, Copy, Debug)]
pub struct Cast;

/// A character the script brings in with a short warp (Issue Short Warp Request): out of the
/// fight (hidden, far away, no AI) until the script first warps it next to the fight (the Monk's
/// phantoms: enabled at 2 bars, warped in by 12505970), and again once an AI command puts it
/// back on what it had before the warp (their "stop", 0 in slot 1, after their turn) and its
/// attack is over. gap: how the game keeps them out of sight in between is not traced (their
/// Lua's command 0 is Wait).
#[derive(Component, Clone, Debug)]
pub struct Phantom {
    out: bool,
    /// The AI commands it had when warped in.
    before: Vec<(i64, i64)>,
    /// The slots that got another command since (its "go").
    went: Vec<i64>,
    /// And then one of those got its old command back.
    resting: bool,
}

impl Phantom {
    pub fn parked(&self) -> bool {
        self.out
    }
}

/// Where an offstage character waits.
const PARK: Vec3 = Vec3::new(0.0, 0.0, 5000.0);

#[derive(Clone, Debug)]
enum Cond {
    /// The condition instruction at this index, registered at this event time.
    Pred(usize, f32),
    /// IF Condition Group: the target group's state equals this.
    Group(i8, bool),
}

#[derive(Clone, Debug, Default)]
struct EvState {
    pc: usize,
    groups: HashMap<i8, Vec<Cond>>,
    compiled: HashMap<i8, bool>,
    /// The MAIN condition it is blocked on.
    main: Option<Cond>,
    wait: f32,
    /// Event time (runs while it is alive).
    t: f32,
    done: bool,
}

/// A running boss script.
pub struct Run {
    pub row: i64,
    def: Arc<ScriptDef>,
    /// The boss part's spot in the arena (the boss's start).
    start: Vec3,
    flags: HashMap<i64, bool>,
    events: Vec<EvState>,
    /// Map entity id -> bevy entity of the characters on the field.
    pub ents: HashMap<i64, Entity>,
    /// HP last frame, for "damaged" checks.
    prev_hp: HashMap<i64, f32>,
    rng: u32,
    /// Time since the script handled the boss's defeat.
    pub defeated: Option<f32>,
    /// Time the boss part has lain dead without one.
    dead_for: f32,
    /// Characters seen dead (still dead once gone).
    dead_ids: HashSet<i64>,
    /// Warps of characters not on the field yet (they come in there).
    pending_warp: HashMap<i64, (Vec3, f32)>,
    /// Requests for characters spawned this frame (applied next frame).
    pending: Vec<Act>,
    /// Disabled characters (Change Character Enable State off) and where they stood.
    disabled: HashMap<i64, (Vec3, f32)>,
    /// Frames run.
    frame: u32,
    /// The event message (`Enemy::msg_serial`) each character's last short warp came on.
    warp_serial: HashMap<i64, u32>,
}

#[derive(Resource, Default)]
pub struct BossScript {
    pub run: Option<Run>,
    /// Fights won (a defeat handled before the duel started over).
    pub defeats: u32,
}

/// One character as the conditions see it this frame.
#[derive(Clone, Debug, Default)]
struct Snap {
    pos: Vec3,
    hp: f32,
    hp_max: f32,
    posture_ratio: f32,
    bars: u32,
    dead: bool,
    sp: HashSet<i64>,
    /// Its event messages (an enemy's one, Wolf's several: his Todome sends 10-13 together).
    msg: Vec<i64>,
    ai_state: i64,
    damaged: bool,
}

/// What an event asked for this frame (applied after all events ran).
#[derive(Clone, Debug)]
enum Act {
    SetSp(i64, i64),
    ClearSp(i64, i64),
    AiCommand(i64, i64, i64),
    Replan(i64),
    AiId(i64, i64),
    AiState(i64, bool),
    ForceAnim(i64, i64, bool),
    EzState(i64, i64),
    Enable(i64, bool),
    /// Who, target type (Target Entity Type: 0 object, 1 area, 2 character), target, a short
    /// warp request (2004[41]).
    Warp(i64, i64, i64, bool),
    Immortal(i64, bool),
    Death(i64),
    BossDefeat,
    CutsceneWarp(i64),
    /// Shoot Bullet: owner, source character (whose dummy poly it leaves from), dummy poly,
    /// behavior id.
    Shoot(i64, i64, i64, i64),
}

impl Run {
    fn new(row: i64, def: Arc<ScriptDef>, start: Vec3, boss: Entity, rng: u32) -> Run {
        let mut flags = HashMap::new();
        for f in &def.flags_on {
            flags.insert(*f, true);
        }
        let mut events: Vec<EvState> = def.events.iter().map(|_| EvState::default()).collect();
        for (ev, st) in def.events.iter().zip(events.iter_mut()) {
            st.pc = ev.start;
            if !ev.done {
                continue;
            }
            st.done = true;
            flags.insert(ev.id, true);
            flags.insert(ev.id + ev.slot, true);
            for (bank, id, v) in &ev.ins {
                let n = |k: usize| v.get(k).copied().unwrap_or(0.0) as i64;
                match (bank, id) {
                    (2003, 2) => {
                        flags.insert(n(0), n(1) == 1);
                    }
                    (2003, 22) => {
                        for f in n(0)..=n(1) {
                            flags.insert(f, n(2) == 1);
                        }
                    }
                    _ => {}
                }
            }
        }
        let mut ents = HashMap::new();
        ents.insert(def.boss, boss);
        Run {
            row,
            def,
            start,
            flags,
            events,
            ents,
            prev_hp: HashMap::new(),
            rng,
            defeated: None,
            dead_for: 0.0,
            dead_ids: HashSet::new(),
            pending_warp: HashMap::new(),
            pending: Vec::new(),
            disabled: HashMap::new(),
            frame: 0,
            warp_serial: HashMap::new(),
        }
    }

    fn rand(&mut self) -> u32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        self.rng
    }

    /// Arena position and yaw of an MSB entity of this script.
    fn place_of(&self, id: i64) -> Option<(Vec3, f32)> {
        let (at, yaw) = if let Some(c) = self.def.chars.get(&id) {
            (c.at, c.yaw)
        } else {
            let p = self.def.places.get(&id)?;
            (p.at, p.yaw)
        };
        let at = serde_json::json!(at);
        Some(super::events::place(self.start, &at, &serde_json::json!(yaw)))
    }

    /// Is the world point inside the MSB region? (Seen from above: gap, heights are dropped.)
    fn inside(&self, region: i64, p: Vec3) -> bool {
        let (Some(pl), Some(sh)) = (self.def.places.get(&region), self.def.regions.get(&region)) else { return false };
        // Into the boss frame (see events::place).
        let (x, z) = (self.start.x - p.x, p.z - self.start.z);
        if sh.shape == 6 {
            return sh.parts.iter().any(|k| shape_contains(k.shape, &k.size, k.at, k.yaw, x, z));
        }
        shape_contains(sh.shape, &sh.size, pl.at, pl.yaw, x, z)
    }

    fn flag(&self, id: i64) -> bool {
        self.flags.get(&id).copied().unwrap_or(false)
    }

    /// The event flags that are on (for the AI's IsEventFlag).
    pub fn flags_on(&self) -> Vec<i64> {
        self.flags.iter().filter(|(_, on)| **on).map(|(id, _)| *id).collect()
    }

    /// An event value: `bits` flags from `base`, the first the highest bit.
    fn value(&self, base: i64, bits: i64) -> i64 {
        (0..bits).fold(0, |v, i| v << 1 | i64::from(self.flag(base + i)))
    }

    fn set_value(&mut self, base: i64, bits: i64, v: i64) {
        for i in 0..bits {
            self.flags.insert(base + i, v >> (bits - 1 - i) & 1 == 1);
        }
    }

    /// Target Event Flag Type: 0 event flag, 1 event id, 2 event id + slot (0 = this event).
    fn flag_of(&self, ty: i64, id: i64, ev: usize) -> bool {
        match ty {
            0 => self.flag(id),
            1 => self.flag(if id == 0 { self.def.events[ev].id } else { id }),
            _ => {
                let me = &self.def.events[ev];
                self.flag(if id == 0 { me.id + me.slot } else { id + me.slot })
            }
        }
    }
}

fn cmp(a: f64, op: i64, b: f64) -> bool {
    match op {
        0 => a == b,
        1 => a != b,
        2 => a > b,
        3 => a < b,
        4 => a >= b,
        _ => a <= b,
    }
}

/// Logical Operation Type over a flag range: 0 all on, 1 all off, 2 not all off, 3 not all on.
fn batch(run: &Run, op: i64, first: i64, last: i64) -> bool {
    let mut on = (first..=last).map(|f| run.flag(f));
    match op {
        0 => on.all(|x| x),
        1 => on.all(|x| !x),
        2 => on.any(|x| x),
        _ => on.any(|x| !x),
    }
}

struct World<'a> {
    chars: &'a HashMap<i64, Snap>,
}

impl World<'_> {
    fn get(&self, id: i64) -> Option<&Snap> {
        self.chars.get(&id)
    }
}

/// "Number of target characters" checks: (1 if it holds else 0) <cmp> count.
fn counted(holds: bool, op: i64, count: f64) -> bool {
    cmp(if holds { 1.0 } else { 0.0 }, op, count)
}

/// A condition instruction's value now.
fn pred(run: &Run, ev: usize, w: &World, ins: &(i32, i32, Vec<f64>), since: f32) -> bool {
    let (bank, id, v) = (ins.0, ins.1, &ins.2);
    let i = |k: usize| v.get(k).copied().unwrap_or(0.0) as i64;
    let f = |k: usize| v.get(k).copied().unwrap_or(0.0);
    let t = run.events[ev].t - since;
    let chr = |k: usize| w.get(i(k));
    match (bank, id) {
        (1, 0) => t >= f(1) as f32,
        (1, 1) => t >= i(1) as f32 / 60.0,
        (3, 0) => {
            let on = run.flag_of(i(2), i(3), ev);
            if i(1) == 2 { false } else { on == (i(1) == 1) }
        }
        (3, 1) => batch(run, i(1), i(3), i(4)),
        // Wolf is not in the arena yet on the script's first frame (the map loads before he walks
        // in: m25's 12505960 waits for him in 2502850 only after 12505990 posed the Monk).
        (3, 2) | (3, 3) if run.frame == 0 && (i(2) == PLAYER || i(3) == PLAYER) => i(1) == 0,
        (3, 2) => {
            let inside = chr(2).is_some_and(|c| run.inside(i(3), c.pos));
            inside == (i(1) == 1)
        }
        (3, 3) => {
            let (Some(a), Some(b)) = (chr(2), chr(3)) else { return false };
            let inside = a.pos.distance(b.pos) <= f(4) as f32;
            inside == (i(1) == 1)
        }
        (3, 8) => i(1) == 1,
        (3, 12) => cmp(run.value(i(1), i(2)) as f64, i(3), f(4)),
        (3, 10) => {
            let n = (i(2)..=i(3)).filter(|f| run.flag(*f)).count() as f64;
            cmp(n, i(4), f(5))
        }
        // IF Damage Type / IF Character Damaged By: it lost HP this frame (gap: by whom and
        // which damage type are not tracked).
        (3, 23) => chr(1).is_some_and(|c| c.damaged),
        (4, 1) => chr(1).is_some_and(|c| c.damaged),
        (3, 31) | (4, 15) | (4, 43) | (5, 10) => true,
        (4, 7) => i(2) == 1,
        (4, 0) => {
            // A character not on the field: dead if it died, else alive (disabled).
            let dead = chr(1).map_or(run.dead_ids.contains(&i(1)), |c| c.dead);
            counted(dead == (i(2) == 1), i(3), f(4))
        }
        (4, 2) => chr(1).is_some_and(|c| counted(cmp((c.hp / c.hp_max.max(1.0)) as f64, i(2), f(3)), i(4), f(5))),
        (4, 5) => chr(1).is_some_and(|c| counted(c.sp.contains(&i(2)) == (i(3) == 1), i(4), f(5))),
        (4, 8) | (4, 21) => chr(1).is_some_and(|c| counted(c.msg.contains(&i(2)) == (i(3) == 1), i(4), f(5))),
        (4, 9) => chr(1).is_some_and(|c| counted(c.ai_state == i(2), i(3), f(4))),
        (4, 14) => chr(1).is_some_and(|c| counted(cmp(c.hp as f64, i(2), f(3)), i(4), f(5))),
        (4, 37) => chr(1).is_some_and(|c| counted(cmp(c.bars as f64, i(2), f(3)), i(4), f(5))),
        (4, 39) => chr(1).is_some_and(|c| counted(cmp(c.posture_ratio as f64, i(2), f(3)), i(4), f(5))),
        // Unknown to this arena (action buttons, NPC parts, swimming, loading, event values,
        // the player standing on a collision): never.
        _ => false,
    }
}

fn eval(run: &Run, ev: usize, w: &World, c: &Cond) -> bool {
    match c {
        Cond::Pred(pc, since) => pred(run, ev, w, &run.def.events[ev].ins[*pc], *since),
        Cond::Group(g, want) => group(run, ev, w, *g) == *want,
    }
}

/// A condition group now: AND (positive) all hold, OR (negative) any holds.
fn group(run: &Run, ev: usize, w: &World, g: i8) -> bool {
    let Some(cs) = run.events[ev].groups.get(&g) else { return g > 0 };
    if g < 0 {
        cs.iter().any(|c| eval(run, ev, w, c))
    } else {
        cs.iter().all(|c| eval(run, ev, w, c))
    }
}

fn is_condition(bank: i32) -> bool {
    (0..1000).contains(&bank)
}

/// Runs one event until it blocks; pushes its actions.
fn step(run: &mut Run, ev: usize, w: &World, dt: f32, acts: &mut Vec<Act>) {
    if run.events[ev].done {
        return;
    }
    run.events[ev].t += dt;
    if run.events[ev].wait > 0.0 {
        run.events[ev].wait -= dt;
        if run.events[ev].wait > 0.0 {
            return;
        }
    }
    let def = run.def.clone();
    let ins = &def.events[ev].ins;
    let label = |l: i64, from: usize| ins.iter().position(|x| x.0 == 1014 && x.1 as i64 == l).or(Some(from));
    for _ in 0..1024 {
        let pc = run.events[ev].pc;
        let Some((bank, id, v)) = ins.get(pc) else {
            // The end of its instructions ends it (the rest behaviour is what a rest at a
            // Sculptor's Idol does to it, not the end).
            end(run, ev, false);
            return;
        };
        let i = |k: usize| v.get(k).copied().unwrap_or(0.0) as i64;
        let f = |k: usize| v.get(k).copied().unwrap_or(0.0);
        let mut next = pc + 1;
        if is_condition(*bank) {
            let g = i(0) as i8;
            let c = if (*bank, *id) == (0, 0) { Cond::Group(i(2) as i8, i(1) == 1) } else { Cond::Pred(pc, run.events[ev].t) };
            if g != 0 {
                run.events[ev].groups.entry(g).or_default().push(c);
                run.events[ev].pc = next;
                continue;
            }
            let c = run.events[ev].main.clone().unwrap_or(c);
            if !eval(run, ev, w, &c) {
                run.events[ev].main = Some(c);
                return;
            }
            let gs: Vec<i8> = run.events[ev].groups.keys().copied().collect();
            let compiled: Vec<(i8, bool)> = gs.iter().map(|g| (*g, group(run, ev, w, *g))).collect();
            let e = &mut run.events[ev];
            e.compiled = compiled.into_iter().collect();
            e.groups.clear();
            e.main = None;
            e.pc = next;
            continue;
        }
        let state = |run: &Run, g: i64| group(run, ev, w, g as i8);
        let compiled = |run: &Run, g: i64| run.events[ev].compiled.get(&(g as i8)).copied().unwrap_or(false);
        match (*bank, *id) {
            (1000, 1) => {
                if state(run, i(2)) == (i(1) == 1) {
                    next += i(0) as usize;
                }
            }
            (1000, 2) => {
                if state(run, i(2)) == (i(1) == 1) {
                    end(run, ev, i(0) == 1);
                    return;
                }
            }
            (1000, 3) => next += i(0) as usize,
            (1000, 4) => {
                end(run, ev, i(0) == 1);
                return;
            }
            (1000, 5) => {
                if cmp(f(2), i(1), f(3)) {
                    next += i(0) as usize;
                }
            }
            (1000, 7) => {
                if compiled(run, i(2)) == (i(1) == 1) {
                    next += i(0) as usize;
                }
            }
            (1000, 101) => {
                if state(run, i(2)) == (i(1) == 1) {
                    next = label(i(0), next).unwrap_or(next);
                }
            }
            (1000, 103) => next = label(i(0), next).unwrap_or(next),
            (1000, 107) => {
                if compiled(run, i(2)) == (i(1) == 1) {
                    next = label(i(0), next).unwrap_or(next);
                }
            }
            (1001, 0) | (1001, 1) | (1001, 2) => {
                let secs = match id {
                    0 => f(0) as f32,
                    1 => i(0) as f32 / 60.0,
                    _ => {
                        let r = (run.rand() % 10_000) as f32 / 10_000.0;
                        f(0) as f32 + (f(1) - f(0)) as f32 * r
                    }
                };
                run.events[ev].pc = next;
                if secs > 0.0 {
                    run.events[ev].wait = secs;
                    return;
                }
                continue;
            }
            (1003, 0) => {
                if run.flag_of(i(1), i(2), ev) != (i(0) == 1) {
                    return;
                }
            }
            (1003, 1) => {
                if run.flag_of(i(2), i(3), ev) == (i(1) == 1) {
                    next += i(0) as usize;
                }
            }
            (1003, 2) => {
                if run.flag_of(i(2), i(3), ev) == (i(1) == 1) {
                    end(run, ev, i(0) == 1);
                    return;
                }
            }
            (1003, 3) => {
                if batch(run, i(1), i(3), i(4)) {
                    next += i(0) as usize;
                }
            }
            (1003, 11) | (1003, 111) | (1003, 112) => {
                let has = w.get(i(1)).is_some_and(|c| counted(c.sp.contains(&i(2)) == (i(3) == 1), i(4), f(5)));
                if has {
                    match id {
                        11 => next = label(i(0), next).unwrap_or(next),
                        111 => {
                            end(run, ev, i(0) == 1);
                            return;
                        }
                        _ => next += i(0) as usize,
                    }
                }
            }
            (1003, 101) => {
                if run.flag_of(i(2), i(3), ev) == (i(1) == 1) {
                    next = label(i(0), next).unwrap_or(next);
                }
            }
            (1003, 103) => {
                if batch(run, i(1), i(3), i(4)) {
                    next = label(i(0), next).unwrap_or(next);
                }
            }
            (2003, 2) => {
                let on = match i(1) {
                    0 => false,
                    1 => true,
                    _ => !run.flag(i(0)),
                };
                // The boss-defeated flags (93xx: Genichiro 9300, Gyoubu 9301, the Ape 9304, Isshin
                // 9312, each set by its defeat event) end the duel too when no Handle Boss Defeat
                // does: Genichiro at the castle top gets away in a cutscene (m11_02 11125800).
                // Only with the boss at 0 health bars (Isshin's entry, 11125830, sets the naked
                // Genichiro's 9311).
                let boss_out = w.chars.get(&run.def.boss).is_some_and(|c| c.bars == 0);
                if on && boss_out && (9300..9400).contains(&i(0)) && !run.flag(i(0)) {
                    acts.push(Act::BossDefeat);
                }
                run.flags.insert(i(0), on);
            }
            (2003, 17) => {
                let (a, b) = (i(0), i(1).max(i(0)));
                let pick = a + (run.rand() as i64).rem_euclid(b - a + 1);
                run.flags.insert(pick, i(2) == 1);
            }
            (2003, 22) => {
                for fl in i(0)..=i(1) {
                    run.flags.insert(fl, i(2) == 1);
                }
            }
            (2003, 31) => {
                let v = (run.value(i(0), i(1)) + 1).min(i(2));
                run.set_value(i(0), i(1), v);
            }
            (2003, 1) => acts.push(Act::ForceAnim(i(0), i(1), i(2) == 1)),
            (2003, 5) => acts.push(Act::Shoot(i(0), i(1), i(2), i(3))),
            (2004, 61) => acts.push(Act::Warp(i(0), 0, i(0), false)),
            (2003, 12) | (2003, 74) => acts.push(Act::BossDefeat),
            (2003, 18) => acts.push(Act::ForceAnim(i(0), i(1), i(2) == 1)),
            (2002, 4) | (2002, 13) => acts.push(Act::CutsceneWarp(i(2))),
            (2004, 1) => acts.push(Act::AiState(i(0), i(1) == 1)),
            (2004, 4) => acts.push(Act::Death(i(0))),
            (2004, 5) => acts.push(Act::Enable(i(0), i(1) == 1)),
            (2004, 6) => acts.push(Act::EzState(i(0), i(1))),
            (2004, 8) => acts.push(Act::SetSp(i(0), i(1))),
            (2004, 12) => acts.push(Act::Immortal(i(0), i(1) == 1)),
            (2004, 17) => acts.push(Act::AiCommand(i(0), i(1), i(2))),
            (2004, 19) => acts.push(Act::AiId(i(0), i(1))),
            (2004, 20) => acts.push(Act::Replan(i(0))),
            (2004, 21) => acts.push(Act::ClearSp(i(0), i(1))),
            (2004, 40) | (2004, 41) | (2004, 42) => acts.push(Act::Warp(i(0), i(1), i(2), *id == 41)),
            _ => {}
        }
        run.events[ev].pc = next;
    }
}

/// SHINOBI_SCRIPT_TRACE=1: print the script's requests and event ends (checks).
pub(crate) fn trace() -> bool {
    static T: OnceLock<bool> = OnceLock::new();
    *T.get_or_init(|| std::env::var("SHINOBI_SCRIPT_TRACE").is_ok())
}

fn end(run: &mut Run, ev: usize, restart: bool) {
    if trace() {
        eprintln!("script: event {} slot {} {}", run.def.events[ev].id, run.def.events[ev].slot, if restart { "restarts" } else { "ends" });
    }
    let (id, slot) = (run.def.events[ev].id, run.def.events[ev].slot);
    let e = &mut run.events[ev];
    if restart {
        // (From its first instruction: a restart is the game's own.)
        *e = EvState { t: 0.0, ..Default::default() };
    } else {
        e.done = true;
        run.flags.insert(id, true);
        run.flags.insert(id + slot, true);
    }
}

/// Wolf's event messages: his anim's TAE 936 (unk0) once started (the Todome's 10-13).
/// gap: how long the exe keeps them is not traced; here while that anim plays.
fn wolf_msg(combat: &Combat, pa: &Actor) -> Vec<i64> {
    let Some(an) = combat.player.anim(&pa.anim) else { return Vec::new() };
    an.events.iter().filter(|e| e.kind == super::TAE_EVENT_MESSAGE && e.start <= pa.t).filter_map(|e| e.arg_i64("unk0")).collect()
}

/// Starts a boss's script when its boss is on the field, runs every event, then applies what
/// they did.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub(super) fn run_scripts(
    mut commands: Commands,
    time: Res<Time>,
    combat: Res<Combat>,
    config: Res<GameConfig>,
    mut bs: ResMut<BossScript>,
    mut fight: ResMut<super::FightReset>,
    mut warp: ResMut<super::events::EventWarp>,
    mut player: Single<(&mut Actor, &Transform), (With<Player>, Without<Enemy>)>,
    wolf: Single<(Entity, &Player), Without<Enemy>>,
    mut shots: ResMut<crate::enemy_bullet::ScriptShots>,
    mut throw: ResMut<crate::player::ActiveThrow>,
    mut q: Query<(Entity, &mut Enemy, &mut Actor, &mut Transform, Option<&Scripted>, Option<&mut Phantom>, Option<&mut Visibility>), Without<Player>>,
    mut brains: NonSendMut<super::Brains>,
) {
    let dt = time.delta_secs();
    // Start: the first enemy whose NpcParam row has a script.
    if bs.run.is_none() {
        for (ent, e, a, _, s, _, _) in &q {
            if s.is_some() {
                continue;
            }
            let row = combat.foe_of(&a).npc_row;
            if let Some(def) = scripts().get(&row) {
                let mut run = Run::new(row, def.clone(), e.start, ent, e.rng ^ 0x5EED_1234);
                // The cast the events only ever disable (Change Character Enable State 2004[5]
                // with 0, never 1) is on the map from the start: the Divine Dragon's tentacles
                // 2500880-2 (m25 12505911 takes them away).
                let mut cast: Vec<i64> = def.chars.keys().copied().filter(|&id| id != def.boss).collect();
                cast.sort();
                for id in cast {
                    let enabled = def.events.iter().any(|ev| ev.ins.iter().any(|x| (x.0, x.1) == (2004, 5) && x.2.first().map(|v| *v as i64) == Some(id) && x.2.get(1).map(|v| *v as i64) == Some(1)));
                    if !enabled {
                        run.pending.push(Act::Enable(id, true));
                    }
                }
                bs.run = Some(run);
                commands.entity(ent).insert(Scripted(def.boss));
                info!("boss script: {} {} ({} events)", def.map, row, def.events.len());
                break;
            }
        }
        return;
    }
    let Some(run) = bs.run.as_mut() else { return };
    // Its cast fights the way the boss does (the real AI or deflect practice, which can change
    // mid-fight): the Corrupted Monk's phantoms spawn before that is set.
    let boss_aggr = q.iter().find(|x| x.4.is_some_and(|s| s.0 == run.def.boss)).map(|x| x.1.aggressive);
    if let Some(on) = boss_aggr {
        for (_, mut e, _, _, s, ..) in &mut q {
            if s.is_some() && e.aggressive != on {
                e.aggressive = on;
            }
        }
    }
    // A defeated boss: the duel starts over a while later.
    // gap: the game has no restart; 5 s after Handle Boss Defeat here.
    if let Some(t) = run.defeated.as_mut() {
        *t += dt;
        if *t >= 5.0 {
            fight.0 = true;
            return;
        }
    }
    // The snapshot the conditions read.
    let (pa, ptf) = (&player.0, player.1);
    let mut chars: HashMap<i64, Snap> = HashMap::new();
    // The boss kneels for Wolf's finisher (player.rs): not a stuck death.
    let mut kneeling = false;
    let prev = run.prev_hp.insert(PLAYER, pa.hp).unwrap_or(pa.hp);
    // Wolf's SpEffects: his anim's, the timed ones (the Divine Dragon's lightning signs put
    // 3531061-5 on him: m25 12505916 shoots its lightning on them) and his Lightning Reversal
    // charge.
    let mut wolf_sp: HashSet<i64> = wolf.1.timed.iter().map(|t| t.0).collect();
    if !pa.anim.is_empty() {
        wolf_sp.extend(combat.player.sp_effects_at(&pa.anim, pa.t).iter().map(|(id, _)| *id));
    }
    wolf_sp.extend(pa.electro.map(|c| c.0));
    chars.insert(
        PLAYER,
        Snap { pos: ptf.translation, hp: pa.hp, hp_max: pa.hp_max, msg: wolf_msg(&combat, pa), ai_state: 3, damaged: pa.hp < prev, sp: wolf_sp, ..Default::default() },
    );
    for (_, e, a, tf, s, ph, _) in &q {
        let Some(Scripted(id)) = s else { continue };
        if ph.is_some_and(|p| p.out) || e.disabled {
            continue;
        }
        let d = combat.data_of(&a);
        if *id == run.def.boss && e.is_dead() && d.finisher_open(&a.anim, a.t) && combat.finisher(&combat.foe_of(&a)).is_some() {
            kneeling = true;
        }
        if trace() {
            eprintln!("anim {id}: {} {:.2} {:?} ai_off {} {:?} tgt {} {:?} wolf {:?} ai {} req {:?}", a.anim, a.t, e.mode, e.ai_off, tf.translation, e.targeting.state, e.targeting.memory.as_ref().map(|m| m.pos), ptf.translation, e.ai_desc, e.event_req);
        }
        let mut sp: HashSet<i64> = a.resident.iter().copied().collect();
        if !a.anim.is_empty() {
            sp.extend(d.sp_effects_at(&a.anim, a.t).iter().map(|(id, _)| *id));
        }
        sp.extend(e.clash.iter().map(|c| c.0));
        // Its additive anim's (the Divine Dragon's PartBlend_Add01-05, a000_009510-4: 3520080
        // hand / body, 3520081 head, which m25 12505927 / 12505890 read with the bolt's 3520085).
        if !a.add_anim.is_empty() {
            sp.extend(d.sp_effects_at(&a.add_anim, a.add_t).iter().map(|(id, _)| *id));
        }
        // The anim-set SpEffect lasts (effectEndurance -1): Isshin's 200031 from his 3015.
        if a.anim_group > 0 {
            sp.insert(200030 + a.anim_group as i64);
        }
        // A death shows a frame late: the killing reaction's first event messages come first
        // (the Corrupted Monk's Todome ThrowDef13700 sends 10 at its start, which ends her
        // immortality: m25 12505965), and an immortal one lives on (see the end).
        let dead = e.is_dead() && a.hp <= 0.0 && !run.dead_ids.insert(*id);
        let prev = run.prev_hp.insert(*id, a.hp).unwrap_or(a.hp);
        chars.insert(
            *id,
            Snap {
                pos: tf.translation,
                hp: a.hp,
                hp_max: a.hp_max,
                posture_ratio: if a.posture_max > 0.0 { (1.0 - a.posture / a.posture_max).max(0.0) } else { 1.0 },
                bars: e.ninsatsu.0,
                dead,
                sp,
                msg: e.msg.into_iter().collect(),
                ai_state: match e.targeting.state {
                    s if s >= crate::stealth::BATTLE => 3,
                    s if s >= crate::stealth::CAUTION => 2,
                    _ => 0,
                },
                damaged: a.hp < prev,
            },
        );
    }
    // The boss lying dead with no defeat handled (its script waits on something this arena
    // lacks): the duel starts over. gap: no such rule in the game.
    if chars.get(&run.def.boss).is_some_and(|c| c.dead) && run.defeated.is_none() && !kneeling {
        run.dead_for += dt;
        if run.dead_for >= 15.0 {
            warn!("boss script: {} dead without a defeat event", run.row);
            fight.0 = true;
            return;
        }
    } else {
        run.dead_for = 0.0;
    }
    if trace() {
        let line: Vec<String> = chars.iter().map(|(id, c)| format!("{id}: bars {} hp {:.0} msg {:?} dead {}", c.bars, c.hp, c.msg, c.dead)).collect();
        eprintln!("snap {}", line.join(" | "));
    }
    let w = World { chars: &chars };
    let mut acts = std::mem::take(&mut run.pending);
    for ev in 0..run.events.len() {
        step(run, ev, &w, dt, &mut acts);
    }
    run.frame += 1;
    if trace() {
        for act in &acts {
            eprintln!("script {:.2}: {act:?}", run.events.first().map_or(0.0, |e| e.t));
        }
    }
    // Apply.
    let ent_of = |run: &Run, id: i64| run.ents.get(&id).copied();
    let aggressive = boss_aggr.unwrap_or(false);
    let finished: HashSet<Entity> = q.iter().filter(|x| x.1.mode == Mode::Dead(f32::INFINITY)).map(|x| x.0).collect();
    for act in acts {
        match act {
            Act::Enable(id, true) => {
                if id == PLAYER {
                    continue;
                }
                if let Some(&ent) = run.ents.get(&id) {
                    // Back where it was disabled.
                    let Some((pos, yaw)) = run.disabled.remove(&id) else { continue };
                    let Ok((_, mut e, mut a, mut tf, _, _, vis)) = q.get_mut(ent) else {
                        run.disabled.insert(id, (pos, yaw));
                        run.pending.push(act);
                        continue;
                    };
                    e.disabled = false;
                    tf.translation = pos;
                    a.yaw = yaw;
                    if let Some(mut vis) = vis {
                        *vis = Visibility::Inherited;
                    }
                    continue;
                }
                let Some(c) = run.def.chars.get(&id).cloned() else { continue };
                let Some(kind) = combat.kind_index(&c.chr, Some(c.npc)) else {
                    warn!("boss script: {} {} is not loaded", c.chr, c.npc);
                    continue;
                };
                let Some((mut pos, mut yaw)) = run.place_of(id) else { continue };
                if let Some(p) = run.pending_warp.remove(&id) {
                    (pos, yaw) = p;
                }
                let short = run.def.events.iter().any(|ev| ev.ins.iter().any(|x| (x.0, x.1) == (2004, 41) && x.2.first().map(|v| *v as i64) == Some(id)));
                let rng = run.rand();
                let ent = super::spawn_one(&mut commands, &combat, &config, kind, if short { PARK } else { pos }, yaw, aggressive, rng);
                commands.entity(ent).insert((Scripted(id), Cast));
                if short {
                    commands.entity(ent).insert((Phantom { out: true, before: Vec::new(), went: Vec::new(), resting: false }, Visibility::Hidden));
                }
                run.ents.insert(id, ent);
                run.dead_ids.remove(&id);
                info!("boss script: {} {} enabled", c.chr, id);
            }
            Act::Enable(id, false) => {
                // Out of the fight, hidden and far away, until enabled again.
                let Some(&ent) = run.ents.get(&id) else { continue };
                let Ok((_, mut e, mut a, mut tf, _, _, vis)) = q.get_mut(ent) else {
                    run.pending.push(act);
                    continue;
                };
                if e.disabled {
                    continue;
                }
                e.disabled = true;
                run.disabled.insert(id, (tf.translation, a.yaw));
                tf.translation = PARK;
                a.move_vel = Vec3::ZERO;
                if let Some(mut vis) = vis {
                    *vis = Visibility::Hidden;
                }
            }
            Act::CutsceneWarp(point) => {
                if let Some(p) = run.place_of(point) {
                    warp.0 = Some(p);
                }

            }
            Act::Warp(who, ty, target, short) => {
                let to = match ty {
                    2 if target == PLAYER => Some((player.1.translation, player.0.yaw)),
                    2 => ent_of(run, target).and_then(|t| q.get(t).ok()).map(|x| (x.3.translation, x.2.yaw)),
                    _ => run.place_of(target),
                };
                let Some((pos, yaw)) = to else { continue };
                if who == PLAYER {
                    warp.0 = Some((pos, yaw));
                    continue;
                }
                let Some(ent) = ent_of(run, who) else {
                    run.pending_warp.insert(who, (pos, yaw));
                    continue;
                };
                let Ok((_, mut e, mut a, mut tf, _, ph, vis)) = q.get_mut(ent) else {
                    run.pending.push(act);
                    continue;
                };
                // A short warp request on one in the fight: once per event message it sent (the
                // Corrupted Monk's 3032 sends 50, and m25 12505981 asks for her warp 1.2 s later
                // and again each time it restarts while she still has it).
                // gap: how the exe takes the request (her vanish's state) is not traced.
                if short && !ph.as_ref().is_some_and(|p| p.out) {
                    if run.warp_serial.get(&who) == Some(&e.msg_serial) {
                        continue;
                    }
                    run.warp_serial.insert(who, e.msg_serial);
                }
                tf.translation = Vec3::new(pos.x, run.start.y, pos.z);
                // gap: the dummy poly offset is dropped; a warp onto another character faces Wolf.
                let to_p = (player.1.translation - tf.translation).with_y(0.0);
                a.yaw = if ty == 2 && to_p.length_squared() > 1e-4 { f32::atan2(-to_p.x, -to_p.z) } else { yaw };
                a.move_vel = Vec3::ZERO;
                if let Some(mut ph) = ph {
                    if ph.out {
                        ph.out = false;
                        ph.before = e.event_req.clone();
                        ph.went.clear();
                        ph.resting = false;
                        e.mode = Mode::Ai;
                        e.cur_ez = None;
                        if let Some(mut vis) = vis {
                            *vis = Visibility::Inherited;
                        }
                    }
                }
            }
            Act::BossDefeat => {
                run.defeated.get_or_insert(0.0);
            }
            Act::Shoot(owner, source, dmy, behavior) => {
                let Some(&first) = run.def.bullets.get(&behavior.to_string()) else { continue };
                let src = if source == PLAYER { Some(wolf.0) } else { ent_of(run, source) };
                let (Some(owner), Some(source)) = (ent_of(run, owner), src) else { continue };
                shots.0.push(crate::enemy_bullet::ScriptShot { def: run.def.clone(), row: first, owner, source, dmy: dmy as i16 });
            }
            Act::EzState(PLAYER, id) => {
                // Not while his deathblow holds one that was already finished before these
                // requests (m11_02 11125874 again on his finisher's message 10, m25 12505963 while
                // his Todome's lasts): only the first cuts it short. gap: the exe's rule for
                // taking EzState requests mid-throw is not traced.
                if throw.0.as_ref().is_some_and(|t| finished.contains(&t.target)) {
                    continue;
                }
                if player.0.play_state(&combat.player, &format!("Event{id}")) {
                    throw.0 = None;
                }
            }
            _ => {
                let id = match &act {
                    Act::SetSp(i, _) | Act::ClearSp(i, _) | Act::AiCommand(i, _, _) | Act::Replan(i) | Act::AiId(i, _) | Act::AiState(i, _) => *i,
                    Act::ForceAnim(i, _, _) | Act::EzState(i, _) | Act::Immortal(i, _) | Act::Death(i) => *i,
                    _ => continue,
                };
                let Some(ent) = ent_of(run, id) else { continue };
                let Ok((_, mut e, mut a, _, _, ph, _)) = q.get_mut(ent) else {
                    // Spawned this frame.
                    run.pending.push(act);
                    continue;
                };
                apply(&combat, &mut brains, ent, &mut e, &mut a, ph, act);
            }
        }
    }
    // Phantoms: back out once resting and their attack is over.
    for (_, e, mut a, mut tf, _, ph, vis) in &mut q {
        let (Some(mut ph), Some(mut vis)) = (ph, vis) else { continue };
        if !ph.out && ph.resting && !e.is_attacking() {
            ph.out = true;
            tf.translation = PARK;
            a.move_vel = Vec3::ZERO;
            *vis = Visibility::Hidden;
        }
    }
    // Set Character Immortality: HP stays at 1 or more; killed by a deathblow it lives on at 0
    // health bars in its reaction (the Guardian Ape's first Todome: m17 11705810 sets it, and
    // only his second set's ThrowDef12000 sends the 10 that ends it, 11705823).
    // Not in a boss's last deathblow, whose reaction adds SpEffect 201000 ("イベント制御_ボスBGM停止",
    // boss BGM stop): Gyoubu's ThrowDef12090 kills him though m11_00 11105820 keeps him immortal
    // since his first deathblow's message 50, and 11105800 waits for his death. gap: the exe's
    // rule is not traced; no fake death (the Ape's first a000_013700) has 201000.
    for (_, mut e, mut a, _, s, ..) in &mut q {
        if !e.immortal || a.hp >= 1.0 {
            continue;
        }
        let last = combat.data_of(&a).anim(&a.anim).is_some_and(|an| {
            an.events.iter().any(|ev| matches!(ev.kind, 66 | 67) && ev.arg_i64("SpEffectID") == Some(201000))
        });
        if last && e.is_dead() {
            continue;
        }
        a.hp = 1.0;
        if e.is_dead() {
            e.ninsatsu.0 = 0;
            e.throw_death = None;
            e.mode = Mode::Held(false);
            if let Some(Scripted(id)) = s {
                run.dead_ids.remove(id);
            }
        }
    }
}

/// A script request on one of its characters.
fn apply(combat: &Combat, brains: &mut super::Brains, ent: Entity, e: &mut Enemy, a: &mut Actor, ph: Option<Mut<Phantom>>, act: Act) {
    let d = combat.data_of(a);
    match act {
        Act::SetSp(_, sp) => {
            if !a.resident.contains(&sp) {
                a.resident.push(sp);
            }
            if let Some(g) = super::anim_group_of(d, std::iter::once(sp)) {
                a.anim_group = g;
            }
        }
        Act::ClearSp(_, sp) => a.resident.retain(|&x| x != sp),
        Act::AiCommand(_, cmd, slot) => {
            if let Some(mut ph) = ph {
                if !ph.out {
                    let old = ph.before.iter().find(|b| b.0 == slot).map(|b| b.1);
                    if old != Some(cmd) {
                        ph.went.push(slot);
                    } else if ph.went.contains(&slot) {
                        ph.resting = true;
                    }
                }
            }
            e.event_req.retain(|&(s, _)| s != slot);
            e.event_req.push((slot, cmd));
        }
        Act::Replan(_) => e.phase_replan = true,
        Act::AiId(_, think) => {
            // Set Character AI ID: its NpcThinkParam row (the Guardian Ape's 51000100 headless).
            e.think_override = Some(think);
            brains.map.remove(&ent);
        }
        Act::AiState(_, on) => {
            e.ai_off = !on;
            // A looping forced anim with the AI on plays once more, then the AI goes on (the
            // Guardian Ape's 20005 roar, m17 11705810: forced looping, then his AI on).
            if on && e.mode == Mode::Held(true) {
                e.mode = Mode::Rising((d.length(&a.anim) - a.t).max(0.1));
            }
        }
        Act::ForceAnim(_, anim, looped) => {
            // Not on a dead one (m25 12505964's 20021 would cut the Corrupted Monk's Todome
            // short: her a100_013700 has no 3500010). gap: the exe's rule is not traced.
            if e.is_dead() && a.hp <= 0.0 {
                return;
            }
            if !play_event(d, a, anim) {
                return;
            }
            e.cur_ez = None;
            e.throw_death = None;
            a.move_vel = Vec3::ZERO;
            e.mode = if looped && e.ai_off { Mode::Held(true) } else { Mode::Rising(d.length(&a.anim).max(0.1)) };
        }
        Act::EzState(_, id) => {
            // Asked again while already dead from it (m25 12505963 restarts while Wolf's
            // Todome message 10 lasts): it keeps playing. gap: the exe's rule is not traced.
            if e.mode == Mode::Dead(f32::INFINITY) {
                return;
            }
            if !play_event(d, a, id) {
                return;
            }
            e.cur_ez = None;
            e.throw_death = None;
            a.move_vel = Vec3::ZERO;
            let len = d.length(&a.anim).max(0.1);
            if e.is_dead() && a.hp <= 0.0 {
                // Its death (20200 after the Todome): held until the script ends the fight.
                e.mode = Mode::Dead(f32::INFINITY);
            } else if e.ninsatsu.0 == 0 && id == 20200 {
                // At 0 health bars but kept alive (immortal: the Guardian Ape's first "death").
                e.down = true;
                e.mode = Mode::Held(false);
            } else if e.ninsatsu.0 == 0 {
                // Getting up from it (the Ape's 20000 after his 20200).
                // gap: what the game gives back is not traced: full HP, posture and one bar.
                e.down = false;
                a.hp = a.hp_max;
                a.posture = 0.0;
                e.ninsatsu.0 = e.ninsatsu.1.max(1);
                e.mode = Mode::Rising(len);
            } else {
                e.mode = Mode::Rising(len);
            }
        }
        Act::Immortal(_, on) => e.immortal = on,
        Act::Death(_) => {
            if !(e.is_dead() && a.hp <= 0.0) {
                a.hp = 0.0;
                e.ninsatsu.0 = 0;
                e.cur_ez = None;
                if !a.play_state(d, "DeathStart") && !a.play_state(d, "Event20200") {
                    a.procedural("Dead");
                }
                e.mode = Mode::Dead(f32::INFINITY);
            }
        }
        _ => {}
    }
}

/// Plays EzState / anim `id` (state Event<id>, else its clip a000_<id> in the anim set).
fn play_event(d: &crate::data::CharData, a: &mut Actor, id: i64) -> bool {
    let state = format!("Event{id}");
    if a.play_state(d, &state) {
        return true;
    }
    let key = a.grouped(&format!("a000_{id:06}"));
    if d.anim(&key).is_none() {
        return false;
    }
    a.play(&state, &key);
    true
}
