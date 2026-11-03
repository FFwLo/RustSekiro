//! Event-controlled map parts, objects and placed effects for arena staging.

use std::collections::HashMap;
use std::path::Path;

use crate::map::visibility::{ArenaVisibility, EventState, VisibilityInputs, VisibilityPart};
use bevy::prelude::*;
use sekiro_sim::arena::scene::SceneState;
use sekiro_sim::arena_b::MapEffect;

#[derive(Component, Clone)]
pub struct Part {
    pub entity: i32,
    pub groups: [i32; 8],
}

#[derive(Resource, Default)]
pub struct Parts(pub HashMap<String, Part>);

#[derive(Resource, Default)]
pub struct Scene(pub SceneState);

#[derive(Resource)]
struct Effects(Vec<MapEffect>);

#[derive(Resource)]
struct StoryHour {
    initial: f32,
    explicit: bool,
    legacy_sky: bool,
}

#[derive(Resource)]
struct VisibilityMap(String);

pub fn install(app: &mut App, cache: &Path, map: &str) -> anyhow::Result<()> {
    let initial = app.world().resource::<crate::lighting::MapLighting>().hour;
    let bytes = std::fs::read(cache.join(format!("raw/map/MapStudio/{map}.msb")))?;
    let msb = sekiro_formats::msb::parse(&bytes)?;
    let parts = msb
        .parts
        .into_iter()
        .map(|p| {
            (
                p.name,
                Part {
                    entity: p.entity_id,
                    groups: p.entity_group_ids,
                },
            )
        })
        .collect();
    app.insert_resource(Parts(parts))
        .insert_resource(VisibilityMap(map.to_string()))
        .init_resource::<ArenaVisibility>()
        .insert_resource(StoryHour {
            initial,
            explicit: std::env::args().any(|a| a == "--hour"),
            legacy_sky: crate::map::sky::legacy_selection(),
        })
        .init_resource::<Scene>()
        .insert_resource(Effects(
            sekiro_sim::arena_b::map_effects(cache, map).map_err(anyhow::Error::msg)?,
        ))
        .add_systems(PostUpdate, visibility.in_set(VisibilityInputs))
        .add_systems(PostUpdate, (effects, lighting));
    Ok(())
}

fn visibility(
    scene: Res<Scene>,
    map: Res<VisibilityMap>,
    mut arena: ResMut<ArenaVisibility>,
    mut parts: Query<(&Part, &mut Visibility), Without<VisibilityPart>>,
) {
    // Managed roots receive one combined decision in Job E's writer.
    // Replace the snapshot even when empty, so restarts release the previous override.
    let unchanged = arena.maps.get(&map.0).is_some_and(|state| {
        state.by_name.is_empty()
            && state.by_entity.len() == scene.0.enabled.len()
            && scene
                .0
                .enabled
                .iter()
                .all(|(id, on)| state.by_entity.get(id) == Some(on))
    });
    if !unchanged {
        let next = EventState {
            by_name: Default::default(),
            by_entity: scene.0.enabled.iter().map(|(&id, &on)| (id, on)).collect(),
        };
        crate::change::insert_if_neq(
            arena.reborrow().map_unchanged(|a| &mut a.maps),
            map.0.clone(),
            next,
        );
    }
    for (part, mut visibility) in &mut parts {
        let on = scene
            .0
            .enabled
            .get(&part.entity)
            .or_else(|| part.groups.iter().find_map(|g| scene.0.enabled.get(g)));
        if let Some(&on) = on {
            visibility.set_if_neq(if on {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            });
        }
    }
}

fn effects(scene: Res<Scene>, effects: Res<Effects>, mut fx: ResMut<crate::fxr::FxrPlayer>) {
    static DISABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    let disabled = *DISABLED.get_or_init(|| std::env::args().any(|a| a == "--no-arena-effects"));
    for effect in &effects.0 {
        // An absent entry after event restart releases the previous scene's hold.
        let on = scene
            .0
            .effects
            .get(&effect.entity)
            .copied()
            .unwrap_or(false);
        let [rx, ry, rz] = effect.rotation.map(f32::to_radians);
        let at = Transform::from_translation(effect.position.into()).with_rotation(
            Quat::from_rotation_y(ry) * Quat::from_rotation_z(rz) * Quat::from_rotation_x(rx),
        );
        fx.hold(
            crate::fxr::HELD,
            effect.effect,
            effect.entity,
            at,
            on && !disabled,
        );
    }
}

