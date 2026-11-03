//! Checks exported player starts and short walks using the same body collision as play mode.
//! Usage: `map_baseline [cache/maps] [map ids...]`; emits JSON for a reproducible map audit.

use std::path::PathBuf;

use sekiro_sim::body::{Body, BodyInput};
use sekiro_sim::collision::CollisionWorld;
use serde_json::{Value, json};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let root = PathBuf::from(args.next().unwrap_or_else(|| "cache/maps".into()));
    let mut ids: Vec<String> = args.collect();
    if ids.is_empty() {
        ids = std::fs::read_dir(&root)?
            .filter_map(Result::ok)
            .filter(|e| e.path().join("layout.json").is_file())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
    }
    ids.sort();
    for id in ids {
        let dir = root.join(&id);
        let layout: Value = serde_json::from_slice(&std::fs::read(dir.join("layout.json"))?)?;
        let world = CollisionWorld::load_map(&dir)?;
        let mut starts = Vec::new();
        for (index, p) in layout["players"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
        {
            let v = &p["translation"];
            let pos = [
                v[0].as_f64().unwrap() as f32,
                v[1].as_f64().unwrap() as f32,
                v[2].as_f64().unwrap() as f32,
            ];
            // Match play::place_pair's floor search, including its one-metre spawn tolerance.
            let floor = world.floor(pos[0], pos[2], pos[1] + 1.0);
            let mut walks = Vec::new();
            if let Some(h) = floor {
                for offset in [0.0_f32, 90.0, 180.0, 270.0] {
                    let yaw =
                        (p["yaw_degrees"].as_f64().unwrap_or(0.0) as f32 + offset).to_radians();
                    let mut body = Body {
                        position: [pos[0], h, pos[2]],
                        yaw,
                        ..Body::default()
                    };
                    for _ in 0..119 {
                        body.step(&BodyInput::default(), 1.0 / 60.0, &world);
                    }
                    let settled = body.position;
                    body.step(&BodyInput::default(), 1.0 / 60.0, &world);
                    let stable = body.grounded && (body.position[1] - settled[1]).abs() < 0.01;
                    let mut below_floor = 0;
                    let mut airborne = 0;
                    for _ in 0..120 {
                        body.step(
                            &BodyInput {
                                root_motion: [0.0, 0.0, -0.0125, 0.0],
                                ..BodyInput::default()
                            },
                            1.0 / 60.0,
                            &world,
                        );
                        if !body.grounded {
                            airborne += 1;
                        }
                        // A ledge with no nearby supporting surface is not a collision hole.
                        if world
                            .floor(body.position[0], body.position[2], body.position[1] + 0.5)
                            .is_some_and(|y| y > body.position[1] + 0.05)
                        {
                            below_floor += 1;
                        }
                    }
                    walks.push(json!({"yaw_offset":offset,"stable_standing":stable,"end":body.position,"grounded":body.grounded,"airborne_ticks":airborne,"below_floor_ticks":below_floor,"vertical_drop":h-body.position[1]}));
                }
            }
            starts.push(json!({"index":index,"name":p["name"],"position":pos,"floor":floor,"floor_gap":floor.map(|h| h-pos[1]),"walks":walks}));
        }
        println!(
            "{}",
            json!({"map":id,"triangles":world.triangle_count(),"starts":starts})
        );
    }
    Ok(())
}
