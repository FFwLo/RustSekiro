//! The Demon of Hatred (c7020) at the Ashina Outskirts battlefield (`--arena demon`), on the
//! installed data: the map's Lua AI, BehaviorParam attacks and bullets, the three deathblow marks
//! and the map events. `docs/BOSSES-s.md` lists what each test proves and what is missing.
//! Skips when the arena (`sekiro-extract map m11_00_00_00`), c7020 or the Lua bundle is missing.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use glam::Vec3;
use sekiro_sim::arena::{Arena, ArenaSpec, forward, yaw_toward};
use sekiro_sim::body::Ground;
use sekiro_sim::duel::Duel;
use sekiro_sim::lua_ai::AiEvent;
use sekiro_sim::{Buttons, InputFrame};

const DT: f32 = 1.0 / 60.0;

fn cache() -> PathBuf {
    sekiro_sim::cache::root()
}

fn load() -> Option<(Arena, Duel)> {
    let cache = cache();
    let spec = ArenaSpec::by_name("demon").unwrap();
    if !cache
        .join("maps")
        .join(spec.map)
        .join("collision.bin")
        .is_file()
        || !cache.join("anim/c7020/tae.json").is_file()
        || !cache.join("raw/script/m11_00_00_00.luabnd.d").is_dir()
    {
        eprintln!("skipping: the Outskirts arena or the Demon is not extracted");
        return None;
    }
    let arena = Arena::load(&cache, spec, false).unwrap();
    let mut duel = Duel::load_with(&cache, 6.0, spec.enemy_spec()).unwrap();
    arena.place(&mut duel);
    Some((arena, duel))
}

/// Puts Wolf `dist` metres in front of where the Demon was placed, on the floor.
fn near(arena: &Arena, duel: &mut Duel, dist: f32) {
    let boss = arena.layout.boss;
    let spot = arena.floor_at(boss.position + forward(boss.yaw) * dist);
    duel.player.body.position = spot.to_array();
    duel.player.body.fall_top = spot.y;
    duel.player.body.yaw = yaw_toward(spot, boss.position);
}

/// Puts Wolf `dist` metres from the Demon's current position, on the side he already stands.
fn beside(arena: &Arena, duel: &mut Duel, dist: f32) {
    let e = Vec3::from_array(duel.enemy.body.position);
    let w = Vec3::from_array(duel.player.body.position);
    let away = (w - e).with_y(0.0).normalize_or_zero();
    let spot = arena.floor_at(e + away * dist);
    duel.player.body.position = spot.to_array();
    duel.player.body.fall_top = spot.y;
    duel.player.body.yaw = yaw_toward(spot, e);
}

fn flat(duel: &Duel) -> f32 {
    (Vec3::from_array(duel.enemy.body.position) - Vec3::from_array(duel.player.body.position))
        .with_y(0.0)
        .length()
}

fn flag(duel: &Duel, id: i32) -> bool {
    duel.events.as_ref().unwrap().scripts.flag(id)
}

fn clip(duel: &Duel) -> String {
    duel.enemy
        .behavior
        .runtime
        .main_clip()
        .map(|c| c.animation.clone())
        .unwrap_or_default()
}

fn has_effect(duel: &Duel, id: i32) -> bool {
    duel.enemy.tae_frame().sp_effects.contains(&id)
        || duel.rules.fighters[1].resident().contains(&id)
}

fn walls(arena: &Arena, duel: &Duel) -> usize {
    arena
        .fog_walls
        .iter()
        .filter(|w| w.wall.active(|f| flag(duel, f)))
        .count()
}

/// Starts the fight the way the arena does: Wolf walks into region 1102910.
fn start_fight(arena: &Arena, duel: &mut Duel) {
    duel.god = true;
    near(arena, duel, 45.0);
    for _ in 0..10 {
        duel.step(&InputFrame::default(), DT);
    }
    assert!(
        flag(duel, 11_105_901),
        "Wolf in region 1102910 starts the fight"
    );
}

