//! The damage type codes the engine stores for the player script (env 202).
//!
//! The defender's code is chosen in `b6bd40` and the attacker's (when its attack was guarded) in
//! `b698d0`; both write the action module's +0x5C word, which env 202 reports (read from code).

use super::hit::HitOutcome;

/// Defender-side damage type, in the engine's order of precedence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefenderReaction {
    /// 2: the hit killed.
    Death,
    /// 1004: guard break by a blast-level hit (damage level 4, 7 or 10).
    GuardBreakBlast,
    /// 1005: guard break by a fling-level hit (damage level 6).
    GuardBreakFling,
    /// 1001: guard break.
    GuardBreak,
    /// 12: blocked by an armoured body part (see [`super::hit::resolve_part_hit`]).
    GuardClash,
    /// 1028: guarded or deflected, and the attacker's posture broke from it with
    /// `doesBreakRepelStamDamage` set.
    GuardAttackerBroken,
    /// 3: guarded (block or deflect; the script tells them apart with behaviour reference 203).
    Guard,
    /// 1027: posture break on a direct hit (also reached through HP with deathblows left).
    DamageBreak,
    /// 99999 in the engine: an ordinary hit, whose reaction is then chosen by damage level.
    Normal,
}

impl DefenderReaction {
    /// The number env 202 reports.
    pub fn code(self) -> i32 {
        match self {
            Self::Death => 2,
            Self::GuardBreakBlast => 1004,
            Self::GuardBreakFling => 1005,
            Self::GuardBreak => 1001,
            Self::GuardClash => 12,
            Self::GuardAttackerBroken => 1028,
            Self::Guard => 3,
            Self::DamageBreak => 1027,
            Self::Normal => 99999,
        }
    }
}

/// What the defender's damage module knows when it picks the reaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DefenderFacts {
    pub outcome: HitOutcome,
    pub dead: bool,
    /// The block emptied the guard (record +0x1C4).
    pub guard_broken: bool,
    /// The guard-break reaction may play (record +0x1C5, the result of a character virtual).
    pub guard_break_reaction: bool,
    /// AtkParam damage level of the hit (record +0x24).
    pub damage_level: u32,
    /// The direct hit broke posture or reached 1 HP with deathblows left (record +0x1C6).
    pub broke: bool,
    /// The attacker's posture breaks from this contact (record +0x1CF).
    pub attacker_broke: bool,
    pub does_break_repel_stam_damage: bool,
}

/// Picks the defender's damage type. Read from code (`b6bd40`); two earlier special cases (codes
/// 7 and 9 for scripted states, and the record kinds 63 and 64) are left out.
pub fn defender_reaction(f: DefenderFacts) -> DefenderReaction {
    if f.dead {
        return DefenderReaction::Death;
    }
    if f.guard_broken && f.guard_break_reaction {
        if f.damage_level < 11 && (0x490u32 >> f.damage_level) & 1 != 0 {
            return DefenderReaction::GuardBreakBlast;
        }
        if f.damage_level == 6 {
            return DefenderReaction::GuardBreakFling;
        }
    }
    let attacker_broken = f.attacker_broke && f.does_break_repel_stam_damage;
    match f.outcome {
        HitOutcome::Deflect => {
            if attacker_broken {
                DefenderReaction::GuardAttackerBroken
            } else {
                DefenderReaction::Guard
            }
        }
        _ if f.guard_broken => DefenderReaction::GuardBreak,
        HitOutcome::Block => {
            if attacker_broken {
                DefenderReaction::GuardAttackerBroken
            } else {
                DefenderReaction::Guard
            }
        }
        HitOutcome::Hit => {
            if f.broke {
                DefenderReaction::DamageBreak
            } else {
                DefenderReaction::Normal
            }
        }
    }
}

/// Attacker-side damage type when its attack touched a guard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttackerReaction(pub i32);

/// Picks the attacker's code (`b698d0`). `category` is the attack's BehaviorParam `category`
/// (2, 6, 7 and 8 have their own codes); `clash` means the hit was guarded by an armoured body part (record +0x1CE).
///
/// Read from code:
/// - blocked: 1017, or 1018 / 1019 / 1020 / 1021 for categories 2 / 6 / 7 / 8;
/// - deflected: 1000, or 1003 / 1008 / 1009 / 1010 for those categories (1022 / 1023 / 1024 /
///   1025 / 1026 when an armoured part did it);
/// - deflected and the attacker's posture broke with `doesBreakRepelStamDamage`: 1033.
///
/// Returns `None` for a direct hit or a guard break, where the attacker gets no code.
pub fn attacker_reaction(
    outcome: HitOutcome,
    guard_broken: bool,
    category: u8,
    clash: bool,
    attacker_broke: bool,
    does_break_repel_stam_damage: bool,
) -> Option<AttackerReaction> {
    if guard_broken {
        return None;
    }
    let code = match outcome {
        HitOutcome::Hit => return None,
        HitOutcome::Block => match category {
            2 => 1018,
            6 => 1019,
            7 => 1020,
            8 => 1021,
            _ => 1017,
        },
        HitOutcome::Deflect => match (category, clash) {
            (2, false) => 1003,
            (6, false) => 1008,
            (7, false) => 1009,
            (8, false) => 1010,
            (2, true) => 1023,
            (6, true) => 1024,
            (7, true) => 1025,
            (8, true) => 1026,
            (_, true) => 1022,
            _ => 1000,
        },
    };
    if attacker_broke && does_break_repel_stam_damage {
        return Some(AttackerReaction(1033));
    }
    Some(AttackerReaction(code))
}
