//! Wolf against the Great Shinobi Owl (c5060, NpcParam 50600000): his own HKS script, behaviour
//! graph, TAE, params and Lua battle AI (NpcThinkParam 50601000). Skips when `cache/` lacks him
//! (`sekiro-extract unpack`, then `anims c5060`).

use std::collections::BTreeMap;

use sekiro_sim::duel::{Duel, DuelReport};
use sekiro_sim::enemy::EnemySpec;
use sekiro_sim::lua_ai::AiEvent;
use sekiro_sim::{Buttons, InputFrame};

const DT: f32 = 1.0 / 60.0;

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

fn assert_no_script_errors(r: &DuelReport) {
    assert!(
        r.player.behavior.script_errors.is_empty(),
        "{:?}",
        r.player.behavior.script_errors
    );
    assert!(
        r.enemy.behavior.script_errors.is_empty(),
        "{:?}",
        r.enemy.behavior.script_errors
    );
}

/// Keeps Wolf alive and unbroken, so the Owl fights for the whole run.
fn keep_wolf_fresh(duel: &mut Duel) {
    let wolf = &mut duel.rules.fighters[0];
    wolf.vitality.hp = wolf.vitality.max_hp;
    wolf.posture.remaining = wolf.posture.max;
}

#[test]
fn owl_fights_a_passive_wolf_for_a_minute() {
    let Some(mut duel) = load(6.0) else { return };
    assert!(duel.ai.is_some(), "the Owl's AI did not load");
    let mut actions: BTreeMap<i32, usize> = BTreeMap::new();
    let mut unknown = Vec::new();
    let mut hits_on_wolf = 0;
    let mut owl_states: Vec<String> = Vec::new();
    for _ in 0..3600 {
        keep_wolf_fresh(&mut duel);
        let r = duel.step(&InputFrame::default(), DT);
        assert_no_script_errors(&r);
        hits_on_wolf += r.hits.iter().filter(|(h, _)| h.attacker == 1).count();
        let name = r.enemy.animation.clone().unwrap_or_default();
        if owl_states.last() != Some(&name) {
            owl_states.push(name);
        }
        for e in duel.ai.as_mut().unwrap().drain_log() {
            match e {
                AiEvent::Action(a) => *actions.entry(a).or_default() += 1,
                AiEvent::Unknown(name) => unknown.push(name),
                AiEvent::Error(e) => panic!("AI error: {e}"),
                _ => {}
            }
        }
    }
    let attacks: usize = actions
        .iter()
        .filter(|&(&a, _)| (3000..3100).contains(&a))
        .map(|(_, n)| n)
        .sum();
    eprintln!("Owl actions over 60 s ({attacks} attacks, {hits_on_wolf} hits on Wolf):");
    for (a, n) in &actions {
        eprintln!("  {a:>6}: {n}");
    }
    eprintln!("unknown engine calls: {unknown:?}");
    eprintln!("clips: {owl_states:?}");
    let distinct = actions.keys().filter(|a| (3000..3100).contains(*a)).count();
    assert!(
        distinct >= 4,
        "only {distinct} different attacks: {actions:?}"
    );
    assert!(attacks >= 15, "{actions:?}");
    assert!(hits_on_wolf >= 10, "{hits_on_wolf}");
}

#[test]
fn guards_and_deflects_work_both_ways() {
    let Some(mut duel) = load(3.0) else { return };
    // [Owl blocked, Owl deflected, Wolf blocked, Wolf deflected]
    let mut counts = [0; 4];
    for i in 0..3600 {
        keep_wolf_fresh(&mut duel);
        // Lock on, then cycle: hold guard, tap guard (short presses deflect when timed), then a
        // burst of slashes.
        let phase = i % 360;
        let guard = phase < 120 || (phase < 300 && phase % 10 < 3);
        let attack = phase >= 300 && phase % 20 == 0;
        let r = duel.step(&frame(guard, attack, (2..4).contains(&i)), DT);
        assert_no_script_errors(&r);
        for (h, res) in &r.hits {
            let k = if h.attacker == 0 { 0 } else { 2 };
            if res.deflected {
                counts[k + 1] += 1;
            } else if res.guarded {
                counts[k] += 1;
            }
        }
    }
    eprintln!(
        "Owl blocked {} and deflected {} of Wolf's slashes; Wolf blocked {} and deflected {}",
        counts[0], counts[1], counts[2], counts[3]
    );
    assert!(counts[0] + counts[1] > 0, "the Owl never guarded");
    assert!(counts[2] > 0, "Wolf never blocked the Owl");
    assert!(counts[3] > 0, "Wolf never deflected the Owl");
}

