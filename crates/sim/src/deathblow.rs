//! Deathblows: the paired "throw" that kills a posture-broken or unaware enemy.
//!
//! See `docs/DEATHBLOW.md` for the data trail. In short:
//! - ThrowParam rows pair an attacker animation with a defender animation. A deathblow is two
//!   rows: a *start* row (the attacker lunges, no defender animation) and a *body* row (both
//!   play, the defender snapped to a dummy poly on the attacker).
//! - The start row's range and angles decide whether the deathblow can begin. The body row is
//!   checked when the start animation's grab event fires.
//! - The defender plays `defAnimId`, or `defAnimId + 1` when the blow kills (the behaviour
//!   graph's `ThrowDefDeath<id>` states). The damage lands on the attacker's
//!   `ThrowAttackBehavior` TAE event.
//!
//! Space is the simulation's world space (Bevy: right-handed, +Y up). A [`Placement`] with yaw 0
//! faces -Z and positive yaw turns counter-clockwise seen from above, as in [`crate::body`].

use std::path::Path;
use std::sync::Arc;

use sekiro_formats::param::{RawParam, Row, Table};
use sekiro_formats::paramdef::ParamDef;
use sekiro_formats::{Result, flver};

use crate::input::angle_diff;
use crate::tae::{TaeDb, TaeKind};

/// One ThrowParam row, with the fields the deathblow needs.
#[derive(Debug, Clone, PartialEq)]
pub struct ThrowRow {
    pub id: i32,
    pub atk_chr: i32,
    pub def_chr: i32,
    /// Maximum horizontal distance between the two characters, metres.
    pub dist: f32,
    /// Allowed angle, degrees, between the defender's back direction and the direction from
    /// the defender to the attacker: 90..180 means "in front", 0..90 "behind".
    pub diff_ang_min: f32,
    pub diff_ang_max: f32,
    /// Allowed height of the defender above / below the attacker.
    pub upper_y: f32,
    pub lower_y: f32,
    /// Maximum angle, degrees, between the attacker's facing and the direction to the defender.
    pub diff_ang_my_to_def: f32,
    pub atk_anim: i32,
    pub def_anim: i32,
    pub atk_anim_offset: i32,
    pub def_anim_offset: i32,
    /// Attacker dummy poly the defender is snapped to (0: none).
    pub atk_sorb_dummy: i16,
    pub def_sorb_dummy: i16,
    pub throw_kind: i64,
    /// Turn the attacker to face the defender when the row starts.
    pub turn_attacker: bool,
    /// Seconds over which the defender slides onto the sorb dummy.
    pub interpolation_time: f32,
}

impl ThrowRow {
    pub fn from_row(row: &Row) -> Result<Self> {
        Ok(Self {
            id: row.id(),
            atk_chr: row.i32("AtkChrId")?,
            def_chr: row.i32("DefChrId")?,
            dist: row.f32("Dist")?,
            diff_ang_min: row.f32("DiffAngMin")?,
            diff_ang_max: row.f32("DiffAngMax")?,
            upper_y: row.f32("upperYRange")?,
            lower_y: row.f32("lowerYRange")?,
            diff_ang_my_to_def: row.f32("diffAngMyToDef")?,
            atk_anim: row.i32("atkAnimId")?,
            def_anim: row.i32("defAnimId")?,
            atk_anim_offset: row.int("atkAnimOffset")? as i32,
            def_anim_offset: row.int("defAnimOffset")? as i32,
            atk_sorb_dummy: row.int("atkSorbDmyId")? as i16,
            def_sorb_dummy: row.int("defSorbDmyId")? as i16,
            throw_kind: row.int("throwKind")?,
            turn_attacker: row.int("isTurnAtker")? != 0,
            interpolation_time: row.f32("adsrobModelPosInterpolationTime")?,
        })
    }

    /// The attacker's clip, e.g. `a200_510000`.
    pub fn attacker_anim(&self) -> String {
        format!("a{:03}_{:06}", self.atk_anim_offset, self.atk_anim)
    }

    /// The defender's clip when it survives, e.g. `a000_012000`.
    pub fn defender_anim(&self) -> Option<String> {
        (self.def_anim > 0).then(|| format!("a{:03}_{:06}", self.def_anim_offset, self.def_anim))
    }

