//! Havok 2016 tagfile (`TAG0`) and type compendium (`TCM0`) reader.
//!
//! A tagfile is a tree of 4-byte-headed sections. The `DATA` section holds every object, the
//! `TYPE` section (or a `TCRF` reference into a separate compendium) describes the reflected
//! types, and `INDX/ITEM` lists the items: each object, array and string in `DATA` with its type
//! and element count. Pointers, arrays and strings inside objects are stored as 32-bit item
//! indices.
//!
//! Format knowledge from the public readers in Grimrukh/soulstruct-havok and PredatorCZ/HavokLib,
//! reimplemented here. Values are read lazily through [`Value`] views, so callers walk objects by
//! type and field name without a schema compiled into the reader.

use std::collections::HashMap;
use std::sync::Arc;

use crate::reader::{Reader, bail, slice};
use crate::{Error, Result};

/// `tagFormatFlags` bits in the `TBDY` type body.
mod format_flags {
    pub const SUB_TYPE: u32 = 1;
    pub const POINTER: u32 = 2;
    pub const VERSION: u32 = 4;
    pub const BYTE_SIZE: u32 = 8;
    pub const ABSTRACT_VALUE: u32 = 16;
    pub const MEMBERS: u32 = 32;
    pub const INTERFACES: u32 = 64;
    pub const UNKNOWN: u32 = 128;
}

/// The data kind in the low nibble of a type's `tagTypeFlags`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Void,
    Opaque,
    Bool,
    String,
    Int,
    Float,
    Pointer,
    Class,
    Array,
    /// A fixed-length inline array `T[N]`.
    Tuple(u32),
}

#[derive(Clone, Debug)]
pub struct Member {
    pub name: String,
    pub flags: u32,
    pub offset: u32,
    pub ty: u32,
}

#[derive(Clone, Debug, Default)]
pub struct TypeInfo {
    pub name: String,
    /// Template arguments: (name, value). Names starting with `t` hold type indices.
    pub templates: Vec<(String, u32)>,
    pub parent: u32,
    pub format_flags: u32,
    pub type_flags: Option<u32>,
    pub pointer: Option<u32>,
    pub version: Option<u32>,
    pub size: Option<(u32, u32)>,
    pub members: Vec<Member>,
    pub interfaces: Vec<(u32, u32)>,
}

/// Every type declared by one `TYPE` section. Index 0 is the null type.
#[derive(Debug, Default)]
pub struct TypeTable {
    pub types: Vec<TypeInfo>,
    by_name: HashMap<String, u32>,
}

impl TypeTable {
    pub fn get(&self, index: u32) -> Option<&TypeInfo> {
        self.types.get(index as usize).filter(|_| index != 0)
    }

    pub fn by_name(&self, name: &str) -> Option<u32> {
        self.by_name.get(name).copied()
    }

    pub fn name(&self, index: u32) -> &str {
        self.get(index).map(|t| t.name.as_str()).unwrap_or("<null>")
    }

    /// Walks the parent chain from `index` (inclusive).
    pub fn ancestry(&self, index: u32) -> impl Iterator<Item = &TypeInfo> {
        let mut cur = index;
        let mut guard = 0;
        std::iter::from_fn(move || {
            guard += 1;
            if guard > 64 {
                return None;
            }
            let t = self.get(cur)?;
            cur = t.parent;
            Some(t)
        })
    }

    /// True if `index` is `name` or derives from it.
    pub fn is_a(&self, index: u32, name: &str) -> bool {
        self.ancestry(index).any(|t| t.name == name)
    }

    pub fn type_flags(&self, index: u32) -> u32 {
        self.ancestry(index).find_map(|t| t.type_flags).unwrap_or(0)
    }

