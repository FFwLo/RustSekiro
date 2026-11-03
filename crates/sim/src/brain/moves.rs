//! Boss knowledge derived once from the same clips, hitboxes and posture resolver as combat.
use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::character::ai_cancel;
use crate::combat::hit::HitOutcome;
use crate::duel::Duel;
use crate::tae::TaeKind;

use super::BossActions;

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Move {
    /// Data-derived direct-hit recovery windows, aligned with hits.
    #[serde(skip)]
    pub hit_stun: Vec<Option<f32>>,
    /// Per-hit contact envelopes, aligned with hits, including combo offsets.
    #[serde(skip)]
    pub contact_windows: Vec<Vec<[f32; 2]>>,
    #[serde(skip)]
    pub projectile_stun: Vec<Option<f32>>,
    /// First-window radial envelopes at 60 Hz; cached geometry, omitted from coach JSON.
    #[serde(skip)]
    pub first_window: Vec<[f32; 2]>,
    pub active: Vec<[f32; 2]>,
    /// Direct-hit [HP, posture] in the same order as hits.
    pub damage: Vec<[f32; 2]>,
    /// Projectile [launch, speed, life, radius, HP, guard posture, launch/parent travel, direct posture, deflect posture, parent launch].
    pub projectiles: Vec<[f32; 10]>,
    #[serde(skip)]
    pub chain_next: Vec<i32>,
    #[serde(skip)]
    pub cancel_into: Vec<i32>,
    pub contact: bool,
    pub first_reach: Option<f32>,
    pub chains: Vec<String>,
    pub posture_total: f32,
    pub last_hit: f32,
    /// [seconds, guard posture, deflect posture, behaviour judge].
    pub hits: Vec<[f32; 4]>,
    pub duration: f32,
    pub recovery: f32,
    /// [start, end, ChrActionFlag, stateInfo]; stateInfo 0 is unconditional.
    pub cancels: Vec<[f32; 4]>,
    pub reach: Option<f32>,
}

pub type MoveSheet = BTreeMap<String, Move>;

/// One simulation tick of safety before Wolf can recover.
pub const PUNISH_MARGIN: f32 = 1.0 / 60.0;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PunishEvidence {
    pub uses: u32,
    pub landed: u32,
    pub converted: u32,
    pub window_hp: f32,
    pub window_posture: f32,
}

pub type PunishMemory = BTreeMap<String, BTreeMap<String, BTreeMap<String, PunishEvidence>>>;

pub fn merge_punishes(into: &mut PunishMemory, from: &PunishMemory) {
    for (phase, buckets) in from {
        for (bucket, options) in buckets {
            for (id, evidence) in options {
                into.entry(phase.clone())
                    .or_default()
                    .entry(bucket.clone())
                    .or_default()
                    .entry(id.clone())
                    .or_default()
                    .merge(evidence);
            }
        }
    }
}

impl PunishEvidence {
    pub fn merge(&mut self, from: &Self) {
        self.uses += from.uses;
        self.landed += from.landed;
        self.converted += from.converted;
        self.window_hp += from.window_hp;
        self.window_posture += from.window_posture;
    }
    pub fn conversion_rate(&self) -> f32 {
        self.converted as f32 / self.uses.max(1) as f32
    }
    pub fn sample(&self, rng: &mut super::sampler::Rng) -> f32 {
        super::bandit::Evidence {
            success: self.landed as f32 + self.converted as f32,
            failure: (self.uses - self.landed.min(self.uses)) as f32
                + (self.uses - self.converted.min(self.uses)) as f32,
        }
        .sample(0.5, rng)
    }
}

/// Facts only, in stable id order. No damage score or ranking.
#[derive(Debug, Clone, PartialEq)]
pub struct Punish {
    pub id: String,
    pub name: String,
    pub first: f32,
    pub left: f32,
    pub hits: u32,
    pub window: [f32; 2],
    pub total: [f32; 2],
    /// None means the required reaction timing is unavailable.
    pub stun_lock: Option<bool>,
    pub guard_after: f32,
    pub recovery: f32,
    pub evidence: PunishEvidence,
    pub conversion_rate: f32,
}

impl Serialize for Punish {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        // The coach wire passes through Value; avoid expanded f32-to-f64 decimals.
        let n = |x: f32| {
            serde_json::from_str::<serde_json::Value>(&format!("{x:.3}"))
                .unwrap_or(serde_json::Value::Null)
        };
        serde_json::json!([
            self.id,
            self.name,
            n(self.first),
            n(self.left),
            self.hits,
            [n(self.window[0]), n(self.window[1])],
            [n(self.total[0]), n(self.total[1])],
            self.stun_lock,
            n(self.guard_after),
            n(self.recovery),
            [self.evidence.uses, self.evidence.landed,],
            n(self.conversion_rate)
        ])
        .serialize(serializer)
    }
}

pub fn bucket(distance: f32) -> &'static str {
    if distance < 3.0 {
        "near"
    } else if distance < 6.0 {
        "mid"
    } else {
        "far"
    }
}

