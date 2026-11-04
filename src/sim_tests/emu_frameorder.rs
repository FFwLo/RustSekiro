//! Frame order against the user's own sekiro.exe (rows E3 / E4, kb/frame-coverage.md "One-shot TAE
//! events' frame", kb/modticks.md "Which frame a hit's stop starts on").
//!
//! E4: tools/emu-cases `frameorder` runs Wolf's whole chr frames FUN_140a4b570 through acttrace's
//! acts with the TAE update FUN_140b5c920, the Havok step FUN_141015fa0, the replay FUN_140b5c730,
//! the 32 / 67 / 401 handlers and the SpEffect add FUN_1409e9c00 logged in call order. The exe adds
//! a TAE 67 / 401 SpEffect inside the Havok step of the frame whose update enters the event; Sv1 now
//! applies its one-shots (`player::tae_one_shots`, `apply_refresh_effects`, the weapon style) right
//! after the `advance` that enters them, not in the next tick's `decide`.
//!
//! E3: tools/emu-cases `hits_stop_*` lands the General's slash with a hit stop (attacker 0.1 s,
//! defender 0.15 s in the emulator's copy of AtkParam_Npc 10200100) and logs both chr clocks
//! (chr+0x108) per frame. Skips when the JSON is missing.

use super::*;

fn emu_json(name: &str) -> Option<serde_json::Value> {
    let text = std::fs::read_to_string(crate::paths::emu().join(format!("{name}.json"))).ok()?;
    serde_json::from_str(&text).ok()
}

const ACTS: [&str; 7] = ["attack", "art", "deflect", "step", "jump", "prosthetic", "style"];

/// Every TAE 67 / 401 SpEffect the exe adds from an event comes from the update that enters it
/// (the window rule `tae_window::starts` on that update's (prev, cur)), inside that frame's Havok
/// step (after FUN_141015fa0 is entered, before the replay FUN_140b5c730), and each update enters
/// exactly the clip's 67 / 401 / 32 events Sv1's rule says (`style`: Wolf's idle given two
/// synthetic TAE 32 SetWeaponStyle events; its handler FUN_140b509b0 runs in the step the same way).
#[test]
pub(crate) fn one_shot_tae_events_land_in_the_steps_frame_in_the_exe() {
    let mut adds = 0;
    let mut entered = 0;
    for act in ACTS {
        let Some(v) = emu_json(&format!("frameorder_{act}")) else {
            eprintln!("skip: no frameorder_{act}.json (tools/emu-cases frameorder)");
            return;
        };
        let clips = v["clips"].as_object().unwrap();
        for fr in v["frames"].as_array().unwrap() {
            let log = fr["log"].as_array().unwrap();
            let step = log.iter().position(|e| e["at"] == "havok_step");
            let replay = log.iter().position(|e| e["at"] == "replay");
            // The update each event dispatch belongs to: the last TAE update logged before it.
            let mut per_update: Vec<((String, f32, f32), [usize; 3])> = Vec::new();
            for (i, e) in log.iter().enumerate() {
                let at = e["at"].as_str().unwrap();
                if at == "tae" {
                    let w = (e["clip"].as_i64().unwrap().to_string(), e["prev"].as_f64().unwrap() as f32, e["cur"].as_f64().unwrap() as f32);
                    per_update.push((w, [0, 0, 0]));
                    continue;
                }
                let kind = match at {
                    "ev67" => 0,
                    "ev401" => 1,
                    "ev32" => 2,
                    _ => continue,
                };
                assert!(step.is_some_and(|s| s < i) && replay.is_none_or(|r| i < r), "{act} f{}: {at} outside the Havok step", fr["f"]);
                if e["entered"] == 1 {
                    per_update.last_mut().expect("a dispatch with no TAE update").1[kind] += 1;
                    entered += 1;
                    // The add that follows it (the handler's own call: FUN_140b56500 / FUN_140b56750).
                    if log.get(i + 1).is_some_and(|n| n["at"] == "sp_add" && n["id"] == e["arg"]) {
                        adds += 1;
                    }
                } else {
                    assert!(log.get(i + 1).is_none_or(|n| !(n["at"] == "sp_add" && n["id"] == e["arg"])), "{act} f{}: an add on a dispatch that did not enter", fr["f"]);
                }
            }
            for ((clip, prev, now), got) in per_update {
                let evs = clips.get(&clip).and_then(|c| c.as_array()).cloned().unwrap_or_default();
                let mut want = [0, 0, 0];
                for ev in &evs {
                    let (start, kind) = (ev[0].as_f64().unwrap() as f32, ev[2].as_i64().unwrap());
                    let k = match kind {
                        67 => 0,
                        401 => 1,
                        32 => 2,
                        _ => continue,
                    };
                    if crate::tae_window::starts(start, prev, now) {
                        want[k] += 1;
                    }
                }
                assert_eq!(got, want, "{act} f{} clip {clip} ({prev}, {now}]: entered 67 / 401 / 32", fr["f"]);
            }
        }
    }
    assert!(adds > 20 && entered >= adds, "adds {adds}, entered {entered}");
}

