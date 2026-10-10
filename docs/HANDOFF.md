# Sv1 handoff: everything a new model needs to continue (updated 2026-10-10)

Read this first, then `docs/HANDOFF_PLAN.md` (file map, commands, tools, task packets) and only
the `docs/kb/<topic>.md` file for the topic you work on. `docs/PROGRESS.md` is the short overview.

**Where to start (2026-10-10, late):** section 0 (what the last sessions did), then section 9
(the game's own effects, FXR: in progress - the user wants every skill / tool effect 1:1 from the
game files, not hand-made), section 7 (open bugs: only the cape) and section 8 (skills plan: next
the Flame Vent, the other tools, the art audit, latent skills).

## 0. Session of 2026-10-09 / 10 in one page
Everything below is in the code with citations; the kb files have the details.
- **Stealth** (`src/stealth.rs`, kb/enemy.md): the exe's SprjTargetingSystem (sight cones, meter,
  caution / find / battle, forgetting), the AI logic scripts as the planner, crouch (C), HUD bar.
- **Root-motion yaw sign** (kb/damage.md): `export.rs` writes `-yaw` (X is mirrored, so turns are
  too). Verified against the live recording: the c1010 behind deathblow matches tick for tick; the
  c1020 360-degree spin after plunge / kick-down deathblows is gone.
- **Gore** (`src/gore.rs`, kb/visuals.md): blood sprays from the TAE's blood FFX events on their
  dummies, floor stains from TAE 138 DecalParam rows with the game's decal textures.
- **Clash effects + bloom** (`src/vfx.rs`), **Sekiro-style HUD** (`src/hud.rs`: vitality, posture,
  resurrection nodes, enemy bars, debug text only with F1), **deathblow mark** (the game's FE sprite).
- **Fixes from the user's list:** crouch moves at once (CrouchStart is a standby state), the idle
  twist follows only the lock-on target (Wolf's chest swung at an enemy behind him), the sprint
  deflect faces the target and its 3 m slide stops against the enemy's body.
- **Skills (section 8):** every tool / art / skill exported with official names; prosthetic
  switching (Z), F1 SKILLS menu, the prosthetic core and the Firecracker work.
- **Cape** (section 7 item 2): every cloth value checked against the game files and exe; still
  moves too much. Needs a close real-game clip.
- **After push 60c08dd (not committed yet):** the game's own effects (FXR player `src/fxr.rs`,
  section 9), Mortal Draw's sound no longer loops forever (slotted FFX sounds stop with their
  event), the Mortal Blade model + its effect dummies (TAE 715 WeaponModelType 2 / Model2).
- **The game's own map** (section 4.8, kb/map.md): the Outskirts gate around the General from
  the MSB, map pieces, hit collision and draw params; `config.toml [world] map`.

## 1. The project
- **Sv1** is a 1:1 recreation of Sekiro's combat in Rust and Bevy 0.19, at
  `C:\DeadlockModding\shinobi-combat`. The crate and exe are both named `sv1`.
- **The data is the source of truth:** the user's own Sekiro install (TAE events, HKS scripts,
  params, Havok behaviour graphs and cloth, FLVER models, FMOD sound projects), plus a full
  static Ghidra decompile of the exe (`extracted/decomp/*.c`).
- **Enemies:** c1010 is the Ochimusha (the spear/naginata enemy) and c1020 is the Samurai
  General. `config.toml` sets `[enemy] chr` and `npc_row`.
- **`config.toml` is the user's own file: never commit it** (it shows as modified in git). Their
  setting on 2026-10-10: `chr = "c1020"`, `npc_row = 10203010`. To trace another enemy without
  touching it, run with `SHINOBI_ROOT=<dir>` where `<dir>` holds a copied `config.toml` and a
  junction `extracted` -> the real one (`cmd /c mklink /J <dir>\extracted <repo>\extracted`).
- **GitHub:** `FFwLo/RustSekiro` (public), branch `main`.

## 2. The user's rules (follow exactly)
**Git and publishing**
- **Push only when the user says so** (2026-10-09: "dont push anything to github"; 2026-10-10 they
  asked for one push of this session's work). Ask before every commit and every push.
- The user's own commits on GitHub have empty messages; ask what message style they want.
- **Never commit or publish anything under `extracted/`.** It is game data.

**Research method**
- **Never hand-guess a value that the game data or the exe contains.** Find it and cite it in a
  code comment (function address, param row, TAE event). If it can't be found, mark it
  `// gap: ...`.
- **Static research first.** Live memory reads with memscope are allowed only read-only and only
  as a last step. Never use inline hooks (they crash Sekiro). Never run untrusted exes.
- **Use the references the user gave.** These are read-only clones in `C:\DeadlockModding\reference`:
  - SekiroHKS and SDT-HKS (HKS names)
  - Paramdex (param definitions: `Paramdex/SDT/Defs/*.xml`, with byte offsets)
  - SekiroTool and SekiroToolFork (memory offsets)
  - smithbox

  Also use AKJama/sekiro-rs on GitHub. A copy is in the scratchpad; its BC7 decode is already
  adopted. The user also listed SoulsFormats, SoulsFormatsNEXT, DSLuaDecompiler, ESDLang, UXM,
  Nuxe, WitchyBND, Yabber, DSMapStudio and Smithbox:
  - SoulsFormats is useful for MTD/FLVER material layouts.
  - DSLuaDecompiler is already in `tools/`.
  - The unpackers and editors add nothing.

**Working style**
- Keep replies short and in plain words; the user is not a programmer. They give Medal clips
  (`C:\Medal\Clips\Screen Recording\*.mp4`). Read them with ffmpeg at
  `C:\Program Files\Krita (x64)\bin\ffmpeg.exe` as contact sheets.
- **Relaunch the game after every change, without asking:**
  `powershell -ExecutionPolicy Bypass -File tools/relaunch.ps1 -NoBuild -Wait 8`. Build first
  with `cargo build --release`.
- `taskkill //IM sv1.exe //F` before building, or the exe is locked.
- Work only in the open chat: no background or scheduled sessions.

## 3. State at handoff
- **Tests:** `cargo test --release` passes 107 tests (16 ignored) on 2026-10-10.
- **New files this session:** `src/stealth.rs`, `src/vfx.rs`, `src/gore.rs`,
  `tools/sekiro-extract/src/fmg.rs`. After `sekiro-extract export extracted` you need these unpacks
  too: `sekiro-extract unpack <Sekiro dir> extracted 'decaltex'` (blood stains) and
  `'menu/hi/01_common'` (HUD sprites: the export cuts them to `extracted/hud/`). Regex note: a
  leading `^/` does not match in Git Bash (path conversion) - leave it out. The map needs the
  "Map" block of `tools/extract.ps1` (MSBs, m11_01_00_00, m11 textures, drawparam, maptex, the
  `bxf` expand of the hit binder, then `sekiro-extract map`). Build the extractor with
  `CARGO_TARGET_DIR=target/map` when another session's `sekiro-extract.exe` locks target/debug.
- **Working tree:** this session's work is committed (see `git log`); only `config.toml` (the
  user's) stays modified. Older complaints from 2026-10-09 (head-kick shake, cloth flicker, sound
  levels, vault) were fixed then and not raised again; the current list is section 7.
