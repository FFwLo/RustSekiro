//! Tactics as plan steps (`docs/BRAIN.md`): each runs through the engine's own native goal on
//! the soldier (c1010) and the Great Shinobi Owl (c5060), ends within its cap and moves him as
//! its name says; plans of several steps run in order and his reflexes still interrupt them;
//! the request modes ask when they should (a `checkpoints` fight makes a handful of requests);
//! the Lua AI covers for a missing answer; and nothing is requested or logged once a fighter
//! is dead. Skips when `cache/` lacks the enemy.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use sekiro_sim::brain::{
    BossActions, BossBrain, BrainReply, BrainRequest, LuaBrain, Memory, Plan, RequestMode,
    ScriptedBrain,
};
use sekiro_sim::duel::Duel;
use sekiro_sim::enemy::EnemySpec;
use sekiro_sim::lua_ai::AiEvent;
use sekiro_sim::{Buttons, InputFrame};

const DT: f32 = 1.0 / 60.0;

#[derive(Clone, Copy)]
enum Who {
    Soldier,
    Owl,
}

impl Who {
    fn chr(self) -> &'static str {
        match self {
            Who::Soldier => "c1010",
            Who::Owl => "c5060",
        }
    }
}

fn load(who: Who, distance: f32) -> Option<Duel> {
    let cache = sekiro_sim::cache::root();
    let (tae, bundle, spec) = match who {
        Who::Soldier => (
            "anim/c1010/tae.json",
            "raw/script/m11_00_00_00.luabnd.d",
            EnemySpec::soldier(),
        ),
        Who::Owl => (
            "anim/c5060/tae.json",
            "raw/script/m11_01_00_00.luabnd.d",
            EnemySpec::owl(),
        ),
    };
    if !cache.join(tae).exists() || !cache.join(bundle).is_dir() {
        eprintln!("skipping: {} is not extracted", who.chr());
        return None;
    }
    Some(Duel::load_with(&cache, distance, spec).unwrap())
}

fn table(who: Who) -> &'static BossActions {
    BossActions::for_chr(who.chr()).unwrap()
}

/// One plan, asked for again at every slot.
fn repeating(who: Who, text: &str, delay: f32) -> Box<ScriptedBrain> {
    let plan = table(who).parse(text).unwrap();
    Box::new(ScriptedBrain::new(plan.clone(), plan, u32::MAX, delay))
}

fn keep_wolf_fresh(duel: &mut Duel) {
    let wolf = &mut duel.rules.fighters[0];
    wolf.vitality.hp = wolf.vitality.max_hp;
    wolf.posture.remaining = wolf.posture.max;
}

fn keep_both_fresh(duel: &mut Duel) {
    keep_wolf_fresh(duel);
    let boss = &mut duel.rules.fighters[1];
    boss.vitality.hp = boss.vitality.max_hp;
    boss.posture.remaining = boss.posture.max;
}

fn dist(duel: &Duel) -> f32 {
    let (p, e) = (duel.player.body.position, duel.enemy.body.position);
    ((p[0] - e[0]).powi(2) + (p[2] - e[2]).powi(2)).sqrt()
}

/// World angle of the boss around Wolf, degrees.
fn orbit(duel: &Duel) -> f32 {
    let (p, e) = (duel.player.body.position, duel.enemy.body.position);
    (e[0] - p[0]).atan2(e[2] - p[2]).to_degrees()
}

fn angle_gap(a: f32, b: f32) -> f32 {
    let d = (a - b).rem_euclid(360.0);
    d.min(360.0 - d)
}

#[derive(Debug)]
struct TacticRun {
    start_dist: f32,
    end_dist: f32,
    secs: f32,
    orbit_change: f32,
    max_dist: f32,
    min_dist: f32,
    saw_guard: bool,
}

/// Runs `text` once, from the step its native goal (`kind`) starts to the step it ends, with
/// Wolf standing still and unhurt.
fn run_tactic(who: Who, start: f32, text: &str, kind: i32) -> Option<TacticRun> {
    let mut duel = load(who, start)?;
    duel.set_brain(repeating(who, text, 0.0), false).unwrap();
    let mut began: Option<(f32, f32, f32)> = None;
    let mut min_dist = f32::MAX;
    let mut max_dist = 0.0f32;
    let mut saw_guard = false;
    for i in 0..60 * 20 {
        keep_wolf_fresh(&mut duel);
        let r = duel.step(&InputFrame::default(), DT);
        let planned = !duel.boss_brain.as_ref().unwrap().history.is_empty();
        let events = duel.ai.as_mut().unwrap().drain_log();
        let t = i as f32 * DT;
        if began.is_some() {
            min_dist = min_dist.min(dist(&duel));
            max_dist = max_dist.max(dist(&duel));
            saw_guard |= r.enemy.animation.as_deref() == Some("a000_003100");
        }
        for e in events {
            match e {
                AiEvent::Error(e) => panic!("AI error: {e}"),
                AiEvent::GoalStart { kind: k, .. } if k == kind && planned && began.is_none() => {
                    began = Some((t, dist(&duel), orbit(&duel)));
                }
                AiEvent::GoalEnd { kind: k, .. } if k == kind && began.is_some() => {
                    let (t0, d0, o0) = began.unwrap();
                    return Some(TacticRun {
                        start_dist: d0,
                        end_dist: dist(&duel),
                        secs: t - t0,
                        orbit_change: angle_gap(orbit(&duel), o0),
                        max_dist,
                        min_dist,
                        saw_guard,
                    });
                }
                _ => {}
            }
        }
    }
    panic!("{text}: the tactic never ended ({began:?})");
}

