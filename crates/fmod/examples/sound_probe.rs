//! Sound coverage of the Owl fight through the game's own FMOD (silent output).
//!
//! 1. Loads the fight's event projects from `cache/raw/sound` and prints their event counts.
//! 2. Census: every TAE sound event of Wolf (c0000) and the Owl (c5060), keyed as the game
//!    keys them (floor and armour materials included), and whether FMOD has and starts it.
//! 3. A headless fight on the castle top (`--seconds`, default 20): Wolf locks on, walks in,
//!    attacks, blocks and tries to deflect while the Owl's own AI fights; every sound cue is
//!    logged with what FMOD did with it.
//!
//! cargo run -p sekiro-fmod --example sound_probe --release -- [--seconds 20] [--quiet]
//! Run from the repo root; the install comes from `--game` or `SEKIRO_DIR`.

use std::collections::BTreeMap;
use std::path::Path;

use sekiro_fmod::{EventSystem, Played, fight_projects, game_dir};
use sekiro_sim::arena::{Arena, ArenaSpec, yaw_toward};
use sekiro_sim::duel::Duel;
use sekiro_sim::input::{Buttons, InputFrame};
use sekiro_sim::sound::{CueSource, HitSounds, SoundCue, tae_cues, tae_keys};
use sekiro_sim::tae::{TaeDb, TaeKind};

const STEP_HZ: f32 = 60.0;

fn arg(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    let i = args.iter().position(|a| a == name)?;
    args.get(i + 1).cloned()
}

/// Plays the first key FMOD has; the key used and the outcome.
fn play(fmod: &mut EventSystem, cue: &SoundCue) -> (String, Played) {
    for k in &cue.keys {
        if fmod.has(k) {
            return (k.clone(), fmod.play(k, cue.position));
        }
    }
    (cue.keys[0].clone(), Played::Unknown)
}

fn outcome(p: Played) -> String {
    match p {
        Played::Started => "ok".into(),
        Played::Unknown => "NO EVENT".into(),
        Played::Failed(c) => format!("FAILED {c}"),
    }
}

