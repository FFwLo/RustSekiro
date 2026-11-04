//! `items`: Wolf using a quick item or a prosthetic tool in whole chr frames FUN_140a4b570 from the
//! snapshot (pad as acttrace: the pad frame [pad+0x38f8] byte +0x67 + input id per HKS ACTION_ARM),
//! with the use chain hooked so each case says on which frame of the anim what happened:
//!
//! - TAE 65-ish item event FUN_140b57740 (ConsumeCurrentGoods: chr +0x11c4 the item in use, +0x11c8
//!   the uses) -> the bullet emitter's goods launch FUN_1407d5d50 (ChrIns +0x10e0, rdx goods id) ->
//!   the launch FUN_1409e4cd0: goods refCategory (EquipParamGoods +0x3b) 2 -> FUN_1409e4fe0 ->
//!   the SpEffect add FUN_1409e9c00 (rdx id); 1 -> FUN_1409e4e60 -> the bullet FUN_140b1ad20;
//!   then PlayerIns::vf125 FUN_140a262d0 -> FUN_14079afa0 (consume) -> AdjustItemCount
//!   FUN_14079ab70 (rdx inventory slot, r8 delta).
//! - TAE 2 BulletBehavior FUN_140b51110 -> the emitter FUN_1407d4fa0 (rdx BehaviorParam id, r8 the
//!   dummy poly) -> FUN_1409e4cd0 -> the bullet FUN_140b1ad20 (BulletMan [0x143d84f70], r8 the
//!   spawn record FUN_1409e4950 builds: +0x10 BulletParam id, +0x40.. the emit matrix).
//!
//! The item is chosen through the inventory's override +0x358 (FUN_14079e2a0, which FUN_140a369b0
//! copies to the quick item chr +0x11c0 each frame and FUN_140a28160 to the item in use +0x11c4
//! when the HKS fixes the item, act 2024) and, when Wolf holds none, an inventory entry. Writes
//! extracted/emu/items_<name>.json for src/sim_tests/emu_items.rs.

use std::collections::BTreeMap;
use std::ffi::c_void;
use std::path::Path;
use std::sync::Mutex;

use serde_json::{json, Value};

use super::{proxy, turn, uc, Emu};

const WORLD_CHR_MAN: u64 = 0x1_43d7_a1e0;
const CHR_FRAME: u64 = 0x1_40a4_b570;
const ARM_INPUT: u64 = 0x1_40b2_58d0;
const MENU_STATE: u64 = 0x1_408c_58e0;
const TAE_UPDATE: u64 = 0x1_40b5_c920;
const DT: f32 = 1.0 / 60.0;

const SUB_ATTACK: u32 = 1;
const USE_ITEM: u32 = 7;

/// The hooked functions: (address, name).
const WATCH: [(u64, &str); 10] = [
    (0x1_40b5_7740, "tae_item"),
    (0x1_407d_5d50, "goods_emit"),
    (0x1_409e_4cd0, "launch"),
    (0x1_409e_9c00, "sp_add"),
    (0x1_4079_afa0, "consume"),
    (0x1_4079_ab70, "adjust_count"),
    (0x1_40b5_1110, "tae_bullet"),
    (0x1_407d_4fa0, "bullet_emit"),
    (0x1_40b1_ad20, "bullet"),
    (0x1_40a2_8160, "fix_item"),
];

static EVENTS: Mutex<Vec<Value>> = Mutex::new(Vec::new());
static ANIMS: Mutex<Vec<i64>> = Mutex::new(Vec::new());
static WOLF: Mutex<u64> = Mutex::new(0);

fn reg(uc: uc::Engine, id: i32) -> u64 {
    let mut v = 0u64;
    unsafe { uc::uc_reg_read(uc, id, &mut v as *mut u64 as _) };
    v
}

fn mem<const N: usize>(uc: uc::Engine, at: u64) -> Option<[u8; N]> {
    let mut b = [0u8; N];
    (unsafe { uc::uc_mem_read(uc, at, b.as_mut_ptr() as _, N) } == 0).then_some(b)
}

