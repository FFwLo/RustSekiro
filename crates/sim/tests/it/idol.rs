use sekiro_sim::InputFrame;
use sekiro_sim::deathblow::Placement;
use sekiro_sim::duel::idol::{
    Destination, IdolPhase, IdolSpot, LIGHTING_SECS, REST_END, REST_RESET, RestClock, WARP_CUT,
    rest_dark,
};
use sekiro_sim::duel::{Duel, RESPAWN_DELAY};

const DT: f32 = 1.0 / 60.0;

/// Steps with `input` until `done`, at most `seconds`; the phases passed on the way.
fn run_with(
    duel: &mut Duel,
    input: &InputFrame,
    seconds: f32,
    done: impl Fn(&Duel) -> bool,
) -> Vec<Option<IdolPhase>> {
    let mut seen = vec![duel.idol_phase()];
    for _ in 0..(seconds / DT) as usize {
        duel.step(input, DT);
        if seen.last() != Some(&duel.idol_phase()) {
            seen.push(duel.idol_phase());
        }
        if done(duel) {
            return seen;
        }
    }
    panic!("not done in {seconds} s: {seen:?}");
}

fn run(duel: &mut Duel, seconds: f32, done: impl Fn(&Duel) -> bool) -> Vec<Option<IdolPhase>> {
    run_with(duel, &InputFrame::default(), seconds, done)
}

#[test]
fn wolf_kneels_rests_stands_and_comes_back_at_the_idol() {
    let cache = sekiro_sim::cache::root();
    if !cache.join("anim/c0000/a000_710310.bin").exists() {
        eprintln!("skipping: cache not extracted");
        return;
    }
    let mut duel = Duel::load(&cache, 30.0).unwrap();
    duel.enemy_present = false;
    duel.enemy_active = false;
    run(&mut duel, 1.0, |d| d.player.body.grounded);
    let start = duel.player.body.position;
    let spot = IdolSpot {
        kneel: Placement {
            position: [start[0] + 1.0, start[1], start[2]],
            yaw: 1.0,
        },
        checkpoint: Placement {
            position: [start[0] + 1.0, start[1], start[2] + 0.5],
            yaw: -2.0,
        },
        entity: 1101950,
    };
    duel.set_idols(vec![spot]);
    assert_eq!(duel.idol_in_reach(), Some(0));

    // The action press kneels him at the spot with full Vitality; holding it does nothing more.
    duel.rules.fighters[0].vitality.hp = 10;
    let mut press = InputFrame::default();
    press.buttons.action = true;
    duel.step(&press, DT);
    assert_eq!(duel.idol(), Some(0));
    assert_eq!(duel.idol_in_reach(), None);
    let [x, _, z] = duel.player.body.position;
    assert!((x - spot.kneel.position[0]).hypot(z - spot.kneel.position[2]) < 0.05);
    let max = duel.rules.fighters[0].vitality.max_hp;
    assert_eq!(duel.rules.fighters[0].vitality.hp, max);
    // The first kneel lights the idol: Wolf stays held while its "found" banner plays, and
    // the menu's Rest and Esc do nothing until it is over.
    assert_eq!(duel.idol_phase(), Some(IdolPhase::Lighting));
    assert_eq!(duel.idol_lighting(), Some(0.0));
    duel.rest_at_idol();
    duel.leave_idol();

    let praying = |d: &Duel| d.idol_phase() == Some(IdolPhase::Praying);
    // His own input is held while he is at the idol: the stick does not walk him away.
    let mut walk = press;
    walk.move_stick = [0.0, 1.0];
    // The banner's whole clip, to the frame.
    for _ in 0..(LIGHTING_SECS / DT) as usize - 2 {
        duel.step(&walk, DT);
        assert_eq!(duel.idol_phase(), Some(IdolPhase::Lighting));
    }
    let seen = run_with(&mut duel, &walk, 4.0 * DT, praying);
    assert_eq!(seen, [Some(IdolPhase::Lighting), Some(IdolPhase::Praying)]);
    assert_eq!(duel.idol_lighting(), None);
    let [x, _, z] = duel.player.body.position;
    assert!((x - spot.kneel.position[0]).hypot(z - spot.kneel.position[2]) < 0.05);
    duel.rules.fighters[0].vitality.hp = 10;
    duel.rest_at_idol();
    // Only once: the second press waits for the first.
    duel.rest_at_idol();
    let seen = run(&mut duel, 5.0, praying);
    assert_eq!(
        seen,
        [
            Some(IdolPhase::Praying),
            Some(IdolPhase::Resting),
            Some(IdolPhase::Praying)
        ]
    );
    assert_eq!(duel.rules.fighters[0].vitality.hp, max);
    duel.leave_idol();
    let seen = run(&mut duel, 3.0, |d| d.idol_phase().is_none());
    assert_eq!(
        seen,
        [Some(IdolPhase::Praying), Some(IdolPhase::Leaving), None]
    );
    duel.step(&InputFrame::default(), DT);
    assert_eq!(duel.idol(), None);

    // A death brings Wolf back at the idol, not at the start.
    duel.rules.fighters[0].vitality.hp = 0;
    run(&mut duel, RESPAWN_DELAY + 1.0, |d| {
        d.respawn_in.is_none() && !d.rules.fighters[0].dead()
    });
    assert_eq!(duel.player.body.position, spot.checkpoint.position);

    // It stays lit through the respawn: kneeling there again goes straight to the prayer.
    run(&mut duel, 2.0, |d| d.idol_in_reach() == Some(0));
    duel.step(&press, DT);
    assert_eq!(duel.idol(), Some(0));
    let seen = run(&mut duel, 3.0, praying);
    assert_eq!(seen, [Some(IdolPhase::Kneeling), Some(IdolPhase::Praying)]);
}

