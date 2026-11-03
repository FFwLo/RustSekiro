//! Wolf's habits, as the boss could see them: where he attacks from, what he does right after
//! the boss's deflected or guarded hits, after his own hits are guarded and after the boss's
//! back steps, how he answers each boss action (deflect, guard, dodge, jump, or gets hit), how
//! fast he attacks once a boss action ends, when he heals, how often he jumps and how long he
//! spends at each range.
//!
//! Mirrored fairness: everything comes from what the boss can see of Wolf (his position,
//! behaviour state, guard, airborne, HP bar and the hits), never from his buttons. The counters
//! ([`Habits`]) are summed across fights in the memory file; [`HabitSummary`] is the compact
//! form sent with each request.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::tally::{Side, TallyEvent, TopMap};

/// Range bands in metres: close below the first, far beyond the second.
pub const RANGE_BANDS: [f32; 2] = [3.0, 6.0];
/// Seconds after a deflect or guard trigger in which Wolf's response counts.
pub const AFTER_WINDOW: f32 = 1.2;
/// Seconds after the start of a boss back step in which Wolf's response counts (the step
/// itself takes most of a second).
pub const BACK_STEP_WINDOW: f32 = 2.0;
/// Seconds after a boss action ends in which Wolf's attack counts as punishing it.
pub const PUNISH_WINDOW: f32 = 1.5;
/// HP fraction below which Wolf counts as low.
pub const LOW_HP: f32 = 0.5;
/// Metres Wolf must back away by, inside a window, to count as backing off.
pub const BACK_OFF: f32 = 1.5;
/// Entries with fewer samples are not sent.
pub const MIN_EVIDENCE: u32 = 2;

/// What Wolf is doing as the boss sees it (his attacks come from the tally's events).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Motion {
    Guard,
    Dodge,
    Jump,
    Heal,
    Other,
}

/// Wolf's motion from his behaviour state's leaf name, airborne and guard flags (ours: the
/// state names of `c0000.hks`, such as `GroundStep`, `GroundJumpStart`, `Item*`).
pub fn motion(state: &str, airborne: bool, guarding: bool) -> Motion {
    if state.contains("Attack") {
        Motion::Other
    } else if state.starts_with("Item") {
        Motion::Heal
    } else if state.contains("Jump") || (airborne && !state.contains("Damage")) {
        Motion::Jump
    } else if state.contains("Step") && !state.contains("FromStep") {
        Motion::Dodge
    } else if guarding {
        Motion::Guard
    } else {
        Motion::Other
    }
}

/// One step of Wolf as the boss sees him.
#[derive(Debug, Clone, Copy)]
pub struct WolfFrame<'a> {
    /// Fight clock, seconds.
    pub t: f32,
    pub dt: f32,
    /// Horizontal distance to the boss, metres.
    pub dist: f32,
    /// Leaf name of his behaviour state.
    pub state: &'a str,
    pub airborne: bool,
    pub guarding: bool,
    /// HP fraction.
    pub hp: f32,
}

/// A running mean.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Mean {
    pub sum: f32,
    pub n: u32,
}

impl Mean {
    pub fn add(&mut self, x: f32) {
        self.sum += x;
        self.n += 1;
    }

    pub fn merge(&mut self, o: &Mean) {
        self.sum += o.sum;
        self.n += o.n;
    }

    pub fn is_empty(&self) -> bool {
        self.n == 0
    }

    pub fn mean(&self) -> Option<f32> {
        (self.n > 0).then(|| self.sum / self.n as f32)
    }
}

/// What Wolf did in reply to something, counted per response.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Responses {
    pub n: u32,
    /// Response name -> times.
    pub to: BTreeMap<String, u32>,
    /// Distance when the response was an attack.
    #[serde(skip_serializing_if = "Mean::is_empty")]
    pub attack_dist: Mean,
}

impl Responses {
    fn add(&mut self, response: &str, attack_dist: Option<f32>) {
        self.n += 1;
        *self.to.entry(response.to_string()).or_default() += 1;
        if let Some(d) = attack_dist {
            self.attack_dist.add(d);
        }
    }

    fn merge(&mut self, o: &Responses) {
        self.n += o.n;
        for (k, v) in &o.to {
            *self.to.entry(k.clone()).or_default() += v;
        }
        self.attack_dist.merge(&o.attack_dist);
    }

