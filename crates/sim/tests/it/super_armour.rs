//! Installed-data interruption regressions using the shipped NPC scripts and graphs.
use sekiro_sim::{InputFrame, duel::Duel, enemy::EnemySpec, hits::HitEvent};
const DT: f32 = 1.0 / 60.0;
fn load(chr: &str, row: i32) -> Option<Duel> {
    let cache = sekiro_sim::cache::root();
    if !cache.join(format!("anim/{chr}/skeleton.bin")).exists() {
        return None;
    }
    let spec = EnemySpec::from_cache(&cache, &format!("{chr}:{row}")).unwrap();
    let mut duel = Duel::load_with(&cache, 1.6, spec).unwrap();
    duel.use_simple_ai();
    Some(duel)
}
fn start(duel: &mut Duel, act: i32, time: f32) {
    duel.enemy.npc.action = act;
    for _ in 0..180 {
        let r = duel.enemy.tick(&InputFrame::default(), DT);
        if r.animation.as_deref() == Some(&format!("a000_{act:06}")) && r.anim_time >= time {
            duel.enemy.npc.action = 0;
            return;
        }
    }
    panic!("act {act} did not reach {time}: {}", duel.enemy.state());
}
fn slash(duel: &mut Duel) -> sekiro_sim::hits::HitResult {
    let attack = duel.fighters[0]
        .attacks
        .values()
        .find(|a| a.atk_id == 5000010)
        .unwrap()
        .clone();
    let hit = HitEvent {
        attacker: 0,
        defender: 1,
        judge: 10,
        attack,
        position: duel.enemy.body.position,
        direction: [0.0, 1.0],
    };
    let result = duel.rules.resolve(
        &hit,
        [&duel.player, &duel.enemy],
        [&duel.fighters[0], &duel.fighters[1]],
    );
    duel.enemy.pending_damage = Some(result.defender);
    result
}
#[test]
fn genichiro_jump_keeps_clip_and_strike_after_damaging_hit() {
    let Some(mut duel) = load("c7100", 71001000) else {
        return;
    };
    start(&mut duel, 3026, 1.5);
    assert!(duel.enemy.authored_air_height() > 0.0);
    let hp = duel.rules.fighters[1].vitality.hp;
    let hit = slash(&mut duel);
    assert!(hit.hp_damage > 0 && hit.posture_damage > 0);
    assert!(duel.rules.fighters[1].vitality.hp < hp);
    let r = duel.enemy.tick(&InputFrame::default(), DT);
    assert_eq!(r.animation.as_deref(), Some("a000_003026"));
    assert!(!duel.enemy.behavior.env.landed);
    let mut strike = false;
    for _ in 0..50 {
        duel.enemy.tick(&InputFrame::default(), DT);
        strike |= !duel.enemy.tae_frame().attacks.is_empty();
    }
    assert!(strike, "the original attack must reach its damage window");
}

#[test]
fn diving_slash_aerial_alias_is_a_legitimate_interrupt() {
    let Some(mut duel) = load("c7100", 71001000) else {
        return;
    };
    start(&mut duel, 3015, 0.5);
    assert!(duel.enemy.clips.clip("a000_008141").is_some());
    let floor = duel.enemy.body.position[1];
    slash(&mut duel);
    let r = duel.enemy.tick(&InputFrame::default(), DT);
    assert_eq!(r.animation.as_deref(), Some("a000_008141"));
    assert!(!r.grounded);
    assert!(r.position[1] > floor + 0.15);
    for _ in 0..180 {
        duel.enemy.tick(&InputFrame::default(), DT);
    }
    assert!(duel.enemy.body.grounded);
    assert!((duel.enemy.body.position[1] - floor).abs() < 0.01);
}
#[test]
fn ordinary_ground_slashes_remain_interruptible() {
    for (chr, row) in [
        ("c7100", 71001000),
        ("c5060", 50600000),
        ("c1010", 10100000),
    ] {
        let Some(mut duel) = load(chr, row) else {
            return;
        };
        start(&mut duel, 3000, 0.25);
        let hit = slash(&mut duel);
        let r = duel.enemy.tick(&InputFrame::default(), DT);
        eprintln!(
            "{chr}: level {} state {} clip {:?}",
            hit.defender.level,
            duel.enemy.state(),
            r.animation
        );
        assert_ne!(r.animation.as_deref(), Some("a000_003000"));
    }
}
#[test]
fn perilous_replacement_keeps_damage_and_attack() {
    let Some(mut duel) = load("c7100", 71001000) else {
        return;
    };
    start(&mut duel, 3025, 0.4);
    let hit = slash(&mut duel);
    assert_eq!(hit.defender.level, 8);
    assert!(hit.hp_damage > 0 && hit.posture_damage > 0);
    let r = duel.enemy.tick(&InputFrame::default(), DT);
    assert_eq!(r.animation.as_deref(), Some("a000_003025"));
}

#[test]
fn owl_and_soldier_committed_windows_keep_their_attacks() {
    for (chr, row, act, time) in [
        ("c5060", 50600000, 3003, 0.5),
        ("c1010", 10100000, 3010, 0.4),
    ] {
        let Some(mut duel) = load(chr, row) else {
            return;
        };
        start(&mut duel, act, time);
        let hit = slash(&mut duel);
        assert_eq!(hit.defender.level, 8);
        assert!(hit.hp_damage > 0 && hit.posture_damage > 0);
        let r = duel.enemy.tick(&InputFrame::default(), DT);
        assert_eq!(
            r.animation.as_deref(),
            Some(format!("a000_{act:06}").as_str())
        );
    }
}

#[test]
fn aerial_posture_break_retains_height_and_lands() {
    let Some(mut duel) = load("c7100", 71001000) else {
        return;
    };
    start(&mut duel, 3015, 0.5);
    let floor = duel.enemy.body.position[1];
    assert!(duel.enemy.authored_air_height() > 0.0);
    duel.enemy.pending_damage = Some(sekiro_sim::player::IncomingDamage {
        damage_type: 1027,
        level: 2,
        ..Default::default()
    });
    let r = duel.enemy.tick(&InputFrame::default(), DT);
    assert_ne!(r.animation.as_deref(), Some("a000_003015"));
    assert!(!r.grounded);
    assert!(r.position[1] > floor + 0.15);
    for _ in 0..180 {
        duel.enemy.tick(&InputFrame::default(), DT);
    }
    assert!(duel.enemy.body.grounded);
    assert!((duel.enemy.body.position[1] - floor).abs() < 0.01);
}
