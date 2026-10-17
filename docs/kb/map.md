# map: the game's own arena (MSB, map pieces, hit collision, draw params)

<!-- index -->
Contents (line numbers of this file; read a range, not the whole file):
- 28: 2026-10-10 The real map around the General (tools/sekiro-extract/src/map.rs, src/map.rs)
- 34: Pipeline (all in `tools/extract.ps1`, block "Map")
- 55: Formats (readers in tools/sekiro-extract/src/)
- 85: What the exporter does
- 106: Game (src/map.rs)
- 177: Sandbox map (2026-10-10, src/sandbox.rs; "sandbox to test everything: textures, enemies, lighti
- 239: Open
- 315: 2026-10-11 Every boss in its own arena (tools/boss_arenas.py, src/map.rs boss_arena)
- 569: 2026-10-11 Grapple points (docs/kb/grapple.md; tools/sekiro-extract map.rs export_grapple, src/
- 603: 2026-10-11 Objects' own collision in the .hit (tools/sekiro-extract map.rs, hkx.rs decode_conve
- 634: 2026-10-12 The sat layer shaders, decompiled (M[Multiple] / M[MultipleGround])
- 733: 2026-10-12 ObjAct doors left out; teleports skip the wall slide ("bug fixes" session)
- 752: 2026-10-12 The bosses fight on the castle top
- 765: 2026-10-12 Boss fog walls (tools/fog_walls.py, src/fog_wall.rs)
- 778: 2026-10-12 The game's height fog (src/height_fog.rs, height_fog.wgsl)
- 794: 2026-10-12 Sky domes and the map's bloom (src/sky.rs, map.rs map_bloom)
- 807: 2026-10-13 SkyDome_Ver2 decoded from the game (shader/shaderbdle.shaderbdlebnd.dcx ->
- 825: 2026-10-13 the same packing for the map layer colours: the exporter's `game_params` now writes
- 838: 2026-10-12 Fog-wall collision and GIIV irradiance volumes (VFX and Maps)
<!-- /index -->

Dated findings, oldest first. Append new entries at the end.

## 2026-10-10 The real map around the General (tools/sekiro-extract/src/map.rs, src/map.rs)

The fight now takes place where the General (c1020_0004, NpcParam 10203010) stands in
m11_01_00_00 (Ashina Outskirts, the big gate): his pieces, hit collision, draw params. Everything
comes from the game's own files; nothing is modelled by hand.

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
  with the sun disk baked in) and `giiv<probe>_<v>_{w,x,y,z}` BC7 3D irradiance volumes (used
  since 2026-10-12, see the end). The probe is the MSB region around the arena: `Env_Box<probe>_...`
  (EnvironmentMapEffectBox, type 17) else the nearest `Env_Point<probe>` (type 2). Boxes are
  bottom-centred (2026-10-12, msb.rs `contains`): they stand on their position as DSMapStudio
  draws them, and sekiro-rs 9ee18b0 (lighting.rs ~584, SHADERS.md "GIIV") places the giiv
  volumes the same way. With that rule the gate origin is 4.4 m under Env_Box380's floor and
  the pick is Env_Box280 -> `gilm0280` (before: centred, `Env_Box380_ハブ櫓` -> `gilm0380`). The
  exported `map_*_env_*.dds` still hold the old pick until the next map export. `cubemap.rs` decodes BC6H
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
  at 24 s; when it "fought" the window had keyboard focus). Pages are shot one run each
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
- Lighting is the game's own light set, GI probe, LUT and auto exposure; in-between hours blend
  the two probe variants around the hour (`probe_maps`, RGBA16F texel blend). (Material names like
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
  Castle top (boss_71001000, 5.3 M triangles, 2026-10-12): 48 fps, 105 without shadows, 58
  without cloth, 66 with one cascade. Exporter `MAP_SHADOW_BELOW` (default 6 m): a near piece
  whose top is more than that under the arena floor casts no shadow (the sun is above the
  horizon, so it could only shade ground further down, out of the fight): 629 of the castle
  top's pieces, 519 -> 477 batches, 66 fps with the 2 cascades kept. Only arenas exported since
  have it (the castle top), the gate (34 pieces, 47 -> 48 fps with the General's group of
  three: its cost is the enemies' cloth and shadows, not the map) and the silvergrass field (35).
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
- **Arena start check (2026-10-11):** the search runs on any terrain (`map::arena_start`), and
  the test `every_arena_start_stands_and_walks` loads every extracted/map_boss_*.hit: a floor
  under the boss's spot, a start found without fallback, and four 3 m walks from it (+-x, +-z,
  0.15 m slides as in play) that never stand over a hole. After sekiro-rs's map_baseline
  (every player start of every map walked four ways). All 8 arenas pass: starts at z 4
  except the Guardian Ape (z 10.5); drops at most 0.3 m; another boss's line-up fits on all
  8. sekiro-rs's own collision-hole scan (docs/COLLISION-HOLES.md) confirmed holes 160-390 m
  from every area we export, so none are in play.
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
- **Which bosses get an arena** (2026-10-11): `tools/boss_arenas.py` OWN_ROWS exports an arena only for the fights
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
- **Castle top** (2026-10-11): Genichiro (71001000) has his own arena on the Ashina Castle
  top, after sekiro-rs `crates/sim/src/arena.rs` ARENAS (burnt floor m015072 skipped with
  `MAP_SKIP_MODEL`); Way of Tomoe (71100000) shares it by the host search. The Owl (50600000) had
  his own export from c5060_0000 too, but that spot is at the platform's edge and the start 8 m
  behind him fell outside the railing onto the roof tiles 2.7 m below, so he is in MOVED as well
  (sekiro-rs starts both fights at player point c0000_0003); the export was deleted (0.4 GB).
  The index's `moved` list (the MOVED rows) makes enemy.rs `on_own_arena` false for them even
  on their own map, so their scripts' place warps are skipped (Bosses, 2026-10-12): before,
  the Owl's CutsceneWarp put Wolf at the outer railing 0.5 s in. `boss_arenas.py` MOVED puts Isshin the Sword Saint (54000000), his opening
  Genichiro (71101000), Emma, the Gentle Blade (74000010) and Isshin Ashina (54300000) on `boss_71001000` too. The silvergrass field
  is still exported as 54000000's map and stays the shared arena of the other bosses. Isshin's
  boss script map (m11_02) is not the arena's, so he is a stranger there: the line-up 8 m ahead
  of Wolf lands on Genichiro's spot (the arena test prints any start the line-up moves).
- **Castle top lighting per fight** (2026-10-12, plan B4). The castle's event script
  (`m11_01_00_00.emevd`, decoded with tools/boss_events.py `emevd_events`; survey script in the
  Map session's scratchpad `lighting_events.py`) switches the block's draw params per fight
  with 2003[88] Set Area Gparam Sub ID (area, block, sub, fade s) and 2003[81] Set Lighting
  (time of day 1 Morning .. 5 Night per DarkScript3's emedf enum, fade s): Genichiro 11115800
  sub 0 + time of day 1 (his cutscenes 11010030/31 with time of day 2 / 3); after him
  11115820 sub 10 (m11_00: 30) + time of day 3; the Owl 11115860 sub 50; Emma 11115910 and
  Isshin Ashina (m11_01_54_10 11115200) sub 40, his time of day 4 (11115930) after his death;
  the ending 11110100 sub 0, time of day 5 then 1; common event 100 sets sub 20 on all of
  area 11. The files are `param/drawparam/m11_01_0<T><SS>.gparam` (T 0..8, SS the sub id);
  sub 40 / 50 exist only as `_0740` / `_0750`, and both differ from `_0000` only at the
  18 h key of light set 100 (sun colour and angle; `_0750` sun x3 from yaw 2.62, `_0740`
  x2.2 from yaw 0.52, pitch -0.26) plus a few fog / bloom values. gap: what the T digit is
  (no Set Lighting value 7 exists; `_0100` / `_0400` only override light set 0) and the
  exe's time-of-day -> key-hour table. Done: exporter `MAP_GPARAM_EXTRA=0740,0750`
  (boss_arenas.py OVERRIDES) writes those files whole as json "drawparam_variants";
  src/map.rs `FIGHT_LIGHTING` picks per (arena, boss row): Genichiro / Way of Tomoe the base
  at 12 h (the real fight is flat grey daylight: the 12 h key's cool weak high sun; 6 h is a
  warm low sun), the Owl `_0750` at 18 h, Emma and Isshin Ashina `_0740` at 18 h; the log
  says "fight lighting: draw params _0750 at 18 h"; `SHINOBI_HOUR` and `[` / `]` still win.
  Judged against the real frames (clips/sekiro_all_bosses, Owl 59:40, Genichiro 25:00):
  still off the same way on all three: ours is flat, ~1 stop too bright, neutral / cool and
  low-contrast; the game has a low warm golden sun, hard long shadows, dark ambient and an
  amber haze sky. That is the general lighting pipeline (exposure, ambient vs sun ratio, the
  hemisphere / probe weight), not the variant choice: the next B4 step.
- **The game's tone curve and colour rules** (2026-10-12, plan B4; the rules traced by
  sekiro-rs docs/SHADER-INPUTS.md, exe functions 310630 / 24d920 / 2508b0 / 24db30, credited in
  THIRD_PARTY.md). In src/map.rs: light and hemisphere colours upload squared (`colour()`:
  `r*r*W`), fog colours byte-quantised then raised to 2.2 (`fog_colour()`), one scene scale
  `SCENE_TO_BEVY = 4500` (sekiro-rs lighting.rs) for lux, environment and ambient, and the
  Adaption Min / Max as the exe's soft limit (`soft_adapt()`: `Max*(1-exp(-x/Max))` above,
  `|Min|*(exp(x/|Min|)-1)` below) sampled into Bevy's exposure curve every 0.5 EV. Map
  cameras with a LUT get `GameToneCurve` and Bevy `Tonemapping::None`: src/grading.rs /
  grading_lut.wgsl apply Bevy's auto exposure (`view.color_grading.exposure`, which the skipped
  tonemapping node would have applied) and the Yebis Sensitometric curve, mode 7, the shipped
  default (`A = 1-e^(-1.442695x)`, `B = 1-e^(-4.456669x)`, `out = A + c(B^3 - A)`, c =
  FilmicContrast 0.5, the castle and gate params having none), then the power 1/2.2 and the
  LUT. `SHINOBI_MAP_TONEMAP=bevy` keeps Bevy's curve for comparison. About 63 fps on the castle.
  Measured against the real frames (scratchpad look.py: mean log2 luminance, p10..p90 range,
  warmth R-B, HSV saturation): the Owl's exposure and range moved toward the game but turned
  blue; Genichiro is flat and dark with no sun.

  | Shot | logL | range (stops) | warmth | saturation |
  |---|---|---|---|---|
  | real Owl | -6.06 | 7.6 | +26 | 100 |
  | ours Owl | -2.73 | 5.0 | -11 | 44 |
  | real Genichiro | -5.10 | 6.1 | +20 | 85 |
  | ours Genichiro | -4.21 | 4.7 | -9 | 25 |

  Open (now "VFX and Maps"'s): re-fit `METER_EV` / `BASE_EV` for the new curve (they were tuned
  for Bevy's); the blue cast and low saturation (check the LUT input is the curve's output range
  and that squared colours are not squared twice in the probe / ambient path); try the 6 h key on
  the castle (DarkScript3 time of day 1 = Morning for Genichiro's event). Per-fight choices
  themselves belong to the boss fights, which were given away (HANDOFF 0.1).
  2026-10-12 (VXf) on the gate (SHINOBI_MAP=m11_01_00_00, the HUD-shot start view: the shaded
  gate stairs), look.py: 18 h logL -4.84, 2.1 stops, warm +34, sat 117; 12 h -5.23, 1.9 stops,
  warm +2, sat 18; 18 h without the env probe (SHINOBI_MAP_NO_ENV) -6.09, 2.1 stops, sat 22;
  without the LUT -4.38, 1.7 stops; `SHINOBI_MAP_TONEMAP=bevy` changed nothing measurable here.
  The gate's four LUTs (lut_0000-0030) are near identity (pure red / green / blue and greys map
  to themselves within ~10 / 255), so the LUT is not the cast. The real frames (all open boss
  areas, 4.7-8.3 stops) are no fair match for that shaded view: tuning waits for a real gate
  frame at a known hour (needs a reference frame). Seen: the 12 h key is grey (sat 18) where the 18 h one
  is saturated; the scene is evenly lit by the ambient / probe with few sun shadows in view.

- **Map emissives** (2026-10-13, not ported; shaderbdle.shaderbdlebnd, D3DDisassemble): the
  Gbuf passes of M[Emissive]_Simple and M[Multiple]_Emissive write a fifth target o4 = the
  emitted colour x SC_PreExposure (the deferred lighting adds it). M[Emissive]_Simple variant 1
  (cb3 from 384: color_0, float_0, _1, _3, _4, _5, _6, float3_10, _11, _5, _7, _8, _9):
  `R = Mask3Map (t3, uv slot 3)`, `m = dot(R, f3_5) * f_1 * dot(R, f3_7) * f_0 * dot(R, f3_8) *
  f_3`; `N = t0 at uv0 + time * (0.15, 0.45)`, `y = 1 - dot(N, f3_10) * f_5 * dot(N, f3_11) *
  f_6 * dot(N, f3_9) * f_4`; `L = luma(vertex colour)` (0.29891, 0.58661, 0.11448);
  `emit = clamp(color_0, 0, 5) * m * L * y * L`. EmissiveCloth_simple: color_0 (1, 0.41, 0.13),
  f_0 / 1 / 3 = 1.5, f_4 / 5 / 6 = 0.9, all float3s (1, 0, 0): `emit = colour * 3.375 R^3 *
  (1 - 0.729 N^3) * L^2`. M[Multiple]_Emissive variant 33 (the m11_00 charcoal / thatch
  materials): the M[Multiple] layers plus an ember term from scrolling masks (t2 at three
  time-driven offsets, `pow(product * color_?, float_?)`) and the vertex blend weights
  TEXCOORD5 x / z / w against height masks (`sat((v5.z - (mask + 0.01) * f * (1 - v5.z)) *
  (1 + f))`); colours color_3 (1, 0.63-0.67, 0.29-0.40) x 7 and color_9 (1, 0.46-0.48, 0.13-0.20)
  x 3-5. The camp curtains are not emissive (M[AMSN_light]_e = MetallicValue).
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

## 2026-10-12 The sat layer shaders, decompiled (M[Multiple] / M[MultipleGround])

Static: `/shader/shaderbdle.shaderbdlebnd.dcx` -> `M[Multiple].shaderbdle` /
`M[MultipleGround].shaderbdle` (not gxflvershader), unpacked with the extractor's `unpack`
(`MSYS_NO_PATHCONV=1` under Git Bash), DXBC disassembled with d3dcompiler_47 D3DDisassemble
(ctypes). The RDEF keeps the cbuffer names. 34 pixel variants per shader
(`M[Multiple]_<N>_{Gbuf,Fwd,...}.ppo`); opaque map geometry draws the **Gbuf** pass (Fwd_27 has
the same blend). **N = the MTD's `GXFT_SATStaticSwitch`** (map FLVERs name per-map MTDs like
`M[m11_01]_Wood2_multi.mtd`: shader `M[Multiple].spx`, switch 1; variant 1 samples exactly its
slots 8, 3, 0, 7, 6). Most used: Multiple 16 x107, 1 x57, 26 x38, 17 x30, 15 x27, 21 x21, 0 x19,
11 x18; MultipleGround 12 x31, 24 x26, 13 x20, 6 x15, 16 x14. Slots per Multiple variant: 0 = 8, 3,
0; 1 + 7, 6; 2 + 7, 6, 16; 5 + 12, 9; 10 + 12, 10, 9; 15 + 5, 4, 9; 16 + 5, 4, 6, 7, 9; 18 + 5, 4,
6, 7, 9, 17; 27 all but 17.

