//! Reproduction of Wolf restarting his first slash on contact with a passive Owl.

use std::fmt::Write;

use sekiro_sim::duel::Duel;
use sekiro_sim::enemy::EnemySpec;
use sekiro_sim::{Buttons, InputFrame};

const DT: f32 = 1.0 / 60.0;

struct Step {
    time: f32,
    state: String,
    clip: String,
    clip_time: f32,
    hit: bool,
    stop: [f32; 2],
    owl_state: String,
    owl_clip: String,
}

struct Run {
    steps: Vec<Step>,
    states: Vec<String>,
    starts: Vec<(String, f32)>,
}

fn run(distance: f32) -> Option<Run> {
    let cache = sekiro_sim::cache::root();
    if !cache.join("anim/c5060/tae.json").exists()
        || !cache.join("raw/script/m11_01_00_00.luabnd.d").is_dir()
    {
        eprintln!("skipping: the Owl is not extracted");
        return None;
    }
    let mut duel = Duel::load_with(&cache, distance, EnemySpec::owl()).unwrap();
    duel.use_simple_ai();
    duel.enemy_active = false;
    // Establish lock-on before the timed input, identically for air and contact.
    for i in 0..30 {
        duel.step(
            &InputFrame {
                buttons: Buttons {
                    lock_on: i == 0,
                    ..Buttons::default()
                },
                ..InputFrame::default()
            },
            DT,
        );
    }
    let mut out = Run {
        steps: Vec::new(),
        states: Vec::new(),
        starts: Vec::new(),
    };
    for i in 0..240 {
        let r = duel.step(
            &InputFrame {
                buttons: Buttons {
                    attack: i % 8 == 0,
                    ..Buttons::default()
                },
                ..InputFrame::default()
            },
            DT,
        );
        let state = r
            .player
            .behavior
            .state_path
            .last()
            .cloned()
            .unwrap_or_default();
        let time = i as f32 * DT;
        if out.states.last() != Some(&state) {
            out.states.push(state.clone());
            if state.starts_with("GroundAttackCombo") {
                out.starts.push((state.clone(), time));
            }
        }
        out.steps.push(Step {
            time,
            state,
            clip: r.player.animation.unwrap_or_default(),
            clip_time: r.player.anim_time,
            hit: r.hits.iter().any(|(h, _)| h.attacker == 0),
            stop: duel.hit_stop.map(|s| s.remaining()),
            owl_state: r
                .enemy
                .behavior
                .state_path
                .last()
                .cloned()
                .unwrap_or_default(),
            owl_clip: r.enemy.animation.unwrap_or_default(),
        });
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
    }
    eprintln!("distance {distance:.1} m states: {:?}", out.states);
    eprintln!("distance {distance:.1} m combo starts: {:?}", out.starts);
    Some(out)
}

impl Run {
    fn reaches_three(&self) -> bool {
        self.states.iter().any(|s| s == "GroundAttackCombo3")
    }

    fn table(&self) -> String {
        let mut table = format!(
            "states: {:?}\ntime Wolf_state Wolf_clip clip_time hit stop_Wolf stop_Owl Owl_state Owl_clip\n",
            self.states
        );
        for (i, s) in self.steps.iter().enumerate() {
            let changed = i == 0 || {
                let p = &self.steps[i - 1];
                s.state != p.state
                    || s.clip != p.clip
                    || s.clip_time < p.clip_time
                    || s.owl_state != p.owl_state
                    || s.owl_clip != p.owl_clip
                    || s.stop != p.stop
            };
            if changed || s.hit {
                writeln!(
                    table,
                    "{:.3} {} {} {:.3} {} {:.3} {:.3} {} {}",
                    s.time,
                    s.state,
                    s.clip,
                    s.clip_time,
                    s.hit,
                    s.stop[0],
                    s.stop[1],
                    s.owl_state,
                    s.owl_clip
                )
                .unwrap();
            }
        }
        table
    }
}

#[test]
fn combo_into_air_chains() {
    let Some(air) = run(15.0) else { return };
    assert!(
        air.reaches_three() && air.steps.iter().all(|s| !s.hit),
        "air did not chain without contact\n{}",
        air.table()
    );
}

#[test]
fn combo_on_owl_chains_like_in_the_air() {
    let Some(hit) = run(1.6) else { return };
    let first = hit
        .starts
        .iter()
        .filter(|(s, _)| s == "GroundAttackCombo1")
        .count();
    assert!(
        hit.steps.iter().any(|s| s.hit) && hit.reaches_three() && first <= 2,
        "contact must reach Combo3 with at most two Combo1 starts; saw {first}\n{}",
        hit.table()
    );
}

#[test]
fn combo_on_owl_timing_matches_air() {
    let Some(air) = run(15.0) else { return };
    let hit = run(1.6).unwrap();
    let mut failures = Vec::new();
    for (state, time) in &air.starts {
        // Compare the first start of each exact combo state, including release states.
        if air.starts.iter().find(|(s, _)| s == state).unwrap().1 != *time {
            continue;
        }
        // Each landed hit freezes Wolf for its hit stop (0.1 s for these slashes), so a combo
        // state may start that much later per hit landed before it, plus one step of slack.
        let hits_before = |t: f32| hit.steps.iter().filter(|s| s.hit && s.time < t).count();
        match hit.starts.iter().find(|(s, _)| s == state) {
            Some((_, on_hit)) if *on_hit <= time + 0.1 * hits_before(*on_hit) as f32 + 0.1 => {}
            other => failures.push(format!("{state}: air {time:.3}, contact {other:?}")),
        }
    }
    assert!(
        air.reaches_three() && hit.steps.iter().any(|s| s.hit) && failures.is_empty(),
        "combo timing mismatch: {failures:?}\nAIR\n{}\nCONTACT\n{}",
        air.table(),
        hit.table()
    );
}

