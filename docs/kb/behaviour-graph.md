# Behaviour graph and HKS survey (2026-11-03)

How much of Wolf's animation and action logic Sv1 takes from the game's own files, and how much is
hand-ported. Goal: run the game's graph (`c0000.hkx`) and scripts (`hks_src/c0000*.lua`) as data,
with the runtime rules taken from byte-matched exe code in DSekiro.

## Graph (`extracted/chr/c0000.behbnd.d/c0000.hkx`, `sekiro-extract hkxdump`)

| Node type | Count | In Sv1 |
|---|---|---|
| hkbClipGenerator | 2,566 | loop flag only (`loop_clips_c0000.json`); playback speed all 1 |
| CustomManualSelectorGenerator | 2,041 | clip reuse only (`blend_src.json`) |
| hkbStateMachine (StateInfo 1,360, TransitionInfo 1,898) | 48 | hand-ported in `player.rs` |
| hkbManualSelectorGenerator | 362 | no |
| hkbVariableBindingSet (861 bindings) | 839 | no |
| CustomDockingGenerator | 105 | hand-ported (`docking.rs`) |
| hkbLayerGenerator (54 layers, 32 bone weight sets) | 22 | guard walk only, by hand |
| CustomLookAtTwistModifier / hkbTwistModifier | 20 / 19 | twist by hand (`anim.rs`) |
| CustomTransitionEffect / hkbBlendingTransitionEffect | 10 / 3 | durations by hand; most crossfades are TAE Blend events |
| hkbBlenderGenerator | 2 | no (locomotion blend by hand) |
| Foot IK / hand IK modifiers | 1 / 2+1 | no (feet slide) |
| hkbScriptGenerator, handle modifiers, others | 4, 6 | no |

At runtime Sv1 reads two facts from the graph; the rest is hand-ported or missing.

## HKS (`extracted/hks_src/c0000*.lua`, 30,038 lines)

4,098 script functions. The engine surface they call is small:
`env` 1,605 calls (81 ids), `act` 683 calls (45 ids), `hkbGetVariable` 195, `hkbFireEvent` 2,
bone access (`hkbGet/SetBoneModelSpace`, `hkbIsBoneValid`, `hkbIsNodeActive`) 6, plus Lua builtins.
The exe's `act` dispatcher is FUN_140b32410 and the `env` dispatcher FUN_140b36410; both and their
per-id handlers are on DSekiro's combat lists (rows C4-C6). Sv1 has no Lua runtime: the scripts'
input, cancel and state rules are hand-ported into `player.rs`.

## Plan

1. DSekiro matches the dispatchers and handlers (rows C4-C6) and Havok's behaviour runtime
   (`data/targets/behavior.txt`, 915 functions, 760 open; rows H1b-H2b).
2. Sv1 runs the HKS files with a Lua runtime, `env` / `act` answered from the handlers (step 1
   below: the env surface read from the decomp and checked in the emulator, byte matching dropped).
3. Sv1 runs `c0000.hkx` with a small graph player (state machines, transitions, selectors,
   layers, blenders, IK) whose rules come from the matched Havok code; the sim tests keep it honest
   while it replaces the hand-ported parts one by one.

Files are read from the player's own install at runtime; none are shipped.

## HKS engine surface, step 1: env ids against the exe (2026-11-03)

The owner stopped byte matching for this (2026-11-03): the handlers are read from the decompiled
dispatchers and checked in the emulator instead. `tools/emu-cases` pilot `hks_env` (`src/hks.rs`)
parses Wolf's four script files, then runs the env dispatcher FUN_140b36410(ctx, id, lua_State) on
Wolf from the Naomori-arena snapshot (standing idle, AI off) for every id and every argument list
the scripts pass: 330 cases over 82 ids, none stopped early, under a second. The Lua state is a
blank block; the four argument readers (FUN_141010dc0 / dd0 int, d50 / d60 float; Lua index 2 is
the first argument) are hooked to answer from the case. `extracted/emu/hks_env.json` holds the
answers, Wolf's module classes (RTTI), his SpEffect list and the pokes.

