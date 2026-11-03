//! Wolf against one enemy: both characters, the enemy's brain, lock-on, hits with the engine's
//! HP and posture rules, deathblows, and a respawn after a death.

use crate::ai::SimpleBrain;
use crate::arena::BossEvents;
use crate::character::{Character, StepReport};
use crate::combat::knockback::{HitStop, Knockback};
use crate::deathblow::{
    AbsorbStep, Absorbed, Deathblow, DummyFrame, Phase, Placement, Situation, ThrowTable,
    ThrowTimes, deathblow_marks, select,
};
use crate::enemy::EnemySpec;
use crate::events::{EventCommand, EventHost, EventScripts, PLAYER_ENTITY};
use crate::fight::CombatRules;
use crate::grab::{EnemyThrow, GrabPhase};
use crate::hits::{Combatant, HitDetector, HitEvent, HitResult};
use crate::input::InputFrame;
use crate::lua_ai::{AiActor, AiBrain, AiWorld, ThinkParams, anim_id_from_clip};
use crate::player::LoadError;
use sekiro_formats::flver::{self, Flver};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[path = "butterfly_illusions.rs"]
pub mod butterfly_illusions;
#[path = "dragon_finish.rs"]
mod dragon_finish;
#[path = "dragon_trees.rs"]
mod dragon_trees;
#[path = "father_bird.rs"]
pub mod father_bird;
#[path = "headless_ape.rs"]
pub mod headless_ape;
pub mod idol;
#[path = "monk_phantoms.rs"]
pub mod monk_phantoms;
#[path = "old_dragons.rs"]
pub mod old_dragons;
pub mod pickup;
pub mod population;
#[path = "screen_monkeys.rs"]
pub mod screen_monkeys;

/// Seconds between a death and both characters respawning at their start positions (a stand-in
/// for the game's death screen and reload).
pub const RESPAWN_DELAY: f32 = 6.0;

/// Seed for the AI's random numbers (fixed, so runs repeat).
const AI_SEED: u64 = 7;

#[derive(serde::Deserialize)]
struct FallMaterial {
    id: u32,
    #[serde(rename = "fallDeathDist")]
    fall_death_dist: u32,
    #[serde(rename = "disableFallDamage")]
    disable_fall_damage: u8,
}

fn load_fall_materials(cache: &Path) -> std::collections::HashMap<u32, FallMaterial> {
    #[derive(serde::Deserialize)]
    struct Materials {
        rows: Vec<FallMaterial>,
    }
    std::fs::read(cache.join("params/HitMtrlParam.json"))
        .ok()
        .and_then(|data| serde_json::from_slice::<Materials>(&data).ok())
        .map(|table| table.rows.into_iter().map(|row| (row.id, row)).collect())
        .unwrap_or_default()
}

/// TAE action flag on the player's attacks that tells the enemy AI to parry
/// (`INTERUPT_ParryTiming`), traced by FwLo's RustSekiro (`docs/kb/guard-deflect.md`).
pub const FLAG_PARRY_TIMING: i32 = 63;

/// Animation offset types that pick a throw clip by its ThrowParam offset: the player graph's
/// throw CMSGs (`a000_5xxxxx`, candidates `a200_` to `a249_`) use type 17 for `atkAnimOffset`
/// (200 for the soldier, 221 for the Owl), the NPC graph's `ThrowDef*` CMSGs type 15 for
/// `defAnimOffset`. The mapping is inferred from the graphs' candidate lists.
const THROW_ATK_OFFSET_TYPE: i64 = 17;
const THROW_DEF_OFFSET_TYPE: i64 = 15;

/// TAE action flags of NPC attacks that accept the next combo attack: 23 "AI combo attack
/// queued" and 86 "AI attack queued".
const COMBO_FLAGS: [i32; 2] = [23, 86];

/// SpEffect `stateInfo` 350: the character cannot be locked on to (SpEffect 200602 "cannot be
/// locked, seconds given, Ninja Army Chief").
const STATE_INFO_NO_LOCK: i32 = 350;

/// Owl's TAE transition judgment marker, active throughout the heat-up and kneeling loop.
const OWL_HEAT_UP: i32 = 3_506_080;

/// env 2004, "swing is hitting" ([`Duel::publish_swing_hit`]).
const ENV_SWING_HIT: i32 = 2004;

/// A deathblow as the duel reports it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DeathblowReport {
    /// ThrowParam start and body rows, on the step the deathblow began.
    pub started: Option<(i32, i32)>,
    /// The blade went in this step.
    pub hit: bool,
    pub killed: bool,
}

/// An enemy throw on Wolf as the duel reports it (see [`crate::grab`]).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GrabReport {
    /// ThrowParam row of a throw that caught Wolf this step.
    pub started: Option<i32>,
    /// The throw's damage that landed this step: (behaviour judge, HP, posture).
    pub hits: Vec<(i32, i32, i32)>,
}

/// Wolf's contact with a map action event wall ([`crate::hit_filter`]).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct HelperEvent {
    pub hit_filter: u8,
    pub part: String,
    pub point: [f32; 3],
    /// Where the fall return put him, for a filter-15 wall.
    pub returned_to: Option<[f32; 3]>,
}

/// One step of the duel.
#[derive(Debug, Clone, Default)]
pub struct DuelReport {
    /// Hit index to originating animation for delayed projectiles.
    pub projectile_sources: std::collections::BTreeMap<usize, i32>,
    pub player: StepReport,
    pub enemy: StepReport,
    pub hits: Vec<(HitEvent, HitResult)>,
    pub locked_on: bool,
    pub deathblow: DeathblowReport,
    pub grab: GrabReport,
    /// Both characters were put back at their start this step.
    pub respawned: bool,
    /// A perilous-attack warning (the red kanji) reached Wolf this step ([`crate::perilous`]).
    pub perilous: Option<crate::perilous::PerilousWarning>,
    /// Wolf's anti-ground attack (the air kick) landed on an enemy in a perilous sweep, whose
    /// "jump kick effective" SpEffect multiplied its posture damage this step.
    pub jump_counter: bool,
    /// Wolf's HKS-powered lightning return opened its actual attack window this step.
    pub other_counter: bool,
    /// ThrowParam row and posture damage of a Mikiri begun this step.
    pub mikiri: Option<(i32, i32)>,
}

/// ThrowParam and both models' sorb dummies, for deathblows.
struct Throws {
    table: Arc<ThrowTable>,
    model: Arc<Flver>,
    /// The enemy's model, for rows that snap Wolf to one of its dummies (`defSorbDmyId`).
    enemy_model: Option<Arc<Flver>>,
}

pub struct Duel {
    /// Optional independent map foes sharing this duel's single live Wolf.
    pub population: Option<population::Population>,
    /// Population kernels receive their target and auto-aim candidates from the owner.
    population_target: Option<bool>,
    /// Independent combatants in the Hall of Illusions, sharing this duel's Wolf.
    pub screen_monkeys: Option<screen_monkeys::Encounter>,
    pub monk_phantoms: Option<monk_phantoms::Encounter>,
    pub father_bird: Option<father_bird::Bird>,
    pub old_dragons: Option<old_dragons::Encounter>,
    pub butterfly_illusions: Option<butterfly_illusions::Encounter>,
    pub headless_ape: Option<headless_ape::Encounter>,
    monk_stats: Option<crate::true_monk::PhaseStats>,
    /// An auxiliary opponent resolves contacts against Wolf's already advanced frame.
    shared_player_frame: bool,
    defer_map_events: bool,
    auxiliary_ezstate: Option<(u8, i32)>,
    auxiliary_animation_enabled: bool,
    pub camera: crate::camera::CameraParams,
    /// The camera as Wolf's animation drives it (the deathblow camera, TAE 151 / 153 / 155),
    /// updated every step.
    pub camera_director: crate::camera::CameraDirector,
    pub player: Character,
    pub enemy: Character,
    /// The stand-in brain, used when [`Duel::ai`] is `None`.
    pub brain: SimpleBrain,
    /// The soldier's own AI (its compiled Lua battle goal); `None` uses [`Duel::brain`].
    pub ai: Option<AiBrain>,
    /// Hitboxes and attack data: index 0 the player, 1 the enemy.
    pub fighters: [Combatant; 2],
    pub detector: HitDetector,
    /// HP and posture of both, and the hit rules.
    pub rules: CombatRules,
    /// Whether the enemy brain runs (off for scripted player-only tests).
    pub enemy_active: bool,
    /// Whether the opponent slot participates at all, independently of death or passive AI.
    pub enemy_present: bool,
    /// The deathblow in progress.
    pub deathblow: Option<Deathblow>,
    /// The enemy's throw on Wolf in progress.
    pub enemy_throw: Option<EnemyThrow>,
    mikiri: Option<crate::perilous::Mikiri>,
    /// Testing aid: the enemy requests this action (an animation id such as 3040) whenever it is
    /// not already playing it, on top of its brain (or alone with the brain off).
    pub force_act: Option<i32>,
    /// Seconds until the respawn after a death.
    pub respawn_in: Option<f32>,
    throws: Option<Throws>,
    pub(crate) cache: PathBuf,
    /// Which enemy this duel loaded.
    pub spec: EnemySpec,
    /// Marks on a later placed entity belonging to this encounter's visible boss bar.
    encounter_extra_marks: i32,
    /// The deathblow marks this encounter's health bar started with (NpcParam `ninsatuNum`, 0
    /// counting as one, plus `encounter_extra_marks`); kept across hand-overs, so a
    /// later phase's last mark still shows.
    encounter_marks: i32,
    initial_spec: EnemySpec,
    pending_handover: Option<(i32, Option<i32>)>,
    /// Map-event cinematic boundary, queued until any active deathblow releases.
    pub cinematic: Option<crate::events::Cinematic>,
    /// Steps since the deathblow switched the enemy to its death animation (0: not yet).
    defender_dying: u8,
    spawn: Option<[Placement; 2]>,
    idols: idol::Idols,
    pickups: pickup::Pickups,
    /// What Wolf carries: tools, goods, Sen, skills (the in-game menu shows it).
    pub kit: crate::kit::Kit,
    /// The opponent was dead last step, so [`Duel::pay_kill`] pays on the death edge only:
    /// once per life, again after a rest or respawn stands it up.
    was_dead: bool,
    lock_button: bool,
    attack_button: bool,
    /// The enemy took a hit last step (the AI's damage interrupt).
    enemy_damaged: bool,
    /// The defender clip of the last deathblow; no new deathblow starts while it plays (the
    /// enemy is in its throw-defence state, getting up from a non-lethal blow).
    last_throw_def_clip: Option<String>,
    /// Supporting floors at pair entry, retained through throw recovery to prevent
    /// successive small steps from migrating down an outer roof during a throw.
    paired_floor: Option<[f32; 2]>,
    /// Wolf's full-body clip in which one of his attacks connected: env 2004 ("swing is hitting")
    /// reads true while it still plays.
    swing_hit: Option<String>,
    /// Hit stop of the player and the enemy: a frozen character does not advance.
    pub hit_stop: [HitStop; 2],
    /// Pushes in progress on the player and the enemy.
    pub knockback: [Option<Knockback>; 2],
    /// Each character's last real step, reported again while it is frozen.
    last_steps: [StepReport; 2],
    /// Thrown and shot attacks in flight.
    pub bullets: crate::bullets::Bullets,
    lightning_returned: bool,
    /// Placeholder void depth below the lowest collision, or spawn without collision, past
    /// which Wolf is returned to where he last stood; `None` disables the return.
    pub fall_respawn_depth: Option<f32>,
    fall_materials: std::collections::HashMap<u32, FallMaterial>,
    /// Where Wolf last stood on collision, for the return after a fall into the void.
    last_safe: Option<([f32; 3], f32)>,
    /// Seconds into the fall return's model fade, after Wolf touched a filter-15 event wall.
    fall_return_fade: Option<f32>,
    /// The filter-15 wall that started that fade, whose [`HelperEvent`] gets the return; walls
    /// passed during the fade are only recorded.
    fall_return_part: Option<String>,
    /// Event wall parts Wolf is in contact with, so one contact is recorded once.
    touching: Vec<String>,
    /// Wolf's contacts with map action event walls (see [`crate::hit_filter`]), oldest first.
    pub helper_events: Vec<HelperEvent>,
    /// The enemy's fall death (see [`crate::npc_fall`]).
    pub enemy_fall: crate::npc_fall::NpcFall,
    /// The boss brain choosing the enemy's acts with its Lua AI (see [`crate::brain`]);
    /// `None` leaves the Lua AI alone.
    pub boss_brain: Option<crate::brain::BrainDriver>,
    /// Testing aid: the player takes no HP or posture damage (hits, reactions, hit stop and
    /// knockback are unchanged).
    pub god: bool,
    /// Testing aid (`--immortal`): Wolf cannot die (HP stops at 1) but his posture, guard breaks
    /// and HP-dependent recovery work as in the game, so mashing into guards and deflects
    /// can still break him.
    pub immortal: bool,
    /// The fight's map events (phase changes, boss bar, defeat), when the enemy has any
    /// ([`crate::arena::boss_events`]).
    pub events: Option<FightEvents>,
    pub isshin_fire: Option<crate::isshin_fire::FireEmitter>,
    /// The arena's gated collision (fog wall and boundary, [`CollisionWorld::set_gate`]), which
    /// blocks while one of the fight's fog walls stands. Set by `Arena::place`.
    pub fog_gate: Option<Arc<crate::collision::CollisionWorld>>,
}

/// The state the map events keep for a fight (see [`crate::events`]).
#[derive(Debug, Clone)]
pub struct FightEvents {
    pub scripts: EventScripts,
    /// The boss's entity id in the events.
    pub boss_entity: i32,
    /// NpcName text id of the boss health bar, while it is shown.
    pub boss_bar: Option<i32>,
    /// The small HP bar over the boss's head is shown.
    pub hp_bar: bool,
    /// `HandleBossDefeat` ran.
    pub defeated: bool,
    /// An event command waiting for the boss's script (`env(105, slot)`): (slot, command).
    pub ezstate: Option<(u8, i32)>,
    /// A map-authored player execution request, consumed by the player's HKS script.
    pub player_ezstate: Option<(u8, i32)>,
    sword_saint_finish: crate::sword_saint::FinishRequests,
    /// Whether the post-intro event setup has already run.
    initialized: bool,
    /// The fight's fog walls (placed by the map's events, see [`crate::fogwall`]).
    pub fog_walls: Vec<crate::fogwall::FogWall>,
}

impl FightEvents {
    /// The fog walls that stand now: their start flag is on and their end flag is off.
    pub fn standing_fog_walls(&self) -> impl Iterator<Item = &crate::fogwall::FogWall> {
        self.fog_walls
            .iter()
            .filter(|w| w.active(|f| self.scripts.flag(f)))
    }

    pub fn load(cache: &Path, spec: &BossEvents) -> Result<Self, String> {
        let mut scripts = EventScripts::load(cache, spec.map, spec.events)?;
        scripts.flags.extend(spec.flags.iter().copied());
        crate::arenas_a::seed_scene(cache, spec, &mut scripts)?;
        crate::arena_c::seed_scene(spec.boss_entity, &mut scripts.cast_scene);
        let fog_walls = if spec.fog_events.is_empty() {
            Vec::new()
        } else {
            crate::fogwall::load(cache, spec.map, spec.fog_events)?
        };
        Ok(Self {
            fog_walls,
            scripts,
            boss_entity: spec.boss_entity,
            boss_bar: None,
            hp_bar: true,
            defeated: false,
            ezstate: None,
            player_ezstate: None,
            sword_saint_finish: Default::default(),
            initialized: false,
        })
    }

    /// The fight reloads: the events start over, their flags stay.
    fn restart(&mut self) {
        self.scripts.restart();
        crate::arena_c::seed_scene(self.boss_entity, &mut self.scripts.cast_scene);
        self.boss_bar = None;
        self.hp_bar = true;
        self.defeated = false;
        self.ezstate = None;
        self.player_ezstate = None;
        self.sword_saint_finish = Default::default();
        self.initialized = false;
    }
}

/// The env id of the event command a character's script reads (`env(105, slot)`, "command id
/// from the event" in the community env list).
const ENV_EVENT_COMMAND: i32 = 105;

/// The env id the NPC script asks "in a throw?" (`env(206)`): its `ThrowCommonFunction` accepts
/// event commands only then and skips damage and death. Ours: true for the enemy while a
/// deathblow holds him or he still plays its defender clip (inferred; not traced in the exe).
const ENV_IN_THROW: i32 = 206;

/// The env id of the AI's endure-cancel action (`env(123)`, "AI endure cancel type" in the
/// community env list): an `EndureAttack` goal's request, which the NPC script's
/// `ExecAIAction` takes before every other cancel.
const ENV_AI_ENDURE_ACTION: i32 = 123;
/// The immediate-action wrapper's request, read by the shipped ExecAIAction dispatcher.
const ENV_AI_IMMEDIATE_ACTION: i32 = 125;

/// TAE action flag 78, "End If AI Move Queued": an NPC clip's move window.
const AI_MOVE_FLAG: i32 = 78;

/// How long an effect with no duration (`effectEndurance` 0, applied once) stays visible to the
/// scripts and the AI's SpEffect observers, in steps (ours).
const INSTANT_EFFECT_STEPS: f32 = 2.0;

