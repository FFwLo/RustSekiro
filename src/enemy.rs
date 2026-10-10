//! The Ashina Samurai General (c1020) as a sparring partner. Stats come from
//! NpcParam 10219000, attacks and their timing from c1020's TAE and
//! AtkParam_Npc, reactions from the shared NPC behaviour (c9997).
//!
//! Defence follows its real AI (ai/102000_battle.lua, Goal.Parry): when the
//! player's attack notifies the AI (TAE flag 63, a few frames before the hit)
//! the enemy picks deflect (anim 3101) or guard (3100) from its parry rank
//! (resident SpEffect 221000-2), the attack type (SpEffect 109970 = thrust)
//! and its consecutive-guard count (weak clashes 200215/6 count up, strong
//! 200210/1 reset). Offence and movement come from the real battle goal running
//! in the Lua AI runtime (ai.rs): acts, combos, spacing, and Kengeki counters
//! after clashes. Without extracted AI scripts it falls back to fixed combos.

use bevy::prelude::*;
use std::collections::{HashMap, HashSet};

use crate::actor::{Actor, ActorSet, Side};
use crate::ai::{Brain, Snapshot};
use crate::config::GameConfig;
use crate::data::Combat;
use crate::player::Player;
use crate::stealth::{self, Targeting};

mod events;
pub mod script;
pub use script::{BossScript, Cast, Phantom};

/// Fallback c1020 attack strings (Goal.Act01..07 of 102000_battle.lua: 3000-3002 combo, 3004-3005, 3006, 3010).
const COMBOS: &[&[&str]] = &[
    &["a000_003000", "a000_003001", "a000_003002"],
    &["a000_003004", "a000_003005"],
    &["a000_003006"],
    &["a000_003010"],
];

/// ChrActionFlag 23: "End If AI ComboAttack Queued" (the chain point).
const FLAG_AI_COMBO: i64 = 23;
/// ChrActionFlag 78: "End If AI Move Queued".
const FLAG_AI_MOVE: i64 = 78;
/// NPC AI cancel flags: 23 combo attack, 86 attack, 79 step, 78 move.
const AI_CANCEL_FLAGS: [i64; 4] = [23, 86, 79, 78];
/// ChrActionFlag 63 on the attacker: the AI notification that fires INTERUPT_ParryTiming.
pub const FLAG_AI_NOTIFY: i64 = 63;
/// PC_ATTACK_DIST_STAND (ai_define): standing player attack reach as the AI sees it.
const PC_ATTACK_DIST_STAND: f32 = 3.4;
/// SpEffect stateInfo 352: posture display "collapsed" (deathblow available).
const STATE_INFO_COLLAPSED: i64 = 352;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Mode {
    /// Driven by the AI (or the passive / fallback behaviour).
    Ai,
    Broken(f32),
    /// A boss's non-final deathblow: its ThrowDef reaction plays through, then it gets up
    /// (c9997 HKS ThrowDef: not env(276) IsThrowSelfDeath -> IdleTransition at env(339) anim end).
    Rising(f32),
    Dead(f32),
    /// Held in a map-script anim, no AI (looping when true): a boss's intro pose (Force
    /// Animation Playback with loop), or down at 0 health bars until the script raises it.
    Held(bool),
}

/// A forced one-shot event anim (Event200xx) that ends into its loop Event21000: the SpEffects
/// it holds to its last frame are held for good there (the Divine Dragon's collapse 20002 ->
/// 21000, both with 3520011 "collapsed" and 5026; also c1250 / c1260 / c1010 pairs).
/// gap: what starts the loop in the game is not traced (no EMEVD, HKS or AI request for the
/// dragon's 21000); by that data match.
fn loop_after(d: &crate::data::CharData, a: &Actor) -> Option<String> {
    if !a.state.starts_with("Event200") {
        return None;
    }
    let key = format!("{}021000", a.anim.get(..5)?);
    let (one, lp) = (d.anim(&a.anim)?, d.anim(&key)?);
    let len = d.length(&a.anim);
    let sp = |e: &crate::data::Event| (e.kind == 67).then(|| e.arg_i64("SpEffectID")).flatten();
    let held: HashSet<i64> = lp.events.iter().filter(|e| e.end > 1e30).filter_map(sp).collect();
    one.events.iter().filter(|e| e.end < 1e30 && e.end >= len - 0.05).filter_map(sp).any(|id| held.contains(&id)).then_some(key)
}

/// ChrActionFlag 69 "ThrowType5": on ThrowDef reactions, where a killed defender switches to
/// its ThrowDefDeath anim.
const FLAG_THROW_TYPE5: i64 = 69;

#[derive(Component)]
pub struct Enemy {
    mode: Mode,
    /// Deathblow kill pending: the death state that follows the ThrowDef reaction
    /// (ThrowDefDeath<def + 1>, or a boss's Event20200).
    throw_death: Option<String>,
    /// Deathblows left and in all (NpcParam ninsatuNum, the boss's red dots; 0 on regular
    /// enemies, which one deathblow kills). The AI reads the first (GetNinsatsuNum).
    pub ninsatsu: (u32, u32),
    /// Boss phase events (`phase_events`): the ones done, the event flags they turned on, the AI
    /// command per slot (GetEventRequest), a pending AI re-plan, and the NpcParam residents to
    /// restore on respawn (the events set / clear residents).
    phase_done: HashSet<i64>,
    phase_flags: HashSet<i64>,
    event_req: Vec<(i64, i64)>,
    phase_replan: bool,
    /// Its event message (TAE 231, EMEVD IF Character Has Event Message): the exe's handler
    /// (decomp/140b5.c, case 0xe7) stores the id in the character and nothing clears it at the
    /// event's end, so it stays until the next 231 (the Monk's attacks send 70, then
    /// 2147483647 to clear it). None before the first.
    pub msg: Option<i64>,
    /// Counts the event messages sent (each TAE 231 as it starts).
    pub msg_serial: u32,
    resident0: Vec<i64>,
    /// Passive enemies walk up but never attack (toggle with T) - for practising timing at your own pace.
    pub aggressive: bool,
    /// Game time until which a prosthetic cool-time SpEffect holds (the firecracker's 107100,
    /// effectEndurance 30 s): no new burst reaction before it.
    pub burst_until: f32,
    cooldown: f32,
    rng: u32,
    /// "ConsecutiveGuardCount" and AI timer 13 (1 s) from common_func_NTC.lua.
    guard_count: i32,
    guard_count_timer: f32,
    /// AI_TIMER_PARRY_INTERVAL (0.1 s).
    parry_timer: f32,
    /// Engine-applied clash SpEffects (200200-200216) with remaining time.
    pub clash: Vec<(i64, f32)>,
    last_notify: f32,
    /// EzState id of the AI-requested anim (attack, step) that is playing.
    pub(crate) cur_ez: Option<i64>,
    ez_started: Option<i64>,
    pub(crate) ez_failed: Option<i64>,
    /// An anim the AI did not request replaced its action since the last AI tick.
    interrupted: bool,
    /// Player attack notify seen; the Lua brain runs Goal.Parry on its next tick.
    parry_timing: bool,
    prev_sp: HashSet<i64>,
    /// Active goal chain, for the debug panel.
    pub ai_desc: String,
    /// Fallback combo when no AI scripts are available: (combo, step).
    fallback: Option<(usize, usize)>,
    /// What it knows of Wolf (stealth.rs): NONE / CAUTION / FIND / BATTLE and the target slots.
    pub targeting: Targeting,
    /// AI-state SpEffect: 200000 default, 200001 caution (non-battle), 200002 caution (battle),
    /// 200004 found / battle. The enemy's alert anims set it in their TAE (e.g. c1020 1000 /
    /// 1010: TAE 66, 1040 / 401020: TAE 401) and it lasts (effectEndurance -1) until replaced;
    /// c9997's HKS reads it (UpdateAIState) to pick the Idle / Walk / Turn variants.
    ai_sp: i64,
    /// Home (POINT_INITIAL) and its facing yaw (POINT_INIT_POSE), taken where it stands when it
    /// starts (re)thinking.
    home: Option<(Vec3, f32)>,
    /// Where it was placed; it comes back 2 m behind it after dying (the duel's reset).
    start: Vec3,
    /// Phase-rule requests for other characters (events.rs): "enable" payloads, "disable"
    /// (itself), Wolf's "warp_player".
    spawn_req: Vec<serde_json::Value>,
    disable_req: bool,
    warp_req: Option<serde_json::Value>,
    /// Map script (script.rs): its AI ID (Set Character AI ID: NpcThinkParam row), AI off (Set
    /// Character AI State), immortality (HP stays at 1 or more), and down at 0 health bars.
    think_override: Option<i64>,
    ai_off: bool,
    immortal: bool,
    down: bool,
    /// Disabled by its map script (Change Character Enable State off): out of the fight.
    pub(crate) disabled: bool,
}

/// SpEffect stateInfo 270-274: "Anime ID offset [0]-[4]" (SpEffect 200030-200034, effectEndurance -1).
const STATE_INFO_ANIM_OFFSET: std::ops::RangeInclusive<i64> = 270..=274;

/// The anim set (0-4 = a000-a400) the last of these SpEffects picks, if any is an offset one.
fn anim_group_of(d: &crate::data::CharData, ids: impl Iterator<Item = i64>) -> Option<u32> {
    ids.filter_map(|id| d.sp_effects.get(&id.to_string()))
        .map(|s| s.state_info)
        .filter(|si| STATE_INFO_ANIM_OFFSET.contains(si))
        .last()
        .map(|si| (si - 270) as u32)
}

/// A boss phase rule from its map's event script (tools/boss_events.py ->
/// extracted/enemies/boss_events.json): `IF Number of Character Health Bars` <cmp> `bars` on the
/// boss, plus its `IF Character Has SpEffect` / `IF Event Flag` conditions, then the actions on
/// the boss: ["sp", id] / ["clear", id] (SetSpEffect / ClearSpEffect), ["anim", id]
/// (ForceAnimationPlayback), ["ezstate", id] (EzState Instruction Request), ["ai", command,
/// slot] (Request Character AI Command) and ["replan"].
#[derive(serde::Deserialize, Clone, Debug)]
pub struct PhaseEvent {
    pub bars: u32,
    /// EMEDF comparison: 0 ==, 1 !=, 2 >, 3 <, 4 >=, 5 <=.
    pub cmp: u8,
    pub event: i64,
    #[serde(default)]
    pub needs_sp: Vec<(i64, bool)>,
    #[serde(default)]
    pub needs_flags: Vec<i64>,
    #[serde(default)]
    pub needs_flags_off: Vec<i64>,
    #[serde(default)]
    pub sets_flags: Vec<i64>,
    /// IF Character Has Event Message on the player: (entity, message id), e.g. (10000, 10),
    /// which Wolf's Todome sends (TAE 936).
    #[serde(default)]
    pub message: Option<(i64, i64)>,
    /// EzState requests to the player in the same event (Wolf's Event7102xx).
    #[serde(default)]
    pub player: Vec<i64>,
    pub actions: Vec<serde_json::Value>,
    /// Conditions of other kinds in the same wait (player event messages, regions, ...).
    pub other: u32,
}

