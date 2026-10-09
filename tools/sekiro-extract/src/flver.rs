//! FLVER2 models (Sekiro version 0x2001A) and TPF texture packs, reduced to
//! what the game needs: bind-pose nodes, LOD0 triangle meshes with skin
//! weights, and each material's albedo texture. Layout follows SoulsFormats'
//! FLVER2 / TPF readers.

use crate::bin::Reader;

pub struct Node {
    pub name: String,
    pub parent: i16,
    pub t: [f32; 3],
    /// Euler radians, applied X then Z then Y (Node.ComputeLocalTransform).
    pub r: [f32; 3],
    pub s: [f32; 3],
}

pub struct Vertex {
    pub pos: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub bones: [u16; 4],
    pub weights: [f32; 4],
}

pub struct Mesh {
    pub material: String,
    /// MTD file stem, e.g. "c1020_armor" (Sekiro FLVERs leave texture paths empty).
    pub mtd: String,
    pub albedo: String,
    /// Normal-map texture stem paired with the albedo (empty when the TPF has none).
    pub normal_map: String,
    /// Metallic mask (MTD MetallicMap, BC4; empty when the material has none).
    pub metallic: String,
    /// The albedo's alpha is a cutout (alpha test); otherwise the material is opaque and the alpha
    /// is a blend mask (DetailBlend / Blend shaders) or unused.
    pub alpha_test: bool,
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    /// FLVER node the mesh belongs to (its name = the Havok cloth display buffer name).
    pub node: i32,
    /// The material's texture slots: (MTD sampler name, texture stem; empty when unset).
    pub textures: Vec<(String, String)>,
}

pub struct Dummy {
    pub id: i16,
    /// Bone whose bind pose the position is expressed in (-1 = model space).
    pub parent: i16,
    /// Bone the dummy follows at runtime.
    pub attach: i16,
    pub pos: [f32; 3],
    /// Forward / upward vectors (model space).
    pub fwd: [f32; 3],
    pub up: [f32; 3],
}

pub struct Flver {
    pub nodes: Vec<Node>,
    pub meshes: Vec<Mesh>,
    pub dummies: Vec<Dummy>,
}

fn utf16(r: &Reader, o: usize) -> String {
    r.utf16z(o)
}

fn stem(path: &str) -> String {
    path.rsplit(['\\', '/']).next().unwrap_or("").split('.').next().unwrap_or("").to_lowercase()
}

