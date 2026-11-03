//! Throw checks, second pass (topic throw2): the sight cast FUN_140be3da0 and the
//! additionalConditionSpecialEffect pair inside a row's test FUN_140be3a90, the pick among chrs
//! (FUN_140be0e50: an IChrFinder over WorldChrMan, FUN_140bdf930 -> FUN_140bde5c0), the kind test
//! FUN_140be3810 on a dying chr, and the fall orbit FUN_140be3ba0.

use super::throw::{chr_pos, enemies, f32s, i32at, kind_ref, module, nearest, place, row_ref, Lcg};
use super::*;

const WORLD_CHR_MAN: u64 = 0x1_43d7_a1e0;
const ROW: u64 = 0x1_40be_3a90;
const SIGHT: u64 = 0x1_40be_3da0;
const FALL: u64 = 0x1_40be_3ba0;
const KIND: u64 = 0x1_40be_3810;
const CAST: u64 = 0x1_4094_d8d0;
const CHR_POS0: u64 = 0x1_409f_0870; // FUN_1409f0870(chr, out): the chr's position (FUN_140be3290)
const RADIUS: u64 = 0x1_40bb_f740; // FUN_140bbf740(physics)
const IS_PLAYER: u64 = 0x1_409f_3430;
const IS_OTHER_PC: u64 = 0x1_409f_36c0;
const FRACS: u64 = 0x1_43b0_15a0; // FUN_140802730: the cast heights' fractions
const HIT_WORLD: u64 = 0x1_43d6_d640; // +0x98
const SET_CHR: u64 = 0x1_40a0_7b50; // FUN_140a07b50(set, i)
const SEARCH: u64 = 0x1_40be_0e50;
const TAKE: u64 = 0x1_40be_2d60;
const KINDS_OF: u64 = 0x1_43d7_8488; // +0x270 + type * 0x20: ThrowKindParam ids by throwCheckSituation
const ATK_TABLE: u64 = 0x1_40be_0950;
const KIND_ATK: u64 = 0x1_40be_3520;

pub fn run(new_emu: &dyn Fn() -> Emu, out: &Path, want: &[&str]) {
    if want.contains(&"throw2_info") {
        info(&mut new_emu());
    }
    if want.contains(&"throw2") || want.contains(&"throw2_sight") {
        sight(new_emu, out);
    }
    if want.contains(&"throw2") || want.contains(&"throw2_pick") {
        pick(new_emu, out);
    }
    if want.contains(&"throw2") || want.contains(&"throw2_speffect") {
        speffect(new_emu, out);
    }
    if want.contains(&"throw2") || want.contains(&"throw2_fall") {
        fall(new_emu, out);
    }
}

const ORBIT: u64 = 0x1_40bb_2850; // FUN_140bb2850(p0, v, a, target, out, time)
const VEL: u64 = 0x1_40bb_f7d0; // FUN_140bbf7d0(physics, out): the proxy's velocity

