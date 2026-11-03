//! The enemy AI frame on the snapshot (topic aiframe): the targeting system's update
//! (SprjTargetingSystem vfunc 0x10 = FUN_14061ff40) over many frames with Wolf moved in and out
//! of sight, the sight ray (FUN_14061c8a0) and nose sphere (FUN_14061c940), the `ai:` queries on
//! TARGET_ENE_0 once a target is held, and the goal -> manipulator path (FUN_1407e67a0).

use std::path::Path;

use serde_json::{json, Value};

use super::aicpp::{ai_chain, f32s, observers, physics, pos, sight, GET_DIST, GET_TO_TARGET_ANGLE};
use super::speffect::call_quiet;
use super::{uc, Emu};

/// SprjTargetingSystem::update (vtable 0x1429635c0 slot 0x10 = FUN_14061ff40), dt in xmm1.
const TS_UPDATE: u64 = 0x1_4061_ff40;
/// FUN_140625cd0(ts): the around meter (its points at +0), or null.
const TS_METER: u64 = 0x1_4062_5cd0;
/// GetOriginDist FUN_1405e3fb0 (aicpp).
const GET_ORIGIN_DIST: u64 = 0x1_405e_3fb0;
/// FUN_14061c8a0(ctx, &point): the sight ray from ctx +0x30 to the point; true = clear.
const SIGHT_RAY: u64 = 0x1_4061_c8a0;
/// FUN_14061c940(ctx): the nose sphere.
const NOSE: u64 = 0x1_4061_c940;
/// ComManipulator vfunc 0x48 (a jump to FUN_1407e67a0): (M, dt, ChrCtrl), runs the AiThink update
/// FUN_140608990 (the Lua goals) and turns its move point and angles into the manipulator's output.
const MANIP_UPDATE: u64 = 0x1_407e_67a0;

extern "C" {
    fn uc_ctl(uc: uc::Engine, control: u32, ...) -> i32;
}

/// Drops Unicorn's translated code between frames (as proxy.rs: the host crashed after about
/// 900 targeting frames on one engine without it).
fn flush_tb(emu: &Emu) {
    unsafe { uc_ctl(emu.uc, 10 | (1 << 30)) };
}

pub fn run(new_emu: &dyn Fn() -> Emu, out: &Path, want: &[&str]) {
    if want.contains(&"aiframe_explore") {
        explore(&mut new_emu());
    }
    if want.contains(&"aiframe") || want.contains(&"aiframe_targeting") {
        targeting_frames(new_emu, out);
    }
    if want.contains(&"aiframe") || want.contains(&"aiframe_ray") {
        ray_cases(new_emu, out);
    }
}

/// The target list (ts +0x18 head, nodes linked by +0x10): (kind = node vfunc 0x10, the slot's
/// forget timer +0x2c, its +0x34 timer, the bytes +0x40..+0x43).
fn nodes(emu: &mut Emu, ts: u64) -> Vec<Value> {
    let mut out = Vec::new();
    let mut n = emu.peek_u64(ts + 0x18);
    while n != 0 && out.len() < 16 {
        let vt = emu.peek_u64(n);
        let kf = emu.peek_u64(vt + 0x10);
        let kind = call_quiet_rax(emu, kf, &[n]) as u32;
        let t: [f32; 3] = f32s(emu, n + 0x2c);
        let b: [u8; 4] = emu.peek(n + 0x40);
        out.push(json!([kind, t[0], t[2], b[0], b[1], b[2], b[3]]));
        n = emu.peek_u64(n + 0x10);
    }
    out
}

fn call_quiet_rax(emu: &mut Emu, va: u64, args: &[u64]) -> u64 {
    assert!(call_quiet(emu, va, args, &[]).is_none(), "stopped in {va:#x}");
    emu.reg(uc::RAX)
}

fn call_quiet_xmm0(emu: &mut Emu, va: u64, args: &[u64]) -> f32 {
    assert!(call_quiet(emu, va, args, &[]).is_none(), "stopped in {va:#x}");
    let mut x0 = [0u8; 16];
    unsafe { uc::uc_reg_read(emu.uc, uc::XMM0, x0.as_mut_ptr() as _) };
    f32::from_le_bytes(x0[..4].try_into().unwrap())
}

