//! Trial (2026-10-19): the game's own player script (action/script c0000*.hks, decompiled into
//! `extracted/hks_src/*.lua` from the player's install) run in standard Lua 5.1 (mlua), with Rust
//! answering its engine questions (`env`) and taking its orders (`act`, `hkbFireEvent`).
//! Not wired into play: the tests show which `W_...` event the game's logic picks from a state
//! for given answers, so player.rs can be checked against it.
//!
//! How the game drives the script (c0000.lua): each frame `Update()`, then the active behaviour
//! state's `<State>_onUpdate()` -> `UpdateState` (Control, Validate, FireStateEndEvent), which
//! fires the next state's event through `hkbFireEvent`. The engine side it needs is small:
//! `env(id[, arg])`, `act(id, ...)`, `hkbGetVariable` / `act(148, name, value)` (SetVariable),
//! `hkbFireEvent`, `hkbIsNodeActive` and three bone calls.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;

use mlua::{Lua, Value, Variadic};

pub use crate::hks::*;

/// The script files in load order (c0000_define first: the constants the others use).
const FILES: [&str; 4] = ["c0000_define.lua", "c0000.lua", "c0000_transition.lua", "c0000_cmsg.lua"];

fn int(v: Option<&Value>) -> Option<i64> {
    match v? {
        Value::Integer(i) => Some(*i as i64),
        Value::Number(n) => Some(*n as i64),
        Value::Boolean(b) => Some(*b as i64),
        _ => None,
    }
}

fn num(v: Option<&Value>) -> Option<f64> {
    match v? {
        Value::Integer(i) => Some(*i as f64),
        Value::Number(n) => Some(*n),
        Value::Boolean(b) => Some(*b as i64 as f64),
        _ => None,
    }
}

/// The game's player script in a Lua 5.1 state.
pub struct HksPlayer {
    lua: Lua,
    pub host: Rc<RefCell<Host>>,
}

impl HksPlayer {
    /// Loads the four c0000 scripts from `dir` (extracted/hks_src) and runs `Initialize()`.
    pub fn load(dir: &Path) -> mlua::Result<Self> {
        Self::load_files(dir, &FILES)
    }

    /// Loads these decompiled scripts in order (an NPC: c9997.lua, then its own) and runs
    /// `Initialize()` when they have one.
    pub fn load_files(dir: &Path, files: &[&str]) -> mlua::Result<Self> {
        let lua = Lua::new();
        let host = Rc::new(RefCell::new(Host::default()));
        let g = lua.globals();
        let h = host.clone();
        g.set(
            "env",
            lua.create_function(move |_, args: Variadic<Value>| {
                let id = int(args.first()).unwrap_or(-1);
                let arg = int(args.get(1));
                let host = &mut *h.borrow_mut();
                let v = host.env.get(&(id, arg)).or_else(|| host.env.get(&(id, None))).copied();
                if v.is_none() {
                    host.unanswered.insert(id);
                }
                Ok(v.unwrap_or(0.0))
            })?,
        )?;
        let h = host.clone();
        g.set(
            "act",
            lua.create_function(move |_, args: Variadic<Value>| {
                let id = int(args.first()).unwrap_or(-1);
                let host = &mut *h.borrow_mut();
                host.acts.push(id);
                if let Some(v) = num(args.get(1)) {
                    host.act_values.insert(id, v);
                }
                if id == ACT_SET_VARIABLE
                    && let (Some(Value::String(name)), Some(v)) = (args.get(1), num(args.get(2)))
                {
                    host.vars.insert(name.to_string_lossy(), v);
                }
                Ok(())
            })?,
        )?;
        let h = host.clone();
        g.set("hkbGetVariable", lua.create_function(move |_, name: String| Ok(h.borrow().vars.get(&name).copied().unwrap_or(0.0)))?)?;
        let h = host.clone();
        g.set("hkbFireEvent", lua.create_function(move |_, name: String| {
            h.borrow_mut().events.push(name);
            Ok(())
        })?)?;
        let h = host.clone();
        g.set("hkbIsNodeActive", lua.create_function(move |_, name: String| Ok(h.borrow().nodes.contains(&name)))?)?;
        // gap: the bone queries (bones at the origin).
        g.set("hkbIsBoneValid", lua.create_function(|_, _: Variadic<Value>| Ok(false))?)?;
        for f in ["hkbGetBoneModelSpace", "hkbGetOldBoneModelSpace", "hkbSetBoneModelSpace"] {
            g.set(f, lua.create_function(|_, _: Variadic<Value>| Ok((0.0, 0.0, 0.0)))?)?;
        }
        for f in files {
            let src = std::fs::read_to_string(dir.join(f)).map_err(mlua::Error::external)?;
            lua.load(src.trim_start_matches('\u{feff}')).set_name(*f).exec()?;
        }
        if let Value::Function(init) = lua.globals().raw_get::<Value>("Initialize")? {
            init.call::<()>(())?;
        }
        Ok(Self { lua, host })
    }

    /// Does the script have callbacks for behaviour state `state`? (Unknown globals read as the
    /// script's `dummy` function, c0000.lua's _G metatable, so the table is read raw.)
    pub fn has_state(&self, state: &str) -> bool {
        matches!(self.lua.globals().raw_get::<Value>(format!("{state}_onUpdate")), Ok(Value::Function(_)))
    }

    /// One frame in behaviour state `state` (e.g. "StandIdle"): `Update()`, then
    /// `<state>_onUpdate()`. Returns the events the script fired.
    pub fn frame(&self, state: &str) -> mlua::Result<Vec<String>> {
        self.host.borrow_mut().clear_frame();
        let g = self.lua.globals();
        g.get::<mlua::Function>("Update")?.call::<()>(())?;
        g.get::<mlua::Function>(format!("{state}_onUpdate"))?.call::<()>(())?;
        Ok(self.host.borrow().events.clone())
    }