fn i32_at(uc: uc::Engine, at: u64) -> Option<i32> {
    mem::<4>(uc, at).map(i32::from_le_bytes)
}

fn f32s_at(uc: uc::Engine, at: u64, n: usize) -> Vec<f32> {
    (0..n).map(|i| mem::<4>(uc, at + 4 * i as u64).map_or(f32::NAN, f32::from_le_bytes)).collect()
}

extern "C" fn on_watch(uc: uc::Engine, addr: u64, _size: u32, _data: *mut c_void) {
    let Some(&(_, name)) = WATCH.iter().find(|(a, _)| *a == addr) else { return };
    let (rcx, rdx, r8, r9) = (reg(uc, uc::RCX), reg(uc, uc::RDX), reg(uc, uc::R8), reg(uc, uc::R9));
    let wolf = *WOLF.lock().unwrap();
    let ev = match name {
        // FUN_140b57740(event ctx, event): ctx +0x10 the chr.
        "tae_item" => {
            let chr = mem::<8>(uc, rcx + 0x10).map_or(0, u64::from_le_bytes);
            if chr != wolf {
                return;
            }
            json!({"fn": name, "item": i32_at(uc, chr + 0x11c4), "uses": i32_at(uc, chr + 0x11c8)})
        }
        "tae_bullet" => {
            let chr = mem::<8>(uc, rcx + 0x10).map_or(0, u64::from_le_bytes);
            let ev = mem::<8>(uc, rdx + 8).map_or(0, u64::from_le_bytes);
            json!({"fn": name, "wolf": chr == wolf, "event": (0..5).map(|i| i32_at(uc, ev + 4 * i)).collect::<Vec<_>>()})
        }
        // (emitter ChrIns+0x10e0, goods id, event, ...)
        "goods_emit" => json!({"fn": name, "goods": rdx as u32 as i32}),
        // (launch record, emitter slot, chr): +0 magic?, +4 goods, +8 behaviour, +0xc bullet.
        "launch" => json!({"fn": name, "record": (0..8).map(|i| i32_at(uc, rcx + 4 * i)).collect::<Vec<_>>(), "wolf": r8 == wolf}),
        "sp_add" => {
            if rcx != wolf {
                return;
            }
            json!({"fn": name, "id": rdx as u32 as i32, "source_wolf": r8 == wolf})
        }
        "consume" => json!({"fn": name, "item": i32_at(uc, rdx), "no_consume": r8 as u8}),
        "adjust_count" => json!({"fn": name, "slot": rdx as u32 as i32, "delta": r8 as u32 as i32}),
        "bullet_emit" => {
            if mem::<8>(uc, rcx + 0x98).map_or(0, u64::from_le_bytes) != wolf {
                return;
            }
            json!({"fn": name, "behavior": rdx as u32 as i32, "dummy": r8 as u32 as i32, "r9": r9 as u32 as i32})
        }
        // FUN_140b1ad20(BulletMan, out, spawn record): record ints 0..16 and the floats after.
        "bullet" => json!({
            "fn": name,
            "ints": (0..16).map(|i| i32_at(uc, r8 + 4 * i)).collect::<Vec<_>>(),
            "floats": f32s_at(uc, r8 + 0x40, 24),
        }),
        "fix_item" => {
            if rcx != wolf {
                return;
            }
            json!({"fn": name, "quick": i32_at(uc, rcx + 0x11c0), "in_use": i32_at(uc, rcx + 0x11c4)})
        }
        _ => return,
    };
    EVENTS.lock().unwrap().push(ev);
}

extern "C" fn on_tae(uc: uc::Engine, _addr: u64, _size: u32, _data: *mut c_void) {
    ANIMS.lock().unwrap().push(reg(uc, uc::RCX) as u32 as i32 as i64);
}

