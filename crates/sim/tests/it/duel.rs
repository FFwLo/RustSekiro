//! Wolf against an Ashina soldier: hits, blocks and a perfect deflect, all driven by both
//! characters' own scripts and graphs. Skips when `cache/` has not been extracted.

use sekiro_sim::duel::{Duel, DuelReport};
use sekiro_sim::hits::HitResult;
use sekiro_sim::{Buttons, InputFrame};

const DT: f32 = 1.0 / 60.0;

#[test]
fn absent_enemy_holds_state_while_passive_and_dead_enemies_advance() {
    let Some(mut duel) = load_at(4.0) else { return };
    duel.enemy_active = false;
    duel.step(&InputFrame::default(), DT);
    duel.enemy.body.position[1] += 4.;
    duel.enemy.body.velocity = [1., 2., 3.];
    duel.enemy.body.grounded = false;
    let body = duel.enemy.body.clone();
    let clip = |duel: &Duel| {
        duel.enemy
            .behavior
            .runtime
            .main_clip()
            .map(|c| (c.animation.clone(), c.time))
    };
    let before = clip(&duel);
    let state = duel.enemy.state().to_string();
    duel.enemy_present = false;
    for _ in 0..10 {
        let report = duel.step(
            &InputFrame {
                buttons: Buttons {
                    lock_on: true,
                    ..Default::default()
                },
                ..Default::default()
            },
            DT,
        );
        assert_eq!(report.enemy.advanced, 0.);
        assert_eq!(clip(&duel), before);
        assert_eq!(duel.enemy.state(), state);
        assert_eq!(duel.enemy.body.position, body.position);
        assert_eq!(duel.enemy.body.velocity, body.velocity);
        assert!(!report.locked_on && report.hits.is_empty());
        assert!(duel.player.aim_candidates.is_empty());
        assert!(duel.respawn_in.is_none());
    }
    duel.enemy_present = true;
    assert!(duel.step(&InputFrame::default(), DT).enemy.advanced > 0.);
    duel.rules.fighters[1].vitality.hp = 0;
    assert!(duel.step(&InputFrame::default(), DT).enemy.advanced > 0.);
}

#[test]
fn dead_target_releases_lock_and_camera_relative_movement() {
    let Some(mut duel) = load_at(4.0) else {
        return;
    };
    duel.enemy_active = false;
    let input = InputFrame {
        buttons: Buttons {
            lock_on: true,
            ..Buttons::default()
        },
        ..InputFrame::default()
    };
    assert!(duel.step(&input, DT).locked_on);
    // Move sideways while locked, then kill the target without releasing the stick.
    let moving = InputFrame {
        move_stick: [1.0, 0.0],
        camera_yaw: 0.0,
        ..InputFrame::default()
    };
    for _ in 0..30 {
        duel.step(&moving, DT);
    }
    duel.rules.fighters[1].vitality.hp = 0;
    let start = duel.player.body.position;
    for _ in 0..30 {
        let r = duel.step(&moving, DT);
        assert!(!r.locked_on);
        assert!(!duel.player.behavior.env.locked_on);
    }
    let end = duel.player.body.position;
    eprintln!(
        "after death movement: {:?}",
        [end[0] - start[0], end[2] - start[2]]
    );
    assert!(end[0] - start[0] > 0.2 && (end[2] - start[2]).abs() < (end[0] - start[0]).abs() * 0.3);
    assert!(!duel.step(&input, DT).locked_on);
    duel.reset().unwrap();
    duel.step(&InputFrame::default(), DT);
    assert!(duel.step(&input, DT).locked_on);
}

