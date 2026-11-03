//! FXR property evaluation (topic fxr): the exe's property function table (44 evaluators
//! `function << 4 | type (| 0x1000 loop)`, registered by FUN_141d98ff0 from 1400f, sorted table at
//! [DAT_143f136b0]) and its modifier table (RandomDelta 0xd05x / RandomFraction 0xf05x /
//! ExternalValue 0xe06x 0xe07x, registered from FUN_1400f7220, table at [DAT_143f13da0]), run as
//! the particle code runs them (FUN_1403f7770: the property's evaluator, then each modifier on its
//! result, with the context {object, argument, object +0xcc random}).
//!
//! Inputs: every keyframed or randomised property of the effects RSekiro plays most (deflect sparks
//! 252001, the hit sparks 201001 / 201002, blood 220502, the firecracker's fire 300071), plus one
//! property of every (function, loop, modifiers, component count) kind found in all of
//! extracted/fxr, written in the exe's binary layout (fxr.ts readProperty / SequenceProperty
//! fields: count, min[n], max[n], positions, values, then t1 / p1 and t2 / p2), evaluated at the
//! keyframes, between them, before the first and past the last, with several particle randoms.
//! Writes extracted/emu/fxr_props.json (`fxr`) and fxr_shapes.json (`fxr_shapes`, the emitter shape
//! and spread generators) for src/sim_tests/emu_fxr.rs.

use super::*;

const PROP_TABLE: u64 = 0x1_43f1_36b0;
const MOD_TABLE: u64 = 0x1_43f1_3da0;
/// The scalar property + modifiers wrapper (particle code: FUN_1403f7770(prop, object, &argument)).
const EVAL_SCALAR: u64 = 0x1_403f_7770;

const FILES: &[i64] = &[252001, 201001, 201002, 220502, 300071];
const RANDOMS: &[u32] = &[0, 1, 0xffff_ffff, 0x8000_0000, 0x1234_5678, 0x9e37_79b9, 0x7fff_0001, 0xdead_beef];

fn func_id(name: &str) -> Option<u32> {
    Some(match name {
        "Zero" => 0,
        "One" => 1,
        "Constant" => 2,
        "Stepped" => 3,
        "Linear" => 4,
        "Bezier" => 5,
        "Hermite" => 6,
        _ => return None,
    })
}

fn comps(v: &Value) -> usize {
    v.as_array().map_or(1, |a| a.len())
}

fn fl(v: &Value, i: usize) -> f32 {
    match v {
        Value::Array(a) => a.get(i).or(a.first()).and_then(Value::as_f64).unwrap_or(0.0) as f32,
        Value::Bool(b) => *b as i32 as f32,
        _ => v.as_f64().unwrap_or(0.0) as f32,
    }
}

fn int(v: &Value, i: usize) -> i32 {
    match v {
        Value::Array(a) => a.get(i).or(a.first()).and_then(Value::as_i64).unwrap_or(0) as i32,
        _ => v.as_i64().unwrap_or(0) as i32,
    }
}

/// (typeEnumA, fields as u32 words, component count) of a property JSON (fxr.ts layout).
fn prop_fields(p: &Value) -> Option<(u32, Vec<u32>, usize)> {
    let f = |x: f32| x.to_bits();
    match p.get("function").and_then(Value::as_str) {
        Some(name) => {
            let func = func_id(name)?;
            let kf = p["keyframes"].as_array()?;
            if kf.is_empty() {
                return None;
            }
            let n = comps(&kf[0]["value"]);
            let lp = p["loop"].as_bool() == Some(true);
            let k = kf.len();
            let mut w = vec![k as u32];
            for c in 0..n {
                w.push(f(kf.iter().map(|q| fl(&q["value"], c)).fold(f32::INFINITY, f32::min)));
            }
            for c in 0..n {
                w.push(f(kf.iter().map(|q| fl(&q["value"], c)).fold(f32::NEG_INFINITY, f32::max)));
            }
            w.extend(kf.iter().map(|q| f(fl(&q["position"], 0))));
            for q in kf {
                w.extend((0..n).map(|c| f(fl(&q["value"], c))));
            }
            let (a, b) = match func {
                5 => ("p1", "p2"),
                6 => ("t1", "t2"),
                _ => ("", ""),
            };
            if !a.is_empty() {
                for key in [a, b] {
                    for q in kf {
                        w.extend((0..n).map(|c| f(fl(&q[key], c))));
                    }
                }
            }
            Some(((n as u32 - 1) | func << 4 | (lp as u32) << 12, w, n))
        }
        None => {
            let v = p.get("value")?;
            let n = comps(v);
            Some(((n as u32 - 1) | 2 << 4, (0..n).map(|c| f(fl(v, c))).collect(), n))
        }
    }
}

