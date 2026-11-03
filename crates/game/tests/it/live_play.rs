//! The real play path in a window: Explore Ashina Outskirts as the menu launches it, with
//! `--live-check` measuring loading, frame times and drawn ground (see
//! `docs/MAP-COMPLETENESS-SWEEP.md`, "Live play check"). Needs a GPU, a desktop session and
//! the extracted cache; it opens one game window at a time.
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Seconds from launch until the loading screen lifts. HEAD e7bebb6 took about 125 s; the
/// streamed, GPU-checked load takes 50 to 60 s on the reference machine (Ryzen 7 5700X3D,
/// RTX 5070 Ti, warm file cache).
const READY_SECONDS: f64 = 90.0;
/// The longest frame while loading, after the first. HEAD e7bebb6 stalled 8.7 s; streaming
/// measures about 0.4 s. Windows marks a window "Not Responding" after 5 s.
const LOADING_FRAME_MS: f64 = 2000.0;
const FRAME_MEDIAN_MS: f64 = 20.0;
const FRAME_P95_MS: f64 = 40.0;

/// Explore mode's arguments for the Outskirts, exactly as `menu.rs` builds them.
const EXPLORE: [&str; 16] = [
    "--play",
    "--enemy",
    "soldier",
    "--map",
    "m11_00_00_00",
    "--start",
    "0",
    "--brain",
    "lua",
    "--progression",
    "castle-roof",
    "--player",
    "default",
    "--objects",
    "--simple-ai",
    "--no-enemy",
];

/// Ashina Castle player starts 1, 2, 3, 5, 6 and 7, reached from the Outskirts load, then a drop
/// into the Outskirts ravine north of start 0, whose only collision is the filter-15 event wall
/// `h900000` 30 m down (`sekiro_sim::hit_filter`): Wolf falls through it and the fall return
/// puts him back on the last stop.
const CASTLE: &str = "202.6,-26.4,176.7;82.9,29.8,254.3;120.3,54.1,234.2;-60.4,-31.2,360.4;336.9,-32.1,252.7;47.1,-2.6,188.6;drop:-137.6,-44.3,-124.5";

/// Run out of the start hall in four directions, then sprint.
const WALK: &str = "\
60 0 0 -
240 0 1 -
240 1 0 -
240 0 -1 -
240 -1 0 -
240 0 1 dodge
60 0 0 -
";

/// Grapple from the Outskirts ledge by the pine's root, north of the ravine, to the far end of
/// the branch over it, then, perched, turn the camera a full circle from step 400
/// (`--live-orbit`). The branch stands on no map collision (only the event walls h900000 and
/// h900001 are below), so the map must stay on the ledge's collision part, whichever way the
/// camera looks. The spawn faces the far hook; the earlier one, east of the ravine at
/// (-132.3, -42.0, -123.9), has had no floor since #189 (#231).
const GRAPPLE_SPAWN: &str = "-136.5,-42.54,-118.0,41,0";
const GRAPPLE: &str = "120 0 0 -
1 0 0 grapple
240 0 0 -
720 0 0 -
";
/// The ledge's collision part, which must select the map from spawn to the end of the turn.
const LEDGE_PART: &str = "m11_00_00_00/h023300";

/// From the lower ground south of the ravine, grapple to the rideable pine `o002500_0005`
/// (`docs/OBJECT-COLLISION.md`) and walk its trunk: down to the tip over the ravine, back, and
/// up to the platform it leads to. The script idles 7 s first (the map and collision are loaded
/// before play starts; Wolf stands grounded through the wait), then grapples and lets the
/// landing finish; `--script-route` steers the walk along the trunk's top.
const TRUNK_SPAWN: &str = "-140,-50,-132,180,0";
const TRUNK_SCRIPT: &str = "420 0 0 -
1 0 0 grapple
420 0 0 -
";
const TRUNK_ROUTE: &str = "-136.2,-121.75;-136.4,-122.0;-136.6,-122.5;-136.9,-122.75;-137.1,-123.0;-137.4,-123.25;-137.75,-123.5;-138.0,-123.75;-138.6,-124.0;-139.4,-124.1;-140.4,-124.15;-139.4,-124.1;-138.6,-124.0;-138.0,-123.75;-137.75,-123.5;-137.4,-123.25;-137.1,-123.0;-136.9,-122.75;-136.6,-122.5;-136.4,-122.0;-136.2,-121.75;-136.1,-121.25;-136.05,-121.0;-136.15,-120.5;-136.25,-120.0;-136.35,-119.5;-136.15,-119.0;-136.0,-118.5;-135.9,-117.5";
const TREE: &str = "m11_00_00_00/o002500_0005";

