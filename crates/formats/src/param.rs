//! PARAM tables: the game's numeric data (attacks, effects, NPC stats, ...).
//!
//! A PARAM file is a header, a row table (id, data offset, name offset) and packed row
//! structs whose layout comes from a PARAMDEF (see [`crate::paramdef`]). Layout knowledge is
//! from SoulsFormats `PARAM.cs`, reimplemented. Sekiro files use the "long data offset"
//! header (flags 0x85 at 0x2D) with the param type string stored at an offset; a few small
//! files use the older 32-bit layout with an inline type string, and both are read.
//!
//! ```no_run
//! # fn main() -> sekiro_formats::Result<()> {
//! use sekiro_formats::param::ParamSet;
//! let set = ParamSet::load(
//!     "cache/raw/param/gameparam/gameparam.parambnd.d".as_ref(),
//!     "cache/refs/paramdex/Defs".as_ref(),
//! )?;
//! let posture = set.table("AtkParam_Npc")?.row(10100100)?.int("atkStam")?;
//! # Ok(()) }
//! ```

pub mod typed;

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::Arc;

use serde::ser::{SerializeMap, SerializeSeq};
use serde::{Serialize, Serializer};

use crate::paramdef::{FieldDef, FieldType, ParamDef, ParamDefs};
use crate::reader::{Error, Reader, Result, bail, sjisz, slice, utf16z};

const FLAG_INT_DATA_OFFSET: u8 = 0x02;
const FLAG_LONG_DATA_OFFSET: u8 = 0x04;
const FLAG_OFFSET_PARAM_TYPE: u8 = 0x80;
const FLAG2_UNICODE_NAMES: u8 = 0x01;

/// One entry of the row table, before any def is applied.
#[derive(Debug, Clone)]
pub struct RawRow {
    pub id: i32,
    pub data_offset: usize,
    /// Name stored in the file; Sekiro mostly ships these empty.
    pub name: String,
}

/// A PARAM file read without a def: header, row table and the raw bytes.
#[derive(Debug, Clone)]
pub struct RawParam {
    pub param_type: String,
    pub data_version: u16,
    /// Header flag bytes at 0x2C..0x30 (endianness, format flags 1 and 2, def format).
    pub flags: [u8; 4],
    pub rows: Vec<RawRow>,
    /// Row stride measured from the data offsets, when it can be measured.
    pub detected_row_size: Option<usize>,
    /// Bytes available after the last row's data offset, up to the string area.
    pub last_row_room: usize,
    pub data: Vec<u8>,
}

