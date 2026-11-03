//! Offline castle Genichiro planning, real clips, arrows and isolated memory.

use sekiro_sim::brain::{BossActions, BrainKind, GENICHIRO, LuaBrain, Plan, ScriptedBrain};
use sekiro_sim::{InputFrame, duel::Duel, lua_ai::AiEvent, registry::Registry};

const DT: f32 = 1.0 / 60.0;

#[test]
fn counter_memory_suppresses_mikiri_bait_and_is_in_the_fitted_coach_input() {
    let Some(mut duel) = load(4.0) else { return };
    duel.set_brain(BrainKind::Scripted.build("c7100"), false)
        .unwrap();
    let driver = duel.boss_brain.as_mut().unwrap();
    driver.memory.boss.insert(
        "3076".into(),
        sekiro_sim::brain::tally::Counts {
            n: 25,
            hit: 1,
            countered: 24,
            counter_kinds: [24, 0, 0],
            ..Default::default()
        },
    );
    duel.step(&InputFrame::default(), DT);
    let hook = duel.ai.as_ref().unwrap().state.act_hook.as_ref().unwrap();
    let factor = hook
        .counter_weights
        .iter()
        .find(|(id, _)| *id == 14)
        .unwrap()
        .1;
    assert!(factor > 0.0 && factor < 0.1, "exploration floor: {factor}");
    assert!(
        hook.counter_weights
            .iter()
            .find(|(id, _)| *id == 1)
            .unwrap()
            .1
            > 0.0
    );
    let mut input = duel
        .boss_brain
        .as_ref()
        .unwrap()
        .coach_input(&Default::default());
    assert!(
        input.fit(sekiro_sim::brain::coach::INPUT_BUDGET) <= sekiro_sim::brain::coach::INPUT_BUDGET
    );
    assert_eq!(input.counters["3076"].countered, 24);
    assert_eq!(input.boss_phase, "c7100");
}

struct CheckpointRecorder {
    inner: ScriptedBrain,
    seen: std::rc::Rc<std::cell::RefCell<Vec<Option<&'static str>>>>,
}

impl sekiro_sim::brain::BossBrain for CheckpointRecorder {
    fn name(&self) -> &'static str {
        "scripted"
    }
    fn default_mode(&self) -> sekiro_sim::brain::RequestMode {
        sekiro_sim::brain::RequestMode::Checkpoints
    }
    fn submit(&mut self, request: &sekiro_sim::brain::BrainRequest) {
        self.seen.borrow_mut().push(request.why);
        self.inner.submit(request);
    }
    fn poll(&mut self, now: f32) -> Option<sekiro_sim::brain::BrainReply> {
        self.inner.poll(now)
    }
}

#[test]
fn genichiro_posture_pressure_checkpoints_rearm() {
    let Some(mut duel) = load(4.0) else { return };
    let seen = std::rc::Rc::default();
    let plan = GENICHIRO.parse("hold2>strafe_left").unwrap();
    duel.set_brain(
        Box::new(CheckpointRecorder {
            inner: ScriptedBrain::new(plan.clone(), plan, u32::MAX, 0.2),
            seen: std::rc::Rc::clone(&seen),
        }),
        false,
    )
    .unwrap();
    duel.boss_brain.as_mut().unwrap().anti_stall = false;
    let attack = duel
        .player
        .tae
        .anim("a050_300000")
        .unwrap()
        .events
        .iter()
        .find_map(|e| match e.kind {
            sekiro_sim::tae::TaeKind::Attack { judge, .. } => duel.fighters[0].attacks.get(&judge),
            _ => None,
        })
        .unwrap();
    let per_hit = duel
        .rules
        .wolf_posture_per_hit(attack, [&duel.player, &duel.enemy])
        .unwrap();
    assert!(per_hit > 0.0);
    for hits_left in [100.0, 5.0, 2.5, 100.0, 5.0, 2.5] {
        for _ in 0..120 {
            let boss = &mut duel.rules.fighters[1];
            boss.posture.remaining = ((hits_left * per_hit) as i32).min(boss.posture.max);
            duel.step(&InputFrame::default(), DT);
            actions(&mut duel);
        }
    }
    assert_eq!(
        *seen.borrow(),
        [
            Some("start"),
            Some("pressure"),
            Some("pressure_critical"),
            Some("pressure"),
            Some("pressure_critical")
        ]
    );
}