pub fn read(d: &[u8]) -> Flver {
    let r = Reader::new(d);
    assert_eq!(&d[0..6], b"FLVER\0", "not a FLVER");
    let version = r.u32(0x08);
    let data_offset = r.u32(0x0C) as usize;
    let dummy_count = r.u32(0x14) as usize;
    let material_count = r.u32(0x18) as usize;
    let bone_count = r.u32(0x1C) as usize;
    let mesh_count = r.u32(0x20) as usize;
    let vb_count = r.u32(0x24) as usize;
    let header_index_size = d[0x48] as usize;
    let unicode = d[0x49] != 0;
    let faceset_count = r.u32(0x50) as usize;
    let layout_count = r.u32(0x54) as usize;
    let texture_count = r.u32(0x58) as usize;
    let s = |o: usize| if unicode { utf16(&r, o) } else { r.cstr(o) };
    let uv_factor = if version >= 0x2000E { 2048.0 } else { 1024.0 };

    let dummies: Vec<Dummy> = (0..dummy_count)
        .map(|i| {
            let b = 0x80 + i * 0x40;
            Dummy {
                pos: [r.f32(b), r.f32(b + 4), r.f32(b + 8)],
                fwd: [r.f32(b + 0x10), r.f32(b + 0x14), r.f32(b + 0x18)],
                up: [r.f32(b + 0x20), r.f32(b + 0x24), r.f32(b + 0x28)],
                id: r.u16(b + 28) as i16,
                parent: r.u16(b + 30) as i16,
                attach: r.u16(b + 44) as i16,
            }
        })
        .collect();
    let mut o = 0x80 + dummy_count * 0x40;
    // Materials: name, mtd, textureCount, textureIndex, ..., 0x20 bytes each.
    let materials: Vec<(String, usize, usize, String)> = (0..material_count)
        .map(|i| {
            let m = o + i * 0x20;
            (s(r.u32(m) as usize), r.u32(m + 8) as usize, r.u32(m + 12) as usize, stem(&s(r.u32(m + 4) as usize)))
        })
        .collect();
    o += material_count * 0x20;
    let nodes: Vec<Node> = (0..bone_count)
        .map(|i| {
            let b = o + i * 0x80;
            Node {
                name: s(r.u32(b + 12) as usize),
                t: [r.f32(b), r.f32(b + 4), r.f32(b + 8)],
                r: [r.f32(b + 16), r.f32(b + 20), r.f32(b + 24)],
                parent: r.u16(b + 28) as i16,
                s: [r.f32(b + 32), r.f32(b + 36), r.f32(b + 40)],
            }
        })
        .collect();
    o += bone_count * 0x80;
    struct MeshHdr {
        use_weights: bool,
        material: usize,
        node: i32,
        bone_indices: Vec<i32>,
        facesets: Vec<usize>,
        vbs: Vec<usize>,
    }
    let mesh_hdrs: Vec<MeshHdr> = (0..mesh_count)
        .map(|i| {
            let m = o + i * 0x30;
            let list = |count: usize, off: usize| (0..count).map(|k| r.i32(off + k * 4)).collect::<Vec<i32>>();
            MeshHdr {
                use_weights: d[m] != 0,
                material: r.u32(m + 4) as usize,
                node: r.i32(m + 16),
                bone_indices: list(r.u32(m + 20) as usize, r.u32(m + 28) as usize),
                facesets: list(r.u32(m + 32) as usize, r.u32(m + 36) as usize).into_iter().map(|v| v as usize).collect(),
                vbs: list(r.u32(m + 40) as usize, r.u32(m + 44) as usize).into_iter().map(|v| v as usize).collect(),
            }
        })
        .collect();
    o += mesh_count * 0x30;
    // Face sets: flags, strip, cull, unk, count, offset, len, 0, indexSize, 0 (0x20 bytes).
    struct FaceSet {
        flags: u32,
        strip: bool,
        indices: Vec<u32>,
    }
    let facesets: Vec<FaceSet> = (0..faceset_count)
        .map(|i| {
            let f = o + i * 0x20;
            let count = r.u32(f + 8) as usize;
            let off = data_offset + r.u32(f + 12) as usize;
            let mut size = r.u32(f + 24) as usize;
            if size == 0 {
                size = header_index_size;
            }
            let indices = match size {
                16 => (0..count).map(|k| r.u16(off + k * 2) as u32).collect(),
                32 => (0..count).map(|k| r.u32(off + k * 4)).collect(),
                _ => Vec::new(), // edge-compressed (8-bit) sets are console-only
            };
            FaceSet { flags: r.u32(f), strip: d[f + 4] != 0, indices }
        })
        .collect();
    o += faceset_count * 0x20;
    // Vertex buffers: bufferIndex, layoutIndex, vertexSize, vertexCount, 0, 0, length, offset.
    let vbs: Vec<(usize, usize, usize, usize)> = (0..vb_count)
        .map(|i| {
            let v = o + i * 0x20;
            (r.u32(v + 4) as usize, r.u32(v + 8) as usize, r.u32(v + 12) as usize, data_offset + r.u32(v + 28) as usize)
        })
        .collect();
    o += vb_count * 0x20;
    // Layouts: memberCount, 0, 0, memberOffset; members: stream, structOffset, type, semantic, index.
    let layouts: Vec<Vec<(u32, u32, u32)>> = (0..layout_count)
        .map(|i| {
            let l = o + i * 0x10;
            let n = r.u32(l) as usize;
            let mo = r.u32(l + 12) as usize;
            (0..n).map(|k| (r.u32(mo + k * 20 + 4), r.u32(mo + k * 20 + 8), r.u32(mo + k * 20 + 12))).collect()
        })
        .collect();
    o += layout_count * 0x10;
    // Textures: path, paramName, tiling..., 0x20 bytes.
    let textures: Vec<(String, String)> = (0..texture_count)
        .map(|i| {
            let t = o + i * 0x20;
            (s(r.u32(t) as usize), s(r.u32(t + 4) as usize))
        })
        .collect();

    let meshes = mesh_hdrs
        .iter()
        .filter_map(|mh| {
            let (mat_name, tex_count, tex_index, mtd) = materials.get(mh.material)?.clone();
            let albedo = (tex_index..tex_index + tex_count)
                .filter_map(|ti| textures.get(ti))
                .find(|(path, param)| {
                    let p = param.to_lowercase();
                    (p.contains("diffuse") || p.contains("albedo")) && !p.contains("2") && !path.is_empty()
                })
                .or_else(|| (tex_index..tex_index + tex_count).filter_map(|ti| textures.get(ti)).find(|(path, _)| stem(path).ends_with("_a")))
                .map(|(path, _)| stem(path))
                .unwrap_or_default();
            let vcount = mh.vbs.first().map(|&v| vbs[v].2)?;
            let mut vertices: Vec<Vertex> = (0..vcount)
                .map(|_| Vertex { pos: [0.0; 3], normal: [0.0, 1.0, 0.0], uv: [0.0; 2], bones: [0; 4], weights: [1.0, 0.0, 0.0, 0.0] })
                .collect();
            for &vb in &mh.vbs {
                let (layout, vsize, count, start) = vbs[vb];
                let mut first_uv = true;
                for (i, v) in vertices.iter_mut().enumerate().take(count) {
                    let base = start + i * vsize;
                    for &(off, ty, sem) in &layouts[layout] {
                        let p = base + off as usize;
                        match (sem, ty) {
                            (0, 2) | (0, 3) => v.pos = [r.f32(p), r.f32(p + 4), r.f32(p + 8)],
                            (1, 16) => v.weights = std::array::from_fn(|k| d[p + k] as i8 as f32 / 127.0),
                            (1, 19) => v.weights = std::array::from_fn(|k| d[p + k] as f32 / 255.0),
                            (1, 26) => v.weights = std::array::from_fn(|k| r.u16(p + k * 2) as i16 as f32 / 32767.0),
                            (2, 17) | (2, 18) | (2, 47) => v.bones = std::array::from_fn(|k| d[p + k] as u16),
                            (2, 24) => v.bones = std::array::from_fn(|k| r.u16(p + k * 2)),
                            (3, 2) | (3, 3) => v.normal = [r.f32(p), r.f32(p + 4), r.f32(p + 8)],
                            (3, 16) | (3, 17) | (3, 19) | (3, 47) => v.normal = std::array::from_fn(|k| (d[p + k] as f32 - 127.0) / 127.0),
                            (3, 26) => v.normal = std::array::from_fn(|k| r.u16(p + k * 2) as i16 as f32 / 32767.0),
                            (5, _) if first_uv => {
                                v.uv = match ty {
                                    1 => [r.f32(p), r.f32(p + 4)],
                                    19 => [d[p] as f32 / 255.0, d[p + 1] as f32 / 255.0],
                                    _ => [r.u16(p) as i16 as f32 / uv_factor, r.u16(p + 2) as i16 as f32 / uv_factor],
                                };
                                first_uv = false;
                            }
                            _ => {}
                        }
                    }
                    first_uv = true;
                }
            }
            for v in vertices.iter_mut() {
                if !mh.use_weights {
                    // Rigid mesh: bound to its first bone index (or the mesh node).
                    let b = if mh.bone_indices.is_empty() { v.bones[0] } else { v.bones[0] };
                    v.bones = [b, 0, 0, 0];
                    v.weights = [1.0, 0.0, 0.0, 0.0];
                }
                if !mh.bone_indices.is_empty() {
                    v.bones = v.bones.map(|b| mh.bone_indices.get(b as usize).copied().unwrap_or(0).max(0) as u16);
                } else if !mh.use_weights && mh.node >= 0 && v.bones[0] == 0 {
                    v.bones[0] = mh.node as u16;
                }
                let sum: f32 = v.weights.iter().sum();
                if sum > 0.0 {
                    v.weights = v.weights.map(|w| w / sum);
                }
            }
            // LOD0, not motion-blur: flags == 0.
            let fs = mh.facesets.iter().map(|&f| &facesets[f]).find(|f| f.flags == 0).or_else(|| mh.facesets.first().map(|&f| &facesets[f]))?;
            let mut indices = Vec::new();
            if fs.strip {
                for k in 0..fs.indices.len().saturating_sub(2) {
                    let (a, b, c) = (fs.indices[k], fs.indices[k + 1], fs.indices[k + 2]);
                    if a == b || b == c || a == c || a == 0xFFFF || b == 0xFFFF || c == 0xFFFF {
                        continue;
                    }
                    if k % 2 == 0 { indices.extend([a, b, c]) } else { indices.extend([b, a, c]) }
                }
            } else {
                indices = fs.indices.clone();
            }
            let slots = (tex_index..tex_index + tex_count).filter_map(|ti| textures.get(ti)).map(|(path, param)| (param.clone(), stem(path))).collect();
            Some(Mesh { material: mat_name, mtd, albedo, normal_map: String::new(), metallic: String::new(), alpha_test: false, vertices, indices, node: mh.node, textures: slots })
        })
        .collect();
    Flver { nodes, meshes, dummies }
}

