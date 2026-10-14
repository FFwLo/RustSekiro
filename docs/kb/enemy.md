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
    the crouch clips carry 109200 (cut 20). Attack from a crouch: CrouchAttackToStand a050_301000
    (+ Release 301050), Wolf stands. Steps / guards / hit reactions from a crouch are the standing
    ones in the HKS (Crouch{Damage,Death}*, CrouchAttackToCrouch only under a forced crouch,
    g_forceCrouch = env 3037 false or SP_EF_REF_FORCE_CROUCH; not modelled: no such area on an
    arena). SP_EF_REF_FORCE_CROUCH 110000 is SpEffect 100210 "in the forced crouching area", set
    only by common_func 20004001(region) (Wolf inside -> set, else clear). The maps start it for
    m10 1002400-02, m11_00 1102102, m11_02 1122400, m15 1502600, m17 1702150-52, m20 2002210/11.
    The nearest to an arena is m15 1502600, an 18 x 9 x 2.4 m box 57 m from the Monk (50001000)
    and 7.7 m below it; the others are 170 m or more away. gap: env 3037 (the ceiling check?) is
    not traced. The gourd crouched:
    CrouchItemGourdDrink / Repeat / Failed (a000_250010 / 250011 / 250015, HKS c0000_transition
    STYLE_TYPE_CROUCH), Wolf stays crouched (done 2026-10-11). Gaps: the other crouched items (no
    other item in the game yet), ACTION_ARM_CROUCH flags.
  * Bush hiding (solved 2026-10-11): common.emevd event 701 (initialised by event 0, restarts):
    IF player has 4200 "ステルス効果_茂み" AND 109200 (crouch), OR 4201 (water bushes) while
    swimming, OR 100210 (forced-crouch area) AND 109200 -> Set SpEffect 109203 (sightSearchEnemyCut
    80); once none holds -> Clear. common_func 20004000 does the same with 109012 (crouching).
    4200 / 4201 are ObjectParam SpecialEffectId_InsideHit (0.5 s, renewed while inside) of the bush
    objects o007001/2, o007280-7287, o007330, o007360, o007501/2. Placed only 2 x o007001 in m11_02
    (~183 m from Isshin's field) and 1 x o007281 in m17 (200+ m from the Ape arenas): no arena has
    a bush, so nothing to model there; a free map would need the objbnd's inside-hit shape.
    109201 "しゃがみ物陰隠密" (cut 100): no EMEVD, param, TAE or exe immediate sets it (unused or built
    in the exe); the MSB collision parts' MapVisibility (Good / Dark / Total Dark) is the likely
    shadow input, not traced.
  * Gaps: LastSightPos forget bookkeeping (forget runs on the normal target), listener ear_dist (done 2026-10-11: enemy.rs wolf_sounds / hearing_rate, FUN_140610a30: reach = rate x radius - ear_soundcut_dist, heard while dist - radii < reach + ear_dist; rate = prod(1 - hearingSearchEnemyCut/100) of the listener, FUN_140bfe590; exported hearingSearchEnemyCut; test a_deafened_enemy_hears_nothing),
    map raycasts (done 2026-10-11: stealth::sight_clear, FUN_14061c8a0; points gap), patrol routes (done 2026-10-12 for config `msb` enemies, see below: MSBS Part.Enemy WalkRouteIndex i16 @type+0x20 -> the n-th PatrolInfo event (PatrolType i32, 32 x i16 walk region indices, 5 x {region i16, ArrivalAnimID i32, ArrivalWaitTime i32}; Smithbox SDT meta / SoulsFormats MSBS); the AI reads it by GetMovePointNumber / GetPrevMovePointNumber / GetMovePointWaitTime / GetMovePointAnimId and POINT_MOVE_POINT in common_non_battle_act.lua; how the engine steps the point is not traced), turn anims (done 2026-10-12, see "Turning in place"), the real HUD art, Wolf's physics radius
    (CAPSULE_RADIUS 0.4 stands in).
  * Patrol routes (2026-10-12): tools/patrols.py -> extracted/enemies/patrols.json (152 routes on 9
    maps, boss rows left out): MSBS enemy type data +0x20 WalkRouteIndex -> the n-th type-14
    PatrolInfo event (SoulsFormats MSBS EventParam.cs: PatrolType i32, 3 x i32 0, 32 x i16 region
    indices, 5 x {region i16, 0, anim i32, wait i32}). Config enemy.group entries take `msb =
    <entity>`: the enemy stands at its MSB spot (map pieces' transform: minus the export origin,
    turned by arena_yaw, X mirrored) facing its MSB way, and its brain gets ai::Patrol.
    GetMovePointNumber / GetPrevMovePointNumber / GetMovePointAnimId(n) / GetMovePointWaitTime(n)
    / GetMovePointType answer from it, POINT_MOVE_POINT is the current point, and the route steps
    when a MoveToSomewhere(POINT_MOVE_POINT) succeeds (common_non_battle_act.lua NonBattleAct_Common
    then plays the arrival anim / wait). MoveToSomewhereSmooth 2013 runs as MoveToSomewhere. Gate
    map (m11_01, 60 m around the export origin): 8 regular enemies have routes, all "Honjo
    invasion" rows (Ochimusha 1010 1110248 / 1110236 / 1110252, Tokugawa 1700 x5); the default-state
    ones there (the General 1110414, c1010_0086) have none. Test an_msb_enemy_walks_its_patrol_route.
    Stepping (traced 2026-10-12, ai.rs Patrol): AiMovePointCtrl at AiThink +0x9060 (cur +0x9078,
    prev +0x907c); FUN_1405afe20 (1405a.c:11008) steps when a move goal to POINT_MOVE_POINT ends
    in success (Terminate callbacks FUN_1405c7b80 / c8050 / ca9b0) or on Lua StepNextMovePoint, not
    by a distance check: prev = cur, cur = next by the PatrolType table 0x143af9550 (empty slots
    skipped, 64 tries, else -1): 0 loop, 1 back and forth, 2 random, 3 loop over slots 0-15, 4
    nearest of slots 16-31, 5 on to the last point then -1 (hook FUN_1405c2710 ends the route),
    6+ never (cur -1). Spawn: prev -1, cur the first point (FUN_1405b06a0). Arrival anim / wait
    match by region (FUN_1405c1ac0 / c1b90), and 4 routes pass a region twice (patrols.json
    "regions"). IsRouteEnd (FUN_1405c20c0) = no point after prev. With cur -1 NonBattleAct plays
    the last point's arrival anim and stays. Only types 0 / 1 / 5 occur (101 / 27 / 24). Test
    patrol_points_step_by_their_type. Type 5's end (FUN_1405c2710 -> chr vtable +0x168 =
    FUN_1409f60c0, 1409f.c:3833) makes the last point (region pos +0x14, rot +0x20) the enemy's
    initial position / rotation (ChrIns +0xe0 / +0xf0, flag +0x1a15 bit 3 = IsChangeInitialPosition,
    FUN_1405e4fc0): Patrol::new_home -> enemy home (patrols.json "yaws"). Route-end anims (ported 2026-10-12): m11_01
    11115236 / 11115238 (1110236 / 1110252, both patrollers; 11115237's 1110296 has no route)
    wait for the enemy inside region 1112238 / 1112252 (3[2] on itself; MSB shape 2 = sphere
    r 1.2) OR its AI state 1 / 2 / 3 (4[9]); alerted it ends, else 2003[18] Force Animation
    Playback 20004 (Should Loop 1). The exe never reads that loop byte: 2003[18] (FUN_140904ef0
    case 0x12, 14090.c:8683) -> FUN_140bd84c0 keeps the anim id only, and the per-frame
    FUN_140bd81b0 (140bd.c:5586) fires W_Event20004 and clears it the same frame. The state runs
    EventCommonFunction (c9997.lua:3884; damage / death / fall take it) to its end (cxxxx.lua:49
    env(339) -> W_Idle); its AI cancels (env 107 / 118 / 120 / 123-125, FUN_140b36410 140b3.c:9548-
    9669) need TAE flags 23 / 79 / 86 / 122-124 (ActionCtrl +0x1f8, FUN_140b51b70), which
    a000_020004 lacks. enemy.rs RouteEndAnim / route_end_anim, patrols.json "route_end"; test
    a_patroller_plays_its_route_end_anim_once. MSB shapes are SoulsFormats' ShapeType (1 circle,
    2 sphere, 3 cylinder, 4 rectangle, 5 box): WakeRegion read 1 / 2 as sphere / cylinder before
    (test msb_region_shapes_follow_the_shape_type; the wake regions are all boxes). Patrol arrival (ported 2026-10-12): MoveToSomewhere(POINT_MOVE_POINT) succeeds when the
    chr is inside the current point's MSB region (update FUN_1405caa00, 1405c.c:7420 ->
    IsInsideMovePoint FUN_1405e55c0 -> FUN_1405ae370 -> FUN_1409825b0, 14098.c:2274: 1 circle XZ,
    2 sphere 3D, 3 cylinder 0 <= dy <= h (FUN_140983380), 4 / 5 oriented box; radius tests are
    strict). The goal's distance param is 0 on patrols. Points are spheres (1313) or cylinders
    (122); 51 are point regions (exact equality: those end by IsArrived alone). IsArrived
    (FUN_1405e4f20 -> FUN_1406052e0 -> FUN_1405b9070, 1405b.c:6531) runs first for every move
    goal: distance < the chr's radius (vfunc 0x68, inferred) + the target's extent (+0x1c, 0 for
    POINT_MOVE_POINT) + the goal's distance for dir type 0 / 9 (0 on patrols), strict, 2D or
    3D by a flag (+0x20, gap: not mapped; read in XZ); a command with no target is arrived. patrols.json "areas", ai.rs
    Patrol::inside_current; test an_msb_enemy_walks_its_patrol_route (steps at the 1.2 m edge).
    gaps: whether the invasion rows are active in the default map state. MSB enemies start
    unaware (enemy.rs StartUnaware -> make_unaware; the duel line-up starts fighting), so the
    route runs in play.
  * Guard stance (2026-10-12): GOAL_COMMON_Guard's usual action 9910 is c9997's ACTION_TYPE_GUARD
    (hks_src/c9997.lua:258), not an anim: it was played as the missing a000_009910, so the goal
    failed and enemies never held their guard. Now (ai.rs Command.guard, enemy.rs guard_stance):
    from idle W_GuardStart (a000_401050) -> W_GuardIdle (500000 loop); while asked for, moves walk
    guarded (GuardWalkFront / Back / Left / Right 50500x, move_table_guard), turns are
    W_TurnGuard_* (50540x), attacks start from it (IdleCommonFunction(STYLE_GUARD)); the request
    gone, W_GuardEnd (501040; ExecAIAction 1134-1140). All the guard clips hold ChrActionFlag 3
    ShieldBlock, so combat.rs blocks Wolf's hits in the stance. Test
    the_guard_goal_holds_the_guard_stance. Move goals carry a guard action as their last param
    (MoveToSomewhere 8, which ApproachTarget / ApproachSettingDirection fill; LeaveTarget 4;
    SidewayMove / KeepDist 5): 9910 moves in the stance (e.g. LeaveTarget(4, TARGET_ENE_0, d,
    TARGET_ENE_0, true, 9910), 22 of 317 LeaveTargets with the literal); test
    a_move_goal_with_9910_moves_guarded. The general stance 9920 (ACTION_TYPE_GENERAL_STANCE,
    c9997.lua:259; ExecAIGeneralStance 1321-1375, ExecMove 1609-1632) runs the same way (ai.rs
    Command.general, enemy.rs Stance): W_GeneralStanceStart401060 -> GeneralStanceIdle (600000),
    GeneralStanceWalk* / RunFront (605000 / 605010), GeneralStanceTrun_* (60540x),
    W_GeneralStanceEnd601040 when the request stops or another action comes; asking for one
    stance in the other ends the other first. Used by KeepDist / SidewayMove /
    ApproachSettingDirection (common_common_func2.lua) and 132000 / 155060 / 510000 / 720000
    battle scripts. Test the_general_stance_runs_like_the_guard. gaps: W_GuardRun (a run walks
    front), the start clips' cancel windows, Its own attacks (ported 2026-10-12): an AI action
    603000-603009 / 601040-601044 out of the stance starts it first (W_GeneralStanceStart401060,
    the goal keeps asking), in it plays W_GeneralStanceAttack<id> / End<id>, back to the stance
    idle; a start in the stance ends it (enemy.rs general_stance_play; c1190 / c1320 have them,
    119000 / 132000 / 155060 battle scripts ask). They count as attacks (is_attacking: flag-73
    windows). Test the_general_stance_attacks_start_the_stance_first.
  * Turning in place (2026-10-12): c9997 ExecTurn (hks_src/c9997.lua:1488-1563) from idle and at
    the cancel of attacks / steps / damage (enable_turn TRUE; never from a turn or a walk / run):
    |TurnAngle| >= 45 (or RequestEmergencyQuickTurn, env(330): AI +0x9054, FUN_1405e76e0) plays
    W_Turn<AI state>_<Left|Right><90|180> (< 135: 90), not under SpEffect ref 1000072. Clips
    a000_005400-3 / 105400-3 / 205400-3 / 405400-3 / 505400-3 (L90 R90 L180 R180); c1010 / c1020
    have no 2054xx, c1700 only 4054xx / 5054xx (those states keep the turnVellocity spin). Root
    motion turns them (no SetTurnSpeed); AI flags 23 / 86 / 79 / 78 open from 0.4 s (90) / 0.6 s
    (180). enemy.rs start_turn, Actor::yaw_scale; the AI's TurnTo (FUN_1405e89f0, target at
    +0x9050) is the request. Test an_enemy_turns_in_place_with_its_turn_clip (150 degrees off
    home: TurnDefault_Right180, lands on the home facing). Scale (traced): act(9105, 90 / 180)
    (FUN_140b32410 case 0x2391 -> FUN_1407de170, BehaviorTurn +0x30) and act(9104) (corrected-turn
    flag); FUN_1407db220 (1407d.c:8110-8127) sets the root-motion rate (motion +0x904 / +0x90c,
    used at 140bc.c:5150) to |yaw error| / that angle: Actor::yaw_scale. TurnAngle = AI desired
    facing (+0x73e4 from the TurnTo target +0x9050, FUN_14060c7d0) minus the chr yaw, degrees,
    right positive (FUN_140b4c600, 140b4.c:4531). env(106) = ActionCtrl +0x594 <- the AI's
    SetAttackRequest (+0x7430) else StartGuard (+0x7414). gaps: the leftover error's 720 deg/s
    correction (FUN_1407dab40), the 0xd branch (1407d.c:8046); the flight / crouch Default
    rule not run (Guard / General stance turns: see Guard stance).
    2026-10-14: a move request from idle turns first. ExecActiveTransition's order is ExecAIAction,
    ExecTurn, then the move (env 3014), and the move goals set the turn target themselves: the
    exe's Approach / MoveToSomewhere (FUN_1405caa00) passes param 3 to TurnTo (FUN_1405e89f0,
    +0x9050); FUN_14060c7d0 (from the AI update FUN_140608990) reads TARGET_SELF (-1) as the move
    point (+0x73d0, only while a move is requested: FUN_1406040b0 == 4) and -2 as no turn. Our
    driver (enemy.rs, the move arm) tries start_turn before the loco state; the run follows from
    the turn clip's AI flag 78. Test: the patrol test a_patrol_point_with_a_zero_region_is_reached_by_is_arrived
    (the point 177 deg behind: TurnDefault_Right180, then RunFrontDefault).
  * Map AI hold (2026-10-12): m11_01 runs common_func 20005110 on 1110236 (15 patrolling enemies
    on 9 maps have one; patrols.json "wake", tools/patrols.py): 2004[1] Set Character AI State
    off, then on with Force Animation -1 once Wolf (10000) is inside its region (3[2]; 1110236's
    is 1112236 "Event_巡回起動領域_地下牢前の橋", a box w37 d81 h13 turned -48°), it has one of
    its 4[5] SpEffects (3500-3503, 3520, 8300, 220018, 230110/1, 9045, 230518/9) or Wolf hurt it.
    enemy.rs AiHold / ai_hold does the same for config `msb` enemies (held = the map script's
    AI off: idle, no senses). MSB region shapes: +0x10 type (SoulsFormats ShapeType: 1 circle r,
    2 sphere r, 3 cylinder r h, 4 rectangle w d, 5 box w d h; data at +0x48); boxes and cylinders stand on their spot (DSMapStudio
    RenderableProxy.cs:1364). Test an_msb_enemy_waits_for_wolf_in_its_wake_region. gaps: a
    guarded hit (no HP lost) does not wake it; Force Animation -1 not played. Only Wolf's hits
    wake it (react_from by_wolf, 2026-10-12 round 14: a rival's blow did too).
  * Goal life end (2026-10-12 bug hunt): a goal whose life runs out while its update still
    says Continue is a Success only if it or a goal above it has SetLifeEndSuccess (0x1405faa20
    sets goal +0x4a0 bit 0x20; FUN_1405d06c0 1405d.c:1970-1985, FUN_1405d1b50 walks up), else
    Failed (the parent fails, queued subgoals dropped). Was: every timed-out goal succeeded, so
    a timed-out approach let the queued CommonAttack swing from out of reach. Started Attack /
    SpinStep still run to their end; Wait / Stay / Guard end by life as a success (gap: their
    updates not traced); FUN_140605ea0 (+0x7420) exception not traced. ai.rs life_end; test
    a_timed_out_goal_fails_unless_life_end_success; all_enemies_full still 94 / 110.
  * AI pacing vs the live recordings (2026-10-12, task 4). Live: memscope-data/logs rec_*_20261009,
    enemies tracked by ChrIns handle (slots 1 / 2 swap every 60 ticks), body anim = TimeAct +0x100
    without the overlay layers (face 7900xx, 9200, 9500-9503, 9600, 9700, 9999, 40000, 41500-41502),
    attack = an anim whose TAE has a type 1 / 2 / 304 event; script kept in the session scratchpad
    (pacing/enemy_pacing.py, not in the repo). Sim: `ai_pacing` (ignored test; SHINOBI_PACE=chr:row),
    Wolf only holding his guard for 90 s.

    | enemy | source | attacks/min | start->start median (mean) | end->start median (mean) | chained <= 0.2 s | distance median (min-max) |
    |---|---|---|---|---|---|---|
    | General 10203010 | live b+c+d, 121 attacks / 238 s, Wolf fighting | 30.4 | 1.54 (1.99) s | 0.52 (0.85) s | 28 % | 2.40 (0.95-6.59) m |
    | General 10203010 | sim, Wolf guarding | 22.7 | 1.85 (2.63) s | 0.00 (1.06) s | 61 % | 2.1 (0.9-6.5) m |
    | Ochimusha (4 rows pooled) | live c1010_b, 22 attacks / 47 s | 28.3 | 1.63 (2.17) s | 0.32 (0.67) s | 44 % | 2.2 (0.68-6.07) m |
    | Ochimusha 10100000 | sim | 24.7 | 0.97 (1.31) s | 0.32 (0.23) s | 28 % | 3.3 (1.7-4.9) m |
    | Ochimusha 10100110 (a100 set) | sim | 14.0 | 1.18 (4.47) s | 0.32 (3.38) s | 10 % | 3.6 (1.9-4.5) m |

    Reading: the gaps and distances are in the same range; the sim General chains more and attacks
    less often, the sim Ochimusha attacks from further out (its a000_003051 x11). Not comparable
    one to one: live, 43 % (General) / 74 % (Ochimusha) of the time between attacks is recoil from
    Wolf's deflects and hits (8600 1.6 s, 8010 / 8050) and ~10 % of the General's starts are cancelled
    into guard 3100 / 3101 when Wolf attacks; the sim Wolf never attacks. Most used live: General
    3000 x25, 3008 x14, 3004 x14, 3007 x10 (chains 3000 -> 3001 -> 3002, 3008 -> 3009 / 3067); sim
    3000 x7, 3001 x5, 3002 x4. No tuning made from this. Data notes: rec_c1010_20261009 is Hanbei
    (c1012, 10120000) with c1001 dummies, not c1010; rec_c1020_20261009 is a deathblow test (no
    attacks); rec_c1020_b/c/d are one General (10203010); rec_c1010_b has 4 Ochimusha rows
    (10100110, 10100000, 10100100, 10100300), each only briefly in the slots. gap: a like-for-like
    recording (Wolf only guarding, one regular enemy, 2+ min) would settle the chain rate.
  * Gate map forced crouch / shadow (2026-10-12): no 20004001 (forced crouch, 100210) in any
    m11_01 EMEVD, so none on the gate; 109201 "crouch shadow stealth" has no setter in EMEVD, params,
    TAE or HKS. The likely shadow input is the hit parts' MapVisibility (collision type data +0x27,
    0 Good / 1 Dark / 2 Total Dark; FUN_1409b5c80 -> chr +0x1acd; FUN_140617150 puts it in the
    sight query, remapped by the target's SpEffect mapVisibilityOverride*, scaled by query
    +0x58[type] 1 / 0.5 / 0.3 in FUN_14061ae40 / FUN_14061c2a0; NpcThinkParam disableDark skips
    it): all 221 m11_01 collision parts are Good, so the gate has no dark places. Not exported;
    other maps have Dark ones (m11_00 2 + 4 Total, m17 19 Total, m20 7).
  * Footsteps: Wolf's walk / run clips carry CreateAISound (TAE 237) for their whole length - walk
    1000 (0.5 m), run 1010 (2 m), crouch walk 1001 (0.25 m), crouch run 1011 (0.5 m); read from the
    clip on screen (Actor::shown_clip), radius x hearingSearchEnemyRate when bSpEffectEnable. 103 tests.
