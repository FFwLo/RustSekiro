// The map's colour-grading LUT (Yebis ColorGrading, grading.rs) on the tonemapped image.
// The LUT is the game's own 16x256 RGBA8 strip: 16 slices of 16x16 stacked top to bottom,
// x = red, row = blue * 16 + green; the display (sRGB) colour in, the graded one out. Red and
// green interpolate inside a slice through the sampler, blue between two slices by hand.
import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput;
import bevy_render::view::View;

@group(0) @binding(0) var screen: texture_2d<f32>;
@group(0) @binding(1) var screen_sampler: sampler;
@group(0) @binding(2) var lut: texture_2d<f32>;
@group(0) @binding(3) var lut_sampler: sampler;
@group(0) @binding(4) var<uniform> view: View;
// x: 1 = tone map here (the game's curve; Bevy's tonemapping is off on this view), y: FilmicContrast.
@group(0) @binding(5) var<uniform> tone: vec4<f32>;

// Yebis Sensitometric tone table (mode 7) as a function of the exposed scene value x
// (sekiro-rs docs/SHADER-INPUTS.md): A = 1 - 2^(-1.442695x), B = 1 - 2^(-4.456669x),
// out = A + c (B^3 - A); the output power 1/2.2 follows (AfterGamma). Base two, not e: the
// game's ToneMap_WriteSensitometricToneMappingTablePS.ppo uses the SM4/5 `exp` instruction,
// which is 2^x, with these two constants as its operands (sekiro-rs docs/EXPOSURE.md).
fn sensitometric(x: vec3<f32>, c: f32) -> vec3<f32> {
    let a = vec3(1.0) - exp2(-1.442695 * x);
    let b = vec3(1.0) - exp2(-4.456669 * x);
    return a + c * (b * b * b - a);
}

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
    var c: vec3<f32>;
    if tone.x > 0.5 {
        // Bevy's auto exposure (EV) sits in the view's colour grading, where its tonemapping reads it.
        let x = max(src.rgb, vec3(0.0)) * exp2(view.color_grading.exposure);
        c = pow(clamp(sensitometric(x, tone.y), vec3(0.0), vec3(1.0)), vec3(1.0 / 2.2));
    } else {
        c = clamp(to_srgb(src.rgb), vec3(0.0), vec3(1.0));
    }
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
