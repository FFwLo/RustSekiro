# M[Sky]_SkyDome_Ver2: decoded pixel math (static, from Sekiro's own files)

Sources: `/shader/shaderbdle.shaderbdlebnd.dcx` -> `M[Sky]_SkyDome_Ver2.shaderbdle` (unpacked copy in
`bdle/`; 43 files: `.metaparam`, the `_0_*` / `_1_*` .ppo pixel variants, the .vpo vertex shaders),
disassembled with d3dcompiler_47 `D3DDisassemble` (`dis.py`). The colour packing comes from the exe
(Ghidra decompile in `extracted/decomp`: `FUN_14036d290`, `FUN_1403dc1a0`, `FUN_1403dc4b0`,
`FUN_1403dc9e0`, class `GXBS::GXMaterialParam_HDRColor`).

Files:
- `MSky_SkyDome_Ver2_0_Fwd.ppo.asm`: forward pixel shader, GXFT_SATStaticSwitch 0
  (morning / evening / night / pastnight)
- `MSky_SkyDome_Ver2_1_Fwd.ppo.asm`: forward pixel shader, switch 1 (afternoon)
- `MSky_SkyDome_Ver2_Fwd.vpo.asm`: the vertex shader. It is generic (nothing sky-specific)
- extras: `MSky_SkyDome_Ver2_0_Fwd_A.ppo.asm` (alpha-test variant), `MSky_SkyDome_Ver2_0_Gbuf.ppo.asm`.
  `_FwdSdw` / `_FwdSSdw` have the same size as `_Fwd` (probably identical, not diffed)

## 1. The key question: colour_N and colour_N-Intensity

The shader never sees the intensity. It reads each colour as one float4 from cbMtdParam, and the CPU
fills that float4 (`GXMaterialParam_HDRColor`, `FUN_1403dc4b0`):

```
c   = MTD colour_N (float4, 0..1, i.e. the 8-bit value / 255)
if (gammaFlag) c = pow(c, float4(2.2, 2.2, 2.2, ONE))      // FUN_1403dc9e0 = per-channel powf
cb.rgb = c.rgb * colour_N-Intensity
cb.a   = c.a * ONE
```

- `gammaFlag` is metaparam entry byte +0x2E (copied to `desc+0x20` in `FUN_14036d290`). **It is 1 for
  every colour in this shader** (and for all 39 colour params in all of shaderbdle, so it always
  applies). Result: **cbuffer colour = (colour^2.2) * Intensity**, i.e. sRGB-ish to linear with a plain
  2.2 power (not the piecewise sRGB curve), then the scalar intensity. The power comes first, then
  the multiply; the intensity is not raised to any power.
- `ONE` = `DAT_143f085ec`, the w of a global float4 in .bss (runtime-initialised, so not readable from
  the file). Every other use is as the w term of a matrix x vector product, so it is 1.0 (near certain).
  Alpha is unused by this shader anyway.
- Uncertain, runtime only: after that, `FUN_1403dc4b0` can blend in an override found by param ID
  (`FUN_1403144b0` / `FUN_140314530` on a per-draw list): `cb = lerp(cb, ovr.rgb*s (ovr.a raw), w)`.
  This looks like the material-modifier path (GXMaterialParamModifier). Plain sky MTD values never go
  through it. It is not known whether anything drives sky colours at runtime.
- A missing MTD colour gets the metaparam default colour with intensity 1.0. `FUN_1403dc1a0` initialises
  +0x60 = 1.0 and only overwrites it when the MTD has the param. The metaparam default intensities are
  not used then (uncertain).
- Metaparam entry layout (0x50 bytes, from offset 0x120): +0 name offset (UTF-16), +0x28 u32 register
  (= cb3 index), +0x2C u16 type (1 float, 2 float2, 3 float3, 0x0C colour+intensity), +0x2E gamma flag,
  +0x2F second flag (selects the constructor path in `FUN_14036d290`), +0x34 default values (colour:
  rgba then intensity).

Floats and float2/3 are copied raw.

## 2. cbuffers and registers

Bindings (pixel): t0 = `Texture2D_3` (MTD `Texture2D_3_Mask3Map` = m99_cloudhigh_3m), t1 = `Texture2D_5`
(`Texture2D_5_AlbedoMap` = m99_hcloud_3m), t47 = `g_VolumeFog0` (3D, engine), s8 = `SS_WrapLinear` (both
cloud textures), s9 = `SS_ClampLinear` (fog). cb1 cbTextureTilingScale, cb2 cbMatDynParam, cb3 cbMtdParam,
cb4 cbInstanceData, cb5 cbLight, cb8 cbSceneParam, cb9 cbPerFrame.

