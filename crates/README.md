# Copied crates

These crates come from sekiro-rs by AKJama (https://github.com/AKJama/sekiro-rs), licensed MIT OR
Apache-2.0 (`LICENSE-MIT`, `LICENSE-APACHE` here; its credits in the top-level `CREDITS.md`). They were
copied on 2026-11-03 from commit 673f68f of the local clone (branch local/game-lib, with one local
change in `game/src/map.rs`) and are maintained in this repository from then on; upstream changes are
not followed.

Ported from sekiro-rs since (2026-11-04): freeing map blocks the player has left and reloading them
before they are seen (AKJama, 77704c4 and 0fca3b8, #271) and the smaller map meshes (16-bit
indices, packed colours, one upload per layout; 3cdcadb, #263), merged onto the trimmed copy.

| Crate | What RSekiro uses it for |
|---|---|
| `hks` | the HavokScript VM that runs the game's compiled scripts (src/hks.rs) |
| `formats` | game file readers (behaviour graphs, GPARAM, ...) |
| `sim` | collision world (`collision`, `body`'s ground queries, `hit_filter`, `lo_collision`), navmesh, and the arena, event and story state the map and its lighting read |
| `game` (library `sekiro_rs`) | the streaming map, lighting, sky and fog (`SHINOBI_SR_MAP=1`, src/sr_map.rs) |
| `extract` (package `sr-extract`) | builds the map cache `game` reads (`SEKIRO_CACHE`, else `../sekiro-rs/cache`): `tools/extract.ps1 -World`. It reads the archive keys, file names and Paramdex defs from `tools/sekiro-extract` (no download); RSekiro's own data comes from `tools/sekiro-extract` |

On 2026-11-04 everything RSekiro does not use was removed as unused: the `brain-net` and `fmod`
crates; sekiro-rs's own game app in `game` (its `sekiro-rs` program, fights, bosses, arenas, duel,
menus, HUD, brain overlay and stage, coach options, input replay scripts, the map viewer and sweep
modes, and their tests); and in `sim` the characters, behaviour runtime, combat, bosses, brains,
Lua AI, duel and its CLIs and examples, with their tests. Its git history has the removed code.

Their rules were read from the exe, not run against it: treat them as unproven until the emulator
checks (docs/kb/emulator-pilot.md) cover them. Covered so far: `sim` collision's floors, wall slides and
rays on m11_00 (row E8, src/sim_tests/emu_srcollide.rs; docs/kb/movement.md "The copied collision against
the exe"), with the mesh's convex radius, the proxy's keep distance and its lifted spheres put in.