/// The fall orbit test FUN_140be3ba0 (kinds with doesCheckNormalFallOrbit, the plunge rows):
/// the orbit's constants, then its closest-approach solver FUN_140bb2850 on chosen starts,
/// velocities and targets, then the whole test with Wolf put above the General (his fall top
/// physics +0x854 set) on the plunge row 11020150 and the kick row 11020160.
fn fall(new_emu: &dyn Fn() -> Emu, out: &Path) {
    let mut emu = new_emu();
    let wolf = emu.st.snap.as_ref().unwrap().player;
    let wph = module(&mut emu, wolf, 0x68);
    let acc0 = f32s::<2>(&mut emu, 0x1_43b0_9ce8);
    let g_add = f32s::<1>(&mut emu, wph + 0x1c4)[0];
    let up = f32s::<4>(&mut emu, 0x1_43f0_83c0);
    let d0 = f32s::<4>(&mut emu, 0x1_43f0_83d0);
    let vb = emu.alloc(16);
    emu.call(VEL, &[wph, vb], &[]);
    let v_wolf = f32s::<4>(&mut emu, vb);
    println!("accel base {acc0:?} physics +0x1c4 {g_add} 143f083c0 {up:?} 143f083d0 {d0:?} wolf velocity {v_wolf:?}");
    let a = [acc0[0], acc0[1] + g_add, 0.0];
    // The solver on its own.
    let mut rng = Lcg(0xfa11);
    let mut orbit = Vec::new();
    for i in 0..400 {
        let p0 = [rng.f(-2.0, 2.0), rng.f(0.0, 12.0), rng.f(-2.0, 2.0)];
        let v = if i % 4 == 0 { [0.0, rng.f(-6.0, 3.0), 0.0] } else { [rng.f(-5.0, 5.0), rng.f(-8.0, 4.0), rng.f(-5.0, 5.0)] };
        let t = [rng.f(-3.0, 3.0), rng.f(-1.0, 1.0), rng.f(-3.0, 3.0)];
        let (pb, vbuf, ab, tb, ob, timeb) = (emu.alloc(16), emu.alloc(16), emu.alloc(16), emu.alloc(16), emu.alloc(16), emu.alloc(16));
        emu.poke_f32s(pb, &[p0[0], p0[1], p0[2], 1.0]);
        emu.poke_f32s(vbuf, &[v[0], v[1], v[2], 0.0]);
        emu.poke_f32s(ab, &[a[0], a[1], a[2], 0.0]);
        emu.poke_f32s(tb, &[t[0], t[1], t[2], 1.0]);
        emu.poke_f32s(ob, &[0.0; 4]);
        emu.raw_write(timeb, &0x1_4294_8018u64.to_le_bytes());
        emu.poke_f32s(timeb + 8, &[9999.0, 0.0]);
        let r = emu.call(ORBIT, &[pb, vbuf, ab, tb, ob, timeb], &[]);
        if r.stopped.is_some() {
            println!("orbit stopped {:?}", r.stopped);
            continue;
        }
        let o = v3(&mut emu, ob);
        let time = f32s::<1>(&mut emu, timeb + 8)[0];
        orbit.push(json!({"p0": p0, "v": v, "a": a, "target": t, "closest": o, "time": time}));
    }
    // The whole test on the snapshot: Wolf above the General, standing still (velocity 0).
    let mut cases = Vec::new();
    for row in [11020150u32, 11020160, 11020030] {
        let mut emu = new_emu();
        let wolf = emu.st.snap.as_ref().unwrap().player;
        let gen = nearest(&mut emu, 1020);
        let wph = module(&mut emu, wolf, 0x68);
        let w0 = v3(&mut emu, wph + 0x80);
        let rr = row_ref(&mut emu, row);
        let mut rng = Lcg(0xfa12 + row as u64);
        for _ in 0..150 {
            let h = rng.f(0.0, 14.0);
            let top = h + rng.f(0.0, 4.0);
            let off = rng.f(0.0, 3.0);
            let ang = rng.f(-3.14159, 3.14159);
            let wp = [w0[0], w0[1] + h, w0[2]];
            let gp = [w0[0] + off * ang.sin(), w0[1], w0[2] + off * ang.cos()];
            place(&mut emu, wolf, wp, 0.0);
            emu.poke_f32s(wph + 0x854, &[w0[1] + top]);
            place(&mut emu, gen, gp, 0.0);
            let r = emu.call(FALL, &[wolf, gen, rr], &[]);
            if r.stopped.is_some() {
                println!("fall stopped {:?}", r.stopped);
                continue;
            }
            let (wq, gq) = (chr_pos(&mut emu, wolf), chr_pos(&mut emu, gen));
            cases.push(json!({"row": row, "wolf": wq, "fall_top": w0[1] + top, "enemy": gq, "ok": r.rax & 1}));
        }
    }
    let body = json!({"function": "FUN_140bb2850 / FUN_140be3ba0", "accel": a, "orbit": orbit, "cases": cases});
    std::fs::write(out.join("throw2_fall.json"), serde_json::to_string_pretty(&body).unwrap()).unwrap();
    println!("throw2_fall: {} orbit runs, {} row cases ({} pass)", orbit.len(), cases.len(), cases.iter().filter(|c| c["ok"] == 1).count());
}

