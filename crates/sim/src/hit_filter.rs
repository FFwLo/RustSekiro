//! What a map collision part's MSB hit filter (`hit_filter_id`) means for the player, traced in
//! the exe (`docs/HIT-FILTERS.md` has the addresses and the full tables).
//!
//! The game turns the hit filter into a Havok collision group (`FUN_1409b58b0`): filter 8 and
//! every unlisted value is ordinary map collision (group 38 for the high-resolution `h` binder,
//! 39 or 40 for the low one), and the helper filters get groups of their own. Whether two groups
//! collide is a 128 x 128 bit matrix built once (`FUN_140933390`). The player's capsule is group
//! 1 and the camera's ray is group 48, so the matrix says which parts the player stands on or
//! bumps into and which only the camera sees. The `h9xxxxx` helpers are mostly not for the
//! player: enemy-only walls, camera blockers, water surfaces, wall-hug and wall-jump volumes,
//! fall-prevention assists, and "map action event walls" (group 44) that nothing solid touches
//! but whose contact runs a reaction (`FUN_1409edf20`).
//!
//! A block's character collision ([`crate::lo_collision`]) holds every part's triangles;
//! [`strip_helpers`] takes the ones the player does not collide with out of it (in place, so the
//! triangle ranges of its layout rows stay valid) and returns the enemy, camera, fall-prevention
//! and wall-hug parts, the swim hits and the contact triggers for
//! [`crate::collision::CollisionWorld::add_helpers`]. Enemies and bosses use
//! [`crate::collision::EnemyGround`]: they also stand on water surfaces and stop at
//! enemy-only walls.

use std::collections::BTreeMap;

use sekiro_formats::hknp::CollisionMesh;
use serde_json::Value;

/// Havok collision group of a collision part (`FUN_1409b58b0`). `low` is the low-resolution
/// (`l` binder) resource and `middle` its middle variant; the high-resolution `h` binder gives
/// ordinary parts group 38. Helper filters get their group from either binder.
pub fn collision_group(hit_filter: u8, low: bool, middle: bool) -> u8 {
    match hit_filter {
        9 => 41,
        11 | 19 => 42,
        12..=15 | 20 | 21 | 23 => 44,
        16 => 46,
        17 => 109,
        18 => 51,
        22 => 43,
        24 => 68,
        25 => 85,
        26 => 88,
        27 => 91,
        28 => 92,
        29 => 79,
        _ if low => 39 + u8::from(middle),
        _ => 38,
    }
}

/// Groups the player's capsule (group 1) collides with, among the groups map parts and placed
/// objects get, from the matrix `FUN_140933390` builds: the object groups 24 to 27, low and
/// middle map (39, 40), the capsule-only wall (43) and the swim hit (91). The high-resolution map
/// (38) is not among them: the player walks on the low-resolution binder
/// ([`crate::lo_collision`], #189).
const PLAYER_GROUPS: [u8; 8] = [24, 25, 26, 27, 39, 40, 43, 91];
/// Groups an enemy's or a boss's capsule (groups 2 and 5, and their map-only variants 7 and
/// 10) collides with: the player's plus the enemy-only wall (42) and the water surface (46).
const ENEMY_GROUPS: [u8; 10] = [24, 25, 26, 27, 39, 40, 42, 43, 46, 91];
/// Groups the camera's ray (group 48) collides with: the object group 24 (オブジェ), low and
/// middle map, the map camera hit (41), the water surface (46) and the object group 108
/// (キャラとロックが当たらないオブジェ).
const CAMERA_GROUPS: [u8; 6] = [24, 39, 40, 41, 46, 108];

