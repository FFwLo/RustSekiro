//! Decoder for `hkaSplineCompressedAnimation` blocks.
//!
//! Each block stores, per transform track, a 4-byte mask (quantization, position, rotation, scale)
//! followed by the track data: constant values or a B-spline (u16 last control point index, u8
//! degree, u8 knots in local frame units, then quantized control points). Splines are evaluated
//! component-wise, then the rotation is normalized.
//!
//! Layout and quantization from the public readers HavokLib (PredatorCZ), MVDX2 (Meowmaritus) and
//! the Funny-Bones hkanim reader, cross-checked against the earlier Python decoder in the
//! sekiro-deflection project. Original implementation.

use crate::Result;
use crate::reader::bail;

/// Position and scale quantization (2 bits).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VecQuant {
    Bits8,
    Bits16,
}

/// Rotation quantization (4 bits).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RotQuant {
    Polar32,
    ThreeComp40,
    ThreeComp48,
    ThreeComp24,
    Straight16,
    Uncompressed,
}

impl RotQuant {
    fn from_bits(b: u8) -> Result<Self> {
        Ok(match b {
            0 => Self::Polar32,
            1 => Self::ThreeComp40,
            2 => Self::ThreeComp48,
            3 => Self::ThreeComp24,
            4 => Self::Straight16,
            5 => Self::Uncompressed,
            _ => return bail(format!("unknown rotation quantization {b}")),
        })
    }
}

/// Counters for unusual cases met while decoding, reported by the extractor.
#[derive(Clone, Copy, Debug, Default)]
pub struct DecodeStats {
    /// Vector axes flagged both static and dynamic (dynamic wins, as in MVDX2 and hkanim).
    pub vector_axis_overlaps: u32,
    /// Rotation masks with both static and dynamic nibbles (dynamic wins, as in all readers).
    pub rotation_mask_overlaps: u32,
    pub max_degree: u8,
    /// Non-zero bytes left after the last track of a block.
    pub nonzero_tail_bytes: u32,
}

struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        match self.data.get(self.pos..self.pos + n) {
            Some(s) => {
                self.pos += n;
                Ok(s)
            }
            None => bail(format!(
                "spline block read of {n} bytes at {:#x} exceeds {:#x}",
                self.pos,
                self.data.len()
            )),
        }
    }
    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn f32(&mut self) -> Result<f32> {
        let v = f32::from_le_bytes(self.take(4)?.try_into().unwrap());
        if !v.is_finite() {
            return bail("non-finite float in spline block");
        }
        Ok(v)
    }
    fn align(&mut self, n: usize) {
        self.pos = self.pos.div_ceil(n) * n;
    }
}

/// A B-spline over control points with u8 knots, or a single constant.
#[derive(Clone, Debug)]
struct Spline<const N: usize> {
    degree: usize,
    knots: Vec<f64>,
    points: Vec<[f64; N]>,
}

impl<const N: usize> Spline<N> {
    fn constant(v: [f64; N]) -> Self {
        Self {
            degree: 0,
            knots: Vec::new(),
            points: vec![v],
        }
    }

    fn validate(&self) -> Result<()> {
        let n = self.points.len();
        if n > 1 {
            if self.knots.len() != n + self.degree + 1 {
                return bail("spline knot count does not match control points");
            }
            if self.knots.windows(2).any(|w| w[1] < w[0]) {
                return bail("spline knots decrease");
            }
            if n <= self.degree {
                return bail("spline has fewer control points than its degree");
            }
        }
        Ok(())
    }

    /// Evaluates at local frame `x` with de Boor's algorithm.
    fn eval(&self, x: f64) -> [f64; N] {
        let n = self.points.len();
        if n == 1 || self.knots.is_empty() {
            return self.points[0];
        }
        let k = &self.knots;
        let p = self.degree;
        let last = n - 1;
        let x = x.clamp(k[p], k[last + 1]);
        let span = if x >= k[last + 1] {
            last
        } else {
            (p..=last)
                .find(|&i| k[i] <= x && x < k[i + 1])
                .unwrap_or(last)
        };
        let mut d: Vec<[f64; N]> = (0..=p).map(|j| self.points[span - p + j]).collect();
        for r in 1..=p {
            for j in (r..=p).rev() {
                let i = span - p + j;
                let den = k[i + p - r + 1] - k[i];
                let a = if den != 0.0 { (x - k[i]) / den } else { 0.0 };
                let prev = d[j - 1];
                for (v, pv) in d[j].iter_mut().zip(prev) {
                    *v = (1.0 - a) * pv + a * *v;
                }
            }
        }
        d[p]
    }
}

