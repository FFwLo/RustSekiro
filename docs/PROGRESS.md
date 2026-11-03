# Sv1: progress and next steps

Goal: a Rust/Bevy combat prototype that plays 1:1 like Sekiro, with numbers read
from the game's own files (TAE, HKS, params) and Ghidra for the gaps.

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
6. Floor sounds ('x' type): the real floor material comes from map collision; FLOOR_MATERIAL 1 (cobblestone, HitMtrlParam names from Paramdex; 4 was wood) stands in.
   (done 2026-10-08: armour 'b' by the wearer's material, hit / guard / deflect sounds by the exe's
   HitEffectSe(JustGuard)Param lookup, effect sounds, gated sounds; smain holds no TAE sounds we use.)
7. (done) Grabbed-player anim a210_600000: the clip is in chr/c0000_c1020.anibnd (Wolf's per-enemy anims).
8. (done) Behaviour-graph transitions are 0 s and locomotion is CMSG clip selection: TAE Blend drives every crossfade.
9. (done) Stick-vs-launch velocity mixing in the air (FUN_140bac120, log 2026-10-07).

## Ghidra notes
Ghidra 12.1.4 at <home>\Desktop\ghidra_12.1.4_PUBLIC. sekiro.exe is Arxan-protected; work from a
dump of the running process plus community symbol names (souls modding wiki / Discord).

## Quick tools (use these instead of ad-hoc snippets; output is compact)
- `python tools/anim_info.py <state|anim> [--enemy] [--types 0,67]` - one state's refs, flags, events;
  `--ref 231` anims raising a ref, `--event 153` anims with a TAE event, `--state Sprint` name search.
- `python tools/model_info.py [name] [--meshes] [--dummy 142]` - exported models: meshes, albedo, dummies.
- `tools/fn.sh <addr> [pattern] [ctx]` - one exe function from extracted/decomp; `tools/fn.sh callers <addr>`.
- `tools/t.sh [filter]` - quiet cargo test (failures and totals only).
- `powershell -ExecutionPolicy Bypass -File tools/photo.ps1 [-Out sheet.png]` - visual check in ONE ~600x500 image:
  photo mode (src/photo.rs, env SHINOBI_PHOTO) shoots back/front/side of Wolf + Wolf mid-slash, both frozen.
  `-Poses "GroundAttackCombo1:12,StandDamageMiddle_F:10" -Cols 4`: one side view per Wolf state at a TAE frame
  (animation audit: 12 poses in one ~800x500 sheet).
- `powershell -ExecutionPolicy Bypass -File tools/relaunch.ps1 [-Shot out.png] [-NoBuild]` - close, rebuild,
  start the game, print model/warn/error log lines (and a screenshot).

## Knowledge base (read only the topic you need)
Detailed findings live in docs/kb/. Each file is dated entries, oldest first. Add new findings to the
matching topic file, and one line to Recent below (keep the last 15).

- [kb/input-buffer.md](kb/input-buffer.md) - Input buffer, accept/execute flags, FireEvent resets (3 entries)
- [kb/attacks.md](kb/attacks.md) - Wolf's attacks: combos, releases, air/sprint/step attacks, combat arts, lunge, auto-homing (14 entries)
- [kb/guard-deflect.md](kb/guard-deflect.md) - Guard, deflect, posture, mikiri, deathblow, perilous attacks (23 entries)
- [kb/damage.md](kb/damage.md) - Damage reactions, knockback, breaks, invulnerability, death and revival (8 entries)
- [kb/movement.md](kb/movement.md) - Locomotion, turning, quick turns, sprint, step, jump, air control, kick (11 entries)
- [kb/camera.md](kb/camera.md) - Camera: LockCamParam/CameraParam, lock-on, TAE 151/153/155 (7 entries)
- [kb/enemy.md](kb/enemy.md) - Enemies: real Lua AI, NpcParam, c1020/c1010, enemy reactions, stealth (9 entries)
- [kb/visuals.md](kb/visuals.md) - Models, textures, animation playback, blending, behaviour graph, lighting (12 entries)
- [kb/sound.md](kb/sound.md) - Sound: FSB/FMOD decoding, sound events (4 entries)
- [kb/tools-exe.md](kb/tools-exe.md) - Extractor, Ghidra/decompile, gamedb, RTTI, exe unpacking (9 entries)
- [kb/next.md](kb/next.md) - NEXT list history and open items
- [kb/log-archive.md](kb/log-archive.md) - the full pre-split log (grep it, don't read it whole)
- [HANDOFF_PLAN.md](HANDOFF_PLAN.md) - task packets for helper models

## Recent
- 2026-11-03 one frame's order checked in the exe (animstart: the HKS decides inside the behaviour step after the clip's TAE update, a pressed clip's first window is the next frame, the hit step runs before the chr frames): Sv1's decide reads the frame before's buttons, resolve runs before advance and its reactions start the tick after; the Loaded Axe release and payment now on the exe's frames 49 / 74 (were 48 / 73); 15 sim tests moved a tick with the exe's evidence (kb/frame-coverage.md "One frame's order").
- 2026-11-03 item use and the prosthetic checked in the exe (items: gourd, pellet, sugars, spiritfall, a throwable, the Loaded Axe): the gourd heals and spends its charge on frame 42 as RSekiro; the axe pays 2 emblems 25 frames into its release as RSekiro but ours starts the tool clip a frame early (gap); the other goods are refCategory-1 bullets RSekiro lacks (gap). Tests `gourd_use_matches_the_exe`, `loaded_axe_emblems_match_the_exe`.
- 2026-11-03 attack record checked against the exe (atkrec: 6,000 judge resolutions, 724 BehaviorParam rows on Wolf and the General, 7 action runs): AtkParam, hit shapes, amounts, posture, flick and SpEffects matched; fixed Force Right Hand events in tool anims, body-row (hitSourceType 1) amounts and flick (0, was 30 / 90) and guard_row's PC fallback. Wolf's attack power = weapon attackBase x correction, Attack Power SpEffect 160000+level scales posture (kb/atkrec.md).
- 2026-11-03 enemy goals into moves checked against the exe (aimove: the General's Lua goals frame by frame at 3-16 m; walk / run / sideways matched; the AI turn rate is now the clip's TAE SetTurnSpeed (run 240, walk 120, was turnVellocity 135); the sight search scheduler ported (no stretch with few enemies); a lunging enemy stops 0.950 m from Wolf without shoving him, as `separate` does).
- 2026-11-03 Wolf in the air checked against the exe (air: whole frames with the pad driven, 17 runs + 24 drops): jump arc, gravity and air steering matched (873 air steps); fixed the walk-off velocity, the fall top (take-off, not apex), SetNoGravity on the exe's window with the launch timer, and fall damage (from 20 m h% of max HP, from 40 m 40% and the return); the StartTime first window is settled (kb/movement.md).
- 2026-11-03 FXR particle runtime checked against the exe (fxr2: the pool update FUN_141dd6f60 frame by frame on 73 particle configs of the deflect / hit sparks, blood and firecracker at 30 and 60 fps, 16,425 frames): fixed the frame order (age and die, emit, move only older particles), one emission a frame with the exe cap, whole 1/60 s effect steps, the 55 / 60 integrators, one random per particle, 502 elliptical spread and direction modes 1-3 (global); 301 and followFactor not covered. kb/fxr.md.
- 2026-11-03 hits landed in the emulator (hits: the hit manager step FUN_1409a0dc0 from MoveMapStep's task, 22 duels): the General's slash on Wolf open / blocked / guard-broken / posture-broken / 14 guard taps and Wolf's on the General; verdicts, reactions, HP / posture, knockback and the deflect window match; fixed the block chip (AtkParam guardCutCancelRate, 0 -> the exe's 205), knockback starting a frame later, the guard break's guardBrake_* knockback (kb/damage.md "Landed hits").
- 2026-11-03 param rows checked against the game's loaded tables (params: SoloParamRepository + ThrowParam read from the snapshot, 140 tables): all 131 exported tables and Wolf's + 102 enemies' 24,690 rows (14 bosses, 18 mini-bosses, common enemies) are the game's values; fixed the swapped ThrowParam sight-cast radii (Top +0x30 / Low +0x32) and the exporter's repeated field names (CoolTimeParam); coverage.md gets a "Data vs exe" column (kb/params.md).
- 2026-11-03 FXR maths checked against the exe (fxr: 6,801 property evaluations on 274 properties of the deflect / hit sparks, blood and firecracker + every kind; 2,176 emitter shape / spread draws): curves matched; fixed the loop wrap (fmodf), the modifier random (the exe's seed x object-random hash, bit exact), and the shapes / spreads ported draw for draw (cone, disk, rectangle distribution, box faces and surface emission, sphere frame, cylinder about Z). kb/fxr.md.
- 2026-11-03 what the exe runs mid-action, traced (acttrace: Wolf's attack, guard, deflect tap, jump, step, sprint, art, prosthetic, item with pad input per frame; the General's forced attack; three duels): about 1,000 functions per action over idle, 947 shared (plumbing); the uncited rule branches ranked for the next wave in kb/frame-coverage.md (item use, attack record build, the hit test's task, physics / fall step). No hit landed in the duel: the collision task is not traced yet.
- 2026-11-03 HKS env ids in poses and the act ids run against the exe (hks2: 223 env poses over 47 ids, 129 act cases, 7 move scenarios): `src/hks_env.rs` mirrors the exe fields the handlers read, 36 more env rules ported (rules for 1,429 of 1,605 calls), 29 act ids write what the exe's do; GetFallHeight's 59 m cap is a no-death debug flag's; the camera-yaw hold and walk halving are debug switches (off as shipped, Sv1 matches)
- 2026-11-03 enemy targeting checked frame by frame against the exe (aiframe: SprjTargetingSystem update on 10,134 emulated frames, 3 enemies, 22 scenarios): the around meter restarts the frame after it fills, slot timers run every frame, the nose sphere ported, the sight ray runs from dummy 500 to Wolf's feet + 1.5, TARGET_ENE_0 queries read -1 with no target; tests targeting_frames_match_the_exe, nose_sphere_matches_the_exe, kb/aicpp.md
- 2026-11-03 SpEffect gaps closed against the exe (speffect2: 40 scenarios, 12,420 frames): the add's "already on" take-over / refuse rules, a replace through the build-up (floor 4011 -> 4012), the regens and posture through `Status`, the buff-duration count, Wolf's worn rows (`player.hard_mode`) in his build-ups; the General's burn was his pre-fight 30201; all 86 runs (44,190 frames) match in speffect_ticks_match_the_exe (kb/speffect.md)
- 2026-11-03 damage-side ticks checked against the exe (damagemod: 1,760 super-armour ticks, 96 whole chr frames after a hit, 154 AtkParam amounts, 54 guard cases): a broken super-armour pool refills on the second tick (`SuperArmor::tick` is FUN_140bdb690), hits with no element damage start no hit stop (`Attack::amounts_zero`), the guard arc is strict (the umbrella guards to 179 deg, not from straight behind), the parts guard never fires in Sekiro; the damage modules tick in ChrIns vf25 on the hit-stop clock; kb/modticks.md
- 2026-11-03 TAE state events and the selector checked against the exe (taestate: Wolf's CustomManualSelectorGenerator run on 134 updates): state events (flags, TAE SpEffects, turn speed, attack windows) now use the exe's windows via `Actor::step` (1,618 event-updates differed from the point test, 0 now; NoGravity stays a point test); the selector's reset (0, offset) for StartTime clips found and left as a gap (it would relaunch Wolf off a land continuation); tests state_events_follow_the_exe_windows, selector_first_window_matches_the_exe, kb/taewin.md
- 2026-11-03 throw checks, second pass, against the exe (throw2: 1,200 sight casts, 16 SpEffect cases, 120 three-chr picks, 850 fall-orbit cases): the sight casts and SpEffect pair ported into the deathblow reach, the pick is the exe's chr order (lowest chr number), not lock / nearest; TAE 932 read in every anim with the ThrowStart2 / 950 / 951 gates; the plunge uses the exe's orbit solver; tests emu_throw2 (kb/guard-deflect.md)
- 2026-11-03 camera gaps closed against the exe (camera2: 2,730 camera frames, 360 manipulator frames): the position chase takes its rate once a frame at 30 fps (0.64 -> 0.4 a frame, was up to 1.35 m off), the follow resumes from a pulled-in focus (was 2.52 m off), the escape-wall option (`input.escape_wall`: wing casts, no fulcrum / orbit casts) and the working rates ported, lock-on at walls matched as it was; the lock-on manipulator takes the stick as it is and a locked, standing Wolf turns to the target; obstacle fade left as a gap; tests camera_escape_wall_pull_in_and_lock_at_walls_match_the_exe, lock_manipulator_matches_the_exe, kb/camera.md
- 2026-11-03 bodies and the floor follow checked against the exe (collide: Wolf into the idle c1150 dog from 8 headings, 25 snap heights, 11 step walks): `separate` now keeps capsules 0.95 apart in 3D with Wolf lifted 0.3 (`actor::contact_flat`, rest 0.3 mm off the exe, was up to 5 cm), grounded bodies follow the floor down 0.6 m (`map::FLOOR_FOLLOW`, was 0.5); no clean 0.4-0.7 m ledge in reach, STEP_HEIGHT 0.5 stays derived
- 2026-11-03 hit stop checked against the exe (modticks: SprjChrHitStopModule start / tick on 273 emulated frames, 54 whole character frames at 1/60 and 1/30): `actor::HitStop` is now the module's state machine (ease in 0, hold 0.06 s, ease out (1 - t / L)^2; no 1e-5 epsilon), the animation steps by the hit-stop clock as the exe's behaviour does; found: a hit with all five amounts 0 starts no hit stop (gap); tests hit_stop_*_matches_the_exe, kb/modticks.md
- 2026-11-03 TAE event window checked against the exe on 2,146 emulated updates (FUN_140b5c920 -> FUN_140654b70 -> FUN_1425e5640 and the queue replay FUN_140b5c730): `src/tae_window.rs` matches every dispatch; one-shot TAE starts now use the exe's 5e-7 edges (Wolf's grid events were a frame late) and nothing re-fires while held at 0; test tae_window_matches_the_exe, kb/taewin.md
- 2026-11-03 throw and deathblow checks run against the exe (throw: 3,000 range placements, the throwable-state table, 112 kind cases): ThrowParam Dist and height strict (throw_range_ok), throwable states ported (a broken unaware enemy takes the break rows, the finisher only in the 104 window), a deflect may empty posture without a break (posture_held); tests throw_range_matches_the_exe, throwable_states_match_the_exe, a_deflect_empties_posture_without_a_break (kb/guard-deflect.md, kb/emulator-pilot.md)
- 2026-11-03 enemy sight cones and the AI's distance queries checked against the exe (aicpp: 52,554 emulated sight placements on six enemies, 288 query placements): the cone maths matched; the sight origin is dummy 500 at the feet height, the light factor / disableDark as the exe's; GetDist measures to Wolf's side (his radius pushed in flat) minus its own radius, GetDistY is unsigned (kb/aicpp.md)
- 2026-11-03 SpEffect list tick checked against the exe on 32,049 emulated frames (add FUN_1409e9c00, tick FUN_140a02a10 / FUN_140a03340): HP per tick summed then one int (burn 9 on Wolf, not 9.6), interval 0 every tick, an expired entry goes a tick later and its replace comes already ticked, instant rows fire on their add tick, a second burn / poison is refused while one is on (no build-up, no refresh), build-up x worn def*ResistDamageRate, gourd heal whole points; tests speffect_ticks_match_the_exe, gourd_heal_matches_the_exe (kb/speffect.md, kb/emulator-pilot.md)
- 2026-11-03 Wolf's character proxy checked against the exe on 810 emulated chr frames (proxy, FUN_140fe7310 in the Havok step): bodies keep 0.05 m off walls (keepDistance; rock face 0.059 -> 0.009 m), the top wall sphere ends at the proxy's 1.8 m; open-ground moves, wall slides, 30 / 60 fps and the 23.52 fall match; step 0.5 derived from the 0.3 lift; test wolf_proxy_at_walls_and_on_open_ground_matches_the_exe (kb/movement.md, kb/emulator-pilot.md)
- 2026-11-03 TAE camera events 151 / 153 / 155 / 156 checked against the exe on 2,160 emulated frames: distance rates per frame, no smoothing, end speed only for TransitionKeyframe; slot blend eased from the blended row, LockCamParam switched at once; angle target from the first frame's yaw; look-at centre from the dummy's view, exe limits and yaw clamp, after-rate, cast gate; test tae_camera_events_match_the_exe (kb/camera.md, kb/emulator-pilot.md)
- 2026-11-03 guard and deflect resolution checked against the exe (guard: 90 emulated cases of FUN_140b6c880 and its apply / attacker sides): a deflect is the flick contest (guard flag + flick defence >= the attack's flick), not any stateInfo 158 SpEffect (the umbrella hold no longer deflects throughout; guardAtkRate 100 blasts are blocked in the window), the guard row's guardAttribute picks the perilous flags (the umbrella blocks thrusts), arcs from guardAngle, no attacker bound on a guard break; test guard_resolution_matches_the_exe (kb/guard-deflect.md, kb/emulator-pilot.md)
- 2026-11-03 Wolf's turn from the move stick checked against the exe on 440 emulated frames (PlayerIns update, 14 scenarios): free turns snap a frame after the stick (no TAE turn rate), a 45 deg stick gate, the drawn facing chases at 792 deg/s; MoveSpeedLevel = stick length confirmed; test wolf_turn_matches_the_exe (kb/movement.md, kb/emulator-pilot.md)
- 2026-11-03 follow camera at walls and the ground checked against the exe (camwall: 2,345 emulated frames, the update's sphere casts hooked): the fulcrum and orbit point are cut by their own casts, the distance chase starts from the cut camera (+0x110) at the hit-wall rate for 2 s after a hit; test follow_camera_at_walls_matches_the_exe (kb/camera.md, kb/emulator-pilot.md)
- 2026-11-03 HKS env surface step 1: Wolf's scripts use 82 env ids (1,605 calls) and 46 act ids (683), each mapped to its handler; the env dispatcher FUN_140b36410 run on Wolf from the snapshot (330 cases, 30 pokes); `src/hks_env.rs` (not wired) answers 22 ids = 1,045 calls (65.1 %), equal to the exe at idle; GetVerticalSpeed on the ground 0 -> -163; test hks_env_matches_the_exe (kb/behaviour-graph.md, kb/emulator-pilot.md)
- 2026-11-03 HP and posture amounts checked against the exe (emulator, 4,410 regen frames, 3,656 damage cases): posture regen per second cut to an int, StaminaControlParam limits by TAE 960 type (a broken enemy's posture held at 0 then set to 30 %), carry runs at full posture; HP damage whole points with a minimum of 1; unguarded posture and zero defences matched as they were; tests posture_regen / hit_hp / hit_posture_matches_the_exe (kb/damage.md, kb/emulator-pilot.md)
- 2026-11-03 lock-on camera checked against the exe on 1,485 emulated locked frames (16 scenarios): lock point averaged over 0.5 s (`LockPointAverage`), framing on the fulcrum's distance, yaw from focus to P, no overhead hold (`lock_chase`), position chase cosine rounded as the exe; pad axes and signs confirmed (stick x = yaw, -y = pitch, move into the camera's view); tests lock_camera_matches_the_exe, pad_mapping_matches_the_exe (kb/camera.md, kb/emulator-pilot.md 3c)
- 2026-11-03 follow camera checked against the exe on 1,626 emulated frames (17 scenarios, memory snapshot): focus follow ported (FUN_141155d20 velocity chase, `FocusChase`), fulcrum sideways move on last frame's right axis, fast turn yaw at the full rate and pitch to the stick's tilt (`direct_pitch`); test follow_camera_matches_the_exe (kb/camera.md, kb/emulator-pilot.md)
- 2026-10-16 tool effects vs the skills recording: axe / spear parts placed right (the 'spear behind Wolf' was a misread axis); every held tool's TAE and bullet effects match the game's; fixes: Divine Abduction's wind-collecting effect 300190 (Wolf's timed SpEffects show their visuals); spear head no longer flies off the shaft (Havok scale rule for tool clips, tip spark 4.26 m vs game 4.4 m); effects on tool parts (flame spear fire, axe trails) were drawn at the world's centre, now on the tool (kb/fxr.md 9)
- 2026-10-16 One Mind slash storm (440131) brighter: lit tracer slashes skip the 1/8 lit scale; quick check of all arts and tools: all effects spawn (kb/fxr.md 9)
- 2026-10-16 Senpou Leaping Kicks / High Monk home on the lock target (homing row 20000, exe FUN_140bb3580 / 3220): no longer jump over close enemies (kb/skills.md); Mortal Draw's emblem-gated blade effects 440005-440008 now play (kb/fxr.md 9)
- 2026-10-16 effects: tracer trails fade out on end (fadeOutTime) instead of hanging as lines; Mortal Draw's brush strokes last their node time; Flame Vent's follow-up spew needs a new press, not a held button (recordings ticks 22283 / 18923) (kb/fxr.md 9, prosthetic.rs held_press)
- 2026-10-15 boss fidelity pass 2: all main bosses' perilous per minute vs the clip (kb/enemy.md); fix: SpEffect watches fire only from the named character (the Headless Ape no longer backsteps all fight); 1 new test
- 2026-10-15 boss fidelity: perilous per minute matches the all-bosses clip for the castle Owl (0, the script never reaches its perilous acts; the clip's 2-4 were Owl Father), Owl Father, Monk and Demon; no code change (kb/enemy.md, kb/clips.md)
- 2026-10-15 effects pass: tracer width along the exe's per-point axis (FUN_14042c4d0 / tracer domain shader); additive tracers skip the lit 1/8 (Mortal Draw red, not pink); fixed-step skill shots; stray hand-made sparks on 27 effect-less tool bullets removed; every art and tool shot, lit smoke darkness left as the light-scale gap (kb/fxr.md 9)
- 2026-10-10 root yaw sign fixed (c1010 behind deathblow matches live, c1020 no 360 spin); crouch moves at once; idle twist lock-on only; sprint deflect stops at the enemy; 105 tests
- 2026-10-10 gore: TAE-driven blood spray (FFX 2205xx on neck / sword dummies) + DecalParam floor stains with the game's decal textures; deathblow mark from the game's HUD atlas; vfx clashes, new HUD, sprint deflect fix; 104 tests
- 2026-10-10 stealth: exe targeting system (sight cones, meter, caution/find/battle, forget) + logic scripts + crouch; 102 tests
- 2026-10-07 helper reports S1/S2/S3/V1/V2 reviewed + integrated: additive deflect (ref 228), guard out of hit (503), sprint-loop attack (ref 1), normal maps, face textures, TAE 151 look limits; 57 tests
- 2026-10-07 gap 7 closed: grabbed clip a210_600000 found in chr/c0000_c1020.anibnd (per-enemy player anims); TAE ImportHKX field parsed
- 2026-10-07 clips: ImportOtherAnim brings the HKX too -> Wolf anims without a clip 58 -> 1, enemy 179 -> 104
- 2026-10-07 deathblow from behind (ThrowParam 11020111); plunge deathblow left as HANDOFF S5
- 2026-10-07 enemy weapons: drawn blade = bone with the attack dummies; TransToBattle draw mask (TAE 233) applied; photo tool
- 2026-10-07 V4 upper-body twist (TAE 700) done: Wolf turns head/torso to the lock target, slashes tilt to height
- 2026-10-07 runtime NPC draw masks (TAE 233/711/713): weapons hide on death/deathblow as in the game
- 2026-10-07 animation audit via photo -Poses: 24 sword/reaction/guard/turn anims, no broken poses
- 2026-10-07 S5 plunge deathblow (ThrowParam 11020150/151 + ChrPhysicsHomingParam 200000300 guaranteed arrival); 59 tests
- 2026-10-07 normal maps Y-flip (DirectX); plunge targets dmy 233; V3 = cloth (c1020_c.hkx), enemy twist mapping open
- 2026-10-07 sound = the game's own FMOD events via fmod_event64.dll (src/fmod.rs); PCM only as fallback
- 2026-10-07 combat arts selectable (config player.combat_art, e.g. 5300 Ichimonji) with per-art AtkParams; 60 tests
- 2026-10-07 additive flinch layer (light hits / minimum guards / additive deflect now animate); c1010 plunge; 61 tests
- 2026-10-07 enemy additive recoils on deflect/block (combo keeps going, recoil visible)
- 2026-10-07 air minimum guards use AirDeflect{Easy,Hard}MinimumAdd (a050_122000/132000) additively
- 2026-10-07 Havok cloth simulated from the game's *_c.hkx: enemy haori/ropes/kusazuri/hakama, Wolf's scarf and robe (src/cloth.rs)
- 2026-10-07 cloth collides with the game's own body capsules (thighs, pelvis, spine, arms, head)
- 2026-10-08 enemy head/torso twist toward Wolf from the game's character properties (TAE 700); Wolf gets both LR+UD twists
- 2026-10-08 combat-art follow-ups (Ichimonji: Double, Floating Passage chains, finishers) from HKS _FireSpAttackCombo + unlock SpEffects; 63 tests
- 2026-10-08 art starts by type: sprint arts, Nightjar step F/B/N (HKS BEH_A_GROUND_SP_ATTACK); 64 tests
- 2026-10-08 hold art (type 104): sheathe, hold, release to draw-slash / HoldAction / cancel; 65 tests
- 2026-10-08 live recording analysed: input windows match (+1 frame engine latency); buffered guard needs the button held (fixed); 66 tests
- 2026-10-08 live recording: speeds, posture costs and regen logged in kb (movement, damage)
- 2026-10-08 art early release = ref 222 (found via live recording); guard posture hook script ready; 68 tests
- 2026-10-15 cloth: solver / damping / collision order / warp-only transfer re-read from the exe, all match; cloth now steps in ~1/60 s pieces at low fps (cloth.rs frame_steps); thin boss cloth swings are in the data (kb/visuals.md)
- 2026-10-15 cloth: all 80 stale cloth.json re-exported (friction, compressible, second sims); hclVolumeConstraintMx ported (Demon mane / beard, Wolf neck scarf, Dragon sakura, Isshin helm fur)
- 2026-10-15 bosses: Emma stays down after her last deathblow; the Demon of Hatred's ghost-fire arm draws (ghost_scroll shader); the dragons are one fight on their own 140 m arena from the sandbox; Genichiro's black-sword sound stops with its event (slotted sounds)
- 2026-10-16 bosses: Bosses tab shows only the 20 main rows; boss audit vs the live recordings (tools/boss_audit.py, docs/reports/boss-audit.md)
- 2026-10-16 boss endings as in the recordings: last mark = usual deathblow, boss kneels (20200), attack during the kneel = Todome; Isshin then kneels again (21000) for the 590 finisher
- 2026-10-16 opacity: a clip without a TAE 193 keyframe takes the group idle's opacity (actor.rs), so a vanish cut short (Owl a000_003037, Owl Father a000_020001, Monk 021000) no longer leaves the boss invisible
- 2026-10-16 Emma's sword: spawn draw mask adds IdleBattle's TAE 233 (only R_Katana #00# and waist Saya #04#, not all five parts); a bone the old clip had no track for no longer fades from its reference pose (her L_Saya floated off her hand). SHINOBI_DUEL_CAM films the enemy from the side; tools/model_info.py reads model v6
- 2026-10-16 Lady Butterfly: a SetNoGravity (TAE 0 flag 27) clip that chains into another keeps her height, and after the lift she falls with gravity instead of snapping to the ground (rec_20261015_225710: 3200 -> 3202 drops 4.5 m)
- 2026-10-16 skinned culling: non-cloth skinned meshes get DynamicSkinnedMeshBounds, so a part far from its bind pose (Gyoubu's face in his deathblow) is not culled while in view
- 2026-10-16 Owl (Father) start: the real game opens with a ~10 s talk (Wolf a000_700400, Owl 401 / 41500 lipsync / 402; rec_20261016_074559), which the port does not have, so he stands in 401 (7.33 s loop). AiState Held now ends at the first TAE 0 AI cancel flag (23/78/79/86)
- 2026-10-16 Owl (Father) talk: src/enemy/talk.rs plays talk ESD t100121's TalkParam 12030000-5 (voices v012030000-5, bank vm10), Wolf's Event700400 / 700420 (added to export_states.txt), the Owl's 402 gesture, Wolf's input held; flag 71000130 at the end lets m10 11005910 turn his AI on. Timing from rec_20261016_074559. gap: lip sync, subtitles. Test `the_owl_father_talks_before_the_fight`
- 2026-10-16 cloth: standard / compressible / bend link stiffness is stored premultiplied with 1 / (invMassA + invMassB) (stretch links are not); cloth.rs divided it again, so light cloth overshot: the Owl's ponytail (invMass 0.16) exploded standing still (gap 0.87 m -> 0.50 m, its LocalRange max). Also stiffens heavy cloth (Isshin, Genichiro's hakama, Wolf's coat) to the data's values
- 2026-10-16 Owl hair cards cut by alpha (Fur_NTC_SSS with g_AlphaRef > 0); Owl blood red squares fixed (DecalParam textureId fallback, missing textures skipped). Open: Owl ponytail too big in his deathblow. kb/visuals.md.
- 2026-10-16 Owl (Father) talk skipped (talk.rs PLAY_TALKS false). Washed-out cloth fixed: empty fabric slots use the game's SYSTEX dummies (grey, not white). Guardian Ape fur shells decoded (C[Shell]). kb/visuals.md.
- 2026-10-17 Weak Lightning Reversal (a050_308910 / 308911) now stuns and does its 508 HP shock: its AtkParam_Pc 5000281 has no damage, only spEffectId0 9400, and the hit filter skipped such attacks (combat.rs). TAE 156 SetCameraAngle ported (camera.rs set_camera_angle, from the exe): about 200 of Wolf's moves, mostly boss deathblows, tilt / turn the camera; Gyoubu's deathblow no longer looks from under the horse. kb/camera.md, kb/skills.md.
- 2026-10-17 Merged claude/move (movement) and claude/amazing-panini-951ca9 (skill effects) into main. Together they broke the head kick over a sweep: the lock-on forward jump's homing row 10000 (point 1 m in front of the target) pulled Wolf back off the head from a body-to-body stand. A homing now does not start when Wolf is already nearer than its point (player.rs, gap: the exe's test not traced; the live head kicks hold at contact). The leaping-kicks test puts the enemy off the unlocked leap's path (a body in the way now stops a jump).
- 2026-10-17 Old Dragons -> Divine Dragon hand-over: m25 12505885 sets 9318 and shows the Divine Dragon's bar (2003[11]); that is a hand-over, not the defeat (script.rs hand_over), so the dragon now fights (15 attack kinds in the audit). The all-bosses defeat test skips the Old Dragons row like the other hand-overs.
- 2026-10-17 Demon of Hatred: his resident foot checks 3702080 / 3702081 (SpEffect stateInfo 275: BehaviorParam 1802 every 0.3 s from dummies 13 / 16 -> Bullet 70200950, 2.5 m) now fire (enemy_bullet.rs fire_worn; the extractor exports behaviorId / dmypolyId and the 'sp<behavior>' bullets). They mark Wolf with 3702085, which his stomp 3046 waits for; the audit now shows 3046, 3039 and 5203. 3006 is still missing there because the audit Wolf hugs him (his 3005 combo point adds 5032, and his script replans at GetDist <= 0.5). His last deathblow now kneels him (20200) for the Todome 17020180: m11_00 11105910's immortality (2004[12]) sat before the event's entry point and was skipped, so the third deathblow killed him (script.rs Run::new carries a skipped 2004[12] over). His idle-arm fire FFX 120 (dummies 820 / 50) is gated on stateInfo 951 = SpEffect 3702021 'HU後炎判定', which no TAE, param, event script or exe constant gives: a gap, probably unused. His self light 3702060 (FXR 670260) is a resident VFX we do not show yet (ffx.rs gap).
- 2026-10-17 Divine Dragon check against rec_20261016_070725: each lightning Wolf sends back (a050_308900) takes 3904 HP (8.7 %) and 867 posture; the first posture break is his HU 20001 (phase 3, 5021), the second 9511 / 20002 -> Event21000, then the finisher 15200090 (a233_510900, 494 s). The sim tests cover that chain (part06, 7 dragon tests pass). The boss audit cannot: its Wolf never jumps, so no lightning is caught, no deathblow lands, and the phase-3 moves (3040-3049, 520000_battle.lua Act40-47) and the height-picked ones (GetDistYSigned >= 4.5 m) never come up there.
- 2026-10-17 Genichiro's trousers and Isshin's robe checked in game: both look right (cloth item closed).
