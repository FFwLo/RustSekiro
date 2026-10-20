//! Wolf's engine side for the game's player script (hks.rs `ScriptChr` on c0000.hkx): what the
//! exe answers to `env(id[, arg])` and the move variables, taken from his Actor / Player as
//! player.rs and combat.rs have him this frame. The lockstep tests (hks_lua.rs) and the live
//! script (SHINOBI_HKS) both use it.

use std::collections::HashMap;

use bevy::prelude::*;

use crate::actor::Actor;
use crate::data::Combat;
use crate::enemy::DamageEnv;
use crate::hks::Host;
use crate::player::{Action, Player};

pub const ARM_ATTACK: i64 = 0;
pub const ARM_SUB_ATTACK: i64 = 1;
pub const ARM_GUARD: i64 = 2;
pub const ARM_JUMP: i64 = 4;
pub const ARM_SP_MOVE: i64 = 5;
pub const ARM_USE_ITEM: i64 = 7;
pub const ARM_CROUCH: i64 = 15;
pub const ARM_SHINOBI_WEP_ACTION: i64 = 18;
pub const ARM_SPECIAL_ATTACK: i64 = 32;
/// SP_EF_REF_IN_NON_COMBAT_AREA (c0000_define 748).
const SP_REF_IN_NON_COMBAT_AREA: i64 = 600;
pub const LEFT: i64 = 0;
pub const RIGHT: i64 = 1;
pub const KUSABIMARU: i64 = 5000;
pub const ACTIONS: [Action; 8] = [Action::Attack, Action::Guard, Action::Jump, Action::Step, Action::UseItem, Action::Prosthetic, Action::Crouch, Action::CombatArt];

/// The ACTION_ARM_* bits a press requests (a combat art: the attack with the guard held).
pub fn arm(a: Action) -> i64 {
    match a {
        Action::Attack | Action::CombatArt => ARM_ATTACK,
        Action::Guard => ARM_GUARD,
        Action::Jump => ARM_JUMP,
        Action::Step => ARM_SP_MOVE,
        Action::UseItem => ARM_USE_ITEM,
        Action::Prosthetic => ARM_SUB_ATTACK,
        Action::Crouch => ARM_CROUCH,
        _ => -1,
    }
}

/// One frame's buttons.
#[derive(Default, Clone)]
pub struct Pad {
    pub press: Vec<Action>,
    /// The other buttons held down (env 1108 while held: the hold arts, the charged tools).
    pub held: Vec<Action>,
    pub guard_held: bool,
    pub stick: Vec2,
}

/// env 237: the exe's guard reaction size by [guardLevel][damage level] (0x143b01bc0, read by
/// FUN_140843820); 0 = no guard reaction for that level.
const GUARD_REACTION: [[i64; 12]; 5] = [
    [0; 12],
    [0, 1, 3, 3, 4, 1, 3, 3, 0, 3, 4, 3],
    [0, 1, 1, 3, 3, 1, 3, 3, 0, 3, 4, 3],
    [0, 1, 1, 3, 3, 1, 3, 3, 0, 3, 4, 3],
    [0, 1, 1, 1, 3, 1, 3, 3, 0, 3, 4, 3],
];

