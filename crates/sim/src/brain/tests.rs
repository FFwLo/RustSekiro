use super::actions::{OWL, PlanError, SOLDIER};
use super::habits::{HabitTracker, WolfFrame};
use super::memory::{FightEnd, Loaded, Memory};
use super::metrics::{self, FightMetrics, FightRecord, Trend};
use super::*;
use crate::lua_ai::NumberOnActivate;

#[test]
fn brain_checkpoint_context_keeps_reasoning_and_bounds_escaped_unicode() {
    use super::coach::{CHECKPOINT_CONTEXT_BUDGET, PlanSet};
    let mut plan = GamePlan {
        notes: "Bait the counter, then finish quickly".into(),
        sets: vec![PlanSet {
            name: "press".into(),
            description: "Stay close and punish recovery".into(),
            ..Default::default()
        }],
        principles: vec!["Wolf waits for long setups".into()],
        move_controls: vec![super::move_bandit::Control {
            plan: "3000".into(),
            confidence: 0.8,
            lock: true,
        }],
        ..Default::default()
    };
    let boss = vec!["Short slashes build posture".into()];
    let lessons = vec!["Old lesson".into(), "Punish the pause".into()];
    let context = plan.checkpoint_context(Some("press"), &boss, &lessons);
    eprintln!(
        "Decisions coach context: {} bytes",
        context.to_string().len()
    );
    assert_eq!(context["notes"], plan.notes);
    assert_eq!(context["intent"], plan.sets[0].description);
    assert_eq!(context["boss"][0], boss[0]);
    assert_eq!(context["player"][0], plan.principles[0]);
    assert_eq!(context["lessons"][1], lessons[1]);
    assert_eq!(
        context["confidence"][0],
        serde_json::json!(["3000", 0.8, true])
    );
    let huge = "雪\"\\\n".repeat(400);
    plan.notes = huge.clone();
    plan.sets[0].description = huge.clone();
    plan.principles = vec![huge.clone(); 6];
    plan.move_controls = (0..32)
        .map(|i| super::move_bandit::Control {
            plan: huge.clone(),
            confidence: i as f32 / 32.0,
            lock: false,
        })
        .collect();
    let context = plan.checkpoint_context(Some("press"), &vec![huge.clone(); 6], &vec![huge; 20]);
    assert!(context.to_string().len() <= CHECKPOINT_CONTEXT_BUDGET);
    assert!(!context["notes"].as_str().unwrap().is_empty());
    assert!(!context["intent"].as_str().unwrap().is_empty());
}

fn start(t: &mut Tally, time: f32, side: Side, key: &str) {
    t.record(
        time,
        TallyEvent::Start {
            side,
            key: key.into(),
        },
    );
}

fn hit(t: &mut Tally, time: f32, attacker: Side, guarded: bool, deflected: bool) {
    t.record(
        time,
        TallyEvent::Hit {
            attacker,
            guarded,
            deflected,
        },
    );
}

#[test]
fn tally_counts_each_outcome() {
    let mut t = Tally::default();
    // 3005 hits, then 3005 is deflected and punished, then 3031 whiffs, then a shuriken (3009)
    // is blocked after its clip ended.
    start(&mut t, 0.0, Side::Boss, "3005");
    hit(&mut t, 0.4, Side::Boss, false, false);
    t.record(1.0, TallyEvent::End { side: Side::Boss });
    start(&mut t, 2.0, Side::Boss, "3005");
    hit(&mut t, 2.4, Side::Boss, true, true);
    t.record(3.0, TallyEvent::End { side: Side::Boss });
    // Wolf's slash lands 0.5 s after the deflected kick ended: punished, and a hit for Wolf.
    start(&mut t, 3.2, Side::Wolf, "GroundAttackCombo1Release");
    hit(&mut t, 3.5, Side::Wolf, false, false);
    t.record(3.8, TallyEvent::End { side: Side::Wolf });
    start(&mut t, 5.0, Side::Boss, "3031");
    start(&mut t, 6.0, Side::Boss, "3009");
    t.record(6.5, TallyEvent::End { side: Side::Boss });
    hit(&mut t, 6.9, Side::Boss, true, false);
    let done = t.advance(10.0);
    assert_eq!(done.len(), 5);
    let kick = t.counts(Side::Boss, "3005");
    assert_eq!(
        kick,
        Counts {
            n: 2,
            hit: 1,
            blocked: 0,
            deflected: 1,
            whiffed: 0,
            punished: 1,
            countered: 0,
            counter_kinds: [0; 3],
        }
    );
    let throw = t.counts(Side::Boss, "3031");
    assert_eq!((throw.n, throw.whiffed, throw.punished), (1, 1, 0));
    let shuriken = t.counts(Side::Boss, "3009");
    assert_eq!((shuriken.n, shuriken.blocked, shuriken.whiffed), (1, 1, 0));
    let slash = t.counts(Side::Wolf, "GroundAttackCombo1Release");
    assert_eq!((slash.n, slash.hit, slash.punished), (1, 1, 0));
}

#[test]
fn punish_window_and_ranked_summary() {
    let mut t = Tally::default();
    start(&mut t, 0.0, Side::Boss, "3000");
    t.record(1.0, TallyEvent::End { side: Side::Boss });
    // 1.5 s after the end: outside the 1 s window, not a punish.
    hit(&mut t, 2.5, Side::Wolf, false, false);
    for (i, key) in ["3001", "3002", "3002"].iter().enumerate() {
        let s = 3.0 + i as f32 * 2.0;
        start(&mut t, s, Side::Boss, key);
        t.record(s + 1.0, TallyEvent::End { side: Side::Boss });
    }
    t.advance(20.0);
    assert_eq!(t.counts(Side::Boss, "3000").punished, 0);
    let s = t.summary();
    assert_eq!(s.boss.len(), 3);
    // Most used first, then by key.
    assert_eq!(
        serde_json::to_string(&s.boss).unwrap(),
        r#"{"3002":[2,0,0,0,2,0,0,0,0,0],"3000":[1,0,0,0,1,0,0,0,0,0],"3001":[1,0,0,0,1,0,0,0,0,0]}"#
    );
}

#[test]
fn tactics_and_back_steps_cannot_whiff_but_attacks_can() {
    let mut tally = Tally::default();
    for (i, key) in [
        "walk_in4",
        "guard1",
        "deflect",
        "dodge_back",
        "5201",
        "5211",
        "3013",
    ]
    .iter()
    .enumerate()
    {
        start(&mut tally, i as f32, Side::Boss, key);
    }
    let done = tally.flush(8.0);
    for x in done {
        let expected = u32::from(x.key == "3013");
        assert_eq!(u32::from(x.whiffed()), expected);
        assert_eq!(tally.counts(Side::Boss, &x.key).whiffed, expected);
    }
}

#[test]
fn exchanges_wait_out_the_punish_window() {
    let mut t = Tally::default();
    start(&mut t, 0.0, Side::Boss, "3005");
    t.record(1.0, TallyEvent::End { side: Side::Boss });
    assert!(t.advance(1.5).is_empty());
    hit(&mut t, 1.8, Side::Wolf, false, false);
    let done = t.advance(2.0);
    assert_eq!(done.len(), 1);
    assert!(done[0].outcome.punished);
}

#[test]
fn plans_become_acts() {
    let firecracker = OWL.act_for(&[3010], 0.7).unwrap();
    assert_eq!(firecracker.goals[0].approach, Some(f64::from(3.5f32 - 0.7)));
    assert_eq!(
        firecracker.goals[0].number,
        Some(NumberOnActivate::Set(5, 0.0))
    );
    let act = OWL.act_for(&[3000, 3001, 3002], 0.7).unwrap();
    assert_eq!(act.goals[0].approach, Some(f64::from(4.8f32 - 0.7)));
    let goals: Vec<_> = act.goals.iter().map(|g| (g.goal, g.params[0])).collect();
    assert_eq!(
        goals,
        [
            ("GOAL_COMMON_ComboAttackTunableSpin", 3000.0),
            ("GOAL_COMMON_ComboRepeat", 3001.0),
            ("GOAL_COMMON_ComboFinal", 3002.0),
        ]
    );
    assert_eq!(act.goals[1].number, Some(NumberOnActivate::Add(5, 7.0)));
    let single = OWL.act_for(&[3031], 0.7).unwrap();
    assert_eq!(single.goals[0].goal, "GOAL_COMMON_AttackTunableSpin");
    let step = OWL.act_for(&[5201, 3005], 0.7).unwrap();
    assert_eq!(step.goals[0].approach, None);
    assert_eq!(step.goals[0].goal, "GOAL_COMMON_SpinStep");
    assert_eq!(step.goals[0].number, Some(NumberOnActivate::Set(5, 0.0)));
    assert_eq!(OWL.act_for(&[], 0.7), Err(PlanError::Empty));
    assert_eq!(
        OWL.act_for(&[3000, 3001, 3002, 3063], 0.7),
        Err(PlanError::TooLong(4))
    );
    assert_eq!(OWL.act_for(&[3037], 0.7), Err(PlanError::Unknown(3037)));
}

fn request(id: u64, t: f32, tally: TallySummary) -> BrainRequest {
    BrainRequest {
        id,
        obs: Observation {
            t,
            ..Observation::default()
        },
        tally,
        memory: None,
        ..BrainRequest::default()
    }
}

