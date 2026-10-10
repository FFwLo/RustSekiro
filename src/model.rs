//! Character meshes: FLVER models exported by tools/sekiro-extract
//! (extracted/model_<chr>.bin + extracted/tex/*.dds), skinned onto the
//! animated skeleton from anim.rs by bone name.
//!
//! Space: FLVER/Havok -> game mirrors X (positions, normals, bind poses), which
//! also flips triangle winding. Each mesh gets its own compact joint list
//! because a FLVER has more nodes than Bevy's 256-joint limit.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::skinning::{SkinnedMesh, SkinnedMeshInverseBindposes};
use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;
use std::collections::HashMap;

use crate::actor::{Actor, Side};
use crate::anim::{AnimLib, HasModel, Skeleton};

mod tool;

struct Node {
    name: String,
    parent: i16,
    local: Transform,
}

struct MeshData {
    /// FLVER material name; "#NN#..." marks display-mask group NN.
    material: String,
    albedo: String,
    /// Normal-map texture stem (empty when the pack has no `_n` for this albedo).
    normal_map: String,
    /// Metallic mask stem (MTD MetallicMap; empty when the material has none).
    metallic: String,
    /// The albedo alpha is a cutout (MTD "_e" / fur / decal); else opaque (DetailBlend / Blend
    /// shaders keep a blend mask in the alpha).
    alpha_test: bool,
    pos: Vec<[f32; 3]>,
    normal: Vec<[f32; 3]>,
    uv: Vec<[f32; 2]>,
    bones: Vec<[u16; 4]>,
    weights: Vec<[f32; 4]>,
    indices: Vec<u32>,
}

struct Dummy {
    id: i16,
    attach: i16,
    /// Model-space bind position (game space).
    pos: Vec3,
    /// Model-space forward (game space), from the model_<chr>.dummies.json sidecar.
    fwd: Option<Vec3>,
    up: Option<Vec3>,
}

struct Model {
    nodes: Vec<Node>,
    meshes: Vec<MeshData>,
    dummies: Vec<Dummy>,
}

/// An effect model (FXR Model appearance 605: the leaves s04010, the shuriken s08050 ...): its
/// triangles (game space mirrored like the characters) and albedo, all meshes merged.
pub(crate) struct FxModel {
    pub albedo: String,
    pub pos: Vec<Vec3>,
    pub uv: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
}

/// Loads extracted/fxr_model/model_<id>.bin (sekiro-extract model s<id:05>.flver <its tpf>).
pub(crate) fn load_fx_model(id: i64) -> Option<FxModel> {
    let m = load(&crate::paths::extracted().join(format!("fxr_model/model_{id}.bin")))?;
    let mut out = FxModel { albedo: m.meshes.first()?.albedo.clone(), pos: Vec::new(), uv: Vec::new(), indices: Vec::new() };
    for md in &m.meshes {
        let base = out.pos.len() as u32;
        out.pos.extend(md.pos.iter().map(|p| Vec3::from_array(*p)));
        out.uv.extend_from_slice(&md.uv);
        out.indices.extend(md.indices.iter().map(|i| base + i));
    }
    Some(out)
}

/// A dummy poly (hitbox / effect anchor) following its attach bone.
#[derive(Component)]
pub struct DummyPoly {
    #[allow(dead_code)] // kept for debugging / future use
    pub id: i16,
}

/// Dummy poly entities of a character by id (weapon dummies win over body ones).
#[derive(Component, Default)]
pub struct Dummies(pub HashMap<i16, Entity>);

/// Wolf's sword and scabbard pivots, for TAE 715 OverrideWeaponModelLocation: Model0 (the blade)
/// / Model1 (the scabbard) move onto the given body dummy while the event runs (combat arts: the
/// iai arts sheathe the blade at 147, Ichimonji holds the scabbard at 76 / 77).
#[derive(Component, Default)]
pub struct WeaponParts {
    blade: Option<Entity>,
    sheath: Option<Entity>,
    /// Right weapon Model2 (WP_A_0300_2): no meshes, only the Mortal Draw effect dummies (TAE
    /// 12200.. = this model's 200..; 240-280 reach out to 6 m). Placed by Model2DummyPolyID.
    model2: Option<Entity>,
    /// WeaponModelType 2 "Mortal Blade": WP_A_0310 (Model0) and its scabbard WP_A_0310_1 (Model1).
    /// Shown only while a TAE 715 of that type places them (Mortal Draw).
    mortal: [Option<Entity>; 2],
    /// Body dummy -> (attach joint, frame in that joint's space).
    dummies: HashMap<i16, (Entity, Mat4)>,
    /// Sheathed (weapon style None): WepAbsorpPosParam rightHang_0 / rightHang_1 (blade into the
    /// scabbard, both on 147).
    hang: (i16, i16),
    /// rightHang_2 (149): Model2 while sheathed.
    hang2: i16,
}