    /// The top two responses with their share, such as `attack 90% from 3.0m, wait 10% n10`.
    pub fn describe(&self) -> String {
        let mut top: Vec<_> = self.to.iter().collect();
        top.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
        let parts: Vec<String> = top
            .iter()
            .take(2)
            .map(|&(name, &k)| {
                let pct = (100.0 * k as f32 / self.n.max(1) as f32).round();
                match (name.as_str(), self.attack_dist.mean()) {
                    ("attack", Some(d)) => format!("attack {pct}% from {:.1}m", d),
                    _ => format!("{name} {pct}%"),
                }
            })
            .collect();
        format!("{} n{}", parts.join(", "), self.n)
    }
}

/// Healing seen.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Heals {
    /// HP fraction at each heal.
    pub hp: Mean,
    /// Heals started below [`LOW_HP`].
    pub at_low: u32,
    /// Times his HP fell below [`LOW_HP`].
    pub low_spells: u32,
}

/// Wolf's habit counters, summed over steps and fights.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Habits {
    /// Seconds watched.
    pub seconds: f32,
    /// Seconds at close, mid and far range ([`RANGE_BANDS`]).
    pub range_seconds: [f32; 3],
    /// Distance when his attacks start (when his swing is signalled, TAE flag 63).
    pub attack_dist: Mean,
    /// Trigger (`deflect`, `guard`, `guarded`, `back_step`) -> his responses (`attack`,
    /// `dodge`, `jump`, `heal`, `guard`, `back`, `wait`).
    pub after: BTreeMap<String, Responses>,
    /// Boss action id -> his answer: the first hit of the exchange (`deflect`, `guard`, `hit`:
    /// the boss's hit landed, `counter`: Wolf's landed first), else `dodge`, `jump` or `none`.
    pub answers: BTreeMap<String, Responses>,
    /// Boss action id -> seconds from its end to his next attack (within [`PUNISH_WINDOW`]).
    pub punish: BTreeMap<String, Mean>,
    pub heals: Heals,
    pub jumps: u32,
}

impl Habits {
    /// Adds another set of counters (merging fights).
    pub fn merge(&mut self, o: &Habits) {
        self.seconds += o.seconds;
        for (a, b) in self.range_seconds.iter_mut().zip(o.range_seconds) {
            *a += b;
        }
        self.attack_dist.merge(&o.attack_dist);
        for (k, r) in &o.after {
            self.after.entry(k.clone()).or_default().merge(r);
        }
        for (k, r) in &o.answers {
            self.answers.entry(k.clone()).or_default().merge(r);
        }
        for (k, m) in &o.punish {
            self.punish.entry(k.clone()).or_default().merge(m);
        }
        self.heals.hp.merge(&o.heals.hp);
        self.heals.at_low += o.heals.at_low;
        self.heals.low_spells += o.heals.low_spells;
        self.jumps += o.jumps;
    }

    /// The compact form for a request: rounded, only entries with [`MIN_EVIDENCE`], the most
    /// sampled first.
    pub fn summary(&self) -> HabitSummary {
        let enough = |r: &&Responses| r.n >= MIN_EVIDENCE;
        let described = |m: &BTreeMap<String, Responses>| {
            TopMap::ranked(
                m.iter()
                    .filter(|(_, r)| enough(r))
                    .map(|(k, r)| (k.clone(), (r.n, r.describe()))),
                |(n, _)| *n as f32,
            )
        };
        let strip =
            |m: TopMap<(u32, String)>| TopMap(m.0.into_iter().map(|(k, (_, s))| (k, s)).collect());
        let punish = TopMap::ranked(
            self.punish
                .iter()
                .filter(|(_, m)| m.n >= MIN_EVIDENCE)
                .map(|(k, m)| (k.clone(), (m.n, r2(m.sum / m.n as f32)))),
            |(n, _)| *n as f32,
        );
        let h = &self.heals;
        HabitSummary {
            attack_m: (self.attack_dist.n >= MIN_EVIDENCE)
                .then(|| r1(self.attack_dist.sum / self.attack_dist.n as f32)),
            range_pct: (self.seconds >= 5.0).then(|| {
                self.range_seconds
                    .map(|s| (100.0 * s / self.seconds).round() as u32)
            }),
            after: strip(described(&self.after)),
            vs: strip(described(&self.answers)),
            punish_s: TopMap(punish.0.into_iter().map(|(k, (_, s))| (k, s)).collect()),
            heal: match h.hp.mean() {
                Some(hp) => Some(format!(
                    "{} heals, mean hp {hp:.2}; healed in {} of {} low-hp spells",
                    h.hp.n,
                    h.at_low.min(h.low_spells),
                    h.low_spells
                )),
                None if h.low_spells > 0 => {
                    Some(format!("never healed in {} low-hp spells", h.low_spells))
                }
                None => None,
            },
            jumps_pm: (self.seconds >= 10.0).then(|| r1(self.jumps as f32 * 60.0 / self.seconds)),
        }
    }
}