- Surface: env 1,605 calls over 82 ids (81 numbers and one string id), act 683 calls over 46 ids (45
  and one string). The string ids go through the exe's name maps ([0x143d85568] env, [0x143d85558]
  act; std::map, UTF-16 names cut to 31 units by a fixed string, so the script's
  `..._寿命延長を厳密に取得` is found as `..._寿命延長を厳密に取`): env -> 3064, act -> 3041. The top 10
  env ids carry 1,162 calls, the top 20 1,374; 3036 GetBehaviorRefID alone 689 (194 refs).
- Wolf's module bag (chr +0x1ff8, RTTI): +0x0 SprjChrActionFlagModule (most HKS state: +0x10 the
  stay / request bits the acts set, +0x38 damage level, +0x40 / +0x44 behaviour ids, +0x4c special
  attribute, +0x58 bit 4 attack contact, +0x5c damage type, +0x68 hit direction, +0x3c8 variable
  changes, +0xaf0 anim-end bits, +0xaf8 dt), +0x18 data, +0x50 talk, +0x68 physics, +0x70 fall,
  +0x80 action request, +0x88 throw, +0x98 damage, +0xa8 knockback, +0x128 docking, +0x130 wire;
  chr +0x50 PlayerCtrl, chr +0x11d0 SpecialEffect.