/// Wolf's sword (EquipParamWeapon 5000): its absorpParamId row of WepAbsorpPosParam says which body
/// dummy each part sits on - right_0 the blade (dummy 20 in the right hand), right_1 the scabbard
/// (147 on the Sheath bone). A part's model space is the dummy's frame.
const SWORD_WEAPON: i64 = 5000;

/// Model-space frame of a dummy: +Z its forward, +Y its upward (X mirrored like positions).
fn dummy_frame(dm: &Dummy) -> Option<Mat4> {
    let (f, u) = (dm.fwd?, dm.up?);
    let z = f.normalize();
    let y = (u - z * u.dot(z)).normalize();
    let rot = Quat::from_mat3(&Mat3::from_cols(y.cross(z), y, z));
    Some(Mat4::from_rotation_translation(rot, dm.pos))
}

fn load(path: &std::path::Path) -> Option<Model> {
    let d = std::fs::read(path).ok()?;
    if &d[0..4] != b"SHMD" {
        return None;
    }
    let version = u32::from_le_bytes(d[4..8].try_into().unwrap());
    let mut o = 8usize;
    let u16_ = |o: usize| u16::from_le_bytes([d[o], d[o + 1]]);
    let u32_ = |o: usize| u32::from_le_bytes(d[o..o + 4].try_into().unwrap());
    let f = |o: usize| f32::from_le_bytes(d[o..o + 4].try_into().unwrap());
    let string = |o: &mut usize| {
        let l = u16_(*o) as usize;
        let s = String::from_utf8_lossy(&d[*o + 2..*o + 2 + l]).into_owned();
        *o += 2 + l;
        s
    };
    let n = u32_(o) as usize;
    o += 4;
    let mut nodes = Vec::with_capacity(n);
    for _ in 0..n {
        let name = string(&mut o);
        let parent = u16_(o) as i16;
        o += 2;
        let v: Vec<f32> = (0..10).map(|k| f(o + k * 4)).collect();
        o += 40;
        let local = Transform {
            translation: Vec3::new(-v[0], v[1], v[2]),
            rotation: Quat::from_xyzw(v[3], -v[4], -v[5], v[6]).normalize(),
            scale: Vec3::new(v[7], v[8], v[9]),
        };
        nodes.push(Node { name, parent, local });
    }
    let m = u32_(o) as usize;
    o += 4;
    let mut meshes = Vec::with_capacity(m);
    for _ in 0..m {
        let material = string(&mut o);
        let albedo = string(&mut o);
        let normal_map = if version >= 3 { string(&mut o) } else { String::new() };
        let metallic = if version >= 4 { string(&mut o) } else { String::new() };
        let alpha_test = if version >= 5 {
            o += 1;
            d[o - 1] != 0
        } else {
            true
        };
        let vc = u32_(o) as usize;
        o += 4;
        let mut md = MeshData { material, albedo, normal_map, metallic, alpha_test, pos: Vec::new(), normal: Vec::new(), uv: Vec::new(), bones: Vec::new(), weights: Vec::new(), indices: Vec::new() };
        for _ in 0..vc {
            md.pos.push([-f(o), f(o + 4), f(o + 8)]);
            md.normal.push([-f(o + 12), f(o + 16), f(o + 20)]);
            md.uv.push([f(o + 24), f(o + 28)]);
            md.bones.push([u16_(o + 32), u16_(o + 34), u16_(o + 36), u16_(o + 38)]);
            md.weights.push([f(o + 40), f(o + 44), f(o + 48), f(o + 52)]);
            o += 56;
        }
        let ic = u32_(o) as usize;
        o += 4;
        // Mirroring flips winding: swap the last two indices of each triangle.
        for t in 0..ic / 3 {
            let i = o + t * 12;
            md.indices.extend([u32_(i), u32_(i + 8), u32_(i + 4)]);
        }
        o += ic * 4;
        meshes.push(md);
    }
    let mut dummies = Vec::new();
    if version >= 2 && o + 4 <= d.len() {
        let n = u32_(o) as usize;
        o += 4;
        for _ in 0..n {
            let id = u16_(o) as i16;
            let attach = u16_(o + 4) as i16;
            dummies.push(Dummy { id, attach, pos: Vec3::new(-f(o + 6), f(o + 10), f(o + 14)), fwd: None, up: None });
            o += 18;
        }
    }
    // Dummy directions (sekiro-extract dummies <flver> model_<chr>.dummies.json; X mirrored like
    // positions). Throws face the absorbed character along them (player.rs follow_throw).
    if let Some(v) = std::fs::read_to_string(path.with_extension("dummies.json")).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()) {
        let vec = |d: &serde_json::Value, k: &str| -> Option<Vec3> {
            let f = d[k].as_array()?;
            let g = |i: usize| f.get(i).and_then(|x| x.as_f64()).unwrap_or(0.0) as f32;
            Some(Vec3::new(-g(0), g(1), g(2))).filter(|v| v.length_squared() > 1e-12)
        };
        let by_id: HashMap<i64, (Option<Vec3>, Option<Vec3>)> =
            v.as_array().into_iter().flatten().filter_map(|d| Some((d["id"].as_i64()?, (vec(d, "fwd"), vec(d, "up"))))).collect();
        for dm in &mut dummies {
            if let Some(&(f, u)) = by_id.get(&(dm.id as i64)) {
                (dm.fwd, dm.up) = (f, u);
            }
        }
    }
    Some(Model { nodes, meshes, dummies })
}

