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

/// An object the script attaches to a character (2003[58]), with the grapple records of its
/// e<id>.edge (tools/boss_scripts.py attached_objects): [hook, landing, WireVariationParam row],
/// in the object's own (FromSoftware, unmirrored) space.
#[derive(serde::Deserialize, Clone, Debug)]
pub struct ObjDef {
    pub model: String,
    #[serde(default)]
    pub edges: Vec<([f32; 3], [f32; 3], i64)>,
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

/// Whether a boss-frame point (x right, y up, z ahead) is in a shape at `at` / `yaw` (degrees).
/// `y` None: seen from above (off the boss's own arena, where events::place drops heights too).
/// A sphere is round about `at`; a cylinder and a box stand on it, `height` up (DSMapStudio
/// DbgPrimWireCylinder bottom 0 / top height, RenderableProxy region box -0.5,0,-0.5 .. 0.5,1,0.5);
/// a circle and a rectangle have no height. gap: the region's x / z rotation is dropped.
fn shape_contains(shape: i32, size: &[f32], at: [f32; 3], yaw: f32, x: f32, y: Option<f32>, z: f32) -> bool {
    let s = |i: usize| size.get(i).copied().unwrap_or(0.0);
    if let Some(y) = y {
        let dy = y - at[1];
        let fits = match shape {
            2 => dy.abs() <= s(0),
            3 => (0.0..=s(1)).contains(&dy),
            5 => (0.0..=s(2)).contains(&dy),
            _ => true,
        };
        if !fits {
            return false;
        }
    }
    let (dx, dz) = (x - at[0], z - at[2]);
    let (sin, cos) = yaw.to_radians().sin_cos();
    // The map's yaw turns +Z toward +X: right = (cos, -sin), ahead = (sin, cos).
    let (lx, lz) = (dx * cos - dz * sin, dx * sin + dz * cos);
    match shape {
        1 | 3 => lx * lx + lz * lz <= s(0) * s(0),
        2 => {
            let dy = y.map_or(0.0, |y| y - at[1]);
            lx * lx + dy * dy + lz * lz <= s(0) * s(0)
        }
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
    /// Characters the row's fight starts after (dead from the first frame): Lady Butterfly's
    /// first form 1000800 for her row after the change (tools/boss_scripts.py dead_at_start).
    #[serde(default)]
    pub dead_at_start: Vec<i64>,
    /// Objects the events attach to characters, by entity id (the Divine Dragon's trees' grapple
    /// points o259920-922).
    #[serde(default)]
    pub objects: HashMap<i64, ObjDef>,
}

impl Run {
    /// The active attached objects that carry grapple edges: (the character's entity, its dummy
    /// poly, the object).
    pub fn grapple_objects(&self) -> Vec<(Entity, i16, &ObjDef)> {
        let mut v: Vec<(i64, Entity, i16, &ObjDef)> = self
            .attached
            .iter()
            .filter(|(_, o)| o.2)
            .filter_map(|(id, o)| Some((*id, *self.ents.get(&o.0)?, o.1, self.def.objects.get(id).filter(|d| !d.edges.is_empty())?)))
            .collect();
        v.sort_by_key(|x| x.0);
        v.into_iter().map(|x| (x.1, x.2, x.3)).collect()
    }
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

/// Set Character Immortality on Wolf (2004[12] on 10000): the prologue Genichiro's 11125812 while
/// Wolf is in region 1122802. HP stays at 1 or more while on (the NoDead clamp, SetHp
/// FUN_140bd64e0). Set by the script; the player side clamps (player.rs / combat.rs).
#[derive(Resource, Default)]
pub struct WolfImmortal(pub bool);

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
    /// The defeat's Text Banner Type (EMEVD 2003[74] arg 1: 1 Shinobi Execution, 5 Immortality
    /// Severed, 3 Gracious Gift of Tears); 2003[12] and the flag path take 1. Round 93.
    pub banner: i64,
    /// Time the boss part has lain dead without one.
    dead_for: f32,
    /// The character 2003[11] put the boss bar on (the Old Dragons' shared bar body 2500810,
    /// NpcParam 53000000 with no deathblow dots: m25 12505801 `2003[11](1, 2500810, 0, 905300)`,
    /// swapped for the Divine Dragon's 2500800 by 12505885). None: the ninsatsu rule (hud.rs).
    pub bar: Option<i64>,
    /// Wolf's Set Character Immortality (mirrored into WolfImmortal).
    pub wolf_immortal: bool,
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
    /// Objects attached to characters (2003[58] Attach Object to Character): object -> (the
    /// character, its dummy poly, active: 2005[3] (De)activate Object).
    attached: HashMap<i64, (i64, i16, bool)>,
    /// Its talk (enemy/talk.rs) was looked up: once per fight.
    pub talk_seen: bool,
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
    /// NpcParam ninsatuNum: 0 on the Old Dragons' shared bar body 2500810 (53000000), whose
    /// `bars` is 0 from the first frame.
    bars_max: u32,
    dead: bool,
    sp: HashSet<i64>,
    /// Its event messages (an enemy's one, Wolf's several: his Todome sends 10-13 together).
    msg: Vec<i64>,
    ai_state: i64,
    damaged: bool,
    /// Wolf only: on the ground (not airborne).
    grounded: bool,
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
    /// warp request (2004[41]), the target character's dummy poly (-1: its root).
    Warp(i64, i64, i64, bool, i64),
    Immortal(i64, bool),
    Death(i64),
    /// The defeat with its Text Banner Type.
    BossDefeat(i64),
    /// 2003[11] Display Boss Health Bar: the character whose bar the HUD shows (on / off).
    BossBar(i64, bool),
    /// The fight's scripted loss (start over).
    ScriptedLoss,
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
        // An event entered past its start (`entry`: at the boss's enable) still left the
        // immortality its skipped setup sets: m11_00 11105910 sets the Demon of Hatred's (2004[12],
        // its 4th instruction) before its region wait and his enable at the 10th, and only his
        // Todome's message 10 ends it (11105920); without it his third deathblow killed him and
        // the 20200 kneel for the Todome (11105921, rec_20261016_072651) never came.
        let mut pending = Vec::new();
        for ev in def.events.iter().filter(|ev| !ev.done && ev.start > 0) {
            for (bank, id, v) in &ev.ins[..ev.start.min(ev.ins.len())] {
                if (*bank, *id) == (2004, 12) && v.len() >= 2 {
                    pending.push(Act::Immortal(v[0] as i64, v[1] as i64 == 1));
                }
            }
        }
        let mut ents = HashMap::new();
        let dead_ids: HashSet<i64> = def.dead_at_start.iter().copied().collect();
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
            banner: 0,
            dead_for: 0.0,
            bar: None,
            wolf_immortal: false,
            dead_ids,
            pending_warp: HashMap::new(),
            pending,
            disabled: HashMap::new(),
            frame: 0,
            warp_serial: HashMap::new(),
            attached: HashMap::new(),
            talk_seen: false,
        }
    }