#[test]
fn genichiro_plan_sets_obey_the_six_second_attack_rule() {
    use sekiro_sim::brain::coach::{GamePlan, PlanSet, WeightedChoice};
    use sekiro_sim::brain::limits::Pacing;
    let plan = GamePlan {
        version: 1,
        sets: vec![PlanSet {
            name: "press".into(),
            plans: ["hold2", "3000+3001+3002", "back_off9>3009+3044"]
                .map(|plan| WeightedChoice {
                    plan: plan.into(),
                    weight: 1.0,
                })
                .to_vec(),
            ..PlanSet::default()
        }],
        start_set: "press".into(),
        ..GamePlan::default()
    }
    .validate(&GENICHIRO)
    .plan;
    assert_eq!(plan.sets[0].plans.len(), 2);
    let pacing = Pacing::new(0.0);
    assert!(pacing.eligible(&GENICHIRO.parse("3000+3001+3002").unwrap(), 6.0, &GENICHIRO));
    assert!(!pacing.eligible(
        &GENICHIRO.parse("back_off9>3009+3044").unwrap(),
        6.0,
        &GENICHIRO
    ));
}

fn load(distance: f32) -> Option<Duel> {
    let cache = sekiro_sim::cache::root();
    if !cache.join("anim/c7100/skeleton.bin").exists() {
        eprintln!("skipping: Genichiro is not extracted");
        return None;
    }
    let spec = Registry::load(&cache)
        .unwrap()
        .resolve("c7100:71001000")
        .unwrap();
    let mut duel = Duel::load_with(&cache, distance, spec).unwrap();
    duel.god = true;
    Some(duel)
}

fn actions(duel: &mut Duel) -> Vec<i32> {
    duel.ai
        .as_mut()
        .unwrap()
        .drain_log()
        .into_iter()
        .filter_map(|e| match e {
            AiEvent::Action(a) => Some(a),
            AiEvent::Error(e) => panic!("{e}"),
            _ => None,
        })
        .collect()
}

#[test]
fn genichiro_brain_idle_guard_is_failed_and_cannot_stall() {
    let Some(mut duel) = load(3.0) else { return };
    let plan = GENICHIRO.parse("guard1>3005").unwrap();
    duel.set_brain(
        Box::new(ScriptedBrain::new(plan.clone(), plan, u32::MAX, 0.0)),
        false,
    )
    .unwrap();
    duel.boss_brain.as_mut().unwrap().anti_stall = true;
    let mut starts = Vec::new();
    let mut guards = 0;
    for i in 0..1200 {
        let r = duel.step(&InputFrame::default(), DT);
        if r.enemy.clip_started {
            if r.enemy.animation.as_deref() == Some("a000_003100") {
                guards += 1;
            }
            if r.enemy
                .animation
                .as_deref()
                .and_then(|s| s.strip_prefix("a000_"))
                .and_then(|s| s.parse::<i32>().ok())
                .is_some_and(|id| (3000..3100).contains(&id))
            {
                starts.push(i as f32 * DT);
            }
        }
    }
    let d = duel.boss_brain.as_ref().unwrap();
    eprintln!("idle guards {guards}, attacks {starts:?}");
    assert!(guards > 0);
    assert!(!starts.is_empty() && starts[0] <= 7.5, "{starts:?}");
    assert!(
        starts.windows(2).all(|pair| pair[1] - pair[0] <= 7.5),
        "{starts:?}"
    );
    assert!(20.0 - starts.last().unwrap() <= 7.5, "{starts:?}");
    let counts = &d.tally.summary().boss.0;
    eprintln!("counts {counts:?}");
    assert!(
        counts
            .iter()
            .any(|(key, c)| key == "guard1" && c.n > 0 && c.whiffed == 0),
        "idle guard is separate from an attack miss"
    );
    assert!(
        d.coach_input(&Default::default())
            .failures
            .values()
            .any(|f| f[3] > 0),
        "idle guard must fail its plan"
    );
    let d = duel.boss_brain.as_mut().unwrap();
    d.end_fight(sekiro_sim::brain::memory::FightEnd::BossWon);
    let record = d.last_record.as_ref().unwrap();
    assert_eq!(
        record.metrics.boss_attacks as usize,
        starts.len(),
        "guards must not count as attacks"
    );
    assert!(
        record.metrics.passive_share() > 0.05,
        "idle guards must stay passive"
    );
}

