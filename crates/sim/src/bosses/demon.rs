//! The Demon of Hatred (c7020, entity 1100900) in the Ashina Outskirts, `docs/BOSSES-s.md`.
//!
//! Most of the fight runs on the shared machinery: the c7020 clips and TAE, the map's Lua AI
//! (`702000_battle.lua`), BehaviorParam attacks, bullets, the three NpcParam deathblow marks and
//! the map events. The pieces that are specific to it are small.
//!
//! - Until Wolf reaches region 1102910 the retry state of event 11105910 holds the Demon's
//!   animation disabled; the duel then freezes him and his AI ([`waiting`]).
//! - Event 11105912 and the heat-up clips drive the phases through SpEffects: `3702001` (NpcParam
//!   resident, phase 1), `3702002` (added by clip 20001 after the first mark and replacing it, see
//!   [`crate::sp_category`]) and `3702005` (the second mark).
//! - After a non-final mark the Demon plays his kneeling clip out before his AI acts again
//!   ([`recovering`]).

use crate::duel::FightEvents;
use crate::enemy::EnemySpec;

/// The Demon's character.
pub const CHR: &str = "c7020";

/// Whether `spec` is the Demon of Hatred.
pub fn owns(spec: &EnemySpec) -> bool {
    spec.chr == CHR
}

/// The retry state's wait for Wolf to enter the arena: the map has disabled the Demon's
/// animation (SetCharacterAnimationState 0) and not yet re-enabled it, or the retry flag stands
/// and the fight flag has not been set yet (event 11105910 reaches its disable on its second step).
pub fn waiting(spec: &EnemySpec, events: Option<&FightEvents>) -> bool {
    owns(spec)
        && events.is_some_and(|e| {
            e.scripts.cast_scene.animation_enabled.get(&e.boss_entity) == Some(&false)
                || (e.scripts.flag(RETRY) && !e.scripts.flag(FIGHT))
        })
}

/// Retry encounter flag (the first visit's soldiers are bypassed) and active-fight flag.
const RETRY: i32 = 11_100_901;
const FIGHT: i32 = 11_105_901;

/// The defender clips of the non-final deathblow pairs (ThrowParam 17020001 and 17020111): the
/// Demon kneels and gets up (`a000_012000` carries the HP reset at 5.7 s) and must not be
/// interrupted by his AI meanwhile.
pub fn recovering(spec: &EnemySpec, clip: Option<&str>) -> bool {
    owns(spec) && matches!(clip, Some("a000_012000" | "a000_013200"))
}

/// The Todome continuation (event 11105921 requests `Event20200` once the last mark has taken the
/// last bar) must play through: HKS's ordinary HP-zero branch would replace it with the generic
/// death clip on its first frame, so the script keeps seeing 1 HP while it runs.
pub fn execution_owns_death(spec: &EnemySpec, character: &crate::character::Character) -> bool {
    owns(spec) && character.state().starts_with("Event20200")
}

/// The offset the Demon's `GetDist` takes off the centre distance: his body radius (NpcParam
/// `chrHitRadius`, 1.4 m). The engine measures `GetDist` from the AI character's own radius
/// (FwLo's static notes; which radius field it reads is not traced), and the Demon's act tables
/// use thresholds of 2, 3, 5 and 10 m that only make sense that way.
pub fn dist_offset(spec: &EnemySpec, fighter: &crate::hits::Combatant) -> f32 {
    if owns(spec) { fighter.body_radius } else { 0.0 }
}

/// The arena starts the retry encounter's wait: the staged "fight is on" flag 11105901 (kept for a
/// flat `--enemy` duel, where no region is ever entered) is removed so event 11105910 holds the
/// Demon until Wolf reaches region 1102910. Called by `Arena::place`.
pub fn on_place(duel: &mut crate::duel::Duel) {
    if owns(&duel.spec)
        && let Some(e) = duel.events.as_mut()
    {
        e.scripts.flags.remove(&FIGHT);
    }
}

/// Wolf's death reloads the fight: the flags that belong to the fight in progress (11105902 the
/// second life, 11100900 the boss music stopped, and in the arena 11105901 the fight is on) are
/// not kept, so event 11105910 waits for Wolf's next entry again. Nothing in the map script ever
/// clears them. Off the arena 11105901 stays staged.
pub fn on_reset(spec: &EnemySpec, events: Option<&mut FightEvents>, arena: bool) {
    if let (true, Some(e)) = (owns(spec), events) {
        for flag in [11_105_902, 11_100_900] {
            e.scripts.flags.remove(&flag);
        }
        if arena {
            e.scripts.flags.remove(&FIGHT);
        }
    }
}
