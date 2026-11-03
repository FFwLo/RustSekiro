//! Damage-side module ticks and the hit's own gates (topic damagemod).
//!
//! - `damagemod_sa`: the super-armour pool's tick FUN_140bdbcb0 -> FUN_140bdb690 (SprjChrSuperArmorModule,
//!   chr bag +0x40, its pool at +0x18) frame by frame after hits FUN_140bdb470, on the General with
//!   vfunc 0x2c8 (superArmorDurability, 0 on his row) answering 200.
//! - `damagemod_frames`: whole chr frames FUN_140a4b570 on the General after a hit (super armour
//!   drained, hit stop started): which clock the super-armour timer runs on (ChrIns vf25's
//!   FUN_140a05060 ticks it, not vf22) and what the damage and toughness modules do.
//! - `damagemod_amounts`: the hit's five amounts (hit record floats 0-4, FUN_14083ddc0 through the
//!   owner's vfunc 0x1d8) that both hit-stop starts and FUN_140b6e6a0 test, for AtkParam rows.
//! - `damagemod_guard`: the guard resolution FUN_140b6c880 with the umbrella equipped (Wolf's
//!   vfunc 0x338 answering EquipParamWeapon 76000, judge 100) and the parts guard (the hit's body
//!   part +0x160 -> FUN_1407dceb0 -> NpcParam partsAtkParamId, FUN_1410c66a0; +0x1ce).
//!
//! Writes extracted/emu/damagemod_{sa,frames,amounts,guard}.json for src/sim_tests/emu_damagemod.rs.

use super::guard::{add_sp, build_hit, enemies, module, vfunc, ParamFile};
use super::*;

/// SprjChrSuperArmorModule tick (rcx module, xmm1 dt).
const SA_TICK: u64 = 0x1_40bd_bcb0;
/// The pool's hit (rcx pool, r8 hit record: +0x210 x +0xe0 the drain, +0x24 != 0 re-arms the timer).
const SA_HIT: u64 = 0x1_40bd_b470;
/// One character's frame (rcx WorldChrMan, xmm1 dt, r8 chr).
const CHR_FRAME: u64 = 0x1_40a4_b570;
/// SprjChrHitStopModule start (rcx module, xmm1 seconds).
const HIT_STOP_START: u64 = 0x1_40b9_dad0;
const WORLD_CHR_MAN: u64 = 0x1_43d7_a1e0;
const RESOLVE: u64 = 0x1_40b6_c880;
/// The hit's body part -> the chr's guard part 1-8 (rcx ChrCtrl, edx part).
const PART_OF: u64 = 0x1_407d_ceb0;
/// The hit's five amounts from its attack info (rcx amounts, rdx info: the owner's vfunc 0x1d8).
const AMOUNTS: u64 = 0x1_4083_ddc0;

extern "C" {
    fn uc_ctl(uc: uc::Engine, control: u32, ...) -> i32;
}

/// Drops Unicorn's translated code (UC_CTL_TB_FLUSH; proxy.rs: the host crashed after ~39 chr
/// frames on one engine without it).
fn flush_tb(emu: &Emu) {
    unsafe { uc_ctl(emu.uc, 10 | (1 << 30)) };
}

/// `Emu::call` without the per-call block hook (speffect.rs: Unicorn segfaulted after some
/// hundred hooked calls on one engine).
fn call_quiet(emu: &mut Emu, va: u64, args: &[u64], xmm: &[f32]) -> Option<(u64, i32)> {
    for (r, v) in [uc::RCX, uc::RDX, uc::R8, uc::R9].iter().zip(args) {
        emu.set_reg(*r, *v);
    }
    for (i, v) in xmm.iter().enumerate() {
        let b = f32_bits(*v);
        unsafe { uc::uc_reg_write(emu.uc, uc::XMM0 + i as i32, b.as_ptr() as _) };
    }
    let sp = (STACK + STACK_SIZE - 0x1000 - 0x28) & !0xf;
    emu.raw_write(sp, &RETURN.to_le_bytes());
    emu.set_reg(uc::RSP, sp);
    emu.st.running = true;
    emu.st.func = va;
    emu.st.calls.clear();
    let r = unsafe { uc::uc_emu_start(emu.uc, va, RETURN, 0, emu.limit) };
    emu.st.running = false;
    let rip = emu.reg(uc::RIP);
    if r == 0 && rip == RETURN { None } else { Some((rip, r)) }
}

