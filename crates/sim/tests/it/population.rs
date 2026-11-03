use sekiro_sim::duel::{Duel, population::Population};
use sekiro_sim::{Buttons, InputFrame};

#[test]
fn population_lod_sleeps_throttles_and_wakes_for_combat() {
    use sekiro_sim::duel::population::Lod;
    for (distance, activation, engaged, expected) in [
        (5.0, 100.0, false, Lod::Full),
        (20.0, 100.0, false, Lod::Full),
        (50.0, 100.0, false, Lod::Reduced),
        (101.0, 100.0, false, Lod::Dormant),
        (11.0, 10.0, false, Lod::Dormant),
        (50.0, 100.0, true, Lod::Full),
        (101.0, 100.0, true, Lod::Full),
    ] {
        assert_eq!(Lod::for_distance(distance, activation, engaged), expected);
    }
    let dt = 1.0 / 60.0;
    let mut pending = 0.0;
    let mut elapsed = 0.0;
    let mut updates = 0;
    for _ in 0..60 {
        if let Some(step) = Lod::Reduced.step_dt(&mut pending, dt) {
            elapsed += step;
            updates += 1;
        }
    }
    assert_eq!(updates, 10);
    assert!((elapsed - 1.0_f32).abs() < 1e-5);
    assert!(Lod::Reduced.step_dt(&mut pending, dt).is_none());
    assert_eq!(
        Lod::Full.step_dt(&mut pending, dt),
        Some(dt),
        "combat wakes immediately without a backlog step"
    );
    for _ in 0..60 {
        assert!(Lod::Dormant.step_dt(&mut pending, dt).is_none());
    }
    assert_eq!(Lod::Full.step_dt(&mut pending, dt), Some(dt));
}

#[test]
fn outskirts_population_matches_source_msb_non_boss_parts() {
    let cache = sekiro_sim::cache::root();
    let path = cache.join("raw/map/MapStudio/m11_00_00_00.msb");
    if !path.exists() {
        return;
    }
    let msb = sekiro_formats::msb::parse(&std::fs::read(path).unwrap()).unwrap();
    let placements = Population::map_placements(&cache, "m11_00_00_00").unwrap();
    let mut expected = 0;
    for part in &msb.parts {
        if part.kind != sekiro_formats::msb::PartKind::Enemy {
            continue;
        }
        let model = &msb.models[part.model_index as usize].name;
        if !model.starts_with('c') || model == "c0000" {
            continue;
        }
        let enemy = part.enemy.as_ref().unwrap();
        // The reviewed Outskirts boss rows: mounted Gyoubu and Demon of Hatred.
        if matches!(
            (model.as_str(), enemy.npc_param_id),
            ("c5080", 50_800_000) | ("c7020", 70_200_000)
        ) {
            continue;
        }
        expected += 1;
        let (_, p) = placements
            .iter()
            .find(|(_, p)| p.part == part.name)
            .expect("every non-boss MSB Enemy part is present");
        assert_eq!(p.npc, enemy.npc_param_id);
        assert_eq!(p.think, enemy.think_param_id);
        assert_eq!(
            p.position,
            [-part.position[0], part.position[1], part.position[2]]
        );
        assert!((p.yaw + part.rotation[1].to_radians()).abs() < 0.0001);
    }
    assert_eq!(placements.len(), expected);
    eprintln!("Outskirts population: {expected} non-boss MSB enemies");
}

#[test]
fn first_visit_population_explains_every_excluded_actor() {
    let cache = sekiro_sim::cache::root();
    if !cache.join("raw/map/MapStudio/m11_00_00_00.msb").exists() {
        return;
    }
    let map = "m11_00_00_00";
    let state = sekiro_sim::world_events::evaluate(
        &cache,
        map,
        sekiro_sim::world_events::FirstVisit::for_map(map).unwrap(),
    )
    .unwrap();
    let (kept, excluded) = Population::starting_placements(&cache, map, Some(&state)).unwrap();
    assert_eq!(
        kept.len() + excluded.len(),
        Population::map_placements(&cache, map).unwrap().len()
    );
    assert!(kept.iter().any(|(s, _)| s.chr == "c1010"));
    for chr in [
        "c1000", "c1001", "c1012", "c5010", "c7010", "c7100", "c7200", "c7400", "c1700",
    ] {
        assert!(
            kept.iter().all(|(s, _)| s.chr != chr),
            "unexpected starting combatant {chr}"
        );
        assert!(
            excluded
                .iter()
                .any(|e| e.chr == chr && !e.reason.is_empty())
        );
    }
    let layout: serde_json::Value = serde_json::from_slice(
        &std::fs::read(cache.join(format!("maps/{map}/layout.json"))).unwrap(),
    )
    .unwrap();
    for disabled in state.parts(&layout).into_iter().filter(|p| !p.enabled) {
        assert!(kept.iter().all(|(_, p)| p.part != disabled.name));
    }
    eprintln!(
        "first-visit population: {} kept, {} excluded",
        kept.len(),
        excluded.len()
    );
}

