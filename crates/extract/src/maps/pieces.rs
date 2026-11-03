//! Map piece FLVERs to static GLBs. Authored detail scenes, mirrored on X like every export (see
//! docs/FORMATS.md). Textures are referenced as external DDS files shared by the whole map.

use std::collections::HashMap;

use anyhow::{Result, bail};
use glam::Vec3;
use sekiro_formats::{flver, mtd};
use serde_json::{Value, json};

use crate::models::glb::Glb;
use crate::models::textures::texture_stem;

/// A texture choice: game path (its stem is the texture name) and UV channel.
#[derive(Clone, Debug)]
pub struct TexRef {
    pub path: String,
    pub uv: usize,
    pub scale: [f32; 2],
}

/// What one FLVER material renders with.
#[derive(Clone, Debug, Default)]
pub struct MaterialChoice {
    pub albedo: Option<TexRef>,
    /// Tangent-space normal map (exported converted to BC5, see `textures::normal_to_bc5`).
    pub normal: Option<TexRef>,
    pub alpha_mode: Option<(&'static str, f32)>,
    pub mtd: String,
    pub layers: Option<Layers>,
    pub double_sided: bool,
    pub fog_surface: Option<FogSurface>,
}

#[derive(Clone, Debug)]
pub struct FogSurface {
    pub family: &'static str,
    pub params: Value,
    pub textures: Vec<(u32, TexRef)>,
}

#[derive(Clone, Debug, Default)]
pub struct Layers {
    pub kind: u8,
    pub variant: Option<usize>,
    pub params: [[f32; 4]; 16],
    pub base_normal: Option<TexRef>,
    pub detail: Option<TexRef>,
    pub mask_l: Option<TexRef>,
    pub wet: Option<TexRef>,
    pub overlay: Option<TexRef>,
    pub overlay_normal: Option<TexRef>,
    pub snow: Option<TexRef>,
    pub snow_normal: Option<TexRef>,
    pub c: Option<TexRef>,
    pub c_normal: Option<TexRef>,
    pub mask: Option<TexRef>,
}

fn slot_number(kind: &str) -> Option<u32> {
    kind.split("Texture2D_")
        .nth(1)?
        .split('_')
        .next()?
        .parse()
        .ok()
}

/// MTDs by lower-case file name.
pub struct Mtds(pub HashMap<String, mtd::Mtd>);

impl Mtds {
    pub fn get(&self, mtd_path: &str) -> Option<&mtd::Mtd> {
        self.0.get(&format!(
            "{}.mtd",
            texture_stem(mtd_path).to_ascii_lowercase()
        ))
    }
}

/// Picks the base colour and normal map for a map material.
///
/// Map materials stack many layers (`M_Multiple`, `MultiBlend3`, ...) and leave the FLVER
/// texture paths empty, so paths come from the MTD. The base layer is the first albedo slot
/// with a path, in MTD order (for `M_Multiple` that is slot 8, the main surface; later slots
/// are moss, snow and other overlays). A slot named after the material (`<name>_a`) wins.
/// The normal map is the one named like the albedo (`x_a` -> `x_n`), else the one named after
/// the material (`<name>_n`, often the mesh's own baked normal), else the first with a path.
pub fn choose_material(material: &flver::Material, mtd: Option<&mtd::Mtd>) -> MaterialChoice {
    let mut albedo = Vec::new();
    let mut normal = Vec::new();
    for t in &material.textures {
        let from_mtd = mtd.and_then(|m| m.textures.iter().find(|mt| mt.kind == t.param));
        let path = if t.path.is_empty() {
            from_mtd.map(|m| m.path.clone()).unwrap_or_default()
        } else {
            t.path.clone()
        };
        if path.is_empty() {
            continue;
        }
        let uv = from_mtd
            .map(|m| (m.uv_number - 1).clamp(0, 1) as usize)
            .unwrap_or(0);
        let param = t.param.to_ascii_lowercase();
        let r = TexRef {
            path,
            uv,
            scale: t.scale,
        };
        if param.contains("albedomap") || param.contains("diffuse") {
            albedo.push(r);
        } else if param.contains("normalmap") {
            normal.push(r);
        }
    }
    // Wind and Gradient variants can use different sampler names for the same foliage.
    // Only fall back when no albedo resolved, and prefer the primary (_0) MTD set.
    if albedo.is_empty()
        && let Some(m) = mtd
    {
        let primary = |needle: &str| {
            m.textures
                .iter()
                .filter(|t| !t.path.is_empty() && t.kind.to_ascii_lowercase().contains(needle))
                .min_by_key(|t| {
                    (
                        !t.kind.ends_with("_0"),
                        slot_number(&t.kind).unwrap_or(u32::MAX),
                    )
                })
                .map(|t| TexRef {
                    path: t.path.clone(),
                    uv: (t.uv_number - 1).clamp(0, 1) as usize,
                    scale: [1.; 2],
                })
        };
        albedo.extend(primary("albedomap"));
        normal.extend(primary("normalmap"));
    }
    let own = format!("{}_a", material.name.to_ascii_lowercase());
    let a = albedo
        .iter()
        .position(|r| texture_stem(&r.path).eq_ignore_ascii_case(&own))
        .or(if albedo.is_empty() { None } else { Some(0) });
    let named = |want: &str| {
        normal
            .iter()
            .position(|r| texture_stem(&r.path).eq_ignore_ascii_case(want))
    };
    let n = a
        .and_then(|a| {
            let stem = texture_stem(&albedo[a].path).to_ascii_lowercase();
            named(&format!("{}_n", stem.strip_suffix("_a")?))
        })
        .or_else(|| named(&format!("{}_n", material.name.to_ascii_lowercase())))
        .or(if normal.is_empty() { None } else { Some(0) });
    let mut choice = MaterialChoice {
        albedo: a.map(|i| albedo[i].clone()),
        normal: n.map(|i| normal[i].clone()),
        alpha_mode: None,
        mtd: texture_stem(&material.mtd).to_string(),
        layers: None,
        double_sided: false,
        fog_surface: None,
    };
    if let Some(m) = mtd {
        let family = if m.shader.contains("[LandScape]_GroundFog") {
            Some("GroundFog")
        } else if m.shader.contains("[LandScape]_CloudFog") {
            Some("CloudFog")
        } else {
            None
        };
        if let Some(family) = family {
            let mut params = json!({});
            for p in &m.params {
                params[&p.name] = match &p.value {
                    mtd::ParamValue::Bool(v) => json!(v),
                    mtd::ParamValue::Ints(v) => json!(v),
                    mtd::ParamValue::Floats(v) => json!(v),
                };
            }
            let textures = m
                .textures
                .iter()
                .filter_map(|t| {
                    let number = slot_number(&t.kind)?;
                    let source = material.textures.iter().find(|f| f.param == t.kind);
                    let path = source
                        .filter(|f| !f.path.is_empty())
                        .map_or(&t.path, |f| &f.path);
                    (!path.is_empty()).then(|| {
                        (
                            number,
                            TexRef {
                                path: path.clone(),
                                uv: (t.uv_number - 1).max(0) as usize,
                                scale: source.map_or([1.; 2], |f| f.scale),
                            },
                        )
                    })
                })
                .collect();
            choice.fog_surface = Some(FogSurface {
                family,
                params,
                textures,
            });
            choice.alpha_mode = Some(("BLEND", 0.));
            // Masks are shader data, not a diffuse texture or a normal map.
            choice.albedo = None;
            choice.normal = None;
        }
        let int = |name: &str| match m.param(name) {
            Some(mtd::ParamValue::Ints(v)) => v.first().copied().unwrap_or(0),
            _ => 0,
        };
        let (alpha_ref, blend) = (int("g_AlphaRef"), int("g_BlendMode"));
        choice.double_sided = int("g_DoubleSided") != 0
            || matches!(m.param("g_DoubleSided"), Some(mtd::ParamValue::Bool(true)));
        if blend == 2 || blend >= 4 {
            choice.alpha_mode = Some(("BLEND", 0.0));
        } else if alpha_ref > 0 || blend == 1 {
            choice.alpha_mode = Some(("MASK", (alpha_ref.max(1) as f32 / 255.0).max(0.3)));
            choice.double_sided = true;
        }
        let kind = sekiro_formats::map_layers::family(&m.shader);
        if kind > 0 {
            let variant = sekiro_formats::map_layers::variant(m);
            let used = variant.map(|v| sekiro_formats::map_layers::slots(kind, v));
            let slot = |number, semantic: &str| {
                if used.is_some_and(|slots| !slots.contains(&(number as u8))) {
                    return None;
                }
                m.textures
                    .iter()
                    .find(|t| slot_number(&t.kind) == Some(number) && t.kind.contains(semantic))
                    .and_then(|t| {
                        let path = material
                            .textures
                            .iter()
                            .find(|f| f.param == t.kind && !f.path.is_empty())
                            .map(|f| &f.path)
                            .unwrap_or(&t.path);
                        (!path.is_empty()).then(|| TexRef {
                            path: path.clone(),
                            uv: (t.uv_number - 1).clamp(0, 1) as usize,
                            scale: material
                                .textures
                                .iter()
                                .find(|f| f.param == t.kind)
                                .map_or([1.; 2], |f| f.scale),
                        })
                    })
            };
            let ground = kind == 2;
            let mountain = kind == 3;
            choice.albedo = slot(if mountain { 3 } else { 8 }, "AlbedoMap");
            choice.normal = slot(if mountain { 0 } else { 3 }, "NormalMap");
            choice.layers = Some(Layers {
                kind,
                variant,
                params: sekiro_formats::map_layers::game_params(m, kind),
                base_normal: slot(if mountain { 0 } else { 3 }, "NormalMap"),
                detail: slot(
                    if mountain {
                        7
                    } else if ground {
                        12
                    } else {
                        0
                    },
                    "NormalMap",
                ),
                mask_l: if ground { None } else { slot(6, "Mask") },
                wet: slot(if ground { 0 } else { 16 }, "NormalMap"),
                overlay: slot(5, "AlbedoMap"),
                overlay_normal: slot(4, "NormalMap"),
                snow: slot(
                    if mountain {
                        2
                    } else if ground {
                        11
                    } else {
                        12
                    },
                    "Albedo",
                ),
                snow_normal: slot(if ground { 7 } else { 10 }, "NormalMap"),
                c: slot(if ground { 1 } else { 7 }, "AlbedoMap"),
                c_normal: if ground { slot(6, "NormalMap") } else { None },
                mask: slot(if mountain { 8 } else { 9 }, "Mask"),
            });
            if mountain && let Some(layers) = &mut choice.layers {
                for (i, r) in [
                    (9, &layers.base_normal),
                    (10, &layers.snow),
                    (11, &choice.albedo),
                    (12, &layers.detail),
                    (13, &layers.mask),
                ] {
                    if let Some(r) = r {
                        layers.params[i] = [r.scale[0], r.scale[1], r.uv as f32, 0.];
                    }
                }
            }
        }
    }
    choice
}

/// Materials whose meshes are not drawn as visible surfaces in our renderer.
pub fn skip_material(mtd_name: &str) -> bool {
    let m = mtd_name.to_ascii_lowercase();
    // Shadow casters, invisible blockers and light-probe helpers.
    m.contains("shadow")
        || m.contains("[dummy]")
        || m.contains("_dummy")
        || m.contains("invisible")
        // Only the two traced landscape families are visible fog surfaces.
        || (m.contains("fog") && !m.contains("groundfog") && !m.contains("cloudfog"))
}

pub struct BuiltPiece {
    pub glb: Vec<u8>,
    pub triangles: usize,
    pub primitives: usize,
    pub bbox_min: [f32; 3],
    pub bbox_max: [f32; 3],
}

/// Builds a static GLB for one map piece. `texture_uri` maps a texture name to the URI to
/// reference, or None when the texture is unavailable (the material then has no texture).
pub fn build(
    flver: &flver::Flver,
    mtds: &Mtds,
    texture_uri: &dyn Fn(&str) -> Option<String>,
) -> Result<BuiltPiece> {
    let mut g = Glb::default();
    let mut levels: [Vec<Value>; 4] = std::array::from_fn(|_| Vec::new());
    let mut auxiliary = Vec::new();
    let mut texture_ids: HashMap<String, usize> = HashMap::new();
    let mut material_ids: HashMap<(usize, bool), usize> = HashMap::new();
    let mut triangles = 0;
    let mut bmin = Vec3::splat(f32::MAX);
    let mut bmax = Vec3::splat(f32::MIN);
    let mut faces: Vec<_> = flver
        .meshes
        .iter()
        .enumerate()
        .flat_map(|(mi, mesh)| {
            mesh.face_sets
                .iter()
                .enumerate()
                .map(move |(fi, face)| (mi, mesh, fi, face))
        })
        .collect();
    // Existing consumers use the legacy material prefix; append alternate variants after it.
    faces.sort_by_key(|(_, _, _, face)| face.flags != 0);
    // Unweighted vertices live in their node's frame; place them as the game draws them.
    let world = flver::world_matrices(&flver.nodes);
    let mut model_space: Vec<Option<std::borrow::Cow<'_, flver::Vertices>>> =
        vec![None; flver.meshes.len()];
    for (mi, mesh, fi, face) in faces {
        let mut indices = face.triangles(mesh.vertices.len());
        if indices.is_empty() {
            continue;
        }
        if indices.iter().any(|&i| i as usize >= mesh.vertices.len()) {
            bail!("mesh {mi} index out of range");
        }
        let material = &flver.materials[mesh.material];
        let mtd = mtds.get(&material.mtd);
        let choice = choose_material(material, mtd);
        if skip_material(&choice.mtd) {
            continue;
        }
        // Same vertex count as `mesh.vertices`, transformed once per mesh on first use.
        let vertices: &flver::Vertices =
            model_space[mi].get_or_insert_with(|| mesh.model_space_vertices(&world));
        let compact = (face.detail_level() > 0).then(|| compact_vertices(vertices, &mut indices));
        let v = compact.as_ref().unwrap_or(vertices);
        let count = v.len();
        if count == 0 {
            continue;
        }
        let positions: Vec<[f32; 3]> = v.positions.iter().map(|p| [-p[0], p[1], p[2]]).collect();
        for p in &positions {
            bmin = bmin.min(Vec3::from(*p));
            bmax = bmax.max(Vec3::from(*p));
        }
        let mut attributes = json!({ "POSITION": g.vec3(&positions, true) });
        if choice.layers.is_some() {
            let mountain = choice.layers.as_ref().is_some_and(|l| l.kind == 3);
            let weights: Vec<[f32; 4]> = (0..count)
                .map(|i| {
                    if mountain {
                        // Mountain's native vertex shader forwards COLOR1.x, not blend bytes.
                        return [
                            v.colors.first().and_then(|c| c.get(i)).map_or(0., |c| c[0]),
                            0.,
                            0.,
                            0.,
                        ];
                    }
                    let byte = |stream: usize, channel: usize| {
                        v.blends
                            .get(stream)
                            .and_then(|b| b.get(i))
                            .map_or(0., |b| b[channel] as f32 / 255.)
                    };
                    let ground = choice.layers.as_ref().is_some_and(|l| l.kind == 2);
                    [
                        byte(0, 0),
                        byte(1, if ground { 1 } else { 2 }),
                        byte(4, 1),
                        byte(5, 2),
                    ]
                })
                .collect();
            attributes["COLOR_0"] = json!(g.vec4(&weights));
            let blend2: Vec<[f32; 4]> = (0..count)
                .map(|i| {
                    let tint = v
                        .blends
                        .get(2)
                        .and_then(|b| b.get(i))
                        .copied()
                        .unwrap_or([255; 4]);
                    let wet = v.blends.get(3).and_then(|b| b.get(i)).map_or(0, |b| b[0]);
                    [
                        tint[0] as f32 / 255.,
                        tint[1] as f32 / 255.,
                        tint[2] as f32 / 255.,
                        wet as f32 / 255.,
                    ]
                })
                .collect();
            attributes["_MAP_BLEND2"] = json!(g.vec4(&blend2));
            if let Some(colors) = v.colors.first()
                && colors.len() == count
            {
                attributes["_MAP_COLOR1"] = json!(g.vec4(colors));
            }
        } else if let Some(fog) = &choice.fog_surface {
            // Native TEXCOORD1.x is coverage; COLOR1.x is baked occlusion.
            // CloudFog also forwards TEXCOORD1.yz, so retain the whole stream.
            let coverage: Vec<_> = (0..count)
                .map(|i| {
                    let data = v
                        .blends
                        .first()
                        .and_then(|b| b.get(i))
                        .copied()
                        .unwrap_or([255; 4]);
                    let occlusion = v.colors.first().and_then(|c| c.get(i)).map_or(1., |c| c[0]);
                    if fog.family == "CloudFog" {
                        [
                            data[0] as f32 / 255.,
                            data[1] as f32 / 255.,
                            data[2] as f32 / 255.,
                            occlusion,
                        ]
                    } else {
                        [data[0] as f32 / 255., 0., 0., occlusion]
                    }
                })
                .collect();
            attributes["COLOR_0"] = json!(g.vec4(&coverage));
        } else if choice.alpha_mode.is_some()
            && let Some(colors) = v.colors.first()
            && colors.len() == count
        {
            // Mesh decals feather coverage in vertex alpha even when their DDS is opaque.
            // RGB here is shader data (often constant red), not a StandardMaterial tint.
            let opacity: Vec<_> = colors.iter().map(|color| [1., 1., 1., color[3]]).collect();
            attributes["COLOR_0"] = json!(g.vec4(&opacity));
        }
        if v.normals.len() == count {
            let normals: Vec<[f32; 3]> = v
                .normals
                .iter()
                .map(|n| {
                    Vec3::new(-n[0], n[1], n[2])
                        .try_normalize()
                        .unwrap_or(Vec3::Y)
                        .into()
                })
                .collect();
            attributes["NORMAL"] = json!(g.vec3(&normals, false));
            // Sekiro's semantic-6 vector is the UV bitangent, not glTF's U tangent.
            // Mirror it with the normal, then reconstruct U using the mirrored handedness.
            // Exporting the stored vector directly rotates normal-map detail by 90 degrees.
            if (choice.normal.is_some() || choice.layers.is_some() || choice.fog_surface.is_some())
                && let Some(t) = v.tangents.first()
                && t.len() == count
            {
                let tangents: Vec<[f32; 4]> = t
                    .iter()
                    .zip(&normals)
                    .map(|(t, n)| {
                        let n = Vec3::from(*n);
                        let bitangent = Vec3::new(-t[0], t[1], t[2]);
                        let w = if t[3] < 0.0 { 1.0 } else { -1.0 };
                        let t3 = (n.cross(bitangent) * w)
                            .try_normalize()
                            .unwrap_or_else(|| n.any_orthonormal_vector());
                        [t3.x, t3.y, t3.z, w]
                    })
                    .collect();
                attributes["TANGENT"] = json!(g.vec4(&tangents));
            }
        }
        for (c, uv) in v.uvs.iter().take(2).enumerate() {
            if uv.len() == count {
                attributes[format!("TEXCOORD_{c}")] = json!(g.vec2(uv));
            }
        }
        let uv_ok = |uv: usize| v.uvs.get(uv).is_some_and(|u| u.len() == count) && uv < 2;

        let mut pbr = json!({ "metallicFactor": 0.0, "roughnessFactor": 0.85 });
        let mut texture = |r: &TexRef, g: &mut Glb| -> Option<(usize, usize)> {
            let name = texture_stem(&r.path).to_ascii_lowercase();
            let id = match texture_ids.get(&name) {
                Some(&t) => t,
                None => {
                    let uri = texture_uri(&name)?;
                    let t = g.texture_uri(&uri, &name);
                    texture_ids.insert(name, t);
                    t
                }
            };
            Some((id, if uv_ok(r.uv) { r.uv } else { 0 }))
        };
        if let Some(r) = &choice.albedo
            && let Some((t, uv)) = texture(r, &mut g)
        {
            pbr["baseColorTexture"] = json!({ "index": t, "texCoord": uv });
        } else {
            // No texture: a neutral stone grey keeps the shape readable.
            pbr["baseColorFactor"] = json!([0.45, 0.43, 0.40, 1.0]);
        }
        let mut mat = json!({
            "name": format!("m{mi} {} | {}", material.name, choice.mtd),
            "pbrMetallicRoughness": pbr,
            "doubleSided": choice.double_sided,
        });
        if let Some(fog) = &choice.fog_surface {
            let textures: Vec<_> = fog
                .textures
                .iter()
                .map(|(slot, r)| {
                    json!({"slot": slot, "name": texture_stem(&r.path).to_ascii_lowercase(),
                    "uri": texture_uri(&texture_stem(&r.path).to_ascii_lowercase()),
                    "uv": r.uv, "scale": r.scale})
                })
                .collect();
            mat["extras"]["sekiro_fog_surface"] = json!({"version": 1,
                "family": fog.family, "params": fog.params, "textures": textures});
            mat["pbrMetallicRoughness"]["baseColorFactor"] = json!([1., 1., 1., 1.]);
        }
        if let Some(layers) = &choice.layers {
            let mut extras = json!({"version": 2, "kind": layers.kind, "variant": layers.variant, "params": layers.params,
                "base_albedo": choice.albedo.is_some()});
            if let Some(r) = &choice.albedo {
                extras["base_uv"] = json!({"uv": r.uv, "scale": r.scale});
            }
            for (key, r) in [
                ("overlay", &layers.overlay),
                ("overlay_normal", &layers.overlay_normal),
                ("snow", &layers.snow),
                ("snow_normal", &layers.snow_normal),
                ("c", &layers.c),
                ("c_normal", &layers.c_normal),
                ("mask", &layers.mask),
                ("base_normal", &layers.base_normal),
                ("detail", &layers.detail),
                ("mask_l", &layers.mask_l),
                ("wet", &layers.wet),
            ] {
                if let Some(r) = r
                    && let Some(uri) = texture_uri(&game_texture_name(
                        r,
                        key.contains("normal") || key == "detail" || key == "wet",
                    ))
                {
                    extras[key] =
                        json!({"uri": uri, "uv": if uv_ok(r.uv) {r.uv} else {0}, "scale": r.scale});
                }
            }
            mat["extras"] = json!({"sekiro_layers": extras});
        }
        if choice.layers.is_none()
            && let Some(r) = &choice.normal
            && let Some((t, uv)) = texture(r, &mut g)
        {
            mat["normalTexture"] = json!({ "index": t, "texCoord": uv });
        }
        if let Some((mode, cutoff)) = choice.alpha_mode {
            mat["alphaMode"] = json!(mode);
            if mode == "MASK" {
                mat["alphaCutoff"] = json!(cutoff);
            }
        }
        {
            let level = face.detail_level();
            let double_sided = !face.cull_backfaces || choice.double_sided;
            mat["doubleSided"] = json!(double_sided);
            let material_id = *material_ids.entry((mi, double_sided)).or_insert_with(|| {
                g.materials.push(mat);
                g.materials.len() - 1
            });
            let primitive = json!({
                "attributes": attributes,
                "indices": g.indices(&indices),
                "material": material_id,
                "extras": {"sekiro_lod": level, "face_flags": face.flags,
                    "source_mesh": mi, "source_face_set": fi, "source_node": mesh.node,
                "source_material": mesh.material,
                    "cull_backfaces": face.cull_backfaces},
            });
            if face.flags & flver::face_set_flags::MOTION_BLUR != 0 {
                auxiliary.push(primitive);
            } else {
                triangles += indices.len() / 3;
                levels[level].push(primitive);
            }
        }
    }
    let count = levels.iter().map(Vec::len).sum();
    let mut scenes = Vec::new();
    for (level, primitives) in levels.into_iter().enumerate() {
        let mut roots = Vec::new();
        if !primitives.is_empty() {
            let mesh = g.meshes.len();
            g.meshes
                .push(json!({"name": format!("LOD{level}"), "primitives": primitives}));
            roots.push(g.nodes.len());
            g.nodes.push(json!({"mesh": mesh}));
        }
        scenes.push(
            json!({"name": format!("LOD{level}"), "nodes": roots, "extras": {"sekiro_lod": level}}),
        );
    }
    if !auxiliary.is_empty() {
        g.meshes
            .push(json!({"name": "MotionBlur", "primitives": auxiliary}));
    }
    let extras: Value = json!({
        "source": "sekiro-extract map",
        "convention": "FromSoftware x mirrored (x -> -x); see docs/FORMATS.md",
        "sekiro_model_bounds": {
            "min": [-flver.bbox_max[0], flver.bbox_min[1], flver.bbox_min[2]],
            "max": [-flver.bbox_min[0], flver.bbox_max[1], flver.bbox_max[2]],
        },
    });
    Ok(BuiltPiece {
        glb: detail_scenes(g.finish(&[], extras), scenes)?,
        triangles,
        primitives: count,
        bbox_min: bmin.to_array(),
        bbox_max: bmax.to_array(),
    })
}

/// Lower-detail faces usually reference only part of the shared FLVER vertex buffer.
/// Remap every vertex stream together, retaining authored values and triangle order.
fn compact_vertices(v: &flver::Vertices, indices: &mut [u32]) -> flver::Vertices {
    let mut used = vec![false; v.len()];
    for &index in indices.iter() {
        used[index as usize] = true;
    }
    let selected: Vec<_> = used
        .iter()
        .enumerate()
        .filter_map(|(i, &used)| used.then_some(i))
        .collect();
    let mut remap = vec![0; v.len()];
    for (new, &old) in selected.iter().enumerate() {
        remap[old] = new as u32;
    }
    for index in indices {
        *index = remap[*index as usize];
    }
    fn stream<T: Clone>(data: &[T], selected: &[usize], count: usize) -> Vec<T> {
        if data.len() == count {
            selected.iter().map(|&i| data[i].clone()).collect()
        } else {
            Vec::new()
        }
    }
    let streams =
        |data: &[Vec<[f32; 3]>]| data.iter().map(|s| stream(s, &selected, v.len())).collect();
    flver::Vertices {
        positions: stream(&v.positions, &selected, v.len()),
        extra_positions: streams(&v.extra_positions),
        normals: stream(&v.normals, &selected, v.len()),
        extra_normals: streams(&v.extra_normals),
        normal_w: stream(&v.normal_w, &selected, v.len()),
        tangents: v
            .tangents
            .iter()
            .map(|s| stream(s, &selected, v.len()))
            .collect(),
        bitangents: stream(&v.bitangents, &selected, v.len()),
        uvs: v
            .uvs
            .iter()
            .map(|s| stream(s, &selected, v.len()))
            .collect(),
        blends: v
            .blends
            .iter()
            .map(|s| stream(s, &selected, v.len()))
            .collect(),
        colors: v
            .colors
            .iter()
            .map(|s| stream(s, &selected, v.len()))
            .collect(),
        bone_weights: stream(&v.bone_weights, &selected, v.len()),
        bone_indices: stream(&v.bone_indices, &selected, v.len()),
    }
}

// The shared character writer has one scene; keep its interface unchanged.
fn detail_scenes(glb: Vec<u8>, scenes: Vec<Value>) -> Result<Vec<u8>> {
    let json_len = u32::from_le_bytes(glb[12..16].try_into().unwrap()) as usize;
    let mut doc: Value = serde_json::from_slice(&glb[20..20 + json_len])?;
    let source = doc["scenes"][0]["extras"].clone();
    doc["scenes"] = json!(scenes);
    doc["extras"] = source;
    doc["extras"]["sekiro_lod_version"] = json!(1);
    let mut bytes = serde_json::to_vec(&doc)?;
    while !bytes.len().is_multiple_of(4) {
        bytes.push(b' ');
    }
    let bin = &glb[20 + json_len..];
    let total = 20 + bytes.len() + bin.len();
    let mut out = glb[..12].to_vec();
    out[8..12].copy_from_slice(&(total as u32).to_le_bytes());
    out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    out.extend_from_slice(b"JSON");
    out.extend_from_slice(&bytes);
    out.extend_from_slice(bin);
    Ok(out)
}

/// Texture names (lower case) a FLVER's visible materials would use, and whether each is a
/// normal map.
pub fn wanted_textures(flver: &flver::Flver, mtds: &Mtds) -> Vec<(String, bool)> {
    let mut out = Vec::new();
    for mesh in &flver.meshes {
        let material = &flver.materials[mesh.material];
        let choice = choose_material(material, mtds.get(&material.mtd));
        if skip_material(&choice.mtd) {
            continue;
        }
        if let Some(fog) = &choice.fog_surface {
            out.extend(
                fog.textures
                    .iter()
                    .map(|(_, r)| (texture_stem(&r.path).to_ascii_lowercase(), false)),
            );
        }
        if let Some(r) = choice.albedo {
            out.push((texture_stem(&r.path).to_ascii_lowercase(), false));
        }
        if choice.layers.is_none()
            && let Some(r) = choice.normal
        {
            out.push((texture_stem(&r.path).to_ascii_lowercase(), true));
        }
        if let Some(l) = choice.layers {
            for (r, normal) in [
                (l.overlay, false),
                (l.overlay_normal, true),
                (l.snow, false),
                (l.snow_normal, true),
                (l.c, false),
                (l.c_normal, true),
                (l.mask, false),
                (l.base_normal, true),
                (l.detail, true),
                (l.mask_l, false),
                (l.wet, true),
            ] {
                if let Some(r) = r {
                    out.push((game_texture_name(&r, normal), false));
                }
            }
        }
    }
    out
}

fn game_texture_name(r: &TexRef, normal: bool) -> String {
    let name = texture_stem(&r.path).to_ascii_lowercase();
    if normal {
        format!("{name}_map_raw")
    } else {
        name
    }
}

#[cfg(test)]
#[path = "pieces_lod_tests.rs"]
mod lod_tests;

#[cfg(test)]
#[path = "pieces_node_tests.rs"]
mod node_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authored_fog_surfaces_keep_masks_coverage_and_material_inputs() {
        let raw = sekiro_formats::cache::root().join("raw");
        if !raw.exists() {
            return;
        }
        let mtds = super::super::load_mtds(&raw).unwrap();
        for (map, model, family, slots) in [
            ("m11_00_00_00", "m900100", "GroundFog", vec![0, 2, 3, 4]),
            ("m11_00_00_00", "m900101", "GroundFog", vec![0, 2, 3, 4]),
            ("m11_00_00_00", "m911008", "CloudFog", vec![1, 7]),
            ("m11_00_00_00", "m911009", "CloudFog", vec![1, 7]),
            ("m11_00_00_00", "m925009", "CloudFog", vec![1, 7]),
            ("m11_01_00_00", "m920009", "CloudFog", vec![1, 7]),
        ] {
            let path = super::super::piece_flver(&raw, map, model);
            let f = flver::parse(&std::fs::read(path).unwrap()).unwrap();
            let built = build(&f, &mtds, &|name| Some(format!("tex/{name}.dds"))).unwrap();
            assert!(built.primitives > 0);
            let n = u32::from_le_bytes(built.glb[12..16].try_into().unwrap()) as usize;
            let doc: Value = serde_json::from_slice(&built.glb[20..20 + n]).unwrap();
            let expected: usize = f
                .meshes
                .iter()
                .filter(|mesh| !skip_material(&f.materials[mesh.material].mtd))
                .map(|mesh| {
                    mesh.face_sets
                        .iter()
                        .filter(|face| face.flags & flver::face_set_flags::MOTION_BLUR == 0)
                        .count()
                })
                .sum();
            assert_eq!(built.primitives, expected);
            for material in doc["materials"].as_array().unwrap() {
                let fog = &material["extras"]["sekiro_fog_surface"];
                if fog.is_null() {
                    continue;
                }
                assert_eq!(fog["family"], family);
                assert_eq!(material["alphaMode"], "BLEND");
                assert_eq!(
                    fog["textures"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|t| t["slot"].as_u64().unwrap())
                        .collect::<Vec<_>>(),
                    slots
                );
                assert!(
                    fog["textures"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .all(|t| t["uri"].is_string())
                );
                let native = mtds.get(&f.materials[0].mtd).unwrap();
                assert_eq!(
                    fog["params"].as_object().unwrap().len(),
                    native.params.len()
                );
            }
            for mesh in doc["meshes"].as_array().unwrap() {
                for primitive in mesh["primitives"].as_array().unwrap() {
                    let attributes = &primitive["attributes"];
                    let source = primitive["extras"]["source_mesh"].as_u64().unwrap() as usize;
                    let vertices = &f.meshes[source].vertices;
                    if choose_material(
                        &f.materials[f.meshes[source].material],
                        mtds.get(&f.materials[f.meshes[source].material].mtd),
                    )
                    .fog_surface
                    .is_none()
                    {
                        continue;
                    }
                    assert!(attributes["TEXCOORD_0"].is_number());
                    assert!(attributes["COLOR_0"].is_number());
                    eprintln!(
                        "{map} {model} mesh={source} bounds={:?}/{:?} blends={:?} colors={:?}",
                        built.bbox_min,
                        built.bbox_max,
                        vertices
                            .blends
                            .iter()
                            .map(|b| b.first())
                            .collect::<Vec<_>>(),
                        vertices
                            .colors
                            .iter()
                            .map(|c| c.first())
                            .collect::<Vec<_>>()
                    );
                    let accessor =
                        &doc["accessors"][attributes["COLOR_0"].as_u64().unwrap() as usize];
                    let view =
                        &doc["bufferViews"][accessor["bufferView"].as_u64().unwrap() as usize];
                    let offset = 20 + n + 8 + view["byteOffset"].as_u64().unwrap() as usize;
                    let first =
                        f32::from_le_bytes(built.glb[offset..offset + 4].try_into().unwrap());
                    assert_eq!(first, vertices.blends[0][0][0] as f32 / 255.);
                }
            }
            let wanted = wanted_textures(&f, &mtds);
            for material in &f.materials {
                if let Some(fog) = choose_material(material, mtds.get(&material.mtd)).fog_surface {
                    for (_, texture) in fog.textures {
                        assert!(
                            wanted.contains(&(
                                texture_stem(&texture.path).to_ascii_lowercase(),
                                false
                            ))
                        );
                    }
                }
            }
        }
        assert!(skip_material("M_FogWall_Dummy"));
    }

