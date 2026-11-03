//! Offline exploration startup, without arena entry-point shortcuts or combat side effects.
//!
//! Constructors 50 then 0 launch parameter-resolved local/common event instances.
//! Unknown predicates remain blocked and are reported, never guessed true.
//! Non-presentation actions are recorded as omitted; this is a startup snapshot, not a
//! gameplay event engine. See the first-visit fixture in docs/MAP-COMPLETENESS.md.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use sekiro_formats::{
    emedf,
    emevd::{Emevd, Instruction},
};
use serde::{Deserialize, Serialize};

use crate::arena::scene::SceneState;

pub const SUPPORTED_MAPS: [&str; 2] = ["m11_00_00_00", "m11_01_00_00"];

/// All flags not listed are off, including quest, pickup, interaction and event-slot flags.
/// Loaded objects answer backread=true; Wolf has no effects and occupies no encounter region.
#[derive(Debug, Clone, Default)]
pub struct FirstVisit {
    pub flags: BTreeSet<i32>,
    pub player_effects: BTreeSet<i32>,
    pub regions: BTreeSet<i32>,
    pub dead_characters: BTreeSet<i32>,
    /// Relevant quest/pickup inventory, keyed by EMEVD item type and item ID.
    pub inventory: BTreeSet<(u8, i32)>,
    /// Default offline event layer set. An instruction with a disjoint layer is skipped.
    pub layers: u32,
}

