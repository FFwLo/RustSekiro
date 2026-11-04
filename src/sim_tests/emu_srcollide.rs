//! The collision copied from sekiro-rs (`crates/sim` `CollisionWorld`, row E8) against the exe:
//! the play world's floors, walls and rays RSekiro takes under sekiro-rs's map (sr_map.rs
//! `WorldCollision for CollisionWorld`) run on the inputs the exe's own casts and proxy steps
//! were recorded with on the Naomori-arena snapshot (tools/emu-cases `proxy`, `collide_*`).

use super::*;
use sekiro_sim::collision::CollisionWorld;

fn floats(x: &serde_json::Value) -> Vec<f32> {
    x.as_array().unwrap().iter().map(|x| x.as_f64().unwrap_or(f64::NAN) as f32).collect()
}

fn load(name: &str) -> Option<serde_json::Value> {
    let text = std::fs::read_to_string(crate::paths::emu().join(name)).ok()?;
    Some(serde_json::from_str(&text).unwrap())
}

/// m11_00_00_00's play world as RSekiro builds it under sekiro-rs's map (sr_map.rs `install`).
fn world() -> Option<CollisionWorld> {
    let cache = crate::sr_map::cache();
    if !cache.join("maps/m11_00_00_00/collision.bin").is_file() {
        return None;
    }
    let layouts = sekiro_rs::map::world::play_blocks(&cache, "m11_00_00_00").ok()?;
    let stories = sekiro_rs::map::story::load_blocks(&cache, &layouts).ok()?;
    sekiro_rs::map::world::collision_world(&cache, &layouts, &stories).ok()
}

/// Exe (FromSoftware) space to sekiro-rs's Bevy space: X mirrored.
fn bevy(p: Vec3) -> Vec3 {
    Vec3::new(-p.x, p.y, p.z)
}

/// The recorded walks: (name, dt, the exe's feet before each frame and after the last, the
/// request each frame, grounded (physics +0xa2) each frame).
type Walk = (String, f32, Vec<Vec3>, Vec<Vec2>, Vec<bool>);

fn walks() -> Vec<Walk> {
    let mut rows: Vec<Walk> = Vec::new();
    let feet = |s: &serde_json::Value, fr: &[serde_json::Value]| -> Vec<Vec3> {
        std::iter::once(Vec3::from_slice(&floats(&s["start"]))).chain(fr.iter().map(|f| Vec3::from_slice(&floats(&f["pos"])))).collect()
    };
    if let Some(v) = load("proxy_frames.json") {
        for s in v["scenarios"].as_array().unwrap() {
            let fr = s["frames"].as_array().unwrap();
            rows.push((
                format!("proxy {}", s["name"].as_str().unwrap()),
                s["dt"].as_f64().unwrap() as f32,
                feet(s, fr),
                fr.iter().map(|f| Vec2::from_slice(&floats(&f["req"]))).collect(),
                fr.iter().map(|f| f["a0"][2].as_u64() == Some(1)).collect(),
            ));
        }
    }
    if let Some(v) = load("collide_steps.json") {
        for s in v["scenarios"].as_array().unwrap() {
            let fr = s["frames"].as_array().unwrap();
            let r = Vec2::from_slice(&floats(&s["req"]));
            rows.push((
                format!("steps {}", s["name"].as_str().unwrap()),
                s["dt"].as_f64().unwrap() as f32,
                feet(s, fr),
                fr.iter().map(|_| r).collect(),
                fr.iter().map(|f| f["a2"][0].as_u64() == Some(1)).collect(),
            ));
        }
    }
    rows
}

/// Whether the copied floor under `feet` (exe space) is one plain surface the whole capsule
/// stands over: found at the feet and at eight points on its 0.4 m rim, all within 5 cm of it.
/// Elsewhere the exe's capsule rests on an edge or a step's nose (its rim), which RSekiro's
/// terrain decides (src/terrain.rs rim support), not the floor query.
fn plain_floor(w: &CollisionWorld, feet: Vec3) -> Option<f32> {
    let at = |dx: f32, dz: f32| w.floor_hit(-(feet.x + dx), feet.z + dz, feet.y + 0.5).map(|h| h.0);
    let h = at(0.0, 0.0)?;
    (0..8)
        .all(|k| {
            let a = k as f32 * std::f32::consts::FRAC_PI_4;
            at(0.4 * a.cos(), 0.4 * a.sin()).is_some_and(|r| (r - h).abs() < 0.05)
        })
        .then_some(h)
}

