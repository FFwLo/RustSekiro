# Sv1 handoff: everything a new model needs to continue (updated 2026-10-10)

Read this first, then `docs/HANDOFF_PLAN.md` (file map, commands, tools, task packets) and only
the `docs/kb/<topic>.md` file for the topic you work on. `docs/PROGRESS.md` is the short overview.

**2026-10-12: every boss in `extracted/enemies/npc_names.json` belongs to an outside contributor;
see section 0.1 before touching any boss fight.**

**Where to start (2026-10-11):** `docs/PLAN.md`, the approved plan to finish the game (phases A
stabilise, B fidelity, C playable game; one column per session), then section 0 (what the last
sessions did). Section 9 (FXR effects), 8 (skills: all done, see its end) and 7 (open bugs: only
the cape) are the detail behind the plan's B items. Released: v0.2.0 and v0.2.1 (both
2026-10-11, GitHub releases with `dist/Sv1-v<version>-windows.zip` from `tools/package.ps1`;
v0.2.1 = the packaging fixes, the cloud sea, the cloth fixes and the three sessions' B work).

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
- **After push 60c08dd (committed and released as v0.2.0 on 2026-10-11):** the game's own effects (FXR player `src/fxr.rs`,
  section 9), Mortal Draw's sound no longer loops forever (slotted FFX sounds stop with their
  event), the Mortal Blade model + its effect dummies (TAE 715 WeaponModelType 2 / Model2).
- **The game's own map** (section 4.8, kb/map.md): the Outskirts gate around the General from
  the MSB, map pieces, hit collision and draw params; `config.toml [world] map`.

## 0.1 The bosses belong to an outside contributor (2026-10-12)
The user: "exclude these bosses name list From the work because i gave it to someone", pointing at
`extracted/enemies/npc_names.json` "rows": all 57 boss and mini-boss NpcParam rows (generals,
mini-bosses and every main boss). The Claude sessions no longer work on those fights: their
scripts, phases, arenas, per-fight lighting, deathblows, clip checks and boss-only effects.
They keep the regular enemies, even when one shares a chr with a boss (c1020, c1010, c1040,
c1150), and the shared engine (combat.rs, the AI runner, the script interpreter, the renderer
and the exporter). Code already written for those fights stays as it is. The contributor's
per-boss status sheet: `docs/reports/bosses-handover.md`. Sessions renamed the same day:
"bug fixes" (was Map), "VFX and Maps" (was VXf), "Bosees" (was Bosses).

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

## 3. State at handoff
- **Tests:** `cargo test --release` passes 198 tests (33 ignored) on 2026-10-11 (older counts
  below in this file are from their day).
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
  `lockCamAdjustRot_MaxX/Y` small manual offsets: no reader found in the ChrCam update
  (FUN_14073c260) or the other CameraParam getter callers.
- **Earlier this session:**
  - floor clamp of the camera arm (the camera went under the one-sided floor, which made enemies
    look like they floated);
  - yaw chase held while Wolf is horizontally over the target.
- **Camera position chase (2026-10-11, `camera::chase_position`):** the targetChaseRate*
  params move the camera position (+0x100) toward the orbit point (+0xf0), not the target
  point. Locked on 0.4 / 0.3 a frame; free 0.1 / 0.4 with a 10-20° dead zone
  (targetChaseRateAngMin/Max); the distance chases at targetChaseDistRate 0.4. Mouse turning
  snaps it for rotCtrlEndWaitTime 2 s, then releases over 1 s. Test
  `camera_position_chases_as_the_exe`. Gaps: per-frame rates taken at 60 fps; the pitch-limit
  check; the wall rates. The fulcrum yaw resync: ported 2026-10-11 (see kb/camera.md).
- **TAE 151 look limits ease back** after the event (FUN_140742230: pitch limits chase the
  default range at lockCamParamLerpRate) instead of clearing at once. Open: the exe centres
  the cone on the dummy matrix's -row 2; we still use the Wolf -> dummy direction.
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
  - sound placement at the TAE event's DummyPoly: done 2026-10-11 (kb/sound.md);
  - map reverb: done 2026-10-11 (mixer.fev ntc_rev, section 4.14);
  - the global param `sd_dimension` is not set.
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
  in front of him; 83 fps (radius 60, LOD 20/40 m, far pieces cast no shadow, 2 cascades). `cargo test --release map::tests` checks the
  spawn floors, the gate posts (walls) and the camera rays on the exported collision.
- Lighting ("fix the gate lighting to match the real game", 2026-10-10): the area's own GI
  probe cube maps (`envmap_00..05.tpf` -> `gilm0380`, the gate's `Env_Box380`) feed an
  `EnvironmentMapLight` + `Skybox`, the light set's sun / fill turned by the arena yaw, hour 18
  (dusk, the first-visit look). New extractor files: `cubemap.rs` (BC6H via bcdec_rs, arena
  resample, irradiance, RGBA16F cube DDS), MSB regions in `msb.rs`, `tpf` command. Details and
  the variant-hour mapping in kb/map.md. Same day: the game's colour-grading LUT
  (`src/grading.rs` + `grading_lut.wgsl`, a post-process after tonemapping; the exporter writes
  `map_<id>_lut_<n>.dds`, the draw params pick the id per hour) and Yebis auto exposure
  (`map.rs auto_exposure`: Bevy `AutoExposure` with a compensation curve from the Tone Map
  group, metering offset measured at 18 h; night capped at +2 EV, judged). Then: probe blend
  between hour variants (RGBA16F DDS lerp at load), the shadow cost (far pieces cast none, 2
  cascades / 60 m: 57 -> 83 fps), and the layered map materials (`MapMaterial`,
  map_material.wgsl: overlay / snow / layer C blended by the vertex blend bytes, bin version
  3). Knobs and details in kb/map.md.
- Sandbox map (2026-10-10, `src/sandbox.rs`, `[world] map = "sandbox"`): a walled 90 m floor
  under the gate's lighting with every one of its 242 materials on a leaning panel (overlay and
  snow gradients across each), a line-up of enemies (`SHINOBI_SANDBOX_ENEMIES=all:<page>`,
  `all:*` or a chr list, spaced by hitRadius, standing still), a screenshot catalog of the
  line-up (`SHINOBI_SANDBOX_SHOTS=<dir>`; all 102 kinds reviewed, textures fine) and `[` / `]`
  to walk the hour on any map. Details and knobs in kb/map.md "Sandbox".
- Cloth performance (2026-10-10, `src/cloth.rs`, kb/visuals.md "Cloth cost"): 20 enemies went
  22 -> 48 fps. An env var read per vertex, a blow-up check that restarted big characters
  every frame (now relative to the cloth's reach, giving up to rigid after 5 frames), the sim
  parallel over characters (`simulate` -> `ClothOut` -> serial `apply`), chunked skinning,
  `SHINOBI_CLOTH_RANGE` (15 m) culling.
- Not yet verified in play: stairs, low ceilings vs the camera. Seen in the sandbox: the snow
  layer blows out white at 0 h.

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
- Every-phase check `all_enemies_full`: 86 / 102 (fixes: grab re-trigger, grab release, AI
  requests in the anim set; then GetDist minus the own radius (exe thunk 1405e2c10), same-frame
  re-plan, held single-pose AI moves (c1500 ground zombie grabs now), Todome event message ->
  boss 20200 + Wolf Event7102xx). Details in `docs/reports/enemies.md`.
- Open: the Aging status (c1300). (Monk 20021: done in 4.10; Demon HU1/HU2: identical
  SpEffect rows nobody reads, nothing to do.)
  c1550 "hidden for directing". (Monk clones and c7100 -> c7110: done in 4.10.)

### 4.10 Several enemies at once (2026-10-10; user: "work on multiple enemies at once")
- **Kinds:** `Combat.kinds: Vec<EnemyKind>` (foe rows + CharData per chr / NpcParam row); an
  enemy's `Actor.kind` indexes it. Use `combat.data_of(&a)`, `combat.foe_of(&a)` and
  `combat.npc(&a)`. `enemy0()` / `foe0()` are only for single-enemy tools and tests. `AnimLib`
  has one lib per kind, and FMOD loads every kind's bank.
- **Config:** `[enemy] group = [{ chr = "c1010", at = [3.0, -6.0] }, { chr = "c1020", npc_row =
  10200010 }]` adds enemies; without `at` they stand in a row beside the first.
- **Fight:**
  * Enemies never hit each other: same side = friendly; AtkParam friendlyTarget rows are for
    objects. gap: NpcParam teamType hostility between NPC teams.
  * Each enemy has its own HUD bars and stealth mark.
  * Lock-on skips dead enemies. **Switch (exe LockTgtManImp):** a frame's mouse movement > 50
    switches (FUN_1409ca120, < 25 re-arms, 0.5 s cooldown per switch, so holding repeats); the
    new target is the best on screen by cos(angle to the movement) / distance within 90 deg
    (FUN_1409ca480); candidates within the camera front 60 deg (8 m) / 45 deg (200 m),
    30 deg up / down. Stick: 0.95 fires, 0.5 re-arms (FUN_1409ca2d0; no pad here).
    gap: mouse units (pixels a frame) and axis signs: the exe negates both mouse axes but
    the mouse state's writer is not found (kb/camera.md 2026-10-11).
  * A deathblow takes the locked-on enemy, else the nearest. Throw rows, bullets, statuses and
    hit sounds are per kind.
- **Boss events** (`tools/boss_events.py`, new actions; `src/enemy/events.rs`):
  * "enable" spawns another character. Its MSB position is relative to the boss part: the
    arena is built around the boss, so it lands at start + (-x, 0, z) with yaw pi - d.
  * "disable" removes the boss; "warp_player" is the cutscene's warp point for Wolf.
  * A wait after the bars wait (Wolf alive / the cutscene's end) continues the rule.
  * Genichiro 71001000 at 0 bars (m11_01 11115820): after his death he is replaced by the Way of
    Tomoe c7110 71100000, and Wolf is warped. Tomoe 71101000 at 0 bars brings Isshin c5400.
  * Corrupted Monk at 2 bars (12505961): her three phantoms c5005 50050000 are parked (hidden, no
    AI). Her event 12505970 calls them while she has SpEffect 5031 (Attack3032 / 3033, TAE 66
    AddSpEffect_Multiplayer, now read by `sp_effects_at`): after 1 / 2 / 3.5 s (+1 s when she
    has event message 50), each warps with AI command 10 / 20 / 30 (slot 1) + 10 (slot 0).
    They leave on their own message 70 or when 5031 ends.
  * **Event messages = TAE 231** (template name SetEzStateRequestID; DS3 SetEventMessageID).
    exe: decomp/140b5.c `case 0xe7` stores the id in the character and nothing clears it, so
    it sticks (`Enemy::msg`): her 3032 sends 50 with 5031 at 1.733 s, the a200 attacks send
    70 near their end and 2147483647 0.1 s later.
  * **Warp points:** boss_events.py now reads the event's In/Outside Area checks
    (3[2] / 0[0] / 1000[101] GOTO / labels) and the MSB box regions (shape @0x10, shape data
    @0x48: width, depth, height) into `areas` / `regions` / `groups`; one point per random flag
    (12505973-8, shared by the three, re-rolled when one leaves). Strips 2502856-9 cross the
    bridge: Wolf in 1 / 3 / 4 -> points 2502860-2, in 2 or none -> 2502863-5.
  * **Fight reset** (`enemy::FightReset`): R, Wolf's death and the debug menu's Reset remove
    every enemy and bring back the starting line-up (Genichiro again after Tomoe, the Monk
    without clones); so does the death of a character a boss event brought in (`FromEvent`).
  * Monk at 0 bars without Wolf's Todome message (m25 12505964: flag 12505954 off, 3500010
    off): forced anim 20021 instead of 20200 (`Enemy::fallback_death`; rules now carry
    `needs_flags_off`).
  * Heights and region heights count on the boss's own arena (events::place, script.rs
    shape_contains). A phantom vanishes by its a200 attacks' TAE 193 opacity keys (fade to 0 as
    message 70 goes out; Actor::opacity); command 0 (slot 1) is only 500005 Act40, Wait 1 s.
    No EMEVD display change (12505970 decoded).
- Tests: `group_of_different_enemies_fights_together`, `genichiro_becomes_way_of_tomoe` (with
  the reset), `monk_phantoms_come_and_go` (both warp groups, message 50 / 70); the test helpers
  skip phantoms (`Without<Phantom>`), `the_corrupted_monk_without_the_todome_message_plays_20021`.
  148 pass; all_enemies_full 86/102.

### 4.11 The rest of the bosses (2026-10-11; user: "what left from the bosses ?" -> "do all of them")
- **Corrections to the old list:** c5200 is the Divine Dragon (the Headless is c1350 and
  passes); c5300 is the Old Dragons of the Tree (Okina dragon group), not the Divine Dragon;
  the Divine Child c7300 is a friendly NPC, never fought.
- **Boss scripts** (`src/enemy/script.rs`, `tools/boss_scripts.py`): Shoot Bullet (2003[5],
  `enemy_bullet.rs ScriptShots`), the Divine Dragon fight (m25 52000000: Lightning Reversal on
  its lightning, the collapse 20002 -> loop 21000 via `enemy.rs loop_after`, the finisher
  window TAE 932 unk0 104/105 `data.rs finisher_open`, ThrowParam 15200090 from dummy 230,
  defeat flag 12505880), NPC part groups (env(1120) from the hit capsule).