/// The user's report ("mashing on the Owl repeats the first slash and stun locks him"),
/// reproduced in the app with `--arena owl --god` and a raw input replay: the Owl's own AI
/// guards (`a000_003100`), Wolf's blocked slash recoils into `EasyDeflected` and the script's
/// follow-up `EasyDeflectedAtk` (a fast first-slash-like cut), which the Owl blocks again. That
/// loop is the game's. The bug was the Owl's side: env 258 (his NpcParam `guardLevel`) read 0,
/// so his script's guard table gave no guard reaction and every blocked slash played the hit
/// flinch `a000_008010`, which has no guard window, stopped his consecutive-guard count (SpEffect
/// 200250 from the guard reactions) and so his chance to deflect.
#[test]
fn owl_blocks_mashed_slashes_with_guard_reactions() {
    let cache = sekiro_sim::cache::root();
    if !cache.join("anim/c5060/tae.json").exists()
        || !cache.join("raw/script/m11_01_00_00.luabnd.d").is_dir()
    {
        eprintln!("skipping: the Owl is not extracted");
        return;
    }
    let mut duel = Duel::load_with(&cache, 1.8, EnemySpec::owl()).unwrap();
    duel.god = true;
    let mut log = String::new();
    let mut guards = 0;
    let mut flinch_after_guard = 0;
    let mut guard_reactions = 0;
    let mut since_guard: Option<usize> = None;
    for i in 0..900 {
        let r = duel.step(
            &InputFrame {
                move_stick: [0.0, 1.0],
                buttons: Buttons {
                    attack: i % 5 < 2,
                    lock_on: i == 0,
                    ..Buttons::default()
                },
                ..InputFrame::default()
            },
            DT,
        );
        let owl_clip = r.enemy.animation.clone().unwrap_or_default();
        let wolf = r
            .player
            .behavior
            .state_path
            .last()
            .cloned()
            .unwrap_or_default();
        for (h, res) in &r.hits {
            writeln!(
                log,
                "{i} {}>{} {} {} wolf {wolf} owl {owl_clip}",
                h.attacker,
                h.defender,
                h.attack.atk_id,
                if res.deflected {
                    "deflect"
                } else if res.guarded {
                    "guard"
                } else {
                    "hit"
                }
            )
            .unwrap();
            if h.attacker == 0 && res.guarded && !res.deflected {
                guards += 1;
                since_guard = Some(0);
            }
        }
        if let Some(n) = since_guard.as_mut() {
            *n += 1;
            // The reaction starts once the hit stop is over and the script has read the hit.
            if *n == 10 {
                writeln!(log, "{i}   owl after guard: {owl_clip}").unwrap();
                if owl_clip.starts_with("a000_0080") {
                    flinch_after_guard += 1;
                }
                if owl_clip.starts_with("a000_0085") {
                    guard_reactions += 1;
                }
                since_guard = None;
            }
        }
        assert!(r.enemy.behavior.script_errors.is_empty());
    }
    eprintln!("{log}");
    assert!(
        guards > 0 && flinch_after_guard == 0 && guard_reactions > 0,
        "guards {guards}, hit flinches after a guard {flinch_after_guard}, guard reactions \
         {guard_reactions}\n{log}"
    );
}

#[test]
fn soldier_guards_mashed_slashes() {
    let cache = sekiro_sim::cache::root();
    if !cache.join("anim/c1010/tae.json").exists() {
        return;
    }
    let mut duel = Duel::load(&cache, 1.6).unwrap();
    duel.god = true;
    let mut counts = [0; 3];
    let mut broken = false;
    for i in 0..240 {
        let r = duel.step(
            &InputFrame {
                move_stick: [0.0, 1.0],
                buttons: Buttons {
                    attack: i % 8 == 0,
                    lock_on: i == 0,
                    ..Buttons::default()
                },
                ..InputFrame::default()
            },
            DT,
        );
        for (h, result) in &r.hits {
            if h.attacker == 0 {
                counts[if result.deflected {
                    2
                } else if result.guarded {
                    1
                } else {
                    0
                }] += 1;
                broken |= result.posture_broke;
                eprintln!("tick {i}: {result:?}, enemy {:?}", r.enemy.animation);
            }
        }
        for e in duel.ai.as_mut().unwrap().drain_log() {
            eprintln!("tick {i}: {e:?}");
        }
    }
    eprintln!("soldier landed/blocked/deflected: {counts:?}");
    assert!(
        counts.iter().sum::<i32>() >= 3,
        "too few contacts: {counts:?}"
    );
    assert!(
        counts[1] + counts[2] > 0,
        "soldier never guarded: {counts:?}"
    );
    assert!(!broken, "mashing broke posture: {counts:?}");
}