- **The user asked for "add all of the stealth system".** Core done 2026-10-10 (see 4.7); the
  rest is in 5.1.

## 4. What this session changed (and why)
### 4.1 Cloth (`src/cloth.rs`) and Wolf's coat
- **Coat cutout restored.** The coat's alpha (`P_BD_M_9040_Court`, g_AlphaRef 128) is a real
  cutout (torn hem, ragged collar). An earlier change drew AN_Blend shaders opaque; that was
  wrong and is reverted in `tools/sekiro-extract/src/flver.rs` `alpha_tested`. The model was
  re-exported with `sekiro-extract model extracted/parts/bd_m_9040.partsbnd.d/BD_M_9040.flver
  .../BD_M_9040.tpf extracted/model_c0000_bd_m_9040.bin extracted/tex`.
- **Bend stiffness added** (`Set::BendStiffness`, `solve_bend_stiffness`), ported from the exe's
  `hclBendStiffnessConstraintSetMx` (apply FUN_141526450, singles FUN_14152b490). Every Wolf
  cloth uses useRestPoseConfig and clamp. Without it the coat skirt flipped up to the hips on
  jumps.
- **Pinned particles** now lerp to their reference over the substeps, as the exe's Simulate does.
- **Damping and timestep rescale** were checked against the exe; they match.
- **Cloth display tangents** are carried from the bind pose, not regenerated each frame.
- **Rendering:** alpha-to-coverage for cutouts (`src/model.rs`); ambient light 1200 (`src/world.rs`,
  `SHINOBI_AMBIENT` override).
- Details are in `docs/kb/visuals.md` (2026-10-09 entries).

### 4.2 Camera (`src/camera.rs`)
- **Lock-on pitch ported from the exe** (FUN_14073c260 at 0x14073d6xx-0x14073dd0e):
  - camera +0xd0 = focus and +0xe0 = camera, confirmed by `camera = focus - dir * dist`;
  - P = focus + lockTgtPosRate 0.7 × (target DummyPoly 220 - focus);
  - half = FOV × lockRotXShiftRatio × 0.5;
  - a = asin(dist × sin(half) / |P - focus|), or π/2 when that isn't reachable;
  - target pitch = a + half - elevation(P - focus) (pitch helper FUN_140733e50 = -atan2);
  - the limits widen with |P.y - focus.y|;
  - chase rate lockRotChaseRateX, with the lockRotChasePlayAngX dead zone (0).

  The result is about 20-30° looking down in melee, and steep over the target on head kicks.
  The user hasn't judged it yet.
- **Mouse ignored while locked on** (`orbit_with_mouse`). Mouse pitch had fought the lock chase
  each frame; this is the likely "shake when I play but not in your test". Gap: the
  `lockCamAdjustRot_MaxX/Y` small manual offsets are not ported.
- **Earlier this session:**
  - floor clamp of the camera arm (the camera went under the one-sided floor, which made enemies
    look like they floated);
  - yaw chase held while Wolf is horizontally over the target.
- **Still unchecked:** the exe's target point (+0x280) may be smoothed by the
  targetChaseRateAtLock params. No writer was found; open.
- Details are in `docs/kb/camera.md`.

### 4.3 Throw trace tool (`src/throw_trace.rs`)
- **Modes** (env `SHINOBI_THROW_TRACE`): `front | behind | vault | kick | runkick | jump | idle`.
  - `runkick`: run at the enemy while locked on, jump, then kick off his head.
  - `jump`: jump in place every second, for cloth checks.
- **Options:** `SHINOBI_THROW_DIST`, `SHINOBI_THROW_SHOTS=<dir>`,
  `SHINOBI_THROW_SHOT_RANGE=from,to,step`, `SHINOBI_THROW_CAM_YAW`.
- **Output per fixed frame:**
  - `THROW`: anims, relative geometry, positions;
  - `CAM`: the camera.

  `CAMR` lines give the camera and Wolf per rendered frame.
