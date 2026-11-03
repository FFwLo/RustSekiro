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

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::ffi::c_void;
use std::path::Path;

use serde_json::{json, Value};

mod camwall;
mod hks;
mod damage;
mod snapshot;
mod guard;
mod trace;
mod lockcam;
mod turn;
mod taecam;
mod modticks;
mod taewin;
mod throw;
mod aicpp;
mod speffect;
mod proxy;

/// The few calls of Unicorn's C API the tool needs (unicorn.h), linked from unicorn.dll.
mod uc {
    use std::ffi::c_void;
    pub type Engine = *mut c_void;
    pub const ARCH_X86: i32 = 4;
    pub const MODE_64: i32 = 8;
    pub const PROT_ALL: u32 = 7;
    pub const PROT_RW: u32 = 3;
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
    pub const GS_BASE: i32 = 251;
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
    size: u64,
    /// The last blocks run (any function), for where a run went wrong.
    trail: Vec<u64>,
    /// The game's memory, mapped a 64 KB window at a time on first touch.
    snap: Option<snapshot::Snap>,
    /// `trace`: function starts (functions.json) and how often each was entered.
    starts: HashSet<u64>,
    entered: BTreeMap<u64, u64>,
    /// `trace`: (return address, function entered) pairs; the caller is the function holding the
    /// return address.
    edges: BTreeSet<(u64, u64)>,
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
    /// Instructions a call may run before it is stopped.
    limit: usize,
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
    let rax = if s.name == "TlsGetValue" { tls_get_value(uc, st) } else { s.rax };
    st.calls.push(s.name);
    unsafe {
        uc::uc_reg_write(uc, uc::RAX, &rax as *const u64 as _);
        if let Some(x) = s.xmm0 {
            let b = f32_bits(x);
            uc::uc_reg_write(uc, uc::XMM0, b.as_ptr() as _);
        }
    }
}

/// Reads 8 bytes, mapping them from the snapshot first when the run has not touched them.
fn read_u64(uc: uc::Engine, st: &mut State, at: u64) -> u64 {
    let mut v = 0u64;
    unsafe {
        if uc::uc_mem_read(uc, at, &mut v as *mut u64 as _, 8) != 0 {
            if let Some((from, bytes)) = st.snap.as_mut().and_then(|s| s.window(at)) {
                uc::uc_mem_map(uc, from, bytes.len(), uc::PROT_ALL);
                uc::uc_mem_write(uc, from, bytes.as_ptr() as _, bytes.len());
                uc::uc_mem_read(uc, at, &mut v as *mut u64 as _, 8);
            }
        }
    }
    v
}

/// TlsGetValue(index) from the snapshot's main-thread TEB: its 64 slots at +0x1480, then the
/// expansion slots behind the pointer at +0x1780.
fn tls_get_value(uc: uc::Engine, st: &mut State) -> u64 {
    let (mut index, mut teb) = (0u64, 0u64);
    unsafe {
        uc::uc_reg_read(uc, uc::RCX, &mut index as *mut u64 as _);
        uc::uc_reg_read(uc, uc::GS_BASE, &mut teb as *mut u64 as _);
    }
    let index = index & 0xffff_ffff;
    if teb == 0 {
        return 0;
    }
    if index < 64 {
        read_u64(uc, st, teb + 0x1480 + 8 * index)
    } else {
        let more = read_u64(uc, st, teb + 0x1780);
        if more == 0 { 0 } else { read_u64(uc, st, more + 8 * (index - 64)) }
    }
}

extern "C" fn on_block(uc: uc::Engine, addr: u64, _size: u32, data: *mut c_void) {
    let st = unsafe { &mut *(data as *mut State) };
    if (st.func..st.func + st.size).contains(&addr) {
        st.blocks.insert(addr - st.func);
    }
    if st.trail.len() == 16 {
        st.trail.remove(0);
    }
    st.trail.push(addr);
    if st.starts.contains(&addr) {
        *st.entered.entry(addr).or_default() += 1;
        let (mut sp, mut ret) = (0u64, 0u64);
        unsafe {
            uc::uc_reg_read(uc, uc::RSP, &mut sp as *mut u64 as _);
            uc::uc_mem_read(uc, sp, &mut ret as *mut u64 as _, 8);
        }
        st.edges.insert((ret, addr));
    }
}