/// (typeEnumA, fields) of a modifier JSON.
fn mod_fields(m: &Value, n: usize) -> Option<(u32, Vec<u32>)> {
    let ty: u32 = match m["type"].as_str()? {
        "RandomDelta" => 21,
        "RandomFraction" => 53,
        _ => return None,
    };
    let mut w: Vec<u32> = (0..n).map(|c| int(&m["seed"], c) as u32).collect();
    w.extend((0..n).map(|c| fl(&m["max"], c).to_bits()));
    Some(((ty >> 4 | 0xc) << 12 | (ty & 0xf) << 4 | (n as u32 - 1), w))
}

/// Every property object (keyframed, or a constant with modifiers) under `v`, with its path.
fn collect(v: &Value, path: String, out: &mut Vec<(String, Value)>) {
    match v {
        Value::Object(o) => {
            if o.contains_key("function") || (o.contains_key("modifiers") && o.contains_key("value")) {
                out.push((path.clone(), v.clone()));
            }
            for (k, x) in o {
                collect(x, format!("{path}/{k}"), out);
            }
        }
        Value::Array(a) => {
            for (i, x) in a.iter().enumerate() {
                collect(x, format!("{path}/{i}"), out);
            }
        }
        _ => {}
    }
}

fn kind(p: &Value) -> String {
    let n = p.get("value").or_else(|| p["keyframes"].get(0).and_then(|k| k.get("value"))).map_or(1, comps);
    let mods: Vec<&str> = p["modifiers"].as_array().map(|a| a.iter().filter_map(|m| m["type"].as_str()).collect()).unwrap_or_default();
    format!("{}|{}|{}|{n}", p["function"].as_str().unwrap_or("Constant"), p["loop"].as_bool().unwrap_or(false), mods.join("+"))
}

/// The arguments to sample: before, at, between and past the keyframes (and a loop's later turns).
fn times(p: &Value) -> Vec<f32> {
    let Some(kf) = p["keyframes"].as_array() else { return vec![0.0, 0.5, 1.7] };
    let pos: Vec<f32> = kf.iter().map(|k| fl(&k["position"], 0)).collect();
    let last = *pos.last().unwrap();
    let mut t = vec![pos[0] - 0.25, last + 0.3, last * 1.5 + 0.1, last * 2.37 + 0.01];
    for (i, &a) in pos.iter().enumerate() {
        t.push(a);
        if let Some(&b) = pos.get(i + 1) {
            for q in [0.1, 0.25, 0.5, 0.75, 0.93] {
                t.push(a + (b - a) * q);
            }
        }
    }
    t
}

fn table(emu: &mut Emu, at: u64) -> HashMap<u32, (u64, u64)> {
    let base = emu.peek_u64(at);
    let count = emu.peek_u64(at + 8).min(64);
    (0..count)
        .map(|i| {
            let e = base + 0x28 * i;
            let id = u16::from_le_bytes(emu.peek(e)) as u32;
            (id, (i, emu.peek_u64(e + 0x10)))
        })
        .collect()
}

pub(crate) fn run(new_emu: &dyn Fn() -> Emu, out: &Path, want: &[&str]) {
    if want.contains(&"fxr") {
        props(new_emu, out);
    }
    if want.contains(&"fxr_shapes") {
        shapes(new_emu, out);
    }
}

/// The FXR RNG (FUN_141dc67a0: a xorshift128 at +4..+0x14, FUN_14014d6e0 [0, 1], FUN_140149aa0
/// a range, FUN_140466380 an int range).
const RNG: u64 = 0x1_41dc_67a0;

/// Emitter shapes and direction spreads (action handlers registered per type by FUN_141d8a410):
/// each generator run on an action built in the binary layout (fxr.ts writeAction: fields at +0x20,
/// properties at +0x30) from a chosen RNG state, so the test redraws the same numbers.
/// (type, generator, kind): kind 0 = (action, ctx) -> xmm0 point, 1 = (point out, frame out,
/// action, ctx), 2 = (frame out, action, ctx).
const GENERATORS: &[(i64, u64, u8)] = &[
    (401, 0x1_41db_3800, 0), // DiskEmitterShape
    (402, 0x1_41db_4160, 0), // RectangleEmitterShape
    (403, 0x1_41db_3ee0, 1), // SphereEmitterShape
    (404, 0x1_41db_3220, 1), // BoxEmitterShape
    (405, 0x1_41db_3ae0, 1), // CylinderEmitterShape
    (501, 0x1_41db_5cf0, 2), // CircularSpread
    (502, 0x1_41db_5fe0, 2), // EllipticalSpread
    (503, 0x1_41db_6740, 2), // RectangularSpread
];