#[test]
fn the_nearer_item_takes_the_press_and_stays_taken_while_rest_and_death_restock_it() {
    let cache = sekiro_sim::cache::root();
    if !cache.join("anim/c0000/a000_710310.bin").exists()
        || !cache.join("raw/map/mapstudio/m11_00_00_00.msb").exists()
    {
        eprintln!("skipping: cache not extracted");
        return;
    }
    let data = std::sync::Arc::new(sekiro_sim::items::ItemData::load(&cache).unwrap());
    let mut pellets = sekiro_sim::items::map_pickups(&cache, "m11_00_00_00", &data)
        .unwrap()
        .into_iter()
        .find(|p| p.name == "Item_宝_02")
        .unwrap();
    assert_eq!((pellets.lot.id, pellets.lot.flag), (1100020, 51100020));
    let mut duel = Duel::load(&cache, 30.0).unwrap();
    duel.enemy_present = false;
    duel.enemy_active = false;
    // The start kit carries the Pellets' limit of 3.
    assert_eq!(duel.kit.count(3020), 3);
    run(&mut duel, 1.0, |d| d.player.body.grounded);
    let start = duel.player.body.position;
    // The Pellets half a metre off, an idol's kneeling spot a metre off.
    pellets.position = [start[0] + 0.5, start[1], start[2]];
    duel.set_pickups(vec![pellets], data);
    duel.set_idols(vec![IdolSpot {
        kneel: Placement {
            position: [start[0] + 1.0, start[1], start[2]],
            yaw: 1.0,
        },
        checkpoint: Placement {
            position: [start[0] + 1.0, start[1], start[2] + 0.5],
            yaw: -2.0,
        },
        entity: 1101950,
    }]);
    assert_eq!(duel.pickup_in_reach(), Some(0));
    assert_eq!(duel.idol_in_reach(), None);

    let mut press = InputFrame::default();
    press.buttons.action = true;
    duel.step(&press, DT);
    // Both Pellets are over the limit: they go to the Storage.
    assert_eq!((duel.kit.count(3020), duel.kit.stored(3020)), (3, 2));
    assert!(duel.kit.taken_flags.contains(&51100020));
    assert_eq!(duel.idol(), None);
    // One used, then a Rest restocks it from the Storage (#186).
    assert!(duel.kit.use_goods(3020));
    // Taken, it leaves the press to the idol; a Rest does not bring it back.
    duel.step(&InputFrame::default(), DT);
    duel.step(&press, DT);
    assert_eq!(duel.idol(), Some(0));
    let praying = |d: &Duel| d.idol_phase() == Some(IdolPhase::Praying);
    run(&mut duel, 5.0, praying);
    assert_eq!(duel.kit.count(3020), 2, "kneeling alone does not restock");
    duel.rest_at_idol();
    run(&mut duel, 0.5, |d| {
        d.idol_phase() == Some(IdolPhase::Resting)
    });
    run(&mut duel, 5.0, praying);
    assert!(!duel.pickup_lies(0));
    assert_eq!((duel.kit.count(3020), duel.kit.stored(3020)), (3, 1));

    // A respawn after death restocks the last one and refills the gourd, as Rest does.
    duel.leave_idol();
    run(&mut duel, 3.0, |d| d.idol_phase().is_none());
    assert!(duel.kit.use_goods(3020) && duel.kit.use_goods(3000));
    duel.rules.fighters[0].vitality.hp = 0;
    run(&mut duel, RESPAWN_DELAY + 1.0, |d| {
        d.respawn_in.is_none() && !d.rules.fighters[0].dead()
    });
    assert_eq!((duel.kit.count(3020), duel.kit.stored(3020)), (3, 0));
    assert_eq!(duel.kit.gourd_charges, duel.kit.gourd_max);
}

