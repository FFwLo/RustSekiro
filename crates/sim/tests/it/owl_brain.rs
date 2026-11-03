//! The boss brain driving the Great Shinobi Owl (c5060): plans run through his own battle goal,
//! the tally counts what happened, a slow controller falls back to his Lua AI without freezing,
//! and his reflexes keep working while a plan runs. Skips when `cache/` lacks him.

use sekiro_sim::brain::observation::observe;
use sekiro_sim::brain::{LuaBrain, Plan, ScriptedBrain, Side};
use sekiro_sim::duel::Duel;
use sekiro_sim::enemy::EnemySpec;
use sekiro_sim::lua_ai::AiEvent;
use sekiro_sim::{Buttons, InputFrame};

const DT: f32 = 1.0 / 60.0;

#[test]
fn wolf_victory_uses_encounter_defeat_for_owl_and_father_variants() {
    let cache = sekiro_sim::cache::root();
    if !cache.join("anim/c5060/tae.json").exists()
        || !cache.join("anim/c5070/tae.json").exists()
        || !cache
            .join("raw/script/m10_00_00_00.luabnd.d/506000_battle.lua")
            .exists()
        || !cache.join("raw/script/m11_01_00_00.luabnd.d").is_dir()
    {
        eprintln!("skipping: Owl and Father are not extracted");
        return;
    }
    for arena in ["owl", "hirata-father", "hirata-inner-father"] {
        let spec = sekiro_sim::arena::ArenaSpec::by_name(arena)
            .unwrap()
            .enemy_spec();
        let mut duel = Duel::load_with(&cache, 1.6, spec).unwrap();
        duel.god = true;
        // Father keeps his native Lua goal; attach the observer without castle move plans.
        duel.set_brain(Box::new(LuaBrain), false).unwrap();
        assert!(duel.events.is_some());
        for _ in 0..150 {
            duel.step(&InputFrame::default(), DT);
        }
        super::genichiro_brain::take_mark(&mut duel);
        let driver = duel.boss_brain.as_ref().unwrap();
        assert_eq!(driver.memory.wolf_wins, 0, "{arena} phase transition");
        assert!(driver.last_record.is_none());
        // Let native phase-two AI leave Owl's heat-up kneel before isolating the final pair.
        duel.enemy_active = true;
        for _ in 0..600 {
            duel.step(&InputFrame::default(), DT);
        }
        if let Some(bird) = duel.father_bird.as_mut() {
            bird.duel.enemy_active = false;
        }
        super::genichiro_brain::take_mark_with_ai(&mut duel, true);
        for _ in 0..600 {
            duel.step(&InputFrame::default(), DT);
            if duel.boss_brain.as_ref().unwrap().last_record.is_some() {
                break;
            }
        }
        let driver = duel.boss_brain.as_ref().unwrap();
        assert_eq!(driver.memory.wolf_wins, 1, "{arena} final deathblow");
        assert_eq!(driver.memory.fights, 1);
        let record = driver.last_record.as_ref().unwrap();
        assert_eq!(record.winner, "wolf");
        assert_eq!(record.metrics.kill_time, Some(record.metrics.duration));
    }
}

fn load(distance: f32) -> Option<Duel> {
    let cache = sekiro_sim::cache::root();
    if !cache.join("anim/c5060/tae.json").exists()
        || !cache.join("raw/script/m11_01_00_00.luabnd.d").is_dir()
    {
        eprintln!("skipping: the Owl is not extracted");
        return None;
    }
    Some(Duel::load_with(&cache, distance, EnemySpec::owl()).unwrap())
}

fn scripted(opener: &[i32], delay: f32) -> Box<ScriptedBrain> {
    Box::new(ScriptedBrain::new(
        Plan::new(opener),
        Plan::new(opener),
        u32::MAX,
        delay,
    ))
}

fn frame(guard: bool, attack: bool, lock_on: bool) -> InputFrame {
    InputFrame {
        buttons: Buttons {
            guard,
            attack,
            lock_on,
            ..Buttons::default()
        },
        ..InputFrame::default()
    }
}

/// Keeps Wolf alive and unbroken, so the Owl fights for the whole run.
fn keep_wolf_fresh(duel: &mut Duel) {
    let wolf = &mut duel.rules.fighters[0];
    wolf.vitality.hp = wolf.vitality.max_hp;
    wolf.posture.remaining = wolf.posture.max;
}

