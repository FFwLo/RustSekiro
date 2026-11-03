//! Outcome memory of one fight: how each of the boss's actions and each of Wolf's attacks
//! ended. Hosted models keep nothing between calls, so this is sent with every request; the
//! memory across fights is [`super::memory`].
//!
//! An *exchange* is one action of one side, from its start until its clip ends, plus a punish
//! window after it. Its outcome flags:
//!
//! - `hit`: one of its hits landed unguarded;
//! - `blocked`: the other side guarded one of its hits;
//! - `deflected`: the other side deflected one of its hits;
//! - `whiffed`: none of the above (dodged, out of range, interrupted before contact);
//! - `punished`: its side took HP/posture damage during it or within the punish window after;
//! - `progress` and `broke`: resolved HP/posture progress and a defender posture break.
//!
//! Hits that arrive after the clip ended (a shuriken in flight) count for the side's latest
//! exchange.

use std::collections::BTreeMap;

use serde::de::Deserializer;
use serde::ser::SerializeMap;
use serde::{Deserialize, Serialize, Serializer};

/// Which side an exchange belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    /// The boss (index 1 in the duel).
    Boss,
    /// The player (index 0).
    Wolf,
}

impl Side {
    pub fn index(self) -> usize {
        match self {
            Side::Wolf => 0,
            Side::Boss => 1,
        }
    }

    pub fn from_index(i: usize) -> Self {
        if i == 0 { Side::Wolf } else { Side::Boss }
    }

    pub fn other(self) -> Self {
        match self {
            Side::Wolf => Side::Boss,
            Side::Boss => Side::Wolf,
        }
    }
}

/// How one exchange ended.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Outcome {
    pub no_contact: bool,
    pub idle_guard: bool,
    pub progress: bool,
    pub broke: bool,
    pub countered: Option<CounterKind>,
    pub hit: bool,
    pub blocked: bool,
    pub deflected: bool,
    pub punished: bool,
}

impl Outcome {
    pub fn whiffed(&self) -> bool {
        !(self.hit || self.blocked || self.deflected || self.countered.is_some())
    }
}

/// One action of one side.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Exchange {
    pub phase: String,
    pub side: Side,
    /// The boss's action id, or the name of Wolf's attack state.
    pub key: String,
    #[serde(serialize_with = "ser_ms")]
    pub start: f32,
    #[serde(serialize_with = "ser_ms_opt")]
    pub end: Option<f32>,
    #[serde(flatten)]
    pub outcome: Outcome,
}

impl Exchange {
    /// Only contact-capable attacks can whiff; passivity is separate failure evidence.
    pub fn whiffed(&self) -> bool {
        let attack = self.side == Side::Wolf
            || self
                .key
                .parse::<i32>()
                .is_ok_and(|id| (3000..3100).contains(&id));
        !self.outcome.no_contact && attack && self.outcome.whiffed()
    }
}

/// Counts over exchanges: `[n, hit, blocked, deflected, whiffed, punished]` in JSON.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    pub countered: u32,
    pub counter_kinds: [u32; 3],
    pub n: u32,
    pub hit: u32,
    pub blocked: u32,
    pub deflected: u32,
    pub whiffed: u32,
    pub punished: u32,
}

impl Counts {
    fn add(&mut self, exchange: &Exchange) {
        let o = &exchange.outcome;
        self.n += 1;
        self.hit += u32::from(o.hit);
        self.blocked += u32::from(o.blocked);
        self.deflected += u32::from(o.deflected);
        self.whiffed += u32::from(exchange.whiffed());
        self.punished += u32::from(o.punished);
        self.countered += u32::from(o.countered.is_some());
        if let Some(kind) = o.countered {
            self.counter_kinds[kind as usize] += 1;
        }
    }

    /// Adds another set of counts (merging fights).
    pub fn merge(&mut self, o: &Counts) {
        self.n += o.n;
        self.hit += o.hit;
        self.blocked += o.blocked;
        self.deflected += o.deflected;
        self.whiffed += o.whiffed;
        self.punished += o.punished;
        self.countered += o.countered;
        for (a, b) in self.counter_kinds.iter_mut().zip(o.counter_kinds) {
            *a += b;
        }
    }

    fn array(&self) -> [u32; 10] {
        [
            self.n,
            self.hit,
            self.blocked,
            self.deflected,
            self.whiffed,
            self.punished,
            self.countered,
            self.counter_kinds[0],
            self.counter_kinds[1],
            self.counter_kinds[2],
        ]
    }
}

impl Serialize for Counts {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.array().serialize(s)
    }
}

