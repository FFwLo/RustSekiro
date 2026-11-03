import bevy_pbr::render::mesh_view_bindings::view;


/// Ordered 8x8 dither threshold in (0, 1) for a pixel.
fn coverage_noise(position: vec2<f32>) -> f32 {
    let p = vec2<u32>(position) & vec2(7u);
    let a = ((p.x & 1u) ^ (p.y & 1u)) * 2u + (p.y & 1u);
    let b = (((p.x >> 1u) & 1u) ^ ((p.y >> 1u) & 1u)) * 2u + ((p.y >> 1u) & 1u);
    let c = (((p.x >> 2u) & 1u) ^ ((p.y >> 2u) & 1u)) * 2u + ((p.y >> 2u) & 1u);
    return (f32(a * 16u + b * 4u + c) + 0.5) / 64.0;
}

fn camera_coverage(position: vec2<f32>, world_position: vec3<f32>, fade: vec4<f32>) {
    if fade.x > 0.0 {
        let opacity = distance(world_position, view.world_position) * fade.z - fade.y;
        if opacity < 0.01 + 0.99 * coverage_noise(position) { discard; }
    }
}

/// Detail-level crossfade (`map_lod::crossfade_tags`): the two levels of a part draw
/// complementary dither pixels. Bit 31 marks a fading instance, bit 30 the higher level of the
/// pair, and the low 16 bits the higher level's weight. Untagged instances draw every pixel.
fn lod_coverage(position: vec2<f32>, tag: u32) {
    if (tag & 0x80000000u) != 0u {
        let weight = f32(tag & 0xffffu) / 65535.0;
        let higher = (tag & 0x40000000u) != 0u;
        if (coverage_noise(position) < weight) != higher { discard; }
    }
}
