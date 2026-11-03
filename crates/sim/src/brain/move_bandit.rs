//! One contextual choice at the ordinary plan slot; Lua owns reaction slots.
use super::{
    bandit::BanditMemory,
    observation::Observation,
    plan::Weighted,
    sampler::{Pick, Rng},
    tally::Exchange,
};
use serde::{Deserialize, Serialize};

pub const MOVE_EVERY: u32 = 2;
pub const REVIEW_EVERY: u32 = 5;

/// Choice-time work stays bounded by this context's arsenal, not lifetime memory size.
pub fn in_context(memory: &BanditMemory, phase: &str, context: &str) -> BanditMemory {
    memory
        .get(phase)
        .and_then(|contexts| contexts.get(context))
        .map(|arms| {
            std::collections::BTreeMap::from([(
                phase.to_string(),
                std::collections::BTreeMap::from([(context.to_string(), arms.clone())]),
            )])
        })
        .unwrap_or_default()
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Explore {
    Low,
    #[default]
    Normal,
    High,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Control {
    pub plan: String,
    pub confidence: f32,
    pub lock: bool,
}

pub struct Settings<'a> {
    pub controls: &'a [Control],
    pub explore: Explore,
    pub every: u32,
}

pub fn context(obs: &Observation, last: &str) -> String {
    let bucket = |p: f32| if p >= 0.75 { "high" } else { "low" };
    let distance = if obs.dist < 3.0 {
        "near"
    } else if obs.dist < 7.0 {
        "mid"
    } else {
        "far"
    };
    let wolf = if obs.wolf.deathblow_open || obs.wolf.posture >= 1.0 {
        "broken"
    } else {
        bucket(obs.wolf.posture)
    };
    format!("{distance}/b{}/w{wolf}/{last}", bucket(obs.boss.posture))
}

pub fn family(arm: &str) -> String {
    arm.split(['>', '+'])
        .find_map(|s| s.parse::<i32>().ok())
        .map_or_else(|| "none".into(), |id| (id / 10).to_string())
}

#[derive(Debug, Clone, Default)]
pub struct Fighter {
    pub evidence: BanditMemory,
    pub provisional: BanditMemory,
    pub display: Option<String>,
    held: Option<(String, String, String, u32)>,
    pub last: String,
    pub runs: Vec<(f32, String, String)>,
    pub progress: std::collections::BTreeMap<String, [u32; 2]>,
    pub contacts: Vec<(f32, String)>,
    pub damage: Vec<(f32, String, f32)>,
    pub openings: Vec<f32>,
}

impl Fighter {
    pub fn choose(
        &mut self,
        phase: &str,
        context: &str,
        set: &[Weighted],
        settings: Settings<'_>,
        rng: &mut Rng,
    ) -> Pick {
        let Settings {
            controls,
            explore,
            every,
        } = settings;
        if set.is_empty() {
            return Pick::Nothing;
        }
        let plans: Vec<_> = (0..set.len()).filter(|&i| set[i].plan.is_some()).collect();
        if plans.is_empty() {
            return Pick::Lua;
        }
        let lua_weight: f32 = set
            .iter()
            .filter(|p| p.plan.is_none())
            .map(|p| p.weight)
            .sum();
        let total_weight: f32 = set.iter().map(|p| p.weight).sum();
        let lua_share = (lua_weight / total_weight.max(1e-6)).clamp(0.0, 0.25);
        let key = |i: usize| {
            set[i]
                .plan
                .as_ref()
                .map_or_else(|| "lua".into(), |p| p.value())
        };
        let attacks = |i: usize| {
            set[i].plan.as_ref().map_or(1, |p| {
                p.actions()
                    .iter()
                    .filter(|id| (3000..3100).contains(*id))
                    .count()
                    .max(1) as u32
            })
        };
        let mut held = None;
        if let Some((held_phase, ctx, arm, left)) = &mut self.held
            && held_phase == phase
            && ctx.rsplit_once('/').map(|c| c.0) == context.rsplit_once('/').map(|c| c.0)
            && let Some(i) = (0..set.len()).find(|&i| key(i) == *arm && attacks(i) <= *left)
        {
            *left -= attacks(i);
            held = Some(i);
        }
        let evidence = |arm: &str| {
            let get = |m: &BanditMemory| {
                m.get(phase)
                    .and_then(|p| p.get(context))
                    .and_then(|a| a.get(arm))
                    .copied()
                    .unwrap_or_default()
            };
            let mut e = get(&self.evidence);
            let now = get(&self.provisional);
            e.success += now.success;
            e.failure += now.failure;
            e
        };
        let max_weight = plans
            .iter()
            .map(|&i| set[i].weight)
            .fold(1e-6_f32, f32::max);
        let priors: Vec<_> = (0..set.len())
            .map(|i| {
                let control = controls.iter().find(|c| c.plan == key(i));
                let confidence = control.map_or(0.0, |c| c.confidence);
                (0.25 + 0.35 * set[i].weight / max_weight + 0.35 * confidence).clamp(0.05, 0.95)
            })
            .collect();
        let means: Vec<_> = (0..set.len())
            .map(|i| evidence(&key(i)).mean(priors[i]))
            .collect();
        let best = plans.iter().map(|&i| means[i]).fold(0.0_f32, f32::max);
        let confidence = controls
            .iter()
            .filter(|c| (0..set.len()).any(|i| key(i) == c.plan))
            .map(|c| c.confidence)
            .fold(0.0_f32, f32::max);
        let floor = (match explore {
            Explore::Low => 0.01,
            Explore::Normal => 0.025,
            Explore::High => 0.08,
        } * (1.0 - 0.75 * confidence))
            .max(0.005);
        let delegated = if held.is_none() && lua_share > 0.0 && rng.next_f32() < lua_share {
            set.iter().position(|p| p.plan.is_none())
        } else {
            None
        };
        let probe = held.is_none() && delegated.is_none() && rng.next_f32() < floor;
        let i = if probe {
            plans[(rng.next_f32() * plans.len() as f32) as usize]
        } else {
            0
        };
        // Stable independent Thompson draws, then argmax.
        let i = if let Some(i) = held.or(delegated) {
            i
        } else if probe {
            i
        } else {
            plans
                .into_iter()
                .map(|i| {
                    let e = evidence(&key(i));
                    let draw = e.sample(priors[i], rng);
                    let score = if e.success + e.failure < 3.0 {
                        draw + 0.4
                    } else {
                        draw * (draw / best.max(0.01)).min(1.0).powi(3)
                    };
                    (i, score)
                })
                .max_by(|a, b| a.1.total_cmp(&b.1))
                .map_or(0, |x| x.0)
        };
        let arm = key(i);
        self.display = Some(format!(
            "{arm} | {context} | mean {:.2} failure {:.2}",
            means[i],
            1.0 - means[i]
        ));
        if set[i].plan.is_none() {
            self.display = Some(format!(
                "lua | {context} | coach fallback {:.0}%",
                100.0 * lua_share
            ));
        }
        let locked = controls.iter().any(|c| c.plan == arm && c.lock);
        if held.is_none() {
            self.held = Some((
                phase.into(),
                context.into(),
                arm.clone(),
                (if locked { 3 } else { every.clamp(1, 3) }).saturating_sub(attacks(i)),
            ));
        }
        self.last = family(&arm);
        if set[i].plan.is_some() {
            Pick::Plan(i)
        } else {
            Pick::Lua
        }
    }

    pub fn settle(&mut self, x: &Exchange) {
        let Some((run_start, context, arm)) = self
            .runs
            .iter()
            .rev()
            .find(|(t, _, _)| *t <= x.start + 1e-4)
        else {
            return;
        };
        if !x.outcome.idle_guard && !arm.split(['>', '+']).any(|s| s == x.key) {
            return;
        }
        let attack = x
            .key
            .parse::<i32>()
            .is_ok_and(|id| (3000..3100).contains(&id));
        if !attack && !x.outcome.punished && !x.outcome.idle_guard {
            return;
        }
        let (success, failure) = super::adaptation::signal(x);
        let contact = self
            .contacts
            .iter()
            .find(|(t, key)| key == &x.key && *t >= x.start && *t <= x.end.unwrap_or(f32::MAX))
            .map(|c| c.0);
        let duration = contact.map_or(x.end.unwrap_or(x.start + 2.5) - x.start, |t| t - x.start);
        let broken = context.contains("wbroken/");
        let opening = self
            .openings
            .iter()
            .rev()
            .find(|t| **t <= x.start + 1e-4)
            .copied()
            .unwrap_or(x.start);
        let fast_hit = broken && x.outcome.hit && contact.is_some_and(|t| t - opening <= 2.5);
        let bars: f32 = self
            .damage
            .iter()
            .filter(|(t, key, _)| key == &x.key && *t >= x.start && *t <= x.end.unwrap_or(f32::MAX))
            .map(|(_, _, bars)| *bars)
            .sum();
        let rate = bars * 60.0 / (x.end.unwrap_or(x.start + 2.5) - run_start).max(0.25);
        let gain = success
            + 0.5 * rate / (1.0 + rate)
            + if x.outcome.broke { 2.0 } else { 0.0 }
            + if fast_hit {
                2.0 / duration.max(0.25)
            } else {
                0.0
            };
        let loss = failure + if broken && !fast_hit { 0.5 } else { 0.0 };
        let reward = gain / (gain + loss + 0.25);
        self.provisional
            .entry(x.phase.clone())
            .or_default()
            .entry(context.clone())
            .or_default()
            .entry(arm.clone())
            .or_default()
            .record(reward);
    }

    pub fn snapshot(&self, stalled: bool) -> BanditMemory {
        let mut evidence = self.provisional.clone();
        if stalled {
            for e in evidence
                .values_mut()
                .flat_map(|p| p.values_mut())
                .flat_map(|a| a.values_mut())
            {
                e.failure += e.success;
                e.success = 0.0;
            }
        }
        evidence
    }

    pub fn finish(&mut self, memory: &mut BanditMemory, stalled: bool) {
        super::bandit::decay(memory);
        super::bandit::merge(memory, &self.snapshot(stalled), 1.0);
    }

    pub fn reset(&mut self) {
        self.provisional.clear();
        self.progress.clear();
        self.runs.clear();
        self.contacts.clear();
        self.damage.clear();
        self.openings.clear();
        self.held = None;
        self.last.clear();
        self.display = None;
    }
}
