//! The coach flow in a real duel (the Ashina soldier): the first fight is his own AI with no
//! calls, a game plan that arrives during the respawn plays from the next fight (opener, then
//! the start set), a late plan plays one fight later, a failed coach keeps the old plan, the
//! checkpoint chooser switches or keeps sets; and the hard limits against stalling and the turn
//! before an attack. Skips when `cache/` lacks him.

use std::cell::RefCell;
use std::rc::Rc;

use sekiro_sim::InputFrame;
use sekiro_sim::brain::coach::{Avoid, PlanSet, WeightedChoice};
use sekiro_sim::brain::{
    BossBrain, BrainReply, BrainRequest, CoachReply, CoachRequest, GamePlan, Plan, SOLDIER,
    ScriptedBrain, ScriptedCoach,
};
use sekiro_sim::duel::Duel;
use sekiro_sim::enemy::EnemySpec;

const DT: f32 = 1.0 / 60.0;

#[test]
fn fresh_player_runs_shared_opener_in_a_real_duel_or_thin_lua_baseline() {
    for fights in [19, 20] {
        let Some((mut duel, coach)) = coached(Vec::new(), 0.0) else {
            return;
        };
        let dir = std::env::temp_dir().join(format!(
            "sekiro-starter-{}-{}-{}",
            std::process::id(),
            fights,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let path = dir.join("c1010.json");
        let mut legacy = sekiro_sim::brain::Memory::new("c1010");
        legacy.fights = fights;
        legacy.coach.plan = Some(plan("old player hypothesis"));
        legacy.save(&path).unwrap();
        let d = duel.boss_brain.as_mut().unwrap();
        d.coach_interval = 0.0;
        d.use_player_memory(path, "newcomer");
        run(&mut duel, 10.0);
        if fights >= 20 {
            assert_eq!(played(&duel).first().map(String::as_str), Some("3002"));
            assert!(driver(&duel).stats.planned > 0);
            assert!(driver(&duel).game_plan.as_ref().unwrap().notes.is_empty());
        } else {
            assert!(driver(&duel).game_plan.is_none());
            assert_eq!(driver(&duel).stats.planned, 0);
            assert!(driver(&duel).stats.fallback > 0);
        }
        assert!(coach.borrow().coach_inputs.is_empty());
    }
}

#[test]
fn short_interval_waits_for_twenty_seconds_and_answer_waits_for_slot() {
    let Some((mut duel, coach)) = coached(vec![Some(plan("mid")); 5], 2.0) else {
        return;
    };
    duel.boss_brain.as_mut().unwrap().coach_interval = 5.0;
    run(&mut duel, 19.9);
    assert!(coach.borrow().coach_inputs.is_empty());
    run(&mut duel, 0.2);
    assert_eq!(coach.borrow().coach_inputs.len(), 1);
    // Only poll the coach, without giving Lua a decision slot.
    let old = driver(&duel).game_plan.clone();
    duel.boss_brain.as_mut().unwrap().tick(2.1);
    assert_eq!(driver(&duel).game_plan, old);
    run(&mut duel, 10.0);
    assert_eq!(driver(&duel).game_plan.as_ref().unwrap().notes, "mid");
    assert!(!played(&duel).contains(&"3002".into()));
}

#[test]
fn long_fight_reviews_at_45_and_90_then_post_and_zero_disables() {
    use rayon::prelude::*;
    // Each interval is its own two-minute fight, so they run side by side.
    [45.0, 0.0].par_iter().for_each(|&interval| {
        let Some((mut duel, coach)) = coached(vec![Some(plan("mid")); 4], 2.0) else {
            return;
        };
        duel.boss_brain.as_mut().unwrap().coach_interval = interval;
        run(&mut duel, 120.0);
        let inputs = coach.borrow().coach_inputs.clone();
        if interval == 0.0 {
            assert!(inputs.is_empty());
        } else {
            assert_eq!(inputs.len(), 2);
            for (input, expected) in inputs.iter().zip([45.0, 90.0]) {
                let value: serde_json::Value = serde_json::from_str(input).unwrap();
                let elapsed = value["elapsed"].as_f64().unwrap();
                assert!((elapsed - expected).abs() < 0.1, "{elapsed}");
                assert_eq!(value["phase"], "mid");
                assert!(input.len() <= sekiro_sim::brain::coach::INPUT_BUDGET);
            }
            assert_eq!(driver(&duel).game_plan.as_ref().unwrap().notes, "mid");
            assert!(
                !played(&duel).contains(&"3002".into()),
                "mid opener must be skipped"
            );
        }
        die(&mut duel);
        assert_eq!(coach.borrow().coach_inputs.len(), inputs.len() + 1);
        assert_eq!(
            driver(&duel).last_record.as_ref().unwrap().mid_coach[0],
            if interval == 0.0 { 0 } else { 2 }
        );
    });
}

#[test]
fn mark_lost_at_30_resets_timer() {
    let Some((mut duel, coach)) = coached(vec![Some(plan("mid")); 4], 2.0) else {
        return;
    };
    duel.rules.fighters[1].vitality.deathblows_left = 2;
    run(&mut duel, 30.0);
    duel.rules.fighters[1].vitality.deathblows_left = 1;
    run(&mut duel, 48.0);
    let c = coach.borrow();
    assert_eq!(c.coach_inputs.len(), 2);
    assert!((c.coach_times[0] - 30.0).abs() < 0.1);
    assert!((c.coach_times[1] - 60.0).abs() < 0.1);
}

#[test]
fn slow_call_serializes_post_review() {
    let Some((mut duel, coach)) = coached(vec![Some(plan("slow")); 4], 60.0) else {
        return;
    };
    duel.boss_brain.as_mut().unwrap().coach_interval = 45.0;
    run(&mut duel, 100.0);
    assert_eq!(coach.borrow().coach_inputs.len(), 1);
    die(&mut duel);
    run(&mut duel, 8.0);
    let c = coach.borrow();
    assert_eq!(c.coach_inputs.len(), 2);
    let second: serde_json::Value = serde_json::from_str(&c.coach_inputs[1]).unwrap();
    assert_eq!(second["phase"], "post");
}

fn load(distance: f32) -> Option<Duel> {
    let cache = sekiro_sim::cache::root();
    if !cache.join("anim/c1010/tae.json").exists()
        || !cache.join("raw/script/m11_00_00_00.luabnd.d").is_dir()
    {
        eprintln!("skipping: the soldier is not extracted");
        return None;
    }
    Some(Duel::load_with(&cache, distance, EnemySpec::soldier()).unwrap())
}

fn fresh(duel: &mut Duel) {
    for f in &mut duel.rules.fighters {
        f.vitality.hp = f.vitality.max_hp;
        f.posture.remaining = f.posture.max;
    }
}

fn run(duel: &mut Duel, secs: f32) {
    for i in 0..(secs / DT) as usize {
        fresh(duel);
        duel.step(
            &InputFrame {
                buttons: sekiro_sim::Buttons {
                    guard: i % 120 < 100,
                    ..Default::default()
                },
                ..InputFrame::default()
            },
            DT,
        );
    }
}

#[test]
fn no_input_fight_is_void_and_never_coached_or_saved() {
    let Some((mut duel, coach)) = coached(vec![Some(plan("unused"))], 0.0) else {
        return;
    };
    let path = std::env::temp_dir().join(format!("sekiro-void-{}.json", std::process::id()));
    duel.boss_brain.as_mut().unwrap().use_memory(path.clone());
    for _ in 0..720 {
        fresh(&mut duel);
        duel.step(&InputFrame::default(), DT);
    }
    assert_eq!(driver(&duel).memory_now().fights, 0);
    die(&mut duel);
    let d = driver(&duel);
    let record = d.last_record.as_ref().unwrap();
    assert!(record.metrics.void);
    assert!(
        record
            .summary_line()
            .contains("fight void: no player input")
    );
    assert_eq!(d.memory.fights, 0);
    assert!(d.memory.history.is_empty() && d.memory.boss.is_empty() && d.memory.wolf.is_empty());
    assert_eq!(d.memory.habits, Default::default());
    assert!(coach.borrow().coach_inputs.is_empty());
    let saved = sekiro_sim::brain::Memory::load(
        &sekiro_sim::brain::memory::LayerPaths::new(&path, "default").player,
        "c1010",
    )
    .0;
    assert_eq!(saved.fights, 0);
    drop(duel);
    let _ = std::fs::remove_file(path);
}

/// Wolf dies; steps until the respawn (the next fight has started).
fn die(duel: &mut Duel) {
    duel.rules.fighters[0].vitality.hp = 0;
    for _ in 0..60 * 30 {
        if duel.step(&InputFrame::default(), DT).respawned {
            return;
        }
        // Keep the boss whole while Wolf lies dead.
        let boss = &mut duel.rules.fighters[1];
        boss.vitality.hp = boss.vitality.max_hp;
    }
    panic!("no respawn");
}

fn set(name: &str, plan: &str) -> PlanSet {
    PlanSet {
        name: name.into(),
        description: format!("the {name} set"),
        plans: vec![WeightedChoice {
            plan: plan.into(),
            weight: 1.0,
        }],
        when: None,
    }
}

fn plan(notes: &str) -> GamePlan {
    GamePlan {
        punish_preferences: Default::default(),
        checkpoint_prefs: Default::default(),
        boss_phase: "c1010".into(),
        counter_weights: Default::default(),
        version: 1,
        opener: "3002".into(),
        sets: vec![set("a", "3000+3001"), set("b", "3005+3007")],
        start_set: "a".into(),
        avoid: vec![Avoid {
            choice: "3003".into(),
            reason: "deflected".into(),
        }],
        notes: notes.into(),
        retry: Vec::new(),
        on_wolf_break: Vec::new(),
        principles: Vec::new(),
        boss_principles: Vec::new(),
        boss_bans: Vec::new(),
        explore: Default::default(),
        move_controls: Default::default(),
        lessons: Default::default(),
    }
}

/// Lets a test keep a hand on the scripted coach the driver owns.
struct Shared(Rc<RefCell<ScriptedCoach>>);

impl BossBrain for Shared {
    fn name(&self) -> &'static str {
        "coach"
    }
    fn coaches(&self) -> bool {
        true
    }
    fn default_mode(&self) -> sekiro_sim::brain::RequestMode {
        sekiro_sim::brain::RequestMode::Checkpoints
    }
    fn submit(&mut self, r: &BrainRequest) {
        self.0.borrow_mut().submit(r);
    }
    fn poll(&mut self, now: f32) -> Option<BrainReply> {
        self.0.borrow_mut().poll(now)
    }
    fn submit_coach(&mut self, r: &CoachRequest, now: f32) {
        self.0.borrow_mut().submit_coach(r, now);
    }
    fn poll_coach(&mut self, now: f32) -> Option<CoachReply> {
        self.0.borrow_mut().poll_coach(now)
    }
}

fn coached(
    plans: Vec<Option<GamePlan>>,
    coach_delay: f32,
) -> Option<(Duel, Rc<RefCell<ScriptedCoach>>)> {
    let mut duel = load(4.0)?;
    let coach = Rc::new(RefCell::new(ScriptedCoach::new(plans, coach_delay, 0.2)));
    duel.set_brain(Box::new(Shared(coach.clone())), false)
        .unwrap();
    Some((duel, coach))
}

fn played(duel: &Duel) -> Vec<String> {
    duel.boss_brain
        .as_ref()
        .unwrap()
        .history
        .iter()
        .map(Plan::value)
        .collect()
}

fn driver(duel: &Duel) -> &sekiro_sim::brain::BrainDriver {
    duel.boss_brain.as_ref().unwrap()
}

#[test]
fn wolf_posture_crossings_reach_the_checkpoint_chooser_and_rearm() {
    let Some((mut duel, coach)) = coached(Vec::new(), 0.0) else {
        return;
    };
    duel.player.body.position[2] += 25.0;
    let d = duel.boss_brain.as_mut().unwrap();
    d.game_plan = Some(plan("posture pressure"));
    d.start_fight();
    for fraction in [0.3, 0.6, 0.8, 0.1, 0.3] {
        for _ in 0..20 {
            fresh(&mut duel);
            let wolf = &mut duel.rules.fighters[0];
            wolf.posture.remaining = (wolf.posture.max as f32 * (1.0 - fraction)) as i32;
            duel.step(&InputFrame::default(), DT);
        }
    }
    let c = coach.borrow();
    let events: Vec<_> = c
        .checkpoint_requests
        .iter()
        .filter_map(|(why, _)| *why)
        .collect();
    assert_eq!(
        events
            .iter()
            .filter(|&&why| why == "wolf_posture_25")
            .count(),
        2,
        "{events:?}"
    );
    assert_eq!(
        events
            .iter()
            .filter(|&&why| why == "wolf_posture_50")
            .count(),
        1,
        "{events:?}"
    );
    assert_eq!(
        events
            .iter()
            .filter(|&&why| why == "wolf_posture_75")
            .count(),
        1,
        "{events:?}"
    );
    assert!(
        c.checkpoint_briefs
            .iter()
            .flatten()
            .any(|b| b.contains("wolf_posture_75"))
    );
}

#[test]
fn fight_one_makes_no_calls_and_a_plan_that_arrives_in_the_respawn_plays_from_fight_two() {
    let Some((mut duel, coach)) = coached(
        vec![
            Some(plan("he deflects thrusts")),
            Some(plan("second notes")),
        ],
        2.0,
    ) else {
        return;
    };
    // Fight 1, no memory: his own AI, no checkpoint call, no coach call.
    run(&mut duel, 12.0);
    assert!(coach.borrow().checkpoint_requests.is_empty());
    assert!(coach.borrow().coach_inputs.is_empty());
    assert!(driver(&duel).game_plan.is_none());
    assert_eq!(driver(&duel).stats.requests, 0);
    assert!(played(&duel).is_empty(), "his own AI plays fight 1");
    assert!(driver(&duel).stats.fallback > 0);
    // The fight ends: exactly one coach call, sent at once; the plan (2 s) is in before the
    // respawn (6 s), so fight 2 starts with it.
    die(&mut duel);
    assert_eq!(coach.borrow().coach_inputs.len(), 1);
    let first = coach.borrow().coach_inputs[0].clone();
    eprintln!("coach input of fight 1 ({} bytes): {first}", first.len());
    assert!(
        first.len() <= sekiro_sim::brain::coach::INPUT_BUDGET,
        "{}",
        first.len()
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&first).unwrap()["result"]["won"],
        "boss"
    );
    assert!(!first.contains("\"played\""), "fight 1 played no plan");
    assert!(
        driver(&duel).game_plan.is_some(),
        "the plan arrived in time"
    );
    assert_eq!(driver(&duel).active_set_name(), Some("a"));
    // Fight 2: the opener first, then the start set; the chooser is asked at `start`.
    run(&mut duel, 25.0);
    let plans = played(&duel);
    eprintln!("fight 2 plans: {plans:?}");
    assert_eq!(plans[0], "3002", "the opener runs first");
    assert!(plans.len() >= 3, "{plans:?}");
    assert!(
        plans[1..].iter().all(|p| p == "3000+3001"
            || p.starts_with("run_in")
                && driver(&duel).history.iter().any(|plan| plan.value() == *p
                    && sekiro_sim::brain::limits::has_attack(plan, &SOLDIER))),
        "{plans:?}"
    );
    {
        let c = coach.borrow();
        assert_eq!(
            c.checkpoint_requests.len(),
            1,
            "{:?}",
            c.checkpoint_requests
        );
        assert_eq!(c.checkpoint_requests[0].0, Some("start"));
        assert_eq!(c.checkpoint_requests[0].1, ["a", "b"]);
    }
    {
        let c = coach.borrow();
        let brief = c.checkpoint_briefs[0].as_deref().expect("a brief");
        eprintln!("checkpoint input ({} bytes): {brief}", brief.len());
        // Deliberately raised from 200: up to 768 bytes of coach intent plus scalars.
        assert!(brief.len() < 1000, "{brief}");
        let input: serde_json::Value = serde_json::from_str(brief).unwrap();
        assert_eq!(input["coach"]["notes"], "he deflects thrusts");
        assert!(!input["coach"]["intent"].as_str().unwrap().is_empty());
        assert!(
            brief.contains("\"why\":\"start\"") && brief.contains("\"set\":\"a\""),
            "{brief}"
        );
        for out in ["memory", "tally", "habits", "obs"] {
            assert!(!brief.contains(out), "{brief}");
        }
    }
    // The coach's notes come back in its next request, with the plan that was played.
    die(&mut duel);
    let second = coach.borrow().coach_inputs[1].clone();
    assert!(
        second.len() <= sekiro_sim::brain::coach::INPUT_BUDGET,
        "{}",
        second.len()
    );
    assert!(
        second.contains("\"notes\":\"he deflects thrusts\""),
        "{second}"
    );
    assert!(second.contains("\"played\":{"), "{second}");
    assert!(second.contains("\"sets\":{\"a\":"), "{second}");
    assert!(second.contains("\"opener\":"), "{second}");
    // Fight 3 plays the second plan.
    assert_eq!(
        driver(&duel).game_plan.as_ref().unwrap().notes,
        "second notes"
    );
}