/// The other enemy kinds (chr, NpcParam row) a boss's map events bring into its fight ("enable"
/// actions, and theirs in turn: Genichiro -> Way of Tomoe; Tomoe -> Isshin; the Monk's phantoms).
pub fn event_kinds(_combat: &Combat, npc_row: i64) -> Vec<(String, i64)> {
    let mut out: Vec<(String, i64)> = Vec::new();
    let mut todo = vec![npc_row];
    let mut seen = HashSet::new();
    while let Some(row) = todo.pop() {
        if !seen.insert(row) {
            continue;
        }
        for r in phase_events(row) {
            for act in r.actions.iter().filter(|a| a.get(0).and_then(|v| v.as_str()) == Some("enable")) {
                let (Some(chr), Some(npc)) = (act[1]["chr"].as_str(), act[1]["npc"].as_i64()) else { continue };
                if !out.iter().any(|(c, n)| c == chr && *n == npc) {
                    out.push((chr.to_string(), npc));
                    todo.push(npc);
                }
            }
        }
        // Its boss script's cast (the Divine Dragon's tentacles c5310).
        if let Some(def) = script::scripts().get(&row) {
            let mut cast: Vec<(&i64, &script::CharDef)> = def.chars.iter().filter(|(id, _)| **id != def.boss).collect();
            cast.sort_by_key(|c| c.0);
            for (_, c) in cast {
                if !out.iter().any(|(chr, n)| *chr == c.chr && *n == c.npc) {
                    out.push((c.chr.clone(), c.npc));
                }
            }
        }
    }
    out
}

/// The phase rules of one NpcParam row (empty for most).
pub fn phase_events(npc_row: i64) -> &'static [PhaseEvent] {
    static ALL: std::sync::OnceLock<HashMap<i64, Vec<PhaseEvent>>> = std::sync::OnceLock::new();
    ALL.get_or_init(|| {
        let path = crate::paths::root().join("extracted/enemies/boss_events.json");
        let Ok(text) = std::fs::read_to_string(&path) else { return HashMap::new() };
        match serde_json::from_str::<HashMap<String, Vec<PhaseEvent>>>(&text) {
            Ok(m) => m.into_iter().filter_map(|(k, v)| Some((k.parse().ok()?, v))).collect(),
            Err(e) => {
                warn!("{}: {e}", path.display());
                HashMap::new()
            }
        }
    })
    .get(&npc_row)
    .map_or(&[], |v| v.as_slice())
}

/// The AI-state SpEffects (c9997 SP_EFFECT_REF_AI_* refs 1000000-1000003).
const AI_STATE_SP: [i64; 4] = [200000, 200001, 200002, 200004];

impl Enemy {
    pub fn is_broken(&self) -> bool {
        matches!(self.mode, Mode::Broken(_))
    }

    pub fn is_dead(&self) -> bool {
        matches!(self.mode, Mode::Dead(_))
    }

    /// Kept alive at 0 health bars by its map script, lying in its death (Isshin's 20200 after
    /// his Todome: m11_02 11125873 keeps him immortal until a message 20 that the cut-short
    /// Todome never sends).
    pub fn is_down(&self) -> bool {
        self.down && matches!(self.mode, Mode::Held(_))
    }

    /// Disabled by its map script (hidden, out of the fight).
    pub fn is_disabled(&self) -> bool {
        self.disabled
    }

    /// Has not found Wolf (target state NONE or CAUTION): open to the stealth deathblows
    /// (ThrowParam 0020 behind, 0030 plunge).
    pub fn is_unaware(&self) -> bool {
        matches!(self.mode, Mode::Ai) && self.targeting.state < stealth::FIND
    }

    /// Back to never having noticed Wolf: no targets, the default AI state (200000) and its
    /// idle a000_000000; home is where it stands now.
    pub fn make_unaware(&mut self, a: &mut Actor) {
        self.mode = Mode::Ai;
        self.targeting.reset();
        self.ai_sp = AI_STATE_SP[0];
        self.home = None;
        self.cur_ez = None;
        self.fallback = None;
        self.interrupt();
        a.play("IdleDefault", "a000_000000");
        a.move_vel = Vec3::ZERO;
    }

    /// Killed by a deathblow: the ThrowParam defender anim ThrowDef<def> plays out the pair with
    /// Wolf (its root motion: the General slides 2 m back), then, with HP at 0, its ChrActionFlag
    /// 69 (ThrowType5) switches to ThrowDefDeath<def + 1> (c9997 HKS: env(276) ->
    /// W_ThrowDefDeath). Live (rec_20261008_054259): 12000 for 111 frames (flag at TAE 55), then
    /// 12001. Without a ThrowDef anim: ThrowDefDeath directly, else a procedural death.
    pub fn deathblow(&mut self, a: &mut Actor, combat: &Combat, def_anim: i64) {
        let d = combat.data_of(a);
        self.cur_ez = None;
        a.move_vel = Vec3::ZERO;
        // The finisher on a kneeling boss (player.rs deathblow_check): its reaction, and it stays
        // down for good (its map script ends the fight on the reaction's message 30, m11_02
        // 11125860).
        if self.is_dead() || self.is_down() {
            if !a.play_state(d, &format!("ThrowDef{def_anim}")) {
                a.procedural("Dead");
            }
            self.throw_death = None;
            self.mode = Mode::Dead(f32::INFINITY);
            return;
        }
        // A boss with deathblows left: the ThrowDef reaction alone (no ThrowDefDeath, which the
        // boss characters c5000 / c7100 do not even have), then it gets up for the next phase.
        if self.ninsatsu.0 > 1 {
            self.ninsatsu.0 -= 1;
            // gap: the exe's refill on a non-final deathblow is not traced; the game shows the
            // boss back at full vitality and an empty posture bar.
            a.hp = a.hp_max;
            a.posture = 0.0;
            if !a.play_state(d, &format!("ThrowDef{def_anim}")) {
                a.procedural("PostureBroken");
            }
            self.throw_death = None;
            self.mode = Mode::Rising(d.length(&a.anim).max(1.0));
            return;
        }
        // (An immortal one's map script keeps it alive after its reaction's first messages:
        // script.rs.)
        self.ninsatsu.0 = 0;
        a.hp = 0.0;
        let (react, death) = (format!("ThrowDef{def_anim}"), format!("ThrowDefDeath{}", def_anim + 1));
        let has = |s: &str| d.anim_key(s).is_some();
        self.throw_death = None;
        if has(&react) && has(&death) && a.play_state(d, &react) {
            self.throw_death = Some(death);
        } else if !has(&death) && has("Event20200") && a.play_state(d, &react) {
            // A major boss has no ThrowDefDeath: its last deathblow (the Todome, ThrowDef13700) plays
            // out, then the map event at 0 health bars requests EzState 20200, its death (e.g.
            // m11_02 11125874 Isshin: IF Number of Character Health Bars == 0 -> EzState 20200).
            self.throw_death = Some("Event20200".into());
        } else if !([death, react].iter().any(|s| has(s) && a.play_state(d, s))) {
            a.procedural("Dead");
        }
        let len = d.length(&a.anim);
        self.mode = Mode::Dead(len + 2.0);
    }

    /// The map-event AI commands by slot (tests).
    #[cfg(test)]
    pub fn event_req(&self) -> &[(i64, i64)] {
        &self.event_req
    }

    /// The boss's phase rules for its health bars left (the deathblows left): set / clear
    /// SpEffects, AI commands, a re-plan and a forced anim (which it then plays out like a
    /// rising). Each rule runs once. Returns whether an anim was forced.
    /// Only rules with bars > 0: the last deathblow's death (EzState 20200) is `deathblow`'s.
    /// gap: rules with other conditions (Wolf's event message, regions) are skipped, and an event
    /// flag no phase rule turns on (set by the arena's own events) counts as on.
    fn run_phase_events(&mut self, a: &mut Actor, combat: &Combat, sp_now: &HashSet<i64>) -> bool {
        let rules = phase_events(combat.foe_of(a).npc_row);
        let d = combat.data_of(a);
        let bars = self.ninsatsu.0;
        let mut forced = false;
        for r in rules {
            let cmp = match r.cmp {
                0 => bars == r.bars,
                1 => bars != r.bars,
                2 => bars > r.bars,
                3 => bars < r.bars,
                4 => bars >= r.bars,
                _ => bars <= r.bars,
            };
            let flag_on = |f: &i64| self.phase_flags.contains(f) || !rules.iter().any(|o| o.sets_flags.contains(f));
            if r.bars == 0
                || r.other > 0
                || self.phase_done.contains(&r.event)
                || !cmp
                || !r.needs_sp.iter().all(|(id, has)| sp_now.contains(id) == *has)
                || !r.needs_flags.iter().all(flag_on)
                || r.needs_flags_off.iter().any(flag_on)
            {
                continue;
            }
            self.phase_done.insert(r.event);
            self.phase_flags.extend(r.sets_flags.iter().copied());
            for act in &r.actions {
                let n = |i: usize| act.get(i).and_then(|v| v.as_i64()).unwrap_or(0);
                match act.get(0).and_then(|v| v.as_str()).unwrap_or("") {
                    "sp" => {
                        if !a.resident.contains(&n(1)) {
                            a.resident.push(n(1));
                        }
                        if let Some(g) = anim_group_of(d, std::iter::once(n(1))) {
                            a.anim_group = g;
                        }
                    }
                    "clear" => a.resident.retain(|&id| id != n(1)),
                    "ai" => {
                        self.event_req.retain(|&(slot, _)| slot != n(2));
                        self.event_req.push((n(2), n(1)));
                    }
                    "replan" => self.phase_replan = true,
                    "enable" => self.spawn_req.push(act[1].clone()),
                    "disable" => self.disable_req = true,
                    "warp_player" => self.warp_req = Some(act[1].clone()),
                    // ForceAnimationPlayback 20010 = a000_020010 (state Event20010); the EzState
                    // request of the same id right after it asks for the same state.
                    "anim" | "ezstate" if !forced => {
                        let state = format!("Event{}", n(1));
                        let key = format!("a000_{:06}", n(1));
                        if a.play_state(d, &state) {
                            forced = true;
                        } else if d.anim(&a.grouped(&key)).is_some() {
                            a.play(&state, &key);
                            forced = true;
                        }
                    }
                    _ => {}
                }
            }
        }
        if forced {
            self.cur_ez = None;
            a.move_vel = Vec3::ZERO;
            self.mode = Mode::Rising(d.length(&a.anim).max(0.5));
        }
        forced
    }