/// Runs Explore with `--live-check`, from `root` with the extracted `cache`.
fn run(root: &Path, cache: &Path, out: &Path, name: &str, extra: &[&str]) -> Value {
    let summary = out.join(format!("{name}.json"));
    let status = Command::new(env!("CARGO_BIN_EXE_sekiro-rs"))
        .current_dir(root)
        .args(EXPLORE)
        .arg("--cache")
        .arg(cache)
        .arg("--live-check")
        .arg(&summary)
        .args(extra)
        .status()
        .expect("start the game");
    assert!(status.success(), "{name}: the game exited with {status}");
    serde_json::from_slice(&std::fs::read(&summary).expect("live check summary")).unwrap()
}

/// `--play` with no `--map` is the flat-floor fight (`docs/GETTING-STARTED.md`). It panicked
/// at startup on a map-only resource (issue #139); the fight must load and play for 10 s.
#[test]
#[ignore = "slow: opens a game window for about 25 s; needs a GPU and the extracted cache"]
fn flat_floor_fight_starts_without_a_map() {
    let cache = sekiro_sim::cache::root();
    if !cache.join("models").is_dir() {
        eprintln!("skipped: no extracted cache");
        return;
    }
    let out = std::env::temp_dir().join(format!("sekiro-live-floor-{}", std::process::id()));
    std::fs::create_dir_all(&out).unwrap();
    let summary = out.join("floor.json");
    let status = Command::new(env!("CARGO_BIN_EXE_sekiro-rs"))
        .args(["--play", "--passive", "--break-posture", "--lock-on"])
        .arg("--cache")
        .arg(&cache)
        .arg("--live-check")
        .arg(&summary)
        .args(["--live-seconds", "10"])
        .status()
        .expect("start the game");
    assert!(status.success(), "the game exited with {status}");
    let s: Value = serde_json::from_slice(&std::fs::read(&summary).unwrap()).unwrap();
    let samples = s["samples"].as_array().expect("samples");
    assert!(
        samples.len() >= 30 && samples.iter().all(|v| v["grounded"] == true),
        "Wolf did not stand on the floor for 10 s of play: {s:#}"
    );
    println!(
        "floor: ready {:.1} s, {} samples, frames median {:.1} ms",
        number(&s, "play_ready_s"),
        samples.len(),
        number(&s, "frame_ms_median")
    );
}

fn number(summary: &Value, key: &str) -> f64 {
    summary[key]
        .as_f64()
        .unwrap_or_else(|| panic!("summary has no {key}: {summary:#}"))
}