    /// The defender's clip when the blow kills: the graph's `ThrowDefDeath<defAnimId + 1>`.
    pub fn defender_death_anim(&self) -> Option<String> {
        (self.def_anim > 0)
            .then(|| format!("a{:03}_{:06}", self.def_anim_offset, self.def_anim + 1))
    }

    /// Whether the two characters satisfy this row's distance, height and angle limits.
    pub fn in_range(&self, attacker: &Placement, defender: &Placement) -> bool {
        let dx = attacker.position[0] - defender.position[0];
        let dz = attacker.position[2] - defender.position[2];
        let dist = (dx * dx + dz * dz).sqrt();
        if dist > self.dist {
            return false;
        }
        let dy = defender.position[1] - attacker.position[1];
        if dy > self.upper_y || dy < -self.lower_y {
            return false;
        }
        if dist < 1e-4 {
            return true;
        }
        let to_attacker = [dx / dist, dz / dist];
        let [fx, fz] = defender.forward();
        let back = [-fx, -fz];
        let def_angle = angle_between(back, to_attacker);
        if def_angle < self.diff_ang_min - 1e-3 || def_angle > self.diff_ang_max + 1e-3 {
            return false;
        }
        let my_angle = angle_between(attacker.forward(), [-to_attacker[0], -to_attacker[1]]);
        my_angle <= self.diff_ang_my_to_def + 1e-3
    }
}

/// Angle in degrees between two unit 2D vectors.
fn angle_between(a: [f32; 2], b: [f32; 2]) -> f32 {
    (a[0] * b[0] + a[1] * b[1])
        .clamp(-1.0, 1.0)
        .acos()
        .to_degrees()
}

/// ThrowParam, loaded from the extracted gameparam folder.
pub struct ThrowTable {
    pub rows: Vec<ThrowRow>,
}

impl ThrowTable {
    pub fn load(param_dir: &Path, defs_dir: &Path) -> Result<Self> {
        let raw = RawParam::parse(std::fs::read(param_dir.join("ThrowParam.param"))?)?;
        let def = ParamDef::load(&defs_dir.join("ThrowParam.xml"))?;
        let table = Table::new("ThrowParam", &raw, Arc::new(def), None)?;
        let rows = table
            .rows()
            .map(|r| ThrowRow::from_row(&r))
            .collect::<Result<_>>()?;
        Ok(Self { rows })
    }

    pub fn load_from_cache(cache: &Path) -> Result<Self> {
        Self::load(
            &cache.join("raw/param/gameparam/gameparam.parambnd.d"),
            &cache.join("refs/paramdex/Defs"),
        )
    }

    /// The player-vs-`def_chr` row with the given suffix (`id % 1000`) and variation digit
    /// (`id / 1000 % 10`; 0 is the standard enemy, others are "Varier" and tutorial copies).
    pub fn player_row(&self, def_chr: i32, variation: i32, suffix: i32) -> Option<&ThrowRow> {
        self.rows.iter().find(|r| {
            r.atk_chr == 0
                && r.def_chr == def_chr
                && r.id.rem_euclid(1000) == suffix
                && (r.id / 1000).rem_euclid(10) == variation
        })
    }

    /// The row an NPC's throw start begins against the player: `AtkChrId` is the NPC's model
    /// number, `DefChrId` 0, and the id is `20000000 + chr * 1000 + throwTypeId * 100` (AtkParam
    /// `throwTypeId`: the Owl's 800 has 0 and begins 25060000, his 810 has 1 and begins 25060100).
    /// That id rule is inferred from those rows; when no row has the id, the NPC's rows against
    /// the player are taken in id order.
    pub fn npc_row(&self, atk_chr: i32, throw_type_id: i32) -> Option<&ThrowRow> {
        let id = 20_000_000 + atk_chr * 1000 + throw_type_id * 100;
        self.rows
            .iter()
            .find(|r| r.id == id && r.atk_chr == atk_chr && r.def_chr == 0)
            .or_else(|| {
                let mut rows: Vec<&ThrowRow> = self
                    .rows
                    .iter()
                    .filter(|r| r.atk_chr == atk_chr && r.def_chr == 0)
                    .collect();
                rows.sort_by_key(|r| r.id);
                rows.get(usize::try_from(throw_type_id).ok()?).copied()
            })
    }
}