#[derive(Clone, Debug)]
struct TrackCurves {
    translation: Spline<3>,
    rotation: Spline<4>,
    scale: Spline<3>,
}

/// One decoded transform track, sampled at every authored frame.
#[derive(Clone, Debug, Default)]
pub struct SampledTrack {
    /// One entry if constant, otherwise one per frame.
    pub translation: Vec<[f32; 3]>,
    pub rotation: Vec<[f32; 4]>,
    pub scale: Vec<[f32; 3]>,
}

fn spline_header(c: &mut Cursor<'_>) -> Result<(usize, usize, Vec<f64>)> {
    let last = c.u16()? as usize;
    let degree = c.u8()? as usize;
    if degree > 7 || last < degree {
        return bail(format!(
            "unsupported spline degree {degree} with {last} points"
        ));
    }
    let knots = c
        .take(last + degree + 2)?
        .iter()
        .map(|&k| k as f64)
        .collect();
    Ok((last + 1, degree, knots))
}

fn vector_curve(
    c: &mut Cursor<'_>,
    mask: u8,
    quant: VecQuant,
    default: f64,
    stats: &mut DecodeStats,
) -> Result<Spline<3>> {
    for axis in 0..3 {
        if mask & (1 << axis) != 0 && mask & (0x10 << axis) != 0 {
            stats.vector_axis_overlaps += 1;
        }
    }
    let mut values = [default; 3];
    let curve = if mask & 0x70 != 0 {
        let (count, degree, knots) = spline_header(c)?;
        stats.max_degree = stats.max_degree.max(degree as u8);
        c.align(4);
        let mut ranges = [None; 3];
        for axis in 0..3 {
            if mask & (0x10 << axis) != 0 {
                let lo = c.f32()? as f64;
                let hi = c.f32()? as f64;
                ranges[axis] = Some((lo, hi));
            } else if mask & (1 << axis) != 0 {
                values[axis] = c.f32()? as f64;
            }
        }
        let mut points = Vec::with_capacity(count);
        for _ in 0..count {
            let mut p = values;
            for axis in 0..3 {
                if let Some((lo, hi)) = ranges[axis] {
                    let t = match quant {
                        VecQuant::Bits8 => c.u8()? as f64 / 255.0,
                        VecQuant::Bits16 => c.u16()? as f64 / 65535.0,
                    };
                    p[axis] = lo + (hi - lo) * t;
                }
            }
            points.push(p);
        }
        let s = Spline {
            degree,
            knots,
            points,
        };
        s.validate()?;
        s
    } else {
        for (axis, v) in values.iter_mut().enumerate() {
            if mask & (1 << axis) != 0 {
                *v = c.f32()? as f64;
            }
        }
        Spline::constant(values)
    };
    c.align(4);
    Ok(curve)
}

fn quat_polar32(c: &mut Cursor<'_>) -> Result<[f64; 4]> {
    let v = c.u32()?;
    let r_frac = 1.0 / 1023.0;
    let mut r = ((v >> 18) & 0x3FF) as f64 * r_frac;
    r = 1.0 - r * r;
    let phi_theta = (v & 0x3FFFF) as f64;
    let mut phi = phi_theta.sqrt().floor();
    let mut theta = 0.0;
    if phi > 0.0 {
        theta = std::f64::consts::FRAC_PI_4 * (phi_theta - phi * phi) / phi;
        phi *= std::f64::consts::FRAC_PI_2 / 511.0;
    }
    let mag = (1.0 - r * r).max(0.0).sqrt();
    let mut q = [
        phi.sin() * theta.cos() * mag,
        phi.sin() * theta.sin() * mag,
        phi.cos() * mag,
        r,
    ];
    for (i, comp) in q.iter_mut().enumerate() {
        if v & (0x1000_0000 << i) != 0 {
            *comp = -*comp;
        }
    }
    Ok(q)
}

