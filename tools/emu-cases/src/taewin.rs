//! taewin: the TAE event window. Havok's CustomManualSelectorGenerator (FUN_1411e56e0) calls the
//! registered TAE callback once per update of its selected clip with (anim id, the clip's local
//! time before the step +0xe0, after it +0xe4, clip length +0xe8, chr): FUN_1411e8f30 ->
//! FUN_140b5c920 -> FUN_140b5c580 on the chr's module [[chr+0x1ff8]+0x10]. That looks the anim up
//! (FUN_14098ed70 on module +0x10), then, with the multithreaded-step flag
//! [0x143d92828]+0x58 clear, runs FUN_140654b70 (dispatch now, callback 0x140654a8c); with it set,
//! FUN_140b58420 (in-step handlers, callback 0x140b4fd9c) and queues (id, prev, cur, length) in a
//! ring of 10 at +0x20 that FUN_140b5c730 replays after the step through FUN_140b58530 (callback
//! 0x140b4fda4). All three turn (prev, cur, length) into query windows and FUN_1425e5640 walks
//! the anim's event groups; each event it returns goes to the callback with its window, an
//! "entered" byte (+0x18) and an "ended" byte (+0x19).
//!
//! The run: Wolf's module on the snapshot; the anim entry of his current anim id points at a
//! synthetic event list (emulator copy only); the callbacks are replaced by `ret` and recorded.
//!
//!   emu-cases <exe> <out> taewin --snapshot <dir>   -> taewin_cases.json

use super::*;

const PLAY: u64 = 0x1_40b5_c920;
const REPLAY: u64 = 0x1_40b5_c730;
const LOOKUP: u64 = 0x1_4098_ed70;
/// FD4 singleton DAT_143d92828; its byte +0x58 is the multithreaded-step flag (FUN_141043150).
const MT_SINGLETON: u64 = 0x1_43d9_2828;
const CALLBACKS: [(u64, &str); 3] = [(0x1_4065_4a8c, "direct"), (0x1_40b4_fd9c, "step"), (0x1_40b4_fda4, "replay")];

#[derive(Default)]
struct Log {
    hits: Vec<Value>,
}

fn mem<const N: usize>(e: uc::Engine, at: u64) -> [u8; N] {
    let mut b = [0u8; N];
    unsafe { uc::uc_mem_read(e, at, b.as_mut_ptr() as _, N) };
    b
}
fn rf(e: uc::Engine, at: u64) -> f32 {
    f32::from_le_bytes(mem(e, at))
}

/// At a callback's entry (receiver, event record): type, window, entered, ended, the event.
extern "C" fn on_event(e: uc::Engine, addr: u64, _size: u32, data: *mut c_void) {
    let log = unsafe { &mut *(data as *mut Log) };
    let mut rec = 0u64;
    unsafe { uc::uc_reg_read(e, uc::RDX, &mut rec as *mut u64 as _) };
    let via = CALLBACKS.iter().find(|c| c.0 == addr).map_or("?", |c| c.1);
    let ev = u64::from_le_bytes(mem(e, rec + 0x20));
    let m1 = mem::<2>(e, rec + 0x18);
    log.hits.push(json!({
        "via": via,
        "type": u32::from_le_bytes(mem(e, rec)),
        "a": rf(e, rec + 0x10),
        "b": rf(e, rec + 0x14),
        "entered": m1[0],
        "ended": m1[1],
        "start": rf(e, ev),
        "end": rf(e, ev + 8),
    }));
}