fn u16at(emu: &mut Emu, at: u64) -> u16 {
    u16::from_le_bytes(emu.peek(at))
}

/// WorldChrMan's chrs in FUN_140a4c260's order: its sets (+0x1c8 count, +0x1d0 entries of 0x18),
/// each set's chrs by index (FUN_140a07b50).
fn chr_order(emu: &mut Emu) -> Vec<u64> {
    let wcm = emu.peek_u64(WORLD_CHR_MAN);
    let n = i32at(emu, wcm + 0x1c8);
    let mut out = Vec::new();
    for s in 0..n.max(0) as u64 {
        let set = emu.peek_u64(wcm + 0x1d0 + 0x18 * s);
        if set == 0 {
            continue;
        }
        let count = i32at(emu, set);
        for i in 0..count.max(0) as u64 {
            let r = emu.call(SET_CHR, &[set, i], &[]);
            if r.stopped.is_none() && r.rax != 0 {
                out.push(r.rax);
            }
        }
    }
    out
}

fn info(emu: &mut Emu) {
    let wolf = emu.st.snap.as_ref().unwrap().player;
    let fr = f32s::<2>(emu, FRACS);
    let dbg = emu.peek_u64(0x1_43d5_be38);
    let flag = emu.peek::<1>(dbg + 0x101)[0];
    println!("fracs {fr:?} debug-force {flag}");
    let gen = nearest(emu, 1020);
    for c in [wolf, gen] {
        let ph = module(emu, c, 0x68);
        let h = f32s::<1>(emu, ph + 0xd0)[0];
        let r = emu.call(RADIUS, &[ph], &[]);
        let fall_top = f32s::<1>(emu, ph + 0x854)[0];
        let buf = emu.alloc(16);
        emu.call(CHR_POS0, &[c, buf], &[]);
        let p0 = f32s::<4>(emu, buf);
        println!("chr {c:x} id {} height {h} radius {} p0 {p0:?} pos {:?} +0x854 {fall_top}", i32at(emu, c + 0x6c), r.xmm0, chr_pos(emu, c));
    }
    let k = emu.peek_u64(KINDS_OF);
    for ty in 0..6u64 {
        let (b, e) = (emu.peek_u64(k + 0x270 + ty * 0x20 + 8), emu.peek_u64(k + 0x270 + ty * 0x20 + 0x10));
        let ids: Vec<i32> = (0..(e.saturating_sub(b) / 4).min(200)).map(|i| i32at(emu, b + 4 * i)).collect();
        println!("kinds of situation {ty}: {ids:?}");
    }
    let order = chr_order(emu);
    println!("chr order ({}):", order.len());
    let wph = module(emu, wolf, 0x68);
    let wp = f32s::<3>(emu, wph + 0x80);
    for c in &order {
        let ph = module(emu, *c, 0x68);
        let p = f32s::<3>(emu, ph + 0x80);
        let d = (0..3).map(|i| (p[i] - wp[i]).powi(2)).sum::<f32>().sqrt();
        let th = module(emu, *c, 0x88);
        let st: [u8; 4] = if th != 0 { emu.peek(th + 0x78) } else { [0; 4] };
        let e18 = emu.peek_u64(c + 0x18);
        let (k10, k20) = (i32at(emu, e18 + 0x10), emu.peek::<1>(e18 + 0x20)[0]);
        let dce = emu.call(0x1_407d_ce60, &[*c], &[]).rax as i32;
        println!("  {c:x} c{} d {d:.1} states {st:?} [+0x18]+0x10 {k10} +0x20 {k20:x} +0x110 {} dce60 {dce}", i32at(emu, c + 0x6c), i32at(emu, c + 0x110));
    }
    let mut atk = Vec::new();
    for s in 0..=127u64 {
        for c in [0u64, 4, 5, 13, 16, 17, 18] {
            let r = emu.call(ATK_TABLE, &[s, c], &[]);
            if r.rax & 1 == 1 {
                atk.push((s, c));
            }
        }
    }
    println!("thrower table (state, category): {atk:?}");
    let _ = (KIND_ATK, SEARCH, TAKE, enemies, kind_ref, KIND, FALL, ROW, place, row_ref, Lcg(0));
}