- **Cast:** characters a script ever enables get `(Scripted(id), Cast)`; the checks' main
  enemy is `Without<Cast>`. `hand_over` in sim_tests swaps the main enemy to a living cast
  member with a script of its own when the boss dies or is disabled (Butterfly 1000800 ->
  1000810, Genichiro -> Tomoe). all_enemies_full now also runs every boss script row that is
  not a chr's default and not brought in by another script.
- **AI:** `IsEventFlag` returns a boolean (a number 0 is true in Lua: c5300 only circled);
  `Snapshot.event_flags` = the script's flags on. **Anim-set overwrite:** a TAE-added anim-set
  SpEffect (200030-200034, all spCategory 100, effectEndurance -1) replaces the resident one of
  the same category (Paramdex SDT spCategory "decides overwriting"; c5300 looped 20001).
- **Monk clones fade** (TAE 193 SetOpacityKeyframe -> `Actor::opacity`, `model.rs
  update_opacity`; spawn takes its a{group}00_000000 idle's value). gap: the hold between
  keyframes is assumed. Test `monk_phantoms_fade_in_and_out`.
- **Team hostility** (`combat.rs teams_hostile / team_of`): 6 vs 24 (invaders vs Ashina, by
  NpcParam row names), 29 (Red Ogre "indiscriminate") vs all but 0/26/30. NPCs of hostile
  teams hurt each other and target the nearest one within eye_dist_normal when Wolf is
  farther (`enemy.rs Rival`). gap: the exe's team table. Test `the_invaders_fight_ashina`.
- **Real arenas per boss** (`tools/boss_arenas.py`, details `docs/kb/map.md`): each boss
  script row's map around its MSB entity -> `extracted/map_boss_<row>.*`, index
  `extracted/map_boss_arenas.json`; `src/map.rs boss_arena` picks it when `[world] map` is on
  (not the sandbox; `SHINOBI_MAP` still overrides). The exporter now also takes map objects
  (MSB part type 1, `obj/*.objbnd`). Cast positions keep their height there
  (`events::place`, `map::on_boss_arena`).
- **Still not a fight here:** the Great Serpent c5010 (hp 9999): a set piece driven by MSB
  regions (`IsInsideTargetRegion`, 501010/501020/501030_battle.lua) and its fall throws
  15010030-32; ai.rs answers region checks false. c1300's Aging status is open.
- Tests: 160 pass; all_enemies_full 94/110 (the 16: merchants, nuns, handmaidens, Hanbei,
  c1260, c1320, c1460, c1550, c5410, c7200, c7300, c1300 Aging, c5300:53000000).

### 4.12 Plan phase A, Map session (2026-10-11; `docs/PLAN.md`)
- **Startup `bevy_render slab_allocator Use-after-free` error: fixed** (was "not traced"). Bevy
  0.19's mesh allocator allocates nothing for a mesh with no vertices but still copies into it
  and logs that error. Two sources: the placeholder capsule of `player_visuals` /
  `enemy_visuals` was created and removed by `build_skeletons` in the same frame (freed slots
  reused by the model's meshes at once) and `ffx.rs` spawned the weapon trail mesh empty for one
  frame. Now the visuals run after `AnimSet` with `Without<Skeleton>` (world.rs, enemy.rs,
  player.rs; anim.rs lost its `.after(..._visuals)`), `attach_models` / tool meshes skip empty
  meshes, the trail is born with its idle triangle. Rule: never let an empty mesh reach a frame.
  `SHINOBI_MESH_EVENTS=1` (map.rs) logs mesh asset events of the first 600 frames.
- **Cloth restart spam (c1060_mino1, c1100_sleeve every frame): fixed** in the exporter
  (`cloth_export.rs`: the hclMoveParticlesOperator of the container's first sim cloth only, pairs
  beyond the particle count dropped; the second sim cloth of such containers is still not
  exported, gap) and guarded at load (`cloth.rs`). The teleport threshold for fixed particles
  scales with the cloth's extent (the Guardian Ape's fur restarted every few frames at 1 m).
  Re-export the models (extract.ps1 does) to get the fixed `.cloth.json`.
- **Release pipeline:** `tools/extract.ps1` now runs every export a fresh install needs (decals,
  HUD sprites, boss scripts, tool models, boss arenas with `-SkipArenas`, the two-pass map export
  with objects); `tools/package.ps1` builds `dist/Sv1-v<version>/` and the zip. Python 3 is
  required for a release install, Node optional (README).
- The Chained Ogre's fur particle 8 blow-up (every ~2.5 s) was an empty constraint order in
  the game data, fixed in 4.17. `SHINOBI_SANDBOX_WATCH=<sec>` logs the AI mode and every actor once.
- **Boss arena start (VXf's report, camera inside Wolf's face):** the config's c5100 loads the
  Guardian Ape's arena, where the arena's player spot (0, 4) is under the grotto rock (1.5 m of
  headroom) and a material-less helper plane (h900702, 0.4 m over the pond floor) was the floor.
  `map::player_start()` walks +z to a spot with 2.8 m of headroom (used by the player.rs reset
  and `settle_player_start`), level material-less helper triangles are dropped at load, and
  `MAP_LOG=1` on the map export names every hit part with its vertex range (kb/map.md).
- **Map surfaces were inside out (the "white sheet" under Wolf on the Ape arena): fixed.**
  `BinBatch::indices` swapped the winding after the X mirror; FLVER triangles only need the
  mirror. Open floors were back-face culled (the Ape's pond bed), closed pieces looked right
  from their far walls. With it: LOD face sets never applied (flags matched as 1/2, now
  0x01000000/0x02000000), strip triangulation restarts, CullBackfaces -> two-sided, mirroring
  scales, a missing texture no longer blanks its batches, and the Guardian Ape pedestal's
  far-view shell needs no special case (it is a correctly wound shell seen from inside). The
  eight kept arenas (see the next bullet) are all re-exported with the fixed exporter (the old
  bins had inverted strips, which lost floors such as the Fountainhead bridge deck once the
  loader stopped swapping). Re-export one with `tools/boss_arenas.py <map>` after deleting the
  bin. Knobs: kb/map.md "Debug knobs". `tools/boss_arenas.py` survives a cp1252 console.
- **Arenas (the user, 2026-10-11):** own arenas only for the Corrupted Monk, Lady Butterfly,
  the two Apes and the Divine Dragon fights; every other boss fights on Isshin the Sword
  Saint's flower field (`boss_54000000`); Genichiro, Way of Tomoe, the Owl (moved onto Genichiro's), Emma and both Isshins
  (Sword Saint, Ashina) fight on the castle top (kb/map.md "Castle top"). `tools/boss_arenas.py` OWN_ROWS / SHARED_ROW / MOVED; exporter `MAP_SHADOW_BELOW` keeps the castle top at 66 fps (kb/map.md "Frame rate"); per-fight draw param variants from the castle's events (`MAP_GPARAM_EXTRA`, map.rs `FIGHT_LIGHTING`, kb/map.md "Castle top lighting per fight"; the look is still flatter and cooler than the game's: the next B4 step),
  `extracted/map_boss_arenas.json`, the other bins deleted (kb/map.md "Which bosses get an
  arena"). `map::switch_map` changes the map on a SpawnEncounter without a relaunch
  (`map::loaded_map()`), `player_start()` on a stranger's arena also checks the line-up spot
  8 m ahead (Bosses' requests). Tests: 182 pass (29 ignored).
- **Start spot on slopes (2026-10-11):** `player_start()` follows the floor from the enemy's
  spot (0, -4) instead of reading it at a fixed 1 m: the Fountainhead bridge (boss_50001000)
  rises 1.7 m by Wolf's spot and he stood under its deck. The Headless Ape's MSB pose faces
  the pit's wall, so `tools/boss_arenas.py` OVERRIDES turns that arena (exporter
  `MAP_CENTRE_YAW`); textures of another area are unpacked on the exporter's "missing"
  report (m13_ground_900 there). `SHINOBI_START=x,z` is a probe knob. The Divine Dragon
  arena (52000000/53000000) is the cloud floor h003100 (the only collision near y 0; the
  palace is 280 m below) with 9 pieces: a white cloud plane (the cloud shader is a stand-in)
  and the Dragon's tree object round its MSB spot, which drew as grey shards until 4.17
  (its blossoms had no texture: a shader-variant slot-name mismatch). The fight stands 24 m ahead of
  that spot on open cloud (exporter `MAP_CENTRE_DIST`); the Dragon is a scripted set piece
  (plan: non-fighters only stand), so the shards wait (kb/map.md "Wolf's start on a boss
  arena"). Tests: 185 pass (29 ignored).

### 4.13 Plan B3 / B6, VXf session (2026-10-11; `docs/PLAN.md`)
- Camera (B3): position chase, TAE 151 look-limit ease-back, lock-switch notes: section 4.2 and
  `docs/kb/camera.md`. Burn / poison exe model: section 8 (status.rs).
- **Grappling hook, player move (B7)**: key G / pad L2. Shoot, move start, launch with
  guaranteed arrival, then land on the point. The HUD marker, rope line, enemy targets and WireHit
  are in, and the Grappling Hook Attack (attack in the wire fall at a hooked Ogre / Gyoubu:
  511600 -> landing 511610, a deathblow: one life bar off; Gyoubu's Todome 511690 on his last
  bar). Details and gaps in `docs/kb/grapple.md` section 6. The rope is the game's wire model (WP_A_9500) stretched to the hook. Open: the 0.6 m perch
  stays only as a fallback for the 29 of 113 gate spots still without object collision.
- **Chain timing (user goal: fluid deflect / art / tool flow)**: deflect -> attack / art / tool,
  tool -> art / guard, art -> attack / tool / guard and attack -> guard / art / tool all fire the
  tick after the TAE execute flag opens (the game's own one-frame read lag). Test
  `deflects_arts_and_tools_chain_on_the_cancel_frame`; table in `docs/kb/clips.md` (last section).
- **Air prosthetic end** (player.rs): `AirSubAttackStart` / `AirSubAttackLoop` end into
  `AirSubAttackLoop` for every tool (SubAttack_SM ev 300); the FreeFall fallback follows the
  CMSG animeEndEventType (0 FireNextStateEvent = the state's highest-priority transition,
  FUN_1411e4f20).
- **Todome recovery SpEffects** last until reload (saveCategory -1 = not saved), as the game.
- **Foot dust** (ffx.rs `FootSfx`): TAE 792 FootSfxParam_Entity plays FootSfxParam
  [character row][FootSFXIndex] (FUN_140b54690 -> FUN_1409da4c0 -> FUN_1410be4e0; Wolf's row is
  TentativePlayerParam PlayerFootEffect_bySFX 1, NPCs NpcParam footEffectSfxId): walk 101001,
  run 101011, sprint 101021 ... at the foot dummy, turned as the character
  (IsIgnoreDummyPolyAngle). `tools/fxr_extract.py` now pulls those FXRs (36 more). gap:
  SpawnFFX_ByFloor (112) has no case in the TAE dispatcher FUN_140b59170 and its handler is not
  found; its FFXIDs (0 / 1 / 11 / 111) are read as the same table's columns. gap: no floor
  material part, no footEffectDecalBaseId footprints (FUN_140b9c340), a following 792 stays put.
  Test `foot_dust_from_foot_sfx_param`.
- **Enemy effects** (`tools/fxr_extract.py`): it read only Wolf + the two combat_data enemies; now
  every extracted/enemies/c*.json, and the characters' own effects (6xxxxx) come from the area
  archives sfxbnd_m10-m25 (extract.ps1 unpacks them): 501 -> 1160 effects, 6 ids left missing
  (2 / 3 / 5, 615102, 654061, 2255505: in no archive). Effect models (605) are found by the script
  (diffuse rule in its doc; matches the old hand list), 15 -> 66. gap: an id in several area
  archives is taken from the first (the game loads the current map's).
- **Stealth meter art** (hud.rs): the game's MENU_Find_02 frame + MENU_Find_01 brush triangle
  (SB_FE; export.rs WANTED), filled from the tip up, amber -> solid amber -> red, over the head.
  2026-10-12: size, colours and fades now from the HUD movie (see "FE movie marks" below).
- Not done (gaps written in code): hit-blood floor decals (AtkParam decalBaseId -> DecalParam
  600xxx / 700xxx id formula not traced), the 609 light unit, effect sizes from the gfx ABC.
- **Enemy effect audit** (fxr.rs): every enemy's TAE effects were checked against what the FXR
  player draws. Added NodeTranslation 15 / NodeSpeed 120 / random turns 83 and the rectangle
  emitter 402. Gaps: GPU particles 10000 (c7020 embers, carp, Serpent), the camera
  attach 46. List in `docs/kb/visuals.md` (last entry). Tests
  `node_translation_and_node_speed_move_the_node`, `rectangle_emitter_stays_inside_its_size`.
- **Deathblow sound mix** (`sound::DeathblowMix`, docs/kb/sound.md): deathblows muffle the
  world sound (lowpass ntc_ninsatsu2, 445 Hz) and fade it back over 2 s, as the exe does.
- **Wall hug / ledge hang: researched, parked** (`docs/kb/docking.md`). The button is the game's
  E (DefaultKeyAssignParam row 20), and the HKS start rules, the c0000.hkx docking offsets and
  every hug / hang anim are exported. Parked because bosses don't use it, the gate has one hang
  edge, and most hug edges are on objects without loaded collision.
- Check tool: `SHINOBI_HUD_SHOTS=<dir>[,<every s>[,<count>]]` saves the game's own frames (HUD
  included) and quits; never screen-grab the desktop (tools/screenshot.ps1 catches whatever
  window is in front when the game is not up yet).

### 4.14 Plan B1, Bosses session (2026-10-11; `docs/PLAN.md`)
- **Hit shapes:** every AtkParam hit shape is used (hit0..hit15; DmyPoly2 -1 = a sphere), from
  `"hits"` in the export (`data.rs Attack::shapes`, combat.rs). Hurt capsules were already the
  game's ragdoll bodies. `model.rs GeometryPlugin` gives headless tests the real dummies and
  capsules (`sim_tests::app_geom`); reach table in `kb/damage.md`.
- **Deathblows:** boss HP refill after a non-final deathblow comes from the ThrowDef's own TAE
  (SpEffect 300610 at its frame). Enemy grabs read ThrowKindParam (exported in combat_data now).
  The absorbed character's model slides into place over ThrowParam
  adsrobModelPosInterpolationTime (`anim.rs ModelLag`); Ochimusha behind deathblow re-checked
  against the live recording with real geometry (matches; `kb/damage.md`).
- **Knockback:** Wolf bounces back off enemies with a knockback cut (TAE flag 72 KnockbackValue,
  `combat.rs recoil`; c1180 75 %).
- **Explained:** the "x2.9" hits are the normal ones; the armored club samurai 11807101 cuts
  them (x0.35 HP / x0.5 posture; rule not found, `kb/damage.md`). TAE flag 73 is the
  opponents' ParryTiming gate in the exe (flag 63 sends the notify); our flinch gate keeps it as
  a proxy (`kb/guard-deflect.md`).
- Open in B1: the clip comparison of the reach table (needs Medal clips). (hitSourceType /
  shape priority, the knockback facing factor and the armor cut: done 2026-10-12, 4.22.)
- **C2 encounter swap (data side):** `enemy::SpawnEncounter { chr, npc_row, group }` swaps the
  fight without a relaunch (`Combat::load_encounter`, `AnimLib::sync_kinds`, then FightReset);
  `data::encounter_list()` lists every exported chr + placed row (boss = has a boss script).
  `enemy::AutoRestart` (default on) is the old 5 s restart after a boss defeat; the menu turns
  it off. Fixed on the way: enemies respawned by a fight reset had no skeleton / model
  (build_skeletons / attach_models now also run in Update, `model::ModelTried`). Gaps: the map
  stays (boss arena chosen at startup: asked Map), FMOD banks / effects of a new chr (VXf).
  Names: `Encounter.name` is the game's health-bar text (`tools/npc_names.py` ->
  extracted/enemies/npc_names.json, bosses and mini-bosses; `Encounter.bar`). On another boss's
  arena (`enemy::on_own_arena()` false) the line-up keeps 8 m in front of `map::player_start()`
  (Map asked to make that spot open for both).
- **B5 enemy bullets:** flight as the exe's bullet update (homing turn rate, in/out-of-range
  acceleration and gravity, min / max speed; `enemy_bullet::steer` / `flight`) and the map
  collision (isPenetrateMap). Details and gaps: `kb/enemy.md`. teamType: the exe's team table
  is not found; the rule stays the row names' (`combat::teams_hostile`, gap).
- **B5 hearing, sight, harness:** the listener side of hearing (ear_dist, ear_soundcut_dist,
  the listener's hearingSearchEnemyCut; `enemy::hearing_rate`), the sight ray through the map
  (`stealth::sight_clear`). c1550's roster row is now a fighting one (rows with resident 220600
  "sight 0 hearing 0" are passed over; 15500202). `all_enemies_smoke` 102/102 with the
  non-fighter "stands" rule (`NON_FIGHTERS`; Wolf walks in on an enemy that moved > 5 m, a grab
  counts as a hit); `all_enemies_full` 95/110 (the 15 non-fighters).

### 4.14 Plan C1 screens / C2 picker / C3 options / C4 gamepad, VXf (2026-10-11)
- **Screens** (`src/menu.rs`, Bevy `States` `Screen`): Title (Start / Fight / Quit), Playing,
  Paused (Esc: Resume / Retry / Title / Quit), Dead (死: Retry / Title, when Wolf lies in
  GroundDeathLoop_F with no resurrection left), Won (忍殺: Next / Retry / Title, WON_HOLD 4 s
  after BossScript `run.defeated`, or every enemy dead in a fight without a boss script), Picker.
  Outside Playing `Time<Virtual>` is paused (after a 1 s warm-up on the first title so the scene
  settles) and the menu eats keys / mouse before the game reads them. The title and picker hide
  every other UI layer (`hide_hud_on_title`, restores each layer's own visibility).
- **Art:** the game's brush marks from menu/hi/01_common (`sekiro-extract` export_hud WANTED):
  MENU_Death 死 and MENU_Ninsatsu 忍殺 (SB_TextEffect, tinted red), MENU_Title_SEKIRO 隻狼 (SB_Title,
  white-on-black: its brightness is made the alpha at export) with MENU_Shadows / MENU_DieTwice.
  Plain text stands in when they are not extracted. Gaps: sizes / places by eye (menu movies
  not decoded), 死 / 忍殺 timings, WON_HOLD.
- **Picker:** tabs Bosses (every row with a boss / mini-boss health bar, `Encounter.bar`: 57) /
  Enemies (one row per other chr; rows named "dummy" left out), 10 rows around the cursor, red
  dots = deathblows. Enter / click writes `SpawnEncounter` and resets Wolf; Won → Next takes the
  following row. Names: the bars' own NPC名 text (Bosses' tools/npc_names.py).
- **Sound of a swapped-in enemy:** `fmod.rs load_new_kinds` loads `<chr>.fev` when Combat
  changes. extracted/sound has every chr bank (extract.ps1 unpacks `sound/(s?main|c[0-9]+)\.`,
  173 MB).
- **Spawn facing:** Wolf faced away from the line-up (yaw π); `player.rs start_yaw()` faces the
  first enemy spot at spawn and Retry. `camera.rs CameraSnap` puts the camera behind him then
  (START_PITCH 15°, gap).
- Checks: `SHINOBI_TITLE=1|paused|dead|won|picker` opens a screen, `SHINOBI_FIGHT=<chr>[:<row>]`
  runs a swap at start, `SHINOBI_HUD_SHOTS=<dir>,<s>,<n>` saves the game's own frames (never
  grab the desktop). Automated runs (other `SHINOBI_*`) start in Playing with AutoRestart on.
