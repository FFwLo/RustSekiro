//! Scaleform GFX movies (menu/*.gfx): the game's HUD and menu layouts.
//!
//! A .gfx is an SWF ("GFX" plain, "CFX" zlib after the 8-byte header) with Scaleform's own tags
//! (1000+). The layout lives in the timelines: every DefineSprite's PlaceObject2/3 records place a
//! character at a depth with a matrix (twips, 1/20 px) and a colour transform, and FrameLabel
//! marks the states and fades ("FadeIn", "State_1", ...). Shapes carry their bounds and the
//! bitmap they fill with (DefineExternalImage2 names the atlas sprite, e.g. MENU_HP_bar).
//!
//! `sekiro-extract gfx <in.gfx> <out.json>` writes the whole tree; `hud-layout <extracted>` reads
//! menu/01_000_fe.gfx and the menu movies and writes extracted/hud/layout.json: every named
//! instance's place on the 1920 x 1080 stage (world matrix and bounds at frame 1), each sprite's
//! labels (frame numbers at the movie's frame rate), for src/hud.rs and src/game_menu.rs.

use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;

struct Bits<'a> {
    d: &'a [u8],
    pos: usize,
    bit: u32,
}

impl<'a> Bits<'a> {
    fn new(d: &'a [u8], pos: usize) -> Self {
        Bits { d, pos, bit: 0 }
    }
    fn ub(&mut self, n: u32) -> u32 {
        let mut v = 0u32;
        for _ in 0..n {
            let byte = *self.d.get(self.pos).unwrap_or(&0);
            let b = (byte >> (7 - self.bit)) & 1;
            v = (v << 1) | b as u32;
            self.bit += 1;
            if self.bit == 8 {
                self.bit = 0;
                self.pos += 1;
            }
        }
        v
    }
    fn sb(&mut self, n: u32) -> i32 {
        if n == 0 {
            return 0;
        }
        let v = self.ub(n);
        if v & (1 << (n - 1)) != 0 { (v as i64 - (1i64 << n)) as i32 } else { v as i32 }
    }
    fn align(&mut self) -> usize {
        if self.bit != 0 {
            self.bit = 0;
            self.pos += 1;
        }
        self.pos
    }
}

struct Rd<'a> {
    d: &'a [u8],
    p: usize,
}