#[test]
fn a_rest_records_the_idol_and_travel_puts_wolf_at_the_other_idols_warp_point() {
    let cache = sekiro_sim::cache::root();
    if !cache.join("anim/c0000/a000_710360.bin").exists() {
        eprintln!("skipping: cache not extracted");
        return;
    }
    let mut duel = Duel::load(&cache, 30.0).unwrap();
    duel.enemy_present = false;
    duel.enemy_active = false;
    run(&mut duel, 1.0, |d| d.player.body.grounded);
    let start = duel.player.body.position;
    let spot = |dx: f32, entity: i32| IdolSpot {
        kneel: Placement {
            position: [start[0] + dx, start[1], start[2]],
            yaw: 1.0,
        },
        checkpoint: Placement {
            position: [start[0] + dx, start[1], start[2] + 3.0],
            yaw: -2.0,
        },
        entity,
    };
    let (here, there) = (spot(1.0, 1101951), spot(20.0, 1101952));
    duel.set_idols(vec![here, there]);

    // Kneeling at the first records it found (lit), the other not yet.
    assert!(!duel.idol_lit(0) && !duel.idol_lit(1));
    let mut press = InputFrame::default();
    press.buttons.action = true;
    duel.step(&press, DT);
    assert_eq!(duel.idol(), Some(0));
    assert!(duel.idol_lit(0) && !duel.idol_lit(1));
    assert_eq!(
        duel.kit.found_idols.iter().copied().collect::<Vec<_>>(),
        [1101951]
    );
    let praying = |d: &Duel| d.idol_phase() == Some(IdolPhase::Praying);
    run(&mut duel, LIGHTING_SECS + 1.0, praying);

    // Not to the idol he is at; to the other the warp clip plays, then he arrives at its warp
    // point facing its way, whole, and it is the checkpoint.
    assert!(!duel.travel_to_idol(Destination::Here(0)));
    duel.rules.fighters[0].vitality.hp = 10;
    assert!(duel.travel_to_idol(Destination::Here(1)));
    run(&mut duel, 0.5, |d| {
        d.idol_phase() == Some(IdolPhase::Traveling)
    });
    run(&mut duel, WARP_CUT + 0.5, |d| d.idol_travel().is_none());
    assert_eq!((duel.respawn_in, duel.idol()), (None, None));
    assert_eq!(duel.player.body.position, there.checkpoint.position);
    assert!((duel.player.body.yaw - there.checkpoint.yaw).abs() < 1e-4);
    let max = duel.rules.fighters[0].vitality.max_hp;
    assert_eq!(duel.rules.fighters[0].vitality.hp, max);
    duel.rules.fighters[0].vitality.hp = 0;
    run(&mut duel, RESPAWN_DELAY + 1.0, |d| {
        d.respawn_in.is_none() && !d.rules.fighters[0].dead()
    });
    assert_eq!(duel.player.body.position, there.checkpoint.position);

    // Travel to another map's idol hands it to the game once, at the cut.
    run(&mut duel, 1.0, |d| d.player.body.grounded);
    duel.player.body.position = here.kneel.position;
    duel.step(&InputFrame::default(), DT);
    duel.step(&press, DT);
    run(&mut duel, 3.0, praying);
    assert!(duel.travel_to_idol(Destination::Away(1111950)));
    run(&mut duel, WARP_CUT + 0.5, |d| d.idol_travel().is_none());
    assert_eq!(duel.take_departure(), Some(1111950));
    assert_eq!(duel.take_departure(), None);
}

