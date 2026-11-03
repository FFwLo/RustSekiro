//! Bounds-checked reader for the little-endian FXR version 5 tables used by Sekiro.
//!
//! The high bytes of record tags are retained: the low 16 bits identify a node, action or
//! property evaluator. Unknown semantics remain raw rather than being guessed as particles.

use crate::reader::{Reader, Result, bail, slice};
use std::ops::Range;

const STRIDES: [usize; 14] = [16, 16, 96, 48, 32, 64, 40, 32, 24, 16, 4, 4, 4, 4];

#[derive(Debug, Clone)]
pub struct Table {
    pub offset: usize,
    pub count: usize,
    pub stride: usize,
}

#[derive(Debug, Clone)]
pub struct State {
    pub tag: u32,
    pub transitions: Range<usize>,
}

#[derive(Debug, Clone)]
pub struct Transition {
    pub tag: u32,
    pub conditions: Range<usize>,
}

#[derive(Debug, Clone)]
pub struct Condition {
    pub words: Vec<u32>,
}

#[derive(Debug, Clone)]
pub struct Node {
    pub tag: u32,
    pub configurations: Range<usize>,
    pub actions: Range<usize>,
    pub children: Range<usize>,
}

#[derive(Debug, Clone)]
pub struct Configuration {
    pub tag: u32,
    pub fields: Vec<u32>,
    pub actions: Range<usize>,
}

#[derive(Debug, Clone)]
pub struct Action {
    pub tag: u32,
    pub variant: u32,
    /// The two fixed-field groups, without the property payloads interleaved after them.
    pub fields: [Vec<u32>; 2],
    pub properties: [Range<usize>; 2],
    /// Sequence descriptors in table 10, distinct from property modifiers in table 8.
    pub sequences: Range<usize>,
}