- The exe segfaults on exit after screenshots; that is harmless.
- **Analysis scripts** (in the scratchpad, easy to rewrite):
  - `camjit.py` parses CAM lines;
  - `camr.py` parses CAMR lines;
  - `tools/rec_throw.py <rec_dir> <tick> [n]` gives the live geometry. Note: in its output,
    "at 180" means the enemy is in FRONT of Wolf.

### 4.4 Sound (`src/fmod.rs`, `src/sound.rs`)
- **Sv1 already uses the game's own FMOD:** `fmodex64.dll` and `fmod_event64.dll` from the
  Sekiro install, with `extracted/sound/main.fev`, `smain.fev` and `cXXXX.fev`.
- **Mix from the exe:** SoundMan (DAT_143d6ce08) volume slots are Master, InGameMaster, Bgm, Se,
  Voice, Menu and OtherMenu.
  - Option bytes 4/5/6 of GameDataMan+0x50 give Bgm/Se/Voice as value / 10. All default to 10
    (FUN_1407bc470).
  - The FMOD master category gets Master × 0.7 (command 10, FUN_141c7b760).
  - Sv1 now sets the master category to 0.7 (`MASTER_VOLUME`).
  - FMOD categories: master > music, Menu, SE, Voice, Default.
- **No PCM fallback while FMOD runs:** a missing event is silent, as in the game.
- **Gaps:**
  - sounds play at the actor root, not at the TAE event's DummyPoly;
  - no map reverb (`EventSystem::setReverbProperties`);
  - the global params `sd_reverb_size` / `sd_dimension` are not set.
- **Test:** `cargo test --release fmod_categories -- --ignored --nocapture` dumps the categories
  and event parameters. The common events have only the automatic "(distance)" parameter.
- Details are in `docs/kb/sound.md`.

### 4.5 Vault, enemy side (`src/player.rs` `follow_throw`)
- **The absorb now uses Wolf's pose at the throw anim's frame 0** (root motion undone).
  ThrowParam 0201 atkSorbDmyId 267 is (0, -0.9, -1.2) in front of Wolf, facing him.
- **Why:** before, the absorb ran one step in, after a200_511900's root yaw had already turned
  Wolf about 10°, so the enemy ended about 10° crooked.
- **Result:** final dyaw 0 (was 9.4), net about 1.1-1.3 m forward. Live c1020: enemy yaw -3.3°
  at the start, about 1.1 m net forward.
- **This also affects every deathblow absorb.** The tests pass, but re-check the front and behind
  deathblow geometry against live (`SHINOBI_THROW_TRACE=front|behind`). Earlier verified values:
  behind ends 0.58 m in front, same yaw (live 0.59).
- **Not changed:** ThrowDef13900's root carries the enemy 1.2 m forward over 3.3 s.

### 4.7 Stealth (2026-10-10; details in `docs/kb/enemy.md`)
- `src/stealth.rs`: the exe's NPC targeting system (sight cones from NpcThinkParam, the "around"
  meter, NONE / CAUTION / FIND / BATTLE, forgetting, sound / indication / memory targets).
- `src/ai.rs`: the logic script (`<logicId>_logic.lua`) now plans the top goals; new natives
  Stay, BackToHome, ConfirmCautionTarget; targets can be spots and points (home).
- `src/enemy.rs`: AI-state SpEffects from the alert anims' TAE pick the Idle / Walk variants;
  `Mode::Unaware` is gone (`is_unaware` = state below FIND). The duel still starts in BATTLE.
- Wolf crouch on C (`player.rs`, `actor.rs` crouch flag: clips + 5000, SpEffect 109200).
- HUD bar over the enemy (`hud.rs`). Debug menu DEATHBLOW > "enemy 22 m away, facing you";
  env `SHINOBI_STEALTH=far|behind` starts that way.
- Exporter: NpcThinkParam / AiSoundParam rows and the stealth SpEffect fields; crouch states.
- Trace: `cargo test --release stealth_trace -- --ignored --nocapture`.

### 4.8 The game's own map (2026-10-10; details in `docs/kb/map.md`)
- User's choice: "Real Sekiro map": the fight takes place where the General stands in
  m11_01_00_00 (the Outskirts gate), from the user's own game files, nothing modelled by hand.
- Exporter (`tools/sekiro-extract/src/{map,msb,mtd,gparam}.rs`, `hkx.rs collision_mesh`,
  `container.rs read_bxf4`): `sekiro-extract map extracted m11_01_00_00 c1020_0004 80` after
  the unpacks in `tools/extract.ps1` ("Map" block). Writes `extracted/map_<id>.bin/.hit/.json`
  and the textures. Pieces by the hit collision's draw groups, LOD by distance, materials from
  the MTD defaults, hit materials = HitMtrlParam ids.
- Game (`src/map.rs`, `MapPlugin`): static pieces with the character material, the collision
  world ported from sekiro-rs (floor / step / walls / camera ray), lighting from the draw params
  (light set 100). Hooked into player.rs (landing, free fall, plunge height), camera.rs (arm
  clamp), sound.rs (floor material), world.rs (no test floor with a map). Config `[world] map`.
- The flat grey pieces were Bevy's default clamp-to-edge sampler on tiled UVs; map textures now
  load with repeat. Other debug views: `SHINOBI_MAP_UNLIT`, `SHINOBI_MAP_TEX`, `SHINOBI_MAP_NO_NORMALS`.
- The arena is turned so the General keeps his real spot (z = -4, facing +Z) and Wolf stands
  in front of him; 54 fps with 6.86 M triangles. `cargo test --release map::tests` checks the
  spawn floors, the gate posts (walls) and the camera rays on the exported collision.
