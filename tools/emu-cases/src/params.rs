//! The game's loaded param tables, read out of the snapshot (no code runs): the
//! SoloParamRepository singleton DAT_143d978b0 (vtable SoloParamRepositoryImp) holds 0x88 slots of
//! 0x48 bytes at +0x70: +0x70 the slot's resource count, +0x78 up to eight ParamResCap pointers
//! (FUN_1410ce110 / FUN_1410ce0e0). A ParamResCap's name is the wide string at +0x18 (inline when
//! its capacity +0x30 is under 8, length +0x28); its PARAM file is [[cap+0x70]+0x70]
//! (FUN_1410c5b10, MultiPlayCorrectionParam). The file: +0xa row count, +0x10 offset of the param
//! type string, rows at +0x40 of 0x18 bytes (id, data offset, name offset), offsets from the file.
//!
//! Each row is decoded with the Paramdex defs (tools/sekiro-extract/defs/paramdef; a reader of its
//! own, not sekiro-extract's) and written as numbers only: extracted/emu/params_rows.json, every
//! table the defs know, {name: {type, stride, defSize, fields, offsets [byte, bit], dupes, rows
//! {id: [values]}}} (fixstr / padding fields left out). src/sim_tests/emu_params.rs checks
//! RSekiro's loaded rows (combat_data.json, enemies/*.json, json/params) against it.

use super::*;

/// SoloParamRepository singleton (1.6.0).
const REPO: u64 = 0x1_43d9_78b0;

struct Field {
    ty: String,
    name: String,
    count: usize,
    bits: Option<u32>,
}

fn size_of(ty: &str) -> usize {
    match ty {
        "s8" | "u8" | "dummy8" | "fixstr" => 1,
        "s16" | "u16" | "fixstrW" => 2,
        "s32" | "u32" | "f32" | "angle32" | "b32" => 4,
        "f64" => 8,
        t => panic!("paramdef type {t}"),
    }
}

/// `Def="u8 name:4"`, `"dummy8 pad[3]"`, `"f32 name = 1"`, `"fixstr name[32]"`.
fn field(def: &str) -> Field {
    let def = def.split('=').next().unwrap().trim();
    let (ty, rest) = def.split_once(' ').unwrap();
    let mut name = rest.replace(' ', "");
    let mut bits = None;
    if let Some((n, b)) = name.clone().split_once(':') {
        name = n.to_string();
        bits = Some(b.parse().unwrap());
    }
    let mut count = 1;
    if let Some((n, c)) = name.clone().split_once('[') {
        name = n.to_string();
        count = c.trim_end_matches(']').parse().unwrap_or(1);
    }
    Field { ty: ty.to_string(), name, count, bits }
}

/// Param type -> fields, from every def XML (a plain scan of the ParamType and Field Def texts).
fn defs(dir: &Path) -> HashMap<String, Vec<Field>> {
    let mut out = HashMap::new();
    for e in std::fs::read_dir(dir).expect("paramdef dir").flatten() {
        let text = std::fs::read_to_string(e.path()).unwrap();
        let Some(ty) = text.split("<ParamType>").nth(1).and_then(|t| t.split('<').next()) else { continue };
        let fields = text.split("<Field Def=\"").skip(1).map(|s| field(s.split('"').next().unwrap())).collect();
        out.insert(ty.to_string(), fields);
    }
    out
}

/// A decoded field: name, type, byte offset, bit offset and width (bitfields), element count.
struct Slot {
    name: String,
    ty: String,
    off: usize,
    bit: u32,
    bits: Option<u32>,
    count: usize,
}

