import bevy_ui_render::ui_vertex_output::UiVertexOutput;

@group(1) @binding(0) var<uniform> settings: vec4<f32>;
@group(1) @binding(1) var sprite: texture_2d<f32>;

fn revealed_texel(at: vec2<i32>, size: vec2<i32>) -> vec4<f32> {
    let texel = clamp(at, vec2(0), size - vec2(1));
    var color = textureLoad(sprite, texel, 0);
    if settings.x < 1.0 {
        let radius = length((vec2<f32>(texel) + 0.5) / vec2<f32>(size) - 0.5);
        let reveal = clamp((settings.z + settings.w * settings.x - radius) * settings.y + 0.5, 0.0, 1.0);
        // Match RGBA8 alpha rounding before bilinear filtering, on the GPU.
        color.a = floor(color.a * reveal * 255.0 + 0.5) / 255.0;
    }
    return color;
}

@fragment
fn fragment(in: UiVertexOutput) -> @location(0) vec4<f32> {
    let size = vec2<i32>(textureDimensions(sprite));
    let position = in.uv * vec2<f32>(size) - 0.5;
    let at = vec2<i32>(floor(position));
    let weight = fract(position);
    return mix(
        mix(revealed_texel(at, size), revealed_texel(at + vec2(1, 0), size), weight.x),
        mix(revealed_texel(at + vec2(0, 1), size), revealed_texel(at + vec2(1, 1), size), weight.x),
        weight.y,
    );
}
