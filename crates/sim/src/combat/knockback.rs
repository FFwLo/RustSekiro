//! Knockback and hit stop: how a connecting attack pushes the defender and freezes both sides.
//!
//! Knockback, per contact (distances from AtkParam, timing from KnockBackParam; the velocity
//! profile and the cut rates were traced by FwLo's RustSekiro, `docs/kb/damage.md`):
//!
//! ```text
//! dist = AtkParam knockbackDist_{DirectHit, Guard, JustGuard}   (hit, block, deflect)
//!        * (1 - NpcParam knockbackRate_vs{Player,Enemy}_* / 100)  (NPC defenders only)
//!        * defender's TAE 226 percent / 100                       (1 when none is active)
//! v    = dist / (cont + dec / 2)
//! speed(t) = v for t < cont, then falls linearly to 0 over dec
//! ```
//!
//! `cont` and `dec` are the `<class>_ContTime` / `<class>_DecTime` columns of the defender's
//! KnockBackParam row: NpcParam `knockbackParamId` for NPCs (0 on every row), the
//! EquipParamProtector `knockbackParamId` of Wolf's armour (1, "For PC"). The class comes from the
//! outcome and the attack's `dmgLevel` ([`KnockbackClass::for_hit`], inferred). The push moves
//! the defender away from the attacker on the ground plane, through its body collision, so walls
//! stop it.
//!
//! Hit stop (code, `b9cd90`, `b9cfb0`, `b9d1b0`; see `docs/COMBAT-RULES.md`, "Hit stop"): when
//! an attack connects, each side may get a hit stop that scales its whole update clock. The
//! attacker's length is AtkParam `hitStopTime` times a scale from the defender's NpcParam
//! `hitStopType` (0 none, 1 half, 2 full; a player defender counts as full); the defender's is
//! `hitStopTime_Defencer` when its NpcParam `hitStopType_Defencer` is 1 (a player defender always
//! takes it). A hit stop holds the clock at zero for the engine's fixed 0.06 s, then eases it back
//! to full speed over the hit stop length, quadratically. Wolf's sword rows have 0.1 s for both,
//! but the soldier and the Owl have type 0 and 0, so neither side stops in those fights.

use super::hit::HitOutcome;
use sekiro_formats::param::Table;
use std::collections::HashMap;

/// KnockBackParam column families, in the param's own order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KnockbackClass {
    DamageMin,
    DamageS,
    DamageM,
    DamageL,
    DamageBlowS,
    DamageBlowM,
    DamageStrike,
    DamageUppercut,
    DamagePush,
    DamageBreath,
    DamageHeadShot,
    GuardS,
    GuardL,
    GuardLL,
    GuardBreak,
}

impl KnockbackClass {
    pub const ALL: [Self; 15] = [
        Self::DamageMin,
        Self::DamageS,
        Self::DamageM,
        Self::DamageL,
        Self::DamageBlowS,
        Self::DamageBlowM,
        Self::DamageStrike,
        Self::DamageUppercut,
        Self::DamagePush,
        Self::DamageBreath,
        Self::DamageHeadShot,
        Self::GuardS,
        Self::GuardL,
        Self::GuardLL,
        Self::GuardBreak,
    ];

