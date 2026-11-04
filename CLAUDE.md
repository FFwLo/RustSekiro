# Sv1 (Sekiro combat recreation in Rust + Bevy)

A 1:1 recreation of Sekiro's combat. Every number
comes from the game's own files (TAE, HKS, params, behaviour graphs, FLVER, FMOD) or the
exe's own code (read with your own reverse-engineering setup and checked against
the exe in the emulator, `tools/emu-cases`). The package and exe are `sv1`; the game is the library `rustsekiro`
(src/lib.rs, `rustsekiro::run()`), src/main.rs only starts it. GitHub: `FFwLo/RustSekiro`, `main`.

**Agents start here:** @AGENTS.md

The skill `.claude/skills/rsekiro-row` does all of it end to end ("do an RSekiro row"). Contributor rules:
`docs/CONTRIBUTING.md`. The guard (`tools/guard.py`, hooks in `.githooks`) refuses game data, decompiled code and
disassembly.

## Commands
- Build: `cargo build --release` (game), `cargo build` (debug). One target folder per session
  (`CARGO_TARGET_DIR=target/<name>`); the extractor uses `tools/sekiro-extract/target`.
- Tests: `tools/t.sh <filter>`, or `tools\t.ps1 <filter>` without bash (quiet: failures and totals only). Same as
  `cargo test --profile quick --lib <filter>`. Run only the covering tests per change; the full
  suite once at the end. "Done" = build and the covering tests pass.
- Regenerate data: `powershell -ExecutionPolicy Bypass -File tools\extract.ps1`.
- Exe checks: `tools/emu-cases` runs the exe's functions on a snapshot of the running game
  (`docs/kb/emulator-agent-brief.md`, `docs/kb/emulator-pilot.md`).
- Trace / screenshots: env knobs documented in the source module headers (`SHINOBI_THROW_TRACE`, `SHINOBI_SKILL`, `SHINOBI_HUD_SHOTS`, ...).

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
| Extractor | `tools/sekiro-extract/src/` |
| The game's HKS script + behaviour graph run as the game does (live by default, SHINOBI_HKS=0 off; tests) | `src/hks_wolf.rs`, `src/hks.rs`, `src/hks_lua.rs`, `src/hkb_runtime.rs`, `docs/kb/behaviour-graph.md` |
| The playthrough start to finish, gates, endings | `docs/kb/game-flow.md` |
| The combat core alone (`--no-default-features`), its hooks; in sekiro-rs's game (`embed`: camera + debug menu, his `--rustsekiro` mode) | `src/lib.rs` (CombatCorePlugin, re-exports) |
| Reference clips | `docs/kb/clips.md` (check before asking for a new recording) |
| Checks against the exe (emulator) | `tools/emu-cases/src/<topic>.rs`, `src/sim_tests/emu_*.rs`, `docs/kb/emulator-pilot.md`, `docs/kb/frame-coverage.md` |
| Streaming map, lighting, collision, script VM (copied from sekiro-rs) | `crates/`, `crates/README.md` |
| Proof level per system and character; the work map (README picture) | `tools/coverage.py` → `docs/kb/coverage.md`; `python tools/workmap/build.py --tests <n> --png` → `target/workmap.html` and the README picture `docs/media/workmap.png` |
| Nothing is left off the board: every enemy and boss has a C row, every coverage.md system an open row | `python tools/board_rows.py` (prints what is missing; `--write <board>/WORK.md` adds the C rows) |
| Live recordings of the real Sekiro | `docs/kb/recordings.md`: read `python -m tools.rec summary <rec> --cat <topic>`, never raw data |

Community references worth cloning next to the repo (read-only): SekiroHKS, SDT-HKS, Paramdex, SekiroTool,
SoulsFormatsNEXT, sekiro-rs, fxr. Check them first for names and layouts.

## Do not read by default
- `target/`.
- `extracted/`: never list or read it wholesale. Open a named file, or grep one subfolder for
  one symbol. Never open `*.bin`; `decomp`, `ghidra`, `json` and `param` are huge.
- Whole large source files: grep for the symbol, read a range. Sim tests are in `src/sim_tests/` (11 parts of ~1.5k lines; `mod.rs` lists which tests are where).

## Starting a session
1. Take a row from the work board (see "Agents start here") and say which.
2. Read only that row's kb file and source module (table above).
3. Record what changed in the kb file for the topic.