#[test]
fn a_late_plan_plays_from_the_fight_after() {
    // 12 s after the fight ends: after the 6 s respawn, during fight 2.
    let Some((mut duel, coach)) = coached(vec![Some(plan("late")), Some(plan("later"))], 12.0)
    else {
        return;
    };
    run(&mut duel, 8.0);
    die(&mut duel);
    run(&mut duel, 9.0);
    assert_eq!(coach.borrow().coach_inputs.len(), 1);
    assert!(
        driver(&duel).game_plan.is_none(),
        "the late plan waits for the next fight"
    );
    assert!(played(&duel).is_empty(), "fight 2 is his own AI");
    assert!(coach.borrow().checkpoint_requests.is_empty());
    die(&mut duel);
    assert_eq!(driver(&duel).game_plan.as_ref().unwrap().notes, "late");
    run(&mut duel, 4.0);
    assert_eq!(played(&duel)[0], "3002");
}

#[test]
fn a_failed_coach_keeps_the_old_plan() {
    let Some((mut duel, coach)) = coached(vec![Some(plan("kept")), None], 1.0) else {
        return;
    };
    run(&mut duel, 8.0);
    die(&mut duel);
    run(&mut duel, 5.0);
    assert_eq!(played(&duel)[0], "3002");
    die(&mut duel);
    // The second coach call failed (during the respawn): the plan stays.
    assert_eq!(coach.borrow().coach_inputs.len(), 2);
    assert_eq!(driver(&duel).game_plan.as_ref().unwrap().notes, "kept");
    run(&mut duel, 5.0);
    let plans = played(&duel);
    assert_eq!(
        plans.iter().filter(|p| *p == "3002").count(),
        2,
        "{plans:?}"
    );
}

