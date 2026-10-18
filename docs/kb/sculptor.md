# Sculptor on the Construct (2026-10-18)

- Character c7010. The Outskirts MSB places entity 1100720 with NpcParam 70100080 (the
  conversation row), not a boss or combat opponent. Model and animation exports already come
  from `sekiro-extract npcs extracted c7010`: model_c7010.bin, anim_c7010.bin, enemies/c7010.json.
- The Construct's Sculptor is placed at (-7, 0, 3), beside Course Start. He reuses the character
  renderer, Havok skeleton, skinned materials and animation events. His data/library are kept
  outside encounter slots, so switching bosses cannot turn him into another character. He has
  no Enemy component and is excluded from combat resolution and enemy-targeted projectiles.
- Talk uses EventText 10010200, on E. Wolf plays Event711200 to sit down, Event711201 while
  seated, and Event711202 to get up. The menu waits for the entry clip to finish; movement
  waits for the exit clip to finish. c7010 plays a000_000400 when the visit starts, loops
  a000_000401 while Wolf visits, then plays a000_000402 and resumes a000_000000 when he leaves.
  Fit and Upgrade retain their data-driven costs and
  ownership rules. Esc/Q from the Workshop closes the visit; backing out of a service returns
  to the Workshop. The idol menu no longer includes it.
- Talking does not heal, refill items, unlock resurrection, respawn enemies or change the idol
  checkpoint. Damage cancels the talk. The NPC is hidden and cannot be talked to on other maps.
- Gaps: service NPC only, without ESD story conversations, voiced dialogue, gifts or sake
  progression. Interaction reach (2 m, height difference under 0.8 m), placement and the service
  animation sequencing are reconstructed for this test map. The regular player camera runs
  throughout the visit; there is no custom conversation angle. His assets load when starting on
  the Construct; starting on another map and switching there later does not spawn him.
- Exporter whitelist: Event711200 / 711201 / 711202 (c0000.hkx CMSG mappings). Re-run
  `sekiro-extract export extracted` after pulling this change so the local player data and
  anim_c0000.bin contain the seated visit clips. The Sculptor animation family is catalogued in
  [TAE Animation Designations](https://www.nexusmods.com/sekiro/articles/236); local exported
  clip data and private real-app captures determine the selected sequence.
- Private screenshot harness: SHINOBI_SCULPTOR_SHOT=1 opens the real talk interaction nearby;
  SHINOBI_SCULPTOR_ANIM=<clip> allows inspecting the NPC's exported clips. Use SHINOBI_SHOT and
  SHINOBI_SHOT_FRAMES=600, without desktop input or publishing game-asset screenshots.
- Validation after the animation follow-up: release build and private real-app seated
  Workshop capture passed with both characters and the regular player camera. Five Sculptor
  tests cover reach, encounter changes, no rest or checkpoint changes, the NPC's visit / return
  sequence, and the full player update completing the sit-down before opening the menu.
  Full suite: 657 passed, 46 ignored with four threads (130.92 s). Fitting and upgrade pages
  also passed private real-app captures during the initial NPC implementation.
