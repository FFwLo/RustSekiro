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

pub fn take(pid: u32, dir: &Path) {
    // PROCESS_QUERY_INFORMATION | PROCESS_VM_READ | PROCESS_SUSPEND_RESUME
    let h = unsafe { OpenProcess(0x0400 | 0x0010 | 0x0800, 0, pid) };
    assert!(!h.is_null(), "cannot open process {pid}");
    println!("waiting for a loaded save (player at [{WORLD_CHR_MAN:#x}]+{PLAYER_INS:#x})");
    let player = loop {
        if let Some(p) = read_u64(h, WORLD_CHR_MAN).filter(|&m| m != 0).and_then(|m| read_u64(h, m + PLAYER_INS)).filter(|&p| p != 0) {
            break p;
        }
        std::thread::sleep(std::time::Duration::from_secs(2));
    };
    // The save is up; give streaming a moment to settle before the copy.
    std::thread::sleep(std::time::Duration::from_secs(15));
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
    unsafe {
        NtResumeProcess(h);
        CloseHandle(h);
    }
    data.flush().unwrap();
    let index = json!({"pid": pid, "player": player, "threads": tebs.iter().map(|(id, teb)| json!({"id": id, "teb": teb})).collect::<Vec<_>>(), "regions": regions});
    std::fs::write(dir.join("regions.json"), serde_json::to_string(&index).unwrap()).unwrap();
    println!("snapshot: {} regions, {} MB, {} threads, game paused {:.1} s", regions.len(), offset >> 20, tebs.len(), t.elapsed().as_secs_f32());
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

    /// The first 8-aligned address holding `v` (a vtable: finds a live object of that class).
    pub fn find_u64(&mut self, v: u64) -> Option<u64> {
        let needle = v.to_le_bytes();
        let mut buf = Vec::new();
        for &(base, size, off) in &self.regions {
            let mut done = 0;
            while done < size {
                let len = (size - done).min(256 << 20);
                buf.resize(len as usize, 0);
                self.file.seek(SeekFrom::Start(off + done)).unwrap();
                self.file.read_exact(&mut buf).unwrap();
                if let Some(i) = buf.chunks_exact(8).position(|c| c == needle) {
                    return Some(base + done + 8 * i as u64);
                }
                done += len;
            }
        }
        None
    }

    pub fn overlaps(&self, lo: u64, hi: u64) -> bool {
        self.regions.iter().any(|r| r.0 < hi && lo < r.0 + r.1)
    }
}