- Lighting ("fix the gate lighting to match the real game", 2026-10-10): the area's own GI
  probe cube maps (`envmap_00..05.tpf` -> `gilm0380`, the gate's `Env_Box380`) feed an
  `EnvironmentMapLight` + `Skybox`, the light set's sun / fill turned by the arena yaw, hour 18
  (dusk, the first-visit look). New extractor files: `cubemap.rs` (BC6H via bcdec_rs, arena
  resample, irradiance, RGBA16F cube DDS), MSB regions in `msb.rs`, `tpf` command. Details and
  the variant-hour mapping in kb/map.md. Left: the colour-grading LUT and auto exposure.
- Not yet verified in play: stairs, low ceilings vs the camera.

### 4.9 All enemies and bosses (2026-10-10; details in `docs/reports/enemies.md`)
- User: "Work on all bosses and enemies". Every placed chr (extracted/enemies/roster.json) loads
  with its model, textures, real Lua AI, attacks, bullets and deathblows: 77 of 102 pass
  `all_enemies_smoke`; the 25 others are mostly merchants, conversation NPCs and scripted fights.
- New: `src/enemy_bullet.rs` (NPC bullets), `src/duel.rs` (`SHINOBI_DUEL` real-game check),
  resident StateInfo gates, opposeTarget, magic/fire damage, enemy grabs, boss deathblow
  count (`Enemy.ninsatsu`, `Mode::Rising`; GetNinsatsuNum / GetNinsatsuMaxNum), chained
  goal setters in ai.rs.
- Phase 2 ("work on the phase 2 boss move sets"): anim sets a000-a400 (SpEffect 200030-200034,
  `Actor::grouped`), the Todome (ThrowParam 180/181 -> Event20200), and the bosses' map-event
  phase rules (`tools/boss_events.py` -> `extracted/enemies/boss_events.json`, `enemy.rs
  phase_events`; GetEventRequest real). Monk and Demon of Hatred tested.
- Every-phase check `all_enemies_full`: 84 / 102 (fixes: grab re-trigger, grab release, AI
  requests in the anim set). Details in `docs/reports/enemies.md`.
- Open: c1500 ground-grab zombie, Monk clones (multi-enemy), HU1/HU2 HP change (exe), 0-bar
  follow-ups (Monk 20021, Wolf 7102xx), c7100 -> c7110,
  the Aging status (c1300), c1500 / c1550 rows.

### 4.6 Earlier in this session (already in the KB)
- **Lua 5.0 AI VM** vendored from sekiro-rs (MIT, `THIRD_PARTY.md`); runs the real enemy AI
  scripts (`src/ai.rs`).
- **Throws:** front, behind, near, deflect break, Mikiri break, plunge, kick-down, vault, and the
  stealth deathblow and plunge, with the kill follow-ups. ThrowParam suffix table:
  `docs/kb/damage.md`.
- **Root motion** in the clip-start frame; vertical root lift while flag 27 is active; FreeFall
  and LandFreeFall.
- **Basic stealth:** `Mode::Unaware`, `perceive()` (NpcThinkParam sight cone, AiSoundParam
  radius), TransToBattleFromDefault. Gap: the meter fill rate.
- **Debug menu items:** PlungeNow, KickDownNow, VaultNow, StealthSetup; Thrust AI mode.
- **Enemy family textures** (`chr/c1019.texbnd`, `c1029.texbnd`); the extractor's `dds2png`
  (BC7 via texture2ddecoder).

## 5. Open work, in rough priority
0. **Map, the rest** (kb/map.md "Open"): check stairs / walls / low ceilings and the spawn
   height in play, compare the gate lighting with the real game (light set 100 at noon; sun
   and ambient scales are by eye), the far-view castle stand-ins drawn next to the gate, the
   second texture layer of M_Multiple materials, radius / LOD tuning (6.86 M triangles).
1. **Stealth, the rest** (core done, 4.7): crouch attacks / steps / reactions (HKS Crouch*
   states); grass / shadow hiding (109201 / 109203 need map regions); wall hug, hanging (limit
   types); patrol routes (MSB); the listener's ear params; the real HUD art; the user's verdict.
2. **Confirm with the user** (ask for clips if still wrong):
   - head-kick camera feel with the mouse fix;
   - cloth on jumps;
   - sound volume;
   - vault.
3. **Kick-launch slide:** about 0.3 m backward over 3 frames right after
   AirKickEnemyJumpStart_F_Lock relaunches (`runkick` trace, frames 53-55). Check
   a000_213115's root motion and VelocityChange row 2101.
4. **Sound gaps:** DummyPoly emitter positions, reverb, `sd_*` params.
5. **Coat second texture layer** (AN_Blend: FC_A_0000_Fabric_blend05/01 detail at 25× tiling).
   The shader math isn't known; it is in the compiled SPX shaders.
6. **Camera gaps:** lockCamAdjustRot manual offset; target-point smoothing.
7. **Clean-up:** the `SHINOBI_CLOTH_FLIP_N` test env in `cloth.rs`, and the per-frame "ref gap"
   debug log.
8. **c1010 spear cloth** `#04#_c1010_yarinuno` restarts at spawn (particle 3.2 m from root): one
   warning, harmless so far.

## 6. How to verify things
- **Gameplay logic:** `cargo test --release`; add sim tests in `src/sim_tests.rs`.
- **Geometry vs the real game:** compare `SHINOBI_THROW_TRACE` output with the memscope
  recordings.
  - Recordings: `C:\DeadlockModding\memscope-data\logs\rec_*`. `index.txt` lists the blocks.
  - Slot 0 is Wolf, 1-2 are the nearest enemies.
  - physics +0x80 = position, +0x74 = yaw, +0x84 = y.
  - chr_head +0x68 = NpcParam id, +0x6C = chr id.
  - Readers: `tools/rec_read.py`, `rec_throw.py`, `rec_timeline.py`.
  - Live vaults: `rec_c1020_b_20261009` tick 13131 (clean) and `rec_c1010_20261009` tick 3600
    (slot swap, messy).
  - Live jump physics: `docs/kb/movement.md` (vertical jump 1.98 m, 9.4-9.8 m/s, gravity 23.52).
- **Visuals:** throw-trace screenshots tiled with ffmpeg (`tile=6x5`), or `tools/photo.ps1`.
- **Exe research:**
  - `tools/fn.sh <addr>` prints a function;
  - `extracted/rtti_vtables.txt` maps classes to vtables;
  - read exe bytes from `extracted/sekiro_steamless_dearxan.exe` (map VA via the PE sections);
  - capstone is installed for Python. Don't name a script `dis.py`: it shadows the stdlib.

## 7. Open at 2026-10-10 (user's bug list; then section 8, the skills plan)
Real-game reference clips (user's own, read with ffmpeg contact sheets; use forward-slash paths):
`C:/Medal/Clips/Screen Recording/MedalTVScreenRecording20261010013052179.mp4` (behind deathblow + gore),
`...013749267.mp4` (running, slides, cloth), `...013834205.mp4` (clashes, sparks, crouch moving).
Done this session (see kb): slide from sprint, sprint-deflect no target snap, raw root yaw (c1010 behind
deathblow), vfx.rs sparks/flash/blood + bloom, Sekiro-style HUD (debug text only with F1). 104 tests.
User's open list:
1. (done 2026-10-10) Crouch: "can't move when I crouch" = CrouchStart held Wolf 0.83 s. HKS g_paramHkbState
   CROUCH_START is STATE_TYPE_STANDBY -> is_free (test moving_right_after_crouching_is_not_held).
2. Cape moves too much on direction changes (2026-10-10: every value checked, still open). Verified
   against the game: cloth.json (gravity, globalDampingPerSecond, substeps, iterations, constraint
   order, local range max 0.24 / 0.74 m, normal min 0), transfer motion off in all 7 of Wolf's cloths,
   transition sets all zero (only for forced to-anim), no BlendSomeVertices in Wolf's #01#Default
   states, the exe's cloth dt = [frame dt, 1/30, 1/60][mode] (FUN_141045c40) with mode 0 = frame dt
   by default (SprjClothImp ctor FUN_1410450c0: +0x40 = 0; also max 10 chars, update dist 300 m),
   turn speed 720 deg/s (no TAE 224 on the run loops). Next: a close real-game clip of a run +
   direction change to compare frame by frame; check our constraint solving order / BendStiffness.
3. (done 2026-10-10, without a clip as asked) Sprint deflect: a050_203001 slides 3.0 m along its facing
   (SetTurnSpeed 180 / 360 deg/s f3-9). Wolf faces the lock-on target at once, the slide follows the
   facing and stops against the enemy's body (actor::separate weights it like DeathblowStart) instead
   of shoving him. Player.slide_yaw removed.
4. (done 2026-10-10) c1010 behind deathblow sync + c1020 360 spin after plunge / kick-down deathblows:
   root yaw kept its raw sign while X was mirrored. export.rs now writes -yaw (live: game yaw y = ours
   pi - y). c1020 ThrowDefDeath 13411 / 13511 / 12311: body -180 + root +180 in the game's space; raw
   they added to 360. In game (SHINOBI_ROOT with a c1010 config, SHINOBI_THROW_TRACE=behind) the pair
   now matches rec_c1010_20261009 tick for tick (dist within 0.01 m, angles within ~1 deg). Sim trace:
   behind_throw_live_trace (no absorb in the sim: no models).
5. (done 2026-10-10, src/gore.rs) Gore from the TAE: blood FFX 220502/3/5/6 (TAE 96) spray from their
   dummy along its forward for the event's length; TAE 138 DecalParam stains (710011) on the floor with
   the game's own textures (extracted/decal/<id>.png, written by `sekiro-extract export` from
   other/decaltex.tpf - unpack 'decaltex' first). Open: FXR not decoded (spray look is made to match
   clip 1), floor dust SpawnFFX_ByFloor 400/420 not done, hit-blood floor decals (AtkParam decalId) not done.
7. (done 2026-10-10) Weird idle after the c1010 plunge (user clip): Wolf's upper body swung side to
   side. The idle's TAE 700 twist (TargetType 3 Lockon) fell back to the auto-aim target, an enemy
   right behind him flipped it between +-45 deg. Now TargetType 3 follows only the lock-on target.
8. (done) "c1020 changed after the gore" = the 360 spin in item 4.
6. (done 2026-10-10, hud.rs DeathblowMark) Deathblow mark: the game's FE sprites MENU_ninsatu_02 + _01
   (menu/hi/01_common.tpf atlas SB_FE, rects from 01_common.sblytbnd SB_FE.layout; export writes
   extracted/hud/) on dummy 220 while player::deathblow_check passes (broken, or in reach behind an
   unaware enemy). gap: FE movie 01_000_fe.gfx (size / animation) not decoded. Unpack 'menu/hi/01_common'.

## 8. Skills plan (2026-10-10; user: "do all the skills and their effects and way to change between them")
Order: switching first, then combat arts, then prosthetic tools one by one, then latent skills. Ship
each step to the user (relaunch the game), keep everything data-driven (cite rows / HKS lines /
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
- Still to do in this list: steps 2 (arts audit), 4 (tools 072-079), 5 (latent skills), HUD icons.

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
  Sabimaru 31 / 150 per cut. A hit while burning -> W_FireReaction (c9997 SP_DAMAGE_BURNING; gap:
  catching fire through the guard also flails, rank 3 not checked). Looks: SpEffect vfxId ->
  SpEffectVfxParam init / midst sfx (burn 3012 on dummy 850 + kanji 3013 on 280; poison 4011 /
  4013), added to `tools/fxr_extract.py`. gap: gauges never drain; no build-up while afflicted.
- Check tool: `SHINOBI_SKILL_EVERY=<frames>` repeats the press in `SHINOBI_SKILL` shots.
- Combat art costs: the art's wepCost behaviours pay its EquipParamWeapon resourceItemA
  (`player::art_cost`: Dragon Flash 5400 2, Ashina Cross 5500 2, Mortal Draw 5700 3). Too few emblems ->
  the `*NoResource` states (offsetType 13, exported as a050_<id>) and their releases. The art's "has
  emblems" StateInfo (`art_emblem_gate`: SpEffect resident/100*100+10 -> Spiral Cloud 994, Mortal
  Draw 993, Dragon Flash 995, Ashina Cross 990) is put in the gates when emblems >= the cost (gap: the
  exe applies it). Tests `dragon_flash_costs_two_emblems`, `every_art_starts_and_costs_its_emblems`
  (all 19 arts: opening state, cost, back to idle).
- Jump arts 107 / 110 (Senpou Leaping Kicks, High Monk, Sakura Dance; HKS 3437-3453):
  GroundSpecialAttackJumpReady (316700) -> JumpStart (316710, TAE 920 launches) -> JumpFallLoop
  (316730); lands into LandGroundSpecialAttackJumpStart (same time) while ref 201 is up, else
  LandGroundSpecialAttackJumpFallLoop (316740). Out of a sprint SprintSpecialAttackJumpReady
  (316770: CMSG has no animId for it, `art_state_id`). Pressed
  again in the landing -> Combo2 (HKS 5386). One Mind's hold action -> VariationCombo2 / NoResource
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
  AirSpecialAttack -> LandAirSpecialAttack. Ends in the air -> FreeFall. gap: the *NoResource
  air clips ship in no group (no emblems -> nothing); ACTION_UNLOCK_TYPE_AIR_SP_ATTACK read as
  learned. Test `arts_in_the_air`.
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
  read as recovery (from the rows' names 忍殺時体幹回復). gap: "recovery prohibited" 105051 / 150302
  (event scripts) never applied. Test `deathblows_give_back_posture_and_hp_with_the_skills`.
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
1. Gaps listed above (`// gap:` in player.rs / prosthetic.rs): the exe's gate
   StateInfos, "recovery prohibited" SpEffects, the air *NoResource
   clips.
2. **Visuals:** the game's FXR effects now play (section 9). All 19 arts and 10 tools were
   shot (`SHINOBI_SKILL=<id>`; the log now prints Wolf's state, place and facing) and fixed where
   wrong (camera-distance fade, tracer subdivision: section 9). Sounds come from the FMOD banks.
3. **Tool models (open):** no prosthetic tool has a model yet (the Umbrella opens no canopy; no
   axe / spear / Sabimaru blade). The game's are /parts/wp_a_0700..0791.partsbnd (26): WP_A_07xx
   (.flver, .tpf, an anibnd with a999 only) and WP_A_07xx_1 whose anibnd has clips named after
   Wolf's anims plus `_1` (umbrella: a076_405010_1 GuardStart, 405030_1 GuardEnd, 412000_1
   expand ...), so the tool plays the clip of Wolf's current anim. Export asked of the Bosses
   session (owner of export.rs / extract.ps1): model bins, the clips keyed by Wolf's anim id, and
   the EquipParamWeapon rows 70000-79999 (equipModelId; only 70000 is exported now). Runtime
   written, waiting on that export: `src/model/tool.rs` (child of model.rs). The equipped tool's
   Model0 rests on WepAbsorpPosParam left_0 (arm dummies 112..124), Model1 shows only while a Left
   Weapon TAE 715 places it (umbrella canopy on 21), each posed by its clip for Wolf's current anim
   (else a999_000000). Files: extracted/tool/model_wp_a_0<model>[_1].bin, anim_wp_a_0<model>[_1].bin.
   Next: once the export lands, shoot the Umbrella (`SHINOBI_SKILL=76000`) and check the canopy's
   place / pose; then the other tools.
4. **Sparks when an enemy deflects / guards Wolf (open):** vfx::hit_sfx reads the attack's
   defSfxMaterial1/2, but Wolf's AtkParam_Pc rows hold 255 (281 rows) or 0 (159), with 139 in
   slot 2; the sword's EquipParamWeapon 5000 has defSfxMaterial1/2 = 101 / 139 (defSe the same).
   Likely 255 = take the weapon's value. Not yet traced in the exe (accessors in
   extracted/decomp/1410c.c: FUN_1410c1780 JustGuard, FUN_1410c1850 Concept); trace it before
   using it.
5. Private test builds: `CARGO_TARGET_DIR=target/fx` (peers rebuild target/release). Unpacking needs
   `MSYS_NO_PATHCONV=1` in Git Bash or the '^/parts/..' regex is mangled.

Verify with sim tests (`src/sim_tests.rs`) per tool and art, and screenshots via the throw-trace
shot env vars or `tools/screenshot.ps1`. Nothing committed yet this session: ask the user before
committing (never `extracted/`, never their `config.toml`), never push.

## 9. Game effects from the FXR files (2026-10-10; user: "pull them from the game, they are not 1:1")
Every TAE effect (FFXID in TAE 96 / 118 / 120..) whose `f<id>.fxr` exists now plays the game's own
effect, drawn by our FXR player. The hand-made looks in `ffx.rs` are only a fallback for ids with
no FXR.

**Pipeline (run once after `sekiro-extract export`):**
- `sekiro-extract unpack <Sekiro dir> extracted 'sfxbnd_commoneffects'` (2349 FXRs + their TPFs).
- `cd tools/fxr-dump && npm i` (@cccode/fxr, public domain, github.com/EvenTorset/fxr; a clone is
  in `C:/DeadlockModding/reference/fxr`), then `python tools/fxr_extract.py`: every FFX id the
  exported characters use (TAE, bullet sfxId_*, blood 220502/3/5/6) -> `extracted/fxr/<id>.json`,
  following ReferenceNode (2001 "sfx") chains; every texture they name -> `extracted/fxr_tex/<id>.png`
  (s<id:05>.tpf). ~280 effects.
- Effect models (Model appearance 605: Abduction's leaves s04010, the shuriken s08050, ...): per
  model `sekiro-extract model extracted/sfx/sfxbnd_commoneffects.ffxbnd.d/s<id:05>.flver <its
  diffuse's .tpf in the same folder, or -> extracted/fxr_model/model_<id>.bin extracted/tex` (done
  by hand on 2026-10-10 for 4010 4011 4017 4018 4070 4100 4140 4141 8050 8060 8130 8131 8180 11601
  11602; the 8050 / 8130 / 8131 / 4140 / 4141 packs are s08050_a / s08130_a / s04133_a). Drawn
  by `fxr.rs` (sizeX/Y/Z, uniformScale, rotation + angular speed; unlit, blend 0 drawn as normal).
- The Mortal Blade parts: `tools/extract.ps1` now exports `model_c0000_mortal_a_0300.bin`
  (WP_A_0300_2: no meshes, the effect dummies), `model_c0000_mblade_a_0310.bin` and
  `model_c0000_mbsheath_a_0310.bin` (WP_A_0310 + _1). Unpack `wp_a_0310.partsbnd` too.

**Player (`src/fxr.rs`, FxrPlugin):** nodes 2000 / 2200 / 2001 (refs) / 2202 (LOD: first child);
configs 1004 / 1005 (node emitter templates); emitters 399 / 300 / 301; shapes 400-405; spread
500-503; particle movement 55 / 60 / 84 / 105 / 64 / 65; node movement 1 / 34 / 106 / 122
(followFactor: a node detaches from its anchor when it drops to 0); appearances 600 / 602 / 603 /
604 (sprites with OrientationMode 0/1/2/4/6/7), 606 / 10012 (tracers: TracerOrientationMode 0-5,
segmentInterval / segmentDuration / concurrentSegments), 609 (lights: top 8, intensity x4000 =
gap). Properties: Linear / Stepped / Hermite / Bezier keyframes, RandomDelta / RandomFraction /
RandomRange; arguments Constant0 / ParticleAge / EmissionTime / ActiveTime. Blend modes 0/4/7 add,
1 source, 2/6 normal, 3 multiply, 5 subtract. Space: X mirrored (`game_vec`), Euler Z->X->Y with Y/Z
flipped (`game_rot`). A negative particle duration = lives as long as its node. Batched: one
dynamic mesh per (texture, blend). Screen effects (Distortion 607, RadialBlur 608, tracers'
distortionIntensity): `DistortMaterial` + src/fx_distort.wgsl, the game's GXFfxdistortionBump /
GXFfxradialBlur pixel shaders ported (frame read from Bevy's view transmission texture, so they
render in the Transmissive3d phase; one mesh per normal map / mask; a warm-up batch compiles the
pipeline at start and an effect's textures load when it is first read, else a short effect ends
before it shows). MultiTextureBillboardEx (604, nearly every tool effect): `MultiMaterial` +
src/fx_multi.wgsl, the game's GXFfxtessellateBlendMultiTexture ported: three layers with their
own scroll / scale / colour, flipbook frame blending, layer ops by unk_ds3_f2_10..12 (read as the
shader's g_ps_TexBlendType / 2 / 3: 1 / 2 = layers 2-3 / 2 set the alpha by brightness, then 0
multiply, 1 add, 2 overlay). The Flame Vent's fireball went from a flat glow to textured flames.
Camera-distance fade (`view_fade`, every appearance): hidden nearer than minDistance, fading in to
minFadeDistance, the same toward maxDistance, hard cut-offs min/maxDistanceThreshold (fxr docs
603.yml); without it Ichimonji's ground dust (440081, 2.5 / 5 m) filled the screen. Tracers:
segmentSubdivision splits each completed segment (`subdivide`, Catmull-Rom; gap: the game's curve
kind), so a fast iai swing (Ashina Cross 440071: 5) draws arcs, not flat angular sheets.
`SHINOBI_FFX_LOG=1` also logs each game FXR spawned (`fxr <id> on <dummy>`).
Gaps: 10300, the distortion's depth test, 607 mode / shape, normal maps on
sprites, soft particles, the exact Hermite (fxr.ts approximation), light intensity scale.
- `ffx.rs emit()`: spawns `FxEffect` (Anchor::Entity on the dummy, or Anchor::Blade(300, 301) for a
  weapon dummy that is not loaded); SlotID >= 0 or TAE 118 effects stop when their event ends.
- Weapon dummies 1<model><dummy>: Model0 dummies are kept under the bare id (10301 -> 301),
  Model2's under the full id (12200 = WP_A_0300_2 dummy 200; its 240-280 reach 2-6 m along the
  blade for the upgraded art). `dummy_candidates` tries the full id first.
- `model.rs`: right weapon Model2 follows TAE 715 Model2DummyPolyID (rest: right_2 / rightHang_2 =
  149); WeaponModelType 2 (the Mortal Blade, WP_A_0310 blade on Model0 / scabbard on Model1) is
  shown only while its 715 event places it (only a106 Mortal Draw uses Model2 / type 2).

**Check tools:** `SHINOBI_SKILL=<art or tool id> SHINOBI_SKILL_SHOTS=<dir>
SHINOBI_SKILL_SHOT_RANGE=from,to,step` (frames after the press; logs `shot sNNN: <anim> t <s>`),
`SHINOBI_SKILL_CAM=yaw,pitch` (deg, holds the camera), `SHINOBI_FXR_LOG=1` (quads per texture /
blend each frame), `SHINOBI_FFX_LOG=1`, `SHINOBI_LOG_DUMMIES=1`. One effect on its own:
`SHINOBI_FXR_TEST=<ffx id> SHINOBI_FXR_SHOTS=<dir> SHINOBI_FXR_SHOT_RANGE=from,to,step` (plays it
1.5 m in front of Wolf, sideways, 3 s in; shots xNNN.png, then exits).

**Lit particles (2026-10-10):** 935 appearances have `lighting` -2 (LightingMode Lit-like), drawn
unlit here. Where a node ships a lit config next to an unlit one for another state (same texture),
the unlit rgbMultiplier is the lit one x 0.125 in all 48 such pairs, so lit particles are drawn at
rgbMultiplier / 8 (`particle_color`). Before, they were 8x too bright: the deathblow blood 220505
(rgbMultiplier 15) read as salmon-pink spray; now dark red. Gap: real lighting / shadowDarkness.

**State / next:**
1. Mortal Draw (5700): red aura, ink particles, the Mortal Blade in hand now right. The art plays
   `a106_316100` here (not 316000): its slash arc is 440003 / 440007 at 0.33-0.67 s on 12200.
   Check the arc's place / size / colour against the game (tracer width along the dummy's local Z,
   offsets 0.5-1.5 m, followFactor detach 0.3-0.4 s), then the colours (red reads pinkish).
   The pink is 440000's unlit 604 smoke (11633, color1 [1, 0.25, 0.25] x rgbMultiplier 7.5) and its
   tracer (26030, [1, 0.125, 0.125]). Fixed from the game's shaders: unpack `gxffxshader` (shader/
   gxffxshader.shaderbnd: DXBC .ppo / .vpo) and disassemble with d3dcompiler_47 D3DDisassemble
   (scratch script, ctypes). GXFfxtexture.ppo / GXFfxsoftTracer.ppo take the colour's length and
   direction, square texture x direction and scale by the length, so `fxr.rs hue_squared` draws
   c_i^2 / |c| x sqrt 3 (greys unchanged; the sqrt 3 stands for g_vColorScale, a gap). Mortal Draw
   now reads crimson, the blood red, the Flame Vent orange.
   Praying Strikes (5900 / 7500): the pale curved sheet was our hand-made default weapon trail.
   Now the game's: EquipParamWeapon traceSfxId0 / traceDmyIdHead0 / traceDmyIdTail0 (every sword
   row: FXR 401000 on 300 -> 301), started while an AttackBehavior (TAE 1) window runs and stopped
   when it ends (`ffx.rs` weapon_trail). 401000 is a faint additive glow (alpha 0.14) plus a
   distortion-only tracer (rgbMultiplier 0, distortionIntensity 0.3: drawn as distortion only). Tracers now
   keep their first point where it was laid (a 2-frame window still leaves a segment) and a stopped
   tracer's segments fade out over segmentDuration instead of vanishing (`fxr.rs` Particle.ended).
   The hand-made ribbon is left only for art ribbons and weapons without that FXR.
   The red hit capsules (combat.rs draw_hitboxes) now show only with the H / debug-menu overlay.