#[test]
fn scripted_brain_switches_and_waits() {
    let mut b = ScriptedBrain {
        delay: 0.5,
        ..ScriptedBrain::default()
    };
    b.submit(&request(1, 2.0, TallySummary::default()));
    assert_eq!(b.poll(2.4), None);
    let r = b.poll(2.5).unwrap();
    assert_eq!((r.id, r.plan), (1, Some(Plan::new(&[3005]))));
    assert_eq!(b.poll(3.0), None, "one answer per request");
    let mut tally = TallySummary::default();
    tally.boss.insert(
        "3005",
        Counts {
            n: 3,
            deflected: 2,
            ..Counts::default()
        },
    );
    b.submit(&request(2, 4.0, tally));
    assert_eq!(b.poll(4.5).unwrap().plan, Some(Plan::new(&[3031])));
    assert!(!LuaBrain.plans());
    assert_eq!(BrainKind::by_name("Scripted"), Ok(BrainKind::Scripted));
    assert!(BrainKind::by_name("gpt").is_err());
}

/// The largest observation: long state names, every number at its widest.
fn widest_observation() -> Observation {
    let side = |act| FighterView {
        hp: 0.33,
        posture: 0.67,
        lives: 2,
        state: "GroundAttackCombo5ReleaseFromStep".into(),
        act,
        attacking: true,
        guarding: true,
        airborne: true,
        staggered: true,
        deathblow_open: true,
    };
    Observation {
        t: 1234.56,
        boss: side(Some(3063)),
        wolf: side(None),
        dist: 12.34,
        dy: -1.25,
        wolf_bearing: -179.0,
        boss_bearing: 135.0,
        locked_on: true,
        since_boss_hit: Some(12.34),
        since_wolf_hit: Some(0.5),
        plan: "walk_in2.5>3000+3001+3002".into(),
    }
}

#[test]
fn observation_json_stays_small() {
    let json = serde_json::to_string(&widest_observation()).unwrap();
    assert!(json.len() < 1024, "{} bytes: {json}", json.len());
    assert!(json.contains("\"dist\":12.34"), "{json}");
}

#[test]
fn soldier_plans_become_his_acts() {
    assert_eq!(BossActions::for_chr("c1010").map(|a| a.chr), Some("c1010"));
    assert_eq!(BossActions::for_chr("c5060").map(|a| a.chr), Some("c5060"));
    assert!(BossActions::for_chr("c9999").is_none());
    // Act01: approach to 2.5 m less his radius, then the combo; no AI number.
    let act = SOLDIER.act_for(&[3000, 3001], 0.5).unwrap();
    assert_eq!(act.goals[0].approach, Some(2.0));
    let goals: Vec<_> = act.goals.iter().map(|g| (g.goal, g.params[0])).collect();
    assert_eq!(
        goals,
        [
            ("GOAL_COMMON_ComboAttackTunableSpin", 3000.0),
            ("GOAL_COMMON_ComboFinal", 3001.0),
        ]
    );
    assert!(act.goals.iter().all(|g| g.number.is_none()));
    // Act09's rush sets AI number 5 to 1; the stone throw (Act07) never approaches.
    let rush = SOLDIER.act_for(&[3011], 0.5).unwrap();
    assert_eq!(rush.goals[0].number, Some(NumberOnActivate::Set(5, 1.0)));
    assert_eq!(
        SOLDIER.act_for(&[3009], 0.5).unwrap().goals[0].approach,
        None
    );
    let step = SOLDIER.act_for(&[5211], 0.5).unwrap();
    assert_eq!(step.goals[0].goal, "GOAL_COMMON_SpinStep");
    assert_eq!(step.goals[0].number, None);
    // Ids his script names but his TAE has no clip for, and the Owl's, are refused.
    for id in [3004, 3006, 5202, 3031] {
        assert_eq!(SOLDIER.act_for(&[id], 0.5), Err(PlanError::Unknown(id)));
    }
    // Every combo is a valid plan of its own table.
    for table in [&SOLDIER, &OWL] {
        for c in table.combos {
            assert!(table.act_for(c.ids, 0.5).is_ok(), "{}", c.value());
        }
    }
    assert_eq!(SOLDIER.combos[1].value(), "3005+3007");
}

#[test]
fn scripted_brain_plays_each_bosses_own_plans() {
    let mut b = ScriptedBrain::for_actions(&SOLDIER);
    b.submit(&request(1, 0.0, TallySummary::default()));
    assert_eq!(b.poll(1.0).unwrap().plan, Some(Plan::new(&[3000, 3001])));
    let mut tally = TallySummary::default();
    tally.boss.insert(
        "3000",
        Counts {
            n: 2,
            deflected: 2,
            ..Counts::default()
        },
    );
    b.submit(&request(2, 1.0, tally));
    assert_eq!(b.poll(2.0).unwrap().plan, Some(Plan::new(&[3005, 3007])));
    // The CLI kinds pick the table by chr.
    let mut built = BrainKind::Scripted.build("c1010");
    built.submit(&request(3, 0.0, TallySummary::default()));
    assert_eq!(
        built.poll(1.0).unwrap().plan,
        Some(Plan::new(&[3000, 3001]))
    );
    let mut owl = BrainKind::Scripted.build("c5060");
    owl.submit(&request(4, 0.0, TallySummary::default()));
    assert_eq!(owl.poll(1.0).unwrap().plan, Some(Plan::new(&[3005])));
}

#[test]
fn wolf_keys_are_short() {
    assert_eq!(
        driver::wolf_key("GroundAttackCombo1Release"),
        "AttackCombo1"
    );
    assert_eq!(
        driver::wolf_key("GroundAttackCombo5ReleaseFromStep"),
        "AttackCombo5FromStep"
    );
    assert_eq!(driver::wolf_key("SprintAttack"), "SprintAttack");
}

const DT: f32 = 1.0 / 60.0;

/// Feeds a tracker a scripted Wolf: `frames` of (state, dist, guarding), with `events` at
/// given frame indices.
fn run_wolf(
    tracker: &mut HabitTracker,
    t0: f32,
    frames: &[(&str, f32, bool)],
    events: &[(usize, TallyEvent)],
) -> f32 {
    let mut t = t0;
    for (i, &(state, dist, guarding)) in frames.iter().enumerate() {
        t += DT;
        let now: Vec<TallyEvent> = events
            .iter()
            .filter(|(k, _)| *k == i)
            .map(|(_, e)| e.clone())
            .collect();
        let frame = WolfFrame {
            t,
            dt: DT,
            dist,
            state,
            airborne: false,
            guarding,
            hp: 1.0,
        };
        tracker.step(&frame, &now);
    }
    t
}

fn boss_start(key: &str) -> TallyEvent {
    TallyEvent::Start {
        side: Side::Boss,
        key: key.into(),
    }
}

fn wolf_attack() -> TallyEvent {
    TallyEvent::Start {
        side: Side::Wolf,
        key: "AttackCombo1".into(),
    }
}

#[test]
fn habits_from_a_scripted_wolf() {
    let mut tr = HabitTracker::new(SOLDIER.back_steps);
    let mut t = 0.0;
    // Ten rounds: the soldier steps back (5201, 0.6 s), Wolf follows and attacks from 3 m
    // 0.4 s after the step ends; then the soldier slashes (3000), Wolf deflects it and holds
    // his guard.
    for _ in 0..10 {
        let mut frames = Vec::new();
        for i in 0..60 {
            frames.push(("StandMove", 2.0 + i as f32 / 60.0, false));
        }
        for _ in 0..24 {
            frames.push(("StandMove", 3.0, false));
        }
        for _ in 0..40 {
            frames.push(("GroundAttackCombo1", 3.0, false));
        }
        for _ in 0..90 {
            frames.push(("DeflectGuardIdle", 2.0, true));
        }
        let events = [
            (0, boss_start("5201")),
            (36, TallyEvent::End { side: Side::Boss }),
            (60, wolf_attack()),
            (100, TallyEvent::End { side: Side::Wolf }),
            (124, boss_start("3000")),
            (
                140,
                TallyEvent::Hit {
                    attacker: Side::Boss,
                    guarded: true,
                    deflected: true,
                },
            ),
            (170, TallyEvent::End { side: Side::Boss }),
        ];
        t = run_wolf(&mut tr, t, &frames, &events);
    }
    // A dodge and a jump answer two of the soldier's kicks.
    let frames: Vec<_> = (0..120)
        .map(|i| match i {
            10..30 => ("GroundStep", 3.0, false),
            70..90 => ("GroundJumpStart", 3.0, false),
            _ => ("StandIdle", 3.0, false),
        })
        .collect();
    let events = [
        (0, boss_start("3005")),
        (50, TallyEvent::End { side: Side::Boss }),
        (60, boss_start("3005")),
        (110, TallyEvent::End { side: Side::Boss }),
    ];
    t = run_wolf(&mut tr, t, &frames, &events);
    tr.flush(t);
    let h = &tr.habits;
    let back = &h.after["back_step"];
    assert_eq!((back.n, back.to["attack"]), (10, 10), "{back:?}");
    assert!((back.attack_dist.mean().unwrap() - 3.0).abs() < 0.01);
    let deflect = &h.after["deflect"];
    assert_eq!((deflect.n, deflect.to["guard"]), (10, 10), "{deflect:?}");
    assert_eq!(h.answers["3000"].to["deflect"], 10);
    assert_eq!(h.answers["5201"].to["none"], 10);
    assert_eq!(h.answers["3005"].to["dodge"], 1);
    assert_eq!(h.answers["3005"].to["jump"], 1);
    assert_eq!(h.jumps, 1);
    // Wolf attacks 0.4 s after each back step ends.
    let p = h.punish["5201"];
    assert_eq!(p.n, 10);
    assert!((p.mean().unwrap() - 0.4).abs() < 0.02, "{p:?}");
    let s = h.summary();
    let json = serde_json::to_string(&s).unwrap();
    eprintln!("{json}");
    assert_eq!(s.after["back_step"], "attack 100% from 3.0m n10");
    assert_eq!(s.after["deflect"], "guard 100% n10");
    assert_eq!(s.vs["3000"], "deflect 100% n10");
    assert_eq!(s.attack_m, Some(3.0));
    assert_eq!(s.punish_s["5201"], 0.4);
    assert_eq!(s.vs["3005"], "dodge 50%, jump 50% n2");
}

