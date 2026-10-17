// Sekiro character materials ("Character_AMSN": Albedo / Metallic / Shininess / Normal) on top of
// Bevy's StandardMaterial. The normal map (_n, BC7) packs the tangent-space normal in RG (DirectX
// green) and the shininess in B; the metallic mask (_m, BC4) is a single channel. The base
// StandardMaterial carries the albedo and alpha mask only.

#import bevy_pbr::{
    pbr_bindings,
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
    pbr_types::PbrInput,
    forward_io::{VertexOutput, FragmentOutput},
    mesh_view_bindings::globals,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var normal_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var normal_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var metallic_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(103) var metallic_sampler: sampler;
// x: has a normal map, y: has a metallic mask.
@group(#{MATERIAL_BIND_GROUP}) @binding(104) var<uniform> sekiro_flags: vec4<u32>;
// Character_AMSN[DetailBlend] (docs/kb/visuals.md "DetailBlend", decompiled Gbuf variants): the
// fabric layers A (slots 1 / 6), B (2 / 5), C (10 / 12), D (4 / 9), all on uv0 x their group's
// tiling, sampled with layer A's (repeating) sampler.
@group(#{MATERIAL_BIND_GROUP}) @binding(110) var layer_a: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(111) var layer_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(112) var layer_a_n: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(113) var layer_b: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(114) var layer_b_n: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(115) var layer_c: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(116) var layer_c_n: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(117) var layer_d: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(118) var layer_d_n: texture_2d<f32>;
struct Detail {
    // x: 1 DetailBlend, 2 the coat shader (`fresnel_layers`), y: layer bits (1 B, 2 C, 4 D; 8 / 16 / 32 = that layer blends after the base
    // modulation; 64 back faces), z: bound textures (bit 0 A albedo .. bit 7 D normal).
    flags: vec4<u32>,
    // [0] color_0 (A), [1] color_1 (B), [2] color_5 (C), [3] color_2 (D) (rgb x Intensity);
    // [4] gloss adds A B C D (float_0 / 1 / 15 / 13); [5] metal A B C D (float_2 / 8 / 16 / 5);
    // [6].x float_10 (layer normal strength); [7] tiling base.xy, A.zw; [8] B.xy, C.zw; [9] D.xy.
    // The coat (x = 2): [0] color_1 (layer A tint), [1] color_3 (B), [2] color_2 (rim), [3]
    // color_0, [4] color_4 (back faces); [5] float_0 / 1 / 11 / 3; [6] float_5 / 8 / 9; [7]
    // tiling A albedo.xy, A normal.zw; [8] B albedo.xy, B normal.zw; [9] base.xy.
    // The ghost fire (x = 3): [0..5] color_0..color_5 (rgb^2.2 x Intensity, a); [6] float_0..3;
    // [7] float_4..7; [8] float_8, float_9, float_10, color_0.a; [9] slot 0 tiling.
    p: array<vec4<f32>, 10>,
}
@group(#{MATERIAL_BIND_GROUP}) @binding(119) var<uniform> detail: Detail;

// An empty slot is the game's dummy (other/systex.tpf): SYSTEX_Dummy_a is BC1 sRGB 128 grey,
// alpha 255 (linear 0.2159, about 1 / 4.55: a missing layer leaves the base albedo as it is);
// SYSTEX_Dummy_n is BC7 (128, 128, 128, 255): flat, gloss 0.5, fabric mask alpha 1.
const DUMMY_A: vec4<f32> = vec4(0.2159, 0.2159, 0.2159, 1.0);
const DUMMY_N: vec4<f32> = vec4(0.502, 0.502, 0.502, 1.0);

// One fabric layer: albedo, normal xy (FLV, Bevy's green), gloss, from its two textures.
struct Layer {
    albedo: vec3<f32>,
    nxy: vec2<f32>,
    gloss: f32,
    alpha: f32,
}

fn flv(n: vec4<f32>) -> vec2<f32> {
    var xy = n.rg * 2.0 - 1.0;
    xy.y = -xy.y;
    return xy;
}

fn layer(a: texture_2d<f32>, n: texture_2d<f32>, bit: u32, uv: vec2<f32>, color: vec3<f32>, gloss_add: f32) -> Layer {
    var l: Layer;
    var ta = DUMMY_A;
    var tn = DUMMY_N;
    if (detail.flags.z & (1u << bit)) != 0u { ta = textureSample(a, layer_sampler, uv); }
    if (detail.flags.z & (2u << bit)) != 0u { tn = textureSample(n, layer_sampler, uv); }
    l.albedo = ta.rgb * color;
    l.nxy = flv(tn);
    l.gloss = tn.b + gloss_add;
    l.alpha = ta.a;
    return l;
}

fn mix_layer(base: Layer, top: Layer, w: f32) -> Layer {
    var l = base;
    l.albedo = mix(base.albedo, top.albedo, w);
    l.nxy = mix(base.nxy, top.nxy, w);
    l.gloss = mix(base.gloss, top.gloss, w);
    return l;
}

#ifdef VERTEX_UVS_A
// DetailBlend: albedo = sat(a7 x 4.55 x layers), gloss = sat(2 x base normal B x layer gloss),
// normal = base + layer normal xy x float_10, metal = the layers' metal (base slot 3 + float_2 for
// A); masks from the base normal: wB = step at B 1/22, wC = (1 - A)^8, wD = A^8. Layers flagged
// "after" blend over the modulated result (gap: their gloss read the same way). Back faces
// (variants 15-20) are drawn as the front (gap).
fn detail_layers(in: VertexOutput, pbr_in: PbrInput) -> PbrInput {
    var pbr_input = pbr_in;
    let p = detail.p;
    let bits = detail.flags.y;
    let uv = in.uv;
    let base_uv = uv * p[7].xy;
    let a7 = textureSample(pbr_bindings::base_color_texture, pbr_bindings::base_color_sampler, base_uv);
    var n0 = DUMMY_N;
    if sekiro_flags.x != 0u { n0 = textureSample(normal_texture, normal_sampler, base_uv); }
    var m3 = 0.0;
    if sekiro_flags.y != 0u { m3 = textureSample(metallic_texture, metallic_sampler, base_uv).r; }
    let wB = min(pow(22.0 * n0.b, 260.0), 1.0);
    let wC = pow(1.0 - n0.a, 8.0);
    let wD = pow(n0.a, 8.0);

    let A = layer(layer_a, layer_a_n, 0u, uv * p[7].zw, p[0].rgb, p[4].x);
    var acc = A;
    var metal = m3 + p[5].x;
    var lb: Layer;
    var lc: Layer;
    var ld: Layer;
    if (bits & 1u) != 0u { lb = layer(layer_b, layer_b_n, 2u, uv * p[8].xy, p[1].rgb, p[4].y); }
    if (bits & 2u) != 0u { lc = layer(layer_c, layer_c_n, 4u, uv * p[8].zw, p[2].rgb, p[4].z); }
    if (bits & 4u) != 0u { ld = layer(layer_d, layer_d_n, 6u, uv * p[9].xy, p[3].rgb, p[4].w); }
    // B shows where the base normal's blue is about 0 (wB = 0).
    if (bits & 9u) == 1u { acc = mix_layer(lb, acc, wB); }
    if (bits & 18u) == 2u { acc = mix_layer(acc, lc, wC); }
    if (bits & 36u) == 4u { acc = mix_layer(acc, ld, wD); }
    if (bits & 1u) != 0u { metal = mix(p[5].y, metal, wB); }
    if (bits & 2u) != 0u { metal = mix(metal, p[5].z, wC); }
    if (bits & 4u) != 0u { metal = mix(metal, p[5].w, wD); }
    var gb = n0.b;
    if (bits & 1u) != 0u { gb = mix(0.5, n0.b, wB); }
    var albedo = saturate(acc.albedo * a7.rgb * 4.55);
    var gloss = saturate(2.0 * gb * acc.gloss);
    var nxy = acc.nxy;
    if (bits & 8u) != 0u { albedo = mix(lb.albedo, albedo, wB); gloss = mix(saturate(lb.gloss), gloss, wB); nxy = mix(lb.nxy, nxy, wB); }
    if (bits & 16u) != 0u { albedo = mix(albedo, lc.albedo, wC); gloss = mix(gloss, saturate(lc.gloss), wC); nxy = mix(nxy, lc.nxy, wC); }
    if (bits & 32u) != 0u { albedo = mix(albedo, ld.albedo, wD); gloss = mix(gloss, saturate(ld.gloss), wD); nxy = mix(nxy, ld.nxy, wD); }

    pbr_input.material.base_color = vec4(saturate(albedo), saturate(a7.a * A.alpha) * pbr_bindings::material.base_color.a);
    pbr_input.material.perceptual_roughness = clamp(1.0 - gloss, 0.089, 1.0);
    pbr_input.material.metallic = saturate(metal);
#ifdef VERTEX_TANGENTS
    if sekiro_flags.x != 0u {
        let b = flv(n0);
        let xy = b + nxy * p[6].x;
        let nt = normalize(vec3(xy, sqrt(max(1.0 - dot(b, b), 0.0))));
        let N = pbr_input.world_normal;
        let T = in.world_tangent.xyz;
        let Bt = in.world_tangent.w * cross(N, T);
        var mapped = normalize(nt.x * T + nt.y * Bt + nt.z * N);
        if dot(N, pbr_input.V) < 0.0 {
            mapped = mapped - 2.0 * dot(mapped, N) / dot(N, N) * N;
        }
        pbr_input.N = mapped;
    }
#endif
    return pbr_input;
}
#endif

#ifdef VERTEX_UVS_A
// Character_AN_Blend_AMSN_[Fresnel] (Wolf's coat; docs/kb/visuals.md "Coat", decompiled Gbuf):
// base normal A^10 picks fabric layer A (slots 3 / 1) over B (4 / 5); albedo = sat(4.55 x base
// albedo x layer albedo x tint), then lerped toward the rim colour by
// fres = (1 - sat(N.V - float_9))^float_8 on the final normal; gloss = sat(2 x base B x the layer
// normal's B (A's x float_3)); normal = base + the layer normal xy x float_1 / float_11; metal =
// slot 0 R + float_0. Back faces: sat(layer B albedo x color_4), no base, no rim.
fn fresnel_layers(in: VertexOutput, pbr_in: PbrInput) -> PbrInput {
    var pbr_input = pbr_in;
    let p = detail.p;
    let uv = in.uv;
    let base_uv = uv * p[9].xy;
    let a2 = textureSample(pbr_bindings::base_color_texture, pbr_bindings::base_color_sampler, base_uv);
    var n7 = DUMMY_N;
    if sekiro_flags.x != 0u { n7 = textureSample(normal_texture, normal_sampler, base_uv); }
    var m0 = 0.0;
    if sekiro_flags.y != 0u { m0 = textureSample(metallic_texture, metallic_sampler, base_uv).r; }
    var a3 = DUMMY_A;
    var n1 = DUMMY_N;
    var a4 = DUMMY_A;
    var n5 = DUMMY_N;
    if (detail.flags.z & 1u) != 0u { a3 = textureSample(layer_a, layer_sampler, uv * p[7].xy); }
    if (detail.flags.z & 2u) != 0u { n1 = textureSample(layer_a_n, layer_sampler, uv * p[7].zw); }
    if (detail.flags.z & 4u) != 0u { a4 = textureSample(layer_b, layer_sampler, uv * p[8].xy); }
    if (detail.flags.z & 8u) != 0u { n5 = textureSample(layer_b_n, layer_sampler, uv * p[8].zw); }
    let w = pow(n7.a, 10.0);
    let nxy = mix(flv(n5) * p[5].z, flv(n1) * p[5].y, w);
    let gloss = saturate(2.0 * n7.b * mix(n5.b, n1.b * p[5].w, w));
    var albedo = 4.55 * a2.rgb * mix(a4.rgb * p[1].rgb, a3.rgb * p[0].rgb, w);
    // Front / back by the vertex normal (the game: the winding; ours is reversed on the
    // X-mirrored meshes, and Bevy's double-sided world_normal follows the winding).
    let vn = normalize(in.world_normal);
    let back = dot(vn, pbr_input.V) < 0.0;
    let Ng = select(vn, -vn, back);
    pbr_input.N = Ng;
#ifdef VERTEX_TANGENTS
    if sekiro_flags.x != 0u {
        let b = flv(n7);
        let xy = b + nxy;
        let nt = normalize(vec3(xy, sqrt(max(1.0 - dot(b, b), 0.0))));
        let T = in.world_tangent.xyz;
        let Bt = in.world_tangent.w * cross(Ng, T);
        pbr_input.N = normalize(nt.x * T + nt.y * Bt + nt.z * Ng);
    }
#endif
    let fres = pow(1.0 - saturate(dot(pbr_input.N, pbr_input.V) - p[6].z), max(p[6].y, 0.001));
    albedo = mix(albedo, mix(p[2].rgb, albedo * p[3].rgb, p[6].x), fres);
    if back { albedo = a4.rgb * p[4].rgb; }
    pbr_input.material.base_color = vec4(saturate(albedo), saturate(a2.a) * pbr_bindings::material.base_color.a);
    pbr_input.material.perceptual_roughness = clamp(1.0 - gloss, 0.089, 1.0);
    pbr_input.material.metallic = saturate(m0 + p[5].x);
    return pbr_input;
}
#endif

#ifdef VERTEX_UVS_A
// C[Shell] (fur shells, C[Shell]_0_Gbuf_[A].ppo; docs/kb/visuals.md "Fur shells"): t0 = slot 6
// noise (layer A here), t4 = slot 7 colour (the base texture), t5 = slot 8 normal, t1 = slot 10
// metal; va = the vertex colour alpha (v4.w).
// albedo = sat(noise.rgb x color_0 x colour x 4.55 x (1 - va)^float_6), alpha = noise.a x float_4
// x va, cut below g_AlphaRef / 255; gloss = sat(normal.b + float_5); metal = sat(t1.r).
// gap: the dithered cut (FC_AlphaRef.y x (dither - 0.5)) is a hard step; o1.y occlusion
// ((1 - va)^float_3 x 0.1) is not used.
fn shell_layers(in: VertexOutput, pbr_in: PbrInput) -> PbrInput {
    var pbr_input = pbr_in;
    let p = detail.p;
    let uv = in.uv;
    var va = 1.0;
#ifdef VERTEX_COLORS
    va = in.color.a;
#endif
    var noise = DUMMY_A;
    if (detail.flags.z & 1u) != 0u { noise = textureSample(layer_a, layer_sampler, uv * p[7].xy); }
    let col = textureSample(pbr_bindings::base_color_texture, pbr_bindings::base_color_sampler, uv * p[7].zw);
    var n8 = DUMMY_N;
    if sekiro_flags.x != 0u { n8 = textureSample(normal_texture, normal_sampler, uv * p[8].xy); }
    var m10 = 0.0;
    if sekiro_flags.y != 0u { m10 = textureSample(metallic_texture, metallic_sampler, uv * p[7].zw).r; }
    let dark = pow(max(1.0 - va, 0.0), p[1].w);
    let albedo = saturate(noise.rgb * p[0].rgb * col.rgb * 4.55 * dark);
    let alpha = noise.a * p[1].y * va;
    pbr_input.material.base_color = vec4(albedo, select(0.0, 1.0, alpha >= p[2].x));
    pbr_input.material.perceptual_roughness = clamp(1.0 - saturate(n8.b + p[1].z), 0.089, 1.0);
    pbr_input.material.metallic = saturate(m10);
#ifdef VERTEX_TANGENTS
    if sekiro_flags.x != 0u {
        let xy = flv(n8);
        let nt = normalize(vec3(xy, sqrt(max(1.0 - dot(xy, xy), 0.0))));
        let N = pbr_input.world_normal;
        let T = in.world_tangent.xyz;
        let Bt = in.world_tangent.w * cross(N, T);
        var mapped = normalize(nt.x * T + nt.y * Bt + nt.z * N);
        if dot(N, pbr_input.V) < 0.0 {
            mapped = mapped - 2.0 * dot(mapped, N) / dot(N, N) * N;
        }
        pbr_input.N = mapped;
    }
#endif
    return pbr_input;
}
#endif

#ifdef VERTEX_UVS_A
// Character_Ghost_scroll_test2 (the Demon of Hatred's burning arm, c7020_arm_test.mtd; decoded
// from shader/shaderbdle.shaderbdlebnd Character_Ghost_scroll_test2_Fwd.ppo): the albedo (t0)
// scrolls by GlobalTime x (0.05, -0.05) and the normal map (t1) by (-0.05, -0.05); with
// v = N.V on the bumped normal, a = (1 - sat(v - float_4)^float_9)^float_3 and
// b = (1 - sat(v - float_2)^float_8)^float_1: albedo = sat(lerp(lerp(color_2, color_3, a),
// lerp(color_5, color_4, a), float_10) x (tex + float_7)), alpha = sat(tex.a x color_0.a x
// lerp(float_5, float_6, b)), emissive = tex x lerp(color_0, color_1, float_10), metal float_0.
// gap: the normal map's tiling is taken as slot 0's; its axes as our other normal maps' (the
// game puts x on the bitangent, y on the tangent); the gloss is not traced; float_10 (SAT_CAParam)
// is the exe's run-time value, 0 here.
fn ghost_scroll(in: VertexOutput, pbr_in: PbrInput) -> PbrInput {
    var pbr_input = pbr_in;
    let p = detail.p;
    let t = globals.time;
    let tile = select(vec2(1.0), p[9].xy, p[9].x != 0.0);
    let tex = textureSample(pbr_bindings::base_color_texture, pbr_bindings::base_color_sampler, (in.uv + t * vec2(0.05, -0.05)) * tile);
#ifdef VERTEX_TANGENTS
    if sekiro_flags.x != 0u {
        let n = textureSample(normal_texture, normal_sampler, (in.uv + t * vec2(-0.05, -0.05)) * tile);
        let xy = flv(n);
        let nt = vec3(xy, sqrt(max(1.0 - dot(xy, xy), 0.0)));
        let N = pbr_input.N;
        let T = in.world_tangent.xyz;
        let B = in.world_tangent.w * cross(N, T);
        pbr_input.N = normalize(nt.x * T + nt.y * B + nt.z * N);
    }
#endif
    let v = dot(pbr_input.N, pbr_input.V);
    let a = pow(1.0 - pow(saturate(v - p[7].x), max(p[8].y, 0.001)), max(p[6].w, 0.001));
    let b = pow(1.0 - pow(saturate(v - p[6].z), max(p[8].x, 0.001)), max(p[6].y, 0.001));
    let tp = tex.rgb + p[7].w;
    let albedo = saturate(mix(mix(p[2].rgb, p[3].rgb, a), mix(p[5].rgb, p[4].rgb, a), p[8].z) * tp);
    let alpha = saturate(tex.a * p[8].w * mix(p[7].y, p[7].z, b));
    pbr_input.material.base_color = vec4(albedo, alpha * pbr_bindings::material.base_color.a);
    pbr_input.material.emissive = vec4(tex.rgb * mix(p[0].rgb, p[1].rgb, p[8].z), 0.0);
    pbr_input.material.metallic = saturate(p[6].x);
    return pbr_input;
}
#endif

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
#ifdef VERTEX_UVS_A
    if detail.flags.x != 0u {
        if detail.flags.x == 4u {
            pbr_input = shell_layers(in, pbr_input);
        } else if detail.flags.x == 3u {
            pbr_input = ghost_scroll(in, pbr_input);
        } else if detail.flags.x == 2u {
            pbr_input = fresnel_layers(in, pbr_input);
        } else {
            pbr_input = detail_layers(in, pbr_input);
        }
        pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);
        var out: FragmentOutput;
        out.color = apply_pbr_lighting(pbr_input);
        out.color = main_pass_post_lighting_processing(pbr_input, out.color);
        return out;
    }
#endif
    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);

#ifdef VERTEX_UVS_A
    if sekiro_flags.x != 0u {
        let n = textureSample(normal_texture, normal_sampler, in.uv);
#ifdef VERTEX_TANGENTS
        // Tangent-space normal from RG; Z rebuilt with a clamp (Sekiro's XY often exceed the unit
        // circle - Bevy's two-component path would take sqrt of a negative there, NaN = black).
        var xy = n.rg * 2.0 - 1.0;
        xy.y = -xy.y; // DirectX green
        let nt = normalize(vec3(xy, sqrt(max(1.0 - dot(xy, xy), 0.0))));
        // Mikktspace TBN (unnormalised, as Bevy's calculate_tbn_mikktspace).
        let N = pbr_input.world_normal;
        let T = in.world_tangent.xyz;
        let B = in.world_tangent.w * cross(N, T);
        var mapped = normalize(nt.x * T + nt.y * B + nt.z * N);
        // Facing by the vertex normal, not the winding (the X-mirrored meshes have reversed
        // winding): the back side of thin cloth lights with the normal turned toward the viewer.
        if dot(N, pbr_input.V) < 0.0 {
            mapped = mapped - 2.0 * dot(mapped, N) / dot(N, N) * N;
        }
        pbr_input.N = mapped;
#endif
        // Shininess (gloss) -> perceptual roughness.
        pbr_input.material.perceptual_roughness = clamp(1.0 - n.b, 0.089, 1.0);
    }
    if sekiro_flags.y != 0u {
        pbr_input.material.metallic = textureSample(metallic_texture, metallic_sampler, in.uv).r;
    }
#endif

    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