#[test]
fn two_soldiers_keep_independent_hits_and_survive_one_death() {
    let cache = sekiro_sim::cache::root();
    if !cache.join("anim/c1010").is_dir() {
        return;
    }
    let spec = sekiro_sim::enemy::EnemySpec::soldier();
    let spot = |i: i32| sekiro_sim::registry::Placement {
        map: "m11_00_00_00".into(),
        part: format!("soldier_{i}"),
        entity: i,
        npc: spec.npc_param,
        think: spec.think,
        position: [i as f32 * 15.0, 0.0, -1.5],
        yaw: std::f32::consts::PI,
    };
    let mut wolf = Duel::load(&cache, 5.0).unwrap();
    wolf.god = true;
    let mut population = Population::load(
        &cache,
        vec![(spec.clone(), spot(0)), (spec.clone(), spot(1))],
    )
    .unwrap();
    assert!(population.foes.iter().all(|f| !f.duel.player.is_present()));
    let a = &population.foes[0].duel.enemy;
    let b = &population.foes[1].duel.enemy;
    assert!(std::sync::Arc::ptr_eq(&a.tae, &b.tae));
    assert!(std::sync::Arc::ptr_eq(&a.skeleton, &b.skeleton));
    assert!(std::ptr::eq(
        a.behavior.runtime.graph(),
        b.behavior.runtime.graph()
    ));
    assert!(std::sync::Arc::ptr_eq(
        &a.clips.clip("a000_000000").unwrap(),
        &b.clips.clip("a000_000000").unwrap()
    ));
    for foe in &mut population.foes {
        foe.duel.enemy_active = false;
        foe.duel.force_act = Some(3000);
    }
    wolf.set_population(population);
    let mut player_hits = [false; 2];
    let mut enemy_actions = [false; 2];
    let dt = 1.0 / 60.0;
    for phase in 0..2 {
        if phase == 1 {
            let p = wolf.population.as_mut().unwrap();
            let dead = &mut p.foes[0].duel.rules.fighters[1].vitality;
            dead.cannot_die = false;
            dead.deathblows_left = 0;
            dead.hp = 0;
            p.foes[1].duel.enemy.body.position = [0.0, 0.0, -1.5];
            wolf.player.body.position = [0.0; 3];
            wolf.player.snap_yaw(0.0);
        }
        for step in 0..480 {
            let report = wolf.step(
                &InputFrame {
                    buttons: Buttons {
                        attack: step % 45 < 20,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                dt,
            );
            assert!(
                !report.respawned,
                "one dead foe must not reload Wolf or the survivor"
            );
            assert!(wolf.respawn_in.is_none());
            for (hit, _) in report.hits {
                if hit.attacker == 0 {
                    player_hits[hit.defender - 1] = true;
                }
                assert!(hit.attacker <= 2 && hit.defender <= 2);
                if phase == 1 {
                    assert!(hit.attacker != 1 && hit.defender != 1);
                }
            }
            for (i, foe) in wolf.population.as_ref().unwrap().foes.iter().enumerate() {
                enemy_actions[i] |= foe
                    .report
                    .enemy
                    .animation
                    .as_ref()
                    .is_some_and(|a| a.contains("_003"));
            }
        }
        assert!(wolf.population.as_ref().unwrap().foes[phase].clock > 0.0);
    }
    assert_eq!(
        player_hits, [true; 2],
        "Wolf must hit each independent defender"
    );
    assert_eq!(
        enemy_actions, [true; 2],
        "each soldier must independently execute attacks"
    );
}

#[test]
fn a_dead_map_soldier_pays_its_npc_param_into_wolfs_kit_once() {
    let cache = sekiro_sim::cache::root();
    if !cache.join("anim/c1010").is_dir() {
        return;
    }
    let spec = sekiro_sim::enemy::EnemySpec::soldier();
    let spot = sekiro_sim::registry::Placement {
        map: "m11_00_00_00".into(),
        part: "soldier_0".into(),
        entity: 0,
        npc: spec.npc_param,
        think: spec.think,
        position: [0.0, 0.0, -3.0],
        yaw: std::f32::consts::PI,
    };
    let mut wolf = Duel::load(&cache, 5.0).unwrap();
    let mut population = Population::load(&cache, vec![(spec, spot)]).unwrap();
    population.foes[0].duel.enemy_active = false;
    let dead = &mut population.foes[0].duel.rules.fighters[1].vitality;
    dead.cannot_die = false;
    dead.deathblows_left = 0;
    dead.hp = 0;
    wolf.set_population(population);
    let start = wolf.kit.clone();
    for _ in 0..30 {
        wolf.step(&InputFrame::default(), 1.0 / 60.0);
    }
    // NpcParam 10100000 (Ochimusha, castle town): skillPoint 21, getSoul 11, paid once.
    assert_eq!(
        (wolf.kit.skill_xp - start.skill_xp, wolf.kit.sen - start.sen),
        (21, 11)
    );
    // Stood up again (as a rest does) and killed again, it pays a second time.
    fn soldier(wolf: &mut Duel) -> &mut sekiro_sim::combat::hp::Vitality {
        &mut wolf.population.as_mut().unwrap().foes[0]
            .duel
            .rules
            .fighters[1]
            .vitality
    }
    for hp in [soldier(&mut wolf).max_hp.max(1), 0] {
        soldier(&mut wolf).hp = hp;
        for _ in 0..5 {
            wolf.step(&InputFrame::default(), 1.0 / 60.0);
        }
    }
    assert_eq!(
        (wolf.kit.skill_xp - start.skill_xp, wolf.kit.sen - start.sen),
        (42, 22)
    );
}
