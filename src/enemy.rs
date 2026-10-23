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
pub mod world;
pub mod grapple;
pub mod script;
pub mod talk;
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
/// NPC AI cancel flags: 23 combo attack, 86 attack, 79 step, 78 move, 122 endure (ai.rs FLAG_ENDURE).
const AI_CANCEL_FLAGS: [i64; 5] = [23, 86, 79, 78, crate::ai::FLAG_ENDURE];
/// ChrActionFlag 63 on the attacker: the AI notification that fires INTERUPT_ParryTiming (exe:
/// dispatcher case 0x3f sets bit 8 of +0x8c; FUN_1407e14d0 sends AI message 7 with the sender
/// (FUN_1405a5ae0); a hostile AI's handler FUN_1405a61f0 -> FUN_14061fad0 raises the interrupt
/// when the sender is within the AI's +0x154 range and has flag 73 or vfunc +0x1a8 true.
/// gap: that range and Wolf's vfunc +0x1a8 are not read; every notify counts here).
pub const FLAG_AI_NOTIFY: i64 = 63;
/// PC_ATTACK_DIST_STAND (ai_define): standing player attack reach as the AI sees it.
const PC_ATTACK_DIST_STAND: f32 = 3.4;
/// SpEffect stateInfo 352: posture display "collapsed" (deathblow available).
const STATE_INFO_COLLAPSED: i64 = 352;

#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Mode {
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

/// changeHpRate recovery (% of max HP) of the instant SpEffects (effectEndurance 0) the anim's
/// TAE AddSpEffect (66 / 67) adds in (from, to] seconds, plus one at 0 on the anim's first
/// frame (from == 0): an event on a frame edge counts once, not in both frames.
pub(crate) fn instant_hp_rate(d: &crate::data::CharData, key: &str, from: f32, to: f32) -> f32 {
    let Some(an) = d.anim(key) else { return 0.0 };
    an.events
        .iter()
        .filter(|ev| matches!(ev.kind, 66 | 67) && ((ev.start > from && ev.start <= to) || (from == 0.0 && ev.start == 0.0)))
        .filter_map(|ev| d.sp_effects.get(&ev.arg_i64("SpEffectID").unwrap_or(0).to_string()))
        .filter(|s| s.effect_endurance == 0.0 && s.change_hp_rate < 0.0)
        .map(|s| -s.change_hp_rate)
        .sum()
}

/// ChrActionFlag 69 "ThrowType5": on ThrowDef reactions, where a killed defender switches to
/// its ThrowDefDeath anim.
const FLAG_THROW_TYPE5: i64 = 69;

/// A hit as c9997 ExecDamage reads it: env 202 damage type (DAMAGE_TYPE_GUARD 3, GUARD_BREAK
/// 1001; a plain hit 99999, FUN_140b6bd40: docs/kb/hks-graph.md), 236 damage level, 222 direction (0 left, 1 right,
/// 2 front, 3 back: GetDamageDirectionOffset onto DamageSmall a000_008000 front / 1 back / 2 left
/// / 3 right), 3056 / 3057 the guard / deflect direction (GUARD_DIR_RIGHT 1, LEFT 2, else -1).
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(not(test), allow(dead_code))]
pub struct DamageEnv {
    pub kind: i64,
    pub level: i64,
    pub dir: i64,
    pub guard_dir: i64,
    pub just_dir: i64,
    /// Its attack blocked / deflected: env 3018 / 3019 the bound side (BOUND_RIGHT 1, LEFT 2,
    /// ADD01-04 11-14), with type GUARDED_NO_BOUND_ENEMYn 1018 + n / GUARDED_BOUND_ENEMYn
    /// 1007 + n / GUARDED_STAM_ZERO 1033.
    pub bound: i64,
    pub just_bound: i64,
    /// In the air at the hit (combat::enemy_is_aerial; the script's IsDamagedAerial: env 201 off).
    pub aerial: bool,
    /// From behind the shoulder line (env 3032, the exe's 8-way sectors 2-5: the back half), set
    /// with Wolf's plain hits; otherwise the back quarter (dir 3).
    pub back_half: bool,
}

#[cfg_attr(not(test), allow(dead_code))]
impl DamageEnv {
    /// No damage this frame (DAMAGE_TYPE_NONE).
    pub const NONE: Self = Self { kind: 99999, level: 0, dir: 2, guard_dir: -1, just_dir: -1, bound: 0, just_bound: 0, aerial: false, back_half: false };
}

#[derive(Component)]
pub struct Enemy {
    pub(crate) mode: Mode,
    /// Deathblow kill pending: the death state that follows the ThrowDef reaction
    /// (ThrowDefDeath<def + 1>, or a boss's Event20200).
    throw_death: Option<String>,
    /// Deathblows left and in all (NpcParam ninsatuNum, the boss's red dots; 0 on regular
    /// enemies, which one deathblow kills). The AI reads the first (GetNinsatsuNum).
    pub ninsatsu: (u32, u32),
    /// Deathblows taken whose hit has not landed yet: the exe lowers the count (chr module
    /// +0x25c) only when the throw's atkAttribute 5 (Ninsatsu) hit is applied (FUN_140b6e6a0,
    /// 140b6.c:8770; Wolf's ThrowAttackBehavior judge 800, a211_511700 at 1.23 s), not at the
    /// throw's start. Until then the AI, the map scripts (4[37]) and the phase rules see it.
    pub(crate) ninsatsu_unhit: u32,
    /// Deathblows whose Ninsatsu hit has landed: drops.rs draws the NpcParam
    /// ninsatsuResourceItemLotId_1 / _2 lots once for each.
    pub(crate) ninsatsu_landed: u32,
    /// Killed by a map event's 2004[4] ForceCharacterDeath with shouldReceiveSouls off: no
    /// experience, Sen or drops (experience.rs, drops.rs).
    pub(crate) unrewarded: bool,
    /// NpcParam ninsatuNum is 0: nothing holds its HP at 1 (exe IsNoDead FUN_140bd5df0), so 0 HP
    /// kills it outright (die_at_zero_hp) instead of opening the deathblow.
    pub(crate) mortal: bool,
    /// The last hit's ExecDeath inputs: damage level, a fire hit on a FIRE_ACTION_ENABLE chr, and
    /// the special poison (refs 1000220 + the hit's 1000032 setting poison off, or 1000031).
    pub(crate) last_hit: (i64, bool, bool),
    /// Boss phase events (`phase_events`): the ones done, the event flags they turned on, the AI
    /// command per slot (GetEventRequest), a pending AI re-plan, and the NpcParam residents to
    /// restore on respawn (the events set / clear residents).
    phase_done: HashSet<i64>,
    phase_flags: HashSet<i64>,
    event_req: Vec<(i64, i64)>,
    /// A Request Character AI Command came this frame (INTERUPT_EventRequest on the next
    /// update; ai.rs Snapshot::event_req_new).
    event_req_new: bool,
    phase_replan: bool,
    /// Its event message (TAE 231, EMEVD IF Character Has Event Message): the exe's handler
    /// (decomp/140b5.c, case 0xe7) stores the id in the character and nothing clears it at the
    /// event's end, so it stays until the next 231 (the Monk's attacks send 70, then
    /// 2147483647 to clear it). None before the first.
    pub msg: Option<i64>,
    /// Counts the event messages sent (each TAE 231 as it starts).
    pub msg_serial: u32,
    resident0: Vec<i64>,
    /// Its armour while it holds (`armor_of`: the EMEVD NPC part of the c1180 servants).
    pub armor: Option<Armor>,
    /// Passive enemies walk up but never attack (toggle with T) - for practising timing at your own pace.
    pub aggressive: bool,
    /// Debug drill: the mode its timer runs for and the seconds to its next attack, per enemy
    /// (the shared EnemyDebug timer ran down once per enemy each tick: N enemies drilled N x as
    /// often as the menu's interval).
    drill_timer: (AiMode, f32),
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
    pub(crate) prev_sp: HashSet<i64>,
    /// TARGET_EVENT (enemy/script.rs Act::EventTarget) and its SpEffects last frame.
    pub(crate) event_target: Option<Entity>,
    event_prev_sp: HashSet<i64>,
    /// Active goal chain, for the debug panel.
    pub ai_desc: String,
    /// The AI's request this frame (what the shared NPC script reads as env 106 / 9910 / 9920
    /// and the move variables; hks_lua's enemy comparison feeds it to the game's script).
    pub ai_cmd: crate::ai::Command,
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
    pub(crate) home: Option<(Vec3, f32)>,
    /// Its spawn facing (a respawn faces it again).
    pub(crate) start_yaw: f32,
    /// It started unaware (an MSB enemy, StartUnaware) and with this map AI hold: a respawn
    /// starts so again.
    unaware0: bool,
    hold0: Option<AiHold>,
    route_end0: Option<RouteEndAnim>,
    /// Last frame's NPC rival target (rival_view).
    rival_last: Option<Entity>,
    /// Its target for the team roles (assign_team_roles): Some(None) Wolf in battle, Some(Some)
    /// an NPC rival, None no target.
    pub(crate) team_target: Option<Option<Entity>>,
    /// Wolf has hit it (react_from): its map AI hold's damage wake (4[1] / 3[23] on 10000).
    pub(crate) hurt_by_wolf: bool,
    /// HP at the map AI hold's last check (None: not checked since spawn / respawn).
    pub(crate) hp_last: Option<f32>,
    /// Where it was placed; it comes back 2 m behind it after dying (the duel's reset).
    pub(crate) start: Vec3,
    /// Phase-rule requests for other characters (events.rs): "enable" payloads, "disable"
    /// (itself), Wolf's "warp_player".
    spawn_req: Vec<serde_json::Value>,
    disable_req: bool,
    warp_req: Option<serde_json::Value>,
    /// Map script (script.rs): its AI ID (Set Character AI ID: NpcThinkParam row; an MSB enemy
    /// starts with its own MSB ThinkParamID, MsbSpot::think), AI off (Set Character AI State),
    /// immortality (HP stays at 1 or more), and down at 0 health bars.
    pub(crate) think_override: Option<i64>,
    /// Its MSB placement groups (an MSB enemy: extracted/enemies/placement_groups.json); the
    /// call-friend sounds reach only a shared one (ai_sound_heard).
    pub(crate) placement: Vec<i64>,
    pub(crate) ai_off: bool,
    immortal: bool,
    down: bool,
    /// Disabled by its map script (Change Character Enable State off): out of the fight.
    pub(crate) disabled: bool,
    /// A hidden stand-in for the map script's main character when that is a c1000 dummy (the
    /// Folding Screen Monkeys' HP-display dummy 2000800, NpcParam 12600050 "Chigo monkey boss HP
    /// display dummy character" on c1000, m20 12005810 `2003[11](1, 2000800, 0, 901260)`): it
    /// carries the boss bar and the script's HP checks (12005800) but is not on the field - no
    /// body, no AI, no hits, parked away (script.rs). The fight is picked as c1260 / 12600050
    /// (the row is in c1260's params too), so it stood in the arena as a fifth monkey that one
    /// deathblow beat.
    pub(crate) stand_in: bool,
}

/// SpEffect stateInfo 270-274: "Anime ID offset [0]-[4]" (SpEffect 200030-200034, effectEndurance -1).
const STATE_INFO_ANIM_OFFSET: std::ops::RangeInclusive<i64> = 270..=274;

/// The anim set (0-4 = a000-a400) the last of these SpEffects picks, if any is an offset one.
/// Puts a lasting (effectEndurance -1) SpEffect on an enemy as the exe's add does
/// (FUN_140c02c50, 140c0.c:2344; match FUN_140c02000 / FUN_140c05a80, accept FUN_140c064c0,
/// deletes FUN_140c05c10). Lower categoryPriority wins.
///   - spCategory 100: overwrites a row of the same category and priority (the AI states
///     200000 / 200001 at 1, the anim sets 200030-200034 at 3).
///   - 110, 130-133, 140, 150-174, 190-204: overwrites the same category.
///   - 175-189: overwrites when not worse, else refused.
///   - 141: worse refused, equal kept beside, better deletes the old.
///   - other categories: one per id.
/// Its maxHpRate / maxStaminaRate scale hp_max / posture_max (and HP / posture with them) while it
/// is worn. gap: the exe's order of a max rate against a same-frame changeHpRate (the Ape's
/// 401000) is not traced; scaling HP with the max keeps "full" full.
pub(crate) fn wear_for_good(d: &crate::data::CharData, a: &mut Actor, id: i64) {
    let row = |id: i64| d.sp_effects.get(&id.to_string());
    let Some(new) = row(id) else { return };
    if a.resident.contains(&id) {
        return;
    }
    let (cat, pri) = (new.sp_category, new.category_priority);
    let mut out: Vec<i64> = Vec::new();
    for &r in &a.resident {
        let Some(old) = row(r).filter(|o| o.sp_category == cat) else { continue };
        match cat {
            100 if old.category_priority == pri => out.push(r),
            110 | 130..=133 | 140 | 150..=174 | 190..=204 => out.push(r),
            175..=189 | 141 if pri > old.category_priority => return,
            175..=189 => out.push(r),
            141 if pri < old.category_priority => out.push(r),
            _ => {}
        }
    }
    let rates = |s: &crate::data::SpEffect| (if s.max_hp_rate > 0.0 { s.max_hp_rate } else { 1.0 }, if s.max_stamina_rate > 0.0 { s.max_stamina_rate } else { 1.0 });
    for r in out {
        a.resident.retain(|x| *x != r);
        if let Some((h, p)) = row(r).map(rates) {
            a.hp_max /= h;
            a.hp = (a.hp / h).min(a.hp_max);
            a.posture_max /= p;
            a.posture = (a.posture / p).min(a.posture_max);
        }
    }
    let (h, p) = rates(new);
    a.hp_max *= h;
    a.hp *= h;
    a.posture_max *= p;
    a.posture *= p;
    a.resident.push(id);
}

/// TAE event 0 ChrActionFlag 96 "Set Immortality" (`Enemy::tae_no_dead`).
pub(crate) const TAE_FLAG_SET_IMMORTALITY: i64 = 96;

