# Sv1 (Sekiro combat recreation in Rust + Bevy)

Project at `C:\DeadlockModding\shinobi-combat`: a 1:1 recreation of Sekiro's combat. Every number
comes from the user's own Sekiro install (TAE, HKS, params, behaviour graphs, FLVER, FMOD) or the
static Ghidra decompile of the exe. The crate and exe are `sv1`. GitHub: `FFwLo/RustSekiro`, `main`.

## Commands
- Build: `cargo build --release` (game), `cargo build` (debug). One target folder per session
  (`CARGO_TARGET_DIR=target/<name>`); the extractor uses `tools/sekiro-extract/target`.
- Tests: `tools/t.sh <filter>` (quiet: failures and totals only). Same as
  `cargo test --profile quick --bin sv1 <filter>`. Run only the covering tests per change; the full
  suite once at the end. "Done" = build and the covering tests pass.
- Regenerate data: `powershell -ExecutionPolicy Bypass -File tools\extract.ps1`.
- Exe research: `tools/fn.sh <addr>` prints a decompiled function; `extracted/rtti_vtables.txt`.
- Trace / screenshots: env knobs documented in `docs/HANDOFF.md` section 6 and the source module
  headers (`SHINOBI_THROW_TRACE`, `SHINOBI_SKILL`, `SHINOBI_HUD_SHOTS`, ...).

## Where things are (read only what the task needs)
| Topic | Read |
|---|---|
| Wolf's states, input, moves | `src/player.rs`, `docs/kb/movement.md`, `attacks.md`, `input-buffer.md` |
| Hits, posture, guard, deflect, deathblow | `src/combat.rs`, `src/actor.rs`, `docs/kb/guard-deflect.md`, `damage.md` |
| Enemies, AI, stealth | `src/enemy.rs`, `src/ai.rs`, `src/stealth.rs`, `docs/kb/enemy.md` |
| Animation, models, cloth, materials | `src/anim.rs`, `src/model.rs`, `src/cloth.rs`, `docs/kb/visuals.md` |
| Effects, blood, bullets | `src/fxr.rs`, `src/ffx.rs`, `src/gore.rs`, `src/vfx.rs`, `docs/kb/fxr.md` |
| Prosthetics, arts, skills | `src/prosthetic.rs`, `src/model/tool.rs`, `src/status.rs`, `docs/kb/skills.md` |
| Grappling hook | `src/wire.rs`, `docs/kb/grapple.md` |
| Map, sky, fog, lighting | `src/map.rs`, `src/sky.rs`, `src/height_fog.rs`, `docs/kb/map.md`, `sky_shader.md` |
| Camera | `src/camera.rs`, `docs/kb/camera.md` |
| Sound, music | `src/sound.rs`, `src/fmod.rs`, `src/area_music.rs`, `docs/kb/sound.md` |
| HUD, menus | `src/hud.rs`, `src/menu.rs` |
| Extractor | `tools/sekiro-extract/src/`, `docs/kb/tools-exe.md` |
| Reference clips | `docs/kb/clips.md` (check before asking the user for a new recording) |

Reference clones (read-only): `C:\DeadlockModding\reference` (SekiroHKS, SDT-HKS, Paramdex,
SekiroTool, SoulsFormatsNEXT, sekiro-rs, fxr, ...). Check them first for names and layouts.

## Do not read by default
- `docs/history/` (grep it), `docs/kb/log-archive.md`, `docs/kb/next.md`, `target/`.
- `extracted/`: never list or read it wholesale. Open a named file, or grep one subfolder for
  one symbol. Never open `*.bin`; `decomp`, `ghidra`, `json` and `param` are huge.
- Whole large source files: grep for the symbol, read a range. Sim tests are in `src/sim_tests/` (11 parts of ~1.5k lines; `mod.rs` lists which tests are where).

## Starting a session
1. Read `docs/PROGRESS.md` and the open items in `docs/PLAN.md`; take one item and say which.
2. Read only that item's kb file and source module (table above).
3. Record what changed in the kb file for the topic and in `docs/PROGRESS.md`, not in
   `docs/HANDOFF.md` (that file is the current-state summary only).
