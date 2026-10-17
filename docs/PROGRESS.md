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
Ghidra 12.1.4 at C:\Users\User\Desktop\ghidra_12.1.4_PUBLIC. sekiro.exe is Arxan-protected; work from a
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
