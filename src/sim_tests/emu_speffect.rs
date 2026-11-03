//! The SpEffect list tick against the user's own sekiro.exe, run in tools/emu-cases `speffect` and
//! `speffect2` on the Naomori-arena snapshot (docs/kb/speffect.md 2026-11-03): the add
//! FUN_1409e9c00 and the character tick FUN_140a02a10 (FUN_140a03340's list tick, then the HP and
//! posture reactions) at 30 and 60 fps. Skips when extracted/emu/speffect_ticks.json is missing.

use super::*;

fn emu_json(name: &str) -> Option<serde_json::Value> {
    let text = std::fs::read_to_string(crate::paths::root().join(format!("extracted/emu/{name}.json"))).ok()?;
    serde_json::from_str(&text).ok()
}

/// Runs left out: the gourd 3000 heals through `player::estus_heal` (its own test below); the
/// deflect's 105010, the umbrella's 107650 and Ichimonji's 140201 are Wolf's TAE rows
/// (`Player::timed`, actor.rs chains), not `Status`.
const SKIP: &[&str] = &["gourd_3000", "deflect_105010", "umbrella_107650", "ichimonji_140201"];

/// Frame by frame: the build-up gauges after each add, HP and posture after each tick and the
/// added effects' list (id, time left, expired flag): `Statuses::build_up` / `start` /
/// `tick_effects` against the exe. The character wears what the snapshot's list holds before the
/// first add (Wolf: Hard mode's 8610, Knowledge of Medicine LV5 150204, the buff-duration skill
/// 150600; the General: his residents, 30201's disables cleared where the run says so).
#[test]
pub(crate) fn speffect_ticks_match_the_exe() {
    let Some(v) = emu_json("speffect_ticks") else { return };
    let mut app = app();
    let combat = app.world_mut().remove_resource::<Combat>().unwrap();
    let e = Entity::PLACEHOLDER;
    let mut bad: Vec<String> = Vec::new();
    let mut frames = 0;
    let mut runs_done = 0;
    let second = emu_json("speffect2_ticks");
    let runs = v["runs"].as_array().unwrap().iter().chain(second.as_ref().map(|s| s["runs"].as_array().unwrap().iter()).into_iter().flatten());
    for run in runs {
        let name = run["name"].as_str().unwrap();
        let enemy = run["enemy"].as_bool().unwrap();
        if SKIP.contains(&name) || !run["stopped"].is_null() {
            continue;
        }
        let fps = run["fps"].as_i64().unwrap();
        let dt = run["dt"].as_f64().unwrap() as f32;
        let start = &run["start"];
        let hp_max = start["hp_max"].as_f64().unwrap() as f32;
        let posture_max = start["posture_max"].as_f64().unwrap() as f32;
        let mut hp = start["hp"].as_f64().unwrap() as f32;
        let mut posture = start["posture"].as_f64().unwrap() as f32;
        let base: Vec<i64> = start["sp"].as_array().unwrap().iter().map(|s| s[0].as_i64().unwrap()).collect();
        let cleared: Vec<i64> = run["cleared"].as_array().map(|c| c.iter().filter_map(|x| x.as_i64()).collect()).unwrap_or_default();
        let worn: Vec<i64> = base.iter().copied().filter(|id| !cleared.contains(id)).collect();
        // The resist module's max per exe type (+0x24: poison, disease/terror, blood/burn, curse,
        // freeze) in Sv1's order (poison, burn, terror, curse, freeze).
        let r = |i: usize| start["resist"][9 + i].as_f64().unwrap() as f32;
        let resist = [r(0), r(2), r(1), r(3), r(4)];
        let mut s = crate::status::Statuses::default();
        let mut stop = Vec::new();
        let adds = run["adds"].as_array().unwrap();
        // Only rows Sv1 has (Wolf's export, with the list's rows `sekiro-extract listsp` adds).
        let has = |id: i64| combat.player.sp_effects.contains_key(&id.to_string()) || combat.kinds.iter().any(|k| k.data.sp_effects.contains_key(&id.to_string()));
        if !adds.iter().all(|a| has(a[1].as_i64().unwrap())) {
            bad.push(format!("{name}: rows not exported"));
            continue;
        }
        // gap: cycleOccurrenceSpEffectId rows (Ungo's 3421 -> 3422 -> 3423) are not in `Status`.
        let mut cycles: Vec<i64> = Vec::new();
        let mut todo: Vec<i64> = adds.iter().map(|a| a[1].as_i64().unwrap()).collect();
        while let Some(id) = todo.pop() {
            if let Some(c) = crate::data::sp_cycle(id).map(|c| c.cycle).filter(|c| *c > 0 && !cycles.contains(c)) {
                cycles.push(c);
                todo.push(c);
            }
        }
        runs_done += 1;
        let tag = format!("{name} {} {fps}fps", if enemy { "enemy" } else { "wolf" });
        for (i, f) in run["frames"].as_array().unwrap().iter().enumerate() {
            for (k, a) in adds.iter().filter(|a| a[0].as_u64() == Some(i as u64)).enumerate() {
                let id = a[1].as_i64().unwrap();
                if crate::status::Statuses::is_build_up(&combat, id) {
                    s.build_up_for(&combat, e, &worn, id, resist);
                } else {
                    s.start_for_wearing(&combat, e, &worn, id);
                }
                // The exe's left per type after this add = resist - Sv1's gauge.
                let after = &f["added"][k]["after"]["resist"];
                let st = s.0.entry(e).or_default();
                for (ty, (exe_i, gauge)) in [(0, st.poison), (2, st.burn), (1, st.terror)].into_iter().enumerate() {
                    let exe = after[4 + exe_i].as_f64().unwrap() as f32;
                    let ours = [resist[0], resist[1], resist[2]][ty] - gauge;
                    if exe != ours {
                        bad.push(format!("{tag} f{i} add {id}: type {ty} left exe {exe} ours {ours}"));
                    }
                }
            }
            let st = s.0.entry(e).or_default();
            let ticked = crate::status::tick_effects(&combat, st, &crate::status::Wearer { worn: &worn, resist }, hp_max, posture_max, dt, &mut stop);
            // Aging's maxHpRate (set_aging, outside the list) lowers the max: the exe's is taken.
            let max_now = f["hp_max"].as_f64().unwrap() as f32;
            hp = (hp + ticked.hp as f32).clamp(0.0, max_now);
            posture = (posture + ticked.posture as f32).clamp(0.0, posture_max);
            frames += 1;
            let exe_hp = f["hp"].as_f64().unwrap() as f32;
            // Wolf's terror kill: the exe's snapshot keeps him at 1 (no-death flag), Sv1 at 0.
            if exe_hp != hp && !(exe_hp == 1.0 && hp == 0.0) {
                bad.push(format!("{tag} f{i}: hp exe {exe_hp} ours {hp}"));
            }
            let exe_posture = f["posture"].as_f64().unwrap() as f32;
            if exe_posture != posture {
                bad.push(format!("{tag} f{i}: posture exe {exe_posture} ours {posture}"));
            }
            let exe: Vec<(i64, f32, bool)> = f["sp"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|x| !base.contains(&x[0].as_i64().unwrap()) && !cycles.contains(&x[0].as_i64().unwrap()))
                .map(|x| (x[0].as_i64().unwrap(), x[1].as_f64().unwrap() as f32, x[4].as_i64() == Some(1)))
                .collect();
            let ours: Vec<(i64, f32, bool)> = st.effects.iter().map(|a| (a.id, a.left, a.expired)).collect();
            let same = exe.len() == ours.len() && exe.iter().zip(&ours).all(|(x, o)| x.0 == o.0 && (x.1 - o.1).abs() < 1e-4 && x.2 == o.2);
            if !same {
                bad.push(format!("{tag} f{i}: list exe {exe:?} ours {ours:?}"));
            }
        }
    }
    assert!(frames > 0, "no runs");
    // The first three per run.
    let mut shown: Vec<String> = Vec::new();
    for b in &bad {
        let run = b.split(" f").next().unwrap_or_default();
        if shown.iter().filter(|s| s.starts_with(run)).count() < 3 {
            shown.push(b.clone());
        }
    }
    assert!(bad.is_empty(), "{} differ in {frames} frames of {runs_done} runs:\n{}", bad.len(), shown.join("\n"));
}

