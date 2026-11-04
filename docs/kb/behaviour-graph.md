# Behaviour graph and HKS (the game's player and enemy scripts, c0000.hkx)

The survey (2026-11-03) comes first; "The pipeline and how the game picks clips" below is the dated log of the port, oldest first.

## Survey (2026-11-03)

How much of Wolf's animation and action logic Sv1 takes from the game's own files, and how much is
hand-ported. Goal: run the game's graph (`c0000.hkx`) and scripts (`hks_src/c0000*.lua`) as data,
with the runtime rules taken from byte-matched exe code in DSekiro.

### Graph (`extracted/chr/c0000.behbnd.d/c0000.hkx`, `sekiro-extract hkxdump`)

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

### HKS (`extracted/hks_src/c0000*.lua`, 30,038 lines)

4,098 script functions. The engine surface they call is small:
`env` 1,605 calls (81 ids), `act` 683 calls (45 ids), `hkbGetVariable` 195, `hkbFireEvent` 2,
bone access (`hkbGet/SetBoneModelSpace`, `hkbIsBoneValid`, `hkbIsNodeActive`) 6, plus Lua builtins.
The exe's `act` dispatcher is FUN_140b32410 and the `env` dispatcher FUN_140b36410; both and their
per-id handlers are on DSekiro's combat lists (rows C4-C6). Sv1 has no Lua runtime: the scripts'
input, cancel and state rules are hand-ported into `player.rs`.

### Plan

1. DSekiro matches the dispatchers and handlers (rows C4-C6) and Havok's behaviour runtime
   (`data/targets/behavior.txt`, 915 functions, 760 open; rows H1b-H2b).
2. Sv1 runs the HKS files with a Lua runtime, `env` / `act` answered from the handlers (step 1
   below: the env surface read from the decomp and checked in the emulator, byte matching dropped).
3. Sv1 runs `c0000.hkx` with a small graph player (state machines, transitions, selectors,
   layers, blenders, IK) whose rules come from the matched Havok code; the sim tests keep it honest
   while it replaces the hand-ported parts one by one.

Files are read from the player's own install at runtime; none are shipped.

### HKS engine surface, step 1: env ids against the exe (2026-11-03)

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

#### env ids (most called first; "exe idle" = the dispatcher's answers on the snapshot)

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

#### act ids (mapped from FUN_140b32410; not run)

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

### HKS engine surface, step 2: poses and acts (2026-11-03, hks2)