pub fn choose_punish<'a>(
    options: &[&'a Punish],
    preferences: &[super::coach::Avoid],
    rng: &mut super::sampler::Rng,
) -> Option<&'a Punish> {
    for preference in preferences {
        if let Some(option) = options.iter().find(|p| p.id == preference.choice) {
            return Some(*option);
        }
    }
    options
        .iter()
        .map(|p| (*p, p.evidence.sample(rng)))
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(p, _)| p)
}

fn intercept(p: &[f32; 10], relative: [f32; 2], velocity: [f32; 2]) -> Option<f32> {
    let r = [
        relative[0] + velocity[0] * p[0],
        relative[1] + velocity[1] * p[0],
    ];
    let radius = p[3] + p[6];
    let a = velocity[0] * velocity[0] + velocity[1] * velocity[1] - p[1] * p[1];
    let b = 2.0 * (r[0] * velocity[0] + r[1] * velocity[1] - p[1] * radius);
    let c = r[0] * r[0] + r[1] * r[1] - radius * radius;
    let travel = if c <= 0.0 {
        0.0
    } else if a.abs() < 1e-6 {
        if b >= 0.0 {
            return None;
        } else {
            -c / b
        }
    } else {
        let d = b * b - 4.0 * a * c;
        if d < 0.0 {
            return None;
        }
        [(-b - d.sqrt()) / (2.0 * a), (-b + d.sqrt()) / (2.0 * a)]
            .into_iter()
            .filter(|t| *t >= 0.0)
            .min_by(f32::total_cmp)?
    };
    (travel <= p[2]).then_some(p[0] + travel)
}

pub fn viable_punishes(
    sheet: &MoveSheet,
    actions: &BossActions,
    relative: [f32; 2],
    velocity: [f32; 2],
    remaining: f32,
    evidence: &BTreeMap<String, PunishEvidence>,
) -> Vec<Punish> {
    sheet
        .iter()
        .filter_map(|(id, row)| {
            if actions.parse(id).is_err() {
                return None;
            }
            if !row.damaging() {
                return None;
            }
            let first = row.reachable_time(relative, velocity)?;
            if first + PUNISH_MARGIN >= remaining {
                return None;
            }
            let mut contacts: Vec<_> = row
                .hits
                .iter()
                .enumerate()
                .filter_map(|(i, h)| {
                    let time = if let Some(windows) = row.contact_windows.get(i) {
                        windows
                            .iter()
                            .find(|w| {
                                (relative[0] + velocity[0] * w[0])
                                    .hypot(relative[1] + velocity[1] * w[0])
                                    <= w[1] + 0.2
                            })
                            .map(|w| w[0])?
                    } else {
                        h[0].max(first)
                    };
                    let distance =
                        (relative[0] + velocity[0] * time).hypot(relative[1] + velocity[1] * time);
                    if row
                        .reach
                        .or(row.first_reach)
                        .is_none_or(|r| distance > r + 0.2)
                    {
                        return None;
                    }
                    Some((
                        time,
                        row.damage.get(i).copied().unwrap_or_default(),
                        h[1],
                        row.hit_stun.get(i).copied().flatten(),
                    ))
                })
                .chain(row.projectiles.iter().enumerate().filter_map(|(i, p)| {
                    Some((
                        intercept(p, relative, velocity)?,
                        [p[4], p[7]],
                        p[5],
                        row.projectile_stun.get(i).copied().flatten(),
                    ))
                }))
                .collect();
            contacts.sort_by(|a, b| a.0.total_cmp(&b.0));
            let missed_contact = contacts.len() < row.hits.len() + row.projectiles.len();
            let mut window = [0.0; 2];
            let mut total = [0.0; 2];
            for damage in &row.damage {
                for i in 0..2 {
                    total[i] += damage[i];
                }
            }
            for p in &row.projectiles {
                total[0] += p[4];
                total[1] += p[7];
            }
            let mut hits = 0;
            let mut guard_after = 0.0;
            let mut locked_until = remaining;
            let mut stun_lock = Some(contacts.len() > 1);
            let mut connected = true;
            let mut unknown = false;
            for (t, damage, guard, stun) in contacts {
                if t < remaining {
                    hits += 1;
                    for i in 0..2 {
                        window[i] += damage[i];
                    }
                } else {
                    guard_after += guard;
                }
                if t > locked_until {
                    connected = false;
                    stun_lock = if unknown { None } else { Some(false) };
                }
                if let Some(stun) = stun {
                    locked_until = t + stun;
                } else {
                    unknown = true;
                    if connected {
                        stun_lock = None;
                    }
                }
            }
            if missed_contact {
                stun_lock = Some(false);
            }
            let evidence = evidence.get(id).cloned().unwrap_or_default();
            let name = id
                .split('+')
                .map(|part| {
                    actions
                        .actions
                        .iter()
                        .find(|a| a.id.to_string() == part)
                        .map_or(part, |a| a.label)
                })
                .collect::<Vec<_>>()
                .join(" + ");
            Some(Punish {
                id: id.clone(),
                name,
                first: rounded(first),
                left: rounded(remaining - first),
                hits,
                window,
                total,
                stun_lock,
                guard_after,
                recovery: row.recovery,
                conversion_rate: evidence.conversion_rate(),
                evidence,
            })
        })
        .collect()
}

