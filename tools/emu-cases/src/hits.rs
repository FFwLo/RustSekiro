//! `hits`: landed hits in whole world frames. acttrace's duels issued attacks (FUN_1409a1290) but
//! nothing resolved: the hit manager runs in MoveMapStep's task FUN_140a746f0 (task group 0x4b),
//! not in the four WorldChrMan tasks. Its step FUN_1409a0dc0(DAT_143d77ef0, dt) updates the attack
//! records (FUN_14099c530; list head +0, next +0x2a0), tests their shapes (FUN_14099b3f0, results
//! +0x210 .. +0x218 read by FUN_14099b0e0, queued by FUN_14099cbe0) and delivers the queued hits
//! (FUN_1409a1bc0 -> FUN_1409956d0 -> the defender's damage module -> FUN_140b6f690: resolve
//! FUN_140b6c880, apply FUN_140b6e6a0, hit stops FUN_140b6e5a0).
//! Each duel frame here runs: the hit manager step, the WorldChrMan pre tasks, the General's chr
//! frame FUN_140a4b570, Wolf's, the post tasks.
//!
//! `hits [hits_<name>|hits_<prefix>*...]` writes extracted/emu/hits_<name>.json: per frame both
//! chrs' HP, posture, hit-stop module, clock, TAE clips with their times, position, and every hit
//! the resolve saw (the hit record at the resolve's entry and after the apply). Results:
//! docs/kb/damage.md "Landed hits".

use std::collections::BTreeMap;
use std::ffi::c_void;
use std::path::Path;
use std::sync::Mutex;

use serde_json::{json, Value};

use super::acttrace::{self, Input, ATTACK, CHR_FRAME, DT, GUARD, TASKS_POST, TASKS_PRE, WORLD_CHR_MAN};
use super::{proxy, turn, uc, Emu};

/// The hit manager (DAT_143d77ef0) and its per-frame step (rcx manager, xmm1 dt).
pub(super) const HIT_MANAGER: u64 = 0x1_43d7_7ef0;
pub(super) const HIT_STEP: u64 = 0x1_409a_0dc0;
/// The defender's resolve (rcx damage module, rdx attacker, r8 hit record), apply, and the hit
/// stops FUN_140b6e5a0 (same arguments; the record's last read point after the apply).
pub(super) const RESOLVE: u64 = 0x1_40b6_c880;
const APPLY: u64 = 0x1_40b6_e6a0;
pub(super) const AFTER_APPLY: u64 = 0x1_40b6_e5a0;
/// Hit-stop start SprjChrHitStopModule FUN_140b9dad0 (rcx module, xmm1 seconds).
const STOP_START: u64 = 0x1_40b9_dad0;
/// TAE update of the selected clip (rcx anim id, xmm1 / xmm2 the clip time before / after).
const TAE_UPDATE: u64 = 0x1_40b5_c920;
/// SpEffect add FUN_1409e9c00 (chr, id, source chr, 0, 0, rates, 1.0, ...), as speffect.rs.
const SP_ADD: u64 = 0x1_409e_9c00;
/// The General's deflect window SpEffect (flick defence 40; kb/guard-deflect.md).
const GENERAL_DEFLECT: u64 = 200220;
/// AtkParam lookup FUN_1410b5160 (out, id pair: the row at out +0x10), as FUN_140b6e5a0 calls it.
const ATK_ROW: u64 = 0x1_410b_5160;
/// Bytes of the hit record kept (it lives on the delivering function's stack).
pub(super) const HIT_SIZE: usize = 0x250;

/// Functions counted per frame (entered via the block hook's `starts`).
const WATCH: [(u64, &str); 9] = [
    (0x1_409a_1290, "attack_issue"),
    (0x1_4099_c530, "record_update"),
    (0x1_4099_b3f0, "shape_test"),
    (0x1_4099_b0e0, "shape_results"),
    (0x1_4099_cbe0, "hit_queue"),
    (0x1_4099_56d0, "deliver"),
    (RESOLVE, "resolve"),
    (APPLY, "apply"),
    (STOP_START, "stop_start"),
];

/// What the hooks saw this frame: per hit (defender damage module, attacker, record at the
/// resolve's entry), the record after the apply; (module, seconds) at each hit-stop start; TAE
/// clips (id, time before, after).
#[derive(Default)]
struct Seen {
    resolves: Vec<(u64, u64, Vec<u8>)>,
    applied: Vec<Vec<u8>>,
    stops: Vec<(u64, f32)>,
    anims: Vec<(i64, f32, f32)>,
}
static SEEN: Mutex<Seen> = Mutex::new(Seen { resolves: Vec::new(), applied: Vec::new(), stops: Vec::new(), anims: Vec::new() });

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

