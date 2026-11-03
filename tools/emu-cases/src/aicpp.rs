//! The enemy AI's C++ side on the snapshot (topic aicpp): the targeting system's sight checks
//! (FUN_140617280 normal / around through FUN_14061bb70 / FUN_14061c2a0) and the distance and
//! angle queries the Lua goals call, with Wolf moved around an enemy.

use std::path::Path;

use serde_json::json;

use super::{uc, Emu};

const WORLD_CHR_MAN: u64 = 0x1_43d7_a1e0;
const ENEMY_VT: u64 = 0x1_42a2_7f28;

/// Reads 8 bytes of the game's memory, or None where the snapshot has nothing.
fn try_u64(emu: &mut Emu, at: u64) -> Option<u64> {
    if at < 0x10000 || at > 0x7fff_ffff_ffff || at % 8 != 0 {
        return None;
    }
    let mut b = [0u8; 8];
    if unsafe { uc::uc_mem_read(emu.uc, at, b.as_mut_ptr() as _, 8) } == 0 {
        return Some(u64::from_le_bytes(b));
    }
    let (from, bytes) = emu.st.snap.as_mut()?.window(at)?;
    emu.map(from, bytes.len() as u64);
    emu.raw_write(from, &bytes);
    Some(u64::from_le_bytes(emu.read(at)))
}

/// Every EnemyIns in WorldChrMan's chr sets (as guard.rs).
fn enemies(emu: &mut Emu) -> Vec<u64> {
    let wcm = emu.peek_u64(WORLD_CHR_MAN);
    let mut out = Vec::new();
    for off in (0..0x2000).step_by(8) {
        let Some(p) = try_u64(emu, wcm + off) else { continue };
        if try_u64(emu, p).is_none() {
            continue;
        }
        for o2 in (0..0x4000).step_by(8) {
            if let Some(q) = try_u64(emu, p + o2) {
                if !out.contains(&q) && try_u64(emu, q) == Some(ENEMY_VT) {
                    out.push(q);
                }
            }
        }
    }
    out
}

fn f32s<const N: usize>(emu: &mut Emu, at: u64) -> [f32; N] {
    std::array::from_fn(|i| f32::from_le_bytes(emu.peek(at + 4 * i as u64)))
}

/// The chr's physics module ([chr+0x1ff8]+0x68): position +0x80.
fn physics(emu: &mut Emu, chr: u64) -> u64 {
    let bag = emu.peek_u64(chr + 0x1ff8);
    emu.peek_u64(bag + 0x68)
}

fn pos(emu: &mut Emu, chr: u64) -> [f32; 3] {
    let ph = physics(emu, chr);
    f32s(emu, ph + 0x80)
}

/// The enemy's AI chain: ComManipulator M = [[chr+0x50]+0x358], AiThink = [M+0x340] (the
/// manipulator's param_1[0x68]; FUN_1407ded90 falls back to +0x120 when +0x358 is null in FUN_1407e67a0), targeting system = [AiThink+0x7b30]
/// (FUN_140608990's param_1[0xf66]).
fn ai_chain(emu: &mut Emu, chr: u64) -> Option<(u64, u64, u64)> {
    let ctrl = try_u64(emu, chr + 0x50)?;
    let m = match try_u64(emu, ctrl + 0x358)? {
        0 => try_u64(emu, ctrl + 0x120)?,
        m => m,
    };
    let ai = try_u64(emu, m + 0x340)?;
    let ts = try_u64(emu, ai + 0x7b30)?;
    Some((m, ai, ts))
}

pub fn run(new_emu: &dyn Fn() -> Emu, out: &Path, want: &[&str]) {
    if want.contains(&"aicpp_explore") {
        explore(&mut new_emu());
    }
    if want.contains(&"aicpp") || want.contains(&"aicpp_sight") {
        sight_cases(new_emu, out);
    }
    if want.contains(&"aicpp") || want.contains(&"aicpp_query") {
        query_cases(new_emu, out);
    }
}

/// Lua `ai:` methods (registered in FUN_14008xxxx; `this` = a wrapper whose +8 is the AiThink,
/// FUN_1405c36a0). GetDist is the thunk `mov r8b,1; jmp FUN_1405f1540`, GetOriginDist r8b 0.
const GET_DIST: u64 = 0x1_405e_2c10;
const GET_ORIGIN_DIST: u64 = 0x1_405e_3fb0;
const GET_TO_TARGET_ANGLE: u64 = 0x1_405e_4880;
const IS_INSIDE_TARGET: u64 = 0x1_405e_56e0;
const GET_DIST_Y: u64 = 0x1_405e_2e70;
const GET_DIST_Y_SIGNED: u64 = 0x1_405e_2f30;
const GET_MAP_HIT_RADIUS: u64 = 0x1_405e_3750;

