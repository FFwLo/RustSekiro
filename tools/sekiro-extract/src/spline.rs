//! hkaSplineCompressedAnimation decoding (Havok 2016, as in Sekiro anibnds).
//! Algorithm adapted from SoulsAssetPipeline's SplineCompressedAnimation.cs
//! (itself from the Havok Format Library): per block, a transform mask per
//! track, then for each of translation / rotation / scale either static values
//! or a B-spline (degree, knots, quantized control points).

use crate::bin::Reader;

#[derive(Clone, Copy, Default)]
pub struct Qs {
    pub t: [f32; 3],
    pub r: [f32; 4],
    pub s: [f32; 3],
}

const STATIC_X: u8 = 0x01;
const STATIC_Y: u8 = 0x02;
const STATIC_Z: u8 = 0x04;
const STATIC_W: u8 = 0x08;
const SPLINE_X: u8 = 0x10;
const SPLINE_Y: u8 = 0x20;
const SPLINE_Z: u8 = 0x40;
const SPLINE_W: u8 = 0x80;

struct Cursor<'a> {
    r: Reader<'a>,
    pos: usize,
}

impl Cursor<'_> {
    fn u8(&mut self) -> u8 {
        let v = self.r.d[self.pos];
        self.pos += 1;
        v
    }
    fn i16(&mut self) -> i16 {
        let v = self.r.u16(self.pos) as i16;
        self.pos += 2;
        v
    }
    fn u16(&mut self) -> u16 {
        let v = self.r.u16(self.pos);
        self.pos += 2;
        v
    }
    fn u32(&mut self) -> u32 {
        let v = self.r.u32(self.pos);
        self.pos += 4;
        v
    }
    fn f32(&mut self) -> f32 {
        let v = self.r.f32(self.pos);
        self.pos += 4;
        v
    }
    fn pad(&mut self, n: usize) {
        self.pos = self.pos.div_ceil(n) * n;
    }
}

fn rot_align(q: u8) -> usize {
    match q {
        0 => 4,
        1 => 1,
        2 => 2,
        3 => 1,
        4 => 2,
        _ => 4,
    }
}

fn read_quat(c: &mut Cursor, q: u8) -> [f32; 4] {
    match q {
        0 => {
            // POLAR32
            let v = c.u32();
            let r_mask = (1u32 << 10) - 1;
            let r_frac = 1.0 / r_mask as f32;
            let pi = std::f32::consts::PI;
            let (pi2, pi4) = (0.5 * pi, 0.25 * pi);
            let phi_frac = pi2 / 511.0;
            let mut r = f32::from_bits((v >> 18) & r_mask) * r_frac;
            r = 1.0 - r * r;
            let phi_theta = (v & 0x3FFFF) as f32;
            let mut phi = phi_theta.sqrt().floor();
            let mut theta = 0.0;
            if phi > 0.0 {
                theta = pi4 * (phi_theta - phi * phi) / phi;
                phi *= phi_frac;
            }
            let mag = (1.0 - r * r).max(0.0).sqrt();
            let mut out = [phi.sin() * theta.cos() * mag, phi.sin() * theta.sin() * mag, phi.cos() * mag, r];
            for (i, bit) in [0x1000_0000u32, 0x2000_0000, 0x4000_0000, 0x8000_0000].iter().enumerate() {
                if v & bit != 0 {
                    out[i] = -out[i];
                }
            }
            out
        }
        1 => {
            // THREECOMP40
            let mut b = [0u8; 8];
            for x in b.iter_mut().take(5) {
                *x = c.u8();
            }
            let v = u64::from_le_bytes(b);
            let mask = (1u64 << 12) - 1;
            let half = (mask >> 1) as i64;
            let frac = 0.000345436f32;
            let comps = [
                ((v & mask) as i64 - half) as f32 * frac,
                (((v >> 12) & mask) as i64 - half) as f32 * frac,
                (((v >> 24) & mask) as i64 - half) as f32 * frac,
            ];
            let shift = ((v >> 36) & 3) as usize;
            let neg = (v >> 38) & 1 != 0;
            assemble(comps, shift, neg)
        }
        2 => {
            // THREECOMP48
            let x = c.i16();
            let y = c.i16();
            let z = c.i16();
            let shift = ((((y >> 14) & 2) | ((x >> 15) & 1)) & 3) as usize;
            let neg = (z >> 15) != 0;
            let mask = (1i32 << 15) - 1;
            let half = mask >> 1;
            let f = 0.000043161f32;
            let comps = [
                ((x as i32 & mask) - half) as f32 * f,
                ((y as i32 & mask) - half) as f32 * f,
                ((z as i32 & mask) - half) as f32 * f,
            ];
            assemble(comps, shift, neg)
        }
        5 => [c.f32(), c.f32(), c.f32(), c.f32()],
        other => panic!("unsupported rotation quantization {other}"),
    }
}

