// Sekiro character materials ("Character_AMSN": Albedo / Metallic / Shininess / Normal) on top of
// Bevy's StandardMaterial. The normal map (_n, BC7) packs the tangent-space normal in RG (DirectX
// green) and the shininess in B; the metallic mask (_m, BC4) is a single channel. The base
// StandardMaterial carries the albedo and alpha mask only.

#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
    forward_io::{VertexOutput, FragmentOutput},
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var normal_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var normal_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var metallic_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(103) var metallic_sampler: sampler;
// x: has a normal map, y: has a metallic mask.
@group(#{MATERIAL_BIND_GROUP}) @binding(104) var<uniform> sekiro_flags: vec4<u32>;

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
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
