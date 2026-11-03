//! The HKS engine surface, step 1: the `env(id, ...)` queries Wolf's behaviour scripts
//! (`extracted/hks_src/c0000*.lua`, 1,605 calls over 82 ids) ask the engine, answered from Sv1's
//! own state where Sv1 has it. Not wired into the game yet: player.rs keeps its hand-ported logic
//! until a Lua runtime runs the scripts (docs/kb/behaviour-graph.md).
//!
//! The exe answers every id in one dispatcher, FUN_140b36410(ctx, id, lua_State): ctx[0] is the
//! ChrIns; arguments are read with FUN_141010dc0 / FUN_141010dd0 (int, Lua index 2 = the first
//! argument after the id). Answers are ints: floats come back x1000 (or x100, x57.29578 for
//! angles), booleans 0 / 1. The modules named below are Wolf's module bag (chr +0x1ff8; class names
//! from RTTI on the 2026-11-03 snapshot): [+0x0] SprjChrActionFlagModule ("flags"), [+0x18]
//! SprjChrDataModule, [+0x50] SprjChrTalkModule, [+0x68] SprjChrPhysicsModule, [+0x70]
//! SprjPlayerFallModule, [+0x80] SprjChrActionRequestModule ("request"), [+0x88]
//! SprjChrThrowModule, [+0x98] SprjPlayerDamageModule, [+0xa8] SprjPlayerKnockBackModule,
//! [+0x128] CSChrDockingModule, [+0x130] CSWireActionModule; chr +0x50 is PlayerCtrl (its +0x50
//! block holds the TAE action flags, "ctrl flags") and chr +0x11d0 SpecialEffect (the SpEffect
//! list: entries from +8, next +0x70, id +0x58, SpEffectParam +0x60, valid when +0x68 > 4).
//!
//! Each id below: the exe case, what it reads (offsets checked by poking them in
//! tools/emu-cases `hks_env`: 30 pokes, each moved the answer as written), and Sv1's source
//! or `gap:`. `HksEnv::env` returns None where Sv1 has no such state; the test
//! `hks_env_matches_the_exe` compares the rest with the exe on Wolf standing idle.
//!
//! Step 2 (tools/emu-cases `hks2`): `HksEnv::fields` mirrors the exe fields by module and offset;
//! `env` is each case's rule on them and `act` what each act case writes. The rules are checked in
//! 223 poses (the fields written on the snapshot's Wolf, test `hks_env_poses_match_the_exe`) and
//! the acts by a byte diff of Wolf's modules (test `hks_act_matches_the_exe`). A `gap:` on an id
//! below whose rule `env` has means Sv1 keeps no state for its field yet (`from_world` leaves it
//! out); the ids without a rule in `env` are the ones still not ported.

// Not wired yet: only the sim test calls it (the id constants document the exe).
#![allow(dead_code)]

use bevy::prelude::*;

use crate::actor::Actor;
use crate::data::Combat;
use crate::player::{Action, Player};

