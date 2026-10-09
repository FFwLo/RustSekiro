# Full log archive (pre-split PROGRESS.md, for grep only)

# Shinobi Combat: progress and next steps

Goal: a Rust/Bevy combat prototype that plays 1:1 like Sekiro, with numbers read
from the user's own Sekiro install (TAE, HKS, params) and Ghidra for the gaps.

## Pipeline (all local, nothing game-derived is committed)

```
tools/extract.ps1    # runs the three steps below
sekiro-extract unpack <Sekiro dir> extracted <regex>   # BHD5/BDT decrypt, DCX (zlib + Oodle via game DLL), BND4
sekiro-extract params extracted/param/gameparam/gameparam.parambnd.d extracted/json/params
sekiro-extract export extracted                         # -> extracted/combat_data.json (read by the game)
```
- HKS decompiled with DSLuaDecompiler (tools/DSLuaDecompiler, built from source, net10.0) -> extracted/hks_src.
- Export list: tools/sekiro-extract/export_states.txt (behavior state names / anim keys).
- Exe research: tools/ghidra.ps1 (DecompileRefs: fn/ref/callers/callees/range/orq) for targeted reads;
  tools/decompile_all.ps1 (DecompileAll, resumable) dumps every function to extracted/decomp/<addr>>16>.c
  (`// @ <entry>` before each). Search it with rg, or index with gamedb (tools/gamedb, github.com/smileybaal/gamedb):
  older targeted dumps live in extracted/decomp_notes. `gamedb index -r extracted/decomp`, then `gamedb graph -r extracted/decomp FUN_xxx --direction callers`.

## Established from data (implemented)
- TAE times are seconds on a 30 fps grid; clip playback speed is 1.0 for all 2566 player clips.
- Deflect window = SpEffect 105010 (behaviorRefId 203). Guard spam chain StandToDeflectGuard -> 2 -> 3 -> 4
  (a050_203000/203005/203006/203007): windows 6/3/2/0 frames; SpEffects 105020-105022 set
  defStaminaAttackRate 1 / 0.5 / 0.25. Chain allowed while ref 212 is up (frames 0-18).
- Attacks: press -> GroundAttackCombo1 (a050_300000); release while ref 208 (f6-15) -> Combo1Release
  (a050_300100, hit f7-11); hold -> charged thrust (hit f23-25). Next combo routed by refs 214-218.
- Input windows = ChrActionFlag cancel types: 115 attack, 117 guard, 26/25 step, 119 jump, 11 move.
- Movement speeds from root motion: walk 1.62 m/s, run 5.29, sprint 8.77; step 3.99 m over 40 frames.
- Posture regen ratio per state from TAE event 960 + StaminaControlParam (attacking/stepping 0%, guard idle 200%).
- Enemy: Ashina Samurai General (c1020), NpcParam 10219000 (HP 1918, posture 600, regen 60/s).

## Gaps (each also marked "gap" in code/config) - current
1. (done) Hurtboxes = ragdoll capsules from chrbnd HKX.
2. (done) Input buffer: accept flags gate presses, execute flags fire them, FireEvent resets (see log 2026-10-07).
3. (done) Player HP / posture graphs 500/501 confirmed (FUN_140a368f0); level input = PlayerGameData stat, 1 here.
4. (row done: armour knockbackParamId 1; kick reach done: the kick is a real attack) Player body radius (0.4 m).
5. (done) Posture after an unused deathblow window: no restore, regen runs through the collapse (log 2026-10-07).
6. Floor/armour sounds (x/b types: engine material mapping), clash SE material table, encrypted smain.fsb.
7. Grabbed-player anim a210_600000 (TAE only, no clip in the archives).
8. (done) Behaviour-graph transitions are 0 s and locomotion is CMSG clip selection: TAE Blend drives every crossfade.
9. (done) Stick-vs-launch velocity mixing in the air (FUN_140bac120, log 2026-10-07).

## Ghidra notes
Ghidra 12.1.4 at C:\Users\User\Desktop\ghidra_12.1.4_PUBLIC. sekiro.exe is Arxan-protected; work from a
dump of the running process (user-run) plus community symbol names (souls modding wiki / Discord).

## Log
- 2026-10-06: extractor, HKS decompile, statemap, export, Bevy game (player FSM, enemy, combat, HUD) built.
- 2026-10-06: game runs; 4 data-rule tests pass (cargo test). Enemy starts passive (T toggles). Auto-resume schedule was blocked by permissions; user must allow it.
- 2026-10-06: tools/sekiro-dearxan (uses tremwil/dearxan, cloned to tools/dearxan): SteamStub 3.1 unwrap works
  (app 814380, .text AES-decrypted, OEP 0x14235a28c restored) -> extracted/sekiro_dearxan.exe (memory layout,
  loads as a normal PE). dearxan found 0 Arxan stubs: Sekiro has no `test rsp,0Fh` signature, so its Arxan
  variant is not one dearxan supports (tested games: DSR, DS2, DS3, ER, AC6, NR). Further Arxan analysis was
  blocked by the session's permission policy; waiting on the user before continuing.
- 2026-10-06: user supplied a Steamless-unpacked exe (Sekiro\sekiro.exe.unpacked.exe). All sections byte-identical
  to tools/sekiro-dearxan's SteamStub unwrap (confirms it). Ghidra project: extracted/ghidra (sekiro), decompiles
  to extracted/decomp/. Exe debug labels found: posture regen debug menu ("current stamina control type: none"
  -> default = 100%, now applied), GuardCut formula label at 0x142a2cb20, player damage = attackBasePhysics x
  atkPhysCorrection (applied; player_hp_damage gap removed).
- 2026-10-06: research: Sekiro has no Arxan (tremwil / me3 blog), so no deobfuscation is needed; Steamless + Ghidra is the
  route. Helpers: RTTI class names in the exe, fromsoftware-rs crates/sekiro, sekiro-coop SDK (AOBs/offsets), LukeYui's
  Sekiro-Debug-Patch (dev debug menu with live posture/stamina readouts, for verifying the prototype against the game).
- 2026-10-06 GHIDRA (Steamless exe, project extracted/ghidra, runner tools/ghidra.ps1): posture regen CONFIRMED.
  ChrIns::Update = FUN_140a04850 (vtable NS_SPRJ::ChrIns 0x142a22948, slot 0xb0):
    pts/s = SpEffectRecoverRate * (base + SpEffectChangeSpeed) * byte(+0x14)*0.01 * chr.f[0x10d0] * (ratio(type)*0.01 if type != -1)
    whole points added per frame, fraction carried (ChrDataModule +0x15c); stamina cur/max at ChrDataModule +0x148/+0x14c.
  base: enemy = NpcParam.staminaRecoverBaseVel (EnemyIns slot 0x218), player = CalcCorrectGraph 504 (FUN_140850b10).
  FUN_140844cb0(cur,max,x) returns x: no posture-ratio curve and no direct HP curve -> hp_scales_regen now off.
  chr.f[0x10d0] = 1.0, or 0.8 / 0.7 from a once-per-second level system in PlayerIns update (thresholds 0.7/0.3); input unidentified.
  CalcCorrectGraph evaluator implemented exactly (adjPt power curves).