/// The engine's answers for Wolf as player.rs has him at the start of this frame, with this
/// frame's hit (combat.rs `Actor::damage_env`; exe FUN_140b6bd40 / FUN_140b698d0, docs/kb/hks-graph.md).
#[allow(clippy::too_many_arguments)]
pub fn answers(h: &mut Host, a: &Actor, p: &Player, combat: &Combat, config: &crate::config::GameConfig, locked: bool, pad: &Pad, damage: Option<DamageEnv>) {
    let d = &combat.player;
    let mut env: HashMap<(i64, Option<i64>), f64> = HashMap::new();
    let mut set = |id: i64, arg: Option<i64>, v: bool| {
        env.insert((id, arg), v as i64 as f64);
    };
    // The state Wolf is in; the TAE answers are the clip on screen's (the walk's too).
    let (shown, t) = a.shown_clip(d);
    let anim = !a.anim.is_empty();
    set(201, None, !a.airborne);
    set(200, None, a.airborne && a.vel_y < 0.0);
    set(207, None, !a.sheathed);
    set(1105, None, crate::player::is_free(&a.state));
    set(2000, None, d.flag(&shown, t, crate::player::FLAG_ACCEPT_MOVE));
    let ended = anim && a.t >= d.length(&a.anim);
    set(339, None, ended);
    set(339, Some(0), ended);
    set(1118, None, locked);
    // 206: in a throw (grabbed, thrown, a deathblow pair): no BEH_R_DEATH, the throw runs on;
    // 276: killed in the grab at its ChrActionFlag 69 (throw state 6): W_ThrowDefDeathStart.
    let throw = a.state == "Grabbed" || a.state.starts_with("ThrowDef") || a.state.starts_with("ThrowEscape") || a.state.starts_with("Deathblow");
    set(206, None, throw);
    set(276, None, a.hp <= 0.0 && a.state == "Grabbed" && anim && d.flag(&a.anim, a.t, crate::player::FLAG_THROW_TYPE5));
    // The SpEffects on Wolf (the anim's TAE ones and the timed ones) and their behaviour refs.
    let mut ids: Vec<i64> = d.sp_effects_at(&shown, t).into_iter().map(|(id, _)| id).collect();
    ids.extend(p.timed.iter().map(|t| t.0));
    // The equipped art's resident SpEffects (the 101 art's back-step unlock 280 and the like).
    let resident: Vec<i64> = if config.player.combat_art > 0 {
        let row = combat.param("EquipParamWeapon", config.player.combat_art);
        ["residentSpEffectId", "residentSpEffectId1", "residentSpEffectId2"].iter().filter_map(|f| row[*f].as_i64().filter(|&id| id > 0)).collect()
    } else {
        Vec::new()
    };
    ids.extend(&resident);
    for id in &ids {
        set(1116, Some(*id), true);
        let r = d.sp_effects.get(&id.to_string()).map_or_else(|| crate::data::sp_effect_behavior_ref(*id), |s| s.behavior_ref_id);
        set(3036, Some(r), resident.contains(id) || crate::player::wolf_ref(d, a, p, r));
    }
    // The X sheathe (ours: the game sheathes only in a non-combat area) is a non-combat area
    // to the script till a draw starts.
    if (a.sheathed && a.state != "GroundNonCombatAreaLeave") || a.state == "GroundNonCombatAreaEnter" {
        set(3036, Some(SP_REF_IN_NON_COMBAT_AREA), true);
    }
    set(1116, None, false);
    set(3036, None, false);
    // The presses: requested (env 1106) while buffered and inside its window, held (1108).
    for act in ACTIONS {
        let pending = p.requests.contains_key(&act) || (pad.press.contains(&act) && crate::player::buffers(d, a, act));
        // In the air the execute window is the clip's accept flag now (player.rs air_ok);
        // the jump's fall loops take every press.
        let air = a.state == "FreeFall" || a.state.ends_with("GroundJumpFall") || (anim && crate::player::accept_flag(act).iter().any(|&f| d.flag(&a.anim, a.t, f)));
        // The prosthetic button is both ACTION_ARM_SUB_ATTACK and ACTION_ARM_SHINOBI_WEP_ACTION
        // (the umbrella's release wants both held).
        let arms: &[i64] = if act == Action::Prosthetic { &[ARM_SUB_ATTACK, ARM_SHINOBI_WEP_ACTION] } else { &[arm(act)] };
        for &arm in arms {
            if pending && if a.airborne { air } else { crate::player::accepts(d, a, act) } {
                set(1106, Some(arm), true);
            }
            if pad.press.contains(&act) || pad.held.contains(&act) {
                set(1108, Some(arm), true);
            }
        }
    }
    if pad.guard_held || pad.press.contains(&Action::CombatArt) {
        set(1108, Some(ARM_GUARD), true);
    }
    set(1106, None, false);
    set(1108, None, false);
    // What Wolf carries.
    let tool = crate::player::equipped_tool(combat, config, p.tool_slot);
    let emblems = |cost: u32| config.player.infinite_emblems || p.emblems >= cost;
    set(3035, Some(ARM_SHINOBI_WEP_ACTION), tool.is_some_and(|t| emblems(t.emblems)));
    set(3035, Some(ARM_SPECIAL_ATTACK), emblems(crate::player::art_cost(combat, config)));
    // A next-item press player.rs takes on its next step is the exe's switch at once.
    let slot = if p.requests.contains_key(&Action::NextItem) || pad.press.contains(&Action::NextItem) { crate::items::next_slot(config, p.quick_slot) } else { p.quick_slot };
    let mut goods = crate::items::in_slot(config, slot);
    // A refilled goods run out is its empty twin (Five-color Rice 3580 -> 3581, the empty shake).
    if goods != 3000 && crate::items::count(config, goods) == 0 && crate::items::empty_state(goods).is_some() {
        goods += 1;
    }
    // itemUseEnable: one left, and no worn SpEffect whose stateInfo is one of the goods'
    // useLimitSpEffectType (the Owl's recovery seal on the gourd).
    let limits = crate::data::goods_use_limits(goods);
    let limited = crate::combat::worn_sp(a, &p.timed).iter().any(|id| crate::data::sp_cycle(*id).is_some_and(|c| limits.contains(&c.state_info)));
    let left = goods > 0 && (crate::items::count(config, goods) > 0 || crate::items::count(config, goods - 1) == 0 && crate::items::empty_state(goods - 1).is_some());
    set(233, None, !limited && if goods == 3000 { p.gourd > 0 } else { left });
    // The moves the learned skills unlock (ACTION_UNLOCK_TYPE_*, combat.rs action_unlocked).
    for ty in 0..32 {
        set(3033, Some(ty), crate::combat::action_unlocked(combat, config, ty as u32));
    }
    // gap: free to leave the crouch collision, not the debug dash.
    set(3033, None, true);
    set(3037, None, true);
    set(3008, None, false);
    // IsSwimContact: in the water (swim.rs).
    set(3025, None, p.swim.is_some());
    let weapon = |row: i64, field: &str| combat.param("EquipParamWeapon", row)[field].as_i64().unwrap_or(-1);
    env.insert((1000, None), a.hp as f64);
    env.insert((333, None), 1000.0 / 60.0);
    env.insert((225, Some(RIGHT)), weapon(KUSABIMARU, "wepmotionCategory") as f64);
    env.insert((225, Some(LEFT)), tool.map_or(-1.0, |t| t.group as f64));
    // No art equipped: SP_ATK_TYPE_NONE.
    env.insert((345, Some(RIGHT)), if config.player.combat_art > 0 { weapon(config.player.combat_art, "spAtkcategory") } else { 0 } as f64);
    env.insert((231, None), crate::items::param("EquipParamGoods", goods)["goodsUseAnim"].as_i64().unwrap_or(-1) as f64);
    // This frame's hit (the exe resets these each frame: no hit is type 99999, sides -1).
    let dm = damage.unwrap_or(DamageEnv::NONE);
    env.insert((202, None), dm.kind as f64);
    env.insert((236, None), dm.level as f64);
    env.insert((256, None), damage.is_some() as i64 as f64);
    env.insert((222, None), dm.dir as f64);
    // gap: 3032 is the exe's 8-way sector 2-5 (the back half); here the back quarter only.
    env.insert((3032, None), (dm.dir == 3) as i64 as f64);
    env.insert((3056, None), dm.guard_dir as f64);
    env.insert((3057, None), dm.just_dir as f64);
    env.insert((3018, None), dm.bound as f64);
    env.insert((3019, None), dm.just_bound as f64);
    // 258 his guardLevel (the katana's), 237 the guard reaction size for this hit.
    let guard_level = weapon(KUSABIMARU, "guardLevel").clamp(0, 4);
    env.insert((258, None), guard_level as f64);
    let guarded = matches!(dm.kind, 3 | 1028);
    let size = if guarded { GUARD_REACTION[guard_level as usize].get(dm.level as usize).copied().unwrap_or(0) } else { 0 };
    env.insert((237, None), size as f64);
    h.env = env;
}

