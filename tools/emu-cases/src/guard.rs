//! Guard and deflect resolution on the snapshot (topic guard).

use super::*;

const WORLD_CHR_MAN: u64 = 0x1_43d7_a1e0;
const ENEMY_VT: u64 = 0x1_42a2_7f28;

/// Reads 8 bytes of the game's memory, or None where the snapshot has nothing.
fn try_u64(emu: &mut Emu, at: u64) -> Option<u64> {
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

/// Every EnemyIns in WorldChrMan's chr sets (tools/memscope/recorder.lua's scan).
pub(crate) fn enemies(emu: &mut Emu) -> Vec<u64> {
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

pub(crate) fn vfunc(emu: &mut Emu, obj: u64, slot: u64) -> u64 {
    let vt = emu.peek_u64(obj);
    emu.peek_u64(vt + slot)
}

/// A module of the chr's +0x1ff8 bag (0 action flags, 0x18 data, 0x98 damage).
pub(crate) fn module(emu: &mut Emu, chr: u64, off: u64) -> u64 {
    let bag = emu.peek_u64(chr + 0x1ff8);
    emu.peek_u64(bag + off)
}

fn pos(emu: &mut Emu, chr: u64) -> [f32; 3] {
    let bag = emu.peek_u64(chr + 0x1ff8);
    let ph = emu.peek_u64(bag + 0x68);
    std::array::from_fn(|i| f32::from_le_bytes(emu.peek(ph + 0x80 + 4 * i as u64)))
}

/// The SpEffect list of a chr ([chr+0x11d0] +8 head, node +0x70 next, +0x58 id, +0x1c flags,
/// +0x60 SpEffectParam row).
fn sp_effects(emu: &mut Emu, chr: u64) -> Vec<(u32, u32, u64)> {
    let list = emu.peek_u64(chr + 0x11d0);
    let mut n = emu.peek_u64(list + 8);
    let mut out = Vec::new();
    while n != 0 && out.len() < 200 {
        let id = u32::from_le_bytes(emu.peek(n + 0x58));
        let flags = u32::from_le_bytes(emu.peek(n + 0x1c));
        let row = emu.peek_u64(n + 0x60);
        out.push((id, flags, row));
        n = emu.peek_u64(n + 0x70);
    }
    out
}

pub fn run(new_emu: &dyn Fn() -> Emu, out: &Path) {
    if std::env::var("GUARD_EXPLORE").is_ok() {
        explore(&mut new_emu(), out);
    }
    if std::env::var("GUARD_BUILD").is_ok() {
        build_probe(&mut new_emu());
    }
    if std::env::var("GUARD_SLOTS").is_ok() {
        let mut emu = new_emu();
        let w = emu.st.snap.as_ref().unwrap().player;
        let f1c0 = vfunc(&mut emu, w, 0x1c0);
        for slot in 0..12u64 {
            let rows: Vec<i64> = [90u64, 100].iter().map(|&j| emu.call(f1c0, &[w, j, slot, 0], &[]).rax as u32 as i32 as i64).collect();
            println!("slot {slot}: judge 90 / 100 -> {rows:?}");
        }
        return;
    }
    let wolf = Case { wolf_defends: true, atk: 10200100, guard: true, angle: 0.0, window: false, spam: 0, posture: None, extra: &[], guard_row: None };
    let general = Case { wolf_defends: false, atk: 5000010, ..wolf };
    let mut cases = Vec::new();
    // Wolf against the General: a plain slash (guardAtkRate 30), the thrust (disableGuard), the
    // sweep (both disables), the Demon's blast row (guardAtkRate 100) and template 17 (0).
    for atk in [10200100, 10200200, 10200570, 70200330, 17] {
        for guard in [false, true] {
            for window in [false, true] {
                cases.push(Case { atk, guard, window, ..wolf });
            }
        }
    }
    // The guard arc (the attack's direction turned away from the face).
    for angle in [30.0, 60.0, 80.0, 89.0, 91.0, 100.0, 120.0, 150.0, 180.0] {
        cases.push(Case { angle, ..wolf });
    }
    for angle in [89.0, 91.0, 180.0] {
        cases.push(Case { angle, window: true, ..wolf });
    }
    // The guard-spam chain 105020-105023, with and without the deflect window.
    for spam in 1..=4 {
        cases.push(Case { window: true, spam, ..wolf });
        cases.push(Case { spam, ..wolf });
    }
    // Posture running out on a block and on a deflect.
    for posture in [300, 192, 191, 100, 1] {
        cases.push(Case { posture: Some(posture), ..wolf });
        cases.push(Case { posture: Some(posture), window: true, ..wolf });
    }
    // Other flick SpEffects: 1130 (stateInfo 134, defFlickPower 65), 900001 (158, 40).
    for extra in [&[1130u32][..], &[900001]] {
        for atk in [10200100, 70200330, 10200200] {
            cases.push(Case { atk, extra, ..wolf });
        }
    }
    // The General against Wolf: the slash (correction 300), the charged slash (disableGuard),
    // the hanging attack (both) and a deathblow start (correction 900).
    for atk in [5000010, 5000000, 5000080, 5000600] {
        for guard in [false, true] {
            for window in [false, true] {
                cases.push(Case { atk, guard, window, ..general });
            }
        }
    }
    for angle in [30.0, 55.0, 59.0, 61.0, 65.0, 90.0, 180.0] {
        cases.push(Case { angle, ..general });
    }
    cases.push(Case { posture: Some(10), ..general });
    cases.push(Case { posture: Some(10), window: true, ..general });
    // The Loaded Umbrella's guard row (AtkParam_Pc 7600100, guardAttribute 1): the
    // vsGuardAttribute1 disables (the thrust 0/0, the sweep 1/1, the Shura stomp 10700370 0/0).
    for atk in [10200100, 10200200, 10200570, 10700370] {
        for window in [false, true] {
            cases.push(Case { atk, window, guard_row: Some(107600100), ..wolf });
        }
    }
    cases.push(Case { atk: 10700370, ..wolf });
    let mut rows = Vec::new();
    for c in &cases {
        let v = one_case(new_emu, c);
        if std::env::var("GUARD_PRINT").is_ok() {
            println!("{v}");
        }
        rows.push(v);
    }
    let body = json!({
        "function": format!("{RESOLVE:#x}"),
        "apply": format!("{APPLY:#x}"),
        "attacker": format!("{ATTACKER:#x}"),
        "flick_defence": format!("{FLICK_DEF:#x}"),
        "cases": rows,
    });
    std::fs::write(out.join("guard_resolution.json"), serde_json::to_string_pretty(&body).unwrap()).unwrap();
    println!("guard_resolution: {} cases", cases.len());
}

/// The hit record from an attack (FUN_1410b6880): info [0] owner chr, +8 AtkParam id, +0xc type
/// (0 Npc, 1 Pc), +0x12c weapon, +0x134 -1.
pub(crate) fn build_hit(emu: &mut Emu, owner: u64, id: u32, pc: bool, weapon: i32) -> (u64, Run) {
    let info = emu.alloc(0x200);
    emu.write(info, &vec![0u8; 0x200]);
    emu.u64(info, owner);
    emu.u32(info + 8, id);
    emu.u32(info + 0xc, pc as u32);
    emu.u32(info + 0x12c, weapon as u32);
    emu.u32(info + 0x134, u32::MAX);
    let ids = emu.alloc(0x40);
    emu.write(ids, &[0xffu8; 0x40]);
    let hit = emu.alloc(0x300);
    emu.write(hit, &vec![0u8; 0x300]);
    let r = emu.call(0x1_410b_6880, &[hit, 0, ids, info, 0, 0, 0], &[]);
    (hit, r)
}

/// An in-memory PARAM file (header +0xa row count, entries at +0x40 of 0x18: id, data offset),
/// found from one row pointer of a known id.
pub(crate) struct ParamFile {
    base: u64,
    count: u64,
}

impl ParamFile {
    pub(crate) fn from_row(emu: &mut Emu, row: u64, id: u32) -> ParamFile {
        let mut x = row & !7;
        for _ in 0..(8 << 20) / 8 {
            x -= 8;
            let Some(v) = try_u64(emu, x) else { break };
            if v as u32 != id {
                continue;
            }
            let Some(off) = try_u64(emu, x + 8) else { continue };
            let base = row.wrapping_sub(off);
            if base > x || (x - base) < 0x40 || (x - base - 0x40) % 0x18 != 0 {
                continue;
            }
            let count = u16::from_le_bytes(emu.peek(base + 0xa)) as u64;
            if (x - base - 0x40) / 0x18 < count {
                return ParamFile { base, count };
            }
        }
        panic!("param file of row {row:#x} not found");
    }
    pub(crate) fn row(&mut self, emu: &mut Emu, id: u32) -> u64 {
        let (mut lo, mut hi) = (0, self.count);
        while lo < hi {
            let mid = (lo + hi) / 2;
            let e = self.base + 0x40 + 0x18 * mid;
            let i = i32::from_le_bytes(emu.peek(e));
            if i == id as i32 {
                return self.base + emu.peek_u64(e + 8);
            }
            if i < id as i32 { lo = mid + 1 } else { hi = mid }
        }
        for i in 0..self.count {
            let e = self.base + 0x40 + 0x18 * i;
            if i32::from_le_bytes(emu.peek(e)) == id as i32 {
                return self.base + emu.peek_u64(e + 8);
            }
        }
        let first = i32::from_le_bytes(emu.peek(self.base + 0x40));
        let last = i32::from_le_bytes(emu.peek(self.base + 0x40 + 0x18 * (self.count - 1)));
        panic!("SpEffectParam {id} not found ({} rows, {first}..{last})", self.count);
    }
}

/// Puts an active SpEffect at the head of a chr's list (node +0x1c flags 0 = active, +0x58 id,
/// +0x60 row, +0x70 next), as its TAE event would have.
pub(crate) fn add_sp(emu: &mut Emu, sp: &mut ParamFile, chr: u64, id: u32) {
    let list = emu.peek_u64(chr + 0x11d0);
    let head = emu.peek_u64(list + 8);
    let node = emu.alloc(0x80);
    emu.write(node, &[0u8; 0x80]);
    emu.u32(node + 0x58, id);
    let row = sp.row(emu, id);
    emu.u64(node + 0x60, row);
    emu.u64(node + 0x70, head);
    emu.raw_write(list + 8, &node.to_le_bytes());
}

/// Switches off a chr's SpEffect (flag bit 0: the lists skip it).
pub(crate) fn drop_sp(emu: &mut Emu, chr: u64, id: u32) {
    let list = emu.peek_u64(chr + 0x11d0);
    let mut n = emu.peek_u64(list + 8);
    while n != 0 {
        if u32::from_le_bytes(emu.peek(n + 0x58)) == id {
            let f = u32::from_le_bytes(emu.peek(n + 0x1c));
            emu.raw_write(n + 0x1c, &(f | 1).to_le_bytes());
        }
        n = emu.peek_u64(n + 0x70);
    }
}

const RESOLVE: u64 = 0x1_40b6_c880;
const APPLY: u64 = 0x1_40b6_e6a0;
const ATTACKER: u64 = 0x1_40b6_98d0;
const FLICK_DEF: u64 = 0x1_4084_2fb0;

#[derive(Clone, Copy)]
struct Case {
    /// Wolf defends against the General (c1020), else the General against Wolf.
    wolf_defends: bool,
    atk: u32,
    guard: bool,
    /// Degrees between the attack's direction and straight into the defender's face.
    angle: f32,
    /// The defender wears its deflect SpEffect (Wolf 105010, NPC 200220).
    window: bool,
    /// Wolf's spam-chain SpEffect 105020 + n - 1 (0 none).
    spam: u32,
    /// Defender posture (remaining, +0x148) before the hit; None = as in the snapshot.
    posture: Option<i32>,
    /// Defender SpEffects added (ids).
    extra: &'static [u32],
    /// A guard behavior row in place of the one vfunc 0x1c0 picks from the judge (the umbrella's
    /// BehaviorParam_PC 107600100, guardAttribute 1, is not equipped in the snapshot).
    guard_row: Option<u32>,
}

fn one_case(new_emu: &dyn Fn() -> Emu, c: &Case) -> Value {
    let mut emu = new_emu();
    let emu = &mut emu;
    let wolf = emu.st.snap.as_ref().unwrap().player;
    let general = enemies(emu).into_iter().find(|&e| u32::from_le_bytes(emu.peek(e + 0x6c)) == 1020).unwrap();
    // The SpEffectParam file, from Wolf's first SpEffect node.
    let list = emu.peek_u64(wolf + 0x11d0);
    let node = emu.peek_u64(list + 8);
    let (nid, nrow) = (u32::from_le_bytes(emu.peek(node + 0x58)), emu.peek_u64(node + 0x60));
    let mut sp = ParamFile::from_row(emu, nrow, nid);
    // A level-1 Wolf: no attack-power growth (160099 "Core Level 99", defStaminaAttackRate 7.23)
    // and no latent skills (150410 deflect posture x1.25, 150420 / 150421 guard cuts).
    for id in [160099, 150410, 150420, 150421] {
        drop_sp(emu, wolf, id);
    }
    let (att, def) = if c.wolf_defends { (general, wolf) } else { (wolf, general) };
    let (hit, b) = build_hit(emu, att, c.atk, !c.wolf_defends, if c.wolf_defends { -1 } else { 7400 });
    assert!(b.stopped.is_none());
    // HP damage > 0 so the reactions run (the builder leaves it to the attacker's stats).
    emu.f32(hit, 100.0);
    // The posture value's multiplier (+0x188, FUN_1408439a0), set where the hit lands.
    emu.f32(hit + 0x188, 1.0);
    // Attack direction (+0x150 / +0x158): into the defender's face, or into its back.
    let fwd = emu.alloc(0x10);
    emu.call(0x1_409e_f970, &[def, fwd], &[]);
    let (fx, fz) = (emu.rf32(fwd), emu.rf32(fwd + 8));
    let (sn, cs) = c.angle.to_radians().sin_cos();
    emu.f32(hit + 0x150, -(fx * cs - fz * sn));
    emu.f32(hit + 0x158, -(fx * sn + fz * cs));
    emu.u32(hit + 0x160, u32::MAX);
    emu.u32(hit + 0x1d4, u32::MAX);
    // Guard: TAE ChrActionFlag 3 ShieldBlock = module0 +0x70 bit 2 and +0xb08 its behavior judge.
    let m0 = module(emu, def, 0);
    let flags = emu.peek_u64(m0 + 0x70);
    let flags = if c.guard { flags | 4 } else { flags & !4 };
    emu.raw_write(m0 + 0x70, &flags.to_le_bytes());
    let judge: u32 = if c.wolf_defends { 90 } else { 900 };
    if let Some(row) = c.guard_row {
        // A copy of the defender's vtable whose slot 0x1c0 is `mov eax, row; ret`.
        let vt = emu.peek_u64(def);
        let copy = emu.alloc(0x800);
        let bytes: Vec<u8> = (0..0x800u64 / 8).flat_map(|i| emu.peek_u64(vt + 8 * i).to_le_bytes()).collect();
        emu.write(copy, &bytes);
        let stub = emu.alloc(0x10);
        let mut code = vec![0xb8u8];
        code.extend(row.to_le_bytes());
        code.push(0xc3);
        emu.write(stub, &code);
        emu.u64(copy + 0x1c0, stub);
        emu.raw_write(def, &copy.to_le_bytes());
    }
    emu.raw_write(m0 + 0xb08, &(if c.guard { judge } else { u32::MAX }).to_le_bytes());
    if c.window {
        add_sp(emu, &mut sp, def, if c.wolf_defends { 105010 } else { 200220 });
    }
    if c.spam > 0 {
        add_sp(emu, &mut sp, def, 105019 + c.spam);
    }
    for &id in c.extra {
        add_sp(emu, &mut sp, def, id);
    }
    let ddata = module(emu, def, 0x18);
    let adata = module(emu, att, 0x18);
    if let Some(p) = c.posture {
        emu.raw_write(ddata + 0x148, &p.to_le_bytes());
    }
    let i32at = |emu: &mut Emu, a: u64| i32::from_le_bytes(emu.peek(a));
    let def_posture = i32at(emu, ddata + 0x148);
    let def_max = i32at(emu, ddata + 0x14c);
    let def_hp = i32at(emu, ddata + 0x130);
    let att_posture = i32at(emu, adata + 0x148);
    let att_max = i32at(emu, adata + 0x14c);
    // The defender's flick (deflect) power while guarding: FUN_140842fb0(chr, 1, guard row, -1).
    let f348 = vfunc(emu, def, 0x348);
    let w = emu.call(f348, &[def], &[]).rax & 0xffff_ffff;
    let f1c0 = vfunc(emu, def, 0x1c0);
    let row = emu.call(f1c0, &[def, judge as u64, w, 0], &[]).rax & 0xffff_ffff;
    let flick_def = emu.call(FLICK_DEF, &[def, 1, row, 0xffff_ffff], &[]).rax as u32 as i32;
    let flick_atk = i32at(emu, hit + 0x40);
    let dmod = module(emu, def, 0x98);
    let amod = module(emu, att, 0x98);
    let before: Vec<u32> = (0..0xc0u64).map(|i| u32::from_le_bytes(emu.read(hit + 4 * i))).collect();
    let r1 = emu.call(RESOLVE, &[dmod, att, hit], &[]);
    if std::env::var("GUARD_DIFF").is_ok() {
        let d: Vec<String> = (0..0xc0u64)
            .filter_map(|i| {
                let v = u32::from_le_bytes(emu.read(hit + 4 * i));
                (v != before[i as usize]).then(|| format!("{:#x}={}/{:.3}", 4 * i, v as i32, f32::from_bits(v)))
            })
            .collect();
        println!("  resolve changed: {}", d.join(" "));
    }
    let u8at = |emu: &Emu, o: u64| emu.ru8(hit + o);
    let guarded = u8at(emu, 0x164);
    let deflected = u8at(emu, 0x1c7);
    let f231 = u8at(emu, 0x231);
    let knockback = emu.rf32(hit + 0x68);
    let posture_dmg = i32::from_le_bytes(emu.read(hit + 0x1e4));
    let r2 = emu.call(APPLY, &[dmod, att, hit, 0, 0], &[]);
    let def_posture_after = i32::from_le_bytes(emu.read(ddata + 0x148));
    let r3 = emu.call(ATTACKER, &[amod, def, hit], &[]);
    let att_posture_after = i32::from_le_bytes(emu.read(adata + 0x148));
    let am0 = module(emu, att, 0);
    let reaction = i32::from_le_bytes(emu.read(am0 + 0x5c));
    for (n, r) in [("resolve", &r1), ("apply", &r2), ("attacker", &r3)] {
        if r.stopped.is_some() {
            println!("  {n} stopped {:?} trail {:x?}", r.stopped, emu.st.trail);
        }
    }
    let v = json!({
        "wolf_defends": c.wolf_defends, "atk": c.atk, "guard": c.guard, "angle": c.angle, "judge": judge, "window": c.window,
        "spam": c.spam, "extra": c.extra, "flick_atk": flick_atk, "flick_def": flick_def, "guard_row": row,
        "guarded": guarded, "deflected": deflected, "flags_231": f231,
        "guard_break": u8at(emu, 0x1c4), "guard_break_hit": u8at(emu, 0x1c5), "broken": u8at(emu, 0x1c6),
        "knockback": knockback, "posture_values": [emu.rf32(hit + 0x34), emu.rf32(hit + 0x38), emu.rf32(hit + 0x3c)],
        "def_hp": def_hp, "def_posture": def_posture, "def_posture_max": def_max, "def_posture_dmg": posture_dmg,
        "def_posture_after": def_posture_after, "att_posture": att_posture, "att_posture_max": att_max,
        "att_posture_after": att_posture_after, "att_reaction": reaction,
        "ran": [r1.stopped.is_none(), r2.stopped.is_none(), r3.stopped.is_none()],
    });
    v
}

fn build_probe(emu: &mut Emu) {
    let wolf = emu.st.snap.as_ref().unwrap().player;
    let general = enemies(emu).into_iter().find(|&e| u32::from_le_bytes(emu.peek(e + 0x6c)) == 1020).unwrap();
    for (owner, id, pc, w) in [(general, 10200100u32, false, -1), (wolf, 5000010, true, 7400)] {
        let (hit, r) = build_hit(emu, owner, id, pc, w);
        println!("build {id}: rax {:#x} stopped {:?} calls {:?}", r.rax, r.stopped, r.calls);
        let words: Vec<String> = (0..0x2a0u64 / 4)
            .filter_map(|i| {
                let b: [u8; 4] = emu.read(hit + 4 * i);
                let v = u32::from_le_bytes(b);
                (v != 0).then(|| format!("{:#x}={}/{:.3}", 4 * i, v as i32, f32::from_bits(v)))
            })
            .collect();
        println!("  {}", words.join(" "));
    }
}

pub fn explore(emu: &mut Emu, _out: &Path) {
    let wolf = emu.st.snap.as_ref().expect("guard needs --snapshot").player;
    let es = enemies(emu);
    let wp = pos(emu, wolf);
    println!("wolf {wolf:#x} at {wp:?}, {} enemies", es.len());
    let mut near: Vec<(f32, u64)> = es.iter().map(|&e| {
        let p = pos(emu, e);
        (((p[0] - wp[0]).powi(2) + (p[2] - wp[2]).powi(2)).sqrt(), e)
    }).collect();
    near.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    for &(d, e) in near.iter().take(6) {
        let id = u32::from_le_bytes(emu.peek(e + 0x68));
        let npc = u32::from_le_bytes(emu.peek(e + 0x6c));
        println!("  enemy {e:#x} dist {d:.2} +0x68 {id} +0x6c {npc}");
    }
    let enemy = near[0].1;
    for (name, chr) in [("wolf", wolf), ("enemy", enemy)] {
        let bag = emu.peek_u64(chr + 0x1ff8);
        let m0 = emu.peek_u64(bag);
        let f70 = emu.peek_u64(m0 + 0x70);
        let judge = u32::from_le_bytes(emu.peek(m0 + 0xb08));
        let c0c = emu.peek::<1>(m0 + 0xc0c)[0];
        let list = emu.peek_u64(chr + 0x11d0);
        let l20 = u32::from_le_bytes(emu.peek(list + 0x20));
        println!("{name} {chr:#x}: module0 {m0:#x} +0x70 {f70:#x} +0xb08 {judge} +0xc0c {c0c} splist +0x20 {l20:#x}");
        let sps = sp_effects(emu, chr);
        println!("  speffects {:?}", sps.iter().map(|s| (s.0, format!("{:#x}", s.1))).collect::<Vec<_>>());
        // Guard row: vfunc 0x1c0(chr, judge, vfunc 0x348(chr), 0).
        let f348 = vfunc(emu, chr, 0x348);
        let w = emu.call(f348, &[chr], &[]).rax & 0xffff_ffff;
        let f1c0 = vfunc(emu, chr, 0x1c0);
        let r = emu.call(f1c0, &[chr, judge as u64, w, 0], &[]);
        let row = r.rax & 0xffff_ffff;
        println!("  vfunc 0x348 -> {w}, guard row (0x1c0) -> {row} stopped {:?}", r.stopped);
        let judge2 = if chr == wolf { 90 } else { 900 };
        let r = emu.call(f1c0, &[chr, judge2, w, 0], &[]);
        let row = r.rax & 0xffff_ffff;
        println!("  judge {judge2}: guard row -> {row}");
        let f440 = vfunc(emu, chr, 0x440);
        let pen = emu.call(f440, &[chr, w], &[]).rax & 0xff;
        println!("  vfunc 0x440 (penalty) -> {pen}");
        let f338 = vfunc(emu, chr, 0x338);
        let wid = emu.call(f338, &[chr, w], &[]).rax as u32 as i32;
        let f278 = vfunc(emu, chr, 0x278);
        let r278 = emu.call(f278, &[chr, row, 0xffff_ffff, 1], &[]);
        println!("  vfunc 0x338 (weapon) -> {wid}; vfunc 0x278 guard flick -> {} calls {:?}", r278.rax as u32 as i32, r278.calls);
        for g in [0u64, 1] {
            let r = emu.call(0x1_4084_2fb0, &[chr, g, row, 0xffff_ffff], &[]);
            println!("  flick defence guarded={g}: {} stopped {:?}", r.rax as u32 as i32, r.stopped);
        }
        if chr == wolf {
            let f1b8 = vfunc(emu, chr, 0x1b8);
            let gd = emu.call(f1b8, &[chr], &[]).rax;
            let stats: Vec<i32> = (0..4).map(|i| i32::from_le_bytes(emu.peek(gd + 0x260 + 4 * i))).collect();
            println!("  game data {gd:#x} +0x260.. {stats:?}");
        }
    }
    // Attack flick power (FUN_14084c650) of NPC attacks, owner = the General, and without owner.
    let general = near.iter().map(|n| n.1).find(|&e| u32::from_le_bytes(emu.peek(e + 0x6c)) == 1020).unwrap();
    for id in [10200000u64, 10200100, 10200150, 10200140, 10200900, 70200330, 17] {
        for owner in [general, 0] {
            let info = emu.alloc(0x200);
            emu.write(info, &vec![0u8; 0x200]);
            emu.u64(info, owner);
            emu.u32(info + 8, id as u32);
            emu.u32(info + 0xc, 0);
            let r = emu.call(0x1_4084_c650, &[info], &[]);
            println!("  atk flick npc {id} owner {owner:#x}: {} stopped {:?}", r.rax as u32 as i32, r.stopped);
        }
    }
}
