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
- Not done (gaps written in code): (hit-blood decals: done 2026-10-11, `gore::hit_stain_id`;
  re-checked against FUN_14088a7f0 2026-10-13), the 609 light unit, effect sizes from the gfx ABC.
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
- **Hit effect turn by attack direction: PORTED 2026-10-13** (replaces the parked note below).
  - Static trace: the attack-hit sparks go hit handler FUN_140888960 / FUN_1408895c0 ->
    FXHGSfxCtrl FUN_140929dd0 / FUN_140929fe0 -> FUN_14092b6c0 (14092.c:7843-8042).
  - When HitEffectSfxParam row R exists in HitEffectSfxAngleParam, FUN_1406cd620(X, Y, Z of
    hit[+0x7e] atkDir) **overwrites the look-at rotation outright** (14092.c:8020-8037) as an
    absolute world rotation. Only the position hit + d x hit[+0x60] survives.
    FUN_140b63fe0 (character-relative) is the hit-SE dispatcher.
  - Port: vfx.rs `hit_sfx_turned` / `Clash::bundle_fx_turned` (combat.rs's 3 clash spawns),
    data.rs `Attack::atk_dir_sfx`, fxr.rs `game_rot` now pub(crate).
  - The extractor exports atkDir_forSfx and HitEffectSfxAngleParam (export.rs).
    extracted/combat_data.json was patched locally (1060 attacks + 9 rows; backup in scratchpad
    bossfx/combat_data.bak.json). The next `extract.ps1` export writes the same.
  - Test: `clash_effects_follow_the_hit_effect_tables` (229001 Upper = X -75, no dir = identity,
    concept 10 = look-at), mutation-checked.
  - Euler order settled: FUN_1406cd620 -> FUN_1401b9820 is XMMatrixRotationRollPitchYaw.
    It is a half-angle quaternion, row-vector Rz Rx Ry (1401b.c:5298-5407) = Qy Qx Qz, the
    same as fxr::game_rot.
- 2026-10-13 **Boss-arena check (22 main-boss rows, 10 arena maps).**
  - Test `map::tests::every_arena_start_stands_and_walks` passes on every exported arena. It
    checks the boss floor at (0, -4), the start search (headroom 2.8 m, the stranger line-up
    8 m ahead), and four 3 m walks without holes. Fog gates come from map_<arena>.gates;
    grapple points and ledges from the arena json.

    | Arena map | Rows | Start (y, z) | Walks | Gates | Grapple / ledges | Notes |
    |---|---|---|---|---|---|---|
    | boss_12600050 | Monkeys | 0.93, 4 | ok (+z 1.4 m wall) | none (12 B) | 208 / 0 | |
    | boss_50000000 | Monk 50000000 | -0.36, 4 | ok | yes | 21 / 0 | |
    | boss_50001000 | Monk 50001000 | 2.63, 4 | ok, 0.3 m drop | yes | 4 / 0 | bridge slope |
    | boss_50900000 | Butterfly x2 | 0.93, 4 | ok | yes | 0 / 7 | |
    | boss_51000000 | Ape | 0.97, **10.5** | ok (1.5-2.1 m walls) | yes | 29 / 0 | start moved for headroom |
    | boss_51000100 | Headless Ape | 1.40, 4 | ok | yes | 10 / 0 | |
    | boss_52000000 | Divine Dragon | 0.90, 4 | ok | none | **1 (at y -277)** / 0 | tree points missing (open) |
    | boss_53000000 | Old Dragons | 0.90, 4 | ok | none | 2 / 0 | |
    | boss_54000000 | Gyoubu, Demon | 0.34, 4 | ok | yes | 0 / 0 | Gyoubu's wire target is on his body (wire.rs, test at sim_tests 9040) |
    | boss_71001000 | Owl x2, Genichiro x2, Tomoe x2, Emma, both Isshins | 0.91, 4 | ok | yes | 115 / 185 | castle top |

  - **Divine Dragon trees (open, a fight mechanic):** the grapple trees are characters, not map
    edges.
    - c5310 (NpcParam 53100001, entities 2500880-84) is warped by event 12505901
      (2004:42) onto the dragon's dummies 50 / 53 / 54 / 55 / 56.
    - The hook is SpEffect 3531500's wire target (chrWireVariationNo 53100, dummy 600, about
      8.8 m up the trunk).
    - Arena-frame hooks: (-40.0, 8.95, -62.1), (39.6, 8.95, -58.3), (-22.1, 8.95, -78.9),
      (-0.2, 8.95, -85.5), (21.0, 8.95, -77.2). They are 70-86 m out, on the side opposite the
      exported origin (24 m ahead of the dragon).
    - Needs the tree characters and their phase (event 12505906, anims 5025-5027, flag
      12505888) in the Dragon fight. That is for whoever owns its script; then check the start
      spot against the field.
  - **Exporter fixed (map.rs `export_grapple`):**
    - Map-owned edge files never matched: the code built `h<map>_<id>` while MSB collision
      models are `h<id>`. m25 alone has 906 hang / 584 hug edges, 0 grapple.
    - `near()` now also requires |dy| <= 30 m. That drops the Dragon's point 277 m below.
    - **Geometry-only re-export, done 2026-10-13:**
      - Exporter `MAP_EDGES_ONLY=1` writes only the arena json's `grapple` / `ledges`. Before
        writing anything it lists the objects it still needs ("objects to unpack"), which the
        script unpacks before running again.
      - `python tools/boss_arenas.py -edges` runs it over all arenas. md5 of every other
        map_boss_* file is unchanged. Jsons backed up in scratchpad arena_bak.
      - Vertical bound (overseer: Sekiro values only, no hand-picked 30 m): 80 m = the largest
        WirePointSearchParam findDistanceFar (50) + the largest WireVariationParam
        extendWireActionDistance (30). The runtime `map::grapple_target` applies the real
        search limits (kb/grapple.md section 2).
      - Grapple counts, before -> after:
        - Castle top 115 -> 115 (2 at y -94 / -83 dropped, 2 new); ledges 185 -> 299.
        - Monkeys 208 -> 148 (51 at y 893-951 dropped, the floor far above; the 6 at y 21-30
          that a 30 m test had cut are kept).
        - Divine Dragon 1 -> 0 (y -277).
        - Old Dragons 2 -> 92 (both old ones at y -280 dropped; 32 distinct hooks from objects
          the first export never unpacked).
        - Unchanged: Monk 21 / 4, Butterfly 0 / 7, Apes 29 / 10, flower field 0.
      - No map-owned edge file holds a grapple record on these maps (0 map edges).

- 2026-10-13 **FX coverage sweep, regular enemies and Wolf** (scratchpad bossfx/sweep2.py):
  - 62 regular characters (outside contributor's and boss chrs excluded), 347 ids. The 8
    archive-only ids are now converted (`fxr_extract.py --ids`; extracted/fxr 1268).
  - 13 unresolved: TAE args 3 / 5 / 21 (not ids), 9 foot 102xxx (row 2, not shipped), and
    615102 (in no archive).
  - Wolf: 553 ids (TAE, bullets, SpEffectVfx, HitEffectSfxParam), 522 converted, 0
    archive-only. The 31 unresolved are in no archive: unused HitEffectSfxParam columns
    206000-254004, plus 30011.
- (old) **Hit effect turn by attack direction: parked (gap, vfx.rs).** HitEffectSfxAngleParam (param
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
  either. TAE 110 "SpawnFFX_110" (ids 2 / 3 / 5, c1320 / c5000 / c7200) is not an FXR id: it
  clears that MultiSfx slot (2026-10-13, see 4.44 "boss effect items").
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

### 4.39 Bug hunt round 19 (2026-10-12)

Round 19 finders on the HUD, EMEVD map events, enemy bullets and the extractor export: 5
confirmed, 2 refuted (enemy bullet knockback facing; NPC bare-judge variation +1).

- **Firecracker SpEffects never landed** (VFX and Maps, fixed): hit_enemy adds the bullet's
  SpEffects (230110 / 230111) before the burst branch returns. Test
  a_firecracker_burst_puts_its_sp_effect_on_the_enemy.
- **Resurrection nodes fixed at 2** (VFX and Maps, fixed): hud.rs ResRow + sync_res_nodes, one
  node per config player.resurrections, only spent ones dim. Test
  hud::res_node_tests::the_resurrection_nodes_follow_the_configured_max.
- **ai_hold ignored timed SpEffects** (Bosees, fixed; see 4.42; was OPEN). enemy.rs ai_hold `has(sp)` never reads
  e.clash, so the Finger Whistle (bullets 790004-6 -> 230518, 790204-6 -> 230519, no damage)
  and the firecracker can never wake a held MSB enemy (both are in every patrols.json wake
  list). Fix: also test e.clash as enemy_refs does. Check whether Bosees already did it.
- **Enemy bullets ignored isUseSharedHitList** (Bosees, fixed; see 4.42; was OPEN). enemy_bullet.rs fly gives every
  pellet of a fan its own full hit (Valley shotgun at 2-4 m: 2-3 x 48 HP). Give each
  spawn_row a chain id shared with its children, skip Wolf once hit in a shared chain
  (prosthetic.rs SharedHits does it for Wolf's bullets).
- **DecalParam rows of npcs-exported enemies** (Bosees, fixed + re-exported; see 4.42; was OPEN). export_decals only scans
  combat_data's chrs, so TAE 138 stains of c1180 / c1190 / c1240 / c1470 / c1500 / c1700
  (801012, 802000...) have no row or png and gore.rs skips them. Collect TAE 137-139 decal ids
  per exported chr into its params and write the pngs; re-export.

### 4.40 Bug hunt round 20 (2026-10-12)

Round 20 finders on Wolf's movement, the grapple, stealth sight and the hit resolve order: 5
confirmed, none refuted.

- **Locked-on legs-only move slid toward the target** ("bug fixes", fixed): move_velocity keeps
  the shown clip's direction (F / B / L / R from the facing) when the stick is let go.
  Player.speed_level is pub(crate) for the test
  a_released_locked_on_back_move_does_not_slide_toward_the_target.
- **Grapple from a perch fell off** ("bug fixes", fixed): the walk-off FreeFall check skips wire
  states while a wire is set (the launch fires on the tick SetNoGravity ends). Test
  a_grapple_from_a_perch_flies_to_the_next_point.
- **Arc to a high ridden point overshot** ("bug fixes", fixed): wire.rs steer zeroes the
  horizontal speed of a ridden flight when its timer ends. Test a_grapple_to_a_high_point_lands_on_it.
- **MSB enemies used the first placement's NpcThinkParam** (Bosees, fixed; see 4.42; was OPEN). tools/patrols.py skips
  the Part.Enemy ThinkParamID at +0x8; export it, carry it in MsbSpot and use it for that
  enemy's think row (stealth, logic, battle goal), kept across respawns. Example: c1010
  npc_row 10102210 msb 1110252 on m11_01 should use think 10102200.
- **The second window of one tick was judged against the deflect the first one started**
  (Bosees, fixed; see 4.42; was OPEN). combat.rs resolve: play_state resets the defender's anim, so a two-blade deflect
  (c1700 Attack3076 judges 160 + 161) turns the second into a Block. Snapshot the defender's
  guard inputs per pair before the window loop. (Bosees may have code for this without a test.)

Pushed to FFwLo/RustSekiro as 265e142 (rounds 10-20 and the sekiro-rs atmosphere ports; config.toml
left out); sekiro-collab WORK.md got three FwLo Done rows (ef21448). Round 21: 4.41. Next round 22, on areas not
hunted yet (world.rs / duel.rs, enemy/script.rs regular acts, sound.rs events, debug_menu.rs).
The round 20 workflow script is kept as tools/bug_hunt_round.js: change its name, AREAS, the
HANDOFF section range and the known-open list, then run it with the Workflow tool.

### 4.41 Bug hunt round 21 (2026-10-12)

Round 21 finders on world.rs / duel.rs, the AI script API (ai.rs; enemy/script.rs is the EMEVD
runner), sound.rs and debug_menu.rs. The user was away ("continue the work ... don't stop");
nothing committed.

- **Debug menu prosthetic slots 2 / 3 edited the wrong slot** ("bug fixes", fixed): the packed
  config list (player::equipped_tool needs it packed) moved a slot-3 tool to slot 2 and slot 3
  always restarted at the first tool. debug_menu.rs keeps its own 3-slot layout (tool_slots),
  used while it packs to the config list. Test a_tool_set_in_slot_3_stays_in_slot_3.
- **Debug menu kept the start-up enemy after a Fight pick** ("bug fixes", fixed):
  sync_enemy_choice re-points the pending type / outfit whenever config.enemy (chr, npc_row)
  changes (enemy::spawn_encounter swaps it in memory); Enter on Outfit no longer writes the
  start-up enemy back.
- **IsInsideTargetEx ignored its observer** ("bug fixes", fixed): the call is (observer, target,
  dir, angle, dist); 61 sidestep picks use (TARGET_ENE_0, TARGET_SELF, R, 180, 999) and the
  101200 / 125000 parry gates (ENE_0, SELF, F, 90, 3) = "am I in front of the player". ai.rs
  inside_ex measures from the observer's facing; Snapshot has target / target_fwd (enemy.rs
  Rival.fwd, Wolf's forward, zero for a caution spot = range only, gap). Test
  inside_target_ex_looks_from_the_observer.
- **CheckDoesExistPath returned the number 0** ("bug fixes", fixed): neither `== true` nor
  `== false` in Lua 5.0, so 126000 Act16 and the 720000 approach branches never ran; stubbed
  true like CheckDoesExistPathWithSetPoint.
- **Options volume / SE sliders ignored without FMOD** ("bug fixes", fixed): the PCM fallback
  played everything at a fixed 0.6. sound.rs fallback_volume sets SoundVolume = 0.6 x volume/10 x
  se/10 (as set_mix does for FMOD). Test the_pcm_fallback_follows_the_volume_and_se_sliders.
  Dropped: unslotted TAE 129 sounds never tracked (no proof any loops).
- **Respawn kept the firecracker cool time and timed SpEffects** (Bosees, fixed): see 4.42.
- **Respawn stepped back along world -Z** (Bosees, fixed): sandbox line-up enemies came back 2 m
  toward Wolf; see 4.42.
- Refuted / not reported: duel.rs single_mut with several enemies (a check tool); IsLookToTarget
  two-arg form and IsInterruptSubGoalChanged (unreachable); player_cheats vs status tick order.

### 4.42 Bosees fixes, rounds 19-23 (2026-10-12/13, user away)

The Bosees items of 4.39 / 4.40 / 4.41 / 4.45, each with a test; the new tests were checked to
fail with the bug put back (drill, respawn cool time). Nothing committed.

- **ai_hold ignored timed SpEffects** (4.39): ai_hold `has` also reads e.clash (Enemy::add_timed:
  the Finger Whistle's 230518 / 230519, the firecracker's). Test
  the_finger_whistle_wakes_a_held_enemy (230518).
- **Enemy bullets ignored isUseSharedHitList** (4.39): every spawn_row takes a chain id its
  children share; fly skips Wolf once a shared chain hit him (enemy_bullet.rs `shared_hit`, a
  `false &&` had left it off). Test a_fans_pellets_hit_wolf_once (c1190 11902100: 1 of 5).
- **DecalParam rows of npcs-exported enemies** (4.39): exporter `decal_ids` / `decal_rows` /
  `decal_textures`; export_npcs writes each chr's TAE 137-139 + AtkParam decal rows into its
  params "DecalParam" (load_kind merges them) and the pngs. Re-exported all 102 npcs chrs
  (c1350-c7600 on 2026-10-12 with tools/sekiro-extract target/bosees release): 208 rows, decal/
  130 -> 132 pngs (c1500: 801112 / 801122 / 803030 / 803031; c1700: 801110 / 801120).
- **MSB ThinkParamID** (4.40): tools/patrols.py exports Part.Enemy +0x8 as "think"; MsbSpot.think
  -> Enemy.think_override at spawn when the kind has the row (msb_think), kept across respawns;
  Enemy::think_id serves stealth, targeting and the battle goal. Test
  an_msb_enemy_runs_its_own_think_row (1110252 -> 10102200).
- **Same-tick second window** (4.40): combat.rs resolve keeps the first swing window's outcome
  per pair (tick_outcome); a later window of that tick reuses it only when it was a Deflect and
  the window is deflectable (the StandDeflect reaction has no just-guard window). Block / Hit are
  judged again: reusing a Block broke the_flame_vent_goes_through_a_guard. Test
  both_blades_of_a_two_blade_swing_are_deflected (c1700 a000_003076, judges 160 + 161).
- **Respawn kept the firecracker cool time** (4.41): the in-place respawn clears e.clash
  (the reaction cool times live there; burst_until was dropped in round 30) and the guard count. Test a_respawn_forgets_the_firecracker_cool_time.
- **Respawn stepped back along world -Z** (4.41): respawn_spot(start, yaw, back, ..) steps back
  along Ry(start_yaw) * +Z. Tests respawn_tests::a_respawn_steps_back_along_its_start_facing and
  the updated a_respawn_stands_on_the_floor_at_its_start.
- **Enemy bullets hit Wolf's corpse** (4.45): fly treats hp <= 0 like an iframe dodge (no hit, no
  build-up, not used up). Test enemy_bullets_pass_through_wolfs_corpse (fires once Wolf lies in
  GroundDeathLoop_F past its TAE 950 f0-4, at his position then; control run alive).
- **Debug drill timer drained once per enemy** (4.45): Enemy.drill_timer (mode, seconds) per
  enemy, restarting when the menu's drill mode changes; EnemyDebug.timer is gone. Test
  each_drilling_enemy_waits_the_full_interval (3 c1020s, 1.5 s: waits were 30 ticks, now >= 85).
- Section 5 for Bosees: shadow hiding needs MapVisibility in the `.hit` (the map extractor, VFX
  and Maps) and only the boss arenas have Dark parts (the gate has none), so it is left; sound
  gaps are VFX and Maps' (PLAN.md). AI pacing still waits on a like-for-like recording.
- **Round 25 sim tests** (asked by "bug fixes"): a_level_0_hit_leaves_the_doll_still_and_wolf_recoils
  (c5040: doll still, Wolf's full recoil; fails with `still` off). a_cautious_enemy_listens_to_a_new_sound
  found a real bug, fixed by "bug fixes" in ai.rs (native_terminate cleared the sound target on
  the 46 plan swap; FUN_1405c3770 clears only with no 46 that frame; see 4.48).
- **a_feigning_wolf_is_dropped_by_the_enemy** (round 27, for "bug fixes"): not an engine bug. The
  test app's enemy is the passive practice partner once in BATTLE (passive_or_fallback runs no AI
  script: the 405000 walk-in / idle flicker), so HiPrioritySetup never ran. The test now makes it
  aggressive; it drops Wolf in GroundRevivalStayLoop_F, and fails with 110010 hidden from it.
- **B1 first-hit vs clips, regular enemies** (2026-10-13): the user's hitbox playlist clips
  (Okami Warrior c1310, Taro Troop c1180 kanabo / bell / greatshield, Nightjar c1450) against
  the TAE windows: no
  mismatch over 3 frames, spacings / lengths within +-1 (kb/damage.md table). No code change.
- **Round 28** (combat.rs): enemy guard posture takes its NpcParam <attr>StaminaDmgRate
  (enemy_guard_posture_rate; test a_guarding_enemy_takes_its_npc_posture_rate).
- **Round 29** (combat.rs): Wolf's hit level goes through damage_motion_level (his 1001
  SpEffects; test wolfs_wake_up_super_armour_keeps_him_from_restaggering). The umbrella guard
  break plays Stand/AirSubGuardDeflectBreak via player::sub_anim(.., 76): a076_405400 / 421400,
  not the clipless a070 keys (unit test an_umbrella_guard_break_plays_the_umbrella_break).
- **Round 29 bullets** (enemy_bullet.rs fly; BulletSpec +hit_radius_max, spread_time,
  dmg_hit_record_life_time):
  - D1: a carrier with opposeTarget or a HitBulletID runs the contact test (test
    a_poison_spit_ends_on_wolf_and_its_cloud_comes_there, c1100 11000020 -> 11000021).
  - D2: a hit is remembered dmgHitRecordLifeTime s, per bullet (last_hit) and per shared
    chain (shared_hit now maps chain -> until). The shared entry was what blocked the re-hit.
    Test a_fire_pool_burns_wolf_again_after_its_hit_record (c1030 10300240). gap: inferred.
  - D3: radius hitRadius -> hitRadiusMax linearly over spreadTime (test
    a_spreading_bullet_catches_wolf_beside_its_path, c1250 12500100). gap: names only.
- **Round 30**:
  - (1) Spear pull: apply_knockback caps a negative distance at the bodies' gap (the peer's
    Actor::limit_knockback). Test a_spear_pull_draws_the_enemy_to_wolfs_capsule; the negative
    leftover moves no attacker (gap).
  - (2) armor_rates / guard rates read the enemy's timed SpEffects (combat.rs timed_rates:
    9800 x1.125; HP by the physical rate, gap for elements). Test
    a_firecracker_burst_makes_the_enemy_take_more.
  - (3) The AI snapshot adds Wolf's and its own sp_chained (actor.rs run_sp_effect_chains):
    the revival's 110032. Test the_enemy_sees_wolfs_revival_chain.
  - (4) wolf_starts_an_item skips ItemGourdDrinkFailed (test a_crouched_drink_raises_use_item).
  - The sword path's SP_DAMAGE_FIRE: a Player swing with spAttribute 2 (the Sparking Axe's v7302,
    the Spear's v7804) on a ref-1000040 enemy plays DamageFire after an igniting hit's
    FireReaction, with the 1000240 cool time. Test the_sparking_axe_sets_a_zombie_flailing.
  - Enemy.burst_until removed (nothing reads it).
  All mutation-checked except the umbrella call site, whose unit test checks the keys only.
- **Round 31** (combat.rs / enemy_bullet.rs / enemy.rs):
  - dodged(): a TAE 954 IFrameType covers a hit only when it equals the AtkParam trackType
    (+0x193; 140b6.c:4234-4241 bit test, 140b5.c:10693). 950 / 951 stay full. The stand-ins are
    gone (type 2 = everything, 6 = atkType 2, 7 = throwFlag, 1 = a 980 warning). Tests
    a_side_step_does_not_dodge_a_sweep, a_back_step_dodges_a_perilous_thrust (locked on: unlocked
    every step is GroundStep_F), a_jump_clears_a_sweep.
  - Mikiri: combat.rs mikiri_atk = disableGuard 1 and (atkType 2 or trackType 6), used by dodged,
    resolve and the Thrust drill. Test the_zombies_spear_thrust_can_be_mikirid (c1500
    a200_003011, atkType 0 / trackType 6). The warning judge does not tell sweeps from thrusts:
    the Ochimusha's Mikiri'd thrust a000_003013 fires 980 (a first "not under 980" rule broke
    mikiri_on_the_ochimusha_..., so it was dropped). Across all 102 chrs each of 980-983
    heralds sweeps, thrusts and grabs alike, so the log line and the debug menu now name the
    kind from the attack (enemy::perilous_kind: throwFlag / trackType 7 grab, 1 sweep,
    mikiri_atk thrust); test a_perilous_kind_comes_from_its_attack.
- **Sweeps after R29-R31** (2026-10-13, release): all_enemies_smoke 102/102; all_enemies_full
  94/110, the same as the 2026-10-12 07:50 run. The 16 are the non-fighters plus c5200 and
  c1000:12600050, which both already failed then. Both are boss rows: not Bosees'.
- **"Deathblow start commits without re-checking" (OPEN in 4.29): as the game, closed.** Static
  trace: the throw judge FUN_140be2d60 (140be.c:3535-3640) tests the defender's throwable
  code (act 161) only when the pair is made; FUN_1407fa680 / FUN_1407fdb90 register it with no
  defender check; Wolf's W_ThrowKill needs env 274 (throw state 5 + the partner's flag) and his
  own ref 211 (c0000_transition.lua:5591-5595); c9997 ThrowDefCommonFunction (3980-4040) leaves
  only on death, damage, a fall, an event or the anim's end. So a started deathblow kills even if
  the break would run out. Ours does too: Mode::Broken stops counting while the start throw
  holds the enemy. Test a_deathblow_pressed_at_the_end_of_the_break_still_kills (test hook
  Enemy::set_broken_left). gap: where the exe sets the collapse (103) / ninsatsu (104/105)
  throwable codes is not found.
- **The 危 kanji is an effect**: SpEffect 211000 (0.1 s) -> SpEffectVfx 48100 "Kanji 危"
  (initSfxId 600035, initDmyId 220). enemy_bullet.rs fly plays FXR 600035 on Wolf's dummy 220
  when the warning bullet reaches him; the HUD text (Perilous::raise(kind, text)) stands in
  only when the effect is missing. (2026-10-13 update: extracted/fxr/600035.json is converted,
  fxr_extract.py SP_EFFECTS; the effect now shows, see the VFX entry "the perilous kanji is the
  game's FXR 600035".) Test the_perilous_kanji_is_its_effect_on_wolf (skips until then;
  mutation-check it once the effect exists). (closed, Bosees) The c1220 / c1470 trackType-6 kicks are
    thrusts in the game's data too. 1409e.c:7395-7434 is the throw-on-hit pairing
    (FUN_1409e5fd0), not a Mikiri gate. The data's thrust marker is SpEffect 104000
    ("critical symbol, piercing", stateInfo 909) in AtkParam spEffectId4, on 99 rows, all
    trackType 6. That includes the six atkType-1 kicks (c1220 12201501, c1470 14700180 /
    14700222), each paired with an xxx940 / 941 "Mikiri dummy" (atkType 0, trackType 6, 0
    damage). combat.rs `mikiri_atk` (disableGuard && (atkType 2 || trackType 6)) agrees. Not
    found: the exe's flag-125 acceptance test, and how 104000 / 104001 / 104002 become the
    kanji (table-driven). Our rule covers every 104000 row, plus the 940-942 dummies and 16
    unguardable thrust rows without the marker: the Ochimusha's treading thrust 10102120, the
    spear monk's 10500003, the zombies' bamboo spears 15000400-402, c7000's 70000160 / 161,
    c5060's dummies, and 0-damage system rows (23 / 24, c1300 1300010x). Kept, since nothing
    in the exe excludes them.
  - The 危 HUD kanji goes up on the warning bullet's hit on Wolf (Bullet <chr>980 / 982 / 983,
    2 m, 0.1 s, SpEffect 211000), not at the TAE start for any enemy. The log line stays at the
    TAE start. Test the_perilous_kanji_needs_its_warning_bullet_to_reach_wolf (1.5 m vs 8 m).
    gap: SpEffectVfx 48100's on-screen time is not read (0.8 s kept).
- **Grab escape, the grabber's side**: the OPEN in 4.50 is done (player.rs round 28 has Wolf's
  side). resolve_throws: once Wolf is in ThrowEscape, an enemy in ThrowAtk<id> plays
  ThrowRelease<id+2> (c9997 ThrowAtkCommonFunction env 275 = own flag 70 + throw state 3/4,
  FUN_140b39e40). mashing_breaks_the_zombie_restraint checks ThrowRelease4122. Traced (re-scout):
  escHp at ThrowParam +0x2c; a hit on the victim while its flag 68 is up takes -damage off
  escHp (FUN_140b68ff0 140b6.c:4419-4425). It never applies: no c0000 clip carries flag 68
  (round 31, player.rs grab-escape comment), so there is nothing to model.

- **R32 (bug fixes), combat.rs**: (B1) a guarded hit with no guard clip for its level
  (GuardPick::None: no GuardDamageLarge on c1360/c1361/c7400/c7401/c7510, ShieldBlock chrs with
  no GuardDamage clips, guardLevel 0's NONE column on c1250/c1260) now goes on through ExecDamage
  (c9997.lua:2252-2301): guard_fall_through plays the level's damage reaction, else the
  ExecNoSyncAddDamage flinch. (B2) no_sync_add_flinches: only a minimum (8), aerial or
  rank-rejected hit flinches additively (c9997.lua:2901); level 0 (flag 24 / super armour) no
  longer twitches. (B3) the "superArmorDurability 0" comments are corrected. Test
  a_guarded_hit_without_a_guard_clip_takes_its_damage_reaction (both bugs mutation-checked).
- **Super-armour meter, traced by re-scout** (implemented later in 4.42, "Super-armour pool"): an always-on pool
  (CSSuperArmorDurability) active while NpcParam superArmorDurability (+ stateInfo-155
  changeSuperArmorPoint) > 0. A hit drains it by the attack's SA damage (NPC: AtkParam
  atkSuperArmor +0x66; PC: weapon saWeaponDamage +0xCE x atkSuperArmorCorrection +0x56 / 100)
  x SpEffect saReceiveDamageRate (FUN_140bdb470, 140bd.c:8941). FUN_140bdbaf0 (140bd.c:8648)
  lets stateInfo-120 SpEffects (c1180 5366 / 5367) into the spCategory-1001 dmgLv pick
  (FUN_140bfd980, 140bf.c:9502) only while the pool is > 0 and unbroken. combat.rs's
  damage_motion_level skips them all, which is the same as a broken pool. Break: knockback
  NpcParam superArmorBrakeKnockbackDist (140bd.c:8795). Not traced: the max init (vfunc 0x2c8),
  the recovery (vfunc 0x2d8 x superArmorRecoverCorrection, delay), and 5366's source (only 5367
  is in TAE: c1180 a000/a200_020110). Every enemy atkSuperArmor in our data except c1350 (36) and
  c7020 (120) is 0, and saWeaponDamage is not extracted. Implement once init and recovery are traced.

- **R33 (bug fixes), AI env stubs, ai.rs + enemy.rs think**: (A1) IsLockOnTarget(TARGET_ENE_0 /
  LOCALPLAYER, TARGET_SELF) is Wolf's lock-on on the AI chr (Snapshot.locked_on from camera.rs
  LockOn); it was always false (the Rappa's 136000_battle.lua:116 keep-distance branch, 115000:281,
  101000_logic:6). Test is_lock_on_target_is_wolfs_lock_on_me. (A2)
  GetExistMeshOnLineDistSpecifyAngle(Ex) returns the free distance along the ray (mesh_line_dist:
  the map raycast at centre height less the radius, and a floor step > STEP_HEIGHT ends it) instead
  of echoing the asked distance, so SpaceCheck (FDP:43-50) refuses backsteps / sidesteps into walls
  and off ledges. Without a map everything is free, as before. Inferred stand-in for the exe's
  navmesh walk. Test mesh_line_dist_stops_at_walls_and_ledges. Both mutation-checked;
  all_enemies_smoke green. (A3) gap: CheckDoesExistPath stays true (no navmesh). The same branch
  catches Wolf hanging / perched via SpEffects 109220 / 109221, and a straight-line stand-in would
  misfire behind pillars. (A4) GetTeamOrder(ORDER_TYPE_Role) is the exe's team order (traced:
  FUN_140640050 14064.c:207-528 via FUN_1405b1b20): every 0.1 s each target's chrs, nearest
  first, are 1 Attack, 1 Torimaki, the rest Kankyaku (the tunables' registered defaults
  14064.c:2715-2785; no shipped override found; the weighted mode is off by default and not
  modelled). enemy.rs assign_team_roles uses last frame's targets (Enemy.team_target).
  CallHelp (type 1) stays 0 (writers not traced). Test team_roles_go_nearest_first_per_target
  (mutation-checked). Groups of 101000 / 136000 enemies now surround and spectate as in Sekiro.
- **Perilous kanji**: VFX extracted FXR 600035, so the_perilous_kanji_is_its_effect_on_wolf now
  asserts the effect exists instead of skipping, and is mutation-checked.

- **R35 (bug fixes), Sabimaru poison reaction, combat.rs (shared system)**:
  special_poison_reaction = c9997 GetSpDamage SP_DAMAGE_POISON_REACTION (894-895) ->
  ExecDebuffReaction W_SpecialPoisonReaction, rank 4 (2969-2976). It needs:
  - the poison set off on this hit ("POISONED"; STATUS_SPECIAL_POISON 0 read as the burn analogue,
    inferred);
  - ref 1000220 on the chr (resident 9080);
  - ref 1000032 on the hit (the Sabimaru's 0-duration build-ups 9003-9009);
  - the clip.
  It is checked first (GetSpDamage's last assignment wins), on hits and through a guard. A
  debuff reaction through a guard now also skips guard_pick (ExecDebuffReaction's TRUE ends
  ExecDamage before ExecGuardBlock), so a burning blocked enemy keeps its FireReaction. Only
  c1310 has the clips, and c1310 is now the outside person's (bosses split 2026-10-13, PLAN.md).
  The code is the generic HKS rule. Test the_sabimaru_poisons_an_okami_warrior_into_her_reaction
  (mutation-checked).
  - The guard-skip now has a test (Bosees, 2026-10-13; was a gap):
    a_blocked_hit_that_ignites_plays_the_fire_reaction. Wolf's sword is given 9105, raised so
    the first blocked hit ignites the General; he plays FireReaction, not his guard.
    Mutation-checked: with `!debuffed` dropped he plays GuardDamageSmall_RighttoLeft.
  - Death at 0 HP: see the rule change below.

- **Rule change (from the exe trace, "bug fixes" agreed; the user can veto): a regular enemy
  dies at 0 HP.**
  - Exe: SetHP FUN_140bd64e0 (140bd.c:4187-4194) holds HP at 1 only while IsNoDead
    (FUN_140bd5df0, 140bd.c:3381-3409): deathblows left (data+0x25c = NpcParam ninsatuNum,
    140bd.c:5116), an active stateInfo-143 SpEffect (5830 / 5831 / 220030-2 / 3137015), or
    debug bits. Otherwise c9997 IsDead (966-977) takes ExecDeath.
  - Ours: Enemy.mortal (raw ninsatuNum 0) + dies_at_zero_hp (no stateInfo-143 SpEffect);
    combat.rs's break loop then calls die_at_zero_hp instead of on_posture_break and logs
    "KILLED". Rows with red dots (the two-dot General 10200000, bosses) keep the deathblow.
    This replaces the 2026-10-06 rule (docs/kb/guard-deflect.md:74) for ninsatuNum-0 rows.
  - ExecDeath (1896-2120), modelled branches only: DeathBlast (levels 4 / 10 / 11),
    DeathStartSpecialPoison (refs 1000220 + the hit's 1000032 setting poison off, or 1000031
    9020), DeathStartFire (a spAttribute-2 hit on ref 1000040; no regular row has a
    physical-type death clip), else DeathDefault0/1 at random (IndexDeathVariation).
    Enemy.last_hit records the inputs on the sword path. Prosthetic and bullet hits keep the
    last sword hit's, or the default.
  - common.emevd's 5830 is an item lot (events 289 / 290), not the SpEffect.
  - gaps:
    - the unidentified IsNoDead bit (chr+0x50+0x50+0x90 bit 1), taken as off. If it turns out to
      keep NPCs alive, it is one switch back (Enemy.mortal).
    - ExecDeath's land / aerial / event-20000 / hide / back-reality / upper / fling / weak /
      physical-type branches.
  - Test a_regular_enemy_dies_at_zero_hp_and_a_red_dot_one_opens_the_deathblow (both clauses
    mutation-checked). The deathblow (26), death (30), kill, poison, burn, hit (34), fight,
    shuriken and bullet filters and all_enemies_smoke stay green; no existing test relied on
    the old rule.
  - Boss rows checked (npc_names.json): every mini-boss and main boss has ninsatuNum >= 1 and
    keeps the deathblow, except 11300000 Armored Warrior (the outside person's; the game ends
    him with the bridge push, bosses-handover.md:290), 12600050 Folding Screen Monkeys and
    53000000 Old Dragons (scripted set pieces). These three are now mortal at 0 HP: check
    them when their fights get scripts.

- **R36 (bug fixes), attack AI sounds, traced by re-scout**:
  - AtkParam AppearAiSoundId (+0x98) and HitAiSoundId (+0x9c) are now emitted. Read by
    data::atk_ai_sounds from extracted/json/params (Attack.atk_param_id, already exported).
  - Appear: once per attack issued, at the attacker (FUN_1409a1290, 1409a.c:2197). Ours: each
    AttackBehavior window opening this tick, in combat.rs resolve. It is not in think, which runs
    before the anims advance and missed windows that a hit cut off.
  - Hit: once per target, after the damage call (FUN_1409e68e0, 1409e.c:3685), so guarded and
    deflected hits emit too. Ours: right after hits_done.push, at the defender. The owner is
    the attacker (inferred; the exe emits through the target's module).
  - enemy.rs AiSounds holds entries for their lifeFrame; the same owner's same id refreshes.
  - The listener (FUN_140610470, 14061.c:1181), ai_sound_heard in think:
    - the row's self / oppose / friendly flag by the relation (teams_hostile; Wolf is hostile to
      all);
    - wolf_sounds' range (hearing rate x radius - cut < distance + ear);
    - then targeting.hear (rank rules as before).
  - So an NPC's swing (2000025, 10 m) and hit (2000020, 20 m; friendlyTarget + bCallFriend)
    call its allies to the fight, and Wolf's 2010 / 2110 (2 / 4 m, oppose) reach enemies.
  - gaps: bCallFriend's owner check (FUN_14060d260 ..., not decoded); bRestrictSamePlacementGroup
    (one group per fight here); bSpEffectEnable's owner scale (1); a bullet's Appear (done
    2026-10-13, see the end of 4.42).
  - Test attacks_make_ai_sounds_that_call_allies (the Appear / Hit emission and the team filter
    each mutation-checked). stealth, sound, group, ai::, unaware, patrol and all_enemies_smoke
    are green.

- **R36 B (bug fixes), blue lightning under ref 289**: a spAttribute-10 hit while Wolf's anim
  holds ref 289 (SpEffect 100235 "super armor for weak charge damage", the air arts a110_3162xx
  / 3167xx) takes c0000_transition's empty branch (1099-1103 in the air, 1157 standing): no
  charge and no reaction (combat.rs weak_charge_armour; the damage still lands). We charged him
  for every airborne 6 / 10 hit. Strong lightning (6) still charges. Test
  an_air_art_shrugs_off_blue_lightning (mutation-checked); the lightning filter is green.
  The wiring in resolve now has a sim test (Bosees, 2026-10-13; was a gap):
  blue_lightning_leaves_an_air_art_going (see 4.42 end), all three gates.

- **R36 C (bug fixes), a bullet's SpEffects, traced by re-scout**: the exe fills a bullet hit's
  SpEffect slots from the Bullet row only (FUN_1410b7910 1410b.c:5026 -> FUN_1410b6880
  1410b.c:8409-8417). AtkParam spEffectId0-4 are read only for non-bullet attacks (1409a.c:337 /
  403, 14099.c:9657-9670). A second application of a build-up row would subtract again, with no
  merge (FUN_140bd9d30; the dedup FUN_140c02000 only blocks a live stateInfo-6 entry).
  - Ours applied both lists. enemy_bullet.rs now clears the AtkParam's slots on the attack it
    hands to combat.rs.
  - 15 rows share an id: 14 Demon of Hatred 70200xxx rows with burn 9116, plus 10900900 with
    3109001. 54000371 / 383 differ (9124 / 9112), so they were never doubled.
  - Test a_bullet_burns_once_not_through_its_atk_param_too (mutation-checked: the double 250
    set Wolf on fire at once). The bullet, burn, poison and status filters are green.
  - (traced, Bosees; see "Attack-occurrence SpEffects" below) FUN_140bfc170 is the attacker's
    attack-occurrence lookup. Children: FUN_140b15570 (140b1.c:5604) copies the parent's launch
    block. The child's hit is its own attack: its Bullet row spEffectId0-4 go through the same
    apply FUN_140b6e6a0, and the AtkParam's spEffects are not read for bullets
    (1410b.c:8409-8417). There is no parent / child dedup, as ours. Data no-ops: AtkParam
    disableHitSpEffect (clears the bullet slots) and effectTargetAttacker in bullet slots, since
    no enemy bullet row uses either. Not traced: the attack manager's hit-list expiry
    (dmgHitRecordLifeTime / isUseSharedHitList).

- **Main-boss sweep (Bosees, 2026-10-13; the 22 main bosses are ours again)**: all_enemies_full
  on c5000-c7400 plus the boss rows: 19 of 22 passed. The three that failed, as before:
  - **12600050 Folding Screen Monkeys**: the row is the bar holder (c1000, think 10000000, hp
    100). Its battle goal 11000 is GOAL_NanimoShinai_11000_Battle (common_011000_battle.lua only
    waits), so it never fights. sim_tests `row_fights` now counts battle goal 11000 as a
    non-combatant, in both sweeps. The monkeys themselves are c1260.
  - **c5200 Divine Dragon**: a harness artefact, not the AI.
    - After 8 s without a hit, fight_until_hits walked Wolf onto the enemy's centre (0.05 m,
      for the hidden ground zombie). The dragon's smashes 3007 / 3008 (AtkParam 120 / 130,
      capsule from hand dummies 10-11, r 1.5) land in front of it, so they missed.
    - Its wind 3006 (Bullet 52000830, atkPhys 0, atkBehaviorId 19) hurts nothing, so it was
      never counted.
    - Now Wolf only closes in on an enemy that has not attacked yet. A new reaction state
      (Damage / StormJumpBack) counts as a hit. With "bug fixes"' R39 wind reaction, c5200
      passes (hits 2).
  - **53000000 Old Dragons shared body: fixed (exporter).** Its AI (530000_battle.lua) read flag
    12505901 as true, so it took Act11 (weight 600: follow TARGET_TEAM_LEADER and strafe) and
    never attacked. re-scout traced the cause through m25_00_00_00.emevd:
    - The Old Dragons fight is 12505801: action button 9821 on 2500806, cutscene 25000000,
      bar 905300 on 2500810. It ends in 12505885 when 2500810's HP reaches 0. That event then
      sets 9318 and 12505881, and the Divine Dragon starts.
    - 12505949 is only a shortcut: 12505882 waits on it and sets 9318, and 12505885 then skips
      the Old Dragons. No m25 or common event sets it. world_flags() turned it on because it
      is read but never set, so the row started in the Divine Dragon phase. 12505888 ends,
      then the tentacle event 12505901 ends, and its ID flag comes on.
    - 12505818-827, the never-set tail of 12505885's "all 12505803-827 on" batch, were also
      turned on. No event sets them; 12505803 sets only 803-817.
    - Fix: `tools/boss_scripts.py` FLAGS_OFF[53000000] = {12505949, 12505818-827}. Regenerating
      changed only that row; 52000000 keeps them, since its fight starts that way.
    - Test the_old_dragons_fight_before_the_divine_dragon (mutation-checked: with 12505949 put
      back, 12505901 and 9318 come on). The dragon filter (5) and the sweep (c5200,
      c5300:53000000) are green; the body now attacks itself (3 atk).
    - gaps: the HP sharing between the body and dragons 2500811-825 (SpEffects 3530020 /
      3530030, counter 12505828) is not traced, and neither are the 12505851 / 12505834 bodies.
  - Full all_enemies_full after these edits: 101 / 110, up from 94. No new failures. The nine
    left were all in the old list: Hanbei c1012, the handmaidens c1110 / c1111, c1260, c1300,
    c1320, c7200, c7430 and c7440.
    - (In that run c5300:53000000 "passed" only through the dragon's wind; fixed above.)
  - **c1260 Folding Screen monkey: a gap, not changed.** Its close-range AI is all map
    regions: 126000_battle.lua takes Act17 (weight 100, plus 24 at 1000) under 5.5 m, and Act17
    is IsInsideTargetRegion over the m20 MSB regions 99028xx. The sandbox has no regions
    (ai.rs answers false, like the Great Serpent's), so it waits and never attacks. Its catch
    (ThrowDef) works.
  - **The other sweep failures (Bosees triage, not changed):**
    - **Hanbei c1012**: 101200_battle.lua acts only on the lesson SpEffects 3100xx that the
      dojo events set. With none, it takes Act26 (wait). That is the data's behaviour.
    - **Fujioka c7430**: default row 74300082 (teamType 26, "Ara-ji Shop Clerk"), c1550
      anims, behaviour variation 11200. His combos 3000-3003 use judges 10 / 20 / 21 / 50 /
      270, but BehaviorParam has only 211200100-130 / 990, and there are no 2000000xx default
      rows, so the attacks do nothing. Row 74305081 (variation 15500) has them all.
      gap: the exe's behaviour lookup when the variation lacks a judge is not traced.
    - **Handmaidens c1110 / c1111**: hp 15, so Wolf's swings kill them (DeathDefault1, the
      R35 0-HP rule) before the harness's deathblow. They have no ninsatuNum, which is the
      game's behaviour.
    - **The nun c7440, Kuro c7200, the carp c1320**: conversation NPCs or set pieces.
    - **c1300, Fountainhead's strong noble**: see "Aging" below.

- **Aging (Bosees, traced by re-scout): the gauge is done; the aged form is "bug fixes"'.**
  The old "resist 999, likely cut" note was wrong. 999 is c1300's own NpcParam resist_curse.
  - Paramdex: stateInfo 116 is "Enfeeble"; registCurse "only works with stateInfo 116". The
    exe's resist type 3 is "curse" (Terror is registIllness / stateInfo 5).
  - 9600 "Aging_resistant scraping" (registCurse 40) is the only real curse build-up. It comes
    from the strong noble's breath Bullet 13000200, 12 ticks per 3001.
  - Wolf's resist is 200, recovering ResistRecoverPoint_Curse_Player at 20 a second.
  - Full, it applies 9600's cycleOccurrenceSpEffectId 9620 "Aging_invoking": 30 s, maxHpRate
    0.125, all cuts 99, changeStaminaRate -100, behaviorRefId 591 SP_EF_REF_AGING, then
    replace 9641.
  - c0000 has the whole aged form: c0000_transition.lua BEH_R_AGING_DAMAGE / W_StandAging /
    BEH_A_AGING_*, THROWABLE_STATE_ATK_PC_AGING 10.
  - status.rs: a fourth gauge, `curse`.
    - ResistRules reads registCurse * registCurseChangeRate and cycleOccurrence from
      SpEffectParam.json (the exported rows don't carry them), plus disableCurse and
      ResistRecoverPoint_Curse_*.
    - NpcParam resist_curse / curseGuardResist and EquipParamWeapon curse(Just)GuardResist
      apply as for the other types.
    - A fill starts the cycleOccurrence row (9620), logged "AGING".
  - enemy.rs think: an affliction's SpEffects are now on its wearer for the AI. HasSpecialEffectId
    / observe on TARGET_ENE_0 (Wolf) and on NPC rivals sees Wolf's 9620, poison 9045 and burns,
    as the game's check of every active SpEffect does. Before, it saw only TAE windows and
    sp_chained. 130000_battle.lua then stops breathing and takes Act03 (3007) / Act02 (3004
    grab).
  - Test the_nobles_breath_ages_wolf. Mutation-checked twice:
    - without the AI seeing afflictions, only the breath combo 3000 / 3001 / 3003 plays;
    - without the curse gauge, Wolf never ages.
    The poison / burn / terror / bullet / sabimaru filters and status's own tests are green.
  - gaps:
    - The fill -> cycleOccurrence step is inferred from the rows; the exe isn't traced.
    - 9620's maxHpRate 0.125 / cuts / stamina are not applied.
    - Wolf's aged form (anims, movement, the aged-Wolf ThrowParam 11300260-281) is player.rs
      ("bug fixes" told).
    - No HUD gauge for Aging.
    - The common.emevd / m10 / m11_02 / m20 uses of 9620 / 9640 (byte hits) are not decoded.

- **Finger Whistle madness (Bosees, with "bug fixes"' R43 cycle rule)**:
  - The exe rule: a row fires its cycleOccurrenceSpEffectId while its
    invocationConditionsStateChange1-3 gate holds (the owner wears a SpEffect of that
    stateInfo, FUN_140c060d0).
  - The whistle's bullets 790004-6 put 230518 on: 230518 -> 230510 (gate 959) -> 230515
    COMMON_SP_EFFECT_CONFUSE. 230515 has effectEndurance -1 and changeTeamType 29,
    TEAM_TYPE_Indiscriminate.
  - The minor version: 230519 -> 230530 (ref 1000035 FingerWhistleReaction) -> 230511 ->
    230516 CONFUSE_GHOST.
  - 959 is the stateInfo of 230500 "madness can be activated", NpcParam spEffectID9 on 80
    rows: dogs, chickens, monkeys ...
  - `prosthetic::madden` walks the chain on a bullet hit and puts a lasting row on the
    enemy's `sp_chained`. There the AI sees it: common_table_ai_common.lua:235 replans on
    CONFUSE.
  - `combat::team_of` now takes an active SpEffect's changeTeamType over NpcParam teamType.
    teams_hostile already treats 29 as hostile to all but 0 / 26 / 30.
  - `data::sp_cycle` reads the cycle / gate / team fields from SpEffectParam.json, since
    combat_data holds only 67 rows.
  - Test the_finger_whistle_maddens_a_dog_not_a_soldier. Mutation-checked twice:
    - without the gate, the soldier is maddened too;
    - without team_of's SpEffect read, the dog stays team 6.
    The tool / prosthetic / team / rival / firecracker / group / npc filters are green.
  - gaps:
    - What ends a -1 madness: done, see 4.42 "isWaitModeDelete".
    - The vfx 7900 / 7901 are VFX's: done 2026-10-13 (status.rs `lasting_glow`, 4.44).
  - Test a_maddened_dog_bites_the_soldier: with 230515 on, the dog takes the soldier's HP
    within 20 s while Wolf stands 30 m off. Mutation-checked: without 230515 the soldier keeps
    it.
- **Behaviour judges: no fallback, but worn SpEffects shift them** (re-scout of the exe).
  - FUN_140a12b80 (140a1.c:2158-2250) builds the NPC row as 200000000 + variation * 1000 +
    judge, with no retry on a miss. On a null row, 14084.c:8433 sets atkId -1, so the attack
    does nothing.
  - So the 304 placements whose variation lacks judges (Fujioka 74300082 / 11200 and others)
    really swing for nothing. That is the game's behaviour, not ours to fix. (Only the PC
    version, 140a2.c:3085, falls back, to (var / 100 + 1000) * 100000 + judge.)
  - The same function adds each worn SpEffect's addBehaviorJudgeId_add when its
    addBehaviorJudgeId_condition is judge / 100 (lines 2191-2236; the field names are inferred
    from the offsets +0x165 / +0x314). This was missing.
    - The Shura samurai c1070's a000_003014 adds 3107010 (poison, +5) and 3107011 (flame, +7),
      gated on his resident 3107040 (950) or 3107041 (951). For 15 s his 1xx judges become
      105 / 107 ... (AtkParam 10700105 with 9013 poison, 10700107 with 9119 burn).
    - The Nightjars c1450 wear 3145040 (700 -> 701 bullet).
    - The Owl's 3506030 (+900 on 1xx) is on boss rows only.
  - Code:
    - `data::shift_judge` (with SpCycle.judge_add and the gate check);
    - `combat::worn_sp` (residents, sp_chained, tae_prev, clash);
    - applied to NPC AttackBehavior windows in combat.rs, and to bullets in enemy_bullet.rs.
      A missing shifted row drops the window.
  - Exporter (export.rs): NPC judge sets also take the shifted judges, so rows that exist get
    exported. Re-exported c1070 / c1450 / c5100: they gained 105-197, 701 and 530, plus the
    peers' newer fields (atkBehaviorId, registIllness).
  - Test the_shura_samurais_enchanted_blade_poisons_or_burns (poison 50 / burn 125, nothing
    unenchanted, nothing with the wrong gate). Mutation-checked: without the shift, no
    build-up.
  - gaps:
    - the exe's "add 0 -> id 5" branch (only the test row 0) is not modelled;
    - the 1181xx-1185xx deathblow-type rows (conditions 3 / 4 / 5): traced 2026-10-13 (data
      only).
      - No EMEVD instruction and no TAE event applies them.
      - They are the `counterSpEffectId` targets of the NPC "designation" residents:
        - NpcParam spEffectID3: 118000 / 118060 / 118070 / 118080 -> 118100 / 160 / 170 / 180
          (blood smoke: normal, red, poison, white blood);
        - spEffectID4: 118200 / 118210 -> 118300 / 118310 (puppet);
        - spEffectID5: 118400 / 118410 / 118420 -> 118500 / 510 / 520 (granted ninja).
      - Only 118160-180 (cond 3: +60 / 70 / 80), 118310 (cond 4: +10) and 118510 / 118520
        (cond 5: +10 / 20) shift a judge.
      - By the names, they are the variant of Wolf's deathblow on that enemy. The likely reading:
        the counter row goes on the attacker (Wolf) and shifts his 3xx-5xx deathblow
        behaviours, e.g. the c1100 family's poisonous blood (118410).
      - So `shift_judge` (non-Player attackers) is right not to use them for NPCs.
      - gap: the exe's counterSpEffectId application (who gets it, and when) and Wolf's
        behaviour lookup with it are not traced. That is the blood-type look of deathblows
        (VFX / gore).
- **Aged deathblows** (11300260-281, PC -> c1300): handed to "bug fixes", who added them in
  R45.
- **Super-armour pool** (the 4.42 gap above; re-scout traced init and recovery from the exe
  bytes).
  - `actor::SuperArmor` on Actor:
    - Max is NpcParam superArmorDurability (vfunc 0x2c8, FUN_140a12de0: +0x118).
    - A draining hit re-arms the wait: 5 x (superArmorRecoverCorrection + 1) s (vfunc 0x2d8,
      FUN_140a12e90: DAT_143b01b5c = 5.0). That is 7 s at 0.4 and 25 s at 4.0.
    - When the wait runs out, the pool snaps back to full (140bd.c:8904-8923). There is no
      gradual refill.
    - A worn stateInfo-331 SpEffect skips the wait (FUN_140bfdb20).
    - Emptied, it breaks. A broken pool refills once the chr flag clears.
    - A posture break or a respawn refills it (FUN_140bdb660).
  - `combat::super_armor_hit` (FUN_140bdb470) drains, before the dmgLv pick, on sword hits
    (combat.rs Outcome::Hit) and tool hits (prosthetic.rs):
    - an NPC attacker drains AtkParam_Npc atkSuperArmor;
    - Wolf drains EquipParamWeapon 5000 saWeaponDamage (40) x AtkParam_Pc
      atkSuperArmorCorrection / 100;
    - either is multiplied by the defender's worn saReceiveDamageRate.
    - `data::super_armor_damage` reads the full param JSONs.
  - `damage_motion_level` now takes the stateInfo-120 rows (c1180's resident 5366, its TAE
    5367) while the pool holds (FUN_140bdbaf0). Before, they were always skipped.
    - So the armoured servants (240), the Shura samurai (200), c1350 (400) and the rest shrug
      off small hits (additive flinch) until the pool breaks.
  - Test the_armoured_servants_super_armour_breaks_and_refills: 8 -> 1 on the break, held
    through the reaction, a 25 s snap-back. Mutation-checked without the gate.
  - Green: armour, shuriken, damage, flinch, super, deathblow, posture.
  - gaps:
    - (done) the broken-hold flag (chr+0x50)+0x8c bit 9: set only by TAE ChrActionFlag 71
      "SetBool32_0x7C_9" (FUN_140b51b70 case 0x47, 140b5.c:7461) and read in the SA tick
      (140bd.c:8924). No Sekiro anim has flag 71, so a broken pool refills on the next tick:
      only the breaking hit staggers. It was held through the reaction before. Code: actor.rs
      regen_posture `reacting` = flag 71. The servants test now drives a real reaction, and it
      fails with the old rule;
    - (done, below) the stateInfo-155 changeSuperArmorPoint bonus;
    - (done, a no-op) the SpEffect +0x8c drain factor: FUN_140bfd040 (140bf.c:9047) multiplies
      in the defender's worn vitalSpotChangeRate (row +0x8c) values >= 0 (140bd.c:8973). Every
      SpEffectParam row has -1, so it never applies;
    - the drain / dmgLv order (taken as drain first). Supported, not proven: the drain
      (FUN_140bdbb80, called at 140b6.c:8214 in FUN_140b6f690) marks a break on the hit
      (hit+0x68, chrmod(+0x1ff8)+0x54 |= 0x10), which is read later. With the next-tick refill,
      a reaction chosen before the drain would never stagger. The dmgLv picker is not found;
    - (done) Wolf's weapon: the hit's equip-slot weapon (FUN_14084fc50 hit+0x12c,
      14084.c:8469; SA damage FUN_140848440, 14084.c:4836). It is a katana or a tool, all 40.
      Combat-art rows (0) are never used, so 5000 is right. ReinforceParamWeapon
      saWeaponAtkRate is 1 on every row. Left: the 110000 bare-hands fallback (flags & 0x60) and
      the FUN_140841bd0 rate;
    - the break knockback (superArmorBrakeKnockbackDist), left out: 0 on every row.
- **Boss transitions** (manual test #3; assigned by the overseer 2026-10-13). Every main-boss
  row was deathblown to its defeat in the sim. Each phase change was checked against its
  EMEVD, TAE and AI.
  - Fixes in tools/boss_scripts.py (FLAGS_OFF: flags `world_flags` turned on because nothing
    in the chosen events sets them):
    - Emma (74000010): her defeat event 11115900 ended at once on 9316 (Isshin Ashina's defeat).
    - The castle Genichiro and the Way of Tomoe (71001000 / 71100000): 11115800 ended on the
      ninja-invasion stage 8301.
    - Isshin Ashina (54300000): 8300 is set only by the m11_01_54_10 variant's 11115200 (and
      cleared by m11_01_50_60's), never in m11_01_00_00 or common. With it on, 11115941 played
      20200 on Wolf's Todome message 10. That cut his a000_013700 before its message 50
      (11.0 s), which his defeat event 11115920 waits on, so the fight never ended.
    - Also kept off: the Owl 50600000 {9316, 11115900}, the Ape 51000000 {8303}, the Interior
      Ministry rows {8306}, Lady Butterfly {8304}. The comment above FLAGS_OFF has the reasons.
  - `dead_at_start` (boss_scripts.py -> ScriptDef, script.rs `dead_ids`): characters outside
    the cast whose dead-wait (4[0]) comes before the boss's enable. Lady Butterfly's second row
    50900001 starts with her first part 1000800 dead.
  - sim_tests `boss_in_play`: the enabled non-Cast enemy, else the enabled one with the most
    HP (after the script swaps parts). `enemy_state` and `break_and_deathblow` use it.
  - Tests (each mutation-checked):
    - every_boss_row_reaches_its_defeat (`deathblow_to_defeat`; replaces the Emma / Genichiro
      test): every boss_scripts.json row, about 18 s. The Divine Dragon and the Sword Saint
      are left out; they have their own tests. Fails on the old json (8300 on).
    - isshins_deathblows_bring_the_spear_then_the_lightning: needs the real AI (sim enemies
      are passive by default).
      - At 2 bars or fewer with his resident 200050, Act40 draws the spear (3015). That gives
        200031 and battle goal 540010.
      - At 1 bar, 540010's Act06 / Act13 opens with 3055 / 3056.
      - Fails when the AI is given the max bar count.
    - the_guardian_ape_gets_up_headless_after_his_first_todome (m17 11705821):
      - his a000_013700 message 20 (6.1 s) + 12 s -> 20000, AI ID 51000100, 5304 / 5305 off,
        5306 on;
      - 11705820's 20200 never comes: his Todome row gives Wolf a230_511700, which has no
        TAE 936 message 10;
      - fails with Set Character AI ID a no-op.
    - lady_butterfly_changes_into_her_second_part (m10 11005812):
      - 1000800 disabled, then 1000810 enabled with forced 21000;
      - AI command 10, then 11005641;
      - fails without the event.
    - Already covered: Genichiro -> Tomoe, the Monk's phase events and phantoms, the Demon's
      last phase, the Owl's heat-up, Gyoubu's Todome, the Sword Saint's Todome message, the
      Old Dragons -> Divine Dragon.
  - Harness notes:
    - `deathblow_to_defeat` counts a defeat when BossScript.defeats > 0 or run.defeated is
      set (the flags reset with the duel).
    - With the AI on, a deathblow needs the AI held: his attacks knock Wolf out of the window.
  - gaps:
    - Isshin Ashina's 8300 fix leans on the variant files being outside the retail map's
      flow. The throw-reaction rule is now traced (c9997.hks, see "Double deathblow" below):
      ThrowDef13700 does take the 20200 request, so with 8300 on the game would cut his Todome
      as well. 8300 off is the fix.
    - The headless Ape after his rise: full HP from 401000 in his a000_020000 (changeHpRate
      -100). Deathblows are now (1, 2), as the finishers clip shows at 2:48: two dots, one used
      (the test checks both). 275100 / 3510991 / 3510110 now stay on (see "Lasting TAE
      SpEffects" below). The order of 275100's x1.1 against 401000 is not traced.
- **Genichiro's lightning, timing / AI side** (manual test #5, shared with VFX, who own the look):
  - When he casts:
    - 711000_battle.lua (the Way of Tomoe) forces Act18 (3043, weight 1e12) while Wolf wears
      109031 / 110125.
    - Those come from Wolf's knockdowns and get-ups (StandDamageLarge*, *BlowLand,
      BreakDown, ThrowDefLargeBlowLand; TAE AddSpEffect), not from his jumps.
    - His Interrupt re-plans (50 %) when 109031 comes and when it goes.
    - Timer 4 (15 s, set by the cast) holds it back, as does the resident 3711500 (row
      71101000, Genichiro in the Sword Saint fight: no lightning there).
    - At 3-7 m Act18 (30) and Act19 (3042, 20-30) are in the normal mix.
    - The castle Genichiro's 710000_battle.lua has no lightning act.
  - Bullets: 3043 fires 986 at 1.17 s and 3042 fires 988 at 0.83 s (dummy 6). Their strike and
    the reversal window are covered by the existing lightning tests
    (lightning_caught_in_the_air_..., lightning_on_the_ground_..., an_air_art_shrugs_off_...).
  - New test the_way_of_tomoe_answers_a_downed_wolf_with_lightning: Wolf held down at 6 m
    during his 3008 opener, and the next attack is 3043. It fails when 109031 is hidden from
    the AI.
  - No engine change was needed: the observed-SpEffect interrupt and the target SpEffects
    (Wolf's TAE plus chained) were already wired.
- **Paused 2026-10-13** at the overseer's request (passed on from the user), and resumed by
  the user ("start") the same day.
- **Double deathblow at the end** (manual test #6, Isshin the Sword Saint).
  - The game's ending (finishers clip 5:32-5:45): Todome stab (ThrowDef13700), his kneel
    (20200, on Wolf's message 10: m11_02 11125874), then the finisher from behind (Wolf
    a243_510900, Isshin ThrowDef12900). a100_012900's message 30 at 11.63 s -> 11125860:
    Handle Boss Defeat 5 s later. Two blows at the end is the game's own flow.
  - Bug: 11125874 restarts once Wolf's message 10 is gone. Wolf's finisher sends 10 again at
    6.63 s (and 12.47 s), so the script put Isshin back into 20200 mid-finisher. 12900 never
    reached message 30, and he knelt waiting for another finisher, again and again.
  - Fix (script.rs `takes_throw_transition`, from c9997.hks): in a throw reaction an NPC
    takes an EzState request (ExecEventTrandition, slot THROW_TRANSITION) only where its
    ThrowDef<N>_onUpdate reaches it:
    - most of them: through ThrowDefCommonFunction / ThrowDefDeathCommonFunction ->
      ThrowCommonFunction;
    - 12090 / 12190 / 13290 / 13690 / 13700 directly;
    - never: 12210 / 12230 / 13210 / 13230 / 14000-14300 / 17210 / 17230 / 18210 / 18230;
    - 12900 / 12950 only with behaviorRefId 1501000 / 1000160 / 1000161 worn.
  - Wolf still takes 710206 mid-finisher: c0000.hks fires W_Event7102xx unconditionally
    (RequestThrowAnimInterrupt).
  - Test isshins_finisher_ends_the_fight_once: Todome, kneel, finisher, then 12900 plays
    uncut to the defeat. Fails with the rule off.
  - Same anim (a100_012900, message 30) on Isshin Ashina, c5430; the rule covers him.
  - Deathblow counts in every_boss_row match each boss's red dots, plus a finisher where the
    game has one.
- **SHINOBI_AI_ATTACK** (overseer, for VFX):
  - `SHINOBI_AI_ATTACK=a000_003042` (or just `3042`) starts EnemyDebug in AiMode::Repeat with
    that attack (enemy.rs EnemyDebug::default). It overrides SHINOBI_AI_MODE.
  - The sword lightning effects 671026-671029 come only from c7110's a000_003042 (TAE
    SpawnOneShotFFX at 1.1 / 1.57 / 1.63 / 1.87 s) and a000_003043 (1.33 / 1.83 / 1.87 /
    2.1 s).
  - Test repeat_mode_plays_the_tomoe_lightning: both anims play again and again. Fails with
    the real AI.
  - Fix (VFX's real run, SHINOBI_FIGHT=c7110): debug_menu.rs reset the drill attack to the
    start kind's first attack on the frames before the encounter's kinds loaded. Now the env
    attack (enemy::env_attack) is never reset. Not sim-tested: the menu system and the env var
    are outside the harness. VFX confirmed it in a real run (3043 on repeat, 671026-29 spawn).
- **Super-armour stateInfo-155 bonus** (old gap from the SA entry):
  - FUN_140bdb690 (140bd.c:8889-8900) sums changeSuperArmorPoint over the worn stateInfo-155
    rows into SA +0x24. A rise is added to cur; the pool refills to NpcParam max + bonus.
  - Rows: 2090 +40, 3510991 +120 (the headless Ape's rise), 3540110 +200 ("revived
    Kensei", 277021 / 277023. Nothing puts 3540110 on anyone: no TAE, NpcParam, EMEVD, AI lua,
    ESD-free data or exe immediate references it, so it is likely unused. Isshin's NpcParam
    pool stays 0, and the test puts it on by hand.)
  - Code: data.rs SpCycle.sa_point; actor.rs SuperArmor.bonus / full(), tick(.., bonus),
    regen_posture sums the deduped worn rows; combat.rs super_armor_hit uses full().
  - Test a_super_armour_bonus_grows_the_pool (unit + c5400 with 3540110), mutation-checked.
- **isWaitModeDelete / what ends a -1 madness** (old gap from the whistle entry):
  - FUN_140c04090 (140c0.c:2641): when an effect's gates fail (HP window FUN_140c05fb0,
    stateInfo gate FUN_140c060d0, chr check FUN_140c06040) it waits, or with isWaitModeDelete
    (SpEffectParam +0x275 bit 5) it is deleted.
  - 230515 / 230516 have it and gate 959, the stateInfo of the beast's NpcParam resident
    230500. So a -1 madness lasts until death, and goes only if 230500 is cleared.
  - Code: data.rs SpCycle.wait_delete; actor.rs run_sp_effect_chains drops a wait-delete
    chained row whose gates aren't among the worn stateInfos (resident + chained + TAE).
    Waiting rows (no bit) are unchanged.
  - Test the_whistle_madness_goes_when_its_gate_does, mutation-checked.
  - gap: madness vfx 7900 / 7901 (VFX's): done 2026-10-13, status.rs `lasting_glow` (4.44).
- **Exporter dropped chained bullet rows** (old "Not checked: c1070 bullet 380's child 10700383"):
  - tools/sekiro-extract export.rs: the HitBulletID / intervalCreateBulletId walk ended when it
    popped a pending id it had already written, and dropped the ids still pending. c1070's 380
    (interval 383, pending) -> 381 -> 382 -> 387 -> 384 -> 384 again: the walk stopped, and
    10700383 was never written. It is the 2.6 s ground row with spEffectId0 9120 and sfx 610742.
  - Fix: skip an id that is written or missing, and pop the next. Re-exported with
    `sekiro-extract npcs extracted c1070`; only bulletRows changed (+10700383). A closure check
    over all enemies found no other gaps, and Wolf's tool bullets are whole too.
  - Test enemy_bullet_rows_hold_their_whole_chain (every enemies/*.json against Bullet.json).
    It fails on the old c1070.json.
- **Attack-occurrence SpEffects** (from the bullet-children gap):
  - Exe: FUN_140bfc170 (140bf.c:8239) walks the attacker's active SpEffects. The first one with
    stateInfo 62 / 64 / 151-153 / 205 / 355-358 (and FUN_140c06240's condition) gives its
    atkOccurrenceSpEffectId (+0x134). The damage apply FUN_140b6e6a0 (140b6.c:9012-9029) adds it
    to the defender on every processed hit, melee and bullet alike.
  - Rows: Wolf's enchantments.
    - Flame Vent 106020 / 106025 -> 106021 (endurance 0 -> replace 9100, burn build-up 31).
    - Spirit 106030 / 106035 / 3451 -> 104016 (a symbol).
    - Poison-blood 106050 -> 9002 (poison 62).
    - White-blood 106060 -> 106065 (changeHpRate -10, effectTargetAttacker: heals Wolf).
    - The Nightjars' 3145000 -> 9125. Nothing found puts 3145000 on them (no TAE, bullet,
      param or EMEVD; their AI reads it), so it is not wired.
    - 2240 (stateInfo 199) never qualifies.
  - Code: data::atk_occurrence (from the full SpEffectParam). combat.rs resolve takes Wolf's
    `timed` rows; occurrence_hit adds the landing ids to both Wolf -> enemy status.hit sites, or
    heals Wolf for an attacker-aimed row.
  - Test an_enchanted_sword_builds_burn (c1020: burn after a slash with 106020, none without).
    It fails with the lookup off. Filters flame / enchant / burn / poison / status / tool /
    prosthetic / deflect / guard: 89 passed. The one failure, guard_style_presses_continue_the_guard,
    was bug fixes' mutation check swapping player.rs mid-run; it is green again.
  - gaps:
    - (done 2026-10-13, see "Hit-category gate" at the end of 4.42) FUN_140c06240's condition;
      guarded hits still count (the block reads no guard state);
    - Wolf's tool bullets: category 2, so none of his enchantments (all wepParamChange 1) lands
      with them (2026-10-13);
    - (done) a bullet hit's slots 0-4 come from the Bullet row only. FUN_1410b6880's param_3 is
      BulletParam +0x6c..0x7c (FUN_1410b7910, 1410b.c:5026) on the bullet path and AtkParam
      +0x20..0x30 on melee (14099.c:9681). enemy_bullet.rs already did this (R36 C).
      prosthetic.rs fly_bullets now does too: Wolf's 500054 / 500512 / 500542 lose the 9002 that
      only their AtkParam had. Tool / burn / poison filters: 50 passed;
    - (no-op) slots 5-7 come from the weapon's spEffectBehaviorId0-2 plus a reinforce byte.
      Every EquipParamWeapon row has -1 there;
    - enemies' occurrence (no worn source found).
- **Predictive shots** (BulletParam predictionShootObserveTime +0xB8, 64 rows; old "musket ... not
  traced"):
  - Exe: FUN_140b0acf0 (140b0.c:8956, test at 9107) reads the field.
  - The target's average velocity comes from a 240-entry {delta, dt} ring on the character:
    FUN_1405ac4e0 writes it, and FUN_1405ac1b0 (1405a.c:9189) returns sum(delta) / sum(dt) over
    the last N s.
  - FUN_140b05b80 (140b0.c:3144) leads the aim. In 1/30 s steps the target moves on at that
    velocity while a bullet flies initVellocity / accelIn / OutRange past dist / min / max, until
    the bullet's distance reaches the target's. No gravity. With about zero speed or velocity,
    it aims at the plain target.
  - The led aim must lie within lockShootLimitAng, and launch mode 4 skips the lead.
  - Code (enemy_bullet.rs): TargetTrack (Wolf's ring, system `track` before `fire`), predict(),
    and fire leads when data::prediction_observe_time(bullet_id) > 0. The field is read from
    the full Bullet table, since the export leaves it out.
  - Users: the c1190 / c1191 archers (0.2 s), c7100 / c7110 540, and the rest of the 64 rows.
  - Test a_predictive_arrow_leads_a_running_wolf (c1190 11900100: ring average, no lead when
    still, lead = run x flight). It fails with predict() a no-op. Filters bullet / arrow /
    archer / musket / predictive / lightning: 24 passed.
  - gaps: the per-frame ring push caller (FUN_1405ac6f0) is not found, so the ring is taken as
    Wolf's root per fixed tick. Why 28 rows have lockShootLimitAng 0 (we keep the 1 deg floor)
    is unknown, and so is the 140b0.c:11071 use.
- **Lasting TAE SpEffects on enemies** (from the Ape's rise gap):
  - TAE 66 and 67 share FUN_140b56500 (140b5.c:3250; the flag is only the net-play gate). The
    effect is added if absent and refreshed (+0x26) if present. Nothing removes it at the
    window's end, and its lifetime is its own effectEndurance: -1 forever (FUN_140c03af0 /
    FUN_140c06b70), >0 seconds, 0 gone a tick later.
  - Add rule (FUN_140c02c50; match FUN_140c02000 / FUN_140c05a80; accept FUN_140c064c0, 140c0.c:5724).
    Lower categoryPriority wins.
    - spCategory 100 overwrites the same category and priority: the AI states 200000 / 200001
      (priority 1) and the anim sets 200030-34 (priority 3).
    - 110, 130-133, 140, 150-174 and 190-204 overwrite the same category.
    - 175-189: a not-worse newcomer overwrites, a worse one is refused.
    - 141: worse is refused, equal coexists, better deletes the old one.
    - Other categories: one per id; 10 stacks.
    - stateInfo deletes (FUN_140c05c10): 0x2c over 0x2b and the like. Not ported: no
      enemy TAE -1 row has stateInfo 43-45 / 111-114 / 192.
  - Code: enemy.rs wear_for_good. It replaces the anim-set-only block and runs for every TAE row
    with endurance -1 into `resident`. maxHpRate / maxStaminaRate scale hp_max / posture_max
    while worn.
  - Tests:
    - the_guardian_ape_gets_up_headless_after_his_first_todome now also checks 275100 / 3510991
      / 3510110 worn, hp_max x1.1 and SA full +120. It fails with the rows window-only.
    - a_lasting_spfx_overwrites_its_category (c1010 rows) fails when category 100 stacks.
    - Both mutation-checked. Filters ape / isshin / okami / boss / tomoe / super / deathblow /
      phase / warrior / lasting / madness / whistle / repeat / throw: 72 passed. Filters enemy /
      ai_ / sweep / grab / npc / monkey / kind / spear / anim: 82 passed.
  - gaps:
    - Wolf's TAE -1 rows: all six have a category, so player.rs `held` already keeps them
      (105051, 1503x2, and 110091 the heartbeat parent, cut by 110093). `held` is not in
      worn_sp; VFX play 110091's 2.5 s cycle 110092 (vfx 5010) from `held` (status.rs
      `heartbeat`, 4.44);
    - the 3-bit stateInfo stack counter;
    - the FUN_140c02140 gate;
    - the 10002 / category-1 suppression.
- **Big enemies reach drill range** (VFX report: Demon of Hatred c7020 never drilled).
  - The drill distance was measured from the body centre, so a hitRadius 2.8 body never got
    close enough. enemy.rs now passes `dist - (hitRadius - 0.5)` to `drill`.
  - Test: a_big_enemy_reaches_drill_range (c7020 row 70200000, Repeat 3043; Wolf at 12 m). It
    fails without the offset; mutation-checked. Filters drill / repeat / debug_menu / big_enemy: 7 passed.
- **Clash timers on the actor clock** (bug fixes R64 suggestion). parry_interrupt's `clash`
  countdown (Enemy::add_timed rows) now uses `a.clock(dt)` (chr+0x108, hit-stop scaled).
  parry_timer and guard_count_timer stay on real dt: they are AI timers, not SpEffects.
  Filters clash / hit_stop / guard / parry / firecracker: 57 passed.
- **MSB region checks: IsInsideTargetRegion / SetEventMoveTarget** (overseer queue 1, Folding
  Screen Monkeys).
  - Exe: IsInsideTargetRegion (registered 14008.c:2536) -> FUN_1405e5c90 -> FUN_1406068f0 (the
    target's position) -> FUN_140678380 -> FUN_1406d35d0 (1406d.c:2135), a tree keyed by the
    region's entity id, then the region's shape test. No offset and no table: an id no MSB has
    is never inside.
  - 126000_battle.lua Act17's 99028xx ids (and 114000's 99020xx) are in no MSB, EMEVD, ESD or
    param, so those branches are dead in the game too. Act17 falls through to the 20028xx twins
    (m20_00_00_00.msb, names 稚児猿_H11 / Y11 / C11_12 / P11 / PY11 ...), else 2002898 (WP1).
  - tools/regions.py writes extracted/enemies/regions.json: every region with an entity id, per
    MSB, in map space.
  - enemy.rs map_regions_on turns them into the arena like msb_patrol_on, cached per map.
    Snapshot.regions carries them.
  - ai.rs: IsInsideTargetRegion tests the target's feet (WakeRegion::contains). SetEventMoveTarget
    sets POINT_EVENT to the region's spot (`event_target`; home until set).
  - Test: monkeys_read_the_senpou_regions (ai.rs; 2002821 / 2002825 / 9902820 and GetDist
    to POINT_EVENT after SetEventMoveTarget(2002859)). Mutation-checked both halves. Filters
    monkey / screen / patrol / ai:: / map_ / c1260: 27 passed.
  - gaps: the decompiled Act17 has two leftovers (`R_23`, `c11_22`: nil, so false). Not
    checked live. RegisterTriggerRegion is still a no-op (AddObserveRegion is done, see the
    observe-area entry below).
- **Old Dragons of the Tree spawn** (overseer queue 2).
  - The 15 dragons are m25 2500811-825 (c5300, rows 53000010 / 11 / 12). Each has a 12505803
    instance: when the dragon is at HP <= 1 (4[14]), it puts SpEffect 3530020 on the shared bar
    2500810 (changeHpRate 4, effectEndurance 0: 4 % of the bar).
  - No event enables them, since they stand from the start, so boss_scripts.py left them in
    `places`, not `chars`.
  - Fix: boss_scripts.py `linked`. A part is cast when an event puts a SpEffect on the boss
    (2004[08]) while it watches that part (4[*]), and the part has the boss's chr.
    - Wider rules pulled in other fights' bystanders: the Owl (Father)'s 1000800, 52000000's
      dragons, 54000000 / 71100000 / 74000010's extras.
    - Regenerating changed only 53000000: 15 chars, 63 -> 79 events.
  - script.rs Act::SetSp: an instant row (effectEndurance 0) changes HP as it lands
    (changeHpRate % of max + changeHpPoint), each time it is set. Before, it only became resident.
    data.rs SpCycle has hp_rate / hp_point.
  - Test the_old_dragons_stand_in_their_fight: 15 dragons up, and one at 1 HP costs the bar.
    Mutation-checked both halves (the old export -> [], no HP change -> bar 1000 -> 1000).
    Filters dragon / script / boss / ape / isshin / owl / monk / butterfly / genichiro / tomoe:
    35 passed.
  - gaps:
    - 12505801 still blocks at its action button 3[24] (9821 on 2500806), so ins 39 never turns
      the bar body 2500810's AI off, and it fights too.
    - The dragons' 1 HP floor (they should not die) is not checked.
    - 12505828's 3[12] count reset is not checked.
- **Isshin Ashina's gun volley** (overseer queue 5).
  - m11_01 11115944: on SpEffect 3540510 (a000_003032 f90, TAE 66), it fires 254000382 (Bullet
    54000382: a still 1.5 m fire spot, life 1.5 s, child 54000383) at 35 sources 1112960-994.
    These are "炎弾丸射出点" spheres; 960 / 961 are in no MSB.
  - The owner 1110921 is a c1000 dummy (NpcParam 10006000) that no event enables, so every shot
    was dropped.
  - Fix: script.rs Act::Shoot fires from a region source's spot (ScriptShot.at). The boss
    stands in for an owner that was not spawned.
  - Test isshin_ashinas_volley_fires_from_its_regions (>= 33 bullets). Mutation-checked: 0
    without region sources. Filters isshin / bullet / shot / script / boss: 29 passed.
  - gap: the dummy owner's own NpcParam scaling.
- **LeaveTarget trace** (Gyoubu's horse gap; no code change).
  - Activate FUN_1405c7e80 (1405c.c:4800, inferred from the ctor FUN_1405c7d60's 7 slots and the
    +0xb6f1 flag that Terminate FUN_1405c8050 clears). If GetDist(p0) < p1:
    - the turn target becomes p2 (ai +0x9050, FUN_1405e89f0);
    - one move-to-point request at AI_DIR_TYPE_ToB (6), (p1 + radius) x 1.5 away
      (FUN_1405e6640 -> FUN_1405e66b0);
    - the move mode chr+0x7410 = 2 - walk: 1 walk, 2 run;
    - guardActId > 0 queues 0x835.
  - Update FUN_1405c80d0 (1405c.c:4738) succeeds at GetDist >= p1. Two exits are untraced:
    FUN_140601d40 / FUN_140607040.
  - With turnTarget TARGET_SELF there is nothing to face, so the chr faces its way and runs off.
    That is what ai.rs does now. There is no explicit self branch in the exe, so this stays
    inferred.
- **Immortality vs the last deathblow** (Gyoubu / Ape queue items; trace, no code change).
  - 2004[12] Set Character Immortality. The NoDead check is FUN_140bd5df0 (140bd.c:3381):
    bit0 of hpmod+0x228 (inferred to be the EMEVD bit), SpEffect type 0x8f, and others.
  - SetHp FUN_140bd64e0 (140bd.c:4165) clamps to 1. No throw path bypasses it.
  - The death comes from the anim. HKS IsDead (c9997.lua:966) takes hp <= 1 + NO_DEAD as dead.
    ThrowDef12090_onUpdate goes to DeathIdle at the anim's end. a000_012090 has ChrActionFlag
    12 (Death) at f241 and 201000 at f0.
  - Gyoubu's messages come from TAE SetEzStateRequestID: 10 at f0 / 50 at f155 on the non-final
    reactions (012000 / 012110 / 013200 / 013610). The final ones send neither, so he stays
    immortal and dies through the anim. 11105800 waits on 201000 (4[5]), not on HP.
  - script.rs's 201000 exception gives the same result. 2026-10-14: the map script's "dead"
    now waits for the reaction's ChrActionFlag 12 (4.63 item 9); the flag's consumer is still
    untraced (the kill routine is FUN_1409fa0c0).
- **Lady Butterfly's second body alone**: dead_at_start [1000800] already lets 11005800
  finish. Test butterflys_second_body_alone_reaches_her_defeat passes. Not mutation-checked:
  the guard is the exporter's dead_at_start, which is covered by every_boss_row_reaches_its_defeat.
- **Observe areas** (shared AI): AddObserveArea(slot, observer, target, dir, angle, dist) keeps
  a cone per slot. IsInsideObserve(slot) answers it like IsInsideTargetEx, and DeleteObserve
  drops it. Before, IsInsideObserve was always false: 102000 / 136000 and about 30 regular
  enemies' scripts pick acts by it.
  - Test observe_areas_see_wolf, mutation-checked. Filters ai:: / enemy / sweep / npc / kind:
    84 passed.
  - gap: the exe's slot bookkeeping and its INTERUPT_Inside/Outside_ObserveArea interrupts
    (GetAreaObserveSlot stays false).
  - AddObserveRegion(slot, target, region) is in too: the target inside the MSB region
    (Snapshot.regions). Slot 30 holds the castle's force-walk regions; 106000 / 121010 /
    145000 use others. Covered by the same test, mutation-checked. Filters: 86 passed.
- **Reservoir Tomoe -> Isshin** (overseer queue 7, row 71101000).
  - 11125830 enables Isshin 1120860 and sets Tomoe's 9311 in the same event. script.rs's
    "93xx with the boss at 0 bars = defeat" rule ended the fight there.
  - Fix: the rule skips an event that also enables another of the cast (2004[05] on): a
    hand-over.
  - Test reservoir_tomoe_hands_over_to_isshin: Isshin comes in, and there is no defeat 6 s
    later. Mutation-checked.
  - every_boss_row_reaches_its_defeat leaves 71101000 out now, like 54000000: it goes on into
    the Sword Saint, whose phases take longer than its 5 rounds.
  - Checked (2026-10-13): Isshin's 11125860 (message 30 -> 9312) ends this row. Test
    the_reservoir_run_ends_with_isshins_finisher. isshins_finisher_ends_the_fight_once's body
    is now isshins_finisher_to_defeat(chr, row), which beats Tomoe first and allows 60 s,
    since the reservoir kneel comes later. The defeat deferral `next_boss` is mutation-checked.
    Filters isshin / reservoir / tomoe / every_boss_row: 14 passed.
- **Observe enter / leave interrupts.** Each tick, ai.rs `observe_edges` compares every slot's
  inside state with last frame's. A slot that went in raises INTERUPT_Inside_ObserveArea (16);
  one that went out raises Outside (40), both through the root goal.
  - GetAreaObserveSlotNum(type) / GetAreaObserveSlot(type, index) give the changed slots (type
    1 in, 2 out; false past the end). common_table_ai_common.lua:940-960 loops over them.
  - A fresh slot starts outside.
  - Test observe_areas_report_enter_and_leave, mutation-checked. Filters ai:: / enemy / sweep /
    npc / kind / patrol / stealth / sound: 106 passed.
  - gaps: not traced in the exe (the slots' bookkeeping, whether a slot registered with the
    target inside fires). Only the root goal's Interrupt hears it, not the table-logic chain
    (interrupt_logic sends 46 only).
- **Prologue Genichiro's scripted loss** (71000000; overseer item 3).
  - 11125812: with Wolf in 1122802, 2004[12] makes Wolf (10000) and Genichiro immortal.
    11125800: Wolf at 1 HP there sets 9300 with Genichiro still up (the arm-loss cutscene).
  - script.rs:
    - Act::Immortal(PLAYER) sets Run.wolf_immortal, mirrored into the WolfImmortal resource.
    - A 93xx flag with the boss up, Wolf at <= 1 HP and immortal is Act::ScriptedLoss: the duel
      starts over.
  - Test the_prologue_genichiro_is_a_scripted_loss (Wolf parked in 1122802, HP set to 1).
    Mutation-checked. Filters genichiro / tomoe / every_boss_row / script / boss: 11 passed.
  - Player side: done by "bug fixes" in round 73 (player.rs wolf_no_dead, the SetHp NoDead clamp
    FUN_140bd64e0 from WolfImmortal; was OPEN here).
    the_prologue_genichiro_is_a_scripted_loss now drives it: an overkill (-10000 HP) on the
    immortal Wolf stops at 1 HP and the loss fires (low HP >= 1). Mutation-checked: with the
    clamp off, no 9300. Filters prologue / genichiro / immortal: 3 passed.
- **Doubled event instances** (Owl (Father) 50601010, Butterfly 50900000 / 50900001).
  - m10 initialises 11005800 / 810 / 900 / 910 twice with the same slot and args.
    boss_scripts.py `instances` now keeps one per (slot, args), on the reading that running
    events are keyed by id and slot (not traced).
  - Only those three rows changed (14 -> 12, 39 -> 37, 35 -> 34 events). Filters butterfly /
    owl / every_boss_row: 5 passed.
  - The Owl (Father)'s skipped warps are the stranger-arena rule (the user's arena choice), so
    they were left alone.
- **Isshin Ashina follows Emma** (74000010; overseer item 3).
  - m11_01 11115930 enables Isshin Ashina 1110920 once her defeat event 11115900's flag is on
    (3[00]), with Wolf in 1112811 standing on 1114801-804 (11[02] IF Player Standing On Hit).
  - Exporter: brought_in is also armed by a 3[00] on the flag of an event that hands out a cast
    member's defeat. 74000010 gains 1110920 and his events. FLAGS_OFF 8300 is kept off for it,
    as for 54300000.
  - Exporter: `other_intro` marks done an event that enables a boss part outside the row's cast:
    another fight's opening.
    - 71100000 alone: 11115810 is Genichiro's intro. Run from 0 it set up his fight once 11[02]
      was answered, and that broke the lightning tests (Wolf took 4700-4702).
    - 50600000: Isshin Ashina's 11115930.
  - script.rs:
    - 11[02] is answered as "Wolf grounded". The arena keeps no collision ids (gap).
    - A handled defeat waits while a boss of its own row in the cast is still to come.
    - Enabling such a boss clears a defeat that was already handled.
    - The 15 s dead rule waits while he fights. The Demon's c1001 dummy (no boss row) must
      not undo his defeat.
  - Test isshin_ashina_follows_emma (deathblows until Isshin is in; still there 6 s later).
    Mutation-checked: no Isshin without the deferral.
  - every_boss_row_reaches_its_defeat leaves 74000010 out, like 71101000.
  - Filters lightning / every_boss_row / script / boss / genichiro / tomoe / isshin / ape / owl
    / butterfly / monk / dragon / prologue / demon: 49 passed.
- **Old Dragons' fight start** (53000000; overseer item 3, Divine Dragon intro).
  - m25 12505801 waits on an Action Button (3[24], 9821 on 2500806), then plays cutscene
    25000000, shows bar 905300 on 2500810 and turns that bar body's AI off (2004[01]).
  - Exporter entry(): with no boss enable in the event, an event that shows the boss's bar
    (2003[11]) starts right after its first 3[24]; the duel is past the button. Only 53000000
    changed (12505801 start 0 -> 3).
  - the_old_dragons_stand_in_their_fight also asserts the bar body's AI is off.
    Mutation-checked: fails with entry() returning 0.
  - the_old_dragons_fight_before_the_divine_dragon no longer asserts the body's own attacks
    3002 / 3003 / 3005 (its AI is off, as in the game); the 12505901 / 9318 assert stays.
  - Not checked: the dragons' 1 HP floor and the 12505828 reset.
    - 12505803 (per dragon) waits on 4[14] HP <= 1, then hurts the bar 4 % (3530020) and
      counts 12505828 up (2003[31]).
    - The 1 HP floor is already ours (re-scout, 2026-10-13). NpcParam 53000000 / 010-012 carry
      5830 "Immortality" (stateInfo 143) in spEffectID20, which the resident scan (slots 0-31)
      picks up. So dies_at_zero_hp is false, and at 0 HP they open the deathblow, not die.
      The 12505828 count is still not checked.
  - Filters old_dragons / dragon / every_boss_row: 8 passed.
- **R35 guard-skip test** (4.42 gap closed, see R35 above). Test
  a_blocked_hit_that_ignites_plays_the_fire_reaction, mutation-checked. Filters flame / burn /
  guard / ignite / poison: 66 passed. Test only; combat.rs unchanged.
- **R36 B resolve wiring test** (gap closed, see R36 B above). Test
  blue_lightning_leaves_an_air_art_going: the General's attacks made blue (spAttribute 10,
  charge 9490), Wolf held mid-100235 in an a110_3162xx / 3167xx art (keys sorted: a random
  HashMap pick made one "bug fixes" full run fail), on the ground and 0.5 m up (held airborne
  at ground height he lands at once). He takes the hit with no shock, no charge and no
  reaction. Each gate is mutation-checked: the air catch (LIGHTNING CAUGHT), the ground shock
  (SHOCKED) and the reaction (Wolf played one). 6 / 6 runs green.
  - Filters lightning / air_art / electro / shock: 12 passed. Test only; combat.rs unchanged.
- **Clearing a SpEffect gives the anim group back** ("bug fixes" R78 lead). script.rs
  Act::ClearSp and enemy.rs's phase "clear" removed the SpEffect but kept a.anim_group. Both now
  re-derive it from the residents with anim_group_of, as spawn / respawn / SetSp do.
  - Defensive: no shipped script or phase clears an enemy's stateInfo 270-27x SpEffect. The only
    such clears are 4700 (stateInfo 278) on Wolf (10000), in the Owl / Genichiro / Emma /
    Butterfly / Monkeys events, and Wolf's SetSp / ClearSp aren't routed.
  - Closed (round 80): SpEffect 4700 is "Warp prohibited" (stateInfo 278, spCategory 0), not an anim offset (those are 200030-200034, stateInfo 270-274). No warping in Sv1, so not applying it is a no-op. 4701 (321, no item use) and 4702 (322, all operations prohibited) are not routed to Wolf either (PLAYER is never in run.ents, script.rs ent_of).
  - No new test (it would need a hand-built Combat). Filters anim_set / second_set / script /
    phase / every_boss_row: 10 passed.
- **Trigger regions** (shared AI; the 4.42 "RegisterTriggerRegion is still a no-op" gap; re-scout
  traced it statically).
  - Exe:
    - RegisterTriggerRegion(cat, unit, a, b, target, dir, extra) (FUN_1405e7280 -> FUN_140642110,
      14064.c:648) puts a sphere into one global store (DAT_143d55070). The centre is the
      target's position + its dir vector x |hit radius + extra| (FUN_14060c250), radius b, life
      a seconds (counted down each frame, FUN_140643250). The key is (registrant, cat, unit),
      and a repeat key replaces the region. The sphere is static.
    - unit is only a tag. The caution logic passes its sound's serial
      (GetLatestSoundTargetInstanceID: the sound record's global counter, FUN_14060f840 /
      FUN_1405e3680).
    - An AI with RegisterTriggerRegionObserver(cat) (FUN_140642ab0) is tested each frame
      (FUN_140642de0) against every live region of that category, but not its own
      (FUN_1406074a0). An entry raises interrupt 48 on that frame only, an exit or expiry 49.
      The Invade / Leave lists (cat, unit) are cleared every frame (AI +0x7460).
    - RemoveTriggerRegionObserver passes a literal 1 (FUN_1405e75f0), so observer 1000 stays.
  - Use (common_common_logic_func.lua:290-348, common_table_ai_common.lua:286):
    - Every cautious enemy observes 1000.
    - One that reaches its sound (within 3 m; ToCaution actions 2 / 3, 505 think rows) leaves
      a 5 m, 5 s region tagged with the sound.
    - An ally that heard the same sound and walks into it stops (ConfirmCautionTarget). So a
      group does not pile onto one noise.
  - Code:
    - ai.rs: TriggerRegion; Snapshot me_id / sound_serial / triggers; Command.triggers.
    - AiState: the observers, last frame's regions, the edges (`trigger_edges`).
    - The env calls above.
    - In tick: 48 goes through interrupt_logic (then the goals), 49 to the goals.
    - enemy.rs TriggerRegions (resource): think ages it, hands each AI a copy, and stores each
      AI's new regions right away (AIs later that frame see them).
    - Sound serials: AiSound.serial from a global counter (kept when an owner refreshes its
      sound); stealth.rs Sound.serial (hear_record). Wolf's TAE sounds use their AiSoundParam
      id (one record per id, as for sound_new).
  - Tests (mutation-checked):
    - ai::tests::trigger_regions_report_invade_and_leave: register / observe / edges /
      getters; the own-region and category filters.
    - a_cautious_enemy_stops_in_an_allys_trigger_region: c1020 walking 7 m to Wolf's AiSound 4
      stops more than 4 m out in a region tagged with that sound's serial, but not in one with
      another serial. It fails without the 48 dispatch.
  - Filters ai:: / stealth / sound / caution / patrol / group / unaware / trigger: 53 passed.
  - gaps:
    - the Remove literal and the dir table (142969948) are read from the decompile only;
    - Wolf's TAE sounds share one serial per id;
    - the region's Y raise for some chr types is left out.
- **IsNoDead inputs** (re-scout of FUN_140bd5df0, closes the "unidentified bit" gap in the
  0-HP rule above).
  - Bit chr+0x50 -> +0x50 -> +0x90 bit 1 is TAE ChrActionFlag 96 "Set Immortality"
    (FUN_140b51b70 case 0x60). It is cleared each frame, so it holds while the event is on.
    Only c1500 a000_020000-020002 and c1360 a000_020000 / 021000 have it, from frame 0.
  - combat.rs break loop: a mortal enemy emptied under it is held at 1 HP, with no death and
    no deathblow (Enemy::tae_no_dead).
  - The other inputs are:
    - debug bits;
    - hpmod +0x228 bit 0 (EMEVD 2004[12] on an NPC, inferred: no writer in the decomp);
    - a stateInfo-143 SpEffect (51 NpcParam rows, some in slots past 7; our resident scan
      covers 0-31);
    - a dead player-only stateInfo 235;
    - ninsatuNum.
  - Test a_zombie_rising_under_flag_96_keeps_1_hp (c1500 15000000; idle control dies).
    Mutation-checked. Filters death / kill / zombie / deathblow / dies / c1500 / burn / poison /
    old_dragons: 75 passed.
- **bCallFriend / CallHelp traced** (re-scout; the R36 and team-order gaps).
  - bCallFriend is a gate on the owner, not a target hand-over. The listener FUN_140610470
    (14061.c:1201-1223) hears a bCallFriend sound only while the owner's AI is in Battle (3) or
    Find (2), or when the owner has no AI. An owner in Idle or Caution calls no one.
  - bRestrictSamePlacementGroup (FUN_140610810) passes when the owner has no group list
    (Wolf), or when the two chrs share any of their up to 8 MSB group bytes.
  - Done: enemy.rs AiSound.owner_alert, refreshed by think from the owner's targeting state;
    ai_sound_heard drops a bCallFriend row (11 rows: 5300, 2000000-2000100, 2103000-2155000)
    while it is false. attacks_make_ai_sounds_that_call_allies asserts it (mutation-checked).
    Filters ai:: / stealth / sound / caution / patrol / group / unaware / trigger / ally / call:
    53 passed. bRestrictSamePlacementGroup stays a gap (one group per fight).
- **Hit-category gate for attack-occurrence SpEffects** (re-scout: FUN_140c06240, 140c0.c:5288).
  - A worn row counts only for the hit's BehaviorParam category (hit +0xda, 14084.c:8460):
    - wepParamChange 0 passes every category;
    - 1 passes the sword / combat arts (1);
    - 2 passes the tools (2);
    - 4 passes a combat art's bullets (9);
    - any other category passes only 0.
  - throwAttackParamChange asks for a throw hit, chargeAttackParamChange for a charge attack.
  - FUN_140bfc170 takes the first worn row that passes.
  - Every enchantment of Wolf's (106020 / 025 / 030 / 035 / 050 / 060, 3451) is
    wepParamChange 1. So his tool bullets no longer carry the Flame Vent burn and the rest.
    The Nightjars' 3145000 (0) passes everywhere.
  - Code: data::atk_occurrence(worn, cat, throw, charge), HIT_CAT_SWORD / HIT_CAT_TOOL.
    combat.rs passes 1 and prosthetic.rs passes 2.
  - Test attack_occurrence_follows_the_hit_category (mutation-checked);
    an_enchanted_sword_builds_burn stays green. Filters attack_occurrence / enchant / burn /
    flame / shuriken / bullet: 32 passed.
  - gaps:
    - the throw / charge flags are passed as false (no enchantment row sets them);
    - categories 3 / 4 / 10-12 and the entry skip mask are not modelled;
    - FUN_1409f8320's use of the guarded flag is not decoded.
- **A bullet's Appear AI sound** (re-scout: FUN_140b0fc60 -> FUN_1409a1290's bullet branch,
  1409a.c:2243-2276).
  - A bullet issues its own attack as it starts. Its AtkParam's AppearAiSoundId goes out at the
    bullet, owned by the shooter.
  - enemy_bullet.rs fly and prosthetic.rs fly_bullets emit it on a bullet's first tick
    (children included). 372 of the 900 enemy bullet rows carry 2000025 (10 m, bCallFriend), so
    an arrow or shot now calls the shooter's allies near where it starts.
  - Test a_bullet_makes_its_appear_sound_where_it_starts (c1190 11900100 6 m off; dummy row
    10001600 none). Mutation-checked. Filters bullet / arrow / archer / musket / sound / ally /
    call / stealth / group / shuriken / prosthetic: 51 passed.
  - gap: a periodic hit's re-issues (140b0.c:8408 / 8627) emit nothing.
  - Flake (bug fixes R87 full run, 1 of 533): the archer's own shot could replace the record
    (same owner and id). The test now sets ai_off and despawns earlier bullets per row; 15/15.
- **ThrowParam forDef picks the row** (bug fixes R89 finding; checked in ThrowParam.json).
  - additionalConditionSpecialEffect_forDef is the SpEffect the defender must wear. 200030 /
    200031 are "Anime ID offset [0] / [1]" (a000 / a100): every "（HU）" row (+500) and c1021 /
    c7410's only mikiri 11021690 / 17410690 need 200031; the plain 0190 / 0120 / 0010 of c1020
    and the bosses 200030. Others: 200080 "body fluid normal" (96 dog / swordsman ninjutsu rows),
    3140300 / 3140301 (swordsman near / far break), 3118160, 3108010, 3501022.
  - combat.rs `throw_row_for(combat, a, sfx)`: the first of sfx / sfx+500 that exists and whose
    forDef is -1, the anim set (200030 + anim_group) or worn (worn_sp). Used by dodged's
    mikiri_row, the resolve mikiri gate, the 0120 / 0190 picks and the deflect-break 0010.
    So c1021 / c7410 can be Mikiri'd (they never were), Isshin's spear phase takes 15400690 /
    0620 / 0510 and Butterfly's second form 15090510.
  - Test a_second_form_takes_its_hu_throw_rows (row picks for c1021 / c1020 / c5400 / c5090; the
    c1021 a100_003015 thrust Mikiri'd). Mutation-checked (no +500; anim set ignored). Filters
    mikiri / deflect_break / a_second_form: 6 passed.
  - Player side (bug fixes R89): deathblow_check's `hu` and the 11 / 12, 121 / 122 follow-ups
    now take throw_row_for; 0005 / 170 / 171 / 179 / 200 / plunge are left as they were.
  - Traced 2026-10-14 (re-scout): the exe never builds an id. FUN_140be2a30 (140be.c:2151)
    walks the ThrowKindParam kinds of the check situation (bucketed by throwCheckSituation and
    heap-ordered by checkPriority, throwChangeId, id: FUN_140a10eb0 / FUN_140a10ce0) and, per
    kind, the attacker's / defender's ThrowParam rows (FUN_140a0ed50 lists, throwKind
    100000000 skipped) with AtkChrId / DefChrId matching; FUN_140be3a90 (140be.c:1908) checks
    additionalConditionSpecialEffect_forAtk / _forDef (+0x6c / +0x70) against the worn SpEffect
    list (FUN_1409f1c80, chr+0x11d0; -1 none), distance / angle / height (FUN_140be0270), the
    fall orbit and the sphere cast. FUN_140be2d60 takes the first match. So the suffix (0190,
    +500) means nothing: rows are told apart by throwKind and forDef (200030 / 200031 are worn
    SpEffects, the anim-set ones). No ninsatsu check in that path. Ours (throw_row_for) is the
    by-suffix approximation; the faithful pick is by (AtkChrId, DefChrId, throwKind) in id
    order with forAtk / forDef worn. gap: within-kind order taken as id-ascending (inferred).
  - CallHelp (team order [1], chr+0x9140+0x14) is written only by the Lua env
    ReqTeam_Support / ClearTeam_Support. No shipped script calls them or reads
    ORDER_TYPE_CallHelp. Ours stays 0 (the exe's idle is -1.0, unread). Closed.
- **The Divine Dragon's trees: on its dummies, and grapple-able when they bloom** (overseer
  priority: without them Wolf cannot reach the Dragon).
  - Place: m25 12505901 warps each tree c5310 (2500880-884) with 2004[42] "Warp Character and
    Copy Floor" (entity, type 2, dest 2500800, dummy poly) onto the dragon's dummies 50 / 53 /
    54 / 55 / 56, gravity off (2004[10]). script.rs `Act::Warp` gained the dummy poly (-1 =
    root); a type-2 warp with one stands on that dummy marker's GlobalTransform, facing its +Z.
    boss_scripts.py brought_in also casts characters a 2004[41 / 42] warps onto a cast member
    (883 / 884 were missing). Test the_divine_dragons_trees_stand_on_its_dummies.
  - Grapple: 12505901 attaches o259920 / 921 / 922 (2501910-924) and o259923-925 (2501830-844)
    to each tree's dummies 600 / 601 / 602 (2003[58] chr, dmy, obj) and switches them off
    (2005[3]). 12505911 sends AI command 10; the tree's 531001_logic.lua answers
    INTERUPT_EventRequest with AttackImmediateAction 20000 / 20002 / 20004 (random); the c5310
    HKS ends those into the loops Event21000 / 21001 / 21002 (TAE 66 5025 / 5026 / 5027 over
    the clip); 12505906 switches on that point's objects while the tree wears it. The grapple
    points are the objects' e<id>.edge records (type 1: hook = V1-V2 midpoint, landing V3,
    variation): four each on o259920 / 921 (variation 45 -> WireVariationParam
    chrPhysicsHomingIdOffset 53105 -> ChrPhysicsHoming 300053105, same values as row 0) and
    four on o259922 (0); the others have none. The tentacle's own wire SpEffect 3531500 has no
    setter (dead data).
    - boss_scripts.py `msb_objects` / `edge_grapples` / `attached_objects` -> ScriptDef.objects
      (52000000 / 53000000 rows regenerated; others unchanged).
    - script.rs Run.attached: (2003, 58) attaches (keeping its active state), (2005, 3)
      switches; `Run::grapple_objects()` lists the active ones with edges.
    - enemy/grapple.rs pushes their edges as EnemyGrapple { object: true } in the dummy
      marker's frame (X mirrored); wire.rs aim_wire takes those as ridden map points (no
      WireHit on the tree); homing_offset gained 45.
    - enemy.rs: c5310's Ez20000 / 20002 / 20004 end into Event21000 / 21001 / 21002, which
      loop and stay interruptible (`free`) for the next request (20 -> 20001 / 3 / 5 closes).
    - gap: 2005[19] Set Object Interaction is not modelled; whether the exe moves an object's
      registered edges with the tree (ActionEdgeMng FUN_140ef4470 from the ObjIns update
      FUN_14087b440) is not traced; ours follow the dummy each frame.
  - **INTERUPT_EventRequest (39) was never raised**: the AI command only set the slot
    (GetEventRequest). Now raised for one update per command (enemy.rs / script.rs
    `event_req_new` -> Snapshot), a repeat of the same value too: bit 39 of the update's
    interrupt mask, FUN_1405b49e0 from the received bit FUN_1405b4730 sets (14060.c:6579,
    1405b.c:3349 / 3475 / 3786). Goals first, then the logic if no goal took it
    (FUN_1405ada60 -> FUN_1405cfbd0, then FUN_1405d3fd0). Also reaches 101000 / 102000 /
    103000 / 104000 / 113000 / 119000 / 121000 / 502000 / 507000 / 507300 logics and the
    132000 / 507000 / 507300 battle goals. gap: the AI+0x7439 branch (14060.c:7541) that skips
    the mask build is not traced.
  - Test the_divine_dragons_trees_open_grapple_points_when_they_bloom: points appear only on a
    blooming tree, at most one object's four, near the tree, the right homing row, and
    map::grapple_target takes one as ridden. Mutation-checked (objects never switched off: 12
    points on a tree; no interrupt 39: no bloom). Filters dragon 9, ai:: 21, script, phase,
    monk, owl, ape, genichiro, butterfly, ogre, gyoubu, tomoe, boss, headless, stealth, caution,
    patrol, sound, trigger, observe, team: all green. isshin: 2 fail on the Round 93
    secs_to_finisher_done check, the same with the interrupt off (not this change).
- **TAE event gates are live, not the static NpcParam list** (overseer item 3, "CharData::fires
  static resident_gates -> dynamic gate").
  - An NPC's StateInfo-gated events (AttackBehavior / BulletBehavior / ChrActionFlag) now fire
    on the stateInfos of every SpEffect it wears now (combat.rs `event_gates`: residents, chained,
    last TAE, clash), a resident giving way to a worn one of its spCategory (sound.rs
    active_state_infos's rule). Wolf keeps player.rs's `resident_gates`.
  - data.rs `fires_on` / `events_at_on` / `attack_windows_on` (the old ones delegate with None);
    used by the AI sound loop, resolve's windows and shield block, and enemy_bullet.rs fly.
  - What changes: Lady Butterfly's second-form bullets 310-530 (StateInfo 184) fire with 3509200
    (from her a100_021000, endurance -1; the static list never had it), the c1300 3130000 (971)
    bullet 900, and the 957 / 956 armour and shield flags of 220062 / 220063 follow the effect.
    guard_half_angle follows too (entry below).
  - Test butterflys_second_form_bullets_need_her_3509200 (0 bullets without, 2 with).
    Mutation-checked (bullets on the static list: 0 with). Filters bullet 15, butterfly, guard
    44, attack 32, window 14: green.
- **Observe slots as the exe's list** (re-scout of the AI observe node; ai.rs `Observe`).
  - One per-AI list for every kind under one byte id, no cap, in insertion order
    (GetAreaObserveSlot lists the changes in that order, not by id). Node: kind 1 area, 2
    region, 3 dummy sphere, 4 dummy area; cur / prev bytes, prev = cur then cur = test each
    update; enter -> INTERUPT_Inside_ObserveArea 16, exit -> 40 (per update).
  - A fresh node starts with prev 0: a target already inside raises 16 on its first update.
    The same id and kind re-added keeps its state (a re-add every think raises nothing);
    another kind resets it to outside. DeleteObserve raises nothing, and a delete and re-add
    in one think is a fresh node.
  - AddObserveArea: the range is 3D (it was horizontal), the cone horizontal (full angle) about
    the AI_DIR.
  - New: AddObserveChrDmySphere(slot, target, dmy, r): the target strictly within r of the AI
    chr's own dummy (the Okami 504000's eight, the Orochi sight sphere 501000 / 501010).
    AddObserveChrDmyArea(slot, target, dmy, A, B, C, D): origin = the dummy moved back D along
    its +Z, D < distance <= C (3D), horizontal cone of full angle B, vertical of full angle A
    (the common battle-stealth slot 101: dmy 7, 90 / 120, 30 m, 4 m, common_common_logic_func /
    common_table_ai_common.lua:274-282, 510000_logic). Both were no-ops.
  - The dummies come in Snapshot::dummies (enemy.rs think: the markers of the ids
    `Brain::observe_dmys()` lists, world position and +Z).
  - Confirmed in the decomp (SprjAiChrDmyObserveArea FUN_140605ee0, 14060.c:5023-5234): +Z
    axis, horizontal projections within B / 2, elevation within A / 2 of the axis's, the
    target chr's plain position. gap: a slot added in a think is first tested on the next
    update.
  - AI+0x7439 (the branch that skips the interrupt-mask build, 14060.c:7541): set only at chr
    init from a spawn config byte (cfg+0x8a, 1407e.c:3580), never from Lua; interrupts are
    still delivered there, without the observe / damage / sound bits. Unidentified config,
    not modelled.
  - Test ai::tests::observe_slots_follow_the_exe_list. Mutation-checked (a kind change keeping
    its state; a delete keeping it, seen on a delete and re-add in one think). Filters observe
    4, stealth 10, caution, sound 15: green.
- **Placement groups for the call-friend sounds** (bRestrictSamePlacementGroup).
  - tools/placement_groups.py -> extracted/enemies/placement_groups.json: {map: {entity:
    [group]}} from the MSBS type-21 PlacementGroup events (32 x i32 indices into all parts; the
    group is the event's ordinal; 280 enemies over 7 maps). The exe keeps them per ChrSet
    (ChrSet+0xb8, 8 bytes per slot, FUN_140a42c70) and FUN_140610810 checks them.
  - Rule: an owner in no group reaches every listener; a listener in none hears nothing; else
    a shared group. Rows: the call-friend sounds 2000000-2155000 and 4050-5400.
  - Enemy::placement (MSB enemies: config enemy.group `msb`, enemy.rs msb_groups); AiSound
    carries the owner's groups, refreshed each frame with owner_alert; ai_sound_heard takes the
    listener's.
  - Test attacks_make_ai_sounds_that_call_allies (shared / other / none / owner in none / an
    unrestricted row). Mutation-checked (rule off: "another group" heard).
- **A growing bullet re-issues its attack** (enemy_bullet.rs fly): spreadTime > 0 and
  hitRadiusMax set and not hitRadius -> FUN_140b20b70 again every 1/6 s (the countdown at
  bullet+0x20 reset to 0x3e2aaaab, 140b0.c:8408 / 8627) while it grows and once when it has,
  each with its AppearAiSoundId again (the record refreshed at the bullet). Test
  a_growing_bullet_reissues_its_appear_sound (c1190 11903311: 4-5 issues, the 3/6 s and the
  final one share a frame or not; 11900100: 1). Mutation-checked (no re-issue: 1). Filter
  bullet 17: green. gap: the re-issue's hit list / damage side is not modelled (the hit test
  already grows the radius).
- **The guard arc on the live gates**: guard_half_angle reads the ShieldBlock flag through
  `event_gates` (combat.rs resolve and prosthetic.rs bullets; the latter without clash
  effects). The c1180 servants' ArgB 800 all-round arc (StateInfo 957) follows 220062 worn.
  Test the_armour_guard_arc_follows_its_worn_effect (90 without, 800 with). Mutation-checked
  (static gates: 90 with). Filters guard 44, prosthetic, bullet: green.

- **TAE 231 event message + the deathblow count drop at the hit (Bosees 2026-10-14, overseer order).**
  - `enemy.rs` TAE_EVENT_MESSAGE = 231 (EzStateRequestID), not 936 (ThrowDirectionDecalParam's decal). Wolf's message is latched in `WolfMessage` (events.rs `messages`, the same rule as an enemy's `Enemy::msg`) and read as a level by the map scripts (Snap.msg) and `todome_message`; cleared with the fight. Exe: TAE 231 writes chr+0x50->+0x50->+0x508 on its first frame (140b5.c:10174); the only clear is a consume-on-match Lua listener (FUN_14066c2a0 -> FUN_140bd84a0); 4[8] (EcTestChrEventMsgId, 1408f.c:545) is a pure level count, no serial.
  - Wolf's 231 values: 10 on every deathblow before its cancel windows (a211_510000 at 3.7 s, cancels from 4.3 s), 50 on every death anim (a000_110000 ...), 60 / 90 / 100 on a few finishers; no Todome a2xx_511700 sends one. So the 10 of his previous deathblow is still there at the Todome unless he died since.
  - The deathblow count (chr module +0x25c, ninsatuNum at NpcParam +0x126) drops when the throw's atkAttribute 5 hit applies (FUN_140b6e6a0, 140b6.c:8770; Wolf's ThrowAttackBehavior judge 800, a211_511700 at 1.23 s), not at the throw's start. New `Enemy::ninsatsu_unhit` / `ninsatsu_left()`: the AI's GetNinsatsuNum, the scripts' 4[37] and the phase rules see the old count until Wolf's anim passes an attribute-5 judge (or he leaves his Deathblow state); the reaction choice in `Enemy::deathblow` is unchanged.
  - Result: the Monk's Todome cut (12505963) comes at 1.25 s, matching the clip (4.57). Filters green: deathblow 34, throw 19, monk 6, isshin 10, ape 5, owl, genichiro, gyoubu, demon 2 each, butterfly 3, emma 1, dragon 9, phase 3, todome 8, every_boss_row.
  - gap: 4[37]'s evaluator reaching +0x25c is inferred (FUN_140748480 fits); the decrement's skip rules (NoDamage, an escaped throw, the outer param_4 path) are not modelled.
- **Stopped here (Bosees 2026-10-14, user's stop via the overseer).** The 231 switch above is done and green; the tree compiles. Next, not started: overseer order 2, the per-boss behaviour check against clips/sekiro_all_bosses_oS_gJf8ur8M_1080p60.mp4 in the order Genichiro, Isshin, Butterfly, Owl, Monk, Ape, Demon, Gyoubu, Emma, Dragon.
  - Tool: ignored test `zz_boss_moveset_trace` (`BOSS=c7100:71000000 SECS=120 ... -- --ignored --nocapture`). Wolf locks on, walks in past 3.5 m, deflects every non-perilous hit and slashes when the boss recovers. It prints each boss state with time, distance, ATK and the perilous kind, then attacks/min and the perilous share.
  - Boss ids: Genichiro c7100:71000000, Isshin Sword Saint c5400:54000000, Isshin Ashina c5430:54300000, Butterfly c5090:50900000, Owl c5060:50600000, Monk c5000:50000000, Ape c5100:51000000, Demon c7020:70200000, Gyoubu c5080:50800000, Emma c7400:74000010, Dragon c5200:52000000.
  - Genichiro so far:
    - The clip's 0:24-4:48 is the scripted prologue fight, not row 71000000. Gameplay runs 61.5-127 s into that segment. It shows about 8 perilous in 65 s, mostly close-range thrusts (Mikiri dust), no bow, and constant close-range slash strings.
    - The sim at 120 s gave 18 attacks/min with 4 perilous (11%: 3020 grab, 3062 / 3025 thrust) and no bow. He was never ≥7 m when choosing an act, which is the Lua's Act10 / Act15 far branch.
    - Clip times: the clips.md chapter starts include walk-ins and cutscenes. Use the measured fight moments listed under the table first (bug fixes, 2026-10-14).
    - Next step: contact-sheet the castle fight 22:27-28:16 (ffmpeg -ss before -i, fps=2, tile 6x5) and compare perilous rate and bow use before changing anything.

### 4.43 Bug hunt round 22 (2026-10-12)

Round 22 finders on the Lua 5.0 VM (ai/lua50.rs), prosthetic tools, gore / vfx / status, enemy
grabs / area music / fmod; then a sweep of AI host methods the scripts read as booleans or
numbers but ai.rs left to the num(0) fallback. VM, grabs and music: nothing solid.

- **Tool bullets dropped magic and dark damage** ("bug fixes", fixed): prosthetic.rs summed only
  physics / fire / thunder, so the Lazulite Shuriken 70500 did 0 HP, Lazulite Sacred Flame 72300
  (magic only) and Projected Force 760000-760020 (dark) did nothing at all (no burn build-up
  either). Projectile.base is now the EquipParamWeapon id and the damage goes through
  combat::player_attack_damage (all five elements x NpcParam cut rates); deals_something also
  counts magic / dark. Test the_lazulite_shuriken_deals_its_magic_and_dark_damage.
  Root cause of the 0 HP was also in the export: combat_data's EquipParamWeapon held only 70000
  of the tool levels, so every other level (bullets AND swings via player_attack_damage) read an
  empty row. export.rs now adds every 70000-79999 row (106 -> 145 rows); combat_data.json
  re-exported 2026-10-13.
  Not reported (gaps): bare-judge 290 / 295 bullets on Ignition LV4 / Spear LV3-B; bullets skip
  the player's stam_atk_rate.
- **Poison mist / burn flames vanished after 20 s** ("bug fixes", fixed): status.rs show spawned
  the midst FXR (4011 / 3012, dummy 850) without `keep`, so fxr's 20 s age cap ended it while the
  affliction (9050: 39.9 s) still ticked. Midst effects now keep; tick / clear stop them.
- **GetDistY / GetDistYSigned read 0** ("bug fixes", fixed): target.y - me.y (positive = above;
  101030 Act27 backs off to height / tan when Wolf is >= 3 m up; 106000 compares GetDistY with
  -1.8, so signed). Test dist_y_is_the_target_height_over_me.
- **INTERUPT_InactivateSpecialEffect never raised** ("bug fixes", fixed): enemy.rs passes sp_gone
  (prev_sp minus now), ai.rs raises interrupt 44 for observed ids and answers
  GetSpecialEffectInactivateInterruptType (115000 / 118020 / 120000 / 102020 wait on 109220 /
  5026-5028 ending); the activate getter now answers only for interrupt 43. Test
  an_ending_sp_effect_raises_the_inactivate_interrupt.
- Still on the num(0) fallback (boolean-ish uses, mostly common goals not on the regular path):
  IsInterruptSubGoalChanged (unreached, round 21), GetAIAttackParam / IsAIAttackParam,
  IsTargetOutOfRangeInterruptSlot, IsFinishAttack / IsHitAttack (common_attack3), IsLanding,
  IsBothHandMode, IsEnableComboAttack / IsEnableCancelAttack / IsEnableCancelMove, ladder and
  obj-act queries. Scanner: scratch aiscan.py logic = every `x:Name(` in extracted/ai_src not
  quoted anywhere in src/ai.rs.

### 4.44 VFX and Maps: fog-wall collision, GIIV, bloom check (2026-10-12, user away)
- **Fog-wall collision** (exporter map.rs, src/map.rs `set_gates` / `Terrain::gates_from`,
  src/fog_wall.rs): on the castle roof the wall object's own box (o001401 hkx, 12 tris) and the
  20005825 part h900800 (24 tris, an octagon of walls round the roof) were baked into the static
  `.hit`: Wolf was shut in for good and the walls never went after a win. The exporter now
  writes every fog_walls.json object / collision entity to `map_<id>.gates` (SHMG v1) instead;
  fog_wall.rs switches them on while its walls stand and `Terrain::slide` pushes bodies out
  (only on the terrain they were switched on for). Test `fog_wall_gates_block_only_while_on`.
  Re-exported: the m11_01 arenas only (boss_71001000 has the gates file). The gate map has no
  fog wall in range (83 m off).
- **GIIV** (exporter `export_giiv`, `MAP_ENV_ONLY=1`; src/map.rs `apply_giiv` / `load_giiv` /
  `GiivParams`; map_material.wgsl `giiv_light` / `lit`): the probe's four BC7 volumes -> one
  RGBA8 3D texture, sampled per fragment as the diffuse ambient in place of the cube's
  (sekiro-rs SHADERS.md "GIIV" decoder and basis, 6de8ff6 placement; THIRD_PARTY.md).
  `SHINOBI_MAP_NO_GIIV=1` turns it off. The gate's probe files were re-exported env-only: its
  probe is now Env_Box280 (the bottom-centred pick, pending since the msb.rs change). Photo-mode
  pairs at 12 h: no artifacts, map surfaces 3-4 / 255 darker, more contrast at the doorway.
  The two variants around the hour blend like the cubes (checked at 15 h).
- Photo mode (`SHINOBI_PHOTO`) quits after ~6 s without shots with the user's current config
  enemy c7400 (a boss row); normal play and HUD shots run. Not this track's (boss / photo.rs).
- **Bloom on screen** (photo mode, 12 h gate, map bloom vs `SHINOBI_MAP_BLOOM=camera`): no
  artifacts, no veil; both restrained and nearly alike (judge: could glow a little more on sky
  and snow). Left as is.
- Sky 18 h and the B4 brightness fit: see section 5 (sky gap; brightness compared 2026-10-13).
- **Sources (user's rule, 2026-10-13: nothing new from sekiro-rs without the user's OK per item;
  merged ports stay unless the user asks):**
  - From the game itself: the fog-wall collision (the MSB parts, the hkx meshes, the EMEVD
    20005820 / 20005825 entities); the GIIV volumes' data, their Env_Box (MSB) and the
    BC7 decode; the hour blend follows Sv1's own cube blend (`probe_maps`).
  - From sekiro-rs (already merged, the user decides): the GIIV texel decoder
    `2 (rgb - 0.5) 2^(20 (a - 0.5))` and the basis `max(n.x X + n.y Y + n.z Z + W, 0)` (its
    SHADERS.md, read from the exe's shaders by sekiro-rs), the box-to-texture axis mapping, the
    one-voxel edge fade and the one-voxel normal bias (its 6de8ff6), the scale = the cube's
    `ENV_PER_UNIT` (SCENE_TO_BEVY, also sekiro-rs). Older merged ports this track builds on: the
    fog walls' event reading (tools/fog_walls.py, after its fogwall.rs), the bloom level weights,
    the sky domes, the height fog, SCENE_TO_BEVY / METER / adaptation (THIRD_PARTY.md).
  - Stopped: B4 brightness / warmth (built on sekiro-rs SHADER-INPUTS values) and the 18 h sky
    (would need the shader; sekiro-rs has none). Not done; left as they are.
- **Load time / fps, sekiro-rs ports on vs off** (2026-10-13, "bug fixes"' request for the
  user; target/map release build, a temporary first-frame log since removed; off =
  `SHINOBI_MAP_NO_FOG=1 SHINOBI_MAP_NO_GIIV=1 SHINOBI_MAP_BLOOM=camera SHINOBI_MAP_PROBE_SKY=1`;
  config enemy c7400; 3 passes, 4 runs per map each, other sessions compiling in the first two):

  | Map | Ports | Map pieces load | Launch -> first frame | fps (5-15 s), quiet pass |
  |---|---|---|---|---|
  | gate m11_01_00_00 | on | 1.0-1.5 s | 7.8-15.3 s | 18, 24 |
  | gate | off | 0.7-1.3 s | 7.2-22.9 s | 19, 21 |
  | castle top boss_71001000 | on | 1.5-2.5 s | 8.5-41 s | 30, 30 |
  | castle top | off | 1.2-3.3 s | 7.5-23 s | 29, 26 |

  No difference above the run-to-run noise in either load or frame rate. The first frame is slow
  (7-23 s, once 41 s, with ports on or off alike: the start-up, not the ports); the map pieces
  load in 1-3 s; the new data the ports read is small (the GIIV volumes 2 x 82 KB). fps here is
  far below kb/map.md's 72-80 (the window ran unfocused beside other builds; the c7400 enemy).
  Bugs traced to the ports (this track): the fog walls' baked collision (fixed, above); the
  probe pick before the bottom-centred box rule (fixed 2026-10-12, gate re-exported env-only);
  the 18 h sky look (open, section 5).
- New: exporter `hitmesh <hkx> <compendium>` (a collision part's counts, bounds, materials).
- Docs: kb/map.md "2026-10-12 Fog-wall collision and GIIV". No commits (user away).
- 2026-10-13 lock-on camera (B3, static exe only): `lockCamAdjustRot_MaxX/Y/ChaseRate` are
  copied and blended with the LockCamParam row but never read (nothing to port); the lock switch's
  mouse value is the raw DirectInput count per frame (FUN_141a8f540), the same unit as Bevy's
  `AccumulatedMouseMotion`, so 50 / 25 stand as is (camera.rs comment only). Details kb/camera.md.
- 2026-10-13 B4 reference: the user's all-bosses clip 4:48-5:22 (General Kawarada, outskirts gate,
  used for lighting only) measures logL -5.0..-6.6, 4.5-6.9 stops, warmth +13..+36, sat 79-106,
  overcast lavender-grey sky. No EMEVD sets a time of day for that first visit (2003[81] only on
  Genichiro / after / Isshin / ending), so its hour is the engine default, not traced. Kawarada
  stands in m11_00_00_00 (entity 1100300, [217.5, -40.4, -5.3]), not in our m11_01 gate map;
  boss_71001000 (his fallback arena) is the castle top and no match.
  Exported that spot as a lighting reference, `map_look_m11_00_gate` (radius 60 around entity
  1100300, light set 100, GIIV giiv0220; `SHINOBI_MAP=look_m11_00_gate`; not a boss arena, no
  index entry; script scratchpad look_export.py, raw map deleted after). look.py, start view
  (palisade + camp curtain), vs the real frames (logL -5.0..-6.6, 4.5-6.9 stops, warm +13..+36,
  sat 79-106):

  | Hour | logL | range | warm | sat |
  |---|---|---|---|---|
  | 6 h | -3.7 | 4.7-5.4 | ~0 | 32 |
  | 12 h | -2.9..-4.2 | 3.0-4.5 | -4..-9 | 25-29 |
  | 18 h | -4.0..-5.3 | 2.6-4.4 | +13..+15 | 53-57 |

  18 h is the match (exposure within ~0.4 EV, warmth in range, hard low sun), so `[world] hour
  = 18` stands. Left: the dark end ~2 stops lifted (p10 -6.6 vs -8.5) and half the saturation
  (the camp curtain m11_camp_curtain_20_a reads olive-brown, real is vivid orange); the m11_00
  gparam has no saturation / contrast field (ColorGrading = LUT id only, Bloom Sturate Ctrl 0),
  so no Sekiro data to tune by; the flat yellow 18 h sky shows here too (section 5 gap).
- 2026-10-13 Shahei occlusion traced (kb/sound.md): a portal / route search over the MSB
  occlusion boards / boxes / portals (CSSoundOcclusionMan, `UseMSP`), no ray cast; step 0.1 a
  call. Not ported (needs the MSB occlusion points exported first; little use on open arenas).
- 2026-10-13 the game's sky shader decoded (M[Sky]_SkyDome_Ver2, kb/map.md "Sky domes"): colours
  are rgb^2.2 x Intensity (sky.rs, was gamma 2), switch 1 lights by the MTD's float3_6
  (sky.wgsl); the rest of our Ver2 branch already matched the game's terms. The same 2.2 rule is
  set for every material colour: the map exporter's layer colours fixed and the gate map + all
  arenas re-exported; **character shaders still raw** (main.rs layered / Fresnel params: owner
  of the model export, please apply `powf(2.2)` before the Intensity and re-export). Tests: sky 2,
  map:: 11, extractor 1 pass. On the gate look map the change is small (saturation 57 -> 56-60);
  the 18 h sky patch stays flat cream (clamped at 1 by the game's own shader).
  Re-export 2026-10-13 04:22-04:28: m11_01_00_00 (gate) and all 10 boss_*.bin via
  `boss_arenas.py -force` (SEKIRO_EXTRACT must be an absolute Windows path). boss_12600050 (the
  monkeys' hall, another session's 04:01 export) came out with the same settings and content
  (MAP_PICK 2000802, radius 85: 968 pieces, 4.19 M triangles), 600 MB. Tests after: map 14, fog_wall
  4, every_arena 1 pass.
- 2026-10-13 sky clouds drift as the game's shader does (sky.wgsl `flow_axis` / `flow_sample`:
  world +X in the per-pixel (B, T, N) frame, from screen derivatives since the dome has no
  exported tangents; U offset 0.075 x float_38 / 41, two-phase crossfade at 2 t x float_39 / 40).
  sky.rs passes the game's +X in arena space (`flow_x`). Check (12 h, camera up): plausible
  clouds, no seams or speckle, the sky changes between shots 6 s apart (mean diff 0.3 vs 0.01 on
  static timber). New debug knob `SHINOBI_CAM_PITCH=<deg>` (camera.rs start pitch, negative looks
  up; HUD shots then skip their lock-on, hud.rs). Left on the sky: the glow target.
- 2026-10-13 the sky is fogged as the game's sky shader fogs it (at its own view depth, section 6
  of docs/kb/sky_shader.md): the opaque dome is now drawn at its real distance (3 km up, 6.4 km at
  the horizon) and writes depth (sky.rs specialize, sky.wgsl vertex), so height_fog.wgsl fogs it
  like any surface; the blended Cloud layer writes none. Check at 12 / 18 h: no missing or hidden
  geometry, the 12 h sky hazier and paler. Tests: sky 2, fog 7, map 14 pass.
- 2026-10-13 map material batching (sekiro-rs 73eca1d, the user's OK; THIRD_PARTY.md). `MapExt`
  is bindless: its uniforms pack into one `MapUniform` (`#[data(100, MapUniform,
  binding_array(124))]`), textures 101..122 go through the index table (binding 123), the two
  GIIV volumes included (`bindless_textures_3d`). map_material.wgsl: `load_map` copies the
  uniforms into private globals at the top of `fragment`, the `s_*` helpers sample either way,
  the base colour comes from `pbr_bindings::material_array` under BINDLESS. Overlap: this is the
  layered map material the user gave to someone else on 2026-10-12; only the binding layout
  changed, not what it draws. Startup logs `map material bindless: true` (RTX 3080, Vulkan);
  test `map_material_is_bindless`. Measured with the bug fixes session's `SHINOBI_BENCH=20`
  (config vsync on), gate m11_01_00_00, machine busy (four orphaned `find` processes, two
  other builds): before 54.9 fps, p50 18.04 ms, render wait 11.49 ms; after 51.9 fps, p50
  16.45 ms, render wait 11.35 ms (the average dragged by stalls from the other builds). So
  batching works but is not the render-thread cost; vsync-off runs (`SHINOBI_FPS_LOG=1`) on that
  machine gave 1-3 fps for both builds, unusable. Re-measure when it is quiet; next lever comes
  from the trace_chrome spans.
- 2026-10-13 `[world] map = "arenas"` (the user: keep only the arenas for testing; config.toml
  switched, the old line kept as a comment there): `map::ARENAS_ONLY`, a fight loads its exported
  arena, any other fight the flat test floor, never the base map; a swap to a fight without an
  arena keeps the loaded map until a relaunch (warns). Test `arenas_only_loads_just_the_arenas`.
  Ask the user before putting m11_01_00_00 back.
- 2026-10-13 the perilous kanji is the game's FXR 600035 (Bosees' enemy_bullet.rs spawns it):
  `tools/fxr_extract.py` now follows SpEffect -> SpEffectVfx for `SP_EFFECTS` (211000 -> 48100 ->
  600035, textures 6301 the 危 glyph, 11419 noise); the run also added 37 effects newer enemy data
  uses (1198 now). Sim tests `*perilous_kanji*` pass. Seen in game (`SHINOBI_AI_MODE=perilous`,
  new knob in enemy.rs `EnemyDebug::default`: passive / perilous / thrust / idle): the glyph is
  drawn on Wolf's dummy 220 (chest) and follows it; next to the clip (combat flow, 0:06.5) ours
  is a faint pink on the bright roof where the game's is vivid red with a white core over a dark
  ground - additive red over a bright floor, and lighting -2 takes the 1/8 lit rule; not tuned
  (check over a dark floor first). From behind at the default pitch Wolf's back can hide it.
  2026-10-13 dark-floor check done (`SHINOBI_FXR_TEST=600035`, sandbox 12 h / 0 h and
  boss_71001000 0 h): over the night sky and the hall's dark wall it is vivid red with a bright
  core like the clip; only over the bright noon sky is it faint pink. So the look is the
  background under an additive effect, not our FXR blend: nothing to tune in the effect. (fxr.rs
  test_shots took Wolf's +Z as his front, so "ahead" was behind him, between him and the
  camera, and this effect's +0.75 m node put the glyph above the frame: now his -Z. Older
  `SHINOBI_FXR_DIST` notes were measured backwards.)
- 2026-10-13 fxr.rs bug: node type 2202 was read as levels of detail (first child only); it is
  the NodeEmitter (reference/fxr NodeType), 2002 is LevelsOfDetail. 66 effects have NodeEmitters
  with 2-6 templates and drew only the first (blood 220502 / 220507, 214000-214004, 300002 ...);
  the 5 LOD effects (440011, 6108xx, 611400) drew every level. Test
  `node_emitters_keep_all_children_levels_of_detail_one`. `SHINOBI_FXR_LOG` now also lists the
  604 multi-texture quads ("multi l1+l2+l3/blend:n").
- 2026-10-13 HUD R34 (bug fixes session's finder, verified in the decomp): posture state is
  FUN_140a02a10 -> chr+0x1a40: 0 normal, 1 cur/max <= DAT_143b06724 / 100 (a static 30.0 debug
  tweak, not MenuParam PlayerDangerSp), 2 posture full, 3 full + ChrActionFlag 0x160 (sticky while
  the flag holds); FUN_140d4d910 fades the FE clips State_0..3 under Gauge Damage / SPCenter /
  SPFrame (player and enemy gauges). Decoded from menu/01_000_fe.gfx (scratch gfx/ tools of
  this session): the fill (MENU_Taikan_bar, grows from the centre) changes colour by the Bar
  clip's State_0..3 labels: x1 / x(0.801, 0.898, 0) + (40, 6, 0) / x(0.898, 0.801, 0) + (40, 8, 0) /
  x(1, 0.699, 0) + (40, 10, 0); the frame art MENU_Taikan_ST1-3 (gold, orange-red, red) and
  the Damage layer (end arrows, State_1 red glow MENU_Taikan_Effect, State_2 arrow pulse every 32
  frames, State_3 break flash) carry the red. No "SPCenter" clip exists in the file. Done:
  hud.rs `posture_state` / `POSTURE_FILL` (test `posture_bar_follows_the_game_states`); gaps: the
  frame art, the glow / flash layers, state 3. H4 done: the player's
  posture gauge stays MenuParam SpMinimumDispTime 0.5 s after it is back to 0 (140e3.c:7637-7661;
  hud.rs `SP_MIN_DISP_TIME`). H2 (deathblow pips: ninsatuNum, drawing not traced) and H3 (8 enemy
  gauge slots, which 8 not traced) open.
- 2026-10-13 `apply_giiv` runs only when the map, hour or material count changes (was 0.48 ms a
  frame of file checks and a JSON parse, the bug fixes session's chrome trace).
- 2026-10-13 bosses are split (docs/PLAN.md top): the 22 main-boss rows are ours again; the
  user asked this session for their cloth, VFX and SFX. Boss effect audit, priority order:
  (1) enemy bullets drew no effects, (2) Demon of Hatred embers 670240 / 670271 (GPU node 10000)
  not drawn, (3) nodeAudio not played, (4) Isshin fxr 654061 and Owl 650630 missing from the
  export, (5) nodeMovement 113 unhandled (671004 Genichiro / Tomoe, 650923, 650603), (6)
  camera-attach 46 (Butterfly 650935, Monk 839653), (7) second-container cloth sims not exported,
  unresolved TAE sound keys, Owl Father texChr 5069 unused.
- Done (1): enemy_bullet.rs bullets carry sfxId_Bullet (else the parent's with
  isInheritSfxToChild, as prosthetic.rs) as an FxEffect anchored on the bullet; it stops and
  detaches where the bullet ends, and sfxId_Hit plays where it landed. A hit on Wolf that
  combat.rs judges carries both ids in `Attack::bullet_fx` (data.rs, serde skip): combat.rs plays
  sfxId_Hit on a hit, sfxId_Flick when guarded or deflected. Tests
  `enemy_bullets_carry_their_effects`, `a_guarded_bullet_leaves_its_shooter_alone` (flick).
- Done (3): fxr.rs plays nodeAudio type 75 (FMOD event `s<id:09>`) once when the node's delay
  has passed, follows the effect, stops when it ends; ffx kinds 96 / 118 skip their own id
  (sound.rs plays it already). Test `node_audio_starts_once_after_the_delay`.
- Done (2): GPUStandardParticle (appearance 10000; only 670240 / 670271 Demon of Hatred
  a000_003043 / 3044 / 3047 at dummy 260, 650133 c5010 a200_012320 / 13700, 650541 c5050
  a000_020000): fxr.rs `gpu_emit` / `gpu_emissions` / `gpu_move` and a draw branch, from the
  reference/fxr 10000.yml / EmitterShape.yml fields (emitter shape, size as full extents,
  distribution, f1_5 = 1 velocity along the outward axis, counts per 1/30 s frame, accelerations,
  gravity, random turns, trace stretch). Checked against sekiro_all_bosses 1:22:01-1:22:03 (fast
  outward streaks at the burst, then slow scattered dots): the burst matches; gaps: the slowing
  after it (ours keep flying 6 s; see below), trace length unit (taken as 1/30 s frames),
  bloom, colorMin / Max. Test `gpu_particles_burst_out_of_their_cylinder`. Screens:
  SHINOBI_FXR_TEST=670240 SHINOBI_FXR_DIST=16 (new: how far ahead of Wolf the test effect plays).
  The game's update shader (shader/gxffxshader.shaderbnd CS_VFX_StandardParticle, D3DDisassemble)
  has a drag: velocity *= exp(-k * dt), k = parameter-texture row 7 at the particle's life
  fraction (no per-particle random; rows 1-4 have one: row 1 = acceleration xyz + w added to y
  (gravity), row 4 = colour, clamped >= 0). Which 10000 field fills row 7 is not found (the
  filling code in the exe, near FUN_14040ada0 / 14047.c, not traced). unkParticleAcceleration is
  not it (reference/fxr: time sq, acts like accelerationZ). Every unnamed scalar left is 0 on
  670240 (p1_14 / p2_0 / p2_1 are 1 on all 8 extracted GPU nodes), so its slowing in the clip
  does not seem to be this drag. Candidate for later: unk_ds3_p1_15 (a 0 -> 32 curve on the
  650133 / 650541 sparks). Not implemented.
- (4) 654061 and 650630 are in none of the game's sfxbnd archives (commoneffects, m10-m25 all
  unpacked): the TAE names effects the game does not ship; nothing to draw.
- Done (5, 6): nodeMovement 113 NodeAccelerationSpin (as 1 plus angularSpeedX/Y/Z x multipliers)
  and 46 NodeAttachToCamera (node on the active 3D camera; followRotation off: position only;
  FxrLib.cam set each tick). Test `node_spin_and_camera_attach_move_the_node`.
- Done (7, cloth): tools/sekiro-extract cloth_export.rs exports every sim cloth of a container
  (the second as "<name>#1"): Move / Simulate ops by simClothIndex, deformers / gathers by their
  input buffer (the k-th "SimBuffer"). Our bosses with two-sim containers: c5000 / c5005
  Corrupted Monk Sleeve#1 (74 particles, 26 bones), c7400 Emma sleeve#1 (50, 12 bones), c5060
  Owl Chainmail#1 (81, 2 bones). Also fixed: the last sim's Simulate config overwrote the first
  sim's (Owl's c5060_coat had execution [] - run as every set in stored order, cloth.rs - and 1
  substep; now its own 7-entry order and 2 substeps). Chained cloth: c5060_coat#1 (the coat's
  3226 feather strands, 6452 particles) has no skinned references - its fixed particles follow the
  coat sim through MeshDeform_2. Exported as "ref_deform" (the source cloth + that deformer's
  slots; fixed-particle bind err 0.108 m) and run in cloth.rs from the coat's particles of the same
  frame (the cloths of a character run in order; a missing source carries it rigidly); the coat no
  longer draws the fur mesh (a deformer feeding another sim's reference buffer is no display).
  Owl in play: 0.94 ms for the feathers, 1.55 ms all cloth. Re-exported model_c5000 / c5005 / c5060 /
  c7400.cloth.json only (the .bin files left as they were). Test
  `second_sim_cloths_are_exported`; Emma in play: one restart at spawn, then steady.
  Regular enemies re-exported the same way (cloth.json only): c1060 (mino1#1 now draws mesh 4,
  which the 59-particle first sim drew with the wrong triangles), c1110 / c1111 / c7520
  c1100_sleeve#1, c1220 juzu#1, c7200 / c7210 / c7401 / c7440 / c7450 / c7530 sleeves (#1), all
  with their own Simulate config; c5040's second sim has no move op (left out); not re-exported:
  c1080 / c7000 (the outside person's), c5061 / c7201 (no cloth.json exported). In-game c1060:
  one restart at spawn, 1.33 ms. Old files: scratchpad cloth/old (this session).
- (8, sounds) Boss sound audit (scratch fev-list of each bank + main + smain against every TAE
  PlaySound key, 'x' floor / 'b' armour skipped): all of c5000 / c5060 / c5080 / c5090 / c5100 /
  c5200 / c53x0 / c7020 / c7400 resolve; c1260's 35 c125xxxxx keys come from its animChr c1250,
  whose bank load_new_kinds already loads. Left: single cues the banks lack (Isshin c099999987,
  c540006111, c540008103; c5410 c541005001 x3; Genichiro / Tomoe c131007100, c711094001):
  silent in the game too (fmod.rs "an event its projects lack is silent").
- (8, Owl texChr) not a gap: every c5060 NpcParam row has normalChangeTexChrId 5069, c5060 has
  no texbnd of its own, and extract.ps1 passes c5069 (family pack and texChr): its 19 textures
  are in extracted/tex. The boss audit list is done; open: GPU ember slowing.
- 2026-10-13 effect time rate: fxr.rs reads the root's Unk10500 rateOfTime (42 extracted effects
  run at 0.5-3, the boss effect 650680 at 1.375; was always 1). Test
  `effects_run_at_their_time_rate`. Audit of the 304 effects our bosses use: every appearance /
  movement / emitter type is handled now; only generic slots remain (10100 / 10400 / 130 unknown).
- 2026-10-13 SpEffects from an enemy's own TAE (67 / 66 / 401) show their SpEffectVfx (ffx.rs
  `tae_sp_vfx`, as status.rs `show` does for afflictions; status.rs VfxRows made pub(crate)): the
  Demon of Hatred's burning body 3702011 -> 670215 / 670216 (dummies 820 / 821), the Ape's
  3510500 -> 651050, the Divine Dragon's 3520000 -> 652012. fxr_extract.py now also converts the
  VFX of every TAE SpEffect (1214 effects). Test `the_demons_burning_body_shows_while_his_tae_says_so`
  (not seen in play yet: in two 30 s runs the Demon did not use those attacks; the camera sits
  inside him in the sandbox). the resident 23061x "enemy turn" glow: not a gap (2026-10-13, see below).
  In-game checks of other enemies without touching config.toml: SHINOBI_ROOT=<dir> with
  junctions to the project folders and an own config.toml.
- Note: the claude-video watch script tried a Gemini upload of the 2.7 GB clip despite the
  local-engine setup (refused: too large); use `--engine local` or ffmpeg contact sheets.

- 2026-10-13 **GIIV several boxes** (kb/map.md "GIIV boxes"): the game's own
  GXEnvMapRegion[Box][Diff] shader (gxrenderershader.shaderbnd) draws every Env_Box as a light
  volume, weight smoothstep(metres to the nearest face / fade distance), blended over the
  boxes before it; the MSB type data gives the fade distance (0x04) and the priority (0x09:
  0 map, 10 play area, 20 areas, 30 rooms, 40 burning hall, 90 reflection). Exporter
  (`export_giiv`, msb.rs `Region::env`, new `envboxes` command): the boxes within the arena
  radius, nearest 16, cropped to radius + 30 m, one atlas per variant (SHGI v2); runtime
  (map.rs `GiivBox` / `giiv_params`, map_material.wgsl `giiv_light`): blended in priority order.
  All 12 exported maps' probes re-exported env-only (scratchpad giiv_reexport.py: only the
  envmap packs unpacked, deleted after). Checks: photo pairs on the gate look map and the
  castle top: no seams; the castle's walls no longer near-black. Tests: map:: (+
  giiv_boxes_fit_their_atlas), extractor 1. Gaps in kb/map.md.
- 2026-10-13 **B4 / curtain traced** (section 5): the curtain is a plain MetallicValue
  material with a brown texture and the game's combine is irradiance x albedo, so B4 is light /
  post only (waits for a real frame). Found instead: map emissive materials are not drawn
  (section 5 "Map emissives", formulas in kb/map.md). The extractor's `MAP_LOG` lists each
  mesh's MTD (scan of the gate: scratchpad mtdscan_gate.log).
- 2026-10-13 the GPU ember drag: see the GPU particles entry above (shader row 7; no field).
- 2026-10-13 **Manual test #5, Genichiro / Tomoe lightning look** (fxr.rs tracers). Scout map: c7100
  Genichiro, c7110 Way of Tomoe, c5400 Isshin; lightning FXRs 671020 charge, 671022 / 671021 / 671023
  bow bolt, 671026-29 / 671032 sword lightning, Tomoe 671120-34 / 671142 / 671144, 440680 / 1,
  Isshin 654140-49: all extracted, played by ffx.rs (TAE 96) and enemy_bullet.rs, appearance types
  all drawn. Against the all-bosses clip 26:02-26:18 (jagged white-gold bolts): our DynamicTracers
  (10012 / 606) ignored the flipbook: 33020 is 16 bolts stacked (totalFrames 16, columns 0), drawn
  as the whole atlas across the trail -> dotted straight lines. Now `tracer_frame` (frameIndex +
  frameIndexOffset cell) across the trail and textureFraction of the width per segment along it,
  fixed where laid (attachedUV off) or for the whole trail (on); fxr_tex batch textures repeat in U
  (clamp V). Night shots (sandbox 0 h, SHINOBI_FXR_TEST 671020 / 671032 / 671022 / 671023): jagged
  bolts like the clip. Test `a_tracer_draws_one_flipbook_frame`. gaps: the sword effects (671026-29)
  ride the blade and need the swing (static test: zero-length segments): check in a fight; bolts a
  bit more orange than the clip's white cores (HDR col ~(43, 31, 24): tonemap / bloom, lighting paused);
  671026 model 8180 (color3 1000) floods orange in the static test; the bow bolt's model 8112 reads
  green ((6, 12.5, 2.5) x 12 x its texture s08111_a); Wolf's lightning-charge glow (now done, see
  the next entries); the same path with other settings elsewhere would share
  the first load's sampler.
- 2026-10-13 **Manual test #2, cloth (cape over-moving, B2 / section 7 item 2): the sim now runs
  in model space** (cloth.rs `carry`). Exe (decomp 14157.c / 14156.c): hclSimulateOperator::execute
  FUN_14156fbc0 moves particles by the character's motion only in "Transfer Motion"
  (FUN_141578d50: the transfer bone's delta, speed-blended, onto current and previous positions),
  and that runs only when its translation / rotation flag is set (FUN_141564780; FUN_141567890
  logs "not enabled") - off in all 435 exported cloths; nothing else moves the particles, so the
  sim is in the pose buffers' (model) space and "Update Effective Damping" (FUN_14156eee0,
  pow(1 - globalDampingPerSecond, dt)) damps motion relative to the model. Ours ran in world
  space: Wolf's 0.99 then acted as air drag, his waist cloth trailed 30-60 deg for a whole run.
  Measured (new `SHINOBI_CAPE_TURN=90|180`, `SHINOBI_CAPE_SHOTS=<dir>`, cloth.rs `cape_probe`
  swing from rest per cloth) against the user's clips (Medal 20261011222656376 99.5-101.5 s /
  144-146 s, 20261010013052179 8-10 s: run sway 10-25 deg, turn flares 50-70 deg in combat
  pivots, settle 0.3-0.5 s): before bodybese01 / 03 30-60 deg running, now 5-25; settle ~0.5 s.
  Every character's cloth (the bosses' too). Test `cloth_rides_with_the_model_root`. gap: Transfer
  Motion itself (all off) not ported; the bosses' cloth not compared with a clip yet; bodybase02
  (a 9 cm piece) reads 25-55 deg running (noisy short lever).
- 2026-10-13 **Manual test #4, effects lingering after arts / tools** (ffx.rs `ends_with_event`):
  only slotted (SlotID >= 0) and 118 effects ended with their TAE event; the rest played to the
  20 s cap if a node is endless (~790 of Wolf's events, nearly all the a07x tools and a10x arts:
  300080, 304601, 303022, 440460 ...). The game sets an effect's External(0) to 1 when a
  SpawnOneShotFFX event with IsRestrictToDummyPoly ends (reference/fxr src/fxr.ts
  ExternalValue.Sekiro.Terminate: "the source of the effect going away"), and 1211 of the 1213
  extracted effects have "External(0) >= 1 then terminate": those events now stop their effect
  with the event or the anim change, as slotted ones (enemies too). Test
  `restricted_one_shot_effects_end_with_their_event`. Sweep (`SHINOBI_SKILL` for the 10 tools and
  9 arts, new `SHINOBI_FXR_LIVE=1` log): nothing older than 4 s but finite data lengths: the
  Shuriken's hit 300082 (a 10 s node, at the target), 440071 (Iai, IsRestrictToDummyPoly off, 4 s),
  303025 / 440003 stopped with 3 s particles finishing. gap: the 2 effects without the terminate
  state stop as well; bullet hit effects (no TAE event) play their full length.
- 2026-10-13 **Manual test #5, Wolf's lightning-charge glow** (status.rs `electro_glow`, read-only
  on Actor.electro, its logic unchanged): while `electro` holds a SpEffect its SpEffectVfx plays on
  Wolf's dummies (`spawn_vfx`, shared with `show`: init effects one-shot, midst effects held) and
  the held ones stop when the charge clears or changes. Wolf's own rows: 9490 -> vfx 6000 midst
  404041, 9495 -> vfx 6001 midst 652030; the enemies' charges 9491-9494 (combat.rs takes the attack's
  9490-9495 SpEffect) have no row on Wolf, so they show his 9490. Test
  `wolfs_lightning_charge_glows_while_it_lasts` (charge 9492). gap: not looked at in game yet.
- 2026-10-13 **Manual test #2, the bosses' cloth: Genichiro (c7100) against the clips; compressible
  links.** Clip: all-bosses 0:24-4:48 (grass field, too far for the fight) and the castle-top rematch
  22:27-28:16. Cutscenes: mantle 0-10 deg at rest. Fight: 30-60 deg in leaps, at most ~80-90 deg
  (never above horizontal). Kusazuri at most ~30 deg (low confidence: 0.5 fps sheets). Ours (new
  `SHINOBI_CLOTH_PROBE=1` with SHINOBI_FIGHT=c7100 + SHINOBI_DUEL: each enemy's anim, speed and
  per cloth the swing from vertical plus `anim` = the sim's swing off the skinned references and
  the LocalRange gap; `SHINOBI_CLOTH_SHOTS=<dir>:<from>:<to>` screenshots at 10 fps): mantle off
  the animation median 6 deg, max ~45. Kusazuri flipped (off the animation up to 150-170 deg in
  lunges a000_003016 / 3006 / 3045 / 3037 at 4-10 m/s, its particles 0.3-0.5 m from their
  references, inside its LocalRange 0.17-0.69 m). Both have an hclCompressibleLinkConstraintSet
  (Genichiro's: mantle 1395 links, kusazuri 56). The export wrote it as an empty set and the solver
  skipped it. Exe (decomp 1414d.c FUN_1414d62e0, Mx 14151.c FUN_141519460 / 14155.c
  FUN_14155ed60): link (a, b, rest, compressionLength, stiffness), untouched between compression and
  rest length, solved back to the nearer bound outside. Now exported ("compressible") and solved
  (cloth.rs `solve_compressible`, our solve_link weighting). Duel A/B x2: kusazuri off the animation
  median 11 -> 8 deg, p90 40 -> 27, LocalRange gap median 0.30 -> 0.20 m. Mantle unchanged.
  Rare lunge flips remain (max ~160 deg; the screenshot at that moment is a clinch, nothing visibly
  wrong). Re-exported .cloth.json only (exporter check: identical apart from the new sets; old
  files in the session scratchpad reexp/orig) for 20 models with compressible cloths: c1110 c1111
  c1140 c1190 c1220 c1550 c5000 c5005 c5021 c5090 c7100 c7210 c7400 c7401 c7440 c7450 c7460
  c7490 c7520 c7530. Not done: the outside person's c1050 / c1070 / c1130 / c1191 / c1300 / c1400
  / c7000 (bosses split; re-run the model export for their cloth.json to give them the sets), c5060 / c7200 (no texbnd under
  extracted/chr to re-run). Wolf (c0000) has none, so the #2 cape numbers stand. Test
  `a_compressible_link_has_a_dead_zone`. gaps: the remaining lunge flips (solver order / substeps
  vs the game not traced); Helfur sits at its LocalRange limit (0.28 m) all the time; the
  castle-top clip is still not sampled finer than 0.5 fps.
- **Lunge flips (overseer boss item 6) closed, 2026-10-13.** The game does it too: the all-bosses
  clip at 1:06:11-1:07:49 (true Corrupted Monk, 6 fps contact sheets) shows her plate skirt and cape
  flung to ~90-120 deg (horizontal, or up beside her torso) in lunges and spins, and 20-45 deg sway in
  slow moves. Ours: median 7-11, p90 25-80, max 137-178 deg. The flips also hit c7100_mant_himo,
  which has no collidables, and the >90 deg frame count swings 5-19 between runs of the same build
  (AI noise), so they are LocalRange lag in fast moves, not collision tunnelling.
  - Exe collision RE (decomp 14157.c FUN_1415786b0 Solve, then "Collide And Solve" FUN_14157f2a0;
    capsule FUN_1415802d0, 14158.c:2326-2534):
    - The push-out matches ours: along p - closest point, to capR + particle radius; no swept test,
      no side chosen from the previous position.
    - Two differences, now ported to cloth.rs:
      - (a) Substep Collidables (14156.c:12749) walks each collidable from last frame's pose to this
        one over the substeps. `ClothInst::caps_prev`, carried with the root like the particles.
        No-op at 1 substep: all four c7100 cloths have 1, so Monk is unchanged.
      - (b) Friction: ParticleData +0xc. Prev += (rel - n (rel.n)) * friction, where rel is the
        particle's motion minus the surface's this substep. The exporter writes it as particle[5];
        it is missing (0) in older .cloth.json.
  - Re-exported only extracted/model_c7100.cloth.json (local; friction mantel 0.5, kusazuri 0.1,
    mant_himo 0.5, Helfur 0; all else identical; old file in scratchpad reexp/fr). Other models get
    friction on their next export.
  - Duel with friction: kusazuri median 9, p90 29, max 144; restarts 6 (baseline 4-7); no blowups.
  - Test `a_capsule_pushes_out_and_its_friction_takes_the_slide` (mutation-checked).
  - Pinch-contact priorities (FUN_14157f850) are not ported: they need 2+ pinch-enabled
    collidables per cloth, not checked per model.
- 2026-10-13 **Manual test #5, sword lightning in a fight** (Bosees' SHINOBI_AI_ATTACK=3043 with
  SHINOBI_FIGHT=c7110; their debug_menu fix for the env attack being reset to a000_003000 before
  the encounter loaded). 671026-29 spawn on the blade and stop after the cast. Against the
  all-bosses clip 26:00-26:20 (cast tiles 1565-1573 s): blade charge, real = a thin white-gold core
  with sparks; ours (shots scratchpad swl/shots k01381) = a smooth orange flame crescent along the
  blade. Cause: model 8180, color1 (1, 0.56, 0.25) x color3 1000. Our hue squaring (fxr.rs
  `hue_squared`) makes that (1, 0.31, 0.06) x ~1000; the game's tonemap whites it out, ours keeps
  the hue. That is the tonemap (lighting paused), not changed. Bolt (k01468): white-gold and jagged
  like the clip, but 3-4 thin parallel bolts where the clip has a dense forked web over ~70 % of
  the frame. Release (k00842): a glare streak where the clip has a starburst on the blade plus
  dark debris; the sky looks tiled there (map, paused). gap: the release starburst.
  2026-10-13 traced: the starburst is 671028 root/nodes[2]/nodes[8]. It is a 603 billboard with
  texture 1003 (a star), additive, width 5 -> 2.5 / height 2.5 -> 1.25 m, delay 0.1 s, duration
  0.267 s, rgbMultiplier 2, bloom true, bloomColor (1, 0.875, 0.75, 0.25). It IS drawn: one
  1003/4 quad in SHINOBI_FXR_LOG; shot scratchpad swl/tl/x008.png. Only its core shows, as a
  small sparkle at the hilt. Its rays (texture values ~0.3) vanish on the bright test sky.
  gap: per-particle bloom (`bloom` / `bloomColor`, reference 603.yml) is not ported for any
  appearance. That is effect bloom, not map bloom; it sits next to the paused tonemap work. Also,
  671029 draws nothing standalone: its tracers need a moving anchor (the swing).
- 2026-10-13 **Effect-light intensity scale** (item 3): FXR 609 point lights now use the sun's
  bridge. map.rs SCENE_TO_BEVY (4500 lux per game unit, now pub), matched at 1 m, gives
  4 pi x 4500 lm per diffuseMultiplier. Before it was x 4000 lm, about 14x dimmer. 671028 in
  daylight (swl/tl2) is not washed out. gap: the game's falloff curve, and a check in a dark arena.
  Lit particles' real lighting (drawn at /8) is still open: it needs the scene's light at the
  particle, which is next to the paused tone work.
- 2026-10-13 **Ember slowing: parked, with evidence.** The GPU update CS (scratchpad envsh/cs_std.asm,
  decoded) is semi-implicit Euler:
  - v += (row1 + noise x (1 - t)) dt + gravity.
  - A separate force velocity is multiplied by exp(-row7 x dt).
  - No clamp and no per-age speed multiplier. Rows 5 / 6 / 8 / 10 are not read there.
  670240 / 670271 have speed up to 15 m/s with acceleration / gravity / row 7 all 0. Their
  WindForce 10300 has f1_31 = 1 (plants only). Left: the emit shader (does the emitted speed go
  into the decaying force velocity?) and the unnamed f1_149 = 0.25 / sdt_f1_163 = 0.5. Not guessed.
  Side note: our gpu_move keeps the Min..Max random acceleration constant; the CS fades it by
  (1 - normAge). It is 0 on these embers, so not changed.
  Emit shader decoded (scratchpad envsh/emit.txt, CS_VFX_StandardParticleEmit.cpo):
  - Velocity (40) = init move vector + noise (min..max) along the emit shape.
  - ForceVelocity (152) = 0 at emit, so the row-7 decay never touches the launch speed.
  - The noise acceleration (struct 120) = NoiseAddMoveVector min..max, 0 on these embers.
  f1_149 (appearance +0x2f0) and sdt_f1_163 (+0x304; 0.5 below version 0xa4) are copied into the
  emitter (+0x44c / +0x460, 14040.c:7889 / 7894, 9898-9914) with no consumer found. They are
  not in the param texture. f1_149 sits between traceParticles and particleTraceLength, so it is
  likely a trace field.
  Still open: the Force update variants (CS_VFX_StandardParticleForce*, force.txt, with scene force
  fields in CB1 / CB2) and which variant the embers run. Not explained by the data we decode.
  **PARKED 2026-10-13 with evidence (Force variants decoded, envsh/force.txt):**
  - 8 variants, picked at 14046.c:8368-8378 (bit 1 Force = the emitter's force-field count
    `*(emitter+0x10)` != 0, bit 2 Yurayura, bit 4 Collision, bit 8 Split1;
    registration at 14045.c:599-614).
  - None damps the main Velocity: force.txt:1825-1827 is the same integration as cs_a.
  - The field loop (:498-1745) only rebuilds ForceVelocity each frame from scene fields:
    radial types 1 / 2 / 4 / 5 and directional ones, with no fields = 0.
  - No global drag default on the velocity path.
  - So no compute shader slows embers whose data has zero acceleration / drag.
  - Left candidates:
    - CPU-side scene force fields (the slowdown would then come from a field pushing
      against them);
    - a fade / size change that only looks like slowing in the clip;
    - undecoded FXR fields.
  - Not pursued further without a new source.
- 2026-10-13 **Ichimonji crescent small / dim: parked as bloom / tonemap.**
  - Data, as ported: 4 tracers on a 1.1 m arm spinning at 2000 deg/s. The white band is w 0.3 at
    rgb x2 premultiplied; the 26030 band is w <= 0.1 with bloom true. The dark 2010 / 2012 bands
    are refraction only now.
  - The clip's thick soft crescent reads as glow around the white band.
  - Per-particle bloom (bloom / bloomColor) is parked too: Bevy's Bloom has no separate input. It
    reads the HDR main image above its threshold (camera.rs / map_bloom), so a bloom-only
    contribution needs a change in the bloom pass itself.
  - The dust 440081's darkness is the same tonemap question.
- 2026-10-13 **Boss FX coverage sweep (22 main-boss rows).**
  - Ids: TAE ffx events, FootSfx, bullet sfxId_*, SpEffectVfx, EMEVD 2006 args and 2001 refs.
    Helper scripts: scratchpad bossfx/sweep.py, feat.py.
  - Result: 448 unique ids. 392 were converted; **41 existed only as f<id>.fxr in the sfx bnds
    and never played** (SpEffect visuals 5013 / 9011-29012 / 612600-2 / 615020 / 310502 /
    310520 / 652040 / 670260; bullets 650620 / 650621 / 652000 / 654080 / 654081; EMEVD
    820130 / 839531 / 839537 / 1060; Gyoubu / Ape / Demon foot 103xxx).
  - Fixed: tools/fxr_extract.py `--ids <file>` converts an extra id list. Run with the sweep's
    448 ids: extracted/fxr is now 1260 effects (+43 with refs), +3 textures.
  - 15 unresolved:
    - 11 foot base ids 102011-102142 (Monk / Owl), probably material-offset bases.
    - 650630 and 654061, which are in no archive.
    - 2255505 (c1260).
    - "2" (a SpawnFFX_110 arg, not an id).
  - Features ranked by boss effects using them (unsupported / partial):

    | Feature | Status | Effects | Bosses |
    |---|---|---|---|
    | 608 RadialBlur | partial: shape | 97 | 20 |
    | 84 ParticleAccelerationRandomTurns | partial: turn axes | 80 | 20 |
    | 732 ParticleForceSpeed | not read | 32 | 13 |
    | 607 Distortion | partial: flat shape | 14 | 15 |
    | 10300 WindForce | not applied | 12 | 19 |
    | 606 / 10012 tracers | partial: curve kind | 126 | 20 |
    | 10000 GPU drag | partial | 2 | 1 |
    | 601 Line | gradient | 1 | 1 |
    | 83 NodeAccelerationRandomTurns | axes | 1 | 2 |
    | 402 rectangle | distribution | 1 | 2 |

    Nothing else falls to `_ => {}`.
  - 732 / 10300 not ported: wind reaches effects (f1_31 = 0) only in 400 / 401, whose 732 speed
    is ~0.1, so the change is invisible.
  - The rest are partials with unknown game data (shape / axes); no source to port from.
  - Bloom / lighting flags excluded (paused): bloom 149, lit -2 216, normalMap 167, specular 50.
  - Draw check: 650620 / 654080 / 652000 / 9011 / 612600 / 820130 / 103001 draw quads standalone.
    670260 is a 609 light only.
  - The 11 foot ids are resolved as **not shipped**:
    - TAE 792 -> FUN_140b54690 -> FUN_1409da4c0 (1409d.c:5777) uses FootSfxParam sfxId[col] as
      is: no floor-material math, no remap. The legacy formula (row + 100) x 1000 + col
      (FUN_1409da4a0) matches the values, and 102xxx = row 2.
    - FUN_1409d0d40 -> FUN_1409ce420 spawns nothing when the FXR is missing.
    - No f000102xxx exists in any archive, so the game shows nothing either. Ours already skips
      them.
- 2026-10-13: the TEMP SHINOBI_TMP_ANCHOR logs are removed (fxr.rs tick / grow_trails / draw,
  model.rs). fxr tests 13/0.
  **Bolt fixed:** the bolt is 671032. Its three DynamicTracer nodes emit 2 tracers a frame (emitter
  300, interval 0; ~34 alive) from one point with particle movement 84
  ParticleAccelerationRandomTurns (maxTurnAngle 45, turnInterval 0). Our particles ignored 84's
  turns, so all 34 flew one straight path and drew as 3-4 lines. Now 84 turns each particle by up
  to maxTurnAngle every turnInterval (0: each 1/30 s frame), its own seed (fxr.rs, Particle
  `next_turn`). Shots (swl/shots3 k01504): a dense forked web from the top of the frame down to
  Wolf with arcs on the ground, like the clip. 452 nodes in 219 extracted effects use 84 with turns
  (sparks of tools / arts 300xxx / 440xxx, 404041 ...): they spread now as well. Test
  `random_turn_particles_fan_out`; fxr / effect filters pass. Left: at the peak the thick trunks
  bloom into near-white sheets (bloom / tonemap, paused).
- 2026-10-13 **GPU particles: rows 6 / 7 of the parameter texture** (FXR gap, the Demon's embers).
  Exe: FUN_14040b820 (decomp 14040.c:9100) bakes the GPU node's properties1 into 32 float4 rows per
  1/30 s slice: row 0 followFactor / p1_1 / p1_2, 1 acceleration xyz x30 + gravity (negated) in w,
  2 p1_7 / p1_8 / angularAccelerationZ x30, 3 growth rates x30, 4 colour x rgb / alpha multipliers,
  5 p1_14, 6 unkParticleAcceleration, 7 unk_ds3_p1_17, 8 randomTurnAngle (rad), 9 p1_15 (frames),
  10 p1_20. Row 7 (the shader's drag, velocity *= exp(-k dt)) = unk_ds3_p1_17: 0 on all extracted
  GPU nodes, so not ported. Row 6 is read only by the CS_VFX_StandardParticleForce* shaders, as a
  factor on the scene force fields' push (cb1 list). It is no acceleration: fxr.rs `gpu_move` added
  it to acceleration Z (+1 m/s^2 on the embers 670240 / 670271); removed. Test
  `gpu_unk_acceleration_is_no_push`. The embers' slowing in the clip is still unexplained (no drag
  in the data; maybe the force fields, which are not ported). Shader disassembly:
  scratchpad envsh/cs.asm, force.asm.
- 2026-10-13 **Lasting SpEffects' look: the Finger Whistle madness** (Bosees' gap "vfx 7900 / 7901
  are VFX's"). status.rs `lasting_glow`: while a row is on `Actor::sp_chained` (a SpEffect cycle,
  or the row a TAE SpEffect hands on to), its SpEffectVfx plays (`spawn_vfx`: init once, midst
  held) and the midst stops when the row goes. Rows the current anim still carries (tae_prev) are
  left to ffx.rs `tae_sp_vfx`, so nothing is shown twice. Madness 230515 -> 7900, midst 300205 on
  dummy 7; minor 230516 -> 7901, 304205. Rows missing from combat data now read vfxId / vfxId1
  from SpEffectParam.json (`param_vfx`). Test `a_maddened_beast_shows_the_whistle_madness`
  (mutation: without the system it fails); whistle / madden / burn / poison / tae_sp filters
  green. gap: not seen in play (needs a beast and the whistle); prosthetic.rs:516 still says
  "gap" (Bosees' file, not edited).
- 2026-10-13 **Resurrection heartbeat** (Bosees' gap, 4.42): a000_110020 / 21 put on 110091
  (Player::held, endurance -1, until 110093). 110091 fires 110092 every motionInterval 2.5 s.
  110092's SpEffectVfx 5010 has no effect, only midstSeId 200001, which is event c000200001 (the
  only main-bank key with that number; the 'c' type is taken from that). status.rs `heartbeat`
  reads Player::held and queues that sound once per cycle (SoundQueue); `param_cycle` / `vfx_se`
  read SpEffectParam / SpEffectVfxParam. Test
  `the_resurrection_heartbeat_beats_every_two_and_a_half_seconds` (3 beats in 6 s, none after);
  resurrect / status filters green. A midst SE lasts while its SpEffect does: with FMOD each beat
  is started tracked and stopped after 110092's effectEndurance 2.4 s (the event runs ~3.4 s, so
  beats no longer overlap) or when 110091 goes; without FMOD it is queued whole. The four lazy
  SpEffectParam reads in status.rs are one parsed table (`param_row`). gap: not heard in play.
- 2026-10-13 **"Enemy turn" glow 23061x: not a gap.** Residents on ~1000 NpcParam rows: 230610 /
  230611 / 230612 (orange S / M / L, SpEffectVfx midst 9011 / 19011 / 29011) and 230615 / 230616 /
  230617 (gold, 9012 / 19012 / 29012). Each is gated: invocationConditionsStateChange1 955 /
  970, the stateInfo of 230620 / 230625 "enemy turn VFX playable". Nothing applies those: no TAE
  event, no common / map EMEVD (little-endian search of extracted/event), no AI script, no
  decomp constant. The 9011 / 9012 effects are not in the extracted sfx either. So the game
  never shows them (the [NTC] rows), and Sv1 shows nothing, which is right (residents are not
  on sp_chained, so `lasting_glow` leaves them alone).
- 2026-10-13 **Boss effect items (overseer's list after the bosses came back):**
  - (1) Divine Dragon arena, magenta rectangle on the horizon: traced 2026-10-11 (kb/map.md "Far
    view"). It is no missing texture: the horizon band (the env probe's purple haze) shows
    through a gap between two far mountain pieces; the game hides it with its fog / sky colour.
    That is sky / fog work, paused with map tone and lighting.
  - (2) Demon of Hatred embers: they draw (SHINOBI_FXR_TEST=670240 SHINOBI_FXR_DIST=16: the
    burst's ember dots fly out, shots scratchpad demon/fx). In a drill they never play:
    SHINOBI_FIGHT=c7020 SHINOBI_AI_ATTACK=3043 leaves him walking at 3.2 m (body collision),
    so 3043 never starts. Sent to Bosees (their drill / AI).
    - Closed 2026-10-13 after Bosees' drill-range fix (4.42). SHINOBI_FIGHT=c7020 with
      SHINOBI_FXR_LIVE spawns:
      - 3043: 670240, live up to 6.8 s.
      - 3044: 670240 + 670241 / 670242 / 670275.
      - 3047: 670271 + 670241 / 670242 / 670276.
    - Standalone, SHINOBI_FXR_TEST=670271 SHINOBI_FXR_DIST=16 draws its spark band and dot / streak
      scatter like 670240, plus a slower orange fireball (sheet scratchpad demon/fx271sheet.png).
    - In-fight stills of 3047 (SHINOBI_CLOTH_SHOTS) were not usable: the camera faced walls or a
      knocked-down Wolf. Only faint streaks were seen.
    - gap: one still at t 3.8 s showed a band of tiled rectangles on the arena horizon (far LOD /
      sky). Logged for Maps, not chased (map work paused).
  - (3) Isshin's 654061 (c5400 / c5430 a000_003036, SpawnOneShotFFX 1.5-1.7 s, dummy 300): in no
    sfx bundle of the game (the dictionary's 8: commoneffects + 7 map bundles, all unpacked; no
    f000654061 in any BND file table, and 654060 is missing too). So the game spawns nothing
    there. What plays in that window: 654062 (0-2 s), 654066 / 654068 (from 1.57 s) on the
    same dummy 300 and 411 on dummy 8, all drawn.
  - (4) TAE 110 (Corrupted Monk c5000 / c5005: 32 per kind, value 0 or 2 at 0.0-0.1 s of the
    damage anims a000_012000 / 012310 / 013200 / 013410 / 013510): exe FUN_140b56900
    (140b5.c:3367, dispatcher case 0x6e) -> FUN_1409da880 builds {slot = value, ffx = -1,
    dummy = -1} -> FUN_1409d0d00 on chr +0x1b40 (the ChrMultiSfxSlot list): it clears slot N's
    effect, spawns nothing, reads no param. The Monk's slot 0 / 2 effects (TAE 96 SlotID 0 / 2:
    220605 / 255605 on dummy 803; TAE 118 slot 0: 696020 / 696021) are, in ffx.rs, already
    ended by their own event / the anim change, so the damage anim never inherits them: the
    behaviour matches with no code. gap: the game's slots outlive an anim change until
    replaced or cleared (that is why 110 exists); ffx.rs ends a slotted effect with its anim.

- 2026-10-13 **Arts and tools vs the all-skills clip (user asked: compare every skill and tool).**
  - Method:
    - Ours: SHINOBI_SKILL=<id> SHINOBI_SKILL_SHOTS, range 3,120,4, SHINOBI_SKILL_CAM=60,10, 6x5 sheets.
    - Game: one 30-cell sheet per chapter of sekiro_all_skills (brightened).
    - Each pair judged by screenshot-judge. Sheets in scratchpad skills/ (ours_<id>.jpg, clip_<chapter>.jpg).
  - Result: major gaps in all 16 arts with footage.

    | Art (id) | Main miss |
    |---|---|
    | Whirlwind 5100, Ichimonji 5300, Ichimonji Double 7100, Floating Passage 5600, Spiral Cloud 7200 | white crescent trails missing or hairline |
    | Dragon Flash 5400 | arc small and far off, near the horizon; no light pillars |
    | Ashina Cross 5500 | one dull arc ~0.8 s; the game flashes crossing blades for 1-2 frames |
    | Nightjar 5200 / Reversal 7000 | orange ember storm missing |
    | Praying Strikes 5900 | white bursts and dust missing |
    | High Monk 7400, Senpou Kicks 5800 | arcs missing; landing dust is a big black cloud (should be pale snow) |
    | Shadowrush 6000 | streak brown, not blue-white |
    | One Mind 6100 | gold ribbons and shockwave missing |
    | Mortal Draw 5700 / Empowered 7300 | huge red crescent and ink missing: a small red trail at the feet |
    | Sakura Dance 7700 (no clip) | arcs drawn, but huge and misplaced: screen edge, a band over the HUD |

  - Tools: Firecracker 71000 and Flame Vent 72000 minor gaps (smoke too heavy and long; no flame core,
    glow or embers). The rest major gaps.

    | Tool (id) | Main miss |
    |---|---|
    | Loaded Shuriken 70000 | did not fire at all in the run |
    | Spear 78000 | sparks and streaks missing |
    | Axe 73000 | arc missing |
    | Mist Raven 74000 | feather ball sparse |
    | Finger Whistle 79000 | wind rings missing; an orange burst instead |
    | Umbrella 76000 | fire missing; canopy cream, not dark |
    | Divine Abduction 77000 | swirls missing |
    | Sabimaru 75000 (no clip) | no poison trail |

  - Measured cause (temp SHINOBI_TMP_ANCHOR log in fxr.rs `tick`, to remove):
    - Mortal Draw's 12200 is at hip height (y 0.3-0.5) with its +Z pointing down (z.y -0.7) all
      through the release a106_316100. Model2 is on its rest place 149 there; TAE 715 holds it on body
      dummy 20 only in 316000 f6-107. So 440003's node offsets (0.5-1.5 m along +Z, orientation 1 =
      local Z) lay the ribbon into the ground.
    - Ichimonji's 440080 sits fixed at 1.5 m with z = -1, 440081 on the ground (follow off). The
      game's big white crescents are probably the weapon's own swing trail, not these FXRs: what draws
      them in the game is not traced yet.
  - Fixed 2026-10-13 (same day):
    - (1) Ordering: fxr.rs `tick` ran in PostUpdate with no order against model.rs
      `override_weapon_location` (TAE 715), so effects read the weapon parts at their rest dummies.
      Mortal Draw's 12200 was Model2 on the hip (149), pointing down. Now `tick` runs after the new
      `model::WeaponLocation` set.
    - (2) Wolf's weapon parts had no dummy-direction sidecar: only c0000 / c1020 / c1010 had one;
      the tools already had theirs. Every weapon dummy took its part's axes instead of its own +Z
      along the blade, which tracers (orientation 1) and node offsets use.
      - tools/extract.ps1 now writes model_c0000_{wp_a_0300, sheath_a_0300, mortal_a_0300,
        mblade_a_0310, mbsheath_a_0310}.dummies.json (written locally too).
      - model.rs part dummies take the full frame (fwd + up, as `dummy_frame`), not fwd only.
    - Result:
      - Mortal Draw's arc now rises with the blade, and the black ink strokes show.
      - Whirlwind's arc is longer and bolder.
      - Still far smaller and fainter than the game.
      - Ichimonji unchanged: its arc 440080 on body dummy 615 (0.8 m ahead, model forward -Z) is
        placed right; size and brightness are the gap.
    - Tests dummy / throw / mortal / fxr pass.
  - StateInfo gates checked (correctly skipped):
    - Ichimonji's 404000 / 404010 / 404020 are the blood / poison / white-blood enchantment variants
      (SpEffect 106040 / 106050 / 106060, stateInfo 153 / 152 / 357).
    - Mortal Draw's wide 440007 / 440005 are the empowered "+1 with emblems" (140610, stateInfo 993).
  - TEMP debug still in: SHINOBI_TMP_ANCHOR logs in fxr.rs `tick` and model.rs
    `override_weapon_location`. Remove when the trail work is done.
  - Next: why our tracers draw thin and dim. Per-art arc effects vs the clip stills; check the
    tracer's width axis (orientation 1: node local Z) and the blend / colour path for 10012.
  - Fixed (same day): tracer orientation 1 (LocalZ) is the ribbon's facing, not its width; width
    = travel x Z (fxr.rs draw). Before it was radial, i.e. inside the swing plane, so a vertical cut
    seen from behind was edge-on. The trails were never degenerate: only the first window is a0 == a1.
    Result: Whirlwind 5100 now has a wide band, but it is pale, flat and has hard quad corners.
    Ichimonji 5300 still shows no crescent in the skill run (standalone 440080 does draw), so
    check its spawn/window in the skill. Mortal Draw is still a small lingering wisp off the sword.
    The TEMP logs are also in grow_trails and the 606|10012 draw loop.
  - Fixed (same day): the fxr tick now substeps (<= 1/60 s, max 8) and lays trail points on each
    substep. grow_trails moved out of the draw into the tick. One long frame (a hitch or a
    screenshot) had turned 440080's 2000 deg/s spin into one chord. fxr tests 13/0.
    Sheets (h4_*):
    - Ichimonji: a crescent now shows, but about 10 % of the game's size and dim; the dust covers it.
    - Whirlwind: early frames are still a flat plank, then a long thin arc.
    - Skill 5400 (its HUD reads Dragon Flash, not Mortal Draw; check the id map): a large bright arc.
  - Fixed (same day): dark tracers with a normalMap and distortion (color1 starting at <= 0.13
    grey; 274 of the 335 distorting tracers) now draw as refraction only. The `peak()` helper reads a
    curve's largest keyframe, because num() was 0 for curves. Whirlwind's flat planks are gone.
    Ichimonji's black 0.6 m band no longer covers the crescent.
    Still open:
    - Ichimonji's crescent is small and dim next to the clip, and its dust 440081 is far darker
      and bigger than the clip's.
    - Whirlwind is shorter than the clip.
    - The colour/width curves are read at ParticleAge (reference 10012.yml). 440080's keys end
      at 0.667 = segmentDuration, which hints at segment age (not changed).
    - The width has `scale: instanceSize` in the reference, and our instance size is not applied
      (check the TAE/ffx spawn scale).

### 4.45 Bug hunt round 23 (2026-10-13)

Round 23 finders on multi-enemy fights, Wolf's death / resurrection, and anim playback / root
motion.

- **Throw auto-capture aimed at a dead enemy's body** ("bug fixes", fixed): prosthetic.rs
  spawn_bullets' locked / captured targets now skip dead enemies (hp 0 or Mode::Dead), as melee
  auto-homing does; right after a kill (lock dropped) the throw bent toward the body.
- **Resurrecting on the death loop's first two ticks kept poison / burn** ("bug fixes", fixed):
  the 8002-8009 cure is read by apply_refresh_effects after decide, once the loop has advanced.
  player.rs opens the choice at prev_t > 0 and queues an earlier press (Player.revive_queued).
  Test mashing_attack_through_death_still_cures_the_poison.
- **Grouped enemies idled in the sword stance** ("bug fixes", fixed): anim.rs proc_clip used
  a000_400000 / a000_008550 for every enemy; now Actor::grouped (spear a200, matchlock a300,
  eight phases a100 ...), falling back to a000 without a clip. Test
  a_grouped_enemy_idles_in_its_own_stance.
- **Enemy bullets hit Wolf's corpse** (Bosees, fixed, enemy_bullet.rs): poison / burn build-up and timed
  SpEffects after the death loop's cure; treat hp <= 0 as dodged. See 4.42.
- **Debug drill timer drained once per enemy** (Bosees, fixed, enemy.rs drill): N enemies fire every
  interval / N. See 4.42.
- Refuted / out of scope: hit stop vs Wolf's airborne gravity (only boss rows stop Wolf); clipless
  TAE ends; t=0 strict edges (boss / unused anims only); TeamHelp stubs (known gap).

### 4.46 Bug hunt round 24 (2026-10-13)

Round 24 finders on combat arts, deathblows, and config / startup / data loading.

- **Base Mortal Draw / Praying Strikes held into the upgraded moveset** ("bug fixes", fixed):
  the HKS BEH_A_GROUND_SP_ATTACK_RELEASE also forces the release at ref 222 when the art lacks
  its unlock (106 without ref 283, 108 without 285); player.rs only released on letting go.
  Test base_mortal_draw_is_released_even_when_held (5700 releases, 7300 holds).
- **Ichimonji's posture recovery never applied** ("bug fixes", fixed): 140210
  (changeStaminaRate 12.5, one-shot) comes by TAE 67 at the slash; apply_refresh_effects read
  only 401. Test ichimonji_recovers_posture.
- **posture.player_level ignored after F5** ("bug fixes", fixed): the R reset recomputes max HP /
  posture / regen (player.rs level_stats, graphs 500 / 501 / 504); it also clears revive_queued.
- **Encounter with missing / unreadable enemies/<chr>.json became an empty "c1020"** ("bug
  fixes", fixed): data.rs load_encounter skips such kinds and keeps the current fight when the
  main enemy fails (the startup fallback needs combat_data's General).
- **OPEN (not changed, exe not traced): the deathblow start commits without re-checking.**
  player.rs DeathblowStart (judge / end -> main_deathblow, or the 201 vault's on_posture_break)
  does not re-check that the enemy is still broken / unaware; the start ThrowParam rows
  (11020000 / 11020110) hold nothing on the defender, and Mode::Broken keeps counting. A press in
  the last ~0.35 s of the 352 window kills a recovered enemy. Whether the exe re-checks at the
  main row's judge is not traced: left as is.

Enemy AI adaptivity (user, 2026-10-13: "i dont want the real enemy ai that learn from you";
then, told it is Sekiro's own: "put the ai learning back"): mapped, kept as Sekiro has it.
Source rule from the same exchange: every new value / behaviour must come from Sekiro's own
files / exe / scripts; nothing new from sekiro-rs, videos or other sources unless the user OKs
that item (merged ports stay unless asked). Only Sekiro's own short windows exist: ConsecutiveGuardCount (1 s, enemy.rs
guard_count -> Common_Parry) and the firecracker 107100 cool time (30 s, prosthetic.rs
burst_until). c1180 118040_battle.lua counts the player's guards / hits, but Sv1 raises neither
INTERUPT_TargetIsGuard nor Goal.Damaged, so it stays at 0.

### 4.47 Bug hunt round 25 (2026-10-13)

Three finders: Wolf's defence numbers, enemy hit reactions, and patrols/hearing/home. Sources are
Sekiro's own params, c9997.lua, common_table_ai_common.lua and the exe decompile only.

- **SpEffect damage rates on Wolf** (combat.rs `npc_hit_damage`). SpEffectParam
  `<attr>DamageCutRate` / `def<Attr>DmgRate` / `<elem>DamageCutRate` are now exported
  (export.rs SP_EFFECT_FIELDS; data.rs `DamageCut`, flattened into `SpEffect`). An NPC hit on Wolf
  is multiplied by them over his TAE and resident SpEffects. 109400 (break anims) gives x1.5.
  gap: the exe site of this multiply is not traced.
- **Elemental chip through a guard / deflect** (combat.rs `guard_chip`). Uses EquipParamWeapon 5000
  `<elem>GuardCutRate` / `<elem>JustGuardCutRate`. Fire 33/50, magic 25/50, dark 50/100 and
  lightning 0/0 % are absorbed, so a torch bandit's fire chips HP through a block.
- **Sword reactions respect the transition rank** (combat.rs `sword_reaction_allowed`). c9997
  ExecDamageDefault needs rank 4, LargeBlow/AerialBlow rank 0, and Blow has no check. A slash
  no longer cuts FireReaction (3), DamageWeak/Lightning (3) or DamageLargeBlow (0); the hit
  falls to the additive flinch.
- **Hearing keeps a higher-rank sound** (stealth.rs `hear`). Exe 14062.c:4230-4257: a footstep
  (rank 0) no longer overwrites a held guard-break sound 5000 (rank 1).
- **INTERUPT_ChangeSoundTarget (46)** (ai.rs `interrupt_logic`). A new sound id raises it through
  InterruptTableLogic → COMMON_Interrupt → InterruptTableLogic_Common, the exe FUN_1405d3fd0
  order. Enemies with 241000 then play ListenSound 700/710. gaps:
  - Only 46 goes through the logic chain; the other interrupts still reach the root goal only.
  - "new" means a new AiSoundParam id; the exe compares the sound serial.
- **Level-0 hits do not push** (combat.rs Outcome::Hit `still`). An enemy whose SpEffects make
  the hit's motion level 0 (straw doll c5040, 5300) stays put, and the attacker gets the whole
  scaled distance as recoil (exe FUN_140b6bfe0: cut 1.0, push start skipped). There is no test
  yet (it needs a c5040 encounter). Unconfirmed: whether flag 24 should also give full recoil;
  left as is.
- Tests:
  - a_broken_wolf_takes_half_again_the_damage
  - a_guarded_fire_hit_chips_wolf
  - a_slash_keeps_the_protected_reactions
  - a_footstep_keeps_a_held_important_sound
  - a_new_sound_plays_the_listen_reaction

### 4.48 Bug hunt round 26 (2026-10-13)

Two finders: prosthetic tools and items, and Wolf's movement and defence. Sekiro sources only.

- **Build-up-only bullets land** (prosthetic.rs hit_enemy). The Lazulite Sabimaru mist (Bullet
  750300-750302, spEffectId0 9009 poison 31; AtkParam 7503150 all 0) was dropped as "deals
  nothing" and never poisoned. Test: the_poison_mist_poisons.
- **Air throws fire their TAE 4 bullets and pay** (prosthetic.rs `crossing`; export.rs tool
  groups take TAE 4 as bullet judges; combat_data re-exported, backup
  $TEMP/combat_data.backup3.json). a07x_403000/403050 hold only TAE 4 BulletBehavior_Midair, so
  the air move played but threw and cost nothing. gap: the exe TAE 4 handler is not traced.
  Test: an_air_throw_fires_and_pays.
- **Gourd Repeat only in the ref-400 window** (player.rs). c0000_transition.lua 3986-4008 needs
  SP_EF_REF_TAE_ENABLE_ITEM_REPEAT (100315: a000_250000 1.1-1.3 s, 250001 0.7-0.9 s); a later
  press restarts a full drink. Test: a_late_second_drink_is_a_fresh_drink.
- **Sliding sprint deflect only with ref 1** (player.rs). c0000_transition.lua 4443-4458: out of
  SprintAttack_L/R, quick turns or SprintStopReady it is StandToDeflectGuard. Before, any
  "Sprint*" state name gave the 0.4 s / 3 m sliding deflect.
  Test: only_a_running_sprint_gives_the_sliding_deflect.
- **The listen reaction survives its own replan** (ai.rs native_terminate). ConfirmCautionTarget's
  terminate (FUN_1405c3770) clears the caution targets only when no new sound (46) came that
  frame. The port cleared them anyway, so the reaction's plan swap wiped the sound, the same id
  was "new" next frame, and the script's Replanning dropped ListenSound 700. Found by Bosees'
  sim test a_cautious_enemy_listens_to_a_new_sound (now enabled). Bosees also added
  a_level_0_hit_leaves_the_doll_still_and_wolf_recoils for round 25's `still`.
- Test adjusted: posture_break_staggers_then_a_second_hit_knocks_wolf_down gives Wolf 2000 HP.
  Round 25's x1.5 broken damage made the second hit lethal at level 1 (128 + 192).
- **Sprint jump** (player.rs; export_states.txt + re-export). HKS 2784-2789: with ref 1 up a jump
  is SprintJumpReady a000_201200 → SprintJumpStart a000_201210, or straight Start under ref 107
  (100399, LandForwardGroundJump frames 0-6). 201210's TAE 920 row 121 launches it at 9.15 m/s
  forward / 9.64 up (plain forward jump: row 101, 6.4 m/s). It lands as a ground jump
  (LandForwardGroundJump, HKS 2057-2062). The states were missing from the whitelist
  (statemap_c0000.json has both). gap: no sprint-jump fall CMSG, so 201210's last frame holds
  until touchdown. Test: a_sprint_jump_is_the_fast_long_jump.
- Not claimed (finder): long-stiff landing at fall height ≥ 20 m (clip not exported); landing
  with dodge held (_fireGroundStep).

### 4.49 Bug hunt round 27 (2026-10-13)

Two finders: enemy grabs and Wolf's death, and status effects on Wolf. Sekiro sources only.

- **Revival wait loop** (player.rs death block). HKS c0000_transition.lua 328-333: with a
  resurrection left, the death start ends in W_GroundRevivalStayLoop (a000_110020), else
  W_GroundDeathLoop. The stay loop wears 110010 "Pretending Dead" (sightSearchEnemyCut 100;
  COMMON_HiPrioritySetup makes the enemy clear its targets). The loop clips are one frame long
  and loop, so their time now wraps and their f0-4 TAE stays on. The revive choice / timeout
  runs on Player.death_wait. gap: env(3028) read as "no resurrection left". Tests updated
  (resurrection, poison cure, corpse bullets).
- Sim test a_feigning_wolf_is_dropped_by_the_enemy (Bosees, fixed): the flicker was the test
  app's passive practice partner (no AI script) once in BATTLE; the test makes it aggressive, no
  longer ignored (4.42).
- **Death in a grab** (player.rs; export_states.txt + re-export of a210_600001, a223_600141 /
  600351 / 600371). Exe traces:
  - env 206 = throw state != 0 (140b3.c:10664 → 140be.c:328), so BEH_R_DEATH waits out the
    throw.
  - The damage that empties HP in a throw sets the defender's throw state 6
    (140b6.c:8217-8238 → FUN_1407fa400).
  - env 276 = own ChrActionFlag 69 pulse and state 6 (140b3.c:2523).
  So Wolf plays the grab clip on to its flag 69 (a210_600000 frame 70), then
  W_ThrowDefDeathStart = the clip + 1, which ends into the death / revival-wait loop.
  Test: a_lethal_grab_plays_its_throw_death.
- OPEN: grab escape (BEH_R_THROW_ESCAPE, env 277 = the thrower's flag-70 pulse and own throw
  state 3/4, 140b3.c:2669). c1500's 4110 has flag 70 at frame 105, and 4120 has it every 15
  frames from 15 to 150. So the pulse is a check point; what it checks (escHp from
  FUN_140be15c0 via message FUN_1407fab00, and button mashing?) is not traced. Not implemented,
  to avoid guessing. Clips: ThrowEscape600350/600370 = a223_600352 / 600372.
- `duel_trace` now ignores time while Wolf is down. With the revival wait's 110010 the General
  idles (IdleBattle) for over 6 s, which is correct and not a stall.
- Full suite: 369 passed, 0 failed.
- Not done (feature, not a bug): Wolf has no poison/burn cure item. The pellets (goods 3200 /
  3210 / 3300 / 3310 → SpEffect 3201 / 3211 / 3301 / 3311, -99999 build-up) are Sekiro's, but Sv1
  has never had items beyond the gourd. Left for the user to decide.
- Status on Wolf otherwise checked and correct: resist 200, decay 5/s, the break threshold,
  guard / deflect resist, tick formulas, disable flags.

### 4.50 Bug hunt round 28 (2026-10-13)

Two finders: Wolf's combat arts, and enemy posture / guard. Bosees also traced the grab escape.

- **Grab escape** (player.rs `GrabEscape`; export of a223_600352 / 600372). Bosees' exe trace,
  also in kb/enemy.md "Grab escape":
  - The ThrowParam escHp meter (throw state +0x1a) loses 1 per selfEscCycleTime window (read as
    ms) with ≥ selfEscCycleCnt presses: FUN_140808640 → FUN_140802c30 → FUN_140bdf220(-1).
  - At 0, the grabber's next TAE flag-70 pulse with Wolf in throw state 3/4 (env 277,
    140b3.c:2669) plays W_ThrowEscape<id> = the grab clip + 2.
  - c1500 rows: 21500100 escHp 20, 21500200 escHp 10; 21500000 escHp 0 has no escape.
  - gaps: which buttons count (any press here; the stick shake is not modelled), the
    damage-on-victim path (FUN_140b68ff0), and the grabber's own anim on the escape
    (Bosees / enemy.rs).
  - Test: mashing_breaks_the_zombie_restraint.
- **Art contact windows deal nothing** (data.rs attack_windows). A TAE 307 PCBehavior judge now
  resolves only to BehaviorParam_PC's pc<judge>. Before, the art's variation row of the same
  number won: the air Mortal Draw a106_316200's 16 contact windows hit as v5007:210 (~200 HP +
  38 posture each). Smaller extra hits came from the air Senpou kicks a107, Praying Strikes a108
  and Shadowrush a109. Test: an_air_arts_contact_windows_deal_nothing.
- **staminaRecoverChangeSpeed** (actor.rs regen_posture; data.rs SpEffect). The regen is
  rate × (base + change) (kb/enemy.md, FUN_140a04850) over the resident and TAE-active
  SpEffects. The TAE ones were not read before, and the change term never: c1010's battle idle
  300240 is +40/s, c1220's guard-break clips 300260 +60/s.
  Test: the_battle_idles_change_speed_adds_to_posture_regen.
- Sent to Bosees (combat.rs):
  - An enemy's block / deflect skips its NpcParam <attr>StaminaDmgRate (exe FUN_1408439a0
    applies it to every outcome; c1180 thrust 1.35, c1250 lightHit 1.25).
  - Unguarded-hit posture is not floored (exe returns int).
- OPEN (low, inferred): a sprint Nightjar Slash picks F/N/B from the stick, but HKS
  BEH_A_GROUND_SP_ATTACK's sprint branch sets Selector_GroundJumpType VERTICAL (no
  _set2DirStepDir). Which clip VERTICAL is (316300?) is only inferred from naming.
- Refuted by the finders:
  - Art attackBase: all 19 art rows equal Kusabimaru's.
  - Costs, the hold / release / combo chain, and Dragon Flash's payment all match.
  - Enemy regen: no HP-ratio curve, no regen timer.

### 4.51 Bug hunt round 29 (2026-10-13)

Two finders: Wolf's hit reactions, and enemy projectiles against Wolf. Every finding is in
Bosees' files (combat.rs, enemy_bullet.rs), so all were forwarded; no code changed here.

- Sent to Bosees (combat.rs):
  - D1 (high): Wolf ignores his own super-armour SpEffects. The Player arm uses the raw
    level, not damage_motion_level, so getting up, the posture-break pose, grappling and some
    tools can be re-staggered.
  - D2 (medium): an umbrella guard break plays the sword clip. It should play
    W_StandSubGuardDeflectBreak (a070_405400) or W_AirSubGuardDeflectBreak (a070_421400).
    player.rs landing / recovery for those is mine if needed.
- Sent to Bosees (enemy_bullet.rs `fly`):
  - D1 (medium): a bullet with no damage and no SpEffects skips the contact test. Its
    HitBulletID child spawns where it lands, not on Wolf. Examples: c1100 bullet 20 (Gecko
    poison spit → 11000021 / 9011) and c1360 bullets 130-132 (→ 13600136 / 9014).
  - D2 (medium): dmgHitRecordLifeTime is never read, so penetrating pools hit once (c1030
    10300240: fire + burn, life 5, record 0.5). The re-hit meaning is inferred from values.
  - D3 (low-medium): hitRadiusMax / spreadTime are ignored (c1250 bullet 100: 0.2 to 1.2 over
    0.8). The meaning comes from param names only.
- OPEN (low, unverified): the side Wolf lands on from a posture-break small blow (env 222).
- Done (Bosees, 4.42): c1070 bullet 380's child 10700383 had no exported row (exporter bug,
  fixed and re-exported). Done (Bosees, 4.42): predictionShootObserveTime, see "Predictive
  shots".
- Refuted: guns can be deflected (musket rows disableGuard 0 / disableJustGuard 0); bullet
  posture costs use the shared melee paths; damp decay matches FUN_140b142e0. The Terror
  SpEffects (stateInfo 5, behaviorRefId 590) are a mechanic gap with no exe evidence.
- Follow-up (Bosees): the R29 bullet items D1-D3 are done, with tests. The umbrella guard break
  now plays the a076 clips via sub_anim (the a070 keys of the state table have no anim).
- Mine (player.rs): AirSubGuardDeflectBreak holds its pose in the air. AirDeflectGuardBreak and
  AirSubGuardDeflectBreak land into their Land versions from the same time only while ref 201
  is up (SpEffect 100302, 0-2.0 s; HKS 1862-1869); later, a plain landing. Before,
  AirDeflectGuardBreak always landed into it from t 0. The a076_405020 umbrella loop has
  JUST_GUARD (107650) on every frame, so an open umbrella deflects every guardable hit.
  Test: an_air_guard_break_lands_into_its_land_version_while_ref_201_is_up.

### 4.52 Bug hunt round 30 (2026-10-13)

Two finders: prosthetic tools against regular enemies, and Wolf's healing / items / revival.

- **Firecracker cool time** (prosthetic.rs hit_enemy). It was a flat 30 s (the bullet's 107100)
  for everyone. Now it is the enemy's own SP_EFFECT_REF_NO_BURST_REACTION (1000044) row, which
  its a000_020110 adds by TAE. The row is gated by invocationConditionsStateChange1 976 (107100)
  and kept past its window by `add_reaction_cool_time`: c1070 251070 15 s, c1130 14 s, c5080
  20 s. The General, soldiers, dogs and monkeys have none. `Enemy.burst_until` is unused now.
  Tests: firecracker_staggers_the_general_every_time,
  an_enemys_own_burst_cool_time_blocks_the_next_firecracker.
- **Firecracker posture**. The burst returned before any posture. The beasts' burst AtkParam
  7100104 (directAtkStamDamage 30) times NpcParam attriAStaminaDmgRate 3.0 now breaks a dog
  (15 posture). The break plays TrunkCollapseBurst1-3 (c9997 ExecDamageBreak; on_posture_break
  keeps it). Test: a_firecracker_breaks_a_dogs_posture.
- **Flame Vent DamageFire**. A spAttribute 2 hit on an enemy with ref 1000040 (NpcParam 6020)
  and without 1000240 plays W_DamageFire (rank 3). The clip's own 1000240 row is kept as the
  cool time; the FireReaction clip's row is kept too. An igniting hit still plays FireReaction
  first (ExecDebuffReaction runs before ExecSpReactionLarge). HANDOFF 8's "correct as is"
  checked only the General's default row (no 6020; his 10209800 has it).
  Test: the_flame_vent_sets_a_zombie_flailing.
- **Regen stop after a hit** (actor.rs `run_sp_effect_chains`, every actor). TAE SpEffects now
  outlive their window: endurance > 0 for that long, endurance 0 as their replaceSpEffectId
  after the window, and timed ones chain on expiry. Wolf's reaction clips' 109450 becomes
  109451 (rate 0, 1 s), so posture regen stays off a second after the window.
  `Actor.sp_chained` is also what Bosees' revival-chain work reads.
  Test: a_hit_stops_wolfs_posture_regen_a_second_past_its_window.
- **Todome recovery hold** (player.rs). A fired row that outranks a held one of its category
  (lower categoryPriority) drops it. Only 14 deathblow clips (a211-a235 groups, e.g.
  a225_510090) fire the release rows 105052 / 1503x3; the ordinary ones do not, so the General's
  deathblow stays blocked after a Todome. The finder's "every deathblow" was wrong.
  Tests: todome_blocks_the_deathblow_recoveries, a_release_row_lifts_the_todome_recovery_block.
- **Empty repeat press**. ItemGourdDrinkRepeatFailed / Crouch… (a000_250006 / 250016, 1.83 s) are
  now exported and played for an empty press inside the ref-400 window (HKS 4024-4033).
  Test: an_empty_repeat_press_plays_the_repeat_failed_clip.
- **Spear pull** (actor.rs). Actor::knockback with a negative distance pulls (v7800 judges
  115-117: -8 / -6 / -4 "Spear_pull"), and limit_knockback caps the travel. Bosees wires
  combat.rs apply_knockback / recoil. Test: a_negative_knockback_pulls_and_stops_at_its_cap.
- Sent to Bosees:
  - Timed SpEffects in Enemy.clash are not read by def_stamina_rate / armor_rates
    (Firecracker LV3-B 9800: x1.125 for 2 s).
  - The revival chain is invisible to enemies (110031 → 110032 → 110033).
  - The empty-gourd shake set off INTERUPT_UseItem (done: wolf_starts_an_item excludes Failed).
  - The sword path's SP_DAMAGE_FIRE gap.
- OPEN (low):
  - (closed, Bosees 4.42) isCheckWall_byCenterRay is not an area-hit wall test. The paramdef
    says it is the launch-position wall-embed check done with a ray between the characters'
    centres: error handling for SEQ23101, where a soul arrow shot point-blank at a tall target
    flipped. 1529 rows have it. We do not model the launch-embed check at all, so it changes
    nothing here.
  - DONE (Bosees, 4.42): the Finger Whistle's cycleOccurrenceSpEffectId chain (230518 → 230510
    → 230515 CONFUSE).
  - DONE (round 35; re-checked in round 45): a Wolf bullet blocked by an enemy gets the guard
    reaction (guard_pick), the clash SpEffects that drive the guard count (add_clash), and the
    deflect's posture floor (max - 1).
  - Resurrection recast (RevivalRecastPoint; SpEffect revivalRecastDamage) is not modelled.
  - DONE: the _B revival (round 33), the throw-death revival (round 37) and the Land revival
    (round 40). The crouch death / revival is out of scope (g_forceCrouch only).
- Refuted: "the Shuriken's 113000 regen debuff" (Bullets 5060xxxx are c5060 Owl's, a boss row,
  not Wolf's); Gourd numbers and windows, posture regen HP factors, status recovery, the revival
  cure, emblem costs, the Axe shield break, and the guard rules of the Firecracker / Flame Vent
  (disableGuard 1).

### 4.53 Bug hunt round 31 (2026-10-13)

Two finders: Wolf's answers to perilous attacks, and Wolf's movement.

- **I-frames go by AtkParam trackType** (sent to Bosees: combat.rs `dodged`, the Mikiri gate).
  The exe's hit test FUN_140b68a50 (140b6.c:4234-4241) skips the hit when the defender's TAE
  954 IFrameType bit of AtkParam +0x193 trackType is set (FUN_140b29cb0 sets it).
  - trackType values: 1 sweeps, 2 general, 6 thrusts, 7 grabs.
  - Wolf's i-frame types: GroundStep_F types 7 and 2; side and back steps 6, 7 and 2; the jump
    start and AirKick 1 only.
  - So a step does not dodge a sweep, the forward step does not dodge a thrust, and a jump
    dodges only sweeps. dodged() used stand-ins instead (Some(2) = everything, atkType, the
    980-warning anim, throwFlag).
  - Done here: the exporter exports trackType, and data.rs has `Attack.track_type`. combat_data
    and every extracted/enemies chr are re-exported (enemy rows: 2 ×72, 6 ×17, 1 ×5, 7 ×2 in
    c1010).
  - Also sent: the Mikiri gate (atkType 2) misses the zombies' bamboo-spear thrust 15000400-402
    (atkType 0, trackType 6); the 危 kanji is shown for any enemy, where the game uses the 2 m,
    0.1 s Bullet 980/982/983 → SpEffect 211000 on Wolf.
- **Free-fall landing with the stick held** (player.rs land). FreeFall and the plain-landing
  fallbacks (hit-jump arts, air tools, the art fall loops) now go straight into moving with the
  stick held (HKS 2026-2056 W_StandMoveStartFromFreeFallShortStiff). Only without the stick do
  they land stiff. Test: a_free_fall_with_the_stick_held_runs_on.
- **An unlocked step ignores the auto-aim homing** (player.rs step). Only lock.target takes the
  4-direction step (HKS _fireGroundStep: env(1118) IsLockedOn). A dodge out of an unlocked swing
  is the free step along the stick. Test: an_unlocked_step_ignores_the_auto_aim_target.
- **_execDamageStep** (player.rs, before the jump / step). While ref 502 is up (100344 on
  StandDamageLarge_F/B, 0.5-2.0 s), a step or jump plays StandStepFromDamage_F/B (a000_206100 /
  206101). While ref 504 is up (100346, 53 anims: get-ups, blow landings, breaks, revival) it
  plays StandStepFromDown_<FaceUp|FaceDown>_<N|F|B|L|R> (206110-206123) by the stick angle from
  the facing (_set4DirJumpDir ±45 / 135). The 12 states were added to export_states.txt.
  - gap: _set4DirJumpTilt is not applied, and the prone side is read from the state name.
  - Test: a_step_out_of_a_large_hit_is_the_damage_step.
- **Held dodge while running sprints** (player.rs). HKS BEH_A_SPRINT: dodge held with the run
  loop's ref 91 (100001) up and no ref 427 queues the step, which goes on into the sprint.
  Test: holding_dodge_then_running_sprints.
- OPEN (low):
  - Fall height is not tracked (FALL_HEIGHT 3 / 5 / 20 m: LongStiff a000_200021, not exported).
  - (done, seen by Bosees 4.42) Walk / run hysteresis: player.rs `stick_runs` (0.9 up / 0.7
    down, c0000_define 205-206). Still unverified: the MoveSpeedLevel scale and whether the exe
    rescales the stick.
  - Wall jump (BEH_A_WALL_JUMP, ref 104): no WallJump states are exported.
- Refuted: grapple range and gating, the locked 4-dir step and tilt, the jump sectors,
  converge_speed, the quick turn, the crouch toggle, the kanji timing, the deflect / block rules
  of perilous attacks, the Mikiri posture and Wolf's flag 125, the 940-942 dummies, and the
  kick-jump condition.

### 4.54 Performance check, build speed, scope, bug hunt round 32 (2026-10-13)

- **Scope (user):** Sekiro's combat and its regular enemies, playable like the game, plus what
  they need (music, HUD, effects, move sets). No whole-game rewrite. Stated at the top of PLAN.md.
- **src/perf.rs (new):**
  - F3 opens an overlay with fps, 1 % low, frame ms (average / p99 / worst), the main world's
    ms per phase and the entity count.
  - `SHINOBI_BENCH=<s>` (default 20) records after a 5 s warm-up, writes `perf_report.txt` next
    to config.toml and quits.
  - Baseline (gate m11_01_00_00 + c7400, release, vsync on): 54.9 fps, 18.2 ms frame, of which
    ~11.5 ms is render wait, so the game is render-bound.
  - VFX's port of the 73eca1d bindless batching (OK'd by the user) did not move the render wait
    (4.44). Open: a chrome-trace run (`--features bevy/trace_chrome`, in target/bugfix) on a
    quiet machine with vsync off, to read the render-world spans.
- **model/tool.rs test:** uses `World::try_schedule_scope(FixedUpdate, ..)`. This fixed Bosees'
  repro failure.
- **Build speed (user OK):**
  - .cargo/config.toml sets the rust-lld linker and the sccache wrapper (it caches dependency
    builds across the target folders).
  - Cargo.toml has `[profile.quick]`: release plus incremental, for tests only, not for benches.
  - One target folder per session: target/bugfix, target/bosees, target/map. The extractor
    builds in tools/sekiro-extract/target. target/release and target/dist are kept (play build,
    package). The 14 other folders were deleted on the user's request (39 GB freed).
  - Per change, run only the matching tests (`cargo test --profile quick --bin sv1 <filter>`).
    Bug fixes runs the full suite once per round and forwards failures to their owners.
- **Walk / run hysteresis (player.rs `stick_runs`):** c0000_transition `_SpeedUpdate` switches
  walk to run above 0.9 and run to walk below 0.7 (c0000_define 205-206). The old single 0.75
  cut (PRM_RUN_STICK_LEVEL) has no reader in the HKS. The state is updated every frame, so a
  released stick drops back to walk. Test walk_and_run_switch_with_hysteresis.
- **Long-stiff landing (player.rs `long_fall`, `Player::air_top`):** BEH_R_LAND 1908, fall
  height (env 224) >= FALL_HEIGHT_LONG_STIFF_LAND 20 m: LandFreeFall with a000_200021
  (Selector_LandStiff LONG, 2.5 s), stick or not.
  - It comes after the guard-break / special-attack / blow lands and before the air combo,
    electro and tool lands.
  - The clip was added to export_states.txt and combat_data.json was re-exported (backup
    $TEMP/combat_data.backup6.json).
  - gap: env(224)'s measure in the exe (here: from the flight's highest point). The 3 / 5 m
    short-stiff lands differ only by the sprint / step out (`_fireGroundStep`); not done.
  - Test a_twenty_metre_fall_lands_long_stiff.
- Bosees (this round): perilous_kind from the first damaging AtkParam (the 980-983 judges are not
  per kind); the 危 kanji is FXR 600035 on dummy 220 (SpEffect 211000 → SpEffectVfx 48100),
  waiting on VFX to convert it; the deathblow-start OPEN is closed (the game does not re-check
  either).
- Suite: 406 passed, 0 failed, 40 ignored (quick profile).
- **Round 32, bug fixes (tests pass):**
  - The walking gourd drink: HKS 3993-4009 W_ItemGourdDrinkMove / RepeatMove (standing, legs
    moving). These are STATE_TYPE_UPPER_ACTION_ITEM_USE with CMSG animId 0, so the standing
    drink plays over the walk (`is_upper_action`). The crouched branch has no Move. The standing
    clip used to slide along. gap: StartTime_02 (the legs' loop time). Test
    drinking_while_walking_is_the_moving_drink.
  - The gourd repeat window skips the Failed shakes (HKS 3986 / 3993 list only the drinks).
  - sound.rs `floor_key`: a floor ('x') sound is the id rounded to 100 plus the material. The
    banks have c000006500-6502, c1550's ...3101 / 4101 / 4201 and c1370's ...6501 next to the
    ...x001 ones; rounded to 1000 they were silent or the plain step. gap: the exe's key builder.
    Test floor_sounds_round_to_the_hundred.
- **Round 32, forwarded to Bosees (combat.rs):**
  - B1: a guarded hit with no usable guard clip falls through to damage (c9997 2252-2288,
    ExecGuardBlock FALSE). Affects c1360/1361/7400/7401/7510 (no GuardDamageLarge), the shield
    chrs without guard clips, and the guardLevel-0 c1250/1260.
  - B2: a level-0 (super armour) hit doesn't flinch additively (c9997 2901: only minimum,
    aerial, or a rank rejection).
  - B3: superArmorDurability is non-zero on many regular rows (the comment said 0). The meter
    rules are not traced.
- **Round 33, bug fixes (tests pass):**
  - Wolf's death side: BEH_R_DEATH 974-983 picks DEATH_TYPE_COMMON_BACK when killed from behind.
    New Actor `hit_from_back` / `hit_back_quadrant` are set in combat.rs's player hit branch
    (Bosees notified). GroundDeathStart_B, the _B loops and GroundRevival_B now play; it was
    always _F. gap: the blow / upper / fling deaths (918-951) have no exported clips, and bullet
    hits don't set the side. Test a_death_from_behind_falls_face_down.
  - A break small blow lands face down only from the 4-way back sector (HKS 1678-1684). Before,
    any _B landed face down. Test a_break_small_blow_from_the_side_lands_face_up.
- **Round 33, forwarded to Bosees (ai.rs stubs + enemy.rs Snapshot):** IsLockOnTarget (always
  false), SpaceCheck / GetExistMeshOnLineDist* (always the asked distance), CheckDoesExistPath
  (always true) and GetTeamOrder role (always Attack: no Kankyaku / Torimaki surrounders).
- **Profiling (chrome trace + quick-profile benches, "arenas" config, c7400):**
  - 87.2 fps, 11.46 ms frame, p99 15.4, render wait 6.83, post-update 2.97.
  - Render thread: Render schedule 7.3 ms self, RenderExtractApp 5.8, allocate_and_free_meshes
    3.7, write_binned_instance_buffers ~1.5, queue_submit 1.2.
  - Cloth: the simulation is ~1.2 ms; uploading 25 display meshes every frame costs ~1.7 ms
    (SHINOBI_CLOTH_NO_UPLOAD=1: 102.5 fps, 9.76 ms). The meshes carry NoFrustumCulling, and in a
    fight both chrs are on screen, so an off-screen skip would not help fights; not done.
  - map::apply_giiv 0.48 ms every frame was sent to VFX (does it need to run each frame?).
  - Profile the same way again: build with `--features bevy/trace_chrome` in a temporary
    target dir (the feature rebuilds Bevy), run `SHINOBI_BENCH=10`, then aggregate trace-*.json
    by span (B/E pairs per tid, self = inclusive - children).
- **Round 34:**
  - Wolf's sword chains match the HKS: the ground combo-ref order, step / crouch / sprint /
    deflect attacks, release attacks with their buffer resets, air combo 1-2-3-2, AirKick into
    Air2, and the 1/6 s auto-aim.
  - HUD items forwarded to VFX (hud.rs): the 4-state posture bar layers (FUN_140a02a10, 30 %),
    deathblow pips (ninsatuNum), the 8-gauge cap and SpMinimumDispTime 0.5. All medium confidence
    or lower.
- **Round 35:**
  - Fixed: a guarding enemy takes Wolf's shuriken like a sword blow (prosthetic.rs hit_enemy).
    The posture goes through enemy_guard_posture_rate x timed_rates, floored. The reaction comes
    from enemy_guard_block / guard_pick (made pub(crate)), or GuardBreakLeft / Right on a break.
    The clashes add_clash 200210 / 211 (deflect) and 200215 / 216 (block). Before, a deflect did
    nothing and a block added the raw repel with no reaction. gap: the clash ids on a bullet are
    inferred from the melee path. Test a_blocked_shuriken_plays_the_guard_reaction.
  - Forwarded to Bosees: the Sabimaru SpecialPoisonReaction (a000_020160) and the poison death
    (a000_010370) for the WOMAN-ref c1310 (c9997 894-895, 2969-2976, 2043-2045).
  - Matching: the deflect window frames (105010), the spam rates 105020-22, the guard chain and
    release gates, Mikiri (flag 125 on step N / F), step i-frames by trackType, the AirKick sweep
    i-frames, the kick-off (ref 204), guard chip rates, emblem costs, the Spear armour break, the
    Axe guard break and Divine Abduction's 107710.
  - Bosees closed R33 A1 / A2 / A4 (lock-on, mesh line distance, team roles traced from
    FUN_140640050). A3 stays a gap.
  - Suite: 417 passed, 0 failed, 40 ignored.
- **Round 36:**
  - Fixed: the deathblow start's height window. player.rs `throw_height_ok`: ThrowParam
    upperYRange / lowerYRange, 1.26 m on c1020 0020 / 0110, as the plunge and deflect-pose
    starts check it. gap: diffAngMyToDef. Test a_stealth_deathblow_keeps_to_its_height_window.
  - Fixed: terror (stateInfo 5, registIllness, the exe's resist type 1 "disease"):
    - status.rs has a third gauge: Desease recovery 2.5 / s, Wolf's resist 200, the guard keeps
      diseaseGuardResist 25 / JustGuard 100 off.
    - A full gauge applies the row's changeHpRate 100 once (effectEndurance 0), which kills.
    - data.rs SpEffect.regist_illness; the extractor exports registIllness(ChangeRate); c1500 was
      re-exported (backup $TEMP/c1500.backup.json).
    - gap: the terror death anim and the HUD gauge.
    - Test the_zombies_dumplings_build_up_terror (the 3rd dumpling hit kills).
  - Forwarded to Bosees:
    - The attack AI sounds (AtkParam Appear / HitAiSoundId: Wolf's 2 / 4 / 8 m, NPC 10 / 20 m,
      rank 1) are never emitted.
    - Ref 289 (SpEffect 100235 on the air arts) should let a blue-lightning hit pass (HKS 1099-1103,
      1157).
    - Exe question: double burn build-up on 16 bullets with the same SpEffect on Bullet and
      AtkParam.
  - Bosees: R35's special_poison_reaction kept as a shared rule (c1310's clips only); a debuff
    reaction is no longer overwritten by guard_pick on the same hit. The NoDead question
    (ninsatuNum-0 regular enemies die at 0 HP per the SetHP / IsNoDead trace) was answered: go
    ahead carefully, record the unknown bit as a gap.
  - Scope change (via VFX, PLAN.md top): the 22 main bosses are ours again. Bosees takes their
    AI and moves, VFX their cloth / VFX / SFX, bug fixes the player side (deathblows, Todome,
    throws).
- **Round 37 (player side of the main bosses):**
  - Fixed: the deathblow start lost `behind`. `Player.throw_start` carries it to main_deathblow,
    so a behind / Todome / HU row no longer turns the defender round to face Wolf.
    deathblow_from_behind_uses_the_back_throw now checks the defender's facing (mutation-checked).
  - Fixed: the break-pose attack takes its Todome row (main + 8) on the last bar, when it
    exists and its anim is exported (Gyoubu's 15080019 a225_510190).
    - gap: Gyoubu's ThrowDef12100 has no ThrowStart2 (67) in his TAE, so in the sim this path never
      finds him. His last bar takes the broken-deathblow Todome 15080009 (a225_510090) instead.
    - How the exe reaches 0019 is not traced.
    - Test gyoubus_last_bar_broken_deathblow_is_the_todome.
  - Fixed: a grab's death loops are the throw's own (c0000_transition 285 / 301 / 354-356,
    Selector_ThrowId).
    - The CMSGs a000_<ThrowId>_DeathStart / _DeathLoop / _RevivalStayLoop / _Revival are
      offsetType 17, the grab's group (ThrowParam defAnimOffset of the DefChrId 0 row).
    - The extractor now resolves them, picking the group whose clip exists when a ThrowId has
      several. 116 of 120 resolved. 600300 has no grab row; 600180's revival, 600340's loops,
      600390's start and 600400's stay loop are not in the game's files.
    - Wolf plays ThrowDefRevivalStayLoop / ThrowDefDeathLoop (e.g. a210_610002 / 610001) and gets
      up with GroundRevival_Throw (the loop id's xx3).
    - menu.rs's death screen also takes ThrowDefDeathLoop.
    - Tests: a_lethal_grab_plays_its_throw_death (updated),
      a_final_grab_death_loops_in_the_throws_own_death_loop.
  - Fixed: grab blow-away ends (HKS 386-399).
    - 600030 / 600040 -> ThrowDefLargeBlowStart (a000_199201), loop 199211, land
      StandDamageLargeBlowLand_B.
    - The Guardian Ape's 600380 -> 199231 / 199241 / 199251 -> StandDamageLargeDown_FaceDown.
    - At HP 0 the Ape takes the Death chain 199261 / 199271 / 199281, which ends in the _B
      death / revival loops.
    - player.rs: throw_blow / grab_end_state / grab_end / throw_blow_step. combat.rs's grab end
      calls player::grab_end; Bosees has been told.
    - Test grabs_with_a_blow_away_end_throw_wolf_down (mutation-checked).
  - Extraction: export_states.txt gains the 120 grab-death CMSGs and the 8 blow-away states.
    combat_data.json was re-exported (additive only; backup $TEMP/combat_data.backup7.json, the
    anim_c0000*.bin backups in $TEMP/animbak).
  - gap: Emma's 600400 (no RevivalStayLoop clip; the death loop and revival exist).
- **Round 37 full suite:** 431 passed, 0 failed (includes Bosees' 0-HP death rule).
- **Round 38:**
  - Fixed: the short-stiff landing (BEH_R_LAND 2026). From 5 m, or 3 m out of a positioning jump
    (p.jump_land), a jump's fall skips its own landings. With the stick held it is the moving
    start; otherwise LandFreeFall (a000_200020). Before, a jump off a ledge landed as
    LandVertical / ForwardGroundJump or the positioning land from any height.
    Test a_jump_off_a_five_metre_ledge_lands_stiff (mutation-checked).
  - Fixed: the terror and poison deaths (BEH_R_DEATH 962-971).
    - status.rs sets Actor.death_ref to the killing row's behaviorRefId: 590 for the terror rows
      9204-9223, 500 for a poison tick.
    - Wolf then plays GroundDeathStartInsane (a000_191600) or GroundDeathStartPoison (191400),
      then the _B loops (IndexDeath B).
    - export_states.txt gains GroundDeathStartInsane / Fire / Poison.
    - the_zombies_dumplings_build_up_terror now also checks the death anim.
    - gap: GroundDeathStartFire (191200 exported). It needs the killing hit's element (env 285) or
      ref 514, and Sekiro's burn rows carry 1000130, not 514.
    - The poison death has no test of its own; it shares the terror path.
  - Checked:
    - The 4.53 item "a Wolf bullet blocked by an enemy skips the guard count" is already done:
      prosthetic.rs adds clash 200215 / 216, which counts.
    - The crouch death / revival (W_CrouchDeathStart) is g_forceCrouch only, the forced crouch
      in crawl spaces. Out of the arena scope.
    - Sprint Nightjar VERTICAL: the c0000.hkx CMSGs are SprintSpecialAttackStep_N (animId 0,
      empty), _F 316301 and _B 316302. If the selector is bound to Selector_GroundJumpType, VERTICAL
      (0) is the empty _N. The binding (hkbVariableBindingSet 10811) is not decoded; kept as is.
    - The resurrection recast, traced statically (re-scout):
      - The point is a float at PlayerGameData+0x150 (debug name "Revive Recast Point").
      - A completed resurrection adds the max (DAT_143b010a0 = 100.0; FUN_140a36a60,
        140a3.c:3400).
      - FUN_140a34b70 allows a new resurrection only when the point is exactly 0.0, plus a
        revival count above 0.
      - Lowered only by active SpEffects' revivalRecastDamage (row offset 0x418): FUN_140bfdce0
        sums them, then FUN_140a30b50 -> FUN_140a23730 subtracts, clamped to [0, max]. Rows
        100 / 3292 / 8100 set 99999. The idol rest also zeroes the point (140a2.c:9956).
      - The per-second decay (+0x2130 RevivalRecastGain) is 0 everywhere.
      - Not found: what applies 8100 in a fight. It is not in our TAE exports, any param field,
        HKS, ai_src or a hard-coded id, and no kill / deathblow code touches +0x150.
      - Not modelled: a lock without its in-fight clear would make a second resurrection
        impossible in a fight, stricter than the game might be. gap (the clear's source).
    - diffAngMyToDef / DiffAngMin / Max: a re-scout trace of the exe's throw check is running.
  - Round 38 full suite: 433 passed, 0 failed.
- **Round 39:**
  - Fixed: blinding (BEH_R_DAMAGE 1104 / 1155 / 1876).
    - Damage level none with AtkParam atkBehaviorId 17 (env 334 BLINDING) plays
      StandDamageBlinding (a000_192600). In the air it is AirDamageBlinding (192610), which
      lands as LandAirDamageBlinding (192611) while ref 201 is up (100302, 0-1.2 s).
    - These rows are all bullets: the Rappa's powder 13600170 / 171, the Taro troop's mud
      11804500, the Yasha apes' sand 12500100 / 12505100 / 12505120, c1070's 10700365 and
      c1210's 12100100.
    - The extractor now exports atkBehaviorId. data.rs has Attack.atk_behavior_id. Re-exported:
      combat_data and enemies c1070 / c1180 / c1210 / c1250 / c1360 (backups
      $TEMP/combat_data.backup8.json, $TEMP/enemybak8).
    - combat.rs (Bosees' file) has the branch before the level-0 "move goes on" one.
    - Test the_apes_sand_blinds_wolf (mutation-checked).
  - Checked (WRONG, corrected in round 43): SP_EF_REF_ENABLE_ELECTRIC_SHOCK(_WEAK) 592 / 593
    are on 9451 / 9450, which are reached only through the cycleOccurrence chain 949x -> 941x ->
    943x. Terror's non-lethal W_StandDamageInsane needs a terror
    row under 100 %; all are 100.
  - Fixed: the Divine Dragon's atkBehaviorId bullets (BEH_R_SPECIAL_DAMAGE 1005-1012).
    - combat.rs special_damage_state (it also does 17): 18 SXL_BLAST (the lightning 52000820)
      plays AirDamageElectroReceiveExplosionStart, then FallLoop and Land.
    - 19 STORM_BACK_JUMP (the strong wind 52000830) plays GroundStormJumpBackReady, then Start,
      fall and land; AirStormJumpBackStart in the air.
    - Both rows deal no damage. enemy_bullet.rs `harmful` (Bosees' file) now also passes an
      attack with an atkBehaviorId.
    - c5200 re-exported (backup $TEMP/enemybak8).
    - gap: BEH_R_SPECIAL_DAMAGE's env 206 / death-type conditions.
    - Test the_divine_dragons_blast_and_wind_throw_wolf (the whole chains).
  - Round 39 full suite: 438 passed, 0 failed.
- **Round 40:**
  - Fixed: the fire death (BEH_R_DEATH 962). In combat.rs's direct hit on Wolf, a kill by
    spAttribute 2 (env 285 DAMAGE_ELEMENT_FIRE) at level NONE / SMALL / MIDDLE / LARGE / PUSH /
    MINIMUM sets Actor.death_ref = 514 (status::REF_FIRE_ADD_DAMAGE_BURN). player.rs then plays
    GroundDeathStartFire (a000_191200) and the _B loops.
    - gap: a burn tick's element. Burn rows carry ref 1000130, not 514.
    - Test a_killing_fire_hit_is_the_fire_death (c1030 10300200; mutation-checked).
  - Fixed: the killing-hit deaths (BEH_R_DEATH 785-951). Before, every kill played
    GroundDeathStart.
    - combat.rs keeps the killing hit's (level, element) in Actor.death_hit.
    - player.rs `death_starts` picks in the HKS order, on the ground:
      - lightning: GroundDeathStartElectricShock (Weak for blue lightning);
      - EXLARGE: StandDamageDeathLargeBlowStart;
      - EX_BLAST: ExLargeBlowStart;
      - BREATH: SpecialLargeBlowStart;
      - SMALL_BLOW: SmallBlow_F/B, by the 4-way angle;
      - UPPER: LargeUpperStart;
      - FLING: LargePound.
    - In the air:
      - FallDeathElectroCharge(Weak)Start;
      - the AirDamageDeath* starts;
      - NONE: FallDeathDmgStartNaturalDeath;
      - otherwise FallDeathDmgStart F/B.
    - `death_chain_step` plays Start, then FallLoop while airborne, then Land.
    - `death_done` (HKS 275/328 and 277/334) sends the ground set to the Ground loops. The
      blows end in IndexDeath B. LandFallDeath* goes to LandDeathLoop / LandRevivalStayLoop, and
      revival from those is LandRevival_F/B.
    - menu.rs: LandDeathLoop also triggers the death screen.
    - 58 states were exported.
    - gap: the game's anibnd has no clips for the AirDamageDeath LargeBlow / ExLargeBlow /
      LargePound / LargeUpper chains or for FallDeathDmgStart / Loop F/B (a000_1210xx-1217xx).
      The air picks fall back to the Stand clip or to NaturalDeath's fall.
    - Inferred (graph-internal): SpecialLargeBlow falls and lands as ExLargeBlow.
    - Test a_killing_hit_picks_the_death_by_its_level covers EXLARGE, SMALL_BLOW from behind,
      lightning, and an air kill with LandRevival (mutation-checked).
- **Round 41:**
  - Fixed: the deathblow reach now follows the exe's throw range check FUN_140be0270 (re-scout
    trace).
    - New `throw_facing_ok`:
      - diffAngMyToDef: the angle between Wolf's forward and the 3D direction to the enemy must
        be <= the field (35 on the General's rows, 22.5 to 90 on others);
      - DiffAngMin/Max: |enemy yaw - Wolf yaw|, inclusive (0 means both face the same way).
    - The front/back start is now picked by that yaw difference, as the rows' ranges are.
    - Before, any facing took the deathblow.
    - The `_start` columns are read only by a debug drawer and are 0 on all 2414 rows.
    - Range is 3D (it already was).
    - deathblow_check takes Wolf's forward. hud.rs gets a one-line edit (VFX notified).
    - The finisher (throwKind 250000) followed in round 42.
    - gap: whether the grab event re-checks the body row is untraced.
    - Test a_deathblow_needs_wolf_to_face_the_enemy (mutation-checked).
  - R40/R41 suite: 442 passed, 0 failed (40 ignored).
- **Round 42:**
  - Fixed: the finisher's facing now uses `throw_facing_ok` too.
    - Isshin's 15400590 (DiffAng 0-45, MyToDef 35) is taken from behind him, facing his way.
      Re-scout: his kneel a100_020200 has a root yaw of 0 throughout, and his absorb dummy 230
      sits at his back. The c5100 and c1012 finisher dummies sit in front, with 90-180 ranges.
    - Before, it was taken in front within 45 degrees of his facing.
    - The unused Throw.diff_ang_max was removed.
    - Test isshins_finisher_is_taken_from_behind (mutation-checked).
  - Checked, no change:
    - Wall jump (BEH_A_WALL_JUMP, HKS 2841-2853 / 440-514): the exe's wall-contact detection
      triggers it. It is traversal, out of scope.
    - The burn-death ref 514 rows 4011 / 4021 are the castle and samurai-residence fire floors
      (4010 / 4020 cycle), which are map hazards, out of scope. A burn tick's element (env 285)
      stays a gap.
    - BEH_R_SPECIAL_DAMAGE 5612: its guards (not dead, not in a throw death, ref 513
      TAE_NO_DAMAGE_ANIM) are already met. A kill takes the death first. The only ref 513 row,
      100289 ("PC: damage animation transition prohibited"), is in no exported Wolf clip.
  - Round 42 suite: 443 passed, 0 failed.
- **Round 43:**
  - Fixed: a lightning hit on Wolf on the ground now shocks him. Before, it was a plain hit.
  - How the exe gates the chain (re-scout trace of FUN_140c060d0, 140c0.c:4832):
    - invocationConditionsStateChange1-3 ask whether the owner holds an active SpEffect whose
      stateInfo (0x15E) equals the value. They are not ground or air tests.
    - 901 is 9480 "shock resistance permission". It is resident on Wolf's protectors 100000 to
      600000, so it is always present.
    - 356 / 151 are the charges' own stateInfo.
    - So the hit's charge enchantment 949x fires 941x, then the damage row 943x, then
      9450 / 9451 (ref 593 / 592), all at once.
  - combat.rs (Bosees notified):
    - A grounded hit's 949x goes to Actor.shock.
    - The air catch now keeps the hit's own 949x.
  - player.rs `lightning_shock`:
    - follows the exported chain;
    - takes HP of changeHpPoint + changeHpRate % (for example 9432: 160 + 30 %);
    - adds posture from the negative stamina fields;
    - plays W_GroundDamageElectricShock(Weak) (a000_192900 / 192920, exported now);
    - a kill becomes the ElectricShock death.
  - The landing shock uses the same chain. Before, it was a fixed 160 + 10 / 20 %.
  - Export: SpEffectParam rows 949x / 941x / 943x / 9450 / 9451 (LIGHTNING_SHOCK_SP) and the two
    states. Round 39's "592 / 593 unused" was corrected above.
  - Test lightning_on_the_ground_shocks_wolf (c7110 71100361; mutation-checked). The air-catch
    test now accepts any weak charge row.
  - gap: whether a guarded or deflected hit's SpEffects land. Here they do, as with the poison
    build-up.
  - gap: 958 / 959 (the whistle's madness gates, stateInfo of 9080 / 230500) are the same
    mechanism. The whistle's CONFUSE (changeTeamType 29) is enemy AI work (Bosees).
  - Round 43 suite: 446 passed, 0 failed (with Bosees' aging gauge in).
- **Round 44:** Wolf's aged form. Bosees did the curse gauge to 9620 (ref 591) in 4.42; before
  this round Wolf showed nothing of it.
  - status.rs `set_aging`:
    - Actor.aged follows ref 591 in Statuses.
    - 9620's maxHpRate 0.125 scales hp_max (HP clamped, restored on the end).
    - The row is worn in `resident`, so npc_hit_damage takes its DamageCutRate 99 (x99).
    - 9620 is now in Wolf's exported spEffects.
    - The reset takes it off.
  - player.rs `aging_step`, run before the normal inputs:
    - Onset (BEH_R_AGING_DAMAGE 1043): StandAging, or FreeFallAging / AgingFreeFallStart in the
      air. Not during a throw or deathblow.
    - The aged standby (AgingIdle / AgingStandMove_Walk_F / sprint while the dodge is held)
      takes attack -> AgingGroundAttack (a050_390000) and guard / step / tool / grapple / art ->
      AgingGroundAction (226010). Its stops are AgingStandWalkStop / AgingSprintEnd, moved by root
      motion and turned by TAE SetTurnSpeed.
    - The aged standby states buffer every input (`buffers`).
    - Falling: AgingFreeFallLoop, then LandAgingFreeFall.
    - Cure (BEH_R_CURE_AGING): ref gone plus an input -> StandIdle.
  - Death:
    - On the ground: AgingGroundDeathStart (BEH_R_DEATH 970, after the blows / fire / terror /
      poison).
    - In the air: AgingFallDeathStart, then Loop, then LandAgingFallDeath (819 / 1778).
    - Both end in AgingGroundDeathLoop (HKS 365), with no resurrection offered.
    - menu.rs shows the death screen.
  - Export: the 25 aged states (AgingStandMove_Walk_F and AgingGroundJumpReady/Start_F/_V are the
    graph's names).
  - gap (the jump closed in round 46): the quick turns, the aged
    item use, 9620's changeStaminaRate -100 / defNinsatsuDmgRate, and
    THROWABLE_STATE_ATK_PC_AGING (the noble's grabs 11300260-281, enemy side).
  - Test an_aged_wolf_plays_the_aged_form (c1300's 9600 build-up; mutation-checked).
  - Round 44 suite: 449 passed, 0 failed.
- **Round 45:**
  - Checked: the resurrection recast clear (8100 / 3292) is not in EMEVD. The binary hits in
    common_func / common / m11 are instruction-table offsets, not event args (decoded with
    boss_events.emevd_events). The gap stays.
  - Stale open items closed: the bullet block's guard reaction / guard count / posture floor
    (round 35), and the revival variants.
  - Fixed: an aged Wolf's deathblow. Before, the aged attack ate it. Bosees read the rows:
    11300260-281 are Wolf -> c1300 only.
    - deathblow_check takes `aged` (THROWABLE_STATE_ATK_PC_AGING 10, c0000.lua 699). While aged
      only the aging rows count:
      - unaware and behind: 0270 / 0271 (a222_502100 -> 512100);
      - broken and behind: 0280 / 0281 (502200 -> 512200);
      - broken in front: 0260 / 0261 (502000 -> 512000).
      - No finisher, near, Todome or second-form rows.
    - aging_step steps aside when one is in reach.
    - `accepts` / `buffers` take the aged standby (is_aged_standby).
    - Player.aged_form brings the form back as AgingIdle after a throw or reaction (HKS 381),
      not W_StandAging again.
    - hud.rs passes wa.aged (VFX's file, one argument).
    - gap: the exe's map from throwable state 10 / 11 to ThrowKind categories 17-19 (by the
      rows' names).
    - Test an_aged_wolf_takes_the_aging_deathblow (mutation-checked).
  - Round 45 suite: 450 passed, 0 failed.
- **Round 46:**
  - Fixed: the aged jump. Before, a jump press while aged did nothing.
    - BEH_A_AGING_GROUND_JUMP (HKS 4707-4718): W_AgingGroundJumpReady_F with the stick (it
      turns Wolf to it), _V without, then its Start (a000_226110 / 226111).
    - aging_step applies the Start's TAE 920 launch by the main tick's rule (it now takes
      `combat`), then AgingGroundJumpFallLoop, and lands as W_LandAgingGroundJump (1776). The
      free fall still lands as LandAgingFreeFall.
    - Also fixed: the aged fall moved the body by vel_y twice (aging_step and actor.rs).
    - Test an_aged_wolf_jumps (two mutations: no jump branch, the free-fall landing).
  - Closed: the aged quick turns. c0000_transition.hks has none (its aged behaviours are 700-702,
    710, 720, 721 and 152), and MOVE_STOP 701 (sprint released with the stick held -> walk, else
    SprintEnd / WalkStop) already matches.
  - Out of scope: the aged item use. BEH_A_AGING_ITEM_USE 152 (4392-4417) fires only
    W_AgingItemReturnStatue, the Homeward Idol's travel; every other item is refused.
  - Round 46 suite: 452 passed, 0 failed (with Bosees' shura enchant test).
  - Checked: guarded lightning on the ground.
    - The HKS settles the open Loaded Umbrella. BEH_R_HIT_DAMAGE's validate
      (c0000_transition.hks 5636) takes a DAMAGE_TYPE_GUARD lightning hit in a SUB_GUARD state as
      a hit, and the guard reaction (5500) leaves lightning out for SUB_GUARD. Its reaction is
      the shock picked by the 9450 / 9451 refs (1014-1041), so the hit's 949x must reach Wolf
      through the umbrella.
    - Ours matches: combat.rs sets `shock` on every grounded outcome, and player.rs plays it from
      any state.
    - gap: for the sword guard / deflect the HKS runs the guard reaction (no lightning test on
      the ground). Whether the exe still applies the 949x there is not traced; ours shocks.
  - Checked: 9620's other fields. defNinsatsuDmgRate and the def*DmgRate fields are 99, as
    the cut rates are. replaceSpEffectId 9641 "release, time elapsed" is effectEndurance 0.1 with
    a VFX only.
    - Fixed: changeStaminaRate -100. The exe (scout trace) re-fires a timed row with
      motionInterval 0 every tick (FUN_140c06b70 / 140c06c60). FUN_140bfe2c0 sums the rate,
      and 140a0.c:5119 adds it to the remaining posture without dt.
      - status.rs set_aging now drains the aged Wolf's posture each frame (our full bar).
      - The an_aged_wolf_plays_the_aged_form assert is mutation-checked.
      - gap: the player's clamp floor (FUN_140bd4e20) is taken as empty, with no break
        reaction.
      - gap: the other timed stamina rows (3062/3071/3076 food regen, 110127 deathblow regen,
        interval 0.1-1 s) are not on our path. 110127 is not in the export.
  - Manual test #1, the infinite Spirit Emblems cheat:
    - F1 PLAYER "infinite Spirit Emblems" (debug_menu.rs player_cheats) tops them up every
      tick to max(config count, 15). Also set by config [player] infinite_emblems (serde
      default false).
    - Test the_infinite_emblems_cheat_refills_them (mutation-checked).
  - Round 46 suite after the RESUME (with Bosees' super-armour): 454 passed, 1 failed. The
    failure is emma_and_the_castle_genichiro_are_defeated: c7400 never sets 9315, looping
    TrunkCollapseFront -> ThrowDef12000. It is sent to the overseer for Bosees.
- **Round 47** (debug_menu.rs / sound.rs bug hunt):
  - Fixed: the enemy actions (heal, break posture, deathblow now / behind, plunge / kick / vault,
    both stealth set-ups) act on one enemy: the nearest that is not dead, not disabled and not a
    parked phantom.
    - Before, they acted on every enemy entity: a group stacked on one spot, the stealth items
      revived dead members, and the Monk's phantoms or the off-field Genichiro / Tomoe were
      warped in front of Wolf.
  - Fixed: god mode drops the killing hit's death pick (death_ref / death_hit). Before, a later
    real death played the spent one (a fire death for a plain hit).
  - Fixed: the outfit sync falls back to the kind's default NpcParam row, as `pending` does.
    Before, with no npc_row the menu showed outfit 0 with "Enter: restart" pending.
  - Fixed: the drill attack is reset when the kind (Fight picker) no longer has its anim.
  - Doc: sound.rs's floor comment now rounds to 100, as floor_key does.
  - Not changed: restart_with sets the in-memory config before the spawn. On failure it says
    "start the game again", and the written config.toml already names the new enemy.
  - No new tests (the menu is key-driven); checked by build and the full suite.
  - Round 47 suite: 456 passed, 0 failed, 41 ignored.
- **Round 48** (world.rs / duel.rs bug hunt; duel = the SHINOBI_DUEL reach check):
  - Fixed: the per-window `gaps` were measured but never written. The output file now ends
    with "anim judge: gap".
  - Fixed: the gap uses every hit shape (AtkParam hit0..hit15) and combat.rs's dmy_entity.
    Before it used hit0 only, with ids >= 10000 always taken as id % 10000 (Model2 dummies
    such as 12220 were missed). dmy_entity is now pub(crate) (Bosees' combat.rs, visibility
    only).
  - Fixed: every enemy is sampled and listed. Before, single_mut() stopped all samples once a
    second enemy existed (group, Tomoe swap, Monk phantoms).
  - Not changed:
    - The duel's Wolf can still die to a single hit >= hp_max (the refill runs after);
      diagnostic only.
    - Identical repeated log lines can be dropped by the overlap match.
    - world.rs's placeholder capsules are PostStartup only. Every exported chr has a
      skeleton, so later spawns are not affected.
  - Round 48 suite: 456 passed, 0 failed, 41 ignored.
- **Round 49** (prosthetic.rs bug hunt):
  - Fixed: the Divine Abduction (077) air press without ref 309. It takes the HKS's last branch,
    "071 or 077" (c0000_transition 3083): W_AirSubAttackCombo1, counted. Before it was dropped as
    "no event". Asserted in tools_work_in_the_air.
  - Fixed: a piercing bullet (isPenetrate) on its landing / last frame hits everyone it
    crosses. Before, only the first: the Flame Vent puff 720000, the firecracker burst 710003 /
    710013.
  - Fixed: the tool rows (v<var>:<judge>, the bare judge) resolve only from tool anims. Before,
    a sword anim's judge with no v5000 row fell through to them unpaid. None in the data today
    (checked over every a000-a069 / a080+ BulletBehavior).
  - Fixed: a switch press while AddSubWeaponChange still plays is refused (validate 6394:
    IsNodeActive("AddSubWeaponChange_CMSG"); ref 320 100359 is only on that clip, a000_412090
    0-0.3 s). Before, each press restarted it and cycled on. Asserted in
    switching_prosthetics_plays_the_next_tool (mutation-checked).
  - Kept: a switch mid-use is the game's. g_behaviorTable[600] allows 12 of the 16 state
    types, attacks included. spawn_bullets reading the current slot is taken as the exe's
    left-hand weapon read (gap: not traced).
  - Round 49 suite: 458 passed, 1 failed. the_guardian_ape_gets_up_headless_after_his_first_todome
    is Bosees' test under edit (no Event20200); sent to the overseer.
  - Re-run after the switch gate: 458 passed, 2 failed (the ape now passes):
    lady_butterfly_changes_into_her_second_part and every_boss_row_reaches_its_defeat, Bosees'
    boss-transition work (#3); sent to the overseer.
- **Round 50:**
  - Fixed: validate 6220's flamethrower clause (prosthetic::hks::flame_gate). Inside ref 329
    (100290 "only the flamethrower cancels", a072 clips) only a Flame Vent level with ref 325
    takes a press; a refused press stays buffered. Before, a tap there with any other tool, or
    a level without 325, replayed Combo1 and paid again. Asserted in tools_work_in_the_air.
  - Round 50 suite: 463 passed, 0 failed, 41 ignored, on the second run.
    - The first run had 1 failure: wolfs_lightning_charge_glows_while_it_lasts (VFX's test;
      status.rs electro_glow). It passes alone and on the rerun.
    - Likely cause: one app.update() under suite load can run no FixedUpdate step before the
      assert. Sent to the overseer for VFX.
- **Round 51** (status.rs / actor.rs bug hunt):
  - Fixed: a despawned character's affliction loops stop (status.rs tick). Before, the
    retain dropped its Status without stopping the kept midst fx (poison 4011, flames 3012),
    which looped on at its last spot after a phase swap or Genichiro -> Tomoe. Test
    a_despawned_enemys_poison_effect_stops (mutation-checked).
  - Fixed: posture regen counts each SpEffect once. A TAE row with effectEndurance > 0 is in
    the window and in sp_chained over its first frames: the rate was squared and the change
    doubled (c1010 300240: +80/s for 0.1 s). Test a_tae_speffect_counts_once_in_posture_regen
    (mutation-checked: 10 points vs 6).
  - Fixed: TAE 760 with no target (the lock target gone, unlocked with nothing to home on)
    sets root_scale 1.0. Before, the last frame's scale stayed into the next clip. gap: the
    exe's no-target branch of FUN_140842980 is not traced.
  - Doc: the posture-regen formula comment moved from run_sp_effect_chains to regen_posture.
  - Traced (scout, static): the hit stop is a per-character scaled dt.
    - FUN_1409f78a0 (1409f.c:6756) runs the HitStop module FUN_140b9d1b0 (140b9.c:9339:
      +0x1c raw dt, +0x18 scaled; hold / freeze / (t/len)^2 ease as HitStop::split) and writes
      the scaled dt into chr+0x108. The later chr tasks get that FD4Time.
    - Physics and fall take the RAW dt: FUN_1407e1e20 builds an FD4Time from HitStop+0x1c
      (1407e.c:2650) for FUN_140bc3470, which runs the fall module FUN_140bac120 and the Havok
      step. So gravity integrating on real dt (player.rs) matches.
    - gap: whether the exe also moves the body by that velocity during the freeze (ours
      freezes the position step via the hit-stop clock) is not traced.
    - Probably scaled (inferred; Ghidra lost the dt args at FUN_140a36cc0 / 140a0.c:3037):
      the SpEffect tick (FUN_140a02a10 -> 140c0.c:403) and posture regen (FUN_140a04850).
      Ours runs both on real dt. Not changed until a disassembly of the xmm1 argument settles
      it.
  - Round 51 suite: 467 passed, 0 failed, 40 ignored.
  - Fixed (R47 follow-up): the drill-attack reset keeps an anim that any kind has, not only
    kinds[0]. Bosees' SHINOBI_AI_ATTACK=3042 picks c7110's later cast member's lightning,
    which the R47 check would have wiped in the game (the sim tests have no debug menu).
- **Round 52** (player.rs death / deathblow bug hunt):
  - Fixed: DeathblowStart has an exit. When the main throw cannot start (its enemy despawned
    in a boss swap, no row 0201 for the vault, no Wolf clip), Wolf goes back to StandIdle.
    Before, he stood frozen with every input ignored. Test
    a_deathblow_start_without_its_enemy_returns_to_idle (mutation-checked).
  - Fixed: the reset clears the last life's death pick (death_ref / death_hit), the shock and
    the aged_form flag. A death that plays its own anim (ThrowDefDeath*) now spends the pick.
    Before, it was left for the next life's plain-hit death (a poison / fire / insane start).
    Test the_reset_clears_the_last_lifes_death_pick (mutation-checked).
  - Fixed: the aged deathblow steps aside from aging_step only where Attack is accepted. Before,
    a press mid-onset, mid-attack or in an aged fall skipped aging_step every tick.
  - Fixed: the aging onset spares every pair state (actor::in_throw, now pub(crate)): a grab,
    the kick vault, the Mikiri pose. Before, it cut them and left the grabber in ThrowAtk.
  - Fixed: a queued revival press is spent when the choice opens. Before, `||` skipped the
    take on a fresh press, and the leftover revived the next death on its first tick (a second
    node).
  - Not changed (low):
    - the procedural "Dead" fallback, when clips are missing, has no loop exit;
    - hp <= 0 during the revival clip is not re-checked.
  - Round 52 suite: 472 passed, 0 failed, 40 ignored.
- **Round 53** (player.rs movement / air bug hunt):
  - Fixed: Sakura Dance's ground leap (110 JumpReady) counts its one air use (HKS 3413 / 3445).
    Before, only the air branch counted, so a second Sakura Dance was taken in the leap. Test
    sakura_dances_ground_leap_counts_its_air_use (mutation-checked).
  - Fixed: a wire perch is kept through a jump or an air reaction while Wolf stays over it, and
    it is a landing floor. Before, it was dropped on leaving the ground, and he landed through it
    on the ground below. Asserted in a_grapple_lands_on_the_point_in_the_move_start_time
    (mutation-checked: y 0.9).
  - Fixed: the idle quick turn takes the lock target only. Before, the attack's auto-homing
    (a.homing) turned an unlocked Wolf after a homed swing, as the step was fixed in round 31.
  - Not changed:
    - wire_throw's orbit lacks fall_reaches' below-target cut-off. That cut-off would refuse
      the rising grapple below the enemy, and the exe's rule is not traced.
    - Air presses with no air action (gourd, step) stay buffered into the landing; whether the
      exe latches them is not traced.
  - Round 53 suite: 476 passed, 0 failed, 40 ignored.
- **Round 54** (player.rs attack / guard bug hunt):
  - Fixed: no gourd and no crouch toggle in guard style. The HKS g_behaviorTable rows ITEM_USE
    and CROUCH_START are 0 for GROUND_GUARD (c0000_transition.lua 6450), so accepts() refuses
    them in the deflect-guard states (new is_guard_style). Before, the gourd was drunk while
    guarding.
  - Fixed: the tool switch works in the air (ADD_SUB_WEAPON_CHANGE is on for FREE_FALL /
    WIRE_FALL). A second press while AddSubWeaponChange plays is dropped (Validate 6394).
    Test no_gourd_while_guarding_and_a_tool_switch_in_the_air (mutation-checked).
  - Fixed: guard + attack pairs into the combat art only with an art equipped (HKS 6193:
    env(345) ~= SP_ATK_TYPE_NONE). Before, without an art, the pair became a CombatArt request
    that nothing took, so both the deflect and the slash were lost. Test
    guard_and_attack_without_an_art_still_slashes (mutation-checked).
  - Not changed (gaps):
    - 6193 also needs the guard held now (guardCount > 0). A released guard tap within 0.1 s
      still pairs.
    - An art combo with no follow-up, the art release order against the plain attack,
      deflect additive refs 228 / 503 in guard style, and refs 229 / 233 / 235 were not traced.
  - Round 54 suite: 480 passed, 0 failed, 40 ignored.
- **Round 55** (sound.rs bug hunt):
  - Fixed: the first step of Locomotion plays the start clip's events at 0 (e.g. a000_005100
    PlaySound 2100). Before, its window was (0, dt]. ffx.rs emit uses sound_windows, so it gets
    this too. Test the_first_locomotion_step_plays_the_start_clips_sound_at_0 (mutation-checked).
  - Fixed: the deathblow muffle (ntc_ninsatsu2 445 Hz) ends on Title / Picker / Dead.
    deathblow_mix runs in FixedUpdate, frozen off Playing, so quitting mid-deathblow kept it on.
    DeathblowMix is now a Resource. Test the_deathblow_muffle_ends_on_the_title (mutation-checked).
  - Fixed: play_tae_sounds / deathblow_mix run after PoseSet and before Resolve. Before, the
    order against Resolve was Bevy's choice; run after it, a reaction's restart dropped the
    interrupted anim's events of that step. For VFX: ffx.rs emit has the same open order
    against Resolve.
  - Fixed: the per-actor step map is pruned of despawned actors (it grew per enemy spawned).
    The sound_windows doc now says a turn or a walk / run switch keeps the clip phase.
  - Not changed:
    - Player loops wrap with a.t %= len without prev_t = 0. No looped player clip has an event
      before 0.034 s, so nothing is lost today.
    - Whether a slot's FMOD handle can be reused after a one-shot ends is unconfirmed.
  - Round 55 suite: 482 passed, 0 failed, 40 ignored.
- **Round 56** (world.rs / enemy/script.rs review, nothing changed):
  - world.rs: enemy_visuals runs only in PostStartup, but a later spawn never lacks a model:
    AnimLib::lib falls back to the first enemy's skeleton. Not a bug.
  - enemy/script.rs is the boss EMEVD runner only (boss_scripts.json: 19 main-boss rows plus
    12600050, the Folding Screen Monkeys mini-boss). For Bosees and the mini-boss owner:
    - Duplicate (id, slot) events run twice in Run::new: 50601010 (11005900, 11005910),
      50900000 (11005800, 11005810) and 50900001 (11005800). Whether the game ignores a second
      start is unconfirmed; deduplicating in Run::new or tools/boss_scripts.py would fix it.
    - The snapshot maps stealth CAUTION to EMEVD AI state 2 (Alert), so state 1 (Recognition)
      never matches. The emedf enum is 0 Normal, 1 Recognition, 2 Alert, 3 Combat. Unconfirmed
      against the exe.
    - A normal end of a slot N > 0 event also sets the bare id flag (slot 0's done flag), at
      script.rs 756-758. Unconfirmed. The sampled slotted events end by restarting, which sets
      no flags.
    - A warp to a short-warp character before it is enabled is dropped, because it spawns at
      PARK (script.rs 965-970). This is latent.
    - These conditions always evaluate false: 3[24] Action Button, 5[2] ObjAct flag, 13[1]
      Player Looking At, 11[2] Standing On Hit. Monkeys events 12005874 / 12005827 wait on
      13[1].
- **Round 57** (status.rs / debug_menu.rs bug hunt):
  - Fixed: ailment immunity counts the SpEffects a character wears, not only its afflictions.
    These are NpcParam residents: 90111 "Poison invalid" (134 rows: zombies, geckos, monkeys),
    90011 poison and flame (147 rows), and 91011 fire (9 rows). Terror now checks
    disableDisease. Before, the Sabimaru filled a zombie's 999 poison gauge. Test
    a_poison_invalid_resident_keeps_the_poison_off (mutation-checked). gap: Wolf's worn rows
    (110010 Pretending Dead disables all four) are not passed to hit_wolf.
  - Fixed: an instant HP-rate row (effectEndurance 0) dies with its own behaviorRefId and is
    logged under its own name. Before, every instant row was terror's 590 insane death. Poison
    4001 / 4003 are instant, but no exported attack carries them today, so there is no test;
    the_zombies_dumplings_build_up_terror still covers 590.
  - Fixed: player_cheats runs after status::tick, so god mode / no posture win over a poison,
    burn or terror tick or the Aging drain in the same step.
  - Fixed: debug "heal enemy" and the two stealth setups also clear the target's afflictions.
    Before, a healed burning enemy kept burning.
  - Fixed: an instant row's effects play once when its gauge fills (Status.flash, shown by
    show). Terror 9200-9223 vfx 48020 is init 5013, the kanji on dummy 280. Before, start
    kept timed rows only and it never showed. Asserted in the_zombies_dumplings_build_up_terror
    (mutation-checked).
  - Not changed:
    - start replaces by spCategory alone. 10004 is shared by poison 1200 / 6500-6515 and burns
      9100-9125, so a poison-cloud fill ends a burn. The exe's category rule and
      categoryPriority are not traced.
    - StealthFar forces aggressive in Passive mode, and the AI-mode line reads the first
      enemy in the query rather than the target.
  - Round 57 suite: 485 passed, 0 failed, 40 ignored.
- **Round 58** (actor.rs / perf.rs bug hunt):
  - Fixed: the airborne vertical step runs on the raw dt, hit stop or not (actor.rs fall). The
    exe gives the fall module and the Havok step the unscaled time (1407e.c:2650), and
    player.rs's gravity already used it. Before, the position froze while vel_y kept building,
    so an air hit's arc came out lower and shorter. This closes the round-53 gap "whether the
    exe moves the body during the freeze" by that trace. Test
    a_hit_stop_in_the_air_does_not_freeze_the_fall (mutation-checked).
  - Fixed: a frozen tick still writes tf.rotation from a.yaw. Before, a turn decided during
    the hold was drawn and hit-tested late.
  - Fixed: Wolf's fight reset clears the old fight's clock state: hit_stop, add_anim / add_t,
    tae_prev, sp_chained, attack_hit, regen_carry, kb_vel and since_posture_damage (99).
    Enemies get a fresh Actor. Before, a running hit stop froze him at the start, a deflect's
    additive window stayed live, and the stale tae_prev handed 109450 on to 109451 (regen off)
    on the reset tick. Asserted in the_reset_clears_the_last_lifes_death_pick
    (mutation-checked).
  - For Bosees (SuperArmor, actor.rs 69-94): the exe re-arms an idle timer (-1 < timer < 0)
    when the pool is not full and no stateInfo-155 bonus is on (140bd.c:8906-8911), and counts
    down at timer >= 0. Ours counts only at > 0, so after a bonus row (3510991 +120) goes, cur
    stays above max.
  - Not changed:
    - The SpEffect chain timers, SuperArmor tick and posture regen still run on real dt
      (round 53 gap: scaled is only inferred).
    - separate looks up NpcParam radius / hitHeight per pair, and regen_posture clones graph
      51 every tick. This is cost only.
  - Round 58 suite: 486 passed, 0 failed, 40 ignored.
- **Round 59** (prosthetic.rs bug hunt):
  - Fixed: Bullet spEffectIDForShooter is applied to Wolf (data.rs BulletSpec field, onto
    p.timed), and the timed rows' stateInfos join resident_gates (sync_art_variation). The
    Flame Vent follow-up 720040 -> 106020 "Flame enchantment" (7.5 s, stateInfo 62) and 720340
    -> 106030 (20 s, 64) now open the sword's a050 BulletBehavior judges 295 / 290 (gated on
    StateInfo 62 / 64, 124 + 56 clips). Before, the sword never caught fire. Test
    the_flame_vent_enchants_the_sword (mutation-checked). gap: children rows fired in flight
    don't apply theirs. data.rs: one new serde field (FYI Bosees).
  - Fixed: Wolf's bullets respect the enemy's invulnerability (combat::dodged: TAE 950 / 951,
    954 IFrames), as the sword does, and the hit is not recorded. Before, a shuriken hit and
    interrupted a knock-down or deathblow throw. Test a_shuriken_passes_an_invulnerable_enemy
    (mutation-checked).
  - Fixed: held_press has HKS 6220's umbrella clause: 076 held inside
    SP_EF_REF_ENABLE_PRESS_SUB_ATTACK (311, 100368 in most recovery windows) opens the Loaded
    Umbrella without a fresh press. Asserted in tools_work_in_the_air (mutation-checked).
  - Fixed: the hit radius grows to hitRadiusMax over spreadTime (Flame Vent 720001 1.0 -> 1.6
    m), and the hit record runs out after dmgHitRecordLifeTime (720001 0.6 s, 740200 0.33 s),
    as enemy_bullet.rs does.
  - Fixed: homing aims at a live lock target only.
  - Fixed: the bullet's DummyPolyID goes through combat::dmy_entity (Lightning Reversal 10100,
    the Sabimaru's 21130). gap: 1130 may be missing on the tool model, so it still falls back.
  - Not changed: a ground tool switch is taken mid-tool-anim (ref 320 is only on a000_412090),
    and the anim's later bullets resolve through the new tool. The game's behaviour there is not
    traced.
  - Round 59 suite: 488 passed, 0 failed, 40 ignored.
- **Round 60** (player.rs reactions / death / revival bug hunt):
  - Fixed: is_death_state takes only the grab deaths (ThrowDefDeath*, ThrowDefRevival*). A
    throw's blow-away (ThrowDefLargeBlow*, env 206) plays out and its landing's end starts the
    death. Before, "ThrowDef" also matched the Ape's non-lethal 600380 blow-away, and a tick that
    emptied HP there left Wolf dead with no death, revival or reset.
  - Fixed: a no-hit death (a status tick) in the air takes W_FallDeathDmgStartNaturalDeath
    (HKS 813-817, damage level NONE), and fire / terror / poison starts are ground-only (960-972).
    Before, GroundDeathStartPoison played mid-air. Test a_tick_kill_in_the_ape_blow_away_still_dies
    covers both (each part mutation-checked). gap: HKS puts NONE before AGING_FALL; aged keeps
    AgingFallDeathStart.
  - Fixed: BEH_R_DEATH also fires during the revival anims (HKS 5605). Before, a tick that
    emptied the revived 1 HP before 110015's +50 % let Wolf get up at 0 and act. Test
    a_kill_during_the_revival_dies_at_once (mutation-checked).
  - Fixed: the death branch's fall counts the perch as a floor, and a dead body above the floor
    (knockback / root motion off a ledge) starts falling. This does not apply in grab deaths or
    SetNoGravity windows. Before, the body floated or fell through the perch. Test
    a_dead_wolf_above_the_floor_falls (mutation-checked).
  - Not changed:
    - The main deathblow doesn't re-check that the enemy is still broken after the 0.3-0.4 s
      step-in (main_deathblow / BreakKickJump). The exe's re-test is not traced; the plunge
      landing does check.
    - With a node left, the resurrect choice dies on its own after 6 s (death_wait > 6.0). The
      HKS has no timer (290-292, env 3028 only). For the user: keep the timeout, or add a
      "die" input?
    - HKS 274-276 also allows revival from the death start itself; ours waits for the loop.
  - Round 60 suite: 491 passed, 0 failed, 40 ignored.
- **Round 61** (player.rs guard / deflect bug hunt):
  - The rule behind these fixes, from g_behaviorTable (c0000_transition 6450): in
    STYLE_TYPE_GROUND_GUARD states only BEH_A_DEFLECT_GUARD_CONTINUE (4460-4477) and _END run,
    never _START. The guard-style states are the guard starts, the easy recoils
    StandDeflectEasy*, LandAirDeflect*, EasyDeflectDmg and StandDeflectGuardFromDamage (cmsg
    g_paramHkbState). The hard deflects StandDeflectHard* are STYLE_TYPE_STAND.
  - Fixed: a guard press in a guard-style state (not guard idle) is CONTINUE: ref 212 steps the
    spam chain, else ref 227 gives StandToDeflectGuard1Reverse, else StandToDeflectGuard. It
    needs no hold (validate 6170: env 1106 only) and never gives the sprint deflect. Before:
    - a press in SprintToDeflectGuard's ref 1 frames restarted it (a fresh 12-frame window and
      slide, no spam penalty);
    - LandAirDeflectGuard's ref 212 was skipped;
    - taps let go before the window were dropped.
  - Fixed: the additive deflect (ref 228) and the hit-deflect cancel (503) are START branches,
    so they no longer apply in guard style. The easy recoils' 228 now gives 227's Reverse
    instead of a layer over the recoil. The old test guard_during_a_block_recoil_adds_a_deflect_window
    asserted the layer from the ref alone, with no live source. It now checks the layer on
    StandDeflectHardSmall and the Reverse on StandDeflectEasySmall.
  - Fixed: BEH_A_DEFLECT_GUARD_END (6176) runs in every guard-style state with ref 207 up.
    Before, the easy recoils / LandAirDeflect* played on and snapped to idle without
    DeflectGuardToStand.
  - Test guard_style_presses_continue_the_guard (mutation-checked, with the rewritten test).
  - Fixed: BEH_A_AIR_DEFLECT_START's ref 228 branch (HKS 2968-2970). A guard press in an air
    recoil with 228 up and 412 down adds AddHardDeflectGuard, and the recoil plays on. Before,
    it was cut into AirDeflectGuardStart, which changed the landing. Test
    guard_in_an_air_recoil_adds_a_deflect_window (mutation-checked). gap: 6061's 411 + 412
    released-tap clause in the air.
  - Not changed:
    - One additive slot is shared: AddSubWeaponChange and the Minimum flinches
      (combat.rs 1587 / 1761) overwrite AddHardDeflectGuard and its 203 / 411 window. The HKS
      runs them as separate layers (cmsg 10000-10015).
    - Unconfirmed: START sets AddDeflectGuardBlend 0 (4432); ours keeps add_anim through a
      guard start.
  - Round 61 suite: 493 passed, 0 failed, 40 ignored.
- **Round 62** (player.rs offence bug hunt):
  - Fixed: the attack button alone continues an art combo in its ref 223 window.
    BEH_A_GROUND_ATTACK tries _FireSpAttackCombo first (HKS 3256 / 5367); a combo that is not
    unlocked falls through to the plain route. The new helper art_combo_pick is shared with the
    CombatArt path. Before, Ichimonji: Double's second cut by R1 became
    GroundAttackCombo1Reverse; the same happened to Praying Strikes, Spiral Cloud and Mortal
    Draw. Asserted in ichimonji_double_follows_up_but_ichimonji_does_not (art_chain_by,
    mutation-checked).
  - Fixed: no art in the wire flight. BEH_A_AIR_SP_ATTACK is off in STYLE_TYPE_WIRE_FALL, so
    the attack + guard pairing and the air-art branch skip while p.wire is set, and the press
    stays the wire-fall attack. Before, AirSpecialAttack dropped the wire.
  - Fixed: a wire-fall attack with a lightning charge throws it back first (HKS 2872-2876).
  - Test wire_flight_attacks_are_no_art_and_throw_lightning_back covers both wire fixes (each
    mutation-checked).
  - Fixed: the art releases keep the input buffer (FireEventNoReset, HKS 3467-3475 / 3493).
    The states added to NO_RESET are GroundSpecialAttackCombo2Release,
    GroundSpecialAttackCombo1ReleaseNoResource, SprintSpecialAttackRelease(NoResource) and
    GroundSpacialAttackHoldEnd.
  - Not changed (validate order, done in round 63):
    - Unconfirmed: sprint art type 101 should open with a101_316300 (HKS 3401-3403 sets
      Selector_GroundJumpType VERTICAL out of a sprint); ours uses the stick, giving 316301.
  - Round 62 suite: 495 passed, 0 failed, 40 ignored.
- **Round 63** (validate order, g_behaviorValidateOrder HKS 6449):
  - Fixed: the air presses go kick (and storm jump), art, air attacks, tool, then the deflect.
    Before, the order was guard, art, tool, kick, attack, so a pending guard beat a kick press
    when both opened at AirComboAttack's frame 12. Test a_pending_kick_beats_a_pending_air_guard
    (mutation-checked).
  - Fixed: the ground blocks go item, jump, step, tool switch, art press, art hold / its
    releases, art release, attack, release attack, tool, then the deflect. Before, the art,
    its hold, the switch and the tool came before item, jump and step. One Mind's hold ate
    jump and step at its flags 119 / 26 (frames 12-33), and a buffered step lost to an art
    press. Blocks were moved whole; the attack + guard pairing stays above them all. Test
    a_jump_press_leaves_the_hold_art (mutation-checked against the old order).
  - Round 63 suite: 496 passed, 0 failed, 40 ignored. Every existing test passed unchanged
    after the reorder.
  - Process note: mutation checks swap shared src files; Bosees' runs during one saw the
    mutated player.rs. Keep the swaps short.
- **Round 64** (Mikiri / perilous answers, hit-stop clocks):
  - Fixed (combat.rs; Bosees notified): a Mikiri needs the attacker's ThrowParam 0190 row,
    with Wolf having its atk anim. Before, c1300's 13000100-120 and c5080's 50800090 (no 0190
    row) cost the enemy 90 posture (staminaDamageAttackHitParry), and Wolf was hit anyway.
    dodged()'s Mikiri i-frame bypass now has the same rule, and never applies to bullets. Test
    a_thrust_without_a_mikiri_row_costs_no_posture (mutation-checked).
  - Fixed (the R51 hit-stop question, static trace): these exe functions read the character's
    own FD4Time (chr+0x100, float chr+0x108, written by FUN_1409f78a0 from HitStop+0x18):
    - posture regen FUN_140a04850: FUN_1405460b0(rate, time) reads time+8;
    - status recovery FUN_140bda460;
    - the SpEffect tick FUN_140a02a10 -> FUN_140c06b70.
    Only the fall and Havok take the raw dt (round 58). The arguments are verified; which
    vtable slot each function sits in is inferred.
    Port: Actor.clock_dt, the scaled dt set by advance (0 in the hold), with
    `Actor::clock(real)`. It is used by regen_posture (regen, run_sp_effect_chains and the
    SuperArmor tick), status::tick (recovery, effect timers, DoT ticks) and Wolf's p.timed
    (prosthetic.rs). Test hit_stop_holds_posture_regen_and_sp_effect_timers
    (mutation-checked).
  - Open (Bosees): enemy.rs `clash` timers (Enemy::add_timed, enemy.rs ~1818) still count on
    real time; they should use `a.clock(dt)`.
  - Unconfirmed, not changed:
    - ThrowBreak should hold the held entity (player.rs ~2785).
    - Crouch from a sprint without unlock 26 should go to CrouchMoveLoop (player.rs ~2430).
  - Round 64 suite: 500 passed, 0 failed, 40 ignored.
- **Round 65** (world / duel / sound / debug_menu, plus the R64 leads):
  - Fixed: crouch out of a sprint without unlock 26 is the plain crouch (HKS 4500-4509;
    BEH_A_CROUCH_START is on in the sprint style, g_behaviorTable column 9). Moving goes
    straight to the crouch move loop, idle goes to CrouchStart. Before, the press was dropped
    because SprintLoop isn't a free state. AddActionInputBlend / W_AddDisableStep are not
    modelled. skills_unlock_their_moves is extended (mutation-checked).
  - Fixed: the ThrowBreak deathblow targets its ThrowHold partner. The old scan for the first
    broken enemy in flag 67 is only the fallback without a hold; with two broken enemies the
    scan could pick the wrong one. No new test (needs a two-enemy break setup); the mikiri and
    deathblow filters are green.
  - Fixed: duel.rs keeps Wolf up in FixedUpdate after Resolve and status::tick, and clears
    death_ref / death_hit. Before, the Update-side restore let a two-step frame play the kill.
  - Fixed: debug "heal enemy" ends a posture break through its normal recovery
    (Enemy::end_break_now in enemy.rs; a small edit, Bosees notified). Before, a healed broken
    enemy stayed open to the deathblow.
  - Refuted: a locomotion loop swap mid-stride does not burst footsteps. locomotion_clip_at
    reads the current move_index for prev_t too. Guard test
    a_mid_stride_loop_swap_does_not_burst_its_sounds.
  - Fixed: the debug "no posture damage" cheat. It zeroed posture after Resolve, so one blocked
    hit that filled the bar still broke the guard. player_cheats now sets Actor.no_posture,
    which makes Actor::add_posture add nothing; combat.rs is untouched (its direct posture
    writes are clamped below max). Test the_no_posture_cheat_blocks_a_bar_filling_hit
    (mutation-checked).
  - Round 65 suite: 503 passed, 0 failed, 40 ignored. The FCC T0 run was withdrawn because the
    free tier was out for the day, so the suite ran here.
- **Round 66** (items, death / revival, perch / wire, status build-ups):
  - Fixed (static): the resurrection choice opens 1.8 s after HP empties and stays open 15 s.
    - Timers: PlayerIns +0x2134 = 1.8 and +0x2128 = 15.0 (FUN_140a217a0, 140a2.c:4244-4246).
      FUN_140a2cc30 arms them at death (140a2.c:8272-8278); FUN_140a36a60 counts them down
      (140a3.c:3351-3380), holding the 1.8 s while stateInfo 351 is worn. The end is the final
      death (FUN_1409eda50).
    - The HKS revives from the death starts too (c0000_transition.lua 274-277): the plain,
      fire, poison, insane and shock starts, the blow lands, the land fall deaths and
      ThrowDefDeathStart. The revival from a start gets up on death_done's side.
    - Before: the choice opened only at the stay loop (1.6-3.3 s late) and closed 6 s into it.
      This answers round 60's open 6 s timeout and the revival from the death start.
    - A start revive skips the loop's f0 cure (8002-8009), as in the HKS. The revival clip's
      own f0 8001 (posture) still runs.
    - Player.dead_for, REVIVE_DELAY / REVIVE_WINDOW. With no resurrection left it is the old
      1.5 s in the death loop.
    - Tests: mashing_attack_through_death_revives_at_1_8_s (out of GroundDeathStart_F at frames
      105-112, mutation-checked) and a_revive_in_the_stay_loop_still_cures_the_poison (the
      round 23 guard, rewritten).
  - Fixed (static): an affliction's damage tick lands on the frame it takes hold.
    - FUN_140c06860 / FUN_140c03af0 (140c0.c:5011, 2506) zero the interval timer on every set
      (all stateInfos but 50 / 52 / 67 / 68). FUN_140c06b70 ticks at <= 0.
    - status.rs start sets next_tick 0 on push, on a category replace and on a re-ignite.
    - Before: one motionInterval late, so a 9.9 s burn ticked 9 times and poison 9045 ticked 19.
    - Test poison_ticks_from_its_first_frame (20 ticks from frame 0, mutation-checked).
  - Fixed (data): the Great Shinobi Owl's recovery seal 3506090 (stateInfo 910, 30 s,
    spCategory 193) now stops the gourd.
    - Goods 3000's useLimitSpEffectType0-4 are 341 / 910 (data::goods_use_limits). A worn row
      (worn_sp plus p.timed) with one of those stateInfos makes the press W_ItemFailed (HKS
      3940-3951): no drink, no charge.
    - gap: ItemFailed a000_259000 and CrouchItemFailed are not exported, so the press is only
      spent.
    - The death loop's refresh chain 8002 (cat 194) -> 8007 (cat 193) now drops Wolf's timed
      rows in those categories (apply_refresh_effects, SpCycle gains category / replace).
    - Test the_owls_recovery_seal_stops_the_gourd (both parts mutation-checked).
  - Matched, no change: the gourd branch order, the empty / repeat shakes, the item gating,
    the death-start and loop picks, perch / wire flow, the status build-up, guard and instant
    rows.
  - Round 66 suite: 510 passed, 0 failed, 40 ignored.
- **Round 67** (prosthetic emblem costs, a shallow pass):
  - Refuted: "one tool use pays several times" (several wepCost judges per tool in
    BehaviorParam_PC). A probe (one press, 240 ticks, 50 emblems) spent exactly resourceItemA:
    70000 1, 70300 2, 71000 2, 72000 3, 73000 2, 75000 1, 76000 1, 77000 3.
  - To check: 70400 spent 0, so its press didn't fire in the probe (unlock, goods or setup?).
  - Open, untraced (exe consume call): the TAE 1 / 940 payment paths don't skip when emblems
    are short (the bullet path does). Left as is until the exe's check is traced.
  - Not covered yet: the bullets, the hit records, enemy SpEffects and the tool-switch routing.
  - No code change; no suite run.
- **Round 68** (tool bullets, Sen Throw, tool switch):
  - Fixed: Sen Throw (70400) threw nothing at any sen.
    - Its throws' judges (101-104, 111-114, 151-154) are gated by stateInfo 999-996.
      common.emevd (~193180-193570) sets those through 127060-127063 "sen held LV0-3" while
      Wolf wears 127022: 0 / 1-249 / 250-499 / 500+.
    - sound::active_state_infos now pushes the level from the new `config.player.sen`
      (serde default 500, the full throw; config.toml untouched).
    - Test sen_throw_throws_by_the_sen_held replaces sen_throw_throws_no_free_shuriken
      (mutation-checked).
  - Fixed: no tool switch while ref 316 is up (the umbrella's guard / release clips, SpEffect
    100393) or while the last switch's expand is pending (g_EndSubWeaponChange), per HKS 6393.
    Test no_tool_switch_in_the_umbrellas_release_window (mutation-checked).
  - Changed, unverified: the chain's shared hit list (isUseSharedHitList) now expires entries
    after dmgHitRecordLifeTime, as a bullet's own record does. A Flame Vent test still saw one
    hit (the parent 720000 lands; the second may be blocked by the enemy's fire-reaction
    i-frames), so the second flame hit is still open.
  - Open: the tool expand only fires from StandIdle (BEH_R_SUB_WEAPON_EXPAND 5745 also takes
    CrouchIdle and anim end). Round 69 fixed the crouch half (below).
  - Open, low confidence: firecracker 710000 / 710020 / 710050 may spawn their child twice (at
    0.1 s and at the end).
  - Round 68 suite: 512 passed, 0 failed, 40 ignored.
- **Round 69:** crouched, the switched tool unfolds in CrouchSubWeaponExpand1 (a07x_412100;
  BEH_R_SUB_WEAPON_EXPAND 5745 takes HKB_STATE_CROUCH_IDLE). Before, the standing expand stood
  him up. Test a_crouched_switch_unfolds_crouched (mutation-checked). Still open: the expand on
  anim end (env 1105 / 2000) outside idle. Round 69 suite: 513 passed, 0 failed, 40 ignored.
- **Round 70** (sound events):
  - Fixed: sound::active_state_infos now counts the live residents (a.resident) for every side.
    - Before, an enemy's NpcParam residents never gated its TAE PlaySound / SpawnFFX events. The
      anim-set rows 200030-200034 give stateInfo 270-274, 3107040 gives 950 / 951.
    - 19 chrs affected, e.g. c1180 (236 events gated 270), c1220, c1260, c1700 (809 gated
      950 / 951) and c7500.
    - Test an_enemys_resident_gates_its_sounds (mutation-checked).
  - Open: CharData::fires / resident_gates (data.rs:1618, 2033) is built once from the default
    NpcParam row. It ignores the TAE anim-set overwrite (spCategory 100 replaces the resident)
    and script-set residents, so the 270 and 271 sets can both fire.
  - Matched, no change: the HitEffectSe lookup and the slotted / deathblow-mix logic.
  - Round 70 suite: 514 passed, 0 failed, 40 ignored.
- **Round 71:** the sound / FFX gate (sound::active_state_infos) applies the anim-set overwrite.
  - A clip's TAE SpEffect in a resident's spCategory (200030-34, category 100) drops that
    resident's stateInfo, so only the clip's set gates. Test
    a_clips_anim_set_overwrites_the_resident_gate (mutation-checked).
  - Still open: data.rs CharData::fires (events_at, the attack / bullet / TAE-event gates) uses
    the static per-kind resident_gates. It needs the actor; enemy.rs / enemy_bullet.rs are
    Bosees' callers.
  - Round 71 suite: 516 passed, 0 failed, 40 ignored on the rerun. The first run had one fail, every_boss_row_reaches_its_defeat (c7110 71101000, no defeat); it passes alone with and without the change, so it is flaky or ran during a parallel edit.
- **Round 72** (Wolf guard / deflect / reactions; a shallow pass): no bug found.
  - Matched: posture regen and its ratio types, the deflect window, the knock-down -> wake-up
    chain, and the return to guard idle.
  - Refuted: ThrowDefLargeBlowLand600380 -> FaceDown (13252 is STATE_TYPE_REACTION_DEATH_START,
    so c0000.lua 358-363 never runs for it).
  - Edge, unconfirmed: in_guard_arc is false with an empty anim (a procedural state never
    blocks).
  - Next: chip damage through the guard, the damage-level x direction tables, and the
    guard-break branches (combat.rs 511-602, 1749-1789).
- **Round 73** (guard chip / guard break; combat.rs edits, Bosees notified):
  - Fixed: the open umbrella's guard chip uses its own EquipParamWeapon row (data::weapon_row,
    the full table). 76000-76300 have dark 100 / 100; the Lotus 76200 has fire 100, the Lilac
    76300 magic 100. guard_chip takes the guarding weapon. Test a_guarded_fire_hit_chips_wolf is
    extended.
  - Fixed: the umbrella's SubGuard breaks no longer knock Wolf down on push / small / middle /
    large hits. c0000_transition.lua:1158 lists only StandDamageBreak, StandDeflectBreak,
    LandAirDamageBreak and LandAirDeflectGuardBreak. New combat::hks_broken, used in
    player_break_state, broken_takes_normal_damage and the 1922 gate. Test
    umbrella_breaks_take_normal_reactions.
  - Added (Bosees' request): player::wolf_no_dead, the exe's SetHp NoDead clamp
    (FUN_140bd64e0). While enemy::script::WolfImmortal is set, HP below 1 from above 0 becomes
    1 and the death pick is dropped; it runs before Decide. Test an_immortal_wolf_stays_at_1_hp
    (the helper; the script rewrites WolfImmortal each frame).
  - Open:
    - Guard chip skips Wolf's own SpEffect damage cuts (npc_hit_damage's rate).
    - The chip-then-break order (needs the exe).
    - HKS env(1121) < 0 flips the damage direction; not modelled.
    - AtkParam_Npc guardCutCancelRate: round 75 found the exe does read it (FUN_140a12d60 NPC / FUN_140a24ba0 PC, (rate + 100) / 100; it returns 1.0 under the +0x138 flag). Its use site is not found, so blocks stay leak-free until a live check.
  - Suite: 517 passed, 4 failed, 40 ignored. The failures still fail with my NoDead clamp
    disabled (lightning ground shock, lightning air catch, boss defeat c7020), or pass alone
    (Isshin follows Emma). They look like Bosees' in-progress boss / bullet / script work
    (script.rs was mid-edit during the run); handed to Bosees and the overseer.
- **Round 74:** the guard chip takes Wolf's own SpEffect damage cuts. combat::damage_rates (the
  TAE and resident rows' per-element product) is shared by npc_hit_damage and guard_chip, so a
  guarded hit runs the same chain (kb/enemy.md). Test a_guarded_fire_hit_chips_wolf is extended.
  - Suite: 518 passed, 3 failed, 40 ignored. The same Bosees-side failures as round 73 (both
    lightning tests, boss defeat c7020). They still fail with the R70 / R71 sound gates and the
    R73 hks_broken gate reverted.
- **Round 75:**
  - Fixed: env(1121) is the signed knockback (HKS dispatcher 140b3.c:10637, knockback module
    +0x10). A pull hit (negative knockbackDist, e.g. AtkParam_Npc 12000241 / 12000242 /
    54000580) mirrors damage_angle F<->B and L<->R (HKS 854 / 1070 / 1212 / 1624), used only by
    SMALL / MIDDLE / PUSH. player_damage_state takes `pull` (atk.knockback_hit < 0). Test
    a_pull_hit_mirrors_the_small_reaction.
  - Matched: the air damage reactions (HKS 1091-1137). Optional: the `_` arm could play nothing,
    as HKS has no branch.
  - Round 75 suite: 521 passed, 1 failed (every_boss_row_reaches_its_defeat, Bosees-side as in rounds 73-74), 40 ignored. The lightning tests pass again.
- **Round 76 (jumps and landings, player.rs):**
  - Every non-sprint jump now sets positioning_land (EnablePositioningJumpLand), taken at landing.
  - Sprint jumps also land stiff after a 5 m drop (the short_fall arm matches SprintJump*).
  - Ground jumps keep the stick tilt (act 3025, _setPositioningJumpTilt / _set4DirJumpTilt) in root_yaw and in the TAE 920 launch, gated by jump_tilted. LockonForward falls back to 0.
  - Kick jumps take the same tilt.
  - Tests: jumps_off_ledges_land_stiff, a_locked_jump_follows_the_sticks_angle. blue_lightning_leaves_an_air_art_going failed once in a full run, then passed 6/6 alone and in the rerun (flaky, watch it).
  - Round 76 suite: 526 passed, 0 failed, 40 ignored.
- **Round 77 (sound.rs/world.rs hunt):** no confirmed bugs.
  - Open, plausible only: SpawnOneShotFFX sounds with SlotID -1 that end with their FFX (ffx::ends_with_event, IsRestrictToDummyPoly / kind 118) are never stopped in play_tae_sounds. Example: a072_400400 300161/300261 stack on every loop. Needs exe proof that the sound stops when the FFX terminates. Proposed fix: track them as Slotted with slot -1 and skip the slot retain.
  - Exe-checked, matching: hit/guard sound lookup (FUN_1410c0ea0 column order), DeathblowMix::step.
  - Not modelled: mix+0xa0 slot override and the +0x48 early return in FUN_140ed7980 (setters untraced).
- **Round 78 (duel/debug/script):**
  - Fixed: duel.rs keep_wolf_up now sets no_posture. Before, a hit that filled the bar broke Wolf inside Resolve, before the reset ran (the same bug as round 65 in debug_menu).
  - Open, plausible, Bosees' files: Act::ClearSp (script.rs:1218) and the boss 'clear' action (enemy.rs:565) never recompute anim_group after removing an anim-offset SpEffect. Fix: anim_group_of(d, resident).unwrap_or(0). Not yet confirmed that any EMEVD clears such an SpEffect.
  - The blue_lightning flake was HashMap key order in the test; Bosees sorted the keys.
- **Round 79 (status/actor):**
  - Fixed: an enemy that died while aged (9620, maxHpRate 0.125) respawned at 1/8 of its bar. The respawn set hp from the aged hp_max, then the cleared affliction's set_aging(None) multiplied hp_max back up. Fix: enemy.rs respawn calls status::set_aging(None) before hp = hp_max. Test: an_aged_enemy_respawns_at_a_full_bar (mutation-checked). Wolf's F5 reset already took aged.
- **Round 80:** 4.42's 4700 is Warp prohibited, not an anim offset; closed as a no-op (see 4.42).
- **Round 81 (player.rs clocks):**
  - Fixed: the Lightning Reversal charge (a.electro, 9490/9495) counted down on raw dt, so it drained during a hit stop. It now uses a.clock (the SpEffect FD4Time, round 65). The test hit_stop_holds_posture_regen_and_sp_effect_timers covers it (mutation-checked).
  - gap (not traced): the TAE 922 velocity-scale blend (vs.elapsed) now runs on a.clock, the same clock as its clip and event times. The exe's time source for FUN_140bb1980 is not traced.
- **Round 82 (prosthetic.rs costs / debug_menu.rs):** no confirmed bugs.
  - Lead: the TAE 1 and TAE 940 wepCost paths (prosthetic.rs ~126-150) take emblems with saturating_sub and apply the 940 SpEffect (e.g. 107700) without checking the balance. The TAE 2/4 bullet path skips the event when emblems < cost. Check whether the press gate already rules this out, and find the exe's behaviour, before changing anything.
  - Static check (round 83): the press gate is traced. env(3035) -> FUN_140a26010 -> FUN_14084dd10 (14084.c:7450) requires held >= resourceItemA/B/C (weapon +0x248..0x24a); a short press goes to SubAttackFailed. The consume call behind TAE 1/940 wepCost is not traced. The likely reading is that it is unconditional and the gate runs only at press, which matches saturating_sub, so there is no code change. gap: trace the wepCost reader to the inventory decrement after FUN_14079e0f0.
- **Round 83 (actor.rs / perf.rs):** no confirmed bugs. play() resets, clip-end clamps, the clock split and the super-armor refill all check out.
  - Lead: play() keeps add_anim/add_t, so a previous action's additive layer can run into the next clip. Whether the exe clears it is not traced. Round 84: still untraced (no exe handler found; most likely ended by the HKX layers themselves).
- **Round 84 (R77 FFX sound lead):**
  - Fixed (likely): an FFX that ends with its event now stops its sound. The FFX's GXFfxSoundHost dtor (14040.c FUN_140406010) releases each started SoundIns through SprjFfxSoundManager slot 0x30 (FUN_140f7b9f0 -> SoundIns+0x12 = 5). sound.rs tracks unslotted ending FFX sounds as Slotted slot -1, never replacing each other, so a072_400400 300161/300261 no longer stack per loop. gap: state 5's FMOD stop is not traced to the end. No FMOD in tests, so this has no unit test.
  - The suite once failed the_reservoir_run_ends_with_isshins_finisher, then passed with and without this change (Bosees' WIP).
- **Round 85 (status.rs build-up):** no confirmed bugs. Fields, decay rates (TentativePlayerParam row 0), clock, reset after a fill, and same-frame second hits all check out. Minor: two instant rows filling in one frame keep only the last row's instant_ref and name.
- **Round 86 (player i-frames / air / Mikiri):** no confirmed bugs. TAE 954 IFrameType windows on steps, jumps and kicks, deflect-into-jump, the BEH_R_LAND chain with its ref 201 gate (HKS 1855-1930), and Mikiri flag 125 on GroundStep_N/F f0-12 all match.
  - Lead: the BEH_R_LAND gate (HKS 5691) includes SP_EF_REF_TAE_NO_LAND_ANIM 103 (SpEffect 100330, on the launch starts a000_1004xx-1129xx f0-3..24). src/ doesn't implement it; checked: player.rs lands only on tf.y <= floor && vel_y < 0 (~4272), and the 100330 frames are the launch's rise, so the gate has nothing to block. No code change; the ref 103 gate is unmodelled (gap).
- **Round 87 (sound.rs outside FFX, resets):** no confirmed bugs. Slotted sounds stop on F5 and fight resets, enemy loop wraps fire neither twice nor never, and the deathblow mix fades on reset.
  - Small fix: an armour 'b' sound for an actor with no defense material above 0 is now skipped instead of playing "" (or, without FMOD, a stand-in rustle).
  - Checked: Wolf's death/revival loops reset prev_t at the wrap, skipping frame-0 events. None of them (a000_110010/11, 110020/21, 112030/31, 112040/41, 192510) has a sound or FFX event, so this is harmless.
  - Not traced: the floor and voice keys against the exe.
  - Round 87 suite: 532 passed, 1 failed (Bosees' new a_bullet_makes_its_appear_sound_where_it_starts, which passes 2/2 alone; it landed mid-run or is flaky), 43 ignored.
- **Round 88 (deathblows / throws / items):** no confirmed bugs. ThrowHold is one-shot and drops on a target loss or state change. Stale throw_target / ActiveThrow after reset_fight are dropped by their entity lookups. Gourd gates match the HKS.
  - Not checked: enemy-side ThrowDef exits on an interrupted throw (Bosees' side), and the ninsatsu <= 1 checks / ThrowParam row picks against the extracted rows.
  - Deferred (user, via the overseer 2026-10-13): Pellets, the sugars, Confetti and the quick-item cycle come after gameplay, together with map tone/lighting.
- **Round 89 (ThrowParam row picks):**
  - Correct: the ninsatsu <= 1 checks, the Todome rows (0180/0181, 0680/0681, main+8), plunge 151, kick-plunge 161, wire 170/171, kick-jump 200/201, aged, near and stealth rows.
  - Open, sent to Bosees: the break and Mikiri picks never add the second-form (HU) +500 offset.
    - c1021 (11021690) and c7410 (17410690) have only a 0690 Mikiri row, so they can never be Mikiri'd (combat.rs:379/1413/1422).
    - Isshin HU should take 15400690 / 15400620 / 15400510. Butterfly HU should take 15090510 (combat.rs:1419/1659).
    - The break-pose follow-ups at player.rs:2884-2886 miss 15400511/512, 15090511/512 and 15400621/622.
- **Round 90 (ThrowParam scan):** across all 10xxxxxxx-19xxxxxxx rows, the only characters where a picked suffix (5, 6, 10, 20, 21, 120, 151, 161, 171, 190) is missing while its +500 row exists are c1021 and c7410 (Mikiri 190 -> 690). No other throw type is affected.
- **Perf pass (4.54 chrome trace, after round 90):**
  - Run: release, `--features bevy/trace_chrome`, vsync off (SHINOBI_FPS_LOG=1), sandbox + c7400 + SHINOBI_SANDBOX_ENEMIES=c1020,c1010,c1040, SHINOBI_BENCH=10.
  - Render world: ~13.7 ms a frame, the bottleneck. The render graph itself (render_system) is only ~4.3 ms. The main thread's "render wait" is the pipelined wait on it.
  - Largest piece: bevy_render allocate_and_free_meshes, 4.4 ms a frame. Bevy 0.19 frees and re-allocates every modified mesh's slab (allocator.rs free_meshes, a TODO to reuse same-size allocations), and cloth rewrote 47 display meshes a frame with insert_attribute.
  - Fixed (cloth.rs, VFX's file, notified): the cloth Mesh assets are no longer modified. `apply` patches position/normal/tangent into a packed copy of the vertex buffer (Packed), and a render-world system (write_cloth_uploads, PrepareAssets after allocate_and_free_meshes) writes it over the mesh's existing slab with queue.write_buffer.
  - Result: allocate_and_free_meshes 4.4 -> 2.7 ms a frame, cloth::apply 0.59 ms, write_cloth_uploads 0.37 ms.
  - SHINOBI_BENCH fps is too noisy on this machine while other sessions build (the same binary gave 51-64 fps). One run each: before 57.5 fps, p50 14.5 ms, render wait 8.2 ms; after 71.6 fps, p50 13.1 ms, render wait 6.6 ms. Re-measure on a quiet machine.
  - Checked: cape-turn screenshots (SHINOBI_CAPE_TURN=180) match the old path at 4 matching times: cloth attached, swinging, same shading. Cloth tests 5/5.
  - Open (VFX): the remaining 2.7 ms is fxr.rs draw's batch meshes and ffx.rs trails, rebuilt every frame with insert_attribute at varying sizes. Fix: pad to capacity buckets and write in place the same way.
  - Other render-world costs: write_binned_instance_buffers (shadow) 1.1 ms, prepare_preprocess_bind_groups 0.9 ms.
  - VFX will do the fxr/ffx write-in-place fix after their lit-particle work lands.
- **Round 89 follow-up (player.rs, Bosees' throw_row_for):**
  - deathblow_check's second-form `hu` now comes from the row's forDef condition (combat::throw_row_for); the old anim_group == 1 rule only applies when no row matches.
  - The break-pose follow-ups (0011/0012, 0121/0122) take the HU rows the same way, falling back to the plain row.
  - Left as they were, so no row disappears behind an unworn forDef SpEffect (e.g. the Swordsman's 3140300/3140301 break rows): near 0005, wire 170/171, 179, 200 and plunge.
  - Suite 536/0/43.
- **Round 91 (Wolf's hit reactions):**
  - Fixed (combat.rs, Bosees notified): the air deflect / guard (player_guard_state, air) used the ground size table. The HKS air handlers (1285-1311 hard, 1322-1349 easy) have three sizes: Large for 3/4/5/6/9, ExLarge for 10/11, Small otherwise. So PUSH played Small, SMALL_BLOW played Large, and MINIMUM played the plain AirDeflect. Test air_deflects_take_the_air_size_table (mutation-checked).
  - Checked, matching: player_damage_state (4-way / FB, the pull flip, the level -> event map, HKS 1064-1200), player_air_damage_state, the broken knock-down and the guard-break pick.
  - Not done: the player.rs side (stagger lengths, cancels, knockback).
- **Round 92 (player.rs cancels / knockback after hits):** no confirmed bugs. The cancels come from the reaction clips' own TAE accept flags (115 / 117 / 26 / 119, read at prev_t for the HKS one-frame delay). _execDamageStep is in. apply_knockback uses the KnockBackParam times and TAE 226, with distance tests. Everything is data-driven, so no hand-tuned windows to be off.
- **R89 test coverage:** a_second_form_takes_its_hu_throw_rows (Bosees) now also asserts c7410 -> 17410690. It already covered c1021's Mikiri and the HU break rows (c5400 0620 / 0510, c5090 0510).
- **Round 93, USER BUG "most of the end deathblows are cut short by the title screen" (menu.rs, enemy/script.rs):**
  - Cause: watch_fight opened Won a fixed WON_HOLD 4 s (hand-set) after Handle Boss Defeat, cutting the finishers. Measured from the defeat: the Monk's Todome ~19 s (Wolf's 10 s Todome, her Event20200, then her scripted Event20006), the General's ThrowDefDeath12001 ~6 s.
  - Fix: Won waits for `menu::finisher_done`: Wolf back in a standby state (player::is_free) and every enemy's clip at its last frame. Safety cap FINISHER_CAP 30 s. A boss-script defeat needs no extra hold; WON_HOLD stays only for fights without a boss script (phase gaps).
  - Banner: EMEVD 2003[74] "Handle Boss Defeat and Display Banner" carries a Text Banner Type (sekiro-common.emedf: 1 Shinobi Execution, 5 Immortality Severed, 3 Gracious Gift of Tears). Act::BossDefeat(banner) stores it in Run::banner, and menu.rs defeat_banner shows it over the play at the defeat with no pause: the brush 忍殺 (MENU_Ninsatsu) for 1, the name in serif for 5 / 3 (no brush art exported).
  - gaps: 2003[12] (no banner arg) is taken as Shinobi Execution. The banner's 3 s on screen is hand-set (the game's banner movie is not decoded).
  - Tests: the_last_deathblow_plays_out_before_won (c5000 / c7100 / c1020: Wolf free and every enemy clip ended at "done", mutation-checked). The Isshin finisher helper (isshins_finisher_to_defeat, both Isshin tests) now also checks ThrowDef12900 ran to its end.
  - Suite 539/0/43.
- **Round 94 (deathblow presentation on regular enemies):** no confirmed bugs. The atk/def SorbDmy anchors (266 / 246 / 523 / 249, def 229 / 233), the enemy-grab dummy swap, the +Z facing and the upper/lowerYRange check all match ThrowParam. Open: throwFollowingType (657 rows set to 1) is never read; added to Live checks wanted. Not reached: the blood/sound TAE triggers, the camera, the enemy death/despawn timing.
- **Round 95 (deathblow sound / blood, corpse and respawn):** no confirmed bugs. TAE sounds fire once per clip run (sound_windows, AT_ZERO). The ThrowDef -> death clip swap at flag 69 restarts at 0 with no double events. The corpse stays for the clip + 2 s. The respawn resets everything spawn sets up (hp with doping, posture, ninsatsu, resident0, super armour, anim group, statuses, brain, mask). Not done: a per-clip audit of ffx.rs SpawnFFX slots in the a2xx_51xxxx clips.
- **Round 96 (ffx.rs SpawnFFX in the deathblow clips):** audited Wolf's 225 a2xx_51xxxx clips with FFX events and the c1020 / c1010 ThrowDef / ThrowDefDeath clips. Each spawns once, the slot / restricted ones end with their event, they sit on the owner's dummies, and 117 takes the target's materialSfx1.
  - Fixed: an effect that ends with its event was timed by its own age (the FXR clock) against the event length. A hit stop froze the clip but not the effect, so it ended before its end frame. ffx.rs emit now ends it by the clip's time (a.t <= event.end), with the age window + 1 s as a backstop. No unit test (the FXR pipeline is not in the test app).
  - Checked (SHINOBI_LOG_DUMMIES): Wolf's 7 / 55 / 58 / 151 / 213 / 260 are all loaded, so dummy_candidates' 220 fallback never applies to these clips.
  - Suite 540/0/43.
- Round 32 refuted / matching: the guard_damage_table, ExecGuardBlock order, GuardKick,
  GuardBreak / ADD / EXLARGE, the TAE sound types, hard-coded hit / guard / deflect sounds, the
  gourd repeat, empty shakes and crouch variants.

### 4.55 Cloth check of the other main bosses (2026-10-13)

Probe (`SHINOBI_FIGHT=<chr> SHINOBI_DUEL=1 SHINOBI_CLOTH_PROBE=1`, 50 s, ~210 samples each) for c5060, c5400, c5430, c5090, c7400, c7110, c5080, c7020; logs + `sum.py` in the session scratchpad `clothchk/`. Per cloth: median / p90 / max angle off the animation, and mean LocalRange gap.

- Body-hugging cloths (coats, chainmail, kimono, hakama, mino, manto, leg cloth) stay close to the animation: medians 2-36 deg, mean gaps well under their max gaps. No LocalRange runaway, and no frame-to-frame flips seen.
- Long thin free-hanging pieces (beards c5400/c5430/c7020, ropes/himo c5080/c5090/c7110, Tomoe hair02, Owl haori_new/breech) go furthest from the anim (p90 80-150 deg, max ~170). That is free swing during dashes and turns, which is expected for these pieces, not a solver flip.
- Every log also has one single-sample set of `c7400_*` entries with fixed values (and gaps up to 7.8 m in the c7020 run). This is a second c7400 cloth instance that is probed only once, probably the Emma ally/arena NPC model. It is not the boss, so it was left alone.
- I didn't compare frame-by-frame against the all-bosses clip in this pass. Nothing was found that needs a data/solver fix, so no code changed.
- **Follow-up (overseer):**
  - **Stray c7400:** it is the start-up enemy from config.toml line 60. SHINOBI_FIGHT swaps it out through the menu picker (menu.rs:758), so it is a test-launch artifact, not an extra entity.
  - **Clip comparison** (all-bosses clip, contact sheets at 1:01:20, 1:02:10 and 1:28:00 in the scratchpad `clothchk/`):
    - Owl's hair and cloak sit about 20-45 deg from vertical and reach about 60-80 deg in leaps.
    - Isshin's coat reaches about 70-90 deg in jumps and spins.
    - Nothing goes past horizontal. The beard and ponytail are too small to read. The Tomoe sample was a cutscene.
  - **Sv1 comparison:** the sim goes past 90 deg and more than 100 deg off the animation in a meaningful share of samples: Tomoe hair02 42 of 210, Owl haori_new 20 of 179, Isshin beard 7 of 209. These happen in the 3000-series attacks.
  - **Checked: no solver mismatch.**
    - Damping is pow(1 - globalDampingPerSecond, h), which is exe-proven (kb/visuals.md 287).
    - Transfer motion is off in all 435 cloths.
    - The wide swings come from the game's own data. Tomoe hair02 has gravity -0.9 and damping 0.9. Owl haori_new and Chainmail have damping 0.001.
    - The pieces the clip can resolve swing about as much as in the clip: Owl coat median 24 / coat#1 32 deg from vertical, Isshin sleeve 27.
    - The 100+ deg pieces (beard, hair02, haori_new) are too small to read in the clip. Nothing to fix without a closer real-game view.

### 4.56 Deathblow effects: main bosses and regular enemies (2026-10-13)

- **Tool:** `SHINOBI_FFX_LOG` now also logs effects that are skipped (`ffx <id> skipped: no fxr / look` or `no dummy <dmy>`) in ffx.rs. Each boss run used `SHINOBI_THROW_TRACE=front` with `SHINOBI_FFX_LOG` and `SHINOBI_GORE_LOG`. Logs are in the session scratchpad `db/`.
- **Main bosses** (front deathblow):
  - Owl c5060: 220505 on dummy 800, Wolf 220502 on 121, 229502 on 10130.
  - Gyoubu c5080: 220505 on 801, Wolf 220502/220503/229500.
  - Butterfly c5090: 220505 on 800.
  - Isshins c5400/c5430: 220506 on 201, Wolf 671105.
  - Demon c7020: 220505 on 241.
  - Genichiro c7100: 220505 on 90, Wolf 671105.
  - Tomoe c7110 (ThrowDef13700): stains only, Wolf 671100/671101.
  - Emma c7400: 674020 on 90.
  - Monkeys c1250 (ThrowDef14500): no blood in that anim's TAE. Its other deathblow anims carry 220505/220506 gated on 964 (SpEffect 200080 "normal fluid", NpcParam spEffectID6 on 1150 rows) or 255505 on 965 (200081 "illusion").
  - All of these match the TAE data, on the TAE dummies.
  - Not reached by the trace: Monk c5000/c5005, Ape c5100, Dragons c5200/c5300 (their kills are scripted). Their immortal-severing effects need a scripted-kill harness, which is not done.
- **Fixed: 452 + 6 effects were in the archives but never converted.** Param-referenced effects (FootSfxParam floor variants, HitEffectSfx, ThrowDirectionSfx, SpEffectVfx) were in sfx/*.d but not in extracted/fxr. The Demon's deathblow foot effect 103002 was one of them. They are now converted with `fxr_extract.py --ids` (extracted/fxr 1268 -> 1729). fxr tests 13/0. The 102xxx floor row is still in no archive.
- **Regular enemies** (c1010 / c1100 / c1180; front, behind, plunge):
  - Blood spray, stains and Wolf's blade blood follow the TAE in every case.
  - Wolf's a200_510000 has no blade-blood event, and c1180's a229/a201 anims do.
  - The gecko's (c1100) lethal plunge shows no spray. Its 12310 blood is at 2.83 s, but the TAE flag 69 (ThrowType5) at 2.5 s switches it to ThrowDefDeath12311, which has no FFX. That is the game data, not a bug.
- **Clip comparison** (all_finishers, contact sheets in `db/clip_*.png`):
  - The clip shows neck/head mist and spray, a red slash/glow on Isshin's and the Dragon's Mortal Blade finishers, and no full-screen red flash.
  - The 忍殺 mark and floor stains are too small to resolve at 480 px.
  - No mismatch was found that is not bloom or tonemap.

### 4.57 Scripted kills' effects, new-effect check, TAE 117 gap (2026-10-13)

- **Harness (test-side only, no script change):** sim_tests `zz_scripted_kill_effects` (ignored; `--ignored --nocapture`). A test `record_fx` system logs every TAE 96/118 effect start each actor crosses, on the clip the game plays (`sound_windows`, as ffx.rs does). It drives Monk 50000000, Ape 51000000 / 51000100 and Old Dragons 53000000 to their defeat with `break_and_deathblow`. `zz_monk_todome_timing` prints the Monk's Todome state timeline. The Divine Dragon (52000000) is not driven: its kill runs through the lightning reversals, and Bosees has the dragon trees.
- **Results:**
  - Monk: deathblows 220605 / 255605 on 800 (gated on 964 / 965), Event20010 220605 on 803, Event20200 220605 on 800. Wolf: Todome a211_501700 4060 on 220, a211_511700 440000 on 100.
  - Ape: Todome reaction a000_013700 651000 / 651001 / 651002 on 30 / 31 / 26; ThrowDef12090 220506 on 803.
  - Old Dragons: 220506 on 500; Wolf a231_510000 220502 / 220503 / 229500.
- **Finding (script, not changed):** the Monk's Todome reaction a100_013700 is cut after 80 frames (1.33 s) into Event20200. So its own blood 220606 (4.5 s), its six effect-110 events (4.17 s) and seven stains (4.57-5.9 s) never play; only 20200's 220605 does. Wolf's a211_511700 has Event933 at 0 and ThrowAttackBehavior 800 at 1.23 s. The script requests 20200 at about 1.33 s. In the game the 20200 waits for the Todome message, or for flag 69 at 8.47 s, or for the reaction's end (enemy.rs 1000). Routed to the overseer.
- **Todome cut, cause found (Bosees 2026-10-13, no behaviour change):** EMEVD 4[8] "Character Has Event Message" reads chr+0x50 -> +0x50 -> +0x508, written by **TAE 231** (first frame, latched), not TAE 936. TAE 936 is ThrowDirectionDecalParam's decal (unk0 = slot, unk1 = spacing), so `script.rs` TAE_EVENT_MESSAGE = 936 is the wrong event. Wolf's a211_510000 / 510110 / 510310 / 511110 carry 231 = 10; a211_511700 (the Todome) has none, so in the game the 10 is the one latched from his earlier deathblow and 12505964's request comes as soon as the Todome starts, as ours. ThrowDef13700_onUpdate takes the event at once (ExecEventTrandition). **Switched to 231 (Bosees 2026-10-14, overseer order), and the count drop moved to the hit:** see 4.42 "TAE 231 event message". The cut now comes at 1.25 s (Wolf's attribute-5 hit at 1.23 s), not 1.33 s; the clip (all_bosses 1:07:15+, 2 fps) agrees: her red dot goes ~2 s after the lunge and her bar ~7 s in (1.23 s + a100_020200's 6 s), while Wolf plays his 9.83 s Todome out.
- **New effects check:** fxr `zz_listed_effects_build_and_emit` (ignored; `SHINOBI_FXR_IDS=<file>`) loads, builds and runs each effect for 3 s headless, then checks particles, panics and missing fxr_tex textures. Of the 458 new ids, 455 are OK. 103016, 180124 and 180131 are empty in the game file itself (a root with no child nodes). GPU drawing is not covered by this check.
- **Gap found: TAE 117 SpawnFFX_ThrowDirection is not implemented.** Wolf's deathblow / Todome anims use it (a211_511700: 1.23 s on dummy 111, 2.43 s on 100, 4.8 s on 121, all index 11). ThrowDirectionSfxParam rows are hit materials (109 meat: col 10 220500, 11 220501, 14 220502, 15 220503, 16 220504, 20 300001; 114 under armour 229500...; 132 phantom 2555xx; 134 poison 2305xx; 135 white 2315xx). **Ported 2026-10-13 (ffx.rs, `FootSfx::throw_id`):**
  - The row is Wolf's throw target's NpcParam materialSfx1; the column is ThrowDirectionSFXIndex.
  - gap: the exe's row choice is not found. The dispatcher case 0x75 (FUN_140b56d40 -> FUN_1409da8b0 -> FUN_1409d0db0) fills a ChrThrowDirectionSfxSlot (vtable 0x142a1fa30). Its spawn FUN_1409da950 takes an already-final sfx id, and no ThrowDirectionSfxParam getter was found.
  - Live check: c1010 front gives 229501 on 120 and 220503 on 100 (row 114).
  - Test `throw_direction_effects_come_from_the_material_row`. ffx/gore/deathblow tests 36/0.
- **Deathblow look vs the clip (c1010 front, scratchpad dbshots/):** our 220505 reads as tall, thin, dark brown-red strands over a large part of the screen. The finishers clip (0:11-0:17) shows a small, bright red, compact spray from the neck. Two causes are possible, neither proven, so nothing was changed:
  - The lit-particle /8 (no real lighting; next to the paused tone work).
  - The orientation-7 axis for its 603 sprites (textures 10019 / 11606 / 31421 / 51201).
  - Added to section 5 "Live checks wanted".


### 4.58 Lit particles and billboard orientation, ported behind switches (2026-10-13)

- **Lit shading (`SHINOBI_FX_LIT=1`, off by default).** The game's PS_VFX_SingleTextureAllLight / BumpLight (gxffxshader.shaderbnd, disassembled into C:\DeadlockModding\tmp_sh\asm) shade `saturate(tex x colour) x (SH ambient + sat(N.L) x FC_DirLight Col_0 x g_ps_envCbColScale x g_vLightMask.x x shadow)`. There is no 1/8 in any shader.
  - Ported: fxr.rs `lit_at` (1 + sat(N.L) in uv_b.y, N = toward the camera), `SoftExt.light` (ambient, sun on the map's ENV_PER_UNIT scale, from `map::particle_light`), and the fx_soft.wgsl branch. The sprite and GPU-particle paths are lit; models, multi-texture and tracers keep the 1/8 stand-in (`LIT_UNLIT`).
  - Col_0 = the LightSet DiffColor0 (r^2 W ..., as map.rs `colour`). That comes from sekiro-rs SHADER-INPUTS.md, which may need a per-item OK. No separate SFX light scale exists in the drawparams.
  - Not found: the writers of g_ps_envCbColScale (cb0 752), g_vLightMask.x (cb0 208) and g_ps_envCbIndex (cb0 180). The SH is the GIIV + point lights (here the hemisphere ambient). The N.L sign convention is not proven.
  - Result with scale 1: the blood 220505 shows as bright orange-red jets with blown-out white cores (scratchpad cmp_ol.png, bottom). It is closer to the clip's bright red but overexposed. So it stays off until cb0[47] / cb0[13] are read live.
- **Orientation (`SHINOBI_FX_ORIENT=1`, off by default).** The exe's billboard builder FUN_140440a60 (the switch at 14044.c:10384; 600's at 14043.c:9564):
  - 1: the camera rows.
  - 2: the node's rotation.
  - 4: the camera rows, with up = a constant (world up).
  - 6: a look-at rolled to world up.
  - 7: a cylinder about the particle's own vector pfVar10[2..4] (node space), turned to the camera.
  - 3 / 5: one row replaced by a runtime constant.
  - 0 / 8+: identity.
  - The constants DAT_143f083c0 / 085d0 / 08390 are zero in the file (set at run time), and what writes pfVar10[2..4] is not traced (read here as the particle's direction).
  - Result: the blood lies in dark strands sprawling sideways (cmp_ol.png, top). Off until the constants and the vector are read live.
- Default behaviour is unchanged. fxr / ffx / gore tests pass.

### 4.59 FX meshes written in place (2026-10-14)

fxr.rs sprite / distort / multi batches and the ffx.rs weapon trails no longer insert_attribute into the Mesh asset each frame (Bevy 0.19 re-allocated their slabs, 2.7 ms a frame per bug fixes' trace). `FxUploads` pads each mesh to a capacity bucket (3 x pow2, min 192, kept while at least 1/4 full) with degenerate triangles; the asset is only replaced when the bucket changes, else the packed bytes go to `write_fx_uploads` (PrepareAssets after allocate_and_free_meshes), as cloth.rs does. Trails gained NoFrustumCulling (their AABB no longer updates). Tests fxr/ffx/gore 16/0. Live check wanted: effects and trails still draw.
Bench (quick profile, SHINOBI_MAP=sandbox + c7400 + SHINOBI_SANDBOX_ENEMIES=c1020,c1010,c1040, vsync off, SHINOBI_BENCH=10; SHINOBI_FX_REALLOC=1 = old path), two runs each on a noisy machine: old 89.3 / 47.2 fps, p50 10.74 / 11.20 ms, render wait 6.10 / 9.78 ms (2nd run hit a build); new 116.7 / 113.1 fps, p50 8.31 / 8.63 ms, render wait 4.17 / 4.34 ms.

### 4.60 Main-boss FFX coverage (2026-10-14)

Every FFXID in the TAE events of the 14 main-boss chrs (c1260 c5000 c5060 c5080 c5090 c5100 c5200 c5300 c5400 c5430 c7020 c7100 c7110 c7400) has a converted extracted/fxr/<id>.json, except ids already known to be in no game archive: 2255505 (c1260), 654061 (c5400/c5430), and the placeholder 2 (c5000, 12 events). So no boss effect is missing for lack of data; what is left is per-boss look checks against clips (catalog shots).

### 4.61 Boss effect catalog: Tomoe a000_003085 (2026-10-14, in progress)

Run: `SHINOBI_MAP=sandbox SHINOBI_FIGHT=c7110 SHINOBI_AI_ATTACK=3085 SHINOBI_CLOTH_PROBE=1 SHINOBI_CLOTH_SHOTS=<dir>:<from>:<to> SHINOBI_FFX_LOG=1` (shots at 10 fps; kill sv1 by PID). All TAE effects spawn on time: 671120 (0.03 s), 671121 (0.5 / 2.05 s), 671122 + 671123 (1.88 s) on dummy 300, plus 410 on dummy 8 and the floor dust. Dummy 300 follows the blade (0.5 m -> 2.6 m high through the overhead swing), so the anchor is right. Look: thin, faint white-blue crackle along the raised blade, no big burst at 1.88 s. That is the same pattern as the 3042/3043 lightning (4.48-era note: the game's tonemap/bloom whites it out), so it is parked with tonemap/bloom. A 1 fps sheet of the clip (1500-1696 s) could not pin the 3085 moment; listed for the live session.
- Tool: scratchpad shoot.sh <chr> <attack> (above run + contact sheet; finds the new sv1 PID and kills only it). Clip chapter times: the Genichiro fight is at 1457-1510 s (umbrella), Tomoe 1560-1600 s; 24-288 s is not Genichiro.
- c7110 3043 (lightning cast) vs clip 1569-1572: every effect spawns (671026-29, 671032, 101142). Ours: huge vertical beams over the frame and a 0.6 s full-screen cream flash; the real one is a radial starburst on the blade/body (~3x height), a blade-tip glow ~0.6 s, arcs ~2x height, black debris. Brightness/flash size is tonemap/bloom (parked); the beam shape vs radial arcs and the missing debris need the live check / a closer look at 671026's emitter (gap).
- c7110 3043 with SHINOBI_FX_ORIENT=1 (4.58 port): fewer, thinner vertical beams, then a dense radial web of branching arcs round the impact with a scorch, and no full-screen white-out. Closer in shape to the clip than the default. Evidence for the orientation port; it stays gated until the constants are read live (4.58).
- c7100 3045 (671110 on foot 420 at 0.43 s, judge 0.47): a quick stomp, not the clip's leap-in at 1500 s (bright flash + beige dust 1.5-2x height). Which anim the leap-in is, unknown; its dust is likely SpawnFFX_ByFloor by the floor material (snowy roof), which the sandbox floor does not have. Clip-only.
- c7100 3020 (612210 on dummy 40 at 0.92 s): a small orange flash on the body; 3008 (671002 on dummy 30 at 0.98 s): a small orange ember at the feet. No clip moment found for either; clip-only.
- Clip ranges (corrected by the clip agents): Butterfly 880-997 (deathblow ~998), Isshin Sword Saint 5258-5357, Guardian Ape 2032-2150, Headless Ape 2254-2352, Monk illusion (Mibu) 2686-2705, Great Owl 3684-3780, Owl Father ..-3963, True Monk 3971-4075, Divine Dragon 4691-4752, Demon of Hatred 4905-5025. 10 fps strips in scratchpad cat/clip_*.
- Isshin (c5400) starts his attacks at ~12.9 s in SHINOBI_FIGHT (walks in), not 7 s; shoot 12.8-18. 3043 (spear lightning 653500/653511) never plays with SHINOBI_AI_ATTACK in phase 1 (he plays 3100/3101 instead): the phase-3 anims need the phase (gap for the shot tool).
- c5400 3006 (654112/654110/654111 at 2.98-3.45 s on dummy 260): all spawn; a white-grey arch with a thin arc rises in front of him for ~0.4 s. The clip's crescent (5289) is violet-white and lasts ~1.5 s. The data has no violet: 654110's tracers (10012) are color1 black->0 (node 0, blend 2) and white x rgbMultiplier 0 / alphaMultiplier 3 (26031), textures white. So the violet is not from these rows (the player's own blade glow in the clip, or tonemap); clip-only. 3011 (654150/654151, a 0.375 m tracer, white): not visible in our shots, the X in the clip is violet; same question. Tracers draw the dummy's path, so SHINOBI_FXR_TEST (static) shows them as hairlines: not a test for them.
- Great Owl (c5060): attacks start at ~14.2 s in SHINOBI_FIGHT. Spawn check only: 3033 (650671 on 20 at 1.45 s, 650672 on 220 / 650673 on 211 at 1.32 s), 3034 (650676 on 220 at 0.33 s), 3039 (650680 on 90, 650682 on 7), 3048 (650685 on 20); 3030 did not play. The shots (6.9-14 s) end before the attacks, so no look comparison yet: re-shoot at 14-21 s.
- **Stopped here 2026-10-14 (user, via the overseer).** Done: Genichiro / Tomoe, Isshin, Butterfly (look checks above); Owl spawn check only. Not started: Owl look, Owl Father, Monk, Ape / Headless Ape, Demon of Hatred, Divine Dragon. Ready clip strips (10 fps, scratchpad cat/): clip_genichiro (6), clip_isshin_butterfly (11), clip_owl_monk_ape (24: Owl, Owl Father, Ape, Headless Ape, Monk illusion + true, Demon, Dragon). Also see docs/kb/clips.md's measured fight moments (bug fixes) before seeking.
- Butterfly: the clip shows only kunai glints and deflect sparks; her illusions / butterflies never appear in it. Nothing to compare.
- c7110 3027 (671133 / 671142 / 671144), 3049 arrows (671055 x5): all spawn on time; no clip moment found yet (arrows not seen in 1460-1600 s at 1 fps).

### 4.62 Fable handler round 1: boss bug hunt + clip comparison (2026-10-14)
One chat does every track now (user: "you will be my fable handler ... focus on the hard tasks first";
Bosees / VFX / bug fixes / overseer stay stopped). Build dirs `target/fable` (traces) and
`target/fable2` (edit/test). Suite at the end: **554 passed / 0 failed** (quick). Release built.
Fixes, each with a sim test:
1. The perilous kanji goes up on every bullet carrying SpEffect 211000, not only the 980 / 982 /
   983 suffixes: Isshin's 986 sweep (a100_003016), the Way of Tomoe's 986 (a000_003043), the Ape's
   phase-1 grabs (Bullet 51000480). enemy_bullet.rs gate, data.rs `perilous_warning` (the AI's
   perilous answers and the trace counted the Ape's grabs as plain). `the_perilous_kanji_comes_with_every_warning_bullet`.
2. A Bullet row's lightning charge (spEffectId 9490-9495) reaches combat.rs as the hit's charge:
   Tomoe's lightning arrow 71100521 shocks Wolf on the ground (before: a 0-damage hit).
   `a_lightning_arrow_shocks_wolf_on_the_ground`.
3. Boss music merges the cast rows' BGM entries (Genichiro -> Tomoe, Emma -> Isshin): fmod.rs
   `boss_music`. `boss_music_follows_the_phase_flags`.
4. The defeat banner waits only on the characters the ending plays on (dead / ThrowDef / Event20200),
   not on cast members alive by design (the Divine Dragon's trees, Gyoubu's horse): menu.rs
   `finisher_done`. `the_last_deathblow_plays_out_before_won`.
5. A wire on a moving enemy follows it (Gyoubu's drifts, the Ape): wire.rs `track`, player.rs
   decide reads EnemyGrapples each tick.
6. 2003[11] Display Boss Health Bar: script.rs `Act::BossBar` / `Run.bar`; hud.rs shows the
   scripted body's bar (the Old Dragons' 2500810, ninsatuNum 0). `the_old_dragons_bar_is_the_scripted_bar_body`.
7. The Lightning Reversal's shock build-up: status.rs STATE_INFO_FREEZE 260 (ResistRules::freeze,
   from the raw SpEffectParam: registFreeze, freezeDamageRate). 9400 (AtkParam_Pc 5000281) fires
   its cycle 9420: 200 vs resist_freeze 100 fills at once, 67 % + 320 HP x the worn
   freezeDamageRate (Tomoe's 9461: 25 %) = 508 of 2556, 180 + 5 % posture, then 9445 / 9450.
   Before, the reversal took 0 HP. `the_reversal_takes_a_fifth_of_tomoes_bar`. gap: whether the
   posture fields scale with freezeDamageRate; the enemy's reaction still needs the combat.rs
   `dmg > 0` path (DamageLightningStart comes from the spAttribute 6 rule, unchanged).
8. The AI's view of Wolf includes his timed rows (Player::timed: the Owl's seal 3506090, 211000)
   and his lightning charge (Actor::electro): enemy.rs think. `the_ai_sees_wolfs_timed_rows_and_charge`.
9. The Old Dragons' fall is a hand-over, not a defeat: a 93xx flag counts as the defeat only for a
   boss that has health bars (Snap.bars_max). m25 12505885 -> 9318 -> the Divine Dragon phase.
   `the_old_dragons_fall_hands_over_to_the_divine_dragon`.
10. The map script sees a boss in its last deathblow as alive until Wolf's Ninsatsu hit lands
   (Snap dead / hp gated on `ninsatsu_unhit`, as `bars`): Gyoubu's Handle Boss Defeat now 6.6 s
   after the throw's start (a225_510090 judge 5.9 + Wait 0.7; before 0.7 s). gap: the game's death
   is the reaction's ChrActionFlag 12 (Gyoubu a000_012090 8.03 s), not read.
   `gyoubus_defeat_waits_for_the_ninsatsu_hit`.
11. The immortality block no longer zeroes `ninsatsu_unhit` (castle Genichiro's 11115820 ran on
   the throw's first frame). `castle_genichiros_last_dot_waits_for_the_hit`.
12. The HUD's deathblow dots use `ninsatsu_left()`: the last dot goes dark at the hit.
13. **A move from idle turns first (2026-10-14).** c9997 ExecActiveTransition tries ExecTurn
   (enable_turn TRUE from idle / attack / step / damage ends) before the move request (env 3014),
   and the exe's Approach / MoveToSomewhere (FUN_1405caa00, 1405c.c) passes the goal's turn
   target (param 3) to TurnTo (FUN_1405e89f0 -> AI +0x9050); the AI update (FUN_140608990 ->
   FUN_14060c7d0) turns TARGET_SELF (-1) into "face the move point" (+0x73d0, while a move is
   requested) and -2 into no turn. So a point 45 deg or more off plays the turn clip first and
   the run starts from its AI flag 78. enemy.rs: the move arm tries `start_turn` before
   `loco_state` (the AI's `face` move flag). Found by `a_patrol_point_with_a_zero_region_is_reached_by_is_arrived`
   after the blend-in port: the patroller (point 177 deg behind) steered at turnVellocity 135
   deg/s while running 6.4 m/s and circled the point for ever; before the port it had scraped
   past within its 0.4 m radius by luck. Suite 556 / 0 after it.
Trace tool: `zz_boss_moveset_trace` (ignored) with env BOSS=chr:row, SECS, CLOSE (m, default 3.5),
GOD=1 (Wolf's HP held full); the Wolf walks at the boss, guards / answers perilous, attacks every
0.5 s in range. Attacks now include damaging bullets (Butterfly's kunai). Perilous by `perilous_warning`.
Clip comparison (judges on 2 fps sheets of the all-bosses clip; attacks/min are swings in the clip
vs attack anims in the sim, so only the shape compares):

| boss | clip | sim (GOD, 3 min) | read |
|---|---|---|---|
| Gyoubu 50800000 | 31-42/min, 1-2 危 / 72 s, 40-50 % far starts | 47.7/min, 1 % | consistent |
| Monk 50000000 | 40-50/min swings, ~10 % 危 | 18.3/min, 24 % | 危 share high: check the thrust weights |
| Demon 70200000 | 35-45/min, ~2 % 危 | 20/min, 8 % | 危 share high |
| Isshin SS 54000000 | ~32/min, ~20 % 危 | 25.7/min, 5 % | 危 share low |
| Genichiro 71001000 | sparks every 2-3 s, ~15 % | 20.3/min, 15 % | consistent |
| Owl 50600000 (506001) | 60-70 swings/min, 2-4 危 / 65 s | 16.7/min, 0 % | 506001 has 3024 only in the dead Act31/32 and 3090 via Kengeki09 (200226): the sim's clashes never set it |
| Butterfly 50900000 | 30-60/min, 1-3 危, 6 wire visits / 54 s | 6.7/min (bullets uncounted), 0 % | the auto-attacking trace Wolf locks her in guard-counter 3100 / 3024 loops |
| Ape 51000000 | ~30/min, ~15 % 危 | 11.7/min, 0 % (480 grabs uncounted, fixed in 1) | rerun |
| Emma / Isshin Ashina | 11-15 / 18 s; 18-24 + 14-20 | 17.7 / 18.0 per min | consistent |
| Divine Dragon | 8-10 sweeps + 5-7 stabs in 36 s | 7.7/min | consistent |

Rerun with Wolf attacking every 2 s (`ATTACK_EVERY=120`, GOD, 3 min; the 0.5 s Wolf kept the
clash-answer goals busy): Isshin SS 54000000 21 % 危 (3005 sweep via ComboFinal; the clip's ~20 %,
so consistent at a human attack rate), Monk 50000000 22 % (3061 thrust / 3010 sweep via ComboFinal
and ComboTunable), Demon 8 % (3025), Ape 17 % (3016 grab; clip ~15 %, consistent), Butterfly 8 %
(3240 grab, 6 in 3 min; clip 1-3 / 54 s), Owl 0 %. The Monk's and Demon's shares stay above the
clip's: their 危 come as combo finals (ENABLE_COMBO_ATK_CANCEL) that the trace Wolf, standing in
front, never breaks by stepping away as the clip's player does; not a known data gap.

Open from this round: the Owl's and Butterfly's perilous paths (Kengeki rows 2002xx on clash),
the flag-12 death timing, Wolf's shock via `lightning_shock` (unchanged, not the status gauge).

### 4.63 Fable handler round 2: the special clash rows (2026-10-14)

The AI's clash SpEffects 200200-200229 are not applied by the exe (no 2002xx literal in the
decomp): each guard / deflect / bound reaction clip carries its own as a TAE AddSpEffect
(c5060 a000_008500 200216, a000_008502 GuardDamageSpecial 200225, a000_008602/3
AttackBoundEnemy2 200226; c5090's 008602/3 200227/8; c7020's 008600 200205). The sim pushed
an engine row (200215 / 200200 ...) on top, and ReturnKengekiSpecialEffect takes 200215 before
200225, so no "special" Kengeki branch ever ran.

1. `combat::clash_after`: the engine row only when the reaction clip has no 2002xx TAE row of its
   own (additive flinch, a refused reaction); the guard count still counts it (Enemy::clash_count).
2. The bound clip number follows the attacker's BehaviorParam `category` (exe FUN_140b698d0,
   140b6.c:5487-5534: damage type 1008 / 1019 for category 6 -> AttackBoundEnemy1 /
   AttackNoBoundEnemy1, 1009 / 1020 for 7 -> Enemy2, 1010 / 1021 for 8 -> Enemy3; c9997
   ExecAttackJustBound / ExecAttackBound), and a chr without that clip plays no bound (the HKS
   chain ends FALSE). `data::behavior_category` reads extracted/json/params/BehaviorParam.json
   (200000000 + behaviorVariationId * 1000 + judge). Category 7 / 8 boundable attacks: the Owl's
   warp slashes (220-260), the Butterfly's kicks, Gyoubu's charges (8), the Ape's slams, Isshin's
   spear thrusts (8), Genichiro's thrusts (8), the Demon's slams; 24 chrs have the Enemy2 / 3 clips.
   `the_owls_deflected_warp_slash_bounds_into_its_sweep`: a deflected a000_003035 ->
   AttackBoundEnemy2_Left -> TAE 200226 -> 506001 Kengeki09 -> ComboFinal 3090 (the perilous
   sweep) at the clip's flag 23. Before, 0 perilous in every Owl trace.
3. ChrActionFlag 12 "Death" is not the game's death: the exe's type-0 handler FUN_140b51b70
   (140b5.c:7162) has no case 12; death stays HP / state based. The 9.5 s banner: closed in
   item 9 below.
4. The castle Owl's script is 506001 (NpcParam 50600000 is placed with think 50601000 in
   m11_01; 506000 is the Hirata boss 50601010). Its perilous pool: 3090 / 3091 after the special
   bound, 3024 in Act32 and the warp slashes' acts 10 / 49 - none of which Goal.Activate's odds
   table reaches (only acts 1-14, 21-31, 40 get odds; Act31 is nulled by SpaceCheck). So the
   castle Owl of the sim throws no category-7 attack and no perilous at all (traces 0 %), while
   the clip's Owl shows 2-4 per 65 s. gap: either the clip's Owl is the Hirata one (506000 /
   506300, where 3024 / 3004 / 3033 are in the odds) or 506001 reaches those acts by a path not
   read (Common_ActivateAct, the Number-5 timing counters). Worth a user check: does the castle
   Owl ever do a perilous sweep in the real game?
   Traces after the fix (2 s Wolf, GOD, 3 min): Butterfly 12 special bounds (AttackBoundEnemy2),
   8 % perilous (3240 grabs); Gyoubu 9 bounds (3 Enemy2, 6 Enemy3), 1 %; Owl unchanged.
5. The Monk / Demon perilous shares above the clip's are combo finals the standing trace Wolf
   never breaks (ENABLE_COMBO_ATK_CANCEL); Isshin SS at a 2 s attack rate matches (21 % vs ~20 %).
6. HUD leftovers: the boss posture glow's state 3 (MENU_Taikan_ST3) shows during the break
   (Enemy::is_broken; the HUD movie's State_3 holds with ChrActionFlag 0x160). The counters are
   the game's goods icons with the count after them (sekiro-extract `hud` cuts them:
   export::hud_icons; the ids come from アイテム名.fmg, the goods rows carry no names).
   **Layout from the HUD movie itself (second pass):** the main timeline of menu/01_000_fe.gfx
   (uncompressed GFX, SWF tags; scratchpad swfnames.py / swfrevive.py list ExportAssets names and
   the PlaceObject matrices per DefineSprite) places every element on the 1920 x 1080 stage, so
   the earlier clip guesses were replaced: PlayerCount (1849.9, 994.6) = the Spirit Emblem count
   (zanki_ari_symbol = MENU_ItemIcon_00001 at 0.126 from x 1750, y 984) bottom right; Item_Right
   (1838, 985, scale 0.42) = the quick item, our Healing Gourd with its stock; SkillPoint_282
   (1484, 88.4) is the top-right element the clip showed (MENU_SkillPoint 70 x 47 px carrying the
   point count, then the 1482-1747 experience bar) - not the emblems; Money_164 (1702, 748.5) is
   the right-middle counter (MENU_Money 33 px, a dark backing, the Sen total right-aligned);
   PlayerHUD Gauge: HP at (200.9, 982.7), SP (posture) at (960, 937.5) x1.3, Resurrection2's
   ResurrectionList Icon_0..2 at x 129.8 + 46.8 i, y 938.3 = MENU_Revive_Window_2 at half size
   with MENU_Revive_GaugeMax over a charged node (hud.rs res_node_bundle, ResLit; the coloured
   squares are gone); Equip Item_0_0..13 (120.5, 1013.9, 36 px apart) are the status icons, not
   quick items. The extractor cuts MENU_SkillPoint and MENU_Money too. Sv1 has no experience,
   skill points or Sen, so those two read 0 (gap). Text on bright ground: the counters and the
   boss name now carry a full-black 1.5 px shadow; the movie's backings are drawn at alpha .85
   because the HDR ground washes out lighter ones. gaps: the recharge needle / gauge fill and the
   Temp node, the quick item's stock placement (the movie's Stock text box is not decoded), the
   MENU_HP_bar_mark caps and glow layers of the experience bar.
8. **Blend-in root motion, ported.** The exe's Havok blending transition
   (hkbBlendingTransitionEffectUtils::generateInternal FUN_14140cfc0, 14140.c:9700 ->
   FUN_1413bf6c0, 1413b.c:11271) mixes the world-from-model as from x (1 - w) + to x w, w =
   FUN_141408650(curve, t) with curve 0 = 3t^2 - 2t^3 (1 linear, 2 t + t^2 - t^3, 3 2t^2 - t^3);
   every c0000.hkx effect has blendCurve 0 (sekiro-extract `transitions`). The effect flags are
   Havok's: 1 = FLAG_IGNORE_FROM_WORLD_FROM_MODEL (TaeBlend_NoSrcMotion*: the to generator's
   root at once), 2 = SYNC, 4 = IGNORE_TO (StateToStateBlendIgnoreToWorld). `sekiro-extract sms`
   now prints the effect per transition (hkx.rs transition_list, TransitionInfo @32); tallied
   per target state into extracted/blend_src.json (1281 states: 932 keep the source motion, 349
   drop it; data::blend_keeps_src, unknown states keep it). The blend length is the to clip's
   TAE Blend (anim.rs already fades the pose by it). actor.rs: Actor.blend_from / blend_from_vel
   / blend_t / blend_len (play() records the from clip and its clock, or the last applied
   move_vel for a procedural state - the action starters zero move_vel before they play, so
   `last_move_vel` is what advance applied); advance() steps the from clip on and applies
   to x w + from x (1 - w) to the step and the yaw. Live check (scratchpad rec_blend.py /
   rec_blend2.py over every rec_* recording, 2727 clip switches with a moving to clip): the
   medians of live / unweighted-clip over the first 6 ticks are 0.7-1.0 for blend lengths
   0.1-0.3 s, far above the to-only weighting (2-10x short) - the from motion continues: an
   attack out of the run loop moves 0.26-0.45 m in 0.1 s (the clip alone 0.094), out of idle
   0.03 (the weight x 0.094 = 0.018). Tests `an_attack_out_of_a_run_keeps_the_runs_momentum`,
   `the_head_kick_relaunch_goes_straight_up` (unchanged). A looping from clip (c0000.hkx
   hkbClipGenerator mode 1 MODE_LOOPING, 247 of 2566: `sekiro-extract clips`, ALL_CLIPS=1 ->
   extracted/loop_clips_c0000.json, data::loops_c0000) wraps its clock through the blend (the
   run loop's tail plus its head; test `an_attack_at_the_run_loops_wrap_keeps_the_momentum_too`,
   presses spread over a loop length). gaps: the enemies' graphs carry no loop flag (c9997
   CustomManualSelectorGenerator), so their from clips still stop at their end; the Release
   clips (live 0.7x the unweighted root out of the hold clip)
   are not explained - their transitions may be TaeBlend_Continue with a clip start time;
   enemies use the default (keep) since their states are Ez labels, not graph state names.
   Fallout (fixed the same day): a clip restarted on its own key is the generator looping
   (hkbClipGenerator MODE_LOOPING), not a transition - play() skips the blend-in then (the guard
   walk's 40-frame loop was losing its step); the guard walk's four directions are layers of the
   one DeflectGuardMove state (DeflectGuard_SM id 30, an hkbLayerGenerator), so a direction
   change clears the blend (player.rs). Reaches out of a standing start shrank (kb/damage.md
   table re-run: Wolf 4.8 -> 4.2 m, Ogre 5.5 -> 5.25, dog 4.0 -> 3.5) and the run stop now
   carries the run's momentum, so `a_cautious_enemy_listens_to_a_new_sound` runs 12 ticks (was
   20, which left Wolf 3 m away, outside the landing sound's 2 m). Full quick suite 556 / 0 (the
   fmod::tests::sounds_pause_and_slots_stop_off_the_playing_screen test failed once in an earlier
   parallel run and passes alone: FMOD timing, flaky).
9. **The boss's death moment for the map script.** EMEVD 11105800 decoded (scratch
   boss_events.emevd_events): OR(has SpEffect 201000, dead) -> set flag -> IfCharacterDead(MAIN)
   -> Wait 0.7 -> Handle Boss Defeat. The exe's dead test is HP <= 0 with an SpEffect guard
   (FUN_140bd5d40, 140bd.c:3239; conditionHp == 0 and a negative changeHpRate / Point keep a chr
   "alive"), SetHp (FUN_140bd64e0) clamps an immortal chr to 1, and the only unclamped writer is
   the kill routine FUN_1409fa0c0 (1409f.c:6930: FUN_140bd5590(hp 0), physics off, vtable
   +0x168(1)); kill helpers FUN_1409ee7e0 / ee830 / ee8a0 set chr+0x1a11 |= 0x80 (callers: chr
   message 6 at 1409f.c:7354, event 0x1b at 1409e.c:10098). So the throw reaction kills him
   late, and the one marker at that time is its ChrActionFlag 12 "Death" (Gyoubu's a000_012090
   f241 = 8.03 s: + 0.7 + the banner's fade-in = the clip's 9.5 s). script.rs's snapshot `dead`
   now waits for that flag's start when the reaction has one (else the Ninsatsu hit as before).
   Test `gyoubus_defeat_waits_for_the_ninsatsu_hit` now expects 8.5-9.5 s. gap: the flag-12
   consumer (who calls FUN_1409fa0c0 - probably through the HKS / TAE path) is not located.
10. The Dead screen timing: no source. Every Medal recording was checked (contact sheets): the two
   long ones are desktop / other games, 20261011222656376 is Sv1 itself, 20261010013834205 has no
   death; the ongbal clip's "death screen at 6:40" is a red-smoke edit transition with an item
   card, no 死. menu.rs keeps the 2.5 s fade + MENU_Death_02 as a gap until the user records a
   death in the real game.

## 5. Open work, in rough priority
- **Live checks wanted** (the real Sekiro running, memscope; the overseer collects them for one
  user session):
  - **guardCutCancelRate (bug fixes, round 75).**
    - Setup: Wolf blocks (no deflect, guard held) one plain physical hit from an ordinary NPC
      whose AtkParam_Npc row has guardCutCancelRate -30 (2017 of 2396 rows; e.g. an Ashina
      soldier's sword swing).
    - Read: Wolf's HP before and after the block. The player ChrIns -> the HP field the existing
      memscope scripts already read.
    - Expect: no change = no leak (our model). About 30 % of the atkPhys gone = the exe applies
      (rate + 100) / 100 to Kusabimaru's 100 % physGuardCutRate.
    - Static side: getters FUN_140a12d60 (NPC) / FUN_140a24ba0 (PC); the caller (a vtable slot)
      is not found yet.
  - **Wolf's deathblow blade blood, TAE 117 row (VFX and Maps, HANDOFF 4.57).**
    - Setup: a front deathblow on an Ashina soldier (c1010, armoured: materialSfx1 114) and on a plain bandit or monk (meat, 109).
    - Read: the FXR ids spawned on Wolf's sword during a200_510000 at about 0.85 s and 1.48 s. That is the sfx slot list at chr+0x1b40 (ChrThrowDirectionSfxSlot: the id at slot+0x24), or the FXR spawn call FUN_14092afe0's id argument.
    - Decides: 229501 / 220503 on the armoured soldier and 220501 / 220503 on the plain one means our row = the target's materialSfx1 is right. A different id means the row comes from somewhere else, so note which row (ThrowDirectionSfxParam) it belongs to.
  - **Thin boss cloth swing (VFX and Maps, HANDOFF 4.55).**
    - Setup: Way of Tomoe (hair02), Owl (the haori_new tail) and Isshin Ashina (beard). Fight them and film close with the camera near during their dash and leap attacks (the 3000-series), or pause on frames.
    - Read: the angle of those pieces from vertical at the peak of a leap or dash. A clip at 60 fps close-up is enough; the cloth particle positions in the hkx sim buffer are a bonus.
    - Decides: if they pass horizontal (over 90 deg, as in Sv1: up to 160-170 deg), our solver matches. If they stay within about 60-80 deg, something damps them in the exe beyond globalDampingPerSecond (look at the cloth's maxVelocity / the sim's collidable inertia).
  - **Deathblow blood look, FXR 220505 (VFX and Maps, HANDOFF 4.57).**
    - Setup: a front deathblow on an Ashina soldier in daylight, with the camera close. Take a still at about 0.3-1.0 s after the stab.
    - Read: the spray's colour (bright red vs dark brown-red), its size on screen (compact jet vs strands reaching ~2 m up) and its shape. Bonus: the particle colour after lighting, if a frame capture tool is at hand.
    - Decides two things. Our lit particles are drawn unlit at rgbMultiplier / 8 (fxr.rs particle_color); bright red in the game means they need the real light at the particle. Our 603 orientation-7 sprites stand along the node's Y axis; a compact jet means mode 7's axis is not the node's Y.
  - **Lit particle light scale and billboard constants (VFX and Maps, HANDOFF 4.58).**
    - Setup: any fight with a deathblow (blood 220505) in daylight. Attach memscope during the FFX draw.
    - Read:
      - On a lit particle draw: PSCommonBuffer cb0 offset 752 (g_ps_envCbColScale), 208 (g_vLightMask.x), 180 (g_ps_envCbIndex), 556 (g_ps_preExposure) and 560 (billboard shadow intensity).
      - The floats at 0x143f083c0, 0x143f085d0 and 0x143f08390 (the billboard builder's constant axes).
      - For one mode-7 particle of 220505, the vector at pfVar10[2..4] in FUN_140440a60, and how it compares with the particle's velocity.
    - Decides: the lit scale that `SHINOBI_FX_LIT` needs (then it goes on by default), and whether mode 7's axis is the particle direction (then `SHINOBI_FX_ORIENT` goes on).
  - **The Monk's Todome length (Bosees, HANDOFF 4.57).**
    - Setup: Corrupted Monk, Mibu (m25), her third deathblow (the Todome with the Mortal Blade).
    - Read: how long she stays in a100_013700 before a100_020200. That is her ChrIns anim id, or count frames on video.
    - Decides: we cut at 1.25 s (1.33 s before 2026-10-14), so her own blood 220606 at 4.5 s and the stains never play. If the game holds it past 4.5 s, the 20200 request in the script path is early.
    - Update (Bosees 2026-10-14): the all-bosses clip already fits the 1.25 s cut (4.57); this check is now only a confirmation.
    - Also read (memscope, Bosees 2026-10-13): Wolf's ChrIns -> +0x50 -> +0x50 -> +0x508, the event message EMEVD 4[8] reads, during a211_511700. Expect 10 latched from his earlier deathblow (a211_510000 TAE 231 = 10) and no new write in 511700 (it has no TAE 231). If it is 10 from the stab's first frame, the game cuts her at the same point we do and nothing is wrong; if it is 0 / other, the message is cleared per throw and the 20200 waits for something else.
  - **An ending FFX's sound (bug fixes, round 84).**
    - Setup: equip the Flame Vent (Spring-Flame or Lazulite, so the a072_400400 loop runs) and hold its use for 3-4 s next to a wall, with the music low.
    - Read: by ear, whether the flame "whoosh" (sound events s000300161 / s000300261, FFX 300161 / 300261, each sample ~1 s+) restarts cleanly on every 1.13 s loop or piles up into a growing roar. Bonus (memscope): the SoundIns state byte at +0x12 of the FFX's sound host entries (GXFfxSoundHost at ffx+0xb8, 14040.c FUN_140406010) going to 5 when the effect ends.
    - Decides: clean restarts = our new stop (sound.rs, slot -1 tracking) is right. A pile-up means the game lets them play out; revert to the untracked f.play for unslotted FFX sounds.
  - **The air-velocity blend's clock (bug fixes, round 81, low priority).**
    - Setup: Sakura Dance or Mist Raven (TAE 922 launch) hitting an enemy right at the start of the hop, so a hit stop lands while the hop's blend is easing in. Film it at 60 fps or step frames.
    - Read: whether the hop's height and arc are the same with and without the hit (hitting vs whiffing the same art from the same spot).
    - Decides: same arc = the blend runs on the character's hit-stop clock (our round 81 choice, player.rs `vs.elapsed += a.clock(dt)`). A visibly shorter or flatter arc on a hit = it runs on real time; switch back to the raw dt.
  - **ThrowParam throwFollowingType (bug fixes, round 94).**
    - Setup: two front deathblows that both use atk dummy 266, one on a row with throwFollowingType 1 (most front rows, e.g. the Ashina General's) and one with 0. Then a deflect-break (246) or a Mikiri (523), all type 1.
    - Read: the distance between Wolf's and the enemy's root positions every frame through the throw (the ChrIns positions memscope already reads). The existing record (player.rs ~1112) shows a type-1 front throw at 1.50 -> 0.85 -> 2.35 m after the snap.
    - Decides: if type 1 keeps the pair locked to the dummy every frame (no drift) while type 0 drifts, follow_throw must re-snap each frame for type 1; today it snaps once and lets go for every type. The same drift on both = the field means something else (note what).
- **VFX and Maps open items (2026-10-12, after 4.44):**
  - **PAUSED 2026-10-13 by the user ("stop the work on map tone and lighting for now"):** B4,
    map emissives, sky, GIIV and bloom below wait until the user resumes them.
  - **B4 brightness / warmth vs the real game:** 2026-10-13 compared against the all-bosses clip's
    outskirts-gate frames on the exported `look_m11_00_gate` (4.44 table): 18 h exposure and
    warmth match; the dark end (~2 stops lifted) and saturation (about half) are left, with no
    Sekiro gparam field to tune by. 2026-10-13 traced: the curtain (m11_camp_curtain_*_a) is a
    plain M[AMSN_light]_e (MetallicValue.spx) whose texture is itself brown (mean sRGB 97 / 79 /
    57, sat 0.41), and the game's combine GXLightCmbAlbedo is `irradiance x GBuffer2` with no
    other term (gxrenderershader; GBuffer2 = sat(albedo x vertex colour) x DifMapMulCol x
    InstanceData.aColor, kb/map.md 645), so its vivid orange in the real frame is light and
    post, not the material: B4 waits for a real frame at a known hour (ask the user).
  - **Map emissives (new, 2026-10-13):** no map material emits light in Sv1. The gate's 60 m
    (MAP_LOG scan) has ~110k triangles of emissive MTDs: M[Multiple]_Emissive (m11_00
    Emissive_wood1 / roof2 / roof_thatch1_e / strawsheaf1: charcoal embers, color_3 / color_9
    orange x 3-7, painted by the vertex blend weights TEXCOORD5 z / w against height masks,
    scrolling ember masks; GXFT_SATStaticSwitch 33, shaderbdle `M[Multiple]_Emissive_33_Gbuf`)
    and M[Emissive]_Simple (Emissive_simple1, EmissiveCloth_simple: decoded, kb/map.md "Map
    emissives"). The game writes them to the fifth GBuffer target (o4, x SC_PreExposure).
    a port is the M[Multiple] game path plus the emissive layer. 2026-10-13 checked (scratch
    tools scratchpad/burnt/msb.py, ev.py): the burnt (x9xx) and intact (x8xx) castle pieces
    share positions and differ only in the part's EntityGroupIDs (entity data +0x1C, 8 x i32;
    msb.rs doesn't read them). m11_01 event 50 calls common events 20004110 / 4112 / 4114
    (activate map part / object / hit group) per group with flags f1-f4 and states s1-s4: the
    group is hidden while any non-zero fi == si. Flags: 8306 prologue, 8301 invasion, 8302 the
    burnt castle (event 911 after 9310). First visit (all off) shows 1117200 / 1201, 1116200 /
    1201, 1114820, 1114330 and hides 1117202-4, 1117210, 1116202-4, 1114310 / 11, 1114821,
    1114400, 1114501: the burnt and invaded sets. So the embers don't show in our arenas'
    state (skip them), and the exporter draws both sets today: fix = read EntityGroupIDs and
    drop parts in hidden groups (parse the map's emevd calls; only m11_01 checked).
  - **Sky 18 h look:** the game's SkyDome_Ver2 shader decoded and matched 2026-10-13 (4.44);
    cloud drift and the fog on the sky ported the same day; left: the glow target.
  - **GIIV, the rest:** several boxes done 2026-10-13 (4.44, kb/map.md "GIIV boxes": the
    game's box shader, MSB fade / priority, up to 16 cropped boxes per arena); left: the
    burning-hall box's event switch, FadeRate / DiffuseColorScale; characters
    (sekiro_material.wgsl, not this track's file) still take the cube's diffuse.
  - **Fog walls on the other maps: done 2026-10-13:** only boss_50900000 (m10)
    had a wall box baked into its `.hit`; o001400 / o001402 were never unpacked (draw groups
    skipped them first), so those walls had no collision at all. The exporter now takes every
    fog-wall object's hkx box into the gates before the draw-group check (and never draws its
    mesh); m17, m15, m11_02, m25, m10, m11_01 re-exported with it (gates: m17 3 walls, m25 2,
    m15 / m11_02 / m10 1 each, the castle roof its box + h900800). Map and fog-wall tests pass
    (14, incl. every_arena_start_stands_and_walks). Disk: C: had 12 GB free.
  - **Bloom:** could take a little more glow on sky / snow by eye; not tuned (no source value).
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
   skips pairs whose heights don't meet (NpcParam hitHeight). The blend-in weighting of the
   root motion (live 0.040 m in frames 0-6 vs the clip's 0.27) is PORTED 2026-10-14 from the exe
   (4.63 item 8). The `runkick` throw trace no longer sets up (Wolf reaches the enemy before the
   jump); use the sim test instead.
4. **Sound gaps:** the other mixer events (ntc_main, bgm); the per-event "Shahei" (occlusion) parameter (placement, reverb + its 2.6 s easing, deathblow mix and boss music done; room / deep / sd_dimension are read by no event).
5. ~~**Coat second texture layer**~~ done 2026-10-12 (kb/visuals.md "Coat shader").
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

### R97 HUD pass (bug fixes, 2026-10-14)
- hud.rs: boss bars added (an enemy with ninsatsu > 0): the HP bar top left (MENU_HP_base_Boss / MENU_HP_bar_Boss), deathblow orbs (MENU_HP_Boss lit, _base spent), the name from npc_names.json, and the posture top centre (MENU_Taikan_base / _bar, centre-out, State tint, MENU_Sp_Center). Positions are from clip r_000230. A boss gets no over-head bars.
- Wolf: the HP bar is 9.5 vw at x 5.7 %, y 91 % (MENU_HP_base_2 + a tinted MENU_HP_bar). The posture is bottom centre, 27 vw at y 86.7 %, in Taikan art. The bottom-left text is gone. Counters: emblems top right, gourd bottom right.
- gaps: the Taikan ST1-3 glow / Effect / Crash layers and the "<" ">" arrows are not drawn yet. The emblem counter art (icon + bar) is not drawn. The 0 counter at right-middle (x 85-94 %, y 69 %) is missing. The item icons are about 2x too large. Text needs a backing on bright ground.
- Suite: 540 / 2 failed. The 2 failures are not HUD-related (ai observe_slots_follow_the_exe_list, a_growing_bullet_reissues_its_appear_sound 11903311); they come from other sessions' WIP.
- R97b: the boss posture glow (MENU_Taikan_ST1/ST2 by posture_state 1/2; ST3 waits on state 3, a gap) sits behind the bar. Item icons are now 4.5 / 3 vw at right 17 %, bottom 6 % (clip x 78.5-83 %). The 2 failures above were Bosees' mid-edit WIP; Bosees reports them green now.
- R97c (clips, 10 fps strips): the defeat banner fades in 0.4 s, holds 2.5 s and fades out 0.4 s (Gyoubu 忍殺 at 12:16.8). The kanji is centred over y 24-52 % with the name in small caps under it. Banner 5 now uses MENU_Fusigiri. Test the_defeat_banner_fades_as_the_clip_does, mutation-checked. Mini-bosses get the same top-left bar block as bosses (Shigenori), which matches the ninsatsu > 0 rule. The bars vanish at the fatal deathblow with no fade (as ours do). The 危 shows 0.6-0.8 s on the chest (ours is 0.8 s).
- R97d: the banner now waits for the boss's death and Wolf's finisher to play out (menu::finisher_done, FINISHER_CAP fallback), as in the clip (Gyoubu: 9.5 s after the fatal deathblow). Won waits for the banner to finish (BannerDone).
- R97d: the HP damage trail, measured at 60 fps on Gyoubu (4 trails): it holds 0.5 s from the first hit, later hits do not restart it, and it collapses in about 0.05 s. The colour is about RGB 165,130,120. This is now on both Wolf's bar and the boss bar (hud::step_trail). Test the_damage_trail_holds_then_collapses, mutation-checked on the hold and the collapse.
- The combat_flow clip is HUD-off, so these stay open.

## 5. Live checks wanted (HUD, R97)
- Over-head bars on regular enemies. Setup: an Ashina soldier, alert it, hit it once, step away. Read: when the bars appear, how long they stay, any fade, the offset over the head. Decides: update_enemy_hud show rule.
- Found-mark hold/fade and the posture "<" ">" arrows. Setup: be spotted by a soldier; damage a boss's posture. Read: the times; the arrow placement. Decides: update_stealth, the boss posture art.
- The Dead screen. Setup: die once. Read: the fade and the 死 timing. Decides: the menu Dead screen (2.5 s fade data).
- R97e: the over-head bars now follow the all-bosses clip (Folding Screen Monkeys, 30:59): HP above posture, both 6.8 vw wide, 5 / 4 px at 1080p, 2 px gap. The posture "< >" arrows show at the bar ends while posture > 0 on both bars (Butterfly 15:52). They are not drawn yet, so that is the next HUD item. The clips.md chapter starts from ch.9 run about +1:00-1:15 late, back near 0 by Shirafuji (32:20).
- Live checks still wanted: when the over-head bars hide and whether they fade; the found-mark hold; the Dead screen (no death in either clip).
- Suite: 544 passed, 1 failed (the_corrupted_monks_last_deathblow_is_her_todome, Bosees' TAE_EVENT_MESSAGE WIP, acknowledged by Bosees).
- R97f: the posture "< >" arrows (MENU_Taikan_Arrow, the left one flipped) sit at both posture bars' ends. The boss's show while posture > 0; Wolf's go with his bar. Fixed the sprite bars drawing as hairlines: Bevy's NodeImageMode::Auto kept the texture's aspect ratio, so every bar image is now Stretch. The boss name is 15 px, and the gourd counter moved clear of the icons. Checked with SHINOBI_HUD_SHOTS (c7100).
- **STOPPED 2026-10-14 by the user (via the overseer) after R97f.** The tree compiles. Suite: 544 passed / 1 failed (the Monk todome test, Bosees' WIP). The clips.md fight moments were added (the chapter starts are not shifted: the chapters include the walk-ins and cutscenes).
  Next, when resumed:
  (1) The Dead screen timing from the Medal recordings (C:/Medal/Clips/Screen Recording, 20261007 / 20261010; the search was stopped unfinished). Then the menu Dead screen: the 2.5 s fade + MENU_Death_02.
  (2) The sound gaps: the mixer event parameters besides sd_reverb_size, the reverb's 2.6 s easing, and a Shahei default for open arenas (the re-scout was stopped before reporting).
  (3) The camera gaps (section 4 item 6).
  (4) HUD leftovers: the emblem counter art, the right-middle 0 counter, the posture state 3 / ST3 glow, and the text backing on bright ground.