    #[test]
    fn mismatched_foliage_sampler_names_use_primary_mtd_albedo_and_cutout() {
        let mat = flver::Material {
            name: "leaves".into(),
            mtd: "leaves.mtd".into(),
            index: 0,
            textures: vec![flver::Texture {
                param: "Character_Wind_Texture2D_2_AlbedoMap".into(),
                path: String::new(),
                scale: [1., 1.],
            }],
        };
        let mtd = mtd::Mtd {
            shader: "Character[Gradient]".into(),
            description: String::new(),
            params: vec![mtd::Param {
                name: "g_AlphaRef".into(),
                kind: "int".into(),
                value: mtd::ParamValue::Ints(vec![128]),
            }],
            textures: vec![mtd::Texture {
                kind: "Character_Gradient_Texture2D_1_AlbedoMap_0".into(),
                uv_number: 1,
                shader_data_index: 0,
                path: "leaves_a.tif".into(),
            }],
        };
        let choice = choose_material(&mat, Some(&mtd));
        assert_eq!(choice.albedo.unwrap().path, "leaves_a.tif");
        assert_eq!(choice.alpha_mode.unwrap().0, "MASK");
        assert!(choice.double_sided);
    }

    #[test]
    fn multiple_materials_choose_base_slot_before_snow() {
        let mat = flver::Material {
            name: "snow_a".into(),
            mtd: "multiple.mtd".into(),
            index: 0,
            textures: vec![],
        };
        let mtd = mtd::Mtd {
            shader: "M[Multiple].spx".into(),
            description: String::new(),
            params: vec![],
            textures: [(12, "snow_a.tif"), (8, "stone_a.tif"), (5, "moss_a.tif")]
                .into_iter()
                .map(|(slot, path)| mtd::Texture {
                    kind: format!("M_Multiple__snp_Texture2D_{slot}_AlbedoMap_0"),
                    uv_number: 1,
                    shader_data_index: 0,
                    path: path.into(),
                })
                .collect(),
        };
        let choice = choose_material(&mat, Some(&mtd));
        assert_eq!(choice.albedo.unwrap().path, "stone_a.tif");
        let layers = choice.layers.unwrap();
        assert_eq!(layers.snow.unwrap().path, "snow_a.tif");
        assert_eq!(layers.overlay.unwrap().path, "moss_a.tif");
    }