- 2026-10-06 GHIDRA: posture DAMAGE confirmed. Attacker FUN_140842790: unguarded directAtkStamDamage_Attacker,
  guarded repelVictoryStamDamage_Attacker, deflected repelLostStamDamage_Attacker x defStaminaAttackRate of defender
  SpEffects with stateInfo 204 (FUN_140bfc490). Defender FUN_140847d50: direct / atkStam / repelLostStamDamage, final
  FUN_1408439a0 = base x hit mult x guard cut (vfunc 0x200) x attribute rate (0x248) x SpEffects. Hit record flags:
  +0x164 guarded, +0x1C7 deflected. The prototype's mapping matches; gaps 1 and 2 mostly closed.

- 2026-10-06 WOLF: i-frames from TAE 954 (IFrameType 2 General, 6 Thrusts, 7 Grabs, 1 Sweeps = judge 980)
  in combat.rs dodged(); mikiri (flag MikiriCounter vs perilous thrust) wins over thrust i-frames. Dodged hits are
  not consumed, so a step pressed too early is still hit by the live hitbox after the 0-9 frame window (as in game).
  Test timed_step_dodges_through_the_slash (step 1 frame before the window). 36 tests pass.

- 2026-10-06 GHIDRA (NEXT 11, partial): ActionRequest mask writers found. Accept (+0xd0) setter FUN_140b2be00(mod, bit, on),
  execute (+0xe0) setter FUN_140b2bdc0. Both are rebuilt every frame (zeroed at the end of FUN_140b2c190). Main writer:
  FUN_140b51b70 = a per-event switch on a u16 id (looks like the TAE ChrActionFlag handler, gated by an SpEffect check
  on arg[7]): case 1 -> accept bits 0,1,25,26 (attack); case 4 -> execute same bits; 0x10 -> execute 2 (guard);
  0x16 -> 22; 0x1a -> 5,13,14; 0x1d -> 16,17; 0x1f -> 7; 0x20 -> 6,9,10,27,12,15,19. Accept-only helpers:
  FUN_140b5b9a0 (attack), FUN_140b5b8e0 (2, +3 unless state 0x3fc0), FUN_140b5b7e0 (6,9,10,27,19), FUN_140b51970
  (a 7-byte bool struct, maybe HKS). NEXT: confirm the u16 is the TAE flag id (our player gates on 115/117/26/119,
  not 1/4), then accept window = accept-flag ranges, execute = cancel flags. Tool: DecompileRefs range:/orq: options.

