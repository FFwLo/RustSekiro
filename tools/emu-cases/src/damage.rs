//! HP and posture amounts on the snapshot: posture regeneration each frame (FUN_140a04850), the
//! posture set with its StaminaControlParam clamp (FUN_140bd6710 -> FUN_140bd4e20), the hit's
//! posture damage (FUN_1408439a0 guarded, FUN_140844070 unguarded) and the HP curve
//! (FUN_140840ce0 / FUN_140840eb0). Writes extracted/emu/damage_*.json for
//! src/sim_tests/emu_damage.rs.

use super::*;

const WORLD_CHR_MAN: u64 = 0x1_43d7_a1e0;
const ENEMY_VT: u64 = 0x1_42a2_7f28;
const PLAYER_VT: u64 = 0x1_42a2_b338;

/// The damage pilots by name: damage_survey, damage_regen, damage_clamp, damage_posture, damage_hp.
pub(crate) fn run(new_emu: &dyn Fn() -> Emu, out: &Path, want: &[&str]) {
    let all = want.contains(&"damage");
    let on = |n: &str| all || want.contains(&n);
    if on("damage_survey") {
        survey(&mut new_emu(), out);
    }
    if on("damage_regen") {
        regen(new_emu, out);
    }
    if on("damage_hp") {
        hp(&mut new_emu(), out);
    }
    if on("damage_posture") {
        posture(new_emu, out);
    }
}

/// Reads 8 bytes of the game's memory, or None when the snapshot has nothing there.
fn try_u64(emu: &mut Emu, at: u64) -> Option<u64> {
    if !(0x10000..0x7fff_ffff_0000).contains(&at) || at % 8 != 0 {
        return None;
    }
    let mut b = [0u8; 8];
    if unsafe { uc::uc_mem_read(emu.uc, at, b.as_mut_ptr() as _, 8) } == 0 {
        return Some(u64::from_le_bytes(b));
    }
    let (from, bytes) = emu.st.snap.as_mut()?.window(at)?;
    if unsafe { uc::uc_mem_map(emu.uc, from, bytes.len(), uc::PROT_ALL) } != 0 {
        return None;
    }
    emu.raw_write(from, &bytes);
    (unsafe { uc::uc_mem_read(emu.uc, at, b.as_mut_ptr() as _, 8) } == 0).then(|| u64::from_le_bytes(b))
}

fn pu32(emu: &mut Emu, at: u64) -> u32 {
    u32::from_le_bytes(emu.peek(at))
}
fn pf32(emu: &mut Emu, at: u64) -> f32 {
    f32::from_le_bytes(emu.peek(at))
}
fn put_u32(emu: &mut Emu, at: u64, v: u32) {
    emu.peek::<4>(at);
    emu.raw_write(at, &v.to_le_bytes());
}
fn put_f32(emu: &mut Emu, at: u64, v: f32) {
    emu.peek::<4>(at);
    emu.raw_write(at, &v.to_le_bytes());
}

/// Wolf, then every EnemyIns held in a container that WorldChrMan points at (rec2.lua's scan).
pub(crate) fn chrs(emu: &mut Emu) -> Vec<u64> {
    let player = emu.st.snap.as_ref().expect("damage pilots need --snapshot").player;
    let mut out = vec![player];
    let wcm = emu.peek_u64(WORLD_CHR_MAN);
    for off in (0..0x2000).step_by(8) {
        let Some(p) = try_u64(emu, wcm + off) else { continue };
        for o2 in (0..0x4000).step_by(8) {
            let Some(q) = try_u64(emu, p + o2) else { continue };
            if !out.contains(&q) && try_u64(emu, q) == Some(ENEMY_VT) {
                out.push(q);
            }
        }
    }
    out
}