/// The world as the map events see it: Wolf is entity 10000, the enemy the boss's entity id.
struct EventView<'a> {
    duel: &'a Duel,
    boss: i32,
}

impl EventView<'_> {
    fn who(&self, entity: i32) -> Option<usize> {
        match entity {
            PLAYER_ENTITY => Some(0),
            e if e == self.boss => Some(1),
            _ => None,
        }
    }

    fn character(&self, i: usize) -> &Character {
        if i == 0 {
            &self.duel.player
        } else {
            &self.duel.enemy
        }
    }
}

impl EventHost for EventView<'_> {
    fn encounter_condition(
        &self,
        instruction: &sekiro_formats::emevd::Instruction,
    ) -> Option<bool> {
        if let Some(value) = self
            .duel
            .screen_monkeys
            .as_ref()
            .and_then(|encounter| encounter.condition(instruction, &self.duel.player))
        {
            return Some(value);
        }
        if instruction.code() == (3, 3) {
            let a = self
                .character(self.who(instruction.i32_at(4)?)?)
                .body
                .position;
            let b = self
                .character(self.who(instruction.i32_at(8)?)?)
                .body
                .position;
            return crate::events::radius_condition(instruction, a, b);
        }
        None
    }
    fn inside_area(&self, entity: i32, area: i32) -> Option<bool> {
        if let Some(bird) = self
            .duel
            .father_bird
            .as_ref()
            .filter(|b| b.entity == entity)
        {
            return crate::arena_c::regions::contains(
                &self.duel.cache,
                "m10_00_00_00",
                area,
                bird.duel.enemy.body.position,
            );
        }
        if let Some(actor) = self
            .duel
            .screen_monkeys
            .as_ref()
            .and_then(|e| e.actor(entity))
        {
            return crate::arena_c::regions::contains(
                &self.duel.cache,
                screen_monkeys::MAP,
                area,
                actor.enemy.body.position,
            );
        }
        let character = self.character(self.who(entity)?);
        let events = crate::arena::boss_events(&self.duel.spec)?;
        crate::arena_c::regions::contains(
            &self.duel.cache,
            crate::sword_saint::world_map(events.map),
            area,
            character.body.position,
        )
    }
    fn in_state(&self, entity: i32, state: i32) -> Option<bool> {
        self.who(entity)?;
        (state == 10).then_some(self.duel.deathblow.is_some())
    }
    fn cutscene_finished(&self, id: i32) -> bool {
        self.duel
            .cinematic
            .as_ref()
            .is_none_or(|c| c.id != id || c.applied)
    }
    fn has_sp_effect(&self, entity: i32, id: i32) -> Option<bool> {
        if let Some(bird) = self
            .duel
            .father_bird
            .as_ref()
            .filter(|b| b.entity == entity)
        {
            if id == 200004
                && bird
                    .duel
                    .ai
                    .as_ref()
                    .is_some_and(|ai| ai.state.target_state >= 2)
            {
                return Some(true);
            }
            return EventView {
                duel: &bird.duel,
                boss: entity,
            }
            .has_sp_effect(entity, id);
        }
        if let Some(cast) = &self.duel.old_dragons {
            if let Some(value) = cast.group_effect(entity, id) {
                return Some(value);
            }
            if let Some(actor) = cast.actor(entity) {
                return EventView {
                    duel: actor,
                    boss: entity,
                }
                .has_sp_effect(entity, id);
            }
        }
        if entity == headless_ape::BROWN
            && let Some(actor) = &self.duel.headless_ape
        {
            return EventView {
                duel: &actor.duel,
                boss: entity,
            }
            .has_sp_effect(entity, id);
        }
        if let Some(actor) = self
            .duel
            .butterfly_illusions
            .as_ref()
            .and_then(|e| e.actor(entity))
        {
            return EventView {
                duel: actor,
                boss: entity,
            }
            .has_sp_effect(entity, id);
        }
        if let Some(actor) = self
            .duel
            .monk_phantoms
            .as_ref()
            .and_then(|e| e.actor(entity))
        {
            return EventView {
                duel: actor,
                boss: entity,
            }
            .has_sp_effect(entity, id);
        }
        if let Some(actor) = self
            .duel
            .screen_monkeys
            .as_ref()
            .and_then(|e| e.actor(entity))
        {
            return EventView {
                duel: actor,
                boss: entity,
            }
            .has_sp_effect(entity, id);
        }
        let i = self.who(entity)?;
        Some(
            self.character(i).tae_frame().sp_effects.contains(&id)
                || self.duel.rules.fighters[i].resident().contains(&id),
        )
    }

    fn has_event_message(&self, entity: i32, id: i32) -> Option<bool> {
        if let Some(bird) = self
            .duel
            .father_bird
            .as_ref()
            .filter(|b| b.entity == entity)
        {
            return EventView {
                duel: &bird.duel,
                boss: entity,
            }
            .has_event_message(entity, id);
        }
        if let Some(actor) = self.duel.old_dragons.as_ref().and_then(|e| e.actor(entity)) {
            return EventView {
                duel: actor,
                boss: entity,
            }
            .has_event_message(entity, id);
        }
        if entity == headless_ape::BROWN
            && let Some(actor) = &self.duel.headless_ape
        {
            return EventView {
                duel: &actor.duel,
                boss: entity,
            }
            .has_event_message(entity, id);
        }
        if let Some(actor) = self
            .duel
            .butterfly_illusions
            .as_ref()
            .and_then(|e| e.actor(entity))
        {
            return EventView {
                duel: actor,
                boss: entity,
            }
            .has_event_message(entity, id);
        }
        if let Some(actor) = self
            .duel
            .monk_phantoms
            .as_ref()
            .and_then(|e| e.actor(entity))
        {
            return EventView {
                duel: actor,
                boss: entity,
            }
            .has_event_message(entity, id);
        }
        if let Some(actor) = self
            .duel
            .screen_monkeys
            .as_ref()
            .and_then(|e| e.actor(entity))
        {
            return EventView {
                duel: actor,
                boss: entity,
            }
            .has_event_message(entity, id);
        }
        let i = self.who(entity)?;
        Some(self.character(i).tae_runtime.event_message == Some(id))
    }

    fn is_dead(&self, entity: i32) -> Option<bool> {
        if let Some(actor) = self.duel.old_dragons.as_ref().and_then(|e| e.actor(entity)) {
            return Some(actor.rules.fighters[1].dead());
        }
        if entity == headless_ape::BROWN
            && let Some(actor) = &self.duel.headless_ape
        {
            return Some(actor.duel.rules.fighters[1].dead());
        }
        if let Some(actor) = self
            .duel
            .butterfly_illusions
            .as_ref()
            .and_then(|e| e.actor(entity))
        {
            return Some(actor.rules.fighters[1].dead());
        }
        if let Some(actor) = self
            .duel
            .screen_monkeys
            .as_ref()
            .and_then(|e| e.actor(entity))
        {
            return Some(actor.rules.fighters[1].dead());
        }
        if crate::arenas_a::retired_butterfly(self.boss, entity) {
            return Some(true);
        }
        Some(self.duel.rules.fighters[self.who(entity)?].dead())
    }

    fn hp(&self, entity: i32) -> Option<i32> {
        if let Some(actor) = self.duel.old_dragons.as_ref().and_then(|e| e.actor(entity)) {
            return Some(actor.rules.fighters[1].vitality.hp);
        }
        if entity == headless_ape::BROWN
            && let Some(actor) = &self.duel.headless_ape
        {
            return Some(actor.duel.rules.fighters[1].vitality.hp);
        }
        if let Some(actor) = self
            .duel
            .butterfly_illusions
            .as_ref()
            .and_then(|e| e.actor(entity))
        {
            return Some(actor.rules.fighters[1].vitality.hp);
        }
        if let Some(actor) = self
            .duel
            .screen_monkeys
            .as_ref()
            .and_then(|e| e.actor(entity))
        {
            return Some(actor.rules.fighters[1].vitality.hp);
        }
        Some(self.duel.rules.fighters[self.who(entity)?].vitality.hp)
    }
    fn max_hp(&self, entity: i32) -> Option<i32> {
        if let Some(actor) = self.duel.old_dragons.as_ref().and_then(|e| e.actor(entity)) {
            return Some(actor.rules.fighters[1].vitality.max_hp);
        }
        if let Some(actor) = self
            .duel
            .screen_monkeys
            .as_ref()
            .and_then(|e| e.actor(entity))
        {
            return Some(actor.rules.fighters[1].vitality.max_hp);
        }
        Some(self.duel.rules.fighters[self.who(entity)?].vitality.max_hp)
    }
    fn health_bars(&self, entity: i32) -> Option<i32> {
        Some(
            self.duel.rules.fighters[self.who(entity)?]
                .vitality
                .deathblows_left
                .max(0),
        )
    }
}

fn placement(c: &Character) -> Placement {
    Placement {
        position: c.body.position,
        yaw: c.body.yaw,
    }
}

/// `a200_510000` -> 510000.
fn anim_id(clip: &str) -> Option<i32> {
    clip.split_once('_')?.1.parse().ok()
}

impl Duel {
    /// Wolf at the origin facing -Z and an Ashina soldier (`c1010`, NpcParam 10100000) `distance`
    /// metres in front of him, facing him.
    pub fn load(cache: &Path, distance: f32) -> Result<Self, LoadError> {
        Self::load_with(cache, distance, EnemySpec::soldier())
    }

    /// Like [`Duel::load`], against the enemy `spec`.
    pub fn load_with(cache: &Path, distance: f32, spec: EnemySpec) -> Result<Self, LoadError> {
        Self::load_with_player(cache, distance, spec, None)
    }

    fn load_with_player(
        cache: &Path,
        distance: f32,
        spec: EnemySpec,
        player: Option<Character>,
    ) -> Result<Self, LoadError> {
        let (player, enemy) = Self::load_characters_with(cache, distance, &spec, player)?;
        let fighters = [
            Combatant::player(cache, &player.skeleton),
            Combatant::npc(cache, &spec.chr, spec.npc_param, &enemy.skeleton),
        ];
        let rules = CombatRules::load(cache, &[&fighters[0], &fighters[1]]);
        let ai = Self::load_ai(cache, &spec);
        let read_model = |chr: &str| {
            let path = cache.join(format!("raw/chr/{chr}.chrbnd.d/{chr}.flver"));
            crate::shared::load(path.clone(), || {
                flver::parse(&std::fs::read(&path).ok()?).ok()
            })
        };
        let throws = crate::shared::load(cache.join("ThrowParam"), || {
            ThrowTable::load_from_cache(cache).ok()
        })
        .and_then(|table| {
            Some(Throws {
                table,
                model: read_model("c0000")?,
                enemy_model: read_model(&spec.chr),
            })
        });
        let encounter_extra_marks = crate::butterfly::additional_marks(cache, &spec);
        let encounter_marks =
            deathblow_marks(rules.fighters[1].vitality.deathblows_left) + encounter_extra_marks;
        Ok(Self {
            monk_stats: crate::true_monk::owns(&spec)
                .then(|| crate::true_monk::PhaseStats::load(cache))
                .flatten(),
            screen_monkeys: None,
            monk_phantoms: None,
            father_bird: None,
            old_dragons: None,
            butterfly_illusions: None,
            headless_ape: None,
            shared_player_frame: false,
            population: None,
            population_target: None,
            defer_map_events: false,
            auxiliary_ezstate: None,
            auxiliary_animation_enabled: true,
            camera: crate::camera::CameraParams::load(cache),
            camera_director: crate::camera::CameraDirector::load(cache),
            player,
            enemy,
            brain: SimpleBrain::default(),
            ai,
            fighters,
            detector: HitDetector::default(),
            rules,
            enemy_active: true,
            enemy_present: true,
            deathblow: None,
            enemy_throw: None,
            mikiri: None,
            force_act: None,
            respawn_in: None,
            throws,
            cache: cache.to_path_buf(),
            initial_spec: spec.clone(),
            encounter_extra_marks,
            encounter_marks,
            pending_handover: None,
            cinematic: None,
            spec,
            defender_dying: 0,
            spawn: None,
            idols: idol::Idols::default(),
            pickups: pickup::Pickups::default(),
            kit: crate::kit::Kit::default(),
            was_dead: false,
            lock_button: false,
            attack_button: false,
            enemy_damaged: false,
            last_throw_def_clip: None,
            paired_floor: None,
            swing_hit: None,
            hit_stop: [HitStop::default(); 2],
            knockback: [None; 2],
            last_steps: Default::default(),
            bullets: Default::default(),
            lightning_returned: false,
            fall_respawn_depth: None,
            fog_gate: None,
            fall_materials: load_fall_materials(cache),
            last_safe: None,
            fall_return_fade: None,
            fall_return_part: None,
            touching: Vec::new(),
            helper_events: Vec::new(),
            enemy_fall: Default::default(),
            boss_brain: None,
            god: false,
            immortal: false,
            events: None,
            isshin_fire: None,
        })
        .and_then(|mut duel| {
            duel.events = crate::arena::boss_events(&duel.spec).and_then(|spec| {
                FightEvents::load(cache, spec)
                    .map_err(|e| eprintln!("{}: map events not loaded: {e}", duel.spec.chr))
                    .ok()
            });
            if screen_monkeys::controller(&duel.spec) {
                duel.screen_monkeys = Some(screen_monkeys::Encounter::load(cache)?);
            }
            if old_dragons::owns(&duel.spec) {
                duel.old_dragons = Some(old_dragons::Encounter::load(cache)?);
            }
            if crate::true_monk::owns(&duel.spec) {
                duel.monk_phantoms = Some(monk_phantoms::Encounter::load(cache)?);
            }
            if crate::owl_father::owns(&duel.spec) {
                duel.father_bird = Some(father_bird::Bird::load(cache, &duel.spec)?);
            }
            if duel.spec.chr == "c5090" {
                duel.butterfly_illusions = Some(butterfly_illusions::Encounter::load(cache)?);
            }
            if duel.spec.chr == "c5100" && duel.spec.npc_param == 51_000_100 {
                duel.headless_ape = Some(headless_ape::Encounter::load(cache)?);
            }
            Ok(duel)
        })
    }

    /// The enemy's own AI from the extracted scripts, or `None` (with a warning) when they
    /// are missing or fail to load.
    fn load_ai(cache: &Path, spec: &EnemySpec) -> Option<AiBrain> {
        let (pd, dd) = (
            cache.join("raw/param/gameparam/gameparam.parambnd.d"),
            cache.join("refs/paramdex/Defs"),
        );
        let think = crate::hits::load_table(&pd, &dd, "NpcThinkParam", "NpcThinkParam")?
            .find(spec.think)
            .and_then(|r| ThinkParams::from_row(&r).ok())?;
        match AiBrain::load(cache, &spec.ai_bundle, think, AI_SEED) {
            Ok(mut brain) => {
                if let Some(events) = crate::arena::boss_events(spec) {
                    brain.state.map_regions =
                        Some(crate::lua_ai::regions::MapRegions::new(cache, events.map));
                }
                if spec.starts_in_battle {
                    brain.start_in_battle();
                }
                Some(brain)
            }
            Err(e) => {
                eprintln!("{} AI not loaded, using the stand-in brain: {e}", spec.chr);
                None
            }
        }
    }

    /// Switches the soldier to the stand-in [`SimpleBrain`] (for comparison and for tests that
    /// need a predictable attacker).
    pub fn use_simple_ai(&mut self) {
        self.boss_brain = None;
        self.ai = None;
    }

    /// Puts a boss brain (see [`crate::brain`]) in charge of the enemy's acts; its Lua AI keeps
    /// the reflexes. `log` writes a JSONL log to `cache/brain_logs/` and returns its path.
    /// Fails for an enemy without an action table or without its Lua AI.
    pub fn set_brain(
        &mut self,
        brain: Box<dyn crate::brain::BossBrain>,
        log: bool,
    ) -> Result<Option<PathBuf>, String> {
        if crate::sword_saint::keep_lua(&self.spec, brain.name(), log)
            || crate::owl_father::keep_lua(&self.spec, brain.name(), log)
        {
            eprintln!(
                "{}: retaining the installed Lua AI",
                self.spec.display_name()
            );
            return Ok(None);
        }
        if self.spec.chr == "c5100"
            && brain.name() == "scripted"
            && crate::brain::BossActions::for_chr(&self.spec.chr).is_none()
            && !log
        {
            eprintln!("c5100: no external action table; retaining the extracted Lua AI");
            return Ok(None);
        }
        if crate::brain::BossActions::for_chr(&self.spec.chr).is_none() && !brain.plans() && !log {
            return Ok(None);
        }
        let actions = crate::brain::BossActions::for_chr(&self.spec.chr)
            .ok_or_else(|| format!("no boss brain action list for {}", self.spec.chr))?;
        if self.ai.is_none() {
            return Err(format!(
                "{} has no Lua AI to pair the brain with",
                self.spec.chr
            ));
        }
        let mut driver = crate::brain::BrainDriver::new(brain, actions);
        driver.move_sheet = crate::brain::moves::build(self, actions);
        driver.observe_space(self);
        let path = if log {
            Some(
                driver
                    .log_to(&self.cache.join("brain_logs"))
                    .map_err(|e| format!("brain log: {e}"))?,
            )
        } else {
            None
        };
        self.boss_brain = Some(driver);
        Ok(path)
    }