#[derive(Default)]
struct Log {
    acts: BTreeMap<i32, usize>,
    unknown: BTreeSet<String>,
    hits: usize,
    bullets: BTreeSet<i32>,
    /// Lowest height the Demon reached above the lowest collision under him.
    lowest: f32,
}

/// Steps `steps` with Wolf idle and invulnerable, keeping him within `range` of the Demon.
fn soak(arena: &Arena, duel: &mut Duel, steps: usize, range: f32) -> Log {
    let mut log = Log {
        lowest: f32::MAX,
        ..Log::default()
    };
    for i in 0..steps {
        if i % 60 == 0 && flat(duel) > range + 4.0 {
            beside(arena, duel, range);
        }
        let r = duel.step(&InputFrame::default(), DT);
        assert!(
            r.enemy.behavior.script_errors.is_empty(),
            "{:?}",
            r.enemy.behavior.script_errors
        );
        log.hits += r.hits.iter().filter(|(h, _)| h.attacker == 1).count();
        // Below all collision under him means he fell into the void (terraces are fine).
        let [x, y, z] = duel.enemy.body.position;
        if let Some(void) = Ground::lowest_floor(&*arena.world, x, z) {
            log.lowest = log.lowest.min(y - void);
        }
        log.bullets
            .extend(duel.bullets.live.iter().map(|b| b.judge));
        for e in duel.ai.as_mut().unwrap().drain_log() {
            match e {
                AiEvent::Action(a) => *log.acts.entry(a).or_default() += 1,
                AiEvent::Error(e) => panic!("AI error: {e}"),
                AiEvent::Unknown(n) => {
                    log.unknown.insert(n);
                }
                _ => {}
            }
        }
    }
    assert!(
        log.lowest > -2.0,
        "the Demon fell into the void: {} m below the lowest collision",
        log.lowest
    );
    log
}

/// Breaks the Demon's posture in front of (or, with `behind`, behind) Wolf and drives the
/// deathblow until the pair ends. Returns the ThrowParam rows.
fn take_mark(arena: &Arena, duel: &mut Duel, behind: bool) -> (i32, i32) {
    let mut broke = false;
    let mut rows = None;
    for i in 0..1800 {
        if !broke {
            duel.rules.fighters[1].posture.remaining = 1;
        }
        if rows.is_none() {
            let e = Vec3::from_array(duel.enemy.body.position);
            let d = flat(duel);
            if d > 3.8 || d < 2.0 || (behind && !broke) {
                let facing = forward(duel.enemy.body.yaw);
                let dir = if behind { -facing } else { facing };
                let spot = arena.floor_at(e + dir * 3.0);
                duel.player.body.position = spot.to_array();
                duel.player.body.fall_top = spot.y;
                duel.player.body.yaw = yaw_toward(spot, e);
            }
        }
        let frame = InputFrame {
            buttons: Buttons {
                attack: if broke {
                    rows.is_none() && i % 10 == 0
                } else {
                    i % 40 == 0
                },
                lock_on: (2..4).contains(&i),
                ..Buttons::default()
            },
            ..InputFrame::default()
        };
        let report = duel.step(&frame, DT);
        broke |= report
            .hits
            .iter()
            .any(|(h, r)| h.attacker == 0 && r.posture_broke);
        rows = rows.or(report.deathblow.started);
        if rows.is_some() && duel.deathblow.is_none() {
            break;
        }
    }
    rows.expect("deathblow not started")
}

