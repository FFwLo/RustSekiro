//! taestate: the selector level of the TAE window. Havok's CustomManualSelectorGenerator keeps its
//! selected clip's TAE times: preUpdate FUN_1411e55e0 stores the clip's local time before the step
//! (+0xe0, from the clip clone's +0x140 through FUN_1411e9f10 -> FUN_1413785f0 / FUN_1413b67b0),
//! update FUN_1411e56e0 the time after it (+0xe4) and calls FUN_1411e8f30 (anim id +0xec, +0xe0,
//! +0xe4, length +0xe8, [hkbCharacter+0x28] = the chr) unless FUN_1411ea620 says the generator is
//! the outgoing one of a transition. On a change of selection (update) and in activate
//! (FUN_1411e4f20) FUN_1411e6de0 reloads id and length and zeroes +0xe0 / +0xe4.
//!
//! The run: Wolf's own selector of his current anim (a000_790010) on the snapshot, its clip
//! clone's local time set per update (the clip is not stepped here); FUN_1411e8f30 runs on into
//! the registered TAE callback FUN_140b5c920 and Wolf's module, the handlers replaced by `ret` and
//! recorded (taewin's hooks), so each update gives the selector's (prev, cur) and the events the
//! exe dispatched for them.
//!
//!   emu-cases <exe> <out> taestate [selector address] --snapshot <dir>   -> taestate_cases.json

use super::*;
use crate::taewin::{self, Log};

const PRE_UPDATE: u64 = 0x1_411e_55e0;
const UPDATE: u64 = 0x1_411e_56e0;
const RESET: u64 = 0x1_411e_6de0;
const TAE: u64 = 0x1_411e_8f30;
const CLONE: u64 = 0x1_4137_85f0;
const OUTGOING: u64 = 0x1_411e_a620;
/// CustomManualSelectorGenerator's vtable (its constructors in FUN_1411e*).
const SELECTOR_VT: u64 = 0x1_42bd_c138;
/// Wolf's CMSG of a000_790010 in the 2026-11-03 Naomori snapshot (found by a scan for the
/// vtable; checked below against his module's current anim id).
const WOLF_SELECTOR: u64 = 0x7df3_e991_6740;

/// At FUN_1411e8f30's entry: (anim id, prev, cur, length, chr) as the selector passes them.
extern "C" fn on_tae(e: uc::Engine, _addr: u64, _size: u32, data: *mut c_void) {
    let log = unsafe { &mut *(data as *mut Log) };
    let mut x = [[0u8; 16]; 4];
    let (mut rcx, mut rsp) = (0u64, 0u64);
    unsafe {
        uc::uc_reg_read(e, uc::RCX, &mut rcx as *mut u64 as _);
        uc::uc_reg_read(e, uc::RSP, &mut rsp as *mut u64 as _);
        for (i, r) in x.iter_mut().enumerate() {
            uc::uc_reg_read(e, uc::XMM0 + i as i32, r.as_mut_ptr() as _);
        }
    }
    let mut chr = [0u8; 8];
    unsafe { uc::uc_mem_read(e, rsp + 0x28, chr.as_mut_ptr() as _, 8) };
    let f = |i: usize| f32::from_le_bytes(x[i][..4].try_into().unwrap());
    log.hits.push(json!({"via": "selector", "id": rcx as u32, "prev": f(1), "cur": f(2), "len": f(3), "chr": u64::from_le_bytes(chr)}));
}

