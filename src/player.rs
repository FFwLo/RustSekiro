//! The player (Wolf): input, and the action state machine ported from
//! Sekiro's c0000.hks. The HKS decides *what* to play; the TAE of the playing
//! animation decides *when* each input is accepted and which variant runs:
//!
//! - ChrActionFlag (TAE 0) cancel windows gate inputs: 115 attack, 117 guard,
//!   26/25 step, 119 jump, 11 move (the engine's env(1106) checks these).
//! - SpEffects (TAE 67) carry behaviour refs the HKS reads via env(3036, ref):
//!   203 deflect window, 208 release-attack, 212 guard chain, 214-218 combo
//!   routing, 221 hold-guard cancel, 207 guard can end, 15 step -> sprint.

use bevy::prelude::*;
use std::collections::HashMap;

use crate::actor::{Actor, ActorSet, Side};
use crate::camera::{LockOn, OrbitCamera};
use crate::config::GameConfig;
use crate::data::{CharData, Combat, FALL_TYPES, calc_correct};
use crate::enemy::Enemy;
use crate::hud::CombatLog;

#[derive(Component)]
pub struct Player {
    /// Plunge deathblow in flight: the posture-broken enemy Wolf is falling onto.
    plunge: Option<Entity>,
    /// HKS MoveSpeedLevel: 0 idle, 1 walk, 2 run (converges at 3/s).
    speed_level: f32,
    requests: HashMap<Action, f32>,
    /// State after the last decide; a different state at the next decide was set by combat.
    last_state: String,
    /// Healing Gourd uses left (config player.gourd_charges; goods 3000 maxNum 10).
    pub gourd: u32,
    /// Resurrection nodes left (config player.resurrections).
    pub resurrections: u32,
    /// Attack auto-homing active (HKS g_autoAimFlag) and just started this decide.
    auto_aim: bool,
    auto_aim_fresh: bool,
    /// Spirit Emblems (prosthetic ammunition; config player.spirit_emblems).
    pub emblems: u32,
    /// Last ground jump was forward (HKS Selector_GroundJumpType): picks its land anim.
    jump_forward: bool,
    /// Landing of a directional (positioning) jump, which takes priority over moving on.
    jump_land: Option<&'static str>,
    /// Locked-on step tilt (HKS _set4DirStepTilt -> act 3025): root-motion yaw offset for the step.
    step_tilt: f32,
    /// Deathblow start throw in progress: the enemy and the main ThrowParam row suffix.
    throw_start: Option<(Entity, i64)>,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Action {
    Attack,
    Guard,
    Step,
    Jump,
    UseItem,
    /// Combat art (attack + guard together): Whirlwind Slash.
    CombatArt,
    /// Shinobi Prosthetic (Sekiro's R2): Loaded Shuriken.
    Prosthetic,
}

const CAPSULE_RADIUS: f32 = 0.4;
const CAPSULE_MIDDLE: f32 = 1.0;
pub const CAPSULE_HALF_HEIGHT: f32 = CAPSULE_RADIUS + CAPSULE_MIDDLE / 2.0;

// ChrActionFlag types (TAE event 0) that open input windows.
pub const FLAG_ACCEPT_ATTACK: i64 = 115;
pub const FLAG_ACCEPT_GUARD: i64 = 117;
pub const FLAG_ACCEPT_STEP: i64 = 26;
pub const FLAG_ACCEPT_STEP_ALT: i64 = 25;
pub const FLAG_ACCEPT_JUMP: i64 = 119;
/// ChrActionFlag 31 "AnimCancelEnd_UseItem" / 90 "Limit Move Speed To Walk".
const FLAG_ACCEPT_ITEM: i64 = 31;
/// ChrActionFlag 137 "AnimCancelEnd_R2_Prosthetic".
const FLAG_ACCEPT_PROSTHETIC: i64 = 137;
const FLAG_LIMIT_WALK: i64 = 90;
/// SpEffect 3000 (goods 3000, the Healing Gourd): changeHpEstusFlaskRate -40 -> 40 % HP.
const GOURD_SPEFFECT: &str = "3000";
/// Whirlwind Slash: weapon 5100 (spAtkcategory 100 -> anim group a100, behavior
/// variation 5001 -> judges 200/201, dark damage 125 %), GroundSpecialAttackCombo1 = 316000.
const ART_WHIRLWIND: &str = "a100_316000";

/// The equipped combat art's first anim: EquipParamWeapon[config player.combat_art]
/// spAtkcategory -> group a<cat>, GroundSpecialAttackCombo1 = <group>_316000 (or 316001).
fn art_anim(d: &CharData, combat: &Combat, config: &GameConfig) -> Option<String> {
    let cat = combat.param("EquipParamWeapon", config.player.combat_art)["spAtkcategory"].as_i64()?;
    [316000, 316001].iter().map(|id| format!("a{cat:03}_{id}")).find(|k| d.anim(k).is_some_and(|a| a.duration.is_some()))
}
/// HKS _FireSpAttackCombo: the art state that follows `state` when the art is pressed again
/// inside SP_EF_REF_TAE_ENABLE_SP_ATK_COMBO (223). `cat` = spAtkcategory (SP_ATK_TYPE_1xx);
/// `unlocked(ref)` = the SP_EF_REF_WEP_SP_ATK_UNLOCK_* ref is up (the upgraded art's resident
/// SpEffect, e.g. Ichimonji: Double 7100 -> 140201 -> ref 281).
fn art_combo_next(state: &str, cat: i64, unlocked: impl Fn(i64) -> bool) -> Option<&'static str> {
    const UNLOCK_102_COMBO: i64 = 281;
    const UNLOCK_105_COMBO: i64 = 282;
    const UNLOCK_107_COMBO: i64 = 284;
    const UNLOCK_108_FINISH: i64 = 285;
    match state {
        "GroundSpecialAttackCombo1" | "GroundSpecialAttackCombo1Release" | "SprintSpecialAttack" | "SprintSpecialAttackRelease" => match cat {
            102 if !unlocked(UNLOCK_102_COMBO) => None,
            107 if !unlocked(UNLOCK_107_COMBO) => Some("GroundSpecialAttackCombo2Finish"),
            108 if !unlocked(UNLOCK_108_FINISH) => Some("GroundSpecialAttackCombo2Finish"),
            _ => Some("GroundSpecialAttackCombo2"),
        },
        "GroundSpecialAttackCombo2" => match cat {
            105 if !unlocked(UNLOCK_105_COMBO) => Some("GroundSpacialAttackVariationCombo3"),
            107 if !unlocked(UNLOCK_107_COMBO) => None,
            108 if !unlocked(UNLOCK_108_FINISH) => None,
            _ => Some("GroundSpecialAttackCombo3"),
        },
        "GroundSpecialAttackCombo3" => Some("GroundSpecialAttackCombo4"),
        "GroundSpecialAttackCombo4" => Some("GroundSpecialAttackCombo5"),
        _ => None,
    }
}

/// HKS BEH_A_GROUND_SP_ATTACK: the art's opening state by type. Out of a sprint (ref 1) the
/// Sprint* variant; 101 steps (_set2DirStepDir: stick within +-90 deg of the facing -> F, no
/// stick -> N, else B; without SP_EF_REF_WEP_SP_ATK_UNLOCK_101_BACK_ATTACK (280) always F);
/// otherwise GroundSpecialAttackCombo1. Candidates in order; the first whose anim exists in the
/// art's group wins; 104 opens its hold (GroundSpacialAttackHoldStart). gap: 107 / 110 jump starts.
fn art_start_states(cat: i64, unlock: i64, sprint: bool, stick: Vec3, fwd: Vec3) -> Vec<String> {
    let mut out = Vec::new();
    let pre = if sprint { "Sprint" } else { "Ground" };
    if cat == 101 {
        let dir = if unlock != 280 || (stick != Vec3::ZERO && stick.dot(fwd) > 0.0) {
            "F"
        } else if stick == Vec3::ZERO {
            "N"
        } else {
            "B"
        };
        out.push(format!("{pre}SpecialAttackStep_{dir}"));
    }
    if cat == 104 {
        out.push(if sprint { "SprintSpecialAttackHoldStart" } else { "GroundSpacialAttackHoldStart" }.to_string());
    }
    if sprint && cat != 104 {
        out.push("SprintSpecialAttack".to_string());
    }
    out.push("GroundSpecialAttackCombo1".to_string());
    out
}

/// The equipped art's spAtkcategory and the unlock ref of its resident SpEffect (0 = none).
fn art_kind(combat: &Combat, config: &GameConfig) -> Option<(i64, i64)> {
    let row = combat.param("EquipParamWeapon", config.player.combat_art);
    let cat = row["spAtkcategory"].as_i64().filter(|&c| c > 0)?;
    let unlock = ["residentSpEffectId", "residentSpEffectId1", "residentSpEffectId2"]
        .iter()
        .filter_map(|f| row[*f].as_i64().filter(|&id| id > 0))
        .map(|id| combat.param("SpEffectParam", id)["behaviorRefId"].as_i64().unwrap_or(0))
        .find(|&r| r > 0)
        .unwrap_or(0);
    Some((cat, unlock))
}

/// ChrActionFlag 27 "SetNoGravity".
const FLAG_NO_GRAVITY: i64 = 27;
pub const FLAG_ACCEPT_MOVE: i64 = 11;
pub const FLAG_SHIELD_BLOCK: i64 = 3;
pub const FLAG_DISABLE_TURN: i64 = 7;

// SP_EF_REF_* behaviour refs (c0000_define.hks).
pub const REF_ENABLE_SPRINT_ACTION: i64 = 1;
pub const REF_TRANSITION_SPRINT: i64 = 15;
pub const REF_JUST_GUARD: i64 = 203;
pub const REF_GUARD_CAN_END: i64 = 207;
pub const REF_RELEASE_ATTACK: i64 = 208;
pub const REF_GUARD_COMBO: i64 = 212;
/// In the HKS check order. 231 (COMBO_1_REVERSE) is up after deflects, left swings and
/// DeflectGuardToStand: the next slash then starts from the other side.
pub const REF_COMBO: [(i64, &str); 7] = [
    (214, "GroundAttackCombo1"),
    (215, "GroundAttackCombo2"),
    (231, "GroundAttackCombo1Reverse"),
    (219, "GroundAttackCombo2Reverse"),
    (216, "GroundAttackCombo3"),
    (217, "GroundAttackCombo4"),
    (218, "GroundAttackCombo5"),
];
pub const REF_PRESS_GUARD: i64 = 221;
/// SP_EF_REF_TAE_ENABLE_SP_ATK_RELEASE: letting go of attack here plays the art's Release
/// (a106_316010 f6-18; live recording: let go at f2-3 -> a106_316110 at f6.5).
pub const REF_SP_ATK_RELEASE: i64 = 222;
/// SP_EF_REF_TAE_ENABLE_SP_ATK_COMBO: the art's follow-up window (SpEffect 100252).
pub const REF_ENABLE_SP_ATK_COMBO: i64 = 223;
/// SP_EF_REF_TAE_ENABLE_ADD_JUST_DEFLECT / _ADD_ACTION_INPUT_GUARD_CANCEL / _HIT_DEFLECT_CANCEL.
pub const REF_ADD_JUST_DEFLECT: i64 = 228;
pub const REF_ADD_GUARD_CANCEL: i64 = 412;
/// SP_EF_REF_TAE_ENABLE_ADD_ACTION_INPUT_GUARD: raised by the additive deflect (a000_299060 f0-9).
pub const REF_ADD_INPUT_GUARD: i64 = 411;
pub const REF_HIT_DEFLECT_CANCEL: i64 = 503;
/// SP_EF_REF_TAE_ENABLE_ORIGINAL_LAND_ACTION: landing keeps the attack going (LandAir* anims).
pub const REF_ORIGINAL_LAND_ACTION: i64 = 201;
/// SP_EF_REF_TAE_ENABLE_DEFLECT_GUARD_ATTACK / _TRANSITION_STEP_ATTACK.
pub const REF_DEFLECT_GUARD_ATTACK: i64 = 213;
pub const REF_STEP_ATTACK: i64 = 224;
pub const REF_GUARD_REVERSE: i64 = 227;

/// Raw input sampled every frame; presses are latched until FixedUpdate reads them.
#[derive(Resource, Default)]
pub struct PadInput {
    pub stick: Vec2,
    pub walk: bool,
    pub attack_held: bool,
    pub guard_held: bool,
    pub dodge_held: bool,
    pub prosthetic_held: bool,
    pressed: Vec<Action>,
    pub reset: bool,
    /// Sheathe / draw the sword (X).
    pub sheathe: bool,
}

