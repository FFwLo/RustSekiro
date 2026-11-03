//! Deterministic lifetime arsenal rows and ten-fight manager review.
use super::{actions::BossActions, memory::Memory, metrics::FightRecord, tally::CountMap};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Digest {
    pub moves: String,
    /// [kill seconds (-1 on loss), break conversion, damage/min, first deathblow (-1 unknown), passive share].
    pub fights: Vec<[f32; 5]>,
    pub trends: [Option<f32>; 5],
    pub best: Vec<String>,
    pub worst: Vec<String>,
    pub lessons: Vec<String>,
    pub hypotheses: Vec<Hypothesis>,
    pub review: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Hypothesis {
    pub fight: u32,
    pub idea: String,
    pub plans: String,
    pub outcome: [f32; 3],
}

fn completed(record: &FightRecord) -> bool {
    !record.metrics.void && matches!(record.winner.as_str(), "boss" | "wolf")
}

fn kill_time(record: &FightRecord) -> Option<f32> {
    record.metrics.time_to_kill_wolf.or_else(|| {
        (record.winner == "boss" && record.metrics.duration > 0.0)
            .then_some(record.metrics.duration)
    })
}

pub fn stalled(history: &[FightRecord]) -> bool {
    let recent: Vec<_> = history.iter().filter(|r| completed(r)).collect();
    if recent.len() < 8 {
        return false;
    }
    let recent = &recent[recent.len() - 8..];
    let score = |rows: &[&FightRecord]| {
        rows.iter()
            .map(|r| {
                kill_time(r).map_or(0.0, |t| 1.0 + 60.0 / t.max(1.0)) + r.metrics.break_punish_rate
            })
            .sum::<f32>()
            / rows.len() as f32
    };
    score(&recent[4..]) <= score(&recent[..4])
}

pub fn build(
    actions: &BossActions,
    memory: &Memory,
    recent: &CountMap,
    progress: &BTreeMap<String, [u32; 2]>,
    every: u32,
) -> Digest {
    let rate = |n: u32, count: u32| {
        if n == 0 {
            0
        } else {
            (100.0 * count as f32 / n as f32).round() as u32
        }
    };
    let mut rows: Vec<_> = actions.actions.iter().map(|a| (a.id, a.label)).collect();
    for id in actions.counter_ids() {
        if !rows.iter().any(|a| a.0 == id) {
            rows.push((id, "counter"));
        }
    }
    let lifetime = memory
        .phase_actions
        .get(actions.chr)
        .unwrap_or(&memory.boss);
    rows.sort_by_key(|(id, _)| {
        let c = lifetime.get(&id.to_string()).copied().unwrap_or_default();
        (
            u32::from(c.n > 0),
            rate(c.n, c.hit) as i32 - rate(c.n, c.countered + c.punished) as i32,
            *id,
        )
    });
    let mut text = String::from("id label n hit dg counter whiff progress recent/life%\n");
    let mut ranks = Vec::new();
    for (id, label) in rows {
        let key = id.to_string();
        let c = lifetime.get(&key).copied().unwrap_or_default();
        let short: String = label
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .take(12)
            .collect();
        if c.n == 0 {
            text.push_str(&format!("{id} {short} unused\n"));
            continue;
        }
        let now = recent.get(&key).copied().unwrap_or_default();
        let recent_hit = if now.n == 0 {
            "-".into()
        } else {
            rate(now.n, now.hit).to_string()
        };
        let p = progress
            .get(&format!("{}:{key}", actions.chr))
            .map_or("?".into(), |p| {
                format!("{}{}", rate(p[0], p[1]), if p[0] < c.n { "*" } else { "" })
            });
        text.push_str(&format!(
            "{id} {short} {} {} {} {} {} {p} {}/{}\n",
            c.n,
            rate(c.n, c.hit),
            rate(c.n, (c.deflected + c.blocked).min(c.n)),
            rate(c.n, c.counter_kinds[0] + c.counter_kinds[1]),
            rate(c.n, c.whiffed),
            recent_hit,
            rate(c.n, c.hit)
        ));
        if c.n >= 3 {
            ranks.push((
                key,
                rate(c.n, c.hit) as i32 - rate(c.n, c.countered + c.punished) as i32,
            ));
        }
    }
    ranks.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));
    let fights: Vec<_> = memory
        .history
        .iter()
        .rev()
        .filter(|r| completed(r))
        .take(10)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(|r| {
            let m = r.metrics;
            [
                kill_time(r).unwrap_or(if r.winner == "wolf" { -1.0 } else { -2.0 }),
                m.break_punish_rate,
                m.damage_per_min(),
                m.player_first_deathblow
                    .map_or(-1.0, |b| u8::from(b) as f32),
                m.passive_share(),
            ]
        })
        .collect();
    let trends = std::array::from_fn(|column| {
        let known: Vec<_> = fights
            .iter()
            .map(|row| row[column])
            .filter(|v| *v >= 0.0)
            .collect();
        (!known.is_empty()).then(|| known.iter().sum::<f32>() / known.len() as f32)
    });
    let completed = memory.boss_wins.saturating_add(memory.wolf_wins);
    Digest {
        moves: text,
        fights,
        trends,
        best: ranks.iter().rev().take(3).map(|r| r.0.clone()).collect(),
        worst: ranks.iter().take(3).map(|r| r.0.clone()).collect(),
        lessons: memory.coach.lessons.clone(),
        hypotheses: memory.coach.hypotheses.clone(),
        review: every > 0
            && completed > memory.coach.last_review_fight
            && (completed > 0 && completed.is_multiple_of(every) || stalled(&memory.history)),
    }
}

pub fn remember(into: &mut Vec<String>, new: &[String]) {
    for lesson in new {
        into.retain(|s| s != lesson);
        into.push(lesson.clone());
    }
    let excess = into.len().saturating_sub(6);
    into.drain(..excess);
}
