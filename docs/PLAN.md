# Sv1: finish, fix and polish (plan for all three sessions, approved 2026-10-11)

## Scope (2026-10-13)

"the scope of my plan is not rewrite the whole game i want the game combat and its Enemies to be
playable same as the game and anything that they require music hud special effects move sets etc"

- **In scope:** the combat and the regular enemies playing as in Sekiro, plus what the fights need:
  move sets, AI, music, HUD, effects and sound, and the arena they are fought in.
- **Out of scope:** rebuilding the rest of the game (open-world systems, full-map fidelity for its
  own sake). Performance work is for smooth fights, not world rendering in general.

## Update 2026-10-13: the bosses are split (read first; replaces "all given away" below)

The outside contributor keeps **their half** - the
mini-bosses and generals: c1020 generals, c1021 Seven Spears, c1400 Ashina Elites, c5020 Chained
Ogre, c1370 Blazing Bull, c1070 Shigekichi / Juzou / Tokujiro, c1050 Enshin, c1470 Lone Shadows,
c1040 Centipedes, c1191 Snake Eyes, c1300 Mist Noble, c7000 O'Rin, c1310 Okami Leader, c1380
Sakura Bull, c1130 Armored Warrior, c1340 / c1350 Headless, c108x Shichimen (35 rows). **The
22 main-boss rows are ours again:** 12600050 Folding Screen Monkeys, 50000000 / 50001000
Corrupted Monk, 50600000 Great Shinobi Owl, 50601010 Owl (Father), 50800000 Gyoubu, 50900000 /
50900001 Lady Butterfly, 51000000 / 51000100 Guardian Ape, 52000000 Divine Dragon, 53000000 Old
Dragons, 54000000 Isshin Sword Saint, 54300000 Isshin Ashina, 70200000 Demon of Hatred, 71000000
/ 71001000 Genichiro, 71100000 / 71101000 Way of Tomoe, 74000010 Emma. VFX and Maps takes the bosses' cloth, visual effects and sound effects. The test map is the
sandbox (config `[world] map = "sandbox"`).

## Update 2026-10-12: who does what now

The plan below is still the reference, with these changes on 2026-10-12:

- **All bosses and mini-bosses are given away.** Every row of `extracted/enemies/npc_names.json`
  (57 rows) belongs to an outside contributor (docs/HANDOFF.md 0.1). Their status sheet is
  `docs/reports/bosses-handover.md`. The sessions keep the regular enemies and the shared engine:
  the harnesses, the script interpreter, model.rs masks, the wire engine and the menus.
  - **Moved out of the items below:** in B1, the Genichiro and Monk clip checks and the boss
    rows' share of `all_enemies_full`. In B6, Genichiro's lightning and the boss FX gaps. In B7,
    the Gyoubu and Ape wire windows and Gyoubu's Todome. In C6, the Genichiro and Ogre feel
    clips. In B4, the per-fight castle lighting and the boss arenas (Dragon far view and cloud
    sea, Monk arenas).
  - **Kept:** the c1020 General, c1010, c1150 and Wolf in B1; the TAE 110 handler and the
    effects catalogue in B6; the wire engine in B7; the General feel clip in C6; tone curve
    and exposure in B4.
- **Release hardening and traversal are given away.** C5's font, debug gating and parser errors,
  C6's quiet logs, and the traversal moves go to the contributor in the "Sv1: work split" doc
  (Track 1 and Track 2).
- **Performance is back** (2026-10-13). The C6 goal stands: 60 fps with vsync on the gate map
  and in the 20-enemy arena.
  - bug fixes: the check itself, src/perf.rs. F3 shows the overlay; `SHINOBI_BENCH=<s>` writes
    perf_report.txt. Also the profiling and the CPU-side fixes in the shared engine (anim,
    cloth, AI, combat), each measured before and after.
  - VFX and Maps: the render side, starting with the map-material batching from sekiro-rs
    73eca1d (2026-10-13). Their gate-map table (HANDOFF 4.44) is the starting
    point: 18-24 fps.
  - Bosees: fixes in their files when the profile points there.
  - Not on the collab board yet.