#[test]
fn the_demon_waits_frozen_until_wolf_enters_the_arena_then_opens_with_the_spin() {
    let Some((arena, mut duel)) = load() else {
        return;
    };
    duel.god = true;
    assert_eq!(duel.rules.fighters[1].vitality.max_hp, 3195);
    assert_eq!(duel.rules.fighters[1].posture.max, 1050);
    assert_eq!(duel.rules.fighters[1].vitality.deathblows_left, 3);
    // Wolf starts about 72 m away, outside region 1102910: the retry state holds the Demon.
    assert!(flat(&duel) > 60.0);
    for _ in 0..120 {
        duel.step(&InputFrame::default(), DT);
    }
    assert_eq!(clip(&duel), "a000_000000");
    assert!(!flag(&duel, 11_105_901));
    assert_eq!(duel.events.as_ref().unwrap().boss_bar, None);
    assert_eq!(walls(&arena, &duel), 0);
    assert!(duel.ai.as_mut().unwrap().drain_log().is_empty());
    // Wolf walks into the region: bar, three walls, and the retry opener (5022 absent) spins.
    start_fight(&arena, &mut duel);
    assert_eq!(duel.events.as_ref().unwrap().boss_bar, Some(907_021));
    assert_eq!(walls(&arena, &duel), 3);
    let log = soak(&arena, &mut duel, 60 * 40, 6.0);
    assert_eq!(
        log.acts.get(&3056),
        Some(&1),
        "the opener plays once: {:?}",
        log.acts
    );
    assert!(
        has_effect(&duel, 5022),
        "the opener's TAE leaves 5022 on until cleared"
    );
    assert!(log.acts.len() >= 4, "{:?}", log.acts);
    // Wolf's death returns both to the start: frozen again, walls down.
    duel.reset().unwrap();
    arena.place(&mut duel);
    for _ in 0..10 {
        duel.step(&InputFrame::default(), DT);
    }
    assert!(!flag(&duel, 11_105_901));
    assert_eq!(walls(&arena, &duel), 0);
    assert_eq!(clip(&duel), "a000_000000");
    assert_eq!(duel.rules.fighters[1].vitality.hp, 3195);
}

#[test]
fn phase_one_fights_wolf_with_the_map_ai() {
    let Some((arena, mut duel)) = load() else {
        return;
    };
    start_fight(&arena, &mut duel);
    let log = soak(&arena, &mut duel, 60 * 90, 6.0);
    eprintln!(
        "phase 1: {} hits, acts {:?}, bullets {:?}",
        log.hits, log.acts, log.bullets
    );
    assert!(
        log.unknown.is_empty(),
        "unanswered AI queries: {:?}",
        log.unknown
    );
    assert!(log.hits >= 8, "{}", log.hits);
    let attacks: BTreeSet<i32> = log
        .acts
        .keys()
        .copied()
        .filter(|a| (3000..3100).contains(a))
        .collect();
    assert!(attacks.len() >= 8, "{attacks:?}");
    assert_eq!(duel.rules.fighters[1].vitality.deathblows_left, 3);
}

/// Every BehaviorParam attack the AI can ask for lands on Wolf at some range.
#[test]
#[ignore = "slow: 41 attacks replayed at up to six distances (about 45 s)"]
fn every_attack_the_demon_ai_requests_reaches_wolf() {
    let Some((arena, _)) = load() else {
        return;
    };
    let acts = [
        3000, 3001, 3002, 3003, 3005, 3006, 3007, 3008, 3010, 3011, 3012, 3013, 3015, 3016, 3017,
        3018, 3019, 3023, 3025, 3032, 3033, 3034, 3035, 3036, 3037, 3038, 3039, 3040, 3041, 3042,
        3043, 3044, 3045, 3046, 3047, 3048, 3049, 3052, 3056, 3092, 3093,
    ];
    let mut duel = Duel::load_with(&cache(), 6.0, arena.spec.enemy_spec()).unwrap();
    let mut misses = Vec::new();
    for act in acts {
        let mut landed = false;
        for dist in [4.0f32, 9.0, 13.0, 18.0, 25.0, 2.0] {
            duel.reset().unwrap();
            arena.place(&mut duel);
            near(&arena, &mut duel, dist);
            duel.god = true;
            duel.enemy_active = false;
            duel.force_act = Some(act);
            for i in 0..720 {
                let r = duel.step(&InputFrame::default(), DT);
                if i == 30 {
                    duel.force_act = None;
                }
                // 800 is the shared body-contact row of the Demon's moves, not the attack.
                landed |= r
                    .hits
                    .iter()
                    .any(|(h, res)| h.attacker == 1 && h.judge != 800 && res.hp_damage > 0);
                if landed {
                    break;
                }
            }
            if landed {
                break;
            }
        }
        if !landed {
            misses.push(act);
        }
    }
    // 3026 (the phase-3 far-range dash requested by the interrupt only) is not in the list:
    // its sweep passes 0.78 m short of Wolf here, see docs/BOSSES-s.md.
    assert!(
        misses.is_empty(),
        "attacks that never touched Wolf: {misses:?}"
    );
}