/// A character's live SpEffect entries ([chr+0x11d0]+8 list, entry +0x58 id, +0x60 param row,
/// +0x70 next, +0x1c flags; FUN_140bfe360 skips flags & 0x800c0003).
fn sp_entries(emu: &mut Emu, chr: u64) -> Vec<u64> {
    let list = emu.peek_u64(chr + 0x11d0);
    let mut out = Vec::new();
    if list == 0 {
        return out;
    }
    let mut e = emu.peek_u64(list + 8);
    while e != 0 && out.len() < 200 {
        out.push(e);
        e = emu.peek_u64(e + 0x70);
    }
    out
}

fn sp_effects(emu: &mut Emu, chr: u64) -> Vec<i64> {
    sp_entries(emu, chr).into_iter().map(|e| pu32(emu, e + 0x58) as i32 as i64).collect::<Vec<_>>()
}

/// Per entry: id, flags, FUN_140c05740 (the regen speed rate with the clear-count correction),
/// row +0x424 (staminaRecoverSpeedRate), +0xb8 (staminaRecoverChangeSpeed, i32), +0x168 bit 6.
fn sp_regen_rows(emu: &mut Emu, chr: u64) -> Vec<Value> {
    let mut out = Vec::new();
    for e in sp_entries(emu, chr) {
        let row = emu.peek_u64(e + 0x60);
        let speed = emu.call(0x1_40c0_5740, &[e], &[]).xmm0;
        out.push(json!({
            "id": pu32(emu, e + 0x58) as i32, "flags": pu32(emu, e + 0x1c), "speed": speed,
            "row_speed": if row == 0 { 1.0 } else { pf32(emu, row + 0x424) },
            "change": if row == 0 { 0 } else { pu32(emu, row + 0xb8) as i32 },
            "clear_count_corrected": row != 0 && (emu.peek::<1>(row + 0x168)[0] >> 6) & 1 == 1,
            // FUN_140c041a0's HP window: row +4 (conditionHp %, < 0 none) and +0x1fc (its floor).
            "cond_hp": if row == 0 { -1.0 } else { pf32(emu, row + 4) },
            "cond_hp_floor": if row == 0 { -1.0 } else { pf32(emu, row + 0x1fc) },
        }));
    }
    out
}

/// The data module ([chr+0x1ff8]+0x18): HP +0x130 / +0x134, posture +0x148 / +0x14c.
fn data_module(emu: &mut Emu, chr: u64) -> u64 {
    let bag = emu.peek_u64(chr + 0x1ff8);
    emu.peek_u64(bag + 0x18)
}

/// The action module whose +0x14 byte is TAE 225's regen percent and +0x88 bit 21 stops regen:
/// [[chr+0x50]+0x50].
fn action_module(emu: &mut Emu, chr: u64) -> u64 {
    let ctrl = emu.peek_u64(chr + 0x50);
    emu.peek_u64(ctrl + 0x50)
}

/// The StaminaControlParam type of TAE 960: [[chr+0x1ff8]]+0xb0c (-1 none).
fn type_slot(emu: &mut Emu, chr: u64) -> u64 {
    let bag = emu.peek_u64(chr + 0x1ff8);
    emu.peek_u64(bag) + 0xb0c
}

fn vcall(emu: &mut Emu, chr: u64, slot: u64, args: &[u64], xmm: &[f32]) -> Run {
    let vt = emu.peek_u64(chr);
    let f = emu.peek_u64(vt + slot);
    let mut a = vec![chr];
    a.extend_from_slice(args);
    emu.call(f, &a, xmm)
}

/// A posture regen scenario: the character's posture `deficit` below full, TAE 960's type, TAE
/// 225's percent, the action flag that stops regen, the frame step and the starting carry.
struct RegenScenario {
    deficit: i32,
    ty: i32,
    tae_pct: u8,
    no_regen: bool,
    dt: f32,
    carry: f32,
    frames: usize,
}

/// The characters the regen runs use (NpcParam ids of the Naomori-arena snapshot): Wolf (row 0,
/// 5221/5222 on at 49 % HP), 10100000 (row 1000100, 7510 x 7900), 10200000 (row 1000200, base
/// 30), 71003080 (row 5000000), 50101000 (base 1000) and 10109009 (no doping).
const REGEN_NPCS: [i32; 6] = [0, 10100000, 10200000, 71003080, 50101000, 10109009];