/// The Owl's attack and step requests (`AiEvent::Action`) since the last call.
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

/// Scripted combat path: a real slash breaks low-posture Wolf, then Lua follows up.
#[test]
fn local_coach_response_replaces_owls_post_break_grab() {
    use sekiro_sim::brain::ScriptedCoach;
    use sekiro_sim::brain::coach::{GamePlan, WeightedChoice};
    let Some(mut duel) = load(1.6) else { return };
    duel.set_brain(Box::new(ScriptedCoach::new(Vec::new(), 0.0, 0.0)), false)
        .unwrap();
    let mut fastest: Vec<_> = sekiro_sim::brain::OWL
        .actions
        .iter()
        .filter_map(|a| {
            Some((
                a.id,
                duel.boss_brain
                    .as_ref()
                    .unwrap()
                    .move_sheet
                    .get(&a.id.to_string())?
                    .first_hit(1.6)?,
            ))
        })
        .collect();
    fastest.sort_by(|a, b| a.1.total_cmp(&b.1));
    eprintln!(
        "Owl fastest at 1.6m: {:?}",
        &fastest[..fastest.len().min(5)]
    );
    duel.boss_brain.as_mut().unwrap().game_plan = Some(GamePlan {
        opener: "3001".into(),
        punish_preferences: punish_preference("3014"),
        on_wolf_break: vec![WeightedChoice {
            plan: "3000".into(),
            weight: 1.0,
        }],
        ..Default::default()
    });
    duel.boss_brain.as_mut().unwrap().start_fight();
    duel.force_act = Some(3001);
    let mut broke = None;
    let mut first_response = None;
    let mut requested_response = false;
    for tick in 0..1200 {
        duel.rules.fighters[0].vitality.hp = duel.rules.fighters[0].vitality.max_hp;
        if broke.is_none() {
            duel.rules.fighters[0].posture.remaining = 1;
        }
        let report = duel.step(&frame(false, false, tick == 0), DT);
        if broke.is_none() && duel.rules.fighters[0].posture.broken() {
            broke = Some(tick);
            duel.force_act = None;
            actions(&mut duel);
            continue;
        }
        let requested = actions(&mut duel);
        if let Some(start) = broke {
            assert!(
                !requested.contains(&3040),
                "Lua grab replaced: {requested:?}"
            );
            requested_response |= requested.contains(&3014);
            if report.enemy.clip_started
                && report
                    .enemy
                    .animation
                    .as_deref()
                    .and_then(sekiro_sim::lua_ai::anim_id_from_clip)
                    == Some(3014)
            {
                first_response = Some(tick - start);
                break;
            }
        }
    }
    assert!(broke.is_some(), "Wolf never broke");
    assert!(requested_response, "3014 never requested");
    let ticks = first_response.expect("3014 animation never started");
    eprintln!(
        "Owl local break response 3014 started after {ticks} ticks ({:.3}s)",
        ticks as f32 * DT
    );
    assert!(ticks <= 18, "response took {ticks} ticks");
    assert_eq!(duel.boss_brain.as_ref().unwrap().memory_now().fights, 0);
}

#[test]
fn lua_brain_leaves_his_choices_unchanged() {
    let Some(mut plain) = load(6.0) else { return };
    let mut with_brain = load(6.0).unwrap();
    with_brain.set_brain(Box::new(LuaBrain), false).unwrap();
    let (mut a, mut b) = (Vec::new(), Vec::new());
    for _ in 0..1800 {
        keep_wolf_fresh(&mut plain);
        keep_wolf_fresh(&mut with_brain);
        plain.step(&InputFrame::default(), DT);
        with_brain.step(&InputFrame::default(), DT);
        a.extend(actions(&mut plain));
        b.extend(actions(&mut with_brain));
    }
    assert!(a.len() >= 8, "{a:?}");
    assert_eq!(a, b);
    let stats = with_brain.boss_brain.as_ref().unwrap().stats;
    assert_eq!((stats.requests, stats.planned), (0, 0));
    assert!(stats.fallback > 0, "{stats:?}");
}

