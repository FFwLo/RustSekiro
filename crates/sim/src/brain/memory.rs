//! Shared boss evidence and isolated player profiles across fights and sessions.
//! [`LayerPaths`] migrates legacy memory without deleting its source, and persists counted
//! boss knowledge beside each player's habits, history and tuned game plan.
//! [`Memory`] is the version-6 profile and legacy format; [`BossKnowledge`] is version 1.

use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::coach::Avoid;
use super::coach::GamePlan;
use super::habits::{HabitSummary, Habits};
use super::metrics::{FightMetrics, FightRecord, HISTORY, Trend};
use super::tally::{CountMap, Counts, Side, Tally, TopMap, merge_counts, ranked_counts};
use std::collections::BTreeMap;

/// The memory file format version.
pub const MEMORY_VERSION: u32 = 6;

/// The oldest format this build still reads (version 1 had no fight history).
pub const OLDEST_VERSION: u32 = 1;

/// Where memory lives by default.
pub const MEMORY_DIR: &str = "logs/memory";

/// How a fight ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FightEnd {
    /// Wolf died.
    BossWon,
    /// The boss died.
    WolfWon,
    /// The game closed (or the file was saved) mid-fight.
    Unfinished,
}

/// The memory file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Memory {
    /// Phase -> distance bucket -> punish id -> observed punish outcomes.
    #[serde(default)]
    pub punishes:
        BTreeMap<String, BTreeMap<String, BTreeMap<String, super::moves::PunishEvidence>>>,
    #[serde(default)]
    pub move_bandit: super::bandit::BanditMemory,
    #[serde(default)]
    pub attack_learning: super::adaptation::AttackMemory,
    #[serde(default)]
    pub bandit: super::bandit::BanditMemory,
    #[serde(default)]
    pub shared_actions: std::collections::BTreeSet<String>,
    #[serde(default)]
    pub phase_actions: BTreeMap<String, CountMap>,
    pub version: u32,
    /// The enemy (`c1010`).
    pub chr: String,
    /// Fights recorded, finished or not.
    #[serde(default)]
    pub fights: u32,
    #[serde(default)]
    pub boss_wins: u32,
    #[serde(default)]
    pub wolf_wins: u32,
    /// The tally of every fight, per boss action id and per Wolf attack.
    #[serde(default)]
    pub boss: CountMap,
    #[serde(default)]
    pub wolf: CountMap,
    #[serde(default)]
    pub habits: Habits,
    /// The last fights, oldest first (the score to beat: `metrics`).
    #[serde(default)]
    pub history: Vec<FightRecord>,
    /// The coach's newest game plan and the outcome of every plan text it ran.
    #[serde(default, skip_serializing_if = "CoachMemory::is_empty")]
    pub coach: CoachMemory,
}

/// What the coach mode keeps across sessions (old files lack it).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CoachMemory {
    /// Phase:move -> hit/miss [sample count, summed first-window distance].
    #[serde(default)]
    pub contact_ranges: BTreeMap<String, [[f32; 2]; 2]>,
    #[serde(default)]
    pub hypotheses: Vec<super::digest::Hypothesis>,
    #[serde(default)]
    pub last_review_fight: u32,
    #[serde(default)]
    pub move_progress: BTreeMap<String, [u32; 2]>,
    #[serde(default)]
    pub lessons: Vec<String>,
    #[serde(default)]
    pub phase_bans: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub phase_plans: BTreeMap<String, GamePlan>,
    #[serde(default)]
    pub break_plans: CountMap,
    /// The newest validated game plan (its `notes` are the coach's hypotheses).
    #[serde(default)]
    pub plan: Option<GamePlan>,
    /// Outcome counts per plan text over every fight: `[exchanges, hit, blocked, deflected,
    /// whiffed, punished]`.
    #[serde(default)]
    pub plans: CountMap,
    #[serde(default)]
    pub failures: std::collections::BTreeMap<String, PlanFailures>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct PlanFailures {
    /// Net-bad attack uses, excluding fatal and fully deflected classifications.
    #[serde(default)]
    pub failed: u32,
    pub uses: u32,
    pub deflected_all: u32,
    pub fatal: u32,
}

impl PlanFailures {
    pub fn rate(&self) -> f32 {
        (self.deflected_all + self.fatal + self.failed) as f32 / self.uses.max(1) as f32
    }
}

