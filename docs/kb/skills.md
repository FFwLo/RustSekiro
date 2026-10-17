# skills: combat arts, prosthetic tools, latent skills (plan and progress)

Moved out of `docs/HANDOFF.md` section 8 on 2026-10-15. Status at the end of the section.

## 8. Skills plan (2026-10-10)
Order: switching first, then combat arts, then prosthetic tools one by one, then latent skills. Ship
each step in game, keep everything data-driven (cite rows / HKS lines /
exe addresses, mark the rest `// gap:`), ask for a real-game clip per tool for the look.

**Done so far (extractor only, exported 2026-10-10, game not yet using it):**
- `export.rs`: every prosthetic state (cmsg offsetType 14: 183 states, W_GroundSubAttack*,
  W_SubAttackJump*, W_*SubAttackGuard*, ...) and every anim of the ten tool groups a070..a079 (328).
- `player.prosthetics`: all 40 tool levels (EquipParamWeapon 70000-79200): id, group
  (`wepmotionCategory` 70..79), variation (`behaviorVariationId`), `emblems` (`resourceItemA`),
  resident SpEffect, `icon` (iconId), `model` (equipModelId), attackBasePhysics / Fire.
- `player.bullets` "v<variation>:<judge>" (BehaviorParam_PC 100000000 + var*1000 + judge; TAE 2 in
  a07g anims; the old bare-judge keys stay for the Shuriken LV1, var 7000), `player.bulletRows` by
  Bullet id with the HitBulletID / intervalCreateBulletId chains (firecracker sparks etc.),
  BULLET_FIELDS now every field a 70xxxx-79xxxx bullet sets (numShoot, shootAngle*, intervalCreate*,
  isPenetrate, spEffectId0-4, spEffectIDForShooter, homing, sfx ids...).
- `player.attacks` "v<variation>:<judge>" for tool melee (TAE 1 in a07g: Axe, Sabimaru, Spear).
- Their SpEffects (bullet / AtkParam spEffectId0-4, shooter, residents) are in `player.spEffects`.
- Example (Firecracker): a071_401000 TAE 2 judges 100 + 110 at f36 on dummy 603 -> Bullet 710000
  (numShoot 8 charged / 3 for 710050, shootAngle -45 step 45, life 0.15, intervalCreateBulletId
  710001 every 0.1 s) and 710010 = the "vs. special-attack characters" twin (beasts) -> 710011.

**Done in the game (2026-10-10, 107 tests):**
- Names: `combat_data.names.weapon` from msg/engus item 武器名.fmg (new `tools/sekiro-extract/src/fmg.rs`);
  `Combat::weapon_name`. All 89 SkillParam rows + their SpEffects exported (page, virtualWeaponId name,
  acquireWeaponId art / tool, spEffect1-3).
- Switching: config `[player] prosthetics` (default [70000]), Z = Action::SwitchTool -> additive
  AddSubWeaponChange (a000_412090), then SubWeaponExpand in the tool's group (`tool_anim`; a071..a079
  body clips fall back to a070's: only a070 ships 412xxx clips). F1 menu SKILLS: combat art (19, by
  name) and 3 tool slots (every level). HUD bottom-left: tool and art names.
- Prosthetic core: equipped tool's anims / bullets "v<var>:<judge>" / emblem cost (resourceItemA, once
  per use) / base damage (attackBasePhysics + Fire x fireDamageCutRate). Release rule from HKS 6227:
  ref 302 window + forced by the LV1 resident (ref 314) - so LV1 tools never charge (as in the game).
- Bullets (`prosthetic.rs` spawn_row / fly_bullets): numShoot fans, shootAngleXZ, intervalCreate and
  HitBulletID children, floor stop, area bursts, shared hit lists, bullet SpEffects.