#[test]
fn scripted_genichiro_attacks_idle_god_wolf_for_sixty_seconds() {
    let Some(mut duel) = load(6.0) else { return };
    duel.set_brain(BrainKind::Scripted.build("c7100"), false)
        .unwrap();
    let mut chosen_attacks = 0;
    for _ in 0..3600 {
        let report = duel.step(&InputFrame::default(), DT);
        assert!(report.enemy.behavior.script_errors.is_empty());
        if report.enemy.clip_started {
            let driver = duel.boss_brain.as_ref().unwrap();
            if let Some(animation) = &report.enemy.animation
                && let Some(id) = animation
                    .strip_prefix("a000_")
                    .and_then(|s| s.parse::<i32>().ok())
                && (3000..3100).contains(&id)
                && driver
                    .history
                    .iter()
                    .any(|p| p.steps.contains(&sekiro_sim::brain::plan::Step::Act(id)))
            {
                chosen_attacks += 1;
            }
        }
        actions(&mut duel);
    }
    let driver = duel.boss_brain.as_ref().unwrap();
    eprintln!(
        "Genichiro: {chosen_attacks} chosen attack starts; {:?}",
        driver.stats
    );
    assert!(chosen_attacks >= 5);
    assert!(driver.stats.planned >= 5);
    assert_eq!(driver.stats.invalid, 0);
    assert!(
        driver
            .history
            .iter()
            .all(|p| sekiro_sim::brain::limits::has_attack(p, &GENICHIRO))
    );
}

#[test]
fn genichiro_move_sheet_builds_real_combo_timelines() {
    let Some(mut duel) = load(4.0) else { return };
    duel.set_brain(BrainKind::Scripted.build("c7100"), false)
        .unwrap();
    let sheet = &duel.boss_brain.as_ref().unwrap().move_sheet;
    for id in [3000, 3001, 3005, 3018, 3034, 3062, 3076] {
        let m = &sheet[&id.to_string()];
        eprintln!(
            "sheet {id}: contact={} first={:?} reach={:?}/{:?} recovery={} hits={:?} damage={:?} projectiles={:?}",
            m.contact,
            m.first_hit(3.0),
            m.first_reach,
            m.reach,
            m.recovery,
            m.hits,
            m.damage,
            m.projectiles
        );
    }
    for action in GENICHIRO.actions {
        assert!(
            sheet.contains_key(&action.id.to_string()),
            "missing {}",
            action.id
        );
    }
    let combo = &sheet["3000+3001+3002"];
    assert_eq!(combo.hits.len(), 3);
    assert_eq!(
        combo.hits.iter().map(|h| h[3] as i32).collect::<Vec<_>>(),
        [100, 101, 102]
    );
    assert!((combo.hits[1][0] - (0.9 + 0.767)).abs() < 0.002);
    assert!(combo.hits.iter().all(|h| h[1] > 0.0 && h[2] > 0.0));
    assert!(combo.reach.unwrap() > 0.0);
    assert_eq!(sheet["3040+3041"].hits.len(), 7);
    assert!(!sheet["3007"].cancels.is_empty());
    assert!((sheet["3000"].hits[0][0] - 0.667).abs() < 0.002);
    assert!(sheet["3000"].first_reach.unwrap() > 3.0);
    assert!(!sheet["3018"].projectiles.is_empty());
    assert!(!sheet["3034"].projectiles.is_empty());
    assert!(!sheet["5201"].contact);
    assert!(sheet["3000"].chain_next.contains(&3001));
    assert!(sheet["3000"].cancel_into.contains(&3005));
}

