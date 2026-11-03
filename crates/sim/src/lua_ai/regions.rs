//! AI region queries use the same installed MSB volumes as map events.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct MapRegions {
    cache: PathBuf,
    map: String,
}

impl MapRegions {
    pub fn new(cache: &Path, map: &str) -> Self {
        Self {
            cache: cache.to_owned(),
            map: crate::sword_saint::world_map(map).to_owned(),
        }
    }

    pub fn contains(&self, region: i32, position: [f32; 3]) -> Option<bool> {
        crate::arena_c::regions::contains(&self.cache, &self.map, region, position)
    }

    pub fn point(&self, region: i32) -> Option<[f32; 3]> {
        type Points = std::collections::HashMap<(PathBuf, i32), Option<[f32; 3]>>;
        static POINTS: std::sync::OnceLock<std::sync::Mutex<Points>> = std::sync::OnceLock::new();
        let mut points = POINTS.get_or_init(Default::default).lock().ok()?;
        *points
            .entry((self.cache.join(&self.map), region))
            .or_insert_with(|| {
                crate::events::warp_point(
                    &self.cache,
                    self.map[1..3].parse().ok()?,
                    self.map[4..6].parse().ok()?,
                    region,
                )
                .map(|spot| spot.position)
            })
    }
}
