import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput;
import bevy_render::view::View;

struct Fog {
    density: vec4<f32>, depths: vec4<f32>, sky: vec4<f32>,
    direct: vec4<f32>, opposite: vec4<f32>, direction: vec4<f32>, extinction: vec4<f32>,
    slices: vec4<f32>,
    background_control: vec4<f32>,
    background: array<vec4<f32>,16>,
    radiance_scale: vec4<f32>,
}
@if(MULTISAMPLED_DEPTH) {
    @group(0) @binding(0) var depth_texture: texture_depth_multisampled_2d;
    @group(0) @binding(1) var<uniform> view: View;
    @group(0) @binding(2) var<uniform> fog: Fog;
}
@if(!MULTISAMPLED_DEPTH) {
    @group(0) @binding(0) var scene: texture_2d<f32>;
    @group(0) @binding(1) var depth_texture: texture_depth_2d;
    @group(0) @binding(2) var<uniform> view: View;
    @group(0) @binding(3) var<uniform> fog: Fog;
}

// Intersect the live authored dome's polygon-ring profile, in camera-centred world space.
// Native SkyDome forward permutations use this finite geometry depth for fog continuation.
fn background_distance(ray: vec3<f32>, ray_cos: f32) -> f32 {
    let count = u32(fog.background_control.x);
    if count < 2u { return fog.depths.y/ray_cos; }
    let angle = atan2(ray.z,ray.x);
    let step = fog.background_control.y;
    let sector = floor((angle-fog.background_control.z)/step);
    let normal_angle = fog.background_control.z+(sector+0.5)*step;
    let radial = length(ray.xz)*cos(angle-normal_angle);
    var result = 1.0e30;
    var found = false;
    for (var i=1u; i<count; i+=1u) {
        let a = fog.background[i-1u].xy;
        let b = fog.background[i].xy;
        let delta = b-a;
        let denominator = radial*delta.y-ray.y*delta.x;
        if abs(denominator) < 0.000001 { continue; }
        let distance = (a.x*delta.y-a.y*delta.x)/denominator;
        if distance <= 0.0 { continue; }
        let point = vec2(radial,ray.y)*distance;
        let along = dot(point-a,delta)/dot(delta,delta);
        if along >= -0.00001 && along <= 1.00001 {
            result = min(result,distance);
            found = true;
        }
    }
    // Clear pixels outside an exported dome have no finite surface endpoint.
    // Use the terminal authored volume there; do not invent a continuation distance.
    return select(fog.depths.y/ray_cos,result,found);
}

fn profile(q: f32) -> f32 {
    return select(2.0 - exp(min(q, 0.0)), exp(-max(q, 0.0)), q >= 0.0);
}
fn primitive(h: f32, k: f32) -> f32 {
    if h >= 0.0 { return -exp(-k*h)/k; }
    return 2.0*h - exp(k*h)/k;
}
fn column(h: f32, k: f32) -> f32 {
    if h > 0.0 { return exp(-k*h)/k; }
    return exp(k*h)/k - 2.0*h;
}
fn optical_segment(h0: f32, h1: f32, length: f32) -> f32 {
    let k = fog.density.y;
    var height_integral = profile(k*h0)*length;
    if abs(k*(h1-h0)) >= 0.0001 {
        height_integral = abs((primitive(h1,k)-primitive(h0,k))/(h1-h0))*length;
    }
    return max(0.0, fog.density.x*length + fog.density.z*height_integral);
}
fn source_at(h: f32, z0: f32, z1: f32, ray: vec3<f32>) -> vec3<f32> {
    let rho = fog.density.x + fog.density.z*profile(fog.density.y*h);
    let angular = clamp(0.5 + 0.5*dot(ray,fog.direction.xyz),0.0,1.0);
    let cd = mix(fog.opposite.xyz,fog.direct.xyz,angular);
    let optical_column = fog.density.z*column(h,fog.density.y);
    let sky_gate = clamp((z1-fog.depths.z)/(z1-z0),0.0,1.0);
    let dir_gate = clamp((z1-fog.depths.w)/(z1-z0),0.0,1.0);
    return rho*(fog.sky.xyz*exp(-optical_column*fog.sky.w)*sky_gate
        + cd*exp(-optical_column*fog.direction.w*fog.direct.w*fog.extinction.xyz)*dir_gate)*fog.radiance_scale.x;
}
fn terminal_source(h: f32, ray: vec3<f32>) -> vec3<f32> {
    let angular = clamp(0.5 + 0.5*dot(ray,fog.direction.xyz),0.0,1.0);
    let cd = mix(fog.opposite.xyz,fog.direct.xyz,angular);
    let optical_column = fog.density.z*column(h,fog.density.y);
    // SolveVolumetricFog's final padding slice stores source colour, not integrated light.
    return (fog.sky.xyz*exp(-optical_column*fog.sky.w)
        + cd*exp(-optical_column*fog.direction.w*fog.direct.w*fog.extinction.xyz))*fog.radiance_scale.x;
}

