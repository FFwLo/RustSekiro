// Map pieces (map.rs): the game's layered map materials (M[Multiple], M[MultipleGround],
// docs/kb/map.md "Layers") on top of Bevy's StandardMaterial. Bins of version 5 take the game's
// own blend (`game_layers`, decompiled); older bins the first reading below. Base albedo from the
// StandardMaterial; the overlay (moss, a second stone) and the snow come as extra textures and
// blend by the vertex blend bytes (vertex colour: x overlay, y layer C, z snow, w byte 2) and,
// for the snow, how much the surface faces up. Normal maps as sekiro_material.wgsl (RG, DirectX
// green, B = gloss).

#import bevy_pbr::{
    mesh_view_bindings::globals,
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
    pbr_types::{PbrInput, STANDARD_MATERIAL_FLAGS_UNLIT_BIT},
    pbr_bindings,
    mesh_functions,
    view_transformations::position_world_to_clip,
    forward_io::{VertexOutput, FragmentOutput},
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var normal_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var normal_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var over_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(103) var over_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(104) var over_normal_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(105) var over_normal_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(106) var snow_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(107) var snow_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(108) var snow_normal_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(109) var snow_normal_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(112) var c_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(113) var c_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(114) var c_normal_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(115) var c_normal_sampler: sampler;
// The base layer's _3m mask: R read as height (block faces high, mortar / plank gaps low).
@group(#{MATERIAL_BIND_GROUP}) @binding(116) var mask_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(117) var mask_sampler: sampler;
// x: has a base normal map (bit 1: has layer C, bit 2: layer C with a normal), y: has an
// overlay (bit 1: with a normal), z: has snow (bit 1: with a normal), w: has a base albedo
// (bit 1: show the blend weights; bit 2: byte 2 darkens (AO); bit 3: byte 2 blends layer C).
@group(#{MATERIAL_BIND_GROUP}) @binding(110) var<uniform> map_flags: vec4<u32>;
// x: overlay uv set (0 / 1; for flow smoke: the mask's tiling), y: snow uv set, z: snow up-facing start (N.y), w: snow up-facing full.
@group(#{MATERIAL_BIND_GROUP}) @binding(111) var<uniform> map_params: vec4<f32>;
// x: has a mask, y: blend softness (0 = the plain vertex weight), z: flow smoke (the cloud sea,
// `smoke` below), w: its pace.
@group(#{MATERIAL_BIND_GROUP}) @binding(118) var<uniform> mask_params: vec4<f32>;
// The game path (bin v5): the detail normal (M[Multiple] slot 0 / M[MultipleGround] 12), the
// layer mask on uv1 (M[Multiple] slot 6), the wet flow normal (slot 16 / 0); sampled with
// normal_sampler (every map texture repeats).
@group(#{MATERIAL_BIND_GROUP}) @binding(120) var detail_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(121) var mask_l_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(122) var wet_texture: texture_2d<f32>;
struct MapGame {
    // x: texture bits (map.rs GameParams), y: kind (0 the old path, 1 M[Multiple],
    // 2 M[MultipleGround]), z: show the blend weights.
    flags: vec4<u32>,
    p: array<vec4<f32>, 16>,
}
@group(#{MATERIAL_BIND_GROUP}) @binding(123) var<uniform> map_game: MapGame;

// The vertex stage: Bevy's mesh.wgsl for static meshes plus the game path's extra vertex data
// (MAP_GAME_ATTRS when the mesh carries it: bin v5 pieces).
struct MapVertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
#ifdef VERTEX_UVS_A
    @location(2) uv: vec2<f32>,
#endif
#ifdef VERTEX_UVS_B
    @location(3) uv_b: vec2<f32>,
#endif
#ifdef VERTEX_TANGENTS
    @location(4) tangent: vec4<f32>,
#endif
#ifdef VERTEX_COLORS
    @location(5) color: vec4<f32>,
#endif
#ifdef MAP_GAME_ATTRS
    @location(8) blend2: vec4<f32>,
    @location(9) uv_scale: f32,
#endif
}

struct MapVertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) world_position: vec4<f32>,
    @location(1) world_normal: vec3<f32>,
#ifdef VERTEX_UVS_A
    @location(2) uv: vec2<f32>,
#endif
#ifdef VERTEX_UVS_B
    @location(3) uv_b: vec2<f32>,
#endif
#ifdef VERTEX_TANGENTS
    @location(4) world_tangent: vec4<f32>,
#endif
#ifdef VERTEX_COLORS
    @location(5) color: vec4<f32>,
#endif
#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    @location(6) @interpolate(flat) instance_index: u32,
#endif
#ifdef VISIBILITY_RANGE_DITHER
    @location(7) @interpolate(flat) visibility_range_dither: i32,
#endif
    @location(8) blend2: vec4<f32>,
    @location(9) uv_scale: f32,
}

@vertex
fn vertex(v: MapVertex) -> MapVertexOutput {
    var out: MapVertexOutput;
    let world_from_local = mesh_functions::get_world_from_local(v.instance_index);
    out.world_normal = mesh_functions::mesh_normal_local_to_world(v.normal, v.instance_index);
    out.world_position = mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(v.position, 1.0));
    out.position = position_world_to_clip(out.world_position.xyz);
#ifdef VERTEX_UVS_A
    out.uv = v.uv;
#endif
#ifdef VERTEX_UVS_B
    out.uv_b = v.uv_b;
#endif
#ifdef VERTEX_TANGENTS
    out.world_tangent = mesh_functions::mesh_tangent_local_to_world(world_from_local, v.tangent, v.instance_index);
#endif
#ifdef VERTEX_COLORS
    out.color = v.color;
#endif
#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    out.instance_index = v.instance_index;
#endif
#ifdef VISIBILITY_RANGE_DITHER
    out.visibility_range_dither = mesh_functions::get_visibility_range_dither_level(v.instance_index, world_from_local[3]);
#endif
#ifdef MAP_GAME_ATTRS
    out.blend2 = v.blend2;
    out.uv_scale = v.uv_scale;
#else
    out.blend2 = vec4(1.0, 1.0, 1.0, 0.0);
    out.uv_scale = 1.0;
#endif
    return out;
}

// A layer's coverage from its vertex weight and the base height: the layer wins where the
// weight exceeds the threshold, within `softness` of it. Overlays (moss, dirt) fill the low
// parts first (threshold = height), snow covers the high parts first (threshold = 1 - height).
fn coverage(weight: f32, threshold: f32) -> f32 {
    let s = mask_params.y;
    if s <= 0.0 {
        return weight;
    }
    // The threshold stays within [s, 1 - s]: weight 0 is nothing and weight 1 everything
    // whatever the height.
    let t = threshold * (1.0 - 2.0 * s) + s;
    return smoothstep(t - s, t + s, weight);
}

fn unpack_normal(n: vec4<f32>) -> vec3<f32> {
    var xy = n.rg * 2.0 - 1.0;
    xy.y = -xy.y; // DirectX green
    return vec3(xy, sqrt(max(1.0 - dot(xy, xy), 0.0)));
}

// The Divine Dragon arena's cloud sea (M[Flow]_Smoke, map.rs alpha 3, kb/map.md): a stand-in.
// The game's shader scrolls a height / blend mask (mask_texture, m25_00_smoke_g) along a flow
// vector map (normal_texture, m25_00_SmokeFlow_v); here the mask is sampled at two phases of
// the flow half a cycle apart and cross-faded (the flow-map trick), drifted by the MTD's two
// float2 rates, and shaded between the MTD's shadow tint (color_3) and light tint (color_2, the
// base colour). gap: the SPX shader's own math (its floats 0..13, which uv set, the view fade).
fn smoke(in: VertexOutput, pbr_input: PbrInput) -> FragmentOutput {
    let t = globals.time * mask_params.w;
    // The mesh's uv spans -1..2 over the 490 m plane: the mask repeats map_params.x times per unit.
    let uv = in.uv * map_params.x;
    let flow = textureSample(normal_texture, normal_sampler, in.uv).rg * 2.0 - 1.0;
    let cycle = 6.0;
    let p0 = fract(t / cycle);
    let p1 = fract(t / cycle + 0.5);
    let w = abs(2.0 * p0 - 1.0);
    let drift = vec2(-0.02, 0.01) * t * 0.05;
    let h0 = textureSample(mask_texture, mask_sampler, uv + flow * p0 * 0.06 + drift).r;
    let h1 = textureSample(mask_texture, mask_sampler, uv + flow * p1 * 0.06 + drift).r;
    // The mask is a soft grey cloud (R 0.61..0.88, mean 0.77): stretched to the full range.
    let h = smoothstep(0.62, 0.88, mix(h0, h1, w));
    let shadow = vec3(0.667, 0.706, 0.765);
    let light = pbr_bindings::material.base_color.rgb;
    let rgb = mix(shadow, light, h);
    let a = 0.92 + 0.08 * h;
    var input = pbr_input;
    input.material.base_color = vec4(rgb, a);
    var out: FragmentOutput;
    out.color = input.material.base_color;
    out.color = main_pass_post_lighting_processing(input, out.color);
    return out;
}

// ---------------------------------------------------------------------------------------------
// The game's own layer blend (bin version 5, kind 1 M[Multiple] / 2 M[MultipleGround]):
// decompiled from shader/shaderbdle.shaderbdlebnd.dcx M[Multiple].shaderbdle and
// M[MultipleGround].shaderbdle, Gbuf pass (docs/kb/map.md "The sat layer shaders, decompiled").
// The constants come from the material's MTD (`map_game.p`, layout in map.rs `GameParams`).
// Vertex data: color = (b0 overlay, b1 layer L, b4 snow, b5 detail strength), blend2 =
// (b2 tint rgb, b3 wet), uv_scale = |part scale X| (the game's uvS = uv0 x |world X axis|).

// hb: the game's height blend of a vertex weight against a mask height.
fn hb(w: f32, h: f32) -> f32 {
    return saturate((w - h * (1.0 - w)) * (1.0 + map_game.p[6].y));
}

// H: a mask's height for one layer: the material's float3 picks the channels, k scales it.
fn H(mask: vec3<f32>, sel: vec3<f32>, k: f32) -> f32 {
    return (dot(mask, sel) * k + 0.01) * map_game.p[6].x;
}

// FLV: a game normal map's xy (z rebuilt), DirectX green flipped into Bevy's tangent frame.
fn flv(n: vec4<f32>) -> vec3<f32> {
    return unpack_normal(n);
}

fn has(bit: u32) -> bool {
    return (map_game.flags.x & bit) != 0u;
}

#ifdef VERTEX_UVS_A
fn game_layers(in: VertexOutput, b: vec4<f32>, b2: vec4<f32>, uv_scale: f32, pbr_in: PbrInput) -> PbrInput {
    var pbr_input = pbr_in;
    let p = map_game.p;
    let ground = map_game.flags.y == 2u;
    let uv0 = in.uv;
    var uv1 = in.uv;
#ifdef VERTEX_UVS_B
    uv1 = in.uv_b;
#endif
    let uvs = uv0 * uv_scale;
    var m9 = vec3(0.0);
    if has(128u) { m9 = textureSample(mask_texture, mask_sampler, uvs).rgb; }
    // M[Multiple]'s layer L / wet / tint mask (slot 6) is on uv1; M[MultipleGround] has none and
    // uses mask 9 for every layer.
    var m6 = m9;
    if has(512u) { m6 = textureSample(mask_l_texture, normal_sampler, uv1).rgb; }

    var wOv = 0.0;
    if has(2u) { wOv = hb(b.x, H(m9, p[9].xyz, p[7].x)); }
    var wL = 0.0;
    if has(32u) {
        if ground { wL = hb(b.y, H(m9, p[10].xyz, p[7].y)); } else { wL = hb(b.y, H(m6, p[10].xyz, p[7].y)); }
    }
    var wWet = 0.0;
    if has(1024u) {
        if ground { wWet = saturate(b2.w * p[7].z); } else { wWet = hb(b2.w, H(m6, p[11].xyz, p[7].z)); }
    }
    let lum = dot(1.0 - b2.rgb, vec3(0.29891, 0.58661, 0.11448));
    let wTint = hb(lum, H(m6, p[12].xyz, p[8].x));

    // Normals: base / overlay (B gloss), the detail normal's xy added, layer L (ground).
    var nb = vec4(0.5, 0.5, 1.0, 0.15);
    if has(1u) { nb = textureSample(normal_texture, normal_sampler, uvs); }
    if has(4u) { nb = mix(nb, textureSample(over_normal_texture, over_normal_sampler, uvs), wOv); }
    if ground && has(64u) { nb = mix(nb, textureSample(c_normal_texture, c_normal_sampler, uvs), wL); }
    var nt = flv(nb);
    var gloss = nb.b;
    if has(256u) {
        let d = mix(vec3(0.502, 0.502, 1.0), textureSample(detail_texture, normal_sampler, uv0).rgb, b.w);
        nt = vec3(nt.xy + flv(vec4(d, 1.0)).xy * p[6].z, nt.z);
    }
    // Wet: a flow normal scrolled on uv1, gloss toward the wet gloss.
    if wWet > 0.0 {
        let wn = textureSample(wet_texture, normal_sampler, uv1 + globals.time * p[15].xy);
        nt = vec3(nt.xy + flv(wn).xy * p[6].w * wWet, nt.z);
        gloss = mix(gloss, p[8].y, wWet);
    }
    var Nw = pbr_input.world_normal;
#ifdef VERTEX_TANGENTS
    let N0 = pbr_input.world_normal;
    let T = in.world_tangent.xyz;
    let B = in.world_tangent.w * cross(N0, T);
    Nw = normalize(nt.x * T + nt.y * B + normalize(nt).z * N0);
#endif
    // Snow: how much the bumped world normal faces the material's snow direction (not
    // normalised), times the vertex weight, height-blended on mask 9.
    var wSnow = 0.0;
    if has(8u) {
        let sd = dot(p[14].xyz, Nw) * b.z;
        wSnow = hb(sd, H(m9, p[13].xyz, p[7].w));
    }
    if wSnow > 0.0 && has(16u) {
        let sn = textureSample(snow_normal_texture, snow_normal_sampler, uvs);
        if ground {
            nt = vec3(nt.xy + flv(sn).xy * p[9].w * wSnow, nt.z);
        } else {
            nt = mix(nt, flv(sn), wSnow);
        }
        gloss = mix(gloss, sn.b, wSnow);
    }

    // Albedo.
    var a = vec3(1.0);
    if has(2048u) { a = textureSample(pbr_bindings::base_color_texture, pbr_bindings::base_color_sampler, uvs).rgb; }
    var alpha = pbr_bindings::material.base_color.a;
    if ground {
        if has(2u) { a = mix(a, textureSample(over_texture, over_sampler, uvs).rgb, wOv); }
        if has(32u) { a = mix(a, textureSample(c_texture, c_sampler, uvs).rgb, wL); }
        a = a * mix(vec3(1.0), b2.rgb, p[8].x);
        // gap: M[MultipleGround] wet darkening read as a lerp toward color_1 (kb/map.md).
        a = mix(a, p[4].rgb, wWet * p[10].w);
    } else {
        a = a * mix(p[0].rgb, p[1].rgb, b.x);
        if has(2u) { a = mix(a, textureSample(over_texture, over_sampler, uvs).rgb * p[2].rgb, wOv); }
        if has(32u) { a = mix(a, textureSample(c_texture, c_sampler, uv1 + p[8].zw).rgb * p[3].rgb, wL); }
        a = mix(a, a * b2.rgb * p[4].rgb, wTint);
    }
    if has(8u) { a = mix(a, textureSample(snow_texture, snow_sampler, uvs).rgb * p[5].rgb, wSnow); }
    a = saturate(a);
    if map_game.flags.z != 0u {
        // Debug (SHINOBI_MAP_SHOW_BLEND=1): overlay red, layer L green, snow blue.
        a = vec3(wOv, wL, wSnow);
    }
    pbr_input.material.base_color = vec4(a, alpha);
    pbr_input.material.perceptual_roughness = clamp(1.0 - saturate(gloss), 0.089, 1.0);
#ifdef VERTEX_TANGENTS
    var mapped = normalize(nt.x * T + nt.y * B + nt.z * N0);
    if dot(N0, pbr_input.V) < 0.0 {
        mapped = mapped - 2.0 * dot(mapped, N0) / dot(N0, N0) * N0;
    }
    pbr_input.N = mapped;
#endif
    return pbr_input;
}
#endif

@fragment
fn fragment(m: MapVertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var in: VertexOutput;
    in.position = m.position;
    in.world_position = m.world_position;
    in.world_normal = m.world_normal;
#ifdef VERTEX_UVS_A
    in.uv = m.uv;
#endif
#ifdef VERTEX_UVS_B
    in.uv_b = m.uv_b;
#endif
#ifdef VERTEX_TANGENTS
    in.world_tangent = m.world_tangent;
#endif
#ifdef VERTEX_COLORS
    in.color = m.color;
#endif
#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    in.instance_index = m.instance_index;
#endif
#ifdef VISIBILITY_RANGE_DITHER
    in.visibility_range_dither = m.visibility_range_dither;
#endif
    var pbr_input = pbr_input_from_standard_material(in, is_front);
#ifdef VERTEX_UVS_A
    if map_game.flags.y != 0u {
        var b = vec4(0.0, 0.0, 0.0, 1.0);
#ifdef VERTEX_COLORS
        b = in.color;
#endif
        pbr_input = game_layers(in, b, m.blend2, m.uv_scale, pbr_input);
        pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);
        var out: FragmentOutput;
        if (pbr_input.material.flags & STANDARD_MATERIAL_FLAGS_UNLIT_BIT) == 0u {
            out.color = apply_pbr_lighting(pbr_input);
        } else {
            out.color = pbr_input.material.base_color;
        }
        out.color = main_pass_post_lighting_processing(pbr_input, out.color);
        return out;
    }
#endif

#ifdef VERTEX_UVS_A
    if mask_params.z > 0.5 {
        return smoke(in, pbr_input);
    }
    var uv_over = in.uv;
    var uv_snow = in.uv;
#ifdef VERTEX_UVS_B
    if map_params.x > 0.5 { uv_over = in.uv_b; }
    if map_params.y > 0.5 { uv_snow = in.uv_b; }
#endif
    var blend = vec4(0.0);
#ifdef VERTEX_COLORS
    blend = in.color;
#endif
    // Base colour: the StandardMaterial's albedo without the vertex colour Bevy multiplied in.
    var color = pbr_input.material.base_color;
    if map_flags.w != 0u {
        color = pbr_bindings::material.base_color * textureSample(pbr_bindings::base_color_texture, pbr_bindings::base_color_sampler, in.uv);
    } else {
        color = pbr_bindings::material.base_color;
    }
    var height = 0.5;
    if mask_params.x > 0.5 {
        height = textureSample(mask_texture, mask_sampler, in.uv).r;
    }
    var nt = vec3(0.0, 0.0, 1.0);
    var gloss = 0.15;
    if (map_flags.x & 1u) != 0u {
        let n = textureSample(normal_texture, normal_sampler, in.uv);
        nt = unpack_normal(n);
        gloss = n.b;
    }
    if (map_flags.w & 4u) != 0u {
        color = vec4(color.rgb * blend.w, color.a);
    }
    if (map_flags.x & 2u) != 0u && (map_flags.w & 8u) != 0u && blend.w > 0.0 {
        let c = textureSample(c_texture, c_sampler, in.uv);
        let w = coverage(blend.w, height) * c.a;
        color = vec4(mix(color.rgb, c.rgb, w), color.a);
        if (map_flags.x & 4u) != 0u {
            let n = textureSample(c_normal_texture, c_normal_sampler, in.uv);
            nt = mix(nt, unpack_normal(n), w);
            gloss = mix(gloss, n.b, w);
        }
    }
    if map_flags.y != 0u && blend.x > 0.0 {
        let o = textureSample(over_texture, over_sampler, uv_over);
        let w = coverage(blend.x, height) * o.a;
        color = vec4(mix(color.rgb, o.rgb, w), color.a);
        if (map_flags.y & 2u) != 0u {
            let n = textureSample(over_normal_texture, over_normal_sampler, uv_over);
            nt = mix(nt, unpack_normal(n), w);
            gloss = mix(gloss, n.b, w);
        }
    }
    if map_flags.z != 0u && blend.z > 0.0 {
        // Snow settles on surfaces that face up.
        let up = smoothstep(map_params.z, map_params.w, pbr_input.world_normal.y);
        let w = coverage(blend.z * up, 1.0 - height);
        if w > 0.0 {
            let sn = textureSample(snow_texture, snow_sampler, uv_snow);
            color = vec4(mix(color.rgb, sn.rgb, w), color.a);
            if (map_flags.z & 2u) != 0u {
                let n = textureSample(snow_normal_texture, snow_normal_sampler, uv_snow);
                nt = mix(nt, unpack_normal(n), w);
                gloss = mix(gloss, n.b, w);
            }
        }
    }
    if (map_flags.w & 2u) != 0u {
        // Debug: the blend weights (SHINOBI_MAP_SHOW_BLEND=1).
        color = vec4(blend.x, blend.z, blend.w, 1.0);
    }
    pbr_input.material.base_color = color;
    pbr_input.material.perceptual_roughness = clamp(1.0 - gloss, 0.089, 1.0);
#ifdef VERTEX_TANGENTS
    if map_flags.x != 0u || map_flags.y != 0u || map_flags.z != 0u {
        let N = pbr_input.world_normal;
        let T = in.world_tangent.xyz;
        let B = in.world_tangent.w * cross(N, T);
        var mapped = normalize(nt.x * T + nt.y * B + nt.z * N);
        if dot(N, pbr_input.V) < 0.0 {
            mapped = mapped - 2.0 * dot(mapped, N) / dot(N, N) * N;
        }
        pbr_input.N = mapped;
    }
#endif
#endif
    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);

    var out: FragmentOutput;
    if (pbr_input.material.flags & STANDARD_MATERIAL_FLAGS_UNLIT_BIT) == 0u {
        out.color = apply_pbr_lighting(pbr_input);
    } else {
        out.color = pbr_input.material.base_color;
    }
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