#[test]
fn held_stick_release_all_camera_directions() {
    use rayon::prelude::*;
    // Every stick direction and several camera yaws, each pair on its own duel.
    let cases = [
        (0.0, [0.0, 1.0]),
        (1.2, [-1.0, 0.0]),
        (-2.4, [0.0, -1.0]),
        (1.2, [1.0, 0.0]),
        (-2.4, [0.0, 1.0]),
    ];
    cases.par_iter().for_each(|&(yaw, stick)| {
        let Some(mut duel) = load_at(4.0) else { return };
        duel.enemy_active = false;
        duel.step(
            &InputFrame {
                buttons: Buttons {
                    lock_on: true,
                    ..Buttons::default()
                },
                ..InputFrame::default()
            },
            DT,
        );
        let input = InputFrame {
            move_stick: stick,
            camera_yaw: yaw,
            ..InputFrame::default()
        };
        for _ in 0..90 {
            duel.step(&input, DT);
        }
        duel.rules.fighters[1].vitality.hp = 0;
        let start = duel.player.body.position;
        for _ in 0..30 {
            duel.step(&input, DT);
        }
        let end = duel.player.body.position;
        let delta = [end[0] - start[0], end[2] - start[2]];
        let (s, c) = yaw.sin_cos();
        let desired = [c * stick[0] - s * stick[1], -s * stick[0] - c * stick[1]];
        let dot = delta[0] * desired[0] + delta[1] * desired[1];
        let cross = delta[0] * desired[1] - delta[1] * desired[0];
        eprintln!("yaw {yaw} stick {stick:?}: {delta:?}, dot {dot} cross {cross}");
        assert!(
            dot > 0.2 && cross.abs() < dot * 0.3,
            "yaw {yaw} stick {stick:?}"
        );
    });
}

fn load() -> Option<Duel> {
    load_at(4.0)
}

fn load_at(distance: f32) -> Option<Duel> {
    let cache = sekiro_sim::cache::root();
    if !cache.join("raw/chr/c1010.behbnd.d/c9997.hkx").exists() {
        eprintln!("skipping: cache not extracted");
        return None;
    }
    let mut duel = Duel::load(&cache, distance).unwrap();
    // The stand-in brain: predictable attacks for the scripted timing below.
    duel.use_simple_ai();
    Some(duel)
}

fn frame(guard: bool, attack: bool) -> InputFrame {
    InputFrame {
        buttons: Buttons {
            guard,
            attack,
            ..Buttons::default()
        },
        ..InputFrame::default()
    }
}

fn run(
    duel: &mut Duel,
    steps: usize,
    mut input: impl FnMut(usize) -> InputFrame,
) -> Vec<DuelReport> {
    (0..steps)
        .map(|i| {
            let r = duel.step(&input(i), DT);
            assert!(
                r.player.behavior.script_errors.is_empty(),
                "{:?}",
                r.player.behavior.script_errors
            );
            assert!(
                r.enemy.behavior.script_errors.is_empty(),
                "{:?}",
                r.enemy.behavior.script_errors
            );
            r
        })
        .collect()
}

fn states(reports: &[DuelReport], enemy: bool) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for r in reports {
        let s = if enemy { &r.enemy } else { &r.player };
        let name = s.behavior.state_path.last().cloned().unwrap_or_default();
        if out.last() != Some(&name) {
            out.push(name);
        }
    }
    out
}

#[test]
fn soldier_attacks_and_guard_blocks_or_deflects() {
    let Some(mut duel) = load() else { return };
    // Held guard from the start: the soldier's first attack is blocked.
    let reports = run(&mut duel, 600, |i| frame(i >= 60, false));
    let hits: Vec<_> = reports.iter().flat_map(|r| r.hits.clone()).collect();
    let (_, blocked) = hits
        .iter()
        .find(|(h, _)| h.attacker == 1)
        .expect("the soldier never hit");
    assert!(blocked.guarded && !blocked.deflected, "{blocked:?}");
    assert!(
        states(&reports, false)
            .iter()
            .any(|s| s.starts_with("StandDeflectEasy"))
    );
    assert!(
        states(&reports, true)
            .iter()
            .any(|s| s.starts_with("AttackNoBound"))
    );
}