    pub fn kind(&self, index: u32) -> Kind {
        let flags = self.type_flags(index);
        match flags & 0xF {
            0 => Kind::Void,
            2 => Kind::Bool,
            3 => Kind::String,
            4 => Kind::Int,
            5 => Kind::Float,
            6 => Kind::Pointer,
            7 => Kind::Class,
            8 if flags & 0x20 != 0 => Kind::Tuple(flags >> 8),
            8 => Kind::Array,
            _ => Kind::Opaque,
        }
    }

    pub fn size(&self, index: u32) -> u32 {
        self.ancestry(index)
            .find_map(|t| t.size)
            .map(|s| s.0)
            .unwrap_or(0)
    }

    /// The pointed-to / element type of a pointer, array or tuple type.
    pub fn pointee(&self, index: u32) -> Option<u32> {
        self.ancestry(index)
            .find_map(|t| t.pointer)
            .filter(|&p| p != 0)
    }

    /// Finds a member by name on `index` or any ancestor.
    pub fn member(&self, index: u32, name: &str) -> Option<&Member> {
        self.ancestry(index)
            .flat_map(|t| t.members.iter())
            .find(|m| m.name == name)
    }

    /// All members, base class first.
    pub fn all_members(&self, index: u32) -> Vec<&Member> {
        let chain: Vec<_> = self.ancestry(index).collect();
        chain.iter().rev().flat_map(|t| t.members.iter()).collect()
    }

    fn parse(r: &mut Reader<'_>, end: usize) -> Result<Self> {
        let mut type_names: Vec<String> = Vec::new();
        let mut field_names: Vec<String> = Vec::new();
        let mut types: Vec<TypeInfo> = Vec::new();
        while r.pos() < end {
            let (tag, body) = section(r)?;
            let mut s = Reader::at(r.data(), body.start);
            match &tag {
                b"TPTR" | b"THSH" | b"TPAD" => {}
                b"TSTR" => type_names = split_strings(&r.data()[body.clone()]),
                b"FSTR" => field_names = split_strings(&r.data()[body.clone()]),
                b"TNAM" | b"TNA1" => {
                    let count = varint(&mut s)? as usize;
                    if count > 100_000 {
                        return bail(format!("implausible type count {count}"));
                    }
                    types = vec![TypeInfo::default(); count.max(1)];
                    for t in types.iter_mut().skip(1) {
                        t.name = lookup(&type_names, varint(&mut s)?)?;
                        let n = varint(&mut s)?;
                        for _ in 0..n {
                            let name = lookup(&type_names, varint(&mut s)?)?;
                            t.templates.push((name, varint(&mut s)? as u32));
                        }
                    }
                }
                b"TBOD" | b"TBDY" => {
                    while s.pos() < body.end {
                        let idx = varint(&mut s)? as usize;
                        if idx == 0 {
                            continue;
                        }
                        let n = types.len();
                        let check = |i: u64| -> Result<u32> {
                            if (i as usize) < n {
                                Ok(i as u32)
                            } else {
                                bail(format!("type index {i} out of range {n}"))
                            }
                        };
                        let parent = check(varint(&mut s)?)?;
                        let fmt = varint(&mut s)? as u32;
                        let mut t =
                            std::mem::take(types.get_mut(idx).ok_or_else(|| {
                                Error::Format(format!("type body {idx} unnamed"))
                            })?);
                        t.parent = parent;
                        t.format_flags = fmt;
                        if fmt & format_flags::SUB_TYPE != 0 {
                            t.type_flags = Some(varint(&mut s)? as u32);
                        }
                        if fmt & format_flags::POINTER != 0 && t.type_flags.unwrap_or(0) & 0xF >= 6
                        {
                            t.pointer = Some(check(varint(&mut s)?)?);
                        }
                        if fmt & format_flags::VERSION != 0 {
                            t.version = Some(varint(&mut s)? as u32);
                        }
                        if fmt & format_flags::BYTE_SIZE != 0 {
                            let size = varint(&mut s)? as u32;
                            let align = varint(&mut s)? as u32;
                            t.size = Some((size, align));
                        }
                        if fmt & format_flags::ABSTRACT_VALUE != 0 {
                            varint(&mut s)?;
                        }
                        if fmt & format_flags::MEMBERS != 0 {
                            let count = varint(&mut s)?;
                            for _ in 0..count {
                                let name = lookup(&field_names, varint(&mut s)?)?;
                                let flags = varint(&mut s)? as u32;
                                let offset = varint(&mut s)? as u32;
                                let ty = check(varint(&mut s)?)?;
                                t.members.push(Member {
                                    name,
                                    flags,
                                    offset,
                                    ty,
                                });
                            }
                        }
                        if fmt & format_flags::INTERFACES != 0 {
                            let count = varint(&mut s)?;
                            for _ in 0..count {
                                let ty = check(varint(&mut s)?)?;
                                let flags = varint(&mut s)? as u32;
                                t.interfaces.push((ty, flags));
                            }
                        }
                        if fmt & format_flags::UNKNOWN != 0 {
                            return bail(format!("type {} has unknown format flag 0x80", t.name));
                        }
                        types[idx] = t;
                    }
                }
                other => {
                    return bail(format!(
                        "unexpected section {:?} in TYPE",
                        String::from_utf8_lossy(other)
                    ));
                }
            }
            r.seek(body.end);
        }
        if types.is_empty() {
            return bail("TYPE section declared no types");
        }
        let by_name = types
            .iter()
            .enumerate()
            .skip(1)
            .map(|(i, t)| (t.name.clone(), i as u32))
            .collect();
        Ok(Self { types, by_name })
    }
}