`tools/emu-cases <exe> extracted/emu hks2 --snapshot <dir>` (`src/hks2.rs`; `hks2_env`, `hks2_act`,
`hks2_move` run one part) on the 2026-11-03 snapshot (AI live; Wolf standing, the frame's dt 0.0455).
`src/hks_env.rs` now keeps a mirror of the exe fields the handlers read, by module and offset
(`HksEnv::fields`, "flags+0x68" = SprjChrActionFlagModule +0x68, the generator's names):
`from_world` fills the ones Sv1 has state for, `env(id)` is the dispatcher's rule on them (None when
a field it needs is absent), `act(id)` writes what the exe's act case writes.

- Env poses (`hks2_env.json`, test `hks_env_poses_match_the_exe`): 105 fields read on the snapshot
  ("idle"), then 223 poses over 47 ids, each writing some fields, running FUN_140b36410 and
  restoring them. All 223 (and their idle answers) equal the ported rules. 36 of the 60 gap ids
  now have their rule ported (384 calls): 3063 GetVariableChangeValue (the flags +0x3c8 list by id,
  x1000), 1108 ActionDuration (request +0xe8 + 4 arm, x1000, arm < 0x21), 2000 IsMoveCancelPossible
  (ctrl flags +0x88 bit 2 and +0x9c bit 4), the hit record 285 / 202 / 236 / 256 / 3032 / 222 (the
  two direction maps over 0..9), 237 GetGuardLevelAction (a 12 x 5 table, all 60 pairs + out of
  range taken from the exe: `GUARD_LEVEL_ACTION`), 207, 334, 339, 349, 105, 1112, 1119, 1121
  (truncated toward 0: 2.7 -> 2, -1.5 -> -1), 3000 (the wire module busy unless +0x11, [0x143d8e058]
  +8 == 1 exactly), 3018 / 3019 / 3056 / 3057 (i16), 3020, 3028 (bit 0 of [[chr+0x18]+0x30]+0x8c),
  3037, 3040 (low nibble), 3050 / 3051 / 3052 (docking +0x844 = +0x7c0 + 12 x 11, +0x84c, +0x84d),
  3054 / 3058 / 3055 / 3061, 200 IsFalling (on the ground 0; in the air physics +0x100 clear or
  +0x900 >= 1, and ctrl flags +0x88 bit 5 clear), 224 GetFallHeight. The ids Sv1 answered on idle
  move with their fields in the other poses too (in the air: 201 / 248 0, 200 1, 3003 the speed;
  1105, 1118, 2004, 333, 1000, the throw ids with an entry type).
- GetFallHeight: physics +0x854 (the fall's top) minus the physics position's y (FUN_140bbfc00),
  10000 with ctrl flags +0x9c bit 1, x100; capped at 59 m when Wolf can't die (FUN_140bd7d00: data
  +0x228 bit 2, DebugFlags +0xb, or the player with DebugFlags +0x0 player-no-death, on in the
  snapshot) and the drop is not deadly (FUN_140bc2210: drop < [0x143b09efc], a debug variable, 1500
  in the snapshot, and physics +0x8d0 < 30). So with the snapshot's debug flag a 70 m fall reads 5900;
  mortal 7000. Sv1: top `Player::air_top`, Set Character Immortality as data +0x228 bit 2 (inferred).
- GetVerticalSpeed standing is -9.8 x dt on this snapshot too (-0.446 at dt 0.0455; -0.163 at 1/60
  on the first one): Sv1's rule -WORLD_GRAVITY x dt holds at both frame times.
- Sv1 has no state yet (the field is absent, the id answers None in the game) for: the hit record
  (flags +0x38 / +0x3c / +0x4c / +0x54 / +0x5c / +0x60..+0x66 / +0x68), the TAE ctrl flags (+0x88,
  +0x9c: also GetFallHeight's), the variable-change list, hold times, the behaviour ids, the
  anim-end bits, docking, talk, wire reach, save states, the throw entry inside a throw. Each is one
  `fields.insert` in `from_world` once Sv1 keeps it.
- Not ported (24 ids, 176 calls): save / equipment (225, 345, 3027, 3033, 3035, 231, 233), item menu
  (113, 115), the docking functions (3011, 3017, 3029, 3038, 3039, 3043-3046, 3048, 3049, 3059, 3060:
  sub-objects at docking +0x20 / +0x290 / +0x4f0), 3025 swim contact, 3053 talk param.

- Acts (`hks2_act.json`, test `hks_act_matches_the_exe`): every act id and argument list of the
  scripts run on a fresh emulator (129 cases; 148 SetHavokVariable, 9000 and the 9103
  hkbGetVariable call not run: string / call arguments), the fields the acts set cleared first (the
  snapshot's flags +0x10 already holds bits 0, 28, 30), a byte diff of chr, the module bag, PlayerCtrl,
  its +0x50 block and BehaviorTurn. `HksEnv::act` writes the same fields for 31 ids; Sv1 writes none
  the exe left. What they do (corrections to the mapped table above in bold):
  flags +0x10 bits: 9100 bit 0, 9102 bit 1, 150 bit 14, 2018 bit 16, 3011 bit 20 (+ PlayerCtrl +0x350
  = 2), 3018 bit 21 (and 22 for 0; it also moves the physics homing target +0x288: gap), 3032 bit 26,
  138 bit 28, 3035 bit 29, 3036 bit 30, 3037 bit 31; 3016 flags +0x20 bit i; **3034 flags +0x28 bit
  type**; 9103 flags +0x14 = v; 9101 request +0x188 bit 0 and FdpDataEx +0x11 = 0; **101 PlayerCtrl
  +0x258 bit 0 = bool**; 141 ctrl flags +0x51d = type, knockback +0x44 = 0x101; 154 chr +0x11cc;
  160 / 161 throw +0x79 / +0x7b; 2015 BehaviorTurn +0x78 / +0x7c (deg -> rad); 3029 talk +0x14;
  **3026 / 3027 / 3028 docking +0x8a0 / +0x8a1 / +0x8a2 = 1**; **3023 docking +0x7bc + 12 x type =
  the anim** (types 2-5 also physics +0x3c0 = anim and +0x3d0 / +0x3e0 fractions from its root
  motion: gap); **159 TurnTowardAttacker: damage +0x50 normalised in place, physics yaw =
  atan2(x, z) + deg** (no hit direction: 0 + deg); **3025 TurnToDirection: physics yaw += deg**;
  both wrapped to [-pi, pi] (pi stays pi); 2002 AddSpEffect: GetSpEffectID answers 1 after it.
  Nothing in those modules on idle Wolf: 123, 127, 147 (menus), 3030 (HUD), 155, 156 / 157 (the
  auto-homing module, bag +0x108), 135 / 136 / 139 (no throw), 2019 / 3019 (not locked on), 2024,
  3004 (no lock target), 3041.
- gap: the acts in other states (in a throw, locked on, a wire target), the SpEffect's
  behaviorRefId in `act` (0; `from_world` looks it up), which act sites clear the bit words each
  frame (the snapshot's bits stay set between frames in the emulator).

### HKS engine surface, step 3: the copied VM's env / act calls (2026-11-04, row E9)

sekiro-rs's VM (crates/hks) hands each `env(id, ...)` / `act(id, ...)` to RSekiro through
`hks::VmHost` (src/hks.rs): the id (a number, or a string the VM passes as `CommandId::Name`), the
first argument as the answer's key, the answer as the number the script sees. Test file
src/sim_tests/emu_hksvm.rs holds that path to the exe's own (no new emulator run):
- Names (`copied_vm_resolves_named_ids_like_the_exe`): every string id in all 81 compiled scripts,
  three names (24 sites), against the exe's name maps in `hks_env.json`. The exe's resolvers
  FUN_140b362d0 (env, map [0x143d85568]) and FUN_140b34ca0 (act, [0x143d85558]) are called by the
  string-id thunks FUN_140b35ac0 / FUN_140b31f40 before the dispatchers; a name not in the map is
  9999 (both dispatchers' default: env 0, act nothing). Fixed: c0000's
  `..._寿命延長を厳密に取得` went to 3036 (the plain ref check), the exe's map gives 3064, the strict
  check; c0000_cmsg's act `ワープタイプ入力受付リセット` went to -1, the exe's is 3041; c5040's env
  `アニメが終了か` is in no map (9999, answer 0; RSekiro's -1 also answered 0).
- Answers (`copied_vm_passes_the_exes_env_answers`): 553 exe answers (hks_env.json's 330 idle cases,
  hks2_env.json's poses) put into `Host` under the case's id and first argument come out of the VM
  unchanged. Fixed: the dispatcher returns an integer in every case (floats as (int)(x * 1000)), and
  `VmHost` passed RSekiro's fractions through: DT (333) read 16.67 at 1/60 s where the exe's is 16
  (c0000_define's dT, the auto-aim timer). `VmHost::env` now truncates as the exe.
- Acts (`copied_vm_routes_the_exes_act_ids`): the 129 cases of hks2_act.json reach `Host` with the
  exe's id (3041 for the string id).
- 3064 itself (case 0xbf8): a worn entry with the ref and every such entry's row with enableLifeTime
  (SpEffectParam +0x16b bit 7), and FUN_140c064a0(entry) false or ctrl flags +0x8c bit 6; RSekiro
  answered it with the 3036 rule. Now hks_wolf.rs sets 3064 per ref from enableLifeTime and
  `HksEnv` (hks_env.rs) does the same. The aging SpEffect 9620 (ref 591) has enableLifeTime 1, so
  the aged idle's end reads as before. gap: the FUN_140c064a0 term is the entry's runtime state
  (+0x1c & 0x80000003, +0x24, +0x25), taken as passing; the numeric id's conversion (the scripts
  pass integer constants only), and the trial `HksPlayer` (hks_lua.rs, decompiled Lua in mlua), which
  still answers names with -1, are not checked.

## The pipeline and how the game picks clips (from 2026-10-20)

How the game picks Wolf's states and clips, and what our test pipeline runs. Dated entries,
oldest first.

### 2026-10-20 The pipeline (tests only so far)

- Script: the compiled `action/script/c0000*.hks` in sekiro-rs's HKS VM (`sekiro-hks`,
  dev-dependency), `src/hks_lua.rs` `HksVm`; the decompiled `hks_src/*.lua` in Lua 5.1 (`HksPlayer`)
  runs every call beside it as a check (0 calls differ).
- Graph: `src/hkb_runtime.rs`, sekiro-rs's behaviour-graph runtime adapted (CREDITS.md), on
  `extracted/chr/c0000.behbnd.d/c0000.hkx` parsed by `sekiro_formats::hkb`.
- `ScriptChr::tick`, one frame: `Update()`, every active state's `_onUpdate`, the
  `_onDeactivate` / `_onActivate` hooks of the states the events moved until the graph settles
  (8 rounds at most), the ended clips' events, the clips' advance, the ends that advance reached.
  The script's events and SetVariable values go to the graph after each call; the graph's active
  nodes answer `hkbIsNodeActive`.
- Engine answers that belong to the graph: env 339 (anim ended) is the graph's clip (the hook's
  own state's, else the full-body clip), not the sim's.
- Clip lengths: `sekiro-extract clip-lengths extracted/chr c0000 extracted/clip_lengths/c0000.json`
  (every clip in c0000*.anibnd, hkaAnimation duration; tools/extract.ps1). The combat export keeps
  only the states in its list (1176 of the graph's ~2040), so clips like the map entry
  a000_700010 were missing; a clip with no length never ends.
- The game starts Wolf with the map entry (c0000.lua InitState: W_GroundMapEnter a000_700010, or
  Crouch / Swim / Dive / Aging versions by env 3061 / 3055); our sim starts him standing.

### 2026-10-20 How a CMSG picks its clip (exe)

- offsetType (CustomManualSelectorGenerator enum, Smithbox HKX2 templates): 0 none, 11 idle
  category, 13 right weapon category, 14 left weapon category, 15 anim id offset, 16 hand style,
  17 throw category (Sekiro; Paramdex ThrowParam atk/defAnimOffset say so).
- FUN_1411e6b40 -> FUN_1411e9670 -> callback FUN_140b417a0 -> FUN_1409f1830: for 13 / 14 the hand's
  row (13: the combat art's row on the right): its spAtkcategory N when the chr has `aN_<animId>`
  (FUN_140987ad0), else the hand's wepmotionCategory (category array chr+0x120, the env 225 one).
  17: the throw's ThrowParam atkAnimOffset (+0x76) / defAnimOffset (+0x78).
- FUN_1411e6b90: the clip named `a%03d_%06d`, else `a000_<animId>`, else the CMSG's last clip.
- So the katana combos stay a050 (no a1xx twin) while GroundSpecialAttackCombo1 plays the art's
  a100..a110; tools a070..a079 by the tool's wepmotionCategory. sekiro-rs's fixed {13: 50, 14: 70}
  with a first-clip fallback only matched the default art and tool.
- gap: the idle category array (slot 2) and its +300 candidate; offsetType 15 for NPCs.

### 2026-10-20 Locomotion variables (exe FUN_140b4afd0, from FUN_140b49d30)

- Written each frame through a 91-name table at 0x143b08e30 (slot 0 IsLockon, 1 MoveAngle,
  2 MoveSpeedLevel, 3 MoveDirection, 4 TurnAngle, 29/30 JumpAngle / JumpStickLevel, ...).
- MoveSpeedLevel: the camera-rotated stick's length, 0..1; 2.0 while the sprint is held (hold
  timer past 0.4 s); capped by a per-chr mode (1: 1.0, 2: 0.1, 3: 0). No smoothing here
  (MoveSpeedLevelReal is the script's ConvergeValue).
- MoveSpeedIndex is the script's (_SpeedUpdate, c0000_transition.lua 4882): to 1 above 0.9, back
  to 0 at 0.7 or below.
- MoveAngle: the stick against the facing, degrees, right positive, 0 with no stick. TurnAngle
  (FUN_140b4c600): manip+0x24 in degrees (source inferred). MoveDirection: the script's
  (_MoveDirectionUpdate: |a| < 55 F, > 125 B, else L / R by sign).
- Ours: player.rs speed_level 0 / 1 / 2 (walk / run) converged at 3 or 6 per s, and the stop
  check `speed_level > 1.2`, is a different scale from the exe's (2 = sprint held). Open.
- The script's GetLocomotionType reads the walk / run clip's TAE refs (90 MOVING_WALK, 91
  MOVING_RUN): player.rs's sp_ref_active now reads the shown clip's TAE (the procedural walk,
  run and idle clips too), not only `a.anim`.

### 2026-10-20 Lockstep result

hks_lua::lockstep: 33 runs of 60 frames (combos, guard, air, arts 5300 / 5600, tools 71000 /
72000, step, step-attack, crouch-attack, gourd, walk / run and stop, run-attack, run-jump): the
clip the game's script + graph plays equals player.rs's shown clip on 1969 frames; 11 frames the
game is one frame ahead at a clip's end (sekiro-rs ends a clip in the same tick its advance
reaches the end; when Havok sends the end event is not traced yet); 0 differ.

### 2026-10-20 Enemy script (c9997 + c1020 on c1020's c9997.hkx)

hks_lua::enemy_lockstep runs the shared NPC script and the soldier's own on its graph beside
enemy.rs. Engine answers, from the sim:
- The AI's request: env 106 (attack id, 9910 guard, 9920 general stance); the cancel slots 107 /
  120 / 118 while the clip's TAE window (flags 23 / 86 / 79) is open. The request stays posted
  until the script plays it (enemy.rs starts it the frame it asks).
- env 2000 (can cancel into movement) = the clip's flag 78 "End If AI Move Queued" (gap: the
  exe's NPC side of 2000). TurnAngle from its facing to the requested point (else Wolf).
  env 1114 IsExistAnime from the chr's clips. MoveSpeedLevel / MoveAngle from the move request
  (gap: the exe's NPC writes are not traced).
- TAE reads (windows, SpEffects) are of the script side's clip, as the game's engine reads the
  clip its own graph plays. The AI thinks before the script runs: the sim steps first.
- A hit (Enemy::damage_env, set in combat.rs): env 202 type (0 plain hit: gap, 3 guard, 1001
  guard break, 1027 posture break, 1019-1021 its swing blocked, 1008-1010 deflected, 1033 the
  deflect that broke it), 236 level (0 for its own met swing), 222 side (0 left, 1 right,
  2 front, 3 back: GetDamageDirectionOffset onto DamageSmall a000_008000 front / 1 back), 3032
  back, 3056 / 3057 guard sides, 3018 / 3019 bound sides, 258 NpcParam guardLevel.
- Throws: the engine fires W_ThrowAtk<id> / W_ThrowDef<id> and W_ThrowDefDeath<id> into the
  graph itself; env 1112 on the first frame (ExecPassiveTransition leaves the clip to the throw,
  before ExecDeath), 206 in a throw, 273 the id. The script's own 276 path would fire
  "W_ThrowDefDeath" .. ThrowID = W_ThrowDefDeath12000, which the graph does not have (it has
  12001): the death clip's start is the engine's (inferred, as sekiro-rs duel.rs).
- After a death clip the script goes to DeathIdleDefault (a000_010001, one frame, TAE 228
  RagdollReviveTime: the body is ragdoll); enemy.rs holds the death clip's last frame. Counted
  apart ("ragdoll"), not as a difference.

Result (6 runs; frame 0 is the script's own InitState -> W_IdleBattle):
- the enemy fighting alone, 900 frames: 899 the same;
- Wolf attacking every 45 frames (flinches, its guard 3100 and GuardDamageSmall /
  JustGuardDamage), 600: 599;
- Wolf holding guard (AttackNoBoundEnemy, the additive AttackGuardBound_Add01, its grab
  ThrowAtk4100), 900: 899; tapping guard (AttackBoundEnemy), 900: 899;
- a 2400-frame mixed fight: 2399;
- posture break (TrunkCollapseFront a000_008300) and deathblow (ThrowDef12000, ThrowDefDeath12001),
  480: 376 the same, 103 ragdoll.
No enemy.rs reaction differed from the script once the engine answers were right.

#### Every regular enemy (survey, 2026-10-20)

`the_games_enemy_scripts_beside_enemy_rs_for_every_regular_enemy` (ignored; `SHINOBI_LOCKSTEP_CHRS`
picks chrs, space-separated; `LOCKSTEP_DEBUG=f,f,...` prints those frames' clip, flags, request,
TurnAngle and 3036 refs) runs 30 regular chrs for 1200 frames each: c9997 + the chr's own
c<chr>.hks on its own behbnd's c9997.hkx, Wolf attacking / guarding on a timer. All 81 c*.hks
are decompiled into extracted/hks_src (DSLuaDecompiler, as c0000 / c9997). Harness answers added
on the way (each was the harness, not enemy.rs):
- AnimOffsets offsetType 15 = the chr's anim set (anim_group * 100): every NPC CMSG in c9997.hkx
  is type 15 (c1020: 264 of them), so the a100 enemies play a100 clips.
- env 330 = the AI's emergency turn; env 1000 HP reads 1 at 0 HP while not dead (IsNoDead);
  env 201 grounded is off while the hit was aerial; IsExistAnime has the TAE aliases too.
- The AI-state SpEffects 200000 / 1 / 2 / 4 are one slot (spCategory 100, categoryPriority 1):
  the AI's state replaces a NpcParam resident one. 51 NpcParam rows wear 200000 from spawn (the
  piranha c1320): with both on, UpdateAIState (DEFAULT first) picked TurnDefault.
- TurnAngle below 0.001 degrees is 0 (float noise made an emergency ExecTurn turn 90).
- env 275 = Wolf escaped its grab (his ThrowEscape) -> ThrowAtkCommonFunction's
  W_ThrowRelease<ThrowID>. env 273 follows the throw's state byte (case 0x111 ->
  FUN_140bdfb20): atkAnimId / defAnimId, and once escaped atkAnimId + 2 / defAnimId + 3, which
  sets ThrowID to the zombie's 4122 (its graph has W_ThrowRelease4122, not 4120).
- IsExistAnime (case 0x45a): the exe tries the id plus each anim offset first
  (FUN_140988510, animmgr +0x60..0x6c), then the raw id, so an a100 enemy's IsExistAnime(4120)
  is its a100_004120. gap: what fills the other offset slots.

enemy.rs fixes it found:
- A turn needs the move window (flag 78 or free), not only a free state, and a turn clip's end
  does not chain into another turn (c9997 TurnCommonFunction passes enable_turn FALSE): a broken
  soldier's TrunkCollapse turns to Wolf at its flag 78.
- The chr's own HKS end chains (data::hks_end_chains, parsed from extracted/hks_src/<chr>.lua):
  Event<n>_onUpdate's `if env(339, 1) == TRUE then Fire("W_Event<m>")`, under its enclosing
  `env(3036, ref)` checks; random (`fate`) and elseif ones are left out. 34 chrs have them, e.g.
  the zombie c1500's held pose 20010 -> 21015 (same pose and TAE), c1020 20002 -> 21001 /
  20004 -> 21007. This replaced the c5310-only chain for the Divine Dragon's trees (their loop
  repeat stays). Mode::Rising's loop_after guess is gone: an event state holds in its clip
  until its HKS moves it (data::hks_events).
- Event states fire before the clip's end too (2026-10-20, EndChain at_end / grounded): with no
  talk ESD running `env(3054) ~= ref` holds and `== ref` never (c1010 21003 -> 21000, c1020
  21007 -> 21005); `env(201) == TRUE` once on the ground (c1100 21005 -> 20005);
  ExecTalkTransitionIndividual's NOT_TALKING moves, run first in EventCommonFunction (c1070
  21000 -> 20001). 81 decompiled files start with a BOM, which had hidden their first function
  (c1070 Event20000_onUpdate). Harness: env 3053 = -1 (not talking), and `GetVariable("Debug<n>")`
  answers a table (c9997 DebugPrint writes `[0]` of the graph's vector variables; c1110 calls it).
  `a and b` conditions are split into parts (c1500 21008 -> 21004 under ref 1000021), and a talk
  check nested under `if anim_id == <n>` counts (c1110 21004 -> 21003). Survey hks_lua
  `the_games_event_anim_ends_and_enemy_rs_agree` (ignored, ~2 min): every a000_02xxxx event of
  the 29 regular chrs forced as a map script does, Wolf far off; the state after it matches the
  game's graph + c9997 + c<chr>.hks in all of them (was 5 off, plus a crash on c1110).
- Death picks by worn SpEffect ref first (ExecDeath): TO_DEATH_EVENT20000 + n (1020000-20) the
  one-shot Event200nn, TO_DEATH_UNIQUE_CASE 1000180 DeathStartUniqueCase 10160 (the zombie killed
  as it lets go).
- The cautious soldier now turns to a sound inside its 1010 reaction's move window and finds Wolf
  standing there (test running_behind_an_unaware_enemy_turns_it_to_wolf); before, the AI's turn
  was dropped until the clip ended. a_cautious_enemy_listens_to_a_new_sound now places its
  sounds ahead of the soldier.

Result: 28 of 30 match on every frame (ragdoll / random death frames apart; c1030 and c1210 one
frame early once at a clip end). Left: c1000 (the hidden bar holder) and c1460 (the Nightjar
kite) have only a T-pose / a few clips: the game stays in Init (a000_000000) while enemy.rs
shows its idle name. Not fighters.

### 2026-10-20 Hit answers in the exe (env 202, 236, 237, 222, 3032, 3056-3057, 3018-3019, 256)

All read the chr's action-flag module D = *(ChrIns+0x1ff8) (env switch FUN_140b36410, 140b3.c);
FUN_140b27c20 resets them each frame before the script runs (D5c = 99999, the
directions -1; ordering inferred). Static research, not traced live.
- 202 = D5c. Defender (FUN_140b6bd40, 140b6.c ~6480-6590), first match: dead in the air 9, dead
  2, rec+0x48 0x3f 1002; guard break 1001; guarded 3 (1028 GUARDATTACKER_STAMZERO when the
  AtkParam doesBreakRepelStamDamage bit is set), a deflect 3 too (1028 likewise), a hit's break
  1027, a plain hit **99999** (c9997's DAMAGE_TYPE_NONE; enemy.rs's DamageEnv kind 0 is a guess).
  Attacker (FUN_140b698d0): deflected 1000 GUARDED (category 2: 1003, 6-8: 1008-1010), blocked
  1017 (category 2: 1018, 6-8: 1019-1021), its posture emptied 1033.
- 237 = a 5x12 table at 0x143b01bc0 [guardLevel][damage level] (FUN_140843820): the guard
  reaction size (0, 1, 3, 4). guardLevel 0 (an unguarded hit) all 0; rows 1-4 by level 0-11:
  `0 1 3 3 4 1 3 3 0 3 4 3`, `0 1 1 3 3 1 3 3 0 3 4 3` (rows 2 and 3), `0 1 1 1 3 1 3 3 0 3 4 3`.
  Wolf's guardLevel is his weapon's EquipParamWeapon.guardLevel; 236 is the remapped level.
- 222 = an 8-way sector (FUN_140b6b350, 45 degree steps in x/z from the facing and the hit
  direction rec+0x150) folded to 4: sectors 0/7 front 2, 1/2 right 1, 3/4 back 3, 5/6 left 0
  (side labels inferred). 3032 = 1 (back) for sectors 2-5.
- 3056 = the AtkParam's deflectAction when guarded, 3057 its justDeflectAction on a deflect (-1
  otherwise). 3018 / 3019 = the attacker's deflectedAction / justDeflectedAction.
- 256 = a hit that dealt damage > 0 this frame (a fully cut block: 202 = 3 but 256 off).
- 1119 = the AtkParam's atkDir_forSfx; 1121 = the knockback magnitude (int), negative only for a
  flipped push (its source rec+0x18c not found). 1007 = a behaviour object state == 6 (unnamed).

### 2026-10-20 Live mode (SHINOBI_HKS=1)

`src/hks_wolf.rs` `HksWolfPlugin`: Wolf's script + graph run in the game, not only in tests.
`src/hks.rs` holds the runner (`ScriptChr`, `HksVm`); `hks_wolf.rs` the answers and the live
systems; `hks_lua.rs` keeps the test-only lockstep and the Lua 5.1 check.

- Order (FixedUpdate): `tick_script` before `ActorSet::Decide`, `apply_script` after it and
  before `Advance`. player.rs still moves, collides, hits and uses items; where the graph plays
  another clip than player.rs picked, the graph's state and clip win (`sync_clip`).
- Both just standing or moving (`player::is_free` and `graph_free`): player.rs keeps its own
  locomotion. player.rs names a state by its CMSG (`rt.clip_cmsg`), the graph's state can hold
  several (GroundJumpStart).
- States player.rs or combat.rs start go into the graph as the exe's own events:
  W_ThrowAtk / W_ThrowDef / W_ThrowKill / W_Event + the anim number, then the grab's
  `W_<state><ThrowId>` (W_ThrowDefDeathStart600000; the grab also sets Selector_ThrowId), then
  `W_<state>` where the graph does not show it yet. A new anim in the same state (the throw kill
  510200 -> 510201) is tried as the numbered events only. Nothing matches: the graph stands aside
  until both show the same clip.
- A throw the hit started (the breaking deflect's pose) replaces that frame's hit: the script
  does not get the hit too. Any other reaction to a hit is the script's own (it gets the hit's
  answers first); only where the graph still differs after the tick is player.rs's state fired.
- Landings: player.rs finds them after the script ran, so the graph keeps the fall one frame and
  the script lands itself on the next (env 248 IsTrulyLanding, 224 GetFallHeight from the
  flight's highest point): BEH_R_LAND sets Selector_LandStiff (20 m: a000_200021).
- player.rs back to standing or moving where the graph still plays an action (a respawn, a
  cancel of its own, a CMSG with no end event held at its last frame): W_Idle / W_CrouchIdle.
  player.rs's clip ending this step is left alone (the graph ends a step earlier).
- New answers: 206 / 276 (in a throw, killed while grabbed), 224 / 248, 3025 (IsSwimContact),
  3033 per unlock type, 3036 with the art's resident SpEffects, 345 0 with no art, 1108 also for
  held attack / dodge / prosthetic, the prosthetic button as both ACTION_ARM_SUB_ATTACK and
  ACTION_ARM_SHINOBI_WEP_ACTION (the umbrella's release wants both held), the X sheathe as ref
  600 (a non-combat area: the game has no sheathe outside one) until a draw starts.
- items.rs BEH_A_ITEM_USE map from the HKS: 30 / 34 ItemPowderMedicineRecoverPoison, 80 / 81
  PaperDollExchangeWhite, 84 ItemAntiHallucinogen.
- Result: the sim suite passes with SHINOBI_HKS=1 as without (698 / 0).
- Cost: ~190 us a tick for Wolf (quick profile), ~97 % of it in the VM (SHINOBI_HKS_TRACE=1
  prints it every 600 ticks): ~1 % of a 60 Hz frame. Enemies on it will need the exe's LOD.
- SHINOBI_HKS_TRACE: 1 engine events, overrides and the cost; 2 both clips every frame; 3 the
  presses and the script's events.

### 2026-10-20 Enemies live (SHINOBI_HKS=1)

`src/hks_enemy.rs`: the enemy answers moved out of the lockstep test (`answers`, which the test
now calls) and `HksEnemyPlugin` runs one `ScriptChr` per enemy (c9997.hks + c<anim_chr>.hks on
its behbnd's c9997.hkx) after `ActorSet::Decide`, at the exe's character LOD (`Actor::lod_due`,
dt x lod). Where the clips differ the graph's state and clip win, as Wolf's (both standing,
moving or turning, or enemy.rs's clip ending this step: enemy.rs keeps its own);
SHINOBI_HKS_ENEMY_LEAD=0 only counts. The lockstep tests take the runner out of their app.
- The sandbox partner's walk (enemy.rs passive / drill, ours) reads as an AI command: a forward
  walk (no turn). Its attacks and any anim forced without an AI request (`force_anim`) go in as
  the engine's W_Event<n>.
- Held by its map script (enemy.rs Mode::Held at 0 bars in its 20200), or dead with its map
  script playing events on the body (Isshin's 21000 before the finisher): the graph stands
  aside. gap: how the exe keeps c9997's ExecDeath (IsDead at HP 0) off then.
- A state enemy.rs started outside the AI's requests (map events, throw ends, test pokes) goes in
  as W_Event<anim> / W_ThrowAtk / W_ThrowDef <anim> / W_<state>; none: the graph stands aside
  till the two meet. The AI's own request is left to the script (it may take a frame).
- The same clip from another anim set (a100 / a200) counts as the same. gap: where the exe's
  offsetType 15 takes another set than Actor::anim_group.
- Cost ~87 us a tick an enemy. SHINOBI_HKS_TRACE=1 prints the differing frames and a summary
  every 600 frames; =3 the hits and the script's events.
- Sim suite with live enemies: 9.6k differing frames (from 188k at the first run): c1000 (no
  clips) 3.7k, boss event flows (c5000-c5400, c7100-c7110) most of the rest. Regular enemies left:
  c1010 / c1020 WalkFrontBattle where the game plays TransToBattleFrom* (the alert transition),
  c1020 Guard where the game idles.
- With the graph leading the sim suite passes (698 / 0); 1449 frames still differ where it does
  not lead (both free, enemy.rs holding an ended deathblow clip, c5000 turning, c1020's Guard
  where the script stays idle).

### 2026-10-20 Default on

The scripts run by default (playtested); SHINOBI_HKS=0 turns both runners off (player.rs and
enemy.rs alone). Next: delete the hand-written picking the graph now overrides, in small tested
batches.

### 2026-10-20 Deleting the hand-written picks

- **Audit.** `SHINOBI_HKS_AUDIT=<file>` appends one line per player.rs pick: the call site
  (`Actor::played_at`, `#[track_caller]` on `play` / `play_state`), player.rs's state and clip, the
  graph's state and clip after its tick, and whether the two clips match (set-only differences
  count as the same). Over the suite: jumps, item use, jump arts and several air moves match every
  time; deathblows, throws, revivals, the grapple shot, idols and swimming never do (engine
  states); attack combos match about 90 %, knockdowns about half.
- **Graph-led clip ends** (`hks_wolf::graph_chains`): where player.rs's clip has played out and
  the graph already went on by itself, tick_script takes the graph's state before player.rs
  decides (take_graph, as sync_clip does after it). Deleted from player.rs: the ground / sprint /
  storm jumps' ready -> start and start -> fall, the jump arts' ready -> leap -> fall loop, the
  slide's end. The side jumps keep their raw anims (no graph clip). With SHINOBI_HKS=0 those
  chains no longer run.
- **Shared clips.** Several CMSGs can hold one clip generator (VerticalGroundJumpFall and
  HorizontalWallJumpFall share a000_201030's): hkb_runtime::owner names it by the active CMSG.
- **Knockdowns** (graph-led too): the posture-break reactions are the script's (BEH_R_BREAK_DAMAGE
  on env 202 = 1027 with the level; it sets Selector_ProneDirection from env 222), and the graph
  takes the break's damage / blow landings into StandDamageBreakDown and its get-up. Fixes it
  needed: `shows` counts the same clip as the same state (one CMSG holds StandDamageBreak_F / _B);
  a hit's reaction and what follows a clip the graph played out (the Ape grab's blow-away) are
  the script's first, synced after the tick only where the graph did not get there; env 3032 is
  the back half (DamageEnv::back_half from combat.rs), not the back quarter.
- **CMSG clips (extractor).** The exe plays a CMSG's clip named a<group>_<animId>, else
  a000_<animId>, else its last clip (FUN_1411e6b90). 55 c0000 CMSGs have no clip named after
  their animId; export.rs now maps Wolf's plain states (offsetType 0 / 11) by that rule: 31
  changed, among them the get-ups (StandDamageLargeDownWakeUp_* a000_10032x, not the knockdown's
  10031x), the air and fall death blows (112xxx, not 121xxx), the jump falls (201030),
  GroundFallReturn (700110), GroundJumpLandReady_F (201055), SprintStopQuickTurnLeftBack (1513).
- **Every graph clip exported.** export.rs also exports every clip of Wolf's graph's CMSGs that
  has a TAE (anims 1739 -> 2206, anim_c0000.bin 388 -> 525 MB); 19 more effects came with them
  (tools/fxr_extract.py). Missing clips made the graph stand aside where it led.
- **Locomotion (free) audit.** sync_clip logs "free" rows where both sides stand or move and the
  clips differ. Fixed from it:
  - crouch start: the script fires W_CrouchStart itself; the free-state sync no longer sends
    W_CrouchIdle over it where the graph already shows Wolf's state (923 rows -> 16);
  - move starts: Locomotion shows the graph's own start clip where it picks another
    (Actor::move_start_clip: StandMoveStartFromLandGroundPositioningJump a000_00041x,
    StandMoveStartFromFreeFall a000_000110, a start player.rs skipped); quick-turn starts not
    (their clips turn Wolf, player.rs turns him itself);
  - walk vs run stop by the shown clip's TAE ref 91 (MOVING_RUN), as BEH_A_GROUND_MOVE_STOP
    reads it, not player.rs's lagging speed level;
  - the quick-turn side at a full reverse: Left only where TwistLowerRootAngle > 0
    (BEH_A_QUICK_TURN_MOVE_START), so a reverse turns right; in the move Right where
    TurnAngle > 0 (BEH_A_STAND_MOVE_QUICK_TURN);
  - the turns' clips play out: Locomotion shows the rest of a quick turn's clip after
    player.rs hands over (the graph runs it to its end, then the loop); where the graph goes
    straight into the loop, Locomotion drops its move start;
  - the in-move 180 turn only from a stop's first 9 frames: BEH_A_STAND_MOVE_QUICK_TURN needs
    TAE ref 14 ENABLE_GROUND_QUICK_TURN, which only StandWalkStop_F / StandRunStopF carry (no
    exe path sets SpEffect 100313). Let go, then the stick back: the turn. Thrown straight
    back, the run swings round at 720 deg/s (FUN_1407da540's default; TAE 224 SetTurnSpeed
    overrides), about 0.25 s; test a_run_thrown_straight_back_swings_round_at_the_turn_rate;
  - sheathed, the turn clips' a010 copies: offsetType 11 is the idle category, 10 sheathed
    (AnimOffsets::idle; the exe's array at chr+0x120 slot 2 is not read).
- **MoveDirection (exe):** slot 3 of the per-frame table is NS_SPRJ::WalkTwist+0x50
  (FUN_1407f7550): the stick in 60 deg cones with 5 deg hysteresis, held at rest, -1 (written 0,
  F) while the twist is off. The twist runs locked on (FUN_1407f6ff0 / FUN_1407e7900); unlocked
  Wolf faces the stick, so F, and the 180 turns pick their *_F clips. The guard walk keeps
  the cones unlocked (inferred: the twist's on flag +0x58 is not traced). TwistLowerRootAngle
  is ~0 while running unlocked.
- **Exe answers fixed on the way:**
  - MoveDirection written per frame (the _MoveDirectionUpdate cones); the keyboard walk key
    caps MoveSpeedLevel at 0.5;
  - env 224 GetFallHeight is whole centimetres (FUN_140b97880: last grounded Y (P+0x854) minus Y x 100, kb/movement.md 2026-10-22; the
    script multiplies it by 0.01). We fed metres, so every landing read as a tiny fall;
  - env 200 (falling, FUN_140b97970): off the ground, with the physics' ground contact held
    while a probe (P+0x920) reads 1.0 or less, or after 1 s airborne under a flag (P+0x100).
    BEH_R_FALL's env 224 >= 30 cm test is in the aged branch only (Lua `and` before `or`). The
    probe is not identified: player.rs's 30 cm walk-off stands in for it;
  - env 200 is off in a throw's clip (the a2xx ThrowParam groups): the plunge deathblow was
    taken into SelfFallStart;
  - numbered exe events (W_ThrowAtk / ThrowDef / ThrowKill / Event + anim number) only where
    that state's CMSG plays that number (hkb_runtime::cmsg_anim_id): W_Event390000 plays
    a000_710000, the aged attack is W_AgingGroundAttack.
- **Input picks taken from the graph (hks_wolf take functions).** When the script fires an
  event for a press, tick_script plays the graph's state and clip and spends the press
  (Player::requests and PadInput::take); player.rs runs the on-enter effects (aim, stop) off
  `a.state != p.last_state`. Deleted from player.rs:
  - crouch start / end (take_crouch: crouch from the graph's style, Crouch* / SprintToCrouch*);
  - the step (W_GroundStep; the tilt is the script's act(3025) angle, recorded in
    Host::act_values; unlocked, Wolf faces the stick as act 2019 does);
  - items and the gourd (take_item; W_ItemGourdDrinkMove plays its *Move copy). An empty
    gourd is goods 3001 (goodsUseAnim 10 ITEM_GOURD_RECOVER_EMPTY), usable (env 233), so the
    script plays the empty swig itself;
  - the guard start, the held-button start (ref 221) and the spam chain (W_StandToDeflectGuard*,
    W_SprintToDeflectGuard). player.rs keeps the rule that drops a tap let go before its
    window (the script's start wants env 1108).
- **Prosthetic presses and releases taken from the graph** (hks_wolf::sub_press_event: the
  tool's opener, combos, guard and leap starts, the failed swing; W_*SubAttack*Release on
  let-go). player.rs keeps the held repeat, the attack-button derives, the walking upper-body
  moves and the clip-end chains. Found on the way: the equipped tool's resident SpEffect
  (the upgraded Flame Vent's hold, the level-1 quick use) was missing from env 3036.
- **Ground jumps and the damage / down steps taken from the graph** (W_GroundJumpReady /
  Start, W_SprintJump*, W_GroundStormJump*, W_SwampJump*; W_StandStepFromDamage / FromDown, tilt
  from act 3025). The side jumps' CMSGs have anim 0: the graph sits in GroundJumpReady over
  a000_201102 / 201103, named Leftside / RightsideGroundJumpReady for player.rs. On entering,
  player.rs sets the launch, landing and tilt bookkeeping. The jump-over throw (ThrowParam 0200)
  goes first (throw_check's jump_throw_ready). player.rs's ground jump block is gone.
- **Air actions taken from the graph** (BEH_A_AIR_*: W_AirKick, W_AirStormJump(Weak)Start,
  W_AirSpecialAttack*, W_AirComboAttack*, the lightning thrown back, the Mist Raven's and
  Shadowfall's derives, W_AirDeflectGuardStart; W_AddHardDeflectGuard on the ground and in the
  air sets the additive layer; W_StandDeflectGuardFromDamage). Not on the wire (wire.rs). The
  plunge throw goes first (throw_check's plunge_ready); the wall jump is docking.rs's
  (player::wall_jump_open: env 3020 wall contact is not answered, so the kick is left alone
  there). Found on the way: the lightning charge's SpEffect (9495 / 9490, refs 151 / 356) was
  missing from env 3036. The air counters (g_airSpecialAttackCount, g_airSubAttackCount) are the
  script's; player.rs's air_art_state / storm_jump / hks::air_press are gone.
- **Recoils the graph names by selector.** player.rs's StandDeflectEasySmall_V1_F has no graph
  state: it is W_StandDeflectEasySmall with Selector_DeflectVariation 0 and Selector_DeflectDir
  0. engine_sync points every manual selector above `<state>_CMSG` at it
  (HkbRuntime::select_path) and fires the bare name, so the graph is in the recoil and the
  script's spam chain and additive deflect run there.
- **Attacks taken from the graph.** The attack button's events (W_GroundAttackCombo*,
  W_GroundStepAttack, W_SprintAttack, W_CrouchAttackTo*, W_HardDeflectAtk, W_EasyDeflectedAtk,
  W_DeflectGuardAttack) and their W_*Release on let-go play the graph's state and clip;
  player.rs's attack_route / release_of are gone, it aims (_StartAutoAim) on entering. The
  exe's throw check goes first: player::throw_check runs before tick_script and, with a
  deathblow in reach, the press stays for decide's deathblow.
- **Combat arts taken from the graph.** The art's openers (sprint, step, leap, hold start,
  NoResource), its combos and finishers (_FireSpAttackCombo) are the script's events
  (hks_wolf::art_press_event); the art key's or the attack press is spent, and the paired
  guard tap. On entering an opener player.rs records the emblem state for the hit jump and
  Sakura Dance's air use. art_start_states / art_combo_next / art_combo_pick are gone. The art
  let go (its Release, the hold's draw and end, the forced releases of base Mortal Draw /
  Praying Strikes) is the script's too (take_graph); the hold's start runs into its loop by
  the graph's clip end (graph_chains).
- **The additive layer's SpEffects count** (env 3036 and sp_ref_active): AddHardDeflectGuard's
  411 with the hard recoil's 412 starts the guard with the tap spent (HKS 6161).
- **Deflect end taken from the graph** (BEH_A_DEFLECT_GUARD_END, W_DeflectGuardToStand*: the
  plain end, or over the walk / run DeflectGuardToStandMoveWalk / Run by MoveSpeedIndex);
  player.rs only sets the move index and speed level on entering the move end.
- **Sprint taken from the graph.** The held step's ref 15 window is W_SprintStartFromStep: the
  graph's Selector_GroundJumpType picks the start by the step's direction (a000_001150-154);
  graph_state names it by its CMSG (SprintStartFromStep_R) when that clip is Wolf's. The start
  runs into SprintLoop and the loop restarts at its clip end (graph_chains; a loop on the same
  clip wraps our clock to the graph's). W_SprintQuickTurnReady, then at its end Left / Right180
  by TurnAngle; W_SprintStopReady, then at its end SprintStopQuickTurn* by TurnAngle
  (transition.lua 569-592). Dodge let go while the stick is still pushed runs on
  (W_StandMoveStartFromSprint, BEH_A_GROUND_MOVE_STOP): that needs env 1105 true in SprintLoop
  (g_paramHkbState: STATE_TYPE_STANDBY). Past the quick turn's move window the held dodge
  steps into a new sprint start, as BEH_A_SPRINT does without ref 15 (_fireGroundStep).
  player.rs only steers the start and the loop and feeds the run's held-dodge step.
- **Landings taken from the graph** (BEH_R_LAND). player.rs finds the ground inside decide,
  after the script, so hks_wolf::land_script ticks the script again on that frame (no time
  passes), as the exe's ground check comes before its HKS update; tick_script and land_script
  share run_script. The landing tick answers with the last airborne pose (state, clip, time),
  so ref 201 and env 3063 (the clip's time in ms, which the script copies to StartTime_00, the
  Land clip's start) are the air clip's. A landing counts as the script's when it fires a land
  event, the move start out of a fall, or the step out of it (that spends the press). Not
  taken: the exe's lands (the plunge deathblow, the fall-death return), ledge catches, the aged
  form and a state set outside decide. player.rs keeps the fall death, the lightning shock,
  the dodge request, and fallbacks: a blow-away the graph is not in (the hit did not reach the
  script as a blow), the jump arts' lands while ref 103 holds BEH_R_LAND off (Sakura Dance
  without emblems; env 3008 not traced), else LandFreeFall / Locomotion. engine_sync also fires
  the graph state holding a CMSG-named state (VerticalGroundJumpFall in GroundJumpFall,
  HkbRuntime::holding_state). The long-stiff land is named LandFreeFallLongStiff (its CMSG).
  A sprint jump keeps the step's Selector_GroundJumpType (only the ground jump sets it), which
  picks its LandForward / LandVertical.
- **Fall starts taken from the graph** (BEH_R_FALL on env 200: W_SelfFallStart,
  W_SelfCrouchFreeFallStart, W_SubGuardEndSelfFallStart). The game's fall start (SelfFallStart,
  a000_200000) now plays before FreeFall, which the graph's clip end leads to (graph_chains).
  player.rs keeps the walk-off's carried move and the void fall.
- **Prosthetic ends, derives and releases taken from the graph.** Every tool state's clip end
  is the graph's (graph_chains: the held loops and their ends, the Mist Raven's leap by the
  stick through the script's _SetJumpDirection, the air falls). The flat leaps end on the floor:
  env 248 is answered for them (HKS 557 W_LandAirSubAttackMove); the upward one goes on in
  the air (player.rs lifts Wolf off on entering AirSubAttackMoveStartToLoop). The attack button
  out of a tool move is the script's BEH_A_GROUND_ATTACK derive (hks_wolf::sub_derive_event,
  the attack press spent); the let-go is the existing sub release take. Removed from
  prosthetic::hks: derive_attack, derive_unlock, release(s), jump_start, air_move_start.
  The held repeat (flame) still goes through player.rs's press.
- **Air clip ends taken from the graph.** Airborne (not on the wire, hanging or aged), every
  clip end is the graph's end transition (hks_wolf, `air_chain`): the jumps' and the storm
  jumps' fall loops, the air art's start into its loop, the electro charge's loops, the blows'
  fall loops, the air guard's loop. The air guard let go is W_AirDeflectGuardEnd (taken).
  player.rs's air clip-end picks are gone; it keeps the walk-off's move loop, the exe's grab
  blow-away, else the last frame holds until the landing.
- **Ground clip ends taken from the graph.** On the ground, every clip end is the graph's end
  transition too (hks_wolf, `ground_chain`), except in the exe's own states (throws, deathblows,
  events, deaths, the idol, the hang, swim, wire, cover, docking, revival, falls, the aged form).
  A graph idle (StandIdle / CrouchIdle) becomes the procedural StandIdle; a free graph move with
  the stick pushed becomes the procedural Locomotion. player.rs's generic ground end
  (DeflectGuardIdle / Locomotion / StandIdle) stays as the fallback for the excluded states.
- **Guard idle and walk taken from the graph.** A guard start or reaction ending with the guard
  held goes to the graph's DeflectGuardIdle, which is player.rs's guard idle (it picks the walk's
  side). Moving or stopping in the guard style is the script's W_DeflectGuardMove /
  W_DeflectGuardIdle (HKS 2340 / 2421), taken the same way. player.rs's to_guard end and its
  guard-move pick are gone; its ground clip end is only StandIdle for the exe's own states. Out
  of an upper action, the chain's Locomotion keeps the legs' phase.
- **The run out of a sprint.** When the graph reaches its own move loop (StandMoveLoop /
  CrouchMoveLoop) out of a graph-led clip (StandMoveLoopFromSprint's end), Wolf goes to
  player.rs's Locomotion on the graph's clock, before player.rs moves him that step. Shown as
  the graph's clip, the stick did not steer and Wolf ran on one way for the clip's whole length
  (test the_run_out_of_a_sprint_steers_and_stops).
- **Engine answers from the script's own tables (2026-10-21).** env 1105 is any STATE_TYPE_STANDBY*
  state of g_paramHkbState (c0000's onUpdate calls act(9100) in each; player::is_standby): the
  sprint's walk-out StandMoveStartFromSprint / LoopFromSprint are ones, so letting go there runs
  BEH_A_GROUND_MOVE_STOP. env 3036 also counts the lasting SpEffects (exe FUN_140b32410 case 0xbdc
  walks the chr's active SpEffect list): a TAE row's replaceSpEffectId after its window, when the
  row's behaviorRefFlag_checkAliveFlagForBehavior is 0 (data::sp_effect_lingering_ref). The
  sprint's 100304 (ref 1 ENABLE_SPRINT_ACTION) ends into 100303 (ref 1, 0.2 s), so the first
  frames of the walk-out still stop as a sprint (W_SprintStopReady). Wolf's own residents (the
  aged form's ref 591 row) count too. The one named env, the strict aging check
  ("特殊効果発動中か_Behavior参照ID_寿命延長を厳密に取得"), is env 3036 (hks.rs). env 3063 takes a layer:
  0 the shown clip, 1 the upper body (an upper action's clip, else the full-body one: the stop of
  the guard's run-out, W_StandMoveableAction, goes on in DeflectGuardToStand at its time through
  StartTime_01, so it ends soon after), 2 the legs' loop.
