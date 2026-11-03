# guard-deflect: Guard, deflect, posture, mikiri, deathblow, perilous attacks

Dated findings, oldest first. Append new entries at the end.

- 2026-10-06 GHIDRA: posture DAMAGE confirmed. Attacker FUN_140842790: unguarded directAtkStamDamage_Attacker,
  guarded repelVictoryStamDamage_Attacker, deflected repelLostStamDamage_Attacker x defStaminaAttackRate of defender
  SpEffects with stateInfo 204 (FUN_140bfc490). Defender FUN_140847d50: direct / atkStam / repelLostStamDamage, final
  FUN_1408439a0 = base x hit mult x guard cut (vfunc 0x200) x attribute rate (0x248) x SpEffects. Hit record flags:
  +0x164 guarded, +0x1C7 deflected. The prototype's mapping matches; gaps 1 and 2 mostly closed.
- 2026-10-06 WOLF: i-frames from TAE 954 (IFrameType 2 General, 6 Thrusts, 7 Grabs, 1 Sweeps = judge 980)
  in combat.rs dodged(); mikiri (flag MikiriCounter vs perilous thrust) wins over thrust i-frames. Dodged hits are
  not consumed, so a step pressed too early is still hit by the live hitbox after the 0-9 frame window (as in game).
  Test timed_step_dodges_through_the_slash (step 1 frame before the window). 36 tests pass.