@fragment
fn fragment(in: FullscreenVertexOutput,
@if(MULTISAMPLED_DEPTH)
    @builtin(sample_index) sample: u32,
) -> @location(0) vec4<f32> {
    let pixel = vec2<i32>(in.position.xy);
@if(MULTISAMPLED_DEPTH)
    let depth = textureLoad(depth_texture,pixel,i32(sample));
    @if(!MULTISAMPLED_DEPTH) {
        let surface = textureLoad(scene,pixel,0);
        let depth = textureLoad(depth_texture,pixel,0);
    }
    let ndc = vec2(in.uv.x*2.0-1.0,1.0-in.uv.y*2.0);
    let reconstructed = view.world_from_clip*vec4(ndc,max(depth,0.000001),1.0);
    let relative = reconstructed.xyz/reconstructed.w - view.world_position;
    let ray = normalize(relative);
    // Native volume integration uses view-depth thickness; height samples use world space.
    let forward = -view.world_from_view[2].xyz;
    let ray_cos = max(dot(ray,forward),0.0001);
    var distance = length(relative);
    if depth <= 0.0 { distance = background_distance(ray,ray_cos); }
    let end_depth = distance*ray_cos;
    let cap = min(end_depth,fog.depths.y);
    let near = min(fog.depths.x,cap);
    // Verified 39fa70 geometric grid, independent of the surface depth.
    let offset = fog.slices.x;
    let base = max(fog.depths.x+offset, 0.000001);
    let ratio = (fog.depths.y+offset)/base;
    let count = u32(fog.slices.y);
    var tau = 0.0;
    var light = vec3(0.0);
    var z0 = near;
    // Inferred ray integration replacement for the native padded volume texture.
    for (var i = 0u; i < count; i += 1u) {
        let boundary = base*pow(ratio,f32(i+1u)/f32(count))-offset;
        let z1 = min(cap,boundary);
        if z1 <= z0 { break; }
        let ds = z1-z0;
        let sample_depth = clamp(base*pow(ratio,(f32(i)+fog.slices.z)/f32(count))-offset,z0,z1);
        let h = fog.extinction.w*(view.world_position.y + ray.y*sample_depth/ray_cos) + fog.density.w;
        let rho = fog.density.x + fog.density.z*profile(fog.density.y*h);
        let dt = rho*ds;
        light += source_at(h,z0,z1,ray)*ds*exp(-fog.extinction.xyz*(tau+0.5*dt));
        tau += dt;
        z0 = z1;
    }
    if end_depth > fog.depths.y && fog.slices.w > 0.5 {
        let start_length = fog.depths.y/ray_cos;
        let h0 = fog.extinction.w*(view.world_position.y + ray.y*start_length) + fog.density.w;
        let h1 = fog.extinction.w*(view.world_position.y + ray.y*distance) + fog.density.w;
        let terminal_light = terminal_source(h0,ray);
        // The analytical source padding slice has alpha zero, so (1 - padding alpha) is one.
        let delta = optical_segment(h0,h1,distance-start_length);
        // Zero extinction channels use the continuous delta limit, not a division by zero.
        let e = max(fog.extinction.xyz,vec3(0.000001));
        let integral = select((vec3(1.0)-exp(-e*delta))/e,vec3(delta),fog.extinction.xyz < vec3(0.000001));
        light += exp(-fog.extinction.xyz*tau)*terminal_light*integral;
        tau += delta;
    }
    let transmission = exp(-fog.extinction.xyz*tau);
    // Multiply the existing sample colour using destination * source blending.
@if(MULTISAMPLED_DEPTH && FOG_ATTENUATION)
    return vec4(transmission,1.0);
    // Add inscattering to that same sample before the render attachment resolves.
@if(MULTISAMPLED_DEPTH && !FOG_ATTENUATION)
    return vec4(light,0.0);
@if(!MULTISAMPLED_DEPTH)
    return vec4(surface.rgb*transmission+light,surface.a);
}