    /// Its last deathblow ended without Wolf's Todome message: a rule at 0 bars that forces an
    /// anim once the ThrowDef's SpEffect is gone and the message rule's flag is still off (the
    /// Corrupted Monk's m25 12505964: 3500010 off, 12505954 off -> ForceAnimationPlayback 20021;
    /// 12505963 sets 12505954 on the message). Returns its state and anim.
    fn fallback_death(&mut self, a: &Actor, combat: &Combat, sp_now: &HashSet<i64>) -> Option<(String, String)> {
        let rules = phase_events(combat.foe_of(a).npc_row);
        let d = combat.data_of(a);
        for r in rules {
            let flag_on = |f: &i64| self.phase_flags.contains(f) || !rules.iter().any(|o| o.sets_flags.contains(f));
            if r.bars != 0
                || r.cmp != 0
                || r.other > 0
                || r.message.is_some()
                || self.phase_done.contains(&r.event)
                || !r.needs_sp.iter().all(|(id, has)| sp_now.contains(id) == *has)
                || !r.needs_flags.iter().all(flag_on)
                || r.needs_flags_off.iter().any(flag_on)
            {
                continue;
            }
            let Some(id) = r.actions.iter().find(|act| act.get(0).and_then(|v| v.as_str()) == Some("anim")).and_then(|act| act.get(1)?.as_i64()) else {
                continue;
            };
            // ForceAnimationPlayback 20021 = a000_020021 (state Event20021 when the HKS has one).
            let state = format!("Event{id}");
            let Some(key) = d.anim_key(&state).map(str::to_string).or_else(|| Some(a.grouped(&format!("a000_{id:06}"))).filter(|k| d.anim(k).is_some())) else {
                continue;
            };
            self.phase_done.insert(r.event);
            self.phase_flags.extend(r.sets_flags.iter().copied());
            return Some((state, key));
        }
        None
    }

    /// After its death at 0 health bars: the boss's rules at 0 bars that bring in other
    /// characters ("enable" / "disable" / "warp_player", e.g. Genichiro's 11115820 -> the Way of
    /// Tomoe). Queues their requests; returns whether there were any (no duel reset then).
    fn final_events(&mut self, a: &Actor, combat: &Combat) -> bool {
        let rules = phase_events(combat.foe_of(a).npc_row);
        let mut any = false;
        for r in rules {
            let others = r.actions.iter().any(|act| matches!(act.get(0).and_then(|v| v.as_str()), Some("enable" | "disable" | "warp_player")));
            let flag_on = |f: &i64| self.phase_flags.contains(f) || !rules.iter().any(|o| o.sets_flags.contains(f));
            if !others || r.bars != 0 || r.cmp != 0 || r.other > 0 || self.phase_done.contains(&r.event) || !r.needs_flags.iter().all(flag_on) || r.needs_flags_off.iter().any(flag_on) {
                continue;
            }
            self.phase_done.insert(r.event);
            self.phase_flags.extend(r.sets_flags.iter().copied());
            for act in &r.actions {
                match act.get(0).and_then(|v| v.as_str()).unwrap_or("") {
                    "enable" => self.spawn_req.push(act[1].clone()),
                    "disable" => self.disable_req = true,
                    "warp_player" => self.warp_req = Some(act[1].clone()),
                    _ => {}
                }
            }
            any = true;
        }
        any
    }

    /// Posture broken: a deflect/guard break keeps its AttackBoundEmptyStamina /
    /// GuardBreak anim, a hit break plays TrunkCollapseFront (Back from behind) as in
    /// c9997 HKS ExecDamageBreak. The deathblow window lasts until the break anim
    /// shows the collapsed posture (stateInfo 352: frames 0-75 of 90); `fallback` if none.
    pub fn on_posture_break(&mut self, a: &mut Actor, combat: &Combat, from_behind: bool, fallback: f32) {
        if matches!(self.mode, Mode::Dead(_) | Mode::Rising(_) | Mode::Held(_)) {
            return;
        }
        let d = combat.data_of(a);
        // A break throw pose (ThrowDef12100 / 13100, player.rs) is the break reaction itself.
        if !a.state.starts_with("AttackBoundEmptyStamina") && !a.state.starts_with("GuardBreak") && !a.state.starts_with("ThrowDef") {
            let s = if from_behind && d.anim_key("TrunkCollapseBack").is_some() { "TrunkCollapseBack" } else { "TrunkCollapseFront" };
            if !a.play_state(d, s) {
                a.procedural("PostureBroken");
            }
        }
        // Window = SpEffect stateInfo 352 ("trunk display: collapsed", 220420, frames 0-75 of
        // every break anim: the deathblow indicator); else the anim's AI attack flag 86.
        let anim = d.anim(&a.anim);
        let collapse_end = anim.and_then(|an| {
            an.events
                .iter()
                .filter(|e| matches!(e.kind, 67 | 401))
                .filter(|e| d.sp_effects.get(&e.arg_i64("SpEffectID").unwrap_or(0).to_string()).is_some_and(|s| s.state_info == STATE_INFO_COLLAPSED))
                .map(|e| e.end)
                .reduce(f32::max)
        });
        let ai_flag = anim.and_then(|an| an.events.iter().filter(|e| e.kind == 0 && e.flag_type() == Some(86)).map(|e| e.start).reduce(f32::min));
        let window = if a.state.starts_with("ThrowDef") {
            // The break pose pair: open while the pose plays.
            (d.length(&a.anim) - a.t).max(0.5)
        } else {
            collapse_end.or(ai_flag).map(|t| (t - a.t).max(0.5)).unwrap_or(fallback)
        };
        self.mode = Mode::Broken(window);
        a.move_vel = Vec3::ZERO;
        self.cur_ez = None;
    }

    /// Plays a reaction state; the AI replans (Kengeki) and acts again at the
    /// reaction's AI cancel flags.
    pub fn react(&mut self, a: &mut Actor, combat: &Combat, state: &str) {
        if matches!(self.mode, Mode::Dead(_) | Mode::Broken(_) | Mode::Rising(_) | Mode::Held(_)) {
            return;
        }
        // Hurt (or made to block) by Wolf: he is a battle target now (target reason 4).
        self.targeting.damaged(combat.param("NpcThinkParam", combat.foe_of(a).think_id));
        if a.play_state(combat.data_of(a), state) {
            self.interrupt();
            a.move_vel = Vec3::ZERO;
        }
    }

    fn interrupt(&mut self) {
        self.interrupted = true;
        self.cur_ez = None;
        self.fallback = None;
    }

    /// A SpEffect put on it for `secs` (a bullet's spEffectId0-4, e.g. the reversal bolt's
    /// 3520085); kept with the clash ones, which the AI and the map script read.
    pub fn add_timed(&mut self, id: i64, secs: f32) {
        self.clash.retain(|c| c.0 != id);
        self.clash.push((id, secs));
    }

    /// Engine clash result SpEffect (lasts 0.1 s); also drives the guard count.
    pub fn add_clash(&mut self, id: i64) {
        self.clash.push((id, 0.1));
        match id {
            200215 | 200216 => {
                self.guard_count = if self.guard_count_timer <= 0.0 { 1 } else { self.guard_count + 1 };
                self.guard_count_timer = 1.0;
            }
            200210 | 200211 => {
                self.guard_count = 0;
                self.guard_count_timer = 0.0;
            }
            _ => {}
        }
    }

    /// Test hook: start a specific combo immediately.
    #[cfg(test)]
    pub fn force_combo(&mut self, a: &mut Actor, combo: usize) {
        a.play(&format!("Combo{combo}.0"), COMBOS[combo][0]);
        self.cur_ez = ez_of(COMBOS[combo][0]);
        self.fallback = Some((combo, 0));
    }

    /// Test hook: play one attack anim as if the AI had requested it.
    #[cfg(test)]
    pub fn force_anim(&mut self, a: &mut Actor, key: &str) {
        let ez = ez_of(key);
        a.play(&format!("Ez{}", ez.unwrap_or(0)), key);
        self.cur_ez = ez;
        self.fallback = None;
    }

    /// Playing one of its own attacks (EzState 3000-3099), not a guard/deflect.
    pub fn is_attacking(&self) -> bool {
        matches!(self.cur_ez, Some(ez) if (3000..3100).contains(&ez))
    }

    pub fn guard_count(&self) -> i32 {
        if self.guard_count_timer > 0.0 { self.guard_count } else { 0 }
    }

    fn rand100(&mut self) -> i32 {
        (self.rand() * 100.0) as i32 + 1
    }

    fn rand(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        (self.rng % 10_000) as f32 / 10_000.0
    }
}

/// "a000_003000" -> 3000
fn ez_of(anim: &str) -> Option<i64> {
    anim.get(5..).and_then(|s| s.parse().ok())
}

/// Lua brains per enemy (the Lua VM is not Send, so this is a non-send resource).
#[derive(Default)]
pub(crate) struct Brains {
    pub(crate) map: HashMap<Entity, Brain>,
    pub(crate) failed: bool,
}

/// Debug-menu control of the enemy's behaviour (debug_menu.rs).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum AiMode {
    /// The game's own Lua battle AI.
    #[default]
    Real,
    /// Walks into range and waits (deflect practice).
    Passive,
    /// Only perilous (red kanji) attacks: sweeps, thrusts, grabs, picked at random.
    Perilous,
    /// Only perilous thrusts (warning judge 982): Mikiri Counter practice.
    Thrust,
    /// One chosen attack, again and again.
    Repeat,
    /// Stands still.
    Idle,
}

#[derive(Resource)]
pub struct EnemyDebug {
    pub mode: AiMode,
    /// Attack anim for Repeat (a000_003xxx).
    pub attack: String,
    /// Seconds between drill attacks.
    pub interval: f32,
    timer: f32,
}

impl Default for EnemyDebug {
    fn default() -> Self {
        EnemyDebug { mode: AiMode::Real, attack: String::new(), interval: 1.5, timer: 0.0 }
    }
}

