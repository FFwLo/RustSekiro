# Credits

Sv1 recreates SEKIRO: SHADOWS DIE TWICE, which belongs to FromSoftware and Activision. This
project is not affiliated with or endorsed by them. Nothing from the game is in this
repository: every player builds the data from their own copy.

## Special thanks

**AKJama**, creator of [sekiro-rs](https://github.com/AKJama/sekiro-rs), another Sekiro
recreation in Rust. From their work:

- **Enemy AI:** the Lua 5.0 loader and interpreter (`src/ai/lua50.rs`) that runs the game's own
  compiled enemy battle scripts.
- **Player script:** the HKS bytecode VM (`sekiro-hks`, used as a library in tests) that runs the
  game's own compiled player script beside ours.
- **Combat rules:** hit stop, the Owl's deathblow gate, bullets fading past their range,
  deathblow positioning giving way at walls, enemy super armour, enemies' reactions to hits in
  the air, guarded bullets leaving their shooter alone, how prosthetic tool hits cause
  reactions, block versus deflect by repel values, the posture and HP a guard lets through,
  the posture ranges of a collapse, the counter-only mikiri hitbox and Attack Power growth.
- **Maps and lighting:** the light and fog colour rules, fog density, the camera's exposure
  metering and adaptation, the GI probe lighting, the bloom levels and the map material's
  batching, from values they traced in the game's exe and shaders. Our own sandbox and
  Construct are lit by their lighting library (light sets, time of day, fog, exposure, bloom,
  sun shadow), and its light units light our effects.
- **Sky and fog:** the sky domes (`src/sky.rs`, `src/sky.wgsl`) and the height fog
  (`src/height_fog.rs`, `src/height_fog.wgsl`) are adapted from their code.
- **World:** the story time of day, the boss fog walls, MSB region boxes and the arena start
  check.
- **Movement:** the grapple marker's grey disc for a point found but out of range.
- **Bosses:** the Headless Ape and its partner reading each other through the map's event
  target.

Where each one is used in the code is listed in "Third-party code" below.

## Code and data used

| Project | What Sv1 uses it for |
|---|---|
| [sekiro-rs](https://github.com/AKJama/sekiro-rs) (MIT / Apache-2.0) | Lua 5.0 interpreter (`src/ai/lua50.rs`), sky domes, height fog, ported rules (see "Third-party code" below) |
| [@cccode/fxr](https://github.com/EvenTorset/fxr) (public domain) | Reads the game's effect files (FXR) for `tools/fxr-dump` |
| [DarkScript3](https://github.com/AinTunez/DarkScript3) | Sekiro's event-script instruction definitions (EMEVD argument types) |
| [Bevy](https://bevyengine.org) (MIT / Apache-2.0) | The game engine |
| The game's own FMOD runtime | Plays the game's sounds (loaded from your Sekiro install, not shipped) |

## References (read for file formats and names, no code copied)

- [SoulsFormatsNEXT](https://github.com/soulsmods/SoulsFormatsNEXT): FLVER, TAE, BND, TPF, MSB,
  param and other FromSoftware file formats.
- [Paramdex](https://github.com/soulsmods/Paramdex): param layouts and field names.
- [DSMapStudio](https://github.com/soulsmods/DSMapStudio) and
  [Smithbox](https://github.com/vawser/Smithbox): map and param editing references.
- [DSAnimStudio](https://github.com/Meowmaritus/DSAnimStudio): TAE event templates.
- [SekiroHKS](https://github.com/iitsigor/SekiroHKS) and
  [SDT-HKS](https://github.com/vawser/SDT-HKS): the character behaviour scripts' names.
- [ESDLang](https://github.com/thefifthmatt/ESDLang): state machine scripts.
- [SekiroTool](https://github.com/borgCode/SekiroTool): memory offsets, for the read-only
  checks against the running game.
- [gamedb](https://github.com/smileybaal/gamedb): indexing the exe decompile.
- The archive keys and file-name list in `tools/sekiro-extract/keys/` are the ones the
  community already ships with its unpacking tools (UXM).

Licences: this project's code is MIT ([LICENSE.md](LICENSE.md)); third-party licence texts
are in "Third-party code" below.

## Third-party code

### sekiro-rs

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

### Event instruction argument types

`tools/emedf_types.json` lists, per EMEVD instruction class and index, the argument types the
game's event scripts use (byte, short, int, float). It is a table of the binary format's facts,
generated from the Sekiro instruction definitions that ship with
[DarkScript3](https://github.com/AinTunez/DarkScript3) (`sekiro-common.emedf.json`, also at
https://soulsmods.github.io/emedf/sekiro-emedf.html); none of that file's names, descriptions
or text is included. `tools/boss_scripts.py` reads the definitions file itself when a
`reference/DarkScript3` clone is next to the repo.

## sekiro-rs's own credits (the crates in `crates/`)

sekiro-rs reimplements its own readers and runtime, but stands on years of community format research.
No code from these projects is vendored; their documentation and source were read as references, except for the code adapted from FwLo's RustSekiro listed below.

### Formats and data

- [soulsmods/SoulsFormatsNEXT](https://github.com/soulsmods/SoulsFormatsNEXT) and [JKAnderson/SoulsFormats](https://github.com/JKAnderson/SoulsFormats): BND4, BXF4, DCX, FLVER, TPF, MTD, MSB, BTL, TAE, PARAM and EMEVD layouts.
- [soulsmods/Paramdex](https://github.com/soulsmods/Paramdex): param definitions and row names (fetched at setup, not vendored).
- [vawser/Smithbox](https://github.com/vawser/Smithbox): param field descriptions and enums.
- [Meowmaritus/DSAnimStudio](https://github.com/Meowmaritus/DSAnimStudio): Sekiro TAE event templates (fetched at setup).
- [Nordgaren/UXM-Selective-Unpack](https://github.com/Nordgaren/UXM-Selective-Unpack): archive keys and the file name dictionary (fetched at setup).
- [PredatorCZ/HavokLib](https://github.com/PredatorCZ/HavokLib), [Grimrukh/soulstruct-havok](https://github.com/Grimrukh/soulstruct-havok), [Meowmaritus/MVDX2](https://github.com/Meowmaritus/MVDX2): Havok tagfile, spline animation and collision references.
- [soulsmods/fstools-rs](https://github.com/soulsmods/fstools-rs): Rust format reading reference.
- EvenTorset's FXR library (Unlicense): Sekiro action field and property layouts.

### Scripts

- [AinTunez/DarkScript3](https://github.com/AinTunez/DarkScript3) (`sekiro-common.emedf.json`, after the soulsmods EMEDF tables): names and argument layouts of the event script instructions and their enums, read as facts for `crates/formats/src/emedf.rs`.
- [katalash/DSLuaDecompiler](https://github.com/katalash/DSLuaDecompiler), [Surasia/hksc-disassembler](https://github.com/Surasia/hksc-disassembler): Havok Script references, used locally for study.
- [iitsigor/SekiroHKS](https://github.com/iitsigor/SekiroHKS): names for engine functions used by the player script.

### Engine research

- [borgCode/SekiroTool](https://github.com/borgCode/SekiroTool), [Xu060113/SekiroCraft-Passthrough](https://github.com/Xu060113/SekiroCraft-Passthrough), [thisguymartin/sekiro-deflect-observer](https://github.com/thisguymartin/sekiro-deflect-observer), [ElaDiDu/Sekiro-Practice-CT](https://github.com/ElaDiDu/Sekiro-Practice-CT): memory layout and posture research used to cross-check our own findings.
- [FFwLo/RustSekiro](https://github.com/FFwLo/RustSekiro) (MIT): traced engine values and rules for movement, combat, camera, sound, animation, grappling and docking.
- Its MIT research notes also informed lighting, colour grading, shadows, map materials, sky and clouds, effects, and lightning reactions.
- These notes were read as references and cross-checked against our own data; adapted code is listed separately below.

### Code adapted from RustSekiro

From [FFwLo/RustSekiro](https://github.com/FFwLo/RustSekiro), under the MIT licence below:

- RustSekiro commit 265e142bf3b36a5190d8ae4885acad7a4890cd16: layered map shader families, variant sampler tables, material constants and vertex blend routing.

- Map shaders and materials: layered shader families, samplers, material constants and vertex blending.
- Combat and camera: deathblow camera, perilous HUD, sweep and kick-off rules, Mikiri, parry posture and throw selection.
- Effects: FXR runtime, particle batching, blood, decals, distortion and point lights.
- Grappling: the camera-view hook search with its soft lock (RustSekiro commit 292655ca24e4438a959770a86b97baedf9584382, `grapple_target`), with the box checked against our own exe trace.
- Idol Travel: the warp state and its 2.0 s cut, the Travel list order and the screen layout (RustSekiro commit f6002306787102f9cd432f2f7c1bda9167f8efca, `src/idol.rs`), and cross-map travel by relaunch (`sr_map.rs`, `map_of`).
- Cloth: simulation solver, collisions, deformation and the traced bend-stiffness formula.
- Character materials: texture interpretation and cutouts, adapted into exported glTF metallic-roughness textures.

From Fireguy32's fork [Fireguy32/RustSekiro](https://github.com/Fireguy32/RustSekiro) (MIT, the same licence below), commit d11ef65:

- Combat HUD: the bar layout and colours measured from game captures, vitality length by maximum vitality, damage trails, posture states, arrows and display time, boss bars, over-head bars on the Head bone, and the item cluster's placement.

```
MIT License

Copyright (c) 2026 FFwLo

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

### Inspiration

- [Funny-Bones/ELDEN-RING-Combat-Rewrite](https://github.com/Funny-Bones/ELDEN-RING-Combat-Rewrite): a data-driven Elden Ring combat sandbox in Bevy.
- [trevaintdead/ai-game-modding-guides](https://github.com/trevaintdead/ai-game-modding-guides): the workflow and rules this project follows.
