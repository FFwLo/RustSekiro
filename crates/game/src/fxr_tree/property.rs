//! Typed action fields and curves, compiled once before particles use them.

use super::{Rnd, game_hermite, lerp4, num, vec4};
use serde_json::Value;
use std::ops::Index;

#[derive(Default)]
enum Base {
    #[default]
    Default,
    Constant([f32; 4]),
    Curve {
        kind: CurveKind,
        looping: bool,
        keys: Vec<Keyframe>,
    },
}

enum CurveKind {
    Linear,
    Stepped,
    Hermite,
    Bezier,
}

struct Keyframe {
    position: f32,
    value: [f32; 4],
    t1: [f32; 4],
    t2: [f32; 4],
    p1: [f32; 4],
    p2: [f32; 4],
}

enum Modifier {
    Delta([f32; 4]),
    Fraction([f32; 4]),
    Range([f32; 4], [f32; 4]),
}

#[derive(Default)]
struct Property {
    base: Base,
    modifiers: Vec<Modifier>,
}

impl Property {
    fn new(v: &Value) -> Self {
        if v.is_null() {
            return Self::default();
        }
        let base = if let Some(function) = v.get("function").and_then(Value::as_str) {
            Base::Curve {
                kind: match function {
                    "Stepped" => CurveKind::Stepped,
                    "Hermite" => CurveKind::Hermite,
                    "Bezier" => CurveKind::Bezier,
                    _ => CurveKind::Linear,
                },
                looping: v["loop"].as_bool() == Some(true),
                keys: v["keyframes"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|k| Keyframe {
                        position: num(&k["position"]),
                        value: vec4(&k["value"]),
                        t1: vec4(&k["t1"]),
                        t2: vec4(&k["t2"]),
                        p1: vec4(&k["p1"]),
                        p2: vec4(&k["p2"]),
                    })
                    .collect(),
            }
        } else {
            Base::Constant(vec4(v.get("value").unwrap_or(v)))
        };
        let modifiers = v
            .get("modifiers")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|m| match m["type"].as_str() {
                Some("RandomDelta") => Some(Modifier::Delta(vec4(&m["max"]))),
                Some("RandomFraction") => Some(Modifier::Fraction(vec4(&m["max"]))),
                Some("RandomRange") => Some(Modifier::Range(vec4(&m["min"]), vec4(&m["max"]))),
                _ => None,
            })
            .collect();
        Self { base, modifiers }
    }

    fn sample(&self, t: f32, rnd: &mut Rnd, default: [f32; 4]) -> [f32; 4] {
        let mut out = match &self.base {
            Base::Default => default,
            Base::Constant(value) => *value,
            Base::Curve {
                kind,
                looping,
                keys,
            } => {
                if let Some(last) = keys.last() {
                    let t = if *looping && last.position > 0.0 {
                        t.rem_euclid(last.position)
                    } else {
                        t
                    };
                    match keys.iter().rposition(|k| k.position <= t) {
                        None => keys[0].value,
                        Some(i) if i + 1 >= keys.len() => keys[i].value,
                        Some(i) => {
                            let (a, b) = (&keys[i], &keys[i + 1]);
                            let x = if b.position > a.position {
                                (t - a.position) / (b.position - a.position)
                            } else {
                                1.0
                            };
                            match kind {
                                CurveKind::Stepped => a.value,
                                CurveKind::Hermite => lerp4(
                                    a.value,
                                    b.value,
                                    std::array::from_fn(|c| game_hermite(a.t1[c], a.t2[c], x)),
                                ),
                                CurveKind::Bezier => std::array::from_fn(|c| {
                                    let (v1, v2) = (a.value[c], b.value[c]);
                                    let (c1, c2) = (v1 + a.p1[c] / 3.0, v2 - b.p2[c] / 3.0);
                                    let u = 1.0 - x;
                                    u * u * u * v1
                                        + 3.0 * u * u * x * c1
                                        + 3.0 * u * x * x * c2
                                        + x * x * x * v2
                                }),
                                CurveKind::Linear => lerp4(a.value, b.value, [x; 4]),
                            }
                        }
                    }
                } else {
                    default
                }
            }
        };
        for modifier in &self.modifiers {
            match modifier {
                Modifier::Delta(max) => {
                    for c in 0..4 {
                        out[c] += (rnd.next() * 2.0 - 1.0) * max[c];
                    }
                }
                Modifier::Fraction(max) => {
                    for c in 0..4 {
                        out[c] += out[c] * (rnd.next() * 2.0 - 1.0) * max[c];
                    }
                }
                Modifier::Range(min, max) => {
                    for c in 0..4 {
                        out[c] += min[c] + (max[c] - min[c]) * rnd.next();
                    }
                }
            }
        }
        out
    }
}