- **Options (C3 UI):** Title / Pause -> Options: camera speed 1-10, invert camera up / down,
  gamepad on / off, volume 0-10 (fmod master category 0.7 * v / 10), fullscreen (borderless),
  vsync. Live on change; written to config.toml on leaving (`config::save_keys` / `set_key`
  keep comments). New tables `[audio] volume`, `[video] fullscreen / vsync` with serde defaults.
  Gaps: (camera speed default 5 = exe option byte 0, traced 2026-10-11, see below).
  Done 2026-10-11: Music / Sound effects / Voice volume rows (config [audio] music / se / voice,
  fmod.rs set_mix: the exe's table at 0x14092d489 gives Default, Menu, SE <- Se slot 3, Music <-
  Bgm slot 2, Voice <- Voice slot 4; gap: in-game master / menu slots 1 / 5 / 6 taken as 1.0);
  window size: menu.rs apply_video reads Map's video.resolution (None = 1024 x 560).
- **Gamepad (C4):** `player.rs read_pad` (R1 attack, L1 guard, circle step / sprint, cross jump,
  square item, R2 prosthetic, L3 crouch, triangle / d-pad up next tool; left stick walks below
  0.75 = HKS PRM_RUN_STICK_LEVEL). R1 / L1 / R3 from DefaultKeyAssignParam00 (pad ids beside
  the mouse buttons); the face buttons are the game's default layout (gap: ids 2000-2007 not
  decoded). Right stick: `camera.rs orbit_with_stick`, CameraParam rotSpeed_Min..Max (deg/s) by
  the camera speed option, rising to rotHiSpeed over rotHiSpeedRateIncTime at full tilt (gap:
  traced 2026-10-11, below); locked on it switches targets (FUN_1409ca2d0: re-arm < 0.5, fire > 0.95).
  R3 locks on. Menus: `menu.rs pad_to_menu_keys` (d-pad / stick arrows, cross Enter, circle Esc,
  Start pause) and clears the pad's buttons so a menu press never reaches Wolf. Test
  `gamepad_feeds_pad_input`; not tried on a real pad yet.
- **Wolf's bullets** (prosthetic.rs fly_bullets) now use Bosses' exe bullet update
  (`enemy_bullet::steer` / `flight`: homing at the lock-on target, in / out of range accel and
  gravity, min / max speed) and stop on the map collision unless isPenetrateMap.