const WHO: [Who; 2] = [Who::Soldier, Who::Owl];

/// Plans that move first and then attack, next to the soldier's own attack actions.
fn range_plans(with_attacks: bool) -> Vec<String> {
    let mut plans: Vec<String> = if with_attacks {
        table(Who::Soldier)
            .actions
            .iter()
            .filter(|a| a.kind == sekiro_sim::brain::actions::ActionKind::Attack)
            .map(|a| a.id.to_string())
            .collect()
    } else {
        Vec::new()
    };
    plans.extend(
        [
            "walk_in4>3013",
            "walk_in2.5>3013",
            "keep_range>3013",
            "walk_in6>3013",
            "strafe_right>3005+3007",
            "walk_in4>3000+3001",
            "keep_range>3000",
            "back_off6>3000",
            "retreat12>3000",
        ]
        .map(str::to_string),
    );
    plans
}

/// Plays `text` against the soldier from `distance` until its last action ends; reports a
/// failure when no hit of his ever connected. `None` when the soldier is not extracted.
fn range_contact(distance: f32, text: &str) -> Option<Option<String>> {
    let mut duel = load(Who::Soldier, distance)?;
    let ai = duel.ai.as_mut().unwrap();
    ai.start_in_battle();
    ai.enable_act_hook(&[]);
    ai.state.act_hook.as_mut().unwrap().choice = Some(
        table(Who::Soldier)
            .act_for_plan(
                &table(Who::Soldier).parse(text).unwrap(),
                duel.fighters[1].radius,
            )
            .unwrap(),
    );
    let mut contacts = 0;
    let mut active = Vec::new();
    let mut previous = false;
    let last = *table(Who::Soldier)
        .parse(text)
        .unwrap()
        .actions()
        .last()
        .unwrap();
    let mut last_started = false;
    let mut start = None;
    let mut reach = 0.0f32;
    let wolf_position = duel.player.body.position;
    for _ in 0..600 {
        keep_both_fresh(&mut duel);
        duel.player.body.position = wolf_position;
        let r = duel.step(&InputFrame::default(), DT);
        let anim = duel
            .enemy
            .behavior
            .runtime
            .main_clip()
            .and_then(|c| sekiro_sim::lua_ai::anim_id_from_clip(&c.animation));
        if last_started && anim != Some(last) {
            break;
        }
        last_started |= anim == Some(last);
        if start.is_none() && anim.is_some_and(|id| (3000..3100).contains(&id)) {
            start = Some(duel.enemy.body.position);
        }
        if let Some(start) = start {
            reach = reach.max(
                duel.fighters[1]
                    .active_reach(&duel.enemy, start, duel.fighters[0].radius)
                    .unwrap_or(0.0),
            );
        }
        let now = !duel.enemy.tae_frame().attacks.is_empty();
        if now && !previous {
            active.push(dist(&duel));
        }
        previous = now;
        contacts += r.hits.iter().filter(|(h, _)| h.attacker == 1).count();
        let events = duel.ai.as_mut().unwrap().drain_log();
        if events
            .iter()
            .any(|e| matches!(e, AiEvent::GoalEnd { kind: 101000, .. }))
        {
            break;
        }
    }
    eprintln!("RANGE {distance} {text}: active={active:.2?} reach={reach:.2} contacts={contacts}");
    Some((contacts == 0).then(|| format!("{distance} {text}: {active:?}")))
}

fn range_failures(distances: &[f32], plans: &[String]) -> Vec<String> {
    use rayon::prelude::*;
    // Every run builds its own `Duel`, so the sweep spreads over the cores.
    let jobs: Vec<(f32, &String)> = distances
        .iter()
        .flat_map(|d| plans.iter().map(move |p| (*d, p)))
        .collect();
    jobs.par_iter()
        .filter_map(|(d, p)| range_contact(*d, p).flatten())
        .collect()
}

/// Each tactic-then-attack plan lands a hit from a mid distance.
#[test]
fn plan_attack_range_reproduction() {
    let failures = range_failures(&[2.5], &range_plans(false));
    assert!(failures.is_empty(), "{failures:?}");
}

/// The same for every soldier attack and four starting distances.
#[test]
#[ignore = "slow: about a hundred duels (every soldier attack at four distances)"]
fn plan_attack_range_reproduction_full_sweep() {
    let failures = range_failures(&[1.6, 2.5, 4.0, 6.0], &range_plans(true));
    assert!(failures.is_empty(), "{failures:?}");
}