/// Earliest unconditional player escape/guard cancel, otherwise the clip's end.
pub fn recovery_window(player: &crate::Character, animation: &str) -> Option<f32> {
    let duration = player.clips.clip(animation)?.duration;
    let tae = player.tae.anim(animation)?;
    Some(tae.events.iter().filter(|e| e.state_info == 0).filter_map(|e| {
        if matches!(e.kind, TaeKind::ActionFlag(flag) if matches!(flag,
            crate::tae::cancel::MOVE | crate::tae::cancel::DODGE | crate::tae::cancel::GUARD
            | crate::tae::cancel::ATTACK | crate::tae::cancel::JUMP | crate::tae::cancel::PROSTHETIC)) {
            Some(e.start)
        } else { None }
    }).fold(duration, f32::min))
}

fn hit_recoveries(cache: &std::path::Path) -> BTreeMap<i32, f32> {
    wolf_recoveries(cache)
        .into_iter()
        .filter(|(key, _)| *key <= 11)
        .collect()
}

pub(crate) fn break_recoveries(cache: &std::path::Path) -> BTreeMap<i32, f32> {
    wolf_recoveries(cache)
        .into_iter()
        .filter(|(key, _)| *key > 11)
        .collect()
}

fn wolf_recoveries(cache: &std::path::Path) -> BTreeMap<i32, f32> {
    type RecoveryCache = std::sync::Mutex<BTreeMap<std::path::PathBuf, BTreeMap<i32, f32>>>;
    static RECOVERIES: std::sync::OnceLock<RecoveryCache> = std::sync::OnceLock::new();
    // Extracted input is immutable during a session. Probe once per data root, not per boss.
    let mut cached = RECOVERIES
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(timings) = cached.get(cache) {
        return timings.clone();
    }
    let timings = reaction_recoveries(
        cache,
        (0..=11).map(|level| (level, 99999, level)).chain(
            [1001, 1004, 1005, 1027, 1028, 1033]
                .into_iter()
                .map(|code| (code, code, 0)),
        ),
    );
    if !timings.is_empty() {
        cached.insert(cache.to_path_buf(), timings.clone());
    }
    timings
}

fn reaction_recoveries(
    cache: &std::path::Path,
    reactions: impl IntoIterator<Item = (i32, i32, i32)>,
) -> BTreeMap<i32, f32> {
    let Ok(mut player) = crate::Character::load_from_cache(cache) else {
        return BTreeMap::new();
    };
    player.settle(1.0 / 60.0, 400);
    let mut timings = BTreeMap::new();
    for (key, damage_type, level) in reactions {
        player.pending_damage = Some(crate::player::IncomingDamage {
            damage_type,
            level,
            ..Default::default()
        });
        player.tick(&crate::InputFrame::default(), 1.0 / 60.0);
        if let Some(clip) = player.behavior.runtime.main_clip()
            && let Some(window) = recovery_window(&player, &clip.animation)
        {
            timings.insert(key, window);
        }
        // Wait for HKS's actual idle transition; this bound limits probe work, not stun timing.
        player.settle(1.0 / 60.0, 600);
        if player.state() != "StandIdle" {
            // Some reactions need world state absent from this isolated probe.
            // Start the next probe from a fresh standing character instead of inheriting it.
            let Ok(fresh) = crate::Character::load_from_cache(cache) else {
                break;
            };
            player = fresh;
            player.settle(1.0 / 60.0, 400);
        }
    }
    timings
}

/// Pool only byte-identical decoded clips with identical resolved TAE event timelines.
pub fn shared_genichiro_actions(
    cache: &std::path::Path,
    old: &crate::Character,
    next: &crate::Character,
) -> BTreeSet<String> {
    let mut shared = BTreeSet::new();
    for id in (3000..3100).chain(5200..5300) {
        let name = format!("a000_{id:06}");
        let (Some(a), Some(b)) = (old.tae.anim(&name), next.tae.anim(&name)) else {
            continue;
        };
        if a.events != b.events {
            continue;
        }
        let first = std::fs::read(cache.join("anim/c7100").join(format!("{name}.bin")));
        let second = std::fs::read(cache.join("anim/c7110").join(format!("{name}.bin")));
        if let (Ok(a), Ok(b)) = (first, second)
            && a == b
        {
            shared.insert(id.to_string());
        }
    }
    shared
}

fn rounded(v: f32) -> f32 {
    (v * 1000.0).round() / 1000.0
}

