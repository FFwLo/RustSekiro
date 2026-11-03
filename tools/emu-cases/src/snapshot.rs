//! One memory snapshot of the running game, for functions that need its global objects (camera,
//! managers): `snapshot <pid> <dir>` waits until a save is loaded (the player exists), pauses the
//! game for the copy, and writes `<dir>/memory.bin` plus `<dir>/regions.json` (each committed
//! readable region, the threads' TEBs). The emulator then maps those regions on first touch.
//! The snapshot holds game memory: it stays on this PC (extracted/ is ignored by git).

use std::ffi::c_void;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

use serde_json::{json, Value};

/// WorldChrMan singleton (1.6.0) and its PlayerIns field: non-null once a save is loaded.
const WORLD_CHR_MAN: u64 = 0x1_43d7_a1e0;
const PLAYER_INS: u64 = 0x88;

#[repr(C)]
#[derive(Default)]
struct MemInfo {
    base: u64,
    alloc_base: u64,
    alloc_protect: u32,
    _a: u32,
    size: u64,
    state: u32,
    protect: u32,
    kind: u32,
    _b: u32,
}

#[repr(C)]
struct ThreadEntry {
    size: u32,
    usage: u32,
    id: u32,
    owner: u32,
    base_pri: i32,
    delta_pri: i32,
    flags: u32,
}

#[repr(C)]
struct ProcessEntry {
    size: u32,
    usage: u32,
    pid: u32,
    heap: usize,
    module: u32,
    threads: u32,
    parent: u32,
    pri: i32,
    flags: u32,
    exe: [u16; 260],
}

#[repr(C)]
#[derive(Default)]
struct ThreadInfo {
    exit: i32,
    teb: u64,
    client: [u64; 2],
    affinity: u64,
    prio: [i32; 2],
}

#[link(name = "kernel32")]
extern "system" {
    fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut c_void;
    fn OpenThread(access: u32, inherit: i32, tid: u32) -> *mut c_void;
    fn CloseHandle(h: *mut c_void) -> i32;
    fn VirtualQueryEx(h: *mut c_void, at: u64, info: *mut MemInfo, len: usize) -> usize;
    fn ReadProcessMemory(h: *mut c_void, at: u64, buf: *mut c_void, len: usize, read: *mut usize) -> i32;
    fn WriteProcessMemory(h: *mut c_void, at: u64, buf: *const c_void, len: usize, written: *mut usize) -> i32;
    fn VirtualAllocEx(h: *mut c_void, at: u64, size: usize, kind: u32, protect: u32) -> u64;
    fn VirtualFreeEx(h: *mut c_void, at: u64, size: usize, kind: u32) -> i32;
    fn CreateRemoteThread(h: *mut c_void, attrs: *mut c_void, stack: usize, start: u64, param: *mut c_void, flags: u32, tid: *mut u32) -> *mut c_void;
    fn WaitForSingleObject(h: *mut c_void, ms: u32) -> u32;
    fn VirtualProtectEx(h: *mut c_void, at: u64, size: usize, protect: u32, old: *mut u32) -> i32;
    fn FlushInstructionCache(h: *mut c_void, at: u64, size: usize) -> i32;
    fn Process32FirstW(snap: *mut c_void, e: *mut ProcessEntry) -> i32;
    fn Process32NextW(snap: *mut c_void, e: *mut ProcessEntry) -> i32;
    fn CreateToolhelp32Snapshot(flags: u32, pid: u32) -> *mut c_void;
    fn Thread32First(snap: *mut c_void, e: *mut ThreadEntry) -> i32;
    fn Thread32Next(snap: *mut c_void, e: *mut ThreadEntry) -> i32;
}

#[link(name = "ntdll")]
extern "system" {
    fn NtSuspendProcess(h: *mut c_void) -> i32;
    fn NtResumeProcess(h: *mut c_void) -> i32;
    fn NtQueryInformationThread(h: *mut c_void, class: u32, info: *mut c_void, len: u32, ret: *mut u32) -> i32;
}

fn read_u64(h: *mut c_void, at: u64) -> Option<u64> {
    let mut v = 0u64;
    let mut n = 0usize;
    (unsafe { ReadProcessMemory(h, at, &mut v as *mut u64 as _, 8, &mut n) } != 0 && n == 8).then_some(v)
}