- All enemies (2026-10-10, docs/reports/enemies.md):
  * NPC damage is atkPhys + atkMag + atkFire + atkThun + atkDark.
  * opposeTarget 0 = hits objects only.
  * NpcParam resident spEffectID0-31 open the 950/951 TAE StateInfo gates.
  * NPC bullets: BehaviorParam 200000000 + variation*1000 + judge with refType 1.
  * Enemy grab: ThrowParam AtkChrId = chr whose throwKind is a ThrowKindParam row with throwCheckSituation 4 and the hit's AtkParam throwTypeId (Paramdex SDT ThrowKindParam throwTypeId; data, 2026-10-11; test enemy_grabs_go_through_the_throw_kind_table). Underwater kinds (category 101, c1340 only) used only when the chr has no other (gap: no water).
  * ninsatuNum > 1: a non-final deathblow is ThrowDef only, then IdleTransition (HKS ThrowDef, not
    env(276)). The boss characters (c5000, c7100) have no ThrowDefDeath anims at all. HP refill: done 2026-10-11 (below).
  * Bosses read GetNinsatsuNum < GetNinsatsuMaxNum as "a deathblow was taken" (500000_battle.lua), and
    the General's last one (<= 1) as all-out (102000_battle.lua).
  * Goal setters chain (AddSubGoal(...):SetLifeEndSuccess(true):TimingSetNumber(...)).
  * Anim sets: SpEffect 200030-200034 (stateInfo 270-274, CMSG offsetType 15) = a000-a400; from
    residents or TAE 66/67/401. NPC TAE ids carry the group (100003001 = a100_003001).
  * Boss phases are map events: EMEVD 4[37] IF Number of Character Health Bars on the boss, then
    SetSpEffect / ClearSpEffect / ForceAnimationPlayback / EzState request / AI command (2004[17],
    read by GetEventRequest(slot)) / AI re-plan. At 0 bars: EzState 20200 + Wolf's 7102xx.
