//! The game's own HavokScript run as the game runs it: the compiled scripts (action/script
//! c0000*.hks for Wolf, c9997.hks + c<chr>.hks for an enemy) in AKJama's HKS VM (sekiro-rs
//! crates/hks), hooked to the character's behaviour graph (hkb_runtime). `Host` is the engine's
//! side: the answers to `env(id[, arg])`, the variables, the events the script fires.
//! How it is driven and checked: docs/kb/hks-graph.md; the decompiled Lua check and the lockstep
//! tests are in hks_lua.rs.

use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::Path;
use std::rc::Rc;

/// act 148: SetVariable(name, value) (c0000.lua).
pub(crate) const ACT_SET_VARIABLE: i64 = 148;

/// The engine's side of one script run.
#[derive(Default)]
pub struct Host {
    /// `env(id)` / `env(id, arg)` answers (TRUE = 1); anything else answers FALSE (0) and is
    /// noted in `unanswered`.
    pub env: HashMap<(i64, Option<i64>), f64>,
    /// Behaviour variables: SetVariable writes them, `hkbGetVariable` reads them (0 when unset).
    pub vars: HashMap<String, f64>,
    /// The events fired this frame (`hkbFireEvent`).
    pub events: Vec<String>,
    /// The `act` ids called this frame.
    pub acts: Vec<i64>,
    /// The env ids asked but not answered, since the last `clear_frame`.
    pub unanswered: BTreeSet<i64>,
    /// The behaviour graph's active nodes (`hkbIsNodeActive`), from `ScriptChr`'s graph.
    pub nodes: HashSet<String>,
    /// The variables the compiled script set since `ScriptChr` last sent them to the graph.
    pub var_writes: Vec<String>,
}

impl Host {
    pub(crate) fn clear_frame(&mut self) {
        self.events.clear();
        self.acts.clear();
        self.unanswered.clear();
    }
}

/// The same player script as the game ships it: the compiled c0000*.hks (extracted/action/script)
/// in AKJama's HavokScript VM (sekiro-rs crates/hks), answered by the same `Host`. The decompiled
/// `HksPlayer` stays for reading and as a check of the decompile (the lockstep test runs both).
pub struct HksVm {
    vm: RefCell<sekiro_hks::Vm>,
    pub host: Rc<RefCell<Host>>,
}

/// `Host` as the VM's engine side.
struct VmHost<'a>(&'a mut Host);

fn vnum(v: Option<&sekiro_hks::Value>) -> Option<f64> {
    match v? {
        sekiro_hks::Value::Bool(b) => Some(*b as i64 as f64),
        v => v.as_number().map(f64::from),
    }
}

