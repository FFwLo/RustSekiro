# enemy: Enemies: real Lua AI, NpcParam, c1020/c1010, enemy reactions

Dated findings, oldest first. Append new entries at the end.

- 2026-10-06: game runs; 4 data-rule tests pass (cargo test). Enemy starts passive (T toggles). Auto-resume schedule was blocked by permissions; user must allow it.
- 2026-10-06 GHIDRA (Steamless exe, project extracted/ghidra, runner tools/ghidra.ps1): posture regen CONFIRMED.
  ChrIns::Update = FUN_140a04850 (vtable NS_SPRJ::ChrIns 0x142a22948, slot 0xb0):
    pts/s = SpEffectRecoverRate * (base + SpEffectChangeSpeed) * byte(+0x14)*0.01 * chr.f[0x10d0] * (ratio(type)*0.01 if type != -1)
    whole points added per frame, fraction carried (ChrDataModule +0x15c); stamina cur/max at ChrDataModule +0x148/+0x14c.
  base: enemy = NpcParam.staminaRecoverBaseVel (EnemyIns slot 0x218), player = CalcCorrectGraph 504 (FUN_140850b10).
  FUN_140844cb0(cur,max,x) returns x: no posture-ratio curve and no direct HP curve -> hp_scales_regen now off.
  chr.f[0x10d0] = 1.0, or 0.8 / 0.7 from a once-per-second level system in PlayerIns update (thresholds 0.7/0.3); input unidentified.
  CalcCorrectGraph evaluator implemented exactly (adjPt power curves).
- 2026-10-06: NEXT 7 done: the enemy runs its REAL AI. src/ai.rs embeds Lua (mlua 5.4) and loads extracted/ai_src
  (common_* + 102000_battle.lua, the decompiled game scripts) on our runtime src/ai/runtime.lua:
  goal tree (Activate/Update/Terminate/Interrupt, life, subgoal queue, TimingSet*), table goals via the game's own
  common_table_ai_common.lua (REGISTER_GOAL hooks), function goals (X_Activate...), and native goals:
  CommonAttack(ez, target, successDist, angle, turnTime, frontAngle, ...), SpinStep, MoveToSomewhere, LeaveTarget,
  SidewayMove, KeepDist, Wait, Guard. ~60 ai: queries (dist, angles, SpEffects, timers, numbers, attack-passed timers).
  Engine rule from the NPC TAEs: a request starts when the anim is free or its AI cancel flag is on:
  23 combo attack (goals with ENABLE_COMBO_ATK_CANCEL: ComboRepeat/ComboFinal), 86 attack, 79 step, 78 move.
  Reactions (react/Goal.Parry) interrupt -> replan -> Kengeki_Activate sees the 0.1 s clash SpEffect and queues the
  counter (e.g. JustGuardDamage opens flag 23 at frame 4 -> 3060/3088). Decompiler fix: `break` came out as
  `loopVar = bound` in Common_Battle/Kengeki_Activate (patched at load, ai::fix_decompiled).
  Observed (headless trace): SidewayMove, backstep 5211, 3014 charge, step-in 5200 -> 3008>3009, 3007, 3006, 3008>3067.
  Character push-out added (NpcParam hitRadius 0.5 + player 0.4 gap). T toggles real AI; passive = practice partner.
  Goal.Parry stays in Rust (tested); Goal.Interrupt runs for observed SpEffects (90, 109220/1, 200210/1/50).
  Gaps: GetNinsatsuNum (1), observe areas, team/platoon, navmesh queries (open floor assumed).
- 2026-10-06: Healing Gourd (E): goods 3000 -> SpEffect 3000 changeHpEstusFlaskRate -40 = 40 % max HP, applied at the
  drink's TAE 65 ConsumeCurrentGoods (ItemGourdDrink frame 21, Repeat frame 9); walk while drinking (flag 90),
  item cancel flag 31, empty gourd -> ItemGourdDrinkFailed; config player.gourd_charges (3). INTERUPT_UseItem is fed
  to the AI (c1020 10219000 lacks the 3102050 drink-punish SpEffect, so it does not specially punish).
- 2026-10-07 shuriken aim: lock target, else LockCamParam bullet auto-capture (bulletMaxRadius 30 m, bulletAngRange 15
  deg of the facing - the CSChrAutoHomingModule bullet limits), then bent toward it within Bullet lockShootLimitAng.
  Was: nearest enemy within lockShootLimitAng. Stopping here: 5h usage near the user's 50 % cap.
