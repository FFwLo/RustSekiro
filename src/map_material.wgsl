// Map pieces (map.rs): the game's layered map materials (M[Multiple], M[MultipleGround],
// docs/kb/map.md "Layers") on top of Bevy's StandardMaterial. Base albedo from the
// StandardMaterial; the overlay (moss, a second stone) and the snow come as extra textures and
// blend by the vertex blend bytes (vertex colour: x overlay, y layer C, z snow, w byte 2) and,
// for the snow, how much the surface faces up. Normal maps as sekiro_material.wgsl (RG, DirectX
// green, B = gloss).

#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
    pbr_types::STANDARD_MATERIAL_FLAGS_UNLIT_BIT,
    pbr_bindings,
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
// x: overlay uv set (0 / 1), y: snow uv set, z: snow up-facing start (N.y), w: snow up-facing full.
@group(#{MATERIAL_BIND_GROUP}) @binding(111) var<uniform> map_params: vec4<f32>;
// x: has a mask, y: blend softness (0 = the plain vertex weight), z / w: unused.
@group(#{MATERIAL_BIND_GROUP}) @binding(118) var<uniform> mask_params: vec4<f32>;

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

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);

#ifdef VERTEX_UVS_A
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
