# Sv1 handoff: everything a new model needs to continue (updated 2026-10-10)

Read this first, then `docs/HANDOFF_PLAN.md` (file map, commands, tools, task packets) and only
the `docs/kb/<topic>.md` file for the topic you work on. `docs/PROGRESS.md` is the short overview.

**Where to start (2026-10-10):** section 0 (what the last session did), then section 7 (the
user's open bug list: only the cape is left) and section 8 (the skills plan, in progress: next is
the Flame Vent, then the other prosthetic tools, the combat-art audit and the latent skills).

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
  leading `^/` does not match in Git Bash (path conversion) - leave it out.
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

**Next steps:**
1. **Switching (S):** `config.toml` `[player]` gets `prosthetics = [70000, 71000, 72000]` (3 slots,
   like the game) next to `combat_art`; a key cycles the slot (default PC binding from
   `DefaultKeyAssignParam00..04` - find the "switch prosthetic" row; else a `// gap:` key) and an
   in-game Skills page (debug menu style, `src/debug_menu.rs`) to pick the art, the 3 tools and
   latent skills live. HUD: current tool + art icons (game icons: iconId -> menu atlas `SB_Icon*`,
   rects in `extracted/menu/hi/01_common.sblytbnd.d/SB_Icon*.layout`, cut like `export_hud`).
2. **Combat arts (M):** all 19 (EquipParamWeapon 5100-7700, groups a100-a110, anims already
   exported) use `art_anim` / `art_start_states` / `art_combo_next` in `player.rs`. Test each art
   (sim test per art: it starts, its judges hit, emblem cost) and fill the gaps: 107 / 110 jump
   starts, Mortal Draw sheath stance, Shadowrush lunge to target, Ascending Carp counter, Dragon
   Flash. Emblem cost of arts: find the field (not resourceItemA; check SkillParam / goods).
3. **Prosthetic core (M):** generalise `src/prosthetic.rs` (today: Shuriken only, a070 + bare bullet
   judges) to the equipped tool: anim key = state's anim id in group `a0<group>`, bullets
   "v<var>:<judge>" from `bulletRows` (numShoot fan, child chains, intervalCreate, life, penetrate),
   melee "v<var>:<judge>", emblem cost `emblems`, resident SpEffect while equipped.
4. **Tools (port the HKS branch of each, `c0000_transition.lua` BEH_A_GROUND_SUB_ATTACK at
   line 3523, plus the hold / jump / guard state updates near lines 536-570, 1275, 1371, 1865-1950):**
   in this order - Firecracker 071 (stun: enemy reaction from the bullet's SpEffect / AI), Flame
   Vent 072 (hold start/loop, burn SpEffect), Axe 073 (combo 1/2, shield break), Umbrella 076
   (W_GroundSubAttackGuardStart: a guard state, FLAG_SHIELD_BLOCK exists), Spear 078 (variation,
   pull), Sabimaru 075 (6-hit combo + derive chain, poison), Mist Raven 074 (W_SubAttackJumpAtemiReady:
   counter-dodge, after-damage kawarimi), Divine Abduction 077 (SP_EF_REF_USED_TEKIMAWASHI), Finger
   Whistle 079 (also usable while hanging). Sprint / crouch / air variants per branch.
   Enemy reactions: what each SpEffect does to NPCs (stateInfo) + each NPC's resistances (NpcParam,
   resident SpEffects) + AI script checks (`HasSpecialEffectId`) - via the Lua VM in `src/ai.rs`.
5. **Latent skills (S each):** SkillParam rows -> SpEffects (Flowing Water 280 and Mikiri already
   work). List every row, implement the ones whose systems exist, list the rest as gaps.
6. **Visuals:** the FXR effects are not decoded; per tool ask the user for a real-game clip and match
   it (like `vfx.rs` / `gore.rs`). Sounds already come from the FMOD banks via TAE.

Verify with sim tests (`src/sim_tests.rs`) per tool and art, and screenshots via the throw-trace
shot env vars or `tools/screenshot.ps1`. Nothing committed yet this session: ask the user before
committing (never `extracted/`, never their `config.toml`), never push.