impl<'de> Deserialize<'de> for Counts {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let values = Vec::<u32>::deserialize(d)?;
        if !matches!(values.len(), 6 | 7 | 10) {
            return Err(serde::de::Error::custom(
                "expected six, seven or ten outcome counts",
            ));
        }
        let [n, hit, blocked, deflected, whiffed, punished] = values[..6].try_into().unwrap();
        Ok(Self {
            countered: values.get(6).copied().unwrap_or_default(),
            counter_kinds: [
                values.get(7).copied().unwrap_or_default(),
                values.get(8).copied().unwrap_or_default(),
                values.get(9).copied().unwrap_or_default(),
            ],
            n,
            hit,
            blocked,
            deflected,
            whiffed,
            punished,
        })
    }
}

/// Counts per key, as stored.
pub type CountMap = BTreeMap<String, Counts>;

/// Adds every key of `from` into `into`.
pub fn merge_counts(into: &mut CountMap, from: &CountMap) {
    for (k, c) in from {
        into.entry(k.clone()).or_default().merge(c);
    }
}

/// A map sent to controllers with its most telling entries first, so a request that must
/// shrink drops the entries with the least evidence ([`TopMap::pop`]). Serializes as a JSON
/// object in that order.
#[derive(Debug, Clone, PartialEq)]
pub struct TopMap<V>(pub Vec<(String, V)>);

impl<V> Default for TopMap<V> {
    fn default() -> Self {
        Self(Vec::new())
    }
}

impl<V> TopMap<V> {
    /// Entries ordered by `weight`, highest first, then by key.
    pub fn ranked(
        entries: impl IntoIterator<Item = (String, V)>,
        weight: impl Fn(&V) -> f32,
    ) -> Self {
        let mut v: Vec<_> = entries.into_iter().collect();
        v.sort_by(|a, b| {
            weight(&b.1)
                .total_cmp(&weight(&a.1))
                .then_with(|| a.0.cmp(&b.0))
        });
        Self(v)
    }

    pub fn get(&self, key: &str) -> Option<&V> {
        self.0.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn values(&self) -> impl Iterator<Item = &V> {
        self.0.iter().map(|(_, v)| v)
    }

    /// Adds or replaces an entry, at the end.
    pub fn insert(&mut self, key: impl Into<String>, value: V) {
        let key = key.into();
        self.0.retain(|(k, _)| *k != key);
        self.0.push((key, value));
    }

    /// Drops the last (least telling) entry while more than `keep` remain; true if one went.
    pub fn pop(&mut self, keep: usize) -> bool {
        if self.0.len() > keep {
            self.0.pop();
            true
        } else {
            false
        }
    }
}

impl<V> std::ops::Index<&str> for TopMap<V> {
    type Output = V;
    fn index(&self, key: &str) -> &V {
        self.get(key)
            .unwrap_or_else(|| panic!("no entry {key} in the map"))
    }
}

impl<V: Serialize> Serialize for TopMap<V> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut m = s.serialize_map(Some(self.0.len()))?;
        for (k, v) in &self.0 {
            m.serialize_entry(k, v)?;
        }
        m.end()
    }
}

/// Counts as a controller sees them: most exchanges first.
pub fn ranked_counts(map: &CountMap) -> TopMap<Counts> {
    TopMap::ranked(map.iter().map(|(k, c)| (k.clone(), *c)), |c| c.n as f32)
}

/// The tally of this fight as a controller sees it.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct TallySummary {
    /// Boss action id -> counts, most used first.
    pub boss: TopMap<Counts>,
    /// Wolf's attack (state name) -> counts.
    pub wolf: TopMap<Counts>,
}

/// Something that happened, as the tally needs it.
#[derive(Debug, Clone, PartialEq)]
pub enum TallyEvent {
    ProjectileHit {
        key: String,
        guarded: bool,
        deflected: bool,
        progress: bool,
    },
    Damaged {
        side: Side,
    },
    Progress {
        attacker: Side,
        broke: bool,
    },
    Countered {
        kind: CounterKind,
    },
    /// A side started an action (ending its previous one).
    Start {
        side: Side,
        key: String,
    },
    /// A side's action ended without a new one.
    End {
        side: Side,
    },
    /// A hit from `attacker`'s side resolved.
    Hit {
        attacker: Side,
        guarded: bool,
        deflected: bool,
    },
}

/// The running tally of one fight.
#[derive(Debug, Clone)]
pub struct Tally {
    contact_until: BTreeMap<String, f32>,
    pub phase: String,
    pub shared: std::collections::BTreeSet<String>,
    pub phases: BTreeMap<String, CountMap>,
    /// Seconds after an action's end during which a hit on its side still punishes it.
    pub punish_window: f32,
    open: [Option<Exchange>; 2],
    /// Ended exchanges still inside their punish window, oldest first.
    closing: Vec<Exchange>,
    all: [CountMap; 2],
}

