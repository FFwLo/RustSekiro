//! sv1_rec_hooks.dll: me3 native for the real Sekiro (1.06, profile me3/sekiro-rec.me3). Hooks a few game
//! functions at entry (ilhook jmp-back: the original runs unchanged) and logs their arguments for the
//! memscope recorder tools/memscope/rec2.lua.
//!
//! Hand-off with the recorder through exported variables (rec2.lua finds them in the export table):
//!   SV1REC_DIR   recording folder (NUL-terminated) - written by rec2.lua before it sets SV1REC_ON
//!   SV1REC_ON    1 = log to <dir>\hooks.jsonl, 0 = stop and close the file
//!   SV1REC_TICK  the recorder's current tick, stamped on every logged call
//!   SV1REC_COUNT calls logged so far (diagnostic)
//! Which hooks: sv1_rec_hooks.txt next to the DLL (one name per line, `#` comments); default set below.
//! Install result: sv1_rec_hooks.log next to the DLL.
use std::ffi::c_void;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use ilhook::x64::{hook_closure_jmp_back, CallbackOption, HookFlags, Registers};

#[no_mangle]
pub static SV1REC_ON: AtomicU32 = AtomicU32::new(0);
#[no_mangle]
pub static SV1REC_TICK: AtomicU32 = AtomicU32::new(0);
#[no_mangle]
pub static SV1REC_COUNT: AtomicU32 = AtomicU32::new(0);
#[no_mangle]
pub static mut SV1REC_DIR: [u8; 512] = [0; 512];

