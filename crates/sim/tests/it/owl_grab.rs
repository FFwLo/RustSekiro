//! The Great Shinobi Owl's grabs (enemy throws on Wolf, `docs/OWL.md` "Grabs"), forced with
//! `Duel::force_act` against a passive Wolf: who gets caught, the paired clips, the damage from
//! the data, and the ways out.
use std::path::{Path, PathBuf};

use glam::Vec3;
use sekiro_sim::arena::{Arena, ArenaSpec, yaw_toward};
use sekiro_sim::duel::{Duel, DuelReport};
use sekiro_sim::enemy::EnemySpec;
use sekiro_sim::{Buttons, Character, InputFrame};

const DT: f32 = 1.0 / 60.0;

/// The Owl's collapse throw (崩し投げ): AtkParam 800 (`throwFlag` 1) at 0.57 to 0.77 s.
const COLLAPSE: i32 = 3040;
/// The Owl's see-through throw (見切り投げ): AtkParam 810 (perilous, `throwTypeId` 1).
const SEE_THROUGH: i32 = 3041;

fn cache() -> Option<PathBuf> {
    let cache = sekiro_sim::cache::root();
    let ok = cache.join("anim/c5060/tae.json").is_file()
        && cache.join("anim/c0000/a221_600110.bin").is_file()
        && cache.join("raw/script/m11_01_00_00.luabnd.d").is_dir();
    if !ok {
        eprintln!("skipping: the Owl or Wolf's c0000_c5060 clips are not extracted");
    }
    ok.then_some(cache)
}

fn duel(cache: &Path, distance: f32, act: i32) -> Duel {
    let mut duel = Duel::load_with(cache, distance, EnemySpec::owl()).unwrap();
    duel.enemy_active = false;
    duel.force_act = Some(act);
    duel
}

fn clip(c: &Character) -> Option<(String, f32)> {
    c.behavior
        .runtime
        .main_clip()
        .map(|c| (c.animation.clone(), c.time))
}

fn clip_name(c: &Character) -> String {
    clip(c).map(|c| c.0).unwrap_or_default()
}

/// Steps with `input(step)` until the first grab starts or `steps` run out; returns the reports.
fn run(duel: &mut Duel, steps: usize, input: impl Fn(usize) -> InputFrame) -> Vec<DuelReport> {
    let mut out = Vec::new();
    for i in 0..steps {
        let r = duel.step(&input(i), DT);
        assert!(r.player.behavior.script_errors.is_empty(), "{r:?}");
        assert!(r.enemy.behavior.script_errors.is_empty(), "{r:?}");
        out.push(r);
    }
    out
}

fn first_grab(reports: &[DuelReport]) -> Option<(usize, i32)> {
    reports
        .iter()
        .enumerate()
        .find_map(|(i, r)| r.grab.started.map(|row| (i, row)))
}

