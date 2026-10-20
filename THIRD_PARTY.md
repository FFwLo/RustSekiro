# Third-party code

## sekiro-rs

`src/ai/lua50.rs` (the Lua 5.0 loader and interpreter) is adapted from
[sekiro-rs](https://github.com/AKJama/sekiro-rs), `crates/sim/src/lua_ai/lua50.rs`,
which is dual-licensed MIT or Apache-2.0.
The HKS bytecode VM (`crates/hks`, `sekiro-hks`) is used unchanged as a dev-dependency
(`src/hks_lua.rs` tests).
Rules also ported from it (no code copied verbatim): the Owl's 3506080 deathblow gate
(`player.rs`, its docs/OWL.md) the bullet decay past range (`data.rs` `Attack::decayed`,
its `bullets.rs`) and the deathblow absorb giving way at walls (`player.rs` `follow_throw`,
its docs/DEATHBLOW.md). Smaller ports from the same repository follow its
docs and code, each credited where it is used: the hit stop rules (`src/combat.rs`
`start_hit_stop`, `src/actor.rs` `HitStop`) and the arena start and walk check
(`src/map.rs` test `every_arena_start_stands_and_walks`, after `crates/sim/examples/map_baseline.rs`), and the
map lighting rules its docs/SHADER-INPUTS.md traced in the exe (`src/map.rs` `colour`,
`fog_colour`, `soft_adapt`, `SCENE_TO_BEVY`: light colours squared times W, fog colours at
gamma 2.2, the soft Adaption Min / Max limit, one scene-to-Bevy light scale), the enemy super
armour damage-level swap (`src/combat.rs` `damage_motion_level`), the enemies' aerial hit
reactions (`enemy_damage_state_air`) and the guarded bullet leaving its shooter alone (its
docs/PROJECTILE-GUARD.md). Also from its traced exe values: the fog density over
GameScale (`src/map.rs` `MapLighting::fog_density`, its docs/SHADER-INPUTS.md), the
game's metering centre weight and adaptation rate (`src/map.rs` `centre_weight`,
`adapt_speed`, its docs/SHADERS.md "Metering and adaptation"), the bottom-centred MSB
region box (`tools/sekiro-extract/src/msb.rs` `Region::contains`, its lighting.rs GIIV
volumes), the story time-of-day flags of common.emevd events 100 / 820 (`src/config.rs`
`default_hour`, its crates/sim/src/story.rs) and the tool hits' reaction path
(`src/prosthetic.rs` `bullet_reaction`, after its fight.rs rules) and the boss fog walls'
common events 20005820 / 20005825 (`tools/fog_walls.py`, `src/fog_wall.rs`, after its
crates/sim/src/fogwall.rs). The grapple marker's grey MENU_Wire_Distance disc for a point found
but out of range (`src/hud.rs` `update_wire_mark`) follows its crates/game/src/grapple_hud.rs. `src/height_fog.wgsl` (the game's fog pass) is adapted from its
crates/game/src/fog.wgsl and `src/height_fog.rs` from its fog.rs, with the exe packing of its
docs/SHADER-INPUTS.md (GameScale, the height offset, the depth slices). The sky domes (`src/sky.rs`,
`src/sky.wgsl`, `tools/sekiro-extract/src/sky.rs`) are adapted from its crates/game/src/sky.rs,
sky.wgsl and crates/extract/src/maps/sky.rs (9ee18b0); the map bloom's level weights
(`src/map.rs` `map_bloom`) follow its SHADER-INPUTS.md "Bloom and glare". The GI probes'
irradiance volumes (`tools/sekiro-extract/src/map.rs` `export_giiv`, `src/map.rs`
`apply_giiv`, `src/map_material.wgsl` `giiv_light` / `lit`) follow its docs/SHADERS.md "GIIV"
(the texel decoder and the directional basis traced in the exe's shaders) and its 6de8ff6
lighting.rs / map_material.wgsl (the box placement, the one-voxel fade and normal bias). The map material's bindless
batching (`src/map.rs` `MapExt` / `MapUniform`, `src/map_material.wgsl` `#ifdef BINDLESS`)
follows its 73eca1d map_material.rs / map_material.wgsl (2026-10-13).
The combat pass (2026-10-19) ported its combat rules, each value checked against the params: the
guard row, block versus deflect by repel values, the guard type's columns, the guard arc and the
posture and HP a guard lets through (`src/guard.rs`, `src/combat.rs` `guard_contact`,
`guard_posture`, `npc_guard_chip`, after its crates/sim/src/combat hit.rs, posture.rs, hp.rs and
effects.rs), the StaminaControlParam posture ranges (`src/actor.rs` `hold_posture_range`, its
posture.rs `ControlRange`), the counter-only mikiri dummy (`src/combat.rs` `counter_dummy`, its
perilous.rs) and the Attack Power growth row (`src/combat.rs` `growth_sp`, its progression.rs).
The vfx and lighting pass (2026-10-19) calls its library instead of copying it: its light unit
constants (`src/map.rs`, `src/map_lights.rs`, `src/fxr.rs` use `sekiro_rs::lighting::SCENE_TO_BEVY`,
`ENV_PER_UNIT`, `LOCAL_LUMENS_PER_UNIT`, traced from the exe's deferred light pass), its
`LightSet::from_gparam` for the lit particles' light (`src/sr_map.rs` `particle_light`), its
`map::add_map` lighting with no map pieces for our sandbox and Construct (`src/sr_map.rs`
`lighting_only`) and the gparam reader of its sekiro-formats crate in tests.
The bosses pass (2026-10-19) ported its TARGET_EVENT rule for the two apes of the Headless Ape
fight (the map's 2004[11] gives each AI its partner's live position, HP, posture and SpEffects;
`src/ai.rs` `EventView`, `src/enemy/script.rs` `Act::EventTarget`, after its
crates/sim/src/headless_ape.rs and docs/BOSSES-b.md), on our enemy AI, not its duel host.
It is used here under the MIT license:

```
MIT License

Copyright (c) 2026 AKJama

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

## Event instruction argument types

`tools/emedf_types.json` lists, per EMEVD instruction class and index, the argument types the
game's event scripts use (byte, short, int, float). It is a table of the binary format's facts,
generated from the Sekiro instruction definitions that ship with
[DarkScript3](https://github.com/AinTunez/DarkScript3) (`sekiro-common.emedf.json`, also at
https://soulsmods.github.io/emedf/sekiro-emedf.html); none of that file's names, descriptions
or text is included. `tools/boss_scripts.py` reads the definitions file itself when a
`reference/DarkScript3` clone is next to the repo.