/// Wolf in `state` from its first frame; ticks until the step enters `start` of `anim`; returns the
/// ticks (1-based) on which the window entered it and on which `landed` first held.
fn tick_of(app: &mut App, start: f32, frames: usize, landed: impl Fn(&Actor) -> bool) -> (Option<usize>, Option<usize>) {
    let (mut enter, mut land) = (None, None);
    for k in 1..=frames {
        app.update();
        let world = app.world_mut();
        let a = world.query_filtered::<&Actor, With<Player>>().single(world).unwrap();
        if enter.is_none() && a.prev_t < a.t && crate::tae_window::starts(start, a.prev_t, a.t) {
            enter = Some(k);
        }
        if land.is_none() && landed(a) {
            land = Some(k);
        }
    }
    (enter, land)
}

/// Sv1 lands its one-shot TAE events on the tick whose step enters them, as the exe (above): the
/// sheathe's TAE 32 SetWeaponStyle None (a000_700500, frame 14), the revival's TAE 401 refresh
/// 8001 (posture to 0) and one-shot 110015 (half HP) at frame 0 of GroundRevival_F. They came a
/// tick late (the next tick's `decide`).
#[test]
pub(crate) fn one_shot_tae_events_land_in_the_steps_tick() {
    // Sheathe.
    let mut app = self::app();
    move_enemy_far(&mut app);
    let start = {
        let c = app.world().resource::<crate::data::Combat>();
        c.player.anim("a000_700500").unwrap().events.iter().find(|e| e.kind == 32).expect("a TAE 32 in a000_700500").start
    };
    app.world_mut().resource_mut::<PadInput>().sheathe = true;
    let (enter, land) = tick_of(&mut app, start, 40, |a| a.sheathed);
    assert!(enter.is_some(), "the sheathe clip never reached {start}");
    assert_eq!(land, enter, "weapon style None at {start}: entered on tick {enter:?}, sheathed on {land:?}");

    // Revival: posture and HP both low, Wolf put in GroundRevival_F at its frame 0.
    let mut app = self::app();
    force_player(&mut app, "GroundRevival_F", 0.0);
    let (hp0, posture0) = {
        let world = app.world_mut();
        let mut a = world.query_filtered::<&mut Actor, With<Player>>().single_mut(world).unwrap();
        a.hp = 10.0;
        a.posture = a.posture_max * 0.5;
        (a.hp, a.posture)
    };
    let (enter, healed) = tick_of(&mut app, 0.0, 6, |a| a.hp > hp0 + 1.0);
    assert_eq!(enter, Some(1), "GroundRevival_F's first step enters frame 0");
    assert_eq!(healed, enter, "110015 half HP: entered on tick {enter:?}, healed on {healed:?}");
    let mut app2 = self::app();
    force_player(&mut app2, "GroundRevival_F", 0.0);
    {
        let world = app2.world_mut();
        let mut a = world.query_filtered::<&mut Actor, With<Player>>().single_mut(world).unwrap();
        a.posture = posture0;
    }
    let (enter, refreshed) = tick_of(&mut app2, 0.0, 6, |a| a.posture < posture0 * 0.5);
    assert_eq!(refreshed, enter, "8001 posture refresh: entered on tick {enter:?}, refreshed on {refreshed:?}");
}