/// One frame's record.
fn frame(emu: &mut Emu, ts: u64, wrap: u64, dt: f32, wolf: [f32; 3], query: bool) -> Value {
    let st: [i32; 4] = std::array::from_fn(|i| i32::from_le_bytes(emu.peek(ts + 0x158 + 4 * i as u64)));
    let flags = u32::from_le_bytes(emu.peek(ts + 0x150));
    let timer = f32s::<1>(emu, ts + 0x28)[0];
    let m = call_quiet_rax(emu, TS_METER, &[ts]);
    let meter = if m != 0 { f32s::<1>(emu, m)[0] } else { -1.0 };
    let mut v = json!({
        "dt": dt, "wolf": wolf, "state": st[0], "prev_state": st[1], "kind": st[2], "prev_kind": st[3],
        "flags": flags, "timer": timer, "meter": meter, "nodes": nodes(emu, ts),
    });
    if query {
        // TARGET_ENE_0 (0) with whatever the targeting system holds.
        v["ene0"] = json!([
            call_quiet_xmm0(emu, GET_DIST, &[wrap, 0]),
            call_quiet_xmm0(emu, GET_ORIGIN_DIST, &[wrap, 0]),
            call_quiet_xmm0(emu, GET_TO_TARGET_ANGLE, &[wrap, 0]),
        ]);
    }
    v
}

/// Wolf's spot as (distance ahead of the sight origin, angle to the right in degrees, height).
type Place = (f32, f32, f32);

/// A scenario: (name, [(seconds, dt, place)]), Wolf held at each place for its time.
type Scenario = (String, Vec<(f32, f32, Place)>);

fn row(params: &Value, id: u32) -> Value {
    params["rows"].as_array().unwrap().iter().find(|r| r["id"].as_u64() == Some(id as u64)).cloned().unwrap()
}

fn scenarios(think: &Value) -> Vec<Scenario> {
    let g = |k: &str| think[k].as_f64().unwrap() as f32;
    let normal_far = g("eye_dist_normal") - g("eye_BackOffsetDist_normal");
    let around_far = g("eye_dist_around") - g("eye_BackOffsetDist_around");
    let battle = g("BattleStartDist");
    let forget = g("SightTargetForgetTime");
    let around_mid = (normal_far + around_far) * 0.5;
    let seen = (normal_far * 0.5, 0.0, 0.0);
    let away = (70.0, 180.0, 0.0);
    let f = 1.0 / 30.0;
    let mut out: Vec<Scenario> = vec![
        // Seen, then out of sight until forgotten, then the memory slot runs out (4 Hz tail).
        ("seen_lost".into(), vec![(1.0, f, seen), (forget + 3.0, f, away), (g("MemoryTargetForgetTime") + 2.0, 0.25, away)]),
        // The around meter fills to an indication target, then it is forgotten.
        ("around_fill".into(), vec![(6.0, f, (around_mid, 0.0, 0.0)), (g("IndicationTargetForgetTime").min(30.0) + 2.0, f, away)]),
        // The meter drains out of the cone and fills again.
        ("around_drain".into(), vec![(2.0, f, (around_mid, 0.0, 0.0)), (3.0, f, away), (1.5, f, (around_mid, 0.0, 0.0)), (0.5, f, away), (4.0, f, (around_mid, 0.0, 0.0))]),
        // In and out of the normal cone every 0.4 s.
        ("flicker".into(), (0..10).flat_map(|_| [(0.4, f, seen), (0.4, f, away)]).collect()),
        // Above the cone's upper angle, then in it (the perceive set is taller once found).
        ("perceive".into(), vec![(1.0, f, (normal_far * 0.5, 0.0, normal_far * 0.5 * 0.5)), (1.0, f, seen), (2.0, f, (normal_far * 0.5, 0.0, normal_far * 0.5 * 0.5)), (1.0, f, away)]),
    ];
    if battle < normal_far {
        // Found beyond BattleStartDist (FIND), then inside it (BATTLE).
        let mid = (battle + normal_far) * 0.5;
        out.push(("find_then_battle".into(), vec![(2.0, f, (mid, 0.0, 0.0)), (1.0, f, (battle * 0.5, 0.0, 0.0)), (1.0, f, (mid, 0.0, 0.0))]));
    }
    // Wolf circling the sight origin at 60 deg/s, 12 s (one place per frame).
    out.push(("circle".into(), (0..360).map(|i| (f, f, (normal_far * 0.6, i as f32 * 2.0, 0.0))).collect()));
    out
}