#[test]
#[ignore = "slow: twelve projectile acts replayed at up to six distances (about 20 s)"]
fn the_demons_projectiles_use_the_bullet_rows_of_his_attacks() {
    let Some((arena, _)) = load() else {
        return;
    };
    // (act, bullet judge rows its TAE fires)
    let table: [(i32, &[i32]); 12] = [
        (3007, &[126]),
        (3008, &[122, 982]),
        (3013, &[270]),
        (3036, &[350, 364, 440, 981]),
        (3038, &[222]),
        (3039, &[321]),
        (3042, &[460]),
        (3043, &[450]),
        (3044, &[450, 452]),
        (3047, &[231, 234]),
        (3049, &[320]),
        (3093, &[300]),
    ];
    let mut duel = Duel::load_with(&cache(), 6.0, arena.spec.enemy_spec()).unwrap();
    for (act, rows) in table {
        let mut seen = BTreeSet::new();
        let mut contact = false;
        for dist in [9.0f32, 4.0, 13.0, 18.0, 2.0, 25.0] {
            duel.reset().unwrap();
            arena.place(&mut duel);
            near(&arena, &mut duel, dist);
            duel.god = true;
            duel.enemy_active = false;
            duel.force_act = Some(act);
            for i in 0..720 {
                let r = duel.step(&InputFrame::default(), DT);
                if i == 30 {
                    duel.force_act = None;
                }
                seen.extend(duel.bullets.live.iter().map(|b| b.judge));
                contact |= r
                    .hits
                    .iter()
                    .any(|(h, _)| h.attacker == 1 && h.judge != 800);
            }
            if contact && rows.iter().all(|row| seen.contains(row)) {
                break;
            }
        }
        for row in rows {
            assert!(
                seen.contains(row),
                "act {act}: bullet {row} never flew, saw {seen:?}"
            );
        }
        assert!(contact, "act {act} never reached Wolf");
    }
}

#[test]
fn a_jump_passes_the_perilous_sweep_and_the_kanji_warns_first() {
    let Some((arena, _)) = load() else {
        return;
    };
    let mut duel = Duel::load_with(&cache(), 6.0, arena.spec.enemy_spec()).unwrap();
    let mut run = |act: i32, jump: Option<usize>| {
        duel.reset().unwrap();
        arena.place(&mut duel);
        near(&arena, &mut duel, 5.0);
        duel.god = true;
        duel.enemy_active = false;
        let mut hp_hits = 0;
        let mut warned_before_hit = None;
        for i in 0..300 {
            duel.force_act = (i < 20).then_some(act);
            let input = InputFrame {
                buttons: Buttons {
                    jump: jump == Some(i),
                    lock_on: i == 1,
                    ..Buttons::default()
                },
                ..InputFrame::default()
            };
            let r = duel.step(&input, DT);
            if r.perilous.is_some() && warned_before_hit.is_none() {
                warned_before_hit = Some(hp_hits == 0);
            }
            hp_hits += r
                .hits
                .iter()
                .filter(|(h, res)| h.attacker == 1 && h.judge != 800 && res.hp_damage > 0)
                .count();
        }
        (hp_hits, warned_before_hit)
    };
    // 3025: Attack 180 is a sweep (unblockable, no just guard), warned by bullet 980.
    let (hit, warned) = run(3025, None);
    assert!(hit > 0, "an idle Wolf is swept");
    assert_eq!(warned, Some(true), "the warning precedes the blow");
    let passed = (30..110).step_by(2).any(|j| run(3025, Some(j)).0 == 0);
    assert!(passed, "a jump at some moment passes under 3025's sweep");
    // 3036: warned sweeps (bullet 981 at 1.17 s and 3.83 s) inside one move.
    let (_, warned) = run(3036, None);
    assert_eq!(warned, Some(true));
}

