//! Native MSB SFX and parameter-resolved first-visit object effects.
use crate::{
    fxr::FxrPlayer,
    map::{
        story::MapStory,
        world::{MapWorld, RequestState},
    },
};
// Alias avoids Bevy 0.20 prelude ContextExt ambiguity.
use anyhow::{Context as ContextExt, Result};
use bevy::prelude::*;
use sekiro_formats::msb::MapRegion;
use std::{collections::BTreeMap, path::Path};

struct Placement {
    block: String,
    effect: i32,
    transform: Transform,
    enabled: bool,
    weather: bool,
    camera_relative: bool,
    activation: Option<crate::map::visibility::VisibilityPart>,
    /// A Sculptor's Idol's idle effect, by the idol's bonfire entity: it shows only while the
    /// idol's lit state matches (#154).
    idol: Option<(i32, sekiro_sim::map_effects::IdolIdle)>,
}

/// The bonfire entities of the play world's idols Wolf has lit (knelt at) so far: their unlit
/// idle effect gives way to the lit one, the flame above the idol's hands.
#[derive(Resource, Default)]
pub(crate) struct FoundIdols(pub std::collections::BTreeSet<i32>);

#[derive(Resource)]
struct MapEffects {
    placements: Vec<Placement>,
    regions: BTreeMap<String, Vec<MapRegion>>,
}

pub(crate) fn install(app: &mut App, dir: &Path) -> Result<()> {
    let primary = dir.file_name().and_then(|s| s.to_str()).context("map ID")?;
    if !sekiro_sim::world_events::SUPPORTED_MAPS.contains(&primary) {
        return Ok(());
    }
    let cache = dir
        .parent()
        .and_then(Path::parent)
        .context("cache directory")?;
    let mut fx = MapEffects {
        placements: Vec::new(),
        regions: BTreeMap::new(),
    };
    if !app.world().contains_resource::<FxrPlayer>() {
        app.add_plugins(crate::fxr::plugin(cache.to_owned(), |_| {}));
    }
    for block in sekiro_sim::world_events::SUPPORTED_MAPS {
        if block != primary && app.world().get_resource::<MapWorld>().is_none() {
            continue;
        }
        let state = app
            .world()
            .get_resource::<MapStory>()
            .and_then(|s| s.maps.get(block))
            .map(|s| s.state.clone())
            .or_else(|| {
                sekiro_sim::world_events::first_visit(cache, block)
                    .ok()
                    .flatten()
            })
            .context("map FX story state")?;
        let catalog =
            sekiro_sim::map_effects::load(cache, block, &state).map_err(anyhow::Error::msg)?;
        let fallback;
        let layout = if let Some(world) = app.world().get_resource::<MapWorld>() {
            &world.blocks.get(block).context("FX map block")?.json
        } else {
            fallback = serde_json::from_slice::<serde_json::Value>(&std::fs::read(
                cache.join(format!("maps/{block}/layout.json")),
            )?)?;
            &fallback
        };
        let mut parents = BTreeMap::new();
        for row in layout["parts"].as_array().context("FX parts")? {
            if !matches!(row["kind"].as_str(), Some("MapPiece" | "Object")) {
                continue;
            }
            if let Some(part) = crate::map::visibility::VisibilityPart::from_row(block, row)? {
                parents.insert(part.name.clone(), part);
            }
        }
        let events: BTreeMap<_, _> = state
            .parts(layout)
            .into_iter()
            .map(|p| (p.name, p.enabled))
            .collect();
        info!(
            "map FX {block}: {} placements, {} enabled",
            catalog.placements.len(),
            catalog.placements.iter().filter(|p| p.enabled).count()
        );
        for p in catalog.placements {
            let (camera_relative, weather) = app
                .world_mut()
                .resource_mut::<FxrPlayer>()
                .map_actions(p.effect);
            fx.placements.push(Placement {
                block: block.into(),
                effect: p.effect,
                transform: Transform::from_translation(Vec3::from_array(p.position))
                    .with_rotation(Quat::from_array(p.rotation)),
                enabled: p.enabled
                    && p.activation
                        .as_ref()
                        .is_none_or(|name| events.get(name).copied().unwrap_or(true)),
                weather: weather && camera_relative,
                camera_relative,
                activation: p.activation.and_then(|name| parents.get(&name).cloned()),
                idol: p.idol.map(|idle| (p.entity, idle)),
            });
        }
        fx.regions.insert(block.into(), catalog.regions);
    }
    if !app.world().contains_resource::<FxrPlayer>() {
        app.add_plugins(crate::fxr::plugin(cache.to_owned(), |_| {}));
    }
    app.insert_resource(fx).add_systems(
        PostUpdate,
        update.after(bevy::transform::TransformSystems::Propagate),
    );
    Ok(())
}