fn assemble(comps: [f32; 3], shift: usize, neg: bool) -> [f32; 4] {
    let mut out = [0.0f32; 4];
    let mut k = 0;
    for (i, o) in out.iter_mut().enumerate() {
        if i != shift {
            *o = comps[k];
            k += 1;
        }
    }
    let w = 1.0 - comps.iter().map(|v| v * v).sum::<f32>();
    out[shift] = if w <= 0.0 { 0.0 } else { w.sqrt() } * if neg { -1.0 } else { 1.0 };
    out
}

fn quant(c: &mut Cursor, min: f32, max: f32, q: u8) -> f32 {
    let ratio = if q == 0 { c.u8() as f32 / 255.0 } else { c.u16() as f32 / 65535.0 };
    min + (max - min) * ratio
}

struct Spline<T> {
    degree: usize,
    knots: Vec<u8>,
    points: Vec<T>,
}

fn find_span(degree: usize, frame: f32, n: usize, knots: &[u8]) -> usize {
    if frame >= knots[n] as f32 {
        return n - 1;
    }
    let (mut low, mut high) = (degree, n);
    let mut mid = (low + high) / 2;
    while frame < knots[mid] as f32 || frame >= knots[mid + 1] as f32 {
        if frame < knots[mid] as f32 {
            high = mid;
        } else {
            low = mid;
        }
        mid = (low + high) / 2;
    }
    mid
}

fn basis(span: usize, degree: usize, frame: f32, knots: &[u8]) -> [f32; 5] {
    let mut n = [1.0f32, 0.0, 0.0, 0.0, 0.0];
    for i in 1..=degree {
        for j in (0..i).rev() {
            let k0 = knots[span - j] as f32;
            let k1 = knots[span + i - j] as f32;
            let a = if k1 != k0 { (frame - k0) / (k1 - k0) } else { 0.0 };
            let tmp = n[j] * a;
            n[j + 1] += n[j] - tmp;
            n[j] = tmp;
        }
    }
    n
}

impl Spline<f32> {
    fn eval(&self, frame: f32) -> f32 {
        if self.points.len() == 1 {
            return self.points[0];
        }
        let span = find_span(self.degree, frame, self.points.len(), &self.knots);
        let n = basis(span, self.degree, frame, &self.knots);
        (0..=self.degree).map(|i| self.points[span - i] * n[i]).sum()
    }
}

impl Spline<[f32; 4]> {
    fn eval(&self, frame: f32) -> [f32; 4] {
        let span = find_span(self.degree, frame, self.points.len(), &self.knots);
        let n = basis(span, self.degree, frame, &self.knots);
        let mut out = [0.0f32; 4];
        for i in 0..=self.degree {
            for (k, o) in out.iter_mut().enumerate() {
                *o += self.points[span - i][k] * n[i];
            }
        }
        let len = out.iter().map(|v| v * v).sum::<f32>().sqrt();
        if len > 0.0 { out.map(|v| v / len) } else { [0.0, 0.0, 0.0, 1.0] }
    }
}

enum Chan {
    None,
    Static(f32),
    Spline(Spline<f32>),
}

impl Chan {
    fn eval(&self, frame: f32, default: f32) -> f32 {
        match self {
            Chan::None => default,
            Chan::Static(v) => *v,
            Chan::Spline(s) => s.eval(frame),
        }
    }
}

struct Vec3Track([Chan; 3]);