fn v3(emu: &mut Emu, at: u64) -> [f32; 3] {
    f32s::<3>(emu, at)
}

/// The sight cast between thrower and defender (FUN_140be3da0) on Wolf -> the General, put around
/// Wolf at random, and the same casts made here (FUN_14094d8d0 with the start, delta and radius
/// FUN_140be3da0 builds) so the port runs on the exe's hits.
fn sight(new_emu: &dyn Fn() -> Emu, out: &Path) {
    let rows: [u32; 3] = [11020000, 11020020, 11020150];
    let mut rng = Lcg(0x5197_7);
    let mut cases = Vec::new();
    let (mut stops, mut differ, mut blocked) = (0, 0, 0);
    let mut meta = json!(null);
    for chunk in 0..12 {
        let mut emu = new_emu();
        let wolf = emu.st.snap.as_ref().unwrap().player;
        let gen = nearest(&mut emu, 1020);
        let wph = module(&mut emu, wolf, 0x68);
        let gph = module(&mut emu, gen, 0x68);
        let w0 = v3(&mut emu, wph + 0x80);
        let fracs = f32s::<2>(&mut emu, FRACS);
        let (wh, gh) = (f32s::<1>(&mut emu, wph + 0xd0)[0], f32s::<1>(&mut emu, gph + 0xd0)[0]);
        let wr = emu.call(RADIUS, &[wph], &[]).xmm0;
        let gr = emu.call(RADIUS, &[gph], &[]).xmm0;
        let world = emu.peek_u64(HIT_WORLD);
        let world = emu.peek_u64(world + 0x98);
        // The filter: 0x4c, 0x41 between the local player and another player (not the case here).
        let p1 = emu.call(IS_PLAYER, &[wolf], &[]).rax & 1 == 1 && emu.call(IS_OTHER_PC, &[gen], &[]).rax & 1 == 1;
        let filter = if p1 { 0x41 } else { 0x4c };
        meta = json!({"fracs": fracs, "wolf_height": wh, "wolf_radius": wr, "enemy_height": gh, "enemy_radius": gr, "filter": filter});
        let row = rows[chunk % 3];
        let rr = row_ref(&mut emu, row);
        let rp = emu.peek_u64(rr);
        let pct = [u16at(&mut emu, rp + 0x30), u16at(&mut emu, rp + 0x32)];
        let skip = emu.peek::<1>(rp + 0x3b)[0] >> 2 & 1;
        for _ in 0..100 {
            let wyaw = rng.f(-3.14159, 3.14159);
            // Near (the deathblow reach, the General sunk in or raised over the floor), then far
            // across the arena (walls, the gate, the steps).
            let (bearing, r, dy) = if chunk < 6 { (rng.f(-180.0, 180.0), rng.f(0.3, 3.0), rng.f(-2.2, 1.0)) } else { (rng.f(-180.0, 180.0), rng.f(2.0, 22.0), rng.f(-3.0, 3.0)) };
            let a = wyaw + bearing.to_radians();
            let gp = [w0[0] - r * a.sin(), w0[1] + dy, w0[2] - r * a.cos()];
            place(&mut emu, wolf, w0, wyaw);
            place(&mut emu, gen, gp, rng.f(-3.14159, 3.14159));
            let res = emu.call(SIGHT, &[wolf, gen, rr, 0], &[]);
            if res.stopped.is_some() {
                stops += 1;
                if stops < 4 {
                    println!("sight stopped {:?} trail {:x?}", res.stopped, emu.st.trail);
                }
                continue;
            }
            let exe = res.rax & 1 == 1;
            // FUN_140be3290: the two positions (FUN_1409f0870; judgeRangeBasePosDmyId -1 here).
            let (bw, bg) = (emu.alloc(16), emu.alloc(16));
            emu.call(CHR_POS0, &[wolf, bw], &[]);
            emu.call(CHR_POS0, &[gen, bg], &[]);
            let (pw, pg) = (v3(&mut emu, bw), v3(&mut emu, bg));
            let mut rays = Vec::new();
            let mut ours = true;
            for i in 0..2 {
                let radius = wr.max(gr) * pct[i] as f32 / 100.0;
                let start = [pw[0], pw[1] + wh * fracs[i], pw[2]];
                let delta = [pg[0] - start[0], pg[1] + gh * fracs[i] - start[1], pg[2] - start[2]];
                if skip == 1 || radius <= 0.0 {
                    rays.push(json!({"start": start, "delta": delta, "radius": radius, "hit": null}));
                    continue;
                }
                let (sb, db, hb) = (emu.alloc(16), emu.alloc(16), emu.alloc(16));
                emu.poke_f32s(sb, &[start[0], start[1], start[2], 1.0]);
                emu.poke_f32s(db, &[delta[0], delta[1], delta[2], 0.0]);
                emu.poke_f32s(hb, &[0.0; 4]);
                let c = emu.call(CAST, &[world, filter, sb, db, radius.to_bits() as u64, hb, 0, 0, 0], &[]);
                assert!(c.stopped.is_none(), "cast stopped {:?}", c.stopped);
                let hit = (c.rax & 0xff != 0).then(|| v3(&mut emu, hb));
                let pass = match hit {
                    None => true,
                    Some(h) => {
                        let d2 = delta.iter().map(|x| x * x).sum::<f32>();
                        let h2 = (0..3).map(|k| (h[k] - start[k]).powi(2)).sum::<f32>();
                        d2 < h2
                    }
                };
                rays.push(json!({"start": start, "delta": delta, "radius": radius, "hit": hit}));
                if !pass {
                    ours = false;
                    break;
                }
            }
            blocked += !exe as usize;
            differ += (exe != ours) as usize;
            cases.push(json!({"row": row, "pct": pct, "skip": skip, "wolf": pw, "enemy": pg, "rays": rays, "sight": exe as u8}));
        }
    }
    let body = json!({"function": "FUN_140be3da0 (casts FUN_14094d8d0)", "meta": meta, "cases": cases});
    std::fs::write(out.join("throw2_sight.json"), serde_json::to_string_pretty(&body).unwrap()).unwrap();
    println!("throw2_sight: {} cases, {blocked} blocked, {differ} differ from the casts made here, {stops} stopped", cases.len());
}

