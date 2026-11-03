//! PARAMDEF layouts from the community Paramdex XML definitions (soulsmods/Paramdex, `SDT/Defs`).
//!
//! A definition lists the fields of one row struct in order. Each `Def` attribute reads
//! `type name[:bits][[count]] [= default]`. Fields are packed with no implicit alignment;
//! consecutive bitfields share one storage unit of their type until it is full, the type
//! changes, or a non-bitfield field follows (the rule SoulsFormats uses).

use std::collections::HashMap;
use std::path::Path;

use crate::reader::{Error, Result, bail};

/// A primitive field type in a PARAMDEF.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FieldType {
    S8,
    U8,
    S16,
    U16,
    S32,
    U32,
    F32,
    /// An f32 holding an angle in degrees.
    Angle32,
    /// A 32-bit boolean.
    B32,
    /// Padding, or an unnamed byte. Named single-bit `dummy8` fields carry real flags.
    Dummy8,
    /// A fixed-length Shift-JIS string; the count is in bytes.
    FixStr,
    /// A fixed-length UTF-16 string; the count is in characters.
    FixStrW,
}

impl FieldType {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "s8" => Self::S8,
            "u8" => Self::U8,
            "s16" => Self::S16,
            "u16" => Self::U16,
            "s32" => Self::S32,
            "u32" => Self::U32,
            "f32" => Self::F32,
            "angle32" => Self::Angle32,
            "b32" => Self::B32,
            "dummy8" => Self::Dummy8,
            "fixstr" => Self::FixStr,
            "fixstrW" => Self::FixStrW,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::S8 => "s8",
            Self::U8 => "u8",
            Self::S16 => "s16",
            Self::U16 => "u16",
            Self::S32 => "s32",
            Self::U32 => "u32",
            Self::F32 => "f32",
            Self::Angle32 => "angle32",
            Self::B32 => "b32",
            Self::Dummy8 => "dummy8",
            Self::FixStr => "fixstr",
            Self::FixStrW => "fixstrW",
        }
    }

    /// Size in bytes of one element.
    pub fn size(self) -> usize {
        match self {
            Self::S8 | Self::U8 | Self::Dummy8 | Self::FixStr => 1,
            Self::S16 | Self::U16 | Self::FixStrW => 2,
            Self::S32 | Self::U32 | Self::F32 | Self::Angle32 | Self::B32 => 4,
        }
    }

    pub fn is_float(self) -> bool {
        matches!(self, Self::F32 | Self::Angle32)
    }

    /// The storage type a bitfield of this type packs into (`dummy8` packs like `u8`).
    fn bit_storage(self) -> Option<Self> {
        match self {
            Self::Dummy8 | Self::U8 => Some(Self::U8),
            Self::S8 | Self::S16 | Self::U16 | Self::S32 | Self::U32 | Self::B32 => Some(self),
            _ => None,
        }
    }
}

/// One field of a row, with its resolved byte position.
#[derive(Debug, Clone)]
pub struct FieldDef {
    /// Internal (Paramdex) name. Repeated names in one def get a `_2`, `_3`, ... suffix.
    pub name: String,
    pub ty: FieldType,
    /// Element count for arrays and strings; 1 otherwise.
    pub count: usize,
    /// Bit width for bitfields.
    pub bits: Option<u8>,
    /// Byte offset in the row. For bitfields, the offset of the storage unit.
    pub offset: usize,
    /// Bit position within the storage unit (bitfields only).
    pub bit_offset: u8,
    /// Japanese display name from the def.
    pub display_name: String,
    /// Japanese description from the def.
    pub description: String,
    /// Enum type name, when the def names one.
    pub enum_name: Option<String>,
    pub default: Option<f64>,
    pub minimum: Option<f64>,
    pub maximum: Option<f64>,
}

impl FieldDef {
    /// Bytes this field occupies on its own (bitfields report their storage unit size).
    pub fn byte_size(&self) -> usize {
        self.ty.size() * self.count
    }