/// [`Habits`] as a controller sees them.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct HabitSummary {
    /// Mean distance his attacks start from, metres.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attack_m: Option<f32>,
    /// Percent of time close (< 3 m), mid and far (> 6 m).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub range_pct: Option<[u32; 3]>,
    /// What he does after `deflect` (he deflected a boss hit), `guard` (he guarded one),
    /// `guarded` (the boss guarded or deflected his hit) and `back_step` (a boss back step).
    #[serde(skip_serializing_if = "TopMap::is_empty")]
    pub after: TopMap<String>,
    /// How he answers each boss action.
    #[serde(skip_serializing_if = "TopMap::is_empty")]
    pub vs: TopMap<String>,
    /// Mean seconds from each boss action's end to his attack.
    #[serde(skip_serializing_if = "TopMap::is_empty")]
    pub punish_s: TopMap<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heal: Option<String>,
    /// Jumps per minute.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jumps_pm: Option<f32>,
}

impl HabitSummary {
    /// Drops one item to shrink a request: the least sampled entry of the first map holding
    /// more than `keep`, in the order `punish_s`, `vs`, `after`; at `keep` 0 also the single
    /// values. True if something went.
    pub fn trim(&mut self, keep: usize) -> bool {
        if self.punish_s.pop(keep) || self.vs.pop(keep) || self.after.pop(keep) {
            return true;
        }
        if keep > 0 {
            return false;
        }
        for slot in [&mut self.jumps_pm, &mut self.attack_m] {
            if slot.take().is_some() {
                return true;
            }
        }
        self.heal.take().is_some() || self.range_pct.take().is_some()
    }
}

fn r1(x: f32) -> f32 {
    (x * 10.0).round() / 10.0
}

fn r2(x: f32) -> f32 {
    (x * 100.0).round() / 100.0
}

/// A trigger waiting for Wolf's response.
#[derive(Debug, Clone)]
struct Pending {
    trigger: &'static str,
    until: f32,
    dist: f32,
}

/// The boss action in progress and how Wolf answered it so far.
#[derive(Debug, Clone)]
struct BossOpen {
    key: String,
    answer: Option<&'static str>,
    dodged: bool,
    jumped: bool,
}

/// Builds [`Habits`] for one fight from the tally's events and one [`WolfFrame`] per step.
#[derive(Debug, Clone, Default)]
pub struct HabitTracker {
    /// This fight's counters.
    pub habits: Habits,
    back_steps: Vec<String>,
    pending: Vec<Pending>,
    boss_open: Option<BossOpen>,
    /// The boss action that ended last and when, while Wolf may still punish it.
    last_end: Option<(String, f32)>,
    prev: Option<Motion>,
    low: bool,
}

impl HabitTracker {
    /// `back_steps`: the boss's back-step action ids.
    pub fn new(back_steps: &[i32]) -> Self {
        Self {
            back_steps: back_steps.iter().map(i32::to_string).collect(),
            ..Self::default()
        }
    }

    /// One step: this step's tally events, then Wolf's state.
    pub fn step(&mut self, frame: &WolfFrame, events: &[TallyEvent]) {
        let t = frame.t;
        for e in events {
            match e {
                TallyEvent::Progress { .. } | TallyEvent::Damaged { .. } => {}
                TallyEvent::ProjectileHit {
                    guarded, deflected, ..
                } => {
                    if *guarded || *deflected {
                        self.trigger(
                            if *deflected { "deflect" } else { "guard" },
                            t + AFTER_WINDOW,
                            frame.dist,
                        );
                    }
                }
                TallyEvent::Countered { .. } => {
                    if let Some(open) = self.boss_open.as_mut() {
                        open.answer = Some("counter");
                    }
                }
                TallyEvent::Start {
                    side: Side::Boss,
                    key,
                } => {
                    self.end_boss(t);
                    if self.back_steps.contains(key) {
                        self.trigger("back_step", t + BACK_STEP_WINDOW, frame.dist);
                    }
                    self.boss_open = Some(BossOpen {
                        key: key.clone(),
                        answer: None,
                        dodged: false,
                        jumped: false,
                    });
                }
                TallyEvent::End { side: Side::Boss } => self.end_boss(t),
                TallyEvent::Start {
                    side: Side::Wolf, ..
                } => self.wolf_attack(t, frame.dist),
                TallyEvent::End { side: Side::Wolf } => {}
                TallyEvent::Hit {
                    attacker: Side::Boss,
                    guarded,
                    deflected,
                } => {
                    let answer = if *deflected {
                        "deflect"
                    } else if *guarded {
                        "guard"
                    } else {
                        "hit"
                    };
                    if let Some(open) = self.boss_open.as_mut() {
                        open.answer.get_or_insert(answer);
                    }
                    if *deflected || *guarded {
                        self.trigger(answer, t + AFTER_WINDOW, frame.dist);
                    }
                }
                TallyEvent::Hit {
                    attacker: Side::Wolf,
                    guarded,
                    deflected,
                } => {
                    if *guarded || *deflected {
                        self.trigger("guarded", t + AFTER_WINDOW, frame.dist);
                    } else if let Some(open) = self.boss_open.as_mut() {
                        // He hit the boss first, through the action.
                        open.answer.get_or_insert("counter");
                    }
                }
            }
        }
        self.frame(frame);
    }