- Firecracker done: burst 710003 -> c9997 GetSpDamage SP_DAMAGE_BURST (refs 1000055/56/57: beasts by
  resident 230100) -> AssassinationBloodReaction (a000_020110); cool time 107100 (stateInfo 976, 30 s)
  read as "no new reaction" (Enemy.burst_until). Test `firecracker_staggers_the_general_once_per_cool_time`.
- Still to do in this list (as of 2026-10-10; all done since, see the end of section 8): steps 2 (arts audit), 4 (tools 072-079), 5 (latent skills), HUD icons.

**Done in the game (2026-10-10, latest; 131 tests):**
- HKS port `prosthetic::hks` (press 3523 / release 3750 / validates 6220-6227 / state ends 536-570):
  every tool's start state per style, combos (ref 300 + same tool), releases, Flame Vent hold
  (HoldStart -> HoldLoop while held -> HoldEnd), Umbrella guard (GuardStart / Loop / End and its own
  guard reactions 1371-1386, additive 676-690), Mist Raven atemi (1152: a hit in ref 305 becomes
  W_SubAttackJumpReady -> stick-direction leap), Divine Abduction SpecialEffect (ref 309), the empty
  clack (W_SubAttackFailed = a070_400900, CMSG animId 400900 offsetType 11).
- Tool levels: TAE events gated by StateInfo fire only for the equipped level's resident (905-946;
  `CharData::fires`). Tool melee resolves "v<var>:<judge>" and deals the tool's own attackBase*.
- Spirit Emblems: paid by every behaviour whose BehaviorParam_PC row has wepCost 1 when it fires
  (TAE 1 / 2 / 940; `CharData::behavior`; exported `player.behaviors`), not once per press. Judge 999
  rows are "form consumption dummy" (Umbrella f0, Flame Vent HoldStart f10 and HoldLoop f110).
- TAE 940 BehaviorParam_AddSpEffect: pays wepCost and puts the refType-2 SpEffect on Wolf for its
  effectEndurance (`Player.timed`, read as env(3036)): Divine Abduction 107700100 -> 107700 ref 309, 3 s.
- gap: ref 321 SP_EF_REF_TAE_TRANSITION_SUB_ATTACK_HOLD (only SpEffect 100260) is put on by no TAE,
  BehaviorParam, SpEffect chain or exe constant; the Flame Vent's hold uses refs 300 + 329 (100290
  "only the flamethrower cancels") as the window instead.
- Attack-button follow-ups out of a tool (BEH_A_GROUND_ATTACK 3199-3324, `hks::derive_attack`):
  Fang and Blade (070/071/078), the Axe / Mist Raven / Spear Combo2 derives, the Sabimaru's directed
  cut (V/F/B/L/R by stick; LV1 has ref 315 = none), Umbrella / Whistle / Flame Vent / Abduction.
  gap: env(3033) ACTION_UNLOCK_* (the Prosthetic Arts skills, mapped inside the exe) taken as learned.
- Burn / poison (`src/status.rs`): build-up SpEffects on the hit (AtkParam / Bullet spEffectId0-4;
  Paramdex SDT: registBlood + stateInfo 6 = burn, poizonAttackPower + stateInfo 2 = poison) fill a
  gauge to NpcParam resist_blood / resist_poison (x (1 - *GuardResist %) when guarded); full, the
  effect runs: changeHpRate % of max HP + changeHpPoint every motionInterval for effectEndurance,
  then replaceSpEffectId (Sabimaru 9004 -> 9045). General: Flame Vent LV1 125 / 200 per use,
  Sabimaru 31 / 150 per cut. The hit that breaks the burn gauge -> W_FireReaction, guarded or not
  (c9997 SP_DAMAGE_BURNING = env(3041) GetStatusActivation; ExecDebuffReaction runs before
  ExecGuardBlock; rank 3). Later hits while burning don't flail. Damage transition ranks (done 2026-10-10,
  prosthetic.rs `transition_rank` / `rank_allows`, c9997 IsEnabledTransitionRank and each state's
  DamageCommonFunction rank): the burn reaction (rank 3) does not cut large blows / posture
  breaks (ranks 0-2), the Firecracker stagger (rank 4) not those nor burning / lightning (3). Looks: SpEffect vfxId ->
  SpEffectVfxParam init / midst sfx (burn 3012 on dummy 850 + kanji 3013 on 280; poison 4011 /
  4013), added to `tools/fxr_extract.py`. Exe model (2026-10-11, SprjChrResistModule vtable
  0x142a8d178): gauges drain at TentativePlayerParam ResistRecoverPoint_*_Enemy / _Player (5 / s
  poison and burn; FUN_140bda460, whole points, fraction carried); build-up is blocked only while
  an active SpEffect has disablePoison / disableBlood (FUN_140bff7a0: poison 9045 yes, the burns
  9100-9125 no), amounts are whole points. Tests `gauges_drain_in_whole_points`,
  `resist_rules_from_the_params`. gap: only our tracked afflictions are checked for the flags.