/// (name, address, what the arguments are). Addresses: Sekiro 1.06 with base 0x140000000.
/// Expected first bytes (read live 2026-10-15) guard against another game version / still-packed code.
const HOOKS: &[(&str, usize, &str, [u8; 6])] = &[
    // SekiroTool Offsets.cs Functions.AdjustItemCount (1.5/1.6 0x79AB70): (inventory, item id, delta, ...)
    ("items", 0x1_4079_ab70, "rcx inventory, rdx item id, r8 delta", [0x48, 0x8B, 0xC4, 0x48, 0x89, 0x58]),
    // SekiroTool Hooks.AddSubGoal (0x5CE510): (goal list, goal id, life, ...)
    ("ai_goal", 0x1_405c_e510, "rcx goals, rdx goal id, r8 life", [0x48, 0x8B, 0xC4, 0x55, 0x56, 0x57]),
    // SekiroTool Hooks.StartMusic (0x8CEBE0)
    ("music", 0x1_408c_ebe0, "rcx this, rdx id", [0x40, 0x57, 0x48, 0x83, 0xEC, 0x30]),
    // FUN_1408439a0 defender posture damage (docs/kb/guard-deflect.md); memscope post hook crashed 2026-10-08
    ("posture", 0x1_4084_39a0, "posture damage", [0x48, 0x89, 0x5C, 0x24, 0x08, 0x48]),
    // FUN_140840870 guard posture cut; memscope pre hook crashed 2026-10-08
    ("guardcut", 0x1_4084_0870, "guard cut", [0x48, 0x8B, 0xC4, 0x48, 0x89, 0x58]),
    // FUN_1407dab40 turn controller (memscope pre hook OK 2026-10-15); ~22 calls/s, off by default
    ("turn", 0x1_407d_ab40, "turn controller", [0x48, 0x89, 0x5C, 0x24, 0x18, 0x48]),
    // FUN_140995470 hit funnel (hit object, target): dispatches on the type of [hit +0x198] to
    // FUN_1409e5fd0 (chr hits: position +0x130, ids +0x50 / +0x54). Logs a blob of the hit object.
    ("hit", 0x1_4099_5470, "rcx hit object, rdx target", [0x48, 0x89, 0x5C, 0x24, 0x08, 0x48]),
    // SprjChrTaeAnimEvent vtable 0x142a7ae70 slots 2 and 3 (event dispatchers, rdx event: +0 type,
    // +8 args, +0x18 flag; chr at [this +0x10]). Called while an event is active: merged into one
    // line per event with start / end ticks.
    ("tae", 0x1_40b5_9170, "rcx SprjChrTaeAnimEvent, rdx event", [0x40, 0x53, 0x48, 0x81, 0xEC, 0x90]),
    ("tae", 0x1_40b5_a6d0, "rcx SprjChrTaeAnimEvent, rdx event", [0x48, 0x83, 0x7A, 0x08, 0x00, 0x4C]),
    // FUN_140b1a960 bullet create (manager, out, request): bullet param id at request +4
    // (looked up with FUN_140b123c0), pool slot *manager + idx * 0xd30
    ("bullet", 0x1_40b1_a960, "rcx manager, rdx out, r8 request", [0x40, 0x55, 0x56, 0x57, 0x41, 0x54]),
    // FUN_14092afe0 FXHGSfxCtrl create (out, sfx system, ffx id, transform: +0 position, ...): 20 callers,
    // e.g. ChrSingleOneshotSfxSlot FUN_1409dd4f0 and ChrSingleSfxSlot FUN_1409deb60. (FUN_1409d1280,
    // the TAE FFX path, logged nothing in rec_20261015_201915: FFX TAE types never reach FUN_140b59170.)
    ("vfx", 0x1_4092_afe0, "rcx out, rdx sfx system, r8d ffx id, r9 transform", [0x40, 0x55, 0x53, 0x56, 0x57, 0x41]),
    // FUN_1409cf930 sound emit on a chr's emitter list (list, sound type, sound id, ?), reached from
    // the TAE sound handlers (DefaultSeEmitCondition) through FUN_141116400
    ("sound", 0x1_409c_f930, "rcx emitter list, edx type, r8d id, r9d", [0x48, 0x89, 0x5C, 0x24, 0x08, 0x48]),
    // SekiroTool Hooks.LockedTarget (1.06 0x9C3ADA, mid-function `mov rax,[rax+0x1FF8]`): rax = the
    // locked-on ChrIns. Logged only when it changes.
    ("lock", 0x1_409c_3ada, "rax locked-on ChrIns", [0x48, 0x8B, 0x80, 0xF8, 0x1F, 0x00]),
    // FUN_1413f92c0 = HKS hkbFireEvent (registered by FUN_1413f78c0): event name = Lua arg 1
    ("hks", 0x1_413f_92c0, "rcx lua state", [0x48, 0x89, 0x5C, 0x24, 0x08, 0x48]),
    // FUN_140900d10 EMEVD command dispatcher (called by SprjEmkEventIns update FUN_14065dd20):
    // r8 event instance: +0x28 event id, +0x30 slot, +0xb0 instruction (+0 bank, +4 id, +8 arg size,
    // +0x10 arg offset), +0xb8 substituted args or 0, +0xa8 -> [+8] emevd file (+0x78 args base).
    // Only action banks (>= 2000) are logged: conditions are re-checked every frame.
    ("emevd", 0x1_4090_0d10, "rcx system, r8 event instance", [0x40, 0x53, 0x56, 0x57, 0x48, 0x81]),
    // SekiroTool Functions.SetEvent (1.06 0x6C4520): (EventFlagMan, flag id, value, ?). Logged when
    // a flag's value changes.
    // FUN_1407fa680 throw start (ThrowMan, thrower node, victim node, throw id, ...): sets throw state
    // 1 / 2 (docs/kb/enemy.md "Grab escape"); node -> [ChrIns] +8 handle. Grabs and deathblows.
    ("throw", 0x1_407f_a680, "rcx ThrowMan, rdx thrower, r8 victim, r9d throw id", [0x40, 0x55, 0x56, 0x57, 0x41, 0x54]),
    ("flag", 0x1_406c_4520, "rcx EventFlagMan, edx flag, r8b value", [0x40, 0x55, 0x41, 0x54, 0x41, 0x55]),
];
const DEFAULT_ON: &[&str] = &["items", "ai_goal", "music", "posture", "guardcut"];

/// hksi_lua_tolstring (FUN_14147d940, called by hksi_luaL_checklstring FUN_1414394c0): no error on
/// a non-string, returns null
const HKS_TOLSTRING: usize = 0x1_4147_d940;

struct Call {
    tick: u32,
    hook: u8,
    tid: u32,
    r: [u64; 4],
    f: [f32; 4],
    ret: u64,
    s5: u64,
    s6: u64,
    /// items hook: (goods id with type nibble, count before) read from the slot table
    item: Option<(u32, u32)>,
    /// extra JSON fields (`,"key":value...`) of the newer hooks
    extra: String,
}

/// A TAE event seen in consecutive ticks, written once it stops (key: dispatcher this + event).
struct Tae {
    start: u32,
    last: u32,
    tid: u32,
    extra: String,
}

static TAE: Mutex<Option<std::collections::HashMap<(u64, u64), Tae>>> = Mutex::new(None);
static TAE_DONE: Mutex<Vec<Tae>> = Mutex::new(Vec::new());
static LAST_LOCK: AtomicU64 = AtomicU64::new(u64::MAX);
static FLAGS: Mutex<Option<std::collections::HashMap<u32, u8>>> = Mutex::new(None);

