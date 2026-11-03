//! Recent attack outcomes, shared by plan sampling and Lua counter callbacks.
//! Counts remain an audit trail; this discounted evidence is the adaptation signal.
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::plan::Plan;
use super::tally::{Counts, Exchange};

pub const MIN_SAMPLES: u32 = 3;
pub const FLOOR: f32 = 0.01;
const RETENTION: f32 = 0.9;

/// Phase -> animation id -> recent success and severity-weighted failure.
pub type AttackMemory = BTreeMap<String, BTreeMap<String, AttackEvidence>>;

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AttackEvidence {
    pub n: u32,
    pub success: f32,
    pub failure: f32,
}

impl AttackEvidence {
    pub fn record(&mut self, success: f32, failure: f32) {
        self.n = self.n.saturating_add(1);
        self.success = RETENTION * self.success + success;
        self.failure = RETENTION * self.failure + failure;
    }

    /// Append a batch while preserving the same decay as sequential observations.
    pub fn append(&mut self, later: Self) {
        let retention = RETENTION.powf(later.n as f32);
        self.success = self.success * retention + later.success;
        self.failure = self.failure * retention + later.failure;
        self.n = self.n.saturating_add(later.n);
    }

    /// Legacy overlapping flags cannot reconstruct distinct failures or their order.
    /// Seed a bounded prior, preserving the raw counts separately.
    pub fn legacy(c: Counts) -> Self {
        let n = c.n as f32;
        let scale = 6.0 / n.max(6.0);
        Self {
            n: c.n,
            success: c.hit as f32 * scale,
            failure: ((c.whiffed as f32
                + c.blocked as f32
                + c.deflected as f32
                + c.countered as f32)
                .min(c.n.saturating_sub(c.hit) as f32)
                + c.countered as f32
                + c.punished as f32)
                * scale,
        }
    }

    pub fn multiplier(self) -> f32 {
        if self.n < MIN_SAMPLES {
            return 1.0;
        }
        let success = finite(self.success);
        let failure = finite(self.failure);
        (2.0 * (success + 0.5) / (success + failure + 1.0))
            .powi(3)
            .clamp(FLOOR, 2.0)
    }

    pub fn pool(&mut self, other: Self) {
        self.n = self.n.saturating_add(other.n);
        self.success += finite(other.success);
        self.failure += finite(other.failure);
    }

    /// Own outcomes dominate; pooled experience contributes at most one effective sample.
    pub fn borrow(&mut self, shared: Self) {
        let total = finite(shared.success) + finite(shared.failure);
        let scale = (1.0 / total.max(1.0)).min(0.25);
        self.success += finite(shared.success) * scale;
        self.failure += finite(shared.failure) * scale;
        if self.n == 0 {
            self.n = shared.n;
        }
    }
}

fn finite(value: f32) -> f32 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    }
}

/// Distinct observations can coexist (a guarded blow can still advance posture).
/// Counters and punishments remain failures even if another hit landed in the clip.
pub fn signal(x: &Exchange) -> (f32, f32) {
    let o = x.outcome;
    let failure = if o.idle_guard {
        0.25
    } else if o.countered.is_some() {
        2.0
    } else if o.deflected {
        1.0
    } else if o.blocked && !o.hit {
        0.6
    } else if x.whiffed() {
        1.0
    } else {
        0.0
    } + if o.punished { 1.5 } else { 0.0 };
    let success = if o.hit { 1.0 } else { 0.0 }
        + if o.progress { 0.25 } else { 0.0 }
        + if o.broke { 1.0 } else { 0.0 };
    (success, failure)
}

pub fn record(memory: &mut AttackMemory, x: &Exchange) {
    if !x
        .key
        .parse::<i32>()
        .is_ok_and(|id| (3000..3100).contains(&id))
    {
        return;
    }
    let (success, failure) = signal(x);
    memory
        .entry(x.phase.clone())
        .or_default()
        .entry(x.key.clone())
        .or_default()
        .record(success, failure);
}

pub fn append(into: &mut AttackMemory, from: &AttackMemory) {
    for (phase, actions) in from {
        for (key, evidence) in actions {
            into.entry(phase.clone())
                .or_default()
                .entry(key.clone())
                .or_default()
                .append(*evidence);
        }
    }
}

pub fn decay(memory: &mut AttackMemory) {
    for e in memory.values_mut().flat_map(|actions| actions.values_mut()) {
        e.success *= 0.85;
        e.failure *= 0.85;
    }
}

