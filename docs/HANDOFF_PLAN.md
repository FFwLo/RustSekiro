# Sv1: work plan for helper models

Lead: Claude (this project's main session). Helpers take one task packet each, work only in
the files their packet lists, and hand back a report. The lead reviews, merges and runs the
game. Give a helper this whole file plus the packet ID ("do task V1").

## 1. What the project is
A Rust + Bevy 0.19 prototype at `C:\DeadlockModding\shinobi-combat` that recreates Sekiro's
combat 1:1. Every number comes from the user's own Sekiro install (TAE events, HKS behaviour
scripts, params, behaviour graphs) or from a static decompile of the exe. Nothing is tuned by
feel when the game has the real value.

## 2. Rules for every helper (read before touching anything)
1. **Never commit, upload or paste anything from `extracted/`.** It is game data. Code only.
2. **No git commits.** Leave changes in the working tree; the lead reviews `git diff`.
3. **Never hand-guess a value the game contains.** Find it in data or the exe, cite where
   (file + line, function address, param row). If it truly can't be found, mark it
   `// gap: <what is missing>` in code and list it in your report.
4. **Static research only.** No Cheat Engine, no memory readers, no running the real game.
5. **Stay in your packet's files.** `src/player.rs` and `src/combat.rs` belong to the lead;
   if your task needs a change there, describe it in the report instead.
6. **Done means:** `cargo build` and `cargo test` pass (run in the project root), and the
   report is written (section 6).
7. Match the code style around you: short doc comments that say where a value comes from.

## 3. Where things are
| What | Where |
|---|---|
| Overview, gaps, knowledge-base index | `docs/PROGRESS.md` (short); details per topic in `docs/kb/<topic>.md` - read only the topic your task needs |
| Game code | `src/` - `player.rs` (Wolf state machine), `combat.rs` (hits, guard, deflect), `enemy.rs` + `ai.rs` (enemy, real Lua AI), `anim.rs` (clip playback, blending), `model.rs` (meshes, skinning, dummies), `camera.rs`, `sound.rs`, `hud.rs`, `data.rs` (loads `extracted/combat_data.json`), `world.rs` (arena, lights) |
| Headless gameplay tests | `src/sim_tests.rs` (`cargo test`) |
| Tunable gaps | `config.toml` (+ `src/config.rs`) |
| Extractor (unpack, params, export, models) | `tools/sekiro-extract/src/` - `main.rs` (commands), `export.rs`, `flver.rs`, `hkx.rs`, `fsb.rs`, `param.rs` |
| States exported from the behaviour graph | `tools/sekiro-extract/export_states.txt` |
| Pipeline script | `tools/extract.ps1` |
| Wolf behaviour scripts (decompiled HKS) | `extracted/hks_src/c0000_transition.lua` (all decisions), `c0000_define.lua` (constants, SP_EF_REF_* ids), `c0000_cmsg.lua` |
| Enemy reaction script | `extracted/hks_src/c9997.lua` |
| Enemy AI (decompiled Lua) | `extracted/ai_src/` |
| Unpacked game files | `extracted/chr/` (chrbnd/anibnd/behbnd), `extracted/parts/` (Wolf equipment), `extracted/tex/` (DDS), `extracted/json/params/*.json` (every param table) |
| Exe decompile (Ghidra, every function) | `extracted/decomp/*.c` (`// @ <addr>` before each function); vtable/class map `extracted/rtti_vtables.txt` |
| TAE event layouts | `tools/sekiro-extract/defs/TAE.Template.SDT.xml` |
| Param layouts | `tools/sekiro-extract/defs/paramdef/*.xml` |

