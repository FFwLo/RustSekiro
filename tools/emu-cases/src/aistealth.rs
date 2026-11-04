//! Stealth inputs of the sight check (topic aistealth, row E33): the observer's geometry bits
//! (FUN_140616f50, which gate the SpEffects with sightCutLimitType 1 / 2), Wolf's sight factors
//! under those bits (FUN_140bfe8a0 distance cut, FUN_140bfea80 / FUN_140bfe6e0 / FUN_140bfe7c0 /
//! FUN_140bfe9a0 angle cuts, FUN_140bfc050 meter rate), the light index (FUN_140617150: Wolf's
//! ChrIns +0x1acd remapped by a stateInfo 340 SpEffect on the observer) and the nose exemptions
//! (FUN_14061c940: stateInfo 333 / 335 on Wolf against 332 / 334 on the observer).

use std::path::Path;

use serde_json::{json, Value};

use super::aicpp::{ai_chain, f32s, observers, CTX_FILL, CTX_NO_RAY, CTX_RESET, CTX_TARGET, SIGHT};
use super::guard::ParamFile;
use super::Emu;

const BITS: u64 = 0x1_4061_6f50;
const ANGLE_FRONT: u64 = 0x1_43af_bb04;
const ANGLE_BACK: u64 = 0x1_43af_bb08;
const CUT: u64 = 0x1_40bf_e8a0;
/// Angle factors in the order of ctx +0x68 / 0x6c / 0x70 / 0x74 (FUN_140bd3e40's setters
/// FUN_140617200 / 1a0 / 1c0 / 1e0).
const ANG: [u64; 4] = [0x1_40bf_ea80, 0x1_40bf_e6e0, 0x1_40bf_e7c0, 0x1_40bf_e9a0];
const AROUND_RATE: u64 = 0x1_40bf_c050;
const LIGHT: u64 = 0x1_4061_7150;
const NOSE: u64 = 0x1_4061_c940;
const DARK_SIGHT: u64 = 0x1_43d8_7fd0;
const IN_DARK_SIGHT: u64 = 0x1_40c0_c170;

pub fn run(new_emu: &dyn Fn() -> Emu, out: &Path, want: &[&str]) {
    let all = want.contains(&"aistealth");
    if all || want.contains(&"aistealth_probe") {
        probe(&mut new_emu());
    }
    let mut body = serde_json::Map::new();
    if all || want.contains(&"aistealth_bits") {
        body.insert("bits".into(), bits_cases(&mut new_emu()));
    }
    if all || want.contains(&"aistealth_factors") {
        body.insert("factors".into(), factor_cases(new_emu));
    }
    if all || want.contains(&"aistealth_light") {
        body.insert("light".into(), light_cases(new_emu));
    }
    if all || want.contains(&"aistealth_nose") {
        body.insert("nose".into(), nose_cases(new_emu));
    }
    if !body.is_empty() {
        let path = out.join("aistealth.json");
        // Keep the parts written by earlier runs.
        let mut old: serde_json::Map<String, Value> = std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        old.extend(body);
        std::fs::write(path, serde_json::to_string(&Value::Object(old)).unwrap()).unwrap();
    }
}

fn sp_file(emu: &mut Emu, wolf: u64) -> ParamFile {
    let list = emu.peek_u64(wolf + 0x11d0);
    let node = emu.peek_u64(list + 8);
    let (nid, nrow) = (u32::from_le_bytes(emu.peek(node + 0x58)), emu.peek_u64(node + 0x60));
    ParamFile::from_row(emu, nrow, nid)
}

/// A copy of a SpEffectParam row (1088 bytes) the caller may change.
fn row_copy(emu: &mut Emu, sp: &mut ParamFile, id: u32) -> u64 {
    let row = sp.row(emu, id);
    let bytes: [u8; 1088] = emu.peek(row);
    let at = emu.alloc(1088);
    emu.write(at, &bytes);
    at
}

/// Puts an active SpEffect node with the given row at the head of a chr's list and opens the
/// holder's stateInfo filter (+0x20, FUN_140bfebf0 / FUN_140bfd8c0 test it first).
fn add_row(emu: &mut Emu, chr: u64, id: u32, row: u64) {
    let list = emu.peek_u64(chr + 0x11d0);
    let head = emu.peek_u64(list + 8);
    let node = emu.alloc(0x80);
    emu.write(node, &[0u8; 0x80]);
    emu.u32(node + 0x58, id);
    emu.u64(node + 0x60, row);
    emu.u64(node + 0x70, head);
    emu.raw_write(list + 8, &node.to_le_bytes());
    emu.raw_write(list + 0x20, &u32::MAX.to_le_bytes());
}

