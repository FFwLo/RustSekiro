//! WorldBackRead's navmesh records (board row E34): the demand FUN_14081f020 (rcx owner, rdx the
//! distance set, r8 the navmesh, xmm3 its distance, fifth arg the debug draw flag) and the per-frame
//! state step FUN_14081eae0 (rcx owner, xmm1 dt, r8 the set), run on sets and fake navmeshes built
//! here, with no snapshot. Each scenario is a run of frames: the frame's navmesh query results go
//! through the demand for the set, then the step runs once.
//!
//! The set (floats): +0x0 start distance, +0x4 end distance, +0x8 promote time, +0xc retire time,
//! +0x18 the record list head, +0x20 32 group masks of 0x20, +0x420 a debug-draw owner.
//! A record (0x30, from the heap at DAT_143d9ce48 slot 0x50): +0x0 next, +0x8 / +0xc the navmesh's
//! two ids (+0x128 / +0x12c of the navmesh), +0x10 the navmesh, +0x18 demanded this frame, +0x19
//! demanded last step, +0x1a state (1 pending, 2 active, 3 retiring), +0x1c timer, +0x20 distance,
//! +0x28 debug-draw object. A navmesh is usable when +0x134 != -1, +0x158 bit 0 and +0x30 == 1.
//!
//! The step sorts the records by distance in a temporary tree (heap from FUN_1419defb0, slots 0x18
//! flags, 0x50 alloc, 0x68 free) and frees a deleted record through FUN_14115b7f0's heap; both are
//! fake heaps here whose alloc / free are hooked, so the deletions are the record frees.
//!
//! `worldbackread` writes extracted/emu/worldbackread.json for crates/game map_world's test
//! `back_read_matches_the_exe`.

use std::ffi::c_void;
use std::path::Path;
use std::sync::Mutex;

use serde_json::{json, Value};

use super::{uc, Emu};

const DEMAND: u64 = 0x1_4081_f020;
const STEP: u64 = 0x1_4081_eae0;
/// The step's temporary-tree heap getter and the record heap lookup, both replaced by stubs.
const TREE_HEAP: u64 = 0x1_419d_efb0;
const RECORD_HEAP_OF: u64 = 0x1_4115_b7f0;
/// The heap records are allocated from.
const RECORD_HEAP: u64 = 0x1_43d9_ce48;
/// The navmesh table FUN_1409c1090 looks a record's ids up in (for its group masks).
const NAV_TABLE: u64 = 0x1_43d7_7fc0;
const NAV_LOOKUP: u64 = 0x1_409c_1090;
/// Debug-draw switch: records get a draw object when it is set.
const DEBUG_DRAW: u64 = 0x1_43f4_2800;

const CODE: u64 = 0x7300_0000_0000;
const ARENA: u64 = 0x7400_0000_0000;
const ARENA_SIZE: u64 = 0x100_0000;

#[derive(Default)]
struct Log {
    next: u64,
    frees: Vec<(u64, u64)>,
    lookups: Vec<(i32, i32)>,
}
static LOG: Mutex<Log> = Mutex::new(Log { next: 0, frees: Vec::new(), lookups: Vec::new() });

fn reg(e: uc::Engine, r: i32) -> u64 {
    let mut v = 0u64;
    unsafe { uc::uc_reg_read(e, r, &mut v as *mut u64 as _) };
    v
}

extern "C" fn on_alloc(e: uc::Engine, _addr: u64, _size: u32, _d: *mut c_void) {
    let size = reg(e, uc::RDX);
    let mut log = LOG.lock().unwrap();
    let at = (log.next + 15) & !15;
    log.next = at + size.max(16);
    assert!(log.next <= ARENA + ARENA_SIZE, "worldbackread arena full");
    let zero = vec![0u8; size as usize];
    unsafe {
        uc::uc_mem_write(e, at, zero.as_ptr() as _, zero.len());
        uc::uc_reg_write(e, uc::RAX, &at as *const u64 as _);
    }
}

extern "C" fn on_free(e: uc::Engine, _addr: u64, _size: u32, _d: *mut c_void) {
    LOG.lock().unwrap().frees.push((reg(e, uc::RCX), reg(e, uc::RDX)));
}