pub(crate) fn run(new_emu: &dyn Fn() -> Emu, out: &Path, want: &[&str]) {
    let all = want.contains(&"damagemod");
    let on = |n: &str| all || want.contains(&n);
    if on("damagemod_sa") {
        sa(new_emu, out);
    }
    if on("damagemod_frames") {
        frames(new_emu, out);
    }
    if on("damagemod_amounts") {
        amounts(new_emu, out);
    }
    if on("damagemod_guard") {
        guard_cases(new_emu, out);
    }
}

/// Writes bytes into the emulator's copy of game memory (mapping the page from the snapshot first).
fn put(emu: &mut Emu, at: u64, bytes: &[u8]) {
    emu.peek::<1>(at);
    emu.peek::<1>(at + bytes.len() as u64 - 1);
    emu.raw_write(at, bytes);
}

fn pf(emu: &mut Emu, at: u64) -> f32 {
    f32::from_le_bytes(emu.peek(at))
}

fn general(emu: &mut Emu) -> u64 {
    enemies(emu).into_iter().find(|&e| u32::from_le_bytes(emu.peek(e + 0x6c)) == 1020).expect("no General in the snapshot")
}

/// A copy of the chr's vtable with some slots replaced, put in place on the chr.
fn patch_vtable(emu: &mut Emu, chr: u64, slots: &[(u64, u64)]) {
    let vt = emu.peek_u64(chr);
    let copy = emu.alloc(0x1000);
    let bytes: Vec<u8> = (0..0x1000u64 / 8).flat_map(|i| emu.peek_u64(vt + 8 * i).to_le_bytes()).collect();
    emu.write(copy, &bytes);
    for &(slot, f) in slots {
        emu.u64(copy + slot, f);
    }
    put(emu, chr, &copy.to_le_bytes());
}

/// The pool (SuperArmor module +0x18): +0x10 current, +0x14 the full size last tick, +0x18 the
/// reserve, +0x1c timer, +0x20 broken, +0x21 broken as of the last tick, +0x24 the
/// stateInfo-155 bonus.
fn pool(emu: &mut Emu, p: u64) -> Value {
    json!({"cur": pf(emu, p + 0x10), "last_full": pf(emu, p + 0x14), "reserve": pf(emu, p + 0x18), "timer": pf(emu, p + 0x1c),
        "broken": emu.peek::<1>(p + 0x20)[0], "was_broken": emu.peek::<1>(p + 0x21)[0], "bonus": pf(emu, p + 0x24)})
}

/// The action flag that holds a broken pool: [[chr+0x50]+0x50]+0x8c bit 9 (TAE ChrActionFlag 71).
fn set_reacting(emu: &mut Emu, chr: u64, on: bool) {
    let ctrl = emu.peek_u64(chr + 0x50);
    let f = emu.peek_u64(ctrl + 0x50);
    let v = u32::from_le_bytes(emu.peek(f + 0x8c));
    let v = if on { v | 1 << 9 } else { v & !(1 << 9) };
    put(emu, f + 0x8c, &v.to_le_bytes());
}

/// Drains the pool as a hit of `amount` super-armour damage (rate 1, timer re-armed).
fn sa_hit(emu: &mut Emu, p: u64, amount: f32) -> Option<(u64, i32)> {
    let dmg = emu.alloc(0x300);
    emu.write(dmg, &[0u8; 0x300]);
    emu.f32(dmg + 0x210, amount);
    emu.f32(dmg + 0xe0, 1.0);
    emu.u32(dmg + 0x24, 1);
    call_quiet(emu, SA_HIT, &[p, 0, dmg], &[])
}

struct SaCase {
    name: &'static str,
    dt: f32,
    frames: usize,
    /// (frame, amount): hits before that frame's tick.
    hits: &'static [(usize, f32)],
    /// Frames [from, to) with the reacting flag set.
    reacting: (usize, usize),
    /// The pool's +0x14 (the full size it saw last tick) at the start.
    last_full: Option<f32>,
}