/// A duel without a foe, Wolf on the ground praying at the first of two idols (entities
/// 1101951 and 1101952) 19 m apart; `None` when the cache is not extracted.
fn praying_at_one_of_two_idols(brain: bool) -> Option<(Duel, [IdolSpot; 2])> {
    let cache = sekiro_sim::cache::root();
    if !cache.join("anim/c0000/a000_710360.bin").exists() {
        eprintln!("skipping: cache not extracted");
        return None;
    }
    let mut duel = Duel::load(&cache, 30.0).unwrap();
    if brain {
        duel.set_brain(Box::new(sekiro_sim::brain::LuaBrain), false)
            .unwrap();
        assert!(duel.boss_brain.is_some());
    }
    duel.enemy_present = false;
    duel.enemy_active = false;
    run(&mut duel, 1.0, |d| d.player.body.grounded);
    let start = duel.player.body.position;
    let spot = |dx: f32, entity: i32| IdolSpot {
        kneel: Placement {
            position: [start[0] + dx, start[1], start[2]],
            yaw: 1.0,
        },
        checkpoint: Placement {
            position: [start[0] + dx, start[1], start[2] + 3.0],
            yaw: -2.0,
        },
        entity,
    };
    let spots = [spot(1.0, 1101951), spot(20.0, 1101952)];
    duel.set_idols(spots.to_vec());
    let mut press = InputFrame::default();
    press.buttons.action = true;
    duel.step(&press, DT);
    assert_eq!(duel.idol(), Some(0));
    run(&mut duel, LIGHTING_SECS + 1.0, |d| {
        d.idol_phase() == Some(IdolPhase::Praying)
    });
    Some((duel, spots))
}

/// Rest (#153): the world resets only once the burst's dark is full, the dark has lifted while
/// Wolf still rests, and a rest cut short before the dark resets nothing.
#[test]
fn rest_resets_the_world_only_under_the_full_dark_and_a_cut_rest_resets_nothing() {
    let Some((mut duel, _)) = praying_at_one_of_two_idols(false) else {
        return;
    };
    let max = duel.rules.fighters[0].vitality.max_hp;
    duel.rules.fighters[0].vitality.hp = 10;
    duel.kit.gourd_charges = 0;
    duel.rest_at_idol();
    assert_eq!(
        duel.idol_rest(),
        Some(RestClock {
            secs: 0.0,
            reset: false
        })
    );
    let mut reset_at = None;
    while let Some(was) = duel.idol_rest() {
        duel.step(&InputFrame::default(), DT);
        let refilled = duel.rules.fighters[0].vitality.hp == max;
        assert_eq!(refilled, duel.kit.gourd_charges == duel.kit.gourd_max);
        match (reset_at, duel.idol_rest()) {
            (None, Some(now)) if refilled => {
                assert!(now.reset && !was.reset);
                assert!(now.secs >= REST_RESET && now.secs < REST_RESET + DT);
                assert_eq!(
                    rest_dark(now.secs),
                    1.0,
                    "reset at {} s, not dark",
                    now.secs
                );
                reset_at = Some(now.secs);
            }
            (None, Some(now)) => assert!(now.secs < REST_RESET, "no reset at {} s", now.secs),
            (None, None) => panic!("the rest ended without a reset"),
            (Some(_), _) => assert!(refilled),
        }
        assert_eq!(duel.idol_phase(), Some(IdolPhase::Resting));
    }
    assert!(reset_at.is_some());
    // The clock stops as the dark has lifted, with Wolf still resting.
    assert_eq!(rest_dark(REST_END), 0.0);
    run(&mut duel, 2.0, |d| {
        d.idol_phase() == Some(IdolPhase::Praying)
    });

    // Cut short: Wolf is taken off the idol mid-burst; the clock stops and nothing is reset.
    duel.rules.fighters[0].vitality.hp = 10;
    duel.kit.gourd_charges = 0;
    duel.rest_at_idol();
    run(&mut duel, 0.3, |d| {
        d.idol_rest().is_some_and(|r| r.secs >= 0.3)
    });
    duel.set_idols(duel.idol_spots().to_vec());
    assert_eq!(duel.idol_rest(), None);
    run(&mut duel, 1.0, |d| {
        d.idol_rest().is_none() && d.respawn_in.is_none()
    });
    for _ in 0..(REST_END / DT) as usize {
        duel.step(&InputFrame::default(), DT);
    }
    assert_eq!(duel.rules.fighters[0].vitality.hp, 10);
    assert_eq!(duel.kit.gourd_charges, 0);
}