fn projectile_time(p: &[f32; 10], distance: f32) -> f32 {
    p[0] + if p[1] > 0.0 {
        (distance - p[3] - p[6]).max(0.0) / p[1]
    } else {
        0.0
    }
}

fn root_at(clip: &sekiro_formats::anim::AnimClip, time: f32) -> glam::Mat4 {
    let Some(rm) = &clip.root_motion else {
        return glam::Mat4::IDENTITY;
    };
    let start = rm.sample(0.0);
    let end = rm.sample(time);
    glam::Mat4::from_translation(glam::Vec3::new(
        end[0] - start[0],
        end[1] - start[1],
        end[2] - start[2],
    )) * glam::Mat4::from_rotation_y(end[3] - start[3])
}

/// Nominal radial envelope without target-dependent TAE 760 scaling or steering.
fn clip_reach(duel: &Duel, id: i32, limit: f32, origin: glam::Mat4) -> Option<f32> {
    let npc = &duel.enemy;
    let fighter = &duel.fighters[1];
    let name = format!("a000_{id:06}");
    let clip = npc.clips.clip(&name)?;
    let tae = npc.tae.anim(&name)?;
    let mut reach: Option<f32> = None;
    for e in tae.events.iter() {
        let TaeKind::Attack { judge, .. } = e.kind else {
            continue;
        };
        if e.state_info != 0 {
            continue;
        }
        let Some(attack) = fighter.attacks.get(&judge) else {
            continue;
        };
        if crate::perilous::counter_dummy(attack) {
            continue;
        }
        let mut t = e.start;
        while t <= e.end.min(clip.duration).min(limit) {
            let model = npc.skeleton.model_space(&clip.sample(t, &npc.skeleton));
            let world = origin * root_at(&clip, t);
            for shape in &attack.shapes {
                for dummy in [shape.dummy_a, shape.dummy_b] {
                    for p in fighter.rig.positions(dummy, &model, world) {
                        let r = p.x.hypot(p.z) + shape.radius + duel.fighters[0].radius;
                        reach = Some(reach.unwrap_or(0.0).max(rounded(r)));
                    }
                }
            }
            t += 1.0 / 60.0;
        }
    }
    reach
}

fn instantaneous_reach(
    duel: &Duel,
    id: i32,
    time: f32,
    origin: glam::Mat4,
    only_judge: Option<i32>,
) -> Option<f32> {
    let npc = &duel.enemy;
    let fighter = &duel.fighters[1];
    let name = format!("a000_{id:06}");
    let clip = npc.clips.clip(&name)?;
    let tae = npc.tae.anim(&name)?;
    let model = npc.skeleton.model_space(&clip.sample(time, &npc.skeleton));
    let world = origin * root_at(&clip, time);
    let mut reach: Option<f32> = None;
    for event in tae
        .events
        .iter()
        .filter(|e| e.state_info == 0 && time >= e.start && time <= e.end)
    {
        let TaeKind::Attack { judge, .. } = event.kind else {
            continue;
        };
        if only_judge.is_some_and(|j| j != judge) {
            continue;
        }
        let Some(attack) = fighter.attacks.get(&judge) else {
            continue;
        };
        if crate::perilous::counter_dummy(attack) {
            continue;
        }
        for shape in &attack.shapes {
            for dummy in [shape.dummy_a, shape.dummy_b] {
                for p in fighter.rig.positions(dummy, &model, world) {
                    let r = p.x.hypot(p.z) + shape.radius + duel.fighters[0].radius;
                    reach = Some(reach.unwrap_or(0.0).max(rounded(r)));
                }
            }
        }
    }
    reach
}

impl Move {
    pub fn damaging(&self) -> bool {
        self.hits.iter().any(|h| h[1] > 0.0)
            || self.damage.iter().any(|d| d[0] > 0.0 || d[1] > 0.0)
            || self.projectiles.iter().any(|p| p[4] > 0.0 || p[5] > 0.0)
    }
    pub fn measurement_time(&self, distance: f32) -> Option<f32> {
        self.hits
            .first()
            .map(|h| h[0])
            .into_iter()
            .chain(self.projectiles.iter().map(|p| {
                p[0] + if p[1] > 0.0 {
                    (distance - p[3] - p[6]).max(0.0) / p[1]
                } else {
                    0.0
                }
            }))
            .min_by(f32::total_cmp)
    }
    pub fn first_hit(&self, distance: f32) -> Option<f32> {
        self.first_window
            .iter()
            .find(|w| distance <= w[1] + 0.2)
            .map(|w| w[0])
            .or_else(|| {
                self.first_window
                    .is_empty()
                    .then(|| self.hits.first().map(|h| h[0]))
                    .flatten()
            })
            .into_iter()
            .chain(
                self.projectiles
                    .iter()
                    .filter(|p| distance <= p[1] * p[2] + p[3] + p[6])
                    .map(|p| p[0] + (distance - p[3] - p[6]).max(0.0) / p[1].max(0.01)),
            )
            .min_by(f32::total_cmp)
    }

