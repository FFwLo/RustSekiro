//! Scripted input for the real Sekiro (docs/kb/rec-research/input-hks.md A): a pre-hook on the
//! ActionRequest module update FUN_140b2c190 (rcx = module). There the move stick is final and the
//! module has not yet turned "held" into pressed / pending / requested, so writing our held bits and
//! stick there drives Wolf exactly as the pad would. Only the player's module is touched
//! (rcx == [[player ChrIns+0x1FF8]+0x80]; enemies' ComManipulators call it too).
//!
//! Hand-off (exported, written by rec2.lua's `play <file>` inbox command):
//!   SV1REC_PLAYFILE  script path (NUL-terminated)
//!   SV1REC_PLAY      1 = load the file and play from frame 0; the DLL sets it back to 0 at the end
//!   SV1REC_PLAYFRAME current script frame (one per module update of the player = one game frame)
//! Script: one op per line, `#` comments, numbers decimal or 0x hex. An op holds its value every
//! frame from its frame on, until `free` or another op on the same field:
//!   <frame> stick <x> <y>                camera-relative stick (x right, y forward, length <= 1)
//!   <frame> f32 <pad|mod> <off> <value>  float at manipulator / module + off
//!   <frame> u8 <pad|mod> <off> <value>
//!   <frame> bit <pad|mod> <off> <bit> <0|1>   bit 0..63 of the u64 at + off (mod 0x10 = held actions)
//!   <frame> free <pad|mod|stick> [off] [bit]   stop overriding (the game's own value shows again)
//!   <frame> end                          stop, free everything
//! Action names -> bits live in the Python side (tools/rec), so a layout fix needs no rebuild.
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

#[no_mangle]
pub static SV1REC_PLAY: AtomicU32 = AtomicU32::new(0);
#[no_mangle]
pub static SV1REC_PLAYFRAME: AtomicU32 = AtomicU32::new(0);
#[no_mangle]
pub static mut SV1REC_PLAYFILE: [u8; 512] = [0; 512];

const WCM: u64 = 0x1_43d7_a1e0;
/// FieldArea; [+0x30] = ChrCam: +0x10 right row, +0x30 forward row (rec chrcam block, 2026-10-21)
const FIELD_AREA: u64 = 0x1_43d5_c0a0;

#[derive(Clone, Copy, PartialEq)]
enum Tgt {
    Pad,
    Mod,
}

#[derive(Clone, Copy)]
enum Val {
    F32(f32),
    U8(u8),
    Bit(u8, bool),
}

enum Op {
    Stick(f32, f32),
    Set(Tgt, usize, Val),
    Free(Option<Tgt>, Option<usize>, Option<u8>),
    End,
}

struct Player {
    ops: Vec<(u32, Op)>,
    next: usize,
    frame: u32,
    stick: Option<(f32, f32)>,
    /// (target, byte offset, bit or 8 for a whole field) -> value held every frame
    held: BTreeMap<(u8, usize, u8), Val>,
}

static PLAYER: Mutex<Option<Player>> = Mutex::new(None);

fn num(s: &str) -> Option<f64> {
    match s.strip_prefix("0x") {
        Some(h) => u64::from_str_radix(h, 16).ok().map(|v| v as f64),
        None => s.parse().ok(),
    }
}

fn tgt(s: Option<&&str>) -> Option<Tgt> {
    match s.copied() {
        Some("pad") => Some(Tgt::Pad),
        Some("mod") => Some(Tgt::Mod),
        _ => None,
    }
}

fn parse(text: &str) -> Vec<(u32, Op)> {
    let mut ops = Vec::new();
    for line in text.lines() {
        let w: Vec<&str> = line.split('#').next().unwrap_or("").split_whitespace().collect();
        let n = |i: usize| w.get(i).and_then(|s| num(s));
        let (Some(frame), Some(kind)) = (n(0), w.get(1)) else { continue };
        let t = tgt(w.get(2));
        let op = match *kind {
            "stick" => n(2).zip(n(3)).map(|(x, y)| Op::Stick(x as f32, y as f32)),
            "f32" => t.zip(n(3)).zip(n(4)).map(|((t, o), v)| Op::Set(t, o as usize, Val::F32(v as f32))),
            "u8" => t.zip(n(3)).zip(n(4)).map(|((t, o), v)| Op::Set(t, o as usize, Val::U8(v as u8))),
            "bit" => match (t, n(3), n(4), n(5)) {
                (Some(t), Some(o), Some(b), Some(v)) => {
                    let b = (b as usize).min(63);
                    Some(Op::Set(t, o as usize + b / 8, Val::Bit((b % 8) as u8, v != 0.0)))
                }
                _ => None,
            },
            "free" if w.get(2) == Some(&"stick") => Some(Op::Free(None, None, None)),
            "free" => t.map(|t| Op::Free(Some(t), n(3).map(|o| o as usize), n(4).map(|b| b as u8))),
            "end" => Some(Op::End),
            _ => None,
        };
        if let Some(op) = op {
            ops.push((frame as u32, op));
        }
    }
    ops.sort_by_key(|(f, _)| *f);
    ops
}

fn ok(p: u64) -> bool {
    (0x10000..0x7FFF_FFFF_0000).contains(&p)
}

unsafe fn rd(p: u64) -> u64 {
    if ok(p) { (p as *const u64).read_unaligned() } else { 0 }
}