    /// Whether Wolf's current attack carries the parry-timing flag ([`FLAG_PARRY_TIMING`]).
    /// Range and facing are the AI script's own checks: `Common_Parry` tests the parry
    /// distance (`GetDist_Parry`) and a front cone itself.
    fn parry_timing(&self) -> bool {
        self.player.tae_frame().flag(FLAG_PARRY_TIMING)
    }

    /// What the soldier's AI sees this step.
    fn ai_world(&self, dt: f32) -> AiWorld {
        let actor = |c: &Character, i: usize| {
            let f = &self.rules.fighters[i];
            let mut sp_effects = c.tae_frame().sp_effects.clone();
            sp_effects.extend_from_slice(f.resident());
            if c.kind == crate::character::ControlKind::Npc
                && (c.behavior.runtime.offsets.by_type.contains_key(&15)
                    || sp_effects.iter().any(|id| {
                        c.tae
                            .sp_effects
                            .get(id)
                            .is_some_and(|info| (270..=274).contains(&info.state_info))
                    }))
            {
                let group = c.behavior.runtime.offsets.offset(15) / 100;
                sp_effects.retain(|id| {
                    c.tae.sp_effects.get(id).is_none_or(|info| {
                        !(270..=274).contains(&info.state_info)
                            || info.state_info as u32 == 270 + group
                    })
                });
                if let Some((&id, _)) = c
                    .tae
                    .sp_effects
                    .iter()
                    .find(|(_, info)| info.state_info as u32 == 270 + group)
                    && !sp_effects.contains(&id)
                {
                    sp_effects.push(id);
                }
            }
            AiActor {
                position: c.body.position,
                yaw: c.body.yaw,
                hp: f.vitality.hp as f32,
                max_hp: f.vitality.max_hp as f32,
                posture: f.posture.remaining as f32,
                max_posture: f.posture.max as f32,
                sp_effects,
                guarding: c.tae_frame().flag(crate::hits::FLAG_GUARD),
            }
        };
        let main = self.enemy.behavior.runtime.main_clip();
        let frame = self.enemy.tae_frame();
        AiWorld {
            dt,
            me: actor(&self.enemy, 1),
            target: (!self.rules.fighters[0].dead() && self.population_target != Some(false))
                .then(|| actor(&self.player, 0)),
            hit_radius: self.fighters[1].radius,
            current_anim: main.and_then(|c| anim_id_from_clip(&c.animation)),
            current_anim_time: main.map_or(0.0, |c| c.time),
            combo_window: COMBO_FLAGS.iter().any(|&f| frame.flag(f))
                || main.is_some_and(|c| c.at_end()),
            home: self
                .spawn
                .map_or(self.enemy.body.position, |s| s[1].position),
            parry_timing: self.parry_timing(),
            target_attacking: self
                .player
                .behavior
                .runtime
                .main_state_path()
                .iter()
                .any(|s| s.contains("Attack")),
            damaged: self.enemy_damaged,
        }
    }

    fn load_characters(
        cache: &Path,
        distance: f32,
        spec: &EnemySpec,
    ) -> Result<(Character, Character), LoadError> {
        Self::load_characters_with(cache, distance, spec, None)
    }

    fn load_characters_with(
        cache: &Path,
        distance: f32,
        spec: &EnemySpec,
        player: Option<Character>,
    ) -> Result<(Character, Character), LoadError> {
        let mut player = match player {
            Some(player) => player,
            None => Character::load_from_cache(cache)?,
        };
        let registry = crate::registry::Registry::load(cache).ok();
        let anim_chr = registry.as_ref().map_or_else(
            || spec.chr.clone(),
            |r| r.anim_chr(&spec.chr, spec.npc_param),
        );
        let mut enemy = Character::load_npc_with_anim(cache, &spec.chr, &anim_chr)?;
        if let Some(row) = registry.as_ref().and_then(|r| r.npc_row(spec.npc_param)) {
            for i in 0..32 {
                if let Some(id) = row[format!("spEffectID{i}")]
                    .as_i64()
                    .and_then(|id| enemy.tae.sp_effects.get(&(id as i32)))
                    .filter(|info| (270..=274).contains(&info.state_info))
                {
                    enemy
                        .behavior
                        .runtime
                        .offsets
                        .by_type
                        .insert(15, (id.state_info as u32 - 270) * 100);
                }
            }
        }
        enemy.body.position = [0.0, 0.0, -distance];
        enemy.snap_yaw(std::f32::consts::PI);
        let dt = 1.0 / 60.0;
        player.settle(dt, 400);
        enemy.settle(dt, 400);
        Ok((player, enemy))
    }

    /// Puts both characters back where they stood on the first step, with fresh scripts, full
    /// HP and posture. Each keeps its ground.
    pub fn reset(&mut self) -> Result<(), LoadError> {
        // Settle a declared result before resetting its event state or rebinding the phase.
        if let Some(mut driver) = self.boss_brain.take() {
            driver.finish_if_over(self);
            self.boss_brain = Some(driver);
        }
        if self.spec != self.initial_spec {
            let spot = self.spawn.map_or(placement(&self.enemy), |s| s[1]);
            self.switch_enemy(self.initial_spec.clone(), spot)?;
        }
        self.pending_handover = None;
        self.cinematic = None;
        let (mut player, mut enemy) = Self::load_characters(&self.cache, 4.0, &self.spec)?;
        std::mem::swap(&mut player.ground, &mut self.player.ground);
        player.docking.edges = std::mem::take(&mut self.player.docking.edges);
        player.docking.rebuild_links();
        player.wire.points = std::mem::take(&mut self.player.wire.points);
        player.wire.search = self.player.wire.search.clone();
        player.wire.variations = self.player.wire.variations.clone();
        player.wire.offsets = self.player.wire.offsets.clone();
        player.wire.collision = self.player.wire.collision.take();
        std::mem::swap(&mut enemy.ground, &mut self.enemy.ground);
        if let Some([p, e]) = self.spawn {
            let p = self.idols.checkpoint.unwrap_or(p);
            player.body.position = p.position;
            player.snap_yaw(p.yaw);
            enemy.body.position = e.position;
            enemy.snap_yaw(e.yaw);
        }
        self.player = player;
        self.enemy = enemy;
        self.rules.reset();
        if crate::owl_father::owns(&self.spec) {
            let mut bird = father_bird::Bird::load(&self.cache, &self.spec)?;
            if let Some(world) = &self.fog_gate {
                bird.place(world);
            }
            self.father_bird = Some(bird);
        }
        if self.spec.chr == "c5100" && self.spec.npc_param == 51_000_100 {
            let mut cast = headless_ape::Encounter::load(&self.cache)?;
            if let Some(world) = &self.fog_gate {
                cast.place(world);
            }
            self.headless_ape = Some(cast);
        }
        if self.spec.chr == "c5090" {
            let mut cast = butterfly_illusions::Encounter::load(&self.cache)?;
            if let Some(world) = &self.fog_gate {
                cast.place(world);
            }
            self.butterfly_illusions = Some(cast);
        }
        self.monk_stats = crate::true_monk::owns(&self.spec)
            .then(|| crate::true_monk::PhaseStats::load(&self.cache))
            .flatten();
        if crate::true_monk::owns(&self.spec) {
            let mut cast = monk_phantoms::Encounter::load(&self.cache)?;
            if let Some(world) = &self.fog_gate {
                cast.place(world);
            }
            self.monk_phantoms = Some(cast);
        }
        self.detector = HitDetector::default();
        if self.ai.is_some() {
            self.ai = Self::load_ai(&self.cache, &self.spec);
        }
        self.enemy_damaged = false;
        self.was_dead = false;
        self.hit_stop = [HitStop::default(); 2];
        self.knockback = [None; 2];
        self.deathblow = None;
        self.enemy_throw = None;
        self.mikiri = None;
        self.respawn_in = None;
        // The driver's fight restarts below (an unfinished one ends as such), not as a win.
        self.idols.arriving = false;
        // A fall return cut short by death must not move the respawned Wolf.
        self.fall_return_fade = None;
        self.fall_return_part = None;
        self.touching.clear();
        self.enemy_fall = Default::default();
        self.defender_dying = 0;
        self.last_throw_def_clip = None;
        self.paired_floor = None;
        self.swing_hit = None;
        self.bullets.clear();
        self.lightning_returned = false;
        if let Some(ev) = self.events.as_mut() {
            ev.restart();
        }
        crate::demon::on_reset(
            &self.spec,
            self.events.as_mut(),
            self.fall_respawn_depth.is_some(),
        );
        if let Some(encounter) = self.old_dragons.as_mut() {
            encounter.reset(&self.cache)?;
        }
        if let Some(encounter) = self.screen_monkeys.as_mut() {
            for monkey in &mut encounter.monkeys {
                monkey.duel.reset()?;
            }
        }
        if let Some(mut driver) = self.boss_brain.take() {
            driver.rebind(self);
            driver.start_fight();
            if let Some(ai) = self.ai.as_mut()
                && let Some(actions) = crate::brain::BossActions::for_chr(&self.spec.chr)
            {
                ai.enable_act_hook(actions.reflex_acts);
            }
            self.boss_brain = Some(driver);
        }
        Ok(())
    }

    /// A map event hands the single active opponent slot to a separately placed character.
    /// Wolf, his health/status, the arena and the original reset point survive the hand-over.
    fn switch_enemy(&mut self, spec: EnemySpec, spot: Placement) -> Result<(), LoadError> {
        let mut next = Self::load_with(&self.cache, 6.0, spec)?;
        if matches!(
            (self.spec.chr.as_str(), next.spec.chr.as_str()),
            ("c7100", "c7110") | ("c7110", "c7100")
        ) && let Some(driver) = self.boss_brain.as_mut()
        {
            driver.tally.shared = crate::brain::moves::shared_genichiro_actions(
                &self.cache,
                &self.enemy,
                &next.enemy,
            );
        }
        std::mem::swap(&mut next.enemy.ground, &mut self.enemy.ground);
        next.enemy.body.position = spot.position;
        next.enemy.snap_yaw(spot.yaw);
        next.rules.fighters[0] = self.rules.fighters[0].clone();
        let flags = self.events.as_ref().map(|e| e.scripts.flags.clone());
        if let Some(events) = next.events.as_mut()
            && let Some(flags) = flags
        {
            events.scripts.flags.extend(flags);
            if let Some(previous) = self.events.as_ref() {
                events
                    .scripts
                    .cast_scene
                    .clone_from(&previous.scripts.cast_scene);
                if next.spec.chr == "c5090" && next.spec.npc_param == 50_900_001 {
                    events
                        .scripts
                        .continue_event_from(&previous.scripts, 11_005_812);
                    if crate::butterfly::entrance_effect_cleared(&previous.scripts) {
                        next.rules.fighters[1].clear_resident_effect(3_509_210);
                    }
                }
            }
        }
        self.enemy = next.enemy;
        self.last_steps[1] = StepReport {
            position: self.enemy.body.position,
            yaw: self.enemy.body.yaw,
            grounded: self.enemy.body.grounded,
            ..Default::default()
        };
        let [_, enemy] = next.fighters;
        self.fighters[1] = enemy;
        self.rules = next.rules;
        self.ai = if self.ai.is_some() { next.ai } else { None };
        self.spec = next.spec;
        self.encounter_extra_marks = next.encounter_extra_marks;
        self.encounter_marks = self.encounter_marks.max(next.encounter_marks);
        self.events = next.events;
        self.butterfly_illusions = next.butterfly_illusions;
        if let (Some(encounter), Some(world)) = (&mut self.butterfly_illusions, &self.fog_gate) {
            encounter.place(world);
        }
        self.throws = next.throws;
        self.enemy_active = true;
        self.last_throw_def_clip = None;
        self.paired_floor = None;
        self.enemy_damaged = false;
        self.detector = HitDetector::default();
        self.knockback[1] = None;
        self.hit_stop[1] = HitStop::default();
        self.bullets.clear();
        self.respawn_in = None;
        self.lightning_returned = false;
        self.defender_dying = 0;
        if let Some(mut driver) = self.boss_brain.take() {
            driver.rebind(self);
            if let Some(ai) = self.ai.as_mut()
                && let Some(actions) = crate::brain::BossActions::for_chr(&self.spec.chr)
            {
                ai.enable_act_hook(actions.reflex_acts);
            }
            self.boss_brain = Some(driver);
        }
        Ok(())
    }

    /// Lock on to the enemy if it is within range and roughly in front of the camera.
    fn update_lock(&mut self, input: &InputFrame) {
        let pressed = input.buttons.lock_on && !self.lock_button;
        self.lock_button = input.buttons.lock_on;
        let enemy_alive = self.enemy_present
            && !self.rules.fighters[1].dead()
            && !self.deathblow.as_ref().is_some_and(|d| d.lethal)
            && self.defender_dying == 0;
        // SpEffect stateInfo 350 "cannot be locked on" (the Owl's smoke screen adds 200602 for
        // 1 s) drops the lock and refuses a new one (that it releases a held lock is inferred).
        let unlockable = self.enemy.tae_frame().sp_effects.iter().any(|id| {
            self.enemy
                .tae
                .sp_effects
                .get(id)
                .is_some_and(|e| e.state_info == STATE_INFO_NO_LOCK)
        });
        if unlockable || !enemy_alive {
            self.release_lock();
            return;
        }
        if !pressed {
            // Follow the target while locked on.
            if self.player.lock_target.is_some() {
                self.player.lock_target = enemy_alive.then_some(self.enemy.body.position);
            }
            return;
        }
        if self.player.lock_target.is_some() {
            self.release_lock();
            return;
        }
        let p = self.player.body.position;
        let e = self.enemy.body.position;
        let dist = ((e[0] - p[0]).powi(2) + (e[2] - p[2]).powi(2)).sqrt();
        if dist <= self.camera.lock_range && enemy_alive {
            self.player.lock_target = Some(e);
        }
    }

    /// Drop the target and its strafe selector together. The HKS move loop only seeds
    /// MoveDirection when movement starts, so a held stick otherwise keeps the dead-target
    /// strafe clip while the body starts steering freely.
    fn release_lock(&mut self) {
        if self.player.lock_target.take().is_some() {
            self.player.release_move_lock();
        }
        self.player.behavior.env.locked_on = false;
    }

    /// Keeps the two bodies from overlapping: pushes both apart along the line between them,
    /// each through its own collision so the push never carries one into a wall.
    /// (Character-character collision; the engine uses the Havok character proxies.)
    /// While the enemy still plays the last throw's defender clip (getting up after a deathblow)
    /// only Wolf gives way, so walking into him does not shove him out of his animation (ours,
    /// after FwLo's RustSekiro, where throw pairs skip the push-apart).
    fn separate(&mut self, before: [[f32; 3]; 2]) {
        let body = |f: &Combatant| {
            if f.body_radius > 0.0 {
                f.body_radius
            } else {
                f.radius
            }
        };
        let min = body(&self.fighters[0]) + body(&self.fighters[1]);
        let (p, e) = (self.player.body.position, self.enemy.body.position);
        let (dx, dz) = (e[0] - p[0], e[2] - p[2]);
        let d = (dx * dx + dz * dz).sqrt();
        let ph = crate::collision::CAPSULE_HEIGHT;
        let eh = if self.fighters[1].height > 0.0 {
            self.fighters[1].height
        } else {
            ph
        };
        if d >= min || p[1] >= e[1] + eh || e[1] >= p[1] + ph {
            return;
        }
        let (nx, nz) = if d > 1e-4 {
            (dx / d, dz / d)
        } else {
            (0.0, -1.0)
        };
        // FwLo actor.rs::separate, recorded proxy response: the side that approached
        // absorbs the overlap, so an idle body is blocked rather than shoved.
        let player_came = ((p[0] - before[0][0]) * nx + (p[2] - before[0][2]) * nz).max(0.0);
        let enemy_came = (-(e[0] - before[1][0]) * nx - (e[2] - before[1][2]) * nz).max(0.0);
        let enemy_share = if self.enemy_in_throw_clip() {
            0.0
        } else if player_came + enemy_came > 1e-5 {
            enemy_came / (player_came + enemy_came)
        } else {
            0.5
        };
        let overlap = min - d;
        for (c, share, sign) in [
            (&mut self.player, 1.0 - enemy_share, -1.0),
            (&mut self.enemy, enemy_share, 1.0),
        ] {
            if share <= 0.0 {
                continue;
            }
            let push = overlap * share * sign;
            let [mx, mz] = c.ground.slide(c.body.position, [nx * push, nz * push]);
            c.body.position[0] += mx;
            c.body.position[2] += mz;
            if !c.body.grounded {
                c.body.velocity[0] = 0.0;
                c.body.velocity[2] = 0.0;
            }
        }
    }

    /// Whether the enemy still plays the last deathblow's defender clip (or its death clip).
    fn enemy_in_throw_clip(&self) -> bool {
        let Some(clip) = self.enemy.behavior.runtime.main_clip() else {
            return false;
        };
        let Some(def) = &self.last_throw_def_clip else {
            return false;
        };
        clip.animation == *def
            || anim_id(def)
                .zip(anim_id(&clip.animation))
                .is_some_and(|(d, c)| c == d + 1)
    }