#[test]
fn a_plan_with_nothing_valid_is_ignored() {
    let junk = GamePlan {
        opener: "zzz".into(),
        sets: vec![set("a", "3031")],
        start_set: "a".into(),
        ..GamePlan::default()
    };
    let Some((mut duel, _coach)) = coached(vec![Some(junk)], 1.0) else {
        return;
    };
    run(&mut duel, 8.0);
    die(&mut duel);
    run(&mut duel, 6.0);
    assert!(driver(&duel).game_plan.is_none());
    assert!(played(&duel).is_empty(), "his own AI plays");
}

#[test]
fn the_checkpoint_chooser_switches_sets_and_keep_keeps() {
    let Some((mut duel, coach)) = coached(vec![Some(plan("n"))], 1.0) else {
        return;
    };
    run(&mut duel, 6.0);
    die(&mut duel);
    assert_eq!(driver(&duel).active_set_name(), Some("a"));
    // The `start` checkpoint's answer switches to b.
    coach.borrow_mut().choice = Some("b".into());
    run(&mut duel, 3.0);
    assert_eq!(driver(&duel).active_set_name(), Some("b"));
    // Half HP raises a checkpoint; `keep` leaves the set as it is.
    coach.borrow_mut().choice = Some("keep".into());
    for _ in 0..180 {
        let wolf = &mut duel.rules.fighters[0];
        wolf.vitality.hp = wolf.vitality.max_hp;
        let boss = &mut duel.rules.fighters[1];
        boss.vitality.hp = (boss.vitality.max_hp as f32 * 0.4) as _;
        boss.posture.remaining = boss.posture.max;
        duel.step(&InputFrame::default(), DT);
    }
    let c = coach.borrow();
    assert_eq!(
        c.checkpoint_requests.len(),
        2,
        "{:?}",
        c.checkpoint_requests
    );
    assert_eq!(c.checkpoint_requests[1].0, Some("half_hp"));
    drop(c);
    assert_eq!(driver(&duel).active_set_name(), Some("b"));
    // The next plans come from set b.
    run(&mut duel, 15.0);
    let plans = played(&duel);
    assert!(plans.iter().any(|p| p == "3005+3007"), "{plans:?}");
}