fn read_vec3(c: &mut Cursor, flags: u8, q: u8) -> Vec3Track {
    let spline = flags & (SPLINE_X | SPLINE_Y | SPLINE_Z) != 0;
    if !spline {
        let mut ch = [Chan::None, Chan::None, Chan::None];
        for (i, bit) in [STATIC_X, STATIC_Y, STATIC_Z].iter().enumerate() {
            if flags & bit != 0 {
                ch[i] = Chan::Static(c.f32());
            }
        }
        return Vec3Track(ch);
    }
    let num = c.i16() as usize;
    let degree = c.u8() as usize;
    let knots: Vec<u8> = (0..num + degree + 2).map(|_| c.u8()).collect();
    c.pad(4);
    let mut bounds = [(0.0f32, 0.0f32); 3];
    let mut kind = [0u8; 3]; // 0 none, 1 static, 2 spline
    let mut statics = [0.0f32; 3];
    for i in 0..3 {
        let (sp, st) = [(SPLINE_X, STATIC_X), (SPLINE_Y, STATIC_Y), (SPLINE_Z, STATIC_Z)][i];
        if flags & sp != 0 {
            bounds[i] = (c.f32(), c.f32());
            kind[i] = 2;
        } else if flags & st != 0 {
            statics[i] = c.f32();
            kind[i] = 1;
        }
    }
    let mut pts: [Vec<f32>; 3] = [Vec::new(), Vec::new(), Vec::new()];
    for _ in 0..=num {
        for i in 0..3 {
            if kind[i] == 2 {
                pts[i].push(quant(c, bounds[i].0, bounds[i].1, q));
            }
        }
    }
    let [p0, p1, p2] = pts;
    let mk = |i: usize, p: Vec<f32>| match kind[i] {
        2 => Chan::Spline(Spline { degree, knots: knots.clone(), points: p }),
        1 => Chan::Static(statics[i]),
        _ => Chan::None,
    };
    Vec3Track([mk(0, p0), mk(1, p1), mk(2, p2)])
}

enum RotTrack {
    Identity,
    Static([f32; 4]),
    Spline(Spline<[f32; 4]>),
}

struct Track {
    t: Vec3Track,
    r: RotTrack,
    s: Vec3Track,
}

fn read_block(data: &[u8], start: usize, tracks: usize) -> Vec<Track> {
    let mut c = Cursor { r: Reader::new(data), pos: start };
    let masks: Vec<[u8; 4]> = (0..tracks).map(|_| [c.u8(), c.u8(), c.u8(), c.u8()]).collect();
    c.pad(4);
    let mut out = Vec::with_capacity(tracks);
    for m in masks {
        let (qt, qr, qs) = (m[0] & 3, (m[0] >> 2) & 0xF, (m[0] >> 6) & 3);
        let t = read_vec3(&mut c, m[1], qt);
        c.pad(4);
        let rf = m[2];
        let r = if rf & (SPLINE_X | SPLINE_Y | SPLINE_Z | SPLINE_W) != 0 {
            let num = c.i16() as usize;
            let degree = c.u8() as usize;
            let knots: Vec<u8> = (0..num + degree + 2).map(|_| c.u8()).collect();
            c.pad(rot_align(qr));
            let points = (0..=num).map(|_| read_quat(&mut c, qr)).collect();
            RotTrack::Spline(Spline { degree, knots, points })
        } else if rf & (STATIC_X | STATIC_Y | STATIC_Z | STATIC_W) != 0 {
            c.pad(rot_align(qr));
            RotTrack::Static(read_quat(&mut c, qr))
        } else {
            RotTrack::Identity
        };
        c.pad(4);
        let s = read_vec3(&mut c, m[3], qs);
        c.pad(4);
        out.push(Track { t, r, s });
    }
    out
}

/// Samples every frame of the animation: result[frame][track].
pub fn sample(data: &[u8], block_offsets: &[u32], tracks: usize, frames: usize, frames_per_block: usize) -> Vec<Vec<Qs>> {
    let blocks: Vec<Vec<Track>> = block_offsets.iter().map(|&o| read_block(data, o as usize, tracks)).collect();
    (0..frames)
        .map(|f| {
            let b = (f / frames_per_block).min(blocks.len() - 1);
            let local = (f - b * frames_per_block) as f32;
            blocks[b]
                .iter()
                .map(|tr| Qs {
                    t: [tr.t.0[0].eval(local, 0.0), tr.t.0[1].eval(local, 0.0), tr.t.0[2].eval(local, 0.0)],
                    r: match &tr.r {
                        RotTrack::Identity => [0.0, 0.0, 0.0, 1.0],
                        RotTrack::Static(q) => *q,
                        RotTrack::Spline(s) => s.eval(local),
                    },
                    s: [tr.s.0[0].eval(local, 1.0), tr.s.0[1].eval(local, 1.0), tr.s.0[2].eval(local, 1.0)],
                })
                .collect()
        })
        .collect()
}