extern "C" fn on_lookup(e: uc::Engine, _addr: u64, _size: u32, _d: *mut c_void) {
    LOG.lock().unwrap().lookups.push((reg(e, uc::RDX) as u32 as i32, reg(e, uc::R8) as u32 as i32));
}

struct World {
    emu: Emu,
    tree_heap: u64,
    record_heap: u64,
    navs: Vec<u64>,
}

/// A fake heap: vtable slot 0x18 -> flags with bit 5, 0x50 alloc, 0x68 free.
fn heap(emu: &mut Emu, alloc: u64, free: u64) -> u64 {
    let flags = emu.alloc(16);
    emu.u32(flags, 0x20);
    let flags_fn = emu.stub("heap_flags", flags, None);
    let vt = emu.vtable(&[(0x18, flags_fn), (0x50, alloc), (0x68, free)], 0x100);
    let h = emu.alloc(0x40);
    emu.u64(h, vt);
    h
}

fn world(new_emu: &dyn Fn() -> Emu, navs: usize) -> World {
    let mut emu = new_emu();
    emu.map(CODE, 0x1000);
    emu.raw_write(CODE, &[0xc3u8; 0x1000]);
    emu.map(ARENA, ARENA_SIZE);
    *LOG.lock().unwrap() = Log { next: ARENA, ..Default::default() };
    emu.hook(uc::HOOK_CODE, on_alloc as *const c_void, CODE, CODE);
    emu.hook(uc::HOOK_CODE, on_free as *const c_void, CODE + 0x10, CODE + 0x10);
    emu.hook(uc::HOOK_CODE, on_lookup as *const c_void, NAV_LOOKUP, NAV_LOOKUP);
    let tree_heap = heap(&mut emu, CODE, CODE + 0x10);
    let record_heap = heap(&mut emu, CODE, CODE + 0x10);
    emu.stub_at(TREE_HEAP, "tree_heap", tree_heap, None);
    emu.stub_at(RECORD_HEAP_OF, "record_heap_of", record_heap, None);
    emu.raw_write(RECORD_HEAP, &record_heap.to_le_bytes());
    // An empty navmesh table: the mask lookup finds nothing (its calls are logged).
    let table = emu.alloc(0x40);
    emu.raw_write(NAV_TABLE, &table.to_le_bytes());
    assert_eq!(u64::from_le_bytes(emu.read(DEBUG_DRAW)), 0, "debug draw on in the image");
    let navs = (0..navs)
        .map(|i| {
            let n = emu.alloc(0x160);
            emu.u32(n + 0x30, 1);
            emu.u32(n + 0x128, i as u32);
            emu.u32(n + 0x12c, 0);
            emu.u32(n + 0x134, 0);
            emu.u8(n + 0x158, 1);
            n
        })
        .collect();
    World { emu, tree_heap, record_heap, navs }
}

struct Scenario {
    name: String,
    set: [f32; 4],
    navs: usize,
    /// Per frame: dt and the query's (navmesh, distance, usable).
    frames: Vec<(f32, Vec<(usize, f32, bool)>)>,
}

