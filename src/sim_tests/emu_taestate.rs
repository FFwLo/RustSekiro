//! State events and the selector's first window against the exe's.

use crate::actor::{Actor, Side};
use crate::tae_window::{dispatch, starts, At};

type Events = Vec<(f32, f32, u64)>;

fn list(evs: &serde_json::Value) -> Events {
    let f = |x: &serde_json::Value| x.as_f64().unwrap() as f32;
    evs.as_array().unwrap().iter().map(|e| (f(&e[0]), f(&e[1]), e[2].as_u64().unwrap())).collect()
}

/// The events the exe dispatched in one update (type, start, end) with their "entered" byte.
fn exe_hits(hits: &serde_json::Value) -> Vec<(u64, f32, f32, bool)> {
    let f = |x: &serde_json::Value| x.as_f64().unwrap() as f32;
    hits.as_array().unwrap().iter().filter(|h| h["via"] == "direct").map(|h| (h["type"].as_u64().unwrap(), f(&h["start"]), f(&h["end"]), h["entered"] == 1)).collect()
}

/// State events (`At::Step`, what `CharData::events_at` / `flag` / `sp_effects_at` ... ask with
/// `Actor::step`) against every dispatch of the taewin cases (FUN_140b5c920 -> FUN_140654b70 ->
/// FUN_1425e5640 on Wolf's module: the synthetic list and Wolf's own anims, 30 / 60 / 20 / 144 fps,
/// speed changes, hit stop, first updates, edges, random windows): an event is dispatched in an
/// update exactly when `At::Step(prev, t).dispatched` says so, on every update that does not wrap.
/// Skipped when extracted/emu/taewin_cases.json is missing.
#[test]
pub(crate) fn state_events_follow_the_exe_windows() {
    let path = crate::paths::emu().join("taewin_cases.json");
    let Ok(text) = std::fs::read_to_string(&path) else { return };
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    let f = |x: &serde_json::Value| x.as_f64().unwrap() as f32;
    let (mut asked, mut point, mut bad) = (0, 0, Vec::new());
    let mut check = |name: &str, evs: &Events, fr: &serde_json::Value| {
        let (prev, cur) = (f(&fr["prev"]), f(&fr["cur"]));
        // (0, 0): RSekiro keeps the events at 0 (a clip played, not stepped yet; `At::dispatched`)
        if cur < prev || cur == 0.0 {
            return;
        }
        let theirs = exe_hits(&fr["hits"]);
        for &(s, e, k) in evs {
            let exe = theirs.iter().any(|h| (h.0, h.1, h.2) == (k, s, e));
            asked += 1;
            point += ((s <= cur && cur < e) != exe) as usize;
            if At::Step(prev, cur).dispatched(s, e) != exe && bad.len() < 8 {
                bad.push(format!("{name} prev {prev} cur {cur}: event {k} [{s}, {e}] exe {exe}"));
            }
        }
    };
    let synth = list(&v["events"]);
    for c in v["cases"].as_array().unwrap() {
        for fr in c["frames"].as_array().unwrap() {
            check(c["name"].as_str().unwrap(), &synth, fr);
        }
    }
    for real in v["real"].as_array().unwrap() {
        let evs = list(&real["events"]);
        for fr in real["frames"].as_array().unwrap() {
            check("wolf", &evs, fr);
        }
    }
    eprintln!("state events: {asked} event-updates; the point test start <= t < end differed on {point}");
    assert!(asked > 10_000);
    assert!(bad.is_empty(), "state events differ from the exe's windows:\n{}", bad.join("\n"));
}

