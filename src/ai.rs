//! Enemy brains: FromSoftware's AI scripts (decompiled into extracted/ai_src by
//! tools/extract) running in an embedded Lua on our goal runtime (src/ai/runtime.lua).
//! Rust gives the brain a world snapshot each fixed frame and gets back one action
//! request (attack anim, move direction, turn), which enemy.rs carries out.

use mlua::{Function, Lua, Table};
use std::collections::HashSet;
use std::path::Path;

/// What the brain sees this frame.
#[derive(Default)]
pub struct Snapshot {
    pub dist: f32,
    /// Signed angle (deg) from the enemy's facing to the target, positive to the right.
    pub angle: f32,
    /// Bearing (deg) of the enemy around the target, for SidewayMove.
    pub bearing: f32,
    pub target_guard: bool,
    pub sp_self: HashSet<i64>,
    pub sp_target: HashSet<i64>,
    pub sp_new: Vec<i64>,
    pub hp_rate: f32,
    pub target_hp_rate: f32,
    /// Posture left (Sekiro's "stamina"): max - accumulated posture damage.
    pub sp: f32,
    pub sp_rate: f32,
    /// The current anim is idle or locomotion (any action may start).
    pub free: bool,
    /// Active NPC cancel flags on the current anim (23 combo, 86 attack, 79 step, 78 move).
    pub flags: Vec<i64>,
    /// EzState id of the AI-requested anim that is playing, its clock and whether it ended.
    pub ez: Option<i64>,
    pub t: f32,
    pub anim_done: bool,
    pub ez_started: Option<i64>,
    pub ez_failed: Option<i64>,
    /// Something other than the AI replaced its action (reaction, parry).
    pub interrupted: bool,
    /// The player's attack just raised the AI notify (INTERUPT_ParryTiming).
    pub parry_timing: bool,
    /// ConsecutiveGuardCount (engine-side clash count, timer 13).
    pub guard_count: i32,
    /// The player just started using an item (INTERUPT_UseItem).
    pub target_use_item: bool,
}

/// One action request.
#[derive(Default, Debug, Clone, PartialEq)]
pub struct Command {
    pub anim: Option<i64>,
    /// 'F', 'B', 'L', 'R'
    pub mv: Option<char>,
    pub walk: bool,
    pub face: bool,
    pub turn: bool,
}

pub struct Brain {
    lua: Lua,
    pub log: Vec<String>,
}

/// Script load order: constants and goal ids first, then the common goal/function
/// files, then the character's battle goal.
fn script_order(dir: &Path, battle_goal: i64) -> Vec<std::path::PathBuf> {
    let first = ["common_ai_define.lua", "common_goal_list.lua", "common_logic_list.lua", "common_event_list.lua"];
    let mut out: Vec<_> = first.iter().map(|f| dir.join(f)).filter(|p| p.exists()).collect();
    let mut rest: Vec<_> = std::fs::read_dir(dir)
        .map(|rd| rd.filter_map(|e| e.ok().map(|e| e.path())).collect())
        .unwrap_or_default();
    rest.sort();
    for p in rest {
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
        if name.starts_with("common_") && name.ends_with(".lua") && !first.contains(&name.as_str()) {
            out.push(p);
        }
    }
    out.push(dir.join(format!("{battle_goal}_battle.lua")));
    out
}

/// Repairs a decompiler artifact: `break` inside a numeric for loop comes out as
/// an assignment of the loop variable to its upper bound (`i = n`), which does not
/// leave the loop in Lua (Common_Battle_Activate / Common_Kengeki_Activate would then
/// run every act after the chosen one). Lines that are not UTF-8 are kept as-is.
pub fn fix_decompiled(src: &[u8]) -> Vec<u8> {
    let mut fors: Vec<(String, String)> = Vec::new();
    let mut out = Vec::with_capacity(src.len());
    for line in src.split(|&b| b == b'\n') {
        let mut replaced = false;
        if let Ok(text) = std::str::from_utf8(line) {
            let t = text.trim();
            if let Some(rest) = t.strip_prefix("for ").and_then(|r| r.strip_suffix(", 1 do")) {
                if let Some((var, range)) = rest.split_once(" = ") {
                    if let Some((_, bound)) = range.split_once(", ") {
                        fors.push((var.to_string(), bound.to_string()));
                    }
                }
            } else if let Some((lhs, rhs)) = t.split_once(" = ") {
                if fors.iter().any(|(v, b)| v == lhs && b == rhs) {
                    let indent = &text[..text.len() - text.trim_start().len()];
                    out.extend_from_slice(format!("{indent}break").as_bytes());
                    replaced = true;
                }
            }
        }
        if !replaced {
            out.extend_from_slice(line);
        }
        out.push(b'\n');
    }
    out.pop();
    out
}