pub(crate) fn run(new_emu: &dyn Fn() -> Emu, out: &Path, want: &[&str]) {
    let sel = want.iter().skip_while(|w| **w != "taestate").nth(1).and_then(|a| u64::from_str_radix(a.trim_start_matches("0x"), 16).ok()).unwrap_or(WOLF_SELECTOR);
    let f30 = 1.0f32 / 30.0;
    // (name, updates): each update is (reset first?, preUpdate?, clip time before, after).
    let steady: Vec<(bool, bool, f32, f32)> = (0..12).map(|i| (false, true, 0.5 + i as f32 * f30, 0.5 + (i + 1) as f32 * f30)).collect();
    let mut sc: Vec<(String, Vec<(bool, bool, f32, f32)>)> = vec![
        ("steady".into(), steady),
        // the clip wraps (hkbClipGenerator MODE_LOOPING) and a hit stop (no step)
        ("wrap_hold".into(), vec![(false, true, 3.95, 3.99), (false, true, 3.99, 0.0233), (false, true, 0.0233, 0.0233), (false, true, 0.0233, 0.0567)]),
    ];
    // activate / a change of selection: FUN_1411e6de0 then update, the new clip already at its
    // start time (StartTime offset); then the next update as usual.
    for off in [0.0f32, 1.0 / 60.0, f30, 0.5, 1.0, 2.0] {
        sc.push((format!("reset_{off:.4}"), vec![(false, true, 3.0, 3.0 + f30), (true, false, 0.0, off), (false, true, off, off + f30)]));
    }
    // the same with a preUpdate between the reset and the update (if the graph ran one)
    sc.push(("reset_pre_1.0".into(), vec![(true, true, 1.0, 1.0), (false, true, 1.0, 1.0 + f30)]));
    // on taewin's synthetic event list (Wolf's anim entry pointed at it): a clip from 0 and clips
    // started at offsets, each run on at 30 fps
    let from = |off: f32, n: usize| -> Vec<(bool, bool, f32, f32)> {
        let mut v = vec![(false, true, 3.0, 3.0 + f30), (true, false, 0.0, off)];
        let mut t = off;
        for _ in 0..n {
            v.push((false, true, t, t + f30));
            t += f30;
        }
        v
    };
    sc.push(("synth_from_0".into(), from(0.0, 36)));
    for off in [1.0 / 60.0, 0.1, 0.25, 0.5, 0.95, 1.0] {
        sc.push((format!("synth_from_{off:.4}"), from(off, 8)));
    }

    let mut cases = Vec::new();
    let mut events = Value::Null;
    let mut outgoing = Value::Null;
    for (name, ups) in &sc {
        let mut emu = new_emu();
        let mut log = Box::new(Log::default());
        taewin::hook(&mut emu, &mut *log);
        let mut h = 0usize;
        assert_eq!(unsafe { uc::uc_hook_add(emu.uc, &mut h, uc::HOOK_CODE, on_tae as *const c_void, &mut *log as *mut Log as *mut c_void, TAE, TAE) }, 0);
        let chr = emu.st.snap.as_ref().expect("taestate needs --snapshot").player;
        let bag = emu.peek_u64(chr + 0x1ff8);
        let module = emu.peek_u64(bag + 0x10);
        let wolf_id = u32::from_le_bytes(emu.peek(module + 0xf8));
        assert_eq!(emu.peek_u64(sel), SELECTOR_VT, "{sel:#x} is not a CustomManualSelectorGenerator");
        let id = u32::from_le_bytes(emu.peek(sel + 0xec));
        assert_eq!(id, wolf_id, "the selector's anim is not Wolf's current one");
        // hkbCharacter: [[chr+0x138]+0x1d88]+0x28 (its +0x28 is the chr, +0x98 the behavior graph)
        let p1 = emu.peek_u64(chr + 0x138);
        let p2 = emu.peek_u64(p1 + 0x1d88);
        let hkb_chr = emu.peek_u64(p2 + 0x28);
        assert_eq!(emu.peek_u64(hkb_chr + 0x28), chr);
        let graph = emu.peek_u64(hkb_chr + 0x98);
        // an hkbContext: +0 character, +8 behavior graph, the rest empty
        let ctx = emu.alloc(0x60);
        emu.write(ctx, &[0u8; 0x60]);
        emu.u64(ctx, hkb_chr);
        emu.u64(ctx + 8, graph);
        let kids = emu.peek_u64(sel + 0x98);
        let k = emu.peek::<1>(sel + 0xfd)[0] as u64;
        let child = emu.peek_u64(kids + 8 * k);
        let r = emu.call(CLONE, &[graph, child], &[]);
        assert!(r.stopped.is_none() && r.rax != 0, "clip clone: {:?}", r.stopped);
        let clip = r.rax;
        let mt_at = emu.peek_u64(taewin::MT_SINGLETON) + 0x58;
        emu.peek::<1>(mt_at);
        emu.raw_write(mt_at, &[0]);
        emu.peek::<4>(clip + 0x140);
        if name.starts_with("synth") {
            let entry = taewin::lookup(&mut emu, module, id).expect("Wolf's anim");
            let evs: Vec<(f32, f32, u32)> = taewin::EVENTS.iter().enumerate().map(|(i, e)| (e.0, e.1, 1000 + i as u32)).collect();
            let anim = taewin::fake_anim(&mut emu, &[&evs[..7], &evs[7..]]);
            emu.u64(entry + 8, anim);
        }
        if events.is_null() {
            let entry = taewin::lookup(&mut emu, module, id).expect("Wolf's anim");
            let anim = emu.peek_u64(entry + 8);
            events = Value::Array(taewin::read_events(&mut emu, anim).iter().map(|e| json!([e.0, e.1, e.2])).collect());
            let res = emu.alloc(8);
            let r = emu.call(OUTGOING, &[res, ctx, sel], &[]);
            assert!(r.stopped.is_none(), "FUN_1411ea620: {:?}", r.stopped);
            outgoing = json!(emu.ru8(r.rax));
        }
        let mut frames = Vec::new();
        for &(reset, pre, t0, t1) in ups {
            log.hits.clear();
            if reset {
                let r = emu.call(RESET, &[sel, ctx], &[]);
                assert!(r.stopped.is_none(), "{name} reset: {:?}", r.stopped);
            }
            if pre {
                emu.raw_write(clip + 0x140, &t0.to_le_bytes());
                let r = emu.call(PRE_UPDATE, &[sel, ctx], &[]);
                assert!(r.stopped.is_none(), "{name} preUpdate: {:?}", r.stopped);
            }
            emu.raw_write(clip + 0x140, &t1.to_le_bytes());
            let r = emu.call(UPDATE, &[sel, ctx], &[0.0, 0.0, f30]);
            assert!(r.stopped.is_none(), "{name} update: {:?}", r.stopped);
            let all = std::mem::take(&mut log.hits);
            let (calls, hits): (Vec<Value>, Vec<Value>) = all.into_iter().partition(|h| h["via"] == "selector");
            frames.push(json!({"reset": reset, "pre": pre, "t0": t0, "t1": t1, "calls": calls, "hits": hits,
                "sel": [emu.rf32(sel + 0xe0), emu.rf32(sel + 0xe4), emu.rf32(sel + 0xe8)]}));
        }
        println!("taestate: {name}: {} updates", frames.len());
        cases.push(json!({"name": name, "frames": frames}));
    }
    let body = json!({"function": format!("{PRE_UPDATE:#x} / {UPDATE:#x} / {RESET:#x} -> {TAE:#x} -> 0x140b5c920"), "selector": format!("{sel:#x}"),
        "outgoing": outgoing, "events": events, "synth_events": taewin::EVENTS.iter().enumerate().map(|(i, e)| json!([e.0, e.1, 1000 + i])).collect::<Vec<_>>(), "cases": cases});
    std::fs::write(out.join("taestate_cases.json"), serde_json::to_string(&body).unwrap()).unwrap();
}