/// Inventory entries of Wolf's PlayerGameData ([chr+0x2000] +0x278 inventory, +0x168 list:
/// [+0x88]+1 slots; slot < [+0x24] at [+0x58] + 0x10 * slot, else [+0x48] + 0x10 * (slot - [+0x24]);
/// entry +4 item id, +8 count; FUN_1407aabf0). (slot, id, count, entry address).
pub(super) fn inventory(emu: &mut Emu, chr: u64) -> Vec<(u32, u32, i32, u64)> {
    let pgd = emu.peek_u64(chr + 0x2000);
    let list = pgd + 0x278 + 0x168;
    let n = u32::from_le_bytes(emu.peek(list + 0x88)).wrapping_add(1);
    let split = u32::from_le_bytes(emu.peek(list + 0x24));
    let (lo, hi) = (emu.peek_u64(list + 0x58), emu.peek_u64(list + 0x48));
    let mut out = Vec::new();
    for slot in 0..n.min(3000) {
        let e = if slot < split { lo + 0x10 * slot as u64 } else { hi + 0x10 * (slot - split) as u64 };
        let id = u32::from_le_bytes(emu.peek(e + 4));
        let count = i32::from_le_bytes(emu.peek(e + 8));
        if id != 0 && id != u32::MAX {
            out.push((slot, id, count, e));
        }
    }
    out
}

fn sp_list(emu: &mut Emu, chr: u64) -> Vec<i32> {
    let list = emu.peek_u64(chr + 0x11d0);
    let mut out = Vec::new();
    let mut e = if list == 0 { 0 } else { emu.peek_u64(list + 8) };
    while e != 0 && out.len() < 200 {
        out.push(i32::from_le_bytes(emu.peek(e + 0x58)));
        e = emu.peek_u64(e + 0x70);
    }
    out
}

fn hp(emu: &mut Emu, chr: u64) -> i32 {
    let bag = emu.peek_u64(chr + 0x1ff8);
    let data = emu.peek_u64(bag + 0x18);
    i32::from_le_bytes(emu.peek(data + 0x130))
}

fn anim(emu: &mut Emu, chr: u64) -> i64 {
    let bag = emu.peek_u64(chr + 0x1ff8);
    let m = emu.peek_u64(bag + 0x10);
    i32::from_le_bytes(emu.peek(m + 0xf8)) as i64
}

fn pos(emu: &mut Emu, chr: u64) -> [f32; 4] {
    let bag = emu.peek_u64(chr + 0x1ff8);
    let phys = emu.peek_u64(bag + 0x68);
    [0x80, 0x84, 0x88, 0x74].map(|o| f32::from_le_bytes(emu.peek(phys + o)))
}

fn setup(emu: &mut Emu) {
    emu.limit = 500_000_000;
    emu.raw_write(0x1_43c1_d520, &1u32.to_le_bytes());
    emu.raw_write(proxy::LONGJMP, proxy::PLAIN_LONGJMP);
    emu.raw_write(MENU_STATE, &[0x31, 0xc0, 0xc3]);
    let menus = emu.call(0x1_40d8_72c0, &[], &[]).rax;
    if menus != 0 {
        emu.u64(menus + 0x100, 0);
    }
    for (a, _) in WATCH {
        emu.hook(uc::HOOK_CODE, on_watch as *const c_void, a, a);
    }
    emu.hook(uc::HOOK_CODE, on_tae as *const c_void, TAE_UPDATE, TAE_UPDATE);
}

fn press(emu: &mut Emu, frame: u64, ids: &BTreeMap<u32, i32>, arms: &[u32]) {
    for id in ids.values().filter(|&&i| (0..0x80).contains(&i)) {
        emu.raw_write(frame + 0x67 + *id as u64, &[0]);
    }
    for a in arms {
        let id = ids[a];
        if id >= 0 {
            emu.raw_write(frame + 0x67 + id as u64, &[1]);
        }
    }
    emu.raw_write(frame + 0x3c, &[0]);
    emu.raw_write(frame + 0x3e, &[0]);
    emu.poke_f32s(frame, &[0.0, 0.0]);
}

