//! Prosthetic tool models: the equipped tool's parts from the game's parts/wp_a_0<model>.partsbnd
//! (EquipParamWeapon equipModelId, e.g. the Loaded Umbrella 76000 -> 760 -> wp_a_0760), exported by
//! tools/tool_export.py to extracted/tool/model_wp_a_0<model>.bin (Model0, WP_A_07xx.flver) and
//! model_wp_a_0<model>_1.bin (Model1, WP_A_07xx_1.flver), and for the Sabimaru (750) and Divine
//! Abduction (770) model_wp_a_0<model>_2.bin (Model2; the fan's 715 a077_400000 puts it on 20, Sabimaru's
//! a075_400000 on 117); extracted/tool/tools.json holds each
//! tool row's equipModelId and its WepAbsorpPosParam left_N dummies.
//! - Place: Model0 rests on WepAbsorpPosParam[absorpParamId] left_0 (the tool's dummy on the arm:
//!   112 shuriken .. 124 whistle); Model1 has no rest place (left_1 0 in every tool row) and shows
//!   only while a TAE 715 OverrideWeaponModelLocation of WeaponModelType 1 (Left Weapon) places it
//!   (the umbrella's canopy: a076_405010 Model1 on 21 = the left hand, the expand a076_412000 on
//!   118). An event's Model0DummyPolyID moves Model0 the same way. A part's model space is the
//!   dummy's frame, as for the sword (model.rs override_weapon_location).
//! - Pose: WP_A_07xx_1.anibnd's clips are named after Wolf's anims plus "_1" (a076_405010_1 ...);
//!   exported keyed by Wolf's anim (extracted/tool/anim_wp_a_0<model>[_1].bin), so the part plays
//!   the clip of Wolf's current anim at Wolf's clock; with none, a999_000000, else the bind pose.
//! - Dummies: a part's dummy polys join Wolf's Dummies as 2<model><dummy> (the TAE's left weapon
//!   ids, as 1<model><dummy> for the right weapon: the axe's / spear's blade 21300 / 21301 = Model1
//!   300 / 301, Sabimaru's 21130 / 21301, the umbrella's 21140), so ffx.rs spawns the tool's
//!   effects on the tool itself.
//! The parts' own TAE (WP_A_07xx[_N].tae) hold no events (only empty anim entries).
//! Sekiro's EquipParamWeapon has no wepInvisibleType (only invisibleOnRemo, for cutscenes) and the
//! part bundles hold no cloth (flver, tpf, anibnd only).

use super::*;

/// The shown tool's parts on Wolf.
#[derive(Component, Default)]
pub struct ToolModel {
    /// EquipParamWeapon id of the tool shown (0: none yet).
    tool: i64,
    parts: Vec<ToolPart>,
    /// The grappling hook (EquipParamWeapon 9500000 "ワイヤー武器", equipModelId 9500:
    /// WP_A_9500, the kunai hook and its rope on an 80-bone chain, its clips named after Wolf's
    /// wire anims). Shown while a TAE 715 of WeaponModelType 3 "Grappling Hook" places it (dummy
    /// 144 in every wire anim). Loaded once.
    wire: Option<ToolPart>,
    /// The rope's length in the bind pose (root to the farthest node, ~4.0 m).
    wire_len: f32,
}

struct ToolPart {
    /// Model number (0 / 1 / 2): which ModelNDummyPolyID places it.
    num: usize,
    pivot: Entity,
    /// Rest dummy (left_0), -1 = hidden unless an event places it.
    rest: i16,
    /// Joint entity per anim bone (by FLVER node name), and the bind pose of the rest.
    joints: Vec<Option<Entity>>,
    lib: crate::anim::Lib,
    /// Its entries in Wolf's Dummies (2<model><dummy>).
    dummy_ids: Vec<i16>,
}

