//! Enemy perception and target state: a port of the exe's NPC targeting system
//! (NS_SPRJ::SprjTargetingSystem, update FUN_14061ff40, per AiThink +0x7b30).
//!
//! Each enemy keeps target slots (normal enemy, sound, indication pos, last memory pos) and
//! derives one target state from them (FUN_1406208c0, in this priority order):
//!   normal enemy -> its own state (FIND, then BATTLE), sound / indication / memory -> CAUTION,
//!   nothing -> NONE. These are AI_TARGET_STATE__NONE/CAUTION/FIND/BATTLE (ai_define.lua).
//!
//! Sight (FUN_14061bb70 "normal", FUN_14061c2a0 "around"): a cone from an eye pushed
//! eye_BackOffsetDist behind the enemy's dummy 500 (at its feet height), from eye_BeginDist out
//! to eye_dist, within eye_ang_left/right (yaw) and eye_ang_upper/bottom (elevation), every
//! NpcThinkParam row. Checked against the exe on 52,554 placements (aicpp, 2026-11-03).
//! Wolf's SpEffects shrink it: distances x prod(1 - sightSearchEnemyCut / 100) (FUN_140bfe8a0),
//! angles x prod(1 - sightSearch<Dir>AngleCut / 100) (FUN_140bfea80 ...).
//! - In the normal cone (the "perceive" set once in FIND/BATTLE) Wolf becomes a normal target
//!   in FIND (FUN_14062bdb0); FIND turns to BATTLE inside BattleStartDist (FUN_14062be40).
//! - In the wider around cone, with no normal target, a meter starts (FUN_140625340) and fills
//!   aroundTargetIncrementPoint per second x prod(aroundSightPointAddRate) while Wolf stays in
//!   the cone, drains 20 per second when he leaves it (FUN_140612400). At 100 it becomes an
//!   indication target at Wolf's position (FUN_1406285c0, IndicationTargetForgetTime): CAUTION.
//! The sight search runs every 0.2-0.4 s (FUN_140623ca0: rand * 0.2 + 0.2, DAT_143afb488/c);
//! the meter updates every frame. Light level (Good / Dark / PitchDark x 1 / 0.5 / 0.3,
//! DAT_143afb490..8): Wolf's floor part's MapVisibility remapped by the observer's stateInfo 340
//! SpEffect (`Seen::observed_by`); Wolf's own index stays Good here (`Seen::light_index`).

use bevy::prelude::*;
use serde_json::Value;

pub const NONE: u8 = 0;
pub const CAUTION: u8 = 1;
pub const FIND: u8 = 2;
pub const BATTLE: u8 = 3;

/// AI_TARGET_TYPE__* (ai_define.lua).
pub const TYPE_NONE: u8 = 0;
pub const TYPE_NORMAL_ENEMY: u8 = 3;
pub const TYPE_SOUND: u8 = 4;
pub const TYPE_MEMORY_ENEMY: u8 = 5;
pub const TYPE_INDICATION_POS: u8 = 6;

/// Meter drain per second while out of the around cone (FUN_140612400: points - dt * 20).
const METER_DRAIN: f32 = 20.0;
/// Meter value that turns into a target (FUN_140626430: 100.0 <= points).
const METER_FULL: f32 = 100.0;
/// Where the sight ray ends on Wolf: his feet + his physics +0xd0 (ctx +0xa0, 1.5 m).
pub const WOLF_SIGHT_HEIGHT: f32 = 1.5;

fn f(think: &Value, k: &str) -> f32 {
    think[k].as_f64().unwrap_or(0.0) as f32
}

/// One NpcThinkParam sight set ("normal", "around" or "perceive").
#[derive(Clone, Copy, Debug)]
pub struct Cone {
    pub dist: f32,
    pub begin: f32,
    pub back: f32,
    pub up: f32,
    pub down: f32,
    pub left: f32,
    pub right: f32,
    /// NpcThinkParam disableDark (row +0x53): the light factor is 1 whatever Wolf's light.
    pub disable_dark: bool,
}

impl Cone {
    pub fn from(think: &Value, set: &str) -> Cone {
        Cone {
            dist: f(think, &format!("eye_dist_{set}")),
            begin: f(think, &format!("eye_BeginDist_{set}")),
            back: f(think, &format!("eye_BackOffsetDist_{set}")),
            up: f(think, &format!("eye_ang_upper_{set}")),
            down: f(think, &format!("eye_ang_bottom_{set}")),
            left: f(think, &format!("eye_ang_left_{set}")),
            right: f(think, &format!("eye_ang_right_{set}")),
            disable_dark: f(think, "disableDark") != 0.0,
        }
    }
}

