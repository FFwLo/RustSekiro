# Sv1 handoff: everything a new model needs to continue (2026-10-09)

Read this first, then `docs/HANDOFF_PLAN.md` (file map, commands, tools) and only the
`docs/kb/<topic>.md` file for the topic you work on. `docs/PROGRESS.md` is the short overview.

## 1. The project
- **Sv1** is a 1:1 recreation of Sekiro's combat in Rust and Bevy 0.19, at
  `C:\DeadlockModding\shinobi-combat`. The crate and exe are both named `sv1`.
- **The data is the source of truth:** the user's own Sekiro install (TAE events, HKS scripts,
  params, Havok behaviour graphs and cloth, FLVER models, FMOD sound projects), plus a full
  static Ghidra decompile of the exe (`extracted/decomp/*.c`).
- **Enemies:** c1010 is the Ochimusha (the spear/naginata enemy) and c1020 is the Samurai
  General. `config.toml` sets `[enemy] chr` and `npc_row`.
- **The user's current config is `chr = "c1020"`, `npc_row = 10100000`.** It is the user's own
  setting: never commit it. If you switch `chr` for a trace, switch it back afterwards.
- **GitHub:** `FFwLo/RustSekiro` (public). The history is one root commit with an empty message.

## 2. The user's rules (follow exactly)
**Git and publishing**
- **"dont push anything to github".** Push only when the user says so. No commit this session;
  ask before committing at all.
- The user wants an empty commit message column on GitHub. Whether future commits should also
  have empty messages is an open question: ask.
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
- **Tests:** `cargo test --release` passes 96 tests (11 ignored).
- **Working tree:** many uncommitted changes since the root commit (see `git status`). New files:
  - `THIRD_PARTY.md`
  - `src/ai/` (the vendored Lua 5.0 VM)
  - `src/throw_trace.rs`
  - `docs/HANDOFF.md`

  `src/ai/runtime.lua` is deleted, and mlua was removed from `Cargo.toml`.
- **The user's latest complaints:**
  1. Head-kick camera / character shake (fixes in 4.2; the user hasn't confirmed them yet).
  2. Cloth worse and flickering (fixed in 4.1, unconfirmed).
  3. Sound too loud and different from the game (4.4, unconfirmed).
  4. Vault: "the problem is the enemy, not Wolf" (fixed in 4.5, unconfirmed).

  Earlier, still to check with the user: behind deathblow, Mikiri / air deathblow debug items,
  Shift + deflect lunge (the data says GroundStep_N is a 4 m forward step; does the real game do
  the same?).
- **The user asked for "add all of the stealth system".** Not done yet; see 5.1.

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
1. **Full stealth system** (user request). Build on `src/enemy.rs` `Mode::Unaware` / `perceive()`:
   - awareness indicator HUD;
   - caution/search states (IdleCautionNoBattle, SearchDefault600/610 anims);
   - losing Wolf (NpcThinkParam forget times);
   - crouch/sneak;
   - noise from Wolf's movement;
   - returning to unaware;
   - patrols.

   Check the HKS / AI scripts first: `extracted/ai_src`, `extracted/hks_src/c9997.lua`, and
   the reference repos.
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
