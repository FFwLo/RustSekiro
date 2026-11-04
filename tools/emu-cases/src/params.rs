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

/// One loaded table for the read watch: its file, rows by data offset and field names by byte.
struct Watched {
    name: String,
    file: u64,
    end: u64,
    stride: u64,
    /// (data offset from the file, row id), sorted.
    rows: Vec<(u64, i32)>,
    /// byte offset in the row -> the fields that start or lie on it.
    fields: HashMap<u64, Vec<String>>,
}

static WATCHED: std::sync::OnceLock<Vec<Watched>> = std::sync::OnceLock::new();
/// (reading instruction, table index, byte offset in the row) -> reads.
static READS: std::sync::Mutex<BTreeMap<(u64, usize, u64), u64>> = std::sync::Mutex::new(BTreeMap::new());

fn watched(emu: &mut Emu) -> Vec<Watched> {
    let here = Path::new(env!("CARGO_MANIFEST_DIR"));
    let defs = defs(&here.join("../sekiro-extract/defs/paramdef"));
    let mut out = Vec::new();
    for (name, file) in tables(emu) {
        if emu.peek::<1>(file + 0x2d)[0] & 0x84 != 0x84 {
            continue;
        }
        let count = u16::from_le_bytes(emu.peek(file + 0xa)) as usize;
        let ty_at = file + emu.peek_u64(file + 0x10);
        let ty_raw = bytes(emu, ty_at, 64);
        let ty = String::from_utf8_lossy(&ty_raw[..ty_raw.iter().position(|&c| c == 0).unwrap_or(64)]).into_owned();
        let Some(f) = defs.get(&ty) else { continue };
        let (slots, size) = layout(f);
        let index = bytes(emu, file + 0x40, count * 0x18);
        let mut rows: Vec<(u64, i32)> = (0..count).map(|i| (u64::from_le_bytes(index[i * 0x18 + 8..i * 0x18 + 16].try_into().unwrap()), i32::from_le_bytes(index[i * 0x18..i * 0x18 + 4].try_into().unwrap()))).collect();
        rows.sort();
        let mut fields: HashMap<u64, Vec<String>> = HashMap::new();
        for s in slots.iter().filter(|s| !s.ty.starts_with("dummy")) {
            let n = if s.bits.is_some() { size_of(&s.ty) } else { size_of(&s.ty) * s.count };
            for b in 0..n as u64 {
                fields.entry(s.off as u64 + b).or_default().push(s.name.clone());
            }
        }
        let (Some(first), Some(last)) = (rows.first(), rows.last()) else { continue };
        out.push(Watched { name, file: file + first.0, end: file + last.0 + size as u64, stride: size as u64, rows: rows.iter().map(|&(o, id)| (o - first.0, id)).collect(), fields });
    }
    out
}

extern "C" fn on_param_read(uc: uc::Engine, _kind: i32, addr: u64, _size: i32, _value: i64, _data: *mut c_void) -> bool {
    let Some(w) = WATCHED.get() else { return true };
    for (i, t) in w.iter().enumerate() {
        if (t.file..t.end).contains(&addr) {
            let rel = addr - t.file;
            let k = t.rows.partition_point(|r| r.0 <= rel);
            if k == 0 {
                break;
            }
            let off = rel - t.rows[k - 1].0;
            if off < t.stride {
                let mut rip = 0u64;
                unsafe { uc::uc_reg_read(uc, uc::RIP, &mut rip as *mut u64 as _) };
                *READS.lock().unwrap().entry((rip, i, off)).or_default() += 1;
            }
            break;
        }
    }
    true
}

/// `EMU_PARAM_READS=<file>`: every Emu made from here on records which instruction reads which
/// param table's field (row data of every loaded table, as `params` lays it out), whatever
/// generator runs; `ReadsLog` writes them when the run ends. The reads say which exe code uses a
/// field (src/sim_tests/emu_params.rs `param_fields_are_read_where_rsekiro_says`).
pub(crate) fn watch(emu: &mut Emu) {
    if std::env::var_os("EMU_PARAM_READS").is_none() || emu.st.snap.is_none() {
        return;
    }
    if WATCHED.get().is_none() {
        let w = watched(emu);
        let _ = WATCHED.set(w);
    }
    let w = WATCHED.get().unwrap();
    let (lo, hi) = (w.iter().map(|t| t.file).min().unwrap_or(0), w.iter().map(|t| t.end).max().unwrap_or(0));
    emu.hook(uc::HOOK_MEM_READ, on_param_read as *const c_void, lo, hi);
}

