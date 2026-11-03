//! Runs the soldier's real AI scripts against a small kinematic stand-in world.
//! Skips when `cache/` has not been extracted.

use std::path::PathBuf;

use sekiro_formats::param::ParamSet;

use super::lua50::{Value, Vm, load_chunk};
use super::runtime::AiEvent;
use super::{AiActor, AiBrain, AiWorld, ThinkParams};

fn cache() -> Option<PathBuf> {
    let c = crate::cache::root();
    c.join("raw/script/aicommon.luabnd.d").is_dir().then_some(c)
}

fn soldier_think(cache: &std::path::Path) -> ThinkParams {
    let set = ParamSet::load(
        &cache.join("raw/param/gameparam/gameparam.parambnd.d"),
        &cache.join("refs/paramdex/Defs"),
    )
    .unwrap();
    ThinkParams::from_row(&set.table("NpcThinkParam").unwrap().row(10100000).unwrap()).unwrap()
}

#[test]
fn every_ai_chunk_parses() {
    let Some(cache) = cache() else { return };
    let dir = cache.join("raw/script/aicommon.luabnd.d");
    let mut n = 0;
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        if e.path().extension().is_some_and(|x| x == "lua") {
            load_chunk(&std::fs::read(e.path()).unwrap())
                .unwrap_or_else(|err| panic!("{}: {err}", e.path().display()));
            n += 1;
        }
    }
    assert!(n > 90, "{n}");
}

/// The patrol steps only in the move goal on arrival at POINT_MOVE_POINT (#114) because no
/// shipped AI script calls `StepNextMovePoint`; NonBattleAct and RouteMove read the stepped
/// route back (GetPrevMovePointNumber, IsRouteEnd) after the move succeeds. A script that
/// did call it would step the route a second time per point.
#[test]
fn no_ai_script_steps_the_patrol_itself() {
    let Some(cache) = cache() else { return };
    let mut n = 0;
    for luabnd in std::fs::read_dir(cache.join("raw/script"))
        .unwrap()
        .flatten()
    {
        if !luabnd.path().is_dir() || !luabnd.file_name().to_string_lossy().ends_with(".luabnd.d") {
            continue;
        }
        for e in std::fs::read_dir(luabnd.path()).unwrap().flatten() {
            let data = std::fs::read(e.path()).unwrap();
            assert!(
                !data.windows(17).any(|w| w == b"StepNextMovePoint"),
                "{} calls StepNextMovePoint",
                e.path().display()
            );
            n += 1;
        }
    }
    assert!(n > 90, "{n}");
}

#[test]
fn luainfo_names_goals() {
    let Some(cache) = cache() else { return };
    let names = super::read_luainfo(
        &std::fs::read(cache.join("raw/script/aicommon.luabnd.d/aiCommon.luainfo")).unwrap(),
    );
    assert_eq!(names.get(&2100).map(String::as_str), Some("Attack"));
    assert_eq!(names.get(&2000).map(String::as_str), Some("Wait"));
    assert_eq!(names.get(&2200).map(String::as_str), Some("CommonAttack"));
}

#[test]
fn vm_runs_closures_loops_and_tables() {
    // A hand-assembled chunk is overkill; run a real one and check a constant it defines.
    let Some(cache) = cache() else { return };
    let mut vm = Vm::new();
    struct NoHost;
    impl super::lua50::Host for NoHost {
        fn call_host(
            &mut self,
            _: &mut Vm,
            _: &str,
            _: &[Value],
        ) -> super::lua50::LuaResult<Vec<Value>> {
            Ok(vec![])
        }
        fn call_method(
            &mut self,
            _: &mut Vm,
            _: super::lua50::Obj,
            _: &str,
            _: &[Value],
        ) -> super::lua50::LuaResult<Vec<Value>> {
            Ok(vec![])
        }
    }
    let p = load_chunk(
        &std::fs::read(cache.join("raw/script/aicommon.luabnd.d/ai_define.lua")).unwrap(),
    )
    .unwrap();
    vm.exec_chunk(&mut NoHost, p).unwrap();
    assert_eq!(vm.get_global("TARGET_ENE_0").num(), Some(0.0));
    assert_eq!(vm.get_global("POINT_INITIAL").num(), Some(100.0));
}

/// A minimal body: walks at 1.6 m/s and runs at 4.5 m/s along the requested yaw, turns
/// instantly, and plays a requested action as a 1.6 s animation with a combo window after
/// 0.9 s.
struct Dummy {
    pos: [f32; 3],
    yaw: f32,
    anim: Option<(i32, f32)>,
    guarding: f32,
}