impl RawParam {
    pub fn parse(data: Vec<u8>) -> Result<Self> {
        let mut r = Reader::new(&data);
        let strings_offset = r.u32()? as usize;
        let mut header = Reader::at(&data, 0x2C);
        let flags: [u8; 4] = header.bytes(4)?.try_into().unwrap();
        if flags[0] != 0 {
            return bail("big-endian PARAM files are not supported");
        }
        let format = flags[1];
        let long = format & FLAG_LONG_DATA_OFFSET != 0;
        r.seek(0x08);
        let data_version = r.u16()?;
        let row_count = r.u16()? as usize;
        let mut type_offset = None;
        let param_type = if format & FLAG_OFFSET_PARAM_TYPE != 0 {
            r.seek(0x10);
            let at = r.offset64()?;
            type_offset = Some(at);
            let raw = sjisz(&data, at)?;
            raw.trim_end().to_string()
        } else {
            let raw = slice(&data, 0x0C, 0x20)?;
            let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
            String::from_utf8_lossy(&raw[..end]).trim_end().to_string()
        };
        let table_start = if long || (format & 0x03) == (0x01 | FLAG_INT_DATA_OFFSET) {
            0x40
        } else {
            0x30
        };
        let unicode_names = flags[2] & FLAG2_UNICODE_NAMES != 0;

        r.seek(table_start);
        let mut rows = Vec::with_capacity(row_count);
        let mut name_offsets = Vec::with_capacity(row_count);
        for _ in 0..row_count {
            let id = r.i32()?;
            let (data_offset, name_offset) = if long {
                r.skip(4);
                (r.offset64()?, r.offset64()?)
            } else {
                (r.u32()? as usize, r.u32()? as usize)
            };
            let name = if name_offset == 0 {
                String::new()
            } else if unicode_names {
                utf16z(&data, name_offset)?
            } else {
                sjisz(&data, name_offset)?
            };
            if name_offset != 0 {
                name_offsets.push(name_offset);
            }
            rows.push(RawRow {
                id,
                data_offset,
                name,
            });
        }
        let table_end = r.pos();

        let mut offsets: Vec<usize> = rows.iter().map(|r| r.data_offset).collect();
        offsets.sort_unstable();
        offsets.dedup();
        if offsets.first().is_some_and(|&o| o < table_end) {
            return bail("row data overlaps the row table");
        }
        let detected_row_size = match offsets.as_slice() {
            [] | [_] => None,
            [first, second, ..] => {
                let stride = second - first;
                offsets
                    .windows(2)
                    .all(|w| w[1] - w[0] == stride)
                    .then_some(stride)
            }
        };
        // The data area ends where the strings begin.
        let last = offsets.last().copied().unwrap_or(table_end);
        let data_end = std::iter::once(strings_offset)
            .chain(type_offset)
            .chain(name_offsets)
            .filter(|&o| o > last)
            .min()
            .unwrap_or(data.len())
            .min(data.len());
        Ok(Self {
            param_type,
            data_version,
            flags,
            rows,
            detected_row_size,
            last_row_room: data_end.saturating_sub(last),
            data,
        })
    }

    /// Checks that rows of `size` bytes fit this file's layout. Returns the measured size
    /// on mismatch.
    pub fn check_row_size(&self, size: usize) -> std::result::Result<(), usize> {
        match self.detected_row_size {
            Some(stride) if stride != size => Err(stride),
            _ if !self.rows.is_empty() && self.last_row_room < size => Err(self.last_row_room),
            _ => Ok(()),
        }
    }
}

/// A field value read from a row.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i64),
    Float(f32),
    Str(String),
    Array(Vec<Value>),
    /// Padding bytes.
    Bytes(Vec<u8>),
}

impl Value {
    pub fn as_i64(&self) -> Option<i64> {
        match *self {
            Value::Int(v) => Some(v),
            _ => None,
        }
    }

    pub fn as_f32(&self) -> Option<f32> {
        match *self {
            Value::Float(v) => Some(v),
            _ => None,
        }
    }

    /// Any numeric value as f64.
    pub fn as_f64(&self) -> Option<f64> {
        match *self {
            Value::Int(v) => Some(v as f64),
            Value::Float(v) => Some(v as f64),
            _ => None,
        }
    }
}

impl Serialize for Value {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        match self {
            Value::Int(v) => s.serialize_i64(*v),
            Value::Float(v) => s.serialize_f32(*v),
            Value::Str(v) => s.serialize_str(v),
            Value::Array(items) => {
                let mut seq = s.serialize_seq(Some(items.len()))?;
                for item in items {
                    seq.serialize_element(item)?;
                }
                seq.end()
            }
            Value::Bytes(bytes) => {
                let mut seq = s.serialize_seq(Some(bytes.len()))?;
                for b in bytes {
                    seq.serialize_element(b)?;
                }
                seq.end()
            }
        }
    }
}

fn read_scalar(ty: FieldType, b: &[u8]) -> Value {
    match ty {
        FieldType::S8 => Value::Int(b[0] as i8 as i64),
        FieldType::U8 | FieldType::Dummy8 | FieldType::FixStr => Value::Int(b[0] as i64),
        FieldType::S16 => Value::Int(i16::from_le_bytes([b[0], b[1]]) as i64),
        FieldType::U16 | FieldType::FixStrW => Value::Int(u16::from_le_bytes([b[0], b[1]]) as i64),
        FieldType::S32 => Value::Int(i32::from_le_bytes(b[..4].try_into().unwrap()) as i64),
        FieldType::U32 | FieldType::B32 => {
            Value::Int(u32::from_le_bytes(b[..4].try_into().unwrap()) as i64)
        }
        FieldType::F32 | FieldType::Angle32 => {
            Value::Float(f32::from_le_bytes(b[..4].try_into().unwrap()))
        }
    }
}