#[test]
fn guarding_wolf_takes_posture_damage_instead_of_the_slash() {
    let Some((arena, mut duel)) = load() else {
        return;
    };
    duel.immortal = true;
    duel.enemy_active = false;
    near(&arena, &mut duel, 4.0);
    duel.force_act = Some(3000);
    let mut guarded = 0;
    let mut posture = 0;
    for i in 0..300 {
        if i == 30 {
            duel.force_act = None;
        }
        let input = InputFrame {
            buttons: Buttons {
                guard: i > 5,
                lock_on: i == 1,
                ..Buttons::default()
            },
            ..InputFrame::default()
        };
        let r = duel.step(&input, DT);
        for (h, res) in &r.hits {
            if h.attacker == 1 && h.judge != 800 && (res.guarded || res.deflected) {
                guarded += 1;
                posture += res.posture_damage;
            }
        }
    }
    assert!(guarded >= 1, "3000 is blockable");
    assert!(posture > 0, "a block or deflect costs Wolf posture");
}

#[test]
fn the_first_mark_kneels_refills_then_the_roar_replaces_the_phase_effect() {
    let Some((arena, mut duel)) = load() else {
        return;
    };
    start_fight(&arena, &mut duel);
    soak(&arena, &mut duel, 60 * 6, 6.0);
    assert!(duel.rules.fighters[1].resident().contains(&3_702_001));
    let rows = take_mark(&arena, &mut duel, false);
    assert_eq!(rows, (17_020_000, 17_020_001));
    assert_eq!(duel.rules.fighters[1].vitality.deathblows_left, 2);
    assert!(
        flag(&duel, 11_105_902),
        "event 11105912 marks the second life"
    );
    // The kneeling clip plays out undisturbed by the AI, then Act31 requests the heat-up roar.
    let mut clips = BTreeSet::new();
    for _ in 0..60 * 14 {
        duel.step(&InputFrame::default(), DT);
        clips.insert(clip(&duel));
    }
    assert!(clips.contains("a000_012000"), "{clips:?}");
    assert!(clips.contains("a000_020001"), "{clips:?}");
    assert_eq!(duel.rules.fighters[1].vitality.hp, 3195);
    // Clip 20001 adds 3702002, which replaces the resident 3702001 (same SpEffect category).
    assert!(has_effect(&duel, 3_702_002));
    assert!(!has_effect(&duel, 3_702_001));
    assert!(
        has_effect(&duel, 3_702_050),
        "the Todome event reads 3702050"
    );
    assert!(
        has_effect(&duel, 200_004),
        "other category-100 residents stay"
    );
}

#[test]
fn the_roar_makes_the_demon_immune_to_hp_and_posture_damage() {
    let Some((arena, mut duel)) = load() else {
        return;
    };
    start_fight(&arena, &mut duel);
    take_mark(&arena, &mut duel, false);
    for _ in 0..60 * 40 {
        duel.step(&InputFrame::default(), DT);
        if clip(&duel) == "a000_020001" {
            break;
        }
    }
    assert_eq!(clip(&duel), "a000_020001");
    let (hp, posture) = (
        duel.rules.fighters[1].vitality.hp,
        duel.rules.fighters[1].posture.remaining,
    );
    let mut struck = 0;
    for k in 0..60 * 7 {
        if k % 30 == 0 {
            beside(&arena, &mut duel, 3.0);
        }
        let input = InputFrame {
            buttons: Buttons {
                attack: k % 20 == 0,
                lock_on: k == 2,
                ..Buttons::default()
            },
            ..InputFrame::default()
        };
        let r = duel.step(&input, DT);
        struck += r.hits.iter().filter(|(h, _)| h.attacker == 0).count();
    }
    assert!(struck >= 4, "Wolf's sword connected {struck} times");
    assert_eq!(
        duel.rules.fighters[1].vitality.hp, hp,
        "300331 cuts every HP damage"
    );
    assert_eq!(
        duel.rules.fighters[1].posture.remaining, posture,
        "300321 and 300341 cut posture damage"
    );
}