fn targeting_frames(new_emu: &dyn Fn() -> Emu, out: &Path) {
    let params: Value = serde_json::from_str(&std::fs::read_to_string(out.join("../json/params/NpcThinkParam.json")).unwrap()).unwrap();
    let mut rows = Vec::new();
    let picks = [11500001u32, 12400000, 10100304];
    let all = observers(&mut new_emu());
    for think_id in picks {
        let Some(oi) = all.iter().position(|o| o.0 == think_id) else { continue };
        let think = row(&params, think_id);
        let mut list = scenarios(&think);
        // The nose sphere: NpcThinkParam nose_dist (row +0x36) set to 3 m in the emulator's copy,
        // Wolf 2 m behind (around and normal cones miss him), then away.
        list.push(("nose".into(), vec![(2.0, 1.0 / 30.0, (2.0, 180.0, 0.0)), (2.0, 1.0 / 30.0, (70.0, 180.0, 0.0))]));
        for (name, steps) in list {
            let t0 = std::time::Instant::now();
            let mut emu = new_emu();
            let (_, e) = observers(&mut emu)[oi];
            let wolf = emu.st.snap.as_ref().unwrap().player;
            let (_, ai, ts) = ai_chain(&mut emu, e).unwrap();
            let (ctx, _, _) = sight(&mut emu, ts, wolf, false);
            if name == "nose" {
                let w = emu.peek_u64(ctx);
                let r = emu.peek_u64(w + 8);
                emu.peek::<2>(r + 0x36);
                emu.raw_write(r + 0x36, &3u16.to_le_bytes());
            }
            let eye: [f32; 3] = f32s(&mut emu, ctx + 0x20);
            let head: [f32; 3] = f32s(&mut emu, ctx + 0x30);
            let fwd: [f32; 3] = f32s(&mut emu, ctx + 0x40);
            let radius = f32s::<1>(&mut emu, ctx + 0x80)[0];
            let ep = pos(&mut emu, e);
            let flat = (fwd[0] * fwd[0] + fwd[2] * fwd[2]).sqrt();
            let (fx, fz) = (fwd[0] / flat, fwd[2] / flat);
            let wph = physics(&mut emu, wolf);
            let wrap = emu.alloc(0x20);
            emu.write(wrap, &[0u8; 0x20]);
            emu.u64(wrap + 8, ai);
            let ray_to = emu.alloc(16);
            let mut frames = Vec::new();
            for (secs, dt, (d, a, h)) in steps {
                let n = (secs / dt).round() as usize;
                let (s, c) = a.to_radians().sin_cos();
                // Right of the facing (x, z) is (-z, x) in the exe's frame (aicpp).
                let w = [eye[0] + d * (c * fx - s * fz), eye[1] + h, eye[2] + d * (c * fz + s * fx)];
                for _ in 0..n {
                    emu.poke_f32s(wph + 0x80, &w);
                    if let Some(s) = call_quiet(&mut emu, TS_UPDATE, &[ts], &[0.0, dt]) {
                        panic!("{think_id} {name}: the update stopped at {s:x?}");
                    }
                    let q = frames.len() % 5 == 0;
                    let mut fr = frame(&mut emu, ts, wrap, dt, w, q);
                    // The sight ray from dummy 500 (ctx +0x30) to Wolf's feet + 1.5 (ctx +0xa0).
                    emu.poke_f32s(ray_to, &[w[0], w[1] + 1.5, w[2], 1.0]);
                    fr["ray"] = json!(call_quiet_rax(&mut emu, SIGHT_RAY, &[ctx, ray_to]) & 0xff);
                    frames.push(fr);
                    flush_tb(&emu);
                }
            }
            println!("{think_id} {name}: {} frames, {:.1} s", frames.len(), t0.elapsed().as_secs_f32());
            // GetMapHitRadius (FUN_1405e3750): the AI chr's own radius in GetDist.
            let own = call_quiet_xmm0(&mut emu, 0x1_405e_3750, &[wrap, u64::MAX]);
            rows.push(json!({
                "think": think_id, "scenario": name, "eye": eye, "head": head, "fwd": fwd, "radius": radius,
                "own_radius": own, "chr_pos": ep, "frames": frames,
            }));
        }
    }
    let body = json!({"function": format!("{TS_UPDATE:#x}"), "scenarios": rows});
    std::fs::write(out.join("aiframe_targeting.json"), serde_json::to_string(&body).unwrap()).unwrap();
}