impl Action {
    /// Sekiro action schema positions (EvenTorset, Unlicense).
    /// These are semantic names, not a serialization of the reference definitions.
    pub fn property<'a>(&self, fxr: &'a Fxr, name: &str) -> Option<&'a Property> {
        let names: &[&str] = match self.id() {
            128 | 129 => &["duration"],
            131 => &["speed", "scale_x", "scale_y", "scale_z", "color"],
            300 => &["interval", "count", "total", "concurrent"],
            301 => &["distance", "total", "concurrent"],
            55 => &["gravity", "acceleration", "acceleration_multiplier"],
            60 => &["gravity", "speed", "speed_multiplier"],
            501 => &["angle", "distribution"],
            401 | 403 => &["radius"],
            402 => &["size_x", "size_y"],
            404 => &["size_x", "size_y", "size_z"],
            603 => &[
                "texture",
                "blend",
                "offset_x",
                "offset_y",
                "offset_z",
                "width",
                "height",
                "color",
                "emission_color",
                "hdr_color",
                "alpha_fade",
                "rotation_x",
                "rotation_y",
                "rotation_z",
                "angular_x",
                "angular_multiplier_x",
                "angular_y",
                "angular_multiplier_y",
                "angular_z",
                "angular_multiplier_z",
                "depth_offset",
                "frame",
                "frame_offset",
            ],
            604 => &[
                "blend",
                "offset_x",
                "offset_y",
                "offset_z",
                "width",
                "height",
                "rotation_x",
                "rotation_y",
                "rotation_z",
                "angular_x",
                "angular_multiplier_x",
                "angular_y",
                "angular_multiplier_y",
                "angular_z",
                "angular_multiplier_z",
                "color",
                "emission_color",
                "hdr_color",
                "layer_color_1",
                "layer_color_2",
                "layer_color_3",
                "alpha_fade",
                "frame",
                "frame_offset",
                "speed_u_1",
                "speed_v_1",
                "offset_u_1",
                "offset_v_1",
                "scale_u_1",
                "scale_v_1",
                "speed_u_2",
                "speed_v_2",
                "offset_u_2",
                "offset_v_2",
                "scale_u_2",
                "scale_v_2",
                "speed_u_3",
                "speed_v_3",
                "offset_u_3",
                "offset_v_3",
                "scale_u_3",
                "scale_v_3",
            ],
            609 => &["color", "specular_color", "radius"],
            _ => return None,
        };
        let (group, index) = if let Some(index) = names.iter().position(|&n| n == name) {
            (0, index)
        } else {
            let second: &[&str] = match self.id() {
                603 | 604 => &[
                    "brightness",
                    "alpha",
                    "unknown_2",
                    "unknown_3",
                    "unknown_4",
                    "unknown_5",
                    "alpha_threshold",
                ],
                609 => &[
                    "unknown_0",
                    "unknown_1",
                    "unknown_2",
                    "diffuse_multiplier",
                    "specular_multiplier",
                ],
                _ => return None,
            };
            (1, second.iter().position(|&n| n == name)?)
        };
        let range = &self.properties[group];
        (index < range.len())
            .then(|| fxr.properties.get(range.start + index))
            .flatten()
    }

    /// Integer/boolean fields retain their raw representation; float fields use `float_field`.
    pub fn field(&self, name: &str) -> Option<u32> {
        let index = match (self.id(), name) {
            (128, "delay")
            | (129, "attachment")
            | (131, "uniform")
            | (400..=402, "direction")
            | (603 | 604, "orientation") => 0,
            (128, "attachment") => 2,
            (603, "normal_map") | (604, "texture_1") => 1,
            (603, "variation_x") | (604, "texture_2") => 2,
            (603, "variation_y") | (604, "texture_3") => 3,
            (603 | 604, "uniform") => 4,
            (603 | 604, "columns") => 6,
            (603 | 604, "frames") => 7,
            (603 | 604, "interpolate_frames") => 8,
            _ => return None,
        };
        self.fields[0].get(index).copied()
    }

    pub fn float_field(&self, name: &str) -> Option<f32> {
        self.field(name)
            .map(f32::from_bits)
            .filter(|v| v.is_finite())
    }

    /// Transform actions store two three-component float vectors, not property records.
    pub fn transform(&self) -> Option<([f32; 3], [f32; 3])> {
        if !matches!(self.id(), 35 | 36) || self.fields[0].len() < 6 {
            return None;
        }
        let offset = std::array::from_fn(|i| f32::from_bits(self.fields[0][i]));
        let rotation = std::array::from_fn(|i| f32::from_bits(self.fields[0][i + 3]));
        offset
            .iter()
            .chain(&rotation)
            .all(|v| v.is_finite())
            .then_some((offset, rotation))
    }
    pub fn id(&self) -> u16 {
        self.tag as u16
    }

    /// Identifies appearances without assuming their still-undocumented field layouts.
    pub fn appearance(&self) -> Option<Appearance> {
        match self.id() {
            603 => Some(Appearance::Billboard),
            604 => Some(Appearance::MultiTextureBillboard),
            606 => Some(Appearance::Tracer),
            607 => Some(Appearance::Distortion),
            608 => Some(Appearance::RadialBlur),
            609 => Some(Appearance::PointLight),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Appearance {
    Billboard,
    MultiTextureBillboard,
    Tracer,
    Distortion,
    RadialBlur,
    PointLight,
}

/// Authored blend operations. Renderers must explicitly reject operations they cannot draw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Blend {
    Additive,
    Source,
    Alpha,
    Multiply,
    Subtract,
}

impl Blend {
    /// Decodes a blend field after its position has been established by the action schema.
    pub fn from_raw(value: u32) -> Option<Self> {
        match value {
            0 | 4 | 7 => Some(Self::Additive),
            1 => Some(Self::Source),
            2 | 6 => Some(Self::Alpha),
            3 => Some(Self::Multiply),
            5 => Some(Self::Subtract),
            _ => None,
        }
    }
}

/// A compiled, unmodified property with an explicitly selected sampling policy.
/// Compilation is done once; sampling does not allocate or decode payloads each frame.
#[derive(Debug, Clone)]
pub struct SampledProperty {
    dimensions: usize,
    argument: Option<u32>,
    stepped: bool,
    times: Vec<f32>,
    values: Vec<[f32; 4]>,
}

/// Sampling policy, selected by the caller once the native evaluator mapping is known.
/// This does not assign native names to raw property function IDs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interpolation {
    Stepped,
    Linear,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SamplingError {
    UnsupportedEvaluator,
    UnsupportedModifiers,
    UnsupportedLoop,
    InvalidPayload,
}

/// Native evaluator families, compiled once and sampled without allocation.
/// Hermite's controls are angles, following FwLo's MIT engine research, not slopes.
#[derive(Debug, Clone)]
pub struct NativeProperty {
    constant: Option<[f32; 4]>,
    curve: Option<Curve>,
}

impl NativeProperty {
    pub fn sample(&self, argument: f32) -> Option<[f32; 4]> {
        if !argument.is_finite() {
            return None;
        }
        if let Some(value) = self.constant {
            return Some(value);
        }
        let curve = self.curve.as_ref()?;
        let last = *curve.times.last()?;
        let argument = if curve.looping {
            argument.rem_euclid(last)
        } else {
            argument
        };
        let right = curve.times.partition_point(|&t| t <= argument);
        let left = right.saturating_sub(1);
        let mut value = [0.0; 4];
        let lanes = curve.values[0].len();
        for (lane, out) in value.iter_mut().enumerate().take(lanes) {
            let a = curve.values[left][lane];
            *out = if right == 0 || right == curve.times.len() || curve.function == 3 {
                a
            } else {
                let b = curve.values[right][lane];
                let x = (argument - curve.times[left]) / (curve.times[right] - curve.times[left]);
                match curve.function {
                    4 => a + (b - a) * x,
                    5 => {
                        let outgoing = curve.controls[(left * 2) * lanes + lane];
                        let incoming = curve.controls[(right * 2 + 1) * lanes + lane];
                        let u = 1.0 - x;
                        u.powi(3) * a
                            + 3.0 * u * u * x * (a + outgoing / 3.0)
                            + 3.0 * u * x * x * (b - incoming / 3.0)
                            + x.powi(3) * b
                    }
                    6 => {
                        let angles = [
                            curve.controls[left * 2 * lanes + lane],
                            curve.controls[(left * 2 + 1) * lanes + lane],
                        ];
                        a + (b - a) * hermite_angle_fraction(x, angles)
                    }
                    _ => return None,
                }
            };
        }
        value.iter().all(|v| v.is_finite()).then_some(value)
    }
}

fn hermite_angle_fraction(x: f32, angles: [f32; 2]) -> f32 {
    // Endpoint Hermite polynomials with authored engine tangents 0.7 and 1.5.
    let first = x * (0.7 + x * (0.1 + 0.2 * x));
    let second = x * (1.5 + x * (-0.7 + 0.2 * x));
    let slow = 1.0 - (1.0 - first * first).max(0.0).sqrt();
    let fast = (1.0 - (second - 1.0).powi(2)).max(0.0).sqrt();
    let quarter_turn = std::f32::consts::FRAC_PI_4;
    let mix = |angle: f32, low: f32, high: f32| {
        if angle < quarter_turn {
            low + (x - low) * angle / quarter_turn
        } else {
            x + (high - x) * (angle / quarter_turn - 1.0)
        }
    };
    let start = mix(angles[0], slow, fast);
    let end = mix(angles[1], fast, slow);
    start * (1.0 - x) + end * x
}

impl SampledProperty {
    pub fn dimensions(&self) -> usize {
        self.dimensions
    }

    /// The caller resolves this authored argument ID to the appropriate effect clock.
    pub fn argument(&self) -> Option<u32> {
        self.argument
    }

    /// Samples an explicitly supplied argument, holding the endpoint outside the key range.
    /// Non-finite arguments are rejected rather than entering a particle transform.
    pub fn sample(&self, argument: f32) -> Option<[f32; 4]> {
        if !argument.is_finite() {
            return None;
        }
        if self.times.is_empty() || argument <= self.times[0] {
            return Some(self.values[0]);
        }
        let right = self.times.partition_point(|&time| time <= argument);
        if right == self.times.len() || self.stepped {
            return Some(self.values[right - 1]);
        }
        let left = right - 1;
        let t = ((f64::from(argument) - f64::from(self.times[left]))
            / (f64::from(self.times[right]) - f64::from(self.times[left]))) as f32;
        let mut result = [0.0; 4];
        for (i, output) in result.iter_mut().enumerate().take(self.dimensions) {
            *output = self.values[left][i] * (1.0 - t) + self.values[right][i] * t;
        }
        Some(result)
    }
}

#[derive(Debug, Clone)]
pub struct Property {
    pub tag: u32,
    pub evaluator: u32,
    /// Raw little-endian words, preserving integers and floating-point bits alike.
    pub values: Vec<u32>,
    pub modifiers: Range<usize>,
}

/// Structure of a sequence payload, retaining cubic controls without guessing their evaluator.
#[derive(Debug, Clone, PartialEq)]
pub struct Curve {
    pub function: u16,
    pub looping: bool,
    pub argument: u32,
    pub bounds: [Vec<f32>; 2],
    pub times: Vec<f32>,
    pub values: Vec<Vec<f32>>,
    pub controls: Vec<f32>,
}

impl Property {
    pub fn compile_native_unmodified(&self) -> std::result::Result<NativeProperty, SamplingError> {
        if !self.modifiers.is_empty() {
            return Err(SamplingError::UnsupportedModifiers);
        }
        if let Some(value) = self.constant() {
            if value.iter().any(|v| !v.is_finite()) {
                return Err(SamplingError::InvalidPayload);
            }
            let mut lanes = [0.0; 4];
            lanes[..value.len()].copy_from_slice(&value);
            return Ok(NativeProperty {
                constant: Some(lanes),
                curve: None,
            });
        }
        let curve = self.curve().ok_or(SamplingError::UnsupportedEvaluator)?;
        if curve
            .times
            .iter()
            .chain(curve.values.iter().flatten())
            .chain(&curve.controls)
            .any(|v| !v.is_finite())
            || curve.times.windows(2).any(|v| v[0] >= v[1])
            || (curve.looping && curve.times.last().is_none_or(|v| *v <= 0.0))
        {
            return Err(SamplingError::InvalidPayload);
        }
        Ok(NativeProperty {
            constant: None,
            curve: Some(curve),
        })
    }
    /// Compiles constants or control-free key arrays using an explicit interpolation policy.
    /// Random modifiers and cubic/loop semantics remain explicit errors, so callers can
    /// retain their visual fallback. Native function-ID mapping is not assumed here.
    pub fn compile_unmodified(
        &self,
        interpolation: Interpolation,
    ) -> std::result::Result<SampledProperty, SamplingError> {
        if !self.modifiers.is_empty() {
            return Err(SamplingError::UnsupportedModifiers);
        }
        let (argument, stepped, times, values) = if let Some(value) = self.constant() {
            (None, false, Vec::new(), vec![value])
        } else {
            let function = (self.tag >> 4) & 255;
            if !matches!(function, 3 | 4) {
                return Err(SamplingError::UnsupportedEvaluator);
            }
            let curve = self.curve().ok_or(SamplingError::InvalidPayload)?;
            if curve.looping {
                return Err(SamplingError::UnsupportedLoop);
            }
            if curve.times.iter().any(|v| !v.is_finite())
                || curve.times.windows(2).any(|pair| pair[0] >= pair[1])
            {
                return Err(SamplingError::InvalidPayload);
            }
            (
                Some(curve.argument),
                interpolation == Interpolation::Stepped,
                curve.times,
                curve.values,
            )
        };
        let dimensions = values[0].len();
        let mut packed = Vec::with_capacity(values.len());
        for value in values {
            if value.iter().any(|v| !v.is_finite()) {
                return Err(SamplingError::InvalidPayload);
            }
            let mut lanes = [0.0; 4];
            lanes[..dimensions].copy_from_slice(&value);
            packed.push(lanes);
        }
        Ok(SampledProperty {
            dimensions,
            argument,
            stepped,
            times,
            values: packed,
        })
    }

    /// Decodes the structure shared by stepped, linear and two cubic evaluator families.
    /// Bounds and cubic control words are data, not an assertion of native interpolation rules.
    pub fn curve(&self) -> Option<Curve> {
        let dimensions = match self.tag & 15 {
            0 => 1,
            1 => 2,
            2 => 3,
            3 => 4,
            _ => return None,
        };
        let function = ((self.tag >> 4) & 255) as u16;
        let controls_per_key = match function {
            3 | 4 => 0,
            5 | 6 => 2 * dimensions,
            _ => return None,
        };
        let header = 1 + 2 * dimensions;
        let payload = self.values.len().checked_sub(header)?;
        let stride = 1 + dimensions + controls_per_key;
        if payload == 0 || payload % stride != 0 {
            return None;
        }
        let count = payload / stride;
        let floats: Vec<_> = self.values.iter().map(|&v| f32::from_bits(v)).collect();
        let times = floats[header..header + count].to_vec();
        let values_end = header + count + count * dimensions;
        let values = floats[header + count..values_end]
            .chunks_exact(dimensions)
            .map(<[f32]>::to_vec)
            .collect();
        Some(Curve {
            function,
            looping: self.tag & 0x1000 != 0,
            argument: self.values[0],
            bounds: [
                floats[1..1 + dimensions].to_vec(),
                floats[1 + dimensions..header].to_vec(),
            ],
            times,
            values,
            controls: floats[values_end..].to_vec(),
        })
    }

    /// Constant evaluators observed in the version 5 files. Curves are deliberately distinct.
    pub fn constant(&self) -> Option<Vec<f32>> {
        let dimensions = match self.tag & 15 {
            0 => 1,
            1 => 2,
            2 => 3,
            3 => 4,
            _ => return None,
        };
        match self.tag & 0xfff0 {
            0 => Some(vec![0.0; dimensions]),
            0x10 => Some(vec![1.0; dimensions]),
            0x20 if self.values.len() == dimensions => {
                Some(self.values.iter().map(|&v| f32::from_bits(v)).collect())
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Modifier {
    pub tag: u32,
    pub evaluator: u32,
    pub values: Vec<u32>,
}

#[derive(Debug, Clone)]
pub struct Sequence {
    pub values: Vec<u32>,
}

#[derive(Debug, Clone)]
pub struct Fxr {
    pub id: u32,
    pub tables: Vec<Table>,
    pub states: Vec<State>,
    pub transitions: Vec<Transition>,
    pub conditions: Vec<Condition>,
    pub nodes: Vec<Node>,
    pub configurations: Vec<Configuration>,
    pub actions: Vec<Action>,
    pub properties: Vec<Property>,
    pub modifiers: Vec<Modifier>,
    pub sequences: Vec<Sequence>,
    /// Whole input retained for state/transition/condition records and unknown fields.
    pub bytes: Vec<u8>,
}

impl Fxr {
    /// State zero is the initial state. Action 199 maps it to a local configuration index.
    pub fn initial_configuration(&self, node: &Node) -> Option<&Configuration> {
        let index = self
            .actions
            .get(node.actions.clone())?
            .iter()
            .find(|a| a.id() == 199)
            .and_then(|a| {
                self.sequences
                    .get(a.sequences.start)
                    .filter(|_| !a.sequences.is_empty())
            })
            .and_then(|s| s.values.first())
            .copied()
            .unwrap_or(0) as usize;
        if index >= node.configurations.len() {
            return None;
        }
        self.configurations.get(node.configurations.start + index)
    }
}

fn word(data: &[u8], at: usize) -> Result<u32> {
    Reader::at(data, at).u32()
}

fn offset(data: &[u8], at: usize) -> Result<usize> {
    Reader::at(data, at).offset64()
}

fn words(data: &[u8], offset: usize, count: usize) -> Result<Vec<u32>> {
    let length = count
        .checked_mul(4)
        .ok_or_else(|| crate::Error::Format("FXR word count overflow".into()))?;
    Ok(slice(data, offset, length)?
        .as_chunks::<4>()
        .0
        .iter()
        .map(|&w| u32::from_le_bytes(w))
        .collect())
}

fn reference(table: &Table, offset: usize, count: usize) -> Result<Range<usize>> {
    if count == 0 {
        return Ok(0..0);
    }
    let Some(relative) = offset.checked_sub(table.offset) else {
        return bail("FXR reference precedes its table");
    };
    if relative % table.stride != 0 {
        return bail("unaligned FXR table reference");
    }
    let start = relative / table.stride;
    let Some(end) = start.checked_add(count).filter(|&end| end <= table.count) else {
        return bail("FXR reference exceeds its table");
    };
    Ok(start..end)
}

/// Reads the table graph without interpreting undocumented action field positions.
pub fn parse(data: &[u8]) -> Result<Fxr> {
    slice(data, 0, 144)?;
    let mut r = Reader::new(data);
    r.magic(b"FXR\0")?;
    if r.u32()? != 0x0005_0000 || r.u32()? != 1 {
        return bail("expected little-endian Sekiro FXR version 5");
    }
    let id = r.u32()?;
    let mut tables = Vec::new();
    for &stride in &STRIDES[..11] {
        let offset = r.u32()? as usize;
        let count = r.u32()? as usize;
        slice(
            data,
            offset,
            count
                .checked_mul(stride)
                .ok_or_else(|| crate::Error::Format("FXR table length overflow".into()))?,
        )?;
        tables.push(Table {
            offset,
            count,
            stride,
        });
    }
    // Version 5 inserts two header flags before its three additional tables.
    r.u32()?;
    r.u32()?;
    for &stride in &STRIDES[11..] {
        let offset = r.u32()? as usize;
        let count = r.u32()? as usize;
        slice(
            data,
            offset,
            count
                .checked_mul(stride)
                .ok_or_else(|| crate::Error::Format("FXR table length overflow".into()))?,
        )?;
        tables.push(Table {
            offset,
            count,
            stride,
        });
    }
    let range =
        |table: usize, offset: usize, count: u32| reference(&tables[table], offset, count as usize);
    let values = |offset: usize, count: usize| {
        reference(&tables[10], offset, count)?;
        words(data, offset, count)
    };
    let mut states = Vec::new();
    let table = &tables[0];
    for i in 0..table.count {
        let at = table.offset + i * table.stride;
        states.push(State {
            tag: word(data, at)?,
            transitions: range(1, offset(data, at + 8)?, word(data, at + 4)?)?,
        });
    }
    let mut transitions = Vec::new();
    let table = &tables[1];
    for i in 0..table.count {
        let at = table.offset + i * table.stride;
        transitions.push(Transition {
            tag: word(data, at)?,
            conditions: range(2, offset(data, at + 8)?, word(data, at + 4)?)?,
        });
    }
    let mut conditions = Vec::new();
    let table = &tables[2];
    for i in 0..table.count {
        conditions.push(Condition {
            words: words(data, table.offset + i * table.stride, 24)?,
        });
    }
    let mut nodes = Vec::new();
    let table = &tables[3];
    for i in 0..table.count {
        let at = table.offset + i * table.stride;
        nodes.push(Node {
            tag: word(data, at)?,
            configurations: range(4, offset(data, at + 24)?, word(data, at + 8)?)?,
            actions: range(5, offset(data, at + 32)?, word(data, at + 12)?)?,
            children: range(3, offset(data, at + 40)?, word(data, at + 16)?)?,
        });
    }
    let mut configurations = Vec::new();
    let table = &tables[4];
    for i in 0..table.count {
        let at = table.offset + i * table.stride;
        configurations.push(Configuration {
            tag: word(data, at)?,
            fields: values(offset(data, at + 16)?, word(data, at + 8)? as usize)?,
            actions: range(5, offset(data, at + 24)?, word(data, at + 12)?)?,
        });
    }
    let mut actions = Vec::new();
    let table = &tables[5];
    for i in 0..table.count {
        let at = table.offset + i * table.stride;
        let first_fields = word(data, at + 8)? as usize;
        let second_fields = word(data, at + 20)? as usize;
        let first_properties = word(data, at + 16)?;
        let second_properties = word(data, at + 28)?;
        let fields_offset = offset(data, at + 32)?;
        let property_offset = offset(data, at + 48)?;
        actions.push(Action {
            tag: word(data, at)?,
            variant: word(data, at + 4)?,
            fields: [
                values(fields_offset, first_fields)?,
                values(fields_offset + first_fields * 4, second_fields)?,
            ],
            properties: [
                range(6, property_offset, first_properties)?,
                range(
                    6,
                    property_offset
                        .checked_add(first_properties as usize * 40)
                        .ok_or_else(|| {
                            crate::Error::Format("FXR property offset overflow".into())
                        })?,
                    second_properties,
                )?,
            ],
            sequences: range(9, offset(data, at + 40)?, word(data, at + 12)?)?,
        });
    }
    let mut properties = Vec::new();
    let table = &tables[6];
    for i in 0..table.count {
        let at = table.offset + i * table.stride;
        properties.push(Property {
            tag: word(data, at)?,
            evaluator: word(data, at + 4)?,
            values: values(offset(data, at + 16)?, word(data, at + 8)? as usize)?,
            modifiers: range(7, offset(data, at + 24)?, word(data, at + 32)?)?,
        });
    }
    let mut modifiers = Vec::new();
    let table = &tables[7];
    for i in 0..table.count {
        let at = table.offset + i * table.stride;
        modifiers.push(Modifier {
            tag: word(data, at)?,
            evaluator: word(data, at + 4)?,
            values: values(offset(data, at + 16)?, word(data, at + 8)? as usize)?,
        });
    }
    let mut sequences = Vec::new();
    let table = &tables[9];
    for i in 0..table.count {
        let at = table.offset + i * table.stride;
        sequences.push(Sequence {
            values: values(offset(data, at)?, word(data, at + 8)? as usize)?,
        });
    }
    Ok(Fxr {
        id,
        tables,
        states,
        transitions,
        conditions,
        nodes,
        configurations,
        actions,
        properties,
        modifiers,
        sequences,
        bytes: data.to_vec(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_curves_use_stepping_looping_and_angle_controls() {
        let stepped = scalar_curve(0x30, &[0.0, 1.0], &[2.0, 6.0])
            .compile_native_unmodified()
            .unwrap();
        assert_eq!(stepped.sample(0.9).unwrap()[0], 2.0);
        let linear = scalar_curve(0x1040, &[0.0, 1.0], &[2.0, 6.0])
            .compile_native_unmodified()
            .unwrap();
        assert_eq!(linear.sample(1.5).unwrap()[0], 4.0);
        assert!(linear.sample(f32::NAN).is_none());
        for x in [0.0, 0.25, 0.5, 0.75, 1.0] {
            assert!(
                (hermite_angle_fraction(x, [std::f32::consts::FRAC_PI_4; 2]) - x).abs() < 0.000001
            );
        }
        assert_eq!(hermite_angle_fraction(0.0, [0.0; 2]), 0.0);
        assert_eq!(hermite_angle_fraction(1.0, [0.0; 2]), 1.0);
        let mut cubic = scalar_curve(0x60, &[0.0, 1.0], &[2.0, 6.0]);
        cubic
            .values
            .extend([std::f32::consts::FRAC_PI_4.to_bits(); 4]);
        assert!(
            (cubic
                .compile_native_unmodified()
                .unwrap()
                .sample(0.5)
                .unwrap()[0]
                - 4.0)
                .abs()
                < 0.000001
        );
        cubic.modifiers = 0..1;
        assert_eq!(
            cubic.compile_native_unmodified().unwrap_err(),
            SamplingError::UnsupportedModifiers
        );
    }

    fn scalar_curve(tag: u32, times: &[f32], values: &[f32]) -> Property {
        let mut payload = vec![2, 0.0f32.to_bits(), 1.0f32.to_bits()];
        payload.extend(times.iter().map(|v| v.to_bits()));
        payload.extend(values.iter().map(|v| v.to_bits()));
        Property {
            tag,
            evaluator: 0,
            values: payload,
            modifiers: 0..0,
        }
    }

    #[test]
    fn samples_linear_and_stepped_keys_and_holds_endpoints() {
        let linear = scalar_curve(0x40, &[0.0, 0.5, 1.0], &[0.0, 1.0, 0.0])
            .compile_unmodified(Interpolation::Linear)
            .unwrap();
        assert_eq!(linear.argument(), Some(2));
        assert_eq!(linear.dimensions(), 1);
        for (argument, expected) in [
            (-1.0, 0.0),
            (0.25, 0.5),
            (0.5, 1.0),
            (0.75, 0.5),
            (2.0, 0.0),
        ] {
            assert_eq!(linear.sample(argument).unwrap()[0], expected);
        }
        assert_eq!(linear.sample(f32::NAN), None);
        let stepped = scalar_curve(0x30, &[0.0, 0.5, 1.0], &[0.0, 1.0, 0.0])
            .compile_unmodified(Interpolation::Stepped)
            .unwrap();
        assert_eq!(stepped.sample(0.499).unwrap()[0], 0.0);
        assert_eq!(stepped.sample(0.5).unwrap()[0], 1.0);
        assert_eq!(stepped.sample(0.999).unwrap()[0], 1.0);
        assert_eq!(stepped.sample(1.0).unwrap()[0], 0.0);
    }

    #[test]
    fn property_sampling_rejects_unsupported_and_malformed_payloads() {
        let error = |p: Property| p.compile_unmodified(Interpolation::Linear).unwrap_err();
        assert_eq!(
            error(scalar_curve(0x1040, &[0.0, 1.0], &[0.0, 1.0])),
            SamplingError::UnsupportedLoop
        );
        assert_eq!(
            error(scalar_curve(0x60, &[0.0, 1.0], &[0.0, 1.0])),
            SamplingError::UnsupportedEvaluator
        );
        for times in [[1.0, 0.0], [0.0, 0.0], [0.0, f32::NAN]] {
            assert_eq!(
                error(scalar_curve(0x40, &times, &[0.0, 1.0])),
                SamplingError::InvalidPayload
            );
        }
        assert_eq!(
            error(scalar_curve(0x40, &[0.0, 1.0], &[0.0, f32::INFINITY])),
            SamplingError::InvalidPayload
        );
        let mut modified = scalar_curve(0x40, &[0.0, 1.0], &[0.0, 1.0]);
        modified.modifiers = 0..1;
        assert_eq!(error(modified), SamplingError::UnsupportedModifiers);
    }

    #[test]
    fn samples_all_constant_dimensions_without_losing_colour_alpha() {
        for dimension in 1..=4 {
            let property = Property {
                tag: 0x20 + dimension as u32 - 1,
                evaluator: 0,
                values: [0.1f32, 0.2, 0.3, 0.4][..dimension]
                    .iter()
                    .map(|v| v.to_bits())
                    .collect(),
                modifiers: 0..0,
            };
            let compiled = property.compile_unmodified(Interpolation::Linear).unwrap();
            assert_eq!(compiled.dimensions(), dimension);
            assert_eq!(compiled.argument(), None);
            assert_eq!(
                compiled.sample(0.5).unwrap()[..dimension],
                [0.1, 0.2, 0.3, 0.4][..dimension]
            );
        }
    }

    #[test]
    fn blend_operations_preserve_non_alpha_modes_and_unknowns() {
        for id in [0, 4, 7] {
            assert_eq!(Blend::from_raw(id), Some(Blend::Additive));
        }
        assert_eq!(Blend::from_raw(1), Some(Blend::Source));
        assert_eq!(Blend::from_raw(2), Some(Blend::Alpha));
        assert_eq!(Blend::from_raw(6), Some(Blend::Alpha));
        assert_eq!(Blend::from_raw(3), Some(Blend::Multiply));
        assert_eq!(Blend::from_raw(5), Some(Blend::Subtract));
        assert_eq!(Blend::from_raw(8), None);
        assert_eq!(Blend::from_raw(u32::MAX), None);
    }

    #[test]
    fn rejects_truncated_headers_and_unsupported_versions() {
        for length in 0..144 {
            let mut data = vec![0; length];
            if length >= 12 {
                data[..4].copy_from_slice(b"FXR\0");
                data[4..8].copy_from_slice(&0x0005_0000u32.to_le_bytes());
                data[8..12].copy_from_slice(&1u32.to_le_bytes());
            }
            assert!(parse(&data).is_err(), "length {length}");
        }
        assert!(parse(&[0; 144]).is_err());
    }

    #[test]
    fn table_references_reject_misalignment_and_overrun() {
        let table = Table {
            offset: 144,
            count: 3,
            stride: 48,
        };
        assert_eq!(reference(&table, 192, 2).unwrap(), 1..3);
        assert!(reference(&table, 143, 1).is_err());
        assert!(reference(&table, 145, 1).is_err());
        assert!(reference(&table, 240, 2).is_err());
    }

    #[test]
    fn rejects_links_outside_the_value_pool_and_full_width_bad_offsets() {
        let mut bytes = vec![0; 260];
        bytes[..4].copy_from_slice(b"FXR\0");
        for (at, value) in [
            (4, 0x0005_0000u32),
            (8, 1),
            (12, 42),
            (40, 144),
            (44, 1), // node table
            (56, 192),
            (60, 1), // action table
            (96, 256),
            (100, 1), // raw-value pool
            (144, 0x0100_07d0),
            (156, 1),
            (176, 192),
            (192, 0x0100_0023),
            (200, 1),
            (224, 256),
            (256, 0x3f80_0000),
        ] {
            bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
        }
        let effect = parse(&bytes).unwrap();
        assert_eq!(effect.nodes[0].actions, 0..1);
        assert_eq!(effect.actions[0].fields[0], [0x3f80_0000]);
        let mut bad = bytes.clone();
        bad[224..232].copy_from_slice(&0u64.to_le_bytes());
        assert!(parse(&bad).is_err());
        let mut bad = bytes.clone();
        bad[224..232].copy_from_slice(&(256u64 + (1 << 32)).to_le_bytes());
        assert!(parse(&bad).is_err());
        let mut bad = bytes;
        bad[200..204].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(parse(&bad).is_err());
    }

    #[test]
    fn decodes_curve_arrays_without_discarding_cubic_controls() {
        let property = |tag, values: &[f32]| Property {
            tag,
            evaluator: 0,
            values: values.iter().map(|v| v.to_bits()).collect(),
            modifiers: 0..0,
        };
        let linear = property(
            0x0100_1040,
            &[0.0, -4.0, 12.0, 0.0, 2.0, 7.0, 12.0, 3.0, -4.0],
        );
        let curve = linear.curve().unwrap();
        assert_eq!(curve.times, [0.0, 2.0, 7.0]);
        assert_eq!(curve.values, [vec![12.0], vec![3.0], vec![-4.0]]);
        assert!(curve.looping);
        assert_eq!(curve.function, 4);
        let cubic = property(
            0x0100_0060,
            &[0.0, 2.0, 20.0, 0.0, 3.0, 2.0, 20.0, 0.2, 0.4, 0.6, 0.8],
        );
        let curve = cubic.curve().unwrap();
        assert_eq!(curve.values, [vec![2.0], vec![20.0]]);
        assert_eq!(curve.controls, [0.2, 0.4, 0.6, 0.8]);
        assert!(property(0x0100_0040, &[0.0; 4]).curve().is_none());
        assert_eq!(property(0x0100_0013, &[]).constant(), Some(vec![1.0; 4]));
    }
}