/// The process's threads in creation order (the first is the main thread) with their TEBs.
fn threads(pid: u32) -> Vec<(u32, u64)> {
    let mut out = Vec::new();
    unsafe {
        let snap = CreateToolhelp32Snapshot(4, 0);
        let mut e = ThreadEntry { size: std::mem::size_of::<ThreadEntry>() as u32, usage: 0, id: 0, owner: 0, base_pri: 0, delta_pri: 0, flags: 0 };
        let mut more = Thread32First(snap, &mut e) != 0;
        while more {
            if e.owner == pid {
                let t = OpenThread(0x0800, 0, e.id); // THREAD_QUERY_LIMITED_INFORMATION
                let mut info = ThreadInfo::default();
                if !t.is_null() && NtQueryInformationThread(t, 0, &mut info as *mut ThreadInfo as _, std::mem::size_of::<ThreadInfo>() as u32, std::ptr::null_mut()) == 0 {
                    out.push((e.id, info.teb));
                }
                if !t.is_null() {
                    CloseHandle(t);
                }
            }
            more = Thread32Next(snap, &mut e) != 0;
        }
        CloseHandle(snap);
    }
    out
}

fn sleep_ms(ms: u64) {
    std::thread::sleep(std::time::Duration::from_millis(ms));
}

/// The pid of the running sekiro.exe.
pub fn find_game() -> Option<u32> {
    unsafe {
        let snap = CreateToolhelp32Snapshot(2, 0);
        let mut e: ProcessEntry = std::mem::zeroed();
        e.size = std::mem::size_of::<ProcessEntry>() as u32;
        let mut more = Process32FirstW(snap, &mut e) != 0;
        let mut found = None;
        while more {
            let len = e.exe.iter().position(|&c| c == 0).unwrap_or(260);
            if String::from_utf16_lossy(&e.exe[..len]).eq_ignore_ascii_case("sekiro.exe") {
                found = Some(e.pid);
                break;
            }
            more = Process32NextW(snap, &mut e) != 0;
        }
        CloseHandle(snap);
        found
    }
}

fn player(h: *mut c_void) -> Option<u64> {
    read_u64(h, WORLD_CHR_MAN).filter(|&m| m != 0).and_then(|m| read_u64(h, m + PLAYER_INS)).filter(|&p| p != 0)
}

/// The player's ChrIns once a save is loaded (polls every 2 s).
fn wait_player(h: *mut c_void) -> u64 {
    println!("waiting for a loaded save (player at [{WORLD_CHR_MAN:#x}]+{PLAYER_INS:#x})");
    loop {
        if let Some(p) = player(h) {
            return p;
        }
        sleep_ms(2000);
    }
}

pub fn take(pid: u32, dir: &Path) {
    // PROCESS_QUERY_INFORMATION | PROCESS_VM_READ | PROCESS_SUSPEND_RESUME
    let h = unsafe { OpenProcess(0x0400 | 0x0010 | 0x0800, 0, pid) };
    assert!(!h.is_null(), "cannot open process {pid}");
    let player = wait_player(h);
    // The save is up; give streaming a moment to settle before the copy.
    sleep_ms(15_000);
    copy(h, pid, player, dir);
    unsafe { CloseHandle(h) };
}