pub(super) fn plugin(app: &mut App) {
    app.add_systems(Update, (equip_tool_models, pose_tool_models.after(crate::wire::draw_rope)).chain())
        .add_systems(PostUpdate, place_tool_models.after(bevy::transform::TransformSystems::Propagate))
        // The sim pose too (as model.rs does for the sword): the tool hit test reads the
        // 21100-21130 dummies in FixedUpdate, which were a rendered frame old (two ticks a
        // frame at 30 fps tested the stale blade).
        .add_systems(FixedUpdate, (pose_tool_models, place_tool_models).chain().in_set(crate::anim::PoseSet).after(crate::anim::store_sim_poses));
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
    mut wolf: Query<(Entity, &crate::player::Player, Option<&mut ToolModel>, Option<&mut Dummies>), With<WeaponParts>>,
    mut rows: Local<Option<serde_json::Value>>,
    mut rope: Option<ResMut<crate::wire::WireRope>>,
) {
    let rows = rows.get_or_insert_with(|| {
        std::fs::read_to_string(tool_dir().join("tools.json")).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    });
    for (e, p, tm, mut dummies) in &mut wolf {
        let id = crate::player::equipped_tool(&combat, &config, p.tool_slot).map(|t| t.id).unwrap_or(0);
        let Some(mut tm) = tm else {
            let mut tm = ToolModel::default();
            let stem = "wp_a_9500";
            if let Some(m) = load(&tool_dir().join(format!("model_{stem}.bin"))) {
                let pivot = commands.spawn((Transform::default(), Visibility::Hidden, Name::new("grappling hook"))).id();
                let (by_name, _) = spawn_part(&mut commands, &assets, &mut meshes, &mut materials, &mut bindposes, &m, pivot);
                let lib = crate::anim::load_lib(&tool_dir().join(format!("anim_{stem}.bin")));
                let joints: Vec<Option<Entity>> = lib.bones.iter().map(|b| by_name.get(&b.name).copied()).collect();
                tm.wire = Some(ToolPart { num: 0, pivot, rest: -1, joints, lib, dummy_ids: Vec::new() });
                let mut world: Vec<Mat4> = Vec::new();
                for (i, n) in m.nodes.iter().enumerate() {
                    let parent = (n.parent >= 0 && (n.parent as usize) < i).then(|| world[n.parent as usize]);
                    world.push(parent.unwrap_or(Mat4::IDENTITY) * n.local.to_matrix());
                }
                tm.wire_len = world.iter().map(|w| w.w_axis.truncate().length()).fold(0.0, f32::max);
                if let Some(r) = rope.as_mut() {
                    r.model = true;
                }
            }
            commands.entity(e).insert(tm);
            continue;
        };
        if tm.tool == id {
            continue;
        }
        for part in tm.parts.drain(..) {
            if let Some(d) = dummies.as_mut() {
                for k in &part.dummy_ids {
                    d.0.remove(k);
                }
            }
            commands.entity(part.pivot).despawn();
        }
        tm.tool = id;
        let row = &rows[id.to_string()];
        let Some(model) = row["equipModelId"].as_i64().filter(|m| *m > 0) else { continue };
        for (num, suffix) in [(0, ""), (1, "_1"), (2, "_2"), (3, "_3")] {
            let stem = format!("wp_a_{model:04}{suffix}");
            let Some(m) = load(&tool_dir().join(format!("model_{stem}.bin"))) else { continue };
            let rest = row[format!("left_{num}")].as_i64().filter(|d| *d > 0).unwrap_or(-1) as i16;
            let pivot = commands.spawn((Transform::default(), Visibility::Hidden, Name::new(format!("tool {stem}")))).id();
            let (by_name, dummy_es) = spawn_part(&mut commands, &assets, &mut meshes, &mut materials, &mut bindposes, &m, pivot);
            let mut dummy_ids = Vec::new();
            for (dm, de) in dummy_es.into_iter().filter(|(dm, _)| (0..1000).contains(dm)) {
                let k = 20000 + num as i16 * 1000 + dm;
                if let Some(d) = dummies.as_mut() {
                    d.0.insert(k, de);
                    dummy_ids.push(k);
                }
            }
            let lib = crate::anim::load_lib(&tool_dir().join(format!("anim_{stem}.bin")));
            let joints: Vec<Option<Entity>> = lib.bones.iter().map(|b| by_name.get(&b.name).copied()).collect();
            if std::env::var("SHINOBI_TOOL_LOG").is_ok() {
                info!(
                    "tool {id} {stem}: rest {rest}, {} meshes, {} clips, {}/{} bones matched, dummies {dummy_ids:?}",
                    m.meshes.len(),
                    lib.clips.len(),
                    joints.iter().flatten().count(),
                    joints.len()
                );
            }
            tm.parts.push(ToolPart { num, pivot, rest, joints, lib, dummy_ids });
        }
    }
}