/// The clip offsets the exe reads for Wolf with this combat art and tool row equipped: the right
/// hand the katana's wepmotionCategory with the art row's spAtkcategory, the left hand the tool's
/// (hkb_runtime::AnimOffsets; docs/kb/hks-graph.md, How a CMSG picks its clip).
pub fn offsets(combat: &Combat, art: i64, tool: i64) -> crate::hkb_runtime::AnimOffsets {
    let field = |row: i64, f: &str| combat.param("EquipParamWeapon", row)[f].as_i64().unwrap_or(-1);
    crate::hkb_runtime::AnimOffsets { hands: [(field(tool, "wepmotionCategory"), field(tool, "spAtkcategory").max(0)), (field(KUSABIMARU, "wepmotionCategory"), field(art, "spAtkcategory").max(0))], ..Default::default() }
}

/// The stick's move variables (exe FUN_140b4afd0: MoveSpeedLevel the stick's length, MoveAngle /
/// TurnAngle against the facing, right positive). MoveSpeedIndex is the script's own
/// (_SpeedUpdate). gap: the sprint hold.
pub fn stick_vars(h: &mut Host, stick: Vec2) {
    let angle = if stick == Vec2::ZERO { 0.0 } else { stick.x.atan2(-stick.y).to_degrees() as f64 };
    h.vars.insert("MoveSpeedLevel".into(), stick.length() as f64);
    h.vars.insert("MoveAngle".into(), angle);
    h.vars.insert("TurnAngle".into(), angle);
}

