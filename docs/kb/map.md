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
SEKIRO_DIR=<Sekiro> sekiro-extract map extracted m11_01_00_00 c1020_0004 80
```
(`SEKIRO_DIR` for oo2core: the pieces inside the mapbnds are DCX KRAK.) The `map` command takes
`<extracted> <map id> <centre part name> <radius m>` and writes `extracted/map_<id>.bin`
(pieces, "SHMP"), `.hit` (collision, "SHMC"), `.json` (centre, light set, draw params) and the
textures into `extracted/tex/`. `MAP_LOG=1` prints every piece taken and every material without
an albedo. Output for radius 80: 230 batches, 6.86 M triangles, 310 MB; 844 k collision
triangles. The game loads it in about 0.5 s.

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
  (0, y, +4) in front of him. Written unmirrored; the game mirrors X and swaps the winding on
  load, like every model (FromSoftware's left-handed space to Bevy's right-handed one, so the
  map keeps the real game's handedness).
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
  **Open**: at 0 h the set's Adaption Max 8 read as +8 EV washed the gate out white, so the
  brightening is capped at `ADAPT_CAP_EV` 2 (not yet looked at; the night probably wants the
  cap by eye, or Yebis's Adaption Max is not an EV clamp). Adaptation speeds are Bevy's defaults.

### Open
- Lighting is now the game's own light set + GI probe at 18 h (see above); left: the LUT and
  auto exposure, and a blend between probe variants for in-between hours. (Material names like
  "本城の遠景モデル" on the gate parts m00202x are reused castle materials, not far-view
  stand-ins.)
- 6.86 M triangles: 54 fps on this machine (the game logs "map frame rate" once, seconds
  5..15); the radius / LOD distances are guesses. Cheaper: LodLevel1 from 20 m, radius 60.
- Tests: `cargo test --release map::tests` (`arena_floor_and_walls` needs the exported .hit;
  `arena_probe -- --ignored --nocapture` prints floors, ceilings, wall distances and slides
  around the spawn spots). `slide` sub-steps are capped at half the body radius: with the old
  cap of 32 steps a long move tunnelled through the gate posts.
- Multi-layer materials (M_Multiple*, MultiBlend) show only their base layer; the blend data in
  the Byte4C "UV" members and the second UV set are not exported.
- Stairs / wall feel, enemy spawn height on slopes, the camera against low ceilings: not yet
  checked in play.