#[test]
fn travel_to_another_map_departs_with_the_kit_restocked_as_a_rest_restocks_it() {
    let Some((mut duel, _)) = praying_at_one_of_two_idols(false) else {
        return;
    };
    // Two gourd uses spent and a hurt Wolf: the kit the next run gets is a rested one.
    duel.kit.gourd_charges -= 2;
    duel.rules.fighters[0].vitality.hp = 10;
    assert!(duel.travel_to_idol(Destination::Away(1111950)));
    // Nothing is given before he leaves.
    duel.step(&InputFrame::default(), DT);
    assert_eq!(duel.kit.gourd_charges, duel.kit.gourd_max - 2);
    run(&mut duel, WARP_CUT + 0.5, |d| d.idol_travel().is_none());
    assert_eq!(duel.take_departure(), Some(1111950));
    assert_eq!(duel.kit.gourd_charges, duel.kit.gourd_max);
    let v = duel.rules.fighters[0].vitality;
    assert_eq!(v.hp, v.max_hp);
}

#[test]
fn travel_within_the_play_world_ends_no_fight() {
    let Some((mut duel, [_, there])) = praying_at_one_of_two_idols(true) else {
        return;
    };
    let fights = |d: &Duel| d.boss_brain.as_ref().unwrap().stats.fights;
    assert_eq!(fights(&duel), 0);
    assert!(duel.travel_to_idol(Destination::Here(1)));
    run(&mut duel, WARP_CUT + 0.5, |d| d.idol_travel().is_none());
    assert_eq!(duel.respawn_in, None);
    assert_eq!(duel.player.body.position, there.checkpoint.position);
    // The arrival's respawn reset the world without settling a fight as Wolf's win.
    assert_eq!(fights(&duel), 0);
    for _ in 0..30 {
        duel.step(&InputFrame::default(), DT);
    }
    assert_eq!(fights(&duel), 0);
    // A death still ends the fight.
    duel.rules.fighters[0].vitality.hp = 0;
    run(&mut duel, RESPAWN_DELAY + 1.0, |d| {
        d.respawn_in.is_none() && !d.rules.fighters[0].dead()
    });
    assert_eq!(fights(&duel), 1);
}

