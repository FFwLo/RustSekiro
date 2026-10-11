# map: the game's own arena (MSB, map pieces, hit collision, draw params)

Dated findings, oldest first. Append new entries at the end.

## 2026-10-10 The real map around the General (tools/sekiro-extract/src/map.rs, src/map.rs)

The fight now takes place where the General (c1020_0004, NpcParam 10203010) stands in
m11_01_00_00 (Ashina Outskirts, the big gate): his pieces, hit collision, draw params. Everything
comes from the user's own game files; nothing is modelled by hand.

### Pipeline (all in `tools/extract.ps1`, block "Map")
```
sekiro-extract unpack <Sekiro> extracted 'map/mapstudio/'            # MSBs
sekiro-extract unpack <Sekiro> extracted 'map/m11_01_00_00/'         # pieces (mapbnd), hit (hkxbhd/bdt)
sekiro-extract unpack <Sekiro> extracted 'map/m11/'                  # m11_*.tpfbhd/.tpfbdt textures
sekiro-extract unpack <Sekiro> extracted 'param/drawparam/m11_01|other/maptex'
sekiro-extract bxf extracted/map/m11_01_00_00/h11_01_00_00.hkxbhd extracted/map/m11_01_00_00/hit
SEKIRO_DIR=<Sekiro> MAP_LOD=20,40 sekiro-extract map extracted m11_01_00_00 c1020_0004 60
```
(`SEKIRO_DIR` for oo2core: the pieces inside the mapbnds are DCX KRAK.) The `map` command takes
`<extracted> <map id> <centre part name> <radius m>` and writes `extracted/map_<id>.bin`
(pieces, "SHMP"), `.hit` (collision, "SHMC"), `.json` (centre, light set, draw params) and the
textures into `extracted/tex/`. `MAP_LOG=1` prints every piece taken and every material without
an albedo. Output for radius 60 with `MAP_LOD=20,40`: 257 batches, 4.13 M triangles, 186 MB;
631 k collision triangles (radius 80 / LOD 30,55: 230 batches, 6.86 M, 310 MB). The game loads
it in about 0.3 s.

Game side: `config.toml` `[world] map = "m11_01_00_00"`, `hour = 12.0`; `SHINOBI_MAP=<id>` or
`SHINOBI_MAP=off` (also "" / "0") override for one run. Without a map, `world.rs` builds the flat test floor as
before.

### Formats (readers in tools/sekiro-extract/src/)
- **MSB** (`msb.rs`, MSBS): MODEL_PARAM_ST and PARTS_PARAM_ST. Part header: name, type (0 map
  piece, 2 enemy, 5 collision, ...), model, pos @0x20, rot @0x2C (degrees, Y-up Euler, rotate =
  Ry*Rz*Rx), scale @0x38, 48 u32 masks @0x50 (display groups [0..8), draw groups [8..16)),
  type data @0x68 (enemy: +8 think id, +12 npc id; collision: +0 HitFilterID), gparam config
  @0x70 ([LightSetID, FogParamID, LightScatteringID, EnvMapID]). sekiro-rs msb.rs is the model.
- **BXF4 split binders** (`container.rs` `read_bxf4`): `.tpfbhd/.tpfbdt`, `.hkxbhd/.hkxbdt`.
  The BHF4 header is the BND4 layout, offsets point into the BDT. `bxf` expands one to a dir.
- **Map FLVER** (`flver.rs`): static, no bones. Face sets carry LOD flags (0 full, 1 LodLevel1,
  2 LodLevel2, 0x8000_0000 motion blur): `read_lod(d, lod)` takes the wanted level, else the next
  lower one. Vertex layouts: pos Float3, normal/tangent Byte4B, a Byte4C (type 0x13) "UV" member
  carrying blend data, then the texture UVs as UV (0x15, two shorts / 2048) or UVPair (0x16, two
  UV sets). `Mesh.layouts` lists them; `sekiro-extract flverinfo <flver>` prints materials, slots,
  layouts and UV samples.
- **MTD** (`mtd.rs`): shader name, int/float params (`g_AlphaRef`, `g_BlendMode`,
  `g_DoubleSided`), texture slots with default paths and `uv_number`. Map FLVERs leave most slot
  paths empty: the MTD's defaults are the real textures (that is how `m[m11_00]_roof1.mtd` and
  friends work in the game).