/// Wolf as the enemy's sight sees him (the exe's sight context filled by FUN_140bd3e40).
/// `Seen::new` holds Wolf's own side; `observed_by` (the observer's SpEffects) and `from_eye`
/// (the observer's place, FUN_140616f50) make the context one enemy's search uses.
#[derive(Clone, Copy, Debug)]
pub struct Seen {
    /// Root (feet) position (ctx +0x90 = FUN_1409f0870).
    pub pos: Vec3,
    /// Added to the far distances (ctx +0x80, the chr physics radius; FUN_140bbf740).
    pub radius: f32,
    /// Distance factor from sightSearchEnemyCut (ctx +0x64).
    pub cut: f32,
    /// Angle factors upper / bottom / left / right (ctx +0x68 / 0x6c / 0x70 / 0x74).
    pub ang: [f32; 4],
    /// Meter fill factor from aroundSightPointAddRate (FUN_140bfc050).
    pub around_rate: f32,
    /// Light factor (ctx +0x58 table [ctx +0x79]): LIGHT_TABLE at `light_index` after the
    /// observer's remap (`observed_by`).
    pub light: f32,
    /// Wolf's facing (ctx +0xb0, FUN_1409eeb40): the geometry bits look from it.
    pub facing: Vec3,
    /// Wolf's light index, ChrIns +0x1acd & 0xf (0 Good / 1 Dark / 2 PitchDark): the MapVisibility
    /// of the MSB collision part he stands on (part type data +0x27; FUN_1409f5820 via
    /// FUN_1409b5c80 when his floor part changes), forced to Good inside a CSDarkSight region
    /// ([0x143d87fd0], FUN_140c0c170: MSB, chr and SpEffect regions). gap: always 0 here; our
    /// hit collision keeps no part per triangle and no DarkSight region is exported (wolf_seen).
    pub light_index: u8,
    /// Factors of the SpEffects with sightCutLimitType 1 / 2: they count only when the geometry
    /// bit 1 / 2 holds (FUN_140bd3e40 -> FUN_140616ef0; `from_eye` folds them in).
    pub limited: [Cuts; 2],
    /// Nose marks: stateInfo 333 (bit 0, ctx +0xc0) / 335 (bit 1, ctx +0xc1) on Wolf.
    pub nose_marks: u8,
    /// The observer answers a mark with stateInfo 332 / 334 (`observed_by`): its nose misses
    /// Wolf (FUN_14061c940). No SpEffectParam row carries 332-335, so with the game's data the
    /// nose is never blocked.
    pub nose_hidden: bool,
}

/// One sightCutLimitType's factors (FUN_140bfe8a0 cut, FUN_140bfea80 / bfe6e0 / bfe7c0 / bfe9a0
/// angles, FUN_140bfc050 meter rate).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cuts {
    pub cut: f32,
    pub ang: [f32; 4],
    pub around_rate: f32,
}

impl Cuts {
    pub const ONE: Cuts = Cuts { cut: 1.0, ang: [1.0; 4], around_rate: 1.0 };

    fn add(&mut self, e: &crate::data::SpEffect) {
        let cut = |pct: f32| if pct > 0.0 { 1.0 - pct * 0.01 } else { 1.0 };
        // The distance cut skips stateInfo 7 / 53 unless the observer's flag (ctx +0x10,
        // observer vfunc 0x68) is set; gap: the flag is taken as off (0 on the snapshot's six
        // enemies; no such row has a cut).
        if !matches!(e.state_info, 7 | 53) {
            self.cut *= cut(e.sight_search_enemy_cut);
        }
        self.ang[0] *= cut(e.sight_search_upper_angle_cut);
        self.ang[1] *= cut(e.sight_search_bottom_angle_cut);
        self.ang[2] *= cut(e.sight_search_left_angle_cut);
        self.ang[3] *= cut(e.sight_search_right_angle_cut);
        self.around_rate *= e.around_sight_point_add_rate;
    }
}

/// The light factors Good / Dark / PitchDark (DAT_143afb490..8 -> ctx +0x58 / 0x5c / 0x60).
pub const LIGHT_TABLE: [f32; 3] = [1.0, 0.5, 0.3];
/// The geometry bits' angles (FUN_14064c4a0 / FUN_14064c4b0: DAT_143afbb04 / 08, read on the
/// snapshot): a 180 deg front, a 220 deg back.
const FRONT_ANGLE: f32 = 180.0;
const BACK_ANGLE: f32 = 220.0;

/// The observer's geometry bits (FUN_140616f50): from Wolf's feet to the observer, flattened,
/// against Wolf's facing. Bit 1: the observer within 90 deg of where he faces (never while the
/// observer's perceive set is on); bit 2: more than 70 deg off it. Checked on 7,776 placements
/// (aistealth 2026-11-04, `geometry_bits_match_the_exe`).
pub fn geometry_bits(observer: Vec3, wolf: Vec3, facing: Vec3, perceive: bool) -> u8 {
    let to = Vec3::new(observer.x - wolf.x, 0.0, observer.z - wolf.z).normalize_or_zero();
    let dot = to.dot(facing);
    let mut bits = 0;
    if !perceive && (FRONT_ANGLE * 0.5 * 0.017453292).cos() <= dot {
        bits |= 1;
    }
    if dot < ((180.0 - BACK_ANGLE * 0.5) * 0.017453292).cos() {
        bits |= 2;
    }
    bits
}

