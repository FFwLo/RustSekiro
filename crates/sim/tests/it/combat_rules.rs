//! Combat rules driven by TAE events and params: invulnerability (TAE 950/951), knockback
//! (AtkParam distances, KnockBackParam timing, TAE 226), hit stop (AtkParam `hitStopTime`) and
//! posture recovery scaled by TAE 225. Skips when `cache/` has not been extracted.

use sekiro_sim::combat::knockback::{KnockbackClass, knockback_distance};
use sekiro_sim::duel::{Duel, DuelReport};
use sekiro_sim::fight::FighterKind;
use sekiro_sim::hits::can_be_hit;
use sekiro_sim::tae::{PlayingClip, TaeDb, TaeFrame, TaeRuntime};
use sekiro_sim::{Buttons, InputFrame};
use std::path::{Path, PathBuf};

const DT: f32 = 1.0 / 60.0;

#[test]
fn soldier_guard_return_matches_wolfs_real_attack_data() {
    let Some(cache) = cache() else { return };
    let mut duel = Duel::load(&cache, 1.6).unwrap();
    let mut deflects = 0;
    for i in 0..240 {
        let before = duel.rules.fighters[0].posture.remaining;
        let r = duel.step(
            &InputFrame {
                move_stick: [0.0, 1.0],
                buttons: Buttons {
                    attack: i % 8 == 0,
                    lock_on: i == 0,
                    ..Buttons::default()
                },
                ..InputFrame::default()
            },
            DT,
        );
        for (h, result) in &r.hits {
            if h.attacker == 0 && result.guarded {
                let atk = h.attack.profile.as_ref().unwrap();
                eprintln!(
                    "tick {i}: atk {} deflect={} return={} posture {} -> {}",
                    atk.id,
                    result.deflected,
                    result.attacker_posture_damage,
                    before,
                    duel.rules.fighters[0].posture.remaining
                );
                assert_eq!(atk.attacker_blocked_stam, 0);
                assert_eq!(atk.attacker_deflected_stam, 0);
                assert_eq!(result.attacker_posture_damage, 0);
                if result.deflected {
                    deflects += 1;
                    assert_eq!(duel.rules.fighters[0].posture.remaining, before);
                }
            }
        }
    }
    assert!(deflects > 0);
}

fn cache() -> Option<PathBuf> {
    let cache = sekiro_sim::cache::root();
    if !cache.join("raw/chr/c1010.behbnd.d/c9997.hkx").exists() {
        eprintln!("skipping: cache not extracted");
        return None;
    }
    Some(cache)
}

/// The TAE state of `anim` at `time` seconds.
fn frame_at(db: &TaeDb, anim: &str, time: f32) -> TaeFrame {
    let mut rt = TaeRuntime::default();
    let clip = PlayingClip {
        animation: anim.to_owned(),
        prev_time: (time - DT).max(0.0),
        time,
        looping: false,
    };
    rt.update(db, &[clip], DT);
    rt.frame
}

/// Wolf walks up to the idle soldier and slashes; the soldier stands still.
fn slash_input(i: usize) -> InputFrame {
    match i {
        0..=90 => InputFrame {
            move_stick: [0.0, 1.0],
            ..InputFrame::default()
        },
        120 => InputFrame {
            buttons: Buttons {
                attack: true,
                ..Buttons::default()
            },
            ..InputFrame::default()
        },
        _ => InputFrame::default(),
    }
}

fn idle_duel(cache: &Path) -> Duel {
    let mut duel = Duel::load(cache, 4.0).unwrap();
    duel.use_simple_ai();
    duel.enemy_active = false;
    duel
}

/// Runs the slash until Wolf's first hit lands; returns the step index and that step's report.
fn until_wolf_hit(duel: &mut Duel) -> (usize, DuelReport) {
    for i in 0..400 {
        let r = duel.step(&slash_input(i), DT);
        if r.hits.iter().any(|(h, _)| h.attacker == 0) {
            return (i, r);
        }
    }
    panic!("Wolf's slash never connected");
}

fn main_clip_name(duel: &Duel, enemy: bool) -> String {
    let c = if enemy { &duel.enemy } else { &duel.player };
    c.behavior
        .runtime
        .main_clip()
        .map_or(String::new(), |c| c.animation.clone())
}

fn main_clip_time(duel: &Duel, enemy: bool) -> f32 {
    let c = if enemy { &duel.enemy } else { &duel.player };
    c.behavior.runtime.main_clip().map_or(0.0, |c| c.time)
}

#[test]
fn knockdown_animations_are_invulnerable() {
    let Some(cache) = cache() else { return };
    let db = TaeDb::load_from_cache(&cache, "c0000");
    // a000_100400 (a knock-down) carries TAE 950 over its first 0.6 s.
    let down = frame_at(&db, "a000_100400", 0.3);
    assert!(down.invulnerable);
    assert!(!can_be_hit(&down));
    let after = frame_at(&db, "a000_100400", 0.7);
    assert!(can_be_hit(&after));
}

