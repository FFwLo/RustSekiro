//! A stand-in NPC brain until the game's own AI runs.
//!
//! Sekiro's enemies think in Lua: `script/aicommon.luabnd` (shared goals such as approach,
//! attack, guard) and `script/ai/out/bin/<chr>_battle.lua`-style logic files per enemy, compiled
//! Lua 5.0 that DSLuaDecompiler also handles. Those goals end up as the same engine interface
//! this brain drives: the requested action (`env(106)`, an animation-like code), movement speed
//! and direction (`MoveSpeedLevel`, `MoveAngle`) and the AI-state SpEffects the NPC script reads
//! (`SP_EFFECT_REF_AI_BATTLE` = 1000003). Replacing [`SimpleBrain`] with the real AI is a later
//! pass; its output type, [`NpcControl`], stays.

use crate::character::{Character, NpcControl};

/// Behaviour reference the NPC script reads as "in battle" (`SP_EFFECT_REF_AI_BATTLE`).
pub const REF_AI_BATTLE: i32 = 1_000_003;
/// The NPC script's guard action code (`ACTION_TYPE_GUARD`).
pub const ACTION_GUARD: i32 = 9910;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Phase {
    Approach,
    Ready,
    Attack(f32),
    Guard(f32),
}

/// Approach to striking range, then attack on a timer and sometimes guard.
#[derive(Debug, Clone)]
pub struct SimpleBrain {
    /// Attack codes to pick from (only those the character has are used).
    pub attacks: Vec<i32>,
    /// Distance at which it stops approaching, metres.
    pub strike_range: f32,
    /// Seconds between attack decisions.
    pub interval: (f32, f32),
    /// Chance of guarding instead of attacking.
    pub guard_chance: f32,
    phase: Phase,
    next_decision: f32,
    rng: u64,
}

impl Default for SimpleBrain {
    fn default() -> Self {
        Self {
            attacks: vec![3000, 3001, 3002, 3003, 3005],
            strike_range: 2.2,
            interval: (2.0, 3.5),
            guard_chance: 0.25,
            phase: Phase::Approach,
            next_decision: 1.5,
            rng: 0x1234_5678_9abc_def1,
        }
    }
}

impl SimpleBrain {
    fn random(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng >> 40) as f32 / (1u64 << 24) as f32
    }

    /// Writes this step's control into `npc.npc`, toward `target`.
    pub fn think(&mut self, npc: &mut Character, target: [f32; 3], dt: f32) {
        let p = npc.body.position;
        let dist = ((target[0] - p[0]).powi(2) + (target[2] - p[2]).powi(2)).sqrt();
        let to_target = npc.yaw_to(target);
        self.next_decision -= dt;
        let attacks: Vec<i32> = self
            .attacks
            .iter()
            .copied()
            .filter(|a| npc.behavior.env.existing_anims.contains(&(*a as i64)))
            .collect();
        let mut c = NpcControl {
            face_yaw: Some(to_target),
            ..NpcControl::default()
        };
        c.refs.insert(REF_AI_BATTLE);
        self.phase = match self.phase {
            Phase::Attack(t) if t > 0.0 => {
                c.action = if t > 0.25 { npc.npc.action } else { 0 };
                Phase::Attack(t - dt)
            }
            Phase::Guard(t) if t > 0.0 => {
                c.action = ACTION_GUARD;
                Phase::Guard(t - dt)
            }
            _ if dist > self.strike_range => {
                c.move_level = if dist > 6.0 { 1.0 } else { 0.5 };
                c.move_yaw = Some(to_target);
                Phase::Approach
            }
            _ if self.next_decision <= 0.0 && !attacks.is_empty() => {
                let (lo, hi) = self.interval;
                self.next_decision = lo + (hi - lo) * self.random();
                if self.random() < self.guard_chance {
                    c.action = ACTION_GUARD;
                    Phase::Guard(1.5)
                } else {
                    let i = (self.random() * attacks.len() as f32) as usize;
                    c.action = attacks[i.min(attacks.len() - 1)];
                    Phase::Attack(0.5)
                }
            }
            _ => Phase::Ready,
        };
        npc.npc = c;
    }
}