#[test]
fn collapse_throw_catches_a_passive_wolf() {
    let Some(cache) = cache() else { return };
    let mut d = duel(&cache, 1.6, COLLAPSE);
    let max_hp = d.rules.fighters[0].vitality.max_hp;
    let atk = d.fighters[1].attacks[&801].clone();
    assert_eq!(atk.throw_flag, 2);
    assert_eq!(d.fighters[1].attacks[&800].throw_flag, 1);
    let mut steps = Vec::new();
    let mut invulnerable_in_clip = true;
    let mut sp_effects = std::collections::HashSet::new();
    let mut released = None;
    for i in 0..700 {
        let r = d.step(&InputFrame::default(), DT);
        assert!(r.player.behavior.script_errors.is_empty());
        let wolf = clip(&d.player);
        if let Some((c, t)) = &wolf
            && c == "a221_600110"
        {
            // TAE 952 (0 to 4.0 s): no other attack lands while thrown.
            if *t < 3.9 {
                invulnerable_in_clip &= d.player.tae_frame().invulnerable;
            }
            sp_effects.extend(d.player.tae_frame().sp_effects.iter().copied());
        }
        if released.is_none() && i > 40 && d.enemy_throw.is_none() && !steps.is_empty() {
            released = Some((i, clip_name(&d.player)));
        }
        steps.push((r, clip_name(&d.enemy), wolf, d.player.body.position));
        if released.is_some() {
            break;
        }
    }
    let reports: Vec<_> = steps.iter().map(|s| s.0.clone()).collect();
    let (at, row) = first_grab(&reports).expect("the Owl never grabbed Wolf");
    assert_eq!(row, 25_060_000, "collapse throw row");
    // Grabbed in his AttackBehavior window (0.567 to 0.767 s of a000_003040).
    assert!((30..50).contains(&at), "grabbed at step {at}");
    // Both play the paired clips from the next step.
    let (_, owl, wolf, _) = &steps[at + 2];
    assert_eq!(owl, "a000_004100");
    assert_eq!(wolf.as_ref().unwrap().0, "a221_600110");
    // Damage from the data: one ThrowAttackBehavior (judge 801) at 1.333 s of a000_004100.
    let hits: Vec<_> = reports.iter().flat_map(|r| r.grab.hits.clone()).collect();
    assert_eq!(hits.len(), 1, "{hits:?}");
    let (judge, hp, posture) = hits[0];
    assert_eq!(judge, 801);
    assert_eq!(hp, atk.atk_phys, "HP damage is AtkParam atkPhys");
    assert_eq!(hp, 192);
    assert_eq!(posture, 12, "posture is AtkParam directAtkStamDamage");
    let landed = reports
        .iter()
        .position(|r| !r.grab.hits.is_empty())
        .unwrap();
    let after = (landed - at) as f32 * DT;
    assert!(
        (after - 1.333).abs() < 0.05,
        "landed {after:.3} s into the throw"
    );
    assert_eq!(d.rules.fighters[0].vitality.hp, max_hp - 192);
    assert!(
        invulnerable_in_clip,
        "Wolf must be invulnerable while thrown"
    );
    // The thrown clip's own TAE SpEffects reach Wolf (110030 for most of it).
    assert!(sp_effects.contains(&110030), "{sp_effects:?}");
    // Released into his get-up: he leaves the thrown clip through its late cancels or its end.
    let (when, into) = released.expect("the throw never released Wolf");
    eprintln!("released at step {when} into {into}");
    assert_ne!(into, "a221_600110");
    assert_ne!(into, "a221_600111", "a non-lethal throw must not kill");
    assert!(!d.rules.fighters[0].dead());
}

#[test]
fn guarding_wolf_is_still_grabbed() {
    let Some(cache) = cache() else { return };
    let mut d = duel(&cache, 1.6, COLLAPSE);
    let reports = run(&mut d, 120, |_| InputFrame {
        buttons: Buttons {
            guard: true,
            ..Buttons::default()
        },
        ..InputFrame::default()
    });
    let guarded = reports.iter().any(|r| {
        r.player
            .action_flags
            .contains(&sekiro_sim::hits::FLAG_GUARD)
    });
    assert!(guarded, "Wolf never raised his guard");
    assert_eq!(first_grab(&reports).map(|g| g.1), Some(25_060_000));
    assert!(
        reports
            .iter()
            .all(|r| r.hits.iter().all(|(h, _)| h.judge != 800)),
        "the grab must not resolve as a blocked hit"
    );
}

#[test]
fn deflect_does_not_stop_the_grab() {
    let Some(cache) = cache() else { return };
    let mut d = duel(&cache, 1.6, COLLAPSE);
    // Tap guard into the grab window (deflect timing) over and over.
    let reports = run(&mut d, 120, |i| InputFrame {
        buttons: Buttons {
            guard: i % 8 < 2,
            ..Buttons::default()
        },
        ..InputFrame::default()
    });
    assert_eq!(first_grab(&reports).map(|g| g.1), Some(25_060_000));
}

