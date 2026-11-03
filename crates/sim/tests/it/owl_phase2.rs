//! The Great Shinobi Owl's phase 2, run by the map's own event script
//! (`event/m11_01_00_00.emevd`, see `docs/OWL.md`, "Phase 2"): his first deathblow's get-up
//! raises event message 20, event 11115870 sends him into his heat-up (`a000_020005`, HKS
//! command 506000), gives him SpEffect 200051 and hides the boss bar until his smoke escape;
//! his AI then picks its phase-2 acts; only the second deathblow ends the fight. Skips when
//! `cache/` lacks the Owl or the event script.

use sekiro_sim::duel::{Duel, DuelReport};
use sekiro_sim::enemy::EnemySpec;
use sekiro_sim::lua_ai::AiEvent;
use sekiro_sim::{Buttons, InputFrame};

const DT: f32 = 1.0 / 60.0;

/// SpEffect "behaviour pattern change 1", the phase-2 marker his AI reads.
const PHASE_2: i32 = 200_051;
/// SpEffect "action judgment, heat-up animation".
const HEAT_UP: i32 = 3_506_080;
/// The flag event 11115870 sets once the heat-up is over.
const PHASE_2_SEEN: i32 = 11_110_860;
/// The Owl's defeat flag (event 11115850).
const DEFEATED: i32 = 9308;

fn load(distance: f32) -> Option<Duel> {
    let cache = sekiro_sim::cache::root();
    if !cache.join("anim/c5060/tae.json").exists()
        || !cache.join("raw/script/m11_01_00_00.luabnd.d").is_dir()
        || !cache.join("raw/event/m11_01_00_00.emevd").is_file()
    {
        eprintln!("skipping: the Owl or his map's event script is not extracted");
        return None;
    }
    let duel = Duel::load_with(&cache, distance, EnemySpec::owl()).unwrap();
    assert!(duel.events.is_some(), "the Owl's map events did not load");
    Some(duel)
}

fn frame(attack: bool, lock_on: bool) -> InputFrame {
    InputFrame {
        buttons: Buttons {
            attack,
            lock_on,
            ..Buttons::default()
        },
        ..InputFrame::default()
    }
}

fn assert_no_script_errors(r: &DuelReport) {
    assert!(
        r.player.behavior.script_errors.is_empty() && r.enemy.behavior.script_errors.is_empty(),
        "{:?} {:?}",
        r.player.behavior.script_errors,
        r.enemy.behavior.script_errors
    );
}

fn keep_wolf_fresh(duel: &mut Duel) {
    let wolf = &mut duel.rules.fighters[0];
    wolf.vitality.hp = wolf.vitality.max_hp;
    wolf.posture.remaining = wolf.posture.max;
}

fn owl_clip(duel: &Duel) -> String {
    duel.enemy
        .behavior
        .runtime
        .main_clip()
        .map(|c| c.animation.clone())
        .unwrap_or_default()
}

fn has_effect(duel: &Duel, id: i32) -> bool {
    duel.enemy.tae_frame().sp_effects.contains(&id)
}

/// Reach the transition through a real first deathblow, then let Wolf recover his controls.
fn transition(duel: &mut Duel, clip: &str) {
    duel.enemy_active = false;
    let (_, killed, _) = break_and_deathblow(duel);
    assert!(!killed);
    for _ in 0..900 {
        let r = duel.step(&InputFrame::default(), DT);
        assert_no_script_errors(&r);
        if owl_clip(duel) == clip {
            let p = duel.player.body.position;
            duel.enemy.body.position = [p[0], p[1], p[2] - 1.6];
            duel.player.body.yaw = 0.0;
            duel.enemy.body.yaw = std::f32::consts::PI;
            assert!(has_effect(duel, HEAT_UP));
            assert_eq!(duel.rules.fighters[1].vitality.hp, 1);
            assert_eq!(duel.rules.fighters[1].vitality.deathblows_left, 1);
            return;
        }
    }
    panic!("never reached {clip}: {}", owl_clip(duel));
}