#[test]
fn walk_in_and_run_in_close_to_their_band() {
    for who in WHO {
        for (text, kind, start, band, cap) in [
            ("walk_in4", 2019, 8.0, 4.0, 5.0),
            ("walk_in2.5", 2019, 7.0, 2.5, 5.0),
            ("run_in3", 2019, 9.0, 3.0, 4.0),
        ] {
            let Some(r) = run_tactic(who, start, text, kind) else {
                return;
            };
            eprintln!("{} {text}: {r:?}", who.chr());
            assert!(r.secs <= cap + 0.25, "{text}: {r:?}");
            // (The Owl acts first, so the start distance is whatever his own act left.)
            if r.start_dist <= band {
                assert!(r.secs < 0.2, "{text} is already there: {r:?}");
                continue;
            }
            let closed = f32::min(1.5, r.start_dist - band - 0.3);
            assert!(r.end_dist < r.start_dist - closed, "{text}: {r:?}");
            // Inside the band when it finished early; within a step of it at the cap.
            if r.secs < cap - 0.1 {
                assert!(r.end_dist <= band + 0.4, "{text}: {r:?}");
            }
        }
    }
}

#[test]
fn reactive_tactics_answer_pressure_and_keep_the_attack_chain() {
    for who in WHO {
        for (text, kind, mash) in [
            ("deflect>3000+3001", -10002, true),
            ("dodge_back>3002", -10003, true),
            ("deflect>3000+3001", -10002, false),
            ("dodge_back>3002", -10003, false),
        ] {
            // Owl's corresponding ranged punish is 3011.
            let text = if matches!(who, Who::Owl) && kind == -10003 {
                "dodge_back>3011"
            } else {
                text
            };
            let expected_attack = table(who).parse(text).unwrap().actions()[0];
            let Some(mut duel) = load(who, 1.6) else {
                return;
            };
            let choice = table(who)
                .act_for_plan(&table(who).parse(text).unwrap(), duel.fighters[1].radius)
                .unwrap();
            let ai = duel.ai.as_mut().unwrap();
            ai.start_in_battle();
            ai.enable_act_hook(&[]);
            ai.state.act_hook.as_mut().unwrap().choice = Some(choice);
            let mut began = None;
            let mut ended = None;
            let mut guarded = false;
            let mut step_requested = false;
            let mut attack_requested = false;
            let mut step_distance = None;
            let mut max_step_gain = 0.0f32;
            for i in 0..600 {
                keep_both_fresh(&mut duel);
                let input = if mash && began.is_some() {
                    swing(i, 8)
                } else {
                    InputFrame::default()
                };
                let r = duel.step(&input, DT);
                guarded |= r
                    .hits
                    .iter()
                    .any(|(h, result)| h.attacker == 0 && result.guarded);
                for event in duel.ai.as_mut().unwrap().drain_log() {
                    match event {
                        AiEvent::GoalStart { kind: k, .. } if k == kind => began = Some(i),
                        AiEvent::GoalEnd { kind: k, .. } if k == kind => ended = Some(i),
                        AiEvent::Action(id) if table(who).back_steps.contains(&id) => {
                            step_requested = true;
                            step_distance = Some(dist(&duel));
                        }
                        AiEvent::Action(id) if began.is_some() && id == expected_attack => {
                            attack_requested = true
                        }
                        AiEvent::Error(e) => panic!("{e}"),
                        _ => {}
                    }
                }
                if let Some(d) = step_distance {
                    max_step_gain = max_step_gain.max(dist(&duel) - d);
                }
                if ended.is_some() && attack_requested && (!mash || kind == -10003 || guarded) {
                    break;
                }
            }
            eprintln!(
                "{} {text} mash={mash}: {began:?}..{ended:?}, guarded={guarded}, step={step_requested}, gain={max_step_gain:.2}, attack={attack_requested}",
                who.chr()
            );
            assert!(attack_requested, "{text}: no following attack");
            assert!(
                ended.unwrap() - began.unwrap() <= 92,
                "{text}: exceeded 1.5 s cap"
            );
            if mash && kind == -10002 {
                assert!(guarded, "{text}: no script guard/deflect");
            }
            if mash && kind == -10003 {
                assert!(
                    step_requested && max_step_gain > 0.2,
                    "{text}: did not step back"
                );
            }
            if !mash {
                assert!(!step_requested && !guarded);
            }
        }
    }
}

#[test]
fn coach_reports_measured_attack_ranges_per_plan() {
    let Some(mut duel) = load(Who::Soldier, 4.0) else {
        return;
    };
    duel.set_brain(repeating(Who::Soldier, "walk_in4>3000+3001", 0.0), false)
        .unwrap();
    for _ in 0..900 {
        keep_both_fresh(&mut duel);
        duel.step(&InputFrame::default(), DT);
        duel.ai.as_mut().unwrap().drain_log();
    }
    let driver = duel.boss_brain.as_mut().unwrap();
    driver.end_fight(sekiro_sim::brain::FightEnd::Unfinished);
    let mut input = driver.coach_input(driver.last_record.as_ref().unwrap());
    let tactic = driver
        .tally
        .counts(sekiro_sim::brain::Side::Boss, "walk_in4");
    eprintln!("COACH walk_in4: {tactic:?}; plans={:?}", input.plans);
    assert_eq!(tactic.whiffed, 0, "movement is not a whiffed attack");
    let [distance, reach] = input.range["walk_in4>3000+3001"];
    assert!(distance > 0.0 && reach > distance, "{distance} {reach}");
    assert!(
        input.fit(sekiro_sim::brain::coach::INPUT_BUDGET) <= sekiro_sim::brain::coach::INPUT_BUDGET
    );
    assert!(input.range.contains_key("walk_in4>3000+3001"));
}

