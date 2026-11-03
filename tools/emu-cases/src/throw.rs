//! Throw and deathblow checks on the snapshot (topic throw): FUN_140be0270 the ThrowParam range
//! test, FUN_140be3810 the ThrowKindParam throwable-state test, FUN_140be3a90 a row's whole test.

use super::*;

const WORLD_CHR_MAN: u64 = 0x1_43d7_a1e0;
const ENEMY_VT: u64 = 0x1_42a2_7f28;

pub(super) fn try_u64(emu: &mut Emu, at: u64) -> Option<u64> {
    if at < 0x10000 || at > 0x7fff_ffff_ffff || at % 8 != 0 {
        return None;
    }
    let mut b = [0u8; 8];
    if unsafe { uc::uc_mem_read(emu.uc, at, b.as_mut_ptr() as _, 8) } == 0 {
        return Some(u64::from_le_bytes(b));
    }
    let (from, bytes) = emu.st.snap.as_mut()?.window(at)?;
    emu.map(from, bytes.len() as u64);
    emu.raw_write(from, &bytes);
    Some(u64::from_le_bytes(emu.read(at)))
}

/// Every EnemyIns in WorldChrMan's chr sets (as guard.rs).
pub(super) fn enemies(emu: &mut Emu) -> Vec<u64> {
    let wcm = emu.peek_u64(WORLD_CHR_MAN);
    let mut out = Vec::new();
    for off in (0..0x2000).step_by(8) {
        let Some(p) = try_u64(emu, wcm + off) else { continue };
        if try_u64(emu, p).is_none() {
            continue;
        }
        for o2 in (0..0x4000).step_by(8) {
            if let Some(q) = try_u64(emu, p + o2) {
                if !out.contains(&q) && try_u64(emu, q) == Some(ENEMY_VT) {
                    out.push(q);
                }
            }
        }
    }
    out
}

pub(super) fn module(emu: &mut Emu, chr: u64, off: u64) -> u64 {
    let bag = emu.peek_u64(chr + 0x1ff8);
    emu.peek_u64(bag + off)
}

pub(super) fn f32s<const N: usize>(emu: &mut Emu, at: u64) -> [f32; N] {
    std::array::from_fn(|i| f32::from_le_bytes(emu.peek(at + 4 * i as u64)))
}

pub(super) fn i32at(emu: &mut Emu, at: u64) -> i32 {
    i32::from_le_bytes(emu.peek(at))
}

fn explore(emu: &mut Emu) {
    let wolf = emu.st.snap.as_ref().unwrap().player;
    let mut chrs = vec![wolf];
    chrs.extend(enemies(emu));
    let wph = module(emu, wolf, 0x68);
    let wp = f32s::<3>(emu, wph + 0x80);
    let mut rows = Vec::new();
    for c in chrs {
        let ph = module(emu, c, 0x68);
        let p = f32s::<3>(emu, ph + 0x80);
        let d = ((p[0] - wp[0]).powi(2) + (p[1] - wp[1]).powi(2) + (p[2] - wp[2]).powi(2)).sqrt();
        rows.push((d, c, p));
    }
    rows.sort_by(|a, b| a.0.total_cmp(&b.0));
    for (d, c, p) in rows.into_iter().take(8) {
        let ph = module(emu, c, 0x68);
        let th = module(emu, c, 0x88);
        let data = module(emu, c, 0x18);
        let id = i32at(emu, c + 0x6c);
        let yaw = f32s::<1>(emu, ph + 0x74)[0];
        let ph90 = f32s::<4>(emu, ph + 0x90);
        let th90 = f32s::<4>(emu, th + 0x90);
        let thb: [u8; 4] = emu.peek(th + 0x78);
        let thv = emu.peek_u64(th);
        let e8 = i32at(emu, th + 0xe8);
        let f15 = emu.peek::<1>(c + 0x1a15)[0];
        let brk: [u8; 3] = emu.peek(data + 0x1c4);
        let hp = i32at(emu, data + 0x130);
        let post = i32at(emu, data + 0x148);
        println!("chr {c:x} id {id} d {d:.2} pos {p:?} yaw {yaw:.3} ph90 {ph90:?} | m88 vt {thv:x} 90 {th90:?} 78 {thb:?} e8 {e8} | 1a15 {f15} brk {brk:?} hp {hp} post? {post}");
    }
}

pub fn run(new_emu: &dyn Fn() -> Emu, out: &Path, want: &[&str]) {
    if want.contains(&"throw_explore") {
        explore(&mut new_emu());
    }
    if want.contains(&"throw") || want.contains(&"throw_kinds") {
        kinds(new_emu, out);
    }
    if want.contains(&"throw") || want.contains(&"throw_ranges") {
        ranges(new_emu, out);
    }
}

