//! Map export: the game's own arena around one MSB part (the fight's enemy). Writes, for the
//! game (`src/map.rs`):
//!
//! - `extracted/map_<id>.bin` "SHMP" v1: the map pieces the game draws there (the draw groups
//!   of the hit collision under the centre part, as the engine swaps far stand-ins for detail),
//!   merged per material into static meshes. Arena space: the origin is 4 m in front of the
//!   centre part (between the fighters), its facing points to +Z (he stands at z = -4, the
//!   player at +4), X is *not* mirrored (the game mirrors X on load like every model).
//! - `extracted/map_<id>.hit` "SHMC" v1: the hit collision triangles around the centre with
//!   their FromSoftware hit material ids (HitMtrlParam rows: floor sounds).
//! - `extracted/map_<id>.json`: the centre, the light set in force (GparamConfig of that
//!   collision) and the area's draw params (`param/drawparam/<area>_0000.gparam`).
//! - the textures the pieces use into `extracted/tex` (from `map/<area>/*.tpfbhd` and
//!   `other/maptex.tpf`).
//!
//! Layout "SHMP" (little endian):
//!   u32 meshes; per mesh: u16 len albedo, u16 len normal map, u8 alpha (0 opaque, 1 cutout,
//!     2 blend), u8 two-sided, u32 vertices; per vertex f32 pos[3] normal[3] uv[2];
//!     u32 indices; u32 each
//! Layout "SHMC": u32 vertices; f32 pos[3] each; u32 triangles; per triangle u32 idx[3], u32 material

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use crate::{container, flver, gparam, hkx, msb, mtd, cubemap};

#[derive(Default)]
struct Batch {
    pos: Vec<[f32; 3]>,
    normal: Vec<[f32; 3]>,
    uv: Vec<[f32; 2]>,
    uv2: Vec<[f32; 2]>,
    blend: Vec<[u8; 4]>,
    indices: Vec<u32>,
}

/// Arena transform: map space -> arena space (see the module doc).
struct Arena {
    origin: [f32; 3],
    /// Rotation about Y (radians) applied after the translation.
    yaw: f32,
}

impl Arena {
    fn point(&self, v: [f32; 3]) -> [f32; 3] {
        let p = [v[0] - self.origin[0], v[1] - self.origin[1], v[2] - self.origin[2]];
        self.dir(p)
    }
    fn dir(&self, v: [f32; 3]) -> [f32; 3] {
        let (s, c) = self.yaw.sin_cos();
        [v[0] * c + v[2] * s, v[1], -v[0] * s + v[2] * c]
    }
    /// Inverse of `dir`: an arena direction back into map space.
    fn undir(&self, v: [f32; 3]) -> [f32; 3] {
        let (s, c) = (-self.yaw).sin_cos();
        [v[0] * c + v[2] * s, v[1], -v[0] * s + v[2] * c]
    }
}

/// Hours of the six `envmap_00..05` probe variants. Matched by colour and sun direction to
/// the light set's keyed times (m11_01: _00 dark, _01 warm morning sun low in -X+Z, _02 cool
/// noon sun high, _03 orange sunset in -Z, _04 dark, _05 blue moonlight with lantern glow;
/// the gparam keys 0, 6, 12, 18, 22, 2 h have those sky colours and sun angles).
const ENV_HOURS: [f32; 6] = [0.0, 6.0, 12.0, 18.0, 22.0, 2.0];

