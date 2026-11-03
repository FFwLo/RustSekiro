//! `fmod_list [project...] [--events] [--has key...]`: loads event projects through the game's
//! FMOD (silent) and prints how many events each has (every event path with `--events`), then
//! whether each `--has` key exists and starts. Without projects it loads the Owl fight's.
//! Run from the repo root; the install comes from `--game` or `SEKIRO_DIR`, the banks from
//! `cache/raw/sound` (or the `FMOD_MEDIA` folder).

use std::path::Path;

use sekiro_fmod::{EventSystem, Played, fight_projects, game_dir};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let split = args.iter().position(|a| a == "--has").unwrap_or(args.len());
    let mut projects: Vec<String> = Vec::new();
    let mut skip_next = false;
    for a in &args[..split] {
        if std::mem::take(&mut skip_next) {
            continue;
        }
        if a == "--game" {
            skip_next = true;
        } else if !a.starts_with("--") {
            projects.push(a.clone());
        }
    }
    if projects.is_empty() {
        projects = fight_projects("c5060", "m11");
    }
    let mut fmod = EventSystem::open(&game_dir(), true).expect("FMOD");
    let media = std::env::var("FMOD_MEDIA").unwrap_or_else(|_| "cache/raw/sound".into());
    fmod.set_media_path(Path::new(&media)).unwrap();
    for p in &projects {
        let file = if p.ends_with(".fev") {
            p.clone()
        } else {
            format!("{p}.fev")
        };
        match fmod.load_project(&file) {
            Ok(n) => println!("{p}: {n} events"),
            Err(e) => println!("{p}: not loaded ({e})"),
        }
    }
    if args[..split].iter().any(|a| a == "--events") {
        for p in fmod.projects() {
            for e in &p.events {
                println!("{e}");
            }
        }
    }
    for key in &args[(split + 1).min(args.len())..] {
        let r = fmod.play(key, [0.0; 3]);
        println!("{key}: {:?} {r:?}", fmod.path_of(key));
        if r == Played::Started {
            fmod.update();
        }
    }
}