    fn rand(&mut self) -> u32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        self.rng
    }

    /// Arena position and yaw of an MSB entity of this script.
    pub(crate) fn place_of(&self, id: i64) -> Option<(Vec3, f32)> {
        let (at, yaw) = if let Some(c) = self.def.chars.get(&id) {
            (c.at, c.yaw)
        } else {
            let p = self.def.places.get(&id)?;
            (p.at, p.yaw)
        };
        let at = serde_json::json!(at);
        Some(super::events::place(self.start, &at, &serde_json::json!(yaw)))
    }

    /// Is the world point inside the MSB region? Heights count on the boss's own arena only.
    fn inside(&self, region: i64, p: Vec3) -> bool {
        let (Some(pl), Some(sh)) = (self.def.places.get(&region), self.def.regions.get(&region)) else { return false };
        // Into the boss frame (see events::place).
        let (x, z) = (self.start.x - p.x, p.z - self.start.z);
        let y = super::on_own_arena().then(|| p.y - self.start.y);
        if sh.shape == 6 {
            return sh.parts.iter().any(|k| shape_contains(k.shape, &k.size, k.at, k.yaw, x, y, z));
        }
        shape_contains(sh.shape, &sh.size, pl.at, pl.yaw, x, y, z)
    }

    /// The boss's entity id in the map (its part's).
    pub fn boss(&self) -> i64 {
        self.def.boss
    }

    pub(crate) fn flag(&self, id: i64) -> bool {
        self.flags.get(&id).copied().unwrap_or(false)
    }

    /// An event flag the AI set (SetEventFlag; enemy.rs think).
    pub fn set_flag(&mut self, id: i64, on: bool) {
        self.flags.insert(id, on);
    }

    /// The event flags that are on (for the AI's IsEventFlag).
    pub fn flags_on(&self) -> Vec<i64> {
        self.flags.iter().filter(|(_, on)| **on).map(|(id, _)| *id).collect()
    }

    /// The boss rows of this run's cast: the picked row first, then every other character of
    /// the script that has a boss script of its own (the next boss the fight goes on with).
    pub fn cast_rows(&self) -> Vec<i64> {
        let mut rows = vec![self.row];
        let mut others: Vec<i64> = self.def.chars.values().map(|c| c.npc).filter(|n| *n != self.row && scripts().contains_key(n)).collect();
        others.sort_unstable();
        others.dedup();
        rows.extend(others);
        rows
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
        // IF Player Standing On Hit (collision part): Isshin Ashina's entry (m11_01 11115930)
        // waits on Wolf standing on the castle top's 1114801-804. gap: the arena keeps no
        // collision ids, so any ground counts.
        (11, 2) => w.chars.get(&PLAYER).is_some_and(|c| c.grounded),
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
                // Genichiro's 9311) - of the bars it has: the Old Dragons' bar body 2500810 has
                // none (ninsatuNum 0), so m25 12505885's 9318 at its 0 HP counted as the defeat and
                // the Divine Dragon phase it starts (AiState 2500800 on, 3520071, the warp) never
                // came (boss bug hunt 2026-10-14).
                let boss_out = w.chars.get(&run.def.boss).is_some_and(|c| c.bars == 0 && c.bars_max > 0);
                // Not when the same event enables another of the cast (2004[05] on): a hand-over,
                // the fight goes on with it (11125830 enables Isshin 1120860 as it sets 9311).
                // Or when it shows another of the cast's boss bar (2003[11] on): m25 12505885 sets
                // the Old Dragons' 9318 and turns the Divine Dragon 2500800 on (AI state, bar) at
                // their 0 HP; the live fight goes on with him (rec_20261016_070725: 12505885 at
                // 120 s, the Divine Dragon from 174 s to his deathblow at 440 s).
                let other = |e: i64| e != run.def.boss && run.def.chars.contains_key(&e);
                let hand_over = ins.iter().any(|x| {
                    let arg = |k: usize| x.2.get(k).map(|v| *v as i64);
                    ((x.0, x.1) == (2004, 5) && arg(1) == Some(1) && arg(0).is_some_and(other)) || ((x.0, x.1) == (2003, 11) && arg(0) == Some(1) && arg(1).is_some_and(other))
                });
                // The scripted loss: the flag with the boss still up and Wolf held at 1 HP by his
                // immortality (the prologue Genichiro's 11125800: Wolf at 1 HP in 1122802 -> 9300,
                // the arm-loss cutscene). The duel starts over.
                let wolf_down = w.chars.get(&PLAYER).is_some_and(|c| c.hp <= 1.0);
                if on && !boss_out && !hand_over && wolf_down && run.wolf_immortal && (9300..9400).contains(&i(0)) && !run.flag(i(0)) {
                    acts.push(Act::ScriptedLoss);
                }
                if on && boss_out && !hand_over && (9300..9400).contains(&i(0)) && !run.flag(i(0)) {
                    acts.push(Act::BossDefeat(1));
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
            // Attach Object to Character (character, dummy poly, object): it rides on the
            // dummy from now on (the trees' o259920-922 on 600-602, m25 12505901), active as
            // it was.
            (2003, 58) => {
                let active = run.attached.get(&i(2)).is_none_or(|o| o.2);
                run.attached.insert(i(2), (i(0), i(1) as i16, active));
            }
            // (De)activate Object: an attached one's grapple edges count only while active
            // (m25 12505906: a tree's point while it wears 5025-5027). gap: 2005[19] Set Object
            // Interaction (switched with it there) is not modelled.
            (2005, 3) => {
                if let Some(o) = run.attached.get_mut(&i(0)) {
                    o.2 = i(1) == 1;
                }
            }
            (2004, 61) => acts.push(Act::Warp(i(0), 0, i(0), false, -1)),
            // 2003[11] Display Boss Health Bar (state, entity, slot, name text id).
            (2003, 11) => acts.push(Act::BossBar(i(1), i(0) == 1)),
            // 2003[12] Handle Boss Defeat (gap: its banner taken as Shinobi Execution, not
            // traced), 2003[74] ... and Display Banner (arg 1, Text Banner Type).
            (2003, 12) => acts.push(Act::BossDefeat(1)),
            (2003, 74) => acts.push(Act::BossDefeat(i(1))),
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
            (2004, 40) | (2004, 41) | (2004, 42) => acts.push(Act::Warp(i(0), i(1), i(2), *id == 41, i(3))),
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
    auto: Res<super::AutoRestart>,
    mut warp: ResMut<super::events::EventWarp>,
    mut player: Single<(&mut Actor, &Transform), (With<Player>, Without<Enemy>)>,
    wolf: Single<(Entity, &Player), Without<Enemy>>,
    mut shots: ResMut<crate::enemy_bullet::ScriptShots>,
    mut throw: ResMut<crate::player::ActiveThrow>,
    mut q: Query<(Entity, &mut Enemy, &mut Actor, &mut Transform, Option<&Scripted>, Option<&mut Phantom>, Option<&mut Visibility>), Without<Player>>,
    mut brains: NonSendMut<super::Brains>,
    wolf_immortal: Option<ResMut<WolfImmortal>>,
    (dummies, markers, wolf_msg): (Query<&crate::model::Dummies>, Query<&GlobalTransform>, Res<super::WolfMessage>),
) {
    let dt = time.delta_secs();
    // (Last frame's: Wolf's immortality as the script left it.)
    if let Some(mut w) = wolf_immortal {
        w.0 = bs.run.as_ref().is_some_and(|r| r.wolf_immortal);
    }
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
    // A defeated boss: the duel starts over a while later, unless the menu shows its Won screen
    // (AutoRestart off). gap: the game has no restart; 5 s after Handle Boss Defeat here.
    if let Some(t) = run.defeated.as_mut() {
        *t += dt;
        if *t >= 5.0 && auto.0 {
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
        Snap { pos: ptf.translation, hp: pa.hp, hp_max: pa.hp_max, msg: wolf_msg.msg.into_iter().collect(), ai_state: 3, damaged: pa.hp < prev, grounded: !pa.airborne, sp: wolf_sp, ..Default::default() },
    );
    for (_, e, a, tf, s, ph, _) in &q {
        let Some(Scripted(id)) = s else { continue };
        if ph.is_some_and(|p| p.out) || e.disabled {
            continue;
        }
        let d = combat.data_of(&a);
        // (Waiting for the Todome in its 20200, or the finisher.)
        let foe = combat.foe_of(&a);
        if *id == run.def.boss && e.is_dead() && d.finisher_open(&a.anim, a.t) && (combat.finisher(&foe).is_some() || combat.throw(foe.throw_row(181)).is_some()) {
            kneeling = true;
        }
        if trace() {
            eprintln!("anim {id}: {} {:.2} {:?} ai_off {} {:?} tgt {} {:?} wolf {:?} ai {} req {:?}", a.anim, a.t, e.mode, e.ai_off, tf.translation, e.targeting.state, e.targeting.memory.as_ref().map(|m| m.pos), ptf.translation, e.ai_desc, e.event_req);
        }
        let mut sp: HashSet<i64> = a.resident.iter().copied().collect();
        if !a.anim.is_empty() {
            sp.extend(d.sp_effects_at(&a.anim, a.t).iter().map(|(id, _)| *id));
            // Through the TAE's last 30 fps frame (its end frame counted in): the Monk's
            // ThrowDef12000 3500010 ("忍殺時イベントアニメ遷移不可", 0-3.67 s) lasts until Wolf's
            // a211_510000 message 10 at 3.70 s, so m25 12505964's 20021 (bars 0, no 3500010,
            // 12505963 not yet run) does not cut her deathblow in the frame between. gap: the exe's
            // TAE SpEffect end is not traced.
            if a.t >= 1.0 / 30.0 {
                sp.extend(d.sp_effects_at(&a.anim, a.t - 1.0 / 30.0).iter().map(|(id, _)| *id));
            }
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
        // And a deathblow kill only once Wolf's Ninsatsu hit has landed (enemy.rs
        // ninsatsu_unhit: the exe applies the kill's damage in that hit, FUN_140b6e6a0, a211_511700
        // at 1.23 s, a225_510090 at 5.9 s), as `bars` already does: IfDead / HP <= 0 events ran at
        // the throw's first frame, so Gyoubu's Handle Boss Defeat (m11_00 11105800: IfDead, Wait
        // 0.7) and the Won timer came ~8 s before the game's (clips.md: the banner 9.5 s after
        // the fatal deathblow). The reaction's ChrActionFlag 12 "Death" (Gyoubu's a000_012090
        // at 8.03 s) is not it: the exe's type-0 handler FUN_140b51b70 (140b5.c:7162) has no
        // case 12, so the flag does nothing; death stays HP / state based (140bd.c:4165 SetHp,
        // no-dead FUN_140bd5df0). gap: the clip's ~9.5 s banner vs the sim's 6.6 s is
        // unexplained (the EMEVD IfCharacterDead handler is not located).
        let pending = e.ninsatsu_unhit > 0;
        // A boss killed by its last deathblow dies for the map script when its reaction's
        // ChrActionFlag 12 "Death" starts (Gyoubu's a000_012090: f241 = 8.03 s; the all-bosses
        // clip's banner 9.5 s after the lunge = 8.03 + 11105800's Wait 0.7 + the banner's fade-in),
        // not at the Ninsatsu hit: the exe's dead test is HP <= 0 (FUN_140bd5d40, 140bd.c:3239)
        // and SetHp clamps an immortal chr to 1, so only the kill routine FUN_1409fa0c0
        // (1409f.c:6930, the unclamped FUN_140bd5590) ends it. gap: its caller - the flag's
        // consumer - is not located; the flag's start stands for that moment.
        let death_flag = d.anim(&a.anim).and_then(|an| an.events.iter().filter(|ev| ev.kind == 0 && ev.flag_type() == Some(12)).map(|ev| ev.start).reduce(f32::min));
        let dead = e.is_dead() && a.hp <= 0.0 && !pending && death_flag.is_none_or(|s| a.t >= s) && !run.dead_ids.insert(*id);
        let hp = if pending { a.hp.max(1.0) } else { a.hp };
        let prev = run.prev_hp.insert(*id, hp).unwrap_or(hp);
        chars.insert(
            *id,
            Snap {
                pos: tf.translation,
                hp,
                hp_max: a.hp_max,
                posture_ratio: if a.posture_max > 0.0 { (1.0 - a.posture / a.posture_max).max(0.0) } else { 1.0 },
                bars: e.ninsatsu_left(),
                bars_max: e.ninsatsu.1,
                dead,
                sp,
                msg: e.msg.into_iter().collect(),
                ai_state: match e.targeting.state {
                    s if s >= crate::stealth::BATTLE => 3,
                    s if s >= crate::stealth::CAUTION => 2,
                    _ => 0,
                },
                damaged: hp < prev,
                grounded: !a.airborne,
            },
        );
    }
    // The boss lying dead with no defeat handled (its script waits on something this arena
    // lacks): the duel starts over. gap: no such rule in the game.
    // (Not while the next boss of the cast fights on: Isshin Ashina after Emma.)
    let next_up = chars.iter().any(|(id, c)| *id != run.def.boss && !c.dead && run.def.chars.get(id).is_some_and(|d| scripts().contains_key(&d.npc)));
    if chars.get(&run.def.boss).is_some_and(|c| c.dead) && run.defeated.is_none() && !kneeling && !next_up {
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
                // The next boss coming in after a handled defeat: the fight goes on with him
                // (Isshin Ashina after Emma: m11_01 11115930 on her defeat event 11115900's flag).
                // (Only a boss of its own row: the Demon's c1001 dummy 1100955 comes back on after
                // his defeat and must not undo it.)
                let next_boss = id != run.def.boss && run.def.chars.get(&id).is_some_and(|c| scripts().contains_key(&c.npc));
                if next_boss && run.defeated.take().is_some() {
                    info!("boss script: {} goes on with {id}", run.row);
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
            // A fight moved onto another boss's arena (map boss_arena, MOVED in tools/boss_arenas.py):
            // its places are in its own map's frame, so warps to them land off the platform (Map's
            // probe: Wolf at the Owl's railing, (1.0, -11.4), 0.5 s in). Wolf keeps the arena's
            // line-up; an enemy warped to a place stays where it is.
            Act::CutsceneWarp(_) if crate::map::on_boss_arena() && !super::on_own_arena() => {}
            Act::CutsceneWarp(point) => {
                if let Some(p) = run.place_of(point) {
                    warp.0 = Some(p);
                }

            }
            Act::Warp(_, ty, _, _, _) if ty != 2 && crate::map::on_boss_arena() && !super::on_own_arena() => {}
            Act::Warp(who, ty, target, short, dmy) => {
                // Onto a character's dummy poly (2004[42] Warp Character and Copy Floor: the
                // Divine Dragon's trees c5310 onto its dummies 50 / 53-56, m25 12505901), facing
                // the dummy's +Z. Without the model's dummies (or -1): the character's root.
                let on_dummy = (ty == 2 && dmy >= 0)
                    .then(|| ent_of(run, target))
                    .flatten()
                    .and_then(|t| dummies.get(t).ok())
                    .and_then(|d| crate::combat::dmy_entity(d, dmy))
                    .and_then(|m| markers.get(m).ok())
                    .map(|g| {
                        let f = g.rotation() * Vec3::Z;
                        (g.translation(), f32::atan2(-f.x, -f.z))
                    });
                let to = match ty {
                    _ if on_dummy.is_some() => on_dummy,
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
                // The target's own height: the place's MSB height on the boss's own arena
                // (events::place; off it the place is at start.y anyway) or the character's.
                tf.translation = pos;
                // A warp onto another character's root faces Wolf (gap: not traced).
                let to_p = (player.1.translation - tf.translation).with_y(0.0);
                a.yaw = if ty == 2 && on_dummy.is_none() && to_p.length_squared() > 1e-4 { f32::atan2(-to_p.x, -to_p.z) } else { yaw };
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
            Act::Immortal(PLAYER, on) => run.wolf_immortal = on,
            Act::ScriptedLoss => {
                warn!("boss script: {} scripted loss", run.row);
                fight.0 = true;
                return;
            }
            Act::BossBar(id, on) => {
                if on {
                    run.bar = Some(id);
                } else if run.bar == Some(id) {
                    run.bar = None;
                }
            }
            Act::BossDefeat(banner) => {
                // Not while another boss of the cast is still to come (a boss row of its own,
                // never on the field yet): Emma's defeat (m11_01 11115900) is followed by Isshin
                // Ashina 1110920 (54300000), whom 11115930 enables on her defeat event's flag.
                let next_boss = run.def.chars.iter().any(|(id, c)| *id != run.def.boss && !run.ents.contains_key(id) && !run.dead_ids.contains(id) && scripts().contains_key(&c.npc));
                if !next_boss && run.defeated.is_none() {
                    run.defeated = Some(0.0);
                    run.banner = banner;
                }
            }
            Act::Shoot(owner, source, dmy, behavior) => {
                let Some(&first) = run.def.bullets.get(&behavior.to_string()) else { continue };
                // The source may be a region: Isshin Ashina's volley (m11_01 11115944) fires
                // 254000382 (a still 1.5 m fire spot, life 1.5 s) at 33 "炎弾丸射出点" spheres
                // 1112962-994 (1112960 / 961 are in no MSB: dropped). Its owner 1110921 is a c1000
                // dummy (NpcParam 10006000) no event enables; the boss stands in for it. gap: the
                // dummy's own NpcParam scaling of the shots.
                let src = if source == PLAYER { Some(wolf.0) } else { ent_of(run, source) };
                let at = if src.is_none() { run.def.regions.contains_key(&source).then(|| run.place_of(source)).flatten().map(|p| p.0) } else { None };
                let Some(owner) = ent_of(run, owner).or_else(|| at.and_then(|_| ent_of(run, run.def.boss))) else { continue };
                let Some(source) = src.or(at.map(|_| owner)) else { continue };
                shots.0.push(crate::enemy_bullet::ScriptShot { def: run.def.clone(), row: first, owner, source, dmy: dmy as i16, at });
            }
            Act::EzState(PLAYER, id) => {
                // Not while his deathblow holds one that was already finished before these
                // requests (m11_02 11125874 again on his finisher's message 10, m25 12505963 while
                // his Todome's lasts): only the first cuts it short. gap: the exe's rule for
                // taking EzState requests mid-throw is not traced.
                if throw.0.as_ref().is_some_and(|t| finished.contains(&t.target)) {
                    continue;
                }
                if player.0.play_state(&combat.player, &crate::player::event_state(&combat.player, id)) {
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
        // A death clip the script requests at 0 health bars (the Mibu Monk 50001000's a000_020000,
        // m15 11505812 on Wolf's message 10) kills at its ChrActionFlag 12 "Death" (3.13 s), its
        // immortality (11505810) or not: rec_20261016_000242's Handle Boss Defeat 11505800 (HP <= 0)
        // 3.1 s after the 20000 request. (As the death flag stands for the kill above.)
        if e.is_dead() && death_flag_at(combat.data_of(&a), &a.anim).is_some_and(|s| a.t >= s) {
            a.hp = 0.0;
            continue;
        }
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
            // (ninsatsu_unhit stays: the count drops at Wolf's Ninsatsu hit, enemy.rs ninsatsu_hit.
            // Zeroed here, the castle Genichiro's 11115820 "bars == 0" ran on the throw's first
            // frame, 0.8 s before his a211_510000 judge; boss bug hunt 2026-10-14.)
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
            // An instant row (effectEndurance 0) changes HP as it lands, each time it is set: the
            // Old Dragons' 3530020 (changeHpRate 4) on their shared bar 2500810 per dragon felled
            // (m25 12505803).
            if let Some(c) = crate::data::sp_cycle(sp).filter(|c| c.endurance == 0.0) {
                let dmg = a.hp_max * c.hp_rate / 100.0 + c.hp_point;
                if dmg != 0.0 && a.hp > 0.0 {
                    a.hp = (a.hp - dmg).clamp(0.0, a.hp_max);
                }
            }
            if !a.resident.contains(&sp) {
                a.resident.push(sp);
            }
            if let Some(g) = super::anim_group_of(d, std::iter::once(sp)) {
                a.anim_group = g;
            }
        }
        Act::ClearSp(_, sp) => {
            // Clearing the SpEffect that set the anim group gives the residents' group back
            // (as spawn / respawn derive it).
            a.resident.retain(|&x| x != sp);
            a.anim_group = super::anim_group_of(d, a.resident.iter().copied()).unwrap_or(0);
        }
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
            e.event_req_new = true;
        }
        Act::Replan(_) => e.phase_replan = true,
        Act::AiId(_, think) => {
            // Set Character AI ID: its NpcThinkParam row (the Guardian Ape's 51000100 headless).
            e.think_override = Some(think);
            brains.map.remove(&ent);
        }
        Act::AiState(_, on) => {
            e.ai_off = !on;
            // A looping forced anim with the AI on plays to its AI cancel window (TAE 0 flags 78 /
            // 79 / 86 / 23 "End If AI Move / Step / Attack / ComboAttack Queued"), else to its end,
            // then the AI goes on: the Guardian Ape's 20005 roar (m17 11705810) to 4.7 s; the Owl
            // (Father)'s a000_000401 (m10 11005910) has the window from 0 s: his first attack
            // (a000_003018) starts as his talk ends (rec_20261016_074559, enemy/talk.rs).
            if on && e.mode == Mode::Held(true) {
                let len = d.length(&a.anim);
                let cancel = d
                    .anim(&a.anim)
                    .into_iter()
                    .flat_map(|an| an.events.iter())
                    .filter(|ev| ev.kind == 0 && matches!(ev.flag_type(), Some(23 | 78 | 79 | 86)) && ev.end > a.t)
                    .map(|ev| ev.start.max(a.t))
                    .fold(len, f32::min);
                e.mode = Mode::Rising((cancel - a.t).max(0.1));
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
            // Its kneel asked again while already dead from it (m25 12505963 restarts while Wolf's
            // Todome message 10 lasts): it keeps playing; the recordings show one W_Event20200 per
            // boss end. Other requests still play on the dead: Isshin's 21000 (m11_02 11125875 on
            // his Todome ThrowDef13700's message 20 at 13 s; rec_20261016_080607 at 926.5 s), the
            // kneel his finisher 15400590 is taken from. gap: the exe's rule is not traced.
            if e.mode == Mode::Dead(f32::INFINITY) && id == 20200 {
                return;
            }
            // Nor while it already plays that event (m25 12505963 restarts every frame while Wolf
            // holds his deathblow's message 10, and the Monk is immortal there): the recordings
            // fire W_Event20200 once per boss end (rec_2026101x_*_boss_*); restarting it each
            // frame froze her kneel at 0 s, before its Todome window.
            if a.state == format!("Event{id}") {
                return;
            }
            if !takes_throw_transition(d, a, &e.clash) {
                return;
            }
            if !play_event(d, a, id) {
                return;
            }
            e.cur_ez = None;
            e.throw_death = None;
            a.move_vel = Vec3::ZERO;
            let len = d.length(&a.anim).max(0.1);
            if e.ninsatsu.0 == 0 && death_flag_at(d, &a.anim).is_some() {
                // Its death clip (the Mibu Monk's 20000: flag 12 at 3.13 s), not a getting up.
                e.down = false;
                e.mode = Mode::Dead(f32::INFINITY);
            } else if e.is_dead() && a.hp <= 0.0 {
                // Its death (20200 after the Todome): held until the script ends the fight.
                e.mode = Mode::Dead(f32::INFINITY);
            } else if e.ninsatsu.0 == 0 && id == 20200 {
                // At 0 health bars but kept alive (immortal: the Guardian Ape's first "death").
                e.down = true;
                e.mode = Mode::Held(false);
            } else if e.ninsatsu.0 == 0 {
                // Getting up from it (the Guardian Ape's 20000 after his Todome): full HP (his
                // a000_020000's TAE 401000 "enemy HP full recovery", changeHpRate -100, at
                // 0.03 s; set here at once) and one more deathblow on top of the one taken: the
                // finishers clip (2:48) shows his headless phase with two dots, one used.
                // gap: posture (taken as reset) is not traced.
                e.down = false;
                a.hp = a.hp_max;
                a.posture = 0.0;
                e.ninsatsu = (1, e.ninsatsu.1.max(1) + 1);
                e.ninsatsu_unhit = 0;
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
                e.ninsatsu_unhit = 0;
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
/// c9997.hks: in a throw reaction an NPC takes an EzState request (ExecEventTrandition, event
/// slot THROW_TRANSITION: 20200, 20000, 21000, ...) only where its ThrowDef<N>_onUpdate gets
/// there: most of them, through ThrowDefCommonFunction / ThrowDefDeathCommonFunction ->
/// ThrowCommonFunction, and 12090 / 12190 / 13290 / 13690 / 13700 directly. These never do,
/// and ThrowDef12900 / 12950 (the finishers, Isshin's a100_012900) only while a SpEffect with
/// behaviorRefId 1501000 (SP_EFFECT_REF_5010_EVENT_TRANSITION), 1000160 or 1000161
/// (SAME_THRWO_DEF / _DEATH) is on. So m11_02 11125874, which Wolf's finisher a243_510900
/// re-arms with its message 10 at 6.63 s, does not cut Isshin's 12900 before its message 30
/// (his defeat, 11125860; manual test #6).
fn takes_throw_transition(d: &crate::data::CharData, a: &Actor, clash: &[(i64, f32)]) -> bool {
    const NEVER: [i64; 12] = [12210, 12230, 13210, 13230, 14000, 14100, 14200, 14300, 17210, 17230, 18210, 18230];
    let Some(n) = a.state.strip_prefix("ThrowDef").and_then(|n| n.parse::<i64>().ok()) else { return true };
    if NEVER.contains(&n) {
        return false;
    }
    if n == 12900 || n == 12950 {
        return crate::combat::worn_sp(a, clash).iter().any(|id| d.sp_effects.get(&id.to_string()).is_some_and(|s| [1501000, 1000160, 1000161].contains(&s.behavior_ref_id)));
    }
    true
}

/// The start of a clip's ChrActionFlag 12 "Death", if it has one.
fn death_flag_at(d: &crate::data::CharData, key: &str) -> Option<f32> {
    d.anim(key)?.events.iter().filter(|ev| ev.kind == 0 && ev.flag_type() == Some(12)).map(|ev| ev.start).reduce(f32::min)
}

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

#[cfg(test)]
mod tests {
    use super::shape_contains;

    #[test]
    fn regions_have_heights_on_the_own_arena() {
        // A 4 x 4 box 2 m high and a 3 m cylinder 1 m high, both standing at y = 1.
        let at = [0.0, 1.0, 0.0];
        assert!(shape_contains(5, &[4.0, 4.0, 2.0], at, 0.0, 1.0, Some(2.5), 1.0));
        assert!(!shape_contains(5, &[4.0, 4.0, 2.0], at, 0.0, 1.0, Some(0.5), 1.0));
        assert!(!shape_contains(5, &[4.0, 4.0, 2.0], at, 0.0, 1.0, Some(3.5), 1.0));
        assert!(shape_contains(5, &[4.0, 4.0, 2.0], at, 0.0, 1.0, None, 1.0));
        assert!(!shape_contains(3, &[3.0, 1.0], at, 0.0, 0.0, Some(2.5), 0.0));
        // A sphere is round in 3D; a circle has no height.
        assert!(!shape_contains(2, &[2.0], at, 0.0, 1.5, Some(2.5), 0.0));
        assert!(shape_contains(1, &[2.0], at, 0.0, 1.5, Some(50.0), 0.0));
    }
}