fn attack_starts(duel: &mut Duel, secs: f32) -> Vec<f32> {
    let mut starts = Vec::new();
    for i in 0..(secs / DT) as usize {
        fresh(duel);
        let r = duel.step(&InputFrame::default(), DT);
        let attack = r
            .enemy
            .animation
            .as_deref()
            .and_then(|a| a.strip_prefix("a000_00"))
            .and_then(|n| n.parse::<i32>().ok())
            .is_some_and(|n| (3000..3100).contains(&n));
        if r.enemy.clip_started && attack {
            starts.push(i as f32 * DT);
        }
    }
    starts
}

fn single_plan_brain(text: &str) -> Box<ScriptedBrain> {
    let plan = SOLDIER.parse(text).unwrap();
    Box::new(ScriptedBrain::new(plan.clone(), plan, u32::MAX, 0.0))
}

#[test]
fn a_set_of_only_retreats_still_attacks_within_six_seconds() {
    let Some(mut duel) = load(4.0) else { return };
    duel.set_brain(single_plan_brain("retreat12"), false)
        .unwrap();
    duel.boss_brain.as_mut().unwrap().anti_stall = true;
    let starts = attack_starts(&mut duel, 60.0);
    let stats = driver(&duel).stats;
    eprintln!("{stats:?}; attacks at {starts:?}");
    // The attack-free retreat is refused; the sheet supplies attacks and approaches.
    assert!(
        !driver(&duel)
            .history
            .iter()
            .any(|p| p.value() == "retreat12")
    );
    assert!(stats.planned > 0, "{stats:?}");
    assert!(starts.len() >= 4, "{starts:?}");
    let mut last = 0.0;
    for s in &starts {
        assert!(s - last <= 9.0, "gap before {s}: {starts:?}");
        last = *s;
    }
    assert!(starts[0] <= 6.5, "{starts:?}");
}