/// The observer's side of the context: the light remap of its stateInfo 340 SpEffect (ctx +8 is
/// the observer's SpEffect holder; FUN_140617150 -> FUN_140616e90: Good / Dark / PitchDark ->
/// mapVisibilityOverrideGood / Dark / PitchDark) and its nose answers (stateInfo 332 / 334).
#[derive(Clone, Copy, Debug, Default)]
pub struct Observer {
    pub light: Option<[u8; 3]>,
    pub nose_marks: u8,
}

impl Observer {
    /// From the observer's active SpEffectParam rows, newest first (the exe takes the first
    /// stateInfo 340 node of the list).
    pub fn from_rows<'a>(rows: impl IntoIterator<Item = &'a Value>) -> Observer {
        let mut o = Observer::default();
        for r in rows {
            let b = |k: &str| r[k].as_u64().unwrap_or(0) as u8;
            match r["stateInfo"].as_i64() {
                Some(340) if o.light.is_none() => {
                    o.light = Some([b("mapVisibilityOverrideGood"), b("mapVisibilityOverrideDark"), b("mapVisibilityOverridePitchDark")]);
                }
                Some(332) => o.nose_marks |= 1,
                Some(334) => o.nose_marks |= 2,
                _ => {}
            }
        }
        o
    }
}

impl Seen {
    /// Wolf's side from his active SpEffects: type 0 effects count at once, types 1 / 2 wait
    /// for the observer's geometry bits (`from_eye`). Checked against the exe's factor
    /// functions and whole contexts (aistealth 2026-11-04, `sight_factors_match_the_exe`).
    pub fn new(pos: Vec3, radius: f32, sp: &[&crate::data::SpEffect]) -> Seen {
        let mut own = Cuts::ONE;
        let mut limited = [Cuts::ONE; 2];
        let mut nose_marks = 0;
        for e in sp {
            match e.sight_cut_limit_type {
                0 => own.add(e),
                1 => limited[0].add(e),
                2 => limited[1].add(e),
                _ => {}
            }
            nose_marks |= match e.state_info {
                333 => 1,
                335 => 2,
                _ => 0,
            };
        }
        let mut s = Seen {
            pos,
            radius,
            cut: own.cut,
            ang: own.ang,
            around_rate: own.around_rate,
            light: LIGHT_TABLE[0],
            facing: Vec3::NEG_Z,
            light_index: 0,
            limited,
            nose_marks,
            nose_hidden: false,
        };
        s.clamp();
        s
    }

    fn clamp(&mut self) {
        self.cut = self.cut.clamp(0.0, 1.0);
        self.ang = self.ang.map(|a| a.clamp(0.0, 1.0));
        self.around_rate = self.around_rate.clamp(0.0, 999.9);
    }

    /// With the geometry bits known: the limited effects of the set bits folded in.
    pub fn with_bits(&self, bits: u8) -> Seen {
        let mut s = *self;
        for (i, l) in self.limited.iter().enumerate() {
            if bits & (1 << i) != 0 {
                s.cut *= l.cut;
                s.ang = std::array::from_fn(|k| s.ang[k] * l.ang[k]);
                s.around_rate *= l.around_rate;
            }
        }
        s.limited = [Cuts::ONE; 2];
        s.clamp();
        s
    }

    /// Seen from an observer at `eye` (ctx +0x20 for the cones; the meter fill measures from the
    /// observer's feet, FUN_140612400) with its perceive set on or off (ctx +0x56).
    pub fn from_eye(&self, eye: Vec3, perceive: bool) -> Seen {
        if self.limited == [Cuts::ONE; 2] {
            return *self;
        }
        self.with_bits(geometry_bits(eye, self.pos, self.facing, perceive))
    }

    /// Seen by one observer: its light remap and nose answers.
    pub fn observed_by(&self, o: &Observer) -> Seen {
        let mut s = *self;
        let i = self.light_index.min(2) as usize;
        let idx = o.light.map_or(i, |m| m[i] as usize);
        s.light = LIGHT_TABLE[idx.min(2)];
        s.nose_hidden = self.nose_marks & o.nose_marks != 0;
        s
    }
}

/// Is `wolf` inside `cone` seen from an enemy whose eye point is `eye` facing `fwd`
/// (FUN_14061bb70 / FUN_14061c2a0), with nothing of the map between its `head` and him
/// (`sight_clear`)?
pub fn in_cone(cone: &Cone, eye: Vec3, head: Vec3, fwd: Vec3, wolf: &Seen) -> bool {
    in_cone_shape(cone, eye, fwd, wolf) && sight_clear(head, wolf.pos)
}

/// The nose sphere (FUN_14061c940, FUN_140617280's result 1 when the normal cone misses): Wolf's
/// feet (ctx +0x90) within NpcThinkParam nose_dist (row +0x36) + his radius (ctx +0x80) of the
/// sight origin (ctx +0x20), in 3D; a row without a nose_dist smells nothing. Checked on 189
/// placements (aiframe 2026-11-03). Hidden from it when Wolf wears stateInfo 333 (ctx +0xc0)
/// and the observer 332, or 335 (ctx +0xc1) and 334 (`Seen::nose_hidden`; 25 pairs,
/// aistealth 2026-11-04).
pub fn in_nose(think: &Value, eye: Vec3, wolf: &Seen) -> bool {
    let nose = f(think, "nose_dist");
    !wolf.nose_hidden && nose > 0.0 && (wolf.pos - eye).length_squared() < (nose + wolf.radius) * (nose + wolf.radius)
}

