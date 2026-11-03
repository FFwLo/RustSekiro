//! Comparison cases from the game itself: runs functions of the user's own sekiro.exe in an x64
//! emulator (Unicorn) with chosen inputs and writes the inputs and results to
//! extracted/emu/<name>.json; the sim tests run Sv1's ports on the same inputs and compare.
//! A Rust host because Unicorn crashes under the CET shadow stack that Python 3.14 enables; it
//! links the prebuilt unicorn.dll (build.rs), so the DLL must be on PATH when it runs.
//!
//!   cargo run --release -- <sekiro.exe> <out dir> [pilot ...]
//!
//! Each case runs the function as the game does, its callees included, unless a callee is stubbed
//! (a separate system whose value is an input of the port). Every case reports the basic blocks it
//! reached (coverage), the fake-object fields read without being set, and reads outside memory.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::ffi::c_void;
use std::path::Path;

use serde_json::{json, Value};

/// The few calls of Unicorn's C API the tool needs (unicorn.h), linked from unicorn.dll.
mod uc {
    use std::ffi::c_void;
    pub type Engine = *mut c_void;
    pub const ARCH_X86: i32 = 4;
    pub const MODE_64: i32 = 8;
    pub const PROT_ALL: u32 = 7;
    pub const HOOK_CODE: i32 = 4;
    pub const HOOK_BLOCK: i32 = 8;
    pub const HOOK_MEM_UNMAPPED: i32 = 112;
    pub const HOOK_MEM_READ: i32 = 1024;
    pub const RAX: i32 = 35;
    pub const RCX: i32 = 38;
    pub const RDX: i32 = 40;
    pub const RIP: i32 = 41;
    pub const RSP: i32 = 44;
    pub const R8: i32 = 106;
    pub const R9: i32 = 107;
    pub const XMM0: i32 = 122;
    extern "C" {
        pub fn uc_open(arch: i32, mode: i32, uc: *mut Engine) -> i32;
        pub fn uc_mem_map(uc: Engine, address: u64, size: usize, perms: u32) -> i32;
        pub fn uc_mem_write(uc: Engine, address: u64, bytes: *const c_void, size: usize) -> i32;
        pub fn uc_mem_read(uc: Engine, address: u64, bytes: *mut c_void, size: usize) -> i32;
        pub fn uc_reg_write(uc: Engine, regid: i32, value: *const c_void) -> i32;
        pub fn uc_reg_read(uc: Engine, regid: i32, value: *mut c_void) -> i32;
        pub fn uc_hook_add(uc: Engine, hh: *mut usize, kind: i32, callback: *const c_void, user_data: *mut c_void, begin: u64, end: u64, ...) -> i32;
        pub fn uc_hook_del(uc: Engine, hh: usize) -> i32;
        pub fn uc_emu_start(uc: Engine, begin: u64, until: u64, timeout: u64, count: usize) -> i32;
    }
}

const PAGE: u64 = 0x1000;
const HEAP: u64 = 0x7000_0000_0000;
const STACK: u64 = 0x7100_0000_0000;
const STUBS: u64 = 0x7200_0000_0000;
const RETURN: u64 = STUBS + 0xfff0;
const HEAP_SIZE: u64 = 0x10_0000;
const STACK_SIZE: u64 = 0x10_0000;

/// A stub's results: rax and an optional float in xmm0.
#[derive(Clone)]
struct Stub {
    name: String,
    rax: u64,
    xmm0: Option<f32>,
}

#[derive(Default)]
struct State {
    stubs: HashMap<u64, Stub>,
    written: HashSet<u64>,
    heap_next: u64,
    running: bool,
    calls: Vec<String>,
    unset: BTreeSet<u64>,
    wild: BTreeSet<u64>,
    blocks: BTreeSet<u64>,
    func: u64,
}

#[allow(dead_code)]
struct Run {
    rax: u64,
    xmm0: f32,
    calls: Vec<String>,
    unset: BTreeSet<u64>,
    wild: BTreeSet<u64>,
    blocks: BTreeSet<u64>,
    /// Where the run stopped early (address, unicorn error); None when it returned.
    stopped: Option<(u64, i32)>,
}

struct Emu {
    uc: uc::Engine,
    st: Box<State>,
    next_stub: u64,
    sizes: HashMap<u64, u64>,
}

fn ok(e: i32, what: &str) {
    assert!(e == 0, "unicorn {what}: error {e}");
}