    /// Calls a script global if it is a function (read raw, as `has_state`); false if none.
    pub fn call(&self, name: &str) -> mlua::Result<bool> {
        match self.lua.globals().raw_get::<Value>(name)? {
            Value::Function(f) => f.call::<()>(()).map(|_| true),
            _ => Ok(false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn player() -> Option<HksPlayer> {
        let dir = crate::paths::extracted().join("hks_src");
        if !dir.join("c0000.lua").exists() {
            eprintln!("no extracted/hks_src: skipped");
            return None;
        }
        Some(HksPlayer::load(&dir).expect("the game's player script loads in Lua 5.1"))
    }

    // c0000_define.lua: ACTION_ARM_ATTACK 0, ACTION_ARM_JUMP 4; env 1106 = an action requested,
    // 207 = arm style (1 = the sword in one hand), 1105 = IsStayState.
    const ENV_ACTION: i64 = 1106;
    const ENV_ARM_STYLE: i64 = 207;
    const ENV_STAY: i64 = 1105;

    /// A healthy Wolf standing with the sword in hand (env names: sekiro-rs docs/HKS-INTERFACE.md):
    /// 1000 HP, 201 landed, 333 the frame's milliseconds, 3037 free to leave the crouch
    /// collision, 3033 every action unlocked, 207 the sword in one hand, 1105 standing still,
    /// 225 the right hand's weapon motion category (WEP_MOTION_CATEGORY_050, the Kusabimaru).
    fn standing(h: &mut Host) {
        for (id, v) in [(1000, 100.0), (201, 1.0), (333, 1000.0 / 30.0), (3037, 1.0), (3033, 1.0), (ENV_ARM_STYLE, 1.0), (ENV_STAY, 1.0), (225, 50.0)] {
            h.env.insert((id, None), v);
        }
    }

    /// The game's script picks, from standing idle: nothing without input, a jump on the jump
    /// press, the first slash on the attack press, a walk start with the stick pushed.
    #[test]
    fn the_games_player_script_picks_from_standing_idle() {
        let Some(p) = player() else { return };
        let run = |set: &dyn Fn(&mut Host)| {
            {
                let mut h = p.host.borrow_mut();
                h.env.clear();
                h.vars.clear();
                standing(&mut h);
                set(&mut h);
            }
            let events = p.frame("StandIdle").expect("StandIdle_onUpdate runs");
            eprintln!("events {events:?}; env asked but unanswered {:?}", p.host.borrow().unanswered);
            events
        };
        assert!(run(&|_| {}).is_empty(), "idle without input fires nothing");
        let jump = run(&|h| {
            h.env.insert((ENV_ACTION, Some(4)), 1.0);
        });
        assert!(jump.iter().any(|e| e.contains("Jump")), "{jump:?}");
        let attack = run(&|h| {
            h.env.insert((ENV_ACTION, Some(0)), 1.0);
        });
        assert!(attack.iter().any(|e| e.contains("Attack")), "{attack:?}");
        let walk = run(&|h| {
            h.vars.insert("MoveSpeedLevel".into(), 0.5);
        });
        assert!(walk.iter().any(|e| e.contains("Move")), "{walk:?}");
    }
}

/// The comparison: one press (or the stick) from standing idle, put through the game's script and
/// through player.rs in the sim, both seeing the same Wolf. The script's answers are taken from
/// the sim's Wolf (`answers`), so a difference is a difference in the decision, not in the setup.
#[cfg(test)]
mod compare {
    use super::*;
    use crate::sim_tests::*;

    // c0000_define.lua ACTION_ARM_*.
    const ARM_ATTACK: i64 = 0;
    const ARM_SUB_ATTACK: i64 = 1;
    const ARM_GUARD: i64 = 2;
    const ARM_JUMP: i64 = 4;
    const ARM_SP_MOVE: i64 = 5;
    const ARM_USE_ITEM: i64 = 7;
    const ARM_CROUCH: i64 = 15;
    const ARM_SHINOBI_WEP_ACTION: i64 = 18;
    const ARM_SPECIAL_ATTACK: i64 = 32;
    // HAND_LEFT / HAND_RIGHT.
    const LEFT: i64 = 0;
    const RIGHT: i64 = 1;
    /// EquipParamWeapon row of the Kusabimaru (wepmotionCategory 50, no art).
    const KUSABIMARU: i64 = 5000;

    /// One case's input: a press and the buttons held with it, or the stick.
    struct Input {
        press: Option<Action>,
        stick: Vec2,
        /// Wolf's Spirit Emblems, when not the sim's own.
        emblems: Option<u32>,
    }

    /// The buttons the script sees for an input: env 1106 (requested this frame) and 1108 (held).
    /// A combat art is the attack press with the guard held (BEH_A_GROUND_SP_ATTACK).
    fn buttons(press: Option<Action>) -> (Vec<i64>, Vec<i64>) {
        match press {
            Some(Action::Attack) => (vec![ARM_ATTACK], vec![ARM_ATTACK]),
            Some(Action::Guard) => (vec![ARM_GUARD], vec![ARM_GUARD]),
            Some(Action::CombatArt) => (vec![ARM_ATTACK], vec![ARM_ATTACK, ARM_GUARD]),
            Some(Action::Jump) => (vec![ARM_JUMP], vec![ARM_JUMP]),
            Some(Action::Step) => (vec![ARM_SP_MOVE], vec![ARM_SP_MOVE]),
            Some(Action::UseItem) => (vec![ARM_USE_ITEM], vec![ARM_USE_ITEM]),
            Some(Action::Prosthetic) => (vec![ARM_SUB_ATTACK], vec![ARM_SUB_ATTACK]),
            Some(Action::Crouch) => (vec![ARM_CROUCH], vec![ARM_CROUCH]),
            _ => (Vec::new(), Vec::new()),
        }
    }

    /// The engine's answers for Wolf as the sim has him (env names: sekiro-rs docs/HKS-INTERFACE.md).
    fn answers(app: &mut App, h: &mut Host, input: &Input) {
        let mut q = app.world_mut().query_filtered::<(&Actor, &Player), With<Player>>();
        let world = app.world();
        let combat = world.resource::<Combat>();
        let config = world.resource::<crate::config::GameConfig>();
        let (a, p) = q.single(world).unwrap();
        let weapon = |row: i64, field: &str| combat.param("EquipParamWeapon", row)[field].as_i64().unwrap_or(-1) as f64;
        let goods = crate::items::in_slot(config, p.quick_slot);
        let have_item = if goods == 3000 { p.gourd > 0 } else { goods > 0 && crate::items::count(config, goods) > 0 };
        let item_anim = crate::items::param("EquipParamGoods", goods)["goodsUseAnim"].as_i64().unwrap_or(-1) as f64;
        let tool = crate::player::equipped_tool(combat, config, p.tool_slot);
        let emblems = |cost: u32| (config.player.infinite_emblems || p.emblems >= cost) as i64 as f64;
        let mut set = |id: i64, arg: Option<i64>, v: f64| {
            h.env.insert((id, arg), v);
        };
        set(1000, None, a.hp as f64);
        set(201, None, (!a.airborne) as i64 as f64);
        set(333, None, 1000.0 / 60.0);
        set(207, None, if a.sheathed { 0.0 } else { 1.0 });
        set(1105, None, crate::player::is_free(&a.state) as i64 as f64);
        set(225, Some(RIGHT), weapon(KUSABIMARU, "wepmotionCategory"));
        set(225, Some(LEFT), tool.map_or(-1.0, |t| t.group as f64));
        // 3035: the action can be paid for (the Spirit Emblems of the tool / the art).
        set(3035, Some(ARM_SHINOBI_WEP_ACTION), tool.map_or(0.0, |t| emblems(t.emblems)));
        set(3035, Some(ARM_SPECIAL_ATTACK), emblems(crate::player::art_cost(combat, config)));
        set(345, Some(RIGHT), weapon(config.player.combat_art, "spAtkcategory"));
        set(231, None, item_anim);
        set(233, None, have_item as i64 as f64);
        // gap: every action unlocked and free to leave the crouch collision (no story locks in the sim).
        set(3033, None, 1.0);
        set(3037, None, 1.0);
        let (pressed, held) = buttons(input.press);
        for b in pressed {
            set(1106, Some(b), 1.0);
        }
        for b in held {
            set(1108, Some(b), 1.0);
        }
        crate::hks_wolf::stick_vars(h, input.stick);
    }

    /// The state the script's event leads to and the sim's first new state. An event `W_X` enters
    /// the behaviour graph's state X, or one of its variants (the jump's direction, the step's):
    /// the sim's state names X inside it.
    fn agree(event: Option<&str>, ours: Option<&str>) -> bool {
        match (event, ours) {
            (None, None) => true,
            (Some(e), Some(o)) => o.contains(e.trim_start_matches("W_")),
            _ => false,
        }
    }

    #[test]
    fn the_games_script_and_player_rs_pick_the_same_from_idle() {
        let dir = crate::paths::extracted().join("hks_src");
        if !dir.join("c0000.lua").exists() {
            eprintln!("no extracted/hks_src: skipped");
            return;
        }
        let script = HksPlayer::load(&dir).expect("the player script loads");
        let none = Vec2::ZERO;
        // The sim's camera looks down -Z and Wolf faces +Z: forward on the stick is -Y.
        let cases = [
            ("nothing", Input { press: None, stick: none, emblems: None }),
            ("attack", Input { press: Some(Action::Attack), stick: none, emblems: None }),
            ("jump", Input { press: Some(Action::Jump), stick: none, emblems: None }),
            ("guard", Input { press: Some(Action::Guard), stick: none, emblems: None }),
            ("step", Input { press: Some(Action::Step), stick: none, emblems: None }),
            ("crouch", Input { press: Some(Action::Crouch), stick: none, emblems: None }),
            ("gourd", Input { press: Some(Action::UseItem), stick: none, emblems: None }),
            ("prosthetic", Input { press: Some(Action::Prosthetic), stick: none, emblems: None }),
            ("combat art", Input { press: Some(Action::CombatArt), stick: none, emblems: None }),
            ("tool, 0 emblems", Input { press: Some(Action::Prosthetic), stick: none, emblems: Some(0) }),
            ("art, 0 emblems", Input { press: Some(Action::CombatArt), stick: none, emblems: Some(0) }),
            ("walk forward", Input { press: None, stick: Vec2::new(0.0, -0.5), emblems: None }),
            ("run forward", Input { press: None, stick: Vec2::new(0.0, -1.0), emblems: None }),
        ];
        let mut table = Vec::new();
        for (name, input) in &cases {
            let mut app = app();
            for _ in 0..10 {
                app.update();
            }
            assert_eq!(player_state(&mut app).0, "StandIdle", "{name}: the sim's Wolf starts idle");
            if let Some(n) = input.emblems {
                let world = app.world_mut();
                world.resource_mut::<crate::config::GameConfig>().player.infinite_emblems = false;
                let mut q = world.query_filtered::<&mut Player, With<Player>>();
                q.single_mut(world).unwrap().emblems = n;
            }
            {
                let mut h = script.host.borrow_mut();
                h.env.clear();
                h.vars.clear();
                answers(&mut app, &mut h, input);
            }
            let events = script.frame("StandIdle").expect("StandIdle_onUpdate runs");
            let unanswered = script.host.borrow().unanswered.len();
            if let Some(a) = input.press {
                app.world_mut().resource_mut::<PadInput>().press(a);
            }
            let mut ours = None;
            for _ in 0..6 {
                {
                    let mut pad = app.world_mut().resource_mut::<PadInput>();
                    pad.stick = input.stick;
                    pad.walk = input.stick != Vec2::ZERO && input.stick.length() <= 0.7;
                    pad.guard_held = matches!(input.press, Some(Action::Guard | Action::CombatArt));
                }
                app.update();
                let mut s = player_state(&mut app).0;
                if s != "StandIdle" {
                    // player.rs's one Locomotion state plays the behaviour graph's StandMoveStart
                    // clip first when the move began from a standstill (Actor::move_start).
                    if s == "Locomotion" {
                        let mut q = app.world_mut().query_filtered::<&Actor, With<Player>>();
                        let start = q.single(app.world()).unwrap().move_start > 0.0;
                        s = (if start { "Locomotion (StandMoveStart)" } else { "Locomotion (StandMoveLoop)" }).into();
                    }
                    ours = Some(s);
                    break;
                }
            }
            let event = events.first().map(String::as_str);
            let same = agree(event, ours.as_deref());
            eprintln!("{:<16} {:<4} script {:<26} player.rs {:<30} ({unanswered} env unanswered)", name, if same { "same" } else { "DIFF" }, event.unwrap_or("-"), ours.as_deref().unwrap_or("-"));
            table.push((*name, same, event.map(str::to_string), ours));
        }
        // gap: the known differences, each with its reason; a new difference fails, and a fixed
        // one fails too until it is taken off this list.
        let known: [(&str, &str); 0] = [];
        for (name, same, event, ours) in &table {
            let listed = known.iter().any(|k| k.0 == *name);
            assert!(*same != listed, "{name}: script {event:?}, player.rs {ours:?} ({})", if listed { "now the same: take it off `known`" } else { "a new difference" });
        }
    }
}

/// The comparison frame by frame: Wolf as the game runs him (`ScriptChr`: the game's compiled
/// script on its behaviour graph) beside player.rs in the sim, the script's answers taken from the
/// sim's Wolf at that frame (the anim's TAE windows and SpEffect refs, the buffered presses, the
/// held buttons). Each frame the clip the game's graph plays is compared with the one player.rs
/// shows. Through combos, the guard, the air, arts and tools, steps, crouch, gourd and the stick.
#[cfg(test)]
mod lockstep {
    use super::*;
    use crate::sim_tests::*;
    use std::collections::HashMap;

    use crate::hks_wolf::{KUSABIMARU, Pad, stick_vars};

    /// The engine's answers for Wolf as the sim has him at the start of this frame
    /// (hks_wolf::answers, this frame's hit taken off him).
    fn answers(app: &mut App, h: &mut Host, pad: &Pad) {
        let damage = {
            let world = app.world_mut();
            world.query_filtered::<&mut Actor, With<Player>>().single_mut(world).unwrap().damage_env.take()
        };
        let mut q = app.world_mut().query_filtered::<(&Actor, &Player, &Transform), With<Player>>();
        let world = app.world();
        let (a, p, tf) = q.single(world).unwrap();
        let feet = tf.translation.y - crate::player::CAPSULE_HALF_HEIGHT - crate::map::ground_y(tf.translation);
        let combat = world.resource::<Combat>();
        let config = world.resource::<crate::config::GameConfig>();
        let locked = world.resource::<crate::camera::LockOn>().target.is_some();
        crate::hks_wolf::answers(h, a, p, combat, config, locked, pad, damage, Some(feet));
    }

    /// The behaviour graph c0000.hkx for the graph runtime (hkb_runtime).
    fn hkb_graph() -> Option<std::sync::Arc<sekiro_formats::hkb::Graph>> {
        let data = std::fs::read(crate::paths::extracted().join("chr/c0000.behbnd.d/c0000.hkx")).ok()?;
        sekiro_formats::hkb::parse(&data).ok().map(std::sync::Arc::new)
    }

    /// The clip offsets the exe reads for Wolf with this (combat art, tool) equipped: the right
    /// hand the katana's wepmotionCategory with the art row's spAtkcategory, the left hand the
    /// tool's (hkb_runtime::AnimOffsets).
    fn offsets((art, tool): (i64, i64)) -> crate::hkb_runtime::AnimOffsets {
        let data: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(crate::paths::extracted().join("combat_data.json")).unwrap()).unwrap();
        let field = |row: i64, f: &str| data["params"]["EquipParamWeapon"][row.to_string()][f].as_i64().unwrap_or(-1);
        crate::hkb_runtime::AnimOffsets { hands: [(field(tool, "wepmotionCategory"), field(tool, "spAtkcategory").max(0)), (field(KUSABIMARU, "wepmotionCategory"), field(art, "spAtkcategory").max(0))], ..Default::default() }
    }

    /// The game's clip against the one player.rs shows, frame by frame. `early`: the game
    /// already shows the clip player.rs shows the next frame (it leaves an ended clip one frame
    /// sooner: when Havok sends a clip's end is not traced yet).
    #[derive(Default)]
    struct ClipCheck {
        same: usize,
        early: usize,
        off: Vec<String>,
    }

    /// One run: `frames` frames of `input` (frame -> pad) with Wolf as the game runs him
    /// (`ScriptChr`: the compiled script on the graph, its answers from the sim's Wolf at that
    /// frame) beside player.rs in the sim. `lengths`: every clip's length (clip_lengths/c0000.json),
    /// else the sim's (a clip playing another's HKX); none for a clip the game lacks (the exe's
    /// clip pick asks whether the chr has it). Returns the sim's (state, shown clip) per frame.
    fn run(wolf: &mut ScriptChr, lengths: &HashMap<String, f32>, clips: &mut ClipCheck, equip: (i64, i64), frames: usize, input: &dyn Fn(usize) -> Pad) -> Vec<(String, String)> {
        let mut app = app();
        {
            let mut config = app.world_mut().resource_mut::<crate::config::GameConfig>();
            config.player.combat_art = equip.0;
            config.player.prosthetics = vec![equip.1];
        }
        // No enemy in reach: its guard would turn the slashes into deflected recoils, reactions
        // the script only learns from the damage answers (202, 256), not given here.
        let world = app.world_mut();
        let mut q = world.query_filtered::<(&mut Enemy, &mut Transform), With<Enemy>>();
        for (mut e, mut t) in q.iter_mut(world) {
            e.disabled = true;
            t.translation.z = 300.0;
        }
        for _ in 0..10 {
            app.update();
        }
        // The game starts Wolf with the map entry (InitState: W_GroundMapEnter, a000_700010);
        // the sim starts him standing, so the game's side plays its entry out first.
        for _ in 0..600 {
            if wolf.rt.main_state_name() == "StandIdle" {
                break;
            }
            answers(&mut app, &mut wolf.vm.host.borrow_mut(), &Pad::default());
            let d = &app.world().resource::<Combat>().player;
            wolf.tick(1.0 / 60.0, &mut |anim: &str| lengths.get(anim).copied().or_else(|| d.anim(anim).map(|_| d.length(anim)))).expect("the script runs");
        }
        assert_eq!(wolf.rt.main_state_name(), "StandIdle", "the game's map entry ends standing");
        let mut shown: Vec<(String, String, String)> = Vec::new();
        let mut out = Vec::new();
        for f in 0..frames {
            let pad = input(f);
            let before = player_state(&mut app).0;
            {
                let mut h = wolf.vm.host.borrow_mut();
                answers(&mut app, &mut h, &pad);
                stick_vars(&mut h, pad.stick);
            }
            let events = {
                let d = &app.world().resource::<Combat>().player;
                wolf.tick(1.0 / 60.0, &mut |anim: &str| lengths.get(anim).copied().or_else(|| d.anim(anim).map(|_| d.length(anim)))).expect("the script runs")
            };
            {
                let mut p = app.world_mut().resource_mut::<PadInput>();
                for a in &pad.press {
                    p.press(*a);
                }
                p.guard_held = pad.guard_held;
                p.stick = pad.stick;
                p.walk = pad.stick != Vec2::ZERO && pad.stick.length() <= 0.7;
            }
            app.update();
            let after = player_state(&mut app).0;
            let ours = {
                let mut q = app.world_mut().query_filtered::<&Actor, With<Player>>();
                q.single(app.world()).unwrap().shown_clip(&app.world().resource::<Combat>().player).0
            };
            let theirs = wolf.rt.main_clip().map(|c| c.animation.clone()).unwrap_or_default();
            shown.push((ours.clone(), theirs, format!("f{f} {before} -> {after}, script {events:?} ({})", wolf.rt.main_state_name())));
            out.push((after, ours));
        }
        for (i, (ours, theirs, at)) in shown.iter().enumerate() {
            if ours == theirs {
                clips.same += 1;
            } else if shown.get(i + 1).is_some_and(|n| &n.0 == theirs) {
                clips.early += 1;
            } else {
                clips.off.push(format!("{at}: player.rs {ours}, game {theirs}"));
            }
        }
        out
    }

    fn press_at(frames: &[(usize, Action)], guard: Option<(usize, usize)>, stick: Option<(usize, usize, Vec2)>) -> impl Fn(usize) -> Pad + '_ {
        move |f| Pad {
            press: frames.iter().filter(|p| p.0 == f).map(|p| p.1).collect(),
            held: Vec::new(),
            guard_held: guard.is_some_and(|(a, b)| f >= a && f < b),
            stick: stick.filter(|(a, b, _)| f >= *a && f < *b).map_or(Vec2::ZERO, |s| s.2),
        }
    }

    /// One run's input.
    struct Run {
        name: String,
        presses: Vec<(usize, Action)>,
        guard: Option<(usize, usize)>,
        stick: Option<(usize, usize, Vec2)>,
        /// (combat art, prosthetic tool)
        equip: (i64, i64),
    }

    #[test]
    fn the_games_script_and_player_rs_agree_frame_by_frame() {
        let dir = crate::paths::extracted().join("hks_src");
        if !dir.join("c0000.lua").exists() {
            eprintln!("no extracted/hks_src: skipped");
            return;
        }
        let Some(hkb) = hkb_graph() else {
            eprintln!("no extracted/chr/c0000.behbnd.d/c0000.hkx: skipped");
            return;
        };
        // Whirlwind Slash (a100) and the Loaded Shuriken (a070).
        const EQUIP: (i64, i64) = (5100, 70000);
        let mut runs: Vec<Run> = Vec::new();
        let mut push = |name: String, presses: Vec<(usize, Action)>, guard: Option<(usize, usize)>, stick: Option<(usize, usize, Vec2)>, equip: (i64, i64)| runs.push(Run { name, presses, guard, stick, equip });
        // Combos: a second (and third) slash at a sweep of delays.
        for k in [6, 10, 14, 18, 22, 26, 30] {
            push(format!("combo, 2nd slash at {k}"), vec![(0, Action::Attack), (k, Action::Attack)], None, None, EQUIP);
        }
        push("combo, 3 slashes".into(), vec![(0, Action::Attack), (16, Action::Attack), (32, Action::Attack)], None, None, EQUIP);
        // The guard: held from frame 0, then a press while held, then let go.
        for (name, act) in [("attack", Action::Attack), ("step", Action::Step), ("jump", Action::Jump), ("prosthetic", Action::Prosthetic)] {
            push(format!("guard held, {name} at 20"), vec![(0, Action::Guard), (20, act)], Some((0, 40)), None, EQUIP);
        }
        push("guard tapped".into(), vec![(0, Action::Guard)], Some((0, 4)), None, EQUIP);
        // The air: a jump, then a press on the way up and on the way down.
        for k in [12, 30] {
            for (name, act) in [("attack", Action::Attack), ("guard", Action::Guard), ("prosthetic", Action::Prosthetic), ("jump", Action::Jump)] {
                push(format!("jump, {name} at {k}"), vec![(0, Action::Jump), (k, act)], None, None, EQUIP);
            }
        }
        // Other arts and tools: the clip group comes from the equipped row (offsetType 13 / 14).
        for art in [5300, 5600] {
            push(format!("art {art}"), vec![(0, Action::CombatArt)], None, None, (art, 70000));
        }
        for tool in [71000, 72000] {
            push(format!("tool {tool}"), vec![(0, Action::Prosthetic)], None, None, (5100, tool));
        }
        // The sim's camera looks down -Z and Wolf faces +Z: forward on the stick is -Y.
        let forward = Vec2::new(0.0, -1.0);
        push("step".into(), vec![(0, Action::Step)], None, None, EQUIP);
        push("step, attack at 15".into(), vec![(0, Action::Step), (15, Action::Attack)], None, None, EQUIP);
        push("crouch, attack at 20".into(), vec![(0, Action::Crouch), (20, Action::Attack)], None, None, EQUIP);
        push("gourd".into(), vec![(0, Action::UseItem)], None, None, EQUIP);
        push("walk, stop at 30".into(), Vec::new(), None, Some((0, 30, forward * 0.5)), EQUIP);
        push("run, stop at 30".into(), Vec::new(), None, Some((0, 30, forward)), EQUIP);
        push("run, attack at 20".into(), vec![(20, Action::Attack)], None, Some((0, 40, forward)), EQUIP);
        push("run, jump at 20".into(), vec![(20, Action::Jump)], None, Some((0, 40, forward)), EQUIP);
        let Ok(lengths) = std::fs::read_to_string(crate::paths::extracted().join("clip_lengths/c0000.json")) else {
            eprintln!("no extracted/clip_lengths/c0000.json (sekiro-extract clip-lengths): skipped");
            return;
        };
        let lengths: HashMap<String, f32> = serde_json::from_str(&lengths).unwrap();
        let mut clips = ClipCheck::default();
        let mut check_off = Vec::new();
        let (mut cost, mut vm_cost, mut ticks) = (std::time::Duration::ZERO, std::time::Duration::ZERO, 0u32);
        for r in &runs {
            let vm = HksVm::load(&crate::paths::extracted().join("action/script")).expect("the compiled player script loads");
            let decompiled = HksPlayer::load(&dir).expect("the decompiled player script loads");
            let rt = crate::hkb_runtime::BehaviorRuntime::new(hkb.clone(), offsets(r.equip));
            // LOCKSTEP_NO_CHECK: without the decompiled check (the cost alone).
            let decompiled = std::env::var("LOCKSTEP_NO_CHECK").is_err().then_some(decompiled);
            let mut wolf = ScriptChr::with_check(vm, rt, decompiled).expect("the start hooks run");
            let n = clips.off.len();
            let frames = run(&mut wolf, &lengths, &mut clips, r.equip, 60, &press_at(&r.presses, r.guard, r.stick));
            let mut path: Vec<&str> = frames.iter().map(|f| f.1.as_str()).collect();
            path.dedup();
            eprintln!("{}: {path:?}", r.name);
            for o in clips.off.iter().skip(n).take(4) {
                eprintln!("  off: {o}");
            }
            check_off.extend(wolf.check_off.drain(..).map(|c| format!("{}: {c}", r.name)));
            cost += wolf.busy;
            vm_cost += wolf.vm_busy;
            ticks += wolf.ticks;
        }
        eprintln!("the game's script + graph for Wolf: {:.1} us a tick ({:.1} in the VM) over {ticks}", cost.as_secs_f64() * 1e6 / ticks.max(1) as f64, vm_cost.as_secs_f64() * 1e6 / ticks.max(1) as f64);
        eprintln!("{} runs: the game's clip vs player.rs's: {} frames the same, {} a frame early at a clip's end, {} differ", runs.len(), clips.same, clips.early, clips.off.len());
        eprintln!("decompiled script vs the compiled one: {} calls differ {check_off:?}", check_off.len());
        assert!(clips.off.is_empty(), "frames where the game (script + graph) shows another clip than player.rs: {:?}", clips.off);
        assert!(check_off.is_empty(), "calls where the decompiled script differs from the game's bytecode: {check_off:?}");
    }
}

/// The enemy side (list item 3, c9997): the General (c1020) as the game runs it, its compiled
/// c9997.hks + c1020.hks on c1020.behbnd's c9997.hkx, beside our enemy.rs in the sim. The
/// script's answers come from the sim's enemy at that frame: its AI's request (enemy.rs
/// `ai_cmd`: env 106, the cancel slots, the move variables), the TAE of the clip on screen, its
/// SpEffects and AI state. Each frame the graph's clip is compared with the sim's.
#[cfg(test)]
mod enemy_lockstep {
    use super::*;
    use crate::sim_tests::*;