impl PadInput {
    /// Queue a button press (used by input reading and by the simulation tests).
    pub fn press(&mut self, a: Action) {
        self.pressed.push(a);
    }
}

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PadInput>()
            .init_resource::<LockOn>()
            .add_systems(Startup, spawn_player)
            .add_systems(Update, read_input.run_if(resource_exists::<ButtonInput<KeyCode>>))
            .init_resource::<ActiveThrow>()
            .add_systems(FixedUpdate, (sync_art_variation, decide, record_state).chain().in_set(ActorSet::Decide))
            .add_systems(FixedUpdate, follow_throw.after(ActorSet::Advance).before(ActorSet::Resolve));
    }
}

/// A deathblow in progress: ThrowParam's dummy-poly absorb, applied by follow_throw.
#[derive(Resource, Default)]
pub struct ActiveThrow(pub Option<ThrowHold>);

#[derive(Clone, Copy, Debug)]
pub struct ThrowHold {
    pub target: Entity,
    pub atk_dmy: i16,
    pub def_dmy: i16,
    /// Wolf's state while the throw lasts ("Deathblow", "Mikiri").
    pub state: &'static str,
}

impl ThrowHold {
    pub fn new(target: Entity, th: &crate::data::Throw, state: &'static str) -> Self {
        ThrowHold { target, atk_dmy: th.atk_dmy, def_dmy: th.def_dmy, state }
    }
}

/// HKS _SetJumpDirection sectors (PRM_GROUND_JUMP_*_STICK_RANGE, degrees from the facing, + right):
/// (ready state, raw anim for the side jumps, landing state).
fn directional_jump(angle: f32) -> (&'static str, Option<&'static str>, &'static str) {
    match angle {
        a if a.abs() <= 18.75 => ("LockonForwardGroundJumpReady", None, "LandGroundPositioningJump_LockOn_F"),
        a if a > 18.75 && a < 67.5 => ("GroundJumpReady_F_R", None, "LandGroundPositioningJump_F_R"),
        a if (67.5..=112.5).contains(&a) => ("RightsideGroundJumpReady", Some("a000_201103"), "LandGroundPositioningJump_R"),
        a if a > 112.5 && a < 157.5 => ("GroundJumpReady_B_R", None, "LandGroundPositioningJump_B_R"),
        a if a < -18.75 && a > -67.5 => ("GroundJumpReady_F_L", None, "LandGroundPositioningJump_F_L"),
        a if (-112.5..=-67.5).contains(&a) => ("LeftsideGroundJumpReady", Some("a000_201102"), "LandGroundPositioningJump_L"),
        a if a < -112.5 && a > -157.5 => ("GroundJumpReady_B_L", None, "LandGroundPositioningJump_B_L"),
        _ => ("BackwardGroundJumpReady", None, "LandGroundPositioningJump_B"),
    }
}

/// The main deathblow throw: Wolf's ThrowParam anim, the enemy's ThrowDef(Death) anim, and the
/// absorb onto the throw dummy (follow_throw). Facing here is the fallback without models.
#[allow(clippy::too_many_arguments)]
fn main_deathblow(
    a: &mut Actor,
    pos: Vec3,
    ea: &mut Actor,
    enemy: &mut Enemy,
    epos: Vec3,
    ee: Entity,
    combat: &Combat,
    th: &crate::data::Throw,
    behind: bool,
    throw: &mut ActiveThrow,
    log: &mut CombatLog,
) {
    let to_enemy = (epos - pos).with_y(0.0).normalize_or_zero();
    a.yaw = yaw_of(to_enemy);
    if combat.player.anim(&th.atk_anim).is_some() {
        a.play("Deathblow", &th.atk_anim);
        a.move_vel = Vec3::ZERO;
    }
    // The defender faces Wolf, or away from him for the behind deathblow.
    ea.yaw = if behind { yaw_of(to_enemy) } else { yaw_of(-to_enemy) };
    enemy.deathblow(ea, combat, th.def_anim);
    throw.0 = Some(ThrowHold::new(ee, th, "Deathblow"));
    log.push("DEATHBLOW", Color::srgb(1.0, 0.2, 0.2));
}