/// Case 0x69: PlayerCtrl +0x50 block +0x4f8 + 4 x arg (event command id per slot; -1 none).
/// gap: Sv1 has no event commands.
pub const GET_COMMAND_ID_FROM_EVENT: i32 = 105;
/// Case 0x71, player only: the selected quick item (FUN_1407ee050) usable (FUN_1409f3890) and the
/// item menu (chr vtable +0x310 object) slot 0 accepts it. gap: no item menu.
pub const IS_ITEM_USE_MENU_OPEN: i32 = 113;
/// Case 0x73: the item menu object (chr vtable +0x310) slot +8. gap: no item menu.
pub const IS_ITEM_USE_MENU_OPENED: i32 = 115;
/// Case 200: FUN_1409f3190(chr). gap: not traced.
pub const IS_FALLING: i32 = 200;
/// Case 0xc9: FUN_1409f3380: physics +0xa2 set and FUN_140bb14d0(physics +0x1c0) false (on the
/// ground). Sv1: not `Actor::airborne` (approx).
pub const IS_LANDING: i32 = 201;
/// Case 0xca: flags +0x5c (the last hit's damage type; 99999 none). gap: Sv1 keeps no hit record.
pub const GET_RECEIVED_DAMAGE_TYPE: i32 = 202;
/// Case 0xcd: in a throw (throw +0x10 entry, its +0x18 type != 0) and flags +0x70 bit 4.
/// Sv1: 0 out of a throw (`actor::in_throw`); gap in one.
pub const IS_NORMAL_DMG_PASS_THROUGH_DURING_THROW: i32 = 205;
/// Case 0xce: FUN_140be0930(throw module): the throw +0x10 entry's +0x18 type != 0. Sv1: 0 out of
/// a throw; gap in one.
pub const IS_THROWN: i32 = 206;
/// Case 0xcf: the int at [chr +0x120] (weapon switch state). gap.
pub const GET_WEAPON_SWITCH_TYPE: i32 = 207;
/// Case 0xde: flags +0x68 (hit direction): 0 / 7 -> 2, 1 / 2 -> 1, 3 / 4 -> 3, else 0. gap.
pub const GET_RECEIVED_DAMAGE_DIRECTION: i32 = 222;
/// Case 0xe0: fall module (bag +0x70) FUN_140b97880, capped at 59 when FUN_140a58a70, x100.
/// gap: Sv1 has `Player::air_top` but not the module's rule.
pub const GET_FALL_HEIGHT: i32 = 224;
/// Case 0xe1: FUN_1409f1b00(chr, hand): the int array at [chr +0x120], hand 0 -> [1], else [hand
/// + 1] when [0] == 1. gap: save-dependent equipment state.
pub const GET_EQUIP_WEAPON_CATEGORY: i32 = 225;
/// Case 0xe7: FUN_1409efba0(chr) (255 idle). gap: no item anims.
pub const GET_ITEM_ANIM_TYPE: i32 = 231;
/// Case 0xe9: FUN_1409f3890(chr, selected quick item, 0). gap.
pub const IS_ITEM_USEABLE: i32 = 233;
/// Case 0xec: flags +0x38 (the last hit's damage level). gap: Sv1 keeps it only for a killing hit
/// (`Actor::death_hit`).
pub const GET_DAMAGE_LEVEL: i32 = 236;
/// Case 0xed: FUN_140843820(flags +0x38, flags +0x3c). gap.
pub const GET_GUARD_LEVEL_ACTION: i32 = 237;
/// Case 0xf8: physics +0x929 set and +0xa48 clear. Sv1: not `Actor::airborne` (approx).
pub const IS_TRULY_LANDING: i32 = 248;
/// Case 0x100: flags +0x54 bit 2. gap.
pub const HAS_RECEIVED_ANY_DAMAGE: i32 = 256;
/// Case 0x111: FUN_140bdfb20(throw module, 0, 1): the throw +0x10 entry's anim by its type (+0x18),
/// -1 with none. Sv1: -1 out of a throw; gap in one (the anim id).
pub const GET_THROW_ANIM_ID: i32 = 273;
/// Case 0x112: FUN_140b39ce0(ctx, chr). Sv1: 0 out of a throw; gap in one.
pub const IS_OPPONENT_DEAD_FROM_THROW: i32 = 274;
/// Case 0x114: FUN_140b39dc0(ctx, chr). Sv1: 0 out of a throw; gap in one (player.rs
/// FLAG_THROW_TYPE5 stands in).
pub const IS_THROW_SELF_DEATH: i32 = 276;
/// Case 0x115: FUN_140b39e90(ctx, chr). Sv1: 0 out of a throw; gap in one.
pub const IS_THROW_SUCCESS: i32 = 277;
/// Case 0x11d: flags +0x4c (the last hit's special attribute). gap.
pub const GET_SPECIAL_ATTRIBUTE: i32 = 285;
/// Case 0x14d: flags +0xaf8 (the frame's dt, s) x 1000. Sv1: the actor's clock step (1/60).
pub const GET_DT: i32 = 333;
/// Case 0x14e: flags +0x40 == arg or flags +0x44 == arg. gap.
pub const GET_BEHAVIOR_ID: i32 = 334;
/// Case 0x151: FUN_140be0170 and FUN_140be0930 on the throw module (a throw running whose
/// FUN_1408029b0 is false). Sv1: 0 out of a throw; gap in one.
pub const IS_THROW_POS_REALIGN: i32 = 337;
/// Case 0x153: flags +0xaf0 bit arg (u64). gap: which clip ends set which bit is not traced.
pub const IS_ANIM_END: i32 = 339;
/// Case 0x159: chr vtable +0x340(hand) -> EquipParamWeapon, its u16 +0xea. gap: save-dependent.
pub const GET_EQUIP_WEAPON_SPECIAL_CATEGORY_NUMBER: i32 = 345;
/// Case 0x15d: ctrl flags +0x8a bit 0. gap.
pub const IS_DAMAGE_MOTION_OFF: i32 = 349;
/// Case 1000: data module (bag +0x18) +0x130 (HP). Sv1: `Actor::hp`.
pub const GET_HP: i32 = 1000;
/// Case 0x3ef: FUN_1409efec0(chr) == 6. Sv1: 0 (Wolf).
pub const IS_COM_PLAYER: i32 = 1007;
/// Case 0x451: flags +0x10 bit 0 (the stay state: the request module then answers from its
/// global mask). Sv1: Wolf in StandIdle / CrouchIdle (approx: which exe states set it is not
/// traced).
pub const IS_STAY_STATE: i32 = 1105;
/// Case 0x452: request module FUN_140b2b6d0(m, 1, arm). Wolf's module has +0x1b8 set, so
/// FUN_140b2cd10: in the stay state the mask at [+0x40] +0x20 (bit arm), else the entry of the
/// current state id (+0xc8) in the list [+0x50, +0x58) of {id, pad, u64 mask}. (+0x1b8 clear:
/// +0x20 or +0x30 by the stay bit.) Sv1: `Player::requests` (buffered presses, by ACTION_ARM).
pub const ACTION_REQUEST: i32 = 1106;
/// Case 0x454: request module +0xe8 + 4 x arm (seconds held, arm < 0x21) x 1000. gap: Sv1 keeps
/// held flags (`PadInput`), not hold times.
pub const ACTION_DURATION: i32 = 1108;
/// Case 0x458: not FUN_140be0890(throw): 0 while the throw +0x10 entry has type 0 (idle), 1
/// otherwise. gap.
pub const HAS_THROW_REQUEST: i32 = 1112;
/// Case 0x45c: FUN_1409f1c80: an entry with id arg in the SpEffect list. Sv1: the SpEffects
/// `HksEnv::sp_effects` gathers.
pub const GET_SP_EFFECT_ID: i32 = 1116;
/// Case 0x45e: chr +0x1070 (lock-on). Sv1: `camera::LockOn::target`.
pub const IS_LOCKED_ON: i32 = 1118;
/// Case 0x45f: damage module (bag +0x98) +0x60. gap.
pub const GET_ATK_DIRECTION: i32 = 1119;
/// Case 0x461: knockback module (bag +0xa8) float +0x10 as int. gap.
pub const GET_KNOCKBACK_DISTANCE: i32 = 1121;
/// Case 2000: ctrl flags +0x88 bit 2 and +0x9c bit 4 (both TAE action flags). gap: which TAE
/// flags they are is not traced (0 standing idle).
pub const IS_MOVE_CANCEL_POSSIBLE: i32 = 2000;
/// Case 0x7d4: flags +0x58 bit 4 (an attack connected; set in FUN_1409e5fd0). Sv1:
/// `Actor::attack_hit`.
pub const IS_ATK_CONTACT_TARGET: i32 = 2004;
/// Case 3000: FUN_1409f3030: the player, wire module (bag +0x130) not busy (+0x10 clear or +0x11
/// set) and [0x143d8e058] +8 == 1 (a wire target in reach). gap: wire.rs has its own search.
pub const IS_WIRE_POSSIBLE: i32 = 3000;
/// Case 0xbbb: physics +0x164 (vertical speed, m/s) x 1000. Sv1: `Actor::vel_y` in the air; on the
/// ground one frame of gravity, -WORLD_GRAVITY x dt (fitted: standing idle the exe gives -163 =
/// -9.8 / 60, so `env(3003) < 0` holds on the ground; the physics rule behind it is not traced).
pub const GET_VERTICAL_SPEED: i32 = 3003;
/// Case 0xbc0: [0x143d7a388] +0x6c (debug dash). Sv1: 0.
pub const IS_DEBUG_DASH: i32 = 3008;
/// Case 0xbc3: docking module FUN_140b7d3a0. gap.
pub const IS_ON_EDGE_OF_DOCKING_TARGET: i32 = 3011;
/// Case 0xbc9: docking module FUN_140b7d230 (signed byte, -1 none). gap.
pub const GET_DOCKING_EDGE_REQUEST: i32 = 3017;
/// Case 0xbca: flags +0x60 (i16). gap.
pub const GET_GUARD_BOUNCE_BEHAVIOR: i32 = 3018;
/// Case 0xbcb: flags +0x62 (i16). gap.
pub const GET_JUST_GUARD_BOUNCE_BEHAVIOR: i32 = 3019;
/// Case 0xbcc: physics +0xab5 set, or +0xab4 set. gap.
pub const IS_WALL_JUMP_CONTACT: i32 = 3020;
/// Case 0xbd1: FUN_1409f2ae0(chr). gap: no water.
pub const IS_SWIM_CONTACT: i32 = 3025;
/// Case 0xbd3: chr vtable +0x420(hand) -> the next slot's EquipParamWeapon byte +0xe3 (-1 none).
/// gap: save-dependent.
pub const GET_NEXT_WEAPON_TYPE: i32 = 3027;
/// Case 0xbd4: FUN_1409e5c00(chr +0x18). gap.
pub const IS_TRUE_DEATH: i32 = 3028;
/// Case 0xbd5: docking module FUN_140b7d710. gap.
pub const IS_DOCKING_EDGE_INVALID: i32 = 3029;
/// Case 0xbd7: throw module byte +0x63. Sv1: 0 out of a throw; gap in one.
pub const IS_THROW_ACTIVE: i32 = 3031;
/// Case 0xbd8: flags +0x68 (hit direction): 0, 1, 6, 7 -> 0 (front), else 1. gap.
pub const GET_DAMAGE_DIRECTION: i32 = 3032;
/// Case 0xbd9: PlayerGameData (chr vtable +0x1b8) +0x278 -> FUN_1407a7fe0(type). gap:
/// save-dependent (Sv1's rule is combat::action_unlocked with config player.skills).
pub const ACTION_UNLOCKED: i32 = 3033;
/// Case 0xbdb: chr vtable +0x2b0(arm). gap.
pub const PC_REQUEST_TYPE: i32 = 3035;
/// Case 0xbdc: the SpEffect list: an entry whose SpEffectParam +0x3c0 (behaviorRefId) == arg,
/// counted when param +0x326 is 0 or entry +0x27 is set. Sv1: `HksEnv::sp_effects` (gap: the
/// +0x326 / +0x27 lifetime clause; every listed SpEffect counts).
pub const GET_BEHAVIOR_REF_ID: i32 = 3036;
/// Case 0xbdd: physics +0x93a. gap (1 standing).
pub const IS_CROUCH_EXIT_POSSIBLE: i32 = 3037;
/// Case 0xbde: docking FUN_140b7d290 == 4 and not FUN_140b7d480(d, 0). gap.
pub const IS_DIVE_ALLOWED: i32 = 3038;
/// Case 0xbdf: docking FUN_140b7d290 == 4 and FUN_140b7d480(d, 1). gap.
pub const IS_DIVE_TO_SWIM_ALLOWED: i32 = 3039;
/// Case 0xbe0: chr +0x1acd & 0xf. gap.
pub const GET_MAP_VISIBILITY_TYPE: i32 = 3040;
/// Case 0xbe3: docking FUN_140b7d440(d, 1). gap.
pub const IS_WALL_HANG_POSSIBLE: i32 = 3043;
/// Case 0xbe4: docking FUN_140b7d440(d, 2). gap.
pub const IS_WALL_HANG_GROUND_POSSIBLE: i32 = 3044;
/// Case 0xbe5: docking FUN_140b7d440(d, 0). gap.
pub const IS_WALL_HANG_AIR_POSSIBLE: i32 = 3045;
/// Case 0xbe6: docking FUN_140b7d260(d, arg 0 -> 1, 1 -> 0), -1 otherwise. gap.
pub const GET_DOCKING_EDGE_VARIATION_ID: i32 = 3046;
/// Case 0xbe8: docking FUN_140b7d740 == 1. gap.
pub const IS_HANG_LEFT_OUTTER_CORNER_POSSIBLE: i32 = 3048;
/// Case 0xbe9: docking FUN_140b7d740 == 2. gap.
pub const IS_HANG_RIGHT_OUTTER_CORNER_POSSIBLE: i32 = 3049;
/// Case 0xbea: docking FUN_140b7d480(d, 0xb). gap.
pub const IS_HANG_AND_CLIMB_POSSIBLE: i32 = 3050;
/// Case 0xbeb: docking +0x84c. gap.
pub const IS_OBSTACLE_AT_DOCKING_START: i32 = 3051;
/// Case 0xbec: docking +0x84d. gap.
pub const IS_OBSTACLE_AT_DOCKING_END: i32 = 3052;
/// Case 0xbed: the talk param of [FUN_140b35960 singleton] +0x20f4 (-1 none). gap.
pub const GET_TALK_PARAM_REF_ID: i32 = 3053;
/// Case 0xbee: talk module (bag +0x50) +0x18 (-1 none). gap.
pub const GET_TALK_EZ_STATE_REF_ID: i32 = 3054;
/// Case 0xbef: player only, chr +0x2008 (-1 otherwise). gap.
pub const GET_SAVE_STATE: i32 = 3055;
/// Case 0xbf0: flags +0x64 (i16). gap.
pub const GET_GUARD_DEFLECT_BEHAVIOR: i32 = 3056;
/// Case 0xbf1: flags +0x66 (i16). gap.
pub const GET_JUST_GUARD_DEFLECT_BEHAVIOR: i32 = 3057;
/// Case 0xbf2: talk module +0x28. gap.
pub const LIP_SYNC_REQUEST: i32 = 3058;
/// Case 0xbf3: docking FUN_140b7d6d0 == 1. gap.
pub const IS_HANG_LEFT_INNER_CORNER_POSSIBLE: i32 = 3059;
/// Case 0xbf4: docking FUN_140b7d6d0 == 2. gap.
pub const IS_HANG_RIGHT_INNER_CORNER_POSSIBLE: i32 = 3060;
/// Case 0xbf5: player only, chr +0x200c (-1 otherwise). gap.
pub const GET_SAFE_POSITION_RETURN_STATE: i32 = 3061;
/// Case 0xbf7: flags +0x3c8 entries {int id, float value, int} (count +0x9d0): value x 1000 of
/// the entry with id arg, 0 none. gap: the TAE event that fills it is not traced.
pub const GET_VARIABLE_CHANGE_VALUE: i32 = 3063;
/// Case 0xbf8, the string id "特殊効果発動中か_Behavior参照ID_寿命延長を厳密に取得" (the exe's name
/// map [0x143d85568], names cut to 31 UTF-16 units): like 3036, but every matching entry needs
/// param +0x16b bit 7 and (FUN_140c064a0(entry) false or ctrl flags +0x8c bit 6). Sv1: the 3036
/// rule (approx).
pub const GET_BEHAVIOR_REF_ID_STRICT: i32 = 3064;