- Boss music (2026-10-11, fmod.rs `boss_music`): the arena's EMEVD starts it through common_func
  20005830 (one track) / 20005831 (two: switch at the phase flag, 2010[8] 257), the tracks being
  MSB Sound regions (SoundType 6) whose SoundID is the FMOD event m<id> of sm<area>.fev.
  tools/boss_bgm.py -> extracted/enemies/boss_bgm.json links each init to the boss whose
  boss_scripts events set its flags (17 rows); the phase flags come from those events at
  runtime (BossScript `flags_on`). The music FSBs are FMOD-encrypted: tools/bgm_decrypt.py finds
  the key in the player's sekiro.exe and writes plain banks (extract.ps1 step, Bosses). Plays at
  the camera (only "(distance)" 0-10 m), during Playing / Paused until the defeat; the first
  track starts with the fight (the game waits for the intro's end flag). Tests
  `boss_music_follows_the_phase_flags`, ignored `fmod_boss_bgm`. Gaps: ordinary battle music
  (common 410), the crossfade time, the stealth "about to be found" tracks.
- Wolf's side (2026-10-11): enemy melee (combat.rs resolve, Hit / Block / Deflect) and bullets
  (enemy_bullet.rs: build-up SpEffects to Statuses, the rest stay timed on Wolf) fill Wolf's
  gauges via `Statuses::hit_wolf`. His resist max is 200 for every type (exe PlayerIns vtable
  +0x1d0 -> FUN_1408457f0: CalcCorrectGraph 110-114 of his stat sum, flat 200, + 120-124, 0).
  The guard keeps his sword's EquipParamWeapon poison/bloodGuardResist % off, a deflect
  *JustGuardResist % (burn 33 / 50, poison 0). HUD: hud.rs WolfBuildUp, the game's
  MENU_Resist_Poison / _Fire sprites over the vitality bar, filled from the bottom, whole and
  pulsing while afflicted. Test `the_bandits_torch_sets_wolf_on_fire`. Gaps: protector /
  SpEffect resist additions; whether the guard cuts a bullet's own SpEffects; the HUD place,
  size and animation (FE movie ActionScript, not decoded); c1300's Aging (9600, resist 999,
  likely cut) is not done.