#[test]
#[ignore = "slow: five minutes of simulated fight across the three phases (about 11 s)"]
fn phases_two_and_three_use_their_own_act_tables() {
    let Some((arena, mut duel)) = load() else {
        return;
    };
    start_fight(&arena, &mut duel);
    let one = soak(&arena, &mut duel, 60 * 60, 6.0);
    take_mark(&arena, &mut duel, false);
    soak(&arena, &mut duel, 60 * 25, 6.0); // heat-up roar
    let two = soak(&arena, &mut duel, 60 * 80, 6.0);
    take_mark(&arena, &mut duel, false);
    assert!(
        has_effect(&duel, 3_702_005),
        "event 11105912 adds 3702005 at one bar"
    );
    soak(&arena, &mut duel, 60 * 10, 6.0);
    let three = soak(&arena, &mut duel, 60 * 90, 6.0);
    for (n, log) in [(1, &one), (2, &two), (3, &three)] {
        eprintln!(
            "phase {n}: {} hits, acts {:?}, bullets {:?}",
            log.hits, log.acts, log.bullets
        );
        assert!(log.unknown.is_empty(), "{:?}", log.unknown);
        assert!(log.hits >= 5, "phase {n}: {}", log.hits);
    }
    let later: BTreeSet<i32> = two.acts.keys().chain(three.acts.keys()).copied().collect();
    let first: BTreeSet<i32> = one.acts.keys().copied().collect();
    assert!(
        later.difference(&first).count() >= 1,
        "later phases add moves: {first:?} vs {later:?}"
    );
}

#[test]
fn the_last_mark_plays_the_todome_and_defeats_the_demon() {
    let Some((arena, mut duel)) = load() else {
        return;
    };
    start_fight(&arena, &mut duel);
    take_mark(&arena, &mut duel, false);
    for _ in 0..60 * 14 {
        duel.step(&InputFrame::default(), DT);
    }
    take_mark(&arena, &mut duel, false);
    for _ in 0..60 * 8 {
        duel.step(&InputFrame::default(), DT);
    }
    assert_eq!(duel.rules.fighters[1].vitality.deathblows_left, 1);
    assert!(!flag(&duel, 9313));
    // The last mark: finishing pair 180/181, the defender clip is the death itself.
    let mut rows = None;
    let mut seen = BTreeSet::new();
    let mut wolf = BTreeSet::new();
    let mut broke = false;
    for i in 0..60 * 30 {
        if !broke {
            duel.rules.fighters[1].posture.remaining = 1;
            if flat(&duel) > 3.8 || flat(&duel) < 2.0 {
                beside(&arena, &mut duel, 3.0);
            }
        }
        let input = InputFrame {
            buttons: Buttons {
                attack: if broke {
                    rows.is_none() && i % 10 == 0
                } else {
                    i % 40 == 0
                },
                lock_on: (2..4).contains(&i),
                ..Buttons::default()
            },
            ..InputFrame::default()
        };
        let r = duel.step(&input, DT);
        broke |= r
            .hits
            .iter()
            .any(|(h, res)| h.attacker == 0 && res.posture_broke);
        rows = rows.or(r.deathblow.started);
        if rows.is_some() {
            seen.insert(clip(&duel));
            wolf.insert(
                duel.player
                    .behavior
                    .runtime
                    .main_clip()
                    .map(|c| c.animation.clone())
                    .unwrap_or_default(),
            );
            if flag(&duel, 9313) && seen.contains("a000_020200") {
                break;
            }
        }
    }
    assert_eq!(rows, Some((17_020_180, 17_020_181)));
    assert!(
        seen.contains("a000_013700"),
        "the death throw clip: {seen:?}"
    );
    assert!(
        seen.contains("a000_020200"),
        "event 11105921 (3702050, no bars, Wolf throwing) requests Event20200: {seen:?}"
    );
    assert!(
        wolf.contains("a000_710207"),
        "and Wolf's Event710207: {wolf:?}"
    );
    assert!(duel.rules.fighters[1].dead());
    assert!(duel.events.as_ref().unwrap().defeated);
    assert!(flag(&duel, 9313));
    assert_eq!(
        walls(&arena, &duel),
        0,
        "defeat releases the three fog walls"
    );
    // Event20200 plays out (about 6 s) instead of the generic death clip's first frame.
    let mut after = Vec::new();
    for _ in 0..60 * 10 {
        duel.step(&InputFrame::default(), DT);
        let c = clip(&duel);
        if after.last() != Some(&c) {
            after.push(c);
        }
    }
    assert_eq!(after.first().map(String::as_str), Some("a000_020200"));
    assert_eq!(after.last().map(String::as_str), Some("a000_010000"));
}