impl<'a> Rd<'a> {
    fn u8(&mut self) -> u8 {
        let v = *self.d.get(self.p).unwrap_or(&0);
        self.p += 1;
        v
    }
    fn u16(&mut self) -> u16 {
        let v = u16::from_le_bytes([self.u8(), self.u8()]);
        v
    }
    fn i16(&mut self) -> i16 {
        self.u16() as i16
    }
    fn u32(&mut self) -> u32 {
        u32::from_le_bytes([self.u8(), self.u8(), self.u8(), self.u8()])
    }
    fn cstr(&mut self) -> String {
        let s = self.p;
        while self.p < self.d.len() && self.d[self.p] != 0 {
            self.p += 1;
        }
        let out = String::from_utf8_lossy(&self.d[s..self.p]).into_owned();
        self.p += 1;
        out
    }
    fn pstr(&mut self) -> String {
        let n = self.u8() as usize;
        let s = self.p;
        self.p = (self.p + n).min(self.d.len());
        String::from_utf8_lossy(&self.d[s..self.p]).into_owned()
    }
    fn rect(&mut self) -> [f64; 4] {
        let mut b = Bits::new(self.d, self.p);
        let n = b.ub(5);
        let r = [b.sb(n), b.sb(n), b.sb(n), b.sb(n)];
        self.p = b.align();
        // xmin, xmax, ymin, ymax in twips -> [x0, y0, x1, y1] px
        [r[0] as f64 / 20.0, r[2] as f64 / 20.0, r[1] as f64 / 20.0, r[3] as f64 / 20.0]
    }
    /// MATRIX -> [a, b, c, d, tx, ty] (x' = a x + c y + tx, y' = b x + d y + ty; px).
    fn matrix(&mut self) -> [f64; 6] {
        let mut b = Bits::new(self.d, self.p);
        let (mut a, mut d) = (1.0, 1.0);
        let (mut b0, mut c0) = (0.0, 0.0);
        if b.ub(1) == 1 {
            let n = b.ub(5);
            a = b.sb(n) as f64 / 65536.0;
            d = b.sb(n) as f64 / 65536.0;
        }
        if b.ub(1) == 1 {
            let n = b.ub(5);
            b0 = b.sb(n) as f64 / 65536.0;
            c0 = b.sb(n) as f64 / 65536.0;
        }
        let n = b.ub(5);
        let tx = b.sb(n) as f64 / 20.0;
        let ty = b.sb(n) as f64 / 20.0;
        self.p = b.align();
        [a, b0, c0, d, tx, ty]
    }
    /// CXFORM(WITHALPHA) -> (mult rgba, add rgba); mult 1.0 = 256.
    fn cxform(&mut self, alpha: bool) -> ([f64; 4], [f64; 4]) {
        let mut b = Bits::new(self.d, self.p);
        let has_add = b.ub(1) == 1;
        let has_mul = b.ub(1) == 1;
        let n = b.ub(4);
        let k = if alpha { 4 } else { 3 };
        let mut m = [1.0; 4];
        let mut a = [0.0; 4];
        if has_mul {
            for v in m.iter_mut().take(k) {
                *v = b.sb(n) as f64 / 256.0;
            }
        }
        if has_add {
            for v in a.iter_mut().take(k) {
                *v = b.sb(n) as f64;
            }
        }
        self.p = b.align();
        (m, a)
    }
    fn rgba(&mut self, alpha: bool) -> [u8; 4] {
        [self.u8(), self.u8(), self.u8(), if alpha { self.u8() } else { 255 }]
    }
}

