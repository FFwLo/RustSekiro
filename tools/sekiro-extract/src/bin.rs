//! Little-endian byte reader helpers.

pub struct Reader<'a> {
    pub d: &'a [u8],
}

impl<'a> Reader<'a> {
    pub fn new(d: &'a [u8]) -> Self {
        Self { d }
    }
    pub fn u8(&self, o: usize) -> u8 {
        self.d[o]
    }
    pub fn u16(&self, o: usize) -> u16 {
        u16::from_le_bytes(self.d[o..o + 2].try_into().unwrap())
    }
    pub fn u32(&self, o: usize) -> u32 {
        u32::from_le_bytes(self.d[o..o + 4].try_into().unwrap())
    }
    pub fn i16(&self, o: usize) -> i16 {
        self.u16(o) as i16
    }
    pub fn i32(&self, o: usize) -> i32 {
        self.u32(o) as i32
    }
    pub fn u64(&self, o: usize) -> u64 {
        u64::from_le_bytes(self.d[o..o + 8].try_into().unwrap())
    }
    pub fn i64(&self, o: usize) -> i64 {
        self.u64(o) as i64
    }
    pub fn f32(&self, o: usize) -> f32 {
        f32::from_bits(self.u32(o))
    }
    pub fn cstr(&self, o: usize) -> String {
        let end = self.d[o..].iter().position(|&b| b == 0).map_or(self.d.len(), |p| o + p);
        String::from_utf8_lossy(&self.d[o..end]).into_owned()
    }
    pub fn utf16z(&self, o: usize) -> String {
        let mut v = Vec::new();
        let mut p = o;
        loop {
            let c = self.u16(p);
            if c == 0 {
                break;
            }
            v.push(c);
            p += 2;
        }
        String::from_utf16_lossy(&v)
    }
}