    /// The engine's answers for the enemy as the sim has it now (hks_enemy::answers).
    fn answers(app: &mut App, h: &mut Host, clip: Option<(String, f32)>, anims: &[i64], request: Option<i64>, damage: Option<crate::enemy::DamageEnv>, throw_start: bool) {
        let (pos, escaped) = {
            let mut q = app.world_mut().query_filtered::<(&Transform, &Actor), With<Player>>();
            let (tf, pa) = q.single(app.world()).unwrap();
            (tf.translation, pa.state == "ThrowEscape")
        };
        let mut q = app.world_mut().query_filtered::<(&Actor, &Enemy, &Transform), Without<Player>>();
        let world = app.world();
        let combat = world.resource::<Combat>();
        let (a, e, tf) = q.iter(world).find(|(_, e, _)| !e.disabled).expect("an enemy");
        crate::hks_enemy::answers(h, a, e, &e.ai_cmd, tf.translation, crate::hks_enemy::WolfSeen { pos, escaped }, combat, clip, anims, request, damage, throw_start);
    }

    fn enemy_request(app: &mut App) -> Option<i64> {
        let mut q = app.world_mut().query_filtered::<&Enemy, Without<Player>>();
        q.iter(app.world()).find(|e| !e.disabled).and_then(|e| e.ai_cmd.anim)
    }