/// Wolf walks up to a passive Owl, breaks his posture with a slash on a nearly empty meter and
/// deathblows him; returns the deathblow's ThrowParam rows, whether it killed and the Owl's
/// behaviour states.
fn break_and_deathblow(duel: &mut Duel) -> (Option<(i32, i32)>, bool, Vec<String>) {
    let mut started = None;
    let mut killed = false;
    let mut broke = false;
    let mut owl_states: Vec<String> = Vec::new();
    for i in 0..1500 {
        keep_wolf_fresh(duel);
        if !broke {
            let owl = &mut duel.rules.fighters[1].posture;
            owl.remaining = owl.remaining.min(1);
        }
        // Slash every 40 steps until the posture breaks, then press attack for the deathblow.
        let attack = if broke {
            started.is_none() && i % 10 == 0
        } else {
            i % 40 == 0
        };
        let r = duel.step(&frame(false, attack, (2..4).contains(&i)), DT);
        assert_no_script_errors(&r);
        broke |= r
            .hits
            .iter()
            .any(|(h, res)| h.attacker == 0 && res.posture_broke);
        started = started.or(r.deathblow.started);
        killed |= r.deathblow.killed;
        let name = r
            .enemy
            .behavior
            .state_path
            .last()
            .cloned()
            .unwrap_or_default();
        if owl_states.last() != Some(&name) {
            owl_states.push(name);
        }
        // Done once the Owl is dead or back on his feet after the blow.
        if started.is_some() && duel.deathblow.is_none() && i % 60 == 0 {
            let owl = duel
                .enemy
                .behavior
                .runtime
                .main_clip()
                .map(|c| c.animation.clone());
            if killed || owl.is_some_and(|c| c.starts_with("a000_00") || c.starts_with("a000_4")) {
                break;
            }
        }
    }
    eprintln!("Owl states: {owl_states:?}");
    assert!(broke, "the Owl's posture never broke");
    (started, killed, owl_states)
}

#[test]
fn owl_takes_two_deathblows() {
    let Some(mut duel) = load(1.6) else { return };
    duel.enemy_active = false;
    // The animation data alone, without his map events: with them the first get-up hands over
    // to his phase-2 heat-up at 2.83 s, before the clip's own HP reset (`owl_phase2.rs`).
    duel.events = None;
    let owl = &duel.rules.fighters[1];
    assert_eq!(owl.vitality.max_hp, 1916);
    assert_eq!(owl.vitality.deathblows_left, 2);

    // The first mark: the ordinary front deathblow, after which he gets up (a000_012000 adds
    // SpEffect 300610 "HP reset" and StaminaControlParam type 7 refills his posture).
    let (rows, killed, states) = break_and_deathblow(&mut duel);
    assert_eq!(rows, Some((15_060_000, 15_060_001)));
    assert!(!killed, "the first deathblow killed");
    assert!(states.iter().any(|s| s == "ThrowDef12000"), "{states:?}");
    let owl = &duel.rules.fighters[1];
    assert_eq!(owl.vitality.deathblows_left, 1);
    assert_eq!(owl.vitality.hp, owl.vitality.max_hp, "HP not restored");
    assert_eq!(
        owl.posture.remaining, owl.posture.max,
        "posture not restored"
    );

    // The last mark: the finishing ("todome") pair, whose defender clip is his death.
    let (rows, killed, states) = break_and_deathblow(&mut duel);
    assert_eq!(rows, Some((15_060_180, 15_060_181)));
    assert!(killed, "the second deathblow did not kill");
    assert!(states.iter().any(|s| s == "ThrowDef13700"), "{states:?}");
    assert!(duel.rules.fighters[1].dead());
}

#[test]
fn owl_throws_shuriken_from_range() {
    let Some(mut duel) = load(12.0) else { return };
    let mut bullet_hits = Vec::new();
    let mut fired = 0;
    for i in 0..1800 {
        keep_wolf_fresh(&mut duel);
        // Wolf backs off now and then, so the Owl keeps choosing his ranged acts.
        let back = i % 300 < 120;
        let input = InputFrame {
            move_stick: [0.0, if back { -1.0 } else { 0.0 }],
            ..InputFrame::default()
        };
        let before = duel.bullets.live.len();
        let r = duel.step(&input, DT);
        assert_no_script_errors(&r);
        fired += duel.bullets.live.len().saturating_sub(before);
        for (h, res) in &r.hits {
            if h.attacker == 1 && duel.fighters[1].bullets.contains_key(&h.judge) {
                bullet_hits.push((i, h.judge, h.attack.atk_id, res.hp_damage, res.guarded));
            }
        }
    }
    eprintln!("bullets fired {fired}, hits {bullet_hits:?}");
    assert!(
        bullet_hits
            .iter()
            .any(|&(_, _, atk, hp, _)| atk == 50_600_150 && hp > 0),
        "no shuriken hit Wolf"
    );
}
