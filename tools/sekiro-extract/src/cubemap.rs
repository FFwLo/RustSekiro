//! Cube maps: the game's GI light-map probes (`map/<id>/<id>_envmap_<v>.tpf`, BC6H cube DDS
//! named `<id>_gilm<probe>_<v>`) decoded to floats, resampled into arena space, convolved to
//! irradiance, and written back as RGBA16F cube DDS files that Bevy loads as is.
//!
//! Face layout is the DirectX / wgpu one: +X, -X, +Y, -Y, +Z, -Z, each face row-major with v
//! down; `dir` and `face_uv` are inverses of each other.

pub struct Cube {
    pub size: usize,
    /// Six faces, `size * size` linear RGB each.
    pub faces: Vec<Vec<[f32; 3]>>,
}

/// Direction of a face texel: (u, v) in 0..1, v down.
pub fn dir(face: usize, u: f32, v: f32) -> [f32; 3] {
    let (s, t) = (2.0 * u - 1.0, 2.0 * v - 1.0);
    match face {
        0 => [1.0, -t, -s],
        1 => [-1.0, -t, s],
        2 => [s, 1.0, t],
        3 => [s, -1.0, -t],
        4 => [s, -t, 1.0],
        _ => [-s, -t, -1.0],
    }
}

/// Face and (u, v) a direction falls on.
pub fn face_uv(d: [f32; 3]) -> (usize, f32, f32) {
    let [x, y, z] = d;
    let (ax, ay, az) = (x.abs(), y.abs(), z.abs());
    let (face, m, s, t) = if ax >= ay && ax >= az {
        if x > 0.0 { (0, ax, -z, -y) } else { (1, ax, z, -y) }
    } else if ay >= az {
        if y > 0.0 { (2, ay, x, z) } else { (3, ay, x, -z) }
    } else if z > 0.0 {
        (4, az, x, -y)
    } else {
        (5, az, -x, -y)
    };
    let m = m.max(1e-20);
    (face, 0.5 * (s / m + 1.0), 0.5 * (t / m + 1.0))
}

fn normalize(d: [f32; 3]) -> [f32; 3] {
    let l = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt().max(1e-20);
    [d[0] / l, d[1] / l, d[2] / l]
}

impl Cube {
    pub fn new(size: usize) -> Cube {
        Cube { size, faces: vec![vec![[0.0; 3]; size * size]; 6] }
    }

    /// Bilinear sample of a direction (clamped at face edges).
    pub fn sample(&self, d: [f32; 3]) -> [f32; 3] {
        let (f, u, v) = face_uv(d);
        let n = self.size as f32;
        let (x, y) = ((u * n - 0.5).clamp(0.0, n - 1.0), (v * n - 0.5).clamp(0.0, n - 1.0));
        let (x0, y0) = (x.floor() as usize, y.floor() as usize);
        let (x1, y1) = ((x0 + 1).min(self.size - 1), (y0 + 1).min(self.size - 1));
        let (fx, fy) = (x - x0 as f32, y - y0 as f32);
        let p = |x: usize, y: usize| self.faces[f][y * self.size + x];
        let mut out = [0.0; 3];
        for c in 0..3 {
            let a = p(x0, y0)[c] * (1.0 - fx) + p(x1, y0)[c] * fx;
            let b = p(x0, y1)[c] * (1.0 - fx) + p(x1, y1)[c] * fx;
            out[c] = a * (1.0 - fy) + b * fy;
        }
        out
    }

    /// A new cube of `size` whose texel in direction `d` holds this cube's value in `src(d)`.
    pub fn resample(&self, size: usize, src: impl Fn([f32; 3]) -> [f32; 3]) -> Cube {
        let mut out = Cube::new(size);
        for f in 0..6 {
            for y in 0..size {
                for x in 0..size {
                    let d = dir(f, (x as f32 + 0.5) / size as f32, (y as f32 + 0.5) / size as f32);
                    out.faces[f][y * size + x] = self.sample(src(normalize(d)));
                }
            }
        }
        out
    }