#[test]
fn genichiro_brain_gate_scripted_retreat_ab() {
    let mut results = Vec::new();
    for gated in [false, true] {
        let Some(mut duel) = load(3.5) else { return };
        let plan = GENICHIRO.parse("3000+3001+3002").unwrap();
        duel.set_brain(
            Box::new(ScriptedBrain::new(plan.clone(), plan, u32::MAX, 0.0).with_move_selection()),
            false,
        )
        .unwrap();
        let d = duel.boss_brain.as_mut().unwrap();
        d.brain_moves = false;
        d.mode = sekiro_sim::brain::RequestMode::Checkpoints;
        d.anti_stall = true;
        let contacts: std::collections::BTreeSet<_> = d
            .move_sheet
            .iter()
            .filter(|(_, row)| row.contact)
            .map(|(key, _)| key.clone())
            .collect();
        if !gated {
            // Disable melee reach rejection without changing outcome classification or
            // projectile settlement, so both sides measure the same genuine attacks.
            for row in d.move_sheet.values_mut() {
                for window in &mut row.first_window {
                    window[1] = f32::MAX;
                }
                if row.first_reach.is_some() {
                    row.first_reach = Some(f32::MAX);
                }
            }
        }
        let mut first_brain_attack = None;
        let mut first_attack = None;
        for frame in 0..1800 {
            // Isolate ordinary brain slots; Lua reflexes have their own unchanged regressions.
            if let Some(hook) = duel.ai.as_mut().unwrap().state.act_hook.as_mut() {
                hook.reflex_acts.clear();
            }
            let mut input = InputFrame::default();
            // Three early presses qualify the fight, then a held guard avoids reflex-counter noise.
            input.buttons.guard = !matches!(frame, 1 | 3);
            // Observable scripted retreat, then a stationary guard, repeated every six seconds.
            if frame % 360 < 120 {
                duel.player.body.position[2] += 3.0 * DT;
            }
            let report = duel.step(&input, DT);
            if report.enemy.clip_started
                && report
                    .enemy
                    .animation
                    .as_deref()
                    .and_then(|name| name.strip_prefix("a000_"))
                    .and_then(|id| id.parse::<i32>().ok())
                    .is_some_and(|id| contacts.contains(&id.to_string()))
            {
                first_attack.get_or_insert(frame as f32 * DT);
            }
            let st = &duel.ai.as_ref().unwrap().state;
            if gated
                && (3000..3100).contains(&st.request.action)
                && st.goals.iter().flatten().any(|goal| {
                    goal.activated
                        && !goal.started_anim
                        && goal.plan_step.is_some()
                        && goal.params.first().and_then(|p| p.num())
                            == Some(f64::from(st.request.action))
                })
            {
                assert!(
                    st.act_hook
                        .as_ref()
                        .unwrap()
                        .reachable
                        .as_ref()
                        .unwrap()
                        .contains(&st.request.action),
                    "brain requested an unreachable attack at frame {frame}"
                );
                first_brain_attack.get_or_insert(frame as f32 * DT);
            }
        }
        if gated {
            assert!(
                first_brain_attack.is_some(),
                "live brain attack gate was not exercised"
            );
            assert!(
                first_attack.is_some_and(|time| time <= 6.0),
                "the boss must attack within the passive deadline: {first_attack:?}"
            );
        }
        let d = duel.boss_brain.as_mut().unwrap();
        d.end_fight(sekiro_sim::brain::memory::FightEnd::Unfinished);
        if gated {
            assert!(
                !d.memory.coach.contact_ranges.is_empty(),
                "real first-hit distance samples must survive the fight"
            );
            assert!(d.session_report().contains("first-hit hit / miss (m)"));
            assert!(
                d.coach_input(d.last_record.as_ref().unwrap())
                    .digest
                    .moves
                    .contains("hit/miss_m")
            );
        }
        let counts = d
            .memory
            .boss
            .iter()
            .filter(|(key, _)| {
                contacts.contains(*key)
                    && key
                        .parse::<i32>()
                        .is_ok_and(|id| (3000..3100).contains(&id))
            })
            .fold((0, 0), |(n, w), (_, c)| (n + c.n, w + c.whiffed));
        eprintln!(
            "gate scripted gated={gated}: n={} whiff={} rate={:.1}%",
            counts.0,
            counts.1,
            100.0 * counts.1 as f32 / counts.0.max(1) as f32
        );
        results.push(counts);
    }
    assert!(results[1].0 > 0, "gate must still attack");
    assert!(
        results[1].1 * results[0].0 * 10 <= results[0].1 * results[1].0 * 7,
        "{results:?}"
    );
}

