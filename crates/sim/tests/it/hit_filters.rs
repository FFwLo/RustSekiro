//! Collision parts by MSB hit filter over the whole Ashina Outskirts and Ashina Castle blocks
//! (`cache/maps/m11_00_00_00`, `m11_01_00_00`), and which of them the player collides with
//! (`sekiro_sim::hit_filter`, traced in `docs/HIT-FILTERS.md`). Skips when the maps have not
//! been exported.

use std::collections::BTreeMap;

use glam::Vec3;
use sekiro_formats::hknp::CollisionMesh;
use sekiro_sim::body::Ground;
use sekiro_sim::collision::CollisionWorld;
use sekiro_sim::hit_filter;

fn load(id: &str) -> Option<(serde_json::Value, CollisionMesh)> {
    let dir = sekiro_sim::cache::root().join("maps").join(id);
    if !dir.join("collision.bin").is_file() {
        eprintln!("skipping: {} not exported", dir.display());
        return None;
    }
    let layout = serde_json::from_slice(&std::fs::read(dir.join("layout.json")).unwrap()).unwrap();
    let mesh =
        CollisionMesh::from_bytes(&std::fs::read(dir.join("collision.bin")).unwrap()).unwrap();
    Some((layout, mesh))
}

/// Hit filter -> (parts, triangles) from the layout's collision rows.
fn census(layout: &serde_json::Value) -> BTreeMap<u8, (usize, usize)> {
    let mut out = BTreeMap::new();
    for row in layout["collisions"].as_array().unwrap() {
        let filter = row["hit_filter_id"].as_u64().unwrap() as u8;
        let triangles = row["triangles"][1].as_u64().unwrap_or(0) as usize;
        let entry = out.entry(filter).or_insert((0, 0));
        entry.0 += 1;
        entry.1 += triangles;
    }
    out
}

/// (hit filter, parts, triangles, the player collides) for each map.
const OUTSKIRTS: [(u8, usize, usize, bool); 9] = [
    (8, 155, 2_323_885, true),
    (9, 1, 655, false),
    (11, 1, 0, false),
    (13, 1, 61, false),
    (15, 1, 60, false),
    (19, 2, 114, false),
    (22, 1, 16, true),
    (25, 2, 136, false),
    (28, 2, 1_428, false),
];
const CASTLE: [(u8, usize, usize, bool); 11] = [
    (8, 192, 1_652_576, true),
    (9, 1, 32, false),
    (11, 3, 66, false),
    (13, 2, 127, false),
    (15, 2, 127, false),
    (16, 1, 7, false),
    (19, 2, 14, false),
    (22, 3, 58, true),
    (26, 1, 228, false),
    (27, 12, 1_609, true),
    (28, 2, 1_540, false),
];

fn check(
    id: &str,
    expected: &[(u8, usize, usize, bool)],
) -> Option<(CollisionWorld, CollisionWorld)> {
    let (layout, mut mesh) = load(id)?;
    let census = census(&layout);
    let got: Vec<_> = census
        .iter()
        .map(|(&f, &(parts, tris))| (f, parts, tris, hit_filter::player_collides(f)))
        .collect();
    assert_eq!(got, expected, "{id}: collision parts by hit filter");
    let unstripped = CollisionWorld::from_mesh(&mesh);
    let triangles = mesh.triangle_count();
    let helpers = hit_filter::strip_helpers(&mut mesh, &layout);
    assert_eq!(
        mesh.triangle_count(),
        triangles,
        "{id}: triangle indices kept"
    );
    // The swim hits leave the walkable mesh too: the player meets them as water.
    let removed: BTreeMap<u8, usize> = expected
        .iter()
        .filter(|e| (!e.3 || e.0 == hit_filter::SWIM_FILTER) && e.2 > 0)
        .map(|e| (e.0, e.2))
        .collect();
    assert_eq!(
        helpers.removed, removed,
        "{id}: triangles out of the walkable mesh"
    );
    let swim: usize = expected
        .iter()
        .filter(|e| e.0 == hit_filter::SWIM_FILTER)
        .map(|e| e.2)
        .sum();
    assert_eq!(helpers.swim.triangle_count(), swim, "{id}: swim hits");
    // Enemy and camera helpers stay, held by those that collide with them.
    let mut kept: BTreeMap<u8, usize> = BTreeMap::new();
    for e in expected
        .iter()
        .filter(|e| !e.3 && hit_filter::mask(e.0) != 0)
    {
        *kept.entry(hit_filter::mask(e.0)).or_default() += e.2;
    }
    let got: BTreeMap<u8, usize> = helpers
        .parts
        .iter()
        .map(|(&m, mesh)| (m, mesh.triangle_count()))
        .collect();
    assert_eq!(got, kept, "{id}: enemy and camera helpers");
    let walls: usize = helpers
        .triggers
        .iter()
        .map(|t| t.mesh.triangle_count())
        .sum();
    let event: usize = expected
        .iter()
        .filter(|e| hit_filter::is_event_wall(e.0))
        .map(|e| e.2)
        .sum();
    assert_eq!(walls, event, "{id}: event wall triangles");
    let mut world = CollisionWorld::from_mesh(&mesh);
    world.add_helpers(&helpers);
    // No player start stood on a helper: the floor under each is unchanged.
    for p in layout["players"].as_array().unwrap() {
        let v: Vec<f32> = p["translation"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_f64().unwrap() as f32)
            .collect();
        let floor = Ground::floor(&world, v[0], v[2], v[1] + 0.5);
        assert!(floor.is_some(), "{id} {}: no floor under {v:?}", p["name"]);
        assert_eq!(
            floor,
            Ground::floor(&unstripped, v[0], v[2], v[1] + 0.5),
            "{id} {}",
            p["name"]
        );
    }
    Some((unstripped, world))
}