/// A weak component suppresses every chain containing it, without multiplying the
/// penalty repeatedly for long combos or rewarding them merely for having more steps.
pub fn plan_multiplier(plan: &Plan, factor: impl Fn(i32) -> f32) -> f32 {
    plan.actions()
        .into_iter()
        .map(factor)
        .reduce(f32::min)
        .unwrap_or(1.0)
}

/// Migrate lazily even for deserialized contribution snapshots, not just disk profiles.
pub fn seed(memory: &mut AttackMemory, phase: &str, counts: &super::tally::CountMap) {
    for (key, counts) in counts {
        if key
            .parse::<i32>()
            .is_ok_and(|id| (3000..3100).contains(&id))
        {
            memory
                .entry(phase.into())
                .or_default()
                .entry(key.clone())
                .or_insert_with(|| AttackEvidence::legacy(*counts));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brain::tally::{CounterKind, Outcome, Side};

    #[test]
    fn always_mikiri_player_adapts_in_a_handful_of_fights_but_retains_exploration() {
        use crate::brain::{
            GENICHIRO,
            coach::{GamePlan, PlanSet, WeightedChoice},
            sampler::{self, Context, Pick, Rng, Streaks},
        };
        use std::collections::HashSet;
        let plan = GamePlan {
            sets: vec![
                PlanSet {
                    name: "thrust".into(),
                    plans: vec![
                        WeightedChoice {
                            plan: "3025".into(),
                            weight: 9.0,
                        },
                        WeightedChoice {
                            plan: "3000".into(),
                            weight: 1.0,
                        },
                    ],
                    ..Default::default()
                },
                PlanSet {
                    name: "slash".into(),
                    plans: vec![
                        WeightedChoice {
                            plan: "3025".into(),
                            weight: 1.0,
                        },
                        WeightedChoice {
                            plan: "3000".into(),
                            weight: 9.0,
                        },
                    ],
                    ..Default::default()
                },
            ],
            start_set: "thrust".into(),
            ..Default::default()
        };
        // Legacy: counters were neither contacts nor repelled streaks. Streak-only:
        // today's sampler without persistent attack evidence. Learned sampler and local:
        // same scripted Wolf and the exact production signal, weights and chooser.
        let mut counts = [[0u32; 6]; 4];
        for (mode, histogram) in counts.iter_mut().enumerate() {
            for seed in 1..=64 {
                let mut rng = Rng::new(seed);
                let mut memory = AttackMemory::new();
                let mut bandit = crate::brain::bandit::BanditMemory::new();
                for failed_uses in histogram.iter_mut() {
                    decay(&mut memory);
                    let mut streaks = Streaks::default();
                    let mut tried = HashSet::new();
                    let mut set = 0;
                    for slot in 0..30 {
                        let factor = |id: i32| {
                            memory
                                .get("c7100")
                                .and_then(|m| m.get(&id.to_string()))
                                .copied()
                                .unwrap_or_default()
                                .multiplier()
                        };
                        if mode == 3 && slot % 10 == 0 {
                            let quality = plan
                                .sets
                                .iter()
                                .enumerate()
                                .map(|(i, set)| {
                                    let weighted = plan.weighted(i, &GENICHIRO);
                                    let q: f32 = weighted
                                        .iter()
                                        .map(|w| {
                                            w.weight
                                                * plan_multiplier(w.plan.as_ref().unwrap(), factor)
                                        })
                                        .sum();
                                    (set.name.clone(), q / 10.0)
                                })
                                .collect();
                            let (name, _) = crate::brain::bandit::choose_with_quality(
                                &bandit, "c7100", "start", &plan, &quality, &mut rng,
                            )
                            .unwrap();
                            set = plan.set_index(&name).unwrap();
                            // Deliberately no useful window reward: every arm gets zero.
                            // The per-attack signal must solve the repetition independently.
                            bandit
                                .entry("c7100".into())
                                .or_default()
                                .entry("start".into())
                                .or_default()
                                .entry(name)
                                .or_default()
                                .record(0.0);
                        }
                        let mut choices = plan.weighted(set, &GENICHIRO);
                        if mode == 1 {
                            // Replay the previous sampler's uncapped streak penalty exactly.
                            for w in &mut choices {
                                let old = sampler::PENALTY
                                    .powi(streaks.worst(w.plan.as_ref().unwrap()).min(30) as i32);
                                w.weight *= old / old.max(FLOOR);
                            }
                        }
                        if mode >= 2 {
                            for w in &mut choices {
                                w.weight *= plan_multiplier(w.plan.as_ref().unwrap(), factor);
                            }
                        }
                        let ctx = Context {
                            streaks: &streaks,
                            tried: &tried,
                            temperature: 1.0,
                        };
                        let Pick::Plan(index) = sampler::pick(&choices, &ctx, &mut rng) else {
                            panic!("two attack choices")
                        };
                        let chosen = choices[index].plan.as_ref().unwrap();
                        tried.insert(chosen.value());
                        let countered = index == 0;
                        *failed_uses += u32::from(countered);
                        let x = Exchange {
                            phase: "c7100".into(),
                            side: Side::Boss,
                            key: if countered { "3025" } else { "3000" }.into(),
                            start: slot as f32,
                            end: Some(slot as f32 + 0.5),
                            outcome: Outcome {
                                countered: countered.then_some(CounterKind::Mikiri),
                                hit: !countered,
                                progress: !countered,
                                ..Default::default()
                            },
                        };
                        if mode > 0 {
                            streaks.record(&x.key, countered);
                        }
                        if mode >= 2 {
                            record(&mut memory, &x);
                        }
                    }
                }
            }
        }
        eprintln!(
            "always-Mikiri, 64 seeds x 30 slots/fight; legacy/streak-only/learned/local countered uses: {counts:?}"
        );
        assert!(counts[0][5] > 1500, "legacy never learns");
        for histogram in &counts[2..] {
            assert!(histogram[1] < 96, "below 5% by fight two: {histogram:?}");
            assert!(
                histogram[1..].iter().sum::<u32>() > 0,
                "countered move remains possible"
            );
        }
    }

    #[test]
    fn legacy_profiles_migrate_without_inventing_counters_or_changing_audit_counts() {
        use crate::brain::memory::{Loaded, MEMORY_VERSION, Memory};
        let root = std::env::temp_dir().join(format!(
            "sekiro-attack-migration-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("player.json");
        for version in 1..=3 {
            std::fs::write(
                &path,
                format!(
                    r#"{{"version":{version},"chr":"c7100","boss":{{"3025":[10,1,0,0,9,0]}}}}"#
                ),
            )
            .unwrap();
            let (memory, loaded) = Memory::load(&path, "c7100");
            assert_eq!(loaded, Loaded::Read);
            assert_eq!(memory.version, MEMORY_VERSION);
            assert_eq!(memory.boss["3025"].n, 10);
            assert_eq!(memory.boss["3025"].countered, 0);
            assert!(memory.attack_learning["c7100"]["3025"].multiplier() < 0.1);
            memory.save(&path).unwrap();
            assert_eq!(Memory::load(&path, "c7100").0, memory);
            let stem = root.join(format!("layers-{version}.json"));
            let original = format!(
                r#"{{"version":{version},"chr":"c7100","boss":{{"3025":[10,1,0,0,9,0]}}}}"#
            );
            std::fs::write(&stem, &original).unwrap();
            let layers = crate::brain::memory::LayerPaths::new(&stem, "default");
            let (boss, profile) = layers.load("c7100").unwrap();
            assert_eq!(profile.attack_learning["c7100"]["3025"].n, 10);
            assert_eq!(
                boss.players["default"].attack_learning,
                profile.attack_learning
            );
            layers.save(&boss, &profile).unwrap();
            assert_eq!(layers.load("c7100").unwrap().1, profile);
            assert_eq!(std::fs::read_to_string(&stem).unwrap(), original);
            for file in [&layers.legacy, &layers.backup, &layers.boss, &layers.player] {
                std::fs::remove_file(file).unwrap();
            }
        }
        std::fs::remove_file(&path).unwrap();
        std::fs::remove_dir(&root).unwrap();
    }

    #[test]
    fn every_bad_outcome_teaches_failure_and_progress_teaches_success() {
        let cases = [
            (Outcome::default(), 0.0, 1.0),
            (
                Outcome {
                    countered: Some(CounterKind::Mikiri),
                    ..Default::default()
                },
                0.0,
                2.0,
            ),
            (
                Outcome {
                    countered: Some(CounterKind::Jump),
                    ..Default::default()
                },
                0.0,
                2.0,
            ),
            (
                Outcome {
                    countered: Some(CounterKind::Other),
                    ..Default::default()
                },
                0.0,
                2.0,
            ),
            (
                Outcome {
                    deflected: true,
                    ..Default::default()
                },
                0.0,
                1.0,
            ),
            (
                Outcome {
                    blocked: true,
                    ..Default::default()
                },
                0.0,
                0.6,
            ),
            (
                Outcome {
                    hit: true,
                    punished: true,
                    ..Default::default()
                },
                1.0,
                1.5,
            ),
            (
                Outcome {
                    hit: true,
                    progress: true,
                    broke: true,
                    ..Default::default()
                },
                2.25,
                0.0,
            ),
            (
                Outcome {
                    blocked: true,
                    progress: true,
                    ..Default::default()
                },
                0.25,
                0.6,
            ),
        ];
        for (outcome, success, failure) in cases {
            let x = Exchange {
                phase: "c7100".into(),
                side: Side::Boss,
                key: "3025".into(),
                start: 0.0,
                end: Some(1.0),
                outcome,
            };
            assert_eq!(signal(&x), (success, failure), "{outcome:?}");
        }
    }

    #[test]
    fn own_recent_counters_override_shared_success_and_recover_after_player_changes() {
        let mut own = AttackEvidence::default();
        own.record(0.0, 2.0);
        own.record(0.0, 2.0);
        assert_eq!(own.multiplier(), 1.0);
        own.record(0.0, 2.0);
        let mut combined = own;
        combined.borrow(AttackEvidence {
            n: 10000,
            success: 10000.0,
            failure: 0.0,
        });
        assert!(combined.multiplier() < 0.1);
        assert!(combined.multiplier() >= FLOOR);
        for _ in 0..12 {
            own.record(1.0, 0.0);
        }
        assert!(own.multiplier() > 1.0);
    }

    #[test]
    fn local_chooser_obeys_structured_coach_lessons_and_attack_outcomes() {
        use crate::brain::{
            bandit,
            coach::{GamePlan, PlanSet, WeightedChoice},
            sampler::Rng,
        };
        let mut plan = GamePlan {
            notes: "Use the slash near a posture break; this player counters the thrust.".into(),
            sets: vec![
                PlanSet {
                    name: "thrust".into(),
                    plans: vec![WeightedChoice {
                        plan: "3025".into(),
                        weight: 1.0,
                    }],
                    ..Default::default()
                },
                PlanSet {
                    name: "slash".into(),
                    when: Some("wolf_posture_75".into()),
                    plans: vec![WeightedChoice {
                        plan: "3000".into(),
                        weight: 1.0,
                    }],
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let memory = bandit::BanditMemory::new();
        let mut rng = Rng::new(42);
        let hints = (0..1000)
            .filter(|_| {
                bandit::choose(&memory, "c7100", "wolf_posture_75", &plan, &mut rng)
                    .unwrap()
                    .0
                    == "slash"
            })
            .count();
        assert!(hints > 800, "structured when lesson: {hints}");
        // Explicit coach judgement overrides the hint, even when notes remain unchanged.
        plan.checkpoint_prefs.insert(
            "wolf_posture_75".into(),
            [("thrust".into(), 0.95), ("slash".into(), 0.05)].into(),
        );
        let explicit = (0..1000)
            .filter(|_| {
                bandit::choose(&memory, "c7100", "wolf_posture_75", &plan, &mut rng)
                    .unwrap()
                    .0
                    == "thrust"
            })
            .count();
        assert!(explicit > 950);
        // Live failures still defeat an overly optimistic coach prior, without a hard ban.
        let quality = [("thrust".into(), FLOOR), ("slash".into(), 2.0)].into();
        let live = (0..1000)
            .filter(|_| {
                bandit::choose_with_quality(
                    &memory,
                    "c7100",
                    "wolf_posture_75",
                    &plan,
                    &quality,
                    &mut rng,
                )
                .unwrap()
                .0 == "slash"
            })
            .count();
        assert!(
            live > 500,
            "recent attack failures must affect checkpoint choice: {live}"
        );
    }

    #[test]
    fn historical_jump_callback_whiffs_were_almost_ignored() {
        let counts = Counts {
            n: 119,
            hit: 1,
            blocked: 2,
            deflected: 1,
            whiffed: 115,
            ..Default::default()
        };
        let old = (1.0 - (counts.countered + counts.deflected) as f32 / counts.n as f32)
            * (0.5 + 0.5 * counts.hit as f32 / counts.n as f32);
        let learned = AttackEvidence::legacy(counts).multiplier();
        assert!(old > 0.49);
        assert_eq!(learned, FLOOR);
        eprintln!(
            "saved 3076 callback: previous multiplier={old:.4}, recent multiplier={learned:.4}"
        );
    }
}