    #[test]
    fn static_switch_routes_detail_layer_masks_and_ground_layer_l() {
        for (shader, switch, expected) in [
            ("M[Multiple].spx", 1, (false, false, true, true, 0)),
            ("M[MultipleGround].spx", 24, (true, false, true, false, 1)),
            ("M[MultipleGround].spx", 27, (true, true, true, false, 1)),
        ] {
            let mat = flver::Material {
                name: "rock".into(),
                mtd: "rock.mtd".into(),
                index: 0,
                textures: vec![flver::Texture {
                    param: "M_Texture2D_8_AlbedoMap_0".into(),
                    path: "override_a.tif".into(),
                    scale: [1., 1.],
                }],
            };
            let slots = [
                (0, "NormalMap"),
                (1, "AlbedoMap"),
                (3, "NormalMap"),
                (4, "NormalMap"),
                (5, "AlbedoMap"),
                (
                    6,
                    if shader.contains("Ground") {
                        "NormalMap"
                    } else {
                        "Mask3Map"
                    },
                ),
                (
                    7,
                    if shader.contains("Ground") {
                        "NormalMap"
                    } else {
                        "AlbedoMap"
                    },
                ),
                (8, "AlbedoMap"),
                (9, "Mask3Map"),
                (11, "AlbedoMap"),
                (12, "NormalMap"),
            ];
            let mtd = mtd::Mtd {
                shader: shader.into(),
                description: String::new(),
                params: vec![mtd::Param {
                    name: "GXFT_SATStaticSwitch".into(),
                    kind: "int".into(),
                    value: mtd::ParamValue::Ints(vec![switch]),
                }],
                textures: slots
                    .into_iter()
                    .map(|(slot, semantic)| mtd::Texture {
                        kind: format!("M_Texture2D_{slot}_{semantic}_0"),
                        uv_number: 2,
                        shader_data_index: 0,
                        path: format!("tex_{slot}.tif"),
                    })
                    .collect(),
            };
            let choice = choose_material(&mat, Some(&mtd));
            assert_eq!(choice.albedo.unwrap().path, "override_a.tif");
            let l = choice.layers.unwrap();
            assert_eq!(
                (
                    l.overlay.is_some(),
                    l.snow.is_some(),
                    l.c.is_some(),
                    l.mask_l.is_some(),
                    u8::from(l.c_normal.is_some())
                ),
                expected
            );
            assert!(l.detail.is_some());
            assert_eq!(l.base_normal.unwrap().path, "tex_3.tif");
            assert_eq!(l.c.unwrap().uv, 1);
        }
    }

