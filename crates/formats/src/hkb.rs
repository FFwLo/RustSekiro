//! Havok behaviour graphs (`hkbBehaviorGraph`) from `chr/*.behbnd`.
//!
//! A behaviour graph is a tree of generators: state machines whose states each own a generator,
//! layer and blender generators that mix several children, selectors that pick one child, and
//! clip generators that play one animation. FromSoftware adds its own node types, most
//! importantly `CustomManualSelectorGenerator` (CMSG), which picks the clip for an animation id
//! by an "offset" (`aXXX_` prefix) the engine derives from equipment, and whose state hooks call
//! into HKS.
//!
//! This module turns the reflected objects from [`crate::hkx::tagfile`] into a plain data model:
//! every generator becomes a [`Node`] in one arena, transitions reference events by index into
//! [`Graph::events`], and variables carry their declared type and initial value.

use crate::hkx::tagfile::{Kind, TagFile, Value};
use crate::reader::{Error, Result, bail};
use std::collections::HashMap;

/// Index into [`Graph::nodes`].
pub type NodeId = usize;
/// Index into [`Graph::effects`].
pub type EffectId = usize;

/// The declared type of a behaviour variable (Havok's `hkbVariableInfo::VariableType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VariableType {
    Bool,
    Int8,
    Int16,
    Int32,
    Real,
    Pointer,
    Vector3,
    Vector4,
    Quaternion,
    Unknown(i64),
}

impl VariableType {
    fn from_raw(v: i64) -> Self {
        match v {
            0 => Self::Bool,
            1 => Self::Int8,
            2 => Self::Int16,
            3 => Self::Int32,
            4 => Self::Real,
            5 => Self::Pointer,
            6 => Self::Vector3,
            7 => Self::Vector4,
            8 => Self::Quaternion,
            other => Self::Unknown(other),
        }
    }
}

/// The initial value of a variable, decoded by its type.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VariableValue {
    Int(i32),
    Real(f32),
    Quad([f32; 4]),
    Pointer(i32),
}

impl VariableValue {
    /// The value as a script number (booleans and integers widen, quads take `x`).
    pub fn as_f32(&self) -> f32 {
        match *self {
            Self::Int(i) | Self::Pointer(i) => i as f32,
            Self::Real(r) => r,
            Self::Quad(q) => q[0],
        }
    }
}

#[derive(Debug, Clone)]
pub struct Variable {
    pub name: String,
    pub ty: VariableType,
    pub initial: VariableValue,
    /// Raw `hkbVariableBounds` words (min, max), decoded like the value.
    pub bounds: (VariableValue, VariableValue),
}

/// A node member driven by a behaviour variable (`hkbVariableBindingSet::Binding`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding {
    /// Member path on the bound node, such as `selectedGeneratorIndex` or `enable`.
    pub member_path: String,
    pub variable: i32,
    /// 0 binds a behaviour variable, 1 a character property.
    pub binding_type: i64,
    /// For boolean members packed into bit fields; -1 otherwise.
    pub bit_index: i64,
}

/// A transition from one state to another, or a wildcard transition into a state.
#[derive(Debug, Clone, PartialEq)]
pub struct Transition {
    /// Index into [`Graph::events`]; -1 for an eventless transition.
    pub event: i32,
    pub to_state: i32,
    pub from_nested_state: i32,
    pub to_nested_state: i32,
    pub priority: i64,
    /// `hkbStateMachine::TransitionInfo::TransitionFlags`, see [`transition_flags`].
    pub flags: u32,
    pub effect: Option<EffectId>,
    /// Whether a condition object is attached (FromSoftware graphs leave this empty).
    pub has_condition: bool,
}

