//! The body on the real m11_00_00_00 hit collision (`cache/maps/m11_00_00_00`, written by
//! `sekiro-extract map m11_00_00_00`). Skips when the map has not been exported.

use std::path::PathBuf;
use std::time::Instant;

use sekiro_sim::body::{Body, BodyInput, Ground};
use sekiro_sim::collision::CollisionWorld;

fn map_dir() -> PathBuf {
    sekiro_sim::cache::root().join("maps/m11_00_00_00")
}

#[test]
fn player_starts_stand_on_the_map() {
    let dir = map_dir();
    if !dir.join("collision.bin").is_file() {
        eprintln!("skipping: {} not exported", dir.display());
        return;
    }
    let t = Instant::now();
    let world = CollisionWorld::load_map(&dir).unwrap();
    eprintln!(
        "{} triangles indexed in {:.2} s",
        world.triangle_count(),
        t.elapsed().as_secs_f32()
    );
    let layout: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("layout.json")).unwrap()).unwrap();
    for p in layout["players"].as_array().unwrap() {
        let v: Vec<f32> = p["translation"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_f64().unwrap() as f32)
            .collect();
        // The floor as the body sees it (sampled around the feet).
        let floor = Ground::floor(&world, v[0], v[2], v[1] + 0.5);
        let name = p["name"].as_str().unwrap();
        let h = floor.unwrap_or_else(|| panic!("{name}: no floor under {v:?}"));
        assert!(
            (h - v[1]).abs() < 0.6,
            "{name}: floor {h} vs start {}",
            v[1]
        );

        // Standing still for two seconds keeps the body on the floor.
        let mut b = Body {
            position: [v[0], h, v[2]],
            ..Body::default()
        };
        for _ in 0..120 {
            b.step(&BodyInput::default(), 1.0 / 60.0, &world);
        }
        assert!(b.grounded, "{name}: {:?}", b.position);
        assert!((b.position[1] - h).abs() < 1e-3, "{name}");
        assert!(world.height(v[0], v[2]).is_some());
    }
}

/// Prints the surfaces under a point given by MAP_PROBE="x,z" (debugging aid).
#[test]
fn probe_point() {
    let Ok(arg) = std::env::var("MAP_PROBE") else {
        return;
    };
    let v: Vec<f32> = arg.split(',').map(|s| s.trim().parse().unwrap()).collect();
    let world = CollisionWorld::load_map(&map_dir()).unwrap();
    for (h, ny) in world.surfaces_at(v[0], v[1]) {
        eprintln!("surface at {h:.3}, normal y {ny:.3}");
    }
}

/// Walks a body from MAP_WALK="x,y,z,dx,dz,ticks" (world-space step per tick), printing it.
#[test]
fn probe_walk() {
    let Ok(arg) = std::env::var("MAP_WALK") else {
        return;
    };
    let v: Vec<f32> = arg.split(',').map(|s| s.trim().parse().unwrap()).collect();
    let world = CollisionWorld::load_map(&map_dir()).unwrap();
    let mut b = Body {
        position: [v[0], v[1], v[2]],
        ..Body::default()
    };
    // Face the walking direction; root motion is forward (source -Z).
    b.yaw = (-v[3]).atan2(-v[4]);
    let len = (v[3] * v[3] + v[4] * v[4]).sqrt();
    for i in 0..v[5] as usize {
        b.step(
            &BodyInput {
                root_motion: [0.0, 0.0, -len, 0.0],
                ..BodyInput::default()
            },
            1.0 / 60.0,
            &world,
        );
        eprintln!(
            "{i}: {:.3?} {}",
            b.position,
            if b.grounded { "" } else { "AIR" }
        );
    }
}

/// Lists ledges 0.8 to 1.6 m high near MAP_LEDGES="x,y,z" (debugging aid): low point, high
/// point, height difference.
#[test]
fn probe_ledges() {
    let Ok(arg) = std::env::var("MAP_LEDGES") else {
        return;
    };
    let v: Vec<f32> = arg.split(',').map(|s| s.trim().parse().unwrap()).collect();
    let world = CollisionWorld::load_map(&map_dir()).unwrap();
    let top = v[1] + 3.0;
    // Highest flat (within about 18 degrees) surface under the point.
    let flat = |x: f32, z: f32| {
        world
            .surfaces_at(x, z)
            .into_iter()
            .find(|(h, _)| *h <= top)
            .filter(|(_, ny)| ny.abs() > 0.95)
            .map(|(h, _)| h)
    };
    let mut found = 0;
    for i in -40..=40 {
        for j in -40..=40 {
            let (x, z) = (v[0] + i as f32 * 0.5, v[2] + j as f32 * 0.5);
            let Some(lo) = world.floor(x, z, top) else {
                continue;
            };
            if (lo - v[1]).abs() > 0.6 {
                continue;
            }
            for (dx, dz) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
                if let Some(hi) = flat(x + dx, z + dz)
                    && (0.8..1.6).contains(&(hi - lo))
                    && found < 30
                {
                    found += 1;
                    eprintln!(
                        "ledge from [{x}, {lo:.2}, {z}] toward [{dx}, {dz}]: +{:.2}",
                        hi - lo
                    );
                }
            }
        }
    }
}