/// The part's FLVER nodes as joint entities under `pivot` (bind pose), its skinned meshes and its
/// dummy polys (on their attach joints, as model.rs does for Wolf's); returns the joints by node
/// name and the dummies by id.
fn spawn_part(
    commands: &mut Commands,
    assets: &AssetServer,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<SekiroMaterial>,
    bindposes: &mut Assets<SkinnedMeshInverseBindposes>,
    model: &Model,
    pivot: Entity,
) -> (HashMap<String, Entity>, Vec<(i16, Entity)>) {
    let mut joints: Vec<Entity> = Vec::with_capacity(model.nodes.len());
    let mut world: Vec<Mat4> = Vec::with_capacity(model.nodes.len());
    for (i, node) in model.nodes.iter().enumerate() {
        let parent = (node.parent >= 0 && (node.parent as usize) < i).then_some(node.parent as usize);
        world.push(parent.map_or(Mat4::IDENTITY, |p| world[p]) * node.local.to_matrix());
        let e = commands.spawn((node.local, Visibility::default())).id();
        commands.entity(parent.map_or(pivot, |p| joints[p])).add_child(e);
        joints.push(e);
    }
    for md in model.meshes.iter().filter(|md| md.albedo != "-" && !md.pos.is_empty()) {
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
        if std::env::var("SHINOBI_TOOL_LOG").is_ok() {
            let (mn, mx) = md.pos.iter().fold((Vec3::MAX, Vec3::MIN), |(a, b), p| (a.min(Vec3::from(*p)), b.max(Vec3::from(*p))));
            info!("tool mesh {} albedo {} verts {} joints {} box {mn:.2} {mx:.2}", md.material, md.albedo, md.pos.len(), mesh_joints.len());
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
            extension: SekiroExt { flags: UVec4::new(normal.is_some() as u32, metallic.is_some() as u32, 0, 0), normal, metallic, ..Default::default() },
        });
        let skin = SkinnedMesh { inverse_bindposes: bindposes.add(SkinnedMeshInverseBindposes::from(inv)), joints: mesh_joints };
        // The pivot is placed after transform propagation (place_tool_models), so the culling
        // pass can still see last frame's origin: never culled (a small mesh).
        let e = commands
            .spawn((Mesh3d(meshes.add(mesh)), MeshMaterial3d(material), Transform::default(), Visibility::default(), skin, bevy::camera::visibility::NoFrustumCulling))
            .id();
        commands.entity(pivot).add_child(e);
    }
    let mut dummies = Vec::with_capacity(model.dummies.len());
    for dm in &model.dummies {
        let (joint, bind) = match usize::try_from(dm.attach).ok().filter(|&i| i < joints.len()) {
            Some(i) => (joints[i], world[i]),
            None => (pivot, Mat4::IDENTITY),
        };
        let local = bind.inverse().transform_point3(dm.pos);
        // Local +Z = the dummy's forward.
        let rot = dm.fwd.map_or(Quat::IDENTITY, |f| Quat::from_rotation_arc(Vec3::Z, bind.inverse().transform_vector3(f).normalize()));
        let e = commands.spawn((DummyPoly { id: dm.id }, Transform::from_translation(local).with_rotation(rot), Visibility::default())).id();
        commands.entity(joint).add_child(e);
        dummies.push((dm.id, e));
    }
    (model.nodes.iter().zip(joints).map(|(n, e)| (n.name.clone(), e)).collect(), dummies)
}