#[test]
fn no_second_deathblow_during_heat_up_or_kneel() {
    for clip in ["a000_020005", "a000_021003"] {
        let Some(mut duel) = load(1.6) else { return };
        transition(&mut duel, clip);
        assert!(!duel.deathblow_available(), "red mark during {clip}");
        let mut landed = false;
        for i in 0..60 {
            let r = duel.step(&frame(i % 20 == 0, false), DT);
            assert_no_script_errors(&r);
            assert!(
                r.deathblow.started.is_none(),
                "second deathblow during {clip}: {:?}",
                r.deathblow.started
            );
            landed |= r.hits.iter().any(|(h, _)| h.attacker == 0);
            assert!(!r.deathblow.killed);
            assert_eq!(duel.rules.fighters[1].vitality.deathblows_left, 1);
            assert!(
                !duel.deathblow_available(),
                "red mark during {clip}, now {}",
                owl_clip(&duel)
            );
            assert!(has_effect(&duel, HEAT_UP), "slash interrupted {clip}");
        }
        assert!(
            landed,
            "ordinary slashes did not contact the Owl during {clip}"
        );
    }
}

#[test]
fn slashing_the_kneeling_owl_triggers_attack_escape_and_phase_two() {
    let Some(mut duel) = load(1.6) else { return };
    transition(&mut duel, "a000_021003");
    // Enable the actual Lua interrupt handler, without walking into its observed area.
    duel.enemy_active = true;
    let mut slash = false;
    let mut escape = false;
    for i in 0..180 {
        keep_wolf_fresh(&mut duel);
        let r = duel.step(&frame(i == 0, false), DT);
        assert_no_script_errors(&r);
        assert!(r.deathblow.started.is_none());
        slash |= r
            .player
            .behavior
            .state_path
            .iter()
            .any(|s| s.contains("Attack"));
        escape |= owl_clip(&duel) == "a000_020007";
        if escape && flag(&duel, PHASE_2_SEEN) {
            break;
        }
    }
    assert!(
        escape,
        "attack did not request escape 20007: {}",
        owl_clip(&duel)
    );
    assert!(slash, "Wolf never performed an ordinary slash");
    assert!(flag(&duel, PHASE_2_SEEN));
    assert!(has_effect(&duel, PHASE_2));
    assert_eq!(boss_bar(&duel), Some(905_060));
    let owl = &duel.rules.fighters[1];
    assert_eq!(owl.vitality.hp, owl.vitality.max_hp);
    assert_eq!(owl.vitality.deathblows_left, 1);
    assert!(!owl.dead());
    assert!(!flag(&duel, DEFEATED));
}

fn flag(duel: &Duel, id: i32) -> bool {
    duel.events.as_ref().unwrap().scripts.flag(id)
}

fn boss_bar(duel: &Duel) -> Option<i32> {
    duel.events.as_ref().unwrap().boss_bar
}

/// Breaks the Owl's posture (held at 1 point) with slashes and deathblows him. Returns the
/// ThrowParam rows, whether it killed and the Owl's clips, once the deathblow pair is over.
fn break_and_deathblow(duel: &mut Duel) -> (Option<(i32, i32)>, bool, Vec<String>) {
    let mut clips: Vec<String> = Vec::new();
    let mut started = None;
    let mut killed = false;
    let mut broke = false;
    for i in 0..1500 {
        keep_wolf_fresh(duel);
        if !broke {
            let owl = &mut duel.rules.fighters[1].posture;
            owl.remaining = owl.remaining.min(1);
        }
        let attack = if broke {
            started.is_none() && i % 10 == 0
        } else {
            i % 40 == 0
        };
        let r = duel.step(&frame(attack, (2..4).contains(&i)), DT);
        assert_no_script_errors(&r);
        broke |= r
            .hits
            .iter()
            .any(|(h, res)| h.attacker == 0 && res.posture_broke);
        started = started.or(r.deathblow.started);
        killed |= r.deathblow.killed;
        let c = owl_clip(duel);
        if clips.last() != Some(&c) {
            clips.push(c);
        }
        if started.is_some() && duel.deathblow.is_none() {
            break;
        }
    }
    assert!(broke, "the Owl's posture never broke");
    (started, killed, clips)
}