/// The exe's chr clocks (chr+0x108) per frame of a `hits_stop_*` duel, (Wolf's, the General's), and
/// the hit frame; checks the hit step itself resolved the hit and started both stops.
fn exe_stop_clocks(v: &serde_json::Value) -> (usize, Vec<(f32, f32)>) {
    let fr = v["frames"].as_array().unwrap();
    let h = fr.iter().position(|x| !x["hits"].as_array().unwrap().is_empty()).expect("the slash landed");
    assert_eq!(fr[h]["step_hits"], 1, "the hit resolved in the hit step, before the chr frames");
    assert_eq!(fr[h]["step_stops"], 2, "both stops started in the hit step");
    let c = |x: &serde_json::Value, who: &str| x[who]["clock"].as_f64().unwrap() as f32;
    (h, fr.iter().map(|x| (c(x, "wolf"), c(x, "general"))).collect())
}

/// Row E3: the General's slash (AtkParam_Npc 10200100 given hitStopTime 0.1 and hitStopTime_Defencer
/// 0.15, as the emulator's copy) on Wolf idle or guarding. The exe resolves the hit in MoveMapStep's
/// hit step FUN_1409a0dc0 (task group 0x4b), before every chr frame (0x52), and starts both stops
/// there: both clocks are 0 from the hit frame itself, whichever chr updates first (`_wfirst`: Wolf's
/// frame before the General's), then hold and ease out on their own lengths. Sv1's `resolve` runs
/// before `advance` for every actor, so the attacker and the defender stop on the hit tick too.
#[test]
pub(crate) fn hit_stops_start_on_the_hit_frame_for_both_sides() {
    for (name, guard) in [("stop_g_open", false), ("stop_g_open_wfirst", false), ("stop_g_guard", true), ("stop_g_guard_wfirst", true)] {
        let Some(v) = emu_json(&format!("hits_{name}")) else {
            eprintln!("skip: no hits_{name}.json (tools/emu-cases hits_stop_*)");
            continue;
        };
        let (h, exe) = exe_stop_clocks(&v);
        // Sv1: the General 2.5 m in front, his slash forced on tick 0, its row given the same times.
        let mut app = app_geom("c1020", None);
        {
            let world = app.world_mut();
            let mut combat = world.resource_mut::<crate::data::Combat>();
            // (Keyed by behaviour judge; only the slash lands in the run, so every row gets them.)
            for k in combat.kinds.iter_mut() {
                for atk in k.data.attacks.values_mut() {
                    atk.hit_stop_time = 0.1;
                    atk.hit_stop_time_defencer = 0.15;
                }
            }
            let mut q = world.query_filtered::<&mut Transform, With<Enemy>>();
            for mut t in q.iter_mut(world) {
                t.translation.z = 2.5;
            }
            let mut a = world.query_filtered::<&mut Actor, With<Player>>().single_mut(world).unwrap();
            a.hp_max = 5000.0;
            a.hp = 5000.0;
            let mut q = world.query::<(&mut Enemy, &mut Actor)>();
            let (mut e, mut a) = q.single_mut(world).unwrap();
            e.force_anim(&mut a, "a000_003000");
        }
        let mut sim: Vec<(f32, f32)> = Vec::new();
        let mut hit = None;
        let mut lines = 0;
        for f in 0..75 {
            if guard {
                let mut p = app.world_mut().resource_mut::<PadInput>();
                if f == 0 {
                    p.press(Action::Guard);
                }
                p.guard_held = true;
            }
            app.update();
            let l = &app.world().resource::<CombatLog>().lines;
            let log = l[lines.min(l.len())..].iter().map(|x| x.0.clone()).collect::<Vec<_>>().join(" | ");
            lines = l.len();
            if hit.is_none() && (log.contains("hit:") || log.contains("blocked")) {
                hit = Some(f);
            }
            let world = app.world_mut();
            let w = world.query_filtered::<&Actor, With<Player>>().single(world).unwrap().clock(1.0 / 60.0);
            let e = boss_in_play(world);
            let g = world.get::<Actor>(e).unwrap().clock(1.0 / 60.0);
            sim.push((w, g));
        }
        let i = hit.unwrap_or_else(|| panic!("{name}: no hit in Sv1"));
        // From the frame before the hit until both run whole frames again.
        for k in 0..16 {
            let (e, s) = (exe[h - 1 + k], sim[i - 1 + k]);
            assert!((e.0 - s.0).abs() < 1e-6 && (e.1 - s.1).abs() < 1e-6, "{name}: hit frame {:+}: exe (Wolf, General) {e:?}, Sv1 {s:?}", k as i32 - 1);
        }
    }
}
