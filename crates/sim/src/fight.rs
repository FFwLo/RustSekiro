//! HP, posture and hit resolution for a fight, on the engine rules in [`crate::combat`].
//!
//! [`CombatRules`] keeps one [`Fighter`] per character (HP, the deathblow counter and the posture
//! meter) and turns each [`HitEvent`] into numbers and the reaction codes the two scripts read:
//!
//! 1. Guard test: the defender's TAE `ShieldBlock` flag names its guard judge, whose AtkParam row
//!    is the guard row; the arc comes from the weapon (Wolf) or NpcParam `guardAngle` (NPCs); the
//!    repel defence from the guard row, the weapon and the active SpEffects (`defFlickPower` 40
//!    of the just-guard window 105010 makes a deflect). See [`crate::combat::hit`].
//! 2. Posture damage to the defender and back to the attacker, with the defender's deflect
//!    effects 105020..105023 halving what a chain of deflects sends back.
//! 3. HP damage through the element sum and the guard cuts.
//! 4. The damage codes (99999, 3, 1001, 1027, 1028, 2 for the defender; 1000, 1008, 1017,
//!    1019, 1033 for the attacker).
//! 5. The defender's knockback and both sides' hit stop (see [`crate::combat::knockback`]),
//!    which the duel applies.
//!
//! Every frame [`CombatRules::recover`] regains posture at the character's speed, scaled by the
//! StaminaControlParam type of its current animation (TAE 960) and its HP-gated SpEffects.
//!
//! Inferred here, not read from the executable (see `docs/COMBAT-RULES.md` for what is):
//! - The player's physical attack power is EquipParamWeapon `attackBasePhysics` times
//!   ReinforceParamWeapon `physicsAtkRate` times AtkParam_Pc `atkPhysCorrection / 100` (the
//!   Dark Souls convention; AtkParam_Pc `atkPhys` is 0). The Kusabimaru's slash is 40 * 300% =
//!   120 for a 300% row (the opening slash uses 200%, giving 80). Attack power selects
//!   SpEffectParam 160000 + N; its element attack rates and staminaAttackRate scale damage.
//! - Wolf's maximum HP, posture and posture recovery are CalcCorrectGraph 500, 501 and 504 at
//!   [`PLAYER_GROWTH_LEVEL`]; which progression value the engine feeds them is open.
//! - The NPC's per-attribute `<attr>DamageCutRate` on unguarded hits is not applied (all 1.0 on
//!   the soldier except dark).

use crate::character::Character;
use crate::combat::damage_type::{DefenderFacts, attacker_reaction, defender_reaction};
use crate::combat::effects::{ActiveEffect, attacker_stamina_attack_rate};
use crate::combat::hit::{
    GuardInput, HitOutcome, npc_guard_repel, player_attack_repel, player_guard_repel,
    resolve_hit_with_repel,
};
use crate::combat::hp::{
    Vitality, attribute_guard_factor, element_damage, final_hp_damage, keep_hp, npc_guard_cut,
    player_guard_cut,
};
use crate::combat::knockback::{
    FLAG_FORCE_HIT_STOP, Knockback, KnockbackClass, KnockbackTable, attacker_hit_stop_scale,
    defender_hit_stop_scale, knockback_distance, outcome_index,
};
use crate::combat::params::{
    AttackProfile, CalcCorrectGraph, NpcCombat, StaminaControl, WeaponGuard,
};
use crate::combat::posture::{
    ControlRange, DefenderPosture, PostureMeter, apply_defender_posture, attacker_breaks,
    attacker_posture_damage, defender_posture_damage, defender_posture_damage_from_base,
    player_posture_base,
};
use crate::combat::recovery::{
    RecoveryAccumulator, RecoveryInputs, player_base_speed, posture_recovery_per_second,
};
use crate::hits::{AttackData, Combatant, HitEvent, HitResult, load_table};
use crate::player::IncomingDamage;
use crate::status::{Applied, Status, StatusBook};
use crate::tae::TaeFrame;
use sekiro_formats::param::Table;
use std::collections::HashMap;
use std::path::Path;

/// The Prayer Necklace growth value fed to CalcCorrectGraph 500/501/504 for Wolf (inferred: 1 is the
/// graphs' first breakpoint, giving 320 HP, 120 posture and 30 posture per second).
pub const PLAYER_GROWTH_LEVEL: f32 = 1.0;

/// CalcCorrectGraph rows: maximum HP, maximum posture, posture recovery speed.
const GRAPH_MAX_HP: i32 = 500;
const GRAPH_MAX_POSTURE: i32 = 501;
const GRAPH_RECOVERY: i32 = 504;
/// CalcCorrectGraph "poison resistance (soul level dependent)": 200 at every stage.
const GRAPH_POISON_RESIST: i32 = 110;

/// ReinforceParamWeapon multipliers of the equipped weapon level.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Reinforce {
    /// physics, magic, fire, thunder, dark attack rates.
    pub attack: [f32; 5],
    pub stamina_atk: f32,
    pub stamina_guard_def: f32,
    pub physics_guard_cut: f32,
    pub passive_stamina_atk: f32,
}

impl Default for Reinforce {
    fn default() -> Self {
        Self {
            attack: [1.0; 5],
            stamina_atk: 1.0,
            stamina_guard_def: 1.0,
            physics_guard_cut: 1.0,
            passive_stamina_atk: 1.0,
        }
    }
}

/// What kind of character a fighter is, with the params its side of the rules reads.
#[derive(Debug, Clone)]
pub enum FighterKind {
    Player {
        weapon: Box<WeaponGuard>,
        /// EquipParamWeapon `attackBase{Physics,Magic,Fire,Thunder,Dark}`.
        attack_base: [f32; 5],
        reinforce: Reinforce,
    },
    Npc(Box<NpcCombat>),
}