#[test]
fn retreat_rechecks_range_before_a_close_slash() {
    let Some(mut duel) = load(Who::Soldier, 1.6) else {
        return;
    };
    let choice = table(Who::Soldier)
        .act_for_plan(
            &table(Who::Soldier).parse("retreat12>3000").unwrap(),
            duel.fighters[1].radius,
        )
        .unwrap();
    let ai = duel.ai.as_mut().unwrap();
    ai.start_in_battle();
    ai.enable_act_hook(&[]);
    ai.state.act_hook.as_mut().unwrap().choice = Some(choice);
    let wolf = duel.player.body.position;
    let mut attack_start = None;
    let mut active_distance = None;
    let mut reach = 0.0f32;
    let mut contacted = false;
    for _ in 0..900 {
        keep_both_fresh(&mut duel);
        duel.player.body.position = wolf;
        let r = duel.step(&InputFrame::default(), DT);
        let attacking = r.enemy.animation.as_deref() == Some("a000_003000");
        if attack_start.is_some() && !attacking {
            break;
        }
        if attacking && attack_start.is_none() {
            attack_start = Some((duel.enemy.body.position, dist(&duel)));
        }
        if let Some((start, _)) = attack_start
            && let Some(r) =
                duel.fighters[1].active_reach(&duel.enemy, start, duel.fighters[0].radius)
        {
            reach = reach.max(r);
            active_distance.get_or_insert(dist(&duel));
        }
        contacted |= r.hits.iter().any(|(h, _)| h.attacker == 1);
        duel.ai.as_mut().unwrap().drain_log();
    }
    eprintln!(
        "RETREAT slash: start={:.2} first_active={active_distance:?} reach={reach:.2} contacted={contacted}",
        attack_start.unwrap().1
    );
    assert!(contacted);
}

#[test]
fn back_off_and_retreat_open_the_distance() {
    for who in WHO {
        for (text, kind, start, cap, gain) in [
            ("back_off6", 2016, 3.0, 4.0, 1.0),
            ("back_off9", 2016, 3.0, 4.0, 1.0),
            ("retreat12", 2016, 3.0, 5.0, 2.5),
        ] {
            let Some(r) = run_tactic(who, start, text, kind) else {
                return;
            };
            eprintln!("{} {text}: {r:?}", who.chr());
            let band: f32 = text
                .trim_start_matches(|c: char| !c.is_ascii_digit())
                .parse()
                .unwrap();
            if r.start_dist >= band {
                // Already far enough (the Owl acts first): it succeeds at once.
                assert!(r.secs < 0.2, "{text}: {r:?}");
                continue;
            }
            assert!(
                r.end_dist > r.start_dist + f32::min(gain, band - r.start_dist - 0.5),
                "{text}: {r:?}"
            );
            assert!(r.secs <= cap + 0.25, "{text}: {r:?}");
        }
    }
}

#[test]
fn hold_keeps_still_and_ends_at_its_cap() {
    for who in WHO {
        for (text, secs) in [("hold1", 1.0), ("hold2", 2.0), ("hold3.5", 3.5)] {
            let Some(r) = run_tactic(who, 5.0, text, 2000) else {
                return;
            };
            eprintln!("{} {text}: {r:?}", who.chr());
            assert!(r.max_dist - r.min_dist < 0.25, "{text} moved: {r:?}");
            assert!((r.end_dist - r.start_dist).abs() < 0.25, "{text}: {r:?}");
            assert!(
                (r.secs - secs).abs() < 0.2,
                "{text} should last {secs} s: {r:?}"
            );
        }
    }
}

#[test]
fn strafe_changes_the_bearing_and_keeps_the_distance() {
    for who in WHO {
        for text in ["strafe_left", "strafe_right"] {
            let Some(r) = run_tactic(who, 5.0, text, 2017) else {
                return;
            };
            eprintln!("{} {text}: {r:?}", who.chr());
            let metres = r.orbit_change.to_radians() * r.start_dist;
            assert!(metres > 0.2, "{text} moved {metres} m: {r:?}");
            assert!((r.secs - 2.0).abs() < 0.25, "{text}: {r:?}");
        }
    }
    // Left and right go opposite ways around him.
    let Some(mut left) = load(Who::Soldier, 5.0) else {
        return;
    };
    let mut right = load(Who::Soldier, 5.0).unwrap();
    left.set_brain(repeating(Who::Soldier, "strafe_left", 0.0), false)
        .unwrap();
    right
        .set_brain(repeating(Who::Soldier, "strafe_right", 0.0), false)
        .unwrap();
    let mut signs = [0.0f32; 2];
    for (i, duel) in [&mut left, &mut right].into_iter().enumerate() {
        let mut first = None;
        for _ in 0..60 * 10 {
            keep_wolf_fresh(duel);
            duel.step(&InputFrame::default(), DT);
            duel.ai.as_mut().unwrap().drain_log();
            if duel.boss_brain.as_ref().unwrap().history.is_empty() {
                continue;
            }
            let o = orbit(duel);
            let f = *first.get_or_insert(o);
            signs[i] = ((o - f + 540.0).rem_euclid(360.0) - 180.0).signum();
            if angle_gap(o, f) > 5.0 {
                break;
            }
        }
    }
    assert_ne!(signs[0], signs[1], "{signs:?}");
}