/// The gourd 3000 on Wolf (Knowledge of Medicine LV5, x1.5): the exe heals (int)(1120 x 40 % x
/// 1.5) = 672, capped at max; `player::estus_heal`.
#[test]
pub(crate) fn gourd_heal_matches_the_exe() {
    let Some(v) = emu_json("speffect_ticks") else { return };
    for run in v["runs"].as_array().unwrap().iter().filter(|r| r["name"] == "gourd_3000") {
        let max = run["start"]["hp_max"].as_f64().unwrap() as f32;
        let hp = run["start"]["hp"].as_f64().unwrap() as f32;
        let exe = run["frames"][0]["hp"].as_f64().unwrap() as f32;
        // Wolf's worn 150204 carries the LV5 correction (changeHpEstusFlaskCorrectRate 1.5).
        let ours = (hp + crate::player::estus_heal(max, 40.0, 1.5)).min(max);
        assert_eq!(ours, exe, "{} fps", run["fps"]);
        let lowered = crate::player::estus_heal(1001.0, 40.0, 1.5);
        assert_eq!(lowered, 600.0, "whole points");
    }
}

/// Wolf's worn rows reach his build-ups (`player::wolf_worn` into `Statuses::hit_wolf_wearing`):
/// Hard mode (config `player.hard_mode`, SpEffect 8610) takes a burn 9100's 31 to 46, as the
/// exe's Wolf wearing 8610 (speffect_ticks burn_9100: 200 -> 154); the skills' rows come with
/// Knowledge of Medicine at its level.
#[test]
pub(crate) fn wolf_worn_rows_reach_his_build_up() {
    let mut app = app();
    let combat = app.world_mut().remove_resource::<Combat>().unwrap();
    let mut config = app.world().resource::<crate::config::GameConfig>().clone();
    config.player.skills.clear();
    config.player.hard_mode = false;
    let wolf = crate::actor::Actor::new(crate::actor::Side::Player, 1120.0, 420.0, 0.0, 0);
    let e = Entity::PLACEHOLDER;
    let gauge = |config: &crate::config::GameConfig| {
        let mut s = crate::status::Statuses::default();
        s.hit_wolf_wearing(&combat, e, &crate::player::wolf_worn(&combat, config, &wolf), config.player.combat_art, &[9100], crate::status::Guard::None);
        s.0.get(&e).map_or(0.0, |st| st.burn)
    };
    assert_eq!(gauge(&config), 31.0, "normal Wolf");
    config.player.hard_mode = true;
    assert_eq!(gauge(&config), 46.0, "hard mode x1.5");
    config.player.skills = vec![170, 171, 600, 601, 602, 370];
    let worn = crate::player::wolf_worn(&combat, &config, &wolf);
    assert!(worn.contains(&150204) && worn.contains(&150600) && !worn.contains(&150200) && !worn.contains(&150210), "{worn:?}");
}
