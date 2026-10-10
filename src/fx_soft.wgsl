// FXR sprites / tracers (fxr.rs SoftExt on StandardMaterial): unlit texture x vertex colour, the
// alpha faded where the particle meets the scene, after the game's soft shaders
// (GXFfxtessellateSoftTexture.ppo / GXFfxsoftTracer.ppo, shader/gxffxshader.shaderbnd):
//   scene = linear depth of g_depthTexture at the pixel, me = the particle's (already pulled
//   toward the camera by the CPU, as their .gpo / .vpo pull it), soft = the per-vertex soft
//   distance (uv_b.x): alpha *= saturate(min(scene - me, soft) / soft); none when soft < 0.0001.

#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, main_pass_post_lighting_processing},
    forward_io::{VertexOutput, FragmentOutput},
    view_transformations::depth_ndc_to_view_z,
}

// The scene depth after the opaque pass (fxr.rs FxDepth; 0 = far when not copied).
@group(#{MATERIAL_BIND_GROUP}) @binding(100) var fx_depth: texture_depth_multisampled_2d;

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);
#ifdef VERTEX_UVS_B
    let soft = in.uv_b.x;
    if soft > 0.0001 {
        let scene = -depth_ndc_to_view_z(textureLoad(fx_depth, vec2<i32>(in.position.xy), 0));
        let me = -depth_ndc_to_view_z(in.position.z);
        pbr_input.material.base_color.a *= clamp(min(scene - me, soft) / soft, 0.0, 1.0);
    }
#endif
    var out: FragmentOutput;
    out.color = main_pass_post_lighting_processing(pbr_input, pbr_input.material.base_color);
    return out;
}