/// The GI probe the centre stands in: the smallest EnvironmentMapEffectBox around the arena
/// origin, else the nearest EnvironmentMapPoint. Writes, per time-of-day variant, the probe's
/// cube map turned into arena space (X mirrored like the pieces) as `map_<id>_env_<v>.dds`
/// (128, mips: specular / sky) and its irradiance as `map_<id>_envd_<v>.dds` (16).
/// The area's colour-grading LUTs: `map/m11/m11_cgrading.tpf` holds `m11_01_cgrading00<n>`
/// for every LutSourceId `n` the draw params (ColorGrading[Yebis]) select over the day: a
/// 16x256 RGBA8 strip of 16 slices of 16x16 (x = red, row = blue * 16 + green), the display
/// colour in, the graded one out. Written as `map_<id>_lut_<n>.dds`; returns the ids found.
fn export_luts(root: &Path, map_id: &str, stem: &str) -> Vec<u32> {
    let area = &map_id[..3];
    let path = root.join(format!("map/{area}/{area}_cgrading.tpf"));
    let Ok(tpf) = std::fs::read(&path) else {
        println!("no {} (unpack 'map/{area}/')", path.display());
        return Vec::new();
    };
    let prefix = format!("{}_cgrading", &map_id[..6]).to_lowercase();
    let mut ids = Vec::new();
    for (name, dds) in flver::tpf(&tpf) {
        if let Some(id) = name.to_lowercase().strip_prefix(&prefix).and_then(|n| n.parse::<u32>().ok()) {
            std::fs::write(root.join(format!("map_{stem}_lut_{id:04}.dds")), &dds).unwrap();
            ids.push(id);
        }
    }
    ids.sort_unstable();
    println!("colour grading LUTs {ids:?}");
    ids
}

fn export_env(root: &Path, map_id: &str, name: &str, msb: &msb::Msb, arena: &Arena) -> Option<u32> {
    let origin = arena.origin;
    let probe = msb
        .regions
        .iter()
        .filter(|r| r.kind == 17 && r.contains(origin))
        .min_by(|a, b| a.volume().total_cmp(&b.volume()))
        .or_else(|| {
            msb.regions.iter().filter(|r| r.kind == 2).min_by(|a, b| {
                let d = |r: &msb::Region| (r.pos[0] - origin[0]).powi(2) + (r.pos[1] - origin[1]).powi(2) + (r.pos[2] - origin[2]).powi(2);
                d(a).total_cmp(&d(b))
            })
        })?;
    let id = probe.probe()?;
    println!("environment probe {} ({}) -> gilm{id:04}", probe.name, if probe.kind == 17 { "box" } else { "point" });
    for (v, hour) in ENV_HOURS.iter().enumerate() {
        let Ok(tpf) = std::fs::read(root.join(format!("map/{map_id}/{map_id}_envmap_{v:02}.tpf"))) else {
            println!("  no envmap_{v:02}.tpf");
            continue;
        };
        let want = format!("{map_id}_gilm{id:04}_{v:02}");
        let Some((_, dds)) = flver::tpf(&tpf).into_iter().find(|(n, _)| n.eq_ignore_ascii_case(&want)) else {
            println!("  {want} not in envmap_{v:02}.tpf");
            continue;
        };
        let Some(cube) = cubemap::read_bc6h(&dds) else {
            println!("  {want}: not a BC6H cube");
            continue;
        };
        let (bdir, bval) = cube.brightest();
        println!("  {hour:>4} h: mean {:?}, brightest toward {:?} = {:?}", cube.mean(), bdir, bval);
        // Arena (Bevy) direction -> map direction: undo the X mirror, then the arena yaw.
        let src = |d: [f32; 3]| arena.undir([-d[0], d[1], d[2]]);
        let spec = cube.resample(cube.size, src);
        let diff = spec.irradiance(16, 1.0);
        std::fs::write(root.join(format!("map_{name}_env_{v:02}.dds")), cubemap::write_rgba16f(&cubemap::mip_chain(spec))).unwrap();
        std::fs::write(root.join(format!("map_{name}_envd_{v:02}.dds")), cubemap::write_rgba16f(&[diff])).unwrap();
    }
    Some(id)
}

/// Materials whose meshes are not visible surfaces (sekiro-rs maps/pieces.rs skip_material):
/// shadow casters, invisible blockers, light-probe helpers, fog and cloud cards.
fn skip_material(mtd_name: &str) -> bool {
    let m = mtd_name.to_lowercase();
    m.contains("shadow") || m.contains("[dummy]") || m.contains("_dummy") || m.contains("invisible") || m.contains("fog")
}