- Check tool: `SHINOBI_SKILL_EVERY=<frames>` repeats the press in `SHINOBI_SKILL` shots.
- Combat art costs: the art's wepCost behaviours pay its EquipParamWeapon resourceItemA
  (`player::art_cost`: Dragon Flash 5400 2, Ashina Cross 5500 2, Mortal Draw 5700 3). Too few emblems ->
  the `*NoResource` states (offsetType 13, exported as a050_<id>) and their releases. The art's "has
  emblems" StateInfo (`art_emblem_gate`) comes from the event scripts, not the exe (`src/emevd.rs`
  reads extracted/event/common.emevd): event 9900 copies goods 1000 + 1001 into event value 9910
  every frame; 9930-9934 (started by event 0 with the cost) Set SpEffect the gate while Wolf has
  the resident and 9910 >= cost, Clear it after: 140300 -> 140310 (995, 2), 140501 -> 140510 (994,
  1), 140600 / 140601 -> 140610 (993, 3), 100286 (art 6100) -> 140410 (990, 3), 140900 / 140901 ->
  140910 (2). Ashina Cross (140400) has none (the old id guess gave it 140410). Tests `dragon_flash_costs_two_emblems`, `every_art_starts_and_costs_its_emblems`
  (all 19 arts: opening state, cost, back to idle).
- Jump arts 107 / 110 (Senpou Leaping Kicks, High Monk, Sakura Dance; HKS 3437-3453):
  GroundSpecialAttackJumpReady (316700) -> JumpStart (316710, TAE 920 launches) -> JumpFallLoop
  (316730); lands into LandGroundSpecialAttackJumpStart (same time) while ref 201 is up, else
  LandGroundSpecialAttackJumpFallLoop (316740). Out of a sprint SprintSpecialAttackJumpReady
  (316770: CMSG has no animId for it, `art_state_id`). Pressed
  again in the landing -> Combo2 (HKS 5386). The leap's 920 row 10010 (6.4 m/s, 9.64 up) homes:
  homingTargetType 1 (taken as the lock target; gap: enum unnamed) -> ChrPhysicsHomingParam 20000
  (initialVelocityCorrection 2, chase 10 m/s^2, rate 1, offset 1 m in front of the target).
  Correction 2 = exe FUN_140bb3580: keep the vertical speed, turn and slow the horizontal one to
  land on the point (never faster than the row's; too far -> the row's leap); chase =
  FUN_140bb3220 (player.rs `LeapHoming`). The skills recording (no lock): ~5-6 m at ~6.4 m/s.
  Before, a close locked enemy was jumped over. Test `leaping_kicks_land_in_front_of_the_lock_target`. One Mind's hold action -> VariationCombo2 / NoResource
  (5379), HoldEnd -> Combo1 (5406). TAE 920: every event fires once as the clock crosses it
  (316710 has two). Test `arts_combo_out_of_their_jump_and_hold`.