#[test]
fn heals_and_low_hp_spells() {
    let mut tr = HabitTracker::new(&[5201]);
    let mut t = 0.0;
    for (state, hp) in [
        ("StandIdle", 0.9),
        ("StandIdle", 0.4),
        ("ItemDrink", 0.4),
        ("StandIdle", 0.8),
        ("StandIdle", 0.3),
        ("StandIdle", 0.7),
    ] {
        for _ in 0..30 {
            t += DT;
            tr.step(
                &WolfFrame {
                    t,
                    dt: DT,
                    dist: 4.0,
                    state,
                    airborne: false,
                    guarding: false,
                    hp,
                },
                &[],
            );
        }
    }
    let heals = tr.habits.heals;
    assert_eq!((heals.hp.n, heals.at_low, heals.low_spells), (1, 1, 2));
    assert_eq!(
        tr.habits.summary().heal.unwrap(),
        "1 heals, mean hp 0.40; healed in 1 of 2 low-hp spells"
    );
}

fn temp_file(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "sekiro-brain-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

#[test]
fn real_owl_datasets_migrate_without_losing_totals() {
    for session in ["owl-2026-10-11-outsmarted", "owl-2026-10-11-alive"] {
        let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!("../../logs/datasets/{session}/c5060.json"));
        // Local research data is optional on other machines, never embedded in source.
        if !source.exists() {
            continue;
        }
        let legacy = temp_file("c5060.json");
        std::fs::copy(&source, &legacy).unwrap();
        let original = std::fs::read(&legacy).unwrap();
        let mut old: Memory = serde_json::from_slice(&original).unwrap();
        // Raw evidence stays exact; recent attack masses are newly derived and tested separately.
        old.version = memory::MEMORY_VERSION;
        let paths = memory::LayerPaths::new(&legacy, "default");
        let (boss, profile) = paths.load("c5060").unwrap();
        assert!(!profile.attack_learning.is_empty());
        old.attack_learning = profile.attack_learning.clone();
        assert_eq!(profile, old);
        assert_eq!(boss.evidence.fights, old.fights);
        assert_eq!(boss.evidence.boss_wins, old.boss_wins);
        assert_eq!(boss.evidence.wolf_wins, old.wolf_wins);
        assert_eq!(boss.evidence.actions, old.boss);
        assert_eq!(boss.evidence.plans, old.coach.plans);
        assert_eq!(boss.evidence.break_plans, old.coach.break_plans);
        assert_eq!(std::fs::read(&paths.backup).unwrap(), original);
        assert_eq!(std::fs::read(&legacy).unwrap(), original);
        assert!(paths.boss.exists() && paths.player.exists());
        let (again, _) = paths.load("c5060").unwrap();
        assert_eq!(boss, again);
        eprintln!(
            "migration {session}: fights={} boss_wins={} wolf_wins={} action_exchanges={} plan_exchanges={} players={}",
            boss.evidence.fights,
            boss.evidence.boss_wins,
            boss.evidence.wolf_wins,
            boss.evidence.actions.values().map(|c| c.n).sum::<u32>(),
            boss.evidence.plans.values().map(|c| c.n).sum::<u32>(),
            boss.summary().players
        );
    }
}

#[test]
fn both_full_layers_fit_coach_budget() {
    let mut input = busy_input();
    input.boss_knowledge = memory::BossSummary {
        best: Some(memory::BestSummary {
            sets: input
                .played
                .as_ref()
                .unwrap()
                .sets
                .iter()
                .map(|s| (s.name.clone(), s.plans.clone()))
                .collect(),
            opener: "3000".into(),
            start_set: "set0".into(),
            ..Default::default()
        }),
        fights: 100,
        players: 8,
        actions: input.overall.clone(),
        plans: input.overall.clone(),
        break_plans: input.overall.clone(),
        principles: vec!["界".repeat(90); 6],
        bans: (0..8)
            .map(|i| coach::Avoid {
                choice: format!("300{i}"),
                reason: "界".repeat(80),
            })
            .collect(),
    };
    input.principles = vec!["界".repeat(90); 6];
    input.played.as_mut().unwrap().boss_principles = input.boss_knowledge.principles.clone();
    input.played.as_mut().unwrap().boss_bans = input.boss_knowledge.bans.clone();
    input.phase = "mid".into();
    assert!(input.fit(INPUT_BUDGET) <= INPUT_BUDGET);
    let value = serde_json::to_value(&input).unwrap();
    assert_eq!(value["boss_knowledge"]["fights"], 100);
    assert!(value["player_profile"].is_object());
    assert!(value["boss_knowledge"].get("habits").is_none());
}

/// A fight's tally and habits: the soldier's 3000 deflected once, Wolf's slash landing.
fn one_fight() -> (Tally, HabitTracker) {
    let mut t = Tally::default();
    let mut h = HabitTracker::new(SOLDIER.back_steps);
    let events = [
        (0, boss_start("3000")),
        (
            20,
            TallyEvent::Hit {
                attacker: Side::Boss,
                guarded: true,
                deflected: true,
            },
        ),
        (40, TallyEvent::End { side: Side::Boss }),
        (50, wolf_attack()),
        (
            60,
            TallyEvent::Hit {
                attacker: Side::Wolf,
                guarded: false,
                deflected: false,
            },
        ),
    ];
    let frames = vec![("StandIdle", 2.0, false); 120];
    for (i, e) in &events {
        t.record(*i as f32 * DT, e.clone());
    }
    let end = run_wolf(&mut h, 0.0, &frames, &events);
    t.flush(end);
    h.flush(end);
    (t, h)
}

#[test]
fn memory_round_trips_and_merges_fights() {
    let path = temp_file("c1010.json");
    let (tally, habits) = one_fight();
    let mut m = Memory::new("c1010");
    m.add_fight(&tally, &habits.habits, FightEnd::BossWon);
    m.save(&path).unwrap();
    let (mut back, how) = Memory::load(&path, "c1010");
    assert_eq!(how, Loaded::Read);
    assert_eq!(back, m);
    back.add_fight(&tally, &habits.habits, FightEnd::WolfWon);
    back.add_fight(&tally, &habits.habits, FightEnd::Unfinished);
    back.save(&path).unwrap();
    let (again, _) = Memory::load(&path, "c1010");
    assert_eq!((again.fights, again.boss_wins, again.wolf_wins), (3, 1, 1));
    assert_eq!(again.boss["3000"].n, 3);
    assert_eq!(again.boss["3000"].deflected, 3);
    assert_eq!(again.wolf["AttackCombo1"].hit, 3);
    assert_eq!(again.habits.answers["3000"].to["deflect"], 3);
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(
        text.contains(&format!("\"version\": {}", memory::MEMORY_VERSION)),
        "{text}"
    );
    assert!(!path.with_extension("json.tmp").exists());
    let _ = std::fs::remove_file(&path);
}