/// The input throw's pick among chrs (FUN_140be0e50): Wolf (his throw module) with the attack
/// pressed, two Ashina soldiers (c1010) and the General (c1020) put in reach and broken (TAE 932
/// state 103 poked in their throw modules +0x7a). FUN_140be2d60 (which starts the throw) is a
/// stub, so the run returns the chr the IChrFinder stopped at.
fn pick(new_emu: &dyn Fn() -> Emu, out: &Path) {
    let mut rng = Lcg(0x91c4);
    let mut cases = Vec::new();
    let mut stops = 0;
    for chunk in 0..6 {
        let mut emu = new_emu();
        emu.stub_at(TAKE, "take", u64::MAX, None);
        let wolf = emu.st.snap.as_ref().unwrap().player;
        let order = chr_order(&mut emu);
        let wph = module(&mut emu, wolf, 0x68);
        let w0 = v3(&mut emu, wph + 0x80);
        // Three awake chrs (state 101 in the snapshot): the two c1010 nearest Wolf and the General.
        let mut chrs: Vec<u64> = Vec::new();
        for id in [1010, 1010, 1020] {
            let mut best = (f32::MAX, 0);
            for &c in &order {
                if i32at(&mut emu, c + 0x6c) != id || chrs.contains(&c) {
                    continue;
                }
                let th = module(&mut emu, c, 0x88);
                if th == 0 || emu.peek::<1>(th + 0x7b)[0] != 101 {
                    continue;
                }
                let ph = module(&mut emu, c, 0x68);
                let p = v3(&mut emu, ph + 0x80);
                let d = (0..3).map(|i| (p[i] - w0[i]).powi(2)).sum::<f32>();
                if d < best.0 {
                    best = (d, c);
                }
            }
            assert!(best.1 != 0, "no awake c{id}");
            chrs.push(best.1);
        }
        let idx: Vec<usize> = chrs.iter().map(|c| order.iter().position(|o| o == c).unwrap()).collect();
        for &c in &chrs {
            let th = module(&mut emu, c, 0x88);
            emu.raw_write(th + 0x7a, &[103]);
            // chr +0x110: the update omission (30 / 5 at 36-90 m in the snapshot); FUN_140bdf930
            // takes only chrs updated every frame (FUN_1407dce60 == 0), as any near Wolf is.
            emu.raw_write(c + 0x110, &0u32.to_le_bytes());
        }
        let th = module(&mut emu, wolf, 0x88);
        let input = emu.alloc(16);
        // byte 0: Wolf's +0x10 bit; then (pressed, held, ?) for buttons 0 / 4 / 5 (FUN_1407d8430).
        emu.raw_write(input, &[0, 1, 1, 0, 0, 0, 0, 0, 0, 0]);
        for _ in 0..20 {
            let wyaw = rng.f(-3.14159, 3.14159);
            let mut placed = Vec::new();
            for (k, &c) in chrs.iter().enumerate() {
                let bearing = rng.f(-25.0, 25.0) + [-50.0, 0.0, 50.0][k] * (chunk % 2) as f32;
                let r = rng.f(0.6, 2.0);
                let a = wyaw + bearing.to_radians();
                let p = [w0[0] - r * a.sin(), w0[1], w0[2] - r * a.cos()];
                // Facing Wolf (the break rows' DiffAngMin 90): forward (-sin y, 0, -cos y).
                let to = [w0[0] - p[0], w0[2] - p[2]];
                let face = (-to[0]).atan2(-to[1]);
                placed.push((c, p, face, r));
            }
            place(&mut emu, wolf, w0, wyaw);
            let far = |p: [f32; 3]| [p[0] + 60.0, p[1], p[2]];
            // Each alone (the others 60 m off), then all three.
            let mut alone = Vec::new();
            for k in 0..3 {
                for (j, &(c, p, f, _)) in placed.iter().enumerate() {
                    place(&mut emu, c, if j == k { p } else { far(p) }, f);
                }
                let r = emu.call(SEARCH, &[th, input], &[]);
                if r.stopped.is_some() {
                    stops += 1;
                    if stops < 4 {
                        println!("search stopped {:?} trail {:x?}", r.stopped, emu.st.trail);
                    }
                }
                alone.push(r.rax == placed[k].0);
            }
            for &(c, p, f, _) in &placed {
                place(&mut emu, c, p, f);
            }
            let r = emu.call(SEARCH, &[th, input], &[]);
            if r.stopped.is_some() {
                stops += 1;
                continue;
            }
            let picked = placed.iter().position(|x| x.0 == r.rax).map_or(-1, |k| k as i64);
            let mut rows = Vec::new();
            for (k, &(c, p, f, d)) in placed.iter().enumerate() {
                rows.push(json!({"chr": i32at(&mut emu, c + 0x6c), "order": idx[k], "pos": p, "yaw": f, "dist": d, "alone": alone[k] as u8}));
            }
            cases.push(json!({"wolf": w0, "wolf_yaw": wyaw, "chrs": rows, "picked": picked}));
        }
    }
    let body = json!({"function": "FUN_140be0e50 (FUN_140be2d60 stubbed)", "cases": cases});
    std::fs::write(out.join("throw2_pick.json"), serde_json::to_string_pretty(&body).unwrap()).unwrap();
    println!("throw2_pick: {} cases, {stops} stopped", cases.len());
}