const SA_MAX: f32 = 200.0;

fn sa(new_emu: &dyn Fn() -> Emu, out: &Path) {
    let cases = [
        SaCase { name: "hit_then_timer", dt: 1.0 / 60.0, frames: 440, hits: &[(0, 60.0)], reacting: (0, 0), last_full: None },
        SaCase { name: "hit_then_timer_30fps", dt: 1.0 / 30.0, frames: 220, hits: &[(0, 60.0)], reacting: (0, 0), last_full: None },
        SaCase { name: "two_hits", dt: 1.0 / 60.0, frames: 560, hits: &[(0, 60.0), (120, 60.0)], reacting: (0, 0), last_full: None },
        SaCase { name: "break", dt: 1.0 / 60.0, frames: 8, hits: &[(0, 400.0)], reacting: (0, 0), last_full: None },
        SaCase { name: "break_held", dt: 1.0 / 60.0, frames: 30, hits: &[(0, 400.0)], reacting: (0, 20), last_full: None },
        SaCase { name: "break_in_timer", dt: 1.0 / 60.0, frames: 40, hits: &[(0, 150.0), (30, 200.0)], reacting: (0, 0), last_full: None },
        SaCase { name: "hit_while_broken", dt: 1.0 / 60.0, frames: 12, hits: &[(0, 400.0), (1, 50.0)], reacting: (0, 0), last_full: None },
        SaCase { name: "full_changed", dt: 1.0 / 60.0, frames: 440, hits: &[], reacting: (0, 0), last_full: Some(150.0) },
        SaCase { name: "reacting_unbroken", dt: 1.0 / 60.0, frames: 10, hits: &[(0, 60.0)], reacting: (0, 10), last_full: None },
    ];
    let mut rows = Vec::new();
    for c in &cases {
        let mut emu = new_emu();
        let chr = general(&mut emu);
        let max = emu.stub("superArmorDurability", 0, Some(SA_MAX));
        patch_vtable(&mut emu, chr, &[(0x2c8, max)]);
        let m = module(&mut emu, chr, 0x40);
        let p = m + 0x18;
        // An idle pool: full, timer -0.01 (its resting value), nothing broken.
        put(&mut emu, p + 0x10, &SA_MAX.to_le_bytes());
        put(&mut emu, p + 0x14, &c.last_full.unwrap_or(SA_MAX).to_le_bytes());
        put(&mut emu, p + 0x18, &0f32.to_le_bytes());
        put(&mut emu, p + 0x1c, &(-0.01f32).to_le_bytes());
        put(&mut emu, p + 0x20, &[0, 0]);
        put(&mut emu, p + 0x24, &0f32.to_le_bytes());
        let f2d8 = vfunc(&mut emu, chr, 0x2d8);
        let delay = emu.call(f2d8, &[chr], &[]).xmm0;
        let ctrl = emu.peek_u64(chr + 0x50);
        let f = emu.peek_u64(ctrl + 0x50);
        let f510 = pf(&mut emu, f + 0x510);
        let start = pool(&mut emu, p);
        // The drain's rates (FUN_140bdb470): the worn saReceiveDamageRate product FUN_140bfdd20 and
        // FUN_140bfd040 (used when >= 0), on the chr's SpEffect list.
        let list = emu.peek_u64(chr + 0x11d0);
        let rate_dd20 = emu.call(0x1_40bf_dd20, &[list], &[]).xmm0;
        let rate_d040 = emu.call(0x1_40bf_d040, &[list], &[]).xmm0;
        let mut frames = Vec::new();
        for i in 0..c.frames {
            let mut hit = Value::Null;
            for &(at, amount) in c.hits {
                if at == i {
                    let r = sa_hit(&mut emu, p, amount);
                    assert!(r.is_none(), "{} hit stopped {r:?}", c.name);
                    hit = pool(&mut emu, p);
                }
            }
            let reacting = i >= c.reacting.0 && i < c.reacting.1;
            set_reacting(&mut emu, chr, reacting);
            let r = call_quiet(&mut emu, SA_TICK, &[m], &[0.0, c.dt]);
            assert!(r.is_none(), "{} tick {i} stopped {r:?}", c.name);
            frames.push(json!({"hit": hit, "reacting": reacting, "after": pool(&mut emu, p)}));
        }
        println!("damagemod_sa {}: {} frames, delay {delay}, +0x510 {f510}", c.name, frames.len());
        rows.push(json!({"name": c.name, "dt": c.dt, "max": SA_MAX, "delay": delay, "rate_dd20": rate_dd20, "rate_d040": rate_d040, "f510": f510, "start": start, "frames": frames}));
    }
    let v = json!({"functions": ["0x140bdbcb0", "0x140bdb690", "0x140bdb470"], "cases": rows});
    std::fs::write(out.join("damagemod_sa.json"), serde_json::to_string_pretty(&v).unwrap()).unwrap();
}

