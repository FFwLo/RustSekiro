// FXR MultiTextureBillboardEx (604) particles (fxr.rs `MultiMaterial`), after the game's pixel
// shader GXFfxtessellateBlendMultiTexture.ppo (shader/gxffxshader.shaderbnd, d3dcompiler_47):
// - layer 1 (g_multiTexture0): the flipbook frame, mixed with the next one by the frame's fraction
//   (interpolateFrames), times layer1Color; alpha = its alpha.
// - layers 2 / 3 (g_multiTexture1 / 2) times layer2Color / layer3Color. g_ps_TexBlendType
//   (modes.x, FXR unk_ds3_f2_10): 1 makes layers 2 and 3, 2 only layer 2, act on the alpha by their
//   brightness (sum of the normalised colour / 3) instead of on the colour. The op per layer is
//   g_ps_TexBlendType2 / 3 (modes.y / .z, unk_ds3_f2_11 / 12): 0 multiply (lerp(1, layer, layer
//   alpha)), 1 add (layer x its alpha), 2 overlay (lerp by the layer alpha), else none.
// - g_ps_bPreAlphaMult (flags.x, premultiplyAlpha): each layer's colour times its alpha first.
// - out = (vertex colour direction x layers)^2 x |vertex colour|, alpha = vertex alpha x layer alpha
//   (the vertex colour arrives hue-squared from fxr.rs particle_color, so here only the layers are
//   squared). The textures are sRGB-decoded on load (~x^2.2); their square root stands in for the
//   game's raw values. Layers wrap (fract: the game's wrap sampler; they scroll).
// gap: g_ps_ColorBlendType (unk_ds3_f2_13) 1 / 2 (all extracted 604s: 0), the alpha offset (v3.z),
// the motion-vector UV bend (cb0[11].w == 1), fog, shadow volume, lighting.
// Soft (GXFfxtessellateSoftMultiTexture.ppo): alpha x saturate(min(scene depth - own depth, soft) /
// soft), soft = the per-vertex distance (location 8; 0 = off), as fx_soft.wgsl.

#import bevy_pbr::{
    mesh_functions::get_world_from_local,
    view_transformations::{position_world_to_clip, depth_ndc_to_view_z},
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var t0: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var s0: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var t1: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var s1: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var t2: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(5) var s2: sampler;
// x: TexBlendType, y: TexBlendType2, z: TexBlendType3, w: ColorBlendType.
@group(#{MATERIAL_BIND_GROUP}) @binding(6) var<uniform> modes: vec4<u32>;
// x: premultiply each layer by its alpha, y: additive (premultiplied output with alpha 0), z: has layer 3.
@group(#{MATERIAL_BIND_GROUP}) @binding(7) var<uniform> flags: vec4<u32>;
// The scene depth after the opaque pass (fxr.rs FxDepth; 0 = far when not copied).
@group(#{MATERIAL_BIND_GROUP}) @binding(8) var fx_depth: texture_depth_multisampled_2d;

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) uv1: vec2<f32>,
    @location(2) uv2: vec2<f32>,
    // xy: layer 3, zw: layer 1's next flipbook frame.
    @location(3) uv3n: vec4<f32>,
    @location(4) color: vec4<f32>,
    // rgb: layer1Color, w: the frame fraction (0 without interpolateFrames).
    @location(5) c1: vec4<f32>,
    @location(6) c2: vec4<f32>,
    @location(7) c3: vec4<f32>,
    @location(8) soft: f32,
};

struct VOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv1: vec2<f32>,
    @location(1) uv2: vec2<f32>,
    @location(2) uv3n: vec4<f32>,
    @location(3) color: vec4<f32>,
    @location(4) c1: vec4<f32>,
    @location(5) c2: vec4<f32>,
    @location(6) c3: vec4<f32>,
    @location(7) soft: f32,
};