    /// True for unnamed padding: `dummy8` that is not a bitfield.
    pub fn is_padding(&self) -> bool {
        self.ty == FieldType::Dummy8 && self.bits.is_none()
    }
}

/// A parsed PARAMDEF.
#[derive(Debug, Clone)]
pub struct ParamDef {
    /// The struct name stored in PARAM headers, e.g. `ATK_PARAM_ST`.
    pub param_type: String,
    pub data_version: u16,
    pub format_version: u16,
    pub big_endian: bool,
    pub unicode: bool,
    /// File stem the def came from, e.g. `AtkParam`.
    pub source: String,
    pub fields: Vec<FieldDef>,
    /// Row size in bytes implied by the fields.
    pub size: usize,
    index: HashMap<String, usize>,
}

impl ParamDef {
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)?;
        let source = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_string();
        Self::parse(&text, &source).map_err(|e| Error::Format(format!("{}: {e}", path.display())))
    }

    pub fn parse(xml: &str, source: &str) -> Result<Self> {
        let doc =
            roxmltree::Document::parse(xml).map_err(|e| Error::Format(format!("bad XML: {e}")))?;
        let root = doc.root_element();
        if root.tag_name().name() != "PARAMDEF" {
            return bail("root element is not PARAMDEF");
        }
        let text = |tag: &str| -> Option<&str> {
            root.children()
                .find(|n| n.has_tag_name(tag))
                .and_then(|n| n.text())
                .map(str::trim)
        };
        let param_type = text("ParamType")
            .ok_or(Error::Format("missing ParamType".into()))?
            .to_string();
        let number = |tag: &str| -> Result<u16> {
            text(tag)
                .unwrap_or("0")
                .parse()
                .map_err(|_| Error::Format(format!("bad {tag}")))
        };
        let flag = |tag: &str| text(tag).is_some_and(|t| t.eq_ignore_ascii_case("true"));
        let data_version = number("DataVersion")?;
        let format_version = number("FormatVersion")?;
        let big_endian = flag("BigEndian");
        let unicode = flag("Unicode");

        let fields_node = root
            .children()
            .find(|n| n.has_tag_name("Fields"))
            .ok_or(Error::Format("missing Fields".into()))?;

        let mut fields = Vec::new();
        let mut layout = Layout::default();
        let mut seen: HashMap<String, usize> = HashMap::new();
        for node in fields_node.children().filter(|n| n.has_tag_name("Field")) {
            let def = node
                .attribute("Def")
                .ok_or(Error::Format("Field without Def".into()))?;
            let parsed = parse_def(def)?;
            let child = |tag: &str| -> Option<String> {
                node.children()
                    .find(|n| n.has_tag_name(tag))
                    .and_then(|n| n.text())
                    .map(|t| t.trim().to_string())
            };
            let child_num = |tag: &str| child(tag).and_then(|t| t.parse::<f64>().ok());
            let (offset, bit_offset) = layout.place(parsed.ty, parsed.count, parsed.bits)?;
            let occurrences = seen.entry(parsed.name.clone()).or_insert(0);
            *occurrences += 1;
            let name = if *occurrences == 1 {
                parsed.name
            } else {
                format!("{}_{}", parsed.name, occurrences)
            };
            fields.push(FieldDef {
                name,
                ty: parsed.ty,
                count: parsed.count,
                bits: parsed.bits,
                offset,
                bit_offset,
                display_name: child("DisplayName").unwrap_or_default(),
                description: child("Description").unwrap_or_default(),
                enum_name: child("Enum"),
                default: parsed.default,
                minimum: child_num("Minimum"),
                maximum: child_num("Maximum"),
            });
        }
        let size = layout.finish();
        let index = fields
            .iter()
            .enumerate()
            .map(|(i, f)| (f.name.clone(), i))
            .collect();
        Ok(Self {
            param_type,
            data_version,
            format_version,
            big_endian,
            unicode,
            source: source.to_string(),
            fields,
            size,
            index,
        })
    }

    pub fn field(&self, name: &str) -> Option<&FieldDef> {
        self.index.get(name).map(|&i| &self.fields[i])
    }

    pub fn field_index(&self, name: &str) -> Option<usize> {
        self.index.get(name).copied()
    }
}

