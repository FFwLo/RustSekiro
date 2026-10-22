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

/// env 248's probe reach below the feet (attach 0.3 m + probe radius), from the movement
/// recording's land-ready heights (0.33-0.47 m on flat ground).
const LAND_PROBE: f32 = 0.4;
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
/// `feet`: Wolf's feet above the ground under him (env 248's probe).
pub fn answers(h: &mut Host, a: &Actor, p: &Player, combat: &Combat, config: &crate::config::GameConfig, locked: bool, pad: &Pad, damage: Option<DamageEnv>, feet: Option<f32>) {
    let d = &combat.player;
    let mut env: HashMap<(i64, Option<i64>), f64> = HashMap::new();
    let mut set = |id: i64, arg: Option<i64>, v: bool| {
        env.insert((id, arg), v as i64 as f64);
    };
    // The state Wolf is in; the TAE answers are the clip on screen's (the walk's too).
    let (shown, t) = a.shown_clip(d);
    let anim = !a.anim.is_empty();
    set(201, None, !a.airborne);
    // IsTrulyLanding (env 248, exe FUN_140b36410 case 0xf8): the physics module's ground
    // contact byte +0x929, which its downward sphere probe (FUN_140bce730) sets with the feet
    // within the attach height 0.3 m plus the probe radius of the ground, before touchdown.
    // So the script sees the floor coming (BEH_R_LAND_READY: W_GroundJumpLandReady).
    set(248, None, a.airborne && a.vel_y <= 0.0 && feet.is_some_and(|f| f < LAND_PROBE));
    // A throw's clip (the ThrowParam anim groups a2xx: the plunge deathblow a201_511400) holds
    // Wolf to it: not falling, no BEH_R_FALL.
    let throw_clip = a.anim.get(1..4).and_then(|g| g.parse::<u32>().ok()).is_some_and(|g| (200..300).contains(&g));
    set(200, None, a.airborne && a.vel_y < 0.0 && !throw_clip);
    set(207, None, !a.sheathed);
    // 1105: a STATE_TYPE_STANDBY state (g_paramHkbState); SprintLoop is one (its stop on the
    // dodge let go, BEH_A_GROUND_MOVE_STOP, needs it), and the sprint's walk-out.
    set(1105, None, crate::player::is_standby(&a.state));
    set(2000, None, crate::player::accept_move(d, a));
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
    // A TAE SpEffect is still seen one TAE frame (2 game ticks) after its window: the recordings'
    // attack out of DeflectGuardToStand* (ref 231, frames 0-9 = 18 ticks) is GroundAttackCombo1Reverse
    // up to 19 ticks after W_DeflectGuardToStand, Combo1 from 20 (as the buffered inputs' one TAE
    // frame, docs/kb/input-buffer.md). gap: where the exe keeps the row on is not traced.
    let lag = 1.0 / crate::data::TAE_FPS;
    if t >= lag {
        ids.extend(d.sp_effects_at(&shown, t - lag).into_iter().map(|(id, _)| id).filter(|id| !ids.contains(id)).collect::<Vec<_>>());
    }
    // The additive layer's too (AddHardDeflectGuard's 411).
    if !a.add_anim.is_empty() {
        ids.extend(d.sp_effects_at(&a.add_anim, a.add_t).into_iter().map(|(id, _)| id));
    }
    // The legs' run under an upper action (its MOVING_RUN / WALK refs: player::legs_clip).
    if let Some((legs, lt)) = crate::player::legs_clip(d, a) {
        ids.extend(d.sp_effects_at(&legs, lt).into_iter().map(|(id, _)| id));
    }
    ids.extend(p.timed.iter().map(|t| t.0));
    ids.extend(a.sp_chained.iter().map(|t| t.0));
    // The equipped art's resident SpEffects (the 101 art's back-step unlock 280 and the like).
    let resident: Vec<i64> = if config.player.combat_art > 0 {
        let row = combat.param("EquipParamWeapon", config.player.combat_art);
        ["residentSpEffectId", "residentSpEffectId1", "residentSpEffectId2"].iter().filter_map(|f| row[*f].as_i64().filter(|&id| id > 0)).collect()
    } else {
        Vec::new()
    };
    // The equipped tool's resident SpEffect too (the upgraded Flame Vent's hold, the level-1
    // tools' quick use, ref 314).
    // And the lightning charge (9495 / 9490: refs 151 / 356, the air attack throws it back), and
    // Wolf's own residents (the aged form's SP_EF_REF_AGING 591 row: status::set_aging).
    let resident: Vec<i64> = resident.into_iter().chain(crate::player::equipped_tool(combat, config, p.tool_slot).map(|t| t.resident).filter(|&id| id > 0)).chain(a.electro.map(|(id, _)| id)).chain(a.resident.iter().copied()).collect();
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
    // The gourd run dry is the empty gourd, goods 3001 (goodsUseAnim 10 ITEM_GOURD_RECOVER_EMPTY:
    // the shake, or the repeat's failed clip in its window, HKS 4010-4036), still usable.
    let gourd = goods == 3000;
    if gourd && p.gourd == 0 {
        goods = 3001;
    }
    // itemUseEnable: one left, and no worn SpEffect whose stateInfo is one of the goods'
    // useLimitSpEffectType (the Owl's recovery seal on the gourd).
    let limits = crate::data::goods_use_limits(goods);
    let limited = crate::combat::worn_sp(a, &p.timed).iter().any(|id| crate::data::sp_cycle(*id).is_some_and(|c| limits.contains(&c.state_info)));
    let left = goods > 0 && (crate::items::count(config, goods) > 0 || crate::items::count(config, goods - 1) == 0 && crate::items::empty_state(goods - 1).is_some());
    set(233, None, !limited && (gourd || left));
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
    // 3063: the clip's time in ms (the Land versions start there: SetVariable StartTime_00).
    // 1: the upper body's (an upper action's clip, else the full-body one: a standing action's
    // walk-off and an upper action's stop go on at its time, StartTime_01); 2: the legs' loop
    // (StartTime_02 of the move loop / start events).
    let shown_ms = (a.shown_clip(d).1 * 1000.0) as f64;
    env.insert((3063, Some(0)), shown_ms);
    env.insert((3063, Some(1)), shown_ms);
    env.insert((3063, Some(2)), crate::player::legs_clip(d, a).map_or(shown_ms, |(_, t)| (t * 1000.0) as f64));
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
    // 3032: the exe's 8-way sectors 2-5, the back half (SELECTOR_DAMAGE_DIR_B 1, else F 0).
    env.insert((3032, None), (dm.back_half || dm.dir == 3) as i64 as f64);
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
    // GetVerticalSpeed (env 3003, case 0xbbb): the module's vy (+0x164, m/s) x 1000, so the
    // land-ready gate LAND_READY_ENABLE_FALL_SPEED -2000 is a 2 m/s fall.
    env.insert((3003, None), (a.vel_y * 1000.0).trunc() as f64);
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
    // Locked on, TurnAngle is the target's angle, but a sprint faces the stick (its TAE 224
    // turn speeds are not lock-on ones): BEH_A_GROUND_MOVE_STOP reads TurnAngle in
    // SprintStartFromStep, and the game keeps a locked back-step sprint going (rec 20261015_*).
    let turn_angle = target.filter(|_| !crate::player::is_sprint_style(&a.state)).map_or(turn, off);
    h.vars.insert("TurnAngle".into(), turn_angle);
    // Slot 3 of the exe's per-frame table: NS_SPRJ::WalkTwist+0x50 (FUN_1407f7550). Locked on,
    // the stick against the facing in 60 deg cones with 5 deg of hysteresis (F 0, B 1, L 2 /
    // R 3), held with the stick at rest; else -1, written as 0 (F): the twist runs locked on
    // (FUN_1407f6ff0 / FUN_1407e7900), so an unlocked 180 turn picks the turn clip
    // (StandRunQuickTurn*_F), not the backward start. The guard walk keeps Wolf's facing
    // unlocked too and its selector (DeflectGuardMove_*) reads the cones (inferred: the
    // twist's on flag WalkTwist+0x58 is not traced).
    let prev = h.vars.get("MoveDirection").copied().unwrap_or(0.0) as i64;
    let dir = if target.is_none() && !crate::player::is_guard_style(&a.state) {
        0
    } else if level <= 0.0 {
        prev
    } else {
        let a = turn.abs();
        let widen = |d: i64| if prev == d { 5.0 } else { -5.0 };
        if a < 60.0 + widen(0) { 0 } else if a > 120.0 - widen(1) { 1 } else if turn < 0.0 { 2 } else { 3 }
    };
    h.vars.insert("MoveDirection".into(), dir as f64);
    h.vars.insert("TwistLowerRootAngle".into(), twist);
    // Docked (BEH_A_DOCKING_MOVE_START / _STOP, c0000_transition 5851-5860): the stick's angle,
    // positive = the clips' R side (hanging, Wolf's own right; hugging, his left: the hug clips
    // are named from the front, docking.rs held_state), and the side of the move clip playing
    // (SELECTOR_DOCKING_MOVE_DIR_L 0 / _R 1). Docking.rs picks the clips; unset, the stop's
    // checks read angle 0 against selector R and stopped every shimmy each tick (start / end
    // flicker, no move loop).
    let hug = a.state.starts_with("Cover");
    h.vars.insert("DockingStickAngle".into(), if hug { -rel } else { rel });
    if (a.state.starts_with("Hang") || hug) && ["Move", "Walk", "Run"].iter().any(|k| a.state.contains(k)) {
        match a.state.chars().last() {
            Some('L') => {
                h.vars.insert("Selector_DockingMoveDir".into(), 0.0);
            }
            Some('R') => {
                h.vars.insert("Selector_DockingMoveDir".into(), 1.0);
            }
            _ => {}
        }
    }
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
            .add_systems(FixedUpdate, ((crate::player::throw_check, tick_script).chain().before(crate::actor::ActorSet::Decide), (land_script, apply_script).chain().after(crate::actor::ActorSet::Decide).before(crate::actor::ActorSet::Advance)));
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
    /// The last airborne frame's state, clip and time: BEH_R_LAND runs on them (player.rs found
    /// the ground after the script and has already put its landing in).
    air_pose: (String, String, f32),
    trace: u8,
    /// The events the script fired this frame (the replay of a recording compares them with the game's).
    pub fired: Vec<String>,
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
        Some(Self { chr, lengths, overrides: 0, started: false, state_before: String::new(), anim_before: String::new(), state_after: String::new(), anim_after: String::new(), stood_aside: false, throw_id: None, fall: 0.0, air: false, air_pose: Default::default(), trace, fired: Vec::new() })
    }

    /// A state the engine started, fired on the graph as the exe does: a throw pair (W_ThrowAtk /
    /// W_ThrowDef<anim>), a throw kill, a map event (W_Event<anim>); with `own` also W_<state>
    /// (the script fired nothing this frame). Stands aside when the graph has none of them.
    /// The graph's state as player.rs names it: its main clip's CMSG where player.rs has it (the
    /// extractor's table), else the graph state.
    #[cfg(test)]
    pub fn global_number(&self, name: &str) -> Option<f32> {
        self.chr.vm.global_number(name)
    }

    fn graph_state(&self, d: &crate::data::CharData) -> String {
        let main = self.chr.rt.main_state_name();
        let clip = self.chr.rt.main_clip();
        // A selector's CMSG player.rs has no state for (SprintStartFromStep_R): its name, when
        // its clip is Wolf's.
        let named = |c: &&str| d.anim_key(c).is_some() || (c.starts_with(main) && clip.is_some_and(|cl| d.anim(&cl.animation).is_some()));
        let cmsg = clip.and_then(|c| self.chr.rt.clip_cmsg(c)).filter(named);
        cmsg.unwrap_or(main).to_string()
    }

    /// player.rs went back to standing or moving where the graph still plays an action (a
    /// respawn, a cancel of its own): the graph goes back to idle with him.
    #[track_caller]
    fn to_idle(&mut self, d: &crate::data::CharData, crouch: bool) {
        let lengths = &self.lengths;
        let mut dur = |a: &str| lengths.get(a).copied().or_else(|| d.anim(a).map(|_| d.length(a)));
        let idle = if crouch { "W_CrouchIdle" } else { "W_Idle" };
        let _ = self.chr.engine_event(idle, &mut dur);
        if self.trace > 0 {
            eprintln!("hks: {} -> {idle} ({})", self.graph_state(d), std::panic::Location::caller().line());
        }
    }

    /// The graph shows Wolf's state: its main clip's CMSG or its state by name, or the same clip
    /// (one CMSG can hold several of player.rs's states: StandDamageBreak's DamageDirection picks
    /// StandDamageBreak_F / _B; another anim set's copy counts too).
    fn shows(&self, d: &crate::data::CharData, wolf: &Actor) -> bool {
        let state = wolf.state.as_str();
        let Some(clip) = self.chr.rt.main_clip() else { return self.chr.rt.main_state_name() == state };
        let ours = wolf.shown_clip(d).0;
        self.chr.rt.clip_cmsg(clip) == Some(state) || self.chr.rt.main_state_name() == state || same_clip(&clip.animation, &ours)
    }

    fn engine_sync(&mut self, d: &crate::data::CharData, state: &str, anim: &str, own: bool) -> Option<String> {
        let n = anim.rsplit('_').next().and_then(|n| n.parse::<u32>().ok());
        // A throw's clips are in its ThrowParam anim group (offsetType 17): the one player.rs
        // took (a211_500000: 211).
        let group = anim.strip_prefix('a').and_then(|a| a.split('_').next()).and_then(|g| g.parse::<i64>().ok());
        if let Some(g) = group.filter(|&g| g > 0) {
            self.chr.rt.offsets.throw = Some(g);
        }
        // Only where that state's CMSG plays the anim's number (W_Event390000 plays a000_710000, not
        // the aged attack a050_390000: that is W_AgingGroundAttack).
        let rt = &self.chr.rt;
        let mut names: Vec<String> = n
            .map(|n| ["ThrowAtk", "ThrowDef", "ThrowKill", "Event"].iter().filter(|k| rt.cmsg_anim_id(&format!("{k}{n}_CMSG")).is_none_or(|id| id == n as i64)).map(|k| format!("W_{k}{n}")).collect())
            .unwrap_or_default();
        // The grab's own follow-ups carry its ThrowId (W_ThrowDefDeathStart600000).
        if let Some(id) = self.throw_id.filter(|_| state.starts_with("ThrowDef")) {
            names.push(format!("W_{state}{id}"));
        }
        if own {
            names.push(format!("W_{state}"));
            // A state named with its selectors' picks (StandDeflectEasySmall_V1_F): the selectors
            // point at its CMSG and the event is the bare name (W_StandDeflectEasySmall).
            if self.chr.rt.graph().node_by_name(state).is_none() {
                let picks = self.chr.rt.select_path(&format!("{state}_CMSG"));
                let mut bare = state;
                while !picks.is_empty() && let Some((head, _)) = bare.rsplit_once('_') {
                    names.push(format!("W_{head}"));
                    bare = head;
                }
                // Named apart from its state (VerticalGroundJumpFall in GroundJumpFall).
                if !picks.is_empty() && let Some(held) = self.chr.rt.holding_state(&format!("{state}_CMSG")) {
                    names.push(format!("W_{held}"));
                }
                let mut host = self.chr.vm.host.borrow_mut();
                for (var, v) in picks {
                    host.vars.insert(var, v as f64);
                }
            }
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
fn tick_script(mut script: NonSendMut<WolfScript>, time: Res<Time>, combat: Res<Combat>, config: Res<crate::config::GameConfig>, lock: Res<crate::view::LockOn>, pad: ResMut<crate::player::PadInput>, view: Res<crate::view::View>, places: Query<&Transform>, wolf: Single<(Entity, &mut Actor, &mut Player)>) {
    script.fired.clear();
    run_script(script, time.delta_secs(), false, combat, config, lock, pad, view, places, wolf);
}

/// The landing player.rs's decide found this frame (its ground check runs after the script):
/// the script ticks again at once (no time passes), as the exe's ground check comes before its
/// HKS update, so BEH_R_LAND picks the landing on the same frame.
fn land_script(script: NonSendMut<WolfScript>, combat: Res<Combat>, config: Res<crate::config::GameConfig>, lock: Res<crate::view::LockOn>, pad: ResMut<crate::player::PadInput>, view: Res<crate::view::View>, places: Query<&Transform>, wolf: Single<(Entity, &mut Actor, &mut Player)>) {
    if !(script.air && !wolf.1.airborne) {
        return;
    }
    run_script(script, 0.0, true, combat, config, lock, pad, view, places, wolf);
}

#[allow(clippy::too_many_arguments)]
fn run_script(mut script: NonSendMut<WolfScript>, dt: f32, land_tick: bool, combat: Res<Combat>, config: Res<crate::config::GameConfig>, lock: Res<crate::view::LockOn>, mut pad: ResMut<crate::player::PadInput>, view: Res<crate::view::View>, places: Query<&Transform>, mut wolf: Single<(Entity, &mut Actor, &mut Player)>) {
    let (me, a, p) = &mut *wolf;
    let target = lock.target.and_then(|t| Some((places.get(t).ok()?.translation - places.get(*me).ok()?.translation).with_y(0.0).normalize_or_zero()));
    let mut damage = a.damage_env.take();
    let held = [(pad.attack_held, Action::Attack), (pad.dodge_held, Action::Step), (pad.prosthetic_held, Action::Prosthetic)].into_iter().filter_map(|(h, a)| h.then_some(a)).collect();
    let input = Pad { press: pad.pressed().to_vec(), held, guard_held: pad.guard_held, stick: pad.stick };
    let s = &mut *script;
    // The last tick's request (cleared when this one runs).
    a.fall_protect = s.chr.vm.host.borrow().fall_protect;
    let d = &combat.player;
    let tool = crate::player::equipped_tool(&combat, &config, p.tool_slot).map_or(-1, |t| t.id);
    let throw = s.chr.rt.offsets.throw;
    s.chr.rt.offsets = offsets(&combat, config.player.combat_art, tool);
    s.chr.rt.offsets.throw = throw;
    s.chr.rt.offsets.idle = if a.sheathed { 10 } else { 0 };
    let locked = lock.target.is_some();
    let stick = crate::player::stick_world(input.stick, view.orbit.map_or(0.0, |o| o.0));
    // The game starts Wolf with the map entry (InitState: W_GroundMapEnter); the sim starts him
    // standing: the script's side plays it out first, with no buttons.
    if !s.started {
        s.started = true;
        {
            let mut h = s.chr.vm.host.borrow_mut();
            answers(&mut h, a, p, &combat, &config, locked, &Pad::default(), None, None);
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
    // An exe landing (the plunge deathblow's throw, the fall death's return) is not BEH_R_LAND's.
    let exe_land = landing && (a.state == "Deathblow" || a.state == "LandDeadReturnStart" || a.anim.get(1..4).and_then(|g| g.parse::<u32>().ok()).is_some_and(|g| (200..300).contains(&g)));
    let landing = landing && !exe_land;
    if a.airborne {
        s.air_pose = (a.state.clone(), a.anim.clone(), a.t);
    }
    // A new anim in the same state (the throw kill's follow-up) only goes in as the exe's events.
    // (Not where the graph already shows it: the script's own W_CrouchStart.)
    if a.state != s.state_after && crate::player::is_free(&a.state) && !landing && !graph_free(&s.graph_state(d)) && !s.shows(d, a) {
        s.to_idle(d, a.crouch);
    }
    // A state set outside player.rs's decide (combat.rs's hits and grab ends, a map event, a
    // test or a load) goes in before the tick as the exe's own events (throws, events), else as
    // W_<state>. Two the script picks itself first, synced after the tick only where the graph
    // did not get there: a hit's reaction, and what follows a clip the graph has played out (the
    // grab end's blow-away, whose prone side the script sets).
    let mut after_tick = landing;
    let clip_end = s.chr.rt.main_clip().is_some_and(|c| c.at_end());
    if (a.state != s.state_after || a.anim != s.anim_after) && !crate::player::is_free(&a.state) && !landing {
        let defer = damage.is_some() || clip_end;
        // A throw the hit started (the breaking deflect's pose) is the exe's reaction to it: the
        // script does not see the hit as well.
        if s.engine_sync(d, &a.state, &a.anim, a.state != s.state_after && !defer).is_some_and(|e| e.starts_with("W_Throw")) {
            damage = None;
        } else {
            after_tick = defer;
        }
    }
    {
        let mut h = s.chr.vm.host.borrow_mut();
        // Landing: the answers are the air pose's (ref 201 on the air clip at touchdown).
        let landed = landing.then(|| {
            let (st, an, t) = s.air_pose.clone();
            let landed = (std::mem::replace(&mut a.state, st), std::mem::replace(&mut a.anim, an), a.t, a.prev_t);
            (a.t, a.prev_t) = (t, t);
            landed
        });
        let feet = places.get(*me).ok().map(|t| t.translation.y - crate::player::CAPSULE_HALF_HEIGHT - crate::terrain::ground_y(t.translation));
        answers(&mut h, a, p, &combat, &config, locked, &input, damage, feet);
        if let Some((st, an, t, prev_t)) = landed {
            (a.state, a.anim, a.t, a.prev_t) = (st, an, t, prev_t);
        }
        // The keyboard's walk key holds a full stick: the script sees a half-tilted one (its
        // _SpeedUpdate walks at 0.7 or below).
        let level = if pad.walk { input.stick.length().min(0.5) } else { input.stick.length() };
        move_vars(&mut h, a, stick, level, target);
        // GetFallHeight in whole centimetres (exe FUN_140b97880: peak Y minus Y, x 100).
        h.env.insert((224, None), (s.fall * 100.0).floor() as f64);
        // Falling (env 200): off the ground; a walk-off on the move keeps ground contact for
        // player.rs's FALL_START_DROP (exe FUN_140b97970's ground probe, a gap).
        if a.airborne && crate::player::moving_locomotion(d, a) && s.fall < crate::player::FALL_START_DROP {
            h.env.insert((200, None), 0.0);
        }
        // A Mist Raven leap ending on the floor is on it (HKS 557: W_LandAirSubAttackMove).
        let on_floor = places.get(*me).is_ok_and(|t| t.translation.y <= crate::terrain::ground_y(t.translation) + crate::player::CAPSULE_HALF_HEIGHT + 0.05);
        let leap_down = on_floor && (a.state.starts_with("SubAttackJumpStart_") || a.state.starts_with("AirSubAttackMoveStart_")) && !a.anim.is_empty() && a.t >= d.length(&a.anim);
        // env 248: answers' probe, and the touchdown itself.
        let near = h.env.get(&(248, None)) == Some(&1.0);
        h.env.insert((248, None), (landing || leap_down || near) as i64 as f64);
        if s.trace >= 3 && !input.press.is_empty() {
            let req: Vec<_> = h.env.iter().filter(|((id, _), v)| (*id == 1106 || *id == 1108) && **v != 0.0).map(|(k, _)| *k).collect();
            eprintln!("hks press {:?} in {} t {:.2}: {req:?}, graph {}", input.press, a.state, a.t, s.chr.rt.main_state_name());
        }
    }
    s.state_before = a.state.clone();
    s.anim_before = a.anim.clone();
    let lengths = &s.lengths;
    let mut dur = |anim: &str| lengths.get(anim).copied().or_else(|| d.anim(anim).map(|_| d.length(anim)));
    let events = match s.chr.tick(dt, &mut dur) {
        Ok(events) => {
            if s.trace >= 3 && !events.is_empty() {
                eprintln!("hks fired {events:?} -> {:?}", s.chr.rt.active_state_names());
            }
            events
        }
        Err(e) => {
            warn!("SHINOBI_HKS: Wolf's script: {e}");
            Vec::new()
        }
    };
    s.fired.extend(events.iter().cloned());
    let fired = !events.is_empty();
    if s.trace > 0 && s.chr.ticks % 600 == 0 {
        let n = s.chr.ticks as f64;
        eprintln!("hks: cost {:.0} us a tick ({:.0} in the VM) over {n} ticks", s.chr.busy.as_secs_f64() * 1e6 / n, s.chr.vm_busy.as_secs_f64() * 1e6 / n);
    }
    if landing {
        s.fall = 0.0;
    }
    // The landing is the script's (BEH_R_LAND): its pick replaces player.rs's from the frame
    // before (that one found the ground after the script ran).
    // Not a ledge catch (docking.rs), the aged form's own style, nor a state set outside decide
    // (a reset between frames).
    // BEH_R_LAND's events (the lands, the move start out of a fall, the step out of it).
    let land_event = events.iter().any(|e| e.contains("Land") || e.starts_with("W_StandMoveStartFrom") || e == "W_GroundStep");
    let land_led = landing && land_event && a.hp > 0.0 && (land_tick || a.state == s.state_after) && !a.state.contains("Hang") && !a.state.contains("Aging") && {
        let graph = s.graph_state(d);
        if s.trace > 0 {
            eprintln!("hks: land |{}|{}| -> |{graph}| {events:?} fall {:.2} air {:?}", a.state, a.anim, s.fall, s.air_pose);
        }
        if graph_free(&graph) {
            if !crate::player::is_free(&a.state) {
                a.procedural(if graph.ends_with("Idle") { "StandIdle" } else { "Locomotion" });
            }
            true
        } else if graph.starts_with("GroundStep") {
            // The dodge out of the landing (_fireGroundStep) spends the press.
            let stepped = take_press(s, d, a, p, &mut pad, Action::Step, false, dt);
            if stepped {
                step_aim(s, a, p, lock.target.is_none(), stick);
            }
            stepped || s.shows(d, a)
        } else {
            take_graph(s, d, a, dt) || s.shows(d, a)
        }
    };
    if land_led {
        s.state_before = a.state.clone();
        s.anim_before = a.anim.clone();
    }
    // The script went on by itself at its clip's end: that stands.
    if after_tick && !land_led && !(fired && clip_end) {
        if !s.shows(d, a) && !crate::player::is_free(&a.state) {
            s.engine_sync(d, &a.state, &a.anim, true);
        }
    }
    // player.rs's clip has played out where the graph already went on by itself (its clip-end
    // transition): that is Wolf's state before player.rs decides, as the exe's graph moves him.
    let ended = !a.anim.is_empty() && a.t >= d.length(&a.anim);
    // In the air every clip end is the graph's (its end transitions: the fall loops, the air
    // art's loop, the guard's loop or end), not on the wire (wire.rs).
    let air_chain = a.airborne && p.wire.is_none() && !crate::wire::is_wire_state(&a.state) && !a.state.contains("Hang") && !a.state.contains("Aging");
    // On the ground too: not the exe's throws, deathblows, events, deaths, the idol, the hang,
    // the swim, the wire, the aged form.
    let exe_state = ["Throw", "Deathblow", "Event", "Dead", "Death", "Hang", "Aging", "Swim", "Dive", "Wire", "Cover", "Docking", "Revival", "Fall", "Statue"].iter().any(|k| a.state.contains(k));
    let ground_chain = !a.airborne && !exe_state;
    if ended && !landing && a.hp > 0.0 && (graph_chains(&a.state) || air_chain || ground_chain) {
        let graph = s.graph_state(d);
        // The graph's idle is player.rs's procedural one (its crouch idle by `a.crouch`).
        let led = if matches!(graph.as_str(), "StandIdle" | "CrouchIdle") {
            a.procedural("StandIdle");
            true
        } else if graph_free(&graph) && graph.starts_with("DeflectGuard") && !in_guard_walk(&a.state) {
            // The graph's guard idle or walk: player.rs's guard idle (it picks the walk's side).
            a.play_state(d, "DeflectGuardIdle")
        } else if graph_free(&graph) && graph.contains("Move") && !graph.starts_with("DeflectGuard") && stick != Vec3::ZERO {
            // The graph's move (StandMoveLoop out of a move start): player.rs's procedural one.
            // Out of an upper action the legs' loop keeps its phase (it ran on the action's clock).
            let (t, prev_t, upper) = (a.t, a.prev_t, crate::player::is_upper_action(&a.state));
            a.procedural("Locomotion");
            if upper {
                (a.t, a.prev_t, a.move_start) = (t, prev_t, 0.0);
            }
            true
        } else if graph == a.state && s.chr.rt.main_clip().is_some_and(|c| c.animation == a.shown_clip(d).0) {
            // The same state on the same clip: the graph's generator looped (MODE_LOOPING,
            // SprintLoop); its clock wrapped, so ours does.
            let t = s.chr.rt.main_clip().map_or(0.0, |c| c.time);
            let anim = a.anim.clone();
            a.play(&graph, &anim);
            a.t = (t - dt).max(0.0);
            a.prev_t = a.t;
            true
        } else {
            !graph_free(&graph) && take_graph(s, d, a, dt)
        };
        if led {
            s.state_before = a.state.clone();
            s.anim_before = a.anim.clone();
        }
    }
    // The graph's own move loop out of a graph-led clip (StandMoveLoopFromSprint's end): player.rs's
    // Locomotion, on the graph's clock, before player.rs moves him this step (shown as the graph's
    // clip, the stick did not steer and Wolf ran on one way until the loop's clip ended).
    if !landing && a.hp > 0.0 && !a.airborne && !crate::player::is_free(&a.state) && matches!(s.graph_state(d).as_str(), "StandMoveLoop" | "CrouchMoveLoop") {
        let t = s.chr.rt.main_clip().map_or(0.0, |c| c.time);
        a.procedural("Locomotion");
        (a.move_start, a.move_start_clip) = (0.0, None);
        // Wolf is already on the move: the walk / run level, not a ramp from a standstill (the
        // sprint kept player.rs's level at 0, read as standing by the stop and quick-turn checks).
        p.speed_level = p.speed_level.max(if pad.walk { 1.0 } else { 2.0 });
        a.t = (t - dt).max(0.0);
        a.prev_t = a.t;
        s.state_before = a.state.clone();
        s.anim_before = a.anim.clone();
    }
    if fired && !landing && a.hp > 0.0 && take_crouch(s, d, a, p, &mut pad, dt) {
        s.state_before = a.state.clone();
        s.anim_before = a.anim.clone();
    }
    // The locked turn in place out of a standby state (_GroundQuickTurn, BEH_A_GROUND_QUICK_TURN
    // or the run / walk stop's MOVE_STOP: W_StandQuickTurn{Right,Left}{90,180}): player.rs turns
    // from StandIdle itself; out of the stops the script's event was never taken and fired every
    // frame (the replay with the lock target: 345 such, the game 18).
    if !landing && a.hp > 0.0 && fired && a.state != "StandIdle" {
        let turn = events.iter().filter_map(|e| e.strip_prefix("W_")).find(|e| (e.starts_with("StandQuickTurnRight") || e.starts_with("StandQuickTurnLeft")) && (e.ends_with("90") || e.ends_with("180")));
        if let Some(st) = turn.filter(|st| a.state != *st) {
            if a.play_state(d, st) {
                a.move_vel = Vec3::ZERO;
                s.state_before = a.state.clone();
                s.anim_before = a.anim.clone();
            }
        }
    }
    // The 180 turn in a stop's first 9 frames (BEH_A_STAND_MOVE_QUICK_TURN: ref 14 and |TurnAngle|
    // > 135, checked every frame with no input needed): locked on, TurnAngle is the target's
    // angle (exe FUN_1407f0f50, stick or not), so a target behind turns him with the stick at
    // rest. player.rs turns only by the stick, so the event was never taken and fired every frame
    // (the replays: 18 of ours, 3 of the game's). Its clip's root carries no turn: the heading
    // takes the target's way at once, as player.rs does with the stick's.
    if !landing && a.hp > 0.0 && fired && (a.state.starts_with("StandRunStop") || a.state.starts_with("StandWalkStop")) {
        let side = events.iter().find_map(|e| match e.as_str() {
            "W_StandMoveQuickTurnRight180" => Some("Right"),
            "W_StandMoveQuickTurnLeft180" => Some("Left"),
            _ => None,
        });
        if let (Some(side), Some(to)) = (side, target.filter(|t| *t != Vec3::ZERO)) {
            let kind = if a.state.starts_with("StandWalkStop") { "StandWalkQuickTurn" } else { "StandRunQuickTurn" };
            // (its clip: the CMSG's front variant, as player.rs picks it)
            if let Some(key) = d.anim_key(&format!("{kind}{side}180_F_CMSG_Anime")).map(str::to_string) {
                a.play(&format!("{kind}{side}180"), &key);
                a.yaw = crate::player::yaw_of(to);
                a.move_vel = Vec3::ZERO;
                s.state_before = a.state.clone();
                s.anim_before = a.anim.clone();
            }
        }
    }
    // The move's stop (BEH_A_GROUND_MOVE_STOP: W_StandRunStop / W_StandWalkStop, crouched
    // W_CrouchRunStop / W_CrouchWalkStop): player.rs stopped only past its own speed level, so a
    // short tap stayed in the move and the script fired the stop every frame (the replays: 216
    // W_StandRunStop of ours, 167 of the game's).
    // In a quick turn's clip too (its run refs 91 count as the move: the locked turn with the
    // stick at rest stopped in the script, ours ran it out firing the stop every frame).
    let turning = crate::player::quick_turn_kind(&a.state).is_some();
    if !landing && a.hp > 0.0 && fired && (a.state == "Locomotion" || turning) && !a.airborne {
        let stop = events.iter().find_map(|e| match e.as_str() {
            "W_StandRunStop" | "W_CrouchRunStop" => Some(true),
            "W_StandWalkStop" | "W_CrouchWalkStop" => Some(false),
            _ => None,
        });
        if let Some(run) = stop {
            if crate::player::play_move_stop(d, a, p, run) {
                s.state_before = a.state.clone();
                s.anim_before = a.anim.clone();
            }
        }
    }
    if !landing && a.hp > 0.0 && fired {
        take_item(s, d, a, p, &mut pad, &config, &events, dt);
    }
    // The deflect starts and the spam chain (HKS 4443-4477), and the held-button start.
    if !landing && a.hp > 0.0 && events.iter().any(|e| e.starts_with("W_StandToDeflectGuard") || e == "W_SprintToDeflectGuard" || e == "W_StandDeflectGuardFromDamage") {
        // HKS 6161: the additive deflect's 411 with the base's 412 starts it with the tap spent.
        use crate::player::{REF_ADD_GUARD_CANCEL, REF_ADD_INPUT_GUARD, wolf_ref};
        let held = pad.guard_held || (wolf_ref(d, a, p, REF_ADD_INPUT_GUARD) && wolf_ref(d, a, p, REF_ADD_GUARD_CANCEL));
        take_press(s, d, a, p, &mut pad, Action::Guard, held, dt);
    }
    // Moving or stopping in the guard style (HKS 2340 / 2421: W_DeflectGuardMove,
    // W_DeflectGuardIdle) out of a guard start, a guard reaction or the air guard's landing:
    // player.rs's guard idle, which picks the walk's side.
    if !landing && a.hp > 0.0 && !a.airborne && !in_guard_walk(&a.state) && events.iter().any(|e| e == "W_DeflectGuardMove" || e == "W_DeflectGuardIdle") && a.play_state(d, "DeflectGuardIdle") {
        s.state_before = a.state.clone();
        s.anim_before = a.anim.clone();
    }
    // The attack button (BEH_A_GROUND_ATTACK, HKS 4300-4370): the combo refs, the step, sprint,
    // crouch and deflect follow-ups.
    if !landing && a.hp > 0.0 && !p.deathblow_ready && events.iter().any(|e| ground_attack_event(e) || sub_derive_event(e)) {
        take_press(s, d, a, p, &mut pad, Action::Attack, false, dt);
    }
    // The combat art (BEH_A_GROUND_SP_ATTACK: attack with guard held, or the art key) and its
    // combo presses (_FireSpAttackCombo): the attack press or the art key's is spent, and the
    // guard tap it paired with.
    if !landing && a.hp > 0.0 && !p.deathblow_ready && events.iter().any(|e| art_press_event(e)) {
        for act in [Action::CombatArt, Action::Attack] {
            if take_press(s, d, a, p, &mut pad, act, false, dt) {
                p.requests.remove(&Action::Guard);
                break;
            }
        }
    }
    // The step or jump out of a hit or a knockdown (_execDamageStep: W_StandStepFromDamage /
    // W_StandStepFromDown, the side and direction by selector; its tilt is act 3025).
    if !landing && a.hp > 0.0 && events.iter().any(|e| e == "W_StandStepFromDamage" || e == "W_StandStepFromDown") {
        let tilt = s.chr.vm.host.borrow().act_values.get(&3025).copied();
        if take_press(s, d, a, p, &mut pad, Action::Step, false, dt) || take_press(s, d, a, p, &mut pad, Action::Jump, false, dt) {
            p.requests.remove(&Action::Step);
            p.requests.remove(&Action::Jump);
            a.move_vel = Vec3::ZERO;
            p.jump_tilt = tilt.map_or(0.0, |t| (-t as f32).to_radians());
        }
    }
    // The additive deflect over a recoil (W_AddHardDeflectGuard, HKS 4438 / 2968: ref 228
    // without 412): the recoil plays on under it.
    if !landing && a.hp > 0.0 && events.iter().any(|e| e == "W_AddHardDeflectGuard") && (p.requests.contains_key(&Action::Guard) || pad.pressed().contains(&Action::Guard)) {
        if let Some(k) = d.anim_key("AddHardDeflectGuard") {
            a.add_anim = k.to_string();
            a.add_t = 0.0;
            p.requests.remove(&Action::Guard);
            pad.take(Action::Guard);
        }
    }
    // In the air (BEH_A_AIR_*, g_behaviorValidateOrder: the storm jump and the kick, the art, the
    // attacks, the deflect), not on the wire (wire.rs). The plunge throw goes first.
    if !landing && a.hp > 0.0 && a.airborne && p.wire.is_none() {
        let wall = places.get(*me).is_ok_and(|t| crate::player::wall_jump_open(d, a, p, t.translation - Vec3::Y * crate::player::CAPSULE_HALF_HEIGHT));
        for e in &events {
            let acts: &[Action] = match e.as_str() {
                // The wall jump is docking.rs's (env 3020 wall contact is not answered).
                "W_AirKick" if wall => continue,
                "W_AirKick" | "W_AirStormJumpStart" | "W_AirStormJumpWeakStart" => &[Action::Jump],
                "W_AirDeflectGuardStart" => &[Action::Guard],
                n if n.starts_with("W_AirSpecialAttack") && !n.contains("Loop") => &[Action::CombatArt, Action::Attack],
                n if !p.plunge_ready && (n.starts_with("W_AirComboAttack") || n.ends_with("ElectroReceiveAttack") || n == "W_AirSubAttackDeriveAttack" || n == "W_GroundSpecialAttackHitJumpDeriveAction") => &[Action::Attack, Action::CombatArt],
                _ => continue,
            };
            let held = acts[0] == Action::Guard && pad.guard_held;
            if acts.iter().any(|&act| take_press(s, d, a, p, &mut pad, act, held, dt)) {
                if acts.contains(&Action::CombatArt) {
                    p.requests.remove(&Action::Guard);
                }
                break;
            }
        }
    }
    // The add-action inputs (BEH_A_ADD_ACTION_INPUT_*, HKS 4733-4751): the graph's add layer
    // keeps the press (the TAE resends it, HKS 717), so the request is spent like any action's.
    // The game fires each once per press; a kept request re-fires every frame and shadows the
    // later behaviours in g_behaviorValidateOrder (the air attack after a kick).
    if !landing && a.hp > 0.0 {
        for e in &events {
            let act = match e.as_str() {
                // The kick's own activation fires it too (HKS 2823): the kick takes the press
                // above, or docking.rs's wall jump does.
                "W_AddActionInputJump" if events.iter().any(|e| e == "W_AirKick") => continue,
                "W_AddActionInputJump" => Action::Jump,
                "W_AddActionInputKick" => Action::Step,
                "W_AddActionInputUseItem" => Action::UseItem,
                "W_AddActionInputCrouch" => Action::Crouch,
                "W_AddActionInputWireShoot" => Action::Grapple,
                "W_AddActionInputSubAttack" => Action::Prosthetic,
                _ => continue,
            };
            p.requests.remove(&act);
            pad.take(act);
        }
    }
    // The air guard let go (BEH_A_AIR_DEFLECT_END, HKS 6066).
    if !landing && a.hp > 0.0 && a.airborne && p.wire.is_none() && events.iter().any(|e| e == "W_AirDeflectGuardEnd") && take_graph(s, d, a, dt) {
        s.state_before = a.state.clone();
        s.anim_before = a.anim.clone();
    }
    // The jump (BEH_A_GROUND_JUMP): the storm, sprint, swamp and ground jumps. The side jumps'
    // CMSGs have anim 0 (the graph's state is GroundJumpReady over a000_201102 / 201103): they are
    // player.rs's Leftside / RightsideGroundJumpReady.
    const JUMPS: [&str; 8] = ["W_GroundJumpReady", "W_GroundJumpStart", "W_SprintJumpReady", "W_SprintJumpStart", "W_GroundStormJumpReady", "W_GroundStormJumpWeakReady", "W_SwampJumpReady", "W_SwampJumpStart"];
    if !landing && a.hp > 0.0 && !p.jump_throw_ready && events.iter().any(|e| JUMPS.contains(&e.as_str())) && take_press(s, d, a, p, &mut pad, Action::Jump, false, dt) && a.state.starts_with("GroundJumpReady") {
        match a.anim.as_str() {
            "a000_201102" => a.state = "LeftsideGroundJumpReady".into(),
            "a000_201103" => a.state = "RightsideGroundJumpReady".into(),
            _ => {}
        }
        s.state_before = a.state.clone();
    }
    // The prosthetic press (BEH_A_*_SUB_ATTACK: the tool's opener, its combos, the guard and leap
    // starts, the failed swing without emblems).
    if !landing && a.hp > 0.0 && events.iter().any(|e| sub_press_event(e)) {
        take_press(s, d, a, p, &mut pad, Action::Prosthetic, false, dt);
    }
    // Attack let go in ref 208 (BEH_A_GROUND_ATTACK_RELEASE): the attack's Release.
    // The art let go (BEH_A_GROUND_SP_ATTACK_RELEASE / _GUARD_RELEASE): its Release, the hold's
    // draw (HoldAction, or Combo1 out of the hold) or its end.
    let art_release = |e: &String| {
        let n = e.as_str();
        (["W_GroundSpecialAttack", "W_GroundSpacialAttack", "W_SprintSpecialAttack"].iter().any(|k| n.starts_with(k)) && (n.contains("Release") || n.contains("HoldEnd") || n.contains("HoldAction")))
            || (a.state.contains("SpacialAttackHold") || a.state == "SprintSpecialAttackHoldStart") && n.starts_with("W_GroundSpecialAttackCombo1")
    };
    if !landing && a.hp > 0.0 && events.iter().any(art_release) && take_graph(s, d, a, dt) {
        s.state_before = a.state.clone();
        s.anim_before = a.anim.clone();
    }
    // The guard let go in ref 207 (BEH_A_DEFLECT_GUARD_END): standing, or over the walk / run.
    if !landing && a.hp > 0.0 && events.iter().any(|e| e.starts_with("W_DeflectGuardToStand")) && take_graph(s, d, a, dt) {
        s.state_before = a.state.clone();
        s.anim_before = a.anim.clone();
    }
    // The sprint (BEH_A_SPRINT and its stop / quick turn, held step button): the step's ref 15
    // into SprintStartFromStep, let go into SprintStopReady, the stick turned back into its
    // quick turn.
    if !landing && a.hp > 0.0 && events.iter().any(|e| e == "W_SprintStartFromStep" || e == "W_SprintStopReady" || e.starts_with("W_SprintQuickTurn") || e.starts_with("W_SprintStopQuickTurn") || e == "W_StandMoveStartFromSprint") && take_graph(s, d, a, dt) {
        s.state_before = a.state.clone();
        s.anim_before = a.anim.clone();
    }
    // The prosthetic let go in ref 302 (BEH_A_*_SUB_ATTACK_RELEASE): the tool's Release.
    let sub_release = |e: &String| e.contains("SubAttack") && e.contains("Release") && !e.starts_with("W_Land") && !e.contains("Move");
    if !landing && a.hp > 0.0 && (events.iter().any(sub_release) || events.iter().any(|e| e.ends_with("Release") && ground_attack_event(&e.replace("Release", "")))) && take_graph(s, d, a, dt) {
        s.state_before = a.state.clone();
        s.anim_before = a.anim.clone();
    }
    // The fall start (BEH_R_FALL on env 200: W_SelfFallStart, crouched or out of the umbrella's
    // guard its own), then the graph's FreeFall at its clip end (graph_chains).
    if !landing && a.hp > 0.0 && a.airborne && events.iter().any(|e| matches!(e.as_str(), "W_SelfFallStart" | "W_SelfCrouchFreeFallStart" | "W_SubGuardEndSelfFallStart")) && take_graph(s, d, a, dt) {
        s.state_before = a.state.clone();
        s.anim_before = a.anim.clone();
    }
    // The step (_fireGroundStep -> W_GroundStep): its tilt is the script's act(3025, angle off
    // the quadrant's centre); unlocked, Wolf faces the stick (act 2019).
    if !landing && a.hp > 0.0 && events.iter().any(|e| e == "W_GroundStep") && take_press(s, d, a, p, &mut pad, Action::Step, false, dt) {
        step_aim(s, a, p, lock.target.is_none(), stick);
    }
    if a.airborne {
        s.air_pose = (a.state.clone(), a.anim.clone(), a.t);
    }
}

/// A step the script fired: its tilt is act(3025, angle off the quadrant's centre); unlocked,
/// Wolf faces the stick (act 2019).
fn step_aim(s: &WolfScript, a: &mut Actor, p: &mut Player, unlocked: bool, stick: Vec3) {
    a.move_vel = Vec3::ZERO;
    let tilt = s.chr.vm.host.borrow().act_values.get(&3025).copied().unwrap_or(0.0) as f32;
    p.step_tilt = (-tilt).to_radians();
    if unlocked && stick != Vec3::ZERO {
        a.yaw = crate::player::yaw_of(stick);
    }
}

/// The quick item (BEH_A_ITEM, HKS 3940-4033: the drink, its repeat and empty shakes, another
/// goods' use state by goodsUseAnim, the crouched ones): Wolf takes the graph's. Standing and
/// moving the drink is W_ItemGourdDrinkMove / RepeatMove, an upper action with no clip of its
/// own: the standing drink's clip over player.rs's legs. A goods' use state is `item_use` (its
/// TAE 65 spends it, player.rs).
fn take_item(s: &mut WolfScript, d: &crate::data::CharData, a: &mut Actor, p: &mut Player, pad: &mut crate::player::PadInput, config: &crate::config::GameConfig, events: &[String], dt: f32) {
    // Walking off in a standing drink or tool (BEH_A_GROUND_MOVE_START with the clip's
    // ITEM_USE_MOVE ref: W_StandMoveLowerStart): the graph's legs start under the same upper
    // action, its Move state (ItemGourdDrink -> ItemGourdDrinkMove, GroundSubAttackCombo1Moveable
    // -> GroundSubAttackCombo1Move), the clip going on (StartTime_01 = env 3063).
    // (The guard's run-out is player.rs's DeflectGuardToStandMove<speed>.)
    if events.iter().any(|e| e == "W_StandMoveLowerStart") && !crate::player::is_upper_action(&a.state) && !a.state.starts_with("DeflectGuard") {
        let base = a.state.strip_suffix("Moveable").unwrap_or(&a.state);
        if let Some(st) = [format!("{base}Move"), format!("{}Move", a.state)].into_iter().find(|c| crate::player::is_upper_action(c)) {
            a.state = st;
            s.state_before = a.state.clone();
            return;
        }
    }
    // And stopping in a walking one (BEH_A_GROUND_MOVE_STOP, HKS 2425: W_StandMoveableAction):
    // the upper action goes on standing (its *Moveable, the items' own state), the legs stop.
    if events.iter().any(|e| e == "W_StandMoveableAction") && crate::player::is_upper_action(&a.state) {
        if let Some(base) = a.state.strip_suffix("Move") {
            if let Some(st) = [format!("{base}Moveable"), base.to_string()].into_iter().find(|c| d.anim_key(c).is_some()) {
                a.state = st;
                a.move_vel = Vec3::ZERO;
                p.speed_level = 0.0;
                s.state_before = a.state.clone();
                return;
            }
        }
    }
    let Some(event) = events.iter().find(|e| e.starts_with("W_Item") || e.starts_with("W_CrouchItem")) else { return };
    if a.airborne || p.swim.is_some() {
        return;
    }
    if let Some(base) = event.strip_prefix("W_").and_then(|e| e.strip_suffix("Move")).filter(|b| b.starts_with("ItemGourdDrink")) {
        let Some(k) = d.anim_key(base).map(str::to_string) else { return };
        if !p.requests.contains_key(&Action::UseItem) && !pad.pressed().contains(&Action::UseItem) {
            return;
        }
        p.requests.remove(&Action::UseItem);
        pad.take(Action::UseItem);
        a.play(&format!("{base}Move"), &k);
        s.state_before = a.state.clone();
        s.anim_before = a.anim.clone();
        return;
    }
    if !take_press(s, d, a, p, pad, Action::UseItem, false, dt) {
        return;
    }
    a.move_vel = Vec3::ZERO;
    // A next-item press still pending is the exe's switch at once (as `answers`).
    let next = p.requests.contains_key(&Action::NextItem) || pad.pressed().contains(&Action::NextItem);
    let goods = crate::items::in_slot(config, if next { crate::items::next_slot(config, p.quick_slot) } else { p.quick_slot });
    let used = crate::items::use_state(crate::items::param("EquipParamGoods", goods)["goodsUseAnim"].as_i64().unwrap_or(255));
    if goods != crate::player::GOURD_GOODS && goods > 0 && crate::items::count(config, goods) > 0 && used == Some(a.state.trim_start_matches("Crouch")) {
        p.item_use = Some((goods, a.state.trim_start_matches("Crouch").to_string()));
    }
}

/// The attack button out of a tool move (BEH_A_GROUND_ATTACK's tool derives: Fang and Blade's
/// follow-ups, the Loaded Axe's / Spear's combos, the Mist Raven's landing slash).
fn sub_derive_event(e: &str) -> bool {
    (e.starts_with("W_Ground") && e.contains("SubAttack") && e.contains("Derive")) || e == "W_LandAirSubAttackMoveDeriveAttack"
}

/// A press the script took (its event fired this tick): Wolf takes the graph's state and clip
/// before player.rs decides, and the press is spent (player.rs's pick for it is gone).
/// `held`: the button held starts it with no press (the guard's ref 221).
#[allow(clippy::too_many_arguments)]
/// The plain attack's events (BEH_A_GROUND_ATTACK), not their releases.
fn ground_attack_event(e: &str) -> bool {
    let Some(n) = e.strip_prefix("W_") else { return false };
    !n.ends_with("Release")
        && (n.starts_with("GroundAttackCombo") || n.starts_with("CrouchAttackTo") || matches!(n, "GroundStepAttack" | "SprintAttack" | "HardDeflectAtk" | "EasyDeflectedAtk" | "DeflectGuardAttack"))
}

/// The art's press events (BEH_A_GROUND_SP_ATTACK, the sprint and step openers, the leap, the
/// hold's start, the combos and finishers), not the releases, the hold's own states or the hit jump.
fn art_press_event(e: &str) -> bool {
    let Some(n) = e.strip_prefix("W_") else { return false };
    ["GroundSpecialAttack", "GroundSpacialAttack", "SprintSpecialAttack"].iter().any(|k| n.starts_with(k))
        && !["Release", "HoldEnd", "HoldLoop", "HoldAction", "HoldMove", "HoldQuickTurn", "HitJump"].iter().any(|k| n.contains(k))
}

/// The prosthetic press's events, not the releases, the attack button's derives, the walking
/// upper-body ones (player.rs plays them over the walk), the loops and ends, the recoils or the
/// additives.
fn sub_press_event(e: &str) -> bool {
    let Some(n) = e.strip_prefix("W_") else { return false };
    (n.contains("SubAttack") && !["Add", "Land", "Hang", "Cover"].iter().any(|k| n.starts_with(k))
        && !["Release", "DeriveAttack", "DeriveDirectivity", "Move", "Loop", "End", "Deflect", "Action"].iter().any(|k| n.contains(k)))
        || matches!(n, "AirSubAttackMoveReady" | "AirSubAttackMoveAtemiReady")
}

fn take_press(s: &mut WolfScript, d: &crate::data::CharData, a: &mut Actor, p: &mut Player, pad: &mut crate::player::PadInput, act: Action, held: bool, dt: f32) -> bool {
    if !held && !p.requests.contains_key(&act) && !pad.pressed().contains(&act) {
        return false;
    }
    let Some(clip) = s.chr.rt.main_clip() else { return false };
    let (theirs, t) = (clip.animation.clone(), clip.time);
    if d.anim(&theirs).is_none() {
        return false;
    }
    p.requests.remove(&act);
    pad.take(act);
    let state = s.graph_state(d);
    if s.trace > 0 {
        eprintln!("hks: press {act:?}: {} -> {state} {theirs} at {t:.2}", a.state);
    }
    // Advance adds this tick's dt, which the graph's clip has already had.
    a.play(&state, &theirs);
    a.t = (t - dt).max(0.0);
    a.prev_t = a.t;
    s.state_before = a.state.clone();
    s.anim_before = a.anim.clone();
    true
}

/// The crouch press is the script's (BEH_A_CROUCH_START / BEH_A_CROUCH_END: W_CrouchStart /
/// W_CrouchEnd standing, W_CrouchMoveLoop / W_StandMoveLoop moving, W_SprintToCrouchReady with
/// unlock 26): where it took it, Wolf crouches or stands as the graph does before player.rs
/// decides. Moving, player.rs's Locomotion plays the crouch's move clips (a.crouch).
fn take_crouch(s: &mut WolfScript, d: &crate::data::CharData, a: &mut Actor, p: &mut Player, pad: &mut crate::player::PadInput, dt: f32) -> bool {
    if !p.requests.contains_key(&Action::Crouch) && !pad.pressed().contains(&Action::Crouch) {
        return false;
    }
    let graph = s.graph_state(d);
    let crouch = graph.starts_with("Crouch") || graph.starts_with("SprintToCrouch");
    if crouch == a.crouch || !(crouch || graph.starts_with("Stand")) {
        return false;
    }
    a.crouch = crouch;
    p.requests.remove(&Action::Crouch);
    pad.take(Action::Crouch);
    if !graph_free(&graph) {
        take_graph(s, d, a, dt);
        a.move_vel = Vec3::ZERO;
    } else if a.state != "Locomotion" && !crate::player::is_free(&a.state) {
        a.procedural("StandIdle");
    }
    true
}

fn apply_script(mut script: NonSendMut<WolfScript>, time: Res<Time>, combat: Res<Combat>, mut wolf: Single<&mut Actor, With<Player>>) {
    let d = &combat.player;
    let s = &mut *script;
    // A state player.rs started is the engine's where the graph takes it as the exe's own events:
    // those win over what the script fired this frame (the exe's throw check takes the press
    // first). Any other the graph does not show yet goes in as W_<state>.
    if wolf.state != s.state_before || wolf.anim != s.anim_before {
        if let (Ok(path), Some(at)) = (std::env::var("SHINOBI_HKS_AUDIT"), wolf.played_at) {
            let theirs = s.chr.rt.main_clip().map(|c| c.animation.clone()).unwrap_or_default();
            let ours = wolf.shown_clip(d).0;
            let same = same_clip(&theirs, &ours);
            let line = format!("{}:{}	{}	{}	{}	{}	{}	{}	{}
", at.file(), at.line(), wolf.state, ours, s.graph_state(d), theirs, same, s.shows(d, &wolf), std::thread::current().name().unwrap_or("-"));
            use std::io::Write;
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
                let _ = f.write_all(line.as_bytes());
            }
        }
    }
    if (wolf.state != s.state_before || wolf.anim != s.anim_before) && !crate::player::is_free(&wolf.state) {
        let own = !s.shows(d, &wolf) && wolf.state != s.state_before;
        s.engine_sync(d, &wolf.state, &wolf.anim, own);
    }
    if wolf.state != s.state_before && crate::player::is_free(&wolf.state) && !graph_free(&s.graph_state(d)) && !s.shows(d, &wolf) {
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
    if both_free && !same_clip(&theirs, &ours) {
        if let Ok(path) = std::env::var("SHINOBI_HKS_AUDIT") {
            let line = format!("free	{}	{}	{}	{}	false	false	{}
", wolf.state, ours, state, theirs, std::thread::current().name().unwrap_or("-"));
            use std::io::Write;
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
                let _ = f.write_all(line.as_bytes());
            }
        }
    }
    // The graph starts a move with another clip than player.rs's Locomotion (out of a landing or a
    // fall, or where player.rs went straight into the loop): Wolf moves on as player.rs has him
    // and shows the graph's start on the graph's clock.
    if both_free && wolf.state == "Locomotion" && graph_move_start(&state) && !held && !same_clip(&theirs, &ours) && d.anim(&theirs).is_some() {
        if s.trace > 0 {
            eprintln!("hks: Locomotion {ours} -> start {state} {theirs} at {t:.2}");
        }
        wolf.move_start_clip = Some(theirs.clone());
        wolf.move_start = d.length(&theirs);
        wolf.t = (t - dt).max(0.0);
        wolf.prev_t = wolf.t;
        s.overrides += 1;
        return;
    }
    // The graph goes straight into the loop (out of an action, W_StandMoveLoop) where player.rs
    // plays a move start first: Locomotion drops the start, on the graph's clock.
    if both_free && wolf.state == "Locomotion" && wolf.t < wolf.move_start && matches!(state.as_str(), "StandMoveLoop" | "CrouchMoveLoop") && !same_clip(&theirs, &ours) {
        if s.trace > 0 {
            eprintln!("hks: Locomotion {ours} -> loop {state} {theirs} at {t:.2}");
        }
        wolf.move_start = 0.0;
        wolf.move_start_clip = None;
        wolf.t = (t - dt).max(0.0);
        wolf.prev_t = wolf.t;
        s.overrides += 1;
        return;
    }
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
///   W_SprintToCrouchLeft / Right);
/// - the crouch start into the crouch idle;
/// - the hold art's start into its loop while held (BEH_A_GROUND_SP_ATTACK_HOLD);
/// - the knockdowns: a posture break's damage and blows' landings into StandDamageBreakDown and
///   its get-up, other blows' landings into StandDamageLargeDown, the side by
///   Selector_ProneDirection (set where the script picks the reaction).
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
        || state == "CrouchStart"
        || matches!(state, "GroundSpacialAttackHoldStart" | "GroundSpacialAttackHoldLoop" | "SprintSpecialAttackHoldStart")
        || state.starts_with("SprintStartFromStep")
        || state == "SprintLoop"
        || matches!(state, "SelfFallStart" | "SelfCrouchFreeFallStart" | "SubGuardEndSelfFallStart")
        || (state.contains("SubAttack") && !state.starts_with("Land") && !state.starts_with("Add"))
        || state == "SprintQuickTurnReady"
        || state == "SprintStopReady"
        || (state.starts_with("SprintQuickTurn") && state != "SprintQuickTurnReady")
        || knockdown(state)
}

/// The states the graph takes into a knockdown at their clip end.
fn knockdown(state: &str) -> bool {
    let landing = state.contains("Land") || state.contains("SmallBlow");
    match state.strip_prefix("StandDamage") {
        Some(rest) if rest.starts_with("Break") => matches!(rest, "BreakDamage" | "BreakLargePound") || rest.starts_with("BreakDown_") || landing,
        Some(rest) => rest == "LargePound" || rest.contains("BlowLand") || rest.contains("UpperLand") || rest.contains("SmallBlow"),
        None => (state.starts_with("AirDamage") && state.contains("Land")) || state == "ThrowDefLargeBlowLand600380",
    }
}

/// The same clip, or its copy in another anim set (a100_ vs a000_).
fn same_clip(a: &str, b: &str) -> bool {
    a == b || (a.len() > 4 && b.len() > 4 && a.starts_with('a') && b.starts_with('a') && a[4..] == b[4..])
}

/// The graph's move starts player.rs shows inside Locomotion (not the quick-turn starts: their
/// clips turn Wolf, which player.rs does by its own quick-turn states).
fn graph_move_start(state: &str) -> bool {
    state.starts_with("StandMoveStart") || state.starts_with("CrouchMoveStart")
}

/// player.rs's guard idle and its walk.
fn in_guard_walk(state: &str) -> bool {
    state == "DeflectGuardIdle" || state.starts_with("DeflectGuardMove")
}

/// A graph state with Wolf standing or moving, no action (the c0000 graph's idle, move, stop and
/// turn states).
fn graph_free(state: &str) -> bool {
    let Some(rest) = ["Stand", "Crouch", "DeflectGuard"].iter().find_map(|p| state.strip_prefix(p)) else { return false };
    rest == "Idle"
        || ["MoveStart", "MoveLoop", "MoveLower", "MoveQuickTurn", "QuickTurn", "RunQuickTurn", "WalkQuickTurn", "RunStop", "WalkStop"].iter().any(|p| rest.starts_with(p))
        || rest == "Move"
        || (state.starts_with("DeflectGuard") && rest.starts_with("Move"))
}