    fn enemy_clip(app: &mut App) -> (String, String) {
        let mut q = app.world_mut().query_filtered::<(&Actor, &Enemy), Without<Player>>();
        let world = app.world();
        let combat = world.resource::<Combat>();
        let (a, _) = q.iter(world).find(|(_, e)| !e.disabled).expect("an enemy");
        (a.state.clone(), a.shown_clip(combat.data_of(a)).0)
    }

    /// Every regular enemy's event anims (a000_02xxxx), each forced as a map script does
    /// (EzState: script.rs play_event, Mode::Rising), Wolf far away: the state the game's graph and
    /// c9997 + c<chr>.hks go on to at the clip's end against enemy.rs's. `SHINOBI_LOCKSTEP_CHRS`
    /// picks chrs. Returns (chr, event, game's next, enemy.rs's next) for each that differs.
    fn event_ends(chrs: &[&str]) -> Vec<(String, i64, String, String)> {
        let x = crate::paths::extracted();
        let mut off = Vec::new();
        for &id in chrs {
            let (Ok(data), Ok(lengths)) = (std::fs::read(x.join(format!("chr/{id}.behbnd.d/c9997.hkx"))), std::fs::read_to_string(x.join(format!("clip_lengths/{id}.json")))) else { continue };
            let lengths: HashMap<String, f32> = serde_json::from_str(&lengths).unwrap();
            let graph = std::sync::Arc::new(sekiro_formats::hkb::parse(&data).unwrap());
            let mut events: Vec<i64> = lengths.keys().filter_map(|k| k.strip_prefix("a000_02")?.parse::<i64>().ok()).map(|n| 20000 + n).collect();
            events.sort_unstable();
            events.dedup();
            for ev in events {
                let own = format!("{id}.hks");
                let vm = HksVm::load_chunks(&x.join("action/script"), &["c9997.hks", own.as_str()]).unwrap();
                let rt = crate::hkb_runtime::BehaviorRuntime::new(graph.clone(), crate::hkb_runtime::AnimOffsets::default());
                let mut chr = ScriptChr::with_check(vm, rt, None).unwrap();
                let mut app = crate::sim_tests::app_with(id);
                app.world_mut().remove_non_send::<crate::hks_enemy::EnemyScripts>();
                {
                    let mut q = app.world_mut().query_filtered::<&mut Transform, With<Player>>();
                    for mut t in q.iter_mut(app.world_mut()) {
                        t.translation.z = 300.0;
                    }
                }
                let anims: Vec<i64> = lengths.keys().filter_map(|k| {
                    let (cat, num) = k.strip_prefix('a')?.split_once('_')?;
                    Some(cat.parse::<i64>().ok()? * 1_000_000 + num.parse::<i64>().ok()?)
                }).collect();
                let mut dur = |anim: &str| lengths.get(anim).copied();
                answers(&mut app, &mut chr.vm.host.borrow_mut(), None, &anims, None, None, false);
                chr.tick(1.0 / 60.0, &mut dur).unwrap();
                if !chr.rt.fire_event(&format!("W_Event{ev}")) {
                    continue;
                }
                {
                    let mut q = app.world_mut().query_filtered::<(&mut Actor, &mut Enemy), Without<Player>>();
                    let world = app.world_mut().as_unsafe_world_cell();
                    // SAFETY: Combat is read while the query writes Actor / Enemy (disjoint).
                    let combat = unsafe { world.get_resource::<Combat>().unwrap() };
                    let (mut a, mut e) = q.iter_mut(unsafe { world.world_mut() }).find(|(_, e)| !e.disabled).unwrap();
                    let d = combat.data_of(&a);
                    if !crate::enemy::script::play_event(d, &mut a, ev) {
                        continue;
                    }
                    let len = d.length(&a.anim).max(0.1);
                    e.cur_ez = None;
                    e.mode = crate::enemy::Mode::Rising(len);
                }
                let (mut game, mut sim): (Vec<String>, Vec<String>) = (Vec::new(), Vec::new());
                for _ in 0..600 {
                    app.update();
                    let clip = chr.rt.main_clip().map(|c| (c.animation.clone(), c.time));
                    answers(&mut app, &mut chr.vm.host.borrow_mut(), clip, &anims, None, None, false);
                    chr.tick(1.0 / 60.0, &mut dur).unwrap();
                    let g = chr.rt.main_state_name().to_string();
                    if game.last() != Some(&g) {
                        game.push(g);
                    }
                    let s = enemy_clip(&mut app).0;
                    if sim.last() != Some(&s) {
                        sim.push(s);
                    }
                }
                let next = |v: &[String]| v.iter().skip_while(|s| **s != format!("Event{ev}")).nth(1).cloned().unwrap_or_else(|| "(stays)".into());
                let (g, s) = (next(&game), next(&sim));
                // Back to the AI: the game's IdleDefault, enemy.rs's idle or the AI's move from it
                // the same tick. Down: the game's DeathIdle*, enemy.rs holding the clip's last pose.
                let ai = |x: &str| ["Idle", "StandIdle", "Approach", "Walk", "Run", "Turn"].iter().any(|p| x.starts_with(p));
                let down = g.starts_with("DeathIdle") && s == "(stays)";
                if g != s && !(g.starts_with("Idle") && ai(&s)) && !down {
                    eprintln!("{id} Event{ev}: game {game:?} | enemy.rs {sim:?}");
                    off.push((id.to_string(), ev, g, s));
                }
            }
        }
        off
    }