- **GPARAM** (`gparam.rs`): header at 4/8 (game 5), groups of params typed 1|5|11 u8, 2 i16,
  3|7 i32, 9 f32, 12..14 vec2/3/4, entries by id (0 base; 100/110/120 the area's own sets) and
  time of day. `to_json` writes them into the `.json`.
- **Havok 2016 hit collision** (`hkx.rs` `collision_mesh`): the binder's `.compendium` holds the
  types (pass it to `hkxdump`/`objects`). hknpPhysicsSceneData -> hknpPhysicsSystemData.bodyCinfos
  (position @48, orientation @64 of bodyCinfoWithAttachment) -> fsnpCustomParamCompressedMeshShape
  -> hknpCompressedMeshShapeData.meshTree (sections, packedVertices, sharedVertices,
  sharedVerticesIndex, primitives). Materials: triangleIndexToShapeKey + pParam
  TriangleData.primitiveDataIndex -> PrimitiveData.materialNameData = HitMtrlParam id. Tagfile
  arrays use the TBDY type size as stride (the item span / count misreads 16-byte padded structs:
  PrimitiveData 64 came out as 67 and the materials were garbage).

### What the exporter does
- Centre part by name; arena origin = his position + 4 m along his facing, turned by -yaw so
  he keeps facing +Z: he stands at (0, y, -4) = the game's enemy spawn, the player spawns at
  (0, y, +4) in front of him. Written unmirrored; the game mirrors X on load (FromSoftware's
  left-handed space to Bevy's right-handed one, so the map keeps the real game's handedness).
  The mirror alone turns the FLVER's DirectX clockwise triangles into Bevy's counter-clockwise
  front faces: no index swap (see "Winding" under 2026-10-11; the Havok hit mesh is
  right-handed data and does need the swap, `Terrain::from_hit`).
- Collision parts within the radius; the part under the origin (`collision_part`, here h022400)
  gives the draw groups and the light set (gparam [100,0,0,0]). Pieces are taken when their draw
  groups intersect those (the engine swaps far stand-ins for detail by these masks), within the
  radius (bounding box in XZ), with LOD by distance: full to 30 m, LodLevel1 to 55 m, LodLevel2
  beyond.
- Materials: `choose()` = sekiro-rs `choose_material`: first albedo slot with a path in MTD
  order (a slot named `<material>_a` wins), the normal map named like it (`_a` -> `_n`), else
  after the material, else the first; alpha from `g_BlendMode` (2 or >= 4 blend, 1 cutout) and
  `g_AlphaRef` (> 0 cutout); cutouts are two-sided. Batches merge by (albedo, normal, alpha,
  two-sided). Skipped: water, sky, decal/ripple-only shaders.
- Textures from `map/m11/m11_*.tpfbhd` and `other/maptex.tpf`, top mips above 1024 dropped
  (`drop_top_mips`). Map albedos are BC1 sRGB (DXGI 72), normals BC7.

### Game (src/map.rs)
- Pieces: static `SekiroMaterial` meshes (albedo + normal map, AlphaToCoverage cutouts, Blend);
  tangents by Lengyel from the UV gradients. **Textures must load with a repeat sampler**: Bevy's
  default clamps to the edge and every piece with UVs outside 0..1 (beams -9..14, roof tiles
  -12..7) came out flat grey (one edge texel smeared; the 0..1 stone blocks looked right, which
  hid it). Anisotropy 8.
- Collision (`Terrain`, port of sekiro-rs sim/collision.rs): 4 m grid; floor = highest walkable
  triangle (|n.y| >= cos 50 deg) at or below the body top; STEP_HEIGHT 0.5; walls push three
  stacked spheres (radius 0.35 at 0.85 / 1.2 / 1.55 m) out horizontally; Moeller-Trumbore ray
  for the camera arm. Hooks: `ground_y` (player.rs landing / free fall / plunge height),
  `raycast` (camera.rs arm clamp), `floor_material` (sound.rs floor sounds by HitMtrlParam id),
  `bodies_on_terrain` (FixedUpdate after ActorSet::Advance: every actor is kept on the floor and
  out of walls).
- Lighting from the draw params (light set 100 here): sun = Directional Light Angle0 /
  DiffColor0, fill = Angle1 / DiffColor1 (toward = Ry(arena yaw) Ry(yaw) Rx(pitch) +Z, then X
  mirrored), sky / fog = VolumetricFog SkyColor and Global FixedDensity (exponential).
  `LUX_PER_UNIT` 9000 is sekiro-rs's by-eye scale; `ENV_PER_UNIT` = 9000 / pi keeps the probes
  on the same scale. Without probes the Hemi colours give a flat ambient (`AMBIENT_PER_UNIT`).
- **GI probes (2026-10-10, "fix the gate lighting")**: `map/<id>/<id>_envmap_00..05.tpf` hold
  the area's light probes: `gilm<probe>_<v>` 128^2 BC6H cube maps (7 mips, sky + surroundings
  with the sun disk baked in) and `giiv<probe>_<v>_{w,x,y,z}` BC7 3D irradiance volumes (not
  used). The probe is the MSB region around the arena: `Env_Box<probe>_...`
  (EnvironmentMapEffectBox, type 17, centred box) else the nearest `Env_Point<probe>`
  (type 2); the gate is `Env_Box380_ハブ櫓` -> `gilm0380`. `cubemap.rs` decodes BC6H
  (bcdec_rs), turns the cube into arena space (undo the X mirror, then the arena yaw), writes
  `map_<id>_env_<v>.dds` (RGBA16F cube with mips; DX10 arraySize must be 6 or Bevy makes a 2D
  texture) and `map_<id>_envd_<v>.dds` (16^2 Lambert irradiance / pi from the cube clamped at
  1.0 so the baked sun does not light everything twice). The game puts them on the camera as
  `EnvironmentMapLight` (diffuse + specular) and `Skybox`, ambient off. The six variants are
  times of day, matched by sky colour and sun direction to the light set keys:
  _00 0 h, _01 6 h, _02 12 h, _03 18 h, _04 22 h, _05 2 h (`ENV_HOURS`, json "env_hours");
  the game takes the nearest to the hour. Probe sun glare vs gparam sun at 18 h: ~30 deg
  apart (artist-placed), the face convention alternatives are all worse.
- Hour: `[world] hour = 18` = dusk, the Outskirts' first-visit look (warm low sun, orange
  haze); 12 gives the cool overcast look of some captures. sekiro-rs used 16 by eye.
- Env knobs: `SHINOBI_HOUR`, `SHINOBI_LIGHT_SET`, `SHINOBI_MAP_LUX` (light scale),
  `SHINOBI_MAP_ENV` (probe scale), `SHINOBI_MAP_NO_ENV=1` (hemisphere ambient + clear colour
  instead of the probe), and the debug views `SHINOBI_MAP_NO_NORMALS=1`, `SHINOBI_MAP_UNLIT=1`,
  `SHINOBI_MAP_TEX=<stem>` (one texture on every piece: a UV check).
