//! `hprate`: where the HP damage of a landed hit gets its rates (rows E12 / E16). Runs the `hits`
//! pilot's duels (whole world frames with the hit manager stepped, hits.rs) with three more hooks:
//! the defender's resolve FUN_140b6c880 at entry (the record's rate fields, and the pokes below),
//! its element sum FUN_140840eb0 at entry (every argument: the five attack amounts, the five
//! defences, the seven rate arrays p5..p11, p12, the flag bytes p13..p15, p16, p17) and the
//! instruction after the call (the sum it returned). Cases change one input each:
//! - Wolf's Attack Power growth SpEffect (160000 + level; the save has 160099),
//! - the defender's ChrActionFlag 41 (module0 +0x70 bit 7, set at the resolve's entry),
//! - AtkParam isArrowAtk (+0x86 bit 5) on the attack's row,
//! - the record's +0x16c / +0x16d / +0x170 (p14 / p15 / p16),
//! - SpEffect 9600 on the attacker (the one FUN_140bfb0e0 looks for: p17).
//!
//! `hprate` (all) or `hprate_<name>` writes extracted/emu/hprate_<name>.json. The duel's own frame
//! file goes to a scratch folder under the output (`hprate_frames/`). Results: docs/kb/damage.md
//! "HP rates and flag factors".

use std::ffi::c_void;
use std::path::Path;
use std::sync::Mutex;

use serde_json::{json, Value};

use super::guard::{drop_sp, enemies, module};
use super::{hits, uc, Emu};

const RESOLVE: u64 = 0x1_40b6_c880;
const SUM: u64 = 0x1_4084_0eb0;
/// The instruction after the resolve's call of the sum (xmm0 = the sum).
const SUM_RET: u64 = 0x1_40b6_dc4f;
/// SpEffect add FUN_1409e9c00 (chr, id, source chr, 0, 0, rates, 1.0, ...), as hits.rs.
const SP_ADD: u64 = 0x1_409e_9c00;
/// AtkParam lookup FUN_1410b5160 (out, type, id): the row at out +0x10.
const ATK_ROW: u64 = 0x1_410b_5160;

/// What to change at the resolve's entry: the defender's module0 (+0x70 |= 0x80), the record's
/// +0x16c / +0x16d / +0x170.
#[derive(Clone, Copy, Default)]
struct Poke {
    flag41_m0: Option<u64>,
    rec: Option<(u8, u8, f32)>,
}

#[derive(Default)]
struct Seen {
    records: Vec<Value>,
    sums: Vec<Value>,
    rets: Vec<f32>,
}
static SEEN: Mutex<Seen> = Mutex::new(Seen { records: Vec::new(), sums: Vec::new(), rets: Vec::new() });
static POKE: Mutex<Poke> = Mutex::new(Poke { flag41_m0: None, rec: None });
static CHRS: Mutex<(u64, u64)> = Mutex::new((0, 0));

fn reg(uc: uc::Engine, id: i32) -> u64 {
    let mut v = 0u64;
    unsafe { uc::uc_reg_read(uc, id, &mut v as *mut u64 as _) };
    v
}

fn mem<const N: usize>(uc: uc::Engine, at: u64) -> [u8; N] {
    let mut b = [0u8; N];
    unsafe { uc::uc_mem_read(uc, at, b.as_mut_ptr() as _, N) };
    b
}

fn put(uc: uc::Engine, at: u64, b: &[u8]) {
    unsafe { uc::uc_mem_write(uc, at, b.as_ptr() as _, b.len()) };
}

fn floats(uc: uc::Engine, at: u64, n: usize) -> Vec<f32> {
    (0..n).map(|i| f32::from_le_bytes(mem(uc, at + 4 * i as u64))).collect()
}

fn who(chr: u64) -> &'static str {
    let (w, g) = *CHRS.lock().unwrap();
    if chr == w {
        "wolf"
    } else if chr == g {
        "general"
    } else {
        "other"
    }
}