- 2026-10-07 WOLF: hits in the air use HKS air damage (c0000_transition.lua ~1106): SMALL/MIDDLE AirDamageSmall,
  LARGE/PUSH AirDamageLargeStart -> FallLoop -> Land, FLING AirDamageLargePound*, EXLARGE/SMALL_BLOW/EX_BLAST/UPPER/BREATH
  -> their air blow states (no clips in c0000_a0xx -> the AirDamageLarge chain); landings -> StandDamageLargeDown face
  down (graph transitions). 27 air states exported. Test hit_in_the_air_uses_the_air_reactions. 41 pass.
- 2026-10-07 NEXT 13: second enemy c1010 Ochimusha (one-handed sword), selected with config enemy.chr ("c1020" /
  "c1010", restart). Unpacked chr/c1010.* + sound/c1010.* (bash: avoid a leading "^/" in the regex, MSYS mangles it);
  decompiled 101000_battle.lua / 101000_logic.lua (m11_00_00_00.luabnd) into ai_src; exporter: Character OCHIMUSHA
  (behavior base 200,000,000 + 10100 * 1000, NpcParam 10100000) -> combat_data "enemies" map (280 states, 499 anims,
  28 attacks), NpcParam / StaminaControlParam 1000100 / ThrowParam 1101xxxx rows; model_c1010.bin; 297 sounds.
  Game: data::Foe {chr, npc_row, think_id 10100000, battle_goal 101000, throw_n} replaces every c1020 constant
  (NpcParam rows, AI brain, ThrowParam 11000000 + n * 1000 + suffix, anim/model bins). NPC display masks: meshes whose
  material is "#NN#..." draw only when NpcParam modelDispMaskNN = 1 (c1010 39 of 81 meshes; applies to c1020 too).
  c1010's deathblows use player anims a200_* (atkAnimOffset 200) - exported. Sim tests pin c1020 (app_with); new
  tests ochimusha_runs_its_own_ai_and_attacks / _deathblow_uses_its_own_throw_rows / _attack_can_be_deflected. 45 pass.
- 2026-10-07 PRIORITY (user): focus on Wolf. No further enemies unless asked; c1010 stays available via config
  enemy.chr but the default opponent is the Samurai General (c1020).
- 2026-10-07 enemy additive recoils (c9997): deflected attack with bound type BOUND_ADD01-04 (AtkParam
  just-deflected action 11-14) -> AttackJustGuardBound_Add0N (anim 9600 + N - 1, a000_009600 15 f) while the combo
  continues (ExecAttackAddJustBound); blocked by Wolf with guard bound 11-14 -> AttackGuardBound_Add0N (9700+);
  the enemy's own GUARD_LEVEL_ADD block -> SABlend_Add_{Front,Back,Left,Right} (a000_00950x) by hit direction.
  Shared additive clock now in actor.rs advance (all actors).
- 2026-10-07 enemy hit without a reaction (mid-swing / minimum level): c9997 ExecNoSyncAddDamage -> PartBlend_Add0N
  by env(1120) damage part when the chr has anim 9xxx part clips (c1020/c1010 have none), else SABlend_Add_<dir>
  (combat.rs, additive layer). Wolf's slashes into a swinging General now visibly jolt him.
- 2026-10-09: the AI now runs the game's COMPILED scripts directly (no DSLuaDecompiler step): src/ai/lua50.rs (Lua 5.0
  VM adapted from sekiro-rs, MIT) loads extracted/script/aicommon.luabnd.d (ai_define, goal_list, logic_list, event_list,
  table_ai_common first, then the rest sorted) + <battle>_battle.lua from the first m*.luabnd.d. src/ai/runtime.lua was
  ported to Rust in src/ai.rs (same goal tree, native goals, ai methods; mlua dropped). Parity: 90 s duel_trace gives
  52 vs 53 enemy actions with the same act mix; ai_stubs reports no stubs. ai::fix_decompiled is gone (bytecode has real breaks).