    /// Whether pressing attack now would start a deathblow.
    pub fn deathblow_available(&self) -> bool {
        if let Some(encounter) = &self.screen_monkeys {
            return encounter.deathblow_available(&self.player);
        }
        self.brown_deathblow_available() || self.deathblow_rows().is_some()
    }

    fn deathblow_rows(&self) -> Option<(i32, i32)> {
        if self.spec.chr == "c5200" {
            return self.dragon_finishing_row();
        }
        if self.deathblow.is_none()
            && !self.rules.fighters[0].dead()
            && let Some(throws) = &self.throws
            && let Some(rows) = crate::sword_saint::finishing_rows(
                &self.spec,
                &self.enemy,
                &throws.table,
                &placement(&self.player),
                &placement(&self.enemy),
            )
            .or_else(|| {
                crate::guardian_ape::finishing_rows(
                    &self.spec,
                    &self.enemy,
                    &throws.table,
                    &placement(&self.player),
                    &placement(&self.enemy),
                )
            })
        {
            return Some(rows);
        }
        // Owl's TAE keeps the heat-up judgment effect on throughout the heat-up and kneel.
        // HP 1 here is the interrupted first deathblow's HP floor, not another opening.
        // Leave attacks enabled: his Lua ParryTiming interrupt uses this same effect to
        // request escape 20007, whose TAE restores HP and posture (docs/OWL.md, Phase 2).
        if self.enemy.tae_frame().sp_effects.contains(&OWL_HEAT_UP) {
            return None;
        }
        if crate::guardian_ape::headless(&self.spec)
            && self.rules.fighters[1].vitality.deathblows_left == 0
            && self.rules.fighters[1].vitality.hp <= 1
            && self.last_throw_def_clip.is_some()
        {
            return None;
        }
        // The enemy must have reacted to the hit that broke it first: while it is frozen in hit
        // stop or has not read the damage yet, its script would still send it into the stagger
        // after the throw event (ours; the engine checks the throwable state the stagger sets).
        if self.deathblow.is_some()
            || self.rules.fighters[0].dead()
            || !self.rules.deathblow_open(1)
            || self.hit_stop[1].active()
            || self.enemy.pending_damage.is_some()
            || self.mikiri.as_ref().is_some_and(|pair| {
                !pair.started || !self.player.tae_frame().flag(crate::tae::cancel::DEATHBLOW)
            })
        {
            return None;
        }
        if let Some(clip) = &self.last_throw_def_clip
            && self.mikiri.is_none()
            && self
                .enemy
                .behavior
                .runtime
                .main_clip()
                .is_some_and(|c| &c.animation == clip)
        {
            return None;
        }
        let throws = self.throws.as_ref()?;
        if self.spec.chr == "c5090" && self.spec.npc_param == 50_900_001 {
            return crate::butterfly::second_phase_deathblow(
                &throws.table,
                &placement(&self.player),
                &placement(&self.enemy),
            );
        }
        if crate::guardian_ape::headless(&self.spec) {
            return crate::guardian_ape::headless_rows(
                &throws.table,
                &placement(&self.player),
                &placement(&self.enemy),
            );
        }
        if self.spec.chr == "c5080" && self.lethal_deathblow() {
            return crate::gyoubu::finishing_rows(
                &throws.table,
                &placement(&self.player),
                &placement(&self.enemy),
            );
        }
        let situation = if self.lethal_deathblow() {
            Situation::Final
        } else {
            Situation::PostureBroken
        };
        select(
            &throws.table,
            self.spec.throw_chr(),
            0,
            situation,
            &placement(&self.player),
            &placement(&self.enemy),
        )
        .map(|(s, b)| (s.id, b.id))
    }

    /// Whether a deathblow now would spend the enemy's last mark (NpcParam `ninsatuNum`, 0
    /// counting as one).
    fn lethal_deathblow(&self) -> bool {
        deathblow_marks(self.rules.fighters[1].vitality.deathblows_left) <= 1
    }

    /// Starts a deathblow on the enemy if one is open and Wolf stands in a ThrowParam range.
    /// Fires the throw events into both graphs, as the engine does (the scripts never start
    /// throws themselves).
    fn try_deathblow(&mut self) -> Option<(i32, i32)> {
        if self.spec.chr == "c5200" {
            return self.try_dragon_finish();
        }
        let rows = self.deathblow_rows()?;
        let throws = self.throws.as_ref()?;
        let row = |id: i32| throws.table.rows.iter().find(|r| r.id == id);
        let (start, body) = (row(rows.0)?, row(rows.1)?);
        let mut resolved_body = body.clone();
        if crate::guardian_ape::headless(&self.spec)
            || (self.spec.chr == "c5090" && self.spec.npc_param == 50_900_001)
            || crate::true_monk::owns(&self.spec)
            || self.spec.chr == "c5400"
        {
            resolved_body.def_anim_offset += self
                .enemy
                .behavior
                .runtime
                .offsets
                .by_type
                .get(&THROW_DEF_OFFSET_TYPE)
                .copied()
                .unwrap_or(0) as i32;
        }
        let body = &resolved_body;
        let (attacker, defender) = (placement(&self.player), placement(&self.enemy));
        let defender_clip = body.defender_anim();
        let times = if rows.0 == rows.1
            && (crate::guardian_ape::headless(&self.spec) || self.spec.chr == "c5400")
        {
            crate::guardian_ape::finishing_times(&self.player.tae, body)
        } else {
            ThrowTimes::from_tae_db(
                &self.player.tae,
                &start.attacker_anim(),
                &body.attacker_anim(),
                defender_clip.as_deref().map(|d| (&*self.enemy.tae, d)),
            )
        }?;
        let sorb = if body.atk_sorb_dummy <= 0 && body.def_sorb_dummy > 0 {
            // Wolf is snapped to the enemy's dummy; hold the enemy where that puts it.
            DummyFrame::from_flver(throws.enemy_model.as_ref()?, body.def_sorb_dummy)?.inverse()
        } else {
            DummyFrame::from_flver(&throws.model, body.atk_sorb_dummy)?
        };
        let (start, body) = (start.clone(), body.clone());
        let body_duration = self.player.clip_duration(&body.attacker_anim())?;
        let marks = self.rules.fighters[1].vitality.deathblows_left;
        // Whether the enemy's TAE has the death clip (env 1114's "does the animation exist").
        let death_clip = body
            .defender_death_anim()
            .and_then(|clip| sekiro_formats::tae::anim_id(&clip))
            .is_some_and(|id| self.enemy.behavior.env.existing_anims.contains(&id));
        let (mut db, first) = Deathblow::begin(
            (&start, &body),
            times,
            sorb,
            body_duration,
            deathblow_marks(marks),
        );
        db.death_clip = death_clip && rows.0 != rows.1;
        if body.atk_sorb_dummy <= 0 && body.def_sorb_dummy > 0 {
            db.absorbed = Absorbed::Attacker;
        }
        self.last_throw_def_clip = body.defender_anim();
        if let Some(yaw) = db.start_facing(&attacker, &defender) {
            self.player.snap_yaw(yaw);
        }
        if let Some(clip) = first.attacker_clip.as_deref().and_then(anim_id) {
            let rt = &mut self.player.behavior.runtime;
            rt.offsets
                .by_type
                .insert(THROW_ATK_OFFSET_TYPE, start.atk_anim_offset as u32);
            rt.fire_event(&format!("W_ThrowAtk{clip}"));
        }
        self.release_lock();
        // The throw takes over both bodies: a knockback left from the breaking hit must not
        // resume (and shove the enemy) once the pair ends.
        self.knockback = [None, None];
        self.deathblow = Some(db);
        // The deathblow replaces the counter pair; its slide must not move the new throw.
        self.mikiri = None;
        self.defender_dying = 0;
        Some(rows)
    }

    /// Advances the deathblow in progress after both characters moved this step.
    fn step_deathblow(&mut self, dt: f32, report: &mut DeathblowReport) {
        let Some(db) = self.deathblow.as_mut() else {
            return;
        };
        let step = db.step(dt, &placement(&self.player), &placement(&self.enemy));
        let pair_start = step.attacker_clip.is_some() && db.phase == Phase::Body;
        let alignment = pair_start.then(|| {
            let p = placement(&self.player);
            let e = placement(&self.enemy);
            let mut aligned = [p.position, e.position];
            match db.absorbed {
                Absorbed::Defender => aligned[1] = db.sorb.placement(&p).position,
                Absorbed::Attacker => aligned[0] = db.sorb.inverse().placement(&e).position,
            }
            // Sorb helper Y is a model offset; the existing floor placement stays authoritative.
            aligned[0][1] = p.position[1];
            aligned[1][1] = e.position[1];
            aligned
        });
        let path = pair_start.then(|| throw_body_path(db, &self.player, &self.enemy));
        if let Some(clip) = step.attacker_clip.as_deref().and_then(anim_id) {
            let rt = &mut self.player.behavior.runtime;
            rt.offsets
                .by_type
                .insert(THROW_ATK_OFFSET_TYPE, db.body.atk_anim_offset as u32);
            rt.fire_event(&format!("W_ThrowAtk{clip}"));
        }
        if let Some(clip) = &step.defender_clip {
            let death = db.body.defender_death_anim().as_deref() == Some(clip.as_str());
            if let Some(id) = anim_id(clip) {
                if death {
                    self.defender_dying = 1;
                }
                self.enemy
                    .behavior
                    .runtime
                    .offsets
                    .by_type
                    .insert(THROW_DEF_OFFSET_TYPE, db.body.def_anim_offset as u32);
                let event = if death {
                    format!("W_ThrowDefDeath{id}")
                } else {
                    format!("W_ThrowDef{id}")
                };
                self.enemy.behavior.runtime.fire_event(&event);
            }
        }
        let left_clip = db.phase == Phase::Body
            && step.attacker_clip.is_none()
            && db.time > dt
            && self
                .player
                .behavior
                .runtime
                .main_clip()
                .is_none_or(|c| c.animation != db.body.attacker_anim());
        if left_clip {
            // The pair ends when Wolf leaves his body clip (its TAE opens move and action
            // cancels before the end); the enemy plays its own clip out.
            db.release();
        }
        if step.hit {
            self.rules.deathblow(1);
            report.hit = true;
            report.killed = step.killed;
            if self.events.is_some() && crate::guardian_ape::false_death(&self.spec) {
                crate::guardian_ape::decapitated(&mut self.rules.fighters[1].vitality);
                report.killed = false;
            }
            if self.events.is_some() && crate::guardian_ape::headless(&self.spec) && step.killed {
                crate::guardian_ape::decapitated(&mut self.rules.fighters[1].vitality);
                report.killed = false;
            }
            if step.killed && !db.death_clip {
                // The defender clip is itself the death: the script may see HP 0 from now on.
                self.defender_dying = self.defender_dying.max(1);
            }
        }
        if matches!(db.phase, Phase::Done | Phase::Missed) {
            self.deathblow = None;
        }
        if let Some(aligned) = alignment {
            let starts = [self.player.body.position, self.enemy.body.position];
            let mut checks = vec![starts, aligned, starts];
            // Include the fully absorbed endpoint even if the strike occurs before its end.
            checks.extend(path.unwrap_or_default());
            self.reserve_pair_path(&checks);
        }
        if let Some(a) = step.absorb {
            self.absorb(&a);
        }
    }

    /// The enemy's newly applied lasting SpEffects replace the active ones of their category
    /// ([`crate::sp_category`]), resident NpcParam effects included. Only the Demon of Hatred
    /// uses it so far: the rule is inferred and other bosses' regressions are tuned without it.
    fn apply_sp_categories(&mut self) {
        let applied = self.enemy.tae_runtime.take_applied();
        if !crate::demon::owns(&self.spec) {
            return;
        }
        for id in applied {
            let active: Vec<i32> = self.rules.fighters[1]
                .resident()
                .iter()
                .copied()
                .chain(self.enemy.tae_runtime.timed_ids())
                .collect();
            for old in crate::sp_category::replaced(&self.enemy.tae, id, active) {
                self.rules.fighters[1].clear_resident_effect(old);
                self.enemy.tae_runtime.clear_timed(old);
            }
        }
    }

    fn reserve_pair(&mut self, mut aligned: [[f32; 3]; 2]) {
        let starts = [self.player.body.position, self.enemy.body.position];
        for i in 0..2 {
            aligned[i][1] = starts[i][1];
        }
        self.reserve_pair_path(&[starts, aligned]);
    }

    fn reserve_pair_path(&mut self, path: &[[[f32; 3]; 2]]) {
        if let Some(shift) = crate::collision::throw_pair_path_shift(
            [self.player.ground.as_ref(), self.enemy.ground.as_ref()],
            path,
        ) {
            if shift[0].hypot(shift[1]) > 0.01 {
                eprintln!(
                    "throw pair wall clearance: x={:.3} z={:.3}",
                    shift[0], shift[1]
                );
            }
            for c in [&mut self.player, &mut self.enemy] {
                c.body.position[0] += shift[0];
                c.body.position[2] += shift[1];
                // A shift over sloped ground must not leave the body hanging (a fall start
                // would cancel the throw clip): follow a floor within a step's height.
                if shift[0].hypot(shift[1]) > 0.01
                    && let Some(y) = c.ground.floor(
                        c.body.position[0],
                        c.body.position[2],
                        c.body.position[1] + 1.0,
                    )
                    && (y - c.body.position[1]).abs() <= 1.0
                {
                    c.body.position[1] = y;
                }
            }
        } else {
            eprintln!("throw pair clearance exhausted; applying safe per-step placement");
        }
    }

    /// Applies authored absorption without independent wall pushes after reserving pair space.
    fn absorb(&mut self, a: &AbsorbStep) {
        // Wolf is the deathblow's attacker.
        self.slide_pair(a.who == Absorbed::Attacker, a);
    }

    /// [`Self::absorb`] for either character: `player_moves` says whether Wolf is the absorbed one.
    fn slide_pair(&mut self, player_moves: bool, a: &AbsorbStep) {
        let moved = if player_moves {
            &mut self.player
        } else {
            &mut self.enemy
        };
        moved.body.position[0] += a.delta[0];
        moved.body.position[2] += a.delta[2];
        moved.snap_yaw(moved.body.yaw + a.yaw);
    }

    /// Validate actual paired travel too: grabs, velocity events and lethal branches are
    /// not all covered by the deathblow's preflight root-motion probe.
    fn constrain_pair_motion(
        &mut self,
        before: [crate::body::Body; 2],
        actors: [bool; 2],
        sweep_motion: bool,
    ) {
        let mut starts = before.clone().map(|b| b.position);
        for (i, c) in [&self.player, &self.enemy].into_iter().enumerate() {
            starts[i][1] = c
                .ground
                .floor(
                    starts[i][0],
                    starts[i][2],
                    starts[i][1] + c.ground.max_step(),
                )
                .unwrap_or(starts[i][1]);
        }
        let mut ends = [self.player.body.position, self.enemy.body.position];
        let floors = *self.paired_floor.get_or_insert(starts.map(|p| p[1]));
        for i in 0..2 {
            starts[i][1] = floors[i];
            ends[i][1] = starts[i][1];
        }
        let indices = if actors == [true, true] {
            [0, 1]
        } else if actors[0] {
            [0, 0]
        } else {
            [1, 1]
        };
        let characters = [&self.player, &self.enemy];
        let solve = if sweep_motion {
            crate::collision::throw_pair_step_shift
        } else {
            crate::collision::throw_pair_floor_step_shift
        };
        let candidate = solve(
            indices.map(|i| characters[i].ground.as_ref()),
            &[indices.map(|i| starts[i]), indices.map(|i| ends[i])],
        );
        let relocating = self
            .deathblow
            .as_ref()
            .is_some_and(|d| d.phase == Phase::Body && d.time == 0.0);
        let shift = candidate.filter(|s| {
            // A small correction added to root travel must not become a large jump.
            // Full preflight relocation is already part of `ends`, not this correction.
            relocating
                || s[0].hypot(s[1]) < 1e-4
                || indices.into_iter().all(|i| {
                    let dx = ends[i][0] - starts[i][0];
                    let dz = ends[i][2] - starts[i][2];
                    (dx + s[0]).hypot(dz + s[1]) <= dx.hypot(dz).max(0.2)
                })
        });
        for (i, c) in [&mut self.player, &mut self.enemy].into_iter().enumerate() {
            if !actors[i] {
                continue;
            }
            if let Some(shift) = shift {
                c.body.position[0] += shift[0];
                c.body.position[2] += shift[1];
                if shift[0].hypot(shift[1]) > 1e-4
                    && before[i].grounded
                    && c.body.velocity[1] <= 0.0
                {
                    c.body.position[1] = c
                        .ground
                        .floor(
                            c.body.position[0],
                            c.body.position[2],
                            starts[i][1] + c.ground.max_step(),
                        )
                        .unwrap_or(starts[i][1]);
                    c.body.grounded = true;
                    c.body.velocity[1] = 0.0;
                }
            } else {
                // Cramped space cannot justify moving either capsule through geometry.
                // Keep the last safe feet while the paired clips and turning continue.
                let yaw = c.body.yaw;
                c.body = before[i].clone();
                c.snap_yaw(yaw);
            }
        }
    }

