//! Small, deterministic Thompson sampler. Evidence excludes the coach's pseudo-counts.
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{coach::GamePlan, metrics::FightMetrics, sampler::Rng};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CheckpointChooser {
    #[default]
    Local,
    Decisions,
    Off,
}

impl CheckpointChooser {
    pub fn by_name(name: &str) -> Option<Self> {
        match name {
            "local" => Some(Self::Local),
            "decisions" => Some(Self::Decisions),
            "off" => Some(Self::Off),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Evidence {
    pub success: f32,
    pub failure: f32,
}

/// Phase -> checkpoint -> set -> discounted successes/failures.
pub type BanditMemory = BTreeMap<String, BTreeMap<String, BTreeMap<String, Evidence>>>;

impl Evidence {
    pub fn record(&mut self, reward: f32) {
        let reward = reward.clamp(0.0, 1.0);
        self.success += reward;
        self.failure += 1.0 - reward;
    }

    pub fn mean(self, prior: f32) -> f32 {
        let (a, b) = self.parameters(prior);
        a / (a + b)
    }

    pub fn sample(self, prior: f32, rng: &mut Rng) -> f32 {
        let (a, b) = self.parameters(prior);
        let a = gamma(a, rng);
        let b = gamma(b, rng);
        a / (a + b)
    }

    fn parameters(self, prior: f32) -> (f32, f32) {
        let count = |n: f32| {
            if n.is_finite() {
                n.clamp(0.0, 100_000.0)
            } else {
                0.0
            }
        };
        let prior = if prior.is_finite() {
            prior.clamp(0.05, 0.95)
        } else {
            0.5
        };
        (
            count(self.success) + 4.0 * prior,
            count(self.failure) + 4.0 * (1.0 - prior),
        )
    }
}

fn gamma(shape: f32, rng: &mut Rng) -> f32 {
    if shape < 1.0 {
        return gamma(shape + 1.0, rng) * rng.next_f32().max(1e-7).powf(1.0 / shape);
    }
    let d = shape - 1.0 / 3.0;
    let c = (9.0 * d).sqrt().recip();
    loop {
        let x = (-2.0 * rng.next_f32().max(1e-7).ln()).sqrt()
            * (std::f32::consts::TAU * rng.next_f32()).cos();
        let v = 1.0 + c * x;
        if v <= 0.0 {
            continue;
        }
        let v = v * v * v;
        let u = rng.next_f32().max(1e-7);
        if u < 1.0 - 0.0331 * x.powi(4) || u.ln() < 0.5 * x * x + d * (1.0 - v + v.ln()) {
            return d * v;
        }
    }
}

pub fn decay(memory: &mut BanditMemory) {
    for arms in memory
        .values_mut()
        .flat_map(|contexts| contexts.values_mut())
    {
        for arm in arms.values_mut() {
            arm.success *= 0.98;
            arm.failure *= 0.98;
        }
    }
}

pub fn merge(into: &mut BanditMemory, from: &BanditMemory, weight: f32) {
    for (phase, contexts) in from {
        for (context, arms) in contexts {
            for (name, arm) in arms {
                let entry = into
                    .entry(phase.clone())
                    .or_default()
                    .entry(context.clone())
                    .or_default()
                    .entry(name.clone())
                    .or_default();
                entry.success += arm.success * weight;
                entry.failure += arm.failure * weight;
            }
        }
    }
}

/// Shared experience informs a new profile without overwhelming its adaptation.
pub fn transfer(into: &mut BanditMemory, from: &BanditMemory) {
    let mut borrowed = from.clone();
    for arm in borrowed
        .values_mut()
        .flat_map(|contexts| contexts.values_mut())
        .flat_map(|arms| arms.values_mut())
    {
        let total = arm.success + arm.failure;
        let weight = (4.0 / total.max(4.0)).min(0.25);
        arm.success *= weight;
        arm.failure *= weight;
    }
    merge(into, &borrowed, 1.0);
}

pub fn choose(
    memory: &BanditMemory,
    phase: &str,
    context: &str,
    plan: &GamePlan,
    rng: &mut Rng,
) -> Option<(String, f32)> {
    choose_with_quality(memory, phase, context, plan, &BTreeMap::new(), rng)
}

pub fn choose_with_quality(
    memory: &BanditMemory,
    phase: &str,
    context: &str,
    plan: &GamePlan,
    quality: &BTreeMap<String, f32>,
    rng: &mut Rng,
) -> Option<(String, f32)> {
    plan.offers()
        .into_iter()
        .map(|offer| {
            let prior = plan
                .checkpoint_prefs
                .get(context)
                .and_then(|p| p.get(&offer.name))
                .copied()
                .unwrap_or_else(|| {
                    if plan
                        .sets
                        .iter()
                        .any(|s| s.name == offer.name && s.when.as_deref() == Some(context))
                    {
                        0.8
                    } else {
                        0.5
                    }
                })
                .clamp(0.05, 0.95);
            let evidence = memory
                .get(phase)
                .and_then(|p| p.get(context))
                .and_then(|p| p.get(&offer.name))
                .copied()
                .unwrap_or_default();
            (
                offer.name.clone(),
                evidence.mean(prior),
                evidence.sample(prior, rng) * quality.get(&offer.name).copied().unwrap_or(1.0),
                prior,
            )
        })
        .max_by(|a, b| a.2.total_cmp(&b.2).then_with(|| a.3.total_cmp(&b.3)))
        .map(|(name, mean, _, _)| (name, mean))
}

/// No reward for mere survival or passive time. All damage is fractions of a full bar.
pub fn reward(before: FightMetrics, after: FightMetrics, deflected_all: u32) -> f32 {
    let delta = |a: f32, b: f32| (a - b).max(0.0);
    let positive = delta(after.posture_gained, before.posture_gained)
        + 2.0 * delta(after.hp_lost, before.hp_lost)
        + 0.5
            * after
                .wolf_posture_breaks
                .saturating_sub(before.wolf_posture_breaks) as f32;
    let negative = delta(after.boss_hp_lost, before.boss_hp_lost)
        + delta(after.boss_posture_gained, before.boss_posture_gained)
        + 0.25 * after.countered.saturating_sub(before.countered) as f32
        + 0.2 * deflected_all as f32;
    let active =
        delta(after.duration, before.duration) - delta(after.passive_secs, before.passive_secs);
    if active <= 0.0 {
        0.0
    } else {
        (positive - negative).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pooled_evidence_cannot_overwhelm_the_selected_player() {
        let mut shared = BanditMemory::new();
        shared
            .entry("c5060".into())
            .or_default()
            .entry("start".into())
            .or_default()
            .insert(
                "press".into(),
                Evidence {
                    success: 1000.0,
                    failure: 0.0,
                },
            );
        let mut personal = BanditMemory::new();
        personal
            .entry("c5060".into())
            .or_default()
            .entry("start".into())
            .or_default()
            .insert(
                "press".into(),
                Evidence {
                    success: 0.0,
                    failure: 20.0,
                },
            );
        transfer(&mut personal, &shared);
        let arm = personal["c5060"]["start"]["press"];
        assert_eq!(arm.success, 4.0);
        assert!(arm.mean(0.5) < 0.25);
        assert_eq!(shared["c5060"]["start"]["press"].success, 1000.0);
    }

    #[test]
    fn reward_is_bounded_and_passivity_does_not_pay() {
        let before = FightMetrics::default();
        let after = FightMetrics {
            duration: 4.0,
            posture_gained: 0.2,
            hp_lost: 0.1,
            wolf_posture_breaks: 1,
            boss_hp_lost: 0.1,
            boss_posture_gained: 0.05,
            countered: 1,
            ..Default::default()
        };
        assert!((reward(before, after, 1) - 0.3).abs() < 1e-6);
        assert_eq!(
            reward(
                before,
                FightMetrics {
                    passive_secs: 4.0,
                    ..after
                },
                0
            ),
            0.0
        );
        assert_eq!(
            reward(
                before,
                FightMetrics {
                    hp_lost: 2.0,
                    ..after
                },
                0
            ),
            1.0
        );
        assert_eq!(
            reward(
                before,
                FightMetrics {
                    duration: 4.0,
                    ..Default::default()
                },
                0
            ),
            0.0
        );
    }

    #[test]
    fn prefs_validate_both_wire_forms_and_only_offer_surviving_arms() {
        let plan = GamePlan::from_json(r#"{"sets":[{"name":"press","plans":[{"plan":"3000","weight":1}]},{"name":"bait","plans":[{"plan":"3008","weight":1}]}],"checkpoint_prefs":{"start":{"press":0.8,"missing":0.7,"bait":-1},"invalid":{"press":0.5}}}"#).unwrap().validate(&super::super::SOLDIER);
        assert_eq!(plan.plan.checkpoint_prefs["start"].len(), 1);
        assert_eq!(
            plan.warnings
                .iter()
                .filter(|w| w.contains("preference dropped"))
                .count(),
            3
        );
        let rows = GamePlan::from_json(
            r#"{"checkpoint_prefs":[{"checkpoint":"start","set":"press","weight":0.8}]}"#,
        )
        .unwrap();
        assert_eq!(rows.checkpoint_prefs, plan.plan.checkpoint_prefs);
        let mut rng = Rng::new(8);
        let choice = choose(&BanditMemory::new(), "c1010", "start", &plan.plan, &mut rng).unwrap();
        assert!(
            plan.plan
                .offers()
                .iter()
                .any(|offer| offer.name == choice.0)
        );
    }

    #[test]
    fn sampling_learns_and_explores() {
        let mut rng = Rng::new(17);
        let mut arms = [Evidence::default(); 2];
        let mut picks = [0; 2];
        for _ in 0..1000 {
            let i = usize::from(arms[1].sample(0.5, &mut rng) > arms[0].sample(0.5, &mut rng));
            picks[i] += 1;
            arms[i].record(if rng.next_f32() < [0.2, 0.8][i] {
                1.0
            } else {
                0.0
            });
        }
        assert!(picks[0] > 0 && picks[1] > 900, "{picks:?}");
    }

    #[test]
    fn priors_shift_early_choices() {
        let mut rng = Rng::new(7);
        let preferred = (0..1000)
            .filter(|_| {
                Evidence::default().sample(0.8, &mut rng)
                    > Evidence::default().sample(0.2, &mut rng)
            })
            .count();
        assert!(preferred > 900);
    }

    #[test]
    fn memory_defaults_round_trips_and_decays() {
        let mut memory = BanditMemory::new();
        memory
            .entry("c5060".into())
            .or_default()
            .entry("start".into())
            .or_default()
            .insert(
                "press".into(),
                Evidence {
                    success: 10.0,
                    failure: 5.0,
                },
            );
        decay(&mut memory);
        assert_eq!(memory["c5060"]["start"]["press"].success, 9.8);
        let text = serde_json::to_string(&memory).unwrap();
        assert_eq!(serde_json::from_str::<BanditMemory>(&text).unwrap(), memory);
        let old = r#"{"version":2,"chr":"c5060"}"#;
        assert!(
            serde_json::from_str::<super::super::memory::Memory>(old)
                .unwrap()
                .bandit
                .is_empty()
        );
    }

    #[test]
    fn player_layers_preserve_evidence_without_duplicate_saves() {
        use super::super::memory::{BossKnowledge, LayerPaths, MEMORY_VERSION};
        let dir = std::env::temp_dir().join(format!(
            "sekiro-bandit-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let paths = LayerPaths::new(&dir.join("c5060.json"), "alice");
        std::fs::write(&paths.legacy, r#"{"version":2,"chr":"c5060","fights":3}"#).unwrap();
        let (mut boss, mut player) = paths.load("c5060").unwrap();
        assert_eq!(player.version, MEMORY_VERSION);
        assert!(player.bandit.is_empty());
        player
            .bandit
            .entry("c5060".into())
            .or_default()
            .entry("wolf_posture_75".into())
            .or_default()
            .entry("finisher".into())
            .or_default()
            .record(0.8);
        boss.update("alice", &player);
        let saved = paths.save(&boss, &player).unwrap();
        paths.save(&saved, &player).unwrap();
        let (back, profile) = paths.load("c5060").unwrap();
        assert_eq!(profile.bandit, player.bandit);
        assert_eq!(back.evidence.bandit, player.bandit);
        assert_eq!(back.players["alice"].bandit, player.bandit);
        let bob = LayerPaths::new(&paths.legacy, "bob");
        let (_, bob) = bob.load("c5060").unwrap();
        assert!(bob.bandit.is_empty());
        let old: BossKnowledge = serde_json::from_str(r#"{"version":1,"chr":"c5060","evidence":{},"players":{},"principles":[],"bans":[],"best":null}"#).unwrap();
        assert!(old.evidence.bandit.is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