/// The enemy's own attacks (EzState 3000-3099 anims with attack windows), sorted, with their
/// perilous warning (TAE 2 BehaviorJudgeID 980 sweep / 982 thrust / 983 grab) if any.
pub fn attack_list(d: &crate::data::CharData) -> Vec<(String, Option<i64>)> {
    let mut v: Vec<(String, Option<i64>)> = d
        .anims
        .keys()
        .filter(|k| k.starts_with("a000_0030") && ez_of(k).is_some_and(|ez| (3000..3100).contains(&ez)))
        .filter(|k| !d.attack_windows(k).is_empty())
        .map(|k| (k.clone(), d.perilous_warning(k, -1.0, 1e9)))
        .collect();
    v.sort();
    v
}

pub struct EnemyPlugin;

impl Plugin for EnemyPlugin {
    fn build(&self, app: &mut App) {
        app.insert_non_send(Brains::default())
            .init_resource::<EnemyDebug>()
            .init_resource::<events::EventWarp>()
            .init_resource::<FightReset>()
            .init_resource::<BossScript>()
            .add_systems(Startup, spawn_enemy)
            .add_systems(
                FixedUpdate,
                (reset_fight, stealth_env_start, parry_interrupt, think, todome_message, events::messages, script::run_scripts, events::apply_event_warp).chain().in_set(ActorSet::Decide),
            )
            .add_systems(Update, toggle_aggression.run_if(resource_exists::<ButtonInput<KeyCode>>));
    }
}

/// TAE 936 (Wolf's event message, unk0 = message id): the Todome sends 10 (a242_511700 at
/// 2.4 s, a240_511700 at 1.7 s; 11-13 with it).
const TAE_EVENT_MESSAGE: i32 = 936;

/// A boss's last deathblow (the Todome) ends by its map event at 0 health bars: on Wolf's event
/// message (IF Character Has Event Message 10000 / 10, his Todome's TAE 936) the boss gets
/// EzState 20200, its death, and Wolf his Event7102xx (c0000.hks: GetCommandIDFromEvent ->
/// RequestThrowAnimInterrupt, W_Event7102xx; e.g. m11_02 11125874 Isshin -> 710206, m11_00
/// 11105921 Demon of Hatred -> 710207). Only 710205-710207 have a state in c0000.hkx; the others
/// (the Corrupted Monk's 710200) fire into nothing, and Wolf plays his Todome out.
/// Without the message the boss switches to 20200 on its ThrowDef's flag 69 or end (`think`).
fn todome_message(
    combat: Res<Combat>,
    mut player: Single<&mut Actor, (With<Player>, Without<Enemy>)>,
    mut q: Query<(&mut Enemy, &mut Actor), (Without<Player>, Without<script::Scripted>)>,
    mut throw: ResMut<crate::player::ActiveThrow>,
) {
    let pa = &mut **player;
    let Some(anim) = combat.player.anim(&pa.anim) else { return };
    let sent: Vec<i64> = anim
        .events
        .iter()
        .filter(|ev| ev.kind == TAE_EVENT_MESSAGE && ev.start > pa.prev_t && ev.start <= pa.t)
        .filter_map(|ev| ev.arg_i64("unk0"))
        .collect();
    if sent.is_empty() {
        return;
    }
    for (mut e, mut a) in &mut q {
        if e.throw_death.as_deref() != Some("Event20200") || script::scripts().contains_key(&combat.foe_of(&a).npc_row) {
            continue;
        }
        let rules = phase_events(combat.foe_of(&a).npc_row);
        let Some(r) = rules.iter().find(|r| {
            r.bars == 0
                && r.message.is_some_and(|(_, m)| sent.contains(&m))
                && r.actions.iter().any(|act| act.get(0).and_then(|v| v.as_str()) == Some("ezstate") && act.get(1).and_then(|v| v.as_i64()) == Some(20200))
                && r.needs_sp.iter().all(|(id, has)| a.resident.contains(id) == *has)
        }) else {
            continue;
        };
        e.throw_death = None;
        let d = combat.data_of(&a);
        if a.play_state(d, "Event20200") {
            e.mode = Mode::Dead(d.length(&a.anim) + 2.0);
        }
        for id in &r.player {
            if pa.play_state(&combat.player, &format!("Event{id}")) {
                throw.0 = None;
                break;
            }
        }
    }
}

/// SHINOBI_STEALTH=far | behind | close: start in a stealth setup instead of the duel (for checks):
/// far = 22 m ahead facing Wolf (the debug menu's StealthFar), behind = 5 m ahead, back turned.
/// close: like behind, 1.5 m away (in deathblow reach).
fn stealth_env_start(
    mut done: Local<bool>,
    player: Single<(&Actor, &Transform), (With<Player>, Without<Enemy>)>,
    mut q: Query<(&mut Enemy, &mut Actor, &mut Transform), Without<Player>>,
) {
    if *done {
        return;
    }
    *done = true;
    let Ok(mode) = std::env::var("SHINOBI_STEALTH") else { return };
    let (pa, ptf) = *player;
    for (mut e, mut a, mut tf) in &mut q {
        let far = mode == "far";
        let dist = match mode.as_str() {
            "far" => 22.0,
            "close" => 1.5,
            _ => 5.0,
        };
        tf.translation = ptf.translation + pa.forward() * dist;
        a.yaw = if far { pa.yaw + std::f32::consts::PI } else { pa.yaw };
        e.make_unaware(&mut a);
        e.aggressive = true;
    }
}

fn npc_stats(combat: &Combat, kind: usize) -> (f32, f32, f32, i64) {
    let n = combat.param("NpcParam", combat.kind(kind).foe.npc_row);
    let f = |k: &str| n[k].as_f64().unwrap_or(0.0) as f32;
    (f("hp"), f("stamina"), f("staminaRecoverBaseVel"), n["staminaControlParamId"].as_i64().unwrap_or(0))
}

fn spawn_enemy(mut commands: Commands, combat: Res<Combat>, config: Res<GameConfig>) {
    spawn_lineup(&mut commands, &combat, &config, false);
}

/// The duel's enemies: config enemy.chr 4 m ahead of Wolf, then each config enemy.group entry
/// at its `at` (x, z), else in a row beside the first (3 m apart).
fn spawn_lineup(commands: &mut Commands, combat: &Combat, config: &GameConfig, aggressive: bool) {
    let y = crate::player::CAPSULE_HALF_HEIGHT + 0.05;
    spawn_one(commands, combat, config, 0, Vec3::new(0.0, y, -4.0), 0.0, aggressive, 0x9E37_79B9);
    for (i, g) in config.enemy.group.iter().enumerate() {
        let Some(kind) = combat.kind_index(&g.chr, g.npc_row) else { continue };
        let side = if i % 2 == 0 { 1.0 } else { -1.0 } * (3.0 * (i / 2 + 1) as f32);
        let [x, z] = g.at.unwrap_or([side, -4.0]);
        // A different seed each, so the group does not move in lockstep.
        spawn_one(commands, combat, config, kind, Vec3::new(x, y, z), 0.0, aggressive, 0x9E37_79B9 ^ 0x85EB_CA6B_u32.wrapping_mul(i as u32 + 1));
    }
}

/// Starts the whole fight over (Wolf's reset or death, or the death of a character a boss event
/// brought in): every enemy goes and the starting line-up comes back, the way the game reloads
/// the boss fight. Keeps the AI on/off setting.
#[derive(Resource, Default)]
pub struct FightReset(pub bool);

fn reset_fight(
    mut commands: Commands,
    mut reset: ResMut<FightReset>,
    combat: Res<Combat>,
    config: Res<GameConfig>,
    q: Query<(Entity, &Enemy)>,
    mut brains: NonSendMut<Brains>,
    mut warp: ResMut<events::EventWarp>,
    mut bs: ResMut<BossScript>,
) {
    if !std::mem::take(&mut reset.0) {
        return;
    }
    if bs.run.take().is_some_and(|r| r.defeated.is_some()) {
        bs.defeats += 1;
    }
    let aggressive = q.iter().next().is_some_and(|(_, e)| e.aggressive);
    for (ent, _) in &q {
        commands.entity(ent).despawn();
    }
    brains.map.clear();
    warp.0 = None;
    spawn_lineup(&mut commands, &combat, &config, aggressive);
}

/// Spawns one enemy of kind `kind` at `at`, facing `yaw`; `aggressive`: the real AI (else the
/// passive partner once fighting).
#[allow(clippy::too_many_arguments)]
pub fn spawn_one(commands: &mut Commands, combat: &Combat, config: &GameConfig, kind: usize, at: Vec3, yaw: f32, aggressive: bool, rng: u32) -> Entity {
    let (hp, posture, regen, ctrl) = npc_stats(combat, kind);
    let mut actor = Actor::new(Side::Enemy, hp, posture, regen, ctrl);
    actor.kind = kind;
    actor.yaw = yaw;
    let k = combat.kind(kind);
    let npc = combat.param("NpcParam", k.foe.npc_row);
    actor.resident = (0..32).filter_map(|i| npc[format!("spEffectID{i}")].as_i64()).filter(|&v| v > 0).collect();
    actor.posture_debt = -(npc["maxDebtStamina"].as_f64().unwrap_or(0.0) as f32).min(0.0);
    actor.guard_angle = npc["guardAngle"].as_f64().unwrap_or(0.0) as f32;
    // Its anim set: the resident "Anime ID offset" SpEffect (c1021 spear General 200031 = a100).
    // (Only anims with a clip: the a000 placeholders fall through to their set, Actor::grouped.)
    actor.anim_keys = Some(std::sync::Arc::new(k.data.anims.iter().filter(|(_, a)| a.duration.is_some()).map(|(k, _)| k.clone()).collect()));
    actor.anim_group = anim_group_of(&k.data, actor.resident.iter().copied()).unwrap_or(0);
    // Its idle's opacity from the start (the Monk's phantoms: a200_000000 holds 0).
    if let Some(o) = k.data.opacity_key(&format!("a{:03}_000000", actor.anim_group * 100), 0.0) {
        actor.opacity = o;
    }
    // Area scaling: HP / posture now, posture regen through the resident product,
    // damage and posture damage in combat.
    if let Some(dope) = k.data.sp_effects.get(&config.enemy.area_doping.to_string()) {
        actor.hp_max *= dope.max_hp_rate;
        actor.hp = actor.hp_max;
        actor.posture_max *= dope.max_stamina_rate;
        actor.atk_rate = dope.physics_attack_power_rate;
        actor.stam_atk_rate = dope.stamina_attack_rate;
        actor.resident.push(config.enemy.area_doping);
    }
    commands
        .spawn((
        Enemy {
            mode: Mode::Ai,
            aggressive,
            burst_until: 0.0,
            cooldown: 1.5,
            rng,
            guard_count: 0,
            guard_count_timer: 0.0,
            parry_timer: 0.0,
            clash: Vec::new(),
            last_notify: -1.0,
            cur_ez: None,
            throw_death: None,
            ninsatsu: {
                let n = npc["ninsatuNum"].as_i64().unwrap_or(0).max(1) as u32;
                (n, n)
            },
            phase_done: HashSet::new(),
            phase_flags: HashSet::new(),
            event_req: Vec::new(),
            phase_replan: false,
            msg: None,
            msg_serial: 0,
            resident0: actor.resident.clone(),
            ez_started: None,
            ez_failed: None,
            interrupted: false,
            parry_timing: false,
            prev_sp: HashSet::new(),
            ai_desc: String::new(),
            fallback: None,
            // The duel: it starts already fighting Wolf (a battle target, AI state 200004).
            // The debug menu's stealth setup (make_unaware) starts it from nothing instead.
            targeting: {
                let mut t = Targeting::new(0x2545_F491 ^ rng);
                t.damaged(combat.param("NpcThinkParam", k.foe.think_id));
                t
            },
            ai_sp: 200004,
            home: None,
            start: at,
            spawn_req: Vec::new(),
            disable_req: false,
            warp_req: None,
            think_override: None,
            ai_off: false,
            immortal: false,
            down: false,
            disabled: false,
        },
        actor,
        Name::new(format!("Enemy {} {}", k.foe.chr, k.foe.npc_row)),
        Transform::from_translation(at),
        Visibility::default(),
    ))
        .id()
}