/// One character's HP and posture.
#[derive(Debug, Clone)]
pub struct Fighter {
    pub vitality: Vitality,
    pub posture: PostureMeter,
    /// Posture recovery speed of the last frame, points per second.
    pub recovery_rate: f32,
    pub kind: FighterKind,
    recovery: RecoveryAccumulator,
    recovery_base: f32,
    control: Option<StaminaControl>,
    /// Always-on SpEffects (NpcParam `spEffectID0..`), still subject to their HP gates.
    resident: Vec<i32>,
    start_resident: Vec<i32>,
    /// KnockBackParam row timing this character's pushes.
    pub knockback_row: i32,
    start: (Vitality, PostureMeter),
    /// TAE SpEffects active on the last step, to find the ones that just started.
    last_tae_effects: Vec<i32>,
    /// Build-up meters and the effects hits left (see [`crate::status`]).
    pub status: Status,
}

impl Fighter {
    /// Back to full HP and posture.
    pub fn reset(&mut self) {
        (self.vitality, self.posture) = self.start;
        self.resident.clone_from(&self.start_resident);
        self.recovery = RecoveryAccumulator::default();
        self.last_tae_effects.clear();
        self.status.clear();
    }

    /// HP left as a fraction of max.
    pub fn hp_fraction(&self) -> f32 {
        self.vitality.hp as f32 / self.vitality.max_hp.max(1) as f32
    }

    /// Posture *damage* as a fraction of max: 0 is a calm meter, 1 is broken. This is what the
    /// game's bar shows, growing from the centre outward.
    pub fn posture_fraction(&self) -> f32 {
        ((self.posture.max - self.posture.remaining) as f32 / self.posture.max.max(1) as f32)
            .clamp(0.0, 1.0)
    }

    /// Always-on SpEffect ids (NpcParam `spEffectID0..`).
    pub fn resident(&self) -> &[i32] {
        &self.resident
    }
    /// A map script can explicitly remove an effect initially supplied by NpcParam.
    pub fn clear_resident_effect(&mut self, id: i32) {
        self.resident.retain(|effect| *effect != id);
    }

    pub fn dead(&self) -> bool {
        self.vitality.hp <= 0
    }

    fn control_range(&self, stamina_type: Option<i32>) -> Option<ControlRange> {
        let kind = usize::try_from(stamina_type?).ok()?;
        ControlRange::new(self.control.as_ref()?, kind, self.posture.max)
    }
}

/// HP, posture and the hit rules for the characters of one fight.
pub struct CombatRules {
    pub fighters: Vec<Fighter>,
    effects: std::sync::Arc<HashMap<i32, ActiveEffect>>,
    damage_motion: std::sync::Arc<HashMap<i32, DamageMotionEffect>>,
    knockback: std::sync::Arc<KnockbackTable>,
    /// Status fields of every SpEffect (build-up, periodic HP change).
    pub statuses: StatusBook,
}

/// SpEffect category 1001 stores replacement *enum* values, not HKS damage levels.
/// Native bfd980 selects the lowest categoryPriority; 10cea20 indexes dmgLv_*;
/// 10b5100 translates through the executable's table at RVA 2b979e8.
#[derive(Debug, Clone)]
struct DamageMotionEffect {
    priority: u8,
    requires_armour: bool,
    levels: [i32; 12],
}

const DAMAGE_LEVEL_FIELDS: [&str; 12] = [
    "dmgLv_None",
    "dmgLv_S",
    "dmgLv_M",
    "dmgLv_L",
    "dmgLv_BlowM",
    "dmgLv_Push",
    "dmgLv_Strike",
    "dmgLv_BlowS",
    "dmgLv_Min",
    "dmgLv_Uppercut",
    "dmgLv_BlowLL",
    "dmgLv_Breath",
];

fn replacement_level(value: i32) -> Option<i32> {
    const LEVELS: [i32; 12] = [0, 8, 1, 2, 3, 7, 4, 6, 9, 5, 10, 11];
    usize::try_from(value - 1)
        .ok()
        .and_then(|i| LEVELS.get(i).copied())
}

fn param_dirs(cache: &Path) -> (std::path::PathBuf, std::path::PathBuf) {
    (
        cache.join("raw/param/gameparam/gameparam.parambnd.d"),
        cache.join("refs/paramdex/Defs"),
    )
}

fn row_f32(t: Option<&std::sync::Arc<Table>>, id: i32, field: &str) -> Option<f32> {
    let row = t?.find(id)?;
    row.f32(field)
        .ok()
        .or_else(|| row.int(field).ok().map(|v| v as f32))
}

impl CombatRules {
    /// Selects the extracted player-growth SpEffect; survives resets and replaces prior AP.
    pub fn set_attack_power(
        &mut self,
        power: crate::progression::AttackPower,
    ) -> Result<(), String> {
        let id = power.effect_id();
        if !self.effects.contains_key(&id) {
            return Err(format!(
                "missing SpEffectParam {id} for attack power {}",
                power.value()
            ));
        }
        let wolf = self.fighters.first_mut().ok_or("missing Wolf")?;
        if !matches!(wolf.kind, FighterKind::Player { .. }) {
            return Err("first fighter is not Wolf".into());
        }
        wolf.resident.retain(|id| !(160000..=160099).contains(id));
        wolf.resident.push(id);
        wolf.start_resident.clone_from(&wolf.resident);
        Ok(())
    }
    /// Direct-hit posture estimate using the same weapon, defence and effects as resolution.
    /// Used for checkpoint lead time, without applying damage or changing either fighter.
    pub fn wolf_posture_per_hit(&self, attack: &AttackData, chars: [&Character; 2]) -> Option<f32> {
        let atk = attack.profile.as_ref()?;
        let FighterKind::Player {
            weapon, reinforce, ..
        } = &self.fighters[0].kind
        else {
            return None;
        };
        let FighterKind::Npc(npc) = &self.fighters[1].kind else {
            return None;
        };
        let effects_a = self.active_effects(0, chars[0]);
        let effects_d = self.active_effects(1, chars[1]);
        let base = player_posture_base(
            HitOutcome::Hit,
            atk,
            weapon,
            reinforce.stamina_atk,
            attacker_stamina_attack_rate(&effects_a),
        );
        let defence = DefenderPosture {
            attribute_rate: npc.stamina_dmg_rates.get(atk.stamina_attribute),
            guard_def: npc.stamina_guard_def as f32,
            ..DefenderPosture::default()
        };
        let damage =
            defender_posture_damage_from_base(HitOutcome::Hit, base, atk, &defence, &effects_d);
        (damage > 0).then_some(damage as f32)
    }