/// TARGET_SELF / TARGET_ENE_0 / TARGET_LOCALPLAYER (ai_define.lua).
const TARGETS: [i64; 2] = [0, 21];

fn query_cases(new_emu: &dyn Fn() -> Emu, out: &Path) {
    let mut rows = Vec::new();
    let n_obs = observers(&mut new_emu()).len().min(3);
    for oi in 0..n_obs {
        let mut emu = new_emu();
        let (think, e) = observers(&mut emu)[oi];
        let wolf = emu.st.snap.as_ref().unwrap().player;
        let (_, ai, _) = ai_chain(&mut emu, e).unwrap();
        let wrap = emu.alloc(0x20);
        emu.write(wrap, &[0u8; 0x20]);
        emu.u64(wrap + 8, ai);
        let ep = pos(&mut emu, e);
        let ph = physics(&mut emu, e);
        let yaw = f32s::<4>(&mut emu, ph + 0x70)[1];
        let wph = physics(&mut emu, wolf);
        let self_radius = emu.call(GET_MAP_HIT_RADIUS, &[wrap, u64::MAX], &[]).xmm0;
        // The physics radii (FUN_1409efb80: [chr+0x1ff8]+0x68 module).
        let chr_radius = emu.call(0x1_409e_fb80, &[e], &[]).xmm0;
        let wolf_radius = emu.call(0x1_409e_fb80, &[wolf], &[]).xmm0;
        let (fx, fz) = (-yaw.sin(), -yaw.cos());
        let mut cases = Vec::new();
        let mut calls = 0;
        for &d in &[0.3f32, 2.0, 5.0, 12.0] {
            for &a in &[0.0f32, 30.0, -30.0, 89.0, 91.0, -90.0, 135.0, 180.0] {
                for &h in &[-3.0f32, 0.0, 2.0] {
                    if calls > 600 {
                        // A fresh engine (Unicorn wears out after many calls).
                        emu = new_emu();
                        emu.write(wrap, &[0u8; 0x20]);
                        emu.u64(wrap + 8, ai);
                        calls = 0;
                    }
                    let (s, c) = a.to_radians().sin_cos();
                    let t = [ep[0] + d * (c * fx - s * fz), ep[1] + h, ep[2] + d * (c * fz + s * fx)];
                    emu.poke_f32s(wph + 0x80, &t);
                    let mut q = serde_json::Map::new();
                    q.insert("wolf".into(), json!(t));
                    for tg in TARGETS {
                        let tg = tg as u64;
                        let mut r = |name: &str, v: f32| {
                            q.insert(format!("{name}_{tg}"), json!(v));
                        };
                        r("dist", emu.call(GET_DIST, &[wrap, tg], &[]).xmm0);
                        r("origin_dist", emu.call(GET_ORIGIN_DIST, &[wrap, tg], &[]).xmm0);
                        r("angle", emu.call(GET_TO_TARGET_ANGLE, &[wrap, tg], &[]).xmm0);
                        r("dist_y", emu.call(GET_DIST_Y, &[wrap, tg], &[]).xmm0);
                        r("dist_y_signed", emu.call(GET_DIST_Y_SIGNED, &[wrap, tg], &[]).xmm0);
                        calls += 5;
                        // IsInsideTarget(target, AI_DIR_TYPE_F/B/L/R 1..4, 90 / 60 deg).
                        let mut inside = Vec::new();
                        for dir in 1..=4u64 {
                            for ang in [90.0f32, 60.0] {
                                let run = emu.call(IS_INSIDE_TARGET, &[wrap, tg, dir], &[0.0, 0.0, 0.0, ang]);
                                inside.push(run.rax & 0xff);
                                calls += 1;
                            }
                        }
                        q.insert(format!("inside_{tg}"), json!(inside));
                    }
                    cases.push(serde_json::Value::Object(q));
                }
            }
        }
        println!("think {think}: {} query cases, map hit radius {self_radius}, physics radius {chr_radius}, Wolf's {wolf_radius}", cases.len());
        rows.push(json!({"think": think, "chr_pos": ep, "chr_yaw": yaw, "map_hit_radius": self_radius, "chr_radius": chr_radius, "wolf_radius": wolf_radius, "cases": cases}));
    }
    let body = json!({
        "functions": {
            "GetDist": format!("{GET_DIST:#x}"), "GetOriginDist": format!("{GET_ORIGIN_DIST:#x}"),
            "GetToTargetAngle": format!("{GET_TO_TARGET_ANGLE:#x}"), "IsInsideTarget": format!("{IS_INSIDE_TARGET:#x}"),
            "GetDistY": format!("{GET_DIST_Y:#x}"), "GetDistYSigned": format!("{GET_DIST_Y_SIGNED:#x}"),
        },
        "observers": rows,
    });
    std::fs::write(out.join("aicpp_query.json"), serde_json::to_string(&body).unwrap()).unwrap();
}