fn read_hit(uc: uc::Engine, at: u64) -> Vec<u8> {
    let mut b = vec![0u8; HIT_SIZE];
    unsafe { uc::uc_mem_read(uc, at, b.as_mut_ptr() as _, HIT_SIZE) };
    b
}

extern "C" fn on_resolve(uc: uc::Engine, _addr: u64, _size: u32, _data: *mut c_void) {
    let hit = read_hit(uc, reg(uc, uc::R8));
    SEEN.lock().unwrap().resolves.push((reg(uc, uc::RCX), reg(uc, uc::RDX), hit));
}

extern "C" fn on_after(uc: uc::Engine, _addr: u64, _size: u32, _data: *mut c_void) {
    let hit = read_hit(uc, reg(uc, uc::R8));
    SEEN.lock().unwrap().applied.push(hit);
}

extern "C" fn on_stop(uc: uc::Engine, _addr: u64, _size: u32, _data: *mut c_void) {
    SEEN.lock().unwrap().stops.push((reg(uc, uc::RCX), xmm(uc, 1)));
}

extern "C" fn on_tae(uc: uc::Engine, _addr: u64, _size: u32, _data: *mut c_void) {
    SEEN.lock().unwrap().anims.push((reg(uc, uc::RCX) as u32 as i32 as i64, xmm(uc, 1), xmm(uc, 2)));
}

fn i32at(emu: &mut Emu, at: u64) -> i32 {
    i32::from_le_bytes(emu.peek(at))
}
fn f32at(emu: &mut Emu, at: u64) -> f32 {
    f32::from_le_bytes(emu.peek(at))
}
pub(super) fn module(emu: &mut Emu, chr: u64, off: u64) -> u64 {
    let bag = emu.peek_u64(chr + 0x1ff8);
    emu.peek_u64(bag + off)
}

/// One chr's state after a frame: data module HP +0x130 / posture +0x148, hit-stop module
/// [bag+0x90] (+0x10 timer, +0x14 length, +0x20 state, +0x34 the last start's time, +0x2e..+0x31
/// started as defender / as attacker / +0x30 / forced), chr +0x108 the frame's clock,
/// module0 +0x5c (the reaction the damage side asked for), physics pos / yaw.
pub(super) fn chr_state(emu: &mut Emu, chr: u64) -> Value {
    let data = module(emu, chr, 0x18);
    let hs = module(emu, chr, 0x90);
    let m0 = module(emu, chr, 0);
    let ph = module(emu, chr, 0x68);
    json!({
        "hp": i32at(emu, data + 0x130), "posture": i32at(emu, data + 0x148), "posture_max": i32at(emu, data + 0x14c),
        "stop": [f32at(emu, hs + 0x10), f32at(emu, hs + 0x14), emu.peek::<1>(hs + 0x20)[0] as f32, f32at(emu, hs + 0x34)],
        "stop_flags": emu.peek::<4>(hs + 0x2e),
        "clock": f32at(emu, chr + 0x108),
        "m0_5c": i32at(emu, m0 + 0x5c),
        "pos": acttrace::pos(emu, chr), "yaw": f32at(emu, ph + 0x74),
    })
}

/// A hit record's fields (guard.rs): +0..+0x10 the five amounts, +0x40 attack flick, +0x50 /
/// +0x54 the AtkParam id pair FUN_1410b5160 looks up, +0x164 guarded, +0x1c7 deflected, +0x1c4
/// guard break, +0x1c5, +0x1c6 broken, +0x68 knockback, +0x1e0 HP damage, +0x1e4 posture damage,
/// +0x150 / +0x158 attack direction, +0x160 body part, +0x231 flags.
pub(super) fn hit_fields(b: &[u8]) -> Value {
    let f = |o: usize| f32::from_le_bytes(b[o..o + 4].try_into().unwrap());
    let i = |o: usize| i32::from_le_bytes(b[o..o + 4].try_into().unwrap());
    json!({
        "amounts": [f(0), f(4), f(8), f(0xc), f(0x10)], "flick_atk": i(0x40), "atk_ids": [i(0x50), i(0x54)],
        "guarded": b[0x164], "deflected": b[0x1c7],
        "guard_break": b[0x1c4], "guard_break_hit": b[0x1c5], "broken": b[0x1c6],
        "knockback": f(0x68), "hp_dmg": i(0x1e0), "posture_dmg": i(0x1e4),
        "dir": [f(0x150), f(0x158)], "part": i(0x160), "flags_231": b[0x231],
    })
}