/// Why the enemy can be deathblown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Situation {
    /// Posture broken (the TrunkCollapse stagger): front or back deathblow.
    PostureBroken,
    /// Not in combat with the player: the stealth deathblow from behind.
    Unaware,
    /// Posture broken on the last deathblow mark: bosses with a "todome" pair (suffixes 180 and
    /// 181, such as the Great Shinobi Owl's 15060180/15060181) are finished with it. Which pair
    /// the engine picks is inferred: the todome defender clip is a death state in the shared NPC
    /// script (`ThrowDef13700` sets `STATE_DEATH`), the ordinary one gets up again.
    Final,
}

/// ThrowParam row suffixes (`id % 1000`) of the standard player-vs-enemy deathblows, as
/// `(start, body)` pairs in the order they are tried.
pub fn candidate_pairs(situation: Situation) -> &'static [(i32, i32)] {
    match situation {
        // The finishing blow (トドメ, "todome") of multi-life bosses, then the ordinary ones.
        Situation::Final => &[(180, 181), (5, 6), (0, 1), (110, 111)],
        // Near front, far front, then from behind a broken enemy.
        Situation::PostureBroken => &[(5, 6), (0, 1), (110, 111)],
        // Stealth deathblow from behind.
        Situation::Unaware => &[(20, 21)],
    }
}

/// Picks the deathblow rows whose start row the current positions satisfy.
pub fn select<'a>(
    table: &'a ThrowTable,
    def_chr: i32,
    variation: i32,
    situation: Situation,
    attacker: &Placement,
    defender: &Placement,
) -> Option<(&'a ThrowRow, &'a ThrowRow)> {
    candidate_pairs(situation).iter().find_map(|&(s, b)| {
        let start = table.player_row(def_chr, variation, s)?;
        let body = table.player_row(def_chr, variation, b)?;
        start.in_range(attacker, defender).then_some((start, body))
    })
}

/// Where a character stands and which way it faces.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Placement {
    pub position: [f32; 3],
    pub yaw: f32,
}

impl Placement {
    pub fn forward(&self) -> [f32; 2] {
        [-self.yaw.sin(), -self.yaw.cos()]
    }

    /// A point given in this character's local space (-Z forward) in world space.
    pub fn transform(&self, local: [f32; 3]) -> [f32; 3] {
        let (s, c) = self.yaw.sin_cos();
        [
            self.position[0] + local[0] * c + local[2] * s,
            self.position[1] + local[1],
            self.position[2] - local[0] * s + local[2] * c,
        ]
    }

    /// Yaw that faces `target` from here.
    pub fn yaw_towards(&self, target: [f32; 3]) -> f32 {
        let dx = target[0] - self.position[0];
        let dz = target[2] - self.position[2];
        (-dx).atan2(-dz)
    }
}

/// A dummy poly as a frame in its character's local space (world convention, mirrored from
/// FromSoftware space).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DummyFrame {
    pub position: [f32; 3],
    /// Yaw of a character standing on the dummy and facing along its forward vector.
    pub yaw: f32,
}

impl DummyFrame {
    /// Reads dummy `reference_id` from a FLVER. Throw dummies hang off root-level nodes with no
    /// attach bone, so they are fixed in the character's frame; the parent node's translation is
    /// applied and its rotation (identity in c0000) ignored.
    pub fn from_flver(model: &flver::Flver, reference_id: i16) -> Option<Self> {
        let d = model
            .dummies
            .iter()
            .find(|d| d.reference_id == reference_id)?;
        let offset = usize::try_from(d.parent_bone)
            .ok()
            .and_then(|p| model.nodes.get(p))
            .map_or([0.0; 3], |n| n.translation);
        let p = [
            d.position[0] + offset[0],
            d.position[1] + offset[1],
            d.position[2] + offset[2],
        ];
        // Mirror X into world space.
        let (fx, fz) = (-d.forward[0], d.forward[2]);
        Some(Self {
            position: [-p[0], p[1], p[2]],
            yaw: (-fx).atan2(-fz),
        })
    }