/// Field placement as MSVC packs the row struct: a bitfield shares the open unit while it is of
/// the same size and fits; anything else closes it. Returns (slots, row size).
fn layout(fields: &[Field]) -> (Vec<Slot>, usize) {
    let (mut slots, mut off) = (Vec::new(), 0usize);
    let mut unit: Option<(usize, u32)> = None; // (size, bits used)
    for f in fields {
        let size = size_of(&f.ty);
        if let Some(bits) = f.bits {
            match unit {
                Some((s, used)) if s == size && used + bits <= s as u32 * 8 => {}
                Some((s, _)) => {
                    off += s;
                    unit = Some((size, 0));
                }
                None => unit = Some((size, 0)),
            }
            let used = unit.as_ref().unwrap().1;
            slots.push(Slot { name: f.name.clone(), ty: f.ty.clone(), off, bit: used, bits: Some(bits), count: 1 });
            unit.as_mut().unwrap().1 += bits;
            continue;
        }
        if let Some((s, _)) = unit.take() {
            off += s;
        }
        slots.push(Slot { name: f.name.clone(), ty: f.ty.clone(), off, bit: 0, bits: None, count: f.count });
        off += size * f.count;
    }
    if let Some((s, _)) = unit {
        off += s;
    }
    (slots, off)
}

/// `n` bytes of the game's memory at `at` (each 64 KB window mapped from the snapshot first).
fn bytes(emu: &mut Emu, at: u64, n: usize) -> Vec<u8> {
    let mut a = at;
    while a < at + n as u64 {
        emu.peek::<1>(a);
        a = (a | 0xffff) + 1;
    }
    let mut b = vec![0u8; n];
    assert_eq!(unsafe { uc::uc_mem_read(emu.uc, at, b.as_mut_ptr() as _, n) }, 0, "read {at:#x}+{n:#x}");
    b
}

fn wide(emu: &mut Emu, at: u64, len: usize) -> String {
    let b = bytes(emu, at, len * 2);
    String::from_utf16_lossy(&b.chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect::<Vec<_>>())
}

fn scalar(b: &[u8], o: usize, ty: &str) -> Value {
    let u = |n: usize| -> u64 { b[o..o + n].iter().rev().fold(0, |a, &x| a << 8 | x as u64) };
    match ty {
        "s8" => json!(b[o] as i8),
        "u8" | "dummy8" => json!(b[o]),
        "s16" => json!(u(2) as u16 as i16),
        "u16" => json!(u(2) as u16),
        "s32" => json!(u(4) as u32 as i32),
        "u32" | "b32" => json!(u(4) as u32),
        "f32" | "angle32" => json!(f32::from_bits(u(4) as u32)),
        "f64" => json!(f64::from_bits(u(8))),
        t => panic!("scalar {t}"),
    }
}

/// A ParamResCap's (name, PARAM file).
fn res_cap(emu: &mut Emu, cap: u64) -> Option<(String, u64)> {
    let len = emu.peek_u64(cap + 0x28) as usize;
    let capacity = emu.peek_u64(cap + 0x30);
    let at = if capacity < 8 { cap + 0x18 } else { emu.peek_u64(cap + 0x18) };
    let name = wide(emu, at, len);
    let holder = emu.peek_u64(cap + 0x70);
    let file = if holder == 0 { 0 } else { emu.peek_u64(holder + 0x70) };
    (file != 0).then_some((name, file))
}

/// Every loaded param table: (resource name, file address). ThrowParam is not in the
/// repository: its own singleton DAT_143d6dae0 holds the ParamResCap at +8 (FUN_14095dcb0).
fn tables(emu: &mut Emu) -> Vec<(String, u64)> {
    let repo = emu.peek_u64(REPO);
    let mut out = Vec::new();
    for slot in 0..0x88u64 {
        let n = u32::from_le_bytes(emu.peek(repo + 0x70 + slot * 0x48)) as u64;
        for j in 0..n.min(8) {
            let cap = emu.peek_u64(repo + 0x78 + slot * 0x48 + j * 8);
            if cap != 0 {
                out.extend(res_cap(emu, cap));
            }
        }
    }
    let throw = emu.peek_u64(0x1_43d6_dae0);
    if throw != 0 {
        let cap = emu.peek_u64(throw + 8);
        if cap != 0 {
            out.extend(res_cap(emu, cap));
        }
    }
    out
}

