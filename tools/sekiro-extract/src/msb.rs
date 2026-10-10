//! Sekiro map layouts (MSB, "MSBS" in SoulsFormats): the model list and the parts that place
//! them (map pieces, objects, enemies, collisions). Layout follows SoulsFormatsNEXT
//! Formats/MSB/MSBS (MSBS.cs param framing, ModelParam.cs, PartsParam.cs Part / EnemyBase).

use crate::bin::Reader;

pub struct Part {
    pub name: String,
    /// PartType: 0 map piece, 1 object, 2 enemy, 4 player, 5 collision, 9 / 10 dummies,
    /// 11 connect collision.
    pub kind: u32,
    /// Model name ("m300000", "c1020", "h000100", "o000100"); empty when the part has none.
    pub model: String,
    pub pos: [f32; 3],
    /// Euler degrees, applied X then Z then Y (as FLVER nodes).
    pub rot: [f32; 3],
    pub scale: [f32; 3],
    /// Enemies: NpcThinkParam / NpcParam ids (EnemyBase type data +0x08 / +0x0C).
    pub think: i32,
    pub npc: i32,
    /// Collisions: HitFilterID (type data +0x00).
    pub hit_filter: u8,
    /// Draw groups (8 words of the 48-word mask block at unkOffset1: display[8], draw[8],
    /// collision mask[32] in DSMapStudio's split). Empty when the part has no block.
    pub draw_groups: Vec<u32>,
    pub display_groups: Vec<u32>,
    /// GparamConfig: LightSetID, FogParamID, LightScatteringID, EnvMapID (-1 when absent).
    pub gparam: [i32; 4],
}

impl Part {
    /// Two parts' draw groups share a bit.
    pub fn draws_with(&self, groups: &[u32]) -> bool {
        self.draw_groups.iter().zip(groups).any(|(a, b)| a & b != 0)
    }
}

/// A point / volume of POINT_PARAM_ST (PointParam.cs Region). Used for the environment map
/// probes: `EnvironmentMapPoint` (type 2) and `EnvironmentMapEffectBox` (type 17), both named
/// `Env_Point<probe>` / `Env_Box<probe>_...` after the GI probe textures (`gilm<probe>`).
pub struct Region {
    pub name: String,
    pub kind: u32,
    pub id: i32,
    pub pos: [f32; 3],
    pub rot: [f32; 3],
    /// MSB shape: 0 point, 1 circle, 2 sphere, 3 cylinder, 4 rectangle, 5 box.
    pub shape: u32,
    /// Box: width (x), depth (z), height (y); sphere / cylinder: radius in [0].
    pub size: [f32; 3],
}

impl Region {
    /// The probe number in the region name (Env_Box380_... -> 380).
    pub fn probe(&self) -> Option<u32> {
        let digits: String = self.name.trim_start_matches("Env_Box").trim_start_matches("Env_Point").chars().take_while(|c| c.is_ascii_digit()).collect();
        digits.parse().ok()
    }

    /// A world point inside this box (boxes are centred on the region position and turned by
    /// its yaw) or sphere.
    pub fn contains(&self, p: [f32; 3]) -> bool {
        let d = [p[0] - self.pos[0], p[1] - self.pos[1], p[2] - self.pos[2]];
        match self.shape {
            5 => {
                let l = rotate([0.0, -self.rot[1], 0.0], d);
                l[0].abs() <= self.size[0] * 0.5 && l[2].abs() <= self.size[1] * 0.5 && l[1].abs() <= self.size[2] * 0.5
            }
            2 => d[0] * d[0] + d[1] * d[1] + d[2] * d[2] <= self.size[0] * self.size[0],
            _ => false,
        }
    }

    pub fn volume(&self) -> f32 {
        match self.shape {
            5 => self.size[0] * self.size[1] * self.size[2],
            2 => self.size[0].powi(3) * 4.19,
            _ => f32::MAX,
        }
    }
}

pub struct Msb {
    pub parts: Vec<Part>,
    pub regions: Vec<Region>,
}

/// Each param: version, offsetCount, nameOffset, (offsetCount - 1) entry offsets, next param offset.
fn params(r: &Reader) -> Vec<(String, Vec<usize>)> {
    let mut o = 0x10; // MSB header ("MSB ", 1, 0x10, ...)
    let mut out = Vec::new();
    loop {
        let count = r.u32(o + 4) as usize;
        let name = r.utf16z(r.u64(o + 8) as usize);
        let entries = (0..count.saturating_sub(1)).map(|i| r.u64(o + 16 + i * 8) as usize).collect();
        let next = r.u64(o + 16 + count.saturating_sub(1) * 8) as usize;
        out.push((name, entries));
        if next == 0 {
            break;
        }
        o = next;
    }
    out
}