fn run_scenario(new_emu: &dyn Fn() -> Emu, s: &Scenario) -> Value {
    let mut w = world(new_emu, s.navs);
    let set = w.emu.alloc(0x440);
    for (i, v) in s.set.iter().enumerate() {
        w.emu.f32(set + 4 * i as u64, *v);
    }
    w.emu.u64(set + 0x18, 0);
    w.emu.u64(set + 0x420, 0);
    let mut frames = Vec::new();
    for (dt, query) in &s.frames {
        for &(nav, dist, usable) in query {
            let n = w.navs[nav];
            w.emu.u32(n + 0x134, if usable { 0 } else { u32::MAX });
            let r = w.emu.call(DEMAND, &[0, set, n, 0, 0], &[0.0, 0.0, 0.0, dist]);
            assert!(r.stopped.is_none(), "{}: demand stopped {:?}", s.name, r.stopped);
        }
        let records_before: Vec<(u64, u64)> = list(&w.emu, set).into_iter().map(|r| (r, u64::from(u32::from_le_bytes(w.emu.read(r + 8))))).collect();
        {
            let mut log = LOG.lock().unwrap();
            log.frees.clear();
            log.lookups.clear();
        }
        let r = w.emu.call(STEP, &[0, 0, set], &[0.0, *dt]);
        assert!(r.stopped.is_none(), "{}: step stopped {:?}", s.name, r.stopped);
        let log = LOG.lock().unwrap();
        let deleted: Vec<u64> = log
            .frees
            .iter()
            .filter(|(h, _)| *h == w.record_heap)
            .map(|(_, p)| records_before.iter().find(|(r, _)| r == p).expect("freed a record not in the list").1)
            .collect();
        let tree_frees = log.frees.iter().filter(|(h, _)| *h == w.tree_heap).count();
        assert_eq!(tree_frees, records_before.len() + 1, "{}: the step's tree was not freed whole", s.name);
        let lookups: Vec<i32> = log.lookups.iter().map(|l| l.0).collect();
        drop(log);
        let records: Vec<Value> = list(&w.emu, set)
            .into_iter()
            .map(|r| {
                json!({
                    "nav": u32::from_le_bytes(w.emu.read(r + 8)),
                    "state": w.emu.ru8(r + 0x1a),
                    "timer": w.emu.rf32(r + 0x1c),
                    "distance": w.emu.rf32(r + 0x20),
                    "demanded": w.emu.ru8(r + 0x19),
                })
            })
            .collect();
        frames.push(json!({
            "dt": dt,
            "query": query.iter().map(|&(n, d, u)| json!([n, d, u])).collect::<Vec<_>>(),
            "records": records,
            "deleted": deleted,
            "masked": lookups,
            "start": w.emu.rf32(set),
        }));
    }
    json!({"name": s.name, "set": s.set, "frames": frames})
}

/// The record list from the set's head, in list order.
fn list(emu: &Emu, set: u64) -> Vec<u64> {
    let mut v = Vec::new();
    let mut at = u64::from_le_bytes(emu.read(set + 0x18));
    while at != 0 {
        v.push(at);
        assert!(v.len() < 1000, "record list loops");
        at = u64::from_le_bytes(emu.read(at));
    }
    v
}

const DEFAULT: [f32; 4] = [0.0, 1.0, 0.2, 3.0];
const HIT: [f32; 4] = [80.0, 90.0, 0.2, 3.0];

struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 33) as u32
    }
    fn f(&mut self) -> f32 {
        self.next() as f32 / (1u64 << 31) as f32
    }
}