Vertex inputs (VS asm + FLVER layout): Color1 (UByte4Norm), UV0 = the UVPair (short4 / 2048:
uv0 = xy, uv1 = zw; VC_TexScroll_0/1 scale xy, offset zw), UV1..UV6 (UByte4Norm) = TEXCOORD of the
same index, so blend[k] = TEXCOORD(k+1), each read from one channel:
b0 = blend[0].R overlay weight; b1 = blend[1].B (MultipleGround .G) layer L weight; b2 =
blend[2].RGB a tint colour; b3 = blend[3].R wet weight; b4 = blend[4].G snow weight; b5 =
blend[5].B detail-normal strength; COLOR1.R multiplies G-buffer o1.z (vertex AO, uncertain).
UV sets: uvS = uv0 * |world X axis| (UVs scale with the instance; 1 unscaled) for base, overlay,
snow and mask 9; the detail normal (slot 0) raw uv0; slots 7, 6, 16 **uv1**. Every sample also x
FC_TextureTilingScale[t-register] (xy scale, zw offset; probably the FLVER texture scale,
unverified).

Multiple (variant 27, and 18), constants `M_Multiple__snp_0_*` from cb3[24] (FLV(x) = (x*2-1).xy,
z rebuilt):
```
hb(w, h) = saturate((w - h*(1-w)) * (1 + float_5))        // height blend
H(mask, sel, k) = (dot(mask.rgb, sel)*k + 0.01) * float_4
m9 = tex9(uvS); m6 = tex6(uv1)
wOv   = hb(b0, H(m9, float3_0, float_29))                 // overlay 5/4
wL    = hb(b1, H(m6, float3_2, float_26))                 // layer 7 (uv1)
wWet  = hb(b3, H(m6, float3_1, float_30))                 // only with slot 16
wTint = hb(luma(1-b2.rgb), H(m6, float3_3, float_2))      // luma .29891 .58661 .11448
d   = lerp((.502,.502,1), tex0(uv0).xyb, b5)              // detail normal
n   = lerp(tex3(uvS), tex4(uvS), wOv)                     // xy normal, B gloss, A occlusion
N   = FLV(n) + float3(FLV(d)*float_1, 0)
Nw  = normalize(TBN*N)
s   = dot(float3_7, Nw) * b4                              // e.g. (-.3, .8, -.3), not normalised
wSnow = hb(s, H(m9, float3_5, float_32))
N  += float3(FLV(tex16(uv1 + time*float2_1))*float_0*wWet, 0)
N   = lerp(N, FLV(tex10(uvS)), wSnow)
gloss = sat(lerp(lerp(n.b, float_19, wWet), tex10.b, wSnow))
occl  = sat(lerp(n.a*d.b, tex10.a, wSnow)) * COLOR1.R
A = tex8.rgb * lerp(color_1, color_7, b0)
A = lerp(A, tex5(uvS)*color_2, wOv)
A = lerp(A, tex7(uv1+float2_0)*color_5, wL)
A = lerp(A, A*b2.rgb*color_6, wTint)
A = sat(lerp(A, tex12(uvS)*color_8, wSnow)) * FC_DifMapMulCol * InstanceData.aColor[FC_ColorSlotNo]
// variant 18 (no snow): metal = sat(lerp(tex17(uvS*float2_10).R, tex17(uvS*float2_11).G, wOv))
```
cb3 slots: color_0..8 [24..30] (color_0 unused), float_0 [31], 1 [32], 19 [33], 2 [34], 26 [35],
29 [36], 30 [37], 32 [38], 4 [39], 41 [40], 5 [41], float2_0 [42], 2_1 [43], 2_10 [44], 2_11 [45],
float3_0 [46], 3_1 [47], 3_2 [48], 3_3 [49], 3_5 [50], 3_7 [51]. Wood2: float_4 0.5, float_5 3,
float3_7 (-0.3, 0.8, -0.3), float_19 0.75.