extern "C" fn on_stub(uc: uc::Engine, addr: u64, _size: u32, data: *mut c_void) {
    let st = unsafe { &mut *(data as *mut State) };
    let Some(s) = st.stubs.get(&addr).cloned() else { return };
    if !st.running {
        return;
    }
    st.calls.push(s.name);
    unsafe {
        uc::uc_reg_write(uc, uc::RAX, &s.rax as *const u64 as _);
        if let Some(x) = s.xmm0 {
            let b = f32_bits(x);
            uc::uc_reg_write(uc, uc::XMM0, b.as_ptr() as _);
        }
    }
}

extern "C" fn on_block(_uc: uc::Engine, addr: u64, _size: u32, data: *mut c_void) {
    let st = unsafe { &mut *(data as *mut State) };
    st.blocks.insert(addr - st.func);
}

extern "C" fn on_unmapped(uc: uc::Engine, kind: i32, addr: u64, _size: i32, _value: i64, data: *mut c_void) -> bool {
    let st = unsafe { &mut *(data as *mut State) };
    // UC_MEM_FETCH_UNMAPPED: a jump through a null or unset pointer stops the run.
    if !st.running || kind == 21 {
        return false;
    }
    st.wild.insert(addr);
    unsafe { uc::uc_mem_map(uc, addr & !(PAGE - 1), PAGE as usize, uc::PROT_ALL) == 0 }
}

extern "C" fn on_read(_uc: uc::Engine, _kind: i32, addr: u64, size: i32, _value: i64, data: *mut c_void) -> bool {
    let st = unsafe { &mut *(data as *mut State) };
    if st.running && addr < st.heap_next && (addr..addr + size as u64).any(|a| !st.written.contains(&a)) {
        st.unset.insert(addr);
    }
    true
}

fn f32_bits(v: f32) -> [u8; 16] {
    let mut b = [0u8; 16];
    b[..4].copy_from_slice(&v.to_le_bytes());
    b
}

impl Emu {
    fn new(exe: &Path) -> Emu {
        let img = std::fs::read(exe).expect("sekiro.exe");
        let u32at = |o: usize| u32::from_le_bytes(img[o..o + 4].try_into().unwrap());
        let pe = u32at(0x3c) as usize;
        let nsec = u16::from_le_bytes(img[pe + 6..pe + 8].try_into().unwrap()) as usize;
        let opt = pe + 24;
        let opt_size = u16::from_le_bytes(img[pe + 20..pe + 22].try_into().unwrap()) as usize;
        let base = u64::from_le_bytes(img[opt + 24..opt + 32].try_into().unwrap());
        let mut handle: uc::Engine = std::ptr::null_mut();
        ok(unsafe { uc::uc_open(uc::ARCH_X86, uc::MODE_64, &mut handle) }, "open");
        let mut e = Emu { uc: handle, st: Box::new(State { heap_next: HEAP, ..Default::default() }), next_stub: STUBS, sizes: functions_json() };
        e.map(base, PAGE);
        for i in 0..nsec {
            let s = opt + opt_size + 40 * i;
            let (vsize, va, rsize, roff) = (u32at(s + 8) as u64, u32at(s + 12) as u64, u32at(s + 16) as usize, u32at(s + 20) as usize);
            e.map(base + va, (vsize.max(rsize as u64) + PAGE - 1) & !(PAGE - 1));
            let n = rsize.min(vsize as usize).min(img.len().saturating_sub(roff));
            e.raw_write(base + va, &img[roff..roff + n]);
        }
        // Windows fills the import table at load; here every slot becomes a stub named after its import.
        let imports = u32at(opt + 112 + 8) as u64;
        let rd = |e: &Emu, at: u64, n: usize| -> Vec<u8> {
            let mut b = vec![0u8; n];
            unsafe { uc::uc_mem_read(e.uc, at, b.as_mut_ptr() as _, n) };
            b
        };
        let mut desc = base + imports;
        let mut slots = Vec::new();
        while imports != 0 {
            let d = rd(&e, desc, 20);
            let (orig, first) = (u32::from_le_bytes(d[0..4].try_into().unwrap()) as u64, u32::from_le_bytes(d[16..20].try_into().unwrap()) as u64);
            if first == 0 {
                break;
            }
            let names = if orig != 0 { orig } else { first };
            for i in 0.. {
                let t = u64::from_le_bytes(rd(&e, base + names + 8 * i, 8).try_into().unwrap());
                if t == 0 {
                    break;
                }
                let name = if t >> 63 == 1 {
                    format!("ordinal_{}", t & 0xffff)
                } else {
                    let raw = rd(&e, base + (t & 0x7fff_ffff) + 2, 64);
                    String::from_utf8_lossy(&raw[..raw.iter().position(|&c| c == 0).unwrap_or(64)]).into_owned()
                };
                slots.push((base + first + 8 * i, name));
            }
            desc += 20;
        }
        e.map(HEAP, HEAP_SIZE);
        e.map(STACK, STACK_SIZE);
        e.map(STUBS, 0x10000);
        e.raw_write(STUBS, &vec![0xc3u8; 0x10000]);
        e.hook(uc::HOOK_CODE, on_stub as *const c_void, STUBS, STUBS + 0xffff);
        for (slot, name) in slots {
            let f = e.stub(&name, 0, None);
            e.raw_write(slot, &f.to_le_bytes());
        }
        e.hook(uc::HOOK_MEM_UNMAPPED, on_unmapped as *const c_void, 1, 0);
        e.hook(uc::HOOK_MEM_READ, on_read as *const c_void, HEAP, HEAP + HEAP_SIZE - 1);
        e
    }