#[test]
fn keep_range_holds_four_to_six_metres_for_its_whole_time() {
    for who in WHO {
        let Some(r) = run_tactic(who, 9.0, "keep_range", 2018) else {
            return;
        };
        eprintln!("{} keep_range: {r:?}", who.chr());
        assert!((r.secs - 2.5).abs() < 0.25, "{r:?}");
        if r.start_dist > 6.5 {
            assert!(r.end_dist < r.start_dist - 2.0, "{r:?}");
        }
        // Already inside the band it still lasts its time rather than succeeding at once.
        let Some(inside) = run_tactic(who, 5.0, "keep_range", 2018) else {
            return;
        };
        assert!((inside.secs - 2.5).abs() < 0.25, "{inside:?}");
        if (4.0..6.0).contains(&inside.start_dist) {
            assert!(
                (inside.end_dist - inside.start_dist).abs() < 1.0,
                "{inside:?}"
            );
        }
    }
}

#[test]
fn guard_holds_the_guard_action_for_its_time() {
    for who in WHO {
        let Some(r) = run_tactic(who, 5.0, "guard2.5", 2101) else {
            return;
        };
        eprintln!("{} guard2.5: {r:?}", who.chr());
        assert!((r.secs - 2.5).abs() < 0.25, "{r:?}");
        assert!(r.saw_guard, "he never played his guard: {r:?}");
        // His guard clip carries a little root motion (the Owl steps about a metre).
        assert!((r.end_dist - r.start_dist).abs() < 1.2, "{r:?}");
    }
}

/// The native goal kinds that start in order, with the actions he requests, while one plan
/// runs (from its first goal on).
fn goal_log(who: Who, start: f32, text: &str, secs: f32) -> Option<(Vec<i32>, Vec<i32>)> {
    let mut duel = load(who, start)?;
    duel.set_brain(repeating(who, text, 0.0), false).unwrap();
    let (mut kinds, mut actions) = (Vec::new(), Vec::new());
    let mut began = None;
    for i in 0..60 * 20 {
        keep_wolf_fresh(&mut duel);
        duel.step(&InputFrame::default(), DT);
        let planned = !duel.boss_brain.as_ref().unwrap().history.is_empty();
        if planned && began.is_none() {
            began = Some(i);
        }
        for e in duel.ai.as_mut().unwrap().drain_log() {
            if !planned || began.is_none_or(|b| i as f32 * DT > b as f32 * DT + secs) {
                continue;
            }
            match e {
                AiEvent::GoalStart { kind, .. }
                    if [2000, 2016, 2017, 2018, 2019].contains(&kind) =>
                {
                    kinds.push(kind)
                }
                AiEvent::Action(a) => actions.push(a),
                _ => {}
            }
        }
    }
    Some((kinds, actions))
}

#[test]
fn a_three_step_plan_runs_in_order() {
    for (who, text, start, attack) in [
        (Who::Soldier, "walk_in4>hold1>3013", 8.0, 3013),
        (Who::Owl, "back_off6>hold1>3005", 3.0, 3005),
    ] {
        let Some((kinds, actions)) = goal_log(who, start, text, 9.0) else {
            return;
        };
        eprintln!("{text}: goals {kinds:?} actions {actions:?}");
        let first_attack = actions.iter().position(|a| *a == attack);
        assert!(first_attack.is_some(), "{text}: {actions:?}");
        // Movement, then the wait, then the attack: the first goals are those steps in order.
        let move_kind = if text.starts_with("walk_in") {
            2019
        } else {
            2016
        };
        assert_eq!(kinds[..2], [move_kind, 2000], "{text}: {kinds:?}");
    }
}

/// Wolf swings every `period` steps from the start, so he attacks the boss during a plan.
fn swing(i: usize, period: usize) -> InputFrame {
    InputFrame {
        buttons: Buttons {
            attack: i.is_multiple_of(period),
            lock_on: i == 2,
            ..Buttons::default()
        },
        ..InputFrame::default()
    }
}

#[test]
fn his_reflexes_still_interrupt_a_tactic_plan() {
    for (who, text, start) in [
        (Who::Soldier, "hold3.5>3002", 2.6),
        (Who::Owl, "hold3.5>3011", 2.6),
    ] {
        let Some(mut duel) = load(who, start) else {
            return;
        };
        duel.set_brain(repeating(who, text, 0.1), false).unwrap();
        let (mut reflex_in_plan, mut plans_cut) = (0, 0);
        let mut seen = 0;
        let mut plan_actions: Vec<i32> = Vec::new();
        for i in 0..60 * 40 {
            keep_both_fresh(&mut duel);
            duel.step(&swing(i, 45), DT);
            let driver = duel.boss_brain.as_ref().unwrap();
            let running = !driver.running.is_empty();
            if driver.history.len() > seen {
                seen = driver.history.len();
                if !plan_actions.is_empty()
                    && !plan_actions.contains(
                        &table(who)
                            .get(match who {
                                Who::Soldier => 3002,
                                Who::Owl => 3011,
                            })
                            .unwrap()
                            .id,
                    )
                {
                    plans_cut += 1;
                }
                plan_actions.clear();
            }
            for e in duel.ai.as_mut().unwrap().drain_log() {
                if let AiEvent::Action(a) = e {
                    if running && (3100..=3103).contains(&a) {
                        reflex_in_plan += 1;
                    }
                    plan_actions.push(a);
                }
            }
        }
        let stats = duel.boss_brain.as_ref().unwrap().stats;
        eprintln!(
            "{}: {stats:?}, guards during plans {reflex_in_plan}, plans cut before their attack {plans_cut}",
            who.chr()
        );
        assert!(stats.planned >= 3, "{stats:?}");
        assert!(reflex_in_plan > 0, "no guard or deflect while a plan ran");
        assert!(plans_cut > 0, "a reflex never cut a plan short");
    }
}