/// TPF (PC): name -> DDS bytes.
pub fn tpf(d: &[u8]) -> Vec<(String, Vec<u8>)> {
    let r = Reader::new(d);
    assert_eq!(&d[0..4], b"TPF\0");
    let count = r.u32(8) as usize;
    let encoding = d[0xE];
    let mut o = 0x10;
    let mut out = Vec::new();
    for _ in 0..count {
        let off = r.u32(o) as usize;
        let size = r.u32(o + 4) as usize;
        let name_off = r.u32(o + 12) as usize;
        let float_struct = r.u32(o + 16) == 1;
        o += 20;
        if float_struct {
            let len = r.u32(o + 4) as usize;
            o += 8 + len;
        }
        let name = if encoding == 1 { r.utf16z(name_off) } else { r.cstr(name_off) };
        out.push((name.to_lowercase(), d[off..off + size].to_vec()));
    }
    out
}

/// Picks an albedo texture for a material from its MTD name when the FLVER has no path:
/// the MTD keywords (weapon, armor, kimono, ...) matched against the TPF's "_a" textures.
pub fn guess_albedo(mtd: &str, albedos: &[String]) -> String {
    let m = mtd.to_lowercase();
    // The lashes and the head rope reference the shared FC_A_0000 pack (P_FC_M_0200_Eyelasher
    // -> parts/tex/FC_A_0000_hairface_a.tif, _head_rope -> FC_A_0000_Rope_a.tif), which is not
    // exported: "-" tells the game to skip the mesh.
    if ["eyelasher", "head_rope"].iter().any(|k| m.contains(k)) {
        return "-".to_string();
    }
    // Damage overlays are never the base colour.
    let albedos: Vec<String> = albedos.iter().filter(|a| !a.contains("damage")).cloned().collect();
    let albedos = &albedos[..];
    // Face decals name their texture in the MTD (P_FC_M_0200_Fur.mtd -> FC_M_0200_Fur_a.tif,
    // P_FC_M_0200_Hair_[AO_Hair].mtd -> FC_M_0200_hair01_a.tif, P_FC_M_0200_EyeB_[CatEye].mtd
    // -> FC_M_0200_eye_a.tif, ...), so these keywords pick the right part of the face pack.
    // "eye_" matches eye_a but not eyeshadow_a; the eye/eyeball meshes share it.
    let rules: &[(&str, &str)] = &[
        ("eyeshadow", "eyeshadow"),
        ("eye", "eye_"),
        ("fur", "fur"),
        ("hair", "hair01"),
        ("mouth", "mouth"),
        ("weapon", "weapon"),
        ("_e", "spun_gold"),
        ("helmet", "head"),
        ("head", "head"),
        ("haori", "cloth_02"),
        ("kimono", "cloth_01"),
        ("armor", "body"),
        ("chain", "body"),
        ("acc", "acc"),
        ("body", "body"),
    ];
    for (key, tex) in rules {
        if m.contains(key) {
            if let Some(t) = albedos.iter().find(|a| a.contains(tex)) {
                return t.clone();
            }
        }
    }
    albedos.iter().find(|a| a.contains("body")).or(albedos.first()).cloned().unwrap_or_default()
}