/// Switches off every SpEffect of a chr (flag bit 0), so only the ones added after count.
fn clear_sp(emu: &mut Emu, chr: u64) {
    let list = emu.peek_u64(chr + 0x11d0);
    let mut n = emu.peek_u64(list + 8);
    while n != 0 {
        let f = u32::from_le_bytes(emu.peek(n + 0x1c));
        emu.raw_write(n + 0x1c, &(f | 1).to_le_bytes());
        n = emu.peek_u64(n + 0x70);
    }
}

fn probe(emu: &mut Emu) {
    let wolf = emu.st.snap.as_ref().unwrap().player;
    let front = f32::from_le_bytes(emu.peek(ANGLE_FRONT));
    let back = f32::from_le_bytes(emu.peek(ANGLE_BACK));
    println!("geometry angles front {front} back {back}");
    let wl = emu.peek::<1>(wolf + 0x1acd)[0];
    println!("wolf {wolf:#x} light byte {wl:#04x} sp holder {:#x}", emu.peek_u64(wolf + 0x11d0));
    let mgr = emu.peek_u64(DARK_SIGHT);
    let wp = super::aicpp::pos(emu, wolf);
    let p = emu.alloc(16);
    emu.poke_f32s(p, &[wp[0], wp[1], wp[2], 1.0]);
    let r = emu.call(IN_DARK_SIGHT, &[mgr, p], &[]);
    println!("dark sight manager {mgr:#x}: wolf inside {} (stopped {:?})", r.rax & 0xff, r.stopped);
    for (think, e) in observers(emu) {
        let (_, _, ts) = ai_chain(emu, e).unwrap();
        let ctx = emu.alloc(0x200);
        emu.write(ctx, &vec![0u8; 0x200]);
        emu.call(CTX_FILL, &[ts, ctx], &[]);
        let c8 = emu.peek_u64(ctx + 8);
        let c10 = emu.peek::<1>(ctx + 0x10)[0];
        println!(
            "think {think} chr {e:#x} holder {:#x} ctx+8 {c8:#x} ctx+0x10 {c10} light byte {:#04x}",
            emu.peek_u64(e + 0x11d0),
            emu.peek::<1>(e + 0x1acd)[0]
        );
    }
}

/// FUN_140616f50 on its own: the observer at every 2.5 deg round a target, three distances and
/// heights, the target facing flat or tilted, perceive off / on.
fn bits_cases(emu: &mut Emu) -> Value {
    let front = f32::from_le_bytes(emu.peek(ANGLE_FRONT));
    let back = f32::from_le_bytes(emu.peek(ANGLE_BACK));
    let out = emu.alloc(16);
    let (a, b, c) = (emu.alloc(16), emu.alloc(16), emu.alloc(16));
    let target = [10.0f32, -3.0, 20.0];
    let facings: [[f32; 3]; 3] = [[0.0, 0.0, 1.0], [0.6, 0.0, -0.8], [0.0, 0.6, 0.8]];
    let mut cases = Vec::new();
    for fwd in facings {
        emu.poke_f32s(c, &[fwd[0], fwd[1], fwd[2], 0.0]);
        emu.poke_f32s(b, &[target[0], target[1], target[2], 1.0]);
        for k in 0..144 {
            let ang = (k as f32 * 2.5).to_radians();
            for d in [0.5f32, 4.0, 15.0] {
                for h in [-4.0f32, 0.0, 2.0] {
                    let o = [target[0] + d * ang.sin(), target[1] + h, target[2] + d * ang.cos()];
                    emu.poke_f32s(a, &[o[0], o[1], o[2], 1.0]);
                    for perceive in [0u64, 1] {
                        emu.u32(out, 0xffff);
                        let r = emu.call(BITS, &[out, a, b, c, perceive], &[]);
                        assert!(r.stopped.is_none(), "bits stopped {:?}", r.stopped);
                        let bits = u32::from_le_bytes(emu.peek(out));
                        cases.push(json!({"observer": o, "target": target, "facing": fwd, "perceive": perceive, "bits": bits}));
                    }
                }
            }
        }
    }
    println!("bits: front {front} back {back}, {} cases", cases.len());
    json!({"function": format!("{BITS:#x}"), "front": front, "back": back, "cases": cases})
}

/// One SpEffect set for Wolf: (name, [(id, sightCutLimitType override or -1, stateInfo override or -1)]).
type Set = (&'static str, &'static [(u32, i32, i32)]);