struct ParsedDef {
    ty: FieldType,
    name: String,
    bits: Option<u8>,
    count: usize,
    default: Option<f64>,
}

/// Parses `type name[:bits][[count]] [= default]`.
fn parse_def(def: &str) -> Result<ParsedDef> {
    let (decl, default) = match def.split_once('=') {
        Some((d, v)) => (d.trim(), v.trim().parse::<f64>().ok()),
        None => (def.trim(), None),
    };
    let (ty, rest) = decl
        .split_once(char::is_whitespace)
        .ok_or_else(|| Error::Format(format!("bad field def {def:?}")))?;
    let ty = FieldType::parse(ty)
        .ok_or_else(|| Error::Format(format!("unknown field type in {def:?}")))?;
    let mut name = rest.trim();
    let mut count = 1;
    if let Some(open) = name.find('[') {
        let close = name
            .rfind(']')
            .filter(|&c| c > open)
            .ok_or_else(|| Error::Format(format!("bad array in {def:?}")))?;
        let inner = name[open + 1..close].trim();
        match inner.parse() {
            Ok(n) => count = n,
            // One Paramdex def annotates a scalar with its allowed values: `u8 x [0,1,2,3]`.
            Err(_) if name[..open].ends_with(char::is_whitespace) && close + 1 == name.len() => {}
            Err(_) => return bail(format!("bad array count in {def:?}")),
        }
        name = &name[..open];
    }
    let mut bits = None;
    if let Some((n, b)) = name.split_once(':') {
        let width: u8 = b
            .trim()
            .parse()
            .map_err(|_| Error::Format(format!("bad bit width in {def:?}")))?;
        bits = Some(width);
        name = n;
    }
    let name = name.trim();
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return bail(format!("bad field name in {def:?}"));
    }
    if matches!(ty, FieldType::FixStr | FieldType::FixStrW) && bits.is_some() {
        return bail(format!("string bitfield in {def:?}"));
    }
    Ok(ParsedDef {
        ty,
        name: name.to_string(),
        bits,
        count,
        default,
    })
}

/// Tracks the running byte offset and any open bitfield storage unit.
#[derive(Default)]
struct Layout {
    offset: usize,
    /// Open bitfield unit: (storage type, unit offset, bits used).
    unit: Option<(FieldType, usize, u8)>,
}

impl Layout {
    fn close_unit(&mut self) {
        if let Some((ty, _, _)) = self.unit.take() {
            self.offset += ty.size();
        }
    }

    fn place(&mut self, ty: FieldType, count: usize, bits: Option<u8>) -> Result<(usize, u8)> {
        let Some(width) = bits else {
            self.close_unit();
            let at = self.offset;
            self.offset += ty.size() * count;
            return Ok((at, 0));
        };
        let storage = ty
            .bit_storage()
            .ok_or_else(|| Error::Format(format!("{} cannot be a bitfield", ty.as_str())))?;
        let limit = (storage.size() * 8) as u8;
        if width == 0 || width > limit || count != 1 {
            return bail(format!("bad bitfield width {width} for {}", ty.as_str()));
        }
        match self.unit {
            Some((open, at, used)) if open == storage && used + width <= limit => {
                self.unit = Some((open, at, used + width));
                Ok((at, used))
            }
            _ => {
                self.close_unit();
                let at = self.offset;
                self.unit = Some((storage, at, width));
                Ok((at, 0))
            }
        }
    }

    fn finish(mut self) -> usize {
        self.close_unit();
        self.offset
    }
}