/// Who collides with a part: bits of [`mask`].
pub const PLAYER: u8 = 1;
pub const ENEMY: u8 = 2;
pub const CAMERA: u8 = 4;
/// The wall-hug parts (壁張り付き中当たり, hit filter 25, group 85: `h900100`, `h900101`), which
/// only a wall-hugging body stands on. Group 85 meets only itself and group 86, and
/// `FUN_1407df900` gives a character group 86 and its IK ray group 87 (the wall-hug IK ray) in
/// the state `FUN_140b7d5c0` tests; 86 also meets the low and middle map (39, 40), the object
/// groups 24 to 27 and the capsule-only wall (43). In m11_00's snake pit the `l` copy of
/// `h900100` is the slab under the narrow ledge of wall `h013600`, which has no floor of its own
/// there in `l` (#189).
pub const WALL_HUG: u8 = 128;
/// Hit filter of the swim hit (水泳当たり, group 91): the plane under a swimming body. env 3025,
/// "is touching the swim hit" (`re/ext/meow_env.txt`), is what the player script swims by, and
/// touching one runs `FUN_1409ee830` (`FUN_1409edf20`'s case 27), which flags the character
/// (`chr+0x1a11` bit 7, `chr+0x1a15` bit 4). So deep water is the swim hits' deep-water
/// triangles, not every triangle with that material: an ordinary part's are ground (group 39 is
/// solid whatever the material), and in `l` many shore and pool-bed triangles carry it (#189).
pub const SWIM_FILTER: u8 = 27;
/// The fall-prevention assist walls (落下防止アシストヒット): group 92 for ground movement and
/// ground actions, group 93 (the `f` binder's bands) for ground actions only. No capsule
/// collides with them; the assist's sphere casts do (groups 94 and 96 meet 92, 95 and 96 meet
/// 93), and only while the player script asks for the assist ([`fall_assist_walls`]).
pub const FALL_ASSIST_MOVE: u8 = 8;
pub const FALL_ASSIST_ACTION: u8 = 16;
/// [`FALL_ASSIST_MOVE`] and [`FALL_ASSIST_ACTION`] walls that are on only while the body stands
/// on an object that can be ridden ([`RIDEABLE_SPECIAL`]). The story switches these off (m11_00
/// group 1106200), and common event 20005560 switches them on while Wolf has SpEffect 4100,
/// which HitMaterialSpecialSettingParam 8 ("objects that can be ridden") gives him.
pub const RIDE_ASSIST_MOVE: u8 = FALL_ASSIST_MOVE << 2;
pub const RIDE_ASSIST_ACTION: u8 = FALL_ASSIST_ACTION << 2;
/// HitMaterialSpecialSettingParam row of "objects that can be ridden" (乗れるオブジェ), the one
/// that gives SpEffect 4100. A collision triangle's material id is its special setting times
/// 100 plus its HitMtrlParam row: the rideable pines' trunks are 808 (tree, row 8), and wood
/// floors 204 and 504 (wood, rows 2 and 5).
pub const RIDEABLE_SPECIAL: u32 = 8;

/// How long SpEffect 4100 ("Hit material judgment _ Riding object", the riding state
/// [`RIDEABLE_SPECIAL`] floors give) lasts after the last tick on such a floor: its
/// SpEffectParam effectEndurance, 0.06 s. A thin map floor poking through a branch, or a tick
/// over a gap in the trunk's surface, so does not drop the [`RIDE_ASSIST_MOVE`] walls.
pub const SP_EFFECT_4100_ENDURANCE: f32 = 0.06;

/// The riding state's SpEffect, which [`RIDE_WALL_EVENT`] waits on.
pub const RIDING_SP_EFFECT: i32 = 4100;

/// Common event that switches its entity group argument's objects on while Wolf has
/// [`RIDING_SP_EFFECT`] and off again when he loses it (m11_00 starts it for group 1106200,
/// the Outskirts trees' fall-prevention walls).
pub const RIDE_WALL_EVENT: i64 = 20_005_560;

/// The entity groups `map_file` starts [`RIDE_WALL_EVENT`] for, after checking in
/// `common_func` that the event waits on [`RIDING_SP_EFFECT`] (`IfCharacterHasSpeffect`, 4[5],
/// whose SpEffect id is the word at byte 8).
pub fn ride_wall_groups(
    map_file: &sekiro_formats::emevd::Emevd,
    common_func: &sekiro_formats::emevd::Emevd,
) -> Result<Vec<u32>, String> {
    let event = common_func
        .event(RIDE_WALL_EVENT)
        .ok_or(format!("common_func.emevd has no event {RIDE_WALL_EVENT}"))?;
    let speffects: Vec<i32> = event
        .instructions
        .iter()
        .filter(|i| i.code() == (4, 5))
        .filter_map(|i| i.i32_at(8))
        .collect();
    if speffects.is_empty() || speffects.iter().any(|&s| s != RIDING_SP_EFFECT) {
        return Err(format!(
            "event {RIDE_WALL_EVENT} waits on SpEffects {speffects:?}, not {RIDING_SP_EFFECT}"
        ));
    }
    map_file
        .starts_of(RIDE_WALL_EVENT)
        .iter()
        .filter(|s| s.common)
        .map(|s| {
            s.arg_i32(0)
                .map(|g| g as u32)
                .ok_or(format!("event {}: no group argument", s.from_event))
        })
        .collect()
}