/// The enemies the cases use: the nearest ones with distinct NpcThinkParam rows that have a
/// sight cone (as (think id, chr)).
fn observers(emu: &mut Emu) -> Vec<(u32, u64)> {
    let wolf = emu.st.snap.as_ref().unwrap().player;
    let wp = pos(emu, wolf);
    let mut list: Vec<(f32, u64)> = enemies(emu)
        .into_iter()
        .map(|e| {
            let p = pos(emu, e);
            ((p[0] - wp[0]).powi(2) + (p[1] - wp[1]).powi(2) + (p[2] - wp[2]).powi(2), e)
        })
        .collect();
    list.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut out: Vec<(u32, u64)> = Vec::new();
    for (_, e) in list {
        let Some((_, ai, _)) = ai_chain(emu, e) else { continue };
        let think = u32::from_le_bytes(emu.peek(ai + 0x28));
        if think == 10010000 || out.iter().any(|o| o.0 == think) {
            continue;
        }
        out.push((think, e));
        if out.len() == 6 {
            break;
        }
    }
    out
}

/// One variant of the sight context: (name, perceive set, light index, distance cut, angle
/// factors upper / bottom / left / right).
type Variant = (&'static str, u8, u8, f32, [f32; 4]);

const VARIANTS: [Variant; 6] = [
    ("plain", 0, 0, 1.0, [1.0; 4]),
    ("perceive", 1, 0, 1.0, [1.0; 4]),
    ("dark", 0, 1, 1.0, [1.0; 4]),
    ("pitch_dark", 0, 2, 1.0, [1.0; 4]),
    ("crouch_cut", 0, 0, 0.8, [1.0; 4]),
    ("angle_cut", 0, 0, 1.0, [0.5, 0.8, 0.6, 0.9]),
];

fn sight_cases(new_emu: &dyn Fn() -> Emu, out: &Path) {
    let dists = [0.5f32, 1.0, 3.0, 4.2, 5.0, 8.0, 12.0, 16.0, 19.0, 21.0, 25.0, 30.0, 33.5, 36.5, 40.5, 46.0, 53.5, 56.5, 60.0];
    let angles = [0.0f32, 8.0, -8.0, 14.0, -14.0, 16.0, -16.0, 29.0, -29.0, 31.0, -31.0, 35.0, -35.0, 37.0, -37.0, 44.0, -44.0, 46.0, -46.0, 90.0, -90.0, 150.0, 180.0];
    let heights = [-12.0f32, -5.0, -1.5, 0.0, 1.5, 5.0, 12.0];
    let mut rows = Vec::new();
    let n_obs = observers(&mut new_emu()).len();
    for oi in 0..n_obs {
        for v in VARIANTS {
            // A fresh engine per enemy and variant (Unicorn wears out after many calls).
            let mut emu = new_emu();
            let (think, e) = observers(&mut emu)[oi];
            let wolf = emu.st.snap.as_ref().unwrap().player;
            let (_, _, ts) = ai_chain(&mut emu, e).unwrap();
            let (ctx, _, _) = sight(&mut emu, ts, wolf, false);
            emu.u8(ctx + 0x56, v.1);
            emu.u8(ctx + 0x79, v.2);
            emu.f32(ctx + 0x64, v.3);
            for (i, a) in v.4.iter().enumerate() {
                emu.f32(ctx + 0x68 + 4 * i as u64, *a);
            }
            let eye: [f32; 3] = f32s(&mut emu, ctx + 0x20);
            let fwd: [f32; 3] = f32s(&mut emu, ctx + 0x40);
            let raise = f32s::<4>(&mut emu, ctx + 0xa0)[1] - f32s::<4>(&mut emu, ctx + 0x90)[1];
            let wrap = emu.peek_u64(ctx);
            let row = emu.peek_u64(wrap + 8);
            let disable_dark = if row != 0 { emu.peek::<1>(row + 0x53)[0] } else { 0 };
            let ep = pos(&mut emu, e);
            let ph = physics(&mut emu, e);
            let yaw = f32s::<4>(&mut emu, ph + 0x70)[1];
            let flat = (fwd[0] * fwd[0] + fwd[2] * fwd[2]).sqrt();
            let (fx, fz) = (fwd[0] / flat, fwd[2] / flat);
            let mut cases = Vec::new();
            let small = v.0 != "plain";
            for &d in &dists {
                for &a in &angles {
                    for &h in &heights {
                        if small && (h.abs() > 2.0 || a.abs() > 50.0 && a != 90.0) {
                            continue;
                        }
                        let (s, c) = a.to_radians().sin_cos();
                        // Right of fwd (x, z) is (-z, x) in this frame; the sign is only a label.
                        let t = [eye[0] + d * (c * fx - s * fz), eye[1] + h, eye[2] + d * (c * fz + s * fx)];
                        emu.poke_f32s(ctx + 0x90, &[t[0], t[1], t[2], 1.0]);
                        emu.poke_f32s(ctx + 0xa0, &[t[0], t[1] + raise, t[2], 1.0]);
                        let n = emu.call(SIGHT, &[ctx, 0], &[]);
                        let r = emu.call(SIGHT_AROUND, &[ctx, 0], &[]);
                        assert!(n.stopped.is_none() && r.stopped.is_none(), "sight stopped");
                        cases.push(json!({"target": t, "normal": n.rax & 0xff, "around": r.rax & 0xff}));
                    }
                }
            }
            println!("think {think} {}: {} cases", v.0, cases.len());
            rows.push(json!({
                "think": think,
                "variant": v.0,
                "perceive": v.1,
                "light_index": v.2,
                "light_table": f32s::<3>(&mut emu, ctx + 0x58),
                "disable_dark": disable_dark,
                "ctx50": f32s::<1>(&mut emu, ctx + 0x50)[0],
                "cut": v.3,
                "ang": v.4,
                "radius": f32s::<1>(&mut emu, ctx + 0x80)[0],
                "eye": eye,
                "fwd": fwd,
                "chr_pos": ep,
                "chr_yaw": yaw,
                "cases": cases,
            }));
        }
    }
    let body = json!({
        "function": format!("{SIGHT:#x}"),
        "around": format!("{SIGHT_AROUND:#x}"),
        "context": format!("{CTX_FILL:#x} / {CTX_TARGET:#x}"),
        "observers": rows,
    });
    std::fs::write(out.join("aicpp_sight.json"), serde_json::to_string(&body).unwrap()).unwrap();
}

fn explore(emu: &mut Emu) {
    let wolf = emu.st.snap.as_ref().unwrap().player;
    let wp = pos(emu, wolf);
    println!("wolf {wolf:#x} at {wp:?}");
    let mut list: Vec<(f32, u64)> = enemies(emu)
        .into_iter()
        .map(|e| {
            let p = pos(emu, e);
            let d = ((p[0] - wp[0]).powi(2) + (p[1] - wp[1]).powi(2) + (p[2] - wp[2]).powi(2)).sqrt();
            (d, e)
        })
        .collect();
    list.sort_by(|a, b| a.0.total_cmp(&b.0));
    for (d, e) in list.iter().take(12) {
        let p = pos(emu, *e);
        let ctrl = try_u64(emu, *e + 0x50).unwrap_or(0);
        let raw = (try_u64(emu, ctrl + 0x358), try_u64(emu, ctrl + 0x120));
        println!("  ctrl {ctrl:#x} +0x358/+0x120 {raw:x?}");
        let chain = ai_chain(emu, *e);
        let (vt_m, vt_ts) = match chain {
            Some((m, _, ts)) => (try_u64(emu, m).unwrap_or(0), try_u64(emu, ts).unwrap_or(0)),
            None => (0, 0),
        };
        println!("enemy {e:#x} d {d:.2} pos {p:?} chain {chain:x?} vt M {vt_m:#x} ts {vt_ts:#x}");
        if let Some((_, ai, _)) = chain {
            println!("  think id? {}", u32::from_le_bytes(emu.peek(ai + 0x28)));
        }
    }
    let e = list[0].1;
    let (_, _, ts) = ai_chain(emu, e).unwrap();
    let obs = emu.peek_u64(ts + 8);
    let ovt = emu.peek_u64(obs);
    let slots: Vec<String> = (0..0x100u64).step_by(8).map(|o| format!("{o:#x}:{:#x}", emu.peek_u64(ovt + o))).collect();
    println!("observer {obs:#x} vt {ovt:#x} {slots:?}");
    let ph = physics(emu, e);
    println!("enemy physics yaw {:?}", f32s::<4>(emu, ph + 0x70));
    let wph = physics(emu, wolf);
    println!("wolf physics yaw {:?}", f32s::<4>(emu, wph + 0x70));
    let up = emu.call(0x1_409e_fb70, &[e], &[]).xmm0;
    let fwd = emu.call(0x1_409e_fb80, &[e], &[]).xmm0;
    let vt = emu.peek_u64(e);
    let f438 = emu.peek_u64(vt + 0x438);
    let id = emu.alloc(8);
    emu.u32(id, 500);
    let has = emu.call(f438, &[e, id], &[]).rax & 0xff;
    println!("enemy physics +0xd0 (up) {up} radius {fwd} dmy 500 present {has}");
    let (ctx, n, a) = sight(emu, ts, wolf, true);
    println!("nearest: sight {n} around {a}");
    dump_ctx(emu, ctx);
    // Wolf moved: does the context follow his physics position?
    let ph = physics(emu, wolf);
    let ep = pos(emu, e);
    emu.poke_f32s(ph + 0x80, &[ep[0] + 1.0, ep[1], ep[2] + 2.0]);
    let (ctx, n, a) = sight(emu, ts, wolf, true);
    println!("wolf at enemy + (1, 0, 2): sight {n} around {a}");
    dump_ctx(emu, ctx);
}

/// FUN_140625d50: the sight context from the observer's targeting system.
const CTX_FILL: u64 = 0x1_4062_5d50;
/// FUN_1406170f0: per-target reset of the context.
const CTX_RESET: u64 = 0x1_4061_70f0;
/// FUN_140623960(ts, handle): ctx +0xc2, skip the sight ray for this target.
const CTX_NO_RAY: u64 = 0x1_4062_3960;
/// FUN_140bd3e40(target's module 0x38, ctx): the target's side (position, cuts, light).
const CTX_TARGET: u64 = 0x1_40bd_3e40;
/// FUN_140617280(ctx, ray): 2 = in the normal (or perceive) cone, 1 = around cone, 0 none.
const SIGHT: u64 = 0x1_4061_7280;
/// FUN_1406172f0(ctx, ray): the around cone alone.
const SIGHT_AROUND: u64 = 0x1_4061_72f0;

/// Builds the context FUN_1406244f0 builds for one target and runs both checks.
fn sight(emu: &mut Emu, ts: u64, target: u64, ray: bool) -> (u64, u64, u64) {
    let ctx = emu.alloc(0x200);
    emu.write(ctx, &vec![0u8; 0x200]);
    emu.call(CTX_FILL, &[ts, ctx], &[]);
    let handle = u32::from_le_bytes(emu.peek(target + 8)) as u64;
    let no_ray = emu.call(CTX_NO_RAY, &[ts, handle], &[]).rax as u8;
    emu.call(CTX_RESET, &[ctx], &[]);
    emu.u8(ctx + 0xc2, no_ray);
    let bag = emu.peek_u64(target + 0x1ff8);
    let module = emu.peek_u64(bag + 0x38);
    emu.call(CTX_TARGET, &[module, ctx], &[]);
    let r = emu.call(SIGHT, &[ctx, ray as u64], &[]);
    if let Some(s) = r.stopped {
        println!("  sight stopped at {:#x} ({})", s.0, s.1);
    }
    let a = emu.call(SIGHT_AROUND, &[ctx, ray as u64], &[]);
    (ctx, r.rax & 0xff, a.rax & 0xff)
}

fn dump_ctx(emu: &mut Emu, ctx: u64) {
    for row in (0..0xd0u64).step_by(16) {
        let f: [f32; 4] = f32s(emu, ctx + row);
        let b: [u8; 16] = emu.peek(ctx + row);
        println!("  +{row:#04x} {f:>12.4?} {b:02x?}");
    }
}
