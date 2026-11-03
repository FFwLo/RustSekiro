//! Rendering and overhead gauges for the optional MSB population.
use super::Sim;
use crate::character::{AnimLibrary, Animator, PoseDormant, PresentationLod, Rig};
use crate::draw_mask::{DrawMask, MaskSource};
use anyhow::Result;
use bevy::camera::primitives::{Frustum, Sphere};
use bevy::camera::visibility::{DynamicSkinnedMeshBounds, NoFrustumCulling};
use bevy::light::NotShadowCaster;
use bevy::mesh::skinning::SkinnedMesh;
use bevy::prelude::*;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

#[derive(Component)]
pub(super) struct FoeActor {
    index: usize,
    meshes: Vec<Entity>,
    posed_clock: f32,
    drawn: bool,
    donor: String,
}
#[derive(Component)]
struct Panel(usize);
#[derive(Component)]
struct Mark(usize);
#[derive(Resource)]
struct Libraries(HashMap<String, AnimLibrary>);

pub(super) fn install(app: &mut App, cache: &Path) -> Result<()> {
    let assets = app.world().resource::<AssetServer>().clone();
    let sim = app.world().non_send::<Sim>();
    let population = sim.duel.population.as_ref().unwrap();
    let roster = population.registry.as_ref().unwrap();
    let mut libraries = HashMap::new();
    let mut actors = Vec::new();
    let mut sound_foes = Vec::new();
    for (index, foe) in population.foes.iter().enumerate() {
        let spec = &foe.duel.spec;
        let donor = roster.anim_chr(&spec.chr, spec.npc_param);
        if let std::collections::hash_map::Entry::Vacant(entry) = libraries.entry(donor.clone()) {
            let mut lib = AnimLibrary::load(cache, &donor)?;
            lib.clip("a000_000000")?;
            entry.insert(lib);
        }
        let lib = libraries.get_mut(&donor).unwrap();
        if foe.active() {
            lib.prefetch_all();
        }
        anyhow::ensure!(
            cache.join(format!("models/{}.glb", spec.chr)).is_file(),
            "map enemies: missing model {}",
            spec.chr
        );
        let mut idle = Animator::new(lib.clip("a000_000000")?, false);
        idle.apply_root_motion = false;
        idle.speed = 0.0;
        sound_foes.push((spec.chr.clone(), spec.npc_param, foe.active()));
        actors.push((
            FoeActor {
                index,
                meshes: Vec::new(),
                posed_clock: f32::NAN,
                drawn: false,
                donor,
            },
            WorldAssetRoot(
                assets
                    .load_builder()
                    .with_settings(|settings: &mut bevy::gltf::GltfLoaderSettings| {
                        settings.skinned_mesh_bounds_policy =
                            Some(bevy::gltf::GltfSkinnedMeshBoundsPolicy::Dynamic);
                    })
                    .load(GltfAssetLabel::Scene(0).from_asset(format!("models/{}.glb", spec.chr))),
            ),
            Transform::from_translation(Vec3::from_array(foe.duel.enemy.body.position))
                .with_rotation(Quat::from_rotation_y(foe.duel.enemy.visual_yaw())),
            Visibility::Inherited,
            Rig::new(Arc::new(lib.skeleton.clone())),
            idle,
            roster
                .draw_mask(cache, spec)
                .map_or_else(|| DrawMask::new(MaskSource::Combat), DrawMask::with_groups),
            PresentationLod {
                draw: true,
                pose: true,
                cloth: foe.active(),
            },
        ));
    }
    for actor in actors {
        app.world_mut().spawn(actor);
    }
    crate::audio::prepare_population(app, &sound_foes);
    app.world_mut()
        .resource_scope(|world, mut fx: Mut<crate::fxr::FxrPlayer>| {
            fx.ensure_tables(&world.non_send::<Sim>().duel);
        });
    app.insert_resource(Libraries(libraries))
        .add_systems(Startup, spawn_gauges.before(crate::map::MapSpawn))
        .add_systems(Update, sound_banks.before(crate::audio::play_cues))
        .add_systems(
            Update,
            (
                sync.before(crate::character::animate)
                    .after(crate::character::bind_rigs),
                gauges.after(super::follow_camera),
            ),
        );
    Ok(())
}