/// The class name (MSVC RTTI) of an object whose vtable lies in the image (hks.rs).
fn class_name(emu: &mut Emu, obj: u64) -> String {
    let vt = emu.peek_u64(obj);
    if !(0x1_4000_0000..0x1_4500_0000).contains(&vt) {
        return String::new();
    }
    let col = emu.peek_u64(vt - 8);
    if !(0x1_4000_0000..0x1_4500_0000).contains(&col) {
        return String::new();
    }
    let td = 0x1_4000_0000 + u32::from_le_bytes(emu.peek::<4>(col + 0xc)) as u64;
    let raw: Vec<u8> = (0..96).map(|i| emu.peek::<1>(td + 0x10 + i)[0]).take_while(|&c| c != 0).collect();
    String::from_utf8_lossy(&raw).into_owned()
}

/// Whole frames of the General after a hit: the pool drained by 60 (or broken by 250) and a hit
/// stop of 0.1 s (or none), AI off. Per frame: chr+0x108 (the clock), the pool, the hit-stop
/// module, the toughness module (+0x48, its gauge at +0x10: +0x20 cur, +0x28 timer) and the
/// damage module's per-frame byte (+0x98 module +0xc8).
fn frames(new_emu: &dyn Fn() -> Emu, out: &Path) {
    let mut scen = Vec::new();
    for (dt, stop, amount) in [(1.0f32 / 60.0, 0.1f32, 60.0f32), (1.0 / 30.0, 0.1, 60.0), (1.0 / 60.0, 0.0, 60.0), (1.0 / 60.0, 0.1, 400.0)] {
        let t = std::time::Instant::now();
        let mut emu = new_emu();
        emu.limit = 500_000_000;
        // AI off (DebugFlags +0x10) so the frames do only what the hit asks.
        put(&mut emu, 0x1_43d7_a376, &[1]);
        let chr = general(&mut emu);
        let max = emu.stub("superArmorDurability", 0, Some(SA_MAX));
        patch_vtable(&mut emu, chr, &[(0x2c8, max)]);
        let wcm = emu.peek_u64(WORLD_CHR_MAN);
        let m = module(&mut emu, chr, 0x40);
        let p = m + 0x18;
        put(&mut emu, p + 0x10, &SA_MAX.to_le_bytes());
        put(&mut emu, p + 0x14, &SA_MAX.to_le_bytes());
        put(&mut emu, p + 0x1c, &(-0.01f32).to_le_bytes());
        put(&mut emu, p + 0x20, &[0, 0]);
        let (t48, d98) = (module(&mut emu, chr, 0x48), module(&mut emu, chr, 0x98));
        let names = json!({"chr+0x10e0": class_name(&mut emu, chr + 0x10e0), "bag+0x40": class_name(&mut emu, m),
            "bag+0x48": class_name(&mut emu, t48), "bag+0x98": class_name(&mut emu, d98)});
        assert!(sa_hit(&mut emu, p, amount).is_none());
        let hs = module(&mut emu, chr, 0x90);
        if stop > 0.0 {
            emu.call(HIT_STOP_START, &[hs], &[0.0, stop]);
        }
        let tough = module(&mut emu, chr, 0x48);
        let dmod = module(&mut emu, chr, 0x98);
        let mut frames = Vec::new();
        for _ in 0..24 {
            flush_tb(&emu);
            let r = emu.call(CHR_FRAME, &[wcm, 0, chr], &[0.0, dt]);
            frames.push(json!({
                "chr_dt": pf(&mut emu, chr + 0x108), "pool": pool(&mut emu, p),
                "hit_stop": {"state": emu.peek::<1>(hs + 0x20)[0], "timer": pf(&mut emu, hs + 0x10), "scaled": pf(&mut emu, hs + 0x18)},
                "toughness": {"cur": pf(&mut emu, tough + 0x20), "last": pf(&mut emu, tough + 0x24), "timer": pf(&mut emu, tough + 0x28)},
                "damage_c8": emu.peek::<1>(dmod + 0xc8)[0],
                "stopped": r.stopped.map(|(a, e)| format!("{a:#x} error {e}")),
            }));
        }
        println!("damagemod_frames dt {dt:.4} stop {stop} amount {amount}: {:.1} s, {names}", t.elapsed().as_secs_f32());
        scen.push(json!({"dt": dt, "hit_stop": stop, "amount": amount, "max": SA_MAX, "classes": names, "frames": frames}));
    }
    let v = json!({"functions": ["0x140a4b570", "0x140a05060", "0x140bdbcb0", "0x140be51e0", "0x140b6a940"], "scenarios": scen});
    std::fs::write(out.join("damagemod_frames.json"), serde_json::to_string_pretty(&v).unwrap()).unwrap();
}