/// The `act(id, ...)` ids (exe dispatcher FUN_140b32410(ctx, id, lua_State), the same argument
/// readers) whose writes land in the fields the env ids read or in Wolf's modules; what each writes
/// was run in tools/emu-cases `hks2_act` (a byte diff of Wolf's modules around each call, test
/// `hks_act_matches_the_exe`). The other act ids Wolf's scripts call write nothing in those
/// modules on idle Wolf: menus (123, 127, 147), HUD (3030), AI notices (155), the throw anim (135,
/// 136, 139: no throw running), the lock-on homing (2019, 3019: not locked on), the auto-homing
/// module (156, 157), items (2024), 3004 (no lock target), 3041, 9000 (debug print), 148
/// (SetHavokVariable, not run).
/// Case 0x65: PlayerCtrl +0x258 bit 0 = bool (FUN_1407d74d0).
pub const ACT_SWITCH_MOTION: i32 = 101;
/// Case 0x8a: flags +0x10 bit 28.
pub const ACT_SET_IS_EVENT_ACTION_POSSIBLE: i32 = 138;
/// Case 0x8d: ctrl flags +0x51d = type; knockback module +0x44 = 0x101 (u16).
pub const ACT_SET_DAMAGE_ANIM_TYPE: i32 = 141;
/// Case 0x96: flags +0x10 bit 14.
pub const ACT_SET_IS_ITEM_ANIMATION_PLAYING: i32 = 150;
/// Case 0x9a: chr +0x11cc = hand.
pub const ACT_WEAPON_PARAMETER_REFERENCE: i32 = 154;
/// Case 0x9f: damage module FUN_140b65680: the hit direction (damage +0x50) normalised in place,
/// physics yaw = atan2(x, z) + deg (rad), wrapped to [-pi, pi] (FUN_1407de0c0).
pub const ACT_TURN_TOWARD_ATTACKER: i32 = 159;
/// Case 0xa0 / 0xa1: throw module +0x79 / +0x7b = state.
pub const ACT_SET_THROW_POSSIBILITY_STATE_ATTACKER: i32 = 160;
pub const ACT_SET_THROW_POSSIBILITY_STATE_DEFENDER: i32 = 161;
/// Case 0x7d2: FUN_1409e9c00(chr, id, chr): the SpEffect on Wolf (GetSpEffectID answers 1 after).
pub const ACT_ADD_SP_EFFECT: i32 = 2002;
/// Case 0x7df: BehaviorTurn ([PlayerCtrl +0x250]) +0x78 / +0x7c = a, b degrees in radians.
pub const ACT_LOCKON_SYSTEM_UNABLE_TO_TURN_ANGLE: i32 = 2015;
/// Case 0x7e2: flags +0x10 bit 16.
pub const ACT_LOCKON_FIXED_ANGLE_CANCEL: i32 = 2018;
/// Case 0xbc3: flags +0x10 bit 20; PlayerCtrl +0x350 = 2.
pub const ACT_UNDERWATER: i32 = 3011;
/// Case 0xbc8: flags +0x20 bit i.
pub const ACT_SET_FALL_PREVENTION_ASSIST: i32 = 3016;
/// Case 0xbca: flags +0x10 bit 21, and bit 22 for 0. gap: the physics homing (+0x248, its
/// target pointer +0x288 moved) to the wire point.
pub const ACT_WIRE_ACTION: i32 = 3018;
/// Case 0xbcf: docking FUN_140b7d920: the anim of the type's slot, docking +0x7bc + 12 x type.
/// gap: physics +0x3c0 (the anim) and +0x3d0 / +0x3e0 (fractions from the anim's root motion) for
/// types 2-5; the wire hit-change module.
pub const ACT_SET_MOVE_PRECALCULATION: i32 = 3023;
/// Case 0xbd1: physics yaw += deg (rad), wrapped to [-pi, pi] (FUN_1407dcce0 -> FUN_1407de0c0).
pub const ACT_TURN_TO_DIRECTION: i32 = 3025;
/// Case 0xbd2 / 0xbd3 / 0xbd4: docking FUN_140b7d810(d, 1 / 2 / 0): the judgment request byte
/// docking +0x8a0 (wall cover) / +0x8a1 (ground hang) / +0x8a2 (air hang) = 1.
pub const ACT_WALL_COVER_START_JUDGMENT: i32 = 3026;
pub const ACT_GROUND_HANG_START_JUDGMENT: i32 = 3027;
pub const ACT_AIR_HANG_START_JUDGMENT: i32 = 3028;
/// Case 0xbd5: talk module +0x14 = ref.
pub const ACT_SET_TALK_ANIM_STATE: i32 = 3029;
/// Case 0xbd8: flags +0x10 bit 26.
pub const ACT_ADHESION_DISABLED: i32 = 3032;
/// Case 0xbda: FUN_140b322f0: flags +0x28 bit type.
pub const ACT_ACTION_BUTTON_EXECUTABLE: i32 = 3034;
/// Case 0xbdb / 0xbdc / 0xbdd: flags +0x10 bit 29 / 30 / 31.
pub const ACT_WIRE_EDGES_PRIORITIZE: i32 = 3035;
pub const ACT_DISABLE_FLOOR_ADHESION_DURING_FAST_MOVEMENT: i32 = 3036;
pub const ACT_ON_WATER: i32 = 3037;
/// Case 0x238c: flags +0x10 bit 0 (the stay bit IsStayState reads).
pub const ACT_WAIT: i32 = 9100;
/// Case 0x238d: request module +0x188 bit 0 (FUN_140b2afd0, the flags list reset); FdpDataExModule
/// (bag +0xc0) +0x11 = 0. Sv1: the buffered presses cleared.
pub const ACT_RESET_INPUT_QUEUE: i32 = 9101;
/// Case 0x238e: flags +0x10 bit 1.
pub const ACT_SET_EVENT_ANIM_IN_PROGRESS: i32 = 9102;
/// Case 0x238f: flags +0x14 = v.
pub const ACT_SET_AI_ATTACK_STATE: i32 = 9103;