    fn map(&mut self, at: u64, size: u64) {
        ok(unsafe { uc::uc_mem_map(self.uc, at, size as usize, uc::PROT_ALL) }, "mem_map");
    }
    fn hook(&mut self, kind: i32, cb: *const c_void, begin: u64, end: u64) -> usize {
        let mut h = 0usize;
        let data = &mut *self.st as *mut State as *mut c_void;
        ok(unsafe { uc::uc_hook_add(self.uc, &mut h, kind, cb, data, begin, end) }, "hook_add");
        h
    }
    fn raw_write(&mut self, at: u64, bytes: &[u8]) {
        ok(unsafe { uc::uc_mem_write(self.uc, at, bytes.as_ptr() as _, bytes.len()) }, "mem_write");
    }
    fn reg(&self, id: i32) -> u64 {
        let mut v = 0u64;
        unsafe { uc::uc_reg_read(self.uc, id, &mut v as *mut u64 as _) };
        v
    }
    fn set_reg(&mut self, id: i32, v: u64) {
        unsafe { uc::uc_reg_write(self.uc, id, &v as *const u64 as _) };
    }

    fn alloc(&mut self, size: u64) -> u64 {
        let at = (self.st.heap_next + 15) & !15;
        self.st.heap_next = at + size;
        assert!(self.st.heap_next <= HEAP + HEAP_SIZE, "emu heap full");
        at
    }

    fn write(&mut self, at: u64, bytes: &[u8]) {
        self.raw_write(at, bytes);
        if (HEAP..HEAP + HEAP_SIZE).contains(&at) {
            self.st.written.extend(at..at + bytes.len() as u64);
        }
    }
    fn u8(&mut self, at: u64, v: u8) { self.write(at, &[v]) }
    fn u32(&mut self, at: u64, v: u32) { self.write(at, &v.to_le_bytes()) }
    fn u64(&mut self, at: u64, v: u64) { self.write(at, &v.to_le_bytes()) }
    fn f32(&mut self, at: u64, v: f32) { self.write(at, &v.to_le_bytes()) }
    fn read<const N: usize>(&self, at: u64) -> [u8; N] {
        let mut b = [0u8; N];
        ok(unsafe { uc::uc_mem_read(self.uc, at, b.as_mut_ptr() as _, N) }, "mem_read");
        b
    }
    fn rf32(&self, at: u64) -> f32 { f32::from_le_bytes(self.read(at)) }
    fn ru8(&self, at: u64) -> u8 { self.read::<1>(at)[0] }

    /// A new stub function returning rax / xmm0.
    fn stub(&mut self, name: &str, rax: u64, xmm0: Option<f32>) -> u64 {
        let at = self.next_stub;
        self.next_stub += 16;
        self.st.stubs.insert(at, Stub { name: name.into(), rax, xmm0 });
        at
    }
    /// Replaces a function of the image with a stub.
    #[allow(dead_code)]
    fn stub_at(&mut self, at: u64, name: &str, rax: u64, xmm0: Option<f32>) {
        self.raw_write(at, &[0xc3]);
        self.hook(uc::HOOK_CODE, on_stub as *const c_void, at, at);
        self.st.stubs.insert(at, Stub { name: name.into(), rax, xmm0 });
    }
    fn set_stub(&mut self, at: u64, rax: u64, xmm0: Option<f32>) {
        let s = self.st.stubs.get_mut(&at).unwrap();
        s.rax = rax;
        s.xmm0 = xmm0;
    }
    /// A fake vtable: slot byte offset -> function; other slots are stubs named by their offset.
    fn vtable(&mut self, slots: &[(u64, u64)], size: u64) -> u64 {
        let vt = self.alloc(size);
        for off in (0..size).step_by(8) {
            let f = slots.iter().find(|s| s.0 == off).map(|s| s.1).unwrap_or_else(|| self.stub(&format!("vslot_{off:#x}"), 0, None));
            self.u64(vt + off, f);
        }
        vt
    }