- 2026-10-06: NEXT 1 done (camera from LockCamParam row 0: dist 4.5, FOV 43, focus 1.5 m, pitch min -40, lock 30 m).
  NEXT 2 done (hit stop: AtkParam hitStopTime / _Defencer, 0.1 s on player hits).
  NEXT 3 mostly done: enemy defence from its AI (Goal.Parry in ai/102000_battle.lua, decompiled from script/*.luabnd):
  player attack notify = TAE flag 63; enemy picks 3101 deflect (SpEffect 200220, stateInfo 158, 12 f) or 3100 guard
  (block 0-12) via parry rank (resident 221000), thrust ref 109970, consecutive-guard count (clash 200215/6 up,
  200210/1 reset). Deflect test now = stateInfo 158 for both sides. Full reaction matrix from c0000/c9997 HKS
  (Hard = deflected, Easy = blocked; sizes by dmgLevel; justDeflectedAction 1/2 bound R/L, 11-14 additive).
  Enemy HP-conditional posture regen: resident SpEffects 300600-2 staminaRecoverSpeedRate 0.6/0.5/0.333 (multiplied).
  NPC states from c9997.hkx (offsetType 15). Remaining for 3: enemy super armour/toughness.
- 2026-10-06: headless combat simulation tests (src/sim_tests.rs, cargo test): timed guard -> deflect + enemy
  AttackBoundEnemy1_Right; late guard -> block; 4th spammed press -> no deflect; no guard -> hit; first player
  attack -> enemy guards (count 0). Found + fixed: release attack clip starts at frame 0 (quick slash = 6 + 7 frames).
  Blade wind-up telegraph until real animations land.
- 2026-10-06: NEXT 8 (part 1) done: perilous attacks, mikiri, deathblow throws - all from data.
  Perilous = AtkParam disableGuard_vsGuardAttribute0 (unblockable) / disableJustGuard_vsGuardAttribute0 (undeflectable):
  c1020 thrusts 150/151/200/642/643 (atkType 2, deflectable), sweeps 140/570 and grab 220 (neither).
  The red kanji = TAE BulletBehavior (type 2) judge 980 sweep / 982 thrust / 983 grab, ~frame 5 of the windup
  (hud::Perilous, Yu Gothic from C:/Windows/Fonts). Mikiri: player TAE flag 125 MikiriCounter (GroundStep_F frames 0-12)
  + perilous thrust -> ThrowParam 11020190 (player a201_511800, enemy ThrowDef13800); AtkParam 940 "見切られダミー" is the
  parallel mikiri hitbox; attacker posture += staminaDamageAttackHitParry (90). Deathblow: ThrowParam 11020001
  (崩し, Dist 2.1: a201_510000 / ThrowDefDeath12001) or 11020010 (弾き = broken by deflect, Dist 4.0: a201_510100 /
  ThrowDefDeath12111). Throw anims live in c0000_a2xx.anibnd (now unpacked); 990-992 are only the deathblow shockwave.
  The 弾き pair needs the attacker in that row's reach (Dist 4.0, upper / lowerYRange 2.4 m; c1450's 4.14) and a
  melee deflect: a shooter broken by its own deflected bullet staggers where it stands (AttackBoundEmptyStamina)
  instead of being pulled onto Wolf (combat.rs throw_in_reach, 2026-10-12).
  An enemy grab absorbs as a deathblow does: Wolf onto the grabber's atkSorbDmyId (230 / 231, the General's
  21020000) over adsrobModelPosInterpolationTime 0.5 (ThrowHold from combat.rs, player.rs follow_throw).
  The break pose's jump (0012 / 0122) is a vault round behind (ThrowDef12120 / 13120), not a kill.
  Tests: perilous thrust breaks a block / can be deflected, sweep can't be deflected, step-in = mikiri, deathblow pair.
  Open in NEXT 8: neck grab (enemy 4100 ThrowAttackBehavior judge 230 -> player a210_600000, clip not in a2xx),
  sweep jump-stomp, guard walk, air deflect, multi-life deathblow (NpcParam lives).
- 2026-10-06: NEXT 8 (part 2) done. Guard walk = DeflectGuardMoveF/B/L/R (a050_0022xx, root 2.0 m / 40 f = 1.5 m/s;
  locked: strafe, free: turn + forward; ShieldBlock flag 3 stays on). Air guard: AirDeflectGuardStart/Loop/End on the
  jump anims' flag 117 (frames 3-33), land in LandAirDeflectGuard; air deflect/block reactions AirDeflect{Hard,Easy}
  {Small,Large}_{L,R}/ExLarge with their own TAE 920 push (row 4600). Kick-jump off an enemy: Jump in the air (flag 119)
  near a body -> AirKickEnemyJumpStart_F (flag 27 SetNoGravity f0-6, TAE 920 row 2101). Air attack AirComboAttack1.
  Neck grab: AtkParam throwFlag 1 (220) -> enemy ThrowAtk4100, its ThrowAttackBehavior (TAE 304, judge 230,
  throwFlag 2, 192 phys) lands at frame 45 (combat::resolve_throws). Exporter now resolves type-304 judges + throwFlag.
  Gaps: player grabbed anim a210_600000 has a TAE but no clip in the archives (player held still); kick-target
  test (1.4 m / body height); kick posture damage; multi-life deathblows.
- 2026-10-06: feel pass. Enemy super armour: NpcParam toughness/superArmorDurability are 0, AtkParam atkSuperArmor 0;
  NPC HKS ExecDamage passes rank NONE in attacks, so the engine's damage level gates flinching. TAE ChrActionFlag 73
  ("SetBool32_0x7C_10") is on in idle/reactions and only in the first ~9-13 windup frames + recovery of every attack,
  never on hit frames -> used as "damage motion allowed" (inferred; test enemy_mid_swing_...). 2026-10-11 exe: flag 73 is the
  opponents' ParryTiming gate, not a flinch rule (dispatcher FUN_140b51b70 case 0x49 -> bit 0x400 at (ChrIns+0x50)+0x50
  +0x8c, read only by FUN_14061fad0: an AI handling the sender's flag-63 message 7 raises INTERUPT_ParryTiming (24) when
  the sender is within its +0x154 range and has this bit or vfunc +0x1a8). Our flinch gate keeps it as a proxy (gap). Posture break: hit breaks
  play TrunkCollapseFront/Back (HKS ExecDamageBreak), deflect/guard breaks keep AttackBoundEmptyStamina / GuardBreak; the
  deathblow window = until the break anim opens AI flag 86 (frame 85-87 of 90, ~2.9 s) instead of config 4 s.
  Clip aliases: 57 enemy / 10 player anims ship TAE-only (mirrors, chain steps); data::alias_missing_clips borrows the
  nearest sibling's clip + root motion (3101 -> 3100, 8551 -> 8550, 203007 -> 203006, neutral step -> backstep 213302).
- 2026-10-06: knockback from data: AtkParam knockbackDist_DirectHit / _Guard / _JustGuard [m] (e.g. c1020 combo 1.08 /
  1.80 / 2.16 m guarded, 0.5 m deflected; player slashes 1.0-1.4 m guarded, 0.4 m deflected), cut by NpcParam
  knockbackRate_vsPlayer_* [%] (0 for c1020), timed by KnockBackParam row 0 (guard_S/L/LL, damage_S/M/L: hold
  ContTime at v, then linear to 0 over DecTime; v = dist / (cont + dec/2)). Gap: the player's KnockBackParam row.
- 2026-10-06: vitality vs posture regen (player) FOUND in data: Wolf's default body armour EquipParamProtector 102000
  carries residentSpEffectId 5221/5222/5223 ("PC HP 75%/50%/25% core performance": staminaRecoverSpeedRate 0.75 /
  0.667 / 0.5 at conditionHp 75/50/25) -> products x0.75 / x0.5 / x0.25 regen; same mechanism as the enemy's
  300600-2. The player's StaminaControlParam row is 0 (its TAE only uses types 0, 5, 10 = row 0's entries:
  attack/step 0 %, guard 200 %); row 0 was not exported before, so those ratios now actually apply. Test added.
- 2026-10-06: vitality emptied by hits opens the deathblow like a posture break (was: enemy left un-hittable at 0 HP);
  after an unused window the enemy is back at 1 HP so the next hit reopens it. Dead enemies are not hittable.
  2026-10-13: only for enemies with deathblows left (NpcParam ninsatuNum > 0) or a stateInfo-143 SpEffect (exe
  IsNoDead); ninsatuNum-0 rows die at 0 HP through ExecDeath (HANDOFF 4.42 "Rule change").
- 2026-10-06: deathblow window corrected: every break anim applies SpEffect 220420 (stateInfo 352 "trunk display:
  collapsed") on frames 0-75 = the deathblow indicator -> window 2.5 s (was flag 86 at 85). Exe FUN_140a02a10 derives
  the posture display state (0 ok / 1 danger / 2 empty / 3 collapsed while 352) from current stamina; the post-window
  posture reset itself is not there (gap stays at 50 %).
  HUD posture bar turns red at >= 70 % full (exe danger threshold 30 % left).
- 2026-10-06: with the AI on, the enemy's defence is the script's own Goal.Parry: the player's flag-63 notify is fed
  to the brain as INTERUPT_ParryTiming (Goal.Interrupt -> Goal.Parry). Adds what the Rust port missed: backstep 5211
  vs 109980 attacks, sprint-attack (109990) answers 3102 / 3088, and the 67 % 3007 counter when the player attacks
  from just outside parry range. Get_ConsecutiveGuardCount reads the engine-side count; unset goal params read as 0
  (engine behaviour; EndureAttack relies on it). The Rust port remains for the passive practice partner.
- 2026-10-07 guard arc from the exe: FUN_140b6ab40 blocks only when facing . attack direction < cos((angle + 180) deg),
  angle = the ShieldBlock flag's ArgB via a vfunc (0 -> same as 90). ArgB 90 everywhere in Wolf's TAE -> the front half.
  combat.rs guard_half_angle reads it per anim; config combat.guard_angle removed (gap closed; old value matched).
- 2026-10-07 FIX: deflects had no facing check (a perfect press deflected attacks from behind). The exe's guard-arc
  test (FUN_140b6ab40) runs before the guard outcome, so the same arc now gates deflect and block on both sides
  (backstabs on a deflecting enemy land too). Test a_deflect_facing_away_is_still_hit. 40 pass.
- 2026-10-07 NEXT 12 done: no posture restore after a missed deathblow. Data-module layout confirmed (HP +0x130/34/38,
  graph-101 stat +0x13c/40/44, posture +0x148 cur /+0x14c max /+0x150 base; FUN_140bd6410/20/30 set the bases). The
  only ratio restore (FUN_140bd62d0) is TAE event 961, absent from all 364 c1020 TAE anims; the collapse SpEffects
  keep staminaRecoverSpeedRate 1.0; regen (FUN_140a04850) runs every frame. So the 50 % reset is gone: the General
  keeps what regenerated during the collapse (staminaRecoverBaseVel 60/s). Also NpcParam maxDebtStamina -30: posture
  may overshoot max by 30 (stamina debt) -> Actor.posture_debt; bars clamp at full.
- 2026-10-07 WOLF: posture breaks in the air (HKS BEH_R_AIR_BREAK_DAMAGE): guard emptied -> AirDeflectGuardBreak
  (a050_190150), hit -> AirDamageBreak (a000_190050; FLING -> AirDamageBreakLargePound chain; the blow / upper break
  clips do not exist), held until landing, then LandAirDamageBreak / LandAirDeflectGuardBreak (a000_190055 /
  a050_190155), which count as broken for the follow-up knock-down (HKS 1158). 16 states exported.
- 2026-10-07 deflect/guard reaction direction per HKS _setDeflectDir: attack deflectAction / justDeflectAction 1 -> L,
  2 -> R, else (0) -> F (front); was L/R only (0 -> R). Easy Middle now also picks V1/V2 like Easy Small (HKS
  Selector_DeflectVariation, random); Hard has no variations. 14 more deflect states exported (all _F, Middle V1/V2).
- 2026-10-07 enemy guard reactions per c9997.lua ExecGuardBlock: GUARD_DIR_RIGHT 1 / LEFT 2 is the mirror of the player's
  DEFLECT_DIR_L 1 / R 2, so value 1 -> *_RighttoLeft / GuardBreakRight and 2 -> *_LefttoRight / GuardBreakLeft (ours
  were swapped: enemy block / deflect / guard-break anims played mirrored). Block size from guard_damage_table
  [dmg level][NpcParam guardLevel 4] (PUSH now Large, MINIMUM = additive only). Enemy guard arc = NpcParam guardAngle
  60 deg (Actor.guard_angle) instead of the TAE's 90: attacks from wider than 60 deg land on a guarding General.
- 2026-10-07 enemy TAE: 703 FixedRotationDirection (IsEnable, on each swing's active frames, e.g. Attack3000 f22-36,
  3002 f28-60) now stops the General's tracking -> sidesteps work as in Sekiro; 225 SetSPRegenRatePercent multiplies
  posture regen (guard / deflect anims 3100-3102: 33 %, fire reactions 0 %). Audit of remaining unhandled enemy events:
  FFX / decals / foot SFX / look-at / draw masks (visual), 66 AddSpEffect_Multiplayer (AI-state / landing behaviour
  SpEffects only).
- 2026-10-07 c1010 defence: its script's Goal.Interrupt calls the shared Common_Parry(ai, goal, 50, 25, 0, 3102)
  (common_common_func_NTC.lua), the General has a custom Goal.Parry. Passive-mode port now has both: Foe.common_parry
  = Some((guardMult 50, stepProb 25, stepType 0, rush 3102)) -> rush attack (SpEffect 109990) EndureAttack 3102,
  thrust by parry rank (c1010 resident 221002 = rank 2: no thrust deflect), 109980 + rank 0 -> back-step 5211, guard
  count * 50 % deflect else guard, and a 25 % back-step when the player is just outside reach. 40 s AI trace: approach
  (MoveToSomewhere), ComboAttackTunableSpin 3000 > ComboFinal 3001, AttackTunableSpin 3008 / 3012. 46 pass.
- 2026-10-07 buffer reset = HKS FireEventNoReset list (player.rs keeps_buffer): the attack Release follow-ups,
  EasyDeflected L/R, DeflectGuardToStand*, StandDeflect{Easy,Hard}Minimum, AirDeflectGuardEnd, sprint quick turns keep
  pending presses (were cleared).
- 2026-10-07 guard release while moving: BEH_A_DEFLECT_GUARD_END plays DeflectGuardToStandMove (hkx
  DeflectGuardToStandMove_Selector on MoveSpeedIndex: Walk a050_203011 / Run a050_203012; ShieldBlock frames 0-6, all
  action cancels from 0) instead of the standing end. Ref 220 (*Variation ends) is raised by no Wolf anim.
- 2026-10-08 DeflectGuardToStandMove is HKS STATE_TYPE_UPPER_ACTION: c0000.hkx StandMoveOverwrite LayerGenerator =
  StandMoveLower_SM (walk/run loop, useMotion) + StandMoveUpper_SM (the release clip) with boneWeights 0 for bones
  0-38 (Master, foot targets, RootPos, Pelvis, legs). Wolf keeps walking/running (lock-on strafes too); at the end
  FireStateEndEvent -> W_StandMoveLoop. Was a full-body clip with no root motion: Wolf stood still 0.67 s.
  The standing DeflectGuardToStand (STATE_TYPE_ACTION) has no TAE flag 11 / IsMoveCancelPossible: no move until it ends.
- 2026-10-07 additive guard flinch (HKS BEH_ADD_R_GUARD_DAMAGE): a MINIMUM-level (8) guard/deflect, or Small/Middle/
  Large/Push while ref 202 GUARD_LEVEL_EXCHANGE_MINIMUM is up (only SprintToDeflectGuard frames 0-12), keeps Wolf's state
  (the game layers StandDeflect{Easy,Hard}Minimum / AirDeflect*MinimumAdd with FireEventNoReset). combat.rs additive_guard.
- 2026-10-07 helper reports integrated (lead review). From S1 (HKS audit): (a) BEH_A_DEFLECT_GUARD_START ref 228
  TAE_ENABLE_ADD_JUST_DEFLECT (StandDeflectEasy* frames 0-9, not with ref 412) -> W_AddHardDeflectGuard: an ADDITIVE
  deflect window (a000_299060, ref 203 frames 0-9) over the block recoil (Actor.add_anim/add_t; combat.rs reads the
  JUST_GUARD window from the base anim or the layer). (b) ref 503 TAE_ENABLE_HIT_DEFLECT_CANCEL (StandDamageMiddle/
  Large from frame 12) -> StandDeflectGuardFromDamage_{F,B,L,R} (a050_2062xx; hkx selector on DamageDirection; deflect
  window frames 15-21). (c) Easy-deflect V1/V2 now a coin flip (HKS math.random() > 0.5). (d) Break damage at
  MINIMUM level plays nothing (ground too). Not done: env(1121) < 0 flips the hit direction (engine value unknown),
  repartition guard reactions (env 334 SP_GUARD_REACTION_1-4), Blinding damage, GUARD_END_VARIATION (no Wolf anim
  raises ref 220), ref 92 MOVING_SPRINT guard-damage skip (unreachable: SprintLoop has no block flag).
  Tests: guard_during_a_block_recoil_adds_a_deflect_window, guard_cancels_a_middle_hit_reaction.
- 2026-10-07 deathblow from behind: ThrowParam start rows pick the side - "崩し始動" 11020000 DiffAngMin..Max 90-180,
  "崩し背後始動" 11020110 0-90 (both diffAngMyToDef 35). Behind (enemy forward within 90 deg of Wolf->enemy) ->
  "崩し背後本体" 11020111: a201_511200 (clip from a201_510200), enemy ThrowDefDeath13201, enemy keeps facing away.
  c1010 rows 11010110/111 exported too. Test deathblow_from_behind_uses_the_back_throw.
  Plunge ("崩し落下" 11020150/151: Dist 20/10, Y range +3/-15, normalFallOrbitCheck range 1.5 m / height 3 /
  2000 ms, atk a201_511400 = 510300's clip with TAE 920 row 8000 homingId 200000300 -> CharaPhysicsHoming param,
  not exported) is not wired: see HANDOFF_PLAN S5.
- 2026-10-07 S5 plunge deathblow done by lead. Start (ThrowParam 崩し落下0, suffix 150): falling, enemy broken, within
  Dist 20, defender within +3 / -15 m, normalFallOrbitCheck = the unsteered fall passes within range 1.5 m and
  heightLimit 3 m above the defender within timeLimit 2000 ms (player.rs start_plunge). Wolf a201_511400 (510300's
  clip, "PlungeDeathblow"), enemy ThrowDef13400. Steering: ChrPhysicsHomingParam 200000300 (via TAE 920 row 8000):
  fallCorrectionGuaranteeArrival 1 -> horizontal velocity re-aimed each frame to land on the target,
  fallCorrectionTurn 1 -> face it; targetBaseDmyPolyId 233 + offset (0, 0, -0.3) approximated by the enemy position
  (gap). Landing -> 崩し落下1 (151): Wolf a201_511410 (510310's clip), enemy ThrowDefDeath13411.
  Test plunging_onto_a_broken_enemy_is_a_deathblow.
- 2026-10-07 c1010 (Ochimusha) gets the same behind / plunge deathblows: rows 11010110/111/150/151, Wolf's a200 group
  (a200_511200 95 f, a200_511400 80 f, a200_511410 105 f via ImportOtherAnim clips). Test
  ochimusha_plunge_deathblow_uses_its_a200_pair. Twist chains ease out by their own offGain now.
- 2026-10-08 LIVE deflect check (29 enemy hits on Wolf while guarding, posture drop without HP loss): deflects at
  press->hit 0-6 TAE frames (= our StandToDeflectGuard window, SpEffect 105010 f0-6); blocks inside 1.5-4.5 frames
  fit the spam chain's shorter windows (3/2/0); deflects at 9.6-25 frames come during recoils (additive deflect
  ref 228 / chain windows). Deflect posture cost on Wolf (this save, max 420): easy 26-100, hard 26-83; blocks
  26-112. Not yet matched per enemy attack (need the attacker's AtkParam).
- 2026-10-08 LIVE #2 (rec_20261008_044802; Kokageshu c1470, bandits): guard taps inside StandDeflectHardSmall
  (ref 228 f0-9) -> StandToDeflectGuard at f9.4-9.5 in 4 samples even when let go: HKS 6161 starts the guard on
  ref 411 (ADD_ACTION_INPUT_GUARD, raised by the additive deflect a000_299060 f0-9) + 412 (base f9-10).
  Implemented (player.rs add_guard); test guard_tap_in_a_hard_deflect_recoil_becomes_a_guard_at_ref_412.
- 2026-10-08 LIVE #3 (rec_20261008_051450, Wolf-only: the enemy array at WorldChrMan+0x998 was empty in that area -
  gap: enemy lookup is map-dependent): 59 guarded hits by guard press -> hit time: deflect at f0-6 (41 of 42
  deflects), block from f6-7 on (f6: 3 deflects / 1 block). = our 6-frame window (SpEffect 105010). Recoil guard
  cancels at f9.1-9.6 again (411 + 412, implemented). Land-air-combo inputs only look early because the Land anim
  continues the air slash's time (our continue_state) - not a gap.

## Hit stop (redone 2026-10-11, combat.rs start_hit_stop, actor.rs HitStop)

The old rule (both sides frozen for the raw AtkParam time on direct hits) was wrong. Exe
(FUN_140b9cd90 attacker, FUN_140b9cfb0 defender, started on every contact, hit / block / deflect;
rules first written up by AKJama/sekiro-rs docs/COMBAT-RULES.md "Hit stop"): attacker =
hitStopTime x the defender's NpcParam hitStopType scale (0 none, 1 0.5, 2 full; Wolf as defender
= full); defender = hitStopTime_Defencer only when its hitStopType_Defencer is 1 (Wolf always).
ChrActionFlag 41 on the defender forces both. The clock holds 0.06 s, then eases back as
(elapsed / length)^2 (0.1 s costs ~0.127 s). Most NpcParam rows are 0 / 0, so Wolf's sword on
ordinary enemies and bosses stops nobody; type 1: Guardian Ape, Okiryu; type 2: the Great Serpent.
Test hit_stop_follows_the_defenders_type.

## The exe's resolution, run on the snapshot (2026-11-03, combat.rs guard_check)

FUN_140b6c880 (defender), FUN_140b6e6a0 (apply) and FUN_140b698d0 (attacker) were run in
tools/emu-cases (`guard`, 90 cases, extracted/emu/guard_resolution.json): Wolf against the
General's attacks and the General against Wolf's, on the memory snapshot.

- Guarded = the ShieldBlock flag (TAE ChrActionFlag 3: module0 +0x70 bit 2, its ArgB the
  behavior judge at +0xb08: Wolf 90, the umbrella 100, NPCs 900, c1180 armour 800), the attack
  inside the arc (or AtkParam isAllDirGuard), not a ghost attack (isGhostAtk without stateInfo
  157 on the defender). The judge picks the guard row (vfunc 0x1c0: BehaviorParam_PC 105000090
  -> AtkParam_Pc 5000090, 107600100 -> 7600100; BehaviorParam 210200900 -> 10200900).
- Arc: facing . attack dir < cos(angle + 180), the angle NpcParam guardAngle for NPCs and the
  weapon's guardAngle for Wolf; 0 = the front half. Run: Wolf guards at 89 deg, not at 91; the
  General (60) at 59, not at 61. ArgB is not an angle (RSekiro read it as one: the umbrella got
  100 deg instead of the weapon's 180, an NPC with guardAngle 0 got 900 = all round).
- Then the flick contest: flick defence (FUN_140842fb0) >= attack flick (hit +0x40) deflects
  unless disableJustGuard is set (then it lands), else it blocks unless disableGuard is set.
  The pair read is the guard row's guardAttribute's (0 or 1; 2+ none): the umbrella's 7600100
  and the armour / shield guard rows are 1, where thrusts and 60 sweeps are 0 / 0 (run: the
  umbrella blocks the General's thrust 10200200 and the Shura stomp 10700370, the sword does not).
- Attack flick: NPC guardAtkRate (30 on most rows, 100 on 213: the Demon's blasts 70200330 are
  blocked inside the deflect window), Wolf (int)(guardAtkRateCorrection / 100 x attackBaseRepel
  10) = 30, deathblow starts 90 (blocked by the General's deflect). Flick defence: Wolf 100 / 100
  x guardBaseRepel 20 = 20, the General's guardBreakRate 20; raised to the largest
  defFlickPower worn, any stateInfo (105010 / 200220 / 900001 40, 1130 65). Template 17 (flick
  20) is deflected by a bare guard.
- So the deflect window is no stateInfo 158 test. RSekiro deflected under any stateInfo 158
  SpEffect: the umbrella hold's 107650 (21 a076 anims, flick 0) deflected for its whole hold,
  now only its opening 105010 (0.4 s) does; NPC 1040 / 3372 the same.
- A block that empties posture sets +0x1c4 (guard break) and hands the attacker reaction 99999
  instead of 1017: no bound for the attacker (fixed in combat.rs). A deflect never sets it, even
  when it empties posture (posture 1 -> 0; RSekiro too since 2026-11-03, below).
- Spam chain 105020-23 on a deflect: the attacker's 30 posture x 1 / 0.5 / 0.25 / 0.125 lands as
  30 / 15 / 7 / 3 (cut to an int; RSekiro now floors it too, 2026-11-03).
- Fixed 2026-11-03 (topic throw): a deflect may empty the defender's posture without a break
  (Actor::add_deflect_posture / posture_held; was kept 1 short). The flags are the hit record's,
  not the chr data's: FUN_140b6e6a0 sets +0x1c6 on an unguarded hit and +0x1c4 on a block when
  the posture setter FUN_140bd4e20 leaves posture < 1 (or the HP / lives test) and the posture
  damage is > 0; a deflect sets neither. So the next posture damage breaks.
- Test guard_resolution_matches_the_exe (sim_tests/emu_guard.rs): flick powers and verdicts
  equal on all 90 cases, the guard break and attacker reactions as above.
- gap: the parts guard (a hit on a guarding body part, FUN_1410c66a0, +0x1ce) is not ported; the
  guardDefFlickPowerRate product and FUN_140bfccc0's NPC rates are taken as 1 (1 on every row
  that counts); the umbrella's 180 deg arc is read from its weapon row, not run (no umbrella
  equipped in the snapshot); strength is the snapshot's 10 (RSekiro has no stats).

## Throw and deathblow checks, run on the snapshot (2026-11-03, player.rs deathblow_check)

tools/emu-cases `throw` (throw.rs): Wolf as thrower, the General (c1020) as defender, on the
Naomori-arena snapshot. extracted/emu/throw_kinds.json, throw_ranges.json.

- FUN_140be1b50 is the throw module's per-frame tick (chr +0x1ff8 bag +0x88, CSChrThrowModule
  vtable 0x142a8e8b0): it clears a pending request (+0x70 -> +0x68 / +0x6c), ticks its timer,
  sets +0x7c and empties the candidate list at +0xb0 (count +0xd8). It decides nothing. The
  search is FUN_140be2a30 (per ThrowKindParam kind: FUN_140be3810, then each ThrowParam row of
  the pair: AtkChrId / DefChrId = chr +0x6c, then FUN_140be3a90).
- FUN_140be3a90 = the additionalConditionSpecialEffect pair (+0x6c / +0x70), FUN_140be0270
  range, FUN_140be3ba0 fall orbit (only kinds with doesCheckNormalFallOrbit), FUN_140be3da0 a
  sight cast between the two. Row and kind refs are looked up by id with FUN_14095e0b0 /
  FUN_1410d0200 (FUN_14095e0e0 / FUN_1410d0370 take a row index).
- Range FUN_140be0270, 3,000 placements (5 rows x 600: random bearing, distance, height, both
  yaws): the defender's chr flag +0x1a15 bit 0, then dy < upperYRange and dy > -lowerYRange,
  then |d|^2 < Dist^2 (3D), then the angle between Wolf's forward and the 3D direction <=
  diffAngMyToDef, then |wrap(defender yaw - thrower yaw)| in [DiffAngMin, DiffAngMax]. Yaws from
  the throw module's +0x94 (= physics +0x74), positions FUN_140bbfc00 (physics +0x80, or the
  rows' judgeRangeBasePosDmyId1 / 2, -1 on Wolf's rows). The forward for yaw y is
  (-sin y, 0, -cos y), Actor::forward's. Sv1's facings matched as they were; fixed: Dist and the
  height window are strict (were inclusive) and a row with neither height set takes no one (was
  "no window"; only the empty row 0). New player.rs throw_range_ok; the plunge, wire and
  combat.rs throw_in_reach starts made strict too. Strictness is from the code; no case sits
  exactly on an edge. Test throw_range_matches_the_exe: all 3,000 equal.
- The whole row: equal to the range but for the plunge row (no fall: the fall orbit is the
  plunge's fall_reaches) and three placements with the General 0.7-0.8 m below Wolf (sunk in the
  floor): the sight cast. gap: the cast and the SpEffect pair are not in throw_range_ok.
- Throwable states FUN_140be0a40 (states 0-127 x categories 0-21 / 100 / 101: 52 pairs pass) and
  the kind test FUN_140be3810 on the General (112 cases). The defender's state is its throw
  module +0x7a, written each frame by TAE 932 (unk0; 931 writes the thrower's +0x78), else +0x7b
  (101 on all 89 enemies, Wolf 1). 101 passes behind 0, crouch 1, play 2 / 3, falls 6 / 7, wall
  and hang 9-11, 12, aging-behind 17; the break anims' 103 passes break 4, 7, 12, plunge 13,
  kick 14, wire 15, aging break 18 / 19, push-down 20 (108, TrunkCollapseLarge: the same
  without 20); 104 passes "enemy special state A" 5 (the finisher rows 250000) with 2 / 3 / 7 /
  12; 105 the Todome 16 with 2 / 3 / 7 / 12. HP 0 changes nothing (defAliveCheck 0 on these
  kinds); the main-throw kinds (throwKindCheckMethod 1) pass only out of their start throw.
  Ported as throwable_state_allows and def_throw_state (finisher window, else 103 while
  Mode::Broken, else 101). Fixed by it: a broken enemy that never noticed Wolf took the stealth
  rows 0020 / 0021 (aged: 0270 / 0271) - the break rows now (103 does not pass 0 / 17); the
  finisher row was taken in a 105 window without a Todome row - only in 104 now; a broken enemy
  in a 104 / 105 window gets no break deathblow. Test throwable_states_match_the_exe.
- TAE 932's 103 span equals Sv1's break window (collapse SpEffect 220420's end) on 868 of the
  1,238 enemy anims with a 103 / 108. gap: the others are not opened as breaks in Sv1:
  DeathDefault0 / 1 (0-45 f on most enemies), ThrowDef12120 / 13120 / 13900 / 14300, LandTrunk,
  TrunkCollapseAerial (3-60 f vs the SpEffect's 0-60); whether a dying chr passes the kind
  test's other gates (vfunc 0x550, the throw node's bits at +0x70) was not run.
- gap: the pick among several passing rows (ThrowKindParam checkPriority: behind 10, break 2,
  break-behind 0) is not traced; Sv1's deathblow_pick takes the lock target, else the nearest.

## Throw checks, second pass (2026-11-03, topic throw2: sight, SpEffect pair, pick, kind gates, fall orbit)

tools/emu-cases `throw2` (throw2.rs) on the Naomori-arena snapshot; extracted/emu/throw2_sight.json,
throw2_speffect.json, throw2_pick.json, throw2_fall.json; tests sim_tests/emu_throw2.rs. The disable-AI
flag (on in the snapshot) does not touch any of these: they are pure checks.

- Sight cast FUN_140be3da0, 1,200 placements of the General around Wolf (near: sunk in / raised over the
  floor; far: across the arena), 559 blocked: skipped when the row sets isSkipSphereCast (+0x3b bit 2);
  else two casts FUN_14094d8d0 (hit world [0x143d6d640]+0x98, filter 0x4c; 0x41 only between the local
  player and another player), from the thrower's point raised by its physics height (+0xd0) x 0.8, then
  x 0.42 (DAT_143b015a0), to the defender's point raised the same; radius = the LARGER of the two radii
  (FUN_140bbf740; Wolf 0.4, the General 0.5) x sphereCastRadiusRateLow / Top (+0x30 / +0x32) / 100, no
  cast when 0. A cast passes with no hit or a hit (the sphere's centre at contact: 0.2 m over the floor)
  farther than the defender's point; both must pass; the debug byte [0x143d5be38]+0x101 forces a pass.
  Recasting here with that rule gives the exe's answer on all 1,200. Ported as player.rs throw_sight_ok
  (test: the same casts and answers), in deathblow_check's reach from the feet with
  map_sphere_cast. gap: Sv1 casts the centre ray (map::raycast), not a swept sphere, on the hit
  collision only.
- SpEffect pair (first test of FUN_140be3a90): _forAtk (+0x6c) must be on the thrower, _forDef (+0x70) on
  the defender (FUN_1409f1c80, chr +0x11d0 list), -1 = none. 16 cases on 11020190 with ids poked (the
  General wears 200030, his a000 set). Ported: throw_speffects_ok / speffect_pair_ok, combat.rs wears_sp
  (200030-200034 by anim set as throw_row_for); deathblow_check takes the defender's side. gap: Wolf's
  side is taken as met there (no loaded row sets _forAtk).
- The pick. The input throw FUN_140be0e50 (called from FUN_1407d8430 with buttons 0 / 4 / 5) keeps the
  kinds Wolf's own state passes (FUN_140be3520, thrower table FUN_140be0950: his state 1 -> categories
  0, 4, 5), then an IChrFinder (vtable 0x142a8ebc0) over WorldChrMan (FUN_140a4c260: chr sets in
  order, each set's chrs by index) whose slot FUN_140bdf930 takes the first chr that is hostile, not
  +0x1ff4 1000, updated every frame (FUN_1407dce60 == 0: chr +0x110, the update omission, 30 / 5 on the
  far chrs of the snapshot, poked 0 here) and has any passing row (FUN_140bde5c0 -> FUN_140be2a30 ->
  FUN_140be2d60, which starts the first triggered one). The lock target and distance play no part. The
  order runs by chr number (c1000 ... c7600, the player set last). The kinds per situation
  (DAT_143d78488 +0x270 + situation x 0x20) are sorted by checkPriority, lowest first (41000 break-behind
  0, 270000 near 1, 40000 break 2, 0 stealth 10, 120000 100). 120 placements of two c1010 and the
  General, broken, in reach: the pick was always the first in chr order with a passing row, and not the
  nearest on 48 of 79 with several. Ported: deathblow_pick takes the lowest chr number, then the earliest
  spawned. Sv1's own row choice (behind 0110, near 0005, else 0000) agrees with the priority order (the
  DiffAng ranges split at 90). gap: within a chr number the exe's set index is taken as spawn order;
  the HUD mark's fallback (broken, out of reach) stays locked / nearest.
- Kind-test gates and the 103 anims: FUN_140be3810 also fails on action flags (chr +0x1ff8 bag[0]
  +0x70) bit 0x20 (ChrActionFlag 67 ThrowStart2, TAE dispatcher FUN_140b51b70 case 0x43: the ThrowDef
  anims' first frames; every kind), bit 0x10 (flag 68, in no enemy anim; start kinds) and vfunc 0x550
  (FUN_1409f3250: invulnerable - +0x70 bits 0 / 1 = TAE 951 / 950, or chr +0x1a11 & 0x20 / +0x1a13 &
  2). A scan of all enemy anims: the 1,267 TAE 932 spans of 103 / 107 / 108 / 111 all have frames free of
  950 / 951 / 67, death anims included (DeathDefault 010000 / 010010 0-45 f; defAliveCheck 0 on the
  break kinds, HP 0 changes nothing): a dying chr passes the kind test. Ported: def_throw_state reads
  TAE 932 in every anim (data.rs throw_state) and throw_kind_closed closes 67 / 950 / 951 (ThrowDef12100
  holds 67 for 0-1.5 s: no deathblow starts then; the check stays, out of reach, so the HUD mark still
  shows on the break pose - gap: the exe's mark display is not traced against the gates). gap: Sv1
  still refuses a Mode::Dead enemy (but in the finisher windows): its dead chr has no way back into a
  ThrowDef; the chr-flag half of vfunc 0x550 is not tracked.
- States 102, 106, 109, 110: no TAE 932 of any enemy anim sets them and no snapshot chr holds one as its
  own +0x7b (101 on every enemy, 255 on c7600, Wolf 1); they exist only in the table. 107 (cats 8, 12)
  is on c1060 / c1080 / c1240 / c1250 / c1310 a000_0030xx / 008602, 111 (12, 21) on c1340 / c1350
  a100_0083xx, 255 on c1030 / c1100 0200xx: read now through throw_state.
- Fall orbit FUN_140be3ba0 (kinds with doesCheckNormalFallOrbit): the fall top (physics +0x854 = Sv1's
  air_top) above the defender must exceed normalFallOrbitCheck_heightLimit; FUN_140baedb0 ->
  FUN_140bb2850 solves the orbit (velocity FUN_140bbf7d0, the proxy's; accel -0.1 along the heading,
  -13.72 + physics +0x1c4 down, DAT_143b09ce8) in the heading / up plane with the target at its full
  horizontal distance: one real root of the cubic -> that time (>= 0); three -> the closest point on 16
  samples 1/6 s apart and the later time its height is reached; no horizontal speed -> the target's
  height on the plumb line. Pass: that point within _range of the defender (3D) and its time within
  (int)(_timeLimit / 1000) s. 400 solver runs (within 1 cm / 0.01 s) and 450 placements over the plunge,
  kick and stealth-plunge rows: all equal. Fixed: Sv1's plunge used its own 1/60 s fall with heightLimit
  as a ceiling over the target; now throw_fall_orbit_ok (start_plunge, and the wire throw, whose kind
  170000 checks no orbit). A flat-ground jump tops out about 2 m over the enemy, under heightLimit 3 m:
  no plunge from it (it used to pass); the plunge tests and the debug menu's plunge now fall from 4 m
  (3 m exactly fails the strict >). gap: Wolf's velocity is his air_base / vel_y, not the proxy's.
