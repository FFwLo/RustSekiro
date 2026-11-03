//! TPF texture containers and the DDS headers inside them.
//!
//! Layout knowledge comes from SoulsFormatsNEXT (`TPF/TPF.cs`), reimplemented. Only the PC
//! platform is supported: its payloads are plain DDS files.

use crate::reader::{Reader, Result, bail, sjisz, utf16z};

#[derive(Debug, Clone)]
pub struct TpfTexture<'a> {
    pub name: String,
    /// FromSoftware's own format byte (not DXGI).
    pub format: u8,
    /// 0 texture, 1 cubemap, 2 volume.
    pub kind: u8,
    pub mipmaps: u8,
    /// The DDS file, header included.
    pub dds: &'a [u8],
}

#[cfg(test)]
mod volume_tests {
    use super::*;

    #[test]
    fn bc7_volume_keeps_all_top_mip_slices_and_rejects_truncation() {
        let mut bytes = vec![0u8; 148 + 3 * 32];
        bytes[..4].copy_from_slice(b"DDS ");
        for (offset, value) in [
            (4, 124u32),
            (12, 4),
            (16, 8),
            (24, 3),
            (28, 2),
            (80, 4),
            (128, 98),
            (132, 4),
            (140, 1),
        ] {
            bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        bytes[84..88].copy_from_slice(b"DX10");
        bytes[148..180].fill(11);
        bytes[180..212].fill(22);
        bytes[212..244].fill(33);
        let header = Dds::parse(&bytes).unwrap();
        assert_eq!((header.width, header.height, header.depth), (8, 4, 3));
        assert_eq!(header.top_volume_mip(&bytes).unwrap().len(), 96);
        assert_eq!(header.top_volume_mip(&bytes).unwrap()[64], 33);
        assert!(header.top_volume_mip(&bytes[..243]).is_err());
    }
}

impl TpfTexture<'_> {
    pub fn header(&self) -> Result<Dds> {
        Dds::parse(self.dds)
    }
}

pub fn is_tpf(data: &[u8]) -> bool {
    data.starts_with(b"TPF\0")
}

/// Parses a PC TPF. Texture payloads borrow from `data`.
pub fn parse(data: &[u8]) -> Result<Vec<TpfTexture<'_>>> {
    let mut r = Reader::new(data);
    r.magic(b"TPF\0")?;
    let _data_length = r.u32()?;
    let count = r.u32()? as usize;
    let platform = r.u8()?;
    let _flag2 = r.u8()?;
    let encoding = r.u8()?;
    r.u8()?;
    if platform != 0 {
        return bail(format!("TPF platform {platform} is not PC"));
    }
    let mut textures = Vec::with_capacity(count);
    for _ in 0..count {
        let offset = r.u32()? as usize;
        let size = r.u32()? as usize;
        let format = r.u8()?;
        let kind = r.u8()?;
        let mipmaps = r.u8()?;
        let flags1 = r.u8()?;
        let name_offset = r.u32()? as usize;
        let has_float_struct = r.u32()? == 1;
        if has_float_struct {
            let _unk = r.i32()?;
            let len = r.u32()? as usize;
            r.skip(len);
        }
        if flags1 == 2 || flags1 == 3 {
            return bail("DCP_EDGE compressed TPF payloads are not supported");
        }
        let name = if encoding == 1 {
            utf16z(data, name_offset)?
        } else {
            sjisz(data, name_offset)?
        };
        textures.push(TpfTexture {
            name,
            format,
            kind,
            mipmaps,
            dds: crate::reader::slice(data, offset, size)?,
        });
    }
    Ok(textures)
}

/// Pixel formats found in Sekiro's PC textures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DdsFormat {
    Bc1,
    Bc2,
    Bc3,
    Bc4,
    Bc5,
    Bc6hUf16,
    Bc6hSf16,
    Bc7,
    Bgra8,
    Rgba8,
    Unknown(u32),
}

impl DdsFormat {
    /// Bytes per 4x4 block for block-compressed formats, or bytes per pixel otherwise.
    pub fn block_bytes(self) -> Option<(usize, bool)> {
        use DdsFormat::*;
        match self {
            Bc1 | Bc4 => Some((8, true)),
            Bc2 | Bc3 | Bc5 | Bc6hUf16 | Bc6hSf16 | Bc7 => Some((16, true)),
            Bgra8 | Rgba8 => Some((4, false)),
            Unknown(_) => None,
        }
    }

    fn from_dxgi(v: u32) -> (Self, bool) {
        use DdsFormat::*;
        match v {
            28 => (Rgba8, false),
            29 => (Rgba8, true),
            70..=71 => (Bc1, false),
            72 => (Bc1, true),
            73..=74 => (Bc2, false),
            75 => (Bc2, true),
            76..=77 => (Bc3, false),
            78 => (Bc3, true),
            79..=81 => (Bc4, false),
            82..=84 => (Bc5, false),
            87 => (Bgra8, false),
            91 => (Bgra8, true),
            94..=95 => (Bc6hUf16, false),
            96 => (Bc6hSf16, false),
            97..=98 => (Bc7, false),
            99 => (Bc7, true),
            other => (Unknown(other), false),
        }
    }
}