/// Plausible user-mode pointer (the hooked code itself reads these).
fn ptr_ok(p: u64) -> bool {
    (0x10000..0x7FFF_FFFF_0000).contains(&p)
}

unsafe fn hex(p: u64, n: usize) -> String {
    if !ptr_ok(p) {
        return String::new();
    }
    std::slice::from_raw_parts(p as *const u8, n).iter().map(|x| format!("{x:02x}")).collect()
}

unsafe fn rd32(p: u64) -> u32 {
    if ptr_ok(p) { (p as *const u32).read_unaligned() } else { 0 }
}

unsafe fn rd64(p: u64) -> u64 {
    if ptr_ok(p) { (p as *const u64).read_unaligned() } else { 0 }
}

/// Fields of the newer hooks, read in the game thread at the hooked call. None = do not log.
unsafe fn extra_of(name: &str, reg: &Registers) -> Option<String> {
    Some(match name {
        "hit" => format!(",\"target\":{},\"target_handle\":{},\"owner\":{},\"blob\":\"{}\"",
            reg.rdx, rd32(reg.rdx + 8), rd64(reg.rcx + 0x198), hex(reg.rcx, 0x140)),
        "bullet" => format!(",\"bullet\":{},\"req\":\"{}\"", rd32(reg.r8 + 4) as i32, hex(reg.r8, 0x40)),
        "vfx" => format!(",\"ffx\":{},\"xf\":\"{}\"", reg.r8 as u32 as i32, hex(reg.r9, 0x40)),
        "sound" => format!(",\"sound_type\":{},\"sound\":{},\"emitters\":{}", reg.rdx as u32 as i32, reg.r8 as u32 as i32, reg.rcx),
        "lock" => {
            if LAST_LOCK.swap(reg.rax, Ordering::Relaxed) == reg.rax {
                return None;
            }
            format!(",\"target\":{},\"target_handle\":{}", reg.rax, rd32(reg.rax + 8))
        }
        "hks" => {
            let f: unsafe extern "win64" fn(u64, i32, *mut usize) -> *const u8 = std::mem::transmute(HKS_TOLSTRING);
            let p = f(reg.rcx, 1, std::ptr::null_mut());
            if p.is_null() {
                return None;
            }
            let ev = std::ffi::CStr::from_ptr(p as *const std::ffi::c_char).to_string_lossy();
            let ev: String = ev.chars().filter(|c| c.is_ascii_graphic() && *c != '"' && *c != '\\').take(80).collect();
            format!(",\"event\":\"{ev}\",\"lua\":{}", reg.rcx)
        }
        "emevd" => {
            let ins = rd64(reg.r8 + 0xb0);
            let bank = rd32(ins);
            if bank < 2000 {
                return None;
            }
            let size = (rd32(ins + 8) as usize).min(0x40);
            let mut args = rd64(reg.r8 + 0xb8);
            if args == 0 {
                let file = rd64(rd64(reg.r8 + 0xa8) + 8);
                let base = rd64(file + 0x78);
                args = if file != 0 && base != u64::MAX { file.wrapping_add(base).wrapping_add(rd64(ins + 0x10)) } else { 0 };
            }
            format!(",\"event\":{},\"slot\":{},\"bank\":{},\"cmd\":{},\"args\":\"{}\"",
                rd32(reg.r8 + 0x28), rd32(reg.r8 + 0x30), bank, rd32(ins + 4), hex(args, size))
        }
        "throw" => format!(",\"throw\":{},\"thrower\":{},\"victim\":{}",
            reg.r9 as u32 as i32, rd32(rd64(reg.rdx) + 8), rd32(rd64(reg.r8) + 8)),
        "flag" => {
            let (id, v) = (reg.rdx as u32, reg.r8 as u8);
            let Ok(mut m) = FLAGS.lock() else { return None };
            if m.get_or_insert_with(Default::default).insert(id, v) == Some(v) {
                return None;
            }
            format!(",\"flag\":{id},\"value\":{v}")
        }
        _ => String::new(),
    })
}

/// TAE dispatcher call: start a new event line or extend the running one.
unsafe fn on_tae(reg: &Registers, tick: u32) {
    let key = (reg.rcx, reg.rdx);
    let Ok(mut m) = TAE.lock() else { return };
    let m = m.get_or_insert_with(Default::default);
    if let Some(e) = m.get_mut(&key) {
        if tick <= e.last + 2 {
            e.last = tick;
            return;
        }
    }
    let chr = rd64(reg.rcx + 0x10);
    let extra = format!(",\"type\":{},\"chr\":{},\"handle\":{},\"args\":\"{}\"",
        rd32(reg.rdx), chr, rd32(chr + 8), hex(rd64(reg.rdx + 8), 0x30));
    if let Some(old) = m.insert(key, Tae { start: tick, last: tick, tid: GetCurrentThreadId(), extra }) {
        tae_done(old);
    }
}