    pub fn fits(&self, relative: [f32; 2], velocity: [f32; 2]) -> bool {
        self.reachable_time(relative, velocity).is_some()
    }

    pub fn reachable_time(&self, relative: [f32; 2], velocity: [f32; 2]) -> Option<f32> {
        let predicted =
            |time: f32| (relative[0] + velocity[0] * time).hypot(relative[1] + velocity[1] * time);
        let melee = if self.first_window.is_empty() {
            self.hits.first().and_then(|h| {
                self.first_reach
                    .filter(|r| predicted(h[0]) <= r + 0.2)
                    .map(|_| h[0])
            })
        } else {
            self.first_window
                .iter()
                .find(|w| predicted(w[0]) <= w[1] + 0.2)
                .map(|w| w[0])
        };
        melee
            .into_iter()
            .chain(
                self.projectiles
                    .iter()
                    .filter_map(|p| intercept(p, relative, velocity)),
            )
            .min_by(f32::total_cmp)
    }

    /// A guarded prefix breaks posture, leaving a quick follow-up inside the conversion window.
    pub fn breaking_followup(&self, remaining: f32, distance: f32) -> Option<f32> {
        let mut hits: Vec<_> = self
            .hits
            .iter()
            .map(|h| [h[0], h[1]])
            .chain(
                self.projectiles
                    .iter()
                    .filter(|p| p[4] > 0.0 || p[5] > 0.0 || p[7] > 0.0)
                    .map(|p| {
                        [
                            p[0] + if p[1] > 0.0 {
                                (distance - p[3] - p[6]).max(0.0) / p[1]
                            } else {
                                0.0
                            },
                            p[5],
                        ]
                    }),
            )
            .collect();
        hits.sort_by(|a, b| a[0].total_cmp(&b[0]));
        let mut posture = 0.0;
        for pair in hits.windows(2) {
            posture += pair[0][1];
            if posture >= remaining && pair[1][0] - pair[0][0] <= 2.5 {
                return Some(pair[1][0] - pair[0][0]);
            }
        }
        None
    }
    /// Time to the first unconditional attack cancel or natural end, from clip time.
    pub fn wait(&self, time: f32) -> f32 {
        self.cancels
            .iter()
            .filter(|w| {
                w[3] == 0.0
                    && matches!(w[2] as i32, ai_cancel::COMBO | ai_cancel::ATTACK)
                    && w[1] > time
            })
            .map(|w| (w[0] - time).max(0.0))
            .fold((self.duration - time).max(0.0), f32::min)
    }
}

