//! A parsed FXR as a named tree (JSON values), for players that want the authored fields by name.
//!
//! The shape follows the object model of EvenTorset's FXR library (Unlicense) and FwLo's
//! RustSekiro player (MIT), which reads effects in this form: a node is
//! `{type, configs: [...], stateConfigMap: [...], nodes: [...], sfx?}`, a config is
//! `{type, <slot>: <action>...}` with the slot names in lower camel case (`nodeAttributes`,
//! `emitter`, `emitterShape`, `directionSpread`, `particleAttributes`, `particleMovement`,
//! `particleModifier`, `appearance`, ...), and an action is `{type, <field>: <value>...}`.
//! A property is a plain number or array when constant, else
//! `{function, loop, keyframes: [{position, value, t1, t2, p1, p2}], modifiers: [...]}`.
//!
//! Field and property names come from [`crate::fxr_schema`], generated from the library's
//! Sekiro layouts. Properties the reader cannot decode (component sequences, external-value
//! functions) are left out, so a player falls back to the field's default.

use serde_json::{Map, Value, json};

use crate::fxr::{Action, Fxr, Modifier, Property};
use crate::fxr_schema::{self, ActionSchema};

/// What the conversion could not represent; for diagnostics and tests.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Report {
    /// Actions with no schema entry.
    pub unknown_actions: Vec<u16>,
    /// Properties left out (unsupported evaluator).
    pub skipped_properties: usize,
    /// Actions whose field or property counts disagree with the schema.
    pub layout_mismatches: Vec<u16>,
}

/// The effect as a tree rooted at node 0.
pub fn to_json(fxr: &Fxr) -> (Value, Report) {
    let mut report = Report::default();
    let root = if fxr.nodes.is_empty() {
        Value::Null
    } else {
        node(fxr, 0, &mut report, 0)
    };
    (json!({ "id": fxr.id, "root": root }), report)
}

fn lower_camel(slot: &str) -> String {
    let mut out = slot.to_string();
    if let Some(first) = out.get_mut(0..1) {
        first.make_ascii_lowercase();
    }
    out
}

fn node(fxr: &Fxr, index: usize, report: &mut Report, depth: usize) -> Value {
    let n = &fxr.nodes[index];
    let mut obj = Map::new();
    obj.insert("type".into(), json!(n.tag as u16));
    for action in &fxr.actions[n.actions.clone()] {
        match action.id() {
            // A reference to another effect's root.
            132 => {
                if let Some(&sfx) = action.fields[0].first() {
                    obj.insert("sfx".into(), json!(sfx as i32));
                }
            }
            // State -> config index.
            199 => {
                let map: Vec<u32> = fxr
                    .sequences
                    .get(action.sequences.start)
                    .filter(|_| !action.sequences.is_empty())
                    .map(|s| s.values.clone())
                    .unwrap_or_default();
                obj.insert("stateConfigMap".into(), json!(map));
            }
            _ => {}
        }
    }
    let configs: Vec<Value> = fxr.configurations[n.configurations.clone()]
        .iter()
        .map(|c| {
            let mut cfg = Map::new();
            cfg.insert("type".into(), json!(c.tag as u16));
            for action in &fxr.actions[c.actions.clone()] {
                let (slot, value) = action_json(fxr, action, report);
                cfg.insert(slot, value);
            }
            Value::Object(cfg)
        })
        .collect();
    obj.insert("configs".into(), Value::Array(configs));
    if depth < 24 {
        let kids: Vec<Value> = n
            .children
            .clone()
            .filter(|&c| c < fxr.nodes.len() && c != index)
            .map(|c| node(fxr, c, report, depth + 1))
            .collect();
        obj.insert("nodes".into(), Value::Array(kids));
    }
    Value::Object(obj)
}

/// An action as (slot key, object).
pub fn action_json(fxr: &Fxr, action: &Action, report: &mut Report) -> (String, Value) {
    let id = action.id();
    let mut obj = Map::new();
    obj.insert("type".into(), json!(id));
    let Some(schema) = fxr_schema::action(id) else {
        if id != 0 {
            report.unknown_actions.push(id);
        }
        return (format!("action{id}"), Value::Object(obj));
    };
    fields(schema, action, &mut obj, report);
    for (section, names) in schema.properties.iter().enumerate() {
        let range = &action.properties[section];
        // Files may end earlier than the schema (its tail covers later game versions).
        if range.len() > names.len() {
            report.layout_mismatches.push(id);
        }
        for (i, name) in names.iter().enumerate() {
            let Some(p) = range.clone().nth(i).and_then(|k| fxr.properties.get(k)) else {
                break;
            };
            match property(fxr, p) {
                Some(v) => {
                    obj.insert((*name).into(), v);
                }
                None => report.skipped_properties += 1,
            }
        }
    }
    (lower_camel(schema.slot), Value::Object(obj))
}