#[test]
fn brain_bow_plan_fires_real_arrow_bullets() {
    let Some(mut duel) = load(8.0) else { return };
    let plan = Plan::new(&[3009, 3044]);
    duel.set_brain(
        Box::new(ScriptedBrain::new(plan.clone(), plan, u32::MAX, 0.0)),
        false,
    )
    .unwrap();
    let mut arrows = std::collections::BTreeSet::new();
    for _ in 0..1800 {
        duel.step(&InputFrame::default(), DT);
        for bullet in &duel.bullets.live {
            if bullet.look() == sekiro_sim::bullets::BulletLook::Missile {
                arrows.insert(bullet.def.id);
            }
        }
        actions(&mut duel);
    }
    eprintln!("brain arrows: {arrows:?}");
    assert!(arrows.contains(&71000500));
    assert!(arrows.contains(&71000530));
    assert!(duel.boss_brain.as_ref().unwrap().stats.planned > 0);
}

#[test]
fn lua_brain_preserves_genichiros_choices() {
    let Some(mut plain) = load(6.0) else { return };
    let mut hooked = load(6.0).unwrap();
    hooked.set_brain(Box::new(LuaBrain), false).unwrap();
    let (mut a, mut b) = (Vec::new(), Vec::new());
    for _ in 0..1800 {
        plain.step(&InputFrame::default(), DT);
        hooked.step(&InputFrame::default(), DT);
        a.extend(actions(&mut plain));
        b.extend(actions(&mut hooked));
    }
    assert!(a.len() >= 5);
    assert_eq!(a, b);
    assert_eq!(hooked.boss_brain.as_ref().unwrap().stats.planned, 0);
}