fn sound_banks(
    sim: NonSend<Sim>,
    fmod: Option<NonSendMut<crate::audio::FmodAudio>>,
    mut previous: Local<Vec<bool>>,
    mut required: Local<Vec<String>>,
) {
    let Some(mut fmod) = fmod else {
        return;
    };
    let foes = &sim.duel.population.as_ref().unwrap().foes;
    let changed = previous.len() != foes.len()
        || previous
            .iter()
            .zip(foes)
            .any(|(was, foe)| *was != foe.active());
    if !changed && !fmod.population_banks_pending() {
        return;
    }
    if changed {
        previous.clear();
        previous.extend(foes.iter().map(|foe| foe.active()));
        required.clear();
        required.extend(
            foes.iter()
                .filter(|foe| foe.active())
                .map(|foe| foe.duel.spec.chr.clone()),
        );
        required.sort();
        required.dedup();
    }
    fmod.population_banks(required.iter().map(String::as_str));
}
fn spawn_gauges(mut commands: Commands, sim: NonSend<Sim>) {
    use crate::combat_hud::{Fill, OVERHEAD_W, bar, deathblow_mark};
    for index in 0..sim.duel.population.as_ref().unwrap().foes.len() {
        commands.spawn((Mark(index), deathblow_mark()));
        commands
            .spawn((
                Panel(index),
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Vw(OVERHEAD_W),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(3),
                    ..default()
                },
                Visibility::Hidden,
            ))
            .with_children(|panel| {
                bar(panel, Fill::Posture(index + 2), (percent(100), px(5)));
                bar(panel, Fill::Hp(index + 2), (percent(100), px(8)));
            });
    }
}
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn sync(
    mut commands: Commands,
    sim: NonSend<Sim>,
    fixed: Res<Time<Fixed>>,
    mut libraries: ResMut<Libraries>,
    cameras: Query<(&Frustum, &GlobalTransform), With<Camera3d>>,
    children: Query<&Children>,
    rigs: Query<&Rig>,
    meshes: Query<
        (
            Option<&SkinnedMesh>,
            Has<DynamicSkinnedMeshBounds>,
            Has<NotShadowCaster>,
        ),
        With<Mesh3d>,
    >,
    mut actors: Query<(
        Entity,
        &mut FoeActor,
        &mut Animator,
        &mut Transform,
        Option<&PoseDormant>,
        &mut Visibility,
        &mut PresentationLod,
    )>,
) {
    let p = sim.duel.population.as_ref().unwrap();
    for (entity, mut actor, mut anim, mut tf, dormant, mut visibility, mut presentation) in
        &mut actors
    {
        let foe = &p.foes[actor.index];
        let position = Vec3::from_array(foe.duel.enemy.body.position);
        let radius = (foe.duel.fighters[1].height * 2.0).max(8.0);
        let near = position.distance(Vec3::from_array(sim.duel.player.body.position)) <= 20.0;
        let draw = near
            || cameras.iter().next().is_none_or(|(frustum, _)| {
                frustum.intersects_sphere(
                    &Sphere {
                        center: (position + Vec3::Y * (radius * 0.5)).into(),
                        radius,
                    },
                    true,
                )
            });
        let full = foe.lod == sekiro_sim::duel::population::Lod::Full;
        let pose =
            draw && ((foe.active() && (full || actor.posed_clock != foe.clock)) || !actor.drawn);
        if pose {
            actor.posed_clock = foe.clock;
        }
        actor.drawn = draw;
        let lod = PresentationLod {
            draw,
            pose,
            cloth: draw && foe.active() && full,
        };
        presentation.set_if_neq(lod);
        // A foe that fell through a filter-15 event wall fades out and stays hidden (#234); the
        // renderer has no model opacity yet, so the fade itself is not drawn.
        visibility.set_if_neq(if draw && foe.duel.enemy_fall_opacity() > 0.0 {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
        let bound = rigs.get(entity).is_ok_and(|r| r.bound);
        let first = actor.meshes.is_empty() && bound;
        if first {
            actor.meshes = children
                .iter_descendants(entity)
                .filter(|e| meshes.contains(*e))
                .collect();
        }
        // GLB mesh/material handles come from one AssetServer scene per model, shared by
        // every placement. Only CPU cloth's mutable display buffers are per character.
        let shadows = cameras
            .iter()
            .next()
            .is_none_or(|(_, cam)| position.distance(cam.translation()) <= 30.0);
        for &mesh in &actor.meshes {
            let Ok((skin, dynamic, no_shadow)) = meshes.get(mesh) else {
                continue;
            };
            if first {
                commands.entity(mesh).remove::<NoFrustumCulling>();
            }
            let update_bounds = skin.is_some() && pose;
            if update_bounds && !dynamic {
                commands.entity(mesh).insert(DynamicSkinnedMeshBounds);
            }
            if !update_bounds && dynamic {
                commands.entity(mesh).remove::<DynamicSkinnedMeshBounds>();
            }
            if shadows && no_shadow {
                commands.entity(mesh).remove::<NotShadowCaster>();
            }
            if !shadows && !no_shadow {
                commands.entity(mesh).insert(NotShadowCaster);
            }
        }
        if !foe.active() {
            if dormant.is_none() {
                commands.entity(entity).insert(PoseDormant);
            }
            continue;
        }
        if dormant.is_some() {
            commands.entity(entity).remove::<PoseDormant>();
        }
        let r = &foe.report.enemy;
        let alpha = fixed.overstep_fraction();
        let (position, yaw) =
            super::shown_transform(foe.previous, &foe.duel.enemy, alpha, sim.step == 0);
        tf.translation = position;
        tf.rotation = Quat::from_rotation_y(yaw);
        anim.walk_twist = foe.duel.enemy.walk_twist;
        anim.clock = Some(foe.clock - super::behind(r, alpha));
        let Some(name) = &r.animation else {
            continue;
        };
        let lib = libraries.0.get_mut(&actor.donor).unwrap();
        if anim.clip_name() != name || (r.clip_started && anim.time > r.anim_time) {
            match lib.clip(name) {
                Ok(clip) => anim.play_named(name, clip, false, r.blend_in),
                Err(error) => {
                    warn!("map enemy {}: {error}", foe.part);
                    continue;
                }
            }
        }
        anim.time = super::drawn_clip_time(r, alpha);
    }
}

#[allow(clippy::type_complexity)]
fn gauges(
    sim: NonSend<Sim>,
    camera: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    mut panels: Query<(&Panel, &mut Node, &mut Visibility)>,
    mut fills: Query<
        (
            &crate::combat_hud::HudPart,
            &mut Node,
            Option<&mut BackgroundColor>,
            Option<&mut Visibility>,
        ),
        Without<Panel>,
    >,
    mut marks: Query<
        (&Mark, &mut Node, &mut Visibility),
        (Without<Panel>, Without<crate::combat_hud::HudPart>),
    >,
) {
    use crate::change::set;
    use crate::combat_hud::{Fill, HudPart, OVERHEAD_W, posture_color, viewport_size};
    let population = sim.duel.population.as_ref().unwrap();
    let Ok((camera, tf)) = camera.single() else {
        return;
    };
    let viewport = viewport_size(Some(camera));
    for (panel, mut node, mut vis) in &mut panels {
        let foe = &population.foes[panel.0];
        let d = &foe.duel;
        let engaged = foe.engaged()
            || (population.selected == Some(panel.0) && sim.duel.player.lock_target.is_some());
        let nearby = Vec3::from_array(d.enemy.body.position)
            .distance(Vec3::from_array(sim.duel.player.body.position))
            <= 30.0;
        let head =
            Vec3::from_array(d.enemy.body.position) + Vec3::Y * (d.fighters[1].height + 0.35);
        let projected = (foe.active() && engaged && nearby && !d.rules.fighters[1].dead())
            .then(|| camera.world_to_viewport(tf, head).ok())
            .flatten();
        vis.set_if_neq(if projected.is_some() {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
        if let Some(screen) = projected {
            set(
                &mut node,
                |n| &mut n.left,
                px(screen.x - viewport.x * OVERHEAD_W / 200.0),
            );
            set(&mut node, |n| &mut n.top, px(screen.y - 0.035 * viewport.y));
        }
    }
    for (part, mut node, color, visibility) in &mut fills {
        let (index, posture, arrow) = match *part {
            HudPart::Fill(Fill::Hp(i) | Fill::Trail(i)) if i >= 2 => (i - 2, false, false),
            HudPart::Fill(Fill::Posture(i)) if i >= 2 => (i - 2, true, false),
            HudPart::PostureArrow(i) if i >= 2 => (i - 2, true, true),
            _ => continue,
        };
        let f = &population.foes[index].duel.rules.fighters[1];
        let fraction = if posture {
            f.posture_fraction()
        } else {
            f.hp_fraction()
        };
        if arrow {
            if let Some(mut vis) = visibility {
                vis.set_if_neq(if fraction > 0.005 {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                });
            }
        } else {
            set(
                &mut node,
                |n| &mut n.width,
                percent(fraction.clamp(0.0, 1.0) * 100.0),
            );
            if posture && let Some(mut color) = color {
                color.set_if_neq(BackgroundColor(posture_color(fraction, f.posture.broken())));
            }
        }
    }
    for (mark, mut node, mut vis) in &mut marks {
        let foe = &population.foes[mark.0];
        let head =
            Vec3::from_array(foe.duel.enemy.body.position) + Vec3::Y * foe.duel.fighters[1].height;
        let nearby = Vec3::from_array(foe.duel.enemy.body.position)
            .distance(Vec3::from_array(sim.duel.player.body.position))
            <= 30.0;
        let projected = (foe.active() && foe.can_deathblow && nearby)
            .then(|| camera.world_to_viewport(tf, head).ok())
            .flatten();
        vis.set_if_neq(if projected.is_some() {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
        if let Some(screen) = projected {
            set(&mut node, |n| &mut n.left, px(screen.x - 8.0));
            set(&mut node, |n| &mut n.top, px(screen.y - 8.0));
        }
    }
}