    #[test]
    #[ignore]
    fn the_games_event_anim_ends_and_enemy_rs_agree() {
        let chrs = std::env::var("SHINOBI_LOCKSTEP_CHRS").unwrap_or_else(|_| "c1010 c1020 c1030 c1040 c1050 c1060 c1070 c1080 c1100 c1110 c1120 c1130 c1150 c1180 c1190 c1200 c1210 c1240 c1250 c1300 c1310 c1320 c1350 c1360 c1370 c1400 c1450 c1470 c1500".into());
        let chrs: Vec<&str> = chrs.split_whitespace().collect();
        let off = event_ends(&chrs);
        eprintln!("{} event ends differ", off.len());
        assert!(off.is_empty(), "{off:?}");
    }

    /// What one enemy comparison found.
    struct EnemyRun {
        same: usize,
        early: usize,
        /// The game's DeathIdle* (a one-frame clip under ragdoll: TAE 228 RagdollReviveTime)
        /// while enemy.rs holds its death clip's last frame.
        ragdoll: usize,
        /// The other DeathDefault variation (GetRandomIndex 0-1, IndexDeathVariation: the script's
        /// math.random and enemy.rs's rand pick apart).
        random: usize,
        off: Vec<String>,
        check_off: Vec<String>,
        path: Vec<String>,
    }