- Shadowrush / Shadowfall (109): the thrust connecting while ref 225 is up (a109_316000 f44-55) with
  the emblems -> GroundSpecialAttackHitJump (a109_316600; judge 215 = 105010215 wepCost pays the 2
  emblems), so a miss is free. Shadowfall (resident 140901 = unlock ref 286): attack while ref 226
  (f24-39) -> GroundSpecialAttackHitJumpDeriveAction (316650), landing into
  LandGroundSpecialAttackHitJumpDeriveAction (316660, same time) while ref 201 is up (HKS 5750,
  2893 / 2928, 2008). State keys a050_*, clips a109_* (other session's export). The emblems are
  checked at the press (`Player.art_enable_jump` = g_enableSpAttaclkJump). Test `shadowrush_jumps_off_the_hit`.
- Arts in the air (BEH_A_AIR_SP_ATTACK 2906-2940, `air_art_state`; landings BEH_R_LAND 1971-2015,
  `air_art_land`; clips a1xx_3162xx): 101 / 107 / 108 AirSpecialAttackStart -> Loop ->
  LandAirSpecialAttackStart (same time, ref 201) / ...Loop; 104 AirSpecialAttackHoldStart -> Loop,
  let go -> AirSpacialAttackHoldEnd (+ Land* versions); 109 with Shadowfall in the hit jump's
  ref 226 -> the derive; 110 Sakura Dance AirSpecialAttackStart, landing while ref 288 is up
  (f0-37) -> AirSpecialAttackLandingJumpReady (316210) -> LandingJumpStart (316260, bounces) ->
  LandGroundSpecialAttackJumpAfterJumpStart (316270), once per jump
  (`Player.air_art_count` = g_airSpecialAttackCount, also counted by the ground leap); the rest
  AirSpecialAttack -> LandAirSpecialAttack. Ends in the air -> FreeFall. Without the emblems the
  *NoResource clips: only 103 (5400) and 110 (Sakura Dance) ship them (a103 / a110 316201 / 316211,
  a110 316261 / 316271 / 316711 / 316721: TAE entries with their own events on the art's clip,
  ImportsHKX 316201 -> 316200). They were missing from tools/sekiro-extract/export_states.txt
  (the export only maps offsetType 13 to the sword's a050); added and re-exported 2026-10-10
  (combat_data diff: those 8 anims + attacks a110:205-207, nothing else). The other arts have
  none. gap: ACTION_UNLOCK_TYPE_AIR_SP_ATTACK read as learned. Tests `arts_in_the_air`,
  `every_art_starts_and_costs_its_emblems` (Sakura Dance's no-emblem leap).
- Tool StateInfo gates: `sound::active_state_infos` (used by ffx.rs, sounds and
  `resident_gates`) now adds the equipped tool's Prosthetic.resident (the tool rows are not in
  params.EquipParamWeapon): the Flame Vent's 127200 "Ignition LV1" -> 915 turns on its flame FFX
  300161 / 300164 and bullet judge 297, Abduction's 127700 -> 934 its wind 300196.
- TAE 922 ChrPhysicsVelosityScale (exe FUN_140b537a0 -> FUN_140bb1980, `player::VelScale`):
  over the event the air velocity blends from its start value to the row's scaled one (0.5 / 0.5)
  along CSEasingValue curves; the u8 args are horizontal curve / exponent, vertical curve /
  exponent (0 Linear, 1 EaseIn x^n, 2 EaseOut, 3 EaseInOut; every event: 3 / 3 = smooth cubic).
  Sakura Dance's hops, the Mist Raven 7401. gap: where the exe applies the row's scale to the
  blend's target is not traced.
- Latent skills (SkillParam spEffect1-3 -> SpEffectParam, `combat::skill_rate`): Mikiri Counter
  posture (70: 150400 attackHitParryStaminaAttackRate 1.25), deflect posture (270: 150410
  defStaminaAttackRate 1.25), Knowledge of Medicine (170/171/600-602: each adds 150210 accumuVal 1;
  the stack climbs 150200's accumuOverFireId chain 150201-150204 -> gourd heal x1.1 .. x1.5,
  `player::medicine_rate`). Test `latent_skill_rates`.
- Finger Whistle walks: its Moveable / Move states are STATE_TYPE_UPPER_ACTION_ATK (HKS 3731), so
  the legs keep walking under it (`is_upper_action`). Test `the_finger_whistle_plays_while_walking`.
- Mist Raven leap (HKS 536-561): the stick leaps (404011-404017) slide 5 m along the floor and land
  (W_LandAirSubAttackMove); the no-stick one (V 404010, SetNoGravity f0-13, root +3 m) ends in the
  air -> AirSubAttackMoveStartToLoop (920 row 7400: +0.6 m/s) -> AirSubAttackMoveLoop on a long drop
  -> the plain LandFreeFall. Attack in the fall while ref 301 is up (419030 f5-20, all of 419031)
  -> AirSubAttackDeriveAttack (413000) -> its loop (413100); lands into LandAirSubAttackDeriveAttack
  (same time) / ...Loop while ref 201 is up, else LandFreeFall (HKS 2890, 1903-1907). TAE 922 (7401) halves
  the rise (see below). Test `the_mist_raven_leap_rises_and_falls`.
- Tools in the air (BEH_A_AIR_SUB_ATTACK 3007, `hks::air_press`): Shuriken AirSubAttackCombo1-3,
  Firecracker AirSubAttackStart -> Loop, Mist Raven AirSubAttackMoveAtemiReady (a hit in ref 305 ->
  AirSubAttackMoveReady -> AirSubAttackMoveStart_<dir>, line 552 / 1095), Umbrella AirSubAttackGuard*
  (its air reactions: DeflectEasySmall / Deflect{Hard,Easy}Add, HKS 657-664 / 1276), Abduction
  SpecialEffect, Whistle LockOn, Spear Variation, the rest Combo1; no emblems -> SubAttackFailedAir
  (a070_403900). One use per jump for 071/072/074/075/077/078/079 (AIR_SUB_ATTACK_COUNT_MAX 1,
  `Player.air_sub_count`, 0 on the ground = _LandReset). Landings `hks::air_land` (HKS 1888-1970).
  The fall loops (FreeFall, *GroundJumpFall) are STATE_TYPE_STANDBY and now take every air input
  (they carry no cancel flags). gap: W_LandAirSubAttackStart is not in the state table (uses
  LandAirSubAttackLoop); an air move ending mid-air goes to FreeFall (graph end not in scripts).
  Test `tools_work_in_the_air`.
- Enemy fire reaction (c9997 GetSpDamage: SP_DAMAGE_FIRE / FIRE_FEAR need ref 1000040 = SpEffect
  6020 "play the flame reaction anim"): 6020 is only on some NpcParam rows (spEffectID9; c1020:
  10209800), not the regular General 10203010, so he has no fire flinch - correct as is. Burning
  (SP_DAMAGE_BURNING) does not need it and works.
- Sprint / crouch tool use checked against HKS 3523 (only 070/071/076/079 have crouch versions;
  the whistle has no sprint branch). Test `tools_from_a_sprint_and_a_crouch`. The crouched whistle
  (CROUCH_SUB_ATTACK_*_MOVEABLE / _MOVE, STATE_TYPE_UPPER_ACTION_ATK) keeps crouch-walking under it.
  Test `the_finger_whistle_plays_while_crouch_walking`.
- Deathblow recovery: the kill frame of a20x_510xxx adds one-shot SpEffects (motionInterval 999):
  105050 posture 34 % for everyone; 150301 / 150311 HP +10 % and 150321 / 150331 posture 34 %, each
  gated by invocationConditionsStateChange1 = its skill's permit stateInfo (skills 80 / 604 -> 986 /
  987, 265 / 605 -> 984 / 985). The same code does the resurrection's 110015. changeStaminaRate > 0
  read as recovery (from the rows' names 忍殺時体幹回復). After a Todome they are blocked
  (105051 / 150302-150332 win their spCategory, section 8 next steps 1). Test `deathblows_give_back_posture_and_hp_with_the_skills`.
- Covert A / B (60: 150000 sight cut 20 + around 0.5; 61: 150010 hearing 0.5): `combat::skill_sp_effect_rows`,
  fed into the enemy sight / hearing by enemy.rs think() (other session).
- HUD icons (hud.rs `ItemIcon`): bottom right, the art (EquipParamWeapon iconId) and the equipped
  tool (Prosthetic.icon) as hud/MENU_ItemIcon_<iconId:05>.png (export_hud cuts them from SB_Icon*).
- Not applicable yet: buff duration (370: 150600 extendLifeRate 1.5) stretches only
  isExtendSpEffectLife effects (the sugars 3401-3441 etc.; no such item in the game); luck (365 /
  366: drops / sen; no economy).
- Note: the SHINOBI_SKILL screenshot run segfaults at its AppExit (after the shots are saved).

**Next steps (items 1-5 of the old list are done: switching, HUD icons, all 19 arts, every tool's
HKS branch, the latent skills that have a system):**
1. Gaps listed above (`// gap:` in player.rs / prosthetic.rs). Done: the arts' gate StateInfos
   (common.emevd, above), the air *NoResource clips and "recovery prohibited" (below), env(3035)
   = Spirit Emblems held >= the weapon's resourceItemA (exe FUN_140a26010 -> FUN_14084dd10), the
   Mist Raven leap sectors (_SetJumpDirection's PRM_GROUND_JUMP_* ranges).
   Skill unlocks (done 2026-10-10): env(3033, ACTION_UNLOCK_TYPE_n) is the exe's FUN_1407a7fe0
   (env case 0xbd9 in 140b3): yes when an owned item's ActionUnlockParam row has action<n>; a
   skill counts through its virtual weapon (SkillParam.virtualWeaponId -> EquipParamWeapon.
   actionUnlockParamId, e.g. skill 6 -> 200500 -> action25 Mid-Air Combat Arts). `combat::
   action_unlocked` gates the mid-air deflect (15), air tools (16), air arts (25), the sprint
   slide (26) and the Prosthetic Arts follow-ups (20-23, `hks::derive_unlock`). Config
   `player.unlock_all_moves` (default true) keeps them all open; false = only the `skills`
   list's. ActionUnlockParam + the virtual EquipParamWeapon rows are exported (combat_data.json
   params); test `skills_unlock_their_moves`. The Firecracker's air
   landing is the game's misspelt state LandAirSubAttacStart (c0000.hkx CMSG; 403050).
   "Recovery prohibited" is not event scripts: Wolf's Todome anims a000_710205-7 add 105051 /
   150302-150332 (TAE 401, endurance -1, kept in `Player.held`, cleared by the fight reset),
   same spCategory as each recovery at categoryPriority 2 vs 3 (lower wins; categoryPriority
   exported 2026-10-10), so the deathblow recoveries do nothing after a Todome. Test
   `todome_blocks_the_deathblow_recoveries`. They end with Wolf's reload: effectEndurance -1,
   saveCategory -1 (not saved), no event script names them. Event 9950-9961 in
   common.emevd is the skill reset (clears every skill SpEffect), unrelated.
2. **Visuals:** the game's FXR effects now play (section 9). All 19 arts and 10 tools were
   shot (`SHINOBI_SKILL=<id>`; the log now prints Wolf's state, place and facing) and fixed where
   wrong (camera-distance fade, tracer subdivision: section 9). Sounds come from the FMOD banks.
3. **Tool models (done, check the rest):** the game's are /parts/wp_a_0700..0791.partsbnd (26):
   WP_A_07xx (Model0: .flver, .tpf, an anibnd with a999 only) and WP_A_07xx_1 (Model1) whose anibnd
   names its clips after Wolf's anims plus `_1` (umbrella: a076_405010_1 GuardStart, 405030_1
   GuardEnd, 412000_1 expand ...), so the tool plays the clip of Wolf's current anim. Export:
   `MSYS_NO_PATHCONV=1 python tools/tool_export.py [--exe ...]` (unpacks the parts, runs the
   extractor's `model` / `dummies` / new `anims <anibnd.d> <out.bin> [suffix]` per part, writes
   extracted/tool/tools.json: EquipParamWeapon 70000-79999 -> equipModelId, absorpParamId,
   WepAbsorpPosParam left_0..5 / leftHang_0..5). It sets SEKIRO_FLVER_REF_POSE=1: rigid weapon
   meshes (FLVER mesh Dynamic 0) keep their vertices in their bone's space and, without a
   BoneIndices member, name the bone in the normal's 4th value (NormalW; DSAnimStudio
   FlverSubmeshRenderer, SoulsFormats Vertex), so flver.rs binds by NormalW and moves them into
   model space by the bone's bind pose (before this the umbrella was one lump on one bone). Off
   by default, so the map and character exports are unchanged (gap: whether those need it too).
   Runtime `src/model/tool.rs` (child of model.rs): the equipped tool's Model0 rests on
   WepAbsorpPosParam left_0 (arm dummies 112..124), Model1 shows only while a Left Weapon TAE 715
   places it (umbrella on 118 / 21), each posed by its clip for Wolf's current anim (else
   a999_000000). SHINOBI_TOOL_LOG=1 logs the parts, places and mesh boxes. Shot: the Umbrella
   opens in front of Wolf, folds and is carried; shuriken, firecracker, flame vent, axe, Mist
   Raven, Sabimaru show their parts. Sabimaru (750) and Divine Abduction (770; 77000 is the fan,
   78000 the spear) have a third part WP_A_07xx_2 (Model2, placed by the 715's
   Model2DummyPolyID: 117 / 20), exported and shown too. The parts' own TAEs hold no events.
   Mist Raven's feathers (Model1) show only in the anims after it triggers (a074_404000 /
   404008 / 419000 place them on 21; the ready stance a074_404040 does not), as in the data.
   The parts' dummy polys join Wolf's Dummies as 2<model><dummy> (the TAE's left-weapon ids:
   21300 / 21301 the axe's / spear's blade, 21130 Sabimaru, 21140 the umbrella), so those FXRs
   now spawn on the tool (checked: Lazulite Axe 73300's 300157 on 21300; the base axe 73000 has
   state 918, so its 919 / 920-gated blade effects stay off, as in the data).
   SpawnFFX_Blade (118) now takes its DummyPolyBladeBaseID (Wolf's are all the axe's / spear's
   21300, tip -1), not the sword's 10301. Finger whistle shot: hand to the mouth, its sound rings around him.
   All 10 tools checked. Nothing left here: Sekiro's EquipParamWeapon has no wepInvisibleType
   (only invisibleOnRemo, cutscenes) and the part bundles hold no cloth. The skill-shot camera stands in front of Wolf (he faces
   it at idle too), so "Wolf faces backward" in those shots is the camera, not the anim.
   The `bevy_render::slab_allocator: Use-after-free` error at start shows with the tools off as
   well (not from tool.rs; not traced).