cb3 = cbMtdParam (MTD constants; the register equals the metaparam id):

| reg | param | metaparam default | sw0 | sw1 |
|---|---|---|---|---|
| cb3[9]  | FC_GlowScale (MTD `g_GlowScale`, rgba) | - | yes | yes |
| cb3[24] | color_0 | (.196,.392,.706) i1 | zenith/base sky | - |
| cb3[25] | color_1 | (.490,.941,.494) i1 | cloud body A | cloud A |
| cb3[26] | color_2 | (1,1,1) i1 | cloud sun-tint multiplier | - |
| cb3[27] | color_3 | (1,1,.8275) i1 | sun colour (outer) | sun-lobe colour (outer) |
| cb3[28] | color_4 | (1,.604,.596) i3.8 | cloud body B | cloud B |
| cb3[29] | color_5 | (1,.537,.1255) i1.6 | sun colour (core) | sun-lobe colour (core) |
| cb3[30] | color_6 | (.196,.392,.706) i1 | horizon gradient start | - |
| cb3[31] | color_7 | (.408,.530,.675) i3.2 | horizon gradient end | - |
| cb3[32] | color_8 | (1,.380,.996) i1 | thin-cloud colour | - |
| cb3[33] | float_10 | 1 | sun-lobe exponent | sun-lobe exponent |
| cb3[34] | float_11 | 1 | base-gradient scale (1+f11) | - |
| cb3[35] | float_12 | 0 | sun-lobe scale (1+f12) | sun-lobe scale |
| cb3[36] | float_13 | 0.55 | body height-blend mask scale | same |
| cb3[37] | float_14 | 0.5 | body height-blend sharpness | same |
| cb3[38] | float_17 | 0.5 | cloud opacity | - |
| cb3[39] | float_20 | 0 | lit height-blend mask scale | - |
| cb3[40] | float_21 | 0 | lit height-blend sharpness | - |
| cb3[41] | float_35 | 5.65 | cloud zenith-fade exponent | same |
| cb3[42] | float_36 | 1.15 | cloud zenith-fade scale | same |
| cb3[43] | float_38 | 5.86 | flow strength, layer A (t0) | same |
| cb3[44] | float_39 | 0.13 | flow speed, layer A | same |
| cb3[45] | float_4 | 0.93 | horizon gradient scale | - |
| cb3[46] | float_40 | 0.13 | flow speed, layer B (t1) | same |
| cb3[47] | float_41 | 3.2 | flow strength, layer B | same |
| cb3[48] | float_46 | 1 | base-gradient exponent | - |
| cb3[49] | float_6 | 7.3 | horizon gradient exponent | - |
| cb3[50] | float_7 | 0.75 | cloud rim width | - |
| cb3[51] | float_8 | 4.45 | - | cloud colour power |
| cb3[52] | float_9 | 2.15 | - | cloud colour boost (1+f9) |
| cb3[53] | float2_0 | (0,0) | UV offset, layer B | same |
| cb3[54] | float2_6 | (4,3) | UV scale, layer A | same |
| cb3[55] | float2_7 | (3,1) | UV scale, layer B | same |
| cb3[56] | float3_5 | (1,0,0) | channel select A -> edge mask | A -> cloud value when hFade=0 |
| cb3[57] | float3_6 | (0,1,0) | - | **sun direction** (normalised in shader) |
| cb3[58] | float3_7 | (0,1,0) | channel select A -> height mask | - |
| cb3[59] | float3_8 | (1,0,0) | channel select B -> cloud value | same |
| cb3[60] | float3_9 | (0,0,1) | - | channel select A -> height mask (saturated) |

Engine-supplied:
- **cb9 cbPerFrame `FC_GlobalTime`** (cb9[27].x). The shader uses `t = 2*FC_GlobalTime`. `FC_TimeOfDay`
  is unused.
- **cb5 cbLight `FC_DirLightVec_0`** (cb5[0].xyz): the sun direction, **switch 0 only**. It is dotted
  directly with the view direction, so it points toward the sun. Sign and normalisation are not verified.
