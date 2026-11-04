//! `prosth`: Wolf's prosthetic tools, each equipped the way the game equips one and used at the
//! General in whole world frames (bullets.rs's frame: bullet step, hit step, WorldChrMan pre
//! tasks, the General's chr frame, Wolf's, post tasks), from the Naomori-arena snapshot.
//!
//! How the exe holds the equipped tool (found with `prosth_probe`):
//! - PlayerGameData [chr+0x2000] +0x278 is EquipGameData: +0x24 + 4 * slot the inventory index of
//!   each of its 25 equip slots, +0x98 (= pgd +0x310) its ChrAsm, +0x374 + 4 * slot the item ids
//!   (pgd +0x5ec ..: slot 0 / 2 / 4 the three prosthetic slots, 1 / 3 / 5 the right hand).
//!   The slot equip is FUN_14079f8a0(EquipGameData, slot, &handle, inventory index, ..)
//!   (FUN_1409ccdb0 calls it for all 25 slots). In the emulator it stops on a CPU exception, so
//!   the harness writes its three fields itself.
//! - ChrAsm: +0xc the active prosthetic slot (0 / 1 / 2 -> equip slot 0 / 2 / 4), +0x24 + 4 * slot
//!   the item handle (the inventory entry's +0). Every chr frame FUN_1409cc9a0 copies
//!   PlayerGameData's ChrAsm into the chr's own ([chr+0x2140]); the switch (ACTION_ARM
//!   CHANGE_WEAPON_L -> HKS W_AddSubWeaponChange; FUN_140bf1640 writes ChrAsm +0xc) changes the
//!   chr's, and FUN_1409ccdb0 copies it back to PlayerGameData (re-equipping every slot).
//! - So writing the id cache pgd +0x5ec alone changes nothing (the 2026-11-03 items gap): the
//!   handle is what counts. The chr's left weapon id then follows ([bag+0xb8] +0x370 the frame
//!   after the switch) and the HKS picks the tool's anim group (a0<group>_412000 expand).
//! - The parts model ([chr+0x2148] slot records +0x5d0 + 0xc0 * n, requested +0x274, shown
//!   +0x270; FUN_140e7b000 loads parts:/<model>.partsbnd) only completes for a tool whose parts
//!   are resident (one of the three saved tools); for the others the shown model stays the saved
//!   one (gap: dummies on the tool model are then the old tool's).
//!
//! Each case writes the tool's inventory handle into prosthetic slot 1 (equip slot 2), switches
//! to it in Wolf-only chr frames until the expand has played, sets Spirit Emblems (goods 1000)
//! to 20, places Wolf `dist` m in front of the General (AI off, updated every frame), then holds
//! ACTION_ARM_SUB_ATTACK for `hold` frames and runs `frames` whole world frames. Per frame:
//! Wolf's clips (id, time), TAE BulletBehavior FUN_140b51110 / AttackBehavior FUN_140b50ae0 /
//! PCBehavior FUN_140b55510 events of Wolf, the emitter FUN_1407d4fa0, every bullet spawn
//! FUN_140b1ad20 (BulletParam id, matrix, the phase it ran in), SpEffects added FUN_1409e9c00 to
//! Wolf or the General, AdjustItemCount FUN_14079ab70, the live bullet slots, the hits resolved
//! (hits.rs fields), both chrs' state. Writes extracted/emu/prosth_<name>.json for
//! src/sim_tests/emu_prosth.rs.

use std::ffi::c_void;
use std::path::Path;
use std::sync::Mutex;

use serde_json::{json, Value};

use super::acttrace::{self, Input, CHR_FRAME, DT, TASKS_POST, TASKS_PRE, WORLD_CHR_MAN};
use super::hits::{self, AFTER_APPLY, HIT_MANAGER, HIT_SIZE, HIT_STEP, RESOLVE};
use super::{items, proxy, turn, uc, Emu};