#[test]
fn outskirts_helpers_are_not_floors() {
    let Some((unstripped, world)) = check("m11_00_00_00", &OUTSKIRTS) else {
        return;
    };
    // The ravine north of start 0: the only "floor" under it was the filter-15 helper
    // h900000, an event wall the player falls through (the fall return).
    let (x, z) = (-137.6, -124.5);
    let helper = Ground::floor(&unstripped, x, z, -44.3).unwrap();
    assert!((helper + 74.18).abs() < 0.05, "{helper}");
    assert_eq!(Ground::floor(&world, x, z, -44.3), None);
    let contact = world
        .trigger_contact(Vec3::new(x, -70.0, z), Vec3::new(x, -78.0, z))
        .expect("the drop meets h900000");
    assert_eq!((contact.hit_filter, contact.part.as_str()), (15, "h900000"));
    assert!((contact.point.y + 74.18).abs() < 0.05, "{contact:?}");
}

#[test]
fn castle_helpers_are_not_floors() {
    check("m11_01_00_00", &CASTLE);
}

/// #189 review: on `l` the helpers keep their masks as on `h`. Castle's camera-only `h900900`
/// (hit filter 9, 52,877 `l` triangles) and Outskirts' wall-hug slabs (hit filter 25) are no
/// floor in the play world ([`CollisionWorld::load_map`]), while the whole `l` mesh as one
/// solid would stand Wolf on them.
#[test]
fn low_resolution_helpers_are_not_floors() {
    for (id, filter) in [("m11_01_00_00", 9u64), ("m11_00_00_00", 25)] {
        let dir = sekiro_sim::cache::root().join("maps").join(id);
        let Some(low) = sekiro_sim::lo_collision::read_low(&dir).unwrap() else {
            eprintln!("skipping {id}: no collision_lo.bin");
            continue;
        };
        let layout: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.join("layout.json")).unwrap()).unwrap();
        let rows = sekiro_sim::lo_collision::low_rows(&layout, &low);
        let world = CollisionWorld::load_map(&dir).unwrap();
        let whole = CollisionWorld::from_mesh(&low.mesh);
        // The walkable mesh alone: a helper lying flush on real ground is no failure.
        let ground = sekiro_sim::lo_collision::block_collision(&dir, &layout, |_| false).unwrap();
        let ground = CollisionWorld::from_mesh(&ground.mesh);
        let mesh = &low.mesh;
        let (mut tops, mut on_helper, mut on_whole) = (0usize, 0usize, 0usize);
        for row in rows["collisions"].as_array().unwrap() {
            if row["hit_filter_id"].as_u64() != Some(filter) {
                continue;
            }
            let first = row["triangles"][0].as_u64().unwrap() as usize;
            let count = row["triangles"][1].as_u64().unwrap() as usize;
            for t in (first..first + count).step_by((count / 400).max(1)) {
                let [a, b, c] =
                    [0, 1, 2].map(|k| Vec3::from(mesh.vertices[mesh.indices[3 * t + k] as usize]));
                let n = (b - a).cross(c - a).normalize_or_zero();
                if n.y.abs() < 0.8 {
                    continue;
                }
                let p = (a + b + c) / 3.0;
                let on = |w: &CollisionWorld| {
                    Ground::floor(w, p.x, p.z, p.y + 0.001).is_some_and(|y| (y - p.y).abs() < 2e-3)
                };
                if on(&ground) {
                    continue;
                }
                tops += 1;
                on_helper += usize::from(on(&world));
                on_whole += usize::from(on(&whole));
            }
        }
        eprintln!(
            "{id} filter {filter}: {tops} flat triangles, {on_whole} floors whole, {on_helper} in play"
        );
        assert!(tops > 0, "{id}: no flat filter-{filter} triangle");
        assert!(on_whole * 10 > tops * 9, "{id}: {on_whole} of {tops}");
        assert_eq!(on_helper, 0, "{id}: filter {filter} stands Wolf");
    }
}