/// Posture regen frame by frame: FUN_140a04850 (the ChrIns update) on the snapshot's characters
/// with the inputs above; per frame the base (vfunc 0x218), the SpEffect product (FUN_140bfe360)
/// and the posture / carry before and after.
fn regen(new_emu: &dyn Fn() -> Emu, out: &Path) {
    const F: u64 = 0x1_40a0_4850;
    let mut scen = Vec::new();
    for ty in [-1, 0, 1, 2, 3, 4, 5, 6, 7, 8, 10] {
        scen.push(RegenScenario { deficit: 400, ty, tae_pct: 100, no_regen: false, dt: 1.0 / 60.0, carry: 0.0, frames: 40 });
    }
    for tae_pct in [0u8, 33, 50, 150] {
        scen.push(RegenScenario { deficit: 400, ty: -1, tae_pct, no_regen: false, dt: 1.0 / 60.0, carry: 0.0, frames: 40 });
    }
    scen.push(RegenScenario { deficit: 400, ty: -1, tae_pct: 100, no_regen: true, dt: 1.0 / 60.0, carry: 0.5, frames: 20 });
    scen.push(RegenScenario { deficit: 400, ty: 1, tae_pct: 33, no_regen: false, dt: 1.0 / 30.0, carry: 0.9, frames: 40 });
    scen.push(RegenScenario { deficit: 3, ty: -1, tae_pct: 100, no_regen: false, dt: 1.0 / 60.0, carry: 0.25, frames: 30 });
    scen.push(RegenScenario { deficit: 0, ty: -1, tae_pct: 100, no_regen: false, dt: 1.0 / 60.0, carry: 0.0, frames: 10 });
    scen.push(RegenScenario { deficit: 100000, ty: -1, tae_pct: 100, no_regen: false, dt: 0.0123, carry: 0.0, frames: 30 });
    scen.push(RegenScenario { deficit: 100000, ty: 6, tae_pct: 100, no_regen: false, dt: 1.0 / 60.0, carry: 0.0, frames: 5 });
    let mut chars = Vec::new();
    let mut total = 0;
    let list_emu = &mut new_emu();
    let all = chrs(list_emu);
    for npc in REGEN_NPCS {
        let Some(chr) = all.iter().copied().find(|&c| pu32(list_emu, c + 0x68) as i32 == npc) else {
            println!("damage_regen: NpcParam {npc} not in the snapshot");
            continue;
        };
        let mut runs = Vec::new();
        let mut emu = new_emu();
        for s in &scen {
            // A fresh snapshot per scenario: Unicorn crashed (host segfault) after ~300 frames of
            // the ChrIns update on one engine.
            emu = new_emu();
            let data = data_module(&mut emu, chr);
            let act = action_module(&mut emu, chr);
            let ty_at = type_slot(&mut emu, chr);
            let max = pu32(&mut emu, data + 0x14c) as i32;
            let timer = emu.alloc(0x10);
            let list = emu.peek_u64(chr + 0x11d0);
            let flags0 = pu32(&mut emu, act + 0x88);
            let mut frames = Vec::new();
            put_u32(&mut emu, data + 0x148, (max - s.deficit).max(0) as u32);
            put_f32(&mut emu, data + 0x15c, s.carry);
            put_u32(&mut emu, ty_at, s.ty as u32);
            emu.peek::<1>(act + 0x14);
            emu.raw_write(act + 0x14, &[s.tae_pct]);
            put_u32(&mut emu, act + 0x88, if s.no_regen { flags0 | 1 << 21 } else { flags0 & !(1 << 21) });
            emu.f32(timer + 8, s.dt);
            let mut stopped = None;
            for _ in 0..s.frames {
                let base = vcall(&mut emu, chr, 0x218, &[], &[]).xmm0;
                let speed = emu.call(0x1_40bf_e360, &[list], &[]).xmm0;
                let before = (pu32(&mut emu, data + 0x148) as i32, pf32(&mut emu, data + 0x15c));
                let state = (pu32(&mut emu, ty_at) as i32, emu.peek::<1>(act + 0x14)[0], (pu32(&mut emu, act + 0x88) >> 21) & 1, pf32(&mut emu, chr + 0x10d0));
                let r = emu.call(F, &[chr, timer], &[]);
                if r.stopped.is_some() {
                    stopped = r.stopped;
                    break;
                }
                frames.push(json!({
                    "base": base, "speed": speed, "level_mult": state.3,
                    "type": state.0, "tae_pct": state.1, "no_regen": state.2,
                    "posture": before.0, "carry": before.1,
                    "out_posture": pu32(&mut emu, data + 0x148) as i32, "out_carry": pf32(&mut emu, data + 0x15c),
                    "amount": pu32(&mut emu, data + 0x22c) as i32, "rate_int": pu32(&mut emu, data + 0x230) as i32,
                }));
                total += 1;
            }
            if let Some((at, e)) = stopped {
                println!("damage_regen: NpcParam {npc} type {} stopped at {at:#x} (error {e})", s.ty);
            }
            runs.push(json!({"deficit": s.deficit, "type": s.ty, "tae_pct": s.tae_pct, "no_regen": s.no_regen, "dt": s.dt, "carry": s.carry, "frames": frames}));
        }
        let data = data_module(&mut emu, chr);
        let max = pu32(&mut emu, data + 0x14c) as i32;
        let ctrl_row = emu.call(0x1_40bd_58c0, &[data], &[]).rax as u32;
        let timer = emu.alloc(0x10);
        // The StaminaControlParam row the clamp read (FUN_1410cf390 +4 / FUN_1410cf4b0 +6 per type).
        let mut acc = [0u32; 4];
        emu.u32(timer, 0xffff_ffff);
        let mut limits = Vec::new();
        let tmp = emu.alloc(0x10);
        emu.write(tmp, &[0xffu8; 16]);
        emu.call(0x1_410c_f6c0, &[tmp, ctrl_row as u64], &[]);
        let row = emu.peek_u64(tmp + 8);
        for t in 0..16u64 {
            if row == 0 {
                break;
            }
            acc[0] = pu32(&mut emu, row + 8 * t);
            let a = u16::from_le_bytes(emu.peek(row + 8 * t + 4));
            let b = u16::from_le_bytes(emu.peek(row + 8 * t + 6));
            limits.push(json!({"recover": acc[0], "limit_a": a, "limit_b": b}));
        }
        chars.push(json!({
            "npc_param": npc, "player": emu.peek_u64(chr) == PLAYER_VT, "posture_max": max, "ctrl_row": ctrl_row,
            "level": if emu.peek_u64(chr) == PLAYER_VT { let g = emu.peek_u64(chr + 0x2000); pu32(&mut emu, g + 0x248) as i32 } else { -1 },
            "row": limits, "sp_regen": sp_regen_rows(&mut emu, chr), "runs": runs,
        }));
    }
    println!("damage_regen: {} characters, {total} frames", chars.len());
    std::fs::write(out.join("damage_regen.json"), serde_json::to_string(&json!({"function": format!("{F:#x}"), "chars": chars})).unwrap()).unwrap();
}