fn quat_40(c: &mut Cursor<'_>) -> Result<[f64; 4]> {
    let b = c.take(5)?;
    let packed = u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], 0, 0, 0]);
    let mut v = [0.0f64; 3];
    for (i, comp) in v.iter_mut().enumerate() {
        *comp = (((packed >> (12 * i)) & 0xFFF) as f64 - 2047.0) * 0.000345436;
    }
    let missing = (1.0 - v.iter().map(|x| x * x).sum::<f64>()).max(0.0).sqrt()
        * if packed & (1 << 38) != 0 { -1.0 } else { 1.0 };
    Ok(insert_missing(v, ((packed >> 36) & 3) as usize, missing))
}

fn quat_48(c: &mut Cursor<'_>) -> Result<[f64; 4]> {
    let x = c.u16()?;
    let y = c.u16()?;
    let z = c.u16()?;
    let slot = (((y >> 14) & 2) | ((x >> 15) & 1)) as usize;
    let negative = (z >> 15) != 0;
    let mut v = [0.0f64; 3];
    for (comp, raw) in v.iter_mut().zip([x, y, z]) {
        *comp = ((raw & 0x7FFF) as f64 - 16383.0) * 0.000043161;
    }
    let missing = (1.0 - v.iter().map(|x| x * x).sum::<f64>()).max(0.0).sqrt()
        * if negative { -1.0 } else { 1.0 };
    Ok(insert_missing(v, slot, missing))
}

fn insert_missing(v: [f64; 3], slot: usize, missing: f64) -> [f64; 4] {
    let mut out = [0.0; 4];
    let mut it = v.into_iter();
    for (i, o) in out.iter_mut().enumerate() {
        *o = if i == slot {
            missing
        } else {
            it.next().unwrap()
        };
    }
    out
}

fn read_quat(c: &mut Cursor<'_>, q: RotQuant) -> Result<[f64; 4]> {
    match q {
        RotQuant::Polar32 => quat_polar32(c),
        RotQuant::ThreeComp40 => quat_40(c),
        RotQuant::ThreeComp48 => quat_48(c),
        RotQuant::Uncompressed => Ok([
            c.f32()? as f64,
            c.f32()? as f64,
            c.f32()? as f64,
            c.f32()? as f64,
        ]),
        other => bail(format!("rotation quantization {other:?} is not supported")),
    }
}

fn rotation_curve(
    c: &mut Cursor<'_>,
    mask: u8,
    quant: RotQuant,
    stats: &mut DecodeStats,
) -> Result<Spline<4>> {
    if mask & 0xF0 != 0 && mask & 0x0F != 0 {
        stats.rotation_mask_overlaps += 1;
    }
    let curve = if mask & 0xF0 != 0 {
        let (count, degree, knots) = spline_header(c)?;
        stats.max_degree = stats.max_degree.max(degree as u8);
        match quant {
            RotQuant::ThreeComp48 | RotQuant::Straight16 => c.align(2),
            RotQuant::Polar32 | RotQuant::Uncompressed => c.align(4),
            _ => {}
        }
        let points = (0..count)
            .map(|_| read_quat(c, quant))
            .collect::<Result<Vec<_>>>()?;
        let s = Spline {
            degree,
            knots,
            points,
        };
        s.validate()?;
        s
    } else if mask & 0x0F != 0 {
        Spline::constant(read_quat(c, quant)?)
    } else {
        Spline::constant([0.0, 0.0, 0.0, 1.0])
    };
    c.align(4);
    Ok(curve)
}

fn vec_quant(bits: u8) -> Result<VecQuant> {
    match bits {
        0 => Ok(VecQuant::Bits8),
        1 => Ok(VecQuant::Bits16),
        _ => bail(format!("unknown vector quantization {bits}")),
    }
}