/// AtkParam rows of an exported param file (extracted/json/params/<name>.json).
fn param_rows(out: &Path, name: &str) -> Vec<Value> {
    let path = out.parent().unwrap().join(format!("json/params/{name}.json"));
    let v: Value = serde_json::from_str(&std::fs::read_to_string(&path).expect("params json")).unwrap();
    match &v["rows"] {
        Value::Array(a) => a.clone(),
        Value::Object(o) => o.values().cloned().collect(),
        _ => Vec::new(),
    }
}

const ELEMENTS: [&str; 5] = ["atkPhys", "atkMag", "atkFire", "atkThun", "atkDark"];
const CORRECTIONS: [&str; 5] = ["atkPhysCorrection", "atkMagCorrection", "atkFireCorrection", "atkThunCorrection", "atkDarkCorrection"];

/// A spread of rows: every row of the id range, the first rows with all five fields 0, and the
/// first rows with only one field set, for each field.
fn pick(rows: &[Value], range: std::ops::Range<i64>, fields: &[&str; 5], n_range: usize) -> Vec<u32> {
    let f = |r: &Value, k: &str| r[k].as_f64().unwrap_or(0.0);
    let mut ids: Vec<u32> = rows.iter().filter(|r| range.contains(&r["id"].as_i64().unwrap_or(-1))).take(n_range).map(|r| r["id"].as_i64().unwrap() as u32).collect();
    ids.extend(rows.iter().filter(|r| fields.iter().all(|k| f(r, k) == 0.0) && r["id"].as_i64().unwrap_or(0) > 0).take(40).map(|r| r["id"].as_i64().unwrap() as u32));
    for k in fields {
        ids.extend(rows.iter().filter(|r| f(r, k) > 0.0 && fields.iter().filter(|o| *o != k).all(|o| f(r, o) == 0.0)).take(8).map(|r| r["id"].as_i64().unwrap() as u32));
    }
    ids.sort();
    ids.dedup();
    ids
}

