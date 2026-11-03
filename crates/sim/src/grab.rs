//! Enemy throws on Wolf ("grabs"), such as the Great Shinobi Owl's 崩し投げ and 見切り投げ.
//!
//! The data trail (see `docs/OWL.md`, "Grabs", and `docs/DEATHBLOW.md`, "Enemy throws on Wolf"):
//!
//! 1. The enemy's attack clip opens an ordinary TAE `AttackBehavior` window whose AtkParam row has
//!    `throwFlag` 1: a throw start. Its capsule is found like any hit ([`crate::hits`]), but it
//!    grabs instead of hitting: guarding or deflecting does not stop it, a step's grab i-frames
//!    (TAE 954 type 7), a jump or being out of reach do ([`crate::hits::can_be_grabbed`]).
//! 2. The contact picks a ThrowParam row by the AtkParam `throwTypeId`
//!    ([`ThrowTable::npc_row`]) and checks its range and angles; the row pairs the enemy's throw
//!    clip (`atkAnimId`, offset `atkAnimOffset`) with Wolf's thrown clip (`defAnimId`, offset
//!    `defAnimOffset`, Wolf's per-enemy clips in `chr/c0000_cXXXX.anibnd`).
//! 3. Both graphs get the throw events (`W_ThrowAtk<atk>`, `W_ThrowDef<def>`); Wolf is absorbed
//!    onto the enemy's `atkSorbDmyId` over `adsrobModelPosInterpolationTime` as the pair stands
//!    when the clips start ([`AbsorbSlide`], shared with deathblows), then each plays its own root
//!    motion. Wolf's thrown clip turns off steering and every cancel until late, and its TAE 952
//!    keeps every other attack off him.
//! 4. The damage lands at the `ThrowAttackBehavior` (TAE 304) events of the enemy's throw clip,
//!    each naming a `throwFlag` 2 AtkParam row (HP and posture as a direct hit, no reaction).
//! 5. A lethal throw branches Wolf to the death clip (`defAnimId + 1`) at the thrown clip's
//!    `ChrActionFlag` 69, as the enemy's deathblow clips do; otherwise he gets up in the clip and
//!    the pair ends when he leaves it.

use crate::deathblow::{AbsorbSlide, AbsorbStep, Absorbed, DummyFrame, Placement, ThrowRow};
use crate::tae::{TaeDb, TaeKind};

/// Where an enemy throw stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrabPhase {
    /// Both play the paired clips; Wolf is held (no push-apart, no knockback).
    Held,
    /// Wolf left his thrown clip (got up, or died); nothing is held any more.
    Done,
}

/// What changed this step.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GrabStep {
    /// Absorb motion for Wolf this step (first `adsrobModelPosInterpolationTime` seconds).
    pub absorb: Option<AbsorbStep>,
    /// `ThrowAttackBehavior` judges of the enemy's clip that landed this step.
    pub hits: Vec<i32>,
    /// Branch Wolf to the death clip this step.
    pub death: bool,
}

/// One enemy throw on Wolf in progress. The enemy is the throw's attacker, Wolf its defender.
#[derive(Debug, Clone)]
pub struct EnemyThrow {
    pub row: ThrowRow,
    pub phase: GrabPhase,
    /// The enemy's clip, e.g. `a000_004100`.
    pub attacker_clip: String,
    /// Wolf's clip, e.g. `a221_600110`.
    pub defender_clip: String,
    /// `ThrowAttackBehavior` events of the attacker clip: (start time, behaviour judge).
    pub hits: Vec<(f32, i32)>,
    /// The thrown clip's `ChrActionFlag` 69 time, where a lethal throw goes to the death clip.
    pub death_branch: Option<f32>,
    /// Seconds since the throw began.
    pub time: f32,
    /// Wolf's HP reached 0 from the throw (set by the world after applying a hit).
    pub lethal: bool,
    /// Wolf went to the death clip.
    pub branched: bool,
    slide: AbsorbSlide,
    next_hit: usize,
    attacker_started: bool,
    defender_started: bool,
}