    /// Whether an enemy throw holds Wolf.
    fn throw_held(&self) -> bool {
        self.mikiri.is_some()
            || self
                .enemy_throw
                .as_ref()
                .is_some_and(|t| t.phase == GrabPhase::Held)
    }

    fn try_mikiri(&mut self, hit: &HitEvent) -> Option<(i32, i32)> {
        if hit.attacker != 1
            || hit.defender != 0
            || !self.player.tae_frame().flag(crate::perilous::FLAG_MIKIRI)
            || !(hit.attack.atk_type == 2
                && hit.attack.perilous() == Some(crate::hits::Perilous::Thrust)
                || crate::perilous::counter_dummy(&hit.attack)
                    && crate::perilous::upcoming_kind(&self.enemy, &self.fighters[1])
                        == Some(crate::hits::Perilous::Thrust))
        {
            return None;
        }
        let throws = self.throws.as_ref()?;
        let damage = hit.attack.mikiri_posture;
        let forward = placement(&self.player).forward();
        let to_enemy = [
            self.enemy.body.position[0] - self.player.body.position[0],
            self.enemy.body.position[2] - self.player.body.position[2],
        ];
        if forward[0] * to_enemy[0] + forward[1] * to_enemy[1] <= 0.0 {
            return None;
        }
        let broken = self.rules.fighters[1].posture.project(-damage, None) < 1;
        let row = throws
            .table
            .player_row(self.spec.throw_chr(), 0, if broken { 120 } else { 190 })?
            .clone();
        self.player.clip_duration(&row.attacker_anim())?;
        let absorbed = if row.atk_sorb_dummy <= 0 && row.def_sorb_dummy > 0 {
            Absorbed::Attacker
        } else {
            Absorbed::Defender
        };
        let sorb = if absorbed == Absorbed::Attacker {
            DummyFrame::from_flver(throws.enemy_model.as_ref()?, row.def_sorb_dummy)?.inverse()
        } else {
            DummyFrame::from_flver(&throws.model, row.atk_sorb_dummy)?
        };
        let yaw = self.player.yaw_to(self.enemy.body.position);
        self.player.snap_yaw(yaw);
        self.enemy
            .snap_yaw(self.enemy.yaw_to(self.player.body.position));
        let slide = crate::deathblow::AbsorbSlide::new(
            absorbed,
            &sorb,
            &placement(&self.player),
            &placement(&self.enemy),
            row.interpolation_time,
        );
        let aligned = if absorbed == Absorbed::Attacker {
            [
                sorb.inverse().placement(&placement(&self.enemy)).position,
                self.enemy.body.position,
            ]
        } else {
            [
                self.player.body.position,
                sorb.placement(&placement(&self.player)).position,
            ]
        };
        self.reserve_pair(aligned);
        self.rules.fighters[1].posture.take(damage, None);
        self.player
            .behavior
            .runtime
            .offsets
            .by_type
            .insert(THROW_ATK_OFFSET_TYPE, row.atk_anim_offset as u32);
        self.player
            .behavior
            .runtime
            .fire_event(&format!("W_ThrowAtk{}", row.atk_anim));
        self.enemy
            .behavior
            .runtime
            .offsets
            .by_type
            .insert(THROW_DEF_OFFSET_TYPE, row.def_anim_offset as u32);
        self.enemy
            .behavior
            .runtime
            .fire_event(&format!("W_ThrowDef{}", row.def_anim));
        self.last_throw_def_clip = row.defender_anim();
        self.knockback = [None, None];
        self.player.pending_damage = None;
        self.enemy.pending_damage = None;
        let result = (row.id, damage);
        self.mikiri = Some(crate::perilous::Mikiri {
            row,
            slide,
            started: false,
        });
        Some(result)
    }

    fn step_mikiri(&mut self, dt: f32) {
        let Some(mut pair) = self.mikiri.take() else {
            return;
        };
        let playing = self
            .player
            .behavior
            .runtime
            .main_clip()
            .is_some_and(|c| c.animation == pair.row.attacker_anim());
        if playing {
            pair.started = true;
        }
        if pair.started && !playing {
            // A counter reaction is not a deathblow get-up lock after Wolf cancels out.
            if self.last_throw_def_clip == pair.row.defender_anim() {
                self.last_throw_def_clip = None;
            }
            return;
        }
        if let Some(a) = pair.slide.advance(dt) {
            self.absorb(&a);
        }
        self.mikiri = Some(pair);
    }

    /// [`Self::force_act`]: asks for the act whenever the enemy is not already in it or in a
    /// throw, facing Wolf.
    fn force_enemy_act(&mut self, enemy_alive: bool) {
        let Some(act) = self.force_act else {
            return;
        };
        let clip = format!("a000_{act:06}");
        let playing = self
            .enemy
            .behavior
            .runtime
            .main_clip()
            .is_some_and(|c| c.animation == clip);
        if !enemy_alive || playing || self.throw_held() || self.deathblow.is_some() {
            return;
        }
        self.enemy.npc.action = act;
        self.enemy.npc.combo = false;
        self.enemy.npc.face_yaw = Some(self.enemy.yaw_to(self.player.body.position));
    }

    /// Starts the enemy's throw on Wolf from a throw-start contact (AtkParam `throwFlag` 1):
    /// picks the ThrowParam row, checks its range, fires the throw events into both graphs and
    /// begins the absorb. Returns the row. A contact the row's range refuses does nothing
    /// (inferred: the engine's handling of a failed throw check is not traced).
    fn try_enemy_throw(&mut self, hit: &HitEvent) -> Option<i32> {
        if self.enemy_throw.is_some()
            || self.deathblow.is_some()
            || self.rules.fighters.iter().any(|f| f.dead())
        {
            return None;
        }
        let throws = self.throws.as_ref()?;
        let row = throws
            .table
            .npc_row(self.spec.throw_chr(), hit.attack.throw_type_id)?
            .clone();
        let (attacker, defender) = (placement(&self.enemy), placement(&self.player));
        if !row.in_range(&attacker, &defender) {
            return None;
        }
        let sorb = if row.atk_sorb_dummy > 0 {
            DummyFrame::from_flver(throws.enemy_model.as_ref()?, row.atk_sorb_dummy)?
        } else {
            // Only Wolf's dummy is named: the enemy stands on it.
            DummyFrame::from_flver(&throws.model, row.def_sorb_dummy)?.inverse()
        };
        let attacker = Placement {
            yaw: if row.turn_attacker {
                attacker.yaw_towards(defender.position)
            } else {
                attacker.yaw
            },
            ..attacker
        };
        let throw = EnemyThrow::begin(
            &row,
            &self.enemy.tae,
            &self.player.tae,
            sorb,
            &attacker,
            &defender,
        )?;
        self.enemy.snap_yaw(attacker.yaw);
        let aligned = if row.atk_sorb_dummy <= 0 && row.def_sorb_dummy > 0 {
            [
                defender.position,
                sorb.inverse().placement(&defender).position,
            ]
        } else {
            [sorb.placement(&attacker).position, attacker.position]
        };
        self.reserve_pair(aligned);
        let rt = &mut self.enemy.behavior.runtime;
        rt.offsets
            .by_type
            .insert(THROW_DEF_OFFSET_TYPE, row.atk_anim_offset as u32);
        rt.fire_event(&format!("W_ThrowAtk{}", row.atk_anim));
        self.player
            .behavior
            .runtime
            .fire_event(&format!("W_ThrowDef{}", row.def_anim));
        // The throw takes over both bodies, as a deathblow does.
        self.knockback = [None, None];
        self.enemy_throw = Some(throw);
        Some(row.id)
    }

    /// Advances the enemy throw after both characters moved this step: the absorb, the damage at
    /// the enemy clip's `ThrowAttackBehavior` events, and the death branch.
    fn step_enemy_throw(&mut self, dt: f32, report: &mut GrabReport) {
        let Some(mut throw) = self.enemy_throw.take() else {
            return;
        };
        let clip = |c: &Character| {
            c.behavior
                .runtime
                .main_clip()
                .map(|c| (c.animation.clone(), c.time))
        };
        let (enemy_clip, wolf_clip) = (clip(&self.enemy), clip(&self.player));
        let step = throw.step(
            dt,
            enemy_clip.as_ref().map(|(c, t)| (c.as_str(), *t)),
            wolf_clip.as_ref().map(|(c, t)| (c.as_str(), *t)),
        );
        if let Some(a) = step.absorb {
            // Wolf is the throw's defender.
            self.slide_pair(a.who == Absorbed::Defender, &a);
        }
        for judge in step.hits {
            let Some(attack) = self.fighters[1].attacks.get(&judge).cloned() else {
                continue;
            };
            let (p, e) = (self.player.body.position, self.enemy.body.position);
            let (dx, dz) = (p[0] - e[0], p[2] - e[2]);
            let len = dx.hypot(dz).max(1e-4);
            let event = HitEvent {
                attacker: 1,
                defender: 0,
                judge,
                attack,
                position: p,
                direction: [dx / len, dz / len],
            };
            // HP and posture as a direct hit of the throw's row (`throwFlag` 2); the thrown clip
            // already shows the impact, so no reaction, knockback or hit stop.
            let result = self.rules.resolve(
                &event,
                [&self.player, &self.enemy],
                [&self.fighters[0], &self.fighters[1]],
            );
            report
                .hits
                .push((judge, result.hp_damage, result.posture_damage));
            throw.lethal |= self.rules.fighters[0].dead();
        }
        if step.death {
            self.player
                .behavior
                .runtime
                .fire_event(&format!("W_ThrowDefDeathStart{}", throw.row.def_anim));
        }
        if throw.phase != GrabPhase::Done {
            self.enemy_throw = Some(throw);
        }
    }

    /// Passes HP and posture to both scripts (env 1000, 1001, 2010). While a deathblow holds the
    /// enemy its script still sees it alive, so the throw animations (with their own death
    /// branch) play out instead of the script's ordinary death.
    /// env 2004 ("swing is hitting", `docs/HKS-INTERFACE.md`) for Wolf's script: true while the
    /// clip in which one of his attacks connected still plays. The script reads it only together
    /// with SpEffect reference 204 (the air kick's window for kicking off the enemy into
    /// `AirKickEnemyJumpStart`, as FwLo's RustSekiro `BEH_R_ENEMY_JUMP`) or a combat art's jump
    /// reference; that it stays true for the rest of the clip is ours.
    fn publish_swing_hit(&mut self) {
        let playing = self
            .player
            .behavior
            .runtime
            .main_clip()
            .map(|c| c.animation.clone());
        if self.swing_hit.is_some() && self.swing_hit != playing {
            self.swing_hit = None;
        }
        let env = &mut self.player.behavior.env;
        env.overrides.retain(|(id, _, _)| *id != ENV_SWING_HIT);
        if self.swing_hit.is_some() {
            env.overrides.push((ENV_SWING_HIT, None, 1.0));
        }
    }

    /// Notes Wolf's landed attacks of this step for env 2004, and returns whether one was an
    /// anti-ground hit (the air kick) on an enemy whose active SpEffects say "jump kick
    /// effective" (a perilous sweep, [`crate::perilous::STATE_INFO_JUMP_KICK_EFFECTIVE`]).
    fn note_player_hits(&mut self, hits: &[(HitEvent, HitResult)]) -> bool {
        let mut counter = false;
        for (hit, _) in hits.iter().filter(|(h, _)| h.attacker == 0) {
            self.swing_hit = self
                .player
                .behavior
                .runtime
                .main_clip()
                .map(|c| c.animation.clone());
            let anti_ground = hit
                .attack
                .profile
                .as_ref()
                .is_some_and(|p| p.stamina_attribute.0 == crate::perilous::STAMINA_ANTI_GROUND);
            counter |= anti_ground
                && self
                    .rules
                    .active_effects(hit.defender, self.chr(hit.defender))
                    .iter()
                    .any(|e| e.state_info == crate::perilous::STATE_INFO_JUMP_KICK_EFFECTIVE);
        }
        counter
    }

    /// Character `i`: 0 Wolf, 1 the enemy.
    fn chr(&self, i: usize) -> &Character {
        if i == 0 { &self.player } else { &self.enemy }
    }

    /// World position of dummy poly `id` of character `i` (0 Wolf, 1 the enemy) in its pose as
    /// of the last step: the camera's look-at target (Wolf's 142 in deathblows) and the kanji's
    /// spot (Wolf's 220).
    pub fn dummy_position(&self, i: usize, id: i16) -> Option<[f32; 3]> {
        let c = self.chr(i);
        self.fighters[i]
            .rig
            .positions(id, &c.bone_source_matrices(), c.source_to_world())
            .first()
            .map(|p| p.to_array())
    }

    /// A dummy poly's world frame as of the last step: position, forward and up (unit length).
    /// Effects ride on these (a blood spray leaves along its dummy's forward).
    pub fn dummy_frame(&self, i: usize, id: i16) -> Option<([f32; 3], [f32; 3], [f32; 3])> {
        let c = self.chr(i);
        self.fighters[i]
            .rig
            .frames(id, &c.bone_source_matrices(), c.source_to_world())
            .first()
            .map(|f| {
                (
                    f[0].to_array(),
                    f[1].normalize_or(glam::Vec3::Z).to_array(),
                    f[2].normalize_or(glam::Vec3::Y).to_array(),
                )
            })
    }

    fn publish_vitals(&mut self) {
        // The script sees HP 0 one step after the death event, once its graph is in the death state.
        let held = self.deathblow.is_some() && self.defender_dying < 2;
        // Wolf's script sees him alive until a lethal throw has his graph in its death state, so
        // the thrown clip's own death branch plays instead of the ordinary death.
        let thrown = self.enemy_throw.as_ref().is_some_and(|t| {
            t.phase == GrabPhase::Held
                && (!t.branched || !self.player.state_name().starts_with("ThrowDefDeath"))
        });
        let count = self.fighter_count();
        for (i, (c, f)) in [&mut self.player, &mut self.enemy]
            .into_iter()
            .zip(&self.rules.fighters)
            .take(count)
            .enumerate()
        {
            let hp = f.vitality.hp as f32;
            c.behavior.env.hp = if (held && i == 1)
                || (thrown && i == 0)
                || (i == 1 && crate::demon::execution_owns_death(&self.spec, c))
                || (i == 1 && crate::true_monk::finishing_message_pending(&self.spec, c))
                || (i == 1
                    && crate::sword_saint::execution_owns_death(
                        &self.spec,
                        c,
                        f.vitality.deathblows_left,
                    )) {
                hp.max(1.0)
            } else {
                hp
            };
            c.behavior.env.posture = f.posture.remaining as f32;
            c.behavior.env.max_posture = f.posture.max as f32;
        }
    }

    /// Steps character `i` by `dt` seconds of its own time, or holds it frozen (hit stop) when
    /// `dt` is 0: no animation, script, TAE or body update, and no TAE one-shots (sounds,
    /// velocity changes) repeated. The player's buttons are still read, so a press during the
    /// freeze is buffered like any other.
    fn advance_character(
        &mut self,
        i: usize,
        input: &InputFrame,
        real_dt: f32,
        dt: f32,
    ) -> StepReport {
        let c = if i == 0 {
            &mut self.player
        } else {
            &mut self.enemy
        };
        if dt > 0.0 {
            let report = c.tick(input, dt);
            self.last_steps[i] = report.clone();
            return report;
        }
        if i == 0 {
            c.input.update(input, real_dt, &c.tae_runtime.frame);
        }
        let frame = &mut c.tae_runtime.frame;
        frame.sounds.clear();
        frame.velocity_changes.clear();
        let mut report = self.last_steps[i].clone();
        report.clip_started = false;
        report.blend_in = 0.0;
        report.advanced = 0.0;
        report.requested.clear();
        report.behavior.events.clear();
        report.behavior.hooks.clear();
        report.behavior.script_errors.clear();
        report
    }

    /// Moves character `i` along its knockback for `dt` seconds of its own time, through its
    /// collision so walls stop the push.
    fn push(&mut self, i: usize, dt: f32) {
        let Some(kb) = self.knockback[i].as_mut() else {
            return;
        };
        let delta = kb.advance(dt);
        if kb.finished() {
            self.knockback[i] = None;
        }
        let c = if i == 0 {
            &mut self.player
        } else {
            &mut self.enemy
        };
        let [mx, mz] = c.ground.slide(c.body.position, delta);
        c.body.position[0] += mx;
        c.body.position[2] += mz;
    }

    /// Leaves a bullet's SpEffects on character `i`: build-up and timed effects in its status
    /// ([`crate::status`]), the timed ones also in its TAE runtime so its scripts and AI see them.
    fn apply_status(&mut self, i: usize, ids: &[i32]) {
        let applied = self.rules.apply_hit_effects(i, ids);
        let c = if i == 0 {
            &mut self.player
        } else {
            &mut self.enemy
        };
        for (id, seconds) in applied.timed {
            c.tae_runtime.add_timed(id, seconds);
        }
        // The weak/strong lightning hit effects point at resistance-damage rows.
        // Unlike poison this is a single discharge, not damage over time.
        if ids.contains(&9400) {
            self.discharge_lightning(i, 9420);
        }
        let grounded = if i == 0 {
            self.player.body.grounded
        } else {
            self.enemy.body.grounded
        };
        if grounded && let Some(id) = ids.iter().find(|id| (9490..=9495).contains(*id)) {
            self.discharge_lightning(i, 9430 + (*id - 9490));
        }
    }