pub struct FieldValue {
    number: Option<f64>,
    integer: Option<i64>,
    boolean: Option<bool>,
    int_value: Option<i64>,
    pub vector: [f32; 4],
    property: Property,
}

impl FieldValue {
    pub fn new(v: &Value) -> Self {
        let int_value = match v {
            Value::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)),
            Value::Bool(b) => Some(*b as i64),
            _ => v.get("value").and_then(Value::as_f64).map(|f| f as i64),
        };
        Self {
            number: v.as_f64(),
            integer: v.as_i64(),
            boolean: v.as_bool(),
            int_value,
            vector: vec4(v),
            property: Property::new(v),
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        self.integer
    }

    pub fn as_f64(&self) -> Option<f64> {
        self.number
    }

    pub fn as_bool(&self) -> Option<bool> {
        self.boolean
    }
}

static MISSING: FieldValue = FieldValue {
    number: None,
    integer: None,
    boolean: None,
    int_value: None,
    vector: [0.0; 4],
    property: Property {
        base: Base::Default,
        modifiers: Vec::new(),
    },
};

pub struct Action {
    // Dense field ids address only the values present in this action.
    slots: Box<[u16]>,
    values: Vec<FieldValue>,
}

impl Action {
    pub fn new(v: &Value) -> Self {
        let mut slots = vec![u16::MAX; Field::ALL.len()].into_boxed_slice();
        let mut values = Vec::new();
        for (i, name) in Field::ALL.iter().enumerate() {
            if let Some(v) = v.get(*name) {
                slots[i] = values.len() as u16;
                values.push(FieldValue::new(v));
            }
        }
        Self { slots, values }
    }

    pub fn get(&self, key: Field) -> Option<&FieldValue> {
        self.values.get(self.slots[key as usize] as usize)
    }
}

impl Index<Field> for Action {
    type Output = FieldValue;

    fn index(&self, key: Field) -> &Self::Output {
        self.get(key).unwrap_or(&MISSING)
    }
}

pub fn prop(v: Option<&FieldValue>, t: f32, rnd: &mut Rnd, default: [f32; 4]) -> [f32; 4] {
    v.map_or(default, |v| v.property.sample(t, rnd, default))
}

pub fn scalar(a: &Action, key: Field, t: f32, rnd: &mut Rnd, default: f32) -> f32 {
    prop(a.get(key), t, rnd, [default; 4])[0]
}

pub fn int(a: &Action, key: Field, default: i64) -> i64 {
    a[key].int_value.unwrap_or(default)
}

macro_rules! fields {
    ($( $name:ident => $json:literal ),+ $(,)?) => {
        #[derive(Clone, Copy)]
        pub enum Field { $($name,)+ }
        impl Field {
            const ALL: &'static [&'static str] = &[$($json,)+];
        }
    };
}