- 2026-10-07 WOLF/GHIDRA: NEXT 11 done (found with the full decompile + gamedb in minutes). FUN_140b59170 is the TAE
  event dispatcher (case 0 ChrActionFlag -> FUN_140b51b70, 0xe0 SetTurnSpeed, 0x140 event 320 bool accept set).
  ChrActionFlag -> ActionRequest bits (HKS ACTION_ARM_*: 0 attack, 1 sub, 2 guard, 4 jump, 5/13/14 step, 7 item,
  18 shinobi tool). ACCEPT (+0xd0, press latched): 87 all, 1 attack, 9/150 guard, 25 step, 151 jump, 30 item,
  136 tool. EXECUTE (+0xe0, request fires): 4/115 attack, 116 sub, 16/117 guard, 26 step, 119 jump, 31 item, 137 tool.
  The flags we already gated on were the execute ones. A flag with StateInfo != 0 applies only with that SpEffect
  state (none in Wolf's accept/execute flags). Reset: HKS FireEvent -> ResetRequest -> act(9101) -> FUN_140b2afd0
  (+0x188 bit 0) drops every pending press; FireEventNoReset (locomotion, quick turns, falls, landings) keeps them.
  Implemented in player.rs (buffers / keeps_buffer). Effect: e.g. the tap slash buffers from frame 6 (executes 15),
  Combo1 hold from 21 (executes 33), guard/jump/step get early cancel windows (frames 3-15). Mash before the window
  is dropped. Test early_mash_is_dropped_but_a_press_in_the_accept_window_chains. 37 tests pass.

- 2026-10-07 WOLF: posture break flow from HKS (DAMAGE_TYPE_DAMAGEBREAK / GUARDBREAK, c0000_transition.lua ~1614).
  Guard emptied -> StandDeflectBreak (a050_190100). Hit emptied -> by damage level: EXLARGE BreakLargeBlowStart,
  EX_BLAST/BREATH BreakExLargeBlow (no clip -> LargeBlow), SMALL_BLOW BreakSmallBlow, UPPER BreakLargeUpper, FLING
  BreakLargePound, else StandDamageBreak_F/B (a000_19000x, 3 s). Any hit while in StandDamageBreak/StandDeflectBreak
  -> StandDamageBreakDamage (a000_191000) -> StandDamageBreakDown_FaceDown/Up (a000_19001x) ->
  StandDamageLargeDownWakeUp (graph transitions; face up only after a front small blow). 25 break states exported
  (names = behavior CMSG names minus _CMSG). Test posture_break_staggers_then_a_second_hit_knocks_wolf_down. 38 pass.

- 2026-10-07 FIX: TAE events with StateInfo != 0 only apply under an SpEffect of that state (exe FUN_140bfebf0 in
  the TAE handlers). Wolf had 163 such AttackBehaviors live: the weapon-enchantment extra hits (SpEffectParam
  stateInfo 152 poison blood, 153 red blood, 357 white blood, 358 orange wind, 940 gold wind) fired with every slash.
  Now gated in Event::active / attack_windows / sounds; also shuriken upgrades 907 and coins 996-999. Shuriken spawns
  from its event's DummyPoly (2, the hand) when the model is loaded.

- 2026-10-07 WOLF: player's KnockBackParam row found in data, not the exe: EquipParamProtector knockbackParamId = 1 on
  every outfit piece (38 rows; NPCs all use 0 via NpcParam). Row 1 holds pushes longer (damage ContTime 0.09 s vs
  0.01-0.05, guard_S 0.06/0.30). Protector knockBackCutRate_* and weapon knockBackCutRate_*_Guard are 0 for Wolf.
  (Exe side: KnockBackParam is param index 0x22 in the name table at 0x143b16380.)

- 2026-10-07 WOLF/GHIDRA: air control from the exe (FUN_140bac120, fall module; table entries = name ptr + h_accel
  +0x78, gravity +0x7c (+0x1c4 extra), stick accel +0x80, stick max +0x84). One horizontal velocity v: decays along
  its direction by h_accel*dt (skipped once slower than a step); stick a = accel*stick*dt split along dir(v) (or the
  stick when still): the across part always applies, the along part applies in full when braking, and when
  accelerating only up to stickMax - |v|. So a fast launch is steered and braked by the stick but never pushed
  faster; the separate capped stick velocity model is gone (Actor.air_stick removed).

- 2026-10-07 NEXT 15 done: swept hit test. combat.rs resolve keeps every dummy's world position from the previous tick
  (Local map) and tests the AtkParam capsule at SWEEP_STEPS = 4 interpolated poses between last tick and now, so a fast
  slash no longer passes through thin ragdoll capsules between ticks. (Idea from the Elden Ring rewrite's blade sweep.)

- 2026-10-07 FIX: models/textures/sounds missing when the release exe is started directly (not via cargo run): Bevy's
  AssetPlugin path "extracted" resolved against target/release. Now concat!(CARGO_MANIFEST_DIR, "/extracted"), same as
  every other loader. 0 "Path not found" errors; both models render.

- 2026-10-07 NEXT 16 done: attack lunge (TAE 760 BoostRootMotionToReachTarget), decoded from FUN_140842980's assembly
  (return in xmm0): scale = clamp(|arrive - self|, EnableRangeMin, EnableRangeMax) / ReferenceDist, arrive = target +
  rotY(ArriveAngle) * dir(self - target) * ArriveDist; RangeMin/Ref when already inside ArriveDist; 1.0 without the
  event or with ReferenceDist 0. Recomputed every frame (FUN_1407f0a70 -> chr+0x2fc). 20 player anims (held combo,
  step/sprint attacks, deflect counters, DeflectGuardAttack, Whirlwind) and 6 enemy ones. Implemented as
  Actor.root_scale (actor.rs boost_root_motion). Target: lock-on target (Wolf) / player (enemy). Gap: Wolf's fallback
  target point when not locked (chr+0x1080 in FUN_1407f0a70) not identified -> no boost unlocked. Test
  locked_slash_lunges_toward_a_far_target. 39 pass.

- 2026-10-07 NEXT 17 done: TAE 226 SetKnockbackPercent (dispatcher case 0xe2 -> FUN_140ba34a0: knockback module +0x40
  = u8 percent * 0.01) scales the knockback the anim's OWN character receives: FUN_140ba37e0 (knockback start) sets
  push = distance * (+0x40). Wolf: Combo1 0 % at frames 23-25 (not pushed when blocked at his hit frame), Combo2 50 %,
  Combo3-5 30 %. Applied in combat.rs apply_knockback. (Also seen: leftover push distance is handed to the other
  character when the knocked one is blocked, FUN_140ba34b0 - not modelled.)

- 2026-10-07 NEXT 19 done: tools/decomp_join_signatures.py joins Ghidra signatures split over two lines (return type
  alone, name below; 11545 of them) in extracted/decomp. gamedb now indexes 182,628 of 185,039 functions (was 178,981)
  and 500k call edges (was 459k). Run it after any new decompile_all.ps1 export, then `gamedb index`.

- 2026-10-07 NEXT 18 partial: `sekiro-extract clips <behavior.hkx>` (hkx::clip_generators) reads every hkbClipGenerator.
  All 2566 in c0000.hkx and all 388 in c9997.hkx are plain (playbackSpeed 1, no crop / start offset / enforced
  duration), so playing TAE clips at 1x from frame 0 is exact. Left: the 13 transition effects (where the state
  machine uses them) and the 2 locomotion hkbBlenderGenerators.

- 2026-10-07 NEXT 18: `sekiro-extract transitions <behavior.hkx>`: all 16 transition effects (StateToStateBlend,
  Duration0, TaeBlend*, DefaultTransition, MovementResetTransition, ThrowTransition) have duration 0, so crossfade
  time = the target anim's TAE Blend event and anims without one snap in. Was: 0.12 s fallback fade. Now: TAE anims
  without a start Blend (60 player states: deflected/stagger/air reactions, 82 enemy: damage, guard bound, death)
  snap; only procedural locomotion keeps 0.12 s (gap: hkbBlenderGenerator weights). Clips: see previous entry.

- 2026-10-07 lunge target, exe detail: chr+0x1070/+0x1080 = lock-on flag / lock target point (written by the lock
  selection FUN_1409c5fe0 via FUN_1409f6130). FUN_1407f0a70 first asks chr module slot 0x21 (FUN_140b2dab0): it returns
  that module's +0x38 target point only while NOT locked on = the unlocked attack auto-aim target. NEXT: identify module
  0x21 (RTTI of the object at modules[0x21]) and how it picks +0x38, then use it for unlocked lunges and attack facing.

- 2026-10-07 WOLF: attack auto-homing (unlocked). chr module slot 0x21 = CSChrAutoHomingModule (RTTI via vtable
  0x142a73540; built in FUN_140a002f0). HKS _StartAutoAim on attack transitions; _UpdateAutoAim calls act(156) for
  1/6 s while not locked -> FUN_140b2e090 -> FUN_140b30fa0 -> FUN_1409c58e0: nearest lock candidate (with line of
  sight) passing FUN_140b2ddc0: 3D distance from feet+1.5 m <= LockCamParam closeMaxRadius (3.0; _forD/_forPD in
  darkness), height within -closeMinHeight (3.0) .. +closeMaxHeight (1.0), angle to stick (or facing) <= closeAngRange
  (30 deg). Limits copied from the LockCamParam row by FUN_140b2e0e0 (param index 0x1f, offsets 0x20-0x44 incl. the
  bullet* auto-capture). FUN_1409efdb0 = attack target point: lock point (chr+0x1080) else homing target. Implemented:
  player.rs start_auto_aim / auto_homing_target, Actor.homing (turning during attacks + TAE 760 lunge). Gaps: darkness
  radii, line-of-sight ray.

- 2026-10-07 NEXT 20 done (light version, no Ghidra write): tools/rtti_vtables.py scans the exe's MSVC RTTI (12662 type
  descriptors -> 11055 complete object locators -> 11055 vtables) into extracted/rtti_vtables.txt and annotates the
  decompile (`&PTR_FUN_142a73540 /* CSChrAutoHomingModule */`, 45212 references). rg the class name to find a
  module's constructor, then its methods next to it. Re-run after decompile_all + decomp_join_signatures.

- 2026-10-07 WOLF: turning from the exe. TAE 224 SetTurnSpeed handler FUN_140b574b0 writes the action module +0x308
  (reset to -1 each frame, FUN at 140b2.c:9873); IsLockOnCheck events only apply while locked on (chr+0x1070) - now
  honoured. FUN_1407daa20 picks the turn rate: TAE +0x308 when >= 0, else module(0xb8)+0x16c when >= 0, else the turn
  controller default +0x28 = 720 deg/s (FUN_1407da540). Config turn_speed 720 is therefore exe-confirmed (gap closed).
  Posture after an unused deathblow: not in the collapse TAE (only SpEffects 220420/5359/220500) or HKS; env(1001) /
  env(2010) = posture cur/max at data module +0x148/+0x14c; FUN_140bd62d0 (ratio restore) is only reached from TAE
  event 961, which no exported anim uses. Still 50 % (gap 5/12).

- 2026-10-07 guard arc from the exe: FUN_140b6ab40 blocks only when facing . attack direction < cos((angle + 180) deg),
  angle = the ShieldBlock flag's ArgB via a vfunc (0 -> same as 90). ArgB 90 everywhere in Wolf's TAE -> the front half.
  combat.rs guard_half_angle reads it per anim; config combat.guard_angle removed (gap closed; old value matched).

- 2026-10-07 FIX: deflects had no facing check (a perfect press deflected attacks from behind). The exe's guard-arc
  test (FUN_140b6ab40) runs before the guard outcome, so the same arc now gates deflect and block on both sides
  (backstabs on a deflecting enemy land too). Test a_deflect_facing_away_is_still_hit. 40 pass.

- 2026-10-07 camera: CameraParam row 0 now exported. Pitch from it: rotRangeXMin..Max = -40..80 deg free (was config 65),
  rotRangeXAtLockMin..Max = -30..30 deg while locked on (camera.rs pitch_range; config camera.max_pitch removed).
  Still open (follow_smoothing gap): chase rates chrTransChaseRateXZ/Y_ForNormal 0.2/0.3 (+0xc/+0x10), lockRotChaseRate
  X/Y 0.6/0.3 (+0x110/+0x114), rotSpeed_Min/MaxX/Y (stick turn speeds); their per-frame vs per-second meaning needs
  the camera code: CameraParam is param index 0x66 (row getter FUN_140737d80), follow camera class ChrExFollowCam
  (vtable 0x1429a7d88), param wrapper ChrCamParamImp (0x1429a7228).

- 2026-10-07 cleanup: config enemy.walk_speed removed (unused; enemy locomotion already plays c1020's Walk*/Run*Battle
  clips with their root motion). [input] comment updated to the accept/execute model.

- 2026-10-07 gap 3 closed: FUN_140a368f0 (on PlayerGameData change) sets max HP = graph 500 (PGD +0x20), +0x2c = graph
  101, max posture = graph 501 (PGD +0x3c), then pushes them into the data module (FUN_140bd6410/20/30). Camera chase
  rates: the CameraParam row (0x1e0 bytes) is copied into the camera object at +8 (FUN_140736af0); consumer not found.

- 2026-10-07 kick-off-enemy research: HKS [BEH_R_ENEMY_JUMP] = env(2004) and SpEffect ref "TAE enable kick enemy jump"
  (AirKick a000_213100 has SpEffect 100334 "kicking enemy jump transition possible" on frames 9-21; no AttackBehavior).
  env(2004) = bit 4 of +0x58 on the module from FUN_140b360d0 (engine contact flag; setter not found yet). NEXT: gate
  our kick jump to that 9-21 window (data) and find the flag's setter for the reach test.

- 2026-10-07 WOLF: air kick per HKS. A jump press in the air now always plays AirKick (BEH_A_AIR_KICK; blocked by an
  SpEffect with behaviorRefId 108), and the enemy jump happens only while AirKick's SpEffect 100334 (behaviorRefId
  204, frames 9-21) is up and Wolf meets a body (stand-in for env 2004). Was: instant kick-jump on the press.
  player.rs sp_ref_active = HKS env(3036, ref). Test kicks on the descent. 40 pass.