    fn discharge_lightning(&mut self, i: usize, damage_effect: i32) {
        let Some(effect) = self.rules.statuses.get(damage_effect).copied() else {
            return;
        };
        let damage = effect.tick_damage(self.rules.fighters[i].vitality.max_hp);
        let f = &mut self.rules.fighters[i];
        f.vitality.set_hp(f.vitality.hp - damage);
        let posture = (f.posture.max as f32 * effect.change_stamina_rate / 100.0
            + effect.change_stamina_point as f32) as i32;
        f.posture.remaining = (f.posture.remaining + posture).clamp(0, f.posture.max);
        f.status.running.retain(|r| !(9490..=9499).contains(&r.id));
        let applied = self.rules.apply_hit_effects(i, &[9445]);
        let c = if i == 0 {
            &mut self.player
        } else {
            &mut self.enemy
        };
        for id in 9490..=9499 {
            c.tae_runtime.clear_timed(id);
        }
        for (id, seconds) in applied.timed {
            c.tae_runtime.add_timed(id, seconds);
        }
        // A one-tick behaviour notification selects the shipped shock reaction.
        c.tae_runtime.add_timed(9450, 2.0 / 60.0);
        c.pending_damage = Some(crate::player::IncomingDamage {
            damage_type: 99999,
            level: 3,
            element: 10,
            ..Default::default()
        });
    }

    fn step_lightning(&mut self) {
        for i in 0..self.fighter_count() {
            let c = if i == 0 { &self.player } else { &self.enemy };
            let released = c
                .tae_frame()
                .sp_effects
                .iter()
                .any(|id| matches!(id, 9505 | 9506 | 9500));
            let charge = self.rules.fighters[i]
                .status
                .running
                .iter()
                .find(|r| (9490..=9495).contains(&r.id))
                .map(|r| r.id);
            if let Some(id) = charge {
                if released {
                    self.rules.fighters[i].status.running.retain(|r| r.id != id);
                    let c = if i == 0 {
                        &mut self.player
                    } else {
                        &mut self.enemy
                    };
                    c.tae_runtime.clear_timed(id);
                } else if c.body.grounded {
                    self.discharge_lightning(i, 9430 + (id - 9490));
                }
            }
        }
    }

    /// Leaves a melee hit's AtkParam SpEffects on character `i`: as [`Self::apply_status`], and
    /// effects with no duration (applied once, such as the Owl's kick marker 3506000 on Wolf)
    /// stay visible for [`INSTANT_EFFECT_STEPS`] steps so the AI's observers see them.
    fn apply_hit_effects(&mut self, i: usize, ids: &[i32], dt: f32) {
        self.apply_status(i, ids);
        for &id in ids {
            let c = if i == 0 { &self.player } else { &self.enemy };
            if c.tae.sp_effects.get(&id).is_some_and(|e| e.duration == 0.0) {
                let c = if i == 0 {
                    &mut self.player
                } else {
                    &mut self.enemy
                };
                c.tae_runtime.add_timed(id, INSTANT_EFFECT_STEPS * dt);
            }
        }
    }

    /// Whether the enemy's health bar is up: always without map events; with them, while the
    /// events show the boss bar or the overhead bar.
    pub fn enemy_bar_shown(&self) -> bool {
        self.events
            .as_ref()
            .is_none_or(|e| e.boss_bar.is_some() || e.hp_bar)
    }

    /// Encounter marks the HUD draws over the enemy's health bar (see
    /// [`crate::deathblow::bar_marks`]: none on an enemy taking a single deathblow); combat
    /// still spends only the active entity's own marks.
    pub fn enemy_bar_marks(&self) -> i32 {
        if old_dragons::owns(&self.spec) {
            return 0;
        }
        if self.screen_monkeys.is_some() {
            return 0;
        }
        let remaining = self.rules.fighters[1].vitality.deathblows_left;
        let left = if self.spec.chr == "c5090" {
            remaining.max(0) + self.encounter_extra_marks
        } else {
            deathblow_marks(remaining)
        };
        crate::deathblow::bar_marks(left, self.encounter_marks)
    }

    /// What the enemy's script reads from the engine about events, throws and endure requests,
    /// before it runs: the waiting event command (`env(105, slot)`), "in a throw" (`env(206)`),
    /// the AI's endure-cancel action (`env(123)`), and, in an event animation, the move cancel
    /// and standby states without the clip-end stand-in (see below).
    fn publish_event_envs(&mut self, endure_action: Option<i32>) {
        self.player
            .behavior
            .env
            .overrides
            .retain(|(id, _, _)| *id != ENV_EVENT_COMMAND);
        if let Some((slot, command)) = self.events.as_ref().and_then(|e| e.player_ezstate) {
            self.player.behavior.env.overrides.push((
                ENV_EVENT_COMMAND,
                Some(i32::from(slot)),
                command as f32,
            ));
        }
        if !self.enemy_present {
            return;
        }
        let in_throw = self.deathblow.is_some() || self.enemy_in_throw_clip();
        let command = self
            .events
            .as_ref()
            .and_then(|e| e.ezstate)
            .or(self.auxiliary_ezstate)
            .filter(|(_, command)| {
                !crate::isshin::defer_finishing_request(&self.spec, &self.enemy, *command)
                    && !(*command == 20200
                        && crate::true_monk::finishing_message_pending(&self.spec, &self.enemy))
            });
        // Event animations (the graph's `Event*` states, such as the Owl's heat-up) end through
        // their own state's end branch (`env(339)`, `W_Event21003` after `a000_020005`); their
        // TAE opens no move window. Our clip-end stand-in for env 2000 / 1105 would hand them to
        // the AI's turn or move at the end instead (ours).
        let frame = self.enemy.tae_frame();
        let event_anim = self.enemy.state_name().starts_with("Event")
            && frame.restricted
            && !frame.flag(crate::tae::cancel::MOVE)
            && !frame.flag(AI_MOVE_FLAG);
        let env = &mut self.enemy.behavior.env;
        env.overrides.retain(|(id, _, _)| {
            ![
                ENV_IN_THROW,
                273,
                ENV_EVENT_COMMAND,
                ENV_AI_ENDURE_ACTION,
                ENV_AI_IMMEDIATE_ACTION,
                crate::player::ENV_MOVE_CANCEL,
                crate::player::ENV_STANDBY_STATE,
            ]
            .contains(id)
        });
        if in_throw {
            env.overrides.push((ENV_IN_THROW, None, 1.0));
        }
        if let Some(pair) = &self.mikiri {
            env.overrides.push((273, None, pair.row.def_anim as f32));
        }
        if let Some(a) = endure_action.filter(|&a| a > 0) {
            env.overrides.push((ENV_AI_ENDURE_ACTION, None, a as f32));
        }
        if self
            .ai
            .as_ref()
            .is_some_and(|ai| ai.state.request.immediate)
            && self.enemy.npc.action > 0
        {
            env.overrides
                .push((ENV_AI_IMMEDIATE_ACTION, None, self.enemy.npc.action as f32));
        }
        if event_anim {
            env.overrides
                .push((crate::player::ENV_MOVE_CANCEL, None, 0.0));
            env.overrides
                .push((crate::player::ENV_STANDBY_STATE, None, 0.0));
        }
        if let Some((slot, command)) = command {
            env.overrides
                .push((ENV_EVENT_COMMAND, Some(slot as i32), command as f32));
        }
    }

    /// Runs the map events for this step and applies what they ask.
    fn step_events(&mut self, dt: f32) {
        let Some(mut ev) = self.events.take() else {
            return;
        };
        // A waiting command is used up once the script played an event animation for it.
        if ev.player_ezstate.is_some()
            && self.last_steps[0]
                .behavior
                .events
                .iter()
                .any(|e| e.starts_with("W_Event"))
        {
            ev.player_ezstate = None;
        }
        if ev.ezstate.is_some()
            && self.last_steps[1]
                .behavior
                .events
                .iter()
                .any(|e| e.starts_with("W_Event"))
        {
            ev.ezstate = None;
        }
        ev.sword_saint_finish
            .release(&mut self.enemy, &mut ev.ezstate);
        let initializing = !ev.initialized;
        if initializing && self.spec.chr == "c5430" && self.isshin_fire.is_none() {
            match crate::isshin_fire::FireEmitter::load(&self.cache, &self.fighters[0]) {
                Ok(emitter) => self.isshin_fire = Some(emitter),
                Err(error) => eprintln!("Shura fire emitter not loaded: {error}"),
            }
        }
        ev.initialized = true;
        let commands = ev.scripts.step(
            dt,
            &EventView {
                duel: self,
                boss: ev.boss_entity,
            },
        );
        if (matches!(self.spec.chr.as_str(), "c5090" | "c5200")
            || crate::owl_father::owns(&self.spec))
            && let Some((animation, _)) = ev.scripts.cast_scene.animations.remove(&ev.boss_entity)
            && animation >= 0
        {
            self.enemy
                .behavior
                .runtime
                .fire_event(&format!("W_Event{animation}"));
        }
        if let Some(region) = ev.scripts.cast_scene.warps.remove(&ev.boss_entity)
            && let Some(events) = crate::arena::boss_events(&self.spec)
        {
            let fields: Vec<_> = events.map.trim_start_matches('m').split('_').collect();
            if let (Ok(area), Ok(block)) = (fields[0].parse(), fields[1].parse())
                && let Some(spot) = crate::events::warp_point(&self.cache, area, block, region)
            {
                self.enemy.body.position = spot.position;
                self.enemy.snap_yaw(spot.yaw);
                self.enemy.body.velocity = [0.0; 3];
            }
        }
        let who = |entity: i32| match entity {
            PLAYER_ENTITY => Some(0),
            e if e == ev.boss_entity => Some(1),
            _ => None,
        };
        for c in commands {
            if let Some(encounter) = self.old_dragons.as_mut() {
                encounter.command(&c, dt);
            }
            if let Some(encounter) = self.butterfly_illusions.as_mut() {
                encounter.command(&c, dt);
            }
            if let Some(encounter) = self.monk_phantoms.as_mut() {
                encounter.command(&c, dt);
            }
            if let Some(bird) = self.father_bird.as_mut() {
                bird.command(&c, dt);
            }
            if let Some(encounter) = self.headless_ape.as_mut() {
                encounter.command(&c, dt);
            }
            if let Some(encounter) = self.screen_monkeys.as_mut() {
                encounter.command(&c, dt, &self.player, &self.fighters[0]);
            }
            match c {
                EventCommand::AiEnabled { entity, enabled }
                    if self.spec.chr == "c5090" && who(entity) == Some(1) =>
                {
                    self.enemy_active = enabled;
                }
                EventCommand::ShootBullet {
                    owner,
                    source,
                    dummy,
                    behavior,
                    angles,
                } if self.spec.chr == "c5430"
                    && owner == crate::isshin_fire::OWNER
                    && dummy == -1
                    && angles == [0; 3] =>
                {
                    if self.isshin_fire.is_none() {
                        match crate::isshin_fire::FireEmitter::load(&self.cache, &self.fighters[0])
                        {
                            Ok(emitter) => self.isshin_fire = Some(emitter),
                            Err(error) => eprintln!("Shura fire emitter not loaded: {error}"),
                        }
                    }
                    if let Some(emitter) = self.isshin_fire.as_ref()
                        && let Err(error) = crate::isshin_fire::shoot(
                            emitter,
                            &mut self.bullets,
                            &self.fighters[1],
                            source,
                            behavior,
                        )
                    {
                        eprintln!("Shura event shot not emitted: {error}");
                    }
                }
                EventCommand::ShootBullet {
                    owner,
                    source,
                    dummy,
                    behavior,
                    angles,
                } if self.spec.chr == "c5200" && owner == ev.boss_entity && angles == [0; 3] => {
                    self.shoot_dragon_event(owner, source, dummy as i16, behavior);
                }
                EventCommand::AiCommand {
                    entity,
                    command,
                    slot,
                } if who(entity) == Some(1) => {
                    if let Some(ai) = self.ai.as_mut() {
                        ai.state.event_requests.insert(i32::from(slot), command);
                    }
                }
                // Post-intro setup must not discard the battle goal already started this tick.
                EventCommand::AiReplan { entity }
                    if who(entity) == Some(1) && !(initializing && self.spec.starts_in_battle) =>
                {
                    if let Some(ai) = self.ai.as_mut() {
                        ai.state.replan = true;
                    }
                }
                EventCommand::AiParam { entity, row } if who(entity) == Some(1) => {
                    self.spec.think = row;
                    self.ai = Self::load_ai(&self.cache, &self.spec);
                }
                EventCommand::Cutscene {
                    id,
                    point,
                    area,
                    block,
                    player,
                } => {
                    let warp = (player == PLAYER_ENTITY && point > 0)
                        .then(|| crate::events::warp_point(&self.cache, area, block, point))
                        .flatten();
                    if point > 0 && warp.is_none() {
                        eprintln!(
                            "cutscene {id}: unresolved MSB warp point {point}; keeping position"
                        );
                    }
                    self.cinematic = Some(crate::events::Cinematic {
                        id,
                        elapsed: 0.0,
                        warp,
                        applied: false,
                    });
                    self.player.input.reset_requests();
                }
                EventCommand::SetSpEffect { entity, id } => {
                    if self.old_dragons.is_some() && entity == ev.boss_entity {
                        self.apply_screen_health_effect(id);
                    }
                    if self.screen_monkeys.is_some() && entity == ev.boss_entity && id == 3_126_010
                    {
                        self.apply_screen_health_effect(id);
                        continue;
                    }
                    if let Some(i) = who(entity) {
                        let c = if i == 0 {
                            &mut self.player
                        } else {
                            &mut self.enemy
                        };
                        // `effectEndurance` -1 lasts until cleared (200051), 0 is applied once.
                        let seconds = match c.tae.sp_effects.get(&id).map(|e| e.duration) {
                            Some(d) if d < 0.0 => f32::INFINITY,
                            Some(d) if d > 0.0 => d,
                            _ => INSTANT_EFFECT_STEPS * dt,
                        };
                        c.tae_runtime.add_timed(id, seconds);
                    }
                }
                EventCommand::ClearSpEffect { entity, id } => {
                    if let Some(i) = who(entity) {
                        let character = if i == 0 {
                            &mut self.player
                        } else {
                            &mut self.enemy
                        };
                        character.tae_runtime.clear_timed(id);
                        if self.spec.chr == "c5090" {
                            character
                                .tae_runtime
                                .resident
                                .retain(|effect| *effect != id);
                            self.rules.fighters[i].clear_resident_effect(id);
                        }
                    }
                }
                EventCommand::EzStateRequest {
                    entity,
                    command,
                    slot: _,
                } if crate::guardian_ape::headless(&self.spec)
                    && ((who(entity) == Some(1) && command == 20200)
                        || (who(entity) == Some(0) && command == 710203)) => {}
                EventCommand::EzStateRequest {
                    entity,
                    command,
                    slot,
                } if who(entity) == Some(1) => {
                    ev.sword_saint_finish
                        .receive(&self.spec, &mut ev.ezstate, (slot, command));
                }
                EventCommand::EzStateRequest {
                    entity,
                    command,
                    slot,
                } if who(entity) == Some(0) => ev.player_ezstate = Some((slot, command)),
                EventCommand::BossBar {
                    shown,
                    entity,
                    name,
                    ..
                } if who(entity) == Some(1) => {
                    ev.boss_bar = shown.then_some(name);
                }
                EventCommand::HpBarDisplay { entity, on } if who(entity) == Some(1) => {
                    ev.hp_bar = on;
                }
                EventCommand::Immortality { entity, on } => {
                    if let Some(i) = who(entity) {
                        self.rules.fighters[i].vitality.cannot_die = on;
                    }
                }
                EventCommand::Gravity { entity, enabled } => {
                    if let Some(i) = who(entity) {
                        let body = if i == 0 {
                            &mut self.player.body
                        } else {
                            &mut self.enemy.body
                        };
                        body.gravity_enabled = enabled;
                        if !enabled {
                            body.velocity[1] = 0.0;
                        }
                    }
                }
                EventCommand::BossDefeat { entity }
                    if who(entity) == Some(1)
                        || crate::arenas_a::retired_butterfly(ev.boss_entity, entity) =>
                {
                    ev.defeated = true;
                }
                EventCommand::CharacterEnabled { entity, enabled } if who(entity) == Some(1) => {
                    self.enemy_active = enabled;
                }
                EventCommand::CharacterEnabled { entity, .. }
                    if crate::arena::depths_palace::simultaneous(ev.boss_entity, entity) => {}
                EventCommand::CharacterEnabled {
                    entity,
                    enabled: true,
                } if who(entity).is_none() && crate::arena_c::allow_handover(&self.spec) => {
                    self.pending_handover = Some((entity, None));
                }
                EventCommand::BossBar {
                    shown: true,
                    entity: 2_500_800,
                    name,
                    ..
                } if ev.boss_entity == 2_500_810 => {
                    // EMEVD 12505885 activates the Dragon through animation/AI state,
                    // then its boss bar, rather than ChangeCharacterEnableState.
                    self.pending_handover = Some((2_500_800, Some(name)));
                }
                EventCommand::BossBar {
                    shown: true,
                    entity,
                    name,
                    ..
                } if self
                    .pending_handover
                    .is_some_and(|(next, _)| next == entity) =>
                {
                    self.pending_handover = Some((entity, Some(name)));
                }
                // Remaining commands require world or cinematic systems outside the duel.
                _ => {}
            }
        }
        self.events = Some(ev);
        if let (Some(gate), Some(ev)) = (&self.fog_gate, &self.events) {
            gate.set_gate(ev.standing_fog_walls().next().is_some());
            for (&entity, &on) in &ev.scripts.cast_scene.enabled {
                gate.set_event_part(entity, on);
            }
        }
        if self.deathblow.is_none()
            && let Some((entity, bar)) = self.pending_handover.take()
        {
            let handover = crate::arena::boss_events(&self.spec).and_then(|events| {
                let roster = crate::registry::Registry::load(&self.cache).ok()?;
                let (spec, p) = roster.entity(events.map, entity).ok()?;
                // A summoned illusion or Father's bird is an additional actor, not a boss phase.
                crate::arena::boss_events(&spec)?;
                Some((
                    spec,
                    Placement {
                        position: p.position,
                        yaw: p.yaw,
                    },
                ))
            });
            if let Some((spec, mut spot)) = handover {
                if self.cinematic.as_ref().is_some_and(|c| c.warp.is_none()) {
                    spot = placement(&self.enemy);
                }
                match self.switch_enemy(spec, spot) {
                    Ok(()) => {
                        if let Some(events) = self.events.as_mut() {
                            events.boss_bar = bar;
                        }
                    }
                    Err(error) => eprintln!("map hand-over to {entity} failed: {error}"),
                }
            }
        }
    }