/// Bits of [`Transition::flags`], as named by Havok.
pub mod transition_flags {
    pub const USE_TRIGGER_INTERVAL: u32 = 0x1;
    pub const USE_INITIATE_INTERVAL: u32 = 0x2;
    pub const UNINTERRUPTIBLE_WHILE_PLAYING: u32 = 0x4;
    pub const UNINTERRUPTIBLE_WHILE_DELAYED: u32 = 0x8;
    pub const DELAY_STATE_CHANGE: u32 = 0x10;
    pub const DISABLED: u32 = 0x20;
    pub const DISALLOW_RETURN_TO_PREVIOUS_STATE: u32 = 0x40;
    pub const DISALLOW_RANDOM_TRANSITION: u32 = 0x80;
    pub const DISABLE_CONDITION: u32 = 0x100;
    pub const ALLOW_SELF_TRANSITION_BY_TRANSITION_FROM_ANY_STATE: u32 = 0x200;
    pub const IS_GLOBAL_WILDCARD: u32 = 0x400;
    pub const IS_LOCAL_WILDCARD: u32 = 0x800;
    pub const FROM_NESTED_STATE_ID_IS_VALID: u32 = 0x1000;
    pub const TO_NESTED_STATE_ID_IS_VALID: u32 = 0x2000;
    pub const ABUT_AT_END_OF_FROM_GENERATOR: u32 = 0x4000;
}