impl sekiro_hks::Host for VmHost<'_> {
    fn env(&mut self, id: sekiro_hks::CommandId<'_>, args: &[sekiro_hks::Value]) -> sekiro_hks::Value {
        // A named command (the aging check) is not answered: -1, as in `HksPlayer`.
        let id = match id {
            sekiro_hks::CommandId::Number(n) => n as i64,
            sekiro_hks::CommandId::Name(_) => -1,
        };
        let arg = vnum(args.first()).map(|a| a as i64);
        let host = &mut *self.0;
        let v = host.env.get(&(id, arg)).or_else(|| host.env.get(&(id, None))).copied();
        if v.is_none() {
            host.unanswered.insert(id);
        }
        sekiro_hks::Value::Number(v.unwrap_or(0.0) as f32)
    }

    fn act(&mut self, id: sekiro_hks::CommandId<'_>, args: &[sekiro_hks::Value]) -> sekiro_hks::Value {
        let id = match id {
            sekiro_hks::CommandId::Number(n) => n as i64,
            sekiro_hks::CommandId::Name(_) => -1,
        };
        self.0.acts.push(id);
        if id == ACT_SET_VARIABLE
            && let (Some(name), Some(v)) = (args.first().and_then(|a| a.as_str()), vnum(args.get(1)))
        {
            self.0.vars.insert(name.to_string(), v);
            self.0.var_writes.push(name.to_string());
        }
        sekiro_hks::Value::Nil
    }

    fn call(&mut self, name: &str, args: &[sekiro_hks::Value]) -> Vec<sekiro_hks::Value> {
        use sekiro_hks::Value as V;
        let first = args.first().and_then(|a| a.as_str()).map(str::to_string);
        match name {
            "hkbFireEvent" => {
                self.0.events.extend(first);
                Vec::new()
            }
            // c9997 DebugPrint writes `GetVariable("Debug<n>")[0]`: the graph's vector variables.
            "hkbGetVariable" if first.as_deref().is_some_and(|n| n.starts_with("Debug")) => vec![V::Table(sekiro_hks::table::Table::new_ref())],
            "hkbGetVariable" => vec![V::Number(first.and_then(|n| self.0.vars.get(&n).copied()).unwrap_or(0.0) as f32)],
            "hkbSetVariable" => {
                if let (Some(n), Some(v)) = (first, vnum(args.get(1))) {
                    self.0.var_writes.push(n.clone());
                    self.0.vars.insert(n, v);
                }
                Vec::new()
            }
            "hkbIsNodeActive" => vec![V::Bool(first.is_some_and(|n| self.0.nodes.contains(&n)))],
            // gap: the bone queries (bones at the origin), as in HksPlayer.
            "hkbIsBoneValid" => vec![V::Bool(false)],
            "hkbGetBoneModelSpace" | "hkbGetOldBoneModelSpace" => vec![V::Number(0.0); 3],
            _ => Vec::new(),
        }
    }
}

impl HksVm {
    /// Loads the compiled player scripts from `dir` (extracted/action/script) and runs `Initialize()`.
    pub fn load(dir: &Path) -> Result<Self, sekiro_hks::VmError> {
        Self::load_chunks(dir, sekiro_hks::PLAYER_SCRIPTS)
    }

    /// Loads these compiled chunks in order (an NPC: c9997.hks, then its own, as the exe loads
    /// them) and runs `Initialize()` when they have one.
    pub fn load_chunks(dir: &Path, chunks: &[&str]) -> Result<Self, sekiro_hks::VmError> {
        let host = Rc::new(RefCell::new(Host::default()));
        let mut vm = sekiro_hks::Vm::new();
        vm.seed(1);
        {
            let mut h = host.borrow_mut();
            let mut vh = VmHost(&mut h);
            for chunk in chunks {
                let data = std::fs::read(dir.join(chunk)).map_err(|e| sekiro_hks::VmError::NoSuchFunction(format!("{chunk}: {e}")))?;
                vm.load(&mut vh, chunk, &data)?;
            }
            if vm.has_function("Initialize") {
                vm.call_global(&mut vh, "Initialize", &[])?;
            }
        }
        Ok(Self { vm: RefCell::new(vm), host })
    }

    pub fn has_state(&self, state: &str) -> bool {
        self.vm.borrow().has_function(&format!("{state}_onUpdate"))
    }

    /// One frame in behaviour state `state`: `Update()`, then `<state>_onUpdate()`.
    pub fn frame(&self, state: &str) -> Result<Vec<String>, sekiro_hks::VmError> {
        let mut h = self.host.borrow_mut();
        h.clear_frame();
        let mut vh = VmHost(&mut h);
        let mut vm = self.vm.borrow_mut();
        vm.call_global(&mut vh, "Update", &[])?;
        vm.call_global(&mut vh, &format!("{state}_onUpdate"), &[])?;
        drop(vh);
        Ok(h.events.clone())
    }

    /// Calls a script global if the script has it; false if not.
    pub fn call(&self, name: &str) -> Result<bool, sekiro_hks::VmError> {
        let mut vm = self.vm.borrow_mut();
        if !vm.has_function(name) {
            return Ok(false);
        }
        let mut h = self.host.borrow_mut();
        vm.call_global(&mut VmHost(&mut h), name, &[]).map(|_| true)
    }
}