    /// The KnockBackParam field prefix (`damage_S` for `damage_S_ContTime`).
    pub fn field_prefix(self) -> &'static str {
        match self {
            Self::DamageMin => "damage_Min",
            Self::DamageS => "damage_S",
            Self::DamageM => "damage_M",
            Self::DamageL => "damage_L",
            Self::DamageBlowS => "damage_BlowS",
            Self::DamageBlowM => "damage_BlowM",
            Self::DamageStrike => "damage_Strike",
            Self::DamageUppercut => "damage_Uppercut",
            Self::DamagePush => "damage_Push",
            Self::DamageBreath => "damage_Breath",
            Self::DamageHeadShot => "damage_HeadShot",
            Self::GuardS => "guard_S",
            Self::GuardL => "guard_L",
            Self::GuardLL => "guard_LL",
            Self::GuardBreak => "guardBrake",
        }
    }

    /// The class of a contact's push, from its outcome and the attack's `dmgLevel`
    /// (`DAMAGE_LEVEL_*` in the HKS: 0 none, 1 small, 2 middle, 3 large, 4 ex-large, 5 push,
    /// 6 fling, 7 small blow, 8 minimum, 9 upper, 10 ex-blast, 11 breath).
    ///
    /// Inferred, not traced: direct hits match the level's name to a damage column; guarded
    /// contacts use the size of the guard reaction the player script plays for that level
    /// (small, middle and push are S, large and the blows L, ex-blast and breath LL), the same
    /// mapping FwLo's RustSekiro uses. The engine's own selection (`b6bfe0`) is not read yet.
    pub fn for_hit(outcome: HitOutcome, damage_level: i32) -> Self {
        match outcome {
            HitOutcome::Hit => match damage_level {
                2 => Self::DamageM,
                3 => Self::DamageL,
                4 | 10 => Self::DamageBlowM,
                5 => Self::DamagePush,
                6 => Self::DamageStrike,
                7 => Self::DamageBlowS,
                8 => Self::DamageMin,
                9 => Self::DamageUppercut,
                11 => Self::DamageBreath,
                _ => Self::DamageS,
            },
            HitOutcome::Block | HitOutcome::Deflect => match damage_level {
                3 | 4 | 6 | 7 | 9 => Self::GuardL,
                10 | 11 => Self::GuardLL,
                _ => Self::GuardS,
            },
        }
    }
}

/// Hold and decay time of one KnockBackParam column pair, seconds.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct KnockbackTiming {
    pub cont: f32,
    pub dec: f32,
}

/// KnockBackParam, by row id.
#[derive(Debug, Clone, Default)]
pub struct KnockbackTable {
    rows: HashMap<i32, HashMap<KnockbackClass, KnockbackTiming>>,
}

impl KnockbackTable {
    pub fn from_table(table: &Table) -> Self {
        let rows = table
            .rows()
            .map(|r| {
                let timing = KnockbackClass::ALL
                    .iter()
                    .map(|&c| {
                        let f = |suffix: &str| {
                            r.f32(&format!("{}_{suffix}", c.field_prefix()))
                                .unwrap_or(0.0)
                        };
                        (
                            c,
                            KnockbackTiming {
                                cont: f("ContTime"),
                                dec: f("DecTime"),
                            },
                        )
                    })
                    .collect();
                (r.id(), timing)
            })
            .collect();
        Self { rows }
    }

    pub fn timing(&self, row: i32, class: KnockbackClass) -> Option<KnockbackTiming> {
        self.rows.get(&row)?.get(&class).copied()
    }
}

/// Index of an outcome into the `[direct hit, guard, just guard]` triples of AtkParam and
/// NpcParam.
pub fn outcome_index(outcome: HitOutcome) -> usize {
    match outcome {
        HitOutcome::Hit => 0,
        HitOutcome::Block => 1,
        HitOutcome::Deflect => 2,
    }
}

/// Push distance in metres: the attack's distance, less the defender's cut percent, times the
/// defender's TAE 226 scale (`None` is 100 %).
pub fn knockback_distance(dist: f32, cut_percent: u8, tae_scale: Option<f32>) -> f32 {
    dist * (1.0 - cut_percent.min(100) as f32 / 100.0) * tae_scale.unwrap_or(1.0)
}

/// A push in progress.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Knockback {
    /// Unit direction on the ground plane (x, z).
    pub direction: [f32; 2],
    /// Speed while holding, metres per second.
    pub speed: f32,
    pub timing: KnockbackTiming,
    /// Seconds since the push began.
    pub elapsed: f32,
}

impl Knockback {
    /// A push of `distance` metres along `direction`, or `None` when there is nothing to push
    /// (no distance, no direction or a zero-length timing).
    pub fn start(direction: [f32; 2], distance: f32, timing: KnockbackTiming) -> Option<Self> {
        let len = (direction[0] * direction[0] + direction[1] * direction[1]).sqrt();
        let span = timing.cont + timing.dec * 0.5;
        if distance <= 0.0 || len < 1e-6 || span <= 0.0 {
            return None;
        }
        Some(Self {
            direction: [direction[0] / len, direction[1] / len],
            speed: distance / span,
            timing,
            elapsed: 0.0,
        })
    }

