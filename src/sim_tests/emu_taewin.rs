//! The TAE event window against the exe's.

use crate::tae_window::{dispatch, starts};

/// The game's TAE dispatch (tools/emu-cases `taewin`: FUN_140b5c920 -> FUN_140b5c580 ->
/// FUN_140654b70 -> FUN_1425e5640 on Wolf's module of the snapshot, and the multithreaded path
/// FUN_140b58420 + the queue replay FUN_140b5c730) on a synthetic event list (start 0, frame-grid
/// edges, zero length, shorter than a 60 fps frame, at and past the clip end) and on Wolf's own
/// anim: 30 / 60 / 20 / 144 fps runs, a phase off the grid, speed changes x0.5 / x3, hit stop
/// (dt 0), a one-shot clip held at its end, the first update of a new clip, windows on and
/// 1e-7 .. 6e-7 s around every edge, loop wraps and 450 random windows. Every dispatch (event,
/// window, entered, ended) must equal `tae_window::dispatch`, and `starts` must equal "entered"
/// wherever the clock does not wrap. Skipped when extracted/emu/taewin_cases.json is missing.
#[test]
pub(crate) fn tae_window_matches_the_exe() {
    let path = crate::paths::root().join("extracted/emu/taewin_cases.json");
    let Ok(text) = std::fs::read_to_string(&path) else { return };
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    let f = |x: &serde_json::Value| x.as_f64().unwrap() as f32;
    let list = |evs: &serde_json::Value| -> Vec<(f32, f32, u64)> { evs.as_array().unwrap().iter().map(|e| (f(&e[0]), f(&e[1]), e[2].as_u64().unwrap())).collect() };
    // (type, a, b, entered, ended) as the exe's callback saw them.
    let exe = |hits: &serde_json::Value, via: &str| -> Vec<(u64, f32, f32, bool, bool)> {
        hits.as_array().unwrap().iter().filter(|h| h["via"] == via).map(|h| (h["type"].as_u64().unwrap(), f(&h["a"]), f(&h["b"]), h["entered"] == 1, h["ended"] == 1)).collect()
    };
    let mut bad = Vec::new();
    let (mut windows, mut dispatched, mut old_enter, mut old_active, mut entered) = (0, 0, 0, 0, 0);
    let mut check = |name: &str, evs: &[(f32, f32, u64)], fr: &serde_json::Value, len: f32| {
        let (prev, cur) = (f(&fr["prev"]), f(&fr["cur"]));
        let spans: Vec<(f32, f32)> = evs.iter().map(|e| (e.0, e.1)).collect();
        let ours: Vec<_> = dispatch(&spans, prev, cur, len).into_iter().map(|(i, (a, b), h)| (evs[i].2, a, b, h.entered, h.ended)).collect();
        windows += 1;
        for via in ["direct", "step", "replay"] {
            let hits = if via == "direct" { &fr["hits"] } else if fr["mt"].is_array() { &fr["mt"] } else { continue };
            let theirs = exe(hits, via);
            if theirs != ours && bad.len() < 8 {
                bad.push(format!("{name} {via} prev {prev} cur {cur}: exe {theirs:?}, ours {ours:?}"));
            }
        }
        // Each direct dispatch as (type, start, end, entered): Wolf's anims repeat types.
        let theirs: Vec<(u64, f32, f32, bool)> = fr["hits"].as_array().unwrap().iter().map(|h| (h["type"].as_u64().unwrap(), f(&h["start"]), f(&h["end"]), h["entered"] == 1)).collect();
        dispatched += theirs.len();
        if cur >= prev {
            for (s, e, k) in evs {
                let exe_enters = theirs.iter().any(|h| (h.0, h.1, h.2) == (*k, *s, *e) && h.3);
                entered += exe_enters as usize;
                if starts(*s, prev, cur) != exe_enters && bad.len() < 8 {
                    bad.push(format!("{name} starts({s}, {prev}, {cur}) = {}, exe {exe_enters}", !exe_enters));
                }
                // RSekiro's rules before this check: the start edge (prev_t, t] (+ an event at 0
                // when prev_t is 0) and the point test start <= t < end for state events.
                old_enter += (((*s > prev || (prev == 0.0 && *s == 0.0)) && *s <= cur) != exe_enters) as usize;
                old_active += ((*s <= cur && cur < *e) != theirs.iter().any(|h| (h.0, h.1, h.2) == (*k, *s, *e))) as usize;
            }
        }
    };
    let synth = list(&v["events"]);
    let cases = v["cases"].as_array().unwrap();
    assert!(cases.len() >= 14);
    for c in cases {
        for fr in c["frames"].as_array().unwrap() {
            check(c["name"].as_str().unwrap(), &synth, fr, f(&c["len"]));
        }
    }
    for real in v["real"].as_array().unwrap() {
        let evs = list(&real["events"]);
        for fr in real["frames"].as_array().unwrap() {
            check("wolf", &evs, fr, f(&real["len"]));
        }
    }
    eprintln!("tae window: {windows} updates, {dispatched} dispatches, {entered} entered; the old start-edge rule differed on {old_enter}, the point test on {old_active}");
    assert!(bad.is_empty(), "the TAE window differs from the exe:\n{}", bad.join("\n"));
}