#[test]
fn owl_performs_the_planned_actions_in_order() {
    let Some(mut duel) = load(4.0) else { return };
    // An order his own acts never use. Not the kick (3005): when it lands it leaves SpEffect
    // 3506000 on Wolf, and his AI's interrupt answers with its own follow-up (3007).
    let plan = [3003, 3002, 3001];
    duel.set_brain(scripted(&plan, 0.1), false).unwrap();
    let mut runs: Vec<Vec<i32>> = Vec::new();
    let mut seen = 0;
    let mut largest_obs = 0;
    for _ in 0..3600 {
        keep_wolf_fresh(&mut duel);
        duel.step(&InputFrame::default(), DT);
        let driver = duel.boss_brain.as_ref().unwrap();
        if driver.history.len() > seen {
            seen = driver.history.len();
            runs.push(Vec::new());
            let obs = observe(&duel, driver.time, [None, Some(1.0)], "3000+3001+3002");
            largest_obs = largest_obs.max(serde_json::to_string(&obs).unwrap().len());
        }
        let acted = actions(&mut duel);
        if let Some(run) = runs.last_mut() {
            run.extend(acted);
        }
    }
    let stats = duel.boss_brain.as_ref().unwrap().stats;
    eprintln!("{stats:?}; runs {runs:?}; largest observation {largest_obs} bytes");
    let complete = runs.iter().filter(|r| r.as_slice() == plan).count();
    assert!(
        complete >= 5,
        "{complete} complete runs of {plan:?}: {runs:?}"
    );
    // Every planned slot starts with the plan's first action (a passive Wolf never makes him
    // react), and nothing but the plan's actions run inside it.
    for r in &runs {
        assert!(r.iter().all(|a| plan.contains(a)), "{r:?}");
    }
    assert!(stats.planned as usize >= complete);
    assert!(largest_obs < 1024, "{largest_obs}");
}

#[test]
fn tally_counts_outcomes_in_a_scripted_duel() {
    let Some(mut duel) = load(3.0) else { return };
    duel.set_brain(scripted(&[3000, 3001], 0.1), false).unwrap();
    // [owl hit, owl blocked or deflected] from the duel's own reports, while Owl plans run.
    let mut owl = [0u32; 2];
    let mut wolf_hits = 0u32;
    for i in 0..3600 {
        keep_wolf_fresh(&mut duel);
        // First 30 s passive, then hold guard and slash now and then.
        let guarding = i >= 1800 && i % 240 < 200;
        let attack = i >= 1800 && i % 240 == 220;
        let r = duel.step(&frame(guarding, attack, i == 1802), DT);
        for (h, res) in &r.hits {
            if h.attacker == 1 {
                owl[usize::from(res.guarded || res.deflected)] += 1;
            } else if !res.guarded && !res.deflected {
                wolf_hits += 1;
            }
        }
        actions(&mut duel);
    }
    for _ in 0..120 {
        duel.step(&InputFrame::default(), DT);
    }
    let driver = duel.boss_brain.as_ref().unwrap();
    let s = driver.tally.summary();
    eprintln!(
        "reports: owl hits {owl:?}, wolf hits {wolf_hits}; tally {}",
        serde_json::to_string(&s).unwrap()
    );
    let total = |side: Side, f: fn(&sekiro_sim::brain::Counts) -> u32| {
        let m = if side == Side::Boss { &s.boss } else { &s.wolf };
        m.values().map(f).sum::<u32>()
    };
    let n: u32 = s
        .boss
        .0
        .iter()
        .filter(|(key, _)| {
            key.parse::<i32>()
                .is_ok_and(|id| (3000..3100).contains(&id))
        })
        .filter(|(key, _)| driver.move_sheet.get(key).is_none_or(|m| m.contact))
        .map(|(_, c)| c.n)
        .sum();
    for (key, c) in &s.boss.0 {
        if key
            .parse::<i32>()
            .is_ok_and(|id| (5200..5300).contains(&id))
        {
            assert_eq!(c.whiffed, 0, "back step {key} cannot whiff");
        }
    }
    let hit = total(Side::Boss, |c| c.hit);
    let guarded = total(Side::Boss, |c| c.blocked + c.deflected);
    let whiffed = total(Side::Boss, |c| c.whiffed);
    assert!(owl[0] > 0 && owl[1] > 0, "{owl:?}");
    // An exchange counts a hit or a guard once however many of its hits landed.
    assert!(hit > 0 && hit <= owl[0], "{hit} vs {owl:?}");
    assert!(guarded > 0 && guarded <= owl[1], "{guarded} vs {owl:?}");
    assert!(
        whiffed + hit + guarded >= n,
        "every attack exchange has an outcome"
    );
    assert!(s.boss["3000"].n > 0 && s.boss["3001"].n > 0);
    // Wolf's slashes are in his tally, and the ones that landed punished the Owl.
    assert!(total(Side::Wolf, |c| c.n) > 0);
    if wolf_hits > 0 {
        assert!(total(Side::Wolf, |c| c.hit) > 0);
        assert!(total(Side::Boss, |c| c.punished) > 0);
    }
}

