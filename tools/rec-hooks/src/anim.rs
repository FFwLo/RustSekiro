//! Per-frame animation sample of the player (docs/kb/rec-research/anim-pose.md), taken on the game thread from the
//! once-per-frame ActionRequest hook (see lib.rs PAD_PLAY):
//!   "anim" -> one hooks.jsonl line per frame: every active hkb clip generator (name, anim id, local time, master
//!             weight, playback speed, at-end, root-motion block) + ragdoll state
//!   "pose" -> pose.bin: per frame u32 tick, u16 bone count, then count x 0x30 bytes local hkQsTransforms
//! Every pointer is checked with VirtualQuery before it is read (a stale pointer must never crash the game).
use std::ffi::c_void;

const WCM: u64 = 0x1_43d7_a1e0;

#[repr(C)]
struct MemInfo {
    base: usize,
    alloc_base: usize,
    alloc_protect: u32,
    partition: u16,
    region_size: usize,
    state: u32,
    protect: u32,
    kind: u32,
}

extern "system" {
    fn VirtualQuery(addr: *const c_void, info: *mut MemInfo, len: usize) -> usize;
}

/// Committed, readable, not a guard page, for all n bytes
fn readable(p: u64, n: usize) -> bool {
    if !(0x10000..0x7FFF_FFFF_0000).contains(&p) {
        return false;
    }
    let mut at = p as usize;
    let end = p as usize + n.max(1);
    while at < end {
        let mut m: MemInfo = unsafe { std::mem::zeroed() };
        if unsafe { VirtualQuery(at as *const c_void, &mut m, std::mem::size_of::<MemInfo>()) } == 0 {
            return false;
        }
        // MEM_COMMIT 0x1000; PAGE_NOACCESS 0x01, PAGE_GUARD 0x100
        if m.state != 0x1000 || m.protect & 0x101 != 0 || m.protect == 0 {
            return false;
        }
        at = m.base + m.region_size;
    }
    true
}

fn r64(p: u64) -> Option<u64> {
    readable(p, 8).then(|| unsafe { (p as *const u64).read_unaligned() })
}
fn r32(p: u64) -> Option<i32> {
    readable(p, 4).then(|| unsafe { (p as *const i32).read_unaligned() })
}
fn rf(p: u64) -> Option<f32> {
    readable(p, 4).then(|| unsafe { (p as *const f32).read_unaligned() })
}
fn r8(p: u64) -> Option<u8> {
    readable(p, 1).then(|| unsafe { *(p as *const u8) })
}
fn bytes(p: u64, n: usize) -> Option<&'static [u8]> {
    readable(p, n).then(|| unsafe { std::slice::from_raw_parts(p as *const u8, n) })
}
fn cstr(p: u64) -> String {
    let p = p & !1;
    let mut s = String::new();
    for i in 0..96 {
        match r8(p + i) {
            Some(0) | None => break,
            Some(c) if c.is_ascii_graphic() && c != b'"' && c != b'\\' => s.push(c as char),
            _ => {}
        }
    }
    s
}
fn num(x: f32) -> String {
    if x.is_finite() { format!("{x}") } else { "null".into() }
}
fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// The player's (ChrIns, module bag)
fn player() -> Option<(u64, u64)> {
    let chr = r64(r64(WCM)? + 0x88)?;
    Some((chr, r64(chr + 0x1FF8)?))
}

