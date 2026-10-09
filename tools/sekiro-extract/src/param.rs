//! PARAM files (Sekiro layout: long data offsets, offset param type, UTF-16 row
//! names) decoded with Paramdex XML paramdefs.

use serde_json::{Map, Value, json};
use std::collections::HashMap;
use std::path::Path;

use crate::bin::Reader;

#[derive(Clone)]
struct Field {
    ty: String,
    name: String,
    count: usize,
    bits: Option<u32>,
}

pub struct ParamDef {
    pub param_type: String,
    fields: Vec<Field>,
}

impl ParamDef {
    pub fn load(path: &Path) -> Self {
        let text = std::fs::read_to_string(path).unwrap();
        let text = text.trim_start_matches('\u{feff}');
        let doc = roxmltree::Document::parse(text).unwrap();
        let param_type = doc
            .descendants()
            .find(|n| n.has_tag_name("ParamType"))
            .and_then(|n| n.text())
            .unwrap_or("")
            .to_string();
        let fields = doc
            .descendants()
            .filter(|n| n.has_tag_name("Field"))
            .map(|n| parse_def(n.attribute("Def").unwrap()))
            .collect();
        Self { param_type, fields }
    }
}

/// "u8 name:4", "dummy8 pad[3]", "f32 name = 1", "fixstr name[32]"
fn parse_def(def: &str) -> Field {
    let def = def.split('=').next().unwrap().trim();
    let (ty, rest) = def.split_once(' ').unwrap();
    let (mut name, mut count, mut bits) = (rest.trim().replace(' ', ""), 1, None);
    if let Some((n, b)) = name.clone().split_once(':') {
        name = n.trim().to_string();
        bits = Some(b.trim().parse().unwrap());
    }
    if let Some((n, c)) = name.clone().split_once('[') {
        name = n.to_string();
        // A few defs annotate allowed values instead ("u8 GroundMaterialType [0,1,2,3]").
        count = c.trim_end_matches(']').parse().unwrap_or(1);
    }
    Field { ty: ty.to_string(), name, count, bits }
}

fn type_size(ty: &str) -> usize {
    match ty {
        "s8" | "u8" | "dummy8" | "fixstr" => 1,
        "s16" | "u16" | "fixstrW" => 2,
        "s32" | "u32" | "f32" | "angle32" | "b32" => 4,
        "f64" => 8,
        t => panic!("unknown paramdef type {t}"),
    }
}

fn read_scalar(r: &Reader, o: usize, ty: &str) -> Value {
    match ty {
        "s8" => json!(r.d[o] as i8),
        "u8" | "dummy8" => json!(r.d[o]),
        "s16" => json!(r.u16(o) as i16),
        "u16" => json!(r.u16(o)),
        "s32" => json!(r.i32(o)),
        "u32" | "b32" => json!(r.u32(o)),
        "f32" | "angle32" => json!(r.f32(o)),
        t => panic!("scalar {t}"),
    }
}

/// Decodes one row's bytes into {fieldName: value}. Padding fields are skipped.
fn decode_row(def: &ParamDef, r: &Reader, start: usize) -> Map<String, Value> {
    let mut out = Map::new();
    let mut o = start;
    // Current bitfield unit: (type, bits consumed).
    let mut unit: Option<(String, u32)> = None;
    for f in &def.fields {
        let size = type_size(&f.ty);
        if let Some(bits) = f.bits {
            let unit_bits = size as u32 * 8;
            let fits = matches!(&unit, Some((t, used)) if type_size(t) == size && used + bits <= unit_bits);
            if !fits {
                if unit.is_some() {
                    o += type_size(&unit.as_ref().unwrap().0);
                }
                unit = Some((f.ty.clone(), 0));
            }
            let (_, used) = unit.as_mut().unwrap();
            let raw = match size {
                1 => r.d[o] as u32,
                2 => r.u16(o) as u32,
                _ => r.u32(o),
            };
            let v = (raw >> *used) & ((1u64 << bits) - 1) as u32;
            *used += bits;
            if !f.ty.starts_with("dummy") {
                out.insert(f.name.clone(), json!(v));
            }
            continue;
        }
        if let Some((t, _)) = unit.take() {
            o += type_size(&t);
        }
        match f.ty.as_str() {
            "dummy8" => {}
            "fixstr" => {
                out.insert(f.name.clone(), json!(r.cstr(o)));
            }
            "fixstrW" => {
                out.insert(f.name.clone(), json!(r.utf16z(o)));
            }
            ty if f.count == 1 => {
                out.insert(f.name.clone(), read_scalar(r, o, ty));
            }
            ty => {
                let vals: Vec<Value> = (0..f.count).map(|i| read_scalar(r, o + i * size, ty)).collect();
                out.insert(f.name.clone(), Value::Array(vals));
            }
        }
        o += size * f.count;
    }
    out
}

pub fn row_size(def: &ParamDef) -> usize {
    let mut size = 0;
    let mut unit: Option<(String, u32)> = None;
    for f in &def.fields {
        let s = type_size(&f.ty);
        if let Some(bits) = f.bits {
            let fits = matches!(&unit, Some((t, used)) if type_size(t) == s && used + bits <= s as u32 * 8);
            if !fits {
                if let Some((t, _)) = &unit {
                    size += type_size(t);
                }
                unit = Some((f.ty.clone(), 0));
            }
            unit.as_mut().unwrap().1 += bits;
            continue;
        }
        if let Some((t, _)) = unit.take() {
            size += type_size(&t);
        }
        size += s * f.count;
    }
    if let Some((t, _)) = unit {
        size += type_size(&t);
    }
    size
}

/// Returns (param type string, rows as JSON).
pub fn read_param(data: &[u8], defs: &HashMap<String, ParamDef>, names: &HashMap<i32, String>) -> (String, Value) {
    let r = Reader::new(data);
    let fmt2d = data[0x2D];
    if fmt2d & 0x04 == 0 || fmt2d & 0x80 == 0 {
        return (format!("unsupported format {fmt2d:#x}"), Value::Null);
    }
    let unicode_names = data[0x2E] & 0x01 != 0;
    let row_count = r.u16(0x0A) as usize;
    let param_type = r.cstr(r.u64(0x10) as usize);
    let Some(def) = defs.get(&param_type) else {
        return (param_type, Value::Null);
    };
    let expected = row_size(def);
    if row_count >= 2 {
        let stride = r.u64(0x40 + 0x18 + 8) - r.u64(0x40 + 8);
        if stride as usize != expected {
            eprintln!("warning: {param_type} paramdef size {expected} != row stride {stride}");
        }
    }
    let mut rows = Vec::with_capacity(row_count);
    for i in 0..row_count {
        let h = 0x40 + i * 0x18;
        let id = r.i32(h);
        let data_off = r.u64(h + 8) as usize;
        let name_off = r.u64(h + 0x10) as usize;
        let mut name = if name_off != 0 && name_off < data.len() {
            if unicode_names { r.utf16z(name_off) } else { r.cstr(name_off) }
        } else {
            String::new()
        };
        if name.is_empty() {
            name = names.get(&id).cloned().unwrap_or_default();
        }
        let mut row = Map::new();
        row.insert("id".into(), json!(id));
        row.insert("name".into(), json!(name));
        row.extend(decode_row(def, &r, data_off));
        rows.push(Value::Object(row));
    }
    (param_type, json!({ "rowSize": expected, "rows": rows }))
}