/// The normal map paired with an albedo stem: Sekiro TPFs name them `x_a` / `x_n`, so
/// `c1020_body_01_a` -> `c1020_body_01_n`. Empty when the pair is not in the pack.
pub fn guess_normal(albedo: &str, normals: &[String]) -> String {
    let Some(base) = albedo.to_lowercase().strip_suffix("_a").map(str::to_string) else {
        return String::new();
    };
    let want = format!("{base}_n");
    normals.iter().find(|n| n.to_lowercase() == want).cloned().unwrap_or_default()
}

/// Texture slots of a Sekiro MTD (allmaterialbnd): (sampler name, texture stem, lowercase; empty when
/// the slot has no path), in file order. The MTD - not the FLVER, whose paths are empty - says which
/// texture each sampler of the material's shader reads ("Character_AMSN" = Albedo / Metallic /
/// Shininess / Normal).
pub fn mtd_textures(d: &[u8]) -> Vec<(String, String)> {
    let mut runs = Vec::new();
    let mut cur = Vec::new();
    for &b in d {
        if (0x20..0x7f).contains(&b) {
            cur.push(b);
        } else {
            if cur.len() >= 4 {
                runs.push(String::from_utf8_lossy(&cur).into_owned());
            }
            cur.clear();
        }
    }
    let mut out: Vec<(String, String)> = Vec::new();
    let mut slot: Option<String> = None;
    for s in runs {
        let is_path = s.contains(":\\") || [".tif", ".tga", ".dds", ".psd"].iter().any(|e| s.to_lowercase().ends_with(e));
        if is_path {
            if let Some(sl) = slot.take() {
                out.push((sl, stem(&s).to_lowercase()));
            }
        } else if s.contains("Texture2D_") || (s.starts_with("g_") && s.ends_with("Texture")) {
            // The byte after the name is printable ('5'): "..._AlbedoMap_05" = slot "..._AlbedoMap_0".
            let name = regex::Regex::new(r"(Map(_\d+)?)\d$").unwrap().replace(&s, "$1").to_string();
            if let Some(sl) = slot.replace(name) {
                out.push((sl, String::new()));
            }
        }
    }
    if let Some(sl) = slot {
        out.push((sl, String::new()));
    }
    out
}

/// The MTD's shader (SPX) name, e.g. "Character_AMSN_[DetailBlend]" or "Fur_NTC".
pub fn mtd_shader(d: &[u8]) -> String {
    let text = String::from_utf8_lossy(d);
    text.split(|c: char| !(c.is_ascii_graphic() || c == ' '))
        .find_map(|s| s.to_lowercase().find(".spx").map(|i| s[..i].rsplit('\\').next().unwrap_or("").to_string()))
        .unwrap_or_default()
}

/// Alpha-tested material: Sekiro's character albedos keep a cutout in their alpha (BC1 punch-through
/// or a 0/255 BC7 alpha: Wolf's tattered coat is 42 % holes), so alpha test is the default. The skin
/// shaders (SSS: faces, hands, mouths) and eyes use the alpha for something else (Wolf's head
/// albedo is 86 % alpha 0) and draw opaque.
pub fn alpha_tested(_mtd: &str, shader: &str) -> bool {
    // The AN_Blend shaders cut too: Wolf's coat (P_BD_M_9040_Court, g_AlphaRef 128) keeps its torn
    // hem and ragged collar edge in the albedo alpha.
    !(shader.contains("SSS") || shader.contains("Eye"))
}
