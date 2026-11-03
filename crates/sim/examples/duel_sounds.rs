//! Runs the built-in scripted duel and logs every sound cue it fires, marking whether the
//! decoded banks in `cache/sound` have it.
//!
//! cargo run -p sekiro-sim --example duel_sounds --release

use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;

use sekiro_sim::demo;
use sekiro_sim::duel::Duel;
use sekiro_sim::sound::{HitSounds, tae_cues};

fn main() {
    let cache = std::env::args()
        .nth(1)
        .map_or_else(sekiro_sim::cache::root, PathBuf::from);
    let mut known = HashSet::new();
    if let Ok(dirs) = std::fs::read_dir(cache.join("sound")) {
        for d in dirs.flatten() {
            if let Ok(text) = std::fs::read_to_string(d.path().join("events.json")) {
                let map: BTreeMap<String, serde_json::Value> =
                    serde_json::from_str(&text).unwrap_or_default();
                known.extend(map.into_keys());
            }
        }
    }
    let hit_sounds = HitSounds::load_from_cache(&cache, 10_100_000).expect("params");
    let mut duel = Duel::load(&cache, 6.0).expect("duel");
    let frames = demo::parse(demo::DEMO).expect("demo script");
    let dt = 1.0 / 30.0;
    let (mut fired, mut playable) = (0, 0);
    let mut by_name: BTreeMap<String, (usize, bool)> = BTreeMap::new();
    for (step, frame) in frames.iter().enumerate() {
        let report = duel.step(frame, dt);
        let mut cues = tae_cues(0, &duel.player, hit_sounds.materials[0]);
        cues.extend(tae_cues(1, &duel.enemy, hit_sounds.materials[1]));
        for (hit, res) in &report.hits {
            for c in hit_sounds.cues(hit, res) {
                cues.push(c);
            }
        }
        for c in cues {
            let name = c.keys[0].clone();
            let ok = c.keys.iter().any(|k| known.contains(k));
            fired += 1;
            playable += ok as usize;
            println!(
                "step {step:4} {} {} {}",
                ["wolf   ", "soldier"][c.actor],
                name,
                if ok { "" } else { "(not in decoded banks)" }
            );
            let e = by_name.entry(name).or_insert((0, ok));
            e.0 += 1;
        }
    }
    println!("{fired} cues fired, {playable} playable from the decoded banks");
    for (name, (n, ok)) in by_name {
        println!("  {name} x{n}{}", if ok { "" } else { " missing" });
    }
}