/// The parts of a DDS header needed to decode its first image.
#[derive(Debug, Clone, Copy)]
pub struct Dds {
    pub width: u32,
    pub height: u32,
    /// Number of Z slices for a volume, one for a 2D image.
    pub depth: u32,
    pub mip_count: u32,
    pub format: DdsFormat,
    pub srgb: bool,
    pub cubemap: bool,
    /// Byte offset of the pixel data (after the DX10 extension when present).
    pub data_offset: usize,
}

impl Dds {
    pub fn parse(data: &[u8]) -> Result<Self> {
        let mut r = Reader::new(data);
        r.magic(b"DDS ")?;
        let _size = r.u32()?;
        let _flags = r.u32()?;
        let height = r.u32()?;
        let width = r.u32()?;
        let _pitch = r.u32()?;
        let depth = r.u32()?.max(1);
        let mip_count = r.u32()?.max(1);
        r.seek(0x50);
        let pf_flags = r.u32()?;
        let fourcc = r.bytes(4)?;
        let bit_count = r.u32()?;
        let r_mask = r.u32()?;
        r.seek(0x70);
        let caps2 = r.u32()?;
        let mut data_offset = 0x80;
        let (format, srgb) = if pf_flags & 0x4 != 0 {
            match fourcc {
                b"DX10" => {
                    let mut x = Reader::at(data, 0x80);
                    let dxgi = x.u32()?;
                    data_offset += 20;
                    DdsFormat::from_dxgi(dxgi)
                }
                b"DXT1" => (DdsFormat::Bc1, false),
                b"DXT2" | b"DXT3" => (DdsFormat::Bc2, false),
                b"DXT4" | b"DXT5" => (DdsFormat::Bc3, false),
                b"ATI1" | b"BC4U" => (DdsFormat::Bc4, false),
                b"ATI2" | b"BC5U" => (DdsFormat::Bc5, false),
                other => (
                    DdsFormat::Unknown(u32::from_le_bytes(other.try_into().unwrap())),
                    false,
                ),
            }
        } else if bit_count == 32 {
            if r_mask == 0x00ff_0000 {
                (DdsFormat::Bgra8, false)
            } else {
                (DdsFormat::Rgba8, false)
            }
        } else {
            (DdsFormat::Unknown(0), false)
        };
        Ok(Self {
            width,
            height,
            depth,
            mip_count,
            format,
            srgb,
            cubemap: caps2 & 0x200 != 0,
            data_offset,
        })
    }

    /// Byte length of mip level 0 of the first image.
    pub fn top_mip_len(&self) -> Option<usize> {
        let (bytes, blocks) = self.format.block_bytes()?;
        let (w, h) = (self.width as usize, self.height as usize);
        Some(if blocks {
            w.div_ceil(4) * h.div_ceil(4) * bytes
        } else {
            w * h * bytes
        })
    }

    /// The pixel data of mip level 0 of the first image (first face for cubemaps).
    pub fn top_mip<'a>(&self, data: &'a [u8]) -> Result<&'a [u8]> {
        let Some(len) = self.top_mip_len() else {
            return bail(format!("unsupported DDS format {:?}", self.format));
        };
        crate::reader::slice(data, self.data_offset, len)
    }

    /// Mip level 0 as RGBA8 (BC1, BC4 as grey with alpha 255, and the 8-bit formats). Other
    /// formats return `None`; this exists for the few small textures a game system needs on the
    /// CPU (decal masks), not as a general decoder.
    #[allow(clippy::chunks_exact_to_as_chunks)]
    pub fn decode_rgba8(&self, data: &[u8]) -> Option<Vec<u8>> {
        let mip = self.top_mip(data).ok()?;
        let (w, h) = (self.width as usize, self.height as usize);
        let mut out = vec![0u8; w * h * 4];
        match self.format {
            DdsFormat::Bc1 => {
                for (b, block) in mip.chunks_exact(8).enumerate() {
                    let colors = bc1_colors(block);
                    let bits = u32::from_le_bytes([block[4], block[5], block[6], block[7]]);
                    put_block(&mut out, w, h, b, |i| {
                        colors[(bits >> (2 * i) & 3) as usize]
                    });
                }
            }
            DdsFormat::Bc4 => {
                for (b, block) in mip.chunks_exact(8).enumerate() {
                    let values = bc4_values(block);
                    let bits = u64::from_le_bytes([
                        block[2], block[3], block[4], block[5], block[6], block[7], 0, 0,
                    ]);
                    put_block(&mut out, w, h, b, |i| {
                        let v = values[(bits >> (3 * i) & 7) as usize];
                        [v, v, v, 255]
                    });
                }
            }
            DdsFormat::Rgba8 => out.copy_from_slice(mip.get(..w * h * 4)?),
            DdsFormat::Bgra8 => {
                for (o, p) in out.chunks_exact_mut(4).zip(mip.chunks_exact(4)) {
                    *o.first_mut()? = p[2];
                    o[1] = p[1];
                    o[2] = p[0];
                    o[3] = p[3];
                }
            }
            _ => return None,
        }
        Some(out)
    }

    /// Contiguous Z slices of mip zero, before the smaller volume mips.
    pub fn top_volume_mip<'a>(&self, data: &'a [u8]) -> Result<&'a [u8]> {
        let len = self
            .top_mip_len()
            .and_then(|n| n.checked_mul(self.depth as usize));
        let Some(len) = len else {
            return bail("unsupported or overflowing DDS volume");
        };
        crate::reader::slice(data, self.data_offset, len)
    }
}