/// Parses one block's transform tracks.
fn parse_block(
    block: &[u8],
    tracks: usize,
    float_tracks: usize,
    stats: &mut DecodeStats,
) -> Result<Vec<TrackCurves>> {
    let mut c = Cursor {
        data: block,
        pos: 0,
    };
    let masks: Vec<[u8; 4]> = (0..tracks)
        .map(|_| Ok(c.take(4)?.try_into().unwrap()))
        .collect::<Result<_>>()?;
    c.take(float_tracks)?;
    c.align(4);
    let mut out = Vec::with_capacity(tracks);
    for [quant, pos, rot, scale] in masks {
        let translation = vector_curve(&mut c, pos, vec_quant(quant & 3)?, 0.0, stats)?;
        let rotation =
            rotation_curve(&mut c, rot, RotQuant::from_bits((quant >> 2) & 0xF)?, stats)?;
        let scale = vector_curve(&mut c, scale, vec_quant((quant >> 6) & 3)?, 1.0, stats)?;
        out.push(TrackCurves {
            translation,
            rotation,
            scale,
        });
    }
    if float_tracks == 0 {
        stats.nonzero_tail_bytes += block[c.pos.min(block.len())..]
            .iter()
            .filter(|&&b| b != 0)
            .count() as u32;
    }
    Ok(out)
}

/// The fields of an `hkaSplineCompressedAnimation` needed to decode it.
pub struct SplineAnimation<'a> {
    pub num_frames: usize,
    pub num_blocks: usize,
    pub max_frames_per_block: usize,
    pub transform_tracks: usize,
    pub float_tracks: usize,
    pub block_offsets: Vec<usize>,
    pub data: &'a [u8],
}

fn normalize(q: [f64; 4]) -> Result<[f32; 4]> {
    let n = q.iter().map(|x| x * x).sum::<f64>().sqrt();
    if !n.is_finite() || n < 1e-8 {
        return bail("degenerate sampled quaternion");
    }
    Ok([
        (q[0] / n) as f32,
        (q[1] / n) as f32,
        (q[2] / n) as f32,
        (q[3] / n) as f32,
    ])
}

fn to_f32<const N: usize>(v: [f64; N]) -> [f32; N] {
    v.map(|x| x as f32)
}

/// Decodes every transform track, sampled at each authored frame.
pub fn decode(anim: &SplineAnimation<'_>, stats: &mut DecodeStats) -> Result<Vec<SampledTrack>> {
    if anim.num_blocks == 0 || anim.block_offsets.len() < anim.num_blocks {
        return bail("spline animation has no blocks");
    }
    if anim.max_frames_per_block < 2 && anim.num_blocks > 1 {
        return bail("spline animation has too few frames per block");
    }
    let mut blocks = Vec::with_capacity(anim.num_blocks);
    for b in 0..anim.num_blocks {
        let start = anim.block_offsets[b];
        let end = anim
            .block_offsets
            .get(b + 1)
            .copied()
            .unwrap_or(anim.data.len())
            .min(anim.data.len());
        if start > end {
            return bail("spline block offsets out of order");
        }
        blocks.push(parse_block(
            &anim.data[start..end],
            anim.transform_tracks,
            anim.float_tracks,
            stats,
        )?);
    }
    let per_block = anim.max_frames_per_block.saturating_sub(1).max(1);
    let frames = anim.num_frames.max(1);
    // A track channel is constant if it is constant in every block.
    let mut out = Vec::with_capacity(anim.transform_tracks);
    for t in 0..anim.transform_tracks {
        let mut track = SampledTrack::default();
        let all_const = |f: &dyn Fn(&TrackCurves) -> bool| blocks.iter().all(|b| f(&b[t]));
        let t_const = all_const(&|c| c.translation.points.len() == 1)
            && blocks
                .iter()
                .all(|b| b[t].translation.points[0] == blocks[0][t].translation.points[0]);
        let r_const = all_const(&|c| c.rotation.points.len() == 1)
            && blocks
                .iter()
                .all(|b| b[t].rotation.points[0] == blocks[0][t].rotation.points[0]);
        let s_const = all_const(&|c| c.scale.points.len() == 1)
            && blocks
                .iter()
                .all(|b| b[t].scale.points[0] == blocks[0][t].scale.points[0]);
        let n = |c: bool| if c { 1 } else { frames };
        for f in 0..n(t_const) {
            let (b, local) = locate(f, per_block, anim.num_blocks);
            track
                .translation
                .push(to_f32(blocks[b][t].translation.eval(local)));
        }
        for f in 0..n(r_const) {
            let (b, local) = locate(f, per_block, anim.num_blocks);
            track
                .rotation
                .push(normalize(blocks[b][t].rotation.eval(local))?);
        }
        for f in 0..n(s_const) {
            let (b, local) = locate(f, per_block, anim.num_blocks);
            track.scale.push(to_f32(blocks[b][t].scale.eval(local)));
        }
        out.push(track);
    }
    Ok(out)
}