const RANGE: u64 = 0x1_40be_0270;
const KIND: u64 = 0x1_40be_3810;
const ROW: u64 = 0x1_40be_3a90;
const TABLE: u64 = 0x1_40be_0a40;
const ROW_REF: u64 = 0x1_4095_e0b0;
const KIND_REF: u64 = 0x1_410d_0200; // by id (FUN_1410d0370 takes a row index)
const CHR_POS: u64 = 0x1_40bb_fc00;

/// The snapshot's chr of this chr number nearest to Wolf.
pub(super) fn nearest(emu: &mut Emu, id: i32) -> u64 {
    let wolf = emu.st.snap.as_ref().unwrap().player;
    let wph = module(emu, wolf, 0x68);
    let wp = f32s::<3>(emu, wph + 0x80);
    let mut best = (f32::MAX, 0);
    for c in enemies(emu) {
        if i32at(emu, c + 0x6c) != id {
            continue;
        }
        let ph = module(emu, c, 0x68);
        let p = f32s::<3>(emu, ph + 0x80);
        let d = (0..3).map(|i| (p[i] - wp[i]).powi(2)).sum::<f32>();
        if d < best.0 {
            best = (d, c);
        }
    }
    assert!(best.1 != 0, "no c{id} in the snapshot");
    best.1
}

/// Puts a chr at `p` facing `yaw`: physics +0x80 / +0x74 and the yaw the throw checks read, the
/// throw module's (+0x88) +0x94 (in the snapshot equal to physics +0x74 on every chr).
pub(super) fn place(emu: &mut Emu, chr: u64, p: [f32; 3], yaw: f32) {
    let ph = module(emu, chr, 0x68);
    emu.poke_f32s(ph + 0x80, &p);
    emu.poke_f32s(ph + 0x74, &[yaw]);
    let th = module(emu, chr, 0x88);
    emu.poke_f32s(th + 0x94, &[yaw]);
}

/// The position the checks read (FUN_140bbfc00: physics +0x80, plus a height offset).
pub(super) fn chr_pos(emu: &mut Emu, chr: u64) -> [f32; 3] {
    let ph = module(emu, chr, 0x68);
    let buf = emu.alloc(16);
    emu.call(CHR_POS, &[ph, buf], &[]);
    f32s::<3>(emu, buf)
}

pub(super) fn row_ref(emu: &mut Emu, id: u32) -> u64 {
    // {row, id, ?}: FUN_14095e0b0(ref, id) (FUN_14095e0e0 takes a row index).
    let r = emu.alloc(16);
    let res = emu.call(ROW_REF, &[r, id as u64], &[]);
    assert!(res.stopped.is_none() && emu.peek_u64(r) != 0, "ThrowParam {id}");
    r
}

/// {?, id, row}: FUN_1410d0200(ref, id) (FUN_1410d0370 takes a row index).
pub(super) fn kind_ref(emu: &mut Emu, id: u32) -> u64 {
    let r = emu.alloc(32);
    let res = emu.call(KIND_REF, &[r, id as u64], &[]);
    assert!(res.stopped.is_none() && emu.peek_u64(r + 8) != 0, "ThrowKindParam {id}");
    r
}

/// A small LCG: the same cases every run.
pub(super) struct Lcg(pub(super) u64);
impl Lcg {
    pub(super) fn f(&mut self, lo: f32, hi: f32) -> f32 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        lo + (hi - lo) * ((self.0 >> 40) as f32 / (1u64 << 24) as f32)
    }
}

/// The throwable-state table FUN_140be0a40 (defender state, ThrowKindParam throwKindCategory)
/// and the kind test FUN_140be3810 on Wolf -> the General with the General's forced state
/// (throw module +0x7a, written by TAE 932) and HP varied.
fn kinds(new_emu: &dyn Fn() -> Emu, out: &Path) {
    let cats: Vec<u32> = (0..=21).chain([100, 101]).collect();
    let mut table = Vec::new();
    let mut emu = new_emu();
    for s in 0..=127u32 {
        for &c in &cats {
            let r = emu.call(TABLE, &[s as u64, c as u64], &[]);
            assert!(r.stopped.is_none());
            if r.rax & 1 == 1 {
                table.push(json!([s, c]));
            }
        }
        if s % 32 == 31 {
            emu = new_emu();
        }
    }
    let mut cases = Vec::new();
    for kind in [0u32, 40000, 40100, 41000, 41100, 130000, 250000] {
        let mut emu = new_emu();
        let wolf = emu.st.snap.as_ref().unwrap().player;
        let gen = nearest(&mut emu, 1020);
        let th = module(&mut emu, gen, 0x88);
        let data = module(&mut emu, gen, 0x18);
        let hp0 = i32at(&mut emu, data + 0x130);
        for state in [0u8, 101, 102, 103, 104, 105, 108, 110] {
            for hp in [hp0, 0] {
                emu.raw_write(th + 0x7a, &[state]);
                emu.raw_write(data + 0x130, &hp.to_le_bytes());
                let k = kind_ref(&mut emu, kind);
                let r = emu.call(KIND, &[wolf, gen, k], &[]);
                if r.stopped.is_some() {
                    println!("kind {kind} state {state} hp {hp} stopped {:?} trail {:x?}", r.stopped, emu.st.trail);
                    continue;
                }
                cases.push(json!({"kind": kind, "state": state, "hp": hp, "ok": r.rax & 1}));
            }
        }
    }
    let body = json!({"function": "FUN_140be0a40 / FUN_140be3810", "table": table, "cases": cases});
    std::fs::write(out.join("throw_kinds.json"), serde_json::to_string_pretty(&body).unwrap()).unwrap();
    println!("throw_kinds: {} table pairs, {} kind cases", table.len(), cases.len());
}