impl FirstVisit {
    pub fn for_map(map: &str) -> Option<Self> {
        SUPPORTED_MAPS.contains(&map).then(|| Self {
            // Castle is reached after Gyoubu, before Genichiro. Outskirts precedes Gyoubu.
            flags: if map == SUPPORTED_MAPS[1] {
                BTreeSet::from([11120100, 9301])
            } else {
                BTreeSet::from([11120100])
            },
            layers: u32::MAX,
            ..Self::default()
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum PartKind {
    Object,
    MapPiece,
    Collision,
    Enemy,
}

impl PartKind {
    fn from_layout(kind: &str) -> Option<Self> {
        match kind {
            "Object" => Some(Self::Object),
            "MapPiece" => Some(Self::MapPiece),
            "Collision" => Some(Self::Collision),
            "Enemy" => Some(Self::Enemy),
            _ => None,
        }
    }
}

/// A real target can be an entity ID or any of a part's eight entity-group IDs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VisibilityWrite {
    pub event: i64,
    pub slot: i32,
    pub kind: PartKind,
    pub target: i32,
    pub enabled: bool,
    pub order: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PartState {
    pub entity_id: i32,
    pub name: String,
    pub part_index: usize,
    pub kind: PartKind,
    /// Story enablement only. Residency, draw masks, LOD and helper handling remain separate.
    pub enabled: bool,
    /// Last applicable entity/group command, or None for authored startup default (enabled).
    pub source: Option<VisibilityWrite>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct Diagnostic {
    pub event: i64,
    pub slot: i32,
    pub instruction: usize,
    pub bank: i32,
    pub id: i32,
}

#[derive(Debug, Clone)]
pub struct WorldState {
    pub map: String,
    pub scene: SceneState,
    pub flags: BTreeSet<i32>,
    /// Latest command per typed entity/group target, in application order.
    pub visibility: Vec<VisibilityWrite>,
    pub unsupported: BTreeSet<Diagnostic>,
    /// Commands intentionally omitted from this visibility-only simulation.
    pub omitted: BTreeSet<Diagnostic>,
    pub started: usize,
    pub ended: usize,
    pub ticks: usize,
    /// Ordered, parameter-resolved map and object FX commands, including deletes.
    pub effects: Vec<EffectWrite>,
    pub effect_controls: Vec<EffectWrite>,
    pub bonfires: BTreeMap<i32, Bonfire>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Bonfire {
    pub flag: i32,
    pub kindling: i32,
    pub event: i64,
    pub slot: i32,
    pub instruction: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct EffectWrite {
    pub event: i64,
    pub slot: i32,
    pub instruction: usize,
    pub command: i32,
    pub entity: i32,
    pub dummy: i32,
    pub effect: i32,
}

impl WorldState {
    /// Resolve ordered entity/group overrides against the lossless job B placement table.
    /// Other kinds (including DummyObjects) do not acquire ordinary-part defaults here.
    pub fn parts(&self, layout: &serde_json::Value) -> Vec<PartState> {
        layout["parts"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|part| {
                let kind = PartKind::from_layout(part["kind"].as_str()?)?;
                let entity_id = part["entity_id"].as_i64().unwrap_or(-1) as i32;
                let groups: Vec<i32> = part["entity_group_ids"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|v| v.as_i64())
                    .filter(|&v| v > 0)
                    .map(|v| v as i32)
                    .collect();
                let source = self
                    .visibility
                    .iter()
                    .rev()
                    .find(|v| {
                        v.kind == kind
                            && v.target > 0
                            && (v.target == entity_id || groups.contains(&v.target))
                    })
                    .cloned();
                Some(PartState {
                    entity_id,
                    name: part["name"].as_str()?.to_owned(),
                    part_index: part["part_index"].as_u64()? as usize,
                    kind,
                    enabled: source.as_ref().is_none_or(|v| v.enabled),
                    source,
                })
            })
            .collect()
    }
}

#[derive(Default)]
struct Instance {
    file: usize,
    event: i64,
    slot: i32,
    code: Vec<Instruction>,
    pc: usize,
    groups: BTreeMap<i8, Vec<Instruction>>,
    finished: BTreeMap<i8, Option<bool>>,
    watched_flags: BTreeMap<i32, bool>,
    wait: f32,
    ended: bool,
}

struct Interpreter {
    files: Vec<Emevd>,
    fixture: FirstVisit,
    state: WorldState,
    instances: Vec<Instance>,
    starts: BTreeSet<(usize, i64, i32, Vec<u8>)>,
    order: u64,
}

fn decoded(ins: &Instruction) -> Vec<emedf::Arg> {
    emedf::decode(ins)
        .map(|(_, a)| a.into_iter().map(|(_, v)| v).collect())
        .unwrap_or_default()
}

fn range_state(flags: &BTreeSet<i32>, first: i32, last: i32, state: u8) -> bool {
    let mut values = (first..=last).map(|flag| flags.contains(&flag));
    match state {
        0 => values.all(|v| v),
        1 => values.all(|v| !v),
        2 => values.any(|v| v),
        3 => values.any(|v| !v),
        _ => false,
    }
}

fn compare(left: i32, right: i32, kind: u8) -> bool {
    match kind {
        0 => left == right,
        1 => left != right,
        2 => left > right,
        3 => left < right,
        4 => left >= right,
        5 => left <= right,
        _ => false,
    }
}

impl Interpreter {
    fn start(&mut self, file: usize, event: i64, slot: i32, args: &[u8]) -> Result<(), String> {
        if !self.starts.insert((file, event, slot, args.to_vec())) {
            return Ok(());
        }
        if self.instances.len() >= 4096 {
            return Err("world event instance budget exceeded".into());
        }
        let ev = self.files[file]
            .event(event)
            .ok_or_else(|| format!("missing event {event} in script {file}"))?;
        // Unlike the arena interpreter, every slot resolves from its own actual call bytes.
        for p in &ev.parameters {
            if p.source_start + p.byte_count > args.len() {
                return Err(format!(
                    "event {event} slot {slot}: missing parameter bytes"
                ));
            }
        }
        let code = ev.resolve(args);
        for (instruction, ins) in code
            .iter()
            .enumerate()
            .filter(|(_, i)| i.bank == 2006 && matches!(i.id, 1..=5))
        {
            let i = |offset| ins.i32_at(offset).unwrap_or(-1);
            self.state.effect_controls.push(EffectWrite {
                event,
                slot,
                instruction,
                command: ins.id,
                entity: i(if ins.id == 3 { 4 } else { 0 }),
                dummy: if ins.id == 4 { i(4) } else { -1 },
                effect: if ins.id == 4 {
                    i(8)
                } else if ins.id == 3 {
                    i(12)
                } else {
                    -1
                },
            });
        }
        self.instances.push(Instance {
            file,
            event,
            slot,
            code,
            pc: 0,
            groups: BTreeMap::new(),
            finished: BTreeMap::new(),
            watched_flags: BTreeMap::new(),
            wait: 0.0,
            ended: false,
        });
        Ok(())
    }

    fn flag(&self, r: &Instance, kind: u8, id: i32) -> bool {
        match kind {
            1 => self.state.flags.contains(&(r.event as i32)),
            2 => self.state.flags.contains(&(r.event as i32 + r.slot)),
            _ => self.state.flags.contains(&id),
        }
    }

    fn group(&self, r: &Instance, group: i8, depth: usize) -> Option<bool> {
        if depth > 32 {
            return None;
        }
        let Some(conditions) = r.groups.get(&group) else {
            return Some(false);
        };
        let values: Vec<_> = conditions
            .iter()
            .map(|ins| self.condition(r, ins, depth + 1))
            .collect();
        if group > 0 {
            if values.contains(&Some(false)) {
                Some(false)
            } else if values.contains(&None) {
                None
            } else {
                Some(true)
            }
        } else if values.contains(&Some(true)) {
            Some(true)
        } else if values.contains(&None) {
            None
        } else {
            Some(false)
        }
    }

    fn condition(&self, r: &Instance, ins: &Instruction, depth: usize) -> Option<bool> {
        let a = decoded(ins);
        let i = |n: usize| a.get(n).map_or(0, |v| v.as_i64());
        match ins.code() {
            (0, 1) => Some(compare(ins.i32_at(4)?, ins.i32_at(8)?, ins.u8_at(1)?)),
            (0, 0) => {
                let state = ins.u8_at(1)? != 0;
                self.group(r, ins.i8_at(2)?, depth).map(|v| v == state)
            }
            (3, 0) => {
                let id = ins.i32_at(4)?;
                Some(if ins.u8_at(1)? == 2 {
                    r.watched_flags.get(&id).is_some_and(|&initial| {
                        initial != self.flag(r, ins.u8_at(2).unwrap_or(0), id)
                    })
                } else {
                    self.flag(r, ins.u8_at(2)?, id) == (ins.u8_at(1)? != 0)
                })
            }
            (3, 1) => Some(range_state(
                &self.state.flags,
                ins.i32_at(4)?,
                ins.i32_at(8)?,
                ins.u8_at(1)?,
            )),
            (3, 2) => Some(self.fixture.regions.contains(&ins.i32_at(8)?) == (ins.u8_at(1)? != 0)),
            (3, 4) => Some(
                self.fixture
                    .inventory
                    .contains(&(ins.u8_at(1)?, ins.i32_at(4)?))
                    == (ins.u8_at(8)? != 0),
            ),
            (3, 5 | 24) | (5, 2) => Some(false), // No interaction occurs in startup.
            (3, 6) => Some(ins.i8_at(1)? == 0),  // Offline host in its own world.
            (3, 8) => Some(
                (ins.u8_at(2)? == 11
                    && ins.u8_at(3)?
                        == if self.state.map == SUPPORTED_MAPS[0] {
                            0
                        } else {
                            1
                        })
                    == (ins.u8_at(1)? != 0),
            ),
            (4, 0) => {
                Some(self.fixture.dead_characters.contains(&ins.i32_at(4)?) == (ins.u8_at(8)? != 0))
            }
            (4, 5) if i(1) == 10000 => {
                Some(self.fixture.player_effects.contains(&(i(2) as i32)) == (i(3) != 0))
            }
            (4, 5) => self
                .state
                .scene
                .character_effects
                .get(&(i(1) as i32, i(2) as i32))
                .map(|&on| on == (i(3) != 0)),
            (4, 7) => Some(
                self.state
                    .scene
                    .backread_enabled
                    .get(&ins.i32_at(4)?)
                    .copied()
                    .unwrap_or(true)
                    == (ins.u8_at(8)? != 0),
            ),
            (4, 8) => Some(ins.u8_at(12)? == 0), // No TAE message has fired.
            // Object backread readiness. The snapshot is evaluated after the full layout loads.
            (5, 10) => Some(ins.u8_at(8)? != 0),
            _ => None,
        }
    }

    fn diagnose(&mut self, r: &Instance, ins: &Instruction, omitted: bool) {
        let value = Diagnostic {
            event: r.event,
            slot: r.slot,
            instruction: r.pc,
            bank: ins.bank,
            id: ins.id,
        };
        if omitted {
            self.state.omitted.insert(value);
        } else {
            self.state.unsupported.insert(value);
        }
    }

    fn set_flag(&mut self, _r: &mut Instance, id: i32, state: u8) {
        let on = if state == 2 {
            !self.state.flags.contains(&id)
        } else {
            state != 0
        };
        if on {
            self.state.flags.insert(id)
        } else {
            self.state.flags.remove(&id)
        };
    }

    fn end(&mut self, r: &mut Instance, restart: u8) {
        if restart == 1 {
            r.pc = 0;
            r.groups.clear();
            r.finished.clear();
            r.watched_flags.clear();
            r.wait = 0.0;
        } else {
            r.ended = true;
            if r.event != 0 && r.event != 50 {
                self.set_flag(r, r.event as i32 + r.slot, 1);
            }
        }
    }

    fn jump(r: &mut Instance, label: u8) -> Result<(), String> {
        r.pc = r
            .code
            .iter()
            .position(|i| i.code() == (1014, i32::from(label)))
            .ok_or_else(|| format!("event {}: missing label {label}", r.event))?
            + 1;
        Ok(())
    }

    fn run(&mut self, r: &mut Instance) -> Result<(), String> {
        if r.ended {
            return Ok(());
        }
        if r.wait > 0.0 {
            r.wait -= 1.0 / 60.0;
            return Ok(());
        }
        for _ in 0..2048 {
            let Some(ins) = r.code.get(r.pc).cloned() else {
                self.end(r, 0);
                return Ok(());
            };
            if ins
                .layer
                .is_some_and(|mask| mask & self.fixture.layers == 0)
            {
                r.pc += 1;
                continue;
            }
            let a = decoded(&ins);
            // Presentation-state decoding never silently treats unknown packed arguments as zero.
            let i = |offset| ins.i32_at(offset).unwrap_or(0);
            let b = |offset| ins.u8_at(offset).unwrap_or(0);
            if (0..=5).contains(&ins.bank) && (ins.code() != (1, 0) || b(0) != 0) {
                if ins.code() == (3, 0) && b(1) == 2 {
                    let initial = self.flag(r, b(2), i(4));
                    r.watched_flags.entry(i(4)).or_insert(initial);
                }
                let group = ins.i8_at(0).unwrap_or(0);
                if self.condition(r, &ins, 0).is_none() {
                    self.diagnose(r, &ins, false);
                }
                if group == 0 {
                    if self.condition(r, &ins, 0) != Some(true) {
                        return Ok(());
                    }
                    r.finished = r.groups.keys().map(|&g| (g, self.group(r, g, 0))).collect();
                    r.groups.clear();
                    r.watched_flags.clear();
                } else {
                    r.groups.entry(group).or_default().push(ins);
                }
                r.pc += 1;
                continue;
            }
            let mut next = r.pc + 1;
            match ins.code() {
                (2000, 0) | (2000, 6) => {
                    let common = ins.id == 6;
                    self.start(
                        if common { 1 } else { r.file },
                        i(if common { 0 } else { 4 }) as i64,
                        if common { 0 } else { i(0) },
                        ins.args
                            .get(if common { 4 } else { 8 }..)
                            .unwrap_or_default(),
                    )?;
                }
                (1000, 4) => {
                    self.end(r, b(0));
                    return Ok(());
                }
                (1000, 3) => next += b(0) as usize,
                (1000, 0) => {
                    if self.group(r, b(1) as i8, 0) != Some(b(0) != 0) {
                        return Ok(());
                    }
                    r.finished = r.groups.keys().map(|&g| (g, self.group(r, g, 0))).collect();
                    r.groups.clear();
                }
                (1000, 5..=6) | (1000, 105) => {
                    if compare(i(4), i(8), b(1)) {
                        if ins.id == 5 {
                            next += b(0) as usize;
                        } else if ins.id == 6 {
                            self.end(r, b(0));
                            return Ok(());
                        } else {
                            Self::jump(r, b(0))?;
                            continue;
                        }
                    }
                }
                (1000, 1..=2) | (1000, 7..=8) | (1000, 101 | 107) => {
                    let value = if matches!(ins.id, 7 | 8 | 107) {
                        r.finished.get(&(b(2) as i8)).copied().flatten()
                    } else {
                        self.group(r, b(2) as i8, 0)
                    };
                    let Some(value) = value else {
                        self.diagnose(r, &ins, false);
                        return Ok(());
                    };
                    if value == (b(1) != 0) {
                        if matches!(ins.id, 1 | 7) {
                            next += b(0) as usize;
                        } else if matches!(ins.id, 2 | 8) {
                            self.end(r, b(0));
                            return Ok(());
                        } else {
                            Self::jump(r, b(0))?;
                            continue;
                        }
                    }
                }
                (1000, 103) => {
                    Self::jump(r, b(0))?;
                    continue;
                }
                (1001, 0..=1) | (1, 0) => {
                    r.wait = if ins.bank == 1 {
                        ins.f32_at(4).unwrap_or(0.0)
                    } else if ins.id == 0 {
                        ins.f32_at(0).unwrap_or(0.0)
                    } else {
                        i(0) as f32 / 60.0
                    };
                    r.pc = next;
                    return Ok(());
                }
                (1003, 0) => {
                    if self.flag(r, b(1), i(4)) != (b(0) != 0) {
                        return Ok(());
                    }
                }
                (1003, 12 | 13) if b(1) == 0 => {
                    if ins.id == 12 {
                        Self::jump(r, b(0))?;
                        continue;
                    }
                    self.end(r, b(0));
                    return Ok(());
                }
                (1003, 12 | 13) => {} // Player is in its own world, so IsNotIn=true fails.
                (1003, 1..=4) | (1003, 101..=102) => {
                    let value = if matches!(ins.id, 3 | 4 | 102) {
                        range_state(&self.state.flags, i(4), i(8), b(1))
                    } else {
                        self.flag(r, b(2), i(4)) == (b(1) != 0)
                    };
                    if value {
                        match ins.id {
                            1 | 3 => next += b(0) as usize,
                            2 | 4 => {
                                self.end(r, b(0));
                                return Ok(());
                            }
                            _ => {
                                Self::jump(r, b(0))?;
                                continue;
                            }
                        }
                    }
                }
                (1014, _) | (2000, 2) => {}
                (2003, 2 | 62) => self.set_flag(r, i(0), b(4)),
                (2003, 22 | 63) => {
                    for id in i(0)..=i(4) {
                        self.set_flag(r, id, b(8));
                    }
                }
                (2005, 7) => {
                    self.state.scene.animations.insert(i(0), (i(4), false));
                }
                (2006, 1..=5) => {
                    self.state.effects.push(EffectWrite {
                        event: r.event,
                        slot: r.slot,
                        instruction: r.pc,
                        command: ins.id,
                        entity: i(if ins.id == 3 { 4 } else { 0 }),
                        dummy: if ins.id == 4 { i(4) } else { -1 },
                        effect: if ins.id == 4 {
                            i(8)
                        } else if ins.id == 3 {
                            i(12)
                        } else {
                            -1
                        },
                    });
                    self.state.scene.instruction(&ins, &a);
                }
                (2009, 3) => {
                    self.state.bonfires.insert(
                        i(4),
                        Bonfire {
                            flag: i(0),
                            kindling: i(16),
                            event: r.event,
                            slot: r.slot,
                            instruction: r.pc,
                        },
                    );
                }
                (2005, 3) | (2012, 1) | (2011, 1) | (2004, 5) => {
                    let kind = match ins.bank {
                        2005 => PartKind::Object,
                        2012 => PartKind::MapPiece,
                        2011 => PartKind::Collision,
                        _ => PartKind::Enemy,
                    };
                    self.order += 1;
                    self.state
                        .visibility
                        .retain(|v| v.kind != kind || v.target != i(0));
                    self.state.visibility.push(VisibilityWrite {
                        event: r.event,
                        slot: r.slot,
                        kind,
                        target: i(0),
                        enabled: b(4) != 0,
                        order: self.order,
                    });
                    self.state.scene.instruction(&ins, &a);
                }
                _ if ins.bank < 2000 => {
                    self.diagnose(r, &ins, false);
                    return Ok(());
                }
                _ => {
                    if !self.state.scene.instruction(&ins, &a) {
                        self.diagnose(r, &ins, true);
                    }
                }
            }
            r.pc = next;
        }
        Err(format!(
            "event {} slot {} exceeded instruction budget",
            r.event, r.slot
        ))
    }
}

/// Run the settled startup of the two supported maps. No filesystem writes or game session.
/// Twelve seconds allows restore sequences to finish; long interaction waits remain parked.
pub fn first_visit(cache: &Path, map: &str) -> Result<Option<WorldState>, String> {
    let Some(fixture) = FirstVisit::for_map(map) else {
        return Ok(None);
    };
    evaluate(cache, map, fixture).map(Some)
}

pub fn evaluate(cache: &Path, map: &str, fixture: FirstVisit) -> Result<WorldState, String> {
    if !SUPPORTED_MAPS.contains(&map) {
        return Err(format!("unsupported story map {map}"));
    }
    let mut files = Vec::new();
    for name in [map, "common_func", "common"] {
        let path = cache.join(format!("raw/event/{name}.emevd"));
        files.push(
            Emevd::parse(&std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?)
                .map_err(|e| e.to_string())?,
        );
    }
    evaluate_files(files, map, fixture)
}

/// Useful for independent, small EMEVD fixtures. Script order is map, common_func, common.
pub fn evaluate_files(
    files: Vec<Emevd>,
    map: &str,
    fixture: FirstVisit,
) -> Result<WorldState, String> {
    if files.len() != 3 {
        return Err("expected map, common_func and common scripts".into());
    }
    let state = WorldState {
        map: map.into(),
        scene: SceneState::default(),
        flags: fixture.flags.clone(),
        visibility: Vec::new(),
        unsupported: BTreeSet::new(),
        omitted: BTreeSet::new(),
        started: 0,
        ended: 0,
        ticks: 720,
        effects: Vec::new(),
        effect_controls: Vec::new(),
        bonfires: BTreeMap::new(),
    };
    let mut vm = Interpreter {
        files,
        fixture,
        state,
        instances: Vec::new(),
        starts: BTreeSet::new(),
        order: 0,
    };
    // Preconstructor children settle before the constructor, as they do on loading.
    for file in [2, 0] {
        if vm.files[file].event(50).is_some() {
            vm.start(file, 50, 0, &[])?;
        }
    }
    for phase in 0..2 {
        if phase == 1 {
            for file in [2, 0] {
                if vm.files[file].event(0).is_some() {
                    vm.start(file, 0, 0, &[])?;
                }
            }
        }
        for _ in 0..if phase == 0 { 2 } else { 720 } {
            let mut k = 0;
            while k < vm.instances.len() {
                let mut instance = std::mem::take(&mut vm.instances[k]);
                vm.run(&mut instance)?;
                vm.instances[k] = instance;
                k += 1;
            }
        }
    }
    vm.state.started = vm.instances.len();
    vm.state.ended = vm.instances.iter().filter(|r| r.ended).count();
    Ok(vm.state)
}