fn tae_done(t: Tae) {
    if let Ok(mut d) = TAE_DONE.lock() {
        if d.len() < 200_000 {
            d.push(t);
        }
    }
}

/// Writer side: events not seen for 3 ticks (or all, when the recording stops) are finished.
fn tae_flush(tick: u32, all: bool) -> Vec<Tae> {
    if let Ok(mut m) = TAE.lock() {
        if let Some(m) = m.as_mut() {
            let ended: Vec<(u64, u64)> = m.iter().filter(|(_, e)| all || e.last + 3 < tick).map(|(k, _)| *k).collect();
            for k in ended {
                if let Some(e) = m.remove(&k) {
                    tae_done(e);
                }
            }
        }
    }
    std::mem::take(&mut *TAE_DONE.lock().unwrap())
}

/// Inventory slot -> (item id, count), as FUN_1407aabf0 does: table at inventory +0x168,
/// slots < [+0x24] at [+0x58] + slot * 0x10, the rest at [+0x48]; entry +0x4 id, +0x8 count.
unsafe fn item_of(inv: u64, slot: u32) -> Option<(u32, u32)> {
    if inv < 0x10000 {
        return None;
    }
    let l = (inv + 0x168) as *const u8;
    let max = (l.add(0x88) as *const i32).read_unaligned();
    if max < 0 || slot >= max as u32 + 1 {
        return None;
    }
    let split = (l.add(0x24) as *const u32).read_unaligned();
    let (base, idx) = if slot < split {
        ((l.add(0x58) as *const u64).read_unaligned(), slot)
    } else {
        ((l.add(0x48) as *const u64).read_unaligned(), slot - split)
    };
    if base < 0x10000 {
        return None;
    }
    let e = (base + idx as u64 * 0x10) as *const u32;
    Some((e.add(1).read_unaligned(), e.add(2).read_unaligned()))
}

static QUEUE: Mutex<Vec<Call>> = Mutex::new(Vec::new());

extern "system" {
    fn GetCurrentThreadId() -> u32;
    fn GetModuleHandleExW(flags: u32, name: *const c_void, module: *mut *mut c_void) -> i32;
    fn GetModuleFileNameW(module: *mut c_void, buf: *mut u16, size: u32) -> u32;
}

fn dll_dir() -> PathBuf {
    unsafe {
        let mut h: *mut c_void = std::ptr::null_mut();
        // GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | UNCHANGED_REFCOUNT
        GetModuleHandleExW(0x4 | 0x2, dll_dir as *const c_void, &mut h);
        let mut buf = [0u16; 1024];
        let n = GetModuleFileNameW(h, buf.as_mut_ptr(), buf.len() as u32) as usize;
        let p = PathBuf::from(String::from_utf16_lossy(&buf[..n]));
        p.parent().map(|d| d.to_path_buf()).unwrap_or_default()
    }
}

fn on_call(hook: u8, reg: *mut Registers) {
    if SV1REC_ON.load(Ordering::Relaxed) == 0 {
        return;
    }
    let reg = unsafe { &*reg };
    let f = |x: u128| f32::from_bits(x as u32);
    // At function entry [rsp] = return address, [rsp+0x28] / [rsp+0x30] = stack args 5 / 6
    let (ret, s5, s6) = unsafe {
        let sp = reg.rsp as *const u64;
        (sp.read_unaligned(), sp.add(5).read_unaligned(), sp.add(6).read_unaligned())
    };
    let tick = SV1REC_TICK.load(Ordering::Relaxed);
    let name = HOOKS[hook as usize].0;
    if name == "tae" {
        unsafe { on_tae(reg, tick) };
        return;
    }
    let Some(extra) = (unsafe { extra_of(name, reg) }) else { return };
    let call = Call {
        tick,
        hook,
        tid: unsafe { GetCurrentThreadId() },
        r: [reg.rcx, reg.rdx, reg.r8, reg.r9],
        f: [f(reg.xmm0), f(reg.xmm1), f(reg.xmm2), f(reg.xmm3)],
        ret,
        s5,
        s6,
        item: if name == "items" { unsafe { item_of(reg.rcx, reg.rdx as u32) } } else { None },
        extra,
    };
    if let Ok(mut q) = QUEUE.lock() {
        if q.len() < 200_000 {
            q.push(call);
        }
    }
}

