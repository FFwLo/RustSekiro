//! Merge static object surfaces across placements, retaining the authored visibility roots.
use super::{LayeredMaterial, MaterialOrigin};
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;
use bevy::tasks::{
    AsyncComputeTaskPool, Task,
    futures_lite::future::{block_on, poll_once},
};
use std::collections::HashMap;
use std::sync::Arc;

struct Source {
    entity: Entity,
    mesh: Handle<Mesh>,
    root: Entity,
    root_pose: Mat4,
    pose: Mat4,
}

struct Group {
    sources: Vec<Source>,
    material: Handle<LayeredMaterial>,
    origin: Option<bevy::asset::AssetPath<'static>>,
}

type VisibilityKey = (String, [u32; 8], [u32; 8], i32, [i32; 8]);
type LayoutKey = (
    bevy::mesh::PrimitiveTopology,
    Vec<(
        bevy::mesh::MeshVertexAttributeId,
        bevy::render::render_resource::VertexFormat,
    )>,
);

#[derive(Hash, Eq, PartialEq)]
struct GroupKey {
    material: AssetId<LayeredMaterial>,
    visibility: VisibilityKey,
    layout: LayoutKey,
    no_shadow: bool,
    no_receiver: bool,
    authored_no_shadow: bool,
    region: [i32; 2],
    scale: u32,
}

struct Batch {
    entity: Entity,
    sources: Vec<Source>,
    merged: bool,
    key: GroupKey,
}
struct Job {
    key: GroupKey,
    group: Group,
    task: Task<Option<Mesh>>,
}

/// Installs per frame, when LOD levels and blocks still spawn and plan merges: each install also
/// uploads a merged mesh, so a time budget alone could upload dozens in one gameplay frame.
const PLAY_APPLY_CAP: usize = 4;

#[derive(Component)]
struct BatchMember(usize);

#[derive(Component)]
struct Planned;

#[derive(Resource, Default)]
pub(crate) struct StaticBatches {
    started: bool,
    jobs: Vec<Job>,
    /// Merge jobs planned so far, for the loading bar. Never reset: a map change relaunches the
    /// process, so each load starts from a fresh resource.
    planned: usize,
    batches: HashMap<usize, Batch>,
    next_batch: usize,
    dirty: Vec<usize>,
    source_to_batch: HashMap<Entity, usize>,
    /// Groups with a single placement so far, merged once a later part shares their key.
    singles: HashMap<GroupKey, Group>,
}

impl StaticBatches {
    pub(crate) fn ready(&self) -> bool {
        self.started && self.jobs.is_empty()
    }

}

pub(super) fn install(app: &mut App) {
    app.init_resource::<StaticBatches>().add_systems(
        PostUpdate,
        (changed_batches, update.run_if(has_work))
            .chain()
            .after(bevy::transform::TransformSystems::Propagate)
            .after(bevy::camera::visibility::VisibilitySystems::VisibilityPropagate)
            .before(bevy::camera::visibility::VisibilitySystems::CheckVisibility),
    );
}

#[allow(clippy::type_complexity)] // Independent source-state change filters.
fn changed_batches(
    changed: Query<
        &BatchMember,
        Or<(
            Changed<GlobalTransform>,
            Changed<InheritedVisibility>,
            Changed<MeshMaterial3d<LayeredMaterial>>,
            Added<NotShadowCaster>,
            Added<NotShadowReceiver>,
        )>,
    >,
    mut removed: RemovedComponents<BatchMember>,
    mut materials: RemovedComponents<MeshMaterial3d<LayeredMaterial>>,
    mut casters: RemovedComponents<NotShadowCaster>,
    mut receivers: RemovedComponents<NotShadowReceiver>,
    mut state: ResMut<StaticBatches>,
) {
    state.dirty.extend(changed.iter().map(|m| m.0));
    for entity in removed.read() {
        if let Some(&index) = state.source_to_batch.get(&entity) {
            state.dirty.push(index);
        }
    }
    for entity in materials
        .read()
        .chain(casters.read())
        .chain(receivers.read())
    {
        if let Some(&index) = state.source_to_batch.get(&entity) {
            state.dirty.push(index);
        }
    }
    state.dirty.sort_unstable();
    state.dirty.dedup();
}