    /// The same relation seen from the other side: for a dummy of the defender that the attacker
    /// is snapped to (ThrowParam `defSorbDmyId` with `atkSorbDmyId` 0), the frame of the defender
    /// in the attacker's space.
    pub fn inverse(&self) -> Self {
        let (s, c) = self.yaw.sin_cos();
        let [x, y, z] = self.position;
        // Undo the yaw rotation of `Placement::transform`, then negate.
        Self {
            position: [-(x * c - z * s), -y, -(x * s + z * c)],
            yaw: -self.yaw,
        }
    }

    /// Where a character snapped to this dummy of `owner` stands.
    pub fn placement(&self, owner: &Placement) -> Placement {
        Placement {
            position: owner.transform(self.position),
            yaw: owner.yaw + self.yaw,
        }
    }
}

/// Times of the throw events in the attacker's clips, from `cache/anim/<chr>/tae.json`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThrowTimes {
    /// The grab check in the start clip (its first throw `CommonBehavior` event).
    pub grab: (f32, f32),
    /// When the damage lands in the body clip (`ThrowAttackBehavior`).
    pub damage: f32,
    /// Time in the defender's survive clip where a lethal blow branches to the death clip
    /// (its one-frame `ChrActionFlag` 69 event; the death clip's first frame matches it).
    pub death_branch: Option<f32>,
}

impl ThrowTimes {
    pub fn from_tae_json(tae: &serde_json::Value, start: &str, body: &str) -> Option<Self> {
        let events = |name: &str| -> Option<Vec<(String, f32, f32)>> {
            let anim = tae["files"]
                .as_array()?
                .iter()
                .flat_map(|f| f["animations"].as_array().into_iter().flatten())
                .find(|a| a["name"] == name)?;
            Some(
                anim["events"]
                    .as_array()?
                    .iter()
                    .filter_map(|e| {
                        Some((
                            e["decoded"]["name"].as_str()?.to_string(),
                            e["start"].as_f64()? as f32,
                            e["end"].as_f64()? as f32,
                        ))
                    })
                    .collect(),
            )
        };
        let grab = events(start)?
            .into_iter()
            .filter(|(n, _, _)| n == "CommonBehavior")
            .map(|(_, s, e)| (s, e))
            .reduce(|a, b| if b.0 < a.0 { b } else { a })?;
        let damage = events(body)?
            .into_iter()
            .filter(|(n, _, _)| n == "ThrowAttackBehavior")
            .map(|(_, s, _)| s)
            .reduce(f32::min)?;
        Some(Self {
            grab,
            damage,
            death_branch: None,
        })
    }

    /// The same times from the characters' already decoded TAE (no file reads): the attacker's
    /// start and body clips and, given the defender's survive clip, its death branch.
    pub fn from_tae_db(
        attacker: &TaeDb,
        start: &str,
        body: &str,
        defender: Option<(&TaeDb, &str)>,
    ) -> Option<Self> {
        let grab = attacker
            .anim(start)?
            .events
            .iter()
            .filter(|e| matches!(e.kind, TaeKind::CommonBehavior { .. }))
            .map(|e| (e.start, e.end))
            .reduce(|a, b| if b.0 < a.0 { b } else { a })?;
        let damage = attacker
            .anim(body)?
            .events
            .iter()
            .filter(|e| matches!(e.kind, TaeKind::ThrowAttack { .. }))
            .map(|e| e.start)
            .reduce(f32::min)?;
        let death_branch = defender.and_then(|(db, clip)| {
            db.anim(clip)?
                .events
                .iter()
                .filter(|e| e.kind == TaeKind::ActionFlag(69))
                .map(|e| e.start)
                .reduce(f32::min)
        });
        Some(Self {
            grab,
            damage,
            death_branch,
        })
    }

    /// Loads the attacker's throw events and, given the defender's survive clip, its death
    /// branch time. Parses whole `tae.json` exports (about half a second for c0000); a running
    /// fight uses [`Self::from_tae_db`].
    pub fn load(
        cache: &Path,
        attacker_chr: &str,
        start: &str,
        body: &str,
        defender: Option<(&str, &str)>,
    ) -> Option<Self> {
        let read = |chr: &str| -> Option<serde_json::Value> {
            let text = std::fs::read_to_string(cache.join(format!("anim/{chr}/tae.json"))).ok()?;
            serde_json::from_str(&text).ok()
        };
        let mut times = Self::from_tae_json(&read(attacker_chr)?, start, body)?;
        if let Some((chr, clip)) = defender {
            times.death_branch = read(chr).and_then(|t| death_branch(&t, clip));
        }
        Some(times)
    }
}