fn writer() {
    let mut file: Option<File> = None;
    loop {
        std::thread::sleep(Duration::from_millis(50));
        let on = SV1REC_ON.load(Ordering::Acquire) != 0;
        if on && file.is_none() {
            let dir = unsafe {
                let d = &*std::ptr::addr_of!(SV1REC_DIR);
                let n = d.iter().position(|&b| b == 0).unwrap_or(d.len());
                String::from_utf8_lossy(&d[..n]).into_owned()
            };
            if !dir.is_empty() {
                file = OpenOptions::new().create(true).append(true).open(PathBuf::from(dir).join("hooks.jsonl")).ok();
            }
        }
        let calls = std::mem::take(&mut *QUEUE.lock().unwrap());
        let taes = tae_flush(SV1REC_TICK.load(Ordering::Relaxed), !on);
        if let Some(fh) = file.as_mut() {
            let mut out = String::with_capacity(calls.len() * 160 + taes.len() * 200);
            for t in &taes {
                out.push_str(&format!("{{\"tick\":{},\"end\":{},\"name\":\"tae\",\"tid\":{}{}}}\n", t.start, t.last, t.tid, t.extra));
            }
            for c in &calls {
                let item = match c.item {
                    Some((id, n)) => format!(",\"item\":{},\"item_type\":{},\"count\":{}", id & 0x0FFF_FFFF, id >> 28, n),
                    None => String::new(),
                };
                out.push_str(&format!(
                    "{{\"tick\":{},\"name\":\"{}\",\"tid\":{},\"arg0\":{},\"arg1\":{},\"arg2\":{},\"arg3\":{},\"f0\":{},\"f1\":{},\"f2\":{},\"f3\":{},\"ret\":{},\"s5\":{},\"s6\":{}{}}}\n",
                    c.tick, HOOKS[c.hook as usize].0, c.tid, c.r[0], c.r[1] as i64 as i32, c.r[2] as i64 as i32, c.r[3] as i64 as i32,
                    num(c.f[0]), num(c.f[1]), num(c.f[2]), num(c.f[3]), c.ret, c.s5, c.s6, item + &c.extra));
            }
            SV1REC_COUNT.fetch_add((calls.len() + taes.len()) as u32, Ordering::Relaxed);
            let _ = fh.write_all(out.as_bytes());
            if !on {
                let _ = fh.flush();
                file = None;
            }
        }
    }
}

fn num(x: f32) -> String {
    if x.is_finite() { format!("{x}") } else { "null".into() }
}

fn install() {
    let dir = dll_dir();
    let mut log = File::create(dir.join("sv1_rec_hooks.log")).ok();
    let want: Vec<String> = match std::fs::read_to_string(dir.join("sv1_rec_hooks.txt")) {
        Ok(s) => s.lines().map(|l| l.split('#').next().unwrap_or("").trim().to_string()).filter(|l| !l.is_empty()).collect(),
        Err(_) => DEFAULT_ON.iter().map(|s| s.to_string()).collect(),
    };
    // wait (up to 3 min) until the code is in place: first bytes of every wanted hook match
    let ready = |a: usize, b: &[u8; 6]| unsafe { std::slice::from_raw_parts(a as *const u8, 6) == b };
    for _ in 0..1800 {
        if HOOKS.iter().filter(|h| want.iter().any(|w| w == h.0)).all(|h| ready(h.1, &h.3)) {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    for (i, (name, addr, what, bytes)) in HOOKS.iter().enumerate() {
        if !want.iter().any(|w| w == name) {
            continue;
        }
        if !ready(*addr, bytes) {
            if let Some(l) = log.as_mut() {
                let _ = l.write_all(format!("SKIPPED {name} 0x{addr:x}: first bytes differ (other game version?)
").as_bytes());
            }
            continue;
        }
        let idx = i as u8;
        let r = unsafe { hook_closure_jmp_back(*addr, move |reg| on_call(idx, reg), CallbackOption::None, HookFlags::empty()) };
        let msg = match r {
            Ok(h) => {
                std::mem::forget(h); // hooks stay for the whole game session
                format!("ok {name} 0x{addr:x} ({what})\n")
            }
            Err(e) => format!("FAILED {name} 0x{addr:x}: {e:?}\n"),
        };
        if let Some(l) = log.as_mut() {
            let _ = l.write_all(msg.as_bytes());
        }
    }
    std::thread::spawn(writer);
}

#[no_mangle]
#[allow(non_snake_case)]
extern "system" fn DllMain(_module: *mut c_void, reason: u32, _reserved: *mut c_void) -> i32 {
    if reason == 1 {
        std::thread::spawn(install);
    }
    1
}