extern "C" fn on_resolve(uc: uc::Engine, _addr: u64, _size: u32, _data: *mut c_void) {
    let rec = reg(uc, uc::R8);
    let poke = *POKE.lock().unwrap();
    if let Some(m0) = poke.flag41_m0 {
        let f = u64::from_le_bytes(mem(uc, m0 + 0x70)) | 0x80;
        put(uc, m0 + 0x70, &f.to_le_bytes());
    }
    if let Some((a, b, p16)) = poke.rec {
        put(uc, rec + 0x16c, &[a, b]);
        put(uc, rec + 0x170, &p16.to_le_bytes());
    }
    let s16 = |o: u64| i16::from_le_bytes(mem(uc, rec + o));
    let v = json!({
        "attacker": who(reg(uc, uc::RDX)),
        "amounts": floats(uc, rec, 5),
        "atk_ids": [i32::from_le_bytes(mem(uc, rec + 0x50)), i32::from_le_bytes(mem(uc, rec + 0x54))],
        "s14": [s16(0x14), s16(0x16), s16(0x18), s16(0x1a), s16(0x1c)],
        "f174": floats(uc, rec + 0x174, 7),
        "b16c": mem::<1>(uc, rec + 0x16c)[0], "b16d": mem::<1>(uc, rec + 0x16d)[0], "f170": floats(uc, rec + 0x170, 1)[0],
        "i11c": i32::from_le_bytes(mem(uc, rec + 0x11c)), "bd9": mem::<1>(uc, rec + 0xd9)[0], "bf0": mem::<1>(uc, rec + 0xf0)[0],
        "handles": [u64::from_le_bytes(mem(uc, rec + 0x190)) != 0, u64::from_le_bytes(mem(uc, rec + 0x198)) != 0],
    });
    SEEN.lock().unwrap().records.push(v);
}

extern "C" fn on_sum(uc: uc::Engine, _addr: u64, _size: u32, _data: *mut c_void) {
    let sp = reg(uc, uc::RSP);
    let arg = |i: u64| u64::from_le_bytes(mem(uc, sp + 8 * i));
    let def: Vec<i32> = (0..5).map(|i| i32::from_le_bytes(mem(uc, reg(uc, uc::R9) + 4 * i))).collect();
    let rates: Vec<Vec<f32>> = (5..=11).map(|i| floats(uc, arg(i), 5)).collect();
    let v = json!({
        "atk": floats(uc, reg(uc, uc::R8), 5), "def": def, "rates": rates,
        "p12": f32::from_bits(arg(12) as u32),
        "p13": arg(13) as u8, "p14": arg(14) as u8, "p15": arg(15) as u8,
        "p16": f32::from_bits(arg(16) as u32), "p17": f32::from_bits(arg(17) as u32),
    });
    SEEN.lock().unwrap().sums.push(v);
}

extern "C" fn on_sum_ret(uc: uc::Engine, _addr: u64, _size: u32, _data: *mut c_void) {
    let mut b = [0u8; 16];
    unsafe { uc::uc_reg_read(uc, uc::XMM0, b.as_mut_ptr() as _) };
    SEEN.lock().unwrap().rets.push(f32::from_le_bytes(b[..4].try_into().unwrap()));
}

#[derive(Clone, Copy)]
struct Case {
    name: &'static str,
    /// The hits duel: "w_open" (Wolf's slash 5000010 on the standing General) or "g_open" (the
    /// General's slash 10200100 on idle Wolf).
    duel: &'static str,
    /// Wolf's SpEffects removed / added before the duel.
    wolf_drop: &'static [u32],
    wolf_add: &'static [u32],
    /// SpEffects added on the attacker.
    att_add: &'static [u32],
    flag41: bool,
    /// isArrowAtk set on this AtkParam row (type 1 Pc / 0 Npc, id).
    arrow: Option<(u32, u32)>,
    rec: Option<(u8, u8, f32)>,
}

const W: Case = Case { name: "", duel: "w_open", wolf_drop: &[], wolf_add: &[], att_add: &[], flag41: false, arrow: None, rec: None };
const G: Case = Case { duel: "g_open", ..W };
const AP1: (&[u32], &[u32]) = (&[160099], &[160001]);
const AP10: (&[u32], &[u32]) = (&[160099], &[160010]);
const AP16: (&[u32], &[u32]) = (&[160099], &[160016]);

fn cases() -> Vec<Case> {
    vec![
        Case { name: "w_ap99", ..W },
        Case { name: "w_ap1", wolf_drop: AP1.0, wolf_add: AP1.1, ..W },
        Case { name: "w_ap10", wolf_drop: AP10.0, wolf_add: AP10.1, ..W },
        Case { name: "w_ap_none", wolf_drop: AP1.0, ..W },
        // 4.45 x 100 = 444.99997 in floats: the record keeps 444.
        Case { name: "w_ap16", wolf_drop: AP16.0, wolf_add: AP16.1, ..W },
        // Ako's Sugar's effect 3401 (slashAttackRate 1.125).
        Case { name: "w_ako", wolf_add: &[3401], ..W },
        Case { name: "w_ap16_ako", wolf_drop: AP16.0, wolf_add: &[160016, 3401], ..W },
        Case { name: "w_flag41", flag41: true, ..W },
        Case { name: "w_flag41_arrow", flag41: true, arrow: Some((1, 5000010)), ..W },
        Case { name: "w_p14", rec: Some((1, 0, -1.0)), ..W },
        Case { name: "w_p15", rec: Some((0, 1, -1.0)), ..W },
        Case { name: "w_p14_p16", rec: Some((1, 0, 0.5)), ..W },
        Case { name: "w_sp9600", att_add: &[9600], ..W },
        Case { name: "g_ap99", ..G },
        Case { name: "g_ap1", wolf_drop: AP1.0, wolf_add: AP1.1, ..G },
        Case { name: "g_flag41", flag41: true, ..G },
        Case { name: "g_flag41_arrow", flag41: true, arrow: Some((0, 10200100)), ..G },
    ]
}

