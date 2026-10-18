# Sculptor on the Construct (2026-10-18)

- Character c7010. The Outskirts MSB places entity 1100720 with NpcParam 70100080 (the
  conversation row), not a boss or combat opponent. Model and animation exports already come
  from `sekiro-extract npcs extracted c7010`: model_c7010.bin, anim_c7010.bin, enemies/c7010.json.
- The Construct's Sculptor is placed at (-7, 0, 3), beside Course Start. He reuses the character
  renderer, Havok skeleton, skinned materials and animation events. His data/library are kept
  outside encounter slots, so switching bosses cannot turn him into another character. He has
  no Enemy component and is excluded from combat resolution and enemy-targeted projectiles.
- Talk uses EventText 10010200, on E. Wolf plays Event700400 / Event700420, his NPC-talk states.
  Entering opens the existing Workshop; Fit and Upgrade retain their data-driven costs and
  ownership rules. Esc/Q from the Workshop closes the visit; backing out of a service returns
  to the Workshop. The idol menu no longer includes it.
- Talking does not heal, refill items, unlock resurrection, respawn enemies or change the idol
  checkpoint. Damage cancels the talk. The NPC is hidden and cannot be talked to on other maps.
- Gaps: service NPC only, without ESD story conversations, voiced dialogue, gifts or sake
  progression. Interaction reach (2 m, height difference under 0.8 m), placement and the initial
  talk camera framing are reconstructed for this test map. His assets load when starting on
  the Construct; starting on another map and switching there later does not spawn him.
- Private screenshot harness: SHINOBI_SCULPTOR_SHOT=1 opens the real talk interaction nearby;
  SHINOBI_SCULPTOR_ANIM=<clip> allows inspecting the NPC's exported clips. Use SHINOBI_SHOT and
  SHINOBI_SHOT_FRAMES=600, without desktop input or publishing game-asset screenshots.
- Validation: release build and private real-app Talk / Workshop capture passed, including the
  seated c7010 model beside Wolf. New tests cover reach and floor height, preserving c7010
  across encounter changes, and leaving without healing, refilling or changing the checkpoint.
  Fitting and upgrade pages also passed private real-app captures. Full suite: 655 passed,
  46 ignored with one thread. The initial four-thread run hit the previously intermittent
  Divine Confetti ghost-barrier assertion; the unchanged suite passed serially.