type Seen = Rc<RefCell<Vec<(f32, String, Option<&'static str>)>>>;

/// Keeps every request (with its `obs.t`, plan and why) and answers like the scripted brain.
struct Recorder {
    inner: ScriptedBrain,
    seen: Seen,
    mode: RequestMode,
}

impl BossBrain for Recorder {
    fn name(&self) -> &'static str {
        "recorder"
    }
    fn default_mode(&self) -> RequestMode {
        self.mode
    }
    fn submit(&mut self, request: &BrainRequest) {
        self.seen
            .borrow_mut()
            .push((request.obs.t, request.obs.plan.clone(), request.why));
        self.inner.submit(request);
    }
    fn poll(&mut self, now: f32) -> Option<BrainReply> {
        self.inner.poll(now)
    }
}

fn recorded(who: Who, text: &str, delay: f32, mode: RequestMode) -> (Box<Recorder>, Seen) {
    let seen: Seen = Rc::default();
    let plan = table(who).parse(text).unwrap();
    let inner = ScriptedBrain::new(plan.clone(), plan, u32::MAX, delay);
    (
        Box::new(Recorder {
            inner,
            seen: seen.clone(),
            mode,
        }),
        seen,
    )
}

#[test]
fn a_decision_is_requested_when_the_plan_finishes_in_plan_end_mode() {
    let Some(mut slot_duel) = load(Who::Soldier, 5.0) else {
        return;
    };
    let mut end_duel = load(Who::Soldier, 5.0).unwrap();
    // A 6 s plan: three holds of 2 s.
    let text = "hold2>hold2>hold2";
    let mut results = Vec::new();
    for (duel, mode) in [
        (&mut slot_duel, RequestMode::Slot),
        (&mut end_duel, RequestMode::PlanEnd),
    ] {
        let (brain, seen) = recorded(Who::Soldier, text, 0.1, mode);
        duel.set_brain(brain, false).unwrap();
        let mut plan_started = None;
        let mut first_request_after = None;
        for _ in 0..60 * 30 {
            keep_wolf_fresh(duel);
            duel.step(&InputFrame::default(), DT);
            duel.ai.as_mut().unwrap().drain_log();
            let d = duel.boss_brain.as_ref().unwrap();
            if plan_started.is_none() && !d.history.is_empty() {
                plan_started = Some(d.time);
            }
            if let (Some(t0), None) = (plan_started, first_request_after) {
                // The first request made after the first plan started running.
                if seen.borrow().len() >= 2 {
                    first_request_after = Some(seen.borrow()[1].0 - t0);
                }
            }
        }
        results.push(first_request_after.expect("a second request"));
    }
    eprintln!(
        "seconds into the plan: slot {:.2}, plan_end {:.2}",
        results[0], results[1]
    );
    assert!(
        results[0] < 1.0,
        "slot mode asks as the plan starts: {results:?}"
    );
    assert!(
        (3.8..6.2).contains(&results[1]),
        "plan_end asks as the last step starts: {results:?}"
    );
}

#[test]
fn a_checkpoints_fight_makes_only_a_handful_of_requests() {
    for who in WHO {
        let Some(mut duel) = load(who, 4.0) else {
            return;
        };
        let (brain, seen) = recorded(who, "hold1>3000", 0.3, RequestMode::Checkpoints);
        duel.set_brain(brain, false).unwrap();
        assert_eq!(
            duel.boss_brain.as_ref().unwrap().mode,
            RequestMode::Checkpoints
        );
        // Fight until the first fight ends (either side dies), Wolf attacking steadily.
        let mut ended = false;
        for i in 0..60 * 240 {
            if i == 60 * 20 {
                // Make sure the fight ends: Wolf dies.
                duel.rules.fighters[0].vitality.hp = 0;
            }
            let r = duel.step(&swing(i, 50), DT);
            duel.ai.as_mut().unwrap().drain_log();
            if r.respawned {
                ended = true;
                break;
            }
        }
        assert!(ended, "the fight never ended");
        let d = duel.boss_brain.as_ref().unwrap();
        let record = d.last_record.clone().expect("a fight record");
        let whys: Vec<_> = seen.borrow().iter().map(|r| r.2).collect();
        eprintln!(
            "{}: {} requests {whys:?}; {} plans, {} lua acts, choices {:?}",
            who.chr(),
            record.requests,
            record.decisions,
            record.lua_acts,
            record.choices
        );
        assert!((1..=7).contains(&record.requests), "{record:?}");
        assert_eq!(whys[0], Some("start"));
        assert!(
            whys.iter().filter(|w| **w == Some("start")).count() <= 2,
            "{whys:?}"
        );
        // One answer, many plans: the set persists and is sampled at every slot.
        assert!(record.decisions >= 3, "{record:?}");
        assert!(
            record.decisions > record.requests,
            "plans without asking again: {record:?}"
        );
    }
}

