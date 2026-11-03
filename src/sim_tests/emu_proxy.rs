//! Wolf's character proxy against the exe (topic proxy, docs/kb/movement.md "Character proxy
//! against the exe"): tools/emu-cases `proxy` runs Wolf's whole chr frame (FUN_140a4b570; the
//! proxy step CSHkBehCharaProxySceneModifier::vf3 FUN_140fe7310 inside the behaviour world step
//! FUN_141015fa0) on the Naomori-arena snapshot with his requested velocity put into the proxy
//! at hknpCharacterProxy::integrate (FUN_1418f8b60), and records his position each frame. Here
//! the same requests go through RSekiro's `Terrain::slide` against a plane rebuilt from the exe's
//! own sphere casts at the rock face, and over open ground.

use super::*;

fn floats(x: &serde_json::Value) -> Vec<f32> {
    x.as_array().unwrap().iter().map(|x| x.as_f64().unwrap_or(f64::NAN) as f32).collect()
}

/// The rock face as a plane: through the contact point of the exe's 0.4 m sphere cast at 1 m
/// over Wolf's spot (sphere centre - normal x 0.4), with the cast's triangle normal (it leans
/// 4 deg). Two triangles 40 m wide, 10 m high.
fn wall_plane(w: &serde_json::Value) -> (crate::map::Terrain, Vec3, Vec2) {
    let c = Vec3::from_slice(&floats(&w["contact_centre"]));
    let n = Vec3::from_slice(&floats(&w["normal"])).normalize();
    let p = c - n * crate::map::RADIUS;
    let u = Vec3::new(-n.z, 0.0, n.x).normalize();
    let up = n.cross(u).normalize();
    let verts = vec![p - u * 20.0 - up * 5.0, p + u * 20.0 - up * 5.0, p + u * 20.0 + up * 5.0, p - u * 20.0 + up * 5.0];
    let t = crate::map::Terrain::from_tris(verts, [([0, 1, 2], 0), ([0, 2, 3], 0)].into_iter());
    (t, c, Vec2::new(n.x, n.z).normalize())
}

#[test]
pub(crate) fn wolf_proxy_at_walls_and_on_open_ground_matches_the_exe() {
    let path = crate::paths::root().join("extracted/emu/proxy_frames.json");
    let Ok(text) = std::fs::read_to_string(&path) else { return };
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    let mut report = Vec::new();
    let mut failures = Vec::new();
    let mut frames = 0;
    let mut ends = std::collections::HashMap::new();
    for s in v["scenarios"].as_array().unwrap() {
        let name = s["name"].as_str().unwrap();
        let dt = s["dt"].as_f64().unwrap() as f32;
        // The exe's proxy: a capsule 0.4 m round, 1.5 m tall (proxy +0x90 / +0xa0), lifted by the
        // physics module's +0x188 (0.3 m) for the move (FUN_140bcac10).
        let b = floats(&s["wolf_box"]);
        assert!((b[4] - crate::map::RADIUS).abs() < 1e-6 && (b[5] - 1.5).abs() < 1e-6, "{name}: Wolf's proxy box {b:?}");
        assert!((floats(&s["lift"])[0] - 0.3).abs() < 1e-6, "{name}: step lift");
        let (terrain, contact, flat) = if s["info"]["wall"].is_object() {
            let (t, c, f) = wall_plane(&s["info"]["wall"]);
            (t, Some(c), f)
        } else if name.starts_with("ground_plus_z") || name == "stand" {
            // Open ground: no wall within reach of the walk (proxy_map), the floor followed by
            // the exe's y.
            (crate::map::Terrain::from_tris(Vec::new(), std::iter::empty()), None, Vec2::X)
        } else {
            // The General (his physics does not step in the snapshot), the cliff edge and the
            // post along +X depend on the map's collision mesh: gap, not compared.
            continue;
        };
        let start = Vec3::from_slice(&floats(&s["start"]));
        let mut own = start;
        let (mut worst_out, mut worst_along) = (0.0f32, 0.0f32);
        for (i, fr) in s["frames"].as_array().unwrap().iter().enumerate() {
            let req = floats(&fr["req"]);
            let exe = Vec3::from_slice(&floats(&fr["pos"]));
            let moved = terrain.slide(Vec3::new(own.x, exe.y, own.z), Vec2::new(req[0], req[1]) * dt);
            own = Vec3::new(own.x + moved.x, exe.y, own.z + moved.y);
            // The face is a plane only 1.3 m either side of the cast: past it (wall_60, the
            // sprint) the rock turns away and the exe walks on where the plane would stop him.
            if contact.is_some_and(|c| Vec2::new(exe.x - c.x, exe.z - c.z).perp_dot(flat).abs() > 1.3) {
                break;
            }
            let d = Vec2::new(own.x - exe.x, own.z - exe.z);
            let (out, along) = (d.dot(flat).abs(), d.perp_dot(flat).abs());
            frames += 1;
            if out > worst_out {
                worst_out = out;
            }
            if along > worst_along {
                worst_along = along;
            }
            ends.insert(name.to_string(), (exe, own, i));
        }
        let (e, o, last) = ends[name];
        let gap = contact.map(|c| (Vec2::new(e.x - c.x, e.z - c.z).dot(flat), Vec2::new(o.x - c.x, o.z - c.z).dot(flat)));
        report.push(format!("{name}: to frame {last}, worst off the wall / across {worst_out:.4}, along {worst_along:.4}; out of the 1 m contact at the end (exe, RSekiro) {gap:.4?}"));
        // The rock face is a mesh, not the plane: its triangles turn by about 1 deg along the
        // slide (1.549 -> 1.586 m/s at 30 deg), so `along` drifts by up to 2 cm over a second.
        if worst_out > 0.03 || worst_along > 0.04 {
            failures.push(format!("{name}: off {worst_out:.4} along {worst_along:.4}"));
        }
    }
    // The proxy's result does not depend on the frame time: 30 and 60 fps end in the same place.
    if let (Some(a), Some(b)) = (ends.get("wall_30").filter(|e| e.2 == 49), ends.get("wall_30_30fps").filter(|e| e.2 == 24)) {
        let (exe, own) = ((a.0 - b.0).with_y(0.0).length(), (a.1 - b.1).with_y(0.0).length());
        report.push(format!("wall_30 at 60 vs 30 fps: exe ends {exe:.4} m apart, RSekiro {own:.4}"));
        if exe > 0.005 || own > 0.005 {
            failures.push(format!("frame time: exe {exe:.4} RSekiro {own:.4}"));
        }
    }
    println!("proxy: {frames} frames\n{}", report.join("\n"));
    assert!(frames > 0, "no compared frames in {}", path.display());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