/// Reads one field from a row's bytes. The caller guarantees the row is `def.size` long.
pub fn read_field(field: &FieldDef, row: &[u8]) -> Value {
    let at = field.offset;
    if let Some(bits) = field.bits {
        let unit = field.ty.size();
        let mut raw = 0u64;
        for (i, &b) in row[at..at + unit].iter().enumerate() {
            raw |= (b as u64) << (8 * i);
        }
        let mask = (1u64 << bits) - 1;
        return Value::Int(((raw >> field.bit_offset) & mask) as i64);
    }
    let bytes = &row[at..at + field.byte_size()];
    match field.ty {
        FieldType::FixStr => {
            let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
            let (text, _, _) = encoding_rs::SHIFT_JIS.decode(&bytes[..end]);
            Value::Str(text.into_owned())
        }
        FieldType::FixStrW => {
            let units: Vec<u16> = bytes
                .as_chunks::<2>()
                .0
                .iter()
                .map(|&c| u16::from_le_bytes(c))
                .take_while(|&u| u != 0)
                .collect();
            Value::Str(String::from_utf16_lossy(&units))
        }
        FieldType::Dummy8 => Value::Bytes(bytes.to_vec()),
        ty if field.count == 1 => read_scalar(ty, bytes),
        ty => Value::Array(
            bytes
                .chunks_exact(ty.size())
                .map(|c| read_scalar(ty, c))
                .collect(),
        ),
    }
}

/// A PARAM table bound to its def, with rows packed at the def's size.
#[derive(Debug)]
pub struct Table {
    name: String,
    def: Arc<ParamDef>,
    data_version: u16,
    ids: Vec<i32>,
    names: Vec<Option<String>>,
    data: Vec<u8>,
    index: HashMap<i32, usize>,
    duplicate_ids: usize,
}

impl Table {
    /// Binds a raw PARAM to a def. Row names come from the file, else from `names`.
    pub fn new(
        name: &str,
        raw: &RawParam,
        def: Arc<ParamDef>,
        names: Option<&HashMap<i32, String>>,
    ) -> Result<Self> {
        if let Err(found) = raw.check_row_size(def.size) {
            return bail(format!(
                "{name}: def {} is {} bytes per row, file has {found}",
                def.source, def.size
            ));
        }
        let size = def.size;
        let mut data = Vec::with_capacity(size * raw.rows.len());
        let mut ids = Vec::with_capacity(raw.rows.len());
        let mut row_names = Vec::with_capacity(raw.rows.len());
        let mut index = HashMap::with_capacity(raw.rows.len());
        let mut duplicate_ids = 0;
        for (i, row) in raw.rows.iter().enumerate() {
            data.extend_from_slice(slice(&raw.data, row.data_offset, size)?);
            ids.push(row.id);
            let name = if !row.name.is_empty() {
                Some(row.name.clone())
            } else {
                names.and_then(|n| n.get(&row.id).cloned())
            };
            row_names.push(name);
            // The first row with an id wins lookups; duplicates stay reachable by index.
            match index.entry(row.id) {
                std::collections::hash_map::Entry::Occupied(_) => duplicate_ids += 1,
                std::collections::hash_map::Entry::Vacant(v) => {
                    v.insert(i);
                }
            }
        }
        Ok(Self {
            name: name.to_string(),
            def,
            data_version: raw.data_version,
            ids,
            names: row_names,
            data,
            index,
            duplicate_ids,
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn def(&self) -> &ParamDef {
        &self.def
    }

    pub fn data_version(&self) -> u16 {
        self.data_version
    }

    pub fn len(&self) -> usize {
        self.ids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    /// Number of rows whose id repeats an earlier row's id.
    pub fn duplicate_ids(&self) -> usize {
        self.duplicate_ids
    }

    /// The row with this id, or an error naming the table and id.
    pub fn row(&self, id: i32) -> Result<Row<'_>> {
        self.find(id)
            .ok_or_else(|| Error::Format(format!("{} has no row {id}", self.name)))
    }

    pub fn find(&self, id: i32) -> Option<Row<'_>> {
        self.index.get(&id).map(|&index| Row { table: self, index })
    }

    pub fn row_at(&self, index: usize) -> Option<Row<'_>> {
        (index < self.ids.len()).then_some(Row { table: self, index })
    }

    pub fn rows(&self) -> impl ExactSizeIterator<Item = Row<'_>> {
        (0..self.ids.len()).map(|index| Row { table: self, index })
    }