/// Throw absorb (ThrowParam): when a paired throw starts, the defender's root is put on the
/// attacker's dummy poly atkSorbDmyId (front deathblow 266, deflect-break 246, Mikiri 523) or the
/// attacker on the defender's defSorbDmyId (behind 229, plunge landing 233), facing along the
/// dummy's forward. Live (rec_20261008_054259): the General snaps to exactly 1.50 m in front of
/// Wolf, facing him (180.0 deg), on the main throw's first frame; after that only the two anims'
/// root motion moves them (1.50 -> 0.85 -> 2.35 m), so it is applied once, not held.
#[allow(clippy::type_complexity)]
fn follow_throw(
    mut throw: ResMut<ActiveThrow>,
    mut player: Single<(&mut Actor, &mut Transform, Option<&crate::model::Dummies>, &GlobalTransform), (With<Player>, Without<Enemy>)>,
    mut enemies: Query<(&mut Actor, &mut Transform, Option<&crate::model::Dummies>, &GlobalTransform), (With<Enemy>, Without<Player>)>,
    globals: Query<&GlobalTransform, (Without<Player>, Without<Enemy>)>,
) {
    let Some(h) = throw.0.as_mut() else { return };
    let (pa, ptf, pdm, pg) = &mut *player;
    let Ok((mut ea, mut etf, edm, eg)) = enemies.get_mut(h.target) else {
        throw.0 = None;
        return;
    };
    if pa.state != h.state {
        throw.0 = None;
        return;
    }
    // The dummy's position and forward (+Z, model_<chr>.dummies.json) in its character's local
    // space (last drawn pose), to be placed on the simulated transform. Id 0 = the root.
    let local = |dm: Option<&crate::model::Dummies>, g: &GlobalTransform, id: i16| -> Option<(Vec3, Vec3)> {
        if id == 0 {
            return Some((Vec3::ZERO, Vec3::Z));
        }
        let dg = globals.get(*dm?.0.get(&id)?).ok()?;
        let inv = g.affine().inverse();
        Some((inv.transform_point3(dg.translation()), inv.transform_vector3(dg.rotation() * Vec3::Z)))
    };
    // The absorbed character moves onto the dummy and faces along its forward (front deathblow:
    // 266 faces back at Wolf; behind: 229 faces the enemy's way; plunge: 233 faces the enemy).
    let absorb = |tf: &mut Transform, a: &mut Actor, owner: &Transform, (l, f): (Vec3, Vec3)| {
        let goal = owner.transform_point(l);
        tf.translation += (goal - tf.translation).with_y(0.0);
        let dir = (owner.rotation * f).with_y(0.0);
        if dir.length_squared() > 1e-6 {
            let want = yaw_of(dir);
            let diff = (want - a.yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
            a.yaw += diff;
            tf.rotation = Quat::from_rotation_y(a.yaw);
        }
    };
    if h.def_dmy != 0 {
        let Some(l) = local(edm, eg, h.def_dmy) else {
            warn!("deathblow: enemy has no dummy poly {} (throw follow off)", h.def_dmy);
            throw.0 = None;
            return;
        };
        let owner = *etf;
        absorb(ptf, pa, &owner, l);
    } else {
        let Some(l) = local(*pdm, pg, h.atk_dmy) else {
            warn!("deathblow: Wolf has no dummy poly {} (throw follow off)", h.atk_dmy);
            throw.0 = None;
            return;
        };
        let owner = **ptf;
        absorb(&mut etf, &mut ea, &owner, l);
    }
    throw.0 = None;
}

/// Keeps CharData.art_variation on the equipped art (config player.combat_art, also after F5).
fn sync_art_variation(config: Res<GameConfig>, mut combat: ResMut<Combat>) {
    if !config.is_changed() && combat.player.art_variation.is_some() {
        return;
    }
    let var = combat.param("EquipParamWeapon", config.player.combat_art)["behaviorVariationId"].as_i64();
    if combat.player.art_variation != var {
        combat.player.art_variation = var;
    }
}

fn spawn_player(mut commands: Commands, combat: Res<Combat>, config: Res<GameConfig>) {
    // Graphs 500 / 501 / 504 = max HP / max posture / posture regen by level (exe: FUN_140a368f0
    // stores 500 -> PlayerGameData +0x20 and 501 -> +0x3c; 504 in FUN_140850b10's regen caller).
    let lvl = config.posture.player_level;
    let hp = calc_correct(combat.param("CalcCorrectGraph", 500), lvl);
    let posture = calc_correct(combat.param("CalcCorrectGraph", 501), lvl);
    let regen = calc_correct(combat.param("CalcCorrectGraph", 504), lvl);
    let mut actor = Actor::new(Side::Player, hp, posture, regen, 0);
    actor.yaw = std::f32::consts::PI;
    // Resident SpEffects of the default outfit (EquipParamProtector residentSpEffectId1-3):
    // 5221-5223 cut posture regen to x0.75 / x0.5 / x0.25 at <= 75 / 50 / 25 % HP.
    if let Some(rows) = combat.params.get("EquipParamProtector").and_then(|v| v.as_object()) {
        for r in rows.values() {
            for k in ["residentSpEffectId", "residentSpEffectId2", "residentSpEffectId3"] {
                if let Some(id) = r[k].as_i64().filter(|&v| v > 0) {
                    actor.resident.push(id);
                }
            }
        }
    }
    commands.spawn((
        Player { plunge: None, speed_level: 0.0, requests: HashMap::new(), last_state: String::new(), gourd: config.player.gourd_charges, emblems: config.player.spirit_emblems, auto_aim: false, auto_aim_fresh: false, resurrections: config.player.resurrections, jump_forward: false, jump_land: None, step_tilt: 0.0, throw_start: None },
        actor,
        Name::new("Player"),
        Transform::from_xyz(0.0, CAPSULE_HALF_HEIGHT, 4.0),
        Visibility::default(),
    ));
}

/// Capsule + blade for the player (windowed app only; the simulation tests skip it).
pub fn player_visuals(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    q: Query<Entity, (With<Player>, Without<Mesh3d>)>,
) {
    for e in &q {
        commands
            .entity(e)
            .insert((
                Mesh3d(meshes.add(Capsule3d::new(CAPSULE_RADIUS, CAPSULE_MIDDLE))),
                MeshMaterial3d(materials.add(Color::srgb(0.85, 0.8, 0.72))),
            ))
            .with_children(|p| crate::world::spawn_blade(p, &mut meshes, &mut materials, Color::srgb(0.8, 0.85, 0.9)));
    }
}

pub(crate) fn read_input(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    cursor: Single<&bevy::window::CursorOptions, With<bevy::window::PrimaryWindow>>,
    mut pad: ResMut<PadInput>,
) {
    let mut stick = Vec2::ZERO;
    if keys.pressed(KeyCode::KeyW) { stick.y += 1.0; }
    if keys.pressed(KeyCode::KeyS) { stick.y -= 1.0; }
    if keys.pressed(KeyCode::KeyD) { stick.x += 1.0; }
    if keys.pressed(KeyCode::KeyA) { stick.x -= 1.0; }
    pad.stick = stick.normalize_or_zero();
    pad.walk = keys.pressed(KeyCode::AltLeft) || keys.pressed(KeyCode::ControlLeft);
    let captured = cursor.grab_mode != bevy::window::CursorGrabMode::None;
    // Mouse buttons only count once the mouse is captured (the first click captures it).
    let lmb = captured && mouse.pressed(MouseButton::Left);
    let rmb = captured && mouse.pressed(MouseButton::Right);
    pad.attack_held = lmb || keys.pressed(KeyCode::KeyJ);
    pad.guard_held = rmb || keys.pressed(KeyCode::KeyK);
    pad.dodge_held = keys.pressed(KeyCode::ShiftLeft);
    if (captured && mouse.just_pressed(MouseButton::Left)) || keys.just_pressed(KeyCode::KeyJ) {
        pad.pressed.push(Action::Attack);
    }
    if (captured && mouse.just_pressed(MouseButton::Right)) || keys.just_pressed(KeyCode::KeyK) {
        pad.pressed.push(Action::Guard);
    }
    if keys.just_pressed(KeyCode::ShiftLeft) {
        pad.pressed.push(Action::Step);
    }
    if keys.just_pressed(KeyCode::KeyE) {
        pad.pressed.push(Action::UseItem);
    }
    if keys.just_pressed(KeyCode::KeyF) {
        pad.pressed.push(Action::Prosthetic);
    }
    pad.prosthetic_held = keys.pressed(KeyCode::KeyF);
    if keys.just_pressed(KeyCode::Space) {
        pad.pressed.push(Action::Jump);
    }
    if keys.just_pressed(KeyCode::KeyR) {
        pad.reset = true;
    }
    if keys.just_pressed(KeyCode::KeyX) {
        pad.sheathe = true;
    }
}

/// Standby states: any action may start (the run/walk stops carry no cancel flags but are
/// standby in the HKS, so input interrupts them).
fn is_free(state: &str) -> bool {
    matches!(state, "StandIdle" | "Locomotion") || state.starts_with("StandRunStop") || state.starts_with("StandWalkStop_") || is_guard_idle(state)
}

/// Guarding with no action: idle or guard walk (DeflectGuardMoveF/B/L/R).
fn is_guard_idle(state: &str) -> bool {
    state == "DeflectGuardIdle" || state.starts_with("DeflectGuardMove")
}

/// WalkTwist convergence: SpinJointBehavior constant-rate mode, WalkTwist +0x70 = 0.1 rad per
/// 1/30 s frame (exe FUN_1407f65a0 / FUN_14072fdb0).
const WALK_TWIST_RATE: f32 = 0.1 * 30.0;

/// HKS STATE_TYPE_UPPER_ACTION states Wolf uses (c0000.hkx StandMoveUpper_SM): drawn over the
/// walk/run by anim.rs, moved by lower_body_move.
pub(crate) fn is_upper_action(state: &str) -> bool {
    state.starts_with("DeflectGuardToStandMove")
}

fn is_air_guard(state: &str) -> bool {
    state.starts_with("AirDeflectGuard")
}

/// SpEffect behaviorRefIds the HKS reads with env(3036, ...) (c0000_define.lua SP_EF_REF_*).
const SP_REF_DISABLE_AIR_KICK: i64 = 108;
const SP_REF_KICK_ENEMY_JUMP: i64 = 204;

/// env(3036, ref): is an SpEffect with this behaviorRefId applied by the current anim's TAE?
fn sp_ref_active(d: &CharData, a: &Actor, behavior_ref: i64) -> bool {
    !a.anim.is_empty() && d.sp_effects_at(&a.anim, a.t).iter().any(|(_, s)| s.behavior_ref_id == behavior_ref)
}

/// Plunge deathblow start (ThrowParam 崩し落下0, suffix 150: Dist 20 m, defender within upperYRange
/// 3 m above / lowerYRange 15 m below, normalFallOrbitCheck: the unsteered fall passes within
/// range 1.5 m of the defender, at most heightLimit 3 m above it, within timeLimit 2000 ms).
/// Plays the throw's attacker anim (a20x_511400 = 510300's clip) and the defender's ThrowDef13400.
fn start_plunge(
    p: &mut Player,
    a: &mut Actor,
    combat: &Combat,
    pos: Vec3,
    enemies: &mut Query<(Entity, &mut Actor, &mut Enemy, &Transform), Without<Player>>,
    log: &mut CombatLog,
) -> bool {
    if a.vel_y >= 0.0 {
        return false;
    }
    let row_id = combat.foe.throw_row(150);
    let row = combat.param("ThrowParam", row_id);
    let Some(th) = combat.throw(row_id) else { return false };
    let f = |k: &str| row[k].as_f64().unwrap_or(0.0) as f32;
    let (upper, lower) = (f("upperYRange"), f("lowerYRange"));
    let (range, height, time_limit) = (f("normalFallOrbitCheck_range"), f("normalFallOrbitCheck_heightLimit"), f("normalFallOrbitCheck_timeLimit") / 1000.0);
    let ft = FALL_TYPES[(a.fall_type as usize).min(FALL_TYPES.len() - 1)];
    for (e, mut ea, mut enemy, etf) in enemies.iter_mut() {
        if !enemy.is_broken() || etf.translation.distance(pos) > th.dist {
            continue;
        }
        let dy = etf.translation.y - pos.y;
        if dy > upper || dy < -lower {
            continue;
        }
        // Normal fall orbit (no steering), 1/60 s steps.
        let (mut q, mut vy) = (pos, a.vel_y);
        let mut hit = false;
        for _ in 0..((time_limit * 60.0) as usize).max(1) {
            vy += ft.gravity() / 60.0;
            q += a.air_base / 60.0 + Vec3::Y * vy / 60.0;
            if (q - etf.translation).with_y(0.0).length() <= range && q.y - etf.translation.y <= height {
                hit = true;
                break;
            }
            if q.y < CAPSULE_HALF_HEIGHT {
                break;
            }
        }
        if !hit || combat.player.anim(&th.atk_anim).is_none() {
            continue;
        }
        a.play("PlungeDeathblow", &th.atk_anim);
        p.plunge = Some(e);
        enemy.react(&mut ea, combat, &format!("ThrowDef{}", th.def_anim));
        log.push("plunge", Color::srgb(1.0, 0.5, 0.3));
        return true;
    }
    false
}

/// HKS SPRINT_BRAKE_ANGLE and SP_EF_REF_TAE_ENABLE_SPRINT_QUICK_TURN (c0000_define.lua).
const SPRINT_BRAKE_ANGLE: f32 = 135.0;
const REF_ENABLE_SPRINT_QUICK_TURN: i64 = 2;

/// HKS _UpdateAutoAim: the homing target is refreshed for this long after an attack starts.
const AUTO_AIM_TIME: f32 = 1.0 / 6.0;

/// HKS _StartAutoAim on an attack transition: pick the homing target now (so the swing's
/// opening facing uses it) and keep refreshing it for AUTO_AIM_TIME.
fn start_auto_aim(
    p: &mut Player,
    a: &mut Actor,
    combat: &Combat,
    pos: Vec3,
    stick: Vec3,
    enemies: &Query<(Entity, &mut Actor, &mut Enemy, &Transform), Without<Player>>,
) {
    p.auto_aim = true;
    p.auto_aim_fresh = true;
    let dir = if stick != Vec3::ZERO { stick } else { a.forward() };
    a.homing = auto_homing_target(combat, pos, dir, enemies.iter().map(|(e, ea, en, t)| (e, t.translation, ea.hp > 0.0 && !en.is_dead())));
}

/// FUN_140b30fa0 / FUN_1409c58e0 / FUN_140b2ddc0: from the eye point (feet + 1.5 m), the
/// nearest live enemy with 3D distance <= closeMaxRadius, height within -closeMinHeight ..
/// +closeMaxHeight, and within closeAngRange of the stick (or facing). LockCamParam row 0.
/// gap: the darkness variants (_forD / _forPD) and the line-of-sight ray are not modelled.
fn auto_homing_target(combat: &Combat, pos: Vec3, dir: Vec3, enemies: impl Iterator<Item = (Entity, Vec3, bool)>) -> Option<Entity> {
    let row = combat.param("LockCamParam", 0);
    let f = |k: &str| row[k].as_f64().unwrap_or(0.0) as f32;
    let (max_r, min_h, max_h, ang) = (f("closeMaxRadius"), f("closeMinHeight"), f("closeMaxHeight"), f("closeAngRange"));
    let eye = pos - Vec3::Y * CAPSULE_HALF_HEIGHT + Vec3::Y * 1.5;
    let dir = dir.with_y(0.0).normalize_or_zero();
    enemies
        .filter(|&(_, _, alive)| alive)
        .filter_map(|(e, t, _)| {
            // Candidate point: the enemy's lock point at the same 1.5 m height.
            let c = t - Vec3::Y * CAPSULE_HALF_HEIGHT + Vec3::Y * 1.5;
            let v = c - eye;
            let d = v.length();
            let flat = v.with_y(0.0).normalize_or_zero();
            let ok = d <= max_r && v.y >= -min_h && v.y <= max_h && flat.angle_between(dir).to_degrees() <= ang;
            ok.then_some((e, d))
        })
        .min_by(|x, y| x.1.total_cmp(&y.1))
        .map(|(e, _)| e)
}

/// Remembers the state decide left the player in (see the ActionRequest buffer).
fn record_state(mut player: Single<(&mut Player, &mut Actor)>) {
    let (p, a) = &mut *player;
    a.attack_hit = false;
    if p.last_state != a.state {
        if !keeps_buffer(&a.state) {
            p.requests.clear();
        }
        p.last_state = a.state.clone();
        // The attack that started auto-homing keeps it; leaving it ends it (FUN_140b2d9d0 clears +0x38).
        if p.auto_aim_fresh {
            p.auto_aim_fresh = false;
        } else {
            p.auto_aim = false;
            a.homing = None;
        }
    }
}


/// HKS FireEvent(state) runs ResetRequest -> act(9101) -> FUN_140b2afd0: every pending
/// request is dropped on a state change. FireEventNoReset (locomotion, quick turns, falls
/// and landings) and graph-driven returns to idle keep them.
fn keeps_buffer(state: &str) -> bool {
    // The c0000_transition.lua FireEventNoReset targets Wolf can reach here.
    const NO_RESET: [&str; 12] = [
        "GroundAttackCombo1Release",
        "GroundAttackCombo1ReverseRelease",
        "GroundAttackCombo2Release",
        "GroundAttackCombo2ReverseRelease",
        "DeflectGuardAttackRelease",
        "GroundSpecialAttackCombo1Release",
        "StandDeflectEasyMinimum",
        "StandDeflectHardMinimum",
        "AirDeflectGuardEnd",
        "EasyDeflectedL",
        "EasyDeflectedR",
        "ThrowDefLargeBlowStart",
    ];
    is_free(state)
        || state.contains("Fall")
        || state.contains("Land")
        || state.starts_with("StandMove")
        || NO_RESET.contains(&state)
        || state.starts_with("HardDeflectAtkRelease")
        || state.starts_with("EasyDeflectedAtkRelease")
        || state.starts_with("DeflectGuardToStand")
        || state.starts_with("SprintQuickTurn") && state != "SprintQuickTurnReady"
        || state.starts_with("SprintStopQuickTurn")
        || state.starts_with("SprintStartFromStep")
}

/// ChrActionFlags that open an action's input buffer (exe: the TAE ChrActionFlag handler
/// FUN_140b51b70 sets accept bits +0xd0 via FUN_140b2be00). 87 AttackAction_Complex opens
/// every action; 1 attack/sub-attack (bits 0,1,25,26), 9 and 150 guard (2), 25 SpMove/
/// Backstep/Rolling (5,13,14), 151 jump (4), 30 item (7), 136 shinobi tool (18).
const FLAG_BUFFER_ALL: i64 = 87;

fn buffer_flags(action: Action) -> &'static [i64] {
    match action {
        Action::Attack | Action::CombatArt => &[1],
        Action::Guard => &[9, 150],
        Action::Step => &[25],
        Action::Jump => &[151],
        Action::UseItem => &[30],
        Action::Prosthetic => &[136],
    }
}

/// Is a press latched right now? (pending |= pressed & acceptMask in FUN_140b2c190.) A
/// press outside every accept window is dropped, not queued.
fn buffers(d: &CharData, a: &Actor, action: Action) -> bool {
    if a.airborne || is_free(&a.state) || a.anim.is_empty() || sprint_window(d, a) {
        return true;
    }
    d.flag(&a.anim, a.t, FLAG_BUFFER_ALL) || buffer_flags(action).iter().any(|&f| d.flag(&a.anim, a.t, f))
}

/// HKS ExecAttack routing (c0000_transition.lua): combo refs 214-219, step attack (ref
/// 224), sprint attack (ref 1), the counter out of a hard deflect (HardDeflectAtk) or
/// from being lightly deflected (EasyDeflectedAtk), the guard attack (ref 213), else
/// GroundAttackCombo1.
fn attack_route(d: &CharData, a: &Actor, stick: Vec3, art_equipped: bool) -> String {
    let has = |r: i64| !a.anim.is_empty() && d.has_ref(&a.anim, a.t, r);
    if let Some((_, s)) = REF_COMBO.iter().find(|(r, _)| has(*r)) {
        return s.to_string();
    }
    if has(REF_STEP_ATTACK) {
        let dir = a.state.rsplit('_').next().filter(|d| matches!(*d, "N" | "F" | "B" | "L" | "R")).unwrap_or("N");
        return format!("GroundStepAttack_{dir}");
    }
    if has(REF_ENABLE_SPRINT_ACTION) {
        // Selector_GroundJumpType 0 = SprintAttack_L, 1 = _R (c0000.hkx "SprintAttack Selector").
        // AttackAngle = signed stick angle from the facing (> 0 right): L when steering right
        // out of SprintLoop, or left / straight out of SprintStartFromStep_F.
        let angle_right = stick.dot(a.forward().cross(Vec3::Y)) > 0.0;
        let from_step = a.state == "SprintStartFromStep_F";
        let left = if from_step { !angle_right } else { angle_right };
        return format!("SprintAttack_{}", if left { "L" } else { "R" });
    }
    // Hard deflect reactions: StandDeflectHard{Minimum,Small,Middle}[_L/_R], HardDeflectDmg*.
    if a.state.starts_with("StandDeflectHard") || a.state.starts_with("HardDeflectDmg") || a.state.starts_with("LandAirDeflectHard") {
        let size = if a.state.contains("Middle") || a.state.contains("Large") { "Middle" } else { "Small" };
        let dir = if a.state.ends_with("_L") || a.state.ends_with('L') && a.state.starts_with("HardDeflectDmg") {
            "L"
        } else if a.state.ends_with("_R") || a.state.ends_with('R') && a.state.starts_with("HardDeflectDmg") {
            "R"
        } else {
            "F"
        };
        return format!("HardDeflectAtk{size}{dir}");
    }
    if let Some(side) = a.state.strip_prefix("EasyDeflected").filter(|s| *s == "L" || *s == "R") {
        return format!("EasyDeflectedAtk{side}");
    }
    // Only without a combat art equipped (env(345, HAND_RIGHT) == SP_ATK_TYPE_NONE).
    if has(REF_DEFLECT_GUARD_ATTACK) && !art_equipped {
        return "DeflectGuardAttack".into();
    }
    "GroundAttackCombo1".into()
}