- Several enemies (2026-10-10):
  * Combat.kinds and Actor.kind (data_of / foe_of / npc); config enemy.group.
  * Same side = no hits.
  * Lock-on flick switch (gap: the exe's rule); deathblow on the locked / nearest enemy.
  * Per-enemy HUD bars and stealth marks.
  * Boss events "enable" / "disable" / "warp_player" (MSB part / region positions relative to
    the boss part: MSBS Part pos @0x20 / rot @0x2C; Region pos @0x14 / rot @0x20, entity id at
    base data 3 + 4). Yaw convention: facing = (sin y, cos y) (msb::rotate).
  * Genichiro c7100 71001000 -> Tomoe c7110 71100000 (11115820, after a 2 s wait for Wolf alive
    and the cutscene 11010020 that warps Wolf to region 1112813).
  * Monk phantoms c5005 (12505961 enables 2500851/3/4). Event 12505970, per phantom: AI command
    (0, slot 1); IF the Monk has SpEffect 5031; wait 1 s (skipped on her message 50); wait
    1 / 2 / 3.5 s; IF she still has 5031, short-warp to 2502860-5 (by Wolf's region
    2502856-9 and a random flag 12505973-8); commands (10 / 20 / 30, slot 1) and (10, slot 0) +
    re-plan; IF its message 70 OR she loses 5031: (0, slot 1) + re-plan; restart.
  * Event messages on NPCs are TAE 231 (DSAnimStudio SDT template: SetEzStateRequestID; DS3
    TAE3 SetEventMessageID). exe decomp/140b5.c case 0xe7 writes the id to the character
    (module +0x508) and nothing clears it at the event end: sticky until the next 231 (attacks
    send 2147483647 to clear). c5000 / c5005: a0/a100_003032 sends 50 at 1.733 s (with 5031),
    the a200 attacks send 70. Wolf's Todome message is TAE 936 (unk0) instead.
  * TAE 66 AddSpEffect_Multiplayer applies its SpEffect like 67 (5031 is only on 66).
  * Region checks: 3[2] In/Outside Area (byte0 condition group, byte1 1 inside / 0 outside,
    entity, region); MSBS Region shape @0x10 (5 = box), shape data @0x48 (box: width x, depth
    z, height y). boss_events.py turns them into `areas` (OR of ANDs per GOTO label).
- 2026-10-11 (bosses finished, HANDOFF 4.11):
  * Corrections: c5200 = Divine Dragon; c5300 = Old Dragons of the Tree (Okina dragon group);
    c1350 = the Headless; c7300 Divine Child = friendly NPC.
  * AI `IsEventFlag` must return a boolean (Lua: 0 is true). It reads the boss script's flags.
  * AI `SetEventFlag(flag, on)` writes the fight's flags (the boss script's, else `enemy::AiFlags`,
    cleared on a fight reset); the setter sees it at once, others next frame. Users: 504000 (sets
    19625680-93 every Update by Wolf's SpEffects / observe areas), Lady Butterfly 509000 (19952081/2,
    read by her illusions 151010), 11005933, 11125650, 19625480-5, 19625862. None of these is set by
    any EMEVD, so they start off. Map flags the AI reads (11100850, 117006xx, 12505855/57 = event
    12505851 slots 4/6 done, 12505875-901, 62000705) come from the boss script run; with no run they
    are off (gap: the world progress of a non-boss fight).
  * Anim-set SpEffects 200030-200034 share spCategory 100 (Paramdex SDT: "category that decides
    overwriting"): a TAE-added one with effectEndurance -1 replaces the resident of that category.
  * TAE 193 SetOpacityKeyframe (OpacityAtEventStart -> End): the Monk's phantoms fade in / out
    in their a200 attacks and hold 0 in their a200 idle. gap: the hold between keyframes.
  * Teams (Paramdex TEAM_TYPE; 0 disabled, 6 enemy, 24 enemy 2, 26 friendly NPC, 29
    indiscriminate, 30 object). Evidence by NpcParam row names: 11502001 / 14701003
    葦名勢敵対用, 11502100 猿敵対用, 17000110 落武者敵対, 11007200 孤影衆と敵対しない,
    50200010 無差別. gap: the exe's table.
  * Great Serpent c5010 (hp 9999): fall throws 15010030-32 (throwKind 70000/70100/70900,
    Dist 57 from dummy 233), 15010150/151 (130000), 15010900 (120000, ThrowDef12900); AI
    501010/501020/501030_battle.lua by IsInsideTargetRegion over MSB regions (1102861/62/65/
    81/95/97/79). Not done: ai.rs answers region checks false.
- 2026-10-11 non-final deathblow HP refill (B1): every chr's ThrowDef reaction anims add SpEffect
  300610 "HP reset" (HPリセット, changeHpRate -100, effectEndurance 0) by TAE 67 AddSpEffect
  (c7100 a000_012000 f130, a000_013410 f99; also 401000 "enemy HP full recovery" in c5100's
  a000_020000 revival). enemy.rs `instant_hp_rate` applies instant changeHpRate SpEffects on any
  living enemy at their frame; the deathblow no longer refills at once (only when the reaction
  has no such event). Test `a_boss_refills_at_its_reactions_hp_reset_frame`. Same rows also add
  275080 (HP / posture total change) and 3710020 ("rival deathblowed: clash count 0"): not used.
- 2026-10-11 C2 runtime encounter: `enemy::SpawnEncounter` -> `Combat::load_encounter` (same load as startup:
  main + group + boss-event kinds; params merged) -> `AnimLib::sync_kinds` (per anim chr, Wolf's anim_c0000_<chr>
  clips) -> FightReset. `data::encounter_list()` from roster.json + boss_scripts.json. Respawned actors get their
  skeleton / model in Update now (were PostStartup only: after a reset the enemy had none). Tests
  spawn_encounter_swaps_the_fight_in_place, enemies_respawned_by_a_fight_reset_get_their_bodies.
- 2026-10-11 C2 names + foreign arena: `tools/npc_names.py` -> extracted/enemies/npc_names.json: every
  2003[11] Display Boss Health Bar / 2003[87] Display Miniboss Health Bar (state 1, entity = MSB enemy part,
  nameId -> msg/engus item NPC名.fmg; NpcParam nameId is 0 on these rows) keyed by the part's NpcParam row
  (57 rows: 20 bosses, the mini-bosses), plus place names (地名.fmg; gap: id = area*100 + block*10, the 11
  ids match the 11 blocks, lookup not traced). `Encounter.name` uses it (shared name: "(place)", then a
  number), `Encounter.bar` = has a bar. Test encounters_carry_the_health_bar_names. After a swap the map stays:
  `enemy::on_own_arena()` (loaded map == map::boss_arena of config enemy) - on another boss's arena the
  line-up stands at map::player_start().z - 8 and events::place drops MSB heights (the Ape grotto hid
  Genichiro under its roof). Map asked to make player_start find room for both on a foreign arena.
- 2026-10-11 B5 enemy bullet flight (enemy_bullet.rs) from the exe bullet update FUN_140b0ded0 (bullet
  0xd30 bytes, BulletParam at +8 of its slot): FollowType odd or 2 -> FUN_140b14b20 (attached; gap), else
  FUN_140b11270 homing, FUN_140b14650 speed, then the move (decomp 140b1.c:5214 adds the step
  length to +0xb60 = metres flown).
  * Homing `steer`: none if homingAngle (0x82) < 0 or flown <= homingBeginDist (0x40); target point +
    targetYOffsetRange (0xa0) in y; no turn while closer than hormingStopRange (0x24); else the flight turns
    toward it by at most homingAngle*dt (acos of max(cos(limit), dot), about cross(dir, to)). homingAngleX
    (0xda) >= 0 also lets a full aim through (gap: its pitch limit). Target = FUN_140b0e680: the target
    chr's matrix * a local offset or one of its dummy polys (id at +0x34 not traced; we aim at Wolf's chest
    like `fire`).
  * Speed `flight`: flown <= +0xabc (taken as `dist`, source not traced): accelInRange (0x2c) along the
    flight and gravityInRange (0x1c) down, else the OutRange pair (0x30 / 0x20); then speed = min if below
    minVellocity (0x38), else at most maxVellocity (0x34) (rows with max 0 all have init 0). Gaps: water
    slow-down FUN_140b123d0, the +0xb6c accel pause. Old code had no in-range accel/gravity or limits (the
    bandit arrow 190: 22 m/s, 15 m/s2 gravity in its 10 m, 8 deg up).
  * Map collision: isPenetrateMap 0 -> Terrain raycast along the step ends it (HitBulletID child as a
    hit); no map: the old floor (Wolf's feet).
- 2026-10-11 B5 AI command slots: Request Character AI Command (2004[17]) -> FUN_1405b4730 writes slot
  0-3 at AI +0x743c and sets bit `1 << slot` at +0x10; FUN_1405b4f10 moves +0x10 to +0x14 each update (lua
  IsEventRequestReceived, FUN_1405e51f0 / FUN_1405b4590; no AI script uses it). GetEventRequest
  (FUN_1405b36b0) reads without clearing; only the AI reset FUN_140602020 -> FUN_1405b2520 refills -1.
  Ours matches (slots kept; respawn = new Enemy). Not a gap any more.
  * teamType (gap, 2026-10-11): no two-team relation function found. A switch over team values
    0..0x20 in decomp (FUN_140c7dda0) takes one team only (multiplayer checks); combat.rs
    teams_hostile stays name-based. A scan of the whole exe for a symmetric 25-40 square team matrix
    (bit rows u32/u64, byte/short cells) with Enemy(6) x 24 hostile found none (only a generic mask
    table @file 0x2f34a80): the relation is code, not a table.
  * EMEVD 2004[2] Set Character Team Type (not run, no effect; 2026-10-11): in the boss scripts it
    only flips the boss away from 6 and back. 71000000 11125810, 71101000 11125840 and 50600000
    11115860 set 26 (Friendly NPC) before the cutscene, behind the event's start point (its last
    Enable), and set 6 after it. Demon of Hatred 70200000 11105910 sets 28 (Co-op NPC) after the
    start point, but Wolf starts inside region 1102910, so 11105901 -> 11105924 sets 6 within 30
    frames (checked with a probe test).
  * Bullet FollowType (done 2026-10-11): of the enemies' bullet rows only c5200 52000980 has one,
    3 "follow target" (Paramdex BULLET_FOLLOW_TYPE), a warning: AtkParam 8 "[System] dummy bullet",
    hitRadius 10, life 0.43, SpEffect 211000 "dangerous enemy attack VFX" (vfx 48100, 0.1 s).
    enemy_bullet::fly keeps it on Wolf's chest. gap: its EmittePosType 6 "above and behind the
    target" offset (it rides on Wolf anyway).
  * Grapple targets (done 2026-10-11, kb/grapple.md section 4): the export keeps SpEffect
    chrWireVariationNo / chrWireTargetDmypolyId / chrWireLandingPointDmypolyId (data::SpEffect
    chr_wire_*). enemy/grapple.rs: wire_effect(actor) = the first TAE (66/67/401) SpEffect, else
    resident, with a variation >= 0; EnemyGrapples (PostUpdate, after transform propagation) lists
    each live enemy's hook / land dummy world positions, variation, homing row 300000000 +
    variation. Combat::wire_throws(foe): ThrowParam throwKind 180000 / 180100 / 180900 (the
    Ogre 15020170/71, Gyoubu 15080170/71/79); throwTypeId 3023/3123 alone also matches the falls
    (70000), crush falls (130000) and kick-downs (160000). Test
    the_ogres_grab_opens_the_grapple_in_its_window. The player move (VXf) and the search (Map)
    use them.
    The hook stuck in an enemy: message grapple::WireHit(entity) (Wolf's AtkParam 9500000 "wire
    stabbed" gives SpEffect 510000, ref 1000410). With ENABLE_WIRE_DAMAGE0-4 (refs 1000400-04,
    SpEffects 510010-14 from its TAE 66: Ogre grabs 510010, tackles 510011; Gyoubu drifts; the
    Ape's sword) it plays DamageWire<n> (a000_008910 / 008911; c9997 GetSpDamage /
    ExecSpReaction). Test the_hook_in_the_ogres_grab_staggers_him. gap: DAMAGE_TRANSITION_RANK__4.
    The hook attack's kill is a deathblow (AtkParam_Pc 5000660 "deathblow body, fall", throwFlag 2,
    x600; the Ogre's 13610 ends with 300610 "HP reset"): VXf calls Enemy::deathblow(13610/13690).
  * Ledge-hang / wall-hug deathblows (2026-10-11): Combat::throws_of_kinds(foe, kinds), ThrowKind
    110000/110010 hanging (12800; 13000), 90000/90010 left wall (12600), 100000/100010 right
    (12700); 48 common kinds have them, no boss. Test the_ochimusha_has_his_hang_and_hug_deathblows.
  * Owl phase 2 (2026-10-11, from AKJama/sekiro-rs b997933, docs/OWL.md): no deathblow while his
    TAE marker 3506080 is up (a000_020005 0-10.27 s, 021003 loop, 20006-20008 first 0.233 s);
    player::deathblow_check. His castle Lua escape 20007 on Wolf's attack runs as is
    (INTERUPT_ParryTiming). Test the_owl_has_no_deathblow_during_his_heat_up. gap: the exe's
    test of the marker; ChrActionFlag 24 (no damage motion) not modelled.
  * Posture recovery HP gates (checked against sekiro-rs COMBAT-FUNDAMENTALS): the resident
    conditionHp SpEffects multiply (actor.rs product: 0.6, 0.3, 0.1 below 80 / 60 / 40 %), same.
  * Bullet decay (2026-10-11): past `dist` a bullet's hits lose damageDamp / staminaDamp /
    knockbackDamp percent per second (Paramdex "decreases in 1 second after the attenuation
    distance"; 100 on many rows, e.g. c1060 / c1450 / c1470 / c5060), linear (gap: curve not
    traced). Attack::decayed, enemy_bullet.rs; test a_bullet_past_its_range_weakens.
  * Deathblow absorb vs walls (2026-10-11, after sekiro-rs docs/DEATHBLOW.md): the snap onto the
    throw dummy goes through Terrain::slide; what a wall stops, the other character gives way
    (player::follow_throw). gap: the engine's handling is not traced; no sim test (needs a map).
  * Enemy posture-damage SpEffects (2026-10-12): an enemy's own TAE / resident SpEffects scale
    the posture it takes by def<Attr>StaminaDmgRate for the attack's staminaPhysicsAttribute
    (combat::def_stamina_rate; hits, blocks, deflects). Isshin's change a000_003015 holds 3540090
    "Revived Kensei posture damage suppression" x0.3 (and 5350: no damage anims, not modelled).
    gap: product vs other combination of several not traced.
  * Native goal distances (2026-10-12 bug hunt): CommonAttack successDist, LeaveTarget and
    KeepDist compare against GetDist's distance (centre minus the AI chr's own radius): their
    updates call the GetDist thunk 0x1405e2c10 (r8b 1; CommonAttack 0x1405c6d30 at 0x1405c7073 /
    0x1405c70cd, LeaveTarget 0x1405c80d0 at 0x1405c8163, KeepDist 0x1405c7c40 at 0x1405c7cb9);
    the plain thunk 0x1405e3fb0 is used by no goal. ai.rs goal_dist (was the centre distance:
    enemies stopped short / attacked from too far by their radius). gap: MoveToSomewhere's
    arrival (successDist) is in its move subgoals (0x1405caa00 / 0x1405c9df0), not traced; read
    the same. SidewayMove has no distance. Test native_goals_measure_distance_like_get_dist.
  * Breaking-deflect pose (2026-10-12): Wolf's ThrowBreak pose and hold play only if the enemy
    entered its ThrowDef<def> (react refuses while Rising / Held / Broken); before, Wolf could be
    left in the close-up with nothing to finish (user's Isshin report). Not reproduced in the
    sim; player.rs logs "deathblow refused: ..." with the reason.
  * Moved fights (2026-10-12): on another boss's arena (Map's MOVED: Isshin, Isshin Ashina, Emma,
    the Owl on boss_71001000) the boss script's CutsceneWarp and Warps to a place are skipped:
    its places are in its own map's frame (Wolf landed at the railing). "Moved" = the index's
    map_boss_arenas.json "moved" list (tools/boss_arenas.py MOVED; enemy::own_arena_of): the Owl,
    Emma and Isshin Ashina share m11_01 with the castle top, so the same map is not enough.
    Map's probe: Wolf stays at (0, 0.91, 4) through 7 s on the Owl and Isshin.
  * Own arena (2026-10-11): enemy::on_own_arena = map::loaded_map() is own_arena_of(fight row): the
    row's map_boss_arenas.json stem whose map_<stem>.json "map" equals its boss_scripts.json "map"
    (Monk, Butterfly, Apes, Dragons, and 54000000 / 71000000 / 71101000 on Isshin's field m11_02).
    set_fight(chr, row) at startup and on SpawnEncounter; a swap's line-up waits one frame for
    map.rs switch_map.
- 2026-10-12 sekiro-rs ports ("bug fixes" findings; THIRD_PARTY.md):
  * Super armour (sekiro-rs c0f97ee): the damage level an enemy reacts to is 0 under ChrActionFlag
    24, else swapped by its active spCategory 1001 SpEffect (lowest categoryPriority; stateInfo
    120 needs a super-armour meter, skipped) through dmgLv_* (FUN_14084ff70 -> bfd980 -> 10cea20
    -> 10b5100; enum table 2b979e8 [0,8,1,2,3,7,4,6,9,5,10,11]); guards and level 0 bypass it.
    combat.rs damage_motion_level; replaced the flag-73 proxy. Test
    super_armour_swaps_the_damage_level_in_its_window (c1010 a000_003010 5358: 2 -> 8).
  * Aerial reactions (sekiro-rs c0f97ee character.rs:1045-1074; c9997 IsDamagedAerial = env 201
    false or ref 1000060): ExecDamageLargeBlow 4 / 9 / 10 -> DamageAerialBlow (8150),
    ExecDamageDefault 2 / 3 / 5 / 6 -> DamageAerialBack (8141, from behind) / Front (8140), else
    the additive flinch; 7 / 11 keep ExecDamageBlow. combat.rs enemy_damage_state_air. gap: env
    201 read as lifted 5 cm above a SetNoGravity leap's start. 15 chrs have the anims. Test
    an_enemy_hit_in_the_air_takes_the_aerial_reactions.
  * Guarded bullets (sekiro-rs a8351b9 fight.rs:644, docs/PROJECTILE-GUARD.md): a blocked or
    deflected bullet gives its shooter no recoil, posture or bound reaction (inferred from the
    rows: every NPC bullet AtkParam has 0 attacker posture and no guarded reaction; the exe's
    bullet-to-owner dispatch is not traced). Test a_guarded_bullet_leaves_its_shooter_alone.
- 2026-10-12 round 15 (ours): enemy guard clips follow c9997 ExecGuardBlock for blocks and
  deflects (combat.rs enemy_guard_block: refs 1000090 / 1000100 / 1000101 / 1000105, next clip
  when one is missing; c1180's shield 400230 deflects without ref 1000090 so plays the plain
  guard); a broken Wolf is knocked down only at levels 1 / 2 / 3 / 5 (or broken again in
  StandDamageBreak), minimum nothing, the rest normal (broken_takes_normal_damage); a crouched
  gourd drink raises INTERUPT_UseItem (enemy.rs wolf_starts_an_item). The flag-73 mid-swing
  gate stays beside the super-armour swap (live rec; sekiro-rs has no such gate).
- 2026-10-12 EXLARGE guard (c9997 guard_damage_table 4: a blow on guardLevel 1, an ex blast on
  1-3): ExecGuardBlock has no clip for it and returns FALSE, so ExecDamage goes on to
  ExecDamageLargeBlow: DamageLargeBlow (8130), in the air DamageAerialBlow (8150); without the
  clip nothing (ExecDamageDefault has no blow branch). It played GuardDamageLarge before.
  combat.rs enemy_guard_block (aerial: enemy_is_aerial); test a_blow_on_a_weak_guard_launches_the_enemy.
- 2026-10-12 map AI hold, the damage wake: EMEVD 4[1] IF Character Damaged By / 3[23] IF Damage
  Type (only types 0-2 are evaluated) read the SprjEventDamageInfo list (DAT_143d56058 +0x38),
  written by FUN_1406d8950 (from FUN_140b6e150, 140b6.c:8033) only when the hit's final HP damage
  (+0x1e0) > 0 and +0x1cb == 0. Guard / deflect hits run the same chain but a full guard cut
  leaves HP damage 0: no record, still held; a guard that chips HP would wake. enemy.rs ai_hold:
  HP lost since its last check + a reaction Wolf caused (react_from) since then. Test
  a_blocked_hit_does_not_wake_a_held_enemy.
- 2026-10-12 round 16 (ours): ChrActionFlag 24 on the defender skips the knockback (and the
  attacker's leftover recoil) and, for Wolf, the reaction (GroundWireShoot keeps its grapple;
  exe b6bfe0, sekiro-rs fight.rs:660-666); Wolf's HP damage sets the hold's damage wake with or
  without a reaction; MSB enemies respawn on their spot (no 2 m -Z offset).
- 2026-10-12 round 18 (ours): IsTargetGuard / GetSp / GetSpRate read their target argument like
  GetHpRate (TARGET_SELF = the enemy itself: Snapshot.self_guard / sp; otherwise Wolf or the
  rival: target_guard / target_sp / target_sp_rate). Area doping now scales HP / posture /
  damage: the SpEffect export gained maxHpRate, maxStaminaRate and physicsAttackPowerRate (7013 =
  2.0; before they read 1).

## Grab escape (exe trace, 2026-10-13, read-only; paths under extracted/decomp)

- Throw state +0x18 (setter FUN_140803030 14080.c:1574): 1 thrower / 2 victim at start
  (1407f.c:6771, 6798), 3 / 4 = escaped (only FUN_1407fa530 1407f.c:6076, from 1 / 2), 5 / 6 = death
  pair (1407f.c:5661), 0 at cleanup (140be.c:858, not from 6).
- escHp: ThrowParam +0x2c copied to state +0x1a (14080.c:80; 1407f.c:6771-6776; 140be.c:880-888).
  FUN_140bdf220 (140bd.c:11248) adds a delta, clamps >= 0, no-op unless the row's escHp > 0; under 1
  -> FUN_140802b70 (14080.c:1691) -> FUN_1407fa530 -> state 3 / 4. FUN_140be15c0 (140be.c:980) is the
  network message path only.
- Single-player decrements: the self-escape cycle FUN_140808640 (14080.c:2740; driven 14080.c:10710-
  10723 when escHp != 0): each cycle window, presses >= count -> FUN_140802c30 (14080.c:1742) ->
  escHp -1. Window / count inferred as selfEscCycleTime (+0x2e) / selfEscCycleCnt (+0x39) (the init
  FUN_140808600's args are lost). Presses: FUN_140807eb0 (button count FUN_140960350 + a stick
  shake with hysteresis; binding is a gap). Also damage on the victim: FUN_140b68ff0 140b6.c:4425
  (-(int) of a float at +0xdc, victim flag +0x70 bit4; gap: unit / which hits).
- env 277 BEH_R_THROW_ESCAPE = FUN_140b39e90 (140b3.c:2669): the thrower's latched TAE flag 70
  (+0xc24 bit1; set unconditionally 140b5.c:7457, latched 140b2.c:6653) AND own state 3 / 4
  (FUN_140be08f0 140be.c:288). HKS c0000_transition.lua:5597-5602 -> :759 W_ThrowEscape<ThrowId>
  (HKB 600355 / 600375, c0000_cmsg.lua:1284).
- c1500: 21500100 (4110 / 600350) escHp 20, cnt 1, time 100; 21500200 (4120 / 600370) escHp 10,
  cnt 1, time 100; 21500000 (4100) and the 115000xx rows 0 (no escape).
- Gaps: thrower-side anim on escape; 1407f.c:7230-7268 callers; the cycle time unit (100 read as ms).
