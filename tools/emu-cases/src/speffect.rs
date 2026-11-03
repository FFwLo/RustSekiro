//! The SpEffect list tick on the snapshot: SpEffects added to Wolf and one enemy through the
//! game's own add (FUN_1409e9c00, as the goods / TAE callers pass it: source = the chr, four 1.0
//! factors, rate 1.0), then the chr's tick vfunc 44 FUN_140a02a10 (FUN_140a03340: the list tick
//! FUN_140c005b0 -> per entry FUN_140c06b70, vfunc 0x158 when an interval fired; then the HP module
//! FUN_140b658b0 -> FUN_140bfd0a0 / FUN_140c014f0 and posture FUN_140bfe2c0) run frame by frame at
//! 30 and 60 fps. Per frame: HP / posture of the data module ([chr+0x1ff8]+0x18 +0x130 / +0x148) and
//! the list ([chr+0x11d0]+8; entry +0 time left, +8 interval timer, +0x1c flags, +0x25 expired,
//! +0xc..+0x18 the add's factors, +0x58 id, +0x70 next). Writes extracted/emu/speffect_ticks.json for
//! src/sim_tests/emu_speffect.rs.

use super::*;

const ADD: u64 = 0x1_409e_9c00;
/// ChrIns vfunc 44 (FUN_140a02a10): the list tick FUN_140a03340 and the reactions after it (HP
/// change FUN_140b658b0 -> FUN_140bd4d40, posture change FUN_140bfe2c0 -> FUN_140bd4de0).
const TICK: u64 = 0x1_40a0_2a10;
/// General Naomori's NpcParam (the arena's boss; 13207 HP).
const ENEMY_NPC: u32 = 71003080;

fn pu32(emu: &mut Emu, at: u64) -> u32 {
    u32::from_le_bytes(emu.peek(at))
}
fn pf32(emu: &mut Emu, at: u64) -> f32 {
    f32::from_le_bytes(emu.peek(at))
}

/// `Emu::call` without its per-call block hook: the hook added and deleted on every call crashed
/// Unicorn (host segfault) after about 650 calls of vfunc 44 on one engine. Returns where it
/// stopped early, None when the function returned.
pub(super) fn call_quiet(emu: &mut Emu, va: u64, args: &[u64], xmm: &[f32]) -> Option<(u64, i32)> {
    for (r, v) in [uc::RCX, uc::RDX, uc::R8, uc::R9].iter().zip(args) {
        emu.set_reg(*r, *v);
    }
    for (i, v) in xmm.iter().enumerate() {
        let b = f32_bits(*v);
        unsafe { uc::uc_reg_write(emu.uc, uc::XMM0 + i as i32, b.as_ptr() as _) };
    }
    let sp = (STACK + STACK_SIZE - 0x1000 - 8 * args.len().saturating_sub(4) as u64 - 0x28) & !0xf;
    for (i, v) in args.iter().skip(4).enumerate() {
        emu.raw_write(sp + 0x28 + 8 * i as u64, &v.to_le_bytes());
    }
    emu.raw_write(sp, &RETURN.to_le_bytes());
    emu.set_reg(uc::RSP, sp);
    emu.st.running = true;
    emu.st.func = va;
    emu.st.calls.clear();
    let r = unsafe { uc::uc_emu_start(emu.uc, va, RETURN, 0, emu.limit) };
    emu.st.running = false;
    let rip = emu.reg(uc::RIP);
    if r == 0 && rip == RETURN { None } else { Some((rip, r)) }
}

pub(super) struct Scenario {
    pub(super) name: &'static str,
    pub(super) enemy: bool,
    pub(super) fps: u32,
    pub(super) frames: usize,
    /// (frame, SpEffect id): added before that frame's tick.
    pub(super) adds: Vec<(usize, i32)>,
}

/// What a scenario changes in its emulator copy before the first frame: the data module's
/// posture (+0x148; the regens need room) and the worn rows whose disable* bits (row +0x16b bits
/// 2-5, +0x275 bit 0) are cleared (the General's 30201 "[Conversation] damage completely
/// invalid", speffect2).
#[derive(Default)]
pub(super) struct Setup {
    pub(super) posture: Option<u32>,
    pub(super) clear: &'static [i32],
}

fn entries(emu: &mut Emu, chr: u64) -> Vec<Value> {
    let list = emu.peek_u64(chr + 0x11d0);
    let mut out = Vec::new();
    let mut e = if list == 0 { 0 } else { emu.peek_u64(list + 8) };
    while e != 0 && out.len() < 200 {
        out.push(json!([
            pu32(emu, e + 0x58) as i32,
            pf32(emu, e),
            pf32(emu, e + 8),
            pu32(emu, e + 0x1c),
            emu.peek::<1>(e + 0x25)[0],
            // the add's factors (FUN_140c05060 multiplies the estus rate by all four)
            [pf32(emu, e + 0xc), pf32(emu, e + 0x10), pf32(emu, e + 0x14), pf32(emu, e + 0x18)],
        ]));
        e = emu.peek_u64(e + 0x70);
    }
    out
}