impl CoachMemory {
    pub fn is_empty(&self) -> bool {
        self.phase_plans.is_empty()
            && self.contact_ranges.is_empty()
            && self.last_review_fight == 0
            && self.hypotheses.is_empty()
            && self.lessons.is_empty()
            && self.move_progress.is_empty()
            && self.phase_bans.is_empty()
            && self.plan.is_none()
            && self.plans.is_empty()
            && self.break_plans.is_empty()
            && self.failures.is_empty()
    }
}

/// How a load went.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Loaded {
    /// Read from the file.
    Read,
    /// No file yet.
    Missing,
    /// The file could not be used (the reason); it was moved aside.
    Corrupt(String),
}

/// [`Memory`] as a controller sees it.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct MemorySummary {
    /// Earlier fights against this enemy, and who won them (the rest were left unfinished).
    pub fights: u32,
    pub boss_wins: u32,
    pub wolf_wins: u32,
    /// The tally over every fight, this one included, most used first.
    pub boss: TopMap<Counts>,
    pub wolf: TopMap<Counts>,
    pub habits: HabitSummary,
    /// Wolf's play over the last fights and this one so far.
    #[serde(skip_serializing_if = "Trend::is_empty")]
    pub trend: Trend,
}

impl MemorySummary {
    /// Drops one item to shrink a request (see `BrainRequest::fit`); true if something went.
    pub fn trim(&mut self, keep: usize) -> bool {
        self.boss.pop(keep) || self.wolf.pop(keep) || self.habits.trim(keep) || self.trend.trim()
    }
}

impl Memory {
    pub fn new(chr: &str) -> Self {
        Self {
            version: MEMORY_VERSION,
            punishes: Default::default(),
            move_bandit: Default::default(),
            attack_learning: Default::default(),
            bandit: Default::default(),
            chr: chr.to_string(),
            fights: 0,
            boss_wins: 0,
            wolf_wins: 0,
            boss: CountMap::new(),
            shared_actions: Default::default(),
            phase_actions: BTreeMap::new(),
            wolf: CountMap::new(),
            habits: Habits::default(),
            history: Vec::new(),
            coach: CoachMemory::default(),
        }
    }

    fn migrate(&mut self) {
        let incoming: Vec<_> = self
            .coach
            .lessons
            .iter()
            .map(|s| s.trim().chars().take(120).collect::<String>())
            .filter(|s| !s.is_empty())
            .collect();
        self.coach.lessons.clear();
        super::digest::remember(&mut self.coach.lessons, &incoming);
        let excess = self.coach.hypotheses.len().saturating_sub(6);
        self.coach.hypotheses.drain(..excess);
        if self.version < 4 {
            super::adaptation::seed(&mut self.attack_learning, &self.chr, &self.boss);
            for (phase, counts) in &self.phase_actions {
                super::adaptation::seed(&mut self.attack_learning, phase, counts);
            }
            for (phase, bans) in &mut self.coach.phase_bans {
                let plan = self.coach.phase_plans.get(phase).or(if phase == &self.chr {
                    self.coach.plan.as_ref()
                } else {
                    None
                });
                bans.retain(|choice| {
                    plan.is_some_and(|p| p.avoid.iter().any(|a| &a.choice == choice))
                });
            }
        }
        // Version 6 adds punish outcomes; serde defaults old profiles to empty evidence.
        // Never infer punish success from historical aggregate hit counts.
        self.version = MEMORY_VERSION;
    }

