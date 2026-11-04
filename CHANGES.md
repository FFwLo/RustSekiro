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
- 2026-11-04 FFwLo/claude: E1 done. the HKS reads the stick of its buttons' frame (hksstick, 44 frames match), 7747a65
- 2026-11-04 FFwLo/claude: E5 done. bullet task runs first in MoveMapStep, first update only launches; shuriken hits/chains match (bullets), 517aabb
- 2026-11-04 FFwLo/claude: E31 done. deathblow window = break anim's TAE 932 span (151 frames), broken_time stand-in removed; 20 chrs without a break anim keep 4 s (gap), f171729
- 2026-11-04 FFwLo/claude: E13 done. guard/posture break: KnockBackParam guardBrake 0/0 = no push (FUN_140be6750 is a position/facing controller, not knockback), 0972b2f
- 2026-11-04 FFwLo/claude: E11 done. sound target pick, 46 on serial change, 31 LoseSightTarget, record refresh (aiint), 25b1b6b
- 2026-11-04 FFwLo/claude: E17 done. fall landing/return rules from the exe: 1 HP floor, lethal = 0.4 maxHP or death, aged 5 m lethal, floor rates, void 448 (air_falldmg)
- 2026-11-04 FFwLo/claude: E7 done. grapple: least-speed arc to the landing spot, braking 10, mean-velocity step, arrival by flight time; every flight frame checked (wire)
- 2026-11-04 FFwLo/claude: E2 done. old clip's TAE window and attack records live into the next hit step (framewin, Actor held)
- 2026-11-04 FFwLo/claude: E3 done. both hit-stops start in the hit step on the hit frame, whoever updates first; Sv1 already matched (hits_stop_*)
- 2026-11-04 FFwLo/claude: E4 done. 67 / 401 / style 32 land in the step's frame (frameorder, player::tae_one_shots)
- 2026-11-04 FFwLo/claude: E33 done. geometry bits, observer light remap, nose marks (aistealth); gap: Wolf's light index needs MSB MapVisibility per collision
- 2026-11-04 FFwLo/claude: E12 done. AP rate enters via the hit entry's int16 percent (hprate)
- 2026-11-04 FFwLo/claude: E16 done. x1.4 = defender flag 41 non-arrow; 1.125 / 0.8 / p16 dead; p17 stays 1
- 2026-11-04 FFwLo/claude: E18 done. 1 cm floor matched, TAE 152 AttachCameraToNode ported, SlowEnd copies only (camleft)
- 2026-11-04 FFwLo/claude: E32 done. free camera swing is the full fulcrum swing; follow_swing stand-in removed (camswing)
- 2026-11-04 FFwLo/claude: E14 done. chr push: stepping side lifted 0.3 m and stops at 0.950 m, other never shoved; humanoids pass, dogs block (pushmove); rim leave by the two-sphere ground check (collide_rimleave)
- 2026-11-04 FFwLo/claude: E15 done. no NPC +1000 fallback in the exe (2,710 keys run); exporter keys fixed, art anims fall back to 5000 (params_keys)
- 2026-11-04 FFwLo/claude: E30 done. 130 cited param fields confirmed read by the cited exe code, no swaps (EMU_PARAM_READS)
- 2026-11-04 FFwLo/claude: E6 done. tool equip via ChrAsm handle; 12 tools frame by frame; interval children and bullet life fixed; victim levels (spear hits once)