#[test]
fn invulnerable_defender_is_not_hit_and_the_window_is_not_spent() {
    let Some(cache) = cache() else { return };
    let (hit_at, _) = until_wolf_hit(&mut idle_duel(&cache));

    // Replay up to the step before the hit, then run the hit step's character updates by hand.
    let mut duel = idle_duel(&cache);
    for i in 0..hit_at {
        duel.step(&slash_input(i), DT);
    }
    duel.player.tick(&slash_input(hit_at), DT);
    duel.enemy.tick(&InputFrame::default(), DT);

    duel.enemy.tae_runtime.frame.invulnerable = true;
    let hits = duel.detector.detect(
        &[&duel.player, &duel.enemy],
        &[&duel.fighters[0], &duel.fighters[1]],
    );
    assert!(hits.iter().all(|h| h.attacker != 0), "{hits:?}");

    // The same window still lands once the invulnerability is gone.
    duel.enemy.tae_runtime.frame.invulnerable = false;
    let hits = duel.detector.detect(
        &[&duel.player, &duel.enemy],
        &[&duel.fighters[0], &duel.fighters[1]],
    );
    assert!(hits.iter().any(|h| h.attacker == 0 && h.defender == 1));
}

#[test]
fn knockback_rows_and_tae_scale() {
    let Some(cache) = cache() else { return };
    let duel = idle_duel(&cache);
    // Wolf's armour names KnockBackParam row 1 ("For PC"), NpcParam row 0 for the soldier.
    assert_eq!(duel.rules.fighters[0].knockback_row, 1);
    assert_eq!(duel.rules.fighters[1].knockback_row, 0);

    // Wolf's second combo slash (a050_300001) takes 50 % knockback around its hit frames.
    let db = TaeDb::load_from_cache(&cache, "c0000");
    let combo = frame_at(&db, "a050_300001", 0.8);
    assert_eq!(combo.knockback_scale, Some(0.5));
    assert!((knockback_distance(1.8, 0, combo.knockback_scale) - 0.9).abs() < 1e-6);
    assert_eq!(frame_at(&db, "a050_300001", 0.2).knockback_scale, None);

    assert_eq!(
        KnockbackClass::for_hit(sekiro_sim::combat::HitOutcome::Hit, 2),
        KnockbackClass::DamageM
    );
}

#[test]
fn wolf_hit_pushes_the_soldier_by_the_attack_distance() {
    let Some(cache) = cache() else { return };
    let mut duel = idle_duel(&cache);
    let (hit_at, report) = until_wolf_hit(&mut duel);
    let (hit, result) = report
        .hits
        .iter()
        .find(|(h, _)| h.attacker == 0)
        .unwrap()
        .clone();
    let atk = hit.attack.profile.as_ref().unwrap();
    assert!(!result.guarded);
    let kb = result.knockback.expect("a direct hit pushes");
    // The soldier cuts nothing (knockbackRate_vsPlayer_DirectHit 0) and plays no TAE 226.
    assert!(
        (kb.distance() - atk.knockback_dist[0]).abs() < 1e-4,
        "{} vs {}",
        kb.distance(),
        atk.knockback_dist[0]
    );
    // Timed by KnockBackParam row 0, along the hit.
    let class =
        KnockbackClass::for_hit(sekiro_sim::combat::HitOutcome::Hit, hit.attack.damage_level);
    let expected_span = kb.timing.cont + kb.timing.dec;
    assert!(expected_span > 0.0, "{class:?} has no timing");
    let dir = hit.direction;
    assert!((kb.direction[0] - dir[0]).abs() < 1e-5 && (kb.direction[1] - dir[1]).abs() < 1e-5);

    // Once the push has played out, the soldier has moved along the hit direction.
    let before = duel.enemy.body.position;
    let steps = ((expected_span + 0.2) / DT) as usize + 10;
    for i in 1..=steps {
        duel.step(&slash_input(hit_at + i), DT);
    }
    let after = duel.enemy.body.position;
    let along = (after[0] - before[0]) * dir[0] + (after[2] - before[2]) * dir[1];
    assert!(along > 0.5 * atk.knockback_dist[0], "moved {along}");
    assert!(duel.knockback[1].is_none());
}