    /// Distance covered from the start to `t` seconds (the exact integral of the speed curve).
    fn covered(&self, t: f32) -> f32 {
        let KnockbackTiming { cont, dec } = self.timing;
        let t = t.max(0.0);
        if t <= cont {
            return self.speed * t;
        }
        let d = (t - cont).min(dec);
        let decay = if dec > 0.0 {
            self.speed * (d - d * d / (2.0 * dec))
        } else {
            0.0
        };
        self.speed * cont + decay
    }

    /// Total distance of the push.
    pub fn distance(&self) -> f32 {
        self.covered(self.timing.cont + self.timing.dec)
    }

    /// Advances by `dt` seconds and returns the ground-plane displacement for the step.
    pub fn advance(&mut self, dt: f32) -> [f32; 2] {
        let before = self.covered(self.elapsed);
        self.elapsed += dt.max(0.0);
        let step = self.covered(self.elapsed) - before;
        [self.direction[0] * step, self.direction[1] * step]
    }

    pub fn finished(&self) -> bool {
        self.elapsed >= self.timing.cont + self.timing.dec
    }
}

/// The full stop at the start of every hit stop, seconds (the engine's default at `a51800`
/// +0x130; only a debug menu changes it).
pub const HIT_STOP_HOLD: f32 = 0.06;
/// TAE `ChrActionFlag` 41 ("HasSpace_Hks305" in the community template): on the defender it
/// forces both full hit stops for non-arrow attacks (the TAE handler `b51b70` sets ActionFlag
/// +0x70 bit 7, read by `b60f10`). Wolf, the soldier and the Owl never set it.
pub const FLAG_FORCE_HIT_STOP: i32 = 41;
/// The attacker scale for an NPC defender with `hitStopType` 1 (`a51800` +0x124).
pub const HIT_STOP_WEAK_RATE: f32 = 0.5;

/// A character's hit stop: a clock scale that holds at zero for [`HIT_STOP_HOLD`], then rises
/// back to one as `(elapsed / length)^2` over the hit stop's length (`b9d1b0`; its ease-in phase
/// has length 0 by default and is left out).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct HitStop {
    phase: HitStopPhase,
    /// Seconds left in the current phase.
    left: f32,
    /// The ease-out length: the scaled AtkParam time.
    length: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum HitStopPhase {
    #[default]
    Idle,
    Hold,
    Ease,
}

impl HitStop {
    /// Leftovers below this are float noise from summing steps.
    const EPSILON: f32 = 1e-5;

    /// Starts a hit stop easing out over `seconds`. A new one replaces one already running, as the
    /// engine's start (`b9dad0`) resets the state; zero or less starts nothing.
    pub fn start(&mut self, seconds: f32) {
        if seconds > 0.0 {
            *self = Self {
                phase: HitStopPhase::Hold,
                left: HIT_STOP_HOLD,
                length: seconds,
            };
        }
    }

    /// Runs `dt` of real time and returns how far the character's own clock advances.
    pub fn split(&mut self, dt: f32) -> f32 {
        match self.phase {
            HitStopPhase::Idle => dt,
            HitStopPhase::Hold => {
                self.left -= dt;
                if self.left <= Self::EPSILON {
                    self.phase = HitStopPhase::Ease;
                    self.left = self.length;
                }
                0.0
            }
            HitStopPhase::Ease => {
                self.left -= dt;
                let eased = (1.0 - self.left / self.length).clamp(0.0, 1.0);
                if self.left <= Self::EPSILON {
                    *self = Self::default();
                    return dt;
                }
                eased * eased * dt
            }
        }
    }

    pub fn active(&self) -> bool {
        self.phase != HitStopPhase::Idle
    }

    /// Real seconds until the clock runs at full speed again.
    pub fn remaining(&self) -> f32 {
        match self.phase {
            HitStopPhase::Idle => 0.0,
            HitStopPhase::Hold => self.left + self.length,
            HitStopPhase::Ease => self.left,
        }
    }
}

/// The attacker's hit stop scale against a defender with NpcParam `hitStopType` `kind` (`None`
/// for the player, whom the engine treats as type 2). Type 3 depends on an attacker action state
/// that is not traced (no NpcParam row uses it) and is treated as none.
pub fn attacker_hit_stop_scale(kind: Option<u8>) -> f32 {
    match kind {
        None | Some(2) => 1.0,
        Some(1) => HIT_STOP_WEAK_RATE,
        _ => 0.0,
    }
}