fn lookup(names: &[String], index: u64) -> Result<String> {
    names
        .get(index as usize)
        .cloned()
        .ok_or_else(|| Error::Format(format!("string index {index} out of range {}", names.len())))
}

fn split_strings(bytes: &[u8]) -> Vec<String> {
    bytes
        .split(|&b| b == 0)
        .map(|s| String::from_utf8_lossy(s).into_owned())
        .collect()
}

/// Havok's variable-length big-endian unsigned integer. The leading bits of the first byte pick
/// the width.
pub fn varint(r: &mut Reader<'_>) -> Result<u64> {
    let b0 = r.u8()? as u64;
    let more = |r: &mut Reader<'_>, n: usize| -> Result<u64> {
        let mut v = b0;
        for _ in 0..n {
            v = (v << 8) | r.u8()? as u64;
        }
        Ok(v)
    };
    Ok(if b0 & 0x80 == 0 {
        b0
    } else if b0 & 0xC0 == 0x80 {
        more(r, 1)? & 0x3FFF
    } else if b0 & 0xE0 == 0xC0 {
        more(r, 2)? & 0x1F_FFFF
    } else if b0 & 0xF8 == 0xE0 {
        more(r, 3)? & 0x7FF_FFFF
    } else if b0 & 0xF8 == 0xE8 {
        more(r, 4)? & 0x7_FFFF_FFFF
    } else if b0 & 0xF8 == 0xF0 {
        more(r, 7)? & 0x07FF_FFFF_FFFF_FFFF
    } else {
        return bail(format!("bad varint marker {b0:#04x}"));
    })
}

/// Reads one section header and returns its tag and body range; leaves the reader at the body.
fn section(r: &mut Reader<'_>) -> Result<([u8; 4], std::ops::Range<usize>)> {
    let start = r.pos();
    let size = (r.u32_be()? & 0x3FFF_FFFF) as usize;
    let tag: [u8; 4] = r.bytes(4)?.try_into().unwrap();
    if size < 8 || start + size > r.data().len() {
        return bail(format!(
            "section {:?} at {start:#x} has bad size {size:#x}",
            String::from_utf8_lossy(&tag)
        ));
    }
    Ok((tag, start + 8..start + size))
}