/// Pauses the game, copies its memory and TEBs into `dir`, resumes it.
fn copy(h: *mut c_void, pid: u32, player: u64, dir: &Path) {
    std::fs::create_dir_all(dir).unwrap();
    let mut data = std::io::BufWriter::new(File::create(dir.join("memory.bin")).unwrap());
    let mut regions = Vec::new();
    let mut offset = 0u64;
    let mut buf = vec![0u8; 1 << 20];
    let t = std::time::Instant::now();
    unsafe { NtSuspendProcess(h) };
    let tebs = threads(pid);
    let mut at = 0u64;
    let mut info = MemInfo::default();
    while unsafe { VirtualQueryEx(h, at, &mut info, std::mem::size_of::<MemInfo>()) } != 0 {
        let next = info.base + info.size;
        // MEM_COMMIT, readable (not NOACCESS, not GUARD)
        if info.state == 0x1000 && info.protect & 0x101 == 0 && info.protect != 0 {
            // Copy in 1 MB pieces; a piece that cannot be read ends the region there.
            let mut done = 0u64;
            while done < info.size {
                let len = (info.size - done).min(buf.len() as u64) as usize;
                let mut n = 0usize;
                if unsafe { ReadProcessMemory(h, info.base + done, buf.as_mut_ptr() as _, len, &mut n) } == 0 || n != len {
                    break;
                }
                data.write_all(&buf[..len]).unwrap();
                done += len as u64;
            }
            if done > 0 {
                regions.push(json!({"base": info.base, "size": done, "offset": offset, "protect": info.protect, "type": info.kind}));
                offset += done;
            }
        }
        if next <= at {
            break;
        }
        at = next;
    }
    unsafe { NtResumeProcess(h) };
    data.flush().unwrap();
    let index = json!({"pid": pid, "player": player, "threads": tebs.iter().map(|(id, teb)| json!({"id": id, "teb": teb})).collect::<Vec<_>>(), "regions": regions});
    std::fs::write(dir.join("regions.json"), serde_json::to_string(&index).unwrap()).unwrap();
    println!("snapshot: {} regions, {} MB, {} threads, game paused {:.1} s", regions.len(), offset >> 20, tebs.len(), t.elapsed().as_secs_f32());
}

/// Debug flags (SekiroTool DebugFlags, 1.6.0): bytes at +0 player no death, +0x10 disable AI.
const DEBUG_FLAGS: u64 = 0x1_43d7_a366;
/// The game's idol travel (SekiroTool Functions.Warp, 1.6.0): FUN_14068e870(0, idol, 0).
const WARP: u64 = 0x1_4068_e870;
/// The object holding the warp's arrival spot (read by FUN_140832a10 / FUN_140832a30).
const WARP_DEST: u64 = 0x1_43d5_c0d0;

/// Where a scene puts Wolf: an idol, then optionally an exact spot (x, y, z, yaw).
pub struct Scene {
    pub idol: u32,
    pub spot: Option<[f32; 4]>,
    pub ai_off: bool,
}

fn write(h: *mut c_void, at: u64, bytes: &[u8]) {
    let mut n = 0usize;
    let ok = unsafe { WriteProcessMemory(h, at, bytes.as_ptr() as _, bytes.len(), &mut n) };
    assert!(ok != 0 && n == bytes.len(), "write at {at:#x} failed");
}

fn read_f32s<const N: usize>(h: *mut c_void, at: u64) -> [f32; N] {
    let mut v = [0f32; N];
    let mut n = 0usize;
    unsafe { ReadProcessMemory(h, at, v.as_mut_ptr() as _, 4 * N, &mut n) };
    v
}

/// The player's physics module (position +0x80, yaw +0x74).
fn physics(h: *mut c_void) -> Option<u64> {
    player(h).and_then(|p| read_u64(h, p + 0x1ff8)).and_then(|bag| read_u64(h, bag + 0x68)).filter(|&ph| ph != 0)
}

/// The warp's two arrival setters, rewritten to store a fixed spot: the spot sits in a page near the
/// image (rip-relative reach), each setter's own bytes are kept to put back.
struct SpotPatch {
    cave: u64,
    saved: Vec<(u64, Vec<u8>)>,
}

impl SpotPatch {
    const SETTERS: [(u64, u32, u64); 2] = [(0x1_4083_2a30, 0xac0, 0), (0x1_4083_2a10, 0xad0, 0x10)];