/// HKS ACTION_ARM ids (c0000_define.lua) of Sv1's buffered actions.
pub fn action_arm(a: Action) -> i32 {
    match a {
        Action::Attack => 0,
        Action::Prosthetic => 1,
        Action::Guard => 2,
        Action::Grapple => 3,
        Action::Jump => 4,
        Action::Step => 5,
        Action::UseItem => 7,
        Action::SwitchTool => 10,
        Action::Crouch => 15,
        Action::CombatArt => 32,
    }
}

/// GetGuardLevelAction (case 0xed): FUN_140843820(level, guard) looks up a 12 x 5 table (level
/// flags +0x38 < 12, guard flags +0x3c < 5; 0 outside). The values are the exe's answers for all
/// 60 pairs (tools/emu-cases `hks2_env`), rows by damage level, columns by guard level.
const GUARD_LEVEL_ACTION: [[i32; 5]; 12] = [
    [0, 0, 0, 0, 0],
    [0, 1, 1, 1, 1],
    [0, 3, 1, 1, 1],
    [0, 3, 3, 3, 1],
    [0, 4, 3, 3, 3],
    [0, 1, 1, 1, 1],
    [0, 3, 3, 3, 3],
    [0, 3, 3, 3, 3],
    [0, 0, 0, 0, 0],
    [0, 3, 3, 3, 3],
    [0, 4, 4, 4, 4],
    [0, 3, 3, 3, 3],
];

