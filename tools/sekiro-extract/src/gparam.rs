//! Draw parameters (`param/drawparam/*.gparam`, "filt" files, Sekiro variant): light sets, fog,
//! tone mapping and other renderer settings, grouped by editor page. Each param holds values
//! tagged with a value id (0 = base; other ids are per-area variants, e.g. a collision part's
//! LightSetID) and a time of day in hours. Layout follows SoulsFormatsNEXT `GPARAM`.

use crate::bin::Reader;

pub struct Entry {
    pub id: i32,
    /// Hours, 0..24.
    pub time: f32,
    pub value: Vec<f32>,
}

pub struct Param {
    pub name: String,
    pub entries: Vec<Entry>,
}

pub struct Group {
    pub name: String,
    pub params: Vec<Param>,
}

pub fn read(d: &[u8]) -> Vec<Group> {
    let r = Reader::new(d);
    let base = if d.starts_with(b"f\0i\0l\0t\0") { 8 } else { 4 };
    assert_eq!(r.u32(base), 5, "GPARAM: not a Sekiro file");
    let group_count = r.i32(base + 8) as usize;
    let header_size = r.i32(base + 16) as usize;
    let group_headers = r.i32(base + 20) as usize;
    let param_header_offsets = r.i32(base + 24) as usize;
    let param_headers = r.i32(base + 28) as usize;
    let values = r.i32(base + 32) as usize;
    let value_ids = r.i32(base + 36) as usize;
    let utf16_len = |o: usize| {
        let mut n = 0;
        while r.u16(o + n * 2) != 0 {
            n += 1;
        }
        (n + 1) * 2
    };
    let mut groups = Vec::with_capacity(group_count);
    for g in 0..group_count {
        let go = group_headers + r.i32(header_size + g * 4) as usize;
        let param_count = r.i32(go) as usize;
        let pho = param_header_offsets + r.i32(go + 4) as usize;
        let name = r.utf16z(go + 8);
        let mut params = Vec::with_capacity(param_count);
        for p in 0..param_count {
            let ph = param_headers + r.i32(pho + p * 4) as usize;
            let vo = values + r.i32(ph) as usize;
            let io = value_ids + r.i32(ph + 4) as usize;
            let ty = r.u8(ph + 8);
            let count = r.u8(ph + 9) as usize;
            let pname = r.utf16z(ph + 12);
            let _ = utf16_len(ph + 12);
            let size = match ty {
                1 | 5 | 11 => 1,
                2 => 2,
                3 | 7 | 9 | 15 => 4,
                12..=14 => 16,
                other => panic!("GPARAM {pname}: unknown type {other}"),
            };
            let mut entries = Vec::with_capacity(count);
            for i in 0..count {
                let v = vo + i * size;
                let value = match ty {
                    1 | 5 | 11 => vec![r.u8(v) as f32],
                    2 => vec![r.i16(v) as f32],
                    3 | 7 => vec![r.i32(v) as f32],
                    9 => vec![r.f32(v)],
                    12 => vec![r.f32(v), r.f32(v + 4)],
                    13 => vec![r.f32(v), r.f32(v + 4), r.f32(v + 8)],
                    14 => vec![r.f32(v), r.f32(v + 4), r.f32(v + 8), r.f32(v + 12)],
                    _ => vec![r.u32(v) as f32],
                };
                entries.push(Entry { id: r.i32(io + i * 8), time: r.f32(io + i * 8 + 4), value });
            }
            params.push(Param { name: pname, entries });
        }
        groups.push(Group { name, params });
    }
    groups
}

pub fn to_json(groups: &[Group]) -> serde_json::Value {
    serde_json::Value::Object(
        groups
            .iter()
            .map(|g| {
                let params = g
                    .params
                    .iter()
                    .map(|p| {
                        let entries: Vec<serde_json::Value> =
                            p.entries.iter().map(|e| serde_json::json!({ "id": e.id, "time": e.time, "value": e.value })).collect();
                        (p.name.clone(), serde_json::Value::Array(entries))
                    })
                    .collect();
                (g.name.clone(), serde_json::Value::Object(params))
            })
            .collect(),
    )
}