- **The sessions are renamed.** "bug fixes" (was Map), "VFX and Maps" (was VXf, now also maps,
  lighting and cloth) and "Bosees" (was Bosses, now regular enemies only).
- **The open items are claimed on the collab board.** `AKJama/sekiro-collab` WORK.md (5b8f1d6,
  FwLo, RustSekiro):

  | Session | Claimed items | From |
  |---|---|---|
  | Bosees | Hit reach and first-hit frames vs clips (Wolf, regular enemies) | B1 |
  | Bosees | hitSourceType priority, knockback facing, armour cut | B1 open, HANDOFF 4.14 |
  | Bosees | Regular enemies' AI pacing, armoured c1180 servant's armour part | B5 |
  | Bosees | Shadow hiding, forced crouch, patrol routes | B5 stealth |
  | VFX and Maps | Hand-made effects onto FXR; regular enemies' attack effects | B6 |
  | VFX and Maps | HUD art: stealth meter, deathblow mark | B6 |
  | VFX and Maps | Mixer events, sound occlusion | HANDOFF 5 item 4 |
  | VFX and Maps | Lock-on camera offsets, smoothing, limits, switch | B3 |
  | VFX and Maps | Tone curve, exposure, night snow, probe blend | B4 |
  | (given away) | Layered map materials (given to someone else, 2026-10-12; v5 bin + M[Multiple] blend already in, gate map only) | B4 |
  | VFX and Maps | Cape vs a clip, coat detail layer | B2 |
  | bug fixes | Defects in the shared engine and regular enemies (verified bug hunt) | 2026-10-12 |


## Context

Sv1 (`..\shinobi-combat`) recreates Sekiro's combat from the game's own files in Rust/Bevy. Three Claude sessions share one working tree: **Map** (this one: map, lighting, sandbox, cloth), **Bosses** (enemies, bosses, AI, stealth) and **VXf** (the skills/tools session: skills, prosthetics, status, HUD, sound, and all VFX: FXR effects, clash/hit effects, gore, blood decals, weapon trails). Decisions (2026-10-10):

- "Finished" = a **playable game**: close the fidelity gaps, then add what a game needs (title, pause, options, gamepad, enemy picker without relaunch, death/victory flow, clean release build).
- **Fighters only**: the 86 kinds that pass the every-phase check are the target; the 16 non-fighters/scripted set pieces only need to stand and not crash.
- Priorities: **deathblows and hitboxes same as the game**, cape/cloth movement, camera and controls feel, map/lighting/sound.

Surveys done for this plan: all open items in `docs/HANDOFF.md`, `docs/kb/*.md`, `docs/reports/enemies.md`, and a code survey (30 k lines, 29 plugins, 179 tests, 85 `SHINOBI_*` knobs, no menus/pause/quit/gamepad/options, fixed 1024x560 window, hand-assembled `dist/`).

## Ground rules (unchanged)