/// The selector (tools/emu-cases `taestate`: Wolf's CustomManualSelectorGenerator of a000_790010
/// on the snapshot, preUpdate FUN_1411e55e0 / update FUN_1411e56e0 / FUN_1411e6de0, its TAE call
/// FUN_1411e8f30 run on into FUN_140b5c920 and Wolf's module; his anim and taewin's synthetic
/// list): every update passes (the clip time at preUpdate, the time after the step); after
/// FUN_1411e6de0 (activate, a change of selection) (0, the new clip's time), so a clip started at
/// a StartTime offset enters every event before it (with a preUpdate between, (t, t): nothing);
/// the dispatches equal `tae_window::dispatch`. RSekiro's `Actor::start_at` (continue_state, the
/// land continuations) takes the preUpdate path (gap, see there): counted, not asserted. Skipped
/// when extracted/emu/taestate_cases.json is missing.
#[test]
pub(crate) fn selector_first_window_matches_the_exe() {
    let path = crate::paths::emu().join("taestate_cases.json");
    let Ok(text) = std::fs::read_to_string(&path) else { return };
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    let f = |x: &serde_json::Value| x.as_f64().unwrap() as f32;
    assert_eq!(v["outgoing"], 0, "Wolf's selector is not the outgoing one of a transition");
    let (real, synth) = (list(&v["events"]), list(&v["synth_events"]));
    let mut bad = Vec::new();
    let (mut updates, mut entered_gap) = (0, 0);
    for c in v["cases"].as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        let evs = if name.starts_with("synth") { &synth } else { &real };
        let frames = c["frames"].as_array().unwrap();
        // The exe: the selector's arguments and the dispatches they gave.
        let mut exe: Vec<(f32, f32, Vec<(u64, f32, f32, bool)>)> = Vec::new();
        for fr in frames {
            let calls = fr["calls"].as_array().unwrap();
            assert_eq!(calls.len(), 1, "{name}: one TAE call per update");
            let (prev, cur, len) = (f(&calls[0]["prev"]), f(&calls[0]["cur"]), f(&calls[0]["len"]));
            let want_prev = if fr["reset"] == true && fr["pre"] == false { 0.0 } else { f(&fr["t0"]) };
            if (prev, cur) != (want_prev, f(&fr["t1"])) && bad.len() < 8 {
                bad.push(format!("{name}: selector passed ({prev}, {cur}), not ({want_prev}, {})", f(&fr["t1"])));
            }
            let spans: Vec<(f32, f32)> = evs.iter().map(|e| (e.0, e.1)).collect();
            let mut ours: Vec<(u64, f32, f32, bool)> = dispatch(&spans, prev, cur, len).into_iter().map(|(i, _, h)| (evs[i].2, evs[i].0, evs[i].1, h.entered)).collect();
            let mut theirs = exe_hits(&fr["hits"]);
            ours.sort_by(|a, b| a.partial_cmp(b).unwrap());
            theirs.sort_by(|a, b| a.partial_cmp(b).unwrap());
            if ours != theirs && bad.len() < 8 {
                bad.push(format!("{name} ({prev}, {cur}): exe {theirs:?}, tae_window {ours:?}"));
            }
            exe.push((prev, cur, theirs));
            updates += 1;
        }
        // RSekiro: the clip started at the offset by `start_at`, then stepped at 30 fps; what the
        // exe's reset update and the one after it enter against RSekiro's first step.
        let Some(r) = frames.iter().position(|fr| fr["reset"] == true && fr["pre"] == false) else { continue };
        if r + 1 >= frames.len() {
            continue;
        }
        let off = f(&frames[r]["t1"]);
        let f30 = 1.0f32 / 30.0;
        let mut a = Actor::new(Side::Player, 1.0, 1.0, 1.0, 0);
        a.play("Test", "");
        a.start_at(off);
        // the exe's reset update and the one after it make RSekiro's first step
        for (k, i) in std::iter::once(r + 1).chain(r + 2..frames.len()).enumerate() {
            a.begin_step();
            a.t += f30;
            let want: Vec<&(u64, f32, f32, bool)> = if k == 0 { exe[r].2.iter().chain(exe[r + 1].2.iter()).collect() } else { exe[i].2.iter().collect() };
            for &(s, e, ty) in evs {
                let exe_in = want.iter().any(|h| (h.0, h.1, h.2) == (ty, s, e) && h.3);
                let exe_on = want.iter().any(|h| (h.0, h.1, h.2) == (ty, s, e));
                let (new_in, new_on) = (starts(s, a.prev_t, a.t), a.step().dispatched(s, e));
                if k == 0 {
                    entered_gap += (new_in != exe_in) as usize;
                } else if (new_in, new_on) != (exe_in, exe_on) && bad.len() < 8 {
                    bad.push(format!("{name} step {k} ({}, {}): event {ty} [{s}, {e}] entered {new_in} / exe {exe_in}, dispatched {new_on} / exe {exe_on}", a.prev_t, a.t));
                }
            }
        }
    }
    eprintln!("selector: {updates} updates; the reset path's first window enters {entered_gap} events RSekiro's start_at does not (gap)");
    assert!(updates >= 60);
    assert!(bad.is_empty(), "the selector's windows differ:\n{}", bad.join("\n"));
}