    /// Wolf's model opacity from the fall return's fade: 1 to 0 over
    /// [`crate::hit_filter::FALL_RETURN_FADE`] after a filter-15 event wall, else 1.
    pub fn fall_return_opacity(&self) -> f32 {
        self.fall_return_fade.map_or(1.0, |t| {
            (1.0 - t / crate::hit_filter::FALL_RETURN_FADE).clamp(0.0, 1.0)
        })
    }

    /// The enemy's model opacity from its fall death's fade: 1 to 0 over
    /// [`crate::hit_filter::FALL_RETURN_FADE`] after a filter-15 event wall, else 1.
    pub fn enemy_fall_opacity(&self) -> f32 {
        self.enemy_fall.opacity()
    }

    /// Kills the enemy as the exe's fall-dead routine does for an NPC (see [`crate::npc_fall`]),
    /// and holds its body once the fade after a filter-15 event wall has ended.
    fn kill_fallen_enemy(&mut self, before: [f32; 3], dt: f32) {
        let enemy = &mut self.enemy;
        let Some(death) = self
            .enemy_fall
            .step(&mut enemy.body, &*enemy.ground, before, dt)
        else {
            return;
        };
        eprintln!(
            "fall death: {} at {:.2?} ({death:?})",
            self.spec.chr, enemy.body.position
        );
        self.rules.fighters[1].vitality.set_hp(0);
    }

    fn return_fallen_enemy(&mut self) {
        // The arena's NPC boundary reset, for an enemy that has not died of its fall: one that
        // has stays where it fell (see [`crate::npc_fall`]).
        if self.enemy_fall.death.is_some() {
            return;
        }
        if let Some((depth, spawn)) = self.fall_respawn_depth.zip(self.spawn) {
            let start = spawn[1];
            if self.enemy.body.position[1] < start.position[1] - depth {
                self.enemy.body.position = start.position;
                self.enemy.snap_yaw(start.yaw);
                self.enemy.body.velocity = [0.0; 3];
                self.enemy.body.grounded = true;
                self.enemy.body.fall_top = start.position[1];
                self.enemy.body.fall_type = 0;
            }
        }
    }

    /// Routes lethal landings through DEATH_FALLING and returns Wolf to where he last stood
    /// after a fall into the void.
    fn kill_fallen(&mut self, was_grounded: bool, before: [f32; 3], dt: f32) {
        if self.rules.fighters[0].dead() {
            return;
        }
        let b = &self.player.body;
        let [x, y, z] = b.position;
        // Only a stance on real collision counts as safe ground.
        let on_floor = self
            .player
            .ground
            .floor(x, z, y + 0.5)
            .is_some_and(|floor| (y - floor).abs() < 0.3);
        if was_grounded && b.grounded && on_floor {
            self.last_safe = Some((b.position, b.yaw));
        }
        // A map action event wall in the path: filter 15 starts the fall return (its model
        // fade, then the return below); other filters' reactions are only recorded.
        let ground = &self.player.ground;
        let contact = ground
            .trigger_contact(before, b.position)
            .filter(|c| !self.touching.contains(&c.part));
        self.touching
            .retain(|part| ground.near_trigger(part, b.position));
        if let Some(contact) = contact {
            let fall_return = contact.hit_filter == crate::hit_filter::FALL_RETURN_FILTER
                && self.fall_return_fade.is_none();
            eprintln!(
                "helper collision: Wolf touched {} (hit filter {}) at {:.2?}{}",
                contact.part,
                contact.hit_filter,
                contact.point.to_array(),
                if fall_return { ", fall return" } else { "" }
            );
            if fall_return {
                self.fall_return_fade = Some(0.0);
                self.fall_return_part = Some(contact.part.clone());
            }
            self.touching.push(contact.part.clone());
            self.helper_events.push(HelperEvent {
                hit_filter: contact.hit_filter,
                part: contact.part,
                point: contact.point.to_array(),
                returned_to: None,
            });
        }
        let b = &self.player.body;
        if let Some(t) = &mut self.fall_return_fade {
            *t += dt;
        }
        let fading = self.fall_return_fade;
        let faded = fading.is_some_and(|t| t >= crate::hit_filter::FALL_RETURN_FADE);
        // Placeholder void boundary where no authored return wall (filter 15) lies below.
        // The lowest supporting collision takes precedence over spawn to allow deep paths.
        let void_fall = fading.is_none()
            && !b.grounded
            && !self.player.ground.fall_return_below(x, y, z)
            && self
                .fall_respawn_depth
                .zip(self.spawn)
                .is_some_and(|(depth, spawn)| {
                    let lowest = self
                        .player
                        .ground
                        .lowest_floor(x, z)
                        .unwrap_or(spawn[0].position[1]);
                    y < lowest - depth
                });
        if void_fall || faded {
            // Sekiro returns Wolf to safe ground after a fall into the void instead of killing
            // him (the script's FALL_DEAD_RETURN route). The HP cost is in the exe and not
            // traced yet, so none is taken here.
            let (position, yaw) = self
                .last_safe
                .or_else(|| self.spawn.map(|s| (s[0].position, s[0].yaw)))
                .unwrap_or((b.position, b.yaw));
            let started = self.fall_return_part.take();
            if self.fall_return_fade.take().is_some()
                && let Some(event) = self.helper_events.iter_mut().rev().find(|e| {
                    e.hit_filter == crate::hit_filter::FALL_RETURN_FILTER
                        && e.returned_to.is_none()
                        && started.as_ref().is_none_or(|part| *part == e.part)
                })
            {
                event.returned_to = Some(position);
            }
            eprintln!(
                "fall return: Wolf back at {position:.2?} ({})",
                if faded {
                    "filter-15 event wall"
                } else {
                    "placeholder void depth"
                }
            );
            self.player.snap_yaw(yaw);
            let body = &mut self.player.body;
            body.position = position;
            body.velocity = [0.0; 3];
            body.grounded = true;
            body.fall_top = position[1];
            body.fall_type = 0;
            return;
        }
        if self.god || self.immortal {
            return;
        }
        let lethal_landing = !was_grounded && b.grounded && {
            // Keyed by HitMtrlParam row, without the material's special setting.
            let material = self
                .player
                .ground
                .material(x, z, y + 0.01)
                .and_then(crate::body::hit_mtrl_row)
                .unwrap_or(1);
            self.fall_materials.get(&material).is_some_and(|m| {
                m.disable_fall_damage == 0 && b.last_fall_height >= m.fall_death_dist as f32
            })
        };
        if lethal_landing {
            self.rules.fighters[0].vitality.hp = 0;
            self.player.pending_damage = Some(crate::player::IncomingDamage {
                damage_type: 6,
                ..Default::default()
            });
        }
    }

    /// An absent slot does not advance or participate in contacts, even if it still has HP.
    fn step_solo(&mut self, input: &InputFrame, dt: f32) -> DuelReport {
        let respawned = self.begin_player_step(input, dt);
        self.attack_button = input.buttons.attack;
        self.publish_player_frame(None);
        self.player.aim_candidates.clear();
        let player_dt = self.player_dt(dt);
        let was_grounded = self.player.body.grounded;
        let before = self.player.body.position;
        self.player.throw_ignore_walls = false;
        let player = self.advance_player(input, dt, player_dt);
        self.push(0, player_dt);
        self.finish_player_movement(was_grounded, before, dt);
        let bullets = self.step_bullets(dt);
        let perilous = self.apply_effect_hits(bullets.into_iter().filter(|b| b.effects_only));
        self.step_lightning();
        let jump_counter = self.note_player_hits(&[]);
        self.tick_player_status(dt);
        self.recover_player(player_dt);
        self.schedule_respawn();
        self.update_player_camera(dt);
        DuelReport {
            player,
            respawned,
            perilous,
            jump_counter,
            ..Default::default()
        }
    }

    fn fighter_count(&self) -> usize {
        1 + usize::from(self.enemy_present)
    }

    fn publish_player_frame(&mut self, endure_action: Option<i32>) {
        self.publish_event_envs(endure_action);
        self.publish_vitals();
        self.publish_swing_hit();
    }

    fn player_dt(&mut self, dt: f32) -> f32 {
        if self.shared_player_frame {
            0.0
        } else {
            self.hit_stop[0].split(dt)
        }
    }

    fn advance_player(&mut self, input: &InputFrame, dt: f32, player_dt: f32) -> StepReport {
        // Auxiliary encounters reuse Wolf's frame, including if their slot is vacated.
        if self.shared_player_frame {
            self.last_steps[0].clone()
        } else {
            self.advance_character(0, input, dt, player_dt)
        }
    }

    fn step_bullets(&mut self, dt: f32) -> Vec<crate::bullets::BulletHit> {
        let count = self.fighter_count();
        let chars = &([&self.player, &self.enemy])[..count];
        let fighters = &([&self.fighters[0], &self.fighters[1]])[..count];
        self.bullets.spawn(chars, fighters);
        self.bullets
            .advance(dt, chars, fighters, &*self.player.ground)
    }

    fn tick_player_status(&mut self, dt: f32) {
        if !self.shared_player_frame {
            self.rules.tick_status(0, dt);
            self.rules.apply_effect_hp(0, self.player.tae_frame());
        }
    }

    fn begin_player_step(&mut self, input: &InputFrame, dt: f32) -> bool {
        if self.spawn.is_none() {
            self.spawn = Some([placement(&self.player), placement(&self.enemy)]);
        }
        let mut respawned = false;
        if let Some(t) = self.respawn_in.as_mut() {
            *t -= dt;
            if *t <= 0.0 && self.reset().is_ok() {
                // A respawn after death restocks the goods as Rest does (docs/FIDELITY.md).
                self.restock_kit();
                respawned = true;
            }
        }
        if !self.shared_player_frame {
            self.update_lock(input);
        }
        respawned
    }

    fn finish_player_movement(&mut self, was_grounded: bool, player_before: [f32; 3], dt: f32) {
        if !self.shared_player_frame {
            self.kill_fallen(was_grounded, player_before, dt);
        }

        if self.god {
            let f = &mut self.rules.fighters[0];
            f.vitality.cannot_die = true;
            f.posture.no_consume = true;
        }
        if self.immortal {
            self.rules.fighters[0].vitality.cannot_die = true;
        }
    }

    fn apply_effect_hits(
        &mut self,
        effect_hits: impl IntoIterator<Item = crate::bullets::BulletHit>,
    ) -> Option<crate::perilous::PerilousWarning> {
        let mut perilous = None;
        for b in effect_hits {
            let kanji = b.sp_effects.iter().any(|id| {
                self.player
                    .tae
                    .sp_effects
                    .get(id)
                    .is_some_and(|e| e.vfx == crate::perilous::KANJI_VFX)
            });
            if b.hit.defender == 0 && kanji && b.hit.attacker < self.fighter_count() {
                let a = b.hit.attacker;
                perilous = Some(crate::perilous::PerilousWarning {
                    attacker: a,
                    judge: b.hit.judge,
                    kind: crate::perilous::upcoming_kind(self.chr(a), &self.fighters[a]),
                });
            }
            if !self.rules.fighters[..self.fighter_count()]
                .iter()
                .any(|f| f.dead())
            {
                self.apply_status(b.hit.defender, &b.sp_effects);
            }
        }
        perilous
    }

    fn recover_player(&mut self, dt: f32) {
        if self.god {
            let v = &mut self.rules.fighters[0].vitality;
            v.hp = v.max_hp;
        }
        if !self.shared_player_frame {
            self.rules.recover(0, &self.player, dt);
        }
    }

    fn schedule_respawn(&mut self) {
        if self.respawn_in.is_none()
            && !self.shared_player_frame
            && self.deathblow.is_none()
            // Map defeat events can still be waiting before handing over to the next boss.
            && (self.events.is_none() || self.rules.fighters[0].dead())
            && self.rules.fighters[..self.fighter_count()].iter().any(|f| f.dead())
        {
            self.respawn_in = Some(RESPAWN_DELAY);
        }
    }

    fn update_player_camera(&mut self, dt: f32) {
        let clip = self
            .player
            .behavior
            .runtime
            .main_clip()
            .map(|c| (c.animation.clone(), c.time));
        if !self.shared_player_frame {
            self.camera_director.update(
                clip.as_ref().map(|(a, t)| (a.as_str(), *t)),
                &self.player.tae,
                dt,
            );
        }
    }

