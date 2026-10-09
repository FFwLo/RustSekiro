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
  never on hit frames -> used as "damage motion allowed" (inferred; test enemy_mid_swing_...). Posture break: hit breaks
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