fn state(emu: &mut Emu, chr: u64) -> Value {
    let bag = emu.peek_u64(chr + 0x1ff8);
    let data = emu.peek_u64(bag + 0x18);
    // SprjChrResistModule [bag+0x20]: +0x10.. resist left per type, +0x24.. max, +0x38 carry.
    let m = emu.peek_u64(bag + 0x20);
    let resist = (0..0x18u64).map(|k| pu32(emu, m + 4 * k)).collect::<Vec<_>>();
    json!({
        "hp": pu32(emu, data + 0x130) as i32, "hp_max": pu32(emu, data + 0x134) as i32,
        "posture": pu32(emu, data + 0x148) as i32, "posture_max": pu32(emu, data + 0x14c) as i32,
        "sp": entries(emu, chr),
        "resist": resist,
    })
}

fn scenarios() -> Vec<Scenario> {
    let mut v = Vec::new();
    for fps in [30, 60] {
        let f = |s: f32| (s * fps as f32).round() as usize;
        for enemy in [false, true] {
            // Burn (9.9 s, every 1 s: 4 + 0.5 % max HP), poison (Sabimaru 9045: 19.9 s, 12 + 0.75 %),
            // the General's poison 9050, terror (endurance 0, 100 % HP, replace 9250).
            // Build-up rows (registBlood 31 / registIllness): one add per 0.1 s, ten times.
            let ten = |id: i32| (0..10).map(|k| (f(0.1) * k, id)).collect::<Vec<_>>();
            v.push(Scenario { name: "burn_9100", enemy, fps, frames: f(12.0), adds: ten(9100) });
            v.push(Scenario { name: "poison_9045", enemy, fps, frames: f(21.0), adds: vec![(0, 9045)] });
            v.push(Scenario { name: "terror_9200", enemy, fps, frames: f(2.0), adds: ten(9200) });
            // Stacking: the same id again, another row of the same category (10004), a lower
            // categoryPriority row of the terror category.
            v.push(Scenario { name: "burn_again", enemy, fps, frames: f(14.0), adds: [ten(9100), vec![(f(3.5), 9100)]].concat() });
            v.push(Scenario { name: "burn_then_9105", enemy, fps, frames: f(14.0), adds: [ten(9100), ten(9105).into_iter().map(|(k, id)| (k + f(3.5), id)).collect()].concat() });
            v.push(Scenario { name: "poison_and_burn", enemy, fps, frames: f(12.0), adds: [vec![(0, 9045)], ten(9100).into_iter().map(|(k, id)| (k + f(0.5), id)).collect()].concat() });
            // An endurance-0 floor row handing on to its replace (4011 2 % -> 4012 burn).
            v.push(Scenario { name: "floor_4011", enemy, fps, frames: f(11.0), adds: vec![(0, 4011)] });
            // The hit's posture-regen stop: 109450 (endurance 0) -> 109451 (1 s).
            v.push(Scenario { name: "stop_109450", enemy, fps, frames: f(1.5), adds: vec![(0, 109450)] });
        }
        // Wolf's items: the gourd 3000, the regen rows 3021 (pellet) / 3061 + 3062 (ohagi), the
        // persimmon's posture regen 3071, both regens of category 20, the fortifying blood 3051,
        // Ungo's sugar 3421 (its cycle 3422), Ako's sugar 3401, the deflect 105010, the
        // umbrella hold 107650, Ichimonji's 140201 (endurance -1).
        v.push(Scenario { name: "gourd_3000", enemy: false, fps, frames: f(0.5), adds: vec![(0, 3000)] });
        v.push(Scenario { name: "pellet_3021", enemy: false, fps, frames: f(11.0), adds: vec![(0, 3021)] });
        v.push(Scenario { name: "ohagi_3061_3062", enemy: false, fps, frames: f(31.0), adds: vec![(0, 3061), (0, 3062)] });
        v.push(Scenario { name: "persimmon_3071", enemy: false, fps, frames: f(31.0), adds: vec![(0, 3071)] });
        v.push(Scenario { name: "pellet_then_ohagi", enemy: false, fps, frames: f(12.0), adds: vec![(0, 3021), (f(2.0), 3061)] });
        v.push(Scenario { name: "blood_3051", enemy: false, fps, frames: f(31.0), adds: vec![(0, 3051)] });
        v.push(Scenario { name: "ungo_3421", enemy: false, fps, frames: f(31.0), adds: vec![(0, 3421)] });
        v.push(Scenario { name: "ako_3401", enemy: false, fps, frames: f(31.0), adds: vec![(0, 3401), (f(10.0), 3401)] });
        v.push(Scenario { name: "deflect_105010", enemy: false, fps, frames: f(0.3), adds: vec![(0, 105010)] });
        v.push(Scenario { name: "umbrella_107650", enemy: false, fps, frames: f(0.3), adds: vec![(0, 107650)] });
        v.push(Scenario { name: "ichimonji_140201", enemy: false, fps, frames: f(2.0), adds: vec![(0, 140201)] });
    }
    v
}