#[test]
fn timed_guard_press_deflects() {
    let Some(mut duel) = load() else { return };
    // Find when the soldier's first hit would land, then replay pressing guard just before it.
    let probe = run(&mut duel, 700, |_| frame(false, false));
    let hit_at = probe
        .iter()
        .position(|r| r.hits.iter().any(|(h, _)| h.attacker == 1))
        .expect("the soldier never hit");
    let mut duel = load().unwrap();
    let press = hit_at.saturating_sub(8);
    let reports = run(&mut duel, hit_at + 60, |i| frame(i >= press, false));
    let deflect = reports
        .iter()
        .flat_map(|r| r.hits.clone())
        .find(|(h, _)| h.attacker == 1)
        .expect("no hit after the press");
    assert!(deflect.1.deflected, "{:?}", deflect.1);
    assert!(
        states(&reports, false)
            .iter()
            .any(|s| s.starts_with("StandDeflectHard"))
    );
    assert!(
        states(&reports, true)
            .iter()
            .any(|s| s.starts_with("AttackBoundEnemy"))
    );
}

#[test]
fn wolf_slash_staggers_the_soldier() {
    let Some(mut duel) = load() else { return };
    duel.enemy_active = false;
    // Walk up to the idle soldier, then attack twice.
    let reports = run(&mut duel, 400, |i| match i {
        0..=90 => InputFrame {
            move_stick: [0.0, 1.0],
            ..InputFrame::default()
        },
        120 | 160 => frame(false, true),
        _ => InputFrame::default(),
    });
    let wolf_hits = reports
        .iter()
        .flat_map(|r| r.hits.clone())
        .filter(|(h, _)| h.attacker == 0)
        .count();
    assert!(wolf_hits >= 1, "the slash never connected");
    assert!(
        states(&reports, true)
            .iter()
            .any(|s| s.starts_with("Damage")),
        "{:?}",
        states(&reports, true)
    );
}

/// Wolf's guard presses: `(first frame, frames held)`.
type Presses = Vec<(usize, usize)>;

fn pressed(presses: &Presses, i: usize) -> bool {
    presses.iter().any(|&(s, n)| i >= s && i < s + n)
}

/// A soldier that attacks with its first slash (AtkParam_Npc 10100100) as soon as it can.
fn eager_soldier() -> Option<Duel> {
    let mut duel = load_at(2.0)?;
    duel.brain.interval = (0.0, 0.0);
    duel.brain.guard_chance = 0.0;
    duel.brain.attacks = vec![3000];
    Some(duel)
}

/// Runs from the start with `presses` until the `n`-th soldier hit (0-based) and returns the
/// duel and that hit's frame.
fn until_soldier_hit(presses: &Presses, n: usize) -> Option<(Duel, usize, HitResult)> {
    let mut duel = eager_soldier()?;
    let mut seen = 0;
    for i in 0..3000 {
        // Locked on from the start, as players fight: Wolf keeps facing the soldier.
        let mut input = frame(pressed(presses, i), false);
        input.buttons.lock_on = (5..8).contains(&i);
        let r = duel.step(&input, DT);
        if let Some((_, res)) = r.hits.iter().find(|(h, _)| h.attacker == 1) {
            if seen == n {
                return Some((duel, i, *res));
            }
            seen += 1;
        }
    }
    None
}

/// Deflect depths 1 to 3 send back 45, 22 and 11 posture.
#[test]
fn deflect_chain_depths_send_back_posture() {
    deflect_chain(false);
}

#[test]
#[ignore = "slow: replays the fight for every deflect until the posture breaks (about 20 s)"]
fn deflect_chain_breaks_the_soldier_then_deathblow() {
    deflect_chain(true);
}

