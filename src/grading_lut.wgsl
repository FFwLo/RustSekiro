// The map's colour-grading LUT (Yebis ColorGrading, grading.rs) on the tonemapped image.
// The LUT is the game's own 16x256 RGBA8 strip: 16 slices of 16x16 stacked top to bottom,
// x = red, row = blue * 16 + green; the display (sRGB) colour in, the graded one out. Red and
// green interpolate inside a slice through the sampler, blue between two slices by hand.
#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput

@group(0) @binding(0) var screen: texture_2d<f32>;
@group(0) @binding(1) var screen_sampler: sampler;
@group(0) @binding(2) var lut: texture_2d<f32>;
@group(0) @binding(3) var lut_sampler: sampler;

fn to_srgb(c: vec3<f32>) -> vec3<f32> {
    let lo = c * 12.92;
    let hi = 1.055 * pow(max(c, vec3(0.0)), vec3(1.0 / 2.4)) - 0.055;
    return select(hi, lo, c <= vec3(0.0031308));
}

fn to_linear(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((c + 0.055) / 1.055, vec3(2.4));
    return select(hi, lo, c <= vec3(0.04045));
}

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let src = textureSample(screen, screen_sampler, in.uv);
    let c = clamp(to_srgb(src.rgb), vec3(0.0), vec3(1.0));
    let b = c.b * 15.0;
    let b0 = floor(b);
    let b1 = min(b0 + 1.0, 15.0);
    let x = (c.r * 15.0 + 0.5) / 16.0;
    let g = c.g * 15.0 + 0.5;
    let s0 = textureSampleLevel(lut, lut_sampler, vec2(x, (b0 * 16.0 + g) / 256.0), 0.0).rgb;
    let s1 = textureSampleLevel(lut, lut_sampler, vec2(x, (b1 * 16.0 + g) / 256.0), 0.0).rgb;
    let graded = mix(s0, s1, b - b0);
    return vec4(to_linear(graded), src.a);
}