pub fn read(d: &[u8]) -> Msb {
    assert_eq!(&d[0..4], b"MSB ", "not an MSB");
    let r = Reader::new(d);
    let ps = params(&r);
    let get = |n: &str| ps.iter().find(|(name, _)| name == n).map(|(_, e)| e.clone()).unwrap_or_default();
    // Model: nameOffset, type, id, sibOffset, instanceCount, unk, typeDataOffset.
    let models: Vec<String> = get("MODEL_PARAM_ST").iter().map(|&o| r.utf16z(o + r.u64(o) as usize)).collect();
    let v3 = |o: usize| [r.f32(o), r.f32(o + 4), r.f32(o + 8)];
    let parts = get("PARTS_PARAM_ST")
        .iter()
        .map(|&o| {
            // Part: nameOffset, type, id, modelIndex, 0, sibOffset, position, rotation, scale,
            // -1, -1, 0, unk1, unk2, entityData, typeData (offsets from the entry start).
            let kind = r.u32(o + 8);
            let model_index = r.i32(o + 16);
            let unk1 = r.u64(o + 0x50) as usize;
            let td = o + r.u64(o + 0x68) as usize;
            let gp = r.u64(o + 0x70) as usize;
            let words = |at: usize, n: usize| (0..n).map(|i| r.u32(at + i * 4)).collect::<Vec<u32>>();
            let (think, npc) = if kind == 2 || kind == 10 { (r.i32(td + 8), r.i32(td + 12)) } else { (0, 0) };
            Part {
                name: r.utf16z(o + r.u64(o) as usize),
                kind,
                model: usize::try_from(model_index).ok().and_then(|i| models.get(i).cloned()).unwrap_or_default(),
                pos: v3(o + 0x20),
                rot: v3(o + 0x2C),
                scale: v3(o + 0x38),
                think,
                npc,
                hit_filter: if kind == 5 { r.u8(td) } else { 0 },
                display_groups: if unk1 != 0 { words(o + unk1, 8) } else { Vec::new() },
                draw_groups: if unk1 != 0 { words(o + unk1 + 32, 8) } else { Vec::new() },
                gparam: if gp != 0 { [r.i32(o + gp), r.i32(o + gp + 4), r.i32(o + gp + 8), r.i32(o + gp + 12)] } else { [-1; 4] },
            }
        })
        .collect();
    // Region: nameOffset, type, id, shapeType, position, rotation, unk2C, baseDataOffset1,
    // baseDataOffset2, -1, layer, shapeDataOffset, baseDataOffset3, typeDataOffset.
    let regions = get("POINT_PARAM_ST")
        .iter()
        .map(|&o| {
            let shape = r.u32(o + 0x10);
            let sd = r.u64(o + 0x48) as usize;
            let size = match (shape, sd) {
                (5, s) if s != 0 => v3(o + s),
                (1 | 2 | 3, s) if s != 0 => [r.f32(o + s), 0.0, 0.0],
                _ => [0.0; 3],
            };
            Region { name: r.utf16z(o + r.u64(o) as usize), kind: r.u32(o + 8), id: r.i32(o + 12), pos: v3(o + 0x14), rot: v3(o + 0x20), shape, size }
        })
        .collect();
    Msb { parts, regions }
}

/// Column-vector rotation of a part: Ry * Rz * Rx (degrees), i.e. SoulsFormats' row-vector
/// X * Z * Y order, the same as FLVER nodes.
pub fn rotate(rot: [f32; 3], v: [f32; 3]) -> [f32; 3] {
    let (sx, cx) = rot[0].to_radians().sin_cos();
    let (sy, cy) = rot[1].to_radians().sin_cos();
    let (sz, cz) = rot[2].to_radians().sin_cos();
    // X
    let v = [v[0], v[1] * cx - v[2] * sx, v[1] * sx + v[2] * cx];
    // Z
    let v = [v[0] * cz - v[1] * sz, v[0] * sz + v[1] * cz, v[2]];
    // Y
    [v[0] * cy + v[2] * sy, v[1], -v[0] * sy + v[2] * cy]
}

impl Part {
    pub fn transform(&self, v: [f32; 3]) -> [f32; 3] {
        let s = [v[0] * self.scale[0], v[1] * self.scale[1], v[2] * self.scale[2]];
        let r = rotate(self.rot, s);
        [r[0] + self.pos[0], r[1] + self.pos[1], r[2] + self.pos[2]]
    }
}