/// Steers a body through `waypoints` with forward root motion of `speed` metres per tick,
/// checking every tick that it is not below a walkable floor it should stand on. Returns the
/// final position.
fn walk_route(world: &CollisionWorld, waypoints: &[[f32; 3]], speed: f32) -> [f32; 3] {
    let mut b = Body {
        position: waypoints[0],
        ..Body::default()
    };
    let mut worst = 0.0f32;
    for (wi, w) in waypoints.iter().enumerate().skip(1) {
        let mut ticks = 0;
        loop {
            let (dx, dz) = (w[0] - b.position[0], w[2] - b.position[2]);
            if (dx * dx + dz * dz).sqrt() < 0.6 {
                break;
            }
            ticks += 1;
            assert!(
                ticks < 1200,
                "stuck before waypoint {wi} {w:?} at {:?}",
                b.position
            );
            // Face the waypoint (forward is (-sin yaw, -cos yaw)).
            b.yaw = (-dx).atan2(-dz);
            b.step(
                &BodyInput {
                    root_motion: [0.0, 0.0, -speed, 0.0],
                    ..BodyInput::default()
                },
                1.0 / 60.0,
                world,
            );
            let p = b.position;
            if b.grounded {
                // Grounded means standing on the highest floor within a step: nothing walkable
                // may be between the feet and a step above them.
                let above = world.floor(p[0], p[2], p[1] + 0.45).unwrap_or(p[1]);
                worst = worst.max(above - p[1]);
                assert!(
                    above - p[1] < 0.03,
                    "grounded {:.3} m below the floor at {p:?} (waypoint {wi})",
                    above - p[1]
                );
            }
            assert!(p[1] > -62.0, "fell out of the gate area at {p:?}");
        }
    }
    eprintln!("route done at {:?}, worst gap {worst:.4} m", b.position);
    b.position
}

/// From player start 3 at the Ashina Castle gate: off the start terrace (a 4 m drop), through
/// the moat gate, up the long stairs to the closed castle gate; then back down the stairs.
/// Walked at run and sprint speeds.
#[test]
fn castle_gate_route_never_sinks_below_the_floor() {
    let dir = map_dir();
    if !dir.join("collision.bin").is_file() {
        eprintln!("skipping: {} not exported", dir.display());
        return;
    }
    let world = CollisionWorld::load_map(&dir).unwrap();
    let up = [
        [-197.45, -48.16, 150.72],
        [-190.77, -47.98, 146.37],
        [-183.98, -59.16, 144.36],
        [-180.63, -59.20, 138.53],
        [-176.32, -58.40, 136.84],
        [-157.75, -52.73, 131.63],
        [-152.47, -51.29, 130.56],
        [-148.38, -49.97, 133.46],
        [-143.62, -47.79, 136.40],
        [-138.94, -45.94, 139.30],
        [-129.50, -45.19, 145.13],
        [-125.60, -43.85, 147.40],
    ];
    let down: Vec<[f32; 3]> = up[3..].iter().rev().copied().collect();
    for speed in [0.08, 0.13] {
        let end = walk_route(&world, &up, speed);
        assert!(end[1] > -44.5, "reached the gate level: {end:?}");
        let back = walk_route(&world, &down, speed);
        assert!((back[1] + 59.2).abs() < 0.5, "back at the moat: {back:?}");
    }
}

/// #146: past the Outskirts Wall - Gate Path idol, Wolf walks through the hole in the broken
/// wooden wall (map part h004810) below the broken railing and back. Its splintered planks hang
/// to 1.77 m above the floor: under the 1.5 m capsule's top, so the body must pass under them.
#[test]
fn wolf_walks_through_the_broken_wall_past_the_gate_path_idol() {
    let dir = map_dir();
    if !dir.join("object_collision.bin").is_file() {
        eprintln!("skipping: {} not exported", dir.display());
        return;
    }
    let mut world = CollisionWorld::load_map(&dir).unwrap();
    world
        .add_block_objects(&dir, "m11_00_00_00", |_, who| who)
        .unwrap();
    // Where the playtester stood at the hole, then through it to the floor 0.2 m lower.
    let through = [
        [-180.97, -29.075, 26.33],
        [-181.8, -29.2, 24.6],
        [-182.6, -29.29, 22.8],
    ];
    let end = walk_route(&world, &through, 0.08);
    assert!((end[1] + 29.29).abs() < 0.1, "on the far floor: {end:?}");
    let back = [end, [-181.8, -29.2, 24.6], [-181.0, -29.075, 26.6]];
    let end = walk_route(&world, &back, 0.08);
    // `l`'s hull of the wall's foot (h004810) stands 0.14 m above `h`'s planks here, and Wolf
    // walks on `l` as in the exe (#189).
    assert!(
        (end[1] + 29.075).abs() < 0.2,
        "back at the idol side: {end:?}"
    );
}