    /// Calls `va` with integer args in rcx, rdx, r8, r9 and floats in xmm0-3 (Windows x64).
    fn call(&mut self, va: u64, args: &[u64], xmm: &[f32]) -> Run {
        for (r, v) in [uc::RCX, uc::RDX, uc::R8, uc::R9].iter().zip(args) {
            self.set_reg(*r, *v);
        }
        for (i, v) in xmm.iter().enumerate() {
            let b = f32_bits(*v);
            unsafe { uc::uc_reg_write(self.uc, uc::XMM0 + i as i32, b.as_ptr() as _) };
        }
        let sp = (STACK + STACK_SIZE - 0x1000 - 8 * args.len().saturating_sub(4) as u64 - 0x28) & !0xf;
        for (i, v) in args.iter().skip(4).enumerate() {
            self.raw_write(sp + 0x28 + 8 * i as u64, &v.to_le_bytes());
        }
        self.raw_write(sp, &RETURN.to_le_bytes());
        self.set_reg(uc::RSP, sp);
        let size = *self.sizes.get(&va).unwrap_or(&0x10000);
        self.st.running = true;
        self.st.func = va;
        self.st.calls.clear();
        self.st.unset.clear();
        self.st.wild.clear();
        self.st.blocks.clear();
        let h = self.hook(uc::HOOK_BLOCK, on_block as *const c_void, va, va + size - 1);
        let r = unsafe { uc::uc_emu_start(self.uc, va, RETURN, 0, 5_000_000) };
        unsafe { uc::uc_hook_del(self.uc, h) };
        self.st.running = false;
        let rip = self.reg(uc::RIP);
        let stopped = if r == 0 && rip == RETURN { None } else { Some((rip, r)) };
        let mut x0 = [0u8; 16];
        unsafe { uc::uc_reg_read(self.uc, uc::XMM0, x0.as_mut_ptr() as _) };
        Run {
            rax: self.reg(uc::RAX),
            xmm0: f32::from_le_bytes(x0[..4].try_into().unwrap()),
            calls: std::mem::take(&mut self.st.calls),
            unset: std::mem::take(&mut self.st.unset),
            wild: std::mem::take(&mut self.st.wild),
            blocks: std::mem::take(&mut self.st.blocks),
            stopped,
        }
    }
}

/// Function sizes from DSekiro's list when it is next to us (coverage ranges); else none.
fn functions_json() -> HashMap<u64, u64> {
    let path = std::env::var("DSEKIRO_FUNCTIONS").unwrap_or_else(|_| r"<dsekiro>\data\functions.json".into());
    let Ok(text) = std::fs::read_to_string(path) else { return HashMap::new() };
    let v: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
    v.as_array()
        .into_iter()
        .flatten()
        .filter_map(|f| Some((u64::from_str_radix(f["va"].as_str()?.trim_start_matches("0x"), 16).ok()?, f["size"].as_u64()?)))
        .collect()
}

struct Report {
    cases: Vec<Value>,
    blocks: BTreeSet<u64>,
    unset: BTreeSet<u64>,
    wild: usize,
    calls: BTreeSet<String>,
}