/// [`ride_wall_groups`] from `cache/raw/event/<map>.emevd` and `common_func.emevd`.
pub fn load_ride_wall_groups(cache: &std::path::Path, map: &str) -> Result<Vec<u32>, String> {
    let read = |name: &str| -> Result<sekiro_formats::emevd::Emevd, String> {
        let path = cache.join(format!("raw/event/{name}.emevd"));
        let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        sekiro_formats::emevd::Emevd::parse(&bytes).map_err(|e| e.to_string())
    };
    ride_wall_groups(&read(map)?, &read("common_func")?)
}

/// True for the material id of a floor that gives the riding state ([`RIDEABLE_SPECIAL`]).
pub fn rideable(floor_material: Option<u32>) -> bool {
    floor_material.is_some_and(|m| m / 100 == RIDEABLE_SPECIAL)
}

/// The ride twins of the fall-prevention bits in `who` ([`FALL_ASSIST_MOVE`] to
/// [`RIDE_ASSIST_MOVE`], [`FALL_ASSIST_ACTION`] to [`RIDE_ASSIST_ACTION`]); every other bit maps
/// to 0, so a part that is no fall-prevention wall gets no ride mask.
pub fn ride_mask(who: u8) -> u8 {
    let mut out = 0;
    if who & FALL_ASSIST_MOVE != 0 {
        out |= RIDE_ASSIST_MOVE;
    }
    if who & FALL_ASSIST_ACTION != 0 {
        out |= RIDE_ASSIST_ACTION;
    }
    out
}

/// The walls of `walls` ([`fall_assist_walls`] bits) plus their [`RIDE_ASSIST_MOVE`] and
/// [`RIDE_ASSIST_ACTION`] twins while the body has the riding state (SpEffect 4100).
pub fn riding_walls(walls: u8, riding: bool) -> u8 {
    if riding {
        walls | ride_mask(walls)
    } else {
        walls
    }
}

/// The fall-prevention walls the assist tests for HKS `act(3016, arg)`. The script
/// (`_UpdateFallProtection`) sends arg 0 in states whose `ENABLE_FALL_PROTECT_WALL_STRONG` is
/// set (standing, walking, running, dodges, landings) and arg 1 where
/// `ENABLE_FALL_PROTECT_WALL_WEAK` is set (ground actions), every frame. The act's handler
/// (`FUN_140b32410`, case 3016) sets bit `arg` of the request word (`1 << arg`, in the module
/// `FUN_140b34de0` returns), and `FUN_140bc6800` picks the sphere cast's group from the two
/// request bits: bit 0 alone casts with group 94, which meets only group 92 (filter 28, object
/// type 6), bit 1 alone with 95, which meets only 93 (the `f` bands, object type 11), both with
/// 96, which meets both.
pub fn fall_assist_walls(arg: i32) -> u8 {
    match arg {
        0 => FALL_ASSIST_MOVE,
        1 => FALL_ASSIST_ACTION,
        _ => 0,
    }
}

/// [`PLAYER`], [`ENEMY`], [`CAMERA`], fall-prevention and [`WALL_HUG`] bits for a Havok
/// collision group.
pub fn group_mask(group: u8) -> u8 {
    [
        (&PLAYER_GROUPS[..], PLAYER),
        (&ENEMY_GROUPS[..], ENEMY),
        (&CAMERA_GROUPS[..], CAMERA),
        (&[92][..], FALL_ASSIST_MOVE),
        (&[93][..], FALL_ASSIST_ACTION),
        (&[85][..], WALL_HUG),
    ]
    .into_iter()
    .filter(|(groups, _)| groups.contains(&group))
    .fold(0, |m, (_, bit)| m | bit)
}

