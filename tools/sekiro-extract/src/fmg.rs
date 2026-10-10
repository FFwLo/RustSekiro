//! FMG text banks (msg/<lang>/*.msgbnd: item, menu names), version 2 (64-bit offsets):
//!   0x0C i32 groups, 0x10 i32 strings, 0x18 i64 string-offset table; groups at 0x28, 16 bytes each
//!   (i32 first string index, i32 first id, i32 last id, pad); strings UTF-16LE, 0-terminated.

use std::collections::BTreeMap;

pub fn read(d: &[u8]) -> BTreeMap<i64, String> {
    let i32_at = |o: usize| i32::from_le_bytes(d[o..o + 4].try_into().unwrap());
    let i64_at = |o: usize| i64::from_le_bytes(d[o..o + 8].try_into().unwrap());
    let mut out = BTreeMap::new();
    if d.len() < 0x28 || d[2] != 2 {
        return out;
    }
    let groups = i32_at(0x0C).max(0) as usize;
    let table = i64_at(0x18) as usize;
    for g in 0..groups {
        let o = 0x28 + g * 16;
        let (idx, first, last) = (i32_at(o) as usize, i32_at(o + 4) as i64, i32_at(o + 8) as i64);
        for (k, id) in (first..=last).enumerate() {
            let so = i64_at(table + (idx + k) * 8);
            if so <= 0 {
                continue;
            }
            let mut e = so as usize;
            while e + 1 < d.len() && (d[e] != 0 || d[e + 1] != 0) {
                e += 2;
            }
            let units: Vec<u16> = d[so as usize..e].chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
            let s = String::from_utf16_lossy(&units);
            if !s.is_empty() && s != "<?null?>" {
                out.insert(id, s);
            }
        }
    }
    out
}