#[test]
fn first_deathblow_starts_phase_two_from_the_event_script() {
    let Some(mut duel) = load(1.6) else { return };
    duel.enemy_active = false;
    // Event 11115860 (joined after its intro) shows the boss bar 2 s into the fight.
    for _ in 0..150 {
        duel.step(&InputFrame::default(), DT);
    }
    assert_eq!(boss_bar(&duel), Some(905_060));
    assert!(!has_effect(&duel, PHASE_2));
    assert!(duel.rules.fighters[1].vitality.cannot_die, "not immortal");

    let (rows, killed, mut clips) = break_and_deathblow(&mut duel);
    assert_eq!(rows, Some((15_060_000, 15_060_001)));
    assert!(!killed);

    // The get-up's event message 20 (at 2.83 s of a000_012000) starts the heat-up: command
    // 506000 to his script plays a000_020005, 200051 goes on and the boss bar goes away.
    let mut heat_up_at = None;
    for i in 0..600 {
        keep_wolf_fresh(&mut duel);
        let r = duel.step(&InputFrame::default(), DT);
        assert_no_script_errors(&r);
        let c = owl_clip(&duel);
        if clips.last() != Some(&c) {
            clips.push(c.clone());
        }
        if (c == "a000_020005" || clips.iter().any(|c| c == "a000_020005")) && heat_up_at.is_none()
        {
            heat_up_at = Some(i);
        }
        if heat_up_at.is_some_and(|t| i > t + 30) {
            break;
        }
    }
    eprintln!("Owl clips after the first deathblow: {clips:?}");
    assert!(heat_up_at.is_some(), "no heat-up: {clips:?}");
    assert!(
        clips
            .windows(2)
            .any(|w| w[0] == "a000_012000" && w[1] == "a000_020005"),
        "the heat-up should follow the get-up clip: {clips:?}"
    );
    assert!(has_effect(&duel, PHASE_2), "200051 not given");
    assert!(has_effect(&duel, HEAT_UP));
    assert_eq!(
        boss_bar(&duel),
        None,
        "the boss bar stays up during the heat-up"
    );
    assert!(!flag(&duel, PHASE_2_SEEN));
    assert_eq!(duel.rules.fighters[1].vitality.deathblows_left, 1);
}

#[test]
fn phase_two_ends_with_his_escape_and_brings_his_poison() {
    let Some(mut duel) = load(1.6) else { return };
    duel.enemy_active = false;
    let (_, killed, _) = break_and_deathblow(&mut duel);
    assert!(!killed);
    // His AI back on; Wolf stands next to him while he begs, so after 2.5 s of the heat-up his
    // observe area makes him throw smoke and escape (20006), which ends the heat-up.
    duel.enemy_active = true;
    let mut escape = None;
    let mut actions = std::collections::BTreeMap::<i32, usize>::new();
    let mut clips: Vec<String> = Vec::new();
    for i in 0..(60 * 60) {
        keep_wolf_fresh(&mut duel);
        let r = duel.step(&InputFrame::default(), DT);
        assert_no_script_errors(&r);
        let c = owl_clip(&duel);
        if clips.last() != Some(&c) {
            clips.push(c.clone());
        }
        if escape.is_none() && ["a000_020006", "a000_020007", "a000_020008"].contains(&c.as_str()) {
            escape = Some((i, c));
        }
        for e in duel.ai.as_mut().unwrap().drain_log() {
            match e {
                AiEvent::Action(a) => *actions.entry(a).or_default() += 1,
                AiEvent::Error(e) => panic!("AI error: {e}"),
                _ => {}
            }
        }
    }
    eprintln!("escape: {escape:?}");
    eprintln!("Owl actions in phase 2: {actions:?}");
    eprintln!("clips: {:?}", &clips[..clips.len().min(60)]);
    assert!(escape.is_some(), "he never left the heat-up: {clips:?}");
    assert!(flag(&duel, PHASE_2_SEEN), "event 11115870 did not finish");
    assert_eq!(
        boss_bar(&duel),
        Some(905_060),
        "the boss bar did not come back"
    );
    let owl = &duel.rules.fighters[1];
    assert!(owl.vitality.hp > 1, "his HP was not reset");
    assert!(has_effect(&duel, PHASE_2));
    // The phase-2-only act: poison (3030) instead of the kick's shuriken follow-up (3007).
    assert!(
        actions.contains_key(&3030),
        "no poison in phase 2: {actions:?}"
    );
    assert!(
        !actions.contains_key(&3007),
        "phase-1 follow-up in phase 2: {actions:?}"
    );
}

