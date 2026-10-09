//! TAE (TimeAct) files, Sekiro format 0x1000D, decoded with DSAnimStudio's
//! TAE.Template.SDT.xml for event names and argument layouts.

use serde_json::{Map, Value, json};
use std::collections::HashMap;
use std::path::Path;

use crate::bin::Reader;

pub struct EventTemplate {
    pub name: String,
    /// (type, name, enum value -> label)
    pub args: Vec<(String, String, HashMap<i64, String>)>,
}

pub fn load_template(path: &Path) -> HashMap<i32, EventTemplate> {
    let text = std::fs::read_to_string(path).unwrap();
    let doc = roxmltree::Document::parse(&text).unwrap();
    let mut out = HashMap::new();
    for action in doc.descendants().filter(|n| n.has_tag_name("action")) {
        let id: i32 = action.attribute("id").unwrap().parse().unwrap();
        let args = action
            .children()
            .filter(|n| n.is_element())
            .enumerate()
            .map(|(i, n)| {
                let enums = n
                    .children()
                    .filter(|e| e.has_tag_name("entry"))
                    .filter_map(|e| Some((e.attribute("value")?.parse().ok()?, e.attribute("name")?.to_string())))
                    .collect();
                let name = n.attribute("name").map(str::to_string).unwrap_or(format!("unk{i}"));
                (n.tag_name().name().to_string(), name, enums)
            })
            .collect();
        out.insert(id, EventTemplate { name: action.attribute("name").unwrap_or("").to_string(), args });
    }
    out
}

fn arg_size(ty: &str) -> usize {
    match ty {
        "s8" | "u8" | "b" | "x8" => 1,
        "s16" | "u16" | "x16" => 2,
        "s32" | "u32" | "f32" | "x32" => 4,
        "s64" | "u64" | "f64" | "x64" => 8,
        _ => 0,
    }
}

fn decode_args(r: &Reader, mut o: usize, end: usize, t: Option<&EventTemplate>) -> Value {
    let Some(t) = t else {
        return json!(r.d[o..end].iter().map(|b| format!("{b:02x}")).collect::<String>());
    };
    let mut m = Map::new();
    for (ty, name, enums) in &t.args {
        let size = arg_size(ty);
        if size == 0 || o + size > r.d.len() {
            break;
        }
        let v: Value = match ty.as_str() {
            "s8" => json!(r.d[o] as i8),
            "u8" | "x8" => json!(r.d[o]),
            "b" => json!(r.d[o] != 0),
            "s16" => json!(r.u16(o) as i16),
            "u16" | "x16" => json!(r.u16(o)),
            "s32" => json!(r.i32(o)),
            "u32" | "x32" => json!(r.u32(o)),
            "f32" => json!(r.f32(o)),
            "s64" => json!(r.i64(o)),
            "u64" | "x64" => json!(r.u64(o)),
            "f64" => json!(f64::from_bits(r.u64(o))),
            _ => Value::Null,
        };
        if !name.starts_with("unk") || v.as_f64() != Some(0.0) {
            let label = v.as_i64().and_then(|i| enums.get(&i));
            m.insert(name.clone(), label.map_or(v, |l| json!(l)));
        }
        o += size;
    }
    Value::Object(m)
}

/// Returns {"id": taeId, "anims": [{id, importFrom?, events:[...]}]}.
pub fn read_tae(data: &[u8], tmpl: &HashMap<i32, EventTemplate>) -> Value {
    let r = Reader::new(data);
    assert_eq!(&data[0..4], b"TAE ");
    assert_eq!(r.u32(0x08), 0x1000D, "not a Sekiro TAE");
    let tae_id = r.i32(0x50);
    let anim_count = r.u32(0x54) as usize;
    let anims_off = r.u64(0x58) as usize;
    let mut anims = Vec::with_capacity(anim_count);
    for i in 0..anim_count {
        let a = anims_off + i * 0x10;
        let anim_id = r.u64(a);
        let off = r.u64(a + 8) as usize;
        let ev_headers = r.u64(off) as usize;
        let ev_groups = r.u64(off + 8) as usize;
        let mini = r.u64(off + 0x18) as usize;
        let ev_count = r.u32(off + 0x20) as usize;

        let mut anim = Map::new();
        anim.insert("id".into(), json!(anim_id));
        // Mini header type 1 = ImportOtherAnim (events and HKX come from another anim).
        if mini != 0 && r.u32(mini) == 1 && r.u64(mini + 8) != 0 {
            anim.insert("importFrom".into(), json!(r.i32(mini + 0x18)));
        }
        // Mini header type 0 = Standard: +0x18 flags (IsLoopByDefault / ImportsHKX / AllowDelayLoad),
        // +0x1c ImportHKXSourceAnimID (-1 = own clip) - observed in Sekiro's TAEs.
        if mini != 0 && r.u32(mini) == 0 {
            let src = r.i32(mini + 0x1c);
            if src >= 0 {
                anim.insert("hkxFrom".into(), json!(src));
            }
        }
        let mut param_offsets = Vec::with_capacity(ev_count);
        let mut events = Vec::with_capacity(ev_count);
        for e in 0..ev_count {
            let h = ev_headers + e * 0x18;
            let start = r.f32(r.u64(h) as usize);
            let end = r.f32(r.u64(h + 8) as usize);
            let data_off = r.u64(h + 0x10) as usize;
            let ty = r.i32(data_off);
            let params = r.u64(data_off + 8) as usize;
            param_offsets.push(if params == 0 { data_off + 0x10 } else { params });
            events.push((start, end, ty));
        }
        let mut out_events = Vec::with_capacity(ev_count);
        for (e, &(start, end, ty)) in events.iter().enumerate() {
            let p = param_offsets[e];
            let limit = if e + 1 < ev_count {
                ev_headers.max(param_offsets[e + 1].saturating_sub(0x10))
            } else if ev_groups != 0 {
                ev_groups
            } else {
                (p + 0x40).min(data.len())
            };
            let t = tmpl.get(&ty);
            out_events.push(json!({
                "type": ty,
                "name": t.map_or("?", |t| t.name.as_str()),
                "start": start,
                "end": end,
                "startFrame": (start * 30.0).round(),
                "endFrame": (end * 30.0).round(),
                "args": decode_args(&r, p, limit.max(p), t),
            }));
        }
        anim.insert("events".into(), Value::Array(out_events));
        anims.push(Value::Object(anim));
    }
    json!({ "id": tae_id, "anims": anims })
}