impl EnemyThrow {
    /// Begins a throw of `row` with the pair standing at `attacker` (the enemy) and `defender`
    /// (Wolf). `sorb` is the dummy Wolf is absorbed onto, in the enemy's frame. `None` when the
    /// enemy's clip has no TAE (the throw times would be unknown).
    pub fn begin(
        row: &ThrowRow,
        attacker_tae: &TaeDb,
        defender_tae: &TaeDb,
        sorb: DummyFrame,
        attacker: &Placement,
        defender: &Placement,
    ) -> Option<Self> {
        let attacker_clip = row.attacker_anim();
        let defender_clip = row.defender_anim()?;
        let mut hits: Vec<(f32, i32)> = attacker_tae
            .anim(&attacker_clip)?
            .events
            .iter()
            .filter_map(|e| match e.kind {
                TaeKind::ThrowAttack { judge } => Some((e.start, judge)),
                _ => None,
            })
            .collect();
        hits.sort_by(|a, b| a.0.total_cmp(&b.0));
        let death_branch = defender_tae.anim(&defender_clip).and_then(|a| {
            a.events
                .iter()
                .filter(|e| e.kind == TaeKind::ActionFlag(69))
                .map(|e| e.start)
                .reduce(f32::min)
        });
        // With `atkSorbDmyId` set Wolf stands on the enemy's dummy; with only `defSorbDmyId`
        // the enemy would stand on Wolf's (no such enemy row is known; the deathblow rule).
        let who = if row.atk_sorb_dummy <= 0 && row.def_sorb_dummy > 0 {
            Absorbed::Attacker
        } else {
            Absorbed::Defender
        };
        Some(Self {
            row: row.clone(),
            phase: GrabPhase::Held,
            attacker_clip,
            defender_clip,
            hits,
            death_branch,
            time: 0.0,
            lethal: false,
            branched: false,
            slide: AbsorbSlide::new(who, &sorb, attacker, defender, row.interpolation_time),
            next_hit: 0,
            attacker_started: false,
            defender_started: false,
        })
    }

    /// Wolf's death clip (`defAnimId + 1`).
    pub fn death_clip(&self) -> Option<String> {
        self.row.defender_death_anim()
    }

