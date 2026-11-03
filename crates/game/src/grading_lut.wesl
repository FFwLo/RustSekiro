import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput;

@group(0) @binding(0) var source: texture_2d<f32>;
// Unorm, deliberately not sRGB: the stored bytes are display-space RGB values.
@group(0) @binding(1) var lut: texture_2d<f32>;

fn linear_to_display(v: vec3<f32>) -> vec3<f32> {
    // Undo native tone-pass storage encoding. This recovers the ordinary output
    // power result for LUT input; it introduces no extra native gamma stage.
    return select(1.055 * pow(max(v, vec3(0.0)), vec3(1.0 / 2.4)) - 0.055,
                  v * 12.92, v <= vec3(0.0031308));
}
fn display_to_linear(v: vec3<f32>) -> vec3<f32> {
    return select(pow((v + 0.055) / 1.055, vec3(2.4)),
                  v / 12.92, v <= vec3(0.04045));
}
fn cell(p: vec3<i32>) -> vec3<f32> {
    // Red changes fastest; green changes within a slice; blue selects the slice.
    return textureLoad(lut, vec2(p.x, p.z * 16 + p.y), 0).rgb;
}
fn lookup(rgb: vec3<f32>) -> vec3<f32> {
    let p = clamp(rgb, vec3(0.0), vec3(1.0)) * 15.0;
    let lo = vec3<i32>(floor(p));
    let hi = min(lo + vec3(1), vec3(15));
    let f = fract(p);
    let a = mix(cell(lo), cell(vec3(hi.x, lo.y, lo.z)), f.x);
    let b = mix(cell(vec3(lo.x, hi.y, lo.z)), cell(vec3(hi.x, hi.y, lo.z)), f.x);
    let c = mix(cell(vec3(lo.x, lo.y, hi.z)), cell(vec3(hi.x, lo.y, hi.z)), f.x);
    let d = mix(cell(vec3(lo.x, hi.y, hi.z)), cell(hi), f.x);
    return mix(mix(a, b, f.y), mix(c, d, f.y), f.z);
}
@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let rgba = textureLoad(source, vec2<i32>(in.position.xy), 0);
    var display = lookup(linear_to_display(rgba.rgb));
    // Sub-LSB display-space dither avoids quantization bands after a contrast LUT.
    let noise = fract(52.9829189 * fract(dot(in.position.xy, vec2(0.06711056, 0.00583715)))) - 0.5;
    display = clamp(display + noise / 255.0, vec3(0.0), vec3(1.0));
    return vec4(display_to_linear(display), rgba.a);
}