/// A real weapon mesh is attached (placeholder blades get hidden).
#[derive(Component)]
pub struct HasWeapon;

/// An NPC mesh in draw-mask group #NN#: its spawn visibility (NpcParam mask, weapon rule) and
/// the actor whose TAE draw-mask events toggle it.
#[derive(Component)]
pub struct MeshGroup {
    owner: Entity,
    group: u32,
    base: bool,
}

/// Groups switched by TAE 233 ChangeChrDrawMask (FUN_140b54330 -> FDPChrPrimDispMask): kept
/// until changed again. TAE 711 HideModelMask / 713 ShowModelMask override only while they run.
#[derive(Component, Default)]
pub struct DrawMask(HashMap<u32, bool>);

/// Sekiro "Character_AMSN" materials: StandardMaterial (albedo, alpha mask) + the normal map with
/// shininess in its blue channel and the metallic mask (sekiro_material.wgsl).
pub type SekiroMaterial = bevy::pbr::ExtendedMaterial<StandardMaterial, SekiroExt>;

#[derive(Asset, bevy::render::render_resource::AsBindGroup, Reflect, Debug, Clone, Default)]
pub struct SekiroExt {
    #[texture(100)]
    #[sampler(101)]
    pub normal: Option<Handle<Image>>,
    #[texture(102)]
    #[sampler(103)]
    pub metallic: Option<Handle<Image>>,
    /// x: has a normal map, y: has a metallic mask.
    #[uniform(104)]
    pub flags: UVec4,
}

impl bevy::pbr::MaterialExtension for SekiroExt {
    fn fragment_shader() -> bevy::shader::ShaderRef {
        "embedded://sv1/sekiro_material.wgsl".into()
    }
}

pub struct ModelPlugin;

impl Plugin for ModelPlugin {
    fn build(&self, app: &mut App) {
        bevy::asset::embedded_asset!(app, "sekiro_material.wgsl");
        app.add_plugins(MaterialPlugin::<SekiroMaterial>::default());
        app.add_systems(PostStartup, attach_models.after(crate::anim::AnimSet))
            .add_systems(Update, (hide_placeholder_blades, update_draw_masks))
            .add_systems(PostUpdate, override_weapon_location.after(bevy::transform::TransformSystems::Propagate));
        tool::plugin(app);
    }
}