/// The sight ray alone (FUN_14061c8a0 from ctx +0x30 = dummy 500) and the nose sphere
/// (FUN_14061c940) on the context the search builds, for points around three enemies.
fn ray_cases(new_emu: &dyn Fn() -> Emu, out: &Path) {
    let mut rows = Vec::new();
    for oi in 0..3 {
        let mut emu = new_emu();
        let (think, e) = observers(&mut emu)[oi];
        let wolf = emu.st.snap.as_ref().unwrap().player;
        let (_, _, ts) = ai_chain(&mut emu, e).unwrap();
        let (ctx, _, _) = sight(&mut emu, ts, wolf, false);
        let ctx_dump: Vec<f32> = f32s::<52>(&mut emu, ctx).to_vec();
        let flags: [u8; 8] = emu.peek(ctx + 0x50);
        let flags2: [u8; 8] = emu.peek(ctx + 0x78);
        let flags3: [u8; 8] = emu.peek(ctx + 0xc0);
        let head: [f32; 3] = f32s(&mut emu, ctx + 0x30);
        let p = emu.alloc(16);
        let mut cases = Vec::new();
        for dx in [-30.0f32, -10.0, -3.0, 0.0, 3.0, 10.0, 30.0] {
            for dz in [-30.0f32, -10.0, -3.0, 3.0, 10.0, 30.0] {
                for dy in [-20.0f32, -5.0, -1.0, 0.0, 2.0, 10.0] {
                    let t = [head[0] + dx, head[1] + dy, head[2] + dz, 1.0];
                    emu.poke_f32s(p, &t);
                    let clear = call_quiet_rax(&mut emu, SIGHT_RAY, &[ctx, p]) & 0xff;
                    cases.push(json!({"to": &t[..3], "clear": clear}));
                    flush_tb(&emu);
                }
            }
        }
        // The nose sphere with nose_dist poked (row +0x36) and Wolf's feet (ctx +0x90) placed.
        let w = emu.peek_u64(ctx);
        let r = emu.peek_u64(w + 8);
        let eye: [f32; 3] = f32s(&mut emu, ctx + 0x20);
        let mut nose = Vec::new();
        for nd in [0u16, 2, 5] {
            emu.peek::<2>(r + 0x36);
            emu.raw_write(r + 0x36, &nd.to_le_bytes());
            for d in [0.5f32, 1.9, 2.3, 2.5, 4.0, 5.3, 5.5] {
                for (ux, uy, uz) in [(1.0f32, 0.0f32, 0.0f32), (0.0, 0.0, -1.0), (0.6, 0.8, 0.0)] {
                    let t = [eye[0] + d * ux, eye[1] + d * uy, eye[2] + d * uz, 1.0];
                    emu.poke_f32s(ctx + 0x90, &t);
                    let hit = call_quiet_rax(&mut emu, NOSE, &[ctx]) & 0xff;
                    nose.push(json!({"nose_dist": nd, "feet": &t[..3], "hit": hit}));
                }
            }
        }
        println!("think {think}: {} rays, {} nose cases; ctx +0x50 {flags:?} +0x78 {flags2:?} +0xc0 {flags3:?}", cases.len(), nose.len());
        rows.push(json!({"think": think, "head": head, "eye": eye, "chr_pos": pos(&mut emu, e), "radius": f32s::<1>(&mut emu, ctx + 0x80)[0],
            "ctx": ctx_dump, "ctx50": flags, "ctx78": flags2, "ctxc0": flags3, "rays": cases, "nose": nose}));
    }
    let body = json!({"ray": format!("{SIGHT_RAY:#x}"), "nose": format!("{NOSE:#x}"), "observers": rows});
    std::fs::write(out.join("aiframe_ray.json"), serde_json::to_string(&body).unwrap()).unwrap();
}