    #[test]
    fn cached_valley_alpha_materials_resolve_real_textures() {
        let raw = sekiro_formats::cache::root().join("raw");
        if !raw.is_dir() {
            return;
        }
        let mtds = super::super::load_mtds(&raw).unwrap();
        for name in ["M[m11_00]_Log1.mtd", "M[m11_00]_Rock4.mtd"] {
            if let Some(m) = mtds.0.get(&name.to_ascii_lowercase()) {
                println!("LAYER VARIANT {name} {}", m.shader);
                for t in &m.textures {
                    println!("VARIANT SLOT {} {}", t.kind, t.path);
                }
            }
        }
        for model in ["409110", "000064", "101100"] {
            let path = raw.join(format!(
                "map/m17_00_00_00/m17_00_00_00_{model}.mapbnd.d/m17_00_00_00_{model}.flver"
            ));
            if !path.is_file() {
                continue;
            }
            let f = flver::parse(&std::fs::read(path).unwrap()).unwrap();
            if model == "101100" {
                let built = build(&f, &mtds, &|_| None).unwrap();
                let len = u32::from_le_bytes(built.glb[12..16].try_into().unwrap()) as usize;
                let doc: Value = serde_json::from_slice(&built.glb[20..20 + len]).unwrap();
                let bin = &built.glb[28 + len..];
                let mut feathered = false;
                for primitive in doc["meshes"][0]["primitives"].as_array().unwrap() {
                    let material =
                        &doc["materials"][primitive["material"].as_u64().unwrap() as usize];
                    if material["alphaMode"] != "BLEND" {
                        continue;
                    }
                    let accessor = &doc["accessors"]
                        [primitive["attributes"]["COLOR_0"].as_u64().unwrap() as usize];
                    let view =
                        &doc["bufferViews"][accessor["bufferView"].as_u64().unwrap() as usize];
                    let offset = view["byteOffset"].as_u64().unwrap() as usize;
                    let count = accessor["count"].as_u64().unwrap() as usize;
                    for vertex in bin[offset..offset + count * 16].as_chunks::<16>().0 {
                        let rgba: Vec<_> = vertex
                            .as_chunks::<4>()
                            .0
                            .iter()
                            .map(|f| f32::from_le_bytes(*f))
                            .collect();
                        assert_eq!(&rgba[..3], &[1., 1., 1.]);
                        feathered |= rgba[3] < 1.;
                    }
                }
                assert!(feathered, "snow/ground decals must retain vertex opacity");
            }
            for mat in &f.materials {
                let mtd = mtds.get(&mat.mtd);
                let choice = choose_material(mat, mtd);
                if choice.alpha_mode.is_some() || choice.mtd.contains("Snow") {
                    assert!(choice.albedo.is_some());
                    if choice.alpha_mode.is_some_and(|a| a.0 == "MASK") {
                        assert!(choice.double_sided);
                    }
                    println!(
                        "VALLEY {} {} albedo={:?} alpha={:?} shader={:?}",
                        model,
                        choice.mtd,
                        choice.albedo,
                        choice.alpha_mode,
                        mtd.map(|m| &m.shader)
                    );
                    for t in &mtd.unwrap().textures {
                        println!("SLOT {} {}", t.kind, t.path);
                    }
                    for mesh in f
                        .meshes
                        .iter()
                        .filter(|mesh| f.materials[mesh.material].name == mat.name)
                    {
                        for colors in &mesh.vertices.colors {
                            println!(
                                "VERTEX COLOR {:?}..{:?}",
                                (0..4)
                                    .map(|c| colors
                                        .iter()
                                        .map(|v| v[c])
                                        .fold(f32::INFINITY, f32::min))
                                    .collect::<Vec<_>>(),
                                (0..4)
                                    .map(|c| colors
                                        .iter()
                                        .map(|v| v[c])
                                        .fold(f32::NEG_INFINITY, f32::max))
                                    .collect::<Vec<_>>()
                            );
                        }
                    }
                    if let Some(a) = choice.albedo {
                        let path = raw.parent().unwrap().join(format!(
                            "maps/m17_00_00_00/models/tex/{}.dds",
                            texture_stem(&a.path).to_ascii_lowercase()
                        ));
                        if path.is_file() {
                            let dds = std::fs::read(path).unwrap();
                            let h = sekiro_formats::tpf::Dds::parse(&dds).unwrap();
                            let rgba = crate::models::textures::decode_rgba(&h, &dds).unwrap();
                            println!(
                                "ALPHA {:?}: transparent={}/{}",
                                h.format,
                                rgba.as_chunks::<4>()
                                    .0
                                    .iter()
                                    .filter(|p| p[3] < 128)
                                    .count(),
                                rgba.len() / 4
                            );
                        }
                    }
                }
            }
        }
    }
}