- cb8 cbSceneParam: `FC_MatrixInvViewProj` (cb8[8..11]) rebuilds the view ray; `FC_ScreenSize.zw`
  (cb8[1]) = 1/size for the fog UV; `SC_RenderCameraPos` (cb8[61]); `SC_PreExposure` (cb8[62].z) and
  `SC_InvPreExposure` (cb8[62].w); `FC_VolumeFogParam[0..3]` (cb8[67..70]).
- cb4 `VC_InstanceData[inst].aColor[FC_ColorSlotNo].w` gives the output alpha. `FC_ColorSlotNo` is
  cb2[0].w (cbMatDynParam).
- cb1 `FC_TextureTilingScale[0]` (t0) and `[1]` (t1): uv*xy + zw. The source is unknown (probably
  identity).
- Vertex shader (generic): world = VC_aObjMatrix * pos; clip = VC_MatrixViewProj * (world -
  SC_CameraPos). Camera-relative, so it passes **clip pos** in POSITION1 (v1, v1.w = view depth),
  world normal / tangent (unorm8: (x-127)/127; tangent.w = bitangent sign) and the instance index in
  NORMAL.w. TEXCOORD0 (v4): xy = UV0 = short.xy/2048 * VC_TexScroll_0.xy + VC_TexScroll_0.zw, zw = UV1 =
  short.zw/2048 * VC_TexScroll_1.xy + VC_TexScroll_1.zw (cbTexScroll, engine. Normally probably scale 1,
  offset 0; any scrolling there is not part of this shader).

## 3. Shared parts

```
t   = 2 * FC_GlobalTime
P   = (FC_MatrixInvViewProj * v1).xyz        // no /w: camera-relative ray (VS is camera-relative)
d   = normalize(P);  up = d.y                // world view direction, Y up

// "flow" vector: world +X turned into the tangent frame (B, T, N each normalised, then the triple)
B   = cross(N, T) * T.w
X   = normalize(float3(normalize(B).x, normalize(T).x, normalize(N).x))
flowA = float2(0.075 * X.x, -0.0 * X.y) * float_38    // y is (-)0: only U moves
flowB = float2(0.075 * X.x, -0.0 * X.y) * float_41

// two-phase flow-map crossfade (one texture each layer)
flowSample(tex, uv, flow, speed, tiling):
    p0 = frac(t*speed); p1 = frac(t*speed + 0.5)
    s0 = tex(tiling(uv - float2(flow.x, -flow.y)*p0))   // = uv - (flow.x*p0, 0)
    s1 = tex(tiling(uv - float2(flow.x, -flow.y)*p1))
    return lerp(s0, s1, abs(2*p0 - 1)).rgb
tiling_k(uv) = uv * FC_TextureTilingScale[k].xy + FC_TextureTilingScale[k].zw

A = flowSample(t0, UV0 * float2_6,            flowA, float_39, tiling_0)   // cloudhigh, rgb
Bc= flowSample(t1, UV1 * float2_7 + float2_0, flowB, float_40, tiling_1)   // hcloud,    rgb

hFade = 1 - min(pow(up * float_36, float_35), 1)   // 1 at the horizon, 0 at up >= 1/float_36;
                                                    // up < 0: pow -> NaN, min(NaN,1)=1 -> hFade 0
sunLobe(Ldir) = min(pow(saturate(dot(d, Ldir)) * (1 + float_12), float_10), 1)
hb(w, h, k)   = saturate((w - h*(1 - w)) * (1 + k))  // the SAT "height blend"
```

## 4. Switch 0 (morning, evening, night, pastnight)

```
sunT   = sunLobe(FC_DirLightVec_0.xyz)                         // engine sun
sunCol = lerp(color_3, color_5, sunT)

// sky gradient: g = 1 - up (0 at the zenith, 1 at the horizon, >1 below)
g      = 1 - up
hor    = lerp(color_6, color_7, min(pow(g * float_4,        float_6 ), 1))
base   = lerp(color_0, hor,     min(pow(g * (1 + float_11), float_46), 1))
sky    = saturate(base + sunT * sunCol)                        // NB clamped to [0,1]

// clouds
c      = dot(Bc, float3_8)                 // cloud density (hcloud)
cm     = c * hFade
inv    = 1 - cm
hMask  = dot(A, float3_7) + 0.01           // cloudhigh channel(s)
eMask  = dot(A, float3_5)
a4     = saturate(4 * cm)

rim     = max(1 - 2*abs(cm*(1 + 4*float_7) - 1), 0)          // bright ring around cm ~ 1/(1+4 f7)
sunTint = color_2 * sunCol * (rim + saturate(inv)) * inv
lit     = hb(inv, hMask * float_20, float_21)                  // = saturate((inv - hMask*f20*cm)*(1+f21))
body    = lerp(color_1, color_4, hb(c, hMask * float_13, float_14))
body    = lerp(color_8, body, smoothstep01(a4))                // a4*a4*(3 - 2*a4)
cloud   = lerp(body, lit * sunTint, sunT * a4)

edge    = saturate(3 * up) * hFade * eMask
alpha   = saturate(4 * float_17 * lerp(edge, cm, a4))
col     = lerp(sky, cloud, alpha)
```