    /// The fight is over: settles the boss action in progress and drops unanswered triggers.
    pub fn flush(&mut self, t: f32) {
        self.end_boss(t);
        self.pending.clear();
        self.last_end = None;
    }

    fn trigger(&mut self, trigger: &'static str, until: f32, dist: f32) {
        if !self.pending.iter().any(|p| p.trigger == trigger) {
            self.pending.push(Pending {
                trigger,
                until,
                dist,
            });
        }
    }

    /// Answers every waiting trigger with `response`.
    fn respond(&mut self, response: &str, attack_dist: Option<f32>) {
        for p in std::mem::take(&mut self.pending) {
            self.habits
                .after
                .entry(p.trigger.to_string())
                .or_default()
                .add(response, attack_dist);
        }
    }

    fn end_boss(&mut self, t: f32) {
        if let Some(open) = self.boss_open.take() {
            let answer = open.answer.unwrap_or(if open.dodged {
                "dodge"
            } else if open.jumped {
                "jump"
            } else {
                "none"
            });
            self.habits
                .answers
                .entry(open.key.clone())
                .or_default()
                .add(answer, None);
            self.last_end = Some((open.key, t));
        }
    }

    fn wolf_attack(&mut self, t: f32, dist: f32) {
        self.habits.attack_dist.add(dist);
        self.respond("attack", Some(dist));
        if let Some((key, end)) = self.last_end.take()
            && t - end <= PUNISH_WINDOW
        {
            self.habits.punish.entry(key).or_default().add(t - end);
        }
    }

    fn frame(&mut self, f: &WolfFrame) {
        let h = &mut self.habits;
        h.seconds += f.dt;
        let band = if f.dist < RANGE_BANDS[0] {
            0
        } else if f.dist <= RANGE_BANDS[1] {
            1
        } else {
            2
        };
        h.range_seconds[band] += f.dt;
        if f.hp < LOW_HP && !self.low {
            h.heals.low_spells += 1;
        }
        self.low = f.hp < LOW_HP;
        if self
            .last_end
            .as_ref()
            .is_some_and(|(_, end)| f.t - end > PUNISH_WINDOW)
        {
            self.last_end = None;
        }
        let m = motion(f.state, f.airborne, f.guarding);
        if self.prev != Some(m) {
            match m {
                Motion::Dodge => {
                    if let Some(o) = self.boss_open.as_mut() {
                        o.dodged = true;
                    }
                    self.respond("dodge", None);
                }
                Motion::Jump => {
                    self.habits.jumps += 1;
                    if let Some(o) = self.boss_open.as_mut() {
                        o.jumped = true;
                    }
                    self.respond("jump", None);
                }
                Motion::Heal => {
                    self.habits.heals.hp.add(f.hp);
                    if f.hp < LOW_HP {
                        self.habits.heals.at_low += 1;
                    }
                    self.respond("heal", None);
                }
                Motion::Guard | Motion::Other => {}
            }
        }
        self.prev = Some(m);
        // Windows that ran out without an action: still guarding, backed off, or waited.
        let (done, keep): (Vec<_>, Vec<_>) = std::mem::take(&mut self.pending)
            .into_iter()
            .partition(|p| f.t >= p.until);
        self.pending = keep;
        for p in done {
            let response = if f.guarding {
                "guard"
            } else if f.dist - p.dist > BACK_OFF {
                "back"
            } else {
                "wait"
            };
            self.habits
                .after
                .entry(p.trigger.to_string())
                .or_default()
                .add(response, None);
        }
    }
}