    pub fn ids(&self) -> &[i32] {
        &self.ids
    }

    /// Looks up a field by name, with a helpful error.
    pub fn field(&self, name: &str) -> Result<&FieldDef> {
        self.def.field(name).ok_or_else(|| {
            Error::Format(format!(
                "{} ({}) has no field {name:?}",
                self.name, self.def.param_type
            ))
        })
    }
}

impl Serialize for Table {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        let mut map = s.serialize_map(Some(5))?;
        map.serialize_entry("table", &self.name)?;
        map.serialize_entry("paramType", &self.def.param_type)?;
        map.serialize_entry("dataVersion", &self.data_version)?;
        map.serialize_entry("rowSize", &self.def.size)?;
        map.serialize_entry("rows", &RowsSer(self))?;
        map.end()
    }
}

struct RowsSer<'a>(&'a Table);

impl Serialize for RowsSer<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        let mut seq = s.serialize_seq(Some(self.0.len()))?;
        for row in self.0.rows() {
            seq.serialize_element(&row)?;
        }
        seq.end()
    }
}

/// One row of a [`Table`].
#[derive(Clone, Copy)]
pub struct Row<'a> {
    table: &'a Table,
    index: usize,
}

impl std::fmt::Debug for Row<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}[{}]", self.table.name, self.id())
    }
}

impl<'a> Row<'a> {
    pub fn id(&self) -> i32 {
        self.table.ids[self.index]
    }

    pub fn name(&self) -> Option<&'a str> {
        self.table.names[self.index].as_deref()
    }

    pub fn table(&self) -> &'a Table {
        self.table
    }

    pub fn bytes(&self) -> &'a [u8] {
        let size = self.table.def.size;
        &self.table.data[self.index * size..(self.index + 1) * size]
    }

    /// Reads a field by Paramdex internal name.
    pub fn get(&self, field: &str) -> Result<Value> {
        Ok(read_field(self.table.field(field)?, self.bytes()))
    }

    /// Reads a field by its def entry (no name lookup).
    pub fn value(&self, field: &FieldDef) -> Value {
        read_field(field, self.bytes())
    }

    /// An f32 or angle32 field.
    pub fn f32(&self, field: &str) -> Result<f32> {
        self.get(field)?
            .as_f32()
            .ok_or_else(|| self.type_error(field, "f32"))
    }

    /// Any integer field (including bitfields and b32) widened to i64.
    pub fn int(&self, field: &str) -> Result<i64> {
        self.get(field)?
            .as_i64()
            .ok_or_else(|| self.type_error(field, "an integer"))
    }

    pub fn i32(&self, field: &str) -> Result<i32> {
        self.narrow(field)
    }

    pub fn u32(&self, field: &str) -> Result<u32> {
        self.narrow(field)
    }

    pub fn i16(&self, field: &str) -> Result<i16> {
        self.narrow(field)
    }

    pub fn u16(&self, field: &str) -> Result<u16> {
        self.narrow(field)
    }

    pub fn u8(&self, field: &str) -> Result<u8> {
        self.narrow(field)
    }

    pub fn i8(&self, field: &str) -> Result<i8> {
        self.narrow(field)
    }

    /// An integer field read as a flag (non-zero is true).
    pub fn bool(&self, field: &str) -> Result<bool> {
        Ok(self.int(field)? != 0)
    }

    pub fn str(&self, field: &str) -> Result<String> {
        match self.get(field)? {
            Value::Str(s) => Ok(s),
            _ => Err(self.type_error(field, "a string")),
        }
    }

    /// Every field in def order, including padding.
    pub fn fields(&self) -> impl Iterator<Item = (&'a FieldDef, Value)> + 'a {
        let bytes = self.bytes();
        self.table
            .def
            .fields
            .iter()
            .map(move |f| (f, read_field(f, bytes)))
    }

    fn narrow<T: TryFrom<i64>>(&self, field: &str) -> Result<T> {
        let v = self.int(field)?;
        T::try_from(v).map_err(|_| {
            Error::Format(format!(
                "{self:?}.{field} = {v} does not fit {}",
                std::any::type_name::<T>()
            ))
        })
    }

    fn type_error(&self, field: &str, wanted: &str) -> Error {
        let ty = self
            .table
            .def
            .field(field)
            .map(|f| f.ty.as_str())
            .unwrap_or("?");
        Error::Format(format!("{self:?}.{field} is {ty}, not {wanted}"))
    }
}

