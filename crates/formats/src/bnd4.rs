//! BND4 binders: flat containers of named files (`.chrbnd`, `.anibnd`, `.parambnd`, ...).

use crate::reader::{Reader, Result, bail, sjisz, slice, utf16z};

#[derive(Debug, Clone)]
pub struct BndFile<'a> {
    pub id: i32,
    pub name: String,
    pub flags: u8,
    pub data: &'a [u8],
}

impl BndFile<'_> {
    /// File name without the virtual directory prefix the binder stores.
    pub fn file_name(&self) -> &str {
        self.name.rsplit(['\\', '/']).next().unwrap_or(&self.name)
    }

    pub fn compressed(&self) -> bool {
        self.flags & 1 != 0
    }

    /// Payload, decompressed if the binder marked it compressed.
    pub fn contents(&self, oodle: Option<&crate::dcx::Oodle>) -> Result<Vec<u8>> {
        if crate::dcx::is_dcx(self.data) {
            crate::dcx::decompress(self.data, oodle)
        } else if self.compressed() {
            crate::dcx::inflate(self.data)
        } else {
            Ok(self.data.to_vec())
        }
    }
}

pub fn is_bnd4(data: &[u8]) -> bool {
    data.starts_with(b"BND4")
}

pub fn parse(data: &[u8]) -> Result<Vec<BndFile<'_>>> {
    let mut r = Reader::new(data);
    r.magic(b"BND4")?;
    let _flag1 = r.u8()?;
    let _flag2 = r.u8()?;
    r.skip(3);
    let big_endian = data.get(9).copied().unwrap_or(0) != 0;
    if big_endian {
        return bail("big-endian BND4 is not used on PC");
    }
    let bit_big_endian = data.get(10).copied().unwrap_or(0) == 0;
    r.seek(12);
    let count = r.u32()? as usize;
    r.seek(32);
    let header_size = r.u64()? as usize;
    r.seek(48);
    let unicode = r.u8()? != 0;
    let raw_format = r.u8()?;
    let format = if bit_big_endian || (raw_format & 1 != 0 && raw_format & 0x80 == 0) {
        raw_format
    } else {
        raw_format.reverse_bits()
    };

    let mut files = Vec::with_capacity(count);
    for i in 0..count {
        let mut h = Reader::at(data, 64 + i * header_size);
        let raw_flags = h.u8()?;
        let flags = if bit_big_endian {
            raw_flags
        } else {
            raw_flags.reverse_bits()
        };
        h.skip(7);
        let compressed_size = h.u64()? as usize;
        if format & 0x20 != 0 {
            h.skip(8); // uncompressed size
        }
        let offset = if format & 0x10 != 0 {
            h.u64()? as usize
        } else {
            h.u32()? as usize
        };
        let id = if format & 0x02 != 0 { h.i32()? } else { -1 };
        let name = if format & 0x0C != 0 {
            let name_offset = h.u32()? as usize;
            if unicode {
                utf16z(data, name_offset)?
            } else {
                sjisz(data, name_offset)?
            }
        } else {
            String::new()
        };
        files.push(BndFile {
            id,
            name,
            flags,
            data: slice(data, offset, compressed_size)?,
        });
    }
    Ok(files)
}
