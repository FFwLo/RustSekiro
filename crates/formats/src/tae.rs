//! TAE (TimeAct Events) files: per-animation event timelines. Sekiro uses the 64-bit layout with
//! version `0x1000D`.
//!
//! Every event keeps its type, start/end seconds and raw parameter bytes. A [`Template`] (the
//! DSAnimStudio `TAE.Template.SDT.xml`, loaded at runtime from `cache/refs`) turns those bytes into
//! named, typed fields.
//!
//! Layout from the public SoulsFormatsNEXT TAE reader and DSAnimStudio, reimplemented.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::reader::{Reader, bail, slice, utf16z};
use crate::{Error, Result};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Tae {
    pub id: i32,
    /// Event bank (selects which template applies; Sekiro has one).
    pub event_bank: i64,
    pub skeleton_name: String,
    pub sib_name: String,
    pub animations: Vec<TaeAnimation>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum AnimHeader {
    /// Plays its own HKX, or another animation's HKX when `imports_hkx_from` is set.
    Standard {
        is_loop: bool,
        allow_delay_load: bool,
        imports_hkx_from: Option<i64>,
    },
    /// Takes both HKX and events from another animation.
    ImportOther { from: i64, unknown: i32 },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TaeAnimation {
    /// Full animation ID, e.g. 3000 for `a000_003000`, 50203000 for `a050_203000`.
    pub id: i64,
    pub header: Option<AnimHeader>,
    /// Source file name stored in the TAE, if any (authoring path, informational).
    pub file_name: String,
    pub events: Vec<TaeEvent>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TaeEvent {
    pub event_type: i32,
    pub start: f32,
    pub end: f32,
    pub unk04: i32,
    /// Event group type this event belongs to, if grouped.
    pub group: Option<i32>,
    /// Raw parameter bytes, as laid out in the file (may include trailing padding).
    pub params: Vec<u8>,
}

/// The animation category of a TAE file from its name: the player's split TAEs are named
/// `a<category>.tae` (`a00.tae`, `a50.tae`, `a200.tae`) and store IDs within that category.
/// Single-file TAEs (`c1010.tae`) store full IDs and get category 0.
pub fn category_from_file_name(file_name: &str) -> i64 {
    let stem = file_name.split('.').next().unwrap_or("");
    stem.strip_prefix('a')
        .and_then(|n| n.parse().ok())
        .unwrap_or(0)
}

/// The full animation ID (`category * 1_000_000 + id`) of an ID read from a TAE of `category`.
/// IDs that already carry a category are returned unchanged.
pub fn full_id(category: i64, id: i64) -> i64 {
    if id >= 1_000_000 {
        id
    } else {
        category * 1_000_000 + id
    }
}

/// `aXXX_YYYYYY` HKX name for a full animation ID.
pub fn anim_name(id: i64) -> String {
    format!("a{:03}_{:06}", id / 1_000_000, id % 1_000_000)
}

/// Parses `aXXX_YYYYYY` back into an animation ID.
pub fn anim_id(name: &str) -> Option<i64> {
    let s = name.strip_prefix('a')?;
    let (cat, rest) = s.split_once('_')?;
    let rest = rest.split('.').next()?;
    Some(cat.parse::<i64>().ok()? * 1_000_000 + rest.parse::<i64>().ok()?)
}

impl TaeAnimation {
    /// The animation whose HKX this one plays (itself unless it imports).
    pub fn hkx_source(&self) -> i64 {
        match &self.header {
            Some(AnimHeader::Standard {
                imports_hkx_from: Some(src),
                ..
            }) => *src,
            Some(AnimHeader::ImportOther { from, .. }) => *from,
            _ => self.id,
        }
    }
}

pub fn parse(data: &[u8]) -> Result<Tae> {
    let mut r = Reader::new(data);
    r.magic(b"TAE ")?;
    let big_endian = r.u8()?;
    r.skip(2);
    let is64 = r.u8()?;
    let version = r.u32()?;
    if big_endian != 0 || is64 != 0xFF || version != 0x1000D {
        return bail(format!(
            "unsupported TAE (big endian {big_endian}, 64-bit {is64:#x}, version {version:#x}); \
             only Sekiro 0x1000D is handled"
        ));
    }
    let event_bank = Reader::at(data, 0x30).i64()?;
    let mut h = Reader::at(data, 0x50);
    let id = h.i32()?;
    let count = h.i32()?;
    let anims_offset = h.offset64()?;
    let skel_off = Reader::at(data, 0xB0).offset64()?;
    let sib_off = Reader::at(data, 0xB8).offset64()?;
    let name_at = |o: usize| -> String {
        if o == 0 || o >= data.len() {
            String::new()
        } else {
            utf16z(data, o).unwrap_or_default()
        }
    };
    if !(0..=200_000).contains(&count) {
        return bail(format!("implausible TAE animation count {count}"));
    }
    let mut animations = Vec::with_capacity(count as usize);
    for i in 0..count as usize {
        let mut e = Reader::at(data, anims_offset + i * 16);
        let anim_id = e.i64()?;
        let body = e.offset64()?;
        animations.push(parse_animation(data, anim_id, body)?);
    }
    Ok(Tae {
        id,
        event_bank,
        skeleton_name: name_at(skel_off),
        sib_name: name_at(sib_off),
        animations,
    })
}

fn parse_animation(data: &[u8], id: i64, body: usize) -> Result<TaeAnimation> {
    let mut a = Reader::at(data, body);
    let events_offset = a.offset64()?;
    let groups_offset = a.offset64()?;
    let times_offset = a.offset64()?;
    let header_offset = a.offset64()?;
    let event_count = a.i32()?;
    let group_count = a.i32()?;
    if !(0..=10_000).contains(&event_count) || !(0..=10_000).contains(&group_count) {
        return bail(format!("animation {id} has implausible event counts"));
    }

    // Event headers: start pointer, end pointer, data pointer.
    let mut headers = Vec::with_capacity(event_count as usize);
    for i in 0..event_count as usize {
        let mut e = Reader::at(data, events_offset + i * 24);
        headers.push((e.offset64()?, e.offset64()?, e.offset64()?));
    }
    // Parameter extent: up to the next event's data block, or the group table.
    let mut data_starts: Vec<usize> = headers.iter().map(|h| h.2).collect();
    data_starts.sort_unstable();
    let mut events = Vec::with_capacity(headers.len());
    for &(start_off, end_off, data_off) in &headers {
        let start = Reader::at(data, start_off).f32()?;
        let end = Reader::at(data, end_off).f32()?;
        let mut d = Reader::at(data, data_off);
        let event_type = d.i32()?;
        let unk04 = d.i32()?;
        let params_off = d.offset64()?;
        let params = if params_off == 0 {
            Vec::new()
        } else {
            let limit = data_starts
                .iter()
                .copied()
                .find(|&s| s > data_off)
                .into_iter()
                .chain([groups_offset, times_offset, header_offset])
                .filter(|&s| s > params_off)
                .min()
                .unwrap_or(data.len())
                .min(data.len());
            slice(
                data,
                params_off,
                limit.saturating_sub(params_off).min(0x400),
            )?
            .to_vec()
        };
        events.push(TaeEvent {
            event_type,
            start,
            end,
            unk04,
            group: None,
            params,
        });
    }

    // Event groups: count, values offset, type offset, zero; values list event header offsets.
    for g in 0..group_count as usize {
        let mut gr = Reader::at(data, groups_offset + g * 32);
        let entries = gr.i64()?;
        let values = gr.offset64()?;
        let type_off = gr.offset64()?;
        let group_type = Reader::at(data, type_off).i32()?;
        if !(0..=10_000).contains(&entries) {
            return bail(format!("animation {id} event group has {entries} entries"));
        }
        for k in 0..entries as usize {
            // Values are 32-bit event header offsets even in 64-bit files.
            let header = Reader::at(data, values + k * 4).offset32()?;
            if header >= events_offset && (header - events_offset).is_multiple_of(24) {
                let index = (header - events_offset) / 24;
                if let Some(ev) = events.get_mut(index) {
                    ev.group = Some(group_type);
                }
            }
        }
    }

    let (header, file_name) = if header_offset == 0 {
        (None, String::new())
    } else {
        parse_anim_header(data, header_offset, times_offset)?
    };
    Ok(TaeAnimation {
        id,
        header,
        file_name,
        events,
    })
}

fn parse_anim_header(
    data: &[u8],
    offset: usize,
    times_offset: usize,
) -> Result<(Option<AnimHeader>, String)> {
    let mut m = Reader::at(data, offset);
    let kind = m.u32()?;
    m.skip(4);
    let inner = m.offset64()?;
    if inner == 0 {
        return Ok((None, String::new()));
    }
    let mut r = Reader::at(data, inner);
    let name_off = r.offset64()?;
    let header = match kind {
        0 => {
            let is_loop = r.u8()? != 0;
            let imports = r.u8()? != 0;
            let allow_delay_load = r.u8()? != 0;
            r.skip(1);
            let source = r.i32()?;
            AnimHeader::Standard {
                is_loop,
                allow_delay_load,
                imports_hkx_from: (imports && source >= 0).then_some(source as i64),
            }
        }
        1 => {
            let from = r.i32()? as i64;
            let unknown = r.i32()?;
            AnimHeader::ImportOther { from, unknown }
        }
        other => return bail(format!("unknown TAE animation header type {other}")),
    };
    let mut file_name = String::new();
    if name_off != 0 && name_off < data.len() && name_off != times_offset {
        // A name offset can point at the end of the file or at a non-name value.
        let looks_like_one = slice(data, name_off, 8).is_ok_and(|b| b == 1u64.to_le_bytes());
        if !looks_like_one
            && let Ok(s) = utf16z(data, name_off)
            && s.chars().all(|c| c.is_ascii_graphic() || c == ' ')
        {
            file_name = s;
        }
    }
    Ok((Some(header), file_name))
}

// ---------------------------------------------------------------------------------------------
// Template

/// The primitive type of a template field.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FieldType {
    S8,
    U8,
    Bool,
    S16,
    U16,
    S32,
    U32,
    S64,
    U64,
    F32,
    /// Two f32s (start and end value of a gradient).
    F32Grad,
    /// Fixed-length byte array.
    Bytes(u32),
}

impl FieldType {
    fn parse(tag: &str, length: Option<u32>) -> Option<Self> {
        Some(match tag {
            "s8" => Self::S8,
            "u8" | "x8" => Self::U8,
            "b" => Self::Bool,
            "s16" => Self::S16,
            "u16" | "x16" => Self::U16,
            "s32" => Self::S32,
            "u32" | "x32" => Self::U32,
            "s64" => Self::S64,
            "u64" | "x64" => Self::U64,
            "f32" => Self::F32,
            "f32grad" => Self::F32Grad,
            "aob" => Self::Bytes(length?),
            _ => return None,
        })
    }

    pub fn size(self) -> usize {
        match self {
            Self::S8 | Self::U8 | Self::Bool => 1,
            Self::S16 | Self::U16 => 2,
            Self::S32 | Self::U32 | Self::F32 => 4,
            Self::S64 | Self::U64 | Self::F32Grad => 8,
            Self::Bytes(n) => n as usize,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Field {
    /// Field name; unnamed fields get `Unk{offset:02X}`.
    pub name: String,
    pub ty: FieldType,
    pub offset: usize,
    /// Expected constant (padding) value, if the template asserts one.
    pub assert: Option<String>,
    /// Enum labels by value.
    pub entries: BTreeMap<i64, String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Action {
    pub id: i32,
    pub name: String,
    pub fields: Vec<Field>,
}

impl Action {
    pub fn size(&self) -> usize {
        self.fields
            .last()
            .map(|f| f.offset + f.ty.size())
            .unwrap_or(0)
    }
}

/// Event schemas by event type.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Template {
    pub game: String,
    pub actions: BTreeMap<i32, Action>,
}

impl Template {
    /// Parses a DSAnimStudio `TAE.Template.*.xml`.
    pub fn parse_xml(text: &str) -> Result<Self> {
        let doc = roxmltree::Document::parse(text)
            .map_err(|e| Error::Format(format!("TAE template XML: {e}")))?;
        let root = doc.root_element();
        let mut actions = BTreeMap::new();
        for node in root.children().filter(|n| n.has_tag_name("action")) {
            let id: i32 = node
                .attribute("id")
                .and_then(|v| v.parse().ok())
                .ok_or_else(|| Error::Format("TAE template action without id".into()))?;
            let name = node.attribute("name").unwrap_or("").to_string();
            let mut fields = Vec::new();
            let mut offset = 0;
            for f in node.children().filter(|n| n.is_element()) {
                let length = f.attribute("length").and_then(|v| v.parse().ok());
                let Some(ty) = FieldType::parse(f.tag_name().name(), length) else {
                    return bail(format!(
                        "TAE template action {id}: unknown field type {}",
                        f.tag_name().name()
                    ));
                };
                let entries = f
                    .children()
                    .filter(|e| e.has_tag_name("entry"))
                    .filter_map(|e| {
                        Some((
                            e.attribute("value")?.parse::<i64>().ok()?,
                            e.attribute("name")?.to_string(),
                        ))
                    })
                    .collect();
                fields.push(Field {
                    name: f
                        .attribute("name")
                        .map(str::to_string)
                        .unwrap_or_else(|| format!("Unk{offset:02X}")),
                    ty,
                    offset,
                    assert: f.attribute("assert").map(str::to_string),
                    entries,
                });
                offset += ty.size();
            }
            actions.insert(id, Action { id, name, fields });
        }
        Ok(Self {
            game: root.attribute("game").unwrap_or("").to_string(),
            actions,
        })
    }

    /// Decodes one event's parameters. Events with no template entry decode to `None`.
    pub fn decode(&self, event: &TaeEvent) -> Option<DecodedEvent> {
        let action = self.actions.get(&event.event_type)?;
        let mut fields = Vec::new();
        let mut assert_mismatches = Vec::new();
        let mut complete = true;
        for f in &action.fields {
            let Some(bytes) = event.params.get(f.offset..f.offset + f.ty.size()) else {
                // Some files pack the next event right after the last meaningful field, so
                // trailing asserted padding can be absent.
                if f.assert.is_none() {
                    complete = false;
                }
                continue;
            };
            let value = FieldValue::read(f.ty, bytes);
            if let Some(expected) = &f.assert {
                if !value.matches(expected) {
                    assert_mismatches.push(f.name.clone());
                } else {
                    continue;
                }
            }
            let label = value.as_int().and_then(|v| f.entries.get(&v).cloned());
            fields.push(DecodedField {
                name: f.name.clone(),
                value,
                label,
            });
        }
        Some(DecodedEvent {
            name: action.name.clone(),
            fields,
            complete,
            assert_mismatches,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FieldValue {
    Int(i64),
    Float(f32),
    Bool(bool),
    Grad([f32; 2]),
    Bytes(Vec<u8>),
}

impl FieldValue {
    fn read(ty: FieldType, b: &[u8]) -> Self {
        let f32_at = |o: usize| f32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        match ty {
            FieldType::S8 => Self::Int(b[0] as i8 as i64),
            FieldType::U8 => Self::Int(b[0] as i64),
            FieldType::Bool => Self::Bool(b[0] != 0),
            FieldType::S16 => Self::Int(i16::from_le_bytes([b[0], b[1]]) as i64),
            FieldType::U16 => Self::Int(u16::from_le_bytes([b[0], b[1]]) as i64),
            FieldType::S32 => Self::Int(i32::from_le_bytes(b[..4].try_into().unwrap()) as i64),
            FieldType::U32 => Self::Int(u32::from_le_bytes(b[..4].try_into().unwrap()) as i64),
            FieldType::S64 | FieldType::U64 => {
                Self::Int(i64::from_le_bytes(b[..8].try_into().unwrap()))
            }
            FieldType::F32 => Self::Float(f32_at(0)),
            FieldType::F32Grad => Self::Grad([f32_at(0), f32_at(4)]),
            FieldType::Bytes(_) => Self::Bytes(b.to_vec()),
        }
    }

    pub fn as_int(&self) -> Option<i64> {
        match self {
            Self::Int(v) => Some(*v),
            Self::Bool(b) => Some(*b as i64),
            _ => None,
        }
    }

    pub fn as_f32(&self) -> Option<f32> {
        match self {
            Self::Float(v) => Some(*v),
            Self::Int(v) => Some(*v as f32),
            _ => None,
        }
    }

    fn matches(&self, expected: &str) -> bool {
        match self {
            Self::Int(v) => expected.parse::<i64>().is_ok_and(|e| e == *v),
            Self::Bool(b) => expected.parse::<i64>().is_ok_and(|e| (e != 0) == *b),
            Self::Float(v) => expected.parse::<f32>().is_ok_and(|e| e == *v),
            _ => true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DecodedField {
    pub name: String,
    pub value: FieldValue,
    /// Enum label from the template, if the value has one.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub label: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DecodedEvent {
    pub name: String,
    /// Named fields; asserted padding that matched is omitted.
    pub fields: Vec<DecodedField>,
    /// False if the raw parameters ended before a non-padding template field.
    pub complete: bool,
    /// Asserted (padding) fields whose value differed from the template.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub assert_mismatches: Vec<String>,
}

impl DecodedEvent {
    pub fn field(&self, name: &str) -> Option<&FieldValue> {
        self.fields
            .iter()
            .find(|f| f.name == name)
            .map(|f| &f.value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const XML: &str = r#"<?xml version="1.0"?>
<action_template game="SDT">
  <action id="1" name="AttackBehavior">
    <s32 name="AttackType"><entry name="Standard" value="0" /></s32>
    <s32 />
    <s32 name="BehaviorJudgeID" />
    <u8 name="DirectionType" />
    <u8 name="Source" />
    <s16 name="StateInfo" />
  </action>
  <action id="2" name="Pad">
    <f32 name="A" />
    <s32 assert="0" />
    <f32grad name="G" />
    <aob name="Mask" length="2" />
  </action>
</action_template>"#;

    #[test]
    fn template_decodes_fields() {
        let t = Template::parse_xml(XML).unwrap();
        assert_eq!(t.actions[&1].size(), 16);
        assert_eq!(t.actions[&1].fields[1].name, "Unk04");
        let mut params = Vec::new();
        params.extend_from_slice(&0i32.to_le_bytes());
        params.extend_from_slice(&0i32.to_le_bytes());
        params.extend_from_slice(&100i32.to_le_bytes());
        params.extend_from_slice(&[3, 1]);
        params.extend_from_slice(&(-2i16).to_le_bytes());
        let ev = TaeEvent {
            event_type: 1,
            start: 0.0,
            end: 1.0,
            unk04: 0,
            group: None,
            params,
        };
        let d = t.decode(&ev).unwrap();
        assert!(d.complete);
        assert_eq!(d.field("BehaviorJudgeID"), Some(&FieldValue::Int(100)));
        assert_eq!(d.field("StateInfo"), Some(&FieldValue::Int(-2)));
        assert_eq!(d.fields[0].label.as_deref(), Some("Standard"));

        let mut p2 = Vec::new();
        p2.extend_from_slice(&1.5f32.to_le_bytes());
        p2.extend_from_slice(&7i32.to_le_bytes());
        p2.extend_from_slice(&1.0f32.to_le_bytes());
        p2.extend_from_slice(&2.0f32.to_le_bytes());
        p2.extend_from_slice(&[9, 8]);
        let d2 = t
            .decode(&TaeEvent {
                event_type: 2,
                params: p2,
                ..ev.clone()
            })
            .unwrap();
        assert_eq!(d2.assert_mismatches, vec!["Unk04".to_string()]);
        assert_eq!(d2.field("G"), Some(&FieldValue::Grad([1.0, 2.0])));
        assert_eq!(d2.field("Mask"), Some(&FieldValue::Bytes(vec![9, 8])));
    }

    #[test]
    fn anim_names_round_trip() {
        assert_eq!(anim_name(3000), "a000_003000");
        assert_eq!(anim_name(50_203_000), "a050_203000");
        assert_eq!(anim_id("a050_203000.hkx"), Some(50_203_000));
        assert_eq!(category_from_file_name("a200.tae"), 200);
        assert_eq!(category_from_file_name("a00.tae"), 0);
        assert_eq!(category_from_file_name("c1010.tae"), 0);
        assert_eq!(anim_name(full_id(200, 516_500)), "a200_516500");
        assert_eq!(full_id(200, 300_003_000), 300_003_000);
    }
}