fn deflect_chain(full: bool) {
    if load().is_none() {
        return;
    }
    // Deflect the soldier's slash at chain depths 1, 2 and 3 (guard pressed once, twice and
    // three times in a row), which apply deflect effects 105020, 105021 and 105022 and send
    // back 45, 22 and 11 posture (docs/COMBAT-RULES.md), then keep deflecting with single
    // presses until its posture breaks.
    let mut presses: Presses = Vec::new();
    let mut sent_back = Vec::new();
    let mut broken_at = None;
    for k in 0..if full { 8 } else { 3 } {
        let depth = if k < 3 { k + 1 } else { 1 };
        let (_, hit_at, _) = until_soldier_hit(&presses, k).expect("the soldier stopped attacking");
        // The last press opens the shortest window (4 frames at depth 3) just before the hit.
        let mut result = None;
        for lead in [2usize, 1, 3, 4] {
            let last = hit_at - lead;
            let mut trial = presses.clone();
            for j in 0..depth {
                trial.push((last - (depth - 1 - j) * 12, 3));
            }
            let (duel, _, res) = until_soldier_hit(&trial, k).unwrap();
            if res.deflected {
                result = Some((trial, duel, res));
                break;
            }
        }
        let (trial, duel, res) = result.expect("no press timing deflected the slash");
        presses = trial;
        let soldier = &duel.rules.fighters[1].posture;
        eprintln!(
            "deflect {k} at depth {depth}: {} back, soldier posture {} of {}, code {:?}",
            res.attacker_posture_damage,
            soldier.remaining,
            soldier.max,
            res.attacker.map(|a| a.damage_type)
        );
        sent_back.push(res.attacker_posture_damage);
        if res.posture_broke {
            assert!(soldier.broken());
            assert_eq!(res.attacker.unwrap().damage_type, 1033);
            assert_eq!(res.defender.damage_type, 1028);
            broken_at = Some(k);
            break;
        }
        assert_eq!(res.attacker.unwrap().damage_type, 1008);
        assert_eq!(res.defender.damage_type, 3);
    }
    assert_eq!(&sent_back[..3], &[45, 22, 11]);
    if !full {
        return;
    }
    let k = broken_at.expect("posture never broke");
    assert!(sent_back[3..].iter().all(|&d| d == 45));

    // Replay to the break, then press attack: Wolf deathblows the staggered soldier.
    let (mut duel, _, _) = until_soldier_hit(&presses, k).unwrap();
    let mut started = None;
    let mut killed = false;
    let mut wolf_states = Vec::new();
    let mut soldier_states = Vec::new();
    for i in 0..400 {
        // The soldier is in AttackBoundEmptyStamina, posture 0; Wolf (locked on) presses attack.
        let mut input = frame(false, (10..14).contains(&i));
        input.buttons.lock_on = i == 20 || i == 200;
        let r = duel.step(&input, DT);
        started = started.or(r.deathblow.started);
        if started.is_some() {
            assert!(
                !r.locked_on,
                "lethal deathblow must not reacquire the target"
            );
            assert!(!duel.player.behavior.env.locked_on);
        }
        killed |= r.deathblow.killed;
        for (states, s) in [
            (&mut wolf_states, &r.player),
            (&mut soldier_states, &r.enemy),
        ] {
            let name = s.behavior.state_path.last().cloned().unwrap_or_default();
            if states.last() != Some(&name) {
                states.push(name);
            }
        }
    }
    eprintln!("wolf {wolf_states:?}\nsoldier {soldier_states:?}");
    assert!(started.is_some(), "no deathblow started");
    assert!(killed, "the deathblow did not kill");
    assert!(duel.rules.fighters[1].dead());
    assert!(wolf_states.iter().any(|s| s.starts_with("ThrowAtk")));
    assert!(soldier_states.iter().any(|s| s.starts_with("ThrowDef")));
}

#[test]
fn wolf_dies_and_respawns() {
    let Some(mut duel) = eager_soldier() else {
        return;
    };
    // Wolf never guards: every slash lands (80 HP and 18 posture on a direct hit).
    let mut died_at = None;
    let mut wolf_states = Vec::new();
    let mut respawned = false;
    for i in 0..4000 {
        let r = duel.step(&frame(false, false), DT);
        for (h, res) in &r.hits {
            if h.attacker == 1 {
                assert_eq!((res.hp_damage, res.posture_damage), (80, 18), "{res:?}");
            }
        }
        let name = r
            .player
            .behavior
            .state_path
            .last()
            .cloned()
            .unwrap_or_default();
        if wolf_states.last() != Some(&name) {
            wolf_states.push(name);
        }
        if died_at.is_none() && duel.rules.fighters[0].dead() {
            died_at = Some(i);
        }
        if r.respawned {
            respawned = true;
            let wolf = &duel.rules.fighters[0];
            assert_eq!(wolf.vitality.hp, wolf.vitality.max_hp);
            assert_eq!(wolf.posture.remaining, wolf.posture.max);
            assert_eq!(duel.player.state(), "StandIdle");
            break;
        }
    }
    eprintln!("wolf {wolf_states:?}");
    assert!(died_at.is_some(), "Wolf never died");
    assert!(
        wolf_states.iter().any(|s| s.contains("Death")),
        "{wolf_states:?}"
    );
    assert!(respawned, "no respawn after death");
}