/// JSON fields (`,"clips":[...],...`) of the player's active clips + ragdoll state
pub fn clips() -> Option<String> {
    let (chr, bag) = player()?;
    let b = r64(bag + 0x28)?;
    let g = r64(r64(r64(r64(b + 0x10)? + 0x28)? + 0x98)? + 0xE0)?;
    let (data, count) = (r64(g)?, r32(g + 8)?);
    // anim table of the TimeAct resource: [R+0x70] entries x 0x48, +0x30 id; R+0x68 count
    let res = r64(r64(bag + 0x10)? + 0x10);
    let table = res.and_then(|r| Some((r64(r + 0x70)?, r32(r + 0x68)?)));
    let mut out = Vec::new();
    for i in 0..count.clamp(0, 512) as u64 {
        let Some(ni) = r64(data + i * 8) else { continue };
        if r8(ni + 0x3E).unwrap_or(0) & 2 == 0 {
            continue;
        }
        let Some(node) = r64(ni + 8) else { continue };
        if r8(node + 0x53) != Some(4) {
            continue;
        }
        let idx = r32(node + 0xC4).map(|v| (v >> 16) as i16 as i32).unwrap_or(-1); // i16 at +0xC6
        let anim = match table {
            Some((t, n)) if idx >= 0 && idx < n => r32(t + idx as u64 * 0x48 + 0x30).unwrap_or(-1),
            _ => -1,
        };
        let ctrl = r64(node + 0xD8).unwrap_or(0);
        out.push(format!(
            "{{\"name\":\"{}\",\"anim\":{anim},\"t\":{},\"w\":{},\"speed\":{},\"end\":{},\"rm\":\"{}\"}}",
            cstr(r64(node + 0x98).unwrap_or(0)),
            num(rf(node + 0x140).unwrap_or(f32::NAN)),
            num(rf(ctrl + 0x1C).unwrap_or(f32::NAN)),
            num(rf(ctrl + 0x64).unwrap_or(f32::NAN)),
            r8(node + 0x150).unwrap_or(0),
            bytes(node + 0x100, 0x30).map(hex).unwrap_or_default()
        ));
    }
    let ctrl = r64(chr + 0x50).unwrap_or(0);
    let rag = r64(ctrl + 0x28).and_then(|r| r32(r + 0xEC)).unwrap_or(-9);
    Some(format!(
        ",\"clips\":[{}],\"rag_req\":{},\"rag_state\":{},\"rag\":{rag},\"pose_on\":{}",
        out.join(","),
        r32(ctrl + 0x1C8).unwrap_or(-9),
        r32(ctrl + 0x1CC).unwrap_or(-9),
        r8(b + 0xBC1).unwrap_or(9)
    ))
}

/// One pose.bin record of the player's local pose
pub fn pose(tick: u32) -> Option<Vec<u8>> {
    let (_, bag) = player()?;
    let imp = r64(r64(bag + 0x28)? + 0x28)?;
    let (data, n) = (r64(imp + 0x50)?, r32(imp + 0x58)?);
    if !(1..=1024).contains(&n) {
        return None;
    }
    let b = bytes(data, n as usize * 0x30)?;
    let mut rec = Vec::with_capacity(6 + b.len());
    rec.extend_from_slice(&tick.to_le_bytes());
    rec.extend_from_slice(&(n as u16).to_le_bytes());
    rec.extend_from_slice(b);
    Some(rec)
}

/// SprjBulletManager [0x143D84F70] (docs/kb/rec-research/ai-world-fx.md E): +0 pool of 0x80 slots x 0xD30; slot +8 handle,
/// +0x30 position, +0xB60 metres flown, +0xC6C flags (bit 1 active, 4 destroy requested, 8 flying); +0x1BC active count.
/// JSON fields of every active bullet, None when there is none.
pub fn bullets() -> Option<String> {
    let mgr = r64(0x1_43d8_4f70)?;
    if r32(mgr + 0x1BC)? <= 0 {
        return None;
    }
    let pool = r64(mgr)?;
    let all = bytes(pool, 0x80 * 0xD30)?;
    let f = |o: usize| f32::from_le_bytes(all[o..o + 4].try_into().unwrap());
    let mut out = Vec::new();
    for i in 0..0x80usize {
        let s = i * 0xD30;
        let flags = all[s + 0xC6C];
        if flags & 2 == 0 {
            continue;
        }
        out.push(format!("[{},{},{},{},{},{}]", u32::from_le_bytes(all[s + 8..s + 12].try_into().unwrap()),
            num(f(s + 0x30)), num(f(s + 0x34)), num(f(s + 0x38)), num(f(s + 0xB60)), flags));
    }
    (!out.is_empty()).then(|| format!(",\"b\":[{}]", out.join(",")))
}

#[cfg(test)]
mod tests {
    #[test]
    fn rejects_bad_pointers() {
        assert!(!super::readable(0, 8));
        assert!(!super::readable(0x1234, 8));
        let x = 5u64;
        assert!(super::readable(&x as *const u64 as u64, 8));
    }
}