/// The move variables live, against Wolf's facing (`stick` in the world). Free (not locked on), a
/// move from a standstill turns the body to the stick at once and the turn becomes the lower-body
/// twist (sekiro-rs character.rs free_move_start_turn): TurnAngle reads 0 and TwistLowerRootAngle
/// the turn, positive to the left, so past 135 deg the script picks its turn-and-go start. Locked
/// on (`target`: the way to the lock target), TurnAngle is the turn to the target.
pub fn move_vars(h: &mut Host, a: &Actor, stick: Vec3, level: f32, target: Option<Vec3>) {
    let fwd = a.forward().with_y(0.0).normalize_or_zero();
    let off = |v: Vec3| if v == Vec3::ZERO { 0.0 } else { v.dot(fwd.cross(Vec3::Y)).atan2(v.dot(fwd)).to_degrees() as f64 };
    let rel = off(stick);
    let standing = target.is_none() && matches!(a.state.as_str(), "StandIdle" | "CrouchIdle") && level > 0.0;
    let (turn, twist) = if standing { (0.0, -rel) } else { (rel, 0.0) };
    h.vars.insert("MoveSpeedLevel".into(), level as f64);
    h.vars.insert("MoveAngle".into(), turn);
    h.vars.insert("TurnAngle".into(), target.map_or(turn, off));
    h.vars.insert("TwistLowerRootAngle".into(), twist);
    // The stick against the facing as the exe hands it to a step (act 2019), a jump, an attack and
    // a tool.
    h.vars.insert("MoveSpeedLevelReal".into(), level as f64);
    for (angle, lvl) in [("KickAngle", "KickStickLevel"), ("JumpAngle", "JumpStickLevel"), ("AttackAngle", "AttackStickLevel"), ("SubAttackAngle", "SubAttackStickLevel")] {
        h.vars.insert(angle.into(), rel);
        h.vars.insert(lvl.into(), level as f64);
    }
}

/// Wolf as the game runs him (SHINOBI_HKS=0: off, player.rs alone). The game's compiled script on his behaviour graph
/// ticks each frame before player.rs decides (on the answers above), and where its graph plays
/// another clip than player.rs picked, the graph's state and clip win. player.rs keeps movement,
/// physics, items and hits, and its locomotion while both are just standing or moving.
/// SHINOBI_HKS_TRACE=1 logs the engine events and overrides, =2 every frame's two clips, =3 the
/// presses and the script's events.
pub struct HksWolfPlugin;

impl Plugin for HksWolfPlugin {
    fn build(&self, app: &mut App) {
        if std::env::var("SHINOBI_HKS").is_ok_and(|v| v == "0") {
            return;
        }
        let Some(script) = WolfScript::load() else {
            warn!("SHINOBI_HKS: no extracted c0000.hkx / action/script / clip_lengths: player.rs alone");
            return;
        };
        app.insert_non_send(script)
            .add_systems(FixedUpdate, (tick_script.before(crate::actor::ActorSet::Decide), apply_script.after(crate::actor::ActorSet::Decide).before(crate::actor::ActorSet::Advance)));
    }
}

/// Wolf's `ScriptChr` (not Send: the VM's host is shared by Rc).
pub struct WolfScript {
    chr: crate::hks::ScriptChr,
    lengths: HashMap<String, f32>,
    /// player.rs's picks the graph overrode.
    pub overrides: u32,
    /// The map entry played out (below).
    started: bool,
    /// Wolf's state and anim when the script ticked.
    state_before: String,
    anim_before: String,
    /// Wolf's state after the last sync: a different one at the next tick was set outside
    /// player.rs's decide (combat.rs's hits and grabs, a map event).
    state_after: String,
    anim_after: String,
    /// player.rs is in a state the graph cannot follow (no event for it): no overrides until the
    /// two show the same clip again.
    stood_aside: bool,
    /// The ThrowId of the grab Wolf is in.
    throw_id: Option<i64>,
    /// The height fallen so far (env 224, GetFallHeight: from the flight's highest point).
    fall: f32,
    /// Wolf was in the air when the script last ticked.
    air: bool,
    trace: u8,
}