/// The fall drop FUN_140bc2210 calls deadly ([0x143b09efc], a debug variable registered by
/// FUN_140795380 with range 0..10000; 1500 in the 2026-11-03 snapshot). GetFallHeight's 59 m cap
/// applies to a Wolf who can't die only below it.
pub const FALL_DEATH_DROP: f32 = 1500.0;

/// What Sv1 knows of Wolf that the env ids read, gathered once per frame.
#[derive(Clone, Debug, Default)]
pub struct HksEnv {
    /// SpEffects on Wolf: (id, behaviorRefId). TAE AddSpEffect events of the clip, resident ids,
    /// `Player::held` / `timed`, `Actor::sp_chained`, the learned skills' SpEffectParam rows and the
    /// equipped weapons' residentSpEffectId (the combat art's virtual weapon and the selected
    /// prosthetic: the exe's Wolf wears 140701 of art weapon 7400 and 127300 of Loaded Axe 73000).
    pub sp_effects: Vec<(i64, i64)>,
    /// Buffered presses as ACTION_ARM ids (`Player::requests`).
    pub requests: Vec<i32>,
    /// The exe fields the handlers read, by module and offset ("flags+0x68": SprjChrActionFlagModule
    /// +0x68; the names of tools/emu-cases `hks2.rs`): "flags", "data", "talk", "phys", "request",
    /// "throw", "damage", "knockback", "fdp", "docking", "wire" (Wolf's module bag), "chr" (the
    /// ChrIns), "ctrl" (PlayerCtrl), "ctrlflags" (PlayerCtrl +0x50 block), "turn" (BehaviorTurn,
    /// PlayerCtrl +0x250), "weapons" ([chr +0x120]), "throw_entry" ([throw +0x10]), "life" ([[chr
    /// +0x18] +0x30]), "wire_target" ([0x143d8e058]), "debug" (DebugFlags 0x143d7a366); "var[id]"
    /// the flags +0x3c8 GetVariableChangeValue entries, "pos_y" the physics position's y. A field
    /// Sv1 keeps no state for is absent (`gap:`), and the ids that need it answer None.
    pub fields: std::collections::HashMap<String, f64>,
}

/// behaviorRefId of any SpEffectParam row: the combat export, else the full dump
/// (extracted/json/params/SpEffectParam.json; the export leaves out e.g. 127300).
fn behavior_ref(combat: &Combat, id: i64) -> i64 {
    if let Some(r) = combat.param("SpEffectParam", id)["behaviorRefId"].as_i64() {
        return r;
    }
    static ALL: std::sync::OnceLock<std::collections::HashMap<i64, i64>> = std::sync::OnceLock::new();
    let all = ALL.get_or_init(|| {
        let path = crate::paths::extracted().join("json/params/SpEffectParam.json");
        let v: serde_json::Value = std::fs::read_to_string(path).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        v["rows"].as_array().into_iter().flatten().filter_map(|r| Some((r["id"].as_i64()?, r["behaviorRefId"].as_i64()?))).collect()
    });
    all.get(&id).copied().unwrap_or(0)
}