/// Runs a minute against a passive Wolf; returns the number of steps with an Owl action
/// request and the longest stretch of steps without one.
fn action_rhythm(duel: &mut Duel) -> (usize, usize) {
    let mut last_action = 0;
    let mut longest_gap = 0;
    let mut count = 0;
    for i in 0..3600 {
        keep_wolf_fresh(duel);
        duel.step(&InputFrame::default(), DT);
        if !actions(duel).is_empty() {
            longest_gap = longest_gap.max(i - last_action);
            last_action = i;
            count += 1;
        }
    }
    (count, longest_gap.max(3600 - last_action))
}

#[test]
fn slow_brain_falls_back_to_his_lua_ai_without_freezing() {
    let Some(mut duel) = load(6.0) else { return };
    // Far slower than any plan: by the next act the answer is still out, so his Lua AI picks.
    duel.set_brain(scripted(&[3005], 4.0), false).unwrap();
    let (count, gap) = action_rhythm(&mut duel);
    let stats = duel.boss_brain.as_ref().unwrap().stats;
    let mut lua = load(6.0).unwrap();
    let (lua_count, lua_gap) = action_rhythm(&mut lua);
    eprintln!(
        "{stats:?}; {count} actions, longest gap {gap} steps (Lua alone: {lua_count}, {lua_gap})"
    );
    assert!(stats.fallback >= 5, "{stats:?}");
    assert!(stats.requests >= 3, "{stats:?}");
    assert!(count >= 15, "{count}");
    // No longer idle than his own AI gets (it waits and strafes too), plus a second.
    assert!(
        gap <= lua_gap.max(240) + 60,
        "the Owl stood still for {gap} steps"
    );
}

#[test]
fn reflex_deflects_happen_while_a_plan_runs() {
    let Some(mut duel) = load(2.5) else { return };
    duel.set_brain(scripted(&[3000, 3001, 3002], 0.1), false)
        .unwrap();
    let mut reflexes_in_plan = 0;
    let mut guarded_in_plan = 0;
    for i in 0..5400 {
        keep_wolf_fresh(&mut duel);
        // Lock on and slash every half second.
        let r = duel.step(&frame(false, i % 30 == 0, i == 2), DT);
        let running = !duel.boss_brain.as_ref().unwrap().running.is_empty();
        let acted = actions(&mut duel);
        if running {
            reflexes_in_plan += acted.iter().filter(|a| (3100..=3103).contains(*a)).count();
            guarded_in_plan += r
                .hits
                .iter()
                .filter(|(h, res)| h.attacker == 0 && (res.guarded || res.deflected))
                .count();
        }
    }
    let stats = duel.boss_brain.as_ref().unwrap().stats;
    eprintln!(
        "{stats:?}; guard/deflect requests {reflexes_in_plan}, guarded hits {guarded_in_plan}"
    );
    assert!(stats.planned >= 3, "{stats:?}");
    assert!(reflexes_in_plan > 0, "no guard or deflect while a plan ran");
}