### Commands
```
tools/t.sh [filter]                          # quiet cargo test - ALWAYS use this, never raw cargo test output
python tools/anim_info.py <state> | --ref N | --event T | --state Name     # TAE/ref/flag look-ups
python tools/model_info.py [name] [--meshes] [--dummy N]                    # exported model summary
tools/fn.sh <addr> [pattern] | tools/fn.sh callers <addr>                   # exe function look-up
powershell -ExecutionPolicy Bypass -File tools/photo.ps1 [-Out sheet.png]   # 4-view contact sheet - use for every visual check (cheap)
powershell -ExecutionPolicy Bypass -File tools/relaunch.ps1 [-Shot out.png] # rebuild + run + log lines
cargo test                                   # all gameplay tests (project root)
cargo build --release                        # game exe: target/release/sv1.exe
tools/sekiro-extract/target/debug/sekiro-extract.exe export extracted          # rebuild combat_data.json
sekiro-extract unpack "<Sekiro dir>" extracted '<regex on archive path>'      # e.g. 'parts/fc_m_0200'
sekiro-extract model <flver> <tpf|-> <out.bin> extracted/tex                    # FLVER -> game model
sekiro-extract selectors <hkx>     # behaviour-graph selectors: variable -> child clips by index
sekiro-extract objects <hkx> <HavokType>   # dump every object of a type (fields, arrays)
sekiro-extract clips|transitions|blenders <hkx>
powershell -ExecutionPolicy Bypass -File tools/screenshot.ps1 -Out shot.png   # while the game runs
rg "<pattern>" extracted/decomp           # exe research; gamedb at C:\DeadlockModding\tools\gamedb\target\release\gamedb.exe -r extracted/decomp
```
Sekiro install: `C:\Program Files (x86)\Steam\steamapps\common\Sekiro`. Build the extractor
with `cargo build` inside `tools/sekiro-extract`.

## 4. Task packets

Size: S = an hour or two, M = half a day, L = a day or more. "Owns" = the only files you edit.

### Visual track (models and animations)

**V1 - Normal maps (M).** Models look flat: only albedo (`*_a`) textures are exported.
- Do: export `*_n` textures from each TPF (`flver.rs` `tpf`, `main.rs` `export_model`), write the
  normal-map name per mesh into the model .bin (bump the SHMD version, keep v2 loading), generate
  tangents in `model.rs` (`Mesh::generate_tangents`) and set `normal_map_texture`. Sekiro normals
  are usually BC5 two-channel; load them non-sRGB and check whether Y needs flipping.
- Owns: `tools/sekiro-extract/src/flver.rs`, `tools/sekiro-extract/src/main.rs` (export_model only), `src/model.rs`.
- Done: before/after screenshots of Wolf and the enemy in the report; models still load.

**V2 - Wolf's hair, brows and lashes (S-M, research first).** Wolf's head is `parts/fc_m_0200`
(EquipParamProtector 100000, equipModelId 200). Its TPF only has `heada_a` and `damage_a`; hair,
fur, lashes, eyes and mouth use textures from a shared pack. Those meshes are skipped today
(albedo "-" in `flver.rs guess_albedo`).
- Do: find the pack holding `fc_m_0200`'s other textures (search the unpack dictionary for
  shared face/hair texture bnds: `parts/`, `other/`, `chr/` common tpfs). Map each mesh's MTD to
  its texture, alpha-tested. Unhide the meshes.
- Owns: `tools/sekiro-extract/src/flver.rs` (guess rules), `src/model.rs` (alpha mode for hair/fur).
- Done: Wolf's hair dark with a visible topknot and brows; screenshot.

**V3 - Scabbards and cloth in bind pose (M). DONE by lead 2026-10-07 (src/cloth.rs; see kb/visuals.md). Open: cloth collisions, bend sets.** The enemy's scabbards, and some of Wolf's
straps, stick out because their bones aren't in the animation skeleton (game drives them with
cloth/physics: `*_c.clm2`, `*_c.hkx` next to each FLVER).
- Do: list the FLVER nodes that aren't animated bones and are weighted by visible meshes. As a
  first pass, parent them so they keep their bind offset relative to the nearest animated
  ancestor (no new rotation), which hangs scabbards correctly. Optional later: read
  `*_c.hkx` ragdoll/cloth constraints for a simple pendulum.