#[allow(clippy::too_many_arguments)]
fn attach_models(
    combat: Res<crate::data::Combat>,
    mut commands: Commands,
    lib: Res<AnimLib>,
    assets: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<SekiroMaterial>>,
    mut bindposes: ResMut<Assets<SkinnedMeshInverseBindposes>>,
    actors: Query<(Entity, &Actor, &Skeleton)>,
    roots: Query<(Entity, &ChildOf, &Name)>,
) {
    let dir = crate::paths::root().join("extracted");
    for (actor_e, a, skel) in &actors {
        let chr = match a.side {
            Side::Player => "c0000",
            Side::Enemy => combat.foe.chr.as_str(),
        };
        // Every model_<chr>*.bin: the whole character (NPCs) or equipment parts (player).
        let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(&dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                let n = p.file_name().unwrap_or_default().to_string_lossy().to_string();
                n.starts_with(&format!("model_{chr}")) && n.ends_with(".bin")
            })
            .collect();
        files.sort();
        let skel_lib = lib.lib(a.side);
        let by_name: HashMap<&str, usize> = skel_lib.bones.iter().enumerate().map(|(i, b)| (b.name.as_str(), i)).collect();
        let Some(skel_root) = roots.iter().find(|(_, p, n)| p.parent() == actor_e && n.as_str() == "SkeletonRoot").map(|(e, _, _)| e) else {
            continue;
        };
        let mut attached = 0;
        let mut dummies = Dummies::default();
        let mut weapon_parts = WeaponParts::default();
        let mut cloths = crate::cloth::Cloths::default();
        for file in files {
        let Some(model) = load(&file) else { continue };
        // Havok cloth (model_<x>.cloth.json): its display meshes are drawn from CPU vertices.
        let mut cloth_defs = crate::cloth::load(&file);
        // Weapon parts (wp_*) hang off the R_Weapon bone; everything else off the skeleton root.
        let is_weapon = file.file_name().unwrap_or_default().to_string_lossy().contains("_wp_");
        // Scabbards (the weapon part's WP_A_xxxx_1.flver, exported as *_sheath_*) hang off the
        // Sheath bone at the left hip.
        let is_sheath = file.file_name().unwrap_or_default().to_string_lossy().contains("_sheath_");
        let fname = file.file_name().unwrap_or_default().to_string_lossy().to_string();
        // Right weapon Model2 (the effect dummies) and the Mortal Blade's two models.
        let part2 = fname.contains("_mortal_");
        let mortal = if fname.contains("_mbsheath_") { Some(1) } else if fname.contains("_mblade_") { Some(0) } else { None };
        let root = if part2 || mortal.is_some() {
            let absorp = combat.param("WepAbsorpPosParam", combat.param("EquipParamWeapon", SWORD_WEAPON)["absorpParamId"].as_i64().unwrap_or(-1));
            // Model2 rests on right_2 (149); the Mortal Blade has no rest place (hidden until TAE 715).
            let id = if part2 { absorp["right_2"].as_i64().unwrap_or(-1) } else if mortal == Some(0) { 20 } else { 149 } as i16;
            weapon_parts.hang2 = absorp["rightHang_2"].as_i64().unwrap_or(-1) as i16;
            let Some(&(joint, local)) = weapon_parts.dummies.get(&id) else {
                warn!("{chr}: no body dummy {id} for {}", file.display());
                continue;
            };
            let pivot = commands.spawn((Transform::from_matrix(local), if mortal.is_some() { Visibility::Hidden } else { Visibility::default() })).id();
            commands.entity(joint).add_child(pivot);
            match mortal {
                Some(i) => weapon_parts.mortal[i] = Some(pivot),
                None => weapon_parts.model2 = Some(pivot),
            }
            pivot
        } else if is_weapon || is_sheath {
            // The part sits on its WepAbsorpPosParam body dummy (the body model loads first).
            let absorp = combat.param("WepAbsorpPosParam", combat.param("EquipParamWeapon", SWORD_WEAPON)["absorpParamId"].as_i64().unwrap_or(-1));
            let id = absorp[if is_sheath { "right_1" } else { "right_0" }].as_i64().unwrap_or(-1) as i16;
            weapon_parts.hang = (absorp["rightHang_0"].as_i64().unwrap_or(-1) as i16, absorp["rightHang_1"].as_i64().unwrap_or(-1) as i16);
            let Some(&(joint, local)) = weapon_parts.dummies.get(&id) else {
                warn!("{chr}: no body dummy {id} for {}", file.display());
                continue;
            };
            // A pivot per part, so TAE 715 OverrideWeaponModelLocation can move the whole part.
            let pivot = commands.spawn((Transform::from_matrix(local), Visibility::default())).id();
            commands.entity(joint).add_child(pivot);
            if is_sheath {
                weapon_parts.sheath = Some(pivot);
            } else {
                weapon_parts.blade = Some(pivot);
            }
            pivot
        } else {
            skel_root
        };
        // Joint entity per FLVER node: the animated bone of the same name, or a static
        // child (FLVER bind pose) under its parent's joint.
        let mut joints: Vec<Entity> = Vec::with_capacity(model.nodes.len());
        let mut world: Vec<Mat4> = Vec::with_capacity(model.nodes.len());
        for (i, node) in model.nodes.iter().enumerate() {
            let parent_world = if node.parent >= 0 && (node.parent as usize) < i { world[node.parent as usize] } else { Mat4::IDENTITY };
            world.push(parent_world * node.local.to_matrix());
            let e = match by_name.get(node.name.as_str()) {
                Some(&b) => skel.bones[b],
                None => {
                    let parent = if node.parent >= 0 && (node.parent as usize) < i { joints[node.parent as usize] } else { root };
                    let e = commands.spawn((node.local, Visibility::default())).id();
                    commands.entity(parent).add_child(e);
                    e
                }
            };
            joints.push(e);
        }
        let bone_map: HashMap<String, Entity> = model.nodes.iter().zip(&joints).map(|(n, &e)| (n.name.clone(), e)).collect();
        if file.file_name().is_some_and(|n| n.to_string_lossy() == format!("model_{chr}.bin")) {
            for dm in &model.dummies {
                if let Some(frame) = dummy_frame(dm) {
                    let (joint, bind) = if dm.attach >= 0 && (dm.attach as usize) < joints.len() {
                        (joints[dm.attach as usize], world[dm.attach as usize])
                    } else {
                        (root, Mat4::IDENTITY)
                    };
                    weapon_parts.dummies.entry(dm.id).or_insert((joint, bind.inverse() * frame));
                }
            }
        }
        let mut cloth_insts: Vec<crate::cloth::ClothInst> =
            cloth_defs.drain(..).map(|d| crate::cloth::ClothInst::new(d, skel_root, bone_map.clone())).collect();
        // NPC appearance variants: a material "#NN#name" is drawn when NpcParam modelDispMaskNN is
        // 1. c1020 10203010 "Honjo, Tokugawa invasion, no camp haori" draws 6,7,10,11,13,14,15,20,
        // 25,27 - armour, helmet, sheath #07 and the sheathed blade #06, and not the haori #12 (the
        // "with haori" rows add 12). (The Ashina Shitenno rows, 8/14/17/24, are another outfit.)
        // The enemy starts in battle, so TransToBattleFromDefault's TAE 233 ChangeChrDrawMask (draw
        // the sword: c1020 #04# on, #06# off; 255 = unchanged) applies from the start.
        let npc = combat.param("NpcParam", combat.foe.npc_row);
        let mut draw_mask: HashMap<u32, bool> = HashMap::new();
        if let Some(an) = combat.enemy.anim_key("TransToBattleFromDefault").and_then(|k| combat.enemy.anim(k)) {
            for e in an.events.iter().filter(|e| e.kind == 233) {
                for n in 0..32u32 {
                    match e.arg_i64(&format!("Mask{n}")) {
                        Some(0) => drop(draw_mask.insert(n, false)),
                        Some(1) => drop(draw_mask.insert(n, true)),
                        _ => {}
                    }
                }
            }
        }
        let visible = |md: &MeshData| {
            if a.side != Side::Enemy {
                return true;
            }
            let Some(rest) = md.material.strip_prefix('#') else { return true };
            let group = rest.split('#').next().and_then(|n| n.parse::<u32>().ok());
            if let Some(&on) = group.and_then(|n| draw_mask.get(&n)) {
                return on;
            }
            match group {
                Some(n) => npc[format!("modelDispMask{n}")].as_i64().unwrap_or(0) == 1,
                None => true,
            }
        };
        // "#NN#" group of an NPC mesh (draw-mask controlled), if any.
        let group_of = |md: &MeshData| -> Option<u32> {
            if a.side != Side::Enemy {
                return None;
            }
            md.material.strip_prefix('#')?.split('#').next()?.parse::<u32>().ok()
        };
        // Group meshes are always spawned (hidden when off) so TAE draw masks can toggle them.
        // SHINOBI_HIDE_MESH=<substring>[,...]: skip meshes whose material or albedo name matches (visual checks).
        let hide: Vec<String> = std::env::var("SHINOBI_HIDE_MESH").map(|v| v.to_lowercase().split(',').map(str::to_string).collect()).unwrap_or_default();
        let hidden = |md: &MeshData| hide.iter().any(|h| md.material.to_lowercase().contains(h) || md.albedo.contains(h));
        for (mi, md) in model.meshes.iter().enumerate().filter(|(_, md)| md.albedo != "-" && !hidden(md) && (visible(md) || group_of(md).is_some())) {
            let cloth = cloth_insts.iter().position(|c| c.def.display_meshes().any(|m| m == mi));
            // Compact joint list for this mesh.
            let mut remap: HashMap<u16, u16> = HashMap::new();
            let mut mesh_joints = Vec::new();
            let mut inv = Vec::new();
            let mut idx = Vec::with_capacity(md.bones.len());
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
                warn!("{chr}: mesh uses {} joints (>256), skipped", mesh_joints.len());
                continue;
            }
            // Cloth meshes keep a main-world copy: their vertices are rewritten every frame.
            let usage = if cloth.is_some() { RenderAssetUsages::all() } else { RenderAssetUsages::RENDER_WORLD };
            let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, usage);
            mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, md.pos.clone());
            mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, md.normal.clone());
            mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, md.uv.clone());
            // Cloth meshes are skinned on the CPU (cloth.rs): joint attributes would make the GPU
            // skin them again with no joints bound.
            if cloth.is_none() {
                mesh.insert_attribute(Mesh::ATTRIBUTE_JOINT_INDEX, VertexAttributeValues::Uint16x4(idx.clone()));
                mesh.insert_attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT, md.weights.clone());
            }
            mesh.insert_indices(Indices::U32(md.indices.clone()));
            // Normal mapping needs tangents (sekiro_material.wgsl builds the TBN from them).
            if !md.normal_map.is_empty() {
                let _ = mesh.generate_tangents();
            }
            // Bind-pose tangents for cloth meshes (cloth.rs carries them with the deformation).
            let bind_tangents: Vec<[f32; 4]> = match mesh.attribute(Mesh::ATTRIBUTE_TANGENT) {
                Some(VertexAttributeValues::Float32x4(t)) => t.clone(),
                _ => Vec::new(),
            };
            let texture = (!md.albedo.is_empty()).then(|| assets.load(format!("tex/{}.dds", md.albedo)));
            // Normal maps and metallic masks are data, not colour: read linear.
            let linear = |stem: &str| {
                (!stem.is_empty()).then(|| {
                    assets.load_builder().with_settings(|s: &mut bevy::image::ImageLoaderSettings| s.is_srgb = false).load(format!("tex/{stem}.dds"))
                })
            };
            // SHINOBI_NO_NORMALS=1: flat normals (visual checks of the normal-map decode).
            let normal = if std::env::var("SHINOBI_NO_NORMALS").is_ok() { None } else { linear(&md.normal_map) };
            let metallic = linear(&md.metallic);
            let material = materials.add(SekiroMaterial {
                base: StandardMaterial {
                    base_color_texture: texture,
                    perceptual_roughness: 0.8,
                    // Cutouts as alpha-to-coverage (with the camera's 4x MSAA): a hard 0.5 mask made
                    // the torn edges of Wolf's coat shimmer frame to frame whenever it moved a little.
                    alpha_mode: if md.alpha_test { AlphaMode::AlphaToCoverage } else { AlphaMode::Opaque },
                    double_sided: true,
                    cull_mode: None,
                    ..default()
                },
                extension: SekiroExt {
                    flags: UVec4::new(normal.is_some() as u32, metallic.is_some() as u32, 0, 0),
                    normal,
                    metallic,
                },
            });
            let base = visible(md);
            let vis = if base { Visibility::default() } else { Visibility::Hidden };
            let handle = meshes.add(mesh);
            let e = commands.spawn((Mesh3d(handle.clone()), MeshMaterial3d(material), Transform::default(), vis)).id();
            if let Some(group) = group_of(md) {
                commands.entity(e).insert(MeshGroup { owner: actor_e, group, base });
            }
            match cloth {
                // World-space CPU vertices (cloth.rs): no GPU skin, no parent, no stale bounds.
                Some(ci) => {
                    commands.entity(e).insert(bevy::camera::visibility::NoFrustumCulling);
                    cloth_insts[ci].displays.push(crate::cloth::DisplayMesh {
                        handle,
                        mesh: mi,
                        pos: md.pos.clone(),
                        normal: md.normal.clone(),
                        tangent: bind_tangents,
                        joints: mesh_joints,
                        inv,
                        idx,
                        weights: md.weights.clone(),
                    });
                }
                None => {
                    let skin = SkinnedMesh { inverse_bindposes: bindposes.add(SkinnedMeshInverseBindposes::from(inv)), joints: mesh_joints };
                    commands.entity(e).insert(skin);
                    commands.entity(root).add_child(e);
                }
            }
        }
        // Dummy polys: offset from the attach bone's bind pose, parented to that bone's joint.
        for dm in &model.dummies {
            let (joint, bind) = if dm.attach >= 0 && (dm.attach as usize) < joints.len() {
                (joints[dm.attach as usize], world[dm.attach as usize])
            } else {
                (root, Mat4::IDENTITY)
            };
            let local = bind.inverse().transform_point3(dm.pos);
            // Local +Z = the dummy's forward.
            let rot = dm.fwd.map_or(Quat::IDENTITY, |f| Quat::from_rotation_arc(Vec3::Z, bind.inverse().transform_vector3(f).normalize()));
            let e = commands.spawn((DummyPoly { id: dm.id }, Transform::from_translation(local).with_rotation(rot), Visibility::default())).id();
            commands.entity(joint).add_child(e);
            if part2 {
                // TAE ids 1<model><dummy>: right weapon Model2's 200 is 12200.
                dummies.0.insert(12000 + dm.id, e);
            } else if mortal.is_some() {
                // Its 300 / 301 would shadow Kusabimaru's; no TAE event names them.
            } else if is_weapon || !dummies.0.contains_key(&dm.id) {
                dummies.0.insert(dm.id, e);
            }
        }
        attached += model.meshes.iter().filter(|md| visible(md) && md.albedo != "-").count();
        for c in &cloth_insts {
            info!("{chr}: cloth {} ({} display meshes)", c.def.name, c.displays.len());
        }
        cloths.0.extend(cloth_insts);
        if is_weapon {
            // The real weapon replaces the placeholder blade.
            commands.entity(actor_e).insert(HasWeapon);
        }
        }
        if attached > 0 {
            commands.entity(actor_e).insert((HasModel, dummies, DrawMask::default(), cloths));
            if weapon_parts.blade.is_some() || weapon_parts.sheath.is_some() || weapon_parts.model2.is_some() {
                commands.entity(actor_e).insert(weapon_parts);
            }
            info!("{chr}: {attached} meshes attached");
        }
    }
}