    /// Advances by `dt`, given the clips each character's graph plays now with their times (after
    /// both stepped). Damage follows the enemy's own clip time, so it waits through hit stop and
    /// stops if the enemy leaves the clip.
    pub fn step(
        &mut self,
        dt: f32,
        attacker_clip: Option<(&str, f32)>,
        defender_clip: Option<(&str, f32)>,
    ) -> GrabStep {
        let mut out = GrabStep::default();
        if self.phase == GrabPhase::Done {
            return out;
        }
        self.time += dt;
        out.absorb = self.slide.advance(dt);
        match attacker_clip {
            Some((clip, t)) if clip == self.attacker_clip => {
                self.attacker_started = true;
                while let Some(&(at, judge)) = self.hits.get(self.next_hit) {
                    if at > t {
                        break;
                    }
                    out.hits.push(judge);
                    self.next_hit += 1;
                }
            }
            // The enemy left its throw clip: the rest of the throw does not land.
            _ if self.attacker_started => self.next_hit = self.hits.len(),
            _ => {}
        }
        let death = self.death_clip();
        let in_defender =
            defender_clip.filter(|(c, _)| *c == self.defender_clip || death.as_deref() == Some(*c));
        match in_defender {
            Some((clip, t)) => {
                self.defender_started = true;
                if self.lethal
                    && !self.branched
                    && clip == self.defender_clip
                    && self.death_branch.is_none_or(|b| t >= b)
                {
                    self.branched = true;
                    out.death = true;
                }
            }
            // Wolf got up (his clip's cancels let him act) or something else took over.
            None if self.defender_started => self.phase = GrabPhase::Done,
            None if self.time > 0.5 => self.phase = GrabPhase::Done,
            None => {}
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row() -> ThrowRow {
        ThrowRow {
            id: 25_060_000,
            atk_chr: 5060,
            def_chr: 0,
            dist: 10.0,
            diff_ang_min: 0.0,
            diff_ang_max: 180.0,
            upper_y: 10.0,
            lower_y: 10.0,
            diff_ang_my_to_def: 180.0,
            atk_anim: 4100,
            def_anim: 600_110,
            atk_anim_offset: 0,
            def_anim_offset: 221,
            atk_sorb_dummy: 233,
            def_sorb_dummy: 0,
            throw_kind: 1_000_000,
            turn_attacker: true,
            interpolation_time: 0.5,
        }
    }

    fn throw() -> EnemyThrow {
        let r = row();
        let sorb = DummyFrame {
            position: [0.0, 0.0, -1.0],
            yaw: std::f32::consts::PI,
        };
        EnemyThrow {
            row: r.clone(),
            phase: GrabPhase::Held,
            attacker_clip: r.attacker_anim(),
            defender_clip: r.defender_anim().unwrap(),
            hits: vec![(1.333, 801)],
            death_branch: Some(3.833),
            time: 0.0,
            lethal: false,
            branched: false,
            slide: AbsorbSlide::new(
                Absorbed::Defender,
                &sorb,
                &Placement::default(),
                &Placement {
                    position: [0.3, 0.0, -1.6],
                    yaw: 2.0,
                },
                0.5,
            ),
            next_hit: 0,
            attacker_started: false,
            defender_started: false,
        }
    }

    #[test]
    fn clips_follow_the_row() {
        let t = throw();
        assert_eq!(t.attacker_clip, "a000_004100");
        assert_eq!(t.defender_clip, "a221_600110");
        assert_eq!(t.death_clip().as_deref(), Some("a221_600111"));
    }

    #[test]
    fn slides_wolf_onto_the_dummy_and_lands_once() {
        let mut t = throw();
        let dt = 1.0 / 60.0;
        let mut wolf = Placement {
            position: [0.3, 0.0, -1.6],
            yaw: 2.0,
        };
        let mut hits = Vec::new();
        for i in 0..400 {
            let at = i as f32 * dt + dt;
            let s = t.step(
                dt,
                Some(("a000_004100", at)),
                Some(("a221_600110", at.min(5.9))),
            );
            if let Some(a) = s.absorb {
                assert_eq!(a.who, Absorbed::Defender);
                wolf.position[0] += a.delta[0];
                wolf.position[2] += a.delta[2];
                wolf.yaw += a.yaw;
            }
            hits.extend(s.hits);
            assert!(!s.death);
        }
        assert_eq!(hits, vec![801]);
        // 1 m in front of the enemy (at the origin facing -Z), facing back at him.
        assert!(wolf.position[0].abs() < 1e-4, "{wolf:?}");
        assert!((wolf.position[2] + 1.0).abs() < 1e-4, "{wolf:?}");
        assert!(
            crate::input::angle_diff(wolf.yaw, std::f32::consts::PI).abs() < 1e-4,
            "{wolf:?}"
        );
    }

    #[test]
    fn hits_wait_for_the_enemy_clip_and_stop_when_it_leaves() {
        let mut t = throw();
        // Frozen before the hit: nothing lands.
        for _ in 0..10 {
            let s = t.step(0.1, Some(("a000_004100", 1.0)), Some(("a221_600110", 1.0)));
            assert!(s.hits.is_empty());
        }
        // Interrupted: the enemy is in another clip when the hit time passes.
        let s = t.step(0.1, Some(("a000_008010", 1.5)), Some(("a221_600110", 1.5)));
        assert!(s.hits.is_empty());
        let s = t.step(0.1, Some(("a000_004100", 1.6)), Some(("a221_600110", 1.6)));
        assert!(s.hits.is_empty());
    }

    #[test]
    fn lethal_throw_branches_at_flag_69_and_ends_when_wolf_leaves() {
        let mut t = throw();
        t.lethal = true;
        let s = t.step(0.1, Some(("a000_004100", 3.0)), Some(("a221_600110", 3.0)));
        assert!(!s.death);
        let s = t.step(0.1, Some(("a000_004100", 3.9)), Some(("a221_600110", 3.9)));
        assert!(s.death);
        let s = t.step(0.1, Some(("a000_004100", 4.0)), Some(("a221_600111", 0.1)));
        assert!(!s.death);
        assert_eq!(t.phase, GrabPhase::Held);
        t.step(0.1, Some(("a000_004100", 4.1)), Some(("a000_000000", 0.1)));
        assert_eq!(t.phase, GrabPhase::Done);
    }
}
