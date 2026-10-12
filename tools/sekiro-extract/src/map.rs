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
//!     2 blend, 3 flow smoke: the mask scrolled along the "normal" flow map), u8 two-sided, u32 vertices; per vertex f32 pos[3] normal[3] uv[2];
//!     u32 indices; u32 each
//! Layout "SHMC": u32 vertices; f32 pos[3] each; u32 triangles; per triangle u32 idx[3], u32 material

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use crate::{container, flver, gparam, hkx, msb, mtd, cubemap};

/// (map, part, mesh index): meshes left out of the export (`MAP_SKIP_MESH=<part>:<index>` adds
/// to the list at run time). Empty since 2026-10-11: the Guardian Ape arena's giant Buddha
/// pedestal shell (m17_00_00_00 m700850_7000 mesh 3, a far-view copy of the floor and walls
/// a metre above the real ones) is back-face culled from inside once the triangles are wound
/// the right way round (src/map.rs BinBatch::indices), so it needs no special case.
const HIDDEN_MESHES: &[(&str, &str, usize)] = &[];

#[derive(Default)]
struct Batch {
    pos: Vec<[f32; 3]>,
    normal: Vec<[f32; 3]>,
    uv: Vec<[f32; 2]>,
    uv2: Vec<[f32; 2]>,
    blend: Vec<[u8; 8]>,
    /// |part scale X|: the game path's uv scale (uvS = uv0 x |world X axis|).
    uv_scale: Vec<f32>,
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

/// One record of an EDGE file (docs/kb/grapple.md section 1), in the owner's local space:
/// the three vertices, the type (1 grapple, 2 hang, 3 hug) and the variation byte.
struct EdgeRecord {
    v: [[f32; 3]; 3],
    kind: u8,
    variation: u8,
}

/// The records of an EDGE file (version 4, 64-byte records from offset 16).
fn edge_records(d: &[u8]) -> Vec<EdgeRecord> {
    if d.len() < 16 || u32::from_le_bytes(d[0..4].try_into().unwrap()) != 4 {
        return Vec::new();
    }
    let n = u32::from_le_bytes(d[4..8].try_into().unwrap()) as usize;
    let f = |o: usize| f32::from_le_bytes(d[o..o + 4].try_into().unwrap());
    (0..n)
        .filter(|i| 16 + i * 64 + 64 <= d.len())
        .map(|i| {
            let o = 16 + i * 64;
            EdgeRecord { v: [[f(o), f(o + 4), f(o + 8)], [f(o + 16), f(o + 20), f(o + 24)], [f(o + 32), f(o + 36), f(o + 40)]], kind: d[o + 52], variation: d[o + 53] }
        })
        .collect()
}

/// Grapple edges of an EDGE file: (hook = the midpoint of V1-V2, landing = V3, variation).
fn edge_grapples(d: &[u8]) -> Vec<([f32; 3], [f32; 3], u8)> {
    edge_records(d)
        .into_iter()
        .filter(|e| e.kind == 1)
        .map(|e| {
            let [v1, v2, v3] = e.v;
            ([(v1[0] + v2[0]) / 2.0, (v1[1] + v2[1]) / 2.0, (v1[2] + v2[2]) / 2.0], v3, e.variation)
        })
        .collect()
}

/// Hang (2) and hug (3) edges of an EDGE file, raw: the three vertices (V1-V2 the edge's
/// segment, V3 the third point: gap, their meaning per type is not decoded; see the gizmos).
fn edge_ledges(d: &[u8]) -> Vec<EdgeRecord> {
    edge_records(d).into_iter().filter(|e| e.kind == 2 || e.kind == 3).collect()
}

/// The grapple points within `radius` + 30 m of the arena origin, in the arena frame
/// (docs/kb/grapple.md section 1): the grapple-type records of objects' `e<id>.edge` files and
/// of the map's `<map>.edgebhd` (one `<map>_NNNNNN.edge` per collision part, local to it).
/// Each entry: {hook, land, fwd, var, src}; `fwd` is the edge's reference direction for the
/// WireVariationParam recognition angles (the owner part's facing: gap, the edge's own
/// direction field is not decoded). The `o000100` marker objects (dummies 90 / 170) are not
/// a source: on the gate map all 40 within range are the MSB Treasure events' item spots
/// (`msb.treasure_parts`), 0.25 m over the floor; treasure parts of any model are skipped.
fn export_grapple(root: &Path, map_id: &str, msb: &msb::Msb, arena: &Arena, origin: [f32; 3], radius: f32, obj_dir: &Path) -> (Vec<serde_json::Value>, Vec<serde_json::Value>) {
    let near = |p: [f32; 3]| (p[0] - origin[0]).hypot(p[2] - origin[2]) <= radius + 30.0;
    // Hang / hug edges ("ledges"): the same files' type 2 / 3 records within the radius + 10 m,
    // raw ({a, b, c, kind, var, src}) for the ledge hang and wall hug moves later (HKS
    // BEH_A_GROUND_HANG_START, COVER_*); the game only draws them (map.rs LedgeEdges).
    let mut ledges = Vec::new();
    let mut ledge = |e: &EdgeRecord, p: &msb::Part| {
        let a = p.transform(e.v[0]);
        if (a[0] - origin[0]).hypot(a[2] - origin[2]) <= radius + 10.0 {
            ledges.push(serde_json::json!({ "a": arena.point(a), "b": arena.point(p.transform(e.v[1])), "c": arena.point(p.transform(e.v[2])), "kind": e.kind, "var": e.variation, "src": p.name }));
        }
    };
    let mut ledge_cache: HashMap<String, Vec<EdgeRecord>> = HashMap::new();
    let norm = |v: [f32; 3]| {
        let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        if l > 1e-6 { [v[0] / l, v[1] / l, v[2] / l] } else { [0.0, 0.0, 1.0] }
    };
    let mut out = Vec::new();
    let mut entry = |hook: [f32; 3], land: [f32; 3], fwd: [f32; 3], var: u8, src: String| -> bool {
        if !near(hook) {
            return false;
        }
        out.push(serde_json::json!({ "hook": arena.point(hook), "land": arena.point(land), "fwd": arena.dir(norm(fwd)), "var": var, "src": src }));
        true
    };
    let mut edge_cache: HashMap<String, Vec<([f32; 3], [f32; 3], u8)>> = HashMap::new();
    let (mut obj_edges, mut map_edges, mut treasures) = (0, 0, 0);
    for (pi, p) in msb.parts.iter().enumerate().filter(|(_, p)| p.kind == 1 && !p.model.is_empty()) {
        if !near(p.pos) {
            continue;
        }
        let m = p.model.to_lowercase();
        if msb.treasure_parts.contains(&pi) {
            treasures += 1;
            if std::env::var("MAP_LOG").is_ok() {
                println!("treasure {} ({m}) entity {} pos {:?}", p.name, p.entity, p.pos);
            }
            continue;
        }
        let edge_file = obj_dir.join(format!("{m}.objbnd.d/e{}.edge", &m[1..]));
        let edges = edge_cache.entry(m.clone()).or_insert_with(|| std::fs::read(&edge_file).map(|d| edge_grapples(&d)).unwrap_or_default());
        for &(hook, land, var) in edges.iter() {
            obj_edges += entry(p.transform(hook), p.transform(land), msb::rotate(p.rot, [0.0, 0.0, 1.0]), var, p.name.clone()) as usize;
        }
        for e in ledge_cache.entry(m.clone()).or_insert_with(|| std::fs::read(&edge_file).map(|d| edge_ledges(&d)).unwrap_or_default()).iter() {
            ledge(e, p);
        }
    }
    // The map's own edge files (none with grapple edges on m11_01_00_00 or m10_00_00_00; kept
    // for other maps).
    let edge_dir = root.join(format!("map/{map_id}"));
    if let (Ok(bhd), Ok(bdt)) = (std::fs::read(edge_dir.join(format!("{map_id}.edgebhd"))), std::fs::read(edge_dir.join(format!("{map_id}.edgebdt")))) {
        for f in container::read_bxf4(&bhd, &bdt) {
            let stem = f.name.rsplit(['\\', '/']).next().unwrap_or(&f.name).trim_end_matches(".edge");
            let Some(id) = stem.rsplit('_').next() else { continue };
            let part_model = format!("h{}_{id}", &map_id[1..]);
            let Some(p) = msb.parts.iter().find(|p| p.kind == 5 && p.model.eq_ignore_ascii_case(&part_model)) else { continue };
            for (hook, land, var) in edge_grapples(&f.data) {
                map_edges += entry(p.transform(hook), p.transform(land), msb::rotate(p.rot, [0.0, 0.0, 1.0]), var, p.name.clone()) as usize;
            }
            for e in edge_ledges(&f.data) {
                ledge(&e, p);
            }
        }
    }
    let hangs = ledges.iter().filter(|l| l["kind"] == 2).count();
    println!("grapple points: {} ({obj_edges} object edges, {map_edges} map edges within the radius; {treasures} treasure spots skipped); ledges: {} ({hangs} hang, {} hug)", out.len(), ledges.len(), ledges.len() - hangs);
    (out, ledges)
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
    /// The game path (bin v5, docs/kb/map.md "The sat layer shaders, decompiled"): 1
    /// M[Multiple], 2 M[MultipleGround], 0 none (the fields below empty). For kinds 1 / 2,
    /// `c_albedo` / `c_normal` are layer L (M[Multiple] slot 7; M[MultipleGround] 1 / 6) and
    /// `mask` is slot 9.
    kind: u8,
    /// The detail normal (slot 0; ground 12), the layer mask on uv1 (M[Multiple] slot 6), the
    /// wet flow normal (slot 16; ground 0).
    detail: String,
    mask_l: String,
    wet: String,
    /// The material's shader constants in the game path's order (`game_params`), as f32 bits.
    params: Vec<u32>,
}

/// The texture slots each pixel-shader variant samples (Gbuf pass, by GXFT_SATStaticSwitch;
/// from the disassembly, docs/kb/map.md). A slot the variant does not sample is dropped.
const MULTIPLE_SLOTS: [&[u8]; 34] = [
    &[8, 0, 3], &[8, 0, 3, 6, 7], &[8, 0, 16, 3, 6, 7], &[8, 0, 3], &[8, 0, 3], &[8, 0, 12, 3, 9], &[8, 0, 12, 3, 6, 7, 9], &[8, 0, 12, 16, 3, 6, 7, 9],
    &[8, 0, 3], &[8, 0, 3], &[8, 0, 10, 12, 3, 9], &[8, 0, 10, 12, 3, 6, 7, 9], &[8, 0, 10, 12, 16, 3, 6, 7, 9], &[8, 0, 3], &[8, 0, 3], &[8, 0, 3, 4, 5, 9],
    &[8, 0, 3, 4, 5, 6, 7, 9], &[8, 0, 16, 3, 4, 5, 6, 7, 9], &[8, 0, 17, 3, 4, 5, 6, 7, 9], &[8, 0, 3], &[8, 0, 12, 3, 4, 5, 9], &[8, 0, 12, 3, 4, 5, 6, 7, 9],
    &[8, 0, 12, 16, 3, 4, 5, 6, 7, 9], &[8, 0, 3], &[8, 0, 3], &[8, 0, 10, 12, 3, 4, 5, 9], &[8, 0, 10, 12, 3, 4, 5, 6, 7, 9], &[8, 0, 10, 12, 16, 3, 4, 5, 6, 7, 9],
    &[8, 0, 3], &[8, 0, 3], &[8, 0, 3], &[8, 0, 3], &[8, 0, 3, 4, 5, 9], &[8, 0, 3, 4, 5, 9],
];
const GROUND_SLOTS: [&[u8]; 34] = [
    &[12, 3, 8], &[0, 12, 3, 8], &[0, 12, 2, 3, 8], &[11, 12, 3, 7, 8, 9], &[0, 11, 12, 3, 7, 8, 9], &[3, 8], &[11, 12, 3, 7, 8, 9], &[0, 11, 12, 3, 7, 8, 9],
    &[3, 8], &[3, 8], &[3, 8], &[3, 8], &[12, 3, 4, 5, 8, 9], &[0, 12, 3, 4, 5, 8, 9], &[0, 12, 2, 3, 4, 5, 8, 9], &[11, 12, 3, 4, 5, 7, 8, 9],
    &[0, 11, 12, 3, 4, 5, 7, 8, 9], &[3, 8], &[11, 12, 3, 4, 5, 7, 8, 9], &[0, 11, 12, 3, 4, 5, 7, 8, 9], &[3, 8], &[3, 8], &[3, 8], &[3, 8],
    &[1, 12, 3, 4, 5, 6, 8, 9], &[0, 1, 12, 3, 4, 5, 6, 8, 9], &[0, 1, 12, 2, 3, 4, 5, 6, 8, 9], &[1, 11, 12, 3, 4, 5, 6, 7, 8, 9], &[3, 8], &[3, 8],
    &[1, 11, 12, 3, 4, 5, 6, 7, 8, 9], &[3, 8], &[3, 8], &[3, 8],
];

/// The game path's constants (64 floats, 16 vec4; map_material.wgsl `game_layers` reads them
/// by index) from the MTD's `M_Multiple__snp_0_*` / `M_MultipleGround__snp_0_*` values. The
/// snow direction is turned into arena space. A value the MTD leaves out is 0 (colours 1).
/// Colours are rgb x `color_N-Intensity` (default 1), as the engine's GXMaterialParam_HDRColor
/// uploads them (FUN_1403dc4b0; constructor FUN_1403dc1a0). gap: that class first raises the
/// colour to the power 2.2 when a flag byte (+0x20 of its definition) is set, not traced: here
/// raw; the shader tool's own defaults for left-out values.
fn game_params(d: &mtd::Mtd, kind: u8, arena: &Arena) -> Vec<f32> {
    let prefix = if kind == 2 { "M_MultipleGround__snp_0_" } else { "M_Multiple__snp_0_" };
    let get = |n: &str| d.floats.iter().find(|(k, _)| *k == format!("{prefix}{n}")).map(|(_, v)| v.clone());
    let f = |n: &str| get(n).and_then(|v| v.first().copied()).unwrap_or(0.0);
    let v3 = |n: &str| get(n).filter(|v| v.len() >= 3).map_or([0.0; 3], |v| [v[0], v[1], v[2]]);
    let v2 = |n: &str| get(n).filter(|v| v.len() >= 2).map_or([0.0; 2], |v| [v[0], v[1]]);
    let col = |n: &str| -> [f32; 4] {
        let c = get(n).filter(|v| v.len() >= 3).map_or([1.0; 3], |v| [v[0], v[1], v[2]]);
        let k = get(&format!("{n}-Intensity")).and_then(|v| v.first().copied()).unwrap_or(1.0);
        [c[0] * k, c[1] * k, c[2] * k, 0.0]
    };
    let w = |a: [f32; 3], x: f32| [a[0], a[1], a[2], x];
    let one = [1.0, 1.0, 1.0, 0.0];
    let p: [[f32; 4]; 16] = if kind == 2 {
        let snow = arena.dir(v3("float3_5"));
        let wet = v2("float2_1");
        [
            one, one, one, one, col("color_1"), col("color_0"),
            [f("float_4"), f("float_5"), f("float_23"), f("float_12")],
            [f("float_10"), f("float_6"), f("float_7"), f("float_22")],
            [f("float_2"), f("float_3"), 0.0, 0.0],
            w(v3("float3_0"), f("float_31")), w(v3("float3_2"), f("float_8")), [0.0; 4], [0.0; 4],
            w(v3("float3_3"), 0.0), w(snow, 0.0), [wet[0], wet[1], 0.0, 0.0],
        ]
    } else {
        let snow = arena.dir(v3("float3_7"));
        let (l, wet) = (v2("float2_0"), v2("float2_1"));
        [
            col("color_1"), col("color_7"), col("color_2"), col("color_5"), col("color_6"), col("color_8"),
            [f("float_4"), f("float_5"), f("float_1"), f("float_0")],
            [f("float_29"), f("float_26"), f("float_30"), f("float_32")],
            [f("float_2"), f("float_19"), l[0], l[1]],
            w(v3("float3_0"), 0.0), w(v3("float3_2"), 0.0), w(v3("float3_1"), 0.0), w(v3("float3_3"), 0.0),
            w(v3("float3_5"), 0.0), w(snow, 0.0), [wet[0], wet[1], 0.0, 0.0],
        ]
    };
    p.iter().flatten().copied().collect()
}

fn choose(m: &flver::Mesh, mtd: Option<&mtd::Mtd>, arena: &Arena) -> Layers {
    let stem = |p: &str| p.rsplit(['\\', '/']).next().unwrap_or("").split('.').next().unwrap_or("").to_lowercase();
    let mut albedo: Vec<String> = Vec::new();
    let mut normal: Vec<String> = Vec::new();
    // Slot name -> path, for the layer slots of the sat shaders.
    let mut by_slot: Vec<(String, String)> = Vec::new();
    // A FLVER whose slot names come from another variant of the shader than the MTD's (the
    // Divine Dragon's tree o258000, c5200_cherryblossoms: FLVER "Character_AMSN__Wind__snp_
    // Texture2D_2_AlbedoMap", MTD shader [Gradient] with "..__Gradient__snp_Texture2D_1_
    // AlbedoMap_0" and a second, withered set in slots 2 / 4 plus two masks) matches no slot
    // by name. Then the MTD's own first albedo / normal "_0" slot stands in (the bloom, not
    // the withered set; gap: the gradient between the two, a shader float). Only when no
    // slot matches at all, so a material with one unused slot keeps its layers as they are.
    let variant_mismatch = mtd.is_some_and(|d| !m.textures.iter().any(|(param, _)| d.textures.iter().any(|t| &t.kind == param)));
    let stand_in = |d: &mtd::Mtd, param: &str| -> Option<String> {
        let want = ["AlbedoMap", "NormalMap"].into_iter().find(|k| param.contains(k))?;
        let slot_no = |k: &str| k.rsplit("Texture2D_").next().and_then(|t| t.split('_').next()).and_then(|n| n.parse::<u32>().ok()).unwrap_or(99);
        d.textures.iter().filter(|t| t.kind.contains(want) && t.kind.ends_with("_0") && !t.path.is_empty()).min_by_key(|t| slot_no(&t.kind)).map(|t| stem(&t.path))
    };
    for (param, tex) in &m.textures {
        let path = if tex.is_empty() {
            mtd.and_then(|d| d.textures.iter().find(|t| &t.kind == param).map(|t| stem(&t.path)).or_else(|| if variant_mismatch { stand_in(d, param) } else { None })).unwrap_or_default()
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
    // The Divine Dragon arena's cloud sea (m600000, M[Flow]_Smoke, kb/map.md): no albedo, a
    // height / blend mask (slots 11 and 5, m25_00_smoke_g) and a flow vector map (slot 12,
    // m25_00_SmokeFlow_v) the shader scrolls. Kept as mask + normal with alpha 3 ("flow
    // smoke"); map.rs draws a stand-in (map_material.wgsl `smoke`).
    if mtd.is_some_and(|d| d.shader.to_lowercase().contains("flow]_smoke")) {
        let (mask, normal) = (slot("texture2d_11_heightmap"), slot("texture2d_12_vectormap"));
        if !mask.is_empty() {
            return Layers { albedo: String::new(), normal, over_albedo: String::new(), over_normal: String::new(), snow_albedo: String::new(), snow_normal: String::new(), c_albedo: String::new(), c_normal: String::new(), mask, alpha: 3, two_sided: true, ..Default::default() };
        }
    }
    // The game path: M[Multiple] / M[MultipleGround] with their decompiled blend (bin v5).
    // Slots by number, kept only when the material's variant (GXFT_SATStaticSwitch) samples
    // them. `MAP_OLD_LAYERS=1` exports the first reading (kind 0) instead.
    let shader = mtd.map(|d| d.shader.to_lowercase()).unwrap_or_default();
    let kind: u8 = if shader.ends_with("\\m[multipleground].spx") { 2 } else if shader.ends_with("\\m[multiple].spx") { 1 } else { 0 };
    if let (Some(d), true) = (mtd, kind > 0 && std::env::var("MAP_OLD_LAYERS").is_err()) {
        let variant = d.int("GXFT_SATStaticSwitch").unwrap_or(0).clamp(0, 33) as usize;
        let used = if kind == 2 { GROUND_SLOTS[variant] } else { MULTIPLE_SLOTS[variant] };
        let at = |n: u8, what: &str| -> String {
            if !used.contains(&n) {
                return String::new();
            }
            let want = format!("texture2d_{n}_{what}");
            by_slot.iter().find(|(p, _)| p.ends_with(&want) || p.ends_with(&format!("{want}_0"))).map(|(_, t)| t.clone()).unwrap_or_default()
        };
        let (mut alpha, mut two_sided) = (0u8, false);
        let alpha_ref = d.int("g_AlphaRef").unwrap_or(0);
        let blend = d.int("g_BlendMode").unwrap_or(0);
        if blend == 2 || blend >= 4 {
            alpha = 2;
        } else if alpha_ref > 0 || blend == 1 {
            alpha = 1;
        }
        two_sided = two_sided || d.int("g_DoubleSided").unwrap_or(0) != 0 || alpha != 0 || !m.cull;
        let ground = kind == 2;
        return Layers {
            albedo: at(8, "albedomap"),
            normal: at(3, "normalmap"),
            over_albedo: at(5, "albedomap"),
            over_normal: at(4, "normalmap"),
            snow_albedo: if ground { at(11, "albedomap") } else { at(12, "albedomap") },
            snow_normal: if ground { at(7, "normalmap") } else { at(10, "normalmap") },
            c_albedo: if ground { at(1, "albedomap") } else { at(7, "albedomap") },
            c_normal: if ground { at(6, "normalmap") } else { String::new() },
            mask: at(9, "mask3map"),
            alpha,
            two_sided,
            kind,
            detail: if ground { at(12, "normalmap") } else { at(0, "normalmap") },
            mask_l: if ground { String::new() } else { at(6, "mask3map") },
            wet: if ground { at(0, "normalmap") } else { at(16, "normalmap") },
            params: game_params(d, kind, arena).into_iter().map(f32::to_bits).collect(),
        };
    }
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
    // The face set's own CullBackfaces flag (flver.rs): off = two-sided whatever the MTD says.
    two_sided |= !m.cull;
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
        ..Default::default()
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
    // `MAP_CENTRE_YAW=<degrees>` turns the arena about the centre part instead of taking his
    // MSB pose (the Headless Ape faces the pit's wall 3 m off; his fight region runs the other
    // way: tools/boss_arenas.py).
    let yaw_add: f32 = std::env::var("MAP_CENTRE_YAW").ok().and_then(|v| v.parse().ok()).unwrap_or(0.0);
    let rot = [c.rot[0], c.rot[1] + yaw_add, c.rot[2]];
    let facing = msb::rotate(rot, [0.0, 0.0, 1.0]);
    // `MAP_CENTRE_DIST=<m>` puts the origin that far in front of the centre part instead of
    // 4 m: the fight then stands away from his MSB spot (the Divine Dragon's is in a cluster
    // of rock and tree pieces).
    let dist: f32 = std::env::var("MAP_CENTRE_DIST").ok().and_then(|v| v.parse().ok()).unwrap_or(4.0);
    let origin = [c.pos[0] + dist * facing[0], c.pos[1], c.pos[2] + dist * facing[2]];
    let arena = Arena { origin, yaw: -rot[1].to_radians() };
    // `MAP_PICK=<entity id>` keeps the pieces within the radius of that part instead of the
    // origin: a fight spread over a building away from its centre part (the Folding Screen
    // Monkeys' bar is a dummy, 2000800, at one end of the temple hall they run through;
    // tools/boss_arenas.py). The arena frame stays on the centre part.
    let pick: [f32; 3] = match std::env::var("MAP_PICK").ok().and_then(|v| v.parse::<i32>().ok()) {
        Some(id) => msb.parts.iter().find(|p| p.entity == id).unwrap_or_else(|| panic!("MAP_PICK: no part with entity {id}")).pos,
        None => origin,
    };
    println!("centre {} ({}) at {:?} yaw {:.1}{}; arena origin {:?}, facing {:?} -> {:?}", c.name, c.model, c.pos, c.rot[1], if yaw_add != 0.0 { format!(" turned {yaw_add:+.0}") } else { String::new() }, origin, facing, arena.dir(facing));
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
        let near = world.iter().any(|v| (v[0] - pick[0]).hypot(v[2] - pick[2]) <= radius + 10.0);
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
        if std::env::var("MAP_LOG").is_ok() {
            // Which part owns a triangle: a .hit triangle's vertex index falls in [base, base + n).
            let mats: std::collections::BTreeSet<u32> = mesh.materials.iter().copied().collect();
            let (lo, hi) = hit_verts[base as usize..].iter().fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), v| (lo.min(v[1]), hi.max(v[1])));
            println!("hit part {} {} verts {}..{} tris {} y {:.2}..{:.2} materials {:?} draw groups {:08x?}", p.name, p.model, base, hit_verts.len(), mesh.indices.len() / 3, lo, hi, mats, p.draw_groups);
        }
    }
    let under_part = under.map(|(_, pi)| &msb.parts[pi]);
    match under_part {
        Some(p) => println!("collision under the centre: {} ({}), {:.2} m below, draw groups {:08x?}, display groups {:08x?}, light set {}", p.name, p.model, under.unwrap().0, p.draw_groups, p.display_groups, p.gparam[0]),
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
    // Pieces whose top is more than `MAP_SHADOW_BELOW` m (default 6) under the centre's floor cast
    // none either: with the sun above the horizon they can only shade ground further down, far
    // below the fight (the castle top: the keep's lower floors and the town under it; 2 cascades
    // over its 5.3 M triangles ran at 48 fps, 105 without shadows).
    let shadow_below: f32 = std::env::var("MAP_SHADOW_BELOW").ok().and_then(|v| v.parse().ok()).unwrap_or(6.0);
    let mut low_pieces = 0;
    // `MAP_FAR=<m>` (default 0 = off): the map's distant-view models (m6xxxxx) and sky / cloud
    // pieces (m9xxxxx) within that many metres of the centre join at their lowest LOD, so the
    // horizon beyond the arena radius is not empty (the castle seen from the gate). Past
    // MAP_SHADOW they cast no shadows; objects and ordinary pieces are not affected.
    let far_m: f32 = std::env::var("MAP_FAR").ok().and_then(|v| v.parse().ok()).unwrap_or(0.0);
    let mut far_pieces = 0;
    // `MAP_CELL=<m>` splits the batches by cells of the piece's centre so that the camera and
    // the shadow cascades can cull them; off by default: 20 m cells (1016 meshes) cost more
    // per entity than the culling saved (39 fps against 56, kb/map.md).
    let cell_m: f32 = std::env::var("MAP_CELL").ok().and_then(|v| v.parse().ok()).unwrap_or(0.0);
    let mut batches: BTreeMap<(Layers, bool, i32, i32), Batch> = BTreeMap::new();
    let mut pieces = 0;
    let mut tris = 0usize;
    let mut skipped_groups = 0;
    let mut mirrored_pieces = 0;
    // Map pieces (part type 0, the map's mapbnds) and objects (type 1: trees, fences, the
    // Divine Dragon's arena; `obj/<model>.objbnd`, whose rigid meshes are in their bone's space,
    // flver.rs SEKIRO_FLVER_REF_POSE). An object not unpacked yet is listed for `unpack`.
    // The objects' own collision (the objbnd's hkx) joins the .hit (below).
    let obj_dir = root.join("obj");
    // ObjectParam materialId per object id (json/params/ObjectParam.json, -1 none): the hit
    // material of an object's collision ("treated the same as the floor material"), for the
    // hkx shapes that carry none (the convex ones never do). 29 "None" when both are missing,
    // so the loader does not take the shape for a helper plane. ObjectParam collisionType
    // (0 / 1 / 4 / 8 / 10 on this map) is not applied: Sekiro's enum is not in Paramdex (gap;
    // the AC6 one reads 0 normal, 1 no camera, 2 no camera/lock/damage, 3 camera only, 4
    // damage only).
    let obj_materials: HashMap<i64, u32> = std::fs::read_to_string(root.join("json/params/ObjectParam.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .and_then(|j| j["rows"].as_array().cloned())
        .map(|rows| rows.iter().filter_map(|r| Some((r["id"].as_i64()?, u32::try_from(r["materialId"].as_i64()?).ok()?))).collect())
        .unwrap_or_default();
    // Meshes the game does not show near the centre although their part passes the draw
    // groups: `MAP_SKIP_MESH=<part>:<mesh index>[,...]` (an experiment) and HIDDEN_MESHES.
    let hidden_meshes: Vec<(String, usize)> = std::env::var("MAP_SKIP_MESH")
        .map(|v| v.split(',').filter_map(|s| s.split_once(':').and_then(|(p, i)| Some((p.to_string(), i.parse().ok()?)))).collect::<Vec<_>>())
        .unwrap_or_default()
        .into_iter()
        .chain(HIDDEN_MESHES.iter().filter(|(m, _, _)| *m == map_id).map(|(_, p, i)| (p.to_string(), *i)))
        .collect();
    // Whole models left out (`MAP_SKIP_MODEL=<model>[,...]`, case-insensitive): overlapping
    // event-swapped floors, e.g. the castle top's burnt floor m015072 over the tatami m015070
    // that the Owl and Genichiro fights show (event 11115943 swaps them after a later cutscene;
    // AKJama/sekiro-rs crates/sim/src/arena.rs `hidden_models`).
    let skip_models: Vec<String> = std::env::var("MAP_SKIP_MODEL").map(|v| v.split(',').map(|s| s.trim().to_lowercase()).filter(|s| !s.is_empty()).collect()).unwrap_or_default();
    let mut objects_missing: std::collections::BTreeSet<String> = Default::default();
    let mut objects = 0;
    let mut objects_used: std::collections::BTreeSet<String> = Default::default();
    let mut object_hits = 0;
    // Doors and gates an ObjAct event works (msb.objact_parts) are left out, mesh and collision:
    // the game opens them through the ObjAct and its animation, which we do not run, so their
    // bind pose is the closed door. The gate map's o111410_8000 stood closed between Wolf's
    // start (0, 4) and the line-up (0, -4), and its hkx wall kept every regular fight from
    // starting (2026-10-12). `MAP_KEEP_OBJACT=1` keeps them.
    let keep_objact = std::env::var("MAP_KEEP_OBJACT").is_ok();
    let mut objact_skipped = 0;
    for (pi, p) in msb.parts.iter().enumerate().filter(|(_, p)| p.kind == 0 || p.kind == 1) {
        if !keep_objact && msb.objact_parts.contains(&pi) {
            if std::env::var("MAP_LOG").is_ok() && (p.pos[0] - pick[0]).hypot(p.pos[2] - pick[2]) <= radius {
                println!("objact part {} {} left out", p.name, p.model);
            }
            objact_skipped += 1;
            continue;
        }
        if !groups.is_empty() && !p.draws_with(&groups) {
            skipped_groups += 1;
            if std::env::var("MAP_LOG").is_ok() && (p.pos[0] - pick[0]).hypot(p.pos[2] - pick[2]) <= radius {
                println!("  not drawn there: {} ({}) draw groups {:08x?}", p.name, p.model, p.draw_groups);
            }
            continue;
        }
        if skip_models.contains(&p.model.to_lowercase()) {
            continue;
        }
        let object = p.kind == 1;
        let path = if object {
            if (p.pos[0] - pick[0]).hypot(p.pos[2] - pick[2]) > radius + 60.0 {
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
        let (mut lo, mut hi, mut top) = ([f32::MAX; 2], [f32::MIN; 2], f32::MIN);
        for v in f.meshes.iter().flat_map(|m| m.vertices.iter()) {
            let w = p.transform(v.pos);
            lo = [lo[0].min(w[0]), lo[1].min(w[2])];
            hi = [hi[0].max(w[0]), hi[1].max(w[2])];
            top = top.max(w[1]);
        }
        let low = top < origin[1] - shadow_below;
        if low && dist_cast_counts(&lo, &hi, origin, shadow_m) {
            low_pieces += 1;
        }
        if lo[0] > hi[0] {
            continue;
        }
        let dx = (lo[0] - pick[0]).max(pick[0] - hi[0]).max(0.0);
        let dz = (lo[1] - pick[2]).max(pick[2] - hi[1]).max(0.0);
        let dist = dx.hypot(dz);
        let far_piece = dist > radius && !object && (p.model.starts_with("m6") || p.model.starts_with("m9")) && dist <= far_m;
        if dist > radius && !far_piece {
            continue;
        }
        far_pieces += far_piece as usize;
        // Level of detail by distance, as the engine swaps them: full within 30 m, LodLevel1
        // to 55 m, LodLevel2 beyond (pieces without a level keep the next lower one).
        // `MAP_LOD=<full m>,<lod1 m>` overrides (docs/kb/map.md, frame-rate tuning).
        let lod = if dist <= lod_dist[0] { 0 } else if dist <= lod_dist[1] { 1 } else { 2 };
        if lod > 0 {
            f = read(lod);
        }
        pieces += 1;
        objects += object as usize;
        if object {
            objects_used.insert(p.model.to_lowercase());
            // The object's own collision (`<model>.hkx` in the objbnd: convex boxes / polytopes
            // / capsules, or a compressed mesh): Wolf stands on roofs and branches, the
            // grapple landing spots. Not for the item pickups (MSB Treasure parts: the o000100
            // marker's 1.2 m box would block the courtyard 3 m from the General); skipped
            // altogether when `MAP_NO_OBJECT_HIT` is set.
            if std::env::var("MAP_NO_OBJECT_HIT").is_err() && !msb.treasure_parts.contains(&pi) {
                let m = p.model.to_lowercase();
                if let Ok(hk) = std::fs::read(obj_dir.join(format!("{m}.objbnd.d/{m}.hkx"))) {
                    let mesh = hkx::collision_mesh(&hk, &compendium);
                    if !mesh.indices.is_empty() {
                        object_hits += 1;
                        let base = hit_verts.len() as u32;
                        hit_verts.extend(mesh.vertices.iter().map(|v| arena.point(p.transform(*v))));
                        let param_mat = m[1..].parse::<i64>().ok().and_then(|id| obj_materials.get(&id).copied()).unwrap_or(29);
                        let mats: Vec<u32> = mesh.materials.iter().map(|&mt| if mt == u32::MAX { param_mat } else { mt }).collect();
                        for t in 0..mesh.indices.len() / 3 {
                            hit_tris.push([base + mesh.indices[t * 3], base + mesh.indices[t * 3 + 1], base + mesh.indices[t * 3 + 2], mats[t]]);
                        }
                        if std::env::var("MAP_LOG").is_ok() {
                            let set: std::collections::BTreeSet<u32> = mats.iter().copied().collect();
                            let (lo, hi) = mesh.vertices.iter().fold(([f32::MAX; 3], [f32::MIN; 3]), |(lo, hi), v| ([lo[0].min(v[0]), lo[1].min(v[1]), lo[2].min(v[2])], [hi[0].max(v[0]), hi[1].max(v[1]), hi[2].max(v[2])]));
                            println!("object hit {} {} tris {} verts {}.. materials {:?} local box {:.2?}..{:.2?}", p.name, m, mesh.indices.len() / 3, base, set, lo, hi);
                        }
                    }
                }
            }
        }
        let piece_tris: usize = f.meshes.iter().map(|m| m.indices.len() / 3).sum();
        if std::env::var("MAP_LOG").is_ok() {
            let (a, b) = (arena.point([lo[0], 0.0, lo[1]]), arena.point([hi[0], 0.0, hi[1]]));
            println!("piece {} {} tris {} dist {:.0} lod {lod} pos {:?} rot {:?} scale {:?} arena xz {:.0},{:.0}..{:.0},{:.0} draw {:08x?} display {:08x?}", p.name, p.model, piece_tris, dx.hypot(dz), p.pos, p.rot, p.scale, a[0].min(b[0]), a[2].min(b[2]), a[0].max(b[0]), a[2].max(b[2]), p.draw_groups, p.display_groups);
        }
        for (mi, m) in f.meshes.iter().enumerate() {
            if skip_material(&m.mtd) || hidden_meshes.iter().any(|(part, i)| *i == mi && *part == p.name) {
                continue;
            }
            let key = m.mtd.to_lowercase();
            let mtd = mtds.entry(key.clone()).or_insert_with(|| std::fs::read(mtd_dir.join(format!("{key}.mtd"))).ok().and_then(|d| mtd::read(&d))).as_ref();
            let layers = choose(m, mtd, &arena);
            if std::env::var("MAP_LOG").is_ok() {
                println!("piece-mesh {} {} lod {} albedo {} [{}] tris {} cull {}", p.name, p.model, lod, layers.albedo, m.mtd, m.indices.len() / 3, m.cull);
            }
            if layers.albedo.is_empty() && std::env::var("MAP_LOG").is_ok() {
                println!("noalbedo {} [{}] shader {} slots {:?} mtd slots {:?}", m.material, m.mtd, mtd.map_or("?", |d| d.shader.as_str()), m.textures, mtd.map(|d| d.textures.iter().map(|t| (t.kind.clone(), t.path.clone())).collect::<Vec<_>>()));
            }
            let centre = [(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5];
            let cell = |v: f32| if cell_m > 0.0 { (v / cell_m).floor() as i32 } else { 0 };
            let b_kind = layers.kind;
            let b = batches.entry((layers, dist <= shadow_m && !low, cell(centre[0]), cell(centre[1]))).or_default();
            let base = b.pos.len() as u32;
            // A mirrored part (odd number of negative scale axes, e.g. the Guardian Ape arena's
            // pond floor m17_00_00_00 ground piece) turns its triangles inside out: flip the
            // winding so back-face culling keeps the floor, and mirror the normals with it.
            let mirrored = p.scale.iter().filter(|s| **s < 0.0).count() % 2 == 1;
            let sign = p.scale.map(|s| if s < 0.0 { -1.0 } else { 1.0 });
            if mirrored {
                mirrored_pieces += 1;
            }
            for v in &m.vertices {
                b.pos.push(arena.point(p.transform(v.pos)));
                let n = arena.dir(msb::rotate(p.rot, [v.normal[0] * sign[0], v.normal[1] * sign[1], v.normal[2] * sign[2]]));
                let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-6);
                b.normal.push([n[0] / l, n[1] / l, n[2] / l]);
                b.uv.push(v.uv);
                b.uv2.push(v.uv2);
                // Blend weights, as the decompiled shaders read them (kb/map.md "The sat layer
                // shaders"): b0 = blend[0].R overlay, b1 = blend[1].B layer L (ground .G),
                // b4 = blend[4].G snow, b5 = blend[5].B detail strength; then b2 = blend[2].RGB
                // tint, b3 = blend[3].R wet. The first reading (kind 0) takes bytes 0 / 1 / 4 / 2
                // in the first four and ignores the rest.
                let bl = &v.blend;
                b.blend.push(if b_kind > 0 {
                    [bl[0][0], bl[1][if b_kind == 2 { 1 } else { 2 }], bl[4][1], bl[5][2], bl[2][0], bl[2][1], bl[2][2], bl[3][0]]
                } else {
                    [bl[0][0], bl[1][0], bl[4][0], bl[2][0], 255, 255, 255, 0]
                });
                b.uv_scale.push(p.scale[0].abs());
            }
            if mirrored {
                b.indices.extend(m.indices.chunks_exact(3).flat_map(|t| [base + t[0], base + t[2], base + t[1]]));
            } else {
                b.indices.extend(m.indices.iter().map(|i| base + i));
            }
            tris += m.indices.len() / 3;
        }
    }
    println!("{pieces} pieces ({objects} objects, {object_hits} with their own collision; {far_pieces} far-view pieces beyond the radius; {skipped_groups} outside the draw groups; {mirrored_pieces} mirrored; {low_pieces} near pieces below the floor cast no shadows; {objact_skipped} ObjAct doors left out), {} batches, {tris} triangles", batches.len());
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
    // Version 5: nine texture names per batch (base, overlay, snow, layer C / L albedo /
    // normal, the base mask), the game path (kind byte, detail / layer mask / wet names, 64
    // f32 constants), a caster byte, and 52-byte vertices (pos, normal, uv, uv2, 8 blend bytes,
    // the uv scale).
    b.extend_from_slice(b"SHMP");
    b.extend_from_slice(&5u32.to_le_bytes());
    b.extend_from_slice(&(batches.len() as u32).to_le_bytes());
    let mut wanted: HashSet<String> = HashSet::new();
    for ((l, casts, _, _), m) in &batches {
        for t in [&l.albedo, &l.normal, &l.over_albedo, &l.over_normal, &l.snow_albedo, &l.snow_normal, &l.c_albedo, &l.c_normal, &l.mask] {
            if !t.is_empty() {
                wanted.insert(t.clone());
            }
            put_str(&mut b, t);
        }
        b.push(l.kind);
        for t in [&l.detail, &l.mask_l, &l.wet] {
            if !t.is_empty() {
                wanted.insert(t.clone());
            }
            put_str(&mut b, t);
        }
        let params: Vec<f32> = if l.params.len() == 64 { l.params.iter().map(|x| f32::from_bits(*x)).collect() } else { vec![0.0; 64] };
        put_f(&mut b, &params);
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
            put_f(&mut b, &[m.uv_scale[i]]);
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
    // Other draw param files of the block (`MAP_GPARAM_EXTRA=0740,0750`: the `<block>_<v>.gparam`
    // variants the area's event scripts switch to for a fight, kb/map.md "Castle top lighting"),
    // written whole under "drawparam_variants" so the loader can pick one per fight.
    let variants: serde_json::Map<String, serde_json::Value> = std::env::var("MAP_GPARAM_EXTRA")
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .filter_map(|v| {
            let path = root.join(format!("param/drawparam/{}_{v}.gparam", &map_id[..6]));
            match std::fs::read(&path) {
                Ok(d) => Some((v.to_string(), gparam::to_json(&gparam::read(&d)))),
                Err(_) => {
                    println!("no {}", path.display());
                    None
                }
            }
        })
        .collect();
    if !variants.is_empty() {
        println!("draw param variants {:?}", variants.keys().collect::<Vec<_>>());
    }
    let env_probe = export_env(root, map_id, name, &msb, &arena);
    let luts = export_luts(root, map_id, name);
    let (grapple, ledges) = export_grapple(root, map_id, &msb, &arena, pick, radius, &obj_dir);
    let json = serde_json::json!({
        "grapple": grapple,
        "ledges": ledges,
        "env_probe": env_probe,
        "env_hours": ENV_HOURS,
        "luts": luts,
        "map": map_id,
        "centre": { "name": c.name, "model": c.model, "npc": c.npc, "pos": c.pos, "yaw": rot[1], "turned": yaw_add },
        "origin": origin,
        "arena_yaw": arena.yaw,
        "collision_part": under_part.map(|p| p.name.clone()),
        "sound_space_type": under_part.map(|p| p.sound_space),
        "light_set": under_part.map(|p| p.gparam[0]).unwrap_or(0),
        "gparam": under_part.map(|p| p.gparam).unwrap_or([-1; 4]),
        "draw_groups": groups,
        "drawparam": draw,
        "drawparam_variants": variants,
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
    // A texture named after another area (the Headless Ape's ground uses m13's
    // m13_ground_900) comes from that area's packs when they are unpacked (tools/boss_arenas.py
    // unpacks them on the exporter's "missing" report and runs again).
    let mut areas: Vec<String> = vec![area.to_string()];
    for n in &wanted {
        if n.len() > 4 && n.starts_with('m') && n.as_bytes()[3] == b'_' && n[1..3].bytes().all(|b| b.is_ascii_digit()) && !areas.iter().any(|a| *a == n[..3]) {
            areas.push(n[..3].to_string());
        }
    }
    for a in &areas {
        for e in std::fs::read_dir(root.join(format!("map/{a}"))).into_iter().flatten().flatten() {
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

/// Would the piece cast by distance alone (within `shadow_m` of the centre)? For the count of
/// pieces the height rule (`MAP_SHADOW_BELOW`) takes out.
fn dist_cast_counts(lo: &[f32; 2], hi: &[f32; 2], origin: [f32; 3], shadow_m: f32) -> bool {
    let dx = (lo[0] - origin[0]).max(origin[0] - hi[0]).max(0.0);
    let dz = (lo[1] - origin[2]).max(origin[2] - hi[1]).max(0.0);
    dx.hypot(dz) <= shadow_m
}
