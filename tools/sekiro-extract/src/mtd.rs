//! MTD material definitions (`mtd/allmaterialbnd.mtdbnd.d/*.mtd`): the shader, its parameters
//! (g_AlphaRef, g_BlendMode, ...) and the texture slots with their default paths and UV channel.
//! Sekiro FLVER materials leave texture paths empty; the MTD supplies them. Layout follows
//! SoulsFormatsNEXT `MTD.cs`.

use crate::bin::Reader;

pub struct Texture {
    /// Slot name, matching the FLVER material texture param.
    pub kind: String,
    /// 1-based UV channel.
    pub uv_number: i32,
    pub path: String,
}

pub struct Mtd {
    pub shader: String,
    pub ints: Vec<(String, Vec<i32>)>,
    pub floats: Vec<(String, Vec<f32>)>,
    pub textures: Vec<Texture>,
}

impl Mtd {
    pub fn int(&self, name: &str) -> Option<i32> {
        self.ints.iter().find(|(n, _)| n == name).and_then(|(_, v)| v.first().copied())
    }
}

struct Mr<'a> {
    r: Reader<'a>,
    o: usize,
}

impl<'a> Mr<'a> {
    fn pad4(&mut self) {
        self.o = self.o.next_multiple_of(4);
    }
    fn u32(&mut self) -> u32 {
        let v = self.r.u32(self.o);
        self.o += 4;
        v
    }
    fn i32(&mut self) -> i32 {
        self.u32() as i32
    }
    fn marker(&mut self) -> u8 {
        let m = self.r.u8(self.o);
        self.o += 1;
        self.pad4();
        m
    }
    fn string(&mut self) -> String {
        let len = self.u32() as usize;
        let bytes = &self.r.d[self.o..self.o + len];
        self.o += len;
        // Shift-JIS names: only ASCII matters here (slot names, shader paths).
        let s = String::from_utf8_lossy(bytes).into_owned();
        self.marker();
        s
    }
    /// Block header: returns (end offset, version).
    fn block(&mut self) -> (usize, i32) {
        self.u32(); // 0
        let length = self.u32() as usize;
        let start = self.o;
        self.i32(); // type
        let version = self.i32();
        self.marker();
        (start + length, version)
    }
}

pub fn read(d: &[u8]) -> Option<Mtd> {
    let mut m = Mr { r: Reader::new(d), o: 0 };
    m.block();
    let (header_end, _) = m.block();
    if m.string() != "MTD " {
        return None;
    }
    m.o = header_end;
    m.marker();
    m.block(); // data
    let shader = m.string();
    let _description = m.string();
    m.u32();
    m.block(); // lists
    m.u32();
    m.marker();
    let param_count = m.u32() as usize;
    let mut ints = Vec::new();
    let mut floats = Vec::new();
    for _ in 0..param_count {
        m.block();
        let name = m.string();
        let kind = m.string().to_lowercase();
        m.u32();
        m.block();
        let count = m.u32() as usize;
        match kind.as_str() {
            "bool" => {
                ints.push((name, vec![m.r.u8(m.o) as i32]));
                m.o += 1;
            }
            "int" | "int2" => ints.push((name, (0..count).map(|_| m.i32()).collect())),
            _ => floats.push((name, (0..count).map(|_| f32::from_bits(m.u32())).collect())),
        }
        m.marker();
        m.u32();
    }
    m.marker();
    let texture_count = m.u32() as usize;
    let mut textures = Vec::with_capacity(texture_count);
    for _ in 0..texture_count {
        let (_, version) = m.block();
        let kind = m.string();
        let uv_number = m.i32();
        m.marker();
        let _shader_data_index = m.i32();
        let path = if version == 5 {
            m.u32();
            let path = m.string();
            let n = m.u32() as usize;
            m.o += n * 4;
            path
        } else {
            String::new()
        };
        textures.push(Texture { kind, uv_number, path });
    }
    Some(Mtd { shader, ints, floats, textures })
}
