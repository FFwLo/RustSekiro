//! Draw parameters (`param/drawparam/*.gparam`, "filt" files): light sets, fog, tone mapping and
//! other renderer settings, grouped by editor page.
//!
//! Layout from SoulsFormatsNEXT `GPARAM` (Sekiro variant), reimplemented. Each param holds one or
//! more values, each tagged with a value id (0 is the base; other ids are per-area variants
//! selected elsewhere) and, in Sekiro, a time of day in hours. [`Param::at`] interpolates the
//! values of one id over the time of day.

use crate::reader::{Reader, Result, bail, utf16z};

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i32),
    Float(f32),
    Bool(bool),
    Vec2([f32; 2]),
    Vec3([f32; 3]),
    Vec4([f32; 4]),
    Bytes([u8; 4]),
}

impl Value {
    /// The value as floats (booleans and integers as 0/1 and their value).
    pub fn floats(&self) -> Vec<f32> {
        match self {
            Value::Int(i) => vec![*i as f32],
            Value::Float(f) => vec![*f],
            Value::Bool(b) => vec![*b as i32 as f32],
            Value::Vec2(v) => v.to_vec(),
            Value::Vec3(v) => v.to_vec(),
            Value::Vec4(v) => v.to_vec(),
            Value::Bytes(b) => b.iter().map(|&x| x as f32).collect(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub id: i32,
    /// Hours, 0 to 24.
    pub time: f32,
    pub value: Value,
}

#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub display_name: String,
    pub entries: Vec<Entry>,
}

impl Param {
    /// Nearest authored time key for discrete values such as LUT indices and switches.
    /// Circular distance wraps midnight; missing area rows fall back to id zero.
    pub fn nearest(&self, id: i32, hour: f32) -> Option<Vec<f32>> {
        let distance = |time: f32| {
            let d = (hour.rem_euclid(24.) - time.rem_euclid(24.)).abs();
            d.min(24. - d)
        };
        self.entries
            .iter()
            .filter(|e| e.id == id)
            .min_by(|a, b| distance(a.time).total_cmp(&distance(b.time)))
            .map(|e| e.value.floats())
            .or_else(|| (id != 0).then(|| self.nearest(0, hour)).flatten())
    }
    /// The value for `id` at `hour`, interpolated linearly between the nearest keyed times
    /// (wrapping around midnight). Falls back to id 0 when `id` has no entries.
    pub fn at(&self, id: i32, hour: f32) -> Option<Vec<f32>> {
        let mut keys: Vec<&Entry> = self.entries.iter().filter(|e| e.id == id).collect();
        if keys.is_empty() && id != 0 {
            return self.at(0, hour);
        }
        if keys.is_empty() {
            return None;
        }
        keys.sort_by(|a, b| a.time.total_cmp(&b.time));
        let hour = hour.rem_euclid(24.0);
        let after = keys.iter().position(|e| e.time > hour);
        let (a, b) = match after {
            Some(0) => (keys[keys.len() - 1], keys[0]),
            Some(i) => (keys[i - 1], keys[i]),
            None => (keys[keys.len() - 1], keys[0]),
        };
        let span = (b.time - a.time).rem_euclid(24.0);
        let t = if span > 0.0 {
            (hour - a.time).rem_euclid(24.0) / span
        } else {
            0.0
        };
        let (va, vb) = (a.value.floats(), b.value.floats());
        Some(va.iter().zip(&vb).map(|(x, y)| x + (y - x) * t).collect())
    }
}

#[derive(Debug, Clone)]
pub struct Group {
    pub name: String,
    pub display_name: String,
    pub params: Vec<Param>,
}

#[derive(Debug, Clone, Default)]
pub struct Gparam {
    pub groups: Vec<Group>,
}

impl Gparam {
    /// The param whose name contains `name`, in the group whose name contains `group`.
    pub fn param(&self, group: &str, name: &str) -> Option<&Param> {
        self.groups
            .iter()
            .filter(|g| g.name.contains(group))
            .flat_map(|g| &g.params)
            .find(|p| p.name.contains(name))
    }
}

/// Priority of each file kind in the engine's layer stack (the table at `0x142b54500`).
/// A higher priority overrides a lower one field by field.
/// The five layers of a map block take `BLOCK_LAYER + 0..=4`.
pub mod priority {
    pub const DEFAULT: u8 = 1;
    pub const MAP_BASE: u8 = 4;
    pub const BLOCK_LAYER: u8 = 5;
    pub const MAP_OVERRIDE: u8 = 10;
}

/// The group whose rows are registered as `(bank, row)` records with exe-recovered defaults.
const ROW_GROUP: &str = "LightSet";
/// Row used when no layer registered the requested row (the exe falls back to record `(1, 0)`).
pub const DEFAULT_ROW: i32 = -1;

impl Gparam {
    /// The row ids a file registers for a group: the value ids of the group's first param.
    /// The engine enumerates rows from that param alone (`17ad40`), so a row that only later
    /// params carry is never created.
    pub fn rows(&self, group: &str) -> std::collections::BTreeSet<i32> {
        self.groups
            .iter()
            .find(|g| g.name.contains(group))
            .and_then(|g| g.params.first())
            .map(|p| p.entries.iter().map(|e| e.id).collect())
            .unwrap_or_default()
    }

    /// Whether this file came from [`layer`] with a default file, which is marked by the default
    /// record's own [`DEFAULT_ROW`] entries.
    pub fn is_layered(&self) -> bool {
        self.rows(ROW_GROUP).contains(&DEFAULT_ROW)
    }

    /// The row to evaluate for a requested row id (see [`DEFAULT_ROW`]); the id itself for a
    /// plain file, which keeps the per-field fallback to row 0.
    /// A layered file resolves a row that no layer registered to the engine's `(1, 0)` record.
    pub fn light_row(&self, id: i32) -> i32 {
        if self.is_layered() && !self.rows(ROW_GROUP).contains(&id) {
            DEFAULT_ROW
        } else {
            id
        }
    }
}

/// Resolves a layer stack the way the engine evaluates a block bank.
///
/// Layers are applied in ascending priority and a higher layer overrides a lower one for every
/// `(group, param, value id)` it carries; the whole time-keyed list of that id comes from one
/// layer.
/// The row records of the `LightSet` group are created for the union of the rows of the layers
/// (see [`Gparam::rows`]).
/// A field that no layer authors for such a row keeps the record default, which is the
/// `defaults` file's row-0 value, never the row 0 of the block file.
/// `defaults` is the lowest layer (`default.gparam`) and is also used for [`DEFAULT_ROW`].
/// Layers with equal priority keep their slice order.
pub fn layer(layers: &[(u8, &Gparam)], defaults: Option<&Gparam>) -> Gparam {
    let mut ordered: Vec<&(u8, &Gparam)> = layers.iter().collect();
    ordered.sort_by_key(|(p, _)| *p);
    let mut out = Gparam::default();
    let sources: Vec<&Gparam> = defaults
        .into_iter()
        .chain(ordered.iter().map(|(_, g)| *g))
        .collect();
    for g in &sources {
        for group in &g.groups {
            let at = match out.groups.iter().position(|o| o.name == group.name) {
                Some(i) => i,
                None => {
                    out.groups.push(Group {
                        name: group.name.clone(),
                        display_name: group.display_name.clone(),
                        params: Vec::new(),
                    });
                    out.groups.len() - 1
                }
            };
            for param in &group.params {
                // Entries of this id from this layer replace any lower layer's entries.
                let target = &mut out.groups[at];
                let slot = match target.params.iter().position(|p| p.name == param.name) {
                    Some(i) => i,
                    None => {
                        target.params.push(Param {
                            name: param.name.clone(),
                            display_name: param.display_name.clone(),
                            entries: Vec::new(),
                        });
                        target.params.len() - 1
                    }
                };
                let merged = &mut target.params[slot].entries;
                let ids: std::collections::BTreeSet<i32> =
                    param.entries.iter().map(|e| e.id).collect();
                merged.retain(|e| !ids.contains(&e.id));
                merged.extend(param.entries.iter().cloned());
            }
        }
    }
    let Some(defaults) = defaults else {
        return out;
    };
    let mut rows: std::collections::BTreeSet<i32> =
        sources.iter().flat_map(|g| g.rows(ROW_GROUP)).collect();
    rows.insert(DEFAULT_ROW);
    let Some(group) = out.groups.iter().position(|g| g.name.contains(ROW_GROUP)) else {
        return out;
    };
    let fallback = defaults.groups.iter().find(|g| g.name.contains(ROW_GROUP));
    for param in &mut out.groups[group].params {
        let Some(default) = fallback.and_then(|g| g.params.iter().find(|p| p.name == param.name))
        else {
            continue;
        };
        let defaults: Vec<&Entry> = default.entries.iter().filter(|e| e.id == 0).collect();
        for &row in &rows {
            if row != 0 && !param.entries.iter().any(|e| e.id == row) {
                param.entries.extend(defaults.iter().map(|e| Entry {
                    id: row,
                    time: e.time,
                    value: e.value.clone(),
                }));
            }
        }
    }
    out
}

pub fn parse(data: &[u8]) -> Result<Gparam> {
    let mut r = Reader::new(data);
    if data.starts_with(b"f\0i\0l\0t\0") {
        r.seek(8);
    } else if data.starts_with(b"filt") {
        r.seek(4);
    } else {
        return bail("not a GPARAM file");
    }
    let game = r.u32()?;
    // Game 5 is the Sekiro layout (value id plus time of day). Game 3 is the Dark Souls 3 layout
    // that `mapoverride_*.gparam` keeps: value ids only, so every value has time 0.
    if game != 5 && game != 3 {
        return bail(format!("GPARAM for game {game}, expected Sekiro (5) or 3"));
    }
    let timed = game == 5;
    r.skip(4);
    let group_count = r.i32()? as usize;
    let _unk14 = r.i32()?;
    let header_size = r.i32()? as usize;
    let group_headers = r.i32()? as usize;
    let param_header_offsets = r.i32()? as usize;
    let param_headers = r.i32()? as usize;
    let values = r.i32()? as usize;
    let value_ids = r.i32()? as usize;

    let mut out = Gparam::default();
    for g in 0..group_count {
        let mut gr = Reader::at(data, header_size + g * 4);
        let go = group_headers + gr.i32()? as usize;
        let mut h = Reader::at(data, go);
        let param_count = h.i32()? as usize;
        let pho = param_header_offsets + h.i32()? as usize;
        let name = utf16z(data, go + 8)?;
        let display_name = utf16z(data, go + 8 + (name.encode_utf16().count() + 1) * 2)?;
        let mut params = Vec::with_capacity(param_count);
        for p in 0..param_count {
            let ph = param_headers + Reader::at(data, pho + p * 4).i32()? as usize;
            let mut x = Reader::at(data, ph);
            let vo = values + x.i32()? as usize;
            let io = value_ids + x.i32()? as usize;
            let ty = x.u8()?;
            let count = x.u8()? as usize;
            let name = utf16z(data, ph + 12)?;
            let display_name = utf16z(data, ph + 12 + (name.encode_utf16().count() + 1) * 2)?;
            let size = match ty {
                1 | 5 | 11 => 1,
                2 => 2,
                3 | 7 | 9 | 15 => 4,
                12..=14 => 16,
                other => return bail(format!("GPARAM param {name}: unknown type {other}")),
            };
            let mut entries = Vec::with_capacity(count);
            for i in 0..count {
                let mut v = Reader::at(data, vo + i * size);
                let value = match ty {
                    1 => Value::Int(v.u8()? as i32),
                    2 => Value::Int(v.i16()? as i32),
                    3 | 7 => Value::Int(v.i32()?),
                    5 | 11 => Value::Bool(v.u8()? != 0),
                    9 => Value::Float(v.f32()?),
                    12 => Value::Vec2([v.f32()?, v.f32()?]),
                    13 => Value::Vec3(v.vec3()?),
                    14 => Value::Vec4(v.vec4()?),
                    _ => Value::Bytes(v.bytes(4)?.try_into().unwrap()),
                };
                let mut id = Reader::at(data, io + i * if timed { 8 } else { 4 });
                entries.push(Entry {
                    id: id.i32()?,
                    time: if timed { id.f32()? } else { 0. },
                    value,
                });
            }
            params.push(Param {
                name,
                display_name,
                entries,
            });
        }
        out.groups.push(Group {
            name,
            display_name,
            params,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn param(times: &[(f32, f32)]) -> Param {
        Param {
            name: "p".into(),
            display_name: "p".into(),
            entries: times
                .iter()
                .map(|&(time, v)| Entry {
                    id: 0,
                    time,
                    value: Value::Float(v),
                })
                .collect(),
        }
    }

    #[test]
    fn interpolates_over_the_day() {
        let p = param(&[(6.0, 0.0), (12.0, 6.0), (18.0, 0.0)]);
        assert_eq!(p.at(0, 9.0).unwrap(), vec![3.0]);
        assert_eq!(p.at(0, 12.0).unwrap(), vec![6.0]);
        // Wraps from 18:00 to 06:00 through midnight.
        assert_eq!(p.at(0, 0.0).unwrap(), vec![0.0]);
        // Unknown ids fall back to the base values.
        assert_eq!(p.at(100, 15.0).unwrap(), vec![3.0]);
    }

    /// `(value id, time, value)` entries of one param.
    type Keys<'a> = &'a [(i32, f32, f32)];

    fn file(group: &str, params: &[(&str, Keys)]) -> Gparam {
        Gparam {
            groups: vec![Group {
                name: group.into(),
                display_name: group.into(),
                params: params
                    .iter()
                    .map(|(name, entries)| Param {
                        name: (*name).into(),
                        display_name: (*name).into(),
                        entries: entries
                            .iter()
                            .map(|&(id, time, v)| Entry {
                                id,
                                time,
                                value: Value::Float(v),
                            })
                            .collect(),
                    })
                    .collect(),
            }],
        }
    }

    fn at(g: &Gparam, name: &str, id: i32) -> f32 {
        g.param("LightSet", name).unwrap().at(id, 0.).unwrap()[0]
    }

    #[test]
    fn higher_layers_override_per_field_and_row() {
        let defaults = file(
            "LightSet",
            &[("Angle0", &[(0, 0., 9.)]), ("Hemi", &[(0, 0., 7.)])],
        );
        let base = file(
            "LightSet",
            &[
                ("Angle0", &[(0, 0., 1.), (100, 0., 2.)]),
                ("Hemi", &[(0, 0., 3.)]),
            ],
        );
        let overlay = file("LightSet", &[("Angle0", &[(100, 0., 5.)]), ("Hemi", &[])]);
        let map_base = file(
            "LightSet",
            &[
                ("Angle0", &[(0, 0., 4.), (100, 0., 4.), (130, 0., 4.)]),
                ("Hemi", &[(0, 0., 6.), (100, 0., 6.), (130, 0., 6.)]),
            ],
        );
        let g = layer(
            &[(10, &overlay), (4, &map_base), (5, &base)],
            Some(&defaults),
        );
        // The block layer beats the map base; the overlay beats both for its row only.
        assert_eq!(at(&g, "Angle0", 0), 1.);
        assert_eq!(at(&g, "Angle0", 100), 5.);
        // Row 100 has no block `Hemi`: it comes from the map base, not from block row 0.
        assert_eq!(at(&g, "Hemi", 100), 6.);
        assert_eq!(at(&g, "Hemi", 0), 3.);
        // A row only the map base registers.
        assert_eq!(at(&g, "Angle0", 130), 4.);
        assert!(g.is_layered());
    }

    #[test]
    fn unauthored_fields_take_the_default_record_not_row_zero() {
        let defaults = file(
            "LightSet",
            &[("Angle0", &[(0, 0., 9.)]), ("Hemi", &[(0, 0., 7.)])],
        );
        // Row 100 is registered by `Angle0` but lacks `Hemi`; row 0 has both.
        let base = file(
            "LightSet",
            &[
                ("Angle0", &[(0, 0., 1.), (100, 0., 2.)]),
                ("Hemi", &[(0, 0., 3.)]),
            ],
        );
        let g = layer(&[(5, &base)], Some(&defaults));
        assert_eq!(at(&g, "Hemi", 100), 7.);
        assert_eq!(at(&g, "Hemi", 0), 3.);
        // A row no layer registers resolves to the default record.
        assert_eq!(g.light_row(120), DEFAULT_ROW);
        assert_eq!(at(&g, "Angle0", g.light_row(120)), 9.);
        assert_eq!(g.light_row(100), 100);
        // A plain file keeps the per-field fallback to row 0.
        assert_eq!(base.light_row(120), 120);
    }

    #[test]
    fn rows_come_from_the_first_param_only() {
        let g = file(
            "LightSet",
            &[("Angle0", &[(0, 0., 0.)]), ("Other", &[(120, 0., 1.)])],
        );
        assert_eq!(g.rows("LightSet"), std::collections::BTreeSet::from([0]));
    }

    #[test]
    fn discrete_keys_wrap_and_do_not_invent_lut_ids() {
        let p = param(&[(2., 20.), (6., 0.), (12., 30.), (18., 0.), (22., 20.)]);
        assert_eq!(p.nearest(0, 10.).unwrap(), vec![30.]);
        assert_eq!(p.nearest(100, 23.).unwrap(), vec![20.]);
        assert_eq!(p.nearest(0, -1.).unwrap(), vec![20.]);
        assert_eq!(p.nearest(0, 7.).unwrap(), vec![0.]);
    }
}
