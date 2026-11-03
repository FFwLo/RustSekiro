//! Perilous attacks (危, "kiken"): the warning and the counters on Wolf's side.
//!
//! The warning, as the data has it (worked out for the soldier, c1010, and the Great Shinobi
//! Owl, c5060; `docs/COMBAT-RULES.md`, "Perilous attacks"):
//!
//! 1. A perilous attack's clip fires a TAE `BulletBehavior` with a judge from 980 to 989 a few
//!    tenths of a second before the blow (soldier sweep `a000_003020` at 0.2 s, Owl sweep
//!    `a000_003004` at 0.0 s, Owl Shinobi stab `a000_003033` at 0.667 s).
//! 2. Its BehaviorParam row is a "dummy bullet, dangerous attack sign" (soldier 10001600, Owl
//!    50601980 to 50601987): 2 m radius, 0.1 s, AtkParam 8 (no damage), `EmittePosType` 7 (made
//!    at the target, [`crate::bullets::BulletDef::emit_at_target`]), not made farther than
//!    `generationMaxDistWhenOwner` from the attacker (5 m soldier, 9 m Owl), and SpEffect 211000.
//! 3. SpEffect 211000 "dangerous enemy attack VFX" (0.1 s) lands on the player only
//!    (`effectTargetPCOnlyTarget`, `effectTargetOpposeTarget`) and shows `vfxId` 48100.
//! 4. SpEffectVfxParam 48100 "漢字_「危」" (the kanji) plays effect 600035 at the player's dummy
//!    poly 220 (`Spine2`): the red symbol appears on Wolf, not over the attacker.
//!
//! The kind of attack (what to do about it) is not in the warning: FwLo's RustSekiro maps judges
//! 980 / 982 / 983 to sweep / thrust / grab, which holds for the Ashina General (c1020) but not in
//! general (the Owl's sweeps use 980, 985 and 986). Here it comes from the AtkParam of the clip's
//! next attack window ([`crate::hits::AttackData::perilous`]).

use crate::character::Character;
use crate::hits::{Combatant, Perilous};
use crate::tae::TaeKind;

/// Forward-step TAE ChrActionFlag 125, active for the first 0.4 seconds.
pub const FLAG_MIKIRI: i32 = 125;

/// The counter pair holds until Wolf leaves the counter animation through its own cancels.
pub struct Mikiri {
    pub row: crate::deathblow::ThrowRow,
    pub slide: crate::deathblow::AbsorbSlide,
    pub started: bool,
}

/// Counter-only BehaviorParam category 6 hitboxes carry posture but no damaging attack.
pub fn counter_dummy(attack: &crate::hits::AttackData) -> bool {
    attack.category == 6
        && attack.atk_type == 0
        && attack.sp_effects.is_empty()
        && attack.mikiri_posture > 0
        && attack.atk_phys == 0
        && attack
            .profile
            .as_ref()
            .is_some_and(|p| p.attack_power == [0.0; 5] && p.behavior_id == 0)
}

/// SpEffectVfxParam row 48100, "漢字_「危」": the perilous-attack kanji.
pub const KANJI_VFX: i32 = 48100;

/// Wolf's dummy poly the kanji effect plays at (SpEffectVfxParam 48100 `initDmyId`, on `Spine2`).
pub const KANJI_DUMMY: i16 = 220;

/// SpEffect `stateInfo` 912, "jump kick effective": SpEffect 400200 (and the tutorial 310601)
/// that the perilous sweeps put on their attacker from the wind-up to the recovery. It carries
/// `defAntiGroundStaminaDmgRate` 8 (40 on the tutorial one): the attacker takes eight times the
/// posture damage from anti-ground attacks (`staminaPhysicsAttribute` 7), and the only Wolf
/// attacks of that kind are the air kick (AtkParam_Pc 901) and the kick combat art.
pub const STATE_INFO_JUMP_KICK_EFFECTIVE: u16 = 912;

/// `staminaPhysicsAttribute` 7, anti-ground (`docs/COMBAT-RULES.md`, posture damage).
pub const STAMINA_ANTI_GROUND: u8 = 7;

/// A perilous-attack warning that reached Wolf this step.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PerilousWarning {
    /// Who attacks (1, the enemy).
    pub attacker: usize,
    /// The warning bullet's behaviour judge (980 to 989).
    pub judge: i32,
    /// What the attack is, from its AtkParam; `None` when the clip has no perilous window after
    /// the warning.
    pub kind: Option<Perilous>,
}

/// The perilous kind of the first attack window in `attacker`'s full-body clip that has not
/// closed yet (the blow the warning announces).
pub fn upcoming_kind(attacker: &Character, fighter: &Combatant) -> Option<Perilous> {
    let clip = attacker.behavior.runtime.main_clip()?;
    let anim = attacker.tae.anim(&clip.animation)?;
    let mut windows: Vec<_> = anim
        .events
        .iter()
        .filter(|e| e.end > clip.time)
        .filter_map(|e| match e.kind {
            TaeKind::Attack { judge, .. } => Some((e.start, judge)),
            _ => None,
        })
        .collect();
    windows.sort_by(|a, b| a.0.total_cmp(&b.0));
    windows
        .into_iter()
        .find_map(|(_, judge)| fighter.attacks.get(&judge)?.perilous())
}
