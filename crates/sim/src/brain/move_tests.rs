use super::{
    GENICHIRO,
    adaptation::{AttackEvidence, signal},
    bandit::{self, Evidence},
    digest,
    memory::{LayerPaths, MEMORY_VERSION, Memory},
    move_bandit::{Control, Explore, Fighter},
    plan::Weighted,
    sampler::{self, Context, Pick, Rng, Streaks},
    tally::{CounterKind, Exchange, Outcome, Side},
};
use std::collections::{BTreeMap, HashSet};

fn set() -> Vec<Weighted> {
    [3025, 3016, 3000, 3001]
        .into_iter()
        .map(|id| Weighted {
            plan: Some(GENICHIRO.parse(&id.to_string()).unwrap()),
            weight: 1.0,
        })
        .collect()
}

fn exchange(id: i32, t: f32, broken: bool) -> Exchange {
    let outcome = match id {
        3025 => Outcome {
            countered: Some(CounterKind::Mikiri),
            ..Default::default()
        },
        3016 => Outcome {
            deflected: true,
            ..Default::default()
        },
        _ => Outcome {
            hit: true,
            progress: true,
            broke: !broken && id == 3000,
            ..Default::default()
        },
    };
    Exchange {
        phase: "c7100".into(),
        side: Side::Boss,
        key: id.to_string(),
        start: t,
        end: Some(t + if id == 3001 { 3.0 } else { 0.4 }),
        outcome,
    }
}