/// One line per character: what the damage and regen functions read.
pub(crate) fn survey(emu: &mut Emu, out: &Path) {
    let list = chrs(emu);
    let mut rows = Vec::new();
    for &chr in &list {
        let data = data_module(emu, chr);
        let act = action_module(emu, chr);
        let ty = type_slot(emu, chr);
        let npc = vcall(emu, chr, 0x178, &[], &[]).rax;
        let npc_row = if npc == 0 { 0 } else { emu.peek_u64(npc + 8) };
        let base = vcall(emu, chr, 0x218, &[], &[]);
        let list_ptr = emu.peek_u64(chr + 0x11d0);
        let speed = emu.call(0x1_40bf_e360, &[list_ptr], &[]);
        let ctrl_row = emu.call(0x1_40bd_58c0, &[data], &[]).rax as u32;
        let row = json!({
            "chr": format!("{chr:#x}"),
            "player": emu.peek_u64(chr) == PLAYER_VT,
            "npc_param": pu32(emu, chr + 0x68) as i32,
            "hp": pu32(emu, data + 0x130) as i32, "hp_max": pu32(emu, data + 0x134) as i32,
            "posture": pu32(emu, data + 0x148) as i32, "posture_max": pu32(emu, data + 0x14c) as i32,
            "carry": pf32(emu, data + 0x15c),
            "rate_int": pu32(emu, data + 0x230) as i32,
            "tae_pct": emu.peek::<1>(act + 0x14)[0],
            "no_regen": (pu32(emu, act + 0x88) >> 21) & 1,
            "type": pu32(emu, ty) as i32,
            "level_mult": pf32(emu, chr + 0x10d0),
            "base": base.xmm0, "base_stopped": base.stopped.map(|s| format!("{:#x}", s.0)),
            "speed": speed.xmm0,
            "ctrl_row": ctrl_row,
            "npc_row_base_vel": if npc_row == 0 { -1 } else { u16::from_le_bytes(emu.peek(npc_row + 0xf2)) as i32 },
            "sp_effects": sp_effects(emu, chr),
            "sp_regen": sp_regen_rows(emu, chr).into_iter().filter(|r| r["speed"].as_f64() != Some(1.0) || r["change"].as_i64() != Some(0)).collect::<Vec<_>>(),
            "player_regen_pct": if emu.peek_u64(chr) == PLAYER_VT { pu32(emu, chr + 0x21f0) as i32 } else { 0 },
            "player_regen_pct_on": if emu.peek_u64(chr) == PLAYER_VT { let bag = emu.peek_u64(chr + 0x1ff8); let m = emu.peek_u64(bag + 0xc0); if m == 0 { 0 } else { emu.peek::<1>(m + 0x10)[0] } } else { 0 },
            "level": if emu.peek_u64(chr) == PLAYER_VT { let g = emu.peek_u64(chr + 0x2000); if g == 0 { -1 } else { pu32(emu, g + 0x248) as i32 } } else { -1 },
        });
        rows.push(row);
    }
    println!("damage_survey: {} characters", rows.len());
    for r in rows.iter().take(12) {
        println!("  {r}");
    }
    std::fs::write(out.join("damage_survey.json"), serde_json::to_string_pretty(&json!({"chrs": rows})).unwrap()).unwrap();
}