    /// Reads the memory for `chr` from `path`; starts fresh when it is missing or unusable,
    /// moving an unusable file aside so the next save does not destroy it.
    pub fn load(path: &Path, chr: &str) -> (Self, Loaded) {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return (Self::new(chr), Loaded::Missing);
            }
            Err(e) => return (Self::new(chr), Loaded::Corrupt(e.to_string())),
        };
        let problem = match serde_json::from_str::<Memory>(&text) {
            Ok(m) if !(OLDEST_VERSION..=MEMORY_VERSION).contains(&m.version) => format!(
                "format version {} (this build reads {OLDEST_VERSION} to {MEMORY_VERSION})",
                m.version
            ),
            Ok(m) if m.chr != chr => format!("memory of {} (wanted {chr})", m.chr),
            Ok(mut m) => {
                // Older files default missing evidence and are saved as the current version.
                m.migrate();
                return (m, Loaded::Read);
            }
            Err(e) => format!("bad JSON: {e}"),
        };
        let mut aside = path.as_os_str().to_owned();
        aside.push(".corrupt");
        let _ = std::fs::rename(path, PathBuf::from(aside));
        (Self::new(chr), Loaded::Corrupt(problem))
    }

    /// Writes the file (through a temporary file, so a crash never leaves half a file).
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir)?;
        }
        let mut tmp = path.as_os_str().to_owned();
        tmp.push(".tmp");
        let tmp = PathBuf::from(tmp);
        {
            let mut f = std::fs::File::create(&tmp)?;
            serde_json::to_writer_pretty(&mut f, self)?;
            f.write_all(b"\n")?;
            f.sync_all()?;
        }
        std::fs::rename(&tmp, path)
    }

    /// Adds one fight: its tally (already flushed) and Wolf's habits in it.
    pub fn add_fight(&mut self, tally: &Tally, habits: &Habits, end: FightEnd) {
        self.fights += 1;
        match end {
            FightEnd::BossWon => self.boss_wins += 1,
            FightEnd::WolfWon => self.wolf_wins += 1,
            FightEnd::Unfinished => {}
        }
        self.add_data(tally, habits);
    }

    /// Adds a fight's tally and habits without counting it as a fight.
    pub fn add_data(&mut self, tally: &Tally, habits: &Habits) {
        self.shared_actions.extend(tally.shared.iter().cloned());
        for (phase, actions) in &tally.phases {
            if phase == "c7110" {
                for (key, counts) in actions
                    .iter()
                    .filter(|(key, _)| tally.shared.contains(*key))
                {
                    self.boss.entry(key.clone()).or_default().merge(counts);
                }
                merge_counts(
                    self.phase_actions.entry(phase.clone()).or_default(),
                    actions,
                );
            } else {
                merge_counts(&mut self.boss, actions);
            }
        }
        merge_counts(&mut self.wolf, tally.all(Side::Wolf));
        self.habits.merge(habits);
    }

    /// Adds a finished fight's record, keeping the last [`HISTORY`].
    pub fn push_record(&mut self, record: FightRecord) {
        self.history.push(record);
        let extra = self.history.len().saturating_sub(HISTORY);
        self.history.drain(..extra);
    }

    pub fn summary(&self) -> MemorySummary {
        self.summary_with(None)
    }

    /// The summary with this fight's numbers so far in the trend.
    pub fn summary_with(&self, now: Option<&FightMetrics>) -> MemorySummary {
        MemorySummary {
            fights: self.fights,
            boss_wins: self.boss_wins,
            wolf_wins: self.wolf_wins,
            boss: ranked_counts(&self.boss),
            wolf: ranked_counts(&self.wolf),
            habits: self.habits.summary(),
            trend: Trend::new(&self.history, now),
        }
    }
}

/// The default memory file for an enemy: `logs/memory/<chr>.json`.
pub fn default_path(chr: &str) -> PathBuf {
    Path::new(MEMORY_DIR).join(format!("{chr}.json"))
}

/// The memory file from `--brain-memory`: `off` none, `on` the default, a path ending in
/// `.json` that file, any other path a folder holding `<chr>.json`. Without the flag, hosted
/// brains remember (`hosted`) and the others do not.
pub fn resolve(arg: Option<&str>, chr: &str, hosted: bool) -> Option<PathBuf> {
    resolve_for(arg, chr, "", hosted)
}

/// Like [`resolve`], knowing the brain: a hosted brain keeps `<chr>.json`; any other brain
/// (`lua`, `scripted`) keeps its own `<chr>.<brain>.json`, so baselines and model runs never
/// mix, and remembers only when asked.
pub fn resolve_for(arg: Option<&str>, chr: &str, brain: &str, hosted: bool) -> Option<PathBuf> {
    let chr = if chr == "c7110" { "c7100" } else { chr };
    let file = if hosted || brain.is_empty() {
        format!("{chr}.json")
    } else {
        format!("{chr}.{brain}.json")
    };
    match arg {
        None => hosted.then(|| Path::new(MEMORY_DIR).join(&file)),
        Some(a) if a.eq_ignore_ascii_case("off") => None,
        Some(a) if a.eq_ignore_ascii_case("on") => Some(Path::new(MEMORY_DIR).join(&file)),
        Some(a) if a.to_ascii_lowercase().ends_with(".json") => Some(PathBuf::from(a)),
        Some(a) => Some(Path::new(a).join(&file)),
    }
}