## 5. Switch 1 (afternoon)

No gradient, no engine light. The whole dome is cloud colour times a sun lobe around the **MTD**
direction float3_6.

```
c     = dot(Bc, float3_8)
v     = lerp(dot(A, float3_5), c, hFade)     // cloudhigh near the zenith, hcloud near the horizon
hMask = saturate(dot(A, float3_9)) + 0.01
k     = hb(v, hMask * float_13, float_14)
cc    = lerp(color_1, color_4, k) * (1 + float_9)
cc    = min(pow(cc, float_8), 1)             // per channel
sunT  = sunLobe(normalize(float3_6))
col   = cc * lerp(color_3, color_5, sunT)
```
Unused in switch 1: color_0, 2, 6, 7, 8 and float_4, 6, 7, 11, 17, 20, 21, 46, float3_7.
Unused in switch 0: float_8, float_9, float3_6, float3_9.

## 6. Output (both switches; forward pass, 2 render targets)

```
inst  = round(NORMAL.w)                                         // instance index from the VS
a_out = VC_InstanceData[inst].aColor[FC_ColorSlotNo].w          // probably 1 (uncertain)

// engine volumetric fog (froxel 3D texture), view depth z = v1.w
uvw   = (SV_Position.xy * FC_ScreenSize.zw,
         log2(min(z, F0.w) + F0.x) * F0.y + F0.z)               // F0..F3 = FC_VolumeFogParam[0..3]
fog   = t47.Sample(clamp, uvw)                                  // rgb in-scatter (pre-exposed), a = transmittance
if (F1.w < z) { ... analytic height-fog extension past the froxel range, see the asm lines 476-551:
    uses SC_RenderCameraPos, F0.w, F2.w (height), F3.xyz (falloff, density, constant),
    samples the last slice (w=1) and adds: fog.rgb += far.rgb*(1-exp(-tau*F1.rgb))/F1.rgb * fog.a^F1.rgb,
    fog.a *= exp(-(1-far.a)*tau) }                               // engine part, not decoded further
T     = pow(fog.a, F1.xyz)                                      // per-channel transmittance

SV_Target0.rgb = (max(col, 0) * T + fog.rgb * SC_InvPreExposure) * SC_PreExposure
SV_Target0.a   = a_out
SV_Target1.rgb = col * FC_GlowScale.rgb * saturate(FC_GlowScale.w) * T * SC_PreExposure
SV_Target1.a   = a_out
```
- No tonemap in the shader. The only scale is the engine pre-exposure (and fog). Target1 is the glow
  (bloom source) output. MTD `g_GlowScale` = (1,1,1,0.2) morning, (1,1,1,1) afternoon. Which buffer
  RT1 is has not been verified, and whether g_GlowScale gets the 2.2 power is untraced (it is not an
  HDRColor snp param, probably raw).
- `col` is linear HDR: colour^2.2*intensity values, e.g. morning color_2-Intensity 30 makes the
  sunlit cloud rims very bright. Switch 0's `sky` term is clamped to 1 before the cloud blend.
  Cloud colours are not clamped.

## Uncertain / not traced
- ONE = DAT_143f085ec assumed 1.0. The runtime override lerp in HDRColor is not traced.
- FC_DirLightVec_0 sign convention (assumed toward the sun because of the dp3_sat lobe).
- FC_TextureTilingScale[0/1] and VC_TexScroll_0/1 values (engine, assumed identity).
- The flow vector depends on the dome's per-vertex tangent frames (B.x component of world X). With the
  -0 factor on V, the clouds drift along U only. The 0.075 constant is baked in the shader.
- aColor[slot].w alpha source; RT1 meaning; the far-fog branch details.
