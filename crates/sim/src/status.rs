//! Status effects that hits leave on a character: SpEffects with a duration (the shuriken's slower
//! posture recovery, the recovery seal), build-up meters (poison) and effects that work on a
//! period (poison damage over time).
//!
//! The data trail:
//!
//! - A bullet's `spEffectId0..4` are applied to the character it hits (Paramdex: "registers
//!   SpEffect ids"; that the target gets them is the community reading, `docs/PARAMS.md`).
//! - SpEffectParam `poizonAttackPower` is "added to the target's poison resistance value on a
//!   hit" (def). When the meter reaches the character's resistance, the effect's
//!   `replaceSpEffectId` is applied and the meter empties. The def calls that field "the SpEffect
//!   added when the lifetime ends"; using it as the build-up's result is the community reading
//!   for this family of games (inferred, not traced). The Owl's poison floor 9019 adds 30 and
//!   names 9050, the enemies' common poison.
//! - Wolf's poison resistance is CalcCorrectGraph 110 "poison resistance (soul level)", flat 200
//!   at every stage, plus his armour's `resistPoison` (0 on every starting piece); an NPC's is
//!   NpcParam `resist_poison`. The meter falls by TentativePlayerParam
//!   `ResistRecoverPoint_Poision_Player` (5) or `_Enemy` (5) per second (the unit is inferred).
//! - An effect with `motionInterval` above 0 works once per interval while it lasts
//!   (`effectEndurance`): it changes HP by `changeHpRate` percent of max HP plus
//!   `changeHpPoint` (def: "per activation"). 9050 takes 0.25 % + 12 every 1.0 s for 39.9 s.
//!   That the first tick comes one interval after the effect starts, and that the sum truncates
//!   like the engine's other HP maths, is ours.
//! - `disablePoison` on an active effect stops new poison build-up (9050 has it, so a poisoned
//!   character's meter does not refill while the poison runs).
//!
//! Timed effects are also handed to the character's TAE runtime, so its scripts (`env(3036)`
//! behaviour refs: 500 is `SP_EF_REF_POISON`), its AI's observed effects and the posture rules
//! (`staminaRecoverChangeSpeed` of 113000) see them like any other active SpEffect.

use std::collections::HashMap;

use sekiro_formats::param::Table;

/// The parts of a SpEffectParam row that status handling reads.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct StatusEffect {
    pub id: i32,
    /// `effectEndurance`, seconds; 0 is a one-off effect at the moment of the hit.
    pub duration: f32,
    /// `motionInterval`, seconds between activations; 0 means it does not repeat.
    pub interval: f32,
    /// `changeHpRate`, percent of max HP taken per activation (negative heals).
    pub change_hp_rate: f32,
    /// `changeHpPoint`, HP taken per activation.
    pub change_hp_point: i32,
    /// SpEffect posture changes; negative values remove remaining stamina.
    pub change_stamina_rate: f32,
    pub change_stamina_point: i32,
    /// Electric-shock build-up uses the legacy freeze field.
    pub electric_attack: i32,
    /// `poizonAttackPower`: poison build-up added on a hit.
    pub poison_attack: i32,
    /// Sekiro terror uses the legacy disease resistance field.
    pub terror_attack: i32,
    pub disable_terror: bool,
    /// `replaceSpEffectId`: the effect a filled meter applies (see the module notes).
    pub replace: i32,
    /// `disablePoison`.
    pub disable_poison: bool,
    pub state_info: i32,
}

impl StatusEffect {
    fn from_row(row: &sekiro_formats::param::Row) -> Self {
        let f = |n: &str| row.f32(n).unwrap_or(0.0);
        let i = |n: &str| row.int(n).unwrap_or(0) as i32;
        Self {
            id: row.id(),
            duration: f("effectEndurance"),
            interval: f("motionInterval"),
            change_hp_rate: f("changeHpRate"),
            change_hp_point: i("changeHpPoint"),
            change_stamina_rate: f("changeStaminaRate"),
            change_stamina_point: i("changeStaminaPoint"),
            electric_attack: i("registFreeze"),
            poison_attack: i("poizonAttackPower"),
            terror_attack: i("registIllness"),
            disable_terror: i("disableDisease") != 0,
            replace: i("replaceSpEffectId"),
            disable_poison: i("disablePoison") != 0,
            state_info: i("stateInfo"),
        }
    }