/// A TAE anim as FUN_1425e5640 walks it: +0x08 groups (0x20 each: u16 count, +0x08 u32
/// offsets, +0x18 base), +0x18 -> int 0 (FUN_140654b70's check), +0x24 group count; an event
/// is (start +0, end +8, +0x10 -> (type, data)).
fn fake_anim(emu: &mut Emu, groups: &[&[(f32, f32, u32)]]) -> u64 {
    let anim = emu.alloc(0x30);
    let zero = emu.alloc(0x10);
    emu.u32(zero, 0);
    let garr = emu.alloc(0x20 * groups.len() as u64);
    for (g, evs) in groups.iter().enumerate() {
        let base = emu.alloc(0x28 * evs.len() as u64 + 0x10);
        let offs = emu.alloc(4 * evs.len() as u64 + 4);
        for (i, &(s, en, kind)) in evs.iter().enumerate() {
            let ev = base + 0x28 * i as u64;
            emu.f32(ev, s);
            emu.f32(ev + 4, 0.0);
            emu.f32(ev + 8, en);
            emu.f32(ev + 0xc, 0.0);
            emu.u64(ev + 0x10, ev + 0x18);
            emu.u32(ev + 0x18, kind);
            emu.u32(ev + 0x1c, 0);
            emu.u64(ev + 0x20, 0);
            emu.u32(offs + 4 * i as u64, (ev - base) as u32);
        }
        let ga = garr + 0x20 * g as u64;
        emu.u64(ga, evs.len() as u64);
        emu.u64(ga + 8, offs);
        emu.u64(ga + 0x10, 0);
        emu.u64(ga + 0x18, base);
    }
    emu.write(anim, &[0u8; 0x30]);
    emu.u64(anim + 8, garr);
    emu.u64(anim + 0x18, zero);
    emu.u32(anim + 0x24, groups.len() as u32);
    anim
}

/// The synthetic list (type = 1000 + index), in two groups: start 0, frame-grid boundaries,
/// zero length, shorter than a 60 fps frame, at the clip end, past it.
const EVENTS: [(f32, f32); 13] = [
    (0.0, 0.1),
    (0.1, 0.2),
    (0.2, 0.2),
    (0.25, 0.26),
    (10.0 / 30.0, 0.4),
    (0.5, 1.0),
    (0.9, 1.0),
    (1.0, 1.0),
    (0.0, 1.0),
    (0.95, 1.2),
    (1.0 / 60.0, 2.0 / 60.0),
    (0.0, 0.0),
    (0.4, 0.40001),
];

struct Wolf {
    chr: u64,
    module: u64,
    id: u32,
    entry: u64,
}

fn wolf(emu: &mut Emu) -> Wolf {
    let chr = emu.st.snap.as_ref().expect("taewin needs --snapshot").player;
    let bag = emu.peek_u64(chr + 0x1ff8);
    let module = emu.peek_u64(bag + 0x10);
    let id = u32::from_le_bytes(emu.peek(module + 0xf8));
    let entry = lookup(emu, module, id).expect("Wolf's current anim");
    Wolf { chr, module, id, entry }
}

/// The module's anim entry for an id (FUN_14098ed70 on module +0x10).
fn lookup(emu: &mut Emu, module: u64, id: u32) -> Option<u64> {
    let table = emu.peek_u64(module + 0x10);
    let key = emu.alloc(8);
    emu.u32(key, id);
    let r = emu.call(LOOKUP, &[table, key, 0], &[]);
    assert!(r.stopped.is_none(), "anim lookup of {id} stopped: {:?}", r.stopped);
    (r.rax != 0).then_some(r.rax)
}

/// The real anim's events (start, end, type) read the way FUN_1425e5640 walks them.
fn read_events(emu: &mut Emu, anim: u64) -> Vec<(f32, f32, u32)> {
    let mut out = Vec::new();
    let n = u32::from_le_bytes(emu.peek(anim + 0x24)) as u64;
    let garr = emu.peek_u64(anim + 8);
    for g in 0..n {
        let ga = garr + 0x20 * g;
        let cnt = u16::from_le_bytes(emu.peek(ga)) as u64;
        let offs = emu.peek_u64(ga + 8);
        let base = emu.peek_u64(ga + 0x18);
        for i in 0..cnt {
            let ev = u32::from_le_bytes(emu.peek(offs + 4 * i)) as u64 + base;
            let d = emu.peek_u64(ev + 0x10);
            out.push((f32::from_le_bytes(emu.peek(ev)), f32::from_le_bytes(emu.peek(ev + 8)), if d == 0 { u32::MAX } else { u32::from_le_bytes(emu.peek(d)) }));
        }
    }
    out
}

fn hook(emu: &mut Emu, log: *mut Log) {
    for (at, _) in CALLBACKS {
        emu.raw_write(at, &[0xc3]);
        let mut h = 0usize;
        let r = unsafe { uc::uc_hook_add(emu.uc, &mut h, uc::HOOK_CODE, on_event as *const c_void, log as *mut c_void, at, at) };
        assert_eq!(r, 0);
    }
}

