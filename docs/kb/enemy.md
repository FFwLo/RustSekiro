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
    map raycasts (done 2026-10-11: stealth::sight_clear, FUN_14061c8a0; points gap), patrol routes (MSB; blocked until enemies stand at their own MSB spots: MSBS Part.Enemy WalkRouteIndex i16 @type+0x20 -> the n-th PatrolInfo event (PatrolType i32, 32 x i16 walk region indices, 5 x {region i16, ArrivalAnimID i32, ArrivalWaitTime i32}; Smithbox SDT meta / SoulsFormats MSBS); the AI reads it by GetMovePointNumber / GetPrevMovePointNumber / GetMovePointWaitTime / GetMovePointAnimId and POINT_MOVE_POINT in common_non_battle_act.lua; how the engine steps the point is not traced), turn anims, the real HUD art, Wolf's physics radius
    (CAPSULE_RADIUS 0.4 stands in).
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
  * Own arena (2026-10-11): enemy::on_own_arena = map::loaded_map() is own_arena_of(fight row): the
    row's map_boss_arenas.json stem whose map_<stem>.json "map" equals its boss_scripts.json "map"
    (Monk, Butterfly, Apes, Dragons, and 54000000 / 71000000 / 71101000 on Isshin's field m11_02).
    set_fight(chr, row) at startup and on SpawnEncounter; a swap's line-up waits one frame for
    map.rs switch_map.
