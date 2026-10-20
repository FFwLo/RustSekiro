//! A behaviour-graph runtime: which states are active, how events move them, which clip plays.
//!
//! Adapted from sekiro-rs (AKJama, MIT / Apache-2.0), crates/sim/src/behavior.rs (CREDITS.md,
//! THIRD_PARTY.md). Changed here: a CMSG's clip is picked the way the exe does it
//! (`AnimOffsets`, `pick_cmsg_clip`); its tests stay in sekiro-rs.
//!
//! This is a deliberately small model of Havok Behavior as FromSoftware uses it:
//!
//! - Every state machine remembers its current state. The active set is everything reachable
//!   from the root through current states, all layers, all blend children, the selected child
//!   of each manual selector and the offset-matched clip of each CMSG.
//! - Events are matched against the active machines first (the current state's own
//!   transitions, then the machine's wildcards), then against the global wildcards of inactive
//!   machines. Taking a global wildcard also switches every enclosing machine to the state that
//!   contains it, which is how one `W_*` event reaches a deeply nested state.
//! - Blends are not modelled: a transition switches instantly. Transition effects are kept on
//!   the [`Transition`] for the animation layer to use.
//! - State hooks are named after the state (`<StateName>_onActivate` and so on), matching the
//!   player script's callback names.

use sekiro_formats::hkb::{Graph, NodeId, NodeKind, Transition, parse_anim_name, transition_flags};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

const FLAG_IGNORE_FROM_WORLD_FROM_MODEL: i64 = 1;
const MAX_EFFECT_CHAIN_DEPTH: usize = 16;

fn resolve_effect(graph: &Graph, mut id: usize) -> Option<&sekiro_formats::hkb::TransitionEffect> {
    for _ in 0..=MAX_EFFECT_CHAIN_DEPTH {
        let effect = graph.effects.get(id)?;
        if effect.choices.is_empty() {
            return Some(effect);
        }
        id = effect
            .choices
            .get(effect.selected_index.unwrap_or(0).max(0) as usize)
            .copied()
            .flatten()?;
    }
    None
}

/// A split motion/animation layer with the engine's locomotion speed selector.
fn locomotion_layer(graph: &Graph, generator: NodeId) -> bool {
    let NodeKind::Layers(layers) = &graph.nodes[generator].kind else {
        return false;
    };
    let Some(speed) = graph.variable_index("MoveSpeedIndex") else {
        return false;
    };
    if !layers.iter().any(|l| !l.use_motion) {
        return false;
    }
    let mut pending: Vec<_> = layers
        .iter()
        .filter(|l| l.use_motion)
        .filter_map(|l| l.generator)
        .collect();
    let mut seen = HashSet::new();
    while let Some(id) = pending.pop() {
        if !seen.insert(id) {
            continue;
        }
        let node = &graph.nodes[id];
        if node.bindings.iter().any(|b| {
            b.binding_type == 0
                && b.member_path == "selectedGeneratorIndex"
                && b.variable as usize == speed
        }) {
            return true;
        }
        if !matches!(node.kind, NodeKind::StateMachine(_)) {
            pending.extend(node.children());
        }
    }
    false
}

/// Supplies clip lengths in seconds; `None` means unknown (treated as never ending).
pub trait ClipDurations {
    fn duration(&mut self, animation: &str) -> Option<f32>;
}

impl<F: FnMut(&str) -> Option<f32>> ClipDurations for F {
    fn duration(&mut self, animation: &str) -> Option<f32> {
        self(animation)
    }
}

/// Which part of a state's lifetime a hook call is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookKind {
    Activate,
    Update,
    Deactivate,
}

impl HookKind {
    pub fn suffix(self) -> &'static str {
        match self {
            HookKind::Activate => "_onActivate",
            HookKind::Update => "_onUpdate",
            HookKind::Deactivate => "_onDeactivate",
        }
    }
}

/// A state hook the script should run, e.g. `StandToDeflectGuard` + `_onActivate`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookCall {
    pub state: String,
    pub kind: HookKind,
}

impl HookCall {
    pub fn function_name(&self) -> String {
        format!("{}{}", self.state, self.kind.suffix())
    }
}

/// A playing clip.
#[derive(Debug, Clone, PartialEq)]
pub struct ClipState {
    pub node: NodeId,
    pub animation: String,
    pub time: f32,
    /// Bound playback offset at activation, distinct from a fresh frame-zero entry.
    pub start_time: f32,
    pub duration: Option<f32>,
    pub looping: bool,
    pub speed: f32,
}

impl ClipState {
    /// True once a non-looping clip has played to its end.
    pub fn at_end(&self) -> bool {
        !self.looping && self.duration.is_some_and(|d| self.time >= d)
    }
}