fn contains(regions: &[MapRegion], index: usize, point: Vec3, depth: usize) -> bool {
    if depth > 8 {
        return false;
    }
    let Some(r) = regions.get(index) else {
        return false;
    };
    if r.shape == 6 {
        return r
            .children
            .iter()
            .any(|&i| i >= 0 && contains(regions, i as usize, point, depth + 1));
    }
    let rot = crate::fxr_tree::game_rot([r.rotation[0], r.rotation[1], r.rotation[2], 0.]);
    let p = rot.inverse() * (point - Vec3::new(-r.position[0], r.position[1], r.position[2]));
    match (r.shape, r.dimensions.as_slice()) {
        (5, [w, d, h]) => p.x.abs() <= w / 2. && p.z.abs() <= d / 2. && p.y >= 0. && p.y <= *h,
        (2, [radius]) => p.length_squared() <= radius * radius,
        _ => false,
    }
}

#[allow(clippy::too_many_arguments)]
fn update(
    fx: Res<MapEffects>,
    mut player: ResMut<FxrPlayer>,
    cameras: Query<
        (
            &GlobalTransform,
            &Camera,
            Option<&bevy::camera::visibility::RenderLayers>,
        ),
        With<Camera3d>,
    >,
    focus: Query<&GlobalTransform, With<crate::map::MapFocus>>,
    world: Option<Res<MapWorld>>,
    time: Res<Time>,
    clock: Res<Time<Virtual>>,
    real: Res<Time<Real>>,
    blocks: Option<Res<crate::map::visibility::VisibilityBlocks>>,
    found: Option<Res<FoundIdols>>,
    mut next_log: Local<f32>,
) {
    if clock.is_paused()
        && blocks
            .as_ref()
            .is_some_and(|b| b.0.values().any(|b| b.groups_off))
    {
        return;
    }
    let Some((camera, _, _)) = cameras.iter().find(|(_, c, layers)| {
        c.is_active
            && c.order == 0
            && layers
                .is_none_or(|l| l.intersects(&bevy::camera::visibility::RenderLayers::layer(0)))
    }) else {
        return;
    };
    let point = focus
        .iter()
        .next()
        .map_or(camera.translation(), |f| f.translation());
    let disabled = std::env::var_os("SEKIRO_NO_MAP_FX").is_some();
    for (i, p) in fx.placements.iter().enumerate() {
        let ready = world.as_ref().is_none_or(|w| {
            w.blocks
                .get(&p.block)
                .is_some_and(|b| b.state == RequestState::Ready)
        });
        let activation = p.activation.as_ref().is_none_or(|part| {
            blocks.as_ref().is_none_or(|blocks| {
                blocks.0.get(&p.block).is_some_and(|block| {
                    crate::map::visibility::decide(part, block, p.enabled).residency
                        != crate::map::visibility::ResidencyRequest::None
                })
            })
        });
        let indoors = p.weather
            && fx.regions.get(&p.block).is_some_and(|regions| {
                regions.iter().enumerate().any(|(i, r)| {
                    matches!(r.entity_id, 1102107 | 1112150 | 1112151)
                        && contains(regions, i, point, 0)
                })
            });
        let idle = p.idol.is_none_or(|(entity, idle)| {
            idle.shows(found.as_ref().is_some_and(|f| f.0.contains(&entity)))
        });
        let on = !disabled
            && p.enabled
            && idle
            && ready
            && activation
            && !indoors
            && (!p.camera_relative
                || world.as_ref().is_none_or(|w| {
                    w.selected
                        .as_ref()
                        .map_or(w.primary == p.block, |(block, _)| *block == p.block)
                }))
            && (p.camera_relative
                || p.transform
                    .translation
                    .distance_squared(camera.translation())
                    < 120f32.powi(2));
        let at = if p.camera_relative {
            Transform::from_translation(camera.translation())
        } else {
            p.transform
        };
        player.map_effect(
            p.effect,
            i as i32,
            at,
            on,
            clock.is_paused() && std::env::var_os("SEKIRO_MAP_FX_REALTIME").is_none(),
        );
    }
    let dt = if std::env::var_os("SEKIRO_MAP_FX_REALTIME").is_some() {
        real.delta_secs()
    } else if clock.is_paused() {
        0.
    } else {
        time.delta_secs()
    };
    player.advance_map(dt.min(1. / 15.));
    // Weather density around the camera twice a second, to check it falls steadily (#84).
    if std::env::var_os("SEKIRO_FXR_LOG").is_some() && time.elapsed_secs() >= *next_log {
        *next_log = time.elapsed_secs() + 0.5;
        // Counts sum every placement of an effect, so log each effect once.
        let effects: std::collections::BTreeSet<i32> = fx
            .placements
            .iter()
            .filter(|p| p.weather)
            .map(|p| p.effect)
            .collect();
        for effect in effects {
            info!(
                "weather {} near camera {} at {:.1?}",
                effect,
                player.map_particles_near(effect, camera.translation(), 5.),
                camera.translation()
            );
        }
    }
}