/// The release follow-up of a held attack state (attack let go while ref 208 is up).
fn release_of(state: &str) -> Option<String> {
    Some(match state {
        "GroundAttackCombo1" | "GroundAttackCombo2" | "GroundAttackCombo1Reverse" | "GroundAttackCombo2Reverse"
        | "DeflectGuardAttack" => format!("{state}Release"),
        s if s.starts_with("HardDeflectAtk") => {
            // HardDeflectAtkSmallL -> HardDeflectAtkReleaseSmall_L
            let rest = &s["HardDeflectAtk".len()..];
            let (size, dir) = rest.split_at(rest.len() - 1);
            format!("HardDeflectAtkRelease{size}_{dir}")
        }
        s if s.starts_with("EasyDeflectedAtk") => format!("EasyDeflectedAtkRelease_{}", &s[s.len() - 1..]),
        s if s.starts_with("GroundStepAttack_") => s.replace("GroundStepAttack_", "GroundStepAttackRelease_"),
        s if s.starts_with("SprintAttack_") => s.replace("SprintAttack_", "SprintAttackRelease_"),
        _ => return None,
    })
}

/// Free turning rate [deg/s]: the anim's TAE SetTurnSpeed (e.g. SprintLoop 360) when it
/// has one, else the engine default, config movement.turn_speed = 720 (exe: the turn
/// controller's +0x28 = 720.0 in FUN_1407da540; FUN_1407daa20 uses the TAE value when >= 0).
fn turn_rate(d: &CharData, a: &Actor, config: &GameConfig, locked: bool) -> f32 {
    if a.anim.is_empty() { None } else { d.turn_speed(&a.anim, a.t, locked) }.unwrap_or(config.movement.turn_speed)
}

fn is_guard_start(state: &str) -> bool {
    state.starts_with("StandToDeflectGuard") || state == "SprintToDeflectGuard"
}

fn is_guard_reaction(state: &str) -> bool {
    state.starts_with("StandDeflect") || state.starts_with("HardDeflectDmg") || state.starts_with("EasyDeflectDmg")
}

/// Execute flags: a pending request fires while one is up (requested |= pending & the
/// execute mask +0xe0, set by FUN_140b2bdc0): 115 attack (bits 0,25), 117 guard (2),
/// 26 step (5,13,14), 119 jump (4), 31 item (7), 137 shinobi tool (18).
fn accept_flag(action: Action) -> &'static [i64] {
    match action {
        Action::Attack => &[FLAG_ACCEPT_ATTACK],
        Action::Guard => &[FLAG_ACCEPT_GUARD],
        Action::Step => &[FLAG_ACCEPT_STEP, FLAG_ACCEPT_STEP_ALT],
        Action::Jump => &[FLAG_ACCEPT_JUMP],
        Action::UseItem => &[FLAG_ACCEPT_ITEM],
        Action::CombatArt => &[FLAG_ACCEPT_ATTACK],
        Action::Prosthetic => &[FLAG_ACCEPT_PROSTHETIC],
    }
}

/// The engine's env(1106, button): is this input accepted right now?
fn accepts(d: &CharData, a: &Actor, action: Action) -> bool {
    if a.airborne {
        return false;
    }
    if is_free(&a.state) || sprint_window(d, a) {
        return true;
    }
    // The HKS sees the flags the previous frame's TAE update set (live: buffered inputs fire one
    // game frame later than a fresh press, docs/kb/input-buffer.md), so the execute window is
    // read at the previous step's time.
    !a.anim.is_empty() && accept_flag(action).iter().any(|&f| d.flag(&a.anim, a.prev_t, f))
}

/// Ref 1 SP_EF_REF_ENABLE_SPRINT_ACTION (SprintStartFromStep_F, SprintLoop, SprintToDeflectGuard
/// 6-15): sprinting is a free state for actions - SprintLoop has no ChrActionFlags at all, and
/// the HKS picks SprintAttack / SprintToDeflectGuard from this ref.
fn sprint_window(d: &CharData, a: &Actor) -> bool {
    !a.anim.is_empty() && d.has_ref(&a.anim, a.t, REF_ENABLE_SPRINT_ACTION)
}

pub(crate) fn stick_world(stick: Vec2, cam_yaw: f32) -> Vec3 {
    let rot = Quat::from_rotation_y(cam_yaw);
    (rot * Vec3::NEG_Z * stick.y + rot * Vec3::X * stick.x).normalize_or_zero()
}

fn yaw_of(dir: Vec3) -> f32 {
    f32::atan2(-dir.x, -dir.z)
}

