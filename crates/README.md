# Copied crates

These crates come from sekiro-rs by AKJama (https://github.com/AKJama/sekiro-rs), licensed MIT OR
Apache-2.0 (`LICENSE-MIT`, `LICENSE-APACHE` here; its credits in `CREDITS-sekiro-rs.md`). They were
copied on 2026-11-03 from commit 673f68f of the local clone (branch local/game-lib, with one local
change in `game/src/map.rs`) and are maintained in this repository from then on; upstream changes are
not followed.

| Crate | What RSekiro uses it for |
|---|---|
| `hks` | the HavokScript VM that runs the game's compiled scripts (src/hks.rs) |
| `formats` | game file readers (behaviour graphs, GPARAM, ...) |
| `sim` | collision world, navmesh, water surface |
| `game` | the streaming map, lighting, sky and fog (`SHINOBI_SR_MAP=1`, src/sr_map.rs) |
| `brain-net`, `fmod` | dependencies of `game` |
| `extract` (package `sr-extract`) | builds the map cache `game` reads (`SEKIRO_CACHE`, else `../sekiro-rs/cache`); RSekiro's own data comes from `tools/sekiro-extract` |

`brain-net` reads `brain-prices.toml` and `BRAIN.md` from its own folder (they sat at the sekiro-rs root).

Their rules were read from the exe, not run against it: treat them as unproven until the emulator
checks (docs/kb/emulator-pilot.md) cover them.