- 2026-10-10 STEALTH (src/stealth.rs + ai.rs logic + enemy.rs): the exe's NPC targeting system
  NS_SPRJ::SprjTargetingSystem (AiThink +0x7b30, update FUN_14061ff40) ported.
  * Target state +0x158 (names at PTR_u_NONE_143afb540: NONE / CAUTION / FIND / BATTLE), prev +0x15c
    (FUN_14062bc00). State from the slots in order (FUN_1406208c0): normal enemy -> its own state;
    sound (4), corpse (7), indication pos (6), memory (5) -> CAUTION; else NONE.
  * Sight cone (FUN_14061bb70 "normal" / "perceive" once flag 0x100000 = FIND/BATTLE is set;
    FUN_14061c2a0 "around"): eye = feet - facing * eye_BackOffsetDist (FUN_14061af30, scale 1.0);
    eye_BeginDist <= d < radius + eye_dist; yaw in [-left, right], elevation in [-bottom, upper];
    far = radius + eye_dist * cut. Getters FUN_1410c6f90 (+0x30 eye_dist_normal) ... mapped by disasm.
    Target point = chr feet (FUN_1409f0870); radius = physics +0xd4 (FUN_140bbf740); height +0xd0.
    Light factors 1 / 0.5 / 0.3 (DAT_143afb490..8, NpcThinkParam disableDark).
  * Wolf's SpEffects (FUN_140bd3e40): dist cut = prod(1 - sightSearchEnemyCut/100) (FUN_140bfe8a0,
    skips stateInfo 7/53 unless the observer flag), angle cuts +0x26c..0x26f, gated by
    sightCutLimitType (+0x107: 1/2 = geometry bits from FUN_140616f50, not traced).
  * Normal cone hit -> normal target in FIND (FUN_14062bdb0); FIND -> BATTLE inside BattleStartDist
    (FUN_14062be40) or platoon battle. Damage (reason 4, FUN_14061d140) -> BATTLE at once.
  * Around cone, no normal target -> meter entry (FUN_140625340): += aroundTargetIncrementPoint (or
    IndicationTargetIncrementPoint) * dt * prod(aroundSightPointAddRate) (FUN_140612400, FUN_140bfc050),
    -20/s out of the cone; >= 100 -> IndicationPos slot (FUN_1406285c0, IndicationTargetForgetTime).
    Search interval rand*0.2+0.2 s (DAT_143afb488/c); meter every frame. HUD: FUN_1408c5fb0(level 0/1/2,
    max meter/100) - drawn as a bar over the enemy (hud.rs, gap: real art).
  * Replan on a state change (FUN_1405ab7d0 -> FUN_14061f500) unless goalAction_ToCaution / ToFind is 0.
  * ConfirmCautionTarget 5100 (CSGoalConfirmCautionTarget, FUN_1405c35a0 / FUN_1405c3770): look anim via
    CommonAttack, Wait(time); terminate in CAUTION without a new sound/indication/corpse -> clear them.
  * AI-state SpEffects 200000/1/2/4 come from the alert anims' TAE (c1020 1000/1010: TAE 66, 1040/401020:
    TAE 401, effectEndurance -1); c9997 HKS UpdateAIState picks Idle/Walk/Turn Default/CautionNoBattle/...
  * The logic script (NpcThinkParam logicId -> 102000_logic.lua / 101000_logic.lua) now runs:
    ExecTableLogic -> Logic.Main -> COMMON_EzSetup picks NonBattleAct / caution search / transitions /
    battle goal. GetEventRequest = -1 (slots filled with -1.0, FUN_1405b2090).
  * Traces (sim_tests stealth_trace, SHINOBI_STEALTH_TRACE=leave|stay|lose): meter full at 4.0 s ->
    700 + 1010 -> walk to the spot -> 600 look + 4 s -> NONE -> 101000 -> walk home -> Stay.
    Found while searching at 15.9 m -> 101040 -> battle. c1010: leashed home at 48 m, forgets after 15 s.
  * Wolf crouch (C): CrouchStart 216000 / CrouchEnd 216100 / crouch locomotion = stand ids + 5000;
    the crouch clips carry 109200 (cut 20). Gaps: crouch attacks/steps/reactions, ACTION_ARM_CROUCH flags.
  * Gaps: LastSightPos forget bookkeeping (forget runs on the normal target), listener ear_dist,
    map raycasts (no walls), patrol routes (MSB), turn anims, the real HUD art, Wolf's physics radius
    (CAPSULE_RADIUS 0.4 stands in).
  * Footsteps: Wolf's walk / run clips carry CreateAISound (TAE 237) for their whole length - walk
    1000 (0.5 m), run 1010 (2 m), crouch walk 1001 (0.25 m), crouch run 1011 (0.5 m); read from the
    clip on screen (Actor::shown_clip), radius x hearingSearchEnemyRate when bSpEffectEnable. 103 tests.