/// What the exe reads to pick a CMSG's clip (the callback FUN_140b417a0 -> FUN_1409f1830): the
/// offsetType names are CustomManualSelectorGenerator's enum (0 none, 11 idle category,
/// 13 / 14 right / left weapon category, 15 the chr's anim id offset, 17 throw category:
/// ThrowParam atk/defAnimOffset). Every NPC CMSG in c9997.hkx is type 15 (c1020 264, c1500 248).
/// Per hand the equipped row's wepmotionCategory and spAtkcategory (EquipParamWeapon +0xEA):
/// for the right hand the combat art's row.
#[derive(Debug, Clone, Default)]
pub struct AnimOffsets {
    /// [left, right]: (wepmotionCategory, spAtkcategory or 0).
    pub hands: [(i64, i64); 2],
    /// The current throw's anim offset (ThrowParam atkAnimOffset for the thrower).
    pub throw: Option<i64>,
    /// offsetType 15: the chr's anim category (an NPC's anim group, its SpEffect's
    /// animIdOffset: a100 for the ground-grab zombie c1500; 0 = a000).
    pub anim: i64,
}

impl AnimOffsets {
    /// The offset an offsetType resolves to for this anim id (FUN_1409f1830): a hand's
    /// spAtkcategory when the chr has that anim (`has`), else its wepmotionCategory; idle and
    /// none are a000. gap: the idle category array (chr+0x120 slot 2) and its +300 candidate.
    pub fn offset(&self, offset_type: i64, anim_id: i64, has: &mut dyn ClipDurations) -> Option<i64> {
        match offset_type {
            13 | 14 => {
                let (cat, sp) = self.hands[(offset_type == 13) as usize];
                if sp > 0 && has.duration(&format!("a{sp:03}_{anim_id:06}")).is_some() {
                    return Some(sp);
                }
                (cat >= 0).then_some(cat)
            }
            17 => self.throw,
            15 => Some(self.anim),
            _ => Some(0),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct StateRef {
    sm: NodeId,
    index: usize,
}

pub struct BehaviorRuntime {
    graph: Arc<Graph>,
    /// For every node, the state that encloses it (first found from the root).
    enclosing: Vec<Option<StateRef>>,
    /// Current state index per state machine node.
    current: HashMap<NodeId, usize>,
    active: Vec<bool>,
    /// Active states in activation order (parents before children).
    active_states: Vec<StateRef>,
    clips: HashMap<NodeId, ClipState>,
    variables: Vec<f32>,
    pub offsets: AnimOffsets,
    queue: VecDeque<usize>,
    /// States re-entered by a self transition since the last rebuild.
    restarted: HashSet<StateRef>,
    /// Machines whose state was set explicitly since the last rebuild.
    pinned: HashSet<NodeId>,
    /// Layer on/off overrides set by layer events, keyed by (layer generator, layer index).
    layer_on: HashMap<(NodeId, usize), bool>,
    /// The active state each active node was reached from in the last rebuild. Nodes can be
    /// shared between states, so this, not the static [`Self::enclosing`] map, says which state
    /// an active clip belongs to right now.
    active_enclosing: HashMap<NodeId, StateRef>,
    /// For every clip, the CMSGs that own it.
    clip_owner: Vec<Vec<NodeId>>,
    /// Clips whose end has already been signalled since they started.
    end_signalled: HashSet<NodeId>,
    /// Set when a variable changed, since bound selectors and layer weights may now differ.
    dirty: bool,
    /// The machine whose active clip answers "animation ended" for slot 0.
    main_machine: NodeId,
    /// Events that matched no transition, most recent last (bounded).
    pub unhandled_events: VecDeque<String>,
    /// Per-target graph flags, scoped to the owning machine and state.
    blend_sources: HashMap<StateRef, bool>,
    locomotion_layers: HashSet<StateRef>,
    /// Name -> index of the graph's events and variables (the script names them every call).
    event_ids: HashMap<String, usize>,
    var_ids: HashMap<String, usize>,
}

impl BehaviorRuntime {
    pub fn new(graph: Arc<Graph>, offsets: AnimOffsets) -> Self {
        let n = graph.nodes.len();
        let mut enclosing = vec![None; n];
        let mut seen = vec![false; n];
        let mut stack = vec![(graph.root, None)];
        while let Some((id, enc)) = stack.pop() {
            if seen[id] {
                continue;
            }
            seen[id] = true;
            enclosing[id] = enc;
            match &graph.nodes[id].kind {
                NodeKind::StateMachine(sm) => {
                    for (i, s) in sm.states.iter().enumerate().rev() {
                        if let Some(g) = s.generator {
                            stack.push((g, Some(StateRef { sm: id, index: i })));
                        }
                    }
                }
                _ => {
                    for c in graph.nodes[id].children().into_iter().rev() {
                        stack.push((c, enc));
                    }
                }
            }
        }
        let mut clip_owner = vec![Vec::new(); n];
        for (id, node) in graph.nodes.iter().enumerate() {
            if let NodeKind::Cmsg(c) = &node.kind {
                for g in c.generators.iter().flatten() {
                    clip_owner[*g].push(id);
                }
            }
        }
        let variables = graph.variables.iter().map(|v| v.initial.as_f32()).collect();
        let main_machine = graph.node_by_name("Master_SM").unwrap_or(graph.root);
        let mut votes: HashMap<StateRef, i32> = HashMap::new();
        let mut locomotion_layers = HashSet::new();
        for (sm_id, sm) in graph.state_machines() {
            for (index, state) in sm.states.iter().enumerate() {
                if state
                    .generator
                    .is_some_and(|id| locomotion_layer(&graph, id))
                {
                    locomotion_layers.insert(StateRef { sm: sm_id, index });
                }
            }
            for t in sm
                .wildcard_transitions
                .iter()
                .chain(sm.states.iter().flat_map(|s| &s.transitions))
            {
                let Some(index) = sm.state_index(t.to_state) else {
                    continue;
                };
                if let Some(flags) = t
                    .effect
                    .and_then(|id| resolve_effect(&graph, id))
                    .and_then(|e| e.flags)
                {
                    *votes.entry(StateRef { sm: sm_id, index }).or_default() +=
                        if flags & FLAG_IGNORE_FROM_WORLD_FROM_MODEL == 0 {
                            1
                        } else {
                            -1
                        };
                }
            }
        }
        let blend_sources = votes
            .into_iter()
            .map(|(state, tally)| (state, tally >= 0))
            .collect();
        let event_ids = graph.events.iter().enumerate().map(|(i, e)| (e.clone(), i)).rev().collect();
        let var_ids = graph.variables.iter().enumerate().map(|(i, v)| (v.name.clone(), i)).rev().collect();
        let mut rt = Self {
            event_ids,
            var_ids,
            blend_sources,
            locomotion_layers,
            graph,
            enclosing,
            current: HashMap::new(),
            active: vec![false; n],
            active_states: Vec::new(),
            clips: HashMap::new(),
            variables,
            offsets,
            queue: VecDeque::new(),
            restarted: HashSet::new(),
            pinned: HashSet::new(),
            layer_on: HashMap::new(),
            clip_owner,
            active_enclosing: HashMap::new(),
            end_signalled: HashSet::new(),
            dirty: false,
            main_machine,
            unhandled_events: VecDeque::new(),
        };
        rt.rebuild(&mut |_: &str| None);
        rt
    }

    pub fn graph(&self) -> &Graph {
        &self.graph
    }

    /// Hooks for the states active after construction (call once after loading the scripts).
    pub fn initial_hooks(&self) -> Vec<HookCall> {
        self.active_states
            .iter()
            .map(|s| HookCall {
                state: self.state_name(*s).to_owned(),
                kind: HookKind::Activate,
            })
            .collect()
    }

    fn state_name(&self, s: StateRef) -> &str {
        match &self.graph.nodes[s.sm].kind {
            NodeKind::StateMachine(sm) => &sm.states[s.index].name,
            _ => "",
        }
    }

    fn current_index(&self, sm_id: NodeId) -> usize {
        let NodeKind::StateMachine(sm) = &self.graph.nodes[sm_id].kind else {
            return 0;
        };
        if let Some(i) = self.current.get(&sm_id) {
            return *i;
        }
        // Sync mode: start in the state whose id the shared sync variable holds.
        if sm.start_state_mode == 1
            && let Some(v) = usize::try_from(sm.sync_variable)
                .ok()
                .and_then(|v| self.variables.get(v))
            && let Some(i) = sm.state_index(*v as i32)
        {
            return i;
        }
        sm.state_index(sm.start_state_id).unwrap_or(0)
    }

    /// Queues an event by name. Returns false if the graph has no such event.
    pub fn fire_event(&mut self, name: &str) -> bool {
        match self.event_ids.get(name).copied() {
            Some(e) => {
                self.queue.push_back(e);
                true
            }
            None => {
                self.note_unhandled(name.to_owned());
                false
            }
        }
    }

    /// Havok FLAG_IGNORE_FROM_WORLD_FROM_MODEL, using the target state's majority
    /// when it has several entry effects (FwLo's data.rs::blend_keeps_src).
    pub fn blend_keeps_source(&self) -> bool {
        self.main_states()
            .find_map(|state| self.blend_sources.get(&state).copied())
            .unwrap_or(true)
    }

    pub fn main_is_locomotion_layer(&self) -> bool {
        self.main_state_key()
            .is_some_and(|s| self.locomotion_layers.contains(&s))
    }

    fn note_unhandled(&mut self, name: String) {
        if self.unhandled_events.len() >= 64 {
            self.unhandled_events.pop_front();
        }
        self.unhandled_events.push_back(name);
    }

    pub fn has_pending_events(&self) -> bool {
        !self.queue.is_empty()
    }

    /// A variable changed since the last rebuild, so selectors and layers bound to it may pick
    /// differently (Havok re-reads bindings every update; a strafe loop follows
    /// `MoveDirection` while it plays).
    pub fn bindings_changed(&self) -> bool {
        self.dirty
    }

    /// Applies all queued events and returns the hooks to run, deactivations first.
    pub fn process_events(&mut self, durations: &mut dyn ClipDurations) -> Vec<HookCall> {
        let before: Vec<StateRef> = self.active_states.clone();
        let mut changed = std::mem::take(&mut self.dirty);
        while let Some(e) = self.queue.pop_front() {
            if self.apply_event(e) {
                changed = true;
            } else {
                let name = self.graph.events[e].clone();
                self.note_unhandled(name);
            }
        }
        if !changed {
            return Vec::new();
        }
        let restarted = self.restarted.clone();
        self.rebuild(durations);
        let after: HashSet<StateRef> = self.active_states.iter().copied().collect();
        let before_set: HashSet<StateRef> = before.iter().copied().collect();
        let mut hooks = Vec::new();
        for s in before.iter().rev() {
            if !after.contains(s) || restarted.contains(s) {
                hooks.push(HookCall {
                    state: self.state_name(*s).to_owned(),
                    kind: HookKind::Deactivate,
                });
            }
        }
        for s in &self.active_states {
            if !before_set.contains(s) || restarted.contains(s) {
                hooks.push(HookCall {
                    state: self.state_name(*s).to_owned(),
                    kind: HookKind::Activate,
                });
            }
        }
        hooks
    }

    /// Update hooks for every active state, in activation order.
    pub fn update_hooks(&self) -> Vec<HookCall> {
        self.active_states
            .iter()
            .map(|s| HookCall {
                state: self.state_name(*s).to_owned(),
                kind: HookKind::Update,
            })
            .collect()
    }

    fn find_transition(transitions: &[Transition], event: i32) -> Option<&Transition> {
        transitions
            .iter()
            .filter(|t| t.event == event && t.flags & transition_flags::DISABLED == 0)
            .min_by_key(|t| t.priority)
    }

    fn apply_event(&mut self, event: usize) -> bool {
        let graph = self.graph.clone();
        let ev = event as i32;
        let mut handled = false;
        // Layer on/off events of active layer generators.
        for (id, node) in graph.nodes.iter().enumerate() {
            if !self.active[id] {
                continue;
            }
            if let NodeKind::Layers(layers) = &node.kind {
                for (i, l) in layers.iter().enumerate() {
                    if l.on_event == ev {
                        self.layer_on.insert((id, i), true);
                        handled = true;
                    } else if l.off_event == ev {
                        self.layer_on.insert((id, i), false);
                        handled = true;
                    }
                }
            }
        }
        // Havok delivers an event to every state machine, so several can transition at once: the
        // additive layers, or the upper- and lower-body machines of a layered state that both
        // listen for the same event. Choices made for this event must agree on every enclosing
        // machine; a machine whose ancestor already switched is skipped.
        let mut chosen: HashMap<NodeId, usize> = HashMap::new();
        let mut machines: Vec<NodeId> = Vec::new();
        for s in &self.active_states {
            if !machines.contains(&s.sm) {
                machines.push(s.sm);
            }
        }
        // Active machines, outermost first: the current state's transitions, then wildcards.
        for &sm_id in &machines {
            if self.ancestor_switched(sm_id, &chosen) {
                continue;
            }
            let NodeKind::StateMachine(sm) = &graph.nodes[sm_id].kind else {
                continue;
            };
            let cur = self.current_index(sm_id);
            let hit = Self::find_transition(&sm.states[cur].transitions, ev)
                .or_else(|| Self::find_transition(&sm.wildcard_transitions, ev));
            if let Some(t) = hit
                && self.take(sm_id, t, &mut chosen)
            {
                handled = true;
            }
        }
        // Global wildcards of inactive machines, as long as they agree with what was chosen.
        for (sm_id, sm) in graph.state_machines() {
            if self.active[sm_id] {
                continue;
            }
            if let Some(t) = Self::find_transition(&sm.wildcard_transitions, ev)
                && t.flags & transition_flags::IS_GLOBAL_WILDCARD != 0
                && self.take(sm_id, t, &mut chosen)
            {
                handled = true;
            }
        }
        handled
    }

    /// True if a machine enclosing `sm_id` was switched by this event to a state that is not on
    /// `sm_id`'s path (so `sm_id` is about to become inactive or was just entered).
    fn ancestor_switched(&self, sm_id: NodeId, chosen: &HashMap<NodeId, usize>) -> bool {
        let mut up = self.enclosing[sm_id];
        while let Some(s) = up {
            if chosen.get(&s.sm).is_some_and(|&i| i != s.index) {
                return true;
            }
            up = self.enclosing[s.sm];
        }
        false
    }

    /// Takes transition `t` of machine `sm_id` unless it conflicts with `chosen` (the states
    /// already picked for this event), and records its path in `chosen`.
    fn take(&mut self, sm_id: NodeId, t: &Transition, chosen: &mut HashMap<NodeId, usize>) -> bool {
        let graph = self.graph.clone();
        let NodeKind::StateMachine(sm) = &graph.nodes[sm_id].kind else {
            return false;
        };
        let Some(index) = sm.state_index(t.to_state) else {
            return false;
        };
        if chosen.get(&sm_id).is_some_and(|&i| i != index) {
            return false;
        }
        let mut up = self.enclosing[sm_id];
        while let Some(s) = up {
            if chosen.get(&s.sm).is_some_and(|&i| i != s.index) {
                return false;
            }
            up = self.enclosing[s.sm];
        }
        chosen.insert(sm_id, index);
        let mut up = self.enclosing[sm_id];
        while let Some(s) = up {
            chosen.insert(s.sm, s.index);
            up = self.enclosing[s.sm];
        }
        let target = StateRef { sm: sm_id, index };
        if self.active[sm_id] && self.current_index(sm_id) == index {
            self.restarted.insert(target);
        }
        self.current.insert(sm_id, index);
        self.pinned.insert(sm_id);
        self.write_sync(sm_id, index);
        // Nested target state inside the destination state's own machine.
        if t.flags & transition_flags::TO_NESTED_STATE_ID_IS_VALID != 0
            && let Some(g) = sm.states[index].generator
            && let NodeKind::StateMachine(inner) = &graph.nodes[g].kind
            && let Some(i) = inner.state_index(t.to_nested_state)
        {
            self.current.insert(g, i);
            self.pinned.insert(g);
        }
        // Switch every enclosing machine to the state containing this one.
        let mut up = self.enclosing[sm_id];
        while let Some(s) = up {
            self.current.insert(s.sm, s.index);
            self.pinned.insert(s.sm);
            up = self.enclosing[s.sm];
        }
        true
    }

    fn rebuild(&mut self, durations: &mut dyn ClipDurations) {
        let graph = self.graph.clone();
        let was_active = std::mem::replace(&mut self.active, vec![false; graph.nodes.len()]);
        let restarted: HashSet<StateRef> = self.restarted.iter().copied().collect();
        let mut states = Vec::new();
        let mut clips = Vec::new();
        let mut stack: Vec<(NodeId, Option<StateRef>)> = vec![(graph.root, None)];
        self.active_enclosing.clear();
        while let Some((id, enc)) = stack.pop() {
            if self.active[id] {
                continue;
            }
            self.active[id] = true;
            if let Some(e) = enc {
                self.active_enclosing.insert(id, e);
            }
            let node = &graph.nodes[id];
            match &node.kind {
                NodeKind::StateMachine(sm) => {
                    if !was_active[id] && !self.pinned.contains(&id) {
                        self.current.remove(&id);
                    }
                    if sm.states.is_empty() {
                        continue;
                    }
                    let index = self.current_index(id);
                    self.write_sync(id, index);
                    let here = StateRef { sm: id, index };
                    states.push(here);
                    if let Some(g) = sm.states[index].generator {
                        stack.push((g, Some(here)));
                    }
                }
                NodeKind::ManualSelector(m) => {
                    let idx = self.bound_index(node, "selectedGeneratorIndex", m.selected_index);
                    let pick = m
                        .generators
                        .get(idx)
                        .or(m.generators.first())
                        .copied()
                        .flatten();
                    stack.extend(pick.map(|p| (p, enc)));
                }
                NodeKind::Cmsg(c) => {
                    if let Some(g) = self.pick_cmsg_clip(c, durations) {
                        stack.push((g, enc));
                    }
                }
                NodeKind::Clip(_) => clips.push(id),
                NodeKind::Layers(layers) => {
                    for (i, l) in layers.iter().enumerate().rev() {
                        if self.layer_enabled(id, i, l)
                            && let Some(g) = l.generator
                        {
                            stack.push((g, enc));
                        }
                    }
                }
                _ => {
                    for ch in node.children().into_iter().rev() {
                        stack.push((ch, enc));
                    }
                }
            }
        }
        self.pinned.clear();
        // Clips: keep time for clips that stayed active, restart the rest.
        let restarted_clips: HashSet<NodeId> = clips
            .iter()
            .copied()
            .filter(|c| {
                self.enclosing_state(*c)
                    .is_some_and(|s| restarted.contains(&s))
            })
            .collect();
        let mut new_clips = HashMap::new();
        for id in clips {
            let NodeKind::Clip(c) = &graph.nodes[id].kind else {
                continue;
            };
            let keep = was_active[id] && !restarted_clips.contains(&id);
            if !keep {
                self.end_signalled.remove(&id);
            }
            let state = match (keep, self.clips.remove(&id)) {
                // Clips first activated before lengths were available learn them now.
                (true, Some(mut s)) => {
                    if s.duration.is_none() {
                        s.duration = durations.duration(&c.animation);
                    }
                    s
                }
                _ => {
                    let start_time = self.bound_value(&graph.nodes[id], "startTime", c.start_time);
                    ClipState {
                        node: id,
                        animation: c.animation.clone(),
                        time: start_time,
                        start_time,
                        duration: durations.duration(&c.animation),
                        looping: c.mode == 1,
                        speed: c.playback_speed,
                    }
                }
            };
            new_clips.insert(id, state);
        }
        self.end_signalled.retain(|c| new_clips.contains_key(c));
        self.clips = new_clips;
        self.active_states = states;
        self.restarted.clear();
    }

    /// The nearest state machine state enclosing `node`, following the active path.
    fn enclosing_state(&self, node: NodeId) -> Option<StateRef> {
        self.encl(node)
    }

    /// A layer contributes when it is switched on (by default or by its on/off events) and its
    /// weight, usually bound to a `*Blend` variable the script sets, is above zero.
    fn layer_enabled(&self, layers: NodeId, index: usize, l: &sekiro_formats::hkb::Layer) -> bool {
        let on = self
            .layer_on
            .get(&(layers, index))
            .copied()
            .unwrap_or(l.on_by_default);
        let weight = l
            .bindings
            .iter()
            .find(|b| b.member_path.ends_with("weight") && b.binding_type == 0)
            .and_then(|b| self.variables.get(b.variable as usize).copied())
            .unwrap_or(l.weight);
        on && weight > 0.0
    }

    /// The clip directly owned by the active state named `state` (not through nested machines).
    pub fn state_clip(&self, state: &str) -> Option<&ClipState> {
        let s = *self
            .active_states
            .iter()
            .find(|s| self.state_name(**s) == state)?;
        self.clips
            .values()
            .filter(|c| self.encl(c.node) == Some(s))
            .min_by_key(|c| c.node)
    }

    /// The enclosing state of `node` on the active path, else its first static parent.
    fn encl(&self, node: NodeId) -> Option<StateRef> {
        self.active_enclosing
            .get(&node)
            .copied()
            .or(self.enclosing[node])
    }

    /// Stores machine `sm_id`'s current state id in its sync variable, if it has one.
    fn write_sync(&mut self, sm_id: NodeId, index: usize) {
        if let NodeKind::StateMachine(sm) = &self.graph.nodes[sm_id].kind
            && let Ok(v) = usize::try_from(sm.sync_variable)
            && let Some(slot) = self.variables.get_mut(v)
        {
            *slot = sm.states[index].id as f32;
        }
    }

    fn bound_index(&self, node: &sekiro_formats::hkb::Node, member: &str, default: i64) -> usize {
        self.bound_value(node, member, default as f32).max(0.0) as usize
    }

    fn bound_value(&self, node: &sekiro_formats::hkb::Node, member: &str, default: f32) -> f32 {
        node.bindings
            .iter()
            .find(|b| b.member_path == member && b.binding_type == 0)
            .and_then(|b| self.variables.get(b.variable as usize))
            .copied()
            .unwrap_or(default)
    }

    /// The clip a CMSG plays (FUN_1411e6b40 / FUN_1411e6b90): the one named
    /// `a<offset>_<animId>`, else `a000_<animId>`, else the last clip.
    fn pick_cmsg_clip(&self, c: &sekiro_formats::hkb::Cmsg, has: &mut dyn ClipDurations) -> Option<NodeId> {
        let want = self.offsets.offset(c.offset_type, c.anim_id, has);
        let offset_of = |g: NodeId| match &self.graph.nodes[g].kind {
            NodeKind::Clip(clip) => parse_anim_name(&clip.animation).map(|(o, _)| o as i64),
            _ => None,
        };
        let gens: Vec<NodeId> = c.generators.iter().flatten().copied().collect();
        gens.iter()
            .copied()
            .find(|&g| want.is_some() && offset_of(g) == want)
            .or_else(|| gens.iter().copied().find(|&g| offset_of(g) == Some(0)))
            .or_else(|| gens.last().copied())
    }

    /// Queues the events that ending clips send, following each owning CMSG's
    /// `animeEndEventType`, as inferred from the shipped graph:
    ///
    /// - 0: take the owning state's own (first) transition, e.g. stand-to-guard into guard idle;
    /// - 2: send the CMSG's end event (event 0, `Idle_wild`, returns to idle);
    /// - 1 and 3: nothing (hold the last frame, or loop).
    ///
    /// Returns the number of events queued.
    pub fn queue_clip_end_events(&mut self) -> usize {
        let graph = self.graph.clone();
        let mut ended: Vec<NodeId> = self
            .clips
            .values()
            .filter(|c| c.at_end() && !self.end_signalled.contains(&c.node))
            .map(|c| c.node)
            .collect();
        ended.sort_unstable();
        let mut queued = 0;
        for clip in ended {
            self.end_signalled.insert(clip);
            let Some(owner) = self.owner(clip) else {
                continue;
            };
            let NodeKind::Cmsg(c) = &graph.nodes[owner].kind else {
                continue;
            };
            let event = match c.anime_end_event_type {
                0 => self.encl(clip).and_then(|s| match &graph.nodes[s.sm].kind {
                    NodeKind::StateMachine(sm) => {
                        sm.states[s.index].transitions.first().map(|t| t.event)
                    }
                    _ => None,
                }),
                2 => Some(c.end_event),
                _ => None,
            };
            if let Some(e) = event.and_then(|e| usize::try_from(e).ok()) {
                self.queue.push_back(e);
                queued += 1;
            }
        }
        queued
    }

    /// Advances every active clip.
    pub fn advance(&mut self, dt: f32) {
        for c in self.clips.values_mut() {
            c.time += dt * c.speed;
            if c.looping
                && let Some(d) = c.duration
                && d > 0.0
            {
                c.time %= d;
            }
        }
    }

    /// The clip playing under the main state machine (the full-body slot 0).
    ///
    /// Follows current states down through nested machines. Where a state runs several machines
    /// in parallel (layers, blends), the first one that leads to a real clip wins; the
    /// `a*_009999` placeholder clips (the empty "no additive" animation) never do.
    pub fn main_clip(&self) -> Option<&ClipState> {
        self.find_main_clip(self.main_machine, 0)
    }

    fn find_main_clip(&self, sm: NodeId, depth: usize) -> Option<&ClipState> {
        if depth > 64 || !self.active[sm] {
            return None;
        }
        let here = StateRef {
            sm,
            index: self.current_index(sm),
        };
        // A split body (StandMoveOverwrite: the legs' loop beside an upper-body action) shows
        // the action: layers that are not locomotion go first.
        let mut children: Vec<&StateRef> = self
            .active_states
            .iter()
            .filter(|s| s.sm != sm && self.encl(s.sm) == Some(here))
            .collect();
        children.sort_by_key(|s| self.locomotion_layers.contains(s));
        for child in children {
            if let Some(clip) = self.find_main_clip(child.sm, depth + 1) {
                return Some(clip);
            }
        }
        self.clips
            .values()
            .filter(|c| self.encl(c.node) == Some(here) && !Self::is_placeholder(c))
            .min_by_key(|c| c.node)
    }

    pub(crate) fn main_state_key(&self) -> Option<StateRef> {
        if let Some(clip) = self.main_clip() {
            return self.encl(clip.node);
        }
        let mut sm = self.main_machine;
        let mut last = None;
        for _ in 0..64 {
            if !self.active[sm] {
                break;
            }
            let here = StateRef {
                sm,
                index: self.current_index(sm),
            };
            last = Some(here);
            let Some(child) = self
                .active_states
                .iter()
                .find(|s| s.sm != sm && self.encl(s.sm) == Some(here))
            else {
                break;
            };
            sm = child.sm;
        }
        last
    }

    /// The CMSG that plays `clip`, without its `_CMSG` suffix: the behaviour state name the
    /// extractor's CMSG table and player.rs use (LockonForwardGroundJumpStart in the graph's
    /// GroundJumpStart).
    /// The CMSG playing a clip generator: several can share one (VerticalGroundJumpFall and
    /// HorizontalWallJumpFall both hold a000_201030's), the active one is its owner.
    fn owner(&self, clip: NodeId) -> Option<NodeId> {
        let owners = self.clip_owner.get(clip)?;
        owners.iter().copied().find(|&o| self.active[o]).or_else(|| owners.last().copied())
    }

    pub fn clip_cmsg(&self, clip: &ClipState) -> Option<&str> {
        let owner = self.owner(clip.node)?;
        Some(self.graph.nodes[owner].name.trim_end_matches("_CMSG"))
    }

    pub fn main_state_name(&self) -> &str {
        self.main_state_key().map_or("", |s| self.state_name(s))
    }

    fn is_placeholder(c: &ClipState) -> bool {
        c.animation.ends_with("_009999")
    }

    /// Leaf first, bounded at the full-body machine rather than its outer graph.
    fn main_states(&self) -> impl Iterator<Item = StateRef> + '_ {
        std::iter::successors(self.main_state_key(), |s| {
            if s.sm == self.main_machine {
                None
            } else {
                self.encl(s.sm)
            }
        })
        .take(65)
    }

    /// Names of the active states under the main machine, outermost first, along the path that
    /// leads to [`Self::main_clip`] (or the first nested machine when there is no clip).
    pub fn main_state_path(&self) -> Vec<String> {
        let mut path: Vec<_> = self
            .main_states()
            .map(|s| self.state_name(s).to_owned())
            .collect();
        path.reverse();
        path
    }

    /// All active state names, in activation order.
    pub fn active_state_names(&self) -> Vec<String> {
        self.active_states
            .iter()
            .map(|s| self.state_name(*s).to_owned())
            .collect()
    }

    /// `env(339, slot)`: whether an animation has ended. When a state hook is running, the
    /// question is about that state's own clip (additive-layer states ask with slot 0 too);
    /// otherwise about the full-body clip. Slots 0 and 1 are modelled.
    pub fn anim_ended(&self, slot: i32, hook_state: Option<&str>) -> bool {
        // The player asks with slot 0, NPC scripts with slot 1; both mean the full-body clip.
        if !(0..=1).contains(&slot) {
            return false;
        }
        hook_state
            .and_then(|s| self.state_clip(s))
            .or_else(|| self.main_clip())
            .is_some_and(ClipState::at_end)
    }

    /// The names of every active node (`hkbIsNodeActive` for a whole script call).
    /// Which nodes are active (index = node id); `active_node_names` changes only with it.
    pub fn active_mask(&self) -> &[bool] {
        &self.active
    }

    pub fn active_node_names(&self) -> HashSet<String> {
        self.graph.nodes.iter().enumerate().filter(|(i, _)| self.active[*i]).map(|(_, n)| n.name.clone()).collect()
    }

    pub fn is_node_active(&self, name: &str) -> bool {
        self.graph
            .nodes
            .iter()
            .enumerate()
            .any(|(i, n)| self.active[i] && n.name == name)
    }

    pub fn variable(&self, name: &str) -> Option<f32> {
        self.var_ids.get(name).and_then(|&i| self.variables.get(i).copied())
    }

    pub fn set_variable(&mut self, name: &str, value: f32) -> bool {
        match self.var_ids.get(name).copied() {
            Some(i) => {
                if self.variables[i] != value {
                    self.variables[i] = value;
                    self.dirty = true;
                }
                true
            }
            None => false,
        }
    }

    pub fn clips(&self) -> impl Iterator<Item = &ClipState> {
        self.clips.values()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// c0000.hkx's picks: the jump's direction from the selector on Selector_GroundJumpType (the
    /// script's SetVariable: 0 the vertical jump a000_201001, 1 the forward one a000_201000) and
    /// the clip group from the equipment (offsetType 13: the art's spAtkcategory where it has the
    /// clip, else the katana's a050; 14: the tool's a07x).
    #[test]
    fn the_graph_picks_clips_by_variable_and_equipment() {
        let dir = crate::paths::extracted();
        let (Ok(data), Ok(lengths)) = (std::fs::read(dir.join("chr/c0000.behbnd.d/c0000.hkx")), std::fs::read_to_string(dir.join("clip_lengths/c0000.json"))) else {
            eprintln!("no extracted c0000.hkx / clip_lengths/c0000.json: skipped");
            return;
        };
        let lengths: HashMap<String, f32> = serde_json::from_str(&lengths).unwrap();
        let graph = Arc::new(sekiro_formats::hkb::parse(&data).unwrap());
        let mut rt = BehaviorRuntime::new(graph, AnimOffsets { hands: [(71, 0), (50, 102)], ..AnimOffsets::default() });
        let mut play = |rt: &mut BehaviorRuntime, event: &str| {
            assert!(rt.fire_event(event), "{event}");
            rt.process_events(&mut |a: &str| lengths.get(a).copied());
            rt.main_clip().map(|c| c.animation.clone()).unwrap_or_default()
        };
        assert_eq!(play(&mut rt, "W_GroundJumpReady"), "a000_201001");
        assert_eq!(play(&mut rt, "W_GroundAttackCombo1"), "a050_300000");
        rt.set_variable("Selector_GroundJumpType", 1.0);
        assert_eq!(play(&mut rt, "W_GroundJumpReady"), "a000_201000");
        assert_eq!(play(&mut rt, "W_GroundSpecialAttackCombo1"), "a102_316000");
        assert_eq!(play(&mut rt, "W_GroundSubAttackCombo1"), "a071_400000");
    }
}