    /// Builds a fighter for each combatant from the params in `cache`. Index 0 is the player
    /// (weapon `param_id`), the rest NPCs (NpcParam `param_id`).
    pub fn load(cache: &Path, combatants: &[&Combatant]) -> Self {
        let (pd, dd) = param_dirs(cache);
        let sp = load_table(&pd, &dd, "SpEffectParam", "SpEffect");
        let npcs = load_table(&pd, &dd, "NpcParam", "NpcParam");
        let weapons = load_table(&pd, &dd, "EquipParamWeapon", "EquipParamWeapon");
        let reinforce = load_table(&pd, &dd, "ReinforceParamWeapon", "ReinforceParamWeapon");
        let control = load_table(&pd, &dd, "StaminaControlParam", "StaminaControlParam");
        let graphs = load_table(&pd, &dd, "CalcCorrectGraph", "CalcCorrectGraph");
        let tentative = load_table(&pd, &dd, "TentativePlayerParam", "TentativePlayerParam");
        let knockback = crate::shared::load(cache.join("shared-knockback"), || {
            Some(
                load_table(&pd, &dd, "KnockBackParam", "KnockBackParam")
                    .map(|t| KnockbackTable::from_table(&t))
                    .unwrap_or_default(),
            )
        })
        .unwrap();
        let wolf_knockback_row = player_knockback_row(&pd, &dd);

        let effects = crate::shared::load(cache.join("shared-active-effects"), || {
            Some(
                sp.as_ref()
                    .map(|t| {
                        t.rows()
                            .filter_map(|r| ActiveEffect::from_row(&r).ok().map(|e| (r.id(), e)))
                            .collect()
                    })
                    .unwrap_or_default(),
            )
        })
        .unwrap();
        let damage_motion = crate::shared::load(cache.join("shared-damage-motion"), || {
            Some(
                sp.as_ref()
                    .map(|table| {
                        table
                            .rows()
                            .filter(|r| r.int("spCategory").ok() == Some(1001))
                            .map(|r| {
                                (
                                    r.id(),
                                    DamageMotionEffect {
                                        priority: r.int("categoryPriority").unwrap_or(255) as u8,
                                        requires_armour: r.int("stateInfo").ok() == Some(120),
                                        levels: DAMAGE_LEVEL_FIELDS
                                            .map(|field| r.int(field).unwrap_or(0) as i32),
                                    },
                                )
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
            )
        })
        .unwrap();
        let control_row = |id: i32| {
            control
                .as_ref()?
                .find(id)
                .and_then(|r| StaminaControl::from_row(&r).ok())
        };
        let graph = |id: i32| {
            graphs
                .as_ref()?
                .find(id)
                .and_then(|r| CalcCorrectGraph::from_row(&r).ok())
        };
        let statuses = StatusBook::shared(cache, sp.as_deref());
        let tentative_f = |f: &str| row_f32(tentative.as_ref(), 0, f).unwrap_or(0.0);
        let mut wolf_status = Status::new(
            graph(GRAPH_POISON_RESIST).map_or(0.0, |g| g.eval(PLAYER_GROWTH_LEVEL).trunc())
                + player_poison_resist(&pd, &dd),
            tentative_f("ResistRecoverPoint_Poision_Player"),
        );
        wolf_status.terror = crate::status::Meter {
            value: 0.0,
            max: graph(111).map_or(0.0, |g| g.eval(PLAYER_GROWTH_LEVEL).trunc()),
            recover: tentative_f("ResistRecoverPoint_Desease_Player"),
        };
        let npc_poison_recover = tentative_f("ResistRecoverPoint_Poision_Enemy");

        let fighters = combatants
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let fighter = |vit: Vitality,
                               posture: PostureMeter,
                               recovery_base,
                               control,
                               resident: Vec<i32>,
                               status: Status,
                               kind: FighterKind| Fighter {
                    vitality: vit,
                    posture,
                    recovery_rate: 0.0,
                    knockback_row: match &kind {
                        FighterKind::Player { .. } => wolf_knockback_row,
                        FighterKind::Npc(npc) => npc.knockback_param_id as i32,
                    },
                    kind,
                    recovery: RecoveryAccumulator::default(),
                    recovery_base,
                    control,
                    start_resident: resident.clone(),
                    resident,
                    start: (vit, posture),
                    last_tae_effects: Vec::new(),
                    status,
                };
                if i == 0 {
                    let w = weapons.as_ref().and_then(|t| t.find(c.param_id));
                    let weapon = w
                        .as_ref()
                        .and_then(|r| WeaponGuard::from_row(r).ok())
                        .expect("EquipParamWeapon row of the player's weapon");
                    let base = |f: &str| w.as_ref().and_then(|r| r.int(f).ok()).unwrap_or(0) as f32;
                    let attack_base = [
                        base("attackBasePhysics"),
                        base("attackBaseMagic"),
                        base("attackBaseFire"),
                        base("attackBaseThunder"),
                        base("attackBaseDark"),
                    ];
                    let rid = base("reinforceTypeId") as i32;
                    let rf = |f: &str| row_f32(reinforce.as_ref(), rid, f).unwrap_or(1.0);
                    let reinforce = Reinforce {
                        attack: [
                            rf("physicsAtkRate"),
                            rf("magicAtkRate"),
                            rf("fireAtkRate"),
                            rf("thunderAtkRate"),
                            rf("darkAtkRate"),
                        ],
                        stamina_atk: rf("staminaAtkRate"),
                        stamina_guard_def: rf("staminaGuardDefRate"),
                        physics_guard_cut: rf("physicsGuardCutRate"),
                        passive_stamina_atk: rf("passiveStaminaAtkRate"),
                    };
                    let level = |id: i32, fallback: f32| {
                        graph(id).map_or(fallback, |g| g.eval(PLAYER_GROWTH_LEVEL).trunc())
                    };
                    let hp = level(GRAPH_MAX_HP, 320.0) as i32;
                    let posture = level(GRAPH_MAX_POSTURE, 120.0) as i32;
                    let recovery = graph(GRAPH_RECOVERY)
                        .map_or(30.0, |g| player_base_speed(&g, PLAYER_GROWTH_LEVEL));
                    let debt = row_f32(tentative.as_ref(), 0, "DebtSp").unwrap_or(0.0) as i32;
                    fighter(
                        Vitality::new(hp, 0),
                        PostureMeter::full(posture, debt),
                        recovery,
                        control_row(0),
                        Vec::new(),
                        wolf_status.clone(),
                        FighterKind::Player {
                            weapon: Box::new(weapon),
                            attack_base,
                            reinforce,
                        },
                    )
                } else {
                    let row = npcs.as_ref().and_then(|t| t.find(c.param_id));
                    let npc = row
                        .as_ref()
                        .and_then(|r| NpcCombat::from_row(r).ok())
                        .expect("NpcParam row");
                    let resist = row
                        .as_ref()
                        .and_then(|r| r.int("resist_poison").ok())
                        .unwrap_or(0) as f32;
                    fighter(
                        Vitality::new(npc.hp, npc.deathblows),
                        PostureMeter::full(npc.max_posture, npc.max_debt_stamina),
                        npc.stamina_recover_base_vel,
                        control_row(npc.stamina_control_param_id as i32),
                        npc.sp_effect_ids
                            .iter()
                            .copied()
                            .filter(|&id| id > 0)
                            .collect(),
                        Status::new(resist, npc_poison_recover),
                        FighterKind::Npc(Box::new(npc)),
                    )
                }
            })
            .collect();
        Self {
            fighters,
            effects,
            damage_motion,
            knockback,
            statuses,
        }
    }

    /// The damage level HKS sees after active category-1001 SpEffects replace it.
    /// Guard contacts bypass replacement, as in native bdbac0/84ff70.
    fn damage_motion_level(
        &self,
        d: usize,
        frame: &TaeFrame,
        level: i32,
        guarded: bool,
        armoured: bool,
    ) -> i32 {
        if guarded || level <= 0 {
            return level;
        }
        let Some(index) = usize::try_from(level).ok().filter(|i| *i < 12) else {
            return level;
        };
        frame
            .sp_effects
            .iter()
            .chain(self.fighters[d].resident.iter())
            .filter(|id| {
                self.effects.get(id).is_some_and(|e| {
                    e.hp_gate_open(
                        self.fighters[d].vitality.hp,
                        self.fighters[d].vitality.max_hp,
                    )
                })
            })
            .filter_map(|id| self.damage_motion.get(id))
            .filter(|e| !e.requires_armour || armoured)
            .min_by_key(|e| e.priority)
            .and_then(|e| replacement_level(e.levels[index]))
            .unwrap_or(level)
    }

    /// The SpEffects acting on fighter `i`: its animation's plus its resident ones, each only
    /// while its HP gates are open.
    pub fn active_effects(&self, i: usize, character: &Character) -> Vec<ActiveEffect> {
        self.effects_for(i, character.tae_frame())
    }

    pub(crate) fn effects_for(&self, i: usize, frame: &TaeFrame) -> Vec<ActiveEffect> {
        let f = &self.fighters[i];
        frame
            .sp_effects
            .iter()
            .chain(f.resident.iter())
            .filter_map(|id| self.effects.get(id))
            .filter(|e| e.hp_gate_open(f.vitality.hp, f.vitality.max_hp))
            .cloned()
            .collect()
    }

    /// Applies the HP change (`changeHpRate`) of every TAE SpEffect that started on fighter `i`
    /// this step, once. Multi-life enemies get their HP back this way: the get-up part of a
    /// non-lethal deathblow (`a000_012000` of c5060 at 4.4 s) adds SpEffect 300610 "HP reset"
    /// with -100 %. That the rate applies once at the start of a timed effect is inferred.
    pub fn apply_effect_hp(&mut self, i: usize, frame: &TaeFrame) {
        let started: Vec<f32> = frame
            .sp_effects
            .iter()
            .filter(|id| !self.fighters[i].last_tae_effects.contains(id))
            // Periodic effects (poison) change HP once per interval instead (`crate::status`).
            .filter(|id| !self.statuses.periodic(**id))
            .filter_map(|id| self.effects.get(id))
            .map(|e| e.change_hp_rate)
            .filter(|&r| r != 0.0)
            .collect();
        let f = &mut self.fighters[i];
        f.last_tae_effects.clone_from(&frame.sp_effects);
        if f.dead() {
            return;
        }
        for rate in started {
            let change = (f.vitality.max_hp as f32 * rate / 100.0).round() as i32;
            let hp = f.vitality.hp - change;
            f.vitality.set_hp(hp);
        }
    }

    /// Regains posture for fighter `i` over `dt` seconds (read from code, see
    /// [`crate::combat::recovery`]). The animation's control type also clamps the meter, which
    /// is how a posture break ends at a fixed value.
    pub fn recover(&mut self, i: usize, character: &Character, dt: f32) {
        self.recover_with_state(
            i,
            character.tae_frame(),
            character.behavior.stamina_recovery_disabled(),
            dt,
        );
    }

    /// [`Self::recover`] for a character whose TAE state is `frame`. TAE 225
    /// (`SetSPRegenRatePercent`) fills the animation percent of the recovery speed: the
    /// soldier's guard and deflect animations carry 33 %, its burn reactions 0 % (the event and
    /// its effect traced by FwLo's RustSekiro, `docs/kb/guard-deflect.md`; that it is the
    /// percent byte `a04850` reads is inferred).
    pub fn recover_with_frame(&mut self, i: usize, frame: &TaeFrame, dt: f32) {
        self.recover_with_state(i, frame, false, dt);
    }

    /// [`Self::recover_with_frame`] for a character whose script may have called `act(110)`
    /// (`disabled`, see [`crate::player::ACT_SET_STAMINA_RECOVERY_DISABLED`]). The engine's
    /// update (`a04850`) then skips the whole step: no gain, no carry change and no new speed
    /// for the debug display, so [`Fighter::recovery_rate`] keeps its last value. This is the
    /// only "no recovery" flag the engine has; there is no timer for a delay after damage, so
    /// pauses come from animations (TAE 225 and the StaminaControlParam type of TAE 960).
    pub fn recover_with_state(&mut self, i: usize, frame: &TaeFrame, disabled: bool, dt: f32) {
        if disabled {
            return;
        }
        let effects = self.effects_for(i, frame);
        let stamina_type = frame.stamina_type;
        let f = &mut self.fighters[i];
        let mut inputs = RecoveryInputs::base(f.recovery_base);
        if let Some(scale) = frame.posture_regen_scale {
            inputs.anim_percent = (scale * 100.0).round().clamp(0.0, 255.0) as u8;
        }
        inputs.control_ratio = stamina_type
            .zip(f.control.as_ref())
            .map(|(t, c)| c.recover_ratio(t.max(0) as usize));
        let rate = posture_recovery_per_second(inputs, &effects);
        f.recovery_rate = rate;
        let gain = f.recovery.tick(rate, dt);
        let control = f.control_range(stamina_type);
        if f.dead() {
            return;
        }
        let target = f.posture.remaining + gain;
        f.posture.set(target, false, control);
    }

    /// Forecast posture for a specified contact outcome using the live resolution path.
    #[allow(clippy::too_many_arguments)]
    pub fn posture_for_outcome(
        &self,
        a: usize,
        d: usize,
        atk: &AttackProfile,
        outcome: HitOutcome,
        guard_row: Option<&AttackData>,
        effects_a: &[ActiveEffect],
        effects_d: &[ActiveEffect],
    ) -> i32 {
        let deflect = outcome == HitOutcome::Deflect;
        let guard_profile = guard_row.and_then(|g| g.profile.as_ref());
        let attack_rate = attacker_stamina_attack_rate(effects_a);
        let mut defender_side = match &self.fighters[d].kind {
            FighterKind::Player {
                weapon, reinforce, ..
            } => DefenderPosture::player(weapon, deflect, reinforce.stamina_guard_def),
            FighterKind::Npc(npc) => DefenderPosture {
                attribute_rate: npc.stamina_dmg_rates.get(atk.stamina_attribute),
                guard_def: npc.stamina_guard_def as f32,
                ..DefenderPosture::default()
            },
        };
        if let Some(g) = guard_profile {
            defender_side.guard_row_cut_rate = g.guard_stamina_cut_rate;
        }
        defender_side.guard_behavior_stamina = guard_row.map_or(0, |g| g.behavior_stamina);
        match &self.fighters[a].kind {
            FighterKind::Player {
                weapon, reinforce, ..
            } => {
                let base =
                    player_posture_base(outcome, atk, weapon, reinforce.stamina_atk, attack_rate);
                defender_posture_damage_from_base(outcome, base, atk, &defender_side, effects_d)
            }
            FighterKind::Npc(_) => {
                defender_posture_damage(outcome, atk, attack_rate, &defender_side, effects_d)
            }
        }
    }

    /// Resolves a contact, applying HP and posture damage to both fighters.
    pub fn resolve(
        &mut self,
        hit: &HitEvent,
        chars: [&Character; 2],
        combatants: [&Combatant; 2],
    ) -> HitResult {
        self.resolve_contact(hit, chars, combatants, false, false)
    }

    /// A projectile owns the contact; its shooter supplies damage scaling, but does not
    /// receive character recoil or posture feedback from the projectile's collision.
    /// Use the actual contact source, not `isArrowAtk` (zero on Owl's shuriken).
    pub fn resolve_bullet(
        &mut self,
        hit: &HitEvent,
        chars: [&Character; 2],
        combatants: [&Combatant; 2],
    ) -> HitResult {
        self.resolve_contact(hit, chars, combatants, true, false)
    }

    /// A static event emitter owns scaling, with no animation effects from the visible boss.
    pub fn resolve_event_bullet(
        &mut self,
        hit: &HitEvent,
        chars: [&Character; 2],
        combatants: [&Combatant; 2],
        owner: &mut Fighter,
    ) -> HitResult {
        std::mem::swap(&mut self.fighters[hit.attacker], owner);
        let result = self.resolve_contact(hit, chars, combatants, true, true);
        std::mem::swap(&mut self.fighters[hit.attacker], owner);
        result
    }

    fn resolve_contact(
        &mut self,
        hit: &HitEvent,
        chars: [&Character; 2],
        combatants: [&Combatant; 2],
        from_bullet: bool,
        static_owner: bool,
    ) -> HitResult {
        let (a, d) = (hit.attacker, hit.defender);
        let Some(atk) = hit.attack.profile.clone() else {
            return HitResult::default();
        };
        let (attacker, defender) = (chars[a], chars[d]);
        let effects_a = if static_owner {
            self.effects_for(a, &TaeFrame::default())
        } else {
            self.active_effects(a, attacker)
        };
        let effects_d = self.active_effects(d, defender);
        let frame_d = defender.tae_frame();
        let guard_row: Option<&AttackData> = frame_d
            .guard_judge
            .and_then(|j| combatants[d].attacks.get(&j));
        let guard_profile = guard_row.and_then(|g| g.profile.as_ref());

        // 1. Guard test.
        let fd = &self.fighters[d];
        let (guard_angle, base_repel) = match &fd.kind {
            FighterKind::Player { weapon, .. } => (
                weapon.guard_angle,
                player_guard_repel(
                    guard_profile.map_or(100, |g| g.guard_break_correction),
                    weapon.guard_base_repel,
                    &effects_d,
                ),
            ),
            FighterKind::Npc(npc) => (
                npc.guard_angle,
                npc_guard_repel(guard_profile.map_or(0, |g| g.guard_break_rate), &effects_d),
            ),
        };
        let guard = GuardInput {
            guarding: frame_d.flag(crate::hits::FLAG_GUARD),
            forward: defender.body.forward(),
            guard_angle,
            base_repel,
            guard_attribute: guard_profile.map_or(0, |g| g.guard_attribute),
        };
        let repel_power = match &self.fighters[a].kind {
            FighterKind::Player { weapon, .. } if atk.guard_atk_rate_correction > 0 => {
                player_attack_repel(&atk, weapon.attack_base_repel)
            }
            _ => atk.guard_atk_rate as i32,
        };
        let resolution =
            resolve_hit_with_repel(&atk, repel_power, hit.direction, &guard, &effects_d);
        let outcome = resolution.outcome;
        let deflect = outcome == HitOutcome::Deflect;

        // 2. Posture damage to the defender.
        let posture_damage =
            self.posture_for_outcome(a, d, &atk, outcome, guard_row, &effects_a, &effects_d);
        let control_d = self.fighters[d].control_range(frame_d.stamina_type);
        let contact = apply_defender_posture(
            &mut self.fighters[d].posture,
            outcome,
            posture_damage,
            &atk,
            control_d,
        );
        let guard_broken = contact.broke && outcome == HitOutcome::Block;

        // 3. Posture damage back to the attacker.
        let weapon_rate = match &self.fighters[d].kind {
            FighterKind::Player {
                weapon, reinforce, ..
            } => weapon.passive_stamina_atk_rate * reinforce.passive_stamina_atk,
            FighterKind::Npc(_) => 1.0,
        };
        let attacker_damage = if from_bullet {
            0
        } else {
            attacker_posture_damage(outcome, &atk, guard_broken, weapon_rate, &effects_d)
        };
        let attacker_broke = !from_bullet
            && outcome.guarded()
            && attacker_breaks(&self.fighters[a].posture, attacker_damage);
        let control_a = self.fighters[a].control_range(attacker.tae_frame().stamina_type);
        if !from_bullet {
            self.fighters[a].posture.take(attacker_damage, control_a);
        }

        // 4. HP damage.
        let hp_damage = self.hp_damage(
            (a, d),
            &atk,
            &hit.attack,
            outcome,
            guard_profile,
            (&effects_a, &effects_d),
        );
        let fd = &mut self.fighters[d];
        let hp_damage = keep_hp(fd.vitality.hp, hp_damage, atk.excess_dmg_keep_hp);
        fd.vitality.take(hp_damage, false);
        let dead = fd.dead();
        let broke = outcome == HitOutcome::Hit && (contact.broke || fd.vitality.deathblow_ready());

        // 5. Reaction codes.
        let defender_code = defender_reaction(DefenderFacts {
            outcome,
            dead,
            guard_broken,
            guard_break_reaction: false,
            damage_level: hit.attack.damage_level.max(0) as u32,
            broke,
            attacker_broke,
            does_break_repel_stam_damage: atk.does_break_repel_stam_damage,
        })
        .code();
        let attacker_code = if from_bullet {
            None
        } else {
            attacker_reaction(
                outcome,
                guard_broken,
                hit.attack.category,
                false,
                attacker_broke,
                atk.does_break_repel_stam_damage,
            )
        };

        // 6. Knockback and hit stop.
        // Native ChrActionFlag 24 sets record +0x1CA: b6bfe0 skips knockback and b68d00
        // publishes damage level 0. The hit still deals HP/posture damage.
        let motion_disabled = frame_d.flag(crate::tae::cancel::DISABLE_DAMAGE_MOTION);
        let knockback = if motion_disabled {
            None
        } else {
            self.knockback(a, d, &atk, hit, outcome, frame_d.knockback_scale)
        };
        let mut hit_stop = self.hit_stop(d, &atk, frame_d);
        if from_bullet {
            hit_stop[0] = 0.0;
        }

        let (direction, front_back) = hit_direction(defender, hit.direction);
        // A hit with no damage level that carries a behaviour identification value (the smoke
        // screen's blinding, 17) reaches the script as DAMAGE_LEVEL_NONE, the level its
        // blinding branch tests (`c0000_transition.hks`); other hits keep at least level 1.
        let level = if motion_disabled || (hit.attack.damage_level <= 0 && atk.behavior_id != 0) {
            0
        } else {
            hit.attack.damage_level.max(1)
        };
        // These NPCs have zero superArmorDurability and toughness. StateInfo 120
        // replacements require a live armour meter; ordinary category-1001 effects do not.
        let level = self.damage_motion_level(d, frame_d, level, outcome.guarded(), false);
        HitResult {
            defender: IncomingDamage {
                damage_type: defender_code,
                level,
                direction,
                front_back,
                guard_level: if outcome.guarded() { 1 } else { 0 },
                // Only the NPC script reads env 258; the player's guard reactions use env 237.
                guard_motion_level: match &self.fighters[d].kind {
                    FighterKind::Npc(npc) => npc.guard_level,
                    FighterKind::Player { .. } => 0,
                },
                repel_dir: if outcome.guarded() { 2 } else { 0 },
                element: atk.sp_attribute as i32,
                behavior_id: atk.behavior_id as i32,
                ..IncomingDamage::default()
            },
            attacker: attacker_code.map(|c| IncomingDamage {
                damage_type: c.0,
                level: 1,
                just_repelled: i32::from(deflect),
                repelled: i32::from(!deflect),
                ..IncomingDamage::default()
            }),
            hp_damage,
            posture_damage: contact.applied,
            attacker_posture_damage: attacker_damage,
            deflected: deflect,
            guarded: outcome.guarded(),
            // A guard break empties the defender's posture just as a direct hit's break does
            // (`b6e6a0` keeps them as separate flags, +0x1C4 and +0x1C6, that pick the reaction code).
            posture_broke: broke || guard_broken || attacker_broke,
            knockback,
            hit_stop,
        }
    }

    /// The attacker's and the defender's hit stop lengths for a contact of `atk` on fighter `d`
    /// (`b6e5a0` into `b9cd90` and `b9cfb0`; see [`crate::combat::knockback`]). The engine starts
    /// them for every contact its damage module takes, blocked and deflected ones too, as long as
    /// the attack carries any attack power (always, for the attacks we resolve). An active TAE
    /// action flag 41 on the defender forces both to full for attacks that are not arrows.
    fn hit_stop(&self, d: usize, atk: &AttackProfile, frame_d: &TaeFrame) -> [f32; 2] {
        let (attacker_type, defender_type) = match &self.fighters[d].kind {
            FighterKind::Npc(npc) => (Some(npc.hit_stop_type), Some(npc.hit_stop_type_defender)),
            FighterKind::Player { .. } => (None, None),
        };
        let forced = !atk.is_arrow && frame_d.flag(FLAG_FORCE_HIT_STOP);
        let scale = |s: f32| if forced { 1.0 } else { s };
        [
            atk.hit_stop_time * scale(attacker_hit_stop_scale(attacker_type)),
            atk.hit_stop_time_defender * scale(defender_hit_stop_scale(defender_type)),
        ]
    }

    /// The push fighter `d` takes from `atk` (see [`crate::combat::knockback`]): the AtkParam
    /// distance for the outcome, cut by an NPC defender's NpcParam rate against the attacker's
    /// side, scaled by the defender's TAE 226, timed by its KnockBackParam row, along the hit.
    fn knockback(
        &self,
        a: usize,
        d: usize,
        atk: &AttackProfile,
        hit: &HitEvent,
        outcome: HitOutcome,
        tae_scale: Option<f32>,
    ) -> Option<Knockback> {
        let idx = outcome_index(outcome);
        let cut = match &self.fighters[d].kind {
            FighterKind::Npc(npc) => match self.fighters[a].kind {
                FighterKind::Player { .. } => npc.knockback_cut_vs_player[idx],
                FighterKind::Npc(_) => npc.knockback_cut_vs_enemy[idx],
            },
            FighterKind::Player { .. } => 0,
        };
        let distance = knockback_distance(atk.knockback_dist[idx], cut, tae_scale);
        let class = KnockbackClass::for_hit(outcome, hit.attack.damage_level);
        let timing = self
            .knockback
            .timing(self.fighters[d].knockback_row, class)?;
        Knockback::start(hit.direction, distance, timing)
    }

    /// HP damage of `atk` from fighter `a` on fighter `d` for `outcome`.
    pub(crate) fn hp_damage(
        &self,
        sides: (usize, usize),
        atk: &AttackProfile,
        data: &AttackData,
        outcome: HitOutcome,
        guard: Option<&AttackProfile>,
        (effects, defender_effects): (&[ActiveEffect], &[ActiveEffect]),
    ) -> i32 {
        let (a, d) = sides;
        let attack = match &self.fighters[a].kind {
            FighterKind::Player {
                attack_base,
                reinforce,
                ..
            } => std::array::from_fn(|i| {
                atk.attack_power[i]
                    + attack_base[i] * reinforce.attack[i] * data.atk_corrections[i] as f32 * 0.01
            }),
            FighterKind::Npc(_) => atk.attack_power,
        };
        let mut mult = [1.0; 5];
        if outcome.guarded() {
            // `atkAttribute` 1.. indexes slash, blow, thrust, ... (0 means none).
            let attr = (atk.atk_attribute as usize).checked_sub(1);
            let guard_rate = guard.map_or(0, |g| g.guard_rate);
            mult[0] = match &self.fighters[d].kind {
                FighterKind::Player {
                    weapon, reinforce, ..
                } => {
                    let rate = attr
                        .and_then(|i| weapon.guard_cut_attribute_rates.get(i))
                        .map_or(0.0, |&r| r as f32);
                    let cut = if outcome == HitOutcome::Deflect {
                        weapon.phys_just_guard_cut_rate
                    } else {
                        weapon.phys_guard_cut_rate
                    };
                    player_guard_cut(
                        cut,
                        reinforce.physics_guard_cut,
                        attribute_guard_factor(rate),
                        guard_rate,
                    )
                }
                FighterKind::Npc(npc) => {
                    let rate = attr
                        .and_then(|i| npc.guard_cut_attribute_rates.get(i))
                        .map_or(0.0, |&r| r as f32);
                    npc_guard_cut(
                        npc.phys_guard_cut_rate,
                        guard_rate,
                        attribute_guard_factor(rate),
                    )
                }
            };
        }
        for (i, rate) in mult.iter_mut().enumerate() {
            *rate *= effects
                .iter()
                .map(|e| e.hp_attack_rates[i])
                .product::<f32>();
        }
        // The defender's SpEffect cut rates: physical by the attack's attribute, then elements.
        for e in defender_effects {
            if let Some(cut) = (atk.atk_attribute as usize)
                .checked_sub(1)
                .and_then(|i| e.hp_cut_phys.get(i))
            {
                mult[0] *= cut;
            }
            for (k, cut) in e.hp_cut_elements.iter().enumerate() {
                mult[k + 1] *= cut;
            }
        }
        let total = element_damage(attack, [0.0; 5], mult, 1.0);
        final_hp_damage(total, 1.0, 1.0)
    }

    /// Ends fighter `d` with a deathblow: spends a deathblow mark and takes all its HP (the
    /// deathblow's own damage is whatever is left, as every Sekiro deathblow on a last mark).
    pub fn deathblow(&mut self, d: usize) {
        let v = &mut self.fighters[d].vitality;
        let hp = v.hp;
        v.cannot_die = false;
        v.take(hp, true);
        if v.deathblows_left == 0 {
            v.set_hp(0);
        }
    }

    /// Whether fighter `d` can be deathblown now: posture broken, or down to 1 HP with marks
    /// left (read from code, see [`Vitality::deathblow_ready`]).
    pub fn deathblow_open(&self, d: usize) -> bool {
        let f = &self.fighters[d];
        !f.dead() && (f.posture.broken() || f.vitality.deathblow_ready())
    }

    pub fn reset(&mut self) {
        for f in &mut self.fighters {
            f.reset();
        }
    }

    /// Applies the SpEffects a hit leaves on fighter `d` (a bullet's `spEffectId0..4`): poison
    /// build-up and timed effects (see [`crate::status`]). The returned timed effects belong in
    /// the character's TAE runtime so its scripts, AI and posture rules see them.
    pub fn apply_hit_effects(&mut self, d: usize, ids: &[i32]) -> Applied {
        if self.fighters[d].dead() {
            return Applied::default();
        }
        let book = &self.statuses;
        let applied = self.fighters[d].status.apply(book, ids);
        let vitality = &mut self.fighters[d].vitality;
        let damage = (vitality.max_hp as f32 * applied.terror_hp_rate / 100.0) as i32
            + applied.terror_hp_point;
        if damage > 0 {
            vitality.set_hp(vitality.hp - damage);
        }
        applied
    }

    /// Advances fighter `i`'s status by `dt` seconds and applies the HP its periodic effects take
    /// (poison). Returns that HP.
    pub fn tick_status(&mut self, i: usize, dt: f32) -> i32 {
        let f = &mut self.fighters[i];
        let hp = f.status.tick(&self.statuses, f.vitality.max_hp, dt);
        if hp != 0 && !f.dead() {
            let left = f.vitality.hp - hp;
            f.vitality.set_hp(left.min(f.vitality.max_hp));
        }
        hp
    }
}

/// Wolf's armour poison resistance: the sum of `resistPoison` over the pieces of CharaInitParam
/// 10010 (all 0; which pieces the engine sums is not traced).
fn player_poison_resist(pd: &Path, dd: &Path) -> f32 {
    let init = load_table(pd, dd, "CharaInitParam", "CharaInitParam");
    let armour = load_table(pd, dd, "EquipParamProtector", "EquipParamProtector");
    let (Some(init), Some(armour)) = (init, armour) else {
        return 0.0;
    };
    let Some(row) = init.find(10010) else {
        return 0.0;
    };
    ["equip_Helm", "equip_Armer", "equip_Gaunt", "equip_Leg"]
        .iter()
        .filter_map(|f| row.int(f).ok())
        .filter_map(|id| armour.find(id as i32))
        .filter_map(|p| p.int("resistPoison").ok())
        .sum::<i64>() as f32
}

/// Wolf's KnockBackParam row: the highest EquipParamProtector `knockbackParamId` among the
/// armour of CharaInitParam 10010 (every outfit piece has 1, "For PC"; found in the data by
/// FwLo's RustSekiro, `docs/kb/damage.md`; which piece the engine reads is not traced).
fn player_knockback_row(pd: &Path, dd: &Path) -> i32 {
    let init = load_table(pd, dd, "CharaInitParam", "CharaInitParam");
    let armour = load_table(pd, dd, "EquipParamProtector", "EquipParamProtector");
    let (Some(init), Some(armour)) = (init, armour) else {
        return 0;
    };
    let Some(row) = init.find(10010) else {
        return 0;
    };
    ["equip_Helm", "equip_Armer", "equip_Gaunt", "equip_Leg"]
        .iter()
        .filter_map(|f| row.int(f).ok())
        .filter_map(|id| armour.find(id as i32))
        .filter_map(|p| p.int("knockbackParamId").ok())
        .max()
        .unwrap_or(0) as i32
}

/// Hit direction codes for the defender's script: `(direction, front_back)`. Direction is 0
/// left, 1 right, 2 front, 3 back as the defender sees the attacker; front_back 0 front, 1 back.
fn hit_direction(defender: &Character, travel: [f32; 2]) -> (i32, i32) {
    let [fx, fz] = defender.body.forward();
    // From the defender toward the attacker.
    let (ix, iz) = (-travel[0], -travel[1]);
    let front = fx * ix + fz * iz;
    let right = fz * ix - fx * iz;
    let from_front = front >= 0.0;
    let direction = if front.abs() >= right.abs() {
        if from_front { 2 } else { 3 }
    } else if right > 0.0 {
        1
    } else {
        0
    };
    (direction, if from_front { 0 } else { 1 })
}