MultipleGround (27, 26, 12; `M_MultipleGround__snp_0_*`): detail normal slot 12 on raw uv0 x b5 x
float_23; overlay 5/4 wOv = hb(b0, H(m9, float3_0, float_10)); layer 1/6 on **uvS** wL = hb(b1(.G),
H(m9, float3_2, float_6)); snow 11/7 s = dot(float3_5, Nw) * b4, wSnow = hb(s, H(m9, float3_3,
float_22)), all with mask 9; snow normal added (N.xy += FLV(tex7) * float_31 * wSnow), gloss
lerp(n.b, tex7.b, wSnow); albedo lerp(lerp(tex8, tex5, wOv), tex1, wL) * lerp(1, b2.rgb, float_2),
then lerp(..., tex11 * color_0, wSnow); o1.z = COLOR1.R. Wet (26): wWet = b3 * float_7 (linear),
normal toward flow normal 0 (uv1 scrolled by float2_1, x float_12) plus Ripple 2 (raw uv0, x
float_11, faded by Ny^10), gloss -> float_3, albedo darkens toward color_1 by wWet * float_8.
cb3: color_0 [24], color_1 [25], color_3 [26], float_10 [27], 11 [28], 12 [29], 2 [30], 20 [31],
22 [32], 23 [33], 3 [34], 31 [35], 4 [36], 5 [37], 6 [38], 7 [39], 8 [40], float2_1 [41], float3_0
[42], 3_2 [43], 3_3 [44], 3_5 [45].

