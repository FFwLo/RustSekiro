//! The game's UI fonts: the DefineFont3 tag inside `font/<lang>_<use>/font.gfx` (Scaleform),
//! turned into a TrueType font the engine can load.
//!
//! DefineFont3 stores quadratic glyph outlines at 20 * 1024 units per em, the same curve kind as
//! TrueType, so the outlines carry over point for point at 2048 units per em (a tenth).
//! After FwLo's RustSekiro `tools/gfx_font.py` (fontTools) and the loading-screen work for #101.
use std::collections::BTreeMap;

use crate::reader::{Error, Reader, Result, bail, slice};

/// DefineFont3 units per TrueType unit (20480 per em to 2048).
const SCALE: f64 = 10.0;

struct Bits<'a> {
    data: &'a [u8],
    at: usize,
}

impl Bits<'_> {
    fn unsigned(&mut self, n: usize) -> Result<u32> {
        if n > 32 || self.at + n > self.data.len() * 8 {
            return bail("truncated GFX bits");
        }
        let mut value = 0;
        for _ in 0..n {
            value = (value << 1) | u32::from((self.data[self.at / 8] >> (7 - self.at % 8)) & 1);
            self.at += 1;
        }
        Ok(value)
    }

    fn signed(&mut self, n: usize) -> Result<i32> {
        let value = self.unsigned(n)?;
        Ok(if n == 0 {
            0
        } else {
            ((value << (32 - n)) as i32) >> (32 - n)
        })
    }

    /// Skips a RECT record (the movie frame, a glyph's bounds).
    fn rect(&mut self) -> Result<()> {
        let n = self.unsigned(5)? as usize;
        for _ in 0..4 {
            self.signed(n)?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
struct Point {
    x: i16,
    y: i16,
    on: bool,
}

fn scaled(v: i32) -> Result<i16> {
    i16::try_from((f64::from(v) / SCALE).round() as i32)
        .map_err(|_| Error::Format(format!("glyph coordinate {v} out of range")))
}

/// The binary-search header of `n` entries of `entry_size` bytes (cmap 4, kern, the table
/// directory): searchRange, entrySelector, rangeShift.
fn search_header(n: usize, entry_size: usize) -> [u16; 3] {
    let power = 1_usize << n.ilog2();
    [
        (power * entry_size) as u16,
        n.ilog2() as u16,
        ((n - power) * entry_size) as u16,
    ]
}

/// A point in TrueType space (y up; Flash's y points down).
fn point(x: i32, y: i32, on: bool) -> Result<Point> {
    Ok(Point {
        x: scaled(x)?,
        y: scaled(-y)?,
        on,
    })
}

/// One SHAPE record's edges as closed contours.
fn contours(data: &[u8]) -> Result<Vec<Vec<Point>>> {
    let mut b = Bits { data, at: 0 };
    let fill = b.unsigned(4)? as usize;
    let line = b.unsigned(4)? as usize;
    let (mut x, mut y) = (0, 0);
    let mut paths = Vec::new();
    let mut path = Vec::new();
    loop {
        if b.unsigned(1)? == 0 {
            let flags = b.unsigned(5)?;
            if flags == 0 {
                break;
            }
            if flags & 16 != 0 {
                return bail("new styles in a font shape");
            }
            if flags & 1 != 0 {
                if !path.is_empty() {
                    paths.push(std::mem::take(&mut path));
                }
                let n = b.unsigned(5)? as usize;
                x = b.signed(n)?;
                y = b.signed(n)?;
                path.push(point(x, y, true)?);
            }
            if flags & 2 != 0 {
                b.unsigned(fill)?;
            }
            if flags & 4 != 0 {
                b.unsigned(fill)?;
            }
            if flags & 8 != 0 {
                b.unsigned(line)?;
            }
        } else {
            if path.is_empty() {
                path.push(point(x, y, true)?);
            }
            let straight = b.unsigned(1)? != 0;
            let n = b.unsigned(4)? as usize + 2;
            if straight {
                if b.unsigned(1)? != 0 {
                    x += b.signed(n)?;
                    y += b.signed(n)?;
                } else if b.unsigned(1)? != 0 {
                    y += b.signed(n)?;
                } else {
                    x += b.signed(n)?;
                }
            } else {
                x += b.signed(n)?;
                y += b.signed(n)?;
                path.push(point(x, y, false)?);
                x += b.signed(n)?;
                y += b.signed(n)?;
            }
            path.push(point(x, y, true)?);
        }
    }
    if !path.is_empty() {
        paths.push(path);
    }
    // A closed contour repeats its first point at the end; TrueType closes implicitly.
    for path in &mut paths {
        let same = |a: &Point, b: &Point| (a.x, a.y) == (b.x, b.y);
        if path.len() > 1 && same(&path[0], &path[path.len() - 1]) {
            path.pop();
        }
    }
    Ok(paths)
}

fn u16be(out: &mut Vec<u8>, value: u16) {
    out.extend(value.to_be_bytes());
}

fn i16be(out: &mut Vec<u8>, value: i16) {
    out.extend(value.to_be_bytes());
}

fn u32be(out: &mut Vec<u8>, value: u32) {
    out.extend(value.to_be_bytes());
}

fn checksum(bytes: &[u8]) -> u32 {
    bytes.chunks(4).fold(0_u32, |sum, c| {
        let mut word = [0; 4];
        word[..c.len()].copy_from_slice(c);
        sum.wrapping_add(u32::from_be_bytes(word))
    })
}

/// The font in a `font.gfx` as a TrueType file.
pub fn to_sfnt(gfx: &[u8]) -> Result<Vec<u8>> {
    if !(gfx.starts_with(b"GFX") || gfx.starts_with(b"FWS")) {
        return bail("expected an uncompressed font GFX");
    }
    let mut frame = Bits {
        data: slice(gfx, 8, gfx.len().saturating_sub(8))?,
        at: 0,
    };
    frame.rect()?;
    // After the frame rect: the frame rate and count (4 bytes), then the tags.
    let mut r = Reader::at(gfx, 8 + frame.at.div_ceil(8) + 4);
    while r.pos() < gfx.len() {
        let header = r.u16()?;
        let tag = header >> 6;
        let len = if header & 63 == 63 {
            r.u32()? as usize
        } else {
            (header & 63) as usize
        };
        let bytes = r.bytes(len)?;
        if tag == 75 {
            return font3(bytes);
        }
        if tag == 0 {
            break;
        }
    }
    bail("no DefineFont3 in the font GFX")
}

fn font3(data: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(data);
    r.u16()?; // font id
    let flags = r.u8()?;
    if flags & 0x8c != 0x8c {
        return bail("expected layout, wide offsets and wide codes in the font");
    }
    r.u8()?; // language
    let name_len = r.u8()? as usize;
    let name = String::from_utf8_lossy(r.bytes(name_len)?)
        .trim_end_matches('\0')
        .to_owned();
    let count = r.u16()? as usize;
    // One cmap format 4 segment per code must fit its 16-bit length.
    if count >= 8190 {
        return bail("font too large for cmap format 4");
    }
    let table = r.pos();
    let mut offsets = Vec::with_capacity(count + 1);
    for _ in 0..=count {
        offsets.push(r.u32()? as usize);
    }

    // Glyph 0 is .notdef, so the font's glyph i is TrueType glyph i + 1.
    let mut glyf = Vec::new();
    // .notdef is empty: it starts and ends at 0; each glyph then adds its end.
    let mut loca = vec![0; 8];
    let mut bearings = Vec::with_capacity(count);
    let (mut max_points, mut max_contours) = (0, 0);
    let mut font_box = [i16::MAX, i16::MAX, i16::MIN, i16::MIN];
    for i in 0..count {
        let len = offsets[i + 1]
            .checked_sub(offsets[i])
            .ok_or_else(|| Error::Format("font glyph offsets out of order".into()))?;
        let paths = contours(slice(data, table + offsets[i], len)?)?;
        let points: Vec<Point> = paths.iter().flatten().copied().collect();
        if points.is_empty() {
            bearings.push(0);
            u32be(&mut loca, glyf.len() as u32);
            continue;
        }
        max_points = max_points.max(points.len());
        max_contours = max_contours.max(paths.len());
        let bounds = [
            points.iter().map(|p| p.x).min().unwrap_or(0),
            points.iter().map(|p| p.y).min().unwrap_or(0),
            points.iter().map(|p| p.x).max().unwrap_or(0),
            points.iter().map(|p| p.y).max().unwrap_or(0),
        ];
        font_box = [
            font_box[0].min(bounds[0]),
            font_box[1].min(bounds[1]),
            font_box[2].max(bounds[2]),
            font_box[3].max(bounds[3]),
        ];
        bearings.push(bounds[0]);
        i16be(&mut glyf, paths.len() as i16);
        for value in bounds {
            i16be(&mut glyf, value);
        }
        let mut end = 0;
        for path in &paths {
            end += path.len();
            u16be(&mut glyf, (end - 1) as u16);
        }
        u16be(&mut glyf, 0); // no instructions
        // Flags: on-curve bit only, so every coordinate is a 16-bit delta.
        glyf.extend(points.iter().map(|p| u8::from(p.on)));
        for y_axis in [false, true] {
            let mut previous = 0_i16;
            for p in &points {
                let value = if y_axis { p.y } else { p.x };
                i16be(&mut glyf, value.wrapping_sub(previous));
                previous = value;
            }
        }
        while glyf.len() % 4 != 0 {
            glyf.push(0);
        }
        u32be(&mut loca, glyf.len() as u32);
    }
    if max_points == 0 {
        font_box = [0; 4];
    }

    r.seek(table + offsets[count]);
    let mut codes = BTreeMap::new();
    for i in 0..count {
        let code = r.u16()?;
        if code != 0 {
            codes.insert(code, (i + 1) as u16);
        }
    }
    let scale = |v: i16| scaled(i32::from(v));
    let ascent = match scale(r.i16()?)? {
        0 => 1800,
        a => a,
    };
    let descent = match scale(r.i16()?)?.abs() {
        0 => -450,
        d => -d,
    };
    let leading = scale(r.i16()?)?;
    let mut hmtx = vec![0; 4];
    let (mut max_advance, mut advance_sum) = (0_u16, 0_u64);
    for bearing in &bearings {
        let advance = scale(r.i16()?)?.max(0) as u16;
        max_advance = max_advance.max(advance);
        advance_sum += u64::from(advance);
        u16be(&mut hmtx, advance);
        i16be(&mut hmtx, *bearing);
    }
    // Each glyph's bounds (byte-aligned RECTs), then the kerning pairs.
    let mut bounds = Bits {
        data,
        at: r.pos() * 8,
    };
    for _ in 0..count {
        bounds.rect()?;
        bounds.at = bounds.at.div_ceil(8) * 8;
    }
    r.seek(bounds.at / 8);
    let pairs = r.u16()?;
    let mut kern_pairs = Vec::new();
    for _ in 0..pairs {
        let (left, right, adjust) = (r.u16()?, r.u16()?, scale(r.i16()?)?);
        if let (Some(&left), Some(&right)) = (codes.get(&left), codes.get(&right)) {
            kern_pairs.push((left, right, adjust));
        }
    }
    kern_pairs.sort_unstable();
    kern_pairs.dedup_by_key(|(l, r, _)| (*l, *r));

    let glyphs = (count + 1) as u16;
    let mut tables = BTreeMap::<[u8; 4], Vec<u8>>::new();

    let mut head = Vec::new();
    for value in [0x10000, 0x10000, 0, 0x5f0f_3cf5] {
        u32be(&mut head, value);
    }
    u16be(&mut head, 3); // baseline and left sidebearing at 0
    u16be(&mut head, 2048);
    head.extend([0; 16]); // created, modified
    for v in font_box {
        i16be(&mut head, v);
    }
    // macStyle, lowestRecPPEM, fontDirectionHint, long loca, glyphDataFormat.
    for v in [0, 8, 2, 1, 0] {
        i16be(&mut head, v);
    }
    tables.insert(*b"head", head);

    let mut hhea = Vec::new();
    u32be(&mut hhea, 0x10000);
    for v in [ascent, descent, leading] {
        i16be(&mut hhea, v);
    }
    u16be(&mut hhea, max_advance);
    for v in [font_box[0], 0, font_box[2], 1, 0, 0, 0, 0, 0, 0, 0] {
        i16be(&mut hhea, v);
    }
    u16be(&mut hhea, glyphs);
    tables.insert(*b"hhea", hhea);

    let mut maxp = Vec::new();
    u32be(&mut maxp, 0x10000);
    for v in [glyphs, max_points as u16, max_contours as u16, 0, 0, 2] {
        u16be(&mut maxp, v);
    }
    maxp.extend([0; 16]);
    tables.insert(*b"maxp", maxp);

    let first = codes.keys().next().copied().unwrap_or(0);
    let last = codes.keys().last().copied().unwrap_or(0);
    let mut os2 = Vec::new();
    u16be(&mut os2, 4);
    i16be(&mut os2, (advance_sum / count.max(1) as u64) as i16);
    for v in [400, 5, 0] {
        u16be(&mut os2, v); // weight, width, fsType
    }
    // Sub- and superscript sizes and offsets, strikeout size and place, family class.
    for v in [1331, 1229, 0, 154, 1331, 1229, 0, 717, 102, 530, 0] {
        i16be(&mut os2, v);
    }
    os2.extend([0; 10]); // panose
    os2.extend([0; 16]); // Unicode ranges
    os2.extend(*b"FSFT");
    for v in [0x40, first, last] {
        u16be(&mut os2, v); // fsSelection regular, first and last code
    }
    for v in [ascent, descent, leading] {
        i16be(&mut os2, v);
    }
    u16be(&mut os2, ascent.max(0) as u16);
    u16be(&mut os2, descent.unsigned_abs());
    u32be(&mut os2, 1); // Latin 1
    u32be(&mut os2, 0);
    for v in [0, 0, 0, 32, 2] {
        u16be(&mut os2, v); // x height, cap height, default, break char, max context
    }
    tables.insert(*b"OS/2", os2);

    tables.insert(*b"glyf", glyf);
    tables.insert(*b"loca", loca);
    tables.insert(*b"hmtx", hmtx);

    // cmap format 4, one segment per code plus the closing 0xFFFF.
    let segs = codes.len() + 1;
    let mut sub = Vec::new();
    for v in [4, (16 + segs * 8) as u16, 0, (segs * 2) as u16]
        .into_iter()
        .chain(search_header(segs, 2))
    {
        u16be(&mut sub, v);
    }
    for code in codes.keys() {
        u16be(&mut sub, *code);
    }
    u16be(&mut sub, 0xffff);
    u16be(&mut sub, 0); // reserved
    for code in codes.keys() {
        u16be(&mut sub, *code);
    }
    u16be(&mut sub, 0xffff);
    for (code, id) in &codes {
        u16be(&mut sub, id.wrapping_sub(*code));
    }
    u16be(&mut sub, 1);
    sub.extend(vec![0; segs * 2]); // idRangeOffset
    let mut cmap = Vec::new();
    for v in [0, 1, 3, 1] {
        u16be(&mut cmap, v);
    }
    u32be(&mut cmap, 12);
    cmap.extend(sub);
    tables.insert(*b"cmap", cmap);

    // Family, style, full name and PostScript name, Windows Unicode English.
    let strings = [
        (1, name.clone()),
        (2, "Regular".to_owned()),
        (4, name.clone()),
        (6, name.replace(' ', "")),
    ];
    let encoded: Vec<Vec<u8>> = strings
        .iter()
        .map(|(_, s)| s.encode_utf16().flat_map(u16::to_be_bytes).collect())
        .collect();
    let mut names = Vec::new();
    for v in [0, strings.len() as u16, (6 + 12 * strings.len()) as u16] {
        u16be(&mut names, v);
    }
    let mut at = 0;
    for ((id, _), bytes) in strings.iter().zip(&encoded) {
        for v in [3, 1, 0x409, *id, bytes.len() as u16, at as u16] {
            u16be(&mut names, v);
        }
        at += bytes.len();
    }
    names.extend(encoded.concat());
    tables.insert(*b"name", names);

    let mut post = vec![0; 32];
    post[..4].copy_from_slice(&0x30000_u32.to_be_bytes());
    tables.insert(*b"post", post);

    if !kern_pairs.is_empty() {
        let n = kern_pairs.len();
        let mut kern = Vec::new();
        for v in [0, 1, 0, (14 + n * 6) as u16, 1, n as u16]
            .into_iter()
            .chain(search_header(n, 6))
        {
            u16be(&mut kern, v);
        }
        for (a, b, v) in kern_pairs {
            u16be(&mut kern, a);
            u16be(&mut kern, b);
            i16be(&mut kern, v);
        }
        tables.insert(*b"kern", kern);
    }

    let n = tables.len();
    let mut out = Vec::new();
    u32be(&mut out, 0x10000);
    for v in [n as u16].into_iter().chain(search_header(n, 16)) {
        u16be(&mut out, v);
    }
    let mut offset = 12 + n * 16;
    let mut head_offset = 0;
    for (tag, table) in &tables {
        out.extend(tag);
        u32be(&mut out, checksum(table));
        u32be(&mut out, offset as u32);
        u32be(&mut out, table.len() as u32);
        if tag == b"head" {
            head_offset = offset;
        }
        offset += table.len().div_ceil(4) * 4;
    }
    for table in tables.values() {
        out.extend(table);
        while out.len() % 4 != 0 {
            out.push(0);
        }
    }
    let adjustment = 0xb1b0_afba_u32.wrapping_sub(checksum(&out));
    out[head_offset + 8..head_offset + 12].copy_from_slice(&adjustment.to_be_bytes());
    Ok(out)
}