fn hide_placeholder_blades(
    with_model: Query<Entity, Or<(Added<HasWeapon>, (Added<HasModel>, With<Actor>))>>,
    actors: Query<(&Actor, Option<&HasWeapon>)>,
    blades: Query<(Entity, &ChildOf), With<crate::world::Blade>>,
    parents: Query<&ChildOf>,
    mut vis: Query<&mut Visibility>,
) {
    if with_model.is_empty() {
        return;
    }
    // Walk each blade up to its actor; hide it when that actor has a real weapon (or is an NPC with a model).
    for (blade, mut parent) in &blades {
        let mut e = parent.parent();
        for _ in 0..64 {
            if let Ok((a, weapon)) = actors.get(e) {
                if weapon.is_some() || a.side == Side::Enemy {
                    if let Ok(mut v) = vis.get_mut(blade) {
                        *v = Visibility::Hidden;
                    }
                }
                break;
            }
            match parents.get(e) {
                Ok(p) => e = p.parent(),
                Err(_) => break,
            }
        }
        let _ = &mut parent;
    }
}

/// Applies TAE draw-mask events to NPC mesh groups: 233 (Mask0-31: 0 hide, 1 show, 255 keep) when
/// it starts - e.g. every death anim hides weapon groups 0-9 - and 711 / 713 while they run.
fn update_draw_masks(
    combat: Res<crate::data::Combat>,
    mut actors: Query<(&Actor, &mut DrawMask)>,
    mut groups: Query<(&MeshGroup, &mut Visibility)>,
) {
    for (a, mut dm) in &mut actors {
        if a.anim.is_empty() {
            continue;
        }
        let Some(an) = crate::actor::data_for(&combat, a.side).anim(&a.anim) else { continue };
        let started = |e: &crate::data::Event| (e.start > a.prev_t || (a.prev_t == 0.0 && e.start == 0.0)) && e.start <= a.t;
        for e in an.events.iter().filter(|e| e.kind == 233 && started(e)) {
            for n in 0..32u32 {
                match e.arg_i64(&format!("Mask{n}")) {
                    Some(0) => drop(dm.0.insert(n, false)),
                    Some(1) => drop(dm.0.insert(n, true)),
                    _ => {}
                }
            }
        }
    }
    for (g, mut vis) in &mut groups {
        let Ok((a, dm)) = actors.get(g.owner) else { continue };
        let mut on = dm.0.get(&g.group).copied().unwrap_or(g.base);
        if !a.anim.is_empty() {
            let key = format!("Mask{}", g.group);
            for e in crate::actor::data_for(&combat, a.side).events_at(&a.anim, a.t) {
                let set = e.args.get(&key).and_then(|v| v.as_bool()) == Some(true);
                match e.kind {
                    711 if set => on = false,
                    713 if set => on = true,
                    _ => {}
                }
            }
        }
        let want = if on { Visibility::Inherited } else { Visibility::Hidden };
        if *vis != want {
            *vis = want;
        }
    }
}