impl Report {
    fn new() -> Self {
        Report { cases: Vec::new(), blocks: BTreeSet::new(), unset: BTreeSet::new(), wild: 0, calls: BTreeSet::new() }
    }
    fn add(&mut self, r: &Run, case: Value, obj: Option<(u64, u64)>) {
        assert!(r.stopped.is_none(), "a case stopped early: {:?}", r.stopped);
        self.blocks.extend(&r.blocks);
        if let Some((at, size)) = obj {
            self.unset.extend(r.unset.iter().filter(|a| (at..at + size).contains(a)).map(|a| a - at));
        }
        self.wild += r.wild.len();
        self.calls.extend(r.calls.iter().cloned());
        self.cases.push(case);
    }
    fn write(self, out: &Path, name: &str, va: u64, emu: &Emu) {
        let total = emu.sizes.get(&va).copied().unwrap_or(0);
        let body = json!({
            "function": format!("{va:#x}"),
            "blocks_reached": self.blocks.len(),
            "unset_fields": self.unset.iter().map(|o| format!("{o:#x}")).collect::<Vec<_>>(),
            "stubs_called": self.calls,
            "cases": self.cases,
        });
        std::fs::write(out.join(format!("{name}.json")), serde_json::to_string_pretty(&body).unwrap()).unwrap();
        println!(
            "{name}: {} cases, {} blocks reached ({} bytes), unset fields {:?}, wild reads {}, stubs {:?}",
            body["cases"].as_array().unwrap().len(),
            self.blocks.len(),
            total,
            body["unset_fields"],
            self.wild,
            body["stubs_called"]
        );
    }
}

/// Pilot 1: the super armour hit (FUN_140bdb470) against actor::SuperArmor::hit.
fn super_armor_hit(emu: &mut Emu, out: &Path) {
    const F: u64 = 0x1_40bd_b470;
    let max_stub = emu.stub("max", 0, Some(0.0));
    let delay_stub = emu.stub("delay", 0, Some(7.0));
    let owner = emu.alloc(0x2000);
    let vt = emu.vtable(&[(0x2c8, max_stub), (0x2d8, delay_stub)], 0x400);
    emu.u64(owner, vt);
    let flags = emu.alloc(0x80);
    let pflags = emu.alloc(8);
    emu.u64(pflags, flags);
    emu.u32(flags + 0x70, 0);
    emu.u64(owner + 0x1ff8, pflags);
    emu.u64(owner + 0x11d0, 0);
    let g = emu.alloc(0x40);
    let dmg = emu.alloc(0x300);
    let mut rep = Report::new();
    for mx in [0.0f32, 100.0, 300.0] {
        for bonus in [0.0f32, 40.0] {
            for frac in [0.0f32, 0.5, 1.0, 1.2] {
                for broken in [false, true] {
                    for timer in [-0.01f32, 3.0] {
                        for amount in [-5.0f32, 0.0, 10.0, 60.0, 150.0, 500.0] {
                            for reserve in [0.0f32, 20.0] {
                                let cur = (mx + bonus) * frac;
                                emu.set_stub(max_stub, 0, Some(mx));
                                emu.u64(g + 8, owner);
                                emu.f32(g + 0x10, cur);
                                emu.f32(g + 0x18, reserve);
                                emu.f32(g + 0x1c, timer);
                                emu.u8(g + 0x20, broken as u8);
                                emu.f32(g + 0x24, bonus);
                                emu.f32(dmg + 0x210, amount);
                                emu.f32(dmg + 0xe0, 1.0);
                                emu.u32(dmg + 0x24, 1);
                                emu.u8(dmg + 0x164, 0);
                                emu.u8(dmg + 0x231, 0);
                                let r = emu.call(F, &[g, 0, dmg], &[]);
                                let ob = emu.ru8(g + 0x20) != 0;
                                let case = json!({
                                    "max": mx, "bonus": bonus, "cur": cur, "broken": broken, "timer": timer, "delay": 7.0,
                                    "amount": amount, "reserve": reserve,
                                    "out": {"cur": emu.rf32(g + 0x10), "reserve": emu.rf32(g + 0x18), "timer": emu.rf32(g + 0x1c), "broken": ob},
                                });
                                rep.add(&r, case, Some((g, 0x40)));
                            }
                        }
                    }
                }
            }
        }
    }
    rep.write(out, "super_armor_hit", F, emu);
}

