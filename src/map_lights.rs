//! The maps' own lamps, torches and fires: the baked local lights of each map's BTL bank 0000
//! (tools/map_lights.py -> extracted/map_lights.json, in MSB space with the MapOffset applied),
//! placed in the loaded arena as Bevy point / spot lights.
//! After sekiro-rs (MIT / Apache-2.0, AKJama) crates/game/src/map_lights.rs: the exe squares the
//! light's RGB before its power (FUN_1403892e0), the intensity shares the sun's unit
//! (`map::SCENE_TO_BEVY`, x 4 pi for a point light's lumens), the range is the authored one, a
//! spot's cone is its full angle in degrees, and its turn is Euler YXZ (-ry, rx, -rz) then
//! half a turn (Bevy's spot shines along -Z).
//! gap: the BTL's native unit, its specular colour / power and sharpness, and which bank a story
//! state loads (only bank 0000, a first visit's) are not traced; shadows stay off (no Ashina
//! light casts one).

use bevy::prelude::*;

use crate::map::MapLighting;

pub struct MapLightsPlugin;

impl Plugin for MapLightsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, place.run_if(resource_exists::<MapLighting>));
    }
}

#[derive(Component)]
struct MapLight;

/// Lights farther than this past the arena's origin are left out (the exported arenas reach
/// about 60 m; a light's range is at most some tens of metres).
const REACH: f32 = 150.0;

/// One BTL light in the arena: (position, linear colour, lumens / candela, range, spot turn
/// and half cone).
struct Placed {
    pos: Vec3,
    color: Color,
    intensity: f32,
    range: f32,
    spot: Option<(Quat, f32)>,
}

fn arena_lights(source_map: &str, origin: Vec3, arena_yaw: f32) -> Vec<Placed> {
    static ALL: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();
    let all = ALL.get_or_init(|| std::fs::read_to_string(crate::paths::extracted().join("map_lights.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default());
    let f = |v: &serde_json::Value| v.as_f64().unwrap_or(0.0) as f32;
    let v3 = |v: &serde_json::Value| Vec3::new(f(&v[0]), f(&v[1]), f(&v[2]));
    let (s, c) = arena_yaw.sin_cos();
    all[source_map]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|l| {
            // MSB -> arena: about the export origin, turned by the arena yaw, X mirrored (as
            // enemy.rs msb_patrol_on).
            let d = v3(&l["pos"]) - origin;
            let pos = Vec3::new(-(d.x * c + d.z * s), d.y, -d.x * s + d.z * c);
            let range = f(&l["range"]);
            let intensity = f(&l["intensity"]);
            if intensity <= 0.0 || range <= 0.0 || Vec2::new(pos.x, pos.z).length() > REACH + range {
                return None;
            }
            let rgb = Vec3::new(f(&l["color"][0]), f(&l["color"][1]), f(&l["color"][2])) / 255.0;
            let rgb = rgb * rgb;
            let spot = (l["kind"].as_u64() == Some(1)).then(|| {
                let r = v3(&l["rot"]);
                let turn = Quat::from_rotation_y(-arena_yaw) * Quat::from_euler(EulerRot::YXZ, -r.y, r.x, -r.z) * Quat::from_rotation_y(std::f32::consts::PI);
                (turn, f(&l["cone"]).to_radians() * 0.5)
            });
            Some(Placed { pos, color: Color::linear_rgb(rgb.x, rgb.y, rgb.z), intensity, range, spot })
        })
        .collect()
}

/// Puts up the loaded map's lights (again after a map switch: the stem changes).
fn place(mut commands: Commands, lighting: Res<MapLighting>, old: Query<Entity, With<MapLight>>, mut shown: Local<Option<String>>) {
    if shown.as_deref() == Some(lighting.stem.as_str()) {
        return;
    }
    *shown = Some(lighting.stem.clone());
    for e in &old {
        commands.entity(e).despawn();
    }
    if std::env::var("SHINOBI_NO_MAP_LIGHTS").is_ok() {
        return;
    }
    let Some(origin) = lighting.map_origin else { return };
    let lights = arena_lights(&lighting.source_map, origin, lighting.arena_yaw);
    let lumens = crate::map::SCENE_TO_BEVY * 4.0 * std::f32::consts::PI;
    for l in &lights {
        let mut e = commands.spawn((MapLight, crate::map::MapPiece, Name::new("map light")));
        match l.spot {
            Some((turn, half)) => {
                e.insert((SpotLight { color: l.color, intensity: l.intensity * lumens, range: l.range, radius: 0.0, inner_angle: 0.0, outer_angle: half, shadow_maps_enabled: false, ..default() }, Transform::from_translation(l.pos).with_rotation(turn)));
            }
            None => {
                e.insert((PointLight { color: l.color, intensity: l.intensity * lumens, range: l.range, radius: 0.0, shadow_maps_enabled: false, ..default() }, Transform::from_translation(l.pos)));
            }
        }
    }
    info!("map lights: {} of {}'s bank 0000 placed ({} spots)", lights.len(), lighting.source_map, lights.iter().filter(|l| l.spot.is_some()).count());
}

#[cfg(test)]
mod tests {
    /// The castle's bank 0000 (34 lights, 2 spots, sekiro-rs btl.rs's census) round the castle
    /// top's export, and its first light's warm colour squared.
    #[test]
    fn the_castle_lights_come_from_its_bank() {
        let dir = crate::paths::extracted();
        let Ok(text) = std::fs::read_to_string(dir.join("map_lights.json")) else {
            eprintln!("no map_lights.json: skipped");
            return;
        };
        let all: serde_json::Value = serde_json::from_str(&text).unwrap();
        let castle = all["m11_01_00_00"].as_array().unwrap();
        assert_eq!(castle.len(), 34);
        assert_eq!(castle.iter().filter(|l| l["kind"] == 1).count(), 2);
        assert_eq!(castle[0]["color"], serde_json::json!([255, 121, 12]));
        assert_eq!(all["m11_00_00_00"].as_array().unwrap().len(), 16);
        // At the export origin of a light, it sits at the arena's centre.
        let p = &castle[0]["pos"];
        let origin = bevy::prelude::Vec3::new(p[0].as_f64().unwrap() as f32, p[1].as_f64().unwrap() as f32, p[2].as_f64().unwrap() as f32);
        let placed = super::arena_lights("m11_01_00_00", origin, 0.3);
        assert!(placed.iter().any(|l| l.pos.length() < 1e-3));
    }
}