@vertex
fn vertex(v: Vertex) -> VOut {
    var o: VOut;
    let world = get_world_from_local(v.instance_index) * vec4(v.position, 1.0);
    o.position = position_world_to_clip(world.xyz);
    o.uv1 = v.uv1;
    o.uv2 = v.uv2;
    o.uv3n = v.uv3n;
    o.color = v.color;
    o.c1 = v.c1;
    o.c2 = v.c2;
    o.c3 = v.c3;
    o.soft = v.soft;
    return o;
}

fn raw(c: vec4<f32>) -> vec4<f32> {
    return vec4(sqrt(max(c.rgb, vec3(0.0))), c.a);
}

fn overlay(b: vec3<f32>, l: vec3<f32>) -> vec3<f32> {
    return select(1.0 - 2.0 * (1.0 - b) * (1.0 - l), 2.0 * b * l, b < vec3(0.5));
}

// A layer on the colour: `pb` the base colour so far, `c` the layer colour, `la` its alpha.
fn color_op(pb: vec3<f32>, c: vec3<f32>, la: f32, mode: u32) -> vec3<f32> {
    switch mode {
        case 0u: { return pb * mix(vec3(1.0), c, la); }
        case 1u: { return pb + c * la; }
        case 2u: { return mix(pb, overlay(pb, c), la); }
        default: { return pb; }
    }
}

// A layer on the alpha by its brightness.
fn alpha_op(a: f32, c: vec3<f32>, la: f32, mode: u32) -> f32 {
    let len = length(c);
    let n = select(c / len, vec3(0.0), len <= 0.00001);
    let lum = clamp((n.x + n.y + n.z) / 3.0, 0.0, 1.0);
    switch mode {
        case 0u: { return a * (1.0 + la * (lum - 1.0)); }
        case 1u: { return a + la * lum; }
        case 2u: {
            let ov = select(1.0 - 2.0 * (1.0 - a) * (1.0 - lum), 2.0 * lum * a, a < 0.5);
            return a + la * (ov - a);
        }
        default: { return a; }
    }
}

@fragment
fn fragment(in: VOut) -> @location(0) vec4<f32> {
    let f0 = raw(textureSample(t0, s0, fract(in.uv1)));
    let f1 = raw(textureSample(t0, s0, fract(in.uv3n.zw)));
    let b = mix(f0, f1, in.c1.w);
    var a = clamp(b.a, 0.0, 1.0);
    let pre = flags.x != 0u;
    var rgb = b.rgb * in.c1.rgb * select(1.0, a, pre);
    let l2 = raw(textureSample(t1, s1, fract(in.uv2))) * in.c2;
    let l3 = raw(textureSample(t2, s2, fract(in.uv3n.xy))) * in.c3;
    let c2 = l2.rgb * select(1.0, l2.a, pre);
    let c3 = l3.rgb * select(1.0, l3.a, pre);
    if modes.x == 1u || modes.x == 2u {
        a = alpha_op(a, c2, l2.a, modes.y);
    } else {
        rgb = color_op(rgb, c2, l2.a, modes.y);
    }
    if flags.z != 0u {
        if modes.x == 1u {
            a = alpha_op(a, c3, l3.a, modes.z);
        } else {
            rgb = color_op(rgb, c3, l3.a, modes.z);
        }
    }
    a = clamp(a, 0.0, 1.0);
    var out_a = clamp(in.color.a * a, 0.0, 1.0);
    if in.soft > 0.0001 {
        let scene = -depth_ndc_to_view_z(textureLoad(fx_depth, vec2<i32>(in.position.xy), 0));
        let me = -depth_ndc_to_view_z(in.position.z);
        out_a *= clamp(min(scene - me, in.soft) / in.soft, 0.0, 1.0);
    }
    let out_rgb = in.color.rgb * rgb * rgb;
    return vec4(max(out_rgb, vec3(0.0)) * out_a, select(out_a, 0.0, flags.y != 0u));
}