4. **Sparks when an enemy deflects / guards Wolf (done):** Wolf's AtkParam_Pc rows hold
   defSfxMaterial1 / defSeMaterial1 = 255 (281 rows) or 0 (161), slot 2 = 139. Traced: the exe's
   FUN_140ba8a50 (SFX) / FUN_140ba8840 (SE) replace 255 by the attacker's weapon's EquipParamWeapon
   defSfxMaterial1/2 / defSeMaterial1/2 (100 without a weapon row; a chr without weapons takes its
   own default). No AtkParam_Npc row holds 255, and every Wolf weapon row is 101 / 139:
   vfx::guard_materials (used by vfx::hit_sfx and sound::guard_sounds) maps 255 to the sword's
   (row 5000). Test: clash_effects_follow_the_hit_effect_tables. 0 stays 0: no concept table has a
   row 0, so those attacks (161 rows, e.g. 7600210) make no guard spark.
5. Private test builds: `CARGO_TARGET_DIR=target/fx` (peers rebuild target/release). Unpacking needs
   `MSYS_NO_PATHCONV=1` in Git Bash or the '^/parts/..' regex is mangled.

Verify with sim tests (`src/sim_tests.rs`) per tool and art, and screenshots via the throw-trace
shot env vars or `tools/screenshot.ps1`.

- 2026-10-17 Lightning Reversal (weak, a050_308910 / landing 308911): AttackBehavior 280 / 281 at
  0.133-0.267 s -> BehaviorParam_PC 105000281 -> AtkParam_Pc 5000281: spAttribute 6, spEffectId0
  9400 (the 508 HP shock), capsule dmy 10120 -> 10141 radius 1.2 (dmy 141 on WP_A_0300 Blade02),
  no atkPhys / atkStam. The hit filter (combat.rs ~1340) dropped attacks with no damage and no
  stamina, so it did 80 HP and no stun; an attack with any SpEffect now hits. Test
  the_reversal_bolt_stuns_tomoe (part06).