pub fn build(duel: &Duel, actions: &BossActions) -> MoveSheet {
    let hit_recoveries = hit_recoveries(&duel.cache);
    let mut sheet = MoveSheet::new();
    let npc = &duel.enemy;
    let fighter = &duel.fighters[1];
    let effects_d = duel.rules.active_effects(0, &duel.player);
    // Wolf's normal sword guard behaviour, independent of his current idle clip.
    let guard = duel.fighters[0].attacks.get(&90);
    let ids: BTreeSet<_> = actions
        .actions
        .iter()
        .map(|a| a.id)
        .chain(actions.counter_ids())
        .chain(3000..3100)
        .collect();
    for id in ids {
        let name = format!("a000_{id:06}");
        let (Some(clip), Some(tae)) = (npc.clips.clip(&name), npc.tae.anim(&name)) else {
            continue;
        };
        let mut row = Move {
            duration: rounded(clip.duration),
            ..Move::default()
        };
        for e in tae.events.iter() {
            if let TaeKind::Attack { judge, .. } = e.kind
                && fighter.attacks.get(&judge).is_some_and(|a| {
                    !crate::perilous::counter_dummy(a)
                        && a.profile.as_ref().is_some_and(|p| {
                            p.attack_power.iter().any(|v| *v > 0.0)
                                || p.direct_stam > 0
                                || p.blocked_stam > 0
                        })
                })
            {
                row.contact = true;
            }
            match e.kind {
                TaeKind::ActionFlag(flag)
                    if matches!(
                        flag,
                        ai_cancel::COMBO
                            | ai_cancel::ATTACK
                            | ai_cancel::STEP
                            | crate::tae::cancel::MOVE
                    ) =>
                {
                    row.cancels.push([
                        rounded(e.start),
                        rounded(e.end),
                        flag as f32,
                        e.state_info as f32,
                    ]);
                }
                TaeKind::Attack { judge, .. } if e.state_info == 0 => {
                    let Some(attack) = fighter.attacks.get(&judge) else {
                        continue;
                    };
                    let Some(profile) = &attack.profile else {
                        continue;
                    };
                    if crate::perilous::counter_dummy(attack) {
                        continue;
                    }
                    row.contact = true;
                    row.active.push([rounded(e.start), rounded(e.end)]);
                    let mut runtime = crate::tae::TaeRuntime::default();
                    runtime.update(
                        &npc.tae,
                        &[crate::tae::PlayingClip {
                            animation: name.clone(),
                            prev_time: e.start,
                            time: e.start,
                            looping: false,
                        }],
                        0.0,
                    );
                    let effects_a = duel.rules.effects_for(1, &runtime.frame);
                    let damage = |outcome| {
                        duel.rules.posture_for_outcome(
                            1, 0, profile, outcome, guard, &effects_a, &effects_d,
                        ) as f32
                    };
                    let hit = [
                        rounded(e.start),
                        damage(HitOutcome::Block),
                        damage(HitOutcome::Deflect),
                        judge as f32,
                    ];
                    if !row.hits.iter().any(|h| h[0] == hit[0] && h[3] == hit[3]) {
                        row.hits.push(hit);
                        row.hit_stun
                            .push(hit_recoveries.get(&attack.damage_level).copied());
                        row.damage.push([
                            duel.rules.hp_damage(
                                (1, 0),
                                profile,
                                attack,
                                HitOutcome::Hit,
                                None,
                                (&effects_a, &effects_d),
                            ) as f32,
                            damage(HitOutcome::Hit),
                        ]);
                    }
                }
                TaeKind::Bullet { judge, dummy_poly } => {
                    if let Some(bullet) = fighter.bullets.get(&judge) {
                        let model = npc
                            .skeleton
                            .model_space(&clip.sample(e.start, &npc.skeleton));
                        let origin = fighter
                            .rig
                            .positions(dummy_poly as i16, &model, root_at(&clip, e.start))
                            .first()
                            .copied()
                            .unwrap_or(glam::Vec3::Y * 1.2);
                        let mut travel = origin.x.hypot(origin.z);
                        let mut runtime = crate::tae::TaeRuntime::default();
                        runtime.update(
                            &npc.tae,
                            &[crate::tae::PlayingClip {
                                animation: name.clone(),
                                prev_time: e.start,
                                time: e.start,
                                looping: false,
                            }],
                            0.0,
                        );
                        let effects_a = duel.rules.effects_for(1, &runtime.frame);
                        let mut bullet = bullet.as_ref();
                        let mut delay = 0.0;
                        for _ in 0..4 {
                            if let Some(attack) = &bullet.attack
                                && bullet.look != crate::bullets::BulletLook::Hidden
                                && let Some(profile) = &attack.profile
                                && (crate::bullets::attack_damages(attack)
                                    || attack.damage_level > 0
                                    || profile.behavior_id != 0)
                            {
                                row.contact = true;
                                row.projectile_stun
                                    .push(hit_recoveries.get(&attack.damage_level).copied());
                                row.projectiles.push([
                                    rounded(e.start + delay),
                                    bullet.speed.abs(),
                                    bullet.life,
                                    bullet.hit_radius.max(bullet.hit_radius_max)
                                        + duel.fighters[0].radius,
                                    duel.rules.hp_damage(
                                        (1, 0),
                                        profile,
                                        attack,
                                        HitOutcome::Hit,
                                        None,
                                        (&effects_a, &effects_d),
                                    ) as f32,
                                    duel.rules.posture_for_outcome(
                                        1,
                                        0,
                                        profile,
                                        HitOutcome::Block,
                                        guard,
                                        &effects_a,
                                        &effects_d,
                                    ) as f32,
                                    rounded(travel),
                                    duel.rules.posture_for_outcome(
                                        1,
                                        0,
                                        profile,
                                        HitOutcome::Hit,
                                        guard,
                                        &effects_a,
                                        &effects_d,
                                    ) as f32,
                                    duel.rules.posture_for_outcome(
                                        1,
                                        0,
                                        profile,
                                        HitOutcome::Deflect,
                                        guard,
                                        &effects_a,
                                        &effects_d,
                                    ) as f32,
                                    rounded(e.start),
                                ]);
                            }
                            let Some(child) = &bullet.child else { break };
                            delay += bullet.life;
                            travel += bullet.speed.abs() * bullet.life;
                            bullet = child;
                        }
                    }
                }
                _ => {}
            }
        }
        row.reach = clip_reach(duel, id, clip.duration, glam::Mat4::IDENTITY);
        row.active.sort_by(|a, b| a[0].total_cmp(&b[0]));
        if let Some(window) = row.active.first() {
            let mut time = window[0] + 0.001;
            while time <= window[1].min(clip.duration) + 0.001 {
                if let Some(reach) = instantaneous_reach(duel, id, time, glam::Mat4::IDENTITY, None)
                {
                    row.first_window.push([rounded(time), reach]);
                }
                time += 1.0 / 60.0;
            }
        }
        row.first_reach = row.first_window.iter().map(|w| w[1]).max_by(f32::total_cmp);
        let mut hits: Vec<_> = std::mem::take(&mut row.hits)
            .into_iter()
            .zip(std::mem::take(&mut row.damage))
            .zip(std::mem::take(&mut row.hit_stun))
            .collect();
        hits.sort_by(|a, b| a.0.0[0].total_cmp(&b.0.0[0]));
        for ((hit, damage), stun) in hits {
            let mut windows = Vec::new();
            if let Some(event) = tae.events.iter().find(|e| {
                rounded(e.start) == hit[0]
                    && matches!(e.kind,TaeKind::Attack { judge,.. } if judge == hit[3] as i32)
            }) {
                let mut time = event.start + 0.001;
                while time <= event.end.min(clip.duration) + 0.001 {
                    if let Some(reach) = instantaneous_reach(
                        duel,
                        id,
                        time,
                        glam::Mat4::IDENTITY,
                        Some(hit[3] as i32),
                    ) {
                        windows.push([rounded(time), reach]);
                    }
                    time += 1.0 / 60.0;
                }
            }
            row.contact_windows.push(windows);
            row.hits.push(hit);
            row.damage.push(damage);
            row.hit_stun.push(stun);
        }
        row.cancels.sort_by(|a, b| a[0].total_cmp(&b[0]));
        row.last_hit = row
            .hits
            .iter()
            .map(|h| h[0])
            .chain(row.projectiles.iter().map(|p| projectile_time(p, 3.0)))
            .fold(0.0, f32::max);
        row.recovery = if row.contact {
            rounded(row.wait(row.last_hit))
        } else {
            0.0
        };
        row.contact =
            row.damaging() || (row.contact && row.hits.is_empty() && row.projectiles.is_empty());
        row.posture_total = row
            .hits
            .iter()
            .map(|h| h[1])
            .chain(row.projectiles.iter().map(|p| p[5]))
            .sum();
        sheet.insert(id.to_string(), row);
    }
    let mut combos: BTreeSet<Vec<i32>> = actions.combos.iter().map(|c| c.ids.to_vec()).collect();
    // Attack groups in mixed tactic plans are part of the offered vocabulary too.
    for chain in actions.chains {
        for group in chain.split('>') {
            if group.contains('+')
                && let Ok(ids) = group
                    .split('+')
                    .map(str::parse::<i32>)
                    .collect::<Result<Vec<_>, _>>()
            {
                combos.insert(ids);
            }
        }
    }
    for ids in combos {
        let mut row = Move::default();
        let mut complete = true;
        let mut origin = glam::Mat4::IDENTITY;
        for (i, id) in ids.iter().enumerate() {
            let Some(part) = sheet.get(&id.to_string()) else {
                complete = false;
                break;
            };
            let offset = row.duration;
            if i == 0 {
                row.first_reach = part.first_reach;
                row.first_window = part.first_window.clone();
            }
            row.contact |= part.contact;
            let length = if i + 1 < ids.len() {
                part.wait(0.0)
            } else {
                part.duration
            };
            row.hits.extend(
                part.hits
                    .iter()
                    .filter(|h| h[0] <= length)
                    .map(|h| [rounded(h[0] + offset), h[1], h[2], h[3]]),
            );
            row.damage.extend(
                part.damage
                    .iter()
                    .zip(&part.hits)
                    .filter(|(_, h)| h[0] <= length)
                    .map(|(d, _)| *d),
            );
            row.hit_stun.extend(
                part.hit_stun
                    .iter()
                    .zip(&part.hits)
                    .filter(|(_, h)| h[0] <= length)
                    .map(|(s, _)| *s),
            );
            row.contact_windows.extend(
                part.contact_windows
                    .iter()
                    .zip(&part.hits)
                    .filter(|(_, h)| h[0] <= length)
                    .map(|(windows, h)| {
                        windows
                            .iter()
                            .filter(|w| w[0] <= length)
                            .filter_map(|w| {
                                Some([
                                    rounded(w[0] + offset),
                                    instantaneous_reach(
                                        duel,
                                        *id,
                                        w[0],
                                        origin,
                                        Some(h[3] as i32),
                                    )?,
                                ])
                            })
                            .collect()
                    }),
            );
            row.active.extend(
                part.active
                    .iter()
                    .filter(|w| w[0] <= length)
                    .map(|w| [rounded(w[0] + offset), rounded(w[1].min(length) + offset)]),
            );
            row.projectiles
                .extend(part.projectiles.iter().filter(|p| p[9] <= length).map(|p| {
                    [
                        rounded(p[0] + offset),
                        p[1],
                        p[2],
                        p[3],
                        p[4],
                        p[5],
                        p[6] + origin.w_axis.x.hypot(origin.w_axis.z),
                        p[7],
                        p[8],
                        rounded(p[9] + offset),
                    ]
                }));
            row.projectile_stun.extend(
                part.projectile_stun
                    .iter()
                    .zip(&part.projectiles)
                    .filter(|(_, p)| p[9] <= length)
                    .map(|(s, _)| *s),
            );
            row.cancels
                .extend(part.cancels.iter().filter(|w| w[0] <= length).map(|w| {
                    [
                        rounded(w[0] + offset),
                        rounded(w[1].min(length) + offset),
                        w[2],
                        w[3],
                    ]
                }));
            row.duration = rounded(offset + length);
            row.recovery = part.recovery;
            row.reach = row
                .reach
                .into_iter()
                .chain(clip_reach(duel, *id, length, origin))
                .max_by(f32::total_cmp);
            if let Some(clip) = npc.clips.clip(&format!("a000_{id:06}")) {
                origin *= root_at(&clip, length);
            }
        }
        if complete {
            for pair in ids.windows(2) {
                let row = sheet.get_mut(&pair[0].to_string()).unwrap();
                if !row.chain_next.contains(&pair[1]) {
                    row.chain_next.push(pair[1]);
                }
            }
            row.posture_total = row
                .hits
                .iter()
                .map(|h| h[1])
                .chain(row.projectiles.iter().map(|p| p[5]))
                .sum();
            row.last_hit = row
                .hits
                .iter()
                .map(|h| h[0])
                .chain(row.projectiles.iter().map(|p| projectile_time(p, 3.0)))
                .fold(0.0, f32::max);
            let key = ids.iter().map(i32::to_string).collect::<Vec<_>>().join("+");
            for id in &ids {
                sheet
                    .get_mut(&id.to_string())
                    .unwrap()
                    .chains
                    .push(key.clone());
            }
            sheet.insert(
                ids.iter().map(i32::to_string).collect::<Vec<_>>().join("+"),
                row,
            );
        }
    }
    let candidates: Vec<_> = actions
        .actions
        .iter()
        .filter(|a| sheet.get(&a.id.to_string()).is_some_and(Move::damaging))
        .map(|a| a.id)
        .collect();
    for row in sheet.values_mut() {
        row.chain_next.sort_unstable();
        if row
            .cancels
            .iter()
            .any(|w| w[3] == 0.0 && matches!(w[2] as i32, ai_cancel::COMBO | ai_cancel::ATTACK))
        {
            row.cancel_into = candidates.clone();
        }
    }
    sheet
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn punish_facts_include_guard_followups_and_projectile_interception() {
        let row = Move {
            first_reach: Some(3.0),
            reach: Some(3.0),
            hits: vec![[0.4, 20.0, 10.0, 1.0], [1.5, 30.0, 15.0, 2.0]],
            damage: vec![[10.0, 12.0], [15.0, 18.0]],
            hit_stun: vec![Some(1.2), Some(0.3)],
            recovery: 0.7,
            ..Default::default()
        };
        let sheet = BTreeMap::from([("3000".into(), row)]);
        let list = viable_punishes(
            &sheet,
            &super::super::GENICHIRO,
            [1.5, 0.0],
            [0.0; 2],
            1.3,
            &Default::default(),
        );
        let p = &list[0];
        assert_eq!(
            (p.hits, p.window, p.total, p.guard_after, p.recovery),
            (1, [10.0, 12.0], [25.0, 30.0], 30.0, 0.7)
        );
        assert_eq!(p.stun_lock, Some(true));
        let projectile = [0.2, 10.0, 2.0, 0.0, 10.0, 12.0, 0.0, 8.0, 4.0, 0.2];
        assert!((intercept(&projectile, [8.0, 0.0], [2.0, 0.0]).unwrap() - 1.25).abs() < 0.001);
        assert!(intercept(&projectile, [8.0, 0.0], [12.0, 0.0]).is_none());
    }

    #[test]
    fn brain_combo_prefix_breaks_with_a_fast_followup_left() {
        let row = Move {
            hits: vec![
                [0.3, 20.0, 10.0, 1.0],
                [0.8, 30.0, 15.0, 2.0],
                [1.2, 40.0, 20.0, 3.0],
            ],
            ..Default::default()
        };
        assert_eq!(row.breaking_followup(45.0, 3.0), Some(0.40000004));
        assert!(row.breaking_followup(60.0, 3.0).is_none());
        let bow_then_slash = Move {
            projectiles: vec![[0.2, 80.0, 2.0, 0.1, 64.0, 17.0, 0.5, 18.0, 8.0, 0.2]],
            hits: vec![[0.6, 30.0, 15.0, 2.0]],
            ..Default::default()
        };
        assert!((bow_then_slash.breaking_followup(15.0, 3.0).unwrap() - 0.37).abs() < 0.001);
    }

    #[test]
    fn attack_wait_uses_only_live_attack_windows_or_end() {
        let row = Move {
            duration: 3.0,
            cancels: vec![
                [0.1, 0.5, 79.0, 0.0],
                [0.2, 0.5, 23.0, 99.0],
                [1.0, 1.5, 23.0, 0.0],
                [2.0, 2.5, 86.0, 0.0],
            ],
            ..Default::default()
        };
        assert_eq!(row.wait(0.0), 1.0);
        assert_eq!(row.wait(1.0), 0.0);
        assert_eq!(row.wait(1.5), 0.5);
        assert_eq!(row.wait(2.5), 0.5);
        assert_eq!(row.wait(3.0), 0.0);
    }
}
