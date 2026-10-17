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
    /// Character_AMSN[DetailBlend] fabric layers (model v6; None for other shaders and older
    /// models).
    detail: Option<DetailData>,
    pos: Vec<[f32; 3]>,
    normal: Vec<[f32; 3]>,
    uv: Vec<[f32; 2]>,
    bones: Vec<[u16; 4]>,
    weights: Vec<[f32; 4]>,
    indices: Vec<u32>,
}

/// DetailBlend (docs/kb/visuals.md "DetailBlend"): the variant, its layer bits (exporter
/// `DETAIL_LAYERS`), the layer textures (A albedo / normal, B, C, D) and the 40 constants.
/// Variant `FRESNEL`: the coat shader instead ("Coat"; layers 3 / 1, 4 / 5).
#[derive(Clone)]
struct DetailData {
    variant: u8,
    bits: u8,
    names: [String; 8],
    params: Vec<f32>,
    /// `SHELL` only: each vertex's colour alpha (the shell layer's opacity).
    vert_alpha: Vec<f32>,
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

/// Tests: one mesh's bind positions, normals and indices (cloth.rs).
#[cfg(test)]
pub(crate) fn test_mesh(path: &std::path::Path, mesh: usize) -> Option<(Vec<[f32; 3]>, Vec<[f32; 3]>, Vec<u32>)> {
    let m = load(path)?.meshes.into_iter().nth(mesh)?;
    Some((m.pos, m.normal, m.indices))
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
        let detail = if version >= 6 {
            let variant = d[o];
            o += 1;
            (variant != 255).then(|| {
                let bits = d[o];
                o += 1;
                let names: [String; 8] = std::array::from_fn(|_| string(&mut o));
                let params = (0..40).map(|k| f(o + k * 4)).collect();
                o += 160;
                let mut vert_alpha = Vec::new();
                if variant == SHELL {
                    let n = u32_(o) as usize;
                    vert_alpha = (0..n).map(|k| f(o + 4 + k * 4)).collect();
                    o += 4 + n * 4;
                }
                DetailData { variant, bits, names, params, vert_alpha }
            })
        } else {
            None
        };
        let vc = u32_(o) as usize;
        o += 4;
        let mut md = MeshData { material, albedo, normal_map, metallic, alpha_test, detail, pos: Vec::new(), normal: Vec::new(), uv: Vec::new(), bones: Vec::new(), weights: Vec::new(), indices: Vec::new() };
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

/// A character mesh: its actor and the material's own alpha mode (TAE 193 opacity blends it).
#[derive(Component)]
pub struct ModelMesh {
    owner: Entity,
    alpha: AlphaMode,
}

/// Groups switched by TAE 233 ChangeChrDrawMask (FUN_140b54330 -> FDPChrPrimDispMask): kept
/// until changed again. TAE 711 HideModelMask / 713 ShowModelMask override only while they run.
#[derive(Component, Default)]
pub struct DrawMask(HashMap<u32, bool>);

impl DrawMask {
    /// Back to the model's own groups (a respawn: the death anim's TAE 233 hid the weapons).
    pub fn clear(&mut self) {
        self.0.clear();
    }

    /// Groups a TAE 233 has hidden.
    #[cfg(test)]
    pub fn hidden(&self) -> usize {
        self.0.values().filter(|on| !**on).count()
    }
}

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
    /// DetailBlend fabric layers (sekiro_material.wgsl `detail_layers`): A albedo / normal, B, C,
    /// D, all repeating; sampled with layer A's sampler.
    #[texture(110)]
    #[sampler(111)]
    pub layer_a: Option<Handle<Image>>,
    #[texture(112)]
    pub layer_a_n: Option<Handle<Image>>,
    #[texture(113)]
    pub layer_b: Option<Handle<Image>>,
    #[texture(114)]
    pub layer_b_n: Option<Handle<Image>>,
    #[texture(115)]
    pub layer_c: Option<Handle<Image>>,
    #[texture(116)]
    pub layer_c_n: Option<Handle<Image>>,
    #[texture(117)]
    pub layer_d: Option<Handle<Image>>,
    #[texture(118)]
    pub layer_d_n: Option<Handle<Image>>,
    #[uniform(119)]
    pub detail: DetailUniform,
}

/// The exporter's variant byte for Character_AN_Blend_AMSN_[Fresnel].
const FRESNEL: u8 = 100;
/// The exporter's variant byte for Character_Ghost_scroll_test2 (the Demon of Hatred's arm).
const GHOST: u8 = 101;
/// The exporter's variant byte for C[Shell] (fur shells; the vertex colour alpha comes with it).
const SHELL: u8 = 102;

/// sekiro_material.wgsl `Detail`: flags x = 1 DetailBlend / 2 the coat shader / 3 the ghost fire, y = the layer bits, z = which layer
/// textures are bound (bit k = `SekiroExt` layer texture k, A albedo first); `p` = the exporter's
/// `detail_params`.
#[derive(bevy::render::render_resource::ShaderType, Reflect, Debug, Clone, Default)]
pub struct DetailUniform {
    pub flags: UVec4,
    pub p: [Vec4; 10],
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
            // Characters spawned later (a fight reset's line-up, the encounter picker).
            .add_systems(Update, attach_models.after(crate::anim::AnimSet))
            .add_systems(Update, (hide_placeholder_blades, update_draw_masks, update_opacity, follow_owner.after(update_draw_masks)))
            // TAE 233 is an edge: caught on the tick it starts (a frame can run two ticks).
            .add_systems(FixedUpdate, apply_draw_mask_events.after(crate::actor::ActorSet::Advance))
            // TAE 715 in the sim pose too (anim.rs PoseSet propagates by hand, from the rest dummies).
            .add_systems(FixedUpdate, override_weapon_location.in_set(crate::anim::PoseSet).after(crate::anim::store_sim_poses))
            .add_systems(PostUpdate, override_weapon_location.in_set(WeaponLocation).after(bevy::transform::TransformSystems::Propagate));
        tool::plugin(app);
    }
}