- 2026-10-07 shuriken aim: lock target, else LockCamParam bullet auto-capture (bulletMaxRadius 30 m, bulletAngRange 15
  deg of the facing - the CSChrAutoHomingModule bullet limits), then bent toward it within Bullet lockShootLimitAng.
  Was: nearest enemy within lockShootLimitAng. Stopping here: 5h usage near the user's 50 % cap.
- 2026-10-07 10:09 scheduled run: checked in, no action (last log write 08:13 < 3 h ago; 5h window at 42 %).

- 2026-10-07 WOLF: the air kick is a real attack. HKS env(2004) = action module +0x58 bit 4, set by the hit processing
  FUN_1409e5fd0 when one of the character's attacks lands. AirKick's TAE 307 PCBehavior (judge 901, GetWeaponData 0,
  frames 9-21) -> BehaviorParam_PC 901 "Kick_in the air" -> AtkParam_Pc 901 (10 phys, 10 posture, push dmgLevel 5,
  capsule dummies 612-613 r 0.4). Exporter resolves 307 judges as attacks "pc<judge>" (also pc210/211/221/222 body
  contact pushes, 0 damage -> ignored). Kick-jump = kick connected (Actor.attack_hit) inside SpEffect 100334's window;
  the KICK_REACH stand-in is gone (gap closed).

- 2026-10-07 NEXT 12 done: no posture restore after a missed deathblow. Data-module layout confirmed (HP +0x130/34/38,
  graph-101 stat +0x13c/40/44, posture +0x148 cur /+0x14c max /+0x150 base; FUN_140bd6410/20/30 set the bases). The
  only ratio restore (FUN_140bd62d0) is TAE event 961, absent from all 364 c1020 TAE anims; the collapse SpEffects
  keep staminaRecoverSpeedRate 1.0; regen (FUN_140a04850) runs every frame. So the 50 % reset is gone: the General
  keeps what regenerated during the collapse (staminaRecoverBaseVel 60/s). Also NpcParam maxDebtStamina -30: posture
  may overshoot max by 30 (stamina debt) -> Actor.posture_debt; bars clamp at full.

- 2026-10-07 NEXT 18 done: `sekiro-extract blenders`: c0000.hkx has only 2 hkbBlenderGenerators (Master Blend over
  Master_SM, and AddHang Blend bound to HangWallAngle) - no locomotion blend tree. Locomotion is CMSG clip selection
  whose transitions are the 0 s TaeBlend effects, so walk/run/idle switches now blend by the shown clip's TAE Blend
  (walk/run 6 frames = 0.2 s, idle 9 = 0.3 s) instead of the 0.12 s fallback (anim.rs proc_clip / clip_key). Gap 8 closed.

- 2026-10-07 camera follow from the exe: CameraParam row access = FUN_140741e70 (global copy DAT_143d59938 + 8). The ChrCam
  update FUN_14073c260 eases working rates (+0x220/+0x224/+0x228) toward chrTransChaseRateXZ/Y_ForNormal (+0xc/+0x10) and
  targetChaseRateXZForNormal (+0x6c), then FUN_141155d20 moves the focus in fixed 1/30 s steps, each closing `rate` of
  the gap (sub-step interpolation of the target). camera.rs focus: blend = 1 - (1 - rate)^(dt*30), XZ 0.2 / Y 0.3.
  follow_smoothing now only drives the lock-on yaw chase (lockRotChaseRateY 0.3 is the likely source - not traced).

- 2026-10-07 lock-on camera chase from the exe: FUN_14073c260 turns the locked camera by diff * rate * (dt / (1/30)) per
  frame, yaw rate lockRotChaseRateY 0.3 (+0x114), pitch lockRotChaseRateX 0.6 (+0x110). Yaw applied; config
  camera.follow_smoothing removed - every camera value now comes from LockCamParam / CameraParam (camera gaps closed,
  except our lock camera not chasing pitch).

- 2026-10-07 WOLF: damage reactions re-checked against HKS normal damage (c0000_transition.lua ~1162). Fixed: NONE (0)
  and MINIMUM (8) play no reaction (were Small); PUSH (5) = Middle 4-way (was LargeBlow); EXLARGE (4) = LargeBlow (was
  ExLargeBlow first); EX_BLAST (10) = ExLargeBlow, BREATH (11) = SpecialLargeBlow (both no clip -> LargeBlow; were
  Large); FLING (6) = StandDamageLargePound (exported now, a000_100500). Knockdowns chain like the graph: BlowLand /
  UpperLand / LargePound / SmallBlow -> StandDamageLargeDown (get-up a000_10031x), prone per _setProneDir (small blow
  from the front lands face up).

- 2026-10-07 enemy damage reactions per c9997.lua ExecDamage*: PUSH -> DamagePushFront/Back, SMALL_BLOW / BREATH ->
  DamageBlowFront/Back, BLOW (4) / EX_BLAST -> DamageLargeBlow (4 s), LARGE / FLING / UPPER -> DamageLarge_<dir>,
  MINIMUM / NONE -> none (were folded into Small/Middle/Large). Player guard sizes re-checked: already match HKS
  (SMALL/MIDDLE/PUSH S, LARGE/SMALL_BLOW M, EXLARGE/FLING/UPPER L, MINIMUM XS, EX_BLAST/BREATH XL).

- 2026-10-07 WOLF: hits in the air use HKS air damage (c0000_transition.lua ~1106): SMALL/MIDDLE AirDamageSmall,
  LARGE/PUSH AirDamageLargeStart -> FallLoop -> Land, FLING AirDamageLargePound*, EXLARGE/SMALL_BLOW/EX_BLAST/UPPER/BREATH
  -> their air blow states (no clips in c0000_a0xx -> the AirDamageLarge chain); landings -> StandDamageLargeDown face
  down (graph transitions). 27 air states exported. Test hit_in_the_air_uses_the_air_reactions. 41 pass.

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