#[test]
fn absent_break_plan_uses_a_reachable_sheet_punish() {
    use sekiro_sim::brain::ScriptedCoach;
    let Some(mut duel) = load(1.6) else { return };
    duel.set_brain(Box::new(ScriptedCoach::new(Vec::new(), 0.0, 0.0)), false)
        .unwrap();
    duel.force_act = Some(3001);
    let mut broke = None;
    for tick in 0..1200 {
        duel.rules.fighters[0].vitality.hp = duel.rules.fighters[0].vitality.max_hp;
        if broke.is_none() {
            duel.rules.fighters[0].posture.remaining = 1;
        }
        duel.step(&frame(false, false, tick == 0), DT);
        if broke.is_none() && duel.rules.fighters[0].posture.broken() {
            broke = Some(tick);
            duel.force_act = None;
            actions(&mut duel);
            continue;
        }
        let requested = actions(&mut duel);
        if let Some(start) = broke
            && requested.iter().any(|id| {
                duel.boss_brain
                    .as_ref()
                    .unwrap()
                    .move_sheet
                    .get(&id.to_string())
                    .is_some_and(|m| {
                        m.damaging() && m.reachable_time([1.6, 0.0], [0.0; 2]).is_some()
                    })
            })
        {
            eprintln!(
                "Owl reachable sheet break follow-up requested after {} ticks",
                tick - start
            );
            return;
        }
    }
    panic!("Owl did not use a reachable punish after the break");
}
#[test]
fn middle_combo_break_uses_tae_window() {
    combo_break_fixture(false, "3014", "3000+3001+3002", 3);
}

#[test]
fn owl_brain_coach_preferred_punish_conversion_ab() {
    use sekiro_sim::brain::{
        ScriptedCoach,
        coach::{GamePlan, WeightedChoice},
    };
    let mut delays = Vec::new();
    for fastest in [false, true] {
        let Some(mut duel) = load(1.6) else { return };
        // Compare a legacy supplied arm with an explicit coach preference from the sheet.
        let brain: Box<dyn sekiro_sim::brain::BossBrain> = if fastest {
            Box::new(ScriptedCoach::new(Vec::new(), 0.0, 0.0))
        } else {
            Box::new(LuaBrain)
        };
        duel.set_brain(brain, false).unwrap();
        duel.boss_brain.as_mut().unwrap().game_plan = Some(GamePlan {
            punish_preferences: if fastest {
                punish_preference("3014")
            } else {
                Default::default()
            },
            on_wolf_break: vec![WeightedChoice {
                plan: "3000".into(),
                weight: 1.0,
            }],
            ..Default::default()
        });
        duel.force_act = Some(3001);
        let positions = [duel.player.body.position, duel.enemy.body.position];
        let mut broke = false;
        let mut converted = None;
        for tick in 0..600 {
            duel.player.body.position = positions[0];
            duel.enemy.body.position = positions[1];
            duel.rules.fighters[0].vitality.hp = duel.rules.fighters[0].vitality.max_hp;
            if !broke {
                duel.rules.fighters[0].posture.remaining = 1;
            }
            duel.step(&frame(false, false, tick == 0), DT);
            if !broke && duel.rules.fighters[0].posture.broken() {
                broke = true;
                duel.force_act = None;
            }
            let input = duel
                .boss_brain
                .as_ref()
                .unwrap()
                .coach_input(&Default::default());
            if let Some(record) = input.break_log.first()
                && record.outcome == "hit"
            {
                converted = Some((tick + 1) as f32 * DT - record.time);
                break;
            }
        }
        let delay = converted.expect("break must be converted");
        eprintln!("Owl conversion fastest={fastest}: 1/1 at {delay:.3}s");
        delays.push(delay);
    }
    assert!(delays[1] < delays[0], "{delays:?}");
}

#[test]
fn middle_combo_break_without_window_waits_until_free() {
    combo_break_fixture(true, "3014", "3000+3001+3002", 3);
}

#[test]
fn middle_combo_break_can_repeat_the_current_attack() {
    combo_break_fixture(false, "3001", "3000+3001+3002", 3);
}

#[test]
fn break_log_knows_the_combo_inside_a_tactic_plan() {
    combo_break_fixture(false, "3014", "walk_in4>3000+3001", 2);
}