/// The first `ChrActionFlag` 69 event in a defender throw clip.
pub fn death_branch(tae: &serde_json::Value, clip: &str) -> Option<f32> {
    let anim = tae["files"]
        .as_array()?
        .iter()
        .flat_map(|f| f["animations"].as_array().into_iter().flatten())
        .find(|a| a["name"] == clip)?;
    anim["events"]
        .as_array()?
        .iter()
        .filter(|e| e["decoded"]["name"] == "ChrActionFlag")
        .filter(|e| {
            e["decoded"]["fields"].as_array().is_some_and(|f| {
                f.iter()
                    .any(|x| x["name"] == "FlagType" && x["value"] == 69)
            })
        })
        .filter_map(|e| e["start"].as_f64().map(|s| s as f32))
        .reduce(f32::min)
}

/// How many deathblows an enemy takes: NpcParam `ninsatuNum`, where 0 (ordinary enemies) means
/// one. Inferred from the data; bosses carry 2 or more.
pub fn deathblow_marks(ninsatu_num: i32) -> i32 {
    ninsatu_num.max(1)
}

/// The most deathblow marks a health bar shows: the largest NpcParam `ninsatuNum` (3); the
/// Butterfly's encounter sums two 1-mark rows.
pub const MAX_BAR_MARKS: i32 = 3;

/// The deathblow marks a health bar draws: `left` of the `total` its encounter started with,
/// none for an enemy taking a single deathblow (the game draws them only on enemies that take
/// more than one).
pub fn bar_marks(left: i32, total: i32) -> i32 {
    if total > 1 {
        left.clamp(0, MAX_BAR_MARKS)
    } else {
        0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// The attacker's lunge; the defender keeps playing its stagger.
    Start,
    /// The paired animation, defender snapped to the attacker's sorb dummy.
    Body,
    /// Over. The defender keeps playing its clip, now under its own root motion.
    Done,
    /// The grab window passed without the body row's conditions holding.
    Missed,
}

/// Which of the pair the throw absorb moves onto the other's sorb dummy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Absorbed {
    /// The attacker stands on the defender's dummy (`atkSorbDmyId` 0, `defSorbDmyId` set: the
    /// Owl's rows). Which side moves in that case is inferred, not traced.
    Attacker,
    /// The defender stands on the attacker's dummy (`atkSorbDmyId` set: the soldier's rows).
    #[default]
    Defender,
}

/// This step's share of the absorb: how far to move the absorbed character (world metres, Y
/// ignored) and turn it (radians). The world moves it through its collision.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AbsorbStep {
    pub who: Absorbed,
    pub delta: [f32; 3],
    pub yaw: f32,
}

/// The absorb of a throw pair: the correction that puts the absorbed character on the other's
/// sorb dummy, worked out once from where the pair stands when the paired clips start (their
/// frame 0), handed out over `adsrobModelPosInterpolationTime`. Deathblows and enemy throws on
/// Wolf ([`crate::grab`]) share it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AbsorbSlide {
    pub who: Absorbed,
    offset: [f32; 3],
    yaw: f32,
    duration: f32,
    time: f32,
    applied: f32,
}

impl AbsorbSlide {
    /// `sorb` is the dummy of the attacker the defender stands on (for [`Absorbed::Attacker`],
    /// the defender's frame in the attacker's space, as [`DummyFrame::inverse`] gives it).
    pub fn new(
        who: Absorbed,
        sorb: &DummyFrame,
        attacker: &Placement,
        defender: &Placement,
        duration: f32,
    ) -> Self {
        let (from, target) = match who {
            Absorbed::Defender => (defender, sorb.placement(attacker)),
            Absorbed::Attacker => (attacker, sorb.inverse().placement(defender)),
        };
        Self {
            who,
            offset: [
                target.position[0] - from.position[0],
                0.0,
                target.position[2] - from.position[2],
            ],
            yaw: angle_diff(target.yaw, from.yaw),
            duration,
            time: 0.0,
            applied: 0.0,
        }
    }

    /// This step's share, or `None` once it is all handed out.
    pub fn with_vertical(mut self, from: &Placement, target: &Placement) -> Self {
        self.offset[1] = target.position[1] - from.position[1];
        self
    }