- 2026-10-07 lock-on pitch (FUN_14073c260 ~asm 0x14073e000): pitch limits = rotRangeXAtLock (-30..30) widening toward
  the free -40..80 as |height gap| goes rotRangeLerpBeginHeight 0.5 -> EndHeight 3.0 m; target pitch = elevation to the
  target + FOV * 0.5 * LockCamParam lockRotXShiftRatio (0.45 -> ~9.7 deg looking down), chased at lockRotChaseRateX 0.6
  per 1/30 s. Implemented in camera.rs (the exe's exact elevation term uses an asin of a sight-line ratio; atan2 of the
  height gap over the flat distance stands in).

- 2026-10-07 WOLF: resurrection. GroundDeathStart_F -> GroundDeathLoop_F, then LMB resurrects while a node is left
  (config player.resurrections = 1, the game's start): GroundRevival_F (a000_110030), whose TAE applies SpEffect
  110015 "Resurrection Technique_HP Half Recovery" (changeHpRate -50 -> +50 % max HP). No node: the old auto reset.
  Exporter now carries changeHpRate / changeHpPoint / changeHpEstusFlaskRate and SpEffect 3000 (gourd -40, was a
  fallback constant). Test resurrection_returns_wolf_at_half_hp. 42 pass. Also: TAE 155 SetLockParamID (8 / 11 / 41)
  indexes CameraSetParam lockParamIdN, which is 0 (LockCamParam row 0) for all three in the default set: no-op.

- 2026-10-07 invulnerability windows: TAE 950 (dispatcher 0x3b6 -> action module +0x70 |= 2) sits on all 55 player
  knock-down / launch / death / revival anims (frames 0-18..33, death 0-103) and 951 (|= 1) on the deathblow throws
  a201_*: now full i-frames in combat.rs dodged() for both sides (no juggling a downed Wolf, invulnerable while
  resurrecting frames 0-27 and during deathblows).

- 2026-10-07 enemy TAE: 703 FixedRotationDirection (IsEnable, on each swing's active frames, e.g. Attack3000 f22-36,
  3002 f28-60) now stops the General's tracking -> sidesteps work as in Sekiro; 225 SetSPRegenRatePercent multiplies
  posture regen (guard / deflect anims 3100-3102: 33 %, fire reactions 0 %). Audit of remaining unhandled enemy events:
  FFX / decals / foot SFX / look-at / draw masks (visual), 66 AddSpEffect_Multiplayer (AI-state / landing behaviour
  SpEffects only).

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

- 2026-10-07 c1010 defence: its script's Goal.Interrupt calls the shared Common_Parry(ai, goal, 50, 25, 0, 3102)
  (common_common_func_NTC.lua), the General has a custom Goal.Parry. Passive-mode port now has both: Foe.common_parry
  = Some((guardMult 50, stepProb 25, stepType 0, rush 3102)) -> rush attack (SpEffect 109990) EndureAttack 3102,
  thrust by parry rank (c1010 resident 221002 = rank 2: no thrust deflect), 109980 + rank 0 -> back-step 5211, guard
  count * 50 % deflect else guard, and a 25 % back-step when the player is just outside reach. 40 s AI trace: approach
  (MoveToSomewhere), ComboAttackTunableSpin 3000 > ComboFinal 3001, AttackTunableSpin 3008 / 3012. 46 pass.

- 2026-10-07 PRIORITY (user): focus on Wolf. No further enemies unless asked; c1010 stays available via config
  enemy.chr but the default opponent is the Samurai General (c1020).

- 2026-10-07 quick turns (HKS): sprint quick turn (SprintLoop/SprintStartFromStep_F, SpEffect ref 2 up, not locked,
  stick > SPRINT_BRAKE_ANGLE 135 deg from facing -> SprintQuickTurnReady -> SprintQuickTurn{Right,Left}180, root motion
  turns; then SprintLoop if dodge still held else SprintStopReady). Locked idle: target > 60 deg off -> StandQuickTurn
  {Right,Left}{90,180} (180 past 120 deg).
- 2026-10-07 FOCUS: sword play (user). Ground routing gained ref 231 GroundAttackCombo1Reverse (a050_300001, + Release
  a050_300101) in the HKS order 214,215,231,219,216,217,218: after every easy deflect, Combo2, DeflectGuardAttack,
  DeflectGuardToStand, sprint/step L attacks the next slash starts from the other side (was Combo1).
- 2026-10-07 air slashes: BEH_A_AIR_ATTACK refs 214/215/216 -> AirComboAttack1/2/3 (1 -> 2 -> 3 -> 2 ...). Landing while
  ref 201 (ENABLE_ORIGINAL_LAND_ACTION, frames 0-27) is up -> LandAirComboAttackN continued at the same time
  (HKS StartTime_00 = env(3063)/1000; Actor::continue_state keeps landed hits). LandAirComboAttack2 carries 231.

- 2026-10-07 attack while holding guard = combat art (ACTION_ARM_SPECIAL_ATTACK); DeflectGuardAttack only routes with no
  art equipped (HKS: env(345, HAND_RIGHT) == SP_ATK_TYPE_NONE). Sim test attack_while_holding_guard_is_the_combat_art.
- 2026-10-07 `sekiro-extract selectors <behavior.hkx>`: every (Custom)ManualSelectorGenerator with its bound variable
  and children by index. "SprintAttack Selector" <- Selector_GroundJumpType: [0] _L a050_302010, [1] _R a050_302011.
  Engine variable table (exe .data 0x143b08e10, {wchar* name, flags}) holds JumpAngle/AttackAngle/SubAttackAngle/KickAngle;
  AttackAngle = signed stick angle from the facing (> 0 right, as _SetStepAngle reads it). Sprint attack now L when
  steering right out of SprintLoop (or left/straight out of SprintStartFromStep_F), else R (was always R).

- 2026-10-07 buffer reset = HKS FireEventNoReset list (player.rs keeps_buffer): the attack Release follow-ups,
  EasyDeflected L/R, DeflectGuardToStand*, StandDeflect{Easy,Hard}Minimum, AirDeflectGuardEnd, sprint quick turns keep
  pending presses (were cleared).
- 2026-10-07 sword-move cameras. TAE 153 CameraModule3 (dispatcher FUN_140b59170 case 0x99 -> FUN_140b51680 ->
  FUN_140832810/830/7f0 into the camera singleton DAT_143d5c0d0 +0x70..+0x85, cleared each frame by FUN_140833e40;
  consumed in ChrCam FUN_14073c260): distance target = LockCamParam camDistTarget chased at CameraParam
  lockCamParamLerpRate (0.05), or at the last EndInterpolationSpeed (+0x80, persists); during the event target =
  CamDistTargetOverride at StartInterpolationSpeed (param rate if <= 0); SlowStart eases from the first-frame distance
  by 1 - (p - 1)^2; the used distance (+0x214) follows at 0.1/frame. FOV (+0x50) and focus height (+0x204) lerp to the
  LockCamParam row at the same rate. TAE 155 SetLockParamID (case 0x9b -> FUN_140737430, slot 1..45, reset to 1 every
  frame in FUN_1407374e0) = CameraSetParam slot (entry id-1: lockParamId, camParamId, beginTime, endTime); row 0:
  slot 11 (air) -> CameraParam 1000 (faster Y chase), 41 (deathblow) -> CameraParam 4000 (lock chase 0.4/0.2),
  8 (death) -> 700/700; blends over beginTime (endTime going back). camera.rs CamState; deathblow test: 4 -> 1.5 -> back.
  Export: all LockCamParam / CameraParam / CameraSetParam rows.

- 2026-10-07 guard release while moving: BEH_A_DEFLECT_GUARD_END plays DeflectGuardToStandMove (hkx
  DeflectGuardToStandMove_Selector on MoveSpeedIndex: Walk a050_203011 / Run a050_203012; ShieldBlock frames 0-6, all
  action cancels from 0) instead of the standing end. Ref 220 (*Variation ends) is raised by no Wolf anim. Gap: with
  lock-on the standing end is kept (the Move clips are forward-only).

- 2026-10-07 TAE 151 CameraLookAtTarget (case 0x97 -> FUN_140b517a0 -> camera singleton +0x3c dmy, +0x40..+0x48
  floats, +0x4c..+0x58 look limits in radians; FUN_14073c260): with dmy >= 0 the focus chases that DummyPoly on Wolf at
  min(+0x40, 1) on all axes (unk1 0.75-0.8 in StandDeflectHardExLarge / deathblows; <= 0 keeps the normal rates; a
  blocked line of sight drops it). Wolf's body dummies were missing (only the sword's were exported): model_c0000.bin
  now holds the base c0000.flver's 563 dummies (0 meshes; tools/extract.ps1 exports it), which also gives body-based
  attacks (kick etc.) their real capsules instead of the reach fallback. Look limits not used yet.