/// Global frame to (block, local frame).
fn locate(frame: usize, per_block: usize, blocks: usize) -> (usize, f64) {
    let b = (frame / per_block).min(blocks - 1);
    (b, (frame - b * per_block) as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_curves() {
        let linear = Spline::<1> {
            degree: 1,
            knots: vec![0.0, 0.0, 4.0, 4.0],
            points: vec![[0.0], [4.0]],
        };
        assert_eq!(linear.eval(1.0), [1.0]);
        let cubic = Spline::<1> {
            degree: 3,
            knots: vec![0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0],
            points: vec![[0.0], [0.0], [0.0], [1.0]],
        };
        assert!((cubic.eval(0.5)[0] - 0.125).abs() < 1e-12);
        assert_eq!(cubic.eval(-2.0), [0.0]);
        assert_eq!(cubic.eval(2.0), [1.0]);
    }

    #[test]
    fn quat40_identity_slots_and_sign() {
        let base: u64 = 2047 | (2047 << 12) | (2047 << 24);
        for slot in 0..4u64 {
            for sign in 0..2u64 {
                let packed = base | (slot << 36) | (sign << 38) | (1 << 39);
                let bytes = packed.to_le_bytes();
                let mut c = Cursor {
                    data: &bytes[..5],
                    pos: 0,
                };
                let q = quat_40(&mut c).unwrap();
                let mut want = [0.0; 4];
                want[slot as usize] = if sign == 1 { -1.0 } else { 1.0 };
                assert_eq!(q, want);
            }
        }
    }

    #[test]
    fn static_track_block() {
        // Position static xyz, static identity quaternion (40-bit), default scale.
        let base: u64 = 2047 | (2047 << 12) | (2047 << 24) | (3 << 36);
        let mut blob = vec![0x45, 7, 1, 0];
        for v in [1.0f32, 2.0, 3.0] {
            blob.extend_from_slice(&v.to_le_bytes());
        }
        blob.extend_from_slice(&base.to_le_bytes()[..5]);
        blob.extend_from_slice(&[0; 3]);
        let mut stats = DecodeStats::default();
        let anim = SplineAnimation {
            num_frames: 3,
            num_blocks: 1,
            max_frames_per_block: 256,
            transform_tracks: 1,
            float_tracks: 0,
            block_offsets: vec![0],
            data: &blob,
        };
        let tracks = decode(&anim, &mut stats).unwrap();
        assert_eq!(tracks[0].translation, vec![[1.0, 2.0, 3.0]]);
        assert_eq!(tracks[0].rotation, vec![[0.0, 0.0, 0.0, 1.0]]);
        assert_eq!(tracks[0].scale, vec![[1.0, 1.0, 1.0]]);
    }

    #[test]
    fn truncated_block_fails() {
        let mut stats = DecodeStats::default();
        let anim = SplineAnimation {
            num_frames: 1,
            num_blocks: 1,
            max_frames_per_block: 256,
            transform_tracks: 1,
            float_tracks: 0,
            block_offsets: vec![0],
            data: &[0x45, 1, 0, 0],
        };
        assert!(decode(&anim, &mut stats).is_err());
    }

    #[test]
    fn block_location() {
        assert_eq!(locate(0, 255, 2), (0, 0.0));
        assert_eq!(locate(255, 255, 2), (1, 0.0));
        assert_eq!(locate(300, 255, 2), (1, 45.0));
        // The last frame of the last block stays in it.
        assert_eq!(locate(510, 255, 2), (1, 255.0));
    }
}