fn combo_break_fixture(no_window: bool, punish: &str, opener: &str, hit_count: usize) {
    use sekiro_sim::brain::ScriptedCoach;
    use sekiro_sim::brain::coach::{GamePlan, WeightedChoice};
    let Some(mut duel) = load(1.6) else { return };
    duel.set_brain(Box::new(ScriptedCoach::new(Vec::new(), 0.0, 0.0)), false)
        .unwrap();
    if no_window {
        let original = &duel.enemy.tae;
        let mut db = sekiro_sim::tae::TaeDb::from_animations(original.anim_ids().map(|id| {
            let name = sekiro_formats::tae::anim_name(id);
            let mut anim = original.anim(&name).unwrap().clone();
            if name == "a000_003001" {
                anim.events = std::sync::Arc::new(
                    anim.events
                        .iter()
                        .filter(|e| {
                            !matches!(
                                e.kind,
                                sekiro_sim::tae::TaeKind::ActionFlag(23 | 86 | 79 | 11)
                            )
                        })
                        .cloned()
                        .collect(),
                );
            }
            (id, anim)
        }));
        db.sp_effects = original.sp_effects.clone();
        db.velocity_changes = original.velocity_changes.clone();
        duel.enemy.tae = std::sync::Arc::new(db);
        duel.boss_brain.as_mut().unwrap().move_sheet =
            sekiro_sim::brain::moves::build(&duel, &sekiro_sim::brain::OWL);
    }
    if punish == "3001" {
        // Isolate the existing same-animation melee cancel regression from faster projectiles.
        for row in duel.boss_brain.as_mut().unwrap().move_sheet.values_mut() {
            row.projectiles.clear();
        }
    }
    duel.boss_brain.as_mut().unwrap().game_plan = Some(GamePlan {
        opener: opener.into(),
        punish_preferences: punish_preference(punish),
        on_wolf_break: vec![WeightedChoice {
            plan: punish.into(),
            weight: 1.0,
        }],
        ..Default::default()
    });
    duel.boss_brain.as_mut().unwrap().start_fight();
    let positions = [duel.player.body.position, duel.enemy.body.position];
    let mut broke = false;
    let mut break_time = 0.0;
    let mut waiting = 0.0;
    let mut natural = 0.0;
    let mut elapsed = 0.0;
    let mut previous_clip_time = 0.0;
    for tick in 0..1200 {
        duel.player.body.position = positions[0];
        duel.enemy.body.position = positions[1];
        let anim = duel
            .enemy
            .behavior
            .runtime
            .main_clip()
            .and_then(|c| sekiro_sim::lua_ai::anim_id_from_clip(&c.animation));
        duel.rules.fighters[0].vitality.hp = duel.rules.fighters[0].vitality.max_hp;
        if !broke {
            duel.rules.fighters[0].posture.remaining = if anim == Some(3001) {
                1
            } else {
                duel.rules.fighters[0].posture.max
            };
        }
        let report = duel.step(&frame(false, false, tick == 0), DT);
        if !broke && duel.rules.fighters[0].posture.broken() {
            let c = duel.enemy.behavior.runtime.main_clip().unwrap();
            assert_eq!(
                sekiro_sim::lua_ai::anim_id_from_clip(&c.animation),
                Some(3001)
            );
            let m = &duel.boss_brain.as_ref().unwrap().move_sheet["3001"];
            waiting = m.wait(c.time);
            natural = m.duration - c.time;
            break_time = c.time;
            previous_clip_time = c.time;
            broke = true;
            continue;
        }
        if broke {
            elapsed += DT;
            let requested = actions(&mut duel);
            if elapsed < 4.0 && (!requested.is_empty() || report.enemy.clip_started) {
                eprintln!(
                    "break trace elapsed {elapsed:.3}: requested {requested:?}, clip {:?}, plan {}",
                    report.enemy.animation,
                    duel.boss_brain.as_ref().unwrap().running.value()
                );
            }
            if report.enemy.clip_started
                && report
                    .enemy
                    .animation
                    .as_deref()
                    .and_then(sekiro_sim::lua_ai::anim_id_from_clip)
                    == punish.parse().ok()
            {
                eprintln!(
                    "middle break clip time {break_time}, cancel wait {waiting}, natural remaining {natural}, punish delay {elapsed}"
                );
                let m = &duel.boss_brain.as_ref().unwrap().move_sheet["3001"];
                let allowed = break_time + waiting;
                assert!(
                    previous_clip_time + DT + 0.001 >= allowed,
                    "started before cancel/free: {previous_clip_time} < {allowed}"
                );
                assert!(
                    previous_clip_time <= allowed + DT + 0.001,
                    "missed first frame: {previous_clip_time} > {allowed}"
                );
                if no_window {
                    assert!((allowed - m.duration).abs() < 0.002);
                } else {
                    assert!(previous_clip_time < m.duration);
                }
                let input = duel
                    .boss_brain
                    .as_ref()
                    .unwrap()
                    .coach_input(&Default::default());
                let log = input.break_log.last().unwrap();
                assert_eq!((log.hit, log.hits), (2, hit_count));
                assert!(log.started && log.start_delay.is_some());
                let mut fitted = input.clone();
                let budget = sekiro_sim::brain::coach::INPUT_BUDGET;
                let bytes = fitted.fit(budget);
                assert!(bytes <= budget);
                assert!(
                    fitted
                        .viable_punishes
                        .values()
                        .flatten()
                        .any(|p| p.id == opener.rsplit('>').next().unwrap())
                );
                assert!(!fitted.break_log.is_empty() && !fitted.break_summary.is_empty());
                eprintln!("request size {bytes} bytes");
                return;
            }
            if let Some(c) = duel.enemy.behavior.runtime.main_clip()
                && sekiro_sim::lua_ai::anim_id_from_clip(&c.animation) == Some(3001)
            {
                previous_clip_time = c.time;
            }
        }
    }
    panic!("middle combo break did not punish: broke={broke}");
}