/// The sight ray's start (ctx +0x30, observer vfunc 0x28 FUN_14060e6b0): dummy 500 where the
/// model has one (0.9-2.0 m above the root on the snapshot, aiframe 2026-11-03), else the root
/// pushed forward by the physics radius and raised by the physics height (+0xd0).
pub fn sight_head(feet: Vec3, fwd: Vec3, dmy500: Option<Vec3>, radius: f32, height: f32) -> Vec3 {
    match dmy500 {
        Some(d) => d,
        None => feet + fwd.with_y(0.0).normalize_or_zero() * radius + Vec3::Y * height,
    }
}

/// The enemy's sight origin (observer vfunc 0x20, FUN_14060e7e0 -> 0x28 FUN_14060e6b0): its
/// dummy 500 when the model has one, else the root pushed forward by its physics radius
/// (FUN_1409efb80), both at the root's height. On the snapshot dummy 500 sat 0.15-1.0 m ahead
/// of the root (six enemies, aicpp 2026-11-03).
pub fn eye_point(feet: Vec3, fwd: Vec3, dmy500: Option<Vec3>, radius: f32) -> Vec3 {
    match dmy500 {
        Some(d) => d.with_y(feet.y),
        None => feet + fwd.with_y(0.0).normalize_or_zero() * radius,
    }
}

/// The cone test alone (FUN_14061bb70 / FUN_14061c2a0 without the ray, FUN_14061c8a0). The
/// observer side: `eye` (ctx +0x20, `eye_point`) and the body facing (ctx +0x40, flattened);
/// gap: ctx +0x50 (observer vfunc 0xe0, 1.0 on the snapshot) and ctx +0x78 (skips the far
/// test) are taken as 1 and off.
pub fn in_cone_shape(cone: &Cone, eye: Vec3, fwd: Vec3, wolf: &Seen) -> bool {
    let fwd = fwd.with_y(0.0).normalize_or_zero();
    let light = if cone.disable_dark { 1.0 } else { wolf.light };
    // The eye pushed back along the facing (FUN_14061af30 / ae40 / b030).
    let cut = wolf.cut * light;
    let eye = eye - fwd * cone.back * cut;
    let d = wolf.pos - eye;
    let d2 = d.length_squared();
    let begin = cone.begin * cut;
    // The outer gate before the angles has the light but not the cut.
    let gate = wolf.radius + cone.dist * light;
    if !(begin * begin <= d2 && d2 < gate * gate) {
        return false;
    }
    // Yaw within [-left, +right] (positive to the enemy's right), elevation within
    // [-bottom, +upper]: the exe tests the half-width cosine around the centre angle.
    let flat = d.with_y(0.0);
    let side = flat.dot(fwd.cross(Vec3::Y));
    let yaw = side.atan2(flat.dot(fwd)).to_degrees();
    let (left, right) = (cone.left * wolf.ang[2], cone.right * wolf.ang[3]);
    if !in_range(yaw, -left, right) {
        return false;
    }
    let elev = d.y.atan2(flat.length()).to_degrees();
    let (up, down) = (cone.up * wolf.ang[0], cone.down * wolf.ang[1]);
    if !in_range(elev, -down, up) {
        return false;
    }
    let far = wolf.radius + cone.dist * cut;
    d2 < far * far
}

/// The sight ray (FUN_14061c8a0, last step of both checks): from the sight context's +0x30 to
/// +0xa0 through the collision (FUN_14094e310, filter 0x32); a hit means not seen. The normal
/// cone casts it unless ctx +0xc2 is set (FUN_140617280). From the observer's `head`
/// (`sight_head`) to Wolf's feet raised by his physics +0xd0, 1.5 m (FUN_1409f0890 /
/// FUN_140bbf730; ctx +0xa0 on the snapshot, aiframe 2026-11-03). gap: a target whose ChrInfo
/// +0x330 >= 0 uses a dummy poly instead; which collision filter 0x32 skips is not known (our map
/// has the hit collision only), and the exe's rays were not compared with our map (the snapshot's
/// area, m11_00, is not extracted).
fn sight_clear(head: Vec3, wolf: Vec3) -> bool {
    let (a, b) = (head, wolf + Vec3::Y * WOLF_SIGHT_HEIGHT);
    let d = b - a;
    let len = d.length();
    len < 1e-4 || crate::terrain::raycast(a, d / len, len).is_none()
}

fn in_range(a: f32, lo: f32, hi: f32) -> bool {
    let centre = (lo + hi) * 0.5;
    let half = (hi - lo) * 0.5;
    ((a - centre + 540.0).rem_euclid(360.0) - 180.0).abs() <= half
}

