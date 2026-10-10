// FXR screen effects (fxr.rs `DistortMaterial`): Distortion (607), the distortion of tracers
// (distortionIntensity) and RadialBlur (608), after the game's pixel shaders
// (shader/gxffxshader.shaderbnd, disassembled with d3dcompiler_47):
// - GXFfxdistortionBump.ppo: d = |2 (uv - 0.5)|, fall = max(1 - g_clampDistanceInverse * d, 0) * 0.01;
//   the frame buffer is read at the pixel + (normal.rg * 2 - 1) * g_waveStrength * fall and tinted
//   by g_color. gap: its depth test (a nearer surface at the bent spot keeps the straight read) is
//   left out.
// - GXFfxradialBlur.ppo: 16 reads from the pixel toward g_screenCenter, each step
//   (center - pixel) * g_radialDistance / 16; the mean of their squares, square-rooted, tinted by
//   g_color; alpha = the mask's alpha x the vertex alpha (discarded at 0).
// The output is premultiplied (fx.w = 1: additive, alpha 0).

#import bevy_pbr::{
    mesh_functions::get_world_from_local,
    view_transformations::position_world_to_clip,
    mesh_view_bindings::{view, view_transmission_texture, view_transmission_sampler},
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var fx_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var fx_sampler: sampler;

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    // The particle's own 0..1 square (tracers: along, across).
    @location(1) uv: vec2<f32>,
    // The normal map's (scrolled, scaled) or the mask's UV.
    @location(2) tex_uv: vec2<f32>,
    @location(3) color: vec4<f32>,
    // x: 0 distortion, 1 radial blur, 2 tracer distortion; y: strength (g_waveStrength /
    // g_radialDistance); z: g_clampDistanceInverse; w: 1 additive.
    @location(4) fx: vec4<f32>,
    // World centre of the particle (radial blur).
    @location(5) center: vec3<f32>,
};

struct VOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) tex_uv: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) fx: vec4<f32>,
    @location(4) center: vec4<f32>,
};

@vertex
fn vertex(v: Vertex) -> VOut {
    var o: VOut;
    let world = get_world_from_local(v.instance_index) * vec4(v.position, 1.0);
    o.position = position_world_to_clip(world.xyz);
    o.uv = v.uv;
    o.tex_uv = v.tex_uv;
    o.color = v.color;
    o.fx = v.fx;
    o.center = position_world_to_clip(v.center);
    return o;
}

fn screen(c: vec4<f32>) -> vec2<f32> {
    let ndc = c.xy / max(c.w, 1e-5);
    return vec2(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
}

@fragment
fn fragment(in: VOut) -> @location(0) vec4<f32> {
    let p = (in.position.xy - view.viewport.xy) / view.viewport.zw;
    var rgb: vec3<f32>;
    var a = in.color.a;
    if in.fx.x == 1.0 {
        let mask = textureSample(fx_texture, fx_sampler, in.tex_uv).a * in.color.a;
        if mask <= 0.0 {
            discard;
        }
        let step = (screen(in.center) - p) * in.fx.y / 16.0;
        var q = p;
        var sum = vec3(0.0);
        for (var i = 0; i < 16; i++) {
            let s = textureSampleLevel(view_transmission_texture, view_transmission_sampler, q, 0.0).rgb;
            sum += s * s;
            q = clamp(q + step, vec2(0.0), vec2(1.0));
        }
        rgb = sqrt(sum / 16.0) * in.color.rgb;
        a = mask;
    } else {
        var d: f32;
        if in.fx.x == 2.0 {
            d = abs(in.uv.y * 2.0 - 1.0);
        } else {
            let e = abs(in.uv - 0.5) * 2.0;
            d = length(e);
        }
        let fall = max(1.0 - in.fx.z * d, 0.0) * 0.01;
        let n = textureSample(fx_texture, fx_sampler, in.tex_uv).rg * 2.0 - 1.0;
        let q = clamp(p + n * in.fx.y * fall, vec2(0.0), vec2(1.0));
        rgb = textureSampleLevel(view_transmission_texture, view_transmission_sampler, q, 0.0).rgb * in.color.rgb;
    }
    a = clamp(a, 0.0, 1.0);
    return vec4(max(rgb, vec3(0.0)) * a, select(a, 0.0, in.fx.w > 0.5));
}
