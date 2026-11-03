//! Fall death of an NPC: the enemy of a [`crate::duel::Duel`], a map foe among them (#234).
//!
//! The exe has one fall-dead routine for every character (`FUN_140b975f0`, which logs
//! `SprjPlaylog_ChrFallDead`). Wolf takes its player branch, the fall return (`docs/HIT-FILTERS.md`);
//! a character that is not a player (`chr` vtable `+0x1a8` false) always takes the kill branch:
//! HP to 0 through the HP setter (`FUN_140bd64e0`, [`crate::combat::hp::Vitality::set_hp`]),
//! damage type 6 (`DEATH_FALLING`), and the NPC death flags (`FUN_1409ee7e0`).
//! Two things call it:
//!
//! - Contact with a map action event wall of hit filter 15 ([`crate::hit_filter::FALL_RETURN_FILTER`],
//!   `FUN_1409edf20`, the per-character contact handler). Since the kill branch returns 1 there,
//!   the model then fades out over [`crate::hit_filter::FALL_RETURN_FADE`] and the body is held
//!   (physics `0x92b`, then `0x92a`).
//! - The per-character update `FUN_1409f78a0`, once the physics module's `FUN_140bc2210` says the
//!   fall has gone on too long: airborne for [`FALL_DEAD_AIRBORNE`] seconds (`+0x8d0`, which
//!   counts up while not on the ground) or [`FALL_DEAD_DROP`] metres below where the body last
//!   stood (`+0x854`). No fade there. Here a body with gravity off does not count as falling.
//!
//! `c9997.hks` then plays the death: a dead NPC in a fall state plays `W_DeathStartDefault` once
//! the fall passes `HEIGHT_FORCE_DEATH` (60 m), or a landing death when it lands.

use crate::body::{Body, Ground};

/// Seconds airborne after which a character dies of the fall (`DAT_143b09f48`, read from the
/// running exe's image).
pub const FALL_DEAD_AIRBORNE: f32 = 30.0;
/// Metres below where it last stood after which a character dies of the fall (`DAT_143b09efc`).
pub const FALL_DEAD_DROP: f32 = 1500.0;

/// What started an NPC's fall death.
#[derive(Debug, Clone, PartialEq)]
pub enum FallDeath {
    /// A filter-15 event wall, by part name.
    EventWall(String),
    /// [`FALL_DEAD_AIRBORNE`] or [`FALL_DEAD_DROP`] passed.
    TooLong,
}

/// One NPC's fall-death state.
#[derive(Debug, Clone, Default)]
pub struct NpcFall {
    /// Seconds since the body left the ground.
    airborne: f32,
    /// Height where the body last stood.
    stood: Option<f32>,
    /// Seconds into the model fade after a filter-15 event wall.
    fade: Option<f32>,
    /// Where the body is held once the fade has ended.
    held: Option<[f32; 3]>,
    /// What killed it, once something has.
    pub death: Option<FallDeath>,
}

impl NpcFall {
    /// The model's opacity: 1 to 0 over the fade after a filter-15 event wall, else 1.
    pub fn opacity(&self) -> f32 {
        self.fade.map_or(1.0, |t| {
            (1.0 - t / crate::hit_filter::FALL_RETURN_FADE).clamp(0.0, 1.0)
        })
    }

    /// Advances one step of `dt` after the body moved from `before`. Holds the body once the
    /// fade has ended, and returns the cause the step the NPC should die of its fall.
    pub fn step(
        &mut self,
        body: &mut Body,
        ground: &dyn Ground,
        before: [f32; 3],
        dt: f32,
    ) -> Option<FallDeath> {
        if let Some(at) = self.held {
            body.position = at;
            body.velocity = [0.0; 3];
            return None;
        }
        if let Some(t) = &mut self.fade {
            *t += dt;
            if *t >= crate::hit_filter::FALL_RETURN_FADE {
                self.held = Some(body.position);
                body.velocity = [0.0; 3];
            }
            return None;
        }
        let mut death = ground
            .trigger_contact(before, body.position)
            .filter(|c| c.hit_filter == crate::hit_filter::FALL_RETURN_FILTER)
            .map(|c| {
                self.fade = Some(0.0);
                FallDeath::EventWall(c.part)
            });
        // A body with gravity off (a flying bird, a floating dragon) is not falling; whether the
        // exe's timer skips such bodies was not traced.
        if body.grounded || !body.gravity_enabled {
            self.airborne = 0.0;
            self.stood = Some(body.position[1]);
        } else {
            self.airborne += dt;
            let dropped = self
                .stood
                .is_some_and(|y| y - body.position[1] >= FALL_DEAD_DROP);
            if death.is_none()
                && self.death.is_none()
                && (self.airborne >= FALL_DEAD_AIRBORNE || dropped)
            {
                death = Some(FallDeath::TooLong);
            }
        }
        if death.is_some() {
            self.death.clone_from(&death);
        }
        death
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A foe that stays airborne with no event wall below dies once, the step it has been
    /// airborne [`FALL_DEAD_AIRBORNE`] seconds.
    #[test]
    fn a_fall_that_goes_on_too_long_kills_once() {
        let ground = crate::body::FlatGround { height: -1000.0 };
        let mut body = Body {
            grounded: true,
            ..Default::default()
        };
        let at = body.position;
        let mut fall = NpcFall::default();
        let dt = 0.5;
        assert_eq!(fall.step(&mut body, &ground, at, dt), None);
        body.grounded = false;
        let deaths: Vec<_> = (1..=80)
            .filter_map(|i| Some((i, fall.step(&mut body, &ground, at, dt)?)))
            .collect();
        assert_eq!(deaths, [(60, FallDeath::TooLong)]);
        assert_eq!(fall.opacity(), 1.0, "only an event wall fades the model");
    }
}