    /// HP one activation takes from a character with `max_hp`.
    pub fn tick_damage(&self, max_hp: i32) -> i32 {
        (max_hp as f32 * self.change_hp_rate / 100.0 + self.change_hp_point as f32) as i32
    }
}

/// Every SpEffect's status fields, by id.
#[derive(Debug, Clone, Default)]
pub struct StatusBook {
    effects: std::sync::Arc<HashMap<i32, StatusEffect>>,
}

impl StatusBook {
    pub fn shared(cache: &std::path::Path, table: Option<&Table>) -> Self {
        Self {
            effects: crate::shared::load(cache.join("shared-statuses"), || {
                Some(
                    table
                        .map(|t| {
                            t.rows()
                                .map(|r| (r.id(), StatusEffect::from_row(&r)))
                                .collect()
                        })
                        .unwrap_or_default(),
                )
            })
            .unwrap(),
        }
    }
    pub fn from_table(table: &Table) -> Self {
        Self {
            effects: table
                .rows()
                .map(|r| (r.id(), StatusEffect::from_row(&r)))
                .collect::<HashMap<_, _>>()
                .into(),
        }
    }

    pub fn get(&self, id: i32) -> Option<&StatusEffect> {
        self.effects.get(&id)
    }

    /// Whether an effect works on a period (its HP change is handled here, not at its start).
    pub fn periodic(&self, id: i32) -> bool {
        self.get(id).is_some_and(|e| e.interval > 0.0)
    }

    pub fn insert(&mut self, effect: StatusEffect) {
        std::sync::Arc::make_mut(&mut self.effects).insert(effect.id, effect);
    }
}

/// A build-up meter (poison).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Meter {
    pub value: f32,
    /// The resistance: the meter triggers at this value.
    pub max: f32,
    /// Points the meter loses per second.
    pub recover: f32,
}

impl Meter {
    /// Build-up as a fraction of the resistance.
    pub fn fraction(&self) -> f32 {
        if self.max <= 0.0 {
            0.0
        } else {
            (self.value / self.max).clamp(0.0, 1.0)
        }
    }
}

/// A status effect running on a character.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Running {
    pub id: i32,
    pub remaining: f32,
    /// Seconds to the next activation of a periodic effect.
    pub next_tick: f32,
}

/// One character's status: its poison meter and the effects hits left on it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Status {
    pub poison: Meter,
    pub terror: Meter,
    pub running: Vec<Running>,
}

/// What applying a hit's effects did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Applied {
    /// Timed effects started or refreshed: (id, seconds), for the character's TAE runtime.
    pub timed: Vec<(i32, f32)>,
    /// The poison meter filled this hit.
    pub poisoned: bool,
    /// HP taken when terror reaches resistance, from the buildup row.
    pub terror_hp_rate: f32,
    pub terror_hp_point: i32,
}

impl Status {
    pub fn new(poison_resist: f32, poison_recover: f32) -> Self {
        Self {
            poison: Meter {
                value: 0.0,
                max: poison_resist,
                recover: poison_recover,
            },
            running: Vec::new(),
            terror: Meter::default(),
        }
    }

    /// Back to no build-up and no effects.
    pub fn clear(&mut self) {
        self.poison.value = 0.0;
        self.terror.value = 0.0;
        self.running.clear();
    }

    pub fn has(&self, id: i32) -> bool {
        self.running.iter().any(|r| r.id == id)
    }

    /// Seconds left of effect `id`.
    pub fn remaining(&self, id: i32) -> Option<f32> {
        self.running
            .iter()
            .find(|r| r.id == id)
            .map(|r| r.remaining)
    }

    fn start(&mut self, e: &StatusEffect, applied: &mut Applied) {
        if e.duration <= 0.0 {
            return;
        }
        match self.running.iter_mut().find(|r| r.id == e.id) {
            // A running effect is refreshed, its period keeps its phase (ours).
            Some(r) => r.remaining = r.remaining.max(e.duration),
            None => self.running.push(Running {
                id: e.id,
                remaining: e.duration,
                next_tick: e.interval,
            }),
        }
        applied.timed.push((e.id, e.duration));
    }