    fn install(h: *mut c_void, [x, y, z, yaw]: [f32; 4]) -> SpotPatch {
        // A data page within 2 GB below the image, for `movups xmm0, [rip+disp32]`.
        let cave = (1..0x4000u64)
            .map(|i| 0x1_4000_0000 - i * 0x10000)
            .find_map(|at| Some(unsafe { VirtualAllocEx(h, at, 0x1000, 0x3000, 0x04) }).filter(|&a| a != 0))
            .expect("no page near the image");
        write(h, cave, &[x, y, z, 1.0, 0.0, yaw, 0.0, 0.0].map(f32::to_le_bytes).concat());
        let rel = |from: u64, to: u64| ((to as i64 - from as i64) as i32).to_le_bytes();
        let mut saved = Vec::new();
        for (at, field, data) in Self::SETTERS {
            // mov rax, [WARP_DEST]; movups xmm0, [cave+data]; movdqa [rax+field], xmm0; ret
            let mut code = vec![0x48, 0x8b, 0x05];
            code.extend(rel(at + 7, WARP_DEST));
            code.extend([0x0f, 0x10, 0x05]);
            code.extend(rel(at + 14, cave + data));
            code.extend([0x66, 0x0f, 0x7f, 0x80]);
            code.extend(field.to_le_bytes());
            code.push(0xc3);
            let mut old = vec![0u8; code.len()];
            let mut n = 0usize;
            unsafe { ReadProcessMemory(h, at, old.as_mut_ptr() as _, old.len(), &mut n) };
            // The setter is 19 bytes and int3 padding follows: the 23-byte version must end in it.
            assert!(old[19..].iter().all(|&b| b == 0xcc), "setter at {at:#x} is not the expected code");
            patch_code(h, at, &code);
            saved.push((at, old));
        }
        SpotPatch { cave, saved }
    }

    fn remove(self, h: *mut c_void) {
        for (at, old) in &self.saved {
            patch_code(h, *at, old);
        }
        unsafe { VirtualFreeEx(h, self.cave, 0, 0x8000) };
    }
}

fn patch_code(h: *mut c_void, at: u64, bytes: &[u8]) {
    let mut old = 0u32;
    unsafe {
        assert!(VirtualProtectEx(h, at, bytes.len(), 0x40, &mut old) != 0, "VirtualProtectEx failed");
        write(h, at, bytes);
        VirtualProtectEx(h, at, bytes.len(), old, &mut old);
        FlushInstructionCache(h, at, bytes.len());
    }
}

/// Runs the game's idol warp on a thread of its own: a small stub in the game calls the warp
/// function as the idol menu does.
fn warp(h: *mut c_void, idol: u32) {
    let mut code = vec![0x48, 0x83, 0xec, 0x28, 0x31, 0xc9, 0xba]; // sub rsp,0x28; xor ecx,ecx; mov edx,imm32
    code.extend(idol.to_le_bytes());
    code.extend([0x45, 0x31, 0xc0, 0x48, 0xb8]); // xor r8d,r8d; mov rax,imm64
    code.extend(WARP.to_le_bytes());
    code.extend([0xff, 0xd0, 0x48, 0x83, 0xc4, 0x28, 0xc3]); // call rax; add rsp,0x28; ret
    unsafe {
        let at = VirtualAllocEx(h, 0, 0x1000, 0x3000, 0x40); // MEM_COMMIT|MEM_RESERVE, PAGE_EXECUTE_READWRITE
        assert!(at != 0, "VirtualAllocEx failed");
        write(h, at, &code);
        let t = CreateRemoteThread(h, std::ptr::null_mut(), 0, at, std::ptr::null_mut(), 0, std::ptr::null_mut());
        assert!(!t.is_null(), "CreateRemoteThread failed");
        WaitForSingleObject(t, 10_000);
        CloseHandle(t);
        VirtualFreeEx(h, at, 0, 0x8000);
    }
}