/// Shared evidence contains no player habits, history or notes.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BossEvidence {
    pub move_bandit: super::bandit::BanditMemory,
    #[serde(default)]
    pub attack_learning: super::adaptation::AttackMemory,
    #[serde(default)]
    pub bandit: super::bandit::BanditMemory,
    pub phase_actions: BTreeMap<String, CountMap>,
    pub fights: u32,
    pub boss_wins: u32,
    pub wolf_wins: u32,
    pub actions: CountMap,
    pub plans: CountMap,
    pub break_plans: CountMap,
}

impl BossEvidence {
    fn from_player(m: &Memory) -> Self {
        Self {
            bandit: m.bandit.clone(),
            move_bandit: m.move_bandit.clone(),
            attack_learning: m.attack_learning.clone(),
            fights: m.fights,
            boss_wins: m.boss_wins,
            wolf_wins: m.wolf_wins,
            actions: m.boss.clone(),
            phase_actions: m.phase_actions.clone(),
            plans: m.coach.plans.clone(),
            break_plans: m.coach.break_plans.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BestPlan {
    pub plan: GamePlan,
    pub results: BossEvidence,
    pub score: f32,
    #[serde(default)]
    pub last_result: Option<FightRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BossKnowledge {
    pub version: u32,
    pub chr: String,
    pub evidence: BossEvidence,
    pub players: BTreeMap<String, BossEvidence>,
    pub principles: Vec<String>,
    pub bans: Vec<Avoid>,
    pub best: Option<BestPlan>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct BossSummary {
    pub fights: u32,
    pub players: usize,
    pub actions: TopMap<Counts>,
    pub plans: TopMap<Counts>,
    pub break_plans: TopMap<Counts>,
    pub principles: Vec<String>,
    pub bans: Vec<Avoid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub best: Option<BestSummary>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct BestSummary {
    pub opener: String,
    pub sets: BTreeMap<String, Vec<super::coach::WeightedChoice>>,
    pub start_set: String,
    pub on_wolf_break: Vec<super::coach::WeightedChoice>,
    /// Last result: [boss won, seconds, Wolf damage/min, break conversion rate].
    pub result: Option<[f32; 4]>,
}

impl BossSummary {
    pub fn trim(&mut self) -> bool {
        self.actions.pop(4) || self.plans.pop(4) || self.break_plans.pop(3)
    }
    pub fn trim_last(&mut self) -> bool {
        self.actions.pop(0)
            || self.plans.pop(0)
            || self.break_plans.pop(0)
            || self.best.take().is_some()
            || self.principles.pop().is_some()
            || self.bans.pop().is_some()
    }
}

impl BossKnowledge {
    pub fn new(chr: &str) -> Self {
        Self {
            version: 1,
            chr: chr.into(),
            evidence: BossEvidence::default(),
            players: BTreeMap::new(),
            principles: Vec::new(),
            bans: Vec::new(),
            best: None,
        }
    }
    pub fn update(&mut self, player: &str, memory: &Memory) {
        self.players
            .insert(player.into(), BossEvidence::from_player(memory));
        self.evidence = BossEvidence::default();
        for e in self.players.values() {
            super::bandit::merge(&mut self.evidence.move_bandit, &e.move_bandit, 1.0);
            for (phase, actions) in &e.attack_learning {
                for (key, evidence) in actions {
                    self.evidence
                        .attack_learning
                        .entry(phase.clone())
                        .or_default()
                        .entry(key.clone())
                        .or_default()
                        .pool(*evidence);
                }
            }
            for (phase, contexts) in &e.bandit {
                for (context, arms) in contexts {
                    for (name, arm) in arms {
                        let shared = self
                            .evidence
                            .bandit
                            .entry(phase.clone())
                            .or_default()
                            .entry(context.clone())
                            .or_default()
                            .entry(name.clone())
                            .or_default();
                        shared.success += arm.success;
                        shared.failure += arm.failure;
                    }
                }
            }
            for (phase, actions) in &e.phase_actions {
                merge_counts(
                    self.evidence
                        .phase_actions
                        .entry(phase.clone())
                        .or_default(),
                    actions,
                );
            }
            self.evidence.fights += e.fights;
            self.evidence.boss_wins += e.boss_wins;
            self.evidence.wolf_wins += e.wolf_wins;
            merge_counts(&mut self.evidence.actions, &e.actions);
            merge_counts(&mut self.evidence.plans, &e.plans);
            merge_counts(&mut self.evidence.break_plans, &e.break_plans);
        }
        // Outcomes inform rates and sampling. Only a reasoned coach reply creates a ban.
        self.bans.retain(|ban| !ban.reason.starts_with("shared: "));
    }
    pub fn learn(&mut self, plan: &GamePlan) {
        for principle in plan.boss_principles.iter().rev() {
            self.principles
                .retain(|p| !p.eq_ignore_ascii_case(principle));
            self.principles.insert(0, principle.clone());
        }
        self.principles.truncate(6);
        for ban in &plan.boss_bans {
            self.bans.retain(|b| b.choice != ban.choice);
            self.bans.push(ban.clone());
        }
    }
    pub fn consider(&mut self, plan: &GamePlan, memory: &Memory) {
        if !plan.usable() {
            return;
        }
        let score = memory.history.last().map_or(0.0, |r| {
            let m = &r.metrics;
            if m.void || m.stalled() {
                -1.0
            } else {
                f32::from(r.winner == "boss") * 100000.0
                    + if r.winner == "boss" {
                        10000.0 / m.duration.max(1.0)
                    } else {
                        0.0
                    }
                    + m.break_punish_rate * 100.0
                    + (m.damage_per_min() - m.boss_damage_taken_per_min()).clamp(-100.0, 100.0)
            }
        });
        if score < 0.0 {
            return;
        }
        if self.best.as_ref().is_none_or(|b| score >= b.score) {
            let mut shared = plan.clone();
            shared.notes.clear();
            shared.principles.clear();
            shared.avoid.clear();
            shared.retry.clear();
            for set in &mut shared.sets {
                set.description = format!("shared {} attack set", set.name);
            }
            shared.boss_principles = self.principles.clone();
            self.best = Some(BestPlan {
                plan: shared,
                results: BossEvidence::from_player(memory),
                score,
                last_result: memory.history.last().cloned(),
            });
        }
    }
    pub fn starter(&self) -> Option<GamePlan> {
        (self.evidence.fights >= 20)
            .then(|| self.best.as_ref().map(|b| b.plan.clone()))
            .flatten()
    }
    pub fn summary(&self) -> BossSummary {
        BossSummary {
            fights: self.evidence.fights,
            players: self.players.values().filter(|e| e.fights > 0).count(),
            actions: ranked_counts(&self.evidence.actions),
            plans: ranked_counts(&self.evidence.plans),
            break_plans: ranked_counts(&self.evidence.break_plans),
            principles: self.principles.clone(),
            bans: self.bans.clone(),
            best: self.best.as_ref().map(|b| BestSummary {
                opener: b.plan.opener.clone(),
                sets: b
                    .plan
                    .sets
                    .iter()
                    .map(|s| (s.name.clone(), s.plans.clone()))
                    .collect(),
                start_set: b.plan.start_set.clone(),
                on_wolf_break: b.plan.on_wolf_break.clone(),
                result: b.last_result.as_ref().map(|r| {
                    [
                        f32::from(r.winner == "boss"),
                        r.metrics.duration,
                        r.metrics.damage_per_min(),
                        r.metrics.break_punish_rate,
                    ]
                }),
            }),
        }
    }
}

/// Validated profile names cannot escape the memory directory.
pub fn player_name(name: &str) -> Result<String, String> {
    if name.is_empty()
        || name.len() > 48
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        Err("--player requires 1 to 48 letters, digits, underscores or hyphens".into())
    } else {
        Ok(name.into())
    }
}

/// Both files are replaced atomically individually; contribution snapshots make retries idempotent.
#[derive(Debug, Clone)]
pub struct LayerPaths {
    pub boss: PathBuf,
    pub player: PathBuf,
    pub legacy: PathBuf,
    pub backup: PathBuf,
}

impl LayerPaths {
    pub fn new(legacy: &Path, name: &str) -> Self {
        let stem = legacy.file_stem().unwrap_or_default().to_string_lossy();
        Self {
            boss: legacy.with_file_name(format!("{stem}.boss.json")),
            player: legacy.with_file_name(format!("{stem}.player-{name}.json")),
            backup: legacy.with_file_name(format!("{stem}.v2-backup.json")),
            legacy: legacy.into(),
        }
    }
    pub fn load(&self, chr: &str) -> std::io::Result<(BossKnowledge, Memory)> {
        let mut boss = match std::fs::read_to_string(&self.boss) {
            Ok(text) => {
                let b: BossKnowledge = serde_json::from_str(&text)?;
                if b.version != 1 || b.chr != chr {
                    return Err(std::io::Error::other("incompatible boss knowledge"));
                }
                b
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => BossKnowledge::new(chr),
            Err(e) => return Err(e),
        };
        let default = Self::new(&self.legacy, "default");
        if !default.player.exists() && self.legacy.exists() {
            // Never rename/delete the source or overwrite an existing backup.
            let text = std::fs::read_to_string(&self.legacy)?;
            let mut m: Memory = serde_json::from_str(&text)?;
            if !(OLDEST_VERSION..=MEMORY_VERSION).contains(&m.version) || m.chr != chr {
                return Err(std::io::Error::other("incompatible legacy memory"));
            }
            if !self.backup.exists() {
                std::fs::copy(&self.legacy, &self.backup)?;
            }
            m.migrate();
            boss.update("default", &m);
            if let Some(p) = &m.coach.plan {
                boss.learn(p);
                boss.consider(p, &m);
            }
            write_json(&self.boss, &boss)?;
            m.save(&default.player)?;
        }
        let mut player = match std::fs::read_to_string(&self.player) {
            Ok(text) => {
                let m: Memory = serde_json::from_str(&text)?;
                if m.chr != chr || !(OLDEST_VERSION..=MEMORY_VERSION).contains(&m.version) {
                    return Err(std::io::Error::other("incompatible player profile"));
                }
                m
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Memory::new(chr),
            Err(e) => return Err(e),
        };
        player.migrate();
        // Repair a interrupted two-file save using the durable profile snapshot.
        let name = self
            .player
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy();
        if let Some((_, name)) = name.rsplit_once(".player-") {
            boss.update(name, &player);
        }
        Ok((boss, player))
    }
    pub fn save(&self, boss: &BossKnowledge, player: &Memory) -> std::io::Result<BossKnowledge> {
        player.save(&self.player)?;
        // Another profile may have been saved since this driver loaded.
        let mut combined = match std::fs::read_to_string(&self.boss) {
            Ok(text) => serde_json::from_str::<BossKnowledge>(&text)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => boss.clone(),
            Err(e) => return Err(e),
        };
        if combined.version != boss.version || combined.chr != boss.chr {
            return Err(std::io::Error::other("incompatible boss knowledge"));
        }
        let name = self
            .player
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy();
        if let Some((_, name)) = name.rsplit_once(".player-") {
            let snapshot = boss.players.get(name).cloned().unwrap_or_default();
            combined.players.insert(name.into(), snapshot);
            // Aggregate the snapshots without importing player habits or provisional fights.
            let snapshot = combined.players.get(name).cloned().unwrap_or_default();
            let mut evidence = Memory::new(&boss.chr);
            evidence.bandit = snapshot.bandit;
            evidence.move_bandit = snapshot.move_bandit;
            evidence.phase_actions = snapshot.phase_actions;
            evidence.fights = snapshot.fights;
            evidence.boss_wins = snapshot.boss_wins;
            evidence.wolf_wins = snapshot.wolf_wins;
            evidence.boss = snapshot.actions;
            evidence.coach.plans = snapshot.plans;
            evidence.coach.break_plans = snapshot.break_plans;
            combined.update(name, &evidence);
        }
        combined.learn(&GamePlan {
            boss_principles: boss.principles.clone(),
            boss_bans: boss.bans.clone(),
            ..Default::default()
        });
        if boss.best.as_ref().is_some_and(|b| {
            combined
                .best
                .as_ref()
                .is_none_or(|old| b.score >= old.score)
        }) {
            combined.best = boss.best.clone();
        }
        write_json(&self.boss, &combined)?;
        Ok(combined)
    }
}

fn write_json(path: &Path, value: &impl Serialize) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    let mut file = std::fs::File::create(&tmp)?;
    serde_json::to_writer_pretty(&mut file, value)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    std::fs::rename(tmp, path)
}