    /// The game's c9997 + c1020 scripts on its graph beside enemy.rs for `frames`, after `setup`,
    /// with Wolf pressing `input(frame)`. None without the extracted data.
    fn run(setup: impl FnOnce(&mut App), input: impl Fn(u32) -> (Vec<Action>, bool), frames: u32) -> Option<EnemyRun> {
        run_chr("c1020", setup, input, frames)
    }

    /// `run` for another enemy: its c9997.hkx, clip lengths and c<chr>.hks (the decompiled check
    /// only where hks_src has the chr's Lua).
    fn run_chr(id: &str, setup: impl FnOnce(&mut App), input: impl Fn(u32) -> (Vec<Action>, bool), frames: u32) -> Option<EnemyRun> {
        let x = crate::paths::extracted();
        let Ok(data) = std::fs::read(x.join(format!("chr/{id}.behbnd.d/c9997.hkx"))) else {
            eprintln!("no extracted {id} behbnd: skipped");
            return None;
        };
        let Ok(lengths) = std::fs::read_to_string(x.join(format!("clip_lengths/{id}.json"))) else {
            eprintln!("no extracted/clip_lengths/{id}.json: skipped");
            return None;
        };
        let lengths: HashMap<String, f32> = serde_json::from_str(&lengths).unwrap();
        let anims: Vec<i64> = lengths
            .keys()
            .filter_map(|k| {
                let (cat, num) = k.strip_prefix('a')?.split_once('_')?;
                Some(cat.parse::<i64>().ok()? * 1_000_000 + num.parse::<i64>().ok()?)
            })
            .collect();
        let graph = std::sync::Arc::new(sekiro_formats::hkb::parse(&data).expect("c9997.hkx parses"));
        let own = format!("{id}.hks");
        let vm = HksVm::load_chunks(&x.join("action/script"), &["c9997.hks", own.as_str()]).expect("the enemy scripts load");
        let own_lua = format!("{id}.lua");
        let decompiled = (x.join("hks_src").join(&own_lua).exists() && std::env::var("LOCKSTEP_NO_CHECK").is_err()).then(|| HksPlayer::load_files(&x.join("hks_src"), &["c9997.lua", own_lua.as_str()]).ok()).flatten();
        let rt = crate::hkb_runtime::BehaviorRuntime::new(graph, crate::hkb_runtime::AnimOffsets::default());
        let mut chr = ScriptChr::with_check(vm, rt, decompiled).expect("the start hooks run");
        let mut app = crate::sim_tests::app_with(id);
        // This run is the script's side itself (the live one would take its hits).
        app.world_mut().remove_non_send::<crate::hks_enemy::EnemyScripts>();
        setup(&mut app);
        // IsExistAnime is the TAE's list (its aliases too, which the clip files lack: c1060's
        // DamageAerialFront a000_008140): the sim's anims as well.
        let anims: Vec<i64> = {
            let mut q = app.world_mut().query_filtered::<&Actor, (With<Enemy>, Without<Player>)>();
            let world = app.world();
            let d = q.iter(world).next().map(|a| world.resource::<Combat>().data_of(a));
            let mut all = anims;
            all.extend(d.into_iter().flat_map(|d| d.anims.keys()).filter_map(|k| {
                let (cat, num) = k.strip_prefix('a')?.split_once('_')?;
                Some(cat.parse::<i64>().ok()? * 1_000_000 + num.parse::<i64>().ok()?)
            }));
            all.sort_unstable();
            all.dedup();
            all
        };
        let mut shown: Vec<(String, String, String)> = Vec::new();
        let mut request: Option<i64> = None;
        let mut prev_clip: Option<(String, f32)> = None;
        {
            // The script's own start (InitState -> W_IdleBattle) a frame before the sim's first.
            let clip = chr.rt.main_clip().map(|c| (c.animation.clone(), c.time));
            answers(&mut app, &mut chr.vm.host.borrow_mut(), clip, &anims, None, None, false);
            let mut q = app.world_mut().query_filtered::<(&Actor, &Enemy), Without<Player>>();
            let world = app.world();
            let (a, _) = q.iter(world).find(|(_, e)| !e.disabled).expect("an enemy");
            let d = world.resource::<Combat>().data_of(a);
            chr.tick(1.0 / 60.0, &mut |anim: &str| lengths.get(anim).copied().or_else(|| d.anim(anim).map(|_| d.length(anim)))).expect("the script starts");
        }
        for f in 0..frames {
            let before = enemy_clip(&mut app).0;
            {
                let mut p = app.world_mut().resource_mut::<PadInput>();
                let (press, guard) = input(f);
                for a in press {
                    p.press(a);
                }
                p.guard_held = guard;
            }
            // The AI thinks before the script runs (the script reads this frame's request), so
            // the sim steps first and the script answers from it.
            app.update();
            {
                // offsetType 15: its anim group (Actor::grouped).
                let mut q = app.world_mut().query_filtered::<(&Actor, &Enemy), Without<Player>>();
                let world = app.world();
                if let Some((a, _)) = q.iter(world).find(|(_, e)| !e.disabled) {
                    chr.rt.offsets.anim = a.anim_group as i64 * 100;
                }
            }
            let clip = chr.rt.main_clip().map(|c| (c.animation.clone(), c.time));
            // The AI's attack request stays posted until the script plays it (enemy.rs starts
            // it the frame it asks, so it shows once).
            if let Some(n) = enemy_request(&mut app) {
                request = Some(n);
            }
            // Played: the script (re)started that clip since the last frame (a request for the clip
            // already playing waits for its restart).
            let started = clip.as_ref().is_some_and(|(c, t)| prev_clip.as_ref().is_none_or(|(pc, pt)| pc != c || t < pt));
            if started && request.is_some_and(|n| clip.as_ref().is_some_and(|(c, _)| c.ends_with(&format!("_{:06}", n % 1_000_000)))) {
                request = None;
            }
            prev_clip = clip.clone();
            let damage = {
                let world = app.world_mut();
                let mut q = world.query_filtered::<(&mut Actor, &Enemy), Without<Player>>();
                q.iter_mut(world).find(|(_, e)| !e.disabled).and_then(|(mut a, _)| a.damage_env.take())
            };
            // The engine starts a throw on the graph itself (W_ThrowAtk<id> / W_ThrowDef<id>).
            let state = enemy_clip(&mut app).0;
            let throw_start = state != before && (state.starts_with("ThrowAtk") || state.starts_with("ThrowDef"));
            if throw_start {
                chr.rt.fire_event(&format!("W_{state}"));
            }
            answers(&mut app, &mut chr.vm.host.borrow_mut(), clip, &anims, request, damage, throw_start);
            let events = {
                let mut q = app.world_mut().query_filtered::<(&Actor, &Enemy), Without<Player>>();
                let world = app.world();
                let (a, _) = q.iter(world).find(|(_, e)| !e.disabled).expect("an enemy");
                let d = world.resource::<Combat>().data_of(a);
                chr.tick(1.0 / 60.0, &mut |anim: &str| lengths.get(anim).copied().or_else(|| d.anim(anim).map(|_| d.length(anim)))).expect("the script runs")
            };
            let (after, ours) = enemy_clip(&mut app);
            if std::env::var("LOCKSTEP_DEBUG").is_ok_and(|v| v.split(',').any(|x| x.parse() == Ok(f))) {
                let world = app.world_mut();
                let mut q = world.query_filtered::<(&Actor, &Enemy), Without<Player>>();
                if let Some((a, e)) = q.iter(world).find(|(_, e)| !e.disabled) {
                    let d = world.resource::<Combat>().data_of(a);
                    let fl: Vec<i64> = [23, 86, 79, 78].into_iter().filter(|&x| d.flag(&a.anim, a.t, x)).collect();
                    let sc = chr.rt.main_clip().map(|c| (c.animation.clone(), c.time));
                    eprintln!("DBG f{f} {before}->{after} {} t {:.3} flags {fl:?} script {sc:?} desc {} cmd {:?} turn {:?} refs {:?} ai_sp {} worn {:?}", a.anim, a.t, e.ai_desc, e.ai_cmd, chr.vm.host.borrow().vars.get("TurnAngle"), chr.vm.host.borrow().env.keys().filter(|k| k.0 == 3036).filter_map(|k| k.1).collect::<Vec<_>>(), e.ai_state_sp(), crate::combat::worn_sp(a, &e.clash).iter().map(|id| (*id, d.sp_effects.get(&id.to_string()).map(|s| s.behavior_ref_id))).collect::<Vec<_>>());
                }
            }
            // enemy.rs's sandbox brings a dead enemy back (Approach): the game's would stay dead.
            if after == "Approach" && before.contains("Death") {
                break;
            }
            let theirs = chr.rt.main_clip().map(|c| c.animation.clone()).unwrap_or_default();
            let hit = damage.map_or(String::new(), |d| format!(" hit {d:?}"));
            shown.push((ours, theirs, format!("f{f} {before} -> {after}{hit}, script {events:?} ({})", chr.rt.main_state_name())));
        }
        let (mut same, mut early, mut ragdoll, mut random, mut off) = (0, 0, 0, 0, Vec::new());
        let death_variation = |c: &str| c.ends_with("_010000") || c.ends_with("_010010");
        for (i, (ours, theirs, at)) in shown.iter().enumerate() {
            if ours == theirs {
                same += 1;
            } else if at.ends_with("(DeathStartDefault)") && death_variation(ours) && death_variation(theirs) {
                random += 1;
            } else if at.ends_with("(DeathIdleDefault)") && shown[..i].iter().rev().find(|s| s.1 != *theirs).is_some_and(|s| s.0 == *ours) {
                ragdoll += 1;
            } else if shown.get(i + 1).is_some_and(|n| &n.0 == theirs) {
                early += 1;
            } else {
                off.push(format!("{at}: enemy.rs {ours}, game {theirs}"));
            }
        }
        let mut path: Vec<String> = shown.iter().map(|s| s.0.clone()).collect();
        path.dedup();
        eprintln!("{id}: the game's script + graph: {:.1} us a tick ({:.1} in the VM) over {}", chr.busy.as_secs_f64() * 1e6 / chr.ticks.max(1) as f64, chr.vm_busy.as_secs_f64() * 1e6 / chr.ticks.max(1) as f64, chr.ticks);
        Some(EnemyRun { same, early, ragdoll, random, off, check_off: chr.check_off.clone(), path })
    }