- **Hit stop as the exe (2026-10-11, combat.rs start_hit_stop, actor.rs HitStop):** started on
  every contact; the attacker's time scales by the defender's NpcParam hitStopType (0 none on
  most enemies: no more 0.1 s freeze on Wolf's hits), 0.06 s hold then a quadratic ease. See
  kb/guard-deflect.md. From AKJama/sekiro-rs (credited), checked at FUN_140b9cd90.
- **Exit crash fixed (2026-10-12, fmod.rs `impl Drop for Fmod`, src/crash.rs):** every exit ended
  in an access violation inside FMOD's DLL unload (FMOD was never released). Drop now calls
  FMOD_EventSystem_Release before the libraries unload. crash.rs logs any native crash (code,
  module + offset, backtrace) to stderr and sv1.log. Note: a game that vanishes with exit code 1
  and no log was closed by another session's `taskkill //IM sv1.exe`, not crashed.
- **Close camera (2026-10-12, camera.rs cam_fade_step / safe_margin):** Wolf fades out (2.5 / s,
  back at 5 / s) when the camera is under CameraParam playerFadeDist 0.5 m from him, as the exe's
  CamFade; the 0.3 m minimum camera distance is gone (the exe has none) and the exe's safe margin
  (min(10 %, 0.3 m) closer) is added. See kb/camera.md. Gap: the obstacle fade (things between
  the camera and Wolf to alpha 0.2).
- **Map layers: the game's own blend (2026-10-12, bin v5):** the M[Multiple] /
  M[MultipleGround] pixel shaders were decompiled; the exporter writes each material's
  constants and the full vertex blend data, map_material.wgsl `game_layers` blends as the game
  (kb/map.md "The sat layer shaders, decompiled"). Gate map re-exported; older bins load as
  before. Test `bin_v5_reads_the_game_path`. Gaps listed there.
  **Given away 2026-10-12** to another person (state and open items: kb/map.md, end of "The sat
  layer shaders, decompiled").
- **Area battle music (2026-10-12, src/area_music.rs):** regular enemies who find Wolf start the
  map's battle track (common.emevd 410; kb/sound.md "Area battle music"), 2 s hold after combat,
  off during boss music. tools/area_bgm.py -> extracted/enemies/area_bgm.json (re-run on a fresh
  extract). Test `event_410_holds_after_combat`.
- **Character fabric layers (2026-10-12, DetailBlend):** model v6, sekiro_material.wgsl
  `detail_layers`; Wolf's bd_m_9040 and 69 roster models re-exported (kb/visuals.md "DetailBlend").
- **Sound mixers (2026-10-12):** the reverb and the deathblow muffle never reached any sound
  (their chains had no input). fmod.rs now builds the game's mixer graph from multimix.mix and
  feeds every event into its mixer (mixerID_0): SFX get the deathblow muffle + reverb, voices
  reverb only, menu dry (kb/sound.md "Mixer routing"; test fmod_mixer_routing).
- **Effects and menu timing (2026-10-12, bug round 4):** ffx.rs `emit` and gore.rs `bleed` now
  run in FixedUpdate after ActorSet::Advance. They read the per-step TAE edges, so before this
  they fired 2-3x at high fps and missed edges at low fps. `emit` uses sound::sound_windows, so
  Wolf's walk/run foot dust (TAE 792/112 in the locomotion clips) now plays. menu.rs:
  `watch_fight` waits while a reset is pending (Retry no longer bounces to 死), and the picker
  tabs act only on a changed Interaction (a clicked tab no longer sticks).
- **Field music (2026-10-12):** with no fight music due, area_music.rs plays the area's 2690
  outdoor track while Wolf is outside every indoor-check region (flag 9100 / common_func
  20004010; tools/area_bgm.py -> area_bgm.json "indoor"; map.rs `arena_to_map`), and fades
  out indoors. kb/sound.md "Field track". Open: the probe box rule (kb/map.md, centred vs
  standing boxes).
- **Coat shader + cloth normals (2026-10-12):** Character_AN_Blend_AMSN_[Fresnel] (Wolf's coat,
  kimonos) as model v6 variant 100, `fresnel_layers`; DetailBlend tiling now uses the MTD's
  per-texture scales. The coat cloths had zero normals (position-only deform), now rebuilt in
  cloth.rs `rebuild_normals`. Wolf's 5 parts and 73 roster models re-exported (kb/visuals.md
  "Coat shader").
- **FE movie marks (2026-10-12, hud.rs MarkClip / lock_clip / deathblow_clip):** the HUD movie
  menu/01_000_fe.gfx was decoded (SWF tags, not ActionScript): the engine places the clips, the
  look is in their timelines (60 fps, 1080 stage). Lock mark: Lockon_Ani_297 shows the left 32 x
  32 of MENU_Lockon01 untinted; FadeIn 15 frames scale 3 -> 1 and alpha 0 -> 1, then a loop
  pulse 1 -> 0.74 -> 0.99 every 68 frames, FadeOut 5 frames. Deathblow: MENU_ninsatu_02 alone
  (405 px x 0.45), FadeIn 11 frames, hold, pulse 1 -> 0.60 -> 0.98 (ninsatu_01 belongs to the skill
  screen). Stealth: MarkerTop_334, Find_02 28.07 px / Find_01 28.8 px, centre 35.5 px over the
  anchor, state colours normal black .30 / white .70, caution (154, 134, 70) .90 / brush x (.719,
  .629, .238), found (163, 64, 57) .32 / .50; FadeIn 15 frames, FadeOut 22. Checked by shots (the
  found mark is ~17 px visible: Find_02's drawn part is 77 of 128 px). Test
  `marks_follow_the_fe_movie`. Gaps: the anchors (dummy 220 is ours), the gauge masks, the
  caution red layer / shadow, the found highlight.
- **Line 601 (2026-10-12, fxr.rs):** drawn as one-pixel-wide quads (width = 2 tan(fov / 2) /
  viewport rows x distance), colour = the node colour x the mean of start / end colour.
- **Hit effect turn by attack direction: parked (gap, vfx.rs).** HitEffectSfxAngleParam (param
  0x3e) is keyed by the HitEffectSfxParam row; AtkParam atkDir_forSfx (+0x88: 1 up, 2 down, 3
  left, 4 right) picks X / Y / Z Euler angles (rows 200 / 290 / 300 / 310 / 550: up X -75, down X
  +45, left Y +65, right Y -65; 570 / 610 half). The exe has two paths that disagree: FUN_14092b6c0
  replaces the look-at basis with R = Ry Rx Rz (absolute world), the event path FUN_140b63fe0 uses
  Euler(E - A) relative to a character. Not applied until one is pinned. atkDir<Dir> row swaps are
  dead data (only 210 -> 211).
- **Lock-on mark: the game's own (2026-10-11, hud.rs LockMark):** the user asked for "lock in
  small circle like in the game". The sprite is MENU_Lockon01 (menu/hi/01_common.tpf, a
  standalone 64 x 32 BC7 texture, not in any .layout atlas: a ~10 px white disc centred at
  (16, 16)); the FE movie clip Lockon_Ani_297 (menu/01_000_fe.gfx) shows it. export_hud writes
  extracted/hud/MENU_Lockon01.png. Drawn on the locked target's dummy 220 (else root + 1.4 m) at
  texture size on the 1080p stage. Gaps: the anchor (scale / fade decoded 2026-10-12, above). Check
  with `SHINOBI_HUD_SHOTS=<dir>,3,4` (it now locks on to the nearest enemy).
- **Hit-blood stains: done 2026-10-11** (`gore::HitStains`, `gore::hit_stain_id`). The hit
  handler FUN_140888960 (hit record param_2, AtkParam row at +0x54) queues up to four DecalParam
  ids: decalId1 / 2 and, per decalBaseId1 / 2, FUN_14088a7f0's id = 600000000 + the defender's
  material (record +0x1dc = materialSfx1; Wolf 146) * 1000 + base / 10 * 10 + variant (record
  +0x7e, read as atkPow_forSfx 0-4). Rows exist for materials 109 / 114 / 134 / 135 / 146 (700...
  = the same in black, bloodTypeEnable 0, not used). appearDirType 2: projected from the hit along
  the blow at pitchAngle (-30..-60, some +40 up), size nearSize -> farSize over the ray's length.
  combat.rs spawns the marker on each hit. Export: export_decals takes every 600-799 million row,
  ATK_FIELDS gains decalBaseId1/2 (re-run `export` and `npcs`). Gaps: the projection past the
  queue (FUN_140866b10), delayAppearFrame 3, stains on walls (dropped), the old 600010 / 610010 /
  620010 rows (not on this path).
- Attack stains (2026-10-11): the 11 AtkParam rows with their own decalId1 / 2 (c1550 oil pot
  50010 / 50011, c1210 vomit 803021-26, c1360 mucus 803040, c1500 dung 803030 / 31, Wolf's oil
  3520, the "mud dirt" foot rows 800000-5) are exported (ATK_FIELDS, export_decals reads every
  AtkParam row; a smaller diffuse is stretched to the mask's size). An enemy bullet that ends sends
  `gore::PlaceStain` for both ids at its end point; gore.rs lays every stain (also TAE 138) on the
  map's ground under it (map::raycast), not y 0. Gap: the rows' pitchAngle / putVertical (walls) are
  laid flat on the floor. Re-export: `sekiro-extract export extracted` then
  `sekiro-extract npcs extracted c1550 c1210 c1360 c1500`.
- Enemy effects check: every FFX id the 102 exported enemies' TAE 96 / 118 name has its FXR except
  615102 (c1550, c7420/30), 654061 (c5400/30), 2255505 (c1250/60): not in any sfxbnd of the game
  either. TAE 110 "SpawnFFX_110" (ids 2 / 3 / 5, c1320 / c5000 / c7200) is not an FXR id and is not
  played (gap: unnamed in DSAnimStudio's SDT template; its handler FUN_140b56900, dispatcher case
  0x6e, hands a fresh struct to chr +0x1b40 and reads no id in the decompile).
- Camera speed option traced (2026-10-11): options byte 0 (default 5) IS the camera speed:
  FUN_140734790 stores clamp(byte / 10, 0, 1) at camera +0x270; FUN_14073c260 keeps
  lerp(rotSpeed_Min, Max, s^2) / lerp(rotHiSpeed_Min, Max, s^2) at +0x250..+0x264 (CameraParam
  0xd4..0xf0). The fast turn ramp (+0x160) runs only at |stick| > 0.998 with a sideways part,
  +-dt / rotHiSpeedRateIncTime (0xb0) per side. camera.rs stick_rate / orbit_with_stick follow
  it (was: linear over 1-10, > 0.95). Gap left: the menu slider's range.
- Map reverb (B4 sound, 2026-10-11): fmod.rs set_reverb / apply_reverb, mixer.fev ntc_rev with
  sd_reverb_size = the arena's MSB SoundSpaceType (kb/sound.md "Reverb, built"). The existing
  map_*.json got "sound_space_type" from a one-off script reading the same MSB byte (no map
  re-export); the extractor writes it from now on.
- Arenas (user rule, via Map 2026-10-11): own arenas only for the Monk, Butterfly, the two Apes
  and the Dragon; everyone else fights on Isshin's flower field (boss_54000000). The Ape arena's
  white sheet / knee-deep floor was inside-out map triangles, fixed by Map (kb/map.md); no
  SHINOBI_MAP_SKIP needed for it.

### 4.15 Plan B7 grapple points, Map session (2026-10-11; user: "the grapple is needed for many bosses")
- Research in `docs/kb/grapple.md` (sources, WirePointSearchParam / WireVariationParam, the
  player's 24 wire states and flight physics, the enemy SpEffect targets, the three-way split).
- Exporter: `export_grapple` writes json "grapple" (o000100 markers, object `.edge` grapple
  records, map edge binders) for every map; all boss arenas re-exported (`boss_arenas.py
  -force`). Game: `map::GrapplePoints`, `map::grapple_target` (search params cited, gaps
  marked), green/yellow gizmos with H. Details in `docs/kb/map.md` "Grapple points".
- The o000100 "markers" turned out to be item spots (MSB Treasure events; the one 3.7 m from
  the General is entity 1111510, event 11115652): the exporter skips treasure parts and takes
  `.edge` grapple records only. Open: the camera-plane rectangle semantics, the edges'
  reference direction. VXf builds the move (src/wire.rs), Bosses the enemy targets.
- Objects' own collision joins the `.hit` (objbnd hkx convex shapes + compressed meshes, hit
  material from ObjectParam materialId): 84 of the gate map's 113 landing spots have a floor
  now, so Wolf lands on the branch itself. `docs/kb/map.md` "Objects' own collision".

### 4.16 Plan C5 release build, Map session (2026-10-11)
- `main.rs`: `windows_subsystem = "windows"` for optimised builds (no console; `startup`
  attaches the parent console so `sv1.exe > log` from a shell still prints), a second log
  layer to `sv1.log` next to config.toml (`SHINOBI_NO_LOG_FILE=1` off; in .gitignore), a
  panic hook that appends `PANIC ...` to sv1.log and, without a terminal, shows a "Sv1
  stopped" dialog with the extract.bat hint (checked: a missing config.toml launched through
  wscript shows it; through PowerShell/Bash it prints instead).
- `Cargo.toml` `[profile.dist]` (inherits release, fat LTO, codegen-units 1, strip) for
  `tools/package.ps1` only (`cargo build --profile dist`, copies target/dist/sv1.exe);
  `windows-sys` dependency (Console + WindowsAndMessaging). README "From a release" / "From
  source" updated.
- `src/icon.rs` (IconPlugin): the window icon (title bar / task bar) is the HUD's red
  resurrection dot drawn in code (no image file) and set on the winit window once it exists.
  Bevy 0.19 keeps the winit windows in the `bevy::winit::WINIT_WINDOWS` thread-local, not a
  `NonSend` resource (that panics "Non-send data not found"), and the system needs a
  `NonSendMarker` parameter or it runs on a worker thread where the map is empty. `winit`
  0.30 is a direct dependency for `winit::window::Icon` (already in Cargo.lock).
- `build.rs`: the exe's own icon (Explorer, task bar) from the same `src/icon_dot.rs`,
  written as a compiled resource file by hand (RT_ICON 16/24/32/48/64/256 as 32-bit DIBs with
  AND masks + RT_GROUP_ICON, the 32-byte ordinal headers and the empty first entry) into
  OUT_DIR and linked with `cargo:rustc-link-arg-bins` (MSVC link takes `.res` inputs; no
  winres crate, no rc.exe, no .ico file). Checked with .NET `ExtractAssociatedIcon`: 32x32,
  red centre, transparent corner. Non-MSVC targets get no exe icon.
- Not done: a bundled font (hud.rs / menu.rs read C:/Windows/Fonts: VXf's files; Bevy's default font is
  the fallback), `[debug] enabled` gating (the F1 menu is documented in the README and the
  probe plugins do nothing without their SHINOBI_* knobs, so nothing is gated). map.rs
  `walk_bin` / `Terrain::from_hit` refuse a truncated map file with a warning (test
  `truncated_map_files_are_refused`); model.rs / anim.rs / sound.rs (other owners) still
  slice-panic on one, which the dialog now reports.
- Frame rate check 2026-10-11 12:35 (one instance; peers relaunch the game too, so count
  `Get-Process sv1` before measuring): gate map 60 fps at the title, 53 fps in play with the
  Guardian Ape as the opponent (its fur cloth 5.7 ms/frame; the General gave 83 on 10-10); the
  Ape arena 60 at the title. Later that day the fur's reference-vertex pass was fixed (kb/visuals.md
  "Cloth cost" 2026-10-11: bone names hashed per entry): Ape arena simulate 4.79 -> 1.27 ms,
  60 fps in play. `frame_rate_log` now counts real time (the title pauses the
  virtual clock). The object collision added 8 k triangles to the gate .hit (no cost seen).

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

### 4.17 Map session, later on 2026-10-11 (the user away; "do as you see fit")
- Cloth: the Guardian Ape's fur cost fixed (kb/visuals.md "Cloth cost" 2026-10-11): simulate
  4.79 -> 1.27 ms a frame, the Ape arena 60 fps in play. `ClothDef::refs` now indexes a
  `ref_bones` table.
- Map: the missing-texture warning fires once per texture per load; `m17_rock_92_3m` is not
  in the game's packs at all (kb/map.md "Missing textures").