/// One case: `goods` the quick item (None: as saved), `count` its inventory count if set,
/// `arm` the button held for `hold` frames, `frames` frames in all.
struct Case {
    name: &'static str,
    goods: Option<u32>,
    count: Option<i32>,
    arm: u32,
    hold: usize,
    frames: usize,
}

const CASES: &[Case] = &[
    Case { name: "probe", goods: None, count: None, arm: USE_ITEM, hold: 0, frames: 1 },
    Case { name: "gourd", goods: Some(3000), count: None, arm: USE_ITEM, hold: 3, frames: 90 },
    Case { name: "pellet", goods: Some(3020), count: Some(3), arm: USE_ITEM, hold: 3, frames: 90 },
    Case { name: "ohagi", goods: Some(3060), count: Some(1), arm: USE_ITEM, hold: 3, frames: 120 },
    Case { name: "ako", goods: Some(3400), count: Some(3), arm: USE_ITEM, hold: 3, frames: 90 },
    Case { name: "gokan", goods: Some(3420), count: Some(3), arm: USE_ITEM, hold: 3, frames: 90 },
    Case { name: "spiritfall", goods: Some(3600), count: Some(1), arm: USE_ITEM, hold: 3, frames: 120 },
    Case { name: "pot", goods: Some(3510), count: Some(3), arm: USE_ITEM, hold: 3, frames: 90 },
    // The snapshot's tool: the Loaded Axe 73000 (PlayerGameData +0x5ec; +0x5f4 78300, +0x5fc
    // 72200). gap: writing another tool there does not change what Wolf swings (the chr's own
    // equip copy is not found), so the shuriken / firecracker / flame vent / umbrella are not run.
    Case { name: "prosthetic", goods: None, count: None, arm: SUB_ATTACK, hold: 3, frames: 120 },
];

