//! Placed objects' collision (`cache/maps/<id>/object_collision.bin`, `docs/OBJECT-COLLISION.md`)
//! over the Ashina Outskirts and Ashina Castle blocks: how many objects and triangles each map
//! carries, by `ObjectParam.collisionType`, and the rideable pine over the Outskirts start ravine,
//! which the player must be able to walk along after the grapple. Skips when the maps have not
//! been exported.

use std::collections::BTreeMap;
use std::path::PathBuf;

use sekiro_formats::hknp::ObjectCollision;
use sekiro_sim::body::{Body, BodyInput};
use sekiro_sim::collision::CollisionWorld;
use sekiro_sim::hit_filter;

fn load(id: &str) -> Option<(PathBuf, ObjectCollision)> {
    let dir = sekiro_sim::cache::root().join("maps").join(id);
    if !dir.join("object_collision.bin").is_file() || !dir.join("collision.bin").is_file() {
        eprintln!(
            "skipping: {} has no exported object collision",
            dir.display()
        );
        return None;
    }
    let objects =
        ObjectCollision::from_bytes(&std::fs::read(dir.join("object_collision.bin")).unwrap())
            .unwrap();
    Some((dir, objects))
}

/// The block's own map collision plus every placed object's. The play world
/// (`map_world::collision_world` in the game crate, tested there on the same pine) adds the
/// connected blocks too and leaves out the objects the story switches off.
fn world(dir: &std::path::Path) -> CollisionWorld {
    let mut world = CollisionWorld::load_map(dir).unwrap();
    world
        .add_block_objects(dir, "m11_00_00_00", |_, who| who)
        .unwrap();
    world
}

/// collisionType -> (placed objects, triangles).
fn census(objects: &ObjectCollision) -> BTreeMap<u8, (usize, usize)> {
    let mut out = BTreeMap::new();
    for part in &objects.parts {
        let entry = out.entry(part.collision_type).or_insert((0, 0));
        entry.0 += 1;
        entry.1 += part.count as usize;
    }
    out
}

#[test]
fn object_collision_per_map() {
    // (map, placed objects with static collision, their triangles, those the player stands on).
    for (id, parts, triangles, player) in [
        ("m11_00_00_00", 1119, 116_996, 901),
        ("m11_01_00_00", 1386, 124_004, 1159),
    ] {
        let Some((_, objects)) = load(id) else {
            continue;
        };
        let by_type = census(&objects);
        println!("{id}: by collisionType {by_type:?}");
        assert_eq!(objects.parts.len(), parts, "{id}");
        assert_eq!(objects.mesh.triangle_count(), triangles, "{id}");
        let standable = objects
            .parts
            .iter()
            .filter(|p| hit_filter::object_mask(p.collision_type) & hit_filter::PLAYER != 0)
            .count();
        assert_eq!(standable, player, "{id}");
    }
}

/// The rideable snow-covered pine over the Outskirts start ravine (`o002500_0005`), from its
/// tip over the ravine (the grapple landing, -141.58, -44.26, -123.86) along the curving trunk
/// to the upper platform it leads to, in waypoints on the trunk's top.
const TRUNK: [[f32; 2]; 8] = [
    [-140.9, -124.1],
    [-139.0, -124.0],
    [-137.8, -123.4],
    [-136.8, -122.5],
    [-136.0, -121.3],
    [-136.0, -120.0],
    [-135.9, -118.8],
    [-135.5, -117.0],
];
const TREE: &str = "o002500_0005";
/// [`TREE`] as [`CollisionWorld::floor_object`] names it.
const TREE_PART: &str = "m11_00_00_00/o002500_0005";

/// Walks a body through `waypoints` at run speed (forward root motion), returning every tick's
/// feet and whether it was grounded.
fn walk(world: &CollisionWorld, start: [f32; 3], waypoints: &[[f32; 2]]) -> Vec<([f32; 3], bool)> {
    let mut b = Body {
        position: start,
        grounded: true,
        ..Body::default()
    };
    let mut out = Vec::new();
    for w in waypoints {
        for _ in 0..600 {
            let (dx, dz) = (w[0] - b.position[0], w[1] - b.position[2]);
            if dx.hypot(dz) < 0.4 || b.position[1] < start[1] - 3.0 {
                break;
            }
            b.yaw = (-dx).atan2(-dz);
            b.step(
                &BodyInput {
                    root_motion: [0.0, 0.0, -0.08, 0.0],
                    ..BodyInput::default()
                },
                1.0 / 60.0,
                world,
            );
            out.push((b.position, b.grounded));
        }
    }
    out
}

#[test]
fn the_grapple_pine_has_a_floor_from_tip_to_platform() {
    let Some((dir, objects)) = load("m11_00_00_00") else {
        return;
    };
    let tree = objects.parts.iter().find(|p| p.name == TREE).expect(TREE);
    assert_eq!(
        tree.collision_type, 0,
        "the pine is an ordinary object (group 24)"
    );
    let bare = CollisionWorld::load_map(&dir).unwrap();
    let world = world(&dir);
    let tip = TRUNK[0];
    let start_y = world
        .floor(tip[0], tip[1], -40.0)
        .expect("a floor at the trunk's tip");
    assert!((start_y + 44.1).abs() < 0.3, "tip floor at {start_y}");
    assert_eq!(world.floor_object(tip[0], tip[1], -40.0), Some(TREE_PART));
    // Without the object collision nothing holds Wolf over the ravine but the event walls.
    assert!(bare.floor(tip[0], tip[1], -40.0).is_none_or(|y| y < -70.0));

    let path = walk(&world, [tip[0], start_y, tip[1]], &TRUNK[1..]);
    let end = path.last().unwrap().0;
    let mut on_tree = 0.0f32;
    for pair in path.windows(2) {
        let ((a, _), (b, grounded)) = (pair[0], pair[1]);
        assert!(grounded, "fell off the trunk at {b:?}");
        if world.floor_object(b[0], b[2], b[1] + 0.5) == Some(TREE_PART) {
            on_tree += (b[0] - a[0]).hypot(b[2] - a[2]);
        }
    }
    println!("walked the trunk {on_tree:.1} m on {TREE}, ended at {end:?}");
    assert!(on_tree > 6.0, "only {on_tree:.1} m on the trunk");
    let last = TRUNK[TRUNK.len() - 1];
    assert!(
        (end[0] - last[0]).hypot(end[2] - last[1]) < 0.6,
        "did not reach the platform: {end:?}"
    );
    assert!(
        bare.floor(end[0], end[2], end[1] + 0.5).is_some(),
        "the platform is map collision"
    );

    // Off the tip's end over the ravine he falls.
    let off = walk(
        &world,
        [-139.5, start_y + 0.05, -124.05],
        &[[-144.0, -124.3]],
    );
    let fell = off
        .iter()
        .position(|(_, g)| !g)
        .expect("walked off the tip without falling");
    let edge = off[fell].0;
    println!("off the tip: airborne at {edge:?}");
    assert!(edge[0] < -141.0, "fell before the tip's end at {edge:?}");
}
