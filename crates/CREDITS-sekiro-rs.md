# Credits

sekiro-rs reimplements its own readers and runtime, but stands on years of community format research.
No code from these projects is vendored; their documentation and source were read as references, except for the code adapted from FwLo's RustSekiro listed below.

## Formats and data

- [soulsmods/SoulsFormatsNEXT](https://github.com/soulsmods/SoulsFormatsNEXT) and [JKAnderson/SoulsFormats](https://github.com/JKAnderson/SoulsFormats): BND4, BXF4, DCX, FLVER, TPF, MTD, MSB, BTL, TAE, PARAM and EMEVD layouts.
- [soulsmods/Paramdex](https://github.com/soulsmods/Paramdex): param definitions and row names (fetched at setup, not vendored).
- [vawser/Smithbox](https://github.com/vawser/Smithbox): param field descriptions and enums.
- [Meowmaritus/DSAnimStudio](https://github.com/Meowmaritus/DSAnimStudio): Sekiro TAE event templates (fetched at setup).
- [Nordgaren/UXM-Selective-Unpack](https://github.com/Nordgaren/UXM-Selective-Unpack): archive keys and the file name dictionary (fetched at setup).
- [PredatorCZ/HavokLib](https://github.com/PredatorCZ/HavokLib), [Grimrukh/soulstruct-havok](https://github.com/Grimrukh/soulstruct-havok), [Meowmaritus/MVDX2](https://github.com/Meowmaritus/MVDX2): Havok tagfile, spline animation and collision references.
- [soulsmods/fstools-rs](https://github.com/soulsmods/fstools-rs): Rust format reading reference.
- EvenTorset's FXR library (Unlicense): Sekiro action field and property layouts.

## Scripts

- [AinTunez/DarkScript3](https://github.com/AinTunez/DarkScript3) (`sekiro-common.emedf.json`, after the soulsmods EMEDF tables): names and argument layouts of the event script instructions and their enums, read as facts for `crates/formats/src/emedf.rs`.
- [katalash/DSLuaDecompiler](https://github.com/katalash/DSLuaDecompiler), [Surasia/hksc-disassembler](https://github.com/Surasia/hksc-disassembler): Havok Script references, used locally for study.
- [iitsigor/SekiroHKS](https://github.com/iitsigor/SekiroHKS): names for engine functions used by the player script.

## Engine research

- [borgCode/SekiroTool](https://github.com/borgCode/SekiroTool), [Xu060113/SekiroCraft-Passthrough](https://github.com/Xu060113/SekiroCraft-Passthrough), [thisguymartin/sekiro-deflect-observer](https://github.com/thisguymartin/sekiro-deflect-observer), [ElaDiDu/Sekiro-Practice-CT](https://github.com/ElaDiDu/Sekiro-Practice-CT): memory layout and posture research used to cross-check our own findings.
- [FFwLo/RustSekiro](https://github.com/FFwLo/RustSekiro) (MIT): traced engine values and rules for movement, combat, camera, sound, animation, grappling and docking.
- Its MIT research notes also informed lighting, colour grading, shadows, map materials, sky and clouds, effects, and lightning reactions.
- These notes were read as references and cross-checked against our own data; adapted code is listed separately below.

## Code adapted from RustSekiro

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

## Inspiration

- [Funny-Bones/ELDEN-RING-Combat-Rewrite](https://github.com/Funny-Bones/ELDEN-RING-Combat-Rewrite): a data-driven Elden Ring combat sandbox in Bevy.
- [trevaintdead/ai-game-modding-guides](https://github.com/trevaintdead/ai-game-modding-guides): the workflow and rules this project follows.