    fn report(name: &str, r: &EnemyRun) {
        eprintln!("{name}: enemy.rs clips {:?}", r.path);
        for o in r.off.iter().take(30) {
            eprintln!("  off: {o}");
        }
        eprintln!("{name}: the game's clip vs enemy.rs's: {} frames the same, {} a frame early, {} ragdoll, {} random, {} differ; decompiled check: {:?}", r.same, r.early, r.ragdoll, r.random, r.off.len(), r.check_off.iter().take(5).collect::<Vec<_>>());
    }

    /// The enemy fights on its own (Wolf stands): its battle goal's moves and attacks.
    #[test]
    fn the_games_enemy_script_and_enemy_rs_agree_frame_by_frame() {
        let Some(r) = run(
            |app| {
                // It fights: its battle goal runs (the sim's default General only stands guard).
                let world = app.world_mut();
                world.query_filtered::<&mut Enemy, Without<crate::enemy::Cast>>().single_mut(world).unwrap().aggressive = true;
            },
            |_| (Vec::new(), false),
            900,
        ) else {
            return;
        };
        report("enemy", &r);
        // Frame 0 is the script's own start (InitState -> W_IdleBattle before it reads a request).
        let off: Vec<&String> = r.off.iter().filter(|o| !o.starts_with("f0 ")).collect();
        assert!(off.is_empty(), "enemy.rs leaves the game's script: {off:#?}");
        assert!(r.check_off.is_empty(), "the compiled and decompiled scripts differ: {:?}", r.check_off);
        assert!(r.same > 850, "{} frames the same", r.same);
    }