impl Default for Tally {
    fn default() -> Self {
        Self {
            phase: String::new(),
            shared: Default::default(),
            phases: BTreeMap::new(),
            punish_window: 1.0,
            open: [None, None],
            closing: Vec::new(),
            contact_until: Default::default(),
            all: [CountMap::new(), CountMap::new()],
        }
    }
}

impl Tally {
    pub fn retain_projectile(&mut self, key: &str, until: f32) {
        self.contact_until.insert(key.into(), until);
    }

    pub fn projectile_hit(&mut self, key: &str, guarded: bool, deflected: bool, progress: bool) {
        let open = self.open[Side::Boss.index()]
            .as_mut()
            .filter(|x| x.key == key);
        let target = open.or_else(|| {
            self.closing
                .iter_mut()
                .rev()
                .find(|x| x.side == Side::Boss && x.key == key)
        });
        if let Some(x) = target {
            x.outcome.hit |= !guarded && !deflected;
            x.outcome.blocked |= guarded && !deflected;
            x.outcome.deflected |= deflected;
            x.outcome.progress |= progress;
        }
    }
    pub fn no_contact(&mut self) {
        if let Some(x) = self.latest_mut(Side::Boss) {
            x.outcome.no_contact = true;
        }
    }
    /// Guard held without an incoming attack is failure evidence for the current exchange.
    pub fn idle_guard(&mut self) {
        if let Some(x) = self.latest_mut(Side::Boss) {
            x.outcome.idle_guard = true;
        }
    }
    /// Applies an event at fight time `time`.
    pub fn record(&mut self, time: f32, event: TallyEvent) {
        match event {
            TallyEvent::ProjectileHit {
                key,
                guarded,
                deflected,
                progress,
            } => self.projectile_hit(&key, guarded, deflected, progress),
            TallyEvent::Damaged { side } => {
                if let Some(x) = self.open[side.index()].as_mut() {
                    x.outcome.punished = true;
                }
                for x in &mut self.closing {
                    if x.side == side && x.end.is_some_and(|end| time - end <= self.punish_window) {
                        x.outcome.punished = true;
                    }
                }
            }
            TallyEvent::Progress { attacker, broke } => {
                if let Some(x) = self.latest_mut(attacker) {
                    x.outcome.progress = true;
                    x.outcome.broke |= broke;
                }
            }
            TallyEvent::Countered { kind } => {
                if let Some(x) = self.latest_mut(Side::Boss) {
                    x.outcome.countered = Some(kind);
                }
            }
            TallyEvent::Start { side, key } => {
                self.close(side, time);
                self.open[side.index()] = Some(Exchange {
                    phase: self.phase.clone(),
                    side,
                    key,
                    start: time,
                    end: None,
                    outcome: Outcome::default(),
                });
            }
            TallyEvent::End { side } => self.close(side, time),
            TallyEvent::Hit {
                attacker,
                guarded,
                deflected,
            } => {
                if let Some(x) = self.latest_mut(attacker) {
                    if deflected {
                        x.outcome.deflected = true;
                    } else if guarded {
                        x.outcome.blocked = true;
                    } else {
                        x.outcome.hit = true;
                    }
                }
                if !guarded && !deflected {
                    let victim = attacker.other();
                    if let Some(x) = self.open[victim.index()].as_mut() {
                        x.outcome.punished = true;
                    }
                    let window = self.punish_window;
                    let recent = |x: &&mut Exchange| {
                        x.side == victim && x.end.is_some_and(|end| time - end <= window)
                    };
                    for x in self.closing.iter_mut().filter(recent) {
                        x.outcome.punished = true;
                    }
                }
            }
        }
    }

    /// Moves exchanges whose punish window has passed into the counts; returns them.
    pub fn advance(&mut self, time: f32) -> Vec<Exchange> {
        let window = self.punish_window;
        let (done, keep): (Vec<_>, Vec<_>) = self.closing.drain(..).partition(|x| {
            time - x.end.unwrap_or(time) >= window
                && self
                    .contact_until
                    .get(&x.key)
                    .is_none_or(|until| time >= *until)
        });
        self.closing = keep;
        for x in &done {
            if x.side == Side::Boss {
                self.phases
                    .entry(x.phase.clone())
                    .or_default()
                    .entry(x.key.clone())
                    .or_default()
                    .add(x);
            }
            self.all[x.side.index()]
                .entry(x.key.clone())
                .or_default()
                .add(x);
        }
        done
    }