- Exporter: a FLVER whose slot names come from another variant of the shader than its MTD
  (the Divine Dragon's tree blossoms) takes the MTD's first albedo / normal `_0` slot (kb/map.md
  "Every boss in its own arena"); boss_52000000 / 53000000 re-exported, the grey "shard
  cluster" is the blossoming tree now. Both m25 arenas only; the other arenas' bins are
  unchanged (no other material in the earlier export logs lacked an albedo except the gate's
  tessellation water, whose slot names match its MTD exactly, so the fallback leaves it alone).
- Exporter `MAP_FAR=<m>` (kb/map.md "Far view"): distant-view m6 / m9 pieces beyond the radius
  at their lowest LOD. Probed on the gate (19 pieces, no fps change); not used there, the
  gatehouse hides the horizon. The old "far-view castle stand-ins" open item is this knob.
- Hang / hug edges exported raw as the map json's `"ledges"` and loaded into `map::LedgeEdges`
  with H gizmos (kb/grapple.md section 1): the gate map has 40 within its radius + 10 m (1
  hang, 39 hug); the arenas get theirs on their next re-export. Data for B5's wall hug /
  ledge hang (Bosses) and the player moves (VXf); nothing consumes it yet.
- Packaged check: `cargo build --profile dist` (20 min, fat LTO; 75 MB exe) + `tools/package.ps1`
  -> dist/Sv1-v0.2.0-windows.zip (28 MB, config.toml from HEAD). The packaged exe started from
  a clean copy without `extracted/` (wscript, no console) shows the "Sv1 stopped" dialog with
  the extract hint and writes sv1.log; its Explorer icon is the red dot. The full clean-folder
  run (extract.ps1 in a copy of the package) found a real packaging bug: the sound stage hung
  for ever on c1080.fsb subsound 0 (FMOD spinning, no crash, so the rerun loop never ran);
  fixed with a 30 s watchdog in `sounds-fmod` (kb/sound.md: c1080 and four more banks hang on
  most of their subsounds, missing Vorbis setup headers; after three hangs the rest of a bank is
  skipped), repackaged. The restarted clean run then found three more packaging bugs, all
  fixed: `package.ps1` did not copy `bgm_decrypt.py`, `boss_bgm.py` and `npc_names.py`;
  `boss_scripts.py` needed the `reference/DarkScript3` clone (its emedf instruction table,
  all rights reserved) so boss scripts and with them every arena failed -> the argument types
  alone are now in `tools/emedf_types.json` (`boss_scripts.py --emedf-table` regenerates it,
  THIRD_PARTY.md) and the clone is optional; `sound/mixer.fev` is a loose game file, not in the
  archives, so the reverb mixer never loaded -> `extract.ps1` copies it. With those the clean
  copy's extraction matches the repo's (arena bins byte for byte; the eight arena jsons gain
  the ledges, one of them 7 hug edges, copied into the repo) and `clean_dist\sv1.exe` plays a
  fight from the title at 60 fps with the mixer loaded. Not kept in the package: the sound
  decode of every bank (the repo's `sound_pcm` is c1010 / c1020 / main only; a full run decodes
  the rest, 8797 files, minus the five hanging banks).
- **Cloud sea stand-in (Divine Dragon arena):** the M[Flow]_Smoke plane was white; the
  exporter now keeps its height / blend mask and flow vector map (batch alpha 3) and
  `map_material.wgsl` `smoke` scrolls the mask along the flow (two phases cross-faded), tinted
  between the MTD's two colours (kb/map.md). Knobs `SHINOBI_MAP_SMOKE_SPEED` / `_TILE`. Both
  m25 arenas re-exported. The shader's own math is not decoded (gap); the look is by eye, and
  subtle: the MTD's two tints differ little.
- **Far view on the Dragon arenas:** `boss_arenas.py` FAR exports m25 with `MAP_FAR=3000`
  (snowy mountains and forest behind the cloud sea; 0.42 M more triangles, load 0.1 s). Open:
  a small magenta rectangle on that horizon (kb/map.md "Far view").
- The package was rebuilt after all of the above (`cargo build --profile dist`, 14 min;
  `tools/package.ps1`): `dist/Sv1-v0.2.0-windows.zip` (28.2 MB) carries the cloud sea shader,
  the fixed extraction (python tools, emedf table, mixer.fev) and `boss_arenas.py` FAR.
- Chained Ogre cloth, fixed: the "particle 8" blow-up was an empty
  `constraintExecution` list in the game data (15 cloths: the Ogre's four fur sims, c1070's
  hair, c1190's, c5020/c5021 hair, c5050 beard2, c5060 coat). Havok runs every constraint set
  in stored order when the list is empty; our loader ran none, so the constraints never held
  the particle. `src/cloth.rs` now defaults an empty list to all sets plus the collision pass;
  the 90 s line-up logs no blow-up (cloth 4.4 ms, the four fur sims now cost real time;
  kb/visuals.md). A 1.4-2.2 m "ref gap" on its cloths was the 15 m cloth range: an instance
  40 m away is carried rigidly in its bind pose while its skeleton sits. The cloth debug line
  now names each reference bone with its position; `sekiro-extract cloth` prints each sim's
  address and each Simulate op config. Logs of a default session: 65 lines, nothing per frame.
  The package was rebuilt once more after this fix.
- Cloths that drew nothing: the Guardian Ape's 12 fur cloths and 96 more on ~40 chrs (hair,
  kimonos, bandages, Wolf's face hair) have no MeshMeshDeform display because the sim mesh is
  the drawn mesh itself (gathered by a Copy or GatherAllVertices op). The exporter now emits
  a direct display for them (cloth_export.rs); every character model was re-exported (the
  extraction does it on its own next time). One miss, c1010's hatimaki. Line-up 109 display
  meshes (was 78), cloth 4.0 ms. The blow-up threshold and the 15 m range cull now add the
  cloth's rest reach (the Demon of Hatred's mane restarted every frame from the bind pose,
  and its root 5 m behind the head got its cloth culled at 16 m). The sandbox catalog shot
  leaves the simulate-time camera at the origin, so cloth in those shots needs
  `SHINOBI_CLOTH_RANGE=0`. kb/visuals.md.
- Not touched: the blossoms' bloom-to-withered gradient.

### 4.18 Folding Screen Monkeys, Map session (2026-10-12; user: "work on the rest of the bosses"; now the contributor's)
The last main boss that did not work. Started by Map and left unfinished. The same day the user gave
every boss in `extracted/enemies/npc_names.json` to an outside contributor (section 0.1), so this
fight is theirs now.
- **The fight's data** (m20_00_00_00 emevd, boss_scripts.json 12600050): the bar sits on a
  c1000 dummy 2000800 (gravity off, 12005813). The four c1260 monkeys are 2000801-804, NpcParam
  12600000 / 10 / 20 / 30 (the hp-5 "visible monkey" is 12600030). Each monkey's death sets
  SpEffect 3126010 on the dummy (12005820 slots 0-3, a quarter of the bar each, gap: confirm the
  row's HP change). The defeat event 12005800 fires on all four dead or the dummy's HP at 0.
  The bar shows when any monkey reaches AI state 3 (12005810, name 901260). The nine c1250
  "Temple Demon Monkey" horde (2000805-814, NpcParam 12501099) wakes at 75 / 50 / 25 % of the
  bar while Wolf is in 2002851 / 2002861 / 2002881 (12005890 / 895 / 900). The fall reset
  12005885 warps Wolf to 2002200 and the monkeys home. Monkey 2 changes AI 12600010 <-> 12600011
  in region 2002830 (12005838). Other monkey events: 12005827-875 (see-through, lock-on point,
  NPC part 5091).
- **Exporter fixes** (Bosses was told): `tools/boss_scripts.py` `defeat_gates` puts the characters
  whose Dead/Alive checks gate 2003[12] into the cast (the monkeys were "places", nothing
  enables them). `FLAGS_OFF = {12600050: {9405}}`: 9405 is the fight's own "over" flag (12005800
  ends on it), so it starts off. Only row 12600050 changed in the json. Lady Butterfly's
  11005800 ends on 8304 in the same way, left alone because the Owl (Father) needs 8304 on.
- **Own arena** `boss_12600050` (the user's rule: the hall's regions are the mechanics):
  `boss_arenas.py` OWN_ROWS plus override `MAP_PICK=2000802`, `RADIUS=85`. The new exporter
  env `MAP_PICK=<entity>` picks pieces round that part while the arena frame stays on the
  centre part. 515 MB, 4.2 M triangles, 1.29 M collision triangles, 969 pieces. Shot: Wolf
  stands in the lit wooden hall; dark overall, cyan sky patches through wall gaps.
- **Open, in order** (1 and 2 are shared-engine defects and stay with the "bug fixes"
  session; 3 is the contributor's):
  1. ~~Frame rate: about 3 fps.~~ **Fixed 2026-10-12 ("bug fixes"): 63 fps.** Not the
     interpreter (all 83 events cost about 110 instructions a frame). Script warps (12005873
     Warp Character and Copy Floor of monkey 4 to Wolf, about 110 m) went through
     `bodies_on_terrain` as a walk: `Terrain::slide` gathered every triangle in the whole move's
     box of the 1.29 M-triangle arena and ran 512 substeps, so one frame took about 33 s, then
     stopped at the first wall and undid the warp, and the event warped again. Now a move
     longer than `map::TELEPORT` (2 m) in one tick skips the slide. Act::Warp also keeps the
     target's height (it was put at the boss's start height).
  2. Kimonos: the meshes are right (the rows' masks 20 / 21 / 22 differ, TAE 233 never touches
     them; the other kimonos spawn hidden). Their cloths were still simulated; since
     2026-10-12 a cloth whose display meshes are all hidden is carried rigidly like one out of
     range (cloth.rs `hidden`; Monkeys cloth cost 1.25 -> 1.12 ms).
  3. Then the fight itself: monkeys flee and hide by their AI, the bar drops a quarter per
     monkey, the horde drops in, defeat. `all_enemies_full` passes `c1000:12600050` but only
     fights the dummy, so it needs a test of its own.
- **Test take:** `%TEMP%\sv1_takes\monkeys` (config chr c1000, npc_row 12600050; its
  `extracted` is a tree of links with its own `enemies/boss_scripts.json` copy for experiments).
  `SHINOBI_FPS_LOG=1` prints the frame time; `SHINOBI_SCRIPT_TRACE=1` the script.

### 4.19 Bug hunt, "bug fixes" session (2026-10-12)

A workflow of six finders (docs, script interpreter, enemy harness, a live run, gameplay logic,
code markers), each finding checked by two skeptics: 13 confirmed, 3 split, 1 refuted. Fixed:

- **Gate fight never started**: the closed ObjAct door o111410_8000 had collision between Wolf
  and the line-up. The exporter leaves ObjAct parts out (kb/map.md); m11_01_00_00 re-exported.
- **Monkeys 3 fps**: long script warps ran as a wall slide (map.rs `TELEPORT`); 63 fps now.
  Act::Warp keeps the target's height (script.rs).
- **One swing, one target**: `Actor.hits_done` is per defender (actor.rs, combat.rs).
- **Parry notify seen once**: enemy.rs parry_interrupt cleared `last_notify` only on a new
  start time, so Combo1 then Combo2 (both 0.6) raised Goal.Parry once; now per window.
- **Respawn HP**: a doped enemy came back at raw NpcParam hp; now hp_max.
- **Ghost arrows**: bullet hits queued on a dead / broken shooter landed later; dropped.
- **Area doping**: deflect posture and throws skipped the attacker's rates; applied.
- **Break facing**: front / back collapse was picked against the pair partner; now Wolf.
- **Rival lunges**: TAE 760 boost and the body twist aimed at Wolf while an enemy fought a rival
  NPC; enemies set `Actor.homing` to the rival.
- **Reset keeps poison**: reset_fight clears `Statuses` and despawns enemy bullets.
- **Debug menu**: "not working yet" on 8 working prosthetics removed.
- **Hidden cloths**: a cloth whose display meshes are all hidden is not simulated (cloth.rs).

Tests: a_fight_reset_clears_wolfs_poison, one_slash_hits_two_enemies_side_by_side,
a_doped_enemy_respawns_with_its_full_bar, the gate walk in arena_floor_and_walls; 218 pass.
Not fixed then (fixed in 4.23): hit capsules tested the last frame's pose (combat.rs resolve in
FixedUpdate vs anim.rs pose_skeletons in Update); region
tests drop heights off a boss's own arena (by design, split verdict; only the Monkeys' kill
boxes are affected and only off their arena).

### 4.20 Bug hunt round 2, "bug fixes" session (2026-10-12)

Three finders (player / camera / HUD; tools / enemy bullets / wire; stealth / events), one
skeptic per finding: 4 confirmed, 2 refuted. Fixed:

- **Stale guard tap became the combat art**: a guard tap let go before its window (flag 117)
  stayed in `Player.requests` for good (buffer 0 never times out), and any later attack paired
  with it into the art. Now the tap is dropped when its window comes with the button up, and
  the pairing needs a guard press of at most 0.1 s (gap: the game's pairing window).
- **Reset in mid-air**: R / Retry / an encounter swap kept airborne, vel_y, air_base,
  move_vel, knockback, crouch, plunge / perch, the queued presses and the timed SpEffects; the
  reset block clears them.
- **Wire points**: WireVariationParam row 5 is 90 all round (was 180 left / right), row 40 is
  swung through (targetMarkerType 1, was ridden). `map::tests::wire_variation_matches_the_param`
  checks the table against the param.
- **Bullets skip the armour** (prosthetic.rs hit_enemy): taken by Bosees, the armour's owner.

Tests: a_dropped_guard_tap_does_not_turn_the_next_attack_into_the_art,
a_reset_mid_jump_lands_wolf_at_the_start, wire_variation_matches_the_param.

### 4.21 Bug hunt round 3, "bug fixes" session (2026-10-12)

Three finders (anim / actor / interp; AI runner / data; script interpreter / sound), one
skeptic per finding: 5 confirmed. Fixed here:

- **Respawn kept the old AI**: the in-place respawn reused the Brain (SetNumber values,
  timers, observes; c1010's Act08 never came back). It is dropped now, as the exe's AI reset
  does (test a_respawned_enemy_starts_with_a_fresh_brain).
- **Teleports drawn as a slide**: a reset or warp inside FixedUpdate was lerped across the map
  for one step (interp.rs); a move longer than `map::TELEPORT` now draws as a jump.

Handed to the owners: knockback integration overshoots dist by 3-76 % (actor.rs advance;
Bosees, with its knockback work), the native AI goals compare centre distance against
script distances that already take off the hit radius (ai.rs; Bosees), and Wolf's walk / run
footsteps never play because procedural Locomotion has no anim (sound.rs; VFX and Maps).
229 tests pass.

### 4.22 Combat task 1 + handed bugs, Bosees session (2026-10-12)

- **Knockback facing** (exe FUN_140b6bfe0): the push is times cos^2 of the angle between
  its direction and the attacker's facing, never below 0.4 (0.4 from behind); Wolf's recoil
  off a knockback-cutting enemy shrinks the same way (`combat::knockback_facing`).
- **Knockback integration**: pushes now cover exactly their distance (were 3-76 % long).
- **Hit shapes**: hitSourceType / shape priority need no code on this data (all NPC rows 0,
  all priorities 0, no body-source PC row names a sword dummy id); kb/damage.md "HIT SHAPES".
- **Bullets on the armour**: the shuriken etc. go through `combat::armor_rates`; HP cut by
  atkAttribute, posture by staminaPhysicsAttribute, all 12 attribute types (the shuriken is
  9 Light Shoot: x0.35 HP, x0.5 posture on 11807101). The export carries the 7 new
  def*StaminaDmgRate SpEffect fields (extractor re-run: export + npcs).
- **Reach / first hit vs the clips** (task 2): table in kb/damage.md. Wolf's Combo1 matches
  (f7 vs ~6.5); Ichimonji and the General's attacks are not comparable in the clips we have;
  Mortal Draw opens 8-12 frames earlier than the clip's first arc (open). No clip shows c1010
  (the Ochimusha), the dog c1150 or a c1020 attack from idle: a recording is needed.
- **Stealth on the gate map** (task 3): patrol routes done (tools/patrols.py ->
  patrols.json; config `enemy.group` entry `msb = <entity>` puts an MSB enemy at its spot on
  its route; ai::Patrol). The gate has no forced-crouch area and no dark collision, and
  109201 has no setter anywhere: nothing to add there (kb/enemy.md). MSB enemies start
  unaware so they walk; their map event's AI hold (common_func 20005110) is run (enemy.rs
  AiHold: idle until Wolf enters its wake region, it gets a wake SpEffect or is hurt). Try it: in config.toml
  `[[enemy.group]] chr = "c1010"  npc_row = 10102010  msb = 1110236` with the gate map.
- **AI pacing vs recordings** (task 4): the live General (10203010) and Ochimusha fights
  compared with the sim (`ai_pacing`, ignored test): same range of gaps and distances; the
  sim General chains more, the sim Ochimusha attacks from further. Not one to one (live Wolf
  attacks and deflects, the sim Wolf only guards); no tuning made. Table and data notes in
  kb/enemy.md "AI pacing". Needed: a recording of Wolf only guarding against one regular
  enemy (rec_c1010_20261009 turned out to be Hanbei c1012).
- **Goal life end**: a timed-out goal fails unless SetLifeEndSuccess is set on it or above
  (exe FUN_1405d06c0); was always a success (`ai.rs life_end`).
- **AI goal distances**: CommonAttack / LeaveTarget / KeepDist now measure like GetDist
  (minus the enemy's own radius), as the exe's goal updates do (`ai.rs goal_dist`).

Tests: knockback_scales_with_the_attackers_facing, a_knockback_covers_exactly_its_distance,
hit_source_and_priority_change_no_hit_on_this_data,
a_shuriken_on_the_armoured_servant_is_cut_like_a_slash, native_goals_measure_distance_like_get_dist,
an_msb_enemy_walks_its_patrol_route.

### 4.23 Bug hunt round 4, "bug fixes" session (2026-10-12)

Finders on the files rounds 1-3 left out (model, menu, ffx / gore, cloth, debug menu),
one skeptic per finding: 6 confirmed. Fixed here, both in the enemy respawn block:

- **Respawn kept hidden parts**: a TAE 233 DrawMask hide (dropped weapon, broken armour
  piece) stayed after the in-place respawn. The mask is cleared now.
- **Respawn kept the broken armour**: `e.armor` stayed None after the armour broke; it is
  set again from the NPC row (`armor_of`).

Test a_respawned_enemy_gets_its_weapons_and_armour_back (fails without the clear; it reads
the mask on the first frame of the new life, later the AI may hide a part on purpose).

- **Hit test on last frame's pose** (left open in 4.19): bones were posed in Update, so
  combat, the grapple and every FixedUpdate dummy reader saw the drawn pose of the frame
  before, one tick late at 60 fps and more below it. Posing (with the twist layers and model
  lag) now runs in FixedUpdate at the sim's anim time, `anim::PoseSet` (after
  ActorSet::Advance, before Resolve), and hand-propagates the skeletons' GlobalTransforms;
  Update draws between the last two ticks' poses by DrawAlpha (`draw_poses`). Test
  the_hit_test_sees_this_ticks_pose. The Ogre grab test now sees the real grab pose (hook
  2.33 m up, bent forward; it read the idle pose crossfading in before).
Handed to VFX and Maps: the death screen's Retry bounces back to 死 (menu.rs), the picker
tab stays stuck after a click, ffx / gore check TAE edges in Update against the
FixedUpdate clock (missed or doubled spawns), and Wolf's locomotion foot dust never plays.
236 tests pass.

### 4.24 Bug hunt round 5, "bug fixes" session (2026-10-12)

Finders on the code changed today (the PoseSet move, armour / knockback, AI goals / menu /
sound), one skeptic per finding: 3 confirmed. Fixed here:

- **TAE 715 lost in the sim pose** (from 4.23): the hand propagation put the weapon parts back
  on their rest dummies every tick, so Mortal Draw's arc and capsules (Model2 on dummy 20) read
  the hip (149). model.rs `override_weapon_location` now also runs in PoseSet after
  `anim::store_sim_poses` (test the_hit_test_sees_weapon_parts_where_tae_715_puts_them).
- **Mortal Draw never used its capsules**: combat.rs looked up 12220 as 2220 (`dmy_key`), but
  Model2's dummies are kept under the full id; every shape dropped and the 2.1 m reach
  approximation ran. `dmy_entity` tries the full id first, as ffx::dummy_candidates
  (test mortal_draw_reaches_an_enemy_four_metres_away; kb/damage.md note: the table's "Mortal
  Draw" row is Ashina Cross, the real one is a106).

Handed to Bosees (fixed): the armour's HP cut ignored defHeavyHit / AntiGround / AntiAir /
LightShoot / NinsatsuDmgRate (atkAttribute 5-9); the shuriken now takes x0.35 as a slash.
238 tests pass.

### 4.25 Bug hunt round 6, "bug fixes" session (2026-10-12)

Finders on status effects, terrain physics and the core combat rules, one skeptic per finding:
5 confirmed, 1 refuted (an airborne Wolf taking a grab start's damage: the data allows it).
Fixed here:

- **Respawn kept the poison / burn**: Statuses is keyed by the entity, which the in-place
  respawn keeps; the new life went on losing HP with the old flames on. `Statuses::clear(e)`
  from the respawn (test a_respawned_enemy_is_not_still_poisoned, fails without it).
- **Respawn at the death height**: it came back at its start x/z but the height it died at
  (killed downhill: buried, as bodies_on_terrain only steps up 0.5 m). `enemy::respawn_spot`
  stands it on the floor there (test a_respawn_stands_on_the_floor_at_its_start).
- **Deflect break pulled from any range**: a shooter broken by its own deflected bullet (the
  Innsmouth dog from 8 m, a Nightjar on a roof) was snapped onto Wolf in the 弾き pair. The pair
  now needs a melee deflect inside the row's Dist / Y ranges (`combat::throw_in_reach`, test
  a_deflect_break_pairs_only_in_reach); else AttackBoundEmptyStamina where it stands.

Handed to the owners (both fixed): MSB patrol enemies spawned on the wrong floor (Bosees,
spawn_lineup), a re-ignited burn orphaned its flames (VFX and Maps, status.rs start refreshes).
242 tests pass.

### 4.26 Bug hunt round 7, "bug fixes" session (2026-10-12)

Finders on the AI env bindings, throws / deathblows and the prosthetic tools, one skeptic per
finding: 5 confirmed, 1 refuted. Fixed here:

- **A jump in the break pose killed**: the deflect / Mikiri break pose's jump (ThrowParam 0012
  弾き1B_ジャンプ / 0122, ThrowKind 20110 弾き1_裏回り: a vault round behind, a201_510120 has no
  ThrowAttackBehavior) went through main_deathblow and killed a regular enemy outright. It
  now vaults as the 0201 kick jump: ThrowDef12120 / 13120, the enemy stays open (test
  a_jump_in_the_break_pose_vaults_without_killing).
- **Enemy grabs ignored the absorb**: Wolf was held where the grab capsule touched him; the
  grab now sets a ThrowHold (atkSorbDmyId 230 / 231: Wolf onto the grabber's dummy, ModelLag
  over adsrobModelPosInterpolationTime; defSorbDmyId: the grabber onto Wolf's), run by
  player.rs follow_throw (test a_grab_pulls_wolf_onto_the_grabbers_dummy: 2 cm off vs 11).

Handed to the owners (all fixed): a goal whose life ran out ended in SUCCESS, the exe fails it
unless SetLifeEndSuccess (Bosees, ai.rs; all_enemies_full still 94/110); the Sen Throw threw a
free LV1 shuriken, and tool bullets ignored disableGuard / disableJustGuard (VFX and Maps,
prosthetic.rs). Gap: the Sen Throw's coin gates 996-999 have no source yet.
247 tests pass.

### 4.27 Bug hunt round 8, "bug fixes" session (2026-10-12)

Finders on movement / air states, map events and sound, one skeptic per finding: 5 confirmed,
1 refuted (the wire perch floor). Fixed here:

- **Early air jump / guard lost**: in the air the Jump and Guard requests were removed before
  their execute flags were checked, so a jump pressed at AirComboAttack1 frame 3-11 (87 buffers
  from 3, 119 opens at 12) never became the AirKick. They now stay pending as the air attack's
  does; a guard tap let go before 117 is dropped, as on the ground (test
  an_early_jump_in_an_air_slash_becomes_the_air_kick).

Handed to VFX and Maps: the area field track is inverted (common_func 20004010 sets 9100 while
Wolf is OUTSIDE the 屋内判定 regions: the 2690 屋外フィールド track plays outdoors), the track after
combat resumes 1 s early (event 410 label 13's second 1.0 s wait), the anim chr's FMOD bank is
never loaded (c1520 / c1151 / c7530 silent), slotted FFX sounds stack on looping idles.
249 tests pass.

### 4.28 Bug hunt round 9, "bug fixes" session (2026-10-12)

Finders on config reload / data loaders, camera and the FXR player, one skeptic per finding:
6 confirmed. Fixed here:

- **F5 reverted the picked fight**: reload_on_f5 replaced the whole GameConfig, so an encounter
  picked in the menu (enemy chr / npc_row / group set in memory) went back to config.toml's
  while Combat.kinds kept the picked one; the next reset spawned the file's group into it.
  `config::reloaded` keeps the fight in progress (test f5_keeps_the_picked_encounter).

Handed to VFX and Maps: FXR tick / draw run in Update and so ride the latest sim pose since
4.23 (trail leads the blade 10-40 cm; move to PostUpdate after Propagate), bullet-anchored
effects start at the world origin, a Retry keeps the death camera's 7.5 m, lock-on switches
while paused. To Bosees: a group npc_row that is not the chr's own is silently dropped.
251 tests pass.

### 4.29 Enemies: map AI hold, patrol stepping, turn clips (Bosees, 2026-10-12)

- **Map AI hold**: config `msb` enemies run their common_func 20005110 (AI off until Wolf is in
  the wake region, a wake SpEffect, or a hurt): patrols.json "wake", enemy.rs AiHold / ai_hold.
- **Patrol stepping** traced (FUN_1405afe20, table 0x143af9550) and ported: type 5 routes end
  (cur -1, NonBattleAct then plays the last point's arrival anim), arrivals match by region,
  IsRouteEnd. ai.rs Patrol.
- **Turning in place**: c9997 ExecTurn's W_Turn*_90 / 180 clips from idle for turns of 45
  degrees or more (enemy.rs start_turn, Actor::yaw_scale), scaled to the asked angle.
- **Round 9's foreign npc_row**: Foe::asked_row; kind_index and the dedupes find it.
- **Route end = new home**: type 5's end calls FUN_1409f60c0 (initial position / rotation,
  IsChangeInitialPosition): Patrol::new_home moves the enemy's home there (patrols.json "yaws").
- **Guard stance**: GOAL_COMMON_Guard's 9910 is ACTION_TYPE_GUARD, not a clip (it failed, so
  enemies never guarded): GuardStart -> GuardIdle, guard walks, W_TurnGuard, GuardEnd; the clips'
  ShieldBlock blocks. enemy.rs guard_stance, ai.rs Command.guard. Move goals with the guard
  action 9910 as their last param walk guarded too. Turn scale traced (act 9105: |err| / 90|180).
- **General stance**: 9920 (ACTION_TYPE_GENERAL_STANCE) runs like the guard (enemy.rs Stance,
  ai.rs Command.general): GeneralStanceStart401060 -> Idle, walk / run front, Trun_* turns,
  GeneralStanceEnd601040; one stance asked for in the other ends the other first.
- **Round 12 (our part)**: respawn clears the moved home, reset takes Wolf's bullets, art bullets
  exported (sekiro-extract), instant_hp_rate window; details in 4.32.
- **Route-end anim**: m11_01 11115236 / 11115238 play Event20004 once when the patroller is at
  its route's end unaware (the exe never reads the event's loop byte); alerted, never.
  enemy.rs RouteEndAnim, patrols.json "route_end". MSB region shapes fixed to SoulsFormats'
  ShapeType (2 is a sphere, not a cylinder).
- Rounds 13 / 14 (our part): respawn opacity / facing / targeting / hold, rival_view,
  react_from, NpcParam attribute rates, StaminaControlParam export, Covert A condition;
  details in 4.33 / 4.34.
- **sekiro-rs ports** ("bug fixes" findings): super armour swaps the damage level
  (damage_motion_level, replaces the flag-73 proxy), aerial hit reactions
  (enemy_damage_state_air), a guarded bullet leaves its shooter alone.
- **Patrol arrival** traced: a point is reached inside its MSB region shape (IsInsideMovePoint),
  patrols.json "areas"; point regions keep the distance test (gap).
- **Later 2026-10-12**: EXLARGE guard launches (DamageLargeBlow / AerialBlow); the general
  stance's own attacks 603000-603009 start the stance first; IsArrived (radius) runs before the
  patrol region test; a blocked hit with no HP lost no longer wakes a map-held enemy.
- kb/enemy.md has the details. 309 tests; all_enemies_full still 94 of 110.

### 4.30 Bug hunt round 10 (2026-10-12)

Finders on the HUD, enemy bullets and map / fight switching, one skeptic per finding:
6 confirmed, none refuted. All fixed by the file owners, each with a test checked to fail
without its fix:

- **Line-up faced away from Wolf** (Bosees): spawn_lineup gave yaw 0 (faces -Z, same as Wolf);
  it now faces player_start. MSB spots keep their yaw. Test the_line_up_faces_wolf.
- **危 outlived a reset** (Bosees): reset_fight zeroes hud::Perilous. Test
  a_reset_clears_the_perilous_kanji.
- **HitBulletID ignored launchConditionType** (Bosees): only 254 was read, so lct 1 "water"
  children (c1310 arrow 13100520 -> 13102501, kemari 13103190 -> 7.5 m lightning) spawned on
  every end. `enemy_bullet::launches` follows the SDT enum (1/2 never, 3 missed, 4 hit, 254
  expired, 255 ground or enemy, else any end). Test a_bullets_child_follows_its_launch_condition.
- **Bullets ignored Wolf's iframes** (Bosees): build-up, SpEffects, despawn and child came
  before combat's dodge test. fly now runs `combat::dodged` (attacker `Option<&Actor>`) first;
  a dodged bullet passes through. Test a_dodged_bullet_passes_through_wolf.
- **Deathblow mark on a different enemy than the attack** (VFX and Maps): player.rs
  `deathblow_pick` (in reach: locked, else nearest) now feeds both; the HUD alone falls back
  to a broken enemy. Test the_deathblow_mark_is_on_the_enemy_the_deathblow_takes.
- **R / Retry deleted the sandbox line-up** (VFX and Maps): SandboxEnemy tag, Lineup kinds,
  respawn_lineup after a reset refreshes Catalog.order. Test a_reset_brings_the_sandbox_line_up_back.

259 tests pass.

### 4.31 Bug hunt round 11 (2026-10-12)

Finders on the Lua VM / EMEVD, FMOD / music, the day's new enemy code and pause / time
handling, one skeptic per finding: 6 confirmed, 1 refuted (latent). Each fix has a test checked
to fail without it:

- **TAE 233 draw masks missed under 60 fps** ("bug fixes"): update_draw_masks read the edge in
  Update from the last tick's (prev_t, t] only, so a frame running two ticks lost a 233 in the
  first (a death anim kept its weapons). model.rs `apply_draw_mask_events` records 233 in
  FixedUpdate after ActorSet::Advance; Update only writes visibility. Test
  a_draw_mask_change_is_kept_when_a_frame_runs_two_ticks.
- **Camera shakes stacked** (VFX and Maps): start_shakes in Update re-pushed the same RumbleCam
  every frame between ticks (2-3x at 144 Hz, one copy per paused frame). Now camera::add_shakes
  in FixedUpdate. Test a_camera_shake_starts_once_per_tae_event.
- **FMOD never paused** (VFX and Maps): SE / voice / default categories pause off the Playing
  screen (music and menu keep on), slotted loops stop on Title / Picker / Dead. Test
  fmod::tests::sounds_pause_and_slots_stop_off_the_playing_screen (needs the FMOD DLLs).
- **Fmod.missing kept keys for good** (VFX and Maps): an encounter swap's old enemies asked for
  the new chr's sounds before its bank loaded; load_project now clears `missing`. Test
  a_key_missing_before_its_project_loads_plays_after.
- **忍殺 mark dropped in the ThrowDef poses** (VFX and Maps): the HUD skipped every ThrowDef*
  state while the attack did not (13900 / 12120 / 13120 / 12100 / 13100 are deathblow-open).
  Test the_deathblow_mark_shows_on_a_break_pose.
- **Penetrating bullets launched by the frame's touch** (Bosees): `launches` now gets the
  bullet's whole-life `b.hit` (c1190 blast 11903310 lct 255 never spawned 11903311). Test
  a_penetrating_bullet_launches_by_its_hit_record.
- Latent (refuted, nothing reaches it today): lua50 `math` lacked tan / deg, called by 45
  battle scripts behind stubbed host calls; Bosees added both.

### 4.32 Bug hunt round 12 (2026-10-12)

Finders on the enemy guard stance / patrol home, grapple / wire, tools and Update-vs-Fixed
event edges, one skeptic per finding: 5 confirmed, none refuted. Each fix has a test checked to
fail without it:

- **One-shot recoveries fired twice** ("bug fixes"): decide's TAE 67 / 401 window used
  `prev_t - 1e-4`, overlapping the previous tick, so a start on a tick boundary (38 of 170
  player recoveries, e.g. a200_510110's kill frame) gave back 68 % posture / 20 % HP.
  player.rs `one_shot_starts` is the strict edge plus frame 0 on the first step. Test
  player::tests::one_shot_recoveries_fire_once. Bosees made the same change to enemy.rs
  instant_hp_rate (test an_instant_recovery_on_a_frame_edge_counts_once).
- **Wire fall type kept after landing** ("bug fixes"): Wire::launch's fallType 2 was never
  reset, so a later walk-off fell at -19.6 with 6 m/s air control. Landings, the reset, the
  death landing and the walk-off FreeFall set fall_type 0. Test a_wire_landing_ends_the_wire_fall_type.
- **Art bullets looked up through the prosthetic** (VFX and Maps + Bosees): art anims
  (a100-a110) now take only `v{art variation}:{judge}`, and the extractor exports the
  v5004 / v5005 / v5030 rows (One Mind, Dragon Flash, ...). The emblem charge stays by wepCost
  (the game's own cost). Tests an_art_fires_its_own_bullets_not_the_prosthetics,
  the_combat_arts_bullets_are_exported.
- **A respawn kept the old route-end home** (Bosees): `e.home = None` with the Brain. Test
  a_respawn_forgets_the_route_end_home.
- **Wolf's bullets survived a reset** (Bosees): reset_fight despawns prosthetic::Projectile and
  stops its anchored FxEffect. Test a_reset_takes_wolfs_bullets_in_flight.

### 4.33 Bug hunt round 13 (2026-10-12)

Finders on reset completeness, panics / NaN, stealth and enemy scripts, one skeptic per
finding: 5 confirmed, none refuted (the panic sweep found nothing solid). Each fix has a test
checked to fail without it:

- **Lightning charge kept through a reset** ("bug fixes"): Actor.electro (9490 / 9495, 30 s)
  was dropped only by the landing, which the reset skips, so the next fight's first air attack
  was a free Lightning Reversal. The pad.reset block clears it. Test
  a_fight_reset_drops_the_lightning_charge.
- **Respawn came back invisible** (Bosees): the death clip's TAE 193 fade (c1180 / c1140
  ThrowDefDeath*) left opacity 0; the respawn sets it as spawn_one does. Test
  a_respawn_comes_back_visible.
- **Respawn kept the death yaw** (Bosees): Enemy.start_yaw; the respawn restores the yaw and
  seeds the home pose with it. Test a_respawn_faces_its_start.
- **Respawn kept the old targeting** (Bosees): MSB / unaware spawns respawn unaware and held
  again (make_unaware + the AiHold they had); the line-up as spawn_one. Test
  a_patrollers_respawn_is_unaware_and_held_again.
- **A rival target looped the alert spin** (Bosees): the forced BATTLE snapshot kept the real
  prev_state, so IsChangeState was always true and 401040 repeated. enemy.rs rival_view makes
  the change show only when the rival comes or goes. Test a_rival_target_changes_state_once.

280 tests pass.

### 4.34 Bug hunt round 14 (2026-10-12)

Finders on group fights, damage formulas vs the params, combat arts / skills and lock-on, one
skeptic per finding: 8 confirmed, none refuted. Each fix has a test checked to fail without it:

- **Enemy grab pairs pushed apart** ("bug fixes"): actor.rs in_throw now covers "Grabbed" and
  ThrowAtk*, so separate() no longer undoes the ThrowParam absorb (c1360 21360000 dummy 173:
  held 0.8 m off instead of riding onto Wolf's back). Test a_grab_pair_is_not_pushed_apart.
- **A rival's blow turned an enemy on Wolf** (Bosees): Enemy::react_from(.., by_wolf, ..);
  only Wolf's contact calls damaged() / wakes an AiHold. Test a_rivals_blow_does_not_turn_it_on_wolf.
- **NpcParam per-attribute rates ignored** (Bosees): <attr>DamageCutRate / DamageRate and
  <attr>StaminaDmgRate multiply Wolf's hits (sword and bullets), e.g. light shoot x2 HP / x8
  posture on the dogs. Test a_shuriken_takes_the_light_shoot_rates.
- **StaminaControlParam rows missing** (Bosees): each enemy file now carries the rows its
  NpcParam names (c1180 1070000, c1060 / c1070 1060000, ...); regen_posture warns once on a
  missing row. Test the_enemies_posture_regen_rows_are_exported.
- **Covert A cut sight standing up** (Bosees): skill SpEffects need their
  invocationConditionsStateChange (949: crouch / wall hug / hang). Test covert_a_cuts_sight_only_crouched.
- **Hold arts released outside their windows** (VFX and Maps): the ground draw needs TAE ref
  222, HoldEnd ref 230; the air hold ends only on those. Test hold_arts_release_only_in_their_windows.
- **Lock-on took the nearest enemy in 360°** (VFX and Maps): camera.rs acquirable (ChrFront
  5 m / 30°, CamFrontNear 8 m / 60x30, CamFrontFar 45x30), then the nearest. Test
  a_new_lock_takes_an_enemy_in_front_not_the_nearest_behind.
- **Switch pitch from the horizontal** (VFX and Maps): cam_angles measures from the camera's
  front. Test the_switch_pitch_is_from_the_camera_front.

288 tests pass.

### 4.35 Bug hunt round 15 + sekiro-rs findings (2026-10-12)

Round 15 finders on enemy TAE coverage, the enemy death flow, Wolf's hit reactions and Wolf's
items / resurrection, one skeptic per finding: 7 confirmed, 1 refuted (the ground deathblow
start's committed throw is intended). Each fix has a test checked to fail without it:

- **Plunge onto a broken enemy** ("bug fixes"): react does nothing on Mode::Broken, so the
  defender pose ThrowDef13400 never played and the Broken window could end mid-fall (the enemy
  stood up, the landing still killed it). start_plunge plays the pose and holds it broken
  (on_posture_break, the pose's 3 s); the landing deathblow needs the enemy still broken. Test
  a_plunge_holds_the_broken_enemy_in_its_pose.
- **Air guard break let Wolf guard at once** ("bug fixes"): is_air_guard matched
  AirDeflectGuardBreak. Test an_air_guard_break_holds_wolf_until_its_guard_flag.
- **Death / revival ignored the refresh SpEffects** ("bug fixes"): player.rs
  apply_refresh_effects applies TAE 401 instant rows: the death loop's 8002-8009 clear Wolf's
  statuses, the revival's 8001 restores posture. Test death_and_resurrection_refresh_wolf.
- **Enemy guard clips** (Bosees): blocks and deflects follow ExecGuardBlock (refs 1000090 /
  1000100 / 1000101 / 1000105, GuardKick, Small / Large, next clip when one is missing); the
  shield SpEffect 400230 no longer reads as a deflect. Test an_enemys_guard_clip_follows_exec_guard_block.
- **A broken Wolf** (Bosees): knocked down only by levels 1 / 2 / 3 / 5, level 8 plays nothing,
  the rest take their normal reactions. Test a_broken_wolf_is_knocked_down_only_by_the_lighter_levels.
- **Crouched drink** (Bosees): raises INTERUPT_UseItem. Test a_crouched_drink_raises_use_item.

Taken from sekiro-rs's 2026-10-11/12 commits (MIT / Apache-2.0, credited in comments and
THIRD_PARTY.md), each with a test:

- **Cross-fades on the character's clock** ("bug fixes"): anim.rs advances the fade by the
  actor's own step, so hit stop freezes it (fd53f99). Test hit_stop_freezes_the_cross_fade.
- **Super armour** (Bosees): damage_motion_level from the spCategory 1001 SpEffect's dmgLv_*
  (traced chain in c0f97ee) plus ChrActionFlag 24; the flag-73 gate stays beside it. Test
  super_armour_swaps_the_damage_level_in_its_window. Prosthetic hits use it too (VFX and Maps).
- **NPC aerial reactions** (Bosees): enemy_damage_state_air (c9997 IsDamagedAerial). Test
  an_enemy_hit_in_the_air_takes_the_aerial_reactions.
- **Guarded bullets leave the shooter alone** (Bosees; inferred, as in sekiro-rs). Test
  a_guarded_bullet_leaves_its_shooter_alone.
- **Fog density / GameScale and metering** (VFX and Maps): FixedDensity / GameScale; AutoExposure
  with the game's centre weight and adaptation rate. Height fog and bloom stay gaps. Tests
  map::tests::fog_density_is_divided_by_game_scale, metering_follows_the_games_centre_weight_and_rate.
- **Env_Box is bottom-centred** (VFX and Maps, extractor; the env cubes change on the next map
  export). Cloth P-deformer normal read fixed. Story hour default 6.0 (no 9800-9803 flags).
- Next with VFX and Maps: boss fog walls, sky domes, GIIV.

### 4.36 Bug hunt round 16 + more from sekiro-rs (2026-10-12)

Round 16 hunted regressions in the day's fixes (4.30-4.35): 8 confirmed, none refuted. Each fix
has a test checked to fail without it:

- **ChrActionFlag 24 for Wolf, and no push under it** (Bosees): combat.rs reads the defender's
  flag once. It skips the knockback (no leftover recoil), uses level 0 for enemies, and Wolf
  plays no reaction and keeps his velocity (the grapple shot a000_202000 carries the flag for
  0.63 s). The posture break stays. Prosthetic hits do the same (prosthetic::tool_hit_push, VFX
  and Maps). Tests a_hit_under_flag_24_keeps_wolfs_grapple,
  a_tool_hit_under_flag_24_has_no_knockback_and_wakes_the_hold.
- **Held enemy cut without a flinch never woke** (Bosees): hurt_by_wolf is set wherever Wolf's
  HP damage lands, reaction or not; the hold needs HP lost since its last check (the EMEVD
  damage record is written only for final damage > 0), so a blocked hit does not wake it. Tests
  a_cut_with_no_reaction_still_wakes_a_held_enemy, a_blocked_hit_does_not_wake_a_held_enemy.
- **MSB enemies respawned 2 m off along world -Z** (Bosees): they come back on e.start; the
  duel line-up keeps its offset. Test a_patrollers_respawn_is_unaware_and_held_again.
- **Plunge fall check stopped at world y 0.9** ("bug fixes"): player.rs fall_reaches stops at
  the target's height, so enemies below the export origin can be plunged on. Test
  player::tests::the_plunge_fall_check_works_below_world_zero.
- **One broken enemy turned off every stealth plunge** ("bug fixes"): the ThrowParam row is
  chosen per target. Test a_broken_enemy_elsewhere_keeps_the_stealth_plunge.
- **Options from the pause menu stopped the music** (VFX and Maps): fmod::fight_screen counts
  Options opened from Playing / Paused. Test the_pause_menus_options_keep_the_fight_music.
- **Empty sandbox line-up re-spawned every frame** (VFX and Maps). Test
  a_lineup_that_places_nobody_is_not_retried.

Also from sekiro-rs (MIT / Apache-2.0, credited), VFX and Maps:

- **Fog walls**: src/fog_wall.rs, 47 walls from the map data, kept data-driven (no per-boss
  tuning).
- **Height fog**: src/height_fog.rs + height_fog.wgsl (a936191 / 884ea0b); SHINOBI_MAP_NO_FOG
  turns it off. The castle haze comes out faint, as the data says.
- **Bloom**: map.rs map_bloom from the draw params, level weights by the exe formula (test);
  not yet checked on screen. Sky domes and GIIV are next.

### 4.37 Bug hunt round 17 (2026-10-12)

Round 17 finders on menus and options, docking / interpolation, status effects, and anim /
model, one skeptic per finding: 6 confirmed, none refuted. Each fix has a test checked to fail
without it:

- **Reset / respawn snap** ("bug fixes", Bosees): new anim.rs SnapPose marker. The next posed
  tick snaps to the new clip (no fade from the old life's pose), the drawn pose does not lerp
  from it, and a running ModelLag ends. Wolf's reset and the enemy in-place respawn insert it
  (a fight reset spawns fresh enemies). Tests a_reset_snaps_wolfs_model_onto_the_start,
  a_respawn_snaps_to_its_new_pose.
- **SkeletonRoot kept a bit of the deathblow slide** ("bug fixes"): the drawn root was read back
  as the sim pose; apply_model_lag now holds the root at rest whenever no ModelLag runs. Test
  the_model_slide_ends_at_rest_when_drawn_between_ticks.
- **A resting mouse overrode menu keys** (VFX and Maps): menu::mouse_pick moves the cursor only
  when a hover begins; a click picks. Test a_resting_mouse_does_not_move_the_menu_cursor.
- **Burn rows stacked** (VFX and Maps): status start() replaces an active affliction with the
  same nonzero spCategory (burns 9100-9125 share 10004), else the same id. Test
  a_second_burn_row_replaces_the_first.
- **Tool hit dummies one frame old** (VFX and Maps): pose_tool_models / place_tool_models also run
  in FixedUpdate (PoseSet, after store_sim_poses). Test tool_parts_are_placed_in_the_sim_pose.
- The two FMOD tests share a static mutex: opening the real FMOD at once crashed a run.

### 4.38 Bug hunt round 18 (2026-10-12)

Round 18 finders on the camera, the AI command bridge, param lookups and cloth, one skeptic per
finding: 4 confirmed, 1 refuted (culled cloth). Each fix has a test checked to fail without it:

- **CameraSnap focus 0.9 m too high** (VFX and Maps): camera::chr_focus measures the focus from
  the feet for both the snap and the follow. Test the_camera_snap_focus_is_measured_from_the_feet.
- **IsTargetGuard ignored its target** (Bosees): TARGET_SELF reads Snapshot.self_guard (the
  enemy's own guard stance; Common_Parry skips its deflect while guarding).
- **GetSp / GetSpRate ignored their target** (Bosees): other targets read target_sp /
  target_sp_rate (Wolf's or the rival's posture; 140010_battle.lua compares the two). Test
  ai::tests::guard_and_posture_queries_follow_their_target.
- **Area doping never scaled HP / posture / damage** (Bosees): the extractor's SpEffect export
  now writes maxHpRate, maxStaminaRate and physicsAttackPowerRate (7013 = 2.0); re-exported.
  Test a_doped_enemy_respawns_with_its_full_bar checks hp and posture x2.

## 5. Open work, in rough priority
- **Armoured enemies: done 2026-10-12 (Bosses).** The c1180 servants' EMEVD armour part
  (common_func 20005410) -> tools/armor_parts.py -> enemy.rs Armor: x0.35 HP / x0.5 posture,
  broken by the Loaded Spear (kb/damage.md); the break anim's TAE 233 hides the armour mesh.
  Gap: every hit counts as on the armour (live 7 of 8 were cut; per-part groups exist in the
  hurtboxes, PelvisArmor_00-02 = group 2, but limiting the cut to them is not backed by a trace).
0. **Map, the rest** (kb/map.md "Open"): check stairs / walls / low ceilings and the spawn
   height in play, compare the gate lighting with the real game (light set 100 at noon; sun
   and ambient scales are by eye), far view (exporter `MAP_FAR`, 4.17; unused on the gate), the
   layer blends use the base `_3m` mask as height (soft knob `SHINOBI_MAP_SOFT`); byte 1 of the
   blend data is unused; frame rate is settled at 72-80 fps (kb/map.md).
1. **Stealth, the rest** (core done, 4.7): crouch attacks / steps / reactions (HKS Crouch*
   states); grass / shadow hiding (109201 no setter; MapVisibility Dark not exported, none on the
   gate); wall hug, hanging (limit types); patrol routes done for config `msb` enemies (4.22;
   stepping traced and ported 2026-10-12, type 5 routes end; their map AI hold done 2026-10-12); the real HUD art; the
   user's verdict. (The listener's ear params were done 2026-10-11, kb/enemy.md.)
2. **Confirm with the user** (ask for clips if still wrong):
   - head-kick camera feel with the mouse fix;
   - cloth on jumps;
   - sound volume;
   - vault.
3. **Kick-launch slide: done 2026-10-11.** It was the clip's own root (213111's hkx: 0.27 m
   forward, back to 0 at frames 17-20). Live, 30 head kicks (rec_* recordings) never show the
   step back: actor.rs advance now skips horizontal root motion while airborne unless flag 27
   SetNoGravity holds. Test `the_head_kick_relaunch_goes_straight_up`. Also actor.rs separate
   skips pairs whose heights don't meet (NpcParam hitHeight). Open: live moves only 0.040 m in
   frames 0-6 (ours 0.27): weighting the root steps by a smoothstep over the TAE Blend event
   (6 frames = 12 ticks) predicts 0.042, a probable blend-in rule for every clip, not ported
   (would change all root motion; needs the exe's blend weight first). The `runkick` throw
   trace no longer sets up (Wolf reaches the enemy before the jump); use the sim test instead.
4. **Sound gaps:** the other mixer events (ntc_main, bgm); the per-event "Shahei" (occlusion) parameter (placement, reverb + its 2.6 s easing, deathblow mix and boss music done; room / deep / sd_dimension are read by no event).
5. **Coat second texture layer** (AN_Blend: FC_A_0000_Fabric_blend05/01 detail at 25× tiling).
   The shader math isn't known; it is in the compiled SPX shaders.
6. **Camera gaps:** lockCamAdjustRot manual offset (no reader found); the obstacle fade of
   enemies between the camera and Wolf (one byte unidentified, kb/camera.md). The fulcrum pitch
   resync is done (2026-10-12, camera.rs chases_pitch). The fulcrum yaw resync is done
   (2026-10-11, camera.rs drag_fulcrum / place_fulcrum): the free camera swings round when Wolf
   runs sideways (about 30 deg in 0.5 s, 53 deg in 1 s at 6 m/s). Ask the user how it feels.
7. **Clean-up: done 2026-10-11.** `SHINOBI_CLOTH_FLIP_N` removed; the "ref gap" log only fires
   at the `SHINOBI_CLOTH_DEBUG=<s>` second, not per frame.
8. **c1010 spear cloth** `#04#_c1010_yarinuno`: no restart warning in the 2026-10-11 runs
   (the jump threshold now scales with the cloth, 4.12); closed unless it comes back.

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
   other/decaltex.tpf - unpack 'decaltex' first). Done later (section 9: the blood FFX play their
   FXR, gore.rs is the fallback). Was: FXR not decoded (spray look is made to match
   clip 1), floor dust SpawnFFX_ByFloor 400/420 not done, hit-blood floor decals (AtkParam decalId) not done.
7. (done 2026-10-10) Weird idle after the c1010 plunge (user clip): Wolf's upper body swung side to
   side. The idle's TAE 700 twist (TargetType 3 Lockon) fell back to the auto-aim target, an enemy
   right behind him flipped it between +-45 deg. Now TargetType 3 follows only the lock-on target.
8. (done) "c1020 changed after the gore" = the 360 spin in item 4.
6. (done 2026-10-10, hud.rs DeathblowMark) Deathblow mark: the game's FE sprites MENU_ninsatu_02 + _01
   (menu/hi/01_common.tpf atlas SB_FE, rects from 01_common.sblytbnd SB_FE.layout; export writes
   extracted/hud/) on dummy 220 while player::deathblow_check passes (broken, or in reach behind an
   unaware enemy). 2026-10-12: only ninsatu_02 now, size / fades from the FE movie (see "FE movie marks"). Unpack 'menu/hi/01_common'.

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
shot env vars or `tools/screenshot.ps1`. Ask the user before
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
Gaps: 607 shape (mode: all 45 are 1 NormalMap, drawn), normal maps on sprites, light intensity scale. Done 2026-10-10: Hermite
keyframes now blend as the exe does (`game_hermite`; property evaluators are a table of 44 ids
`function << 4 | type (| 0x1000 loop)` registered in 1400f by FUN_141d98ff0, float Hermite body
FUN_141d94960: two fixed Hermite curves bent into circle arcs, blended by the earlier keyframe's
t1 / t2 angles, pi/4 = straight), replacing fxr.ts's guess; Bezier was already the exe's curve
(FUN_141d94300). Also done: the distortion's depth test (GXFfxdistortionBump.ppo: where the
scene at the bent spot is not farther than the particle, the straight read is kept; fx_distort.wgsl,
via the same `FxDepth` copy, which runs before the transmissive pass).

**Soft particles (2026-10-10):** billboards (603 / 604) with `depthBlend` set (1102 of 1596) fade
where they meet the scene instead of cutting a hard line through floors and walls, as the game's
soft shaders do (GXFfxtessellateSoftTexture .gpo / .ppo, GXFfxsoftTracer .vpo / .ppo, disassembled
as above). Vertex: h = scale x size / 2, the view depth w moves to w - (h + offset), back toward w
by up to 1 if that is nearer than near + 1; fragment: alpha x saturate(min(scene - own, soft) /
soft), soft = 2 offset + h. `fxr.rs soften` does the pull on the CPU, fx_soft.wgsl (sprites,
an ExtendedMaterial on StandardMaterial) / fx_multi.wgsl (604) the fade. The scene depth is the
camera's depth buffer copied after the opaque pass (`FxDepth`, `copy_fx_depth` in the Core3d
schedule; no measurable frame cost). Not Bevy's DepthPrepass: it draws the 4 M-triangle map twice
(~2.4 ms) and, with MSAA, writes alpha-to-coverage hair / leaves solid, so hair edges showed the
clear colour. `SHINOBI_NO_SOFT=1` turns it off; `SHINOBI_FPS_LOG=1` logs the mean frame time
(vsync off). Gaps: (scale, offset) read as unkDepthBlend1 / unkDepthBlend2, size as the larger
side; trails / lines / point sprites have no depthBlend field and stay hard (what picks
GXFfxsoftTracer is not traced).
WindForce (10300, in 24 FXRs): needs nothing more. Its unk_ds3_f1_31 = 1 (plants only) in all but
400 / 401, and the particle force speed there (732) is ~0.1 in the 8 effects that use it.
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