/// The changed 4-byte words of the record from the resolve's entry to after the apply.
fn hit_diff(a: &[u8], b: &[u8]) -> Vec<String> {
    (0..HIT_SIZE / 4)
        .filter(|k| a[4 * k..4 * k + 4] != b[4 * k..4 * k + 4])
        .map(|k| {
            let v = u32::from_le_bytes(b[4 * k..4 * k + 4].try_into().unwrap());
            format!("{:#x}={}/{:.4}", 4 * k, v as i32, f32::from_bits(v))
        })
        .collect()
}

/// Wolf's pad in a duel.
#[derive(Clone, Copy)]
enum Pad {
    Idle,
    /// Guard held from frame 0.
    Guard,
    /// Guard pressed for 2 frames from this frame.
    Tap(usize),
    /// Attack pressed for 3 frames from this frame.
    Attack(usize),
}

/// The General: AI on with the attack behaviour forced on frames 0-2 (`Attack`), or AI off
/// (`Stand`), optionally guarding (ShieldBlock forced each frame before the hit step) and with
/// his deflect SpEffect 200220 added on a frame through the game's add FUN_1409e9c00.
#[derive(Clone, Copy)]
enum Foe {
    Attack(i32),
    Stand { guard: bool, deflect_at: Option<usize> },
}

/// A duel: Wolf's pad, the General, the distance between them, Wolf's posture preset.
pub(crate) struct Duel {
    pub(crate) name: String,
    frames: usize,
    pad: Pad,
    foe: Foe,
    dist: f32,
    wolf_posture: Option<i32>,
}

/// The General's slash (a000_003000) reaches Wolf on frame 42 with Wolf idle (hits_g_open).
const CONTACT: usize = 42;

pub(crate) fn duels() -> Vec<Duel> {
    let d = |name: &str, frames, pad, foe, dist, wolf_posture| Duel { name: name.into(), frames, pad, foe, dist, wolf_posture };
    let stand = Foe::Stand { guard: false, deflect_at: None };
    let mut v = vec![
        d("g_open", 75, Pad::Idle, Foe::Attack(3000), 2.5, None),
        d("g_guard", 75, Pad::Guard, Foe::Attack(3000), 2.5, None),
        d("g_guard_nocancel", 60, Pad::Guard, Foe::Attack(3000), 2.5, None),
        d("g_break", 75, Pad::Guard, Foe::Attack(3000), 2.5, Some(40)),
        d("g_open_low", 75, Pad::Idle, Foe::Attack(3000), 2.5, Some(40)),
        d("w_open", 60, Pad::Attack(0), stand, 2.0, None),
        d("w_guard", 60, Pad::Attack(0), Foe::Stand { guard: true, deflect_at: None }, 2.0, None),
        d("w_deflect", 60, Pad::Attack(0), Foe::Stand { guard: true, deflect_at: Some(26) }, 2.0, None),
    ];
    // Guard taps k frames before the contact frame: the deflect window's edges.
    // Row E3: the slash with a hit stop (attacker 0.1 s, defender 0.15 s), the General's frame
    // first (he is the attacker) or Wolf's first.
    for name in ["stop_g_open", "stop_g_open_wfirst", "stop_g_guard", "stop_g_guard_wfirst"] {
        let pad = if name.contains("guard") { Pad::Guard } else { Pad::Idle };
        v.push(d(name, 75, pad, Foe::Attack(3000), 2.5, None));
    }
    for k in [1, 2, 3, 4, 6, 8, 10, 12, 14, 15, 16, 20, 24, 28] {
        v.push(d(&format!("g_tap{k}"), 60, Pad::Tap(CONTACT - k), Foe::Attack(3000), 2.5, None));
    }
    v
}

const IDLE: Input = Input { arms: &[], stick: [0.0, 0.0] };
const fn arms(a: &'static [u32]) -> Input {
    Input { arms: a, stick: [0.0, 0.0] }
}

fn pad(p: Pad, f: usize) -> Input {
    match p {
        Pad::Idle => IDLE,
        Pad::Guard => arms(&[GUARD]),
        Pad::Tap(at) if (at..at + 2).contains(&f) => arms(&[GUARD]),
        Pad::Attack(at) if (at..at + 3).contains(&f) => arms(&[ATTACK]),
        _ => IDLE,
    }
}