/// The part's clip for Wolf's current anim at Wolf's clock (else a999_000000 at 0), sampled
/// like pose_skeletons (linear between frames).
/// The wire's clips keep the rope at its modelled length (only its shape moves); here its chain
/// is lengthened to reach the hook (each bone's offset from its parent times the hand-to-tip
/// distance over the bind length), the kunai on the last bones keeping its shape. gap: the
/// exe's own rope placement (TAE 717 WireModelEventUnk / 900 / 901) is not traced.
fn pose_tool_models(combat: Res<crate::data::Combat>, rope: Option<Res<crate::wire::WireRope>>, wolf: Query<(&Actor, &ToolModel)>, mut tfs: Query<&mut Transform>) {
    let reach = rope.and_then(|r| r.ends).map(|(hand, tip)| hand.distance(tip));
    for (a, tm) in &wolf {
        let d = crate::actor::data_for(&combat, a);
        let stretch = reach.filter(|_| tm.wire_len > 0.0).map(|r| (r / tm.wire_len).max(0.02));
        for (part, wire) in tm.parts.iter().map(|p| (p, false)).chain(tm.wire.iter().map(|p| (p, true))) {
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
                    if let (true, Some(s), true) = (wire, stretch, bone > 0) {
                        tf.translation *= s;
                    }
                }
            }
        }
    }
}