unsafe fn rdf(p: u64) -> f32 {
    if ok(p) { (p as *const f32).read_unaligned() } else { 0.0 }
}

/// (manipulator, ActionRequest module) of the player
unsafe fn player() -> (u64, u64) {
    let chr = rd(rd(WCM) + 0x88);
    if !ok(chr) {
        return (0, 0);
    }
    (rd(chr + 0x58), rd(rd(chr + 0x1FF8) + 0x80))
}

fn load() -> Option<Player> {
    let path = unsafe {
        let d = &*std::ptr::addr_of!(SV1REC_PLAYFILE);
        let n = d.iter().position(|&b| b == 0).unwrap_or(d.len());
        String::from_utf8_lossy(&d[..n]).into_owned()
    };
    let text = std::fs::read_to_string(path).ok()?;
    Some(Player { ops: parse(&text), next: 0, frame: 0, stick: None, held: BTreeMap::new() })
}

/// Camera-relative stick -> the world vector the pad writes (x, z on the ground plane)
unsafe fn world_stick(x: f32, y: f32) -> (f32, f32) {
    let cam = rd(rd(FIELD_AREA) + 0x30);
    let (rx, rz, fx, fz) = (rdf(cam + 0x10), rdf(cam + 0x18), rdf(cam + 0x30), rdf(cam + 0x38));
    let (rl, fl) = ((rx * rx + rz * rz).sqrt().max(1e-6), (fx * fx + fz * fz).sqrt().max(1e-6));
    (x * rx / rl + y * fx / fl, x * rz / rl + y * fz / fl)
}

/// rcx of FUN_140b2c190 is the player's module (enemies' ComManipulators call it too)
pub fn is_player_module(module: u64) -> bool {
    let (_, md) = unsafe { player() };
    ok(md) && module == md
}

/// Pre-hook body: `module` = rcx of FUN_140b2c190. Returns a log line at start / end.
pub unsafe fn on_module_update(module: u64) -> Option<String> {
    if SV1REC_PLAY.load(Ordering::Acquire) == 0 {
        return None;
    }
    let (pad, md) = player();
    if !ok(pad) || !ok(md) || module != md {
        return None;
    }
    let Ok(mut g) = PLAYER.lock() else { return None };
    let mut log = None;
    if g.is_none() {
        *g = Some(load()?);
        log = Some(",\"play\":\"start\"".to_string());
    }
    let p = g.as_mut().unwrap();
    let mut ended = false;
    while p.next < p.ops.len() && p.ops[p.next].0 <= p.frame {
        match p.ops[p.next].1 {
            Op::Stick(x, y) => p.stick = Some((x, y)),
            Op::Set(t, o, v) => {
                let bit = if let Val::Bit(b, _) = v { b } else { 8 };
                p.held.insert((t as u8, o, bit), v);
            }
            Op::Free(None, _, _) => p.stick = None,
            Op::Free(Some(t), o, b) => p.held.retain(|k, _| {
                !(k.0 == t as u8 && (o.is_none() || Some(k.1) == o) && (b.is_none() || Some(k.2) == b))
            }),
            Op::End => ended = true,
        }
        p.next += 1;
    }
    if p.next >= p.ops.len() && p.held.is_empty() && p.stick.is_none() {
        ended = true;
    }
    if !ended {
        if let Some((x, y)) = p.stick {
            // pad +0x10 (locomotion) and +0xC0 (camera chase) = world vector x,0,z; FUN_1407f1710
            let (wx, wz) = world_stick(x, y);
            for o in [0x10u64, 0xC0] {
                ((pad + o) as *mut [f32; 4]).write_unaligned([wx, 0.0, wz, 0.0]);
            }
            let moving = x * x + y * y > 1e-12;
            *((pad + 0x438) as *mut u8) = moving as u8;
            let f = (md + 0x184) as *mut u32;
            *f = if moving { *f | 1 } else { *f & !1 };
        }
        for (&(t, o, _), v) in &p.held {
            let at = ((if t == Tgt::Pad as u8 { pad } else { md }) + o as u64) as *mut u8;
            match *v {
                Val::F32(x) => (at as *mut f32).write_unaligned(x),
                Val::U8(x) => *at = x,
                Val::Bit(b, on) => *at = if on { *at | (1 << b) } else { *at & !(1 << b) },
            }
        }
    }
    SV1REC_PLAYFRAME.store(p.frame, Ordering::Relaxed);
    p.frame += 1;
    if ended {
        log = Some(format!(",\"play\":\"end\",\"frames\":{}", p.frame));
        *g = None;
        SV1REC_PLAY.store(0, Ordering::Release);
    }
    log
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_sorts() {
        let ops = parse("# t\n10 bit mod 0x10 14 1\n0 stick 0 1\n20 free mod 0x10\n25 free stick\n30 end\nbad line\n");
        assert_eq!(ops.iter().map(|o| o.0).collect::<Vec<_>>(), vec![0, 10, 20, 25, 30]);
        // bit 14 of the u64 at +0x10 = byte +0x11, bit 6
        assert!(matches!(ops[1].1, Op::Set(Tgt::Mod, 0x11, Val::Bit(6, true))));
        assert!(matches!(ops[3].1, Op::Free(None, None, None)));
    }
}