#[test]
#[ignore = "slow: opens a game window and loads Ashina Outskirts five times, about 9 minutes; needs a GPU and the extracted cache"]
fn outskirts_loads_then_plays_smoothly_on_drawn_ground() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    // `SEKIRO_CACHE` or the checkout's own: a second clone has no `cache/` of its own.
    let cache = sekiro_sim::cache::root();
    if !cache.join("maps/m11_00_00_00/layout.json").is_file() {
        eprintln!("skipped: Ashina Outskirts is not extracted");
        return;
    }
    let out = std::env::temp_dir().join(format!("sekiro-live-play-{}", std::process::id()));
    std::fs::create_dir_all(&out).unwrap();
    let walk = out.join("walk.txt");
    std::fs::write(&walk, WALK).unwrap();
    let grapple = out.join("grapple.txt");
    std::fs::write(&grapple, GRAPPLE).unwrap();
    let trunk = out.join("trunk.txt");
    std::fs::write(&trunk, TRUNK_SCRIPT).unwrap();

    // A scripted walk from the menu's start, timed from launch.
    let s = run(
        &root,
        &cache,
        &out,
        "walk",
        &["--script", walk.to_str().unwrap()],
    );
    println!(
        "walk: ready {:.1} s, longest loading frame {:.0} ms, frames median {:.1} ms p95 {:.1} ms, holes {}/{}",
        number(&s, "play_ready_s"),
        number(&s, "longest_loading_frame_ms"),
        number(&s, "frame_ms_median"),
        number(&s, "frame_ms_p95"),
        number(&s, "ground_holes"),
        number(&s, "grounded_samples")
    );
    assert!(number(&s, "play_ready_s") <= READY_SECONDS, "slow load");
    assert!(
        number(&s, "longest_loading_frame_ms") <= LOADING_FRAME_MS,
        "the window stalls while loading"
    );
    assert!(
        number(&s, "frame_ms_median") <= FRAME_MEDIAN_MS,
        "low frame rate"
    );
    assert!(
        number(&s, "frame_ms_p95") <= FRAME_P95_MS,
        "frame spikes in play"
    );
    assert!(number(&s, "grounded_samples") >= 20.0, "Wolf never walked");
    assert_eq!(number(&s, "ground_holes"), 0.0, "walked on undrawn ground");
    assert!(
        number(&s, "floor_coverage_min") >= 0.9,
        "floor around Wolf undrawn"
    );
    let start = s["samples"][0]["position"].clone();
    let end = &s["final_position"];
    let moved = (0..3)
        .map(|i| (end[i].as_f64().unwrap() - start[i].as_f64().unwrap()).powi(2))
        .sum::<f64>()
        .sqrt();
    assert!(moved > 3.0, "the walk did not move Wolf ({moved:.1} m)");

    // Every authored player start: drawn ground under Wolf after a teleport.
    let s = run(&root, &cache, &out, "starts", &["--live-tour", "starts"]);
    let stops = s["stops"].as_array().expect("tour stops");
    assert!(stops.len() >= 9, "player starts missing from the tour");
    for stop in stops {
        assert!(stop["floor"].is_number(), "no collision floor at {stop}");
        assert_eq!(
            stop["ground_drawn"].as_bool(),
            Some(true),
            "undrawn ground at {stop}"
        );
    }

    // Far into Ashina Castle (block m11_01), 300 to 600 m from the start: the connected blocks
    // draw where Wolf stands after loading from the Outskirts.
    let s = run(&root, &cache, &out, "castle", &["--live-tour", CASTLE]);
    let stops = s["stops"].as_array().expect("tour stops");
    assert_eq!(stops.len(), 7, "tour stops missing");
    for stop in stops {
        assert_eq!(
            stop["ground_drawn"].as_bool(),
            Some(true),
            "undrawn ground at {stop}"
        );
    }
    // The ravine drop: through h900000 (y -74.2), never standing on it, then back on the
    // last stop's ground.
    let drop = &stops[6];
    let events = drop["helper_events"].as_array().expect("helper events");
    let wall = events
        .iter()
        .find(|e| e["part"] == "h900000")
        .unwrap_or_else(|| panic!("the drop never met h900000: {drop}"));
    assert_eq!(wall["hit_filter"].as_u64(), Some(15), "{drop}");
    let back: Vec<f64> = wall["returned_to"]
        .as_array()
        .expect("no fall return")
        .iter()
        .map(|v| v.as_f64().unwrap())
        .collect();
    let last = stops[5]["position"].as_array().unwrap();
    for i in 0..3 {
        let (a, b) = (back[i], last[i].as_f64().unwrap());
        assert!(
            (a - b).abs() < 0.5,
            "returned to {back:?}, last stop {last:?}"
        );
    }
    assert!(
        number(drop, "lowest_y") < -75.0,
        "Wolf stopped on the helper: {drop}"
    );
    assert_eq!(drop["grounded"].as_bool(), Some(true), "{drop}");
    println!(
        "ravine drop: fell to {:.1}, contacts {}, back at {back:.2?}",
        number(drop, "lowest_y"),
        events.len()
    );

    // The grapple to the branch and a camera turn there: the Outskirts block keeps its display
    // groups and the map around Wolf stays drawn at every camera yaw.
    let s = run(
        &root,
        &cache,
        &out,
        "grapple",
        &[
            "--spawn",
            GRAPPLE_SPAWN,
            "--script",
            grapple.to_str().unwrap(),
            "--live-orbit",
            "400,8",
        ],
    );
    assert_eq!(
        s["orbit_turned"].as_bool(),
        Some(true),
        "the camera never turned"
    );
    let end = &s["final_position"];
    let perched = [[-137.58, -44.30, -124.49], [-141.58, -44.26, -123.86]]
        .iter()
        .any(|p| (0..3).all(|i| (end[i].as_f64().unwrap() - p[i]).abs() < 0.5));
    assert!(perched, "Wolf did not reach the branch: {end}");
    let changes = s["block_changes"].as_array().expect("block changes");
    for change in changes {
        if change["block"] == "m11_00_00_00" {
            assert_eq!(
                change["state"], "Ready",
                "the Outskirts block lost its groups: {change}"
            );
        }
        assert_eq!(
            change["selected"], LEDGE_PART,
            "the map left the ledge's part: {change}"
        );
    }
    assert_eq!(
        s["block_states"]["m11_00_00_00"], "Ready",
        "the Outskirts block is not drawn at the end"
    );
    assert_eq!(
        s["selections"],
        serde_json::json!([LEDGE_PART]),
        "the map left the ledge's part"
    );
    let samples = s["samples"].as_array().expect("samples");
    let near: Vec<u64> = samples
        .iter()
        .map(|v| v["drawn_meshes_near"].as_u64().unwrap())
        .collect();
    assert!(near.len() >= 40, "too few samples: {near:?}");
    assert!(
        near.iter().all(|&n| n >= 100),
        "the map around Wolf vanished: drawn meshes near {near:?}"
    );
    println!(
        "grapple: perched at {end}, {} samples, fewest drawn meshes near {}",
        near.len(),
        near.iter().min().unwrap()
    );

    // Grapple to the pine and walk its trunk: Wolf stands on the tree's own collision all the
    // way to the tip and back up to the platform, never falling into the ravine.
    let s = run(
        &root,
        &cache,
        &out,
        "trunk",
        &[
            "--spawn",
            TRUNK_SPAWN,
            "--script",
            trunk.to_str().unwrap(),
            "--script-route",
            TRUNK_ROUTE,
        ],
    );
    assert!(
        number(&s, "map_loaded_s") <= number(&s, "sim_started_s"),
        "play started before the map loaded"
    );
    let samples = s["samples"].as_array().expect("samples");
    let at = |v: &Value| -> [f64; 3] {
        let p = v["position"].as_array().unwrap();
        [0, 1, 2].map(|i| p[i].as_f64().unwrap())
    };
    let before: Vec<&Value> = samples
        .iter()
        .filter(|v| v["step"].as_u64().is_some_and(|n| n < 420))
        .collect();
    assert!(
        before.len() >= 20 && before.iter().all(|v| v["grounded"] == true),
        "Wolf was not standing through the wait before the grapple"
    );
    let mut on_tree = 0.0;
    let mut last: Option<[f64; 3]> = None;
    for v in samples {
        let p = at(v);
        let tree = v["grounded"] == true && v["floor_object"] == TREE;
        if let Some(l) = last.filter(|_| tree) {
            on_tree += ((p[0] - l[0]).powi(2) + (p[2] - l[2]).powi(2)).sqrt();
        }
        last = tree.then_some(p);
        if v["step"].as_u64().is_some_and(|n| n > 560) {
            assert!(p[1] > -45.0, "Wolf fell off the trunk: {v}");
        }
    }
    let end: Vec<f64> = s["final_position"]
        .as_array()
        .expect("final position")
        .iter()
        .map(|v| v.as_f64().unwrap())
        .collect();
    let end_ok = (end[0] + 135.9).abs() < 0.6 && (end[2] + 117.5).abs() < 0.6;
    assert!(end_ok, "Wolf did not reach the platform: {end:?}");
    assert!(on_tree > 10.0, "only {on_tree:.1} m walked on the trunk");
    println!("trunk: {on_tree:.1} m on {TREE}, ended at {end:?}");
}