- Lead findings (2026-10-07): c1020 has 126 animated bones and 346 FLVER nodes (220 not animated).
  `Sheath01/02[omit]` are children of Pelvis (so they already follow the hips); `Ctrl_Sheath01/02`,
  `Ctrl_R_Katana_*`, `Collidable_Sheath*` and `katana_*` are root-level (parent -1) helper nodes.
  Most root-level nodes (`c9520_*`, `C1020_*`) are mesh-object nodes at the origin, not bones.
  In a full-res crop (`tools/screenshot.ps1 -Crop 0.33,0.12,0.36,0.62`) one long pale blade sticks
  out sideways at hip height; first find which mesh/bone that is (`python tools/model_info.py c1020 --meshes`).
- Update (lead, later 2026-10-07): the duplicate blade is solved (model.rs hides in-hand blade groups on the
  bone without attack dummies, and applies TAE draw masks). What remains is physics: the `Sheath01/02[omit]`
  bones lie flat across the hips in bind pose. `extracted/chr/c1020.chrbnd.d/c1020.HKX` holds
  `hknpRagdollData` (19 bodies, boneToBodyMap, bodyCinfos with mass / motion properties) and constraints:
  `hkpRagdollConstraintData` x14, `hkpLimitedHingeConstraintData` x4 (bodies named "Sheath0*" appear in it).
  Dump them with `sekiro-extract objects <hkx> hkpLimitedHingeConstraintData` / `hknpConstraintCinfo` /
  `hknpPhysicsSystemData::bodyCinfoWithAttachment`. Goal: a per-frame hinge pendulum (gravity, the hinge axis
  and min/max angle from the constraint atoms, some damping) for the [omit] bodies' bones. Check with
  `tools/photo.ps1` (1_back shows the sheaths).
- Owns: `src/model.rs`, `src/anim.rs` (only the non-skeleton node handling).
- Done: scabbards hang along the hip in the screenshot.

**V4 - Upper-body aim, TAE 700 (M). DONE by lead 2026-10-07 (anim.rs apply_twists).** `CustomLookAtTwistModifier` (in `extracted/chr/c0000.behbnd.d/c0000.hkx`;
run `sekiro-extract objects <that hkx> CustomLookAtTwistModifier`). TwistParam = {i16 bone from,
i16 bone to, f32 weight, f32 speed x2}; limits in degrees; the TAE 700 event's ModifierID picks
the modifier (0_Twist = lock-on idle, ±45° left/right and ±25° up/down; 100-130_Attack = slashes
tilt up/down toward the target).
- Do: in `anim.rs`, after posing, rotate the listed bone ranges by weight × the clamped angle to
  the lock-on target, blended in and out at the speeds. Bone indices are into Wolf's animation
  skeleton (`extracted/chr/c0000.chrbnd.d/c0000.HKX`, order as in `anim_c0000.bin`).
- Owns: `src/anim.rs`. Data access via `src/data.rs` `events_at` (read only).
- Done: Wolf's torso turns toward a locked target that stands off to the side; screenshot.

**V5 - Animation polish audit (S, research only, no code).** Static poses already checked by the lead (photo -Poses, 24 anims OK); look for motion issues only (sliding, pops, blend lengths). Play the game (`cargo run --release`;
controls are on screen, T toggles the enemy AI, Q locks on) and list every visual animation issue
you see: foot sliding, pops between clips, wrong blend lengths, T-poses, jitter. For each: the state
name (top-left HUD), what it looks like, and a guess at the cause from `anim.rs`
(crossfades = TAE Blend event 16; locomotion speed = root motion).
- Owns: nothing. Report only: `docs/reports/V5.md`.

### Swordplay track