- Static research first; every value cited from data or the exe, else `// gap:`; memscope only read-only, as the last step.
- `taskkill //IM sv1.exe //F` before building.
- File ownership stays as today (Map: map.rs, sandbox.rs, grading.rs, cloth.rs, shaders, extractor map code; Bosses: enemy.rs, enemy/*, ai/*, stealth.rs, sim_tests enemy tests, data export; VXf: player.rs skills, prosthetic.rs, status.rs, ffx.rs, fxr.rs, vfx.rs, gore.rs, hud.rs, sound.rs, fmod.rs). New front-end files are assigned below. Cross-file edits: tell the owner by SendMessage first.
- Each phase ends with: tests green (`cargo test --release`, smoke harnesses), docs updated (kb + HANDOFF).

## Phase A: stabilise (all sessions, first)

1. **Bosses**: fix `sim_tests::a_boss_takes_one_deathblow_per_red_dot` (second deathblow did not kill; in-progress enemy.rs work). Whole suite green (last: 152 pass, 1 fail).
2. **Map**: quiet the per-frame cloth restart spam (`cloth.rs:458` "restart (fixed particle jumped)" for c1100_sleeve / c1060_mino1): find why their fixed particles jump (bone the ref skin reads vs the animated bone; likely a bone missing from `bones` map so the ref lands at the origin), fix or rate-limit the log to once per cloth.
3. **VXf**: rate-limit `actor.rs:227` "state has no exported anim" (once per state per actor).
4. **Map**: trace the startup `bevy_render slab_allocator Use-after-free` (HANDOFF L651, L800): reproduce with `SHINOBI_MAP=""` vs map; likely a mesh asset replaced in the same frame it is uploaded (cloth `insert_attribute` on a mesh the map/model just spawned). If harmless, document; if a Bevy 0.19 bug, pin the workaround.
5. **All**: HANDOFF clean-up: reconcile stale lines (test counts 107/104/131/148; L8/L29/L95 contradictions; section 8 "still to do" vs L594; section 7 #5 gore vs section 9). One pass by Map session, others confirm their sections.

## Phase B: fidelity, in priority order (sessions in parallel)

### B1 Deathblows and hitboxes same as the game (Bosses, with Map for geometry tooling)

Goal: frame-for-frame match of hit capsules, hurt capsules and deathblow placement against live clips.

- **Hurtboxes from the game data, not stand-ins.** Today Wolf is a 0.4 m capsule (`kb/enemy.md:101`, `kb/movement.md:58`), enemies use NpcParam hitRadius/hitHeight. Port the per-bone hit capsules the game uses for damage (the chr's `_c.hkx`/ragdoll hit shapes already parsed for cloth collidables in `tools/sekiro-extract` cloth_export.rs; the same capsule list, exported as `hurt` into `extracted/enemies/<chr>.json` and the Wolf model) and test them in `combat.rs` hit test (`combat.rs:685` hit capsule between AtkParam dummy polys stays). Keep hitRadius for AI distance only.
- **Hit capsule verification pass**: for Wolf's basic swings, Ichimonji, Mortal Draw and 6 common enemy attacks (General c1020, Ogre c5020 (c1040 is the Long-arm Centipede), c1010 spear, c1150, Genichiro, Monk), compare first-hit frame and reach between sim trace (`SHINOBI_TRACE`, sim_tests `*_trace`) and the Medal clips (ffmpeg contact sheets, as before). Record the table in `docs/kb/damage.md`.
- **Deathblow geometry**: re-check front/behind deathblow placement after the absorb change (HANDOFF L186-188); the Ochimusha behind-deathblow drift up to 0.4 m (`kb/damage.md:174-177`): trace the exe's throw position interpolation (adsrobModelPosInterpolationTime 0.5, atkSorbDmyId 249) and the `targetBaseDmyPolyId 233 + (0,0,-0.3)` approximation (`kb/guard-deflect.md:148`); HP refill after a non-final boss deathblow (`kb/enemy.md:112`); grab ThrowParam lookup exe match (`kb/enemy.md:110`).
- **Unexplained damage**: Wolf's x2.9 HP / x2 posture hits (`kb/damage.md:75,83`), leftover knockback push to the other character (`kb/damage.md:24`), TAE flag 73 confirmation (`kb/guard-deflect.md:50`).
- Verify: new sim tests per capsule set (hit at the right frame, miss one frame earlier), `all_enemies_full` stays 86/102, live-clip tables in the KB.

### B2 Cape / cloth movement (Map)

- Get a close real-game clip of the cape on direction changes (one clip: run, stop, turn, jump). 2026-10-13: the all-skills clip's Run and Slide (9:17-9:33) is from behind and too far (Wolf ~40 px at 480 wide) to read the skirt; still needs a close clip. Compare with the same inputs in sim (`SHINOBI_PHOTO`/`SHINOBI_PHOTO_POSES` or a sim trace with cloth positions logged).
- Constraint execution order and BendStiffness vs the exe (`hclBendStiffnessConstraintSetMx` already ported; check the order list in `constraintExecution` is applied as the exe does, and the transition sets, `kb/visuals.md:124-125`).
- ~~Coat second texture layer~~ done 2026-10-12 (Character_AN_Blend_AMSN_[Fresnel] port, `kb/visuals.md` "Coat shader").
- c1010 spear cloth restart at spawn; the Phase A c1100/c1060 fix.
- Verify: side-by-side contact sheets at the same frames; cloth cost stays under 4 ms with 20 enemies.

### B3 Camera and controls feel (VXf owns camera.rs/player.rs input; Map helps with clips)

- ~~Port `lockCamAdjustRot_MaxX/Y` manual offsets~~ (2026-10-13: unused by the exe, `kb/camera.md`) and the target-point smoothing by `targetChaseRateAtLock` (HANDOFF L132-139, L355).
- Lock-on switch: mouse units done 2026-10-13 (raw counts a frame, as Bevy's); the screen y sign stays open (`kb/camera.md`).
- TAE 151 look limits: cone centre from the dummy basis and lerp back (`kb/camera.md:45`).
- Head-kick camera, vault and jump cloth: check in game with a clip each (HANDOFF L345-348).
- Controls: `invert_y`, `mouse_sensitivity` already; add `[input]` key map in config (see C3) and gamepad (C4) once the front end exists.
- Verify: camera sim tests (pitch formula already ported) plus user play check.

### B4 Map, lighting, sound (Map; sound items with VXf)

- Night snow blow-out at 0 h: decide whether the snow albedo is scaled by the sat shader or the night light set is read too bright (compare the light set rows at 0 h vs 18 h and the real game's night shots; `kb/map.md:197-201`).
- Probe blend between hour variants for in-between hours (`kb/map.md:204`); the +2 EV adaption cap and adaptation speeds from the Yebis params (`kb/map.md:142-146`); LUX_PER_UNIT by comparison shots instead of by eye (`kb/map.md:97`).
- Layer materials: bind each layer's own `_3m` mask, settle UV sets, use blend byte 1 / RGB bytes (`kb/map.md:232-250`); far-view castle stand-ins; `giiv` volumes later.
- In play: stairs, walls, low ceilings vs the camera, spawn height on slopes (`kb/map.md:252`).
- Sound (VXf): play at the TAE event's DummyPoly, map reverb (`setReverbProperties`, `sd_reverb_size`/`sd_dimension`), volume check in game (`kb/sound.md:62`, HANDOFF L347-352).
- Verify: sandbox `[`/`]` walk at 0/6/12/18 h screenshots vs real-game shots; gate map at 60 fps.

### B5 Enemies and AI, fighters only (Bosses)

Status 2026-10-11: done except the Aging status (with VXf) and these documented gaps: the teamType
table, shadow hiding 109201 (no bushes on any arena), patrol routes (blocked), the forced
crouch (no forced-crouch area on any arena; env 3037 untraced). Aging (c1300): gap per VXf (9600
curse build-up -> 9620, resist_curse 999 on every NpcParam, recovers 20/s; the old-man move set).
Foreign arena start: done (Map player_start + enemy::on_own_arena on map::loaded_map).


- Remaining fighter gaps: Aging status for c1300 (`status.rs`, with VXf), c1550 "hidden for directing" row, teamType hostility (EMEVD 2004[2] team flips: no effect, kb/enemy.md), GetDist 3D and the radius field, AI command slot clearing (done), event flags default (done: AI SetEventFlag), Monk phantom vanish/region heights (done), bullets map collision and homing.
- Stealth: map raycasts for sight (use `Terrain::raycast` from map.rs), patrol routes from MSB (blocked: enemies stand at their MSB spots only in boss arenas; format in kb/enemy.md), grass/shadow regions (bushes solved: EMEVD 701 + bush objects, none on any arena; shadow 109201 unset by data), crouch attacks/steps/reactions (done: CrouchAttackToStand; the rest are the standing ones, forced crouch gap), listener ear params.
- AI pacing vs live needs a recorded passive fight (one clip).
- Non-fighters: make sure the 16 spawn, idle and never panic (smoke test asserts "no crash, stands").
- Verify: `all_enemies_full` 86/102 and `all_enemies_smoke` with the non-fighter "stands" rule 102/102. (2026-10-11: smoke 102/102 done; full 95/110 = all but the 15 non-fighters. Done: GetDist 3D, bullets map/homing, sight raycast, ear params, c1550 row.)

### B6 Skills, VFX, HUD art (VXf owns all effects: fxr.rs, ffx.rs, vfx.rs, gore.rs, HUD)

- Status gauges drain and build-up while afflicted; catching fire through the guard; air move ending mid-air; the Todome "recovery prohibited" SpEffects (105051/150302); ActionUnlockParam + virtual weapon rows export (ask Bosses, the export owner).
- VFX through the game's own FXR files: route the hand-made `vfx.rs` (deflect burst, guard sparks, hit blood) and `gore.rs` effects through `fxr.rs` (`kb/visuals.md:219-232`); blood drawn by the FXR player; floor dust SpawnFFX_ByFloor 400/420; hit-blood floor decals (AtkParam decalId, decal blend); lit particles and the 609 light intensity scale; soft-particle gaps (607 shape mode, tracer curve kind); Mortal Draw arc place/size/colour against the clip; the remaining hand-made ribbons for arts without a trace FXR.
- VFX for enemies: every fighter's attack effects (dust, sparks, fire, poison, lightning for Genichiro, the Ogre's grab) play from its TAE SpawnFFX events; catalog shots per enemy in the sandbox (`SHINOBI_SANDBOX_ENEMIES=<chr>` plus `SHINOBI_FXR_SHOTS`) to find missing or wrong effects.
- HUD art: stealth meter with MENU_Find_01/02, deathblow mark FE movie sizing.
- Verify: `SHINOBI_SKILL_SHOTS` sheets for all 19 arts and 10 tools and `SHINOBI_FXR_SHOTS` sheets per enemy, compared with the game clips already collected.

### B7 Grappling hook (all; added 2026-10-11)

Research and the split in `docs/kb/grapple.md`. Map: grapple points exported per map / arena
(object `.edge` grapple edges and map `.edge` files; the o000100 "markers" are item spots) into
`map_<name>.json "grapple"`, a `GrapplePoints` resource with the WirePointSearchParam /
WireVariationParam search and the obstacle raycast, sandbox gizmos. VXf: the player move
(HKS BEH_A_GROUND/AIR_WIRE_SHOOT, anims 202000-202710 + 309000/203400, TAE 900-905 / 921 /
920 row 1100 with ChrPhysicsHomingParam 300000000 guaranteed arrival), hook arm, rope FX, HUD
marker. Bosses: enemies as targets through their `chrWire*` SpEffects (TAE 66 windows; Ogre
3502020-22, Gyoubu 3508020-22/60/61, Ape 3510050), the Grappling Hook Attack throw pair
(511600 / 13600). Verify: sim test for a ground wire to a point 8 m away, the Ogre grab window.
Status 2026-10-11 (VXf): the player move done (shoot, move start, guaranteed-arrival launch,
landing / perch / moving landing, wire attack / guard, wire-to-wire, HUD marker, the WP_A_9500
hook-and-rope model), the Grappling Hook Attack throw (deathblow on landing, Gyoubu's Todome).
Gaps in `kb/grapple.md` section 6 (rope placement, crouch landing, AirBreakFall wire, swim).
Status 2026-10-11 (Bosses): the enemy side done. `enemy/grapple.rs` EnemyGrapples (chrWire*
SpEffects, hook / land dummies, homing row), WireHit -> DamageWire0/1 (c9997 SP_DAMAGE_WIRE),
`Combat::wire_throws` / `throws_of_kinds` (also the hang / hug deathblow rows); kb/enemy.md.

## Phase C: playable game (parallel; new files, little churn in owned ones)

### C1 App states and screens (VXf, new `src/menu.rs`; HUD owner)

- Bevy `States`: `Title`, `Playing`, `Paused`, `Dead`, `Won`. Title: Start / Encounter / Options / Quit. `Esc` pauses (Resume / Options / Title / Quit) instead of only releasing the cursor (`camera.rs:159`). Death: the game's "死" screen look, Retry / Title; Won: "忍殺" / Immortality Severed, Next / Retry / Title. Uses the existing HUD font/atlas path (`hud.rs`, Scaleform atlases in `menu/hi/01_common.tpf`) for the real look.
- `AppExit` from the menu; time pause via `Time<Virtual>` (the F1 Speed path already does this).

### C2 Encounter picker without relaunch (Bosses: data; VXf: UI)

- Today changing enemy type rewrites `config.toml` and relaunches (`debug_menu.rs:442 restart_with`). Make `Combat.kinds` growable at runtime: `DataPlugin` already pushes extra kinds; add `Combat::load_kind(chr, npc_row)` (uses `EnemyFile::load`) and a `SpawnEncounter { map, kinds }` message that `FightReset` handles. Roster from `extracted/enemies/roster.json`; maps: gate, sandbox arena (`SHINOBI_SANDBOX_NO_GALLERY` as the "arena" choice).
- Keep the relaunch path as a fallback for map changes until map hot-swap is proven (map load is 0.1-0.5 s, so hot-swap is feasible: despawn `MapPiece`, rebuild `Terrain`).

### C3 Options (Map: video/audio plumbing; VXf: UI)

- `config.toml` gains `[video] fullscreen, resolution, vsync, shadow_res` (maps to `WindowMode`, `PresentMode`, the existing `SHINOBI_MAP_SHADOW_RES` path), `[audio] master, sfx, music` (FMOD channel group volumes in `fmod.rs`), `[input] invert_y, keys = {...}, gamepad = true`. F5 reload already exists; the Options screen edits and saves the same file (`config.rs` writer from the debug menu).

### C4 Gamepad (VXf, `player.rs` input section)

- Feed `PadInput` (`player.rs:358`) from Bevy `Gamepad` with Sekiro's layout: left stick move, right stick camera, R1/RB attack, L1/LB guard, B/circle step, A/cross jump, X/square gourd, Y/triangle prosthetic, R3 lock-on, L2/R2 tool/art, d-pad switch, Start pause. Deadzones and camera stick speed in `[input]`. Menu navigation with d-pad/A/B.

### C5 Release build (Map)

Status 2026-10-11: no-console exe + sv1.log + panic dialog, `[profile.dist]` + package.ps1, window + exe icon (icon.rs, build.rs) done (HANDOFF 4.16); bundled font, debug gating, parser errors open.

- `#![windows_subsystem = "windows"]` with log to file (`sv1.log`), an app icon (`build.rs` + winres), `[profile.release]` (lto = "fat", codegen-units = 1, strip), window title "Sv1" with the chosen resolution.
- Replace the `C:/Windows/Fonts` font lookups (`hud.rs:122, 468`) with a bundled font under `assets/` (license-checked).
- Missing `config.toml` / `combat_data.json`: an error screen with the `extract.bat` instruction instead of a panic (`config.rs:180`, `data.rs:1133`); binary parsers return errors rather than slice-panic on truncated files (`map.rs`, `model.rs`, `anim.rs`, `sound.rs`).
- Debug surfaces behind `[debug] enabled = false`: F1 menu, T/H/R/`[`/`]`, probe plugins (trace, photo, duel, throw_trace registered only when enabled). Env knobs stay (harmless).
- `tools/package.ps1`: builds release, copies exe + config + tools + README + LICENSE + THIRD_PARTY into `dist/Sv1-vX/` and zips it. README updated (controls incl. gamepad, options, encounter picker).

### C6 Polish pass (all)

- Performance (back on 2026-10-13, see the update above): 60 fps with vsync on the gate map and the 20-enemy arena (today 59/48; the gate map measured 18-24 fps on 2026-10-13); cloth range, shadow cascades, FXR soft particles are the knobs. Measure with src/perf.rs (F3, SHINOBI_BENCH).
- Quiet logs in release (info only for load lines), no per-frame warnings.
- Feel check: three clips (fight the General, the Ogre, Genichiro) compared with the game, last tweaks.

### C7 World map events (2026-10-20, next fresh session)
Run each map's own EMEVD in the open world (the sekiro-rs map mode), not only in boss fights.
Today the open world has no enemies: enemy.rs spawn_lineup returns "line-up: none (idol travel)".
Steps, each behind `SHINOBI_WORLD_ENEMIES=1` until playtested:
1. Exporter (tools/world_scripts.py, sibling of boss_scripts.py / patrols.py): per map, every MSB
   enemy part (entity, chr, NpcParam row, think id, map-space pos / yaw, patrol, wake region) and
   the events that enable / disable characters (event 0 and common_func initialisers expanded:
   2004[5] Change Character Enable State, backread / unload, gated by flag sets and IfEventFlag),
   with each event's rest behaviour -> extracted/enemies/world_<map>.json. patrols.json covers
   only the patrolling ones (25 on m11_01).
2. Placement in the sr_map frame (sekiro-rs world coordinates, not the own map's json_frame).
3. A WorldScript resource beside BossScript, reusing script.rs's step / group / eval with an
   entity table from the export (no boss-relative placement, no fight hand-over).
4. Rest at an idol: re-instance the enemies, restart events by their rest behaviour, keep the
   ones whose defeat flags are set off.
5. Test on m11_01 at idol 1111950: rest, the same enemies come back; a killed one with a
   defeat flag stays gone.
Done = enemies stand where the MSB puts them, wake by their map events, respawn on rest, fps
measured before / after at the Ashina Castle idol. Later: doors, items, NPCs (need sekiro-rs
objects by entity id), the map flags the AI reads (kb/enemy.md: 11100850, 117006xx, ...).
Depends on navmesh (list item 6) for enemies leaving their routes.
Progress 2026-10-20: steps 1-4 in, parked for combat (tools/world_scripts.py, src/enemy/world.rs,
kb/enemy.md "The open world's map events"). Open: fps with 64 enemies (9 fps: the per-frame
side), the start point's story flags, a sim test, step 5's playtest.

## Order and ownership summary

| Phase | Map (this session) | Bosses | VXf |
|---|---|---|---|
| A | cloth spam, use-after-free, HANDOFF clean-up | failing boss test | anim warning rate limit |
| B | B2 cloth, B4 map/lighting, geometry tooling for B1, B7 grapple points | B1 deathblows/hitboxes, B5 enemies/AI, B7 enemy targets | B3 camera/controls, B6 skills + all VFX + HUD, B4 sound, B7 player move |
| C | C3 video/audio plumbing, C5 release build | C2 encounter data/hot-swap | C1 screens, C2 UI, C3 UI, C4 gamepad |
| Polish | perf | AI feel | effects/HUD |

B runs in parallel per session; C starts when a session's B list is done (do not wait for the others). "Save" after each phase.

## Verification

- `cargo test --release` green after every phase (shared target: coordinate builds by message; peers kill `sv1.exe` by image name, screenshots by PID).
- Enemy harnesses: `all_enemies_full` 86/102, `all_enemies_smoke` 102/102 with the "stands" rule.
- Fidelity tables in the KB: hit frames and reach vs clips (B1), cloth frames (B2), camera (B3), lighting shots at four hours (B4), skill sheets (B6).
- Playable check: fresh `dist/` zip on a clean folder: `extract.bat` then `sv1.exe` from the title screen to a won fight and back, keyboard and gamepad, options saved and reloaded, no console window, no panic on a missing config.