    /// This step's share, or `None` once it is all handed out.
    pub fn advance(&mut self, dt: f32) -> Option<AbsorbStep> {
        self.time += dt;
        let w = if self.duration > 0.0 {
            (self.time / self.duration).min(1.0)
        } else {
            1.0
        };
        if w <= self.applied {
            return None;
        }
        let share = w - self.applied;
        self.applied = w;
        Some(AbsorbStep {
            who: self.who,
            delta: self.offset.map(|v| v * share),
            yaw: self.yaw * share,
        })
    }
}

/// What changed this step.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DeathblowStep {
    /// Start this clip on the attacker.
    pub attacker_clip: Option<String>,
    /// Start this clip on the defender.
    pub defender_clip: Option<String>,
    /// Absorb motion to apply this step (Body phase, first `adsrobModelPosInterpolationTime`
    /// seconds only).
    pub absorb: Option<AbsorbStep>,
    /// The blow landed this step.
    pub hit: bool,
    /// The defender died this step.
    pub killed: bool,
}

/// One deathblow in progress.
///
/// Both characters play their paired clips under their own root motion. When the body starts,
/// the absorbed character (see [`Absorbed`]) is moved onto the other's sorb dummy as it stands
/// at the body clip's first frame, spread over `adsrobModelPosInterpolationTime`; after that
/// nobody is held to anybody, so the attacker's later motion (or input once its clip allows
/// moving) never drags the defender. From FwLo's RustSekiro (`docs/kb/damage.md`, matched
/// against live recordings): the absorb places the defender at the throw clip's frame 0, and
/// root motion runs in each clip's start frame.
#[derive(Debug, Clone)]
pub struct Deathblow {
    pub start: ThrowRow,
    pub body: ThrowRow,
    pub phase: Phase,
    /// Time in the current phase's attacker clip.
    pub time: f32,
    pub times: ThrowTimes,
    pub sorb: DummyFrame,
    /// Duration of the attacker's body clip.
    pub body_duration: f32,
    /// The blow kills (no deathblow marks left after it).
    pub lethal: bool,
    /// The defender has a separate death clip (`defAnimId + 1`) to branch to. Without one the
    /// body row's own defender clip is the death (the Owl's final blow `a000_013700`).
    pub death_clip: bool,
    /// Which character the absorb moves.
    pub absorbed: Absorbed,
    /// The absorb, from the body's start.
    slide: Option<AbsorbSlide>,
    hit: bool,
    branched: bool,
}

impl Deathblow {
    /// Begins a deathblow. Returns it with the first step, which starts the attacker's lunge
    /// and, for turning rows, faces it toward the defender.
    pub fn begin(
        rows: (&ThrowRow, &ThrowRow),
        times: ThrowTimes,
        sorb: DummyFrame,
        body_duration: f32,
        marks_left: i32,
    ) -> (Self, DeathblowStep) {
        let db = Self {
            start: rows.0.clone(),
            body: rows.1.clone(),
            phase: Phase::Start,
            time: 0.0,
            times,
            sorb,
            body_duration,
            lethal: marks_left <= 1,
            death_clip: true,
            absorbed: Absorbed::Defender,
            slide: None,
            hit: false,
            branched: false,
        };
        let step = DeathblowStep {
            attacker_clip: Some(db.start.attacker_anim()),
            ..DeathblowStep::default()
        };
        (db, step)
    }

    /// The yaw the attacker should take when the start row turns it toward the defender.
    pub fn start_facing(&self, attacker: &Placement, defender: &Placement) -> Option<f32> {
        self.start
            .turn_attacker
            .then(|| attacker.yaw_towards(defender.position))
    }

    /// The defender clip the body starts with: the survive clip, which a lethal blow leaves for
    /// the death clip at [`ThrowTimes::death_branch`] (straight away when that is unknown).
    pub fn defender_clip(&self) -> Option<String> {
        if self.lethal && self.death_clip && self.times.death_branch.is_none() {
            self.body.defender_death_anim()
        } else {
            self.body.defender_anim()
        }
    }