/// Wolf steps back (stick back, dodge pressed) as the Owl lunges: out of reach of the grab.
#[test]
fn stepping_away_avoids_the_grab() {
    let Some(cache) = cache() else { return };
    let mut d = duel(&cache, 1.6, COLLAPSE);
    let reports = run(&mut d, 130, |i| InputFrame {
        move_stick: if (12..30).contains(&i) {
            [0.0, -1.0]
        } else {
            [0.0, 0.0]
        },
        buttons: Buttons {
            dodge: (14..16).contains(&i),
            ..Buttons::default()
        },
        ..InputFrame::default()
    });
    let stepped = reports.iter().any(|r| {
        r.player
            .animation
            .as_deref()
            .is_some_and(|a| a.starts_with("a000_2"))
    });
    assert!(stepped, "Wolf never stepped");
    let owl_tried = reports
        .iter()
        .any(|r| r.enemy.animation.as_deref() == Some("a000_003040") && r.enemy.anim_time > 0.8);
    assert!(owl_tried, "the Owl's grab never got past its window");
    assert_eq!(
        first_grab(&reports),
        None,
        "stepping away must avoid the grab"
    );
}

/// Wolf jumps as the grab comes: an airborne Wolf cannot be grabbed.
#[test]
fn jumping_avoids_the_grab() {
    let Some(cache) = cache() else { return };
    let mut d = duel(&cache, 1.6, COLLAPSE);
    let reports = run(&mut d, 130, |i| InputFrame {
        buttons: Buttons {
            jump: (22..24).contains(&i),
            ..Buttons::default()
        },
        ..InputFrame::default()
    });
    let airborne = reports.iter().any(|r| !r.player.grounded);
    assert!(airborne, "Wolf never left the ground");
    assert_eq!(
        first_grab(&reports),
        None,
        "a jump must get out of the grab"
    );
}

/// The see-through throw (perilous, unblockable and undeflectable): its own row and two damage
/// events, the first of which cannot kill (`excessDmgKeepHp` 1).
#[test]
fn see_through_throw_lands_both_cuts() {
    let Some(cache) = cache() else { return };
    let mut d = duel(&cache, 1.2, SEE_THROUGH);
    let profile = d.fighters[1].attacks[&810].profile.clone().unwrap();
    assert!(profile.disable_guard[0] && profile.disable_just_guard[0]);
    let reports = run(&mut d, 700, |_| InputFrame::default());
    assert_eq!(first_grab(&reports).map(|g| g.1), Some(25_060_100));
    let hits: Vec<_> = reports.iter().flat_map(|r| r.grab.hits.clone()).collect();
    let judges: Vec<_> = hits.iter().map(|h| h.0).collect();
    assert_eq!(&judges[..2], &[811, 812], "{hits:?}");
    assert_eq!(hits[0].1, 32);
    assert_eq!(hits[1].1, 208);
    assert!(
        reports
            .iter()
            .any(|r| r.player.animation.as_deref() == Some("a221_600120"))
    );
}

/// A throw that empties Wolf's HP sends him to the death clip at the thrown clip's flag 69.
#[test]
fn lethal_grab_plays_the_death_clip() {
    let Some(cache) = cache() else { return };
    let mut d = duel(&cache, 1.6, COLLAPSE);
    d.rules.fighters[0].vitality.hp = 100;
    let mut death = None;
    for i in 0..500 {
        let r = d.step(&InputFrame::default(), DT);
        assert!(r.player.behavior.script_errors.is_empty());
        if r.player.animation.as_deref() == Some("a221_600111") {
            death = Some(i);
            break;
        }
    }
    let death = death.expect("Wolf never played the throw's death clip");
    eprintln!("death clip from step {death}");
    assert!(d.rules.fighters[0].dead());
}