/// Whether the player stands on and bumps into parts with this hit filter.
pub fn player_collides(hit_filter: u8) -> bool {
    mask(hit_filter) & PLAYER != 0
}

/// Whether enemies and bosses stand on and bump into parts with this hit filter.
pub fn enemy_collides(hit_filter: u8) -> bool {
    mask(hit_filter) & ENEMY != 0
}

/// Whether the camera ray stops at parts with this hit filter.
pub fn camera_collides(hit_filter: u8) -> bool {
    mask(hit_filter) & CAMERA != 0
}

/// [`PLAYER`], [`ENEMY`], [`CAMERA`], fall-prevention and [`WALL_HUG`] bits for the parts with
/// this hit filter.
pub fn mask(hit_filter: u8) -> u8 {
    // Ordinary parts as their low-resolution body (group 39).
    group_mask(collision_group(hit_filter, true, false))
}

/// Hit filters whose parts are map action event walls (group 44): untouchable, but a
/// character's contact with one runs that filter's reaction in `FUN_1409edf20`.
pub fn is_event_wall(hit_filter: u8) -> bool {
    collision_group(hit_filter, false, false) == 44
}

/// The event wall reaction this simulation applies: contact with a filter-15 wall starts the
/// fall return. The exe fades the character's model from 1 to 0 at -2 per second
/// (`chr+0x1a48`, `chr+0x1a4c`), and when it reaches 0 sets the physics module's `0x92b`,
/// which makes `FUN_140bcc420` set `0x92a` and hold the body still.
pub const FALL_RETURN_FILTER: u8 = 15;
/// Seconds of the fall return's model fade: 1.0 at -2.0 per second.
pub const FALL_RETURN_FADE: f32 = 0.5;

/// Havok group of a placed object's static and keyframed bodies, by its model's
/// `ObjectParam.collisionType` (`FUN_140872b40` reads it through `FUN_1408705d0`, row `+0x57`,
/// and indexes the table at `0x1429e73b8`; out-of-range types use entry 0). Dynamic bodies take
/// the parallel table at `0x1429e73e8` instead (28, 29, 31, ...), which nobody stands on.
pub fn object_group(collision_type: u8) -> u8 {
    const GROUPS: [u8; 12] = [24, 25, 27, 41, 47, 67, 92, 108, 47, 51, 32, 93];
    GROUPS
        .get(collision_type as usize)
        .copied()
        .unwrap_or(GROUPS[0])
}

/// [`PLAYER`], [`ENEMY`] and [`CAMERA`] bits for an object's static bodies ([`group_mask`] of
/// its [`object_group`]): the player (`FUN_1409325c0` for group 1) and enemies
/// (`FUN_140932480` for group 2) collide with the object groups 24 to 27, the camera ray
/// (group 48) with 24, the camera-only map hit 41 and 108. Groups 32, 47, 51 and 67 hold none
/// of them; 92 and 93 are fall-prevention walls ([`FALL_ASSIST_MOVE`], [`FALL_ASSIST_ACTION`]).
pub fn object_mask(collision_type: u8) -> u8 {
    group_mask(object_group(collision_type))
}

/// The helper triangles taken out of the player's collision.
#[derive(Debug, Default, Clone)]
pub struct HelperCollision {
    /// Parts only enemies, the camera, the fall-prevention assist or a wall-hugging body
    /// collide with, by [`mask`] (hit filters 9, 11, 16, 19, 25 and 28).
    pub parts: BTreeMap<u8, CollisionMesh>,
    /// The swim hits ([`SWIM_FILTER`]): the player and enemies collide with them like ordinary
    /// collision, except that their deep-water triangles are the water planes bodies swim at.
    pub swim: CollisionMesh,
    /// Map action event walls by hit filter, each with its part names.
    pub triggers: Vec<Trigger>,
    /// Triangles removed per hit filter, for logs and tests.
    pub removed: BTreeMap<u8, usize>,
}

#[derive(Debug, Default, Clone)]
pub struct Trigger {
    pub hit_filter: u8,
    pub part: String,
    pub mesh: CollisionMesh,
}