/// Capsule + blade for the enemy (windowed app only).
pub fn enemy_visuals(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    q: Query<Entity, (With<Enemy>, Without<Mesh3d>)>,
) {
    for e in &q {
        commands
            .entity(e)
            .insert((Mesh3d(meshes.add(Capsule3d::new(0.45, 1.1))), MeshMaterial3d(materials.add(Color::srgb(0.45, 0.2, 0.18)))))
            .with_children(|p| crate::world::spawn_blade(p, &mut meshes, &mut materials, Color::srgb(0.9, 0.75, 0.6)));
    }
}

fn turn_toward(yaw: f32, target: f32, max_step: f32) -> f32 {
    let diff = (target - yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
    yaw + diff.clamp(-max_step, max_step)
}

fn in_front(from: &Actor, from_pos: Vec3, to_pos: Vec3, half_angle_deg: f32) -> bool {
    let dir = (to_pos - from_pos).with_y(0.0).normalize_or_zero();
    from.forward().angle_between(dir).to_degrees() <= half_angle_deg
}

/// Goal.Parry (102000_battle.lua), run when the player's attack notifies the AI. With the
/// real AI on, the notify is forwarded to the Lua brain (its own Goal.Parry); this Rust
/// port only drives the passive practice partner.
fn parry_interrupt(
    time: Res<Time>,
    combat: Res<Combat>,
    player: Single<(&Actor, &Transform), (With<Player>, Without<Enemy>)>,
    mut q: Query<(&mut Enemy, &mut Actor, &Transform), Without<Player>>,
    brains: NonSend<Brains>,
) {
    let dt = time.delta_secs();
    let (pa, ptf) = *player;
    let pd = &combat.player;
    for (mut e, mut a, tf) in &mut q {
        e.parry_timer -= dt;
        e.guard_count_timer -= dt;
        e.clash.retain_mut(|c| {
            c.1 -= dt;
            c.1 > 0.0
        });
        if pa.anim.is_empty() || matches!(e.mode, Mode::Dead(_) | Mode::Broken(_) | Mode::Rising(_) | Mode::Held(_)) || a.hp <= 0.0 {
            continue;
        }
        // Rising edge of the notify flag on the player's current anim.
        let notify = pd.events_at(&pa.anim, pa.t).find(|ev| ev.kind == 0 && ev.flag_type() == Some(FLAG_AI_NOTIFY)).map(|ev| ev.start);
        let Some(start) = notify else { continue };
        if (start - e.last_notify).abs() < 1e-4 && pa.t - start < 0.2 {
            continue;
        }
        e.last_notify = start;
        // With the real AI running, the script's own Goal.Parry decides (Lua interrupt).
        if e.aggressive && !brains.failed {
            e.parry_timing = true;
            continue;
        }
        // Goal.Interrupt gates: battle state (SpEffect 200004, implied here), parry interval timer.
        if e.parry_timer > 0.0 || e.is_attacking() {
            continue;
        }
        let rank = if a.resident.contains(&221000) { 0 } else if a.resident.contains(&221001) { 1 } else if a.resident.contains(&221002) { 2 } else { -1 };
        if rank < 0 {
            continue;
        }
        e.parry_timer = 0.1;
        let dist = tf.translation.distance(ptf.translation);
        let player_refs: Vec<i64> = pd.sp_effects_at(&pa.anim, pa.t).iter().map(|(id, _)| *id).collect();
        let thrust = player_refs.contains(&109970);
        let r = e.rand100();
        let facing = in_front(&a, tf.translation, ptf.translation, 90.0) && in_front(pa, ptf.translation, tf.translation, 90.0);
        // Common_Parry (common_common_func_NTC.lua) for enemies whose script uses it.
        if let Some((guard_mult, step_prob, step_type, rush_anim)) = combat.foe_of(&a).common_parry.clone() {
            let step = if step_type == 1 { "a000_005201" } else { "a000_005211" };
            let choice = if facing && dist <= PC_ATTACK_DIST_STAND {
                if player_refs.contains(&109990) {
                    Some(rush_anim.as_str()) // rush attack: EndureAttack(rushAnim)
                } else if thrust {
                    match rank {
                        0 => Some("a000_003101"),
                        1 if r <= 50 => Some("a000_003101"),
                        _ => None,
                    }
                } else if player_refs.contains(&109980) && step_type != -1 && rank == 0 {
                    Some(step)
                } else if r <= e.guard_count() * guard_mult {
                    Some("a000_003101")
                } else {
                    Some("a000_003100")
                }
            } else if facing && dist <= PC_ATTACK_DIST_STAND + 1.0 && step_type != -1 && e.rand100() <= step_prob {
                Some(step) // just out of reach: back-step away
            } else {
                None
            };
            if let Some(anim) = choice {
                let label = match anim {
                    "a000_003101" => "Deflect(3101)",
                    "a000_003100" => "Guard(3100)",
                    _ => "Parry",
                };
                a.play(label, anim);
                a.yaw = f32::atan2(-(ptf.translation - tf.translation).x, -(ptf.translation - tf.translation).z);
                a.move_vel = Vec3::ZERO;
                e.interrupt();
            }
            continue;
        }
        if !(facing && dist <= PC_ATTACK_DIST_STAND) {
            continue;
        }
        let choice = if thrust {
            // Rank 0 always deflects a thrust; rank 1 does half the time.
            if rank == 0 || (rank == 1 && r <= 50) { Some("a000_003101") } else { None }
        } else if player_refs.contains(&109980) {
            Some("a000_003100")
        } else if r <= e.guard_count() * 50 {
            Some("a000_003101")
        } else {
            Some("a000_003100")
        };
        if let Some(anim) = choice {
            let label = if anim.ends_with("101") { "Deflect(3101)" } else { "Guard(3100)" };
            a.play(label, anim);
            a.yaw = f32::atan2(-(ptf.translation - tf.translation).x, -(ptf.translation - tf.translation).z);
            a.move_vel = Vec3::ZERO;
            e.interrupt();
        }
    }
}

/// Wolf as the enemies see him (stealth.rs): his feet, capsule radius and the sight factors of
/// his active SpEffects (crouch 109200 cuts sight distances 20 %) and learned skills (`skills`:
/// Covert A 150000 sightSearchEnemyCut 20, aroundSightPointAddRate 0.5).
fn wolf_seen(combat: &Combat, pa: &Actor, pos: Vec3, skills: &[crate::data::SpEffect]) -> stealth::Seen {
    // The TAE of the clip on screen: crouch idle / moves are procedural states whose clips
    // carry the stealth SpEffect 109200.
    let (key, t) = pa.shown_clip(&combat.player);
    let mut sp: Vec<&crate::data::SpEffect> = combat.player.sp_effects_at(&key, t).into_iter().map(|(_, e)| e).collect();
    sp.extend(skills.iter());
    let feet = pos - Vec3::Y * (crate::player::CAPSULE_HALF_HEIGHT + 0.05);
    stealth::Seen::new(feet, crate::player::CAPSULE_RADIUS, &sp)
}

/// The AI sounds Wolf's TAE made this step (CreateAISound, TAE 237): (AiSoundParam id, rank)
/// of each whose radius reaches `listener`. Rows with bSpEffectEnable scale the radius by Wolf's
/// SpEffects' hearingSearchEnemyRate (Covert B 150010: 0.5). gap: that scaling is read from the
/// field names, not traced in the exe; the listener's ear_dist / ear_soundcut_dist are not
/// applied.
fn wolf_sounds(combat: &Combat, pa: &Actor, wolf: Vec3, listener: Vec3, skills: &[crate::data::SpEffect]) -> Vec<(i64, i64)> {
    // The clip on screen: walk / run loops are procedural states whose clips emit for their whole
    // length (walk a000_000200: 1000 r 0.5 m, run 000500: 1010 r 2 m, crouch walk 005200: 1001
    // r 0.25 m, crouch run 005500: 1011 r 0.5 m), refreshed while the event is active.
    let (key, t) = pa.shown_clip(&combat.player);
    let Some(an) = combat.player.anim(&key) else { return Vec::new() };
    let hearing: f64 = combat.player.sp_effects_at(&key, t).iter().map(|(_, e)| *e).chain(skills.iter()).map(|e| e.hearing_search_enemy_rate as f64).product();
    let started = |e: &&crate::data::Event| !pa.anim.is_empty() && e.start > pa.prev_t && e.start <= pa.t;
    an.events
        .iter()
        .filter(|e| e.kind == 237 && (e.active(t) || started(e)))
        .filter_map(|e| {
            let id = e.arg_i64("AISoundID")?;
            let row = combat.param("AiSoundParam", id);
            let scale = if row["bSpEffectEnable"].as_i64() == Some(1) { hearing } else { 1.0 };
            let r = row["radius"].as_f64()? * scale;
            ((wolf.distance(listener) as f64) <= r).then(|| (id, row["rank"].as_i64().unwrap_or(0)))
        })
        .collect()
}

/// c9997's state-name suffix for the AI state (UpdateAIState -> IndexAiState).
fn ai_suffix(ai_sp: i64) -> &'static str {
    match ai_sp {
        200000 => "Default",
        200001 => "CautionNoBattle",
        200002 => "CautionBattle",
        _ => "Battle",
    }
}

