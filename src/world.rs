//! The arena (checkered floor, pillars, sun) and the blade visual each
//! fighter carries. The blade swings while a TAE hitbox window is active.

use bevy::prelude::*;

use crate::actor::{Actor, data_for};
use crate::data::Combat;

pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_world)
            .add_systems(PostStartup, (crate::player::player_visuals, crate::enemy::enemy_visuals))
            .add_systems(Update, pose_blades);
    }
}

const TILE_SIZE: f32 = 2.0;
const TILES_PER_SIDE: i32 = 30;

#[derive(Component)]
pub struct Blade;

pub fn spawn_blade(parent: &mut ChildSpawnerCommands, meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>, color: Color) {
    parent
        .spawn((Blade, Transform::from_xyz(0.35, 0.1, -0.2), Visibility::default(), Name::new("Blade")))
        .with_children(|b| {
            b.spawn((
                Mesh3d(meshes.add(Cuboid::new(0.04, 0.06, 1.0))),
                MeshMaterial3d(materials.add(StandardMaterial { base_color: color, metallic: 0.8, perceptual_roughness: 0.3, ..default() })),
                Transform::from_xyz(0.0, 0.0, -0.5),
            ));
        });
}

/// Rest: blade held low and forward. Wind-up: from the anim start the blade rises
/// toward a raised pose, reaching it at the first hitbox frame (a stand-in telegraph
/// until real animations play). Hitbox active: blade swept across the front.
fn pose_blades(combat: Res<Combat>, actors: Query<(&Actor, &Children)>, mut blades: Query<&mut Transform, With<Blade>>) {
    for (a, children) in &actors {
        let d = data_for(&combat, &a);
        let windows = d.attack_windows(&a.anim);
        let damaging = |atk: &crate::data::Attack| atk.atk_stam > 0.0 || atk.atk_phys > 0.0;
        let active = windows.iter().find(|(e, atk, _)| e.in_time(a.t) && damaging(atk));
        let upcoming = windows.iter().filter(|(e, atk, _)| e.start > a.t && damaging(atk)).map(|(e, _, _)| e.start).reduce(f32::min);
        let guarding = !a.anim.is_empty() && d.flag(&a.anim, a.t, crate::player::FLAG_SHIELD_BLOCK);
        let rest = Transform::from_xyz(0.35, 0.1, -0.2).with_rotation(Quat::from_rotation_x(-0.5));
        let raised = Transform::from_xyz(0.25, 0.75, 0.05).with_rotation(Quat::from_rotation_y(1.2) * Quat::from_rotation_x(1.1));
        for c in children.iter() {
            let Ok(mut tf) = blades.get_mut(c) else { continue };
            *tf = if let Some((e, _, _)) = active {
                let f = ((a.t - e.start) / (e.end - e.start).max(1e-3)).clamp(0.0, 1.0);
                Transform::from_xyz(0.0, 0.3, -0.1).with_rotation(Quat::from_rotation_y(1.2 - 2.4 * f) * Quat::from_rotation_x(-0.1))
            } else if let Some(hit) = upcoming {
                let f = (a.t / hit.max(1e-3)).clamp(0.0, 1.0).powf(0.6);
                Transform {
                    translation: rest.translation.lerp(raised.translation, f),
                    rotation: rest.rotation.slerp(raised.rotation, f),
                    scale: Vec3::ONE,
                }
            } else if guarding {
                Transform::from_xyz(0.1, 0.45, -0.35).with_rotation(Quat::from_rotation_y(1.45) * Quat::from_rotation_z(0.3))
            } else {
                rest
            };
        }
    }
}

fn spawn_world(config: Res<crate::config::GameConfig>, mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    // The game's own map (map.rs) brings its floor, walls and draw-param lighting.
    if crate::map::map_id(&config).is_some() {
        return;
    }
    let tile = meshes.add(Plane3d::default().mesh().size(TILE_SIZE, TILE_SIZE));
    let light_tile = materials.add(Color::srgb(0.42, 0.45, 0.38));
    let dark_tile = materials.add(Color::srgb(0.32, 0.35, 0.29));
    let half = TILES_PER_SIDE / 2;
    for x in -half..half {
        for z in -half..half {
            let material = if (x + z) % 2 == 0 { &light_tile } else { &dark_tile };
            commands.spawn((
                Mesh3d(tile.clone()),
                MeshMaterial3d(material.clone()),
                Transform::from_xyz((x as f32 + 0.5) * TILE_SIZE, 0.0, (z as f32 + 0.5) * TILE_SIZE),
            ));
        }
    }

    let pillar = meshes.add(Cuboid::new(0.8, 4.0, 0.8));
    let stone = materials.add(Color::srgb(0.55, 0.5, 0.45));
    for i in 0..8 {
        let angle = i as f32 / 8.0 * std::f32::consts::TAU;
        commands.spawn((Mesh3d(pillar.clone()), MeshMaterial3d(stone.clone()), Transform::from_xyz(angle.cos() * 14.0, 2.0, angle.sin() * 14.0)));
    }

    // Key light (warm, shadows), a cool fill from the opposite side and a sky-coloured ambient
    // so the dark cloth and armour textures keep their detail in the shade.
    commands.spawn((
        DirectionalLight { illuminance: 12_000.0, color: Color::srgb(1.0, 0.95, 0.88), shadow_maps_enabled: true, ..default() },
        Transform::from_xyz(8.0, 15.0, 6.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        DirectionalLight { illuminance: 2_000.0, color: Color::srgb(0.75, 0.82, 1.0), shadow_maps_enabled: false, ..default() },
        Transform::from_xyz(-6.0, 6.0, -8.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    // Hand-picked (no map lighting yet). 1200 rather than 300: with the shade that dark, the
    // normal maps of the matte cloth turned into hard light/dark pixels near the light's edge
    // (Wolf's coat looked speckled and shimmered); the game's ambient keeps that side soft.
    // SHINOBI_AMBIENT overrides it for checks.
    let ambient = std::env::var("SHINOBI_AMBIENT").ok().and_then(|v| v.parse().ok()).unwrap_or(1200.0);
    commands.insert_resource(GlobalAmbientLight { color: Color::srgb(0.8, 0.85, 1.0), brightness: ambient, ..default() });
}