#[test]
fn no_hit_stop_against_the_soldier() {
    let Some(cache) = cache() else { return };
    let mut duel = idle_duel(&cache);
    let FighterKind::Npc(npc) = &duel.rules.fighters[1].kind else {
        panic!("the enemy is an NPC")
    };
    // NpcParam 10100000: hitStopType 0 and hitStopType_Defencer 0 (the Owl's 50600000 too).
    assert_eq!((npc.hit_stop_type, npc.hit_stop_type_defender), (0, 0));
    let (hit_at, report) = until_wolf_hit(&mut duel);
    let (hit, result) = report.hits.iter().find(|(h, _)| h.attacker == 0).unwrap();
    let atk = hit.attack.profile.as_ref().unwrap();
    // Wolf's sword attacks carry 0.1 s for both sides, but the soldier's types scale it to 0.
    assert_eq!((atk.hit_stop_time, atk.hit_stop_time_defender), (0.1, 0.1));
    assert_eq!(result.hit_stop, [0.0, 0.0]);
    let before = main_clip_time(&duel, false);
    duel.step(&slash_input(hit_at + 1), DT);
    assert!((main_clip_time(&duel, false) - before - DT).abs() < 1e-5);
}

/// With the soldier's row set to the full types (as the Great Serpent's attacker type 2), both
/// clocks hold exactly still for 0.06 s, then ease back to full speed over 0.1 s.
#[test]
fn hit_stop_holds_both_clocks_then_eases_back() {
    let Some(cache) = cache() else { return };
    let mut duel = idle_duel(&cache);
    let FighterKind::Npc(npc) = &mut duel.rules.fighters[1].kind else {
        panic!("the enemy is an NPC")
    };
    npc.hit_stop_type = 2;
    npc.hit_stop_type_defender = 1;
    let (hit_at, report) = until_wolf_hit(&mut duel);
    let (_, result) = report.hits.iter().find(|(h, _)| h.attacker == 0).unwrap();
    assert_eq!(result.hit_stop, [0.1, 0.1]);

    let held = [main_clip_time(&duel, false), main_clip_time(&duel, true)];
    let pos = [duel.player.body.position, duel.enemy.body.position];
    let mut step = hit_at;
    // 0.06 s at 60 Hz: the step that crosses it is still held, four steps.
    for i in 1..=4 {
        step += 1;
        duel.step(&slash_input(step), DT);
        assert_eq!(main_clip_time(&duel, false), held[0], "Wolf moved at {i}");
        assert_eq!(main_clip_time(&duel, true), held[1], "soldier moved at {i}");
        assert_eq!(duel.player.body.position, pos[0], "Wolf moved at {i}");
        assert_eq!(duel.enemy.body.position, pos[1], "pushed during the hold");
    }
    // The ease: each step advances (k / 6)^2 of a step, then full steps.
    let mut last = held;
    let mut names = [main_clip_name(&duel, false), main_clip_name(&duel, true)];
    for k in 1..=8 {
        step += 1;
        duel.step(&slash_input(step), DT);
        let now = [main_clip_time(&duel, false), main_clip_time(&duel, true)];
        let now_names = [main_clip_name(&duel, false), main_clip_name(&duel, true)];
        let e = (k as f32 / 6.0).min(1.0);
        for side in 0..2 {
            let advanced = now[side] - last[side];
            // The soldier's reaction starts on his first running step; only check same-clip steps.
            if now_names[side] == names[side] && advanced >= 0.0 {
                assert!(
                    (advanced - e * e * DT).abs() < 1e-4,
                    "side {side} ease step {k}: advanced {advanced}, expected {}",
                    e * e * DT
                );
            }
        }
        last = now;
        names = now_names;
    }
    assert!(!duel.hit_stop[0].active() && !duel.hit_stop[1].active());
}

#[test]
fn posture_recovery_is_scaled_by_tae_225() {
    let Some(cache) = cache() else { return };
    let mut duel = idle_duel(&cache);
    let db = TaeDb::load_from_cache(&cache, "c1010");
    // The soldier's guard (a100_003100) recovers at 25 %.
    let guard = frame_at(&db, "a100_003100", 0.1);
    assert_eq!(guard.posture_regen_scale, Some(0.25));

    let rules = &mut duel.rules;
    rules.fighters[1].posture.remaining = 10;
    rules.recover_with_frame(1, &TaeFrame::default(), DT);
    let normal = rules.fighters[1].recovery_rate;
    assert!(normal > 0.0);
    rules.recover_with_frame(1, &guard, DT);
    let scaled = rules.fighters[1].recovery_rate;
    assert!(
        (scaled - normal * 0.25).abs() < 1e-4,
        "{scaled} vs {normal}"
    );

    // 0 % (a000_008200) stops recovery entirely.
    let stopped = frame_at(&db, "a000_008200", 1.0);
    assert_eq!(stopped.posture_regen_scale, Some(0.0));
    let before = rules.fighters[1].posture.remaining;
    for _ in 0..60 {
        rules.recover_with_frame(1, &stopped, DT);
    }
    assert_eq!(rules.fighters[1].posture.remaining, before);
    assert_eq!(rules.fighters[1].recovery_rate, 0.0);
}