/// The additionalConditionSpecialEffect pair (ThrowParam +0x6c thrower, +0x70 defender) in
/// FUN_140be3a90: the General's Mikiri row 11020190 (forDef 200030) with the General in reach,
/// with and without a SpEffect node of that id on him (chr +0x11d0 -> +8 list, id +0x58, next
/// +0x70; FUN_1409f1c80), and the thrower side on the row with +0x6c poked to an id.
fn speffect(new_emu: &dyn Fn() -> Emu, out: &Path) {
    let mut cases = Vec::new();
    {
        let mut emu = new_emu();
        let gen = nearest(&mut emu, 1020);
        let list = emu.peek_u64(gen + 0x11d0);
        let mut n = emu.peek_u64(list + 8);
        let mut ids = Vec::new();
        while n != 0 && ids.len() < 64 {
            ids.push(i32at(&mut emu, n + 0x58));
            n = emu.peek_u64(n + 0x70);
        }
        println!("the General's SpEffects: {ids:?}");
    }
    for (atk_id, def_id) in [(-1i32, 200030i32), (-1, 3_990_001), (200031, -1), (200031, 3_990_001)] {
        for (on_wolf, on_gen) in [(false, false), (true, false), (false, true), (true, true)] {
            let mut emu = new_emu();
            let wolf = emu.st.snap.as_ref().unwrap().player;
            let gen = nearest(&mut emu, 1020);
            let rr = row_ref(&mut emu, 11020190);
            let rp = emu.peek_u64(rr);
            emu.peek::<8>(rp + 0x6c);
            emu.raw_write(rp + 0x6c, &atk_id.to_le_bytes());
            emu.raw_write(rp + 0x70, &def_id.to_le_bytes());
            for (c, on, id) in [(wolf, on_wolf, atk_id), (gen, on_gen, def_id)] {
                if !on {
                    continue;
                }
                let list = emu.peek_u64(c + 0x11d0);
                let head = emu.peek_u64(list + 8);
                let node = emu.alloc(0x80);
                emu.raw_write(node, &[0u8; 0x80]);
                emu.raw_write(node + 0x58, &id.max(1).to_le_bytes());
                emu.raw_write(node + 0x70, &head.to_le_bytes());
                emu.raw_write(list + 8, &node.to_le_bytes());
            }
            let wph = module(&mut emu, wolf, 0x68);
            let w0 = v3(&mut emu, wph + 0x80);
            place(&mut emu, wolf, w0, 0.0);
            // 1.5 m in front (forward (-sin y, 0, -cos y)), facing Wolf.
            place(&mut emu, gen, [w0[0], w0[1], w0[2] - 1.5], std::f32::consts::PI);
            let r = emu.call(ROW, &[wolf, gen, 0, rr], &[]);
            assert!(r.stopped.is_none(), "row stopped {:?}", r.stopped);
            cases.push(json!({"atk_sp": atk_id, "def_sp": def_id, "wolf_has": on_wolf, "enemy_has": on_gen, "row_ok": r.rax & 1}));
        }
    }
    let body = json!({"function": "FUN_140be3a90 (ThrowParam 11020190)", "cases": cases});
    std::fs::write(out.join("throw2_speffect.json"), serde_json::to_string_pretty(&body).unwrap()).unwrap();
    println!("throw2_speffect: {} cases: {:?}", cases.len(), cases.iter().map(|c| c["row_ok"].as_u64().unwrap()).collect::<Vec<_>>());
}