struct Temp(std::path::PathBuf);
impl Temp {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!(
            "sekiro-move-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn replay_memory() -> Memory {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let tmp = Temp::new();
    for file in ["c7100.boss.json", "c7100.player-default.json"] {
        let source = root.join("logs/memory").join(file);
        if source.exists() {
            std::fs::copy(source, tmp.0.join(file)).unwrap();
        }
    }
    LayerPaths::new(&tmp.0.join("c7100.json"), "default")
        .load("c7100")
        .unwrap()
        .1
}

#[test]
fn move_bandit_converges_explores_and_capitalises_without_family_churn() {
    let set = set();
    let mut fighter = Fighter::default();
    let mut rng = Rng::new(8);
    let mut counts = [0; 4];
    let mut switches = 0;
    let mut last = String::new();
    for t in 0..500 {
        let Pick::Plan(i) = fighter.choose(
            "c7100",
            "near/blow/wbroken/300",
            &set,
            super::move_bandit::Settings {
                controls: &[],
                explore: Explore::Normal,
                every: 2,
            },
            &mut rng,
        ) else {
            panic!()
        };
        let arm = set[i].plan.as_ref().unwrap().value();
        let family = super::move_bandit::family(&arm);
        switches += usize::from(!last.is_empty() && last != family);
        last = family;
        fighter
            .runs
            .push((t as f32 * 4.0, "near/blow/wbroken/300".into(), arm.clone()));
        let x = exchange(arm.parse().unwrap(), t as f32 * 4.0, true);
        fighter.openings.push(x.start);
        if x.outcome.hit {
            fighter.contacts.push((x.end.unwrap(), arm));
        }
        fighter.settle(&x);
        if t >= 300 {
            counts[i] += 1;
        }
    }
    assert!(counts[2] > 180, "{counts:?}");
    assert!(counts[0] + counts[1] < 10, "{counts:?}");
    assert!(fighter.provisional["c7100"]["near/blow/wbroken/300"].len() == 4);
    assert!(switches <= 250, "{switches}");
}

#[test]
fn move_coach_priors_and_exploration_controls_shift_early_choices() {
    let set = set();
    let sample = |confidence, heavy| {
        let mut set = set.clone();
        if heavy {
            set[2].weight = 9.0;
        }
        (0..500)
            .filter(|seed| {
                let mut f = Fighter::default();
                f.choose(
                    "c7100",
                    "near/blow/wlow/none",
                    &set,
                    super::move_bandit::Settings {
                        controls: &[Control {
                            plan: "3000".into(),
                            confidence,
                            lock: false,
                        }],
                        explore: Explore::Low,
                        every: 2,
                    },
                    &mut Rng::new(*seed),
                ) == Pick::Plan(2)
            })
            .count()
    };
    assert!(sample(1.0, false) > sample(0.0, false) + 50);
    assert!(sample(0.0, true) > sample(0.0, false) + 50);
}

#[test]
fn move_decay_and_both_legacy_loaders_roundtrip_lessons() {
    let tmp = Temp::new();
    let path = tmp.0.join("c7100.json");
    let mut m = Memory::new("c7100");
    let mut lessons = Vec::new();
    for i in 0..9 {
        digest::remember(&mut lessons, &[format!("hypothesis {i}")]);
    }
    assert_eq!(lessons.len(), 6);
    assert_eq!(lessons[0], "hypothesis 3");
    m.coach.lessons = lessons.clone();
    m.coach.hypotheses.push(digest::Hypothesis {
        fight: 5,
        idea: "quick punish converts".into(),
        plans: "3000".into(),
        outcome: [20.0, 0.8, 240.0],
    });
    m.move_bandit
        .entry("c7100".into())
        .or_default()
        .entry("near/blow/wlow/none".into())
        .or_default()
        .insert(
            "3000".into(),
            Evidence {
                success: 10.0,
                failure: 4.0,
            },
        );
    bandit::decay(&mut m.move_bandit);
    assert!((m.move_bandit["c7100"]["near/blow/wlow/none"]["3000"].success - 9.8).abs() < 1e-5);
    let text = serde_json::to_string(&m).unwrap();
    std::fs::write(&path, &text).unwrap();
    let (loaded, _) = Memory::load(&path, "c7100");
    assert_eq!(loaded, m);
    let paths = LayerPaths::new(&path, "default");
    std::fs::write(&paths.player, &text).unwrap();
    assert_eq!(paths.load("c7100").unwrap().1, m);
    let mut old: serde_json::Value = serde_json::from_str(&text).unwrap();
    old["version"] = 4.into();
    old.as_object_mut().unwrap().remove("move_bandit");
    old["coach"].as_object_mut().unwrap().remove("lessons");
    let text = old.to_string();
    std::fs::write(&path, &text).unwrap();
    std::fs::write(&paths.player, &text).unwrap();
    for loaded in [
        Memory::load(&path, "c7100").0,
        paths.load("c7100").unwrap().1,
    ] {
        assert_eq!(loaded.version, MEMORY_VERSION);
        assert!(loaded.move_bandit.is_empty());
        assert!(loaded.coach.lessons.is_empty());
    }
}

#[test]
fn move_review_triggers_periodically_and_on_stall_and_table_prioritises_unused() {
    let mut memory = Memory::new("c7100");
    memory.fights = 5;
    memory.boss_wins = 5;
    memory.boss.insert(
        "3025".into(),
        super::tally::Counts {
            n: 10,
            countered: 10,
            ..Default::default()
        },
    );
    memory.boss.insert(
        "3000".into(),
        super::tally::Counts {
            n: 10,
            hit: 10,
            ..Default::default()
        },
    );
    let build = |m: &Memory, every| {
        digest::build(
            &GENICHIRO,
            m,
            &Default::default(),
            &Default::default(),
            every,
        )
    };
    let d = build(&memory, 5);
    assert!(d.review);
    assert!(d.moves.find("unused").unwrap() < d.moves.find("3025 ").unwrap());
    assert!(d.moves.find("3025 ").unwrap() < d.moves.find("3000 ").unwrap());
    assert!(d.moves.len() < 2000);
    memory.fights = 6;
    memory.boss_wins = 6;
    assert!(!build(&memory, 5).review);
    memory.fights = 10;
    assert!(
        !build(&memory, 5).review,
        "unfinished fights do not advance the review interval"
    );
    memory.history = vec![
        super::metrics::FightRecord {
            winner: "wolf".into(),
            ..Default::default()
        };
        8
    ];
    assert!(build(&memory, 5).review);
    assert!(!build(&memory, 0).review);
    memory.coach.last_review_fight = 6;
    assert!(!build(&memory, 5).review);
    memory.history.push(super::metrics::FightRecord {
        winner: "boss".into(),
        metrics: super::metrics::FightMetrics {
            duration: 25.0,
            ..Default::default()
        },
        ..Default::default()
    });
    memory.history.push(super::metrics::FightRecord::default());
    let digest = build(&memory, 5);
    assert_eq!(
        digest.fights.len(),
        9,
        "unfinished records do not enter the trend"
    );
    assert_eq!(
        digest.fights.last().unwrap()[0],
        25.0,
        "legacy winning duration supplies kill time"
    );
}

/// This is a selection replay, not a claim to emulate Wolf's HKS or Lua callbacks.
/// Outcomes go through the production classifier and per-move settlement.
#[test]
fn move_offline_real_memory_three_sampler_replay() {
    let memory = replay_memory();
    let set = set();
    let mut totals = [[[0_u32; 4]; 6]; 3];
    let mut switches = [0_u32; 3];
    for (mode, fights) in totals.iter_mut().enumerate() {
        for seed in 0..64 {
            let mut rng = Rng::new(seed);
            let mut fighter = Fighter::default();
            let mut learned: BTreeMap<String, AttackEvidence> = memory
                .attack_learning
                .get("c7100")
                .cloned()
                .unwrap_or_default();
            // Reproduce the v4 migration case even after the live profile has advanced to v5.
            let mut persisted = bandit::BanditMemory::new();
            for (fight, counts) in fights.iter_mut().enumerate() {
                let before_switches = switches[mode];
                let mut streaks = Streaks::default();
                let mut tried = HashSet::new();
                let mut last_family = String::new();
                for t in 0..96 {
                    // Four ordinary slots then four broken-posture slots, repeated.
                    let broken = t % 8 >= 4;
                    let context = format!(
                        "near/blow/w{}/{}",
                        if broken { "broken" } else { "low" },
                        fighter.last
                    );
                    let i = if mode == 2 {
                        fighter.evidence = persisted.clone();
                        let Pick::Plan(i) = fighter.choose(
                            "c7100",
                            &context,
                            &set,
                            super::move_bandit::Settings {
                                controls: &[],
                                explore: Explore::Normal,
                                every: 2,
                            },
                            &mut rng,
                        ) else {
                            panic!()
                        };
                        i
                    } else {
                        let mut view = set.clone();
                        if mode == 1 {
                            for w in &mut view {
                                let key = w.plan.as_ref().unwrap().value();
                                w.weight *=
                                    learned.get(&key).copied().unwrap_or_default().multiplier();
                            }
                        }
                        let Pick::Plan(i) = sampler::pick(
                            &view,
                            &Context {
                                streaks: &streaks,
                                tried: &tried,
                                temperature: 1.0,
                            },
                            &mut rng,
                        ) else {
                            panic!()
                        };
                        i
                    };
                    counts[i] += 1;
                    let arm = set[i].plan.as_ref().unwrap().value();
                    let family = super::move_bandit::family(&arm);
                    switches[mode] += u32::from(!last_family.is_empty() && last_family != family);
                    last_family = family;
                    let x = exchange(arm.parse().unwrap(), (fight * 96 + t) as f32 * 4.0, broken);
                    let (success, failure) = signal(&x);
                    if mode == 0 {
                        streaks.record(&arm, x.outcome.deflected || x.outcome.blocked);
                    }
                    if mode == 1 {
                        streaks.record(&arm, failure > success);
                        learned
                            .entry(arm.clone())
                            .or_default()
                            .record(success, failure);
                    }
                    if mode == 2 {
                        fighter.runs.push((x.start, context, arm.clone()));
                        if broken {
                            fighter.openings.push(x.start);
                        }
                        if x.outcome.hit {
                            fighter.contacts.push((x.end.unwrap(), arm.clone()));
                            fighter.damage.push((x.end.unwrap(), arm.clone(), 0.15));
                        }
                        fighter.settle(&x);
                    }
                    tried.insert(arm);
                }
                if mode == 2 {
                    assert!(switches[mode] - before_switches <= 48);
                }
                fighter.finish(&mut persisted, false);
                fighter.reset();
            }
        }
    }
    for (mode, fights) in totals.iter().enumerate() {
        eprintln!(
            "REPLAY {} bad_percent={:?} quick_punisher={:?} switches={:.3}",
            ["old_streak", "persistent_evidence", "move_bandit"][mode],
            fights.map(|c| 100.0 * (c[0] + c[1]) as f32 / 6144.0),
            fights.map(|c| 100.0 * c[2] as f32 / 6144.0),
            switches[mode] as f32 / (64.0 * 6.0 * 95.0)
        );
    }
    for counts in &totals[2][3..] {
        assert!(
            (counts[0] + counts[1]) as f32 / 6144.0 < 0.05,
            "{:?}",
            totals[2]
        );
    }
    assert!(totals[2][2..].iter().map(|c| c[0] + c[1]).sum::<u32>() > 0);
    assert!(totals[2][5][2] > totals[2][0][2], "{:?}", totals[2]);
    assert!(switches[2] as f32 / (64.0 * 6.0 * 95.0) < 0.55);
}

#[test]
fn move_opening_reward_uses_contact_time_not_clip_recovery() {
    let mut f = Fighter::default();
    let context = "near/blow/wbroken/300";
    f.runs.push((1.0, context.into(), "3000".into()));
    f.openings.push(0.9);
    f.contacts.push((1.1, "3000".into()));
    let mut x = exchange(3000, 1.0, true);
    x.end = Some(7.0);
    f.settle(&x);
    assert!(f.provisional["c7100"][context]["3000"].success > 0.95);
}

#[test]
fn move_damage_rate_rewards_real_progress_over_elapsed_plan_time() {
    let reward = |elapsed| {
        let mut f = Fighter::default();
        f.runs
            .push((0.0, "near/blow/wlow/300".into(), "3000".into()));
        f.damage.push((0.4, "3000".into(), 0.15));
        let mut x = exchange(3000, 0.0, false);
        x.outcome.broke = false;
        x.end = Some(elapsed);
        f.settle(&x);
        f.provisional["c7100"]["near/blow/wlow/300"]["3000"].success
    };
    assert!(reward(0.4) > reward(8.0));
}

#[test]
fn move_safe_setup_is_neutral_but_a_punished_setup_teaches_failure() {
    let mut f = Fighter::default();
    f.runs
        .push((0.0, "near/blow/wlow/300".into(), "hold1>3000".into()));
    let mut x = exchange(3000, 0.0, false);
    x.key = "hold1".into();
    x.outcome = Default::default();
    f.settle(&x);
    assert!(f.provisional.is_empty());
    x.outcome.punished = true;
    f.settle(&x);
    assert_eq!(
        f.provisional["c7100"]["near/blow/wlow/300"]["hold1>3000"].failure,
        1.0
    );
}

#[test]
fn move_lua_delegation_does_not_become_an_unlearned_novelty_arm() {
    let mut set = set();
    set.push(Weighted {
        plan: None,
        weight: 100.0,
    });
    let mut f = Fighter::default();
    let mut rng = Rng::new(71);
    let mut lua = 0;
    for _ in 0..1000 {
        lua += usize::from(
            f.choose(
                "c7100",
                "near/blow/wlow/300",
                &set,
                super::move_bandit::Settings {
                    controls: &[],
                    explore: Explore::Normal,
                    every: 1,
                },
                &mut rng,
            ) == Pick::Lua,
        );
    }
    assert!((180..=310).contains(&lua), "{lua}");
}

#[test]
fn move_phase_handover_reconsiders_the_arm_without_reusing_a_held_castle_choice() {
    let set = set();
    let mut switched = 0;
    for seed in 0..40 {
        let mut f = Fighter::default();
        for (phase, best) in [("c7100", 0), ("c7110", 2)] {
            for (i, w) in set.iter().enumerate() {
                f.evidence
                    .entry(phase.into())
                    .or_default()
                    .entry("near/blow/wlow/300".into())
                    .or_default()
                    .insert(
                        w.plan.as_ref().unwrap().value(),
                        Evidence {
                            success: if i == best { 1000.0 } else { 0.0 },
                            failure: if i == best { 0.0 } else { 1000.0 },
                        },
                    );
            }
        }
        let mut rng = Rng::new(seed);
        let settings = || super::move_bandit::Settings {
            controls: &[],
            explore: Explore::Low,
            every: 2,
        };
        let castle = f.choose("c7100", "near/blow/wlow/300", &set, settings(), &mut rng);
        let tomoe = f.choose("c7110", "near/blow/wlow/300", &set, settings(), &mut rng);
        switched += usize::from(castle == Pick::Plan(0) && tomoe == Pick::Plan(2));
    }
    assert!(switched >= 35, "{switched}");
}

#[test]
fn move_table_measures_text_and_array_encoding_and_preserves_review_budget() {
    let memory = replay_memory();
    let table = digest::build(
        &GENICHIRO,
        &memory,
        &Default::default(),
        &Default::default(),
        5,
    )
    .moves;
    let arrays: Vec<Vec<serde_json::Value>> = table
        .lines()
        .map(|l| {
            l.split_whitespace()
                .map(|c| {
                    c.parse::<u32>()
                        .map_or_else(|_| serde_json::Value::from(c), serde_json::Value::from)
                })
                .collect()
        })
        .collect();
    let json = serde_json::to_string(&arrays).unwrap();
    let text = serde_json::to_string(&table).unwrap();
    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/alt-fb");
    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(
        out.join("table-formats.json"),
        serde_json::to_string(&serde_json::json!({"text":text,"arrays":json})).unwrap(),
    )
    .unwrap();
    eprintln!("TABLE bytes text={} arrays={}", text.len(), json.len());
    assert!(text.len() < json.len());
    let mut input = super::coach::CoachInput::default();
    input.digest.moves =
        "id label n hit dg counter whiff progress recent/life%\n3000 test unused\n".into();
    input
        .digest
        .moves
        .push_str(&"3001 used 100 20 30 20 30 ? 20/20\n".repeat(300));
    input.digest.review = true;
    assert!(input.fit(super::coach::INPUT_BUDGET) <= super::coach::INPUT_BUDGET);
    assert!(input.digest.moves.contains("unused"));
    assert!(input.digest.review);
}