fn setup(emu: &mut Emu, c: &Case) {
    let wolf = emu.st.snap.as_ref().unwrap().player;
    let general = enemies(emu).into_iter().find(|&e| u32::from_le_bytes(emu.peek(e + 0x6c)) == 1020).unwrap();
    *CHRS.lock().unwrap() = (wolf, general);
    for &id in c.wolf_drop {
        drop_sp(emu, wolf, id);
    }
    // Added through the game's own add (a node put in by hand is not seen by the attack info's
    // rates, FUN_140bf8900).
    let ones = emu.alloc(0x10);
    emu.poke_f32s(ones, &[1.0; 4]);
    let add = |emu: &mut Emu, chr: u64, id: u32| {
        let r = emu.call(SP_ADD, &[chr, id as u64, chr, 0, 0, ones, 0x3f80_0000, 0, 0, 0], &[]);
        assert!(r.stopped.is_none(), "SpEffect add {id} stopped {:?}", r.stopped);
    };
    for &id in c.wolf_add {
        add(emu, wolf, id);
    }
    let (att, def) = if c.duel == "w_open" { (wolf, general) } else { (general, wolf) };
    for &id in c.att_add {
        add(emu, att, id);
    }
    if let Some((kind, id)) = c.arrow {
        let out = emu.alloc(0x20);
        emu.write(out, &[0xff, 0xff, 0xff, 0xff, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        emu.call(ATK_ROW, &[out, kind as u64, id as u64], &[]);
        let row = emu.peek_u64(out + 0x10);
        assert!(row != 0, "AtkParam {kind}/{id} not found");
        let b = emu.peek::<1>(row + 0x86)[0];
        emu.raw_write(row + 0x86, &[b | 0x20]);
    }
    let m0 = module(emu, def, 0);
    *POKE.lock().unwrap() = Poke { flag41_m0: c.flag41.then_some(m0), rec: c.rec };
    *SEEN.lock().unwrap() = Seen::default();
    emu.hook(uc::HOOK_CODE, on_resolve as *const c_void, RESOLVE, RESOLVE);
    emu.hook(uc::HOOK_CODE, on_sum as *const c_void, SUM, SUM);
    emu.hook(uc::HOOK_CODE, on_sum_ret as *const c_void, SUM_RET, SUM_RET);
}

pub(crate) fn run(new_emu: &dyn Fn() -> Emu, out: &Path, want: &[&str]) {
    let names: Vec<&str> = want.iter().filter_map(|w| w.strip_prefix("hprate_")).collect();
    let frames = out.join("hprate_frames");
    std::fs::create_dir_all(&frames).unwrap();
    let duels = hits::duels();
    for c in cases() {
        if !names.is_empty() && !names.iter().any(|n| n.strip_suffix('*').map_or(*n == c.name, |p| c.name.starts_with(p))) {
            continue;
        }
        let d = duels.iter().find(|d| d.name == c.duel).unwrap();
        let wrap = || {
            let mut e = new_emu();
            setup(&mut e, &c);
            e
        };
        hits::duel(&wrap, &frames, d);
        let seen = std::mem::take(&mut *SEEN.lock().unwrap());
        let v = json!({
            "name": c.name, "duel": c.duel, "wolf_drop": c.wolf_drop, "wolf_add": c.wolf_add, "att_add": c.att_add,
            "flag41": c.flag41, "arrow": c.arrow.map(|a| json!([a.0, a.1])), "rec": c.rec.map(|r| json!([r.0, r.1, r.2])),
            "records": seen.records, "sums": seen.sums, "sum_out": seen.rets,
        });
        println!("hprate {}: {} records, sums {:?}", c.name, seen.records.len(), seen.rets);
        std::fs::write(out.join(format!("hprate_{}.json", c.name)), serde_json::to_string_pretty(&v).unwrap()).unwrap();
    }
}