fn explore(emu: &mut Emu) {
    let obs = observers(emu);
    println!("observers {obs:x?}");
    let (think, e) = obs[0];
    let (m, ai, ts) = ai_chain(emu, e).unwrap();
    println!("think {think} M {m:#x} ai {ai:#x} ts {ts:#x}");
    for o in [0x73d0u64, 0x73e0, 0x7410, 0x7418] {
        let f: [f32; 4] = f32s(emu, ai + o);
        let q: [u8; 8] = emu.peek(ai + o);
        println!("  ai +{o:#x} {f:?} {q:?}");
    }
    let ctrl = emu.peek_u64(e + 0x50);
    println!("ctrl {ctrl:#x} +0x10 {:#x} (chr {e:#x})", emu.peek_u64(ctrl + 0x10));
    let wolf = emu.st.snap.as_ref().unwrap().player;
    let ep = pos(emu, e);
    let ph = physics(emu, e);
    let yaw = f32s::<4>(emu, ph + 0x70)[1];
    let (fx, fz) = (-yaw.sin(), -yaw.cos());
    let wph = physics(emu, wolf);
    emu.poke_f32s(wph + 0x80, &[ep[0] + 8.0 * fx, ep[1], ep[2] + 8.0 * fz]);
    let dt = 1.0f32 / 30.0;
    // The snapshot has DebugFlags +0x10 (disable AI) set: cleared in this copy.
    emu.raw_write(0x1_43d7_a376, &[0]);
    emu.limit = 100_000_000;
    let chr18 = emu.peek_u64(e + 0x18);
    let gate = emu.call(0x1_409e_5c00, &[chr18], &[]).rax & 0xff;
    println!("disable AI {:?}, chr+0x18 gate {gate}, dd8e0 {}", emu.peek::<1>(0x1_43d7_a376), emu.call(0x1_407d_d8e0, &[ctrl], &[]).rax & 0xff);
    for i in 0..150 {
        if i == 30 {
            // The AiThink update stubbed: the move point / type / angles poked instead.
            emu.stub_at(0x1_4060_8990, "ai_think_update", 0, None);
            println!("move mode FUN_1406040b0 {}", emu.call(0x1_4060_40b0, &[ai], &[]).rax & 0xff);
        }
        if i >= 30 {
            let k = (i - 30) / 40;
            emu.u32(ai + 0x7410, k as u32);
            emu.poke_f32s(ai + 0x73d0, &[ep[0] + 3.0, ep[1] + 1.0, ep[2] + 4.0, 1.0]);
            emu.poke_f32s(ai + 0x73e0, &[0.1, 1.0, 0.0, 0.0]);
        }
        emu.set_reg(uc::RDX, dt.to_bits() as u64);
        let r = emu.call(MANIP_UPDATE, &[m, dt.to_bits() as u64, ctrl], &[0.0, dt]);
        if let Some(s) = r.stopped {
            println!("f{i} stopped {s:x?} wild {:x?} calls {:?}", r.wild, r.calls);
            return;
        }
        flush_tb(emu);
        if i % 4 == 0 {
            let mv: [f32; 4] = f32s(emu, m + 0x380);
            let turn: [f32; 4] = f32s(emu, m + 0x20);
            let a60: [f32; 8] = f32s(emu, m + 0x60);
            let mp: [f32; 4] = f32s(emu, ai + 0x73d0);
            let ang: [f32; 2] = f32s(emu, ai + 0x73e0);
            let mt = i32::from_le_bytes(emu.peek(ai + 0x7410));
            let st = i32::from_le_bytes(emu.peek(ts + 0x158));
            println!("f{i} state {st} move {mv:.3?} turn {turn:.3?} +0x60 {a60:.2?} movept {mp:.2?} ang {ang:.3?} type {mt} calls {}", r.calls.len());
        }
    }
}