- **Colour-grading LUT (2026-10-10, src/grading.rs + grading_lut.wgsl)**: the draw params'
  `ColorGrading[Yebis]` "LutSourceId" picks a LUT per time of day (m11_01: 0 at 0/6/18 h, 20
  at 2/22 h, 30 at 12 h; nearest key, `MapLighting::nearest`). The exporter writes every
  `m11_01_cgrading00<n>` of `map/m11/m11_cgrading.tpf` as `map_<id>_lut_<n>.dds` (json
  "luts"): a 16x256 RGBA8 strip, 16 slices of 16x16, x = red, row = blue * 16 + green, display
  colour in, graded colour out. 0000 is a slight warm grade (blue down to -31/255), 0010 warmer,
  0020 cool, 0030 near identity (blue +15). The game puts `ColorGradingLut` on the camera; the
  pass is Bevy's custom post-process pattern (`ViewQuery`, skipped on views without the
  component), after `tonemapping` in `Core3dSystems::PostProcess`, on the Rgba16Float HDR
  target: sRGB-encode, look the slice pair up (blue by hand, red/green through the sampler),
  decode. `SHINOBI_MAP_NO_LUT=1` turns it off.
- **Auto exposure (same day, `map.rs auto_exposure`)**: Bevy's `AutoExposure` with a
  compensation curve from the `Tone Map[Yebis]` group: target EV(L) = base + Exposure +
  clamp(MiddleGray + METER - L, Adaption Min, Adaption Max), L = the metered average log2
  luminance; the curve c(L) = target + L is a 4-point `LinearSpline` (points snapped to 1/64 EV:
  Bevy rejects the spline when a segment's end sample is not bit-exactly the next point).
  `METER_EV` 2.6 was measured: with it the 18 h gate meters at adaptation 0 (mean luminance
  equal to the fixed-exposure reference within 0.03 EV; `scratchpad pngmean.py` on `myshot`
  screenshots); `BASE_EV` -0.8 cancels the 18 h Exposure so dusk keeps the look. Noon is a
  fixed exposure (adaption +-0.01, 0.3 EV under the old look). Knobs: `SHINOBI_MAP_METER`,
  `SHINOBI_MAP_EV` (add EV), `SHINOBI_MAP_NO_AE=1`. Param names: match "ToneMap-Exposure", a
  bare "Exposure" finds "AutoExposure Adaption Max" first.
  At 0 h the set's Adaption Max 8 read as +8 EV washed the gate out white, so the brightening
  is capped at `ADAPT_CAP_EV` 2: the 0 h gate then sits 0.9 EV over the fixed exposure, dark
  blue and readable like the game's night (22 h alike; 2 h is as bright as 6 h by its own
  set). Yebis's Adaption Max is probably not a plain EV clamp. Adaptation speeds are Bevy's
  defaults.

### Sandbox map (2026-10-10, src/sandbox.rs; "sandbox to test everything: textures, enemies, lighting")
- `[world] map = "sandbox"` (or `SHINOBI_MAP=sandbox` for one run): a 90 x 90 m flat floor with
  5 m walls, built in code, under a real map's lighting: `SHINOBI_SANDBOX_SRC` (default
  m11_01_00_00) names the map whose `map_<id>.json` light set, GI probes, LUTs and auto
  exposure it uses, and whose `map_<id>.bin` supplies the materials (only the batch headers
  are read; the pieces are not spawned). Loads in 0.1 s.
- The gallery: one leaning panel (2 m wide, 45 degrees back) per material of the source bin,
  242 for the gate, in rows of 20 ahead of Wolf (-Z, from z = -10, 3 m per row, most
  triangles first). A panel's blend bytes run overlay 0 -> 1 left to right and snow 0 -> 1
  bottom to top (the lean gives the snow its up-facing surface), layer C at the real map's
  constant 0.5, so every layer of every material shows on one panel. The floor takes the
  biggest ground / soil material (`SHINOBI_SANDBOX_FLOOR=<albedo stem part>`), its overlay
  rising towards +X and its snow towards -X; the walls the biggest stone wall
  (`SHINOBI_SANDBOX_WALL`). `extracted/map_sandbox_panels.txt` lists row / column -> material.
  `SHINOBI_SANDBOX_AT=x,z[,yaw deg]` stands Wolf there at the start (row r is at z = -10 - 3r;
  stand 5 m in front of it for a screenshot). `SHINOBI_SANDBOX_NO_GALLERY=1`: floor and walls
  only; `SHINOBI_SANDBOX_PANEL_SHADOW=1`: the panels cast shadows (off by default: 242 casters
  in two cascades).
- Enemies: the config line-up as always, plus `SHINOBI_SANDBOX_ENEMIES=c1020,c1010` (a list of
  chr ids, any kind with an `extracted/enemies/<chr>.json`: data.rs adds them to `Combat.kinds`
  for the sandbox) or `=all` (page 0 of the roster, 20 per page, `all:<n>` the n-th page,
  `all:*` every kind, `SHINOBI_SANDBOX_PAGE=<n>` another page size). They stand behind Wolf at
  +Z facing him, standing still (`EnemyDebug.mode` Idle; the passive mode walks them up to
  Wolf), spaced by their NpcParam hitRadius (1 m between bodies, 3 m at least, the next row
  clear of the widest body) from 9 m back to the back wall. Kinds wider than `MAX_RADIUS`
  2.5 m (c5050, c7020) are left out of pages and logged; name one alone to see it. Collision:
  the floor and walls go through `Terrain::from_tris` (the same walls / floor / step code as
  the hit files).