/// The most rounds of hooks and events one tick settles (sekiro-rs player.rs MAX_EVENT_ROUNDS).
const MAX_EVENT_ROUNDS: usize = 8;

/// A character as the game runs it: its compiled script hooked to its behaviour graph
/// (hkb_runtime; Wolf: c0000*.hks on c0000.hkx, an NPC: c9997.hks + its own on c9997.hkx). A
/// `tick` goes in the order sekiro-rs's player tick uses:
/// `Update()`, every active state's `_onUpdate`, the hooks (`_onDeactivate` / `_onActivate`) of
/// the states the events moved, until the graph settles, the ended clips' events, the clips'
/// advance and the ends that advance reached. The script's events and variables go to the graph
/// after each call; the graph's active nodes answer `hkbIsNodeActive`. In tests `check` (the decompiled
/// script) gets every call too, on the same answers; a call whose events differ goes to
/// `check_off`.
pub struct ScriptChr {
    pub vm: HksVm,
    pub rt: crate::hkb_runtime::BehaviorRuntime,
    #[cfg(test)]
    pub check: Option<crate::hks_lua::HksPlayer>,
    #[cfg(test)]
    pub check_off: Vec<String>,
    /// The script functions that ran this tick.
    pub calls: Vec<String>,
    /// How many of this tick's events went to the graph.
    sent: usize,
    /// Time spent in `tick` and how many ticks (the cost per character per frame).
    pub busy: std::time::Duration,
    pub ticks: u32,
    /// Of `busy`, the time in the script VM's calls.
    pub vm_busy: std::time::Duration,
    /// The graph's active nodes when `vm.host.nodes` was last filled.
    nodes_of: Vec<bool>,
}

impl ScriptChr {
    pub fn new(vm: HksVm, rt: crate::hkb_runtime::BehaviorRuntime) -> Result<Self, sekiro_hks::VmError> {
        #[cfg(test)]
        return Self::with_check(vm, rt, None);
        #[cfg(not(test))]
        Self::start(Self { vm, rt, calls: Vec::new(), sent: 0, busy: Default::default(), ticks: 0, vm_busy: Default::default(), nodes_of: Vec::new() })
    }

    /// `new` with the decompiled script run beside it as a check (`check_off`).
    #[cfg(test)]
    pub fn with_check(vm: HksVm, rt: crate::hkb_runtime::BehaviorRuntime, check: Option<crate::hks_lua::HksPlayer>) -> Result<Self, sekiro_hks::VmError> {
        Self::start(Self { vm, rt, check, check_off: Vec::new(), calls: Vec::new(), sent: 0, busy: Default::default(), ticks: 0, vm_busy: Default::default(), nodes_of: Vec::new() })
    }

    fn start(mut w: Self) -> Result<Self, sekiro_hks::VmError> {
        w.begin();
        let hooks = w.rt.initial_hooks();
        w.run(&hooks)?;
        Ok(w)
    }

    fn begin(&mut self) {
        self.vm.host.borrow_mut().clear_frame();
        #[cfg(test)]
        if let Some(c) = &self.check {
            c.host.borrow_mut().clear_frame();
        }
        self.calls.clear();
        self.sent = 0;
        self.sync_nodes();
        // The engine's own writes (the stick's move variables) since the last tick, then only the
        // script's.
        let mut h = self.vm.host.borrow_mut();
        h.var_writes.clear();
        for (k, v) in &h.vars {
            self.rt.set_variable(k, *v as f32);
        }
    }

    /// `hkbIsNodeActive`'s answers, rebuilt only when the graph's active nodes changed.
    fn sync_nodes(&mut self) {
        if self.rt.active_mask() != self.nodes_of.as_slice() {
            self.nodes_of = self.rt.active_mask().to_vec();
            self.vm.host.borrow_mut().nodes = self.rt.active_node_names();
        }
    }

    /// Runs the hooks' functions in order.
    fn run(&mut self, hooks: &[crate::hkb_runtime::HookCall]) -> Result<(), sekiro_hks::VmError> {
        for h in hooks {
            self.call_one(&h.function_name(), Some(&h.state))?;
        }
        Ok(())
    }

