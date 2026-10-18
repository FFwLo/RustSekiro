//! The maps' placed effects: the MSB SFX / WindSFX regions round the loaded arena (snowfall,
//! mist, drifting leaves, embers, waterfalls' spray), from tools/map_fx.py ->
//! extracted/map_fx.json, played by the FXR player (fxr.rs) as lasting effects.
//! After sekiro-rs (MIT / Apache-2.0, AKJama) docs/MAP-FX.md: regions are placed as they are
//! (no MapOffset), a start-off one is on when the first visit's event script spawns it (2006[2];
//! the castle's snow 1114350, m11_01 11115170; the Outskirts' 1108010, m11_00 11105107), and
//! the camera-following ones (snow "雪_カメラ前", node movement 46) ride the camera (fxr.rs).
//! gap: the ModelSfxParam effects on placed objects (torch flames, lanterns' glow), the events'
//! later deletes (the snow stops for the Tomoe storm, m11_01 11115820; indoors, 3[2] regions),
//! and the regions' activation parts are not done.

use bevy::prelude::*;

use crate::map::MapLighting;

pub struct MapFxPlugin;

impl Plugin for MapFxPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, place.run_if(resource_exists::<MapLighting>));
    }
}

#[derive(Component)]
struct MapFx;

/// Regions farther than this from the arena's origin are left out (the exported arenas reach
/// about 60 m; camera-following ones are placed anyway).
const REACH: f32 = 120.0;

/// (FXR id, arena transform) of the loaded map's first-visit effects.
fn arena_fx(source_map: &str, origin: Vec3, arena_yaw: f32) -> Vec<(i64, Transform)> {
    static ALL: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();
    let all = ALL.get_or_init(|| std::fs::read_to_string(crate::paths::extracted().join("map_fx.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default());
    let f = |v: &serde_json::Value| v.as_f64().unwrap_or(0.0) as f32;
    all[source_map]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|e| e["on"].as_bool() == Some(true))
        .filter_map(|e| {
            let p = &e["pos"];
            let pos = crate::fog_wall::map_to_arena(origin, arena_yaw, Vec3::new(f(&p[0]), f(&p[1]), f(&p[2])));
            let follows = e["name"].as_str().is_some_and(|n| n.contains("カメラ"));
            if !follows && Vec2::new(pos.x, pos.z).length() > REACH {
                return None;
            }
            let r = &e["rot"];
            let rot = crate::fog_wall::part_rotation(arena_yaw, [f(&r[0]), f(&r[1]), f(&r[2])]);
            Some((e["fxr"].as_i64()?, Transform::from_translation(pos).with_rotation(rot)))
        })
        .collect()
}

/// Starts the loaded map's effects (again after a map switch: the old ones stop).
fn place(mut commands: Commands, lighting: Res<MapLighting>, old: Query<(Entity, &mut crate::fxr::FxEffect), With<MapFx>>, mut shown: Local<Option<String>>) {
    if shown.as_deref() == Some(lighting.stem.as_str()) {
        return;
    }
    *shown = Some(lighting.stem.clone());
    for (e, _) in &old {
        commands.entity(e).despawn();
    }
    if std::env::var("SHINOBI_NO_MAP_FX").is_ok() {
        return;
    }
    let Some(origin) = lighting.map_origin else { return };
    let mut fx = arena_fx(&lighting.source_map, origin, lighting.arena_yaw);
    // `SHINOBI_MAP_FX_ONLY=<id>,<id>`: only these effects (checks).
    if let Ok(only) = std::env::var("SHINOBI_MAP_FX_ONLY") {
        let ids: Vec<i64> = only.split(',').filter_map(|v| v.trim().parse().ok()).collect();
        fx.retain(|(id, _)| ids.contains(id));
    }
    for (id, at) in &fx {
        let mut e = crate::fxr::FxEffect::new(*id, None, false, *at);
        e.keep = true;
        commands.spawn((e, MapFx, Name::new(format!("map fx {id}"))));
    }
    let mut ids: Vec<i64> = fx.iter().map(|(id, _)| *id).collect();
    ids.sort_unstable();
    ids.dedup();
    info!("map fx: {} effects of {} placed ({ids:?})", fx.len(), lighting.source_map);
}

#[cfg(test)]
mod tests {
    /// The census sekiro-rs reports (157 Outskirts / 95 castle regions), the castle's snowfall
    /// on for a first visit and the Outskirts' invasion embers off.
    #[test]
    fn the_castle_snow_falls_on_a_first_visit() {
        let Ok(text) = std::fs::read_to_string(crate::paths::extracted().join("map_fx.json")) else {
            eprintln!("no map_fx.json: skipped");
            return;
        };
        let all: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(all["m11_00_00_00"].as_array().unwrap().len(), 157);
        assert_eq!(all["m11_01_00_00"].as_array().unwrap().len(), 95);
        let ent = |m: &str, id: i64| all[m].as_array().unwrap().iter().find(|e| e["entity"] == id).cloned().unwrap();
        assert_eq!(ent("m11_01_00_00", 1114350)["fxr"], 839500);
        assert_eq!(ent("m11_01_00_00", 1114350)["on"], true);
        assert_eq!(ent("m11_00_00_00", 1108010)["on"], true);
        assert_eq!(ent("m11_00_00_00", 1108020)["on"], false);
        // The snow follows the camera: placed even far from the arena.
        let placed = super::arena_fx("m11_01_00_00", bevy::prelude::Vec3::new(1.0e5, 0.0, 0.0), 0.0);
        assert!(placed.iter().any(|(id, _)| *id == 839500));
    }
}