fn round(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

fn mat_json(m: &[f64; 6]) -> Value {
    json!(m.iter().map(|v| round(*v)).collect::<Vec<_>>())
}

/// One placement record of a timeline.
#[derive(Clone, Debug)]
pub struct Place {
    pub frame: u32,
    pub depth: u16,
    /// None: move / modify what is at the depth.
    pub ch: Option<u16>,
    pub matrix: Option<[f64; 6]>,
    pub cx: Option<([f64; 4], [f64; 4])>,
    pub name: Option<String>,
    pub clip_depth: Option<u16>,
    pub class: Option<String>,
    pub remove: bool,
    pub visible: Option<bool>,
    /// PlaceObject's Move flag: modify what is at the depth.
    pub mv: bool,
}

#[derive(Default, Debug)]
pub struct Sprite {
    pub frames: u32,
    pub labels: Vec<(String, u32)>,
    pub places: Vec<Place>,
}

#[derive(Default, Debug)]
pub struct Movie {
    pub version: u8,
    pub stage: [f64; 4],
    pub fps: f64,
    pub frames: u32,
    pub root: Sprite,
    pub sprites: BTreeMap<u16, Sprite>,
    /// Shape id -> (bounds px, bitmap ids, solid colours).
    pub shapes: BTreeMap<u16, ([f64; 4], Vec<(u16, [f64; 6])>, Vec<[u8; 4]>)>,
    /// Image id -> (export name, file name, target w, h).
    pub images: BTreeMap<u32, (String, String, u16, u16)>,
    pub texts: BTreeMap<u16, Value>,
    pub exports: BTreeMap<u16, String>,
    pub classes: BTreeMap<u16, String>,
    pub fonts: BTreeMap<u16, String>,
}

pub fn parse(raw: &[u8]) -> Result<Movie, String> {
    if raw.len() < 8 {
        return Err("short".into());
    }
    let sig = &raw[0..3];
    let body: Vec<u8> = match sig {
        b"GFX" | b"FWS" => raw.to_vec(),
        b"CFX" | b"CWS" => {
            use std::io::Read;
            let mut out = raw[..8].to_vec();
            flate2::read::ZlibDecoder::new(&raw[8..]).read_to_end(&mut out).map_err(|e| e.to_string())?;
            out
        }
        _ => return Err(format!("not a gfx: {:?}", sig)),
    };
    let mut r = Rd { d: &body, p: 8 };
    let mut m = Movie { version: body[3], ..Default::default() };
    m.stage = r.rect();
    let fr = r.u16();
    m.fps = (fr >> 8) as f64 + (fr & 0xff) as f64 / 256.0;
    m.frames = r.u16() as u32;
    let end = body.len();
    let root = tags(&body, r.p, end, &mut m);
    m.root = root;
    Ok(m)
}

fn tags(d: &[u8], mut p: usize, end: usize, m: &mut Movie) -> Sprite {
    let mut sp = Sprite::default();
    let mut frame = 0u32;
    while p + 2 <= end {
        let mut r = Rd { d, p };
        let cl = r.u16();
        let code = cl >> 6;
        let mut len = (cl & 0x3f) as usize;
        if len == 0x3f {
            len = r.u32() as usize;
        }
        let s = r.p;
        let e = (s + len).min(end);
        p = e;
        let mut t = Rd { d: &d[..e], p: s };
        match code {
            0 => break,
            1 => frame += 1,
            43 => sp.labels.push((t.cstr(), frame)),
            39 => {
                let id = t.u16();
                let frames = t.u16() as u32;
                let mut inner = tags(d, t.p, e, m);
                inner.frames = frames;
                m.sprites.insert(id, inner);
            }
            26 | 70 => {
                let f1 = t.u8();
                let f2 = if code == 70 { t.u8() } else { 0 };
                let depth = t.u16();
                let class = if code == 70 && f2 & 0x08 != 0 { Some(t.cstr()) } else { None };
                let ch = (f1 & 0x02 != 0).then(|| t.u16());
                let matrix = (f1 & 0x04 != 0).then(|| t.matrix());
                let cx = (f1 & 0x08 != 0).then(|| t.cxform(true));
                if f1 & 0x10 != 0 {
                    t.u16();
                }
                let name = (f1 & 0x20 != 0).then(|| t.cstr());
                let clip_depth = (f1 & 0x40 != 0).then(|| t.u16());
                // Filters / blend / cache are skipped; the visible flag comes after them, so it
                // is read only when no filter list precedes it.
                let mut visible = None;
                if code == 70 && f2 & 0x01 == 0 && f1 & 0x80 == 0 {
                    if f2 & 0x02 != 0 {
                        t.u8();
                    }
                    if f2 & 0x04 != 0 {
                        t.u8();
                    }
                    if f2 & 0x20 != 0 {
                        visible = Some(t.u8() != 0);
                    }
                }
                sp.places.push(Place { frame, depth, ch, matrix, cx, name, clip_depth, class, remove: false, visible, mv: f1 & 0x01 != 0 });
            }
            4 => {
                let ch = t.u16();
                let depth = t.u16();
                let matrix = Some(t.matrix());
                sp.places.push(Place { frame, depth, ch: Some(ch), matrix, cx: None, name: None, clip_depth: None, class: None, remove: false, visible: None, mv: false });
            }
            5 | 28 => {
                if code == 5 {
                    t.u16();
                }
                let depth = t.u16();
                sp.places.push(Place { frame, depth, ch: None, matrix: None, cx: None, name: None, clip_depth: None, class: None, remove: true, visible: None, mv: false });
            }
            2 | 22 | 32 | 83 => {
                let id = t.u16();
                let b = t.rect();
                if code == 83 {
                    t.rect();
                    t.u8();
                }
                let (bm, cols) = fill_styles(&mut t, code);
                m.shapes.insert(id, (b, bm, cols));
            }
            37 => {
                let id = t.u16();
                let b = t.rect();
                let f1 = t.u8();
                let f2 = t.u8();
                let font = (f1 & 0x01 != 0).then(|| t.u16());
                let font_class = (f2 & 0x80 != 0).then(|| t.cstr());
                let height = (f1 & 0x01 != 0 || f2 & 0x80 != 0).then(|| t.u16() as f64 / 20.0);
                let color = (f1 & 0x04 != 0).then(|| t.rgba(true));
                let maxlen = (f1 & 0x02 != 0).then(|| t.u16());
                let layout = (f2 & 0x20 != 0).then(|| {
                    let align = t.u8();
                    let lm = t.u16() as f64 / 20.0;
                    let rm = t.u16() as f64 / 20.0;
                    let ind = t.u16() as f64 / 20.0;
                    let lead = t.i16() as f64 / 20.0;
                    let al = ["left", "right", "center", "justify"].get(align as usize).copied().unwrap_or("?");
                    json!({"align": al, "left_margin": lm, "right_margin": rm, "indent": ind, "leading": lead})
                });
                let var = t.cstr();
                let text = (f1 & 0x80 != 0).then(|| t.cstr());
                m.texts.insert(id, json!({"bounds": b.map(round), "font": font, "font_class": font_class, "height": height, "color": color, "max_len": maxlen, "layout": layout, "var": var, "text": text, "html": f2 & 0x02 != 0, "auto_size": f2 & 0x40 != 0}));
            }
            48 | 75 => {
                let id = t.u16();
                t.u8();
                t.u8();
                let name = t.pstr();
                m.fonts.insert(id, name.trim_end_matches('\0').to_string());
            }
            56 | 76 => {
                let n = t.u16();
                for _ in 0..n {
                    let id = t.u16();
                    let name = t.cstr();
                    if code == 56 { m.exports.insert(id, name) } else { m.classes.insert(id, name) };
                }
            }
            1001 | 1009 => {
                let id = t.u32();
                let _fmt = t.u16();
                let w = t.u16();
                let h = t.u16();
                let (name, file) = if code == 1009 { (t.pstr(), t.pstr()) } else { (String::new(), t.pstr()) };
                m.images.insert(id, (name, file, w, h));
            }
            _ => {}
        }
    }
    sp
}

/// The fill styles' bitmaps (id, matrix) and solid colours.
fn fill_styles(t: &mut Rd, code: u16) -> (Vec<(u16, [f64; 6])>, Vec<[u8; 4]>) {
    let alpha = code == 32 || code == 83;
    let mut n = t.u8() as usize;
    if n == 0xff && code != 2 {
        n = t.u16() as usize;
    }
    let (mut bm, mut cols) = (vec![], vec![]);
    for _ in 0..n.min(64) {
        let ty = t.u8();
        match ty {
            0x00 => cols.push(t.rgba(alpha)),
            0x10 | 0x12 | 0x13 => {
                t.matrix();
                let g = t.u8();
                for _ in 0..(g & 0x0f) {
                    t.u8();
                    cols.push(t.rgba(alpha));
                }
                if ty == 0x13 {
                    t.u16();
                }
            }
            0x40..=0x43 => {
                let id = t.u16();
                let mx = t.matrix();
                bm.push((id, mx));
            }
            _ => break,
        }
    }
    (bm, cols)
}

fn mul(p: &[f64; 6], c: &[f64; 6]) -> [f64; 6] {
    // world = p * c (apply c first, then p)
    [
        p[0] * c[0] + p[2] * c[1],
        p[1] * c[0] + p[3] * c[1],
        p[0] * c[2] + p[2] * c[3],
        p[1] * c[2] + p[3] * c[3],
        p[0] * c[4] + p[2] * c[5] + p[4],
        p[1] * c[4] + p[3] * c[5] + p[5],
    ]
}

const ID: [f64; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

fn apply(m: &[f64; 6], x: f64, y: f64) -> (f64, f64) {
    (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])
}

fn bbox(m: &[f64; 6], b: &[f64; 4]) -> [f64; 4] {
    let pts = [apply(m, b[0], b[1]), apply(m, b[2], b[1]), apply(m, b[0], b[3]), apply(m, b[2], b[3])];
    let mut o = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
    for (x, y) in pts {
        o[0] = o[0].min(x);
        o[1] = o[1].min(y);
        o[2] = o[2].max(x);
        o[3] = o[3].max(y);
    }
    o
}

fn union(a: Option<[f64; 4]>, b: [f64; 4]) -> Option<[f64; 4]> {
    Some(match a {
        None => b,
        Some(a) => [a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])],
    })
}