pub(crate) fn duel(new_emu: &dyn Fn() -> Emu, out: &Path, d: &Duel) {
    let t = std::time::Instant::now();
    let mut emu = new_emu();
    let w = turn::wolf(&mut emu);
    let ids = acttrace::arm_inputs(&mut emu);
    let g = acttrace::general(&mut emu);
    let ai = acttrace::ai_of(&mut emu, g);
    let wcm = emu.peek_u64(WORLD_CHR_MAN);
    let mgr = emu.peek_u64(HIT_MANAGER);
    // As acttrace's start_trace: CRT ISA level 1, the plain longjmp, no menu open.
    emu.limit = 500_000_000;
    emu.raw_write(0x1_43c1_d520, &1u32.to_le_bytes());
    emu.raw_write(proxy::LONGJMP, proxy::PLAIN_LONGJMP);
    emu.raw_write(acttrace::MENU_STATE, &[0x31, 0xc0, 0xc3]);
    let menus = emu.call(0x1_40d8_72c0, &[], &[]).rax;
    if menus != 0 {
        emu.u64(menus + 0x100, 0);
    }
    // Attacking: the AI on (the forced request goes through its manipulator). Standing: AI off.
    if let Foe::Attack(_) = d.foe {
        emu.raw_write(0x1_43d7_a376, &[0]);
    }
    emu.hook(uc::HOOK_CODE, acttrace::on_request_copy as *const c_void, acttrace::AI_REQUEST_COPY, acttrace::AI_REQUEST_COPY);
    acttrace::every_frame(&mut emu, g);
    emu.hook(uc::HOOK_CODE, on_resolve as *const c_void, RESOLVE, RESOLVE);
    emu.hook(uc::HOOK_CODE, on_after as *const c_void, AFTER_APPLY, AFTER_APPLY);
    emu.hook(uc::HOOK_CODE, on_stop as *const c_void, STOP_START, STOP_START);
    emu.hook(uc::HOOK_CODE, on_tae as *const c_void, TAE_UPDATE, TAE_UPDATE);
    emu.st.starts = WATCH.iter().map(|w| w.0).collect();
    // Wolf's HP raised so a hit is not lethal (the General's slash takes all of the snapshot's 548).
    let wdata = module(&mut emu, w.player, 0x18);
    emu.raw_write(wdata + 0x130, &5000i32.to_le_bytes());
    emu.raw_write(wdata + 0x134, &5000i32.to_le_bytes());
    if d.name.ends_with("_nocancel") {
        // The slash's AtkParam_Npc row (FUN_1410b5160(out, 0, id): out +0x10) with its
        // guardCutCancelRate (+0x7c, s8, -30) set to 0 in the emulator's copy.
        let out = emu.alloc(0x20);
        emu.write(out, &[0xff, 0xff, 0xff, 0xff, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        let r = emu.call(ATK_ROW, &[out, 0, 10200100], &[]);
        let row = emu.peek_u64(out + 0x10);
        assert!(r.stopped.is_none() && row != 0, "AtkParam row lookup {:?}", r.stopped);
        println!("  AtkParam 10200100 row {row:#x}: guardCutCancelRate {}", emu.peek::<1>(row + 0x7c)[0] as i8);
        emu.raw_write(row + 0x7c, &[0]);
    }
    if d.name.starts_with("stop_") {
        // Row E3: the slash's AtkParam_Npc row given hitStopTime (+0x1c, which the attacker's start
        // FUN_140b9cd90 reads) 0.1 and hitStopTime_Defencer (+0x1cc, FUN_140b9cfb0) 0.15 in the
        // emulator's copy (the General's rows have none): which frame each side's clock stops on.
        let out = emu.alloc(0x20);
        emu.write(out, &[0xff, 0xff, 0xff, 0xff, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        let r = emu.call(ATK_ROW, &[out, 0, 10200100], &[]);
        let row = emu.peek_u64(out + 0x10);
        assert!(r.stopped.is_none() && row != 0, "AtkParam row lookup {:?}", r.stopped);
        println!("  AtkParam 10200100 row {row:#x}: hitStopTime {} _Defencer {}", f32at(&mut emu, row + 0x1c), f32at(&mut emu, row + 0x1cc));
        emu.raw_write(row + 0x1c, &0.1f32.to_le_bytes());
        emu.raw_write(row + 0x1cc, &0.15f32.to_le_bytes());
    }
    if let Some(p) = d.wolf_posture {
        emu.raw_write(wdata + 0x148, &p.to_le_bytes());
    }
    // The defender stays where the snapshot has him (his hurt shapes, the ragdoll bodies, are not
    // moved by the proxy; the world's physics step that would carry them is not run) and the
    // attacker is put `dist` m in front of him, facing him (forward = (-sin yaw, 0, -cos yaw)).
    let (def, att) = match d.foe {
        Foe::Attack(_) => (w.player, g),
        Foe::Stand { .. } => (g, w.player),
    };
    let dp = acttrace::pos(&mut emu, def);
    let dphys = module(&mut emu, def, 0x68);
    let yaw = f32at(&mut emu, dphys + 0x74);
    let at = [dp[0] - d.dist * yaw.sin(), dp[1], dp[2] - d.dist * yaw.cos()];
    let aphys = module(&mut emu, att, 0x68);
    let (wrapper, _) = proxy::proxy_of(&mut emu, aphys);
    proxy::place(&mut emu, aphys, wrapper, at);
    emu.poke_f32s(aphys + 0x74, &[yaw + std::f32::consts::PI]);
    let ones = emu.alloc(0x10);
    emu.poke_f32s(ones, &[1.0; 4]);
    let gm0 = module(&mut emu, g, 0);
    let blk = emu.alloc(0x40);
    emu.poke_f32s(blk + 8, &[DT]);
    let (wdmod, whs) = (module(&mut emu, w.player, 0x98), module(&mut emu, w.player, 0x90));
    println!("hits {}: Wolf {:#x} General {:#x} hit manager {mgr:#x}", d.name, w.player, g);
    let mut frames = Vec::new();
    let mut since: Option<usize> = None;
    let mut stopped = None;
    let dt_bits = DT.to_bits() as u64;
    for f in 0..d.frames {
        acttrace::press(&mut emu, w.stick, &ids, &pad(d.pad, f));
        let req = match d.foe {
            Foe::Attack(b) if f < 3 => b,
            _ => -1,
        };
        *acttrace::FORCE.lock().unwrap() = (ai, req);
        if let Foe::Stand { guard, deflect_at } = d.foe {
            // Guard: TAE ChrActionFlag 3 ShieldBlock = module0 +0x70 bit 2, +0xb08 its judge (900).
            if guard {
                let flags = emu.peek_u64(gm0 + 0x70) | 4;
                emu.raw_write(gm0 + 0x70, &flags.to_le_bytes());
                emu.raw_write(gm0 + 0xb08, &900u32.to_le_bytes());
            }
            if deflect_at == Some(f) {
                let r = emu.call(SP_ADD, &[g, GENERAL_DEFLECT, g, 0, 0, ones, 0x3f80_0000, 0, 0, 0], &[]);
                assert!(r.stopped.is_none(), "SpEffect add stopped {:?}", r.stopped);
            }
        }
        proxy::flush_tb(&emu);
        *SEEN.lock().unwrap() = Seen::default();
        emu.st.entered.clear();
        let mut stop = emu.call(HIT_STEP, &[mgr, dt_bits], &[0.0, DT]).stopped;
        for task in TASKS_PRE {
            stop = stop.or(emu.call(task, &[wcm, blk], &[]).stopped);
        }
        SEEN.lock().unwrap().anims.clear();
        // Hits resolved (and stops started) by the hit step, before any chr frame.
        let (step_hits, step_stops) = {
            let s = SEEN.lock().unwrap();
            (s.resolves.len(), s.stops.len())
        };
        // `_wfirst`: Wolf's chr frame before the General's (row E3: whether the order matters).
        let (g_anims, w_anims) = if d.name.ends_with("_wfirst") {
            stop = stop.or(emu.call(CHR_FRAME, &[wcm, 0, w.player], &[0.0, DT]).stopped);
            let w_anims = std::mem::take(&mut SEEN.lock().unwrap().anims);
            stop = stop.or(emu.call(CHR_FRAME, &[wcm, 0, g], &[0.0, DT]).stopped);
            (std::mem::take(&mut SEEN.lock().unwrap().anims), w_anims)
        } else {
            stop = stop.or(emu.call(CHR_FRAME, &[wcm, 0, g], &[0.0, DT]).stopped);
            let g_anims = std::mem::take(&mut SEEN.lock().unwrap().anims);
            stop = stop.or(emu.call(CHR_FRAME, &[wcm, 0, w.player], &[0.0, DT]).stopped);
            (g_anims, std::mem::take(&mut SEEN.lock().unwrap().anims))
        };
        for task in TASKS_POST {
            stop = stop.or(emu.call(task, &[wcm, blk], &[]).stopped);
        }
        let seen = std::mem::take(&mut *SEEN.lock().unwrap());
        let mut hits = Vec::new();
        for (k, (dmod, att, before)) in seen.resolves.iter().enumerate() {
            let after = seen.applied.get(k).cloned().unwrap_or_default();
            let mut h = if after.len() == HIT_SIZE { hit_fields(&after) } else { json!({}) };
            h["in"] = hit_fields(before);
            if after.len() == HIT_SIZE {
                h["changed"] = json!(hit_diff(before, &after));
            }
            h["defender"] = json!(if *dmod == wdmod { "wolf" } else { "general" });
            h["attacker"] = json!(if *att == w.player { "wolf" } else if *att == g { "general" } else { "other" });
            hits.push(h);
        }
        let stops: Vec<Value> = seen.stops.iter().map(|&(m, s)| json!({"chr": if m == whs { "wolf" } else { "general" }, "seconds": s})).collect();
        let counts: BTreeMap<&str, u64> = WATCH.iter().filter_map(|(a, n)| emu.st.entered.get(a).map(|&c| (*n, c))).collect();
        let (mut ws, mut gs) = (chr_state(&mut emu, w.player), chr_state(&mut emu, g));
        ws["tae"] = json!(w_anims);
        gs["tae"] = json!(g_anims);
        if since.is_none() && g_anims.iter().any(|a| (3000..4000).contains(&(a.0 % 1_000_000))) {
            since = Some(f);
        }
        let head = emu.peek_u64(mgr);
        let clips = |a: &[(i64, f32, f32)]| a.iter().filter(|c| !(790000..800000).contains(&c.0)).map(|c| format!("{}@{:.3}", c.0, c.2)).collect::<Vec<_>>();
        if !hits.is_empty() || f % 10 == 0 || !stops.is_empty() {
            println!("  f{f} wolf {} {} {:?} | general {} {} {:?} | records {} {counts:?} stops {stops:?}", ws["hp"], ws["posture"], clips(&w_anims), gs["hp"], gs["posture"], clips(&g_anims), head != 0);
            for h in &hits {
                println!("    hit {} -> {}: guarded {} deflected {} hp {} posture {} knockback {} break {} broken {}", h["attacker"], h["defender"], h["guarded"], h["deflected"], h["hp_dmg"], h["posture_dmg"], h["knockback"], h["guard_break"], h["broken"]);
            }
        }
        frames.push(json!({"f": f, "wolf": ws, "general": gs, "hits": hits, "stops": stops, "counts": counts, "records": head != 0,
            "step_hits": step_hits, "step_stops": step_stops}));
        if stop.is_some() {
            stopped = stop.map(|(a, e)| format!("{a:#x} error {e}"));
            println!("  stopped {stopped:?} trail {:x?}", emu.st.trail);
            break;
        }
    }
    println!("hits {} ({} frames, attack anim from {since:?}, {:.0} s)", d.name, frames.len(), t.elapsed().as_secs_f32());
    let foe = match d.foe {
        Foe::Attack(b) => json!({"attack": b}),
        Foe::Stand { guard, deflect_at } => json!({"guard": guard, "deflect_at": deflect_at}),
    };
    let pad = match d.pad {
        Pad::Idle => json!("idle"),
        Pad::Guard => json!("guard"),
        Pad::Tap(at) => json!({"tap": at}),
        Pad::Attack(at) => json!({"attack": at}),
    };
    let v = json!({"name": d.name, "dt": DT, "pad": pad, "foe": foe, "dist": d.dist, "wolf_posture": d.wolf_posture, "attack_from": since, "frames": frames, "stopped": stopped});
    std::fs::write(out.join(format!("hits_{}.json", d.name)), serde_json::to_string(&v).unwrap()).unwrap();
}

/// `hits` runs every duel; `hits_<name>` one; `hits_<prefix>*` those starting with it.
pub fn run(new_emu: &dyn Fn() -> Emu, out: &Path, want: &[&str]) {
    let names: Vec<&str> = want.iter().filter_map(|w| w.strip_prefix("hits_")).collect();
    for d in duels() {
        let on = names.is_empty() || names.iter().any(|n| n.strip_suffix('*').map_or(*n == d.name, |p| d.name.starts_with(p)));
        if on {
            duel(new_emu, out, &d);
        }
    }
}