#[cfg(test)]
mod tests {
    /// The ThrowParam absorb dummies exist on the models (player.rs follow_throw).
    #[test]
    fn throw_dummies_exist() {
        let dir = crate::paths::root().join("extracted");
        let ids = |prefix: &str| -> std::collections::HashSet<i16> {
            std::fs::read_dir(&dir)
                .into_iter()
                .flatten()
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.file_name().unwrap().to_string_lossy().starts_with(prefix) && p.extension().is_some_and(|x| x == "bin"))
                .filter_map(|p| super::load(&p))
                .flat_map(|m| m.dummies.into_iter().map(|d| d.id))
                .collect()
        };
        let (wolf, general) = (ids("model_c0000"), ids("model_c1020"));
        println!("dummies: Wolf {} c1020 {}", wolf.len(), general.len());
        if wolf.is_empty() || general.is_empty() {
            return; // models not extracted
        }
        for id in [266, 246, 295, 523] {
            assert!(wolf.contains(&id), "Wolf lacks dummy {id}");
        }
        for id in [229, 233] {
            assert!(general.contains(&id), "c1020 lacks dummy {id}");
        }
    }

    /// The blade hangs on WepAbsorpPosParam right_0 = body dummy 20, not 22: same spot and forward,
    /// opposite up (on 22 the katana pointed behind Wolf).
    #[test]
    fn sword_sits_on_dummy_20() {
        let path = crate::paths::root().join("extracted/model_c0000.bin");
        let Some(m) = super::load(&path) else { return };
        let frame = |id: i16| m.dummies.iter().find(|d| d.id == id).and_then(super::dummy_frame);
        let (Some(d20), Some(d22), Some(_)) = (frame(20), frame(22), frame(147)) else { panic!("dummy 20 / 22 / 147 has no frame") };
        let (r20, r22) = (d20.to_scale_rotation_translation().1, d22.to_scale_rotation_translation().1);
        assert!((r20 * bevy::math::Vec3::Z).dot(r22 * bevy::math::Vec3::Z) > 0.99, "forward differs");
        assert!((r20 * bevy::math::Vec3::Y).dot(r22 * bevy::math::Vec3::Y) < -0.99, "up not opposite");
    }
}