fn amounts(new_emu: &dyn Fn() -> Emu, out: &Path) {
    let npc = param_rows(out, "AtkParam_Npc");
    let pc = param_rows(out, "AtkParam_Pc");
    let npc_ids = pick(&npc, 10_200_000..10_210_000, &ELEMENTS, 120);
    let pc_ids = pick(&pc, 5_000_000..5_010_000, &CORRECTIONS, 80);
    let mut cases = Vec::new();
    for (pc_side, ids) in [(false, &npc_ids), (true, &pc_ids)] {
        for chunk in ids.chunks(150) {
            let mut emu = new_emu();
            let wolf = emu.st.snap.as_ref().expect("damagemod needs --snapshot").player;
            let owner = if pc_side { wolf } else { general(&mut emu) };
            for &id in chunk {
                // The attack info as build_hit makes it ([0] owner, +8 AtkParam id, +0xc 1 for
                // AtkParam_Pc, +0x12c weapon), and the amounts block as FUN_1410b6880 starts it
                // (floats 0-4 zero, rates 1).
                let info = emu.alloc(0x200);
                emu.write(info, &[0u8; 0x200]);
                emu.u64(info, owner);
                emu.u32(info + 8, id);
                emu.u32(info + 0xc, pc_side as u32);
                emu.u32(info + 0x12c, if pc_side { 5000 } else { u32::MAX });
                emu.u32(info + 0x134, u32::MAX);
                // The attack's own rates the attacker's attack registration fills: +0x70 physical,
                // +0xa4-0xb0 magic / fire / lightning / dark (1 = as the row), the flat adds
                // +0x2c / +0x60-0x6c 0, and +0x138 on (a player's weapon attack power is used).
                emu.f32(info + 0x70, 1.0);
                emu.poke_f32s(info + 0xa4, &[1.0; 4]);
                emu.u8(info + 0x138, 1);
                let am = emu.alloc(0x40);
                emu.write(am, &[0u8; 0x40]);
                emu.poke_f32s(am + 0x18, &[1.0; 7]);
                let r = emu.call(AMOUNTS, &[am, info], &[]);
                let a: Vec<f32> = (0..5).map(|i| emu.rf32(am + 4 * i)).collect();
                cases.push(json!({"pc": pc_side, "id": id, "amounts": a, "ran": r.stopped.is_none()}));
            }
        }
    }
    println!("damagemod_amounts: {} npc rows, {} pc rows", npc_ids.len(), pc_ids.len());
    let v = json!({"functions": ["0x1410b6880", "0x14083ddc0"], "weapon_pc": 5000, "cases": cases});
    std::fs::write(out.join("damagemod_amounts.json"), serde_json::to_string_pretty(&v).unwrap()).unwrap();
}