    /// Half-size box filter.
    pub fn downsample(&self) -> Cube {
        let n = self.size / 2;
        let mut out = Cube::new(n.max(1));
        for f in 0..6 {
            for y in 0..out.size {
                for x in 0..out.size {
                    let mut acc = [0.0; 3];
                    for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                        let p = self.faces[f][(2 * y + dy).min(self.size - 1) * self.size + (2 * x + dx).min(self.size - 1)];
                        for c in 0..3 {
                            acc[c] += p[c] * 0.25;
                        }
                    }
                    out.faces[f][y * out.size + x] = acc;
                }
            }
        }
        out
    }

    /// Lambert irradiance over the whole sphere, divided by pi (Bevy's diffuse map holds
    /// radiance). `size` texels per face; the source is box-filtered down to 32 first.
    /// `clamp` caps the source radiance first: the probes hold the sun disk (7-10 units against
    /// a sky of 0.2-0.5), which the directional light already provides; left in, it would light
    /// every upward face twice and tint the whole arena with the sun colour.
    pub fn irradiance(&self, size: usize, clamp: f32) -> Cube {
        let mut src = Cube { size: self.size, faces: self.faces.iter().map(|f| f.iter().map(|c| [c[0].min(clamp), c[1].min(clamp), c[2].min(clamp)]).collect()).collect() };
        while src.size > 32 {
            src = src.downsample();
        }
        let n = src.size;
        // Source texels with their directions and solid angles.
        let mut texels: Vec<([f32; 3], f32, [f32; 3])> = Vec::with_capacity(6 * n * n);
        for f in 0..6 {
            for y in 0..n {
                for x in 0..n {
                    let (u, v) = ((x as f32 + 0.5) / n as f32, (y as f32 + 0.5) / n as f32);
                    let (s, t) = (2.0 * u - 1.0, 2.0 * v - 1.0);
                    let omega = 4.0 / (n * n) as f32 / (1.0 + s * s + t * t).powf(1.5);
                    texels.push((normalize(dir(f, u, v)), omega, src.faces[f][y * n + x]));
                }
            }
        }
        let mut out = Cube::new(size);
        for f in 0..6 {
            for y in 0..size {
                for x in 0..size {
                    let nrm = normalize(dir(f, (x as f32 + 0.5) / size as f32, (y as f32 + 0.5) / size as f32));
                    let mut acc = [0.0; 3];
                    for (d, omega, c) in &texels {
                        let cos = nrm[0] * d[0] + nrm[1] * d[1] + nrm[2] * d[2];
                        if cos > 0.0 {
                            let w = cos * omega / std::f32::consts::PI;
                            acc[0] += c[0] * w;
                            acc[1] += c[1] * w;
                            acc[2] += c[2] * w;
                        }
                    }
                    out.faces[f][y * size + x] = acc;
                }
            }
        }
        out
    }

    /// Direction and value of the brightest texel (a sun check against the draw params).
    pub fn brightest(&self) -> ([f32; 3], [f32; 3]) {
        let mut best = (0.0f32, [0.0; 3], [0.0; 3]);
        for f in 0..6 {
            for y in 0..self.size {
                for x in 0..self.size {
                    let c = self.faces[f][y * self.size + x];
                    let l = c[0] + c[1] + c[2];
                    if l > best.0 {
                        best = (l, normalize(dir(f, (x as f32 + 0.5) / self.size as f32, (y as f32 + 0.5) / self.size as f32)), c);
                    }
                }
            }
        }
        (best.1, best.2)
    }

    /// Mean radiance over the sphere.
    pub fn mean(&self) -> [f32; 3] {
        let mut acc = [0.0f64; 3];
        let mut n = 0.0;
        for f in &self.faces {
            for c in f {
                acc[0] += c[0] as f64;
                acc[1] += c[1] as f64;
                acc[2] += c[2] as f64;
                n += 1.0;
            }
        }
        [(acc[0] / n) as f32, (acc[1] / n) as f32, (acc[2] / n) as f32]
    }
}