/// Puts Wolf in a scene and takes the snapshot there: no death on, warp to the idol with the
/// arrival spot set to the scene's, wait for the new area, AI off if asked, let it settle, check
/// Wolf is at the spot, copy.
pub fn scene(pid: u32, sc: &Scene, dir: &Path) {
    // QUERY_INFORMATION | VM_READ | VM_WRITE | VM_OPERATION | CREATE_THREAD | SUSPEND_RESUME
    let h = unsafe { OpenProcess(0x0400 | 0x0010 | 0x0020 | 0x0008 | 0x0002 | 0x0800, 0, pid) };
    assert!(!h.is_null(), "cannot open process {pid}");
    wait_player(h);
    write(h, DEBUG_FLAGS, &[1]);
    let old: [f32; 3] = read_f32s(h, physics(h).unwrap() + 0x80);
    let t = std::time::Instant::now();
    // The warp sets its arrival spot through two setters (FUN_140832a30: [WARP_DEST]+0xac0 =
    // x, y, z, 1; FUN_140832a10: +0xad0 = 0, yaw, 0, 0) and reads it in the same frame, so writing
    // the field from outside loses the race (seen 2026-11-03: Wolf came out at the idol). As
    // SekiroTool does, the setters store the scene's spot while the warp loads, then get their own
    // bytes back; the game itself spawns Wolf there.
    let patched = sc.spot.map(|spot| SpotPatch::install(h, spot));
    warp(h, sc.idol);
    let moved = |p: [f32; 3]| (0..3).map(|i| (p[i] - old[i]).abs()).sum::<f32>() > 0.5;
    loop {
        assert!(t.elapsed().as_secs() < 60, "the warp did not finish loading in 60 s");
        sleep_ms(100);
        if physics(h).is_some_and(|ph| moved(read_f32s(h, ph + 0x80))) {
            break;
        }
    }
    sleep_ms(1000);
    if let Some(p) = patched {
        p.remove(h);
    }
    println!("warped to idol {} in {:.1} s", sc.idol, t.elapsed().as_secs_f32());
    if sc.ai_off {
        write(h, DEBUG_FLAGS + 0x10, &[1]);
    }
    sleep_ms(8000);
    let at: [f32; 3] = read_f32s(h, physics(h).unwrap() + 0x80);
    println!("Wolf at {:.2} {:.2} {:.2}", at[0], at[1], at[2]);
    if let Some([x, y, z, _]) = sc.spot {
        assert!((at[0] - x).abs() + (at[2] - z).abs() < 2.0 && (at[1] - y).abs() < 3.0, "Wolf is not at the spot: no snapshot taken");
    }
    let player = player(h).unwrap();
    copy(h, pid, player, dir);
    unsafe { CloseHandle(h) };
}

/// A loaded snapshot: region index plus the open data file, for mapping pages on first touch.
pub struct Snap {
    file: File,
    /// (base, size, file offset), sorted by base.
    regions: Vec<(u64, u64, u64)>,
    pub player: u64,
    pub main_teb: u64,
}

impl Snap {
    pub fn open(dir: &Path) -> Snap {
        let index: Value = serde_json::from_str(&std::fs::read_to_string(dir.join("regions.json")).expect("regions.json")).unwrap();
        let mut regions: Vec<(u64, u64, u64)> = index["regions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| (r["base"].as_u64().unwrap(), r["size"].as_u64().unwrap(), r["offset"].as_u64().unwrap()))
            .collect();
        regions.sort();
        Snap {
            file: File::open(dir.join("memory.bin")).expect("memory.bin"),
            regions,
            player: index["player"].as_u64().unwrap(),
            main_teb: index["threads"][0]["teb"].as_u64().unwrap_or(0),
        }
    }

    /// The snapshot's bytes for the 64 KB window around `addr` (clipped to its region), or None
    /// when the game had nothing there.
    pub fn window(&mut self, addr: u64) -> Option<(u64, Vec<u8>)> {
        let i = self.regions.partition_point(|r| r.0 <= addr).checked_sub(1)?;
        let (base, size, off) = self.regions[i];
        if addr >= base + size {
            return None;
        }
        let from = (addr & !0xffff).max(base);
        let to = ((addr | 0xffff) + 1).min(base + size);
        let mut b = vec![0u8; (to - from) as usize];
        self.file.seek(SeekFrom::Start(off + from - base)).unwrap();
        self.file.read_exact(&mut b).unwrap();
        Some((from, b))
    }

    /// Every region inside [lo, hi): to load an image whole.
    pub fn within(&self, lo: u64, hi: u64) -> Vec<(u64, u64)> {
        self.regions.iter().filter(|r| r.0 >= lo && r.0 + r.1 <= hi).map(|r| (r.0, r.1)).collect()
    }

    pub fn overlaps(&self, lo: u64, hi: u64) -> bool {
        self.regions.iter().any(|r| r.0 < hi && lo < r.0 + r.1)
    }
}