#[test]
fn owl_move_sheet_matches_guard_and_deflect_resolution_at_current_power() {
    use sekiro_sim::brain::OWL;
    use sekiro_sim::hits::HitEvent;
    use sekiro_sim::progression::AttackPower;
    let Some(mut duel) = load(1.6) else { return };
    for power in [1, 6] {
        duel.rules
            .set_attack_power(AttackPower::new(power).unwrap())
            .unwrap();
        duel.set_brain(Box::new(LuaBrain), false).unwrap();
        let sheet = duel.boss_brain.as_ref().unwrap().move_sheet.clone();
        assert!(sheet.len() > OWL.actions.len() + OWL.combos.len());
        for counter in [3061, 3066, 3064, 3068, 3060, 3065, 3069, 3090, 3091] {
            assert!(
                sheet.contains_key(&counter.to_string()),
                "missing counter {counter}"
            );
        }
        assert_eq!(sheet["3000+3001"].hits.len(), 2);
        for action in [
            "3000",
            "3001",
            "3002",
            "3005",
            "3017",
            "3063",
            "3000+3001",
            "3000+3001+3002",
        ] {
            let row = &sheet[action];
            eprintln!(
                "Owl AP{power} {action}: {} hits {:?}, duration {}, recovery {}, reach {:?}, cancels {:?}",
                row.hits.len(),
                row.hits,
                row.duration,
                row.recovery,
                row.reach,
                row.cancels
            );
        }
        let row = &sheet["3000"];
        let attack = duel.fighters[1].attacks[&100].clone();
        let forward = duel.player.body.forward();
        let contact = HitEvent {
            attacker: 1,
            defender: 0,
            judge: 100,
            attack,
            position: duel.player.body.position,
            direction: [-forward[0], -forward[1]],
        };
        for (deflect, column) in [(false, 1), (true, 2)] {
            keep_wolf_fresh(&mut duel);
            let frame = &mut duel.player.tae_runtime.frame;
            frame.flags.insert(sekiro_sim::hits::FLAG_GUARD);
            frame.guard_judge = Some(90);
            frame.sp_effects = if deflect { vec![105010] } else { Vec::new() };
            let result = duel.rules.resolve(
                &contact,
                [&duel.player, &duel.enemy],
                [&duel.fighters[0], &duel.fighters[1]],
            );
            assert!(result.guarded);
            assert_eq!(result.deflected, deflect);
            assert_eq!(result.posture_damage as f32, row.hits[0][column]);
        }
        // A new power forecast uses a fresh idle guard baseline, not the previous contact's effect.
        duel.player.tae_runtime.frame = Default::default();
    }
}

fn punish_preference(
    id: &str,
) -> std::collections::BTreeMap<String, Vec<sekiro_sim::brain::coach::Avoid>> {
    std::collections::BTreeMap::from([(
        "near".into(),
        vec![sekiro_sim::brain::coach::Avoid {
            choice: id.into(),
            reason: "Use this authored follow-up in the cancel regression".into(),
        }],
    )])
}