/// The frame windows of a clip run: f32 time stepped by dt (a list per frame), wrapped at the
/// length when looping (hkbClipGenerator MODE_LOOPING), else held at the end.
fn run(start: f32, dts: &[f32], len: f32, looping: bool) -> Vec<(f32, f32)> {
    let mut t = start;
    let mut w = Vec::new();
    for &dt in dts {
        let prev = t;
        t += dt;
        if t >= len {
            t = if looping { t - len } else { len };
        }
        w.push((prev, t));
    }
    w
}

fn rep(dt: f32, n: usize) -> Vec<f32> {
    vec![dt; n]
}

/// A small deterministic generator for the random sweep.
fn lcg(s: &mut u64) -> f32 {
    *s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    ((*s >> 40) as f32) / ((1u64 << 24) as f32)
}

pub(crate) fn run_cases(new_emu: &dyn Fn() -> Emu, out: &Path) {
    let len = 1.0f32;
    let f30 = 1.0f32 / 30.0;
    let f60 = 1.0f32 / 60.0;
    let mut sc: Vec<(String, Vec<(f32, f32)>, f32, bool)> = vec![
        ("fps30_loop".into(), run(0.0, &rep(f30, 40), len, true), len, true),
        ("fps60_loop".into(), run(0.0, &rep(f60, 75), len, true), len, false),
        ("fps30_phase".into(), run(0.005, &rep(f30, 35), len, true), len, false),
        ("fps20".into(), run(0.0, &rep(0.05, 25), len, true), len, false),
        ("fps144".into(), run(0.0, &rep(1.0 / 144.0, 150), len, false), len, false),
        // speed changes: x1, x0.5, x3 (one jump of 0.1 s spans whole events), then x1
        ("speed_change".into(), run(0.0, &[rep(f60, 10), rep(f60 * 0.5, 10), rep(f60 * 3.0, 10), rep(f60, 20)].concat(), len, true), len, false),
        // hit stop: frames of dt 0 at the start and mid-event
        ("hit_stop".into(), run(0.0, &[rep(0.0, 3), rep(f60, 6), rep(0.0, 4), rep(f60, 10), rep(0.0, 2), rep(f60, 3)].concat(), len, false), len, true),
        // a one-shot clip reaching its end and held there
        ("once_end".into(), run(0.9, &rep(f60, 12), len, false), len, false),
    ];
    // anim change: the selector sets prev = cur = 0 (FUN_1411e6de0), then cur = the new clip's time
    let starts = [0.0f32, 1e-7, f60, f30, 0.1, 0.25, 0.5, 0.95, 1.0];
    sc.push(("anim_change".into(), starts.iter().map(|&c| (0.0, c)).collect(), len, true));
    // boundaries exactly on event edges and on the 30 / 60 fps grids
    let mut edges = Vec::new();
    for k in 0..31 {
        edges.push((k as f32 / 30.0, (k + 1) as f32 / 30.0));
    }
    for (s, e) in EVENTS {
        for (p, c) in [(s, e), (s - f60, s), (e - f60, e), (e, e + f60), (s, s), (s - 1e-7, s), (s + 3e-7, s + f60), (e - 3e-7, e + f60), (e + 6e-7, e + f60)] {
            edges.push((p, c));
        }
    }
    sc.push(("edges".into(), edges, len, false));
    // loop wraps (cur < prev)
    sc.push(("wrap".into(), vec![(0.98, 0.0133), (0.99, 0.0), (0.9999, 0.0001), (1.0, 0.016), (0.95, 0.96), (0.5, 0.25), (0.0167, 0.0)], len, true));
    let mut seed = 0x5eed_u64;
    for chunk in 0..3 {
        let w: Vec<(f32, f32)> = (0..150).map(|_| ((lcg(&mut seed) * 1.2 * 30.0).round() / 30.0 * if lcg(&mut seed) < 0.5 { 1.0 } else { 1.0 + 0.01 * lcg(&mut seed) }, lcg(&mut seed) * 1.2)).collect();
        sc.push((format!("random_{chunk}"), w, len, false));
    }

    let mut cases = Vec::new();
    let mut real = Value::Null;
    for (name, windows, len, mt) in &sc {
        let mut emu = new_emu();
        let mut log = Box::new(Log::default());
        hook(&mut emu, &mut *log);
        let w = wolf(&mut emu);
        if real.is_null() {
            // Wolf's own anims: his current one and three with many events (a050_300000 the first
            // slash, a110_316200, a073_401000), 30 fps over 1.5x the last event end, looping.
            let mt_at = emu.peek_u64(MT_SINGLETON) + 0x58;
            emu.peek::<1>(mt_at);
            emu.raw_write(mt_at, &[0]);
            let mut anims = Vec::new();
            for id in [w.id, 50_300_000, 110_316_200, 73_401_000] {
                let Some(entry) = lookup(&mut emu, w.module, id) else {
                    println!("taewin: Wolf has no anim {id}");
                    continue;
                };
                let anim = emu.peek_u64(entry + 8);
                if anim == 0 {
                    continue;
                }
                let evs = read_events(&mut emu, anim);
                let rlen = evs.iter().map(|e| e.1).fold(0.0f32, f32::max).max(1.0);
                let rw = run(0.0, &rep(f30, (rlen * 45.0) as usize), rlen, true);
                let mut frames = Vec::new();
                for &(p, c) in &rw {
                    log.hits.clear();
                    let r = emu.call(PLAY, &[id as u64, 0, 0, 0, w.chr], &[0.0, p, c, rlen]);
                    assert!(r.stopped.is_none(), "real anim stopped at {:?}", r.stopped);
                    frames.push(json!({"prev": p, "cur": c, "hits": std::mem::take(&mut log.hits)}));
                }
                println!("taewin: Wolf's anim {id}: {} events, {} windows", evs.len(), frames.len());
                anims.push(json!({"anim": id, "len": rlen, "events": evs.iter().map(|e| json!([e.0, e.1, e.2])).collect::<Vec<_>>(), "frames": frames}));
            }
            real = Value::Array(anims);
        }
        let half = 7;
        let groups: [&[(f32, f32, u32)]; 2] = [
            &EVENTS[..half].iter().enumerate().map(|(i, e)| (e.0, e.1, 1000 + i as u32)).collect::<Vec<_>>(),
            &EVENTS[half..].iter().enumerate().map(|(i, e)| (e.0, e.1, 1000 + (half + i) as u32)).collect::<Vec<_>>(),
        ];
        let anim = fake_anim(&mut emu, &groups);
        emu.u64(w.entry + 8, anim);
        let mt_at = emu.peek_u64(MT_SINGLETON) + 0x58;
        emu.peek::<1>(mt_at);
        let mut frames = Vec::new();
        for &(p, c) in windows {
            emu.raw_write(mt_at, &[0]);
            log.hits.clear();
            let r = emu.call(PLAY, &[w.id as u64, 0, 0, 0, w.chr], &[0.0, p, c, *len]);
            assert!(r.stopped.is_none(), "{name} stopped at {:?}", r.stopped);
            let direct = std::mem::take(&mut log.hits);
            let mut f = json!({"prev": p, "cur": c, "hits": direct});
            if *mt {
                // the multithreaded step: dispatch in the step, queue, replay after it
                emu.raw_write(mt_at, &[1]);
                let head: [u8; 4] = emu.peek(w.module + 0xe8);
                emu.raw_write(w.module + 0xec, &head);
                let r = emu.call(PLAY, &[w.id as u64, 0, 0, 0, w.chr], &[0.0, p, c, *len]);
                assert!(r.stopped.is_none(), "{name} mt stopped at {:?}", r.stopped);
                let r = emu.call(REPLAY, &[w.module], &[]);
                assert!(r.stopped.is_none(), "{name} replay stopped at {:?}", r.stopped);
                f["mt"] = Value::Array(std::mem::take(&mut log.hits));
            }
            frames.push(f);
        }
        println!("taewin: {name}: {} windows", frames.len());
        cases.push(json!({"name": name, "len": len, "frames": frames}));
    }
    let events: Vec<Value> = EVENTS.iter().enumerate().map(|(i, e)| json!([e.0, e.1, 1000 + i])).collect();
    let body = json!({"function": format!("{PLAY:#x} -> 0x140b5c580 -> 0x140654b70 / 0x140b58420 + {REPLAY:#x}, 0x1425e5640"), "events": events, "cases": cases, "real": real});
    std::fs::write(out.join("taewin_cases.json"), serde_json::to_string(&body).unwrap()).unwrap();
}
