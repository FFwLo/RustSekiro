// FXR sprites / tracers (fxr.rs SoftExt on StandardMaterial): unlit texture x vertex colour, the
// alpha faded where the particle meets the scene, after the game's soft shaders
// (GXFfxtessellateSoftTexture.ppo / GXFfxsoftTracer.ppo, shader/gxffxshader.shaderbnd):
//   scene = linear depth of g_depthTexture at the pixel, me = the particle's (already pulled
//   toward the camera by the CPU, as their .gpo / .vpo pull it), soft = the per-vertex soft
//   distance (uv_b.x): alpha *= saturate(min(scene - me, soft) / soft); none when soft < 0.0001.

import bevy_pbr::render::{ pbr_fragment::pbr_input_from_standard_material, pbr_functions::{alpha_discard, main_pass_post_lighting_processing}, forward_io::{VertexOutput, FragmentOutput}, view_transformations::depth_ndc_to_view_z, };

// The scene depth after the opaque pass (fxr.rs FxDepth; 0 = far when not copied).
@group(constants::MATERIAL_BIND_GROUP) @binding(100) var fx_depth: texture_depth_multisampled_2d;
// Lit particles' light (fxr.rs SoftExt.light): [0] ambient, [1] sun, rgb on the map's scale.
@group(constants::MATERIAL_BIND_GROUP) @binding(101) var<uniform> fx_light: array<vec4<f32>, 2>;

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    pbr_input.material.base_color = alpha_discard(pbr_input.material.flags, pbr_input.material.alpha_cutoff, pbr_input.material.base_color);
@if(VERTEX_UVS_B) {
    // Lit (uv_b.y = 1 + sat(N.L)): the game's PS_VFX_SingleTextureAllLight / BumpLight,
    // saturate(texture x colour) x (ambient + sun x N.L) (mov_sat before the light multiply).
    if in.uv_b.y >= 1.0 {
        let ndl = in.uv_b.y - 1.0;
        let c = clamp(pbr_input.material.base_color.rgb, vec3(0.0), vec3(1.0));
        pbr_input.material.base_color = vec4(c * (fx_light[0].rgb + fx_light[1].rgb * ndl), pbr_input.material.base_color.a);
    }
    let soft = in.uv_b.x;
    if soft > 0.0001 {
        let scene = -depth_ndc_to_view_z(textureLoad(fx_depth, vec2<i32>(in.position.xy), 0));
        let me = -depth_ndc_to_view_z(in.position.z);
        pbr_input.material.base_color.a *= clamp(min(scene - me, soft) / soft, 0.0, 1.0);
    }
}
    var out: FragmentOutput;
    out.color = main_pass_post_lighting_processing(pbr_input, pbr_input.material.base_color);
    return out;
}
