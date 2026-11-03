import bevy_pbr::render::forward_io::Vertex;
import bevy_pbr::render::forward_io::VertexOutput;
import bevy_pbr::render::mesh_view_bindings::view;
import bevy_pbr::render::mesh_view_bindings::view_transmission_texture;
import bevy_pbr::render::mesh_view_bindings::view_transmission_sampler;

@group(constants::MATERIAL_BIND_GROUP) @binding(0) var normals: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(1) var normal_sampler: sampler;

@vertex
fn vertex(v: Vertex) -> VertexOutput {
    var out: VertexOutput;
    out.position = view.clip_from_world * vec4(v.position, 1.0);
    out.world_position = vec4(v.position, 1.0);
    out.world_normal = v.normal;
    out.uv = v.uv;
    out.color = v.color;
    out.world_tangent = v.tangent;
@if(VERTEX_OUTPUT_INSTANCE_INDEX)
    out.instance_index = v.instance_index;
    return out;
}

@fragment
fn fragment(v: VertexOutput) -> @location(0) vec4<f32> {
    let circle = max(0.0, 1.0 - length(v.world_tangent.zw * 2.0 - 1.0) * v.world_tangent.y);
    let normal = textureSample(normals, normal_sampler, v.uv).xy * 2.0 - 1.0;
    let screen = (v.position.xy - view.viewport.xy) / view.viewport.zw;
    let offset = normal * v.world_tangent.x * circle * 0.01;
    let background = textureSampleLevel(view_transmission_texture, view_transmission_sampler, clamp(screen + offset, vec2(0.001), vec2(0.999)), 0.0);
    return vec4(background.rgb, clamp(v.color.a * circle, 0.0, 1.0));
}