/// The locomotion state for a move request in this AI state (c9997 HKS move events: e.g.
/// W_WalkFrontCautionNoBattle), falling back to walking, then to the battle variant when the
/// character has no such anim (c1020 has no RunFrontCautionBattle or WalkLeftDefault).
fn loco_state(d: &crate::data::CharData, ai_sp: i64, mv: char, walk: bool) -> String {
    let base = match (mv, walk) {
        ('F', false) => "RunFront",
        ('F', true) => "WalkFront",
        ('B', _) => "WalkBack",
        ('L', _) => "WalkLeft",
        _ => "WalkRight",
    };
    let sfx = ai_suffix(ai_sp);
    [format!("{base}{sfx}"), if mv == 'F' { format!("WalkFront{sfx}") } else { String::new() }, format!("{base}Battle")]
        .into_iter()
        .find(|s| !s.is_empty() && d.anim_key(s).is_some())
        .unwrap_or_else(|| format!("{base}Battle"))
}

/// Idle / locomotion states: any AI action may start.
fn is_free_state(state: &str) -> bool {
    state.starts_with("Idle") || state.starts_with("Walk") || state.starts_with("Run") || matches!(state, "Approach" | "StandIdle")
}

#[allow(clippy::too_many_arguments)]
/// Another NPC in the fight as a possible target (think).
struct Rival {
    ent: Entity,
    team: i64,
    pos: Vec3,
    hp_rate: f32,
    guard: bool,
    sp: Vec<i64>,
}