#[test]
fn retreating_plans_run_at_most_once_per_five_seconds() {
    let Some(mut duel) = load(4.0) else { return };
    duel.set_brain(single_plan_brain("back_off6>3009"), false)
        .unwrap();
    duel.boss_brain.as_mut().unwrap().anti_stall = true;
    let mut times = Vec::new();
    let mut seen = 0;
    for _ in 0..60 * 80 {
        fresh(&mut duel);
        duel.step(&InputFrame::default(), DT);
        let d = driver(&duel);
        if d.history.len() > seen {
            if d.history[seen..]
                .iter()
                .any(|p| p.value() == "back_off6>3009")
            {
                times.push(d.time);
            }
            seen = d.history.len();
        }
    }
    eprintln!("retreating plans started at {times:?}");
    assert!(times.len() >= 2, "{times:?}");
    for w in times.windows(2) {
        assert!(w[1] - w[0] >= 4.9, "{times:?}");
    }
}

#[test]
fn an_attack_after_retreat12_turns_to_face_wolf_first() {
    let Some(mut duel) = load(4.0) else { return };
    // Wolf stays 2 m behind him while he runs away facing away (the retreat's 5 s), then
    // stands still: the kick (3005) needs no approach and must not swing at empty air.
    let plan = SOLDIER.parse("retreat12>3005").unwrap();
    duel.set_brain(
        Box::new(ScriptedBrain::new(plan.clone(), plan, u32::MAX, 0.0)),
        false,
    )
    .unwrap();
    let mut plan_started = None;
    let mut worst_before = 0.0f32;
    let mut at_swing = None;
    for i in 0..60 * 25 {
        fresh(&mut duel);
        let t = i as f32 * DT;
        if plan_started.is_none() && !driver(&duel).history.is_empty() {
            plan_started = Some(t);
        }
        if plan_started.is_some_and(|s| t < s + 5.0) {
            let e = duel.enemy.body.position;
            let yaw = duel.enemy.body.yaw;
            duel.player.body.position = [e[0] + 2.0 * yaw.sin(), e[1], e[2] + 2.0 * yaw.cos()];
        }
        let r = duel.step(&InputFrame::default(), DT);
        let bearing = |d: &Duel| {
            let (p, e) = (d.player.body.position, d.enemy.body.position);
            let to = (-(p[0] - e[0])).atan2(-(p[2] - e[2]));
            let mut a = to - d.enemy.body.yaw;
            while a > std::f32::consts::PI {
                a -= std::f32::consts::TAU;
            }
            while a < -std::f32::consts::PI {
                a += std::f32::consts::TAU;
            }
            a.to_degrees().abs()
        };
        if at_swing.is_none() && plan_started.is_some() {
            worst_before = worst_before.max(bearing(&duel));
        }
        if r.enemy.clip_started && r.enemy.animation.as_deref() == Some("a000_003005") {
            at_swing = Some(bearing(&duel));
            break;
        }
    }
    let at_swing = at_swing.expect("the kick started");
    eprintln!("bearing at the swing {at_swing:.1} deg, worst while retreating {worst_before:.1}");
    assert!(worst_before > 90.0, "he really ran with his back to Wolf");
    assert!(
        at_swing <= 30.0,
        "swung with Wolf {at_swing:.1} deg off his facing"
    );
}