/// Decodes mip 0 of a BC6H cube DDS (DX10 header, DXGI 95 unsigned / 96 signed).
pub fn read_bc6h(dds: &[u8]) -> Option<Cube> {
    let u32_ = |o: usize| u32::from_le_bytes(dds[o..o + 4].try_into().unwrap());
    if dds.len() < 148 || &dds[0..4] != b"DDS " || &dds[84..88] != b"DX10" {
        return None;
    }
    let (h, w, mips) = (u32_(12) as usize, u32_(16) as usize, u32_(28).max(1) as usize);
    let dxgi = u32_(128);
    if w != h || !(dxgi == 95 || dxgi == 96) || u32_(112) & 0x200 == 0 {
        return None;
    }
    let block_bytes = |s: usize| s.div_ceil(4) * s.div_ceil(4) * 16;
    let face_bytes: usize = (0..mips).map(|m| block_bytes((w >> m).max(1))).sum();
    let mut cube = Cube::new(w);
    let bw = w.div_ceil(4);
    for f in 0..6 {
        let base = 148 + f * face_bytes;
        let mut px = vec![0.0f32; w * w * 3];
        for by in 0..bw {
            for bx in 0..bw {
                let b = base + (by * bw + bx) * 16;
                if b + 16 > dds.len() {
                    return None;
                }
                let at = (by * 4 * w + bx * 4) * 3;
                bcdec_rs::bc6h_float(&dds[b..b + 16], &mut px[at..], w * 3, dxgi == 96);
            }
        }
        for i in 0..w * w {
            cube.faces[f][i] = [px[i * 3], px[i * 3 + 1], px[i * 3 + 2]];
        }
    }
    Some(cube)
}

fn half(x: f32) -> u16 {
    let bits = x.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let exp = ((bits >> 23) & 0xff) as i32 - 127 + 15;
    let mant = bits & 0x7f_ffff;
    if x.is_nan() {
        return sign | 0x7e00;
    }
    if exp >= 31 {
        return sign | 0x7c00;
    }
    if exp <= 0 {
        if exp < -10 {
            return sign;
        }
        let m = (mant | 0x80_0000) >> (1 - exp);
        return sign | ((m + 0x1000) >> 13) as u16;
    }
    let h = sign | ((exp as u16) << 10) | (mant >> 13) as u16;
    // Round to nearest.
    if mant & 0x1000 != 0 { h + 1 } else { h }
}

/// A cube DDS (RGBA16F, DX10 header, cube caps, all mips of every face) from a mip chain.
pub fn write_rgba16f(mips: &[Cube]) -> Vec<u8> {
    let size = mips[0].size as u32;
    let mut out = Vec::new();
    let put = |out: &mut Vec<u8>, v: u32| out.extend_from_slice(&v.to_le_bytes());
    out.extend_from_slice(b"DDS ");
    put(&mut out, 124);
    put(&mut out, 0x1 | 0x2 | 0x4 | 0x1000 | 0x2_0000); // caps, height, width, pixel format, mip count
    put(&mut out, size);
    put(&mut out, size);
    put(&mut out, size * size * 8);
    put(&mut out, 1);
    put(&mut out, mips.len() as u32);
    for _ in 0..11 {
        put(&mut out, 0);
    }
    put(&mut out, 32);
    put(&mut out, 0x4); // fourcc
    out.extend_from_slice(b"DX10");
    for _ in 0..5 {
        put(&mut out, 0);
    }
    put(&mut out, 0x1000 | 0x8 | 0x40_0000); // texture, complex, mipmap
    put(&mut out, 0xfe00); // cube map, all faces
    put(&mut out, 0);
    put(&mut out, 0);
    put(&mut out, 0);
    put(&mut out, 10); // R16G16B16A16_FLOAT
    put(&mut out, 3); // 2D
    put(&mut out, 0x4); // cube
    put(&mut out, 6); // array size: the faces (Bevy's loader takes its layer count from here)
    put(&mut out, 0);
    for f in 0..6 {
        for m in mips {
            for c in &m.faces[f] {
                for v in c {
                    out.extend_from_slice(&half(*v).to_le_bytes());
                }
                out.extend_from_slice(&half(1.0).to_le_bytes());
            }
        }
    }
    out
}

/// The full mip chain of a cube down to 1 texel.
pub fn mip_chain(top: Cube) -> Vec<Cube> {
    let mut chain = vec![top];
    while chain.last().unwrap().size > 1 {
        let next = chain.last().unwrap().downsample();
        chain.push(next);
    }
    chain
}
