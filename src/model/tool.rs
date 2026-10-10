//! Prosthetic tool models: the equipped tool's two parts from the game's parts/wp_a_0<model>.partsbnd
//! (EquipParamWeapon equipModelId, e.g. the Loaded Umbrella 76000 -> 760 -> wp_a_0760), exported by
//! tools/sekiro-extract to extracted/tool/model_wp_a_0<model>.bin (Model0, WP_A_07xx.flver) and
//! model_wp_a_0<model>_1.bin (Model1, WP_A_07xx_1.flver).
//! - Place: Model0 rests on WepAbsorpPosParam[absorpParamId] left_0 (the tool's dummy on the arm:
//!   112 shuriken .. 124 whistle); Model1 has no rest place (left_1 0 in every tool row) and shows
//!   only while a TAE 715 OverrideWeaponModelLocation of WeaponModelType 1 (Left Weapon) places it
//!   (the umbrella's canopy: a076_405010 Model1 on 21 = the left hand, the expand a076_412000 on
//!   118). An event's Model0DummyPolyID moves Model0 the same way. A part's model space is the
//!   dummy's frame, as for the sword (model.rs override_weapon_location).
//! - Pose: WP_A_07xx_1.anibnd's clips are named after Wolf's anims plus "_1" (a076_405010_1 ...);
//!   exported keyed by Wolf's anim (extracted/tool/anim_wp_a_0<model>[_1].bin), so the part plays
//!   the clip of Wolf's current anim at Wolf's clock; with none, a999_000000, else the bind pose.
//! gap: wepInvisibleType, the parts' own TAE (WP_A_07xx[_1].tae), their dummies' effects, cloth.

use super::*;

/// The shown tool's parts on Wolf.
#[derive(Component, Default)]
pub struct ToolModel {
    /// EquipParamWeapon id of the tool shown (0: none yet).
    tool: i64,
    parts: Vec<ToolPart>,
}

struct ToolPart {
    /// Model number (0 / 1): which ModelNDummyPolyID places it.
    num: usize,
    pivot: Entity,
    /// Rest dummy (left_0), -1 = hidden unless an event places it.
    rest: i16,
    /// Joint entity per anim bone (by FLVER node name), and the bind pose of the rest.
    joints: Vec<Option<Entity>>,
    lib: crate::anim::Lib,
}

pub(super) fn plugin(app: &mut App) {
    app.add_systems(Update, (equip_tool_models, pose_tool_models).chain())
        .add_systems(PostUpdate, place_tool_models.after(bevy::transform::TransformSystems::Propagate));
}

fn tool_dir() -> std::path::PathBuf {
    crate::paths::root().join("extracted/tool")
}

#[allow(clippy::too_many_arguments)]
fn equip_tool_models(
    mut commands: Commands,
    combat: Res<crate::data::Combat>,
    config: Res<crate::config::GameConfig>,
    assets: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<SekiroMaterial>>,
    mut bindposes: ResMut<Assets<SkinnedMeshInverseBindposes>>,
    mut wolf: Query<(Entity, &crate::player::Player, Option<&mut ToolModel>), With<WeaponParts>>,
) {
    for (e, p, tm) in &mut wolf {
        let id = crate::player::equipped_tool(&combat, &config, p.tool_slot).map(|t| t.id).unwrap_or(0);
        let Some(mut tm) = tm else {
            commands.entity(e).insert(ToolModel::default());
            continue;
        };
        if tm.tool == id {
            continue;
        }
        for part in tm.parts.drain(..) {
            commands.entity(part.pivot).despawn();
        }
        tm.tool = id;
        let row = combat.param("EquipParamWeapon", id);
        let Some(model) = row["equipModelId"].as_i64().filter(|m| *m > 0) else { continue };
        let absorp = combat.param("WepAbsorpPosParam", row["absorpParamId"].as_i64().unwrap_or(-1));
        for (num, suffix) in [(0, ""), (1, "_1")] {
            let stem = format!("wp_a_{model:04}{suffix}");
            let Some(m) = load(&tool_dir().join(format!("model_{stem}.bin"))) else { continue };
            let rest = absorp[format!("left_{num}")].as_i64().filter(|d| *d > 0).unwrap_or(-1) as i16;
            let pivot = commands.spawn((Transform::default(), Visibility::Hidden, Name::new(format!("tool {stem}")))).id();
            let by_name = spawn_part(&mut commands, &assets, &mut meshes, &mut materials, &mut bindposes, &m, pivot);
            let lib = crate::anim::load_lib(&tool_dir().join(format!("anim_{stem}.bin")));
            let joints = lib.bones.iter().map(|b| by_name.get(&b.name).copied()).collect();
            tm.parts.push(ToolPart { num, pivot, rest, joints, lib });
        }
    }
}