/// One display-list entry of a sprite at a frame.
#[derive(Clone, Debug)]
pub struct Entry {
    pub depth: u16,
    pub ch: u16,
    pub matrix: [f64; 6],
    pub cx: Option<([f64; 4], [f64; 4])>,
    pub name: Option<String>,
    pub clip_depth: Option<u16>,
}

impl Sprite {
    /// The display list after `frame` (0-based) has been shown.
    pub fn at(&self, frame: u32) -> Vec<Entry> {
        let mut list: BTreeMap<u16, Entry> = BTreeMap::new();
        for p in self.places.iter().filter(|p| p.frame <= frame) {
            if p.remove {
                list.remove(&p.depth);
                continue;
            }
            let fresh = !p.mv || !list.contains_key(&p.depth);
            if fresh {
                if let Some(ch) = p.ch {
                    list.insert(p.depth, Entry { depth: p.depth, ch, matrix: p.matrix.unwrap_or(ID), cx: p.cx, name: p.name.clone(), clip_depth: p.clip_depth });
                }
                continue;
            }
            let e = list.get_mut(&p.depth).unwrap();
            if let Some(mx) = p.matrix {
                e.matrix = mx;
            }
            if p.cx.is_some() {
                e.cx = p.cx;
            }
            if let Some(n) = &p.name {
                e.name = Some(n.clone());
            }
            if let Some(ch) = p.ch {
                e.ch = ch;
            }
        }
        list.into_values().collect()
    }
}