const SETS: [Set; 7] = [
    ("none", &[]),
    ("crouch", &[(109200, -1, -1)]),
    ("wall_hug", &[(109210, -1, -1)]),
    ("hang", &[(109220, -1, -1)]),
    ("wall_hug_type1", &[(109210, 1, -1)]),
    ("crouch_wall_hug", &[(109200, -1, -1), (109210, -1, -1)]),
    ("crouch_state7", &[(109200, -1, 7)]),
];

fn wolf_with(emu: &mut Emu, set: &[(u32, i32, i32)]) -> u64 {
    let wolf = emu.st.snap.as_ref().unwrap().player;
    let mut sp = sp_file(emu, wolf);
    clear_sp(emu, wolf);
    for &(id, ty, si) in set {
        let row = row_copy(emu, &mut sp, id);
        if ty >= 0 {
            emu.u8(row + 0x107, ty as u8);
        }
        if si >= 0 {
            emu.write(row + 0x15e, &(si as u16).to_le_bytes());
        }
        add_row(emu, wolf, id, row);
    }
    wolf
}

/// The five factor functions with every bit pair and the observer flag, then the whole target
/// side (FUN_140bd3e40) with the observer placed round Wolf.
fn factor_cases(new_emu: &dyn Fn() -> Emu) -> Value {
    let mut rows = Vec::new();
    for (name, set) in SETS {
        let mut emu = new_emu();
        let wolf = wolf_with(&mut emu, set);
        let holder = emu.peek_u64(wolf + 0x11d0);
        let scale = f32::from_le_bytes(emu.peek(holder + 0x14));
        let mut direct = Vec::new();
        for (b1, b2) in [(0u64, 0u64), (1, 0), (0, 1), (1, 1)] {
            for flag in [0u64, 1] {
                let cut = emu.call(CUT, &[holder, flag, b1, b2], &[]).xmm0;
                let ang: Vec<f32> = ANG.iter().map(|&f| emu.call(f, &[holder, b1, b2], &[]).xmm0).collect();
                let rate = emu.call(AROUND_RATE, &[holder, b1, b2], &[]).xmm0;
                direct.push(json!({"bit1": b1, "bit2": b2, "flag": flag, "cut": cut, "ang": ang, "around_rate": rate}));
            }
        }
        // The whole target side: the observer (nearest enemy) moved round Wolf at 6 m.
        let (think, e) = observers(&mut emu)[0];
        let (_, _, ts) = ai_chain(&mut emu, e).unwrap();
        let handle = u32::from_le_bytes(emu.peek(wolf + 8)) as u64;
        let bag = emu.peek_u64(wolf + 0x1ff8);
        let module = emu.peek_u64(bag + 0x38);
        let mut placed = Vec::new();
        let mut facing = [0.0f32; 3];
        let mut feet = [0.0f32; 3];
        for k in 0..72 {
            for perceive in [0u8, 1] {
                let ctx = emu.alloc(0x200);
                emu.write(ctx, &vec![0u8; 0x200]);
                emu.call(CTX_FILL, &[ts, ctx], &[]);
                let no_ray = emu.call(CTX_NO_RAY, &[ts, handle], &[]).rax as u8;
                emu.call(CTX_RESET, &[ctx], &[]);
                emu.u8(ctx + 0xc2, no_ray);
                emu.u8(ctx + 0x56, perceive);
                // Wolf's feet first (the target side writes them again, unchanged).
                emu.call(CTX_TARGET, &[module, ctx], &[]);
                feet = f32s(&mut emu, ctx + 0x90);
                facing = f32s(&mut emu, ctx + 0xb0);
                let ang = (k as f32 * 5.0).to_radians();
                let eye = [feet[0] + 6.0 * ang.sin(), feet[1] + 0.5, feet[2] + 6.0 * ang.cos()];
                emu.poke_f32s(ctx + 0x20, &[eye[0], eye[1], eye[2], 1.0]);
                // The observer faces Wolf, so the cone result shows the cuts.
                let to = [feet[0] - eye[0], 0.0, feet[2] - eye[2]];
                let l = (to[0] * to[0] + to[2] * to[2]).sqrt();
                emu.poke_f32s(ctx + 0x40, &[to[0] / l, 0.0, to[2] / l, 0.0]);
                emu.call(CTX_TARGET, &[module, ctx], &[]);
                let normal = emu.call(SIGHT, &[ctx, 0], &[]).rax & 0xff;
                placed.push(json!({
                    "eye": eye,
                    "perceive": perceive,
                    "cut": f32s::<1>(&mut emu, ctx + 0x64)[0],
                    "ang": f32s::<4>(&mut emu, ctx + 0x68),
                    "light_index": emu.peek::<1>(ctx + 0x79)[0],
                    "flag": emu.peek::<1>(ctx + 0x10)[0],
                    "normal": normal,
                }));
            }
        }
        println!("factors {name}: {} direct, {} placed (think {think})", direct.len(), placed.len());
        rows.push(json!({"set": name, "effects": set.iter().map(|s| json!([s.0, s.1, s.2])).collect::<Vec<_>>(), "holder_scale": scale, "direct": direct, "think": think, "feet": feet, "facing": facing, "placed": placed}));
    }
    Value::Array(rows)
}