- 2026-10-07 additive guard flinch (HKS BEH_ADD_R_GUARD_DAMAGE): a MINIMUM-level (8) guard/deflect, or Small/Middle/
  Large/Push while ref 202 GUARD_LEVEL_EXCHANGE_MINIMUM is up (only SprintToDeflectGuard frames 0-12), keeps Wolf's state
  (the game layers StandDeflect{Easy,Hard}Minimum / AirDeflect*MinimumAdd with FireEventNoReset). combat.rs additive_guard.

- 2026-10-07 `sekiro-extract objects <hkx> <Type>` dumps every object's members (raw i32/f32, strings, array items).
  TAE 700 CustomLookAtTwistModifier (c0000.hkx): TwistParam {i16 bone from, i16 to, f32 weight, f32 x2 speeds};
  0_Twist (lock-on idle/turns, TargetType Lockon) LR +-45 / UD +-25 over bones 7-44 (0.3) and 79-80 (0.7), sensing dmy 261;
  100/110/120/130_Attack (Freeaim, slashes) UD only, up 30-35 / down 25-35, bones 7-43 weight 1; 30/31_Throw on dmy 260.
  Not implemented: on flat ground the attack twists are ~0 (the lock-on idle twist is visual only).
SWORDPLAY NEXT (for the next session): (a) TAE 700 twists (vertical aim of slashes at height differences, idle torso
  twist to the lock target); (b) TAE 151 look limits (+0x4c..+0x58, used by FUN_140741?/14074.c:401); (c) user feedback.

- 2026-10-07 VISUALS (user: "models and animations to be better"): (1) Wolf had no head: EquipParamProtector 100000
  equipModelId 200 (category head) = parts/fc_m_0200 (face + hair; unpacked + exported to model_c0000_fc_m_0200.bin).
  fc_m_0100 is another face (beard). Face decals (fur, lashes, eyeshadow, mouth) use a shared alpha pack that is not
  exported: flver guess_albedo returns "-" and model.rs skips them; damage overlays are never used as albedo.
  (2) Enemy drew 1 of 36 meshes: NpcParam modelDispMaskNN = 1 HIDES group #NN# in practice (under "show", the General's
  8/14/17/24 match almost no mesh); c1020 now 35 meshes. (3) Lighting: warm key 12k + cool fill 2k + ambient 300.
  VISUALS NEXT: normal maps (_n textures not exported; need tangents), Wolf hair colour (hair texture in the shared pack),
  enemy scabbards stick out (physics/cloth bones stay in bind pose), animation polish per user feedback.

- 2026-10-07 15:09 scheduled run: checked in, no action (last log write 14:46 < 3 h ago; another session likely active).

- 2026-10-07 docs/HANDOFF_PLAN.md: task packets for helper models (V1-V5 visuals, S1-S4 swordplay), file ownership,
  report format (docs/reports/<ID>.md). Lead keeps player.rs / combat.rs.

## NEXT (priority order; tick off with a log line)
1. [x] Camera from data: CameraParam / LockCamParam (distance, height, lock-on behaviour, FOV).
2. [x] Hit stop / hit feedback from data (AtkParam / HitEffect params / TAE events) incl. deflect freeze.
3. [x] Enemy defence: c1020 guard/deflect of player attacks + reactions (deflected, guard, damage, posture break,
       stagger) from c1020 TAE + NPC HKS (extracted/hks_src/c1020.lua) + NpcParam guard fields.
4. [x] Real animation playback + meshes: decode hkaSplineCompressedAnimation (+skeleton) and draw the skeleton, then
       FLVER mesh + textures for Wolf and the Samurai General (chrbnd/partsbnd/texbnd).