impl Movie {
    /// A character's own bounds at frame 0 (px, its own space).
    pub fn bounds(&self, ch: u16, depth_left: u32) -> Option<[f64; 4]> {
        if let Some((b, _, _)) = self.shapes.get(&ch) {
            return Some(*b);
        }
        if let Some(t) = self.texts.get(&ch) {
            let b = &t["bounds"];
            return Some([b[0].as_f64()?, b[1].as_f64()?, b[2].as_f64()?, b[3].as_f64()?]);
        }
        // A bitmap placed as itself (Scaleform DefineExternalImage2): its target size from 0, 0.
        if let Some((_, _, w, h)) = self.images.get(&(ch as u32)) {
            return Some([0.0, 0.0, *w as f64, *h as f64]);
        }
        if depth_left == 0 {
            return None;
        }
        let sp = self.sprites.get(&ch)?;
        let mut o = None;
        for e in sp.at(0) {
            if e.clip_depth.is_some() {
                continue;
            }
            if let Some(b) = self.bounds(e.ch, depth_left - 1) {
                o = union(o, bbox(&e.matrix, &b));
            }
        }
        o
    }
    fn kind(&self, ch: u16) -> &'static str {
        if self.shapes.contains_key(&ch) {
            "shape"
        } else if self.texts.contains_key(&ch) {
            "text"
        } else if self.sprites.contains_key(&ch) {
            "sprite"
        } else if self.images.contains_key(&(ch as u32)) {
            "image"
        } else {
            "?"
        }
    }
    /// The atlas sprite names a shape fills with.
    fn images_of(&self, ch: u16) -> Vec<String> {
        if let Some(i) = self.images.get(&(ch as u32)) {
            return vec![i.0.clone()];
        }
        self.shapes.get(&ch).map(|(_, bm, _)| bm.iter().filter_map(|(id, _)| self.images.get(&(*id as u32)).map(|i| if i.0.is_empty() { i.1.clone() } else { i.0.clone() })).collect()).unwrap_or_default()
    }
    fn class_of(&self, ch: u16) -> Option<&String> {
        self.classes.get(&ch).or_else(|| self.exports.get(&ch))
    }
    /// The whole tree as JSON (sprites with labels and placement records, shapes, images, texts).
    pub fn to_json(&self) -> Value {
        let sprite = |s: &Sprite| {
            json!({
                "frames": s.frames,
                "labels": s.labels.iter().map(|(n, f)| json!([n, f])).collect::<Vec<_>>(),
                "places": s.places.iter().map(|p| {
                    let mut o = serde_json::Map::new();
                    o.insert("f".into(), json!(p.frame));
                    o.insert("depth".into(), json!(p.depth));
                    if p.remove { o.insert("remove".into(), json!(true)); }
                    if let Some(c) = p.ch { o.insert("ch".into(), json!(c)); }
                    if let Some(mx) = &p.matrix { o.insert("m".into(), mat_json(mx)); }
                    if let Some((mu, ad)) = &p.cx { o.insert("cx".into(), json!({"mul": mu.map(round), "add": ad})); }
                    if let Some(n) = &p.name { o.insert("name".into(), json!(n)); }
                    if let Some(c) = &p.clip_depth { o.insert("clip".into(), json!(c)); }
                    if let Some(c) = &p.class { o.insert("class".into(), json!(c)); }
                    if let Some(v) = p.visible { o.insert("visible".into(), json!(v)); }
                    Value::Object(o)
                }).collect::<Vec<_>>(),
            })
        };
        json!({
            "version": self.version,
            "stage": self.stage,
            "fps": self.fps,
            "frames": self.frames,
            "root": sprite(&self.root),
            "sprites": self.sprites.iter().map(|(id, s)| (id.to_string(), { let mut v = sprite(s); if let Some(c) = self.class_of(*id) { v["class"] = json!(c); } v })).collect::<serde_json::Map<_, _>>(),
            "shapes": self.shapes.iter().map(|(id, (b, bm, c))| (id.to_string(), json!({"bounds": b.map(round), "bitmaps": bm.iter().map(|(i, mx)| json!({"id": i, "image": self.images.get(&(*i as u32)).map(|im| if im.0.is_empty() { im.1.clone() } else { im.0.clone() }), "m": mat_json(mx)})).collect::<Vec<_>>(), "colors": c}))).collect::<serde_json::Map<_, _>>(),
            "images": self.images.iter().map(|(id, (n, f, w, h))| (id.to_string(), json!({"name": n, "file": f, "w": w, "h": h}))).collect::<serde_json::Map<_, _>>(),
            "texts": self.texts.iter().map(|(id, t)| (id.to_string(), t.clone())).collect::<serde_json::Map<_, _>>(),
            "fonts": self.fonts,
            "exports": self.exports,
            "classes": self.classes,
        })
    }

    /// Every placed instance at frame 0, depth first: its path (instance names joined by '/',
    /// unnamed ones as "#<depth>"), world matrix on the stage and world bounds. `max_depth` stops
    /// the walk; sprites' label lists go in `sprites`.
    pub fn layout(&self) -> Value {
        let mut out = serde_json::Map::new();
        self.walk(&self.root, "", &ID, None, 0, &mut out);
        let sprites: serde_json::Map<String, Value> = self
            .sprites
            .iter()
            .filter(|(_, s)| !s.labels.is_empty())
            .map(|(id, s)| {
                let key = self.class_of(*id).cloned().unwrap_or_else(|| format!("sprite_{id}"));
                (key, json!({"id": id, "frames": s.frames, "labels": s.labels.iter().map(|(n, f)| (n.clone(), json!(f))).collect::<serde_json::Map<_, _>>()}))
            })
            .collect();
        json!({"stage": self.stage, "fps": self.fps, "instances": out, "sprites": sprites})
    }

    fn walk(&self, sp: &Sprite, path: &str, world: &[f64; 6], cx: Option<([f64; 4], [f64; 4])>, level: u32, out: &mut serde_json::Map<String, Value>) {
        if level > 12 {
            return;
        }
        for e in sp.at(0) {
            let seg = e.name.clone().unwrap_or_else(|| format!("#{}", e.depth));
            let p = if path.is_empty() { seg } else { format!("{path}/{seg}") };
            let w = mul(world, &e.matrix);
            // Colour transforms compose: mult multiplies, add goes through the outer mult.
            let c = match (cx, e.cx) {
                (None, x) | (x, None) => x,
                (Some((pm, pa)), Some((m, a))) => Some((std::array::from_fn(|i| pm[i] * m[i]), std::array::from_fn(|i| pm[i] * a[i] + pa[i]))),
            };
            let b = self.bounds(e.ch, 8).map(|b| bbox(&w, &b));
            let mut o = serde_json::Map::new();
            o.insert("ch".into(), json!(e.ch));
            o.insert("kind".into(), json!(self.kind(e.ch)));
            if let Some(cl) = self.class_of(e.ch) {
                o.insert("class".into(), json!(cl));
            }
            o.insert("m".into(), mat_json(&w));
            o.insert("local".into(), mat_json(&e.matrix));
            if let Some(b) = b {
                o.insert("bounds".into(), json!(b.map(round)));
            }
            if let Some((m, a)) = c {
                o.insert("cx".into(), json!({"mul": m.map(round), "add": a}));
            }
            let imgs = self.images_of(e.ch);
            if !imgs.is_empty() {
                o.insert("images".into(), json!(imgs));
            }
            if let Some(t) = self.texts.get(&e.ch) {
                o.insert("text".into(), t.clone());
            }
            if e.clip_depth.is_some() {
                o.insert("mask".into(), json!(true));
            }
            out.insert(p.clone(), Value::Object(o));
            if let Some(child) = self.sprites.get(&e.ch) {
                self.walk(child, &p, &w, c, level + 1, out);
            }
        }
    }
}