/// FUN_140617150 for light index 0 / 1 / 2 with each stateInfo 340 SpEffect on the observer, and
/// (to show whose holder ctx +8 is) on Wolf.
fn light_cases(new_emu: &dyn Fn() -> Emu) -> Value {
    let ids = [0u32, 110130, 200500, 200510, 200511, 200520, 200530, 200531, 200532];
    let mut cases = Vec::new();
    for on_wolf in [false, true] {
        for id in ids {
            let mut emu = new_emu();
            let wolf = emu.st.snap.as_ref().unwrap().player;
            let (think, e) = observers(&mut emu)[0];
            if id != 0 {
                let mut sp = sp_file(&mut emu, wolf);
                let row = sp.row(&mut emu, id);
                add_row(&mut emu, if on_wolf { wolf } else { e }, id, row);
            }
            let (_, _, ts) = ai_chain(&mut emu, e).unwrap();
            let ctx = emu.alloc(0x200);
            emu.write(ctx, &vec![0u8; 0x200]);
            emu.call(CTX_FILL, &[ts, ctx], &[]);
            let mut got = Vec::new();
            for idx in 0u64..3 {
                emu.call(LIGHT, &[ctx, idx], &[]);
                got.push(emu.peek::<1>(ctx + 0x79)[0]);
            }
            println!("light {id} on {}: {got:?} (think {think})", if on_wolf { "wolf" } else { "observer" });
            cases.push(json!({"sp": id, "on": if on_wolf { "wolf" } else { "observer" }, "index": [0, 1, 2], "got": got}));
        }
    }
    Value::Array(cases)
}

/// FUN_14061c940 with Wolf 1 m from the sight origin and nose_dist poked to 3: every pair of
/// stateInfo marks (none / 332..335) on Wolf and on the observer.
fn nose_cases(new_emu: &dyn Fn() -> Emu) -> Value {
    let marks = [0u16, 332, 333, 334, 335];
    let mut cases = Vec::new();
    for &w in &marks {
        for &o in &marks {
            let mut emu = new_emu();
            let wolf = emu.st.snap.as_ref().unwrap().player;
            let (think, e) = observers(&mut emu)[0];
            let mut sp = sp_file(&mut emu, wolf);
            for (chr, si) in [(wolf, w), (e, o)] {
                if si != 0 {
                    let row = row_copy(&mut emu, &mut sp, 109200);
                    emu.write(row + 0x15e, &si.to_le_bytes());
                    add_row(&mut emu, chr, 109200, row);
                }
            }
            let (_, _, ts) = ai_chain(&mut emu, e).unwrap();
            let ctx = emu.alloc(0x200);
            emu.write(ctx, &vec![0u8; 0x200]);
            emu.call(CTX_FILL, &[ts, ctx], &[]);
            let wrap = emu.peek_u64(ctx);
            let row = emu.peek_u64(wrap + 8);
            let _ = emu.peek::<2>(row + 0x36);
            emu.write(row + 0x36, &3u16.to_le_bytes());
            emu.call(CTX_RESET, &[ctx], &[]);
            let bag = emu.peek_u64(wolf + 0x1ff8);
            let module = emu.peek_u64(bag + 0x38);
            emu.call(CTX_TARGET, &[module, ctx], &[]);
            let eye: [f32; 3] = f32s(&mut emu, ctx + 0x20);
            emu.poke_f32s(ctx + 0x90, &[eye[0] + 1.0, eye[1], eye[2], 1.0]);
            let c0 = emu.peek::<1>(ctx + 0xc0)[0];
            let c1 = emu.peek::<1>(ctx + 0xc1)[0];
            let hit = emu.call(NOSE, &[ctx], &[]).rax & 0xff;
            println!("nose wolf {w} observer {o}: ctx c0 {c0} c1 {c1} hit {hit} (think {think})");
            cases.push(json!({"wolf": w, "observer": o, "c0": c0, "c1": c1, "hit": hit}));
        }
    }
    Value::Array(cases)
}