impl WolfScript {
    fn load() -> Option<Self> {
        let x = crate::paths::extracted();
        let graph = sekiro_formats::hkb::parse(&std::fs::read(x.join("chr/c0000.behbnd.d/c0000.hkx")).ok()?).ok()?;
        let lengths: HashMap<String, f32> = serde_json::from_str(&std::fs::read_to_string(x.join("clip_lengths/c0000.json")).ok()?).ok()?;
        let vm = crate::hks::HksVm::load(&x.join("action/script")).ok()?;
        let rt = crate::hkb_runtime::BehaviorRuntime::new(std::sync::Arc::new(graph), crate::hkb_runtime::AnimOffsets::default());
        let chr = crate::hks::ScriptChr::new(vm, rt).ok()?;
        let trace = std::env::var("SHINOBI_HKS_TRACE").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
        Some(Self { chr, lengths, overrides: 0, started: false, state_before: String::new(), anim_before: String::new(), state_after: String::new(), anim_after: String::new(), stood_aside: false, throw_id: None, fall: 0.0, air: false, trace })
    }

    /// A state the engine started, fired on the graph as the exe does: a throw pair (W_ThrowAtk /
    /// W_ThrowDef<anim>), a throw kill, a map event (W_Event<anim>); with `own` also W_<state>
    /// (the script fired nothing this frame). Stands aside when the graph has none of them.
    /// The graph's state as player.rs names it: its main clip's CMSG where player.rs has it (the
    /// extractor's table), else the graph state.
    fn graph_state(&self, d: &crate::data::CharData) -> String {
        let cmsg = self.chr.rt.main_clip().and_then(|c| self.chr.rt.clip_cmsg(c)).filter(|c| d.anim_key(c).is_some());
        cmsg.unwrap_or_else(|| self.chr.rt.main_state_name()).to_string()
    }

    /// player.rs went back to standing or moving where the graph still plays an action (a
    /// respawn, a cancel of its own): the graph goes back to idle with him.
    fn to_idle(&mut self, d: &crate::data::CharData, crouch: bool) {
        let lengths = &self.lengths;
        let mut dur = |a: &str| lengths.get(a).copied().or_else(|| d.anim(a).map(|_| d.length(a)));
        let idle = if crouch { "W_CrouchIdle" } else { "W_Idle" };
        let _ = self.chr.engine_event(idle, &mut dur);
        if self.trace > 0 {
            eprintln!("hks: {} -> {idle}", self.graph_state(d));
        }
    }

    /// The graph shows `state` (its main clip's CMSG or its state).
    fn shows(&self, state: &str) -> bool {
        self.chr.rt.main_clip().is_some_and(|c| self.chr.rt.clip_cmsg(c) == Some(state)) || self.chr.rt.main_state_name() == state
    }

    fn engine_sync(&mut self, d: &crate::data::CharData, state: &str, anim: &str, own: bool) -> Option<String> {
        let n = anim.rsplit('_').next().and_then(|n| n.parse::<u32>().ok());
        // A throw's clips are in its ThrowParam anim group (offsetType 17): the one player.rs
        // took (a211_500000: 211).
        let group = anim.strip_prefix('a').and_then(|a| a.split('_').next()).and_then(|g| g.parse::<i64>().ok());
        if let Some(g) = group.filter(|&g| g > 0) {
            self.chr.rt.offsets.throw = Some(g);
        }
        let mut names: Vec<String> = n.map(|n| ["ThrowAtk", "ThrowDef", "ThrowKill", "Event"].iter().map(|k| format!("W_{k}{n}")).collect()).unwrap_or_default();
        // The grab's own follow-ups carry its ThrowId (W_ThrowDefDeathStart600000).
        if let Some(id) = self.throw_id.filter(|_| state.starts_with("ThrowDef")) {
            names.push(format!("W_{state}{id}"));
        }
        if own {
            names.push(format!("W_{state}"));
        }
        let lengths = &self.lengths;
        let mut dur = |a: &str| lengths.get(a).copied().or_else(|| d.anim(a).map(|_| d.length(a)));
        let chr = &mut self.chr;
        let synced = names.into_iter().find(|e| {
            chr.engine_event(e, &mut dur).unwrap_or_else(|err| {
                warn!("SHINOBI_HKS: {e}: {err}");
                false
            })
        });
        if own || synced.is_some() {
            self.stood_aside = synced.is_none();
        }
        // A grab sets the graph's Selector_ThrowId as the exe does (the death and escape events
        // are named by it).
        if let Some(id) = synced.as_deref().and_then(|e| e.strip_prefix("W_ThrowDef")).and_then(|n| n.parse::<i64>().ok()) {
            self.throw_id = Some(id);
            self.chr.rt.set_variable("Selector_ThrowId", id as f32);
            self.chr.vm.host.borrow_mut().vars.insert("Selector_ThrowId".into(), id as f64);
        }
        if self.trace > 0 {
            eprintln!("hks: engine {state} {anim} ({})", synced.as_deref().unwrap_or("not in the graph"));
        }
        synced
    }
}