/// `gfx <in.gfx> <out.json>`: the movie's whole tree.
pub fn dump(input: &Path, out: &Path) {
    let raw = std::fs::read(input).unwrap_or_else(|e| panic!("{}: {e}", input.display()));
    let m = parse(&raw).unwrap_or_else(|e| panic!("{}: {e}", input.display()));
    std::fs::write(out, serde_json::to_string_pretty(&m.to_json()).unwrap()).unwrap();
    eprintln!("{}: {} sprites, {} shapes, {} images, {} texts", input.display(), m.sprites.len(), m.shapes.len(), m.images.len(), m.texts.len());
}

/// The movies the HUD and menus are laid out from (menu/<name>.gfx).
pub const MOVIES: &[&str] = &["01_000_fe", "02_000_ingametop", "02_200_mainmenu", "02_202_equipweaponselect", "03_206_skill", "05_230_optionsetting", "01_002_fe_saveicon"];

/// `hud-layout <extracted>`: menu/<movie>.gfx (extracted/menu, else the unpacked cache in
/// SEKIRO_CACHE/raw/menu) -> extracted/hud/layout/<movie>.json (instances at frame 0 with world
/// places, sprites' labels).
pub fn export_layouts(root: &Path) {
    let out_dir = root.join("hud/layout");
    let _ = std::fs::create_dir_all(&out_dir);
    let cache = std::env::var("SEKIRO_CACHE").ok().map(|c| Path::new(&c).join("raw/menu"));
    for name in MOVIES {
        // The archive's names differ in case (01_000_FE.gfx): matched without it.
        let find = |dir: &Path| std::fs::read_dir(dir).ok().into_iter().flatten().flatten().map(|e| e.path()).find(|p| p.extension().is_some_and(|x| x == "gfx") && p.file_stem().is_some_and(|s| s.to_string_lossy().eq_ignore_ascii_case(name)));
        let Some(path) = find(&root.join("menu")).or_else(|| cache.as_deref().and_then(find)) else {
            eprintln!("menu/{name}.gfx not found (unpack menu/ or set SEKIRO_CACHE): skipped");
            continue;
        };
        let raw = std::fs::read(&path).unwrap();
        match parse(&raw) {
            Ok(m) => {
                let l = m.layout();
                let n = l["instances"].as_object().map_or(0, |o| o.len());
                std::fs::write(out_dir.join(format!("{name}.json")), serde_json::to_string_pretty(&l).unwrap()).unwrap();
                eprintln!("{}: {n} instances -> hud/layout/{name}.json", path.display());
            }
            Err(e) => eprintln!("{}: {e}", path.display()),
        }
    }
}