/// Base colour and normal map of a map material. Map materials stack layers (`M_Multiple`,
/// `MultiBlend3`, ...) and leave the FLVER paths empty: the MTD supplies the slot paths. The base
/// layer is the first albedo slot with a path in MTD order (for `M_Multiple` that is the main
/// surface; later slots are moss / snow overlays); a slot named `<material>_a` wins. The normal
/// map is the one named like the albedo (`x_a` -> `x_n`), else the one named after the
/// material, else the first with a path. (sekiro-rs maps/pieces.rs choose_material.)
/// A map material's texture layers (docs/kb/map.md "Layers"): the base albedo / normal, the
/// overlay (M[Multiple] slots 5 / 4: moss, a second stone...) and the snow (slots 12 / 10;
/// M[MultipleGround] 11 / 7), blended in the game by the vertex blend bytes.
#[derive(Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
struct Layers {
    albedo: String,
    normal: String,
    over_albedo: String,
    over_normal: String,
    snow_albedo: String,
    snow_normal: String,
    /// Layer C (M[Multiple] slots 7 / 0), blended by byte 2.
    c_albedo: String,
    c_normal: String,
    /// The base layer's `_3m` mask (slot 9): R is read as height for the layer blends.
    mask: String,
    alpha: u8,
    two_sided: bool,
}

fn choose(m: &flver::Mesh, mtd: Option<&mtd::Mtd>) -> Layers {
    let stem = |p: &str| p.rsplit(['\\', '/']).next().unwrap_or("").split('.').next().unwrap_or("").to_lowercase();
    let mut albedo: Vec<String> = Vec::new();
    let mut normal: Vec<String> = Vec::new();
    // Slot name -> path, for the layer slots of the sat shaders.
    let mut by_slot: Vec<(String, String)> = Vec::new();
    for (param, tex) in &m.textures {
        let path = if tex.is_empty() {
            mtd.and_then(|d| d.textures.iter().find(|t| &t.kind == param)).map(|t| stem(&t.path)).unwrap_or_default()
        } else {
            tex.clone()
        };
        if path.is_empty() {
            continue;
        }
        let p = param.to_lowercase();
        by_slot.push((p.clone(), path.clone()));
        if p.contains("albedomap") || p.contains("diffuse") {
            albedo.push(path);
        } else if p.contains("normalmap") || p.contains("bumpmap") {
            normal.push(path);
        }
    }
    let slot = |suffix: &str| by_slot.iter().find(|(p, _)| p.ends_with(suffix)).map(|(_, t)| t.clone()).unwrap_or_default();
    let ground = m.mtd.to_lowercase().contains("ground") || mtd.is_some_and(|d| d.shader.to_lowercase().contains("multipleground"));
    let (over_albedo, over_normal) = (slot("texture2d_5_albedomap_0"), slot("texture2d_4_normalmap_0"));
    let (c_albedo, c_normal) = if ground { (String::new(), String::new()) } else { (slot("texture2d_7_albedomap_0"), slot("texture2d_0_normalmap_0")) };
    let mask = slot("texture2d_9_mask3map_0");
    let (snow_albedo, snow_normal) = if ground {
        (slot("texture2d_11_albedomap_0"), slot("texture2d_7_normalmap"))
    } else {
        (slot("texture2d_12_albedomap_0"), slot("texture2d_10_normalmap_0"))
    };
    let own = format!("{}_a", m.material.to_lowercase());
    let a = albedo.iter().position(|t| *t == own).or(if albedo.is_empty() { None } else { Some(0) });
    let n = a
        .and_then(|a| albedo[a].strip_suffix("_a").map(|b| format!("{b}_n")))
        .and_then(|want| normal.iter().position(|t| *t == want))
        .or_else(|| normal.iter().position(|t| *t == format!("{}_n", m.material.to_lowercase())))
        .or(if normal.is_empty() { None } else { Some(0) });
    let (mut alpha, mut two_sided) = (0u8, false);
    if let Some(d) = mtd {
        let alpha_ref = d.int("g_AlphaRef").unwrap_or(0);
        let blend = d.int("g_BlendMode").unwrap_or(0);
        if blend == 2 || blend >= 4 {
            alpha = 2;
        } else if alpha_ref > 0 || blend == 1 {
            alpha = 1;
        }
        two_sided = d.int("g_DoubleSided").unwrap_or(0) != 0 || alpha != 0;
    }
    let albedo = a.map(|i| albedo[i].clone()).unwrap_or_default();
    // An overlay that is the base again (stonewall_block_edge: the same albedo with another
    // normal) carries nothing worth a second sample.
    let (over_albedo, over_normal) = if over_albedo == albedo { (String::new(), String::new()) } else { (over_albedo, over_normal) };
    let (c_albedo, c_normal) = if c_albedo == albedo || c_albedo.is_empty() { (String::new(), String::new()) } else { (c_albedo, c_normal) };
    Layers {
        albedo,
        normal: n.map(|i| normal[i].clone()).unwrap_or_default(),
        over_albedo,
        over_normal,
        snow_albedo,
        snow_normal,
        c_albedo,
        c_normal,
        mask,
        alpha,
        two_sided,
    }
}