pub(crate) fn run(emu: &mut Emu, out: &Path) {
    let here = Path::new(env!("CARGO_MANIFEST_DIR"));
    let defs = defs(&here.join("../sekiro-extract/defs/paramdef"));
    let mut doc = serde_json::Map::new();
    for (name, file) in tables(emu) {
        // Format byte +0x2d: 0x04 64-bit offsets, 0x80 the type string by offset (the layout
        // read here); the few others (MenuColorTableParam) are skipped, as sekiro-extract does.
        let fmt = emu.peek::<1>(file + 0x2d)[0];
        if fmt & 0x84 != 0x84 {
            println!("params: {name}: format {fmt:#x} skipped");
            continue;
        }
        let count = u16::from_le_bytes(emu.peek(file + 0xa)) as usize;
        let ty_at = file + emu.peek_u64(file + 0x10);
        let ty_raw = bytes(emu, ty_at, 64);
        let ty = String::from_utf8_lossy(&ty_raw[..ty_raw.iter().position(|&c| c == 0).unwrap_or(64)]).into_owned();
        let Some(fields) = defs.get(&ty) else {
            println!("params: {name} ({ty}): no paramdef");
            continue;
        };
        let (slots, size) = layout(fields);
        let index = bytes(emu, file + 0x40, count * 0x18);
        let ent = |i: usize| -> (i32, u64) {
            let e = &index[i * 0x18..];
            (i32::from_le_bytes(e[0..4].try_into().unwrap()), u64::from_le_bytes(e[8..16].try_into().unwrap()))
        };
        let stride = if count >= 2 { ent(1).1 - ent(0).1 } else { size as u64 };
        let keep: Vec<&Slot> = slots.iter().filter(|s| !s.ty.starts_with("dummy") && !s.ty.starts_with("fixstr")).collect();
        // A name the def repeats (CoolTimeParam, NetworkParam) is `<name>_<n>` from its second
        // use on, as sekiro-extract keys it.
        let mut names: Vec<String> = Vec::new();
        for s in &keep {
            let mut k = s.name.clone();
            let mut n = 0;
            while names.contains(&k) {
                n += 1;
                k = format!("{}_{n}", s.name);
            }
            names.push(k);
        }
        let mut rows = serde_json::Map::new();
        let mut dupes = 0;
        for i in 0..count {
            let (id, off) = ent(i);
            let b = bytes(emu, file + off, size);
            let vals: Vec<Value> = keep
                .iter()
                .map(|s| match s.bits {
                    Some(w) => {
                        let raw = match size_of(&s.ty) {
                            1 => b[s.off] as u32,
                            2 => u16::from_le_bytes([b[s.off], b[s.off + 1]]) as u32,
                            _ => u32::from_le_bytes(b[s.off..s.off + 4].try_into().unwrap()),
                        };
                        json!((raw >> s.bit) & ((1u64 << w) - 1) as u32)
                    }
                    None if s.count == 1 => scalar(&b, s.off, &s.ty),
                    None => Value::Array((0..s.count).map(|k| scalar(&b, s.off + k * size_of(&s.ty), &s.ty)).collect()),
                })
                .collect();
            // The game's lookup is a binary search over the sorted ids: the first of equal ids is
            // the one found; keep that one.
            if rows.contains_key(&id.to_string()) {
                dupes += 1;
                continue;
            }
            rows.insert(id.to_string(), Value::Array(vals));
        }
        if stride != size as u64 {
            println!("params: {name} ({ty}) row stride {stride} != paramdef size {size}");
        }
        doc.insert(
            name.clone(),
            json!({
                "type": ty, "stride": stride, "defSize": size, "dupes": dupes,
                "fields": names,
                "offsets": keep.iter().map(|s| [s.off as u32, s.bit]).collect::<Vec<_>>(),
                "rows": rows,
            }),
        );
        println!("params: {name} {count} rows ({ty})");
    }
    let path = out.join("params_rows.json");
    std::fs::write(&path, serde_json::to_string(&json!({ "source": "SoloParamRepository DAT_143d978b0 in the snapshot", "tables": doc })).unwrap()).unwrap();
    println!("wrote {} ({} tables)", path.display(), doc.len());
}