impl HksEnv {
    /// Wolf's state in the sim world.
    pub fn from_world(world: &mut World) -> HksEnv {
        let locked_on = world.get_resource::<crate::camera::LockOn>().is_some_and(|l| l.target.is_some());
        let immortal = world.get_resource::<crate::enemy::script::WolfImmortal>().is_some_and(|w| w.0);
        let mut q = world.query::<(&Player, &Actor, &Transform)>();
        let (p, a, tf) = q.single(world).expect("one Wolf");
        let combat = world.resource::<Combat>();
        let config = world.resource::<crate::config::GameConfig>();
        let d = combat.data_of(a);
        let mut ids: Vec<i64> = Vec::new();
        if !a.anim.is_empty() {
            ids.extend(d.sp_effects_at(&a.anim, a.step()).iter().map(|(id, _)| *id));
        }
        ids.extend(a.resident.iter().copied());
        ids.extend(p.held.iter().copied());
        ids.extend(p.timed.iter().map(|(id, _)| *id));
        ids.extend(a.sp_chained.iter().map(|(id, _)| *id));
        for &skill in &config.player.skills {
            let row = combat.param("SkillParam", skill);
            ids.extend(["spEffect1", "spEffect2", "spEffect3"].iter().filter_map(|f| row[*f].as_i64()).filter(|&i| i > 0));
        }
        let tool = config.player.prosthetics.get(p.tool_slot).copied();
        for w in std::iter::once(config.player.combat_art).chain(tool) {
            let row = combat.param("EquipParamWeapon", w);
            ids.extend(["residentSpEffectId", "residentSpEffectId1", "residentSpEffectId2"].iter().filter_map(|f| row[*f].as_i64()).filter(|&i| i > 0));
        }
        ids.sort_unstable();
        ids.dedup();
        let sp_effects = ids.iter().map(|&id| (id, behavior_ref(combat, id))).collect();
        let mut requests: Vec<i32> = p.requests.keys().map(|&k| action_arm(k)).collect();
        requests.sort_unstable();
        let dt = a.clock_dt.unwrap_or(1.0 / 60.0);
        let ground = !a.airborne as i32 as f64;
        let mut fields: std::collections::HashMap<String, f64> = [
            ("data+0x130", a.hp as f64),
            // the frame's dt (flags +0xaf8)
            ("flags+0xaf8", dt as f64),
            // the stay bit (approx: which exe states set it is not traced)
            ("flags+0x10", matches!(a.state.as_str(), "StandIdle" | "CrouchIdle") as i32 as f64),
            ("flags+0x58", if a.attack_hit { 16.0 } else { 0.0 }),
            ("chr+0x1070", locked_on as i32 as f64),
            // physics +0xa2 / +0x929 on the ground, +0xa48 clear (approx: not `Actor::airborne`);
            // +0x164 the vertical speed: on the ground one frame of gravity, -9.8 x dt (the
            // 2026-11-03 snapshot, dt 0.0455: -0.446, as on the first one at 1/60: -0.163).
            ("phys+0xa2", ground),
            ("phys+0x929", ground),
            ("phys+0xa48", 0.0),
            ("phys+0x164", if a.airborne { a.vel_y as f64 } else { (-crate::data::WORLD_GRAVITY * dt) as f64 }),
            // GetFallHeight: the fall's top (`Player::air_top`) over the feet.
            ("phys+0x854", p.air_top as f64),
            ("pos_y", tf.translation.y as f64),
            // can't die: Set Character Immortality (approx: that it is data +0x228 bit 2 is
            // inferred, not traced); Sv1 has no no-death debug flags.
            ("data+0x228", if immortal { 4.0 } else { 0.0 }),
            ("debug+0x0", 0.0),
            ("debug+0xb", 0.0),
            ("fall_death_drop", FALL_DEATH_DROP as f64),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect();
        // Out of a throw the throw module's entry has type 0; in one Sv1 has no type (gap).
        if !crate::actor::in_throw(a) {
            fields.insert("throw_entry+0x18".into(), 0.0);
        }
        HksEnv { sp_effects, requests, fields }
    }

    fn f(&self, name: &str) -> Option<f64> {
        self.fields.get(name).copied()
    }
    fn i(&self, name: &str) -> Option<i64> {
        self.f(name).map(|v| v as i64)
    }

    /// The exe's answer to `env(id, args...)` from the fields (FUN_140b36410's case for the id);
    /// None where a field it needs is absent (the `gap:` ids above). `args` are the script's
    /// arguments after the id.
    pub fn env(&self, id: i32, args: &[i64]) -> Option<i32> {
        let arg = |i: usize| args.get(i).copied().unwrap_or(0);
        let b = |v: bool| Some(v as i32);
        // floats come back as (int)(x * 1000.0f)
        let milli = |v: f64| ((v as f32) * 1000.0) as i32;
        let bit = |name: &str, n: u32| self.i(name).map(|v| (v >> n) & 1 != 0);
        let in_throw = || self.i("throw_entry+0x18").map(|t| t != 0);
        match id {
            GET_BEHAVIOR_REF_ID | GET_BEHAVIOR_REF_ID_STRICT => b(self.sp_effects.iter().any(|&(_, r)| r != 0 && r == arg(0))),
            GET_SP_EFFECT_ID => b(self.sp_effects.iter().any(|&(i, _)| i == arg(0))),
            ACTION_REQUEST => b(self.requests.contains(&(arg(0) as i32))),
            IS_LOCKED_ON => b(self.i("chr+0x1070")? != 0),
            IS_STAY_STATE => b(bit("flags+0x10", 0)?),
            GET_HP => Some(self.i("data+0x130")? as i32),
            GET_DT => Some(milli(self.f("flags+0xaf8")?)),
            GET_VERTICAL_SPEED => Some(milli(self.f("phys+0x164")?)),
            // gap: FUN_140bb14d0(physics +0x1c0) taken false.
            IS_LANDING => b(self.i("phys+0xa2")? != 0),
            IS_TRULY_LANDING => b(self.i("phys+0x929")? != 0 && self.i("phys+0xa48")? == 0),
            // FUN_140b97970: not on the ground, then (FUN_140bb15c0 (gap: taken false) or physics
            // +0x100 clear or +0x900 >= 1) and ctrl flags +0x88 bit 5 clear.
            IS_FALLING => {
                if self.i("phys+0xa2")? != 0 {
                    return Some(0);
                }
                let free = self.i("phys+0x100")? == 0 || self.f("phys+0x900")? as f32 >= 1.0;
                b(free && !bit("ctrlflags+0x88", 5)?)
            }
            IS_ATK_CONTACT_TARGET => b(bit("flags+0x58", 4)?),
            IS_COM_PLAYER | IS_DEBUG_DASH => Some(0),
            GET_COMMAND_ID_FROM_EVENT => Some(self.i(&format!("ctrlflags+{:#x}", 0x4f8 + 4 * arg(0)))? as i32),
            GET_RECEIVED_DAMAGE_TYPE => Some(self.i("flags+0x5c")? as i32),
            GET_DAMAGE_LEVEL => Some(self.i("flags+0x38")? as i32),
            GET_SPECIAL_ATTRIBUTE => Some(self.i("flags+0x4c")? as i32),
            HAS_RECEIVED_ANY_DAMAGE => b(bit("flags+0x54", 2)?),
            GET_BEHAVIOR_ID => b(self.i("flags+0x40")? == arg(0) || self.i("flags+0x44")? == arg(0)),
            IS_ANIM_END => b(bit("flags+0xaf0", arg(0) as u32)?),
            GET_GUARD_BOUNCE_BEHAVIOR => Some(self.i("flags+0x60")? as i32),
            GET_JUST_GUARD_BOUNCE_BEHAVIOR => Some(self.i("flags+0x62")? as i32),
            GET_GUARD_DEFLECT_BEHAVIOR => Some(self.i("flags+0x64")? as i32),
            GET_JUST_GUARD_DEFLECT_BEHAVIOR => Some(self.i("flags+0x66")? as i32),
            GET_DAMAGE_DIRECTION => Some(!matches!(self.i("flags+0x68")?, 0 | 1 | 6 | 7) as i32),
            GET_RECEIVED_DAMAGE_DIRECTION => Some(match self.i("flags+0x68")? {
                0 | 7 => 2,
                1 | 2 => 1,
                3 | 4 => 3,
                _ => 0,
            }),
            GET_GUARD_LEVEL_ACTION => {
                let (lvl, guard) = (self.i("flags+0x38")?, self.i("flags+0x3c")?);
                Some(if (0..12).contains(&lvl) && (0..5).contains(&guard) { GUARD_LEVEL_ACTION[lvl as usize][guard as usize] } else { 0 })
            }
            IS_MOVE_CANCEL_POSSIBLE => b(bit("ctrlflags+0x88", 2)? && bit("ctrlflags+0x9c", 4)?),
            IS_DAMAGE_MOTION_OFF => b(bit("ctrlflags+0x88", 16)?),
            ACTION_DURATION => {
                if !(0..0x21).contains(&arg(0)) {
                    return Some(0);
                }
                Some(milli(self.f(&format!("request+{:#x}", 0xe8 + 4 * arg(0)))?))
            }
            GET_VARIABLE_CHANGE_VALUE => {
                // the list is known when its count is
                self.f("flags+0x9d0")?;
                Some(self.f(&format!("var[{}]", arg(0))).map(milli).unwrap_or(0))
            }
            GET_WEAPON_SWITCH_TYPE => Some(self.i("weapons+0x0")? as i32),
            GET_ATK_DIRECTION => Some(self.i("damage+0x60")? as i32),
            GET_KNOCKBACK_DISTANCE => Some(self.f("knockback+0x10")? as f32 as i32),
            GET_TALK_EZ_STATE_REF_ID => Some(self.i("talk+0x18")? as i32),
            LIP_SYNC_REQUEST => Some(self.i("talk+0x28")? as i32),
            GET_SAVE_STATE => Some(self.i("chr+0x2008")? as i32),
            GET_SAFE_POSITION_RETURN_STATE => Some(self.i("chr+0x200c")? as i32),
            GET_MAP_VISIBILITY_TYPE => Some((self.i("chr+0x1acd")? & 0xf) as i32),
            IS_TRUE_DEATH => Some((self.i("life+0x8c")? & 1) as i32),
            IS_WALL_JUMP_CONTACT => b(self.i("phys+0xab5")? != 0 || self.i("phys+0xab4")? != 0),
            IS_CROUCH_EXIT_POSSIBLE => Some(self.i("phys+0x93a")? as i32),
            IS_HANG_AND_CLIMB_POSSIBLE => Some(self.i(&format!("docking+{:#x}", 0x7c0 + 0xc * 0xb))? as i32),
            IS_OBSTACLE_AT_DOCKING_START => Some(self.i("docking+0x84c")? as i32),
            IS_OBSTACLE_AT_DOCKING_END => Some(self.i("docking+0x84d")? as i32),
            // FUN_1409f3030: the player (Wolf: true), the wire module not busy, a wire point in reach.
            IS_WIRE_POSSIBLE => b((self.i("wire+0x10")? == 0 || self.i("wire+0x11")? != 0) && self.i("wire_target+0x8")? == 1),
            GET_FALL_HEIGHT => {
                let (top, y) = (self.f("phys+0x854")? as f32, self.f("pos_y")? as f32);
                let mut h = if bit("ctrlflags+0x9c", 1)? { 10000.0 } else { top - y };
                // FUN_140a58a70: Wolf can't die (FUN_140bd7d00) and the drop is not deadly
                // (FUN_140bc2210: physics +0x854 - y < FALL_DEATH_DROP and +0x8d0 < 30).
                let immortal = bit("data+0x228", 2)? || self.i("debug+0xb")? != 0 || self.i("debug+0x0")? != 0;
                if immortal && top - y < self.f("fall_death_drop")? as f32 && (self.f("phys+0x8d0")? as f32) < 30.0 && h > 59.0 {
                    h = 59.0;
                }
                Some((h * 100.0) as i32)
            }
            IS_THROWN | HAS_THROW_REQUEST => b(in_throw()?),
            IS_NORMAL_DMG_PASS_THROUGH_DURING_THROW => b(in_throw()? && bit("flags+0x70", 4)?),
            IS_THROW_ACTIVE => Some(self.i("throw+0x63")? as i32),
            GET_THROW_ANIM_ID => (!in_throw()?).then_some(-1),
            IS_OPPONENT_DEAD_FROM_THROW | IS_THROW_SELF_DEATH | IS_THROW_SUCCESS | IS_THROW_POS_REALIGN => (!in_throw()?).then_some(0),
            _ => None,
        }
    }

    /// `act(id, args...)`: what the exe's act case writes, applied to the fields (and the SpEffect
    /// and request lists). Returns false for the ids not ported (nothing they write is mirrored).
    pub fn act(&mut self, id: i32, args: &[f64]) -> bool {
        let arg = |i: usize| args.get(i).copied().unwrap_or(0.0);
        let get = |s: &Self, n: &str| s.f(n).unwrap_or(0.0) as i64;
        let set_bit = |s: &mut Self, n: &str, b: u32| {
            let v = get(s, n) as u64 | (1u64 << b);
            s.fields.insert(n.to_string(), v as f64);
        };
        // FUN_1407de0c0 keeps the yaw in [-pi, pi]
        let wrap = |a: f32| {
            use std::f32::consts::{PI, TAU};
            if a > PI {
                a - TAU
            } else if a < -PI {
                a + TAU
            } else {
                a
            }
        };
        match id {
            ACT_WAIT => set_bit(self, "flags+0x10", 0),
            ACT_SET_EVENT_ANIM_IN_PROGRESS => set_bit(self, "flags+0x10", 1),
            ACT_SET_IS_ITEM_ANIMATION_PLAYING => set_bit(self, "flags+0x10", 14),
            ACT_LOCKON_FIXED_ANGLE_CANCEL => set_bit(self, "flags+0x10", 16),
            ACT_UNDERWATER => {
                set_bit(self, "flags+0x10", 20);
                self.fields.insert("ctrl+0x350".into(), 2.0);
            }
            ACT_WIRE_ACTION => {
                set_bit(self, "flags+0x10", 21);
                if arg(0) == 0.0 {
                    set_bit(self, "flags+0x10", 22);
                }
            }
            ACT_ADHESION_DISABLED => set_bit(self, "flags+0x10", 26),
            ACT_SET_IS_EVENT_ACTION_POSSIBLE => set_bit(self, "flags+0x10", 28),
            ACT_WIRE_EDGES_PRIORITIZE => set_bit(self, "flags+0x10", 29),
            ACT_DISABLE_FLOOR_ADHESION_DURING_FAST_MOVEMENT => set_bit(self, "flags+0x10", 30),
            ACT_ON_WATER => set_bit(self, "flags+0x10", 31),
            ACT_SET_FALL_PREVENTION_ASSIST => set_bit(self, "flags+0x20", arg(0) as u32),
            ACT_ACTION_BUTTON_EXECUTABLE => set_bit(self, "flags+0x28", arg(0) as u32),
            ACT_SET_AI_ATTACK_STATE => {
                self.fields.insert("flags+0x14".into(), arg(0));
            }
            ACT_RESET_INPUT_QUEUE => {
                set_bit(self, "request+0x188", 0);
                self.fields.insert("fdp+0x11".into(), 0.0);
                self.requests.clear();
            }
            ACT_SWITCH_MOTION => {
                let v = (get(self, "ctrl+0x258") & !1) | (arg(0) != 0.0) as i64;
                self.fields.insert("ctrl+0x258".into(), v as f64);
            }
            ACT_SET_DAMAGE_ANIM_TYPE => {
                self.fields.insert("ctrlflags+0x51d".into(), arg(0));
                self.fields.insert("knockback+0x44".into(), 0x101 as f64);
            }
            ACT_WEAPON_PARAMETER_REFERENCE => {
                self.fields.insert("chr+0x11cc".into(), arg(0));
            }
            ACT_TURN_TOWARD_ATTACKER => {
                let d: Vec<f32> = ["damage+0x50", "damage+0x54", "damage+0x58"].iter().map(|n| self.f(n).unwrap_or(0.0) as f32).collect();
                let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
                let d: Vec<f32> = if len > 0.0 { d.iter().map(|v| v / len).collect() } else { vec![0.0; 3] };
                for (n, v) in ["damage+0x50", "damage+0x54", "damage+0x58"].iter().zip(&d) {
                    self.fields.insert(n.to_string(), *v as f64);
                }
                let yaw = wrap(d[0].atan2(d[2]) + (arg(0) as f32).to_radians());
                self.fields.insert("phys+0x74".into(), yaw as f64);
            }
            ACT_TURN_TO_DIRECTION => {
                let yaw = wrap(self.f("phys+0x74").unwrap_or(0.0) as f32 + (arg(0) as f32).to_radians());
                self.fields.insert("phys+0x74".into(), yaw as f64);
            }
            ACT_SET_THROW_POSSIBILITY_STATE_ATTACKER => {
                self.fields.insert("throw+0x79".into(), arg(0));
            }
            ACT_SET_THROW_POSSIBILITY_STATE_DEFENDER => {
                self.fields.insert("throw+0x7b".into(), arg(0));
            }
            ACT_LOCKON_SYSTEM_UNABLE_TO_TURN_ANGLE => {
                self.fields.insert("turn+0x78".into(), (arg(0) as f32).to_radians() as f64);
                self.fields.insert("turn+0x7c".into(), (arg(1) as f32).to_radians() as f64);
            }
            ACT_SET_TALK_ANIM_STATE => {
                self.fields.insert("talk+0x14".into(), arg(0));
            }
            ACT_WALL_COVER_START_JUDGMENT => {
                self.fields.insert("docking+0x8a0".into(), 1.0);
            }
            ACT_GROUND_HANG_START_JUDGMENT => {
                self.fields.insert("docking+0x8a1".into(), 1.0);
            }
            ACT_AIR_HANG_START_JUDGMENT => {
                self.fields.insert("docking+0x8a2".into(), 1.0);
            }
            ACT_SET_MOVE_PRECALCULATION => {
                self.fields.insert(format!("docking+{:#x}", 0x7bc + 12 * arg(0) as i64), arg(1));
            }
            ACT_ADD_SP_EFFECT => {
                let id = arg(0) as i64;
                if !self.sp_effects.iter().any(|&(i, _)| i == id) {
                    // gap: its behaviorRefId (the param lookup is in `from_world`)
                    self.sp_effects.push((id, 0));
                }
            }
            _ => return false,
        }
        true
    }
}