impl Dummy {
    fn step(&mut self, c: &crate::character::NpcControl, dt: f32) {
        if let Some((_, t)) = &mut self.anim {
            *t += dt;
            if *t > 1.6 {
                self.anim = None;
            }
        }
        self.guarding = (self.guarding - dt).max(0.0);
        if c.action == 9910 {
            self.guarding = 0.2;
        } else if c.action != 0 {
            let combo_ok = self.anim.is_some_and(|(_, t)| t > 0.9);
            if self.anim.is_none() || combo_ok {
                self.anim = Some((c.action, 0.0));
            }
        }
        if self.anim.is_none() {
            if let Some(y) = c.face_yaw {
                self.yaw = y;
            }
            if let Some(y) = c.move_yaw {
                let v = if c.move_level > 0.75 { 4.5 } else { 1.6 } * c.move_level.clamp(0.5, 1.0);
                self.pos[0] -= y.sin() * v * dt;
                self.pos[2] -= y.cos() * v * dt;
            }
        }
    }
}

#[test]
fn soldier_approaches_and_attacks_with_its_own_ai() {
    let Some(cache) = cache() else { return };
    let think = soldier_think(&cache);
    assert_eq!((think.logic_id, think.battle_goal_id), (101000, 101000));
    let mut brain = AiBrain::load(&cache, "m11_00_00_00.luabnd.d", think, 7).unwrap();
    let errors: Vec<_> = brain
        .drain_log()
        .into_iter()
        .filter(|e| matches!(e, AiEvent::Error(_)))
        .collect();
    assert!(errors.is_empty(), "{errors:?}");
    let mut me = Dummy {
        pos: [0.0, 0.0, 0.0],
        yaw: 0.0,
        anim: None,
        guarding: 0.0,
    };
    let player = [0.0, 0.0, -12.0];
    let dt = 1.0 / 30.0;
    let mut actions = Vec::new();
    let mut min_dist = f32::MAX;
    let mut log = Vec::new();
    for frame in 0..(30 * 20) {
        // The player swings every 2.5 s once the soldier is close: 0.2 s of parry timing.
        let d_now = ((player[0] - me.pos[0]).powi(2) + (player[2] - me.pos[2]).powi(2)).sqrt();
        let parry_timing = d_now < 4.0 && frame % 75 < 6;
        let world = AiWorld {
            dt,
            me: AiActor {
                position: me.pos,
                yaw: me.yaw,
                hp: 195.0,
                max_hp: 195.0,
                posture: 75.0,
                max_posture: 75.0,
                sp_effects: vec![200030, 221002],
                guarding: me.guarding > 0.0,
            },
            target: Some(AiActor {
                position: player,
                yaw: 0.0,
                hp: 500.0,
                max_hp: 500.0,
                posture: 100.0,
                max_posture: 100.0,
                sp_effects: vec![],
                guarding: false,
            }),
            hit_radius: 0.4,
            current_anim: me.anim.map(|(a, _)| a),
            current_anim_time: me.anim.map_or(0.0, |(_, t)| t),
            combo_window: me.anim.is_some_and(|(_, t)| t > 0.9),
            home: [0.0; 3],
            parry_timing,
            target_attacking: parry_timing,
            damaged: false,
        };
        let c = brain.think(world);
        if c.action != 0 && c.action != 9910 && actions.last() != Some(&c.action) {
            actions.push(c.action);
        }
        me.step(&c, dt);
        let d = ((player[0] - me.pos[0]).powi(2) + (player[2] - me.pos[2]).powi(2)).sqrt();
        min_dist = min_dist.min(d);
        log.extend(brain.drain_log());
    }
    for e in log.iter().take(120) {
        eprintln!("{e:?}");
    }
    let errors: Vec<_> = log
        .iter()
        .filter(|e| matches!(e, AiEvent::Error(_)))
        .collect();
    assert!(errors.is_empty(), "{errors:#?}");
    eprintln!("actions {actions:?}, closest {min_dist:.2} m");
    assert!(min_dist < 6.0, "the soldier never closed in ({min_dist})");
    assert!(
        actions.iter().any(|a| (3000..3100).contains(a)),
        "no attack chosen: {actions:?}"
    );
    assert!(
        actions.iter().any(|a| *a == 3100 || *a == 3101),
        "never guarded or deflected: {actions:?}"
    );
}