/// The floor the copied `CollisionWorld::floor_hit` finds under the exe's feet on each grounded
/// frame of the proxy and step walks (`proxy_frames.json`, `collide_steps.json`), where the
/// ground under the capsule is one plain surface: the exe's feet stand on the map mesh pushed out
/// by its convex radius (`MESH_CONVEX_RADIUS`, 0.01 m), 1 mm over it.
#[test]
pub(crate) fn copied_floor_under_wolf_matches_the_exe() {
    let Some(w) = world() else { return };
    let rows = walks();
    if rows.is_empty() {
        return;
    }
    let (mut n, mut edge, mut worst, mut sum) = (0, 0, 0.0f32, 0.0f32);
    let mut report = Vec::new();
    for (name, _, pos, _, grounded) in &rows {
        let mut own = 0.0f32;
        for (i, &g) in grounded.iter().enumerate() {
            let exe = pos[i + 1];
            if !g {
                continue;
            }
            let Some(h) = plain_floor(&w, exe) else {
                edge += 1;
                continue;
            };
            let e = exe.y - h;
            own = own.max(e.abs());
            worst = worst.max(e.abs());
            sum += e;
            n += 1;
        }
        report.push(format!("{name}: worst {own:.4}"));
    }
    println!("copied floor: {n} grounded frames on plain ground, worst {worst:.4} m, mean feet over floor {:.4}; {edge} on an edge or step (not compared)
{}", sum / n as f32, report.join("
"));
    assert!(n > 500, "only {n} frames compared");
    // The feet ride 1 mm over the surface (FUN_140bca6d0 lowers the proxy to the ground); the
    // bumps of the arena's floor turn under the capsule's rim by up to 5 mm more.
    assert!(worst < 0.006, "worst {worst}");
}

/// The copied `CollisionWorld::slide` on each grounded frame of the same walks: from the exe's
/// feet before the frame, the frame's request (x, z) * dt slid along the map's walls, against
/// where the exe's proxy (keepDistance 0.05, the mesh's 0.01 convex radius) ended it.
#[test]
pub(crate) fn copied_slide_matches_the_exe() {
    let Some(w) = world() else { return };
    let rows = walks();
    if rows.is_empty() {
        return;
    }
    let mut report = Vec::new();
    let mut failures = Vec::new();
    let mut n = 0;
    for (name, dt, pos, req, grounded) in &rows {
        // The dog in enemy_straight is a body, not map collision (emu_collide.rs).
        if name.ends_with("enemy_straight") {
            continue;
        }
        let (mut worst, mut at, mut frames) = (0.0f32, 0, 0);
        for i in 0..req.len() {
            if !grounded[i] || (i > 0 && !grounded[i - 1]) {
                continue;
            }
            let (before, exe) = (pos[i], pos[i + 1]);
            let m = w.slide(bevy(before).into(), [-req[i].x * dt, req[i].y * dt]);
            let ours = Vec2::new(before.x - m[0], before.z + m[1]);
            let e = ours.distance(exe.xz());
            if e > worst {
                (worst, at) = (e, i);
            }
            frames += 1;
        }
        n += frames;
        report.push(format!("{name}: {frames} frames, worst {worst:.4} at frame {at}"));
        // gap, bounded but not matched: the sprint along the rock face past its 1.3 m plane
        // (the mesh turns), the 30 fps walk round the post (one 0.1 m sub-step where the proxy
        // integrates its contacts), the step walks (the proxy climbing a 52 deg nose 0.3-0.4 m
        // high stalls for five frames; stepping down, a riser the lower sphere is already in is
        // pushed out at once where Havok recovers penetration over time).
        let loose = name.ends_with("wall_sprint_30") || name.ends_with("_30fps") && name.contains("ground") || name.starts_with("steps");
        if worst > if loose { 0.06 } else { 0.01 } {
            failures.push(format!("{name}: {worst:.4} at frame {at}"));
        }
    }
    println!("copied slide: {n} frames
{}", report.join("
"));
    assert!(n > 500);
    assert!(failures.is_empty(), "{}", failures.join("
"));
}

/// The copied `CollisionWorld::raycast` on the enemies' sight rays the exe cast
/// (`aiframe_ray.json`: FUN_14061c8a0 from each observer's head to 36 points around it, three
/// enemies of the Naomori arena): clear in the exe exactly when the copied ray meets nothing.
#[test]
pub(crate) fn copied_raycast_matches_the_exe_sight_rays() {
    let Some(w) = world() else { return };
    let Some(v) = load("aiframe_ray.json") else { return };
    let f = |v: &serde_json::Value| bevy(Vec3::from_slice(&floats(v)));
    let (mut agree, mut n) = (0, 0);
    let mut differ = Vec::new();
    for o in v["observers"].as_array().unwrap() {
        let head = f(&o["head"]);
        for r in o["rays"].as_array().unwrap() {
            let d = f(&r["to"]) - head;
            let hit = w.raycast(head, d.normalize(), d.length());
            n += 1;
            if (r["clear"] == 1) == hit.is_none() {
                agree += 1;
            } else {
                differ.push(format!("{} {:.1} m: exe clear {}, copied hit at {hit:?}", o["think"], d.length(), r["clear"]));
            }
        }
    }
    println!("copied raycast: {agree} of {n} sight rays agree
{}", differ.join("
"));
    // gap: 10 rays of two enemies (12400000, 10200020) are clear in the exe but meet the map's
    // own triangles (no placed object) 6-33 m out: parts the sight filter passes, not traced.
    assert!(n == 756 && agree >= 746, "{agree} of {n}");
}