#[test]
fn rest_travel_and_death_leave_the_storage_alone_while_every_pellet_is_carried() {
    let Some((mut duel, [_, there])) = praying_at_one_of_two_idols(false) else {
        return;
    };
    let cache = sekiro_sim::cache::root();
    let data = std::sync::Arc::new(sekiro_sim::items::ItemData::load(&cache).unwrap());
    // The Pellets' row: three carried, the rest stored, restocked on Rest.
    let pellet = data.rule(3020);
    assert_eq!(
        (pellet.carry, pellet.store, pellet.restock),
        (3, 9999, true)
    );
    duel.set_pickups(Vec::new(), data);
    // The start kit carries all three, two more in the Storage (m11_00's pair, #202).
    assert_eq!(duel.kit.count(3020), pellet.carry);
    duel.kit.storage.insert(3020, 2);
    let pellets = |d: &Duel| (d.kit.count(3020), d.kit.stored(3020));
    let praying = |d: &Duel| d.idol_phase() == Some(IdolPhase::Praying);

    // Rest with the gourd drunk: it refills, the Pellets stay as they were.
    duel.kit.gourd_charges = 0;
    let before = duel.kit.clone();
    duel.rest_at_idol();
    run(&mut duel, 5.0, |d| {
        d.idol_phase() == Some(IdolPhase::Resting)
    });
    run(&mut duel, 5.0, praying);
    assert_eq!(duel.kit.gourd_charges, duel.kit.gourd_max);
    assert_eq!(pellets(&duel), (3, 2));
    // Besides the gourd, only the start kit's one goods 3800 changes: a full-supply goods, it
    // refills to its maxNum of 3 (#208).
    let mut rested = before.clone();
    rested.gourd_charges = before.gourd_max;
    for (_, n) in rested.goods.iter_mut().filter(|(g, _)| *g == 3800) {
        *n = 3;
    }
    assert_eq!(rested, duel.kit);

    // One used: the next Rest moves exactly one from the Storage, and a Travel after it
    // (which restocks again, then respawns him there) moves no more.
    assert!(duel.kit.use_goods(3020));
    duel.rest_at_idol();
    run(&mut duel, 5.0, |d| {
        d.idol_phase() == Some(IdolPhase::Resting)
    });
    run(&mut duel, 5.0, praying);
    assert_eq!(pellets(&duel), (3, 1));
    assert!(duel.travel_to_idol(Destination::Here(1)));
    run(&mut duel, WARP_CUT + 0.5, |d| d.idol_travel().is_none());
    assert_eq!(duel.player.body.position, there.checkpoint.position);
    assert_eq!(pellets(&duel), (3, 1));

    // A death: the respawn restocks as Rest does, nothing to move.
    duel.rules.fighters[0].vitality.hp = 0;
    run(&mut duel, RESPAWN_DELAY + 1.0, |d| {
        d.respawn_in.is_none() && !d.rules.fighters[0].dead()
    });
    assert_eq!(pellets(&duel), (3, 1));
}

#[test]
fn rest_and_death_refill_the_full_supply_goods_without_the_storage() {
    let Some((mut duel, _)) = praying_at_one_of_two_idols(false) else {
        return;
    };
    let cache = sekiro_sim::cache::root();
    let data = std::sync::Arc::new(sekiro_sim::items::ItemData::load(&cache).unwrap());
    // Every isFullSuppleItem row: carried up to maxNum, no Storage (isDeposit 0).
    let full: Vec<_> = data
        .goods
        .iter()
        .filter(|(_, r)| r.refill)
        .map(|(id, r)| (*id, r.carry, r.store))
        .collect();
    assert_eq!(
        full,
        [
            (3000, 10, 0),
            (3300, 5, 0),
            (3310, 5, 0),
            (3320, 5, 0),
            (3580, 5, 0),
            (3800, 3, 0)
        ]
    );
    duel.set_pickups(Vec::new(), data);
    let rice = |d: &Duel| (d.kit.count(3580), d.kit.stored(3580));

    // All five Five-color Rice thrown and the gourd drunk: Rest refills both, the gourd to
    // its gourd_max rather than its row's maxNum of 10.
    for _ in 0..5 {
        assert!(duel.kit.use_goods(3580));
    }
    duel.kit.gourd_charges = 0;
    duel.rest_at_idol();
    run(&mut duel, 5.0, |d| {
        d.idol_phase() == Some(IdolPhase::Resting)
    });
    run(&mut duel, 5.0, |d| {
        d.idol_phase() == Some(IdolPhase::Praying)
    });
    assert_eq!(rice(&duel), (5, 0));
    assert_eq!(duel.kit.count(3800), 3);
    assert_eq!(duel.kit.gourd_charges, duel.kit.gourd_max);

    // Two thrown, then a death: the respawn refills them as Rest does.
    assert!(duel.kit.use_goods(3580) && duel.kit.use_goods(3580));
    duel.rules.fighters[0].vitality.hp = 0;
    run(&mut duel, RESPAWN_DELAY + 1.0, |d| {
        d.respawn_in.is_none() && !d.rules.fighters[0].dead()
    });
    assert_eq!(rice(&duel), (5, 0));
    assert!(duel.kit.storage.is_empty());
}
