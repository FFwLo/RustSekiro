# Board changes

- 2026-11-04 FFwLo/claude: board created. Kinds E (emulator checks), P (port gaps), B (bugs and cleanup). Rows
  seeded from the 2026-11-03 handoff (frame order, goods bullets, other prosthetics, grapple scene, kb "Not covered"
  items, per-boss checks, emulator checks of the copied sekiro-rs crates). Claims are comments on the "Work board
  claims" issue.
- 2026-11-04 FFwLo/claude: E31 (deathblow window) and E32 (free camera swing) find the last gameplay stand-ins in `config.toml` in the exe; B8 leaves only player and sandbox settings there.
- 2026-11-04 FFwLo/heron: B5 done. tools/t.ps1, a PowerShell twin of t.sh for contributors without bash (#4)
- 2026-11-04 FFwLo/claude: E8 done. crates/sim collision vs the exe's casts and pushes (radius 0.01, keep 0.05, support/ceiling contacts), emu_srcollide tests
- 2026-11-04 FFwLo/claude: E9 done. crates/hks env and act calls vs FUN_140b36410 / FUN_140b32410, emu_hksvm tests
- 2026-11-04 FFwLo/claude: E34 done. map_world back_read matches WorldBackRead frame by frame (46 scenarios), 83256bd
- 2026-11-04 FFwLo/claude: E10 done. goal life f32 countdown, REGISTER_GOAL_UPDATE_TIME timer, aimove hook reads xmm2 (922573e); search ring earlier. Left: goal order frame by frame vs our Lua run