pub(crate) fn run(new_emu: &dyn Fn() -> Emu, out: &Path) {
    run_set(new_emu, out, "speffect_ticks", scenarios().into_iter().map(|s| (s, Setup::default())).collect());
}

/// Runs `set` and writes extracted/emu/<file>.json (speffect_try.json under SPEFFECT_ONLY).
pub(super) fn run_set(new_emu: &dyn Fn() -> Emu, out: &Path, file: &str, set: Vec<(Scenario, Setup)>) {
    let first = &mut new_emu();
    let all = super::damage::chrs(first);
    let wolf = all[0];
    let Some(enemy) = all.iter().copied().find(|&c| pu32(first, c + 0x68) == ENEMY_NPC) else {
        panic!("speffect: NpcParam {ENEMY_NPC} not in the snapshot");
    };
    let mut runs = Vec::new();
    let mut frames_total = 0;
    // SPEFFECT_ONLY=name,name runs those scenarios only (and writes speffect_try.json).
    let only: Vec<String> = std::env::var("SPEFFECT_ONLY").map(|v| v.split(',').map(str::to_string).collect()).unwrap_or_default();
    for (s, setup) in set.into_iter().filter(|(s, _)| only.is_empty() || only.iter().any(|o| o == s.name)) {
        let emu = &mut new_emu();
        let chr = if s.enemy { enemy } else { wolf };
        if s.enemy {
            // The General waits for his fight with chr +0x1a11 bit 0x20 set, which makes vfunc 0x558
            // (FUN_1409f3300) say "no damage": the SpEffect damage (FUN_140c014f0's last argument)
            // and the build-ups are dropped. Cleared in the emulator copy so his reactions run.
            let b = emu.peek::<1>(chr + 0x1a11)[0];
            emu.raw_write(chr + 0x1a11, &[b & !0x20]);
        }
        for &id in setup.clear {
            let list = emu.peek_u64(chr + 0x11d0);
            let mut e = if list == 0 { 0 } else { emu.peek_u64(list + 8) };
            while e != 0 {
                if pu32(emu, e + 0x58) as i32 == id {
                    let row = emu.peek_u64(e + 0x60);
                    let b = emu.peek::<1>(row + 0x16b)[0];
                    emu.raw_write(row + 0x16b, &[b & !0x3c]);
                    let b = emu.peek::<1>(row + 0x275)[0];
                    emu.raw_write(row + 0x275, &[b & !1]);
                }
                e = emu.peek_u64(e + 0x70);
            }
        }
        if let Some(p) = setup.posture {
            let bag = emu.peek_u64(chr + 0x1ff8);
            let data = emu.peek_u64(bag + 0x18);
            emu.peek::<4>(data + 0x148);
            emu.raw_write(data + 0x148, &p.to_le_bytes());
        }
        let ones = emu.alloc(0x10);
        emu.poke_f32s(ones, &[1.0, 1.0, 1.0, 1.0]);
        let dt = 1.0 / s.fps as f32;
        let start = state(emu, chr);
        let mut frames = Vec::new();
        let mut stopped = None;
        for i in 0..s.frames {
            let mut added = Vec::new();
            for &(at, id) in &s.adds {
                if at == i {
                    let r = emu.call(ADD, &[chr, id as u32 as u64, chr, 0, 0, ones, 0x3f80_0000, 0, 0, 0], &[]);
                    if let Some(st) = r.stopped {
                        stopped = Some(format!("add {id} stopped at {:#x} ({})", st.0, st.1));
                    }
                    added.push(json!({"id": id, "after": state(emu, chr)}));
                }
            }
            if stopped.is_some() {
                break;
            }
            if let Some(st) = call_quiet(emu, TICK, &[chr, 0], &[0.0, dt]) {
                stopped = Some(format!("tick {i} stopped at {:#x} ({})", st.0, st.1));
                break;
            }
            let mut st = state(emu, chr);
            st["added"] = json!(added);
            frames.push(st);
            frames_total += 1;
        }
        if let Some(why) = &stopped {
            println!("speffect: {} ({}, {} fps): {why}", s.name, if s.enemy { "enemy" } else { "wolf" }, s.fps);
        }
        runs.push(json!({
            "name": s.name, "enemy": s.enemy, "fps": s.fps, "dt": dt,
            "adds": s.adds.iter().map(|(f, id)| json!([f, id])).collect::<Vec<_>>(),
            "cleared": setup.clear,
            "start": start, "frames": frames, "stopped": stopped,
        }));
    }
    println!("speffect: {} scenarios, {frames_total} frames", runs.len());
    std::fs::write(
        out.join(if only.is_empty() { format!("{file}.json") } else { "speffect_try.json".to_string() }),
        serde_json::to_string(&json!({"add": format!("{ADD:#x}"), "tick": format!("{TICK:#x}"), "runs": runs})).unwrap(),
    )
    .unwrap();
}