/// The part's FLVER nodes as joint entities under `pivot` (bind pose) and its skinned meshes;
/// returns the joints by node name.
fn spawn_part(
    commands: &mut Commands,
    assets: &AssetServer,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<SekiroMaterial>,
    bindposes: &mut Assets<SkinnedMeshInverseBindposes>,
    model: &Model,
    pivot: Entity,
) -> HashMap<String, Entity> {
    let mut joints: Vec<Entity> = Vec::with_capacity(model.nodes.len());
    let mut world: Vec<Mat4> = Vec::with_capacity(model.nodes.len());
    for (i, node) in model.nodes.iter().enumerate() {
        let parent = (node.parent >= 0 && (node.parent as usize) < i).then_some(node.parent as usize);
        world.push(parent.map_or(Mat4::IDENTITY, |p| world[p]) * node.local.to_matrix());
        let e = commands.spawn((node.local, Visibility::default())).id();
        commands.entity(parent.map_or(pivot, |p| joints[p])).add_child(e);
        joints.push(e);
    }
    for md in model.meshes.iter().filter(|md| md.albedo != "-") {
        let mut remap: HashMap<u16, u16> = HashMap::new();
        let (mut mesh_joints, mut inv, mut idx) = (Vec::new(), Vec::new(), Vec::with_capacity(md.bones.len()));
        for (b, w) in md.bones.iter().zip(&md.weights) {
            let mut out = [0u16; 4];
            for k in 0..4 {
                if w[k] <= 0.0 && k > 0 {
                    continue;
                }
                let g = (b[k] as usize).min(model.nodes.len() - 1) as u16;
                let next = remap.len() as u16;
                out[k] = *remap.entry(g).or_insert_with(|| {
                    mesh_joints.push(joints[g as usize]);
                    inv.push(world[g as usize].inverse());
                    next
                });
            }
            idx.push(out);
        }
        if mesh_joints.len() > 256 {
            continue;
        }
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD);
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, md.pos.clone());
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, md.normal.clone());
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, md.uv.clone());
        mesh.insert_attribute(Mesh::ATTRIBUTE_JOINT_INDEX, VertexAttributeValues::Uint16x4(idx));
        mesh.insert_attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT, md.weights.clone());
        mesh.insert_indices(Indices::U32(md.indices.clone()));
        if !md.normal_map.is_empty() {
            let _ = mesh.generate_tangents();
        }
        let linear = |stem: &str| {
            (!stem.is_empty()).then(|| assets.load_builder().with_settings(|s: &mut bevy::image::ImageLoaderSettings| s.is_srgb = false).load(format!("tex/{stem}.dds")))
        };
        let (normal, metallic) = (linear(&md.normal_map), linear(&md.metallic));
        // As attach_models' character materials.
        let material = materials.add(SekiroMaterial {
            base: StandardMaterial {
                base_color_texture: (!md.albedo.is_empty()).then(|| assets.load(format!("tex/{}.dds", md.albedo))),
                perceptual_roughness: 0.8,
                alpha_mode: if md.alpha_test { AlphaMode::AlphaToCoverage } else { AlphaMode::Opaque },
                double_sided: true,
                cull_mode: None,
                ..default()
            },
            extension: SekiroExt { flags: UVec4::new(normal.is_some() as u32, metallic.is_some() as u32, 0, 0), normal, metallic },
        });
        let skin = SkinnedMesh { inverse_bindposes: bindposes.add(SkinnedMeshInverseBindposes::from(inv)), joints: mesh_joints };
        // The pivot is placed after transform propagation (place_tool_models), so the culling
        // pass can still see last frame's origin: never culled (a small mesh).
        let e = commands
            .spawn((Mesh3d(meshes.add(mesh)), MeshMaterial3d(material), Transform::default(), Visibility::default(), skin, bevy::camera::visibility::NoFrustumCulling))
            .id();
        commands.entity(pivot).add_child(e);
    }
    model.nodes.iter().zip(joints).map(|(n, e)| (n.name.clone(), e)).collect()
}