#[allow(clippy::too_many_arguments)]
fn tick_script(mut script: NonSendMut<WolfScript>, time: Res<Time>, combat: Res<Combat>, config: Res<crate::config::GameConfig>, lock: Res<crate::camera::LockOn>, pad: Res<crate::player::PadInput>, camera: Option<Single<&crate::camera::OrbitCamera>>, places: Query<&Transform>, mut wolf: Single<(Entity, &mut Actor, &Player)>) {
    let (me, a, p) = &mut *wolf;
    let target = lock.target.and_then(|t| Some((places.get(t).ok()?.translation - places.get(*me).ok()?.translation).with_y(0.0).normalize_or_zero()));
    let mut damage = a.damage_env.take();
    let held = [(pad.attack_held, Action::Attack), (pad.dodge_held, Action::Step), (pad.prosthetic_held, Action::Prosthetic)].into_iter().filter_map(|(h, a)| h.then_some(a)).collect();
    let input = Pad { press: pad.pressed().to_vec(), held, guard_held: pad.guard_held, stick: pad.stick };
    let s = &mut *script;
    let d = &combat.player;
    let tool = crate::player::equipped_tool(&combat, &config, p.tool_slot).map_or(-1, |t| t.id);
    let throw = s.chr.rt.offsets.throw;
    s.chr.rt.offsets = offsets(&combat, config.player.combat_art, tool);
    s.chr.rt.offsets.throw = throw;
    let locked = lock.target.is_some();
    let stick = crate::player::stick_world(input.stick, camera.map_or(0.0, |c| c.yaw));
    // The game starts Wolf with the map entry (InitState: W_GroundMapEnter); the sim starts him
    // standing: the script's side plays it out first, with no buttons.
    if !s.started {
        s.started = true;
        {
            let mut h = s.chr.vm.host.borrow_mut();
            answers(&mut h, a, p, &combat, &config, locked, &Pad::default(), None);
            move_vars(&mut h, a, Vec3::ZERO, 0.0, target);
        }
        let lengths = &s.lengths;
        let mut dur = |anim: &str| lengths.get(anim).copied().or_else(|| d.anim(anim).map(|_| d.length(anim)));
        for _ in 0..600 {
            if s.chr.rt.main_state_name() == "StandIdle" || s.chr.tick(1.0 / 60.0, &mut dur).is_err() {
                break;
            }
        }
        // Wolf set in another state before the first frame (a test, a load) is synced below.
        s.state_after = "StandIdle".into();
    }
    if a.airborne {
        s.fall = (p.air_top - places.get(*me).map_or(0.0, |t| t.translation.y)).max(0.0);
    }
    // A landing player.rs found (its ground check runs after the script): the script lands
    // itself on this frame's answers (BEH_R_LAND picks the stiffness by the fall height).
    let landing = s.air && !a.airborne;
    s.air = a.airborne;
    // A new anim in the same state (the throw kill's follow-up) only goes in as the exe's events.
    if a.state != s.state_after && crate::player::is_free(&a.state) && !landing && !graph_free(&s.graph_state(d)) {
        s.to_idle(d, a.crouch);
    }
    // A reaction to this frame's hit is the script's own (below) unless the exe's.
    let mut after_tick = landing;
    if (a.state != s.state_after || a.anim != s.anim_after) && !crate::player::is_free(&a.state) && !landing {
        let hit = damage.is_some();
        // A throw the hit started (the breaking deflect's pose) is the exe's reaction to it: the
        // script does not see the hit as well.
        if s.engine_sync(d, &a.state, &a.anim, a.state != s.state_after && !hit).is_some_and(|e| e.starts_with("W_Throw")) {
            damage = None;
        } else {
            after_tick = hit;
        }
    }
    {
        let mut h = s.chr.vm.host.borrow_mut();
        answers(&mut h, a, p, &combat, &config, locked, &input, damage);
        move_vars(&mut h, a, stick, input.stick.length(), target);
        h.env.insert((224, None), s.fall as f64);
        h.env.insert((248, None), landing as i64 as f64);
        if s.trace >= 3 && !input.press.is_empty() {
            let req: Vec<_> = h.env.iter().filter(|((id, _), v)| (*id == 1106 || *id == 1108) && **v != 0.0).map(|(k, _)| *k).collect();
            eprintln!("hks press {:?} in {} t {:.2}: {req:?}, graph {}", input.press, a.state, a.t, s.chr.rt.main_state_name());
        }
    }
    s.state_before = a.state.clone();
    s.anim_before = a.anim.clone();
    let lengths = &s.lengths;
    let mut dur = |anim: &str| lengths.get(anim).copied().or_else(|| d.anim(anim).map(|_| d.length(anim)));
    match s.chr.tick(time.delta_secs(), &mut dur) {
        Ok(events) => {
            if s.trace >= 3 && !events.is_empty() {
                eprintln!("hks fired {events:?} -> {:?}", s.chr.rt.active_state_names());
            }
        }
        Err(e) => {
            warn!("SHINOBI_HKS: Wolf's script: {e}");
        }
    }
    if s.trace > 0 && s.chr.ticks % 600 == 0 {
        let n = s.chr.ticks as f64;
        eprintln!("hks: cost {:.0} us a tick ({:.0} in the VM) over {n} ticks", s.chr.busy.as_secs_f64() * 1e6 / n, s.chr.vm_busy.as_secs_f64() * 1e6 / n);
    }
    if landing {
        s.fall = 0.0;
    }
    if after_tick {
        if !s.shows(&a.state) && !crate::player::is_free(&a.state) {
            s.engine_sync(d, &a.state, &a.anim, true);
        }
    }
    // player.rs's clip has played out where the graph already went on by itself (its clip-end
    // transition): that is Wolf's state before player.rs decides, as the exe's graph moves him.
    let ended = !a.anim.is_empty() && a.t >= d.length(&a.anim);
    if ended && !landing && a.hp > 0.0 && graph_chains(&a.state) && !graph_free(&s.graph_state(d)) && take_graph(s, d, a, time.delta_secs()) {
        s.state_before = a.state.clone();
        s.anim_before = a.anim.clone();
    }
}