#[test]
fn choices_and_memory_are_character_specific() {
    assert_eq!(BossActions::for_chr("c7100").unwrap().chr, "c7100");
    assert!(BossActions::for_chr("c7110").unwrap().get(3039).is_some());
    for choice in GENICHIRO.choices() {
        assert!(GENICHIRO.parse(&choice.value).is_ok(), "{}", choice.value);
    }
    for id in [3020, 3021, 3039, 3042, 3043, 3033, 3047, 3060] {
        assert!(GENICHIRO.get(id).is_none());
    }
    let dir = std::env::temp_dir().join(format!(
        "sekiro-genichiro-brain-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let path =
        sekiro_sim::brain::memory::resolve_for(Some(dir.to_str().unwrap()), "c7100", "coach", true)
            .unwrap();
    let layers = sekiro_sim::brain::memory::LayerPaths::new(&path, "testwolf");
    assert_eq!(layers.boss, dir.join("c7100.boss.json"));
    assert_eq!(layers.player, dir.join("c7100.player-testwolf.json"));
    for chr in ["c1010", "c5060"] {
        let other = sekiro_sim::brain::memory::LayerPaths::new(
            &dir.join(format!("{chr}.json")),
            "testwolf",
        );
        assert_ne!(layers.boss, other.boss);
        assert_ne!(layers.player, other.player);
    }
    let Some(mut duel) = load(4.0) else { return };
    duel.set_brain(BrainKind::Scripted.build("c7100"), false)
        .unwrap();
    duel.boss_brain
        .as_mut()
        .unwrap()
        .use_player_memory(path, "testwolf");
    for _ in 0..960 {
        duel.step(&InputFrame::default(), DT);
    }
    drop(duel);
    assert!(layers.boss.exists());
    assert!(layers.player.exists());
    std::fs::remove_dir_all(&dir).unwrap();
}
#[test]
fn tomoe_handover_preserves_the_brain_and_reset_continues_numbering() {
    let Some(mut duel) = load(1.6) else { return };
    duel.set_brain(BrainKind::Scripted.build("c7100"), false)
        .unwrap();
    for _ in 0..150 {
        duel.step(&InputFrame::default(), DT);
    }
    take_mark(&mut duel);
    assert_eq!(duel.rules.fighters[1].vitality.deathblows_left, 1);
    assert!(duel.boss_brain.is_some());
    take_mark(&mut duel);
    for _ in 0..600 {
        duel.step(&InputFrame::default(), DT);
        if duel.spec.chr == "c7110" {
            break;
        }
    }
    assert_eq!(
        (duel.spec.chr.as_str(), duel.spec.npc_param),
        ("c7110", 71100000)
    );
    assert!(duel.boss_brain.is_some());
    assert_eq!(duel.boss_brain.as_ref().unwrap().overlay().fight, 1);
    assert_eq!(duel.boss_brain.as_ref().unwrap().stats.fights, 0);
    duel.step(&InputFrame::default(), DT);
    assert!(duel.ai.as_ref().unwrap().state.act_hook.is_some());
    assert!(
        duel.boss_brain
            .as_ref()
            .unwrap()
            .move_sheet
            .contains_key("3039")
    );
    // The headless duel starts at the origin, whereas the map hand-over uses
    // Tomoe's authored castle placement. Put idle Wolf in that fight's range.
    let p = duel.enemy.body.position;
    duel.player.body.position = [p[0], p[1], p[2] + 6.0];
    duel.enemy.body.yaw = std::f32::consts::PI;
    let mut attacks = 0;
    for _ in 0..1800 {
        duel.step(&InputFrame::default(), DT);
        attacks += actions(&mut duel)
            .iter()
            .filter(|id| (3000..3100).contains(*id))
            .count();
    }
    assert!(attacks >= 3, "Tomoe attacks: {attacks}");
    assert!(
        duel.boss_brain
            .as_ref()
            .unwrap()
            .history
            .iter()
            .any(|p| p.actions().contains(&3039))
    );
    duel.reset().unwrap();
    assert_eq!(duel.spec.chr, "c7100");
    assert_eq!(duel.boss_brain.as_ref().unwrap().overlay().fight, 2);
    let planned = duel.boss_brain.as_ref().unwrap().stats.planned;
    for _ in 0..600 {
        duel.step(&InputFrame::default(), DT);
    }
    assert!(duel.boss_brain.as_ref().unwrap().stats.planned > planned);
}

#[test]
fn final_deathblow_records_one_wolf_win_across_genichiro_phases() {
    let Some(mut duel) = load(1.6) else { return };
    assert!(duel.events.is_some(), "exercise the map event path");
    duel.set_brain(BrainKind::Scripted.build("c7100"), false)
        .unwrap();
    let dir = std::env::temp_dir().join(format!(
        "sekiro-wolf-win-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("c7100.json");
    let layers = sekiro_sim::brain::memory::LayerPaths::new(&path, "testwolf");
    duel.boss_brain
        .as_mut()
        .unwrap()
        .use_player_memory(path, "testwolf");
    let driver = duel.boss_brain.as_mut().unwrap();
    driver.enable_session_report(&dir);
    let log = driver.log_to(&dir).unwrap();
    for _ in 0..150 {
        duel.step(&InputFrame::default(), DT);
    }
    for _ in 0..2 {
        take_mark(&mut duel);
        let driver = duel.boss_brain.as_ref().unwrap();
        assert_eq!(driver.memory.wolf_wins, 0, "a phase is not a victory");
        assert!(driver.last_record.is_none());
    }
    for _ in 0..600 {
        duel.step(&InputFrame::default(), DT);
        if duel.spec.chr == "c7110" {
            break;
        }
    }
    assert_eq!(duel.spec.chr, "c7110");
    take_mark(&mut duel);
    for _ in 0..600 {
        duel.step(&InputFrame::default(), DT);
        if duel.boss_brain.as_ref().unwrap().last_record.is_some() {
            break;
        }
    }
    let driver = duel.boss_brain.as_ref().unwrap();
    assert_eq!(
        driver.memory.wolf_wins, 1,
        "final deathblow must record victory"
    );
    let record = driver.last_record.as_ref().unwrap();
    assert_eq!(record.winner, "wolf");
    assert!(
        !record.metrics.void,
        "a victory survives the sparse-input gate"
    );
    assert_eq!(record.metrics.kill_time, Some(record.metrics.duration));
    let input = driver.coach_input(record);
    assert_eq!(
        input.trend.kill_s.last(),
        Some(&Some(record.metrics.duration.round() as u32))
    );
    assert!(driver.session_report().contains("| 1 | wolf |"));
    assert!(driver.session_report().contains("Wolf kill time (s)"));
    driver.print_session_report().unwrap();
    assert!(
        record
            .summary_line()
            .contains(&format!("wolf_kill_time {:.1}s", record.metrics.duration))
    );
    let (_, saved) = layers.load("c7100").unwrap();
    assert_eq!(saved.wolf_wins, 1);
    assert_eq!(
        saved.history.last().unwrap().metrics.kill_time,
        record.metrics.kill_time
    );
    assert_eq!(
        saved.chr, "c7100",
        "Tomoe belongs to the castle fight memory"
    );
    for _ in 0..120 {
        duel.step(&InputFrame::default(), DT);
    }
    duel.boss_brain
        .as_mut()
        .unwrap()
        .end_fight(sekiro_sim::brain::memory::FightEnd::Unfinished);
    assert_eq!(duel.boss_brain.as_ref().unwrap().memory.fights, 1);
    let events: Vec<serde_json::Value> = std::fs::read_to_string(&log)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .filter(|row: &serde_json::Value| row["ev"] == "fight_summary")
        .collect();
    assert_eq!(events.len(), 1, "exactly one persisted fight summary");
    assert_eq!(events[0]["record"]["winner"], "wolf");
    assert!(
        events[0]["record"]["metrics"]["kill_time"]
            .as_f64()
            .unwrap()
            > 0.0
    );
    duel.reset().unwrap();
    assert_eq!(duel.spec.chr, "c7100");
    for i in 0..150 {
        duel.step(
            &InputFrame {
                buttons: sekiro_sim::Buttons {
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
    assert_eq!(
        (
            driver.memory.fights,
            driver.memory.wolf_wins,
            driver.memory.boss_wins
        ),
        (2, 1, 1)
    );
    let record = driver.last_record.as_ref().unwrap();
    assert_eq!(record.winner, "boss");
    assert_eq!(record.metrics.kill_time, None);
    assert_eq!(
        record.metrics.time_to_kill_wolf,
        Some(record.metrics.duration)
    );
    drop(duel);
    std::fs::remove_dir_all(dir).unwrap();
}

pub(super) fn take_mark(duel: &mut Duel) {
    take_mark_with_ai(duel, false);
}

pub(super) fn take_mark_with_ai(duel: &mut Duel, enemy_active: bool) {
    duel.enemy_active = enemy_active;
    for _ in 0..360 {
        duel.step(&InputFrame::default(), 1.0 / 60.0);
    }
    let p = duel.player.body.position;
    duel.player.body.yaw = 0.0;
    duel.enemy.body.position = [p[0], p[1], p[2] - 1.6];
    duel.enemy.body.yaw = std::f32::consts::PI;
    let mut broke = false;
    let mut started = false;
    for i in 0..1500 {
        // Owl can recover from his kneel before becoming throwable; break him again.
        if !started {
            broke &= duel.rules.deathblow_open(1);
        }
        if !broke {
            duel.rules.fighters[1].posture.remaining = 1;
        }
        let frame = InputFrame {
            // Follow the boss after contact stops Wolf rather than shoving the pair.
            move_stick: [0.0, if !broke && !started { 0.3 } else { 0.0 }],
            camera_yaw: duel.player.yaw_to(duel.enemy.body.position),
            camera_pitch: 0.0,
            buttons: sekiro_sim::Buttons {
                attack: if broke {
                    !started && i % 10 == 0
                } else {
                    i % 40 == 0
                },
                lock_on: (2..4).contains(&i),
                ..sekiro_sim::Buttons::default()
            },
        };
        let report = duel.step(&frame, 1.0 / 60.0);
        assert!(
            report.enemy.behavior.script_errors.is_empty(),
            "{:?}",
            report.enemy.behavior.script_errors
        );
        broke |= report
            .hits
            .iter()
            .any(|(h, r)| h.attacker == 0 && r.posture_broke);
        started |= report.deathblow.started.is_some();
        if started && duel.deathblow.is_none() {
            break;
        }
    }
    assert!(
        started,
        "deathblow not started for {}:{} (broke={broke}, marks={}, clip={:?}, open={}, Wolf={:?}, boss={:?})",
        duel.spec.chr,
        duel.spec.npc_param,
        duel.rules.fighters[1].vitality.deathblows_left,
        duel.enemy
            .behavior
            .runtime
            .main_clip()
            .map(|c| &c.animation),
        duel.deathblow_available(),
        duel.player.body.position,
        duel.enemy.body.position
    );
}
