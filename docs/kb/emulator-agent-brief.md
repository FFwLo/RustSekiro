# Shared brief: check one RSekiro system against the real game in the emulator

RSekiro (crate name sv1, `<repo>`) is a Rust/Bevy recreation of Sekiro whose goal is to
play and feel exactly like the game. You check ONE system (your prompt names it) against the user's own
`sekiro.exe`: run the game's functions in the Unicorn emulator on a memory snapshot of the running game, write the
inputs and outputs as JSON cases, add a sim test that runs RSekiro's code on the same inputs, and fix RSekiro until
it matches. Faithful beats fast: port what the code does; if a rule can only be fitted to the numbers, say so.

## Read first
- `docs/kb/emulator-pilot.md` (how the tool works, what is done) and your topic's `docs/kb/<topic>.md`.
- The worked example: commit `f04cae7` (`git show f04cae7 --stat`, then read by range): the camera generator
  `camera_frames` in `tools/emu-cases/src/main.rs`, `FocusChase` in `src/camera.rs`, and the test
  `follow_camera_matches_the_exe` in `src/sim_tests/part11.rs`.

## Where things are (absolute paths; your worktree has no git-ignored data)
- Exe: `<dsekiro>\image\sekiro.exe` (1.6.0).
- Snapshot (READ ONLY, shared by 6 agents, never modify or delete): `<repo>\extracted\snapshot`.
  General Naomori's arena (taken 2026-11-03 again), Wolf at (201.79, -26.23, 23.88), debug flag player-no-death on,
  disable-AI ON (0x143d7a376 = 1; AI frozen; poke `b143d7a376=0` in the emulator copy to run the AI). Run with `--snapshot <that dir>`.
- Output JSON: `<repo>\extracted\emu\<your_topic>_*.json` (your own file names).
- Decompiled C of the whole exe: `<repo>\extracted\decomp\*.c` (`// @ <entry>` before
  each function; grep it) and `<dsekiro>\work\decomp\FUN_<addr>.c`. Do NOT run Ghidra or
  `tools/decomp.py` (shared project lock).
- Known addresses: WorldChrMan 0x143D7A1E0 (+0x88 PlayerIns); PlayerIns +0x1ff8 module bag: +0x68 physics (pos +0x80,
  yaw +0x74), +0x18 data (HP +0x130 / max +0x134), +0x90 SprjPlayerHitStopModule, +0x88 lock state; EnemyIns vtable
  0x142a27f28, PlayerIns vtable 0x142a2b338; FieldArea [0x143d5c0a0]+0x30 = ChrCam (vtable 0x1429a6d40), ChrCam +0x60 =
  follow cam (ChrExFollowCam 0x1429a7d88); DebugFlags 0x143D7A366; Wolf's manipulator
  M = [[PlayerIns+0x50]+0x358] (or +0x120), right stick M+0x80 (pitch, yaw), mouse M+0xd0 (degrees).
- Driving whole frames (see `tools/emu-cases/src/acttrace.rs`, kb/frame-coverage.md "Mid-action frames"): the snapshot
  has a menu open, which drops every button: in the emulator copy return 0 from FUN_1408c58e0 and clear the open menu
  window, as acttrace does. Enemies update every 30 frames unless chr +0x110 (update skip) is set to 0. Force an enemy
  attack by writing AI +0x7430 = behaviour id when FUN_1407e6570 starts. Flush Unicorn's code cache (uc_ctl TB flush)
  every frame / every few hundred calls or the host crashes; CRT longjmp FUN_1426963a0 -> plain longjmp (proxy.rs).
- Emulator API (main.rs): `Emu::new(exe, Some(snap))`, `peek::<N>`, `peek_u64`, `poke_f32s`, `raw_write`, `chr_cam`;
  calls take integer args and xmm args (see `camera_frames`). Unmapped reads load 64 KB of the snapshot on demand.

## Build and run
- emu-cases: `cd tools/emu-cases && cargo build --release`, then copy
  `<repo>\tools\emu-cases\target\release\unicorn.dll` next to your
  `tools/emu-cases/target/release/emu-cases.exe` (CFG is off for any exe named emu-cases.exe).
  Run: `tools/emu-cases/target/release/emu-cases.exe <dsekiro>/image/sekiro.exe <repo>/extracted/emu <pilot> --snapshot <repo>/extracted/snapshot`.
- Put your generator in a NEW module `tools/emu-cases/src/<topic>.rs` and add only a `mod` line and one dispatch
  line to `main.rs` (six agents edit main.rs; keep your part small so the branches merge).
- Put your test in a NEW file `src/sim_tests/emu_<topic>.rs` with one `mod` / `use` line in `src/sim_tests/mod.rs`
  (same reason). The test skips when its JSON is missing, like `super_armor_hit_matches_the_exe`.
- RSekiro tests: `SHINOBI_ROOT=<repo> CARGO_TARGET_DIR=target/run cargo test --release <filter>`
  (SHINOBI_ROOT makes the tests read the main checkout's `extracted/`). Six agents build at once on 16 cores: run
  long builds in the background, avoid needless rebuilds. Run the full suite once before your final commit.

## Rules
- Never touch the live game: no memory writes, no scenes, no new snapshots, no screen control.
- Never commit or paste game bytes, disassembly, decompiled code or `functions.json`; commit only your own Rust and
  numbers the functions computed (the JSON stays in the git-ignored `extracted/`).
- Files are LF. Never write repo files with Python text mode on Windows (it writes CRLF); use the Edit tool or
  binary writes, and check `git diff --stat` before committing.
- Commit in your worktree branch: `git -c user.name="FFwLo" -c user.email="<e-mail>" commit`, message
  ending with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Never push; no remotes.
- Code reads like the surrounding code: comments name the exe function and camera/chr offsets, say `gap:` for what
  is not ported or not checked.
- Notes: a dated section in your topic's `docs/kb/<topic>.md` (what matched, what was fixed with numbers, what is not
  covered), one row in the `docs/kb/emulator-pilot.md` results table, one line at the top of "## Recent" in
  `docs/PROGRESS.md`.

## Report back (short)
Branch and commits; what was run (functions, case count); what matched as it was; what was fixed (before -> after,
numbers); what is not covered and why; full test suite result.