/// The four colours of a BC1 block.
fn bc1_colors(block: &[u8]) -> [[u8; 4]; 4] {
    let c0 = u16::from_le_bytes([block[0], block[1]]);
    let c1 = u16::from_le_bytes([block[2], block[3]]);
    let expand = |c: u16| {
        let (r, g, b) = ((c >> 11) & 31, (c >> 5) & 63, c & 31);
        [
            (r * 255 / 31) as u8,
            (g * 255 / 63) as u8,
            (b * 255 / 31) as u8,
        ]
    };
    let (a, b) = (expand(c0), expand(c1));
    let mix = |x: [u8; 3], y: [u8; 3], wx: u32, wy: u32, d: u32| {
        [0, 1, 2].map(|i| ((u32::from(x[i]) * wx + u32::from(y[i]) * wy) / d) as u8)
    };
    let (c2, c3) = if c0 > c1 {
        (mix(a, b, 2, 1, 3), mix(a, b, 1, 2, 3))
    } else {
        (mix(a, b, 1, 1, 2), [0, 0, 0])
    };
    let alpha = |a: u8| if c0 <= c1 && a == 3 { 0 } else { 255 };
    [
        [a[0], a[1], a[2], 255],
        [b[0], b[1], b[2], 255],
        [c2[0], c2[1], c2[2], 255],
        [c3[0], c3[1], c3[2], alpha(3)],
    ]
}

/// The eight values of a BC4 block's first two bytes.
fn bc4_values(block: &[u8]) -> [u8; 8] {
    let (a, b) = (u32::from(block[0]), u32::from(block[1]));
    let mut v = [0u8; 8];
    v[0] = a as u8;
    v[1] = b as u8;
    if a > b {
        for i in 1..7u32 {
            v[i as usize + 1] = (((7 - i) * a + i * b) / 7) as u8;
        }
    } else {
        for i in 1..5u32 {
            v[i as usize + 1] = (((5 - i) * a + i * b) / 5) as u8;
        }
        v[6] = 0;
        v[7] = 255;
    }
    v
}

/// Writes the 4x4 block `index` (row-major blocks) through `pixel(i)`, i the texel in the block.
fn put_block(out: &mut [u8], w: usize, h: usize, index: usize, pixel: impl Fn(usize) -> [u8; 4]) {
    let blocks_x = w.div_ceil(4);
    let (bx, by) = (index % blocks_x, index / blocks_x);
    for i in 0..16 {
        let (x, y) = (bx * 4 + i % 4, by * 4 + i / 4);
        if x < w && y < h {
            let at = (y * w + x) * 4;
            out[at..at + 4].copy_from_slice(&pixel(i));
        }
    }
}

#[cfg(test)]
mod decode_tests {
    use super::*;

    fn dds(format_code: &[u8; 4], w: u32, h: u32, payload: &[u8]) -> Vec<u8> {
        let mut bytes = vec![0u8; 128];
        bytes[..4].copy_from_slice(b"DDS ");
        for (offset, value) in [(4, 124u32), (12, h), (16, w), (28, 1)] {
            bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        bytes[80..84].copy_from_slice(&4u32.to_le_bytes());
        bytes[84..88].copy_from_slice(format_code);
        bytes.extend_from_slice(payload);
        bytes
    }

    #[test]
    fn bc1_block_decodes_its_two_endpoint_colours() {
        // c0 = pure red (0xF800), c1 = pure blue (0x001F); texels 0 and 1 pick c0 and c1.
        let payload = [0x00, 0xF8, 0x1F, 0x00, 0b0000_0100, 0, 0, 0];
        let data = dds(b"DXT1", 4, 4, &payload);
        let header = Dds::parse(&data).unwrap();
        let rgba = header.decode_rgba8(&data).unwrap();
        assert_eq!(&rgba[0..4], &[255, 0, 0, 255]);
        assert_eq!(&rgba[4..8], &[0, 0, 255, 255]);
    }

    #[test]
    fn bc4_block_decodes_endpoints_and_the_interpolated_ramp() {
        // 255 / 0 endpoints (8-value ramp); texel 0 = index 0, texel 1 = index 1, texel 2 = index 2.
        let payload = [255, 0, 0b1000_1000, 0, 0, 0, 0, 0];
        let data = dds(b"ATI1", 4, 4, &payload);
        let header = Dds::parse(&data).unwrap();
        assert_eq!(header.format, DdsFormat::Bc4);
        let rgba = header.decode_rgba8(&data).unwrap();
        assert_eq!(rgba[0], 255);
        assert_eq!(rgba[4], 0);
        assert_eq!(rgba[8], 218);
    }
}