extern "C" fn on_unmapped(uc: uc::Engine, kind: i32, addr: u64, _size: i32, _value: i64, data: *mut c_void) -> bool {
    let st = unsafe { &mut *(data as *mut State) };
    // UC_MEM_FETCH_UNMAPPED: a jump through a null or unset pointer stops the run.
    if !st.running {
        return false;
    }
    if let Some((from, bytes)) = st.snap.as_mut().and_then(|s| s.window(addr)) {
        unsafe {
            return uc::uc_mem_map(uc, from, bytes.len(), uc::PROT_ALL) == 0 && uc::uc_mem_write(uc, from, bytes.as_ptr() as _, bytes.len()) == 0;
        }
    }
    if kind == 21 {
        return false;
    }
    st.wild.insert(addr);
    // Read/write only: a later jump into this zero page stops the run instead of running zeros.
    unsafe { uc::uc_mem_map(uc, addr & !(PAGE - 1), PAGE as usize, uc::PROT_RW) == 0 }
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
    fn new(exe: &Path, snap: Option<snapshot::Snap>) -> Emu {
        let img = std::fs::read(exe).expect("sekiro.exe");
        let u32at = |o: usize| u32::from_le_bytes(img[o..o + 4].try_into().unwrap());
        let pe = u32at(0x3c) as usize;
        let nsec = u16::from_le_bytes(img[pe + 6..pe + 8].try_into().unwrap()) as usize;
        let opt = pe + 24;
        let opt_size = u16::from_le_bytes(img[pe + 20..pe + 22].try_into().unwrap()) as usize;
        let base = u64::from_le_bytes(img[opt + 24..opt + 32].try_into().unwrap());
        let mut handle: uc::Engine = std::ptr::null_mut();
        ok(unsafe { uc::uc_open(uc::ARCH_X86, uc::MODE_64, &mut handle) }, "open");
        let mut image_end = base;
        let mut e = Emu { uc: handle, st: Box::new(State { heap_next: HEAP, ..Default::default() }), next_stub: STUBS, sizes: functions_json(), limit: 5_000_000 };
        e.map(base, PAGE);
        for i in 0..nsec {
            let s = opt + opt_size + 40 * i;
            let (vsize, va, rsize, roff) = (u32at(s + 8) as u64, u32at(s + 12) as u64, u32at(s + 16) as usize, u32at(s + 20) as usize);
            e.map(base + va, (vsize.max(rsize as u64) + PAGE - 1) & !(PAGE - 1));
            let n = rsize.min(vsize as usize).min(img.len().saturating_sub(roff));
            e.raw_write(base + va, &img[roff..roff + n]);
            image_end = image_end.max(base + va + vsize);
        }
        // With a snapshot the image is the game's own copy (its globals as they were); the import
        // slots below still become stubs.
        if let Some(mut s) = snap {
            for (lo, size) in s.within(base, image_end) {
                let mut at = lo;
                while at < lo + size {
                    let (from, bytes) = s.window(at).unwrap();
                    e.raw_write(from, &bytes);
                    at = from + bytes.len() as u64;
                }
            }
            assert!(!s.overlaps(HEAP, STUBS + 0x10000), "the snapshot uses the emulator's heap/stack range");
            e.set_reg(uc::GS_BASE, s.main_teb);
            // The CRT picked its FMA3 maths at startup (this CPU has it); Unicorn has no FMA, so
            // clear the CRT's use-FMA flag and the maths takes its SSE2 path (may differ by 1 ulp).
            e.raw_write(0x1_43f3_36f4, &0u32.to_le_bytes());
            e.st.snap = Some(s);
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
    /// Reads memory the run has not touched yet, mapping it from the snapshot first.
    fn peek<const N: usize>(&mut self, at: u64) -> [u8; N] {
        let mut b = [0u8; N];
        if unsafe { uc::uc_mem_read(self.uc, at, b.as_mut_ptr() as _, N) } != 0 {
            let (from, bytes) = self.st.snap.as_mut().and_then(|s| s.window(at)).expect("address not in the snapshot");
            self.map(from, bytes.len() as u64);
            self.raw_write(from, &bytes);
            b = self.read(at);
        }
        b
    }
    fn peek_u64(&mut self, at: u64) -> u64 { u64::from_le_bytes(self.peek(at)) }
    /// Writes floats into the game's memory (mapping the page from the snapshot first).
    fn poke_f32s(&mut self, at: u64, v: &[f32]) {
        self.peek::<4>(at);
        self.raw_write(at, &v.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<_>>());
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
        self.st.size = size;
        self.st.trail.clear();
        let h = self.hook(uc::HOOK_BLOCK, on_block as *const c_void, 1, 0);
        let r = unsafe { uc::uc_emu_start(self.uc, va, RETURN, 0, self.limit) };
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

/// The game's ChrCam: FieldArea [0x143d5c0a0] +0x30 (SekiroTool), checked by its vtable.
fn chr_cam(emu: &mut Emu) -> u64 {
    let field_area = emu.peek_u64(0x1_43d5_c0a0);
    let cam = emu.peek_u64(field_area + 0x30);
    assert_eq!(emu.peek_u64(cam), 0x1_429a_6d40, "FieldArea +0x30 is not a ChrCam");
    cam
}

/// Pilot 3: the follow camera's frame update (FUN_140734790 -> FUN_14073c260) on the game's own
/// memory (--snapshot): ChrCam found by its vtable, the player and the pad state as the game's
/// caller FUN_140a6cf00 passes them. Reports how far it runs and which camera fields change.
fn camera(emu: &mut Emu, out: &Path) {
    const F: u64 = 0x1_4073_4790;
    let player = emu.st.snap.as_ref().expect("camera needs --snapshot").player;
    let cam = chr_cam(emu);
    let follow = emu.peek_u64(cam + 0x60);
    let pad_man = emu.peek_u64(0x1_43d6_d640);
    let pad = emu.peek_u64(pad_man + 0x98);
    let size = 0x800;
    let before: Vec<[u8; 4]> = (0..size / 4).map(|i| emu.peek(follow + 4 * i)).collect();
    let mut rep = Report::new();
    for dt in [1.0f32 / 60.0, 1.0 / 30.0] {
        let r = emu.call(F, &[cam, 0, player, pad, 0], &[0.0, dt]);
        let changed: Vec<Value> = (0..size / 4)
            .filter_map(|i| {
                let now: [u8; 4] = emu.read(follow + 4 * i);
                (now != before[i as usize]).then(|| json!({"off": 4 * i, "before": f32::from_le_bytes(before[i as usize]), "after": f32::from_le_bytes(now)}))
            })
            .collect();
        if r.stopped.is_some() {
            println!("  calls {:?}
  last blocks {:x?}", r.calls, emu.st.trail);
        }
        println!("camera dt {dt:.4}: {} blocks, stopped {:?}, {} follow-cam fields changed, {} stubs hit", r.blocks.len(), r.stopped.map(|(a, e)| (format!("{a:#x}"), e)), changed.len(), r.calls.len());
        rep.add(&r, json!({"dt": dt, "changed": changed}), None);
    }
    rep.write(out, "camera", F, emu);
}

/// One camera scenario: per frame, the right stick (M+0x80: x raises pitch +0x170, y raises yaw
/// +0x174), the mouse (M+0xd0, degrees) and Wolf's move (metres a frame, world).
struct CamScenario {
    name: &'static str,
    frames: usize,
    dt: f32,
    input: fn(usize) -> ([f32; 2], [f32; 2], [f32; 3]),
}

const CAM_SCENARIOS: [CamScenario; 17] = [
    CamScenario { name: "still", frames: 60, dt: 1.0 / 60.0, input: |_| ([0.0; 2], [0.0; 2], [0.0; 3]) },
    CamScenario { name: "stick_yaw_full_then_release", frames: 90, dt: 1.0 / 60.0, input: |f| (if f < 60 { [0.0, 1.0] } else { [0.0; 2] }, [0.0; 2], [0.0; 3]) },
    CamScenario { name: "stick_yaw_half", frames: 60, dt: 1.0 / 60.0, input: |_| ([0.0, 0.5], [0.0; 2], [0.0; 3]) },
    CamScenario { name: "stick_pitch_up_half", frames: 60, dt: 1.0 / 60.0, input: |_| ([0.5, 0.0], [0.0; 2], [0.0; 3]) },
    CamScenario { name: "mouse_yaw_3deg", frames: 60, dt: 1.0 / 60.0, input: |_| ([0.0; 2], [0.0, 3.0], [0.0; 3]) },
    CamScenario { name: "wolf_runs_x_6ms", frames: 90, dt: 1.0 / 60.0, input: |_| ([0.0; 2], [0.0; 2], [0.1, 0.0, 0.0]) },
    CamScenario { name: "still_30fps", frames: 30, dt: 1.0 / 30.0, input: |_| ([0.0; 2], [0.0; 2], [0.0; 3]) },
    CamScenario { name: "wolf_climbs_3ms", frames: 90, dt: 1.0 / 60.0, input: |_| ([0.0; 2], [0.0; 2], [0.0, 0.05, 0.0]) },
    // The snapshot's camera looks along (0.574, -0.819) on X / Z: this runs at it.
    CamScenario { name: "wolf_runs_at_camera_6ms", frames: 90, dt: 1.0 / 60.0, input: |_| ([0.0; 2], [0.0; 2], [-0.0574, 0.0, 0.0819]) },
    CamScenario { name: "wolf_runs_then_stops", frames: 120, dt: 1.0 / 60.0, input: |f| ([0.0; 2], [0.0; 2], if f < 40 { [0.1, 0.0, 0.0] } else { [0.0; 3] }) },
    CamScenario { name: "stick_full_diagonal", frames: 60, dt: 1.0 / 60.0, input: |_| ([0.7071, 0.7071], [0.0; 2], [0.0; 3]) },
    CamScenario { name: "stick_yaw_left_full", frames: 60, dt: 1.0 / 60.0, input: |_| ([0.0, -1.0], [0.0; 2], [0.0; 3]) },
    // 2.4 m/s: 8.8 m, short of the wall about 10 m along +X from the spot (there the exe's
    // wall handling pulls the camera in).
    CamScenario { name: "stick_yaw_then_run", frames: 240, dt: 1.0 / 60.0, input: |f| (if f < 20 { [0.0, 1.0] } else { [0.0; 2] }, [0.0; 2], if f >= 20 { [0.04, 0.0, 0.0] } else { [0.0; 3] }) },
    CamScenario { name: "stick_full_mostly_yaw", frames: 60, dt: 1.0 / 60.0, input: |_| ([0.3, 0.954], [0.0; 2], [0.0; 3]) },
    CamScenario { name: "stick_full_diagonal_down", frames: 60, dt: 1.0 / 60.0, input: |_| ([-0.7071, 0.7071], [0.0; 2], [0.0; 3]) },
    CamScenario { name: "stick_full_diagonal_long", frames: 300, dt: 1.0 / 60.0, input: |_| ([0.7071, 0.7071], [0.0; 2], [0.0; 3]) },
    CamScenario { name: "stick_pitch_then_full_yaw", frames: 120, dt: 1.0 / 60.0, input: |f| (if f < 60 { [0.5, 0.0] } else { [0.0, 1.0] }, [0.0; 2], [0.0; 3]) },
];

/// Pilot 3b: the follow camera over many frames on the snapshot (FUN_140734790 with the stick
/// gate open, as the in-game caller FUN_140a74dc0 passes it), each scenario from the snapshot's
/// state. Writes every frame's inputs and the ChrExFollowCam fields 0x0..0x420 (as f32) to
/// camera_frames.json for the sim tests.
fn camera_frames(new_emu: &dyn Fn() -> Emu, out: &Path) {
    const F: u64 = 0x1_4073_4790;
    const FIELDS: u64 = 0x420;
    let mut scenarios = Vec::new();
    for sc in &CAM_SCENARIOS {
        let mut emu = new_emu();
        let player = emu.st.snap.as_ref().expect("camera_frames needs --snapshot").player;
        let cam = chr_cam(&mut emu);
        let follow = emu.peek_u64(cam + 0x60);
        let world = emu.peek_u64(0x1_43d6_d640);
        let world = emu.peek_u64(world + 0x98);
        // Wolf's manipulator (FUN_1409efea0) and physics module (position +0x80).
        let chr_ctrl = emu.peek_u64(player + 0x50);
        let m = match emu.peek_u64(chr_ctrl + 0x358) {
            0 => emu.peek_u64(chr_ctrl + 0x120),
            m => m,
        };
        let bag = emu.peek_u64(player + 0x1ff8);
        let phys = emu.peek_u64(bag + 0x68);
        let mut frames = Vec::new();
        let mut stopped = None;
        for f in 0..sc.frames {
            let (stick, mouse, step) = (sc.input)(f);
            emu.poke_f32s(m + 0x80, &stick);
            emu.poke_f32s(m + 0xd0, &mouse);
            let pos: [f32; 3] = std::array::from_fn(|i| f32::from_le_bytes(emu.peek(phys + 0x80 + 4 * i as u64)));
            let pos: [f32; 3] = std::array::from_fn(|i| pos[i] + step[i]);
            emu.poke_f32s(phys + 0x80, &pos);
            let before: Vec<f32> = (0..FIELDS / 4).map(|i| f32::from_le_bytes(emu.peek(follow + 4 * i))).collect();
            let r = emu.call(F, &[cam, 0, player, world, 1], &[0.0, sc.dt]);
            if r.stopped.is_some() {
                stopped = r.stopped;
                break;
            }
            let after: Vec<f32> = (0..FIELDS / 4).map(|i| f32::from_le_bytes(emu.read(follow + 4 * i))).collect();
            frames.push(json!({"stick": stick, "mouse": mouse, "wolf": pos, "before": before, "after": after}));
        }
        let yaw = |v: &Value| v["after"][0x174 / 4].as_f64().unwrap_or(0.0);
        println!(
            "{}: {} frames, yaw {:.3} -> {:.3}, stopped {:?}",
            sc.name,
            frames.len(),
            frames.first().map(yaw).unwrap_or(0.0),
            frames.last().map(yaw).unwrap_or(0.0),
            stopped.map(|(a, e)| (format!("{a:#x}"), e))
        );
        scenarios.push(json!({"name": sc.name, "dt": sc.dt, "frames": frames}));
    }
    let body = json!({"function": format!("{F:#x}"), "fields": format!("ChrExFollowCam 0x0..{FIELDS:#x} as f32"), "scenarios": scenarios});
    std::fs::write(out.join("camera_frames.json"), serde_json::to_string(&body).unwrap()).unwrap();
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
    let mut args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("snapshot") {
        let pid = args.get(2).and_then(|p| p.parse().ok()).expect("usage: emu-cases snapshot <pid> <dir>");
        snapshot::take(pid, Path::new(args.get(3).expect("snapshot dir")));
        return;
    }
    // scene <dir> <idol> [x y z yaw] [ai-off]: warp, place Wolf, snapshot (finds sekiro.exe itself).
    if args.get(1).map(String::as_str) == Some("scene") {
        let usage = "usage: emu-cases scene <dir> <idol> [x y z yaw] [ai-off]";
        let ai_off = args.iter().any(|a| a == "ai-off");
        let rest: Vec<&str> = args[2..].iter().map(String::as_str).filter(|a| *a != "ai-off").collect();
        let dir = Path::new(rest.first().expect(usage));
        let idol = rest.get(1).and_then(|i| i.parse().ok()).expect(usage);
        let f: Vec<f32> = rest[2..].iter().map(|v| v.parse().expect(usage)).collect();
        let spot = match f.len() {
            0 => None,
            4 => Some([f[0], f[1], f[2], f[3]]),
            _ => panic!("{usage}"),
        };
        let pid = snapshot::find_game().expect("sekiro.exe is not running");
        snapshot::scene(pid, &snapshot::Scene { idol, spot, ai_off }, dir);
        return;
    }
    // --snapshot <dir>: run on the game's memory from `snapshot`.
    let snap_dir = args.iter().position(|a| a == "--snapshot").map(|i| {
        let d = args.remove(i + 1);
        args.remove(i);
        d
    });
    let snap = || snap_dir.as_deref().map(|d| snapshot::Snap::open(Path::new(d)));
    let exe = Path::new(args.get(1).expect("usage: emu-cases <sekiro.exe> <out dir> [pilot ...]"));
    let out = Path::new(args.get(2).expect("out dir"));
    std::fs::create_dir_all(out).unwrap();
    let want: Vec<&str> = args[3..].iter().map(String::as_str).collect();
    let on = |n: &str| want.is_empty() || want.contains(&n);
    let t = std::time::Instant::now();
    if want.first() == Some(&"probe") {
        let mut emu = Emu::new(exe, snap());
        for a in &want[1..] {
            probe(&mut emu, u64::from_str_radix(a.trim_start_matches("0x"), 16).unwrap());
        }
        return;
    }
    if want.first() == Some(&"turn") {
        return turn::main(&|| Emu::new(exe, snap()), out, &want[1..]);
    }
    // trace <name> <va> [args...]: integer args in order ("player" = PlayerIns, "wcm" = WorldChrMan,
    // or hex), "x<i>=<f>"
    // for xmm<i>; writes trace_<name>.json.
    if want.first() == Some(&"trace") {
        let mut emu = Emu::new(exe, snap());
        let player = emu.st.snap.as_ref().map(|s| s.player);
        let (mut ints, mut xmm) = (Vec::new(), vec![0.0f32; 4]);
        for a in &want[3..] {
            if let Some((i, v)) = a.strip_prefix('x').and_then(|r| r.split_once('=')) {
                xmm[i.parse::<usize>().unwrap()] = v.parse().unwrap();
            } else if *a == "player" {
                ints.push(player.expect("player needs --snapshot"));
            } else if *a == "wcm" {
                ints.push(emu.peek_u64(0x1_43d7_a1e0));
            } else if let Some((at, v)) = a.strip_prefix('b').and_then(|r| r.split_once('=')) {
                // set a byte in the emulator's copy before the call, e.g. b143d7a376=0 turns the
                // snapshot's disable-AI debug flag (DebugFlags 0x143d7a366 +0x10) off
                emu.raw_write(u64::from_str_radix(at, 16).unwrap(), &[v.parse().unwrap()]);
            } else if let Some(dt) = a.strip_prefix("dt=") {
                // a task call's data block: frame time at +8 (WorldChrManImp's task methods read it there)
                let at = emu.alloc(0x40);
                emu.poke_f32s(at + 8, &[dt.parse().unwrap()]);
                ints.push(at);
            } else {
                ints.push(u64::from_str_radix(a.trim_start_matches("0x"), 16).unwrap());
            }
        }
        let va = u64::from_str_radix(want[2].trim_start_matches("0x"), 16).unwrap();
        trace::trace(&mut emu, va, &ints, &xmm, out, want[1]);
        return;
    }
    if on("super_armor_hit") {
        let mut emu = Emu::new(exe, None);
        super_armor_hit(&mut emu, out);
    }
    if want.contains(&"camera") {
        let mut emu = Emu::new(exe, snap());
        camera(&mut emu, out);
    }
    if want.contains(&"camera_frames") {
        camera_frames(&|| Emu::new(exe, snap()), out);
    }
    if want.contains(&"guard") {
        guard::run(&|| Emu::new(exe, snap()), out);
    }
    if want.iter().any(|w| w.starts_with("camwall")) {
        camwall::run(&|| Emu::new(exe, snap()), out, &want);
    }
    if want.contains(&"hks_env") {
        hks::env_cases(&mut Emu::new(exe, snap()), out);
    }
    if want.iter().any(|w| w.starts_with("damage")) {
        damage::run(&|| Emu::new(exe, snap()), out, &want);
    }
    if want.iter().any(|w| w.starts_with("lockcam")) {
        lockcam::run(&want, &|| Emu::new(exe, snap()), out);
    }
    if want.iter().any(|w| w.starts_with("aicpp")) {
        aicpp::run(&|| Emu::new(exe, snap()), out, &want);
    }
    if want.first().is_some_and(|w| w.starts_with("proxy")) {
        return proxy::run(&|| Emu::new(exe, snap()), out, &want);
    }
    if want.contains(&"taecam") {
        taecam::taecam_frames(&|| Emu::new(exe, snap()), out);
    }
    if want.iter().any(|w| w.starts_with("modticks")) {
        modticks::run(&|| Emu::new(exe, snap()), out, &want);
    }
    if want.contains(&"taewin") {
        taewin::run_cases(&|| Emu::new(exe, snap()), out);
    }
    if want.iter().any(|w| w.starts_with("throw")) {
        throw::run(&|| Emu::new(exe, snap()), out, &want);
    }
    if want.contains(&"speffect") {
        speffect::run(&|| Emu::new(exe, snap()), out);
    }
    if on("hit_stop") {
        let mut emu = Emu::new(exe, None);
        hit_stop(&mut emu, out);
    }
    println!("done in {:.1} s", t.elapsed().as_secs_f32());
}