/// A `TCM0` type compendium: shared type definitions that many `TCRF` tagfiles reference by ID.
#[derive(Debug)]
pub struct Compendium {
    pub ids: Vec<[u8; 8]>,
    pub types: Arc<TypeTable>,
}

impl Compendium {
    pub fn parse(data: &[u8]) -> Result<Self> {
        let mut r = Reader::new(data);
        let (tag, root) = section(&mut r)?;
        if &tag != b"TCM0" {
            return bail("not a Havok compendium (TCM0)");
        }
        let mut ids = Vec::new();
        let mut types = None;
        while r.pos() < root.end {
            let (tag, body) = section(&mut r)?;
            match &tag {
                b"TCID" => {
                    let (chunks, _) = data[body.clone()].as_chunks::<8>();
                    ids.extend_from_slice(chunks);
                }
                b"TYPE" => types = Some(TypeTable::parse(&mut r, body.end)?),
                _ => {}
            }
            r.seek(body.end);
        }
        Ok(Self {
            ids,
            types: Arc::new(types.ok_or_else(|| Error::Format("compendium has no TYPE".into()))?),
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Item {
    pub ty: u32,
    /// Offset into the DATA section.
    pub offset: u32,
    pub count: u32,
    /// Set for items that are pointed-to objects rather than array/string contents.
    pub is_object: bool,
}

/// A parsed `TAG0` file.
pub struct TagFile {
    pub sdk_version: String,
    pub types: Arc<TypeTable>,
    pub data: Vec<u8>,
    pub items: Vec<Item>,
}

impl std::fmt::Debug for TagFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TagFile")
            .field("sdk_version", &self.sdk_version)
            .field("types", &self.types.types.len())
            .field("items", &self.items.len())
            .finish()
    }
}

/// True if `data` starts like a tagfile or compendium.
pub fn is_tagfile(data: &[u8]) -> bool {
    data.len() >= 8 && (&data[4..8] == b"TAG0" || &data[4..8] == b"TCM0")
}

/// The compendium ID a `TCRF` tagfile references, if it uses one.
pub fn compendium_ref(data: &[u8]) -> Result<Option<[u8; 8]>> {
    let mut r = Reader::new(data);
    let (tag, root) = section(&mut r)?;
    if &tag != b"TAG0" {
        return bail("not a Havok tagfile (TAG0)");
    }
    while r.pos() < root.end {
        let (tag, body) = section(&mut r)?;
        if &tag == b"TCRF" {
            return Ok(Some(slice(data, body.start, 8)?.try_into().unwrap()));
        }
        r.seek(body.end);
    }
    Ok(None)
}

impl TagFile {
    /// Parses a tagfile. Files that reference a compendium (`TCRF`) need it passed in.
    pub fn parse(data: &[u8], compendium: Option<&Compendium>) -> Result<Self> {
        let mut r = Reader::new(data);
        let (tag, root) = section(&mut r)?;
        if &tag != b"TAG0" {
            return bail("not a Havok tagfile (TAG0)");
        }
        let mut sdk_version = String::new();
        let mut data_range = None;
        let mut types = None;
        let mut item_range = None;
        while r.pos() < root.end {
            let (tag, body) = section(&mut r)?;
            match &tag {
                b"SDKV" => sdk_version = String::from_utf8_lossy(&data[body.clone()]).into(),
                b"DATA" => data_range = Some(body.clone()),
                b"TYPE" => types = Some(Arc::new(TypeTable::parse(&mut r, body.end)?)),
                b"TCRF" => {
                    let id: [u8; 8] = slice(data, body.start, 8)?.try_into().unwrap();
                    let comp = compendium.ok_or_else(|| {
                        Error::Format(
                            "tagfile references a compendium (TCRF) but none given".into(),
                        )
                    })?;
                    if !comp.ids.contains(&id) {
                        return bail(format!(
                            "compendium does not contain ID {:02x?} referenced by tagfile",
                            id
                        ));
                    }
                    types = Some(comp.types.clone());
                }
                b"INDX" => {
                    let mut s = Reader::at(data, body.start);
                    while s.pos() < body.end {
                        let (tag, sub) = section(&mut s)?;
                        if &tag == b"ITEM" {
                            item_range = Some(sub.clone());
                        }
                        s.seek(sub.end);
                    }
                }
                _ => {}
            }
            r.seek(body.end);
        }
        let types = types.ok_or_else(|| Error::Format("tagfile has no TYPE or TCRF".into()))?;
        let data_range = data_range.ok_or_else(|| Error::Format("tagfile has no DATA".into()))?;
        let item_range = item_range.ok_or_else(|| Error::Format("tagfile has no ITEM".into()))?;
        let body = data[data_range].to_vec();
        let mut items = Vec::new();
        let mut s = Reader::at(data, item_range.start);
        while s.pos() + 12 <= item_range.end {
            let info = s.u32()?;
            let offset = s.u32()?;
            let count = s.u32()?;
            let ty = info & 0x00FF_FFFF;
            if ty as usize >= types.types.len() {
                return bail(format!("item type {ty} out of range"));
            }
            let item = Item {
                ty,
                offset,
                count,
                is_object: info & 0x1000_0000 != 0,
            };
            let elem = types.size(ty).max(1) as usize;
            if info != 0 && offset as usize + elem * count as usize > body.len() {
                return bail(format!("item at {offset:#x} x{count} exceeds DATA"));
            }
            items.push(item);
        }
        Ok(Self {
            sdk_version,
            types,
            data: body,
            items,
        })
    }

    /// The type name of an object item.
    pub fn type_name(&self, ty: u32) -> &str {
        self.types.name(ty)
    }

    /// All object items whose type is `name` or a subclass of it.
    pub fn objects_of(&self, name: &str) -> Vec<Value<'_>> {
        self.items
            .iter()
            .filter(|it| it.is_object && self.types.is_a(it.ty, name))
            .map(|it| Value {
                file: self,
                ty: it.ty,
                offset: it.offset as usize,
            })
            .collect()
    }

    /// The first object of type `name` (or a subclass).
    pub fn first(&self, name: &str) -> Result<Value<'_>> {
        self.objects_of(name)
            .into_iter()
            .next()
            .ok_or_else(|| Error::Format(format!("no {name} object in tagfile")))
    }