/// Each part on its dummy: the running Left Weapon TAE 715's ModelNDummyPolyID, else its rest
/// dummy; hidden with neither.
fn place_tool_models(
    combat: Res<crate::data::Combat>,
    rope: Option<Res<crate::wire::WireRope>>,
    wolf: Query<(&Actor, &WeaponParts, &ToolModel)>,
    mut globals: Query<&mut GlobalTransform>,
    transforms: Query<&Transform>,
    children: Query<&Children>,
    mut vis: Query<&mut Visibility>,
) {
    for (a, wp, tm) in &wolf {
        let d = crate::actor::data_for(&combat, a);
        let event = |ty: char| {
            d.events_at(&a.anim, a.t).find(|e| e.kind == 715 && e.args.get("WeaponModelType").and_then(|v| v.as_str()).is_some_and(|s| s.starts_with(ty)))
        };
        let (ev, wire_ev) = (event('1'), event('3'));
        for (part, ev) in tm.parts.iter().map(|p| (p, ev)).chain(tm.wire.iter().map(|p| (p, wire_ev))) {
            let id = ev
                .and_then(|e| e.arg_i64(&format!("Model{}DummyPolyID", part.num)).filter(|id| *id >= 0))
                .map(|id| id as i16)
                .or((part.rest >= 0).then_some(part.rest));
            let mut place = id.and_then(|id| wp.dummies.get(&id)).and_then(|&(joint, local)| globals.get(joint).ok().map(|g| g.to_matrix() * local));
            // The wire: its rope (the model's -Y) aimed from the hand at the hook's tip.
            if let (true, Some(m), Some((_, tip))) = (part.pivot == tm.wire.as_ref().map_or(Entity::PLACEHOLDER, |w| w.pivot), place, rope.as_ref().and_then(|r| r.ends)) {
                let at = m.w_axis.truncate();
                let to = (tip - at).normalize_or_zero();
                if to != Vec3::ZERO {
                    place = Some(Mat4::from_rotation_translation(Quat::from_rotation_arc(Vec3::NEG_Y, to), at));
                }
            }
            if let Ok(mut v) = vis.get_mut(part.pivot) {
                let want = if place.is_some() { Visibility::Inherited } else { Visibility::Hidden };
                if *v != want {
                    *v = want;
                }
            }
            if std::env::var("SHINOBI_TOOL_LOG").is_ok() {
                info!("tool part {} on {:?} at {:?}", part.num, id, place.map(|m| m.w_axis.truncate()));
            }
            if let Some(m) = place {
                set_global(part.pivot, GlobalTransform::from(bevy::math::Affine3A::from_mat4(m)), &mut globals, &transforms, &children);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The tool parts are posed and placed in the sim's own pose pass (FixedUpdate), not only
    /// for drawing (round 17).
    #[test]
    fn tool_parts_are_placed_in_the_sim_pose() {
        let mut app = App::new();
        plugin(&mut app);
        let world = app.world_mut();
        // System names need Bevy's debug feature: the plugin's FixedUpdate holds the pose and the
        // place pass (it had none).
        // Only FixedUpdate is taken out: initialising a system param may insert a resource, which
        // a scope over the whole Schedules resource forbids (it failed on some target dirs).
        let count = world
            .try_schedule_scope(FixedUpdate, |world, s| {
                s.initialize(world).unwrap();
                s.systems().unwrap().count()
            })
            .unwrap_or(0);
        assert_eq!(count, 2, "the tool parts are only posed and placed for drawing");
    }

    /// The wire model's reach per clip: the farthest node from the root over the clip.
    #[test]
    #[ignore]
    fn wire_clip_reach_probe() {
        let m = load(&tool_dir().join("model_wp_a_9500.bin")).expect("model");
        let lib = crate::anim::load_lib(&tool_dir().join("anim_wp_a_9500.bin"));
        let mut keys: Vec<_> = lib.clips.keys().cloned().collect();
        keys.sort();
        for k in keys {
            let clip = &lib.clips[&k];
            let tracks = clip.track_to_bone.len();
            let mut out = Vec::new();
            for f in (0..clip.frames).step_by(5) {
                let mut local: Vec<Transform> = m.nodes.iter().map(|n| n.local).collect();
                for (track, &bone) in clip.track_to_bone.iter().enumerate() {
                    let Some(b) = usize::try_from(bone).ok().and_then(|b| lib.bones.get(b)) else { continue };
                    if let Some(i) = m.nodes.iter().position(|n| n.name == b.name) {
                        let p = &clip.data[f * tracks + track];
                        local[i] = Transform { translation: p.translation, rotation: p.rotation, scale: p.scale };
                    }
                }
                let mut world: Vec<Mat4> = Vec::new();
                for (i, n) in m.nodes.iter().enumerate() {
                    let parent = (n.parent >= 0 && (n.parent as usize) < i).then(|| world[n.parent as usize]);
                    world.push(parent.unwrap_or(Mat4::IDENTITY) * local[i].to_matrix());
                }
                let far = world.iter().map(|w| w.w_axis.truncate().length()).fold(0.0f32, f32::max);
                out.push(format!("{f}:{far:.1}"));
            }
            let matched = clip.track_to_bone.iter().filter(|&&b| usize::try_from(b).ok().and_then(|b| lib.bones.get(b)).is_some_and(|b| m.nodes.iter().any(|n| n.name == b.name))).count();
            let moving: Vec<String> = clip.track_to_bone.iter().enumerate().filter(|(t, _)| (clip.data[*t].translation - clip.data[(clip.frames - 1) * tracks + t].translation).length() > 0.01).map(|(_, &b)| lib.bones.get(b as usize).map_or("?".into(), |b| b.name.clone())).take(8).collect();
            println!("{k} frames {} tracks {tracks} matched {matched} moving {moving:?}: {}", clip.frames, out.join(" "));
        }
    }
}