So the 2026-10-10 "Layers" reading above is wrong in places: the `_3m` masks are not height in
R (a per-material float3 picks the channel, and a high value pushes the layer back through hb);
the overlay uses mask 9; slot 7 is an albedo layer on uv1 by b1 with mask 6; slot 0 is the
detail normal (every variant, strength b5); slot 17 a metal mask (variant 18); b2 an RGB tint
gated by mask 6 (closer to "ao" than "c"); b3 the wet / flow normal (slot 16, uv1); snow = the
world normal . float3_7 x b4, height-blended on mask 9. Uncertain: o1.x gloss / o1.z occlusion,
whether color_N carry the MTD "-Intensity", where FC_TextureTilingScale comes from.
Disassembly and tables (scratch, VXf session): satshader/asm/gbuf, variant_textures.txt,
static_switch_histogram.txt, mtd/*.txt, dis.py, flv.py (copied to extracted/ref_shaders/).

Ported 2026-10-12 (bin v5): the exporter's `choose` takes M[Multiple] / M[MultipleGround]
materials by slot number, keeping only the slots the variant samples (`MULTIPLE_SLOTS` /
`GROUND_SLOTS`), and writes their constants (`game_params`, 16 vec4: colours x
`-Intensity`, which the engine multiplies in (GXMaterialParam_HDRColor, FUN_1403dc4b0); the snow
direction turned into arena space) plus 8 blend bytes and the uv scale per vertex.
map_material.wgsl `game_layers` runs the blend above; map.rs feeds the extra vertex data through
its own vertex stage (`ATTR_MAP_BLEND2`, `ATTR_MAP_UV_SCALE`). v1-v4 bins and kind 0 keep the
first reading. `MAP_OLD_LAYERS=1` exports the old way; `SHINOBI_MAP_SHOW_BLEND=1` shows overlay
red, layer L green, snow blue. Gaps: the HDR colour's optional pow 2.2 (flag not traced), the
G-buffer occlusion (n.a x d.b, COLOR1.R), FC_TextureTilingScale, MultipleGround's ripple and
its wet darkening (read as a lerp toward color_1), the metal mask (variant 18), the shader
tool's defaults for constants an MTD leaves out (0, colours 1). Gate check: no breakage, wood
a little darker and greyer (the tint layer), snow on up-facing ground. The boss arenas are
still v4 until re-exported.

Given away 2026-10-12: layered map materials now belong to someone else.
State: bin v5 + `game_layers` built and working on the gate (m11_01_00_00 re-exported 05:5x,
448 batches, 60 fps, test `bin_v5_reads_the_game_path`); boss arenas still v4. Open: the gaps
listed above, a real gate frame to compare against, re-exporting the other arenas.

## 2026-10-12 ObjAct doors left out; teleports skip the wall slide ("bug fixes" session)

- **The gate fight never started.** Since objects bring their own collision (above), the
  gatehouse door o111410_8000 stood closed (bind pose) at z 1.2-1.38 across the passage
  between Wolf's start (0, 4) and the line-up (0, -4): a solid wall, the General stayed behind
  the slats. It is an **ObjAct** part (MSB event type 7; type data ObjActEntityID,
  ObjActPartIndex, ObjActID, StateType, ..., EventFlagID: SoulsFormats MSBS EventParam.ObjAct):
  the game opens it through the ObjAct and its anibnd, which we do not run. The exporter now
  reads `Msb::objact_parts` and leaves those parts out, mesh and collision ("N ObjAct doors
  left out"; `MAP_KEEP_OBJACT=1` keeps them). Gate map: o111410_8000 / 8001 and o111300_8000
  (231 -> 228 objects, 182 -> 179 with collision). `arena_floor_and_walls` now also walks from
  (0, -4) to (0, 4). Boss arenas keep their ObjAct doors until their next export.
- **Teleports are not walks.** `bodies_on_terrain` slid every change of position since the last
  tick along the floor; a script / event warp, the parking spot or a reset is a teleport. A
  move longer than `TELEPORT` (2 m in one 60 Hz tick, 120 m/s) skips the slide now. The slide of
  a 110 m warp gathered every triangle of the move's box on the 1.29 M-triangle Monkeys arena
  and ran 512 substeps: one frame took about 33 s (the Monkeys fight's "3 fps"), and the warp
  stopped at the first wall.

## 2026-10-12 The bosses fight on the castle top

- Bosses fight on the castle top unless they don't fit. `boss_arenas.py` now sends every npc_names.json row without its own arena to
  `boss_71001000` (the castle top; `TOP_ROW`), and the moved rows too: the Owl (Father),
  prologue Genichiro 71000000, Isshin's field guests. `DONT_FIT` keeps Gyoubu 50800000 and
  the Demon of Hatred 70200000 on Isshin's silvergrass field (too big for the roof). Own arenas
  stay where a fight needs its place: Monk, Butterfly, both Apes, the Dragon, the Old Dragons,
  the Monkeys. Index: 46 rows on boss_71001000, 2 on boss_54000000.
- Check: General Naomori 10200000 loads boss_71001000, Wolf at [0, 0.906, 4], 54 fps; the
  General stands on the roof behind the title screen. Mini-bosses still get no mini-boss bar
  or music there (no map events for them).

## 2026-10-12 Boss fog walls (tools/fog_walls.py, src/fog_wall.rs)

- The maps raise a boss room's fog with common event 20005820 (end flag, object, SFX, start
  flag; the SFX on dummy 101) and its boundary with 20005825 (end flag, collision, start flag).
  `fog_walls.py` reads every map's starts into extracted/enemies/fog_walls.json (47 walls).
- A fight's walls are its map's walls whose end flag its own boss_scripts.json events use (17 of
  20 rows have some; the Monk on m17 gets its near door 1701800, not the two 470 m away).
  `fog_wall.rs` puts their FXR up at the object's place from the start of the fight
  (`FxEffect.keep`: no age limit) and lets it fade when the script handles the defeat.
- gap: the collision part (h900800 on the roof) is not loaded, so the fog does not stop Wolf,
  and the object's model is not drawn. After sekiro-rs crates/sim/src/fogwall.rs (credited in
  THIRD_PARTY.md).

## 2026-10-12 The game's height fog (src/height_fog.rs, height_fog.wgsl)

- Bevy's DistanceFog (exponential, no height, a hand-set sun glow) is replaced by a pass after
  the main pass: the game's fog model from sekiro-rs fog.wgsl with the exe's packing from its
  docs/SHADER-INPUTS.md (884ea0b): FixedDensity, HeightFogDistribution and Density Scale over
  GameScale (sH halved), Near / Far / StartDists times it, h = map Y - HeightFogOffset in
  metres, the exe's geometric depth slices, then the analytical continuation past Far. Colours
  are gamma 2.2 times W on the probes' radiance scale (ENV_PER_UNIT) times the camera exposure.
  `SHINOBI_MAP_NO_FOG=1` turns it off.
- Check (castle roof, 18 h): no broken pixels; the haze is faint (FixedDensity 0.0015 at
  GameScale 1; the height layer sits about 140 m below the roof).
- gap: VolumeDepth (128 slices, the exe clamp's top), AbsorptionScale, both HeightAttens,
  NearOffset and EnableAnalyticalFog are not in the exports; the AngleClip floor; the fog light's
  LightBankId (the light set's sun stands in); local fog lights; sun shadows in the fog; the sky
  box is left unfogged; particles take the fog of what is behind them.

## 2026-10-12 Sky domes and the map's bloom (src/sky.rs, map.rs map_bloom)

- The sky is the game's own: one dome of the sky library m89_00_00_99 (`sekiro-extract sky
  extracted` -> extracted/sky/sky.json + 5 textures from other/maptex.tpf; the 27 m89 mapbnds
  unpacked with `unpack ... 'm89_00_00_99/m89_00_00_99_9[0-9]+\.mapbnd'`, MSYS_NO_PATHCONV=1).
  common.emevd event 100 shows group 8907000..8907070 by the story time: m900040 0 h, 050 2 h,
  000 6 h, 010 12 h, 020 18 h, 030 22 h, the nearest to the hour; m10 060, m25 070 (+ a Cloud
  layer). Camera-centred at infinite depth, no depth write; the probe stays the reflections
  and diffuse light but is no longer the sky box (`SHINOBI_MAP_PROBE_SKY=1` brings it back).
  Ported from sekiro-rs sky.rs / sky.wgsl (THIRD_PARTY.md).
- Check: 12 h pale hazy blue, 22 h dark teal with cloud wisps; 18 h flat ochre with dark cloud
  smudges. gap: the colour packing (gamma 2 times Intensity) and cloud shading are sekiro-rs's
  approximations, not the traced SkyDome_Ver2 shader; the fire sky and 8907015 are not chosen.
- 2026-10-13 SkyDome_Ver2 decoded from the game (shader/shaderbdle.shaderbdlebnd.dcx ->
  M[Sky]_SkyDome_Ver2.shaderbdle, `_{0,1}_Fwd.ppo` D3DDisassemble; spec:
  docs/kb/sky_shader.md). Colours reach the shader as rgb^2.2 x Intensity
  (GXMaterialParam_HDRColor FUN_1403dc4b0; the power is on when the metaparam entry byte +0x2E is
  set, 1 for every colour of shaderbdle). Switch 0 (morning / evening / night / past night):
  sun = engine FC_DirLightVec_0, sunT = min(pow(sat(d.L)(1 + float_12), float_10), 1), sunCol =
  lerp(color_3, color_5, sunT); g = 1 - d.y; hor = lerp(color_6, color_7, min(pow(g float_4,
  float_6), 1)); base = lerp(color_0, hor, min(pow(g (1 + float_11), float_46), 1)); sky =
  saturate(base + sunT sunCol); clouds: two masks (A = cloudhigh on UV0 x float2_6, B = hcloud on
  UV1 x float2_7 + float2_0) picked by float3_5 / 7 / 8 / 9, rim, body lerp(color_8, lerp(color_1,
  color_4, ...)), sun tint color_2, alpha 4 float_17 (same terms as sky.wgsl). Switch 1
  (afternoon): no gradient, the lobe faces the MTD's float3_6. Output col x pre-exposure, fogged
  by the froxel volume; render target 1 = col x g_GlowScale (glow). Clouds drift by a two-phase
  flow crossfade (frac(2 t speed), float_39 / 40 speeds, float_38 / 41 strengths, along U only).
  Ported: the packing (was gamma 2), switch 1's float3_6 sun and the flow drift (sky.wgsl
  flow_sample; tangent frame from screen derivatives); the rest already matched. gaps: the fog and glow outputs, the original SkyDome / Cloud shaders. The 18 h patch on
  the gate look map stays a flat cream: the sky is clamped to 1 before exposure, and that view faces
  the low sun.
- 2026-10-13 the same packing for the map layer colours: the exporter's `game_params` now writes
  M_Multiple / M_MultipleGround colours as rgb^2.2 x Intensity (was raw rgb x Intensity). Most are
  white (unchanged); tints like (0.78, 0.86, 1.0) become (0.58, 0.72, 1.0). The gate map and all
  arenas re-exported 2026-10-13. Character shaders (main.rs: the layered skin and the coat
  Fresnel params, `-Intensity` at main.rs:474 / 524) still pack raw rgb x Intensity: same fix
  needed there, with a model re-export (not this track's files).
- Bloom from Bloom[Yebis] (map_bloom): the exe's level weights Alpha * Luminance * Smooth^i /
  sum Smooth^j reproduced exactly by Bevy's additive chain (intensity = level 0's weight,
  low_frequency_boost = Smooth - it, curvature 0.9999), NumLevels mips, hard knee at the raw
  Glare Threshold. gap: the threshold sees Bevy's pre-auto-exposure buffer; the 7-tap Gaussian,
  level-0 size, Saturate Ctrl and FeedbackWeight are not modelled. `SHINOBI_MAP_BLOOM=camera`
  keeps the camera's old bloom.

## 2026-10-12 Fog-wall collision and GIIV irradiance volumes (VFX and Maps)

- **Fog-wall collision ("gates")**: on the castle roof (boss_71001000) both blockers were already
  in the static `.hit`: the wall object o001401's own hkx box (12 triangles, 5.95 m x 6.95 m x
  5 cm, ObjectParam material 29) and the 20005825 part h900800 (24 triangles, a material-less
  octagon of walls round the roof, open where the wall object stands). So Wolf was always shut
  in, and the walls never went after a win. The exporter now writes every
  `enemies/fog_walls.json` object / collision entity of the map to `map_<id>.gates` ("SHMG" v1:
  u32 version, u32 gates; per gate i32 entity + an SHMC body) instead of the `.hit`;
  `src/fog_wall.rs` switches them on through `map::set_gates` while the fight's walls stand,
  and `Terrain::slide` pushes bodies out of them (test `fog_wall_gates_block_only_while_on`).
  The gate map (m11_01_00_00 around c1020_0004) has none in range (the roof wall is 83 m off).
  2026-10-13: of the other arenas only boss_50900000 (m10, o001401 at 39 m) had the box baked
  in; o001400 / o001402 were never unpacked (the draw-group check skipped the parts before the
  missing-object list), so their walls had no collision. The exporter now takes a fog-wall
  object's hkx box into the gates before the draw-group / ObjAct checks and never draws its
  mesh (the wall shows as its SFX); m17, m15, m11_02, m25, m10, m11_01 re-exported.
- **GIIV**: `export_giiv` (sekiro-extract map.rs, called by `export_env`) decodes the probe's
  `giiv<probe>_<v>_{x,y,z,w}` (DX10 BC7, dimension 4: the gate's 0280 is 20 x 16 x 16 over
  Env_Box280, 62.7 x 59.0 m, 53.7 m high) into `map_<id>_giiv_<v>.bin` ("SHGI" v1: width,
  height, depth, then the four RGBA8 volumes x, y, z, w) and `map_<id>_giiv.json` (`uvw`: Bevy
  point -> volume 0..1, the box bottom-centred, texture x / y / z along width / height / depth;
  `nrm`: Bevy normal -> map space). The game (`apply_giiv`) loads the variant nearest the hour
  as one 3D texture (volumes stacked on depth) on every map material; `map_material.wgsl` `lit`
  fades the probe cube's diffuse out by the volume weight and adds
  `diffuse * max(n.x X + n.y Y + n.z Z + W, 0) * ENV_PER_UNIT` (texel decode
  `2 (rgb - 0.5) 2^(20 (a - 0.5))`, sekiro-rs SHADERS.md "GIIV"). At the gate origin, noon:
  W = (0.019, 0.024, 0.029), up-facing 0.03-0.05, the cube's mean 0.037-0.057, so the scale
  carries over. Photo-mode pairs at 12 h (`SHINOBI_MAP_NO_GIIV=1` for the cube only): no
  artifacts; map surfaces 3-4 / 255 darker on average, the stone by the doorway clearly darker
  (more contrast between the open yard and the gate interior); the characters keep the cube.
  `MAP_ENV_ONLY=1` re-exports only the probe (cubes + volumes) and the json's `env_probe`: the
  gate's probe is now Env_Box280 (`gilm0280`), as the bottom-centred box rule picks.
  The two variants around the hour blend like the cubes (decoded, then mixed in the shader;
  `giiv_next`, weight `scale.y`; 15 h = 12 h and 18 h at 0.5: no artifacts).
  Gaps: characters (sekiro_material.wgsl) still take the cube's diffuse. (The single volume
  above was replaced 2026-10-13 by every box round the arena: "GIIV boxes" below.)
- **GIIV boxes** (2026-10-13, from the game's own shader): gxrenderershader.shaderbnd
  `GXEnvMapRegion[Box][Diff].ppo` (+ DiffSpc / Spc / Pcc / Lerp / Eh variants, a [Stencil]
  pass; unpack `^/shader/`, D3DDisassemble) draws each Env_Box as a deferred light volume:
  the pixel's point to the box's [-1, 1] space (`g_envMapInvWorld`), outside -> nothing; weight
  `smoothstep(saturate(min((1 - |p|) * g_envMapDrawAreaExtents) * g_envMapInvFadeDistance))`
  (metres to the nearest face over the fade distance) x `g_envMapFadeRate`; the four
  `g_IrradianceVolumeX..W` sampled at the point plus a step along the normal
  (`g_envMapInvIv`, `/ g_envMapIvSamplingParam`), decoded `2 (rgb - 0.5) 2^(20 (a - 0.5))`,
  `max(dot(n, XYZ) + W, 0) * g_envMapDiffuseColorScale * SC_PreExposure` to RT1 rgb with the
  weight in alpha: the boxes blend over each other in drawing order. The MSB's type data
  (SoulsFormats MSBS EnvironmentMapEffectBox; read by msb.rs `Region::env`, printed by
  `sekiro-extract envboxes <msb> x y z [m]`): 0x04 the fade distance (m: 3 for areas, 1.5 rooms,
  0.25 the castle's burning hall, 0 the reflection boxes), byte 0x09 the priority (0 whole map,
  10 play area, 19-20 areas, 30-32 rooms, 40 the burning hall Env_Box400, 90 the 映りこみ用
  reflection boxes): the drawing order taken (then the MSB's order); 0x24 = 2 only on the
  reflection boxes (taken as specular only: left out), 1 on the whole-map boxes and Env_Box400;
  0x28 / 0x2C 1 / 1.5 and 0.25 / 1 (maybe the FadeRate / DiffuseColorScale: not mapped, taken
  as 1). Every Env_Box has its own `giiv<box>` volumes (m11_01: ~120; Env_Box400 also 2400 /
  3400 / 4400, event states, not used). The exporter (`export_giiv`) takes the boxes within the
  arena radius of the pick point (nearest first, at most 16, atlas depth <= 2048), crops each
  to the voxels within radius + 30 m (+2 voxels), and stacks them in one RGBA8 atlas per
  variant ("SHGI" v2; a box without a variant takes its nearest); the json lists them in
  drawing order (`uvw`, `size`, `fade`, `grid`, `lo`, `vox`). map_material.wgsl `giiv_light`
  loops the boxes as the game blends them; the cover fades the cube's diffuse out. Arenas:
  castle top 16 boxes (Env_Box346 meditation room 32 and Env_Box400 40 over it), monkeys' hall
  16, gate 16, the rest 3-10; 204 MB of atlases in all (37 MB with one box). Checks (photo mode,
  12 h, on vs `SHINOBI_MAP_NO_GIIV=1`): no seams or blocks on the gate look map or the castle
  top; the castle's dark wood walls / ceilings read instead of near-black. Test
  `giiv_boxes_fit_their_atlas`. gaps: the boxes past the 16 nearest (the castle has ~55 within
  60 m, mostly reflection boxes and lower floors); the burning-hall box is always on (its event
  switch and variants 2400-4400 untraced); FadeRate / DiffuseColorScale; the exe's draw-list
  build (GXEnvMapRegionManager) not read, so the priority order is inferred from the data.
- 2026-10-15 the dragons on the sandbox: map = "sandbox" keeps 52000000 / 53000000 on their own
  m25 arena (map.rs OWN_ARENA_ROWS / sandbox_exception; Wolf fell through the sandbox floor
  there), and both arenas reach 140 m (tools/boss_arenas.py OVERRIDES; the Divine
  Dragon's wind pushes Wolf away), 96 grapple points each.
- 2026-10-17 every main boss on its own arena: map = "sandbox" now keeps every row of
  menu.rs MAIN_BOSS_ROWS on its arena (map.rs sandbox_exception; OWN_ARENA_ROWS removed), and
  the rows that used to share the castle top got their own exports (tools/boss_arenas.py
  OWN_ROWS): the Sword Saint 54000000 and Tomoe 71101000 (m11_02 field), the prologue
  Genichiro 71000000 (m11_02), Owl (Father) 50601010 (m10 Hirata), Gyoubu 50800000 and the
  Demon of Hatred 70200000 (m11_00). Emma / Isshin Ashina 74000010 / 54300000 and the Owl
  50600000 stay on the castle top (MOVED). Gyoubu's MSB spot faces the gate's walls and steps:
  his arena is turned 180 and stands 16 m into the field (OVERRIDES; at 0 / +-45 / -90 there was
  no floor under Wolf's start). sekiro-rs has no arena besides the Owl's castle top (m11_01).
  Checked in game (SHINOBI_AUTOBOSS): Gyoubu, Butterfly, Owl (Father), Genichiro and the Demon
  reach their defeat on their arenas.
- 2026-10-17 crash on a switch into Hirata (m10, Butterfly / Owl (Father)): the Vulkan driver
  lost the device 20-30 s after a switch from the castle top (DeviceLost, weird colours first).
  Only with the fog and the sun's shadows both on; a fresh start on Hirata never did, nor DX12
  (which draws no 3D at all here). The castle top has VolumetricFog, Hirata has it off: the fog
  had made the camera's depth sampleable (height_fog.rs depth_binding) and that stayed after the
  fog went. Now the usage goes with the fog (FogDepthBinding) and the fog pass skips a frame
  whose depth is not sampleable yet. Also a map without a probe now removes the last map's
  EnvironmentMapLight / Skybox (apply_lighting). Checked: castle top -> Butterfly, -> Owl
  (Father) and back, 100 s each, no error. Test knob: SHINOBI_ARENA_CYCLE=<chr>:<row>,...
  (autoboss.rs) starts those fights 6 s apart.