impl Serialize for Row<'_> {
    /// `{"id": .., "name": .., <field>: <value>, ...}` in def order, padding skipped.
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        let mut map = s.serialize_map(None)?;
        map.serialize_entry("id", &self.id())?;
        map.serialize_entry("name", &self.name())?;
        for (field, value) in self.fields() {
            if !field.is_padding() {
                map.serialize_entry(&field.name, &value)?;
            }
        }
        map.end()
    }
}

/// Why a PARAM file did not load (or loaded with a caveat).
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum IssueKind {
    /// The file could not be read as a PARAM.
    Unreadable { error: String },
    /// No def declares this param type.
    NoDef,
    /// Defs exist but none matches the file's row size.
    #[serde(rename_all = "camelCase")]
    SizeMismatch {
        defs: Vec<String>,
        def_sizes: Vec<usize>,
        file_row_size: Option<usize>,
    },
    /// Loaded, but the file's data version differs from the def's.
    #[serde(rename_all = "camelCase")]
    DataVersion { def: u16, file: u16 },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParamIssue {
    pub table: String,
    pub param_type: String,
    #[serde(flatten)]
    pub kind: IssueKind,
}

impl ParamIssue {
    /// True when the table was not loaded.
    pub fn is_fatal(&self) -> bool {
        !matches!(self.kind, IssueKind::DataVersion { .. })
    }
}

/// Reads a Paramdex `Names/<Table>.txt` file: `<id> <name>` per line.
pub fn parse_names(text: &str) -> HashMap<i32, String> {
    let mut out = HashMap::new();
    for line in text.lines() {
        let line = line.trim_start_matches('\u{feff}').trim();
        let (id, name) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
        if let Ok(id) = id.parse::<i32>() {
            let name = name.trim();
            if !name.is_empty() {
                out.entry(id).or_insert_with(|| name.to_string());
            }
        }
    }
    out
}

/// Every `*.param` in one unpacked binder folder, bound to Paramdex defs.
#[derive(Debug, Default)]
pub struct ParamSet {
    tables: BTreeMap<String, Table>,
    issues: Vec<ParamIssue>,
    def_errors: Vec<(String, String)>,
}

impl ParamSet {
    /// Loads `dir/*.param` with defs from `defs_dir`. Row names come from the sibling
    /// Paramdex `Names` folder (`defs_dir/../Names`) when it exists.
    pub fn load(dir: &Path, defs_dir: &Path) -> Result<Self> {
        let defs = ParamDefs::load_dir(defs_dir)?;
        let names = defs_dir.parent().map(|p| p.join("Names"));
        let mut set = Self::load_with(dir, &defs, names.as_deref().filter(|p| p.is_dir()))?;
        set.def_errors = defs.errors().to_vec();
        Ok(set)
    }

    /// Loads `dir/*.param` with already-loaded defs. Files that fail are recorded in
    /// [`ParamSet::issues`] rather than failing the whole set.
    pub fn load_with(dir: &Path, defs: &ParamDefs, names_dir: Option<&Path>) -> Result<Self> {
        let mut paths: Vec<_> = std::fs::read_dir(dir)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| {
                p.extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("param"))
            })
            .collect();
        paths.sort();
        // Tables that share a def (AtkParam_Npc and AtkParam_Pc) share one copy of it.
        let mut shared: HashMap<String, Arc<ParamDef>> = HashMap::new();
        let mut set = Self::default();
        for path in paths {
            let table = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_string();
            let names = names_dir
                .map(|d| d.join(format!("{table}.txt")))
                .and_then(|p| std::fs::read_to_string(p).ok())
                .map(|t| parse_names(&t));
            let raw = match std::fs::read(&path)
                .map_err(Error::from)
                .and_then(RawParam::parse)
            {
                Ok(raw) => raw,
                Err(e) => {
                    set.issues.push(ParamIssue {
                        table,
                        param_type: String::new(),
                        kind: IssueKind::Unreadable {
                            error: e.to_string(),
                        },
                    });
                    continue;
                }
            };
            let candidates: Vec<&ParamDef> = defs.for_type(&raw.param_type).collect();
            let issue = |kind| ParamIssue {
                table: table.clone(),
                param_type: raw.param_type.clone(),
                kind,
            };
            if candidates.is_empty() {
                set.issues.push(issue(IssueKind::NoDef));
                continue;
            }
            let Some(def) = candidates
                .iter()
                .find(|d| raw.check_row_size(d.size).is_ok())
            else {
                set.issues.push(issue(IssueKind::SizeMismatch {
                    defs: candidates.iter().map(|d| d.source.clone()).collect(),
                    def_sizes: candidates.iter().map(|d| d.size).collect(),
                    file_row_size: raw.detected_row_size.or(Some(raw.last_row_room)),
                }));
                continue;
            };
            if def.data_version != raw.data_version {
                set.issues.push(issue(IssueKind::DataVersion {
                    def: def.data_version,
                    file: raw.data_version,
                }));
            }
            let def = shared
                .entry(def.source.clone())
                .or_insert_with(|| Arc::new((*def).clone()))
                .clone();
            match Table::new(&table, &raw, def, names.as_ref()) {
                Ok(t) => {
                    set.tables.insert(table, t);
                }
                Err(e) => set.issues.push(issue(IssueKind::Unreadable {
                    error: e.to_string(),
                })),
            }
        }
        Ok(set)
    }

    /// The table loaded from `<name>.param`, or an error explaining why it is missing.
    pub fn table(&self, name: &str) -> Result<&Table> {
        self.tables.get(name).ok_or_else(|| {
            match self.issues.iter().find(|i| i.table == name && i.is_fatal()) {
                Some(issue) => Error::Format(format!("{name} did not load: {:?}", issue.kind)),
                None => Error::Format(format!("no param table {name}")),
            }
        })
    }

    pub fn get(&self, name: &str) -> Option<&Table> {
        self.tables.get(name)
    }

    pub fn tables(&self) -> impl Iterator<Item = &Table> {
        self.tables.values()
    }

    pub fn len(&self) -> usize {
        self.tables.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tables.is_empty()
    }

    /// Files that failed to load, and data-version caveats on ones that did.
    pub fn issues(&self) -> &[ParamIssue] {
        &self.issues
    }

    /// `(file name, error)` for Paramdex defs that failed to parse (set by [`ParamSet::load`]).
    pub fn def_errors(&self) -> &[(String, String)] {
        &self.def_errors
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a minimal Sekiro-layout PARAM (flags 0x85) with u16 rows.
    fn synth(rows: &[(i32, u16, &str)]) -> Vec<u8> {
        let n = rows.len();
        let table_end = 0x40 + 24 * n;
        let data_start = table_end;
        let strings = data_start + 2 * n;
        let mut out = vec![0u8; strings];
        out[0..4].copy_from_slice(&(strings as u32).to_le_bytes());
        out[8..10].copy_from_slice(&1u16.to_le_bytes());
        out[10..12].copy_from_slice(&(n as u16).to_le_bytes());
        out[0x2C..0x30].copy_from_slice(&[0, 0x85, 0x07, 0]);
        out[0x30..0x38].copy_from_slice(&(data_start as i64).to_le_bytes());
        let mut name_offsets = Vec::new();
        let mut strings_area = Vec::new();
        let type_offset = strings;
        strings_area.extend_from_slice(b"TEST_ST\0");
        for (_, _, name) in rows {
            name_offsets.push(strings + strings_area.len());
            for u in name.encode_utf16() {
                strings_area.extend_from_slice(&u.to_le_bytes());
            }
            strings_area.extend_from_slice(&[0, 0]);
        }
        out[0x10..0x18].copy_from_slice(&(type_offset as i64).to_le_bytes());
        for (i, (id, value, _)) in rows.iter().enumerate() {
            let at = 0x40 + 24 * i;
            out[at..at + 4].copy_from_slice(&id.to_le_bytes());
            out[at + 8..at + 16].copy_from_slice(&((data_start + 2 * i) as i64).to_le_bytes());
            out[at + 16..at + 24].copy_from_slice(&(name_offsets[i] as i64).to_le_bytes());
            out[data_start + 2 * i..data_start + 2 * i + 2].copy_from_slice(&value.to_le_bytes());
        }
        out.extend_from_slice(&strings_area);
        out
    }

    fn def() -> ParamDef {
        ParamDef::parse(
            r#"<PARAMDEF XmlVersion="3"><ParamType>TEST_ST</ParamType><DataVersion>1</DataVersion>
            <BigEndian>False</BigEndian><Unicode>True</Unicode><FormatVersion>202</FormatVersion>
            <Fields><Field Def="u8 lo" /><Field Def="u8 f:1" /><Field Def="u8 g:7" /></Fields></PARAMDEF>"#,
            "Test",
        )
        .unwrap()
    }

    #[test]
    fn reads_synthetic_param() {
        let raw = RawParam::parse(synth(&[(10, 0x0305, "ten"), (20, 0x8107, "")])).unwrap();
        assert_eq!(raw.param_type, "TEST_ST");
        assert_eq!(raw.detected_row_size, Some(2));
        let names = parse_names("20 twenty\n");
        let table = Table::new("Test", &raw, Arc::new(def()), Some(&names)).unwrap();
        let row = table.row(10).unwrap();
        assert_eq!(row.name(), Some("ten"));
        assert_eq!(row.u8("lo").unwrap(), 5);
        assert_eq!(row.int("f").unwrap(), 1);
        assert_eq!(row.int("g").unwrap(), 1);
        let row = table.row(20).unwrap();
        assert_eq!(row.name(), Some("twenty"));
        assert_eq!(row.int("g").unwrap(), 0x40);
        assert!(row.f32("lo").is_err());
        assert!(row.get("missing").is_err());
        let json = serde_json::to_string(&row).unwrap();
        assert_eq!(json, r#"{"id":20,"name":"twenty","lo":7,"f":1,"g":64}"#);
    }

    #[test]
    fn rejects_wrong_row_size() {
        let raw = RawParam::parse(synth(&[(1, 0, ""), (2, 0, "")])).unwrap();
        let mut wide = def();
        wide.size = 4;
        assert!(Table::new("Test", &raw, Arc::new(wide), None).is_err());
    }
}