**S1 - HKS gap audit (M, research only).** Compare every `BEH_A_*` / `BEH_R_*` branch in
`extracted/hks_src/c0000_transition.lua` that Wolf can reach with a sword (ground/air attack,
step, deflect guard start/continue/end, hit/guard/break damage, throws, mikiri) against
`src/player.rs` and `src/combat.rs`. List each difference: HKS line, what the game does, what we do.
Skip swimming, hanging, wire, crouch/stealth, prosthetic tools other than the shuriken, aging.
- Owns: nothing. Report: `docs/reports/S1.md`. The lead implements.

**S2 - Camera look limits for TAE 151 (S).** Wolf's TAE 151 also writes look limits (camera
singleton `DAT_143d5c0d0` +0x4c..+0x58, radians). Find their consumer (`extracted/decomp/14074.c`
near line 401, function around `FUN_1408311c0`) and apply it in `camera.rs` (`follow_player`,
next to the existing TAE 151 focus code). The TAE 153/155 handling there is already decoded; see the
2026-10-07 entries in docs/kb/camera.md.
- Owns: `src/camera.rs`.

**S3 - Tests for this week's swordplay (S).** Add sim tests in `src/sim_tests.rs` for:
guard release while moving → `DeflectGuardToStandMoveRun`; a minimum-level (damage level 8)
guarded hit keeps Wolf's state (`combat.rs additive_guard`); sprint attack picks `SprintAttack_L`
when steering right; landing mid air slash → `LandAirComboAttackN`. Use the helpers already in
the file (`app()`, `player_state`, `PadInput::press`).
- Owns: `src/sim_tests.rs`.

**S4 - Legacy FEV sound events (L, NEXT 10 in PROGRESS). MOSTLY DONE by lead 2026-10-07: the game's FMOD event DLL plays the real events (src/fmod.rs). Remaining: HitEffectSeJustGuardParam clash row selection (docs/kb/sound.md).** Parse the FMOD Designer `.fev` (LGCY)
event → layer → sound → sounddef → waveform chain so armour, clash and floor sounds resolve
exactly (gap 6). See docs/kb/sound.md and `fsb.rs`.
- Owns: `tools/sekiro-extract/src/fsb.rs` (+ a new `fev.rs`), `src/sound.rs`.

**S5 - Plunging deathblow (M). DONE by lead 2026-10-07 (player.rs start_plunge).** Jumping onto a posture-broken enemy and attacking while falling
does a plunge deathblow in Sekiro. Data: ThrowParam 11020150 / 11020151 ("崩し落下0/1", see
`docs/kb/guard-deflect.md` 2026-10-07): Dist, upper/lowerYRange and the normalFallOrbitCheck_* fields
(predicted landing within `range` of the defender within `timeLimit` ms). Wolf's anim a201_511400
(clip imported from a201_510300) fires TAE 920 ChrPhysicsVelocityChangeParam 8000 with
homingId 200000300 -> the homing param table (find its name in `tools/sekiro-extract/defs/paramdef`,
export the row in `export.rs`, read it in `data.rs`). Find how the exe evaluates the fall-orbit check
(`rg normalFallOrbit` gives nothing; search the decompile for reads of ThrowParam +0x65..+0x75).
- Owns: `tools/sekiro-extract/src/export.rs` (rows only), `src/data.rs` (new accessors); write the
  trigger logic as a proposed patch for `player.rs` in the report (lead applies it).

## 5. Order and parallelism
These can run at the same time (no shared files): **V1 ∥ V4 ∥ S1 ∥ S3 ∥ V5**.
Then V2 (after V1, both edit `flver.rs` / `model.rs`), V3 (after V1), S2, S4.
The lead keeps `player.rs` / `combat.rs` and implements what S1 and V5 find.

## 6. Report format (every task)
Write `docs/reports/<ID>.md`:
```
# <ID> - <title>
Status: done | partial | blocked
Changed files: ...
What I did: (3-8 lines)
Evidence: param rows / HKS lines / exe addresses used for every value
Screenshots: paths (visual tasks)
Gaps left: ... (also marked // gap: in code)
cargo test: <pass count>
```