fn fields(
    schema: &ActionSchema,
    action: &Action,
    obj: &mut Map<String, Value>,
    report: &mut Report,
) {
    for (section, list) in schema.fields.iter().enumerate() {
        let words = &action.fields[section];
        let mut at = 0;
        for (name, kind) in list.iter() {
            let size = match kind {
                3 => 3,
                4 => 4,
                _ => 1,
            };
            let Some(w) = words.get(at..at + size) else {
                break;
            };
            at += size;
            let float = |b: u32| json_f32(f32::from_bits(b));
            let value = match kind {
                1 => float(w[0]),
                2 => json!(w[0] != 0),
                3 | 4 => Value::Array(w.iter().map(|&b| float(b)).collect()),
                _ => json!(w[0] as i32),
            };
            obj.insert((*name).into(), value);
        }
        if at < words.len() {
            report.layout_mismatches.push(schema.id);
        }
    }
}

fn json_f32(v: f32) -> Value {
    if v.is_finite() {
        // f32 -> f64 keeps the exact value; round-trips through serde_json.
        json!(f64::from(v))
    } else {
        json!(0.0)
    }
}

fn value_of(lanes: &[f32]) -> Value {
    if lanes.len() == 1 {
        json_f32(lanes[0])
    } else {
        Value::Array(lanes.iter().map(|&v| json_f32(v)).collect())
    }
}

fn property(fxr: &Fxr, p: &Property) -> Option<Value> {
    let mods: Vec<Value> = fxr.modifiers[p.modifiers.clone()]
        .iter()
        .filter_map(modifier)
        .collect();
    if let Some(c) = p.constant() {
        let v = value_of(&c);
        return Some(if mods.is_empty() {
            v
        } else {
            json!({ "value": v, "modifiers": mods })
        });
    }
    let curve = p.curve()?;
    let dims = curve.values.first()?.len();
    let function = match curve.function {
        3 => "Stepped",
        4 => "Linear",
        5 => "Bezier",
        6 => "Hermite",
        _ => return None,
    };
    let keys: Vec<Value> = curve
        .times
        .iter()
        .enumerate()
        .map(|(k, &t)| {
            let mut key = Map::new();
            key.insert("position".into(), json_f32(t));
            key.insert("value".into(), value_of(&curve.values[k]));
            if !curve.controls.is_empty() {
                let first = &curve.controls[(k * 2) * dims..(k * 2 + 1) * dims];
                let second = &curve.controls[(k * 2 + 1) * dims..(k * 2 + 2) * dims];
                for (a, b) in [("t1", "p1"), ("t2", "p2")] {
                    let src = if a == "t1" { first } else { second };
                    key.insert(a.into(), value_of(src));
                    key.insert(b.into(), value_of(src));
                }
            }
            Value::Object(key)
        })
        .collect();
    let mut obj = Map::new();
    obj.insert("function".into(), json!(function));
    obj.insert("loop".into(), json!(curve.looping));
    obj.insert("keyframes".into(), Value::Array(keys));
    if !mods.is_empty() {
        obj.insert("modifiers".into(), Value::Array(mods));
    }
    Some(Value::Object(obj))
}

/// Random modifiers: RandomDelta 21, RandomRange 24, RandomFraction 53 (seeds then the bounds,
/// one word each per component). Others (external values) are left out.
fn modifier(m: &Modifier) -> Option<Value> {
    let kind = ((m.tag >> 12) & 3) << 4 | ((m.tag >> 4) & 15);
    let dims = (m.tag & 15) as usize + 1;
    let floats = |from: usize| -> Option<Vec<f32>> {
        m.values
            .get(from..from + dims)
            .map(|w| w.iter().map(|&b| f32::from_bits(b)).collect())
    };
    match kind {
        21 | 53 => Some(json!({
            "type": if kind == 21 { "RandomDelta" } else { "RandomFraction" },
            "max": value_of(&floats(dims)?),
        })),
        24 => Some(json!({
            "type": "RandomRange",
            "min": value_of(&floats(dims)?),
            "max": value_of(&floats(dims * 2)?),
        })),
        _ => None,
    }
}
