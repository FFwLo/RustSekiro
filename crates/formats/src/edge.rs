//! Authored action edges, version 4. Coordinates remain in the owner's local space.
use crate::reader::{Reader, Result, bail};

#[derive(Debug, Clone, PartialEq)]
pub struct Edge {
    pub start: [f32; 3],
    pub end: [f32; 3],
    pub offset: [f32; 3],
    pub chain: u32,
    pub flags: u32,
}

impl Edge {
    pub fn kind(&self) -> u8 {
        self.flags as u8
    }
    pub fn variation(&self) -> u8 {
        (self.flags >> 8) as u8
    }
}

#[derive(Debug, Clone)]
pub struct EdgeFile {
    pub owner: u32,
    pub edges: Vec<Edge>,
}

pub fn parse(data: &[u8]) -> Result<EdgeFile> {
    let mut r = Reader::new(data);
    if r.u32()? != 4 {
        return bail("unsupported edge version");
    }
    let count = r.u32()? as usize;
    let owner = r.u32()?;
    if r.u32()? != 0 || count.checked_mul(64).and_then(|n| n.checked_add(16)) != Some(data.len()) {
        return bail("invalid edge header or record length");
    }
    let mut edges = Vec::with_capacity(count);
    for _ in 0..count {
        let mut vector = || -> Result<[f32; 3]> {
            let v = [r.f32()?, r.f32()?, r.f32()?];
            r.f32()?;
            if v.iter().any(|f| !f.is_finite()) {
                return bail("non-finite edge coordinate");
            }
            Ok(v)
        };
        let start = vector()?;
        let end = vector()?;
        let offset = vector()?;
        let chain = r.u32()?;
        let flags = r.u32()?;
        r.skip(8);
        edges.push(Edge {
            start,
            end,
            offset,
            chain,
            flags,
        });
    }
    Ok(EdgeFile { owner, edges })
}
