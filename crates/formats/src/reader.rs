//! Bounds-checked little-endian reading over byte slices.

use std::fmt;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Format(String),
    #[error("read of {len} bytes at {offset:#x} exceeds buffer of {size:#x}")]
    OutOfBounds {
        offset: usize,
        len: usize,
        size: usize,
    },
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

pub fn bail<T>(message: impl fmt::Display) -> Result<T> {
    Err(Error::Format(message.to_string()))
}

/// A cursor over a byte slice. All reads are bounds-checked and little-endian unless stated.
#[derive(Clone)]
pub struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

macro_rules! read_le {
    ($($name:ident: $ty:ty),* $(,)?) => {
        $(
            pub fn $name(&mut self) -> Result<$ty> {
                let bytes = self.bytes(std::mem::size_of::<$ty>())?;
                Ok(<$ty>::from_le_bytes(bytes.try_into().unwrap()))
            }
        )*
    };
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    pub fn at(data: &'a [u8], pos: usize) -> Self {
        Self { data, pos }
    }

    pub fn data(&self) -> &'a [u8] {
        self.data
    }

    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn seek(&mut self, pos: usize) {
        self.pos = pos;
    }

    pub fn skip(&mut self, len: usize) {
        self.pos += len;
    }

    pub fn bytes(&mut self, len: usize) -> Result<&'a [u8]> {
        let out = slice(self.data, self.pos, len)?;
        self.pos += len;
        Ok(out)
    }

    pub fn magic(&mut self, expected: &[u8]) -> Result<()> {
        let found = self.bytes(expected.len())?;
        if found != expected {
            return bail(format!(
                "bad magic at {:#x}: expected {:?}, found {:?}",
                self.pos - expected.len(),
                String::from_utf8_lossy(expected),
                String::from_utf8_lossy(found)
            ));
        }
        Ok(())
    }

    read_le!(
        u8: u8, i8: i8, u16: u16, i16: i16, u32: u32, i32: i32, u64: u64, i64: i64, f32: f32
    );

    pub fn u32_be(&mut self) -> Result<u32> {
        Ok(u32::from_be_bytes(self.bytes(4)?.try_into().unwrap()))
    }

    pub fn vec3(&mut self) -> Result<[f32; 3]> {
        Ok([self.f32()?, self.f32()?, self.f32()?])
    }

    pub fn vec4(&mut self) -> Result<[f32; 4]> {
        Ok([self.f32()?, self.f32()?, self.f32()?, self.f32()?])
    }

    /// Reads a 64-bit offset and returns it as usize.
    pub fn offset64(&mut self) -> Result<usize> {
        let v = self.i64()?;
        usize::try_from(v).map_err(|_| Error::Format(format!("negative offset {v}")))
    }

    pub fn offset32(&mut self) -> Result<usize> {
        let v = self.i32()?;
        usize::try_from(v).map_err(|_| Error::Format(format!("negative offset {v}")))
    }
}

pub fn slice(data: &[u8], offset: usize, len: usize) -> Result<&[u8]> {
    offset
        .checked_add(len)
        .filter(|&end| end <= data.len())
        .map(|end| &data[offset..end])
        .ok_or(Error::OutOfBounds {
            offset,
            len,
            size: data.len(),
        })
}

/// Reads a NUL-terminated UTF-16LE string.
pub fn utf16z(data: &[u8], offset: usize) -> Result<String> {
    let mut units = Vec::new();
    let mut pos = offset;
    loop {
        let pair = slice(data, pos, 2)?;
        let unit = u16::from_le_bytes([pair[0], pair[1]]);
        if unit == 0 {
            break;
        }
        units.push(unit);
        pos += 2;
    }
    Ok(String::from_utf16_lossy(&units))
}

/// Reads a NUL-terminated Shift-JIS string.
pub fn sjisz(data: &[u8], offset: usize) -> Result<String> {
    let rest = data.get(offset..).ok_or(Error::OutOfBounds {
        offset,
        len: 1,
        size: data.len(),
    })?;
    let end = rest.iter().position(|&b| b == 0).unwrap_or(rest.len());
    let (text, _, _) = encoding_rs::SHIFT_JIS.decode(&rest[..end]);
    Ok(text.into_owned())
}
