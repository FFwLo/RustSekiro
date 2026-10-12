//! Boss fog walls: the effect that seals a boss room while its fight is on
//! (tools/fog_walls.py -> extracted/enemies/fog_walls.json).
//!
//! The maps start common event 20005820 (end flag, object entity, SFX, start flag) per wall: the
//! object stands with the SFX on its dummy 101 while the start flag is on and the end flag (the
//! boss's defeat) is off. A fight's walls are the ones in its map whose end flag its own events
//! (boss_scripts.json) use. Sv1's fight is on from the start, so the walls stand until the script
//! handles the defeat (BossScript run.defeated) or the fight changes.
//! After sekiro-rs (MIT / Apache-2.0, crates/sim/src/fogwall.rs): the castle roof's wall, object
//! 1111800 (o001401) with SFX 11 on dummy 101.
//! gap: the wall's collision part (20005825, e.g. h900800) is not loaded, so the fog does not
//! stop Wolf; the object's own model (o001401) is not drawn, only its effect.

use bevy::prelude::*;
use serde_json::Value;
use std::collections::HashSet;
use std::sync::OnceLock;

pub struct FogWallPlugin;

impl Plugin for FogWallPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WallFight>().add_systems(Update, (fight_now, sync_walls).chain());
    }
}

/// The boss row whose walls stand (its fight is on and not won), and the arena it is in.
#[derive(Resource, Default, PartialEq, Clone, Debug)]
pub(crate) struct WallFight(pub(crate) Option<(i64, Arena)>);

/// The loaded arena as the walls need it: its source map and the export's origin and turn.
#[derive(PartialEq, Clone, Debug)]
pub(crate) struct Arena {
    pub(crate) map: String,
    pub(crate) origin: Vec3,
    pub(crate) yaw: f32,
}

/// A standing wall's effect.
#[derive(Component)]
pub(crate) struct FogWall;

/// One wall of a fight: its SFX and its place in the arena.
#[derive(Debug)]
pub(crate) struct Wall {
    pub(crate) sfx: i64,
    pub(crate) object: i64,
    pub(crate) at: Transform,
}