fn turn_toward(yaw: f32, target: f32, max_step: f32) -> f32 {
    let diff = (target - yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
    yaw + diff.clamp(-max_step, max_step)
}

#[allow(clippy::too_many_arguments)]
fn decide(
    time: Res<Time>,
    combat: Res<Combat>,
    config: Res<GameConfig>,
    lock: Res<LockOn>,
    mut pad: ResMut<PadInput>,
    mut log: ResMut<CombatLog>,
    camera: Option<Single<&OrbitCamera>>,
    mut player: Single<(&mut Player, &mut Actor, &mut Transform), Without<Enemy>>,
    mut enemies: Query<(Entity, &mut Actor, &mut Enemy, &Transform), Without<Player>>,
    enemy_dummies: Query<&crate::model::Dummies, With<Enemy>>,
    globals: Query<&GlobalTransform>,
    mut throw: ResMut<ActiveThrow>,
) {
    let dt = time.delta_secs();
    let d = &combat.player;
    let (p, a, tf) = &mut *player;
    // Set again by the guard walk below when it applies; a locked-on step keeps its tilt.
    a.root_yaw = if a.state.starts_with("GroundStep_") { p.step_tilt } else { 0.0 };
    // WalkTwist (exe FUN_1407f6ff0): last tick's target eases in; locomotion / the guard walk set
    // the next one below, anything else lets it return to 0.
    let step = WALK_TWIST_RATE * dt;
    a.twist += (a.twist_target - a.twist).clamp(-step, step);
    a.twist_target = 0.0;

    // ActionRequest buffer (exe: SprjChrActionRequestModule, FUN_140b2c190). A press is
    // latched only while its accept bit is set (TAE accept flags, see buffers()) and stays
    // pending, with no timer, until the action fires on its execute flag (accept_flag) or a
    // state change resets it (keeps_buffer). config input.buffer > 0 adds a timed expiry
    // for experiments.
    if a.state != p.last_state {
        if !keeps_buffer(&a.state) {
            p.requests.clear();
        }
        // Combat changed the state; record_state then only sees decide's own changes.
        p.last_state = a.state.clone();
    }
    // Raw press (death screen choices are not TAE-buffered).
    let attack_pressed = pad.pressed.contains(&Action::Attack);
    for act in pad.pressed.drain(..) {
        if buffers(d, a, act) {
            p.requests.insert(act, 0.0);
        }
    }
    let buffer = config.input.buffer;
    p.requests.retain(|_, age| {
        *age += dt;
        buffer <= 0.0 || *age <= buffer
    });

    if pad.reset {
        pad.reset = false;
        p.gourd = config.player.gourd_charges;
        p.emblems = config.player.spirit_emblems;
        p.resurrections = config.player.resurrections;
        a.hp = a.hp_max;
        a.posture = 0.0;
        a.play("StandIdle", "");
        tf.translation = Vec3::new(0.0, CAPSULE_HALF_HEIGHT, 4.0);
    }
    // Resurrection: GroundRevival's TAE applies SpEffect 110015 "Resurrection Technique_HP
    // Half Recovery" (changeHpRate -50 -> +50 % of max HP) at frame 0.
    if a.state.starts_with("GroundRevival_") {
        let heal = d.anim(&a.anim).map_or(0.0, |an| {
            an.events
                .iter()
                .filter(|e| matches!(e.kind, 67 | 401) && e.start > a.prev_t - 1e-4 && e.start <= a.t && a.prev_t < a.t)
                .filter_map(|e| d.sp_effects.get(&e.arg_i64("SpEffectID").unwrap_or(0).to_string()))
                .map(|s| -s.change_hp_rate)
                .filter(|r| *r > 0.0)
                .sum::<f32>()
        });
        if heal > 0.0 {
            a.hp = (a.hp + a.hp_max * heal / 100.0).min(a.hp_max);
            log.push(format!("resurrected: +{:.0} HP", a.hp_max * heal / 100.0), Color::srgb(1.0, 0.5, 0.5));
        }
    }
    // Weapon style (TAE 32 SetWeaponStyle): "0: None" = sheathed (the blade rests in the scabbard,
    // WepAbsorpPosParam rightHang_0), "1: Right Weapon One-Handed" = in the hand.
    if let Some(an) = d.anim(&a.anim).filter(|_| a.prev_t < a.t) {
        let style = an.events.iter().filter(|e| e.kind == 32 && e.start > a.prev_t && e.start <= a.t).filter_map(|e| e.args.get("WeaponStyle")?.as_str()).last();
        if let Some(style) = style {
            a.sheathed = style.starts_with('0');
        }
    }
    // Sheathe / draw (X): HKS BEH_R_NON_COMBAT_AREA_ENTER / LEAVE, standing or moving variant
    // (GroundNonCombatArea[Move]Enter a000_700500 / 700501, Leave 700510 / 700511). Sheathed, an
    // attack / guard / art / prosthetic press draws the sword instead (Sekiro has no sword
    // actions while sheathed).
    let sword_press = [Action::Attack, Action::Guard, Action::CombatArt, Action::Prosthetic].iter().any(|k| p.requests.contains_key(k));
    let sheathe = std::mem::take(&mut pad.sheathe);
    if a.hp > 0.0 && !a.airborne && (sheathe || a.sheathed && sword_press) && (is_free(&a.state) || (!a.anim.is_empty() && accepts(d, a, Action::Attack))) {
        let moving = a.state == "Locomotion";
        let state = match (a.sheathed, moving) {
            (false, false) => "GroundNonCombatAreaEnter",
            (false, true) => "GroundNonCombatAreaMoveEnter",
            (true, false) => "GroundNonCombatAreaLeave",
            (true, true) => "GroundNonCombatAreaMoveLeave",
        };
        if a.play_state(d, state) {
            a.move_vel = Vec3::ZERO;
            p.requests.clear();
            return;
        }
    }
    if a.sheathed {
        for k in [Action::Attack, Action::Guard, Action::CombatArt, Action::Prosthetic] {
            p.requests.remove(&k);
        }
    }
    if a.hp <= 0.0 && !a.state.starts_with("GroundRevival_") {
        // GroundDeathStart (100 frames), then the choice: attack resurrects while a node is left
        // (GroundRevival_F), else back to the start position after the death loop.
        if !a.state.starts_with("GroundDeath") && !a.play_state(d, "GroundDeathStart_F") {
            a.procedural("Dead");
        }
        let start_done = a.state == "GroundDeathStart_F" && !a.anim.is_empty() && a.t >= d.length(&a.anim);
        if start_done {
            a.play_state(d, "GroundDeathLoop_F");
        }
        a.move_vel = Vec3::ZERO;
        // Killed in the air (e.g. hit as a jump starts): the body still falls and lands
        // instead of keeping its launch velocity.
        if a.airborne {
            let g = FALL_TYPES[(a.fall_type as usize).min(FALL_TYPES.len() - 1)].gravity();
            a.grav_y = g;
            a.vel_y += g * dt;
            a.air_base = Vec3::ZERO;
            if tf.translation.y <= CAPSULE_HALF_HEIGHT && a.vel_y < 0.0 {
                tf.translation.y = CAPSULE_HALF_HEIGHT;
                a.airborne = false;
                a.vel_y = 0.0;
            }
        }
        let choosing = a.state == "GroundDeathLoop_F";
        if choosing && attack_pressed && p.resurrections > 0 {
            p.resurrections -= 1;
            a.hp = 1.0; // the revival anim's SpEffect brings it to half
            a.play_state(d, "GroundRevival_F");
            log.push(format!("resurrection ({} left)", p.resurrections), Color::srgb(1.0, 0.5, 0.5));
            return;
        }
        if choosing && a.t > if p.resurrections > 0 { 6.0 } else { 1.5 } {
            pad.reset = true;
            log.push("revived", Color::WHITE);
        }
        return;
    }

    let cam_yaw = camera.map_or(0.0, |c| c.yaw);
    let stick = stick_world(pad.stick, cam_yaw);
    // Attack auto-homing (CSChrAutoHomingModule): while not locked on, an attack's first 1/6 s
    // (HKS _UpdateAutoAim -> act(156)) picks the nearest enemy inside LockCamParam's melee
    // auto-capture box; FUN_1409efdb0 then gives attacks the lock point or this target.
    if lock.target.is_none() && p.auto_aim && a.t < AUTO_AIM_TIME {
        let dir = if stick != Vec3::ZERO { stick } else { a.forward() };
        a.homing = auto_homing_target(&combat, tf.translation, dir, enemies.iter().map(|(e, ea, en, t)| (e, t.translation, ea.hp > 0.0 && !en.is_dead())));
    }
    let target_pos = lock
        .target
        .or(a.homing)
        .and_then(|e| enemies.get(e).ok())
        .map(|(_, _, _, t)| t.translation);
    let target_dir = target_pos.map(|tp| (tp - tf.translation).with_y(0.0).normalize_or_zero());
    let len = d.length(&a.anim);
    let ended = !a.anim.is_empty() && a.t >= len;

    // Deathblow start throw (ThrowParam 0000 崩し始動 a201_500000 / 0110 崩し背後始動): Wolf steps
    // in; the start anim's CommonBehavior (judge 600 at TAE frame 11; behind: 640 at 7) runs the main throw. Live
    // (rec_20261008_054259): 0.4 s of start, then the main anims begin together.
    if a.state == "DeathblowStart" {
        if let Some((ee, suffix)) = p.throw_start {
            let judged = d.anim(&a.anim).is_some_and(|an| an.events.iter().any(|e| e.kind == 5 && e.start <= a.t));
            if judged || ended {
                p.throw_start = None;
                if let (Ok((_, mut ea, mut enemy, etf)), Some(th)) = (enemies.get_mut(ee), combat.throw(combat.foe.throw_row(suffix))) {
                    main_deathblow(a, tf.translation, &mut ea, &mut enemy, etf.translation, ee, &combat, &th, suffix == 111, &mut throw, &mut log);
                }
            }
        }
        return;
    }

    // Deathblow: attack near a posture-broken enemy. ThrowParam 11020010 ("弾き",
    // broken by a deflect, reach 4 m) or 11020001 ("崩し", reach 2.1 m) give the
    // player's throw anim (a201_...) and the enemy's ThrowDef(Death) anim.
    if p.requests.contains_key(&Action::Attack) && accepts(d, a, Action::Attack) {
        for (ee, mut ea, mut enemy, etf) in &mut enemies {
            if !enemy.is_broken() {
                continue;
            }
            // ThrowParam start rows pick the side: "崩し始動" (0000) DiffAng 90-180 = facing the enemy,
            // "崩し背後始動" (0110) DiffAng 0-90 = behind it (Wolf and the enemy face the same way).
            // Behind -> "崩し背後本体" (0111, a20x_511200 = imports 510200's clip; isTurnAtker);
            // in front: "弾き0" (0010) after a deflect break, else "崩し本体" (0001).
            let to_enemy = (etf.translation - tf.translation).with_y(0.0).normalize_or_zero();
            let behind = ea.forward().angle_between(to_enemy).to_degrees() < 90.0;
            let suffix = if behind {
                111
            } else if ea.state.starts_with("AttackBoundEmptyStamina") {
                10
            } else {
                1
            };
            let row = combat.foe.throw_row(suffix);
            let Some(th) = combat.throw(row) else { continue };
            if etf.translation.distance(tf.translation) <= th.dist {
                p.requests.remove(&Action::Attack);
                // isTurnAtker: Wolf turns to the enemy at once (live: exactly).
                a.yaw = yaw_of((etf.translation - tf.translation).with_y(0.0));
                a.move_vel = Vec3::ZERO;
                let start = if suffix == 10 { None } else { combat.throw(combat.foe.throw_row(suffix - 1)) };
                if let Some(st) = start.filter(|st| d.anim(&st.atk_anim).is_some()) {
                    a.play("DeathblowStart", &st.atk_anim);
                    p.throw_start = Some((ee, suffix));
                    return;
                }
                main_deathblow(a, tf.translation, &mut ea, &mut enemy, etf.translation, ee, &combat, &th, behind, &mut throw, &mut log);
                return;
            }
        }
    }

    // Facing helper for action starts: auto-aim at the lock target, else snap to the stick.
    let aim = |a: &mut Actor| {
        if let Some(dir) = target_dir {
            a.yaw = yaw_of(dir);
        } else if stick != Vec3::ZERO {
            a.yaw = yaw_of(stick);
        }
    };

    // --- HKS g_behaviorValidateOrder (subset), highest priority first ---

    // Combat art (ACTION_ARM_SPECIAL_ATTACK): attack and guard pressed together, or attack
    // while guard is held. With an art equipped the HKS never reaches DeflectGuardAttack
    // (it needs env(345) == SP_ATK_TYPE_NONE).
    let art = art_anim(d, &combat, &config);
    let art_equipped = art.is_some();
    if p.requests.contains_key(&Action::Attack)
        && (p.requests.contains_key(&Action::Guard) || (pad.guard_held && art_equipped))
    {
        p.requests.remove(&Action::Guard);
        p.requests.remove(&Action::Attack);
        p.requests.insert(Action::CombatArt, 0.0);
    }
    if p.requests.contains_key(&Action::CombatArt) && accepts(d, a, Action::CombatArt) && art_equipped {
        p.requests.remove(&Action::CombatArt);
        // AttackAngle is measured from the facing before the art turns Wolf.
        let facing = a.forward();
        start_auto_aim(p, a, &combat, tf.translation, stick, &enemies);
        aim(a);
        a.move_vel = Vec3::ZERO;
        // Pressed again inside the art's combo window: its next state, in the art's anim group.
        if d.has_ref(&a.anim, a.t, REF_ENABLE_SP_ATK_COMBO) {
            let next = art_kind(&combat, &config).and_then(|(cat, unlock)| {
                let state = art_combo_next(&a.state, cat, |r| r == unlock)?;
                let id = d.states.get(state)?.rsplit('_').next()?.to_string();
                let key = format!("a{cat:03}_{id}");
                d.anim(&key).is_some_and(|an| an.duration.is_some()).then_some((state, key))
            });
            if let Some((state, key)) = next {
                a.play(state, &key);
                return;
            }
        }
        // Opening state by art type (sprint / step variants), in the art's anim group.
        let start = art_kind(&combat, &config).and_then(|(cat, unlock)| {
            art_start_states(cat, unlock, sprint_window(d, a), stick, facing).into_iter().find_map(|state| {
                let id = d.states.get(&state)?.rsplit('_').next()?.to_string();
                let key = format!("a{cat:03}_{id}");
                d.anim(&key).is_some_and(|an| an.duration.is_some()).then_some((state, key))
            })
        });
        match start {
            Some((state, key)) => a.play(&state, &key),
            None => a.play("GroundSpecialAttackCombo1", art.as_deref().unwrap_or(ART_WHIRLWIND)),
        }
        return;
    }

    // Hold art (spAtkType 104): GroundSpacialAttackHoldStart -> HoldLoop while the art is held.
    // Letting go of attack = BEH_A_GROUND_SP_ATTACK_RELEASE: HoldAction with SP_EF_REF_WEP_SP_ATK_
    // UNLOCK_104_VARIATION_ATTACK (287, art 6100) up, else GroundSpecialAttackCombo1 (the draw
    // slash); letting go of guard = BEH_A_GROUND_SP_ATTACK_GUARD_RELEASE: HoldEnd.
    if matches!(a.state.as_str(), "GroundSpacialAttackHoldStart" | "GroundSpacialAttackHoldLoop" | "SprintSpecialAttackHoldStart") {
        if let Some((cat, unlock)) = art_kind(&combat, &config) {
            let next = if !pad.attack_held {
                Some(if unlock == 287 { "GroundSpacialAttackHoldAction" } else { "GroundSpecialAttackCombo1" })
            } else if !pad.guard_held {
                Some("GroundSpacialAttackHoldEnd")
            } else if ended {
                Some("GroundSpacialAttackHoldLoop")
            } else {
                None
            };
            if let Some(state) = next {
                if let Some(key) = d.states.get(state).and_then(|k| k.rsplit('_').next()).map(|id| format!("a{cat:03}_{id}")) {
                    if state != "GroundSpacialAttackHoldEnd" {
                        start_auto_aim(p, a, &combat, tf.translation, stick, &enemies);
                    }
                    a.play(state, &key);
                }
            }
            return;
        }
    }

    // Shinobi Prosthetic: the shuriken throw (the bullet itself spawns from its TAE).
    if p.requests.contains_key(&Action::Prosthetic) && accepts(d, a, Action::Prosthetic) {
        p.requests.remove(&Action::Prosthetic);
        if p.emblems == 0 {
            log.push("no spirit emblems", Color::srgb(0.7, 0.7, 0.7));
        } else {
            aim(a);
            a.move_vel = Vec3::ZERO;
            a.play_state(d, "GroundSubAttackCombo1");
            return;
        }
    }
    // Let go before the full throw (frame 21): the quick Release throw.
    if a.state == "GroundSubAttackCombo1" && !pad.prosthetic_held && a.t < 20.0 / crate::data::TAE_FPS {
        a.play_state(d, "GroundSubAttackCombo1Release");
        return;
    }

    // Healing Gourd: drink (again from a drink: Repeat), or the empty-gourd shake.
    if p.requests.contains_key(&Action::UseItem) && accepts(d, a, Action::UseItem) {
        p.requests.remove(&Action::UseItem);
        let state = if p.gourd == 0 {
            "ItemGourdDrinkFailed"
        } else if a.state.starts_with("ItemGourdDrink") {
            "ItemGourdDrinkRepeat"
        } else {
            "ItemGourdDrink"
        };
        a.move_vel = Vec3::ZERO;
        a.play_state(d, state);
        return;
    }
    // ConsumeCurrentGoods (TAE 65, drink frame 21 / repeat frame 9): one charge, the heal.
    if a.state == "ItemGourdDrink" || a.state == "ItemGourdDrinkRepeat" {
        let consumed = d.anim(&a.anim).is_some_and(|an| an.events.iter().any(|e| e.kind == 65 && e.start > a.prev_t && e.start <= a.t));
        if consumed && p.gourd > 0 {
            p.gourd -= 1;
            let rate = d.sp_effects.get(GOURD_SPEFFECT).map_or(40.0, |s| -s.change_hp_estus_flask_rate);
            let heal = a.hp_max * rate / 100.0;
            a.hp = (a.hp + heal).min(a.hp_max);
            log.push(format!("gourd: +{heal:.0} HP ({} left)", p.gourd), Color::srgb(0.5, 1.0, 0.6));
        }
        // Flag 90: may walk while drinking.
        if !a.anim.is_empty() && d.flag(&a.anim, a.t, FLAG_LIMIT_WALK) {
            let walk = d.root_velocity("a000_000200").length();
            a.move_vel = stick * walk;
        }
    }

    // Jump
    if p.requests.contains_key(&Action::Jump) && accepts(d, a, Action::Jump) {
        p.requests.remove(&Action::Jump);
        p.jump_forward = stick != Vec3::ZERO;
        p.jump_land = None;
        match (stick != Vec3::ZERO, lock.target.is_some()) {
            (false, _) => {
                a.play_state(d, "VerticalGroundJumpReady");
            }
            // HKS _SetJumpDirection: locked on, the stick's angle to the facing (the target) picks
            // the jump: |a| <= 18.75 lock-on forward, then 45-degree sectors F_R / R / B_R / B and
            // mirrored. Each launches by its own ChrPhysicsVelocityChangeParam row (111-118) and
            // lands in its LandGroundPositioningJump (live: lock-on forward 1.26 m, 0.65 s).
            (true, true) => {
                if let Some(td) = target_dir {
                    a.yaw = yaw_of(td);
                }
                let local = Quat::from_rotation_y(-a.yaw) * stick;
                let angle = local.x.atan2(-local.z).to_degrees();
                let (ready, anim, land) = directional_jump(angle);
                let played = match anim {
                    Some(k) if d.anim(k).is_some() => {
                        a.play(ready, k);
                        true
                    }
                    Some(_) => false,
                    None => a.play_state(d, ready),
                };
                if played {
                    p.jump_land = Some(land);
                } else {
                    a.play_state(d, "LockonForwardGroundJumpReady");
                    p.jump_land = Some("LandGroundPositioningJump_LockOn_F");
                }
            }
            (true, false) => {
                a.yaw = yaw_of(stick);
                a.play_state(d, "ForwardGroundJumpReady");
            }
        }
        a.move_vel = Vec3::ZERO;
        return;
    }

    // Step (dodge). Locked on: 4 directions relative to the target. Free: forward along the stick, or neutral.
    if p.requests.contains_key(&Action::Step) && accepts(d, a, Action::Step) {
        p.requests.remove(&Action::Step);
        a.move_vel = Vec3::ZERO;
        p.step_tilt = 0.0;
        let state = match (target_dir, stick != Vec3::ZERO) {
            (Some(td), true) => {
                a.yaw = yaw_of(td);
                let local = Quat::from_rotation_y(-a.yaw) * stick;
                // HKS _set4DirStepDir (PRM_4DIR_STEP_STICK_RANGE: 45-degree quadrants) and
                // _set4DirStepTilt: the step is turned by the stick's offset from the quadrant
                // centre (none within +-18.75 of forward), so it goes along the stick.
                let angle = local.x.atan2(-local.z).to_degrees();
                let (state, centre) = if angle.abs() < 45.0 {
                    ("GroundStep_F", 0.0)
                } else if angle > -135.0 && angle < -45.0 {
                    ("GroundStep_L", -90.0)
                } else if (45.0..135.0).contains(&angle) {
                    ("GroundStep_R", 90.0)
                } else {
                    ("GroundStep_B", if angle > 0.0 { 180.0 } else { -180.0 })
                };
                let tilt = if state == "GroundStep_F" && angle.abs() < 18.75 { 0.0 } else { angle - centre };
                p.step_tilt = (-tilt).to_radians();
                state
            }
            (Some(td), false) => {
                a.yaw = yaw_of(td);
                "GroundStep_N"
            }
            (None, true) => {
                a.yaw = yaw_of(stick);
                "GroundStep_F"
            }
            (None, false) => "GroundStep_N",
        };
        a.play_state(d, state);
        return;
    }

    // Attack: routed by the combo refs active in the current anim.
    if p.requests.contains_key(&Action::Attack) && accepts(d, a, Action::Attack) {
        p.requests.remove(&Action::Attack);
        let next = attack_route(d, a, stick, art_equipped);
        start_auto_aim(p, a, &combat, tf.translation, stick, &enemies);
        aim(a);
        a.move_vel = Vec3::ZERO;
        if !a.play_state(d, &next) {
            a.play_state(d, "GroundAttackCombo1");
        }
        return;
    }

    // Release attack: let go of attack while ref 208 is up (otherwise the hold becomes the charged thrust).
    if !pad.attack_held && !a.anim.is_empty() && d.has_ref(&a.anim, a.t, REF_RELEASE_ATTACK) {
        if let Some(r) = release_of(&a.state).filter(|r| d.anim_key(r).is_some()) {
            // The release clip starts from frame 0: its AI notify (flag 63) is at frame 3 and its
            // hit at frames 7-11, so a quick slash lands 6 + 7 frames after the press.
            a.play_state(d, &r);
            return;
        }
    }

    // Art release (HKS BEH_A_GROUND_SP_ATTACK_RELEASE): Combo1 -> Combo1Release, Combo2 ->
    // Combo2Release, SprintSpecialAttack -> SprintSpecialAttackRelease, in the art's anim group.
    if !pad.attack_held && !a.anim.is_empty() && d.has_ref(&a.anim, a.t, REF_SP_ATK_RELEASE) {
        let rel = match a.state.as_str() {
            "GroundSpecialAttackCombo1" => Some("GroundSpecialAttackCombo1Release"),
            "GroundSpecialAttackCombo2" => Some("GroundSpecialAttackCombo2Release"),
            "SprintSpecialAttack" => Some("SprintSpecialAttackRelease"),
            _ => None,
        };
        let next = rel.zip(art_kind(&combat, &config)).and_then(|(state, (cat, _))| {
            let id = d.states.get(state)?.rsplit('_').next()?.to_string();
            let key = format!("a{cat:03}_{id}");
            d.anim(&key).is_some_and(|an| an.duration.is_some()).then_some((state, key))
        });
        if let Some((state, key)) = next {
            a.play(state, &key);
            return;
        }
    }

    // BEH_A_DEFLECT_GUARD_START, first branches (c0000_transition.lua:4438-4442):
    // ref 228 TAE_ENABLE_ADD_JUST_DEFLECT (and not 412 ..._GUARD_CANCEL; StandDeflectEasy* frames
    // 0-9) -> W_AddHardDeflectGuard: an additive deflect window (a000_299060, ref 203 frames 0-9)
    // over the block recoil, which keeps playing. Ref 503 TAE_ENABLE_HIT_DEFLECT_CANCEL
    // (StandDamageMiddle/Large from frame 12) -> StandDeflectGuardFromDamage, its direction the
    // reaction's DamageDirection (hkx selector [0] F [1] B [2] L [3] R).
    if p.requests.contains_key(&Action::Guard) && accepts(d, a, Action::Guard) && !a.anim.is_empty() && !a.airborne {
        if d.has_ref(&a.anim, a.t, REF_ADD_JUST_DEFLECT) && !d.has_ref(&a.anim, a.t, REF_ADD_GUARD_CANCEL) {
            if let Some(k) = d.anim_key("AddHardDeflectGuard") {
                p.requests.remove(&Action::Guard);
                a.add_anim = k.to_string();
                a.add_t = 0.0;
                return;
            }
        }
        if d.has_ref(&a.anim, a.t, REF_HIT_DEFLECT_CANCEL) {
            let dir = a.state.rsplit('_').next().filter(|s| matches!(*s, "F" | "B" | "L" | "R")).unwrap_or("F").to_string();
            p.requests.remove(&Action::Guard);
            aim(a);
            a.move_vel = Vec3::ZERO;
            if a.play_state(d, &format!("StandDeflectGuardFromDamage_{dir}")) {
                return;
            }
        }
    }

    // Deflect chain: pressing again during a guard start (ref 212) steps 1 -> 2 -> 3 -> 4.
    if is_guard_start(&a.state)
        && p.requests.contains_key(&Action::Guard)
        && accepts(d, a, Action::Guard)
        && d.has_ref(&a.anim, a.t, REF_GUARD_COMBO)
    {
        p.requests.remove(&Action::Guard);
        let next = match a.state.as_str() {
            "StandToDeflectGuard2" => "StandToDeflectGuard3",
            "StandToDeflectGuard3" | "StandToDeflectGuard4" => "StandToDeflectGuard4",
            _ => "StandToDeflectGuard2",
        };
        aim(a);
        a.play_state(d, next);
        return;
    }

    // Deflect start: fresh press in a guard window, or holding guard while ref 221 is up.
    // A buffered guard press only fires while guard is still held (live recording 2026-10-08:
    // taps let go before the window opened were dropped out of GroundStep_N, HardDeflectedL,
    // GroundAttackCombo1Release and Combo3; attack presses survive being let go). The spam chain
    // (ref 212) and the additive deflect / guard cancel (228 / 412) above keep released taps.
    // A press made this frame counts as held (the button is still down when it executes).
    let fresh_guard = p.requests.get(&Action::Guard).is_some_and(|&age| age <= dt * 1.5);
    let pressed_guard = p.requests.contains_key(&Action::Guard) && accepts(d, a, Action::Guard) && (pad.guard_held || fresh_guard);
    // HKS guard start (c0000_transition.lua:6161): ADD_ACTION_INPUT_GUARD (411, from the additive
    // deflect) together with the base's ADD_ACTION_INPUT_GUARD_CANCEL (412) starts the guard even
    // with the button let go - live: StandDeflectHardSmall tap at f1.5-3.4 -> StandToDeflectGuard f9.4.
    let add_guard = !a.add_anim.is_empty()
        && d.has_ref(&a.add_anim, a.add_t, REF_ADD_INPUT_GUARD)
        && !a.anim.is_empty()
        && d.has_ref(&a.anim, a.t, REF_ADD_GUARD_CANCEL);
    let pressed_guard = pressed_guard || add_guard;
    let held_guard = pad.guard_held && !a.anim.is_empty() && d.has_ref(&a.anim, a.t, REF_PRESS_GUARD);
    if (pressed_guard || held_guard) && !is_guard_idle(&a.state) && !a.airborne {
        p.requests.remove(&Action::Guard);
        let sprinting = a.state.starts_with("Sprint") || (!a.anim.is_empty() && d.has_ref(&a.anim, a.t, REF_ENABLE_SPRINT_ACTION));
        let state = if sprinting {
            "SprintToDeflectGuard"
        } else if !a.anim.is_empty() && d.has_ref(&a.anim, a.t, REF_GUARD_REVERSE) {
            "StandToDeflectGuard1Reverse"
        } else {
            "StandToDeflectGuard"
        };
        aim(a);
        a.move_vel = Vec3::ZERO;
        a.play_state(d, state);
        return;
    }

    // Deflect end
    let can_end = is_guard_idle(&a.state) || (is_guard_start(&a.state) && d.has_ref(&a.anim, a.t, REF_GUARD_CAN_END));
    if can_end && !pad.guard_held {
        // BEH_A_DEFLECT_GUARD_END: idle -> DeflectGuardToStand; moving -> DeflectGuardToStandMove
        // (hkx selector on MoveSpeedIndex: [0] Walk a050_203011, [1] Run a050_203012), an upper-body
        // state over the walk/run (is_upper_action). Ref 220 picks the *Variation ends; no Wolf
        // anim raises it.
        let state = if stick == Vec3::ZERO {
            "DeflectGuardToStand"
        } else if pad.walk {
            "DeflectGuardToStandMoveWalk"
        } else {
            "DeflectGuardToStandMoveRun"
        };
        a.move_index = if pad.walk { 0 } else { 1 };
        if p.speed_level < 1.0 && stick != Vec3::ZERO {
            // The guard walk is the walk speed level.
            p.speed_level = 1.0;
        }
        a.play_state(d, state);
        return;
    }

    // Sprint: hold dodge through the step's ref 15 window, keep holding to keep sprinting.
    let moving = stick != Vec3::ZERO;
    if a.state.starts_with("GroundStep") && d.has_ref(&a.anim, a.t, REF_TRANSITION_SPRINT) && pad.dodge_held && moving {
        a.yaw = yaw_of(stick);
        a.play_state(d, "SprintStartFromStep_F");
        return;
    }
    // Sprint quick turn (HKS BEH_A_SPRINT_QUICK_TURN): SprintQuickTurnReady (1 frame) picks
    // Right/Left180 by the side the stick points to; the clip's root motion turns Wolf around.
    if a.state == "SprintQuickTurnReady" {
        let side = if stick.dot(a.forward().cross(Vec3::Y)) > 0.0 { "SprintQuickTurnRight180" } else { "SprintQuickTurnLeft180" };
        a.play_state(d, side);
        return;
    }
    if a.state.starts_with("SprintQuickTurn") && ended {
        a.play_state(d, if pad.dodge_held && moving { "SprintLoop" } else { "SprintStopReady" });
        return;
    }
    if a.state == "SprintStartFromStep_F" || a.state == "SprintLoop" {
        if !(pad.dodge_held && moving) {
            a.play_state(d, "SprintStopReady");
            return;
        }
        // Stick more than SPRINT_BRAKE_ANGLE (135 deg) from the facing while the TAE allows it
        // (SpEffect ref 2 SP_EF_REF_TAE_ENABLE_SPRINT_QUICK_TURN), not locked on.
        if lock.target.is_none()
            && !a.anim.is_empty()
            && d.has_ref(&a.anim, a.t, REF_ENABLE_SPRINT_QUICK_TURN)
            && a.forward().angle_between(stick).to_degrees() > SPRINT_BRAKE_ANGLE
            && a.play_state(d, "SprintQuickTurnReady")
        {
            return;
        }
        a.yaw = turn_toward(a.yaw, yaw_of(stick), turn_rate(d, a, &config, lock.target.is_some()).to_radians() * dt);
        if ended {
            a.play_state(d, "SprintLoop");
        }
        return;
    }

    // TAE 920 ChrPhysicsVelocityChange: new velocity = current * scale + change.
    if !a.vel_change_done {
        if let Some(vc) = d.velocity_change(&a.anim, a.prev_t, a.t).and_then(|id| combat.velocity_change_row(id)) {
            a.vel_change_done = true;
            // Facing = 0 deg; positive angles taken as to the right.
            let dir = Quat::from_rotation_y(-vc.h_angle.to_radians()) * a.forward();
            a.air_base = a.move_vel * vc.h_scale + dir * vc.h_change;
            a.vel_y = a.vel_y * vc.v_scale + vc.v_change;
            a.fall_type = vc.fall_type;
            a.airborne = true;
            a.move_vel = a.air_base;
        }
    }

    // Jumps: Ready -> Start (TAE 920 launches) -> Fall loop -> land.
    if ended && a.state.contains("GroundJumpReady") {
        // Side jumps have no behaviour-graph clip (CMSG anim 0): their raw anims a000_20111x.
        match a.state.as_str() {
            "LeftsideGroundJumpReady" => a.play("LeftsideGroundJumpStart", "a000_201112"),
            "RightsideGroundJumpReady" => a.play("RightsideGroundJumpStart", "a000_201113"),
            _ => {
                let start = a.state.replace("Ready", "Start");
                a.play_state(d, &start);
            }
        }
        return;
    }
    if a.airborne {
        // Air actions open on the jump/air anims' own cancel flags.
        let air_ok = |a: &Actor, f: i64| !a.anim.is_empty() && d.flag(&a.anim, a.t, f);
        if p.requests.remove(&Action::Guard).is_some() {
            if air_ok(a, FLAG_ACCEPT_GUARD) || is_air_guard(&a.state) {
                a.play_state(d, "AirDeflectGuardStart");
            }
        } else if p.requests.remove(&Action::Jump).is_some() {
            // BEH_A_AIR_KICK: a jump press in the air always kicks (AirKick, a000_213100) unless
            // an SpEffect with behaviorRefId 108 (SP_EF_REF_DISABLE_AIR_KICK) is active.
            if air_ok(a, FLAG_ACCEPT_JUMP) && !sp_ref_active(d, a, SP_REF_DISABLE_AIR_KICK) {
                a.play_state(d, "AirKick");
            }
        } else if p.requests.contains_key(&Action::Attack) && air_ok(a, FLAG_ACCEPT_ATTACK) && start_plunge(p, a, &combat, tf.translation, &mut enemies, &mut log) {
            p.requests.remove(&Action::Attack);
        } else if p.requests.contains_key(&Action::Attack) && air_ok(a, FLAG_ACCEPT_ATTACK) {
            p.requests.remove(&Action::Attack);
            // BEH_A_AIR_ATTACK: refs 214/215/216 pick AirComboAttack1/2/3 (1 -> 2 -> 3 -> 2 ...).
            let has = |r: i64| !a.anim.is_empty() && d.has_ref(&a.anim, a.t, r);
            let next = if has(215) {
                "AirComboAttack2"
            } else if has(216) {
                "AirComboAttack3"
            } else {
                "AirComboAttack1"
            };
            a.play_state(d, next);
        }
        // BEH_R_ENEMY_JUMP: env(2004) - the kick's own attack (TAE 307 PCBehavior 901 ->
        // AtkParam_Pc 901, frames 9-21) connected - while AirKick's SpEffect 100334
        // (behaviorRefId 204, frames 9-21) is up -> AirKickEnemyJumpStart (no gravity frames
        // 0-6, then TAE 920 row 2101 relaunches). Also how sweeps are jumped (AirKick has
        // sweep i-frames).
        if a.state == "AirKick" && a.attack_hit && sp_ref_active(d, a, SP_REF_KICK_ENEMY_JUMP) {
            a.play_state(d, "AirKickEnemyJumpStart_F");
            a.air_base = Vec3::ZERO;
            a.vel_y = 0.0;
            log.push("kick jump", Color::srgb(0.8, 0.9, 1.0));
        }
        let in_air_guard = is_air_guard(&a.state);
        if a.state == "AirDeflectGuardLoop" && !pad.guard_held {
            a.play_state(d, "AirDeflectGuardEnd");
        }

        let ft = FALL_TYPES[(a.fall_type as usize).min(FALL_TYPES.len() - 1)];
        // ChrActionFlag 27 SetNoGravity holds the character in place.
        if !a.anim.is_empty() && d.flag(&a.anim, a.t, FLAG_NO_GRAVITY) {
            a.vel_y = 0.0;
            a.grav_y = 0.0;
        } else {
            a.grav_y = ft.gravity();
            a.vel_y += ft.gravity() * dt;
        }
        // One horizontal air velocity (exe FUN_140bac120, fall module). It decays along its
        // direction by HorizontalAcceleration (left alone once slower than one step).
        let speed = a.air_base.length();
        let decel = ft.h_accel * dt;
        if speed > 0.0 && (decel >= 0.0 || -decel <= speed) {
            let step = a.air_base / speed * decel;
            a.air_base += step;
        }
        // Stick: a = StickAcceleration * stick * dt, split along the current velocity (or the
        // stick when still). The across part always steers; the along part brakes in full but
        // only speeds up while under StickAccelerationMaxVelocity, which is all it caps.
        if ft.controllable && stick != Vec3::ZERO {
            let acc = stick * ft.stick_accel * dt;
            let speed = a.air_base.length();
            let dir = if speed > 0.0 { a.air_base / speed } else { stick.normalize() };
            let along = dir.dot(acc);
            let par = dir * along;
            a.air_base += if along <= 0.0 { par } else { dir * along.min((ft.stick_max_vel - speed).max(0.0)) };
            a.air_base += acc - par;
        }
        // Plunge deathblow: ChrPhysicsHomingParam 200000300 (TAE 920 row 8000 on the plunge anim):
        // fallCorrectionGuaranteeArrival 1 - the horizontal velocity is re-aimed every frame so
        // the fall ends on the target - and fallCorrectionTurn 1 (face it); no chase acceleration.
        // Target: the defender's targetBaseDmyPolyId 233 (on its model root) + targetOffset
        // (0, 0, -0.3) in that dummy's space; the enemy's position without a model (tests).
        if a.state == "PlungeDeathblow" {
            let dmy = p.plunge.and_then(|e| enemy_dummies.get(e).ok()).and_then(|dm| dm.0.get(&233)).and_then(|&de| globals.get(de).ok());
            let tp = match dmy {
                Some(g) => Some(g.transform_point(Vec3::new(0.0, 0.0, -0.3))),
                None => p.plunge.and_then(|e| enemies.get(e).ok()).map(|(_, _, _, t)| t.translation),
            };
            if let Some(tp) = tp {
                let g = ft.gravity();
                let h = tf.translation.y - CAPSULE_HALF_HEIGHT;
                // Time to the floor: h + vy t + g t^2 / 2 = 0 (g < 0).
                let disc = a.vel_y * a.vel_y - 2.0 * g * h;
                let t_land = if g < 0.0 && disc >= 0.0 { (-a.vel_y - disc.sqrt()) / g } else { 0.0 };
                let to = (tp - tf.translation).with_y(0.0);
                if t_land > 1e-3 {
                    a.air_base = to / t_land;
                }
                if to.length_squared() > 1e-4 {
                    a.yaw = yaw_of(to);
                }
            }
        }
        a.move_vel = a.air_base;
        if tf.translation.y <= CAPSULE_HALF_HEIGHT && a.vel_y < 0.0 {
            tf.translation.y = CAPSULE_HALF_HEIGHT;
            a.airborne = false;
            a.vel_y = 0.0;
            a.move_vel = Vec3::ZERO;
            a.air_base = Vec3::ZERO;
            // Plunge lands: ThrowParam 崩し落下1 (suffix 151): Wolf a20x_511410 (510310's clip),
            // the enemy ThrowDefDeath13411.
            if a.state == "PlungeDeathblow" {
                if let (Some(target), Some(th)) = (p.plunge.take(), combat.throw(combat.foe.throw_row(151))) {
                    if let Ok((_, mut ea, mut enemy, etf)) = enemies.get_mut(target) {
                        if d.anim(&th.atk_anim).is_some() {
                            a.play("Deathblow", &th.atk_anim);
                        }
                        ea.yaw = yaw_of((tf.translation - etf.translation).with_y(0.0));
                        enemy.deathblow(&mut ea, &combat, th.def_anim);
                        throw.0 = Some(ThrowHold::new(target, &th, "Deathblow"));
                        log.push("DEATHBLOW (plunge)", Color::srgb(1.0, 0.2, 0.2));
                        return;
                    }
                }
            }
            // Blown away: the matching Land anim (StandDamage*Blow*/Upper*).
            let falling_reaction = |st: &str| {
                st.contains("BlowStart") || st.contains("BlowFallLoop") || st.contains("UpperStart") || st.contains("UpperFallLoop")
                    || (st.starts_with("AirDamage") && (st.contains("Start") || st.contains("FallLoop")))
            };
            // Air posture breaks land into their LandAir versions (W_LandAirDamageBreak /
            // W_LandAirDeflectGuardBreak).
            // Landing mid air-slash while ref 201 (SP_EF_REF_TAE_ENABLE_ORIGINAL_LAND_ACTION) is
            // up: LandAirComboAttackN continues from the same time (W_LandAirComboAttackN).
            if a.state.starts_with("AirComboAttack") && !a.anim.is_empty() && d.has_ref(&a.anim, a.t, REF_ORIGINAL_LAND_ACTION) {
                let land = format!("Land{}", a.state);
                if a.continue_state(d, &land) {
                    return;
                }
            }
            if matches!(a.state.as_str(), "AirDamageBreak" | "AirDeflectGuardBreak") {
                let land = format!("Land{}", a.state);
                if a.play_state(d, &land) {
                    return;
                }
            }
            let blow_land = falling_reaction(&a.state)
                .then(|| a.state.replace("FallLoop", "Land").replace("Start", "Land"));
            match blow_land {
                Some(land) if a.play_state(d, &land) => {}
                _ if in_air_guard && pad.guard_held => {
                    a.play_state(d, "LandAirDeflectGuard");
                }
                // c0000_transition.lua BEH_R_LAND: stick held -> straight into moving
                // (W_StandMoveStartFromFreeFallShortStiff); else W_LandGroundJump by jump type
                // (LandVerticalGroundJump a000_201040 / LandForwardGroundJump a000_201045).
                // GroundJumpLandReady (a000_201050) is the pose just before touchdown, not a landing.
                // Directional jumps: W_LandGroundPositioningJump comes before moving on.
                _ if p.jump_land.is_some() && a.state.contains("GroundJump") => {
                    let land = p.jump_land.take().unwrap_or("LandGroundPositioningJump_F");
                    a.play_state(d, land);
                }
                _ if stick != Vec3::ZERO => {
                    p.speed_level = if pad.walk { 1.0 } else { 2.0 };
                    a.procedural("Locomotion");
                }
                _ => {
                    a.play_state(d, if p.jump_forward { "LandForwardGroundJump" } else { "LandVerticalGroundJump" });
                }
            }
            return;
        }
        if ended {
            if a.state.ends_with("GroundJumpStart") {
                let fall = a.state.replace("Start", "Fall");
                if !a.play_state(d, &fall) {
                    a.t = len;
                }
            } else if a.state.ends_with("GroundJumpFall") {
                a.t %= len.max(1e-3); // loop
            } else if a.state.contains("BlowStart") || a.state.contains("UpperStart") || (a.state.starts_with("AirDamage") && a.state.contains("Start")) {
                let fall = a.state.replace("Start", "FallLoop");
                if !a.play_state(d, &fall) {
                    a.t = len;
                }
            } else if a.state.contains("BlowFallLoop") || a.state.contains("UpperFallLoop") || (a.state.starts_with("AirDamage") && a.state.contains("FallLoop")) {
                a.t %= len.max(1e-3);
            } else if a.state == "AirDamageBreak" || a.state == "AirDeflectGuardBreak" {
                a.t = len; // hold the broken pose until landing
            } else if a.state == "AirDeflectGuardStart" || a.state == "AirDeflectGuardLoop" {
                a.play_state(d, if pad.guard_held { "AirDeflectGuardLoop" } else { "AirDeflectGuardEnd" });
            } else if !a.play_state(d, "ForwardGroundJumpFall") {
                // Air reactions, kick, attack, guard end: fall.
                a.t = len;
            }
        }
        return;
    }

    // Anim ended: guard starts and guard reactions return to guard idle if guard is held.
    // Knocked down out of a posture break (behavior graph transitions): BreakDamage and
    // the break blows' landings -> StandDamageBreakDown -> StandDamageLargeDownWakeUp,
    // face up only after a small blow from the front (Selector_ProneDirection).
    if ended {
        let down = if a.state == "StandDamageBreakSmallBlow_F" { "FaceUp" } else { "FaceDown" };
        // Unbroken knockdowns (graph: *BlowLand / LargeUpperLand / LargePound / SmallBlow ->
        // StandDamageLargeDown): prone side per HKS _setProneDir - a small blow from the front
        // lands face up, everything else face down. StandDamageLargeDown plays the get-up clip
        // (a000_10031x, the same one the break chain ends with).
        let normal_down = if a.state == "StandDamageSmallBlow_F" { "FaceUp" } else { "FaceDown" };
        let next = if a.state == "StandDamageBreakDamage"
            || a.state == "StandDamageBreakLargePound"
            || (a.state.starts_with("StandDamageBreak") && (a.state.contains("Land") || a.state.contains("SmallBlow")))
        {
            Some(format!("StandDamageBreakDown_{down}"))
        } else if !a.state.starts_with("StandDamageBreak")
            && (a.state.starts_with("StandDamage") && (a.state.contains("BlowLand") || a.state.contains("UpperLand") || a.state.contains("SmallBlow"))
                || a.state == "StandDamageLargePound")
        {
            Some(format!("StandDamageLargeDown_{normal_down}"))
        } else if a.state.starts_with("AirDamage") && a.state.contains("Land") {
            // AirDamageLargeLand / LargePoundLand -> StandDamageLargeDown, face down.
            Some("StandDamageLargeDown_FaceDown".to_string())
        } else {
            a.state.strip_prefix("StandDamageBreakDown_").map(|dir| format!("StandDamageLargeDownWakeUp_{dir}"))
        };
        if let Some(next) = next {
            if a.play_state(d, &next) {
                return;
            }
        }
    }
    if ended {
        let to_guard = (is_guard_start(&a.state) || is_guard_reaction(&a.state) || is_guard_idle(&a.state) || a.state == "LandAirDeflectGuard")
            && pad.guard_held;
        if to_guard {
            a.play_state(d, "DeflectGuardIdle");
        } else if is_upper_action(&a.state) && moving {
            // FireStateEndEvent: an upper action ends into W_StandMoveLoop while moving; the legs'
            // loop keeps its phase (it ran on the action's clock).
            let (t, prev_t) = (a.t, a.prev_t);
            a.procedural("Locomotion");
            (a.t, a.prev_t, a.move_start) = (t, prev_t, 0.0);
        } else {
            a.play("StandIdle", "");
        }
    }

    // Locked-on idle turn in place (HKS BEH_A_GROUND_QUICK_TURN / _GroundQuickTurn): standing
    // still with the target more than 60 deg off the facing -> StandQuickTurn{Right,Left}{90,180}
    // (180 beyond 120 deg); the clip's root motion does the turning.
    if a.state == "StandIdle" && stick == Vec3::ZERO {
        if let Some(td) = target_dir {
            let ang = a.forward().angle_between(td).to_degrees();
            if ang > 60.0 {
                let side = if td.dot(a.forward().cross(Vec3::Y)) > 0.0 { "Right" } else { "Left" };
                let size = if ang > 120.0 { "180" } else { "90" };
                if a.play_state(d, &format!("StandQuickTurn{side}{size}")) {
                    return;
                }
            }
        }
    }

    // Movement: free states, or the TAE move window.
    let move_ok = is_free(&a.state) || (!a.anim.is_empty() && d.flag(&a.anim, a.t, FLAG_ACCEPT_MOVE));
    // Moving out of a guard start or a block/deflect reaction with guard held stays in the guard
    // style: HKS BEH_A_GROUND_MOVE_START with STYLE_TYPE_GROUND_GUARD -> W_DeflectGuardMove (not
    // the run - which also stood Wolf still through the reaction's tail before running off).
    if moving && move_ok && pad.guard_held && !a.airborne && (is_guard_start(&a.state) || is_guard_reaction(&a.state)) {
        a.play_state(d, "DeflectGuardIdle");
    }
    if is_guard_idle(&a.state) {
        // Guard walk: DeflectGuardMoveF/B/L/R (root motion 2.0 m / 40 frames). Wolf keeps his
        // facing (guard idle TAE SetTurnSpeed 0; live, rec_20261008_051450: the facing holds while
        // he walks any way) - locked on he faces the target. The stick's angle to the facing picks
        // the clip as HKS _MoveDirectionUpdate does: |a| < 55 F, > 125 B, else L (a < 0) / R.
        a.move_vel = Vec3::ZERO;
        if let Some(td) = target_dir {
            a.yaw = yaw_of(td);
        } else if moving {
            // Free guard walk faces the camera's direction (Sekiro strafes while blocking; live:
            // the facing drifts with the camera while walking, and holds while standing - guard
            // idle's TAE SetTurnSpeed 0).
            let cam_fwd = stick_world(Vec2::new(0.0, 1.0), cam_yaw);
            a.yaw = turn_toward(a.yaw, yaw_of(cam_fwd), turn_rate(d, a, &config, false).to_radians() * dt);
        }
        let local = Quat::from_rotation_y(-a.yaw) * stick;
        let angle = local.x.atan2(-local.z).to_degrees();
        let (want, clip_angle) = if !moving {
            ("DeflectGuardIdle", angle)
        } else if angle.abs() < 55.0 {
            ("DeflectGuardMoveF", 0.0)
        } else if angle.abs() > 125.0 {
            ("DeflectGuardMoveB", 180.0)
        } else if angle < 0.0 {
            ("DeflectGuardMoveL", -90.0)
        } else {
            ("DeflectGuardMoveR", 90.0)
        };
        // Move along the stick (live: 50 deg off forward while the F clip plays).
        let off = (angle - clip_angle + 180.0).rem_euclid(360.0) - 180.0;
        a.root_yaw = (-off).to_radians();
        if moving {
            // The legs turn with it (WalkTwist), so they step along the stick.
            a.twist_target = off.to_radians();
        }
        if a.state != want || ended {
            a.play_state(d, want);
        }
        return;
    }
    if is_upper_action(&a.state) {
        // StandMoveOverwrite: the legs run the walk/run (StandMoveLower_SM carries the motion).
        lower_body_move(p, a, d, &config, stick, target_dir, pad.walk, dt);
    } else if move_ok && (moving || a.state == "Locomotion") {
        locomotion(p, a, d, &config, stick, target_dir, pad.walk, dt);
    } else if !move_ok {
        a.move_vel = Vec3::ZERO;
        p.speed_level = 0.0;
        // Turning during actions follows TAE SetTurnSpeed unless turning is disabled.
        if !a.anim.is_empty() && !d.flag(&a.anim, a.t, FLAG_DISABLE_TURN) {
            if let Some(speed) = d.turn_speed(&a.anim, a.t, lock.target.is_some()) {
                let want = target_dir.or((stick != Vec3::ZERO).then_some(stick));
                if let Some(w) = want {
                    a.yaw = turn_toward(a.yaw, yaw_of(w), speed.to_radians() * dt);
                }
            }
        }
    }
}

/// Walk/run with speeds taken from the walk (a000_0001xx) and run (a000_0004xx)
/// clips' root motion, blended by the HKS speed level.
#[allow(clippy::too_many_arguments)]
fn locomotion(
    p: &mut Player,
    a: &mut Actor,
    d: &CharData,
    config: &GameConfig,
    stick: Vec3,
    target_dir: Option<Vec3>,
    walk_held: bool,
    dt: f32,
) {
    converge_speed(p, stick, walk_held, dt);
    // Stick released: the stop anims carry their own slide (root motion), in the move direction
    // (HKS BEH_A_GROUND_MOVE_STOP: W_StandRunStop / W_StandWalkStop by MoveDirection).
    if stick == Vec3::ZERO && a.state == "Locomotion" && p.speed_level > 0.3 {
        let dir = ["F", "B", "L", "R"][a.move_dir.min(3) as usize];
        let stop = if p.speed_level > 1.2 { format!("StandRunStop{dir}") } else { format!("StandWalkStop_{dir}") };
        if a.play_state(d, &stop) || a.play_state(d, if p.speed_level > 1.2 { "StandRunStopF" } else { "StandWalkStop_F" }) {
            a.move_vel = Vec3::ZERO;
            p.speed_level = 0.0;
            return;
        }
    }
    if p.speed_level <= 0.0 && stick == Vec3::ZERO {
        a.move_vel = Vec3::ZERO;
        if a.state == "Locomotion" {
            a.play("StandIdle", "");
        }
        return;
    }
    if stick != Vec3::ZERO {
        // HKS _SpeedUpdate: MoveSpeedIndex from the stick level (walk button = walk).
        a.move_index = if walk_held { 0 } else { 1 };
    }
    // A move from a standstill or out of an action plays the move start first (W_StandMoveStart).
    let fresh = a.state != "Locomotion" || !a.anim.is_empty();
    a.procedural("Locomotion");
    if fresh {
        a.move_start = d.length(&a.move_clip(true));
    }
    move_velocity(p, a, d, config, stick, target_dir, dt);
}

/// Upper-body states (HKS STATE_TYPE_UPPER_ACTION, c0000.hkx StandMoveOverwrite): the action's
/// clip plays above the pelvis while the legs keep walking / running. The speed level converges
/// as in locomotion; letting go of the stick slows the legs to a stop under the action.
#[allow(clippy::too_many_arguments)]
fn lower_body_move(
    p: &mut Player,
    a: &mut Actor,
    d: &CharData,
    config: &GameConfig,
    stick: Vec3,
    target_dir: Option<Vec3>,
    walk_held: bool,
    dt: f32,
) {
    converge_speed(p, stick, walk_held, dt);
    if stick != Vec3::ZERO {
        a.move_index = if walk_held { 0 } else { 1 };
    }
    move_velocity(p, a, d, config, stick, target_dir, dt);
}

/// HKS GetMoveSpeed: ConvergeValue(level, speed, inc 3, dec 3), inc 6 toward level 2.
fn converge_speed(p: &mut Player, stick: Vec3, walk_held: bool, dt: f32) {
    let target_level = if stick == Vec3::ZERO { 0.0 } else if walk_held { 1.0 } else { 2.0 };
    let inc = if target_level >= 2.0 { 6.0 } else { 3.0 };
    p.speed_level = if p.speed_level < target_level {
        (p.speed_level + inc * dt).min(target_level)
    } else {
        (p.speed_level - 3.0 * dt).max(target_level)
    };
}

/// Walk/run velocity for the current speed level, turning toward the stick (free) or facing
/// the target (locked on, strafing).
fn move_velocity(p: &Player, a: &mut Actor, d: &CharData, config: &GameConfig, stick: Vec3, target_dir: Option<Vec3>, dt: f32) {
    // Locked on: the stick relative to the facing picks the directional clip; free: forward.
    let local = match target_dir {
        Some(td) => {
            a.yaw = yaw_of(td);
            Quat::from_rotation_y(-a.yaw) * stick
        }
        None => {
            if stick != Vec3::ZERO {
                a.yaw = turn_toward(a.yaw, yaw_of(stick), turn_rate(d, a, &config, target_dir.is_some()).to_radians() * dt);
            }
            Vec3::NEG_Z
        }
    };
    // Move direction class: HKS _MoveDirectionUpdate cones (|a| < 55 F, > 125 B, else L / R) with
    // the exe WalkTwist classifier's hysteresis (FUN_1407f7550: 60 deg cones -+5, so the current
    // class holds out to 65 deg / in to 115 deg) - no flicker at a cone edge.
    if target_dir.is_some() && stick != Vec3::ZERO {
        let angle = local.x.atan2(-local.z).to_degrees();
        let side = if angle < 0.0 { 2 } else { 3 };
        let (front, back) = match a.move_dir {
            0 => (65.0, 125.0),
            1 => (55.0, 115.0),
            _ => (55.0, 125.0),
        };
        a.move_dir = if angle.abs() < front { 0 } else if angle.abs() > back { 1 } else { side };
        // WalkTwist target: the move direction relative to the clip's (F 0, B 180, L -90, R 90).
        let clip_angle = [0.0, 180.0, -90.0, 90.0][a.move_dir as usize];
        a.twist_target = ((angle - clip_angle + 180.0).rem_euclid(360.0) - 180.0).to_radians();
    } else if target_dir.is_none() {
        a.move_dir = 0;
    }
    // Speed = the shown clip's root motion at its current time (Havok: the move layers'
    // MotionSelector plays the same clip with useMotion), so the planted foot stays put - the
    // move starts accelerate, the loops surge with each stride. Letting go of the stick scales it
    // down with the speed level until the stop anim / idle takes over.
    let (key, ct, looping) = a.locomotion_clip_at(a.t);
    let clip_speed = d.root_velocity_at(&key, ct, looping).length();
    let speed = if stick == Vec3::ZERO { clip_speed * p.speed_level.min(1.0) } else { clip_speed };
    let dir = if stick == Vec3::ZERO { a.forward() } else if target_dir.is_some() { stick } else { a.forward() };
    a.move_vel = dir * speed;
}