5. [x] Hitboxes from dummy polys (FLVER dmy + animated bones) instead of reach approximations.
6. [x] Ghidra: jump launch/gravity, ActionRequest input buffer, the 0.8/0.7 regen level input.
7. [x] Enemy AI: decompile c1020 Lua AI (script/aicommon + c1020 battle logic) and port its decisions.
8. [x] Remaining player rules: deathblow (ThrowParam), mikiri counter, perilous attacks, guard walk, air deflect.
9. [x] Sound: extract deflect/guard/hit SFX from the FMOD banks (sound/).
Round 2 (after NEXT 1-9):
10. [ ] Legacy FEV parse (LGCY: event -> layer -> sound -> sounddef -> waveform) for exact armour / clash / floor events.
11. [x] Exe: writer of ActionRequest accept mask (+0xd0) and clear flag (+0x188) -> exact input buffer windows.
12. [x] Exe: posture value after an unused deathblow window (no restore; regen through the collapse + debt).
13. [x] More enemies through the same pipeline (e.g. Ashina soldier c1000 / c1010 with their battle Lua).
        Plan: (1) unpack chr/c1000.* + its map AI luabnd (script/ai/*.luabnd holding the c1000 battle goal) and
        sound/c1000.*; (2) sekiro-extract export: a CharConfig for c1000 (NpcParam row, behavior/atk params, think id
        from NpcThinkParam); (3) game: replace the c1020 constants (NpcParam 10219000 in combat.rs/enemy.rs/actor
        setup, THINK_ID 10200000, BATTLE_GOAL 102000, model/anim bin names) with a config enemy.chr selection.
        Candidates (NpcParam): c1010 Ochimusha one-handed sword 10100000 (hp 195, posture 75; basic sword enemy),
        c1050 Spear Monk 10500000, c1070 Shura Samurai 10700000. c1000 is only dummies. Map AI luabnds
        (extracted/script/m*.luabnd.d) are unpacked; decompile the battle goal from NpcThinkParam with DSLuaDecompiler.
14. [~] Healing gourd (done) (item use TAE + SpEffect) and combat arts (e.g. Whirlwind Slash) for the full player kit.
Round 3 (2026-10-07, Wolf feel; ideas from studying the ER/Mirror's Edge/FNV rewrites):
15. [x] Swept hit test: sample the attack capsule between the previous and current tick (fast slashes tunnel today).
16. [x] TAE 760 BoostRootMotionToReachTarget: root-motion scale chr+0x2fc from FUN_140842980 (asm: needed, return is
        in xmm0). Fields ReferenceDist/RangeMin/RangeMax/ArriveAngle/ArriveDist (+0x338..+0x348); debug path uses Max/Ref.
17. [x] TAE 226 SetKnockbackPercent: knockback module +0x40 = percent/100 on the actor running the anim (scales the
        knockback it receives?) - confirm the consumer of +0x40 in the decompile, then apply.
18. [x] Behaviour-graph blend data from c0000.hkx (hkbBlendingTransitionEffect durations, locomotion blender weights,
        play rate by speed) -> real crossfades and locomotion blending (gap 8).
        Survey (hkxdump c0000.hkx): only 3 hkbBlendingTransitionEffect + 10 CustomTransitionEffect (duration@184), so most
        crossfades really are TAE Blend events. Worth reading next: hkbClipGenerator x2566 (playbackSpeed@184,
        cropStart/End, startTime, enforcedDuration) -> any clip not at speed 1.0 plays wrong today; the 2
        hkbBlenderGenerators (blendParameter@156, children weights@80) = locomotion blend; CustomManualSelectorGenerator
        x2041 (offsetType/animId, generatorChangedTransitionEffect). Needs a field reader in tools/sekiro-extract/src/hkx.rs.
19. [x] gamedb: join split Ghidra signatures (return type on its own line) before indexing (~6k functions missed).
20. [x] Name the exe from its own RTTI (Ghidra RecoverClassesFromRTTIScript on a copy) for faster research.
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
- 2026-10-06: NEXT 4 phase A done: real animations. tools/sekiro-extract/src/spline.rs decodes hkaSplineCompressedAnimation
  (port of SoulsAssetPipeline's decoder); export writes extracted/anim_c0000.bin (146 bones, 67 clips) and anim_c1020.bin
  (126 bones, 185 clips). src/anim.rs builds bone hierarchies, poses them from actor.anim/t, draws bone lines, crossfades
  clips (0.12 s, gap: real per-transition blend times), mounts blades (player R_Weapon local -Y; enemy R_Hand -> R_Katana_long).
  Phase B (FLVER meshes + textures) next. tools/screenshot.ps1 captures the window (run with -ExecutionPolicy Bypass).
- 2026-10-06: NEXT 4 phase B done: real meshes. tools/sekiro-extract/src/flver.rs (FLVER2 + TPF), command
  `model <flver> <tpf> <out> <texdir>`; Sekiro FLVERs have empty texture paths, albedo picked from MTD name keywords.
  Exported model_c1020.bin (346 nodes, 36 meshes) and Wolf parts model_c0000_{am_m_9000,bd_m_9040,lg_m_9000,wp_a_0300}.bin
  (default outfit from EquipParamProtector 100000-103000, Kusabimaru = wp_a_0300 from EquipParamWeapon 5000).
  src/model.rs skins them onto the animated skeleton by bone name (compact per-mesh joint lists, 256 limit);
  weapon parts mount on R_Weapon. Asset root = extracted/ (DDS via bevy "dds" feature).
  Open: tiling bandage/rope textures (shared texture pack), normal maps, player head check.
- 2026-10-06: NEXT 5 done: hitboxes from dummy polys. Model format v2 carries FLVER dummies (id, attach bone, model-space pos);
  model.rs parents DummyPoly entities to the attach joints (offset = bind^-1 * pos), Dummies map per actor (weapon wins).
  combat.rs: capsule hit0_DmyPolyId1 -> 2 (radius hit0_Radius) vs body segment feet+0.3..1.5 (r 0.4); reach fallback.
  Player slash = weapon dmy 120 -> 100 r 0.4; c1020 = 11 -> 10. Verified in game (enemy combo hits via capsule).
  Debug draw: active hit capsules in red (combat::draw_hitboxes).
- 2026-10-06: NEXT 6a done: jump physics from data + exe. Jump Start anims fire TAE 920 ChrPhysicsVelocityChange
  (a000_201011 -> row 100: 9.64 m/s up; a000_201010 -> row 101: + 6.4 m/s forward; scales 0 = velocity replaced).
  Fall control is a static exe table (0x143b09ce0, exposed by the physics debug menu), per fallType:
  Normal / Normal_Uncontrollable / Decelerating: horiz accel -0.1, vert -13.72 m/s^2, stick accel 10, stick max 2 m/s;
  Wire: -15, -9.8, 9, 6. Vertical jump apex 3.39 m at 0.70 s (= frame 21, matches SpEffect 100397 frames 18-24), 1.41 s air.
  Ready -> Start -> *GroundJumpFall loop -> GroundJumpLandReady_F. [jump] config gap removed. Sim test added.
  Gap: how stick input combines with the launch velocity (modelled as a separate capped stick velocity).
- 2026-10-06: NEXT 6b: input buffer from the exe. SprjChrActionRequestModule (RTTI vtable 0x142a729d8), update FUN_140b2c190:
  33 action bits (HKS ACTION_ARM_*); pressed = held & ~prevHeld; pending |= pressed & acceptMask(+0xd0);
  requested(+0x30, env 1106 via FUN_140b2b6d0) |= pending & executeMask(+0xe0); masks are rebuilt every frame;
  pending has NO timer (cleared only by an engine flag / on use). Per-action hold timers at +0xe8 (env 1108).
  Implemented: buffer = 0 (no expiry); pending cleared when combat puts the player in a damage state.
  Gap: what sets the accept mask (+0xd0) and the clear flag (+0x188 bit 0) - writer not found yet.
- 2026-10-06: NEXT 6c done: the regen level multiplier chr.f[0x10d0] (set in FUN_1409e6xxx PlayerIns update) is the
  Dark Souls equip-load class: FUN_14084d0c0(ratio) -> 4 if ratio > 1.0 or SpEffect stateInfo 102 (row 500, poison
  DoT), 3 if > 0.7, 2 if > 0.3, else 1/0; multiplier 0.8 (3), 0.7 (4), else 1.0. Max load = SpEffect weight rates x
  (1.0 * stat + 40) (FUN_140a34a80). All EquipParamWeapon/Protector weights are 0 in Sekiro -> always 1.0. Gap closed.
  The same level picks the player's breathing add-blend (cases 5/6/7 -> level + 5/10/15).
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
- 2026-10-06: NEXT 9 done: sound. sound/*.fsb are FSB5, codec 15 (Vorbis) with stripped setup headers; FMOD Ex keeps
  them in fmodex64.dll (16-byte {ptr,size,crc} entries; some headers are split codebook/rest blobs). Rather than
  reimplementing that, `sekiro-extract sounds-fmod` drives the game's own fmodex64.dll (FMOD_System_Create /
  CreateSound OPENONLY / GetSubSound / ReadData, NOSOUND output) -> extracted/sound_pcm/<name>.pcm (SPCM header + i16).
  FMOD segfaults on ~140 of 1604 subsounds; progress file + rerun loop skips them (1467 decoded). smain.fsb is
  encrypted (skipped). (`sounds` = experimental pure-Rust Ogg remux, works only for full-header CRCs.)
  Game: src/sound.rs = PcmSound asset + rodio Source, plays TAE PlaySound (types 128-132) keyed
  "<letter><SoundID:09>" with random b/c/d variants (deflect c000004011, guard c000006510, swings c000004010...).
  Enemy events resolve 85%, player 'c' 50% (rest in encrypted smain). Gaps: floor 'x' / armour 'b' sounds are
  material-resolved in the .fev event project; hit/clash SE (HitEffectSe*Param) not wired; no 3D panning.
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
- 2026-10-06: area scaling: "Growth Doping" SpEffects 7010-7014 (Ashina Outskirts morning = base values; day/evening/
  night/bell = x1.25/1.5/2/3 maxHp, maxStamina (posture), physics attack, staminaAttackRate, posture regen). config
  [enemy] area_doping (default 7010); applied at spawn + Actor.atk_rate / stam_atk_rate in combat.
- 2026-10-06: polish: crossfades use each anim's TAE "Blend" event (type 16 at frame 0; player 67/102 anims, mostly
  3-6 frames; c1020 291/364, 1-15 frames) instead of a uniform 0.12 s. Positional sound (SpatialListener on the
  camera, sounds at the emitting actor). Flesh-hit SE s000003010 (HitEffectSeParam Body_* 3010001) on hits.
  Footstep/armour (x/b) sounds still need the engine's floor-material mapping.
- 2026-10-06: real hurtboxes. chrbnd cXXXX.HKX (TAG0) holds hknpRagdollData: 18-19 bodies, each a hknpCapsuleShape
  (a, b, convexRadius) with a bind-pose position/orientation; names "Ragdoll_Ctrl_<bone>" (c1020) / "Ragdoll_<bone>NNN"
  (c0000, NNN -> Spine/Spine1/Spine2). Exported per character ("hurtboxes"), parented to the animated bones in bind
  space (anim.rs), tested capsule-vs-capsule against the AtkParam dummy capsule (combat.rs). H toggles the overlay.
  Tagfile reader now handles 5/6/8/9-byte varints (needed by physics files); `hkxdump` / `ragdoll` debug commands.
- 2026-10-06: player attack routing = HKS ExecAttack: combo refs 214-219, GroundStepAttack_{N,F,B,L,R} (ref 224),
  SprintAttack (ref 1), HardDeflectAtk{Small,Middle}{F,L,R} out of StandDeflectHard*/HardDeflectDmg* (the fast
  counter after a deflect), EasyDeflectedAtk{L,R}, DeflectGuardAttack from guard (ref 213), else Combo1; each with its
  Release on ref 208. Long scripted duel vs the real AI (duel_trace) is now a regression test (no state > 6 s).
- 2026-10-06: hit reactions by direction + level. Enemy: Damage{Small,Middle,Large}_{Front,Back,Left,Right} from the
  attacker's side (HKS GetDirOfPlayableDamage). Player (DAMAGE_LEVEL_*): 1/2 Small/Middle x 4 dirs, 3 Large F/B,
  4 ExLargeBlow, 5-6 LargeBlow, 7 SmallBlow, 9 LargeUpper; blows launch via their TAE 920 (rows 3040/3090/3100),
  loop *FallLoop and land in the matching *Land anim. Test: heavy thrust blows the player away and lands.
- 2026-10-06: vitality emptied by hits opens the deathblow like a posture break (was: enemy left un-hittable at 0 HP);
  after an unused window the enemy is back at 1 HP so the next hit reopens it. Dead enemies are not hittable.
- 2026-10-06: player death plays GroundDeathStart_F (100 f) -> GroundDeathLoop_F, then revives (prototype loop).
- 2026-10-06: floor sounds: main.fev defines c000001000-c0000010xx (and 2000/3000 families) = TAE 'x' id rounded to
  1000 + floor material; arena uses material 4 (hard per HitMtrlParam, shortest/brightest set). Armour 'b' still off.
- 2026-10-06: free turning uses the anim's TAE SetTurnSpeed when present (SprintLoop 360 deg/s, sprint start 180->360,
  guard idle 0); config movement.turn_speed only for walk/run, which carry none.
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
- 2026-10-06: armour sounds: TAE 'b' = armour-material events (FEV has c0000xx113 = Wolf's protector defenseMaterial
  113, c0000xx108 = c1020 materialSe). FEV legacy (LGCY) STRR string table parsed (event names, sounddefs at string
  index 1879+ = ordinal), event->sounddef link not decoded; inferred sets: cloth robe (body-lobe-N) for Wolf, plate
  (body-armor-N) for the General.