/// How a transition blends from the old generator to the new one.
#[derive(Debug, Clone, PartialEq)]
pub struct TransitionEffect {
    pub type_name: String,
    pub name: String,
    pub bindings: Vec<Binding>,
    /// Blend duration in seconds, for blending effects.
    pub duration: Option<f32>,
    pub self_transition_mode: i64,
    pub event_mode: i64,
    pub end_mode: Option<i64>,
    pub blend_curve: Option<i64>,
    pub flags: Option<i64>,
    pub to_generator_start_time_fraction: Option<f32>,
    /// For `hkbManualSelectorTransitionEffect`: the candidate effects and the default index
    /// (usually bound to a variable).
    pub choices: Vec<Option<EffectId>>,
    pub selected_index: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct State {
    pub name: String,
    pub id: i32,
    pub generator: Option<NodeId>,
    pub transitions: Vec<Transition>,
    pub probability: f32,
    pub enable: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StateMachine {
    pub start_state_id: i32,
    pub states: Vec<State>,
    /// Transitions that can be taken from any state of this machine.
    pub wildcard_transitions: Vec<Transition>,
    pub return_to_previous_state_event: i32,
    pub self_transition_mode: i64,
    /// `hkbStateMachine::StartStateMode`: 0 uses `start_state_id`, 1 (sync) starts in the state
    /// whose id is stored in `sync_variable`, which every machine sharing it keeps up to date.
    pub start_state_mode: i64,
    /// Behaviour variable that machines in sync mode share (-1 for none).
    pub sync_variable: i32,
}

impl StateMachine {
    pub fn state_index(&self, id: i32) -> Option<usize> {
        self.states.iter().position(|s| s.id == id)
    }
}

/// FromSoftware's `CustomManualSelectorGenerator`: one state's animation.
#[derive(Debug, Clone, PartialEq)]
pub struct Cmsg {
    /// Candidate clips (or nested generators); the engine picks by animation offset.
    pub generators: Vec<Option<NodeId>>,
    /// Which equipment-derived offset selects the `aXXX_` clip prefix.
    pub offset_type: i64,
    /// The six-digit animation id within the offset, such as 203000.
    pub anim_id: i64,
    pub anime_end_event_type: i64,
    /// Whether the HKS state hooks run for this node.
    pub enable_script: bool,
    pub enable_tae: bool,
    pub change_type_of_selected_index_after_activate: i64,
    pub generator_changed_effect: Option<EffectId>,
    pub check_anim_end_slot: i64,
    pub end_event: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ManualSelector {
    pub generators: Vec<Option<NodeId>>,
    /// Default index; usually driven by a binding on `selectedGeneratorIndex`.
    pub selected_index: i64,
    pub index_can_change_after_activate: bool,
    pub generator_changed_effect: Option<EffectId>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Clip {
    /// Animation name such as `a050_203000`.
    pub animation: String,
    /// `hkbClipGenerator::PlaybackMode`: 0 single play, 1 looping, 2 user controlled,
    /// 3 ping pong, 4 count.
    pub mode: i64,
    pub playback_speed: f32,
    pub start_time: f32,
    pub crop_start: f32,
    pub crop_end: f32,
    pub enforced_duration: f32,
    pub user_controlled_time_fraction: f32,
    pub flags: i64,
}

impl Clip {
    /// The animation id as `(offset, id)`, parsed from `aOOO_IIIIII`.
    pub fn anim_id(&self) -> Option<(u32, u32)> {
        parse_anim_name(&self.animation)
    }
}

/// Parses `aOOO_IIIIII` (with an optional suffix) into `(offset, id)`.
pub fn parse_anim_name(name: &str) -> Option<(u32, u32)> {
    let rest = name.strip_prefix('a')?;
    let offset = rest.get(..3)?.parse().ok()?;
    let id = rest.get(4..10)?.parse().ok()?;
    (rest.as_bytes().get(3) == Some(&b'_')).then_some((offset, id))
}

#[derive(Debug, Clone, PartialEq)]
pub struct Layer {
    pub generator: Option<NodeId>,
    pub bindings: Vec<Binding>,
    pub weight: f32,
    pub on_by_default: bool,
    pub on_event: i32,
    pub off_event: i32,
    pub fade_in: f32,
    pub fade_out: f32,
    pub use_motion: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BlendChild {
    pub generator: Option<NodeId>,
    pub bindings: Vec<Binding>,
    pub weight: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Script {
    pub child: Option<NodeId>,
    pub on_activate: String,
    pub on_pre_update: String,
    pub on_generate: String,
    pub on_handle_event: String,
    pub on_deactivate: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum NodeKind {
    StateMachine(StateMachine),
    Cmsg(Cmsg),
    ManualSelector(ManualSelector),
    Clip(Clip),
    Layers(Vec<Layer>),
    Blender {
        blend_parameter: f32,
        children: Vec<BlendChild>,
    },
    Script(Script),
    Modifier {
        child: Option<NodeId>,
    },
    /// `hkbBehaviorReferenceGenerator`: runs another behaviour file, e.g. `Behaviors\c9997`
    /// (the shared NPC graph that `c1010.hkx` wraps).
    BehaviorReference {
        behavior_name: String,
    },
    /// Any other generator; child generators found by walking pointer members.
    Other {
        children: Vec<NodeId>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub type_name: String,
    pub name: String,
    pub user_data: i64,
    pub bindings: Vec<Binding>,
    pub kind: NodeKind,
}

impl Node {
    /// Direct child generators, in declaration order.
    pub fn children(&self) -> Vec<NodeId> {
        let flat = |v: &[Option<NodeId>]| v.iter().flatten().copied().collect();
        match &self.kind {
            NodeKind::StateMachine(sm) => sm.states.iter().filter_map(|s| s.generator).collect(),
            NodeKind::Cmsg(c) => flat(&c.generators),
            NodeKind::ManualSelector(m) => flat(&m.generators),
            NodeKind::Clip(_) => Vec::new(),
            NodeKind::Layers(l) => l.iter().filter_map(|l| l.generator).collect(),
            NodeKind::Blender { children, .. } => {
                children.iter().filter_map(|c| c.generator).collect()
            }
            NodeKind::Script(s) => s.child.into_iter().collect(),
            NodeKind::Modifier { child } => child.iter().copied().collect(),
            NodeKind::BehaviorReference { .. } => Vec::new(),
            NodeKind::Other { children } => children.clone(),
        }
    }
}

/// A parsed behaviour graph.
#[derive(Debug, Clone)]
pub struct Graph {
    pub name: String,
    pub events: Vec<String>,
    pub event_flags: Vec<i64>,
    pub variables: Vec<Variable>,
    pub animation_names: Vec<String>,
    pub nodes: Vec<Node>,
    pub effects: Vec<TransitionEffect>,
    pub root: NodeId,
}

impl Graph {
    pub fn event_index(&self, name: &str) -> Option<usize> {
        self.events.iter().position(|e| e == name)
    }

    pub fn variable_index(&self, name: &str) -> Option<usize> {
        self.variables.iter().position(|v| v.name == name)
    }

    pub fn node_by_name(&self, name: &str) -> Option<NodeId> {
        self.nodes.iter().position(|n| n.name == name)
    }

    /// Every state machine node id.
    pub fn state_machines(&self) -> impl Iterator<Item = (NodeId, &StateMachine)> {
        self.nodes
            .iter()
            .enumerate()
            .filter_map(|(i, n)| match &n.kind {
                NodeKind::StateMachine(sm) => Some((i, sm)),
                _ => None,
            })
    }

    /// An indented outline of the tree from `node`, for debugging dumps.
    pub fn outline(&self, node: NodeId, max_depth: usize) -> String {
        let mut out = String::new();
        let mut seen = vec![false; self.nodes.len()];
        self.outline_into(node, 0, max_depth, &mut seen, &mut out);
        out
    }

    fn outline_into(
        &self,
        id: NodeId,
        depth: usize,
        max_depth: usize,
        seen: &mut [bool],
        out: &mut String,
    ) {
        use std::fmt::Write;
        let n = &self.nodes[id];
        let pad = "  ".repeat(depth);
        let detail = match &n.kind {
            NodeKind::Clip(c) => format!(
                " anim={} mode={} speed={}",
                c.animation, c.mode, c.playback_speed
            ),
            NodeKind::Cmsg(c) => format!(
                " anim_id={} offset_type={} end_type={} script={}",
                c.anim_id, c.offset_type, c.anime_end_event_type, c.enable_script
            ),
            NodeKind::StateMachine(sm) => format!(
                " start={} states={} wildcards={}",
                sm.start_state_id,
                sm.states.len(),
                sm.wildcard_transitions.len()
            ),
            _ => String::new(),
        };
        let _ = writeln!(out, "{pad}{} {:?}{detail}", n.type_name, n.name);
        if seen[id] {
            let _ = writeln!(out, "{pad}  (shared, see above)");
            return;
        }
        seen[id] = true;
        if depth >= max_depth {
            return;
        }
        if let NodeKind::StateMachine(sm) = &n.kind {
            for s in &sm.states {
                let _ = writeln!(out, "{pad}  state {} {:?}", s.id, s.name);
                if let Some(g) = s.generator {
                    self.outline_into(g, depth + 2, max_depth, seen, out);
                }
            }
            return;
        }
        if let NodeKind::Layers(layers) = &n.kind {
            let ev = |e: i32| {
                usize::try_from(e)
                    .ok()
                    .and_then(|e| self.events.get(e))
                    .map_or("-", String::as_str)
            };
            for l in layers {
                let _ = writeln!(
                    out,
                    "{pad}  layer on_by_default={} on={} off={} weight={} bindings={}",
                    l.on_by_default,
                    ev(l.on_event),
                    ev(l.off_event),
                    l.weight,
                    l.bindings.len()
                );
                if let Some(g) = l.generator {
                    self.outline_into(g, depth + 2, max_depth, seen, out);
                }
            }
            return;
        }
        for c in n.children() {
            self.outline_into(c, depth + 1, max_depth, seen, out);
        }
    }
}

struct Builder<'a> {
    nodes: Vec<Option<Node>>,
    by_offset: HashMap<usize, NodeId>,
    effects: Vec<TransitionEffect>,
    effect_by_offset: HashMap<usize, EffectId>,
    file: &'a TagFile,
}

fn str_of(v: &Value<'_>, name: &str) -> Result<String> {
    if v.has(name) {
        v.get(name)?.string()
    } else {
        Ok(String::new())
    }
}

fn int_of(v: &Value<'_>, name: &str) -> Result<i64> {
    v.get(name)?.int()
}

fn opt_int(v: &Value<'_>, name: &str) -> Result<Option<i64>> {
    if v.has(name) {
        Ok(Some(v.get(name)?.int()?))
    } else {
        Ok(None)
    }
}

fn f32_of(v: &Value<'_>, name: &str) -> Result<f32> {
    v.get(name)?.f32()
}

fn opt_f32(v: &Value<'_>, name: &str) -> Result<Option<f32>> {
    if v.has(name) {
        Ok(Some(v.get(name)?.f32()?))
    } else {
        Ok(None)
    }
}

fn bindings_of(v: &Value<'_>) -> Result<Vec<Binding>> {
    if !v.has("variableBindingSet") {
        return Ok(Vec::new());
    }
    let Some(set) = v.get("variableBindingSet")?.deref()? else {
        return Ok(Vec::new());
    };
    set.get("bindings")?
        .elements()?
        .iter()
        .map(|b| {
            Ok(Binding {
                member_path: b.get("memberPath")?.string()?,
                variable: b.get("variableIndex")?.int()? as i32,
                binding_type: b.get("bindingType")?.int()?,
                bit_index: b.get("bitIndex")?.int()?,
            })
        })
        .collect()
}

impl<'a> Builder<'a> {
    fn effect(&mut self, ptr: Value<'a>) -> Result<Option<EffectId>> {
        let Some(e) = ptr.deref()? else {
            return Ok(None);
        };
        if let Some(&id) = self.effect_by_offset.get(&e.offset) {
            return Ok(Some(id));
        }
        let id = self.effects.len();
        self.effect_by_offset.insert(e.offset, id);
        self.effects.push(TransitionEffect {
            type_name: e.type_name().to_owned(),
            name: str_of(&e, "name")?,
            bindings: Vec::new(),
            duration: None,
            self_transition_mode: 0,
            event_mode: 0,
            end_mode: None,
            blend_curve: None,
            flags: None,
            to_generator_start_time_fraction: None,
            choices: Vec::new(),
            selected_index: None,
        });
        let mut choices = Vec::new();
        if e.has("transitionEffects") {
            for c in e.get("transitionEffects")?.elements()? {
                choices.push(self.effect(c)?);
            }
        }
        let fx = TransitionEffect {
            type_name: e.type_name().to_owned(),
            name: str_of(&e, "name")?,
            bindings: bindings_of(&e)?,
            duration: opt_f32(&e, "duration")?,
            self_transition_mode: opt_int(&e, "selfTransitionMode")?.unwrap_or(0),
            event_mode: opt_int(&e, "eventMode")?.unwrap_or(0),
            end_mode: opt_int(&e, "endMode")?,
            blend_curve: opt_int(&e, "blendCurve")?,
            flags: opt_int(&e, "flags")?,
            to_generator_start_time_fraction: opt_f32(&e, "toGeneratorStartTimeFraction")?,
            choices,
            selected_index: opt_int(&e, "selectedIndex")?,
        };
        self.effects[id] = fx;
        Ok(Some(id))
    }

    fn transitions(&mut self, ptr: Value<'a>) -> Result<Vec<Transition>> {
        let Some(arr) = ptr.deref()? else {
            return Ok(Vec::new());
        };
        let mut out = Vec::new();
        for t in arr.get("transitions")?.elements()? {
            out.push(Transition {
                event: int_of(&t, "eventId")? as i32,
                to_state: int_of(&t, "toStateId")? as i32,
                from_nested_state: int_of(&t, "fromNestedStateId")? as i32,
                to_nested_state: int_of(&t, "toNestedStateId")? as i32,
                priority: int_of(&t, "priority")?,
                flags: int_of(&t, "flags")? as u32,
                effect: self.effect(t.get("transition")?)?,
                has_condition: t.get("condition")?.deref()?.is_some(),
            });
        }
        Ok(out)
    }

    fn child(&mut self, ptr: Value<'a>, depth: usize) -> Result<Option<NodeId>> {
        match ptr.deref()? {
            Some(v) => Ok(Some(self.node(v, depth + 1)?)),
            None => Ok(None),
        }
    }

    fn children(&mut self, arr: Value<'a>, depth: usize) -> Result<Vec<Option<NodeId>>> {
        let mut out = Vec::new();
        for p in arr.elements()? {
            out.push(self.child(p, depth)?);
        }
        Ok(out)
    }

    fn node(&mut self, v: Value<'a>, depth: usize) -> Result<NodeId> {
        if let Some(&id) = self.by_offset.get(&v.offset) {
            return Ok(id);
        }
        if depth > 512 {
            return bail("behaviour graph nests too deeply");
        }
        let id = self.nodes.len();
        self.by_offset.insert(v.offset, id);
        self.nodes.push(None);
        let kind = if v.is_a("hkbStateMachine") {
            let mut states = Vec::new();
            for p in v.get("states")?.elements()? {
                let Some(s) = p.deref()? else { continue };
                states.push(State {
                    name: str_of(&s, "name")?,
                    id: int_of(&s, "stateId")? as i32,
                    generator: self.child(s.get("generator")?, depth)?,
                    transitions: self.transitions(s.get("transitions")?)?,
                    probability: f32_of(&s, "probability")?,
                    enable: s.get("enable")?.bool()?,
                });
            }
            NodeKind::StateMachine(StateMachine {
                start_state_id: int_of(&v, "startStateId")? as i32,
                states,
                wildcard_transitions: self.transitions(v.get("wildcardTransitions")?)?,
                return_to_previous_state_event: int_of(&v, "returnToPreviousStateEventId")? as i32,
                self_transition_mode: int_of(&v, "selfTransitionMode")?,
                start_state_mode: int_of(&v, "startStateMode")?,
                sync_variable: int_of(&v, "syncVariableIndex")? as i32,
            })
        } else if v.is_a("CustomManualSelectorGenerator") {
            NodeKind::Cmsg(Cmsg {
                generators: self.children(v.get("generators")?, depth)?,
                offset_type: int_of(&v, "offsetType")?,
                anim_id: int_of(&v, "animId")?,
                anime_end_event_type: int_of(&v, "animeEndEventType")?,
                enable_script: v.get("enableScript")?.bool()?,
                enable_tae: v.get("enableTae")?.bool()?,
                change_type_of_selected_index_after_activate: int_of(
                    &v,
                    "changeTypeOfSelectedIndexAfterActivate",
                )?,
                generator_changed_effect: self
                    .effect(v.get("generatorChangedTransitionEffect")?)?,
                check_anim_end_slot: int_of(&v, "checkAnimEndSlotNo")?,
                end_event: int_of(&v.get("endEvent")?, "id")? as i32,
            })
        } else if v.is_a("hkbManualSelectorGenerator") {
            NodeKind::ManualSelector(ManualSelector {
                generators: self.children(v.get("generators")?, depth)?,
                selected_index: int_of(&v, "selectedGeneratorIndex")?,
                index_can_change_after_activate: v
                    .get("selectedIndexCanChangeAfterActivate")?
                    .bool()?,
                generator_changed_effect: self
                    .effect(v.get("generatorChangedTransitionEffect")?)?,
            })
        } else if v.is_a("hkbClipGenerator") {
            NodeKind::Clip(Clip {
                animation: v.get("animationName")?.string()?,
                mode: int_of(&v, "mode")?,
                playback_speed: f32_of(&v, "playbackSpeed")?,
                start_time: f32_of(&v, "startTime")?,
                crop_start: f32_of(&v, "cropStartAmountLocalTime")?,
                crop_end: f32_of(&v, "cropEndAmountLocalTime")?,
                enforced_duration: f32_of(&v, "enforcedDuration")?,
                user_controlled_time_fraction: f32_of(&v, "userControlledTimeFraction")?,
                flags: int_of(&v, "flags")?,
            })
        } else if v.is_a("hkbLayerGenerator") {
            let mut layers = Vec::new();
            for p in v.get("layers")?.elements()? {
                let Some(l) = p.deref()? else { continue };
                let b = l.get("blendingControlData")?;
                layers.push(Layer {
                    generator: self.child(l.get("generator")?, depth)?,
                    bindings: bindings_of(&l)?,
                    weight: f32_of(&b, "weight")?,
                    on_by_default: b.get("onByDefault")?.bool()?,
                    on_event: int_of(&b, "onEventId")? as i32,
                    off_event: int_of(&b, "offEventId")? as i32,
                    fade_in: f32_of(&b, "fadeInDuration")?,
                    fade_out: f32_of(&b, "fadeOutDuration")?,
                    use_motion: l.get("useMotion")?.bool()?,
                });
            }
            NodeKind::Layers(layers)
        } else if v.is_a("hkbBlenderGenerator") {
            let mut children = Vec::new();
            for p in v.get("children")?.elements()? {
                let Some(c) = p.deref()? else { continue };
                children.push(BlendChild {
                    generator: self.child(c.get("generator")?, depth)?,
                    bindings: bindings_of(&c)?,
                    weight: f32_of(&c, "weight")?,
                });
            }
            NodeKind::Blender {
                blend_parameter: f32_of(&v, "blendParameter")?,
                children,
            }
        } else if v.is_a("hkbScriptGenerator") {
            NodeKind::Script(Script {
                child: self.child(v.get("child")?, depth)?,
                on_activate: v.get("onActivateScript")?.string()?,
                on_pre_update: v.get("onPreUpdateScript")?.string()?,
                on_generate: v.get("onGenerateScript")?.string()?,
                on_handle_event: v.get("onHandleEventScript")?.string()?,
                on_deactivate: v.get("onDeactivateScript")?.string()?,
            })
        } else if v.is_a("hkbBehaviorReferenceGenerator") {
            NodeKind::BehaviorReference {
                behavior_name: v.get("behaviorName")?.string()?,
            }
        } else if v.is_a("hkbModifierGenerator") {
            NodeKind::Modifier {
                child: self.child(v.get("generator")?, depth)?,
            }
        } else {
            NodeKind::Other {
                children: self.generic_children(v, depth)?,
            }
        };
        self.nodes[id] = Some(Node {
            type_name: v.type_name().to_owned(),
            name: str_of(&v, "name")?,
            user_data: opt_int(&v, "userData")?.unwrap_or(0),
            bindings: bindings_of(&v)?,
            kind,
        });
        Ok(id)
    }

    /// Child generators of an unmodelled node type: every pointer (or array of pointers) member
    /// that points at a generator.
    fn generic_children(&mut self, v: Value<'a>, depth: usize) -> Result<Vec<NodeId>> {
        let mut out = Vec::new();
        for m in self.file.types.all_members(v.ty) {
            let mv = v.get(&m.name)?;
            let ptrs = match mv.kind() {
                Kind::Pointer => vec![mv],
                Kind::Array => match mv.elements() {
                    Ok(e) if e.first().is_some_and(|x| x.kind() == Kind::Pointer) => e,
                    _ => continue,
                },
                _ => continue,
            };
            for p in ptrs {
                if let Some(t) = p.deref()?
                    && t.is_a("hkbGenerator")
                {
                    out.push(self.node(t, depth + 1)?);
                }
            }
        }
        Ok(out)
    }
}

fn decode_value(ty: VariableType, word: i64, quads: &[Vec<f32>]) -> VariableValue {
    match ty {
        VariableType::Real => VariableValue::Real(f32::from_bits(word as u32)),
        VariableType::Vector3 | VariableType::Vector4 | VariableType::Quaternion => {
            let q = quads.get(word as usize).cloned().unwrap_or_default();
            let mut a = [0.0; 4];
            for (d, s) in a.iter_mut().zip(q) {
                *d = s;
            }
            VariableValue::Quad(a)
        }
        VariableType::Pointer => VariableValue::Pointer(word as i32),
        _ => VariableValue::Int(word as i32),
    }
}

/// Reads the behaviour graph from a parsed behaviour tagfile.
pub fn from_tagfile(file: &TagFile) -> Result<Graph> {
    let graph = file.first("hkbBehaviorGraph")?;
    let data = graph
        .get("data")?
        .deref()?
        .ok_or_else(|| Error::Format("behaviour graph has no data".into()))?;
    let strings = data
        .get("stringData")?
        .deref()?
        .ok_or_else(|| Error::Format("behaviour graph has no string data".into()))?;
    let names = |field: &str| -> Result<Vec<String>> {
        strings
            .get(field)?
            .elements()?
            .iter()
            .map(|s| s.string())
            .collect()
    };
    let events = names("eventNames")?;
    let variable_names = names("variableNames")?;
    let animation_names = names("animationNames")?;
    let event_flags = data
        .get("eventInfos")?
        .elements()?
        .iter()
        .map(|e| e.get("flags")?.int())
        .collect::<Result<Vec<_>>>()?;
    let infos = data.get("variableInfos")?.elements()?;
    let bounds = data.get("variableBounds")?.elements()?;
    let (words, quads) = match data.get("variableInitialValues")?.deref()? {
        Some(set) => (
            set.get("wordVariableValues")?
                .elements()?
                .iter()
                .map(|w| w.get("value")?.int())
                .collect::<Result<Vec<_>>>()?,
            set.get("quadVariableValues")?
                .elements()?
                .iter()
                .map(|q| q.floats())
                .collect::<Result<Vec<_>>>()?,
        ),
        None => (Vec::new(), Vec::new()),
    };
    let mut variables = Vec::with_capacity(variable_names.len());
    for (i, name) in variable_names.into_iter().enumerate() {
        let ty = match infos.get(i) {
            Some(info) => VariableType::from_raw(info.get("type")?.int()?),
            None => VariableType::Unknown(-1),
        };
        let word = words.get(i).copied().unwrap_or(0);
        let bound = |field: &str| -> Result<VariableValue> {
            Ok(match bounds.get(i) {
                Some(b) => decode_value(ty, b.get(field)?.get("value")?.int()?, &[]),
                None => VariableValue::Int(0),
            })
        };
        variables.push(Variable {
            name,
            ty,
            initial: decode_value(ty, word, &quads),
            bounds: (bound("min")?, bound("max")?),
        });
    }
    let mut b = Builder {
        nodes: Vec::new(),
        by_offset: HashMap::new(),
        effects: Vec::new(),
        effect_by_offset: HashMap::new(),
        file,
    };
    let root_value = graph
        .get("rootGenerator")?
        .deref()?
        .ok_or_else(|| Error::Format("behaviour graph has no root generator".into()))?;
    let root = b.node(root_value, 0)?;
    let nodes = b
        .nodes
        .into_iter()
        .map(|n| n.ok_or_else(|| Error::Format("unfinished behaviour node".into())))
        .collect::<Result<Vec<_>>>()?;
    Ok(Graph {
        name: str_of(&graph, "name")?,
        events,
        event_flags,
        variables,
        animation_names,
        nodes,
        effects: b.effects,
        root,
    })
}

/// Parses a behaviour `.hkx` (a self-contained `TAG0` tagfile).
pub fn parse(data: &[u8]) -> Result<Graph> {
    let file = TagFile::parse(data, None)?;
    from_tagfile(&file)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anim_names() {
        assert_eq!(parse_anim_name("a050_203000"), Some((50, 203000)));
        assert_eq!(
            parse_anim_name("a000_299000_AddActionInput"),
            Some((0, 299000))
        );
        assert_eq!(parse_anim_name("b050_203000"), None);
        assert_eq!(parse_anim_name("a05_203000"), None);
    }
}