const HOOK_MEM_WRITE: i32 = 2048;
const SUB_ATTACK: u32 = 1;
const CHANGE_WEAPON_L: u32 = 10;
const TAE_UPDATE: u64 = 0x1_40b5_c920;
const BULLET_MAN: u64 = 0x1_43d8_4f70;
const BULLET_STEP: u64 = 0x1_40b1_c050;
const SPAWN: u64 = 0x1_40b1_ad20;
const SLOTS: u64 = 0x80;
const SLOT_SIZE: u64 = 0xd30;
/// The emblems' goods (inventory id 0x4000_0000 | 1000).
const EMBLEM: u32 = 0x4000_03e8;

/// (address, name) of the hooked calls.
const WATCH: [(u64, &str); 7] = [
    (0x1_40b5_1110, "tae_bullet"),
    (0x1_40b5_0ae0, "tae_attack"),
    (0x1_40b5_5510, "tae_pc"),
    (0x1_407d_4fa0, "bullet_emit"),
    (SPAWN, "spawn"),
    (0x1_409e_9c00, "sp_add"),
    (0x1_4079_ab70, "adjust_count"),
];

struct Seen {
    wolf: u64,
    general: u64,
    phase: &'static str,
    events: Vec<Value>,
    anims: Vec<(&'static str, i64, f32, f32)>,
    resolves: Vec<(u64, u64, Vec<u8>, &'static str)>,
    applied: Vec<Vec<u8>>,
}
static SEEN: Mutex<Seen> = Mutex::new(Seen { wolf: 0, general: 0, phase: "", events: Vec::new(), anims: Vec::new(), resolves: Vec::new(), applied: Vec::new() });

fn reg(uc: uc::Engine, id: i32) -> u64 {
    let mut v = 0u64;
    unsafe { uc::uc_reg_read(uc, id, &mut v as *mut u64 as _) };
    v
}

fn xmm(uc: uc::Engine, i: i32) -> f32 {
    let mut b = [0u8; 16];
    unsafe { uc::uc_reg_read(uc, uc::XMM0 + i, b.as_mut_ptr() as _) };
    f32::from_le_bytes(b[..4].try_into().unwrap())
}

fn rd<const N: usize>(uc: uc::Engine, at: u64) -> Option<[u8; N]> {
    let mut b = [0u8; N];
    (unsafe { uc::uc_mem_read(uc, at, b.as_mut_ptr() as _, N) } == 0).then_some(b)
}

fn rd_i32(uc: uc::Engine, at: u64) -> Option<i32> {
    rd::<4>(uc, at).map(i32::from_le_bytes)
}

fn rd_u64(uc: uc::Engine, at: u64) -> u64 {
    rd::<8>(uc, at).map_or(0, u64::from_le_bytes)
}

fn rd_f32s(uc: uc::Engine, at: u64, n: usize) -> Vec<f32> {
    (0..n).map(|i| rd::<4>(uc, at + 4 * i as u64).map_or(f32::NAN, f32::from_le_bytes)).collect()
}

extern "C" fn on_watch(uc: uc::Engine, addr: u64, _size: u32, _data: *mut c_void) {
    let Some(&(_, name)) = WATCH.iter().find(|(a, _)| *a == addr) else { return };
    let (rcx, rdx, r8) = (reg(uc, uc::RCX), reg(uc, uc::RDX), reg(uc, uc::R8));
    let mut s = SEEN.lock().unwrap();
    let (wolf, general) = (s.wolf, s.general);
    let who = |c: u64| if c == wolf { "wolf" } else if c == general { "general" } else { "other" };
    let mut ev = match name {
        // (TAE context: +0x10 the chr; the event: +8 its data).
        "tae_bullet" | "tae_attack" | "tae_pc" => {
            if rd_u64(uc, rcx + 0x10) != wolf {
                return;
            }
            let data = rd_u64(uc, rdx + 8);
            json!({"fn": name, "ints": (0..6).map(|i| rd_i32(uc, data + 4 * i)).collect::<Vec<_>>()})
        }
        // (emitter: +0x98 the chr, BehaviorParam id, dummy poly).
        "bullet_emit" => json!({"fn": name, "chr": who(rd_u64(uc, rcx + 0x98)), "behavior": rdx as u32 as i32, "dummy": r8 as u32 as i32}),
        // (BulletMan, out, spawn record: +0x10 BulletParam id, +0x40 the emit matrix: rows x / y /
        // z with the position in the 4th column).
        "spawn" => {
            let m = rd_f32s(uc, r8 + 0x40, 12);
            json!({
                "fn": name, "bullet": rd_i32(uc, r8 + 0x10), "ints": (0..16).map(|i| rd_i32(uc, r8 + 4 * i)).collect::<Vec<_>>(),
                "pos": [m[3], m[7], m[11]], "dir": [m[2], m[6], m[10]],
            })
        }
        "sp_add" => {
            if who(rcx) == "other" {
                return;
            }
            json!({"fn": name, "chr": who(rcx), "id": rdx as u32 as i32, "source": who(r8)})
        }
        "adjust_count" => json!({"fn": name, "slot": rdx as u32 as i32, "delta": r8 as u32 as i32}),
        _ => return,
    };
    ev["phase"] = json!(s.phase);
    s.events.push(ev);
}

extern "C" fn on_tae(uc: uc::Engine, _addr: u64, _size: u32, _data: *mut c_void) {
    let mut s = SEEN.lock().unwrap();
    let p = s.phase;
    s.anims.push((p, reg(uc, uc::RCX) as u32 as i32 as i64, xmm(uc, 1), xmm(uc, 2)));
}

fn hit_bytes(uc: uc::Engine) -> Vec<u8> {
    let at = reg(uc, uc::R8);
    (0..HIT_SIZE / 8).flat_map(|i| rd_u64(uc, at + 8 * i as u64).to_le_bytes()).collect()
}

extern "C" fn on_resolve(uc: uc::Engine, _addr: u64, _size: u32, _data: *mut c_void) {
    let b = hit_bytes(uc);
    let mut s = SEEN.lock().unwrap();
    let p = s.phase;
    s.resolves.push((reg(uc, uc::RCX), reg(uc, uc::RDX), b, p));
}

extern "C" fn on_after(uc: uc::Engine, _addr: u64, _size: u32, _data: *mut c_void) {
    let b = hit_bytes(uc);
    SEEN.lock().unwrap().applied.push(b);
}

fn try_u32(emu: &mut Emu, at: u64) -> Option<u32> {
    let mut b = [0u8; 4];
    if unsafe { uc::uc_mem_read(emu.uc, at, b.as_mut_ptr() as _, 4) } == 0 {
        return Some(u32::from_le_bytes(b));
    }
    let (from, bytes) = emu.st.snap.as_mut()?.window(at)?;
    emu.map(from, bytes.len() as u64);
    emu.raw_write(from, &bytes);
    Some(u32::from_le_bytes(emu.read(at)))
}

fn f32at(emu: &mut Emu, at: u64) -> f32 {
    f32::from_le_bytes(emu.peek(at))
}

/// The live bullet slots (bullets.rs): slot, BulletParam id, position, metres flown, age, flags.
fn slots(emu: &mut Emu, pool: u64) -> Vec<Value> {
    let mut out = Vec::new();
    for i in 0..SLOTS {
        let s = pool + i * SLOT_SIZE;
        let flags = emu.peek::<1>(s + 0xc6c)[0];
        if flags & 1 == 0 {
            continue;
        }
        let pos: Vec<f32> = (0..3).map(|k| f32at(emu, s + 0x10 + 4 * k)).collect();
        out.push(json!({
            "slot": i, "bullet": i32::from_le_bytes(emu.peek(s + 0x4b4)), "pos": pos,
            "flown": f32at(emu, s + 0xb60), "age": f32at(emu, s + 0xcd4), "flags": flags,
        }));
    }
    out
}

/// The chr's left weapon id the HKS reads ([bag+0xb8] +0x370).
fn left_weapon(emu: &mut Emu, chr: u64) -> i32 {
    let m = hits::module(emu, chr, 0xb8);
    i32::from_le_bytes(emu.peek(m + 0x370))
}

fn setup(emu: &mut Emu) {
    emu.limit = 500_000_000;
    emu.raw_write(0x1_43c1_d520, &1u32.to_le_bytes());
    emu.raw_write(proxy::LONGJMP, proxy::PLAIN_LONGJMP);
    emu.raw_write(acttrace::MENU_STATE, &[0x31, 0xc0, 0xc3]);
    let menus = emu.call(0x1_40d8_72c0, &[], &[]).rax;
    if menus != 0 {
        emu.u64(menus + 0x100, 0);
    }
}

struct Case {
    name: &'static str,
    /// EquipParamWeapon id of the tool level.
    tool: u32,
    /// Frames ACTION_ARM_SUB_ATTACK is held from the first world frame.
    hold: usize,
    /// Wolf's distance in front of the General.
    dist: f32,
    /// Whole world frames run.
    frames: usize,
}

const CASES: &[Case] = &[
    Case { name: "shuriken_tap", tool: 70000, hold: 3, dist: 6.0, frames: 70 },
    Case { name: "shuriken_hold", tool: 70000, hold: 40, dist: 6.0, frames: 80 },
    Case { name: "firecracker", tool: 71000, hold: 3, dist: 3.0, frames: 110 },
    Case { name: "flame_vent", tool: 72000, hold: 3, dist: 3.0, frames: 140 },
    Case { name: "okinaga_vent", tool: 72200, hold: 3, dist: 3.0, frames: 140 },
    Case { name: "axe", tool: 73000, hold: 3, dist: 2.5, frames: 110 },
    Case { name: "mist_raven", tool: 74000, hold: 3, dist: 3.0, frames: 110 },
    Case { name: "sabimaru", tool: 75000, hold: 3, dist: 2.5, frames: 110 },
    Case { name: "umbrella", tool: 76000, hold: 60, dist: 3.0, frames: 100 },
    Case { name: "divine_abduction", tool: 77000, hold: 3, dist: 3.0, frames: 120 },
    Case { name: "spear", tool: 78000, hold: 3, dist: 3.0, frames: 110 },
    Case { name: "spiral_spear", tool: 78300, hold: 3, dist: 3.0, frames: 110 },
    Case { name: "whistle", tool: 79000, hold: 3, dist: 4.0, frames: 120 },
];

type Phase<'a> = (&'static str, &'a dyn Fn(&mut Emu) -> Option<(u64, i32)>);

fn run_case(new_emu: &dyn Fn() -> Emu, out: &Path, c: &Case) {
    let t0 = std::time::Instant::now();
    let mut emu = new_emu();
    let w = turn::wolf(&mut emu);
    let ids = acttrace::arm_inputs(&mut emu);
    let g = acttrace::general(&mut emu);
    let wcm = emu.peek_u64(WORLD_CHR_MAN);
    let mgr = emu.peek_u64(HIT_MANAGER);
    let bm = emu.peek_u64(BULLET_MAN);
    let pool = emu.peek_u64(bm);
    let pgd = emu.peek_u64(w.player + 0x2000);
    {
        let mut s = SEEN.lock().unwrap();
        *s = Seen { wolf: w.player, general: g, phase: "", events: Vec::new(), anims: Vec::new(), resolves: Vec::new(), applied: Vec::new() };
    }
    // The tool's inventory entry (+0 its handle) into prosthetic slot 1 = equip slot 2.
    let inv = items::inventory(&mut emu, w.player);
    let (slot, _, _, at) = inv.iter().find(|e| e.1 == c.tool).copied().unwrap_or_else(|| panic!("{}: tool {} not in the inventory", c.name, c.tool));
    let handle = try_u32(&mut emu, at).unwrap();
    emu.raw_write(pgd + 0x310 + 0x24 + 4 * 2, &handle.to_le_bytes());
    emu.raw_write(pgd + 0x278 + 0x24 + 4 * 2, &slot.to_le_bytes());
    emu.raw_write(pgd + 0x278 + 0x374 + 4 * 2, &c.tool.to_le_bytes());
    // Spirit Emblems: 20.
    let emblems = inv.iter().find(|e| e.1 == EMBLEM).copied().expect("no emblem entry");
    emu.raw_write(emblems.3 + 8, &20i32.to_le_bytes());
    setup(&mut emu);
    acttrace::every_frame(&mut emu, g);
    for (a, _) in WATCH {
        emu.hook(uc::HOOK_CODE, on_watch as *const c_void, a, a);
    }
    emu.hook(uc::HOOK_CODE, on_tae as *const c_void, TAE_UPDATE, TAE_UPDATE);
    emu.hook(uc::HOOK_CODE, on_resolve as *const c_void, RESOLVE, RESOLVE);
    emu.hook(uc::HOOK_CODE, on_after as *const c_void, AFTER_APPLY, AFTER_APPLY);
    // The switch (Wolf's chr frames alone): CHANGE_WEAPON_L held 3 frames, then idle until the
    // expand a0<group>_412000 has played out.
    let mut switch = Vec::new();
    let expand = (c.tool / 1000) as i64 * 1_000_000 + 412000;
    let mut seen_expand = false;
    for f in 0..240 {
        let arms: &'static [u32] = if f < 3 { &[CHANGE_WEAPON_L] } else { &[] };
        acttrace::press(&mut emu, w.stick, &ids, &Input { arms, stick: [0.0, 0.0] });
        proxy::flush_tb(&emu);
        SEEN.lock().unwrap().phase = "switch";
        let r = emu.call(CHR_FRAME, &[wcm, 0, w.player], &[0.0, DT]);
        assert!(r.stopped.is_none(), "{}: switch frame {f} stopped {:?}", c.name, r.stopped);
        let a = acttrace::anim(&mut emu, w.player);
        seen_expand |= a == expand;
        switch.push(json!({"anim": a, "left": left_weapon(&mut emu, w.player), "active": try_u32(&mut emu, pgd + 0x31c)}));
        if seen_expand && a != expand {
            break;
        }
    }
    let left = left_weapon(&mut emu, w.player);
    println!("prosth {}: tool {} (inventory {slot}, handle {handle:#x}), switch {} frames, expand seen {seen_expand}, left weapon {left}", c.name, c.tool, switch.len());
    {
        let mut s = SEEN.lock().unwrap();
        s.events.clear();
        s.anims.clear();
        s.resolves.clear();
        s.applied.clear();
    }
    // Wolf `dist` m in front of the General, facing him (bullets.rs).
    let gp = acttrace::pos(&mut emu, g);
    let gphys = hits::module(&mut emu, g, 0x68);
    let gyaw = f32at(&mut emu, gphys + 0x74);
    let (fx, fz) = (-gyaw.sin(), -gyaw.cos());
    let wpos = [gp[0] + c.dist * fx, gp[1], gp[2] + c.dist * fz];
    let wphys = hits::module(&mut emu, w.player, 0x68);
    let (wrapper, _) = proxy::proxy_of(&mut emu, wphys);
    proxy::place(&mut emu, wphys, wrapper, wpos);
    emu.poke_f32s(wphys + 0x74, &[gyaw + std::f32::consts::PI]);
    let time = turn::fd4_time(&mut emu, DT);
    let blk = emu.alloc(0x40);
    emu.poke_f32s(blk + 8, &[DT]);
    let (wdmod, gdmod) = (hits::module(&mut emu, w.player, 0x98), hits::module(&mut emu, g, 0x98));
    let dt_bits = DT.to_bits() as u64;
    let mut frames = Vec::new();
    let mut stopped = None;
    for f in 0..c.frames {
        let arms: &'static [u32] = if f < c.hold { &[SUB_ATTACK] } else { &[] };
        acttrace::press(&mut emu, w.stick, &ids, &Input { arms, stick: [0.0, 0.0] });
        proxy::flush_tb(&emu);
        let bullets = |e: &mut Emu| e.call(BULLET_STEP, &[bm, time], &[]).stopped;
        let hit_step = |e: &mut Emu| e.call(HIT_STEP, &[mgr, dt_bits], &[0.0, DT]).stopped;
        let pre = |e: &mut Emu| TASKS_PRE.iter().fold(None, |s, &t| s.or(e.call(t, &[wcm, blk], &[]).stopped));
        let general = |e: &mut Emu| e.call(CHR_FRAME, &[wcm, 0, g], &[0.0, DT]).stopped;
        let wolf = |e: &mut Emu| e.call(CHR_FRAME, &[wcm, 0, w.player], &[0.0, DT]).stopped;
        let post = |e: &mut Emu| TASKS_POST.iter().fold(None, |s, &t| s.or(e.call(t, &[wcm, blk], &[]).stopped));
        let phases: [Phase; 6] = [("bullets", &bullets), ("hit_step", &hit_step), ("pre_tasks", &pre), ("general", &general), ("wolf", &wolf), ("post_tasks", &post)];
        let mut stop = None;
        let mut moved = Vec::new();
        for (name, run) in phases {
            SEEN.lock().unwrap().phase = name;
            if stop.is_none() {
                stop = run(&mut emu);
            }
            if name == "bullets" {
                moved = slots(&mut emu, pool);
            }
        }
        let (events, anims, resolves, applied) = {
            let mut s = SEEN.lock().unwrap();
            (std::mem::take(&mut s.events), std::mem::take(&mut s.anims), std::mem::take(&mut s.resolves), std::mem::take(&mut s.applied))
        };
        let mut hits_v = Vec::new();
        for (k, (dmod, att, before, phase)) in resolves.iter().enumerate() {
            let after = applied.get(k).cloned().unwrap_or_default();
            let mut h = if after.len() == HIT_SIZE { hits::hit_fields(&after) } else { json!({}) };
            h["in"] = hits::hit_fields(before);
            h["phase"] = json!(phase);
            h["defender"] = json!(if *dmod == wdmod { "wolf" } else if *dmod == gdmod { "general" } else { "other" });
            h["attacker"] = json!(if *att == w.player { "wolf" } else if *att == g { "general" } else { "other" });
            hits_v.push(h);
        }
        let clips = |who: &str| anims.iter().filter(|a| a.0 == who).map(|a| json!([a.1, a.2, a.3])).collect::<Vec<_>>();
        let (wtae, gtae) = (clips("wolf"), clips("general"));
        let (mut ws, mut gs) = (hits::chr_state(&mut emu, w.player), hits::chr_state(&mut emu, g));
        ws["anim"] = json!(acttrace::anim(&mut emu, w.player));
        ws["emblems"] = json!(try_u32(&mut emu, emblems.3 + 8).unwrap() as i32);
        gs["anim"] = json!(acttrace::anim(&mut emu, g));
        let live = slots(&mut emu, pool);
        if !events.is_empty() || !hits_v.is_empty() {
            let short: Vec<String> = events
                .iter()
                .map(|e| {
                    let mut e = e.clone();
                    if let Some(o) = e.as_object_mut() {
                        o.remove("ints");
                    }
                    serde_json::to_string(&e).unwrap()
                })
                .collect();
            let wc: Vec<String> = wtae.iter().map(|c| format!("{}@{:.3}", c[0], c[2].as_f64().unwrap_or(0.0))).collect();
            println!("  f{f} wolf {} {wc:?} | {short:?} | live {}", ws["anim"], live.len());
            for h in &hits_v {
                println!("    hit {} -> {} in {}: hp {} posture {} guarded {}", h["attacker"], h["defender"], h["phase"], h["hp_dmg"], h["posture_dmg"], h["guarded"]);
            }
        }
        frames.push(json!({"f": f, "events": events, "wolf": ws, "general": gs, "wolf_tae": wtae, "general_tae": gtae, "slots": live, "slots_moved": moved, "hits": hits_v}));
        if let Some(s) = stop {
            stopped = Some(format!("{:#x} error {}", s.0, s.1));
            println!("  stopped {stopped:?} trail {:x?}", emu.st.trail);
            break;
        }
    }
    println!("prosth {} ({} frames, {:.0} s)", c.name, frames.len(), t0.elapsed().as_secs_f32());
    let v = json!({
        "name": c.name, "tool": c.tool, "hold": c.hold, "dist": c.dist, "dt": DT, "left_weapon": left,
        "expand_seen": seen_expand, "switch": switch, "general_pos": gp, "general_yaw": gyaw, "wolf_pos": wpos,
        "frames": frames, "stopped": stopped,
    });
    std::fs::write(out.join(format!("prosth_{}.json", c.name)), serde_json::to_string(&v).unwrap()).unwrap();
}

static WRITES: Mutex<Vec<(u64, u64, i64)>> = Mutex::new(Vec::new());

extern "C" fn on_write(uc: uc::Engine, _kind: i32, addr: u64, _size: i32, value: i64, _data: *mut c_void) {
    WRITES.lock().unwrap().push((reg(uc, uc::RIP), addr, value));
}

/// `prosth_probe`: the equip slots, then one switch with the writes to the active slot (the
/// PlayerGameData ChrAsm and the chr's) and the chr's left weapon id recorded.
fn probe(new_emu: &dyn Fn() -> Emu) {
    let mut emu = new_emu();
    let w = turn::wolf(&mut emu);
    let ids = acttrace::arm_inputs(&mut emu);
    let wcm = emu.peek_u64(WORLD_CHR_MAN);
    let pgd = emu.peek_u64(w.player + 0x2000);
    let asm = emu.peek_u64(w.player + 0x2140);
    let lw = hits::module(&mut emu, w.player, 0xb8) + 0x370;
    for k in 0..6u64 {
        let idx = try_u32(&mut emu, pgd + 0x278 + 0x24 + 4 * k).unwrap() as i32;
        let h = try_u32(&mut emu, pgd + 0x310 + 0x24 + 4 * k).unwrap();
        let id = try_u32(&mut emu, pgd + 0x278 + 0x374 + 4 * k).unwrap();
        println!("  equip slot {k}: inventory {idx}, handle {h:#x}, id {id}");
    }
    setup(&mut emu);
    emu.hook(HOOK_MEM_WRITE, on_write as *const c_void, pgd + 0x318, pgd + 0x31f);
    emu.hook(HOOK_MEM_WRITE, on_write as *const c_void, asm + 0xc, asm + 0xf);
    emu.hook(HOOK_MEM_WRITE, on_write as *const c_void, lw, lw + 3);
    for f in 0..8 {
        let arms: &'static [u32] = if f < 3 { &[CHANGE_WEAPON_L] } else { &[] };
        acttrace::press(&mut emu, w.stick, &ids, &Input { arms, stick: [0.0, 0.0] });
        proxy::flush_tb(&emu);
        emu.call(CHR_FRAME, &[wcm, 0, w.player], &[0.0, DT]);
        let ws: Vec<String> = std::mem::take(&mut *WRITES.lock().unwrap()).iter().map(|(rip, a, v)| format!("{a:#x}={} @{rip:#x}", *v as i32)).collect();
        println!("  f{f} anim {} writes {ws:?}", acttrace::anim(&mut emu, w.player));
    }
}

/// `prosth` runs every case; `prosth_<name>` one; `prosth_probe` the switch probe.
pub fn run(new_emu: &dyn Fn() -> Emu, out: &Path, want: &[&str]) {
    if want.contains(&"prosth_probe") {
        return probe(new_emu);
    }
    let names: Vec<&str> = want.iter().filter_map(|w| w.strip_prefix("prosth_")).collect();
    for c in CASES {
        if names.is_empty() || names.contains(&c.name) {
            run_case(new_emu, out, c);
        }
    }
}