/// (fields1 as ints, properties1 as constants) per case, from the values the files use.
fn shape_setups(ty: i64) -> Vec<(Vec<i32>, Vec<f32>)> {
    let mut v = Vec::new();
    match ty {
        401 => {
            for d in [0.0, -0.5, -1.0, 0.4, 1.0, -0.3, 0.65] {
                v.push((vec![0], vec![1.5, d]));
            }
        }
        402 => {
            for d in [0.0, -0.5, 0.5, -0.9] {
                v.push((vec![0], vec![2.0, 0.5, d]));
            }
        }
        403 => {
            for inside in [0, 1] {
                v.push((vec![inside], vec![0.8]));
            }
        }
        404 => {
            for dir in [0, 1, 6] {
                for inside in [0, 1] {
                    v.push((vec![dir, inside], vec![2.0, 1.0, 0.5]));
                }
            }
        }
        405 => {
            for dir in [0, 1] {
                for inside in [0, 1] {
                    v.push((vec![dir, inside, 1], vec![0.7, 1.2]));
                }
            }
        }
        501 => {
            for d in [0.0, -0.5, -0.25, -1.0, 1.0, 0.5, -0.9] {
                v.push((vec![0], vec![30.0, d]));
            }
            v.push((vec![0], vec![0.0, 0.0]));
            v.push((vec![0], vec![120.0, -0.5]));
        }
        502 | 503 => {
            for d in [0.0, -0.5, -1.0, 0.5] {
                v.push((vec![0], vec![40.0, 15.0, d]));
            }
        }
        _ => {}
    }
    v
}

fn shapes(new_emu: &dyn Fn() -> Emu, out: &Path) {
    const DRAWS: usize = 64;
    let mut cases = Vec::new();
    let mut lcg: u64 = 0x2545_f491_4f6c_dd1d;
    let mut next = || {
        lcg = lcg.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((lcg >> 32) as u32) | 1
    };
    for &(ty, gen, kind) in GENERATORS {
        for (fields, values) in shape_setups(ty) {
            // a fresh emulator per setup (one emulator crashes the host after ~10,000 calls)
            let mut emu = new_emu();
            let props_t = table(&mut emu, PROP_TABLE);
            let (index, _) = props_t[&0x20];
            let rng = emu.call(RNG, &[], &[]).rax;
            let base = HEAP;
            emu.st.heap_next = HEAP;
            emu.alloc(0x2000);
            let (obj, ctx, action, fptr, pptr, pfields, o1, o2) = (base, base + 0x200, base + 0x240, base + 0x280, base + 0x300, base + 0x600, base + 0x700, base + 0x780);
            emu.write(obj, &[0u8; 0x200]);
            let mut a = vec![0u8; 0x40];
            a[0..2].copy_from_slice(&(ty as u16).to_le_bytes());
            a[8..12].copy_from_slice(&(fields.len() as u32).to_le_bytes());
            a[0x10..0x14].copy_from_slice(&(values.len() as u32).to_le_bytes());
            a[0x20..0x28].copy_from_slice(&fptr.to_le_bytes());
            a[0x30..0x38].copy_from_slice(&pptr.to_le_bytes());
            emu.write(action, &a);
            emu.write(fptr, &fields.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<_>>());
            for (i, &x) in values.iter().enumerate() {
                let mut p = vec![0u8; 0x28];
                p[0..2].copy_from_slice(&0x20u16.to_le_bytes());
                p[3] = 1;
                p[4..8].copy_from_slice(&(index as u32).to_le_bytes());
                p[8..12].copy_from_slice(&1u32.to_le_bytes());
                p[0x10..0x18].copy_from_slice(&(pfields + 4 * i as u64).to_le_bytes());
                emu.write(pptr + 0x28 * i as u64, &p);
                emu.f32(pfields + 4 * i as u64, x);
            }
            emu.u64(ctx, obj);
            emu.f32(ctx + 8, 0.0);
            let mut rows = Vec::new();
            for _ in 0..DRAWS {
                let state = [next(), next(), next(), next()];
                emu.raw_write(rng + 4, &state.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<_>>());
                emu.write(o1, &[0u8; 0x80]);
                let run = match kind {
                    0 => emu.call(gen, &[action, ctx], &[]),
                    1 => emu.call(gen, &[o1, o2, action, ctx], &[]),
                    _ => emu.call(gen, &[o2, action, ctx], &[]),
                };
                if run.stopped.is_some() {
                    println!("fxr_shapes: {ty} stopped at {:x?}", run.stopped);
                    break;
                }
                let mut x0 = [0u8; 16];
                unsafe { uc::uc_reg_read(emu.uc, uc::XMM0, x0.as_mut_ptr() as _) };
                let point: [f32; 4] = match kind {
                    0 => std::array::from_fn(|i| f32::from_le_bytes(x0[4 * i..4 * i + 4].try_into().unwrap())),
                    1 => std::array::from_fn(|i| emu.rf32(o1 + 4 * i as u64)),
                    _ => [0.0; 4],
                };
                let frame: Vec<f32> = if kind == 0 { Vec::new() } else { (0..16).map(|i| emu.rf32(o2 + 4 * i)).collect() };
                let after: Vec<u32> = (0..4).map(|i| u32::from_le_bytes(emu.read(rng + 4 + 4 * i))).collect();
                rows.push(json!({"state": state, "after": after, "point": point, "frame": frame}));
            }
            cases.push(json!({"type": ty, "fields": fields, "props": values, "rows": rows}));
        }
        println!("fxr_shapes: {ty} done");
    }
    let body = json!({
        "about": "FXR emitter shape / spread generators of sekiro.exe on the 2026-11-03 snapshot, RNG state set before each draw",
        "cases": cases,
    });
    std::fs::write(out.join("fxr_shapes.json"), serde_json::to_string(&body).unwrap()).unwrap();
}