fn think(
    mut commands: Commands,
    mut warp: ResMut<events::EventWarp>,
    time: Res<Time>,
    combat: Res<Combat>,
    config: Res<GameConfig>,
    player: Single<(&Actor, &Transform), (With<Player>, Without<Enemy>)>,
    mut q: Query<(Entity, &mut Enemy, &mut Actor, &mut Transform, Option<&Phantom>, Has<events::FromEvent>, Has<script::Scripted>), Without<Player>>,
    mut fight: ResMut<FightReset>,
    mut brains: NonSendMut<Brains>,
    mut log: Option<ResMut<crate::hud::CombatLog>>,
    mut perilous: Option<ResMut<crate::hud::Perilous>>,
    mut debug: Option<ResMut<EnemyDebug>>,
    bs: Res<script::BossScript>,
) {
    let dt = time.delta_secs();
    let (pa, ptf) = *player;
    let event_flags = bs.run.as_ref().map(|r| r.flags_on()).unwrap_or_default();
    // Every NPC in the fight, for the ones on a hostile team (combat::teams_hostile).
    let npcs: Vec<Rival> = q
        .iter()
        .filter(|x| !x.1.disabled && !x.1.is_dead() && x.2.hp > 0.0 && !x.4.is_some_and(|p| p.parked()))
        .map(|(ent, _, a, tf, ..)| Rival {
            ent,
            team: crate::combat::team_of(&combat, a),
            pos: tf.translation,
            hp_rate: a.hp / a.hp_max.max(1.0),
            guard: a.state.contains("Guard"),
            sp: combat.data_of(a).sp_effects_at(&a.anim, a.t).iter().map(|(id, _)| *id).collect(),
        })
        .collect();
    // Wolf's learned skills' SpEffects (Covert A / B) for the sight and hearing checks.
    let skills = crate::combat::skill_sp_effect_rows(&combat, &config);
    for (entity, mut e, mut a, mut tf, phantom, from_event, scripted) in &mut q {
        // A phantom out of the fight does nothing until its boss's script calls it.
        if phantom.is_some_and(|p| p.parked()) || e.disabled {
            continue;
        }
        let d = combat.data_of(&a);
        let foe = combat.foe_of(&a);
        let npc = combat.npc(&a);
        // (Also before its script has started: the first frame.)
        let scripted = scripted || script::scripts().contains_key(&foe.npc_row);
        let turn_rate = npc["turnVellocity"].as_f64().unwrap_or(135.0) as f32;
        // An anim's TAE SpEffect can switch the anim set for good (Isshin's 3015 -> 200031 = a100;
        // effectEndurance -1): the next state plays from it. It overwrites the set's SpEffect it
        // had (the "Anime ID offset" rows 200030-200034 share spCategory 100, Paramdex SDT: the
        // category that decides overwriting), so the AI no longer sees that one: the Okami
        // warriors' resident 200031 (a100: their Act31 appearance 20001) gives way to 20001's
        // 200030. gap: the exe's overwrite rule per category is not traced.
        if !a.anim.is_empty() {
            let set = d.sp_effects_at(&a.anim, a.t).iter().map(|(id, _)| *id).filter(|id| anim_group_of(d, std::iter::once(*id)).is_some()).last();
            if let Some(id) = set {
                a.anim_group = anim_group_of(d, std::iter::once(id)).unwrap_or(0);
                let cat = d.sp_effects.get(&id.to_string()).map(|s| s.sp_category);
                if !a.resident.contains(&id) && d.sp_effects.get(&id.to_string()).is_some_and(|s| s.effect_endurance < 0.0) {
                    a.resident.retain(|r| d.sp_effects.get(&r.to_string()).map(|s| s.sp_category) != cat || anim_group_of(d, std::iter::once(*r)).is_none());
                    a.resident.push(id);
                }
            }
        }
        // Boss phase rules (map events on the health bars left), while it fights or gets up;
        // a boss with its map script (script.rs) has those run instead.
        if !scripted && matches!(e.mode, Mode::Ai | Mode::Rising(_)) && !phase_events(foe.npc_row).is_empty() {
            let mut sp_now: HashSet<i64> = a.resident.iter().copied().collect();
            if !a.anim.is_empty() {
                sp_now.extend(d.sp_effects_at(&a.anim, a.t).iter().map(|(id, _)| *id));
            }
            sp_now.extend(e.clash.iter().map(|c| c.0));
            let forced = e.run_phase_events(&mut a, &combat, &sp_now);
            if events::apply_requests(&mut commands, &combat, &config, &mut e, &mut warp) {
                commands.entity(entity).despawn();
                brains.map.remove(&entity);
                continue;
            }
            if forced {
                continue;
            }
        }
        let to_player = (ptf.translation - tf.translation).with_y(0.0);
        let dist = to_player.length();
        let want_yaw = f32::atan2(-to_player.x, -to_player.z);
        match e.mode {
            Mode::Dead(t) if scripted => {
                // Its map script ends it (EzState 20200 on Wolf's Todome message, Handle Boss
                // Defeat, the duel's reset). Without a request by the end of the ThrowDef (plus
                // 2 s): its death state.
                if let Some(death) = e.throw_death.clone() {
                    if t - dt <= 0.0 {
                        e.throw_death = None;
                        a.play_state(d, &death);
                        e.mode = Mode::Dead(f32::INFINITY);
                    } else {
                        e.mode = Mode::Dead(t - dt);
                    }
                }
                continue;
            }
            Mode::Held(looped) => {
                if looped && !a.anim.is_empty() && a.t >= d.length(&a.anim) {
                    a.t = 0.0;
                    a.prev_t = 0.0;
                }
                continue;
            }
            Mode::Dead(t) => {
                // ThrowDef reaction -> its death anim on flag 69 (or when it ends).
                if let Some(death) = e.throw_death.clone() {
                    let ended = a.t >= d.length(&a.anim);
                    if (!a.anim.is_empty() && d.flag(&a.anim, a.t, FLAG_THROW_TYPE5)) || ended {
                        e.throw_death = None;
                        let mut sp_now: HashSet<i64> = a.resident.iter().copied().collect();
                        if !ended {
                            sp_now.extend(d.sp_effects_at(&a.anim, a.t).iter().map(|(id, _)| *id));
                        }
                        let fallback = if death == "Event20200" { e.fallback_death(&a, &combat, &sp_now) } else { None };
                        if let Some((state, key)) = fallback {
                            a.play(&state, &key);
                        } else {
                            a.play_state(d, &death);
                        }
                        e.mode = Mode::Dead(d.length(&a.anim) + 2.0);
                    }
                    continue;
                }
                if t - dt <= 0.0 && t.is_finite() && e.final_events(&a, &combat) {
                    // Its map event brings in the next character (Genichiro -> Tomoe): no reset.
                    if events::apply_requests(&mut commands, &combat, &config, &mut e, &mut warp) {
                        commands.entity(entity).despawn();
                        brains.map.remove(&entity);
                    } else {
                        e.mode = Mode::Dead(f32::INFINITY);
                    }
                    continue;
                }
                if t - dt <= 0.0 && from_event {
                    // A boss event's character (Tomoe) died: the fight starts over with the boss.
                    fight.0 = true;
                    continue;
                }
                if t - dt <= 0.0 {
                    let (hp, _, _, _) = npc_stats(&combat, a.kind);
                    a.hp = hp;
                    a.posture = 0.0;
                    e.ninsatsu.0 = e.ninsatsu.1;
                    a.resident = e.resident0.clone();
                    e.phase_done.clear();
                    e.phase_flags.clear();
                    e.event_req.clear();
                    e.phase_replan = false;
                    a.anim_group = anim_group_of(d, a.resident.clone().into_iter()).unwrap_or(0);
                    tf.translation = Vec3::new(e.start.x, tf.translation.y, e.start.z - 2.0);
                    e.mode = Mode::Ai;
                    e.cooldown = 2.0;
                    e.interrupt();
                    a.procedural("Approach");
                } else {
                    e.mode = Mode::Dead(t - dt);
                }
                continue;
            }
            Mode::Rising(t) => {
                if t - dt <= 0.0 && let Some(key) = loop_after(d, &a) {
                    a.play("Event21000", &key);
                    e.mode = Mode::Held(true);
                    continue;
                }
                if t - dt <= 0.0 {
                    e.mode = Mode::Ai;
                    e.cooldown = 0.5;
                    e.interrupt();
                } else {
                    e.mode = Mode::Rising(t - dt);
                }
                continue;
            }
            Mode::Broken(t) => {
                if t - dt <= 0.0 {
                    // No restore: the collapse SpEffects (220420 / 5359 / 220500) keep
                    // staminaRecoverSpeedRate 1.0 and no c1020 TAE has the ratio-restore event 961
                    // (FUN_140bd62d0), so posture simply regenerated through the collapse
                    // (FUN_140a04850 runs every frame) and the enemy keeps what it recovered.
                    // Emptied vitality: back at the brink, the next hit reopens the deathblow.
                    a.hp = a.hp.max(1.0);
                    e.mode = Mode::Ai;
                    e.cooldown = 0.5;
                    e.interrupt();
                } else {
                    e.mode = Mode::Broken(t - dt);
                }
                continue;
            }
            Mode::Ai => {}
        }
        // Map script: AI off (an intro waiting for Wolf). Its anim plays out, then it idles.
        if e.ai_off {
            if !a.anim.is_empty() && a.t >= d.length(&a.anim) && !is_free_state(&a.state) {
                a.procedural("IdleBattle");
            }
            a.move_vel = Vec3::ZERO;
            e.cur_ez = None;
            continue;
        }

        // What it knows of Wolf (stealth.rs), every frame: sounds, then sight and the state.
        // Set Character AI ID (map script) swaps its NpcThinkParam row.
        let think_id = e.think_override.unwrap_or(foe.think_id);
        let think_row = combat.param("NpcThinkParam", think_id);
        let (home, home_yaw) = *e.home.get_or_insert((tf.translation, a.yaw));
        let seen = wolf_seen(&combat, pa, ptf.translation, &skills);
        let feet = tf.translation - Vec3::Y * (crate::player::CAPSULE_HALF_HEIGHT + 0.05);
        for (id, rank) in wolf_sounds(&combat, pa, ptf.translation, tf.translation, &skills) {
            e.targeting.hear(think_row, seen.pos, id, rank);
        }
        e.targeting.update(think_row, feet, a.forward(), &seen, dt);
        if e.targeting.changed() {
            if let Some(log) = log.as_mut() {
                let (text, color) = match e.targeting.state {
                    stealth::NONE => ("enemy: lost you", Color::srgb(0.6, 0.8, 0.6)),
                    stealth::CAUTION => ("enemy: alerted (searching)", Color::srgb(1.0, 0.85, 0.2)),
                    stealth::FIND => ("enemy: found you", Color::srgb(1.0, 0.45, 0.15)),
                    _ => ("enemy: fighting", Color::srgb(1.0, 0.25, 0.15)),
                };
                log.push(text, color);
            }
        }
        // The AI-state SpEffect the playing anim's TAE sets (kept until replaced).
        if !a.anim.is_empty() {
            let set = d
                .events_at(&a.anim, a.t)
                .filter(|ev| matches!(ev.kind, 66 | 67 | 401))
                .filter_map(|ev| ev.arg_i64("SpEffectID"))
                .find(|id| AI_STATE_SP.contains(id));
            if let Some(id) = set {
                e.ai_sp = id;
            }
        }
        // Idles loop.
        if a.state.starts_with("Idle") && !a.anim.is_empty() && a.t >= d.length(&a.anim) {
            a.t = 0.0;
            a.prev_t = 0.0;
        }
        // The current target: Wolf, or the spot it is searching; or the nearest NPC of a hostile
        // team within its sight distance (eye_dist_normal) when Wolf is farther or unknown, which
        // it fights as found. gap: the sight cone and hearing are not checked against NPCs.
        let team = crate::combat::team_of(&combat, &a);
        let sight = think_row["eye_dist_normal"].as_f64().unwrap_or(20.0) as f32;
        let wolf_dist = if e.targeting.state == stealth::BATTLE { dist } else { f32::INFINITY };
        let rival = npcs
            .iter()
            .filter(|r| r.ent != entity && crate::combat::teams_hostile(team, r.team))
            .map(|r| (r, r.pos.distance(tf.translation)))
            .filter(|(_, d)| *d <= sight && *d < wolf_dist)
            .min_by(|x, y| x.1.total_cmp(&y.1))
            .map(|(r, _)| r);
        let t_pos = match rival {
            Some(r) => r.pos,
            None => e.targeting.target_pos(ptf.translation).unwrap_or(ptf.translation),
        };
        let to_t = (t_pos - tf.translation).with_y(0.0);
        let t_yaw = f32::atan2(-to_t.x, -to_t.z);

        // An AI move whose clip is a single held pose (the hidden ground zombie c1500's 20010:
        // a100_020010 imports the 1-frame a000_000401) lasts for its TAE: its state
        // (c9997.hkx Event20010_CMSG) has animeEndEventType 3 (None), so the clip's end fires
        // nothing and the pose holds while the TAE's AI-queue windows (flags 23 / 86, 0-0.5 s)
        // let the AI chain the next move (Act15 rise-and-grab 3015 while SpEffect 5030 is on).
        // gap: what ends such a held state in the exe when the AI chains nothing is not traced;
        // here the TAE's last event.
        let len = match d.anim(&a.anim) {
            Some(an) if e.cur_ez.is_some() && an.duration.is_some_and(|du| du <= 0.05) => {
                an.events.iter().map(|ev| ev.end).filter(|t| *t < 1e6).fold(d.length(&a.anim), f32::max)
            }
            _ => d.length(&a.anim),
        };
        let ended = !a.anim.is_empty() && a.t >= len;
        // c9997 DamageLightningStart / Loop _onUpdate (7972-8010): the start into a loop clip
        // (ANIME_ID_SPECIAL_DAMAGE_LIGHTNING_LOOP 8220 + GetRandomIndex 0-2, the ones it has),
        // which repeats while SP_EFFECT_REF_LIGHTNING_LOOP_LIFE (1000051: 6030-6039) is on, then
        // W_DamageLightningEnd (or W_DeathStartLightning at 0 HP).
        if ended && matches!(a.state.as_str(), "DamageLightningStart" | "DamageLightningLoop") {
            let life = a.state == "DamageLightningStart"
                || a.resident.iter().chain(e.clash.iter().map(|c| &c.0)).any(|id| d.sp_effects.get(&id.to_string()).is_some_and(|s| s.behavior_ref_id == 1000051));
            let loops: Vec<String> = (0..3).map(|i| a.grouped(&format!("a000_{:06}", 8220 + i))).filter(|k| d.anim(k).is_some()).collect();
            if life && !loops.is_empty() {
                let k = loops[(e.rand() * loops.len() as f32) as usize % loops.len()].clone();
                a.play("DamageLightningLoop", &k);
            } else if !(a.hp <= 0.0 && a.play_state(d, "DeathStartLightning")) && !a.play_state(d, "DamageLightningEnd") {
                a.procedural("IdleBattle");
            }
            continue;
        }
        let free = a.anim.is_empty() || ended || is_free_state(&a.state);
        // Perilous warning (red kanji) when the attack's TAE fires it.
        if let Some(kind) = d.perilous_warning(&a.anim, a.prev_t, a.t) {
            if let Some(p) = perilous.as_mut() {
                p.raise(kind);
            }
            if let Some(log) = log.as_mut() {
                let what = match kind {
                    980 => "sweep - jump",
                    982 => "thrust - deflect or mikiri (step in)",
                    983 => "grab - dodge",
                    _ => "unblockable",
                };
                log.push(format!("危 perilous {what}"), Color::srgb(1.0, 0.2, 0.1));
            }
        }
        // Track the player while the TAE allows turning (SetTurnSpeed), like NPC homing, except
        // inside TAE 703 FixedRotationDirection (each swing's active frames, e.g. 3000 f22-36).
        let fixed = !a.anim.is_empty()
            && d.events_at(&a.anim, a.t).any(|e| e.kind == 703 && e.args.get("IsEnable").and_then(|v| v.as_bool()).unwrap_or(false));
        if e.cur_ez.is_some() && !ended && !fixed {
            if let Some(speed) = d.turn_speed(&a.anim, a.t, true) {
                a.yaw = turn_toward(a.yaw, t_yaw, speed.to_radians() * dt);
            }
        }

        // Debug-menu drills: perilous-only / one repeated attack / standing still.
        if let Some(dbg) = debug.as_deref_mut() {
            if matches!(dbg.mode, AiMode::Perilous | AiMode::Thrust | AiMode::Repeat | AiMode::Idle) {
                drill(&mut e, &mut a, d, &config, dbg, dist, want_yaw, dt, free, ended);
                continue;
            }
        }
        // Passive (deflect practice) once fighting, or the fallback when the AI scripts are
        // missing. Before it has found Wolf the real AI runs either way (stealth).
        // Passive waits for its alert anim to have set the battle AI state (200004).
        if (!e.aggressive && e.targeting.state == stealth::BATTLE && e.ai_sp == 200004) || brains.failed {
            passive_or_fallback(&mut e, &mut a, d, &config, dist, want_yaw, dt, free, ended);
            continue;
        }

        if !brains.map.contains_key(&entity) {
            let dir = crate::paths::root().join("extracted/script");
            let radius = npc["hitRadius"].as_f64().unwrap_or(0.5) as f32;
            match Brain::load(&dir, think_id, think_row, radius, e.rng) {
                Ok(b) => {
                    for l in b.log.iter().filter(|l| !l.starts_with("stub:")) {
                        warn!("AI: {l}");
                    }
                    brains.map.insert(entity, b);
                }
                Err(err) => {
                    warn!("AI scripts unavailable ({err}); using fallback combos");
                    brains.failed = true;
                    continue;
                }
            }
        }

        // World snapshot for the brain.
        let fwd = a.forward();
        let right = fwd.cross(Vec3::Y);
        let dir = to_t.normalize_or_zero();
        let mut sp_self: HashSet<i64> = a.resident.iter().copied().collect();
        sp_self.extend(d.sp_effects_at(&a.anim, a.t).iter().map(|(id, _)| *id));
        sp_self.extend(e.clash.iter().map(|c| c.0));
        sp_self.insert(e.ai_sp);
        // The anim-set SpEffect lasts (effectEndurance -1): Corrupted Monk / Isshin read 200031 as
        // their second phase ("IsHU2", 500000_battle.lua).
        if a.anim_group > 0 {
            sp_self.insert(200030 + a.anim_group as i64);
        }
        let sp_target: HashSet<i64> = match rival {
            Some(r) => r.sp.iter().copied().collect(),
            None => combat.player.sp_effects_at(&pa.anim, pa.t).iter().map(|(id, _)| *id).collect(),
        };
        let sp_new: Vec<i64> = sp_self.iter().chain(sp_target.iter()).filter(|id| !e.prev_sp.contains(*id)).copied().collect();
        e.prev_sp = sp_self.iter().chain(sp_target.iter()).copied().collect();
        // (In any anim set: a100_003000 is EzState 3000 too.)
        let playing_ez = e.cur_ez.filter(|ez| ez_of(&a.anim) == Some(*ez));
        let mut tg = e.targeting.clone();
        if rival.is_some() {
            tg.state = stealth::BATTLE;
        }
        let snap = Snapshot {
            dist: to_t.length(),
            angle: dir.dot(right).atan2(dir.dot(fwd)).to_degrees(),
            bearing: (-to_t.x).atan2(-to_t.z).to_degrees(),
            target_guard: rival.map_or(pa.state.contains("Guard"), |r| r.guard),
            sp_self,
            sp_target,
            sp_new,
            hp_rate: a.hp / a.hp_max,
            target_hp_rate: rival.map_or(pa.hp / pa.hp_max, |r| r.hp_rate),
            sp: a.posture_max - a.posture,
            sp_rate: (1.0 - a.posture / a.posture_max).max(0.0),
            free,
            flags: AI_CANCEL_FLAGS.iter().copied().filter(|&f| !a.anim.is_empty() && d.flag(&a.anim, a.t, f)).collect(),
            ez: playing_ez,
            t: a.t,
            anim_done: ended,
            ez_started: e.ez_started.take(),
            ez_failed: e.ez_failed.take(),
            interrupted: std::mem::take(&mut e.interrupted),
            parry_timing: std::mem::take(&mut e.parry_timing),
            guard_count: e.guard_count(),
            target_use_item: rival.is_none() && pa.state.starts_with("ItemGourd") && pa.prev_t == 0.0 && pa.t > 0.0,
            me: tf.translation,
            fwd,
            home,
            home_fwd: Vec3::new(-home_yaw.sin(), 0.0, -home_yaw.cos()),
            t_state: tg.state,
            t_prev: tg.prev_state,
            t_kind: tg.kind,
            t_changed: tg.changed(),
            visible: tg.normal.is_some_and(|n| n.visible),
            forgetting: tg.forgetting_time(think_row),
            sound_id: tg.sound.map_or(0, |s| s.id),
            sound_rank: tg.sound.map_or(0, |s| s.rank),
            t_replan: tg.replan || std::mem::take(&mut e.phase_replan),
            event_req: e.event_req.clone(),
            event_flags: event_flags.clone(),
            ninsatsu: e.ninsatsu.0,
            ninsatsu_max: e.ninsatsu.1,
        };
        let brain = brains.map.get_mut(&entity).unwrap();
        let cmd = brain.tick(&snap, dt);
        if script::trace() && scripted {
            eprintln!("brain {}: anim {:?} mv {:?} at {:?} face {} free {} ez {:?} state {}", foe.npc_row, cmd.anim, cmd.mv, cmd.at, cmd.face, free, snap.ez, a.state);
        }
        for l in brain.log.drain(..) {
            if !l.starts_with("stub:") {
                warn!("AI: {l}");
            }
        }
        e.ai_desc = brain.describe();
        let c = cmd.clear;
        if c & crate::ai::CLEAR_ENEMY != 0 {
            e.targeting.clear_enemy();
        }
        if c & crate::ai::CLEAR_SOUND != 0 {
            e.targeting.clear_sound();
        }
        if c & crate::ai::CLEAR_INDICATION != 0 {
            e.targeting.clear_indication();
        }
        if c & crate::ai::CLEAR_MEMORY != 0 {
            e.targeting.clear_memory();
        }
        // Moves and turns are toward the request's point, else the current target.
        let goal_yaw = cmd.at.map_or(t_yaw, |p| {
            let v = (p - tf.translation).with_y(0.0);
            f32::atan2(-v.x, -v.z)
        });

        // Carry out the request.
        let move_ok = free || d.flag(&a.anim, a.t, FLAG_AI_MOVE);
        if let Some(ez) = cmd.anim {
            // In its anim set, or the set that has the clip (the ground-grab zombie's 20011 is
            // a100 only: Actor::grouped).
            let key = a.grouped(&format!("a000_{ez:06}"));
            if d.anim(&key).is_some() {
                a.play(&format!("Ez{ez}"), &key);
                a.move_vel = Vec3::ZERO;
                e.cur_ez = Some(ez);
                e.ez_started = Some(ez);
                if let Some(log) = log.as_mut() {
                    log.push(&format!("enemy {ez}: {}", e.ai_desc), Color::srgb(1.0, 0.7, 0.6));
                }
            } else {
                e.ez_failed = Some(ez);
            }
        } else if let (Some(mv), true) = (cmd.mv, move_ok) {
            let state = loco_state(d, e.ai_sp, mv, cmd.walk);
            if a.state != state || ended {
                a.play_state(d, &state);
            }
            e.cur_ez = None;
        } else if free && (ended || a.state.starts_with("Walk") || a.state.starts_with("Run")) {
            // The idle of its AI state (c9997 IdleTransition -> ANIME_ID_IDLE_DEFAULT 0 /
            // _CAUTION_NO_BATTLE 100000 / _CAUTION_BATTLE 200000 / _BATTLE 400000).
            let idle = match e.ai_sp {
                200000 => Some(0),
                200001 => Some(100000),
                200002 => Some(200000),
                _ => None,
            }
            .map(|id| format!("a000_{id:06}"))
            .filter(|k| d.anim(k).is_some());
            match idle {
                Some(k) => a.play(&format!("Idle{}", ai_suffix(e.ai_sp)), &k),
                None => a.procedural("IdleBattle"),
            }
            a.move_vel = Vec3::ZERO;
            e.cur_ez = None;
        }
        if (cmd.face || cmd.turn) && (free || cmd.mv.is_some()) {
            a.yaw = turn_toward(a.yaw, goal_yaw, turn_rate.to_radians() * dt);
        }
    }
}