/// Lets the (passive) Owl settle, then puts him 1.6 m in front of Wolf, facing him.
fn face_off(duel: &mut Duel) {
    for _ in 0..240 {
        keep_wolf_fresh(duel);
        duel.step(&InputFrame::default(), DT);
    }
    let (p, yaw) = (duel.player.body.position, duel.player.body.yaw);
    duel.enemy.body.position = [p[0] - 1.6 * yaw.sin(), p[1], p[2] - 1.6 * yaw.cos()];
    duel.enemy.body.yaw = yaw + std::f32::consts::PI;
}

/// Steps with his AI on until event 11115870 has finished (his heat-up is over), Wolf walking
/// up to him when `approach` (else standing still); returns the clip he escaped with and the
/// step it started on, counted from the call.
fn finish_heat_up(duel: &mut Duel, approach: bool) -> (String, usize) {
    duel.enemy_active = true;
    let mut escape = None;
    for i in 0..(60 * 60) {
        keep_wolf_fresh(duel);
        let (p, e) = (duel.player.body.position, duel.enemy.body.position);
        let far = (e[0] - p[0]).hypot(e[2] - p[2]) > 1.2;
        let input = InputFrame {
            move_stick: if approach && far {
                [0.0, 1.0]
            } else {
                [0.0, 0.0]
            },
            camera_yaw: duel.player.yaw_to(e),
            ..InputFrame::default()
        };
        let r = duel.step(&input, DT);
        assert_no_script_errors(&r);
        let c = owl_clip(duel);
        if escape.is_none() && ["a000_020006", "a000_020007", "a000_020008"].contains(&c.as_str()) {
            escape = Some((c, i));
        }
        if flag(duel, PHASE_2_SEEN) {
            break;
        }
    }
    escape.expect("he never left the heat-up")
}

#[test]
fn walking_up_to_him_while_he_begs_makes_him_escape() {
    let Some(mut duel) = load(1.6) else { return };
    // His AI runs for a while first, so its battle goal is observing the heat-up SpEffect
    // before it turns on (an observer only sees an activation it was there for).
    duel.enemy_active = true;
    for _ in 0..120 {
        keep_wolf_fresh(&mut duel);
        duel.step(&InputFrame::default(), DT);
    }
    duel.enemy_active = false;
    face_off(&mut duel);
    let (_, killed, _) = break_and_deathblow(&mut duel);
    assert!(!killed);
    // His observe area (2 m all around, from 2.5 s into the heat-up) answers with the smoke
    // escape 20006 instead of waiting out five begging loops (20008).
    let (clip, at) = finish_heat_up(&mut duel, true);
    eprintln!("escaped with {clip} after {:.2} s", at as f32 * DT);
    assert_eq!(clip, "a000_020006");
    assert!(at < 60 * 12, "{at}");
    assert_eq!(boss_bar(&duel), Some(905_060));
    let owl = &duel.rules.fighters[1];
    assert_eq!(owl.vitality.hp, owl.vitality.max_hp, "20006 resets his HP");
}

