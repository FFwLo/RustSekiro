// The game's global fog (height_fog.rs): a constant density plus a bounded height profile,
// lit by the sky colour and the sun-angle direct / opposite colours, marched over the exe's
// geometric depth slices up to the volume's far depth, then continued analytically.
// Adapted from sekiro-rs crates/game/src/fog.wgsl (MIT / Apache-2.0); the slice boundaries,
// GameScale units and height packing follow its docs/SHADER-INPUTS.md (exe 39e7e0, 39fa70,
// 39f000).

#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput
#import bevy_render::view::View

struct GameFog {
    // FixedDensity', kH, sH, height offset (map Y of the arena origin - HeightFogOffset).
    density: vec4<f32>,
    // Near', Far', SkyLight StartDist', DirLight StartDist'.
    depths: vec4<f32>,
    // SkyColor radiance, SkyLight HeightAtten.
    sky: vec4<f32>,
    // DirectColor radiance, DirLight HeightAtten.
    direct: vec4<f32>,
    // OppositeColor radiance, height sign.
    opposite: vec4<f32>,
    // Toward the fog's light, DirLightAngleInvCos.
    direction: vec4<f32>,
    // ExtinctionScale, slice count.
    extinction: vec4<f32>,
    // NearOffset', analytical continuation (1 / 0).
    extra: vec4<f32>,
}
@group(0) @binding(0) var scene: texture_2d<f32>;
#ifdef MULTISAMPLED_DEPTH
@group(0) @binding(1) var depth_texture: texture_depth_multisampled_2d;
#else
@group(0) @binding(1) var depth_texture: texture_depth_2d;
#endif
@group(0) @binding(2) var<uniform> view: View;
@group(0) @binding(3) var<uniform> fog: GameFog;

// The height profile: e^-q above the reference plane, approaching 2 below it.
fn profile(q: f32) -> f32 {
    return select(2.0 - exp(min(q, 0.0)), exp(-max(q, 0.0)), q >= 0.0);
}
// Its primitive in h (the far continuation).
fn primitive(h: f32, k: f32) -> f32 {
    if h >= 0.0 { return -exp(-k * h) / k; }
    return 2.0 * h - exp(k * h) / k;
}
// The vertical optical column above h (light attenuation).
fn column(h: f32, k: f32) -> f32 {
    if k <= 0.000001 { return 0.0; }
    if h > 0.0 { return exp(-k * h) / k; }
    return exp(k * h) / k - 2.0 * h;
}
fn density_at(h: f32) -> f32 {
    return fog.density.x + fog.density.z * profile(fog.density.y * h);
}
fn height(world_y: f32) -> f32 {
    return fog.opposite.w * (world_y + fog.density.w);
}
// Optical depth of a straight segment from height h0 to h1 of the given length.
fn optical_segment(h0: f32, h1: f32, length: f32) -> f32 {
    let k = fog.density.y;
    var height_integral = profile(k * h0) * length;
    if abs(k * (h1 - h0)) >= 0.0001 {
        height_integral = abs((primitive(h1, k) - primitive(h0, k)) / (h1 - h0)) * length;
    }
    return max(0.0, fog.density.x * length + fog.density.z * height_integral);
}
// The light scattered toward the eye in a slice [z0, z1] of view depth at height h; each source
// starts at its StartDist (a fraction of the slice that reaches past it).
fn source_at(h: f32, z0: f32, z1: f32, ray: vec3<f32>) -> vec3<f32> {
    let mu = dot(ray, fog.direction.xyz);
    let cd = (1.0 + mu) * 0.5 * fog.direct.xyz + (1.0 - mu) * 0.5 * fog.opposite.xyz;
    let optical_column = fog.density.z * column(h, fog.density.y);
    let sky_gate = clamp((z1 - fog.depths.z) / max(z1 - z0, 0.000001), 0.0, 1.0);
    let dir_gate = clamp((z1 - fog.depths.w) / max(z1 - z0, 0.000001), 0.0, 1.0);
    var dir_atten = vec3(1.0);
    if fog.density.y > 0.0 {
        dir_atten = exp(-optical_column * fog.direction.w * fog.direct.w * fog.extinction.xyz);
    }
    return density_at(h) * (fog.sky.xyz * exp(-optical_column * fog.sky.w) * sky_gate + cd * dir_atten * dir_gate);
}

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let pixel = vec2<i32>(in.position.xy);
    let surface = textureLoad(scene, pixel, 0);
    var depth = textureLoad(depth_texture, pixel, 0);
#ifdef MULTISAMPLED_DEPTH
    for (var sample = 1u; sample < textureNumSamples(depth_texture); sample += 1u) {
        depth = max(depth, textureLoad(depth_texture, pixel, i32(sample)));
    }
#endif
    // Only a bare background (the probe sky box) is left unfogged: the sky domes write their
    // real depth (sky.rs) and are fogged at it, as the game's sky shader is.
    if depth <= 0.0 {
        return surface;
    }
    let ndc = vec2(in.uv.x * 2.0 - 1.0, 1.0 - in.uv.y * 2.0);
    let reconstructed = view.world_from_clip * vec4(ndc, depth, 1.0);
    let relative = reconstructed.xyz / reconstructed.w - view.world_position;
    let distance = length(relative);
    let ray = relative / max(distance, 0.000001);
    // The slices are in view depth; the segment lengths along the ray.
    let forward = -view.world_from_view[2].xyz;
    let ray_cos = max(dot(ray, forward), 0.0001);
    let end_depth = distance * ray_cos;
    let n = fog.depths.x;
    let f = fog.depths.y;
    let o = fog.extra.x;
    let slices = fog.extinction.w;
    let cap = min(end_depth, f);
    // The source radiance is in scene units before the camera's exposure.
    let radiance = view.exposure;
    var tau = 0.0;
    var light = vec3(0.0);
    var z0 = 0.0;
    // The exe's boundaries: (n+o) * ((f+o)/(n+o))^(k/Z) - o, k = 0..Z, each slice integrated
    // at its midpoint (the solver's deterministic march).
    let ratio = (f + o) / max(n + o, 0.000001);
    for (var k = 0.0; k <= slices; k += 1.0) {
        if z0 >= cap { break; }
        let z1 = min((n + o) * pow(ratio, k / max(slices, 1.0)) - o, cap);
        if z1 <= z0 { continue; }
        let ds = (z1 - z0) / ray_cos;
        let h = height(view.world_position.y + ray.y * (z0 + z1) * 0.5 / ray_cos);
        let dt = density_at(h) * ds;
        light += source_at(h, z0, z1, ray) * ds * exp(-fog.extinction.xyz * (tau + 0.5 * dt));
        tau += dt;
        z0 = z1;
    }
    // Past the volume's far depth: the analytical height integral (EnableAnalyticalFog), its
    // added optical depth scaled by 1 - the volume's end transmittance (the forward shader's).
    if end_depth > f && fog.extra.y > 0.5 {
        let start_length = f / ray_cos;
        let h0 = height(view.world_position.y + ray.y * start_length);
        let h1 = height(view.world_position.y + ray.y * distance);
        let terminal_light = light;
        let delta = optical_segment(h0, h1, distance - start_length) * (1.0 - exp(-tau));
        let e = max(fog.extinction.xyz, vec3(0.000001));
        let integral = select((vec3(1.0) - exp(-e * delta)) / e, vec3(delta), fog.extinction.xyz < vec3(0.000001));
        light += exp(-fog.extinction.xyz * tau) * terminal_light * integral;
        tau += delta;
    }
    return vec4(surface.rgb * exp(-fog.extinction.xyz * tau) + light * radiance, surface.a);
}