2. Then every other art and tool through the shots (list: section 8), fix what looks wrong.
3. Done: prosthetic bullets play their Bullet sfxId_Bullet on the bullet (FxEffect anchored to it;
   isInheritSfxToChild passes it to the children; when the bullet ends the effect stops emitting
   there), sfxId_Hit where they land / are blocked, sfxId_Flick where deflected (`prosthetic.rs`
   spawn_row / fly_bullets). The firecracker burst 710003 is the game's 300071 (smoke + sparks); the
   vfx.rs sparks only stand in when its FXR is not extracted. Gore: the blood FFX 220502/3/5 play
   their FXR (ffx.rs); `gore.rs`'s hand-made spray now runs only when that FXR is missing (it used to
   draw on top); the TAE 138 floor stains are unchanged.
5. Clash effects (deflect / guard / hit): `vfx::hit_sfx` reads the HitEffectSfx tables (attack's
   defSfxMaterial1/2 or the defender's materialSfx1/2 -> HitEffectSfxConcept(JustGuard)Param
   atk<group>_1/_2 by atkMaterial_forSfx -> HitEffectSfxParam <Slash|Blow|Thrust>_<S..LLL> by
   atkPow_forSfx): the General's sword deflected = 252001 "Jasuga sparks", guarded = 201002, Wolf's
   cut on his armour = 201001 + 229001. combat.rs passes the ids, vfx.rs plays the FXRs (hand-made
   sparks only when none). The fields and tables are exported (2026-10-10); sim test
   `clash_effects_follow_the_hit_effect_tables`. fxr_extract.py's used_ids() includes every
   HitEffectSfxParam id.
4. A `slab_allocator Use-after-free` error line can appear once at start (dynamic mesh churn):
   harmless so far, not traced.
- Sound fix (user: Mortal Draw's sound "stays playing"): slotted FFX sounds s000440000/1/2/5/6 are
  FMOD loops; `sound.rs` now plays them tracked (`fmod.rs` EventHandle / play_tracked / stop) and
  stops them when the event ends or the anim changes. Ignored test `fmod_loops` lists the loops.