- The catalog: `SHINOBI_SANDBOX_SHOTS=<dir>` walks the line-up after a 4 s warm-up, stands
  Wolf in front of each enemy (1.2 m to the side, 3.5 m back plus 3 m per metre of hitRadius
  over 0.5, capped at 14; camera pitched 6 degrees), waits 1.5 s and saves
  `<dir>/<idx>_<name>.png`, then exits. An enemy that walked more than 5 m from its spot is
  skipped. `SHINOBI_SANDBOX_WATCH=<sec>` logs the debug AI mode, Wolf and every enemy's
  state / stealth state / position once at that game time (checked: the line-up still stands
  at 24 s; when it "fought" the window had the user's keyboard). Pages are shot one run each
  (`all:0` .. `all:5`); the scratchpad `sheet.ps1`
  tiles a page's shots into a contact sheet (crop 270,30,540,520 of the 1024x560 window).
  2026-10-10: all 102 kinds shot and the sheets reviewed: no missing / magenta textures on
  the human-sized kinds. The scene-sized bosses (c5xxx) fill the frame whatever the distance:
  their hitRadius is small next to the mesh, so they are framed like a man.
- Lag with the line-up (2026-10-10, "the game lags when they all spawn"): 20 enemies ran at
  22 fps, all of it cloth (41 ms a frame). Fixed in src/cloth.rs (details in
  kb/visuals.md, "Cloth cost"): page 0 now 48 fps, the empty sandbox 69, the gate map 59.
- Lighting walk: `[` / `]` move the hour back / on by one on any map (the light set, probe
  blend, LUT and exposure curve re-apply; the combat log shows the hour). `SHINOBI_HOUR` still
  sets the start.
- Code shared with the real map (src/map.rs): `walk_bin` (the SHMP batches, versions 1-4),
  `MaterialKey` / `MaterialCache::get` (the layered material for a key), `MaterialOptions`
  (the SHINOBI_MAP_* knobs), `tangents`, `Terrain::from_tris` / `install`.
- Seen on the gallery (2026-10-10): the layer blends read as intended at 18 h; at 0 h the snow
  layer (m11_snow_01 / _20, near-white albedo) blows out white under the night "sun" (weight 2)
  plus the +2 EV adaption cap, while the floor stays readable. Open: whether the game's snow
  is darker (a snow colour param, or the sat shader scaling the layer) or the night light set
  is read too bright. The data behind it (2026-10-11): light set 100 at 0 h is a "moon"
  DiffColor0 (0.69, 0.76, 0.83) weight 2.0 at 20 deg (as strong as the 18 h sun's 2.0), the
  hemisphere colours exist for set 0 only, where 0 h is black (weight 0), so the night has no
  ambient at all; Tone Map Middle Gray is -3.0 at 0 h against -6.6 at 18 h (+3.6 EV target)
  with Adaption Max 8, so the exposure runs to ADAPT_CAP_EV. Sets 110 (a 3.0 dusk sun) and
  120 (a flat grey 1.0, no angle) are the map's other variants. No m11_01 scene of the game
  is lit at 0 h (the Outskirts is a dusk area), so this stays a sandbox-only look until a
  night map (m11_00 castle, m10_00 Hirata) is compared; nothing to fix from the data.

### Open
- Lighting is now the game's own light set + GI probe at 18 h (see above); left: the LUT and
  auto exposure, and a blend between probe variants for in-between hours. (Material names like
  "本城の遠景モデル" on the gate parts m00202x are reused castle materials, not far-view
  stand-ins.)
- **Frame rate (2026-10-10)**: the game logs "map frame rate" once over seconds 5..15 (no map:
  103 fps). The sun's shadow cascades were the cost, not the triangles: radius 80 / 6.86 M
  tris 57 fps, radius 60 / LOD 20,40 / 4.13 M 60, no shadows 111, 2 cascades over 60 m 70,
  shadow map 1024 no change. Now: `MAP_SHADOW` (45 m) tags far batches as non-casters
  (`NotShadowCaster`, bin format version 2 = a caster byte per batch) and the sun defaults to
  2 cascades / 60 m (`SHINOBI_MAP_SHADOW=<cascades>,<m>`, `SHINOBI_MAP_NO_SHADOW=1`,
  `SHINOBI_MAP_SHADOW_RES=<px>`): 83-86 fps (3 cascades 71). Splitting batches into 20 m cells
  (`MAP_CELL`) for culling made it worse (1016 meshes: 39 fps, 86 without shadows): the
  per-entity cost beats the culling, so it is off. Measurements are +-8 fps run to run.
- **Far view** (2026-10-11, exporter `MAP_FAR=<m>`, off by default): the map's distant-view
  models (m6xxxxx) and sky / cloud pieces (m9xxxxx) within that range join at their lowest LOD
  (no shadows past MAP_SHADOW). A gate probe at 600 m added 19 pieces, 0.2 M triangles, no
  frame-rate change (54 fps with the Ape either way), but the gatehouse is closed at both
  ends, so nothing of it shows from the fight; the gate stays exported without it. For an
  arena with an open horizon the switch is `tools/boss_arenas.py` FAR (map -> metres; the env
  `MAP_FAR` overrides): the Divine Dragon's m25 at 3000 m adds 67 pieces and 0.42 M
  triangles to each of its two arenas (bin 7.5 -> 53 MB, load 0.1 s) and puts the snowy
  mountains and forests behind the cloud sea in place of a flat band. gap: a small purple
  rectangle on the horizon ahead of the start: the horizon band (purple at the horizon, blue
  above: what the arena shows without any far piece, i.e. the sky behind everything) seen
  through a gap between two far mountain pieces. `SHINOBI_MAP_SKIP` runs placed it (skipping
  branch_02 / cliff_00 / treebark_10 / mountain_01 removes every mountain and the band shows
  whole; skipping the rocks, the mist planes or the ridge ring m692000 / sky_105 does not close
  it; a one-mip sky_105 changes nothing); `MAP_FAR=6000` adds no piece (all 67 far pieces lie
  within 3 km). The game hides that horizon with its fog and sky colour; ours is the probe's
  purple haze. Not chased further.
- Tests: `cargo test --release map::tests` (`arena_floor_and_walls` needs the exported .hit;
  `arena_probe -- --ignored --nocapture` prints floors, ceilings, wall distances and slides
  around the spawn spots). `slide` sub-steps are capped at half the body radius: with the old
  cap of 32 steps a long move tunnelled through the gate posts.
- **Layers (2026-10-10, `MapMaterial` in src/map.rs + src/map_material.wgsl)**: the map's sat
  shader materials stack texture layers. `sekiro-extract mtd <file>` dumps an MTD; M[Multiple]
  slots: 8 albedo / 3 normal / 9 mask = base, 5 / 4 / 6 = overlay (moss, a second stone, wood
  variant), 7 / 0 / 17 = layer C (often the base albedo again with a rockier normal), 12 / 10 =
  snow; M[MultipleGround]: 8 / 3 base, 5 / 4 overlay, 11 / 7 snow. The FLVER vertices carry a
  UVPair (two UV sets) and six UByte4Norm "UV" members: greyscale blend weights
  (`sekiro-extract vblend <flver>` prints their ranges per mesh). Read off the data, not the
  shader (the compiled sat shaders were not decompiled): byte 0 = overlay weight (a wall whose
  overlay is its own base albedo is painted 254 throughout), byte 4 = snow (143 constant on
  "wood + snow", 100-170 on roof ridges, patches on walls), byte 2 = layer C weight (127
  constant where C is the base albedo with another normal: a 50 % normal blend; 0..255 on the
  gate's wood, where C is a lighter plank texture). Byte 1 (0..184 on the stone walls) and the
  rare RGB-coloured byte 3 / 5 members are not used. Bin format version 4 carries nine
  texture names per batch (base, overlay, snow, layer C albedo / normal, the base `_3m` mask)
  and 44-byte vertices (pos, normal, uv, uv2, bytes 0 / 1 / 4 / 2);
  the game puts uv2 in `ATTRIBUTE_UV_1` and the weights in `ATTRIBUTE_COLOR`, the shader
  samples the base albedo itself (Bevy would have multiplied the vertex colour in), blends
  albedo + normal + gloss per layer, the snow also by how much the surface faces up
  (`SHINOBI_MAP_SNOW=<start>,<full>` on N.y, default 0.2..0.7). Knobs: `SHINOBI_MAP_NO_LAYERS=1`
  (base only), `SHINOBI_MAP_SHOW_BLEND=1` (weights as colour: red overlay, green snow, blue
  byte 2), `SHINOBI_MAP_BYTE2=c|ao|off` (byte 2 as layer C, as a darkening, or ignored; "ao"
  halves the whole right gate wall, which is why "c" is the default). Which UV set each layer
  uses is a guess (overlay uv0, snow uv1). **Masks**: the `_3m` textures (slot 9 base, 6
  overlay, 17 C; DXT1, three independent greyscale channels: on the stone wall R is high on
  the block faces and low in the mortar, G the mortar lines, B lichen-like spots; on wood R
  follows the planks with the gaps low) are read as height in R: `coverage()` in the shader
  makes a layer win where its vertex weight exceeds a threshold within `SHINOBI_MAP_SOFT`
  (0.25; 0 = plain weights) of it, overlays and layer C at threshold = height (they fill the
  recesses first), snow at 1 - height (it settles on the raised faces first). Only the base
  mask is bound (bind-group texture count); the layers' own masks are not. 72-80 fps with the
  layers (86 without).
- Stairs / wall feel, enemy spawn height on slopes, the camera against low ceilings: not yet
  checked in play.

## 2026-10-11 Every boss in its own arena (tools/boss_arenas.py, src/map.rs boss_arena)

- `sekiro-extract map` takes the centre as a part name or an MSB **entity id** (MSBS part
  entity data at `o + u64(o + 0x60)`, EntityID at +0; `msb::Part.entity`), and `MAP_NAME`
  names the output (`map_<MAP_NAME>.bin/.hit/.json/_env_XX/_envd_XX/_lut_XXXX`).
- `tools/boss_arenas.py` reads boss_scripts.json (each boss script's map and boss entity) and,
  one map at a time: unpack the map + envmap + area textures + draw params, `bxf` the hit,
  export each boss (`MAP_NAME=boss_<NpcParam row>`, 60 m, `MAP_LOD=20,40`), unpack the objects
  the export lists and export again, then delete the raw map (about 2 GB each; m11_01_00_00
  stays for the General's arena). A boss standing within 40 m in another's cast shares that
  arena (Butterfly 50900001 -> 50900000, Tomoe 71100000 -> 71001000).
  `extracted/map_boss_arenas.json` = {row: stem}.
- Game: `map_id` with `[world] map` on (not the sandbox) takes `boss_arena(chr, npc_row)`
  (the row: config, else the chr's roster default) when its `.bin` exists. `SHINOBI_MAP`
  still overrides. `map::on_boss_arena()` lets `events::place` keep the cast's heights.
- **Collision under the centre** must draw something: the Headless Ape 1700850 stands on
  h900707 (a helper collision, all draw groups 0) above the cave floor h005500; the nearest
  floor with any draw group is taken.
- **Helper planes are not floors** (2026-10-11, `Terrain::from_tris`): the h9xxxxx helper
  collisions without a hit material (u32::MAX) contribute level triangles that are not
  walkable ground: the Guardian Ape's h900702 is a 66 m plane 0.4 m over the pond floor
  (Wolf stood on it), the maps' kill planes sit 20-50 m below. Level material-less triangles
  are dropped at load (logged with the triangle count); their walls (h900700, the arena's
  bounds) stay. `MAP_LOG=1` on the export prints every hit part with its vertex range, so a
  `.hit` triangle's first vertex index names its part.
- **Winding** (2026-10-11, the "white sheet" under Wolf on the Ape arena): every drawn map
  surface was inside out. `BinBatch::indices` swapped the triangle order after the X mirror,
  as the hit loader does, but the FLVER data is DirectX clockwise and the mirror alone makes it
  counter-clockwise: the swap turned front faces into back faces. Closed rocks, planks and
  walls still looked right (their far walls were seen from inside, lit by the real vertex
  normals), only open floors vanished: the Ape arena's pond bed (m700500 ground_903 and the
  m700000 underwater ground) was back-face culled, and the "sheet" was a mud plane of the
  valley 50 m below and 60-300 m behind (m519000, a two-sided blend batch) seen through the
  missing floor. Found with `SHINOBI_MAP_ONLY=#34` + `SHINOBI_MAP_TWO_SIDED=1` (the floor
  appeared) and `SHINOBI_MAP_PROBE=0,10.5` (the final triangles' face normals pointed down on
  the Ape floor and on the gate map's plank tops alike). Fix: no swap. The same mistake hid
  the Guardian Ape pedestal's far-view shell (m700850_7000 mesh 3, a copy of the floor and
  walls a metre above the real ones): seen from inside, a correctly wound shell is back-face
  culled, which is the game's "rule" that was not found on 2026-10-11 by groups, LODs, node
  bindings or events. The exporter's HIDDEN_MESHES list is empty again (`MAP_SKIP_MESH` stays
  as a knob). Models (model.rs) keep their swap: their exporter writes them the other way
  round and the characters are right.
- **LOD face sets** (2026-10-11, flver.rs): FaceSet flags are 0 full, 0x01000000 LodLevel1,
  0x02000000 LodLevel2, 0x80000000 motion blur; the exporter matched 1 and 2 until then, so
  `MAP_LOD` never applied (the Ape arena went from 2.96 M to 1.68 M triangles). Triangle
  strips follow SoulsFormats `Triangulate`: the winding flips every step, degenerate steps
  included, and starts over at a primitive-restart index (the old index parity put strips
  after an odd restart inside out). A face set with CullBackfaces off (byte 5 of the set) is
  exported two-sided whatever the MTD says (the Ape's water and plants). A part with a
  mirroring scale (odd negative axes) gets its winding and normals flipped; none on the Ape
  arena. `FLVER_LOG=1` prints each mesh's material, node, face-set flags and fallbacks,
  `MAP_LOG=1` each hit part (vertex range, materials, groups), piece (pos, rot, scale, LOD,
  groups) and `piece-mesh` (LOD, albedo, MTD, triangles, cull).
- **Missing textures** (2026-10-11, `MaterialCache::get`): a texture the export did not write
  (m17_rock_92_3m on the Ape arena) left Bevy's material unprepared for ever, so its batches
  never drew. A missing file now drops that layer with a warning (once per texture per load).
  The export searched every m17 pack and other/maptex.tpf and found 220 of the 221 textures
  the Ape arena's FLVERs name: `m17_rock_92_3m` is not in the game's files at all (the `_a` /
  `_n` are), so the game itself runs that material without its mask (whatever its fallback
  is; gap) and the warning is expected, not a broken export. The gate map's one missing
  texture is the same kind: `m11_ground_101_n` (a normal map named by a material whose
  albedo is another texture; neither `m11_ground_101_a` nor `_n` is in the m11 packs).
- **Debug knobs** (src/map.rs, 2026-10-11): `SHINOBI_MAP_SKIP=<text>[,#<batch>]` leaves out
  batches by texture name or index, `SHINOBI_MAP_ONLY=#<batch>[,...]` loads only those,
  `SHINOBI_MAP_TWO_SIDED=1` turns culling off, `SHINOBI_MAP_PROBE=x,z` logs the final
  triangles over a spot with their face normal, `SHINOBI_MESH_OWN_SLABS=1` gives every mesh
  its own GPU buffer (allocator suspects), `SHINOBI_SANDBOX_WATCH=<sec>` also lists the
  non-map meshes near Wolf, the map pieces whose bounds reach under him (with visibility)
  and every map material texture that is not loaded. `SHINOBI_PHOTO=<dir>` (photo.rs) is the
  repeatable camera for map comparisons: the desktop grabs of the game window vary with the
  camera's settle and the window's size.
- **Water** (2026-10-11): the pond and stream of the Ape arena (batches with no albedo and
  the `m99_water_n` normal; the gate's river at -20 m is the same) drew as an opaque grey
  floor, so Wolf stood "knee-deep in the ground" (VXf's report): the hit floor is the pond bed
  under it, as in the game (the water surface is only the h900702 helper plane). Stand-in in
  `Materials::get`: water batches are dark (0.10, 0.15, 0.16) and 60 % translucent with the
  ripple normal. gap: the game's water shader (reflection, refraction, flow), foam
  (`m11_00_waterform_a`) is untouched.
- **Wolf's start on a boss arena** (`map::player_start`, used by the reset in player.rs and
  by `settle_player_start` at PostStartup): the arena's player spot (0, 4) is 8 m in front of
  the boss's MSB placement, and the Guardian Ape sleeps in a grotto whose rock (h019400) is
  1.5 m over that spot, so the camera had no room (VXf's report: the arm clamped to 0.3 m).
  The start walks +z half a metre at a time to the first spot with a floor within 0.5 m of
  the arena's and 2.8 m of headroom (the Ape: z 10.5). Since 2026-10-11 the floor is
  followed from the enemy's spot (0, -4), 2 m of step at most and the nearer of a ledge and
  the ground under it to the last floor: the Fountainhead bridge (boss_50001000) rises 0.25 m
  per metre toward Wolf (1.7 m at z 4, the fixed 1 m search top found nothing and Wolf stood
  under the deck), so `player_start().y` is now his real floor height, not
  CAPSULE_HALF_HEIGHT. A wall across the walk ends it: the Headless Ape's MSB pose faces the
  pit's wall 3 m off (at (0, 4) the only floor is 11.5 m up) while his fight region 1702870
  runs 43 m the other way, so `tools/boss_arenas.py` OVERRIDES turns that arena round
  (exporter `MAP_CENTRE_YAW=180`). The Divine Dragon's MSB spot (c5200_0000, and the Old
  Dragons' c5300 row that shares the place) sits in a cluster of rock and tree pieces that
  draw as grey shards, on the cloud floor h003100 (materials 0 and 40, the only collision
  within 280 m of y 0), so `MAP_CENTRE_DIST=24` puts both fights 24 m ahead on open cloud.
  The "shard cluster" (fixed 2026-10-11) was the tree object o258000's cherry blossoms
  (material c5200_cherryblossoms, 13.9 k two-sided alpha-tested triangles) drawn with no
  albedo: the FLVER names its slots after the `[Wind]` variant of the Character_AMSN shader
  ("Character_AMSN__Wind__snp_Texture2D_2_AlbedoMap", empty path) while the MTD is the
  `[Gradient]` variant with "..__Gradient__snp_Texture2D_1_AlbedoMap_0" (m25_dragon_
  cherryblossoms_a), slots 2 / 4 a withered set and slots 0 / 7 two masks, so the exact
  slot-name lookup found nothing. The exporter's `choose` now falls back, only when no slot
  name matches at all, to the MTD's lowest-numbered albedo / normal `_0` slot (the bloom).
  gap: the gradient between the bloom and the withered set (a shader float). The cloud sea
  m600000 "雲海_中央" (M[Flow]_Smoke, no albedo; a height / blend mask `m25_00_smoke_g` in
  slots 11 / 5 and a flow vector map `m25_00_SmokeFlow_v` in slot 12, both on uv set 1;
  g_BlendMode 2, emissive, forward) was a white plane until 2026-10-11: the exporter's `choose`
  keeps that pair as mask + "normal" with alpha 3 ("flow smoke"), and map.rs draws it unlit
  and blended with `map_material.wgsl` `smoke`: the mask sampled at two phases of the flow
  half a cycle apart and cross-faded (the flow-map trick, 6 s cycle, flow 0.06 uv), drifted by
  the MTD's two float2 rates ((0, 0.01), (-0.02, 0)), shaded from the MTD's color_3 (0.67,
  0.71, 0.76) to color_2 (0.78, 0.86, 0.98) by the mask, alpha from the mask (0.03..0.4).
  The mask is a soft grey cloud (R 0.61..0.88), stretched to the full range in the shader; the
  mesh's uv spans -1..2 over the 490 m plane, so the mask is tiled 8 times per unit (20 m a
  repeat; `SHINOBI_MAP_SMOKE_TILE=<n>`). `SHINOBI_MAP_SMOKE_SPEED=<x>` scales the pace. gap: the SPX shader's own math (its floats
  0..13, uv set 1 vs 0, the view fade) is not decoded; the look is by eye. `SHINOBI_START=x,z` puts Wolf on another spot of the arena
  (probing). gap: the game starts the fight where the boss script's warp or the fog gate
  puts Wolf.
- **Which bosses get an arena** (the user, 2026-10-11: "do one arena for the bosses that
  don't need special arena and do arenas for bosses that need them ... Isshin boss arena, the
  one with flowers"): `tools/boss_arenas.py` OWN_ROWS exports an arena only for the fights
  whose mechanics live in their surroundings: Corrupted Monk (50000000 Mibu phantom spots,
  50001000 the Fountainhead bridge), Lady Butterfly (50900000/1, the illusions' spots),
  Guardian Ape and Headless Ape (51000000, 51000100: the valley's pond and grotto), Divine
  Dragon and the Old Dragons (52000000, 53000000: the cloud sea, the lightning trees). Every
  other boss row maps to `boss_54000000`, Isshin the Sword Saint's silvergrass field below the
  castle (`extracted/map_boss_arenas.json`); the ten other bins were deleted (2.9 GB). On the
  field a stranger boss is not on its own arena (enemy.rs `on_own_arena`), so the line-up
  stands 8 m ahead of `player_start()`, whose search then also wants a floor and headroom at
  that spot: the field falls 0.27 m per metre toward Wolf (the boss spot is 1.3 m above his),
  so that spot may differ by 2 m in height; Wolf's own spot keeps the 0.5 m band of the Ape
  fix. Beyond z 10 the field's centre line has no floor (a drop), so the search stays short.
- **Map switch at run time** (`switch_map`, 2026-10-11, Bosses' request for the encounter
  picker): a `SpawnEncounter` message with the config's `[world] map` on (not the sandbox, no
  `SHINOBI_MAP`) despawns the pieces and the two lights, clears the terrain and runs
  `load_map_id` for the new boss's arena (`boss_arena`) or the config map; the fresh
  `MapLighting` makes `apply_lighting` re-dress the camera (fog, probe, sky box, LUT,
  exposure), Wolf is put on `player_start()`. `map::loaded_map()` tells the map in force.
  Load is 0.3-0.5 s; the old pieces' meshes and materials are dropped with their entities.
- **Objects** (MSB part type 1, model `o######` -> `obj/o######.objbnd.d/o######.flver`): drawn
  with the same draw-group rule, rigid meshes moved into model space (flver.rs
  SEKIRO_FLVER_REF_POSE), textures from the objbnd's `.tpf` or the area packs. The Divine
  Dragon's arena is almost only objects: its collision h003100 draws group 0 bit 31, which
  only the cloud sea m600000, water and o258000 (the ground, 129 k triangles) carry.
  `objects to unpack: ^/obj/(...)\.objbnd` is printed for what is missing (run `unpack`
  with `MSYS_NO_PATHCONV=1` from Git Bash, or it rewrites the leading `/`).
  Objects' own collision joined the `.hit` on 2026-10-11 (section below); gap: breakable /
  animated objects are drawn in their bind pose.
- `MAP_LOG=1` now also lists the parts within the radius that the draw groups leave out.
- Size: 60 m arenas are 60-130 MB, the castle top (m11_01: Owl, Genichiro, Emma, Isshin's
  rows; ~8 M triangles) about 400 MB each. They overlap (the same roof within 10 m) but each
  is centred on its own boss.

## 2026-10-11 Grapple points (docs/kb/grapple.md; tools/sekiro-extract map.rs export_grapple, src/map.rs)

- The exporter writes `"grapple": [{hook, land, fwd, var, src}]` (arena space, metres) into
  every `map_<id>.json`, first key, from the grapple-type (1) records of each object's
  `obj/<model>.objbnd.d/e<id>.edge` within the radius + 30 m (hook = the midpoint of V1/V2,
  land = V3, `fwd` = the part's facing, `var` = the record's variation byte: 1 common, 3, 5,
  20, 40/41 kites) and of the map's own `<id>.edgebhd/.edgebdt` (one `.edge` per collision
  part, local space; no map of ours has a grapple edge there: the gate map has 963 hang +
  1326 hug, Hirata m10_00 393 + 864). The `o000100` "marker" objects are item spots, not
  hooks: all 40 near the General are MSB Treasure events (`msb.treasure_parts`, EventParam
  type 4 TreasurePartIndex; o000100_0073 = entity 1111510, event 11115652), 0.25 m over the
  floor; every treasure part is skipped. "grapple points: N (object edges, map edges;
  treasures skipped)" on the export. Counts: gate map 113; the boss arenas are in
  `docs/kb/grapple.md`. `tools/boss_arenas.py -force` re-exports the arenas that already
  exist (after an exporter change). Since later on 2026-10-11 the hang (2) / hug (3) records
  of the same files within the radius + 10 m go out raw as `"ledges"` ({a, b, c, kind, var,
  src}) and load into `map::LedgeEdges` (H gizmos: hang orange, hug cyan; gate map 40: 1
  hang, 39 hug). Nothing consumes them yet (kb/grapple.md section 1).
- Game: `map::GrapplePoints` (resource, `GrapplePoint { hook, land, fwd, variation }`, X
  mirrored on load like every model; empty in the sandbox: its arena is generated) and
  `map::grapple_target(cam_pos, cam_fwd, wolf_centre, prev, &points) -> Option<GrappleTarget
  { index, point, dist, in_action_range, ride }>` = WirePointSearchParam 0 (find 25 m, act
  3..15 m + the variation row's extendWireActionDistance; the camera-plane rectangle 14.3 wide
  x 10.2 up/down, the soft-lock one 7.15 x 10.2 that keeps `prev`; nearest to the view axis
  wins) with the WireVariationParam recognition angles (the player's side of the point's
  facing) and a `raycast` from Wolf's chest to the hook (a collision triangle in between hides
  the point). `ride` is false for the "can't ride" rows 20/23/24 and the kite row 41. Gaps
  marked in the code: whether the rectangle is metres or screen fractions, which side "left"
  is after the X mirror, whether the angles read from the hook or the landing point.
- Debug: with the hurtbox overlay (H) every point is a green hook sphere, a line to its landing
  spot and a blue facing tick; the current target is yellow (grey when out of the action range)
  with a line from Wolf. The player's wire move, HUD marker and rope are VXf's (src/wire.rs),
  enemy targets Bosses' (`enemy::grapple::EnemyGrapples`).

## 2026-10-11 Objects' own collision in the .hit (tools/sekiro-extract map.rs, hkx.rs decode_convex)

- Every object drawn within the radius adds its objbnd `<model>.hkx` collision to the map's
  `.hit` ("N with their own collision" on the export; `MAP_NO_OBJECT_HIT=1` leaves them out).
  The shapes: hknpBoxShape / hknpConvexPolytopeShape (hkRelArray `vertices` of hkVector4f,
  `faces` {u16 firstIndex, u8 numIndices, u8 minHalfAngle}, u8 `indices`; fans per face),
  hknpCapsuleShape / hknpSphereShape (1-2 vertices + convexRadius: an 8-sided prism extended
  by the radius) and fsnpCustomParamCompressedMeshShape (the map decoder). Census of the 234
  objbnds with an hkx: 93 one box, 52 one polytope, 61 compressed meshes, the rest mixes.
  hkRelArray members are a u32 item index (HKLib ArrayFormatHandler ReadHkRelArray):
  `Tagfile::deref_rel`.
- Hit material: the compressed meshes carry per-triangle ids; the convex shapes none (their
  hknpMaterial userData is 0), so the triangle takes **ObjectParam materialId** ("treated the
  same as the floor material": 4 wood for the trees o003610/o003600, 10 straw, 11 iron, 16
  tile, 807 for o115410), 29 "None" when the row says -1, so the loader's helper-plane rule
  (material-less level triangles are dropped) leaves them alone. ObjectParam collisionType
  (0 / 1 / 4 / 8 / 10 here) is not applied: Sekiro's enum is not in Paramdex (gap; AC6's
  reads 0 normal, 1 no camera, 2 no camera/lock/damage, 3 camera only, 4 damage only).
- Not for the MSB Treasure parts (the o000100 item markers; they have no hkx collision anyway).
  Check of the walkable floor (`object_collision_floor_diff`, ignored test: the gate .hit
  against a `MAP_NO_OBJECT_HIT=1 MAP_NAME=gate_noobj` export): 116 of 5990 half-metre spots
  within 20 m of the fight stand higher, all on solid things (the banner post o116000_8000
  3 m beside the General, 1.27 m; rocks o113400; a roof at (-4, -18)); the fight spots and
  the courtyard floor are unchanged. Vertex / triangle indices in the game's `Terrain` differ
  from the file's (helper planes dropped): map a triangle to its object through the vertex
  index and the export's `verts N..` (MAP_LOG).
- Check: `cargo test --release grapple_floor_probe -- --ignored --nocapture`: 84 of the gate
  map's 113 grapple landing spots have a floor within 1.5 m under them (none before), so Wolf
  stands on the branch or roof after LandWireMove instead of VXf's 0.6 m perch. Gate map:
  182 of 231 objects contributed. The arenas were re-exported with it.