fn height_on(x: f32, z: f32, a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> Option<f32> {
    let d = (b[2] - c[2]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[2] - c[2]);
    if d.abs() < 1e-9 {
        return None;
    }
    let l1 = ((b[2] - c[2]) * (x - c[0]) + (c[0] - b[0]) * (z - c[2])) / d;
    let l2 = ((c[2] - a[2]) * (x - c[0]) + (a[0] - c[0]) * (z - c[2])) / d;
    let l3 = 1.0 - l1 - l2;
    (l1 >= 0.0 && l2 >= 0.0 && l3 >= 0.0).then(|| l1 * a[1] + l2 * b[1] + l3 * c[1])
}

/// `map <extracted> <map id> <centre part name> <radius m>`.
/// `centre`: a part name, or an entity id (a boss script's character, e.g. 1700800). The files
/// are named after `MAP_NAME` when set (one arena per boss: `boss_<NpcParam row>`), else the map.
pub fn export(root: &Path, map_id: &str, centre: &str, radius: f32) {
    let msb = msb::read(&std::fs::read(root.join(format!("map/mapstudio/{map_id}.msb"))).expect("read msb"));
    let entity: Option<i32> = centre.parse().ok();
    let c = msb.parts.iter().find(|p| p.name == centre || (entity.is_some() && Some(p.entity) == entity)).unwrap_or_else(|| panic!("no part {centre}"));
    let name = std::env::var("MAP_NAME").unwrap_or_else(|_| map_id.to_string());
    let name = name.as_str();
    // The enemy faces +Z in its own space; the origin is 4 m in front of him and the arena is
    // turned so he keeps facing +Z: the game spawns the enemy at (0, y, -4), his real spot, and
    // the player at (0, y, 4), in front of him where the player walks up in the game.
    let facing = msb::rotate(c.rot, [0.0, 0.0, 1.0]);
    let origin = [c.pos[0] + 4.0 * facing[0], c.pos[1], c.pos[2] + 4.0 * facing[2]];
    let arena = Arena { origin, yaw: -c.rot[1].to_radians() };
    println!("centre {} ({}) at {:?} yaw {:.1}; arena origin {:?}, facing {:?} -> {:?}", c.name, c.model, c.pos, c.rot[1], origin, facing, arena.dir(facing));
    let area = &map_id[..3];
    let dir = root.join(format!("map/{map_id}"));

    // Hit collision parts near the centre (all of them share one transform in a map).
    let hit_dir = dir.join("hit");
    let compendium = hkx::read_types(&std::fs::read(hit_dir.join(format!("h{}.compendium", &map_id[1..]))).expect("read compendium (run `bxf` on the h*.hkxbhd first)"));
    let mut hit_verts: Vec<[f32; 3]> = Vec::new();
    let mut hit_tris: Vec<[u32; 4]> = Vec::new();
    let mut under: Option<(f32, usize)> = None; // (height below the centre, part index)
    let mut hit_parts = 0;
    for (pi, p) in msb.parts.iter().enumerate().filter(|(_, p)| p.kind == 5) {
        let id = p.model.trim_start_matches('h');
        let Ok(data) = std::fs::read(hit_dir.join(format!("h{}_{id}.hkx", &map_id[1..]))) else { continue };
        let mesh = hkx::collision_mesh(&data, &compendium);
        let world: Vec<[f32; 3]> = mesh.vertices.iter().map(|v| p.transform(*v)).collect();
        // Keep the part when any vertex is within the radius (horizontally).
        let near = world.iter().any(|v| (v[0] - origin[0]).hypot(v[2] - origin[2]) <= radius + 10.0);
        if !near {
            continue;
        }
        hit_parts += 1;
        let base = hit_verts.len() as u32;
        for t in 0..mesh.indices.len() / 3 {
            let (a, b, cc) = (mesh.indices[t * 3] as usize, mesh.indices[t * 3 + 1] as usize, mesh.indices[t * 3 + 2] as usize);
            if let Some(h) = height_on(c.pos[0], c.pos[2], world[a], world[b], world[cc]) {
                let below = c.pos[1] + 0.5 - h;
                // The nearest floor under the centre part's feet that draws anything (the
                // Guardian Ape's 1700850 stands on h900707, a helper collision with no draw
                // groups, over the cave floor that has them).
                if below >= 0.0 && under.is_none_or(|(d, _)| below < d) && p.draw_groups.iter().any(|g| *g != 0) {
                    under = Some((below, pi));
                }
            }
            hit_tris.push([base + a as u32, base + b as u32, base + cc as u32, mesh.materials[t]]);
        }
        hit_verts.extend(world.iter().map(|v| arena.point(*v)));
    }
    let under_part = under.map(|(_, pi)| &msb.parts[pi]);
    match under_part {
        Some(p) => println!("collision under the centre: {} ({}), {:.2} m below, draw groups {:08x?}, light set {}", p.name, p.model, under.unwrap().0, p.draw_groups, p.gparam[0]),
        None => println!("no collision under the centre: all pieces within the radius are kept"),
    }
    let groups: Vec<u32> = under_part.map(|p| p.draw_groups.clone()).unwrap_or_default();
    let hit_mats: HashSet<u32> = hit_tris.iter().map(|t| t[3]).collect();
    println!("collision: {hit_parts} parts, {} triangles, materials {:?}", hit_tris.len(), hit_mats);

    // MTDs.
    let mtd_dir = root.join("mtd/allmaterialbnd.mtdbnd.d");
    let mut mtds: HashMap<String, Option<mtd::Mtd>> = HashMap::new();

    let lod_dist: [f32; 2] = std::env::var("MAP_LOD")
        .ok()
        .and_then(|v| {
            let d: Vec<f32> = v.split(',').filter_map(|x| x.trim().parse().ok()).collect();
            (d.len() == 2).then(|| [d[0], d[1]])
        })
        .unwrap_or([30.0, 55.0]);
    // Pieces within `MAP_SHADOW` m (default 45) of the arena cast the sun's shadows; the rest
    // are tagged not to (the shadow passes over the whole map cost half the frame rate).
    let shadow_m: f32 = std::env::var("MAP_SHADOW").ok().and_then(|v| v.parse().ok()).unwrap_or(45.0);
    // `MAP_CELL=<m>` splits the batches by cells of the piece's centre so that the camera and
    // the shadow cascades can cull them; off by default: 20 m cells (1016 meshes) cost more
    // per entity than the culling saved (39 fps against 56, kb/map.md).
    let cell_m: f32 = std::env::var("MAP_CELL").ok().and_then(|v| v.parse().ok()).unwrap_or(0.0);
    let mut batches: BTreeMap<(Layers, bool, i32, i32), Batch> = BTreeMap::new();
    let mut pieces = 0;
    let mut tris = 0usize;
    let mut skipped_groups = 0;
    // Map pieces (part type 0, the map's mapbnds) and objects (type 1: trees, fences, the
    // Divine Dragon's arena; `obj/<model>.objbnd`, whose rigid meshes are in their bone's space,
    // flver.rs SEKIRO_FLVER_REF_POSE). An object not unpacked yet is listed for `unpack`.
    // gap: objects' own collision (the objbnd's hkx) is not added to the .hit.
    let obj_dir = root.join("obj");
    let mut objects_missing: std::collections::BTreeSet<String> = Default::default();
    let mut objects = 0;
    let mut objects_used: std::collections::BTreeSet<String> = Default::default();
    for p in msb.parts.iter().filter(|p| p.kind == 0 || p.kind == 1) {
        if !groups.is_empty() && !p.draws_with(&groups) {
            skipped_groups += 1;
            if std::env::var("MAP_LOG").is_ok() && (p.pos[0] - origin[0]).hypot(p.pos[2] - origin[2]) <= radius {
                println!("  not drawn there: {} ({}) draw groups {:08x?}", p.name, p.model, p.draw_groups);
            }
            continue;
        }
        let object = p.kind == 1;
        let path = if object {
            if (p.pos[0] - origin[0]).hypot(p.pos[2] - origin[2]) > radius + 60.0 {
                continue;
            }
            let m = p.model.to_lowercase();
            obj_dir.join(format!("{m}.objbnd.d/{m}.flver"))
        } else {
            let id = p.model.trim_start_matches('m');
            dir.join(format!("{map_id}_{id}.mapbnd.d/{map_id}_{id}.flver"))
        };
        let Ok(data) = std::fs::read(&path) else {
            if object {
                objects_missing.insert(p.model.to_lowercase());
            }
            continue;
        };
        let read = |lod: u32| {
            if object {
                unsafe { std::env::set_var("SEKIRO_FLVER_REF_POSE", "1") };
            }
            let f = flver::read_lod(&data, lod);
            if object {
                unsafe { std::env::remove_var("SEKIRO_FLVER_REF_POSE") };
            }
            f
        };
        let mut f = read(0);
        let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
        for v in f.meshes.iter().flat_map(|m| m.vertices.iter()) {
            let w = p.transform(v.pos);
            lo = [lo[0].min(w[0]), lo[1].min(w[2])];
            hi = [hi[0].max(w[0]), hi[1].max(w[2])];
        }
        if lo[0] > hi[0] {
            continue;
        }
        let dx = (lo[0] - origin[0]).max(origin[0] - hi[0]).max(0.0);
        let dz = (lo[1] - origin[2]).max(origin[2] - hi[1]).max(0.0);
        if dx.hypot(dz) > radius {
            continue;
        }
        // Level of detail by distance, as the engine swaps them: full within 30 m, LodLevel1
        // to 55 m, LodLevel2 beyond (pieces without a level keep the next lower one).
        // `MAP_LOD=<full m>,<lod1 m>` overrides (docs/kb/map.md, frame-rate tuning).
        let dist = dx.hypot(dz);
        let lod = if dist <= lod_dist[0] { 0 } else if dist <= lod_dist[1] { 1 } else { 2 };
        if lod > 0 {
            f = read(lod);
        }
        pieces += 1;
        objects += object as usize;
        if object {
            objects_used.insert(p.model.to_lowercase());
        }
        let piece_tris: usize = f.meshes.iter().map(|m| m.indices.len() / 3).sum();
        if std::env::var("MAP_LOG").is_ok() {
            println!("piece {} {} tris {} dist {:.0}", p.name, p.model, piece_tris, dx.hypot(dz));
        }
        for m in &f.meshes {
            if skip_material(&m.mtd) {
                continue;
            }
            let key = m.mtd.to_lowercase();
            let mtd = mtds.entry(key.clone()).or_insert_with(|| std::fs::read(mtd_dir.join(format!("{key}.mtd"))).ok().and_then(|d| mtd::read(&d))).as_ref();
            let layers = choose(m, mtd);
            if layers.albedo.is_empty() && std::env::var("MAP_LOG").is_ok() {
                println!("noalbedo {} [{}] shader {} slots {:?} mtd slots {:?}", m.material, m.mtd, mtd.map_or("?", |d| d.shader.as_str()), m.textures, mtd.map(|d| d.textures.iter().map(|t| (t.kind.clone(), t.path.clone())).collect::<Vec<_>>()));
            }
            let centre = [(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5];
            let cell = |v: f32| if cell_m > 0.0 { (v / cell_m).floor() as i32 } else { 0 };
            let b = batches.entry((layers, dist <= shadow_m, cell(centre[0]), cell(centre[1]))).or_default();
            let base = b.pos.len() as u32;
            for v in &m.vertices {
                b.pos.push(arena.point(p.transform(v.pos)));
                let n = arena.dir(msb::rotate(p.rot, v.normal));
                let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-6);
                b.normal.push([n[0] / l, n[1] / l, n[2] / l]);
                b.uv.push(v.uv);
                b.uv2.push(v.uv2);
                // Blend weights: overlay (byte 0), layer C (byte 1), snow (byte 4), byte 2
                // (kb/map.md "Layers": read off the vertex data, not the shader).
                b.blend.push([v.blend[0][0], v.blend[1][0], v.blend[4][0], v.blend[2][0]]);
            }
            b.indices.extend(m.indices.iter().map(|i| base + i));
            tris += m.indices.len() / 3;
        }
    }
    println!("{pieces} pieces ({objects} objects; {skipped_groups} outside the draw groups), {} batches, {tris} triangles", batches.len());
    if !objects_missing.is_empty() {
        let list: Vec<String> = objects_missing.iter().cloned().collect();
        println!("objects to unpack: ^/obj/({})\\.objbnd", list.join("|"));
    }

    let put_str = |b: &mut Vec<u8>, s: &str| {
        b.extend_from_slice(&(s.len() as u16).to_le_bytes());
        b.extend_from_slice(s.as_bytes());
    };
    let put_f = |b: &mut Vec<u8>, v: &[f32]| v.iter().for_each(|x| b.extend_from_slice(&x.to_le_bytes()));
    let mut b: Vec<u8> = Vec::new();
    // Version 4: nine texture names per batch (base, overlay, snow, layer C albedo / normal,
    // the base mask), a caster byte, and 44-byte vertices (pos, normal, uv, uv2, 4 blend bytes).
    b.extend_from_slice(b"SHMP");
    b.extend_from_slice(&4u32.to_le_bytes());
    b.extend_from_slice(&(batches.len() as u32).to_le_bytes());
    let mut wanted: HashSet<String> = HashSet::new();
    for ((l, casts, _, _), m) in &batches {
        for t in [&l.albedo, &l.normal, &l.over_albedo, &l.over_normal, &l.snow_albedo, &l.snow_normal, &l.c_albedo, &l.c_normal, &l.mask] {
            if !t.is_empty() {
                wanted.insert(t.clone());
            }
            put_str(&mut b, t);
        }
        b.push(l.alpha);
        b.push(l.two_sided as u8);
        b.push(*casts as u8);
        b.extend_from_slice(&(m.pos.len() as u32).to_le_bytes());
        for i in 0..m.pos.len() {
            put_f(&mut b, &m.pos[i]);
            put_f(&mut b, &m.normal[i]);
            put_f(&mut b, &m.uv[i]);
            put_f(&mut b, &m.uv2[i]);
            b.extend_from_slice(&m.blend[i]);
        }
        b.extend_from_slice(&(m.indices.len() as u32).to_le_bytes());
        m.indices.iter().for_each(|x| b.extend_from_slice(&x.to_le_bytes()));
    }
    let out = root.join(format!("map_{name}.bin"));
    std::fs::write(&out, &b).unwrap();
    println!("{} ({} MB)", out.display(), b.len() / 1_000_000);

    let mut h: Vec<u8> = Vec::new();
    h.extend_from_slice(b"SHMC");
    h.extend_from_slice(&1u32.to_le_bytes());
    h.extend_from_slice(&(hit_verts.len() as u32).to_le_bytes());
    hit_verts.iter().for_each(|v| put_f(&mut h, v));
    h.extend_from_slice(&(hit_tris.len() as u32).to_le_bytes());
    hit_tris.iter().flatten().for_each(|x| h.extend_from_slice(&x.to_le_bytes()));
    std::fs::write(root.join(format!("map_{name}.hit")), &h).unwrap();

    // Draw params of the area (the base file; the light set id picks the variant).
    let gparam_path = root.join(format!("param/drawparam/{}_0000.gparam", &map_id[..6]));
    let draw = std::fs::read(&gparam_path).ok().map(|d| gparam::to_json(&gparam::read(&d)));
    if draw.is_none() {
        println!("no {} (unpack 'param/drawparam/{}')", gparam_path.display(), &map_id[..6]);
    }
    let env_probe = export_env(root, map_id, name, &msb, &arena);
    let luts = export_luts(root, map_id, name);
    let json = serde_json::json!({
        "env_probe": env_probe,
        "env_hours": ENV_HOURS,
        "luts": luts,
        "map": map_id,
        "centre": { "name": c.name, "model": c.model, "npc": c.npc, "pos": c.pos, "yaw": c.rot[1] },
        "origin": origin,
        "arena_yaw": arena.yaw,
        "collision_part": under_part.map(|p| p.name.clone()),
        "light_set": under_part.map(|p| p.gparam[0]).unwrap_or(0),
        "gparam": under_part.map(|p| p.gparam).unwrap_or([-1; 4]),
        "draw_groups": groups,
        "drawparam": draw,
    });
    std::fs::write(root.join(format!("map_{name}.json")), serde_json::to_string_pretty(&json).unwrap()).unwrap();

    // Textures: every BXF4 pack of the area (map/m11/m11_0000..0003.tpfbhd, one TPF per
    // texture) and the shared other/maptex.tpf.
    let tex_dir = root.join("tex");
    std::fs::create_dir_all(&tex_dir).unwrap();
    let mut found: HashSet<String> = HashSet::new();
    let mut take = |name: String, dds: &[u8]| {
        if wanted.contains(&name) && found.insert(name.clone()) {
            std::fs::write(tex_dir.join(format!("{name}.dds")), drop_top_mips(dds, 1024)).unwrap();
        }
    };
    for e in std::fs::read_dir(root.join(format!("map/{area}"))).into_iter().flatten().flatten() {
        let p = e.path();
        if p.extension().is_none_or(|x| x != "tpfbhd") {
            continue;
        }
        let (Ok(bhd), Ok(bdt)) = (std::fs::read(&p), std::fs::read(p.with_extension("tpfbdt"))) else { continue };
        for f in container::read_bxf4(&bhd, &bdt) {
            if f.data.starts_with(b"TPF\0") {
                for (name, dds) in flver::tpf(&f.data) {
                    take(name, &dds);
                }
            }
        }
    }
    // Objects carry their textures in the objbnd (`<model>.tpf`).
    for m in &objects_used {
        for e in std::fs::read_dir(obj_dir.join(format!("{m}.objbnd.d"))).into_iter().flatten().flatten() {
            if e.path().extension().is_some_and(|x| x == "tpf") {
                if let Ok(d) = std::fs::read(e.path()) {
                    for (name, dds) in flver::tpf(&d) {
                        take(name, &dds);
                    }
                }
            }
        }
    }
    if let Ok(d) = std::fs::read(root.join("other/maptex.tpf")) {
        for (name, dds) in flver::tpf(&d) {
            take(name, &dds);
        }
    }
    let mut missing: Vec<_> = wanted.iter().filter(|t| !found.contains(*t)).cloned().collect();
    missing.sort();
    println!("textures: {} of {} found; missing {missing:?}", found.len(), wanted.len());
}

/// A DDS (DX10 header, block-compressed) without its largest mip levels until it is at most
/// `max` texels wide and high: the arena's 350 textures are 1.5 GB at full size (2048 x 1024
/// BC7 with mips), which takes the better part of a minute to reach the GPU.
fn drop_top_mips(dds: &[u8], max: u32) -> Vec<u8> {
    let u32_ = |o: usize| u32::from_le_bytes(dds[o..o + 4].try_into().unwrap());
    if dds.len() < 148 || &dds[0..4] != b"DDS " || &dds[84..88] != b"DX10" {
        return dds.to_vec();
    }
    let (mut h, mut w, mut mips) = (u32_(12), u32_(16), u32_(28).max(1));
    let fmt = u32_(128);
    // Bytes per 4x4 block: BC1 / BC4 8, the rest 16.
    let block = if matches!(fmt, 70..=73 | 79..=81) { 8 } else { 16 };
    let mut o = 148;
    while (w > max || h > max) && mips > 1 {
        o += (w.div_ceil(4) * h.div_ceil(4)) as usize * block;
        w = (w / 2).max(1);
        h = (h / 2).max(1);
        mips -= 1;
    }
    if o == 148 || o > dds.len() {
        return dds.to_vec();
    }
    let mut out = dds[..148].to_vec();
    out[12..16].copy_from_slice(&h.to_le_bytes());
    out[16..20].copy_from_slice(&w.to_le_bytes());
    out[20..24].copy_from_slice(&((w.div_ceil(4) * block as u32).max(1)).to_le_bytes());
    out[28..32].copy_from_slice(&mips.to_le_bytes());
    out.extend_from_slice(&dds[o..]);
    out
}
