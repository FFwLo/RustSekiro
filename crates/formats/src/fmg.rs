//! Sekiro FMG v2 text banks: grouped IDs, 64-bit offsets and UTF-16LE strings.
//! Layout follows FwLo's RustSekiro `tools/sekiro-extract/src/fmg.rs`.
use std::collections::BTreeMap;

use crate::reader::{Reader, Result, bail, slice, utf16z};

pub fn parse(data: &[u8]) -> Result<BTreeMap<i32, String>> {
    let mut r = Reader::new(data);
    if r.bytes(4)? != [0, 0, 2, 0] {
        return bail("expected little-endian FMG v2");
    }
    let size = r.u32()? as usize;
    if size != data.len() {
        return bail("FMG size does not match header");
    }
    r.seek(0x0c);
    let groups = r.u32()? as usize;
    let count = r.u32()? as usize;
    r.seek(0x18);
    let offsets = r.offset64()?;
    slice(
        data,
        0x28,
        groups
            .checked_mul(16)
            .ok_or_else(|| crate::Error::Format("FMG group overflow".into()))?,
    )?;
    slice(
        data,
        offsets,
        count
            .checked_mul(8)
            .ok_or_else(|| crate::Error::Format("FMG offset overflow".into()))?,
    )?;
    let mut out = BTreeMap::new();
    for group in 0..groups {
        r.seek(0x28 + group * 16);
        let index = r.u32()? as usize;
        let first = r.i32()?;
        let last = r.i32()?;
        let length = i64::from(last) - i64::from(first) + 1;
        if length <= 0
            || length as u64 > count as u64
            || index
                .checked_add(length as usize)
                .is_none_or(|end| end > count)
        {
            return bail("FMG group outside string table");
        }
        for (k, id) in (first..=last).enumerate() {
            r.seek(offsets + (index + k) * 8);
            let at = r.offset64()?;
            if at == 0 {
                continue;
            }
            let text = utf16z(data, at)?;
            if !text.is_empty() && text != "<?null?>" {
                out.insert(id, text);
            }
        }
    }
    Ok(out)
}