fields! {
    Acceleration => "acceleration",
    AccelerationMultiplier => "accelerationMultiplier",
    AccelerationMultiplierZ => "accelerationMultiplierZ",
    AccelerationY => "accelerationY",
    AccelerationZ => "accelerationZ",
    AlphaFadeThreshold => "alphaFadeThreshold",
    AlphaMultiplier => "alphaMultiplier",
    AlphaThreshold => "alphaThreshold",
    Angle => "angle",
    AngleX => "angleX",
    AngleY => "angleY",
    AngularSpeedMultiplierZ => "angularSpeedMultiplierZ",
    AngularSpeedX => "angularSpeedX",
    AngularSpeedY => "angularSpeedY",
    AngularSpeedZ => "angularSpeedZ",
    Attachment => "attachment",
    BlendMode => "blendMode",
    Color => "color",
    Color1 => "color1",
    Color2 => "color2",
    Color3 => "color3",
    Columns => "columns",
    ConcurrentSegments => "concurrentSegments",
    Delay => "delay",
    DepthBlend => "depthBlend",
    DiffuseColor => "diffuseColor",
    DiffuseMultiplier => "diffuseMultiplier",
    Direction => "direction",
    Duration => "duration",
    EmissionIntervalMin => "emissionIntervalMin",
    EmissionParticleCount => "emissionParticleCount",
    EmitInside => "emitInside",
    EmitterRotation => "emitterRotation",
    EmitterSize => "emitterSize",
    EndFadeEndpoint => "endFadeEndpoint",
    FollowFactor => "followFactor",
    FrameIndex => "frameIndex",
    FrameIndexOffset => "frameIndexOffset",
    Gravity => "gravity",
    Height => "height",
    Intensity => "intensity",
    InterpolateFrames => "interpolateFrames",
    Interval => "interval",
    Layer1 => "layer1",
    Layer1Color => "layer1Color",
    Layer1OffsetU => "layer1OffsetU",
    Layer1OffsetV => "layer1OffsetV",
    Layer1ScaleU => "layer1ScaleU",
    Layer1ScaleV => "layer1ScaleV",
    Layer1SpeedU => "layer1SpeedU",
    Layer1SpeedV => "layer1SpeedV",
    Layer2 => "layer2",
    Layer2Color => "layer2Color",
    Layer2OffsetU => "layer2OffsetU",
    Layer2OffsetV => "layer2OffsetV",
    Layer2ScaleU => "layer2ScaleU",
    Layer2ScaleV => "layer2ScaleV",
    Layer2SpeedU => "layer2SpeedU",
    Layer2SpeedV => "layer2SpeedV",
    Layer3 => "layer3",
    Layer3Color => "layer3Color",
    Layer3OffsetU => "layer3OffsetU",
    Layer3OffsetV => "layer3OffsetV",
    Layer3ScaleU => "layer3ScaleU",
    Layer3ScaleV => "layer3ScaleV",
    Layer3SpeedU => "layer3SpeedU",
    Layer3SpeedV => "layer3SpeedV",
    Length => "length",
    LengthMultiplier => "lengthMultiplier",
    Lighting => "lighting",
    MaxConcurrent => "maxConcurrent",
    MaxDistance => "maxDistance",
    MaxDistanceThreshold => "maxDistanceThreshold",
    MaxFadeDistance => "maxFadeDistance",
    MaxTurnAngle => "maxTurnAngle",
    MaxViewDistance => "maxViewDistance",
    MinDistance => "minDistance",
    MinDistanceThreshold => "minDistanceThreshold",
    MinFadeDistance => "minFadeDistance",
    NormalMap => "normalMap",
    NormalMapOffsetU => "normalMapOffsetU",
    NormalMapOffsetV => "normalMapOffsetV",
    NormalMapScaleU => "normalMapScaleU",
    NormalMapScaleV => "normalMapScaleV",
    NormalMapSpeedU => "normalMapSpeedU",
    NormalMapSpeedV => "normalMapSpeedV",
    Offset => "offset",
    OffsetVariance => "offsetVariance",
    Orientation => "orientation",
    ParticleDuration => "particleDuration",
    ParticleDurationMultiplier => "particleDurationMultiplier",
    ParticleFollowFactor => "particleFollowFactor",
    ParticleSizeMultiplier => "particleSizeMultiplier",
    ParticleSizeX => "particleSizeX",
    ParticleSizeXMax => "particleSizeXMax",
    ParticleSizeXMin => "particleSizeXMin",
    ParticleSpeed => "particleSpeed",
    ParticleSpeedMax => "particleSpeedMax",
    ParticleSpeedMin => "particleSpeedMin",
    PerEmission => "perEmission",
    PremultiplyAlpha => "premultiplyAlpha",
    Radius => "radius",
    RgbMultiplier => "rgbMultiplier",
    Rotation => "rotation",
    RotationVariance => "rotationVariance",
    RotationZ => "rotationZ",
    ScaleX => "scaleX",
    ScaleY => "scaleY",
    SegmentDuration => "segmentDuration",
    SegmentInterval => "segmentInterval",
    SegmentSubdivision => "segmentSubdivision",
    Size => "size",
    SizeX => "sizeX",
    SizeY => "sizeY",
    SizeZ => "sizeZ",
    Speed => "speed",
    SpeedMultiplier => "speedMultiplier",
    SpeedMultiplierZ => "speedMultiplierZ",
    SpeedZ => "speedZ",
    StartFadeEndpoint => "startFadeEndpoint",
    Texture => "texture",
    Threshold => "threshold",
    TotalEmissions => "totalEmissions",
    TotalFrames => "totalFrames",
    Translation => "translation",
    TurnInterval => "turnInterval",
    Type => "type",
    UniformScale => "uniformScale",
    UnkDepthBlend1 => "unkDepthBlend1",
    UnkDepthBlend2 => "unkDepthBlend2",
    UnkDs3F2_10 => "unk_ds3_f2_10",
    UnkDs3F2_11 => "unk_ds3_f2_11",
    UnkDs3F2_12 => "unk_ds3_f2_12",
    UnkDs3F2_13 => "unk_ds3_f2_13",
    UnkDs3P2_0 => "unk_ds3_p2_0",
    Width => "width",
    WidthMultiplier => "widthMultiplier",
    YAxis => "yAxis",
}