fn scenarios() -> Vec<Scenario> {
    let mut v = Vec::new();
    let steady = |dt: f32, n: usize, q: Vec<(usize, f32, bool)>| (0..n).map(|_| (dt, q.clone())).collect::<Vec<_>>();
    // One navmesh walked through every distance band of a set, then left, then back.
    for (name, set, path) in [
        ("default_walk", DEFAULT, vec![0.0, 0.0, 0.5, 1.0, 1.2, 0.0, 0.5, 2.0]),
        ("hit_walk", HIT, vec![100.0, 85.0, 79.0, 40.0, 85.0, 89.9, 90.0, 95.0, 85.0, 80.0, 70.0, 95.0, 120.0]),
    ] {
        let mut frames = Vec::new();
        for d in path {
            frames.extend(steady(1.0 / 30.0, 12, vec![(0, d, true)]));
        }
        frames.extend(steady(1.0 / 30.0, 100, vec![]));
        v.push(Scenario { name: name.into(), set, navs: 1, frames });
    }
    // Demand gone for less and more than the retire time, at 30 and 60 fps and in big steps.
    for (fps_name, dt) in [("30", 1.0 / 30.0), ("60", 1.0 / 60.0), ("big", 0.7)] {
        for gone in [1.0f32, 2.9, 3.1, 5.0] {
            let n = (gone / dt).round() as usize;
            let mut frames = steady(dt, (1.0 / dt) as usize + 1, vec![(0, 0.0, true)]);
            frames.extend(steady(dt, n, vec![]));
            frames.extend(steady(dt, 4, vec![(0, 0.0, true)]));
            frames.extend(steady(dt, (4.0 / dt) as usize + 2, vec![]));
            v.push(Scenario { name: format!("retire_{fps_name}_{gone}"), set: DEFAULT, navs: 1, frames });
        }
    }
    // The 5 m wait: several new records at once, near and far, with active ones between.
    for (name, dists) in [
        ("wait_near_far", vec![3.0, 10.0, 20.0, 60.0]),
        ("wait_all_far", vec![6.0, 7.0, 30.0]),
        ("wait_ties", vec![10.0, 10.0, 10.0, 4.0]),
        ("wait_zero", vec![0.0, 0.0, 50.0]),
    ] {
        let q: Vec<_> = dists.iter().enumerate().map(|(i, &d)| (i, d, true)).collect();
        let mut frames = steady(1.0 / 30.0, 30, q.clone());
        // a far one joins once the others are active
        let mut q2 = q.clone();
        q2.push((dists.len(), 70.0, true));
        frames.extend(steady(1.0 / 30.0, 30, q2));
        v.push(Scenario { name: name.into(), set: HIT, navs: dists.len() + 1, frames });
    }
    // An unusable navmesh is not demanded; a usable one beyond the end distance neither.
    {
        let mut frames = steady(1.0 / 30.0, 20, vec![(0, 10.0, true), (1, 10.0, true)]);
        frames.extend(steady(1.0 / 30.0, 40, vec![(0, 10.0, false), (1, 91.0, true)]));
        frames.extend(steady(1.0 / 30.0, 80, vec![(0, 10.0, true), (1, 50.0, true)]));
        v.push(Scenario { name: "unusable".into(), set: HIT, navs: 2, frames });
    }
    // Odd sets: start past end - 1 (clamped by the step), long promote, zero retire.
    for (name, set) in [
        ("odd_clamp", [50.0, 40.0, 0.2, 3.0]),
        ("odd_promote", [20.0, 30.0, 1.5, 0.5]),
        ("odd_zero", [5.0, 6.0, 0.0, 0.0]),
    ] {
        let mut r = Lcg(name.len() as u64 * 77);
        let mut frames = Vec::new();
        for _ in 0..90 {
            let mut q = Vec::new();
            for i in 0..4 {
                if r.f() < 0.7 {
                    q.push((i, r.f() * 60.0, true));
                }
            }
            frames.push((1.0 / 30.0, q));
        }
        v.push(Scenario { name: name.into(), set, navs: 4, frames });
    }
    // Random walks: navmeshes drift in and out of both sets, demand flickers, frame times vary.
    for seed in 0..24u64 {
        let mut r = Lcg(seed * 7919 + 1);
        let set = if seed % 2 == 0 { HIT } else { DEFAULT };
        let scale = if seed % 2 == 0 { 110.0 } else { 3.0 };
        let navs = 3 + (seed % 5) as usize;
        let mut dist: Vec<f32> = (0..navs).map(|_| r.f() * scale).collect();
        let mut frames = Vec::new();
        let n = 70 + (seed % 3) as usize * 20;
        for f in 0..n {
            let dt = match r.next() % 6 {
                0 => 1.0 / 60.0,
                1 => 0.1,
                2 => 0.5 + r.f(),
                _ => 1.0 / 30.0,
            };
            for d in dist.iter_mut() {
                *d = (*d + (r.f() - 0.5) * scale * 0.15).clamp(0.0, scale);
                if r.next() % 13 == 0 {
                    // snap to a band edge
                    *d = [set[0], set[1], set[0] + 0.001, 5.0][(r.next() % 4) as usize];
                }
            }
            let gone = f % 25 > 18;
            let mut q = Vec::new();
            for (i, &d) in dist.iter().enumerate() {
                if !gone && r.f() < 0.85 {
                    q.push((i, d, r.f() < 0.95));
                }
            }
            frames.push((dt, q));
        }
        v.push(Scenario { name: format!("random_{seed}"), set, navs, frames });
    }
    v
}

pub(crate) fn run(new_emu: &dyn Fn() -> Emu, out: &Path) {
    let t = std::time::Instant::now();
    let cases: Vec<Value> = scenarios().iter().map(|s| run_scenario(new_emu, s)).collect();
    let frames: usize = cases.iter().map(|c| c["frames"].as_array().unwrap().len()).sum();
    let body = json!({"demand": format!("{DEMAND:#x}"), "step": format!("{STEP:#x}"), "cases": cases});
    std::fs::write(out.join("worldbackread.json"), serde_json::to_string(&body).unwrap()).unwrap();
    println!("worldbackread: {} scenarios, {frames} frames in {:.1} s", body["cases"].as_array().unwrap().len(), t.elapsed().as_secs_f32());
}