    /// One script call (nothing if the script has no such function). env 339 (the anim ended)
    /// is the graph's answer: the hook's own state's clip, else the full-body clip.
    fn call_one(&mut self, name: &str, state: Option<&str>) -> Result<(), sekiro_hks::VmError> {
        let ended = self.rt.anim_ended(0, state) as i64 as f64;
        #[cfg_attr(not(test), allow(unused_variables))]
        let before = {
            let mut h = self.vm.host.borrow_mut();
            for slot in [None, Some(0), Some(1)] {
                h.env.insert((339, slot), ended);
            }
            #[cfg(test)]
            if let Some(c) = &self.check {
                let mut ch = c.host.borrow_mut();
                ch.env = h.env.clone();
                ch.nodes = h.nodes.clone();
                ch.vars = h.vars.clone();
            }
            h.events.len()
        };
        let t0 = std::time::Instant::now();
        let ran = self.vm.call(name)?;
        self.vm_busy += t0.elapsed();
        if !ran {
            return Ok(());
        }
        self.calls.push(name.to_string());
        let mut h = self.vm.host.borrow_mut();
        let writes = std::mem::take(&mut h.var_writes);
        #[cfg(test)]
        if let Some(c) = &self.check {
            let theirs_before = c.host.borrow().events.len();
            if let Err(e) = c.call(name) {
                self.check_off.push(format!("{name}: {e}"));
            }
            let ch = c.host.borrow();
            if ch.events[theirs_before..] != h.events[before..] {
                self.check_off.push(format!("{name}: compiled {:?}, decompiled {:?}", &h.events[before..], &ch.events[theirs_before..]));
            }
        }
        for k in &writes {
            self.rt.set_variable(k, h.vars[k] as f32);
        }
        for e in &h.events[self.sent..] {
            self.rt.fire_event(e);
        }
        self.sent = h.events.len();
        Ok(())
    }

    fn settle(&mut self, durations: &mut dyn crate::hkb_runtime::ClipDurations) -> Result<(), sekiro_hks::VmError> {
        for _ in 0..MAX_EVENT_ROUNDS {
            if !self.rt.has_pending_events() && !self.rt.bindings_changed() {
                break;
            }
            let hooks = self.rt.process_events(durations);
            self.sync_nodes();
            self.run(&hooks)?;
        }
        Ok(())
    }

    /// The engine fires `event` on the graph itself (a throw pair's W_ThrowAtk<id>, a map
    /// event's anim): false if the graph has no such event.
    pub fn engine_event(&mut self, event: &str, durations: &mut dyn crate::hkb_runtime::ClipDurations) -> Result<bool, sekiro_hks::VmError> {
        if !self.rt.fire_event(event) {
            return Ok(false);
        }
        self.begin();
        self.settle(durations)?;
        Ok(true)
    }

    /// One frame of `dt` seconds on the answers already in `vm.host.env`; returns the events the
    /// script fired.
    pub fn tick(&mut self, dt: f32, durations: &mut dyn crate::hkb_runtime::ClipDurations) -> Result<Vec<String>, sekiro_hks::VmError> {
        let t0 = std::time::Instant::now();
        let out = self.tick_inner(dt, durations);
        self.busy += t0.elapsed();
        self.ticks += 1;
        out
    }

    fn tick_inner(&mut self, dt: f32, durations: &mut dyn crate::hkb_runtime::ClipDurations) -> Result<Vec<String>, sekiro_hks::VmError> {
        self.begin();
        self.call_one("Update", None)?;
        let updates = self.rt.update_hooks();
        self.run(&updates)?;
        self.settle(durations)?;
        if self.rt.queue_clip_end_events() > 0 {
            self.settle(durations)?;
        }
        self.rt.advance(dt);
        if self.rt.queue_clip_end_events() > 0 {
            self.settle(durations)?;
        }
        Ok(self.vm.host.borrow().events.clone())
    }
}