fn apply_script(mut script: NonSendMut<WolfScript>, time: Res<Time>, combat: Res<Combat>, mut wolf: Single<&mut Actor, With<Player>>) {
    let d = &combat.player;
    let s = &mut *script;
    // A state player.rs started is the engine's where the graph takes it as the exe's own events:
    // those win over what the script fired this frame (the exe's throw check takes the press
    // first). Any other the graph does not show yet goes in as W_<state>.
    // A landing goes to the script on the next frame (above): the graph keeps the fall till then.
    if s.air && !wolf.airborne && !s.shows(&wolf.state) {
        return;
    }
    if wolf.state != s.state_before || wolf.anim != s.anim_before {
        if let (Ok(path), Some(at)) = (std::env::var("SHINOBI_HKS_AUDIT"), wolf.played_at) {
            let theirs = s.chr.rt.main_clip().map(|c| c.animation.clone()).unwrap_or_default();
            let ours = wolf.shown_clip(d).0;
            let same = theirs == ours || (theirs.len() > 4 && ours.len() > 4 && theirs[4..] == ours[4..]);
            let line = format!("{}:{}	{}	{}	{}	{}	{}	{}
", at.file(), at.line(), wolf.state, ours, s.graph_state(d), theirs, same, s.shows(&wolf.state));
            use std::io::Write;
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
                let _ = f.write_all(line.as_bytes());
            }
        }
    }
    if (wolf.state != s.state_before || wolf.anim != s.anim_before) && !crate::player::is_free(&wolf.state) {
        let own = !s.shows(&wolf.state) && wolf.state != s.state_before;
        s.engine_sync(d, &wolf.state, &wolf.anim, own);
    }
    if wolf.state != s.state_before && crate::player::is_free(&wolf.state) && !graph_free(&s.graph_state(d)) {
        s.to_idle(d, wolf.crouch);
    }
    sync_clip(s, d, &mut wolf, time.delta_secs());
    s.state_after = wolf.state.clone();
    s.anim_after = wolf.anim.clone();
}