impl Brain {
    pub fn load(dir: &Path, battle_goal: i64, think_id: i64, hit_radius: f32, seed: u32) -> Result<Self, String> {
        let lua = Lua::new();
        let mut log = Vec::new();
        lua.load(include_str!("ai/runtime.lua")).set_name("runtime.lua").exec().map_err(|e| e.to_string())?;
        let battle = dir.join(format!("{battle_goal}_battle.lua"));
        if !battle.exists() {
            return Err(format!("{} missing", battle.display()));
        }
        for path in script_order(dir, battle_goal) {
            let Ok(src) = std::fs::read(&path) else { continue };
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            let src = fix_decompiled(&src);
            if let Err(e) = lua.load(&src[..]).set_name(name.as_str()).exec() {
                log.push(format!("{name}: {e}"));
            }
        }
        // The guard count is kept engine-side (clash SpEffects 200210/1/5/6 in combat.rs).
        lua.load("function Get_ConsecutiveGuardCount(ai) return W.guard_count or 0 end").exec().map_err(|e| e.to_string())?;
        let g = lua.globals();
        let run = || -> mlua::Result<()> {
            g.get::<Function>("AI_BindFunctionGoals")?.call::<()>(())?;
            let w: Table = g.get("W")?;
            w.set("battle_goal", battle_goal)?;
            w.set("think_id", think_id)?;
            w.set("hit_radius", hit_radius)?;
            let math: Table = g.get("math")?;
            math.get::<Function>("randomseed")?.call::<()>(seed)?;
            Ok(())
        };
        run().map_err(|e| e.to_string())?;
        Ok(Brain { lua, log })
    }

    pub fn tick(&mut self, s: &Snapshot, dt: f32) -> Command {
        match self.try_tick(s, dt) {
            Ok(c) => c,
            Err(e) => {
                let msg = e.to_string();
                if self.log.last() != Some(&msg) {
                    self.log.push(msg);
                }
                Command::default()
            }
        }
    }

    fn try_tick(&mut self, s: &Snapshot, dt: f32) -> mlua::Result<Command> {
        let lua = &self.lua;
        let g = lua.globals();
        let w: Table = g.get("W")?;
        let set = |v: &HashSet<i64>| -> mlua::Result<Table> {
            let t = lua.create_table()?;
            for id in v {
                t.set(*id, true)?;
            }
            Ok(t)
        };
        w.set("dist", s.dist)?;
        w.set("angle", s.angle)?;
        w.set("bearing", s.bearing)?;
        w.set("target_guard", s.target_guard)?;
        w.set("sp_self", set(&s.sp_self)?)?;
        w.set("sp_target", set(&s.sp_target)?)?;
        w.set("sp_new", lua.create_sequence_from(s.sp_new.iter().copied())?)?;
        w.set("hp_rate", s.hp_rate)?;
        w.set("target_hp_rate", s.target_hp_rate)?;
        w.set("sp", s.sp)?;
        w.set("sp_rate", s.sp_rate)?;
        w.set("free", s.free)?;
        let flags = lua.create_table()?;
        for f in &s.flags {
            flags.set(*f, true)?;
        }
        w.set("flags", flags)?;
        w.set("ez", s.ez)?;
        w.set("t", s.t)?;
        w.set("anim_done", s.anim_done)?;
        w.set("ez_started", s.ez_started)?;
        w.set("ez_failed", s.ez_failed)?;
        w.set("interrupted", s.interrupted)?;
        w.set("parry_timing", s.parry_timing)?;
        w.set("guard_count", s.guard_count)?;
        w.set("use_item", s.target_use_item)?;
        g.get::<Function>("AI_Tick")?.call::<()>(dt)?;
        let cmd: Table = g.get("CMD")?;
        let logs: Table = g.get("AI_LOG")?;
        for line in logs.sequence_values::<String>().flatten() {
            self.log.push(line);
        }
        g.set("AI_LOG", lua.create_table()?)?;
        Ok(Command {
            anim: cmd.get::<Option<i64>>("anim")?,
            mv: cmd.get::<Option<String>>("move")?.and_then(|m| m.chars().next()),
            walk: cmd.get::<Option<bool>>("walk")?.unwrap_or(false),
            face: cmd.get::<Option<bool>>("face")?.unwrap_or(false),
            turn: cmd.get::<Option<bool>>("turn")?.unwrap_or(false),
        })
    }