/// PostUpdate: TAE 715 moves the weapon parts (and their dummies) after transform propagation.
/// Anything reading weapon dummies in PostUpdate (the effects, fxr.rs `tick`) runs after it, or it
/// sees the parts at their rest dummies (Mortal Draw's arcs were drawn from Model2 on the hip, 149).
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct WeaponLocation;

/// The characters' real geometry without drawing it (sim tests): the FLVER joints, weapon
/// pivots and dummy polys (`Dummies`) on the AnimPlugin skeletons, so the attack capsules
/// between dummy polys and the ragdoll hurt capsules are the game's, headless.
pub struct GeometryPlugin;

impl Plugin for GeometryPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PostStartup, attach_models.after(crate::anim::AnimSet))
            .add_systems(Update, attach_models.after(crate::anim::AnimSet))
            // TAE 715 in the sim pose too (anim.rs PoseSet propagates by hand, from the rest dummies).
            .add_systems(FixedUpdate, override_weapon_location.in_set(crate::anim::PoseSet).after(crate::anim::store_sim_poses))
            .add_systems(PostUpdate, override_weapon_location.in_set(WeaponLocation).after(bevy::transform::TransformSystems::Propagate));
    }
}

/// An actor attach_models has handled (with or without a model found), so it runs once each.
#[derive(Component)]
pub struct ModelTried;