#[derive(Clone, Copy, Debug)]
pub struct Spot {
    pub pos: Vec3,
    /// Seconds until forgotten.
    pub forget: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct Sound {
    pub pos: Vec3,
    pub forget: f32,
    /// AiSoundParam row and its rank (AI_SOUND_RANK__NORMAL 0 / IMPORTANT 1).
    pub id: i64,
    pub rank: i64,
    /// The sound record's serial (the exe's global counter, FUN_14060f840):
    /// GetLatestSoundTargetInstanceID (enemy.rs AiSounds, Wolf's TAE sounds too).
    pub serial: i64,
}

/// A sound record one listener hears this frame (`Targeting::listen`).
#[derive(Clone, Copy, Debug)]
pub struct Heard {
    pub pos: Vec3,
    pub id: i64,
    pub rank: i64,
    pub serial: i64,
}

#[derive(Clone, Copy, Debug)]
pub struct Normal {
    /// FIND or BATTLE.
    pub state: u8,
    /// Seconds until Wolf is forgotten while unseen (SightTargetForgetTime when seen).
    pub forget: f32,
    /// Seen at the last sight search.
    pub visible: bool,
    /// Seen or smelt (nose sphere) at the last sight search: the forget time runs only without.
    pub held: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct Meter {
    /// 0..100 points.
    pub points: f32,
    pub visible: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Targeting {
    pub normal: Option<Normal>,
    pub sound: Option<Sound>,
    /// A sound target was taken or changed this frame (cleared by the hearing pass, enemy.rs).
    pub sound_new: bool,
    pub indication: Option<Spot>,
    pub memory: Option<Spot>,
    pub meter: Option<Meter>,
    pub state: u8,
    pub prev_state: u8,
    pub kind: u8,
    pub prev_kind: u8,
    /// Flag 0x100000 of the targeting system: set in FIND / BATTLE, cleared in NONE. While set,
    /// sight uses the "perceive" set (FUN_140625d50 -> ctx +0x56).
    pub perceive: bool,
    /// The state changed in a way that drops the AI's plan (FUN_14061f500).
    pub replan: bool,
    /// Wolf was in the around cone at the last sight search with no normal target held: every
    /// frame without a meter the meter update starts one from it (FUN_140626430), so a full meter
    /// starts again the next frame (aiframe 2026-11-03).
    around_seen: bool,
    pub(crate) search_timer: f32,
    /// A sight search ran this frame and drew a new interval (SearchRing::place stretches it).
    pub searched: bool,
    rng: u32,
}

/// The search scheduler every enemy's sight interval goes through (DAT_143d547d8 +0x28, made by
/// FUN_1406142b0): 100 buckets of frames, at most `cap` searches per bucket (+0x68 = 3; +0x69, no
/// limit, 0). FUN_140614be0 puts a new interval `t` in the bucket (cursor + (int)(t x 30)) % 100;
/// a full one moves it k buckets on to the first with room and returns t + k / 30. The world
/// task FUN_140a74560 -> FUN_140614dd0 clears the cursor's bucket and moves the cursor on once a
/// frame. Checked against the exe on 400 rings (aimove 2026-11-03, `search_ring_matches_the_exe`).
/// With a few enemies a bucket never fills, so the 0.2-0.4 s interval stays as drawn; the
/// 0.62 s aiframe saw came from a ring the emulator never moved on (the task was not run).
#[derive(Clone, Debug)]
pub struct SearchRing {
    pub counts: [u8; 100],
    pub cursor: usize,
    pub cap: u8,
}

impl Default for SearchRing {
    fn default() -> Self {
        SearchRing { counts: [0; 100], cursor: 0, cap: 3 }
    }
}

impl SearchRing {
    /// FUN_140614dd0's per-frame step.
    pub fn advance(&mut self) {
        self.counts[self.cursor] = 0;
        self.cursor = (self.cursor + 1) % 100;
    }

    /// FUN_140614be0: the interval, stretched past full buckets.
    pub fn place(&mut self, secs: f32) -> f32 {
        let mut i = ((secs * 30.0) as i32 + self.cursor as i32) as u32 as usize % 100;
        if self.counts[i] < self.cap {
            self.counts[i] += 1;
            return secs;
        }
        let mut k = 0;
        for _ in 0..100 {
            i = (i + 1) % 100;
            k += 1;
            if self.counts[i] < self.cap {
                self.counts[i] += 1;
                break;
            }
        }
        k as f32 / 30.0 + secs
    }
}

impl Targeting {
    pub fn new(seed: u32) -> Self {
        Targeting { rng: seed | 1, ..Default::default() }
    }

    fn rand(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        (self.rng % 10_000) as f32 / 10_000.0
    }

    /// The target state changed since the previous update (ai:IsChangeState).
    pub fn changed(&self) -> bool {
        self.state != self.prev_state
    }

    /// Where the current target is: Wolf himself, or the remembered / heard / indicated spot.
    pub fn target_pos(&self, wolf: Vec3) -> Option<Vec3> {
        match self.kind {
            TYPE_NORMAL_ENEMY => Some(wolf),
            TYPE_SOUND => self.sound.map(|s| s.pos),
            TYPE_INDICATION_POS => self.indication.map(|s| s.pos),
            TYPE_MEMORY_ENEMY => self.memory.map(|s| s.pos),
            _ => None,
        }
    }

    /// Seconds since Wolf was last seen (ai:GetTopNormalEnemyForgettingTime, FUN_14061f3f0:
    /// SightTargetForgetTime minus the remaining forget time); 0 without a normal target.
    pub fn forgetting_time(&self, think: &Value) -> f32 {
        self.normal.map_or(0.0, |n| (f(think, "SightTargetForgetTime") - n.forget).max(0.0))
    }

    /// The stealth HUD indicator (FUN_1408c5fb0 args from FUN_14061ff40): level 0 none,
    /// 1 caution, 2 found; ratio = the fullest meter / 100.
    pub fn hud(&self) -> (u8, f32) {
        let level = match self.state {
            CAUTION => 1,
            FIND | BATTLE => 2,
            _ => 0,
        };
        (level, self.meter.map_or(0.0, |m| (m.points / METER_FULL).clamp(0.0, 1.0)))
    }

    /// Wolf hurt the enemy: a normal target straight in BATTLE (target reason 4,
    /// FUN_14061d140 sets state 3).
    pub fn damaged(&mut self, think: &Value) {
        let forget = f(think, "SightTargetForgetTime");
        self.normal = Some(Normal { state: BATTLE, forget, visible: true, held: true });
        self.meter = None;
    }

    /// One sound record (a single pass of `listen`, its serial the id: tests).
    #[cfg(test)]
    pub fn hear(&mut self, think: &Value, pos: Vec3, id: i64, rank: i64) {
        self.listen(think, &[Heard { pos, id, rank, serial: id }]);
    }

    /// The listener pass of one frame (exe FUN_140624b70, every frame while no character target
    /// is held): every record in reach is offered to a fresh slot (FUN_140610b30), which keeps the
    /// higher rank, and of one rank the newer record (serial FUN_1406101a0: new - held > 0); the
    /// records of a rank are scanned only under a held sound of no higher rank (rank 0 while the
    /// held rank < 1, rank 1 while < 2: FUN_14064c540 channels 0 / 1), so Wolf's footsteps (rank
    /// 0) do not overwrite a guard break's IMPORTANT sound 5000 (round 25). The pick becomes the
    /// sound target (FUN_14062a2a0: its spot, id, rank, serial; forget SoundTargetForgetTime,
    /// refreshed each frame it is heard); a serial other than the held one raises
    /// INTERUPT_ChangeSoundTarget (46: the list's changed flag, FUN_14061cd20 / FUN_140612980).
    /// Checked against the exe (aiint): two records in one frame -> the newer; a record refreshed
    /// in its life keeps its serial (no 46); the same id raised again after its life ran out is a
    /// new record (46); a newer record of the same rank wins over the held one still heard.
    /// gap: ranks >= 2 go through other channels (FUN_140628230 / FUN_140629f10, not traced):
    /// taken here as rank 1 is.
    pub fn listen(&mut self, think: &Value, heard: &[Heard]) {
        let held = self.sound.map(|s| s.rank);
        let pick = heard.iter().filter(|h| held.is_none_or(|r| r <= h.rank)).max_by_key(|h| (h.rank, h.serial));
        if let Some(h) = pick {
            self.sound_new |= self.sound.is_none_or(|s| s.serial != h.serial);
            self.sound = Some(Sound { pos: h.pos, forget: f(think, "SoundTargetForgetTime"), id: h.id, rank: h.rank, serial: h.serial });
        }
    }

    pub fn clear_enemy(&mut self) {
        self.normal = None;
    }
    pub fn clear_sound(&mut self) {
        self.sound = None;
    }
    pub fn clear_indication(&mut self) {
        self.indication = None;
    }
    pub fn clear_memory(&mut self) {
        self.memory = None;
    }

    /// Back to "never noticed" (debug: stealth setup).
    pub fn reset(&mut self) {
        let rng = self.rng;
        *self = Targeting::new(rng);
    }

    /// One frame (FUN_14061ff40 -> FUN_140623ca0). `me` / `fwd`: the enemy's feet and facing;
    /// `eye`: its sight origin (`eye_point`); `head`: the sight ray's start (`sight_head`).
    /// Checked frame by frame against the exe (aiframe 2026-11-03, test
    /// `targeting_frames_match_the_exe`).
    #[allow(clippy::too_many_arguments)]
    pub fn update(&mut self, think: &Value, me: Vec3, eye: Vec3, head: Vec3, fwd: Vec3, wolf: &Seen, dt: f32) {
        let around = Cone::from(think, "around");
        // Wolf's limited SpEffects by where this enemy stands (FUN_140616f50): from the sight
        // origin for the cones, from its feet for the meter rate (FUN_140612400). gap: the
        // meter's perceive argument (its fifth) is taken as the perceive set.
        let rate = wolf.from_eye(me, self.perceive).around_rate;
        let wolf = &wolf.from_eye(eye, self.perceive);
        // Meter (FUN_140626430, every frame before the search): started from the last search's
        // around sighting, filled while Wolf stays in the around cone, drained out of it.
        if self.meter.is_none() && self.around_seen && self.normal.is_none() {
            self.meter = Some(Meter { points: 0.0, visible: true });
        }
        if let Some(mut m) = self.meter {
            m.visible = in_cone(&around, eye, head, fwd, wolf);
            if m.visible {
                m.points += f(think, "aroundTargetIncrementPoint") * dt * rate;
            } else {
                m.points -= dt * METER_DRAIN;
            }
            self.meter = if m.points <= 0.0 {
                None
            } else if m.points >= METER_FULL {
                self.indication = Some(Spot { pos: wolf.pos, forget: f(think, "IndicationTargetForgetTime") });
                None
            } else {
                Some(m)
            };
        }
        // Sight search on its interval (FUN_140625f30 / FUN_1406250a0 / FUN_140624620): the
        // normal (or perceive) cone, else the nose sphere (FUN_140617280 result 2 / 1; both make
        // a normal target, FUN_14060e390, only sight marks it visible), else the around cone.
        // The new interval is spread over the frames by the shared ring (FUN_140614be0, see
        // SearchRing): `searched` tells the caller to place it there.
        self.search_timer -= dt;
        self.searched = self.search_timer <= 0.0;
        if self.searched {
            self.search_timer = self.rand() * 0.2 + 0.2;
            let cone = Cone::from(think, if self.perceive { "perceive" } else { "normal" });
            let seen = in_cone(&cone, eye, head, fwd, wolf);
            if seen || in_nose(think, eye, wolf) {
                // FUN_14062a5e0 -> FUN_140629250: a new normal target starts in FIND.
                let forget = f(think, "SightTargetForgetTime");
                let n = self.normal.get_or_insert(Normal { state: FIND, forget, visible: seen, held: true });
                n.forget = forget;
                n.visible = seen;
                n.held = true;
                self.meter = None;
                self.around_seen = false;
            } else {
                if let Some(n) = self.normal.as_mut() {
                    n.visible = false;
                    n.held = false;
                }
                self.around_seen = self.normal.is_none() && in_cone(&around, eye, head, fwd, wolf);
            }
        }
        // Slots: the normal target keeps the memory slot on Wolf (FUN_140628860,
        // MemoryTargetForgetTime) and turns FIND into BATTLE inside BattleStartDist
        // (FUN_14062be40); then every slot's timer runs (FUN_14062a510; the normal target's only
        // while unseen) and spent slots go (FUN_140629720). gap: the exe also counts the sight
        // forget on a LastSightPos slot (FUN_140628b60, gone a frame before the normal target).
        if let Some(n) = self.normal.as_mut() {
            if n.state == FIND && me.distance(wolf.pos) < f(think, "BattleStartDist") {
                n.state = BATTLE;
            }
            self.memory = Some(Spot { pos: wolf.pos, forget: f(think, "MemoryTargetForgetTime") });
            if !n.held {
                n.forget -= dt;
            }
            if n.forget <= 0.0 {
                self.normal = None;
            }
        }
        for s in [&mut self.indication, &mut self.memory] {
            if let Some(spot) = s.as_mut() {
                spot.forget -= dt;
                if spot.forget <= 0.0 {
                    *s = None;
                }
            }
        }
        if let Some(s) = self.sound.as_mut() {
            s.forget -= dt;
            if s.forget <= 0.0 {
                self.sound = None;
            }
        }
        // State (FUN_1406208c0) and its bookkeeping (FUN_14061ff40, FUN_14062bc00).
        let (state, kind) = if let Some(n) = self.normal {
            (n.state, TYPE_NORMAL_ENEMY)
        } else if self.sound.is_some() {
            (CAUTION, TYPE_SOUND)
        } else if self.indication.is_some() {
            (CAUTION, TYPE_INDICATION_POS)
        } else if self.memory.is_some() {
            (CAUTION, TYPE_MEMORY_ENEMY)
        } else {
            (NONE, TYPE_NONE)
        };
        self.prev_state = self.state;
        self.prev_kind = self.kind;
        self.state = state;
        self.kind = kind;
        if matches!(state, FIND | BATTLE) {
            self.perceive = true;
        } else if state == NONE {
            self.perceive = false;
        }
        // FUN_14061f500: a state change replans unless its NpcThinkParam goalAction is 0;
        // memory -> sound also replans.
        self.replan = match (self.changed(), state) {
            (false, _) => self.prev_kind == TYPE_MEMORY_ENEMY && kind == TYPE_SOUND,
            (true, CAUTION) => f(think, "goalAction_ToCaution") != 0.0,
            (true, FIND) => f(think, "goalAction_ToFind") != 0.0,
            (true, _) => true,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// NpcThinkParam 10200000 (c1020) as exported.
    fn c1020() -> Value {
        json!({
            "eye_dist_normal": 20, "eye_BeginDist_normal": 4, "eye_BackOffsetDist_normal": 4,
            "eye_ang_upper_normal": 15, "eye_ang_bottom_normal": 15, "eye_ang_left_normal": 45, "eye_ang_right_normal": 45,
            "eye_dist_around": 40, "eye_BeginDist_around": 14, "eye_BackOffsetDist_around": 14,
            "eye_ang_upper_around": 18, "eye_ang_bottom_around": 18, "eye_ang_left_around": 45, "eye_ang_right_around": 45,
            "eye_dist_perceive": 40, "eye_BeginDist_perceive": 4, "eye_BackOffsetDist_perceive": 4,
            "eye_ang_upper_perceive": 60, "eye_ang_bottom_perceive": 60, "eye_ang_left_perceive": 45, "eye_ang_right_perceive": 45,
            "aroundTargetIncrementPoint": 25.0, "IndicationTargetForgetTime": 150.0, "SightTargetForgetTime": 3600.0,
            "MemoryTargetForgetTime": 150.0, "SoundTargetForgetTime": 150.0, "BattleStartDist": 15,
            "goalAction_ToCaution": 2, "goalAction_ToFind": 3
        })
    }

    fn wolf(pos: Vec3) -> Seen {
        Seen::new(pos, 0.4, &[])
    }

    /// Enemy at the origin facing +Z.
    fn run(t: &mut Targeting, think: &Value, w: &Seen, secs: f32) {
        for _ in 0..(secs * 60.0) as usize {
            t.update(think, Vec3::ZERO, Vec3::ZERO, Vec3::Y * 1.5, Vec3::Z, w, 1.0 / 60.0);
        }
    }

    /// Round 25: a footstep (rank 0) does not overwrite a held guard-break sound (rank 1); an
    /// equal or higher rank does.
    #[test]
    fn a_footstep_keeps_a_held_important_sound() {
        let think = c1020();
        let mut t = Targeting::new(1);
        let spot = Vec3::new(5.0, 0.0, 5.0);
        t.hear(&think, spot, 5000, 1);
        t.hear(&think, Vec3::ZERO, 1010, 0);
        assert_eq!(t.sound.map(|s| (s.pos, s.id, s.rank)), Some((spot, 5000, 1)));
        t.hear(&think, Vec3::ONE, 5000, 1);
        assert_eq!(t.sound.map(|s| s.pos), Some(Vec3::ONE));
        let mut t = Targeting::new(1);
        t.hear(&think, spot, 1010, 0);
        t.hear(&think, Vec3::ONE, 1000, 0);
        assert_eq!(t.sound.map(|s| s.id), Some(1000));
    }

    #[test]
    fn close_in_front_is_found_then_battle() {
        let think = c1020();
        let mut t = Targeting::new(1);
        run(&mut t, &think, &wolf(Vec3::new(0.0, 0.0, 10.0)), 0.5);
        assert_eq!(t.state, BATTLE, "10 m is inside BattleStartDist 15");
        let mut t = Targeting::new(1);
        run(&mut t, &think, &wolf(Vec3::new(0.0, 0.0, 15.5)), 0.5);
        assert_eq!(t.state, FIND, "15.5 m: seen but outside BattleStartDist");
    }

    #[test]
    fn behind_is_not_seen() {
        let think = c1020();
        let mut t = Targeting::new(1);
        run(&mut t, &think, &wolf(Vec3::new(0.0, 0.0, -3.0)), 2.0);
        assert_eq!((t.state, t.meter.is_none()), (NONE, true));
    }

    #[test]
    fn far_fills_the_meter_into_caution() {
        let think = c1020();
        let mut t = Targeting::new(1);
        // 25 m ahead: beyond the normal cone (20 - 4 back offset = 16 m), inside around (26 m).
        let w = wolf(Vec3::new(0.0, 0.0, 25.0));
        run(&mut t, &think, &w, 2.0);
        assert_eq!(t.state, NONE);
        let (_, ratio) = t.hud();
        assert!((0.35..0.6).contains(&ratio), "{ratio}");
        run(&mut t, &think, &w, 2.5);
        assert_eq!((t.state, t.kind), (CAUTION, TYPE_INDICATION_POS), "25 points/s: full in 4 s");
        assert!(t.target_pos(w.pos).is_some());
    }

    #[test]
    fn crouch_cut_shortens_sight() {
        let think = c1020();
        // 15.8 m ahead: just inside the normal cone, outside it with 109200's 20 % cut.
        let mut t = Targeting::new(1);
        run(&mut t, &think, &wolf(Vec3::new(0.0, 0.0, 15.8)), 0.5);
        assert_eq!(t.state, FIND);
        let mut t = Targeting::new(1);
        let mut w = wolf(Vec3::new(0.0, 0.0, 15.8));
        w.cut = 0.8;
        run(&mut t, &think, &w, 0.5);
        assert_eq!(t.state, NONE);
    }

    #[test]
    fn forgotten_wolf_leaves_a_memory_caution() {
        let mut think = c1020();
        think["SightTargetForgetTime"] = json!(15.0); // c1010's row
        let mut t = Targeting::new(1);
        run(&mut t, &think, &wolf(Vec3::new(0.0, 0.0, 10.0)), 0.5);
        assert_eq!(t.state, BATTLE);
        // Behind it now, out of every cone.
        let w = wolf(Vec3::new(0.0, 0.0, -20.0));
        run(&mut t, &think, &w, 10.0);
        assert_eq!(t.state, BATTLE);
        run(&mut t, &think, &w, 6.0);
        assert_eq!((t.state, t.kind), (CAUTION, TYPE_MEMORY_ENEMY));
    }
}