#[cfg(test)]
mod mask_probe {
    /// c1020 mesh list: group, material, dominant bone, vertex count (draw-mask research).
    #[test]
    #[ignore]
    fn c1020_mesh_groups() {
        let dir = crate::paths::root().join("extracted");
        let chr = std::env::var("PROBE_CHR").unwrap_or("c1020".into());
        let m = super::load(&dir.join(format!("model_{chr}.bin"))).expect("model");
        for (i, md) in m.meshes.iter().enumerate() {
            let mut count: std::collections::HashMap<u16, usize> = std::collections::HashMap::new();
            for (b, w) in md.bones.iter().zip(&md.weights) {
                let k = (0..4).max_by(|&i, &j| w[i].total_cmp(&w[j])).unwrap_or(0);
                *count.entry(b[k]).or_default() += 1;
            }
            let bone = count.into_iter().max_by_key(|&(_, c)| c).map(|(b, _)| m.nodes.get(b as usize).map(|n| n.name.clone()).unwrap_or_default()).unwrap_or_default();
            println!("{i:3} {:40} {:28} {:20} v{}", md.material, md.albedo, bone, md.pos.len());
        }
    }
}

/// TAE 715 OverrideWeaponModelLocation (WeaponModelType 0, the right weapon): while it runs, the
/// blade (Model0DummyPolyID) and the scabbard (Model1DummyPolyID) sit on those body dummies (part
/// space = the dummy's frame, as on their WepAbsorpPosParam dummies). -1 = where they normally are.
/// Sheathed (Actor::sheathed) they sit on the rightHang dummies.
fn override_weapon_location(
    combat: Res<crate::data::Combat>,
    actors: Query<(&Actor, &WeaponParts)>,
    mut globals: Query<&mut GlobalTransform>,
    transforms: Query<&Transform>,
    children: Query<&Children>,
    mut vis: Query<&mut Visibility>,
) {
    for (a, wp) in &actors {
        let d = crate::actor::data_for(&combat, a.side);
        let of_type = |t: char| {
            d.events_at(&a.anim, a.t)
                .find(|e| e.kind == 715 && e.args.get("WeaponModelType").and_then(|v| v.as_str()).is_some_and(|s| s.starts_with(t)))
        };
        let e = of_type('0');
        // WeaponModelType 2 (the Mortal Blade): visible only while its event places it.
        let m = of_type('2');
        for (i, part) in wp.mortal.iter().enumerate() {
            let Some(part) = *part else { continue };
            let id = m.and_then(|e| e.arg_i64(if i == 0 { "Model0DummyPolyID" } else { "Model1DummyPolyID" }).filter(|id| *id >= 0));
            if let Ok(mut v) = vis.get_mut(part) {
                let want = if id.is_some() { Visibility::Inherited } else { Visibility::Hidden };
                if *v != want {
                    *v = want;
                }
            }
            let Some(&(joint, local)) = id.and_then(|id| wp.dummies.get(&(id as i16))) else { continue };
            let Ok(jg) = globals.get(joint).map(|g| g.to_matrix()) else { continue };
            set_global(part, GlobalTransform::from(bevy::math::Affine3A::from_mat4(jg * local)), &mut globals, &transforms, &children);
        }
        for (part, key, hang) in [(wp.blade, "Model0DummyPolyID", wp.hang.0), (wp.sheath, "Model1DummyPolyID", wp.hang.1), (wp.model2, "Model2DummyPolyID", wp.hang2)] {
            // The event's dummy, else the sheathed (hang) position while the sword is put away.
            let id = e.and_then(|e| e.arg_i64(key).filter(|id| *id >= 0)).or_else(|| (a.sheathed && hang >= 0).then_some(hang as i64));
            let (Some(part), Some(id)) = (part, id) else { continue };
            let Some(&(joint, local)) = wp.dummies.get(&(id as i16)) else { continue };
            let Ok(jg) = globals.get(joint).map(|g| g.to_matrix()) else { continue };
            let target = GlobalTransform::from(bevy::math::Affine3A::from_mat4(jg * local));
            set_global(part, target, &mut globals, &transforms, &children);
        }
    }
}

/// Overrides an entity's world transform after propagation and re-propagates to its children.
fn set_global(e: Entity, g: GlobalTransform, globals: &mut Query<&mut GlobalTransform>, transforms: &Query<&Transform>, children: &Query<&Children>) {
    if let Ok(mut gt) = globals.get_mut(e) {
        *gt = g;
    }
    if let Ok(kids) = children.get(e) {
        for &k in kids {
            if let Ok(t) = transforms.get(k) {
                set_global(k, g.mul_transform(*t), globals, transforms, children);
            }
        }
    }
}