impl HelperCollision {
    /// Every helper mesh bodies collide with, for cropping them with the rest of an arena's
    /// collision. Not the event walls ([`HelperCollision::triggers`]), which an arena keeps or
    /// drops whole ([`crate::arena`]).
    pub fn meshes_mut(&mut self) -> impl Iterator<Item = &mut CollisionMesh> {
        self.parts
            .values_mut()
            .chain(std::iter::once(&mut self.swim))
    }

    /// Appends another block's helpers.
    pub fn extend(&mut self, other: HelperCollision) {
        for (mask, mesh) in other.parts {
            self.parts.entry(mask).or_default().extend(&mesh);
        }
        self.swim.extend(&other.swim);
        self.triggers.extend(other.triggers);
        for (filter, count) in other.removed {
            *self.removed.entry(filter).or_default() += count;
        }
    }
}

/// The hit filter of one of a layout's `collisions` rows.
pub fn row_filter(row: &Value) -> Option<u8> {
    row["collision"]["hit_filter_id"]
        .as_u64()
        .or_else(|| row["hit_filter_id"].as_u64())
        .and_then(|f| u8::try_from(f).ok())
}

/// Takes the triangles of every collision part the player does not collide with, and of the swim
/// hits, out of `mesh`, a block's collision read with its `layout` rows. Removed triangles collapse
/// to a point, which [`crate::collision::CollisionWorld`] skips, so every triangle index in the
/// layout still addresses the same triangle. Triangles outside every part stay, and ones an
/// earlier step already collapsed (hidden or disabled parts) are not taken.
pub fn strip_helpers(mesh: &mut CollisionMesh, layout: &Value) -> HelperCollision {
    let mut out = HelperCollision::default();
    let triangles = mesh.triangle_count();
    for row in layout["collisions"].as_array().into_iter().flatten() {
        let Some(filter) = row_filter(row) else {
            continue;
        };
        let swim = filter == SWIM_FILTER;
        if player_collides(filter) && !swim {
            continue;
        }
        let (Some(first), Some(count)) = (
            row["triangles"][0].as_u64().map(|v| v as usize),
            row["triangles"][1].as_u64().map(|v| v as usize),
        ) else {
            continue;
        };
        let range = first.min(triangles)..(first + count).min(triangles);
        let mut part = CollisionMesh::default();
        for t in range {
            let tri = &mut mesh.indices[t * 3..t * 3 + 3];
            if tri[0] == tri[1] && tri[1] == tri[2] {
                continue;
            }
            let base = part.vertices.len() as u32;
            part.vertices
                .extend(tri.iter().map(|&i| mesh.vertices[i as usize]));
            part.indices.extend([base, base + 1, base + 2]);
            part.materials
                .push(mesh.materials.get(t).copied().unwrap_or(u32::MAX));
            let keep = tri[0];
            tri.fill(keep);
        }
        *out.removed.entry(filter).or_default() += part.triangle_count();
        if part.indices.is_empty() {
            continue;
        }
        let who = mask(filter);
        if swim {
            out.swim.extend(&part);
        } else if who != 0 {
            out.parts.entry(who).or_default().extend(&part);
        } else if is_event_wall(filter) {
            out.triggers.push(Trigger {
                hit_filter: filter,
                part: row["name"].as_str().unwrap_or_default().to_string(),
                mesh: part,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_follow_the_exe_switch() {
        assert_eq!(collision_group(8, false, false), 38);
        assert_eq!(collision_group(8, true, false), 39);
        assert_eq!(collision_group(8, true, true), 40);
        assert_eq!(collision_group(15, true, false), 44);
        assert_eq!(collision_group(27, false, false), 91);
    }

    #[test]
    fn player_and_camera_sets() {
        let player: Vec<u8> = (0..32).filter(|&f| player_collides(f)).collect();
        // Every unlisted filter is ordinary map collision.
        assert!(player.contains(&8) && player.contains(&22) && player.contains(&27));
        for f in [
            9, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 23, 24, 25, 26, 28, 29,
        ] {
            assert!(!player_collides(f), "{f}");
        }
        let camera: Vec<u8> = (8..30).filter(|&f| camera_collides(f)).collect();
        assert_eq!(camera, [8, 9, 10, 16]);
        let enemy: Vec<u8> = (8..30).filter(|&f| enemy_collides(f)).collect();
        assert_eq!(enemy, [8, 10, 11, 16, 19, 22, 27]);
        assert_eq!(mask(8), PLAYER | ENEMY | CAMERA);
        assert_eq!(mask(16), ENEMY | CAMERA);
        assert_eq!(mask(15), 0);
        assert_eq!(mask(28), FALL_ASSIST_MOVE);
        // Only a wall-hugging body (group 86) meets the wall-hug parts (group 85).
        assert_eq!(mask(25), WALL_HUG);
        assert_eq!(fall_assist_walls(0), FALL_ASSIST_MOVE);
        assert_eq!(fall_assist_walls(1), FALL_ASSIST_ACTION);
        assert!(rideable(Some(808)) && !rideable(Some(8)) && !rideable(None));
        assert_eq!(
            riding_walls(FALL_ASSIST_MOVE, true),
            FALL_ASSIST_MOVE | RIDE_ASSIST_MOVE
        );
        assert_eq!(riding_walls(FALL_ASSIST_MOVE, false), FALL_ASSIST_MOVE);
        assert_eq!(riding_walls(0, true), 0);
        assert_eq!(ride_mask(FALL_ASSIST_ACTION), RIDE_ASSIST_ACTION);
        assert_eq!(ride_mask(7), 0);
    }

    #[test]
    fn object_groups_follow_the_exe_table() {
        assert_eq!(object_group(0), 24);
        assert_eq!(object_group(6), 92);
        assert_eq!(object_group(200), 24);
        assert_eq!(object_mask(0), PLAYER | ENEMY | CAMERA);
        assert_eq!(object_mask(1), PLAYER | ENEMY);
        assert_eq!(object_mask(2), PLAYER | ENEMY);
        assert_eq!(object_mask(3), CAMERA);
        assert_eq!(object_mask(7), CAMERA);
        assert_eq!(object_mask(6), FALL_ASSIST_MOVE);
        assert_eq!(object_mask(11), FALL_ASSIST_ACTION);
        for t in [4, 5, 8, 9, 10] {
            assert_eq!(object_mask(t), 0, "{t}");
        }
    }

    /// m11_00 starts common event 20005560 for the Outskirts trees' fall-prevention walls
    /// alone, and m11_01 (the castle) for none (skipped without `cache/raw/event`).
    #[test]
    fn ride_wall_groups_come_from_the_map_events() {
        let cache = crate::cache::root();
        if !cache.join("raw/event/common_func.emevd").is_file() {
            return;
        }
        assert_eq!(
            load_ride_wall_groups(&cache, "m11_00_00_00").unwrap(),
            [1_106_200]
        );
        assert_eq!(
            load_ride_wall_groups(&cache, "m11_01_00_00").unwrap(),
            Vec::<u32>::new()
        );
    }

    #[test]
    fn strip_keeps_triangle_indices() {
        let mut mesh = CollisionMesh {
            vertices: vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0],
                [0.0, -5.0, 0.0],
                [1.0, -5.0, 0.0],
                [0.0, -5.0, 1.0],
            ],
            indices: vec![0, 1, 2, 3, 4, 5],
            materials: vec![1, 2],
        };
        let layout = serde_json::json!({"collisions": [
            {"name": "h000100", "hit_filter_id": 8, "triangles": [0, 1]},
            {"name": "h900000", "hit_filter_id": 15, "triangles": [1, 1]},
        ]});
        let helpers = strip_helpers(&mut mesh, &layout);
        assert_eq!(mesh.triangle_count(), 2);
        assert_eq!(&mesh.indices[..3], &[0, 1, 2]);
        assert_eq!(&mesh.indices[3..], &[3, 3, 3]);
        assert_eq!(helpers.triggers.len(), 1);
        assert_eq!(helpers.triggers[0].part, "h900000");
        assert_eq!(helpers.triggers[0].mesh.vertices[0], [0.0, -5.0, 0.0]);
        assert_eq!(helpers.removed.get(&15), Some(&1));
    }
}