fn run_case(new_emu: &dyn Fn() -> Emu, out: &Path, c: &Case) {
    let t = std::time::Instant::now();
    let mut emu = new_emu();
    let w = turn::wolf(&mut emu);
    *WOLF.lock().unwrap() = w.player;
    let ids: BTreeMap<u32, i32> = (0..33u32).map(|arm| (arm, emu.call(ARM_INPUT, &[arm as u64], &[]).rax as u32 as i32)).collect();
    let wcm = emu.peek_u64(WORLD_CHR_MAN);
    let inv = inventory(&mut emu, w.player);
    let quick: Vec<i32> = (0..4).map(|i| i32::from_le_bytes(emu.peek(w.player + 0x11c0 + 4 * i))).collect();
    let debug: Vec<u8> = (0..0x12).map(|i| emu.peek::<1>(0x1_43d7_a366 + i)[0]).collect();
    println!("{}: quick +0x11c0.. {quick:x?}, debug flags {debug:?}", c.name);
    println!("  inventory ({}): {:?}", inv.len(), inv.iter().map(|(s, id, n, _)| (*s, format!("{id:#x}"), *n)).collect::<Vec<_>>());
    if c.name == "probe" {
        // Where PlayerGameData holds the prosthetic tools (EquipParamWeapon 70000 .. 79999).
        let pgd = emu.peek_u64(w.player + 0x2000);
        for off in (0..0x1000u64).step_by(4) {
            let v = u32::from_le_bytes(emu.peek(pgd + off));
            if (70000..80000).contains(&v) || (5000..5100).contains(&v) {
                println!("  pgd +{off:#x} = {v}");
            }
        }
        return;
    }
    if let Some(g) = c.goods {
        let item = 0x4000_0000 | g;
        // The inventory entry: the goods' own if held, else the first goods entry rewritten.
        let held = inv.iter().find(|e| e.1 == item).copied();
        let entry = held.or_else(|| inv.iter().find(|e| e.1 >> 28 == 4 && e.1 != 0x4000_0bb8 && e.1 & 0xfff_ffff >= 3000).copied());
        if let Some((slot, _, n, at)) = entry {
            emu.raw_write(at + 4, &item.to_le_bytes());
            let n = c.count.unwrap_or(n);
            emu.raw_write(at + 8, &n.to_le_bytes());
            println!("  goods {g}: slot {slot} count {n} (held {})", held.is_some());
        }
        // The quick item FUN_140a369b0 copies into chr +0x11c0 each frame: FUN_14079e2a0's
        // override, inventory +0x358 (else the selected quick slot's entry).
        let pgd = emu.peek_u64(w.player + 0x2000);
        emu.raw_write(pgd + 0x278 + 0x358, &item.to_le_bytes());
    }
    setup(&mut emu);
    EVENTS.lock().unwrap().clear();
    let mut frames = Vec::new();
    let mut stopped = None;
    let mut sp_before = sp_list(&mut emu, w.player);
    for f in 0..c.frames {
        let arms: &[u32] = if f < c.hold { std::slice::from_ref(&c.arm) } else { &[] };
        press(&mut emu, w.stick, &ids, arms);
        ANIMS.lock().unwrap().clear();
        proxy::flush_tb(&emu);
        let r = emu.call(CHR_FRAME, &[wcm, 0, w.player], &[0.0, DT]);
        let evs: Vec<Value> = std::mem::take(&mut *EVENTS.lock().unwrap());
        let sp = sp_list(&mut emu, w.player);
        let added: Vec<i32> = sp.iter().filter(|i| !sp_before.contains(i)).copied().collect();
        let gone: Vec<i32> = sp_before.iter().filter(|i| !sp.contains(i)).copied().collect();
        sp_before = sp;
        let a = anim(&mut emu, w.player);
        let h = hp(&mut emu, w.player);
        let p = pos(&mut emu, w.player);
        let anims: Vec<i64> = ANIMS.lock().unwrap().clone();
        if !evs.is_empty() || !added.is_empty() || !gone.is_empty() {
            println!("  f{f} anim {a} tae {:?} hp {h} sp +{added:?} -{gone:?} {}", ANIMS.lock().unwrap(), serde_json::to_string(&evs).unwrap());
        }
        frames.push(json!({"anim": a, "tae": anims, "hp": h, "pos": p, "sp_added": added, "sp_gone": gone, "events": evs, "quick": i32::from_le_bytes(emu.peek(w.player + 0x11c0)), "in_use": i32::from_le_bytes(emu.peek(w.player + 0x11c4))}));
        if r.stopped.is_some() {
            stopped = r.stopped.map(|(a, e)| format!("{a:#x} error {e}"));
            break;
        }
    }
    let inv_after = inventory(&mut emu, w.player);
    let counts = |v: &[(u32, u32, i32, u64)]| v.iter().filter(|e| e.1 >> 28 == 4).map(|e| json!([e.1 & 0xfff_ffff, e.2])).collect::<Vec<_>>();
    let anims: std::collections::BTreeSet<i64> = frames.iter().map(|f| f["anim"].as_i64().unwrap()).collect();
    println!("{} ({} frames, {:.0} s): anims {anims:?}, stopped {stopped:?}", c.name, frames.len(), t.elapsed().as_secs_f32());
    let v = json!({
        "case": c.name, "goods": c.goods, "count": c.count, "arm": c.arm, "hold": c.hold, "dt": DT,
        "hp_max": i32::from_le_bytes({ let bag = emu.peek_u64(w.player + 0x1ff8); let d = emu.peek_u64(bag + 0x18); emu.peek(d + 0x134) }),
        "goods_before": counts(&inv), "goods_after": counts(&inv_after),
        "frames": frames, "stopped": stopped,
    });
    std::fs::write(out.join(format!("items_{}.json", c.name)), serde_json::to_string(&v).unwrap()).unwrap();
}

pub fn run(new_emu: &dyn Fn() -> Emu, out: &Path, want: &[&str]) {
    let names: Vec<&str> = want.iter().filter_map(|w| w.strip_prefix("items_")).collect();
    for c in CASES {
        if (names.is_empty() && c.name != "probe") || names.contains(&c.name) {
            run_case(new_emu, out, c);
        }
    }
}