/// Where the graph plays another clip than player.rs, its state and clip win.
fn sync_clip(s: &mut WolfScript, d: &crate::data::CharData, wolf: &mut Actor, dt: f32) {
    let Some(clip) = s.chr.rt.main_clip() else { return };
    let (theirs, t, held) = (clip.animation.clone(), clip.time, clip.at_end());
    // player.rs names a state by its CMSG (the extractor's table); the graph's state can hold
    // several (GroundJumpStart: one per direction).
    let ours = wolf.shown_clip(d).0;
    let state = s.graph_state(d);
    if s.trace == 2 {
        eprintln!("hks frame: ours {} {ours} {:.2} | theirs {state} {theirs} {t:.2}", wolf.state, wolf.t);
    }
    if theirs == ours {
        s.stood_aside = false;
    }
    // Both just standing or moving: player.rs keeps its own locomotion (its blends, stops and
    // turns); the graph takes over where it starts an action.
    let both_free = crate::player::is_free(&wolf.state) && graph_free(&state);
    // player.rs's clip ends on this step (the graph's ran a step ahead): it leaves it itself.
    let ending = !wolf.anim.is_empty() && wolf.t + dt >= d.length(&wolf.anim);
    if theirs == ours || s.stood_aside || both_free || ending || d.anim(&theirs).is_none() {
        return;
    }
    // The graph holds a finished clip (a CMSG with no end event, the crouched items) where
    // player.rs is back to standing or moving: the graph goes back to idle with him.
    if held && crate::player::is_free(&wolf.state) {
        s.to_idle(d, wolf.crouch);
        return;
    }
    take_graph(s, d, wolf, dt);
}

/// Wolf takes the graph's state and clip where it plays an action player.rs does not.
fn take_graph(s: &mut WolfScript, d: &crate::data::CharData, wolf: &mut Actor, dt: f32) -> bool {
    let Some(clip) = s.chr.rt.main_clip() else { return false };
    let (theirs, t) = (clip.animation.clone(), clip.time);
    let ours = wolf.shown_clip(d).0;
    let state = s.graph_state(d);
    if theirs == ours || s.stood_aside || d.anim(&theirs).is_none() {
        return false;
    }
    if s.trace > 0 {
        eprintln!("hks: {} {ours} -> {state} {theirs} at {t:.2}", wolf.state);
    }
    // Advance adds this tick's dt, which the graph's clip has already had.
    wolf.play(&state, &theirs);
    wolf.t = (t - dt).max(0.0);
    wolf.prev_t = wolf.t;
    s.overrides += 1;
    true
}

/// The clip-end transitions the graph makes for player.rs (it no longer picks them):
/// - the ground, sprint and storm jumps' ready into their start, the ground jumps' start into
///   their fall (the side jumps have no graph clip, CMSG anim 0: player.rs plays their raw anims);
/// - the jump arts' ready into the leap and the leap into its fall loop;
/// - the slide's end (HKS state-end of HKB_STATE_SPRINT_TO_CROUCH_READY: TurnAngle picks
///   W_SprintToCrouchLeft / Right).
fn graph_chains(state: &str) -> bool {
    if state.starts_with("Leftside") || state.starts_with("Rightside") || state.starts_with("Aging") {
        return false;
    }
    state.contains("GroundJumpReady")
        || state.ends_with("GroundJumpStart")
        || state == "SprintJumpReady"
        || (state.starts_with("GroundStormJump") && state.ends_with("Ready"))
        || state.starts_with("GroundSpecialAttackJumpReady")
        || state.starts_with("SprintSpecialAttackJumpReady")
        || state.starts_with("GroundSpecialAttackJumpStart")
        || state == "SprintToCrouchReady"
}

/// A graph state with Wolf standing or moving, no action (the c0000 graph's idle, move, stop and
/// turn states).
fn graph_free(state: &str) -> bool {
    let Some(rest) = ["Stand", "Crouch", "DeflectGuard"].iter().find_map(|p| state.strip_prefix(p)) else { return false };
    rest == "Idle"
        || ["MoveStart", "MoveLoop", "MoveLower", "MoveQuickTurn", "QuickTurn", "RunStop", "WalkStop"].iter().any(|p| rest.starts_with(p))
        || rest == "Move"
        || (state.starts_with("DeflectGuard") && rest.starts_with("Move"))
}