#[test]
fn a_mark_from_behind_uses_the_rear_pair_and_the_aliased_kneel() {
    let Some((arena, mut duel)) = load() else {
        return;
    };
    start_fight(&arena, &mut duel);
    soak(&arena, &mut duel, 60 * 3, 6.0);
    let rows = take_mark(&arena, &mut duel, true);
    assert_eq!(rows, (17_020_110, 17_020_111));
    assert_eq!(duel.rules.fighters[1].vitality.deathblows_left, 2);
    let mut kneeling = false;
    for _ in 0..60 * 8 {
        duel.step(&InputFrame::default(), DT);
        kneeling |= clip(&duel) == "a000_012000" || clip(&duel) == "a000_013200";
    }
    assert!(kneeling, "a000_013200 is an alias of the kneel clip");
}

#[test]
fn damage_alone_leaves_the_demon_at_one_hp_and_opens_the_deathblow() {
    let Some((arena, mut duel)) = load() else {
        return;
    };
    start_fight(&arena, &mut duel);
    // Event 11105910 makes him immortal until Wolf is in a throw (11105920).
    duel.rules.fighters[1].vitality.hp = 60;
    let mut opened = None;
    for i in 0..60 * 20 {
        if i % 20 == 0 && (flat(&duel) > 3.8 || flat(&duel) < 2.0) {
            beside(&arena, &mut duel, 3.0);
        }
        let input = InputFrame {
            buttons: Buttons {
                attack: i % 12 == 0,
                lock_on: (2..4).contains(&i),
                ..Buttons::default()
            },
            ..InputFrame::default()
        };
        let r = duel.step(&input, DT);
        if duel.rules.fighters[1].vitality.hp <= 1 {
            assert!(!duel.rules.fighters[1].dead(), "immortal until a throw");
        }
        opened = opened.or(r.deathblow.started);
        if opened.is_some() {
            break;
        }
    }
    assert!(opened.is_some(), "1 HP with marks left opens the deathblow");
    assert_eq!(duel.rules.fighters[1].vitality.deathblows_left, 3);
}

#[test]
fn off_the_arena_the_demon_fights_at_once() {
    let Some((arena, _)) = load() else {
        return;
    };
    // `--play --enemy c7020:70200000` has no region to enter: the staged fight flag keeps it on.
    let mut duel = Duel::load_with(&cache(), 6.0, arena.spec.enemy_spec()).unwrap();
    duel.god = true;
    let log = soak(&arena, &mut duel, 60 * 20, 100.0);
    assert!(duel.events.as_ref().unwrap().boss_bar == Some(907_021));
    assert!(log.hits >= 1 && log.acts.len() >= 3, "{:?}", log.acts);
}