#[test]
fn lua_acts_while_no_answer_comes_in_checkpoints_mode() {
    let Some(mut duel) = load(Who::Soldier, 4.0) else {
        return;
    };
    let (brain, seen) = recorded(Who::Soldier, "hold1>3000", 1000.0, RequestMode::Checkpoints);
    duel.set_brain(brain, false).unwrap();
    let mut attacks = 0;
    for _ in 0..60 * 40 {
        keep_wolf_fresh(&mut duel);
        duel.step(&InputFrame::default(), DT);
        attacks += duel
            .ai
            .as_mut()
            .unwrap()
            .drain_log()
            .iter()
            .filter(|e| matches!(e, AiEvent::Action(a) if (3000..3100).contains(a)))
            .count();
    }
    let stats = duel.boss_brain.as_ref().unwrap().stats;
    eprintln!("{stats:?}; {attacks} attacks; requests {:?}", seen.borrow());
    // One request (the start), never answered; his own AI acts the whole time.
    assert_eq!((stats.requests, stats.replies, stats.planned), (1, 0, 0));
    assert!(stats.fallback >= 4 && attacks >= 4, "{stats:?} {attacks}");
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "sekiro-tactics-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn nothing_is_requested_or_logged_once_a_fighter_is_dead() {
    let Some(mut duel) = load(Who::Soldier, 4.0) else {
        return;
    };
    let (brain, _seen) = recorded(Who::Soldier, "walk_in4>3000+3001", 0.1, RequestMode::Slot);
    duel.set_brain(brain, false).unwrap();
    let dir = temp_dir("log");
    let driver = duel.boss_brain.as_mut().unwrap();
    let log = driver.log_to(&dir).unwrap();
    let mut died_at = None;
    let mut respawned = false;
    let mut requests_when_dead = None;
    for i in 0..60 * 120 {
        if i < 60 * 12 {
            keep_both_fresh(&mut duel);
        } else if i == 60 * 12 {
            duel.rules.fighters[0].vitality.hp = 1;
        }
        let r = duel.step(&swing(i, 50), DT);
        duel.ai.as_mut().unwrap().drain_log();
        if died_at.is_none() && duel.respawn_in.is_some() {
            died_at = Some(i);
            requests_when_dead = Some(duel.boss_brain.as_ref().unwrap().stats.requests);
        }
        if r.respawned {
            respawned = true;
            break;
        }
    }
    assert!(died_at.is_some() && respawned, "Wolf died and respawned");
    let stats = duel.boss_brain.as_ref().unwrap().stats;
    assert_eq!(
        Some(stats.requests),
        requests_when_dead,
        "a request after death"
    );
    let text = std::fs::read_to_string(&log).unwrap();
    let events: Vec<String> = text
        .lines()
        .map(|l| {
            let v: serde_json::Value = serde_json::from_str(l).unwrap();
            v["ev"].as_str().unwrap().to_string()
        })
        .collect();
    eprintln!("{events:?}");
    let end = events
        .iter()
        .position(|e| e == "fight_end")
        .expect("fight_end");
    let after: Vec<_> = events[end + 1..].iter().collect();
    // After the fight_end line: only its summary and the next fight's start, nothing else.
    for e in &after {
        assert!(
            ["fight_summary", "fight_start"].contains(&e.as_str()),
            "logged {e} after the fight ended: {after:?}"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_fight_summary_and_the_baseline_are_saved_with_the_metrics() {
    // The Lua brain fights as the baseline: its own memory file, with a fight record.
    let Some(mut duel) = load(Who::Soldier, 4.0) else {
        return;
    };
    duel.set_brain(Box::new(LuaBrain), false).unwrap();
    let dir = temp_dir("baseline");
    let path = dir.join("c1010.lua.json");
    duel.boss_brain.as_mut().unwrap().use_memory(path.clone());
    for i in 0..60 * 90 {
        if i < 60 * 15 {
            keep_both_fresh(&mut duel);
        } else if i == 60 * 15 {
            duel.rules.fighters[0].vitality.hp = 1;
        }
        let input = if i < 60 * 15 {
            InputFrame {
                move_stick: [0.0, 1.0],
                ..swing(i, 50)
            }
        } else {
            InputFrame::default()
        };
        let r = duel.step(&input, DT);
        duel.ai.as_mut().unwrap().drain_log();
        if r.respawned {
            break;
        }
    }
    let record = duel
        .boss_brain
        .as_ref()
        .unwrap()
        .last_record
        .clone()
        .expect("a record");
    eprintln!("{}", record.summary_line());
    assert_eq!(record.winner, "boss");
    assert!(record.metrics.hits_taken >= 1, "{record:?}");
    assert!(record.metrics.duration > 15.0 && record.metrics.kill_time.is_none());
    assert!(record.metrics.hp_lost > 0.0);
    assert_eq!(record.requests, 0, "the Lua brain asks nothing");
    let (saved, _) = Memory::load(
        &sekiro_sim::brain::memory::LayerPaths::new(&path, "default").player,
        "c1010",
    );
    assert_eq!(
        (saved.version, saved.fights, saved.history.len()),
        (sekiro_sim::brain::memory::MEMORY_VERSION, 1, 1)
    );
    assert_eq!(saved.history[0], record);

    let _ = std::fs::remove_dir_all(&dir);
    // A plan type is available for the tests above.
    let _ = Plan::default();
}

#[test]
fn checkpoint_events_ask_once_each() {
    let Some(mut duel) = load(Who::Soldier, 4.0) else {
        return;
    };
    let (brain, seen) = recorded(
        Who::Soldier,
        "hold2>strafe_left",
        0.2,
        RequestMode::Checkpoints,
    );
    duel.set_brain(brain, false).unwrap();
    // This test is about the checkpoints, with plans that have no attack.
    duel.boss_brain.as_mut().unwrap().anti_stall = false;
    for i in 0..60 * 40 {
        keep_wolf_fresh(&mut duel);
        let boss = &mut duel.rules.fighters[1];
        let damage = match i / 60 {
            8..=11 | 20..=23 => 0.10,
            12..=15 | 24..=27 => 0.35,
            _ => 0.0,
        };
        boss.posture.remaining = (boss.posture.max as f32 * (1.0 - damage)) as i32;
        if i == 60 * 28 {
            // The boss drops below half HP, and stays there.
            let boss = &mut duel.rules.fighters[1];
            boss.vitality.hp = boss.vitality.max_hp * 2 / 5;
        }
        duel.step(&InputFrame::default(), DT);
        duel.ai.as_mut().unwrap().drain_log();
    }
    let whys: Vec<_> = seen.borrow().iter().map(|r| r.2).collect();
    eprintln!("{whys:?}");
    assert_eq!(
        whys,
        [
            Some("start"),
            Some("pressure"),
            Some("pressure_critical"),
            Some("pressure"),
            Some("pressure_critical"),
            Some("half_hp")
        ]
    );
    let d = duel.boss_brain.as_ref().unwrap();
    assert!(d.stats.planned >= 8, "{:?}", d.stats);
}

#[test]
fn deathblow_checkpoints_wait_for_getup_and_only_report_surviving_marks() {
    for who in [Who::Soldier, Who::Owl] {
        let Some(mut duel) = load(who, 1.6) else {
            continue;
        };
        let (brain, seen) = recorded(who, "hold2>strafe_left", 0.0, RequestMode::Checkpoints);
        duel.set_brain(brain, false).unwrap();
        duel.boss_brain.as_mut().unwrap().anti_stall = false;
        duel.enemy_active = false;
        duel.events = None;
        // Use the real attack, posture-break reaction, ThrowParam pair and get-up.
        // Manually run the request hook while passive so AI movement cannot move the target.
        let mut started = false;
        let mut broke = false;
        let mut checked_open = false;
        for i in 0..1200 {
            keep_wolf_fresh(&mut duel);
            if !broke {
                duel.rules.fighters[1].posture.remaining = 1;
            }
            let attack = if broke {
                !started && i % 10 == 0
            } else {
                i % 40 == 0
            };
            let r = duel.step(
                &InputFrame {
                    buttons: Buttons {
                        attack,
                        lock_on: (2..4).contains(&i),
                        ..Buttons::default()
                    },
                    ..InputFrame::default()
                },
                DT,
            );
            broke |= r
                .hits
                .iter()
                .any(|(h, res)| h.attacker == 0 && res.posture_broke);
            started |= r.deathblow.started.is_some();
            checked_open |= duel.deathblow_available();
            let blocked = duel.rules.deathblow_open(1)
                || duel.deathblow.is_some()
                || duel.respawn_in.is_some()
                || duel.rules.fighters[1].dead()
                || duel
                    .enemy
                    .behavior
                    .runtime
                    .main_state_path()
                    .first()
                    .is_some_and(|s| s.contains("Damage") || s.contains("Throw"));
            let count = seen.borrow().len();
            let mut driver = duel.boss_brain.take().unwrap();
            let mut ai = duel.ai.take().unwrap();
            driver.before_think(&mut ai, 1.0);
            driver.after_think(&mut ai, &duel);
            duel.ai = Some(ai);
            duel.boss_brain = Some(driver);
            if blocked {
                assert_eq!(
                    seen.borrow().len(),
                    count,
                    "request during an unavailable decision"
                );
            }
            if started
                && (duel.rules.fighters[1].dead()
                    || seen.borrow().iter().any(|r| r.2 == Some("mark_lost")))
            {
                break;
            }
        }
        assert!(
            broke && started && checked_open,
            "real deathblow did not run for {}",
            who.chr()
        );
        let marks = seen
            .borrow()
            .iter()
            .filter(|r| r.2 == Some("mark_lost"))
            .count();
        assert_eq!(marks, usize::from(matches!(who, Who::Owl)), "{}", who.chr());
        if matches!(who, Who::Owl) {
            assert_eq!(duel.rules.fighters[1].vitality.deathblows_left, 1);
        }
    }
}