fn props(new_emu: &dyn Fn() -> Emu, out: &Path) {
    let mut emu = new_emu();
    let props_t = table(&mut emu, PROP_TABLE);
    let mods_t = table(&mut emu, MOD_TABLE);
    println!("fxr: {} property evaluators, {} modifier evaluators", props_t.len(), mods_t.len());
    let dir = out.parent().unwrap().join("fxr");
    let mut chosen: Vec<(String, Value)> = Vec::new();
    for id in FILES {
        let Ok(s) = std::fs::read_to_string(dir.join(format!("{id}.json"))) else { continue };
        let v: Value = serde_json::from_str(&s).unwrap();
        let mut all = Vec::new();
        collect(&v, format!("{id}"), &mut all);
        // one of each distinct property per file (the same curve is repeated across configs)
        let mut seen = HashSet::new();
        chosen.extend(all.into_iter().filter(|(_, p)| seen.insert(p.to_string())));
    }
    // one property of every kind in the whole set
    let mut kinds: HashSet<String> = chosen.iter().map(|(_, p)| kind(p)).collect();
    let mut names: Vec<_> = std::fs::read_dir(&dir).map(|d| d.filter_map(|e| e.ok()).map(|e| e.path()).collect()).unwrap_or_default();
    names.sort();
    for f in names {
        let Ok(s) = std::fs::read_to_string(&f) else { continue };
        let Ok(v) = serde_json::from_str::<Value>(&s) else { continue };
        let mut all = Vec::new();
        collect(&v, f.file_stem().unwrap().to_string_lossy().into_owned(), &mut all);
        for (path, p) in all {
            if kinds.insert(kind(&p)) {
                chosen.push((path, p));
            }
        }
    }
    println!("fxr: {} properties, {} kinds", chosen.len(), kinds.len());

    let mut cases = Vec::new();
    let mut stops = 0;
    // A fresh emulator every few properties: one emulator crashes the host after ~10,000 calls.
    for (k, (path, p)) in chosen.iter().enumerate() {
        if k % 20 == 0 && k > 0 {
            emu = new_emu();
        }
        // fixed scratch: object (its +0xcc random), context, property, modifiers, fields, outputs
        if k % 20 == 0 {
            emu.st.heap_next = HEAP;
        }
        let obj = HEAP;
        let ctx = obj + 0x200;
        let prop = ctx + 0x20;
        let mods = prop + 0x30;
        let fields = mods + 0x100;
        let mfields = fields + 0x4000;
        let outb = mfields + 0x400;
        let arg = outb + 0x40;
        if k % 20 == 0 {
            emu.alloc(arg + 0x10 - HEAP);
            emu.write(obj, &[0u8; 0x200]);
        }
        let Some((enum_a, w, n)) = prop_fields(p) else { continue };
        let Some(&(index, eval)) = props_t.get(&enum_a) else {
            println!("fxr: no evaluator {enum_a:#x} for {path}");
            continue;
        };
        let ms: Vec<(u32, Vec<u32>)> = p["modifiers"].as_array().map(|a| a.iter().filter_map(|m| mod_fields(m, n)).collect()).unwrap_or_default();
        let wb: Vec<u8> = w.iter().flat_map(|x| x.to_le_bytes()).collect();
        emu.write(fields, &wb);
        let mut mb = Vec::new();
        let mut mf_at = mfields;
        let mut mod_evals = Vec::new();
        for (ea, mw) in &ms {
            let (mi, me) = mods_t[ea];
            mod_evals.push((mods + 0x20 * mod_evals.len() as u64, me));
            let mut m = vec![0u8; 0x20];
            m[0..2].copy_from_slice(&(*ea as u16).to_le_bytes());
            m[3] = 1;
            m[4..8].copy_from_slice(&(mi as u32).to_le_bytes());
            m[8..12].copy_from_slice(&(mw.len() as u32).to_le_bytes());
            m[0x10..0x18].copy_from_slice(&mf_at.to_le_bytes());
            mb.extend(m);
            let bytes: Vec<u8> = mw.iter().flat_map(|x| x.to_le_bytes()).collect();
            emu.write(mf_at, &bytes);
            mf_at += bytes.len() as u64;
        }
        if !mb.is_empty() {
            emu.write(mods, &mb);
        }
        let mut pb = vec![0u8; 0x28];
        pb[0..2].copy_from_slice(&(enum_a as u16).to_le_bytes());
        pb[3] = 1;
        pb[4..8].copy_from_slice(&(index as u32).to_le_bytes());
        pb[8..12].copy_from_slice(&(w.len() as u32).to_le_bytes());
        pb[0x10..0x18].copy_from_slice(&fields.to_le_bytes());
        pb[0x18..0x20].copy_from_slice(&mods.to_le_bytes());
        pb[0x20..0x24].copy_from_slice(&(ms.len() as u32).to_le_bytes());
        emu.write(prop, &pb);
        let randoms: &[u32] = if ms.is_empty() { &RANDOMS[..1] } else { RANDOMS };
        let mut rows = Vec::new();
        for t in times(p) {
            for &r in randoms {
                emu.u64(ctx, obj);
                emu.f32(ctx + 8, t);
                emu.u32(ctx + 0xc, r);
                let run = emu.call(eval, &[outb, prop, ctx], &[]);
                if run.stopped.is_some() {
                    stops += 1;
                    continue;
                }
                let base: [f32; 4] = std::array::from_fn(|i| emu.rf32(run.rax + 4 * i as u64));
                let mut val = base;
                let mut ok = true;
                for &(m, me) in &mod_evals {
                    emu.write(arg, &val.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<_>>());
                    let run = emu.call(me, &[outb + 0x20, arg, m, ctx], &[]);
                    if run.stopped.is_some() {
                        ok = false;
                        break;
                    }
                    val = std::array::from_fn(|i| emu.rf32(run.rax + 4 * i as u64));
                }
                if !ok {
                    stops += 1;
                    continue;
                }
                // the particle code's own wrapper for a scalar (the same chain)
                let mut wrap = Value::Null;
                if n == 1 {
                    emu.u32(obj + 0xcc, r);
                    emu.f32(arg, t);
                    let run = emu.call(EVAL_SCALAR, &[prop, obj, arg], &[]);
                    if run.stopped.is_none() {
                        wrap = json!(run.xmm0);
                    }
                }
                rows.push(json!({"t": t, "random": r, "base": &base[..n], "out": &val[..n], "wrapper": wrap}));
            }
        }
        cases.push(json!({"path": path, "prop": p, "type_a": enum_a, "rows": rows}));
    }
    let nrows: usize = cases.iter().map(|c| c["rows"].as_array().unwrap().len()).sum();
    println!("fxr: {} properties, {nrows} evaluations, {stops} stopped", cases.len());
    let body = json!({
        "about": "FXR property evaluators of sekiro.exe (table [0x143f136b0] / modifiers [0x143f13da0]) on the 2026-11-03 snapshot",
        "cases": cases,
    });
    std::fs::write(out.join("fxr_props.json"), serde_json::to_string(&body).unwrap()).unwrap();
}