    /// A special-state row has a body clip but no separate grab-start animation.
    pub fn begin_body(
        row: &ThrowRow,
        times: ThrowTimes,
        sorb: DummyFrame,
        body_duration: f32,
        attacker: &Placement,
        defender: &Placement,
    ) -> (Self, DeathblowStep) {
        let (mut db, _) = Self::begin((row, row), times, sorb, body_duration, 1);
        db.phase = Phase::Body;
        db.death_clip = false;
        db.absorbed = Absorbed::Attacker;
        db.slide = Some(
            AbsorbSlide::new(
                db.absorbed,
                &db.sorb,
                attacker,
                defender,
                row.interpolation_time,
            )
            .with_vertical(attacker, &db.sorb.inverse().placement(defender)),
        );
        let first = DeathblowStep {
            attacker_clip: Some(row.attacker_anim()),
            defender_clip: row.defender_anim(),
            ..Default::default()
        };
        (db, first)
    }

    /// Ends the pairing early: the attacker left its body clip (its TAE allowed a move or
    /// another action). The defender plays on under its own root motion.
    pub fn release(&mut self) {
        if self.phase == Phase::Body {
            self.phase = Phase::Done;
        }
    }

    /// Advances by `dt` with the characters where they are now (the attacker already moved by
    /// its own root motion this frame).
    pub fn step(&mut self, dt: f32, attacker: &Placement, defender: &Placement) -> DeathblowStep {
        let mut out = DeathblowStep::default();
        match self.phase {
            Phase::Start => {
                self.time += dt;
                if self.time >= self.times.grab.0 {
                    if self.body.in_range(attacker, defender) {
                        self.phase = Phase::Body;
                        self.time = 0.0;
                        self.slide = Some(AbsorbSlide::new(
                            self.absorbed,
                            &self.sorb,
                            attacker,
                            defender,
                            self.body.interpolation_time,
                        ));
                        out.attacker_clip = Some(self.body.attacker_anim());
                        out.defender_clip = self.defender_clip();
                    } else if self.time > self.times.grab.1 {
                        self.phase = Phase::Missed;
                    }
                }
            }
            Phase::Body => {
                self.time += dt;
                out.absorb = self.slide.as_mut().and_then(|s| s.advance(dt));
                if !self.hit && self.time >= self.times.damage {
                    self.hit = true;
                    out.hit = true;
                    out.killed = self.lethal;
                }
                if self.lethal
                    && self.death_clip
                    && !self.branched
                    && self.times.death_branch.is_some_and(|b| self.time >= b)
                {
                    self.branched = true;
                    out.defender_clip = self.body.defender_death_anim();
                }
                if self.time >= self.body_duration {
                    self.phase = Phase::Done;
                }
            }
            Phase::Done | Phase::Missed => {}
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_enemies_taking_more_than_one_deathblow_show_marks() {
        // An ordinary soldier (ninsatuNum 0) takes one deathblow and shows no dots.
        assert_eq!(bar_marks(deathblow_marks(0), deathblow_marks(0)), 0);
        // A 2-mark enemy shows 2, then 1 after the first deathblow.
        assert_eq!(bar_marks(deathblow_marks(2), 2), 2);
        assert_eq!(bar_marks(deathblow_marks(1), 2), 1);
        assert_eq!(bar_marks(7, 7), MAX_BAR_MARKS);
    }

    fn row() -> ThrowRow {
        ThrowRow {
            id: 11010000,
            atk_chr: 0,
            def_chr: 1010,
            dist: 3.0,
            diff_ang_min: 90.0,
            diff_ang_max: 180.0,
            upper_y: 1.8,
            lower_y: 1.8,
            diff_ang_my_to_def: 35.0,
            atk_anim: 500000,
            def_anim: 0,
            atk_anim_offset: 200,
            def_anim_offset: 0,
            atk_sorb_dummy: 0,
            def_sorb_dummy: 0,
            throw_kind: 40000,
            turn_attacker: true,
            interpolation_time: 0.5,
        }
    }

    #[test]
    fn front_angle_and_range() {
        let r = row();
        let attacker = Placement::default(); // at origin facing -Z
        let facing_back = Placement {
            position: [0.0, 0.0, -2.0],
            yaw: std::f32::consts::PI, // facing +Z, toward the attacker
        };
        assert!(r.in_range(&attacker, &facing_back));
        let turned_away = Placement {
            position: [0.0, 0.0, -2.0],
            yaw: 0.0, // facing away: attacker is behind it
        };
        assert!(!r.in_range(&attacker, &turned_away));
        let far = Placement {
            position: [0.0, 0.0, -3.5],
            ..facing_back
        };
        assert!(!r.in_range(&attacker, &far));
        let attacker_looking_away = Placement {
            yaw: std::f32::consts::FRAC_PI_2,
            ..attacker
        };
        assert!(!r.in_range(&attacker_looking_away, &facing_back));
    }

    #[test]
    fn timeline_lunge_lock_hit_branch() {
        let start = row();
        let body = ThrowRow {
            id: 11010001,
            dist: 2.0,
            diff_ang_min: 0.0,
            diff_ang_max: 180.0,
            diff_ang_my_to_def: 180.0,
            atk_anim: 510000,
            def_anim: 12000,
            atk_sorb_dummy: 267,
            turn_attacker: false,
            ..row()
        };
        let times = ThrowTimes {
            grab: (0.27, 0.33),
            damage: 0.9,
            death_branch: Some(1.67),
        };
        let sorb = DummyFrame {
            position: [0.0, 0.0, -1.2],
            yaw: std::f32::consts::PI,
        };
        let (mut db, first) = Deathblow::begin((&start, &body), times, sorb, 2.67, 1);
        assert_eq!(first.attacker_clip.as_deref(), Some("a200_500000"));
        let mut attacker = Placement::default();
        let mut defender = Placement {
            position: [0.0, 0.0, -1.8],
            yaw: 3.0,
        };
        let dt = 1.0 / 30.0;
        let mut log = Vec::new();
        let mut body_start_z = None;
        for _ in 0..120 {
            attacker.position[2] -= 0.5 * dt; // the attacker's root motion
            let s = db.step(dt, &attacker, &defender);
            if s.attacker_clip.as_deref() == Some("a200_510000") {
                body_start_z = Some(attacker.position[2]);
            }
            if let Some(a) = s.absorb {
                assert_eq!(a.who, Absorbed::Defender);
                for i in 0..3 {
                    defender.position[i] += a.delta[i];
                }
                defender.yaw += a.yaw;
            }
            log.push((db.phase, s, defender));
        }
        let body_start = log
            .iter()
            .position(|(_, s, _)| s.attacker_clip.is_some())
            .unwrap();
        assert_eq!(
            log[body_start].1.attacker_clip.as_deref(),
            Some("a200_510000")
        );
        assert_eq!(
            log[body_start].1.defender_clip.as_deref(),
            Some("a000_012000")
        );
        assert_eq!(log.iter().filter(|(_, s, _)| s.killed).count(), 1);
        let branch = log
            .iter()
            .find_map(|(_, s, _)| s.defender_clip.clone().filter(|c| c.ends_with("012001")));
        assert!(branch.is_some());
        // After the slide-in the defender stands on the dummy of the attacker as it was when the
        // body began, 1.2 m ahead and facing it; the attacker's later motion does not drag it.
        let z = body_start_z.unwrap();
        let (_, _, p) = log
            .iter()
            .rev()
            .find(|(p, _, _)| *p == Phase::Body)
            .unwrap();
        assert!((p.position[2] - (z - 1.2)).abs() < 1e-4, "{p:?}");
        assert!(
            angle_diff(p.yaw, std::f32::consts::PI).abs() < 1e-4,
            "{p:?}"
        );
        assert!(log.iter().any(|(p, _, _)| *p == Phase::Done));
    }
    #[test]
    fn sorb_dummy_in_front_facing_back() {
        // c0000 dummy 267: 1.2 m in front of the player (source -Z), forward +Z.
        let d = DummyFrame {
            position: [0.0, 0.0, -1.2],
            yaw: (-0.0f32).atan2(-1.0),
        };
        let attacker = Placement {
            position: [1.0, 0.0, 0.0],
            yaw: std::f32::consts::FRAC_PI_2, // facing -X
        };
        let p = d.placement(&attacker);
        assert!((p.position[0] - (1.0 - 1.2)).abs() < 1e-5, "{p:?}");
        assert!(p.position[2].abs() < 1e-5, "{p:?}");
        // Faces back along +X, toward the attacker.
        let f = p.forward();
        assert!((f[0] - 1.0).abs() < 1e-5 && f[1].abs() < 1e-5, "{f:?}");
    }
}