/// The row tests on Wolf -> the General (c1020): FUN_140be0270 range (Dist, upperYRange /
/// lowerYRange, diffAngMyToDef, DiffAngMin / Max) and FUN_140be3a90 the whole row (SpEffect
/// conditions, range, fall orbit, the sight cast), the General put around Wolf.
fn ranges(new_emu: &dyn Fn() -> Emu, out: &Path) {
    let rows: [(u32, f32, f32); 5] = [(11020000, 2.6, 1.6), (11020110, 2.6, 1.6), (11020020, 2.6, 1.6), (11020001, 2.6, 1.6), (11020150, 22.0, 17.0)];
    let mut rng = Lcg(0x5e41_70);
    let mut cases = Vec::new();
    let mut stops = 0;
    for (row, rmax, ymax) in rows {
        for chunk in 0..4 {
            let mut emu = new_emu();
            let wolf = emu.st.snap.as_ref().unwrap().player;
            let gen = nearest(&mut emu, 1020);
            let wph = module(&mut emu, wolf, 0x68);
            let w0 = f32s::<3>(&mut emu, wph + 0x80);
            let rr = row_ref(&mut emu, row);
            for i in 0..150 {
                let wyaw = rng.f(-std::f32::consts::PI, std::f32::consts::PI);
                // Spread, then near the front, then level, then the yaw edges.
                let (bearing, r, dy, gyaw) = match chunk {
                    0 => (rng.f(-180.0, 180.0), rng.f(0.0, rmax), rng.f(-ymax, ymax), rng.f(-180.0, 180.0)),
                    1 => (rng.f(-50.0, 50.0), rng.f(0.0, rmax), rng.f(-ymax, ymax) * 0.5, rng.f(-180.0, 180.0)),
                    2 => (rng.f(-40.0, 40.0), rng.f(0.3, rmax * 0.8), rng.f(-0.3, 0.3), rng.f(-180.0, 180.0)),
                    _ => (rng.f(-40.0, 40.0), rng.f(0.3, rmax * 0.8), rng.f(-0.3, 0.3), [0.0, 89.5, 90.5, 179.5, -90.5][i % 5]),
                };
                // The exe's forward for yaw y is (-sin y, 0, -cos y) (the diffAngMyToDef cases).
                let a = wyaw + bearing.to_radians();
                let gp = [w0[0] - r * a.sin(), w0[1] + dy, w0[2] - r * a.cos()];
                let gy = wyaw + gyaw.to_radians();
                place(&mut emu, wolf, w0, wyaw);
                place(&mut emu, gen, gp, gy);
                let (wp, ep) = (chr_pos(&mut emu, wolf), chr_pos(&mut emu, gen));
                let r1 = emu.call(RANGE, &[wolf, gen, rr, 0], &[]);
                let r2 = emu.call(ROW, &[wolf, gen, 0, rr], &[]);
                if r1.stopped.is_some() || r2.stopped.is_some() {
                    stops += 1;
                    if stops < 5 {
                        println!("row {row} stopped {:?} {:?} trail {:x?}", r1.stopped, r2.stopped, emu.st.trail);
                    }
                    continue;
                }
                cases.push(json!({
                    "row": row, "wolf": wp, "wolf_yaw": wyaw, "enemy": ep, "enemy_yaw": gy,
                    "range": r1.rax & 1, "row_ok": r2.rax & 1,
                }));
            }
        }
    }
    let body = json!({"function": "FUN_140be0270 / FUN_140be3a90", "cases": cases});
    std::fs::write(out.join("throw_ranges.json"), serde_json::to_string_pretty(&body).unwrap()).unwrap();
    println!("throw_ranges: {} cases, {stops} stopped", cases.len());
}