    /// The active goal chain, e.g. "GOAL_SamuraiTaisho_102000_Battle > 2221 > 2200(3000)".
    pub fn describe(&self) -> String {
        self.lua
            .globals()
            .get::<Function>("AI_Describe")
            .and_then(|f| f.call::<String>(()))
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn brain() -> Option<Brain> {
        let dir = crate::paths::root().join("extracted/ai_src");
        if !dir.exists() {
            return None;
        }
        Some(Brain::load(&dir, 102000, 10200000, 0.5, 7).expect("load"))
    }

    #[test]
    fn decompiler_break_is_repaired() {
        let src = b"for i = 1, n, 1 do\n    if x then\n        f()\n        i = n\n    end\nend";
        let out = String::from_utf8(fix_decompiled(src)).unwrap();
        assert!(out.contains("        break\n") && !out.contains("i = n"), "{out}");
    }

    #[test]
    fn scripts_load_and_pick_an_act() {
        let Some(mut b) = brain() else { return };
        let errors: Vec<_> = b.log.iter().filter(|l| !l.starts_with("stub:")).cloned().collect();
        assert!(errors.is_empty(), "{errors:#?}");
        // Far away and free: the battle goal must approach (move) or start an attack.
        let mut s = Snapshot { dist: 8.0, free: true, hp_rate: 1.0, target_hp_rate: 1.0, sp: 1000.0, sp_rate: 1.0, ..Default::default() };
        s.sp_self.insert(200004); // battle state
        let mut acted = false;
        for _ in 0..30 {
            let c = b.tick(&s, 1.0 / 60.0);
            if c.anim.is_some() || c.mv.is_some() {
                acted = true;
                break;
            }
        }
        let errors: Vec<_> = b.log.iter().filter(|l| !l.starts_with("stub:")).cloned().collect();
        assert!(errors.is_empty(), "{errors:#?}");
        assert!(acted, "no action; goals: {}; log: {:?}", b.describe(), b.log);
    }

    /// cargo test ai_stubs -- --ignored --nocapture : engine API the scripts reached that is stubbed.
    #[test]
    #[ignore]
    fn ai_stubs() {
        let Some(mut b) = brain() else { return };
        for i in 0..2000 {
            let mut s = Snapshot { dist: (i % 9) as f32, free: i % 3 == 0, hp_rate: 1.0, target_hp_rate: 1.0, sp: 1000.0, sp_rate: 1.0, ..Default::default() };
            s.sp_self.insert(200004);
            if i % 50 == 0 {
                s.sp_self.insert([200200, 200201, 200205, 200206, 200210, 200211, 200215][(i / 50) % 7]);
                s.interrupted = true;
            }
            s.flags = vec![23, 86, 79, 78];
            s.anim_done = i % 7 == 0;
            b.tick(&s, 1.0 / 60.0);
        }
        println!("{:#?}", b.log);
    }

    #[test]
    fn parry_timing_runs_goal_parry() {
        let Some(mut b) = brain() else { return };
        // Free, facing the player 2 m away, no clash history: Goal.Parry -> EndureAttack 3100 (guard).
        let mut s = Snapshot { dist: 2.0, free: true, hp_rate: 1.0, target_hp_rate: 1.0, sp: 1000.0, sp_rate: 1.0, parry_timing: true, ..Default::default() };
        s.sp_self.insert(200004);
        s.sp_self.insert(221000); // parry rank 0 (c1020 resident)
        let c = b.tick(&s, 1.0 / 60.0);
        assert_eq!(c.anim, Some(3100), "{c:?}; goals: {}; log: {:?}", b.describe(), b.log);
        // A thrust (109970) from a rank-0 parrier is always deflected (3101).
        let Some(mut b) = brain() else { return };
        s.sp_target.insert(109970);
        let c = b.tick(&s, 1.0 / 60.0);
        assert_eq!(c.anim, Some(3101), "{c:?}; goals: {}", b.describe());
    }

    #[test]
    fn deflected_enemy_counters_with_kengeki() {
        let Some(mut b) = brain() else { return };
        // Enemy got deflected (200200) while 1.5 m away: Kengeki picks a counter
        // (3060/3061/3063, ComboFinal) that may start at the bound anim's flag 23.
        let mut s = Snapshot { dist: 1.5, hp_rate: 1.0, target_hp_rate: 1.0, sp: 1000.0, sp_rate: 1.0, interrupted: true, ..Default::default() };
        s.sp_self.insert(200004);
        s.sp_self.insert(200200);
        let c = b.tick(&s, 1.0 / 60.0);
        assert_eq!(c.anim, None, "must wait for the cancel flag");
        s.interrupted = false;
        s.flags = vec![23];
        let c = b.tick(&s, 1.0 / 60.0);
        assert!(matches!(c.anim, Some(3060 | 3061 | 3063)), "{c:?}; goals: {}; log: {:?}", b.describe(), b.log);
    }
}
