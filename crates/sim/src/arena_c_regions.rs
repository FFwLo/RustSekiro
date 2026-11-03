//! Cached gameplay regions from local MSBS data, for arena-entry event conditions.
use glam::{Quat, Vec3};
use sekiro_formats::reader::{Reader, utf16z};
use std::{
    collections::HashMap,
    path::Path,
    sync::{Mutex, OnceLock},
};

#[derive(Clone)]
struct Region {
    origin: Vec3,
    rotation: Quat,
    shape: u32,
    size: Vec3,
}

fn read(cache: &Path, map: &str) -> Option<HashMap<i32, Region>> {
    let data = std::fs::read(cache.join(format!("raw/map/MapStudio/{map}.msb"))).ok()?;
    let mut table = 0x10;
    while table != 0 {
        let mut r = Reader::at(&data, table);
        r.i32().ok()?;
        let count = usize::try_from(r.i32().ok()?).ok()?;
        if !(1..=100_000).contains(&count) {
            return None;
        }
        let name = r.offset64().ok()?;
        let entries: Vec<_> = (1..count)
            .map(|_| r.offset64())
            .collect::<Result<_, _>>()
            .ok()?;
        table = r.offset64().ok()?;
        if utf16z(&data, name).ok()? != "POINT_PARAM_ST" {
            continue;
        }
        let mut regions = HashMap::new();
        for at in entries {
            let base = Reader::at(&data, at + 0x50).offset64().ok()?;
            if base == 0 {
                continue;
            }
            let id = Reader::at(&data, at + base + 4).i32().ok()?;
            if id <= 0 {
                continue;
            }
            let mut r = Reader::at(&data, at + 0x10);
            let shape = r.u32().ok()?;
            let [x, y, z] = r.vec3().ok()?;
            let [rx, ry, rz] = r.vec3().ok()?.map(f32::to_radians);
            let offset = Reader::at(&data, at + 0x48).offset64().ok()?;
            let mut size = Reader::at(&data, at + offset);
            let dimensions = match shape {
                2 => Vec3::splat(size.f32().ok()?),                      // sphere
                3 => Vec3::new(size.f32().ok()?, size.f32().ok()?, 0.0), // cylinder
                5 => {
                    let [width, depth, height] = size.vec3().ok()?;
                    Vec3::new(width, height, depth)
                }
                _ => Vec3::ZERO,
            };
            regions.insert(
                id,
                Region {
                    origin: Vec3::new(-x, y, z),
                    rotation: Quat::from_rotation_y(-ry)
                        * Quat::from_rotation_z(-rz)
                        * Quat::from_rotation_x(rx),
                    shape,
                    size: dimensions,
                },
            );
        }
        return Some(regions);
    }
    None
}

pub fn contains(cache: &Path, map: &str, entity: i32, position: [f32; 3]) -> Option<bool> {
    type Maps = HashMap<std::path::PathBuf, Option<HashMap<i32, Region>>>;
    static MAPS: OnceLock<Mutex<Maps>> = OnceLock::new();
    let mut maps = MAPS.get_or_init(Default::default).lock().ok()?;
    let key = cache.join(format!("raw/map/MapStudio/{map}.msb"));
    let regions = maps
        .entry(key)
        .or_insert_with(|| read(cache, map))
        .as_ref()?;
    let region = regions.get(&entity)?;
    let p = region.rotation.inverse() * (Vec3::from_array(position) - region.origin);
    Some(match region.shape {
        2 => p.length_squared() <= region.size.x * region.size.x,
        3 => {
            p.x * p.x + p.z * p.z <= region.size.x * region.size.x
                && (0.0..=region.size.y).contains(&p.y)
        }
        5 => {
            p.x.abs() <= region.size.x * 0.5
                && p.z.abs() <= region.size.z * 0.5
                && (0.0..=region.size.y).contains(&p.y)
        }
        _ => return None,
    })
}