/// Every PARAMDEF in a Paramdex `Defs` folder, indexed by param type.
#[derive(Debug, Default)]
pub struct ParamDefs {
    defs: Vec<ParamDef>,
    by_type: HashMap<String, Vec<usize>>,
    errors: Vec<(String, String)>,
}

impl ParamDefs {
    /// Loads every `*.xml` in `dir`. Defs that fail to parse are skipped and listed in
    /// [`ParamDefs::errors`]; only an unreadable folder is an error.
    pub fn load_dir(dir: &Path) -> Result<Self> {
        let mut paths: Vec<_> = std::fs::read_dir(dir)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("xml")))
            .collect();
        paths.sort();
        let mut out = Self::default();
        for path in paths {
            match ParamDef::load(&path) {
                Ok(def) => out.insert(def),
                Err(e) => {
                    let name = path.file_name().unwrap_or_default().to_string_lossy();
                    out.errors.push((name.into_owned(), e.to_string()));
                }
            }
        }
        Ok(out)
    }

    /// `(file name, error)` for each def that failed to parse.
    pub fn errors(&self) -> &[(String, String)] {
        &self.errors
    }

    pub fn insert(&mut self, def: ParamDef) {
        self.by_type
            .entry(def.param_type.clone())
            .or_default()
            .push(self.defs.len());
        self.defs.push(def);
    }

    /// All defs declaring this param type (usually one).
    pub fn for_type<'a>(&'a self, param_type: &str) -> impl Iterator<Item = &'a ParamDef> + 'a {
        self.by_type
            .get(param_type)
            .into_iter()
            .flatten()
            .map(|&i| &self.defs[i])
    }

    pub fn iter(&self) -> impl Iterator<Item = &ParamDef> {
        self.defs.iter()
    }

    pub fn len(&self) -> usize {
        self.defs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.defs.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<PARAMDEF XmlVersion="3">
  <ParamType>TEST_ST</ParamType>
  <DataVersion>2</DataVersion>
  <BigEndian>False</BigEndian>
  <Unicode>True</Unicode>
  <FormatVersion>202</FormatVersion>
  <Fields>
    <Field Def="f32 a = 1"><DisplayName>A</DisplayName></Field>
    <Field Def="u8 f0:1" />
    <Field Def="u8 f1:3 = 1" />
    <Field Def="dummy8 pad0:4" />
    <Field Def="u8 f2:1" />
    <Field Def="u16 w:4" />
    <Field Def="s16 b" />
    <Field Def="fixstrW s[4]" />
    <Field Def="dummy8 pad[3]" />
    <Field Def="u32 big = 1E+08" />
    <Field Def="s16 b" />
  </Fields>
</PARAMDEF>"#;

    #[test]
    fn layout_and_bitfields() {
        let def = ParamDef::parse(XML, "Test").unwrap();
        assert_eq!(def.param_type, "TEST_ST");
        assert_eq!(def.data_version, 2);
        let at = |n: &str| {
            let f = def.field(n).unwrap();
            (f.offset, f.bit_offset)
        };
        assert_eq!(at("a"), (0, 0));
        assert_eq!(at("f0"), (4, 0));
        assert_eq!(at("f1"), (4, 1));
        assert_eq!(at("pad0"), (4, 4));
        // The u8 unit is full, so f2 opens a new one.
        assert_eq!(at("f2"), (5, 0));
        // A different storage type opens a new unit.
        assert_eq!(at("w"), (6, 0));
        assert_eq!(at("b"), (8, 0));
        assert_eq!(at("s"), (10, 0));
        assert_eq!(at("pad"), (18, 0));
        assert_eq!(at("big"), (21, 0));
        assert_eq!(def.field("big").unwrap().default, Some(1e8));
        assert_eq!(at("b_2"), (25, 0));
        assert_eq!(def.size, 27);
        assert_eq!(def.field("a").unwrap().display_name, "A");
    }
}