/// Wolf backed to the castle-top railing when the Owl grabs: no step of either crosses map
/// geometry or jumps, and both end on a floor.
#[test]
fn grab_near_a_wall_does_not_clip() {
    let Some(cache) = cache() else { return };
    if !cache.join("maps/m11_01_00_00/collision.bin").is_file() {
        eprintln!("skipping: the castle top is not extracted");
        return;
    }
    let mut d = duel(&cache, 1.6, COLLAPSE);
    let arena = Arena::load(&cache, ArenaSpec::by_name("owl").unwrap(), false).unwrap();
    arena.place(&mut d);
    let center = arena.floor_at(arena.layout.center());
    let mut setup = None;
    for i in 0..360 {
        let angle = (i as f32).to_radians();
        let direction = Vec3::new(angle.sin(), 0.0, angle.cos());
        let Some(distance) = arena.world.raycast(center + Vec3::Y * 0.8, direction, 25.0) else {
            continue;
        };
        if distance < 3.0 {
            continue;
        }
        let wolf = center + direction * (distance - 0.45);
        let owl = wolf - direction * 1.6;
        let floors = [owl, wolf].map(|p| arena.world.floor(p.x, p.z, center.y + 0.3));
        if floors
            .iter()
            .all(|h| h.is_some_and(|h| (h - center.y).abs() < 0.1))
        {
            setup = Some((owl, wolf));
            break;
        }
    }
    let (owl, wolf) = setup.expect("no wall with a level approach");
    for (c, p, t) in [(&mut d.player, wolf, owl), (&mut d.enemy, owl, wolf)] {
        c.body.position = p.to_array();
        c.body.yaw = yaw_toward(p, t);
        c.body.fall_top = center.y;
    }
    // The first step settles Wolf against the wall's collision (a few centimetres off the ray);
    // the Owl's lunge only starts its contact window at 0.57 s.
    d.step(&InputFrame::default(), DT);
    let mut positions = vec![[
        Vec3::from(d.player.body.position),
        Vec3::from(d.enemy.body.position),
    ]];
    let mut grabbed = false;
    let mut min_gap = f32::MAX;
    for _ in 0..500 {
        let r = d.step(&InputFrame::default(), DT);
        grabbed |= r.grab.started.is_some();
        positions.push([d.player.body.position.into(), d.enemy.body.position.into()]);
        if d.enemy_throw.is_some() {
            let (p, e) = (d.player.body.position, d.enemy.body.position);
            min_gap = min_gap.min((p[0] - e[0]).hypot(p[2] - e[2]));
        }
        if grabbed && d.enemy_throw.is_none() {
            break;
        }
    }
    assert!(grabbed, "the Owl never grabbed Wolf at the wall");
    eprintln!("closest the held pair came: {min_gap:.3} m");
    let mut violations = Vec::new();
    for (step, pair) in positions.windows(2).enumerate() {
        for (actor, (&from, &to)) in pair[0].iter().zip(&pair[1]).enumerate() {
            let delta = to - from;
            let length = delta.length();
            if length < 1e-5 {
                continue;
            }
            for height in [0.05, 0.8] {
                if let Some(hit) =
                    arena
                        .world
                        .raycast(from + Vec3::Y * height, delta / length, length)
                    && hit > 1e-4
                    && hit < length - 1e-4
                {
                    violations.push(format!("step {step}, actor {actor}: {from:?} -> {to:?}"));
                }
            }
            // Walkable roof tiles include 0.115 m rises; use the terrain's actual step
            // limit rather than the former 0.1 m assumption. Wall crossings stay forbidden.
            if delta.x.hypot(delta.z) > 0.2 || delta.y > sekiro_sim::collision::STEP_HEIGHT {
                violations.push(format!("step {step}, actor {actor}: jumped {delta:?}"));
            }
        }
    }
    for (actor, p) in positions.last().unwrap().iter().enumerate() {
        let floor = arena.world.floor(p.x, p.z, p.y + 0.3);
        if !floor.is_some_and(|h| (p.y - h).abs() < 0.06) {
            violations.push(format!("final actor {actor}: {p:?}, floor {floor:?}"));
        }
    }
    assert!(violations.is_empty(), "{}", violations.join("\n"));
}