    /// Applies the SpEffects `ids` of one hit.
    pub fn apply(&mut self, book: &StatusBook, ids: &[i32]) -> Applied {
        let mut applied = Applied::default();
        for &id in ids {
            let Some(e) = book.get(id).copied() else {
                continue;
            };
            if e.poison_attack > 0 && self.poison.max > 0.0 && !self.poison_blocked(book) {
                self.poison.value += e.poison_attack as f32;
                if self.poison.value >= self.poison.max {
                    self.poison.value = 0.0;
                    if let Some(result) = book.get(e.replace).copied() {
                        self.start(&result, &mut applied);
                        applied.poisoned = true;
                    }
                }
            }
            self.start(&e, &mut applied);
            if e.terror_attack > 0
                && self.terror.max > 0.0
                && !self
                    .running
                    .iter()
                    .any(|r| book.get(r.id).is_some_and(|e| e.disable_terror))
            {
                self.terror.value += e.terror_attack as f32;
                if self.terror.value >= self.terror.max {
                    self.terror.value = 0.0;
                    applied.terror_hp_rate += e.change_hp_rate;
                    applied.terror_hp_point += e.change_hp_point;
                    if let Some(result) = book.get(e.replace) {
                        self.start(result, &mut applied);
                    }
                }
            }
        }
        applied
    }

    fn poison_blocked(&self, book: &StatusBook) -> bool {
        self.running
            .iter()
            .any(|r| book.get(r.id).is_some_and(|e| e.disable_poison))
    }

    /// Advances every effect and the meter by `dt` seconds; returns the HP the periodic effects
    /// took (negative heals).
    pub fn tick(&mut self, book: &StatusBook, max_hp: i32, dt: f32) -> i32 {
        self.poison.value = (self.poison.value - self.poison.recover * dt).max(0.0);
        self.terror.value = (self.terror.value - self.terror.recover * dt).max(0.0);
        let mut hp = 0;
        for r in &mut self.running {
            r.remaining -= dt;
            let Some(e) = book.get(r.id) else { continue };
            if e.interval > 0.0 {
                r.next_tick -= dt;
                // An activation that falls exactly on the end still counts.
                while r.next_tick <= 1e-5 && r.remaining > -1e-5 {
                    hp += e.tick_damage(max_hp);
                    r.next_tick += e.interval;
                }
            }
        }
        self.running.retain(|r| r.remaining > 1e-5);
        hp
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn book() -> StatusBook {
        let mut b = StatusBook::default();
        b.insert(StatusEffect {
            id: 9019,
            poison_attack: 30,
            replace: 9050,
            ..StatusEffect::default()
        });
        b.insert(StatusEffect {
            id: 9050,
            duration: 39.9,
            interval: 1.0,
            change_hp_rate: 0.25,
            change_hp_point: 12,
            disable_poison: true,
            ..StatusEffect::default()
        });
        b
    }

    #[test]
    fn poison_builds_up_triggers_and_ticks() {
        let b = book();
        let mut s = Status::new(200.0, 5.0);
        let mut hits = 0;
        loop {
            hits += 1;
            let a = s.apply(&b, &[9019]);
            if a.poisoned {
                assert_eq!(a.timed, vec![(9050, 39.9)]);
                break;
            }
            assert!(hits < 10);
        }
        // 200 / 30 rounds up to 7 hits.
        assert_eq!(hits, 7);
        assert!(s.has(9050));
        // Poisoned: no more build-up.
        s.apply(&b, &[9019]);
        assert_eq!(s.poison.value, 0.0);
        // 320 max HP: 0.8 + 12 = 12 per second.
        let mut taken = 0;
        for _ in 0..60 {
            taken += s.tick(&b, 320, 1.0 / 60.0);
        }
        assert_eq!(taken, 12);
        let mut rest = 0;
        for _ in 0..(60 * 40) {
            rest += s.tick(&b, 320, 1.0 / 60.0);
        }
        // 39 activations in 39.9 s.
        assert_eq!(taken + rest, 39 * 12);
        assert!(!s.has(9050));
    }

    #[test]
    fn meter_recovers_between_hits() {
        let b = book();
        let mut s = Status::new(200.0, 5.0);
        s.apply(&b, &[9019]);
        s.tick(&b, 320, 2.0);
        assert!((s.poison.value - 20.0).abs() < 1e-4);
        s.tick(&b, 320, 10.0);
        assert_eq!(s.poison.value, 0.0);
    }
}
