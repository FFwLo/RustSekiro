import bevy_pbr::render::forward_io::Vertex;
import bevy_pbr::render::forward_io::VertexOutput;
import bevy_pbr::render::mesh_view_bindings::view;
import bevy_pbr::render::view_transformations::depth_ndc_to_view_z;

// FXR sprite and tracer batches: unlit texture x vertex colour, scaled by the camera exposure
// (the colour arrives in scene units, see fxr.rs). Two alpha controls per vertex ride in the
// tangent: x the fade threshold, y the hard threshold. w is g_ps_bPreAlphaMult. z is the soft distance: where a particle
// meets the scene its alpha fades over that distance (soft particles, after the game's soft
// shaders as traced by FwLo's RustSekiro); 0 keeps a hard edge.

@group(constants::MATERIAL_BIND_GROUP) @binding(0) var particle_texture: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(1) var particle_sampler: sampler;
// The scene depth after the opaque pass (fxr.rs DepthCopy; 0 = far when not copied).
@group(constants::MATERIAL_BIND_GROUP) @binding(2) var fx_depth: texture_depth_multisampled_2d;

@vertex
fn vertex(v: Vertex) -> VertexOutput {
    var out: VertexOutput;
    // Batch corners are in world space; the entity transform is identity.
    out.position = view.clip_from_world * vec4(v.position, 1.0);
    out.world_position = vec4(v.position, 1.0);
    out.world_normal = v.normal;
    out.uv = v.uv;
    out.color = v.color;
    // This attribute carries alpha controls, not a surface tangent.
    out.world_tangent = v.tangent;
@if(VERTEX_OUTPUT_INSTANCE_INDEX)
    out.instance_index = v.instance_index;
    return out;
}

@fragment
fn fragment(v: VertexOutput) -> @location(0) vec4<f32> {
    var texture_color = textureSample(particle_texture, particle_sampler, v.uv);
    let fade = clamp(v.world_tangent.x, 0.0, 1.0);
    texture_color.a = clamp((texture_color.a - fade) / max(1.0 - fade, 0.00001), 0.0, 1.0);
    if texture_color.a < v.world_tangent.y { discard; }
    // g_ps_bPreAlphaMult (GXFfxtessellateBlendSoftTexture.ppo): the texture colour is scaled by
    // its alpha, then squared, so a soft glow's edge fades with alpha cubed rather than linearly.
    if v.world_tangent.w > 0.5 {
        texture_color = vec4(texture_color.rgb * texture_color.a * texture_color.a, texture_color.a);
    }
    var color = texture_color * v.color;
    let soft = v.world_tangent.z;
    if soft > 0.0001 {
        // Single-sample captures (the map sweep and pose server) carry Bevy's depth prepass,
        // whose depth cannot be copied into the four-sample slot below.
@if(DEPTH_PREPASS)
        let raw_depth = bevy_pbr::prepass::utils::prepass_depth(v.position, 0u);
@if(!DEPTH_PREPASS)
        let raw_depth = textureLoad(fx_depth, vec2<i32>(v.position.xy), 0);
        let scene = -depth_ndc_to_view_z(raw_depth);
        let me = -depth_ndc_to_view_z(v.position.z);
        color.a = color.a * clamp(min(scene - me, soft) / soft, 0.0, 1.0);
    }
    // Bevy routes AlphaMode::Add through premultiplied blending. Zero coverage keeps
    // the destination, while source alpha weights the emitted radiance.
@if(BLEND_PREMULTIPLIED_ALPHA)
    return vec4(color.rgb * view.exposure * clamp(color.a, 0.0, 1.0), 0.0);
@if(!BLEND_PREMULTIPLIED_ALPHA)
    return vec4(color.rgb * view.exposure, clamp(color.a, 0.0, 1.0));
}
