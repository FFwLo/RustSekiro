//! Boss fog walls, from the map's event script.
//!
//! A boss room is sealed by a placed asset with an effect on it, enabled when the fight starts
//! and removed on victory. The maps start two common events for it (`event/common_func.emevd`),
//! read here from the events that start them:
//!
//! - 20005820 (end flag, asset entity, effect id, start flag): disables the asset, then while
//!   the start flag is on and the end flag is off enables it and plays the effect on its dummy
//!   polygon 101 (`CreateAssetfollowingSFX`, 2006\[4\]);
//! - 20005825 (end flag, collision entity, start flag): the same for a collision part (the
//!   invisible boundary of the arena, `2011[1]`).
//!
//! The Ashina Castle roof uses entities 1111800 (asset `o001401`, a 6 by 7 m plane, effect
//! 11) and 1114800 (collision `h900800`, a rim wall around the roof) for the Owl and for
//! Genichiro alike.

use std::path::Path;

use sekiro_formats::emevd::Emevd;

/// The common event that places the asset and its effect.
pub const WALL_EVENT: i64 = 20_005_820;
/// The common event that switches a collision part.
pub const BOUNDARY_EVENT: i64 = 20_005_825;

/// One fog wall of a fight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FogWall {
    /// The MSB asset (object) entity the wall is.
    pub object_entity: i32,
    /// The effect (FXR id) played on the asset.
    pub sfx: i32,
    /// The asset's dummy polygon the effect follows.
    pub dummy: i32,
    /// The wall stands while this flag is on and [`FogWall::end_flag`] is off.
    pub start_flag: i32,
    pub end_flag: i32,
    /// The MSB collision entity that blocks with it, when the map pairs one. The map gives it
    /// its own start flag (the Owl's is set when the fight begins, the wall's when the room is
    /// entered); both stand from the fight's start, so one gate follows [`FogWall::active`].
    pub collision_entity: Option<i32>,
}

impl FogWall {
    pub fn active(&self, is_set: impl Fn(i32) -> bool) -> bool {
        is_set(self.start_flag) && !is_set(self.end_flag)
    }
}

/// The walls started by `fog_events` (events of `map_file`) through the two common events.
pub fn resolve(
    map_file: &Emevd,
    common_func: &Emevd,
    fog_events: &[i64],
) -> Result<Vec<FogWall>, String> {
    let dummy = common_func
        .event(WALL_EVENT)
        .and_then(|e| e.instructions.iter().find(|i| i.code() == (2006, 4)))
        .and_then(|i| i.i32_at(4))
        .ok_or("common_func.emevd has no fog wall event 20005820")?;
    let starts = map_file.starts();
    let mut walls = Vec::new();
    for from in fog_events {
        let from_here = |event: i64| {
            starts
                .iter()
                .filter(move |s| s.common && s.from_event == *from && s.event == event)
        };
        let arg = |s: &sekiro_formats::emevd::EventStart, i: usize| {
            s.arg_i32(i)
                .ok_or(format!("event {from}: short fog wall arguments"))
        };
        for s in from_here(WALL_EVENT) {
            let (end_flag, object_entity, sfx, start_flag) =
                (arg(s, 0)?, arg(s, 1)?, arg(s, 2)?, arg(s, 3)?);
            let collision_entity = from_here(BOUNDARY_EVENT)
                .find(|b| b.arg_i32(0) == Some(end_flag))
                .and_then(|b| b.arg_i32(1));
            walls.push(FogWall {
                object_entity,
                sfx,
                dummy,
                start_flag,
                end_flag,
                collision_entity,
            });
        }
    }
    Ok(walls)
}

/// [`resolve`] from `cache/raw/event/<map>.emevd` and `common_func.emevd`.
pub fn load(cache: &Path, map: &str, fog_events: &[i64]) -> Result<Vec<FogWall>, String> {
    let read = |name: &str| -> Result<Emevd, String> {
        let path = cache.join(format!("raw/event/{name}.emevd"));
        let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        Emevd::parse(&bytes).map_err(|e| e.to_string())
    };
    resolve(&read(map)?, &read("common_func")?, fog_events)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The gate blocks only while on, and the castle roof's walls read back from the event
    /// script (skipped without `cache/raw`).
    #[test]
    fn fog_wall_gate_blocks_only_while_the_wall_stands() {
        use crate::arena::FogWallPlace;
        use crate::collision::CollisionWorld;
        use glam::{Quat, Vec3};
        use sekiro_formats::hknp::CollisionMesh;

        let place = FogWallPlace {
            wall: FogWall {
                object_entity: 1,
                sfx: 11,
                dummy: 101,
                start_flag: 2,
                end_flag: 3,
                collision_entity: None,
            },
            position: Vec3::new(0.0, 0.0, 2.0),
            rotation: Quat::IDENTITY,
            local_min: Vec3::new(-3.0, 0.0, -0.05),
            local_max: Vec3::new(3.0, 7.0, 0.05),
        };
        let floor = CollisionMesh {
            vertices: vec![[-9.0, 0.0, -9.0], [9.0, 0.0, -9.0], [0.0, 0.0, 9.0]],
            indices: vec![0, 2, 1],
            materials: vec![0],
        };
        let world = CollisionWorld::from_mesh_with_gate(&floor, &place.blocker());
        let walk = |w: &CollisionWorld| w.slide([0.0, 0.0, 0.0], [0.0, 4.0])[1];
        assert!(walk(&world) > 3.9, "the gate starts off");
        world.set_gate(true);
        assert!(walk(&world) < 1.5, "the wall stops a walk through it");
        world.set_gate(false);
        assert!(walk(&world) > 3.9);

        let cache = crate::cache::root();
        if !cache.join("raw/event/m11_01_00_00.emevd").is_file() {
            return;
        }
        let owl = load(&cache, "m11_01_00_00", &[11_115_861]).unwrap();
        assert_eq!(
            owl,
            [FogWall {
                object_entity: 1_111_800,
                sfx: 11,
                dummy: 101,
                start_flag: 11_115_722,
                end_flag: 9308,
                collision_entity: Some(1_114_800),
            }]
        );
        let genichiro = load(&cache, "m11_01_00_00", &[11_115_811]).unwrap();
        assert_eq!(genichiro.len(), 1);
        assert_eq!(
            (genichiro[0].start_flag, genichiro[0].end_flag),
            (11_115_801, 9403)
        );
        assert!(genichiro[0].active(|f| f == 11_115_801));
        assert!(!genichiro[0].active(|f| f == 11_115_801 || f == 9403));
        assert!(!genichiro[0].active(|_| false));
    }
}