/// Debug drills: walk into range, then every `interval` s one attack - a random perilous one, or
/// the chosen one - played as the AI would request it.
#[allow(clippy::too_many_arguments)]
fn drill(e: &mut Enemy, a: &mut Actor, d: &crate::data::CharData, config: &GameConfig, dbg: &mut EnemyDebug, dist: f32, want_yaw: f32, dt: f32, free: bool, ended: bool) {
    if !free {
        return;
    }
    if ended {
        e.cur_ez = None;
    }
    if dbg.mode == AiMode::Idle {
        a.procedural("IdleBattle");
        a.move_vel = Vec3::ZERO;
        return;
    }
    a.yaw = turn_toward(a.yaw, want_yaw, std::f32::consts::PI * dt);
    if dist > config.enemy.attack_range {
        if a.state != "WalkFrontBattle" || ended {
            a.play_state(d, "WalkFrontBattle");
        }
        return;
    }
    a.procedural("IdleBattle");
    a.move_vel = Vec3::ZERO;
    dbg.timer -= dt;
    if dbg.timer > 0.0 {
        return;
    }
    let key = if dbg.mode == AiMode::Repeat && d.anim(&dbg.attack).is_some() {
        dbg.attack.clone()
    } else {
        // Mikiri practice: the perilous attacks whose hit is an undeflectable-by-guard thrust
        // (AtkParam atkType 2 + disableGuard), the ones combat.rs turns into a Mikiri.
        let thrust_only = dbg.mode == AiMode::Thrust;
        let is_thrust = |k: &str| d.attack_windows(k).iter().any(|(_, atk, _)| atk.atk_type == 2 && atk.disable_guard == 1);
        let red: Vec<String> = attack_list(d).into_iter().filter(|(k, p)| p.is_some() && (!thrust_only || is_thrust(k))).map(|(k, _)| k).collect();
        if red.is_empty() {
            return;
        }
        red[(e.rand() * red.len() as f32) as usize % red.len()].clone()
    };
    a.yaw = want_yaw;
    let ez = ez_of(&key);
    a.play(&format!("Ez{}", ez.unwrap_or(0)), &key);
    e.cur_ez = ez;
    e.fallback = None;
    dbg.timer = dbg.interval;
}

/// Passive practice partner (walks into range and waits), and the stand-in when
/// the AI scripts are missing (aggressive: c1020's combos from its AI acts).
#[allow(clippy::too_many_arguments)]
fn passive_or_fallback(e: &mut Enemy, a: &mut Actor, d: &crate::data::CharData, config: &GameConfig, dist: f32, want_yaw: f32, dt: f32, free: bool, ended: bool) {
    if let Some((combo, step)) = e.fallback {
        let chain_point = d.flag(&a.anim, a.t, FLAG_AI_COMBO) && a.t > 0.2;
        let next = step + 1;
        if (chain_point || ended) && next < COMBOS[combo].len() && dist < config.enemy.attack_range + 2.0 {
            a.play(&format!("Combo{combo}.{next}"), COMBOS[combo][next]);
            e.cur_ez = ez_of(COMBOS[combo][next]);
            e.fallback = Some((combo, next));
        } else if ended {
            e.fallback = None;
            e.cur_ez = None;
            let r = e.rand();
            e.cooldown = config.enemy.cooldown_min + r * (config.enemy.cooldown_max - config.enemy.cooldown_min);
        }
        return;
    }
    if !free {
        return;
    }
    if ended {
        e.cur_ez = None;
    }
    e.cooldown -= dt;
    a.yaw = turn_toward(a.yaw, want_yaw, std::f32::consts::PI * dt);
    if dist > config.enemy.attack_range {
        // Battle walk speed from WalkFrontBattle's root motion.
        if a.state != "WalkFrontBattle" || ended {
            a.play_state(d, "WalkFrontBattle");
        }
    } else {
        a.procedural("IdleBattle");
        a.move_vel = Vec3::ZERO;
        if e.cooldown <= 0.0 && e.aggressive {
            let combo = (e.rand() * COMBOS.len() as f32) as usize % COMBOS.len();
            a.yaw = want_yaw;
            a.play(&format!("Combo{combo}.0"), COMBOS[combo][0]);
            e.cur_ez = ez_of(COMBOS[combo][0]);
            e.fallback = Some((combo, 0));
        }
    }
}

fn toggle_aggression(keys: Res<ButtonInput<KeyCode>>, mut q: Query<&mut Enemy>, mut log: ResMut<crate::hud::CombatLog>) {
    if keys.just_pressed(KeyCode::KeyT) {
        // All enemies switch together (the first one's state flipped).
        let Some(on) = q.iter().next().map(|e| !e.aggressive) else { return };
        for mut e in &mut q {
            e.aggressive = on;
        }
        log.push(if on { "enemy: AGGRESSIVE (real AI)" } else { "enemy: passive" }, Color::WHITE);
    }
}