    pub fn step(&mut self, input: &InputFrame, dt: f32) -> DuelReport {
        // The hook search looks through the follow camera as the director left it last step,
        // as the camera on screen does. A duel sharing the player's frame (a cast actor) does
        // not own the camera, so it leaves the rig alone.
        if !self.shared_player_frame {
            self.player.wire.rig = crate::wire::CameraRig {
                focus_height: self.camera_director.focus_height,
                distance: self.camera_director.distance,
            };
        }
        let held = self.idol_input(input, dt);
        let input = held.as_ref().unwrap_or(input);
        if self.population.is_some() {
            return self.step_population(input, dt);
        }
        if !self.enemy_present {
            return self.step_solo(input, dt);
        }
        if old_dragons::owns(&self.spec) && self.old_dragons.is_some() && !self.defer_map_events {
            return self.step_old_dragons(input, dt);
        }
        if self.headless_ape.is_some() && !self.defer_map_events {
            return self.step_ape_encounter(input, dt);
        }
        if self.screen_monkeys.is_some() {
            return self.step_screen_monkeys(input, dt);
        }
        // Let an already playing pair finish before beginning a queued cinematic. Otherwise
        // pausing the pair would prevent the map's hand-over from ever becoming eligible.
        if self.deathblow.is_none() && self.cinematic.is_some() {
            let cinematic = self.cinematic.as_mut().unwrap();
            let was_black = cinematic.elapsed >= crate::events::Cinematic::FADE;
            cinematic.elapsed += dt;
            let apply = !cinematic.applied && was_black;
            if apply {
                cinematic.applied = true;
                if let Some(warp) = cinematic.warp {
                    self.player.body.position = warp.position;
                    self.player.snap_yaw(warp.yaw);
                    self.player.body.velocity = [0.0; 3];
                    self.player.body.fall_top = warp.position[1];
                    self.player.body.grounded = true;
                }
                self.step_events(dt);
                // Event-only cinematic frames bypass after_step, but may declare victory.
                if let Some(mut driver) = self.boss_brain.take() {
                    driver.finish_if_over(self);
                    self.boss_brain = Some(driver);
                }
            }
            for c in [&mut self.player, &mut self.enemy] {
                c.tae_runtime.frame.sounds.clear();
                c.tae_runtime.frame.velocity_changes.clear();
            }
            let reports: [StepReport; 2] = std::array::from_fn(|i| {
                let mut r = self.last_steps[i].clone();
                let c = if i == 0 { &self.player } else { &self.enemy };
                r.position = c.body.position;
                r.yaw = c.body.yaw;
                r.advanced = 0.0;
                r.clip_started = false;
                r.requested.clear();
                r.behavior.events.clear();
                r.behavior.hooks.clear();
                r.behavior.script_errors.clear();
                r
            });
            if self
                .cinematic
                .as_ref()
                .is_some_and(|c| c.elapsed >= crate::events::Cinematic::DURATION)
            {
                self.cinematic = None;
                self.player.input = Default::default();
                self.attack_button = false;
                self.lock_button = false;
            }
            let [player, enemy] = reports;
            return DuelReport {
                player,
                enemy,
                ..Default::default()
            };
        }
        let raw_input = *input;
        let respawned = self.begin_player_step(input, dt);

        // The engine checks for a deathblow before the attack reaches the script.
        let mut input = *input;
        let attack_pressed = input.buttons.attack && !self.attack_button;
        self.attack_button = input.buttons.attack;
        let mut deathblow = DeathblowReport::default();
        if !self.shared_player_frame
            && attack_pressed
            && let Some(rows) = self.try_deathblow()
        {
            deathblow.started = Some(rows);
            input.buttons.attack = false;
        }

        if let Some(d) = self.boss_brain.as_mut() {
            d.tick(dt);
        }
        let enemy_alive = !self.rules.fighters[1].dead();
        let mut endure_action = None;
        let demon_idle = crate::demon::waiting(&self.spec, self.events.as_ref())
            || crate::demon::recovering(
                &self.spec,
                self.enemy
                    .behavior
                    .runtime
                    .main_clip()
                    .map(|c| c.animation.as_str()),
            );
        if self.enemy_active && enemy_alive && self.deathblow.is_none() && !respawned && !demon_idle
        {
            if self.ai.is_some() {
                let world = self.ai_world(dt);
                let mut driver = self.boss_brain.take();
                if let Some(mut ai) = self.ai.take() {
                    if let Some(d) = driver.as_mut() {
                        d.before_think(&mut ai, self.fighters[1].radius);
                    }
                    let max_marks = match &self.rules.fighters[1].kind {
                        crate::fight::FighterKind::Npc(n) => n.deathblows,
                        _ => 0,
                    };
                    ai.state.ninsatsu =
                        (self.rules.fighters[1].vitality.deathblows_left, max_marks);
                    if let Some(events) = &self.events {
                        ai.state.event_flags.clone_from(&events.scripts.flags);
                    }
                    ai.state.dist_offset = crate::demon::dist_offset(&self.spec, &self.fighters[1]);
                    self.enemy.npc = ai.think(world);
                    if let Some(events) = self.events.as_mut() {
                        for (flag, on) in ai.state.event_flag_changes.drain() {
                            if on {
                                events.scripts.flags.insert(flag);
                            } else {
                                events.scripts.flags.remove(&flag);
                            }
                        }
                    }
                    endure_action = ai.state.request.endure.then_some(self.enemy.npc.action);
                    if let Some(d) = driver.as_mut() {
                        d.after_think(&mut ai, self);
                    }
                    self.ai = Some(ai);
                }
                self.boss_brain = driver;
            } else {
                self.brain
                    .think(&mut self.enemy, self.player.body.position, dt);
            }
        } else {
            self.enemy.npc = Default::default();
        }
        self.force_enemy_act(enemy_alive);
        self.publish_player_frame(endure_action);
        // Who each side aims at: auto-aim candidates for Wolf, Wolf for the soldier.
        if self.population_target.is_none() {
            self.player.aim_candidates = if enemy_alive {
                vec![self.enemy.body.position]
            } else {
                Vec::new()
            };
        }
        self.enemy.npc_target =
            (!self.rules.fighters[0].dead()).then_some(self.player.body.position);
        // Hit stop: each character advances only by what is left of the step after its freeze.
        let monk_frozen = crate::demon::waiting(&self.spec, self.events.as_ref())
            || (crate::owl_father::owns(&self.spec)
                && self.events.as_ref().is_some_and(|e| {
                    e.scripts.cast_scene.animation_enabled.get(&e.boss_entity) == Some(&false)
                }))
            || (crate::true_monk::owns(&self.spec)
                && self.events.as_ref().is_some_and(|e| {
                    e.scripts.cast_scene.animation_enabled.get(&e.boss_entity) == Some(&false)
                }))
            || !self.auxiliary_animation_enabled;
        let dts = [
            self.player_dt(dt),
            if monk_frozen {
                0.0
            } else {
                self.hit_stop[1].split(dt)
            },
        ];
        let was_grounded = self.player.body.grounded;
        let pair_before = [self.player.body.clone(), self.enemy.body.clone()];
        let player_before = pair_before[0].position;
        let enemy_before = pair_before[1].position;
        let paired_active = self.deathblow.is_some() || self.throw_held();
        let recovering = [
            self.player.state_name().starts_with("Throw"),
            self.enemy_in_throw_clip(),
        ];
        let held = self
            .deathblow
            .as_ref()
            .is_some_and(|d| d.phase == Phase::Body)
            || self.throw_held();
        self.player.throw_ignore_walls = held;
        self.enemy.throw_ignore_walls = held;
        let player = self.advance_player(&input, dt, dts[0]);
        let enemy = if monk_frozen {
            // Animation disable holds the clip clock, while HKS still accepts AI/event requests.
            let report = self.enemy.tick(&InputFrame::default(), 0.0);
            self.last_steps[1] = report.clone();
            report
        } else {
            self.advance_character(1, &InputFrame::default(), dt, dts[1])
        };
        self.apply_sp_categories();
        if self.deathblow.is_none() && !self.throw_held() {
            self.push(0, dts[0]);
            self.push(1, dts[1]);
        }
        if self.defender_dying > 0 {
            self.defender_dying = self.defender_dying.saturating_add(1);
        }
        self.step_deathblow(dt, &mut deathblow);
        let mut grab = GrabReport::default();
        self.step_enemy_throw(dt, &mut grab);
        self.step_mikiri(dt);
        if paired_active || self.deathblow.is_some() || self.throw_held() {
            let sweep_motion = held
                || self
                    .deathblow
                    .as_ref()
                    .is_some_and(|d| d.phase == Phase::Body)
                || self.throw_held();
            self.constrain_pair_motion(pair_before, [true, true], sweep_motion);
        } else if recovering.iter().any(|v| *v) {
            // Once Wolf leaves his clip, only the still-playing throw recovery is
            // constrained. Free movement and knockback remain the controller's job.
            self.constrain_pair_motion(pair_before, recovering, false);
        } else {
            self.paired_floor = None;
        }
        if self.deathblow.is_none() && !self.throw_held() && enemy_alive && self.spec.chr != "c5005"
        {
            self.separate([player_before, enemy_before]);
        }
        self.kill_fallen_enemy(enemy_before, dt);
        if !self.shared_player_frame {
            self.return_fallen_enemy();
        }
        self.finish_player_movement(was_grounded, player_before, dt);
        let mut found = self.detector.detect(
            &[&self.player, &self.enemy],
            &[&self.fighters[0], &self.fighters[1]],
        );
        // The lightning capsule ends on weapon dummy 10141, absent from the
        // current rig. A facing beam substitutes for that endpoint until the
        // native weapon-dummy transform is reconstructed. Radius is AtkParam;
        // ten metres of reach is explicitly a stand-in.
        let reversal = self
            .player
            .tae_frame()
            .attacks
            .iter()
            .any(|e| matches!(e.kind, crate::tae::TaeKind::Attack { judge: 281, .. }));
        if !reversal {
            self.lightning_returned = false;
        }
        let other_counter = reversal && !self.lightning_returned;
        if other_counter {
            self.lightning_returned = true;
            if let Some(attack) = self.fighters[0].attacks.get(&281).cloned() {
                let start = glam::Vec3::from_array(self.player.body.position) + glam::Vec3::Y;
                let [x, z] = self.player.body.forward();
                let end = start + glam::Vec3::new(x, 0.0, z) * 10.0;
                let feet = glam::Vec3::from_array(self.enemy.body.position);
                let radius = attack.shapes.first().map_or(0.0, |s| s.radius);
                if crate::hits::can_be_hit(self.enemy.tae_frame())
                    && crate::hits::segment_distance(
                        start,
                        end,
                        feet,
                        feet + glam::Vec3::Y * self.fighters[1].height,
                    ) <= radius + self.fighters[1].radius
                {
                    found.retain(|h| !(h.attacker == 0 && h.judge == 281));
                    found.push(HitEvent {
                        attacker: 0,
                        defender: 1,
                        judge: 281,
                        attack,
                        position: feet.to_array(),
                        direction: [x, z],
                    });
                }
            }
        }
        let bullet_hits = self.step_bullets(dt);
        let first_bullet_hit = found.len();
        // Contacts that only carry SpEffects (the poison floor) skip the hit rules (ours).
        let (effect_hits, bullet_hits): (Vec<_>, Vec<_>) =
            bullet_hits.into_iter().partition(|b| b.effects_only);
        let bullet_effects: Vec<Vec<i32>> =
            bullet_hits.iter().map(|b| b.sp_effects.clone()).collect();
        let bullet_owners: Vec<Option<i32>> = bullet_hits.iter().map(|b| b.event_owner).collect();
        let bullet_sources: std::collections::BTreeMap<usize, i32> = bullet_hits
            .iter()
            .enumerate()
            .filter_map(|(i, b)| Some((found.len() + i, b.source_action?)))
            .collect();
        found.extend(bullet_hits.into_iter().map(|b| b.hit));
        let mut hits = Vec::new();
        let mut projectile_sources = std::collections::BTreeMap::new();
        // Counter contacts win over the damaging blade contact of the same step.
        let mikiri = if self.deathblow.is_none()
            && !self.throw_held()
            && !self.rules.fighters.iter().any(|f| f.dead())
        {
            found
                .iter()
                .take(first_bullet_hit)
                .find_map(|hit| self.try_mikiri(hit))
        } else {
            None
        };
        self.enemy_damaged = false;
        for (k, hit) in found.into_iter().enumerate() {
            if self.deathblow.is_some()
                || self.throw_held()
                || self.rules.fighters.iter().any(|f| f.dead())
            {
                break;
            }
            // A throw start grabs instead of hitting (guarding does not stop it).
            if crate::perilous::counter_dummy(&hit.attack) {
                continue;
            }
            if hit.attacker == 1 && hit.attack.throw_flag == 1 {
                grab.started = self.try_enemy_throw(&hit);
                continue;
            }
            let from_bullet = k >= first_bullet_hit;
            let chars = [&self.player, &self.enemy];
            let combatants = [&self.fighters[0], &self.fighters[1]];
            let mut result = if from_bullet
                && bullet_owners[k - first_bullet_hit] == Some(crate::isshin_fire::OWNER)
                && let Some(emitter) = self.isshin_fire.as_mut()
            {
                self.rules
                    .resolve_event_bullet(&hit, chars, combatants, &mut emitter.fighter)
            } else if from_bullet {
                self.rules.resolve_bullet(&hit, chars, combatants)
            } else {
                self.rules.resolve(&hit, chars, combatants)
            };
            let after_contact = (
                self.rules.fighters[hit.defender].vitality.hp,
                self.rules.fighters[hit.defender].posture.remaining,
            );
            let (att, def) = if hit.attacker == 0 {
                (&mut self.player, &mut self.enemy)
            } else {
                (&mut self.enemy, &mut self.player)
            };
            def.pending_damage = Some(result.defender);
            // The AtkParam's SpEffects stay on a landed melee hit's defender (bullets carry
            // their own, below).
            let lightning = hit
                .attack
                .profile
                .as_ref()
                .is_some_and(|p| matches!(p.sp_attribute, 6 | 10));
            let melee_effects = if !from_bullet && (!result.guarded || lightning) {
                hit.attack.sp_effects.clone()
            } else {
                Vec::new()
            };
            if let Some(back) = result.attacker {
                att.pending_damage = Some(back);
            }
            self.knockback[hit.defender] = result.knockback;
            // A bullet's hit does not freeze its distant shooter (ours).
            if !from_bullet {
                self.hit_stop[hit.attacker].start(result.hit_stop[0]);
            }
            self.hit_stop[hit.defender].start(result.hit_stop[1]);
            self.enemy_damaged |= hit.defender == 1 && !result.guarded;
            // A bullet's SpEffects stay on a guarded hit's guard (inferred).
            if from_bullet && (!result.guarded || lightning) {
                self.apply_status(hit.defender, &bullet_effects[k - first_bullet_hit]);
            }
            if !melee_effects.is_empty() {
                self.apply_hit_effects(hit.defender, &melee_effects, dt);
            }
            result.hp_damage += after_contact.0 - self.rules.fighters[hit.defender].vitality.hp;
            result.posture_damage +=
                after_contact.1 - self.rules.fighters[hit.defender].posture.remaining;
            if let Some(source) = bullet_sources.get(&k) {
                projectile_sources.insert(hits.len(), *source);
            }
            hits.push((hit, result));
        }
        let perilous = self.apply_effect_hits(effect_hits);
        self.step_lightning();
        let jump_counter = self.note_player_hits(&hits);
        self.tick_player_status(dt);
        self.rules.tick_status(1, dt);
        if crate::guardian_ape::headless(&self.spec)
            && self.spec.npc_param == 51000000
            && self.enemy.tae_frame().sp_effects.contains(&275100)
        {
            let _ = crate::guardian_ape::revival_stats(&self.cache, &mut self.rules.fighters[1]);
        }
        if let Some(stats) = self.monk_stats.as_mut() {
            stats.apply(&mut self.rules.fighters[1], self.enemy.tae_frame());
        }
        self.rules.apply_effect_hp(1, self.enemy.tae_frame());
        if crate::guardian_ape::headless(&self.spec)
            && self.enemy.tae_frame().sp_effects.contains(&401000)
        {
            self.defender_dying = 0;
        }
        self.recover_player(dts[0]);
        self.rules.recover(1, &self.enemy, dts[1]);
        self.schedule_respawn();
        if crate::guardian_ape::headless(&self.spec)
            && self.enemy.tae_runtime.event_message == Some(30)
        {
            self.rules.fighters[1].vitality.cannot_die = false;
            self.rules.fighters[1].vitality.hp = 0;
        }
        if !self.defer_map_events {
            self.step_events(dt);
        }
        if !self.shared_player_frame && self.rules.fighters[1].dead() {
            self.release_lock();
        }
        self.pay_kill();
        let mut report = DuelReport {
            projectile_sources,
            locked_on: self.player.lock_target.is_some(),
            player,
            enemy,
            hits,
            deathblow,
            grab,
            respawned,
            perilous,
            jump_counter,
            other_counter,
            mikiri,
        };
        self.update_player_camera(dt);
        if let Some(mut d) = self.boss_brain.take() {
            d.after_step(self, &report, &raw_input);
            self.boss_brain = Some(d);
        }
        self.step_monk_phantoms(dt, &mut report);
        self.step_father_bird(dt, &mut report);
        self.step_dragon_trees(dt, &mut report);
        self.step_butterfly_illusions(dt, &mut report);
        report
    }

    /// A defeated opponent pays its NpcParam `skillPoint` experience and `getSoul` Sen into
    /// Wolf's kit, once per life; a hidden opponent pays nothing. A map foe's duel holds Wolf's
    /// kit while it steps (`swap_screen_player`), so its reward lands in the one kit.
    /// Boss and encounter NpcParam rows (5000xxxx, 5060xxxx, 7100xxxx) pay 0, so phase deaths
    /// pay nothing extra.
    fn pay_kill(&mut self) {
        let dead = self.rules.fighters[1].dead();
        if dead
            && !self.was_dead
            && self.enemy_present
            && let crate::fight::FighterKind::Npc(npc) = &self.rules.fighters[1].kind
        {
            let points = self.kit.earn(npc.skill_point, npc.get_soul);
            eprintln!(
                "defeated NpcParam {}: +{} experience, +{} Sen, +{points} skill points",
                npc.id, npc.skill_point, npc.get_soul
            );
        }
        self.was_dead = dead;
    }
}

/// Predicts the authored capsule paths through the paired clip, including its absorption.
/// Uses the same reference-frame delta integration as Character, without advancing scripts.
/// Floors and airborne velocity events are not predicted by this probe.
fn throw_body_path(db: &Deathblow, player: &Character, enemy: &Character) -> Vec<[[f32; 3]; 2]> {
    use crate::body::{Body, BodyInput, FlatGround};
    let placements = [placement(player), placement(enemy)];
    let clips = [
        player.clips.clip(&db.body.attacker_anim()),
        db.body
            .defender_anim()
            .and_then(|name| enemy.clips.clip(&name)),
    ];
    let mut bodies = placements.map(|p| Body {
        position: p.position,
        yaw: p.yaw,
        ..Body::default()
    });
    let mut slide = crate::deathblow::AbsorbSlide::new(
        db.absorbed,
        &db.sorb,
        &placements[0],
        &placements[1],
        db.body.interpolation_time,
    );
    let mut path = vec![bodies.clone().map(|b| b.position)];
    let end = db.body_duration;
    let mut time = 0.0;
    while time < end {
        let dt = (end - time).min(1.0 / 60.0);
        for i in 0..2 {
            let motion = clips[i]
                .as_ref()
                .and_then(|c| c.root_motion.as_ref())
                .map_or([0.0; 4], |rm| {
                    let a = rm.sample(time);
                    let b = rm.sample(time + dt);
                    let [x, z] = crate::motion::rotate_xz([b[0] - a[0], b[2] - a[2]], -a[3]);
                    [x, b[1] - a[1], z, b[3] - a[3]]
                });
            bodies[i].step(
                &BodyInput {
                    root_motion: motion,
                    ignore_walls: true,
                    ..Default::default()
                },
                dt,
                &FlatGround {
                    height: placements[i].position[1],
                },
            );
        }
        if let Some(a) = slide.advance(dt) {
            let i = if a.who == Absorbed::Attacker { 0 } else { 1 };
            bodies[i].position[0] += a.delta[0];
            bodies[i].position[2] += a.delta[2];
            bodies[i].yaw += a.yaw;
        }
        time += dt;
        path.push(bodies.clone().map(|b| b.position));
    }
    path
}