    /// All object items in file order.
    pub fn objects(&self) -> impl Iterator<Item = Value<'_>> {
        self.items.iter().filter(|it| it.is_object).map(|it| Value {
            file: self,
            ty: it.ty,
            offset: it.offset as usize,
        })
    }

    fn item(&self, index: u32) -> Result<Option<&Item>> {
        if index == 0 {
            return Ok(None);
        }
        self.items
            .get(index as usize)
            .map(Some)
            .ok_or_else(|| Error::Format(format!("item index {index} out of range")))
    }
}

/// A typed view of one value inside a tagfile's DATA section.
#[derive(Clone, Copy)]
pub struct Value<'a> {
    pub file: &'a TagFile,
    pub ty: u32,
    pub offset: usize,
}

impl std::fmt::Debug for Value<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}@{:#x}", self.type_name(), self.offset)
    }
}

impl<'a> Value<'a> {
    pub fn type_name(&self) -> &'a str {
        self.file.types.name(self.ty)
    }

    pub fn is_a(&self, name: &str) -> bool {
        self.file.types.is_a(self.ty, name)
    }

    pub fn kind(&self) -> Kind {
        self.file.types.kind(self.ty)
    }

    fn bytes(&self, len: usize) -> Result<&'a [u8]> {
        slice(&self.file.data, self.offset, len)
    }

    fn u32_raw(&self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.bytes(4)?.try_into().unwrap()))
    }

    /// A member of this class value, searched through base classes.
    pub fn get(&self, name: &str) -> Result<Value<'a>> {
        let m =
            self.file.types.member(self.ty, name).ok_or_else(|| {
                Error::Format(format!("{} has no member {name}", self.type_name()))
            })?;
        Ok(Value {
            file: self.file,
            ty: m.ty,
            offset: self.offset + m.offset as usize,
        })
    }

    pub fn has(&self, name: &str) -> bool {
        self.file.types.member(self.ty, name).is_some()
    }

    pub fn int(&self) -> Result<i64> {
        let flags = self.file.types.type_flags(self.ty);
        let kind = self.kind();
        if !matches!(kind, Kind::Int | Kind::Bool) {
            return bail(format!("{} is not an integer", self.type_name()));
        }
        let signed = flags & 0x200 != 0;
        let size = self.file.types.size(self.ty);
        let b = self.bytes(size as usize)?;
        Ok(match (size, signed) {
            (1, false) => b[0] as i64,
            (1, true) => b[0] as i8 as i64,
            (2, false) => u16::from_le_bytes(b.try_into().unwrap()) as i64,
            (2, true) => i16::from_le_bytes(b.try_into().unwrap()) as i64,
            (4, false) => u32::from_le_bytes(b.try_into().unwrap()) as i64,
            (4, true) => i32::from_le_bytes(b.try_into().unwrap()) as i64,
            (8, _) => i64::from_le_bytes(b.try_into().unwrap()),
            _ => return bail(format!("{} has integer size {size}", self.type_name())),
        })
    }

    pub fn bool(&self) -> Result<bool> {
        Ok(self.int()? != 0)
    }

    pub fn f32(&self) -> Result<f32> {
        match (self.kind(), self.file.types.size(self.ty)) {
            (Kind::Float, 4) => Ok(f32::from_le_bytes(self.bytes(4)?.try_into().unwrap())),
            (Kind::Float, 8) => Ok(f64::from_le_bytes(self.bytes(8)?.try_into().unwrap()) as f32),
            _ => bail(format!("{} is not a float", self.type_name())),
        }
    }

    /// Follows a pointer. `None` for null.
    pub fn deref(&self) -> Result<Option<Value<'a>>> {
        if self.kind() != Kind::Pointer {
            return bail(format!("{} is not a pointer", self.type_name()));
        }
        Ok(self.file.item(self.u32_raw()?)?.map(|it| Value {
            file: self.file,
            ty: it.ty,
            offset: it.offset as usize,
        }))
    }

    /// Reads a string (`hkStringPtr`, `char*`).
    pub fn string(&self) -> Result<String> {
        if self.kind() != Kind::String {
            return bail(format!("{} is not a string", self.type_name()));
        }
        let Some(it) = self.file.item(self.u32_raw()?)? else {
            return Ok(String::new());
        };
        let bytes = slice(&self.file.data, it.offset as usize, it.count as usize)?;
        let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
        Ok(String::from_utf8_lossy(&bytes[..end]).into_owned())
    }

    /// Elements of an `hkArray` (out of line) or a `T[N]` tuple (inline).
    pub fn elements(&self) -> Result<Vec<Value<'a>>> {
        match self.kind() {
            Kind::Array => {
                let Some(it) = self.file.item(self.u32_raw()?)? else {
                    return Ok(Vec::new());
                };
                let stride = self.file.types.size(it.ty) as usize;
                Ok((0..it.count as usize)
                    .map(|i| Value {
                        file: self.file,
                        ty: it.ty,
                        offset: it.offset as usize + i * stride,
                    })
                    .collect())
            }
            Kind::Tuple(n) => {
                let ty = self.file.types.pointee(self.ty).ok_or_else(|| {
                    Error::Format(format!("{} has no element type", self.type_name()))
                })?;
                let stride = self.file.types.size(ty) as usize;
                Ok((0..n as usize)
                    .map(|i| Value {
                        file: self.file,
                        ty,
                        offset: self.offset + i * stride,
                    })
                    .collect())
            }
            _ => bail(format!("{} is not an array", self.type_name())),
        }
    }

    /// Raw bytes of an `hkArray<hkUint8>` (or any array, as its packed element bytes).
    pub fn array_bytes(&self) -> Result<&'a [u8]> {
        if self.kind() != Kind::Array {
            return bail(format!("{} is not an array", self.type_name()));
        }
        let Some(it) = self.file.item(self.u32_raw()?)? else {
            return Ok(&[]);
        };
        let stride = self.file.types.size(it.ty) as usize;
        slice(
            &self.file.data,
            it.offset as usize,
            stride * it.count as usize,
        )
    }

    /// Flattens any value made only of floats (vectors, quaternions, transforms) into its floats.
    pub fn floats(&self) -> Result<Vec<f32>> {
        let mut out = Vec::new();
        self.collect_floats(&mut out, 0)?;
        Ok(out)
    }

    fn collect_floats(&self, out: &mut Vec<f32>, depth: u32) -> Result<()> {
        if depth > 16 {
            return bail("float value nests too deeply");
        }
        match self.kind() {
            Kind::Float => out.push(self.f32()?),
            Kind::Tuple(_) => {
                for e in self.elements()? {
                    e.collect_floats(out, depth + 1)?;
                }
            }
            Kind::Class => {
                for m in self.file.types.all_members(self.ty) {
                    Value {
                        file: self.file,
                        ty: m.ty,
                        offset: self.offset + m.offset as usize,
                    }
                    .collect_floats(out, depth + 1)?;
                }
            }
            _ => return bail(format!("{} is not made of floats", self.type_name())),
        }
        Ok(())
    }

    /// Integers of an array or tuple of integers.
    pub fn ints(&self) -> Result<Vec<i64>> {
        self.elements()?.iter().map(|e| e.int()).collect()
    }

    /// A short human-readable description of this value, for debugging dumps.
    pub fn describe(&self, depth: u32) -> String {
        let pad = "  ".repeat(depth as usize);
        match self.kind() {
            Kind::Int | Kind::Bool => format!("{}", self.int().unwrap_or(0)),
            Kind::Float => format!("{}", self.f32().unwrap_or(f32::NAN)),
            Kind::String => format!("{:?}", self.string().unwrap_or_default()),
            Kind::Pointer => match self.deref() {
                Ok(Some(v)) => format!("-> {}@{:#x}", v.type_name(), v.offset),
                _ => "null".into(),
            },
            Kind::Array => match self.elements() {
                Ok(e) => format!(
                    "[{} x {}]",
                    e.len(),
                    e.first().map(|v| v.type_name()).unwrap_or("")
                ),
                Err(e) => format!("<{e}>"),
            },
            Kind::Tuple(_) => match self.floats() {
                Ok(f) => format!("{f:?}"),
                Err(_) => "(tuple)".into(),
            },
            Kind::Class if depth < 4 => {
                let mut s = format!("{} {{\n", self.type_name());
                for m in self.file.types.all_members(self.ty) {
                    let v = Value {
                        file: self.file,
                        ty: m.ty,
                        offset: self.offset + m.offset as usize,
                    };
                    s += &format!("{pad}  {}: {}\n", m.name, v.describe(depth + 1));
                }
                s + &pad + "}"
            }
            Kind::Class => format!("{} {{..}}", self.type_name()),
            Kind::Void | Kind::Opaque => "()".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn varint_widths() {
        let cases: &[(&[u8], u64)] = &[
            (&[0x05], 5),
            (&[0x81, 0x02], 0x102),
            (&[0xC1, 0x02, 0x03], 0x10203),
            (&[0xE1, 0x02, 0x03, 0x04], 0x1020304),
        ];
        for (bytes, want) in cases {
            assert_eq!(varint(&mut Reader::new(bytes)).unwrap(), *want);
        }
        assert!(varint(&mut Reader::new(&[0xF8])).is_err());
    }
}
