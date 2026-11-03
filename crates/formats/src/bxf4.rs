//! BXF4 split binders: a `BHF4` header file (`.tpfbhd`, `.hkxbhd`) listing named entries whose
//! payloads live in a separate `BDF4` data file (`.tpfbdt`, `.hkxbdt`).
//!
//! The header is laid out exactly like a BND4 header (same flags, format byte and per-file
//! records); only the payload offsets point into the BDT. Layout knowledge from
//! SoulsFormatsNEXT `BXF4`, reimplemented.

use crate::reader::{Reader, Result, bail, sjisz, slice, utf16z};

/// One entry of a split binder. `data` is the raw (usually DCX-compressed) payload.
#[derive(Debug, Clone)]
pub struct BxfFile<'a> {
    pub id: i32,
    pub name: String,
    pub flags: u8,
    pub data: &'a [u8],
}

impl BxfFile<'_> {
    /// File name without the virtual directory prefix.
    pub fn file_name(&self) -> &str {
        self.name.rsplit(['\\', '/']).next().unwrap_or(&self.name)
    }

    /// Payload, decompressed when it is a DCX container or flagged compressed.
    pub fn contents(&self, oodle: Option<&crate::dcx::Oodle>) -> Result<Vec<u8>> {
        if crate::dcx::is_dcx(self.data) {
            crate::dcx::decompress(self.data, oodle)
        } else if self.flags & 1 != 0 {
            crate::dcx::inflate(self.data)
        } else {
            Ok(self.data.to_vec())
        }
    }
}

pub fn is_bhf4(data: &[u8]) -> bool {
    data.starts_with(b"BHF4")
}

/// Parses a BHF4 header and resolves each entry's payload in the BDF4 data file.
pub fn parse<'a>(bhd: &[u8], bdt: &'a [u8]) -> Result<Vec<BxfFile<'a>>> {
    if !bdt.starts_with(b"BDF4") {
        return bail("BXF4 data file does not start with BDF4");
    }
    let mut r = Reader::new(bhd);
    r.magic(b"BHF4")?;
    if bhd.get(9).copied().unwrap_or(0) != 0 {
        return bail("big-endian BHF4 is not used on PC");
    }
    let bit_big_endian = bhd.get(10).copied().unwrap_or(0) == 0;
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
        let mut h = Reader::at(bhd, 64 + i * header_size);
        let raw_flags = h.u8()?;
        let flags = if bit_big_endian {
            raw_flags
        } else {
            raw_flags.reverse_bits()
        };
        h.skip(7);
        let size = h.u64()? as usize;
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
                utf16z(bhd, name_offset)?
            } else {
                sjisz(bhd, name_offset)?
            }
        } else {
            String::new()
        };
        files.push(BxfFile {
            id,
            name,
            flags,
            data: slice(bdt, offset, size)?,
        });
    }
    Ok(files)
}
