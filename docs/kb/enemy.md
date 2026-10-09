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