fn main() {
    let cache = sekiro_formats::cache::root();
    let seconds: f32 = arg("--seconds")
        .and_then(|s| s.parse().ok())
        .unwrap_or(20.0);
    let quiet = std::env::args().any(|a| a == "--quiet");
    let game = game_dir();
    let mut fmod = EventSystem::open(&game, true).expect("FMOD from the install");
    fmod.set_media_path(&cache.join("raw/sound")).unwrap();
    println!("== projects (FMOD from {})", game.display());
    for p in fight_projects("c5060", "m11") {
        match fmod.load_project(&format!("{p}.fev")) {
            Ok(n) => println!("{p}: {n} events"),
            Err(e) => println!("{p}: not loaded ({e})"),
        }
    }
    if std::env::args().any(|a| a == "--preload") {
        for p in ["main", "c5060", "m11"] {
            let t = std::time::Instant::now();
            let r = fmod.preload(p);
            println!("preload {p}: {r:?} in {:.2} s", t.elapsed().as_secs_f32());
        }
    }

    let spec = ArenaSpec::by_name("owl").unwrap();
    let arena = Arena::load(&cache, spec, false).expect("arena (sekiro-extract map)");
    let hit_sounds = HitSounds::load_from_cache(&cache, 50_600_000).expect("params");
    let floor_at = |p: glam::Vec3| arena.world.floor_material(p.x, p.z, p.y + 1.0);
    let floor = floor_at(arena.layout.boss.position).unwrap_or(1);
    println!(
        "floor material: Owl's spot {:?}, Wolf's spot {:?}; materials Wolf {:?}, Owl {:?}",
        floor_at(arena.layout.boss.position),
        floor_at(arena.layout.wolf.position),
        hit_sounds.materials[0],
        hit_sounds.materials[1]
    );

    println!("\n== TAE census (floor material {floor})");
    for (chr, actor) in [("c0000", 0), ("c5060", 1)] {
        let db = TaeDb::load_from_cache(&cache, chr);
        // letter -> (events, unique keys, keys found, keys started)
        let mut by_letter: BTreeMap<char, (usize, BTreeMap<String, Played>)> = BTreeMap::new();
        let mut gated = 0;
        let mut slowest = 0.0f32;
        for id in db.anim_ids() {
            let name = sekiro_formats::tae::anim_name(id);
            let Some(anim) = db.anim(&name) else { continue };
            for e in anim.events.iter() {
                let TaeKind::Sound { kind, id } = e.kind else {
                    continue;
                };
                if e.state_info != 0 {
                    gated += 1;
                    continue;
                }
                let keys = tae_keys(kind, id, floor, hit_sounds.materials[actor]);
                if keys.is_empty() {
                    continue;
                }
                let letter = sekiro_sim::tae::sound_name(kind, id)
                    .chars()
                    .next()
                    .unwrap();
                let cue = SoundCue {
                    keys,
                    actor,
                    position: [0.0; 3],
                    source: CueSource::Tae,
                };
                let entry = by_letter.entry(letter).or_default();
                entry.0 += 1;
                let first = cue.keys[0].clone();
                if let std::collections::btree_map::Entry::Vacant(slot) = entry.1.entry(first) {
                    let t = std::time::Instant::now();
                    let (_, r) = play(&mut fmod, &cue);
                    slowest = slowest.max(t.elapsed().as_secs_f32());
                    slot.insert(r);
                }
            }
            fmod.update();
        }
        for (letter, (events, keys)) in &by_letter {
            let started = keys.values().filter(|r| **r == Played::Started).count();
            let failed: Vec<&String> = keys
                .iter()
                .filter(|(_, r)| matches!(r, Played::Failed(_)))
                .map(|(k, _)| k)
                .collect();
            let missing: Vec<&String> = keys
                .iter()
                .filter(|(_, r)| **r == Played::Unknown)
                .map(|(k, _)| k)
                .collect();
            println!(
                "{chr} '{letter}': {events} events, {} keys, {started} play, {} fail, {} no event{}",
                keys.len(),
                failed.len(),
                missing.len(),
                if missing.is_empty() && failed.is_empty() {
                    String::new()
                } else {
                    format!(
                        " (e.g. {:?} {:?})",
                        &missing[..missing.len().min(6)],
                        &failed[..failed.len().min(4)]
                    )
                }
            );
        }
        println!("{chr}: slowest first play {:.1} ms", slowest * 1000.0);
        println!("{chr}: {gated} gated events skipped (StateInfo, off by default)");
    }

    println!("\n== loudness by distance (summed channel audibility, listener at the origin)");
    fmod.set_listener([0.0; 3], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]);
    let distances = [0.5f32, 1.0, 2.0, 3.0, 5.0, 8.0, 12.0, 20.0, 30.0];
    println!("{:>11} {distances:?}", "metres");
    for key in [
        "c000004010",
        "c000006510",
        "c000001028",
        "z199999980",
        "z000000013",
        "c506001001",
        "c506005000",
    ] {
        let row: Vec<String> = distances
            .iter()
            .map(|&d| {
                // Average a few starts: events pick random variations and volumes.
                let a: f32 = (0..4)
                    .filter_map(|_| fmod.audibility(key, [0.0, 0.0, -d]))
                    .sum::<f32>()
                    / 4.0;
                format!("{a:.2}")
            })
            .collect();
        let props: Vec<String> = fmod
            .properties(key)
            .into_iter()
            .filter(|(n, _)| {
                let n = n.to_ascii_lowercase();
                n.contains("3d") || n.contains("distance") || n.contains("volume")
            })
            .map(|(n, v)| format!("{n}={v}"))
            .collect();
        println!("{key:>11} [{}]  {}", row.join(", "), props.join(" "));
    }

    println!("\n== headless fight, {seconds} s");
    let mut duel = Duel::load_with(&cache, 6.0, spec.enemy_spec()).expect("duel");
    arena.place(&mut duel);
    let dt = 1.0 / STEP_HZ;
    let steps = (seconds * STEP_HZ) as usize;
    // (source, key) -> (count, outcome)
    let mut tally: BTreeMap<(String, String), (usize, String)> = BTreeMap::new();
    let mut fired = 0;
    let mut started = 0;
    for step in 0..steps {
        let p = glam::Vec3::from(duel.player.body.position);
        let e = glam::Vec3::from(duel.enemy.body.position);
        let dist = p.with_y(0.0).distance(e.with_y(0.0));
        let phase = step % 240;
        let buttons = Buttons {
            lock_on: step < 3,
            attack: phase < 60 && phase % 15 < 3,
            guard: (60..150).contains(&phase) || (phase >= 150 && phase % 20 < 2),
            ..Default::default()
        };
        let frame = InputFrame {
            move_stick: if dist > 2.5 && phase < 60 {
                [0.0, 1.0]
            } else {
                [0.0, 0.0]
            },
            camera_yaw: yaw_toward(p, e),
            camera_pitch: 0.0,
            buttons,
        };
        let report = duel.step(&frame, dt);
        let mut cues = tae_cues(0, &duel.player, hit_sounds.materials[0]);
        cues.extend(tae_cues(1, &duel.enemy, hit_sounds.materials[1]));
        for (hit, res) in &report.hits {
            cues.extend(hit_sounds.cues(hit, res));
        }
        for c in &cues {
            let (key, r) = play(&mut fmod, c);
            fired += 1;
            started += (r == Played::Started) as usize;
            if !quiet {
                println!(
                    "{:6.2}s {:5} {:7} {key} {}",
                    step as f32 * dt,
                    ["wolf", "owl"][c.actor],
                    format!("{:?}", c.source),
                    outcome(r)
                );
            }
            let t = tally
                .entry((format!("{:?} {}", c.source, ["wolf", "owl"][c.actor]), key))
                .or_insert((0, outcome(r)));
            t.0 += 1;
        }
        fmod.update();
    }
    println!("{fired} cues fired, {started} started in FMOD");
    let mut last = String::new();
    for ((group, key), (n, r)) in &tally {
        if *group != last {
            println!("{group}:");
            last = group.clone();
        }
        println!("  {key} x{n} {r}");
    }
    if !fmod.broken().is_empty() {
        let mut b: Vec<_> = fmod.broken().iter().collect();
        b.sort();
        println!("events FMOD could not start: {b:?}");
    }
    let _ = Path::new("");
}