/// Writes the recorded reads (`watch`) on drop: {tables: [name], reads: [[rip, table, offset,
/// fields, count]]}, merged into the file's earlier runs.
pub(crate) struct ReadsLog;

impl Drop for ReadsLog {
    fn drop(&mut self) {
        let (Some(path), Some(w)) = (std::env::var_os("EMU_PARAM_READS"), WATCHED.get()) else { return };
        let mut all: BTreeMap<(u64, String, u64), (Value, u64)> = BTreeMap::new();
        if let Ok(old) = std::fs::read_to_string(&path).map(|t| serde_json::from_str::<Value>(&t).unwrap_or(Value::Null)) {
            for r in old["reads"].as_array().into_iter().flatten() {
                all.insert((r[0].as_u64().unwrap(), r[1].as_str().unwrap().to_string(), r[2].as_u64().unwrap()), (r[3].clone(), r[4].as_u64().unwrap()));
            }
        }
        for (&(rip, t, off), &n) in READS.lock().unwrap().iter() {
            let names = json!(w[t].fields.get(&off).cloned().unwrap_or_default());
            all.entry((rip, w[t].name.clone(), off)).or_insert((names, 0)).1 += n;
        }
        // The function holding each instruction (DSEKIRO_FUNCTIONS: DSekiro's function list).
        let mut starts: Vec<u64> = super::functions_json().into_keys().collect();
        starts.sort();
        let func = |rip: u64| starts.partition_point(|&s| s <= rip).checked_sub(1).map(|i| starts[i]);
        let reads: Vec<Value> = all.into_iter().map(|((rip, t, off), (f, n))| json!([rip, t, off, f, n, func(rip)])).collect();
        println!("params reads: {} (instruction, field) pairs -> {}", reads.len(), Path::new(&path).display());
        std::fs::write(&path, serde_json::to_string(&json!({ "reads": reads, "columns": ["instruction", "table", "row offset", "fields", "reads", "function"] })).unwrap()).unwrap();
    }
}

/// TAE AttackBehavior's id (EnemyIns vfunc 0x1c0, FUN_140a12b80: rcx chr, edx judge, r8d hand).
const NPC_BEHAVIOR_ID: u64 = 0x1_40a1_2b80;
/// The attack info's constructor and fill (atkrec.rs: rcx info, rdx chr, r8 BehaviorParam id,
/// r9 hand, [5] flags); +8 the AtkParam id (-1 when the row is missing or not refType 0).
const INFO_INIT: u64 = 0x1_4083_d220;
const INFO_FILL: u64 = 0x1_4084_fc50;