#[test]
fn corrupt_or_foreign_memory_starts_fresh_and_is_kept_aside() {
    for (text, want) in [
        ("{ not json", "bad JSON"),
        (r#"{"version":99,"chr":"c1010"}"#, "format version 99"),
        (r#"{"version":1,"chr":"c5060"}"#, "memory of c5060"),
    ] {
        let path = temp_file("c1010.json");
        std::fs::write(&path, text).unwrap();
        let (m, how) = Memory::load(&path, "c1010");
        assert_eq!(m, Memory::new("c1010"));
        match how {
            Loaded::Corrupt(why) => assert!(why.contains(want), "{why}"),
            other => panic!("{other:?}"),
        }
        let mut aside = path.as_os_str().to_owned();
        aside.push(".corrupt");
        assert_eq!(std::fs::read_to_string(&aside).unwrap(), text);
        assert!(!path.exists());
        let _ = std::fs::remove_file(aside);
    }
    let missing = temp_file("none.json");
    assert_eq!(Memory::load(&missing, "c1010").1, Loaded::Missing);
}

#[test]
fn memory_paths_from_the_flag() {
    use std::path::PathBuf;
    let def = PathBuf::from("logs/memory/c1010.json");
    assert_eq!(memory::resolve(None, "c1010", true), Some(def.clone()));
    assert_eq!(memory::resolve(None, "c1010", false), None);
    assert_eq!(memory::resolve(Some("off"), "c1010", true), None);
    assert_eq!(memory::resolve(Some("on"), "c1010", false), Some(def));
    assert_eq!(
        memory::resolve(Some("m/me.json"), "c1010", true),
        Some(PathBuf::from("m/me.json"))
    );
    assert_eq!(
        memory::resolve(Some("m"), "c1010", true),
        Some(PathBuf::from("m").join("c1010.json"))
    );
}

/// A memory with every map full: many fights of every soldier action, many Wolf attacks and
/// every habit.
fn full_memory() -> Memory {
    let mut m = Memory::new("c1010");
    m.fights = 123;
    m.boss_wins = 45;
    m.wolf_wins = 67;
    let counts = |n| Counts {
        n,
        hit: n / 3,
        blocked: n / 4,
        deflected: n / 5,
        whiffed: n / 6,
        punished: n / 7,
        countered: 0,
        counter_kinds: [0; 3],
    };
    for (i, a) in SOLDIER.actions.iter().enumerate() {
        m.boss.insert(a.id.to_string(), counts(100 + i as u32));
        let r = m.habits.answers.entry(a.id.to_string()).or_default();
        r.n = 100;
        r.to.insert("deflect".into(), 61);
        r.to.insert("dodge".into(), 39);
        m.habits
            .punish
            .insert(a.id.to_string(), habits::Mean { sum: 41.7, n: 100 });
    }
    for w in [
        "AttackCombo1",
        "AttackCombo2",
        "AttackCombo3",
        "AttackCombo5FromStep",
        "SprintAttack",
        "JumpAttack",
    ] {
        m.wolf.insert(w.into(), counts(150));
    }
    for trig in ["deflect", "guard", "guarded", "back_step"] {
        let r = m.habits.after.entry(trig.into()).or_default();
        r.n = 100;
        r.to.insert("attack".into(), 77);
        r.to.insert("wait".into(), 23);
        r.attack_dist = habits::Mean { sum: 312.0, n: 77 };
    }
    m.habits.seconds = 3600.0;
    m.habits.range_seconds = [1200.0, 1800.0, 600.0];
    m.habits.attack_dist = habits::Mean {
        sum: 2900.0,
        n: 1000,
    };
    m.habits.jumps = 90;
    m.habits.heals.hp = habits::Mean { sum: 12.3, n: 40 };
    m.habits.heals.at_low = 30;
    m.habits.heals.low_spells = 41;
    m
}

#[test]
fn requests_fit_the_budget() {
    let mut tally = Tally::default();
    for (i, a) in SOLDIER.actions.iter().enumerate() {
        for k in 0..=i {
            tally.record(k as f32, boss_start(&a.id.to_string()));
            tally.record(k as f32 + 0.5, TallyEvent::End { side: Side::Boss });
        }
    }
    for w in ["AttackCombo1", "AttackCombo2", "SprintAttack", "JumpAttack"] {
        tally.record(
            0.0,
            TallyEvent::Start {
                side: Side::Wolf,
                key: w.into(),
            },
        );
    }
    tally.flush(100.0);
    let mut req = BrainRequest {
        id: 123456,
        obs: widest_observation(),
        tally: tally.summary(),
        memory: Some(full_memory().summary()),
        ..BrainRequest::default()
    };
    let untrimmed = req.json_len();
    let len = req.fit(REQUEST_BUDGET);
    let json = serde_json::to_string(&req).unwrap();
    eprintln!("untrimmed {untrimmed} bytes, fitted {len}: {json}");
    assert!(untrimmed > REQUEST_BUDGET, "the test should need trimming");
    assert!(len <= REQUEST_BUDGET, "{len}");
    assert_eq!(len, json.len());
    // The most telling entries survive: this fight's busiest boss action, the habits'
    // responses and single values, the fight count.
    assert!(json.contains("\"5211\":[15,"), "{json}");
    let m = req.memory.as_ref().unwrap();
    assert!(!m.habits.after.is_empty(), "{json}");
    assert!(m.habits.attack_m.is_some() && m.habits.range_pct.is_some());
    assert_eq!(m.fights, 123);
    // A typical request (one earlier fight, this fight's few actions) needs no trimming.
    let (t1, h1) = one_fight();
    let mut small = Memory::new("c1010");
    small.add_fight(&t1, &h1.habits, FightEnd::BossWon);
    let mut typical = BrainRequest {
        id: 12,
        obs: widest_observation(),
        tally: t1.summary(),
        memory: Some(small.summary()),
        ..BrainRequest::default()
    };
    let before = typical.json_len();
    eprintln!(
        "typical {before} bytes: {}",
        serde_json::to_string(&typical).unwrap()
    );
    assert_eq!(typical.fit(REQUEST_BUDGET), before);
    assert!(before < 900, "{before}");
}

// ---- tactics, plans, sampling, metrics ----

#[test]
fn plan_text_forms_round_trip() {
    for table in [&OWL, &SOLDIER] {
        for c in table.choices() {
            let plan = table.parse(&c.value).unwrap();
            assert_eq!(plan.value(), c.value);
            assert!(plan.len() <= table.max_plan);
        }
    }
    let p = SOLDIER.parse("walk_in4>hold2>3013").unwrap();
    assert_eq!(
        p.steps,
        [
            Step::Tactic(Tactic::WalkIn(4.0)),
            Step::Tactic(Tactic::Hold(2.0)),
            Step::Act(3013)
        ]
    );
    // Consecutive attacks are one chain in the text form.
    assert_eq!(SOLDIER.parse("3000>3001").unwrap().value(), "3000+3001");
    assert_eq!(
        SOLDIER.parse("walk_in4>hold2>3013>3000"),
        Err(PlanError::TooLong(4))
    );
    assert_eq!(
        SOLDIER.parse("fly3"),
        Err(PlanError::UnknownToken("fly3".into()))
    );
}

#[test]
fn tactics_run_through_the_engines_own_goals() {
    let expect = [
        ("hold2", "GOAL_COMMON_Wait", 2.0),
        ("guard2.5", "GOAL_COMMON_Guard", 2.5),
        ("walk_in4", "GOAL_COMMON_MoveToSomewhere", 5.0),
        ("run_in3", "GOAL_COMMON_MoveToSomewhere", 4.0),
        ("strafe_left", "GOAL_COMMON_SidewayMove", 2.0),
        ("back_off9", "GOAL_COMMON_LeaveTarget", 4.0),
        ("retreat12", "GOAL_COMMON_LeaveTarget", 5.0),
        ("keep_range", "GOAL_COMMON_KeepDist", 2.5),
    ];
    let act = |text: &str| {
        SOLDIER
            .act_for_plan(&SOLDIER.parse(text).unwrap(), 0.5)
            .unwrap()
    };
    for (token, goal, life) in expect {
        let act = act(token);
        let g = &act.goals[0];
        assert_eq!((g.goal, g.life), (goal, life), "{token}");
        assert!(
            g.timed && g.approach.is_none() && g.number.is_none(),
            "{token}"
        );
        assert_eq!(g.step, Some(0));
    }
    // Distance bands are the goal's distance parameter; running is walk = false.
    assert_eq!(act("walk_in2.5").goals[0].params, [0.0, 0.0, 2.5, 0.0, 1.0]);
    assert_eq!(act("run_in3").goals[0].params, [0.0, 0.0, 3.0, 0.0, 0.0]);
    let retreat = act("retreat12");
    assert!(retreat.goals[0].away);
    assert_eq!(retreat.goals[0].params, [0.0, 12.0, 0.0, 0.0]);
    // A mixed plan: the tactic, then the attack with its own approach (a tactic can leave him
    // far away); steps count from 0 across both.
    let shape: Vec<_> = act("back_off6>3000+3001")
        .goals
        .iter()
        .map(|g| (g.goal, g.step, g.approach.is_some()))
        .collect();
    assert_eq!(
        shape,
        [
            ("GOAL_COMMON_LeaveTarget", Some(0), false),
            ("GOAL_COMMON_ComboAttackTunableSpin", Some(1), true),
            ("GOAL_COMMON_ComboFinal", Some(2), false),
        ]
    );
    // An attack then a tactic: the tactic comes last.
    let after = act("3008>back_off6");
    assert_eq!(after.goals[0].goal, "GOAL_COMMON_AttackTunableSpin");
    assert_eq!(after.goals[1].goal, "GOAL_COMMON_LeaveTarget");
}

use sampler::{Context, EXCLUDE_STREAK, Pick, Rng, Streaks};
use std::collections::HashSet;

fn set_of(plans: &[(&str, f32)]) -> Vec<Weighted> {
    plans
        .iter()
        .map(|(v, w)| Weighted {
            plan: (*v != "lua").then(|| SOLDIER.parse(v).unwrap()),
            weight: *w,
        })
        .collect()
}

fn counts(picks: &[Pick], n: usize) -> Vec<usize> {
    (0..n)
        .map(|i| picks.iter().filter(|p| **p == Pick::Plan(i)).count())
        .collect()
}

fn sample(
    set: &[Weighted],
    streaks: &Streaks,
    tried: &HashSet<String>,
    temperature: f32,
    seed: u64,
    n: usize,
) -> Vec<Pick> {
    let mut rng = Rng::new(seed);
    let ctx = Context {
        streaks,
        tried,
        temperature,
    };
    (0..n).map(|_| sampler::pick(set, &ctx, &mut rng)).collect()
}

fn tried_all(values: &[&str]) -> HashSet<String> {
    values.iter().map(|s| s.to_string()).collect()
}

#[test]
fn sampling_follows_the_probabilities_and_is_seeded() {
    let set = set_of(&[("3000", 0.6), ("3005", 0.3), ("hold2", 0.1)]);
    // Everything already tried: no bonus, so the weights are the probabilities.
    let tried = tried_all(&["3000", "3005", "hold2"]);
    let none = Streaks::default();
    let picks = sample(&set, &none, &tried, 1.0, 7, 20_000);
    let c = counts(&picks, 3);
    for (c, want) in c.iter().zip([0.6, 0.3, 0.1]) {
        let got = *c as f32 / 20_000.0;
        assert!((got - want).abs() < 0.02, "{c:?}");
    }
    // The same seed gives the same sequence; another seed another.
    assert_eq!(sample(&set, &none, &tried, 1.0, 7, 200)[..], picks[..200]);
    assert_ne!(sample(&set, &none, &tried, 1.0, 8, 200)[..], picks[..200]);
    // Temperature: low sharpens, zero is greedy, high flattens.
    let sharp = counts(&sample(&set, &none, &tried, 0.25, 7, 5000), 3);
    assert!(sharp[0] > 4500, "{sharp:?}");
    let greedy = sample(&set, &none, &tried, 0.0, 7, 50);
    assert!(greedy.iter().all(|p| *p == Pick::Plan(0)));
    let flat = counts(&sample(&set, &none, &tried, 8.0, 7, 9000), 3);
    assert!(flat.iter().all(|c| *c > 2400), "{flat:?}");
}

#[test]
fn repeated_deflects_push_an_option_down_and_untried_ones_get_chosen() {
    let set = set_of(&[("3003", 0.5), ("3000", 0.25), ("hold1", 0.25)]);
    let tried = tried_all(&["3003", "3000", "hold1"]);
    let share = |streaks: &Streaks| {
        let c = counts(&sample(&set, streaks, &tried, 1.0, 3, 10_000), 3);
        c[0] as f32 / 10_000.0
    };
    let mut s = Streaks::default();
    let fresh = share(&s);
    s.record("3003", true);
    let once = share(&s);
    s.record("3003", true);
    s.record("3003", true);
    let thrice = share(&s);
    assert!((fresh - 0.5).abs() < 0.03, "{fresh}");
    assert!(
        once < fresh - 0.1 && thrice < once - 0.1,
        "{fresh} {once} {thrice}"
    );
    // A landed hit (or any other outcome) resets it.
    s.record("3003", false);
    assert!((share(&s) - fresh).abs() < 0.03);
    // Untried options carry a bonus and, over enough acts, all get run: a boss that has only
    // tried 3003 so far picks 3000 and hold1 more often than their probability says.
    let only = tried_all(&["3003"]);
    let none = Streaks::default();
    let c = counts(&sample(&set, &none, &only, 1.0, 5, 10_000), 3);
    assert!(c[1] > 2500 && c[2] > 2500, "{c:?}");
    let mut seen = [false; 3];
    for p in sample(&set, &none, &only, 1.0, 11, 60) {
        if let Pick::Plan(i) = p {
            seen[i] = true;
        }
    }
    assert!(seen.iter().all(|s| *s), "{seen:?}");
    // A set of one (no probabilities) is not sampled, and gives way to the Lua AI once
    // repelled `EXCLUDE_STREAK` times in a row.
    let one = set_of(&[("3003", 1.0)]);
    let mut s = Streaks::default();
    for _ in 0..EXCLUDE_STREAK - 1 {
        s.record("3003", true);
    }
    assert_eq!(sample(&one, &s, &tried, 1.0, 1, 1), [Pick::Plan(0)]);
    s.record("3003", true);
    assert_eq!(sample(&one, &s, &tried, 1.0, 1, 1), [Pick::Lua]);
    assert_eq!(sample(&[], &s, &tried, 1.0, 1, 1), [Pick::Nothing]);
}

#[test]
fn a_plan_is_as_worn_as_its_most_repelled_step() {
    // Streak keys: action ids for attacks, tokens for tactics.
    let mut s = Streaks::default();
    s.record("walk_in4", true);
    s.record("3013", true);
    let plan = SOLDIER.parse("walk_in4>3013").unwrap();
    assert_eq!(s.worst(&plan), 1);
    s.record("3013", true);
    assert_eq!(s.worst(&plan), 2);
}

#[test]
fn metrics_from_a_scripted_fight() {
    use metrics::MetricsTracker;
    let mut m = MetricsTracker::new(10.0);
    // The boss's hits: two deflected, one guarded, one landed.
    m.boss_hit(true, true);
    m.boss_hit(true, true);
    m.boss_hit(true, false);
    m.boss_hit(false, false);
    // Wolf's HP: 1.0 -> 0.9 -> 0.95 (a heal) -> 0.7; posture 0 -> 0.2 -> 0.1 -> 0.4.
    for (hp, posture) in [(1.0, 0.0), (0.9, 0.2), (0.95, 0.1), (0.7, 0.4)] {
        m.wolf(hp, posture);
    }
    let lost = m.finish(25.0, 3, true);
    assert_eq!(
        (lost.deflected, lost.guarded, lost.hits_taken, lost.punishes),
        (2, 1, 1, 3)
    );
    assert_eq!(lost.deflect_rate(), Some(0.5));
    assert_eq!(lost.guard_rate(), Some(0.25));
    assert!((lost.hp_lost - 0.35).abs() < 1e-5, "{}", lost.hp_lost);
    assert!(
        (lost.posture_gained - 0.5).abs() < 1e-5,
        "{}",
        lost.posture_gained
    );
    assert_eq!((lost.duration, lost.kill_time), (15.0, Some(15.0)));
    // The boss wins: no kill time.
    assert_eq!(m.finish(25.0, 0, false).kill_time, None);
    // No resolved hits: no rate.
    assert_eq!(FightMetrics::default().deflect_rate(), None);
}

fn record(defl: u32, hits: u32, kill: Option<f32>) -> FightRecord {
    FightRecord {
        winner: "wolf".into(),
        metrics: FightMetrics {
            deflected: defl,
            hits_taken: hits,
            kill_time: kill,
            ..FightMetrics::default()
        },
        ..FightRecord::default()
    }
}

#[test]
fn trend_shows_the_last_three_fights() {
    let history = [
        record(9, 1, Some(40.0)),
        record(5, 5, Some(30.0)),
        record(10, 0, Some(20.0)),
        record(8, 2, None),
    ];
    let now = FightMetrics {
        deflected: 1,
        hits_taken: 1,
        ..FightMetrics::default()
    };
    let t = Trend::new(&history, Some(&now));
    assert_eq!(t.defl, [50, 100, 80]);
    assert_eq!(t.hits, [5, 0, 2]);
    assert_eq!(t.kill_s, [Some(30), Some(20), None]);
    assert_eq!(t.now, Some([50, 1]));
    assert_eq!(
        serde_json::to_string(&t).unwrap(),
        r#"{"boss_damage_taken_per_min":[0.0,0.0,0.0],"duration":[0.0,0.0,0.0],"wolf_posture_breaks":[0,0,0],"hits_after_break":[0,0,0],"break_punish_rate":[0.0,0.0,0.0],"break_conversion_delay":[null,null,null],"wolf_posture_peak":[0.0,0.0,0.0],"time_to_kill_wolf":[null,null,null],"defl":[50,100,80],"guard":[0,0,0],"hits":[5,0,2],"hp":[0,0,0],"kill_s":[30,20,null],"apm":[0,0,0],"dpm":[0,0,0],"passive":[0,0,0],"engaged_time":[0,0,0],"now":[50,1]}"#
    );
}

#[test]
fn memory_v1_migrates_and_keeps_ten_fights() {
    let path = temp_file("c1010.json");
    std::fs::write(
        &path,
        r#"{"version":1,"chr":"c1010","fights":2,"boss_wins":1,"wolf_wins":1,
            "boss":{"3000":[5,1,1,1,2,0]},"wolf":{},"habits":{}}"#,
    )
    .unwrap();
    let (mut m, how) = Memory::load(&path, "c1010");
    assert_eq!(how, Loaded::Read);
    assert_eq!(
        (m.version, m.fights, m.history.len()),
        (memory::MEMORY_VERSION, 2, 0)
    );
    assert_eq!(m.boss["3000"].n, 5);
    assert!(
        m.punishes.is_empty(),
        "legacy hits are not evidence of punish success"
    );
    for i in 0..13 {
        m.push_record(record(i, 0, None));
    }
    assert_eq!(m.history.len(), metrics::HISTORY);
    assert_eq!(m.history[0].metrics.deflected, 3);
    m.save(&path).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(
        text.contains(&format!("\"version\": {}", memory::MEMORY_VERSION)),
        "{text}"
    );
    let (back, _) = Memory::load(&path, "c1010");
    assert_eq!(back, m);
    assert_eq!(back.summary().trend.defl.len(), 3);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn the_request_with_a_trend_and_tactics_stays_under_the_budget() {
    let mut memory = full_memory();
    for i in 0..10 {
        memory.push_record(record(3 + i, i, Some(30.0 + i as f32)));
    }
    let mut tally = Tally::default();
    for (i, a) in SOLDIER.actions.iter().enumerate() {
        for k in 0..=i {
            tally.record(k as f32, boss_start(&a.id.to_string()));
            tally.record(k as f32 + 0.5, TallyEvent::End { side: Side::Boss });
        }
    }
    for t in ["walk_in4", "hold2", "strafe_left", "retreat12", "back_off9"] {
        tally.record(0.0, boss_start(t));
        tally.record(0.5, TallyEvent::End { side: Side::Boss });
    }
    tally.flush(100.0);
    let now = FightMetrics::default();
    let mut req = BrainRequest {
        id: 123456,
        obs: widest_observation(),
        tally: tally.summary(),
        memory: Some(memory.summary_with(Some(&now))),
        why: Some("deflected_thrice"),
        ..BrainRequest::default()
    };
    let len = req.fit(REQUEST_BUDGET);
    let json = serde_json::to_string(&req).unwrap();
    eprintln!("{len} bytes: {json}");
    assert!(len <= REQUEST_BUDGET && len == json.len());
    assert!(
        serde_json::from_str::<serde_json::Value>(&json).unwrap()["memory"]["trend"]["defl"]
            .is_array(),
        "{json}"
    );
    assert!(json.contains("\"why\":\"deflected_thrice\""));
    // `excluded` is for the request body, never part of the input text.
    req.excluded = vec!["3003".into()];
    assert!(!serde_json::to_string(&req).unwrap().contains("excluded"));
}

#[test]
fn request_modes_have_names() {
    assert_eq!(
        RequestMode::by_name("Checkpoints"),
        Some(RequestMode::Checkpoints)
    );
    assert_eq!(RequestMode::by_name("plan_end"), Some(RequestMode::PlanEnd));
    assert_eq!(RequestMode::by_name("slot"), Some(RequestMode::Slot));
    assert_eq!(RequestMode::by_name("often"), None);
    assert_eq!(RequestMode::default(), RequestMode::Slot);
}

#[test]
fn memory_files_of_different_brains_do_not_mix() {
    use std::path::PathBuf;
    let p = |arg, brain, hosted| memory::resolve_for(arg, "c1010", brain, hosted);
    let dir = PathBuf::from("logs/memory");
    assert_eq!(
        p(Some("on"), "lua", false),
        Some(dir.join("c1010.lua.json"))
    );
    assert_eq!(
        p(Some("on"), "decisions", true),
        Some(dir.join("c1010.json"))
    );
    assert_eq!(p(None, "decisions", true), Some(dir.join("c1010.json")));
    assert_eq!(p(None, "lua", false), None);
    assert_eq!(
        p(Some("m"), "lua", false),
        Some(PathBuf::from("m").join("c1010.lua.json"))
    );
    assert_eq!(
        p(Some("m/x.json"), "lua", false),
        Some(PathBuf::from("m/x.json"))
    );
}

// ---- the coach's game plan, the stalling limits and the new metrics ----

use super::coach::{CoachInput, FightLine, GamePlan, INPUT_BUDGET, NOTES_MAX, PlanSet, SetStat};
use super::limits::{self, Pacing};
use super::tally::TopMap;

fn set(name: &str, plans: &[(&str, f32)]) -> PlanSet {
    PlanSet {
        name: name.into(),
        description: format!("the {name} set"),
        plans: plans
            .iter()
            .map(|(p, w)| coach::WeightedChoice {
                plan: (*p).into(),
                weight: *w,
            })
            .collect(),
        when: None,
    }
}

fn good_plan() -> GamePlan {
    GamePlan {
        punish_preferences: Default::default(),
        explore: Default::default(),
        move_controls: Vec::new(),
        lessons: Vec::new(),
        checkpoint_prefs: Default::default(),
        boss_phase: "c1010".into(),
        counter_weights: Default::default(),
        version: coach::GAME_PLAN_VERSION,
        opener: "hold3.5>walk_in4".into(),
        sets: vec![
            set("press", &[("3000+3001", 2.0), ("3008", 1.0)]),
            PlanSet {
                when: Some("half_hp".into()),
                ..set(
                    "bait",
                    &[("walk_in4>3013", 1.0), ("hold2>3002", 1.0), ("3000", 1.0)],
                )
            },
        ],
        start_set: "bait".into(),
        avoid: vec![coach::Avoid {
            choice: "3003".into(),
            reason: "always deflected".into(),
        }],
        notes: "he deflects thrusts early".into(),
        retry: Vec::new(),
        on_wolf_break: Vec::new(),
        principles: Vec::new(),
        boss_principles: Vec::new(),
        boss_bans: Vec::new(),
    }
}

#[test]
fn a_good_game_plan_is_accepted_whole() {
    let plan = good_plan();
    let v = plan.validate(&SOLDIER);
    assert!(v.warnings.is_empty(), "{:?}", v.warnings);
    assert_eq!(v.plan, plan);
    assert!(v.usable());
    // The opener is a plan with tactics, the start set resolves, and sets become weighted plans.
    assert_eq!(plan.opener_plan(&SOLDIER).unwrap().len(), 2);
    assert_eq!(plan.start_index(), Some(1));
    let weighted = plan.weighted(0, &SOLDIER);
    assert_eq!(weighted.len(), 2);
    assert_eq!(weighted[0].weight, 2.0);
    assert_eq!(
        plan.offers()[1].description,
        "the bait set (suits: half_hp)"
    );
    // The JSON form round-trips, and a missing version reads as the current one.
    let json = serde_json::to_string(&plan).unwrap();
    assert_eq!(GamePlan::from_json(&json).unwrap(), plan);
    let bare = GamePlan::from_json(r#"{"opener":"3000"}"#).unwrap();
    assert!(bare.validate(&SOLDIER).usable());
    assert_eq!(plan.summary_lines().len(), 4);
}

#[test]
fn coach_caps_lua_and_keeps_bounded_principles_and_break_plans() {
    let mut p = good_plan();
    p.sets[0].plans.extend([
        coach::WeightedChoice {
            plan: "lua".into(),
            weight: 100.0,
        },
        coach::WeightedChoice {
            plan: "lua".into(),
            weight: 50.0,
        },
    ]);
    p.principles = vec!["界".repeat(120); 8];
    p.on_wolf_break = vec![
        coach::WeightedChoice {
            plan: "3000".into(),
            weight: 2.0,
        },
        coach::WeightedChoice {
            plan: "hold2".into(),
            weight: 1.0,
        },
        coach::WeightedChoice {
            plan: "9999".into(),
            weight: 1.0,
        },
    ];
    let v = p.validate(&SOLDIER);
    let set = &v.plan.sets[0];
    let lua: f32 = set
        .plans
        .iter()
        .filter(|p| p.plan == "lua")
        .map(|p| p.weight)
        .sum();
    let total: f32 = set.plans.iter().map(|p| p.weight).sum();
    assert!(lua / total <= 0.250001);
    assert_eq!(v.plan.principles.len(), 6);
    assert!(v.plan.principles.iter().all(|s| s.chars().count() == 90));
    assert_eq!(v.plan.on_wolf_break.len(), 1);
    assert!(
        v.plan
            .summary_lines()
            .iter()
            .any(|l| l.starts_with("principles:"))
    );
    let json = serde_json::to_string(&v.plan).unwrap();
    assert_eq!(GamePlan::from_json(&json).unwrap(), v.plan);
    let legacy = GamePlan::from_json("{\"version\":1}").unwrap();
    assert!(legacy.principles.is_empty() && legacy.on_wolf_break.is_empty());
}

#[test]
fn posture_break_metrics_count_follow_up_hits_and_successful_breaks() {
    let mut tracker = metrics::MetricsTracker::new(0.0);
    tracker.wolf(1.0, 0.75);
    tracker.wolf_break(1.0);
    tracker.post_break_hit(1.2);
    tracker.post_break_hit(3.5);
    tracker.post_break_hit(3.51);
    tracker.wolf_break(5.0);
    tracker.post_break_hit(7.51);
    let m = tracker.snapshot(8.0);
    assert_eq!(m.wolf_posture_breaks, 2);
    assert_eq!(m.hits_after_break, 2);
    assert_eq!(m.break_punish_rate, 0.5);
    assert_eq!(m.wolf_posture_peak, 1.0);
    let old: FightMetrics = serde_json::from_str("{\"duration\":31.0}").unwrap();
    assert_eq!(old.wolf_posture_breaks, 0);
    assert!(old.time_to_kill_wolf.is_none());
}

#[test]
fn boss_damage_rate_counts_drops_and_rises_without_refunding_recovery() {
    let mut tracker = metrics::MetricsTracker::new(0.0);
    tracker.boss(1.0, 0.0);
    tracker.boss(0.8, 0.5);
    tracker.boss(1.0, 0.0);
    tracker.boss(0.9, 0.2);
    let m = tracker.snapshot(60.0);
    assert!((m.boss_damage_taken_per_min - 100.0).abs() < 0.001);
    assert!(
        (Trend::new(
            &[FightRecord {
                metrics: m,
                ..Default::default()
            }],
            None
        )
        .boss_damage_taken_per_min[0]
            - 100.0)
            .abs()
            < 0.001
    );
}

#[test]
fn unknown_choices_are_dropped_and_a_plan_with_nothing_valid_is_not_usable() {
    let mut plan = good_plan();
    plan.opener = "fly9".into();
    plan.sets[0].plans.push(coach::WeightedChoice {
        plan: "3031".into(), // the Owl's
        weight: 1.0,
    });
    plan.sets[0].plans.push(coach::WeightedChoice {
        plan: "3008".into(),
        weight: -1.0,
    });
    plan.sets[1].when = Some("on_tuesday".into());
    plan.avoid.push(coach::Avoid {
        choice: "nonsense".into(),
        reason: String::new(),
    });
    let v = plan.validate(&SOLDIER);
    assert_eq!(v.plan.opener, "");
    assert_eq!(v.plan.sets[0].plans.len(), 2, "{:?}", v.plan.sets[0]);
    assert_eq!(v.plan.sets[1].when, None);
    assert_eq!(v.plan.avoid.len(), 1);
    assert!(v.warnings.len() >= 5, "{:?}", v.warnings);
    assert!(v.usable());
    // Nothing valid at all: falls back to his own AI.
    let junk = GamePlan {
        opener: "zzz".into(),
        sets: vec![
            set("a", &[("3031", 1.0), ("zzz", 1.0)]),
            set("keep", &[("3000", 1.0)]),
        ],
        start_set: "a".into(),
        ..GamePlan::default()
    };
    let v = junk.validate(&SOLDIER);
    assert!(!v.usable(), "{:?}", v.plan);
    assert!(!GamePlan::default().validate(&SOLDIER).usable());
    // A foreign version is refused whole.
    let future = GamePlan {
        version: 9,
        ..good_plan()
    };
    assert!(!future.validate(&SOLDIER).usable());
}

#[test]
fn sets_cannot_hold_plans_without_an_attack_or_in_the_avoid_list() {
    let plan = GamePlan {
        sets: vec![set(
            "a",
            &[
                ("retreat12", 1.0),
                ("hold2", 1.0),
                ("5201", 1.0),
                ("back_off6>3009", 1.0),
                ("3003", 1.0),
            ],
        )],
        avoid: vec![coach::Avoid {
            choice: "3003".into(),
            reason: "x".into(),
        }],
        ..GamePlan::default()
    };
    let v = plan.validate(&SOLDIER);
    let kept: Vec<_> = v.plan.sets[0]
        .plans
        .iter()
        .map(|p| p.plan.as_str())
        .collect();
    assert_eq!(kept, ["back_off6>3009"], "{:?}", v.warnings);
    // The opener is exempt from the attack rule.
    let open = GamePlan {
        opener: "retreat12>hold2".into(),
        ..GamePlan::default()
    };
    assert_eq!(open.validate(&SOLDIER).plan.opener, "retreat12>hold2");
}

#[test]
fn names_counts_and_notes_are_bounded() {
    let mut plan = good_plan();
    plan.notes = "é".repeat(NOTES_MAX + 50);
    plan.start_set = "nope".into();
    plan.sets.push(set("bad name!", &[("3000", 1.0)]));
    plan.sets.push(set("keep", &[("3000", 1.0)]));
    plan.sets.push(set("press", &[("3000", 1.0)]));
    for n in ["c", "d", "e"] {
        plan.sets.push(set(n, &[("3000", 1.0)]));
    }
    let v = plan.validate(&SOLDIER);
    assert_eq!(v.plan.notes.chars().count(), NOTES_MAX);
    assert_eq!(
        v.plan.sets.len(),
        4,
        "{:?}",
        v.plan.sets.iter().map(|s| &s.name).collect::<Vec<_>>()
    );
    assert_eq!(v.plan.start_set, "press");
    assert!(
        v.plan
            .sets
            .iter()
            .all(|s| s.name != "keep" && s.name != "bad name!")
    );
}

fn busy_input() -> CoachInput {
    let counts = |n: u32| Counts {
        n,
        hit: n / 3,
        blocked: n / 4,
        deflected: n / 5,
        whiffed: n / 6,
        punished: n / 7,
        countered: 0,
        counter_kinds: [0; 3],
    };
    let mut plans = TopMap::default();
    let mut overall = TopMap::default();
    for (i, c) in SOLDIER.choices().iter().enumerate() {
        plans.insert(c.value.clone(), counts(20 + i as u32));
        overall.insert(c.value.clone(), counts(200 + i as u32));
    }
    let mut wolf = TopMap::default();
    for w in [
        "AttackCombo1",
        "AttackCombo2",
        "SprintAttack",
        "JumpAttack",
        "EasyDeflectedAtk",
    ] {
        wolf.insert(w, counts(40));
    }
    let mut memory = full_memory();
    for i in 0..10 {
        memory.push_record(record(3 + i, i, Some(30.0 + i as f32)));
    }
    let big_plan = GamePlan {
        punish_preferences: Default::default(),
        explore: Default::default(),
        move_controls: Vec::new(),
        lessons: Vec::new(),
        checkpoint_prefs: Default::default(),
        boss_phase: "c1010".into(),
        counter_weights: Default::default(),
        version: 1,
        opener: "hold3.5>walk_in4".into(),
        sets: (0..4)
            .map(|i| {
                set(
                    &format!("set{i}"),
                    &[
                        ("walk_in4>hold1>3013", 1.0),
                        ("strafe_left>3005+3007", 1.0),
                        ("hold2>3002", 1.0),
                        ("keep_range>3013", 1.0),
                        ("3000+3001>back_off6", 1.0),
                        ("run_in3>3008", 1.0),
                    ],
                )
            })
            .collect(),
        start_set: "set0".into(),
        avoid: (0..4)
            .map(|_| coach::Avoid {
                choice: "3003".into(),
                reason: "deflected every time".into(),
            })
            .collect(),
        notes: "n".repeat(NOTES_MAX),
        retry: Vec::new(),
        on_wolf_break: Vec::new(),
        principles: Vec::new(),
        boss_principles: Vec::new(),
        boss_bans: Vec::new(),
    };
    let mut sets = std::collections::BTreeMap::new();
    for i in 0..4 {
        sets.insert(
            format!("set{i}"),
            SetStat {
                secs: 12.5,
                c: counts(30),
            },
        );
    }
    CoachInput {
        digest: Default::default(),
        banned: Vec::new(),
        failures: Default::default(),
        fight: 123,
        result: FightLine {
            won: "boss".into(),
            secs: 98,
            defl: 61,
            guard: 12,
            hits: 9,
            hp: 77,
            posture: 40,
            punishes: 6,
            apm: 14,
            dpm: 55,
            passive: 12,
            engaged_time: 98,
            ..Default::default()
        },
        trend: Trend::new(&memory.history, None),
        range: plans
            .0
            .iter()
            .map(|(plan, _)| (plan.clone(), [4.25, 3.75]))
            .collect(),
        plans,
        sets,
        wolf,
        overall,
        habits: memory.habits.summary(),
        played: Some(big_plan.clone()),
        notes: big_plan.notes,
        ..Default::default()
    }
}

#[test]
fn the_coach_input_is_bounded_and_keeps_the_notes_and_the_played_plan() {
    let mut input = busy_input();
    input.principles = vec!["?".repeat(90); 6];
    input.actions = SOLDIER
        .actions
        .iter()
        .map(|a| {
            (
                a.id.to_string(),
                Counts {
                    n: 12,
                    hit: 5,
                    whiffed: 7,
                    ..Default::default()
                },
            )
        })
        .collect();
    input.action_range = SOLDIER
        .actions
        .iter()
        .map(|a| (a.id.to_string(), [4.25, 3.75]))
        .collect();
    let before = input.json_len();
    let after = input.fit(INPUT_BUDGET);
    let json = serde_json::to_string(&input).unwrap();
    eprintln!("coach input: {before} bytes untrimmed, {after} fitted: {json}");
    assert!(
        before > INPUT_BUDGET,
        "the test needs a worst case, got {before}"
    );
    assert!(after <= INPUT_BUDGET && after == json.len(), "{after}");
    assert!(json.contains(&"n".repeat(NOTES_MAX)), "the notes survive");
    assert!(json.contains("\"played\":{"), "the played plan survives");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&json).unwrap()["result"]["won"],
        "boss"
    );
    assert!(json.contains("\"passive\":12"));
    assert!(!input.range.is_empty());
    assert!(
        input
            .range
            .keys()
            .all(|key| input.plans.0.iter().any(|(plan, _)| plan == key))
    );
    // A normal input (a few plans, one earlier fight) is far smaller and untouched.
    let mut small = busy_input();
    small.overall = TopMap::default();
    small.plans = TopMap::default();
    small.range.clear();
    small.wolf = TopMap::default();
    small.habits = Default::default();
    small.trend = Trend::default();
    let len = small.json_len();
    assert_eq!(small.fit(INPUT_BUDGET), len);
    assert!(len < INPUT_BUDGET, "{len}");
}

#[test]
fn all_known_bans_and_their_failure_counts_fit_the_coach_budget() {
    let mut input = busy_input();
    input.banned = SOLDIER.choices().iter().map(|c| c.value.clone()).collect();
    input.banned.sort();
    input.banned.dedup();
    input.failures = input
        .banned
        .iter()
        .map(|s| (s.clone(), [123, 99, 34, 0]))
        .collect();
    let banned = input.banned.clone();
    assert!(input.fit(INPUT_BUDGET) <= INPUT_BUDGET);
    assert_eq!(input.banned, banned);
    assert!(!input.failures.is_empty());
    assert!(input.failures.keys().all(|key| banned.contains(key)));
}

#[test]
fn mid_input_keeps_current_plan_and_results_first_within_four_kb() {
    let mut input = busy_input();
    input.phase = "mid".into();
    input.elapsed = Some(45.0);
    input.vitals = Some([0.7, 0.8, 0.9, 0.3]);
    input.marks = Some(2);
    assert!(input.fit(INPUT_BUDGET) <= INPUT_BUDGET);
    let text = serde_json::to_string(&input).unwrap();
    assert!(input.played.is_some());
    assert!(!input.plans.0.is_empty());
    assert!(text.find("\"boss_knowledge\"").unwrap() < text.find("\"player_profile\"").unwrap());
    assert!(text.starts_with("{\"boss_knowledge\":"));
}

#[test]
fn passive_share_and_the_rate_metrics_are_computed() {
    use metrics::MetricsTracker;
    let mut m = MetricsTracker::new(0.0);
    m.wolf(1.0, 0.0);
    // 20 s in 0.1 s steps; attacks start at 1, 2 and 10 s. Passive: more than 4 s since the
    // last attack, so 6 to 10 s and 14 to 20 s (the start counts as an attack at 0).
    for i in 1..=200 {
        let t = i as f32 * 0.1;
        m.input_tick(
            &crate::InputFrame {
                buttons: crate::Buttons {
                    guard: i % 20 < 15,
                    ..Default::default()
                },
                ..Default::default()
            },
            t,
            0.1,
        );
        if matches!(i, 10 | 20 | 100) {
            m.attack_started(t);
        }
        if i == 150 {
            m.wolf(0.7, 0.2);
        }
        m.passive_tick(t, 0.1);
    }
    let f = m.finish(20.0, 0, false);
    assert_eq!(f.boss_attacks, 3);
    assert!(
        (f.passive_share() - 0.5).abs() < 0.02,
        "{}",
        f.passive_share()
    );
    assert!((f.attacks_per_min() - 9.0).abs() < 1e-3);
    assert!(
        (f.damage_per_min() - 150.0).abs() < 0.01,
        "{}",
        f.damage_per_min()
    );
    assert!(f.stalled());
    assert_eq!(f.engaged_time, 0.0);
    // A fighter that attacks every 3 s is never passive.
    let mut busy = MetricsTracker::new(0.0);
    for i in 1..=300 {
        let t = i as f32 * 0.1;
        if i % 30 == 0 {
            busy.attack_started(t);
        }
        busy.passive_tick(t, 0.1);
    }
    let b = busy.finish(30.0, 0, false);
    assert_eq!((b.passive_share(), b.stalled()), (0.0, false));
    assert_eq!(b.engaged_time, 30.0);
    assert!((b.attacks_per_min() - 20.0).abs() < 1e-3);
    // The trend and the summary line show them.
    let t = Trend::new(
        &[FightRecord {
            metrics: f,
            ..FightRecord::default()
        }],
        None,
    );
    assert_eq!((t.apm[0], t.dpm[0], t.passive[0]), (9, 150, 50));
    let line = FightRecord {
        metrics: f,
        ..FightRecord::default()
    }
    .summary_line();
    assert!(
        line.contains("attacks/min 9 damage/min 150% passive 50% (STALLED: failed)"),
        "{line}"
    );
}

#[test]
fn activity_requires_early_presses_and_forty_percent_held_input() {
    let held = crate::InputFrame {
        move_stick: [0.0, 1.0],
        buttons: crate::Buttons {
            attack: true,
            guard: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut tracker = metrics::MetricsTracker::new(0.0);
    tracker.input_tick(&held, 1.0, 1.0);
    assert!(tracker.player_active(2.5));
    assert!(!tracker.player_active(2.6));
    assert_eq!(tracker.finish(2.6, 0, false).early_inputs, 3);
    let mut late = metrics::MetricsTracker::new(0.0);
    late.input_tick(&held, 10.1, 10.1);
    assert!(!late.player_active(10.1));
    let mut one = metrics::MetricsTracker::new(0.0);
    one.input_tick(
        &crate::InputFrame {
            move_stick: [0.0, 1.0],
            ..Default::default()
        },
        1.0,
        1.0,
    );
    assert!(!one.player_active(1.0));
}

#[test]
fn the_limits_force_attacks_and_rate_limit_retreats() {
    let parse = |t: &str| SOLDIER.parse(t).unwrap();
    let mut p = Pacing::new(0.0);
    // Plans without an attack are never eligible.
    for t in [
        "retreat12",
        "hold2",
        "back_off9",
        "5201",
        "strafe_left",
        "deflect",
        "dodge_back",
    ] {
        assert!(!p.eligible(&parse(t), 1.0, &SOLDIER), "{t}");
    }
    assert!(!limits::has_attack(&parse("walk_in4"), &SOLDIER));
    assert!(limits::has_attack(&parse("walk_in4>3013"), &SOLDIER));
    assert!(limits::is_retreat(&parse("back_off6>3009"), &SOLDIER));
    assert!(limits::is_retreat(&parse("5201>3000"), &SOLDIER));
    assert!(!limits::is_retreat(&parse("walk_in4>3013"), &SOLDIER));
    assert!(limits::is_retreat(&parse("dodge_back>3002"), &SOLDIER));
    assert_eq!(
        limits::lead_secs(&parse("deflect>3000+3001"), &SOLDIER),
        1.5
    );
    assert_eq!(
        limits::lead_secs(&parse("guard1>deflect>3008"), &SOLDIER),
        2.5
    );
    // One retreating plan per 5 s, including multi-retreat bait chains.
    let bait = parse("back_off6>3009");
    assert!(p.eligible(&bait, 1.0, &SOLDIER));
    p.last_retreat = Some(1.0);
    p.last_attack = 4.9;
    assert!(!p.eligible(&bait, 5.0, &SOLDIER));
    assert!(!p.eligible(&bait, 5.9, &SOLDIER));
    p.last_attack = 6.0;
    assert!(p.eligible(&bait, 6.0, &SOLDIER));
    assert!(!p.eligible(&parse("back_off6>back_off9>3000"), 6.0, &SOLDIER));
    // Short steps can be chained into a bait without exceeding the attack deadline.
    assert!(p.eligible(&parse("5211>5211>3000"), 6.0, &SOLDIER));
    p.last_attack = 9.0;
    assert!(p.eligible(&bait, 9.1, &SOLDIER));
    // A plan that does not retreat is not held back by it.
    assert!(p.eligible(&parse("3013"), 5.0, &SOLDIER));
    // Overdue (6 s without an attack): only plans that reach an attack within a second.
    let p = Pacing::new(0.0);
    assert!(p.eligible(&parse("walk_in4>3013"), 1.0, &SOLDIER));
    assert!(!p.eligible(&parse("walk_in4>3013"), 7.0, &SOLDIER));
    assert!(!p.eligible(&parse("hold2>3008"), 7.0, &SOLDIER));
    assert!(p.eligible(&parse("hold1>3008"), 7.0, &SOLDIER));
    assert!(p.eligible(&parse("3013"), 7.0, &SOLDIER));
    // A chain whose lead-in could take longer than the time left is held back early too.
    assert!(!p.eligible(&parse("retreat12>run_in3>3003"), 0.0, &SOLDIER));
    assert_eq!(
        limits::lead_secs(&parse("retreat12>hold2>3003"), &SOLDIER),
        7.0
    );
}

#[test]
fn break_log_outcomes_and_mean_conversion_delay() {
    use metrics::{BreakRecord, MetricsTracker, break_summary};
    let mut tracker = MetricsTracker::new(0.0);
    let add = |tracker: &mut MetricsTracker, t| {
        tracker.wolf_break(t);
        tracker.breaks.push(BreakRecord {
            time: t,
            action: "3000+3001+3002".into(),
            hit: 2,
            hits: 3,
            outcome: "pending".into(),
            ..Default::default()
        });
    };
    add(&mut tracker, 1.0);
    tracker.break_started(1.2);
    tracker.post_break_hit(1.5);
    tracker.post_break_hit(1.6); // One conversion, even with two landed hits.
    add(&mut tracker, 4.0);
    tracker.break_started(4.1);
    tracker.break_guarded(4.4);
    tracker.settle_break(7.0, false);
    add(&mut tracker, 8.0);
    tracker.break_started(8.2);
    tracker.settle_break(11.0, false);
    add(&mut tracker, 12.0);
    tracker.settle_break(15.0, false);
    add(&mut tracker, 16.0);
    tracker.break_started(16.1);
    tracker.post_break_hit(16.3);
    let m = tracker.snapshot(17.0);
    assert!((m.break_conversion_delay.unwrap() - 0.4).abs() < 1e-5);
    assert_eq!(m.break_punish_rate, 0.4);
    assert_eq!(
        tracker
            .breaks
            .iter()
            .map(|r| r.outcome.as_str())
            .collect::<Vec<_>>(),
        ["hit", "guarded", "whiffed", "missed window", "hit"]
    );
    assert_eq!(
        break_summary(&tracker.breaks)["3000+3001+3002 hit 2/3"],
        [5, 2]
    );
    assert!(tracker.breaks[3].start_delay.is_none());
    let old: FightMetrics = serde_json::from_str("{}").unwrap();
    assert!(old.break_conversion_delay.is_none());
}