#[allow(clippy::type_complexity)]
fn has_work(
    pending: Res<crate::map::Pending>,
    state: Res<StaticBatches>,
    unplanned: Query<
        (),
        (
            With<Mesh3d>,
            With<MeshMaterial3d<LayeredMaterial>>,
            Without<Planned>,
        ),
    >,
) -> bool {
    pending.loaded()
        && (!state.started
            || !state.jobs.is_empty()
            || !state.dirty.is_empty()
            || !unplanned.is_empty())
}

fn update(world: &mut World) {
    if !world.resource::<crate::map::Pending>().loaded() {
        return;
    }
    let _timer = crate::perf_log::SystemTimer::start("static map batches");
    let mut state = world.remove_resource::<StaticBatches>().unwrap();
    let unplanned = world
        .query_filtered::<Entity, (
            With<Mesh3d>,
            With<MeshMaterial3d<LayeredMaterial>>,
            Without<Planned>,
        )>()
        .iter(world)
        .next()
        .is_some();
    if !state.started || unplanned {
        state.started = true;
        let jobs = plan(world, &mut state);
        // Later plans run as LOD levels and blocks spawn; most add nothing.
        if !jobs.is_empty() {
            info!(
                "static map merge: {} groups, {} source meshes",
                jobs.len(),
                jobs.iter().map(|j| j.group.sources.len()).sum::<usize>()
            );
        }
        state.planned += jobs.len();
        state.jobs.extend(jobs);
    }
    // A few a frame.
    let mut completed = 0;
    let mut i = 0;
    while i < state.jobs.len() && completed < PLAY_APPLY_CAP {
        let Some(mesh) = block_on(poll_once(&mut state.jobs[i].task)) else {
            i += 1;
            continue;
        };
        let job = state.jobs.swap_remove(i);
        completed += 1;
        let Some(mut mesh) = mesh else {
            continue;
        };
        if job
            .group
            .sources
            .iter()
            .any(|s| world.get_entity(s.entity).is_err() || world.get_entity(s.root).is_err())
        {
            continue;
        }
        // Collision is loaded independently. Sources retain CPU data for guarded fallback,
        // but the generated batch only needs its GPU copy after bounds are calculated.
        mesh.asset_usage = bevy::asset::RenderAssetUsages::RENDER_WORLD;
        let mesh = world.resource_mut::<Assets<Mesh>>().add(mesh);
        let parent = job.group.sources[0].root;
        let mut entity = world.spawn((
            Mesh3d(mesh),
            MeshMaterial3d(job.group.material),
            Transform::IDENTITY,
            Visibility::Hidden,
            ChildOf(parent),
            Planned,
        ));
        if job.key.no_shadow {
            entity.insert(NotShadowCaster);
        }
        if job.key.authored_no_shadow {
            entity.insert(crate::map::world::AuthoredNoShadow);
        }
        if job.key.no_receiver {
            entity.insert(NotShadowReceiver);
        }
        if let Some(origin) = job.group.origin {
            entity.insert(MaterialOrigin(origin));
        }
        let entity = entity.id();
        let index = state.next_batch;
        state.next_batch += 1;
        for source in &job.group.sources {
            world.entity_mut(source.entity).insert(BatchMember(index));
            state.source_to_batch.insert(source.entity, index);
        }
        state.batches.insert(
            index,
            Batch {
                entity,
                sources: job.group.sources,
                merged: false,
                key: job.key,
            },
        );
        state.dirty.push(index);
    }
    for index in std::mem::take(&mut state.dirty) {
        let Some(batch) = state.batches.get_mut(&index) else {
            continue;
        };
        guard(world, batch);
        if !batch.merged && world.get::<Visibility>(batch.entity) != Some(&Visibility::Hidden) {
            state.dirty.push(index);
        }
        if world.get_entity(batch.entity).is_err()
            || batch
                .sources
                .iter()
                .any(|s| world.get_entity(s.entity).is_err() || world.get_entity(s.root).is_err())
        {
            let batch = state.batches.remove(&index).unwrap();
            world.despawn(batch.entity);
            for source in batch.sources {
                state.source_to_batch.remove(&source.entity);
                if let Ok(mut entity) = world.get_entity_mut(source.entity) {
                    entity.remove::<(BatchMember, Planned)>();
                }
            }
        }
    }
    world.insert_resource(state);
}

