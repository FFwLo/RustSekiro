//! The boss brain driving the Ashina soldier (c1010): plans of his own action ids run through
//! his battle goal, unknown ids are refused, his Lua AI picks when no answer comes, the repeat
//! aid forces his acts, and the memory of every fight survives a death and a new session.
//! Skips when `cache/` lacks him.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use sekiro_sim::brain::{
    BossBrain, BrainKind, BrainReply, BrainRequest, Loaded, Memory, Plan, REQUEST_BUDGET,
    ScriptedBrain,
};
use sekiro_sim::duel::Duel;
use sekiro_sim::enemy::EnemySpec;
use sekiro_sim::lua_ai::AiEvent;
use sekiro_sim::{Buttons, InputFrame};

const DT: f32 = 1.0 / 60.0;

#[test]
fn single_phase_deathblow_and_player_death_record_opposite_winners() {
    let Some(mut duel) = load(1.6) else { return };
    duel.god = true;
    duel.set_brain(Box::new(sekiro_sim::brain::LuaBrain), false)
        .unwrap();
    assert!(duel.events.is_none());
    for _ in 0..150 {
        duel.step(&InputFrame::default(), DT);
    }
    super::genichiro_brain::take_mark(&mut duel);
    let driver = duel.boss_brain.as_ref().unwrap();
    assert_eq!(driver.memory.wolf_wins, 1);
    assert_eq!(driver.memory.fights, 1);
    let record = driver.last_record.as_ref().unwrap();
    assert_eq!(record.winner, "wolf");
    assert!(record.metrics.kill_time.is_some());
    duel.reset().unwrap();
    for i in 0..150 {
        duel.step(
            &InputFrame {
                buttons: Buttons {
                    guard: i % 40 != 0,
                    ..Default::default()
                },
                ..Default::default()
            },
            DT,
        );
    }
    duel.god = false;
    duel.rules.fighters[0].vitality.hp = 0;
    duel.step(&InputFrame::default(), DT);
    let driver = duel.boss_brain.as_ref().unwrap();
    assert_eq!(driver.memory.boss_wins, 1);
    assert_eq!(driver.memory.wolf_wins, 1);
    assert_eq!(driver.memory.fights, 2);
    let record = driver.last_record.as_ref().unwrap();
    assert_eq!(record.winner, "boss");
    assert_eq!(record.metrics.kill_time, None);
    assert_eq!(
        record.metrics.time_to_kill_wolf,
        Some(record.metrics.duration)
    );
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

fn scripted(plan: &[i32], delay: f32) -> Box<ScriptedBrain> {
    Box::new(ScriptedBrain::new(
        Plan::new(plan),
        Plan::new(plan),
        u32::MAX,
        delay,
    ))
}

fn keep_wolf_fresh(duel: &mut Duel) {
    let wolf = &mut duel.rules.fighters[0];
    wolf.vitality.hp = wolf.vitality.max_hp;
    wolf.posture.remaining = wolf.posture.max;
}

/// Keeps the soldier unbroken too (Wolf's slashes otherwise break and deathblow him).
fn keep_both_fresh(duel: &mut Duel) {
    keep_wolf_fresh(duel);
    let boss = &mut duel.rules.fighters[1];
    boss.vitality.hp = boss.vitality.max_hp;
    boss.posture.remaining = boss.posture.max;
}

/// The soldier's action requests since the last call.
fn actions(duel: &mut Duel) -> Vec<i32> {
    duel.ai
        .as_mut()
        .unwrap()
        .drain_log()
        .into_iter()
        .filter_map(|e| match e {
            AiEvent::Action(a) => Some(a),
            AiEvent::Error(e) => panic!("AI error: {e}"),
            _ => None,
        })
        .collect()
}

fn attack_every(i: usize, period: usize) -> InputFrame {
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
fn soldier_performs_the_planned_actions_in_order() {
    let Some(mut duel) = load(4.0) else { return };
    // An order his own acts never use: the running sweep, then the sweeping slash. (Not the
    // leaping slash 3003: it knocks Wolf down, SpEffect 109031, and his interrupt replans
    // into his own reactions, Act24 and Act25, which drop the plan as they should.)
    let plan = [3002, 3000];
    duel.set_brain(scripted(&plan, 0.1), false).unwrap();
    let mut runs: Vec<Vec<i32>> = Vec::new();
    let mut seen = 0;
    for _ in 0..3600 {
        keep_wolf_fresh(&mut duel);
        duel.step(&InputFrame::default(), DT);
        let driver = duel.boss_brain.as_ref().unwrap();
        if driver.history.len() > seen {
            seen = driver.history.len();
            runs.push(Vec::new());
        }
        let acted = actions(&mut duel);
        if let Some(run) = runs.last_mut() {
            run.extend(acted);
        }
    }
    let stats = duel.boss_brain.as_ref().unwrap().stats;
    eprintln!("{stats:?}; runs {runs:?}");
    let complete = runs.iter().filter(|r| r.as_slice() == plan).count();
    assert!(
        complete >= 4,
        "{complete} complete runs of {plan:?}: {runs:?}"
    );
    for r in &runs {
        assert!(r.iter().all(|a| plan.contains(a)), "{r:?}");
    }
    assert_eq!(stats.invalid, 0);
}

#[test]
fn unknown_ids_are_refused_and_his_ai_acts() {
    let Some(mut duel) = load(4.0) else { return };
    // 3004 is in his script (Act04) but his TAE has no clip for it; 3031 is the Owl's.
    for bad in [3004, 3031] {
        duel.set_brain(scripted(&[bad], 0.1), false).unwrap();
        let mut acted = Vec::new();
        for _ in 0..1200 {
            keep_wolf_fresh(&mut duel);
            duel.step(&InputFrame::default(), DT);
            acted.extend(actions(&mut duel));
        }
        let stats = duel.boss_brain.as_ref().unwrap().stats;
        eprintln!("{bad}: {stats:?}; {acted:?}");
        assert!(stats.invalid > 0, "{stats:?}");
        assert_eq!(stats.planned, 0, "{stats:?}");
        assert!(!acted.contains(&bad));
        assert!(
            acted.iter().any(|a| (3000..3100).contains(a)),
            "his Lua AI attacks: {acted:?}"
        );
    }
}

#[test]
fn lua_picks_when_no_answer_comes() {
    let Some(mut duel) = load(4.0) else { return };
    // An answer that never arrives within the run.
    duel.set_brain(scripted(&[3008], 1000.0), false).unwrap();
    let mut attacks = 0;
    for _ in 0..2400 {
        keep_wolf_fresh(&mut duel);
        duel.step(&InputFrame::default(), DT);
        attacks += actions(&mut duel)
            .iter()
            .filter(|a| (3000..3100).contains(*a))
            .count();
    }
    let stats = duel.boss_brain.as_ref().unwrap().stats;
    eprintln!("{stats:?}; {attacks} attacks");
    assert_eq!((stats.requests, stats.replies, stats.planned), (1, 0, 0));
    assert!(stats.fallback >= 4, "{stats:?}");
    assert!(attacks >= 4, "{attacks}");
}

#[test]
fn repeat_aid_and_force_act_drive_his_acts() {
    let Some(mut duel) = load(3.0) else { return };
    // The repeat brain (one action over and over) through his battle goal.
    duel.set_brain(BrainKind::Repeat(3013).build("c1010"), false)
        .unwrap();
    let mut thrusts = 0;
    for _ in 0..1800 {
        keep_wolf_fresh(&mut duel);
        let r = duel.step(&InputFrame::default(), DT);
        thrusts += usize::from(
            r.enemy.clip_started && r.enemy.animation.as_deref() == Some("a000_003013"),
        );
        actions(&mut duel);
    }
    assert!(thrusts >= 3, "{thrusts} perilous thrusts");
    // `--force-act` with his AI off (`--passive`).
    let mut duel = load(3.0).unwrap();
    duel.enemy_active = false;
    duel.force_act = Some(3014);
    let mut sweeps = 0;
    for _ in 0..600 {
        keep_wolf_fresh(&mut duel);
        let r = duel.step(&InputFrame::default(), DT);
        sweeps += usize::from(
            r.enemy.clip_started && r.enemy.animation.as_deref() == Some("a000_003014"),
        );
    }
    assert!(sweeps >= 2, "{sweeps} low sweeps");
}

/// A scripted brain that keeps every request it is sent, as JSON.
struct Recorder {
    inner: ScriptedBrain,
    seen: Rc<RefCell<Vec<String>>>,
}

impl BossBrain for Recorder {
    fn name(&self) -> &'static str {
        "recorder"
    }
    fn submit(&mut self, request: &BrainRequest) {
        self.seen
            .borrow_mut()
            .push(serde_json::to_string(request).unwrap());
        self.inner.submit(request);
    }
    fn poll(&mut self, now: f32) -> Option<BrainReply> {
        self.inner.poll(now)
    }
}

fn memory_file() -> PathBuf {
    std::env::temp_dir()
        .join(format!(
            "sekiro-soldier-memory-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
        .join("c1010.json")
}

#[test]
fn saved_plan_failures_inform_the_coach_without_an_automatic_ban() {
    let Some(mut duel) = load(4.0) else { return };
    let path = memory_file();
    let mut memory = Memory::new("c1010");
    memory.coach.failures.insert(
        "3000".into(),
        sekiro_sim::brain::memory::PlanFailures {
            failed: 0,
            uses: 3,
            deflected_all: 2,
            fatal: 0,
        },
    );
    memory.save(&path).unwrap();
    duel.set_brain(scripted(&[3000], 0.0), false).unwrap();
    duel.boss_brain.as_mut().unwrap().use_memory(path.clone());
    for _ in 0..900 {
        keep_both_fresh(&mut duel);
        duel.step(&InputFrame::default(), DT);
    }
    let driver = duel.boss_brain.as_ref().unwrap();
    assert!(!driver.history.is_empty());
    assert!(driver.stats.planned > 0);
    assert_eq!(driver.memory.coach.failures["3000"].uses, 3);
    drop(duel);
    assert_eq!(
        Memory::load(
            &sekiro_sim::brain::memory::LayerPaths::new(&path, "default").player,
            "c1010"
        )
        .0
        .coach
        .failures["3000"]
            .deflected_all,
        2
    );
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_dir(path.parent().unwrap());
}

#[test]
fn memory_survives_a_death_and_a_new_session() {
    let Some(mut duel) = load(4.0) else { return };
    let path = memory_file();
    let seen = Rc::new(RefCell::new(Vec::new()));
    duel.set_brain(
        Box::new(Recorder {
            inner: ScriptedBrain::for_actions(&sekiro_sim::brain::SOLDIER),
            seen: seen.clone(),
        }),
        false,
    )
    .unwrap();
    assert_eq!(
        duel.boss_brain.as_mut().unwrap().use_memory(path.clone()),
        Loaded::Missing
    );
    // Fight 1: Wolf slashes now and then for 15 s, then is left at 1 HP for the soldier.
    let mut respawned_at = None;
    for i in 0..60 * 90 {
        if i < 60 * 15 {
            keep_both_fresh(&mut duel);
        } else if i == 60 * 15 {
            duel.rules.fighters[0].vitality.hp = 1;
        }
        let input = if i < 60 * 15 {
            InputFrame {
                move_stick: [0.0, 1.0],
                ..attack_every(i, 50)
            }
        } else {
            InputFrame::default()
        };
        let r = duel.step(&input, DT);
        if r.respawned {
            respawned_at = Some(i);
            break;
        }
    }
    let respawned_at = respawned_at.expect("Wolf died and respawned");
    {
        let d = duel.boss_brain.as_ref().unwrap();
        assert_eq!(d.stats.fights, 1);
        assert_eq!((d.memory.fights, d.memory.boss_wins), (1, 1));
        assert!(d.memory.habits.attack_dist.n > 0, "{:?}", d.memory.habits);
        assert!(d.tally.is_empty(), "a fresh tally for fight 2");
    }
    let saved = Memory::load(
        &sekiro_sim::brain::memory::LayerPaths::new(&path, "default").player,
        "c1010",
    )
    .0;
    assert_eq!((saved.fights, saved.boss_wins), (1, 1));
    // Fight 2: 15 s, then the session ends mid-fight.
    for i in 0..60 * 15 {
        keep_both_fresh(&mut duel);
        duel.step(
            &InputFrame {
                move_stick: [0.0, 1.0],
                ..attack_every(i, 50)
            },
            DT,
        );
    }
    let first_session = seen.borrow().clone();
    drop(duel);
    let (after, how) = Memory::load(
        &sekiro_sim::brain::memory::LayerPaths::new(&path, "default").player,
        "c1010",
    );
    assert_eq!(how, Loaded::Read);
    eprintln!(
        "respawn at step {respawned_at}; memory after two fights: {}",
        serde_json::to_string(&after).unwrap()
    );
    assert_eq!((after.fights, after.boss_wins, after.wolf_wins), (2, 1, 0));
    assert!(
        after.boss.values().map(|c| c.n).sum::<u32>()
            > saved.boss.values().map(|c| c.n).sum::<u32>()
    );
    assert!(after.wolf.values().map(|c| c.n).sum::<u32>() > 0);
    assert!(after.habits.seconds > 30.0, "{}", after.habits.seconds);
    // Requests carried the memory and stayed in budget.
    let largest = first_session.iter().map(String::len).max().unwrap();
    assert!(largest <= REQUEST_BUDGET, "{largest}");
    assert!(
        first_session
            .iter()
            .any(|r| r.contains("\"memory\":{\"fights\":1,\"boss_wins\":1")),
        "fight 2's requests remember fight 1"
    );
    // A new session starts from both fights.
    let mut duel = load(4.0).unwrap();
    let seen = Rc::new(RefCell::new(Vec::new()));
    duel.set_brain(
        Box::new(Recorder {
            inner: ScriptedBrain::for_actions(&sekiro_sim::brain::SOLDIER),
            seen: seen.clone(),
        }),
        false,
    )
    .unwrap();
    assert_eq!(
        duel.boss_brain.as_mut().unwrap().use_memory(path.clone()),
        Loaded::Read
    );
    for _ in 0..600 {
        keep_wolf_fresh(&mut duel);
        duel.step(&InputFrame::default(), DT);
    }
    let first = seen.borrow().first().cloned().expect("a request");
    eprintln!(
        "first request of session 2 ({} bytes): {first}",
        first.len()
    );
    assert!(
        first.contains("\"memory\":{\"fights\":2,\"boss_wins\":1,\"wolf_wins\":0"),
        "{first}"
    );
    drop(duel);
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_dir(path.parent().unwrap());
}
