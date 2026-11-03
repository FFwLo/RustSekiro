# Shared brief: check one RSekiro system against the real game in the emulator

RSekiro (crate name sv1) is a Rust/Bevy recreation of Sekiro whose goal is to
play and feel exactly like the game. You check ONE system (your prompt names it) against the user's own
`sekiro.exe`: run the game's functions in the Unicorn emulator on a memory snapshot of the running game, write the
inputs and outputs as JSON cases, add a sim test that runs RSekiro's code on the same inputs, and fix RSekiro until
it matches. Faithful beats fast: port what the code does; if a rule can only be fitted to the numbers, say so.

## Read first
- `docs/kb/emulator-pilot.md` (how the tool works, what is done) and your topic's `docs/kb/<topic>.md`.
- The worked example: commit `f04cae7` (`git show f04cae7 --stat`, then read by range): the camera generator
  `camera_frames` in `tools/emu-cases/src/main.rs`, `FocusChase` in `src/camera.rs`, and the test
  `follow_camera_matches_the_exe` in `src/sim_tests/part11.rs`.

## Where things are (your worktree has no git-ignored data)
- Exe: your own `sekiro.exe` 1.6.0 (`<exe>` below). Never commit it or anything read from it.
- Snapshot: your own, taken with `emu-cases snapshot <pid> <snapshot dir>` while the game runs with a save loaded
  (about 10 GB, kept outside git; delete it when done). READ ONLY while agents share it. The maintainers' scene is
  General Naomori's arena, Wolf at (201.79, -26.23, 23.88), debug flag player-no-death on, disable-AI on
  (0x143d7a376 = 1; poke `b143d7a376=0` in the emulator copy to run the AI). Run with `--snapshot <snapshot dir>`.
- Output JSON: `$SHINOBI_ROOT/extracted/emu/<your_topic>_*.json` (git-ignored; your own file names).
- Reading the exe: use your own reverse-engineering setup (decompiler or disassembler); cite functions by address in
  comments and notes, never paste their code.
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
- emu-cases: `cd tools/emu-cases && cargo build --release`; `unicorn.dll` must sit next to
  `tools/emu-cases/target/release/emu-cases.exe` (copy it from the build output; Windows CFG must be off for that exe).
  Run: `tools/emu-cases/target/release/emu-cases.exe <exe> $SHINOBI_ROOT/extracted/emu <pilot> --snapshot <snapshot dir>`.
- Put your generator in a NEW module `tools/emu-cases/src/<topic>.rs` and add only a `mod` line and one dispatch
  line to `main.rs` (six agents edit main.rs; keep your part small so the branches merge).
- Put your test in a NEW file `src/sim_tests/emu_<topic>.rs` with one `mod` / `use` line in `src/sim_tests/mod.rs`
  (same reason). The test skips when its JSON is missing, like `super_armor_hit_matches_the_exe`.
- RSekiro tests: `SHINOBI_ROOT=<test root> cargo test --release <filter>` (the test root holds the branch's
  `config.toml` and your `extracted/`; `tools/worktree.py` makes one). Run long builds in the background. Run the full suite once before your final commit.

## Rules
- Agents never touch the live game (no memory writes, scenes, new snapshots or screen control) unless their human says so.
- Never commit or paste game bytes, disassembly, decompiled code or function lists; commit only your own Rust and
  numbers the functions computed (the JSON stays in the git-ignored `extracted/`).
- Files are LF. Never write repo files with Python text mode on Windows (it writes CRLF); use the Edit tool or
  binary writes, and check `git diff --stat` before committing.
- Commit in your worktree branch as yourself; an AI agent adds its `Co-Authored-By:` line. Open a pull request
  (docs/CONTRIBUTING.md).
- Code reads like the surrounding code: comments name the exe function and camera/chr offsets, say `gap:` for what
  is not ported or not checked.
- Notes: a dated section in your topic's `docs/kb/<topic>.md` (what matched, what was fixed with numbers, what is not
  covered), one row in the `docs/kb/emulator-pilot.md` results table, one line at the top of "## Recent" in
  `docs/PROGRESS.md`.

## Report back (short)
Branch and commits; what was run (functions, case count); what matched as it was; what was fixed (before -> after,
numbers); what is not covered and why; full test suite result.
