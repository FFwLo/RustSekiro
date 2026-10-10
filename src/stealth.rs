//! Enemy perception and target state: a port of the exe's NPC targeting system
//! (NS_SPRJ::SprjTargetingSystem, update FUN_14061ff40, per AiThink +0x7b30).
//!
//! Each enemy keeps target slots (normal enemy, sound, indication pos, last memory pos) and
//! derives one target state from them (FUN_1406208c0, in this priority order):
//!   normal enemy -> its own state (FIND, then BATTLE), sound / indication / memory -> CAUTION,
//!   nothing -> NONE. These are AI_TARGET_STATE__NONE/CAUTION/FIND/BATTLE (ai_define.lua).
//!
//! Sight (FUN_14061bb70 "normal", FUN_14061c2a0 "around"): a cone from an eye pushed
//! eye_BackOffsetDist behind the enemy's feet, from eye_BeginDist out to eye_dist, within
//! eye_ang_left/right (yaw) and eye_ang_upper/bottom (elevation), every NpcThinkParam row.
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
//! DAT_143afb490..8) is "Good" here: there is no map lighting.

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
        }
    }
}

/// Wolf as the enemy's sight sees him (the exe's sight context filled by FUN_140bd3e40).
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
}

impl Seen {
    /// Wolf's sight factors from his active SpEffects. Effects with sightCutLimitType 1 / 2
    /// apply only when the observer's geometry bits hold (FUN_140616f50); gap: those bits are
    /// not traced, so such effects (hanging 109220, wall hug 109210) are left out.
    pub fn new(pos: Vec3, radius: f32, sp: &[&crate::data::SpEffect]) -> Seen {
        let mut s = Seen { pos, radius, cut: 1.0, ang: [1.0; 4], around_rate: 1.0 };
        for e in sp.iter().filter(|e| e.sight_cut_limit_type == 0) {
            let cut = |pct: f32| if pct > 0.0 { 1.0 - pct * 0.01 } else { 1.0 };
            s.cut *= cut(e.sight_search_enemy_cut);
            s.ang[0] *= cut(e.sight_search_upper_angle_cut);
            s.ang[1] *= cut(e.sight_search_bottom_angle_cut);
            s.ang[2] *= cut(e.sight_search_left_angle_cut);
            s.ang[3] *= cut(e.sight_search_right_angle_cut);
            s.around_rate *= e.around_sight_point_add_rate;
        }
        s.cut = s.cut.clamp(0.0, 1.0);
        s.around_rate = s.around_rate.clamp(0.0, 999.9);
        s
    }
}

/// Is `wolf` inside `cone` seen from an enemy at `me` facing `fwd` (FUN_14061bb70 /
/// FUN_14061c2a0 without the map raycast: the arena has no walls)?
pub fn in_cone(cone: &Cone, me: Vec3, fwd: Vec3, wolf: &Seen) -> bool {
    let fwd = fwd.with_y(0.0).normalize_or_zero();
    // Eye: the enemy's feet pushed back along its facing (FUN_14061af30 / ae40 / b030).
    let eye = me - fwd * cone.back * wolf.cut;
    let d = wolf.pos - eye;
    let d2 = d.length_squared();
    let begin = cone.begin * wolf.cut;
    let gate = wolf.radius + cone.dist;
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
    let far = wolf.radius + cone.dist * wolf.cut;
    d2 < far * far
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
}

#[derive(Clone, Copy, Debug)]
pub struct Normal {
    /// FIND or BATTLE.
    pub state: u8,
    /// Seconds until Wolf is forgotten while unseen (SightTargetForgetTime when seen).
    pub forget: f32,
    /// Seen at the last sight search.
    pub visible: bool,
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
    search_timer: f32,
    rng: u32,
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
        self.normal = Some(Normal { state: BATTLE, forget, visible: true });
        self.meter = None;
    }

    /// An AI sound reached the enemy: a sound target at its source (SoundTargetForgetTime).
    pub fn hear(&mut self, think: &Value, pos: Vec3, id: i64, rank: i64) {
        self.sound = Some(Sound { pos, forget: f(think, "SoundTargetForgetTime"), id, rank });
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

    /// One frame. `me` / `fwd`: the enemy's feet and facing.
    pub fn update(&mut self, think: &Value, me: Vec3, fwd: Vec3, wolf: &Seen, dt: f32) {
        let around = Cone::from(think, "around");
        // Meter (FUN_140626430, every frame).
        if let Some(mut m) = self.meter {
            m.visible = in_cone(&around, me, fwd, wolf);
            if m.visible {
                m.points += f(think, "aroundTargetIncrementPoint") * dt * wolf.around_rate;
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
        // Sight search on its interval (FUN_140625f30 / FUN_1406250a0 / FUN_140624620).
        self.search_timer -= dt;
        if self.search_timer <= 0.0 {
            self.search_timer = self.rand() * 0.2 + 0.2;
            let cone = Cone::from(think, if self.perceive { "perceive" } else { "normal" });
            let seen = in_cone(&cone, me, fwd, wolf);
            if seen {
                // FUN_14062a5e0 -> FUN_140629250: a new normal target starts in FIND.
                let forget = f(think, "SightTargetForgetTime");
                let n = self.normal.get_or_insert(Normal { state: FIND, forget, visible: true });
                n.forget = forget;
                n.visible = true;
                self.meter = None;
            } else {
                if let Some(n) = self.normal.as_mut() {
                    n.visible = false;
                }
                if self.normal.is_none() && self.meter.is_none() && in_cone(&around, me, fwd, wolf) {
                    self.meter = Some(Meter { points: 0.0, visible: true });
                }
            }
        }
        // Slot timers; the normal target keeps the memory slot on Wolf (FUN_140628860,
        // MemoryTargetForgetTime) until it is forgotten itself.
        // gap: the exe counts the sight forget down on the LastSightPos slot (FUN_140628b60);
        // here it runs on the normal target itself.
        if let Some(n) = self.normal.as_mut() {
            if !n.visible {
                n.forget -= dt;
            }
            // FIND -> BATTLE inside BattleStartDist (FUN_14062be40).
            if n.state == FIND && me.distance(wolf.pos) < f(think, "BattleStartDist") {
                n.state = BATTLE;
            }
            self.memory = Some(Spot { pos: wolf.pos, forget: f(think, "MemoryTargetForgetTime") });
            if n.forget <= 0.0 {
                self.normal = None;
            }
        } else {
            for s in [&mut self.indication, &mut self.memory] {
                if let Some(spot) = s.as_mut() {
                    spot.forget -= dt;
                    if spot.forget <= 0.0 {
                        *s = None;
                    }
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
        Seen { pos, radius: 0.4, cut: 1.0, ang: [1.0; 4], around_rate: 1.0 }
    }

    /// Enemy at the origin facing +Z.
    fn run(t: &mut Targeting, think: &Value, w: &Seen, secs: f32) {
        for _ in 0..(secs * 60.0) as usize {
            t.update(think, Vec3::ZERO, Vec3::Z, w, 1.0 / 60.0);
        }
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