/// `params_keys`: how the exe turns an NPC's TAE judge into its AtkParam row, for every
/// (behavior variation, judge) RSekiro's enemy files use: each enemy's main NpcParam variation and
/// every "v<variation>:" key's, times every judge of its attack keys and its anims' AttackBehavior
/// events. The General's EnemyIns runs FUN_140a12b80 with its NpcParam row's behaviorVariationId
/// (the row vfunc 0x178 returns, +8) set to the variation in the emulator's copy, then the info
/// fill FUN_14084fc50 with that id. Writes extracted/emu/params_keys.json for
/// src/sim_tests/emu_params.rs.
pub(crate) fn keys(new_emu: &dyn Fn() -> Emu, out: &Path) {
    let root = out.parent().unwrap();
    let rows = |name: &str| -> HashMap<i64, Value> {
        let v: Value = serde_json::from_str(&std::fs::read_to_string(root.join(format!("json/params/{name}.json"))).expect("params json")).unwrap();
        match &v["rows"] {
            Value::Array(a) => a.iter().map(|r| (r["id"].as_i64().unwrap(), r.clone())).collect(),
            Value::Object(o) => o.iter().map(|(k, r)| (k.parse().unwrap(), r.clone())).collect(),
            _ => HashMap::new(),
        }
    };
    let npc = rows("NpcParam");
    let mut pairs: std::collections::BTreeSet<(i64, i64)> = Default::default();
    let mut files = Vec::new();
    for e in std::fs::read_dir(root.join("enemies")).expect("enemies").flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if !(name.len() == 10 && name.starts_with('c') && name.ends_with(".json")) {
            continue;
        }
        let d: Value = serde_json::from_str(&std::fs::read_to_string(e.path()).unwrap()).unwrap();
        let main = d["roster"]["npc"].as_i64().unwrap_or(0);
        let var = npc.get(&main).and_then(|r| r["behaviorVariationId"].as_i64()).unwrap_or(0);
        let mut vars = vec![var];
        let mut judges = std::collections::BTreeSet::new();
        for k in d["attacks"].as_object().into_iter().flat_map(|o| o.keys()) {
            if let Ok(j) = k.parse::<i64>() {
                judges.insert(j);
            } else if let Some((v, j)) = k.strip_prefix('v').and_then(|s| s.split_once(':')) {
                vars.push(v.parse().unwrap());
                judges.insert(j.parse().unwrap());
            }
        }
        for a in d["anims"].as_object().into_iter().flat_map(|o| o.values()) {
            for ev in a["events"].as_array().into_iter().flatten() {
                if ev["type"].as_i64() == Some(1) {
                    if let Some(j) = ev["args"]["BehaviorJudgeID"].as_i64() {
                        judges.insert(j);
                    }
                }
            }
        }
        vars.sort();
        vars.dedup();
        for &v in &vars {
            for &j in &judges {
                pairs.insert((v, j));
            }
        }
        files.push(json!({"chr": &name[..5], "npc": main, "variation": var}));
    }
    let pairs: Vec<(i64, i64)> = pairs.into_iter().collect();
    let t = std::time::Instant::now();
    let mut cases = Vec::new();
    let mut general_var = Value::Null;
    for chunk in pairs.chunks(200) {
        let mut emu = new_emu();
        emu.limit = 500_000_000;
        emu.raw_write(0x1_43c1_d520, &1u32.to_le_bytes());
        emu.raw_write(super::proxy::LONGJMP, super::proxy::PLAIN_LONGJMP);
        let chr = super::acttrace::general(&mut emu);
        let row_fn = super::guard::vfunc(&mut emu, chr, 0x178);
        let holder = emu.call(row_fn, &[chr], &[]).rax;
        let row = emu.peek_u64(holder + 8);
        assert!(row != 0, "the General has no NpcParam row");
        let own = u32::from_le_bytes(emu.peek(row));
        if general_var.is_null() {
            general_var = json!(own);
        }
        let hand_fn = super::guard::vfunc(&mut emu, chr, 0x330);
        let hand = emu.call(hand_fn, &[chr], &[]).rax as u32 as u64;
        let info = emu.alloc(0x200);
        for (i, &(v, j)) in chunk.iter().enumerate() {
            emu.raw_write(row, &(v as u32).to_le_bytes());
            let id = emu.call(NPC_BEHAVIOR_ID, &[chr, j as u64, hand, 0], &[]).rax as u32 as i32;
            emu.raw_write(info, &[0u8; 0x160]);
            let r0 = emu.call(INFO_INIT, &[info], &[]);
            let r1 = emu.call(INFO_FILL, &[info, chr, id as u32 as u64, hand, 1], &[]);
            let atk = i32::from_le_bytes(emu.peek(info + 8));
            let stopped = r0.stopped.or(r1.stopped).map(|(a, e)| format!("{a:#x} error {e}"));
            cases.push(json!({"variation": v, "judge": j, "row": id, "atk": atk, "stopped": stopped}));
            if i % 20 == 19 {
                super::proxy::flush_tb(&emu);
            }
        }
    }
    let found = cases.iter().filter(|c| c["atk"].as_i64() != Some(-1)).count();
    println!("params_keys: {} (variation, judge) pairs, {found} with an AtkParam row ({:.0} s)", cases.len(), t.elapsed().as_secs_f32());
    let v = json!({"functions": ["0x140a12b80", "0x14083d220", "0x14084fc50"], "general_variation": general_var, "files": files, "cases": cases});
    std::fs::write(out.join("params_keys.json"), serde_json::to_string(&v).unwrap()).unwrap();
}