fn lighting(
    scene: Res<Scene>,
    story: Res<StoryHour>,
    mut light: ResMut<crate::lighting::MapLighting>,
    mut probes: ResMut<crate::lighting::MapProbes>,
    selection: Res<crate::lighting_selection::LightingSelection>,
) {
    if story.explicit {
        return;
    }
    let hour = scene
        .0
        .lighting
        .and_then(sekiro_sim::story::StoryTime::from_variant)
        .map_or(story.initial, sekiro_sim::story::StoryTime::hour);
    if light.hour != hour {
        light.hour = hour;
        light.applied = None;
        if story.legacy_sky {
            probes.variant = selection.environment_for_hour(hour);
            probes.active = None;
            probes.handles.clear();
        }
    }
    let environment = scene
        .0
        .environment
        .map(u32::from)
        .or_else(|| (!story.legacy_sky).then_some(selection.initial.environment));
    if let Some(variant) = environment
        && probes.variant != variant
    {
        probes.variant = variant;
        probes.active = None;
        probes.handles.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lighting_clock_preserves_environment_until_its_own_event_and_restart() {
        let cache = sekiro_sim::cache::root();
        let id = "m11_01_00_00";
        let initial = crate::lighting_selection::select(&cache, id, None, None).unwrap();
        let selection =
            crate::lighting_selection::LightingSelection::load(&cache, id, initial, false).unwrap();
        let mut app = App::new();
        app.insert_resource(Scene::default())
            .insert_resource(StoryHour {
                initial: 6.,
                explicit: false,
                legacy_sky: false,
            })
            .insert_resource(crate::lighting::MapLighting {
                gparam: default(),
                hour: 6.,
                light_set: 0,
                applied: Some(0),
                source: String::new(),
            })
            .insert_resource(crate::lighting::MapProbes {
                regions: vec![],
                available: default(),
                prefix: String::new(),
                variant: 1,
                active: Some(123),
                handles: default(),
            })
            .insert_resource(selection)
            .add_systems(Update, lighting);
        app.world_mut().resource_mut::<Scene>().0.lighting = Some(4);
        app.update();
        assert_eq!(
            app.world().resource::<crate::lighting::MapLighting>().hour,
            22.
        );
        let probes = app.world().resource::<crate::lighting::MapProbes>();
        assert_eq!(probes.variant, 1);
        assert_eq!(probes.active, Some(123));
        app.world_mut().resource_mut::<Scene>().0.environment = Some(3);
        app.update();
        let probes = app.world().resource::<crate::lighting::MapProbes>();
        assert_eq!(probes.variant, 3);
        assert_eq!(probes.active, None);
        app.world_mut().resource_mut::<Scene>().0 = default();
        app.update();
        assert_eq!(
            app.world().resource::<crate::lighting::MapLighting>().hour,
            6.
        );
        assert_eq!(
            app.world().resource::<crate::lighting::MapProbes>().variant,
            1
        );
    }

    #[test]
    fn managed_roots_combine_groups_events_and_residency_across_restarts() {
        use crate::map::visibility::{
            AuthoredVisibility, BlockRequest, BlockVisibility, MapVisibilityPlugin, SourceMasks,
            VisibilityBlocks, VisibilityDecision,
        };
        let map = "m11_01_00_00";
        let mut app = App::new();
        app.add_plugins(MapVisibilityPlugin)
            .insert_resource(Scene::default())
            .insert_resource(VisibilityMap(map.into()))
            .add_systems(PostUpdate, visibility.in_set(VisibilityInputs));
        let mut draw = [0; 8];
        draw[0] = 1;
        let source = SourceMasks {
            primary_draw: draw,
            alternative_draw: [0; 8],
            draw_selector: [0; 8],
            primary_backread: draw,
            alternative_backread: [0; 8],
            backread_selector: [0; 8],
            condition1: 0,
            condition2: 0,
        };
        let root = app
            .world_mut()
            .spawn((
                VisibilityPart {
                    map: map.into(),
                    name: "door".into(),
                    entity_id: 1101000,
                    entity_groups: [-1; 8],
                    effective: source.effective("Object"),
                    source,
                },
                Part {
                    entity: 1101000,
                    groups: [-1; 8],
                },
                Visibility::Inherited,
            ))
            .id();
        let block = BlockVisibility {
            resident: true,
            ready: true,
            draw,
            request: BlockRequest {
                normal: draw,
                ..default()
            },
            ..default()
        };
        app.world_mut()
            .resource_mut::<VisibilityBlocks>()
            .0
            .insert(map.into(), block);
        app.world_mut()
            .resource_mut::<Scene>()
            .0
            .enabled
            .insert(1101000, true);
        app.world_mut()
            .resource_mut::<VisibilityBlocks>()
            .0
            .get_mut(map)
            .unwrap()
            .draw = [0; 8];
        app.update();
        assert_eq!(
            *app.world().get::<Visibility>(root).unwrap(),
            Visibility::Hidden
        );
        let changed = app
            .world()
            .entity(root)
            .get_ref::<Visibility>()
            .unwrap()
            .last_changed();
        app.update();
        assert_eq!(
            app.world()
                .entity(root)
                .get_ref::<Visibility>()
                .unwrap()
                .last_changed(),
            changed,
            "arena staging must not write a managed root even when the final state is hidden"
        );
        app.world_mut()
            .resource_mut::<VisibilityBlocks>()
            .0
            .get_mut(map)
            .unwrap()
            .draw = draw;
        app.update();
        assert_eq!(
            *app.world().get::<Visibility>(root).unwrap(),
            Visibility::Inherited
        );
        app.world_mut()
            .resource_mut::<Scene>()
            .0
            .enabled
            .insert(1101000, false);
        app.update();
        assert!(!app.world().get::<VisibilityDecision>(root).unwrap().visible);
        app.world_mut().resource_mut::<Scene>().0 = SceneState::default();
        app.world_mut()
            .resource_mut::<VisibilityBlocks>()
            .0
            .get_mut(map)
            .unwrap()
            .ready = false;
        app.update();
        assert_eq!(
            *app.world().get::<Visibility>(root).unwrap(),
            Visibility::Hidden
        );
        app.world_mut()
            .resource_mut::<VisibilityBlocks>()
            .0
            .get_mut(map)
            .unwrap()
            .ready = true;
        app.update();
        assert_eq!(
            *app.world().get::<Visibility>(root).unwrap(),
            Visibility::Inherited
        );
        app.world_mut()
            .resource_mut::<AuthoredVisibility>()
            .maps
            .insert(
                map.into(),
                EventState {
                    by_name: std::collections::BTreeMap::from([("door".into(), false)]),
                    ..default()
                },
            );
        app.update();
        assert_eq!(
            *app.world().get::<Visibility>(root).unwrap(),
            Visibility::Hidden
        );
    }

    #[test]
    fn restart_releases_held_map_effects_for_twenty_fights() {
        let cache = sekiro_sim::cache::root();
        let placed = sekiro_sim::arena_b::map_effects(&cache, "m11_01_00_00").unwrap();
        let mut app = App::new();
        app.insert_resource(Scene::default())
            .insert_resource(Effects(placed))
            .insert_resource(crate::fxr::FxrPlayer::new(cache))
            .add_systems(Update, effects);
        let entities: Vec<_> = app
            .world()
            .resource::<Effects>()
            .0
            .iter()
            .map(|e| e.entity)
            .collect();
        for fight in 0..20 {
            for &entity in &entities {
                app.world_mut()
                    .resource_mut::<Scene>()
                    .0
                    .effects
                    .insert(entity, true);
            }
            app.update();
            let active = app
                .world()
                .resource::<crate::fxr::FxrPlayer>()
                .live_counts()
                .1;
            assert!(active > 0, "installed effects must be exercised");
            // Restart replaces SceneState with a fresh event scene, without off entries.
            app.world_mut().resource_mut::<Scene>().0 = SceneState::default();
            app.update();
            assert_eq!(
                app.world()
                    .resource::<crate::fxr::FxrPlayer>()
                    .live_counts()
                    .1,
                0,
                "fight {fight}"
            );
            app.world_mut()
                .resource_mut::<crate::fxr::FxrPlayer>()
                .advance(31.);
            assert_eq!(
                app.world()
                    .resource::<crate::fxr::FxrPlayer>()
                    .live_counts(),
                (0, 0, 0)
            );
        }
    }
}
