//! MTD material definitions (`mtd/allmaterialbnd.mtdbnd`).
//!
//! Sekiro FLVER materials name an MTD and list texture slots, but leave texture paths empty;
//! the MTD supplies the default texture path for each slot. Layout knowledge comes from
//! SoulsFormatsNEXT (`MTD.cs`), reimplemented.

use crate::reader::{Reader, Result, bail};

#[derive(Debug, Clone)]
pub struct Mtd {
    pub shader: String,
    pub description: String,
    pub params: Vec<Param>,
    pub textures: Vec<Texture>,
}

#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub kind: String,
    pub value: ParamValue,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ParamValue {
    Bool(bool),
    Ints(Vec<i32>),
    Floats(Vec<f32>),
}

#[derive(Debug, Clone)]
pub struct Texture {
    /// Slot name, matching the FLVER material texture `param`.
    pub kind: String,
    pub uv_number: i32,
    pub shader_data_index: i32,
    /// Default texture path, e.g. `N:\...\c1010_body_a.tga`. Empty when not extended.
    pub path: String,
}

impl Mtd {
    pub fn param(&self, name: &str) -> Option<&ParamValue> {
        self.params
            .iter()
            .find(|p| p.name == name)
            .map(|p| &p.value)
    }
}

struct Mr<'a> {
    r: Reader<'a>,
}

impl<'a> Mr<'a> {
    fn pad4(&mut self) {
        let p = self.r.pos();
        self.r.seek(p.next_multiple_of(4));
    }

    fn marker(&mut self, expected: u8) -> Result<()> {
        let m = self.r.u8()?;
        if m != expected {
            return bail(format!(
                "MTD marker {m:#x} != {expected:#x} at {:#x}",
                self.r.pos() - 1
            ));
        }
        self.pad4();
        Ok(())
    }

    fn any_marker(&mut self) -> Result<u8> {
        let m = self.r.u8()?;
        self.pad4();
        Ok(m)
    }

    fn string(&mut self, marker: u8) -> Result<String> {
        let len = self.r.u32()? as usize;
        let bytes = self.r.bytes(len)?;
        let (text, _, _) = encoding_rs::SHIFT_JIS.decode(bytes);
        self.marker(marker)?;
        Ok(text.into_owned())
    }

    /// Reads a block header; returns (end offset, type, version).
    fn block(&mut self) -> Result<(usize, i32, i32)> {
        if self.r.i32()? != 0 {
            return bail("MTD block does not start with 0");
        }
        let length = self.r.u32()? as usize;
        let start = self.r.pos();
        let kind = self.r.i32()?;
        let version = self.r.i32()?;
        self.any_marker()?;
        Ok((start + length, kind, version))
    }
}

pub fn parse(data: &[u8]) -> Result<Mtd> {
    let mut m = Mr {
        r: Reader::new(data),
    };
    m.block()?; // file
    let (header_end, _, _) = m.block()?;
    if m.string(0x34)? != "MTD " {
        return bail("not an MTD");
    }
    m.r.seek(header_end);
    m.marker(0x01)?;
    m.block()?; // data
    let shader = m.string(0xA3)?;
    let description = m.string(0x03)?;
    m.r.u32()?;
    m.block()?; // lists
    m.r.u32()?;
    m.marker(0x03)?;
    let param_count = m.r.u32()? as usize;
    let mut params = Vec::with_capacity(param_count);
    for _ in 0..param_count {
        m.block()?;
        let name = m.string(0xA3)?;
        let kind = m.string(0x04)?;
        m.r.u32()?;
        m.block()?;
        let count = m.r.u32()? as usize;
        let value = match kind.to_ascii_lowercase().as_str() {
            "bool" => ParamValue::Bool(m.r.u8()? != 0),
            "int" | "int2" => {
                ParamValue::Ints((0..count).map(|_| m.r.i32()).collect::<Result<Vec<_>>>()?)
            }
            _ => ParamValue::Floats((0..count).map(|_| m.r.f32()).collect::<Result<Vec<_>>>()?),
        };
        m.marker(0x04)?;
        m.r.u32()?;
        params.push(Param { name, kind, value });
    }
    m.marker(0x03)?;
    let texture_count = m.r.u32()? as usize;
    let mut textures = Vec::with_capacity(texture_count);
    for _ in 0..texture_count {
        let (_, _, version) = m.block()?;
        let kind = m.string(0x35)?;
        let uv_number = m.r.i32()?;
        m.marker(0x35)?;
        let shader_data_index = m.r.i32()?;
        let path = if version == 5 {
            m.r.u32()?;
            let path = m.string(0xBA)?;
            let floats = m.r.u32()? as usize;
            m.r.skip(floats * 4);
            path
        } else {
            String::new()
        };
        textures.push(Texture {
            kind,
            uv_number,
            shader_data_index,
            path,
        });
    }
    Ok(Mtd {
        shader,
        description,
        params,
        textures,
    })
}