- 30 pokes (a handler's field written, the id run again, restored) all moved the answer as the
  table says. ActionRequest: Wolf's request module runs with +0x1b8 set, so the arm bit comes from
  the mask at [+0x40] +0x20 only in the stay state (flags +0x10 bit 0, which act 9100 Wait sets);
  in any other state from the entry of the current state id (+0xc8) in the list [+0x50, +0x58) (the
  decompiler dropped the arm argument of FUN_140b2cd10; the +0x30 mask is never read for Wolf).
  GetBehaviorRefID counts an entry only if its param +0x326 is 0 or the entry's +0x27 is set.
- Idle answers worth knowing: Wolf wears 27 SpEffects; refs 314 (127300, Loaded Axe 73000's
  residentSpEffectId) and 284 (140701, art weapon 7400's) are the only GetBehaviorRefID hits.
  GetVerticalSpeed is -163 standing (= -9.8 / 60: one frame of gravity), so `env(3003) < 0` holds
  on the ground. HP 548 of 1,120. ActionUnlocked is 1 for every type but 0 (dive) on this save.

`src/hks_env.rs` (not wired; player.rs keeps the hand-ported logic): one documented constant per
env id (exe case, what it reads, Sv1's source or `gap:`), `HksEnv::from_world` gathers Wolf's
state, `HksEnv::env(id, args)` answers or returns None. Its SpEffect set also holds the equipped
art's and prosthetic's residentSpEffectId (the combat export lacks 127300: its behaviorRefId comes
from `json/params/SpEffectParam.json`). Test `hks_env_matches_the_exe` (src/sim_tests/emu_hks.rs):
the 30 pokes, then every case Sv1 answers for idle Wolf (with the snapshot's equipment and HP)
equals the exe: 239 cases. The first run differed in two: GetHP (the save's 548 vs Sv1's config,
now set from the snapshot) and GetVerticalSpeed on the ground (Sv1 0 -> -WORLD_GRAVITY x dt = -163,
fitted on the one standing sample).

Coverage: Sv1 answers 22 ids = 1,045 of 1,605 env calls (65.1 %); 3036 and 1106 alone are 831.
The act side is mapped (46 ids, all 683 calls, below) but not run or ported: its writes land in
state Sv1 keeps elsewhere. Next: the gaps that carry calls (3063 GetVariableChangeValue 76, 1108
ActionDuration 54, 2000 IsMoveCancelPossible 48: the TAE flags behind ctrl flags +0x88 / +0x9c and
flags +0x3c8 are not traced), the hit record (202 / 236 / 285 / 3032 / 222), the save-dependent
equipment ids (225 / 345 / 3027 / 3033), poses other than idle (a snapshot mid-attack or in the
air, where most answered ids would move), then a Lua runtime.

### env ids (most called first; "exe idle" = the dispatcher's answers on the snapshot)

| id | name | calls | case | reads | exe idle | Sv1 source | status |
|---|---|---|---|---|---|---|---|
| 3036 | GetBehaviorRefID | 689 | Case 0xbdc | the SpEffect list: an entry whose SpEffectParam +0x3c0 (behaviorRefId) == arg, counted when param +0x326 is 0 or entry +0x27 is set. | 0, 1 | `HksEnv::sp_effects` (gap: the +0x326 / +0x27 lifetime clause; every listed SpEffect counts). | answered, = exe idle |
| 1106 | ActionRequest | 142 | Case 0x452 | request module FUN_140b2b6d0(m, 1, arm). Wolf's module has +0x1b8 set, so FUN_140b2cd10: in the stay state the mask at [+0x40] +0x20 (bit arm), else the entry of the current state id (+0xc8) in the list [+0x50, +0x58) of {id, pad, u64 mask}. (+0x1b8 clear: +0x20 or +0x30 by the stay bit.) | 0 | `Player::requests` (buffered presses, by ACTION_ARM). | answered, = exe idle |
| 3063 | GetVariableChangeValue | 76 | Case 0xbf7 | flags +0x3c8 entries {int id, float value, int} (count +0x9d0): value x 1000 of the entry with id arg, 0 none. | 0 | gap: the TAE event that fills it is not traced. | gap |
| 1108 | ActionDuration | 54 | Case 0x454 | request module +0xe8 + 4 x arm (seconds held, arm < 0x21) x 1000. | 0 | gap: Sv1 keeps held flags (`PadInput`), not hold times. | gap |
| 2000 | IsMoveCancelPossible | 48 | Case 2000 | ctrl flags +0x88 bit 2 and +0x9c bit 4 (both TAE action flags). | 0 | gap: which TAE flags they are is not traced (0 standing idle). | gap |
| 273 | GetThrowAnimID | 35 | Case 0x111 | FUN_140bdfb20(throw module, 0, 1): the throw +0x10 entry's anim by its type (+0x18), -1 with none. | -1 | -1 out of a throw; gap in one (the anim id). | answered, = exe idle |
| 1118 | IsLockedOn | 31 | Case 0x45e | chr +0x1070 (lock-on). | 0 | `camera::LockOn::target`. | answered, = exe idle |
| 225 | GetEquipWeaponCategory | 29 | Case 0xe1 | FUN_1409f1b00(chr, hand): the int array at [chr +0x120], hand 0 -> [1], else [hand + 1] when [0] == 1. | 50, 73 | gap: save-dependent equipment state. | gap |
| 3033 | ActionUnlocked | 29 | Case 0xbd9 | PlayerGameData (chr vtable +0x1b8) +0x278 -> FUN_1407a7fe0(type). | 0, 1 | gap: save-dependent (Sv1's rule is combat::action_unlocked with config player.skills). | gap |
| 345 | GetEquipWeaponSpecialCategoryNumber | 29 | Case 0x159 | chr vtable +0x340(hand) -> EquipParamWeapon, its u16 +0xea. | 107 | gap: save-dependent. | gap |
| 1105 | IsStayState | 28 | Case 0x451 | flags +0x10 bit 0 (the stay state: the request module then answers from its global mask). | 1 | Wolf in StandIdle / CrouchIdle (approx: which exe states set it is not traced). | answered, = exe idle |
| 285 | GetSpecialAttribute | 25 | Case 0x11d | flags +0x4c (the last hit's special attribute). | 0 | gap. | gap |
| 3017 | GetDockingEdgeRequest | 25 | Case 0xbc9 | docking module FUN_140b7d230 (signed byte, -1 none). | -1 | gap. | gap |
| 202 | GetReceivedDamageType | 21 | Case 0xca | flags +0x5c (the last hit's damage type; 99999 none). | 99999 | gap: Sv1 keeps no hit record. | gap |
| 236 | GetDamageLevel | 21 | Case 0xec | flags +0x38 (the last hit's damage level). | 0 | gap: Sv1 keeps it only for a killing hit (`Actor::death_hit`). | gap |
| 3008 | IsDebugDash | 21 | Case 0xbc0 | [0x143d7a388] +0x6c (debug dash). | 0 | 0. | answered, = exe idle |
| 3064 | (string id) strict GetBehaviorRefID | 20 | Case 0xbf8, the string id "特殊効果発動中か_Behavior参照ID_寿命延長を厳密に取得" (the exe's name map [0x143d85568], names cut to 31 UTF-16 units) | like 3036, but every matching entry needs param +0x16b bit 7 and (FUN_140c064a0(entry) false or ctrl flags +0x8c bit 6). | 0 | the 3036 rule (approx). | answered, = exe idle |
| 207 | GetWeaponSwitchType | 18 | Case 0xcf | the int at [chr +0x120] (weapon switch state). | 1 | gap. | gap |
| 337 | IsThrowPosRealign | 18 | Case 0x151 | FUN_140be0170 and FUN_140be0930 on the throw module (a throw running whose FUN_1408029b0 is false). | 0 | 0 out of a throw; gap in one. | answered, = exe idle |
| 3035 | PCRequestType | 15 | Case 0xbdb | chr vtable +0x2b0(arm). | 1 | gap. | gap |
| 334 | GetBehaviorID | 14 | Case 0x14e | flags +0x40 == arg or flags +0x44 == arg. | 0 | gap. | gap |
| 1112 | HasThrowRequest | 11 | Case 0x458 | not FUN_140be0890(throw): 0 while the throw +0x10 entry has type 0 (idle), 1 otherwise. | 0 | gap. | gap |
| 205 | IsNormalDmgPassThroughDuringThrow | 11 | Case 0xcd | in a throw (throw +0x10 entry, its +0x18 type != 0) and flags +0x70 bit 4. | 0 | 0 out of a throw (`actor::in_throw`); gap in one. | answered, = exe idle |
| 248 | IsTrulyLanding | 10 | Case 0xf8 | physics +0x929 set and +0xa48 clear. | 1 | not `Actor::airborne` (approx). | answered, = exe idle |
| 3032 | GetDamageDirection | 10 | Case 0xbd8 | flags +0x68 (hit direction): 0, 1, 6, 7 -> 0 (front), else 1. | 0 | gap. | gap |
| 1116 | GetSpEffectID | 8 | Case 0x45c | FUN_1409f1c80: an entry with id arg in the SpEffect list. | 0 | the SpEffects `HksEnv::sp_effects` gathers. | answered, = exe idle |
| 3000 | IsWirePossible | 8 | Case 3000 | FUN_1409f3030: the player, wire module (bag +0x130) not busy (+0x10 clear or +0x11 set) and [0x143d8e058] +8 == 1 (a wire target in reach). | 0 | gap: wire.rs has its own search. | gap |
| 3028 | IsTrueDeath | 7 | Case 0xbd4 | FUN_1409e5c00(chr +0x18). | 0 | gap. | gap |
| 3054 | GetTalkEzStateRefID | 7 | Case 0xbee | talk module (bag +0x50) +0x18 (-1 none). | -1 | gap. | gap |
| 3003 | GetVerticalSpeed | 6 | Case 0xbbb | physics +0x164 (vertical speed, m/s) x 1000. | -163 | `Actor::vel_y` in the air; on the ground one frame of gravity, -WORLD_GRAVITY x dt (fitted: standing idle the exe gives -163 = -9.8 / 60, so `env(3003) < 0` holds on the ground; the physics rule behind it is not traced). | answered, = exe idle |
| 339 | IsAnimEnd | 6 | Case 0x153 | flags +0xaf0 bit arg (u64). | 0 | gap: which clip ends set which bit is not traced. | gap |
| 1119 | GetAtkDirection | 5 | Case 0x45f | damage module (bag +0x98) +0x60. | 0 | gap. | gap |
| 3011 | IsOnEdgeOfDockingTarget | 5 | Case 0xbc3 | docking module FUN_140b7d3a0. | 0 | gap. | gap |
| 3025 | IsSwimContact | 5 | Case 0xbd1 | FUN_1409f2ae0(chr). | 0 | gap: no water. | gap |
| 3039 | IsDiveToSwimAllowed | 5 | Case 0xbdf | docking FUN_140b7d290 == 4 and FUN_140b7d480(d, 1). | 0 | gap. | gap |
| 1007 | IsCOMPlayer | 4 | Case 0x3ef | FUN_1409efec0(chr) == 6. | 0 | 0 (Wolf). | answered, = exe idle |
| 1121 | GetKnockbackDistance | 4 | Case 0x461 | knockback module (bag +0xa8) float +0x10 as int. | 0 | gap. | gap |
| 201 | IsLanding | 4 | Case 0xc9 | FUN_1409f3380: physics +0xa2 set and FUN_140bb14d0(physics +0x1c0) false (on the ground). | 1 | not `Actor::airborne` (approx). | answered, = exe idle |
| 206 | IsThrown | 4 | Case 0xce | FUN_140be0930(throw module): the throw +0x10 entry's +0x18 type != 0. | 0 | 0 out of a throw; gap in one. | answered, = exe idle |
| 222 | GetReceivedDamageDirection | 4 | Case 0xde | flags +0x68 (hit direction): 0 / 7 -> 2, 1 / 2 -> 1, 3 / 4 -> 3, else 0. | 2 | gap. | gap |
| 224 | GetFallHeight | 4 | Case 0xe0 | fall module (bag +0x70) FUN_140b97880, capped at 59 when FUN_140a58a70, x100. | 0 | gap: Sv1 has `Player::air_top` but not the module's rule. | gap |
| 231 | GetItemAnimType | 4 | Case 0xe7 | FUN_1409efba0(chr) (255 idle). | 255 | gap: no item anims. | gap |
| 256 | HasReceivedAnyDamage | 4 | Case 0x100 | flags +0x54 bit 2. | 0 | gap. | gap |
| 3045 | IsWallHangAirPossible | 4 | Case 0xbe5 | docking FUN_140b7d440(d, 0). | 0 | gap. | gap |
| 3051 | IsObstacleAtDockingStart | 4 | Case 0xbeb | docking +0x84c. | 0 | gap. | gap |
| 3052 | IsObstacleAtDockingEnd | 4 | Case 0xbec | docking +0x84d. | 0 | gap. | gap |
| 1000 | GetHP | 3 | Case 1000 | data module (bag +0x18) +0x130 (HP). | 548 | `Actor::hp`. | answered, = exe idle |
| 3018 | GetGuardBounceBehavior | 3 | Case 0xbca | flags +0x60 (i16). | -1 | gap. | gap |
| 3019 | GetJustGuardBounceBehavior | 3 | Case 0xbcb | flags +0x62 (i16). | -1 | gap. | gap |
| 3031 | IsThrowActive | 3 | Case 0xbd7 | throw module byte +0x63. | 0 | 0 out of a throw; gap in one. | answered, = exe idle |
| 3037 | IsCrouchExitPossible | 3 | Case 0xbdd | physics +0x93a. gap (1 standing). | 1 |  | gap |
| 3050 | IsHangAndClimbPossible | 3 | Case 0xbea | docking FUN_140b7d480(d, 0xb). | 0 | gap. | gap |
| 333 | GetDT | 3 | Case 0x14d | flags +0xaf8 (the frame's dt, s) x 1000. | 16 | the actor's clock step (1/60). | answered, = exe idle |
| 113 | IsItemUseMenuOpen | 2 | Case 0x71, player only | the selected quick item (FUN_1407ee050) usable (FUN_1409f3890) and the item menu (chr vtable +0x310 object) slot 0 accepts it. | 0 | gap: no item menu. | gap |
| 115 | IsItemUseMenuOpened | 2 | Case 0x73 | the item menu object (chr vtable +0x310) slot +8. | 0 | gap: no item menu. | gap |
| 2004 | IsAtkContactTarget | 2 | Case 0x7d4 | flags +0x58 bit 4 (an attack connected; set in FUN_1409e5fd0). | 0 | `Actor::attack_hit`. | answered, = exe idle |
| 233 | IsItemUseable | 2 | Case 0xe9 | FUN_1409f3890(chr, selected quick item, 0). | 0 | gap. | gap |
| 237 | GetGuardLevelAction | 2 | Case 0xed | FUN_140843820(flags +0x38, flags +0x3c). | 0 | gap. | gap |
| 3038 | IsDiveAllowed | 2 | Case 0xbde | docking FUN_140b7d290 == 4 and not FUN_140b7d480(d, 0). | 0 | gap. | gap |
| 3040 | GetMapVisibilityType | 2 | Case 0xbe0 | chr +0x1acd & 0xf. | 0 | gap. | gap |
| 3043 | IsWallHangPossible | 2 | Case 0xbe3 | docking FUN_140b7d440(d, 1). | 0 | gap. | gap |
| 3044 | IsWallHangGroundPossible | 2 | Case 0xbe4 | docking FUN_140b7d440(d, 2). | 0 | gap. | gap |
| 3046 | GetDockingEdgeVariationID | 2 | Case 0xbe6 | docking FUN_140b7d260(d, arg 0 -> 1, 1 -> 0), -1 otherwise. | -1 | gap. | gap |
| 3048 | IsHangLeftOutterCornerPossible | 2 | Case 0xbe8 | docking FUN_140b7d740 == 1. | 0 | gap. | gap |
| 3049 | IsHangRightOutterCornerPossible | 2 | Case 0xbe9 | docking FUN_140b7d740 == 2. | 0 | gap. | gap |
| 3053 | GetTalkParamRefID | 2 | Case 0xbed | the talk param of [FUN_140b35960 singleton] +0x20f4 (-1 none). | -1 | gap. | gap |
| 3056 | GetGuardDeflectBehavior | 2 | Case 0xbf0 | flags +0x64 (i16). | -1 | gap. | gap |
| 3057 | GetJustGuardDeflectBehavior | 2 | Case 0xbf1 | flags +0x66 (i16). | -1 | gap. | gap |
| 3058 | LipSyncRequest | 2 | Case 0xbf2 | talk module +0x28. | 0 | gap. | gap |
| 3059 | IsHangLeftInnerCornerPossible | 2 | Case 0xbf3 | docking FUN_140b7d6d0 == 1. | 0 | gap. | gap |
| 3060 | IsHangRightInnerCornerPossible | 2 | Case 0xbf4 | docking FUN_140b7d6d0 == 2. | 0 | gap. | gap |
| 349 | IsDamageMotionOff | 2 | Case 0x15d | ctrl flags +0x8a bit 0. | 0 | gap. | gap |
| 105 | GetCommandIDFromEvent | 1 | Case 0x69 | PlayerCtrl +0x50 block +0x4f8 + 4 x arg (event command id per slot; -1 none). | -1 | gap: Sv1 has no event commands. | gap |
| 200 | IsFalling | 1 | Case 200 | FUN_1409f3190(chr). | 0 | gap: not traced. | gap |
| 274 | IsOpponentDeadFromThrow | 1 | Case 0x112 | FUN_140b39ce0(ctx, chr). | 0 | 0 out of a throw; gap in one. | answered, = exe idle |
| 276 | IsThrowSelfDeath | 1 | Case 0x114 | FUN_140b39dc0(ctx, chr). | 0 | 0 out of a throw; gap in one (player.rs FLAG_THROW_TYPE5 stands in). | answered, = exe idle |
| 277 | IsThrowSuccess | 1 | Case 0x115 | FUN_140b39e90(ctx, chr). | 0 | 0 out of a throw; gap in one. | answered, = exe idle |
| 3020 | IsWallJumpContact | 1 | Case 0xbcc | physics +0xab5 set, or +0xab4 set. | 0 | gap. | gap |
| 3027 | GetNextWeaponType | 1 | Case 0xbd3 | chr vtable +0x420(hand) -> the next slot's EquipParamWeapon byte +0xe3 (-1 none). | 78 | gap: save-dependent. | gap |
| 3029 | IsDockingEdgeInvalid | 1 | Case 0xbd5 | docking module FUN_140b7d710. | 0 | gap. | gap |
| 3055 | GetSaveState | 1 | Case 0xbef | player only, chr +0x2008 (-1 otherwise). | -1 | gap. | gap |
| 3061 | GetSafePositionReturnState | 1 | Case 0xbf5 | player only, chr +0x200c (-1 otherwise). | -1 | gap. | gap |

### act ids (mapped from FUN_140b32410; not run)

| id | name | calls | case | what it does | Sv1 counterpart |
|---|---|---|---|---|---|
| 154 | WeaponParameterReference(hand) | 150 | 0x9a | player only: chr +0x11cc = hand (FUN_1409f6750, then FUN_14117a9d0): which hand's weapon the attacks take | implicit (the sword) |
| 9101 | ResetInputQueue | 140 | 0x238d | request module FUN_140b2afd0 (+0x188 bit 0, flags list reset FUN_140b28980); FdpDataExModule (bag +0xc0) +0x11 = 0 | `Player::requests` cleared by the action starters |
| 127 | CloseMenuWhenUsingItem | 35 | 0x7f | player only: item menu object (chr vtable +0x308) slot +0x18 | none (no item menu) |
| 135 | ForceCancelThrowAnim | 33 | 0x87 | FUN_140be16b0(throw module, 0x140000001) | player.rs throw end |
| 3023 | SetMovePrecalculation(type, anim) | 32 | 0xbcf | docking FUN_140b7d920(type, anim); physics +0x1c0 FUN_140baeea0 (type 2 / 6 -> 0, 3-5 -> 1); wire hit-change module FUN_140bebcb0 | wire.rs / docking.rs |
| 3025 | TurnToDirection(deg) | 30 | 0xbd1 | PlayerCtrl FUN_1407dcce0 angle + deg x pi/180 -> FUN_1407de0c0 | `Player::step_tilt` / `jump_tilt` |
| 141 | SetDamageAnimType(type) | 29 | 0x8d | ctrl flags +0x51d = type; knockback module FUN_140ba3190(type), +0x44 = 0x101 | combat.rs reaction choice |
| 3034 | ActionButtonExecutable(type) | 22 | 0xbda | FUN_140b322f0(flags module, type) | none |
| 159 | TurnTowardAttacker(deg) | 22 | 0x9f | damage module FUN_140b65680(deg x pi/180) | combat.rs turn to the attacker |
| 2002 | AddSpEffect(id) | 17 | 0x7d2 | FUN_1409e9c00(chr, id, chr) | `Player::held` / `timed` |
| 160 | SetThrowPossibilityState_Attacker(s) | 17 | 0xa0 | throw module +0x79 = s | player.rs deathblow checks |
| 161 | SetThrowPossibilityState_Defender(s) | 16 | 0xa1 | throw module +0x7b = s | player.rs throw states |
| 101 | SwitchMotion(bool) | 15 | 0x65 | FUN_1407d74d0(ctrl flags, bool) | none |
| 2018 | LockonFixedAngleCancel | 13 | 0x7e2 | flags +0x10 bit 16 | none |
| 147 | SetIsEquipmentChangeableFromMenu | 11 | 0x93 | player only: menu object slot +0x108 | none |
| 138 | SetIsEventActionPossible | 10 | 0x8a | flags +0x10 bit 28 | none |
| 9100 | Wait | 8 | 0x238c | flags +0x10 bit 0 (the stay bit env 1105 reads) | idle states |
| 3030 | ActionGuideDisplay(guide, arm) | 8 | 0xbd6 | FUN_140ea3280([0x143d8d2b8], guide, arm) | none (no guide HUD) |
| 3029 | SetTalkAnimState(ref) | 7 | 0xbd5 | talk module +0x14 = ref | enemy/talk.rs |
| 3019 | LockHomingAction(b) | 7 | 0xbcb | player locked on: physics +0x248 homing to the lock target (FUN_140bb7a40) or off (FUN_140bb7100) | player.rs homing |
| 136 | SetThrowState(s) | 6 | 0x88 | FUN_140be16a0(throw module, s) | player.rs throws |
| 3032 | AdhesionDisabled | 6 | 0xbd8 | flags +0x10 bit 26 | none |
| 3018 | WireAction(b) | 5 | 0xbca | wire target notice ([0x143d8e058]); flags +0x10 bit 21 (and 22 for 0); physics +0x248 homing to the wire point | wire.rs |
| 155 | AINotifyAttackType(t) | 5 | 0x9b | player only: t 0 FUN_1409f3f50(chr), 1 FUN_1405a5c60(chr) | none |
| 9103 | SetAIAttackState(v) | 4 | 0x238f | flags +0x14 = v | none |
| 2024 | UseItemDecision | 4 | 0x7e8 | player only: FUN_140a28160(chr) | player.rs gourd |
| 2015 | LockonSystemUnableToTurnAngle(a, b) | 3 | 0x7df | [PlayerCtrl +0x250] +0x78 / +0x7c = a, b deg in rad | camera.rs lock-on |
| 150 | SetIsItemAnimationPlaying | 3 | 0x96 | flags +0x10 bit 14 | none |
| 9102 | SetEventAnimInProgress | 3 | 0x238e | flags +0x10 bit 1 | none |
| 2019 | TurnToLockonTargetImmediately | 3 | 0x7e3 | locked on (chr +0x1070): face the lock point chr +0x1080 (FUN_1409f5080) | player.rs lock-on turn |
| 3035 | WireEdgesPrioritize | 2 | 0xbdb | flags +0x10 bit 29 | none |
| 123 | OpenMenuWhenUsingItem | 2 | 0x7b | menu object slot +0x10 (the selected item) | none |
| 3016 | SetFallPreventionAssist(i) | 2 | 0xbc8 | flags +0x20 bit i | none |
| 148 | SetHavokVariable(name, value) | 1 | 0x94 | FUN_140b350a0(ctx, L) | anim.rs by hand |
| 139 | RequestThrowAnimInterrupt | 1 | 0x8b | FUN_140be16b0(throw module, 1) when its throw allows | player.rs throws |
| 3037 | OnWater | 1 | 0xbdd | flags +0x10 bit 31 | none |
| 3011 | Underwater | 1 | 0xbc3 | flags +0x10 bit 20; PlayerCtrl +0x350 = 2 | none |
| 3026 | WallCoverStartJudgment | 1 | 0xbd2 | docking FUN_140b7d810(d, 1) | docking.rs |
| 3027 | GroundHangStartJudgment | 1 | 0xbd3 | docking FUN_140b7d810(d, 2) | docking.rs |
| 3028 | AirHangStartJudgment | 1 | 0xbd4 | docking FUN_140b7d810(d, 0) | docking.rs |
| 3004 | AddSpEffectToLockOnTarget(id) | 1 | 0xbbc | the lock target ([0x143d78058] lookup): FUN_1409e9c00(target, id, chr) | not checked |
| 9000 | DebugLogOutput(str) | 1 | 9000 | prints "%5d: %s" (chr +0x1fb0, str) when ctrl flags +0x5b4 | none |
| 156 | SetAutoCaptureTarget | 1 | 0x9c | auto-homing module (bag +0x108) FUN_140b2e090 | `Actor::homing` |
| 157 | ClearAutoCaptureTarget | 1 | 0x9d | auto-homing module FUN_140b2d9d0 | `Actor::homing` |
| 3036 | DisableFloorAdhesionDuringFastMovement | 1 | 0xbdc | flags +0x10 bit 30 | none |
| 3041 | (string) warp-type input reset | 1 | 0xbe1 | [chr vtable +0x308 object] slot +0xb0 | none |
