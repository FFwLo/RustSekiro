// Original implementation of the recovered weighted biased-log reduction.
struct Settings {
    exposure: vec4<f32>, adaptation: vec4<f32>, bloom: vec4<f32>,
    transfer: vec4<f32>, controls: vec4<f32>, curve: vec4<f32>, reinhard: vec4<f32>,
    colour: mat4x4<f32>,
}
@group(0) @binding(0) var scene: texture_2d<f32>;
@group(0) @binding(1) var<uniform> settings: Settings;
@group(0) @binding(2) var<storage, read_write> history: vec4<f32>;
var<workgroup> sums: array<vec2<f32>, 256>;

@compute @workgroup_size(256)
fn main(@builtin(local_invocation_index) lane: u32) {
    // Capture controls hold the last native gain without resetting adaptation.
    // This condition is uniform across the entire workgroup.
    if settings.controls.w > 0.5 { return; }
    let size = textureDimensions(scene);
    var sum = vec2(0.0);
    // 16 samples per 16x16 reduction cell, for a 64x64 stratified meter.
    let cell = vec2(lane % 16u, lane / 16u);
    for (var i = 0u; i < 16u; i++) {
        let uv = (vec2<f32>(cell * 4u + vec2(i % 4u, i / 4u)) + 0.5) / 64.0;
        let rgb = textureLoad(scene, min(vec2<i32>(uv * vec2<f32>(size)), vec2<i32>(size) - 1), 0).rgb;
        let y = dot(rgb, vec3(0.299, 0.587, 0.114)) * settings.exposure.y;
        var log_y = log2(max(y, 0.0) + 0.00001) + 20.0;
        if !(y >= 0.0) || y > 3.402823e38 { log_y = log2(0.00001) + 20.0; }
        let center = settings.transfer.w * vec2(1.0, f32(size.y) / f32(size.x));
        let r = length(2.0 * (uv - 0.5) * center);
        let w = 1.0 - pow(1.0 - exp2(-r), 3.0);
        sum += vec2(w * log_y, w);
    }
    sums[lane] = sum;
    workgroupBarrier();
    for (var stride = 128u; stride > 0u; stride /= 2u) {
        if lane < stride { sums[lane] += sums[lane + stride]; }
        workgroupBarrier();
    }
    if lane == 0u {
        let current = sums[0].x / max(sums[0].y, 0.00001);
        let desired = current - settings.exposure.z;
        var old = history.x;
        // The native reset rate argument is 4, effectively instant.
        if history.w == 0.0 { old = desired; }
        let argument = select(settings.adaptation.w, settings.adaptation.z, desired > old);
        var adapted = old + (desired - old) * (1.0 - pow(10.0, -argument));
        // Executable 24d920 limits the adapted EV AFTER smoothing, with natural
        // exponential shoulders. The uploaded bounds are not hard clamps.
        // The shipped CPU path reads the previous adapted value one frame late.
        // Retain that latency on GPU, without synchronously reading back to CPU.
        let ev = old - 20.0;
        var offset = 0.0;
        if ev > 0.0 && settings.adaptation.y > 0.0 {
            offset = settings.adaptation.y * (1.0 - exp(-ev / settings.adaptation.y));
        }
        if ev < 0.0 && settings.adaptation.x < 0.0 {
            let magnitude = -settings.adaptation.x;
            offset = magnitude * (exp(ev / magnitude) - 1.0);
        }
        if settings.controls.y > 0.5 || settings.exposure.w < 0.5 { offset = 0.0; }
        let gain = exp2(-offset);
        if settings.exposure.w < 0.5 { adapted = 20.0; }
        history = vec4(adapted, gain, current, 1.0);
    }
}