    /// Wolf attacks the enemy (its default guarding goal): the game's ExecDamage / ExecGuardBlock
    /// against combat.rs's picks.
    #[test]
    fn the_games_enemy_script_and_enemy_rs_agree_on_hits() {
        let aggressive = |app: &mut App| {
            let world = app.world_mut();
            world.query_filtered::<&mut Enemy, Without<crate::enemy::Cast>>().single_mut(world).unwrap().aggressive = true;
        };
        let Some(r) = run(aggressive, |f| (if f % 45 == 10 { vec![Action::Attack] } else { Vec::new() }, false), 600) else {
            return;
        };
        report("enemy hits", &r);
        let off: Vec<&String> = r.off.iter().filter(|o| !o.starts_with("f0 ")).collect();
        assert!(off.is_empty(), "enemy.rs leaves the game's script: {off:#?}");
        assert!(r.check_off.is_empty(), "the compiled and decompiled scripts differ: {:?}", r.check_off);
    }

    /// Wolf guards the enemy's attacks: held (blocks, ExecAttackBound) and tapped every 10
    /// frames (deflects, ExecAttackJustBound).
    #[test]
    fn the_games_enemy_script_and_enemy_rs_agree_on_blocks_and_deflects() {
        let aggressive = |app: &mut App| {
            let world = app.world_mut();
            world.query_filtered::<&mut Enemy, Without<crate::enemy::Cast>>().single_mut(world).unwrap().aggressive = true;
        };
        for (name, input) in [
            ("enemy blocked", Box::new(|_| (Vec::new(), true)) as Box<dyn Fn(u32) -> (Vec<Action>, bool)>),
            ("enemy deflected", Box::new(|f| (if f % 10 == 0 { vec![Action::Guard] } else { Vec::new() }, f % 10 < 3))),
        ] {
            let Some(r) = run(aggressive, input, 900) else {
                return;
            };
            report(name, &r);
            let off: Vec<&String> = r.off.iter().filter(|o| !o.starts_with("f0 ")).collect();
            assert!(off.is_empty(), "{name}: enemy.rs leaves the game's script: {off:#?}");
            assert!(r.check_off.is_empty(), "{name}: the compiled and decompiled scripts differ: {:?}", r.check_off);
        }
    }

    /// A longer mixed fight: Wolf attacks every 25 frames and taps guard every 13, so hits,
    /// guards, deflects both ways and a posture break meet in one run.
    #[test]
    fn the_games_enemy_script_and_enemy_rs_agree_in_a_mixed_fight() {
        let aggressive = |app: &mut App| {
            let world = app.world_mut();
            world.query_filtered::<&mut Enemy, Without<crate::enemy::Cast>>().single_mut(world).unwrap().aggressive = true;
        };
        let input = |f: u32| {
            let mut press = Vec::new();
            if f % 25 == 0 {
                press.push(Action::Attack);
            }
            if f % 13 == 5 {
                press.push(Action::Guard);
            }
            (press, f % 13 >= 5 && f % 13 < 8)
        };
        let Some(r) = run(aggressive, input, 2400) else {
            return;
        };
        report("mixed fight", &r);
        let off: Vec<&String> = r.off.iter().filter(|o| !o.starts_with("f0 ")).collect();
        assert!(off.is_empty(), "enemy.rs leaves the game's script: {off:#?}");
        assert!(r.check_off.is_empty(), "the compiled and decompiled scripts differ: {:?}", r.check_off);
    }

    /// Its posture near full: Wolf's hits break it (ExecDamageBreak), then his deathblow (the
    /// engine's W_ThrowDef<id>).
    #[test]
    fn the_games_enemy_script_and_enemy_rs_agree_on_a_posture_break_and_deathblow() {
        let near_break = |app: &mut App| {
            let world = app.world_mut();
            world.query_filtered::<&mut Enemy, Without<crate::enemy::Cast>>().single_mut(world).unwrap().aggressive = true;
            let mut q = world.query_filtered::<&mut Actor, (With<Enemy>, Without<crate::enemy::Cast>)>();
            let mut a = q.single_mut(world).unwrap();
            a.posture = a.posture_max - 1.0;
        };
        // (enemy.rs's sandbox brings it back 2 s after the death clip: the run ends before.)
        let Some(r) = run(near_break, |f| (if f % 20 == 10 { vec![Action::Attack] } else { Vec::new() }, false), 480) else {
            return;
        };
        report("posture break", &r);
        let off: Vec<&String> = r.off.iter().filter(|o| !o.starts_with("f0 ")).collect();
        assert!(off.is_empty(), "enemy.rs leaves the game's script: {off:#?}");
        assert!(r.check_off.is_empty(), "the compiled and decompiled scripts differ: {:?}", r.check_off);
        assert!(r.ragdoll > 0, "the run reaches the death");
    }

    /// The other regular enemies in a mixed fight (report only: the survey of enemy.rs against
    /// each one's own script).
    #[test]
    #[ignore]
    fn the_games_enemy_scripts_beside_enemy_rs_for_every_regular_enemy() {
        let chrs = std::env::var("SHINOBI_LOCKSTEP_CHRS").unwrap_or_else(|_| "c1000 c1010 c1030 c1040 c1050 c1060 c1070 c1080 c1100 c1110 c1120 c1130 c1150 c1180 c1190 c1200 c1210 c1240 c1250 c1300 c1310 c1320 c1350 c1360 c1370 c1400 c1450 c1460 c1470 c1500".into());
        let input = |f: u32| {
            let mut press = Vec::new();
            if f % 25 == 0 {
                press.push(Action::Attack);
            }
            if f % 13 == 5 {
                press.push(Action::Guard);
            }
            (press, f % 13 >= 5 && f % 13 < 8)
        };
        let mut lines = Vec::new();
        for chr in chrs.split_whitespace() {
            let aggressive = |app: &mut App| {
                let world = app.world_mut();
                if let Ok(mut e) = world.query_filtered::<&mut Enemy, Without<crate::enemy::Cast>>().single_mut(world) {
                    e.aggressive = true;
                }
            };
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run_chr(chr, aggressive, input, 1200)));
            match r {
                Ok(Some(r)) => {
                    report(chr, &r);
                    lines.push(format!("{chr}: {} same, {} early, {} ragdoll, {} random, {} differ, first {:?}", r.same, r.early, r.ragdoll, r.random, r.off.len(), r.off.iter().find(|o| !o.starts_with("f0 "))));
                }
                Ok(None) => lines.push(format!("{chr}: no data")),
                Err(_) => lines.push(format!("{chr}: panicked")),
            }
        }
        for l in lines {
            eprintln!("SURVEY {l}");
        }
    }
}

