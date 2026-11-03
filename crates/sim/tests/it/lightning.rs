use sekiro_sim::{Buttons, InputFrame, brain::BrainKind, duel::Duel, registry::Registry};

fn tomoe() -> Option<Duel> {
    let cache = sekiro_sim::cache::root();
    if !cache.join("anim/c7110/skeleton.bin").exists() {
        return None;
    }
    let spec = Registry::load(&cache)
        .unwrap()
        .resolve("c7110:71100000")
        .unwrap();
    let mut duel = Duel::load_with(&cache, 6.0, spec).unwrap();
    duel.set_brain(BrainKind::Repeat(3039).build("c7110"), false)
        .unwrap();
    duel.god = true;
    duel.step(
        &InputFrame {
            buttons: Buttons {
                lock_on: true,
                ..Default::default()
            },
            ..Default::default()
        },
        1.0 / 60.0,
    );
    Some(duel)
}

fn airborne_capture(duel: &mut Duel) {
    let mut jumped = false;
    for _ in 0..900 {
        let jump = !jumped
            && duel
                .enemy
                .behavior
                .runtime
                .main_clip()
                .is_some_and(|c| c.animation == "a000_003039" && c.time > 1.7);
        jumped |= jump;
        let report = duel.step(
            &InputFrame {
                buttons: Buttons {
                    jump,
                    ..Default::default()
                },
                ..Default::default()
            },
            1.0 / 60.0,
        );
        assert!(report.player.behavior.script_errors.is_empty());
        if duel.rules.fighters[0].status.has(9492) && !duel.player.body.grounded {
            duel.enemy_active = false;
            return;
        }
    }
    panic!("Wolf did not capture the bolt airborne");
}

#[test]
fn airborne_capture_without_attack_discharges_on_landing() {
    let Some(mut duel) = tomoe() else {
        return;
    };
    airborne_capture(&mut duel);
    assert!(!duel.rules.fighters[0].status.has(9445));
    for _ in 0..180 {
        let r = duel.step(&InputFrame::default(), 1.0 / 60.0);
        assert!(r.player.behavior.script_errors.is_empty());
        if duel.rules.fighters[0].status.has(9445) {
            return;
        }
    }
    panic!("landing did not electrify Wolf");
}

#[test]
fn airborne_attack_reverses_lightning() {
    let Some(mut duel) = tomoe() else {
        return;
    };
    airborne_capture(&mut duel);
    let hp = duel.rules.fighters[1].vitality.hp;
    let mut reversal = false;
    for tick in 0..180 {
        let r = duel.step(
            &InputFrame {
                buttons: Buttons {
                    attack: tick % 2 == 0,
                    ..Default::default()
                },
                ..Default::default()
            },
            1.0 / 60.0,
        );
        assert!(r.player.behavior.script_errors.is_empty());
        reversal |= r
            .player
            .animation
            .as_deref()
            .is_some_and(|s| s == "a050_308910" || s == "a050_308911");
        if duel.rules.fighters[1].status.has(9445) {
            assert!(reversal);
            assert!(duel.rules.fighters[1].vitality.hp < hp);
            assert!(!duel.rules.fighters[0].status.has(9492));
            return;
        }
    }
    assert!(reversal, "HKS did not select reversal");
    panic!("reversal did not hit Genichiro");
}

#[test]
fn guarded_lightning_still_electrifies() {
    let Some(mut duel) = tomoe() else {
        return;
    };
    let mut guarded = false;
    for _ in 0..900 {
        let r = duel.step(
            &InputFrame {
                buttons: Buttons {
                    guard: true,
                    ..Default::default()
                },
                ..Default::default()
            },
            1.0 / 60.0,
        );
        for (hit, result) in &r.hits {
            if hit.attack.atk_id == 71100521 {
                assert!(result.attacker.is_none());
                assert_eq!(result.attacker_posture_damage, 0);
                assert!(duel.enemy.pending_damage.is_none());
            }
        }
        guarded |= r
            .hits
            .iter()
            .any(|(h, result)| h.attack.atk_id == 71100521 && result.guarded);
        if duel.rules.fighters[0].status.has(9445) {
            assert!(guarded);
            return;
        }
    }
    panic!("guard prevented electric shock");
}

#[test]
fn lightning_bullet_reaches_beyond_melee_range() {
    let Some(duel) = tomoe() else {
        return;
    };
    let row = &duel.boss_brain.as_ref().unwrap().move_sheet["3039"];
    assert!(
        !row.projectiles.is_empty(),
        "lightning bullet missing from move sheet: {row:?}"
    );
    assert!(
        row.fits([6.0, 0.0], [0.0; 2]),
        "lightning bullet cannot reach Wolf: {row:?}"
    );
}

#[test]
fn grounded_lightning_electrifies_wolf() {
    let Some(mut duel) = tomoe() else {
        return;
    };
    let mut bolt = false;
    for _ in 0..900 {
        duel.step(&InputFrame::default(), 1.0 / 60.0);
        bolt |= duel.bullets.live.iter().any(|b| b.def.id == 71100521);
        if duel.rules.fighters[0].status.has(9445) {
            assert!(bolt);
            return;
        }
    }
    assert!(bolt, "lightning act did not fire its data bullet");
    panic!("grounded Wolf never became electrified");
}

#[test]
fn lightning_slashes_use_their_melee_attack() {
    for act in [3042, 3043] {
        let Some(mut duel) = tomoe() else {
            return;
        };
        duel.set_brain(BrainKind::Repeat(act).build("c7110"), false)
            .unwrap();
        let mut contact = false;
        for _ in 0..1200 {
            let r = duel.step(&InputFrame::default(), 1.0 / 60.0);
            contact |= r.hits.iter().any(|(h, _)| h.attack.atk_id == 71100361);
            if duel.rules.fighters[0].status.has(9445) {
                break;
            }
        }
        assert!(contact, "act {act} never hit with lightning slash 71100361");
        assert!(duel.rules.fighters[0].status.has(9445));
    }
}