/// The defenders the posture runs use: Wolf, 10200000 (c1020 General), 10100000 (c1010),
/// 11800000 (c1180), 50101000 and 10109009 (no doping).
const POSTURE_NPCS: [i32; 6] = [0, 10200000, 10100000, 11800000, 50101000, 10109009];

/// A hit's posture damage on a defender: FUN_1408439a0 (guarded: block value +0x34 or deflect
/// value +0x38 by the block flag) and FUN_140844070 (unguarded: +0x3c), on a damage record with
/// only the fields they read set (+0x2c attribute, +0x188 rate, +0x164, +0x1d0 guard behaviour,
/// +0xd9, +0x231), attacker none or Wolf.
fn posture(new_emu: &dyn Fn() -> Emu, out: &Path) {
    const GUARDED: u64 = 0x1_4084_39a0;
    const UNGUARDED: u64 = 0x1_4084_4070;
    let first = &mut new_emu();
    let all = chrs(first);
    let wolf = all[0];
    let mut defenders = Vec::new();
    let mut n = 0;
    for npc in POSTURE_NPCS {
        let Some(chr) = all.iter().copied().find(|&c| pu32(first, c + 0x68) as i32 == npc) else { continue };
        let mut cases = Vec::new();
        let mut sp = Vec::new();
        for attr in 0..13u32 {
            // A fresh engine every few hundred calls: Unicorn crashed (host segfault) after about
            // a thousand calls on one engine.
            let emu = &mut new_emu();
            let dmg = emu.alloc(0x300);
            let atk = emu.alloc(0x40);
            let wep = emu.alloc(0x20);
            sp = sp_effects(emu, chr);
            // The defender's attribute rate (vfunc 0x248) alone.
            let attr_rate = vcall(emu, chr, 0x248, &[attr as u64], &[]).xmm0;
            for value in [12.0f32, 21.0, 27.0, 60.0, 100.5] {
                for rate in [1.0f32, 0.6] {
                    for flag164 in [0u8, 1] {
                        for guard_id in [-1i32, 105000090] {
                            for attacker in [0u64, wolf] {
                                if attacker == chr || (guard_id != -1 && npc != 0) {
                                    continue;
                                }
                                emu.write(dmg, &[0u8; 0x300]);
                                emu.u32(dmg + 0x2c, attr);
                                emu.f32(dmg + 0x34, value + 1.0);
                                emu.f32(dmg + 0x38, value);
                                emu.f32(dmg + 0x3c, value + 2.0);
                                emu.f32(dmg + 0x188, rate);
                                emu.u8(dmg + 0x164, flag164);
                                emu.u32(dmg + 0x1d0, guard_id as u32);
                                emu.u32(dmg + 0x1d4, 0xffff_ffff);
                                emu.write(atk, &[0u8; 0x40]);
                                emu.u32(wep, 0xffff_ffff);
                                emu.write(wep + 4, &[0u8; 12]);
                                let mut row = json!({"attr": attr, "value": value, "rate": rate, "flag164": flag164, "guard_id": guard_id, "attacker": attacker != 0});
                                for block in [0u64, 1] {
                                    let r = emu.call(GUARDED, &[dmg, atk, wep, block, chr, attacker], &[]);
                                    row[if block == 1 { "block" } else { "deflect" }] = if r.stopped.is_some() { Value::Null } else { json!(r.rax as u32 as i32) };
                                }
                                if guard_id == -1 {
                                    let r = emu.call(UNGUARDED, &[dmg, atk, chr, attacker], &[]);
                                    row["hit"] = if r.stopped.is_some() { Value::Null } else { json!(r.rax as u32 as i32) };
                                }
                                row["attr_rate"] = json!(attr_rate);
                                cases.push(row);
                                n += 1;
                            }
                        }
                    }
                }
            }
        }
        defenders.push(json!({"npc_param": npc, "player": chr == wolf, "sp_effects": sp, "cases": cases}));
    }
    println!("damage_posture: {} defenders, {n} cases", defenders.len());
    std::fs::write(out.join("damage_posture.json"), serde_json::to_string(&json!({"defenders": defenders})).unwrap()).unwrap();
}