    /// The fight is over: ends both sides' open exchanges now and counts everything still
    /// waiting out its punish window; returns those exchanges.
    pub fn flush(&mut self, time: f32) -> Vec<Exchange> {
        self.close(Side::Boss, time);
        self.close(Side::Wolf, time);
        self.advance(f32::INFINITY)
    }

    /// The side's action in progress.
    pub fn current(&self, side: Side) -> Option<&Exchange> {
        self.open[side.index()].as_ref()
    }

    /// Counts for one key of one side, over the fight.
    pub fn counts(&self, side: Side, key: &str) -> Counts {
        self.all[side.index()].get(key).copied().unwrap_or_default()
    }

    /// Every counted exchange of one side, per key.
    pub fn all(&self, side: Side) -> &CountMap {
        &self.all[side.index()]
    }

    /// Whether nothing has been counted or is in progress.
    pub fn is_empty(&self) -> bool {
        self.all.iter().all(BTreeMap::is_empty)
            && self.closing.is_empty()
            && self.open.iter().all(Option::is_none)
    }

    pub fn summary(&self) -> TallySummary {
        TallySummary {
            boss: ranked_counts(&self.all[1]),
            wolf: ranked_counts(&self.all[0]),
        }
    }

    fn close(&mut self, side: Side, time: f32) {
        if let Some(mut x) = self.open[side.index()].take() {
            x.end = Some(time);
            self.closing.push(x);
        }
    }

    /// The side's open exchange, or its most recently ended one.
    fn latest_mut(&mut self, side: Side) -> Option<&mut Exchange> {
        let i = side.index();
        if self.open[i].is_some() {
            return self.open[i].as_mut();
        }
        self.closing.iter_mut().rev().find(|x| x.side == side)
    }
}

/// A successful counter reported by combat resolution, rather than inferred from avoidance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CounterKind {
    Mikiri,
    Jump,
    Other,
}

fn ms(x: f32) -> f64 {
    (f64::from(x) * 1000.0).round() / 1000.0
}

fn ser_ms<S: Serializer>(x: &f32, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_f64(ms(*x))
}

fn ser_ms_opt<S: Serializer>(x: &Option<f32>, s: S) -> Result<S::Ok, S::Error> {
    x.map(ms).serialize(s)
}

#[cfg(test)]
mod counter_tests {
    use super::*;
    #[test]
    fn brain_projectile_contact_stays_with_the_firing_move_and_utility_cannot_whiff() {
        let mut tally = Tally::default();
        tally.record(
            0.0,
            TallyEvent::Start {
                side: Side::Boss,
                key: "3018".into(),
            },
        );
        tally.retain_projectile("3018", 3.0);
        tally.record(
            0.5,
            TallyEvent::Start {
                side: Side::Boss,
                key: "3034".into(),
            },
        );
        assert!(tally.advance(2.0).is_empty());
        tally.record(
            2.1,
            TallyEvent::ProjectileHit {
                key: "3018".into(),
                guarded: false,
                deflected: false,
                progress: true,
            },
        );
        let done = tally.advance(3.0);
        assert!(done[0].outcome.hit && !done[0].whiffed());
        tally.record(
            3.1,
            TallyEvent::Start {
                side: Side::Boss,
                key: "3031".into(),
            },
        );
        tally.no_contact();
        let done = tally.flush(4.0);
        assert!(done.iter().find(|x| x.key == "3034").unwrap().whiffed());
        assert!(!done.iter().find(|x| x.key == "3031").unwrap().whiffed());
    }
    #[test]
    fn successful_counters_are_not_whiffs_and_legacy_counts_default_to_zero() {
        let old: Counts = serde_json::from_str("[25,1,0,0,24,0]").unwrap();
        assert_eq!(old.countered, 0);
        for kind in [CounterKind::Mikiri, CounterKind::Jump, CounterKind::Other] {
            let mut tally = Tally {
                phase: "c7110".into(),
                ..Tally::default()
            };
            tally.record(
                0.0,
                TallyEvent::Start {
                    side: Side::Boss,
                    key: "3076".into(),
                },
            );
            tally.record(0.5, TallyEvent::Countered { kind });
            tally.record(0.6, TallyEvent::Countered { kind });
            let done = tally.flush(1.0);
            assert_eq!(done[0].outcome.countered, Some(kind));
            assert_eq!(tally.counts(Side::Boss, "3076").countered, 1);
            assert_eq!(tally.counts(Side::Boss, "3076").whiffed, 0);
            let mut memory = super::super::memory::Memory::new("c7100");
            memory.add_data(&tally, &super::super::habits::Habits::default());
            assert!(memory.boss.is_empty());
            assert_eq!(memory.phase_actions["c7110"]["3076"].countered, 1);
        }
    }
}