#[allow(clippy::too_many_arguments)]
fn attach_models(
    combat: Res<crate::data::Combat>,
    mut commands: Commands,
    lib: Res<AnimLib>,
    // None headless (sim tests with real geometry): the joints, weapon pivots and dummy polys
    // are built, the meshes and cloth are not.
    assets: Option<Res<AssetServer>>,
    mut meshes: Option<ResMut<Assets<Mesh>>>,
    mut materials: Option<ResMut<Assets<SekiroMaterial>>>,
    mut bindposes: Option<ResMut<Assets<SkinnedMeshInverseBindposes>>>,
    actors: Query<(Entity, &Actor, &Skeleton), Without<ModelTried>>,
    roots: Query<(Entity, &ChildOf, &Name)>,
) {
    let dir = crate::paths::root().join("extracted");
    for (actor_e, a, skel) in &actors {
        let chr = match a.side {
            Side::Player => "c0000",
            Side::Enemy => combat.foe_of(a).chr.as_str(),
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
        let skel_lib = lib.lib(&a);
        let by_name: HashMap<&str, usize> = skel_lib.bones.iter().enumerate().map(|(i, b)| (b.name.as_str(), i)).collect();
        let Some(skel_root) = roots.iter().find(|(_, p, n)| p.parent() == actor_e && n.as_str() == "SkeletonRoot").map(|(e, _, _)| e) else {
            continue;
        };
        commands.entity(actor_e).insert(ModelTried);
        let mut attached = 0;
        let mut dummies = Dummies::default();
        let mut weapon_parts = WeaponParts::default();
        let mut cloths = crate::cloth::Cloths::default();
        for file in files {
        let Some(model) = load(&file) else { continue };
        // Havok cloth (model_<x>.cloth.json): its display meshes are drawn from CPU vertices.
        let headless = meshes.is_none() || materials.is_none() || bindposes.is_none() || assets.is_none();
        let mut cloth_defs = if headless { Vec::new() } else { crate::cloth::load(&file) };
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
        // the sword: c1020 #04# on, #06# off; 255 = unchanged) applies from the start, then
        // IdleBattle's, where that transition ends: Emma's (c7400 a000_001040) turns all five
        // sword parts #00#-#04# on and her a000_400000 keeps #00# and #04# only (without it her
        // attacks' TAE 711 / 713 drew the extra parts: a handle held up with no blade, a000_003004).
        let npc = combat.npc(a);
        let mut draw_mask: HashMap<u32, bool> = HashMap::new();
        for an in ["TransToBattleFromDefault", "IdleBattle"].into_iter().filter_map(|s| combat.data_of(a).anim_key(s).and_then(|k| combat.data_of(a).anim(k))) {
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
        // A mesh without vertices is skipped: Bevy's mesh allocator allocates nothing for it
        // but still copies, logging "slab_allocator Use-after-free" at start.
        for (mi, md) in model.meshes.iter().enumerate().filter(|(_, md)| !headless && md.albedo != "-" && !md.pos.is_empty() && !hidden(md) && (visible(md) || group_of(md).is_some())) {
            let (Some(assets), Some(meshes), Some(materials), Some(bindposes)) = (assets.as_deref(), meshes.as_deref_mut(), materials.as_deref_mut(), bindposes.as_deref_mut()) else { break };
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
            // C[Shell]: the shell opacity rides in the vertex colour alpha (the shader's v4.w).
            if let Some(d) = md.detail.as_ref().filter(|d| d.variant == SHELL && d.vert_alpha.len() == md.pos.len()) {
                mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, d.vert_alpha.iter().map(|&a| [1.0, 1.0, 1.0, a]).collect::<Vec<[f32; 4]>>());
            }
            mesh.insert_indices(Indices::U32(md.indices.clone()));
            // Culling bounds that follow the joints: the bind-pose box left a part that moved far
            // from it culled while in view (Gyoubu's face in his deathblow, falling off the horse:
            // the user, 2026-10-16).
            let skin_bounds = cloth.is_none() && mesh.generate_skinned_mesh_bounds().is_ok();
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
            // DetailBlend layers: tiled (the group UV tiling runs far past 0..1), so repeating;
            // albedos sRGB, normals linear. `SHINOBI_NO_DETAIL=1` draws the base alone (before).
            let layer = |stem: &str, srgb: bool| -> Option<Handle<Image>> {
                (!stem.is_empty() && crate::paths::extracted().join(format!("tex/{stem}.dds")).is_file()).then(|| {
                    assets
                        .load_builder()
                        .with_settings(move |s: &mut bevy::image::ImageLoaderSettings| {
                            s.is_srgb = srgb;
                            s.sampler = bevy::image::ImageSampler::Descriptor(bevy::image::ImageSamplerDescriptor {
                                address_mode_u: bevy::image::ImageAddressMode::Repeat,
                                address_mode_v: bevy::image::ImageAddressMode::Repeat,
                                mag_filter: bevy::image::ImageFilterMode::Linear,
                                min_filter: bevy::image::ImageFilterMode::Linear,
                                mipmap_filter: bevy::image::ImageFilterMode::Linear,
                                anisotropy_clamp: 8,
                                ..default()
                            });
                        })
                        .load(format!("tex/{stem}.dds"))
                })
            };
            let detail = md.detail.as_ref().filter(|_| std::env::var("SHINOBI_NO_DETAIL").is_err());
            let layers: [Option<Handle<Image>>; 8] = std::array::from_fn(|k| detail.and_then(|d| layer(&d.names[k], k % 2 == 0)));
            let detail_u = detail.map_or_else(DetailUniform::default, |d| DetailUniform {
                flags: UVec4::new(match d.variant { FRESNEL => 2, GHOST => 3, SHELL => 4, _ => 1 }, d.bits as u32, layers.iter().enumerate().fold(0, |f, (k, l)| f | ((l.is_some() as u32) << k)), 0),
                p: std::array::from_fn(|k| Vec4::from_slice(&d.params[k * 4..k * 4 + 4])),
            });
            let [layer_a, layer_a_n, layer_b, layer_b_n, layer_c, layer_c_n, layer_d, layer_d_n] = layers;
            // Cutouts as alpha-to-coverage (with the camera's 4x MSAA): a hard 0.5 mask made
            // the torn edges of Wolf's coat shimmer frame to frame whenever it moved a little.
            // The ghost fire (c7020_arm_test.mtd g_BlendMode 2) is alpha blended.
            let alpha_mode = if md.detail.as_ref().is_some_and(|d| d.variant == GHOST) {
                AlphaMode::Blend
            } else if md.alpha_test {
                AlphaMode::AlphaToCoverage
            } else {
                AlphaMode::Opaque
            };
            let material = materials.add(SekiroMaterial {
                base: StandardMaterial {
                    base_color_texture: texture,
                    perceptual_roughness: 0.8,
                    alpha_mode,
                    double_sided: true,
                    cull_mode: None,
                    ..default()
                },
                extension: SekiroExt {
                    flags: UVec4::new(normal.is_some() as u32, metallic.is_some() as u32, 0, 0),
                    normal,
                    metallic,
                    layer_a,
                    layer_a_n,
                    layer_b,
                    layer_b_n,
                    layer_c,
                    layer_c_n,
                    layer_d,
                    layer_d_n,
                    detail: detail_u,
                },
            });
            let base = visible(md);
            let vis = if base { Visibility::default() } else { Visibility::Hidden };
            let handle = meshes.add(mesh);
            let e = commands.spawn((Mesh3d(handle.clone()), MeshMaterial3d(material), Transform::default(), vis, ModelMesh { owner: actor_e, alpha: alpha_mode })).id();
            if let Some(group) = group_of(md) {
                commands.entity(e).insert(MeshGroup { owner: actor_e, group, base });
            }
            match cloth {
                // World-space CPU vertices (cloth.rs): no GPU skin, no parent, no stale bounds.
                Some(ci) => {
                    commands.entity(e).insert((bevy::camera::visibility::NoFrustumCulling, FreeMesh::default()));
                    cloth_insts[ci].displays.push(crate::cloth::DisplayMesh {
                        handle,
                        entity: e,
                        mesh: mi,
                        pos: md.pos.clone(),
                        normal: md.normal.clone(),
                        tangent: bind_tangents,
                        joints: mesh_joints,
                        inv,
                        idx,
                        weights: md.weights.clone(),
                        tris: md.indices.chunks_exact(3).map(|t| [t[0], t[1], t[2]]).collect(),
                    });
                }
                None => {
                    let skin = SkinnedMesh { inverse_bindposes: bindposes.add(SkinnedMeshInverseBindposes::from(inv)), joints: mesh_joints };
                    commands.entity(e).insert(skin);
                    if skin_bounds {
                        commands.entity(e).insert(bevy::camera::visibility::DynamicSkinnedMeshBounds);
                    } else {
                        commands.entity(e).insert(bevy::camera::visibility::NoFrustumCulling);
                    }
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
            // Local +Z = the dummy's forward, +Y its up (as `dummy_frame`), in the bone's space.
            let rot = match (dm.fwd, dm.up) {
                (Some(f), Some(u)) => {
                    let inv = bind.inverse();
                    let z = inv.transform_vector3(f).normalize();
                    let y = inv.transform_vector3(u);
                    let y = (y - z * y.dot(z)).normalize_or_zero();
                    if y == Vec3::ZERO { Quat::from_rotation_arc(Vec3::Z, z) } else { Quat::from_mat3(&Mat3::from_cols(y.cross(z), y, z)) }
                }
                (Some(f), None) => Quat::from_rotation_arc(Vec3::Z, bind.inverse().transform_vector3(f).normalize()),
                _ => Quat::IDENTITY,
            };
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

/// Records TAE 233 ChangeChrDrawMask (Mask0-31: 0 hide, 1 show, 255 keep) on the tick it starts -
/// e.g. every death anim hides weapon groups 0-9. In FixedUpdate after the advance: (prev_t, t]
/// is one tick's window, and Update would miss a start in the first of two ticks run in a frame.
pub(crate) fn apply_draw_mask_events(combat: Res<crate::data::Combat>, mut actors: Query<(&Actor, &mut DrawMask)>) {
    for (a, mut dm) in &mut actors {
        if a.anim.is_empty() {
            continue;
        }
        let Some(an) = crate::actor::data_for(&combat, &a).anim(&a.anim) else { continue };
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
}

/// Shows NPC mesh groups by the kept TAE 233 mask, and TAE 711 / 713 while they run.
pub(crate) fn update_draw_masks(
    combat: Res<crate::data::Combat>,
    actors: Query<(&Actor, &DrawMask)>,
    mut groups: Query<(&MeshGroup, &mut Visibility)>,
) {
    for (g, mut vis) in &mut groups {
        let Ok((a, dm)) = actors.get(g.owner) else { continue };
        let mut on = dm.0.get(&g.group).copied().unwrap_or(g.base);
        if !a.anim.is_empty() {
            let key = format!("Mask{}", g.group);
            for e in crate::actor::data_for(&combat, &a).events_at(&a.anim, a.t) {
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

/// A character mesh outside the actor's hierarchy: the cloth display meshes (world-space CPU
/// vertices, so no parent). `hidden` holds the visibility it had while its actor is not drawn.
#[derive(Component, Default)]
pub struct FreeMesh {
    hidden: Option<Visibility>,
}

/// The free meshes go with their actor: despawned with it (a fight reset, the encounter
/// picker, a boss's map event handing over to the next one: the coats stayed on the arena) and
/// hidden while it is (the Monk's phantoms out of the fight).
fn follow_owner(mut commands: Commands, owners: Query<Option<&InheritedVisibility>>, mut meshes: Query<(Entity, &ModelMesh, &mut FreeMesh, &mut Visibility)>) {
    for (e, m, mut free, mut vis) in &mut meshes {
        let Ok(owner) = owners.get(m.owner) else {
            commands.entity(e).despawn();
            continue;
        };
        if owner.is_some_and(|v| !v.get()) {
            if *vis != Visibility::Hidden {
                free.hidden = Some(*vis);
                *vis = Visibility::Hidden;
            }
        } else if let Some(v) = free.hidden.take() {
            *vis = v;
        }
    }
}

/// TAE 193 opacity (Actor::opacity) times the camera's fade (Actor::cam_alpha) on the actor's
/// materials: blended below 1, its own alpha mode again at 1.
fn update_opacity(
    actors: Query<&Actor>,
    meshes: Query<(&ModelMesh, &MeshMaterial3d<SekiroMaterial>)>,
    mut materials: ResMut<Assets<SekiroMaterial>>,
) {
    let mut seen: HashMap<Entity, f32> = HashMap::new();
    for (m, mat) in &meshes {
        let op = *seen.entry(m.owner).or_insert_with(|| actors.get(m.owner).map_or(1.0, |a| a.opacity * a.cam_alpha));
        let Some(cur) = materials.get(&mat.0) else { continue };
        let mode = if op < 1.0 { AlphaMode::Blend } else { m.alpha };
        if (cur.base.base_color.alpha() - op).abs() < 0.004 && cur.base.alpha_mode == mode {
            continue;
        }
        let Some(mut w) = materials.get_mut(&mat.0) else { continue };
        w.base.base_color.set_alpha(op);
        w.base.alpha_mode = mode;
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

    /// A coat (free cloth display mesh) hides with its actor, comes back with it and is despawned
    /// with it (the coats stayed on the arena after the fight).
    #[test]
    fn free_meshes_follow_their_actor() {
        use bevy::prelude::*;
        let mut app = App::new();
        app.add_systems(Update, super::follow_owner);
        let owner = app.world_mut().spawn(InheritedVisibility::VISIBLE).id();
        let coat = app.world_mut().spawn((super::ModelMesh { owner, alpha: AlphaMode::Opaque }, super::FreeMesh::default(), Visibility::Inherited)).id();
        app.update();
        assert_eq!(app.world().get::<Visibility>(coat), Some(&Visibility::Inherited));
        app.world_mut().entity_mut(owner).insert(InheritedVisibility::HIDDEN);
        app.update();
        assert_eq!(app.world().get::<Visibility>(coat), Some(&Visibility::Hidden));
        app.world_mut().entity_mut(owner).insert(InheritedVisibility::VISIBLE);
        app.update();
        assert_eq!(app.world().get::<Visibility>(coat), Some(&Visibility::Inherited));
        app.world_mut().entity_mut(owner).despawn();
        app.update();
        assert!(app.world().get_entity(coat).is_err(), "the coat outlived its actor");
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
        let d = crate::actor::data_for(&combat, &a);
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