- 2026-10-06: camera shake from data: TAE RumbleCam events (145/146 global on the player's anims, 144/147 local with
  FalloffStart/End metres on the enemy's) -> other/default.rumblebnd camera_NNN.hkx = hkaInterleavedUncompressed
  animations (42 shakes, e.g. 61: 0.17 s / 1.1 deg, 56: 0.3 s / 4 deg, 377: 3.3 s rumble); applied as the rotation
  delta from frame 0 (Havok -> game mirrored) on top of the follow camera.
- 2026-10-06: locomotion: directional walk/run clips (a000_0001/0004 + F/B/L/R) chosen from the move direction vs
  facing (locked-on strafing), and stick release plays StandRunStopF / StandWalkStop_F (their root motion slides).
- 2026-10-06: Healing Gourd (E): goods 3000 -> SpEffect 3000 changeHpEstusFlaskRate -40 = 40 % max HP, applied at the
  drink's TAE 65 ConsumeCurrentGoods (ItemGourdDrink frame 21, Repeat frame 9); walk while drinking (flag 90),
  item cancel flag 31, empty gourd -> ItemGourdDrinkFailed; config player.gourd_charges (3). INTERUPT_UseItem is fed
  to the AI (c1020 10219000 lacks the 3102050 drink-punish SpEffect, so it does not specially punish).
- 2026-10-06: Whirlwind Slash (attack + guard together): skill -> virtual weapon 5100 "rotating slash"
  (spAtkcategory 100 -> anim a100_316000 in c0000_a07x, behaviorVariationId 5001 -> judges 200/201 resolved at +1000).
  Its damage is the "dark" element: player damage now sums attackBase{Physics,Magic,Fire,Thunder,Dark} x AtkParam
  element corrections x NpcParam <element>DamageCutRate (c1020 dark 0.6). Dummy ids 10000+ = weapon dummy (id-10000).
  Focus from here (user): Wolf's own combat.
- 2026-10-06: Wolf's prosthetic - Loaded Shuriken (F): weapon 70000 (wepmotionCategory 70 -> offsetType 14 -> a070
  anims in c0000_a07x / a70.tae; behaviorVariationId 7000). GroundSubAttackCombo1 (a070_400000, held: TAE 2 judge 100
  frame 21 -> Bullet 700000) or let go -> GroundSubAttackCombo1Release (a070_400100, judge 150 frame 3 -> Bullet
  700050). Bullet: 80 m/s, life 0.5 s, past dist 5 m decelerate (accelOutRange -40) and drop (gravityOutRange 5),
  hitRadius 0.1, AtkParam 7000150: 200 % of attackBasePhysics 20, posture 5. Exporter: bullets per judge
  (BehaviorParam refType 1 -> Bullet -> atkId_Bullet). src/prosthetic.rs: projectiles vs ragdoll hurtboxes, enemy
  guard/deflect, 1 Spirit Emblem per throw (config player.spirit_emblems = 15). Gap: hand dummy 2 position.