#[test]
fn only_the_second_deathblow_ends_the_fight() {
    let Some(mut duel) = load(1.6) else { return };
    duel.enemy_active = false;
    let (_, killed, _) = break_and_deathblow(&mut duel);
    assert!(!killed);
    assert!(!duel.rules.fighters[1].dead());
    finish_heat_up(&mut duel, false);
    assert!(!flag(&duel, DEFEATED));
    // His AI off again, and him put back in front of Wolf, for a scripted second deathblow.
    duel.enemy_active = false;
    face_off(&mut duel);
    let (rows, killed, _) = break_and_deathblow(&mut duel);
    assert_eq!(rows, Some((15_060_180, 15_060_181)));
    assert!(killed, "the second deathblow did not kill");
    // The finishing clip's message 50 (at 8.87 s) makes event 11115850 handle the defeat.
    for _ in 0..(60 * 12) {
        duel.step(&InputFrame::default(), DT);
        if duel.events.as_ref().unwrap().defeated {
            break;
        }
    }
    assert!(duel.rules.fighters[1].dead());
    assert!(duel.events.as_ref().unwrap().defeated, "no boss defeat");
    assert!(flag(&duel, DEFEATED));
}

#[test]
fn phase_two_hp_depletion_still_opens_the_finishing_deathblow() {
    let Some(mut duel) = load(1.6) else { return };
    duel.enemy_active = false;
    let (_, killed, _) = break_and_deathblow(&mut duel);
    assert!(!killed);
    finish_heat_up(&mut duel, false);
    duel.enemy_active = false;
    face_off(&mut duel);
    // One ordinary landed hit reaches the protected HP floor without emptying posture.
    duel.rules.fighters[1].vitality.hp = 40;
    let mut hp_open = false;
    let mut hp_floor = false;
    let mut killed = false;
    let mut rows = None;
    for i in 0..1200 {
        keep_wolf_fresh(&mut duel);
        let r = duel.step(&frame(i % 20 == 0, false), DT);
        assert_no_script_errors(&r);
        if duel.rules.fighters[1].vitality.hp == 1 {
            if !hp_floor {
                assert!(duel.rules.fighters[1].posture.remaining > 0);
                hp_floor = true;
            }
            hp_open |= duel.deathblow_available();
        }
        rows = rows.or(r.deathblow.started);
        killed |= r.deathblow.killed;
        if killed {
            break;
        }
    }
    assert!(hp_open, "no HP-depletion deathblow in phase 2");
    assert_eq!(rows, Some((15_060_180, 15_060_181)));
    assert!(killed);
    assert!(duel.rules.fighters[1].dead());
    assert_eq!(duel.rules.fighters[1].vitality.deathblows_left, 0);
}

#[test]
fn in_phase_one_his_kick_follows_with_shuriken_not_poison() {
    let Some(mut duel) = load(3.0) else { return };
    let mut actions = std::collections::BTreeMap::<i32, usize>::new();
    for _ in 0..(60 * 60) {
        keep_wolf_fresh(&mut duel);
        let r = duel.step(&InputFrame::default(), DT);
        assert_no_script_errors(&r);
        for e in duel.ai.as_mut().unwrap().drain_log() {
            if let AiEvent::Action(a) = e {
                *actions.entry(a).or_default() += 1;
            }
        }
    }
    eprintln!("Owl actions in phase 1: {actions:?}");
    assert!(!has_effect(&duel, PHASE_2));
    // The kick's AtkParam (50601130) leaves 3506000 on Wolf; his AI answers with 3007.
    assert!(actions.contains_key(&3007), "{actions:?}");
    assert!(!actions.contains_key(&3030), "{actions:?}");
}