/// The anim set (a<g>00) of the last anim-offset SpEffect (stateInfo 270-274) among `ids`. An id
/// outside the character's own table (a map event's, e.g. the tutorial ronin's 200030 from m11_00
/// 11105275 over its resident 200034) is read from the whole SpEffectParam.
pub(crate) fn anim_group_of(d: &crate::data::CharData, ids: impl Iterator<Item = i64>) -> Option<u32> {
    ids.filter_map(|id| d.sp_effects.get(&id.to_string()).map(|s| s.state_info).or_else(|| crate::data::sp_cycle(id).map(|c| c.state_info)))
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

/// The AI-state SpEffects (c9997 SP_EFFECT_REF_AI_* refs 1000000-1000003). One slot: all
/// spCategory 100 at categoryPriority 1, so the AI's state replaces a NpcParam resident one
/// (the 51 rows wearing 200000 from spawn, e.g. the piranha c1320).
pub(crate) const AI_STATE_SP: [i64; 4] = [200000, 200001, 200002, 200004];

impl Enemy {
    /// Its NpcThinkParam row: the map's (MSB ThinkParamID, Set Character AI ID), else its kind's.
    pub(crate) fn think_id(&self, foe: &crate::data::Foe) -> i64 {
        self.think_override.unwrap_or(foe.think_id)
    }

    /// Its mode (tests).
    #[cfg(test)]
    pub fn mode_name(&self) -> String {
        format!("{:?}", self.mode)
    }

    pub fn is_broken(&self) -> bool {
        matches!(self.mode, Mode::Broken(_))
    }

    pub fn is_dead(&self) -> bool {
        matches!(self.mode, Mode::Dead(_))
    }

    /// Exe SetHP (FUN_140bd64e0, 140bd.c:4187-4194) holds HP at 1 while IsNoDead
    /// (FUN_140bd5df0, 140bd.c:3381-3409): deathblows left (NpcParam ninsatuNum > 0) or an active
    /// stateInfo 143 SpEffect (5830 / 5831 / 220030-220032 / 3137015); else 0 HP is death (c9997
    /// IsDead, 966-977). The TAE ChrActionFlag 96 bit is `tae_no_dead`. gap: the debug bits and
    /// EMEVD 2004[12] on an enemy (hpmod +0x228 bit 0, inferred) are taken as off.
    pub(crate) fn dies_at_zero_hp(&self, a: &Actor, combat: &Combat) -> bool {
        if !self.mortal {
            return false;
        }
        let d = combat.data_of(a);
        let mut ids: Vec<i64> = a.resident.clone();
        if !a.anim.is_empty() {
            ids.extend(d.sp_effects_at(&a.anim, a.t).iter().map(|(id, _)| *id));
        }
        ids.extend(self.clash.iter().map(|c| c.0));
        ids.extend(a.sp_chained.iter().map(|c| c.0));
        !ids.iter().any(|id| d.sp_effects.get(&id.to_string()).map(|s| s.state_info).or_else(|| crate::data::sp_cycle(*id).map(|c| c.state_info)) == Some(143))
    }

    /// IsNoDead's TAE input (re-scout): ChrActionFlag 96 "Set Immortality" sets ActionCtrl +0x90
    /// bit 1 (FUN_140b51b70 case 0x60, 140b5.c:7587), cleared every frame (FUN_1407d8ce0), so it
    /// holds while the event is on: c1500's a000_020000-020002 and c1360's a000_020000 / 021000
    /// (from frame 0, about 3-4 s). HP then stays at 1 (SetHP FUN_140bd64e0).
    pub(crate) fn tae_no_dead(a: &Actor, combat: &Combat) -> bool {
        !a.anim.is_empty() && combat.data_of(a).flag(&a.anim, a.t, TAE_FLAG_SET_IMMORTALITY)
    }

    /// Death at 0 HP: c9997 ExecDeath's ground choice (1896-2120) from the last hit: DeathBlast
    /// (W_DeathStartBlast, 10120) for a blow / ex-blast / breath level, DeathStartSpecialPoison
    /// (10370), DeathStartFire (10200, a fire hit when the chr has no physical-type death clip:
    /// none of the regular rows checked has one), else W_DeathStartDefault: DeathDefault<0-1>
    /// (IndexDeathVariation, GetRandomIndex 0-1). Before those, a worn SpEffect's ref picks:
    /// SP_EFFECT_REF_TO_DEATH_EVENT20000 + n (1020000-1020020) the one-shot Event200nn,
    /// SP_EFFECT_REF_TO_DEATH_UNIQUE_CASE (1000180) DeathStartUniqueCase (10160: the zombie c1500
    /// killed as it lets Wolf go). The Snap Seed's back-reality death is prosthetic.rs
    /// `hit_enemy`'s. gap: the land / aerial / hide / upper / fling / weak / physical-type
    /// branches are not modelled.
    pub(crate) fn die_at_zero_hp(&mut self, a: &mut Actor, combat: &Combat) {
        let d = combat.data_of(a);
        let (level, fire, poison) = self.last_hit;
        let has = |s: &str| d.anim_key(s).is_some();
        let mut worn: Vec<i64> = crate::combat::worn_sp(a, &self.clash);
        if !a.anim.is_empty() {
            worn.extend(d.sp_effects_at(&a.anim, a.t).iter().map(|(id, _)| *id));
        }
        let refs: Vec<i64> = worn.iter().map(|id| d.sp_effects.get(&id.to_string()).map_or_else(|| crate::data::sp_effect_behavior_ref(*id), |s| s.behavior_ref_id)).collect();
        let event = (0..=20).find(|n| refs.contains(&(1_020_000 + n))).map(|n| format!("Event{}", 20_000 + n)).filter(|s| has(s));
        let pick = if let Some(ev) = event {
            ev
        } else if refs.contains(&1_000_180) && has("DeathStartUniqueCase") {
            "DeathStartUniqueCase".to_string()
        } else if matches!(level, 4 | 10 | 11) && has("DeathBlast") {
            "DeathBlast".to_string()
        } else if poison && has("DeathStartSpecialPoison") {
            "DeathStartSpecialPoison".to_string()
        } else if fire && has("DeathStartFire") {
            "DeathStartFire".to_string()
        } else {
            let i = (self.rand() * 2.0) as usize % 2;
            [format!("DeathDefault{i}"), "DeathDefault0".to_string()].into_iter().find(|s| has(s)).unwrap_or_default()
        };
        self.die_as(a, combat, &pick);
    }

    /// Its death in state `pick` (none or missing: the procedural fall).
    pub(crate) fn die_as(&mut self, a: &mut Actor, combat: &Combat, pick: &str) {
        let d = combat.data_of(a);
        self.cur_ez = None;
        self.throw_death = None;
        self.ninsatsu.0 = 0;
        self.ninsatsu_unhit = 0;
        a.hp = 0.0;
        a.move_vel = Vec3::ZERO;
        if pick.is_empty() || !a.play_state(d, pick) {
            a.procedural("Dead");
        }
        self.mode = Mode::Dead(d.length(&a.anim) + 2.0);
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
            // The Todome on a boss kept alive in its kneel (the Monk's 20200: immortal until her
            // ThrowDef13700's message 10, m25 12505965) kills it at Wolf's Ninsatsu hit, as the last
            // deathblow did before (ninsatsu_unhit; a fake death's immortality still keeps it, the
            // Guardian Ape's first 13700: script.rs).
            if self.is_down() {
                a.hp = 0.0;
                self.ninsatsu_unhit += 1;
            }
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
            self.ninsatsu_unhit += 1;
            // Its vitality comes back from the reaction's own TAE (300610 "HP reset", think:
            // instant_hp_rate); a reaction without it is refilled at once. gap: the posture
            // bar's reset (shown empty in the game) is not traced to an event.
            a.posture = 0.0;
            if !a.play_state(d, &format!("ThrowDef{def_anim}")) {
                a.procedural("PostureBroken");
            }
            if instant_hp_rate(d, &a.anim, -1.0, f32::INFINITY) <= 0.0 {
                a.hp = a.hp_max;
            }
            self.throw_death = None;
            self.mode = Mode::Rising(d.length(&a.anim).max(1.0));
            return;
        }
        // (An immortal one's map script keeps it alive after its reaction's first messages:
        // script.rs.)
        self.ninsatsu_unhit += self.ninsatsu.0;
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

    /// The AI-state SpEffect it wears (200000 default .. 200004 battle; c9997's
    /// SP_EFFECT_REF_AI_* refs 1000000-1000003).
    pub fn ai_state_sp(&self) -> i64 {
        self.ai_sp
    }

    /// Deathblows left as the game reads them (GetNinsatsuNum, 4[37]): the count less the
    /// deathblows whose ninsatsu hit has landed (`ninsatsu_unhit`).
    pub fn ninsatsu_left(&self) -> u32 {
        self.ninsatsu.0 + self.ninsatsu_unhit
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
        let bars = self.ninsatsu_left();
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
                    "clear" => {
                        a.resident.retain(|&id| id != n(1));
                        a.anim_group = anim_group_of(d, a.resident.iter().copied()).unwrap_or(0);
                    }
                    "ai" => {
                        self.event_req.retain(|&(slot, _)| slot != n(2));
                        self.event_req.push((n(2), n(1)));
                        self.event_req_new = true;
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
        // A break throw pose (ThrowDef12100 / 13100, player.rs) is the break reaction itself, as is
        // a firecracker's TrunkCollapseBurstN (prosthetic.rs hit_enemy, c9997 ExecDamageBreak).
        if !a.state.starts_with("AttackBoundEmptyStamina") && !a.state.starts_with("GuardBreak") && !a.state.starts_with("ThrowDef") && !a.state.starts_with("TrunkCollapseBurst") {
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

    /// Debug menu heal: the break window ends on the next tick, through its normal recovery
    /// (round 65: a healed broken enemy stayed open to the deathblow at full HP).
    pub(crate) fn end_break_now(&mut self) {
        if let Mode::Broken(_) = self.mode {
            self.mode = Mode::Broken(0.0);
        }
    }

    /// Test hook: the posture-break window left [s].
    #[cfg(test)]
    pub(crate) fn set_broken_left(&mut self, left: f32) {
        if let Mode::Broken(_) = self.mode {
            self.mode = Mode::Broken(left);
        }
    }

    /// Plays a reaction state; the AI replans (Kengeki) and acts again at the
    /// reaction's AI cancel flags.
    pub fn react(&mut self, a: &mut Actor, combat: &Combat, state: &str) {
        self.react_from(a, combat, true, state);
    }

    /// `react` to a contact from Wolf (`by_wolf`) or from a hostile NPC (combat.rs resolve's
    /// enemy-vs-enemy exchanges): only Wolf's makes him its battle target.
    pub fn react_from(&mut self, a: &mut Actor, combat: &Combat, by_wolf: bool, state: &str) {
        if matches!(self.mode, Mode::Dead(_) | Mode::Broken(_) | Mode::Rising(_) | Mode::Held(_)) {
            return;
        }
        // Hurt (or made to block) by Wolf: he is a battle target now (target reason 4). A
        // rival's blow did not: it put both NPCs in BATTLE on Wolf, unseen, so they dropped
        // each other when he was nearer and refused the stealth deathblow.
        if by_wolf {
            self.targeting.damaged(combat.param("NpcThinkParam", self.think_id(combat.foe_of(a))));
            self.hurt_by_wolf = true;
        }
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

    /// Engine clash result SpEffect (lasts 0.1 s); also drives the guard count. For a guard /
    /// deflect / bound reaction clip use [`crate::combat::clash_after`]: the clip's own TAE
    /// AddSpEffect row (a000_008502 GuardDamageSpecial -> 200225, a000_008602 AttackBoundEnemy2
    /// -> 200226) is what the AI reads, and this engine row would hide it (ReturnKengekiSpecialEffect
    /// takes 200215 before 200225).
    pub fn add_clash(&mut self, id: i64) {
        self.clash.push((id, 0.1));
        self.clash_count(id);
    }

    /// The consecutive-guard count's side of a clash (c9997 Set_ConsecutiveGuardCount: weak
    /// blocks count up, strong deflects reset).
    pub fn clash_count(&mut self, id: i64) {
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
    /// An attack anim (3000-3099, or the general stance's 603000-603009: AttackCommonFunction).
    pub fn is_attacking(&self) -> bool {
        matches!(self.cur_ez, Some(ez) if (3000..3100).contains(&ez) || (603000..=603009).contains(&ez))
    }

    pub fn guard_count(&self) -> i32 {
        if self.guard_count_timer > 0.0 { self.guard_count } else { 0 }
    }

    fn rand100(&mut self) -> i32 {
        (self.rand() * 100.0) as i32 + 1
    }

    pub(crate) fn rand(&mut self) -> f32 {
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
    /// AI team roles by enemy (GetTeamOrder(ORDER_TYPE_Role), assign_team_roles) and the clock
    /// to their next update.
    pub(crate) roles: HashMap<Entity, f64>,
    pub(crate) role_clock: f32,
}

/// The exe's team-order update (FUN_140640b80 -> FUN_140640050, 14064.c:157 / 207-528): every
/// 0.1 s (tunable DAT_143afb998's registered default, 14064.c:2746) the chrs on one target,
/// nearest (3D) first, are ROLE_TYPE_Attack (0.0) for the first DAT_143afb990, Torimaki (1.0,
/// surround) for the next DAT_143afb994, Kankyaku (2.0, spectate) for the rest; one without a
/// target is ROLE_TYPE_None (-1.0). The counts are the tunables' registered defaults (1 / 1,
/// 14064.c:2715-2785); gap: no shipped value overriding them was found, and the weighted mode
/// (DAT_143afb99c, NpcThinkParam TeamAttackEffectivity?) is off by default and not modelled.
pub(crate) const TEAM_ORDER_INTERVAL: f32 = 0.1;
const TEAM_ATTACKERS: usize = 1;
const TEAM_SURROUNDERS: usize = 1;

/// [`TEAM_ORDER_INTERVAL`]'s pass over (target, distance to it, chr): the role of each chr.
pub(crate) fn assign_team_roles<K: std::hash::Hash + Eq>(members: impl IntoIterator<Item = (K, f32, Entity)>) -> HashMap<Entity, f64> {
    let mut groups: HashMap<K, Vec<(f32, Entity)>> = HashMap::new();
    for (k, d, ent) in members {
        groups.entry(k).or_default().push((d, ent));
    }
    let mut roles = HashMap::new();
    for g in groups.values_mut() {
        g.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (i, (_, ent)) in g.iter().enumerate() {
            let role = if i < TEAM_ATTACKERS { 0.0 } else if i < TEAM_ATTACKERS + TEAM_SURROUNDERS { 1.0 } else { 2.0 };
            roles.insert(*ent, role);
        }
    }
    roles
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
    /// Only perilous thrusts (combat::mikiri_atk): Mikiri Counter practice.
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
}

impl Default for EnemyDebug {
    fn default() -> Self {
        // SHINOBI_AI_MODE=passive / perilous / thrust / idle: the F1 menu's AI mode from the start
        // (for shots of one kind of attack).
        let mode = match std::env::var("SHINOBI_AI_MODE").as_deref() {
            Ok("passive") => AiMode::Passive,
            Ok("perilous") => AiMode::Perilous,
            Ok("thrust") => AiMode::Thrust,
            Ok("idle") => AiMode::Idle,
            _ => AiMode::Real,
        };
        // SHINOBI_AI_ATTACK=a000_003042 (or just 3042): "repeat one attack" with that anim from
        // the start (VFX shots of one attack, e.g. the Way of Tomoe's lightning).
        if let Some(attack) = env_attack() {
            return EnemyDebug { mode: AiMode::Repeat, attack: attack.to_string(), interval: 1.5 };
        }
        EnemyDebug { mode, attack: String::new(), interval: 1.5 }
    }
}

/// SHINOBI_AI_ATTACK as an anim name (digits -> a000_<6 digits>), read once.
pub(crate) fn env_attack() -> Option<&'static str> {
    static ATTACK: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    ATTACK
        .get_or_init(|| {
            let v = std::env::var("SHINOBI_AI_ATTACK").ok()?;
            let v = v.trim();
            let attack = if !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit()) { format!("a000_{:06}", v.parse::<u32>().unwrap_or(0)) } else { v.to_string() };
            (!attack.is_empty()).then_some(attack)
        })
        .as_deref()
}

/// The enemy's own attacks (EzState 3000-3099 anims with attack windows), sorted, with their
/// perilous warning (TAE 2 BehaviorJudgeID 980-983) if any.
/// What a perilous attack is, by its first damaging window's AtkParam (the warning judge
/// 980-983 does not say: each heralds sweeps, thrusts and grabs alike, e.g. the Ochimusha's
/// Mikiri'd thrust a000_003013 under 980): "grab" (throwFlag 1 / trackType 7), "sweep"
/// (trackType 1), "thrust" (combat::mikiri_atk), else "unblockable".
pub fn perilous_kind(d: &crate::data::CharData, key: &str) -> &'static str {
    let windows = d.attack_windows(key);
    let Some((_, atk, _)) = windows.iter().filter(|(_, atk, _)| atk.throw_flag == 1 || atk.atk_phys > 0.0 || atk.atk_stam > 0.0).min_by(|x, y| x.0.start.total_cmp(&y.0.start)) else {
        return "unblockable";
    };
    if atk.throw_flag == 1 || atk.track_type == 7 {
        "grab"
    } else if atk.track_type == 1 {
        "sweep"
    } else if crate::combat::mikiri_atk(atk) {
        "thrust"
    } else {
        "unblockable"
    }
}

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
            .init_resource::<AiFlags>()
            .init_resource::<AiSounds>()
            .init_resource::<TriggerRegions>()
            .init_resource::<SwapFrame>()
            .init_resource::<AutoRestart>()
            .add_message::<SpawnEncounter>()
            .add_message::<grapple::WireHit>()
            .init_resource::<BossScript>()
            .init_resource::<world::WorldScripts>()
            .init_resource::<world::WorldBar>()
            .init_resource::<talk::Talk>()
            .add_systems(Update, talk::hold_input.after(crate::player::read_input))
            .init_resource::<script::WolfImmortal>()
            .init_resource::<WolfMessage>()
            .add_systems(Startup, spawn_enemy.after(crate::place::MapLoad))
            .add_systems(
                FixedUpdate,
                (spawn_encounter, reset_fight, stealth_env_start, start_unaware, ai_hold, route_end_anim, parry_interrupt, world::update_lod, think, grapple::on_wire_hit, ninsatsu_hit, deathblow_flinch, death_noise, todome_message, events::messages, script::run_scripts, world::run_world, talk::run_talk, events::apply_event_warp).chain().in_set(ActorSet::Decide),
            )
            .add_systems(Update, toggle_aggression.run_if(resource_exists::<ButtonInput<KeyCode>>))
            .init_resource::<grapple::EnemyGrapples>()
            .add_systems(PostUpdate, grapple::update.after(bevy::transform::TransformSystems::Propagate));
    }
}

/// TAE 231 (EzStateRequestID): a character's event message, the field EMEVD 4[8] "IF Character
/// Has Event Message" reads (chr+0x50 -> +0x50 -> +0x508; the handler 140b5.c case 0xe7 stores
/// it and nothing clears it at the event's end). Wolf's deathblows send 10 near their end
/// (a211_510000 at 3.7 s, a242_510000 at 3.6 s ...); no Todome (a2xx_511700) sends one, so the
/// 10 his last deathblow left is there when it starts. (TAE 936 is ThrowDirectionDecalParam's
/// decal: unk0 the slot, unk1 the spacing; it was read as the message before.)
pub(crate) const TAE_EVENT_MESSAGE: i32 = 231;

/// Wolf's event message (TAE 231, latched as an enemy's `Enemy::msg`; events.rs messages) and a
/// count of the ones sent. Cleared with the fight.
#[derive(Resource, Default, Clone, Copy, Debug)]
pub struct WolfMessage {
    pub msg: Option<i64>,
    pub serial: u32,
}

/// A boss's last deathblow (the Todome) ends by its map event at 0 health bars: on Wolf's event
/// message (IF Character Has Event Message 10000 / 10: the one his deathblows leave) the boss gets
/// EzState 20200, its death, and Wolf his Event7102xx (c0000.hks: GetCommandIDFromEvent ->
/// RequestThrowAnimInterrupt, W_Event7102xx; e.g. m11_02 11125874 Isshin -> 710206, m11_00
/// 11105921 Demon of Hatred -> 710207). Only 710205-710207 have a state in c0000.hkx; the others
/// (the Corrupted Monk's 710200) fire into nothing, and Wolf plays his Todome out.
/// Without the message the boss switches to 20200 on its ThrowDef's flag 69 or end (`think`).
/// A deathblow's count drop lands with Wolf's atkAttribute 5 (Ninsatsu) hit (`ninsatsu_unhit`):
/// at that hit of his deathblow, or once he is out of it without one.
fn ninsatsu_hit(combat: Res<Combat>, player: Single<&Actor, (With<Player>, Without<Enemy>)>, mut q: Query<&mut Enemy, Without<Player>>) {
    let pa = *player;
    let hit = !pa.state.starts_with("Deathblow")
        || combat.player.anim(&pa.anim).is_some_and(|an| {
            an.events.iter().any(|ev| {
                ev.start > pa.prev_t && ev.start <= pa.t && ev.arg_i64("BehaviorJudgeID").is_some_and(|j| combat.player.attacks.get(&j.to_string()).is_some_and(|atk| atk.atk_attribute == 5))
            })
        });
    if hit {
        for mut e in &mut q {
            if e.ninsatsu_unhit > 0 {
                e.ninsatsu_unhit = 0;
                e.ninsatsu_landed += 1;
            }
        }
    }
}

/// How near Wolf a fighting enemy must stand to flinch at his deathblow [m].
const DEATHBLOW_FLINCH_RANGE: f32 = 10.0;
/// Within this the near reaction plays, past it the far one [m].
const THROW_NEAR_RANGE: f32 = 4.0;

/// As Wolf's deathblow blade goes in (its atkAttribute 5 Ninsatsu hit, as `ninsatsu_hit`), the
/// other enemies fighting him within DEATHBLOW_FLINCH_RANGE drop what they are doing and jump
/// back: the HKS's own reaction to a deathblow nearby, W_ThrowNearReaction /
/// W_ThrowFarReaction (c9997 IndexThrowNearReaction / IndexThrowFarReaction; c1010
/// ThrowNearReaction0 / 1 a000_020100 / 020101, ThrowFarReaction0 / 1 020105 / 020106; c1020 one
/// each), facing him (the user's capture; "they have special animation when they jump back").
/// Near within THROW_NEAR_RANGE, a variation at random; a kind without them flinches as hit.
/// gap: the exe's trigger, range and variation pick are not traced.
fn deathblow_flinch(
    combat: Res<Combat>,
    player: Single<(&Actor, &Transform), (With<Player>, Without<Enemy>)>,
    mut q: Query<(&mut Enemy, &mut Actor, &Transform), Without<Player>>,
) {
    let (pa, ptf) = *player;
    let blade = pa.state.contains("Deathblow")
        && combat.player.anim(&pa.anim).is_some_and(|an| {
            an.events.iter().any(|ev| {
                ev.start > pa.prev_t && ev.start <= pa.t && ev.arg_i64("BehaviorJudgeID").is_some_and(|j| combat.player.attacks.get(&j.to_string()).is_some_and(|atk| atk.atk_attribute == 5))
            })
        });
    if !blade {
        return;
    }
    for (mut e, mut a, tf) in &mut q {
        if e.is_dead() || e.is_broken() || e.targeting.state != crate::stealth::BATTLE || tf.translation.distance(ptf.translation) > DEATHBLOW_FLINCH_RANGE {
            continue;
        }
        let d = combat.data_of(&a);
        let to = (ptf.translation - tf.translation).with_y(0.0);
        let kind = if to.length() <= THROW_NEAR_RANGE { "ThrowNearReaction" } else { "ThrowFarReaction" };
        let have: Vec<String> = (0..4).map(|i| format!("{kind}{i}")).filter(|s| d.anim_key(s).is_some()).collect();
        let state = if have.is_empty() {
            crate::combat::enemy_damage_state_air(d, 1, &a, ptf.translation, tf.translation, false)
        } else {
            let pick = (e.rand() * have.len() as f32) as usize % have.len();
            Some(have[pick].clone())
        };
        if let Some(state) = state {
            if to.length() > 0.01 {
                a.yaw = f32::atan2(-to.x, -to.z);
            }
            e.react_from(&mut a, &combat, false, &state);
        }
    }
}

fn todome_message(
    combat: Res<Combat>,
    mut player: Single<&mut Actor, (With<Player>, Without<Enemy>)>,
    mut q: Query<(&mut Enemy, &mut Actor), (Without<Player>, Without<script::Scripted>)>,
    mut throw: ResMut<crate::player::ActiveThrow>,
    wolf_msg: Res<WolfMessage>,
) {
    let pa = &mut **player;
    // A level test, as the EMEVD condition: the message Wolf holds now.
    let sent: Vec<i64> = wolf_msg.msg.into_iter().collect();
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
            if pa.play_state(&combat.player, &crate::player::event_state(&combat.player, *id)) {
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
    set_fight(&config.enemy.chr, config.enemy.npc_row);
    spawn_lineup(&mut commands, &combat, &config, false);
}

/// The fight's boss NpcParam row (set_fight).
static FIGHT_ROW: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(-1);

/// The fight is now `chr` / `npc_row` (config enemy.*; None: the chr's roster row). Set at
/// startup and by an encounter swap (spawn_encounter; map.rs switch_map before it places Wolf).
pub fn set_fight(chr: &str, npc_row: Option<i64>) {
    let row = npc_row.or_else(|| {
        let text = std::fs::read_to_string(crate::paths::extracted().join(format!("enemies/{chr}.json"))).ok()?;
        serde_json::from_str::<serde_json::Value>(&text).ok()?["roster"]["npc"].as_i64()
    });
    FIGHT_ROW.store(row.unwrap_or(-1), std::sync::atomic::Ordering::Relaxed);
}

/// The arena that is a boss row's own: map.rs boss_arena's stem when that arena was cut from the
/// boss's own MSB map (map_<stem>.json "map" = boss_scripts.json "map"). The user's rule
/// (2026-10-11): only the Monk, Butterfly, the Apes and the Dragons keep their own surroundings;
/// the others share Isshin's field boss_54000000 (m11_02), which is still the own arena of the
/// rows fought there (54000000, Genichiro Way of Tomoe 71000000, 71101000).
fn own_arena_of(row: i64) -> Option<String> {
    static CACHE: std::sync::OnceLock<HashMap<i64, String>> = std::sync::OnceLock::new();
    let all = CACHE.get_or_init(|| {
        let dir = crate::paths::extracted();
        let read = |p: std::path::PathBuf| std::fs::read_to_string(p).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok());
        let (Some(index), Some(scripts)) = (read(dir.join("map_boss_arenas.json")), read(dir.join("enemies/boss_scripts.json"))) else { return HashMap::new() };
        let moved: Vec<i64> = index["moved"].as_array().into_iter().flatten().filter_map(|v| v.as_i64()).collect();
        let mut out = HashMap::new();
        for (row, stem) in index.as_object().into_iter().flatten() {
            let (Ok(r), Some(stem)) = (row.parse::<i64>(), stem.as_str()) else { continue };
            let Some(arena) = read(dir.join(format!("map_{stem}.json"))) else { continue };
            // Same map is not enough: the fights Map MOVED onto another boss's arena (the index's
            // "moved" list, tools/boss_arenas.py MOVED: the Owl, Emma, Isshin Ashina onto
            // Genichiro's castle top, m11_01 too) are strangers there.
            if arena["map"].as_str().is_some() && arena["map"].as_str() == scripts[row]["map"].as_str() && !moved.contains(&r) {
                out.insert(r, stem.to_string());
            }
        }
        out
    });
    all.get(&row).cloned()
}

/// An armoured enemy's armour: the NPC part its map event makes (tools/armor_parts.py ->
/// extracted/enemies/armor_parts.json; common_func 20005410: the c1180 servants 11807101,
/// 11801100, 11806010). While it holds, the part (group 2, the PelvisArmor bodies) takes Wolf's
/// hits and passes `body_rate` (0.7) of them on, and its SpEffect 400250 halves HP and posture
/// damage: x0.35 HP, x0.5 posture, as live (kb/damage.md). Only the prosthetic `prosthetic` (11 =
/// Loaded Spear) damages the part; at part HP 0 it plays `break_anim` (20039), its SpEffects go
/// and `after` (230400 / 230401) come.
#[derive(Clone, Debug, serde::Deserialize)]
pub struct Armor {
    pub part_hp: f32,
    pub damage_correction: f32,
    pub body_rate: f32,
    pub prosthetic: i64,
    pub sp_effects: Vec<i64>,
    pub break_anim: i64,
    pub after: Vec<i64>,
    /// Its SpEffects' <attr>DamageCutRate / def<Attr>StaminaDmgRate, multiplied (the export
    /// reads SpEffectParam: these rows are not in the combat data).
    #[serde(default)]
    pub rates: HashMap<String, f32>,
}

/// The armour of an NpcParam row (`Armor`), if its map event gives it one.
pub fn armor_of(row: i64) -> Option<Armor> {
    static CACHE: std::sync::OnceLock<HashMap<i64, Armor>> = std::sync::OnceLock::new();
    CACHE
        .get_or_init(|| {
            let path = crate::paths::extracted().join("enemies/armor_parts.json");
            let text = std::fs::read_to_string(path).unwrap_or_default();
            let all: HashMap<String, Armor> = serde_json::from_str(&text).unwrap_or_default();
            all.into_iter().filter_map(|(k, v)| Some((k.parse().ok()?, v))).collect()
        })
        .get(&row)
        .cloned()
}

impl Armor {
    /// HP and posture rates of a hit on it: its SpEffects' <attr>DamageCutRate /
    /// def<Attr>StaminaDmgRate for the attack's staminaPhysicsAttribute (400250: 0.5 each), the
    /// HP also x body_rate. gap: the exe's attribute of the HP cut (atkAttribute?) is not
    /// traced; the one uncut live hit of 8 (a non-armour body?) neither.
    pub fn rates(&self, atk_attribute: i64, stamina_attribute: i64) -> (f32, f32) {
        let rate = |f: Option<&str>| f.and_then(|f| self.rates.get(f)).copied().unwrap_or(1.0);
        (rate(crate::data::damage_cut_field(atk_attribute)) * self.body_rate, rate(crate::data::stamina_rate_field(stamina_attribute)))
    }

    /// The armour breaks: its break anim, its SpEffects swapped for `after`.
    pub fn shatter(&self, e: &mut Enemy, a: &mut Actor, combat: &Combat) {
        a.resident.retain(|id| !self.sp_effects.contains(id));
        for &id in &self.after {
            if !a.resident.contains(&id) {
                a.resident.push(id);
            }
        }
        let d = combat.data_of(a);
        let state = format!("Event{}", self.break_anim);
        if !matches!(e.mode, Mode::Dead(_)) && a.play_state(d, &state) {
            e.interrupt();
            a.move_vel = Vec3::ZERO;
            e.mode = Mode::Rising(d.length(&a.anim).max(0.1));
        }
        e.armor = None;
    }
}

/// Is the fight on its boss's own arena (`own_arena_of` the fight's row is the loaded map,
/// map::loaded_map)? Elsewhere the map is a stranger's: its MSB heights and spots do not hold.
pub fn on_own_arena() -> bool {
    let row = FIGHT_ROW.load(std::sync::atomic::Ordering::Relaxed);
    crate::place::on_boss_arena() && own_arena_of(row).is_some_and(|a| crate::place::loaded_map().as_deref() == Some(a.as_str()))
}

/// The duel's enemies: config enemy.chr 4 m ahead of Wolf, then each config enemy.group entry
/// at its `at` (x, z), else in a row beside the first (3 m apart). On another boss's arena
/// (after an encounter swap) the line-up keeps its 8 m from Wolf's start: that boss's spot can
/// be in a cave (the Guardian Ape's grotto, map.rs player_start), out of sight.
fn spawn_lineup(commands: &mut Commands, combat: &Combat, config: &GameConfig, aggressive: bool) {
    // Idol travel's run roams the map: no arena fight, at the start or after a rest.
    if (crate::encounter::hooks().idol_start)().is_some() {
        info!("line-up: none (idol travel)");
        return;
    }
    // Line-up spots are arena-local (x, z); `at` puts them in the fight's space (map.rs Frame:
    // the source map's absolute place under sekiro-rs's map).
    let frame = crate::place::frame();
    let start = (crate::encounter::hooks().player_start)();
    let dz = if crate::place::on_boss_arena() && !on_own_arena() { (crate::encounter::hooks().player_start_local)().z - 4.0 } else { 0.0 };
    // Standing on the ground under each spot (Isshin's shared field slopes 0.27 m per metre: 8 m
    // ahead of Wolf's start is 1.3 m higher); the start's height without a map or over nothing.
    let at = |x: f32, z: f32| {
        let p = frame.point(Vec3::new(x, 0.0, z));
        let floor = crate::terrain::terrain().and_then(|t| t.enemy_floor(p.x, p.z, start.y + 3.0)).unwrap_or(frame.offset.y);
        p.with_y(floor + crate::player::CAPSULE_HALF_HEIGHT + 0.05)
    };
    // Facing Wolf's start (the export's enemy faces +Z; forward = Ry(yaw) * -Z).
    let facing = |x: f32, z: f32| {
        let v = start - frame.point(Vec3::new(x, 0.0, z));
        f32::atan2(-v.x, -v.z)
    };
    let hooks = crate::encounter::hooks();
    if let Some((chr, spots)) = (hooks.patrols)(config) {
        // The map's guards walk their rounds (the Construct's courtyard), unaware of Wolf until
        // they see or hear him.
        if let Some(kind) = combat.kind_index(chr, None) {
            for (i, (at, yaw, route)) in spots.into_iter().enumerate() {
                // On whatever is under its point: the ground, the terrace, the tower.
                let floor = crate::terrain::terrain().and_then(|t| t.enemy_floor(at.x, at.z, at.y + 1.0)).unwrap_or(at.y);
                let at = at.with_y(floor + crate::player::CAPSULE_HALF_HEIGHT + 0.05);
                let e = spawn_one(commands, combat, config, kind, at, yaw, true, 0x2545_F491 ^ 0x85EB_CA6B_u32.wrapping_mul(i as u32 + 1));
                commands.entity(e).insert((PatrolRoute(route), StartUnaware, crate::encounter::PatrolGuard));
            }
        }
    }
    // Where the map puts the config's enemy (the Construct: the middle of the second section,
    // facing its door), its group in a row beside it.
    let spot = (hooks.duel_spot)(config);
    let construct = spot.is_some();
    let first = spot.map_or(Vec3::new(0.0, 0.0, -4.0 + dz), |s| s.0);
    let first_yaw = spot.map_or_else(|| facing(first.x, first.z), |s| s.1);
    let first_at = if construct { first.with_y(at(first.x, first.z).y) } else { at(first.x, first.z) };
    info!("line-up: {} at {first_at} (Wolf's start {start}), yaw {first_yaw:.2}", config.enemy.chr);
    spawn_one(commands, combat, config, 0, first_at, first_yaw, aggressive, 0x9E37_79B9);
    for (i, g) in config.enemy.group.iter().enumerate() {
        let Some(kind) = combat.kind_index(&g.chr, g.npc_row) else { continue };
        // An MSB enemy of the loaded map: at its spot, its way, on its patrol route.
        if let Some(spot) = g.msb.and_then(msb_patrol) {
            // On the floor under its own MSB height (a step up allowed), not the line-up's search
            // from Wolf's start: the gate's MSB enemies stand from 5 m below to 11 m above it.
            let floor = crate::terrain::terrain().and_then(|t| t.enemy_floor(spot.at.x, spot.at.z, spot.at.y + crate::terrain::STEP_HEIGHT)).unwrap_or(spot.at.y);
            let at = Vec3::new(spot.at.x, floor + crate::player::CAPSULE_HALF_HEIGHT + 0.05, spot.at.z);
            let e = spawn_one(commands, combat, config, kind, at, spot.yaw, aggressive, 0x9E37_79B9 ^ 0x85EB_CA6B_u32.wrapping_mul(i as u32 + 1));
            let think = msb_think(combat, &spot);
            commands.entity(e).insert((spot.route, StartUnaware));
            let placement = g.msb.map(msb_groups).unwrap_or_default();
            commands.entity(e).queue(move |mut w: EntityWorldMut| {
                if let Some(mut en) = w.get_mut::<Enemy>() {
                    if think.is_some() {
                        en.think_override = think;
                    }
                    en.placement = placement;
                }
            });
            if let Some(hold) = spot.hold {
                commands.entity(e).insert(hold);
            }
            if let Some(re) = spot.route_end {
                commands.entity(e).insert(re);
            }
            continue;
        }
        let side = if i % 2 == 0 { 1.0 } else { -1.0 } * (3.0 * (i / 2 + 1) as f32);
        let [x, z] = g.at.unwrap_or(if construct { [first.x + side, first.z] } else { [side, -4.0] });
        let z = z + dz;
        let yaw = if construct { first_yaw } else { facing(x, z) };
        // A different seed each, so the group does not move in lockstep.
        let spot = if construct { Vec3::new(x, at(x, z).y, z) } else { at(x, z) };
        spawn_one(commands, combat, config, kind, spot, yaw, aggressive, 0x9E37_79B9 ^ 0x85EB_CA6B_u32.wrapping_mul(i as u32 + 1));
    }
}

/// An MSB enemy's patrol route on the loaded map, in world space (ai::Patrol takes it when
/// the enemy's brain is made, also after a respawn).
#[derive(Component, Clone, Debug)]
pub struct PatrolRoute(pub crate::ai::Patrol);

/// An MSB enemy starts unaware of Wolf (no battle target, AI state reset: `make_unaware`), as
/// the map's enemies do, so its NonBattleAct walks its patrol route; the duel line-up starts
/// fighting instead.
#[derive(Component)]
pub struct StartUnaware;

fn start_unaware(mut commands: Commands, mut q: Query<(Entity, &mut Enemy, &mut Actor, Option<&AiHold>, Option<&RouteEndAnim>), With<StartUnaware>>) {
    for (entity, mut e, mut a, hold, route_end) in &mut q {
        e.make_unaware(&mut a);
        // (A respawn starts so again.)
        e.unaware0 = true;
        e.hold0 = hold.cloned();
        e.route_end0 = route_end.cloned();
        commands.entity(entity).remove::<StartUnaware>();
    }
}

/// Its map event's AI hold (common_func 20005110, the gate's c1010 1110236 and 14 more on
/// patrols.json "wake", tools/patrols.py): 2004[1] Set Character AI State off at the map's
/// start, then on (with Force Animation -1) once Wolf (10000) is inside the wake region (3[2]),
/// it has one of the listed SpEffects (4[5]: 3500-3503, 3520, 8300, 220018, 230110/1, 9045,
/// 230518/9) or Wolf damaged it (4[1] / 3[23]). Held, it stands idle like a map script's AI
/// off (think). gap: hits it guards (no HP lost) do not wake it; the Force Animation -1 is not
/// played (its anim is already idle).
#[derive(Component, Clone, Debug)]
pub struct AiHold {
    pub region: Option<WakeRegion>,
    pub sp_effects: Vec<i64>,
}

/// Its map event's route-end anim (m11_01 11115236 / 11115238 on the patrollers 1110236 /
/// 1110252; patrols.json "route_end", tools/patrols.py): the event waits for the enemy inside its
/// region (3[2] on itself: a 1.2 m sphere at the type 5 route's end) or alerted (4[9] AI state
/// 1 / 2 / 3); alerted, it ends; else 2003[18] Force Animation Playback 20004 once. The exe never
/// reads its Should Loop byte (FUN_140904ef0 case 0x12 -> FUN_140bd84c0 keeps the anim id only;
/// FUN_140bd81b0 fires W_Event20004 and clears it the same frame), so W_Event20004 plays to its end
/// (cxxxx.lua:49 env(339) -> W_Idle) unless a hit or death takes it (EventCommonFunction,
/// c9997.lua:3884): its clip has no AI cancel flag (env 107 / 118 / 120 / 123-125 stay -1).
#[derive(Component, Clone, Debug)]
pub struct RouteEndAnim {
    pub region: WakeRegion,
    pub anim: i64,
}

/// Runs the [`RouteEndAnim`] events.
fn route_end_anim(mut commands: Commands, combat: Res<Combat>, mut q: Query<(Entity, &mut Enemy, &mut Actor, &Transform, &RouteEndAnim)>) {
    for (entity, mut e, mut a, tf, re) in &mut q {
        if e.is_dead() || e.disabled || e.stand_in {
            continue;
        }
        if e.targeting.state != stealth::NONE {
            commands.entity(entity).remove::<RouteEndAnim>();
            continue;
        }
        let feet = tf.translation - Vec3::Y * (crate::player::CAPSULE_HALF_HEIGHT + 0.05);
        if matches!(e.mode, Mode::Ai) && re.region.contains(feet) {
            let d = combat.data_of(&a);
            if a.play_state(d, &format!("Event{}", re.anim)) {
                e.interrupt();
                a.move_vel = Vec3::ZERO;
            }
            commands.entity(entity).remove::<RouteEndAnim>();
        }
    }
}

/// An MSB region in the arena: its spot, its local X / Z axes (the MSB rotation's Y turned like
/// the map) and its shape (SoulsFormats MSB/Shape.cs ShapeType): 1 circle [r] and 4 rectangle
/// [w, d] (flat: any height), 2 sphere [r], 3 cylinder [r, h], 5 box [w, d, h]. Boxes and
/// cylinders stand on the spot (DSMapStudio RenderableProxy.cs:1364 draws the box from
/// (-0.5, 0, -0.5) to (0.5, 1, 0.5)); spheres are centred on it.
#[derive(Clone, Debug)]
pub struct WakeRegion {
    pub shape: i64,
    pub at: Vec3,
    pub x: Vec3,
    pub z: Vec3,
    pub size: Vec<f32>,
}

impl WakeRegion {
    /// Whether the point (a character's feet) is inside.
    pub fn contains(&self, p: Vec3) -> bool {
        let d = p - self.at;
        let (lx, ly, lz) = (d.dot(self.x), d.y, d.dot(self.z));
        let s = |i: usize| self.size.get(i).copied().unwrap_or(0.0);
        match self.shape {
            // Strict radius tests, as FUN_1409825b0 / FUN_140983380 (14098.c:2274 / ~2372).
            1 => lx * lx + lz * lz < s(0) * s(0),
            2 => d.length_squared() < s(0) * s(0),
            3 => lx * lx + lz * lz < s(0) * s(0) && (0.0..=s(1)).contains(&ly),
            4 => lx.abs() <= s(0) / 2.0 && lz.abs() <= s(1) / 2.0,
            5 => lx.abs() <= s(0) / 2.0 && lz.abs() <= s(1) / 2.0 && (0.0..=s(2)).contains(&ly),
            _ => false,
        }
    }
}

/// INTERUPT_UseItem: Wolf just started a gourd drink, standing or crouched (W_ItemGourdDrink /
/// W_CrouchItemGourdDrink; c1020 10203010 punishes it with 3012).
pub(crate) fn wolf_starts_an_item(pa: &Actor) -> bool {
    // Not the empty gourd's shake (ItemGourdDrinkFailed): HKS acts AI_INTERUPT_USE_ITEM only on
    // the drink branch (c0000_transition.lua 3971 vs 4010-4037; round 30).
    let s = pa.state.trim_start_matches("Crouch");
    s.starts_with("ItemGourd") && !s.contains("Failed") && pa.prev_t == 0.0 && pa.t > 0.0
}

/// Holds the AI of the enemies with an [`AiHold`] until their map event wakes them.
fn ai_hold(
    mut commands: Commands,
    combat: Res<Combat>,
    player: Single<&Transform, (With<Player>, Without<Enemy>)>,
    mut q: Query<(Entity, &mut Enemy, &Actor, &AiHold)>,
) {
    let wolf = player.translation - Vec3::Y * crate::player::CAPSULE_HALF_HEIGHT;
    for (entity, mut e, a, hold) in &mut q {
        let d = combat.data_of(a);
        // Also the SpEffects Wolf's bullets put on it (Enemy::add_timed: the Finger Whistle's
        // 230518 / 230519, which every patrols.json wake list has).
        let timed: Vec<i64> = e.clash.iter().map(|c| c.0).collect();
        let has = |sp: i64| {
            a.resident.contains(&sp)
                || timed.contains(&sp)
                || (!a.anim.is_empty() && d.sp_effects_at(&a.anim, a.t).iter().any(|(id, _)| *id == sp))
                || (!a.add_anim.is_empty() && d.sp_effects_at(&a.add_anim, a.add_t).iter().any(|(id, _)| *id == sp))
        };
        // 4[1] IF Character Damaged By Wolf / 3[23] IF Damage Type: the event's damage record is
        // written only for a hit whose final HP damage is > 0 (FUN_1406d8950 from FUN_140b6e150,
        // 140b6.c:8033; read by the EMEVD conditions at DAT_143d56058 +0x38): a guarded or
        // deflected hit with its HP fully cut leaves it held. Ours: HP lost since the last check
        // and a reaction Wolf caused (react_from) since then.
        let hurt = e.hurt_by_wolf && e.hp_last.is_some_and(|h| a.hp < h);
        e.hp_last = Some(a.hp);
        e.hurt_by_wolf = false;
        let wake = hold.region.as_ref().is_some_and(|r| r.contains(wolf)) || hold.sp_effects.iter().any(|&sp| has(sp)) || hurt;
        e.ai_off = !wake;
        if wake {
            commands.entity(entity).remove::<AiHold>();
        }
    }
}

/// Where an MSB enemy stands on the loaded map, its route and its AI hold (`msb_patrol`).
pub struct MsbSpot {
    pub at: Vec3,
    pub yaw: f32,
    pub route: PatrolRoute,
    pub hold: Option<AiHold>,
    pub route_end: Option<RouteEndAnim>,
    /// Part.Enemy ThinkParamID (patrols.json "think"): its own NpcThinkParam row (1110252,
    /// NpcParam 10102210, has 10102200; the roster's first 10102210 placement another).
    pub think: Option<i64>,
}

/// The MSB enemy `entity` of the loaded map's MSB (map_<id>.json "map") from
/// extracted/enemies/patrols.json (tools/patrols.py), turned into the arena like the map
/// pieces: minus the export origin, turned by arena_yaw (sekiro-extract map.rs Arena::point),
/// X mirrored. Its yaw is the MSB rotation's +Z the same way, as an Actor yaw (forward =
/// Ry(yaw) * -Z). None without a map, or when the entity has no route there.
pub fn msb_patrol(entity: i64) -> Option<MsbSpot> {
    msb_patrol_on(&crate::place::loaded_map()?, entity)
}

/// The MSB placement groups of enemy `entity` on the loaded map (tools/placement_groups.py).
pub fn msb_groups(entity: i64) -> Vec<i64> {
    static GROUPS: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();
    let groups = GROUPS.get_or_init(|| {
        std::fs::read_to_string(crate::paths::extracted().join("enemies/placement_groups.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    });
    let Some(stem) = crate::place::loaded_map() else { return Vec::new() };
    let Some(map) = std::fs::read_to_string(crate::paths::extracted().join(format!("map_{stem}.json"))).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()) else {
        return Vec::new();
    };
    let Some(m) = map["map"].as_str() else { return Vec::new() };
    groups[m][entity.to_string()].as_array().map_or(Vec::new(), |a| a.iter().filter_map(|v| v.as_i64()).collect())
}

/// [`msb_patrol`] on the exported map `stem` (extracted/map_<stem>.json).
pub fn msb_patrol_on(stem: &str, entity: i64) -> Option<MsbSpot> {
    let map: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(crate::paths::extracted().join(format!("map_{stem}.json"))).ok()?).ok()?;
    let (origin, yaw) = crate::place::json_frame(&map)?;
    msb_spot_in(map["map"].as_str()?, origin, yaw, entity)
}

/// The patrol of MSB enemy `entity` of game map `map` (patrols.json), minus `origin`, turned by
/// `yaw`, X mirrored (origin 0 and no turn: the open world's absolute place, sekiro-rs's map).
pub fn msb_spot_in(map: &str, origin: Vec3, yaw: f32, entity: i64) -> Option<MsbSpot> {
    static PATROLS: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();
    let patrols = PATROLS.get_or_init(|| {
        std::fs::read_to_string(crate::paths::extracted().join("enemies/patrols.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    });
    let p = patrols.get(map)?.get(entity.to_string())?;
    let f = |v: &serde_json::Value| v.as_f64().unwrap_or(0.0) as f32;
    let (s, c) = yaw.sin_cos();
    let turn = |x: f32, z: f32| Vec3::new(-(x * c + z * s), 0.0, -x * s + z * c);
    let point = |v: &serde_json::Value| -> Option<Vec3> {
        let a = v.as_array()?;
        Some(turn(f(&a[0]) - origin.x, f(&a[2]) - origin.z).with_y(f(&a[1]) - origin.y))
    };
    let at = point(&p["pos"])?;
    let (sy, cy) = f(&p["yaw"]).to_radians().sin_cos();
    let fwd = turn(sy, cy);
    let region_of = |r: &serde_json::Value| {
        point(&r["pos"]).map(|at| {
            let (sy, cy) = f(&r["yaw"]).to_radians().sin_cos();
            WakeRegion {
                shape: r["shape"].as_i64().unwrap_or(0),
                at,
                x: turn(cy, -sy),
                z: turn(sy, cy),
                size: r["size"].as_array().into_iter().flatten().map(f).collect(),
            }
        })
    };
    let hold = p.get("wake").map(|w| AiHold { region: region_of(&w["region"]), sp_effects: w["sp_effects"].as_array().into_iter().flatten().filter_map(|v| v.as_i64()).collect() });
    let route_end = p.get("route_end").and_then(|r| Some(RouteEndAnim { region: region_of(&r["region"])?, anim: r["anim"].as_i64()? }));
    let points = p["points"].as_array()?.iter().filter_map(point).collect::<Vec<_>>();
    let yaws: Vec<f32> = p["yaws"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|v| {
            let (s, c) = f(v).to_radians().sin_cos();
            let fwd = turn(s, c);
            (-fwd.x).atan2(-fwd.z)
        })
        .collect();
    let arrivals = p["arrivals"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|a| (a["region"].as_i64().unwrap_or(-1), a["anim"].as_i64().unwrap_or(-1), a["wait"].as_f64().unwrap_or(0.0)))
        .collect();
    let regions = p["regions"].as_array().into_iter().flatten().filter_map(|v| v.as_i64()).collect();
    let areas = p["areas"].as_array().into_iter().flatten().map(region_of).collect();
    Some(MsbSpot {
        at,
        yaw: (-fwd.x).atan2(-fwd.z),
        route: PatrolRoute({
            let mut route = crate::ai::Patrol::new(p["type"].as_i64().unwrap_or(0), points, regions, arrivals);
            route.yaws = yaws;
            route.areas = areas;
            route
        }),
        hold,
        route_end,
        think: p["think"].as_i64().filter(|t| *t > 0),
    })
}

/// The loaded map's MSB regions by entity id (`map_regions_on`); in the host's open world
/// (no exported map loaded) every play block's ([`world_regions`]).
pub fn map_regions() -> Option<std::sync::Arc<HashMap<i64, WakeRegion>>> {
    match crate::place::loaded_map() {
        Some(stem) => map_regions_on(&stem),
        None => world_regions(),
    }
}

/// The open world's regions: every play block's ([`crate::encounter::MapHooks::blocks`]) MSB
/// regions in the space its world scripts place characters in (MSB with X mirrored,
/// script.rs `place_of`). Built once the blocks are known; none outside the open world.
fn world_regions() -> Option<std::sync::Arc<HashMap<i64, WakeRegion>>> {
    static CACHE: std::sync::Mutex<Option<std::sync::Arc<HashMap<i64, WakeRegion>>>> = std::sync::Mutex::new(None);
    let mut cache = CACHE.lock().ok()?;
    if let Some(r) = cache.as_ref() {
        return Some(r.clone());
    }
    let blocks = (crate::encounter::hooks().blocks)();
    if blocks.is_empty() {
        return None;
    }
    let all: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(crate::paths::extracted().join("enemies/regions.json")).ok()?).ok()?;
    let f = |v: &serde_json::Value| v.as_f64().unwrap_or(0.0) as f32;
    let mirror = |x: f32, z: f32| Vec3::new(-x, 0.0, z);
    let mut out = HashMap::new();
    for block in &blocks {
        for (id, r) in all[block.as_str()].as_object().into_iter().flatten() {
            let (Ok(id), Some(a)) = (id.parse(), r["pos"].as_array()) else { continue };
            let (sy, cy) = f(&r["yaw"]).to_radians().sin_cos();
            let size = r["size"].as_array().into_iter().flatten().map(f).collect();
            out.insert(id, WakeRegion { shape: r["shape"].as_i64().unwrap_or(-1), at: mirror(f(&a[0]), f(&a[2])).with_y(f(&a[1])), x: mirror(cy, -sy), z: mirror(sy, cy), size });
        }
    }
    let r = std::sync::Arc::new(out);
    *cache = Some(r.clone());
    Some(r)
}

/// Every region with an entity id of the exported map `stem`'s MSB (extracted/enemies/
/// regions.json, tools/regions.py), turned into the arena like `msb_patrol_on`'s regions. The
/// AI's IsInsideTargetRegion / SetEventMoveTarget look one up by the id alone (FUN_1406d35d0,
/// 1406d.c:2135); point regions (shape 0) only give a spot. Cached per map.
pub fn map_regions_on(stem: &str) -> Option<std::sync::Arc<HashMap<i64, WakeRegion>>> {
    type Cache = std::sync::Mutex<HashMap<String, Option<std::sync::Arc<HashMap<i64, WakeRegion>>>>>;
    static ALL: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();
    static CACHE: std::sync::OnceLock<Cache> = std::sync::OnceLock::new();
    let mut cache = CACHE.get_or_init(Default::default).lock().ok()?;
    if let Some(r) = cache.get(stem) {
        return r.clone();
    }
    let all = ALL.get_or_init(|| {
        std::fs::read_to_string(crate::paths::extracted().join("enemies/regions.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    });
    let build = || -> Option<HashMap<i64, WakeRegion>> {
        let map: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(crate::paths::extracted().join(format!("map_{stem}.json"))).ok()?).ok()?;
        let f = |v: &serde_json::Value| v.as_f64().unwrap_or(0.0) as f32;
        let (origin, yaw) = crate::place::json_frame(&map)?;
        let (s, c) = yaw.sin_cos();
        let turn = |x: f32, z: f32| Vec3::new(-(x * c + z * s), 0.0, -x * s + z * c);
        let regions = all[map["map"].as_str()?].as_object()?;
        Some(
            regions
                .iter()
                .filter_map(|(id, r)| {
                    let a = r["pos"].as_array()?;
                    let at = turn(f(&a[0]) - origin.x, f(&a[2]) - origin.z).with_y(f(&a[1]) - origin.y);
                    let (sy, cy) = f(&r["yaw"]).to_radians().sin_cos();
                    let size = r["size"].as_array().into_iter().flatten().map(f).collect();
                    Some((id.parse().ok()?, WakeRegion { shape: r["shape"].as_i64().unwrap_or(-1), at, x: turn(cy, -sy), z: turn(sy, cy), size }))
                })
                .collect(),
        )
    };
    let r = build().map(std::sync::Arc::new);
    cache.insert(stem.to_string(), r.clone());
    r
}

/// An MSB enemy's own NpcThinkParam row ([`MsbSpot::think`]) when its kind has the row.
pub(crate) fn msb_think(combat: &Combat, spot: &MsbSpot) -> Option<i64> {
    spot.think.filter(|t| !combat.param("NpcThinkParam", *t).is_null())
}

/// Starts the whole fight over (Wolf's reset or death, or the death of a character a boss event
/// brought in): every enemy goes and the starting line-up comes back, the way the game reloads
/// the boss fight. Keeps the AI on/off setting.
#[derive(Resource, Default)]
pub struct FightReset(pub bool);

/// Starts another fight without relaunching (the encounter picker; `data::encounter_list`):
/// config enemy.chr / npc_row / group take its values, Combat loads the kinds
/// (`Combat::load_encounter`), AnimLib follows, and FightReset brings in the new line-up.
/// gap: the map stays (a boss's own arena, map.rs boss_arena, is picked at startup only).
#[derive(Message, Clone, Debug, Default)]
pub struct SpawnEncounter {
    pub chr: String,
    pub npc_row: Option<i64>,
    pub group: Vec<crate::config::GroupEnemy>,
}

fn spawn_encounter(
    mut msgs: MessageReader<SpawnEncounter>,
    mut combat: ResMut<Combat>,
    lib: Option<ResMut<crate::anim::AnimLib>>,
    mut config: ResMut<GameConfig>,
    mut reset: ResMut<FightReset>,
    mut swap: ResMut<SwapFrame>,
    frame: Option<Res<bevy::diagnostic::FrameCount>>,
) {
    let Some(m) = msgs.read().last().cloned() else { return };
    set_fight(&m.chr, m.npc_row);
    config.enemy.chr = m.chr.clone();
    config.enemy.npc_row = m.npc_row;
    config.enemy.group = m.group.clone();
    combat.load_encounter(&m.chr, m.npc_row, &m.group);
    if let Some(mut lib) = lib {
        lib.sync_kinds(&combat);
    }
    info!("encounter: {} {:?} (+{} group) -> {} kinds", m.chr, m.npc_row, m.group.len(), combat.kinds.len());
    reset.0 = true;
    swap.0 = frame.map(|f| f.0);
}

/// The frame of the last encounter swap: its line-up waits for a later frame, after map.rs
/// switch_map (Update, after this FixedUpdate) has loaded the new arena and its terrain.
#[derive(Resource, Default)]
struct SwapFrame(Option<u32>);

/// A won boss fight starts over by itself 5 s after the boss's defeat (BossScript run.defeated,
/// set by the map event's Handle Boss Defeat). The menu turns it off to keep its Won screen up.
#[derive(Resource)]
pub struct AutoRestart(pub bool);

impl Default for AutoRestart {
    fn default() -> Self {
        AutoRestart(true)
    }
}

/// The event flags the enemies' AI set (SetEventFlag) when no boss script runs; with one they go
/// to its flags. The fight starts with them off (none of 504000 / 509000 / 151010's flags is
/// set by an EMEVD - checked against every extracted/event/*.emevd).
#[derive(Resource, Default)]
pub struct AiFlags(pub HashMap<i64, bool>);

/// AI sounds in the air (exe SprjAiSound entries, manager FUN_14064a5f0): the attacks'
/// AtkParam AppearAiSoundId (once per attack, at the attacker: FUN_1409a1290, 1409a.c:2197) and
/// HitAiSoundId (once per target it lands on, guarded or not: FUN_1409e68e0, 1409e.c:3685), each
/// for its AiSoundParam lifeFrame. Wolf's TAE 237 sounds stay wolf_sounds. The owner is the
/// attacker (inferred for Hit: the exe emits through the target's module).
#[derive(Resource, Default)]
pub struct AiSounds(pub Vec<AiSound>);

#[derive(Clone, Debug)]
pub struct AiSound {
    pub id: i64,
    pub pos: Vec3,
    /// The emitting NPC, None for Wolf.
    pub owner: Option<Entity>,
    /// The owner's team (combat::team_of), None for Wolf.
    pub team: Option<i64>,
    pub life: f32,
    /// The record's serial (GetLatestSoundTargetInstanceID), kept when the owner refreshes it.
    pub serial: i64,
    /// The owner NPC's AI is in Battle or Find (think refreshes it each frame): a bCallFriend
    /// sound is heard only then (FUN_140610470, 14061.c:1201-1223). True for Wolf's (no AI).
    pub owner_alert: bool,
    /// The owner's MSB placement groups (`Enemy::placement`, refreshed with owner_alert).
    pub owner_groups: Vec<i64>,
    /// Heard by the owner's side only (`AiSounds::call`: a spotter's shout to its comrades; not
    /// a game flag).
    pub friends_only: bool,
}

/// The call an enemy gives as it spots Wolf (think): AiSoundParam 2000000.
const SPOT_CALL_SOUND: i64 = 2000000;
/// The noise of an enemy's death (`death_noise`): AiSoundParam 4060 (20 m, heard by friends and
/// foes, rank 1, not a call). gap: no row is tied to a death in the data; 4060 is picked by its
/// range and listeners.
const DEATH_NOISE_SOUND: i64 = 4060;

/// An enemy's fall is heard by its comrades (the user: "should get aware when there is
/// non-player noise"): `DEATH_NOISE_SOUND` where it dies, once per death.
fn death_noise(combat: Res<Combat>, q: Query<(Entity, &Enemy, &Actor, &Transform)>, mut sounds: ResMut<AiSounds>, mut dead: Local<HashSet<Entity>>) {
    dead.retain(|e| q.get(*e).is_ok_and(|(_, en, ..)| en.is_dead()));
    for (ent, e, a, tf) in &q {
        if e.is_dead() && dead.insert(ent) {
            sounds.emit(&combat, DEATH_NOISE_SOUND, tf.translation, Some(ent), Some(crate::combat::team_of(&combat, a)));
        }
    }
}

/// The exe's sound-record counter (DAT_143d547b0, FUN_14060f840). Ours starts past every
/// AiSoundParam id, which Wolf's TAE sounds use as their serial.
static SOUND_SERIAL: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(1 << 32);

/// Every live trigger region (RegisterTriggerRegion, ai.rs TriggerRegion): one store for all
/// AIs, as the exe's singleton DAT_143d55070. think counts their lives down each frame.
#[derive(Resource, Default)]
pub struct TriggerRegions(pub Vec<crate::ai::TriggerRegion>);

impl AiSounds {
    /// Adds a sound (ids <= 0 and rows of radius 0 are none, FUN_14060f290 / 14060f840); the same
    /// owner's same sound is refreshed (FUN_14060fa90 merges an owner's sound of one rank).
    pub fn emit(&mut self, combat: &Combat, id: i64, pos: Vec3, owner: Option<Entity>, team: Option<i64>) {
        let row = combat.param("AiSoundParam", id);
        if id <= 0 || row["radius"].as_f64().unwrap_or(0.0) <= 0.0 {
            return;
        }
        let life = row["lifeFrame"].as_f64().unwrap_or(0.0).max(1.0 / 30.0) as f32;
        let old = self.0.iter().position(|s| s.owner == owner && s.id == id).map(|i| self.0.remove(i).serial);
        let serial = old.unwrap_or_else(|| SOUND_SERIAL.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
        self.0.push(AiSound { id, pos, owner, team, life, serial, owner_alert: true, owner_groups: Vec::new(), friends_only: false });
    }

    /// `emit` for a call to the owner's comrades only (`think`'s spot call): its foes (a maddened
    /// dog beside it, test a_maddened_dog_bites_the_soldier) do not take it as a sound target.
    pub fn call(&mut self, combat: &Combat, id: i64, pos: Vec3, owner: Entity, team: i64) {
        self.emit(combat, id, pos, Some(owner), Some(team));
        if let Some(s) = self.0.iter_mut().rev().find(|s| s.owner == Some(owner) && s.id == id) {
            s.friends_only = true;
        }
    }
}

/// Whether listener `me` (team `team`) at `at` hears `s` (exe SprjAiSoundNormalListener,
/// FUN_140610470, 14061.c:1181): the row's selfTarget / opposeTarget / friendlyTarget flag for
/// the owner's relation to it (FUN_140c09760's team matrix, here combat::teams_hostile; Wolf is
/// hostile to every NPC), then the range as wolf_sounds' (FUN_140610b30: hearing rate x radius
/// less the cut, within the ear distance). A bCallFriend row is heard only while its NPC owner's
/// AI is in Battle or Find (`owner_alert`; an owner in Idle or Caution calls no one). A
/// bRestrictSamePlacementGroup row (the call-friend sounds 2000000-2155000 and 4050-5400) needs
/// the owner and listener `mine` to share an MSB placement group (FUN_140610810, the per-ChrSet
/// group bytes: an owner in none passes, a listener in none fails; tools/placement_groups.py).
/// gap: bSpEffectEnable's owner scale (1) is not modelled.
pub(crate) fn ai_sound_heard(combat: &Combat, s: &AiSound, me: Entity, team: i64, la: &Actor, at: Vec3, think: &serde_json::Value, mine: &[i64]) -> Option<(i64, i64)> {
    let row = combat.param("AiSoundParam", s.id);
    let flag = if s.owner == Some(me) {
        "selfTarget"
    } else if s.team.is_none_or(|t| crate::combat::teams_hostile(t, team)) {
        "opposeTarget"
    } else {
        "friendlyTarget"
    };
    if row[flag].as_i64() != Some(1) || (row["bCallFriend"].as_i64() == Some(1) && !s.owner_alert) || (s.friends_only && flag != "friendlyTarget") {
        return None;
    }
    if row["bRestrictSamePlacementGroup"].as_i64() == Some(1) && !s.owner_groups.is_empty() && !s.owner_groups.iter().any(|g| mine.contains(g)) {
        return None;
    }
    let ear = think["ear_dist"].as_f64().unwrap_or(0.0);
    let cut = think["ear_soundcut_dist"].as_f64().unwrap_or(0.0);
    let reach = hearing_rate(combat, la) * row["radius"].as_f64()? - cut;
    (reach > 0.0 && (s.pos.distance(at) as f64) < reach + ear).then(|| (s.id, row["rank"].as_i64().unwrap_or(0)))
}

fn reset_fight(
    mut commands: Commands,
    mut reset: ResMut<FightReset>,
    combat: Res<Combat>,
    config: Res<GameConfig>,
    q: Query<(Entity, &Enemy, Has<crate::encounter::PatrolGuard>)>,
    mut brains: NonSendMut<Brains>,
    mut warp: ResMut<events::EventWarp>,
    mut bs: ResMut<BossScript>,
    mut ai_flags: ResMut<AiFlags>,
    swap: Res<SwapFrame>,
    frame: Option<Res<bevy::diagnostic::FrameCount>>,
    (mut statuses, bullets): (ResMut<crate::status::Statuses>, Query<Entity, With<crate::enemy_bullet::EnemyBullet>>),
    perilous: Option<ResMut<crate::combat::Perilous>>,
    (wolf_bullets, mut effects): (Query<(Entity, &Transform), With<crate::prosthetic::Projectile>>, Query<&mut crate::fxr::FxEffect>),
    mut wolf_msg: ResMut<WolfMessage>,
) {
    if !reset.0 || swap.0.is_some_and(|f| frame.is_some_and(|n| n.0 == f)) {
        return;
    }
    reset.0 = false;
    bs.resets += 1;
    ai_flags.0.clear();
    *wolf_msg = WolfMessage::default();
    // Wolf's poison / burn gauges and afflictions end with the fight, and the old line-up's
    // arrows in flight go (they still applied their build-up after the reset).
    statuses.clear_all();
    // The perilous kanji of the old line-up's attack goes too.
    if let Some(mut p) = perilous {
        p.timer = 0.0;
    }
    for b in &bullets {
        commands.entity(b).despawn();
    }
    // Wolf's prosthetic bullets go as well; their flying effects stop where they were, as an
    // ended bullet's do (prosthetic.rs fly: stop, no anchor; what they spawned plays out).
    for mut f in &mut effects {
        let Some(crate::fxr::Anchor::Entity(b)) = f.anchor else { continue };
        if let Ok((_, tf)) = wolf_bullets.get(b) {
            f.stop = true;
            f.follow = false;
            f.anchor = None;
            f.at = *tf;
        }
    }
    for (b, _) in &wolf_bullets {
        commands.entity(b).despawn();
    }
    if bs.run.take().is_some_and(|r| r.defeated.is_some()) {
        bs.defeats += 1;
    }
    // (The courtyard guards are always aggressive: the duel enemy's setting carries over.)
    let aggressive = q.iter().find(|(_, _, guard)| !guard).is_some_and(|(_, e, _)| e.aggressive);
    for (ent, ..) in &q {
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
    actor.super_armor = crate::actor::SuperArmor::new(npc["superArmorDurability"].as_f64().unwrap_or(0.0) as f32, npc["superArmorRecoverCorrection"].as_f64().unwrap_or(0.0) as f32);
    // Its armour's SpEffects (220061 / 220062 / 400250), set by its map event (armor_of).
    let armor = armor_of(k.foe.npc_row);
    for &id in armor.iter().flat_map(|ar| ar.sp_effects.iter()) {
        if !actor.resident.contains(&id) {
            actor.resident.push(id);
        }
    }
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
            drill_timer: (AiMode::Real, 0.0),
            cooldown: 1.5,
            rng,
            guard_count: 0,
            guard_count_timer: 0.0,
            parry_timer: 0.0,
            clash: Vec::new(),
            last_notify: -1.0,
            cur_ez: None,
            throw_death: None,
            armor,
            ninsatsu: {
                let n = npc["ninsatuNum"].as_i64().unwrap_or(0).max(1) as u32;
                (n, n)
            },
            ninsatsu_unhit: 0,
            ninsatsu_landed: 0,
            unrewarded: false,
            phase_done: HashSet::new(),
            phase_flags: HashSet::new(),
            event_req: Vec::new(),
            event_req_new: false,
            phase_replan: false,
            msg: None,
            msg_serial: 0,
            resident0: actor.resident.clone(),
            ez_started: None,
            ez_failed: None,
            interrupted: false,
            parry_timing: false,
            prev_sp: HashSet::new(),
            event_target: None,
            event_prev_sp: HashSet::new(),
            ai_desc: String::new(),
            ai_cmd: crate::ai::Command::default(),
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
            start_yaw: yaw,
            unaware0: false,
            hold0: None,
            route_end0: None,
            rival_last: None,
            team_target: None,
            hurt_by_wolf: false,
            mortal: npc["ninsatuNum"].as_i64().unwrap_or(0) == 0,
            last_hit: (0, false, false),
            hp_last: None,
            start: at,
            spawn_req: Vec::new(),
            disable_req: false,
            warp_req: None,
            think_override: None,
            placement: Vec::new(),
            ai_off: false,
            immortal: false,
            down: false,
            disabled: false,
            stand_in: false,
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
    // Runs after the skeletons (world.rs): an actor with one has its model, no capsule.
    q: Query<Entity, (With<Enemy>, Without<Mesh3d>, Without<crate::anim::Skeleton>)>,
) {
    for e in &q {
        commands
            .entity(e)
            .insert((Mesh3d(meshes.add(Capsule3d::new(0.45, 1.1))), MeshMaterial3d(materials.add(Color::srgb(0.45, 0.2, 0.18)))))
            .with_children(|p| crate::model::spawn_blade(p, &mut meshes, &mut materials, Color::srgb(0.9, 0.75, 0.6)));
    }
}

/// The targeting the AI sees: its own, or with an NPC rival as the target (teams_hostile) a
/// BATTLE one on that rival.
/// BATTLE one on that rival (TYPE_NORMAL_ENEMY). Its state changes only on the frame the rival
/// comes or goes: the real prev_state (NONE / CAUTION) kept IsChangeState true every frame, and
/// _COMMON_AddStateTransitionGoal looped 401040 instead of attacking.
pub(crate) fn rival_view(real: &Targeting, rival: Option<Entity>, last: Option<Entity>) -> Targeting {
    let mut tg = real.clone();
    if rival.is_some() {
        if rival == last {
            tg.prev_state = stealth::BATTLE;
            tg.prev_kind = stealth::TYPE_NORMAL_ENEMY;
        } else if last.is_none() {
            tg.prev_state = real.state;
            tg.prev_kind = real.kind;
        }
        tg.state = stealth::BATTLE;
        tg.kind = stealth::TYPE_NORMAL_ENEMY;
    } else if last.is_some() {
        tg.prev_state = stealth::BATTLE;
        tg.prev_kind = stealth::TYPE_NORMAL_ENEMY;
    }
    tg
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
        // Timed SpEffects run on the character's own clock (chr+0x108, hit-stop scaled; R64).
        let sp_dt = a.clock(dt);
        e.clash.retain_mut(|c| {
            c.1 -= sp_dt;
            c.1 > 0.0
        });
        if pa.anim.is_empty() || matches!(e.mode, Mode::Dead(_) | Mode::Broken(_) | Mode::Rising(_) | Mode::Held(_)) || a.hp <= 0.0 {
            continue;
        }
        // Rising edge of the notify flag on the player's current anim.
        let notify = pd.events_at(&pa.anim, pa.t).find(|ev| ev.kind == 0 && ev.flag_type() == Some(FLAG_AI_NOTIFY)).map(|ev| ev.start);
        // Seen once per flag window: last_notify is cleared when no window is on, so the next
        // swing whose window starts at the same time (Combo1 and Combo2 both at 0.6) counts again.
        let Some(start) = notify else {
            e.last_notify = -1.0;
            continue;
        };
        if (start - e.last_notify).abs() < 1e-4 {
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
pub(crate) fn wolf_seen(combat: &Combat, pa: &Actor, pos: Vec3, skills: &[crate::data::SpEffect]) -> stealth::Seen {
    // The TAE of the clip on screen: crouch idle / moves are procedural states whose clips
    // carry the stealth SpEffect 109200.
    let (key, t) = pa.shown_clip(&combat.player);
    let mut sp: Vec<&crate::data::SpEffect> = combat.player.sp_effects_at(&key, t).into_iter().map(|(_, e)| e).collect();
    let skills = skills_in_force(skills, &sp);
    sp.extend(skills);
    let feet = pos - Vec3::Y * (crate::player::CAPSULE_HALF_HEIGHT + 0.05);
    stealth::Seen::new(feet, crate::player::CAPSULE_RADIUS, &sp)
}

/// The learned skills' SpEffects that work now: those without invocationConditionsStateChange1-3,
/// or with one of them a stateInfo on Wolf (his active SpEffects', or a skill's own), the rule
/// player.rs uses for the deathblow recoveries. Covert A 150000 needs 949, the crouch / wall-hug
/// / hang stateInfo (109200 / 109210 / 109220 / 109221): standing, its sight cut is off.
fn skills_in_force<'a>(skills: &'a [crate::data::SpEffect], active: &[&crate::data::SpEffect]) -> Vec<&'a crate::data::SpEffect> {
    let states: Vec<i64> = active.iter().map(|s| s.state_info).chain(skills.iter().map(|s| s.state_info)).filter(|&s| s > 0).collect();
    skills
        .iter()
        .filter(|s| {
            let conds = [s.invocation_conditions_state_change1, s.invocation_conditions_state_change2, s.invocation_conditions_state_change3];
            conds.iter().all(|&c| c <= 0) || conds.iter().any(|c| *c > 0 && states.contains(c))
        })
        .collect()
}

/// The AI sounds Wolf's TAE made this step (CreateAISound, TAE 237): (AiSoundParam id, rank)
/// of each the listener `la` (at `listener`, NpcThinkParam `think`) hears. Rows with
/// bSpEffectEnable scale the radius by Wolf's SpEffects' hearingSearchEnemyRate (Covert B
/// 150010: 0.5; gap: that scaling is read from the field names, not traced). The listener side
/// is the exe's (FUN_140610470 -> FUN_140610b30 -> FUN_140610a30): reach = its hearing rate
/// (`hearing_rate`) x radius - ear_soundcut_dist (none if <= 0); heard while the distance less
/// both bodies' radii is under reach + ear_dist x the listener's factor at +0x20 (gap: taken as
/// 1). gap: the radii are the vfuncs +0x50 (listener) / FUN_1409efb80 (source): our capsule
/// radii stand in.
fn wolf_sounds(combat: &Combat, pa: &Actor, wolf: Vec3, la: &Actor, listener: Vec3, think: &serde_json::Value, skills: &[crate::data::SpEffect]) -> Vec<(i64, i64)> {
    let rate = hearing_rate(combat, la);
    let ear = think["ear_dist"].as_f64().unwrap_or(0.0);
    let cut = think["ear_soundcut_dist"].as_f64().unwrap_or(0.0);
    let gap = (wolf.distance(listener) - crate::player::CAPSULE_RADIUS - crate::combat::BODY_RADIUS) as f64;
    // The clip on screen: walk / run loops are procedural states whose clips emit for their whole
    // length (walk a000_000200: 1000 r 0.5 m, run 000500: 1010 r 2 m, crouch walk 005200: 1001
    // r 0.25 m, crouch run 005500: 1011 r 0.5 m), refreshed while the event is active.
    let (key, t) = pa.shown_clip(&combat.player);
    let Some(an) = combat.player.anim(&key) else { return Vec::new() };
    let active: Vec<&crate::data::SpEffect> = combat.player.sp_effects_at(&key, t).into_iter().map(|(_, e)| e).collect();
    let hearing: f64 = active.iter().copied().chain(skills_in_force(skills, &active)).map(|e| e.hearing_search_enemy_rate as f64).product();
    let started = |e: &&crate::data::Event| !pa.anim.is_empty() && e.start > pa.prev_t && e.start <= pa.t;
    an.events
        .iter()
        .filter(|e| e.kind == 237 && (e.active(t) || started(e)))
        .filter_map(|e| {
            let id = e.arg_i64("AISoundID")?;
            let row = combat.param("AiSoundParam", id);
            let scale = if row["bSpEffectEnable"].as_i64() == Some(1) { hearing } else { 1.0 };
            let reach = rate * row["radius"].as_f64()? * scale - cut;
            (reach > 0.0 && gap < reach + ear).then(|| (id, row["rank"].as_i64().unwrap_or(0)))
        })
        .collect()
}

/// A listener's hearing rate (exe FUN_140bfe590): prod(1 - hearingSearchEnemyCut / 100) over
/// its SpEffects (resident and its clip's TAE ones), within 0..1. gap: the base factor at the
/// SpEffect module +0x14 is taken as 1; timed effects other enemies or Wolf's tools put on it
/// are not held per enemy yet.
pub(crate) fn hearing_rate(combat: &Combat, la: &Actor) -> f64 {
    let d = combat.data_of(la);
    let row = |id: i64| d.sp_effects.get(&id.to_string()).or_else(|| combat.player.sp_effects.get(&id.to_string()));
    let clip: Vec<i64> = if la.anim.is_empty() { Vec::new() } else { d.sp_effects_at(&la.anim, la.t).into_iter().map(|(id, _)| id).collect() };
    let r: f64 = la.resident.iter().chain(&clip).filter_map(|&id| row(id)).map(|e| 1.0 - (e.hearing_search_enemy_cut as f64).max(0.0) / 100.0).product();
    r.clamp(0.0, 1.0)
}

/// The observer rates of the SpEffects put on an enemy in play (Enemy::clash, Actor::sp_chained):
/// the product of their sightSearchRate, and of (1 - hearingSearchEnemyCut / 100).
fn put_on_rates(e: &Enemy, a: &Actor) -> (f32, f64) {
    let mut sight = 1.0f32;
    let mut ear = 1.0f64;
    for id in e.clash.iter().map(|c| c.0).chain(a.sp_chained.iter().map(|c| c.0)) {
        let row = crate::items::param("SpEffectParam", id);
        sight *= row["sightSearchRate"].as_f64().unwrap_or(1.0) as f32;
        ear *= 1.0 - row["hearingSearchEnemyCut"].as_f64().unwrap_or(0.0).clamp(0.0, 100.0) / 100.0;
    }
    (sight.max(0.0), ear)
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
    // The guard stance's idle and walks too (GuardIdle_onUpdate -> IdleCommonFunction(STYLE_GUARD)).
    state.starts_with("GuardIdle") || state.starts_with("GuardWalk") || state.starts_with("Idle") || state.starts_with("Walk") || state.starts_with("Run") || matches!(state, "Approach" | "StandIdle")
}

#[allow(clippy::too_many_arguments)]
/// Another NPC in the fight as a possible target (think).
struct Rival {
    ent: Entity,
    team: i64,
    pos: Vec3,
    /// Its facing (IsInsideTargetEx with TARGET_ENE_0 as the observer).
    fwd: Vec3,
    hp_rate: f32,
    guard: bool,
    sp: Vec<i64>,
    /// Posture left and its rate (GetSp / GetSpRate on it).
    posture_left: f32,
    posture_rate: f32,
}

/// An AI script message, logged once per distinct text (a failing call repeats every frame).
fn warn_once(line: &str) {
    static SEEN: std::sync::Mutex<Option<HashSet<String>>> = std::sync::Mutex::new(None);
    let mut seen = SEEN.lock().unwrap_or_else(|e| e.into_inner());
    if seen.get_or_insert_with(HashSet::new).insert(line.to_string()) {
        warn!("AI: {line}");
    }
}

/// Where a regular enemy's new life starts: `back` m behind its spawn spot along its start
/// facing `yaw` (forward = Ry(yaw) * -Z), on the floor there found from its spawn height (a step
/// up, plus up to 1 m per metre of those 2 m for a slope); the spawn height without a map or a
/// floor.
fn respawn_spot(start: Vec3, yaw: f32, back: f32, terrain: Option<&crate::terrain::Terrain>) -> Vec3 {
    let behind = Quat::from_rotation_y(yaw) * Vec3::Z * back;
    let (x, z) = (start.x + behind.x, start.z + behind.z);
    let feet = start.y - crate::player::CAPSULE_HALF_HEIGHT;
    let floor = terrain.and_then(|t| t.enemy_floor(x, z, feet + crate::terrain::STEP_HEIGHT + 2.0));
    Vec3::new(x, floor.map_or(start.y, |h| h + crate::player::CAPSULE_HALF_HEIGHT), z)
}

#[cfg(test)]
mod respawn_tests {
    use super::*;

    /// Killed 3 m down a slope from its spot, it comes back on the slope at its start, not buried
    /// at the height it died at (2026-10-12 bug hunt round 6).
    #[test]
    fn a_respawn_stands_on_the_floor_at_its_start() {
        // A ramp rising 0.5 m per metre toward -z (z 10 -> -10: y -5 -> 5).
        let verts = vec![Vec3::new(-10.0, -5.0, 10.0), Vec3::new(10.0, -5.0, 10.0), Vec3::new(10.0, 5.0, -10.0), Vec3::new(-10.0, 5.0, -10.0)];
        let t = crate::terrain::Terrain::from_tris(verts, [([0, 1, 2], 0), ([0, 2, 3], 0)].into_iter());
        let half = crate::player::CAPSULE_HALF_HEIGHT;
        let start = Vec3::new(1.0, 0.0 + half + 0.05, 0.0);
        // Facing +Z (yaw pi): behind is -Z, up the ramp.
        let pi = std::f32::consts::PI;
        let at = respawn_spot(start, pi, 2.0, Some(&t));
        // (Terrain::floor takes the highest of the feet's samples 8 cm around: +0.04 m here.)
        assert!((at - Vec3::new(1.0, 1.0 + half, -2.0)).length() < 0.05, "{at}");
        // Without a map: the spawn height.
        assert!((respawn_spot(start, pi, 2.0, None) - Vec3::new(1.0, start.y, -2.0)).length() < 1e-4);
        // An MSB enemy (no offset): its own spot.
        assert_eq!(respawn_spot(start, pi, 0.0, None), start);
    }

    /// A sandbox line-up enemy past Wolf (z >= 9, yaw 0: facing -Z toward him) comes back 2 m
    /// behind itself (+Z), not 2 m in front along world -Z (2026-10-12 bug hunt round 21).
    #[test]
    fn a_respawn_steps_back_along_its_start_facing() {
        let start = Vec3::new(0.0, 1.0, 9.0);
        assert!((respawn_spot(start, 0.0, 2.0, None) - Vec3::new(0.0, 1.0, 11.0)).length() < 1e-4);
        // Facing +X (yaw -pi/2: Ry(-pi/2) * -Z = +X): behind is -X.
        let at = respawn_spot(start, -std::f32::consts::FRAC_PI_2, 2.0, None);
        assert!((at - Vec3::new(-2.0, 1.0, 9.0)).length() < 1e-4, "{at}");
    }
}

fn think(
    mut commands: Commands,
    mut warp: ResMut<events::EventWarp>,
    time: Res<Time>,
    combat: Res<Combat>,
    config: Res<GameConfig>,
    player: Single<(Entity, &Actor, &Transform, &Player), (With<Player>, Without<Enemy>)>,
    mut q: Query<(Entity, &mut Enemy, &mut Actor, &mut Transform, Option<&Phantom>, Has<events::FromEvent>, Has<script::Scripted>, Option<&mut crate::model::DrawMask>), Without<Player>>,
    mut fight: ResMut<FightReset>,
    mut brains: NonSendMut<Brains>,
    (routes, dummies, markers): (Query<&PatrolRoute>, Query<&crate::model::Dummies>, Query<&GlobalTransform>),
    mut log: Option<ResMut<crate::combat::CombatLog>>,
    mut debug: Option<ResMut<EnemyDebug>>,
    mut bs: ResMut<script::BossScript>,
    (mut ai_flags, mut ai_sounds, mut triggers): (ResMut<AiFlags>, ResMut<AiSounds>, ResMut<TriggerRegions>),
    mut statuses: Option<ResMut<crate::status::Statuses>>,
    lock: Option<Res<crate::view::LockOn>>,
) {
    let _prof = crate::perf::span("think");
    let dt = time.delta_secs();
    let (pent, pa, ptf, pp) = *player;
    // The afflictions' SpEffects are on their wearer too (HasSpecialEffectId sees every active
    // one): Wolf aged by the Fountainhead noble's breath (9620, 130000_battle.lua), poisoned...
    let worn: HashMap<Entity, Vec<i64>> = statuses.as_ref().map_or(HashMap::new(), |s| s.0.iter().map(|(e, st)| (*e, st.effects.iter().map(|a| a.id).collect())).collect());
    let afflictions = |ent: Entity| worn.get(&ent).cloned().unwrap_or_default();
    let regions = crate::enemy::map_regions();
    let mut event_flags = match &bs.run {
        Some(r) => r.flags_on(),
        None => ai_flags.0.iter().filter(|f| *f.1).map(|f| *f.0).collect(),
    };
    // Every NPC in the fight, for the ones on a hostile team (combat::teams_hostile).
    let npcs: Vec<Rival> = q
        .iter()
        .filter(|x| !x.1.disabled && !x.1.stand_in && !x.1.is_dead() && x.2.hp > 0.0 && !x.4.is_some_and(|p| p.parked()))
        .map(|(ent, _, a, tf, ..)| Rival {
            ent,
            team: crate::combat::team_of(&combat, a),
            pos: tf.translation,
            fwd: a.forward(),
            hp_rate: a.hp / a.hp_max.max(1.0),
            guard: a.state.contains("Guard"),
            sp: combat.data_of(a).sp_effects_at(&a.anim, a.t).iter().map(|(id, _)| *id).chain(afflictions(ent)).collect(),
            posture_left: a.posture_max - a.posture,
            posture_rate: (1.0 - a.posture / a.posture_max.max(1.0)).max(0.0),
        })
        .collect();
    // What a TARGET_EVENT reads of each character (Enemy::event_target), a dead one too (the
    // Headless Ape drops its partner branch at the partner's HP rate 0, 510010_battle.lua:30):
    // every SpEffect it wears, as an AI sees on itself.
    // Only the characters some AI targets (usually none), not every enemy every frame.
    let targeted: Vec<Entity> = q.iter().filter_map(|x| x.1.event_target).collect();
    let event_views: HashMap<Entity, crate::ai::EventView> = q
        .iter()
        .filter(|x| !x.1.disabled && !x.1.stand_in && targeted.contains(&x.0))
        .map(|(ent, e, a, tf, ..)| {
            let sp = crate::combat::worn_sp(a, &e.clash).into_iter().chain(combat.data_of(a).sp_effects_at(&a.anim, a.t).iter().map(|(id, _)| *id)).chain(afflictions(ent)).chain([e.ai_sp]).collect();
            let view = crate::ai::EventView {
                pos: tf.translation,
                fwd: a.forward(),
                hp_rate: (a.hp / a.hp_max.max(1.0)).max(0.0),
                sp_left: a.posture_max - a.posture,
                sp_rate: (1.0 - a.posture / a.posture_max.max(1.0)).max(0.0),
                sp,
                ..Default::default()
            };
            (ent, view)
        })
        .collect();
    // The AI team roles, from last frame's targets.
    brains.role_clock -= dt;
    if brains.role_clock <= 0.0 {
        brains.role_clock = TEAM_ORDER_INTERVAL;
        let members: Vec<_> = q
            .iter()
            .filter(|x| !x.1.disabled && !x.1.stand_in && !x.1.is_dead() && x.2.hp > 0.0 && !x.4.is_some_and(|p| p.parked()))
            .filter_map(|(ent, e, _, tf, ..)| {
                let key = e.team_target?;
                let at = match key {
                    None => ptf.translation,
                    Some(r) => npcs.iter().find(|n| n.ent == r)?.pos,
                };
                Some((key, tf.translation.distance(at), ent))
            })
            .collect();
        brains.roles = assign_team_roles(members);
    }
    // The AI sounds age (combat.rs resolve emits the attacks').
    triggers.0.retain_mut(|r| {
        r.life -= dt;
        r.life > 0.0
    });
    ai_sounds.0.retain_mut(|s| {
        s.life -= dt;
        s.life > 0.0
    });
    let alert: HashMap<Entity, (bool, Vec<i64>)> = q.iter().map(|(ent, e, ..)| (ent, (matches!(e.targeting.state, stealth::BATTLE | stealth::FIND), e.placement.clone()))).collect();
    for s in &mut ai_sounds.0 {
        if let Some(o) = s.owner {
            s.owner_alert = alert.get(&o).is_none_or(|a| a.0);
            s.owner_groups = alert.get(&o).map(|a| a.1.clone()).unwrap_or_default();
        }
    }
    // Wolf's learned skills' SpEffects (Covert A / B) and his items' timed ones (Gachiin's Sugar
    // 3441: sightSearchEnemyCut 50, aroundSightPointAddRate 0.5) for the sight and hearing checks.
    let mut skills = crate::combat::skill_sp_effect_rows(&combat, &config);
    skills.extend(pp.timed.iter().filter_map(|(id, _)| serde_json::from_value(crate::items::param("SpEffectParam", *id).clone()).ok()));
    // Crouched in a bush (the Construct's outpost, sandbox.rs in_bush): SpEffect 109203, of the
    // crouch family (109200 crouch, 109210 hug, 109220 hang) the grass one by its sight cut 80
    // (gap: nothing in the params puts 109201-109203 on; the engine's grass rule is not traced,
    // 80 picked over the 100 rows so a guard that walks right up still finds him).
    if crate::player::crouch_ok(&pa.state) && (crate::encounter::hooks().hidden)(ptf.translation - Vec3::Y * (crate::player::CAPSULE_HALF_HEIGHT + 0.05)) {
        skills.extend(serde_json::from_value(crate::items::param("SpEffectParam", 109203).clone()).ok());
    }
    for (entity, mut e, mut a, mut tf, phantom, from_event, scripted, mut draw_mask) in &mut q {
        // A phantom out of the fight does nothing until its boss's script calls it.
        if phantom.is_some_and(|p| p.parked()) || e.disabled || e.stand_in {
            continue;
        }
        // Its update level (Actor::lod): due ticks only, with their time.
        if !a.lod_due {
            continue;
        }
        let dt = dt * a.lod as f32;
        let d = combat.data_of(&a);
        let foe = combat.foe_of(&a);
        let npc = combat.npc(&a);
        // (Also before its script has started: the first frame.)
        let scripted = scripted || script::scripts().contains_key(&foe.npc_row);
        let turn_rate = npc["turnVellocity"].as_f64().unwrap_or(135.0) as f32;
        // An anim's TAE SpEffect (66 / 67) with effectEndurance -1 stays on for good: the exe never
        // removes a TAE effect at the window's end, -1 never counts down (FUN_140b56500 ->
        // FUN_140c02c50; FUN_140c03af0 / FUN_140c06b70). So Isshin's 3015 -> 200031 switches the
        // anim set (a100) and the headless Ape's rise keeps 275100 / 3510991 / 3510110. The
        // category rule (wear_for_good) overwrites: the Okami warriors' resident 200031 (a100,
        // their Act31 appearance 20001) gives way to 20001's 200030.
        if !a.anim.is_empty() {
            let lasting: Vec<i64> = d.sp_effects_at(&a.anim, a.t).iter().filter(|(_, s)| s.effect_endurance == -1.0).map(|(id, _)| *id).collect();
            for id in lasting {
                wear_for_good(d, &mut a, id);
            }
            if let Some(g) = anim_group_of(d, d.sp_effects_at(&a.anim, a.t).iter().map(|(id, _)| *id)) {
                a.anim_group = g;
            }
        }
        // Instant recoveries from the anim's TAE AddSpEffect (66 / 67): every ThrowDef reaction
        // adds 300610 "HP reset" (HPリセット: changeHpRate -100, effectEndurance 0; Genichiro
        // c7100 a000_012000 at frame 130, a000_013410 at 99), which is how a boss with deathblows
        // left is back at full vitality, at that frame (the dead ones are not touched).
        if !a.anim.is_empty() && !e.is_dead() && a.prev_t < a.t {
            let heal = instant_hp_rate(d, &a.anim, a.prev_t, a.t);
            if heal > 0.0 {
                a.hp = (a.hp + a.hp_max * heal / 100.0).min(a.hp_max);
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
                // 2 s): its death state. A real ThrowDefDeath clip still comes on the reaction's
                // flag 69 or its end, script or not (c9997 HKS ThrowDefCommonFunction: env(276) ->
                // W_ThrowDefDeath<id>): Emma (c7400, scripted) played all 9 s of ThrowDef12000 /
                // 12110, whose end is her getting up, before 12001 / 12111.
                if let Some(death) = e.throw_death.clone() {
                    let switch = death != "Event20200"
                        && !a.anim.is_empty()
                        && (a.t >= d.length(&a.anim) || d.flag(&a.anim, a.t, FLAG_THROW_TYPE5));
                    if t - dt <= 0.0 || switch {
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
                // The Puppeteer's puppet (ninjutsu.rs): its ThrowDef12220 played out with
                // NOT_TO_DEATH_ANIME (220030) worn, it gets up and fights on Wolf's side (team 15)
                // until that runs out (220022 empties its HP).
                if crate::ninjutsu::wears_ref(&a, crate::ninjutsu::REF_NOT_TO_DEATH) {
                    if a.anim.is_empty() || a.t >= d.length(&a.anim) {
                        a.hp = a.hp_max;
                        e.targeting.reset();
                        e.ninsatsu.0 = e.ninsatsu.0.max(1);
                        e.mode = Mode::Ai;
                        e.cooldown = 0.5;
                        e.interrupt();
                        if let Some(log) = log.as_mut() {
                            log.push("PUPPET", Color::srgb(0.9, 0.3, 0.3));
                        }
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
                    // hp_max already carries the area doping's max_hp_rate (spawn_one); the
                    // raw NpcParam hp brought a doped enemy back at a fraction of its bar.
                    crate::status::set_aging(&mut a, &combat, None);
                    a.hp = a.hp_max;
                    a.posture = 0.0;
                    e.ninsatsu.0 = e.ninsatsu.1;
                    e.ninsatsu_unhit = 0;
                    a.resident = e.resident0.clone();
                    a.super_armor.refill();
                    e.phase_done.clear();
                    e.phase_flags.clear();
                    e.event_req.clear();
                    e.phase_replan = false;
                    a.anim_group = anim_group_of(d, a.resident.clone().into_iter()).unwrap_or(0);
                    // Seen again: its idle's opacity as at spawn, not the death clip's TAE 193
                    // fade to 0 (c1180 ThrowDefDeath12001; Approach below has no anim to read it).
                    a.opacity = d.opacity_key(&format!("a{:03}_000000", a.anim_group * 100), 0.0).unwrap_or(1.0);
                    // Facing its spawn way, not the way it died (a deathblow from behind).
                    a.yaw = e.start_yaw;
                    // On the floor at its start, not at the height it died at (bodies_on_terrain
                    // only steps up STEP_HEIGHT: killed downhill it came back buried).
                    // The line-up comes back 2 m behind its spot along its start facing (the
                    // duel's faces +Z, the sandbox's past Wolf -Z); an MSB enemy (it started
                    // unaware on its map spot) at the spot itself, which stays its home.
                    let back = if e.unaware0 { 0.0 } else { 2.0 };
                    tf.translation = respawn_spot(e.start, e.start_yaw, back, crate::terrain::terrain().as_deref());
                    // Its new pose at once: no crossfade from the death pose, no deathblow ModelLag.
                    commands.entity(entity).insert(crate::anim::SnapPose);
                    // Its poison / burn and their flames stay with the old life.
                    if let Some(st) = statuses.as_mut() {
                        st.clear(entity);
                    }
                    e.mode = Mode::Ai;
                    e.cooldown = 2.0;
                    e.interrupt();
                    // A new life, a new AI (the exe's AI reset FUN_140602020): the old Brain kept
                    // its SetNumber values, timers and observes (c1010's Act08 never came back).
                    brains.map.remove(&entity);
                    // The death anim's TAE 233 hid the sword and sheath (Mask0-9 = 0) and a broken
                    // armour stays broken: both come back with the new life, as at spawn.
                    if let Some(dm) = draw_mask.as_mut() {
                        dm.clear();
                    }
                    e.armor = armor_of(combat.foe_of(&a).npc_row);
                    a.procedural("Approach");
                    // Its targeting starts over as at spawn: an MSB enemy unaware again (patrol,
                    // NonBattleAct, the stealth deathblows) under its map AI hold; the line-up
                    // fighting Wolf (spawn_one). The old life's BATTLE / forget time stayed.
                    if e.unaware0 {
                        e.make_unaware(&mut a);
                        if let Some(h) = e.hold0.clone() {
                            commands.entity(entity).insert(h);
                            e.ai_off = true;
                        }
                        if let Some(re) = e.route_end0.clone() {
                            commands.entity(entity).insert(re);
                        }
                    } else {
                        e.targeting = Targeting::new(0x2545_F491 ^ e.rng);
                        let think = e.think_id(foe);
                        e.targeting.damaged(combat.param("NpcThinkParam", think));
                        e.ai_sp = 200004;
                    }
                    // Home is its start pose, not a home its last patrol moved (Patrol::new_home)
                    // nor where and how it died.
                    e.home = Some((tf.translation, e.start_yaw));
                    e.hurt_by_wolf = false;
                    e.hp_last = None;
                    // The old life's timed SpEffects (its reaction cool times, prosthetic.rs
                    // add_reaction_cool_time) and its guard count stay with it.
                    e.clash.clear();
                    e.guard_count = 0;
                    e.guard_count_timer = 0.0;
                } else {
                    e.mode = Mode::Dead(t - dt);
                }
                continue;
            }
            Mode::Rising(t) => {
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
                    // The collapse holds the meter empty (TAE 960 type 5) and sets it to 30 % left
                    // as it rises (type 6, actor::hold_posture_range); the rise's range may start
                    // a frame after the window ends, so it is taken now. A meter still held empty
                    // keeps one point: the exe breaks it again only on a hit (b6e6a0).
                    if a.posture_broken() {
                        let rise = d.anim(&a.anim).and_then(|an| an.events.iter().filter(|e| e.kind == 960 && e.start >= a.t - 0.1).min_by(|x, y| x.start.total_cmp(&y.start)).and_then(|e| e.arg_i64("StaminaRatioType")));
                        if let Some(ty) = rise {
                            crate::actor::hold_posture_range(&mut a, ty);
                        }
                        if a.posture_broken() {
                            a.posture = a.posture_max - 1.0;
                        }
                    }
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
        let think_id = e.think_id(foe);
        let think_row = combat.param("NpcThinkParam", think_id);
        let (home, home_yaw) = *e.home.get_or_insert((tf.translation, a.yaw));
        let mut seen = wolf_seen(&combat, pa, ptf.translation, &skills);
        // What Wolf's bullets and throws put on it: the Bloodsmoke's 220014 (sightSearchRate 0,
        // hearingSearchEnemyCut 100) blinds and deafens it in the smoke, the Puppeteer's 220021
        // doubles its sight. A charmed puppet (team 15, ninjutsu.rs) does not hunt Wolf at all.
        // gap: the observer rates' place in the exe's sight test is not traced (they scale the
        // sight distances as Wolf's cut does); the puppet's 360 deg eye angles are not applied.
        let (sight_rate, ear_rate) = put_on_rates(&e, &a);
        let charmed = crate::combat::team_of(&combat, &a) == crate::combat::TEAM_CHARMED;
        seen.cut = if charmed { 0.0 } else { seen.cut * sight_rate };
        let feet = tf.translation - Vec3::Y * (crate::player::CAPSULE_HALF_HEIGHT + 0.05);
        e.targeting.sound_new = false;
        // The listener's sound pass runs only while the targeting list holds no character target
        // (exe FUN_140624b70 14062.c:4204 -> FUN_1406296e0: no entry of type 1, the chr-handle
        // target of FUN_140629430): an enemy that has Wolf takes no new sounds, so Wolf's own
        // footsteps and swings in a fight raise no ChangeSoundTarget (46), whose common handler
        // (common_table_ai_common.lua:247-267) replans and dropped the guard Goal.Parry had just
        // picked. (Inferred: the normal target as the type-1 entry.)
        let listening = e.targeting.normal.is_none();
        let listening_wolf = listening && !charmed && ear_rate > 0.0;
        for (id, rank) in wolf_sounds(&combat, pa, ptf.translation, &a, tf.translation, think_row, &skills).into_iter().filter(|_| listening_wolf) {
            e.targeting.hear(think_row, seen.pos, id, rank);
        }
        let my_team = crate::combat::team_of(&combat, &a);
        for s in ai_sounds.0.iter().filter(|_| listening) {
            // A comrade's spot call is for those not yet fighting: one already in battle keeps its
            // fight (a soldier at a maddened dog's throat, test a_maddened_dog_bites_the_soldier).
            if s.friends_only && e.targeting.state >= stealth::FIND {
                continue;
            }
            if let Some((id, rank)) = ai_sound_heard(&combat, s, entity, my_team, &a, tf.translation, think_row, &e.placement) {
                e.targeting.hear_record(think_row, s.pos, id, rank, s.serial);
            }
        }
        let was = e.targeting.state;
        e.targeting.update(think_row, feet, a.forward(), &seen, dt);
        // Spotting Wolf it calls its comrades: AiSoundParam 2000000 (bCallFriend, 20 m, friends
        // and foes; AtkParam 1500's AppearAiSoundId, the shout no c1010 / c1020 clip plays), heard
        // by those of its side in range while it is alert (ai_sound_heard). The user: "enemies
        // should hear their comrades spotting". gap: what in the game makes a finder call is not
        // traced (no TAE 237 on c1010; its NpcThinkParam callHelp_* are 0).
        if was < stealth::FIND && e.targeting.state >= stealth::FIND {
            ai_sounds.call(&combat, SPOT_CALL_SOUND, tf.translation, entity, my_team);
        }
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
        // The opponent for the root-motion boost (actor.rs) and the body twist (anim.rs).
        a.homing = rival.map(|r| r.ent);
        e.team_target = match rival {
            Some(r) => Some(Some(r.ent)),
            None if e.targeting.state == stealth::BATTLE => Some(None),
            None => None,
        };
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
        let mut ended = !a.anim.is_empty() && a.t >= len;
        // An event state as the chr's own HKS runs it (data::hks_events, Event<n>_onUpdate): at the
        // clip's end (env 339, before a held pose's TAE runs out) it goes on to the next event
        // (the zombie c1500's held 20010 -> 21015, c1020 20002 -> 21001), back to the idle
        // (W_Idle), into the death pose (ExecTransToDeathIdle: c1020's sting death 20005), or,
        // with no end for the refs it wears, holds in its loop clip (c1020 21000, the Divine
        // Dragon's trees c5310 21000-21002, whose open blooms m25 12505906 reads) until something
        // else takes it. Some fire before the end: with no talk running (c1020 21007 -> 21005,
        // c1070 21000 -> 20001) or once on the ground (c1100 21005 -> 20005), the Divine Dragon's
        // collapse 20002 -> 21000 at its end. EventCommonFunction's ExecActiveTransition runs with
        // is_on_cancel TRUE: the AI acts only through the clip's cancel windows (env 107 / 120 /
        // 118) and turns or moves at flag 78 (env 2000), as in any other clip.
        // Without the chr's decompiled HKS the clip's end goes back to the AI.
        let ev = a.state.strip_prefix("Ez").or(a.state.strip_prefix("Event")).and_then(|n| n.parse::<i64>().ok());
        let hks = ev.map(|_| crate::data::hks_events(&combat.foe_of(&a).chr));
        let in_event = ev.zip(hks.as_ref()).and_then(|(n, h)| h.states.contains_key(&n).then_some(n));
        if let (Some(from), Some(h)) = (in_event, hks.as_ref()) {
            let at_end = !a.anim.is_empty() && a.t >= d.length(&a.anim);
            let refs: Vec<i64> = crate::combat::worn_sp(&a, &e.clash)
                .into_iter()
                .chain(d.sp_effects_at(&a.anim, a.t).iter().map(|(id, _)| *id))
                .chain([e.ai_sp])
                .chain((a.anim_group > 0).then_some(200030 + a.anim_group as i64))
                .map(|id| d.sp_effects.get(&id.to_string()).map_or_else(|| crate::data::sp_effect_behavior_ref(id), |s| s.behavior_ref_id))
                .collect();
            match h.next(from, &refs, !a.airborne, at_end) {
                Some(crate::data::EndTo::Event(id)) => {
                    let k = a.grouped(&format!("a000_{id:06}"));
                    if d.anim(&k).is_some() {
                        a.play(&format!("Event{id}"), &k);
                        continue;
                    }
                }
                Some(crate::data::EndTo::Idle) => ended = true,
                Some(crate::data::EndTo::DeathIdle) => {
                    // Down in the clip's last pose, out of the AI (gap: DeathIdle's ragdoll).
                    a.move_vel = Vec3::ZERO;
                    e.cur_ez = None;
                    e.mode = Mode::Held(false);
                    continue;
                }
                None if at_end => {
                    let (state, key) = (a.state.clone(), a.anim.clone());
                    a.play(&state, &key);
                    ended = false;
                }
                None => {}
            }
        }
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
        // Perilous warning when the attack's TAE fires it: the combat log line. The HUD kanji is
        // its warning bullet's hit on Wolf (enemy_bullet.rs fly).
        if d.perilous_warning(&a.anim, a.prev_t, a.t).is_some() {
            if let Some(log) = log.as_mut() {
                let what = match perilous_kind(d, &a.anim) {
                    "sweep" => "sweep - jump",
                    "thrust" => "thrust - deflect or mikiri (step in)",
                    "grab" => "grab - dodge",
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
                // Range from his body's edge: a big one (Demon of Hatred, NpcParam hitRadius
                // 2.8) is held off by the body push-out before the centre distance reaches the
                // range, and walked on forever (VFX's c7020 drill).
                let body = (npc["hitRadius"].as_f64().unwrap_or(0.5) as f32 - 0.5).max(0.0);
                drill(&mut e, &mut a, d, &config, dbg, dist - body, want_yaw, dt, free, ended);
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
                Ok(mut b) => {
                    if let Ok(route) = routes.get(entity) {
                        b.set_patrol(route.0.clone());
                    }
                    for l in b.log.iter().filter(|l| !l.starts_with("stub:")) {
                        warn_once(l);
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
        // A move that has played out no longer runs its TAE, also an AddSpEffect that runs past
        // the clip's end: Lady Butterfly's drop a100_003290 (3.0 s) holds 5028 to 3.2 s, and
        // 509000_battle.lua:59 picks the drop again (Act38) while she wears 5028, so the next
        // plan must not see it (she dropped over and over, 2026-10-17). 5028 has effectEndurance 0.
        if !(a.state.starts_with("Ez") && a.t >= d.length(&a.anim)) {
            sp_self.extend(d.sp_effects_at(&a.anim, a.t).iter().map(|(id, _)| *id));
        }
        sp_self.extend(e.clash.iter().map(|c| c.0));
        // Its TAE SpEffects kept past their window and their replaceSpEffectId chains (actor.rs
        // run_sp_effect_chains), as Wolf's below.
        sp_self.extend(a.sp_chained.iter().map(|c| c.0));
        sp_self.insert(e.ai_sp);
        // The anim-set SpEffect lasts (effectEndurance -1): Corrupted Monk / Isshin read 200031 as
        // their second phase ("IsHU2", 500000_battle.lua).
        if a.anim_group > 0 {
            sp_self.insert(200030 + a.anim_group as i64);
        }
        let sp_target: HashSet<i64> = match rival {
            Some(r) => r.sp.iter().copied().collect(),
            // With his chained ones: the revival's a000_110030 TAE 401 110031 (0.3 s) -> 110032
            // (2.5 s) -> 110033 (0.5 s), whose 110032 common_table_ai_common.lua:178-200 answers
            // (EndureAttack 1040 / KankyakuAct). Only the TAE-window 110031 was seen (round 30).
            // And his timed rows (Player::timed: the Owl's recovery seal 3506090 from his 3031
            // "Divine Abduction" bullet, which 506001_battle.lua re-weighs on
            // HasSpecialEffectId(TARGET_ENE_0, 3506090); a warning's 211000) and the lightning
            // charge on him (Actor::electro, 9490-9495: the Way of Tomoe's 9492 stateInfo 356).
            // Before, the AI never saw them and the Owl kept re-sealing (boss bug hunt 2026-10-14).
            None => combat
                .player
                .sp_effects_at(&pa.anim, pa.t)
                .iter()
                .map(|(id, _)| *id)
                .chain(pa.sp_chained.iter().map(|c| c.0))
                .chain(afflictions(pent))
                .chain(pp.timed.iter().map(|t| t.0))
                .chain(pa.electro.map(|e| e.0))
                .collect(),
        };
        let sp_new: Vec<i64> = sp_self.iter().chain(sp_target.iter()).filter(|id| !e.prev_sp.contains(*id)).copied().collect();
        let sp_gone: Vec<i64> = e.prev_sp.iter().filter(|id| !sp_self.contains(*id) && !sp_target.contains(*id)).copied().collect();
        e.prev_sp = sp_self.iter().chain(sp_target.iter()).copied().collect();
        // (In any anim set: a100_003000 is EzState 3000 too.)
        let playing_ez = e.cur_ez.filter(|ez| ez_of(&a.anim) == Some(*ez));
        let event = e.event_target.and_then(|t| event_views.get(&t)).map(|v| crate::ai::EventView {
            sp_new: v.sp.iter().filter(|id| !e.event_prev_sp.contains(*id)).copied().collect(),
            sp_gone: e.event_prev_sp.iter().filter(|id| !v.sp.contains(*id)).copied().collect(),
            ..v.clone()
        });
        e.event_prev_sp = event.as_ref().map(|v| v.sp.clone()).unwrap_or_default();
        let tg = rival_view(&e.targeting, rival.map(|r| r.ent), e.rival_last);
        e.rival_last = rival.map(|r| r.ent);
        let snap = Snapshot {
            // 3D, as the exe's GetDist (FUN_1405f1540: sqrt of dx^2 + dy^2 + dz^2 between the two
            // characters' positions); angles stay horizontal.
            dist: (t_pos - tf.translation).length(),
            angle: dir.dot(right).atan2(dir.dot(fwd)).to_degrees(),
            bearing: (-to_t.x).atan2(-to_t.z).to_degrees(),
            target_guard: rival.map_or(pa.state.contains("Guard"), |r| r.guard),
            self_guard: stance_of(&a.state) == Some(Stance::Guard),
            sp_self,
            sp_target,
            sp_new,
            sp_gone,
            hp_rate: a.hp / a.hp_max,
            target_hp_rate: rival.map_or(pa.hp / pa.hp_max, |r| r.hp_rate),
            sp: a.posture_max - a.posture,
            sp_rate: (1.0 - a.posture / a.posture_max).max(0.0),
            target_sp: rival.map_or(pa.posture_max - pa.posture, |r| r.posture_left),
            target_sp_rate: rival.map_or((1.0 - pa.posture / pa.posture_max.max(1.0)).max(0.0), |r| r.posture_rate),
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
            target_use_item: rival.is_none() && wolf_starts_an_item(pa),
            me: tf.translation,
            fwd,
            target: t_pos,
            // A caution spot has no facing (zero).
            target_fwd: match rival {
                Some(r) => r.fwd,
                None if t_pos == ptf.translation => pa.forward(),
                None => Vec3::ZERO,
            },
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
            sound_new: tg.sound_new,
            t_replan: tg.replan || std::mem::take(&mut e.phase_replan),
            event_req: e.event_req.clone(),
            event_req_new: std::mem::take(&mut e.event_req_new),
            // The dummies its observe slots read (AddObserveChrDmySphere / Area): world position
            // and +Z axis of each marker.
            dummies: brains
                .map
                .get(&entity)
                .map(|b| b.observe_dmys())
                .unwrap_or_default()
                .into_iter()
                .filter_map(|id| {
                    let m = dummies.get(entity).ok().and_then(|d| crate::combat::dmy_entity(d, id as i64))?;
                    let g = markers.get(m).ok()?;
                    Some((id, g.translation(), g.rotation() * Vec3::Z))
                })
                .collect(),
            event_flags: event_flags.clone(),
            ninsatsu: e.ninsatsu_left(),
            ninsatsu_max: e.ninsatsu.1,
            locked_on: rival.is_none() && lock.as_ref().is_some_and(|l| l.target == Some(entity)),
            team_role: brains.roles.get(&entity).copied().unwrap_or(-1.0),
            regions: regions.clone(),
            me_id: entity.to_bits(),
            sound_serial: tg.sound.map_or(-1, |s| s.serial),
            triggers: triggers.0.clone(),
            event,
        };
        let brain = brains.map.get_mut(&entity).unwrap();
        let cmd = brain.tick(&snap, dt);
        // Its new trigger regions are there for the AIs after it this frame (a repeat key
        // replaces the old region, FUN_140643380).
        for r in &cmd.triggers {
            triggers.0.retain(|x| (x.owner, x.cat, x.unit) != (r.owner, r.cat, r.unit));
            triggers.0.push(r.clone());
        }
        // A finished type 5 patrol route moved its home (ai.rs Patrol::new_home).
        if let Some(h) = brain.patrol_home() {
            e.home = Some(h);
        }
        for &(id, on) in &cmd.flags {
            match bs.run.as_mut() {
                Some(r) => r.set_flag(id, on),
                None => {
                    ai_flags.0.insert(id, on);
                }
            }
            event_flags.retain(|f| *f != id);
            if on {
                event_flags.push(id);
            }
        }
        if script::trace() && scripted {
            eprintln!("brain {}: anim {:?} mv {:?} at {:?} face {} free {} ez {:?} state {}", foe.npc_row, cmd.anim, cmd.mv, cmd.at, cmd.face, free, snap.ez, a.state);
        }
        for l in brain.log.drain(..) {
            if !l.starts_with("stub:") {
                warn_once(&l);
            }
        }
        e.ai_desc = brain.describe();
        e.ai_cmd = cmd.clone();
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
        // A turn clip does not chain into another turn: c9997 TurnCommonFunction passes
        // enable_turn FALSE, so its end goes to idle (IdleTransition) and the next turn comes from
        // the idle a frame later.
        let turn_ok = move_ok && !(ended && a.state.starts_with("Turn"));
        let mut turned_first = false;
        let cur = stance_of(&a.state);
        let want = Stance::wanted(&cmd).filter(|s| d.anim_key(s.idle()).is_some());
        let ending = a.state == "GuardEnd" || a.state.starts_with("GeneralStanceEnd");
        // Another AI action from the general stance ends it first (c9997.lua:1353-1355; the
        // stance's own start / attack / end ids excepted); from the guard it plays at once.
        let leaves_general = cur == Some(Stance::General)
            && cmd.anim.is_some_and(|ez| !matches!(ez, 401060..=401064 | 601040..=601044 | 603000..=603009));
        if (cmd.anim.is_none() || leaves_general) && (cur.is_some() || (want.is_some() && free && !ending)) {
            guard_stance(&mut a, d, &cmd, ended, move_ok);
            e.cur_ez = None;
        } else if let Some(ez) = cmd.anim {
            // In its anim set, or the set that has the clip (the ground-grab zombie's 20011 is
            // a100 only: Actor::grouped).
            let key = a.grouped(&format!("a000_{ez:06}"));
            let general_state = general_stance_play(&a.state, ended, ez);
            let starting = general_state.as_deref().is_some_and(|s| !s.starts_with("GeneralStanceAttack"));
            if let Some(state) = general_state.filter(|s| d.anim_key(s).is_some()) {
                if !(starting && settling(&a.state) && !ended) && a.state != state {
                    a.play_state(d, &state);
                    a.move_vel = Vec3::ZERO;
                    if !starting {
                        e.cur_ez = Some(ez);
                        e.ez_started = Some(ez);
                    }
                }
            } else if d.anim(&key).is_some() {
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
            // From idle the turn comes before the move: c9997 ExecActiveTransition tries ExecTurn
            // (enable_turn TRUE) before the move request (env 3014), and the exe's Approach
            // (FUN_1405caa00, 1405c.c) makes the goal's turn target (param 3; -1 = the move
            // point, FUN_14060c7d0) the TurnTo target, so a point 45 deg or more off plays the
            // turn clip first and the run starts from its AI flag 78. (A runner that only
            // steered at turnVellocity 135 deg/s circled a patrol point behind it for ever.)
            if cmd.face && turn_ok && start_turn(&e, &mut a, d, goal_yaw, cmd.emergency) {
                turned_first = true;
            } else {
                let state = loco_state(d, e.ai_sp, mv, cmd.walk);
                if a.state != state || ended {
                    a.play_state(d, &state);
                }
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
        // A turn too from the clip's move window (c9997 ExecTurn on a cancel: env 2000, as a move):
        // a broken soldier's TrunkCollapse turned to Wolf behind it at its flag 78, not its end.
        let turned = turned_first || (cmd.turn && cmd.anim.is_none() && cmd.mv.is_none() && turn_ok && start_turn(&e, &mut a, d, goal_yaw, cmd.emergency));
        if (cmd.face || cmd.turn) && (free || cmd.mv.is_some()) && !turned && !(a.state.starts_with("Turn") && !ended) {
            a.yaw = turn_toward(a.yaw, goal_yaw, turn_rate.to_radians() * dt);
        }
    }
}

/// The two held stances (HKS styles STYLE_GUARD / STYLE_GENERAL_STANCE).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Stance {
    Guard,
    General,
}

impl Stance {
    /// The stance the AI asks for this frame (the general one first: ExecAIAction tries
    /// ExecAIGeneralStance before ExecAIGuard).
    fn wanted(cmd: &crate::ai::Command) -> Option<Stance> {
        if cmd.general {
            Some(Stance::General)
        } else if cmd.guard {
            Some(Stance::Guard)
        } else {
            None
        }
    }
    fn start(self) -> &'static str {
        match self {
            Stance::Guard => "GuardStart",
            Stance::General => "GeneralStanceStart401060",
        }
    }
    fn idle(self) -> &'static str {
        match self {
            Stance::Guard => "GuardIdle",
            Stance::General => "GeneralStanceIdle",
        }
    }
    fn end(self) -> &'static str {
        match self {
            Stance::Guard => "GuardEnd",
            Stance::General => "GeneralStanceEnd601040",
        }
    }
    /// The move state (move_table_guard / move_table_general_stance, c9997.lua:697; ExecMove).
    fn walk(self, mv: char, walk: bool) -> String {
        let dir = match mv {
            'B' => "Back",
            'L' => "Left",
            'R' => "Right",
            _ => "Front",
        };
        match self {
            Stance::Guard => format!("GuardWalk{dir}"),
            Stance::General if mv == 'F' && !walk => "GeneralStanceRunFront".into(),
            Stance::General => format!("GeneralStanceWalk{dir}"),
        }
    }
}

/// The stance a state belongs to, and whether it is one that plays out first (a start or a
/// turn; the general stance's own starts 401060-401064 also come as AI action anims).
fn stance_of(state: &str) -> Option<Stance> {
    if ["GuardStart", "GuardIdle", "GuardWalk", "TurnGuard_"].iter().any(|s| state.starts_with(s)) {
        Some(Stance::Guard)
    } else if ["GeneralStanceStart", "GeneralStanceIdle", "GeneralStanceWalk", "GeneralStanceRun", "GeneralStanceTrun_", "GeneralStanceAttack", "Ez40106"].iter().any(|s| state.starts_with(s)) {
        Some(Stance::General)
    } else {
        None
    }
}

/// c9997 ExecAIGeneralStance (c9997.lua:1321-1350) for an AI action `ez`: the stance's own
/// attacks / ends (603000-603009 / 601040-601044) out of the stance start it first
/// (W_GeneralStanceStart401060; the goal keeps asking), in it they play as
/// W_GeneralStanceAttack<id> / End<id>, which go back to the stance idle (GeneralStanceAttack*
/// _onUpdate: env(339) -> W_GeneralStanceIdle); a start (401060-401064) in the stance ends it.
/// A start, turn or attack plays out first. None: not a general stance action.
pub(crate) fn general_stance_play(state: &str, ended: bool, ez: i64) -> Option<String> {
    let in_general = stance_of(state) == Some(Stance::General) && (!settling(state) || ended);
    match ez {
        603000..=603009 if in_general => Some(format!("GeneralStanceAttack{ez}")),
        601040..=601044 if in_general => Some(format!("GeneralStanceEnd{ez}")),
        603000..=603009 | 601040..=601044 => Some("GeneralStanceStart401060".to_string()),
        401060..=401064 if in_general => Some("GeneralStanceEnd601040".to_string()),
        _ => None,
    }
}

fn settling(state: &str) -> bool {
    ["GuardStart", "TurnGuard_", "GeneralStanceStart", "GeneralStanceTrun_", "GeneralStanceAttack", "Ez40106"].iter().any(|s| state.starts_with(s))
}

/// The guard stance (c9997 ExecAIGuard / ExecAIAction, hks_src/c9997.lua:1110-1320, 10524-10556):
/// the guard goal's request 9910 (ACTION_TYPE_GUARD) from idle fires W_GuardStart, which ends
/// in W_GuardIdle (a000_500000 loop); in guard style a move request walks with the guard up
/// (move_table_guard: GuardWalkFront / Back / Left / Right, a000_50500x), a turn is
/// W_TurnGuard_* (start_turn), another AI action (an attack) plays from it as from idle, and
/// when the request stops (env(106) <= 0 in STYLE_GUARD) W_GuardEnd (a000_501040) goes back to
/// idle. Every guard clip holds ChrActionFlag 3 ShieldBlock, so the stance blocks (combat.rs).
/// The general stance (9920, ACTION_TYPE_GENERAL_STANCE; ExecAIGeneralStance c9997.lua:1321-1375,
/// ExecMove 1609-1632, GeneralStance*_onUpdate 10800-10890) runs the same way: W_GeneralStanceStart401060
/// -> W_GeneralStanceIdle (a000_600000), W_GeneralStanceWalk* / RunFront (605000 / 605010),
/// W_GeneralStanceTrun_* turns (60540x), W_GeneralStanceEnd601040 when the request stops or
/// another action comes; asking for one stance in the other ends the other first (1361-1363).
/// gaps: W_GuardRun (no c1010 / c1020 clip; a run asks for the front walk here), the start
/// clips' cancel windows (TransStyleCommonFunction / AttackCommonFunction) and the general
/// stance's own attacks (603000-603009, no c1010 state).
pub(crate) fn guard_stance(a: &mut Actor, d: &crate::data::CharData, cmd: &crate::ai::Command, ended: bool, move_ok: bool) {
    let st = a.state.clone();
    let cur = stance_of(&st);
    let want = Stance::wanted(cmd).filter(|s| d.anim_key(s.idle()).is_some());
    let leaving = cmd.anim.is_some() || want != cur;
    if let (Some(c), true) = (cur, leaving) {
        // The request gone, or another stance / action asked for: this one's end clip.
        if !settling(&st) || ended {
            if !a.play_state(d, c.end()) {
                a.procedural("IdleBattle");
            }
            a.move_vel = Vec3::ZERO;
        }
        return;
    }
    let Some(s) = want else { return };
    if cur.is_none() {
        a.play_state(d, s.start());
        a.move_vel = Vec3::ZERO;
        return;
    }
    if settling(&st) {
        if ended {
            a.play_state(d, s.idle());
            a.move_vel = Vec3::ZERO;
        }
        return;
    }
    if let (Some(mv), true) = (cmd.mv, move_ok) {
        let state = s.walk(mv, cmd.walk);
        if (st != state || ended) && a.play_state(d, &state) {
            return;
        }
        if st == state {
            return;
        }
    }
    if st != s.idle() || ended {
        a.play_state(d, s.idle());
        a.move_vel = Vec3::ZERO;
    }
}

/// Turning in place (c9997 ExecTurn, hks_src/c9997.lua:1488-1563): an AI turn request (TurnTo)
/// from idle of 45 degrees or more (any angle with RequestEmergencyQuickTurn, env(330)) plays
/// W_Turn<AI state>_<Left|Right><90|180>: under 135 the 90 clip, else the 180 one; not under
/// SpEffect ref 1000072 SP_EFFECT_REF_NO_QUICK_TURN, nor from a turn or a walk / run (their
/// TurnCommonFunction / move states pass enable_turn FALSE; idle, attack, step and damage ends
/// pass TRUE). The clip's root motion turns it (no
/// SetTurnSpeed on them); its TAE's AI flags 23 / 86 / 79 / 78 open the AI's next action from
/// 0.4 s (90) / 0.6 s (180). The AI states pick Default / CautionNoBattle / CautionBattle /
/// Battle (IndexAiState, ai_suffix); a state without the clip (c1010 has no 2054xx) turns as
/// before (turnVellocity). The clip's turn is scaled to the asked angle: act(9105, 90 / 180)
/// (FUN_140b32410 case 0x2391, 140b3.c:8728 -> FUN_1407de170) and act(9104) (the corrected-turn
/// flag) make FUN_1407db220 set the root-motion rate to |error| / 90 or 180. TurnAngle is the
/// AI's desired facing (AI +0x73e4 from the TurnTo target +0x9050, FUN_14060c7d0) minus the
/// chr's yaw, in degrees, right positive (FUN_140b4c600, 140b4.c:4531). gaps: the leftover error's
/// 720 deg/s correction (FUN_1407dab40) and the 0xd branch (1407d.c:8046); the General stance
/// turn and the flight / crouch Default rule are not run.
pub(crate) fn start_turn(e: &Enemy, a: &mut Actor, d: &crate::data::CharData, goal_yaw: f32, emergency: bool) -> bool {
    if ["Turn", "Walk", "Run", "Guard", "GeneralStance", "Ez40106"].iter().any(|s| a.state.starts_with(s)) && !a.state.starts_with("GuardIdle") && !a.state.starts_with("GeneralStanceIdle") {
        return false;
    }
    // In the guard stance (GuardIdle): W_TurnGuard_* (ExecTurn, action type ACTION_TYPE_GUARD);
    // in the general stance W_GeneralStanceTrun_* (sic).
    let guard = a.state.starts_with("GuardIdle");
    let general = a.state.starts_with("GeneralStanceIdle");
    let no_turn = a.resident.iter().chain(e.clash.iter().map(|c| &c.0)).any(|id| d.sp_effects.get(&id.to_string()).is_some_and(|s| s.behavior_ref_id == 1000072))
        || (!a.anim.is_empty() && d.sp_effects_at(&a.anim, a.t).iter().any(|(_, s)| s.behavior_ref_id == 1000072));
    // TurnAngle: right is positive; Bevy's yaw grows to the left.
    let angle = -(goal_yaw - a.yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) + std::f32::consts::PI;
    let deg = angle.to_degrees();
    if no_turn || deg.abs() <= 0.0 || (deg.abs() < 45.0 && !emergency) {
        return false;
    }
    let (side, clip) = (if deg < 0.0 { "Left" } else { "Right" }, if deg.abs() < 135.0 { 90.0 } else { 180.0 });
    let state = if guard {
        format!("TurnGuard_{side}{clip}")
    } else if general {
        format!("GeneralStanceTrun_{side}{clip}")
    } else {
        format!("Turn{}_{side}{clip}", ai_suffix(e.ai_sp))
    };
    if !d.anim_key(&state).is_some_and(|k| d.anim(&a.grouped(k)).is_some()) || !a.play_state(d, &state) {
        return false;
    }
    // act(9105, 90 / 180) -> FUN_1407de170: BehaviorTurn +0x30 = the clip's angle; its first
    // frame FUN_1407db220 (1407d.c:8110-8127) sets the root-motion rate (motion +0x904 / +0x90c)
    // to |yaw error| / that angle.
    a.yaw_scale = deg.abs() / clip;
    a.move_vel = Vec3::ZERO;
    true
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
    // A new drill mode starts at once.
    if e.drill_timer.0 != dbg.mode {
        e.drill_timer = (dbg.mode, 0.0);
    }
    e.drill_timer.1 -= dt;
    if e.drill_timer.1 > 0.0 {
        return;
    }
    let key = if dbg.mode == AiMode::Repeat && d.anim(&dbg.attack).is_some() {
        dbg.attack.clone()
    } else {
        // Mikiri practice: the perilous thrusts combat.rs turns into a Mikiri (mikiri_atk:
        // disableGuard + atkType 2 or trackType 6), under a perilous warning.
        let thrust_only = dbg.mode == AiMode::Thrust;
        let is_thrust = |k: &str| d.attack_windows(k).iter().any(|(_, atk, _)| crate::combat::mikiri_atk(atk));
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
    e.drill_timer.1 = dbg.interval;
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
    // On the Construct a passive enemy holds its spot (the user: "static unless AI toggle button
    // is pressed"); elsewhere it walks into range for deflect practice.
    let hold = (crate::encounter::hooks().hold)();
    if dist > config.enemy.attack_range && !hold {
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

fn toggle_aggression(keys: Res<ButtonInput<KeyCode>>, mut q: Query<&mut Enemy, Without<crate::encounter::PatrolGuard>>, mut log: ResMut<crate::combat::CombatLog>) {
    if keys.just_pressed(KeyCode::KeyT) {
        // All enemies switch together (the first one's state flipped).
        let Some(on) = q.iter().next().map(|e| !e.aggressive) else { return };
        for mut e in &mut q {
            e.aggressive = on;
        }
        log.push(if on { "enemy: AGGRESSIVE (real AI)" } else { "enemy: passive" }, Color::WHITE);
    }
}