/// One guard resolution FUN_140b6c880 (as guard.rs one_case): `wolf_defends` against the
/// General's `atk` (else the General against Wolf's), the defender's ShieldBlock judge `judge`
/// (None: not guarding), the attack `angle` degrees off the defender's face, the hit's body part
/// `part` (+0x160), and on Wolf `weapon`: vfunc 0x338 answering that weapon id for every slot.
#[allow(clippy::too_many_arguments)]
fn resolve(new_emu: &dyn Fn() -> Emu, wolf_defends: bool, atk: u32, judge: Option<u32>, angle: f32, part: i32, weapon: Option<u32>, parts_row: Option<u32>, window: bool) -> Value {
    let mut emu = new_emu();
    let emu = &mut emu;
    let wolf = emu.st.snap.as_ref().unwrap().player;
    let gen = general(emu);
    let (att, def) = if wolf_defends { (gen, wolf) } else { (wolf, gen) };
    if let Some(w) = weapon {
        let stub = emu.stub("weapon_in_slot", w as u64, None);
        patch_vtable(emu, wolf, &[(0x338, stub)]);
    }
    // The General's NpcParam row (vfunc 0x178 -> +8): partsAtkParamId1-8 at +0x1e4.
    let f178 = vfunc(emu, gen, 0x178);
    let npc_holder = emu.call(f178, &[gen], &[]).rax;
    let npc_row = emu.peek_u64(npc_holder + 8);
    let parts_before: Vec<i32> = (0..8).map(|i| i32::from_le_bytes(emu.peek(npc_row + 0x1e4 + 4 * i))).collect();
    if let Some(row) = parts_row {
        for i in 0..8 {
            put(emu, npc_row + 0x1e4 + 4 * i, &row.to_le_bytes());
        }
    }
    let (hit, b) = build_hit(emu, att, atk, !wolf_defends, if wolf_defends { -1 } else { 5000 });
    assert!(b.stopped.is_none());
    emu.f32(hit, 100.0);
    emu.f32(hit + 0x188, 1.0);
    let fwd = emu.alloc(0x10);
    emu.call(0x1_409e_f970, &[def, fwd], &[]);
    let (fx, fz) = (emu.rf32(fwd), emu.rf32(fwd + 8));
    let (sn, cs) = angle.to_radians().sin_cos();
    emu.f32(hit + 0x150, -(fx * cs - fz * sn));
    emu.f32(hit + 0x158, -(fx * sn + fz * cs));
    emu.u32(hit + 0x160, part as u32);
    emu.u32(hit + 0x1d4, u32::MAX);
    let m0 = module(emu, def, 0);
    let flags = emu.peek_u64(m0 + 0x70);
    let flags = if judge.is_some() { flags | 4 } else { flags & !4 };
    put(emu, m0 + 0x70, &flags.to_le_bytes());
    put(emu, m0 + 0xb08, &judge.unwrap_or(u32::MAX).to_le_bytes());
    if window {
        let list = emu.peek_u64(wolf + 0x11d0);
        let node = emu.peek_u64(list + 8);
        let (nid, nrow) = (u32::from_le_bytes(emu.peek(node + 0x58)), emu.peek_u64(node + 0x60));
        let mut sp = ParamFile::from_row(emu, nrow, nid);
        add_sp(emu, &mut sp, def, if wolf_defends { 105010 } else { 200220 });
    }
    // What the defender's guard angle vfunc 0x258 answers, and the part the hit's index maps to.
    let f258 = vfunc(emu, def, 0x258);
    let angle_def = emu.call(f258, &[def], &[]).rax as u32 as i32;
    let ctrl = emu.peek_u64(def + 0x50);
    let part_of = emu.call(PART_OF, &[ctrl, part as u32 as u64], &[]).rax as u32 as i32;
    let dmod = module(emu, def, 0x98);
    let r = emu.call(RESOLVE, &[dmod, att, hit], &[]);
    json!({
        "wolf_defends": wolf_defends, "atk": atk, "judge": judge, "angle": angle, "part": part, "weapon": weapon,
        "parts_row": parts_row, "parts_before": parts_before, "window": window,
        "guard_angle": angle_def, "part_of": part_of, "flick_atk": i32::from_le_bytes(emu.read(hit + 0x40)),
        "guarded": emu.ru8(hit + 0x164), "deflected": emu.ru8(hit + 0x1c7), "parts_guard": emu.ru8(hit + 0x1ce),
        "guard_row": i32::from_le_bytes(emu.read(hit + 0x1d0)), "parts_judge": i32::from_le_bytes(emu.read(hit + 0x1d4)),
        "ran": r.stopped.is_none(),
    })
}

fn guard_cases(new_emu: &dyn Fn() -> Emu, out: &Path) {
    let mut cases = Vec::new();
    // The umbrella: Wolf guards with judge 100 (BehaviorParam_PC 107600100) and the umbrella as the
    // weapon in hand (EquipParamWeapon 76000, guardAngle 180), against the General's slash; the
    // katana (vfunc 0x338 as it is) for comparison.
    for weapon in [Some(76000u32), None] {
        for angle in [0.0f32, 60.0, 89.0, 91.0, 120.0, 150.0, 170.0, 179.0, 180.0] {
            cases.push(resolve(new_emu, true, 10200100, Some(100), angle, -1, weapon, None, false));
        }
    }
    for angle in [91.0f32, 179.0, 180.0] {
        cases.push(resolve(new_emu, true, 10200100, Some(100), angle, -1, Some(76000), None, true));
    }
    // Parts guard: Wolf's slash on the General, not guarding, at each body part index; then with
    // his partsAtkParamId1-8 set to his guard row 10200900 (guardBreakRate 20) in the emulator copy.
    for part in [-1, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 12, 16, 20, 24, 30] {
        cases.push(resolve(new_emu, false, 5000010, None, 0.0, part, None, None, false));
    }
    for part in [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 12, 16, 20, 24, 30] {
        cases.push(resolve(new_emu, false, 5000010, None, 0.0, part, None, Some(10200900), false));
    }
    println!("damagemod_guard: {} cases", cases.len());
    let v = json!({"functions": ["0x140b6c880", "0x1407dceb0", "0x1410c66a0"], "cases": cases});
    std::fs::write(out.join("damagemod_guard.json"), serde_json::to_string_pretty(&v).unwrap()).unwrap();
}