#[test]
fn blocking_on_an_empty_posture_breaks_wolfs_guard() {
    let Some(mut duel) = eager_soldier() else {
        return;
    };
    // Nearly broken and unable to recover: the next blocked slash (17 posture) breaks the guard.
    let mut states = Vec::new();
    let mut code = None;
    for i in 0..600 {
        duel.rules.fighters[0].posture.remaining = duel.rules.fighters[0].posture.remaining.min(5);
        let r = duel.step(&frame(i > 20, false), DT);
        if let Some((_, res)) = r.hits.iter().find(|(h, _)| h.attacker == 1) {
            code = Some(res.defender.damage_type);
            // A guard break is a posture break in the hit report too (#192).
            assert!(res.posture_broke, "{res:?}");
        }
        let name = r
            .player
            .behavior
            .state_path
            .last()
            .cloned()
            .unwrap_or_default();
        if states.last() != Some(&name) {
            states.push(name);
        }
        if code.is_some() && i > 0 && states.len() > 4 {
            break;
        }
    }
    eprintln!("wolf {states:?}");
    assert_eq!(code, Some(1001));
    // The script calls the guard-break state "StandDeflectBreak".
    assert!(
        states.iter().any(|s| s.contains("DeflectBreak")),
        "{states:?}"
    );
}

#[test]
fn soldier_fights_with_its_own_ai() {
    let cache = sekiro_sim::cache::root();
    if !cache.join("raw/script/aicommon.luabnd.d").exists() {
        return;
    }
    let mut duel = Duel::load(&cache, 6.0).unwrap();
    assert!(duel.ai.is_some(), "the soldier's AI did not load");
    // Wolf locks on and attacks every 1.5 s, guarding in between.
    let mut actions = Vec::new();
    let mut soldier_states: Vec<String> = Vec::new();
    let mut hits = Vec::new();
    for i in 0..1800 {
        let mut input = frame(i % 90 > 40, i % 90 == 0 && i > 300);
        input.buttons.lock_on = (2..4).contains(&i);
        let r = duel.step(&input, DT);
        assert!(
            r.enemy.behavior.script_errors.is_empty(),
            "{:?}",
            r.enemy.behavior.script_errors
        );
        let a = duel.enemy.npc.action;
        if a != 0 && actions.last() != Some(&a) {
            actions.push(a);
        }
        let name = r
            .enemy
            .behavior
            .state_path
            .last()
            .cloned()
            .unwrap_or_default();
        if soldier_states.last() != Some(&name) {
            eprintln!("{i:5} soldier {name} (action {a})");
            soldier_states.push(name);
        }
        for (h, res) in &r.hits {
            let line = format!(
                "{i:5} {} -> {} {}: HP -{} posture -{} / back -{}, codes {} {:?}",
                ["wolf", "soldier"][h.attacker],
                ["wolf", "soldier"][h.defender],
                if res.deflected {
                    "deflected"
                } else if res.guarded {
                    "blocked"
                } else {
                    "hit"
                },
                res.hp_damage,
                res.posture_damage,
                res.attacker_posture_damage,
                res.defender.damage_type,
                res.attacker.map(|a| a.damage_type)
            );
            eprintln!("{line}");
            hits.push(line);
        }
    }
    eprintln!("AI actions {actions:?}");
    for err in duel.ai.as_mut().unwrap().drain_log() {
        if let sekiro_sim::lua_ai::AiEvent::Error(e) = err {
            panic!("AI error: {e}");
        }
    }
    assert!(
        actions.iter().any(|&a| (3000..3100).contains(&a)),
        "{actions:?}"
    );
    assert!(
        soldier_states.iter().any(|s| s.starts_with("Attack30")),
        "{soldier_states:?}"
    );
    assert!(!hits.is_empty());
}