fn read(name: &str, cell: &'static OnceLock<Value>) -> &'static Value {
    cell.get_or_init(|| std::fs::read_to_string(crate::paths::extracted().join(name)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default())
}

/// A source-map (MSB) point in the arena: the export origin off, turned by the arena's yaw with X
/// mirrored (enemy.rs `msb_patrol`'s point; map.rs `arena_to_map` is its inverse).
pub(crate) fn map_to_arena(origin: Vec3, arena_yaw: f32, p: Vec3) -> Vec3 {
    let d = p - origin;
    let (s, c) = arena_yaw.sin_cos();
    Vec3::new(-(d.x * c + d.z * s), d.y, -d.x * s + d.z * c)
}

/// An MSB part's rotation (degrees; Ry * Rz * Rx, sekiro-extract msb.rs `rotate`) in the arena:
/// the X mirror flips the Y and Z angles, and the arena turn adds to the yaw.
pub(crate) fn part_rotation(arena_yaw: f32, rot: [f32; 3]) -> Quat {
    let r = rot.map(f32::to_radians);
    Quat::from_rotation_y(-(arena_yaw + r[1])) * Quat::from_rotation_z(-r[2]) * Quat::from_rotation_x(r[0])
}

fn ints(v: &Value, out: &mut HashSet<i64>) {
    match v {
        Value::Array(a) => a.iter().for_each(|x| ints(x, out)),
        Value::Object(o) => o.values().for_each(|x| ints(x, out)),
        Value::Number(n) => out.extend(n.as_i64()),
        _ => {}
    }
}

/// The walls of a boss row's fight in the arena: the walls of its map (fog_walls.json) whose end
/// flag its events use, one per object (a wall's start can come from several events).
pub(crate) fn walls_for(row: i64, arena: &Arena) -> Vec<Wall> {
    static SCRIPTS: OnceLock<Value> = OnceLock::new();
    static WALLS: OnceLock<Value> = OnceLock::new();
    let script = &read("enemies/boss_scripts.json", &SCRIPTS)[row.to_string()];
    if script["map"].as_str() != Some(arena.map.as_str()) {
        return Vec::new();
    }
    let mut used = HashSet::new();
    ints(&script["events"], &mut used);
    let mut seen = HashSet::new();
    let f = |v: &Value| v.as_f64().unwrap_or(0.0) as f32;
    read("enemies/fog_walls.json", &WALLS)[arena.map.as_str()]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|w| w["end_flag"].as_i64().is_some_and(|e| used.contains(&e)))
        .filter_map(|w| {
            let o = &w["object"];
            let object = o["entity"].as_i64()?;
            let pos = o["pos"].as_array().filter(|a| a.len() == 3)?;
            let rot = o["rot"].as_array().filter(|a| a.len() == 3)?;
            seen.insert(object).then(|| Wall {
                sfx: w["sfx"].as_i64().unwrap_or(0),
                object,
                // The SFX's dummy (101 on o001401) sits at the object's origin.
                // gap: other wall models' dummy offsets are not read.
                at: Transform::from_translation(map_to_arena(arena.origin, arena.yaw, Vec3::new(f(&pos[0]), f(&pos[1]), f(&pos[2]))))
                    .with_rotation(part_rotation(arena.yaw, [f(&rot[0]), f(&rot[1]), f(&rot[2])])),
            })
        })
        .filter(|w| w.sfx > 0)
        .collect()
}

/// The fight the walls stand for: the boss script's row until its defeat, on an arena exported
/// from the row's map.
fn fight_now(script: Option<Res<crate::enemy::BossScript>>, lighting: Option<Res<crate::map::MapLighting>>, mut fight: ResMut<WallFight>) {
    let want = script.and_then(|s| s.run.as_ref().filter(|r| r.defeated.is_none()).map(|r| r.row)).zip(lighting.and_then(|l| {
        Some(Arena { map: l.source_map.clone(), origin: l.map_origin?, yaw: l.arena_yaw }).filter(|a| !a.map.is_empty())
    }));
    if fight.0 != want {
        fight.0 = want;
    }
}

/// Puts the walls up for a new fight and lets the old ones fade (their emitters stop).
fn sync_walls(mut commands: Commands, fight: Res<WallFight>, mut walls: Query<(Entity, &mut crate::fxr::FxEffect), With<FogWall>>) {
    if !fight.is_changed() {
        return;
    }
    for (e, mut fx) in &mut walls {
        fx.stop = true;
        commands.entity(e).remove::<FogWall>();
    }
    let Some((row, arena)) = &fight.0 else { return };
    for w in walls_for(*row, arena) {
        info!("fog wall: object {} sfx {} at {:.1}", w.object, w.sfx, w.at.translation);
        let mut fx = crate::fxr::FxEffect::new(w.sfx, None, false, w.at);
        fx.keep = true;
        commands.spawn((fx, FogWall));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arena(stem: &str) -> Option<Arena> {
        let json: Value = serde_json::from_str(&std::fs::read_to_string(crate::paths::extracted().join(format!("map_{stem}.json"))).ok()?).ok()?;
        let o = json["origin"].as_array()?;
        Some(Arena {
            map: json["map"].as_str()?.to_string(),
            origin: Vec3::from_array(std::array::from_fn(|i| o[i].as_f64().unwrap_or(0.0) as f32)),
            yaw: json["arena_yaw"].as_f64()? as f32,
        })
    }

    /// The map -> arena place undoes map.rs `arena_to_map`, and a part's turned +Z is the MSB yaw's
    /// forward in the arena (enemy.rs `msb_patrol`'s fwd).
    #[test]
    fn a_wall_place_maps_into_the_arena_and_back() {
        let (origin, yaw) = (Vec3::new(-110.0, 54.0, 239.0), 0.69);
        let p = Vec3::new(-120.896, 53.83, 229.08);
        assert!((crate::map::arena_to_map(origin, yaw, map_to_arena(origin, yaw, p)) - p).length() < 1e-3);
        let ry = 47.073f32;
        let (s, c) = ry.to_radians().sin_cos();
        let fwd = map_to_arena(Vec3::ZERO, yaw, Vec3::new(s, 0.0, c));
        assert!((part_rotation(yaw, [0.0, ry, 0.0]) * Vec3::Z - fwd).length() < 1e-4);
        // Pitch and roll as the MSB's Ry * Rz * Rx with X mirrored.
        let rot = [-8.0f32, 30.0, 5.0];
        let v = [0.3f32, 0.5, 0.8];
        let (sx, cx) = rot[0].to_radians().sin_cos();
        let (sy, cy) = rot[1].to_radians().sin_cos();
        let (sz, cz) = rot[2].to_radians().sin_cos();
        let g = [v[0], v[1] * cx - v[2] * sx, v[1] * sx + v[2] * cx];
        let g = [g[0] * cz - g[1] * sz, g[0] * sz + g[1] * cz, g[2]];
        let g = [g[0] * cy + g[2] * sy, g[1], -g[0] * sy + g[2] * cy];
        let game = map_to_arena(Vec3::ZERO, yaw, Vec3::from_array(g));
        let ours = part_rotation(yaw, rot) * Vec3::new(-v[0], v[1], v[2]);
        assert!((game - ours).length() < 1e-4, "{game} vs {ours}");
    }

    /// Each fight raises only its own room's walls: the castle roof's (o001401, SFX 11) for
    /// Genichiro, the Corrupted Monk's near door and not the far ones of its map.
    #[test]
    fn a_fight_raises_its_own_rooms_fog_walls() {
        let (Some(roof), Some(monk)) = (arena("boss_71001000"), arena("boss_51000000")) else {
            eprintln!("no exported arenas: skipped");
            return;
        };
        let w = walls_for(50600000, &roof);
        assert_eq!(w.iter().map(|w| (w.object, w.sfx)).collect::<Vec<_>>(), vec![(1111800, 11)]);
        let back = crate::map::arena_to_map(roof.origin, roof.yaw, w[0].at.translation);
        assert!((back - Vec3::new(-120.896, 53.83, 229.08)).length() < 0.01, "{back}");
        let near = walls_for(51000000, &monk);
        assert_eq!(near.iter().map(|w| w.object).collect::<Vec<_>>(), vec![1701800]);
        assert!(near[0].at.translation.length() < 20.0, "the monk's wall is off in the map: {}", near[0].at.translation);
        assert!(walls_for(51000000, &roof).is_empty(), "another map's fight raised walls here");
    }

    /// The walls stand while the fight is on and go when it is won, and they outlast the
    /// effects' age limit.
    #[test]
    fn fog_walls_stand_until_the_fight_is_won() {
        let Some(roof) = arena("boss_71001000") else {
            eprintln!("no exported arena: skipped");
            return;
        };
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).init_resource::<WallFight>().add_systems(Update, sync_walls);
        let count = |app: &mut App| app.world_mut().query_filtered::<&crate::fxr::FxEffect, With<FogWall>>().iter(app.world()).filter(|f| f.keep && !f.stop).count();
        app.update();
        assert_eq!(count(&mut app), 0);
        app.world_mut().resource_mut::<WallFight>().0 = Some((50600000, roof));
        app.update();
        assert_eq!(count(&mut app), 1, "no wall for the castle roof fight");
        app.world_mut().resource_mut::<WallFight>().0 = None;
        app.update();
        assert_eq!(count(&mut app), 0, "the wall stands after the win");
        assert_eq!(app.world_mut().query::<&crate::fxr::FxEffect>().iter(app.world()).filter(|f| f.stop).count(), 1, "the old wall should fade, not vanish");
    }
}