/// The defender's hit stop scale for NpcParam `hitStopType_Defencer` `kind` (`None` for the
/// player, who always takes it).
pub fn defender_hit_stop_scale(kind: Option<u8>) -> f32 {
    match kind {
        None | Some(1) => 1.0,
        _ => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_covers_its_distance_with_hold_then_linear_decay() {
        let timing = KnockbackTiming {
            cont: 0.06,
            dec: 0.3,
        };
        let mut kb = Knockback::start([3.0, 4.0], 1.0, timing).unwrap();
        // v = dist / (cont + dec / 2) = 1 / 0.21.
        assert!((kb.speed - 1.0 / 0.21).abs() < 1e-4);
        assert!((kb.distance() - 1.0).abs() < 1e-5);
        let mut total = [0.0f32; 2];
        let mut steps = 0;
        while !kb.finished() {
            let d = kb.advance(1.0 / 60.0);
            total[0] += d[0];
            total[1] += d[1];
            steps += 1;
        }
        assert_eq!(steps, 22);
        assert!((total[0] - 0.6).abs() < 1e-4 && (total[1] - 0.8).abs() < 1e-4);
        // During the hold the speed is constant: the first step moves v * dt.
        let mut kb = Knockback::start([1.0, 0.0], 1.0, timing).unwrap();
        assert!((kb.advance(0.05)[0] - 0.05 / 0.21).abs() < 1e-5);
    }

    #[test]
    fn nothing_to_push() {
        let timing = KnockbackTiming {
            cont: 0.0,
            dec: 0.0,
        };
        assert!(Knockback::start([1.0, 0.0], 1.0, timing).is_none());
        let timing = KnockbackTiming {
            cont: 0.1,
            dec: 0.1,
        };
        assert!(Knockback::start([1.0, 0.0], 0.0, timing).is_none());
        assert!(Knockback::start([0.0, 0.0], 1.0, timing).is_none());
    }

    #[test]
    fn distance_cut_and_tae_scale() {
        assert_eq!(knockback_distance(2.0, 0, None), 2.0);
        assert!((knockback_distance(2.0, 25, Some(0.5)) - 0.75).abs() < 1e-6);
        assert_eq!(knockback_distance(2.0, 100, None), 0.0);
    }

    #[test]
    fn hit_stop_holds_then_eases_out() {
        let dt = 1.0 / 60.0;
        let mut stop = HitStop::default();
        stop.start(0.1);
        assert!((stop.remaining() - 0.16).abs() < 1e-6);
        let advanced: Vec<f32> = (0..12).map(|_| stop.split(dt)).collect();
        // 0.06 s held: the step that crosses it is still held (four steps at 60 Hz).
        assert!(advanced[..4].iter().all(|&a| a == 0.0), "{advanced:?}");
        // Then (elapsed / 0.1)^2 of each step over six steps, then full speed.
        for (k, &a) in advanced[4..10].iter().enumerate() {
            let e = (k + 1) as f32 / 6.0;
            assert!((a - e * e * dt).abs() < 1e-5, "step {k}: {advanced:?}");
        }
        assert!(advanced[10..].iter().all(|&a| a == dt), "{advanced:?}");
        assert!(!stop.active());
    }

    #[test]
    fn a_new_hit_stop_restarts_and_zero_starts_nothing() {
        let dt = 1.0 / 60.0;
        let mut stop = HitStop::default();
        stop.start(0.0);
        assert!(!stop.active());
        stop.start(0.1);
        for _ in 0..6 {
            stop.split(dt);
        }
        stop.start(0.1);
        assert_eq!(stop.split(dt), 0.0);
        assert!((stop.remaining() - (0.16 - dt)).abs() < 1e-5);
    }

    #[test]
    fn hit_stop_scales_by_type() {
        assert_eq!(attacker_hit_stop_scale(None), 1.0);
        assert_eq!(attacker_hit_stop_scale(Some(0)), 0.0);
        assert_eq!(attacker_hit_stop_scale(Some(1)), 0.5);
        assert_eq!(attacker_hit_stop_scale(Some(2)), 1.0);
        assert_eq!(attacker_hit_stop_scale(Some(3)), 0.0);
        assert_eq!(defender_hit_stop_scale(None), 1.0);
        assert_eq!(defender_hit_stop_scale(Some(0)), 0.0);
        assert_eq!(defender_hit_stop_scale(Some(1)), 1.0);
    }
}