fn guard(world: &mut World, batch: &mut Batch) {
    let first = batch
        .sources
        .first()
        .and_then(|s| world.get::<InheritedVisibility>(s.entity))
        .map(|v| v.get());
    let coherent = world.get_entity(batch.entity).is_ok()
        && first.is_some()
        && batch.sources.iter().all(|s| {
            world
                .get::<InheritedVisibility>(s.entity)
                .is_some_and(|v| Some(v.get()) == first)
                && world
                    .get::<GlobalTransform>(s.root)
                    .is_some_and(|p| p.to_matrix() == s.root_pose)
                && world
                    .get::<GlobalTransform>(s.entity)
                    .is_some_and(|p| p.to_matrix() == s.pose)
                && world
                    .get::<MeshMaterial3d<LayeredMaterial>>(s.entity)
                    .is_some_and(|m| m.0.id() == batch.key.material)
                && world.get::<NotShadowCaster>(s.entity).is_some() == batch.key.no_shadow
                && world.get::<NotShadowReceiver>(s.entity).is_some() == batch.key.no_receiver
        });
    if coherent != batch.merged {
        for source in &batch.sources {
            if let Ok(mut entity) = world.get_entity_mut(source.entity) {
                if coherent {
                    entity.remove::<Mesh3d>();
                } else {
                    entity.insert(Mesh3d(source.mesh.clone()));
                }
            }
        }
        batch.merged = coherent;
    }
    let waiting = !coherent
        && world
            .get_resource::<crate::mesh_residency::OffGpuMeshes>()
            .is_some_and(|off_gpu| {
                let off_gpu = off_gpu.0.lock().unwrap();
                batch.sources.iter().any(|s| off_gpu.contains(&s.mesh.id()))
            });
    if waiting {
        return;
    }
    let shown = coherent && first == Some(true);
    if let Some(mut visibility) = world.get_mut::<Visibility>(batch.entity) {
        visibility.set_if_neq(if shown {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
    // This exclusive system runs after hierarchy propagation and before culling. Publish
    // the same-frame gate as well, so activation never removes sources a frame too early.
    if let Some(mut visibility) = world.get_mut::<InheritedVisibility>(batch.entity) {
        visibility.set_if_neq(if shown {
            InheritedVisibility::VISIBLE
        } else {
            InheritedVisibility::HIDDEN
        });
    }
}

fn plan(world: &mut World, state: &mut StaticBatches) -> Vec<Job> {
    let entities: Vec<_> = world
        .query_filtered::<Entity, (
            With<Mesh3d>,
            With<MeshMaterial3d<LayeredMaterial>>,
            Without<Planned>,
        )>()
        .iter(world)
        .collect();
    let mut groups: HashMap<GroupKey, Group> = HashMap::new();
    let mut root_keys = HashMap::new();
    for entity in entities {
        if world.get::<Mesh3d>(entity).is_none()
            || world
                .get::<MeshMaterial3d<LayeredMaterial>>(entity)
                .is_none()
        {
            continue;
        }
        world.entity_mut(entity).insert(Planned);
        if world.get::<Children>(entity).is_some_and(|c| !c.is_empty())
            || world.get::<bevy::mesh::MeshTag>(entity).is_some()
        {
            continue;
        }
        let mut root = entity;
        while world.get::<crate::map::world::BlockPart>(root).is_none() {
            let Some(parent) = world.get::<ChildOf>(root) else {
                break;
            };
            root = parent.parent();
        }
        // MapPiece levels keep independent distance crossfades. Grass is already merged per surface.
        if world.get::<crate::map::world::BlockPart>(root).is_none()
            || world.get::<crate::map::world::DetailPart>(root).is_some()
        {
            continue;
        }
        let Some(part) = world.get::<crate::map::visibility::VisibilityPart>(root) else {
            continue;
        };
        let key = root_keys
            .entry(root)
            .or_insert_with(|| {
                (
                    part.map.clone(),
                    part.effective.draw,
                    part.effective.backread,
                    part.entity_id,
                    part.entity_groups,
                )
            })
            .clone();
        let material = world
            .get::<MeshMaterial3d<LayeredMaterial>>(entity)
            .unwrap()
            .0
            .clone();
        if !world
            .resource::<Assets<LayeredMaterial>>()
            .get(&material)
            .is_some_and(|m| crate::map::opaque(m.base.alpha_mode))
        {
            continue;
        }
        let mesh = world.get::<Mesh3d>(entity).unwrap().0.clone();
        let Some(asset) = world.resource::<Assets<Mesh>>().get(&mesh) else {
            continue;
        };
        if asset.contains_attribute(Mesh::ATTRIBUTE_JOINT_INDEX) || asset.morph_targets().is_some()
        {
            continue;
        }
        let Some(pose) = world.get::<GlobalTransform>(entity).map(|p| p.to_matrix()) else {
            continue;
        };
        let root_pose = world.get::<GlobalTransform>(root).unwrap().to_matrix();
        let scale = pose.x_axis.truncate().length();
        if scale.to_bits() != root_pose.x_axis.truncate().length().to_bits() {
            continue;
        }
        let region = (pose.w_axis.truncate().xz() / 128.)
            .floor()
            .as_ivec2()
            .to_array();
        let layout = (
            asset.primitive_topology(),
            asset
                .attributes()
                .map(|(a, _)| (a.id, a.format))
                .collect::<Vec<_>>(),
        );
        let no_shadow = world.get::<NotShadowCaster>(entity).is_some();
        let no_receiver = world.get::<NotShadowReceiver>(entity).is_some();
        let authored_no_shadow = world
            .get::<crate::map::world::AuthoredNoShadow>(entity)
            .is_some();
        let origin = world.get::<MaterialOrigin>(entity).map(|o| o.0.clone());
        let group = groups
            .entry(GroupKey {
                material: material.id(),
                visibility: key,
                layout,
                no_shadow,
                no_receiver,
                authored_no_shadow,
                region,
                scale: scale.to_bits(),
            })
            .or_insert_with(|| Group {
                sources: Vec::new(),
                material,
                origin,
            });
        group.sources.push(Source {
            entity,
            mesh,
            root,
            root_pose,
            pose,
        });
    }
    for (key, group) in &mut groups {
        if let Some(single) = state.singles.remove(key) {
            group.sources.extend(
                single
                    .sources
                    .into_iter()
                    .filter(|s| world.get_entity(s.entity).is_ok()),
            );
        }
    }
    // Rebuild only groups receiving new placements, including groups still being merged.
    let mut index = 0;
    while index < state.jobs.len() {
        if let Some(group) = groups.get_mut(&state.jobs[index].key) {
            let job = state.jobs.swap_remove(index);
            group.sources.extend(job.group.sources);
        } else {
            index += 1;
        }
    }
    let affected: Vec<_> = state
        .batches
        .iter()
        .filter(|(_, batch)| groups.contains_key(&batch.key))
        .map(|(&index, _)| index)
        .collect();
    for index in affected {
        let mut batch = state.batches.remove(&index).unwrap();
        guard(world, &mut batch);
        if !batch.merged {
            state.batches.insert(index, batch);
            continue;
        }
        world.despawn(batch.entity);
        for source in &batch.sources {
            state.source_to_batch.remove(&source.entity);
            world
                .entity_mut(source.entity)
                .insert(Mesh3d(source.mesh.clone()))
                .remove::<BatchMember>();
        }
        groups
            .get_mut(&batch.key)
            .unwrap()
            .sources
            .extend(batch.sources);
    }
    let mut jobs = Vec::new();
    let mut meshes = HashMap::new();
    for (key, group) in groups {
        if group.sources.len() < 2 {
            state.singles.insert(key, group);
            continue;
        }
        let inverse = group.sources[0].root_pose.inverse();
        let mut inputs = Vec::new();
        for source in &group.sources {
            let relative = inverse * source.pose;
            let transform = Transform::from_matrix(relative);
            if !transform.to_matrix().abs_diff_eq(relative, 0.00001) {
                inputs.clear();
                break;
            }
            let Some(mesh) = world.resource::<Assets<Mesh>>().get(&source.mesh) else {
                inputs.clear();
                break;
            };
            let mesh = meshes
                .entry(source.mesh.id())
                .or_insert_with(|| Arc::new(mesh.clone()));
            inputs.push((mesh.clone(), transform));
        }
        if inputs.is_empty() {
            continue;
        }
        let task = AsyncComputeTaskPool::get().spawn(async move {
            let _timer = crate::perf_log::SystemTimer::start("static map merge worker");
            crate::map::merge_meshes(
                inputs
                    .iter()
                    .map(|(mesh, transform)| (mesh.as_ref(), *transform)),
            )
        });
        jobs.push(Job { key, group, task });
    }
    jobs
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn key(material: AssetId<LayeredMaterial>) -> GroupKey {
        GroupKey {
            material,
            visibility: (String::new(), [0; 8], [0; 8], 0, [0; 8]),
            layout: (bevy::mesh::PrimitiveTopology::TriangleList, Vec::new()),
            no_shadow: false,
            no_receiver: false,
            authored_no_shadow: false,
            region: [0; 2],
            scale: 1_f32.to_bits(),
        }
    }

    #[test]
    fn unmerging_waits_for_source_mesh_residency() {
        let mut world = World::new();
        let off_gpu = crate::mesh_residency::OffGpuMeshes::default();
        world.insert_resource(off_gpu.clone());
        let mesh = Handle::<Mesh>::default();
        off_gpu.0.lock().unwrap().insert(mesh.id());
        let source = world
            .spawn((
                GlobalTransform::IDENTITY,
                InheritedVisibility::VISIBLE,
                MeshMaterial3d::<LayeredMaterial>(Handle::default()),
            ))
            .id();
        let entity = world
            .spawn((Visibility::Inherited, InheritedVisibility::VISIBLE))
            .id();
        let mut batch = Batch {
            entity,
            sources: vec![Source {
                entity: source,
                mesh: mesh.clone(),
                root: source,
                root_pose: Mat4::IDENTITY,
                pose: Mat4::from_translation(Vec3::X),
            }],
            merged: true,
            key: key(Handle::<LayeredMaterial>::default().id()),
        };
        guard(&mut world, &mut batch);
        assert!(!batch.merged);
        assert!(world.get::<Mesh3d>(source).is_some());
        assert_eq!(
            world.get::<Visibility>(entity),
            Some(&Visibility::Inherited)
        );
        assert!(world.get::<InheritedVisibility>(entity).unwrap().get());
        off_gpu.0.lock().unwrap().clear();
        guard(&mut world, &mut batch);
        assert_eq!(world.get::<Visibility>(entity), Some(&Visibility::Hidden));
        assert!(!world.get::<InheritedVisibility>(entity).unwrap().get());
    }

    #[test]
    fn a_part_spawned_after_the_first_plan_is_merged() {
        AsyncComputeTaskPool::get_or_init(bevy::tasks::TaskPool::new);
        let mut world = World::new();
        world.init_resource::<StaticBatches>();
        world.insert_resource(crate::map::Pending {
            loaded_at: Some(std::time::Instant::now()),
            ..default()
        });
        let mut meshes = Assets::<Mesh>::default();
        let mesh = meshes.add(Mesh::from(Triangle3d::new(Vec3::ZERO, Vec3::X, Vec3::Y)));
        world.insert_resource(meshes);
        let mut materials = Assets::<LayeredMaterial>::default();
        let material = materials.add(LayeredMaterial::default());
        world.insert_resource(materials);
        let row = serde_json::json!({
            "kind": "Object", "name": "test", "entity_id": -1,
            "entity_group_ids": vec![0; 8], "display_data": {
                "collision_mask": vec![0; 32], "display_groups": vec![0; 8],
                "draw_groups": vec![0; 8], "condition1": 0, "condition2": 0
            }
        });
        let root = world
            .spawn((
                GlobalTransform::IDENTITY,
                crate::map::world::BlockPart {
                    block: "m11_00_00_00".into(),
                    part_index: 0,
                },
                crate::map::visibility::VisibilityPart::from_row("m11_00_00_00", &row)
                    .unwrap()
                    .unwrap(),
            ))
            .id();
        let spawn = |world: &mut World| {
            world
                .spawn((
                    Mesh3d(mesh.clone()),
                    MeshMaterial3d(material.clone()),
                    GlobalTransform::IDENTITY,
                    InheritedVisibility::VISIBLE,
                    ChildOf(root),
                ))
                .id()
        };
        let first = spawn(&mut world);
        let second = spawn(&mut world);
        update(&mut world);
        assert!(world.resource::<StaticBatches>().started);
        let late = spawn(&mut world);
        update(&mut world);
        let mut state = world.remove_resource::<StaticBatches>().unwrap();
        assert_eq!(state.jobs.len() + state.batches.len(), 1);
        if let Some(job) = state.jobs.pop() {
            assert_eq!(
                job.group
                    .sources
                    .iter()
                    .map(|s| s.entity)
                    .collect::<HashSet<_>>(),
                HashSet::from([first, second, late])
            );
            assert_eq!(block_on(job.task).unwrap().count_vertices(), 9);
        } else {
            let batch = state.batches.values().next().unwrap();
            assert!(batch.merged);
            assert_eq!(batch.sources.len(), 3);
            assert!(world.get::<BatchMember>(late).is_some());
        }
    }

    #[test]
    fn a_source_material_change_restores_the_source_mesh() {
        let mut app = App::new();
        let root = app.world_mut().spawn(GlobalTransform::IDENTITY).id();
        let mut materials = Assets::<LayeredMaterial>::default();
        let original = materials.add(LayeredMaterial::default());
        let replacement = materials.add(LayeredMaterial::default());
        let source = app
            .world_mut()
            .spawn((
                GlobalTransform::IDENTITY,
                InheritedVisibility::VISIBLE,
                Mesh3d(Handle::default()),
                MeshMaterial3d(original.clone()),
                BatchMember(0),
            ))
            .id();
        let entity = app.world_mut().spawn(Visibility::Hidden).id();
        let mut batch = Batch {
            entity,
            sources: vec![Source {
                entity: source,
                mesh: Handle::default(),
                root,
                root_pose: Mat4::IDENTITY,
                pose: Mat4::IDENTITY,
            }],
            merged: false,
            key: key(original.id()),
        };
        guard(app.world_mut(), &mut batch);
        assert!(batch.merged);
        let mut state = StaticBatches {
            started: true,
            ..default()
        };
        state.source_to_batch.insert(source, 0);
        state.batches.insert(0, batch);
        app.insert_resource(state)
            .insert_resource(crate::map::Pending {
                loaded_at: Some(std::time::Instant::now()),
                ..default()
            });
        app.add_systems(Update, changed_batches);
        app.update();
        app.world_mut()
            .resource_mut::<StaticBatches>()
            .dirty
            .clear();
        app.world_mut()
            .entity_mut(source)
            .insert(MeshMaterial3d(replacement));
        app.update();
        assert_eq!(app.world().resource::<StaticBatches>().dirty, [0]);
        update(app.world_mut());
        assert!(!app.world().resource::<StaticBatches>().batches[&0].merged);
        assert!(app.world().get::<Mesh3d>(source).is_some());
        assert_eq!(
            *app.world().get::<Visibility>(entity).unwrap(),
            Visibility::Hidden
        );
    }

    #[test]
    fn merged_parts_fall_back_when_an_event_hides_one_or_a_placement_moves() {
        let mut world = World::new();
        let root = world.spawn(GlobalTransform::IDENTITY).id();
        let sources: Vec<_> = (0..2)
            .map(|_| {
                let entity = world
                    .spawn((
                        GlobalTransform::IDENTITY,
                        InheritedVisibility::VISIBLE,
                        Mesh3d(Handle::default()),
                        MeshMaterial3d::<LayeredMaterial>(Handle::default()),
                    ))
                    .id();
                Source {
                    entity,
                    root,
                    mesh: Handle::default(),
                    root_pose: Mat4::IDENTITY,
                    pose: Mat4::IDENTITY,
                }
            })
            .collect();
        let entity = world.spawn(Visibility::Hidden).id();
        let mut batch = Batch {
            entity,
            sources,
            merged: false,
            key: key(Handle::<LayeredMaterial>::default().id()),
        };
        guard(&mut world, &mut batch);
        assert!(batch.merged);
        assert!(
            batch
                .sources
                .iter()
                .all(|s| world.get::<Mesh3d>(s.entity).is_none())
        );
        world
            .entity_mut(batch.sources[1].entity)
            .insert(InheritedVisibility::HIDDEN);
        guard(&mut world, &mut batch);
        assert!(!batch.merged);
        assert_eq!(
            *world.get::<Visibility>(entity).unwrap(),
            Visibility::Hidden
        );
        assert!(
            batch
                .sources
                .iter()
                .all(|s| world.get::<Mesh3d>(s.entity).is_some())
        );
        world
            .entity_mut(batch.sources[1].entity)
            .insert(InheritedVisibility::VISIBLE);
        guard(&mut world, &mut batch);
        assert!(batch.merged);
        world
            .entity_mut(root)
            .insert(GlobalTransform::from_translation(Vec3::X));
        guard(&mut world, &mut batch);
        assert!(!batch.merged);
    }
}
