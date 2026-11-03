import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput;

struct Settings {
    exposure: vec4<f32>, adaptation: vec4<f32>, bloom: vec4<f32>,
    transfer: vec4<f32>, controls: vec4<f32>, curve: vec4<f32>, reinhard: vec4<f32>,
    colour: mat4x4<f32>,
}
@group(0) @binding(0) var scene: texture_2d<f32>;
@group(0) @binding(1) var level0: texture_2d<f32>;
@group(0) @binding(2) var level1: texture_2d<f32>;
@group(0) @binding(3) var level2: texture_2d<f32>;
@group(0) @binding(4) var linear_sampler: sampler;
@group(0) @binding(5) var<uniform> settings: Settings;
@group(0) @binding(6) var<storage, read> adapted: vec4<f32>;

fn sample_scene(uv: vec2<f32>) -> vec3<f32> {
    return textureSampleLevel(scene, linear_sampler, uv, 0.0).rgb;
}
fn display_to_linear(v: vec3<f32>) -> vec3<f32> {
    return select(pow((v + 0.055) / 1.055, vec3(2.4)), v / 12.92, v <= vec3(0.04045));
}
fn tone(x: vec3<f32>) -> vec3<f32> {
    // Evaluate the native table writer analytically, avoiding table quantization.
    // Its x/(x+0.2) address and 0.2*u/(1-u) reconstruction cancel.
    if settings.transfer.z == 0.0 {
        let v = max(x - 0.004, vec3(0.0));
        return pow(v * (6.2*v + 0.5) / (v*(6.2*v + 1.7) + 0.06), vec3(2.2 / settings.transfer.x));
    }
    if settings.transfer.z == 7.0 {
        // DXBC `exp` is base two, unlike WGSL `exp` and the CPU soft EV limit.
        let a = 1.0 - exp2(-1.442695 * x);
        let b = 1.0 - exp2(-4.456669 * x);
        return pow(max(a + settings.curve.z * (b*b*b - a), vec3(0.0)), vec3(1.0 / settings.transfer.x));
    }
    if settings.transfer.z == 8.0 {
        let z = pow(x * settings.reinhard.x, vec3(settings.curve.w));
        return pow(z / (1.0 + z), vec3(1.0 / settings.transfer.x));
    }
    return pow(x * (1.0 + 0.01*x) / (1.0 + x), vec3(1.0 / settings.transfer.x));
}

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    @if(BRIGHT) {
        let c = sample_scene(in.uv) * settings.exposure.x * adapted.y;
        let y = dot(c, vec3(0.29891, 0.58661, 0.11448));
        var bright = c * max(y - settings.bloom.x, 0.0) / (y + 0.0001);
        if any(bright != bright) { bright = vec3(0.0); }
        let gray = dot(bright, vec3(0.29891, 0.58661, 0.11448));
        return vec4(mix(bright, vec3(gray), settings.controls.x), 1.0);
    }
    @if(!BRIGHT && COPY) {
        return vec4(sample_scene(in.uv), 1.0);
    }
    @if(!BRIGHT && !COPY && UPSAMPLE) {
        return vec4(sample_scene(in.uv)
            + settings.reinhard.z * textureSampleLevel(level0, linear_sampler, in.uv, 0.0).rgb, 1.0);
    }
    @if(!BRIGHT && !COPY && !UPSAMPLE && TONE) {
        var x = max(sample_scene(in.uv) * settings.exposure.x * adapted.y, vec3(0.0));
        if settings.bloom.y > 0.0 {
            let glare = textureSampleLevel(level0, linear_sampler, in.uv, 0.0).rgb;
            x += glare * settings.bloom.y * settings.bloom.w;
        }
        let corrected = (settings.colour * vec4(tone(x), 1.0)).rgb;
        var display = pow(max(corrected, vec3(0.0)), vec3(settings.transfer.y));
        if settings.controls.z < 0.5 {
            let noise = fract(52.9829189 * fract(dot(in.position.xy, vec2(0.06711056, 0.00583715)))) - 0.5;
            display = max(display + noise / 255.0, vec3(0.0));
        }
        // Grading undoes this storage conversion before its lookup.
        return vec4(display_to_linear(display), 1.0);
    }
    @if(!BRIGHT && !COPY && !UPSAMPLE && !TONE) {
        // 2556e0 normalizes exp(-offset^2/(2*sigma^2)), sigma=1.5.
        let weights = array<f32, 7>(0.036633, 0.111281, 0.216745, 0.270682, 0.216745, 0.111281, 0.036633);
        let size = vec2<f32>(textureDimensions(scene));
        @if(BLUR_H) { let axis = vec2(1.0, 0.0); }
        @if(!BLUR_H) { let axis = vec2(0.0, 1.0); }
        var result = vec3(0.0);
        for (var i = 0; i < 7; i++) {
            result += sample_scene(in.uv + axis * f32(i - 3) * settings.bloom.z / size) * weights[i];
        }
        return vec4(result, 1.0);
    }
}