/// The part's clip for Wolf's current anim at Wolf's clock (else a999_000000 at 0), sampled
/// like pose_skeletons (linear between frames).
fn pose_tool_models(combat: Res<crate::data::Combat>, wolf: Query<(&Actor, &ToolModel)>, mut tfs: Query<&mut Transform>) {
    for (a, tm) in &wolf {
        let d = crate::actor::data_for(&combat, a.side);
        for part in &tm.parts {
            let (clip, t) = match part.lib.clips.get(&a.anim).or_else(|| part.lib.clips.get(d.clip_key(&a.anim))) {
                Some(c) => (c, a.t),
                None => match part.lib.clips.get("a999_000000") {
                    Some(c) => (c, 0.0),
                    None => continue,
                },
            };
            let tracks = clip.track_to_bone.len();
            if tracks == 0 || clip.frames == 0 {
                continue;
            }
            let f = (t / clip.frame_duration).clamp(0.0, (clip.frames - 1) as f32);
            let (i0, k) = (f.floor() as usize, f.fract());
            let i1 = (i0 + 1).min(clip.frames - 1);
            for (track, &bone) in clip.track_to_bone.iter().enumerate() {
                let Some(&Some(j)) = usize::try_from(bone).ok().and_then(|b| part.joints.get(b)) else { continue };
                let (p, q) = (&clip.data[i0 * tracks + track], &clip.data[i1 * tracks + track]);
                if let Ok(mut tf) = tfs.get_mut(j) {
                    *tf = Transform { translation: p.translation.lerp(q.translation, k), rotation: p.rotation.slerp(q.rotation, k), scale: p.scale.lerp(q.scale, k) };
                }
            }
        }
    }
}

/// Each part on its dummy: the running Left Weapon TAE 715's ModelNDummyPolyID, else its rest
/// dummy; hidden with neither.
fn place_tool_models(
    combat: Res<crate::data::Combat>,
    wolf: Query<(&Actor, &WeaponParts, &ToolModel)>,
    mut globals: Query<&mut GlobalTransform>,
    transforms: Query<&Transform>,
    children: Query<&Children>,
    mut vis: Query<&mut Visibility>,
) {
    for (a, wp, tm) in &wolf {
        let d = crate::actor::data_for(&combat, a.side);
        let ev = d
            .events_at(&a.anim, a.t)
            .find(|e| e.kind == 715 && e.args.get("WeaponModelType").and_then(|v| v.as_str()).is_some_and(|s| s.starts_with('1')));
        for part in &tm.parts {
            let id = ev
                .and_then(|e| e.arg_i64(&format!("Model{}DummyPolyID", part.num)).filter(|id| *id >= 0))
                .map(|id| id as i16)
                .or((part.rest >= 0).then_some(part.rest));
            let place = id.and_then(|id| wp.dummies.get(&id)).and_then(|&(joint, local)| globals.get(joint).ok().map(|g| g.to_matrix() * local));
            if let Ok(mut v) = vis.get_mut(part.pivot) {
                let want = if place.is_some() { Visibility::Inherited } else { Visibility::Hidden };
                if *v != want {
                    *v = want;
                }
            }
            if let Some(m) = place {
                set_global(part.pivot, GlobalTransform::from(bevy::math::Affine3A::from_mat4(m)), &mut globals, &transforms, &children);
            }
        }
    }
}