/// The HP side: the attack / defence curve FUN_140840ce0 over a grid, the five-element sum
/// FUN_140840eb0 over chosen arrays and flags, the globals they read, and every character's
/// defence (vfunc 0x228: FUN_140a24350 player, FUN_140a12ab0 -> FUN_1408417a0 NPC).
fn hp(emu: &mut Emu, out: &Path) {
    const CURVE: u64 = 0x1_4084_0ce0;
    const SUM: u64 = 0x1_4084_0eb0;
    let globals: Vec<Value> = [0x1_43d5_c100u64, 0x1_43d5_c104, 0x1_43d5_c108, 0x1_43d5_c10c, 0x1_43d5_c110, 0x1_43d5_c114, 0x1_43b0_1918, 0x1_43b0_191c, 0x1_43b0_1920, 0x1_43b0_1924, 0x1_43b0_18d4, 0x1_43b0_18d8, 0x1_43b0_18dc, 0x1_43b0_18f0, 0x1_43b0_95c8]
        .iter()
        .map(|&a| json!({"at": format!("{a:#x}"), "f32": pf32(emu, a)}))
        .collect();
    let mut curve = Vec::new();
    for atk in [0.0f32, 0.4, 0.99, 1.0, 1.5, 10.0, 32.0, 48.0, 96.0, 128.0, 360.0, 1000.0, 5000.0, -5.0] {
        for def in [0.0f32, 0.5, 1.0, 10.0, 50.0, 96.0, 100.0, 300.0, 1000.0, 20000.0] {
            let r = emu.call(CURVE, &[], &[atk, def]);
            assert!(r.stopped.is_none());
            curve.push(json!({"atk": atk, "def": def, "out": r.xmm0}));
        }
    }
    // FUN_140840eb0(out1, out2, atk[5], def[5], rates p5..p11 [5] each, p12, flags p13-15, p16, p17).
    let arrays = emu.alloc(0x400);
    let outs = emu.alloc(0x10);
    let mut sums = Vec::new();
    let atks: [[f32; 5]; 6] = [[360.0, 0.0, 0.0, 0.0, 0.0], [96.0, 20.0, 30.0, 0.0, 40.0], [0.3, 0.0, 0.0, 0.0, 0.0], [0.3, 0.2, 0.0, 0.0, 0.1], [120.5, 0.0, 0.0, 0.0, 0.0], [-10.0, 5.0, 0.0, 0.0, 0.0]];
    let rates: [[f32; 5]; 4] = [[1.0; 5], [0.5, 0.6, 1.0, 1.0, 0.25], [0.35, 1.0, 1.0, 1.0, 1.0], [1.5, 1.5, 1.5, 1.5, 1.5]];
    for atk in atks {
        for (ri, rate) in rates.iter().enumerate() {
            for which in 0..7usize {
                for (p13, p14, p15, p16, p17) in [(0u8, 0u8, 0u8, 1.0f32, 1.0f32), (1, 0, 0, 1.0, 1.0), (0, 1, 0, 0.5, 1.0), (0, 1, 0, -1.0, 1.0), (0, 0, 1, -1.0, 0.8), (0, 0, 0, 1.0, 0.8)] {
                    if (ri > 0 && which > 0 && p13 + p14 + p15 > 0) || (which > 0 && p17 != 1.0) {
                        continue;
                    }
                    let at = |i: u64| arrays + 0x20 * i;
                    for (i, v) in atk.iter().enumerate() {
                        emu.f32(at(0) + 4 * i as u64, *v);
                        emu.f32(at(1) + 4 * i as u64, 0.0);
                    }
                    for k in 0..7u64 {
                        for i in 0..5u64 {
                            emu.f32(at(2 + k) + 4 * i, if k as usize == which { rate[i as usize] } else { 1.0 });
                        }
                    }
                    emu.write(outs, &[0u8; 16]);
                    let mut args = vec![outs, outs + 4, at(0), at(1)];
                    args.extend((0..7).map(|k| at(2 + k)));
                    args.extend([0u64, p13 as u64, p14 as u64, p15 as u64, p16.to_bits() as u64, p17.to_bits() as u64]);
                    let r = emu.call(SUM, &args, &[]);
                    assert!(r.stopped.is_none());
                    sums.push(json!({"atk": atk, "rate": rate, "slot": which, "p13": p13, "p14": p14, "p15": p15, "p16": p16, "p17": p17, "out": r.xmm0, "out1": emu.rf32(outs), "out2": emu.rf32(outs + 4)}));
                }
            }
        }
    }
    let mut defs = Vec::new();
    let buf = emu.alloc(0x40);
    for chr in chrs(emu) {
        emu.write(buf, &[0u8; 0x20]);
        let r = vcall(emu, chr, 0x228, &[buf, 0, 0, 0, 0], &[]);
        let d: Vec<f32> = (0..5).map(|i| emu.rf32(buf + 4 * i)).collect();
        defs.push(json!({"npc_param": pu32(emu, chr + 0x68) as i32, "player": emu.peek_u64(chr) == PLAYER_VT, "def": d, "stopped": r.stopped.is_some()}));
    }
    println!("damage_hp: {} curve points, {} sums, {} defences", curve.len(), sums.len(), defs.len());
    let body = json!({"globals": globals, "curve": curve, "sums": sums, "defence": defs});
    std::fs::write(out.join("damage_hp.json"), serde_json::to_string_pretty(&body).unwrap()).unwrap();
}