/// Pilot 2: the attacker's hit stop start (FUN_140b9cd90) for every defender hit-stop type,
/// component mode (+0x2d), engine switch (+0x11d of the global at DAT_143d7a388), action-state
/// bit and ChrActionFlag 41; its neighbours are stubs so the rule table stands alone.
fn hit_stop(emu: &mut Emu, out: &Path) {
    const F: u64 = 0x1_40b9_cd90;
    let chr = emu.alloc(0x2000);
    let chr_get = 0x1_40b2_71d0;
    emu.stub_at(chr_get, "chr_of", chr, None);
    let kind = 0x1_40b9_d850;
    emu.stub_at(kind, "hitStopType", 0, None);
    emu.stub_at(0x1_40b6_0ec0, "attack_kind", 0, None);
    let flag41 = 0x1_40b6_0f10;
    emu.stub_at(flag41, "flag41", 0, None);
    emu.stub_at(0x1_40b9_dad0, "start_a", 0, None);
    emu.stub_at(0x1_40b9_d930, "start_b", 0, None);
    // The chr's +0x1ff8 module: [+0] -> +0x54 bit 3 (an action state) and +0x90 (passed to the type).
    let module = emu.alloc(0x100);
    let inner = emu.alloc(0x100);
    emu.u64(chr + 0x1ff8, module);
    emu.u64(module, inner);
    emu.u64(module + 0x90, 0);
    let global = emu.alloc(0x200);
    emu.u64(0x1_43d7_a388, global);
    let comp = emu.alloc(0x40);
    let times = emu.alloc(0x20);
    let attack = emu.alloc(0x20);
    let atk_data = emu.alloc(0x40);
    let mut rep = Report::new();
    for mode in [0u8, 1, 2, 3] {
        for ty in [0u64, 1, 2, 3, 4] {
            for f41 in [0u64, 1] {
                for state in [0u32, 1] {
                    for sw in [0u8, 1] {
                        emu.set_stub(kind, ty, None);
                        emu.set_stub(flag41, f41, None);
                        emu.u32(inner + 0x54, state << 3);
                        emu.u8(global + 0x11d, sw);
                        emu.f32(global + 0x124, 0.5);
                        for i in 0..0x40 {
                            emu.u8(comp + i, 0);
                        }
                        emu.u8(comp + 0x2d, mode);
                        for i in 0..5 {
                            emu.f32(times + 4 * i, if i == 0 { 0.1 } else { 0.0 });
                        }
                        emu.u64(attack + 0x10, atk_data);
                        emu.f32(atk_data + 0x1c, 0.1);
                        let r = emu.call(F, &[comp, chr, times, attack], &[]);
                        let case = json!({
                            "mode": mode, "type": ty, "flag41": f41 == 1, "state": state == 1, "switch": sw == 1, "time": 0.1, "weak": 0.5,
                            "out": {"time": emu.rf32(comp + 0x34), "started": r.rax & 0xff == 1, "forced": emu.ru8(comp + 0x31) == 1},
                        });
                        rep.add(&r, case, Some((comp, 0x40)));
                    }
                }
            }
        }
    }
    rep.write(out, "hit_stop", F, emu);
}

/// probe <addr>: runs a function on zeroed objects (rcx, rdx, r8, r9 each 0x4000 bytes) and lists
/// the fields it reads, how far it gets and what it calls: the setup a comparison test needs.
fn probe(emu: &mut Emu, va: u64) {
    let objs: Vec<u64> = (0..4).map(|_| emu.alloc(0x4000)).collect();
    let r = emu.call(va, &objs, &[0.0, 0.0, 0.0, 0.0]);
    let per = |o: u64| r.unset.iter().filter(|a| (o..o + 0x4000).contains(*a)).count();
    let mut calls: Vec<_> = r.calls.clone();
    calls.sort();
    calls.dedup();
    println!(
        "probe {va:#x}: {} blocks reached ({} bytes); unset fields read rcx {} rdx {} r8 {} r9 {}; reads outside memory {}; stubs {}",
        r.blocks.len(), emu.sizes.get(&va).copied().unwrap_or(0), per(objs[0]), per(objs[1]), per(objs[2]), per(objs[3]), r.wild.len(), calls.len()
    );
    if let Some((at, e)) = r.stopped {
        println!("  stopped early at {at:#x} (error {e})");
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let exe = Path::new(args.get(1).expect("usage: emu-cases <sekiro.exe> <out dir> [pilot ...]"));
    let out = Path::new(args.get(2).expect("out dir"));
    std::fs::create_dir_all(out).unwrap();
    let want: Vec<&str> = args[3..].iter().map(String::as_str).collect();
    let on = |n: &str| want.is_empty() || want.contains(&n);
    let t = std::time::Instant::now();
    if want.first() == Some(&"probe") {
        let mut emu = Emu::new(exe);
        for a in &want[1..] {
            probe(&mut emu, u64::from_str_radix(a.trim_start_matches("0x"), 16).unwrap());
        }
        return;
    }
    if on("super_armor_hit") {
        let mut emu = Emu::new(exe);
        super_armor_hit(&mut emu, out);
    }
    if on("hit_stop") {
        let mut emu = Emu::new(exe);
        hit_stop(&mut emu, out);
    }
    println!("done in {:.1} s", t.elapsed().as_secs_f32());
}
